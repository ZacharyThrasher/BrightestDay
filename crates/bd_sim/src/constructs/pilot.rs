//! **Piloted constructs, power suits and transformers** (COMBAT.md §3d, §5d, §5d-bis) — held
//! engineer.
//!
//! * [`step_suit`] — [`super::Controller::Suit`]: a human-scale suit wrapping its pilot. The pilot
//!   keeps their own movement (×0.85, applied by the world; bhop / tethers still work), the suit
//!   replaces LMB / RMB with its mechanics (repulsor / ion-blaster bolts for Shoots / Beam / payload,
//!   fusion cannon for Explodes, punch otherwise; RMB = blade swing for gunner suits, block for Guard,
//!   missile volley for Explodes brawlers, repulsor shove otherwise) and absorbs hits with its
//!   integrity first (world). Flies / Thrust suits fly on held Jump ([`suit_movement_mods`]).
//! * [`step_piloted`] — [`super::Controller::Piloted`]: a seated pilot drives the construct's own
//!   locomotion (walk / fly / roll by form, features and Transforms mode), the mouse aims its head /
//!   turret, LMB = punch / payload volley / breath, RMB = stomp (or Charge dash), Space = jump /
//!   climb, **E** = eject (the world's big upward launch). `HeldState::third_person` tells the client
//!   to pull the camera back.
//! * **Flight** ([`locomotion`], COMBAT.md §5d-ter): whatever flies never rolls (a Vehicle-form jet /
//!   helicopter / Starscream's jet mode flies). Jets ([`jet_like`]) fly where the mouse aims with a
//!   minimum airspeed and bank; hoverers (rotors, UFOs, flying beasts) hover; terrain is resolved axis
//!   by axis (slide + small bounce, never tunnel). Robot modes of flying transformers walk and
//!   hover-boost on held Jump. Ground vehicles stand on their body's sole (`integrate`).
//! * **Boarding** ([`boardable`], [`on_board`], [`on_leave`]): the owner boards their own construct
//!   (the world's `board` / `board_nearby` / `board_command`: the Transform key on foot, "get in");
//!   hopping out hands it to its own AI as a follower.
//! * **Weak points** ([`weak_mult`]): joints, cockpit and nape of piloted giants / kaiju take ×3 from
//!   Blade / Pierce. Shattered → pilot ejected tumbling (store `reap`).
//! * **Transformers** (`Mechanic::Transforms`): [`toggle`] (the world's `toggle_transform` command —
//!   client binding Mouse3) switches robot ⇄ alt (vehicle) mode with a ~0.9 s staggered sequence
//!   (`AnimState::Telegraph` + `HeldFx::Transform`). Suit vehicle mode: drive at up to
//!   [`VEHICLE_SPEED`] m/s (below skilled bhop / swing chains), ramps launch (movement's ramp
//!   launch), ram = Charge contact hits, no weapons, third-person camera. Robot mode: movement ×0.9 +
//!   the suit's weapons. Transforming vehicle → robot mid-air arms a **landing slam** (stomp scaled by
//!   the fall speed). Big piloted transformers switch between rolling and walking the same way.

use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Feature, Form, Mechanic};
use bd_core::voxel::{VoxelQuery, world_to_voxel};
use bd_core::{Quat, Vec3};

use super::held::{HeldState, fx, mech_mult, slam};
use super::{AnimState, Construct, ConstructOut, Driver, Env, Loco, PlayerView, Proxy, form_spec, player_key};
use crate::combat::{CarveRequest, HitInfo, HitKind};
use crate::events::{GameEvent, HeldFx};
use crate::geom;
use crate::movement::{MoveModifiers, PlayerState};
use crate::projectile::{ProjectileKind, ProjectileSpawn};
use crate::status::StatusKind;
use crate::weapons::GunArchetype;

/// Transform sequence length (s).
pub const TRANSFORM_SECS: f32 = 0.9;
/// Suit vehicle-mode ground speed cap (m/s).
pub const VEHICLE_SPEED: f32 = 24.0;
/// Robot-mode movement multiplier for transforming suits.
pub const ROBOT_SPEED: f32 = 0.9;
/// The world's suit movement factor (world/mod.rs) we compensate for in vehicle / robot mode.
const WORLD_SUIT_FACTOR: f32 = 0.85;
/// Weak-point multiplier for Blade / Pierce.
pub const WEAK_MULT: f32 = 3.0;

fn has_feature(c: &Construct, f: Feature) -> bool {
    c.recipe.features.contains(&f)
}

fn flies(c: &Construct) -> bool {
    c.has(Mechanic::Flies)
        || matches!(c.recipe.form, Form::Bird | Form::Insect)
        || has_feature(c, Feature::Rotor)
        || has_feature(c, Feature::Propeller)
        || has_feature(c, Feature::Jet)
}

fn shooter(c: &Construct) -> bool {
    c.has(Mechanic::Shoots)
        || c.has(Mechanic::Beam)
        || c.recipe.payload.is_some()
        || has_feature(c, Feature::Turret)
        || has_feature(c, Feature::Barrels)
}

fn starts_as_vehicle(c: &Construct) -> bool {
    c.has(Mechanic::Transforms)
        && (c.recipe.form == Form::Vehicle || has_feature(c, Feature::Wheels) || has_feature(c, Feature::Treads))
}

/// A ground vehicle body (wheels / treads / the Vehicle form).
fn wheeled(c: &Construct) -> bool {
    c.recipe.form == Form::Vehicle || has_feature(c, Feature::Wheels) || has_feature(c, Feature::Treads)
}

/// How a seated pilot moves `c` this tick: `(rolling, flying, jump_boost)`.
/// * Anything that flies (Flies / Thrust, a Jet / Rotor / Propeller, a bird) FLIES — it never rolls,
///   even when its form is Vehicle (fighter jet, helicopter, Starscream's jet mode).
/// * Transformers: the alt (vehicle) mode flies if the construct flies, else rolls (or walks, for a
///   beast mode); the robot mode walks, with a hover / boost on held Jump when it flies or thrusts.
/// * Otherwise wheels / treads / the Vehicle form roll; everything else walks.
pub fn locomotion(c: &Construct, s: &HeldState) -> (bool, bool, bool) {
    let can_fly = flies(c) || c.has(Mechanic::Thrust);
    if c.has(Mechanic::Transforms) {
        if s.vehicle {
            return (!can_fly && wheeled(c), can_fly, false);
        }
        return (false, false, can_fly);
    }
    (!can_fly && wheeled(c), can_fly, false)
}

/// Flies like a jet (keeps a minimum speed, banks) rather than hovering: a Jet feature or Thrust,
/// without a rotor, and not a flying beast.
pub fn jet_like(c: &Construct) -> bool {
    (has_feature(c, Feature::Jet) || c.has(Mechanic::Thrust))
        && !has_feature(c, Feature::Rotor)
        && c.recipe.form.category() != bd_core::mechanics::FormCategory::Creature
}

