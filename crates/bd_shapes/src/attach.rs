//! Generic mechanic parts for ANY form (COMBAT.md §5b): wings, exhaust, spikes, blade edges, tail,
//! jaw/maw, pincers, horns, walker legs, spinner hub, springs, launch chevrons, guard boss, barrel,
//! lens, fuse, seeker eye, goo, mini canopy, hook, speed fins.
//!
//! Placement is relative to the base body's bounding box and the shape conventions (+Y up, −Z front):
//! wings on the sides near the top, exhaust at the back, horns/jaw/spikes/pincers/barrel at the front,
//! legs/springs underneath, hub/fuse/canopy on top. Idempotent where the part has a role (wings, tail,
//! jaw, pincers, legs): grafting `wings` onto something that already has wings adds nothing. For
//! decorations without a role (spikes, exhaust…), pass only *newly grafted* mechanics.

use bd_core::mechanics::Mechanic;
use bd_core::shape::{Part, PartId, PartRole, PrimKind, ShapeSpec};
use glam::{Quat, Vec3};

use crate::build::{A, B, C, D, P, v3};
use crate::parts::{HornStyle, TailSpec, TailTip, WingStyle, exhaust, horns, hub, jaw, leg, pincer, spring, tail, wing_pair};
use crate::voxelize::{PrimEval, SEAT_PART};

/// Axis-aligned bounds in construct-voxel units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl SpecBounds {
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn size(&self) -> Vec3 {
        (self.max - self.min).max(Vec3::ONE)
    }
}

fn bounds_filtered(spec: &ShapeSpec, f: impl Fn(PartId) -> bool) -> Option<SpecBounds> {
    let mut mn = Vec3::splat(f32::MAX);
    let mut mx = Vec3::splat(f32::MIN);
    let mut any = false;
    for p in spec.prims.iter().filter(|p| !p.subtract && f(p.part) && p.center.is_finite() && p.size.is_finite() && p.rot_deg.is_finite()) {
        let e = PrimEval::new(p.kind, p.center, p.size, p.rot_deg);
        mn = mn.min(e.lo.as_vec3());
        mx = mx.max(e.hi.as_vec3());
        any = true;
    }
    any.then_some(SpecBounds { min: mn, max: mx })
}

/// Bounds of all additive primitives.
pub fn spec_bounds(spec: &ShapeSpec) -> SpecBounds {
    bounds_filtered(spec, |_| true).unwrap_or(SpecBounds { min: Vec3::splat(-1.0), max: Vec3::ONE })
}

/// Bounds of one part's additive primitives.
pub fn spec_bounds_of_part(spec: &ShapeSpec, part: PartId) -> Option<SpecBounds> {
    bounds_filtered(spec, |p| p == part)
}

fn has_role(spec: &ShapeSpec, f: impl Fn(PartRole) -> bool) -> bool {
    spec.parts.iter().any(|p| f(p.role))
}

fn role_part(spec: &ShapeSpec, role: PartRole) -> Option<PartId> {
    spec.parts.iter().find(|p| p.role == role).map(|p| p.id)
}

/// Volume-weighted centroid of the additive prims of `part` (falls back to the bounds centre).
fn centroid(spec: &ShapeSpec, part: PartId, fb: Vec3) -> Vec3 {
    let mut acc = Vec3::ZERO;
    let mut w = 0.0;
    for p in spec.prims.iter().filter(|p| !p.subtract && p.part == part && p.center.is_finite() && p.size.is_finite()) {
        let v = (p.size.x * p.size.y * p.size.z).abs().min(1e9);
        acc += p.center * v;
        w += v;
    }
    if w > 0.0 { acc / w } else { fb }
}

