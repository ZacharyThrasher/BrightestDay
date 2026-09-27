//! **Objects** (COMBAT.md §4 Objects) — construct engineer.
//!
//! Thrown, dropped or launched free bodies. The controller only does what the *form* means —
//! forming in the hand (or above the caster for colossal "throw a moon" objects), the launch, the
//! DropFromSky telegraph, vehicle driving, the bomb fuse — and hands everything else to physics
//! ([`super::body`]) and the generic components (impact hits, Explodes, Sticky, Ricochet,
//! Returning, Homing, Thrust, Flies…).
//!
//! * Ball — bouncy, 8 dmg. Anvil — DropFromSky + Heavy: 12 m above the aim point, 0.5 s shadow
//!   telegraph, crushes (30 dmg) and craters; slides on down a ramp. Bomb — Explodes: 1.2 s fuse or
//!   impact. Boulder — rolls, keeps momentum, follows slopes, flattens things. Rocket — Homing +
//!   Thrust + Explodes. Vehicle — drives along the ground in your look direction and rams
//!   (`features`: Wheels/Treads → ground, Rotor/Jet/Propeller → it flies). Blob0 — unknown word: a
//!   thrown lump, never nothing.

use bd_core::mechanics::{Form, Mechanic};
use bd_core::Vec3;

use super::components::{eff_has, speed_mult};
use super::{AnimState, Construct, ConstructOut, Driver, Env, Loco, form_spec};
use crate::geom;

/// Bomb fuse (s).
pub const BOMB_FUSE: f32 = 1.2;
/// Colossal objects cross the sky slowly (a telegraph everyone can dodge).
pub const COLOSSAL_THROW_SPEED: f32 = 14.0;

fn drops(c: &Construct) -> bool {
    eff_has(c, Mechanic::DropFromSky)
}

/// [`super::StepFn`] for [`super::Controller::Object`].
pub fn step(c: &mut Construct, env: &Env<'_>, _out: &mut ConstructOut) {
    let o = &env.owner;
    if !c.active() {
        c.body.loco = Loco::Kinematic;
        if c.colossal {
            // "Throw a moon": it forms above the caster.
            c.pos = o.pos + Vec3::Y * (c.half.y + 6.0) + o.look * 2.0;
        } else if !drops(c) && c.parent.is_none() {
            c.pos = o.hand + o.look * (c.half.max_element() + 0.3);
        }
        c.vel = Vec3::ZERO;
        return;
    }
    if !c.comp.launched {
        launch(c, env);
        return;
    }
    // DropFromSky: hover with a shadow telegraph, then fall.
    if drops(c) && !c.comp.dropped {
        c.comp.drop_t += env.dt();
        if c.body.claim(Driver::Hold) {
            c.body.loco = Loco::Kinematic;
            c.vel = Vec3::ZERO;
        }
        if c.comp.drop_t >= super::components::DROP_TELEGRAPH {
            c.comp.dropped = true;
            c.body.loco = Loco::Ballistic;
            c.body.gravity = 1.4;
            c.vel = Vec3::new(0.0, -8.0, 0.0);
            c.set_anim(AnimState::Move, env.tick());
        }
        return;
    }
    if c.recipe.form == Form::Vehicle && !eff_has(c, Mechanic::Transforms) {
        drive_vehicle(c, env);
    } else if c.body.loco == Loco::Kinematic && c.stuck_to.is_none() && c.comp.attached_to.is_none() {
        c.body.loco = Loco::Ballistic;
    }
}

