//! Animating the world (COMBAT.md §3c): lift a connected piece of terrain out of the island so a
//! graft can turn it into a construct (a tree with legs, a boulder with wings).
//!
//! [`ExtractOp`] is replicated like a `CarveOp`: every client applies [`apply_extract`] to its own
//! identical voxel world and gets the identical [`VoxelModel`] (voxels + original materials), so the
//! model never travels over the network.
//!
//! The cut: breadth-first flood from the aimed voxel through destructible solid voxels, restricted
//! to `y ≥ min_y` (a horizontal cut plane a little below the aim — trunks are severed, islands are
//! not hollowed out) and to a cube of half-size `radius` around the seed, taking at most
//! `max_voxels` in BFS order (a roughly spherical bite when the piece is part of something huge).
//! Neighbours are visited in a fixed order ⇒ deterministic. Pieces left floating are dropped via
//! bd_voxel's collapse detection (also deterministic).

use std::collections::VecDeque;

use bd_core::voxel::{AIR, Material};
use bd_core::IVec3;
use bd_voxel::{FallingChunk, RemovedVoxel, VoxelWorld, material_def};
use serde::{Deserialize, Serialize};

use crate::constructs::VoxelModel;

/// Max voxels lifted by one extraction (~6k per design).
pub const EXTRACT_MAX_VOXELS: u32 = 6000;
/// Half-size (voxels) of the region the flood may reach (12 m).
pub const EXTRACT_RADIUS: i32 = 24;
/// The cut plane sits this many voxels below the aimed voxel.
pub const EXTRACT_CUT_DEPTH: i32 = 3;

/// A replicated terrain extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExtractOp {
    /// Aimed voxel (flood seed).
    pub seed: IVec3,
    /// Cut plane: only voxels with `y >= min_y` are taken.
    pub min_y: i32,
    pub radius: i32,
    pub max_voxels: u32,
}

impl ExtractOp {
    /// The standard op for a graft aimed at voxel `v`.
    pub fn at(v: IVec3) -> Self {
        Self { seed: v, min_y: v.y - EXTRACT_CUT_DEPTH, radius: EXTRACT_RADIUS, max_voxels: EXTRACT_MAX_VOXELS }
    }

    /// Clamp untrusted fields (snapshot / event decode).
    pub fn sanitized(mut self) -> Self {
        self.radius = self.radius.clamp(1, EXTRACT_RADIUS);
        self.max_voxels = self.max_voxels.clamp(1, EXTRACT_MAX_VOXELS);
        self.min_y = self.min_y.clamp(self.seed.y - EXTRACT_RADIUS, self.seed.y);
        self
    }
}

/// Result of an extraction.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtractOutcome {
    /// The lifted piece (empty if the seed wasn't extractable).
    pub model: VoxelModel,
    /// Terrain left unsupported by the cut (fall like any collapse).
    pub falling: Vec<FallingChunk>,
}

const NEIGHBOURS: [IVec3; 6] = [
    IVec3::new(0, -1, 0),
    IVec3::new(0, 1, 0),
    IVec3::new(-1, 0, 0),
    IVec3::new(1, 0, 0),
    IVec3::new(0, 0, -1),
    IVec3::new(0, 0, 1),
];

fn extractable(m: Material) -> bool {
    m != AIR && !material_def(m).indestructible
}

