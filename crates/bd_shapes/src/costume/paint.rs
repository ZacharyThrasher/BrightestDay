//! Painted costumes: explicit sRGB colours per primitive, carried beside the [`ShapeSpec`] (index
//! aligned with `spec.prims`) and baked into per-voxel colours after voxelisation.
//!
//! * A prim's paint follows the voxeliser's rule: the last additive prim covering a voxel decides its
//!   colour (`None` = the construct palette, e.g. the lantern ring stays corps-coloured).
//! * **Decals** (`Paint::decal`) never add geometry: they only recolour voxels that exist (faces,
//!   emblems, web lines). They ride every spec transform (head scale, guardrail, cap shrink) because
//!   they are ordinary prims until the bake.
//! * The client renders painted voxels through the construct shader's per-voxel colour channel
//!   (`bd_fx::VoxelColors`), tinted with a little hard light; galleries/previews use the colour as is.

use std::collections::HashMap;

use bd_core::shape::{PartId, PartRole, ShapeSpec, Tint};
use glam::{IVec3, Quat, Vec3};

use super::{COSTUME_UNIT, MAX_COSTUME_VOXELS};
use crate::build::{B, v3};
use crate::voxelize::{PrimEval, VoxelModel};

/// sRGB colour, 0..255.
pub type Rgb = [u8; 3];

/// Paint of one prim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paint {
    /// Explicit colour; `None` = palette slot (corps colour etc.).
    pub rgb: Option<Rgb>,
    /// Recolour existing voxels only (no geometry).
    pub decal: bool,
}

/// A dressed costume: the spec (1/16 m units, humanoid rig) plus its paint.
#[derive(Clone, Debug, Default)]
pub struct Costume {
    pub spec: ShapeSpec,
    /// Index-aligned with `spec.prims` (shorter = the rest unpainted).
    pub paint: Vec<Paint>,
    /// Canonical name of the character look used, if any (tests / debugging; never shown in UI).
    pub character: Option<String>,
    /// `(variant index, variant count)` for character looks.
    pub variant: Option<(usize, usize)>,
    /// Which tier dressed it.
    pub tier: Tier,
}

/// Resolution tier that produced a costume (see `costume::dress`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tier {
    /// Hand-authored character look (assets/costumes/characters.ron).
    Character,
    /// Derived from a curated humanoid icon's shape.
    Derived,
    /// Vocabulary-driven (the original costume builder).
    #[default]
    Vocabulary,
}

impl Costume {
    /// Unpainted costume (the vocabulary tier).
    pub fn plain(spec: ShapeSpec) -> Self {
        Costume { spec, ..Default::default() }
    }
    pub fn is_painted(&self) -> bool {
        self.paint.iter().any(|p| p.rgb.is_some())
    }
}

/// Voxel model + per-part voxel colours (index-aligned with `model.parts`; keys = model voxels).
pub struct PaintedModel {
    pub model: VoxelModel,
    pub colors: Vec<HashMap<IVec3, Rgb>>,
}

/// Spec without decal prims (what gets voxelised).
fn geometry(spec: &ShapeSpec, paint: &[Paint]) -> ShapeSpec {
    let prims = spec.prims.iter().enumerate().filter(|(i, _)| !paint.get(*i).is_some_and(|p| p.decal)).map(|(_, p)| p.clone()).collect();
    ShapeSpec { prims, parts: spec.parts.clone() }
}

/// Voxelise a costume (capped like [`super::costume_model`]) and bake its paint.
pub fn costume_model_painted(c: &Costume) -> PaintedModel {
    let mut spec = c.spec.clone();
    let mut m = crate::voxelize::voxelize_with_unit(&geometry(&spec, &c.paint), COSTUME_UNIT);
    if m.voxel_count > MAX_COSTUME_VOXELS {
        let f = ((MAX_COSTUME_VOXELS as f32 / m.voxel_count as f32).sqrt() * 0.95).clamp(0.3, 1.0);
        crate::voxelize::scale_spec(&mut spec, f);
        m = crate::voxelize::voxelize_with_unit(&geometry(&spec, &c.paint), COSTUME_UNIT);
        m.unit_m = COSTUME_UNIT * f;
    }
    let colors = if c.is_painted() { bake(&spec, &c.paint, &m) } else { vec![HashMap::new(); m.parts.len()] };
    PaintedModel { model: m, colors }
}

