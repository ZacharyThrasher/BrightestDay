//! `Evade` — a perfect-dodge window (docs/toolbox/OPS.md). For `secs` (≤ 0.6) the first dodgeable
//! hit that would land is slipped: the normal dodge (its i-frames and sidestep) starts off the hit's
//! line, and the reward is `slow` s of time slow (the TimeStop rule: YOU move `speed`× while the
//! world greys; nobody freezes) plus, with `blink` > 0, a reposition `blink` m behind the attacker.
//! Witch Time, Mikiri, a Chaos-Control counter, a quickstep. Whiffing the window costs the move.

use bd_core::{MAX_PLAYERS, PlayerId, Vec3};

use super::OpEvent;
use crate::combat::HitInfo;
use crate::events::GameEvent;
use crate::powers::{PowerError, PowerEvent};
use crate::world::World;

pub const MAX_SECS: f32 = 0.6;
pub const MAX_SLOW: f32 = 3.0;

/// An open window, then (after a dodge) the slow it granted (`OpState::evades`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EvadeWindow {
    pub owner: PlayerId,
    /// Window time left (0 = closed: only the slow runs).
    pub left: f32,
    pub slow: f32,
    pub speed: f32,
    pub blink: f32,
    /// Seconds of granted slow left (the power step times it for power holders; this does it for
    /// a borrowed evade by a player who holds no power).
    pub slowing: f32,
}

impl World {
    pub(crate) fn op_evade(&mut self, i: usize, secs: f32, slow: f32, speed: f32, blink: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let secs = if secs.is_finite() { secs.clamp(0.05, MAX_SECS) } else { 0.3 };
        let slow = if slow.is_finite() { slow.clamp(0.0, MAX_SLOW) } else { 0.0 };
        let speed = if speed.is_finite() && speed > 0.0 { speed.clamp(1.0, 3.0) } else { 2.0 };
        let blink = if blink.is_finite() { blink.clamp(0.0, 4.0) } else { 0.0 };
        self.powers.ops.evades[i] = Some(EvadeWindow { owner: id, left: secs, slow, speed, blink, slowing: 0.0 });
        let pos = self.players[i].hull(&self.cfg.move_params).center();
        self.op_event(OpEvent::Evade { player: id, dodged: false, attacker: None, pos, secs });
        Ok(())
    }

    pub(crate) fn step_evades(&mut self, dt: f32) {
        for i in 0..MAX_PLAYERS.min(self.players.len()) {
            let Some(mut w) = self.powers.ops.evades[i] else { continue };
            let p = &mut self.players[i];
            if p.id != w.owner || !p.alive {
                self.powers.ops.evades[i] = None;
                continue;
            }
            w.left = (w.left - dt).max(0.0);
            if w.slowing > 0.0 {
                w.slowing = (w.slowing - dt).max(0.0);
                if p.power.id.is_none() {
                    p.power.time_stop = w.slowing;
                }
            }
            self.powers.ops.evades[i] = (w.left > 0.0 || w.slowing > 0.0).then_some(w);
        }
    }

    /// Target side of a hit: an open window slips it. `true` = the hit is cancelled.
    pub(crate) fn evade_on_hit(&mut self, ti: usize, hit: &HitInfo) -> bool {
        let Some(mut w) = self.powers.ops.evades[ti] else { return false };
        let p = &self.players[ti];
        if w.owner != p.id || w.left <= 0.0 || !p.alive {
            return false;
        }
        if !crate::dodge::ignores(hit.kind) || hit.source == Some(hit.target) || (hit.damage <= 0.0 && hit.knockback <= 0.0) {
            return false;
        }
        let dodge_time = self.cfg.move_params.dodge_time;
        if !dodge_time.is_finite() || dodge_time <= 0.0 {
            return false;
        }
        let id = p.id;
        let from = p.mv.pos;
        // Sideways off the hit's line and a little back (the Ultra Instinct sidestep).
        let incoming = Vec3::new(hit.impulse_dir.x, 0.0, hit.impulse_dir.z).normalize_or(-p.look());
        let side = Vec3::Y.cross(incoming).normalize_or(Vec3::X) * if self.tick % 2 == 0 { 1.0 } else { -1.0 };
        let dir = (side * 0.85 + incoming * 0.35).normalize_or(side);
        let attacker = hit.source.and_then(|s| self.pidx(s));
        {
            let p = &mut self.players[ti];
            let x = &mut p.mv.extra;
            x.dodge_dir = dir;
            x.dodge_base = Vec3::new(p.mv.vel.x, 0.0, p.mv.vel.z);
            x.dodge_left = dodge_time;
            x.since_dodge = 0.0;
        }
        // Behind them: where they can't see you (only onto open ground).
        if w.blink > 0.0
            && let Some(a) = attacker
        {
            let q = &self.players[a];
            let back = Vec3::new(q.look().x, 0.0, q.look().z).normalize_or(Vec3::NEG_Z);
            let want = q.mv.pos - back * (w.blink + 0.9);
            if self.op_clear_feet(ti, want) {
                let p = &mut self.players[ti];
                p.mv.pos = want;
                p.mv.grounded = false;
                self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: id, from, to: want, afterimage: true }));
            }
        }
        if w.slow > 0.0 {
            let p = &mut self.players[ti];
            p.power.time_stop = p.power.time_stop.max(w.slow);
            p.power.time_speed = w.speed;
            w.slowing = w.slow;
            self.inject_event(GameEvent::Power(PowerEvent::TimeStop { player: id, secs: w.slow }));
        }
        w.left = 0.0;
        self.powers.ops.evades[ti] = (w.slowing > 0.0).then_some(w);
        let pos = self.players[ti].hull(&self.cfg.move_params).center();
        self.op_event(OpEvent::Evade { player: id, dodged: true, attacker: hit.source, pos, secs: w.slow });
        true
    }

    /// Is a player hull of `i`'s size clear of terrain with its feet at `feet`, and in play?
    pub(crate) fn op_clear_feet(&self, i: usize, feet: Vec3) -> bool {
        if !feet.is_finite() || !self.in_play(feet, 0.0) {
            return false;
        }
        let half = self.players[i].hull(&self.cfg.move_params).half();
        let lo = feet + Vec3::new(-half.x + 0.08, 0.08, -half.z + 0.08);
        let hi = feet + Vec3::new(half.x - 0.08, half.y * 2.0 - 0.08, half.z - 0.08);
        let (a, b) = (bd_core::voxel::world_to_voxel(lo), bd_core::voxel::world_to_voxel(hi));
        for x in a.x..=b.x {
            for y in a.y..=b.y {
                for z in a.z..=b.z {
                    if self.voxels.get(bd_core::IVec3::new(x, y, z)) != 0 {
                        return false;
                    }
                }
            }
        }
        true
    }
}
