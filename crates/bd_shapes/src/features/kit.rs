//! Machine-kit part builders (ShapeSpec units). Static geometry goes on the current part; moving
//! assemblies declare their own parts through [`B::mpart`] (which records the motion link).
//! Tints: Dark = iron (tyres, frames), Primary = body, Accent = trim/brass, Core = glowing pins,
//! lamps and fire — the "every nut and bolt" highlights.

use std::f32::consts::{PI, TAU};

use bd_core::shape::{PartId, PartRole, PrimKind, Tint};
use glam::{Quat, Vec3};

use super::{CrankMember, MAX_ANIMATED_PARTS, MotionSpec, PuffSpec};
use crate::build::{A, B, C, D, P, v3};

/// Two unit vectors spanning the plane ⟂ `axis` (for X: u = +Y, v = +Z).
pub(crate) fn perp_basis(axis: Vec3) -> (Vec3, Vec3) {
    let a = axis.normalize_or(Vec3::X);
    let r = if a.x.abs() > 0.9 { Vec3::Y } else { Vec3::X };
    let u = (r - a * r.dot(a)).normalize_or(Vec3::Y);
    (u, a.cross(u))
}

/// Unit vector at angle `ang` in the plane ⟂ `axis`.
pub(crate) fn around(axis: Vec3, ang: f32) -> Vec3 {
    let (u, v) = perp_basis(axis);
    u * ang.cos() + v * ang.sin()
}

impl B {
    /// Declare a machine part with a motion (and make it current). Once the spec holds
    /// [`MAX_ANIMATED_PARTS`] parts, returns `parent` instead: the geometry is built static.
    pub(crate) fn mpart(&mut self, parent: PartId, pivot: Vec3, role: PartRole, motion: MotionSpec) -> PartId {
        if self.spec.parts.len() + 2 > MAX_ANIMATED_PARTS || !pivot.is_finite() {
            self.cur = parent;
            return parent;
        }
        let id = self.part(parent, pivot, role);
        self.machine.links.push((id, motion));
        id
    }

    pub(crate) fn puff(&mut self, p: PuffSpec) -> u8 {
        let k = self.machine.puffs.len();
        if k < 16 {
            self.machine.puffs.push(p);
        }
        k.min(15) as u8
    }

    /// Cylinder along `axis` (any direction).
    pub(crate) fn cyl(&mut self, c: Vec3, axis: Vec3, d: f32, len: f32, t: Tint) -> &mut Self {
        let q = Quat::from_rotation_arc(Vec3::Y, axis.normalize_or(Vec3::Y));
        self.q(PrimKind::Cylinder, c, v3(d, len, d), q, t)
    }

    /// Bar (box) from `a` to `b`.
    pub(crate) fn bar(&mut self, a: Vec3, b: Vec3, w: f32, t: Tint) -> &mut Self {
        self.seg(PrimKind::Box, a, b, w, w, t)
    }

    /// Bolt heads (1-voxel studs) around a circle on a face.
    pub(crate) fn bolts(&mut self, c: Vec3, axis: Vec3, r: f32, n: usize, t: Tint) -> &mut Self {
        for i in 0..n {
            let p = c + around(axis, i as f32 / n as f32 * TAU + 0.4) * r;
            self.bx(p, Vec3::ONE, t);
        }
        self
    }

    /// A row of rivets from `a` to `b`, one every `step`.
    pub(crate) fn rivets(&mut self, a: Vec3, b: Vec3, step: f32, t: Tint) -> &mut Self {
        let n = (((b - a).length() / step.max(1.0)).round() as usize).clamp(1, 24);
        for i in 0..=n {
            self.bx(a.lerp(b, i as f32 / n as f32), Vec3::ONE, t);
        }
        self
    }
}

/// Open ring (a loop of short flat bars) in the plane ⟂ `axis`: `thick` along the axis, `w` radial.
pub(crate) fn ring(b: &mut B, c: Vec3, axis: Vec3, r: f32, thick: f32, w: f32, t: Tint) {
    let a = axis.normalize_or(Vec3::X);
    if r < 1.6 {
        b.cyl(c, a, 2.0 * r + w, thick, t);
        return;
    }
    let n = ((TAU * r / 1.5).ceil() as usize).clamp(8, 48);
    for i in 0..n {
        let p0 = c + around(a, i as f32 / n as f32 * TAU) * r;
        let p1 = c + around(a, (i + 1) as f32 / n as f32 * TAU) * r;
        let d = (p1 - p0).normalize_or(Vec3::Y);
        b.flat(PrimKind::Box, p0 - d * 0.35, p1 + d * 0.35, w, thick, a, t);
    }
}

