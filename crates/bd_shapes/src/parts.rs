//! Reusable part builders: wings, tails, legs, jaws, horns, exhaust, pincers, springs, spinner hubs.
//! Shared by the form generators and by `attach::attach_mechanic_parts` so a "flying hammer" gets the
//! same wings a bird does.

use bd_core::shape::{PartId, PartRole, PrimKind, Tint};
use glam::{Mat3, Quat, Vec3};

use crate::build::{A, B, C, D, P, v3};

// ------------------------------------------------------------------------------------ flat prims

impl B {
    /// Flat primitive from `a` to `b` (local Y), `width` along the in-plane perpendicular, `thick`
    /// along `normal`.
    pub fn flat(&mut self, kind: PrimKind, a: Vec3, b: Vec3, width: f32, thick: f32, normal: Vec3, t: Tint) -> &mut Self {
        let d = b - a;
        let len = d.length().max(0.5);
        let q = basis_quat(d, normal);
        self.q(kind, (a + b) * 0.5, v3(width, len, thick), q, t)
    }

    /// Fill the triangle-ish fan (apex → points along the edge `e0..e1`, bowed toward the apex by
    /// `bow`), as flat boxes lying in the plane with `normal`.
    pub fn fan(&mut self, apex: Vec3, e0: Vec3, e1: Vec3, bow: f32, thick: f32, normal: Vec3, t: Tint) -> &mut Self {
        let edge = (e1 - e0).length();
        let n = ((edge / 1.2).ceil() as usize).clamp(2, 24);
        let width = (edge / n as f32) * 1.9 + 0.6;
        for i in 0..=n {
            let f = i as f32 / n as f32;
            let mut p = e0.lerp(e1, f);
            let pull = (std::f32::consts::PI * f).sin() * bow;
            p = p.lerp(apex, pull);
            self.flat(PrimKind::Box, apex, p, width, thick, normal, t);
        }
        self
    }
}

/// Rotation whose local +Y is `dir` and local +Z is as close as possible to `normal`.
pub(crate) fn basis_quat(dir: Vec3, normal: Vec3) -> Quat {
    let y = dir.normalize_or(Vec3::Y);
    let mut z = normal - y * normal.dot(y);
    if z.length_squared() < 1e-6 {
        z = y.any_orthonormal_vector();
    }
    let z = z.normalize();
    let x = y.cross(z);
    Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize()
}

// ------------------------------------------------------------------------------------------ wings

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WingStyle {
    Feather,
    Bat,
    Jet,
    Insect,
    Butterfly,
}

