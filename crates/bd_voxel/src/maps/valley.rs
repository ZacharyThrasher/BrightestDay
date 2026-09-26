//! **Valley of the End** (feature `valley`) — work in progress.

use glam::IVec3;

use super::MapInfo;
use crate::world::VoxelWorld;

/// World dimensions in voxels.
pub const VALLEY_SIZE_VOXELS: IVec3 = IVec3::new(384, 192, 448);

/// Build the Valley of the End. Deterministic for a given seed.
pub fn generate_valley(_seed: u64) -> (VoxelWorld, MapInfo) {
    unimplemented!("valley: in progress")
}
