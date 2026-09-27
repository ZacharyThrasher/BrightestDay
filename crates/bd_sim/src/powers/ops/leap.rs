//! `Leap { .. }` — a ballistic jump that LANDS ON the point under the crosshair (docs/toolbox/OPS.md):
//! Hanuman's leap across the ocean, Cú Chulainn's salmon leap, a Hulk jump.
//!
//! Not an impulse along the look (that's `Impulse`): the op solves the arc. The landing point is
//! what the crosshair ray hits within `range` (a wall's foot is the ground below the hit), else the
//! ground under the farthest point of the ray. The apex is `apex` m above the higher end, and the
//! launch velocity is exactly the one that falls onto the point under the caster's gravity — so
//! the arc reads from anywhere on the map and a following `SlamOnLand` (compose it in the same move)
//! lands exactly where you aimed. You keep full air control (steer off it if you like); the flight
//! is plain movement state, so client prediction replays it like any impulse.
//!
//! Deterministic, allocation-free; stateless (the arc lives in the player's velocity).

use bd_core::{PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use super::OpEvent;
use crate::movement::{ColliderId, CollisionWorld, raycast_world};
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_RANGE: f32 = 90.0;
/// Horizontal launch speed cap (m/s): a longer leap just flies higher.
pub const MAX_FLAT_SPEED: f32 = 48.0;
pub const MAX_APEX: f32 = 30.0;

/// Presentation: a leap started (`from` → `to`, `secs` of flight, `apex` height) — the landing
/// marker everyone can read.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeapArc {
    pub player: PlayerId,
    pub from: Vec3,
    pub to: Vec3,
    pub secs: f32,
    pub apex: f32,
}

/// Launch velocity from `from` onto `to` peaking `apex` m above the higher end under gravity `g`
/// (m/s², > 0). Returns (velocity, flight seconds). The flat speed is capped at
/// [`MAX_FLAT_SPEED`] by raising the arc.
pub fn solve_arc(from: Vec3, to: Vec3, apex: f32, g: f32) -> (Vec3, f32) {
    let g = if g.is_finite() && g > 0.1 { g } else { 25.0 };
    let flat = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    let d = flat.length();
    let mut top = from.y.max(to.y) + apex.clamp(0.5, MAX_APEX);
    let mut out = (Vec3::ZERO, 0.0);
    for _ in 0..4 {
        let vy = (2.0 * g * (top - from.y)).max(0.0).sqrt();
        let t = vy / g + (2.0 * (top - to.y).max(0.0) / g).sqrt();
        let t = t.max(0.05);
        let vh = d / t;
        out = (Vec3::new(0.0, vy, 0.0) + flat.normalize_or_zero() * vh.min(MAX_FLAT_SPEED), t);
        if vh <= MAX_FLAT_SPEED {
            break;
        }
        // Too flat for the cap: a higher arc buys more air time.
        top += (vh / MAX_FLAT_SPEED).powi(2) * 3.0;
    }
    out
}

impl World {
    pub(crate) fn op_leap(&mut self, i: usize, range: f32, apex: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let range = if range.is_finite() && range > 0.0 { range.clamp(3.0, MAX_RANGE) } else { 40.0 };
        let apex = if apex.is_finite() && apex > 0.0 { apex.clamp(0.5, MAX_APEX) } else { 6.0 };
        let (eye, look) = self.op_eye_look(i);
        let base = self.cfg.move_params.clone();
        let params = self.players[i].params(&base);
        let from = self.players[i].mv.pos;
        let land = {
            let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(ColliderId::Player(id));
            let (probe, wall) = match raycast_world(&cw, eye, look, range, false) {
                // A floor: land on it. A wall / ceiling: the ground in front of it.
                Some(h) if h.normal.y > 0.5 => (h.point + Vec3::Y * 0.05, false),
                Some(h) => (h.point + h.normal * 0.6, true),
                None => (eye + look * range, true),
            };
            if wall {
                raycast_world(&cw, probe, Vec3::NEG_Y, 60.0, false).map(|h| h.point + Vec3::Y * 0.05)
            } else {
                Some(probe)
            }
        };
        // Nothing under the far end (the void): refuse rather than throw the caster off the map.
        let Some(to) = land.filter(|p| self.in_play(*p, 0.0)) else { return Err(PowerError::NoTarget) };
        if (to - from).length() < 1.0 {
            return Err(PowerError::NoTarget);
        }
        let g = params.gravity * self.players[i].mv.mods.gravity_scale.clamp(0.05, 4.0);
        let (vel, secs) = solve_arc(from, to, apex, g);
        let p = &mut self.players[i];
        p.mv.release_tethers();
        p.mv.extra.cancel_dodge(&mut p.mv.vel);
        p.mv.vel = vel;
        p.mv.grounded = false;
        p.mv.tumble = 0.0;
        self.op_event(OpEvent::Leap(LeapArc { player: id, from, to, secs, apex }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_lands_on_the_point() {
        let g = 25.0;
        for (from, to) in [
            (Vec3::ZERO, Vec3::new(30.0, 0.0, 0.0)),
            (Vec3::ZERO, Vec3::new(0.0, 8.0, -20.0)),
            (Vec3::new(0.0, 10.0, 0.0), Vec3::new(12.0, 0.0, 12.0)),
        ] {
            let (v, t) = solve_arc(from, to, 5.0, g);
            let end = from + v * t + Vec3::Y * (-0.5 * g * t * t);
            assert!((end - to).length() < 0.05, "{from} -> {to}: {end}");
        }
    }

    #[test]
    fn long_leaps_fly_higher_not_faster() {
        let (v, _) = solve_arc(Vec3::ZERO, Vec3::new(90.0, 0.0, 0.0), 3.0, 25.0);
        assert!(Vec3::new(v.x, 0.0, v.z).length() <= MAX_FLAT_SPEED + 1e-3);
    }
}