/// One wing, spread horizontally with a dihedral, as its own part (WingLeft/WingRight).
/// `root` = shoulder joint (model voxel units); `span` = root→tip length; `side` −1 left, +1 right.
pub(crate) fn wing(b: &mut B, parent: PartId, root: Vec3, side: f32, span: f32, style: WingStyle, dihedral_deg: f32) -> PartId {
    let role = if side < 0.0 { PartRole::WingLeft } else { PartRole::WingRight };
    let id = b.part(parent, root, role);
    let s = span.max(3.0);
    let rd = Quat::from_rotation_z(side * dihedral_deg.to_radians());
    let n = rd * Vec3::Y;
    // (out, back) → model space.
    let pt = |out: f32, back: f32| root + rd * v3(side * out * s, 0.0, back * s);
    let thick = 1.0;
    match style {
        WingStyle::Feather => {
            let wrist = pt(0.45, -0.12);
            let tip = pt(1.0, 0.18);
            // Coverts / arm (Accent leading edge).
            let bt = (s * 0.14).clamp(1.2, 4.0);
            b.flat(PrimKind::Box, pt(0.0, 0.0), wrist, bt, thick + 0.6, n, A);
            b.flat(PrimKind::Box, wrist, tip, bt * 0.8, thick + 0.4, n, A);
            // Feathers trailing back.
            let nf = ((s / 1.5).round() as usize).clamp(4, 10);
            let l1 = (wrist - pt(0.0, 0.0)).length();
            let l2 = (tip - wrist).length();
            for i in 0..nf {
                let f = (i as f32 + 0.5) / nf as f32;
                let dist = f * (l1 + l2);
                let p = if dist < l1 { pt(0.0, 0.0).lerp(wrist, dist / l1) } else { wrist.lerp(tip, (dist - l1) / l2) };
                let len = s * (0.36 + 0.14 * f);
                let ang = (5.0 + 55.0 * f * f).to_radians();
                let dir = rd * v3(side * ang.sin(), 0.0, ang.cos());
                let w = ((l1 + l2) / nf as f32 * 1.6).max(1.6);
                let tint = if i + 1 == nf { A } else { P };
                b.flat(PrimKind::Box, p, p + dir * len, w, thick, n, tint);
            }
        }
        WingStyle::Bat => {
            let r0 = pt(0.0, 0.0);
            let wrist = pt(0.42, -0.15);
            let tips = [pt(1.0, 0.1), pt(0.82, 0.5), pt(0.5, 0.68), pt(0.08, 0.5)];
            b.fan(wrist, tips[0], tips[1], 0.28, thick, n, P);
            b.fan(wrist, tips[1], tips[2], 0.28, thick, n, P);
            b.fan(wrist, tips[2], tips[3], 0.25, thick, n, P);
            b.fan(wrist, r0, tips[3], 0.0, thick, n, P);
            let bt = (s * 0.08).clamp(1.0, 2.5);
            b.flat(PrimKind::Box, r0, wrist, bt * 1.4, thick + 0.8, n, D);
            for t in tips.iter().take(3) {
                b.flat(PrimKind::Box, wrist, *t, bt, thick + 0.5, n, D);
            }
            // Thumb claw.
            b.cone_to(wrist, wrist + rd * v3(side * 0.05 * s, 0.05 * s, -0.12 * s), bt * 1.5, C);
        }
        WingStyle::Jet => {
            let lead0 = pt(0.0, -0.25);
            let lead1 = pt(1.0, 0.22);
            let trail0 = pt(0.0, 0.35);
            let trail1 = pt(1.0, 0.42);
            b.fan(trail0, lead0, lead1, 0.0, thick + 0.5, n, P);
            b.fan(lead1, trail0, trail1, 0.0, thick + 0.5, n, P);
            b.flat(PrimKind::Box, lead0, lead1, 1.3, thick + 0.8, n, A);
            b.bx(lead1.lerp(trail1, 0.5), Vec3::splat(1.2), C);
        }
        WingStyle::Insect => {
            let fq = |_c: Vec3, dir: Vec3| basis_quat(dir, n);
            let d1 = rd * v3(side * 1.0, 0.0, 0.18).normalize();
            let c1 = pt(0.5, 0.02);
            b.q(PrimKind::Ellipsoid, c1, v3(s * 0.34, s * 1.0, thick), fq(c1, d1), A);
            let d2 = rd * v3(side * 1.0, 0.0, 0.55).normalize();
            let c2 = pt(0.4, 0.3);
            b.q(PrimKind::Ellipsoid, c2, v3(s * 0.28, s * 0.8, thick), fq(c2, d2), A);
            b.flat(PrimKind::Box, pt(0.0, -0.05), pt(0.95, 0.12), 1.0, thick + 0.3, n, C);
        }
        WingStyle::Butterfly => {
            let d1 = rd * v3(side * 1.0, 0.0, -0.35).normalize();
            let c1 = pt(0.5, -0.12);
            let d2 = rd * v3(side * 0.8, 0.0, 0.6).normalize();
            let c2 = pt(0.36, 0.34);
            b.q(PrimKind::Ellipsoid, c1, v3(s * 0.62, s * 1.05, thick), basis_quat(d1, n), A);
            b.q(PrimKind::Ellipsoid, c2, v3(s * 0.5, s * 0.72, thick), basis_quat(d2, n), A);
            b.q(PrimKind::Ellipsoid, c1, v3(s * 0.48, s * 0.88, thick), basis_quat(d1, n), P);
            b.q(PrimKind::Ellipsoid, c2, v3(s * 0.36, s * 0.58, thick), basis_quat(d2, n), P);
            b.q(PrimKind::Ellipsoid, pt(0.62, -0.18), v3(s * 0.18, s * 0.18, thick), basis_quat(d1, n), C);
        }
    }
    id
}