/// Mount attach point (bd_core v1.6 `Recipe::mount`): put `mount` (a real-size spec, e.g.
/// `forms::generate(mount_recipe)`) under `rider`'s feet. The rider is lifted so its feet (lowest
/// point) stand on the mount's top, both centred on the rider's footprint in X/Z; the combined origin
/// stays feet-centre on the ground (the mount's bottom). Mount parts are re-numbered after the rider's
/// and keep their roles (Wheel parts roll, Rotor spins...); the mount's root prims join the rider's
/// root part; its seat marker is dropped (the rider is the seat).
pub fn attach_mount(rider: &mut ShapeSpec, mount: &ShapeSpec) {
    if mount.prims.is_empty() {
        return;
    }
    let rb = spec_bounds(rider);
    let mb = spec_bounds(mount);
    // mount: footprint centred under the rider, bottom on y = 0
    let m_off = Vec3::new(rb.center().x - mb.center().x, -mb.min.y, rb.center().z - mb.center().z);
    // rider: feet on the mount's top surface
    let lift = (mb.max.y - mb.min.y) - rb.min.y;
    for p in &mut rider.prims {
        p.center.y += lift;
    }
    for p in &mut rider.parts {
        p.pivot.y += lift;
    }
    let base = rider.parts.iter().map(|p| p.id).filter(|&id| id != SEAT_PART).max().unwrap_or(0);
    let remap = |id: PartId| -> PartId {
        if id == 0 || id == SEAT_PART { 0 } else { base.saturating_add(id).min(254) }
    };
    for p in &mount.parts {
        if p.id == SEAT_PART {
            continue;
        }
        let mut q = p.clone();
        q.id = remap(p.id);
        q.parent = remap(p.parent);
        q.pivot += m_off;
        rider.parts.push(q);
    }
    for p in &mount.prims {
        let mut q = p.clone();
        q.center += m_off;
        q.part = remap(p.part);
        rider.prims.push(q);
    }
}

/// Attach visible parts for `mechanics` to `spec`, placed relative to `bounds` (usually the body /
/// root part bounds — see [`spec_bounds_of_part`]).
pub fn attach_mechanic_parts(spec: &mut ShapeSpec, mechanics: &[Mechanic], bounds: SpecBounds) {
    attach_mechanic_parts_filtered(spec, mechanics, bounds, false)
}