/// Top flight speed (m/s): jets fast, hoverers slower; scales gently with size, Fast / Heavy.
pub fn flight_speed(c: &Construct, jet: bool) -> f32 {
    let scale = c.recipe.scale.max(0.5);
    let mut s = if jet { 34.0 } else { 18.0 } * (scale / 6.0).powf(0.1);
    if c.has(Mechanic::Fast) {
        s *= 1.3;
    }
    if c.has(Mechanic::Heavy) {
        s *= 0.8;
    }
    s.clamp(8.0, if jet { 46.0 } else { 30.0 })
}

/// Jet minimum airspeed as a fraction of the top speed (below it only near the ground, braking).
pub const JET_MIN_FRAC: f32 = 0.35;
/// Within this height of the ground a braking jet may hover-taxi / land (m).
pub const JET_TAXI_HEIGHT: f32 = 4.0;

fn signed_angle_y(from: f32, to: f32) -> f32 {
    let mut d = to - from;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

/// Jet flight: the velocity turns toward the aim (turn-rate limited; Space / Ctrl tilt it up / down),
/// W throttles to top speed, S brakes to the minimum (to a hover near the ground), nothing = cruise.
/// The hull faces the velocity and banks into turns.
fn fly_jet(c: &mut Construct, inp: &PlayerInput, look: Vec3, top: f32, scale: f32, env: &Env<'_>) {
    let dt = env.dt();
    let speed = c.vel.length();
    let fwd = c.forward();
    let dir = if speed > 0.5 { c.vel / speed } else { fwd };
    let mut want = look.normalize_or(fwd);
    if inp.held(buttons::JUMP) {
        want += Vec3::Y * 0.7;
    } else if inp.held(buttons::CROUCH) {
        want -= Vec3::Y * 0.7;
    }
    let want = want.normalize_or(dir);
    let rate = 2.4 / (scale / 6.0).max(1.0).powf(0.3);
    let ang = dir.dot(want).clamp(-1.0, 1.0).acos();
    let new_dir = if ang <= rate * dt || ang < 1e-4 {
        want
    } else {
        let axis = dir.cross(want).normalize_or(Vec3::Y);
        (Quat::from_axis_angle(axis, rate * dt) * dir).normalize_or(want)
    };
    let near_ground = ground_at(env.ctx.voxels, c.pos, 0.5, JET_TAXI_HEIGHT).is_some();
    let min = top * JET_MIN_FRAC;
    let brake = inp.move_z < 0;
    let floor = if brake && near_ground { 0.0 } else { min };
    let target = match inp.move_z.signum() {
        1 => top,
        -1 => floor,
        _ => speed.clamp(min, top * 0.7).max(floor),
    };
    let accel = top * 0.8;
    let speed = (speed + (target - speed).clamp(-accel * dt, accel * dt)).max(floor);
    c.vel = new_dir * speed;
    // Face the flight path (or the aim while hovering), banked into the turn.
    let face = if speed > 1.0 { new_dir } else { Vec3::new(look.x, 0.0, look.z).normalize_or(fwd) };
    let old_yaw = (-fwd.x).atan2(-fwd.z);
    let new_yaw = (-face.x).atan2(-face.z);
    let yaw_rate = signed_angle_y(old_yaw, new_yaw) / dt.max(1e-4);
    let bank = (yaw_rate * 0.35).clamp(-0.9, 0.9);
    let pitch = face.y.clamp(-1.0, 1.0).asin();
    c.rot = Quat::from_rotation_y(new_yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(bank);
}

/// Hover flight (helicopter, UFO, flying beast): WASD relative to the aim (its pitch included), Space
/// climbs, Ctrl descends; the hull yaws after the camera and tilts into the motion.
#[allow(clippy::too_many_arguments)]
fn fly_hover(c: &mut Construct, inp: &PlayerInput, look: Vec3, cam_right: Vec3, cur_yaw: f32, top: f32, turn: f32, env: &Env<'_>) {
    let dt = env.dt();
    let wish = (look * inp.move_z.clamp(-1, 1) as f32 + cam_right * inp.move_x.clamp(-1, 1) as f32).normalize_or_zero();
    let mut want = wish * top;
    if inp.held(buttons::JUMP) {
        want.y = top * 0.6;
    } else if inp.held(buttons::CROUCH) {
        want.y = -top * 0.6;
    }
    // Horizontal glides; vertical settles quickly (a hover holds its altitude when you let go).
    let (bh, bv) = ((3.0 * dt).min(1.0), (6.0 * dt).min(1.0));
    c.vel.x += (want.x - c.vel.x) * bh;
    c.vel.z += (want.z - c.vel.z) * bh;
    c.vel.y += (want.y - c.vel.y) * bv;
    let yaw = cur_yaw + signed_angle_y(cur_yaw, inp.yaw).clamp(-turn * dt, turn * dt);
    let yq = Quat::from_rotation_y(yaw);
    let local = yq.inverse() * c.vel;
    let tilt_fwd = (local.z / top.max(1.0) * 0.3).clamp(-0.3, 0.3);
    let tilt_side = (-local.x / top.max(1.0) * 0.3).clamp(-0.3, 0.3);
    c.rot = yq * Quat::from_rotation_x(tilt_fwd) * Quat::from_rotation_z(tilt_side);
}

/// Is any sample of the body (centre, nose, tail, belly, top) inside terrain with the pivot at `pos`?
fn body_blocked<V: VoxelQuery + ?Sized>(c: &Construct, v: &V, pos: Vec3) -> bool {
    let center = pos + c.rot * c.local_center;
    let f = c.rot * Vec3::NEG_Z * (c.half.z * 0.8);
    let up = Vec3::Y * (c.half.y * 0.8);
    [center, center + f, center - f, center - up, center + up].iter().any(|p| v.is_solid(world_to_voxel(*p)))
}

/// Flight integration: axis by axis, so a flier slides along terrain and bounces a little off it
/// instead of stopping dead or tunnelling; a body that starts inside terrain is lifted out.
fn fly_integrate(c: &mut Construct, env: &Env<'_>) {
    let dt = env.dt();
    let v = env.ctx.voxels;
    if !c.vel.is_finite() {
        c.vel = Vec3::ZERO;
    }
    let mut lift = 0;
    while lift < 16 && body_blocked(c, v, c.pos) {
        c.pos.y += 0.25;
        lift += 1;
    }
    let step = c.vel * dt;
    for axis in [Vec3::X, Vec3::Z, Vec3::Y] {
        let d = axis * step.dot(axis);
        if d == Vec3::ZERO {
            continue;
        }
        let next = c.pos + d;
        if body_blocked(c, v, next) {
            let along = c.vel.dot(axis);
            c.vel -= axis * along * 1.2; // stop + a small bounce
        } else {
            c.pos = next;
        }
    }
    c.grounded = false;
}

/// Can a pilot board `c` (walk up + Transform key, "get in" / "ride it")? Anything you'd drive or
/// ride: a vehicle, a transformer, a cockpit (mech / jet), whatever already says Piloted, and
/// creatures big enough to sit on. Never held / worn things, structures (unless Piloted: a surfboard),
/// swarm units, attached, forming-less terrain lumps or dissolving constructs.
pub fn boardable(c: &Construct) -> bool {
    use bd_core::mechanics::FormCategory as C;
    if c.gone.is_some() || c.attached() || c.model.is_some() || c.recipe.form == Form::Blob0 {
        return false;
    }
    match c.category {
        C::Held | C::Worn => false,
        C::Structure => c.has(Mechanic::Piloted),
        C::Creature => c.recipe.scale >= 1.2 && !c.has(Mechanic::Swarm),
        C::Object => c.recipe.form == Form::Vehicle || c.has(Mechanic::Piloted) || c.has(Mechanic::Transforms) || has_feature(c, Feature::Cockpit),
    }
}

/// The pilot hopped out of `c` (E / tumble eject): it is now the owner's FOLLOWER, run by its own AI
/// (creature AI, a vehicle hunting enemies, a transformer's mode machine). A transformer keeps the mode
/// it was left in for a moment before its AI decides (no instant flip under the pilot's feet).
pub fn on_leave(c: &mut Construct) {
    c.comp.xform.since = 0.0;
    c.held.transform_req = false;
    c.held.dash_t = 0.0;
}

/// A pilot just took the seat of `c` (spawned seated, or boarded later): it is no longer a thrown
/// object waiting for its launch, and a transformer keeps the mode it is in (robot or vehicle).
pub fn on_board(c: &mut Construct) {
    c.comp.launched = true;
    let transforms = c.has(Mechanic::Transforms);
    let robot = transforms && super::components::boarded_mode_is_robot(c);
    let vehicle = if transforms { !robot } else { starts_as_vehicle(c) };
    let s = &mut c.held;
    s.mode_init = true;
    s.vehicle = vehicle;
    s.transform_req = false;
    s.transform_t = 0.0;
    s.slam_armed = false;
    s.dash_t = 0.0;
    if transforms {
        super::components::mirror_pilot_mode(c, robot);
    }
}

fn tick_timers(s: &mut HeldState, dt: f32) {
    s.cd1 = (s.cd1 - dt).max(0.0);
    s.cd2 = (s.cd2 - dt).max(0.0);
    s.ram_cd = (s.ram_cd - dt).max(0.0);
    s.transform_t = (s.transform_t - dt).max(0.0);
    s.dash_t = (s.dash_t - dt).max(0.0);
}

/// Toggle robot ⇄ alt mode (transformers). `airborne` = the body is in the air (arms a slam).
pub fn toggle(c: &mut Construct, s: &mut HeldState, airborne: bool, fall: f32, env: &Env<'_>, out: &mut ConstructOut) {
    if !c.has(Mechanic::Transforms) || s.transform_t > 0.0 {
        return;
    }
    let to_robot = s.vehicle;
    s.vehicle = !s.vehicle;
    s.transform_t = TRANSFORM_SECS;
    super::components::mirror_pilot_mode(c, to_robot); // an ejected follower keeps this mode
    s.slam_armed = to_robot && airborne;
    s.fall_speed = fall.max(0.0);
    c.set_anim(AnimState::Telegraph, env.tick());
    fx(out, c, HeldFx::Transform, c.center(), c.forward(), if to_robot { 1.0 } else { -1.0 });
}

// ------------------------------------------------------------------------------------ suit

/// [`super::StepFn`] for [`super::Controller::Suit`].
pub fn step_suit(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    c.body.claim(Driver::Attached);
    c.body.loco = Loco::Kinematic;
    let pilot = c.pilot.and_then(|p| env.ctx.player(p)).copied().unwrap_or(env.owner);
    let dt = env.dt();
    let mut s = c.held;
    if !s.mode_init {
        s.mode_init = true;
        s.vehicle = starts_as_vehicle(c);
    }
    tick_timers(&mut s, dt);
    let prev = c.pos;
    c.pos = pilot.pos;
    let hv = Vec3::new(pilot.vel.x, 0.0, pilot.vel.z);
    c.rot = if s.vehicle && hv.length_squared() > 4.0 { geom::yaw_rotation(hv) } else { Quat::from_rotation_y(pilot.yaw) };
    c.vel = (c.pos - prev) / dt.max(1e-6);
    s.third_person = s.vehicle;
    s.guarding = false;
    if !c.active() || !pilot.alive {
        c.held = s;
        return;
    }
    // SMALL TRANSFORMERS (FEEL pass): you ARE the transformer. The suit fights with its archetype
    // like a piloted one (strings, heavies, the blaster, transform attacks, voice orders); its
    // body is YOUR body, so whatever the moveset does to the robot's velocity is handed to you.
    if c.has(Mechanic::Transforms) {
        if suit_transformer(c, &mut s, &pilot, env, out) {
            c.held = s;
            return;
        }
    }
    if s.transform_req {
        s.transform_req = false;
        toggle(c, &mut s, !pilot.grounded, -pilot.vel.y, env, out);
    }
    if s.slam_armed {
        s.fall_speed = s.fall_speed.max(-pilot.vel.y);
        if pilot.grounded {
            s.slam_armed = false;
            slam(c, env, out, pilot.pos, s.fall_speed, HeldFx::TransformSlam);
        }
    }
    if s.transform_t > 0.0 {
        c.set_anim(AnimState::Telegraph, env.tick());
    } else if s.vehicle {
        ram(c, &mut s, &pilot, env, out);
        c.set_anim(AnimState::Move, env.tick());
    } else if c.has(Mechanic::Transforms) {
        suit_brawl(c, &mut s, &pilot, env, out);
    } else {
        suit_attacks(c, &mut s, &pilot, env, out);
    }
    c.held = s;
}

/// Hand the moveset's velocity change to the suit's wearer (the suit has no body of its own).
fn push_wearer(c: &Construct, pilot: &PlayerView, out: &mut ConstructOut) {
    let dv = c.vel - pilot.vel;
    if dv.is_finite() && dv.length_squared() > 0.25 {
        out.impulses.push((pilot.id, dv.clamp_length_max(60.0), 0.0));
    }
}

/// A transformer suit's transform attack / voice-ordered burst. Returns true when it ran this tick
/// (the caller skips the rest of the suit step).
fn suit_transformer(c: &mut Construct, s: &mut HeldState, pilot: &PlayerView, env: &Env<'_>, out: &mut ConstructOut) -> bool {
    use super::brawl;
    let t = &env.ctx.tuning.transformer;
    // Voice orders that need the other body first (vehicle mode): a burst straight from the
    // vehicle, a boost, or a fast snap to robot for the rest.
    if s.transform_t <= 0.0 && c.brawl.xatk == brawl::XPhase::None && s.vehicle && c.brawl.order != brawl::Order::None {
        c.held = *s;
        vehicle_order(c, env, out, pilot.look, pilot.vel);
        *s = c.held;
    }
    let start = s.transform_req && s.transform_t <= 0.0 && ((!s.vehicle && brawl::wants_xattack(c)) || (s.vehicle && brawl::wants_burst_out(c, t)));
    if !start && c.brawl.xatk == brawl::XPhase::None {
        return false;
    }
    let again = s.transform_req && !start;
    s.transform_req = false;
    c.held = *s;
    c.vel = pilot.vel;
    c.grounded = pilot.grounded;
    if start {
        if c.held.vehicle {
            brawl::burst_out(c, env, out, pilot.look);
        } else {
            brawl::start_xattack(c, env, out, pilot.look);
        }
    } else {
        let bi = brawl::Input::of_pilot(&pilot.input, pilot.pressed, pilot.look, false);
        brawl::step_xattack(c, env, out, again, pilot.grounded, &bi);
    }
    push_wearer(c, pilot, out);
    c.set_anim(if c.held.vehicle { AnimState::Move } else { AnimState::Active }, env.tick());
    *s = c.held;
    true
}

/// A transformer suit in robot mode: the archetype's moveset on your own body.
fn suit_brawl(c: &mut Construct, s: &mut HeldState, pilot: &PlayerView, env: &Env<'_>, out: &mut ConstructOut) {
    use super::brawl;
    c.held = *s;
    let mut bi = brawl::Input::of_pilot(&pilot.input, pilot.pressed, pilot.look, false);
    // Your own dodge is the suit's boost-step (movement handles it; a dodged hit opens the
    // counter: `World::apply_hit` → `brawl::open_counter`).
    bi.dodge = false;
    if pilot.busy {
        bi = brawl::Input { aim: bi.aim, wish: bi.wish, ..brawl::Input::default() };
    }
    let order_x = brawl::run_order(c, env, &mut bi);
    c.vel = pilot.vel;
    c.grounded = pilot.grounded;
    brawl::robot(c, env, out, &bi, pilot.grounded);
    brawl::land(c, env, out, pilot.grounded);
    push_wearer(c, pilot, out);
    *s = c.held;
    if order_x {
        s.transform_req = true;
    }
    let moving = Vec3::new(pilot.vel.x, 0.0, pilot.vel.z).length_squared() > 1.0;
    if c.anim_state != AnimState::Active {
        c.set_anim(if moving { AnimState::Move } else { AnimState::Idle }, env.tick());
    }
}

/// A voice order said while in VEHICLE mode (not a beast): "... super" bursts straight out of the
/// vehicle into the transform attack, "... dash" is a boost, anything else snaps to robot first
/// (a fast combat transform) and carries on there.
fn vehicle_order(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, look: Vec3, vel: Vec3) {
    use super::brawl::{self, Order};
    match c.brawl.order {
        Order::Super => {
            c.brawl.order = Order::None;
            brawl::start_xattack(c, env, out, look);
        }
        Order::Dash => {
            c.brawl.order = Order::None;
            let f = Vec3::new(c.forward().x, 0.0, c.forward().z).normalize_or(Vec3::NEG_Z);
            let a = env.ctx.tuning.transformer.robot_row(c);
            let burst = a.drift_boost.max(8.0);
            c.vel = vel + f * burst;
            fx(out, c, HeldFx::Thrust, c.center() - f * c.half.z, -f, 1.5);
        }
        Order::None => {}
        _ => brawl::snap_robot(c, env, out),
    }
}

/// Vehicle mode: running into enemies fast rams them (Charge).
fn ram(c: &mut Construct, s: &mut HeldState, pilot: &PlayerView, env: &Env<'_>, out: &mut ConstructOut) {
    let v = pilot.vel;
    let speed = Vec3::new(v.x, 0.0, v.z).length();
    if s.ram_cd <= 0.0 {
        c.hits.clear();
    }
    if speed < 12.0 {
        return;
    }
    let body = geom::inflate(&pilot.hull, 0.45);
    let (dm, km) = mech_mult(c);
    for t in env.ctx.enemies_of(c.owner) {
        if t.id == pilot.id || !t.hull.intersects(&body) || !c.hits.insert(player_key(t.id)) {
            continue;
        }
        let dir = v.normalize_or_zero() + Vec3::Y * 0.4;
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::Melee, (6.0 + speed * 0.5) * dm, (8.0 + speed * 0.6) * km, dir, t.hull.center())
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(1.5),
        );
        fx(out, c, HeldFx::PilotAttack, t.hull.center(), dir, speed / 12.0);
        s.ram_cd = 0.5;
    }
}