/// A symmetric pair of wings. Returns (left, right).
pub(crate) fn wing_pair(b: &mut B, parent: PartId, root: Vec3, span: f32, style: WingStyle, dihedral: f32) -> (PartId, PartId) {
    let l = wing(b, parent, v3(-root.x.abs(), root.y, root.z), -1.0, span, style, dihedral);
    let r = wing(b, parent, v3(root.x.abs(), root.y, root.z), 1.0, span, style, dihedral);
    (l, r)
}

// ------------------------------------------------------------------------------------------ tails

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TailTip {
    None,
    Spike,
    Club,
    Stinger,
    FishFin,
    Fan,
    Rattle,
}

pub(crate) struct TailSpec {
    pub start: Vec3,
    pub dir: Vec3,
    pub segments: usize,
    pub length: f32,
    pub r0: f32,
    pub r1: f32,
    /// Per-segment rotation applied to the direction (bend/droop/curl).
    pub bend: Quat,
    pub tip: TailTip,
    /// Dorsal plates (tint) on the first `plates_n` segments.
    pub plates: Option<(Tint, usize)>,
    pub flat_y: f32,
}

/// Chained tail segments (role Tail, parent → child). Returns segment ids base → tip.
pub(crate) fn tail(b: &mut B, parent: PartId, t: &TailSpec) -> Vec<PartId> {
    let mut ids = Vec::with_capacity(t.segments);
    let mut pos = t.start;
    let mut dir = t.dir.normalize_or(Vec3::Z);
    let mut prev = parent;
    let n = t.segments.max(1);
    let seg_len = t.length / n as f32;
    for k in 0..n {
        let f = k as f32 / n as f32;
        let r = t.r0 + (t.r1 - t.r0) * f;
        let id = b.part(prev, pos, PartRole::Tail);
        let end = pos + dir * seg_len;
        let q = basis_quat(dir, Vec3::Y);
        let c = (pos + end) * 0.5;
        b.q(PrimKind::Ellipsoid, c, v3((2.0 * r).max(1.0), seg_len * 1.45, (2.0 * r * t.flat_y).max(1.0)), q, P);
        if k % 2 == 1 && r > 1.2 {
            // Belly/Dark ring for segmentation readability.
            b.q(PrimKind::Ellipsoid, c + Vec3::NEG_Y * (r * 0.35), v3((2.0 * r) * 0.9, seg_len * 0.6, (2.0 * r * t.flat_y) * 0.9), q, D);
        }
        if let Some((pt, pn)) = t.plates {
            if k < pn {
                let h = (r * 1.4 * (1.0 - 0.5 * f)).max(1.2);
                let up = (q * Vec3::Z).normalize(); // local "up" of the segment (normal hint)
                let top = c + up * (r * t.flat_y * 0.8);
                b.q(PrimKind::Box, top + up * (h * 0.4), v3(1.0, h, h), basis_quat(up, dir) * Quat::from_rotation_x(0.785), pt);
            }
        }
        prev = id;
        ids.push(id);
        pos = end;
        if k + 1 < n {
            dir = (t.bend * dir).normalize();
        }
    }
    // Tip ornaments belong to the last segment.
    let r = t.r1.max(0.6);
    match t.tip {
        TailTip::None => {}
        TailTip::Spike => {
            b.cone_to(pos - dir * 0.5, pos + dir * (r * 4.0 + 1.5), (r * 2.4).max(1.5), C);
        }
        TailTip::Club => {
            let c = pos + dir * (r * 1.5);
            b.el(c, Vec3::splat(r * 4.5 + 1.0), A);
            for s in [-1.0f32, 1.0] {
                let side = dir.cross(Vec3::Y).normalize_or(Vec3::X) * s;
                b.cone_to(c + side * r * 1.5, c + side * (r * 3.5 + 1.5), 1.6, C);
            }
            b.cone_to(c + Vec3::Y * r * 1.5, c + Vec3::Y * (r * 3.5 + 1.5), 1.6, C);
        }
        TailTip::Stinger => {
            let d2 = (t.bend * t.bend * dir).normalize();
            b.el(pos + dir * r, Vec3::splat(r * 2.6 + 1.0), A);
            b.cone_to(pos + dir * r * 1.5, pos + dir * r * 1.5 + d2 * (r * 3.0 + 2.0), (r * 1.6).max(1.2), C);
        }
        TailTip::FishFin => {
            let back = dir;
            let up = Vec3::Y;
            let len = (t.length * 0.55).max(3.0);
            for s in [1.0f32, -1.0] {
                let tipp = pos + (back * 0.8 + up * s * 0.9).normalize() * len;
                b.flat(PrimKind::Box, pos - back * 0.5, tipp, (len * 0.35).max(1.5), 1.0, Vec3::X, A);
            }
        }
        TailTip::Fan => {
            let len = (t.length * 0.9).max(3.0);
            for i in -2..=2 {
                let a = i as f32 * 0.28;
                let d = Quat::from_rotation_y(a) * dir;
                b.flat(PrimKind::Box, pos - dir, pos + d * len, (len * 0.3).max(1.5), 1.0, Vec3::Y, if i == 0 { A } else { P });
            }
        }
        TailTip::Rattle => {
            for i in 0..3 {
                b.el(pos + dir * (i as f32 * r * 1.4 + r), Vec3::splat(r * 2.2), if i % 2 == 0 { A } else { C });
            }
        }
    }
    ids
}

