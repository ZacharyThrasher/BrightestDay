//! `MoveOp::GazeStun` — it only works on someone looking at you (docs/toolbox/OPS.md). Genjutsu,
//! Tsukuyomi, a blinding flash, a petrifying stare.
//!
//! Every living enemy within `range` m with a clear line to your eyes whose view points at you
//! (within `cone`° of their look) gets `status` for `secs` — `aimed: true` = only the enemy under
//! your crosshair (aim-forgiving). The counterplay is the point: look away (or dodge — the gaze is a
//! dodge-able credit touch) and it does nothing. Short and escapable: Stun is capped at
//! [`STUN_CAP`] s, anything else at [`OTHER_CAP`] s. A radial flash always goes off (the flare is
//! the tell); an aimed gaze with nobody to catch fails (nothing spent).
//! Deterministic, allocation-free.

use bd_core::MAX_PLAYERS;

use super::super::PowerError;
use super::live::ToolboxFx;
use crate::combat::HitKind;
use crate::status::StatusKind;
use crate::world::World;

pub const STUN_CAP: f32 = 1.2;
pub const OTHER_CAP: f32 = 4.0;

impl World {
    pub(crate) fn op_gaze_stun(&mut self, i: usize, range: f32, cone: f32, secs: f32, status: StatusKind, aimed: bool) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let my_eye = self.players[i].eye(&base);
        let range = super::fclamp(range, 1.0, 60.0);
        let cos = super::fclamp(cone, 5.0, 90.0).to_radians().cos();
        let secs = if status == StatusKind::Stun { super::fclamp(secs, 0.1, STUN_CAP) } else { super::fclamp(secs, 0.1, OTHER_CAP) };
        let mut victims = [None; MAX_PLAYERS];
        let mut n = 0;
        let only = if aimed {
            match self.toolbox_target(i, range, 8.0, true) {
                Some(k) => Some(k),
                None => return Err(PowerError::NoTarget),
            }
        } else {
            None
        };
        for k in 0..self.players.len() {
            if k == i || !self.players[k].alive || n >= victims.len() || only.is_some_and(|o| o != k) {
                continue;
            }
            let q = &self.players[k];
            let eye = q.eye(&base);
            let d = my_eye - eye;
            let len = d.length();
            if !(0.3..=range).contains(&len) {
                continue;
            }
            // Are they looking at you?
            if q.look().dot(d / len) < cos {
                continue;
            }
            if self.toolbox_blocked(i, my_eye, eye) {
                continue;
            }
            victims[n] = Some(k);
            n += 1;
        }
        if !aimed {
            self.toolbox_fx(id, ToolboxFx::Flash, my_eye, my_eye, secs, range);
        }
        let mut caught = 0;
        for k in victims.iter().flatten().copied() {
            if !self.toolbox_touch(i, k, HitKind::RingShove) {
                continue;
            }
            self.apply_status(k, status, secs, Some(id));
            if status == StatusKind::Stun {
                // A dazed mind also sees darkness at the edges.
                self.apply_status(k, StatusKind::Shadow, secs + 0.6, Some(id));
            }
            let to = self.players[k].eye(&base);
            self.toolbox_fx(id, ToolboxFx::Gaze, my_eye, to, secs, 0.0);
            caught += 1;
        }
        if aimed && caught == 0 {
            return Err(PowerError::NoTarget);
        }
        Ok(())
    }
}