/// Melee in front of `eye` along `look` (suit punch / blade, mech punch).
#[allow(clippy::too_many_arguments)]
fn melee_front(c: &Construct, env: &Env<'_>, out: &mut ConstructOut, eye: Vec3, look: Vec3, reach: f32, dmg: f32, kb: f32, impact: f32) -> bool {
    let center = eye + look * reach * 0.6;
    let r = reach * 0.6;
    let mut any = false;
    for t in env.ctx.enemies_of(c.owner) {
        if Some(t.id) == c.pilot {
            continue;
        }
        let hull = env.ctx.hull_for(c.owner, t);
        if geom::dist_sq_point_aabb(&hull, center) > r * r {
            continue;
        }
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::Melee, dmg, kb, look, geom::closest_point(&hull, center))
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(impact),
        );
        any = true;
    }
    any
}

#[allow(clippy::too_many_arguments)]
pub(super) fn shoot(c: &Construct, out: &mut ConstructOut, from: Vec3, dir: Vec3, speed: f32, dmg: f32, kb: f32, explode: bool, arch: GunArchetype, recoil: f32) {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    out.events.push(GameEvent::ShotFired { owner: c.owner, construct: Some(c.id), archetype: arch, muzzle: from, dir, recoil, pellets: 1 });
    if recoil > 0.0 {
        out.recoils.push((c.owner, dir, recoil));
    }
    out.projectiles.push(ProjectileSpawn {
        owner: c.owner,
        construct: Some(c.id),
        kind: if explode { ProjectileKind::CannonBall } else { ProjectileKind::Bolt },
        pos: from,
        vel: dir * speed,
        damage: dmg,
        knockback: kb,
        element: c.recipe.element,
        radius: if explode { 0.35 } else { 0.2 },
        life: 2.5,
        explode: explode.then_some((2.8, 1.4)),
        hit_kind: HitKind::Shot,
        ricochet: 0,
        effects: 0,
        falloff_start: 0.0,
        chip: 0.0,
        muzzle_speed: speed,
    });
}