/// Spokes from radius `r0` to `r1`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spokes(b: &mut B, c: Vec3, axis: Vec3, r0: f32, r1: f32, n: usize, thick: f32, w: f32, phase: f32, t: Tint) {
    for i in 0..n {
        let d = around(axis, phase + i as f32 / n as f32 * TAU);
        b.flat(PrimKind::Box, c + d * r0, c + d * r1, w, thick, axis, t);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WheelStyle {
    /// Locomotive driver: spoked, counterweight + crank boss at `boss` angle.
    Driver,
    /// Open spoked wheel (carts, bogies, clockwork).
    Spoked,
    /// Solid tyre with hub cap and lug nuts.
    Tire,
    /// Tread road wheel.
    Road,
    /// Toothed tread sprocket.
    Sprocket,
}

/// A wheel on an X axle centred at `c` (diameter `d`, width `width` along X). Rolls with distance.
pub(crate) fn wheel(b: &mut B, parent: PartId, c: Vec3, d: f32, width: f32, style: WheelStyle, boss: f32) -> PartId {
    let id = b.mpart(parent, c, PartRole::Wheel, MotionSpec::roll());
    let r = (d * 0.5).max(1.0);
    let ax = Vec3::X;
    let w = width.max(1.0);
    match style {
        WheelStyle::Driver | WheelStyle::Spoked => {
            ring(b, c, ax, r - 0.6, w, 1.3, if style == WheelStyle::Driver { A } else { D });
            if r >= 2.5 {
                let n = if r >= 4.5 { 10 } else if r >= 3.0 { 8 } else { 6 };
                spokes(b, c, ax, 0.9, r - 0.9, n, 1.0, 1.0, 0.0, P);
            } else {
                b.cx(c, 2.0 * r - 1.0, 1.0, P);
            }
            b.cx(c, (r * 0.55).max(2.0), w + 0.4, A);
            b.cx(c, 1.0, w + 2.0, C);
            if style == WheelStyle::Driver && r >= 2.5 {
                // Counterweight opposite the crank (Accent crescent) + crank boss.
                let cw = around(ax, boss + PI);
                b.flat(PrimKind::Box, c + cw * (r * 0.3), c + cw * (r - 1.1), r * 0.95, w - 0.2, ax, D);
                let bd = around(ax, boss);
                b.cx(c + bd * (r * 0.5), 2.0, w + 0.4, D);
            }
        }
        WheelStyle::Tire => {
            b.cx(c, 2.0 * r, w, D);
            b.cx(c, (2.0 * r * 0.58).max(2.0), w + 0.6, A);
            b.cx(c, 1.0, w + 1.2, C);
            for k in 0..3 {
                let p = c + around(ax, boss + k as f32 * TAU / 3.0) * (r * 0.42).max(1.0);
                b.cx(p, 1.0, w + 0.9, D);
            }
        }
        WheelStyle::Road => {
            b.cx(c, 2.0 * r, w, P);
            b.cx(c, (r * 0.9).max(1.5), w + 0.6, A);
            b.cx(c + around(ax, boss) * (r * 0.55), 1.0, w + 0.8, C);
        }
        WheelStyle::Sprocket => {
            b.cx(c, 2.0 * r - 0.8, w, D);
            let n = ((TAU * r / 1.8).round() as usize).clamp(6, 20);
            for i in 0..n {
                let dd = around(ax, (i as f32 + 0.5) / n as f32 * TAU);
                b.flat(PrimKind::Box, c + dd * (r - 1.0), c + dd * (r + 0.1), 1.0, w + 0.2, ax, A);
            }
            b.cx(c, (r * 0.7).max(1.5), w + 0.6, A);
            b.cx(c, 1.0, w + 1.2, C);
        }
    }
    b.on(parent);
    id
}

/// A spur gear ⟂ `axis` (pitch radius `r`), open-spoked, with teeth. `sign` alternates for meshing
/// gears; `tick` = clockwork stepping. Returns the part id.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gear(b: &mut B, parent: PartId, c: Vec3, axis: Vec3, r: f32, thick: f32, sign: f32, surf: f32, roll: f32, tick: bool) -> PartId {
    let a = axis.normalize_or(Vec3::X);
    let r = r.max(1.5);
    let id = b.mpart(parent, c, PartRole::Wheel, MotionSpec::Spin { axis: a, sign, rate: 0.0, surf, roll, radius: r, tick });
    let t = thick.max(1.0);
    if r >= 2.6 {
        ring(b, c, a, r - 0.5, t, 1.4, D);
        let n = if r >= 5.0 { 6 } else { 4 };
        spokes(b, c, a, 0.8, r - 0.8, n, t, 1.0, 0.0, P);
    } else {
        b.cyl(c, a, 2.0 * r, t, D);
    }
    let n = ((TAU * r / 1.9).round() as usize).clamp(6, 30);
    for i in 0..n {
        let d = around(a, (i as f32 + 0.5) / n as f32 * TAU);
        b.flat(PrimKind::Box, c + d * (r - 0.2), c + d * (r + 1.0), 1.0, t, a, A);
    }
    b.cyl(c, a, 2.0, t + 0.8, A);
    b.cyl(c, a, 1.0, t + 1.8, C);
    b.on(parent);
    id
}

/// A meshing gear train along `path` points (alternating direction), radii `radii`. The first gear
/// is driven by distance (`roll`) and idles at `surf` m/s surface speed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gear_train(b: &mut B, parent: PartId, start: Vec3, dir: Vec3, axis: Vec3, radii: &[f32], thick: f32, surf: f32, roll: f32, tick: bool) -> Vec<PartId> {
    let mut out = Vec::new();
    let d = dir.normalize_or(Vec3::Z);
    let mut c = start;
    let mut sign = 1.0;
    for (k, &r) in radii.iter().enumerate() {
        if k > 0 {
            c += d * (radii[k - 1] + r + 0.9);
        }
        out.push(gear(b, parent, c, axis, r, thick, sign, surf, roll, tick));
        sign = -sign;
    }
    out
}

/// Horizontal/vertical boiler from `a` to `e` (diameter `d`) on the current part: bands, rivet rows,
/// front smokebox door.
pub(crate) fn boiler(b: &mut B, a: Vec3, e: Vec3, d: f32, door: bool) {
    let dir = (e - a).normalize_or(Vec3::NEG_Z);
    let len = (e - a).length();
    b.seg(PrimKind::Cylinder, a, e, d, d, P);
    let n = ((len / 5.0).round() as usize).clamp(1, 8);
    for i in 1..n {
        let p = a.lerp(e, i as f32 / n as f32);
        b.seg(PrimKind::Cylinder, p - dir * 0.5, p + dir * 0.5, d + 1.0, d + 1.0, A);
    }
    // Rivet rows along the flanks.
    let side = dir.cross(Vec3::Y).normalize_or(Vec3::X);
    for s in [-1.0f32, 1.0] {
        let off = side * s * (d * 0.5 + 0.2) + Vec3::Y * (d * 0.15);
        b.rivets(a + off + dir * 1.0, e + off - dir * 1.0, 2.0, D);
    }
    if door {
        // Smokebox: dark front ring, door with hinge straps and a glowing number plate.
        b.seg(PrimKind::Cylinder, a - dir * 0.5, a + dir * 2.0, d + 0.6, d + 0.6, D);
        b.cyl(a - dir * 0.8, dir, (d * 0.72).max(2.0), 1.0, A);
        b.bar(a - dir * 1.2 + side * (d * 0.28), a - dir * 1.2 - side * (d * 0.28), 1.0, D);
        b.bx(a - dir * 1.4, Vec3::ONE, C);
        b.bolts(a - dir * 1.0, dir, d * 0.42, 8, D);
    }
}

