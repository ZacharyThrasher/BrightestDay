//! `MoveOp::Transpose` — swap places with the aimed thing, no ROOM needed (docs/toolbox/OPS.md).
//!
//! The clap (Boogie Woogie): you and the aimed player — enemy or not — or anyone's loose
//! construct trade positions instantly; both keep their velocity, so a swap mid-dash or mid-fall
//! carries on in the new spot. Nothing aimed = the enemy you tagged (`MoveOp::Tag`) if one lives.
//! `others: true` swaps two OTHER things instead: the aimed thing and the enemy you last hit (or
//! tagged) — a shell game on your opponents. A zero-damage credit hit on a swapped enemy keeps
//! combos honest (a `Finisher` right after the clap lands inside its window).
//! Deterministic, allocation-free; does nothing unless a move uses it.

use bd_core::mechanics::FormCategory;
use bd_core::{ConstructId, PlayerId, Vec3};

use super::super::{PowerError, PowerEvent};
use crate::combat::{HitInfo, HitKind};
use crate::events::GameEvent;
use crate::world::World;

/// A swappable body: a player index or a construct.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Body {
    Player(usize),
    Construct(ConstructId),
}

/// Constructs bigger than this (half-extent, m) are never swapped (giants, kaiju).
const MAX_HALF: f32 = 3.0;
/// A player swapped anywhere but by their own move needs ground somewhere under their feet within this (m):
/// a fall is fine, the void is not.
pub const SAFE_DEPTH: f32 = 64.0;

impl World {
    pub(crate) fn op_transpose(&mut self, i: usize, range: f32, cone: f32, others: bool) -> Result<(), PowerError> {
        let range = super::fclamp(range, 1.0, 60.0);
        let cone = super::fclamp(cone, 0.0, 20.0);
        let aimed = self.transpose_aimed(i, range, cone);
        let me = Body::Player(i);
        let (a, b) = if others {
            // Two other things: the aimed one and your last target (or tag).
            let Some(b) = aimed else { return Err(PowerError::NoTarget) };
            let Some(a) = self.transpose_remembered(i).filter(|a| *a != b) else { return Err(PowerError::NoTarget) };
            (a, b)
        } else {
            match aimed.or_else(|| self.transpose_tagged(i)) {
                Some(b) => (me, b),
                None => return Err(PowerError::NoTarget),
            }
        };
        let pa = self.transpose_pos(a).ok_or(PowerError::NoTarget)?;
        let pb = self.transpose_pos(b).ok_or(PowerError::NoTarget)?;
        // Never a ring-out (OPS_REVIEW blocker 3): anyone but you only goes where there is ground
        // under them and room for their body; otherwise the clap fails (nothing spent).
        if !self.transpose_safe(i, a, pb, b) || !self.transpose_safe(i, b, pa, a) {
            return Err(PowerError::NoTarget);
        }
        self.transpose_place(a, pb, b);
        self.transpose_place(b, pa, a);
        let id = self.players[i].id;
        // Credit (zero damage, no knockback): the swap interrupts and opens a combo window.
        for body in [a, b] {
            if let Body::Player(k) = body
                && k != i
            {
                let tid = self.players[k].id;
                let c = self.players[k].hull(&self.cfg.move_params).center();
                self.apply_hit(HitInfo::new(tid, Some(id), HitKind::RingShove, 0.0, 0.0, Vec3::Y, c));
            }
        }
        // `a` is always a player (you, or the enemy you last hit).
        let Body::Player(ka) = a else { return Ok(()) };
        let (b_player, b_construct) = match b {
            Body::Player(k) => (Some(self.players[k].id), None),
            Body::Construct(c) => (None, Some(c)),
        };
        let (ev_a, a_pos, b_pos) = (self.players[ka].id, self.players[ka].mv.pos, pa);
        self.inject_event(GameEvent::Power(PowerEvent::Swapped { a: ev_a, b_player, b_construct, a_pos, b_pos }));
        Ok(())
    }

