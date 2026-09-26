//! Destruction: carving shapes out of the world, and structural collapse.
//!
//! **Determinism contract.** Carves are replicated over the network as [`CarveOp`]s and applied
//! independently by every client, so the result must be bit-identical everywhere given the same
//! world and the same op. Hence: no global RNG, no hash-map iteration order, voxels visited in a fixed
//! (y, z, x) order, edge noise from integer hashes of the voxel coordinate + op seed, and only basic
//! IEEE float arithmetic (`+ - * / sqrt floor`, which Rust never fuses or reorders).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::hash::{BuildHasherDefault, Hasher};

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{AIR, Aabb, Material, voxel_center};
use glam::{IVec3, Vec3};
use serde::{Deserialize, Serialize};

use crate::material::material_def;
use crate::noise;
use crate::world::VoxelWorld;

/// A voxel removed by a carve (or by collapse).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RemovedVoxel {
    pub pos: IVec3,
    pub material: Material,
}

/// The volume a carve removes. All coordinates are world metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CarveShape {
    Sphere {
        center: Vec3,
        radius: f32,
    },
    Box {
        min: Vec3,
        max: Vec3,
    },
    /// Swept sphere from `a` to `b` — a trench torn through terrain by something big moving fast.
    Capsule {
        a: Vec3,
        b: Vec3,
        radius: f32,
    },
}

/// A replicable destruction op: identical inputs ⇒ identical edits on every client.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CarveOp {
    pub shape: CarveShape,
    /// Destructive strength relative to material hardness (1.0 fully removes stone within the shape).
    pub power: f32,
    /// Seed for the noisy edge. Any value; use something the host picks (e.g. op counter).
    pub seed: u32,
}

/// A connected piece of terrain that lost its support and was removed from the world. The client/sim
/// turns it into a falling rigid body or a burst of debris cubes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FallingChunk {
    pub voxels: Vec<(IVec3, Material)>,
}

impl FallingChunk {
    pub fn len(&self) -> usize {
        self.voxels.len()
    }
    pub fn is_empty(&self) -> bool {
        self.voxels.is_empty()
    }
    /// Inclusive voxel bounds.
    pub fn voxel_bounds(&self) -> (IVec3, IVec3) {
        let mut lo = IVec3::MAX;
        let mut hi = IVec3::MIN;
        for (v, _) in &self.voxels {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        (lo, hi)
    }
    /// World-space bounds in metres.
    pub fn aabb(&self) -> Aabb {
        let (lo, hi) = self.voxel_bounds();
        Aabb { min: lo.as_vec3() * VOXEL_SIZE, max: (hi + IVec3::ONE).as_vec3() * VOXEL_SIZE }
    }
    /// Centre of mass in metres (uniform density).
    pub fn center_of_mass(&self) -> Vec3 {
        if self.voxels.is_empty() {
            return Vec3::ZERO;
        }
        let sum: Vec3 = self.voxels.iter().map(|(v, _)| voxel_center(*v)).sum();
        sum / self.voxels.len() as f32
    }
}

/// Everything a carve changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CarveOutcome {
    pub removed: Vec<RemovedVoxel>,
    pub falling: Vec<FallingChunk>,
}

/// Bounds on collapse detection so a carve can never stall a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollapseLimits {
    /// A connected piece larger than this is assumed to be supported (and stays put, floating
    /// Cube-World style). Big enough for a giant tree's canopy.
    pub max_component: usize,
    /// Total voxels visited per call across all flood fills; remaining seeds are assumed supported.
    pub max_total: usize,
}

impl Default for CollapseLimits {
    fn default() -> Self {
        Self { max_component: 32_768, max_total: 131_072 }
    }
}

/// Voxels whose material hardness exceeds `power` by this factor are untouched.
const MIN_POWER_RATIO: f32 = 0.15;
/// Soft materials are carved a little beyond the nominal shape (up to this fraction of its size).
const MAX_SOFT_BONUS: f32 = 0.15;

impl CarveShape {
    /// Signed distance (voxel units, negative inside) from voxel-space point `p`, plus the shape's
    /// characteristic size (voxel units) used to scale edge noise and hardness falloff.
    #[inline]
    fn sdf_vox(&self, p: Vec3) -> f32 {
        let k = 1.0 / VOXEL_SIZE;
        match *self {
            CarveShape::Sphere { center, radius } => (p - center * k).length() - radius * k,
            CarveShape::Box { min, max } => {
                let (lo, hi) = (min * k, max * k);
                let c = (lo + hi) * 0.5;
                let h = (hi - lo) * 0.5;
                let q = (p - c).abs() - h;
                q.max(Vec3::ZERO).length() + q.max_element().min(0.0)
            }
            CarveShape::Capsule { a, b, radius } => {
                let (a, b) = (a * k, b * k);
                let ab = b - a;
                let len2 = ab.length_squared();
                let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
                (p - (a + ab * t)).length() - radius * k
            }
        }
    }