/// Smokestack standing on `base` (height `h`, diameter `d`): one part that stretches on every puff.
/// The puff emitter chuffs `per_rev`× per revolution of `wheel` (if any) plus `idle_hz`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn smokestack(b: &mut B, parent: PartId, base: Vec3, h: f32, d: f32, wheel: Option<PartId>, per_rev: f32, idle_hz: f32) -> PartId {
    let top = base + Vec3::Y * h;
    let puff_idx = b.machine.puffs.len().min(15) as u8;
    let id = b.mpart(parent, base, PartRole::Other, MotionSpec::Pulse { puff: puff_idx });
    b.cy(base + Vec3::Y * (h * 0.5 - 0.5), d, h, D);
    b.cy(base + Vec3::Y * 0.5, d + 1.0, 1.0, A);
    // Flared crown (cone base up) + lip + hot mouth.
    b.cone_to(top + Vec3::Y * 1.0, top - Vec3::Y * (h * 0.35).max(1.5), d * 1.8, P);
    b.cy(top + Vec3::Y * 0.5, d * 1.8 + 0.6, 1.0, A);
    b.cy(top + Vec3::Y * 0.8, (d * 0.8).max(1.0), 1.0, C);
    b.on(parent);
    let pid = if id == parent { parent } else { id };
    b.puff(PuffSpec { part: pid, at: top + Vec3::Y * 1.5, dir: Vec3::Y, size: (d * 0.9).max(1.5), per_rev, wheel, idle_hz, per_m: 0.0 });
    id
}

/// Bell in a yoke hanging at `top` (bell height `s`): swings with speed.
pub(crate) fn bell(b: &mut B, parent: PartId, top: Vec3, s: f32) -> PartId {
    let s = s.max(2.0);
    let hinge = top + Vec3::Y * (s * 1.2);
    b.sym(|b, sd| {
        b.bar(top + v3(sd * s * 0.6, 0.0, 0.0), hinge + v3(sd * s * 0.6, 0.0, 0.0), 1.0, D);
    });
    b.bar(hinge + v3(-s * 0.6, 0.0, 0.0), hinge + v3(s * 0.6, 0.0, 0.0), 1.0, D);
    let id = b.mpart(parent, hinge, PartRole::Other, MotionSpec::Sway { axis: Vec3::X, amp: 0.1, hz: 0.8, phase: 0.0, speed_amp: 0.45, lean: 0.0 });
    b.cone_to(hinge - Vec3::Y * (s * 1.05), hinge - Vec3::Y * 0.2, s * 1.1, A);
    b.cy(hinge - Vec3::Y * (s * 0.95), s * 1.1 + 0.6, 1.0, A);
    b.bx(hinge - Vec3::Y * (s * 1.1), Vec3::ONE, C);
    b.on(parent);
    id
}

/// Headlamp at `pos` facing `dir`: housing, glowing lens, rim.
pub(crate) fn headlight(b: &mut B, pos: Vec3, dir: Vec3, s: f32) {
    let d = dir.normalize_or(Vec3::NEG_Z);
    let s = s.max(1.5);
    b.cyl(pos - d * (s * 0.4), d, s + 1.0, s, D);
    b.cyl(pos + d * (s * 0.1), d, s + 1.0, 1.0, A);
    b.cyl(pos + d * (s * 0.2), d, s - 0.5, 1.0, C);
}

/// Cowcatcher / plow wedge at the front (−Z) face `z`, spanning `w`, from `y0` (low) to `y1`.
pub(crate) fn plow(b: &mut B, cx: f32, z: f32, y0: f32, y1: f32, w: f32) {
    let h = (y1 - y0).max(2.0);
    let reach = h * 0.9;
    let tip = v3(cx, y0, z - reach);
    b.bar(v3(cx - w * 0.5, y1, z), v3(cx + w * 0.5, y1, z), 1.5, D);
    let n = ((w / 1.6).round() as usize).clamp(3, 14);
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let top = v3(cx - w * 0.5 + w * f, y1, z);
        let bottom = v3(cx + (f - 0.5) * w * 0.35, y0, z - reach * (1.0 - (2.0 * f - 1.0).abs() * 0.55));
        b.bar(top, bottom, 1.0, A);
    }
    b.bar(v3(cx - w * 0.2, y0, tip.z + reach * 0.2), v3(cx + w * 0.2, y0, tip.z + reach * 0.2), 1.0, D);
    b.bx(tip + Vec3::Y * 0.5, Vec3::ONE, C);
}

/// Rotor / propeller / fan: `blades` flat blades of radius `r` ⟂ `axis`, spinning at `rate` rad/s.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rotor(b: &mut B, parent: PartId, hub: Vec3, axis: Vec3, r: f32, blades: usize, rate: f32, t: Tint) -> PartId {
    let ax = axis.normalize_or(Vec3::Y);
    let id = b.mpart(parent, hub, PartRole::Other, MotionSpec::spin(ax, rate));
    let n = blades.clamp(2, 8);
    let bw = (r * 0.16).clamp(1.2, 3.0);
    for i in 0..n {
        let d = around(ax, i as f32 / n as f32 * TAU);
        b.flat(PrimKind::Box, hub + d * 0.8, hub + d * r, bw, 1.0, ax, t);
        b.flat(PrimKind::Box, hub + d * (r - 1.2), hub + d * r, bw, 1.0, ax, A);
    }
    b.cyl(hub, ax, 2.6, 2.0, A);
    b.cyl(hub + ax * 1.0, ax, 1.0, 1.0, C);
    b.on(parent);
    id
}