pub(crate) fn attach_mechanic_parts_filtered(spec: &mut ShapeSpec, mechanics: &[Mechanic], bounds: SpecBounds, held: bool) {
    use Mechanic::*;
    let e = bounds.size();
    let c = bounds.center();
    let s = e.max_element();
    let cen = centroid(spec, 0, c);
    let head = role_part(spec, PartRole::Head);
    let head_b = head.and_then(|h| spec_bounds_of_part(spec, h));
    let mut b = B::from_spec(std::mem::take(spec));
    let has = |m: Mechanic| mechanics.contains(&m);

    if (has(Flies) || has(Glide)) && !has_role(&b.spec, |r| matches!(r, PartRole::WingLeft | PartRole::WingRight)) {
        let style = if has(Flies) { WingStyle::Feather } else { WingStyle::Jet };
        let span = (0.62 * e.x.max(e.z).max(e.y * 0.8)).clamp(3.5, 40.0);
        let root = v3((e.x * 0.5 - 0.5).max(0.5), bounds.min.y + e.y * 0.72, cen.z);
        wing_pair(&mut b, 0, root + v3(c.x, 0.0, 0.0), span, style, 22.0);
    }
    if has(Thrust) {
        let r = (0.14 * e.x.min(e.y)).clamp(1.0, 5.0);
        let at = v3(c.x, if held { 0.0 } else { c.y }, bounds.max.z);
        b.on(0);
        if e.x >= 7.0 && !held {
            exhaust(&mut b, 0, at + v3(-e.x * 0.25, 0.0, 0.0), Vec3::Z, r);
            b.on(0);
            exhaust(&mut b, 0, at + v3(e.x * 0.25, 0.0, 0.0), Vec3::Z, r);
        } else {
            exhaust(&mut b, 0, at, Vec3::Z, r);
        }
    }
    b.on(0);
    if has(Pierce) {
        let len = (0.35 * s).clamp(2.0, 12.0);
        let d = (0.12 * s).clamp(1.5, 5.0);
        let z = bounds.min.z + 0.5;
        let mut pts = vec![v3(c.x, c.y, z)];
        if e.x >= 6.0 && e.y >= 6.0 {
            pts.push(v3(c.x - e.x * 0.3, c.y - e.y * 0.2, z));
            pts.push(v3(c.x + e.x * 0.3, c.y - e.y * 0.2, z));
        }
        for p in pts {
            b.cone_to(p, p + Vec3::NEG_Z * len, d, C);
        }
    }
    if has(Blade) {
        let r = (0.15 * s).clamp(1.5, 4.0);
        let at = v3(c.x, bounds.min.y + e.y * 0.35, bounds.min.z);
        // Its own Blade part (pivot at the middle): slides out on summon, flicks on attack.
        b.rpart(0, at, PartRole::Blade);
        b.bxr(at, v3(e.x * 0.95, r, r), v3(45.0, 0.0, 0.0), C);
        b.on(0);
    }
    if has(ArticulatedTail) && !has_role(&b.spec, |r| r == PartRole::Tail) {
        let r0 = (0.1 * s).clamp(1.0, 8.0);
        tail(
            &mut b,
            0,
            &TailSpec {
                start: v3(c.x, bounds.min.y + e.y * 0.4, bounds.max.z - 0.5),
                dir: v3(0.0, -0.2, 1.0),
                segments: 5,
                length: (0.8 * s).clamp(4.0, 60.0),
                r0,
                r1: (r0 * 0.35).max(0.6),
                bend: Quat::from_rotation_x(-0.12),
                tip: TailTip::Spike,
                plates: Some((A, 3)),
                flat_y: 1.0,
            },
        );
        b.on(0);
    }
    if has(Breath) && !has_role(&b.spec, |r| r == PartRole::Jaw) {
        if let (Some(hp), Some(hb)) = (head, head_b) {
            let he = hb.size();
            let hinge = v3(hb.center().x, hb.min.y + he.y * 0.3, hb.center().z + he.z * 0.2);
            jaw(&mut b, hp, hinge, he.z * 0.75, (he.x * 0.7).max(1.0), (he.y * 0.25).max(1.0), 25.0, true);
            b.on(hp).bx(hinge + v3(0.0, 0.5, -he.z * 0.3), v3((he.x * 0.5).max(1.0), 1.0, 2.0), C);
        } else {
            // Maw on the body front: glowing throat + hinged lower jaw.
            let len = (0.4 * e.z.max(e.x * 0.5)).clamp(3.0, 20.0);
            let w = (e.x * 0.7).clamp(1.0, 30.0);
            let hinge = v3(c.x, bounds.min.y + e.y * 0.3, bounds.min.z + len * 0.5);
            b.bx(v3(c.x, hinge.y + 1.0, bounds.min.z), v3(w, 1.0, 2.0), C);
            for k in [-1.0f32, 0.0, 1.0] {
                let p = v3(c.x + k * w * 0.35, hinge.y + 1.5, bounds.min.z - 0.5);
                b.cone_to(p, p + v3(0.0, -1.8, 0.0), 1.3, C);
            }
            jaw(&mut b, 0, hinge, len, w, (e.y * 0.15).clamp(1.0, 6.0), 25.0, true);
        }
        b.on(0);
    }
    if has(Grab) && !has_role(&b.spec, |r| matches!(r, PartRole::ArmLeft | PartRole::ArmRight)) {
        let size = (0.5 * s).clamp(3.0, 20.0);
        for sd in [-1.0f32, 1.0] {
            b.on(0);
            pincer(&mut b, 0, v3(c.x + sd * (e.x * 0.5 - 0.5).max(0.5), c.y, bounds.min.z + 1.0), sd, size);
        }
        b.on(0);
    }
    if has(Charge) {
        let len = (0.3 * s).clamp(3.0, 16.0);
        if let (Some(hp), Some(hb)) = (head, head_b) {
            b.on(hp);
            horns(&mut b, v3(hb.center().x, hb.max.y - 0.5, hb.min.z + hb.size().z * 0.3), (hb.size().x * 0.3).max(1.0), len, HornStyle::Bull);
        } else {
            horns(&mut b, v3(c.x, bounds.max.y - 0.5, bounds.min.z + e.z * 0.15), (e.x * 0.3).max(1.0), len, HornStyle::Bull);
        }
        b.on(0);
    }
    if has(Walks) && !has_role(&b.spec, |r| matches!(r, PartRole::LegFrontLeft | PartRole::LegFrontRight | PartRole::LegBackLeft | PartRole::LegBackRight)) {
        let len = (0.45 * e.y.max(s * 0.5)).clamp(3.0, 30.0);
        let lw = (0.12 * s).clamp(1.0, 6.0).round();
        let y0 = bounds.min.y + 1.0;
        let biped = e.x >= 1.5 * e.z;
        let spots: Vec<(Vec3, PartRole)> = if biped {
            vec![
                (v3(c.x - e.x * 0.3, y0, cen.z), PartRole::LegBackLeft),
                (v3(c.x + e.x * 0.3, y0, cen.z), PartRole::LegBackRight),
            ]
        } else {
            let lx = (e.x * 0.35).max(lw);
            let lz = e.z * 0.33;
            vec![
                (v3(c.x - lx, y0, cen.z - lz), PartRole::LegFrontLeft),
                (v3(c.x + lx, y0, cen.z - lz), PartRole::LegFrontRight),
                (v3(c.x - lx, y0, cen.z + lz), PartRole::LegBackLeft),
                (v3(c.x + lx, y0, cen.z + lz), PartRole::LegBackRight),
            ]
        };
        for (hip, role) in spots {
            let knee = hip + v3(0.0, -len * 0.5, len * 0.22);
            let foot = hip + v3(0.0, -len, 0.0);
            leg(&mut b, 0, role, hip, Some(knee), foot, lw, lw + 2.0, Vec3::NEG_Z, PrimKind::Box);
            b.bx(hip, Vec3::splat(lw + 1.0), D);
        }
        b.on(0);
    }
    if has(Spin) {
        hub(&mut b, v3(c.x, bounds.max.y, c.z), (0.25 * e.x.max(e.z)).clamp(2.0, 10.0));
    }
    if has(Bounce) {
        let h = (0.15 * s).clamp(2.0, 6.0);
        let d = (0.12 * s).clamp(1.5, 4.0);
        let spots: Vec<Vec3> = if e.x.min(e.z) >= 6.0 {
            vec![v3(-1.0, 0.0, -1.0), v3(1.0, 0.0, -1.0), v3(-1.0, 0.0, 1.0), v3(1.0, 0.0, 1.0)]
        } else {
            vec![Vec3::ZERO]
        };
        for sp in spots {
            let p = v3(c.x + sp.x * e.x * 0.3, bounds.min.y, c.z + sp.z * e.z * 0.3);
            spring(&mut b, p - Vec3::Y * h, p, d);
            b.cy(p - Vec3::Y * (h + 0.5), d + 1.0, 1.0, D);
        }
    }
    if has(Launch) {
        for i in 0..2 {
            let z = c.z - e.z * 0.1 + i as f32 * e.z * 0.25;
            let apex = v3(c.x, bounds.max.y + 0.5, z - e.z * 0.12);
            b.sym(|b, sd| {
                b.seg(PrimKind::Box, v3(c.x + sd * e.x * 0.3, bounds.max.y + 0.5, z + e.z * 0.05), apex, 1.0, 1.0, C);
            });
        }
    }
    if has(Guard) || has(Barrier) {
        let d = (0.6 * e.x.min(e.y)).clamp(3.0, 20.0);
        let z = bounds.min.z - 0.5;
        b.cz(v3(c.x, c.y, z), d + 1.0, 1.0, D);
        b.cz(v3(c.x, c.y, z - 0.5), d - 1.0, 1.0, A);
        b.el(v3(c.x, c.y, z - 1.0), v3(d * 0.3, d * 0.3, 2.0), A);
        b.bx(v3(c.x, c.y, z - 2.0), Vec3::ONE, C);
    }
    if has(Shoots) {
        let d = (0.15 * e.x.min(e.y)).clamp(1.0, 5.0).round().max(1.0);
        let bl = (0.35 * s).clamp(2.0, 14.0);
        let z = bounds.min.z;
        // A Barrel part rooted at the body face: pitches toward the aim, recoils on every shot.
        b.rpart(0, v3(c.x, c.y, z + 0.5), PartRole::Barrel);
        b.cz(v3(c.x, c.y, z - bl * 0.5 + 0.5), d, bl, D);
        b.cz(v3(c.x, c.y, z - bl + 1.0), d + 1.0, 1.0, A);
        b.cz(v3(c.x, c.y, z - bl), (d - 1.0).max(1.0), 1.0, C);
        b.on(0);
    }
    if has(Beam) {
        let d = (0.35 * e.x.min(e.y)).clamp(2.0, 12.0);
        let z = bounds.min.z - 0.5;
        b.cz(v3(c.x, c.y, z), d + 1.0, 1.0, A);
        b.cz(v3(c.x, c.y, z - 0.5), d - 1.0, 1.0, C);
    }
    if has(Explodes) {
        let top = v3(c.x, bounds.max.y, c.z);
        let h = (0.2 * s).clamp(2.0, 8.0);
        b.cy(top + v3(0.0, 0.5, 0.0), 3.0, 1.0, D);
        b.seg(PrimKind::Box, top, top + v3(0.0, h, h * 0.3), 1.0, 1.0, D);
        let sp = top + v3(0.0, h, h * 0.3);
        b.bx(sp, v3(3.0, 1.0, 1.0), C);
        b.bx(sp, v3(1.0, 3.0, 1.0), C);
        b.bx(sp, v3(1.0, 1.0, 3.0), C);
    }
    if has(Homing) {
        let p = v3(c.x, bounds.max.y - 0.5, bounds.min.z + 0.5);
        b.el(p, v3(3.0, 3.0, 3.0), A);
        b.bx(p + v3(0.0, 0.0, -1.2), v3(1.0, 1.0, 1.0), C);
    }
    if has(Sticky) {
        for k in [-1.0f32, 0.0, 1.0] {
            let p = v3(c.x + k * e.x * 0.3, bounds.min.y, c.z + k * e.z * 0.2);
            b.el(p, v3(2.0, 3.0, 2.0), A);
            b.el(p - Vec3::Y * 2.0, v3(1.0, 1.5, 1.0), A);
        }
    }
    if has(SlowFall) {
        let top = v3(c.x, bounds.max.y, c.z);
        let h = (0.5 * s).clamp(3.0, 16.0);
        let r = (0.55 * s).clamp(3.0, 18.0);
        b.seg(PrimKind::Box, top, top + Vec3::Y * h, 1.0, 1.0, D);
        let apex = top + Vec3::Y * (h + r * 0.35);
        let n = 6;
        for i in 0..n {
            let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
            let r0 = top + v3(a0.cos() * r, h, a0.sin() * r);
            let r1 = top + v3(a1.cos() * r, h, a1.sin() * r);
            let nrm = (r0 - apex).cross(r1 - apex).normalize_or(Vec3::Y);
            b.fan(apex, r0, r1, 0.0, 1.0, nrm, if i % 2 == 0 { P } else { A });
        }
    }
    if has(Pull) {
        let p = v3(c.x, c.y, bounds.min.z);
        let l = (0.25 * s).clamp(2.0, 8.0);
        b.seg(PrimKind::Box, p, p + v3(0.0, 0.0, -l), 1.0, 1.0, D);
        b.seg(PrimKind::Box, p + v3(0.0, 0.0, -l), p + v3(0.0, -l * 0.5, -l * 1.2), 1.0, 1.0, D);
        b.cone_to(p + v3(0.0, -l * 0.5, -l * 1.2), p + v3(0.0, -l * 0.2, -l * 0.8), 1.5, C);
    }
    if has(Fast) {
        b.sym(|b, sd| {
            let root = v3(c.x + sd * e.x * 0.5, c.y, bounds.max.z - e.z * 0.2);
            b.flat(PrimKind::Box, root, root + v3(sd * (0.25 * s).clamp(2.0, 8.0), 0.0, (0.3 * s).clamp(2.0, 10.0)), 2.0, 1.0, Vec3::Y, A);
        });
    }
    // ---- second-wave verbs: each gets a unique front-mounted silhouette tell.
    let front = |k: f32| v3(c.x, c.y + k, bounds.min.z);
    let em = (0.45 * e.x.min(e.y).max(2.0)).clamp(3.0, 16.0);
    if has(Portal) {
        // Hoop on a stalk: Accent outer rim, Core inner rim, empty middle.
        let d = odd_f(em * 1.3);
        let y = 0.0;
        let cpos = front(y) + v3(0.0, 0.0, -2.5);
        b.seg(PrimKind::Box, front(y), cpos + v3(0.0, -d * 0.5, 0.0), 1.0, 1.0, D);
        portal_ring(&mut b, cpos, d);
    }
    if has(Attract) {
        // Horseshoe magnet prongs with glowing tips.
        let w = em;
        let l = (em * 0.9).max(3.0);
        let y = if has(Portal) { e.y * 0.35 } else { 0.0 };
        let base = front(y);
        b.bx(base + v3(0.0, 0.0, -0.5), v3(w + 2.0, 2.0, 2.0), A);
        b.sym(|b, s| {
            let p0 = base + v3(s * w * 0.5, 0.0, -0.5);
            b.seg(PrimKind::Box, p0, p0 + v3(0.0, 0.0, -l), 2.0, 2.0, A);
            b.bx(p0 + v3(0.0, 0.0, -l - 0.5), v3(2.0, 2.0, 2.0), C);
        });
        b.bx(base + v3(0.0, 0.0, -l * 0.5), v3(1.0, 1.0, 1.0), C);
    }
    if has(Repel) {
        // Fan: shroud ring + 3 flat blades facing outward, glowing hub.
        let d = odd_f(em * 1.2);
        let y = if has(Portal) || has(Attract) { -e.y * 0.3 } else { 0.0 };
        let cpos = front(y) + v3(0.0, 0.0, -1.5);
        b.cz(cpos + v3(0.0, 0.0, 1.0), 1.0, 2.0, D);
        b.cz(cpos, d + 2.0, 1.0, A);
        b.cut(PrimKind::Cylinder, cpos, v3(d, 3.0, d), v3(90.0, 0.0, 0.0));
        for i in 0..3 {
            let a = i as f32 * std::f32::consts::TAU / 3.0 + 0.3;
            let dir = v3(a.cos(), a.sin(), 0.0);
            b.flat(PrimKind::Box, cpos, cpos + dir * (d * 0.48), (d * 0.28).max(1.5), 1.0, v3(0.3 * -dir.y, 0.3 * dir.x, 1.0), P);
        }
        b.bx(cpos + v3(0.0, 0.0, -0.5), v3(1.0, 1.0, 1.0), C);
    }
    if has(Shrink) || has(Grow) {
        // Ray barrel: Shrink narrows to a point, Grow flares into a bell.
        let bd = (0.18 * e.x.min(e.y)).clamp(1.0, 5.0).round().max(1.0);
        let bl = (0.3 * s).clamp(3.0, 12.0);
        let y = if has(Portal) || has(Attract) || has(Repel) { e.y * 0.2 } else { 0.0 };
        let x = if has(Shrink) && has(Grow) { e.x * 0.25 } else { 0.0 };
        for (i, grow) in [(0, false), (1, true)] {
            if (grow && !has(Grow)) || (!grow && !has(Shrink)) {
                continue;
            }
            let sx = if i == 0 { -x } else { x };
            let base = front(y) + v3(sx, 0.0, 0.0);
            b.cz(base + v3(0.0, 0.0, -bl * 0.5), bd, bl, D);
            b.cz(base + v3(0.0, 0.0, -bl * 0.3), bd + 2.0, 1.0, A);
            b.cz(base + v3(0.0, 0.0, -bl * 0.6), bd + 2.0, 1.0, A);
            let tip = base + v3(0.0, 0.0, -bl);
            if grow {
                b.cone_to(tip + v3(0.0, 0.0, -bl * 0.45), tip + v3(0.0, 0.0, 0.5), bd * 2.8 + 2.0, A);
                b.cz(tip + v3(0.0, 0.0, -bl * 0.3), bd * 1.5 + 1.0, 1.0, C);
            } else {
                b.cone_to(tip + v3(0.0, 0.0, 0.5), tip + v3(0.0, 0.0, -bl * 0.6), bd + 2.0, C);
            }
        }
    }
    if has(Link) {
        // A dangling chain of alternating links (its own swaying chain of parts) with a glowing shackle.
        let size = (0.12 * s).clamp(1.5, 5.0);
        let start = v3(c.x + e.x * 0.5, bounds.min.y + e.y * 0.6, c.z);
        let mut parent = 0;
        let mut p = start;
        for i in 0..5 {
            parent = b.part(parent, p, PartRole::Other);
            link(&mut b, p + v3(0.0, -size, 0.0), size, i % 2 == 1, if i == 4 { C } else { A });
            p += v3(0.0, -size * 1.6, 0.0);
        }
        b.on(0);
    }

    if has(Piloted) && !b.spec.parts.iter().any(|p| p.id == SEAT_PART) {
        cockpit(&mut b, bounds, cen);
    }
    b.on(0);
    *spec = b.spec;
}

