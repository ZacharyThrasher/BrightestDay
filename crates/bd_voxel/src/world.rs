//! The chunked, bounded voxel world.
//!
//! Layout: the world covers a box of whole 32³ chunks. Each chunk is either unallocated (all air) or a
//! dense `[u8; 32768]` of material ids, indexed `x | z << 5 | y << 10`. Anything outside the box is air.
//!
//! Every mutating call records which chunks need remeshing (including neighbours when an edited voxel
//! sits on a chunk border, because face culling and AO look one voxel across). The client drains that
//! list with [`VoxelWorld::drain_dirty`] in a deterministic order.

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{AIR, Aabb, Material, VoxelQuery};
use glam::{IVec3, Vec3};

/// Chunk edge length in voxels.
pub const CHUNK_SIZE: i32 = 32;
pub const CHUNK_SHIFT: i32 = 5;
pub const CHUNK_MASK: i32 = CHUNK_SIZE - 1;
pub const CHUNK_VOLUME: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Local (0..32)³ coordinate → index inside a chunk.
#[inline]
pub fn local_index(l: IVec3) -> usize {
    (l.x | (l.z << CHUNK_SHIFT) | (l.y << (2 * CHUNK_SHIFT))) as usize
}

/// Chunk coordinate containing voxel `v`.
#[inline]
pub fn chunk_of(v: IVec3) -> IVec3 {
    IVec3::new(v.x >> CHUNK_SHIFT, v.y >> CHUNK_SHIFT, v.z >> CHUNK_SHIFT)
}

/// Voxel coordinate of a chunk's minimum corner.
#[inline]
pub fn chunk_origin(c: IVec3) -> IVec3 {
    c * CHUNK_SIZE
}

pub(crate) struct Chunk {
    pub(crate) voxels: Box<[Material; CHUNK_VOLUME]>,
    /// Number of non-air voxels (lets the mesher skip empty chunks).
    pub(crate) solid: u32,
    /// Render-only: bit set = this air voxel is sealed inside a closed building interior (see
    /// [`VoxelWorld::seal_enclosed_air`]). `None` = no enclosed air in this chunk.
    pub(crate) enclosed: Option<Box<[u64; CHUNK_VOLUME / 64]>>,
}

impl Chunk {
    fn new() -> Self {
        Self { voxels: Box::new([AIR; CHUNK_VOLUME]), solid: 0, enclosed: None }
    }
}

pub struct VoxelWorld {
    /// Minimum chunk coordinate.
    min_chunk: IVec3,
    /// Size in chunks.
    dims: IVec3,
    chunks: Vec<Option<Chunk>>,
    dirty_flag: Vec<bool>,
    dirty_list: Vec<IVec3>,
    /// Known indestructible voxels deep in each island. Only a *search hint* for collapse detection
    /// (best-first search heads towards the nearest one); correctness never depends on it.
    anchor_hints: Vec<IVec3>,
    /// Any chunk carries enclosed-air bits (render-only face culling of sealed interiors).
    has_enclosed: bool,
}

impl VoxelWorld {
    /// A world covering at least voxels `min .. min + size`. `min` is rounded down and the extent up
    /// to whole chunks.
    pub fn new(min: IVec3, size: IVec3) -> Self {
        assert!(size.cmpgt(IVec3::ZERO).all(), "world size must be positive");
        let min_chunk = chunk_of(min);
        let max_chunk = chunk_of(min + size - IVec3::ONE);
        let dims = max_chunk - min_chunk + IVec3::ONE;
        let n = (dims.x * dims.y * dims.z) as usize;
        Self {
            min_chunk,
            dims,
            chunks: (0..n).map(|_| None).collect(),
            dirty_flag: vec![false; n],
            dirty_list: Vec::new(),
            anchor_hints: Vec::new(),
            has_enclosed: false,
        }
    }

    /// A world horizontally centred on the origin: x/z in `-size/2 .. size/2`, y in `0 .. size.y`
    /// (voxel units).
    pub fn centered(size: IVec3) -> Self {
        Self::new(IVec3::new(-size.x / 2, 0, -size.z / 2), size)
    }