/// Jet engine from `front` (intake) to `back` (nozzle): nacelle, spinning fan, flickering flame.
pub(crate) fn jet(b: &mut B, parent: PartId, front: Vec3, back: Vec3, r: f32) {
    let d = (back - front).normalize_or(Vec3::Z);
    let r = r.max(1.5);
    b.on(parent);
    b.seg(PrimKind::Cylinder, front, back, 2.0 * r + 1.0, 2.0 * r + 1.0, P);
    b.seg(PrimKind::Cylinder, front - d * 0.5, front + d * 1.0, 2.0 * r + 2.0, 2.0 * r + 2.0, A);
    b.cone_to(back + d * 1.5, back - d * (r * 0.8), 2.0 * r + 0.5, D);
    b.cyl(front - d * 0.6, d, (2.0 * r - 1.0).max(1.0), 1.0, D);
    rotor(b, parent, front - d * 0.8, -d, r - 0.4, 6, 22.0, Tint::Primary);
    let flame = b.part(parent, back + d * 1.5, PartRole::Exhaust);
    b.cone_to(back + d * 1.0, back + d * (r * 2.5 + 2.0), 2.0 * r - 0.5, C);
    let _ = flame;
    b.on(parent);
}

/// Exhaust pipe at `at` pointing `dir` with a flickering flame and puffs.
pub(crate) fn exhaust_pipe(b: &mut B, parent: PartId, at: Vec3, dir: Vec3, r: f32, flame: bool) {
    let d = dir.normalize_or(Vec3::Z);
    let r = r.max(1.0);
    b.on(parent);
    b.seg(PrimKind::Cylinder, at - d * (r * 2.0), at, 2.0 * r, 2.0 * r, D);
    b.seg(PrimKind::Cylinder, at - d * 0.5, at + d * 0.5, 2.0 * r + 1.0, 2.0 * r + 1.0, A);
    if flame {
        b.part(parent, at, PartRole::Exhaust);
        b.cone_to(at + d * 0.5, at + d * (r * 2.5 + 1.5), 2.0 * r - 0.2, C);
        b.on(parent);
    }
    b.puff(PuffSpec { part: parent, at: at + d * 1.0, dir: (d + Vec3::Y * 0.4).normalize(), size: (r * 1.6).max(1.2), per_rev: 0.0, wheel: None, idle_hz: 2.2, per_m: 0.35 });
}

/// Turret (aim yaw) at `pos` with a pitching gun mount (aim pitch). Returns (turret, mount).
#[allow(clippy::too_many_arguments)]
pub(crate) fn turret(b: &mut B, parent: PartId, pos: Vec3, w: f32, l: f32, h: f32, barrel_len: f32, barrel_d: f32, twin: bool) -> (PartId, PartId) {
    let yaw = b.mpart(parent, pos, PartRole::Other, MotionSpec::Aim { pitch: false });
    let c = pos + Vec3::Y * (h * 0.5);
    b.cy(pos + Vec3::Y * 0.5, w * 0.95, 1.0, D);
    b.el(c, v3(w, h, l), A);
    b.bx(c + v3(0.0, -h * 0.1, 0.0), v3(w * 0.9, h * 0.55, l * 0.85), A);
    // Hatch + periscope.
    b.cy(c + v3(w * 0.15, h * 0.5, l * 0.15), (w * 0.3).max(1.5), 1.0, D);
    b.bx(c + v3(-w * 0.2, h * 0.5, -l * 0.2), v3(1.0, 1.0, 1.0), C);
    b.rivets(c + v3(-w * 0.45, 0.0, -l * 0.3), c + v3(-w * 0.45, 0.0, l * 0.3), 2.0, D);
    b.rivets(c + v3(w * 0.45, 0.0, -l * 0.3), c + v3(w * 0.45, 0.0, l * 0.3), 2.0, D);
    let trunnion = c + v3(0.0, 0.0, -l * 0.42);
    let pitch = b.mpart(yaw, trunnion, PartRole::Other, MotionSpec::Aim { pitch: true });
    b.bx(trunnion, v3((w * 0.5).max(2.0), (h * 0.55).max(2.0), 2.0), D);
    let xs: &[f32] = if twin { &[-1.0, 1.0] } else { &[0.0] };
    for &s in xs {
        let o = trunnion + v3(s * (barrel_d + 0.5), 0.0, 0.0);
        barrel(b, o, Vec3::NEG_Z, barrel_len, barrel_d);
    }
    b.on(parent);
    (yaw, pitch)
}

/// A gun barrel from `base` along `dir`: tube, reinforcing ring, muzzle brake, glowing bore.
pub(crate) fn barrel(b: &mut B, base: Vec3, dir: Vec3, len: f32, d: f32) {
    let dd = dir.normalize_or(Vec3::NEG_Z);
    let d = d.max(1.0);
    let tip = base + dd * len;
    b.seg(PrimKind::Cylinder, base, tip, d, d, P);
    b.seg(PrimKind::Cylinder, base.lerp(tip, 0.3), base.lerp(tip, 0.3) + dd * 1.0, d + 1.0, d + 1.0, D);
    b.seg(PrimKind::Cylinder, tip - dd * 1.5, tip, d + 1.0, d + 1.0, A);
    b.cyl(tip + dd * 0.2, dd, (d - 1.0).max(1.0), 1.0, C);
}

/// Spinning drill from `base` along `dir` (length `len`, base radius `r`) with spiral flutes.
pub(crate) fn drill(b: &mut B, parent: PartId, base: Vec3, dir: Vec3, len: f32, r: f32) -> PartId {
    let d = dir.normalize_or(Vec3::NEG_Z);
    let r = r.max(1.5);
    let id = b.mpart(parent, base, PartRole::Other, MotionSpec::spin(d, 9.0));
    let tip = base + d * len;
    b.cone_to(base, tip, 2.0 * r, P);
    b.cyl(base + d * 0.5, d, 2.0 * r + 1.0, 1.5, D);
    // Spiral flutes: stepped Accent ridges twisting toward the tip.
    let steps = ((len / 1.0).round() as usize).clamp(4, 40);
    for k in 0..steps {
        let f = (k as f32 + 0.5) / steps as f32;
        let rr = r * (1.0 - f) + 0.4;
        for s in [0.0f32, PI] {
            let p = base + d * (len * f) + around(d, f * TAU * 1.6 + s) * rr;
            b.bx(p, Vec3::ONE, A);
        }
    }
    b.cone_to(tip - d * (len * 0.18).max(1.0), tip + d * 0.5, (r * 0.7).max(1.0), C);
    b.on(parent);
    id
}

