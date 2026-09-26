//! Incremental terrain hash for desync detection.
//!
//! `VoxelWorld::content_hash` walks every chunk (~5 ms on the archipelago): far too slow to run
//! mid-match on the main thread. This hash is the XOR of per-chunk hashes (chunk coordinate +
//! contents, full avalanche), so after the one-off full pass (done on the world-build worker) only
//! chunks marked dirty since the last check are rehashed — microseconds for typical carves.
//! The sim world's dirty list is otherwise unused (render mirrors keep their own), so draining it
//! here is safe. Host and client compute it at the same point of the reliable event stream.

use std::collections::HashMap;

use bd_core::IVec3;
use bd_voxel::VoxelWorld;

#[inline]
fn mix(h: u64, b: u64) -> u64 {
    let mut z = (h ^ b).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn chunk_hash(w: &VoxelWorld, c: IVec3) -> u64 {
    let Some(data) = w.chunk_data(c) else { return 0 };
    let mut h = mix(0xcbf2_9ce4_8422_2325, ((c.x as u32 as u64) << 42) ^ ((c.y as u32 as u64) << 21) ^ (c.z as u32 as u64));
    for word in data.chunks_exact(8) {
        let mut b = [0u8; 8];
        b.copy_from_slice(word);
        h = mix(h, u64::from_le_bytes(b));
    }
    h
}

#[derive(Default)]
pub struct TerrainHasher {
    per_chunk: HashMap<IVec3, u64>,
    acc: u64,
    scratch: Vec<IVec3>,
}

impl TerrainHasher {
    /// Hash everything (slow: do it on a worker right after generating / building the world).
    pub fn full(w: &mut VoxelWorld) -> Self {
        w.drain_dirty();
        let mut s = Self::default();
        let chunks: Vec<IVec3> = w.nonempty_chunks().collect();
        for c in chunks {
            let h = chunk_hash(w, c);
            s.per_chunk.insert(c, h);
            s.acc ^= h;
        }
        s
    }

    /// Rehash the chunks edited since the last call and return the terrain hash.
    pub fn update(&mut self, w: &mut VoxelWorld) -> u64 {
        w.drain_dirty_into(&mut self.scratch);
        for &c in &self.scratch {
            let new = chunk_hash(w, c);
            let old = if new == 0 { self.per_chunk.remove(&c).unwrap_or(0) } else { self.per_chunk.insert(c, new).unwrap_or(0) };
            self.acc ^= old ^ new;
        }
        self.scratch.clear();
        self.acc
    }

    #[allow(dead_code)]
    pub fn value(&self) -> u64 {
        self.acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_voxel::{CarveOp, CarveShape};

    #[test]
    fn incremental_equals_full() {
        let (mut a, _) = bd_voxel::generate_flat_test(64.0);
        let mut h = TerrainHasher::full(&mut a);
        let (mut b, _) = bd_voxel::generate_flat_test(64.0);
        assert_eq!(h.value(), TerrainHasher::full(&mut b).value());
        for (i, x) in [-10.0f32, 0.0, 7.5, 20.0].iter().enumerate() {
            let op = CarveOp { shape: CarveShape::Sphere { center: bd_core::Vec3::new(*x, 8.0, 3.0), radius: 2.5 }, power: 2.0, seed: i as u32 + 1 };
            let _ = bd_sim::world::apply_carve_op(&mut a, &op);
            let inc = h.update(&mut a);
            let full = TerrainHasher::full(&mut a).value();
            assert_eq!(inc, full, "carve {i}");
            let _ = bd_sim::world::apply_carve_op(&mut b, &op);
        }
        assert_eq!(h.update(&mut a), TerrainHasher::full(&mut b).value());
        // Different terrain ⇒ different hash.
        let (mut c, _) = bd_voxel::generate_flat_test(64.0);
        assert_ne!(TerrainHasher::full(&mut c).value(), h.value());
    }
}