fn suit_attacks(c: &mut Construct, s: &mut HeldState, pilot: &PlayerView, env: &Env<'_>, out: &mut ConstructOut) {
    let ok = !pilot.busy;
    let p1 = ok && pilot.pressed & buttons::PRIMARY != 0;
    let h1 = ok && pilot.input.held(buttons::PRIMARY);
    let p2 = ok && pilot.pressed & buttons::SECONDARY != 0;
    let h2 = ok && pilot.input.held(buttons::SECONDARY);
    let (dm, km) = mech_mult(c);
    let look = pilot.look;
    let muzzle = pilot.hand + look * 0.4;
    let ranged = shooter(c) || c.has(Mechanic::Explodes);
    let mut anim = AnimState::Idle;
    if ranged && h1 && s.cd1 <= 0.0 {
        if c.has(Mechanic::Explodes) && !c.has(Mechanic::Shoots) {
            // Fusion cannon.
            shoot(c, out, muzzle, look, 55.0, 22.0 * dm, 18.0 * km, true, GunArchetype::Rocket, 4.0);
            s.cd1 = 0.9;
        } else {
            // Repulsor / ion blaster.
            shoot(c, out, muzzle, look, 90.0, 12.0 * dm, 9.0 * km, c.has(Mechanic::Explodes), GunArchetype::HandCannon, 1.0);
            s.cd1 = 0.28;
        }
        anim = AnimState::Active;
    } else if !ranged && p1 && s.cd1 <= 0.0 {
        melee_front(c, env, out, pilot.eye, look, 2.2, 12.0 * dm, 13.0 * km, 1.3);
        fx(out, c, HeldFx::PilotAttack, pilot.hand, look, 1.0);
        s.cd1 = 0.45;
        anim = AnimState::Active;
    }
    if c.has(Mechanic::Guard) {
        if h2 {
            if !s.guarding {
                s.guard_t = 0.0;
            }
            s.guarding = true;
            s.guard_t += env.dt();
            anim = AnimState::Active;
        }
    } else if p2 && s.cd2 <= 0.0 {
        if ranged || c.has(Mechanic::Blade) {
            // Energon axe / arm blade.
            melee_front(c, env, out, pilot.eye, look, 2.6, 16.0 * dm, 12.0 * km, 1.5);
            s.cd2 = 0.6;
        } else if c.has(Mechanic::Explodes) {
            for k in 0..3 {
                let right = look.cross(Vec3::Y).normalize_or_zero();
                let d = look + right * (k as f32 - 1.0) * 0.08 + Vec3::Y * 0.05;
                shoot(c, out, muzzle, d, 45.0, 10.0 * dm, 10.0 * km, true, GunArchetype::Rocket, if k == 0 { 3.0 } else { 0.0 });
            }
            s.cd2 = 2.0;
        } else {
            // Repulsor shove: 5 m cone push.
            for t in env.ctx.enemies_of(c.owner) {
                let d = t.hull.center() - pilot.eye;
                let len = d.length();
                if len < 5.0 && len > 1e-3 && d.dot(look) / len > 0.6 {
                    out.impulses.push((t.id, d / len * 14.0 + Vec3::Y * 3.0, 0.0));
                }
            }
            s.cd2 = 1.5;
        }
        fx(out, c, HeldFx::PilotAttack, pilot.hand, look, 1.3);
        anim = AnimState::Active;
    }
    let moving = Vec3::new(pilot.vel.x, 0.0, pilot.vel.z).length_squared() > 1.0;
    c.set_anim(if anim == AnimState::Idle && moving { AnimState::Move } else { anim }, env.tick());
}

