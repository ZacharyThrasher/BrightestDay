//! `MoveOp::DelayedHit` — the delayed-death verb (docs/toolbox/OPS.md).
//!
//! A visible mark is planted on the aimed enemy (`range`, aim-forgiving `cone`) — or, with
//! `radius` > 0, on every enemy within `radius` m of you (a wave that passes through a crowd) — and
//! after `secs` it bursts on each of them wherever they are: `damage`, `knockback` (away from where
//! you stood) with an `up` share, and the `element`'s status. The five-point palm, a dusting wave, a
//! curse with a countdown. The victim sees it coming (`OpEvent::Marked`); a dodge's i-frames at the
//! burst still pass through it (the ordinary dodge rule of `apply_hit`). Never an instant KO: the
//! burst is a hit like any other (clamped). One mark per (owner, target): re-marking refreshes it.
//! Deterministic; a fixed table in `OpState::marks` (allocation-free); nothing steps while empty.

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, PlayerId, Vec3};

use super::OpEvent;
use super::afflict::fin_range;
use super::super::PowerError;
use crate::combat::{HitInfo, HitKind};
use crate::world::World;

/// Most live marks in the world (the one closest to bursting goes off early beyond it).
pub const MAX_MARKS: usize = 24;

/// A planted mark (`OpState::marks`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelayedMark {
    pub owner: PlayerId,
    pub target: PlayerId,
    /// Seconds left.
    pub t: f32,
    /// Where the owner stood (the burst pushes away from it).
    pub from: Vec3,
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub element: Element,
}

/// `MoveOp::DelayedHit`'s numbers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DelayedHitArgs {
    pub range: f32,
    pub cone: f32,
    pub radius: f32,
    pub secs: f32,
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub element: Element,
}

/// Longest countdown (s).
pub const MAX_DELAY: f32 = 8.0;

impl World {
    pub(crate) fn op_delayed_hit(&mut self, i: usize, a: DelayedHitArgs) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let me = self.players[i].hull(&base).center();
        let fin = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
        let secs = fin(a.secs, 0.2, MAX_DELAY);
        let mut targets = [None; MAX_PLAYERS];
        let mut n = 0;
        let radius = fin(a.radius, 0.0, 40.0);
        if radius > 0.0 {
            for (k, q) in self.players.iter().enumerate() {
                if k == i || !q.alive || n >= targets.len() {
                    continue;
                }
                if (q.hull(&base).center() - me).length() <= radius {
                    targets[n] = Some(k);
                    n += 1;
                }
            }
        } else if let Some(k) = self.op_aimed_player(i, fin_range(a.range), a.cone.clamp(0.0, 30.0)) {
            targets[0] = Some(k);
            n = 1;
        }
        if n == 0 {
            return Err(PowerError::NoTarget);
        }
        for k in targets.iter().flatten().copied() {
            let tid = self.players[k].id;
            let point = self.players[k].hull(&base).center();
            let m = DelayedMark {
                owner: id,
                target: tid,
                t: secs,
                from: me,
                damage: fin(a.damage, 0.0, 60.0),
                knockback: fin(a.knockback, 0.0, 60.0),
                up: fin(a.up, 0.0, 2.0),
                element: a.element,
            };
            let marks = &mut self.powers.ops.marks;
            let slot = marks
                .iter()
                .position(|o| o.is_some_and(|o| o.owner == id && o.target == tid))
                .or_else(|| marks.iter().position(Option::is_none));
            match slot {
                Some(s) => marks[s] = Some(m),
                None => {
                    // Full: the one closest to bursting goes off now, and this one takes its place.
                    let s = marks.iter().enumerate().min_by(|x, y| x.1.map_or(0.0, |m| m.t).total_cmp(&y.1.map_or(0.0, |m| m.t))).map_or(0, |(s, _)| s);
                    let early = marks[s].take();
                    marks[s] = Some(m);
                    if let Some(e) = early {
                        self.burst_mark(&e);
                    }
                }
            }
            self.op_event(OpEvent::Marked { player: id, target: tid, secs, burst: false, point });
        }
        Ok(())
    }

    /// Count the marks down; burst the ones due (from `step_generic_ops`).
    pub(crate) fn step_delayed_hits(&mut self, dt: f32) {
        for s in 0..MAX_MARKS {
            let Some(mut m) = self.powers.ops.marks[s] else { continue };
            let alive = self.pidx(m.target).is_some_and(|k| self.players[k].alive);
            m.t -= dt;
            if !alive {
                self.powers.ops.marks[s] = None;
            } else if m.t > 0.0 {
                self.powers.ops.marks[s] = Some(m);
            } else {
                self.powers.ops.marks[s] = None;
                self.burst_mark(&m);
            }
        }
    }

    fn burst_mark(&mut self, m: &DelayedMark) {
        let Some(k) = self.pidx(m.target) else { return };
        if !self.players[k].alive {
            return;
        }
        let c = self.players[k].hull(&self.cfg.move_params).center();
        let away = Vec3::new(c.x - m.from.x, 0.0, c.z - m.from.z).normalize_or(Vec3::Y);
        let push = away * (1.0 - m.up * 0.5).max(0.0) + Vec3::Y * m.up.max(0.15);
        self.op_event(OpEvent::Marked { player: m.owner, target: m.target, secs: 0.0, burst: true, point: c });
        let source = self.pidx(m.owner).map(|_| m.owner);
        self.apply_hit(HitInfo::new(m.target, source, HitKind::Explosion, m.damage, m.knockback, push, c).with_element(m.element).with_impact(1.3));
    }
}
