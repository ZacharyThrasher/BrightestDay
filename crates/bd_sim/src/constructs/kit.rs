//! **TRANSFORMERS V3 — MOBILITY PARITY + VEHICLE WEAPONS** (COMBAT.md §5d-sexies; owner: "compared
//! to the freedom when not in a transformer it feels very limited. It's hard to catch up to /
//! follow people" + "megatron not being able to shoot while in tank form").
//!
//! * **The pilot's kit in robot mode** ([`step_body`]): a piloted hero-scale robot (or beast mode)
//!   moves with the PLAYER's own controller (`movement::step_player`): bunny-hop / ground-strafe
//!   acceleration, air strafing, the air jump, walls, slides, gas boost and **both tethers** (fired
//!   from the robot's hand: swing, reel to anchors, hook an enemy and reel to them). Same inputs,
//!   same momentum rules; the hull is the robot's (`MoveParams::scaled`), the speed is weight-scaled
//!   by the archetype row (never below `kit_speed_min`). The state lives in `Construct::kit`
//!   (host-only, like every cockpit: seat prediction is untouched, so it never fights the pilot).
//!   Dodge stays the robot's **boost-step** (`brawl::fight`, perfect-dodge window) — its momentum now
//!   carries into the hop. Bodies taller than `kit_max_height` (giants, combiners) keep the old walker.
//! * **Chase tools**: a forward boost-step **steers at the chase target** ([`dash_seek`]: the
//!   combo-target pick rule `powers::combo_target::pick`, else the enemy you last hit in front of
//!   you); **vehicle mode is the sprint** (instant throttle `vehicle_accel`, and transforming out of
//!   a dash / run launches the vehicle at ≥ `vehicle_launch` × its top speed, keeping more);
//!   **flash-transform pop-out** ([`pop_out`]: vehicle → robot with an enemy ahead inside
//!   `pop_range` is a fast `pop_secs` transform that carries you to melee range — drive in, pop out,
//!   hit); robot-mode fliers thrust on held Jump.
//! * **Vehicle weapons** ([`armament`] / [`vehicle_fire`], from the alt mode's DATA, never names):
//!   tank (treads, or a turret without wheels) = a turret that aims at the crosshair independently
//!   of the hull: RMB main-cannon shell (heavy, arcing, splash), LMB coaxial MG; jet = nose cannons
//!   (LMB) + missiles (RMB); helicopter / hoverer = chin gun (LMB) + rocket pods (RMB); car / truck /
//!   bike = RMB boost-ram, LMB gun only with Barrels / Turret. Host-authoritative like the robot
//!   blaster (`pilot::shoot` → `GameEvent::ShotFired` + projectiles), numbers in combat.ron
//!   `transformer.v3`; followers fire them too ([`ai_vehicle_fire`]). Every shot also emits
//!   `HeldFx::VehicleFire` (`strength` = [`VehicleGun`] code) for presentation.

use bd_core::PlayerId;
use bd_core::Vec3;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Feature, Mechanic};
use serde::{Deserialize, Serialize};

use super::held::{HeldState, fx};
use super::{Construct, ConstructOut, Env, PlayerView};
use crate::events::HeldFx;
use crate::movement::{Collider, ColliderId, CollisionWorld, MoveModifiers, MoveParams, PlayerState, step_player};
use crate::weapons::GunArchetype;

/// One vehicle gun (combat.ron `transformer.v3.<gun>`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GunTuning {
    /// Seconds between shots (held = auto-fire for LMB guns).
    pub cd: f32,
    /// Muzzle speed (m/s).
    pub speed: f32,
    pub dmg: f32,
    pub kb: f32,
    /// Explodes on impact (arcing cannon ball) instead of a straight bolt.
    pub explode: bool,
    /// Rounds per trigger (a salvo; alternating sides).
    pub salvo: u8,
}

impl Default for GunTuning {
    fn default() -> Self {
        GunTuning { cd: 0.12, speed: 90.0, dmg: 3.0, kb: 2.0, explode: false, salvo: 1 }
    }
}