/// Circular saw ⟂ `axis` at `c` (radius `r`): disc, glowing teeth, hub. Spins fast.
pub(crate) fn saw(b: &mut B, parent: PartId, c: Vec3, axis: Vec3, r: f32) -> PartId {
    let a = axis.normalize_or(Vec3::X);
    let r = r.max(2.0);
    let id = b.mpart(parent, c, PartRole::Other, MotionSpec::spin(a, -16.0));
    b.cyl(c, a, 2.0 * r - 1.0, 1.0, P);
    ring(b, c, a, r * 0.55, 1.0, 1.0, A);
    let n = ((TAU * r / 1.6).round() as usize).clamp(8, 32);
    for i in 0..n {
        let d = around(a, i as f32 / n as f32 * TAU);
        let d2 = around(a, (i as f32 + 0.6) / n as f32 * TAU);
        b.flat(PrimKind::Box, c + d * (r - 0.6), c + d2 * (r + 0.8), 1.0, 1.0, a, C);
    }
    b.cyl(c, a, 2.5, 2.0, D);
    b.cyl(c, a, 1.0, 3.0, C);
    b.on(parent);
    id
}

/// Square sail hanging from its yard at `top` (width `w`, height `h`), bellied forward (−Z).
pub(crate) fn sail(b: &mut B, parent: PartId, top: Vec3, w: f32, h: f32, phase: f32) -> PartId {
    let id = b.mpart(parent, top, PartRole::Cloth, MotionSpec::Sway { axis: Vec3::X, amp: 0.035, hz: 0.33, phase, speed_amp: 0.05, lean: 0.14 });
    let n = ((h / 1.5).round() as usize).clamp(2, 24);
    let belly = (h * 0.16).max(1.0);
    for i in 0..n {
        let f = (i as f32 + 0.5) / n as f32;
        let y = top.y - f * h;
        let z = top.z - belly * (PI * f).sin();
        let ww = w * (1.0 - 0.12 * f);
        b.bx(v3(top.x, y, z), v3(ww, h / n as f32 + 0.5, 1.0), if i == n - 1 { A } else { P });
    }
    // Reef band + lantern emblem.
    b.bx(v3(top.x, top.y - h * 0.3, top.z - belly * (PI * 0.3).sin()), v3(w * 0.96, 1.0, 1.0), A);
    let ec = v3(top.x, top.y - h * 0.55, top.z - belly - 0.4);
    b.bx(ec, v3((w * 0.18).max(1.0), (h * 0.18).max(1.0), 1.0), C);
    b.on(parent);
    id
}

/// Flag streaming back (+Z) from a pole top.
pub(crate) fn flag(b: &mut B, parent: PartId, at: Vec3, w: f32, h: f32) -> PartId {
    let id = b.mpart(parent, at, PartRole::Cloth, MotionSpec::Sway { axis: Vec3::Y, amp: 0.2, hz: 1.1, phase: 0.7, speed_amp: 0.15, lean: 0.0 });
    let n = ((w / 1.5).round() as usize).clamp(2, 12);
    for i in 0..n {
        let f = (i as f32 + 0.5) / n as f32;
        let wave = (f * PI * 2.0).sin() * 0.6;
        b.bx(at + v3(wave, -h * 0.5, f * w), v3(1.0, h * (1.0 - 0.3 * f), w / n as f32 + 0.4), if i % 2 == 0 { A } else { P });
    }
    b.bx(at + v3(0.0, -h * 0.5, w * 0.4), v3(1.2, (h * 0.35).max(1.0), (w * 0.2).max(1.0)), C);
    b.on(parent);
    id
}

/// A mast from `base` (height `h`) with `yards` square sails of width `w` (0 = bare pole), a
/// crow's nest and a flag.
pub(crate) fn mast(b: &mut B, parent: PartId, base: Vec3, h: f32, w: f32, yards: usize, phase: f32) {
    b.on(parent);
    let d = (h * 0.06).clamp(1.0, 3.0);
    b.cy(base + Vec3::Y * (h * 0.5), d, h, D);
    let top = base + Vec3::Y * h;
    let n = yards.min(3);
    let seg = h * 0.82 / n.max(1) as f32;
    for i in 0..n {
        let yy = top.y - h * 0.08 - i as f32 * seg;
        let ww = w * (1.0 - 0.18 * (n - 1 - i) as f32 / n as f32);
        b.on(parent);
        b.bar(v3(base.x - ww * 0.55, yy, base.z), v3(base.x + ww * 0.55, yy, base.z), 1.0, A);
        sail(b, parent, v3(base.x, yy - 0.6, base.z - 1.0), ww, seg * 0.86, phase + i as f32 * 0.9);
    }
    b.on(parent);
    // Crow's nest.
    let cn = top - Vec3::Y * (h * 0.12);
    b.cy(cn, d + 3.0, 1.0, D);
    ring(b, cn + Vec3::Y * 1.0, Vec3::Y, (d + 2.0) * 0.5, 1.0, 1.0, A);
    flag(b, parent, top + Vec3::Y * 0.5, (h * 0.18).clamp(2.0, 10.0), (h * 0.1).clamp(1.5, 6.0));
    b.bx(top + Vec3::Y * 0.5, Vec3::ONE, C);
    b.on(parent);
}

