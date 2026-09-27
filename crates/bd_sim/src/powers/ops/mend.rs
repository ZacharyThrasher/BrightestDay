//! `Mend` — heal (docs/toolbox/OPS.md): `pct` off your damage % at once, then `per_s` for `secs`
//! (≤ 6 s). A hit that deals damage ends the over-time part (drink your estus behind cover). At 0 %
//! with nothing to heal over time the move does nothing (and costs nothing).

use bd_core::{MAX_PLAYERS, PlayerId};

use super::OpEvent;
use crate::combat::HitInfo;
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_SECS: f32 = 6.0;

/// Healing over time (`OpState::mends`, indexed by player index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mending {
    pub owner: PlayerId,
    pub per_s: f32,
    pub left: f32,
}

impl World {
    pub(crate) fn op_mend(&mut self, i: usize, pct: f32, per_s: f32, secs: f32) -> Result<(), PowerError> {
        let pct = if pct.is_finite() { pct.clamp(0.0, 200.0) } else { 0.0 };
        let per_s = if per_s.is_finite() { per_s.clamp(0.0, 60.0) } else { 0.0 };
        let secs = if secs.is_finite() { secs.clamp(0.0, MAX_SECS) } else { 0.0 };
        let over_time = per_s > 0.0 && secs > 0.0;
        let p = &mut self.players[i];
        if p.damage_pct <= 0.0 && !over_time {
            return Err(PowerError::NoTarget);
        }
        let id = p.id;
        p.damage_pct = (p.damage_pct - pct).max(0.0);
        self.powers.ops.mends[i] = over_time.then_some(Mending { owner: id, per_s, left: secs });
        self.op_event(OpEvent::Mended { player: id, pct, secs: if over_time { secs } else { 0.0 } });
        Ok(())
    }

    pub(crate) fn step_mends(&mut self, dt: f32) {
        // Every slot (not just the live player count): a slot left behind by a player who left is
        // cleared too, so `adventure_active` goes quiet again (OPS_REVIEW low).
        for i in 0..MAX_PLAYERS {
            let Some(mut m) = self.powers.ops.mends[i] else { continue };
            let Some(p) = self.players.get_mut(i).filter(|p| p.id == m.owner && p.alive) else {
                self.powers.ops.mends[i] = None;
                continue;
            };
            let t = dt.min(m.left);
            p.damage_pct = (p.damage_pct - m.per_s * t).max(0.0);
            m.left -= dt;
            self.powers.ops.mends[i] = (m.left > 0.0).then_some(m);
        }
    }

    /// A damaging hit on `ti` ends their healing over time.
    pub(crate) fn mend_on_hit(&mut self, ti: usize, hit: &HitInfo) {
        if hit.damage > 0.0 && hit.source != Some(hit.target) && self.powers.ops.mends[ti].is_some_and(|m| m.owner == hit.target) {
            self.powers.ops.mends[ti] = None;
        }
    }
}