const fn gun(cd: f32, speed: f32, dmg: f32, kb: f32, explode: bool, salvo: u8) -> GunTuning {
    GunTuning { cd, speed, dmg, kb, explode, salvo }
}

/// `combat.ron` → `transformer.v3` (host-synced with the rest of the transformer tuning).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct V3Tuning {
    /// Robot mode runs the pilot's movement kit (false = the V2 walker).
    pub kit: bool,
    /// Bodies taller than this (m) keep the heavy walker (giants, combiners).
    pub kit_max_height: f32,
    /// Robot speed = the row's `robot_speed` × this, clamped to [`kit_speed_min`, `kit_speed_max`].
    pub kit_speed: f32,
    pub kit_speed_min: f32,
    pub kit_speed_max: f32,
    /// Jump height multiplier (a robot jumps a touch higher than a person).
    pub kit_jump: f32,
    /// A forward boost-step steers at the chase target within this range (m).
    pub dash_seek_range: f32,
    /// ... when the wish direction is within this cosine of the target (0.3 ≈ 72°).
    pub dash_seek_cos: f32,
    /// Vehicle throttle response (1/s; 2.5 = the V2 truck, ~10 = instant).
    pub vehicle_accel: f32,
    /// Robot → vehicle: the vehicle starts at least at this fraction of its top speed.
    pub vehicle_launch: f32,
    /// ... and keeps up to this multiple of its top speed of the momentum it transformed out of.
    pub vehicle_carry_max: f32,
    /// Flash-transform pop-out: vehicle → robot with an enemy ahead within this range (m).
    pub pop_range: f32,
    /// ... is this fast a transform (s) ...
    pub pop_secs: f32,
    /// ... that carries you to this far off them (m, from the body's edge) at up to `pop_speed` m/s.
    pub pop_gap: f32,
    pub pop_speed: f32,
    /// Robot-mode fliers: thrust on held Jump in the air (m/s², top m/s).
    pub fly_thrust: f32,
    pub fly_max: f32,
    /// Car / truck / bike RMB boost-ram: speed (× the vehicle's top speed), seconds, cooldown.
    pub ram_boost: f32,
    pub ram_secs: f32,
    pub ram_cd: f32,
    /// Tank turret main cannon (RMB) and coaxial MG (LMB).
    pub shell: GunTuning,
    pub coax: GunTuning,
    /// Jet nose cannons (LMB) and missiles (RMB).
    pub nose: GunTuning,
    pub missile: GunTuning,
    /// Helicopter chin gun (LMB) and rocket pods (RMB).
    pub chin: GunTuning,
    pub rockets: GunTuning,
    /// A car's gun (only with Barrels / Turret).
    pub car_gun: GunTuning,
    /// Followers fire vehicle weapons at targets within this range (m).
    pub ai_range: f32,
}

impl Default for V3Tuning {
    fn default() -> Self {
        V3Tuning {
            kit: true,
            kit_max_height: 7.0,
            kit_speed: 1.0,
            kit_speed_min: 0.9,
            kit_speed_max: 1.1,
            kit_jump: 1.1,
            dash_seek_range: 40.0,
            dash_seek_cos: 0.3,
            vehicle_accel: 9.0,
            vehicle_launch: 0.7,
            vehicle_carry_max: 1.3,
            pop_range: 16.0,
            pop_secs: 0.22,
            pop_gap: 1.2,
            pop_speed: 40.0,
            fly_thrust: 34.0,
            fly_max: 24.0,
            ram_boost: 1.35,
            ram_secs: 0.45,
            ram_cd: 1.6,
            shell: gun(1.4, 70.0, 22.0, 26.0, true, 1),
            coax: gun(0.1, 110.0, 2.6, 1.5, false, 1),
            nose: gun(0.09, 130.0, 2.8, 1.5, false, 2),
            missile: gun(1.1, 75.0, 13.0, 16.0, true, 2),
            chin: gun(0.1, 110.0, 2.6, 1.5, false, 1),
            rockets: gun(1.0, 70.0, 9.0, 12.0, true, 4),
            car_gun: gun(0.14, 110.0, 2.8, 1.5, false, 1),
            ai_range: 45.0,
        }
    }
}

// ------------------------------------------------------------------------------------ the kit