/// Apply an extraction to `world` (host and clients alike).
pub fn apply_extract(world: &mut VoxelWorld, op: &ExtractOp) -> ExtractOutcome {
    let op = op.sanitized();
    let mut out = ExtractOutcome::default();
    if !extractable(world.get(op.seed)) {
        return out;
    }
    let r = op.radius;
    let side = (2 * r + 1) as usize;
    let mut visited = vec![false; side * side * side];
    let idx = |v: IVec3| -> Option<usize> {
        let l = v - op.seed + IVec3::splat(r);
        if l.cmplt(IVec3::ZERO).any() || l.cmpge(IVec3::splat(2 * r + 1)).any() {
            return None;
        }
        Some((l.y as usize * side + l.z as usize) * side + l.x as usize)
    };
    let mut queue = VecDeque::new();
    let mut taken: Vec<(IVec3, Material)> = Vec::new();
    if let Some(i) = idx(op.seed) {
        visited[i] = true;
        queue.push_back(op.seed);
    }
    while let Some(v) = queue.pop_front() {
        if taken.len() as u32 >= op.max_voxels {
            break;
        }
        let m = world.get(v);
        taken.push((v, m));
        for d in NEIGHBOURS {
            let n = v + d;
            if n.y < op.min_y {
                continue;
            }
            let Some(i) = idx(n) else { continue };
            if visited[i] {
                continue;
            }
            visited[i] = true;
            if extractable(world.get(n)) {
                queue.push_back(n);
            }
        }
    }
    let mut removed = Vec::with_capacity(taken.len());
    let mut lo = IVec3::MAX;
    let mut hi = IVec3::MIN;
    for (v, m) in &taken {
        world.set(*v, AIR);
        removed.push(RemovedVoxel { pos: *v, material: *m });
        lo = lo.min(*v - op.seed);
        hi = hi.max(*v - op.seed);
    }
    out.model = VoxelModel {
        origin: op.seed,
        voxels: taken.iter().map(|(v, m)| (*v - op.seed, *m)).collect(),
        min: if taken.is_empty() { IVec3::ZERO } else { lo },
        max: if taken.is_empty() { IVec3::ZERO } else { hi },
    };
    out.falling = world.detect_collapse(&removed, crate::world::SIM_COLLAPSE_LIMITS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_voxel::mat;

    #[test]
    fn lifts_a_pillar_above_the_cut_and_is_deterministic() {
        let mk = || {
            let mut w = VoxelWorld::centered(IVec3::new(64, 64, 64));
            // (VoxelWorld::centered spans y ≥ 0.) Bedrock 0..1, stone 2..11, pillar 12..21.
            w.fill_box(IVec3::new(-20, 0, -20), IVec3::new(19, 1, 19), mat::CORE);
            w.fill_box(IVec3::new(-20, 2, -20), IVec3::new(19, 11, 19), mat::STONE);
            w.fill_box(IVec3::new(0, 12, 0), IVec3::new(1, 21, 1), mat::WOOD);
            w
        };
        let mut a = mk();
        let mut b = mk();
        let op = ExtractOp::at(IVec3::new(0, 17, 0));
        let oa = apply_extract(&mut a, &op);
        let ob = apply_extract(&mut b, &op);
        assert_eq!(oa.model, ob.model);
        assert_eq!(a.content_hash(), b.content_hash());
        // Pillar from y=14 (cut plane) to 21: 2×2×8 = 32 voxels; the stump stays.
        assert_eq!(oa.model.voxels.len(), 32);
        assert!(oa.model.voxels.iter().all(|(_, m)| *m == mat::WOOD));
        assert_eq!(a.get(IVec3::new(0, 13, 0)), mat::WOOD);
        assert_eq!(a.get(IVec3::new(0, 14, 0)), AIR);
    }

    #[test]
    fn huge_pieces_are_bounded() {
        let mut w = VoxelWorld::centered(IVec3::new(128, 64, 128));
        w.fill_box(IVec3::new(-60, 0, -60), IVec3::new(59, 0, 59), mat::CORE);
        w.fill_box(IVec3::new(-60, 1, -60), IVec3::new(59, 20, 59), mat::STONE);
        let o = apply_extract(&mut w, &ExtractOp::at(IVec3::new(0, 20, 0)));
        assert!(o.model.voxels.len() as u32 <= EXTRACT_MAX_VOXELS);
        assert!(!o.model.voxels.is_empty());
        let air = apply_extract(&mut w, &ExtractOp::at(IVec3::new(0, 40, 0)));
        assert!(air.model.voxels.is_empty());
    }
}
