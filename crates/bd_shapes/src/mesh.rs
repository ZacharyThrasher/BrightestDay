//! Part mesher: culled voxel faces (only faces that see the *outside* air, so hollow interiors cost
//! nothing), per-vertex ambient occlusion and a per-face UV so the client shader can darken voxel edges.
//! Positions are metres in model rest-pose space: render each part with its `anim::pose` transform.

use bd_core::shape::Tint;
use glam::IVec3;

use crate::voxelize::{Grid, PartModel, VoxelModel, code_tint, tint_code};

/// Plain vertex arrays (Bevy-friendly: positions/normals `[f32;3]`, uvs `[f32;2]`, u32 indices, CCW).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Corner of the voxel face (0/1): `min(u,1-u,v,1-v)` → distance to the voxel edge, for edge
    /// darkening / glow lines in the shader.
    pub uvs: Vec<[f32; 2]>,
    /// Palette slot per vertex: 0 Primary, 1 Accent, 2 Dark, 3 Core. The client maps these to the
    /// corps colour (Core ⇒ emissive).
    pub tints: Vec<u8>,
    /// Ambient occlusion (0.5 … 1.0) per vertex.
    pub shade: Vec<f32>,
    pub indices: Vec<u32>,
}

impl PartMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Vertex colours from a 4-slot palette (Primary, Accent, Dark, Core), AO baked in.
    pub fn colors(&self, palette: &[[f32; 3]; 4]) -> Vec<[f32; 4]> {
        self.tints
            .iter()
            .zip(&self.shade)
            .map(|(t, s)| {
                let c = palette[*t as usize & 3];
                let s = if *t == 3 { 0.85 + 0.15 * s } else { *s };
                [c[0] * s, c[1] * s, c[2] * s, 1.0]
            })
            .collect()
    }
}

pub fn tint_index(t: Tint) -> u8 {
    tint_code(t) - 1
}

const AO_SHADE: [f32; 4] = [0.5, 0.68, 0.84, 1.0];

/// Mesh one part (rest pose, metres).
pub fn mesh_part(part: &PartModel) -> PartMesh {
    let mut m = PartMesh::default();
    if part.voxels.is_empty() {
        return m;
    }
    // Solid grid padded by 1 so the border is guaranteed outside air.
    let mut g = Grid::new(part.vmin - IVec3::ONE, part.vmax + IVec3::ONE);
    for (v, t) in &part.voxels {
        if let Some(i) = g.idx(*v) {
            g.data[i] = tint_code(*t);
        }
    }
    // Exterior flood fill (6-connected) over empty cells: 1 = outside air.
    let mut outside = vec![0u8; g.data.len()];
    let mut stack = vec![g.min];
    if let Some(i) = g.idx(g.min) {
        outside[i] = 1;
    }
    const N6: [IVec3; 6] = [
        IVec3::new(1, 0, 0),
        IVec3::new(-1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(0, -1, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(0, 0, -1),
    ];
    while let Some(p) = stack.pop() {
        for d in N6 {
            let q = p + d;
            if let Some(i) = g.idx(q) {
                if outside[i] == 0 && g.data[i] == 0 {
                    outside[i] = 1;
                    stack.push(q);
                }
            }
        }
    }
    let solid = |p: IVec3| g.get(p) != 0;
    let is_out = |p: IVec3| g.idx(p).is_none_or(|i| outside[i] != 0);

    let s = part.voxel_size;
    m.positions.reserve(part.voxels.len() * 8);
    for (v, t) in &part.voxels {
        let tint = tint_index(code_tint(tint_code(*t)));
        for axis in 0..3usize {
            for sign in [1i32, -1] {
                let mut d = IVec3::ZERO;
                d[axis] = sign;
                let n = *v + d;
                if !is_out(n) {
                    continue;
                }
                let ua = (axis + 1) % 3;
                let wa = (axis + 2) % 3;
                let mut eu = IVec3::ZERO;
                eu[ua] = 1;
                let mut ew = IVec3::ZERO;
                ew[wa] = 1;
                // Corner order CCW seen from outside for +normal.
                let mut corners = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
                if sign < 0 {
                    corners.reverse();
                }
                let base = m.positions.len() as u32;
                let mut ao = [0u8; 4];
                for (k, (su, sw)) in corners.iter().enumerate() {
                    let s1 = solid(n + eu * *su);
                    let s2 = solid(n + ew * *sw);
                    let c = solid(n + eu * *su + ew * *sw);
                    let a = if s1 && s2 { 0 } else { 3 - (s1 as u8 + s2 as u8 + c as u8) };
                    ao[k] = a;
                    let mut p = v.as_vec3();
                    p[axis] += 0.5 * sign as f32;
                    p[ua] += 0.5 * *su as f32;
                    p[wa] += 0.5 * *sw as f32;
                    m.positions.push((p * s).to_array());
                    m.normals.push(d.as_vec3().to_array());
                    m.uvs.push([(*su > 0) as u8 as f32, (*sw > 0) as u8 as f32]);
                    m.tints.push(tint);
                    m.shade.push(AO_SHADE[a as usize]);
                }
                if ao[0] as u32 + ao[2] as u32 > ao[1] as u32 + ao[3] as u32 {
                    m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                } else {
                    m.indices.extend_from_slice(&[base + 1, base + 2, base + 3, base + 1, base + 3, base]);
                }
            }
        }
    }
    m
}

/// Mesh for a dynamic Link chain: links along +Z from the origin, `length_m` long (quantise/cache by
/// length). Place it between two points with [`chain_transform`] (its scale stretches it to fit).
pub fn chain_link_mesh(length_m: f32) -> PartMesh {
    let model = crate::voxelize::voxelize(&crate::attach::chain_spec(length_m));
    model.parts.first().map(mesh_part).unwrap_or_default()
}

/// Transform mapping a [`chain_link_mesh`] of `mesh_length_m` onto the segment `a → b`.
pub fn chain_transform(a: glam::Vec3, b: glam::Vec3, mesh_length_m: f32) -> glam::Affine3A {
    let d = b - a;
    let len = d.length();
    if !len.is_finite() || len < 1e-4 || !a.is_finite() {
        return glam::Affine3A::from_translation(if a.is_finite() { a } else { glam::Vec3::ZERO });
    }
    let q = glam::Quat::from_rotation_arc(glam::Vec3::Z, d / len);
    let s = len / mesh_length_m.max(0.1);
    glam::Affine3A::from_scale_rotation_translation(glam::Vec3::new(1.0, 1.0, s), q, a)
}

/// Mesh every part (index-aligned with `model.parts`).
pub fn mesh_model(model: &VoxelModel) -> Vec<PartMesh> {
    model.parts.iter().map(mesh_part).collect()
}
