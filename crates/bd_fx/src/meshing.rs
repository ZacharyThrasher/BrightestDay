//! Worker-thread meshing: bd_shapes `mesh_part` → Bevy `Mesh` with the construct shader's packed
//! attributes (strong-edge bits, weak-point bit, palette slot, AO, optional per-voxel colour),
//! centred per part so identical parts share one mesh (content-hashed).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use bd_shapes::{PartMesh, VoxelModel, mesh_part};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use glam::IVec3;

/// Per-voxel colours for models that are not pure hard light (terrain lifted by a graft keeps its
/// materials): `parts[i]` maps part `i`'s model voxels to an sRGB colour. Voxels without an entry
/// (or parts past the end) use the construct palette.
#[derive(Clone, Debug, Default)]
pub struct VoxelColors {
    pub parts: Vec<HashMap<IVec3, [u8; 3]>>,
    /// Body regions of painted voxels (`bd_shapes::costume::region`: 1 hair, 2 eyes, 3 skin),
    /// index-aligned like `parts`. The construct shader recolours them from
    /// `ConstructVisual::costume` at runtime (power forms) — no re-mesh.
    pub regions: Vec<HashMap<IVec3, u8>>,
}

/// RGB565 of an sRGB colour.
#[inline]
fn rgb565(c: [u8; 3]) -> u32 {
    ((c[0] as u32 >> 3) << 11) | ((c[1] as u32 >> 2) << 5) | (c[2] as u32 >> 3)
}

/// One part's mesh, positions relative to `offset` (model rest-pose metres).
pub struct BuiltPart {
    pub mesh: Mesh,
    pub offset: Vec3,
    pub content: u64,
}

/// Result for one model: index-aligned with `model.parts` (None = empty/marker part).
pub struct BuiltModel {
    pub parts: Vec<Option<BuiltPart>>,
}

fn axis_of(n: [f32; 3]) -> (usize, i32) {
    let a = if n[0].abs() > 0.5 {
        0
    } else if n[1].abs() > 0.5 {
        1
    } else {
        2
    };
    (a, if n[a] > 0.0 { 1 } else { -1 })
}

fn unit(axis: usize) -> IVec3 {
    let mut v = IVec3::ZERO;
    v[axis] = 1;
    v
}

/// Convert one bd_shapes part mesh into render vertex arrays.
///
/// `uv_b.x` packs (exact in f32, < 2^24): palette slot (2 bits) | strong edges (4) | weak point (1) |
/// has-colour (1) | RGB565 colour (16). `uv_b.y` = AO shade (0.5..1) + 4 × body region (0..3).
pub fn build_part_mesh(pm: &PartMesh, voxel_size: f32, offset: Vec3, weak: &[(Vec3, f32)]) -> Option<(Mesh, u64)> {
    build_part_mesh_colored(pm, voxel_size, offset, weak, None)
}

/// [`build_part_mesh`] with optional per-voxel colours (keys = the part's model voxel coordinates).
pub fn build_part_mesh_colored(pm: &PartMesh, voxel_size: f32, offset: Vec3, weak: &[(Vec3, f32)], colors: Option<&HashMap<IVec3, [u8; 3]>>) -> Option<(Mesh, u64)> {
    build_part_mesh_painted(pm, voxel_size, offset, weak, colors, None)
}

