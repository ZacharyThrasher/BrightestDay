//! `MoveOp::Banish` — warp the aimed thing somewhere else (docs/toolbox/OPS.md). Kamui's swirl.
//!
//! The aimed ENEMY CONSTRUCT (a projectile, a summon, an orb — anything on your aim line that isn't
//! yours, worn or held) is swallowed: it's simply gone. Else the aimed enemy PLAYER is displaced
//! `dist` m straight up (less if that leaves the play area or isn't clear): no damage, no stun, they
//! keep full control and fall back into the fight — a reposition, never a removal. Nothing aimed =
//! the move fails (nothing spent). Deterministic, allocation-free.

use bd_core::Vec3;

use super::super::PowerError;
use super::live::ToolboxFx;
use crate::combat::HitKind;
use crate::events::GoneReason;
use crate::world::World;

impl World {
    pub(crate) fn op_banish(&mut self, i: usize, range: f32, dist: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.toolbox_eye_look(i);
        let range = range.clamp(1.0, 40.0);
        // A construct first: swallowing an incoming thing is the defensive use.
        let hit = self
            .constructs
            .segment_query(eye, eye + look * range, 0.8, None)
            .filter(|(c, _, _)| self.constructs.get(*c).is_some_and(|c| c.owner != id && !c.attached() && c.parent != Some(c.id) && c.gone.is_none()));
        let aimed = self.toolbox_target(i, range, 6.0, true);
        let player_first = match (hit, aimed) {
            (Some((_, t, _)), Some(k)) => {
                let d = self.players[k].hull(&self.cfg.move_params).center().distance(eye);
                d < t * range
            }
            (None, Some(_)) => true,
            _ => false,
        };
        if !player_first {
            if let Some((cid, _, _)) = hit {
                let Some(c) = self.constructs.get_mut(cid) else { return Err(PowerError::NoTarget) };
                let at = c.center();
                c.gone = Some(GoneReason::Spent);
                self.toolbox_fx(id, ToolboxFx::Banish, at, at, 0.0, 1.0);
                return Ok(());
            }
        }
        let Some(k) = aimed else { return Err(PowerError::NoTarget) };
        if self.players[k].seat.is_some() {
            return Err(PowerError::NoTarget);
        }
        let from = self.players[k].mv.pos;
        // As high as allowed (in play, clear), up to `dist`.
        let mut to = None;
        let mut h = dist.clamp(2.0, 14.0);
        while h >= 2.0 {
            let p = from + Vec3::Y * h;
            if self.in_play(p + Vec3::Y * 2.0, 0.0) && self.toolbox_clear(k, p) {
                to = Some(p);
                break;
            }
            h -= 1.5;
        }
        let Some(to) = to else { return Err(PowerError::NoTarget) };
        if !self.toolbox_touch(i, k, HitKind::RingShove) {
            return Err(PowerError::NoTarget);
        }
        let q = &mut self.players[k];
        q.mv.release_tethers();
        q.mv.pos = to;
        q.mv.vel = Vec3::ZERO;
        q.mv.grounded = false;
        self.toolbox_fx(id, ToolboxFx::Banish, from, to, 0.0, 0.0);
        Ok(())
    }
}
