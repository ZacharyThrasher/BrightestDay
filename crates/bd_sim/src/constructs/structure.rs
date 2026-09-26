//! **Structures** (COMBAT.md §4 Structures) — construct engineer.
//!
//! Placed at the aim, facing the caster, *hard light*: they don't fall on their own (a pillar over
//! the void is a lifeline) — until something knocks them loose (a big ram, a blast, a Blade slice,
//! thrusters), then they fall and topple as physical bodies ([`super::body`]) and crush what they
//! land on. Mechanics layer on top through the generic components: Flies (a flying wall hunts you
//! as a moving barrier), Homing (a homing trampoline slides under an enemy), Charge (a charging
//! wall bulldozes), Walks (a wall with legs — the Walker controller), Spin (a spinning pillar
//! sweeps like a lighthouse), Trap (the Cage snaps shut; a flying cage carries its victim off).
//!
//! * Wall — blocks everything; Ramp — Launch, a surfable slope (movement treats the tilted OBB as a
//!   ramp); Pillar — a fresh tether anchor anywhere; Dome — shield bubble that follows its caster
//!   for 3 s (stops enemy projectiles: `segment_query`); Cage — Trap; Platform — floating ledge;
//!   Trampoline — Bounce (restitution 1.1, cap 35 m/s).
//! * [`post_move`]: what players feel when they touch structure surfaces after moving —
//!   trampolines / Bounce tops, Bounce side-walls (a bouncy wall rebounds players), Launch tops
//!   and fronts, and Homing trampolines launching whoever stands on them.

use bd_core::input::PlayerInput;
use bd_core::mechanics::{Form, FormCategory, Mechanic};
use bd_core::{PlayerId, Vec3};

use super::components::{self, eff_has};
use super::{AnimState, Construct, ConstructOut, ConstructStore, Env, Loco};
use crate::movement::{MoveEvents, MoveParams, PlayerState};

/// Trampoline bounce cap (m/s) and restitution (COMBAT.md §4).
pub const TRAMPOLINE_CAP: f32 = 35.0;
pub const TRAMPOLINE_RESTITUTION: f32 = 1.1;
/// A loose structure that has settled for this long becomes static again (a fallen wall lies there).
const SETTLE_SECS: f32 = 1.5;

/// [`super::StepFn`] for [`super::Controller::Structure`].
pub fn step(c: &mut Construct, env: &Env<'_>, _out: &mut ConstructOut) {
    if !c.active() {
        c.body.loco = Loco::Static;
        return;
    }
    if c.recipe.form == Form::Dome && !c.body.loose && !eff_has(c, Mechanic::Flies) {
        // A shield bubble around its caster.
        c.body.loco = Loco::Kinematic;
        c.vel = env.owner.vel;
        c.pos = env.owner.pos;
        return;
    }
    if c.body.loose {
        c.body.loco = Loco::Ballistic;
        if c.body.supported && c.vel.length_squared() < 0.2 && c.body.spin == Vec3::ZERO {
            c.comp.rest_t += env.dt();
            if c.comp.rest_t > SETTLE_SECS && c.parent.is_none() {
                c.body.loose = false;
                c.body.loco = Loco::Static;
                c.vel = Vec3::ZERO;
            }
        } else {
            c.comp.rest_t = 0.0;
        }
        c.set_anim(AnimState::Move, env.tick());
        return;
    }
    // Placed and still: steering components may take over (Flies / Homing / Charge / Thrust).
    if matches!(c.body.loco, Loco::Kinematic | Loco::Ballistic) && c.comp.attached_to.is_none() && !c.body.loose {
        c.body.loco = Loco::Static;
    }
    if matches!(c.body.loco, Loco::Walk | Loco::Fly) && !eff_has(c, Mechanic::Flies) && !eff_has(c, Mechanic::Homing) && !eff_has(c, Mechanic::Charge) {
        c.body.loco = Loco::Static;
    }
    if c.body.loco == Loco::Static {
        c.vel = Vec3::ZERO;
    }
    if c.anim_state != AnimState::Active && c.anim_state != AnimState::Telegraph {
        c.set_anim(AnimState::Idle, env.tick());
    }
}

/// Are players blocked by this object (big / resting objects become solid)?
pub fn object_is_solid(c: &Construct) -> bool {
    c.forming_t <= 0.0
        && c.category == FormCategory::Object
        && c.half.min_element() >= 0.45
        && c.stuck_to.is_none()
        && (c.body.supported || c.vel.length_squared() < 4.0)
}

/// Surfaces players touch after moving: trampolines, launch pads, bouncy walls.
pub fn post_move(store: &ConstructStore, pid: PlayerId, input: &PlayerInput, st: &mut PlayerState, pre_vel: Vec3, ev: &MoveEvents, p: &MoveParams) {
    let _ = input;
    let feet = st.pos;
    let hw = p.hull_half_width;
    for c in store.iter() {
        if !c.active() || c.attached() || c.category == FormCategory::Held {
            continue;
        }
        let bounce = eff_has(c, Mechanic::Bounce);
        // SURF: a gliding (surf) ramp is ridden, never a launch pad.
        let launch = eff_has(c, Mechanic::Launch) && !(c.recipe.form == bd_core::mechanics::Form::Ramp && eff_has(c, Mechanic::Glide));
        if !bounce && !launch {
            continue;
        }
        let b = c.bounds();
        let over = feet.x + hw > b.min.x && feet.x - hw < b.max.x && feet.z + hw > b.min.z && feet.z - hw < b.max.z;
        let on_top = over && (feet.y - b.max.y).abs() <= 0.35;
        if on_top && (ev.landed.is_some() || st.grounded) {
            if bounce && pre_vel.y < -2.0 {
                let up = (-pre_vel.y * TRAMPOLINE_RESTITUTION).clamp(12.0, TRAMPOLINE_CAP);
                st.apply_impulse(Vec3::new(0.0, up - st.vel.y.max(0.0), 0.0), 0.0);
                continue;
            }
            if bounce && eff_has(c, Mechanic::Homing) && c.owner != pid && st.vel.y <= 0.5 {
                // A homing trampoline that slid under you launches you.
                st.apply_impulse(Vec3::new(0.0, 22.0 - st.vel.y.max(0.0), 0.0), 0.0);
                continue;
            }
            if launch {
                let fwd = c.forward();
                let fwd = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::NEG_Z);
                st.apply_impulse(fwd * 14.0 + Vec3::Y * 12.0, 0.0);
                continue;
            }
        }
        // Side contact: a bouncy wall rebounds you; a launch front flings you.
        if let Some(w) = ev.wall_hit {
            if w.speed < 2.0 {
                continue;
            }
            let hull_c = feet + Vec3::Y * 0.9;
            let near = crate::geom::dist_sq_point_aabb(&crate::geom::inflate(&b, hw + 0.35), hull_c) <= 0.0 && crate::geom::sphere_obb(hull_c - w.normal * (hw + 0.2), 0.4, c.center(), c.half, c.rot);
            if !near {
                continue;
            }
            if bounce {
                let v = (w.speed * TRAMPOLINE_RESTITUTION).clamp(10.0, TRAMPOLINE_CAP);
                st.apply_impulse(w.normal * v + Vec3::Y * 3.0, 0.0);
            } else if launch {
                st.apply_impulse(c.forward() * 14.0 + Vec3::Y * 12.0, 0.0);
            }
        }
    }
}

/// Loosen `c` with a spin (used by counters / chemistry).
pub fn knock_loose(c: &mut Construct, spin: Vec3) {
    components::loosen(c, spin);
}