fn can_fly_suit(c: &Construct) -> bool {
    c.has(Mechanic::Flies) || c.has(Mechanic::Thrust) || has_feature(c, Feature::Jet)
}

/// Suit movement (called from `worn::movement_mods`): transformer vehicle / robot speed, flight.
pub fn suit_movement_mods(c: &Construct, input: &PlayerInput, st: &PlayerState, mods: &mut MoveModifiers) {
    let s = &c.held;
    let jump = input.held(buttons::JUMP) && !input.held(buttons::TYPING);
    if c.has(Mechanic::Transforms) {
        if s.vehicle {
            // Drive: run speed 10 → 24 m/s, grippy acceleration, cars don't really jump.
            mods.speed_mult *= VEHICLE_SPEED / 10.0 / WORLD_SUIT_FACTOR;
            mods.accel_mult *= 1.5;
            if !can_fly_suit(c) {
                mods.jump_mult *= 0.5; // a flying alt mode takes off instead
            }
            mods.air_control *= 0.3;
        } else {
            mods.speed_mult *= ROBOT_SPEED / WORLD_SUIT_FACTOR;
        }
        if s.transform_t > 0.0 {
            mods.speed_mult *= 0.5;
        }
    }
    let can_fly = c.has(Mechanic::Flies) || c.has(Mechanic::Thrust) || has_feature(c, Feature::Jet);
    // Flying suits fly on held Jump — the jet (vehicle) mode of a flying transformer suit too, faster.
    if can_fly && jump && !st.grounded {
        mods.thrust = mods.thrust.max(if s.vehicle { 40.0 } else { 30.0 });
        mods.thrust_max_speed = if s.vehicle { VEHICLE_SPEED } else { 18.0 };
        mods.gravity_scale = mods.gravity_scale.min(0.2);
    }
}

// ------------------------------------------------------------------------------------ cockpit

fn pilot_speed(c: &Construct, rolling: bool) -> f32 {
    let scale = c.recipe.scale.max(0.3);
    let mut s = if rolling { 18.0 * (scale / 3.0).powf(-0.1) } else { 9.0 * (scale / 2.0).powf(-0.25) };
    if c.has(Mechanic::Fast) {
        s *= 1.4;
    }
    if c.has(Mechanic::Heavy) {
        s *= 0.75;
    }
    s.clamp(2.5, VEHICLE_SPEED)
}

/// Highest walkable surface under (x, z) within [y − down, y + up]. Returns the top y.
fn ground_at<V: VoxelQuery + ?Sized>(v: &V, p: Vec3, up: f32, down: f32) -> Option<f32> {
    let top = world_to_voxel(p + Vec3::Y * up);
    let bottom = world_to_voxel(p - Vec3::Y * down);
    let mut y = top.y;
    while y >= bottom.y {
        let cell = bd_core::IVec3::new(top.x, y, top.z);
        if v.is_solid(cell) && !v.is_solid(cell + bd_core::IVec3::Y) {
            return Some((y + 1) as f32 * bd_core::VOXEL_SIZE);
        }
        y -= 1;
    }
    None
}