/// Does `c` (a piloted transformer's fighting body: robot mode or beast mode) move with the kit?
pub fn applies(c: &Construct, t: &V3Tuning) -> bool {
    t.kit && c.has(Mechanic::Transforms) && c.half.y * 2.0 <= t.kit_max_height && !super::titan::is_titan(c)
}

/// The player's movement numbers scaled to the robot's body (hull, eye, hand, steps).
pub fn params(c: &Construct, base: &MoveParams) -> MoveParams {
    let h = (c.half.y * 2.0).max(base.stand_height);
    let s = h / base.stand_height.max(0.1);
    let mut p = base.scaled(s);
    p.hull_half_width = c.half.x.min(c.half.z).clamp(base.hull_half_width, base.hull_half_width * s);
    p.hand_offset = base.hand_offset * s;
    p
}

/// Pivot → sole (the kit state's `pos` is the feet).
fn sole(c: &Construct) -> Vec3 {
    Vec3::Y * (c.local_center.y - c.half.y)
}

/// Weight-scaled modifiers: the archetype row's robot speed, clamped so a robot never feels sluggish.
pub fn mods(c: &Construct, t: &V3Tuning, row_speed: f32, inp: &PlayerInput, flier: bool, grounded: bool) -> MoveModifiers {
    let mut m = MoveModifiers::default();
    let mut s = (row_speed * t.kit_speed).clamp(t.kit_speed_min, t.kit_speed_max);
    if c.has(Mechanic::Heavy) {
        s = (s * 0.95).max(t.kit_speed_min);
    }
    m.speed_mult = s;
    m.jump_mult = t.kit_jump;
    if flier && inp.held(buttons::JUMP) && !inp.held(buttons::TYPING) && !grounded {
        m.thrust = t.fly_thrust;
        m.thrust_max_speed = t.fly_max;
        m.gravity_scale = m.gravity_scale.min(0.2);
    }
    m
}

/// One tick of the robot body on the pilot's kit. The body's `vel` going in carries every override
/// the moveset made (boost-step, lunges, knockback); `pos` / `vel` / `grounded` come out.
pub fn step_body(c: &mut Construct, pv: &PlayerView, env: &Env<'_>, row_speed: f32, flier: bool) {
    let t = &env.ctx.tuning.transformer.v3;
    let params = params(c, env.ctx.params);
    let off = sole(c);
    let mut k = c.kit.unwrap_or_else(|| {
        let mut k = PlayerState::new(c.pos + off, &params);
        k.prev_buttons = pv.input.buttons; // no phantom press on the first tick
        k.grounded = c.grounded;
        k
    });
    k.pos = c.pos + off;
    k.vel = if c.vel.is_finite() { c.vel } else { Vec3::ZERO };
    // Dodge is the robot's boost-step (brawl), not the on-foot dodge.
    let mut inp = pv.input;
    inp.buttons &= !buttons::DODGE;
    k.mods = mods(c, t, row_speed, &inp, flier, k.grounded);
    // Terrain + every construct but this one; enemies are hook targets (reel to them).
    let hook_r = env.ctx.tuning.assist.hook.radius;
    let mut cols: Vec<Collider> = env.ctx.colliders.iter().filter(|k| !matches!(k.id, ColliderId::Construct { id, .. } if id == c.id)).copied().collect();
    for e in env.ctx.enemies_of(c.owner) {
        if Some(e.id) != c.pilot && e.seat != Some(c.id) {
            cols.push(crate::hook::player_collider(e.id, e.hull, e.vel, hook_r));
        }
    }
    let pilot = c.pilot.unwrap_or(PlayerId(u8::MAX));
    let cw = CollisionWorld::new(env.ctx.voxels, &cols).ignoring(ColliderId::Player(pilot));
    let _ = step_player(&mut k, &inp, &cw, &params, env.dt());
    if !k.pos.is_finite() || !k.vel.is_finite() {
        c.kit = None;
        return;
    }
    c.pos = k.pos - off;
    c.vel = k.vel;
    c.grounded = k.grounded;
    c.kit = Some(k);
}

