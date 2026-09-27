//! `MoveOp::Strain` — the power's toll (docs/toolbox/OPS.md, CONTENT: TV & CARTOONS).
//!
//! The user takes `pct` damage % themselves: no hit, no knockback, no interrupt, no status — only
//! the number that decides how far the next hit launches them. Eleven's nosebleed after a big
//! telekinetic throw, a bloodbender's strain, a blood-price spell. Put it AFTER the move's main op:
//! a move whose first op fails runs nothing else, so the toll is only paid when the move fires.
//! Deterministic, allocation-free; does nothing unless a move uses it.

use super::super::PowerError;
use crate::world::World;

/// Most damage % one Strain may cost (data is clamped to it).
pub const STRAIN_MAX_PCT: f32 = 40.0;

impl World {
    pub(crate) fn op_strain(&mut self, i: usize, pct: f32) -> Result<(), PowerError> {
        let pct = if pct.is_finite() { pct.clamp(0.0, STRAIN_MAX_PCT) } else { 0.0 };
        let p = &mut self.players[i];
        if p.alive {
            p.damage_pct = (p.damage_pct + pct).min(999.0);
        }
        Ok(())
    }
}