/// Kinematic ground integration for a piloted body (gravity, ground following with step-up, walls).
/// Works on the body's SOLE, not its pivot: creatures pivot at the feet, objects (vehicles) at their
/// centre — a piloted car / truck / robot-mode transformer stands on the ground instead of sinking
/// half into it (and a transformer's reshaped robot box can't get stuck under the surface).
fn integrate(c: &mut Construct, env: &Env<'_>) {
    let dt = env.dt();
    let v = env.ctx.voxels;
    let off = Vec3::Y * (c.local_center.y - c.half.y); // pivot → sole
    c.vel.y -= 25.0 * dt;
    let step_up = (c.recipe.scale * 0.2).clamp(1.0, 4.0);
    let feet = c.pos + off;
    let next = feet + c.vel * dt;
    match ground_at(v, next, step_up, (c.vel.y.abs() * dt + 0.05).max(0.05)) {
        Some(gy) if gy >= next.y - 0.05 && c.vel.y <= 0.5 => {
            c.pos = Vec3::new(next.x, gy, next.z) - off;
            c.vel.y = 0.0;
            c.grounded = true;
        }
        _ => {
            let wall = ground_at(v, next, 30.0, 0.0).is_some_and(|gy| gy > feet.y + step_up);
            if wall {
                c.vel.x = 0.0;
                c.vel.z = 0.0;
                c.pos.y = next.y.max(feet.y) - off.y;
            } else {
                c.pos = next - off;
            }
            c.grounded = false;
        }
    }
}

