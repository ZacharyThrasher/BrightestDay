//! `MoveOp::Phase` — become intangible for a moment (docs/toolbox/OPS.md). Kamui, a time skip,
//! ghosting through an attack.
//!
//! For `secs` (≤ [`MAX_PHASE`]) every hit on you passes through — projectiles, melee, blasts,
//! beams (the powers hit hook, [`World::phase_ignores`]) — and you keep full control. It ends early
//! the moment you fire a power move (you must be solid to strike), so the play is "phase through
//! it → come out swinging". Deterministic, allocation-free.

use super::super::PowerError;
use super::live::ToolboxFx;
use crate::world::World;

pub const MAX_PHASE: f32 = 2.5;

impl World {
    pub(crate) fn op_phase(&mut self, i: usize, secs: f32) -> Result<(), PowerError> {
        let secs = secs.clamp(0.1, MAX_PHASE);
        let id = self.players[i].id;
        let at = self.players[i].hull(&self.cfg.move_params).center();
        self.players[i].power.live.phase = secs;
        self.toolbox_fx(id, ToolboxFx::PhaseOn, at, at, secs, 0.0);
        Ok(())
    }

    /// The powers hit hook: is player `ti` intangible right now?
    pub(crate) fn phase_ignores(&self, ti: usize) -> bool {
        self.players.get(ti).is_some_and(|p| p.power.live.phase > 0.0)
    }

    /// A power move is starting: phasing ends (called by the move runner before the ops run).
    pub(crate) fn phase_break(&mut self, i: usize) {
        if self.players[i].power.live.phase > 0.0 {
            self.players[i].power.live.phase = 0.0;
            let id = self.players[i].id;
            let at = self.players[i].hull(&self.cfg.move_params).center();
            self.toolbox_fx(id, ToolboxFx::PhaseOff, at, at, 0.0, 0.0);
        }
    }

    pub(crate) fn step_phase(&mut self, i: usize, dt: f32) {
        let t = self.players[i].power.live.phase;
        if t <= 0.0 {
            return;
        }
        let t = t - dt;
        if t <= 0.0 {
            self.phase_break(i);
        } else {
            self.players[i].power.live.phase = t;
        }
    }
}
