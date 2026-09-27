//! `MoveOp::Contagion` — a brand that won't go out and jumps to whoever comes close
//! (docs/toolbox/OPS.md). Amaterasu's black flames, a plague, a curse mark.
//!
//! The aimed enemy (crosshair, aim-forgiving `cone`, line of sight, ≤ `range` m; a dodge-able
//! touch) is branded with `status` for `secs`: the status is re-applied every
//! [`TICK`] s for the whole time, so water / cleansing can't put it out early. Any OTHER enemy of the
//! caster who comes within `spread` m of a branded player catches it too (for [`SPREAD_KEEP`] of the
//! time left, at least a second — it always burns out). Nobody aimed = the move fails.
//! Deterministic, allocation-free.

use bd_core::MAX_PLAYERS;

use super::super::PowerError;
use super::live::{Brand, ToolboxFx};
use crate::combat::HitKind;
use crate::status::StatusKind;
use crate::world::World;

/// Re-application period (s) and the status time each re-application gives.
pub const TICK: f32 = 0.25;
const HOLD: f32 = 0.6;
/// A caught brand keeps this share of the carrier's time left.
pub const SPREAD_KEEP: f32 = 0.7;
const MAX_BRAND: f32 = 10.0;

impl World {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn op_contagion(&mut self, i: usize, range: f32, cone: f32, status: StatusKind, secs: f32, spread: f32) -> Result<(), PowerError> {
        let Some(k) = self.toolbox_target(i, range, cone, true) else { return Err(PowerError::NoTarget) };
        if !self.toolbox_touch(i, k, HitKind::Thrown) {
            return Err(PowerError::NoTarget);
        }
        let source = self.players[i].id;
        self.brand(k, Brand { status, t: secs.clamp(0.5, MAX_BRAND), spread: spread.clamp(0.0, 6.0), source, tick_t: 0.0 }, false);
        Ok(())
    }

    fn brand(&mut self, k: usize, b: Brand, spread_from: bool) {
        let at = self.players[k].hull(&self.cfg.move_params).center();
        let t = b.t;
        self.players[k].power.live.brand = Some(b);
        self.apply_status(k, b.status, HOLD, Some(b.source));
        self.toolbox_fx(b.source, ToolboxFx::Brand, at, at, t, if spread_from { 1.0 } else { 0.0 });
    }

    pub(crate) fn step_brand(&mut self, i: usize, dt: f32) {
        let Some(mut b) = self.players[i].power.live.brand else { return };
        b.t -= dt;
        if b.t <= 0.0 {
            self.players[i].power.live.brand = None;
            let at = self.players[i].hull(&self.cfg.move_params).center();
            self.toolbox_fx(b.source, ToolboxFx::BrandEnd, at, at, 0.0, 0.0);
            return;
        }
        b.tick_t -= dt;
        let tick = b.tick_t <= 0.0;
        if tick {
            b.tick_t += TICK;
        }
        self.players[i].power.live.brand = Some(b);
        if !tick {
            return;
        }
        self.apply_status(i, b.status, HOLD, Some(b.source));
        if b.spread <= 0.0 {
            return;
        }
        // Catch: other enemies of the caster standing close.
        let at = self.players[i].mv.pos;
        let mut catch = [None; MAX_PLAYERS];
        let mut n = 0;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive || q.id == b.source || q.power.live.brand.is_some() || n >= catch.len() {
                continue;
            }
            if q.mv.pos.distance(at) <= b.spread {
                catch[n] = Some(k);
                n += 1;
            }
        }
        for k in catch.iter().flatten().copied() {
            let t = (b.t * SPREAD_KEEP).max(1.0);
            self.brand(k, Brand { t, tick_t: TICK, ..b }, true);
        }
    }
}