/// [`build_part_mesh_colored`] + per-voxel body regions (runtime recolour hooks, see [`VoxelColors`]).
pub fn build_part_mesh_painted(
    pm: &PartMesh,
    voxel_size: f32,
    offset: Vec3,
    weak: &[(Vec3, f32)],
    colors: Option<&HashMap<IVec3, [u8; 3]>>,
    regions: Option<&HashMap<IVec3, u8>>,
) -> Option<(Mesh, u64)> {
    if pm.is_empty() {
        return None;
    }
    let s = voxel_size.max(1e-4);
    let quads = pm.positions.len() / 4;
    // Pass 1: voxel + face id per quad.
    let mut faces: Vec<(IVec3, usize, i32, u8)> = Vec::with_capacity(quads);
    let mut tint_of: HashMap<(IVec3, u8), u8> = HashMap::with_capacity(quads);
    for q in 0..quads {
        let i0 = q * 4;
        let n = pm.normals[i0];
        let (axis, sign) = axis_of(n);
        let ua = (axis + 1) % 3;
        let wa = (axis + 2) % 3;
        let p = pm.positions[i0];
        let uv = pm.uvs[i0];
        let mut c = [p[0] / s, p[1] / s, p[2] / s];
        c[axis] -= 0.5 * sign as f32;
        c[ua] -= 0.5 * (uv[0] * 2.0 - 1.0);
        c[wa] -= 0.5 * (uv[1] * 2.0 - 1.0);
        let v = IVec3::new(c[0].round() as i32, c[1].round() as i32, c[2].round() as i32);
        let dir = (axis as u8) * 2 + (sign > 0) as u8;
        tint_of.insert((v, dir), pm.tints[i0]);
        faces.push((v, axis, sign, dir));
    }

    let n_verts = pm.positions.len();
    let mut positions = Vec::with_capacity(n_verts);
    let mut uvb = Vec::with_capacity(n_verts);
    for (q, (v, axis, _sign, dir)) in faces.iter().enumerate() {
        let ua = (axis + 1) % 3;
        let wa = (axis + 2) % 3;
        let eu = unit(ua);
        let ew = unit(wa);
        let tint = pm.tints[q * 4] & 3;
        // Strong edge where the coplanar neighbour face is missing (crease/silhouette) or has a
        // different palette slot (colour boundary).
        let strong = |nb: IVec3| -> bool {
            match tint_of.get(&(nb, *dir)) {
                None => true,
                Some(t) => (*t & 3) != tint,
            }
        };
        let mut bits = 0u32;
        if strong(*v - eu) {
            bits |= 1; // uv.x = 0 side
        }
        if strong(*v + eu) {
            bits |= 2; // uv.x = 1 side
        }
        if strong(*v - ew) {
            bits |= 4; // uv.y = 0 side
        }
        if strong(*v + ew) {
            bits |= 8; // uv.y = 1 side
        }
        let centre = v.as_vec3();
        let cm = Vec3::new(centre.x, centre.y, centre.z) * s;
        let is_weak = weak.iter().any(|(c, r)| c.distance_squared(cm) <= r * r);
        let color = colors.and_then(|m| m.get(v)).map_or(0, |c| (1 << 7) | (rgb565(*c) << 8));
        let packed = tint as u32 | (bits << 2) | ((is_weak as u32) << 6) | color;
        let region = regions.and_then(|m| m.get(v)).map_or(0.0, |r| f32::from(r & 3) * 4.0);
        for k in 0..4 {
            let p = pm.positions[q * 4 + k];
            positions.push([p[0] - offset.x, p[1] - offset.y, p[2] - offset.z]);
            uvb.push([packed as f32, pm.shade[q * 4 + k].clamp(0.0, 3.9) + region]);
        }
    }

    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.to_bits().hash(&mut h);
    for p in &positions {
        p[0].to_bits().hash(&mut h);
        p[1].to_bits().hash(&mut h);
        p[2].to_bits().hash(&mut h);
    }
    for u in &uvb {
        u[0].to_bits().hash(&mut h);
        u[1].to_bits().hash(&mut h);
    }
    let content = h.finish();

    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, pm.normals.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, pm.uvs.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uvb)
        .with_inserted_indices(Indices::U32(pm.indices.clone()));
    Some((mesh, content))
}

/// Mesh every part of a model (runs on the async compute pool).
pub fn build_model(model: &VoxelModel) -> BuiltModel {
    build_model_colored(model, None)
}

