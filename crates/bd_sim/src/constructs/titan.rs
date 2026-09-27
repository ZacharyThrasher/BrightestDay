//! **Titans** — piloting a giant walker feels like a Titanfall titan (GIANTS worker; owner: "Can we
//! make kaiju movement actually feel good somehow? How did a game like titan fall pull it off?").
//!
//! (The pilot's state rides the top of the body: `pos` = feet + [`TitanParams::lift`] — the seat,
//! so the pilot's hull and eye stay in the cockpit.)
//!
//! A *titan* is a piloted walker: a seated (not suit) construct that walks — it neither rolls nor
//! flies and is not a transformer (mechs, ridden kaiju / beasts, a "giant robot I can pilot"). Its
//! body is driven through the **pilot's own movement state**: while seated, the pilot's
//! `PlayerState` *is* the body's feet (`pos`, `vel`, `grounded`; dash charges in `gas`, the dash in
//! progress in `burst_cooldown`), stepped by the pure [`step`] on the host (`World::step_movement`)
//! and by client prediction (`World::predict_player`) alike — so the pilot's own titan responds
//! with zero input lag and reconciles like on-foot movement. The body follows it
//! (`pilot::step_piloted`: position = the pilot's feet, torso yaw).
//!
//! * **Decoupled aim**: the camera and every attack follow the mouse instantly (`pv.look`); the torso
//!   swings after the aim with a short lag (`titan_torso_lag`), the client plants the legs.
//! * **Responsive**: full speed in `titan_accel` s, stops in `titan_stop` s, strafes freely (the
//!   wish direction is camera-relative). Weight comes from presentation (camera dip, footfalls,
//!   thuds), never from input lag.
//! * **Dash** (Dodge key): `titan_charges` charges, one back every `titan_recharge` s: a heavy burst
//!   at `titan_dash` × walk speed for `titan_dash_secs` ([`TitanEvents::dashed`] → thud).
//! * **Landing crunch**: [`TitanEvents::landed`] carries the fall speed (client dip + crunch + dust).

use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Feature, Form, Mechanic};
use bd_core::voxel::{VoxelQuery, world_to_voxel};
use bd_core::{IVec3, VOXEL_SIZE, Vec3};

use super::Construct;
use super::giant::GiantTuning;
use crate::movement::PlayerState;

/// Is `c` a titan (its pilot's movement state drives it)?
pub fn is_titan(c: &Construct) -> bool {
    c.pilot.is_some() && walks(c)
}

/// Would `c` be a titan when piloted (a cockpit walker: not a suit, doesn't roll or fly, doesn't
/// transform)? Recipe-only, so a client replica agrees for its seated local player.
pub fn walks(c: &Construct) -> bool {
    if c.suit || c.has(Mechanic::Transforms) {
        return false;
    }
    let f = &c.recipe.features;
    let rolls = c.recipe.form == Form::Vehicle || f.contains(&Feature::Wheels) || f.contains(&Feature::Treads);
    let flies = c.has(Mechanic::Flies)
        || matches!(c.recipe.form, Form::Bird | Form::Insect)
        || f.contains(&Feature::Rotor)
        || f.contains(&Feature::Propeller)
        || f.contains(&Feature::Jet);
    !rolls && !flies
}

/// Per-titan movement numbers (from its recipe + `combat.ron` → `giants.titan_*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TitanParams {
    /// Walk speed (m/s).
    pub speed: f32,
    /// Acceleration / braking (m/s²).
    pub accel: f32,
    pub decel: f32,
    /// Tallest ledge it steps up (m).
    pub step_up: f32,
    pub jump: f32,
    pub dash_speed: f32,
    pub dash_secs: f32,
    pub charges: f32,
    pub recharge: f32,
    pub gravity: f32,
    /// The pilot rides the top of the body: pilot state position = feet + `lift`·Y.
    pub lift: f32,
}

/// Movement numbers for titan `c`.
pub fn params(c: &Construct, t: &GiantTuning) -> TitanParams {
    let scale = (c.recipe.scale * c.size_scale).max(0.5);
    let mut s = 9.0 * (scale / 2.0).powf(-0.25);
    if c.has(Mechanic::Fast) {
        s *= 1.4;
    }
    if c.has(Mechanic::Heavy) {
        s *= 0.75;
    }
    let speed = (s * t.titan_speed).clamp(2.5, 26.0);
    TitanParams {
        speed,
        accel: speed / t.titan_accel.max(0.02),
        decel: speed / t.titan_stop.max(0.02),
        step_up: (scale * 0.2).clamp(1.0, 4.0),
        jump: (7.0 + scale * 0.6).min(14.0),
        dash_speed: speed * t.titan_dash,
        dash_secs: t.titan_dash_secs,
        charges: t.titan_charges,
        recharge: t.titan_recharge.max(0.1),
        gravity: 25.0,
        lift: (c.local_center.y + c.half.y).max(0.5),
    }
}

/// What happened this step (presentation / move flags).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TitanEvents {
    pub dashed: bool,
    pub jumped: bool,
    /// Landed from a fall at this speed (m/s).
    pub landed: Option<f32>,
}

/// Seat a pilot in titan `c`: their movement state becomes the body's feet, dash charges full.
pub fn seat(s: &mut PlayerState, c: &Construct, t: &GiantTuning) {
    s.pos = c.pos + Vec3::Y * params(c, t).lift;
    s.vel = c.vel;
    s.grounded = c.grounded;
    s.gas = params(c, t).charges;
    s.burst_cooldown = 0.0;
    s.air_time = 0.0;
}