fn odd_f(x: f32) -> f32 {
    crate::build::odd(x)
}

/// One chain link: a rectangular loop of 4 boxes, `size` long along Y, in the XY plane (or ZY when
/// `turned`).
pub(crate) fn link(b: &mut B, center: Vec3, size: f32, turned: bool, t: bd_core::shape::Tint) {
    let h = size;
    let w = (size * 0.6).max(1.0);
    let side = if turned { Vec3::Z } else { Vec3::X };
    for s in [-1.0f32, 1.0] {
        b.seg(PrimKind::Box, center + side * (s * w * 0.5) + Vec3::Y * (h * 0.5), center + side * (s * w * 0.5) - Vec3::Y * (h * 0.5), 1.0, 1.0, t);
        b.seg(PrimKind::Box, center + Vec3::Y * (s * h * 0.5) - side * (w * 0.5), center + Vec3::Y * (s * h * 0.5) + side * (w * 0.5), 1.0, 1.0, t);
    }
}

/// Flat hoop facing −Z (in the XY plane) centred on `c`: Dark back ring, Accent outer rim, Core
/// inner rim, open middle. `d` = outer diameter in construct units.
pub(crate) fn portal_ring(b: &mut B, c: Vec3, d: f32) {
    let d = d.max(5.0);
    let rim = (d * 0.12).clamp(1.0, 4.0).round();
    b.cz(c + v3(0.0, 0.0, 1.0), d, 1.0, D);
    b.cz(c, d + 1.0, 2.0, A);
    b.cz(c + v3(0.0, 0.0, -0.5), d + 1.0 - rim * 2.0, 1.0, crate::build::C);
    b.cut(PrimKind::Cylinder, c, v3(d + 1.0 - rim * 4.0, 5.0, d + 1.0 - rim * 4.0), v3(90.0, 0.0, 0.0));
}

