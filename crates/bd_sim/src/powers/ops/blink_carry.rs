//! `MoveOp::BlinkCarry` — teleport along the aim TAKING whoever you're touching (docs/toolbox/OPS.md).
//!
//! Nightcrawler's "bamf and drop": the nearest enemy whose body is within `grab` m of yours (the one
//! in front first) goes with you. You both reappear `dist` m along the aim (walls stop it, like
//! `Blink`) and `up` m higher (ceilings stop that); they are set down beside you with no footing — a
//! fall and a short tumble — while you hang a beat (a Float) to blink away, strike or drop on them.
//! Taken over the void, that's a KO; taken up a cliff, a spike. Nobody in reach = a plain blink
//! (with the same lift and hang). Both ends of both jumps emit `PowerEvent::Blinked` (the smoke
//! puffs). A zero-damage credit hit keeps combos and KO credit honest. Deterministic,
//! allocation-free; does nothing unless a move uses it.

use bd_core::Vec3;

use super::super::{PowerError, PowerEvent};
use crate::combat::{HitInfo, HitKind};
use crate::events::GameEvent;
use crate::geom;
use crate::movement::{ColliderId, CollisionWorld, raycast_world};
use crate::status::StatusKind;
use crate::world::World;

/// Seconds the passenger tumbles after the drop (no air control: they FALL).
pub const CARRY_DROP_TUMBLE: f32 = 0.45;
/// Seconds you hang at the top (gravity ×0.3).
pub const CARRY_HANG_SECS: f32 = 0.4;
/// Widest reach that counts as "touching" (m).
pub const CARRY_MAX_GRAB: f32 = 4.0;

impl World {
    pub(crate) fn op_blink_carry(&mut self, i: usize, dist: f32, up: f32, grab: f32) -> Result<(), PowerError> {
        let (_, look) = self.op_eye_look(i);
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let hull = self.players[i].hull(&base);
        let me = hull.center();
        let fl = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
        let grab = if grab.is_finite() { grab.clamp(0.0, CARRY_MAX_GRAB) } else { 0.0 };
        // The passenger: the nearest enemy within `grab` of your body (in front first).
        let mut pass: Option<(usize, f32)> = None;
        if grab > 0.0 {
            for (k, q) in self.players.iter().enumerate() {
                if k == i || !q.alive || q.seat.is_some() || q.shielded() {
                    continue;
                }
                let qh = q.hull(&base);
                let gap = (geom::closest_point(&qh, me) - geom::closest_point(&hull, qh.center())).length();
                if gap > grab {
                    continue;
                }
                let qc = qh.center();
                let front = Vec3::new(qc.x - me.x, 0.0, qc.z - me.z).normalize_or_zero().dot(fl);
                let score = gap - front * 0.75;
                if pass.is_none_or(|b| score < b.1) {
                    pass = Some((k, score));
                }
            }
        }
        // Destination: Blink's (stops before walls), then up to `up` m higher (stops under ceilings).
        let from = self.players[i].mv.pos;
        let dir = Vec3::new(look.x, look.y * 0.5, look.z).normalize_or(fl);
        let want = if dist.is_finite() { dist.clamp(0.0, 40.0) } else { 0.0 };
        let h = hull.half().y;
        let lift_want = if up.is_finite() { up.clamp(0.0, 30.0) } else { 0.0 };
        let (to, lift) = {
            let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(ColliderId::Player(id));
            let mut d = want;
            if want > 0.0 {
                for off in [Vec3::ZERO, Vec3::Y * (h - 0.15), Vec3::Y * -(h - 0.15)] {
                    if let Some(hit) = raycast_world(&cw, me + off, dir, want + 0.5, false) {
                        d = d.min((hit.t - 0.5).max(0.0));
                    }
                }
            }
            let flat_to = from + dir * d;
            let mut lift = 0.0;
            if lift_want > 0.0 {
                // Headroom for two bodies stacked is not needed (they stand side by side): one body.
                let head = flat_to + Vec3::Y * (h * 2.0 - 0.05);
                lift = raycast_world(&cw, head, Vec3::Y, lift_want + 0.4, false).map_or(lift_want, |hit| (hit.t - 0.4).max(0.0));
            }
            (flat_to + Vec3::Y * lift, lift)
        };
        if ((to - from).length() < 0.25 && pass.is_none()) || !self.in_play(to, 0.0) {
            return Err(PowerError::NoTarget);
        }
        // Where the passenger lands: beside you (front, back, right, left), in open air.
        let mut drop = None;
        if let Some((k, _)) = pass {
            let qh = self.players[k].hull(&base);
            let qhalf = qh.half();
            let right = Vec3::new(-fl.z, 0.0, fl.x);
            let side = hull.half().x + qhalf.x + 0.35;
            for off in [fl * side, -fl * side, right * side, -right * side] {
                let feet = to + off;
                let free = [0.3, qhalf.y, qhalf.y * 2.0 - 0.2].iter().all(|y| self.voxels.get_at(feet + Vec3::Y * *y) == 0);
                if free && self.in_play(feet, 0.0) {
                    drop = Some((k, feet));
                    break;
                }
            }
        }
        // Go.
        {
            let p = &mut self.players[i];
            p.mv.pos = to;
            p.mv.grounded = false;
            if lift > 0.0 || drop.is_some() {
                p.mv.vel = Vec3::new(p.mv.vel.x * 0.2, 0.0, p.mv.vel.z * 0.2);
            }
        }
        self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: id, from, to, afterimage: true }));
        if lift > 0.0 || drop.is_some() {
            self.apply_status(i, StatusKind::Float, CARRY_HANG_SECS, Some(id));
        }
        if let Some((k, feet)) = drop {
            let q = &mut self.players[k];
            let qfrom = q.mv.pos;
            q.mv.release_tethers();
            q.mv.extra.cancel_dodge(&mut q.mv.vel);
            q.mv.pos = feet;
            q.mv.vel = Vec3::new(0.0, -1.0, 0.0);
            q.mv.grounded = false;
            q.mv.tumble = q.mv.tumble.max(CARRY_DROP_TUMBLE);
            let tid = q.id;
            let c = q.hull(&base).center();
            self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: tid, from: qfrom, to: feet, afterimage: true }));
            // A zero-damage hit: credit (the drop can KO) + interrupts, no knockback.
            self.apply_hit(HitInfo::new(tid, Some(id), HitKind::RingShove, 0.0, 0.0, Vec3::NEG_Y, c));
        }
        Ok(())
    }
}