    // ----- extents -------------------------------------------------------------------------------

    /// Inclusive minimum voxel coordinate.
    pub fn voxel_min(&self) -> IVec3 {
        chunk_origin(self.min_chunk)
    }
    /// Exclusive maximum voxel coordinate.
    pub fn voxel_max(&self) -> IVec3 {
        chunk_origin(self.min_chunk + self.dims)
    }
    /// World extent in metres.
    pub fn bounds(&self) -> Aabb {
        Aabb { min: self.voxel_min().as_vec3() * VOXEL_SIZE, max: self.voxel_max().as_vec3() * VOXEL_SIZE }
    }
    pub fn min_chunk(&self) -> IVec3 {
        self.min_chunk
    }
    /// Size in chunks.
    pub fn chunk_dims(&self) -> IVec3 {
        self.dims
    }
    #[inline]
    pub fn in_bounds(&self, v: IVec3) -> bool {
        v.cmpge(self.voxel_min()).all() && v.cmplt(self.voxel_max()).all()
    }

    #[inline]
    fn chunk_slot(&self, c: IVec3) -> Option<usize> {
        let r = c - self.min_chunk;
        if r.x < 0 || r.y < 0 || r.z < 0 || r.x >= self.dims.x || r.y >= self.dims.y || r.z >= self.dims.z {
            return None;
        }
        Some((r.x + self.dims.x * (r.z + self.dims.z * r.y)) as usize)
    }

    fn slot_coord(&self, i: usize) -> IVec3 {
        let i = i as i32;
        let x = i % self.dims.x;
        let z = (i / self.dims.x) % self.dims.z;
        let y = i / (self.dims.x * self.dims.z);
        self.min_chunk + IVec3::new(x, y, z)
    }