/// Highest walkable surface under (x, z) within [y − down, y + up].
fn ground_at<V: VoxelQuery + ?Sized>(v: &V, p: Vec3, up: f32, down: f32) -> Option<f32> {
    if !p.is_finite() {
        return None;
    }
    let top = world_to_voxel(p + Vec3::Y * up.clamp(0.0, 64.0));
    let bottom = world_to_voxel(p - Vec3::Y * down.clamp(0.0, 64.0));
    let mut y = top.y;
    while y >= bottom.y {
        let cell = IVec3::new(top.x, y, top.z);
        if v.is_solid(cell) && !v.is_solid(cell + IVec3::Y) {
            return Some((y + 1) as f32 * VOXEL_SIZE);
        }
        y -= 1;
    }
    None
}

/// One titan tick from the pilot's input. Pure: the host and prediction call it with the same
/// state, input, terrain and params.
pub fn step<V: VoxelQuery + ?Sized>(s: &mut PlayerState, input: &PlayerInput, v: &V, p: &TitanParams, dt: f32) -> TitanEvents {
    let mut ev = TitanEvents::default();
    if !(dt > 0.0 && dt.is_finite()) || !s.pos.is_finite() {
        return ev;
    }
    if !s.vel.is_finite() {
        s.vel = Vec3::ZERO;
    }
    let typing = input.held(buttons::TYPING);
    let held = if typing { 0 } else { input.buttons };
    let pressed = held & !s.prev_buttons;
    s.prev_buttons = held;
    let yaw = if input.yaw.is_finite() { input.yaw } else { 0.0 };
    let (sy, cy) = yaw.sin_cos();
    let fwd = Vec3::new(-sy, 0.0, -cy);
    let right = Vec3::new(cy, 0.0, -sy);
    let (mx, mz) = if typing { (0.0, 0.0) } else { (input.move_x.clamp(-1, 1) as f32, input.move_z.clamp(-1, 1) as f32) };
    let wish = (fwd * mz + right * mx).normalize_or_zero();
    // Dash charges come back one at a time.
    s.gas = if s.gas.is_finite() { (s.gas + dt / p.recharge).min(p.charges) } else { p.charges };
    if pressed & buttons::DODGE != 0 && s.gas >= 1.0 && s.burst_cooldown <= 0.0 {
        s.gas -= 1.0;
        s.burst_cooldown = p.dash_secs;
        let d = if wish != Vec3::ZERO { wish } else { fwd };
        s.vel.x = d.x * p.dash_speed;
        s.vel.z = d.z * p.dash_speed;
        ev.dashed = true;
    }
    let mut h = Vec3::new(s.vel.x, 0.0, s.vel.z);
    if s.burst_cooldown > 0.0 {
        // The dash carries (a heavy burst), then hands back to the walk.
        s.burst_cooldown = (s.burst_cooldown - dt).max(0.0);
    } else {
        // Linear to the wish: full speed in `accel_time`, crisp stops, strafing, turns on a dime.
        let want = wish * p.speed;
        let a = if want == Vec3::ZERO || want.dot(h) < 0.0 { p.decel } else { p.accel } * if s.grounded { 1.0 } else { 0.35 };
        let dv = want - h;
        let max = a * dt;
        h += if dv.length() > max { dv.normalize_or_zero() * max } else { dv };
    }
    s.vel.x = h.x;
    s.vel.z = h.z;
    if pressed & buttons::JUMP != 0 && s.grounded {
        s.vel.y = p.jump;
        s.grounded = false;
        ev.jumped = true;
    }
    // Gravity + ground following with step-up (terrain only: the body is hard light), on the feet.
    let lift = Vec3::Y * p.lift;
    s.pos -= lift;
    let was_grounded = s.grounded;
    s.vel.y -= p.gravity * dt;
    s.vel.y = s.vel.y.max(-60.0);
    let next = s.pos + s.vel * dt;
    match ground_at(v, next, p.step_up, (s.vel.y.abs() * dt + 0.05).max(0.05)) {
        Some(gy) if gy >= next.y - 0.05 && s.vel.y <= 0.5 => {
            if !was_grounded {
                ev.landed = Some((-s.vel.y).max(0.0));
            }
            s.pos = Vec3::new(next.x, gy, next.z);
            s.vel.y = 0.0;
            s.grounded = true;
        }
        _ => {
            let wall = ground_at(v, next, 30.0, 0.0).is_some_and(|gy| gy > s.pos.y + p.step_up);
            if wall {
                s.vel.x = 0.0;
                s.vel.z = 0.0;
                s.pos.y = next.y.max(s.pos.y);
            } else {
                s.pos = next;
            }
            // Walking off an edge: a step down within the step height keeps it on the ground.
            match ground_at(v, s.pos, 0.05, p.step_up).filter(|_| was_grounded && s.vel.y <= 0.0) {
                Some(gy) => {
                    s.pos.y = gy;
                    s.vel.y = 0.0;
                    s.grounded = true;
                }
                None => s.grounded = false,
            }
        }
    }
    s.pos += lift;
    s.air_time = if s.grounded { 0.0 } else { s.air_time + dt };
    ev
}

/// Torso yaw after `dt`: swings toward the aim with a critically-damped lag (`lag` s).
pub fn torso_yaw(cur: f32, aim: f32, lag: f32, dt: f32) -> f32 {
    let mut d = aim - cur;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    cur + d * (1.0 - (-dt / lag.max(1e-3)).exp())
}