/// Whip antenna from `base` (height `h`) with a glowing tip; sways, streams back with speed.
pub(crate) fn antenna(b: &mut B, parent: PartId, base: Vec3, h: f32) -> PartId {
    b.on(parent);
    b.cy(base + Vec3::Y * 0.5, 2.0, 1.0, D);
    let id = b.mpart(parent, base, PartRole::Other, MotionSpec::Sway { axis: Vec3::X, amp: 0.05, hz: 1.7, phase: 0.3, speed_amp: 0.05, lean: 0.35 });
    b.bar(base, base + Vec3::Y * h, 1.0, A);
    b.bx(base + Vec3::Y * (h + 0.5), v3(1.0, 1.0, 1.0), C);
    b.bx(base + Vec3::Y * (h * 0.55), v3(1.0, 1.0, 1.0), C);
    b.on(parent);
    id
}

/// Radar dish on a post at `base`, sweeping slowly.
pub(crate) fn dish(b: &mut B, parent: PartId, base: Vec3, r: f32) -> PartId {
    let r = r.max(2.0);
    b.on(parent);
    b.cy(base + Vec3::Y * 1.0, 1.5, 2.0, D);
    let id = b.mpart(parent, base + Vec3::Y * 2.0, PartRole::Other, MotionSpec::spin(Vec3::Y, 0.9));
    let c = base + Vec3::Y * (2.0 + r * 0.7);
    let face = v3(0.0, 0.45, -1.0).normalize();
    b.bar(base + Vec3::Y * 2.0, c, 1.0, D);
    b.q(PrimKind::Ellipsoid, c, v3(2.0 * r, 2.0 * r, 1.5), Quat::from_rotation_arc(Vec3::Z, face), P);
    ring(b, c + face * 0.4, face, r - 0.3, 1.0, 1.0, A);
    b.bar(c, c + face * (r * 0.8), 1.0, D);
    b.bx(c + face * (r * 0.8 + 0.5), Vec3::ONE, C);
    b.on(parent);
    id
}

/// Crane: turntable at `base`, lattice boom luffing slowly, hook on a swaying chain.
pub(crate) fn crane(b: &mut B, parent: PartId, base: Vec3, len: f32) {
    let len = len.max(6.0);
    b.on(parent);
    b.cy(base + Vec3::Y * 0.5, (len * 0.25).max(3.0), 1.0, D);
    b.bx(base + Vec3::Y * 2.0, v3((len * 0.18).max(3.0), 3.0, (len * 0.22).max(3.0)), A);
    b.bx(base + v3(0.0, 2.5, -(len * 0.11).max(1.5) - 0.5), v3(2.0, 2.0, 1.0), C);
    let pivot = base + Vec3::Y * 3.5;
    let boom = b.mpart(parent, pivot, PartRole::Other, MotionSpec::Sway { axis: Vec3::X, amp: 0.07, hz: 0.18, phase: 0.0, speed_amp: 0.0, lean: 0.0 });
    let dir = v3(0.0, 0.62, -0.78).normalize();
    let tip = pivot + dir * len;
    let side = Vec3::X * 1.0;
    b.bar(pivot - side, tip - side * 0.4, 1.0, P);
    b.bar(pivot + side, tip + side * 0.4, 1.0, P);
    let n = ((len / 2.5).round() as usize).clamp(2, 16);
    for i in 0..n {
        let a = pivot.lerp(tip, i as f32 / n as f32);
        let c = pivot.lerp(tip, (i as f32 + 1.0) / n as f32);
        let s = if i % 2 == 0 { side } else { -side };
        b.bar(a + s * 0.8, c - s * 0.8, 1.0, A);
    }
    b.bx(tip, v3(3.0, 2.0, 2.0), D);
    b.on(boom);
    chain(b, boom, tip - Vec3::Y * 1.0, 3, 1.6, true);
    b.on(parent);
}

/// Hanging chain of `n` links from `top` (each its own swaying part); `hook` ends it with a hook.
pub(crate) fn chain(b: &mut B, parent: PartId, top: Vec3, n: usize, size: f32, hook: bool) -> PartId {
    let mut p = top;
    let mut par = parent;
    let mut first = parent;
    for i in 0..n.clamp(1, 6) {
        let id = b.mpart(par, p, PartRole::Cloth, MotionSpec::Sway { axis: Vec3::X, amp: 0.1, hz: 0.7, phase: i as f32 * 0.8, speed_amp: 0.05, lean: 0.25 / (1.0 + i as f32) });
        if i == 0 {
            first = id;
        }
        crate::attach::link(b, p - Vec3::Y * size * 0.5, size, i % 2 == 1, if i % 2 == 0 { A } else { D });
        par = id;
        p -= Vec3::Y * (size * 1.0);
    }
    if hook {
        b.bar(p, p - Vec3::Y * 1.5, 1.0, D);
        b.bar(p - Vec3::Y * 1.5, p - Vec3::Y * 1.5 + v3(0.0, 0.0, -1.5), 1.0, D);
        b.bar(p + v3(0.0, -1.5, -1.5), p + v3(0.0, -0.3, -1.8), 1.0, C);
    }
    b.on(parent);
    first
}