/// Standalone portal ring (Mechanic::Portal's gateway): a flat torus standing upright, facing −Z,
/// origin at its centre. `diameter_m` is clamped to 1–40 m.
pub fn portal_ring_spec(diameter_m: f32) -> ShapeSpec {
    let d = crate::build::odd((if diameter_m.is_finite() { diameter_m } else { 2.5 }).clamp(1.0, 40.0) / bd_core::CONSTRUCT_VOXEL_SIZE);
    let mut b = B::new();
    portal_ring(&mut b, Vec3::ZERO, d);
    b.spec
}

/// A straight chain (Mechanic::Link) running from the origin along +Z for `length_m` (quantised to
/// whole links; clamped 0.5–30 m) at 0.25 m voxels — spec form. See [`crate::chain_link_mesh`].
pub fn chain_spec(length_m: f32) -> ShapeSpec {
    let len = (if length_m.is_finite() { length_m } else { 1.0 }).clamp(0.5, 30.0) / bd_core::CONSTRUCT_VOXEL_SIZE;
    let size = 3.0;
    let n = ((len / (size * 0.85)).round() as usize).clamp(1, 200);
    let mut b = B::new();
    for i in 0..n {
        let z = size * 0.5 + i as f32 * size * 0.85;
        // Links along Z: build in the XY-plane variant, then swap axes via a rotated centre frame.
        let t = if i % 2 == 0 { A } else { P };
        let (w, h) = ((size * 0.6).max(1.0), size);
        let side = if i % 2 == 0 { Vec3::X } else { Vec3::Y };
        let cpos = v3(0.0, 0.0, z);
        for s in [-1.0f32, 1.0] {
            b.seg(PrimKind::Box, cpos + side * (s * w * 0.5) + Vec3::Z * (h * 0.5), cpos + side * (s * w * 0.5) - Vec3::Z * (h * 0.5), 1.0, 1.0, t);
            b.seg(PrimKind::Box, cpos + Vec3::Z * (s * h * 0.5) - side * (w * 0.5), cpos + Vec3::Z * (s * h * 0.5) + side * (w * 0.5), 1.0, 1.0, t);
        }
    }
    b.spec
}