/// Leave the kit (vehicle mode, a transform attack, the pilot hopped out): ropes let go.
pub fn drop_kit(c: &mut Construct) {
    c.kit = None;
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// The chase target: the combo-target pick rule on the pilot's crosshair, else the enemy the pilot
/// last hit / aimed at when it's in front (within ~60° of the look) and in `range`.
pub fn chase_target(c: &Construct, pv: &PlayerView, env: &Env<'_>, range: f32) -> Option<Vec3> {
    let here = c.center();
    let voxels = env.ctx.voxels;
    let cands = env.ctx.enemies_of(c.owner).filter(|e| Some(e.id) != c.pilot).map(|e| (e.id, e.hull));
    let eye = here + Vec3::Y * c.half.y * 0.6;
    let picked = crate::powers::combo_target::pick(eye, pv.look, cands, |a, b| crate::powers::combo_target::clear_line(voxels, a, b));
    let id = picked.or(pv.last_target)?;
    let e = env.ctx.enemies_of(c.owner).find(|e| e.id == id)?;
    let p = e.hull.center();
    let d = p - here;
    if d.length() > range {
        return None;
    }
    if picked.is_none() && flat(d).normalize_or_zero().dot(flat(pv.look).normalize_or_zero()) < 0.5 {
        return None;
    }
    Some(p)
}

/// A boost-step aimed forward-ish steers at the chase target: the flat direction to dash in.
pub fn dash_seek(c: &Construct, pv: &PlayerView, env: &Env<'_>, wish: Vec3) -> Option<Vec3> {
    let t = &env.ctx.tuning.transformer.v3;
    let p = chase_target(c, pv, env, t.dash_seek_range)?;
    let dir = flat(p - c.center()).normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let w = flat(wish).normalize_or_zero();
    (w != Vec3::ZERO && w.dot(dir) >= t.dash_seek_cos).then_some(dir)
}

/// Robot → vehicle: launch the vehicle along the momentum (or the facing) at ≥ `vehicle_launch` ×
/// its top speed, keeping a dash's / hop's speed up to `vehicle_carry_max` × top.
pub fn launch_vehicle(c: &mut Construct, t: &V3Tuning, top: f32) {
    let h = flat(c.vel);
    let dir = if h.length() > 2.0 { h.normalize() } else { flat(c.forward()).normalize_or(Vec3::NEG_Z) };
    let speed = h.length().max(top * t.vehicle_launch).min(top * t.vehicle_carry_max.max(t.vehicle_launch));
    c.vel = Vec3::new(dir.x * speed, c.vel.y, dir.z * speed);
    c.rot = crate::geom::yaw_rotation(dir);
}

/// Vehicle → robot with an enemy ahead inside `pop_range`: a fast transform that carries the robot
/// to melee range facing them. Returns true when it popped.
pub fn pop_out(c: &mut Construct, s: &mut HeldState, pv: &PlayerView, env: &Env<'_>, out: &mut ConstructOut) -> bool {
    let t = &env.ctx.tuning.transformer.v3;
    let Some(p) = chase_target(c, pv, env, t.pop_range + c.half.z.max(c.half.x)).or_else(|| nearest_ahead(c, env, t.pop_range)) else { return false };
    let d = flat(p - c.center());
    let dir = d.normalize_or(flat(c.forward()).normalize_or(Vec3::NEG_Z));
    let body = c.half.x.max(c.half.z);
    let gap = (d.length() - body - t.pop_gap).max(0.0);
    let secs = t.pop_secs.max(0.05);
    let v = (gap / secs).min(t.pop_speed);
    c.vel = Vec3::new(dir.x * v, c.vel.y.max(1.5), dir.z * v);
    c.rot = crate::geom::yaw_rotation(dir);
    s.transform_t = s.transform_t.min(secs);
    let speed = (super::pilot::TRANSFORM_SECS / secs).max(1.0);
    fx(out, c, HeldFx::Transform, c.center(), dir, speed);
    true
}

fn nearest_ahead(c: &Construct, env: &Env<'_>, range: f32) -> Option<Vec3> {
    let here = c.center();
    let f = flat(c.forward()).normalize_or(Vec3::NEG_Z);
    env.ctx
        .enemies_of(c.owner)
        .filter(|e| Some(e.id) != c.pilot)
        .map(|e| e.hull.center())
        .filter(|p| {
            let d = flat(*p - here);
            d.length() <= range + c.half.z.max(c.half.x) && d.normalize_or_zero().dot(f) > 0.5
        })
        .min_by(|a, b| (*a - here).length_squared().total_cmp(&(*b - here).length_squared()))
}

// ------------------------------------------------------------------------------------ vehicle weapons

/// What an alt mode carries (from its data: flight, treads / wheels, turret / barrels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Armament {
    None,
    Tank,
    Jet,
    Heli,
    /// Car / truck / bike: RMB boost-ram; `gun` = LMB gun (Barrels / Turret).
    Car { gun: bool },
}

