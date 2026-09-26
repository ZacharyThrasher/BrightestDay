//! **Water-walk** (Valley of the End: chakra walking on the river). A map with
//! `bd_voxel::ArenaInfo::water_walk` turns each of its `water_sheets` into a solid, flat-topped box
//! collider for *player movement*: run, bunny-hop, slide and land on the water like any floor.
//!
//! * Pushed every tick next to the surf lines, by the host (`World::tick`) and by client prediction
//!   (`World::apply_snapshot`) from the same generated map data, so prediction collides identically.
//! * Not anchorable: a tether fired at the water is blocked by it (you can't hook a river).
//! * Constructs and projectiles don't use these colliders: they sink to the voxel bed.
//! * Maps without `water_walk` push nothing — every other map is bit-identical.

use bd_core::{Quat, Vec3};

use super::world::{Collider, ColliderId};

/// Collider ids of map water sheets start here (`ColliderId::Other`).
pub const WATER_ID_BASE: u32 = 0x5741_0000;

/// Append the map's walkable water (if `arena.water_walk`) as colliders: one box per sheet, top face
/// at the water surface, as deep as the water (flush with the bed, so nothing slips underneath).
pub fn push_water_colliders(arena: &bd_voxel::ArenaInfo, out: &mut Vec<Collider>) {
    if !arena.water_walk {
        return;
    }
    for (i, s) in arena.water_sheets.iter().enumerate() {
        let d = if s.depth.is_finite() { s.depth.clamp(0.25, 8.0) } else { 1.0 };
        let center = Vec3::new(s.center.x, s.center.y - d * 0.5, s.center.z);
        let mut c = Collider::new(center, Vec3::new(s.half.x, d * 0.5, s.half.y), Quat::from_rotation_y(s.yaw), ColliderId::Other(WATER_ID_BASE + i as u32));
        c.anchorable = false;
        out.push(c);
    }
}

/// Is `id` one of the map's water sheets?
#[inline]
pub fn is_water(id: ColliderId) -> bool {
    matches!(id, ColliderId::Other(i) if (WATER_ID_BASE..WATER_ID_BASE + 0x1_0000).contains(&i))
}
