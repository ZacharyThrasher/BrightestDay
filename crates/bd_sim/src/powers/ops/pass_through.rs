//! `MoveOp::PassThrough` — step THROUGH the wall in front of you (docs/toolbox/OPS.md, VERB GRAMMAR).
//!
//! "phase through the wall", "walk through walls", "teleport through the floor": the terrain under
//! the crosshair within [`REACH`] m (flat look; looking steeply down / up = through the floor /
//! ceiling) is crossed to the first open air on its far side, at most `depth` m of rock. You keep
//! your momentum and come out the other side airborne. No wall ahead, or no air within `depth` =
//! the move fails (nothing spent). Deterministic, allocation-free; nothing runs unless used.

use bd_core::Vec3;

use super::super::{PowerError, PowerEvent};
use super::flat;
use crate::events::GameEvent;
use crate::movement::{ColliderId, CollisionWorld, raycast_world};
use crate::world::World;

/// How far ahead the wall may be (m).
pub const REACH: f32 = 3.5;
/// Thickest rock crossed (m).
pub const MAX_DEPTH: f32 = 16.0;

impl World {
    pub(crate) fn op_pass_through(&mut self, i: usize, depth: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let hull = self.players[i].hull(&base);
        let h = hull.half().y;
        let (_, look) = self.op_eye_look(i);
        let dir = if look.y.abs() > 0.6 { look.normalize_or(Vec3::NEG_Y) } else { flat(look, Vec3::NEG_Z) };
        let start = hull.center();
        let hit = {
            let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(ColliderId::Player(id));
            raycast_world(&cw, start, dir, REACH, false)
        };
        let Some(hit) = hit else { return Err(PowerError::NoTarget) };
        let depth = if depth.is_finite() { depth.clamp(1.0, MAX_DEPTH) } else { 8.0 };
        let free = |w: &World, feet: Vec3| [0.3, h, h * 2.0 - 0.2].iter().all(|y| w.voxels.get_at(feet + Vec3::Y * *y) == 0) && w.in_play(feet, 0.0);
        let mut to = None;
        let mut d = 0.8;
        while d <= depth + 0.01 {
            let q = hit.point + dir * d;
            // centre → feet (through a floor: land the head below it)
            let feet = if dir.y < -0.6 { q - Vec3::Y * (h * 2.0) } else { q - Vec3::Y * h };
            if free(self, feet) {
                to = Some(feet);
                break;
            }
            d += 0.5;
        }
        let Some(to) = to else { return Err(PowerError::NoTarget) };
        let from = self.players[i].mv.pos;
        let p = &mut self.players[i];
        p.mv.pos = to;
        p.mv.grounded = false;
        self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: id, from, to, afterimage: true }));
        Ok(())
    }
}