/// One tank tread along X = `x` from `z0` (front) to `z1` (back), height `h`, width `w`: band, road
/// wheels and sprockets (rolling), and top/bottom cleat rows that scroll.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tread(b: &mut B, parent: PartId, x: f32, z0: f32, z1: f32, h: f32, w: f32, outer: f32) {
    let h = h.max(3.0);
    let r = h * 0.5;
    let cy = r + 0.5;
    let (za, zb) = (z0 + r, z1 - r);
    b.on(parent);
    // Band: stadium profile.
    b.bx(v3(x, cy, (za + zb) * 0.5), v3(w, h, (zb - za).max(1.0)), D);
    b.cx(v3(x, cy, za), h, w, D);
    b.cx(v3(x, cy, zb), h, w, D);
    // Side plate (inner guide) in Primary so the rolling wheels read against it.
    b.bx(v3(x + outer * (w * 0.5), cy, (za + zb) * 0.5), v3(1.0, (h - 3.0).max(1.0), (zb - za).max(1.0)), P);
    // Road wheels + sprockets on the outer face.
    let wx = x + outer * (w * 0.5 + 0.5);
    let sd = (h - 1.6).max(2.0);
    wheel(b, parent, v3(wx, cy, za), sd, 1.0, WheelStyle::Sprocket, 0.0);
    wheel(b, parent, v3(wx, cy, zb), sd, 1.0, WheelStyle::Sprocket, 0.3);
    let span = zb - za;
    let rd = (h - 2.0).max(2.0);
    let n = ((span / (rd + 0.8)).floor() as usize).clamp(1, 6);
    for i in 0..n {
        let z = za + span * (i as f32 + 1.0) / (n as f32 + 1.0);
        wheel(b, parent, v3(wx, cy - 0.5, z), rd, 1.0, WheelStyle::Road, i as f32 * 1.3);
    }
    // Cleats (scroll): top run moves forward relative to the hull, bottom run back.
    let period = 2.0;
    for (y, dir) in [(cy + r + 0.3, Vec3::NEG_Z), (cy - r - 0.3, Vec3::Z)] {
        let id = b.mpart(parent, v3(x, y, (za + zb) * 0.5), PartRole::Other, MotionSpec::Scroll { dir, period });
        let mut z = za - 0.5;
        while z <= zb + 0.5 {
            b.bx(v3(x, y, z), v3(w + 0.6, 1.0, 1.0), A);
            z += period;
        }
        b.on(parent);
        let _ = id;
    }
    b.on(parent);
}

/// Pipe run through `pts` (diameter `d`): elbows, a valve wheel and a glowing pressure gauge.
pub(crate) fn pipe_run(b: &mut B, pts: &[Vec3], d: f32) {
    if pts.len() < 2 {
        return;
    }
    for w in pts.windows(2) {
        b.seg(PrimKind::Cylinder, w[0], w[1], d, d, A);
    }
    for p in &pts[1..pts.len() - 1] {
        b.el(*p, Vec3::splat(d + 1.0), D);
    }
    let k = pts.len() / 2;
    let (a, c) = (pts[k - 1], pts[k]);
    let mid = a.lerp(c, 0.5);
    let dir = (c - a).normalize_or(Vec3::Y);
    let n = dir.cross(Vec3::Y).normalize_or(Vec3::X);
    b.bar(mid, mid + n * 1.5, 1.0, D);
    ring(b, mid + n * 1.5, n, 1.5, 1.0, 1.0, D);
    b.cyl(mid - n * (d * 0.5 + 0.5), n, 2.0, 1.0, C);
}

/// Reciprocating piston: static cylinder from `base` along `dir` (stroke `len`), sliding rod part.
#[allow(clippy::too_many_arguments)]
pub(crate) fn piston(b: &mut B, parent: PartId, base: Vec3, dir: Vec3, len: f32, d: f32, per_m: f32, hz: f32, phase: f32) -> PartId {
    let dd = dir.normalize_or(Vec3::Y);
    let d = d.max(1.0);
    b.on(parent);
    let cyl_end = base + dd * (len * 0.55);
    b.seg(PrimKind::Cylinder, base, cyl_end, d + 1.0, d + 1.0, D);
    b.seg(PrimKind::Cylinder, cyl_end - dd * 0.5, cyl_end + dd * 0.5, d + 2.0, d + 2.0, A);
    b.cyl(base, dd, d + 2.0, 1.0, A);
    let id = b.mpart(parent, cyl_end, PartRole::Other, MotionSpec::Slide { axis: dd, amp: (len * 0.18).max(0.6), hz, per_m, phase });
    b.seg(PrimKind::Cylinder, cyl_end - dd * (len * 0.2), base + dd * len, (d - 1.0).max(1.0), (d - 1.0).max(1.0), C);
    b.el(base + dd * len, Vec3::splat(d + 0.5), A);
    b.on(parent);
    id
}

/// Riveted armour plate centred at `c` of `size`, rivets on the face pointing `normal`.
pub(crate) fn plate(b: &mut B, c: Vec3, size: Vec3, normal: Vec3, t: Tint) {
    b.bx(c, size, t);
    let n = normal.normalize_or(Vec3::X);
    let off = n * ((size * n.abs()).max_element() * 0.5 + 0.3);
    let (u, v) = perp_basis(n);
    let hu = (size * u.abs()).max_element() * 0.5 - 0.8;
    let hv = (size * v.abs()).max_element() * 0.5 - 0.8;
    if hu > 0.5 && hv > 0.5 {
        for (su, sv) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            b.bx(c + off + u * (su * hu) + v * (sv * hv), Vec3::ONE, D);
        }
    }
}

/// Crew cab: box, overhanging roof, glowing windows on the sides and the face toward `-Z`.
pub(crate) fn cab(b: &mut B, c: Vec3, w: f32, h: f32, l: f32) {
    b.bx(c, v3(w, h, l), P);
    b.bx(c + Vec3::Y * (h * 0.5 + 0.5), v3(w + 2.0, 1.0, l + 2.0), D);
    b.bx(c + Vec3::Y * (h * 0.5 + 1.0), v3(w - 1.0, 1.0, l), A);
    let wy = c.y + h * 0.15;
    let ww = (l * 0.35).max(1.0);
    let wh = (h * 0.32).max(1.0);
    b.sym(|b, s| {
        b.bx(v3(c.x + s * (w * 0.5), wy, c.z - l * 0.2), v3(1.2, wh, ww), C);
        b.bx(v3(c.x + s * (w * 0.5), wy, c.z + l * 0.25), v3(1.2, wh, ww * 0.7), C);
        b.bx(v3(c.x + s * w * 0.25, wy, c.z - l * 0.5), v3((w * 0.3).max(1.0), wh, 1.2), C);
    });
    b.rivets(v3(c.x - w * 0.5 - 0.3, c.y - h * 0.4, c.z - l * 0.45), v3(c.x - w * 0.5 - 0.3, c.y - h * 0.4, c.z + l * 0.45), 2.0, D);
    b.rivets(v3(c.x + w * 0.5 + 0.3, c.y - h * 0.4, c.z - l * 0.45), v3(c.x + w * 0.5 + 0.3, c.y - h * 0.4, c.z + l * 0.45), 2.0, D);
}

