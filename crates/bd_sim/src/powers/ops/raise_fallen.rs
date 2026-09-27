//! `MoveOp::RaiseFallen` (docs/toolbox/OPS.md): raise the fallen as your soldiers.
//!
//! Every player KO'd near you who hasn't respawned yet (within `radius`; their body's spot, on
//! solid ground) rises as one `recipe` creature of yours — Arise. Nobody fallen nearby = the
//! recipe is cast normally along your aim (its authored `count`). Each soldier is an ordinary cast
//! (it pays its own presence and obeys the creature slot limits). Deterministic (player order);
//! stateless — a dead player's body keeps its KO position until the respawn timer ends.

use bd_core::{MAX_PLAYERS, Vec3};

use super::super::{PowerError, RecipeDef};
use crate::world::{Aim, World};

/// Most soldiers one Arise raises from bodies.
pub const MAX_RAISED: usize = 6;

impl World {
    pub(crate) fn op_raise_fallen(&mut self, i: usize, recipe: &RecipeDef, count: u8, radius: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let me = self.players[i].mv.pos;
        let radius = if radius > 0.0 { radius.min(60.0) } else { 30.0 };
        let want = (count.max(1) as usize).min(MAX_RAISED);
        let mut spots = [None::<Vec3>; MAX_PLAYERS];
        let mut n = 0;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || q.alive || q.eliminated || n >= want.min(spots.len()) {
                continue;
            }
            let p = q.mv.pos;
            if !p.is_finite() || (p - me).length() > radius || !self.in_play(p, 0.0) {
                continue;
            }
            // Only a body lying on something (a void KO leaves nothing to raise).
            if bd_core::voxel::raycast(&self.voxels, p + Vec3::Y * 1.0, Vec3::NEG_Y, 8.0).is_none() {
                continue;
            }
            spots[n] = Some(p);
            n += 1;
        }
        if n == 0 {
            let (eye, look) = self.op_eye_look(i);
            return match self.cast(id, recipe.to_recipe(), None, Aim::new(eye, look)) {
                Ok(_) => Ok(()),
                Err(crate::events::CastError::NotEnoughWillpower { .. }) => Err(PowerError::NotEnoughWillpower),
                Err(_) => Err(PowerError::Busy),
            };
        }
        let mut raised = 0;
        for p in spots.iter().flatten().copied() {
            let mut r = recipe.clone();
            r.count = 1;
            // Straight down onto the body: it stands up where they fell.
            if self.cast(id, r.to_recipe(), None, Aim::new(p + Vec3::Y * 4.0, Vec3::NEG_Y)).is_ok() {
                raised += 1;
            }
        }
        if raised == 0 { Err(PowerError::Busy) } else { Ok(()) }
    }
}
