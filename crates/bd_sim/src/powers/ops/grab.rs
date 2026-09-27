//! `Grab { .. }` — the command grab: seize the enemy right in front of you (`range` m) — it
//! ignores guards, counters and deflect stances (a grab beats a block; that is the read) but not a
//! dodge (a jump / dodge beats a grab) — hold them a short beat (`hold` ≤ [`MAX_HOLD`] s: never a
//! lock) and throw them along your look (`damage`, `knockback`, `up`; `back` = over your shoulder).
//! `lift` launches you both upward on the seize (Zangief's Spinning Piledriver leap, a suplex), and
//! the held body swings with your camera while you hold (King's Giant Swing — turn to aim the
//! throw). Nobody in reach = a whiff: the move is spent (a whiffed grab is punishable).

use bd_core::{PlayerId, Vec3};

use super::{GrabPhase, OpEvent, flat};
use crate::combat::{HitInfo, HitKind};
use crate::powers::PowerError;
use crate::world::World;

/// Longest hold (s): owner rule, control is never taken away for long.
pub const MAX_HOLD: f32 = 0.45;

pub fn hold_secs(hold: f32) -> f32 {
    if hold.is_finite() { super::fclamp(hold, 0.05, MAX_HOLD) } else { 0.2 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GrabSpec {
    pub range: f32,
    pub hold: f32,
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub lift: f32,
    pub back: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hold {
    pub owner: PlayerId,
    target: PlayerId,
    spec: GrabSpec,
    t: f32,
}

impl World {
    pub(crate) fn op_grab(&mut self, i: usize, spec: GrabSpec) -> Result<(), PowerError> {
        let id = self.players[i].id;
        if self.powers.ops.grabs[i].is_some_and(|g| g.owner == id) {
            return Err(PowerError::Busy);
        }
        let spec = GrabSpec {
            range: super::fclamp(spec.range, 0.8, 5.0),
            hold: hold_secs(spec.hold),
            damage: super::fclamp(spec.damage, 0.0, 60.0),
            knockback: super::fclamp(spec.knockback, 0.0, 80.0),
            up: super::fclamp(spec.up, -1.0, 2.0),
            lift: super::fclamp(spec.lift, 0.0, 20.0),
            ..spec
        };
        let base = self.cfg.move_params.clone();
        let (_, look) = self.op_eye_look(i);
        let fwd = flat(look, Vec3::NEG_Z);
        let me = self.players[i].hull(&base).center();
        // The nearest enemy in reach, in front (a generous 60° half-cone), not dodging through it.
        let mut best: Option<(usize, f32)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive || q.shielded() || q.seat.is_some_and(|s| !s.suit) {
                continue;
            }
            let h = q.hull(&base);
            let c = crate::geom::closest_point(&h, me);
            let d = c - me;
            let len = d.length();
            let to = (h.center() - me).normalize_or(fwd);
            if len > spec.range || flat(to, fwd).dot(fwd) < 0.5 {
                continue;
            }
            let probe = HitInfo::new(q.id, Some(id), HitKind::Melee, 0.0, 0.0, fwd, h.center());
            if crate::dodge::intangible(self, k, &probe) {
                continue;
            }
            if best.is_none_or(|b| len < b.1) {
                best = Some((k, len));
            }
        }
        let Some((k, _)) = best else {
            self.op_event(OpEvent::Grab { player: id, phase: GrabPhase::Whiff, target: None, pos: me + fwd, dir: fwd });
            return Ok(());
        };
        let tid = self.players[k].id;
        // Seized: interrupts (oath, charge, their own ram / lock-on), no push.
        self.op_touch(i, k, fwd);
        self.break_lock_on(k);
        if self.powers.ops.rams[k].is_some() {
            self.end_ram(k, super::RamPhase::End);
        }
        let lift = spec.lift;
        for n in [i, k] {
            let mv = &mut self.players[n].mv;
            mv.release_tethers();
            mv.extra.cancel_dodge(&mut mv.vel);
            mv.vel = Vec3::new(0.0, if lift > 0.0 { lift } else { mv.vel.y.min(0.0) }, 0.0);
            if lift > 0.0 {
                mv.grounded = false;
            }
        }
        self.powers.ops.grabs[i] = Some(Hold { owner: id, target: tid, spec, t: 0.0 });
        let pos = self.players[k].hull(&base).center();
        self.op_event(OpEvent::Grab { player: id, phase: GrabPhase::Seize, target: Some(tid), pos, dir: fwd });
        Ok(())
    }

    pub(crate) fn step_grabs(&mut self, dt: f32) {
        let base = self.cfg.move_params.clone();
        for i in 0..self.powers.ops.grabs.len() {
            let Some(mut g) = self.powers.ops.grabs[i] else { continue };
            let ok = self.players.get(i).is_some_and(|p| p.id == g.owner && p.alive);
            let k = self.pidx(g.target).filter(|&k| self.players[k].alive);
            let (true, Some(k)) = (ok, k) else {
                self.powers.ops.grabs[i] = None;
                continue;
            };
            g.t += dt;
            let look = self.players[i].look();
            let fwd = flat(look, Vec3::NEG_Z);
            let (pos, vel) = (self.players[i].mv.pos, self.players[i].mv.vel);
            if g.t >= g.spec.hold {
                self.powers.ops.grabs[i] = None;
                let dir = if g.spec.back { -fwd } else { fwd };
                let v = &mut self.players[k];
                let vc = v.hull(&base).center();
                let hit = HitInfo::new(g.target, Some(g.owner), HitKind::HeavyMelee, g.spec.damage, g.spec.knockback, dir + Vec3::Y * g.spec.up, vc).with_impact(2.0);
                self.powers.ops.unblockable = true;
                self.apply_hit(hit);
                self.powers.ops.unblockable = false;
                self.op_event(OpEvent::Grab { player: g.owner, phase: GrabPhase::Throw, target: Some(g.target), pos: vc, dir });
                continue;
            }
            // Held in front (it swings with your camera: aim the throw).
            let v = &mut self.players[k];
            v.mv.pos = pos + fwd * 0.95 + Vec3::Y * 0.35;
            v.mv.vel = vel;
            v.mv.grounded = false;
            v.mv.tumble = v.mv.tumble.max(0.1);
            self.powers.ops.grabs[i] = Some(g);
        }
    }

    /// Player `i` was hit hard: whatever they are holding drops.
    pub(crate) fn break_grab(&mut self, i: usize) {
        let owner = self.players.get(i).map(|p| p.id);
        if let Some(g) = self.powers.ops.grabs.get_mut(i).filter(|g| g.is_some_and(|g| Some(g.owner) == owner)).and_then(|g| g.take()) {
            let pos = self.players.get(i).map_or(Vec3::ZERO, |p| p.mv.pos);
            self.op_event(OpEvent::Grab { player: g.owner, phase: GrabPhase::Whiff, target: Some(g.target), pos, dir: Vec3::ZERO });
        }
    }
}