    /// All coordinates (and the extents derived from them) are finite. Carves failing this are
    /// ignored: a NaN/inf op arriving over the network must be a no-op on every client, never a
    /// stray hole at the origin.
    fn is_finite(&self) -> bool {
        match *self {
            CarveShape::Sphere { center, radius } => center.is_finite() && radius.is_finite(),
            CarveShape::Box { min, max } => min.is_finite() && max.is_finite() && (max - min).is_finite(),
            CarveShape::Capsule { a, b, radius } => a.is_finite() && b.is_finite() && radius.is_finite() && (b - a).is_finite(),
        }
    }

    fn size_vox(&self) -> f32 {
        let k = 1.0 / VOXEL_SIZE;
        match *self {
            CarveShape::Sphere { radius, .. } | CarveShape::Capsule { radius, .. } => radius * k,
            CarveShape::Box { min, max } => ((max - min) * 0.5 * k).min_element(),
        }
    }

    /// Inclusive voxel bounds, padded by `pad` voxels.
    fn voxel_bounds(&self, pad: f32) -> (IVec3, IVec3) {
        let k = 1.0 / VOXEL_SIZE;
        let (lo, hi) = match *self {
            CarveShape::Sphere { center, radius } => (center - radius, center + radius),
            CarveShape::Box { min, max } => (min.min(max), max.max(min)),
            CarveShape::Capsule { a, b, radius } => (a.min(b) - radius, a.max(b) + radius),
        };
        ((lo * k - pad).floor().as_ivec3(), (hi * k + pad).floor().as_ivec3())
    }

    /// Voxel always removed (if the material allows) so tiny carves still bite.
    fn core_voxel(&self) -> IVec3 {
        let k = 1.0 / VOXEL_SIZE;
        match *self {
            CarveShape::Sphere { center, .. } => (center * k).floor().as_ivec3(),
            CarveShape::Box { min, max } => ((min + max) * 0.5 * k).floor().as_ivec3(),
            CarveShape::Capsule { a, .. } => (a * k).floor().as_ivec3(),
        }
    }
}

impl VoxelWorld {
    /// Carve a sphere (metres). Returns removed voxels in deterministic (y, z, x) order.
    pub fn carve_sphere(&mut self, center: Vec3, radius: f32, power: f32, seed: u32) -> Vec<RemovedVoxel> {
        self.carve(&CarveOp { shape: CarveShape::Sphere { center, radius }, power, seed })
    }

    /// Carve an axis-aligned box (metres).
    pub fn carve_box(&mut self, min: Vec3, max: Vec3, power: f32, seed: u32) -> Vec<RemovedVoxel> {
        self.carve(&CarveOp { shape: CarveShape::Box { min, max }, power, seed })
    }

    /// Carve a capsule (swept sphere) from `a` to `b` (metres).
    pub fn carve_capsule(&mut self, a: Vec3, b: Vec3, radius: f32, power: f32, seed: u32) -> Vec<RemovedVoxel> {
        self.carve(&CarveOp { shape: CarveShape::Capsule { a, b, radius }, power, seed })
    }

    /// Apply a carve op (no collapse). Hardness: a voxel of hardness `h` is removed where
    /// `sdf <= edge_noise - (1 - sqrt(clamp(power / h, 0, 1 + bonus))) * size`, i.e. harder
    /// materials survive in a thicker rim, very soft ones (leaves, sand) get chewed slightly beyond
    /// the shape, and anything with `power / h < 0.15` (or indestructible) is untouched.
    pub fn carve(&mut self, op: &CarveOp) -> Vec<RemovedVoxel> {
        let mut out = Vec::new();
        self.carve_into(op, &mut out);
        out
    }

