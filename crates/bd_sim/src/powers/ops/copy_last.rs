//! `MoveOp::CopyLast` — replay, as yourself, the last power move an enemy fired near you
//! (docs/toolbox/OPS.md). The copy wheel: read it, then throw it back.
//!
//! Every successful power move (held, tier, kit or borrowed) is noted by every OTHER living player
//! within [`SEE_RANGE`] m of the user ([`World::note_seen_move`], one line in the move runner): the
//! (power, tier, slot) plus how far away it was. `CopyLast` replays that move's ops as the copier
//! if it was fired within `range` m and at most `secs` s ago. Ops that act on the ORIGINAL user's own
//! power (Transform / Tier) and copies of copies are skipped; a move with nothing left to run —
//! or nothing seen — fails (nothing spent). You pay the copied move's own willpower weight (`cost`).
//! Deterministic, allocation-free.

use super::super::{MoveOp, PowerError, PowerTable};
use super::live::{Seen, ToolboxFx};
use crate::world::World;

/// Players farther than this don't notice a move at all.
pub const SEE_RANGE: f32 = 45.0;

impl World {
    /// Called once per successful power move by player `i` (power `power`, tier, slot).
    pub(crate) fn note_seen_move(&mut self, i: usize, power: u16, tier: u8, slot: u8) {
        let at = self.players[i].mv.pos;
        for k in 0..self.players.len() {
            if k == i || !self.players[k].alive {
                continue;
            }
            let dist = self.players[k].mv.pos.distance(at);
            if dist <= SEE_RANGE {
                self.players[k].power.live.seen = Some(Seen { power, tier, slot, age: 0.0, dist });
            }
        }
    }

    pub(crate) fn step_seen(&mut self, i: usize, dt: f32) {
        if let Some(s) = self.players[i].power.live.seen.as_mut() {
            s.age += dt;
            if s.age > 30.0 {
                self.players[i].power.live.seen = None;
            }
        }
    }

    pub(crate) fn op_copy_last(&mut self, i: usize, range: f32, secs: f32) -> Result<(), PowerError> {
        let Some(s) = self.players[i].power.live.seen.filter(|s| s.age <= secs.clamp(0.5, 30.0) && s.dist <= range.clamp(1.0, SEE_RANGE)) else {
            return Err(PowerError::NoTarget);
        };
        let Some(def) = PowerTable::get().def(s.power) else { return Err(PowerError::NoTarget) };
        let Some(mv) = def.moves_at(s.tier).get(s.slot as usize) else { return Err(PowerError::NoTarget) };
        let copyable = |o: &MoveOp| !matches!(o, MoveOp::Transform | MoveOp::Tier(_) | MoveOp::CopyLast { .. });
        if !mv.ops.iter().any(copyable) {
            return Err(PowerError::NoTarget);
        }
        // You perform it yourself: its own willpower weight is yours to pay (a copied ultimate is
        // never a discount ultimate). Cast ops still pay the cast pipeline's price as usual.
        let price = if self.cfg.rules.infinite_willpower() { 0.0 } else { mv.cost.max(0.0) };
        if price > self.players[i].willpower + 1e-4 {
            return Err(PowerError::NotEnoughWillpower);
        }
        let id = self.players[i].id;
        let (eye, look) = self.toolbox_eye_look(i);
        self.toolbox_fx(id, ToolboxFx::Copy, eye, eye + look * 3.0, 0.0, 0.0);
        let mut first: Option<Result<(), PowerError>> = None;
        for op in mv.ops.iter().filter(|o| copyable(o)) {
            let r = self.run_op(i, def, mv, op);
            if first.is_none() {
                first = Some(r);
                if r.is_err() {
                    break;
                }
            }
            if !self.players[i].alive {
                break;
            }
        }
        // Used up: the next copy needs a fresh read.
        self.players[i].power.live.seen = None;
        let r = first.unwrap_or(Err(PowerError::NoTarget));
        if r.is_ok() {
            let p = &mut self.players[i];
            p.willpower = (p.willpower - price).max(0.0);
        }
        r
    }
}