// ------------------------------------------------------------------------------------------- legs

/// A rigid leg part pivoting at `hip`, optional knee, Dark foot at `foot` pointing along `toe_dir`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn leg(b: &mut B, parent: PartId, role: PartRole, hip: Vec3, knee: Option<Vec3>, foot: Vec3, w: f32, foot_len: f32, toe_dir: Vec3, kind: PrimKind) -> PartId {
    let id = b.part(parent, hip, role);
    match knee {
        Some(k) => {
            b.segx(kind, hip, k, w, w * 0.35, P);
            b.segx(kind, k, foot, (w * 0.8).max(1.0), w * 0.25, P);
            b.el(k, Vec3::splat(w * 1.05), D);
        }
        None => {
            b.segx(kind, hip, foot, w, w * 0.3, P);
        }
    }
    if foot_len > 0.0 {
        let fh = (w * 0.45).max(1.0);
        let fw = (w * 1.2).max(1.0);
        let c = foot + toe_dir * (foot_len * 0.3) + Vec3::Y * (fh * 0.5 - 0.5);
        let q = basis_quat(Vec3::Y, toe_dir);
        b.q(PrimKind::Box, c, v3(fw, fh, foot_len), q, D);
    }
    id
}

// ------------------------------------------------------------------------------- head furniture

