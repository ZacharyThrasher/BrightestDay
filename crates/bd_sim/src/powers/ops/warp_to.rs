//! `MoveOp::WarpTo` — lock onto the enemy you look at and appear right behind them
//! (docs/toolbox/OPS.md). Instant Transmission, a flash step behind, sonido.
//!
//! The target is the enemy under the crosshair, else the one nearest it inside `cone`° — no line
//! of sight needed (you sense them). You arrive `behind` m behind them (their back, from their own
//! facing; `behind` < 0 = in FRONT of them, face to face), at their height. No target / no clear
//! spot = the move fails (nothing spent). Unlike a blink (a distance along your aim) this is
//! target-relative: you always end up on them, ready for the next move in a chain.
//! Deterministic, allocation-free.

use bd_core::Vec3;

use super::super::PowerError;
use super::live::ToolboxFx;
use crate::world::World;

impl World {
    pub(crate) fn op_warp_to(&mut self, i: usize, range: f32, cone: f32, behind: f32, line: bool) -> Result<(), PowerError> {
        let Some(k) = self.toolbox_target(i, range, cone, false) else { return Err(PowerError::NoTarget) };
        let id = self.players[i].id;
        let q = &self.players[k];
        // `line`: measured along your own line to them (you arrive facing them); else from their facing.
        let want = if line {
            let d = q.mv.pos - self.players[i].mv.pos;
            q.mv.pos - Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::Z) * behind.clamp(-4.0, 6.0)
        } else {
            q.mv.pos - Vec3::new(q.look().x, 0.0, q.look().z).normalize_or(Vec3::Z) * behind.clamp(-4.0, 6.0)
        };
        let Some(to) = self.toolbox_spot(i, want) else { return Err(PowerError::NoTarget) };
        let from = self.players[i].mv.pos;
        self.toolbox_teleport(i, to);
        self.players[i].mv.vel = Vec3::ZERO;
        self.toolbox_fx(id, ToolboxFx::Warp, from, to, 0.0, 0.0);
        self.inject_event(crate::events::GameEvent::Power(super::super::PowerEvent::Blinked { player: id, from, to, afterimage: true }));
        Ok(())
    }
}
