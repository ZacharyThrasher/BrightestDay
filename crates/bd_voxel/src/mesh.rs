//! Chunk mesher: culled cube faces with baked ambient occlusion and per-voxel colour jitter.
//!
//! Output is plain data ([`ChunkMesh`]) in world-space metres so the client can upload it to any
//! renderer (Bevy: `Mesh::ATTRIBUTE_POSITION / NORMAL / COLOR` + `Indices::U32`). Colours are
//! **linear** RGBA with AO, face shading and jitter baked in; alpha is always 1. A separate per-vertex
//! [`ChunkMesh::emissive`] channel carries the material's self-light (0 = lit surface … 1 = neon), so a
//! renderer can add glow on top (e.g. Miami's `neon_boost`) without re-deriving materials.
//!
//! Meshing only reads `&VoxelWorld`, so any number of chunks can be meshed in parallel on worker
//! threads (use one [`Mesher`] scratch per thread to avoid allocations).

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{AIR, Material};
use glam::IVec3;

use crate::material::{MATERIALS, material_def, srgb_to_linear};
use crate::noise;
use crate::world::{CHUNK_SIZE, VoxelWorld, chunk_origin, local_index};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Linear RGBA.
    pub colors: Vec<[f32; 4]>,
    /// Per-vertex emissive weight: the voxel material's [`crate::MaterialDef::emissive`] (0 … 1).
    pub emissive: Vec<f32>,
    pub indices: Vec<u32>,
}

