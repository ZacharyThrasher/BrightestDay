//! Tracking primitives (immediate, this-frame-only hard light on the particle pool — no new
//! pipeline, no allocation): a **lantern beacon** (a faint, tall light pillar that stays a couple
//! of pixels wide at any range), **x-ray strokes** (a polyline re-projected along each point's view
//! ray to just in front of the camera: identical on screen, drawn over walls — the reveal pulse),
//! and **tapered strokes** (ribbons / filaments / ropes as chains of thin segments).
//!
//! Used by the game's tracking presentation (`bd_game::present::tracking`); the charge aura's far
//! pillar is the same look and can call [`beacon_pillar`].

use bevy::prelude::*;

use crate::particles::{FxParticles, PFlags};

/// A faint vertical light pillar from `feet` up `height` metres, seen from `eye`. `strength` 0..1
/// fades it (thinner + dimmer); the width is scaled with distance so it stays ~`px_k · distance`
/// wide (a few pixels) far away. Drawn in `pieces` stacked segments that thin toward the top.
pub fn beacon_pillar(
    fx: &mut FxParticles,
    eye: Vec3,
    feet: Vec3,
    height: f32,
    color: [f32; 3],
    emissive: f32,
    strength: f32,
    px_k: f32,
    pieces: u32,
) {
    let k = if strength.is_finite() { strength.clamp(0.0, 1.0) } else { 0.0 };
    if k <= 0.002 || !feet.is_finite() || !eye.is_finite() || height <= 0.0 {
        return;
    }
    let d = eye.distance(feet).max(1.0);
    let base = (d * px_k).max(0.02);
    let n = pieces.clamp(1, 8);
    for i in 0..n {
        let (a, b) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        let taper = 1.0 - 0.6 * a;
        let p0 = feet + Vec3::Y * (height * a);
        let p1 = feet + Vec3::Y * (height * b);
        // Opaque hard light: fade by thinning (dimming would only darken it).
        fx.immediate_segment(p0, p1, base * taper * k, color, emissive * (0.6 + 0.4 * k) * (1.0 - 0.4 * a), PFlags::PLAIN);
    }
}

/// Re-project `p` along its view ray from `eye` to at most `near` metres away. Returns the new
/// point and the scale factor (multiply sizes by it to keep the same on-screen size).
#[inline]
pub fn xray_point(eye: Vec3, p: Vec3, near: f32) -> (Vec3, f32) {
    let d = p - eye;
    let len = d.length();
    if len <= near || len < 1e-4 {
        return (p, 1.0);
    }
    let k = near / len;
    (eye + d * k, k)
}

/// A polyline drawn "through walls": every point is pulled along its view ray to `near` metres
/// from `eye` (same screen position, in front of any wall further than that); `thickness` is the
/// world thickness at the real distance. `closed` joins the last point to the first.
pub fn xray_polyline(
    fx: &mut FxParticles,
    eye: Vec3,
    pts: &[Vec3],
    closed: bool,
    thickness: f32,
    color: [f32; 3],
    emissive: f32,
    near: f32,
) {
    let n = pts.len();
    if n < 2 || emissive <= 0.0 || thickness <= 0.0 {
        return;
    }
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs {
        let (a, ka) = xray_point(eye, pts[i], near);
        let (b, kb) = xray_point(eye, pts[(i + 1) % n], near);
        fx.immediate_segment(a, b, thickness * 0.5 * (ka + kb), color, emissive, PFlags::PLAIN);
    }
}

/// A circle of `radius` around `center` facing the viewer (a ring in the screen plane), as points.
pub fn view_ring(eye: Vec3, center: Vec3, radius: f32, out: &mut [Vec3]) {
    let n = out.len();
    if n == 0 {
        return;
    }
    let view = (center - eye).normalize_or(Vec3::NEG_Z);
    let (u, v) = view.any_orthonormal_pair();
    for (i, p) in out.iter_mut().enumerate() {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        *p = center + (u * c + v * s) * radius;
    }
}

/// A tapered stroke along `pts`: thickness and emissive interpolate from the first point's
/// (`t0`, `e0`) to the last's (`t1`, `e1`); the colour from `c0` to `c1`. Skips segments longer
/// than `max_seg` (teleports) and returns the number of segments drawn.
#[allow(clippy::too_many_arguments)]
pub fn tapered_stroke(
    fx: &mut FxParticles,
    pts: &[Vec3],
    t0: f32,
    t1: f32,
    c0: [f32; 3],
    c1: [f32; 3],
    e0: f32,
    e1: f32,
    max_seg: f32,
) -> usize {
    let n = pts.len();
    if n < 2 {
        return 0;
    }
    let mut drawn = 0;
    for i in 0..n - 1 {
        let (a, b) = (pts[i], pts[i + 1]);
        if !a.is_finite() || !b.is_finite() || a.distance_squared(b) > max_seg * max_seg {
            continue;
        }
        let f = (i as f32 + 0.5) / (n - 1) as f32;
        let th = t0 + (t1 - t0) * f;
        let em = e0 + (e1 - e0) * f;
        if th <= 1e-5 || em <= 0.0 {
            continue;
        }
        let c = [c0[0] + (c1[0] - c0[0]) * f, c0[1] + (c1[1] - c0[1]) * f, c0[2] + (c1[2] - c0[2]) * f];
        if fx.immediate_segment(a, b, th, c, em, PFlags::PLAIN) {
            drawn += 1;
        }
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xray_keeps_the_screen_position() {
        let eye = Vec3::new(1.0, 2.0, 3.0);
        let p = Vec3::new(10.0, 4.0, -30.0);
        let (q, k) = xray_point(eye, p, 1.5);
        assert!(((q - eye).length() - 1.5).abs() < 1e-4);
        assert!(((q - eye).normalize() - (p - eye).normalize()).length() < 1e-5);
        assert!((k - 1.5 / (p - eye).length()).abs() < 1e-6);
        assert_eq!(xray_point(eye, eye + Vec3::X, 1.5), (eye + Vec3::X, 1.0));
    }

    #[test]
    fn view_ring_faces_the_viewer() {
        let eye = Vec3::ZERO;
        let c = Vec3::new(0.0, 0.0, -10.0);
        let mut pts = [Vec3::ZERO; 12];
        view_ring(eye, c, 2.0, &mut pts);
        for p in pts {
            assert!(((p - c).length() - 2.0).abs() < 1e-4);
            assert!((p - c).dot(Vec3::Z).abs() < 1e-4);
        }
    }
}