    /// All chunk coordinates in the world, in a fixed order (y, then z, then x).
    pub fn all_chunks(&self) -> impl Iterator<Item = IVec3> + '_ {
        (0..self.chunks.len()).map(|i| self.slot_coord(i))
    }

    /// Chunk coordinates that contain at least one solid voxel, in a fixed order.
    pub fn nonempty_chunks(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.chunks.iter().enumerate().filter(|(_, c)| c.as_ref().is_some_and(|c| c.solid > 0)).map(|(i, _)| self.slot_coord(i))
    }

    /// Raw voxel data of a chunk (`None` = all air or out of range). Index with [`local_index`].
    pub fn chunk_data(&self, c: IVec3) -> Option<&[Material; CHUNK_VOLUME]> {
        let s = self.chunk_slot(c)?;
        self.chunks[s].as_ref().filter(|ch| ch.solid > 0).map(|ch| &*ch.voxels)
    }

    /// Number of solid voxels in a chunk.
    pub fn chunk_solid_count(&self, c: IVec3) -> u32 {
        self.chunk_slot(c).and_then(|s| self.chunks[s].as_ref()).map_or(0, |ch| ch.solid)
    }

    /// Total solid voxels in the world.
    pub fn solid_count(&self) -> u64 {
        self.chunks.iter().flatten().map(|c| c.solid as u64).sum()
    }

    // ----- access --------------------------------------------------------------------------------

    /// Material at voxel `v` (air outside the world).
    #[inline]
    pub fn get(&self, v: IVec3) -> Material {
        match self.chunk_slot(chunk_of(v)) {
            Some(s) => match &self.chunks[s] {
                Some(ch) => ch.voxels[local_index(v & CHUNK_MASK)],
                None => AIR,
            },
            None => AIR,
        }
    }

    /// Material at world point `p` (metres).
    #[inline]
    pub fn get_at(&self, p: Vec3) -> Material {
        self.get(bd_core::voxel::world_to_voxel(p))
    }

    /// Set voxel `v` without dirty tracking. Returns the previous material. Out of bounds is ignored.
    #[inline]
    pub fn set_raw(&mut self, v: IVec3, m: Material) -> Material {
        let Some(s) = self.chunk_slot(chunk_of(v)) else { return AIR };
        let ch = match &mut self.chunks[s] {
            Some(ch) => ch,
            slot @ None => {
                if m == AIR {
                    return AIR;
                }
                slot.insert(Chunk::new())
            }
        };
        let li = local_index(v & CHUNK_MASK);
        let cell = &mut ch.voxels[li];
        let prev = *cell;
        if prev != m {
            *cell = m;
            match (prev == AIR, m == AIR) {
                (true, false) => ch.solid += 1,
                (false, true) => ch.solid -= 1,
                _ => {}
            }
        }
        if self.has_enclosed {
            if let Some(bits) = ch.enclosed.as_mut() {
                bits[li >> 6] &= !(1u64 << (li & 63));
            }
            if m == AIR && prev != AIR {
                self.unseal_around(v);
            }
        }
        prev
    }

    /// Set voxel `v`, marking its chunk (and bordering chunks) dirty when it changes.
    /// Returns the previous material.
    #[inline]
    pub fn set(&mut self, v: IVec3, m: Material) -> Material {
        let prev = self.set_raw(v, m);
        if prev != m && self.in_bounds(v) {
            self.mark_voxel_dirty(v);
        }
        prev
    }

    // ----- dirty tracking ------------------------------------------------------------------------

    /// Mark a chunk as needing a remesh.
    pub fn mark_chunk_dirty(&mut self, c: IVec3) {
        if let Some(s) = self.chunk_slot(c) {
            if !self.dirty_flag[s] {
                self.dirty_flag[s] = true;
                self.dirty_list.push(c);
            }
        }
    }

    /// Mark the chunk containing `v`, plus every neighbour chunk whose mesh can see `v` (faces and AO
    /// look one voxel across chunk borders, including diagonally).
    pub fn mark_voxel_dirty(&mut self, v: IVec3) {
        let c = chunk_of(v);
        let l = v & CHUNK_MASK;
        let range = |a: i32| -> (i32, i32) { (if a == 0 { -1 } else { 0 }, if a == CHUNK_MASK { 1 } else { 0 }) };
        let (rx, ry, rz) = (range(l.x), range(l.y), range(l.z));
        for dy in ry.0..=ry.1 {
            for dz in rz.0..=rz.1 {
                for dx in rx.0..=rx.1 {
                    self.mark_chunk_dirty(c + IVec3::new(dx, dy, dz));
                }
            }
        }
    }

    /// Mark every chunk overlapping voxel box `min..=max` (plus a one-voxel apron) dirty.
    pub fn mark_region_dirty(&mut self, min: IVec3, max: IVec3) {
        let (c0, c1) = (chunk_of(min - IVec3::ONE), chunk_of(max + IVec3::ONE));
        for y in c0.y..=c1.y {
            for z in c0.z..=c1.z {
                for x in c0.x..=c1.x {
                    self.mark_chunk_dirty(IVec3::new(x, y, z));
                }
            }
        }
    }

    /// Mark every non-empty chunk dirty (e.g. after generation).
    pub fn mark_all_dirty(&mut self) {
        let list: Vec<IVec3> = self.nonempty_chunks().collect();
        for c in list {
            self.mark_chunk_dirty(c);
        }
    }

    pub fn has_dirty(&self) -> bool {
        !self.dirty_list.is_empty()
    }

    /// Take the list of chunks that need remeshing, in the order they were first dirtied.
    pub fn drain_dirty(&mut self) -> Vec<IVec3> {
        let mut out = Vec::new();
        self.drain_dirty_into(&mut out);
        out
    }

    /// Like [`Self::drain_dirty`] but appends into a reusable buffer.
    pub fn drain_dirty_into(&mut self, out: &mut Vec<IVec3>) {
        let list = std::mem::take(&mut self.dirty_list);
        for &c in &list {
            if let Some(s) = self.chunk_slot(c) {
                self.dirty_flag[s] = false;
            }
        }
        out.extend_from_slice(&list);
        // Keep the allocation for next frame.
        self.dirty_list = list;
        self.dirty_list.clear();
    }

    // ----- anchors -------------------------------------------------------------------------------

    /// Register an indestructible voxel as a search hint for collapse detection.
    pub fn add_anchor_hint(&mut self, v: IVec3) {
        self.anchor_hints.push(v);
    }

    pub fn anchor_hints(&self) -> &[IVec3] {
        &self.anchor_hints
    }

    // ----- bulk fills (generation helpers; all mark dirty) ---------------------------------------

    /// Fill a vertical run `y0..y1` (exclusive) of column (x, z) with `m`.
    pub fn fill_column(&mut self, x: i32, z: i32, y0: i32, y1: i32, m: Material) {
        if y1 <= y0 {
            return;
        }
        if self.has_enclosed {
            // Keep the enclosed-air bits exact (rare at runtime): go voxel by voxel.
            for y in y0.max(self.voxel_min().y)..y1.min(self.voxel_max().y) {
                self.set_raw(IVec3::new(x, y, z), m);
            }
            self.mark_region_dirty(IVec3::new(x, y0, z), IVec3::new(x, y1 - 1, z));
            return;
        }
        let lo = self.voxel_min().y.max(y0);
        let hi = self.voxel_max().y.min(y1);
        let mut y = lo;
        while y < hi {
            // Write a whole chunk-column segment at once.
            let c = chunk_of(IVec3::new(x, y, z));
            let seg_end = hi.min(chunk_origin(c).y + CHUNK_SIZE);
            let Some(s) = self.chunk_slot(c) else { return };
            if m == AIR && self.chunks[s].is_none() {
                y = seg_end;
                continue;
            }
            let ch = self.chunks[s].get_or_insert_with(Chunk::new);
            let (lx, lz) = (x & CHUNK_MASK, z & CHUNK_MASK);
            for yy in y..seg_end {
                let cell = &mut ch.voxels[local_index(IVec3::new(lx, yy & CHUNK_MASK, lz))];
                match (*cell == AIR, m == AIR) {
                    (true, false) => ch.solid += 1,
                    (false, true) => ch.solid -= 1,
                    _ => {}
                }
                *cell = m;
            }
            y = seg_end;
        }
        self.mark_region_dirty(IVec3::new(x, lo, z), IVec3::new(x, hi - 1, z));
    }

    /// Fill the voxel box `min..=max` (inclusive) with `m`.
    pub fn fill_box(&mut self, min: IVec3, max: IVec3, m: Material) {
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                self.fill_column(x, z, min.y, max.y + 1, m);
            }
        }
    }

    /// Visit every voxel in `min..=max` and replace it with whatever `f(v, old)` returns
    /// (`None` = leave unchanged). The general-purpose generation brush.
    pub fn fill_with(&mut self, min: IVec3, max: IVec3, mut f: impl FnMut(IVec3, Material) -> Option<Material>) {
        let min = min.max(self.voxel_min());
        let max = max.min(self.voxel_max() - IVec3::ONE);
        if min.cmpgt(max).any() {
            return;
        }
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let v = IVec3::new(x, y, z);
                    let old = self.get(v);
                    if let Some(m) = f(v, old) {
                        self.set_raw(v, m);
                    }
                }
            }
        }
        self.mark_region_dirty(min, max);
    }

    /// Fill an axis-aligned ellipsoid given in metres. `only_air` preserves existing solids.
    pub fn fill_ellipsoid(&mut self, center: Vec3, radii: Vec3, m: Material, only_air: bool) {
        let c = center / VOXEL_SIZE;
        let r = radii / VOXEL_SIZE;
        let inv = Vec3::ONE / (r * r);
        let (lo, hi) = ((c - r).floor().as_ivec3(), (c + r).ceil().as_ivec3());
        self.fill_with(lo, hi, |v, old| {
            let d = v.as_vec3() + Vec3::splat(0.5) - c;
            let inside = (d * d * inv).element_sum() <= 1.0;
            (inside && (!only_air || old == AIR)).then_some(m)
        });
    }

    /// Fill a sphere given in metres.
    pub fn fill_sphere(&mut self, center: Vec3, radius: f32, m: Material, only_air: bool) {
        self.fill_ellipsoid(center, Vec3::splat(radius), m, only_air);
    }

    // ----- enclosed air (render-only) -------------------------------------------------------------

    /// Does any chunk carry enclosed-air bits?
    #[inline]
    pub fn has_enclosed_air(&self) -> bool {
        self.has_enclosed
    }

    /// Is `v` air sealed inside a closed interior (its bounding faces can't be seen from anywhere a
    /// player can be, so the mesher skips them)? Never affects collision, carving or hashing.
    #[inline]
    pub fn is_enclosed_air(&self, v: IVec3) -> bool {
        if !self.has_enclosed {
            return false;
        }
        match self.chunk_slot(chunk_of(v)).and_then(|s| self.chunks[s].as_ref()) {
            Some(ch) => match &ch.enclosed {
                Some(bits) => {
                    let li = local_index(v & CHUNK_MASK);
                    bits[li >> 6] & (1u64 << (li & 63)) != 0
                }
                None => false,
            },
            None => false,
        }
    }

    fn set_enclosed_bit(&mut self, v: IVec3, on: bool) {
        let Some(s) = self.chunk_slot(chunk_of(v)) else { return };
        let Some(ch) = self.chunks[s].as_mut() else { return };
        let li = local_index(v & CHUNK_MASK);
        if on {
            let bits = ch.enclosed.get_or_insert_with(|| Box::new([0u64; CHUNK_VOLUME / 64]));
            bits[li >> 6] |= 1u64 << (li & 63);
        } else if let Some(bits) = ch.enclosed.as_mut() {
            bits[li >> 6] &= !(1u64 << (li & 63));
        }
    }

    /// Generation-time: for each `(seed, box_min, box_max)`, flood the air around `seed` inside the
    /// inclusive voxel box; if the flood never reaches the box boundary (the air pocket is sealed by
    /// solids, e.g. a closed building floor), mark the whole pocket as enclosed air. Returns the number
    /// of voxels sealed. Removing any voxel next to a pocket later unseals it (and dirties the chunks
    /// that can see it), so blasting a facade reveals the floors behind it. Render-only: collision,
    /// carving, collapse and `content_hash` never look at these bits.
    pub fn seal_enclosed_air(&mut self, seeds: &[(IVec3, IVec3, IVec3)]) -> usize {
        let mut total = 0;
        let mut visited: Vec<bool> = Vec::new();
        let mut stack: Vec<IVec3> = Vec::new();
        let mut pocket: Vec<IVec3> = Vec::new();
        for &(seed, lo, hi) in seeds {
            if self.get(seed) != AIR || self.is_enclosed_air(seed) || !self.in_bounds(seed) {
                continue;
            }
            let size = hi - lo + IVec3::ONE;
            if size.cmple(IVec3::ZERO).any() || seed.cmplt(lo).any() || seed.cmpgt(hi).any() {
                continue;
            }
            let idx = |v: IVec3| {
                let r = v - lo;
                (r.x + size.x * (r.z + size.z * r.y)) as usize
            };
            visited.clear();
            visited.resize((size.x * size.y * size.z) as usize, false);
            stack.clear();
            pocket.clear();
            stack.push(seed);
            visited[idx(seed)] = true;
            let mut sealed = true;
            'flood: while let Some(v) = stack.pop() {
                pocket.push(v);
                for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                    let n = v + d;
                    if self.get(n) != AIR {
                        continue;
                    }
                    if n.cmplt(lo).any() || n.cmpgt(hi).any() || !self.in_bounds(n) {
                        sealed = false;
                        break 'flood;
                    }
                    let i = idx(n);
                    if !visited[i] {
                        visited[i] = true;
                        stack.push(n);
                    }
                }
            }
            if sealed {
                for &v in &pocket {
                    self.set_enclosed_bit(v, true);
                }
                self.has_enclosed = true;
                total += pocket.len();
            }
        }
        total
    }

    /// A voxel at `v` just became air: every enclosed pocket touching it is now open to the world.
    fn unseal_around(&mut self, v: IVec3) {
        let dirs = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
        let mut stack: Vec<IVec3> = dirs.iter().map(|&d| v + d).filter(|&n| self.is_enclosed_air(n)).collect();
        while let Some(u) = stack.pop() {
            if !self.is_enclosed_air(u) {
                continue;
            }
            self.set_enclosed_bit(u, false);
            self.mark_voxel_dirty(u);
            for d in dirs {
                if self.is_enclosed_air(u + d) {
                    stack.push(u + d);
                }
            }
        }
    }

    // ----- queries -------------------------------------------------------------------------------

    /// Highest solid voxel in column (x, z), if any.
    pub fn top_solid_y(&self, x: i32, z: i32) -> Option<i32> {
        let (lo, hi) = (self.voxel_min().y, self.voxel_max().y);
        (lo..hi).rev().find(|&y| self.get(IVec3::new(x, y, z)) != AIR)
    }

    /// Stable 64-bit hash of the full voxel contents (for determinism checks / desync detection).
    /// Air chunks and unallocated chunks hash identically.
    ///
    /// Each 8-voxel word goes through a full-avalanche mix (splitmix64 finaliser). Plain FNV over
    /// whole words only propagates differences *upwards*: an edit to the 8th voxel of a word changed
    /// only the top 8 bits of the hash, so two such edits collided ~1/256 of the time (found by
    /// `tests/props_voxel.rs`).
    pub fn content_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |b: u64| {
            let mut z = (h ^ b).wrapping_add(0x9e37_79b9_7f4a_7c15);
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            h = z ^ (z >> 31);
        };
        for (i, c) in self.chunks.iter().enumerate() {
            let Some(c) = c.as_ref().filter(|c| c.solid > 0) else { continue };
            eat(i as u64);
            for word in c.voxels.chunks_exact(8) {
                eat(u64::from_le_bytes(word.try_into().unwrap()));
            }
        }
        h
    }
}