fn launch(c: &mut Construct, env: &Env<'_>) {
    let o = &env.owner;
    c.comp.launched = true;
    c.comp.rest_t = 0.0;
    if drops(c) {
        c.comp.drop_t = c.comp.drop_t.min(0.0);
        c.set_anim(AnimState::Telegraph, env.tick());
        c.body.loco = Loco::Kinematic;
        return;
    }
    let spec = form_spec(c.recipe.form);
    // FREEDOM HOOK: thrown / driven at the player it was cast at by name (freedom/deliver.rs).
    let named = c.comp.named.and_then(|t| env.ctx.player(t)).filter(|t| t.alive).copied();
    if c.recipe.form == Form::Vehicle {
        let look = named.map_or(o.look, |t| (t.pos - c.pos).normalize_or(o.look));
        let h = Vec3::new(look.x, 0.0, look.z).normalize_or(c.forward());
        c.rot = geom::yaw_rotation(h);
        c.pos = Vec3::new(c.pos.x, c.pos.y.max(o.pos.y + c.half.y), c.pos.z);
        c.vel = h * spec.speed * speed_mult(c) * 0.5;
        c.body.loco = if eff_has(c, Mechanic::Flies) { Loco::Fly } else { Loco::Walk };
        c.set_anim(AnimState::Move, env.tick());
        return;
    }
    let speed = if c.colossal { COLOSSAL_THROW_SPEED } else { spec.speed.max(18.0) * speed_mult(c) };
    c.vel = o.look * speed + o.vel * 0.5 + Vec3::Y * 2.0;
    if let Some(t) = named {
        let flies = eff_has(c, Mechanic::Flies) || eff_has(c, Mechanic::Thrust);
        let g = if flies { 0.0 } else { super::body::GRAVITY * if c.colossal { 0.25 } else { 1.0 } };
        let aim_at = (t.hull.min + t.hull.max) * 0.5;
        if let Some(v) = crate::freedom::deliver::lob(c.center(), aim_at, t.vel, speed, g) {
            c.vel = v;
        }
    }
    let heading = c.vel.normalize_or(o.look);
    c.rot = geom::look_rotation(if named.is_some() { heading } else { o.look });
    c.body.loco = if eff_has(c, Mechanic::Flies) { Loco::Fly } else { Loco::Ballistic };
    if (c.recipe.form == Form::Bomb || (eff_has(c, Mechanic::Explodes) && c.recipe.form == Form::Blob0)) && !eff_has(c, Mechanic::Attract) && !eff_has(c, Mechanic::Returning) {
        c.comp.fuse = BOMB_FUSE;
    }
    if c.colossal {
        // Slow and floaty so everyone sees it coming.
        c.body.gravity = 0.25;
    }
    c.set_anim(AnimState::Move, env.tick());
}

/// Vehicle: drives along its heading and rams (contact hits in `components::react`). Homing turns
/// it toward the target; Flies / rotor features let it take off.
fn drive_vehicle(c: &mut Construct, env: &Env<'_>) {
    if !c.body.claim(Driver::Steer) {
        return;
    }
    let dt = env.dt();
    let spec = form_spec(c.recipe.form);
    let mut speed = spec.speed * speed_mult(c);
    let mut heading = Vec3::new(c.forward().x, 0.0, c.forward().z).normalize_or(Vec3::NEG_Z);
    if eff_has(c, Mechanic::Piloted) {
        // PILOTING: a vehicle you hopped out of is your FOLLOWER — it hunts the nearest enemy and
        // rams them; with no one to fight it drives back to you and idles nearby.
        let target = env.target_for(c);
        c.target = target.map(|t| t.id);
        let (goal, idle) = match target {
            Some(t) => (t.pos, false),
            None => (env.owner.pos, Vec3::new(env.owner.pos.x - c.pos.x, 0.0, env.owner.pos.z - c.pos.z).length() < 10.0),
        };
        let want = Vec3::new(goal.x - c.pos.x, 0.0, goal.z - c.pos.z).normalize_or(heading);
        let turn = (2.2 * dt).min(1.0);
        heading = (heading + (want - heading) * turn).normalize_or(heading);
        c.rot = geom::yaw_rotation(heading);
        if idle {
            speed = 0.0;
        }
    } else if eff_has(c, Mechanic::Homing) {
        if let Some(t) = env.target_for(c) {
            let want = Vec3::new(t.pos.x - c.pos.x, 0.0, t.pos.z - c.pos.z).normalize_or(heading);
            heading = (heading + (want - heading) * (2.0 * dt).min(1.0)).normalize_or(heading);
            c.rot = geom::yaw_rotation(heading);
        }
    }
    if eff_has(c, Mechanic::Flies) {
        c.body.loco = Loco::Fly;
        let v = heading * speed;
        c.vel += (Vec3::new(v.x, 0.0, v.z) - c.vel) * (3.0 * dt).min(1.0);
    } else {
        c.body.loco = Loco::Walk;
        let blend = (3.0 * dt).min(1.0);
        c.vel.x += (heading.x * speed - c.vel.x) * blend;
        c.vel.z += (heading.z * speed - c.vel.z) * blend;
    }
    c.set_anim(AnimState::Move, env.tick());
}