    /// Like [`Self::carve`] but appends into `out` (no allocation when reused).
    pub fn carve_into(&mut self, op: &CarveOp, out: &mut Vec<RemovedVoxel>) {
        let shape = op.shape;
        let size = shape.size_vox().max(0.0);
        if !(shape.is_finite() && size.is_finite() && op.power.is_finite()) || op.power <= 0.0 {
            return;
        }
        // Edge noise amplitude grows gently with size; small carves stay crisp.
        let edge = (0.35 + 0.08 * size).min(2.5);
        let soft_bonus = MAX_SOFT_BONUS * size;
        let (lo, hi) = shape.voxel_bounds(edge + soft_bonus + 1.0);
        let (lo, hi) = (lo.max(self.voxel_min()), hi.min(self.voxel_max() - IVec3::ONE));
        let core = shape.core_voxel();
        let lf = 0.35; // low-frequency noise scale (per voxel)
        let start = out.len();

        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let v = IVec3::new(x, y, z);
                    let m = self.get(v);
                    if m == AIR {
                        continue;
                    }
                    let def = material_def(m);
                    if def.indestructible {
                        continue;
                    }
                    let ratio = op.power / def.hardness;
                    if ratio < MIN_POWER_RATIO {
                        continue;
                    }
                    let p = v.as_vec3() + Vec3::splat(0.5);
                    let sd = shape.sdf_vox(p);
                    let remove = if v == core && size >= 0.5 {
                        true
                    } else {
                        let k = ratio.min(1.0 + MAX_SOFT_BONUS * 2.0).sqrt();
                        let penalty = (1.0 - k) * size;
                        let n = 0.6 * noise::value3(p.x * lf, p.y * lf, p.z * lf, op.seed)
                            + 0.4 * noise::signed(noise::hash_v(v, op.seed ^ 0xa5a5_1234));
                        sd <= n * edge - penalty
                    };
                    if remove {
                        self.set_raw(v, AIR);
                        out.push(RemovedVoxel { pos: v, material: m });
                    }
                }
            }
        }
        if out.len() > start {
            self.mark_region_dirty(lo, hi);
        }
    }

    /// Carve and then detect/remove unsupported pieces — the one-call entry point for gameplay.
    pub fn apply_carve(&mut self, op: &CarveOp) -> CarveOutcome {
        let removed = self.carve(op);
        let falling = self.detect_collapse(&removed, CollapseLimits::default());
        CarveOutcome { removed, falling }
    }

    /// After voxels were removed, find connected solid pieces next to the removed voxels that can no
    /// longer reach an anchor (an indestructible voxel), remove them from the world and return them.
    ///
    /// Search is best-first towards the nearest registered anchor hint (so supported pieces usually
    /// resolve after a few hundred voxels) and bounded by `limits` (pieces too big to finish are
    /// assumed supported). Fully deterministic: seeds are sorted, neighbours visited in fixed order,
    /// ties broken by discovery order.
    pub fn detect_collapse(&mut self, removed: &[RemovedVoxel], limits: CollapseLimits) -> Vec<FallingChunk> {
        let mut seeds: Vec<IVec3> = Vec::with_capacity(removed.len());
        for r in removed {
            for d in NEIGHBOURS {
                let n = r.pos + d;
                if self.get(n) != AIR {
                    seeds.push(n);
                }
            }
        }
        self.detect_collapse_from_seeds(seeds, limits)
    }

    /// Collapse detection starting from explicit seed voxels (any order; they're sorted).
    pub fn detect_collapse_from_seeds(&mut self, mut seeds: Vec<IVec3>, limits: CollapseLimits) -> Vec<FallingChunk> {
        seeds.sort_unstable_by_key(|v| (v.y, v.z, v.x));
        seeds.dedup();
        let mut falling = Vec::new();
        // voxel → id of the flood that claimed it. Membership-only; never iterated.
        let mut owner: FastMap<IVec3, u32> = FastMap::default();
        let mut heap: BinaryHeap<Reverse<u64>> = BinaryHeap::new();
        let mut nodes: Vec<IVec3> = Vec::new();
        let mut total = 0usize;

        for (flood_id, seed) in seeds.into_iter().enumerate() {
            let flood_id = flood_id as u32;
            if total >= limits.max_total {
                break;
            }
            let m = self.get(seed);
            if m == AIR || owner.contains_key(&seed) {
                continue;
            }
            if material_def(m).indestructible {
                continue;
            }
            let target = self.nearest_anchor_hint(seed);
            let prio = |v: IVec3| -> u64 {
                match target {
                    Some(t) => {
                        let d = (v - t).abs();
                        (d.x + d.y + d.z) as u64
                    }
                    None => 0,
                }
            };
            heap.clear();
            nodes.clear();
            owner.insert(seed, flood_id);
            nodes.push(seed);
            heap.push(Reverse(prio(seed) << 32));
            let mut supported = false;

            'flood: while let Some(Reverse(key)) = heap.pop() {
                let v = nodes[(key & 0xffff_ffff) as usize];
                for d in NEIGHBOURS {
                    let n = v + d;
                    let nm = self.get(n);
                    if nm == AIR {
                        continue;
                    }
                    if material_def(nm).indestructible {
                        supported = true;
                        break 'flood;
                    }
                    match owner.get(&n) {
                        Some(&id) if id != flood_id => {
                            // Touches a piece an earlier flood proved (or assumed) supported.
                            supported = true;
                            break 'flood;
                        }
                        Some(_) => continue,
                        None => {}
                    }
                    if nodes.len() >= limits.max_component {
                        supported = true;
                        break 'flood;
                    }
                    owner.insert(n, flood_id);
                    let seq = nodes.len() as u64;
                    nodes.push(n);
                    heap.push(Reverse((prio(n) << 32) | seq));
                }
            }
            total += nodes.len();

            if !supported {
                let mut chunk = FallingChunk { voxels: Vec::with_capacity(nodes.len()) };
                // Sorted order so consumers see a canonical list regardless of search order.
                nodes.sort_unstable_by_key(|v| (v.y, v.z, v.x));
                for &v in &nodes {
                    let m = self.set_raw(v, AIR);
                    chunk.voxels.push((v, m));
                    self.mark_voxel_dirty(v);
                    owner.remove(&v);
                }
                falling.push(chunk);
            }
        }
        falling
    }

    /// Remove every solid voxel that is not 6-connected to an indestructible voxel. Whole-world
    /// flood (tens of ms): a generation-time cleanup so no stray speck is left floating unanchored.
    /// Returns the number of voxels removed.
    pub fn prune_unanchored(&mut self) -> usize {
        let (lo, hi) = (self.voxel_min(), self.voxel_max());
        let size = hi - lo;
        let idx = |v: IVec3| {
            let r = v - lo;
            (r.x + size.x * (r.z + size.z * r.y)) as usize
        };
        let mut seen = vec![false; (size.x * size.y * size.z) as usize];
        let mut stack: Vec<IVec3> = Vec::new();
        let chunks: Vec<IVec3> = self.nonempty_chunks().collect();
        let each_voxel = |c: IVec3, f: &mut dyn FnMut(IVec3)| {
            let o = crate::world::chunk_origin(c);
            for i in 0..crate::world::CHUNK_VOLUME as i32 {
                f(o + IVec3::new(i & 31, i >> 10, (i >> 5) & 31));
            }
        };
        for &c in &chunks {
            each_voxel(c, &mut |v| {
                if material_def(self.get(v)).indestructible {
                    seen[idx(v)] = true;
                    stack.push(v);
                }
            });
        }
        while let Some(v) = stack.pop() {
            for d in NEIGHBOURS {
                let n = v + d;
                if self.in_bounds(n) && !seen[idx(n)] && self.get(n) != AIR {
                    seen[idx(n)] = true;
                    stack.push(n);
                }
            }
        }
        let mut orphans = Vec::new();
        for &c in &chunks {
            each_voxel(c, &mut |v| {
                if self.get(v) != AIR && !seen[idx(v)] {
                    orphans.push(v);
                }
            });
        }
        for &v in &orphans {
            self.set(v, AIR);
        }
        orphans.len()
    }

    fn nearest_anchor_hint(&self, v: IVec3) -> Option<IVec3> {
        self.anchor_hints().iter().copied().min_by_key(|a| {
            let d = *a - v;
            (d.x as i64 * d.x as i64 + d.y as i64 * d.y as i64 + d.z as i64 * d.z as i64, a.y, a.z, a.x)
        })
    }
}

/// Face neighbours, fixed order (−Y first: support is usually below).
const NEIGHBOURS: [IVec3; 6] =
    [IVec3::new(0, -1, 0), IVec3::new(1, 0, 0), IVec3::new(-1, 0, 0), IVec3::new(0, 0, 1), IVec3::new(0, 0, -1), IVec3::new(0, 1, 0)];

/// FxHash-style hasher: fast for small integer keys. Only used for membership tests.
#[derive(Default, Clone, Copy)]
struct FastHasher(u64);

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u64(*b as u64);
        }
    }
    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.write_u64(i as u32 as u64);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(0x51_7cc1_b727_220a_95);
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;
