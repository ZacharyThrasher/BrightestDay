//! `SelfResize { scale, secs }` — you grow (> 1) or shrink (< 1) for `secs` (docs/toolbox/OPS.md):
//! Hanuman swelling to a giant, Alice's drink-me / eat-me, a titan's stature.
//!
//! The existing player resize does everything (`World::resize_player`, space.rs `SizeState`): a
//! grown body is bigger, reaches further, hits harder in melee, is pushed less and runs a bit
//! slower; a shrunk one is quick, light and fragile. Clamped to the resize range (0.5 … 1.6) and
//! 1 … 20 s. Stateless; replicated by the ordinary `Resized` event + player snapshots.

use crate::powers::PowerError;
use crate::world::World;

impl World {
    pub(crate) fn op_self_resize(&mut self, i: usize, scale: f32, secs: f32) -> Result<(), PowerError> {
        let scale = if scale.is_finite() && scale > 0.0 { scale.clamp(crate::space::SHRINK_SCALE, crate::space::GROW_SCALE) } else { crate::space::GROW_SCALE };
        let secs = if secs.is_finite() && secs > 0.0 { secs.clamp(1.0, 20.0) } else { 8.0 };
        let id = self.players[i].id;
        self.resize_player(id, scale, secs);
        Ok(())
    }
}
