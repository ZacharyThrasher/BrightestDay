//! `MoveOp::Afflict` — a spell / a force hits THAT person (docs/toolbox/OPS.md).
//!
//! The aimed enemy (`range`, aim-forgiving `cone`) gets a short `status` (capped per kind: stuns are
//! micro, floats and chills brief — never a freeze), optional `damage` and `knockback` along your aim,
//! and an optional `up` lift (m/s) that raises them off their feet (a Float on top makes them hang:
//! Wingardium Leviosa, a Force lift). Stupefy is Stun + knockback, Petrificus a Stun alone, a Force
//! choke a small lift + Float + a little damage. An `OpEvent::Zap` draws the bolt hand → target.
//! Deterministic, allocation-free; does nothing unless a move uses it.

use bd_core::Vec3;
use bd_core::mechanics::Element;

use super::OpEvent;
use super::super::PowerError;
use crate::combat::{HitInfo, HitKind};
use crate::status::StatusKind;
use crate::world::World;

/// The longest a move may apply each status to another player (s): control stays theirs.
pub fn status_cap(s: StatusKind) -> f32 {
    match s {
        StatusKind::Stun => 1.2,
        StatusKind::Float => 2.5,
        StatusKind::Chill => 3.0,
        StatusKind::Shadow => 3.0,
        StatusKind::Burn | StatusKind::Poison => 4.0,
        StatusKind::Wet => 6.0,
    }
}

/// A finite op range (m), 1..=60.
pub(crate) fn fin_range(r: f32) -> f32 {
    if r.is_finite() { r.clamp(1.0, 60.0) } else { 10.0 }
}

/// `MoveOp::Afflict`'s numbers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AfflictArgs {
    pub range: f32,
    pub cone: f32,
    pub status: Option<StatusKind>,
    pub secs: f32,
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub element: Element,
}

impl World {
    pub(crate) fn op_afflict(&mut self, i: usize, a: AfflictArgs) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.op_eye_look(i);
        let Some(k) = self.op_aimed_player(i, fin_range(a.range), a.cone.clamp(0.0, 30.0)) else { return Err(PowerError::NoTarget) };
        let base = self.cfg.move_params.clone();
        let tid = self.players[k].id;
        let c = self.players[k].hull(&base).center();
        let fin = |v: f32, hi: f32| if v.is_finite() { v.clamp(0.0, hi) } else { 0.0 };
        let damage = fin(a.damage, 60.0);
        let knockback = fin(a.knockback, 60.0);
        let up = fin(a.up, 30.0);
        let dir = (c - eye).normalize_or(look);
        let push = Vec3::new(dir.x, dir.y.max(0.0) + if up > 0.0 { 0.6 } else { 0.15 }, dir.z);
        self.op_event(OpEvent::Zap { player: id, from: eye + look * 0.5, to: c, element: a.element, target: Some(tid) });
        // The hit itself (a zero one still credits and interrupts).
        let r = self.apply_hit(HitInfo::new(tid, Some(id), HitKind::RingBlast, damage, knockback, push, c).with_element(a.element).with_impact(if damage > 10.0 { 1.2 } else { 0.7 }));
        if !r.applied {
            // Shielded / dodged through: the spell fizzles on them (the move still fired).
            return Ok(());
        }
        if let Some(s) = a.status {
            let secs = fin(a.secs, status_cap(s));
            if secs > 0.0 {
                self.apply_status(k, s, secs, Some(id));
            }
        }
        if up > 0.0 {
            let q = &mut self.players[k];
            if q.alive && q.seat.is_none() {
                q.mv.vel.y = q.mv.vel.y.max(up);
                q.mv.grounded = false;
            }
        }
        Ok(())
    }
}
