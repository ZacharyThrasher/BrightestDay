//! `Lockout { range, secs }` — the aimed enemy (generous 8° assist cone) can't use power moves
//! (held or borrowed) for `secs` (≤ [`MAX_SECS`]): Sombra's hack, a silence, a Nullify. Movement,
//! tethers, casting constructs and their weapons stay theirs — it takes a verb away, never control.
//! Zones can carry the same lock (`Zone { lockout: true }`: an EMP, an anti-magic field).

use super::OpEvent;
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_SECS: f32 = 2.5;

impl World {
    pub(crate) fn op_lockout(&mut self, i: usize, range: f32, secs: f32) -> Result<(), PowerError> {
        let Some(k) = self.op_aimed_player(i, super::fclamp(range, 1.0, 40.0), 8.0) else { return Err(PowerError::NoTarget) };
        let secs = super::fclamp(secs, 0.2, MAX_SECS);
        let (_, look) = self.op_eye_look(i);
        if !self.op_touch(i, k, look) {
            return Err(PowerError::NoTarget);
        }
        self.lock_out(k, secs);
        let (player, target) = (self.players[i].id, self.players[k].id);
        self.op_event(OpEvent::LockedOut { player, target, secs });
        Ok(())
    }

    /// Lock player index `k`'s power moves for at least `secs`.
    pub(crate) fn lock_out(&mut self, k: usize, secs: f32) {
        let Some(p) = self.players.get(k) else { return };
        let Some(slot) = self.powers.ops.locked.get_mut(k) else { return };
        let left = slot.filter(|(id, _)| *id == p.id).map_or(0.0, |(_, t)| t);
        *slot = Some((p.id, left.max(secs.min(MAX_SECS))));
    }
}