impl ChunkMesh {
    pub fn clear(&mut self) {
        self.positions.clear();
        self.normals.clear();
        self.colors.clear();
        self.emissive.clear();
        self.indices.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Padded chunk edge: one voxel of neighbour data on each side.
const P: i32 = CHUNK_SIZE + 2;
const P_VOL: usize = (P * P * P) as usize;

#[inline]
fn pidx(x: i32, y: i32, z: i32) -> usize {
    (x + P * (z + P * y)) as usize
}

/// AO brightness per level (0 = fully occluded corner, 3 = open).
const AO_CURVE: [f32; 4] = [0.46, 0.64, 0.82, 1.0];
/// Face shading multipliers: +X −X +Y −Y +Z −Z. Only a subtle top brightening/bottom darkening —
/// the renderer does the real lighting.
const FACE_SHADE: [f32; 6] = [1.0, 1.0, 1.12, 0.86, 1.0, 1.0];

/// Per-face description: normal, and tangent axes U, V with U × V = N so the corner order
/// (−,−) (+,−) (+,+) (−,+) is counter-clockwise seen from outside.
struct Face {
    n: IVec3,
    u: IVec3,
    v: IVec3,
}

const FACES: [Face; 6] = [
    Face { n: IVec3::X, u: IVec3::Y, v: IVec3::Z },
    Face { n: IVec3::NEG_X, u: IVec3::Z, v: IVec3::Y },
    Face { n: IVec3::Y, u: IVec3::Z, v: IVec3::X },
    Face { n: IVec3::NEG_Y, u: IVec3::X, v: IVec3::Z },
    Face { n: IVec3::Z, u: IVec3::X, v: IVec3::Y },
    Face { n: IVec3::NEG_Z, u: IVec3::Y, v: IVec3::X },
];

const CORNERS: [(i32, i32); 4] = [(-1, -1), (1, -1), (1, 1), (-1, 1)];

/// Reusable per-thread scratch for meshing.
pub struct Mesher {
    padded: Vec<Material>,
    /// Padded "never visible" flags for air cells: sealed interior air (see
    /// [`VoxelWorld::is_enclosed_air`]) and the space below the world floor. Faces towards these are
    /// culled; AO still only looks at `padded`, so lighting is unchanged.
    hidden: Vec<bool>,
    /// Linear base colours per material id.
    base: [[f32; 3]; 256],
}

impl Default for Mesher {
    fn default() -> Self {
        Self::new()
    }
}

impl Mesher {
    pub fn new() -> Self {
        let mut base = [[1.0, 0.0, 1.0]; 256];
        for (i, d) in MATERIALS.iter().enumerate() {
            base[i] = [srgb_to_linear(d.color[0]), srgb_to_linear(d.color[1]), srgb_to_linear(d.color[2])];
        }
        Self { padded: vec![AIR; P_VOL], hidden: vec![false; P_VOL], base }
    }

    /// Copy the chunk plus a one-voxel apron into the padded scratch buffer.
    fn gather(&mut self, world: &VoxelWorld, chunk: IVec3) {
        let o = chunk_origin(chunk);
        let data = world.chunk_data(chunk);
        for y in -1..=CHUNK_SIZE {
            for z in -1..=CHUNK_SIZE {
                let row = pidx(0, y + 1, z + 1);
                let inner_row = (0..CHUNK_SIZE).contains(&y) && (0..CHUNK_SIZE).contains(&z);
                if inner_row {
                    // Interior: straight copy of 32 voxels, borders from the neighbours.
                    self.padded[row] = world.get(o + IVec3::new(-1, y, z));
                    self.padded[row + 1 + CHUNK_SIZE as usize] = world.get(o + IVec3::new(CHUNK_SIZE, y, z));
                    let dst = &mut self.padded[row + 1..row + 1 + CHUNK_SIZE as usize];
                    match data {
                        Some(d) => {
                            let s = local_index(IVec3::new(0, y, z));
                            dst.copy_from_slice(&d[s..s + CHUNK_SIZE as usize]);
                        }
                        None => dst.fill(AIR),
                    }
                } else {
                    for x in -1..=CHUNK_SIZE {
                        self.padded[row + (x + 1) as usize] = world.get(o + IVec3::new(x, y, z));
                    }
                }
            }
        }
        // Hidden air: below the world floor (nobody is ever down there) + sealed interiors.
        self.hidden.fill(false);
        let floor = world.voxel_min().y;
        let enclosed = world.has_enclosed_air();
        if o.y - 1 < floor || enclosed {
            for y in -1..=CHUNK_SIZE {
                for z in -1..=CHUNK_SIZE {
                    for x in -1..=CHUNK_SIZE {
                        let v = o + IVec3::new(x, y, z);
                        let i = pidx(x + 1, y + 1, z + 1);
                        if self.padded[i] == AIR && (v.y < floor || (enclosed && world.is_enclosed_air(v))) {
                            self.hidden[i] = true;
                        }
                    }
                }
            }
        }
    }

    /// Mesh one chunk into `out` (cleared first).
    pub fn mesh_chunk_into(&mut self, world: &VoxelWorld, chunk: IVec3, out: &mut ChunkMesh) {
        out.clear();
        if world.chunk_solid_count(chunk) == 0 {
            return;
        }
        self.gather(world, chunk);
        let origin = chunk_origin(chunk);
        let pad = &self.padded;
        let hidden = &self.hidden;
        let solid = |x: i32, y: i32, z: i32| pad[pidx(x, y, z)] != AIR;

        for y in 1..=CHUNK_SIZE {
            for z in 1..=CHUNK_SIZE {
                for x in 1..=CHUNK_SIZE {
                    let m = pad[pidx(x, y, z)];
                    if m == AIR {
                        continue;
                    }
                    let world_v = origin + IVec3::new(x - 1, y - 1, z - 1);
                    let mut color: Option<[f32; 3]> = None;
                    for (fi, f) in FACES.iter().enumerate() {
                        let a = IVec3::new(x, y, z) + f.n;
                        if solid(a.x, a.y, a.z) || hidden[pidx(a.x, a.y, a.z)] {
                            continue;
                        }
                        let c = *color.get_or_insert_with(|| jitter_color(self.base[m as usize], m, world_v));
                        let emissive = material_def(m).emissive;
                        // AO for the four corners, sampled in the air layer in front of the face.
                        let mut ao = [3u8; 4];
                        for (k, &(du, dv)) in CORNERS.iter().enumerate() {
                            let su = a + f.u * du;
                            let sv = a + f.v * dv;
                            let sc = a + f.u * du + f.v * dv;
                            let (s1, s2, cr) = (solid(su.x, su.y, su.z), solid(sv.x, sv.y, sv.z), solid(sc.x, sc.y, sc.z));
                            ao[k] = if s1 && s2 { 0 } else { 3 - s1 as u8 - s2 as u8 - cr as u8 };
                        }
                        let base_i = out.positions.len() as u32;
                        let shade = FACE_SHADE[fi];
                        let nrm = f.n.as_vec3().to_array();
                        // Corner position: voxel min + (n>0 on N axis) + (du>0 on U) + (dv>0 on V).
                        let npos = f.n.max(IVec3::ZERO);
                        for (k, &(du, dv)) in CORNERS.iter().enumerate() {
                            let corner = world_v + npos + if du > 0 { f.u } else { IVec3::ZERO } + if dv > 0 { f.v } else { IVec3::ZERO };
                            out.positions.push((corner.as_vec3() * VOXEL_SIZE).to_array());
                            out.normals.push(nrm);
                            let occl = AO_CURVE[ao[k] as usize];
                            let light = (occl + (1.0 - occl) * emissive) * shade;
                            out.colors.push([c[0] * light, c[1] * light, c[2] * light, 1.0]);
                            out.emissive.push(emissive);
                        }
                        // Split along the diagonal whose endpoints differ most (so a single dark or
                        // bright corner fans smoothly instead of forming a hard triangle); tie →
                        // split between the brighter pair (no dark stripe across the quad).
                        let (a0, a1, a2, a3) = (ao[0] as i32, ao[1] as i32, ao[2] as i32, ao[3] as i32);
                        let d02 = (a0 - a2).abs();
                        let d13 = (a1 - a3).abs();
                        let use_02 = if d02 != d13 { d02 > d13 } else { a0 + a2 >= a1 + a3 };
                        if use_02 {
                            out.indices.extend_from_slice(&[base_i, base_i + 1, base_i + 2, base_i, base_i + 2, base_i + 3]);
                        } else {
                            out.indices.extend_from_slice(&[base_i + 1, base_i + 2, base_i + 3, base_i + 1, base_i + 3, base_i]);
                        }
                    }
                }
            }
        }
    }
}

/// Deterministic per-voxel colour variation: a brightness wobble plus a smaller per-channel tint.
#[inline]
fn jitter_color(base: [f32; 3], m: Material, v: IVec3) -> [f32; 3] {
    let j = material_def(m).jitter;
    let h = noise::hash_v(v, 0x00c0_10e5);
    let b = 1.0 + j * noise::signed(h);
    let h2 = noise::mix32(h);
    let t = j * 0.35;
    let tint = [1.0 + t * noise::signed(h2), 1.0 + t * noise::signed(h2.rotate_left(11)), 1.0 + t * noise::signed(h2.rotate_left(22))];
    [base[0] * b * tint[0], base[1] * b * tint[1], base[2] * b * tint[2]]
}

/// Convenience: mesh one chunk with a fresh scratch (allocates; prefer [`Mesher`] in hot loops).
pub fn mesh_chunk(world: &VoxelWorld, chunk: IVec3) -> ChunkMesh {
    let mut out = ChunkMesh::default();
    Mesher::new().mesh_chunk_into(world, chunk, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_voxel_is_a_cube() {
        let mut w = VoxelWorld::centered(IVec3::new(64, 64, 64));
        w.set(IVec3::new(1, 1, 1), crate::material::mat::STONE);
        let m = mesh_chunk(&w, IVec3::ZERO);
        assert_eq!(m.vertex_count(), 24);
        assert_eq!(m.triangle_count(), 12);
        // All vertices within the voxel's box, windings face outward.
        for tri in m.indices.chunks(3) {
            let p: Vec<glam::Vec3> = tri.iter().map(|&i| glam::Vec3::from(m.positions[i as usize])).collect();
            let n = glam::Vec3::from(m.normals[tri[0] as usize]);
            let cross = (p[1] - p[0]).cross(p[2] - p[0]);
            assert!(cross.dot(n) > 0.0, "triangle winding must be CCW seen from outside");
        }
        // Unoccluded ⇒ full AO everywhere.
        let top: Vec<_> = (0..m.vertex_count()).filter(|&i| m.normals[i][1] > 0.5).collect();
        assert_eq!(top.len(), 4);
    }

    #[test]
    fn emissive_channel_follows_material() {
        let mut w = VoxelWorld::centered(IVec3::new(64, 64, 64));
        w.set(IVec3::new(1, 1, 1), crate::material::mat::STONE);
        w.set(IVec3::new(5, 1, 1), crate::material::mat::NEON_PINK);
        let m = mesh_chunk(&w, IVec3::ZERO);
        assert_eq!(m.emissive.len(), m.vertex_count());
        let lit = m.emissive.iter().filter(|e| **e == 0.0).count();
        let neon = m.emissive.iter().filter(|e| **e == 1.0).count();
        assert_eq!((lit, neon), (24, 24));
        // Colours are unchanged by the channel (alpha stays 1).
        assert!(m.colors.iter().all(|c| c[3] == 1.0));
    }

    #[test]
    fn culls_across_chunk_borders() {
        let mut w = VoxelWorld::centered(IVec3::new(64, 64, 64));
        // Two voxels touching across the x=0 chunk border.
        w.set(IVec3::new(-1, 5, 5), 3);
        w.set(IVec3::new(0, 5, 5), 3);
        let a = mesh_chunk(&w, IVec3::new(-1, 0, 0));
        let b = mesh_chunk(&w, IVec3::new(0, 0, 0));
        assert_eq!(a.vertex_count(), 20);
        assert_eq!(b.vertex_count(), 20);
    }

    #[test]
    fn ao_darkens_inner_corner() {
        let mut w = VoxelWorld::centered(IVec3::new(64, 64, 64));
        for x in 0..4 {
            for z in 0..4 {
                w.set(IVec3::new(x, 0, z), 3);
            }
        }
        w.set(IVec3::new(1, 1, 1), 3); // a bump on the floor
        let m = mesh_chunk(&w, IVec3::ZERO);
        // Some top-face vertex of the floor must be darkened by the bump.
        let min_top = (0..m.vertex_count())
            .filter(|&i| m.normals[i][1] > 0.5 && m.positions[i][1] < 0.6)
            .map(|i| m.colors[i][0] + m.colors[i][1] + m.colors[i][2])
            .fold(f32::MAX, f32::min);
        let max_top = (0..m.vertex_count())
            .filter(|&i| m.normals[i][1] > 0.5 && m.positions[i][1] < 0.6)
            .map(|i| m.colors[i][0] + m.colors[i][1] + m.colors[i][2])
            .fold(0.0, f32::max);
        assert!(min_top < max_top * 0.8);
    }
}