/// Glass canopy (glowing) with frame ribs, centred at `c`.
pub(crate) fn canopy(b: &mut B, c: Vec3, w: f32, h: f32, l: f32) {
    b.el(c, v3(w, h * 2.0, l), C);
    b.bx(c - Vec3::Y * (h * 0.5), v3(w + 1.0, 1.0, l + 1.0), D);
    b.bar(c + v3(0.0, h * 0.95, -l * 0.45), c + v3(0.0, h * 0.95, l * 0.45), 1.0, A);
    for z in [-0.2f32, 0.2] {
        b.bar(c + v3(-w * 0.5, 0.0, z * l), c + v3(w * 0.5, 0.0, z * l) + Vec3::Y * 0.01, 1.0, A);
        b.bar(c + v3(-w * 0.45, 0.0, z * l), c + v3(0.0, h * 0.95, z * l), 1.0, A);
        b.bar(c + v3(w * 0.45, 0.0, z * l), c + v3(0.0, h * 0.95, z * l), 1.0, A);
    }
}

/// Row of glowing windows on the side face x = `x` from `z0` to `z1` at height `y`.
pub(crate) fn window_row(b: &mut B, x: f32, y: f32, z0: f32, z1: f32, size: f32) {
    let step = size * 1.8 + 1.0;
    let n = (((z1 - z0).abs() / step).floor() as usize).clamp(1, 16);
    for i in 0..n {
        let z = z0 + (z1 - z0) * (i as f32 + 0.5) / n as f32;
        b.bx(v3(x, y, z), v3(1.2, size, size), C);
        b.bx(v3(x, y - size * 0.5 - 0.5, z), v3(1.4, 1.0, size + 1.0), D);
    }
}

/// Tail fin from `root` to `tip` (flat, in the plane containing both and `normal`).
pub(crate) fn fin(b: &mut B, root: Vec3, tip: Vec3, chord: f32, normal: Vec3) {
    b.flat(PrimKind::Cone, root, tip, chord, 1.0, normal, A);
    b.flat(PrimKind::Box, root, root.lerp(tip, 0.4), 1.0, 1.2, normal, D);
}

/// Crank-slider assembly on one side of a locomotive (all parts parented to `parent`).
/// `wheels` = the coupled drivers (the middle one is the main driver), `x` = plane of the coupling
/// rod (connecting rod / crosshead one voxel further out), `boss` = crank angle, `rc` = crank radius,
/// `cross_z` = crosshead rest z. Builds the cylinder + slide bars (static) and returns nothing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn crank_side(b: &mut B, parent: PartId, wheels: &[(PartId, Vec3)], main: usize, x: f32, out: f32, boss: f32, rc: f32, cross_z: f32, cyl_d: f32) {
    if wheels.is_empty() {
        return;
    }
    let pin_of = |c: Vec3| v3(x, c.y, c.z) + around(Vec3::X, boss) * rc;
    let (mid, mc) = wheels[main.min(wheels.len() - 1)];
    let pin = pin_of(mc);
    // Coupling rod through every pin.
    if wheels.len() > 1 {
        b.mpart(parent, pin, PartRole::Other, MotionSpec::Crank { wheel: mid, member: CrankMember::Coupling, pin, cross: v3(x, mc.y, cross_z) });
        let first = pin_of(wheels[0].1);
        let last = pin_of(wheels[wheels.len() - 1].1);
        b.bar(first, last, 1.0, C);
        for (_, c) in wheels {
            let p = pin_of(*c);
            b.bx(p + Vec3::X * (out * 0.5), v3(2.0, 2.0, 2.0), A);
            b.bx(p + Vec3::X * out, Vec3::ONE, C);
        }
        b.on(parent);
    }
    // Connecting rod (outboard) from the main pin to the crosshead.
    let xo = x + out;
    let pin_o = v3(xo, pin.y, pin.z);
    let cross = v3(xo, mc.y, cross_z);
    b.mpart(parent, pin_o, PartRole::Other, MotionSpec::Crank { wheel: mid, member: CrankMember::Rod, pin: pin_o, cross });
    b.bar(pin_o, cross, 1.0, C);
    b.bx(pin_o, v3(1.0, 2.0, 2.0), A);
    b.bx(cross, v3(1.0, 2.0, 1.0), A);
    b.bx(pin_o + Vec3::X * out, Vec3::ONE, C);
    b.on(parent);
    // Crosshead + piston rod (slides), cylinder + slide bars (static).
    let fwd = if cross_z < mc.z { Vec3::NEG_Z } else { Vec3::Z };
    let stroke = rc * 2.0;
    b.mpart(parent, cross, PartRole::Other, MotionSpec::Crank { wheel: mid, member: CrankMember::Piston, pin: pin_o, cross });
    b.bx(cross + Vec3::X * out * 0.5, v3(2.0, 2.0, 2.0), P);
    b.bar(cross, cross + fwd * (stroke + 3.0), 1.0, C);
    b.on(parent);
    let cyl_back = cross + fwd * (rc + 1.5);
    let cyl_len = stroke + 3.0;
    let cyl_c = cyl_back + fwd * (cyl_len * 0.5);
    b.cyl(cyl_c, fwd, cyl_d, cyl_len, P);
    b.cyl(cyl_back + fwd * 0.5, fwd, cyl_d + 1.0, 1.0, A);
    b.cyl(cyl_back + fwd * (cyl_len - 0.5), fwd, cyl_d + 1.0, 1.0, A);
    b.bolts(cyl_back + fwd * (cyl_len + 0.1), fwd, cyl_d * 0.4, 6, D);
    for s in [-1.0f32, 1.0] {
        let yo = Vec3::Y * (s * 2.0);
        b.bar(cross - fwd * (rc + 0.5) + yo, cyl_back + yo, 1.0, D);
    }
}