/// `HeldFx::VehicleFire` strength codes (presentation: muzzle flash / shell / smoke per gun).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VehicleGun {
    TankShell = 1,
    Coax = 2,
    Nose = 3,
    Missile = 4,
    Chin = 5,
    Rocket = 6,
    CarGun = 7,
    RamBoost = 8,
}

pub fn armament(c: &Construct) -> Armament {
    if !c.has(Mechanic::Transforms) {
        return Armament::None;
    }
    let f = &c.recipe.features;
    let has = |x: Feature| f.contains(&x);
    if super::pilot::flies(c) || c.has(Mechanic::Thrust) {
        return if super::pilot::jet_like(c) { Armament::Jet } else { Armament::Heli };
    }
    if has(Feature::Treads) || (has(Feature::Turret) && !has(Feature::Wheels)) {
        return Armament::Tank;
    }
    if super::pilot::wheeled(c) {
        return Armament::Car { gun: has(Feature::Barrels) || has(Feature::Turret) };
    }
    Armament::None
}

/// The turret / gun pivot (top of the hull) and the muzzle for `aim`.
fn muzzle(c: &Construct, aim: Vec3, side: f32, nose: bool) -> Vec3 {
    let up = c.center() + Vec3::Y * c.half.y * if nose { 0.0 } else { 0.8 };
    let right = c.rot * Vec3::X;
    let a = if nose { c.forward() } else { aim.normalize_or(c.forward()) };
    up + a * (c.half.z.max(c.half.x) * if nose { 1.0 } else { 0.6 } + 0.4) + right * side * c.half.x * 0.7
}

fn fire(c: &Construct, out: &mut ConstructOut, g: &GunTuning, aim: Vec3, nose: bool, code: VehicleGun, arch: GunArchetype, k: f32) {
    let aim = aim.normalize_or(c.forward());
    let n = g.salvo.max(1);
    for i in 0..n {
        let side = if n == 1 { 0.0 } else if i % 2 == 0 { 1.0 } else { -1.0 };
        let from = muzzle(c, aim, side, nose);
        // A salvo fans a touch so the rounds read as several.
        let fan = if n > 1 { (c.rot * Vec3::X) * side * 0.02 * (1 + i / 2) as f32 } else { Vec3::ZERO };
        let dir = (aim + fan).normalize_or(aim);
        // Arcing shells get a small lift so they land on the crosshair at mid range.
        let dir = if g.explode && arch == GunArchetype::Cannon { (dir + Vec3::Y * 0.04).normalize_or(dir) } else { dir };
        super::pilot::shoot(c, out, from, dir, g.speed, g.dmg * k, g.kb * k, g.explode, arch, if g.explode { 1.5 } else { 0.2 });
        fx(out, c, HeldFx::VehicleFire, from, dir, code as u8 as f32);
    }
}

/// Buttons for [`vehicle_fire`] (a pilot's input or an AI's decision).
#[derive(Clone, Copy, Debug, Default)]
pub struct Trigger {
    pub hold1: bool,
    pub press2: bool,
}

