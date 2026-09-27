//! `MoveOp::Contagion` — a brand that won't go out and jumps to whoever comes close
//! (docs/toolbox/OPS.md). Amaterasu's black flames, a plague, a curse mark.
//!
//! The aimed enemy (crosshair, aim-forgiving `cone`, line of sight, ≤ `range` m; a dodge-able
//! touch) is branded with `status` for `secs`: the status is re-applied every
//! [`TICK`] s for the whole time, so water / cleansing can't put it out early. Any OTHER enemy of the
//! caster who comes within `spread` m of a branded player catches it too (for [`SPREAD_KEEP`] of the
//! time left — each hop is shorter, so it always burns out). Nobody aimed = the move fails.
//!
//! Never take control away (OPS_REVIEW blocker 1): a CONTROL status (Stun, Float) is applied for at
//! most [`CONTROL_CAP`] s in total per victim per brand, and a player a brand just burned out on
//! can't catch one again for [`IMMUNE`] s (no passing it back and forth).
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
/// Shortest brand worth passing on (a hop that would be shorter doesn't happen).
const MIN_SPREAD: f32 = 0.5;
/// Most control-status time (Stun / Float) one brand ever holds one victim for (s).
pub const CONTROL_CAP: f32 = 1.2;
/// After a brand burns out on someone they can't catch one for this long (s).
pub const IMMUNE: f32 = 4.0;

/// Statuses that take (part of) control away: capped.
fn control(s: StatusKind) -> bool {
    matches!(s, StatusKind::Stun | StatusKind::Float)
}

impl World {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn op_contagion(&mut self, i: usize, range: f32, cone: f32, status: StatusKind, secs: f32, spread: f32) -> Result<(), PowerError> {
        let Some(k) = self.toolbox_target(i, range, cone, true) else { return Err(PowerError::NoTarget) };
        if !self.toolbox_touch(i, k, HitKind::Thrown) {
            return Err(PowerError::NoTarget);
        }
        let source = self.players[i].id;
        let t = super::fclamp(secs, 0.5, MAX_BRAND);
        let spread = super::fclamp(spread, 0.0, 6.0);
        self.brand(k, Brand { status, t, spread, source, tick_t: 0.0, ctl: CONTROL_CAP }, false);
        Ok(())
    }

    fn brand(&mut self, k: usize, mut b: Brand, spread_from: bool) {
        let at = self.players[k].hull(&self.cfg.move_params).center();
        let t = b.t;
        let hold = self.brand_hold(&mut b);
        self.players[k].power.live.brand = Some(b);
        if hold > 0.0 {
            self.apply_status(k, b.status, hold, Some(b.source));
        }
        self.toolbox_fx(b.source, ToolboxFx::Brand, at, at, t, if spread_from { 1.0 } else { 0.0 });
    }

    /// The status time one application gives. A control status spends the brand's budget: the
    /// first application costs its full HOLD, each re-application only the TICK it extends the
    /// status by (the rest overlaps), and the last one extends by exactly what is left.
    fn brand_hold(&self, b: &mut Brand) -> f32 {
        if !control(b.status) {
            return HOLD;
        }
        if b.ctl >= CONTROL_CAP {
            let hold = HOLD.min(CONTROL_CAP);
            b.ctl = CONTROL_CAP - hold;
            return hold;
        }
        if b.ctl <= 0.0 {
            return 0.0;
        }
        let ext = TICK.min(b.ctl);
        b.ctl -= ext;
        HOLD - TICK + ext
    }

    pub(crate) fn step_brand(&mut self, i: usize, dt: f32) {
        let live = &mut self.players[i].power.live;
        if live.brand_immune > 0.0 {
            live.brand_immune = (live.brand_immune - dt).max(0.0);
        }
        let Some(mut b) = live.brand else { return };
        b.t -= dt;
        if b.t <= 0.0 {
            self.players[i].power.live.brand = None;
            self.players[i].power.live.brand_immune = IMMUNE;
            let at = self.players[i].hull(&self.cfg.move_params).center();
            self.toolbox_fx(b.source, ToolboxFx::BrandEnd, at, at, 0.0, 0.0);
            return;
        }
        b.tick_t -= dt;
        let tick = b.tick_t <= 0.0;
        if tick {
            b.tick_t += TICK;
        }
        let hold = if tick { self.brand_hold(&mut b) } else { 0.0 };
        self.players[i].power.live.brand = Some(b);
        if !tick {
            return;
        }
        if hold > 0.0 {
            self.apply_status(i, b.status, hold, Some(b.source));
        }
        let t = b.t * SPREAD_KEEP;
        if b.spread <= 0.0 || t < MIN_SPREAD {
            return;
        }
        // Catch: other enemies of the caster standing close (not one it just burned out on).
        let at = self.players[i].mv.pos;
        let mut catch = [None; MAX_PLAYERS];
        let mut n = 0;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive || q.id == b.source || q.power.live.brand.is_some() || q.power.live.brand_immune > 0.0 || n >= catch.len() {
                continue;
            }
            if q.mv.pos.distance(at) <= b.spread {
                catch[n] = Some(k);
                n += 1;
            }
        }
        for k in catch.iter().flatten().copied() {
            self.brand(k, Brand { t, tick_t: TICK, ctl: CONTROL_CAP, ..b }, true);
        }
    }
}