/// Per-voxel colours of a voxelised spec (same prim order rule as the voxeliser).
pub fn bake(spec: &ShapeSpec, paint: &[Paint], m: &VoxelModel) -> Vec<HashMap<IVec3, Rgb>> {
    const MAX_COORD: f32 = 4096.0;
    struct Dense {
        min: IVec3,
        dim: IVec3,
        idx: Vec<i32>,
        col: Vec<Option<Rgb>>,
    }
    let mut dense: Vec<Dense> = m
        .parts
        .iter()
        .map(|p| {
            if p.voxels.is_empty() {
                return Dense { min: IVec3::ZERO, dim: IVec3::ZERO, idx: Vec::new(), col: Vec::new() };
            }
            let dim = p.vmax - p.vmin + IVec3::ONE;
            let mut idx = vec![-1i32; (dim.x * dim.y * dim.z) as usize];
            for (k, (v, _)) in p.voxels.iter().enumerate() {
                let l = *v - p.vmin;
                idx[(l.x + dim.x * (l.y + dim.y * l.z)) as usize] = k as i32;
            }
            Dense { min: p.vmin, dim, idx, col: vec![None; p.voxels.len()] }
        })
        .collect();
    for (i, p) in spec.prims.iter().enumerate() {
        if p.subtract || !(p.size.is_finite() && p.center.is_finite() && p.rot_deg.is_finite()) {
            continue;
        }
        let paint = paint.get(i).copied().unwrap_or_default();
        if paint.decal && paint.rgb.is_none() {
            continue;
        }
        let pi = m.part_index(p.part).unwrap_or(0);
        let Some(d) = dense.get_mut(pi) else { continue };
        if d.idx.is_empty() {
            continue;
        }
        let c = p.center.clamp(Vec3::splat(-MAX_COORD), Vec3::splat(MAX_COORD));
        let s = p.size.abs().min(Vec3::splat(MAX_COORD));
        let e = PrimEval::new(p.kind, c, s, p.rot_deg);
        let lo = e.lo.max(d.min);
        let hi = e.hi.min(d.min + d.dim - IVec3::ONE);
        for z in lo.z..=hi.z {
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    let q = IVec3::new(x, y, z);
                    let l = q - d.min;
                    let k = d.idx[(l.x + d.dim.x * (l.y + d.dim.y * l.z)) as usize];
                    if k < 0 || !e.inside(q.as_vec3()) {
                        continue;
                    }
                    d.col[k as usize] = paint.rgb;
                }
            }
        }
    }
    m.parts
        .iter()
        .zip(dense)
        .map(|(p, d)| p.voxels.iter().zip(d.col).filter_map(|((v, _), c)| c.map(|c| (*v, c))).collect())
        .collect()
}

/// Colours for a part mesh's vertices (`PartMesh::colors` with per-voxel paint): painted voxels use
/// their colour, the rest the palette.
pub fn mesh_colors(pm: &crate::PartMesh, voxel_size: f32, palette: &[[f32; 3]; 4], paint: Option<&HashMap<IVec3, Rgb>>) -> Vec<[f32; 4]> {
    let base = pm.colors(palette);
    let Some(paint) = paint.filter(|p| !p.is_empty()) else { return base };
    let s = voxel_size.max(1e-4);
    let mut out = base;
    for q in 0..pm.positions.len() / 4 {
        let v = quad_voxel(pm, q, s);
        if let Some(c) = paint.get(&v) {
            let lin = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
            for k in 0..4 {
                let sh = pm.shade[q * 4 + k];
                out[q * 4 + k] = [lin[0] * sh, lin[1] * sh, lin[2] * sh, 1.0];
            }
        }
    }
    out
}

/// Model voxel a mesh quad belongs to (the face lies on the voxel's side along its normal).
pub fn quad_voxel(pm: &crate::PartMesh, q: usize, s: f32) -> IVec3 {
    let i0 = q * 4;
    let n = pm.normals[i0];
    let axis = if n[0].abs() > 0.5 {
        0
    } else if n[1].abs() > 0.5 {
        1
    } else {
        2
    };
    let sign = if n[axis] > 0.0 { 1.0 } else { -1.0 };
    let ua = (axis + 1) % 3;
    let wa = (axis + 2) % 3;
    let p = pm.positions[i0];
    let uv = pm.uvs[i0];
    let mut c = [p[0] / s, p[1] / s, p[2] / s];
    c[axis] -= 0.5 * sign;
    c[ua] -= 0.5 * (uv[0] * 2.0 - 1.0);
    c[wa] -= 0.5 * (uv[1] * 2.0 - 1.0);
    IVec3::new(c[0].round() as i32, c[1].round() as i32, c[2].round() as i32)
}