/// Vehicle-mode weapons for one tick. `aim` = the crosshair (turrets aim there regardless of the
/// hull). Uses `HeldState::cd1` (LMB gun) / `cd2` (RMB). Returns the weapon fired, if any.
pub fn vehicle_fire(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, aim: Vec3, trig: Trigger, top: f32) -> Option<VehicleGun> {
    let t = &env.ctx.tuning.transformer.v3;
    let (dm, _) = super::held::mech_mult(c);
    let k = dm * ((c.half.y * 2.0).max(0.5) / 3.0).sqrt().clamp(0.8, 1.5);
    let arm = armament(c);
    let (lmb, rmb): (Option<(&GunTuning, VehicleGun, GunArchetype, bool)>, Option<(&GunTuning, VehicleGun, GunArchetype, bool)>) = match arm {
        Armament::None => return None,
        Armament::Tank => (Some((&t.coax, VehicleGun::Coax, GunArchetype::Minigun, false)), Some((&t.shell, VehicleGun::TankShell, GunArchetype::Cannon, false))),
        Armament::Jet => (Some((&t.nose, VehicleGun::Nose, GunArchetype::Minigun, true)), Some((&t.missile, VehicleGun::Missile, GunArchetype::Rocket, true))),
        Armament::Heli => (Some((&t.chin, VehicleGun::Chin, GunArchetype::Minigun, false)), Some((&t.rockets, VehicleGun::Rocket, GunArchetype::Rocket, false))),
        Armament::Car { gun } => (gun.then_some((&t.car_gun, VehicleGun::CarGun, GunArchetype::Minigun, false)), None),
    };
    let mut fired = None;
    if trig.press2 && s.cd2 <= 0.0 {
        if let Some((g, code, arch, nose)) = rmb {
            fire(c, out, g, aim, nose, code, arch, k);
            s.cd2 = g.cd;
            fired = Some(code);
        } else if matches!(arm, Armament::Car { .. }) {
            // Boost-ram: a burst along the facing; the rolling rams do the hitting.
            let f = flat(c.forward()).normalize_or(Vec3::NEG_Z);
            let along = c.vel.dot(f).max(top * t.ram_boost);
            c.vel = Vec3::new(f.x * along, c.vel.y, f.z * along);
            c.hits.clear();
            s.dash_t = t.ram_secs;
            s.ram_cd = 0.0;
            s.cd2 = t.ram_cd;
            fx(out, c, HeldFx::VehicleFire, c.center() - f * c.half.z, -f, VehicleGun::RamBoost as u8 as f32);
            fx(out, c, HeldFx::Thrust, c.center() - f * c.half.z, -f, 1.5);
            fired = Some(VehicleGun::RamBoost);
        }
    }
    if trig.hold1 && s.cd1 <= 0.0 {
        if let Some((g, code, arch, nose)) = lmb {
            fire(c, out, g, aim, nose, code, arch, k);
            s.cd1 = g.cd;
            fired = fired.or(Some(code));
        }
    }
    fired
}

/// A follower's vehicle weapons: fire at `target` when it's in range (and, for fixed guns, ahead).
pub fn ai_vehicle_fire(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, target: Vec3) {
    let t = &env.ctx.tuning.transformer.v3;
    let arm = armament(c);
    if arm == Armament::None || c.held.transform_t > 0.0 || !c.held.vehicle {
        return;
    }
    let d = target - c.center();
    let dist = d.length();
    if dist > t.ai_range || dist < 1.0 {
        return;
    }
    let aim = d / dist;
    let ahead = flat(aim).normalize_or_zero().dot(flat(c.forward()).normalize_or_zero()) > 0.8;
    // Turrets aim anywhere; nose guns / rams need the target ahead.
    let can = arm == Armament::Tank || ahead;
    if !can {
        return;
    }
    let mut s = c.held;
    let dt = env.dt();
    s.cd1 = (s.cd1 - dt).max(0.0);
    s.cd2 = (s.cd2 - dt).max(0.0);
    let ram_ok = !matches!(arm, Armament::Car { .. }) || dist < 18.0;
    let trig = Trigger { hold1: true, press2: ram_ok };
    let top = crate::constructs::pilot::VEHICLE_SPEED;
    let _ = vehicle_fire(c, &mut s, env, out, aim, trig, top);
    c.held.cd1 = s.cd1;
    c.held.cd2 = s.cd2;
    c.held.dash_t = s.dash_t;
}