    /// The aimed player (crosshair, then the forgiving cone), else the first swappable construct
    /// on the aim line.
    fn transpose_aimed(&self, i: usize, range: f32, cone: f32) -> Option<Body> {
        if let Some(k) = self.op_aimed_player(i, range, cone) {
            if self.players[k].seat.is_none() {
                return Some(Body::Player(k));
            }
        }
        let (eye, look) = self.op_eye_look(i);
        let (cid, _, _) = self.constructs.segment_query(eye, eye + look * range, 0.5, None)?;
        self.swappable(cid).then_some(Body::Construct(cid))
    }

    fn swappable(&self, cid: ConstructId) -> bool {
        self.constructs.get(cid).is_some_and(|c| {
            c.active()
                && !c.attached()
                && c.parent != Some(c.id)
                && c.pilot.is_none()
                && matches!(c.category, FormCategory::Creature | FormCategory::Object)
                && c.half.max_element() <= MAX_HALF
        })
    }

    /// The enemy you tagged (alive, tag still on).
    fn transpose_tagged(&self, i: usize) -> Option<Body> {
        let id = self.players[i].id;
        let k = self.powers_tagged_by(id).next()?;
        (self.players[k].seat.is_none()).then_some(Body::Player(k))
    }

    /// For `others`: the enemy you last hit, else the one you tagged.
    fn transpose_remembered(&self, i: usize) -> Option<Body> {
        let last: Option<PlayerId> = self.players[i].last_target;
        if let Some(k) = last.and_then(|t| self.pidx(t)).filter(|&k| k != i && self.players[k].alive && self.players[k].seat.is_none()) {
            return Some(Body::Player(k));
        }
        self.transpose_tagged(i)
    }

    fn transpose_pos(&self, b: Body) -> Option<Vec3> {
        match b {
            Body::Player(k) => self.players.get(k).filter(|p| p.alive).map(|p| p.mv.pos),
            Body::Construct(c) => self.constructs.get(c).filter(|_| self.swappable(c)).map(|c| c.pos),
        }
    }

    /// Where a player put in `other`'s place stands (on top of a construct).
    fn transpose_lift(&self, other: Body) -> f32 {
        match other {
            Body::Construct(c) => self.constructs.get(c).map_or(0.0, |c| (c.half.y * 2.0 + 0.1).min(MAX_HALF * 2.0)),
            Body::Player(_) => 0.0,
        }
    }

    /// May `b` go to `to` (where `other` is)? The caster may put themself anywhere; any other
    /// player needs to land in play, with room for their body and ground within [`SAFE_DEPTH`] m
    /// under their feet (no swapping someone onto a construct flying over the void).
    fn transpose_safe(&self, i: usize, b: Body, to: Vec3, other: Body) -> bool {
        let Body::Player(k) = b else { return true };
        if k == i {
            return true;
        }
        let lift = self.transpose_lift(other);
        let feet = to + Vec3::Y * lift;
        let h = self.players[k].hull(&self.cfg.move_params).half().y;
        let room = [0.3, h, h * 2.0 - 0.2].iter().all(|y| self.voxels.get_at(feet + Vec3::Y * *y) == 0);
        self.in_play(feet, 0.0) && room && self.ground_below(feet + Vec3::Y * 0.3, SAFE_DEPTH + lift).is_some()
    }

    /// Put `b` where `other` stood (feet to feet; a construct lands on its origin, a player put
    /// where a construct was stands on top of it).
    fn transpose_place(&mut self, b: Body, to: Vec3, other: Body) {
        match b {
            Body::Player(k) => {
                let lift = self.transpose_lift(other);
                let p = &mut self.players[k];
                p.mv.pos = to + Vec3::Y * lift;
                p.mv.grounded = false;
            }
            Body::Construct(c) => {
                if let Some(c) = self.constructs.get_mut(c) {
                    c.pos = to;
                    c.prev_pos = to;
                }
            }
        }
    }
}