/// [`super::StepFn`] for [`super::Controller::Piloted`].
pub fn step_piloted(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    c.body.claim(Driver::Attached);
    c.body.loco = Loco::Kinematic;
    let Some(pv) = c.pilot.and_then(|p| env.ctx.player(p)).copied() else {
        c.pilot = None;
        return;
    };
    if !c.active() {
        return;
    }
    let dt = env.dt();
    let mut s = c.held;
    if !s.mode_init {
        s.mode_init = true;
        s.vehicle = starts_as_vehicle(c);
    }
    tick_timers(&mut s, dt);
    s.third_person = true;
    // BEAST MODES (FEEL pass): a transformer whose alt mode walks (a T-rex, a gorilla) fights in
    // that mode too, with its archetype's `alt` row (bites, tail sweeps, breath, pounces, roars).
    let tt = &env.ctx.tuning.transformer;
    let beast_mode = c.has(Mechanic::Transforms) && s.vehicle && !(flies(c) || c.has(Mechanic::Thrust)) && !wheeled(c) && super::brawl::beast_alt(c, tt);
    // VOICE ORDERS (`brawl::order`: "optimus prime combo", "ion blaster", "energon axe"…): this
    // tick's scripted buttons. Vehicle mode first bursts / boosts / snaps to robot.
    let mut bi = super::brawl::Input::of_pilot(&pv.input, pv.pressed, pv.look, false);
    if c.has(Mechanic::Transforms) && c.brawl.order != super::brawl::Order::None && s.transform_t <= 0.0 && c.brawl.xatk == super::brawl::XPhase::None {
        c.held = s;
        if s.vehicle && !beast_mode {
            vehicle_order(c, env, out, pv.look, c.vel);
        } else if super::brawl::run_order(c, env, &mut bi) {
            c.held.transform_req = true;
        }
        s = c.held;
    }
    // TRANSFORMER COMBAT (`brawl.rs`): Transform during / right after a string = a transform attack
    // (the vehicle bursts through the target, then the robot's finisher); while it runs it drives
    // the body, and a second Transform press brings the finisher early. In a beast mode it is the
    // robot bursting out of the beast into its finisher.
    if c.has(Mechanic::Transforms) {
        if s.transform_req && !s.vehicle && s.transform_t <= 0.0 && super::brawl::wants_xattack(c) {
            s.transform_req = false;
            c.held = s;
            super::brawl::start_xattack(c, env, out, pv.look);
            s = c.held;
        } else if s.transform_req && beast_mode && s.transform_t <= 0.0 && super::brawl::wants_burst_out(c, tt) {
            s.transform_req = false;
            c.held = s;
            super::brawl::burst_out(c, env, out, pv.look);
            s = c.held;
        }
        if c.brawl.xatk != super::brawl::XPhase::None {
            let again = s.transform_req;
            s.transform_req = false;
            c.held = s;
            let grounded = c.grounded;
            let flying = super::brawl::step_xattack(c, env, out, again, grounded, &bi);
            if flying {
                fly_integrate(c, env);
            } else {
                integrate(c, env);
            }
            c.held.third_person = true;
            return;
        }
    }
    if s.transform_req {
        s.transform_req = false;
        // Airborne = really off the ground (not merely "not yet integrated": a transform requested
        // while it formed must not slam a crater under it).
        let sole = c.pos + Vec3::Y * (c.local_center.y - c.half.y);
        let airborne = !c.grounded && (c.vel.y.abs() > 0.5 || ground_at(env.ctx.voxels, sole, 0.3, 0.15).is_none());
        let fall = -c.vel.y;
        toggle(c, &mut s, airborne, fall, env, out);
    }
    let inp = pv.input;
    let (sy, cy) = inp.yaw.sin_cos();
    let cam_fwd = Vec3::new(-sy, 0.0, -cy);
    let cam_right = Vec3::new(cy, 0.0, -sy);
    let (rolling, flier, boost) = locomotion(c, &s);
    let jet = flier && jet_like(c);
    // TRANSFORMER COMBAT: the archetype tailors the robot's speed / air control and the vehicle's
    // top speed / grip / drift / turn rate (combat.ron `transformer.archetypes`).
    let transformer = c.has(Mechanic::Transforms);
    let brawler = transformer && !s.vehicle && !rolling && !flier;
    let beast = beast_mode && !rolling && !flier;
    let fighter = brawler || beast;
    let (robot_speed, air_control, veh_speed, veh_grip, drift_boost, veh_turn) = if transformer {
        let t = &env.ctx.tuning.transformer;
        let a = t.robot_row(c);
        let (rs, ac) = match t.alt_row(c).filter(|_| beast) {
            Some(b) => (b.robot_speed, b.air_control),
            None => (a.robot_speed, a.air_control),
        };
        (rs, ac, a.vehicle_speed, a.vehicle_grip, a.drift_boost, a.vehicle_turn)
    } else {
        (1.0, 1.0, 1.0, 1.0, 0.0, 1.0)
    };
    let speed = if flier {
        flight_speed(c, jet)
    } else if rolling {
        pilot_speed(c, rolling) * veh_speed
    } else if fighter {
        pilot_speed(c, rolling) * robot_speed
    } else {
        pilot_speed(c, rolling)
    };
    let scale = c.recipe.scale.max(0.5);
    // Big things turn slower.
    let turn = 3.5 / (scale / 2.0).max(1.0).powf(0.4) * if rolling { veh_turn } else { 1.0 };
    let cur_yaw = {
        let f = c.forward();
        (-f.x).atan2(-f.z)
    };
    // TITANS (`titan.rs`): a piloted walker's body is its pilot's movement state (stepped by the
    // world and by client prediction); here it only follows: feet, torso yaw after the aim, attacks.
    if super::titan::is_titan(c) {
        let tp = super::titan::params(c, &env.ctx.tuning.giants);
        c.pos = pv.pos - Vec3::Y * tp.lift;
        c.vel = pv.vel;
        c.grounded = pv.grounded;
        c.rot = Quat::from_rotation_y(super::titan::torso_yaw(cur_yaw, inp.yaw, env.ctx.tuning.giants.titan_torso_lag, dt));
        s.dash_t = 0.0;
        let fwd = c.forward();
        // A dash rams (once per dash).
        let walk = tp.speed;
        let hs = Vec3::new(c.vel.x, 0.0, c.vel.z).length();
        if hs > walk * 1.5 {
            dash_hits(c, env, out);
        } else {
            c.hits.clear();
        }
        pilot_attacks(c, &mut s, &pv, env, out, fwd);
        s.dash_t = 0.0;
        if c.anim_state != AnimState::Active {
            c.set_anim(if hs > 1.0 { AnimState::Move } else { AnimState::Idle }, env.tick());
        }
        c.held = s;
        return;
    }
    let blend = (6.0 * dt).min(1.0);
    if flier && s.dash_t <= 0.0 {
        // FLIGHT (arcade): fly where the mouse aims. Jets keep a minimum speed (W throttle, S brakes;
        // near the ground S hover-taxis / lands), bank into turns; helicopters / UFOs / flying beasts
        // hover (WASD camera-relative, the aim's pitch included). Space climbs, Ctrl dives.
        if jet {
            fly_jet(c, &inp, pv.look, speed, scale, env);
        } else {
            fly_hover(c, &inp, pv.look, cam_right, cur_yaw, speed, turn, env);
        }
    }
    let mut dyaw = inp.yaw - cur_yaw;
    while dyaw > std::f32::consts::PI {
        dyaw -= std::f32::consts::TAU;
    }
    while dyaw < -std::f32::consts::PI {
        dyaw += std::f32::consts::TAU;
    }
    // DRIFT (transformer vehicle modes): hold Dodge while turning — the tail slides, it turns
    // sharper, and letting go fires a boost.
    let speed_h = Vec3::new(c.vel.x, 0.0, c.vel.z).length();
    let drifting = rolling && transformer && drift_boost > 0.0 && inp.held(buttons::DODGE) && dyaw.abs() > 0.2 && speed_h > 6.0;
    if !(flier && s.dash_t <= 0.0) {
        let turn = if drifting { turn * 1.6 } else { turn };
        let yaw = cur_yaw + dyaw.clamp(-turn * dt, turn * dt);
        c.rot = Quat::from_rotation_y(yaw);
    }
    let fwd = c.forward();
    if s.dash_t > 0.0 {
        // Charge dash in progress.
        c.vel = Vec3::new(fwd.x * 25.0, c.vel.y, fwd.z * 25.0);
        dash_hits(c, env, out);
    } else if rolling {
        // Throttle along the facing; the hull steers toward the camera; lateral grip.
        let throttle = inp.move_z.clamp(-1, 1) as f32;
        let along = c.vel.dot(fwd);
        let target = throttle * speed * if throttle < 0.0 { 0.5 } else { 1.0 };
        let mut along = along + (target - along) * (2.5 * dt).min(1.0);
        // Lateral grip: plain vehicles have full grip; transformers slide by their archetype, a drift
        // lets the tail go.
        let lateral = if transformer {
            let h = Vec3::new(c.vel.x, 0.0, c.vel.z);
            let lat = h - fwd * h.dot(fwd);
            let k = if drifting { 1.2 } else { 30.0 * veh_grip.max(0.05) };
            lat * (-k * dt).exp()
        } else {
            Vec3::ZERO
        };
        if drifting {
            c.brawl.drift_t += dt;
        } else {
            if c.brawl.drift_t > 0.35 && drift_boost > 0.0 {
                along += drift_boost;
                fx(out, c, HeldFx::Thrust, c.center() - fwd * c.half.z, -fwd, 1.5);
            }
            c.brawl.drift_t = 0.0;
        }
        c.vel = Vec3::new(fwd.x * along + lateral.x, c.vel.y, fwd.z * along + lateral.z);
    } else if !flier && c.brawl.pounce_t <= 0.0 {
        let wish = (cam_fwd * inp.move_z.clamp(-1, 1) as f32 + cam_right * inp.move_x.clamp(-1, 1) as f32).normalize_or_zero();
        let want = wish * speed;
        let blend = if fighter && !c.grounded { blend * air_control } else { blend };
        c.vel.x += (want.x - c.vel.x) * blend;
        c.vel.z += (want.z - c.vel.z) * blend;
    }
    if !flier && inp.held(buttons::JUMP) && c.grounded && !rolling {
        c.vel.y = (7.0 + scale * 0.6).min(14.0);
        c.grounded = false;
    } else if boost && inp.held(buttons::JUMP) && !c.grounded {
        // Robot mode of a flying transformer: hold Jump to hover / boost (like a flying suit).
        c.vel.y = c.vel.y.max((c.vel.y + 40.0 * dt).min(7.0));
    }
    // TRANSFORMER COMBAT: robot-mode strings / heavies / air combos / blaster / boost-step (these
    // may override the velocity: the boost-step, a lunge, the air float).
    if fighter && s.transform_t <= dt {
        c.held = s;
        let grounded = c.grounded;
        if beast {
            super::brawl::beast(c, env, out, &bi, grounded);
        } else {
            super::brawl::robot(c, env, out, &bi, grounded);
        }
        s = c.held;
    }
    if flier {
        fly_integrate(c, env);
    } else {
        integrate(c, env);
    }
    if fighter {
        super::brawl::land(c, env, out, c.grounded);
    }
    if s.slam_armed {
        s.fall_speed = s.fall_speed.max(-c.vel.y);
        if c.grounded {
            s.slam_armed = false;
            slam(c, env, out, c.pos, s.fall_speed, HeldFx::TransformSlam);
        }
    }
    let moving = Vec3::new(c.vel.x, 0.0, c.vel.z).length_squared() > 1.0;
    if s.transform_t > 0.0 {
        c.set_anim(AnimState::Telegraph, env.tick());
    } else if rolling {
        c.set_anim(if moving { AnimState::Move } else { AnimState::Idle }, env.tick());
        if moving {
            let v = c.vel;
            let mut ps = s;
            // Rams (vehicle / tank).
            let speed_h = Vec3::new(v.x, 0.0, v.z).length();
            if speed_h > 10.0 && ps.ram_cd <= 0.0 {
                c.hits.clear();
                ps.ram_cd = 0.5;
                dash_hits(c, env, out);
            }
            s = ps;
        }
    } else {
        if !fighter {
            pilot_attacks(c, &mut s, &pv, env, out, fwd);
        }
        if c.anim_state != AnimState::Active {
            c.set_anim(if moving { AnimState::Move } else { AnimState::Idle }, env.tick());
        }
    }
    c.held = s;
}