// ------------------------------------------------------------------------------------ painter

/// Painting builder over a costume [`B`]: every prim it adds gets the current colour. Tint slots
/// are allocated per distinct colour (cycling Primary/Accent/Dark) so colour boundaries also get the
/// construct shader's crease lines; `glow` uses the Core slot.
pub(crate) struct Pn<'a> {
    pub b: &'a mut B,
    pub paint: &'a mut Vec<Paint>,
    pub slots: &'a mut Vec<Rgb>,
    cur: Option<Rgb>,
    decal: bool,
    tint: Option<Tint>,
}

#[allow(dead_code)]
impl<'a> Pn<'a> {
    pub fn new(b: &'a mut B, paint: &'a mut Vec<Paint>, slots: &'a mut Vec<Rgb>) -> Self {
        Pn { b, paint, slots, cur: None, decal: false, tint: None }
    }
    /// Current colour for the next prims.
    pub fn c(&mut self, rgb: Rgb) -> &mut Self {
        self.cur = Some(rgb);
        self.decal = false;
        self.tint = None;
        self
    }
    /// Unpainted (palette slot `t`) prims, e.g. the lantern ring.
    pub fn palette(&mut self, t: Tint) -> &mut Self {
        self.cur = None;
        self.decal = false;
        self.tint = Some(t);
        self
    }
    /// Recolour-only prims in `rgb`.
    pub fn decal(&mut self, rgb: Rgb) -> &mut Self {
        self.cur = Some(rgb);
        self.decal = true;
        self.tint = None;
        self
    }
    pub fn on(&mut self, id: PartId) -> &mut Self {
        self.b.on(id);
        self
    }
    pub fn part(&mut self, parent: PartId, pivot: Vec3, role: PartRole) -> PartId {
        self.b.part(parent, pivot, role)
    }
    fn tint_for(&mut self) -> Tint {
        if let Some(t) = self.tint {
            return t;
        }
        let Some(c) = self.cur else { return Tint::Primary };
        let i = match self.slots.iter().position(|s| *s == c) {
            Some(i) => i,
            None => {
                self.slots.push(c);
                self.slots.len() - 1
            }
        };
        [Tint::Primary, Tint::Accent, Tint::Dark][i % 3]
    }
    fn stamp(&mut self) -> &mut Self {
        let n = self.b.spec.prims.len();
        if n == 0 {
            return self;
        }
        let t = self.tint_for();
        self.b.spec.prims[n - 1].tint = t;
        if self.paint.len() < n - 1 {
            self.paint.resize(n - 1, Paint::default());
        }
        self.paint.truncate(n - 1);
        self.paint.push(Paint { rgb: self.cur, decal: self.decal });
        self
    }
    /// Box over voxel indices `x0..=x1, y0..=y1, z0..=z1` (exact voxel coverage).
    pub fn vb(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, z0: i32, z1: i32) -> &mut Self {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let (z0, z1) = (z0.min(z1), z0.max(z1));
        let c = v3(x0 as f32 + x1 as f32, y0 as f32 + y1 as f32, z0 as f32 + z1 as f32) * 0.5;
        let s = v3((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32, (z1 - z0 + 1) as f32);
        self.b.bx(c, s, Tint::Primary);
        self.stamp()
    }
    /// Mirrored pair of [`Pn::vb`] boxes (x and −x); `x0..x1` on the +x side.
    pub fn vbs(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, z0: i32, z1: i32) -> &mut Self {
        self.vb(x0, x1, y0, y1, z0, z1);
        self.vb(-x0, -x1, y0, y1, z0, z1)
    }
    /// One voxel.
    pub fn px(&mut self, x: i32, y: i32, z: i32) -> &mut Self {
        self.vb(x, x, y, y, z, z)
    }
    pub fn bx(&mut self, c: Vec3, s: Vec3) -> &mut Self {
        self.b.bx(c, s, Tint::Primary);
        self.stamp()
    }
    pub fn bxr(&mut self, c: Vec3, s: Vec3, r: Vec3) -> &mut Self {
        self.b.bxr(c, s, r, Tint::Primary);
        self.stamp()
    }
    pub fn el(&mut self, c: Vec3, s: Vec3) -> &mut Self {
        self.b.el(c, s, Tint::Primary);
        self.stamp()
    }
    pub fn elr(&mut self, c: Vec3, s: Vec3, r: Vec3) -> &mut Self {
        self.b.elr(c, s, r, Tint::Primary);
        self.stamp()
    }
    pub fn cy(&mut self, c: Vec3, d: f32, h: f32) -> &mut Self {
        self.b.cy(c, d, h, Tint::Primary);
        self.stamp()
    }
    pub fn cz(&mut self, c: Vec3, d: f32, len: f32) -> &mut Self {
        self.b.cz(c, d, len, Tint::Primary);
        self.stamp()
    }
    pub fn cxl(&mut self, c: Vec3, d: f32, len: f32) -> &mut Self {
        self.b.cx(c, d, len, Tint::Primary);
        self.stamp()
    }
    pub fn cone_to(&mut self, base: Vec3, tip: Vec3, d: f32) -> &mut Self {
        self.b.cone_to(base, tip, d, Tint::Primary);
        self.stamp()
    }
    /// Box whose local +Y runs `a → b` (`wx` × `wz` cross-section).
    pub fn seg(&mut self, a: Vec3, b: Vec3, wx: f32, wz: f32) -> &mut Self {
        self.b.seg(bd_core::shape::PrimKind::Box, a, b, wx, wz, Tint::Primary);
        self.stamp()
    }
    /// Cylinder `a → b`.
    pub fn rod(&mut self, a: Vec3, b: Vec3, d: f32) -> &mut Self {
        self.b.seg(bd_core::shape::PrimKind::Cylinder, a, b, d, d, Tint::Primary);
        self.stamp()
    }
    /// Box rotated by a quaternion.
    pub fn bq(&mut self, c: Vec3, s: Vec3, q: Quat) -> &mut Self {
        self.b.q(bd_core::shape::PrimKind::Box, c, s, q, Tint::Primary);
        self.stamp()
    }
    /// Ellipsoid rotated by a quaternion.
    pub fn eq(&mut self, c: Vec3, s: Vec3, q: Quat) -> &mut Self {
        self.b.q(bd_core::shape::PrimKind::Ellipsoid, c, s, q, Tint::Primary);
        self.stamp()
    }
    /// Pixel-art decal on a plane: `rows[0]` is the top row; each char looks up `pal` (`.`/space =
    /// skip). `origin` = voxel of the top-left cell, `right`/`down` = voxel steps, depth along `n`
    /// (voxels painted from `origin` inward by `depth` layers).
    pub fn mosaic(&mut self, rows: &[&str], pal: &dyn Fn(char) -> Option<Rgb>, origin: IVec3, right: IVec3, down: IVec3, n: IVec3, depth: i32) -> &mut Self {
        let saved = (self.cur, self.decal, self.tint);
        for (r, row) in rows.iter().enumerate() {
            for (k, ch) in row.chars().enumerate() {
                let Some(col) = pal(ch) else { continue };
                let p = origin + right * k as i32 + down * r as i32;
                let q = p + n * (depth - 1);
                self.decal(col);
                self.vb(p.x, q.x, p.y, q.y, p.z, q.z);
            }
        }
        (self.cur, self.decal, self.tint) = saved;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_follows_last_writer_and_decals_add_no_geometry() {
        let mut b = B::new();
        let mut paint = Vec::new();
        let mut slots = Vec::new();
        {
            let mut p = Pn::new(&mut b, &mut paint, &mut slots);
            p.c([255, 0, 0]).vb(0, 3, 0, 3, 0, 3);
            p.c([0, 0, 255]).vb(0, 0, 0, 3, 0, 3);
            p.decal([0, 255, 0]).vb(3, 5, 3, 5, 3, 5);
        }
        let c = Costume { spec: b.spec, paint, ..Default::default() };
        let pm = costume_model_painted(&c);
        assert_eq!(pm.model.voxel_count, 64, "decal added no voxels");
        let cols = &pm.colors[0];
        assert_eq!(cols.get(&IVec3::new(0, 1, 1)), Some(&[0, 0, 255]));
        assert_eq!(cols.get(&IVec3::new(2, 0, 0)), Some(&[255, 0, 0]));
        assert_eq!(cols.get(&IVec3::new(3, 3, 3)), Some(&[0, 255, 0]));
    }
}