/// [`build_model`] with per-voxel colours.
pub fn build_model_colored(model: &VoxelModel, colors: Option<&VoxelColors>) -> BuiltModel {
    let s = model.voxel_size;
    let mut parts = Vec::with_capacity(model.parts.len());
    let mut seen: HashMap<u64, usize> = HashMap::new();
    for (pi, part) in model.parts.iter().enumerate() {
        if part.voxels.is_empty() || part.id == bd_shapes::SEAT_PART {
            parts.push(None);
            continue;
        }
        let pm = mesh_part(part);
        // Centre on the part's voxel-grid bounds (keeps the voxel lattice aligned).
        let mid = (part.vmin + part.vmax) / 2;
        let offset = Vec3::new(mid.x as f32, mid.y as f32, mid.z as f32) * s;
        let weak: Vec<(Vec3, f32)> = model
            .weak_points
            .iter()
            .filter(|w| w.part_index == pi)
            .map(|w| (Vec3::new(w.center.x, w.center.y, w.center.z), w.radius))
            .collect();
        let pc = colors.and_then(|c| c.parts.get(pi)).filter(|m| !m.is_empty());
        let pr = colors.and_then(|c| c.regions.get(pi)).filter(|m| !m.is_empty());
        match build_part_mesh_painted(&pm, s, offset, &weak, pc, pr) {
            Some((mesh, content)) => {
                seen.entry(content).or_insert(pi);
                parts.push(Some(BuiltPart { mesh, offset, content }));
            }
            None => parts.push(None),
        }
    }
    BuiltModel { parts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::shape::Tint;
    use bevy::mesh::VertexAttributeValues;

    fn part(voxels: Vec<(IVec3, Tint)>) -> bd_shapes::PartModel {
        let vmin = voxels.iter().fold(IVec3::MAX, |a, (v, _)| a.min(*v));
        let vmax = voxels.iter().fold(IVec3::MIN, |a, (v, _)| a.max(*v));
        bd_shapes::PartModel { id: 0, parent: 0, parent_index: 0, pivot: glam::Vec3::ZERO, role: None, voxels, vmin, vmax, depth: 0, voxel_size: 0.5 }
    }

    fn packed(mesh: &Mesh) -> Vec<u32> {
        match mesh.attribute(Mesh::ATTRIBUTE_UV_1) {
            Some(VertexAttributeValues::Float32x2(v)) => v.iter().map(|p| p[0] as u32).collect(),
            _ => panic!("uv_b missing"),
        }
    }

    #[test]
    fn voxel_colours_pack_exactly() {
        let p = part(vec![(IVec3::new(0, 0, 0), Tint::Primary), (IVec3::new(1, 0, 0), Tint::Primary)]);
        let pm = bd_shapes::mesh_part(&p);
        let mut colors = HashMap::new();
        colors.insert(IVec3::new(0, 0, 0), [255u8, 0, 0]);
        let (mesh, _) = build_part_mesh_colored(&pm, 0.5, Vec3::ZERO, &[], Some(&colors)).unwrap();
        let v = packed(&mesh);
        let red = v.iter().filter(|x| (*x >> 7) & 1 == 1).count();
        assert_eq!(red, 5 * 4, "5 exposed faces of the coloured voxel");
        assert!(v.iter().filter(|x| (*x >> 7) & 1 == 1).all(|x| x >> 8 == 0xF800));
        assert!(v.iter().all(|x| *x < (1 << 24)), "exact in f32");
        // Without colours nothing is flagged, and the content hash differs.
        let (plain, h0) = build_part_mesh(&pm, 0.5, Vec3::ZERO, &[]).unwrap();
        assert!(packed(&plain).iter().all(|x| (*x >> 7) == 0));
        let (_, h1) = build_part_mesh_colored(&pm, 0.5, Vec3::ZERO, &[], Some(&colors)).unwrap();
        assert_ne!(h0, h1);
    }

    #[test]
    fn body_regions_ride_the_shade_lane() {
        let p = part(vec![(IVec3::new(0, 0, 0), Tint::Primary), (IVec3::new(1, 0, 0), Tint::Primary)]);
        let pm = bd_shapes::mesh_part(&p);
        let mut colors = HashMap::new();
        colors.insert(IVec3::new(0, 0, 0), [250u8, 210, 40]);
        let mut regions = HashMap::new();
        regions.insert(IVec3::new(0, 0, 0), 1u8);
        let (mesh, _) = build_part_mesh_painted(&pm, 0.5, Vec3::ZERO, &[], Some(&colors), Some(&regions)).unwrap();
        let Some(VertexAttributeValues::Float32x2(v)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1) else { panic!() };
        // Decode like construct.wgsl: region = floor(y / 4), shade = y - 4 region.
        let hair: Vec<_> = v.iter().filter(|p| (p[1] / 4.0).floor() as u32 == 1).collect();
        assert_eq!(hair.len(), 5 * 4, "the tagged voxel's faces");
        assert!(hair.iter().all(|p| (p[0] as u32 >> 7) & 1 == 1), "only painted voxels are tagged");
        assert!(v.iter().all(|p| {
            let s = p[1] - 4.0 * (p[1] / 4.0).floor();
            (0.49..=1.01).contains(&s)
        }));
    }
}