/// Body-contact hits (Charge dash / ram): once per dash per target.
fn dash_hits(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let b = geom::inflate(&c.bounds(), 0.3);
    let spec = form_spec(c.recipe.form);
    let (dm, km) = mech_mult(c);
    let speed = c.vel.length();
    for t in env.ctx.enemies_of(c.owner) {
        if Some(t.id) == c.pilot || !b.intersects(&t.hull) || !c.hits.insert(player_key(t.id)) {
            continue;
        }
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::Creature, (spec.damage.max(8.0) + speed * 0.3) * dm, (12.0 + speed * 0.4) * km, c.vel + Vec3::Y * 2.0, t.hull.center())
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(1.5),
        );
    }
}

fn pilot_attacks(c: &mut Construct, s: &mut HeldState, pv: &PlayerView, env: &Env<'_>, out: &mut ConstructOut, fwd: Vec3) {
    let inp = pv.input;
    let p1 = pv.pressed & buttons::PRIMARY != 0;
    let h1 = inp.held(buttons::PRIMARY);
    let p2 = pv.pressed & buttons::SECONDARY != 0;
    let spec = form_spec(c.recipe.form);
    let (dm, km) = mech_mult(c);
    let sz = (c.recipe.scale / 2.0).clamp(0.5, 11.0);
    let aim = pv.look;
    let head = c.center() + Vec3::Y * c.half.y * 0.6 + fwd * c.half.z;
    if c.has(Mechanic::Breath) && h1 && s.cd1 <= 0.0 {
        // Breath cone along the aim.
        let range = (5.0 + c.half.y * 2.0).min(30.0);
        for t in env.ctx.enemies_of(c.owner) {
            let d = t.hull.center() - head;
            let len = d.length();
            if len < range && len > 1e-3 && d.dot(aim) / len > 0.85 {
                out.player_hits.push(HitInfo::new(t.id, Some(c.owner), HitKind::Creature, 4.0 * dm, 3.0, aim, t.hull.center()).with_element(c.recipe.element).with_construct(c.id).with_impact(0.5));
            }
        }
        s.cd1 = 0.15;
        c.set_anim(AnimState::Active, env.tick());
    } else if shooter(c) && p1 && s.cd1 <= 0.0 {
        let explode = c.has(Mechanic::Explodes) || c.recipe.payload.as_ref().is_some_and(|p| p.has(Mechanic::Explodes) || p.form == Form::Bomb);
        shoot(c, out, head + aim * 0.5, aim, 60.0, 16.0 * sz.sqrt() * dm, 14.0 * km, explode, GunArchetype::Cannon, 0.0);
        s.cd1 = 0.5;
        c.set_anim(AnimState::Active, env.tick());
        fx(out, c, HeldFx::PilotAttack, head, aim, sz.sqrt());
    } else if p1 && s.cd1 <= 0.0 {
        // Punch / swipe along the aim.
        let reach = c.half.z.max(c.half.x) + 2.0;
        let front = c.center() + Vec3::new(aim.x, aim.y * 0.5, aim.z).normalize_or_zero() * reach * 0.6;
        for t in env.ctx.enemies_of(c.owner) {
            if geom::dist_sq_point_aabb(&t.hull, front) <= reach * reach {
                out.player_hits.push(
                    HitInfo::new(t.id, Some(c.owner), HitKind::Creature, spec.damage.max(8.0) * sz.sqrt() * dm, spec.knockback.max(10.0) * sz.sqrt().min(2.0) * km, aim, t.hull.center())
                        .with_element(c.recipe.element)
                        .with_construct(c.id)
                        .with_impact(1.4),
                );
            }
        }
        s.cd1 = 0.6;
        c.set_anim(AnimState::Active, env.tick());
        fx(out, c, HeldFx::PilotAttack, front, aim, sz.sqrt());
    }
    if p2 && s.cd2 <= 0.0 {
        if c.has(Mechanic::Charge) {
            s.dash_t = 0.5;
            c.hits.clear();
            s.cd2 = 2.0;
            fx(out, c, HeldFx::PilotAttack, c.center(), fwd, 1.5);
        } else if c.grounded {
            // Stomp: ring shockwave + crater under the feet.
            let ring = (scale_ring(c)).clamp(3.0, 10.0);
            for t in env.ctx.enemies_of(c.owner) {
                if Some(t.id) == c.pilot || (t.pos.y - c.pos.y).abs() > 2.5 {
                    continue;
                }
                let d = Vec3::new(t.pos.x - c.pos.x, 0.0, t.pos.z - c.pos.z);
                if d.length() < ring {
                    out.player_hits.push(
                        HitInfo::new(t.id, Some(c.owner), HitKind::Creature, 8.0 * sz.sqrt() * dm, 13.0 * km, d.normalize_or_zero() + Vec3::Y, t.pos)
                            .with_construct(c.id)
                            .with_impact(1.6),
                    );
                    if t.grounded {
                        out.statuses.push((t.id, StatusKind::Stun, 0.12, Some(c.owner)));
                    }
                }
            }
            out.carves.push(CarveRequest { center: c.pos, radius: (c.recipe.scale * 0.25).clamp(1.0, 3.0), power: 0.7 });
            s.cd2 = 1.2;
            c.set_anim(AnimState::Active, env.tick());
            fx(out, c, HeldFx::PilotAttack, c.pos, Vec3::NEG_Y, 2.0);
        }
    }
}

fn scale_ring(c: &Construct) -> f32 {
    c.recipe.scale * 1.0
}

// ------------------------------------------------------------------------------------ weak points

/// Weak points of a construct in its local frame (relative to its centre): nape (top-back), cockpit
/// (top), two leg joints. Only piloted giants, kaiju and big creatures have them.
pub fn weak_points(p: &Proxy, out: &mut [(Vec3, f32); 4]) -> usize {
    if p.giant {
        return 0; // GIANTS: their anatomy has its own weak points (`giant::damage`)
    }
    let big = p.has(Mechanic::Piloted) || p.form == Form::Kaiju || (p.category == bd_core::mechanics::FormCategory::Creature && p.half.y >= 1.5);
    if !big {
        return 0;
    }
    let h = p.half;
    let r = (h.min_element() * 0.6).clamp(0.4, 2.5);
    let locals = [Vec3::new(0.0, h.y * 0.75, h.z * 0.8), Vec3::new(0.0, h.y, 0.0), Vec3::new(h.x * 0.5, -h.y * 0.35, 0.0), Vec3::new(-h.x * 0.5, -h.y * 0.35, 0.0)];
    for (k, l) in locals.iter().enumerate() {
        out[k] = (p.center + p.rot * *l, r);
    }
    4
}

/// Damage multiplier for a hit at `point` on `p`: ×[`WEAK_MULT`] on a weak point for Blade / Pierce.
pub fn weak_mult(p: &Proxy, point: Vec3, sharp: bool) -> f32 {
    if !sharp {
        return 1.0;
    }
    let mut pts = [(Vec3::ZERO, 0.0); 4];
    let n = weak_points(p, &mut pts);
    if pts[..n].iter().any(|(c, r)| (point - *c).length_squared() <= r * r) { WEAK_MULT } else { 1.0 }
}
