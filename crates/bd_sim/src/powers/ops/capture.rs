//! `Capture` — possess the aimed enemy CONSTRUCT for a while (docs/toolbox/OPS.md). Cappy lands on
//! it and you ARE it: the construct becomes yours (it turns on its maker), and with `ride` you are
//! seated in it and drive it if it can be ridden (a creature big enough, a vehicle, a mech — the
//! ordinary boarding rules). When `secs` run out you pop out and it goes back to its maker.
//!
//! Owner rule: players are never captured (no mind control) — only constructs. Held / worn /
//! attached things, gravity cores and anything already piloted are refused. Deterministic, one slot
//! per capturer, nothing runs while nobody holds a capture.

use bd_core::{ConstructId, MAX_PLAYERS, PlayerId};

use super::OpEvent;
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_SECS: f32 = 8.0;
pub const MAX_RANGE: f32 = 40.0;

/// A live capture (`OpState::captures`, indexed by the capturer's player index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Captured {
    pub owner: PlayerId,
    pub construct: ConstructId,
    /// Who made it (it goes back to them).
    pub maker: PlayerId,
    pub left: f32,
    pub ridden: bool,
}

impl World {
    pub(crate) fn op_capture(&mut self, i: usize, range: f32, secs: f32, ride: bool) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.op_eye_look(i);
        let range = if range.is_finite() { range.clamp(2.0, MAX_RANGE) } else { 20.0 };
        let secs = if secs.is_finite() { secs.clamp(0.5, MAX_SECS) } else { 5.0 };
        // Let a previous capture go first (one at a time).
        self.release_capture(i);
        let hit = self.constructs.segment_query(eye, eye + look * range, 0.7, Some(id));
        let Some((cid, _, _)) = hit else { return Err(PowerError::NoTarget) };
        let Some(c) = self.constructs.get_mut(cid) else { return Err(PowerError::NoTarget) };
        use bd_core::mechanics::FormCategory as C;
        if c.gone.is_some() || c.attached() || c.parent == Some(c.id) || c.pilot.is_some() || matches!(c.category, C::Held | C::Worn) {
            return Err(PowerError::NoTarget);
        }
        let maker = c.owner;
        c.owner = id;
        c.target = None;
        let pos = c.center();
        let rideable = crate::constructs::pilot::boardable(c);
        let ridden = ride && rideable && self.board(id, cid, MAX_RANGE + 20.0).is_ok();
        self.powers.ops.captures[i] = Some(Captured { owner: id, construct: cid, maker, left: secs, ridden });
        self.op_event(OpEvent::Captured { player: id, construct: cid, pos, secs, on: true });
        Ok(())
    }

    /// End player `i`'s capture: out of the seat, and the construct goes back to its maker.
    pub(crate) fn release_capture(&mut self, i: usize) {
        let Some(cap) = self.powers.ops.captures.get(i).copied().flatten() else { return };
        self.powers.ops.captures[i] = None;
        if cap.ridden && self.players.get(i).is_some_and(|p| p.id == cap.owner && p.seat.is_some_and(|s| s.construct == cap.construct)) {
            self.eject(cap.owner);
        }
        let mut pos = None;
        if let Some(c) = self.constructs.get_mut(cap.construct)
            && c.owner == cap.owner
        {
            c.owner = cap.maker;
            c.target = None;
            pos = Some(c.center());
        }
        let pos = pos.unwrap_or_else(|| self.players.get(i).map_or(bd_core::Vec3::ZERO, |p| p.mv.pos));
        self.op_event(OpEvent::Captured { player: cap.owner, construct: cap.construct, pos, secs: 0.0, on: false });
    }

    pub(crate) fn step_captures(&mut self, dt: f32) {
        for i in 0..MAX_PLAYERS.min(self.players.len()) {
            let Some(mut cap) = self.powers.ops.captures[i] else { continue };
            let p = &self.players[i];
            let alive = self.constructs.get(cap.construct).is_some_and(|c| c.gone.is_none() && c.owner == cap.owner);
            // A ride ends when you leave the seat yourself.
            let left_seat = cap.ridden && p.seat.is_none_or(|s| s.construct != cap.construct);
            cap.left -= dt;
            if p.id != cap.owner || !p.alive || !alive || left_seat || cap.left <= 0.0 {
                self.powers.ops.captures[i] = Some(Captured { ridden: cap.ridden && !left_seat, ..cap });
                self.release_capture(i);
                continue;
            }
            self.powers.ops.captures[i] = Some(cap);
        }
    }
}
