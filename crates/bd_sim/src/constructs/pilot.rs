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

/// Rolling locomotion (tank / car / vehicle mode) vs walking / flying.
fn rolls(c: &Construct, s: &HeldState) -> bool {
    if c.has(Mechanic::Transforms) {
        return s.vehicle;
    }
    c.recipe.form == Form::Vehicle || has_feature(c, Feature::Wheels) || has_feature(c, Feature::Treads)
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
    } else {
        suit_attacks(c, &mut s, &pilot, env, out);
    }
    c.held = s;
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
fn shoot(c: &Construct, out: &mut ConstructOut, from: Vec3, dir: Vec3, speed: f32, dmg: f32, kb: f32, explode: bool, arch: GunArchetype, recoil: f32) {
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

/// Suit movement (called from `worn::movement_mods`): transformer vehicle / robot speed, flight.
pub fn suit_movement_mods(c: &Construct, input: &PlayerInput, st: &PlayerState, mods: &mut MoveModifiers) {
    let s = &c.held;
    let jump = input.held(buttons::JUMP) && !input.held(buttons::TYPING);
    if c.has(Mechanic::Transforms) {
        if s.vehicle {
            // Drive: run speed 10 → 24 m/s, grippy acceleration, cars don't really jump.
            mods.speed_mult *= VEHICLE_SPEED / 10.0 / WORLD_SUIT_FACTOR;
            mods.accel_mult *= 1.5;
            mods.jump_mult *= 0.5;
            mods.air_control *= 0.3;
        } else {
            mods.speed_mult *= ROBOT_SPEED / WORLD_SUIT_FACTOR;
        }
        if s.transform_t > 0.0 {
            mods.speed_mult *= 0.5;
        }
    }
    let can_fly = c.has(Mechanic::Flies) || c.has(Mechanic::Thrust) || has_feature(c, Feature::Jet);
    if can_fly && jump && !st.grounded && !s.vehicle {
        mods.thrust = mods.thrust.max(30.0);
        mods.thrust_max_speed = 18.0;
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

/// Kinematic integration for a piloted body (ground following with step-up, or flight).
fn integrate(c: &mut Construct, env: &Env<'_>, flier: bool) {
    let dt = env.dt();
    let v = env.ctx.voxels;
    if flier {
        let next = c.pos + c.vel * dt;
        if v.is_solid(world_to_voxel(next + c.rot * c.local_center)) || v.is_solid(world_to_voxel(next)) {
            c.vel = Vec3::ZERO;
        } else {
            c.pos = next;
        }
        c.grounded = false;
        return;
    }
    c.vel.y -= 25.0 * dt;
    let step_up = (c.recipe.scale * 0.2).clamp(1.0, 4.0);
    let next = c.pos + c.vel * dt;
    match ground_at(v, next, step_up, (c.vel.y.abs() * dt + 0.05).max(0.05)) {
        Some(gy) if gy >= next.y - 0.05 && c.vel.y <= 0.5 => {
            c.pos = Vec3::new(next.x, gy, next.z);
            c.vel.y = 0.0;
            c.grounded = true;
        }
        _ => {
            let wall = ground_at(v, next, 30.0, 0.0).is_some_and(|gy| gy > c.pos.y + step_up);
            if wall {
                c.vel.x = 0.0;
                c.vel.z = 0.0;
                c.pos.y = next.y.max(c.pos.y);
            } else {
                c.pos = next;
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
    if s.transform_req {
        s.transform_req = false;
        let airborne = !c.grounded;
        let fall = -c.vel.y;
        toggle(c, &mut s, airborne, fall, env, out);
    }
    let inp = pv.input;
    let (sy, cy) = inp.yaw.sin_cos();
    let cam_fwd = Vec3::new(-sy, 0.0, -cy);
    let cam_right = Vec3::new(cy, 0.0, -sy);
    let rolling = rolls(c, &s);
    let flier = flies(c) && !rolling;
    let speed = pilot_speed(c, rolling);
    let scale = c.recipe.scale.max(0.5);
    // Big things turn slower.
    let turn = 3.5 / (scale / 2.0).max(1.0).powf(0.4);
    let cur_yaw = {
        let f = c.forward();
        (-f.x).atan2(-f.z)
    };
    let mut dyaw = inp.yaw - cur_yaw;
    while dyaw > std::f32::consts::PI {
        dyaw -= std::f32::consts::TAU;
    }
    while dyaw < -std::f32::consts::PI {
        dyaw += std::f32::consts::TAU;
    }
    let yaw = cur_yaw + dyaw.clamp(-turn * dt, turn * dt);
    c.rot = Quat::from_rotation_y(yaw);
    let fwd = c.forward();
    let blend = (6.0 * dt).min(1.0);
    if s.dash_t > 0.0 {
        // Charge dash in progress.
        c.vel = Vec3::new(fwd.x * 25.0, c.vel.y, fwd.z * 25.0);
        dash_hits(c, env, out);
    } else if rolling {
        // Throttle along the facing; the hull steers toward the camera; lateral grip.
        let throttle = inp.move_z.clamp(-1, 1) as f32;
        let along = c.vel.dot(fwd);
        let target = throttle * speed * if throttle < 0.0 { 0.5 } else { 1.0 };
        let along = along + (target - along) * (2.5 * dt).min(1.0);
        c.vel = Vec3::new(fwd.x * along, c.vel.y, fwd.z * along);
    } else {
        let wish = (cam_fwd * inp.move_z.clamp(-1, 1) as f32 + cam_right * inp.move_x.clamp(-1, 1) as f32).normalize_or_zero();
        let mut want = wish * speed;
        if flier {
            if inp.held(buttons::JUMP) {
                want.y = speed * 0.6;
            } else if inp.held(buttons::CROUCH) {
                want.y = -speed * 0.6;
            }
            c.vel += (want - c.vel) * blend;
        } else {
            c.vel.x += (want.x - c.vel.x) * blend;
            c.vel.z += (want.z - c.vel.z) * blend;
        }
    }
    if !flier && inp.held(buttons::JUMP) && c.grounded && !rolling {
        c.vel.y = (7.0 + scale * 0.6).min(14.0);
        c.grounded = false;
    }
    integrate(c, env, flier);
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
        pilot_attacks(c, &mut s, &pv, env, out, fwd);
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