impl VoxelQuery for VoxelWorld {
    #[inline]
    fn material(&self, v: IVec3) -> Material {
        self.get(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_set_and_bounds() {
        let mut w = VoxelWorld::centered(IVec3::new(64, 64, 64));
        assert_eq!(w.voxel_min(), IVec3::new(-32, 0, -32));
        assert_eq!(w.voxel_max(), IVec3::new(32, 64, 32));
        let v = IVec3::new(-5, 10, 31);
        assert_eq!(w.set(v, 3), AIR);
        assert_eq!(w.get(v), 3);
        assert_eq!(w.solid_count(), 1);
        // Out of bounds reads air, writes are ignored.
        assert_eq!(w.set(IVec3::new(100, 0, 0), 3), AIR);
        assert_eq!(w.get(IVec3::new(100, 0, 0)), AIR);
        assert_eq!(w.get(IVec3::new(0, -1, 0)), AIR);
        w.set(v, AIR);
        assert_eq!(w.solid_count(), 0);
    }

    #[test]
    fn dirty_tracking_includes_border_neighbours() {
        let mut w = VoxelWorld::centered(IVec3::new(128, 64, 128));
        w.drain_dirty();
        // Interior voxel: one chunk.
        w.set(IVec3::new(5, 5, 5), 1);
        assert_eq!(w.drain_dirty(), vec![IVec3::new(0, 0, 0)]);
        // Corner voxel (local 31,31,31 in chunk 0,0,0): 8 chunks (minus any out of range).
        w.set(IVec3::new(31, 31, 31), 1);
        let d = w.drain_dirty();
        assert_eq!(d.len(), 8);
        assert!(d.contains(&IVec3::new(1, 1, 1)));
        // Re-setting the same value doesn't dirty.
        w.set(IVec3::new(31, 31, 31), 1);
        assert!(!w.has_dirty());
    }

    #[test]
    fn fill_column_counts() {
        let mut w = VoxelWorld::centered(IVec3::new(64, 128, 64));
        w.fill_column(3, 4, 10, 100, 2);
        assert_eq!(w.solid_count(), 90);
        assert_eq!(w.top_solid_y(3, 4), Some(99));
        w.fill_column(3, 4, 50, 60, AIR);
        assert_eq!(w.solid_count(), 80);
        assert_eq!(w.get(IVec3::new(3, 55, 4)), AIR);
    }
}