/// Lower jaw part hinged at `hinge`, extending forward (−Z), pre-opened by `open_deg`.
pub(crate) fn jaw(b: &mut B, parent: PartId, hinge: Vec3, len: f32, w: f32, h: f32, open_deg: f32, teeth: bool) -> PartId {
    let id = b.part(parent, hinge, PartRole::Jaw);
    let rq = Quat::from_rotation_x(-open_deg.to_radians());
    let c = hinge + rq * v3(0.0, -h * 0.5, -len * 0.5);
    b.q(PrimKind::Box, c, v3(w, h.max(1.0), len), rq, P);
    // Dark underside line.
    b.q(PrimKind::Box, hinge + rq * v3(0.0, -h + 0.5, -len * 0.5), v3(w, 1.0, len * 0.9), rq, D);
    if teeth {
        for s in [-1.0f32, 1.0] {
            let base = hinge + rq * v3(s * (w * 0.5 - 0.5).max(0.0), 0.0, -len + 0.8);
            b.cone_to(base, base + rq * Vec3::Y * (h * 0.6 + 1.0), 1.2, C);
        }
    }
    id
}

/// Two Core eyes on a face plane.
pub(crate) fn eyes(b: &mut B, center: Vec3, spacing: f32, size: f32) {
    let s = size.max(1.0);
    b.sym(|b, sd| {
        b.bx(center + v3(sd * spacing, 0.0, 0.0), v3(s, s, 1.0), C);
    });
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HornStyle {
    Bull,
    Ram,
    Rhino,
    Devil,
}

/// Horns on the current part. `base` = centre of the head top-front; `spread` = half distance.
pub(crate) fn horns(b: &mut B, base: Vec3, spread: f32, len: f32, style: HornStyle) {
    let d = (len * 0.22).max(1.5);
    match style {
        HornStyle::Bull => {
            b.sym(|b, s| {
                let a = base + v3(s * spread, 0.0, 0.0);
                let m = a + v3(s * len * 0.55, len * 0.12, -len * 0.05);
                let t = m + v3(s * len * 0.15, len * 0.35, -len * 0.4);
                b.segx(PrimKind::Cylinder, a, m, d, d * 0.3, A);
                b.cone_to(m, t, d, A);
                b.cone_to(m + (t - m) * 0.55, t, d * 0.55, C);
            });
        }
        HornStyle::Ram => {
            b.sym(|b, s| {
                let mut p = base + v3(s * spread, 0.0, 0.0);
                let mut dir = v3(s * 0.4, 0.6, 0.7).normalize();
                let n = 5;
                for i in 0..n {
                    let seg = len / n as f32;
                    let q = p + dir * seg;
                    let w = d * (1.0 - 0.12 * i as f32);
                    b.segx(PrimKind::Cylinder, p, q, w, 0.4, if i == n - 1 { C } else { A });
                    p = q;
                    dir = (Quat::from_axis_angle(v3(s, 0.0, 0.0), 1.1) * dir).normalize();
                }
            });
        }
        HornStyle::Rhino => {
            b.cone_to(base, base + v3(0.0, len * 0.8, -len * 0.55), d * 1.5, A);
            b.cone_to(base + v3(0.0, len * 0.45, -len * 0.3), base + v3(0.0, len * 0.8, -len * 0.55), d * 0.8, C);
        }
        HornStyle::Devil => {
            b.sym(|b, s| {
                let a = base + v3(s * spread, 0.0, 0.0);
                b.cone_to(a, a + v3(s * len * 0.35, len * 0.8, len * 0.3), d, A);
                b.cone_to(a + v3(s * len * 0.2, len * 0.45, len * 0.17), a + v3(s * len * 0.35, len * 0.8, len * 0.3), d * 0.5, C);
            });
        }
    }
}

// ----------------------------------------------------------------------------------- machinery

/// Nozzle (Dark, on the current part) + flame part (all Core, role Other — animated as a flicker).
/// `dir` = exhaust direction (away from the body). Returns the flame part id.
pub(crate) fn exhaust(b: &mut B, parent: PartId, at: Vec3, dir: Vec3, r: f32) -> PartId {
    let dir = dir.normalize_or(Vec3::Z);
    let cur = b.cur;
    let d = (r * 2.0).max(1.5);
    let h = (r * 1.4).max(1.5);
    // Nozzle flares outward: cone base at the exit, tip back inside the body.
    b.cone_to(at + dir * h, at - dir * (h * 0.4), d * 1.25, D);
    b.segx(PrimKind::Cylinder, at - dir * h * 0.3, at + dir * h * 0.4, d * 0.9, 0.0, A);
    let flame = b.part(parent, at + dir * h, PartRole::Other);
    b.cone_to(at + dir * (h * 0.9), at + dir * (h + r * 4.5 + 2.0), d * 0.9, C);
    b.on(cur);
    flame
}

/// Grabbing pincer arm (ArmLeft/ArmRight) reaching forward from `shoulder`.
pub(crate) fn pincer(b: &mut B, parent: PartId, shoulder: Vec3, side: f32, size: f32) -> PartId {
    let role = if side < 0.0 { PartRole::ArmLeft } else { PartRole::ArmRight };
    let id = b.part(parent, shoulder, role);
    let s = size.max(3.0);
    let w = (s * 0.18).max(1.2);
    let elbow = shoulder + v3(side * s * 0.35, -s * 0.05, -s * 0.35);
    let wrist = elbow + v3(-side * s * 0.05, 0.0, -s * 0.4);
    b.segx(PrimKind::Cylinder, shoulder, elbow, w, w * 0.3, P);
    b.segx(PrimKind::Cylinder, elbow, wrist, w, w * 0.3, P);
    b.el(elbow, Vec3::splat(w * 1.1), D);
    // Claw: palm + two prongs making an open "C".
    b.el(wrist, v3(w * 2.0, w * 1.8, w * 2.2), A);
    let fl = s * 0.42;
    let up = wrist + v3(0.0, w * 0.7, -w * 0.5);
    let lo = wrist + v3(0.0, -w * 0.7, -w * 0.5);
    b.segx(PrimKind::Box, up, up + v3(0.0, fl * 0.25, -fl * 0.8), w * 1.1, 0.3, A);
    b.segx(PrimKind::Box, lo, lo + v3(0.0, -fl * 0.2, -fl * 0.7), w * 1.0, 0.3, A);
    b.cone_to(up + v3(0.0, fl * 0.25, -fl * 0.75), up + v3(0.0, 0.0, -fl * 1.15), w, C);
    b.cone_to(lo + v3(0.0, -fl * 0.2, -fl * 0.65), lo + v3(0.0, 0.1, -fl * 1.0), w, C);
    id
}

/// Coil spring between two points (static), alternating Accent/Core rings.
pub(crate) fn spring(b: &mut B, bottom: Vec3, top: Vec3, d: f32) {
    let n = (((top - bottom).length() / 1.0).round() as usize).clamp(2, 12);
    for i in 0..n {
        let f = (i as f32 + 0.5) / n as f32;
        let c = bottom.lerp(top, f) + v3(if i % 2 == 0 { 0.4 } else { -0.4 }, 0.0, 0.0);
        b.cy(c, d, 1.0, if i % 2 == 0 { A } else { C });
    }
}

/// Spinner hub (static decoration; the sim spins the body): axle + disc + blades on top.
pub(crate) fn hub(b: &mut B, top: Vec3, r: f32) {
    let r = r.max(2.0);
    b.cy(top + v3(0.0, 1.0, 0.0), 1.5, 3.0, D);
    b.cy(top + v3(0.0, 2.5, 0.0), r * 0.6, 1.0, A);
    for i in 0..3 {
        let a = i as f32 * std::f32::consts::TAU / 3.0;
        let d = v3(a.cos(), 0.0, a.sin());
        b.flat(PrimKind::Box, top + v3(0.0, 2.5, 0.0), top + v3(0.0, 2.5, 0.0) + d * r * 1.6, (r * 0.5).max(1.5), 1.0, Vec3::Y, P);
        b.bx(top + v3(0.0, 2.5, 0.0) + d * r * 1.6, Vec3::splat(1.2), C);
    }
    b.bx(top + v3(0.0, 3.2, 0.0), Vec3::splat(1.3), C);
}