/// Cockpit / saddle for `Mechanic::Piloted` + the seat marker part ([`SEAT_PART`]).
/// Mechs (arms + head) get a glowing chest canopy, mounts (legs/tail/wings, no arms) a saddle on the
/// back, everything else an open cockpit tub on top.
fn cockpit(b: &mut B, bounds: SpecBounds, cen: Vec3) {
    let e = bounds.size();
    let c = bounds.center();
    let roles = |f: &dyn Fn(PartRole) -> bool| b.spec.parts.iter().any(|p| f(p.role));
    let arms = roles(&|r| matches!(r, PartRole::ArmLeft | PartRole::ArmRight));
    let head = roles(&|r| r == PartRole::Head);
    let mount = roles(&|r| matches!(r, PartRole::LegFrontLeft | PartRole::LegFrontRight | PartRole::LegBackLeft | PartRole::LegBackRight | PartRole::Tail | PartRole::WingLeft | PartRole::WingRight));
    b.on(0);
    let seat;
    if arms && head {
        // Chest canopy (mech / jaeger).
        let y = bounds.min.y + e.y * 0.72;
        let w = (e.x * 0.5).max(3.0);
        let h = (e.y * 0.2).max(2.0);
        let z = bounds.min.z;
        b.bx(v3(c.x, y, z + 0.5), v3(w + 2.0, h + 2.0, 2.0), A);
        b.bx(v3(c.x, y - h * 0.5 - 1.0, z), v3(w + 2.0, 1.0, 2.0), D);
        // The glass is a Hatch hinged along its bottom edge: it swings down/out for the pilot.
        let glass = v3(c.x, y, z - 0.2);
        let hatch = b.rpart(0, glass - Vec3::Y * (h * 0.5), PartRole::Hatch);
        if hatch != 0 {
            b.cut(PrimKind::Ellipsoid, glass, v3(w, h, 2.5), Vec3::ZERO);
            b.on(hatch);
        }
        b.el(glass, v3(w, h, 2.5), C);
        b.on(0);
        seat = v3(c.x, y - h * 0.3, z + e.z * 0.3);
    } else if mount {
        // Saddle on the back.
        let top = bounds.max.y;
        let z = cen.z - e.z * 0.08;
        let w = (e.x * 0.55).max(3.0);
        let l = (e.z * 0.3).max(3.0);
        b.bx(v3(c.x, top + 0.5, z), v3(w, 1.5, l), D);
        b.bx(v3(c.x, top + 1.5, z - l * 0.5 + 0.5), v3(w * 0.6, 2.0, 1.0), A);
        b.bx(v3(c.x, top + 1.5, z + l * 0.5 - 0.5), v3(w * 0.8, 1.0, 1.0), A);
        b.bx(v3(c.x, top + 3.0, z - l * 0.5 + 0.5), v3(1.0, 1.0, 1.0), C);
        b.sym(|b, s| {
            b.seg(PrimKind::Box, v3(c.x + s * w * 0.5, top + 0.5, z), v3(c.x + s * (w * 0.5 + 0.5), top - e.y * 0.25, z), 1.0, 1.0, D);
            b.bx(v3(c.x + s * (w * 0.5 + 0.5), top - e.y * 0.25, z), v3(1.0, 1.0, 2.0), A);
        });
        seat = v3(c.x, top + 1.5, z);
    } else {
        // Open cockpit tub with a glowing windscreen.
        let top = bounds.max.y;
        let w = (e.x * 0.4).clamp(3.0, 12.0);
        let l = (e.z * 0.35).clamp(3.0, 12.0);
        let z = c.z;
        b.bx(v3(c.x, top + 0.5, z), v3(w + 2.0, 1.0, l + 2.0), D);
        b.sym(|b, s| {
            b.bx(v3(c.x + s * (w * 0.5 + 0.5), top + 1.5, z), v3(1.0, 2.0, l + 2.0), A);
        });
        b.bx(v3(c.x, top + 1.5, z + l * 0.5 + 0.5), v3(w, 2.0, 1.0), A);
        // Windscreen = a Hatch hinged at its base: folds forward for the pilot, snaps up after.
        let ws = b.rpart(0, v3(c.x, top + 1.0, z - l * 0.5 - 0.5), PartRole::Hatch);
        b.bxr(v3(c.x, top + 2.5, z - l * 0.5 - 0.5), v3(w + 2.0, 3.0, 1.0), v3(-25.0, 0.0, 0.0), C);
        b.on(0);
        let _ = ws;
        seat = v3(c.x, top + 1.5, z);
    }
    b.spec.parts.push(Part { id: SEAT_PART, parent: 0, pivot: seat, role: PartRole::Other });
}
