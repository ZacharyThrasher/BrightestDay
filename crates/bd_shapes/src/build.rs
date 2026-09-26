//! Authoring helpers for `ShapeSpec`s. All coordinates are construct-voxel units (0.25 m); voxel `v`
//! is the cube centred on integer point `v`, so odd widths centred on 0 are symmetric.

use bd_core::shape::{Part, PartId, PartRole, Prim, PrimKind, ShapeSpec, Tint};
use glam::{EulerRot, Quat, Vec3};

pub(crate) const P: Tint = Tint::Primary;
pub(crate) const A: Tint = Tint::Accent;
pub(crate) const D: Tint = Tint::Dark;
pub(crate) const C: Tint = Tint::Core;

#[inline]
pub(crate) fn v3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// Euler XYZ degrees (the `Prim::rot_deg` convention: intrinsic X then Y then Z, i.e. `Rx·Ry·Rz`).
pub fn quat_from_rot_deg(rot_deg: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        rot_deg.x.to_radians(),
        rot_deg.y.to_radians(),
        rot_deg.z.to_radians(),
    )
}

pub(crate) fn rot_deg_from_quat(q: Quat) -> Vec3 {
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

/// Nearest odd integer ≥ 1 (for widths centred on a voxel line).
pub(crate) fn odd(x: f32) -> f32 {
    let n = (x.max(1.0) - 1.0) / 2.0;
    n.round() * 2.0 + 1.0
}

pub(crate) struct B {
    pub spec: ShapeSpec,
    pub cur: PartId,
}

impl B {
    pub fn new() -> Self {
        B { spec: ShapeSpec::default(), cur: 0 }
    }

    pub fn from_spec(spec: ShapeSpec) -> Self {
        B { spec, cur: 0 }
    }

    pub fn next_part_id(&self) -> PartId {
        self.spec.parts.iter().map(|p| p.id).filter(|&id| id != crate::voxelize::SEAT_PART).max().unwrap_or(0).saturating_add(1).min(254)
    }

    /// Declare a new part (and make it current).
    pub fn part(&mut self, parent: PartId, pivot: Vec3, role: PartRole) -> PartId {
        let id = self.next_part_id();
        self.spec.parts.push(Part { id, parent, pivot, role });
        self.cur = id;
        id
    }

    pub fn on(&mut self, id: PartId) -> &mut Self {
        self.cur = id;
        self
    }

    pub fn prim(&mut self, kind: PrimKind, center: Vec3, size: Vec3, rot_deg: Vec3, tint: Tint, subtract: bool) -> &mut Self {
        self.spec.prims.push(Prim { kind, center, size: size.abs(), rot_deg, tint, part: self.cur, subtract });
        self
    }

    pub fn q(&mut self, kind: PrimKind, c: Vec3, s: Vec3, q: Quat, t: Tint) -> &mut Self {
        self.prim(kind, c, s, rot_deg_from_quat(q), t, false)
    }

    pub fn bx(&mut self, c: Vec3, s: Vec3, t: Tint) -> &mut Self {
        self.prim(PrimKind::Box, c, s, Vec3::ZERO, t, false)
    }
    pub fn bxr(&mut self, c: Vec3, s: Vec3, r: Vec3, t: Tint) -> &mut Self {
        self.prim(PrimKind::Box, c, s, r, t, false)
    }
    pub fn el(&mut self, c: Vec3, s: Vec3, t: Tint) -> &mut Self {
        self.prim(PrimKind::Ellipsoid, c, s, Vec3::ZERO, t, false)
    }
    pub fn elr(&mut self, c: Vec3, s: Vec3, r: Vec3, t: Tint) -> &mut Self {
        self.prim(PrimKind::Ellipsoid, c, s, r, t, false)
    }
    /// Cylinder along Y.
    pub fn cy(&mut self, c: Vec3, d: f32, h: f32, t: Tint) -> &mut Self {
        self.prim(PrimKind::Cylinder, c, v3(d, h, d), Vec3::ZERO, t, false)
    }
    /// Cylinder along Z.
    pub fn cz(&mut self, c: Vec3, d: f32, len: f32, t: Tint) -> &mut Self {
        self.prim(PrimKind::Cylinder, c, v3(d, len, d), v3(90.0, 0.0, 0.0), t, false)
    }
    /// Cylinder along X.
    pub fn cx(&mut self, c: Vec3, d: f32, len: f32, t: Tint) -> &mut Self {
        self.prim(PrimKind::Cylinder, c, v3(d, len, d), v3(0.0, 0.0, 90.0), t, false)
    }
    pub fn cor(&mut self, c: Vec3, s: Vec3, r: Vec3, t: Tint) -> &mut Self {
        self.prim(PrimKind::Cone, c, s, r, t, false)
    }

    pub fn cut_bx(&mut self, c: Vec3, s: Vec3) -> &mut Self {
        self.prim(PrimKind::Box, c, s, Vec3::ZERO, P, true)
    }
    pub fn cut_bxr(&mut self, c: Vec3, s: Vec3, r: Vec3) -> &mut Self {
        self.prim(PrimKind::Box, c, s, r, P, true)
    }
    pub fn cut_el(&mut self, c: Vec3, s: Vec3) -> &mut Self {
        self.prim(PrimKind::Ellipsoid, c, s, Vec3::ZERO, P, true)
    }
    pub fn cut(&mut self, kind: PrimKind, c: Vec3, s: Vec3, r: Vec3) -> &mut Self {
        self.prim(kind, c, s, r, P, true)
    }

    /// A primitive whose local +Y runs from `a` to `b`; `w` = (x, z) cross-section size.
    pub fn seg(&mut self, kind: PrimKind, a: Vec3, b: Vec3, wx: f32, wz: f32, t: Tint) -> &mut Self {
        let d = b - a;
        let len = d.length().max(0.5);
        let q = Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::Y));
        self.q(kind, (a + b) * 0.5, v3(wx, len, wz), q, t)
    }

    /// Like `seg` but extends `ext` voxels past both ends (hides joints).
    pub fn segx(&mut self, kind: PrimKind, a: Vec3, b: Vec3, w: f32, ext: f32, t: Tint) -> &mut Self {
        let d = (b - a).normalize_or(Vec3::Y);
        self.seg(kind, a - d * ext, b + d * ext, w, w, t)
    }

    /// Cone from base centre `base` pointing to `tip`.
    pub fn cone_to(&mut self, base: Vec3, tip: Vec3, d: f32, t: Tint) -> &mut Self {
        self.seg(PrimKind::Cone, base, tip, d, d, t)
    }

    /// Run `f` for the left (-1) and right (+1) side.
    pub fn sym(&mut self, mut f: impl FnMut(&mut B, f32)) -> &mut Self {
        f(self, -1.0);
        f(self, 1.0);
        self
    }
}
