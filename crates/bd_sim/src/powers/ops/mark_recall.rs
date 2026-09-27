//! `MoveOp::MarkRecall` — throw a marker, then teleport to it whenever you like (docs/toolbox/OPS.md).
//!
//! First use: a marker flies out along the crosshair (≤ `range` m, instant) and sticks — to the
//! enemy it meets (it rides on them) or to the terrain it lands on. It lasts `secs`. Using the move
//! again while the marker lives: you vanish and reappear at it (next to the marked enemy, behind
//! them), arriving with an optional round `strike` (damage; 0 = none). The marker stays until it
//! expires, so you can flash back to it again (the move's cooldown paces it). Nothing to mark (the
//! throw hit nothing within range) = the marker lands at the range's end in mid-air (a waypoint).
//! Deterministic, allocation-free; nothing happens unless a move uses it.

use bd_core::Vec3;
use bd_core::mechanics::Element;

use super::super::PowerError;
use super::live::{MarkAt, MarkState, ToolboxFx};
use crate::movement::{CollisionWorld, raycast_world};
use crate::world::World;

impl World {
    pub(crate) fn op_mark_recall(&mut self, i: usize, range: f32, secs: f32, strike: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let feet = self.players[i].mv.pos;
        // Recall: the marker lives.
        if let Some(m) = self.players[i].power.live.mark {
            let want = match m.at {
                MarkAt::Point(p) => Some(p),
                MarkAt::Player(t) => self.pidx(t).filter(|&k| self.players[k].alive).map(|k| {
                    let q = &self.players[k];
                    let back = Vec3::new(q.look().x, 0.0, q.look().z).normalize_or(Vec3::Z);
                    q.mv.pos - back * 1.4
                }),
            };
            match want.and_then(|w| self.toolbox_spot(i, w)) {
                Some(to) => {
                    self.toolbox_teleport(i, to);
                    self.toolbox_fx(id, ToolboxFx::Recall, feet, to, 0.0, 0.0);
                    self.inject_event(crate::events::GameEvent::Power(super::super::PowerEvent::Blinked { player: id, from: feet, to, afterimage: false }));
                    if strike > 0.0 {
                        let c = self.players[i].hull(&base).center();
                        self.toolbox_burst(i, c, 2.6, strike.clamp(0.0, 40.0), 10.0 + strike * 0.6, Element::Light);
                    }
                    return Ok(());
                }
                // The marked enemy is gone / nowhere clear: the marker is spent; throw a fresh one.
                None => self.players[i].power.live.mark = None,
            }
        }
        // Throw.
        let (eye, look) = self.toolbox_eye_look(i);
        let range = range.clamp(2.0, 60.0);
        let secs = secs.clamp(1.0, 30.0);
        let (at, point, on_player) = match self.toolbox_target(i, range, 0.0, true) {
            Some(k) => {
                let t = self.players[k].id;
                (MarkAt::Player(t), self.players[k].hull(&base).center(), true)
            }
            None => {
                let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(id));
                match raycast_world(&cw, eye, look, range, false) {
                    Some(h) => {
                        // Back off the surface so a recall lands you beside it, feet down.
                        let stand = h.point + h.normal * 0.6 - Vec3::Y * 0.9;
                        (MarkAt::Point(stand), h.point, false)
                    }
                    None => {
                        let p = eye + look * range;
                        (MarkAt::Point(p - Vec3::Y * 0.9), p, false)
                    }
                }
            }
        };
        if let MarkAt::Player(t) = at {
            // A credit touch (dodge-able: a dodged marker falls where they stood).
            let k = self.pidx(t).unwrap_or(i);
            if k != i && !self.toolbox_touch(i, k, crate::combat::HitKind::Thrown) {
                let p = self.players[k].mv.pos;
                self.players[i].power.live.mark = Some(MarkState { at: MarkAt::Point(p), t: secs });
                self.toolbox_fx(id, ToolboxFx::Mark, eye + look * 0.5, p, secs, 0.0);
                return Ok(());
            }
        }
        self.players[i].power.live.mark = Some(MarkState { at, t: secs });
        self.toolbox_fx(id, ToolboxFx::Mark, eye + look * 0.5, point, secs, if on_player { 1.0 } else { 0.0 });
        Ok(())
    }

    pub(crate) fn step_mark(&mut self, i: usize, dt: f32) {
        let Some(mut m) = self.players[i].power.live.mark else { return };
        m.t -= dt;
        let gone = match m.at {
            MarkAt::Player(t) => self.pidx(t).is_none_or(|k| !self.players[k].alive),
            MarkAt::Point(_) => false,
        };
        self.players[i].power.live.mark = if m.t <= 0.0 || gone { None } else { Some(m) };
    }
}
