//! OP VISUALS primitives: the shared shapes every generic-op event is drawn with. Stateless: each is
//! called with its entry's age / time left and draws THIS frame (immediate segments + boxes) or
//! emits a one-shot burst (GPU particles through the movefx [`Pen`], so the near-camera guard holds).
//!
//! * [`marker`] — the lingering TARGET MARKER (tag nail, countdown sigil, lock-on brackets, status
//!   halo, black-flame brand, seal, captured crown): shape by kind, pulse by time left. Drawn on the
//!   target for everyone (it is the counterplay: "I'm marked — dodge at the burst").
//! * [`telegraph`] — the DANGER ZONE of a strike that lands later: a ground ring that fills as it
//!   closes (blast), spatial cut lines that grow (cut), a sky column (bolt), a descending halo (holy),
//!   a swirling pinch (void).
//! * [`tether`] — a travelling line with a head: chain + kunai (hook), rope, energy lead, bungee.
//! * [`strike`] — the landing by style: a clean slash that opens space, a bolt from the sky, a pillar
//!   of light, a void pinch-pop, a body-shaped bloom.
//! * [`aura`] — a state on a body: super state, dodge window, counter gleam, guard cage, deflect arcs,
//!   phase vortex, heal-over-time, lock-on glare.
//! * [`heal_pulse`] — the resource pulse: rising crosses + a ring climbing the body.
//! * Extras: [`crescent`] (a flying slash), [`orb`] (a hand-held grinding sphere / crackling hand),
//!   [`arc_preview`] (a leap's landing), [`wheel`] (a roulette), [`portal_oval`], [`impact_star`].

use super::super::movefx::draw::{Look, Pen, basis, circle, mix3, zig};
use super::super::movefx::table::Body;
use bd_core::mechanics::Element;
use bd_fx::{Emit, PFlags, PKind};
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

/// The camera this frame (markers face it; nothing is drawn through the lens).
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub cam: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
}

impl Default for View {
    fn default() -> Self {
        Self { cam: Vec3::ZERO, right: Vec3::X, up: Vec3::Y, fwd: Vec3::NEG_Z }
    }
}

/// A small, brief flash. The engine's flash cube darkens as it grows (a big one ends as a black
/// block), so every op flash goes through here: ≤ 0.6 m, ≤ 0.09 s.
pub fn flash(pen: &mut Pen, pos: Vec3, size: f32, color: [f32; 3], glow: f32, life: f32) {
    pen.flash(pos, size.min(0.6), color, glow, life.min(0.09));
}

/// An immediate line, dropped when it would sit in the lens (both ends ≤ 0.9 m from the camera).
pub fn seg(pen: &mut Pen, v: &View, a: Vec3, b: Vec3, thick: f32, color: [f32; 3], glow: f32) {
    if a.distance_squared(v.cam) < 0.81 && b.distance_squared(v.cam) < 0.81 {
        return;
    }
    if !(a.is_finite() && b.is_finite()) {
        return;
    }
    pen.fx.immediate_segment(a, b, thick, color, glow, PFlags::PLAIN);
}

fn cbox(pen: &mut Pen, v: &View, c: Vec3, rot: Quat, size: Vec3, color: [f32; 3], glow: f32) {
    if c.distance_squared(v.cam) < 0.81 || !c.is_finite() {
        return;
    }
    pen.fx.immediate_box(c, rot, size, color, glow, PFlags::PLAIN);
}

/// A camera-facing ring (markers read the same from every side).
fn face_ring(pen: &mut Pen, v: &View, c: Vec3, r: f32, segs: usize, thick: f32, color: [f32; 3], glow: f32, phase: f32) {
    let n = (c - v.cam).normalize_or(v.fwd);
    circle(pen.fx, c, n, r, segs, thick, color, glow, phase);
}

/// A dashed ring (only every other segment) in the plane ⟂ `normal`.
pub fn dashed_ring(pen: &mut Pen, v: &View, c: Vec3, normal: Vec3, r: f32, segs: usize, thick: f32, color: [f32; 3], glow: f32, phase: f32) {
    let (u, w) = basis(normal);
    let segs = segs.max(6);
    for i in (0..segs).step_by(2) {
        let a0 = phase + i as f32 / segs as f32 * TAU;
        let a1 = phase + (i + 1) as f32 / segs as f32 * TAU;
        seg(pen, v, c + (u * a0.cos() + w * a0.sin()) * r, c + (u * a1.cos() + w * a1.sin()) * r, thick, color, glow);
    }
}

/// An arc of a ring (fraction `frac` of the turn from `phase`).
pub fn arc(pen: &mut Pen, v: &View, c: Vec3, normal: Vec3, r: f32, phase: f32, frac: f32, segs: usize, thick: f32, color: [f32; 3], glow: f32) {
    let (u, w) = basis(normal);
    let n = ((segs as f32 * frac.clamp(0.0, 1.0)).ceil() as usize).max(1);
    let span = frac.clamp(0.0, 1.0) * TAU;
    let mut prev = c + (u * phase.cos() + w * phase.sin()) * r;
    for i in 1..=n {
        let a = phase + span * i as f32 / n as f32;
        let p = c + (u * a.cos() + w * a.sin()) * r;
        seg(pen, v, prev, p, thick, color, glow);
        prev = p;
    }
}

/// The hue wheel (a Super-Star rainbow), linear-ish rgb.
pub fn hue(h: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    }
}

fn h01(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// 0.6..1 pulse whose rate climbs as time runs out (1 Hz → 5 Hz in the last second).
pub fn pulse(t: f32, left: f32) -> f32 {
    let rate = if left < 1.0 { 5.0 } else if left < 2.0 { 2.8 } else { 1.4 };
    0.6 + 0.4 * ((t * rate * TAU).cos() * 0.5 + 0.5)
}

// ───────────────────────────────────────────────────────────────── target markers

/// What a target marker says (its shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// Tag (Resonance nail, Bungee Gum): a nail driven into them + a spinning diamond overhead.
    Tag,
    /// DelayedHit countdown: counter-rotating rings tightening round the chest + a clock arc overhead.
    Delayed,
    /// LockOn: corner brackets closing in as the lock grows (`strength` = 0..1 lock).
    Lock,
    /// A short "being hunted" reticle (a homing lunge / seeker picked you).
    Hunted,
    /// Powers locked (Lockout): a broken, glitching halo.
    Lockout,
    /// Stunned / dazed (GazeStun, a genjutsu): a spiral over the eyes + orbiting sparks.
    Dazed,
    /// Contagion: black flames clinging to the body.
    Brand,
    /// A thrown seal / marker (MarkRecall) riding a player or stuck to a surface.
    Seal,
    /// Captured / possessed construct: a crown over it.
    Captured,
    /// Held in a command grab.
    Held,
}

/// Draw a marker on a body. `feet` / `size` place it (a point marker passes its point and 1).
/// `strength` is the lock fraction for [`MarkKind::Lock`], else 1.
#[allow(clippy::too_many_arguments)]
pub fn marker(pen: &mut Pen, v: &View, look: &Look, kind: MarkKind, feet: Vec3, size: f32, t: f32, left: f32, total: f32, strength: f32, seed: u32) {
    let s = size.max(0.3);
    let chest = feet + Vec3::Y * 1.05 * s;
    // Lines thicken with distance (a 1-px mark at 30 m reads as nothing): ×1 … ×2.4.
    let far = ((chest - v.cam).length() / 12.0).clamp(1.0, 2.4);
    let ts = s * far;
    let head = feet + Vec3::Y * 2.35 * s;
    let p = pulse(t, left);
    let fade_in = (t / 0.12).min(1.0);
    let glow = look.glow * fade_in;
    match kind {
        MarkKind::Tag => {
            // The nail: driven in at an angle, its head glinting out of the body.
            let yaw = h01(seed) * TAU;
            let d = Vec3::new(yaw.cos(), -0.35, yaw.sin()).normalize();
            let tip = chest + Vec3::Y * 0.15 * s;
            let out = tip - d * 0.75 * s;
            seg(pen, v, tip, out, 0.045 * ts, look.dark, 0.6);
            seg(pen, v, out, out - d * 0.08, 0.13 * ts, mix3(look.core, look.main, 0.3), glow * p * 1.4);
            // The diamond overhead, spinning; flickers in the last 1.5 s (it is about to be used / run out).
            let flick = if left < 1.5 && ((t * 12.0) as u32) % 2 == 0 { 0.35 } else { 1.0 };
            let r = 0.26 * s;
            let a = t * 2.4;
            let h = Vec3::new(a.cos(), 0.0, a.sin()) * r;
            let pts = [head + Vec3::Y * r * 1.4, head + h, head - Vec3::Y * r * 1.4, head - h];
            let c = mix3(look.main, look.core, 0.25);
            for k in 0..4 {
                seg(pen, v, pts[k], pts[(k + 1) % 4], 0.035 * ts, c, glow * p * flick * 1.3);
            }
            seg(pen, v, head - Vec3::Y * r * 1.4, chest + Vec3::Y * 0.9 * s, 0.012, look.main, glow * 0.5 * flick);
        }
        MarkKind::Delayed => {
            let k = (left / total.max(0.1)).clamp(0.0, 1.0);
            let r = (0.55 + 0.35 * k) * s;
            circle(pen.fx, chest, Vec3::Y, r, 18, 0.06 * ts, look.main, glow * 1.5 * p, t * 3.0);
            circle(pen.fx, chest + Vec3::Y * 0.35 * s, Vec3::Y, r * 0.7, 14, 0.03 * ts, look.core, glow * p, -t * 4.0);
            circle(pen.fx, chest, Vec3::new(0.0, 0.2, 1.0), r * 0.85, 16, 0.03 * ts, look.main, glow * 0.8 * p, t * 2.0);
            // The clock over the head: an arc that empties as the burst nears (readable from afar).
            let n = (head - v.cam).normalize_or(v.fwd);
            arc(pen, v, head, n, 0.36 * s, PI * 0.5, k, 20, 0.07 * ts, mix3(look.core, look.main, 0.4), glow * 1.4 * p);
            face_ring(pen, v, head, 0.4 * s, 20, 0.018 * ts, look.dark, 0.5, 0.0);
        }
        MarkKind::Lock | MarkKind::Hunted => {
            // Four corner brackets round the body, closing in and squaring up as the lock grows.
            let lock = strength.clamp(0.0, 1.0);
            let c = feet + Vec3::Y * 1.0 * s;
            let half = (1.35 - 0.5 * lock) * s;
            let rot = (1.0 - lock) * 0.8 + if kind == MarkKind::Hunted { t * 2.0 } else { 0.0 };
            let (r, u) = (v.right * rot.cos() + v.up * rot.sin(), v.up * rot.cos() - v.right * rot.sin());
            let arm = half * 0.38;
            let full = lock >= 0.999;
            let col = if full { mix3(look.core, look.main, 0.2) } else { look.main };
            let g = glow * (if full { 1.8 * p } else { 0.8 + 0.8 * lock });
            let th = (0.04 + 0.04 * lock) * ts;
            for (sx, sy) in [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                let corner = c + r * sx * half + u * sy * half;
                seg(pen, v, corner, corner - r * sx * arm, th, col, g);
                seg(pen, v, corner, corner - u * sy * arm, th, col, g);
            }
            if lock > 0.5 || kind == MarkKind::Hunted {
                // The pip: the dead centre of the chest.
                let d = 0.07 * s * (0.6 + lock);
                seg(pen, v, c - r * d, c + r * d, 0.05 * ts, look.core, g * 1.2);
                seg(pen, v, c - u * d, c + u * d, 0.05 * ts, look.core, g * 1.2);
            }
            if full {
                // A closed diamond: they are dead to rights.
                let dd = half * 0.55;
                let q = [c + u * dd, c + r * dd, c - u * dd, c - r * dd];
                for k in 0..4 {
                    seg(pen, v, q[k], q[(k + 1) % 4], 0.03 * ts, look.main, g);
                }
            }
        }
        MarkKind::Lockout => {
            // A broken halo that glitches: dashed, jumping, blinking cubes.
            let jump = ((t * 14.0) as u32).wrapping_mul(2654435761);
            let ph = h01(jump) * 0.6;
            dashed_ring(pen, v, head, Vec3::Y, 0.42 * s, 12, 0.05 * ts, look.main, glow * p, ph);
            for k in 0..4u32 {
                let r = h01(jump ^ (k * 97));
                if r < 0.5 {
                    continue;
                }
                let a = h01(jump.wrapping_add(k * 13)) * TAU;
                let q = head + Vec3::new(a.cos(), (r - 0.75) * 0.8, a.sin()) * 0.5 * s;
                cbox(pen, v, q, Quat::IDENTITY, Vec3::splat(0.09 * s), if k % 2 == 0 { look.core } else { look.main }, glow * 1.4);
            }
            // A slash through it (no powers).
            seg(pen, v, head - v.right * 0.35 * s - Vec3::Y * 0.2 * s, head + v.right * 0.35 * s + Vec3::Y * 0.2 * s, 0.05 * ts, look.main, glow * p);
        }
        MarkKind::Dazed => {
            // A spiral over the eyes, turning; three sparks orbiting the head.
            let eyes = feet + Vec3::Y * 1.62 * s + (v.cam - feet).normalize_or(Vec3::Z).with_y(0.0).normalize_or(Vec3::Z) * 0.35 * s;
            let n = (eyes - v.cam).normalize_or(v.fwd);
            let (u, w) = basis(n);
            let mut prev = eyes;
            for i in 1..=18 {
                let f = i as f32 / 18.0;
                let a = -t * 7.0 + f * 3.0 * TAU;
                let q = eyes + (u * a.cos() + w * a.sin()) * f * 0.28 * s;
                seg(pen, v, prev, q, 0.02 * ts, mix3(look.main, look.core, f), glow * p);
                prev = q;
            }
            for k in 0..3 {
                let a = t * 4.0 + k as f32 * TAU / 3.0;
                let q = head - Vec3::Y * 0.3 * s + Vec3::new(a.cos(), 0.0, a.sin()) * 0.4 * s;
                cbox(pen, v, q, Quat::from_rotation_y(a), Vec3::splat(0.07 * s), look.core, glow * 1.5);
            }
        }
        MarkKind::Brand => {
            // Black flames that cling: dark motes licking up off the body, a smouldering ring at the feet.
            let dark = Look { body: Body::Flame, main: [0.015, 0.005, 0.02], core: mix3([0.02, 0.0, 0.02], look.main, 0.5), glow: 0.5, size: 3.2 * s, ..*look };
            let n = 4;
            for _ in 0..n {
                let a = pen.fx.rng.f() * TAU;
                let h = pen.fx.rng.f() * 1.7 * s;
                let q = feet + Vec3::new(a.cos() * 0.32 * s, h, a.sin() * 0.32 * s);
                let up = Vec3::Y * (1.2 + pen.fx.rng.f());
                pen.mote(&dark, q, up, 0.5, 0.0);
            }
            if pen.fx.rng.f() < 0.35 {
                let q = chest + pen.fx.rng.dir() * 0.3 * s;
                let ember = Look { body: Body::Flame, main: look.main, core: look.core, glow: look.glow * 0.8, size: 1.4, ..*look };
                pen.mote(&ember, q, Vec3::Y * 1.5, 0.35, 0.0);
            }
            circle(pen.fx, feet + Vec3::Y * 0.05, Vec3::Y, 0.55 * s, 16, 0.05 * ts, [0.02, 0.0, 0.02], 0.2, t);
            circle(pen.fx, feet + Vec3::Y * 0.06, Vec3::Y, 0.5 * s, 16, 0.02 * ts, look.main, glow * 0.4 * p, -t);
        }
        MarkKind::Seal => {
            // An eight-point seal (two squares), turning, with a beacon line up so it can be found.
            let n = if size <= 0.35 { Vec3::Y } else { (head - v.cam).normalize_or(v.fwd) };
            let c = if size <= 0.35 { feet + Vec3::Y * 0.08 } else { chest };
            let (u, w) = basis(n);
            let r = if size <= 0.35 { 0.45 } else { 0.5 * s };
            for (k, off) in [0.0f32, PI * 0.25].into_iter().enumerate() {
                let a0 = t * 0.9 * if k == 0 { 1.0 } else { -1.0 } + off;
                let q: [Vec3; 4] = std::array::from_fn(|i| {
                    let a = a0 + i as f32 * PI * 0.5;
                    c + (u * a.cos() + w * a.sin()) * r
                });
                for i in 0..4 {
                    seg(pen, v, q[i], q[(i + 1) % 4], 0.03, if k == 0 { look.main } else { look.core }, glow * p);
                }
            }
            circle(pen.fx, c, n, r * 0.35, 12, 0.025 * ts, look.core, glow * 1.3 * p, 0.0);
            let top = c + Vec3::Y * 3.5;
            for i in (0..10).step_by(2) {
                seg(pen, v, c.lerp(top, i as f32 / 10.0), c.lerp(top, (i + 1) as f32 / 10.0), 0.018, look.main, glow * 0.5 * p);
            }
        }
        MarkKind::Captured => {
            // A crown over the possessed thing (three points), and a soft glow ring round it.
            let c = feet + Vec3::Y * (1.9 * s + 0.5);
            let r = 0.32 * s.max(1.0);
            for k in 0..6 {
                let a0 = t * 1.5 + k as f32 * TAU / 6.0;
                let a1 = t * 1.5 + (k + 1) as f32 * TAU / 6.0;
                let b0 = c + Vec3::new(a0.cos(), 0.0, a0.sin()) * r;
                let b1 = c + Vec3::new(a1.cos(), 0.0, a1.sin()) * r;
                seg(pen, v, b0, b1, 0.05 * ts, look.main, glow * 1.3 * p);
                if k % 2 == 0 {
                    seg(pen, v, b0, b0 + Vec3::Y * 0.3 * s.max(1.0), 0.045 * ts, look.core, glow * 1.6 * p);
                }
            }
            circle(pen.fx, feet + Vec3::Y * 0.1, Vec3::Y, 0.9 * s.max(1.0), 20, 0.03 * ts, look.main, glow * 0.5 * p, -t);
        }
        MarkKind::Held => {
            // A clamp: two brackets squeezing the torso.
            let c = chest;
            let r = 0.5 * s;
            for sgn in [-1.0f32, 1.0] {
                let side = v.right * sgn * r;
                seg(pen, v, c + side + Vec3::Y * 0.35 * s, c + side - Vec3::Y * 0.35 * s, 0.05 * ts, look.main, glow);
                seg(pen, v, c + side + Vec3::Y * 0.35 * s, c + side * 0.6 + Vec3::Y * 0.35 * s, 0.05 * ts, look.main, glow);
                seg(pen, v, c + side - Vec3::Y * 0.35 * s, c + side * 0.6 - Vec3::Y * 0.35 * s, 0.05 * ts, look.main, glow);
            }
        }
    }
}

/// Your own marker, seen from inside your body in first person: a small glyph low in the view
/// (well under the crosshair) so the marked player always knows.
pub fn self_marker(pen: &mut Pen, v: &View, look: &Look, kind: MarkKind, t: f32, left: f32, total: f32, strength: f32) {
    let c = v.cam + v.fwd * 2.4 - v.up * 0.78;
    let p = pulse(t, left);
    let n = -v.fwd;
    match kind {
        MarkKind::Delayed => {
            let k = (left / total.max(0.1)).clamp(0.0, 1.0);
            arc(pen, v, c, n, 0.1, PI * 0.5, k, 20, 0.012, look.main, look.glow * 1.3 * p);
            circle(pen.fx, c, n, 0.13, 20, 0.006, look.core, look.glow * 0.6 * p, 0.0);
        }
        MarkKind::Lock | MarkKind::Hunted => {
            let half = 0.1 + 0.05 * (1.0 - strength);
            let arm = half * 0.4;
            for (sx, sy) in [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                let corner = c + v.right * sx * half + v.up * sy * half;
                seg(pen, v, corner, corner - v.right * sx * arm, 0.01, look.main, look.glow * (0.8 + strength) * p);
                seg(pen, v, corner, corner - v.up * sy * arm, 0.01, look.main, look.glow * (0.8 + strength) * p);
            }
        }
        _ => {
            circle(pen.fx, c, n, 0.08, 4, 0.012, look.main, look.glow * p, t * 2.0);
            circle(pen.fx, c, n, 0.05, 12, 0.008, look.core, look.glow * 0.8 * p, 0.0);
        }
    }
}

// ───────────────────────────────────────────────────────────────── telegraphs + strikes

/// How a strike lands (and how its telegraph reads) — from the style's body + the op's cut flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrikeKind {
    /// A clean cut: spatial slash lines grow, then open space (Judgment Cut).
    Slash,
    /// A bolt from the sky (Urbosa's Fury, Judgment Bolt).
    Bolt,
    /// A pillar of light from above.
    Holy,
    /// A pinch in space that pops.
    Void,
    /// A blast: a filling ground ring, then a body-shaped bloom (the world's explosion does the rest).
    Blast,
}

pub fn strike_kind(look: &Look, cut: bool, element: Element) -> StrikeKind {
    if !cut {
        return StrikeKind::Blast;
    }
    match look.body {
        Body::Bolt | Body::Streak => StrikeKind::Bolt,
        Body::Halo => StrikeKind::Holy,
        Body::Void => StrikeKind::Void,
        _ if element == Element::Electric => StrikeKind::Bolt,
        _ => StrikeKind::Slash,
    }
}

/// The two cut directions of a slash at a spot (fixed per strike: telegraph and landing agree).
pub fn slash_dirs(seed: u32) -> [Vec3; 2] {
    let a = h01(seed) * TAU;
    let tilt = 0.35 + 0.5 * h01(seed ^ 0x51);
    let d1 = Vec3::new(a.cos(), tilt, a.sin()).normalize();
    let b = a + 1.1 + 0.9 * h01(seed ^ 0x77);
    let d2 = Vec3::new(b.cos(), -tilt * 0.8, b.sin()).normalize();
    [d1, d2]
}

/// A marked strike's warning. `pos` = where it lands, `ground` = the floor under it (if any),
/// `u` = 0..1 of the wait, `left` = seconds to landing.
#[allow(clippy::too_many_arguments)]
pub fn telegraph(pen: &mut Pen, v: &View, look: &Look, kind: StrikeKind, pos: Vec3, ground: Option<Vec3>, radius: f32, u: f32, left: f32, t: f32, seed: u32) {
    let r = radius.clamp(0.4, 12.0);
    let p = pulse(t, left);
    let g = ground.map(|g| g + Vec3::Y * 0.07);
    let far = ((pos - v.cam).length() / 12.0).clamp(1.0, 2.4);
    let fade_in = (t / 0.1).min(1.0);
    let glow = look.glow * fade_in;
    let danger = mix3(look.main, [1.0, 0.25, 0.2], 0.15);
    match kind {
        StrikeKind::Slash => {
            // Hairline cuts hanging in the air, lengthening and brightening until they open.
            let [d1, d2] = slash_dirs(seed);
            let len = r * (0.35 + 0.95 * u);
            let jit = if u > 0.8 { (pen.fx.rng.f() - 0.5) * 0.08 } else { 0.0 };
            for (k, d) in [d1, d2].into_iter().enumerate() {
                if k == 1 && u < 0.35 {
                    break;
                }
                let o = Vec3::Y * jit;
                seg(pen, v, pos - d * len + o, pos + d * len + o, (0.018 + 0.03 * u) * far, look.core, glow * (0.8 + 1.4 * u) * p);
                seg(pen, v, pos - d * len * 0.6, pos + d * len * 0.6, (0.06 + 0.05 * u) * far, look.main, glow * 0.35 * p);
            }
            if let Some(g) = g {
                dashed_ring(pen, v, g, Vec3::Y, r, 24, 0.04 * far, danger, glow * 0.45 * p, t * 0.6);
            }
        }
        StrikeKind::Bolt => {
            let g = g.unwrap_or(pos);
            // The ground ring (the reach of the bolt) with an inner ring closing on the centre.
            circle(pen.fx, g, Vec3::Y, r, 28, 0.045 * far, danger, glow * 0.8 * p, t);
            circle(pen.fx, g, Vec3::Y, r * (1.0 - 0.85 * u).max(0.1), 20, 0.03 * far, look.core, glow * 1.2 * p, -t * 2.0);
            // A faint column up into the sky: where it will come from, with sparks crawling up it.
            let top = g + Vec3::Y * 16.0;
            for i in (0..16).step_by(2) {
                seg(pen, v, g.lerp(top, i as f32 / 16.0), g.lerp(top, (i + 1) as f32 / 16.0), (0.025 + 0.025 * u) * far, look.main, glow * (0.3 + 0.6 * u) * p);
            }
            if pen.fx.rng.f() < 0.4 + 0.5 * u {
                let q = g.lerp(top, pen.fx.rng.f()) + pen.fx.rng.dir() * 0.2;
                let rs = pen.fx.rng.next_u32();
                let rd = pen.fx.rng.dir();
                zig(pen.fx, q, q + Vec3::Y * 0.8 + rd * 0.3, 3, 0.15, rs, 0.02, look.core, glow * 1.5);
            }
        }
        StrikeKind::Holy => {
            // A halo sinking out of the sky onto the spot.
            let g0 = g.unwrap_or(pos);
            let h = 9.0 * (1.0 - u) + 1.5;
            circle(pen.fx, g0 + Vec3::Y * h, Vec3::Y, r * 0.8, 24, 0.06 * far, look.main, glow * 1.2 * p, t * 0.8);
            circle(pen.fx, g0 + Vec3::Y * h, Vec3::Y, r * 0.55, 20, 0.03 * far, look.core, glow * 1.5 * p, -t);
            circle(pen.fx, g0, Vec3::Y, r, 28, 0.04 * far, danger, glow * 0.7 * p, t * 0.5);
            circle(pen.fx, g0, Vec3::Y, r * u.max(0.05), 24, 0.025 * far, look.core, glow * p, 0.0);
        }
        StrikeKind::Void => {
            // Space puckers: a dark shell spinning in, tighter and faster toward the pop.
            let rr = r * (1.0 - 0.7 * u);
            circle(pen.fx, pos, Vec3::Y, rr, 20, 0.05 * far, look.dark, 0.3, t * (2.0 + 6.0 * u));
            circle(pen.fx, pos, Vec3::new(0.3, 0.2, 1.0), rr * 0.8, 18, 0.03 * far, look.main, glow * p, -t * (3.0 + 6.0 * u));
            if pen.fx.rng.f() < 0.5 {
                pen.implode(look, pos, rr * 1.3, 0.35, 0.15);
            }
            if let Some(g) = g {
                dashed_ring(pen, v, g, Vec3::Y, r, 20, 0.04 * far, danger, glow * 0.5 * p, t);
            }
        }
        StrikeKind::Blast => {
            let g0 = g.unwrap_or(pos);
            // The classic: an outline at the blast radius, a fill ring growing to it, four ticks.
            circle(pen.fx, g0, Vec3::Y, r, 32, 0.05 * far, danger, glow * 0.9 * p, 0.0);
            circle(pen.fx, g0, Vec3::Y, r * u.max(0.04), 28, 0.07 * far, mix3(look.main, look.core, 0.4 * u), glow * (0.5 + u) * p, 0.0);
            for k in 0..4 {
                let a = k as f32 * PI * 0.5 + t * 0.4;
                let d = Vec3::new(a.cos(), 0.0, a.sin());
                seg(pen, v, g0 + d * r * 1.08, g0 + d * r * 0.8, 0.06 * far, danger, glow * p);
            }
            if g.is_some() && pos.y - g0.y > 1.2 {
                for i in (0..8).step_by(2) {
                    seg(pen, v, pos.lerp(g0, i as f32 / 8.0), pos.lerp(g0, (i + 1) as f32 / 8.0), 0.025, look.main, glow * 0.5);
                }
            }
        }
    }
}

/// A strike landing: the one-shot part (bursts, beams with a life). `dir` = a direction to read
/// it along (ZERO = none). A Bolt also re-strikes per frame for a beat: see [`bolt_now`].
pub fn strike(pen: &mut Pen, look: &Look, kind: StrikeKind, pos: Vec3, ground: Option<Vec3>, radius: f32, seed: u32) {
    let r = radius.clamp(0.4, 12.0);
    match kind {
        StrikeKind::Slash => {
            // The cut opens: a bright seam with a dark slit inside, the space around it shatters.
            let [d1, d2] = slash_dirs(seed);
            for (k, d) in [d1, d2].into_iter().enumerate() {
                let len = r * 1.35;
                let (a, b) = (pos - d * len, pos + d * len);
                let across = d.cross((pos - pen.cam).normalize_or(Vec3::Z)).normalize_or(Vec3::Y);
                pen.beam(a, b, 0.2 * look.size, look.main, look.glow * 1.2, 0.18);
                pen.beam(a + across * 0.05, b + across * 0.05, 0.04, look.core, look.glow * 3.0, 0.22 + 0.04 * k as f32);
                pen.beam(a - across * 0.05, b - across * 0.05, 0.04, look.core, look.glow * 3.0, 0.22 + 0.04 * k as f32);
                let shards = Look { body: Body::Shard, ..*look };
                pen.burst(&shards, pos, 0.5, 5.0, Some(across), 0.5, 0.35);
                pen.burst(&shards, pos, 0.5, 5.0, Some(-across), 0.5, 0.35);
            }
            flash(pen, pos, 0.9 * look.size, look.core, look.glow * 1.5, 0.08);
        }
        StrikeKind::Bolt => {
            let g = ground.unwrap_or(pos);
            flash(pen, g + Vec3::Y * 0.6, 1.0, look.core, look.glow * 1.6, 0.1);
            pen.ring(look, g + Vec3::Y * 0.1, Vec3::Y, 0.3, r * 4.0, 1.2, 0.3);
            let sparks = Look { body: Body::Bolt, ..*look };
            pen.burst(&sparks, g + Vec3::Y * 0.3, 1.2, 9.0, Some(Vec3::Y), 1.1, 0.35);
            pen.beam(g, g + Vec3::Y * 26.0, 0.35 * look.size, look.main, look.glow * 1.2, 0.12);
        }
        StrikeKind::Holy => {
            let g = ground.unwrap_or(pos - Vec3::Y);
            pen.beam(g, g + Vec3::Y * 18.0, r * 0.45, look.main, look.glow * 1.1, 0.45);
            pen.beam(g, g + Vec3::Y * 18.0, r * 0.15, look.core, look.glow * 2.2, 0.35);
            pen.ring(look, g + Vec3::Y * 0.1, Vec3::Y, 0.3, r * 3.5, 1.4, 0.4);
            flash(pen, g + Vec3::Y * 1.0, 1.0, look.core, look.glow * 1.4, 0.14);
            let halo = Look { body: Body::Halo, ..*look };
            pen.burst(&halo, g + Vec3::Y * 0.5, 1.0, 3.0, Some(Vec3::Y), 0.4, 0.8);
        }
        StrikeKind::Void => {
            pen.implode(look, pos, r * 1.2, 0.1, 1.2);
            flash(pen, pos, 0.9, look.main, look.glow, 0.14);
            pen.ring(look, pos, Vec3::Y, 0.2, r * 3.5, 1.0, 0.3);
            pen.burst(look, pos, 1.0, r * 2.5, None, 0.0, 0.4);
        }
        StrikeKind::Blast => {
            // The world's explosion draws the fireball / flash / debris; this puts the MOVE's body
            // into it (smoke, shards, sparks, halo…) and a ground ring in its colour.
            pen.burst(look, pos, 1.3, r * 3.0, None, 0.0, 0.5);
            if let Some(g) = ground {
                pen.ring(look, g + Vec3::Y * 0.1, Vec3::Y, 0.4, r * 3.2, 1.2, 0.35);
            }
        }
    }
}

/// A bolt re-struck this frame (the sky → the spot), `u` 0..1 of its short life.
pub fn bolt_now(pen: &mut Pen, look: &Look, from: Vec3, to: Vec3, u: f32, seed: u32) {
    let fade = (1.0 - u).max(0.0);
    zig(pen.fx, from, to, 14, 1.4 + look.jitter, seed, 0.09 * fade + 0.03, look.core, look.glow * 3.0 * fade);
    zig(pen.fx, from, to, 10, 2.2 + look.jitter, seed ^ 0x55, 0.05 * fade + 0.015, look.main, look.glow * 2.0 * fade);
    // Two forks off the main channel.
    for k in 0..2u32 {
        let f = 0.35 + 0.25 * k as f32;
        let q = from.lerp(to, f);
        let d = Vec3::new(h01(seed ^ (k * 31)) - 0.5, -0.6, h01(seed ^ (k * 57)) - 0.5).normalize() * 3.5;
        zig(pen.fx, q, q + d, 5, 0.5, seed.wrapping_add(k * 7), 0.025 * fade, look.main, look.glow * 1.6 * fade);
    }
}

/// A radial impact star (a hit landing: beams out of the point, a flash, sparks back along `dir`).
pub fn impact_star(pen: &mut Pen, look: &Look, pos: Vec3, dir: Vec3, size: f32) {
    let s = size.clamp(0.3, 3.0);
    flash(pen, pos, 0.9 * s, look.core, look.glow * 1.5, 0.1);
    for k in 0..6 {
        let d = pen.fx.rng.dir();
        pen.beam(pos + d * 0.2 * s, pos + d * (0.9 + 0.6 * (k % 3) as f32 * 0.5) * s, 0.05 * s, if k % 2 == 0 { look.core } else { look.main }, look.glow * 1.6, 0.12);
    }
    let back = if dir.length_squared() > 0.01 { Some(dir.normalize()) } else { None };
    pen.burst(look, pos, 0.7 * s, 6.0 * s, back, 0.7, 0.3);
}

// ───────────────────────────────────────────────────────────────── tethers

/// What the line is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TetherBody {
    /// Chain links + a kunai / hook head (Scorpion's spear, Roadhog's hook).
    Chain,
    /// A plain energy lead with a hot core (web lines, grapples in hard light).
    Lead,
    /// A stretchy bungee band (Bungee Gum, a haul line): wobbles, sags when slack.
    Bungee,
    /// A ghostly possession trail (dotted, wavy).
    Spirit,
}

pub fn tether_body(look: &Look) -> TetherBody {
    match look.body {
        Body::Rubber => TetherBody::Bungee,
        Body::Smoke | Body::Void => TetherBody::Spirit,
        Body::Energy | Body::Halo | Body::Bolt | Body::Streak | Body::Glitch => TetherBody::Lead,
        _ => TetherBody::Chain,
    }
}

/// A tether from `from` (the hand) to `tip`. `slack` 0 = taut … 1 = sagging; `head` = draw the head.
#[allow(clippy::too_many_arguments)]
pub fn tether(pen: &mut Pen, v: &View, look: &Look, body: TetherBody, from: Vec3, tip: Vec3, t: f32, slack: f32, head: bool, seed: u32) {
    let d = tip - from;
    let len = d.length();
    if len < 0.05 {
        return;
    }
    let dir = d / len;
    let segs = ((len * 1.5) as usize).clamp(6, 40);
    // From your own fist the near links are right at the lens: thinner there.
    // …and thicker far away (a chain 20 m off must still read as a chain).
    let near = |p: Vec3| {
        let d = p.distance(v.cam);
        ((d - 0.5) / 3.0).clamp(0.35, 1.0) * (d / 12.0).clamp(1.0, 2.2)
    };
    let sag = slack.clamp(0.0, 1.0) * (len * 0.08).min(2.5);
    let (u, w) = basis(dir);
    let wob = if body == TetherBody::Bungee { 0.08 + 0.25 * slack } else { 0.0 };
    let at = |f: f32| -> Vec3 {
        let env = (f * PI).sin();
        let mut p = from + d * f - Vec3::Y * sag * env * 4.0 * f * (1.0 - f).max(0.0) * 1.0;
        if wob > 0.0 {
            p += (u * ((f * 3.0 + t * 2.2) * TAU).sin() + w * ((f * 2.0 - t * 1.7) * TAU).cos() * 0.5) * wob * env;
        }
        if body == TetherBody::Spirit {
            p += (u * ((f * 5.0 - t * 3.0) * TAU).sin()) * 0.12 * env;
        }
        p
    };
    let mut prev = from;
    for i in 1..=segs {
        let f = i as f32 / segs as f32;
        let p = at(f);
        match body {
            TetherBody::Chain => {
                // Links: alternating boxes turned 90° to each other along the line, a dull rope core.
                let mid = (prev + p) * 0.5;
                let k = near(mid);
                seg(pen, v, prev, p, 0.025 * k, look.dark, 0.4);
                let q = Quat::from_rotation_arc(Vec3::Z, (p - prev).normalize_or(dir)) * Quat::from_rotation_z(if i % 2 == 0 { 0.0 } else { PI * 0.5 });
                cbox(pen, v, mid, q, Vec3::new(0.09 * k, 0.035 * k, (p - prev).length() * 0.8), mix3(look.dark, look.main, 0.55), look.glow * 0.6);
            }
            TetherBody::Lead => {
                let k = near((prev + p) * 0.5);
                seg(pen, v, prev, p, 0.07 * look.size * k, look.main, look.glow * 0.7);
                seg(pen, v, prev, p, 0.025 * look.size * k, look.core, look.glow * 1.4);
            }
            TetherBody::Bungee => {
                let k = near((prev + p) * 0.5);
                seg(pen, v, prev, p, 0.06 * look.size * k, look.main, look.glow * 0.8);
                if i % 2 == 0 {
                    seg(pen, v, prev, p, 0.02, look.core, look.glow * 1.3);
                }
            }
            TetherBody::Spirit => {
                if i % 2 == 0 {
                    seg(pen, v, prev, p, 0.05, look.main, look.glow * 0.9);
                }
            }
        }
        prev = p;
    }
    // Flame bodies shed embers along the rope (Scorpion's hellfire chain).
    if matches!(look.body, Body::Flame) && pen.fx.rng.f() < 0.6 {
        let q = at(pen.fx.rng.f());
        pen.mote(look, q, Vec3::Y * 0.8, 0.3, 0.0);
    }
    if head {
        let rot = Quat::from_rotation_arc(Vec3::Z, dir);
        match body {
            TetherBody::Chain => {
                // A kunai / hook: a blade box, two barbs swept back.
                cbox(pen, v, tip - dir * 0.15, rot, Vec3::new(0.07, 0.07, 0.4), mix3(look.core, [0.8, 0.8, 0.85], 0.5), look.glow * 0.8);
                let (bu, _) = basis(dir);
                seg(pen, v, tip - dir * 0.1, tip - dir * 0.4 + bu * 0.2, 0.035, look.main, look.glow);
                seg(pen, v, tip - dir * 0.1, tip - dir * 0.4 - bu * 0.2, 0.035, look.main, look.glow);
            }
            _ => {
                cbox(pen, v, tip, rot * Quat::from_rotation_z(t * 6.0), Vec3::splat(0.16 * look.size), look.core, look.glow * 1.3);
            }
        }
        let _ = seed;
    }
}

// ───────────────────────────────────────────────────────────────── auras

/// A state on a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuraKind {
    /// Empower: body motes streaming off you, a pulsing ground ring (rainbow for a holy / light style).
    Empower,
    /// Perfect-dodge window: a silver shimmer spinning round you.
    EvadeWindow,
    /// Counter stance: a gleam in front of the chest.
    Counter,
    /// Guard: a hex cage.
    Guard,
    /// Deflect: arcs across the front cone (`param` = the cone half-angle, radians).
    Deflect,
    /// Phase (intangible): a vortex round the body + ghost flicker.
    Phase,
    /// Heal over time: rising crosses.
    Mend,
    /// Lock-on channel (the caster): a glare disc behind the head.
    Glare,
    /// Rush: fists hammering in front.
    Rush,
}

/// Draw an aura on a player (`feet`, `size`, `yaw`/`look` for the front ones). `k` = 0..1 fade.
#[allow(clippy::too_many_arguments)]
pub fn aura(pen: &mut Pen, v: &View, look: &Look, kind: AuraKind, feet: Vec3, size: f32, front: Vec3, t: f32, left: f32, k: f32, param: f32, rainbow: bool, own_fp: bool) {
    let s = size.max(0.3);
    let chest = feet + Vec3::Y * 1.05 * s;
    let glow = look.glow * k;
    let front = Vec3::new(front.x, 0.0, front.z).normalize_or(Vec3::NEG_Z);
    match kind {
        AuraKind::Empower => {
            let col = if rainbow { hue(t * 0.9) } else { look.main };
            let col2 = if rainbow { hue(t * 0.9 + 0.33) } else { look.core };
            let styled = Look { main: col, core: col2, ..*look };
            // Streaming off the body (two motes a frame, budgeted by the pen).
            if !own_fp {
                let styled = Look { size: styled.size * 1.8, ..styled };
                for _ in 0..4 {
                    let a = pen.fx.rng.f() * TAU;
                    let h = pen.fx.rng.f() * 1.8 * s;
                    let q = feet + Vec3::new(a.cos() * 0.38 * s, h, a.sin() * 0.38 * s);
                    pen.mote(&styled, q, Vec3::Y * 1.8 + Vec3::new(a.cos(), 0.0, a.sin()) * 0.4, 0.45, 0.0);
                }
            }
            let p = 0.7 + 0.3 * (t * 6.0).sin();
            circle(pen.fx, feet + Vec3::Y * 0.06, Vec3::Y, 0.8 * s * (1.0 + 0.08 * (t * 6.0).sin()), 24, 0.05 * s, col, glow * p, t * 2.0);
            if !own_fp {
                circle(pen.fx, chest, Vec3::Y, 0.6 * s, 18, 0.045 * s, col2, glow * 1.2 * p, -t * 3.0);
                circle(pen.fx, feet + Vec3::Y * (0.2 + (t * 1.5).fract() * 1.8) * s, Vec3::Y, 0.7 * s, 18, 0.04 * s, col, glow * (1.0 - (t * 1.5).fract()), t);
            }
            if left < 1.2 && ((t * 10.0) as u32) % 2 == 0 {
                circle(pen.fx, feet + Vec3::Y * 0.9 * s, Vec3::Y, 0.7 * s, 18, 0.03 * s, look.core, glow, 0.0);
            }
        }
        AuraKind::EvadeWindow => {
            if own_fp {
                return;
            }
            let silver = mix3([0.85, 0.9, 1.0], look.main, 0.25);
            for j in 0..2 {
                let a = t * 14.0 + j as f32 * PI;
                let n = Vec3::new(a.cos(), 0.0, a.sin());
                circle(pen.fx, chest, n, 0.95 * s, 20, 0.018 * s, silver, glow * 1.3, 0.0);
            }
        }
        AuraKind::Counter => {
            if own_fp {
                return;
            }
            let c = chest + front * 0.55 * s;
            let n = front;
            let p = 0.7 + 0.3 * (t * 9.0).sin();
            circle(pen.fx, c, n, 0.55 * s, 24, 0.03 * s, look.main, glow * p, t);
            // The gleam: a four-point star sliding round the rim.
            let (u, w) = basis(n);
            let a = t * 5.0;
            let g = c + (u * a.cos() + w * a.sin()) * 0.55 * s;
            let r = 0.22 * s * p;
            seg(pen, v, g - u * r, g + u * r, 0.03 * s, look.core, glow * 2.0);
            seg(pen, v, g - w * r, g + w * r, 0.03 * s, look.core, glow * 2.0);
        }
        AuraKind::Guard => {
            let p = 0.7 + 0.3 * (t * 5.0).sin();
            let r = 0.75 * s;
            let n = 6;
            for i in 0..n {
                let a = i as f32 / n as f32 * TAU + t * 0.5;
                let q = feet + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                if own_fp && i % 2 == 1 {
                    continue;
                }
                seg(pen, v, q + Vec3::Y * 0.1, q + Vec3::Y * 1.9 * s, 0.025 * s, look.main, glow * 0.7 * p);
            }
            circle(pen.fx, feet + Vec3::Y * 0.1, Vec3::Y, r, 6, 0.03 * s, look.main, glow * p, t * 0.5);
            if !own_fp {
                circle(pen.fx, feet + Vec3::Y * 1.9 * s, Vec3::Y, r, 6, 0.03 * s, look.main, glow * p, t * 0.5);
            }
        }
        AuraKind::Deflect => {
            if own_fp {
                return;
            }
            let half = param.clamp(0.2, PI);
            let side = Vec3::new(-front.z, 0.0, front.x);
            for j in 0..3 {
                let r = (0.8 + 0.18 * j as f32) * s;
                let h = chest + Vec3::Y * (j as f32 - 1.0) * 0.45 * s;
                let n = 10;
                let mut prev = h + (front * (-half).cos() + side * (-half).sin()) * r;
                for i in 1..=n {
                    let a = -half + 2.0 * half * i as f32 / n as f32;
                    let q = h + (front * a.cos() + side * a.sin()) * r;
                    let flick = 0.6 + 0.4 * ((t * 20.0 + i as f32 + j as f32 * 3.0).sin());
                    seg(pen, v, prev, q, 0.025 * s, look.main, glow * flick);
                    prev = q;
                }
            }
        }
        AuraKind::Phase => {
            if !own_fp {
                // Two spiral arms turning round the body (a vortex), thin and fast.
                for arm in 0..2 {
                    let mut prev = feet;
                    for i in 1..=16 {
                        let f = i as f32 / 16.0;
                        let a = t * 5.0 + f * 2.5 * TAU + arm as f32 * PI;
                        let r = (0.55 - 0.25 * f) * s;
                        let q = feet + Vec3::new(a.cos() * r, f * 2.0 * s, a.sin() * r);
                        seg(pen, v, prev, q, 0.02 * s, if arm == 0 { look.main } else { look.core }, glow * 0.9);
                        prev = q;
                    }
                }
            }
            circle(pen.fx, feet + Vec3::Y * 0.05, Vec3::Y, 0.6 * s, 18, 0.03 * s, look.dark, 0.3, t);
        }
        AuraKind::Mend => {
            if pen.fx.rng.f() < 0.5 && !own_fp {
                let a = pen.fx.rng.f() * TAU;
                let q = feet + Vec3::new(a.cos() * 0.5 * s, 0.4 + pen.fx.rng.f() * 1.2 * s, a.sin() * 0.5 * s);
                cross(pen, look, q, 0.09 * s, Vec3::Y * 1.4, 0.7);
            }
            circle(pen.fx, feet + Vec3::Y * (0.1 + (t * 0.8).fract() * 1.8 * s), Vec3::Y, 0.5 * s, 18, 0.02 * s, look.main, glow * 0.8 * (1.0 - (t * 0.8).fract()), 0.0);
        }
        AuraKind::Glare => {
            if own_fp {
                return;
            }
            // High noon: a sun disc behind the head, rays turning slowly, a red ring at the feet.
            let c = feet + Vec3::Y * 2.1 * s - front * 0.25 * s;
            let n = (c - v.cam).normalize_or(v.fwd);
            let p = 0.75 + 0.25 * (t * 3.0).sin();
            circle(pen.fx, c, n, 0.32 * s, 18, 0.05 * s, look.core, glow * 1.4 * p, 0.0);
            let (u, w) = basis(n);
            for i in 0..8 {
                let a = t * 0.6 + i as f32 * TAU / 8.0;
                let d = u * a.cos() + w * a.sin();
                seg(pen, v, c + d * 0.42 * s, c + d * (0.62 + 0.12 * (i % 2) as f32) * s, 0.03 * s, look.main, glow * p);
            }
            circle(pen.fx, feet + Vec3::Y * 0.06, Vec3::Y, 1.3 * s, 28, 0.035 * s, look.main, glow * 0.7 * p, t * 0.3);
        }
        AuraKind::Rush => {
            if own_fp {
                return;
            }
            // Fists everywhere in front: light boxes blinking in and out along the punch lines.
            let side = Vec3::new(-front.z, 0.0, front.x);
            for _ in 0..3 {
                let r1 = pen.fx.rng.f() - 0.5;
                let r2 = pen.fx.rng.f() - 0.5;
                let reach = 0.6 + pen.fx.rng.f() * 0.9;
                let base = chest + side * r1 * 0.9 * s + Vec3::Y * r2 * 0.8 * s;
                let tip = base + front * reach * s;
                seg(pen, v, base, tip, 0.05 * s, look.main, glow * 0.8);
                cbox(pen, v, tip, Quat::from_rotation_arc(Vec3::Z, front), Vec3::splat(0.2 * s), look.core, glow * 1.2);
            }
        }
    }
}

/// A heal cross floating up: seven cubes (a 3D plus, reads as "+" from every side) sharing one
/// velocity and life.
pub fn cross(pen: &mut Pen, look: &Look, c: Vec3, r: f32, vel: Vec3, life: f32) {
    const ARMS: [[f32; 3]; 7] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
    for (k, o) in ARMS.into_iter().enumerate() {
        let seed = pen.fx.rng.next_u32() & !0xFFFF;
        pen.put(Emit {
            kind: PKind::default(),
            flags: PFlags::PLAIN,
            pos: c + Vec3::from(o) * r,
            vel,
            size: r * 0.95,
            color: if k == 0 { look.core } else { look.main },
            emissive: look.glow * 1.2,
            life,
            gravity: 0.0,
            drag: 0.0,
            spin: 0.0,
            seed,
            ..default()
        });
    }
}

/// The resource pulse (a heal landing): a ring climbing the body, crosses bursting up, a soft flash.
pub fn heal_pulse(pen: &mut Pen, look: &Look, feet: Vec3, size: f32, amount: f32) {
    let s = size.max(0.3);
    let k = (amount / 30.0).clamp(0.4, 1.6);
    pen.ring(look, feet + Vec3::Y * 0.1, Vec3::Y, 0.3, 3.5 * k, 1.0, 0.45);
    let up = Look { body: Body::Halo, ..*look };
    pen.burst(&up, feet + Vec3::Y * 0.9 * s, 1.0 * k, 2.5, Some(Vec3::Y), 0.6, 0.9);
    for i in 0..(4.0 * k) as usize {
        let a = i as f32 * 2.1;
        let q = feet + Vec3::new(a.cos() * 0.5 * s, 0.6 + 0.3 * i as f32 * s, a.sin() * 0.5 * s);
        cross(pen, look, q, 0.1 * s, Vec3::Y * 1.6, 0.8);
    }
    flash(pen, feet + Vec3::Y * 1.0 * s, 1.1 * s, look.core, look.glow * 0.8, 0.15);
}

// ───────────────────────────────────────────────────────────────── extras

/// A flying crescent (a Getsuga, a flying slash, a Destructo Disc when `flat`): `pos` = its centre,
/// `dir` its travel, `width` its span. Bowed forward; afterimages trail behind.
#[allow(clippy::too_many_arguments)]
pub fn crescent(pen: &mut Pen, v: &View, look: &Look, pos: Vec3, dir: Vec3, width: f32, flat: bool, t: f32, glow: f32) {
    let dir = dir.normalize_or(Vec3::NEG_Z);
    let side = Vec3::Y.cross(dir).normalize_or(Vec3::X);
    // The crescent's span axis: across the travel, upright (a slash) or flat (a disc).
    let span = if flat { side } else { (Vec3::Y * 0.8 + side * 0.6).normalize() };
    let half = width.clamp(0.5, 12.0) * 0.5;
    let n = 14;
    let far = ((pos - v.cam).length() / 12.0).clamp(1.0, 2.2);
    for ghost in 0..3 {
        let back = dir * ghost as f32 * 0.55;
        let g = glow * [1.0, 0.45, 0.2][ghost];
        let th = [0.3, 0.18, 0.1][ghost] * look.size * far;
        let mut prev = None;
        for i in 0..=n {
            let f = i as f32 / n as f32 * 2.0 - 1.0;
            let bow = (1.0 - f * f) * half * 0.45;
            let thin = 1.0 - f.abs() * 0.8;
            let q = pos - back + span * f * half + dir * bow;
            if let Some(pq) = prev {
                seg(pen, v, pq, q, th * thin, if ghost == 0 { look.main } else { look.dark }, g);
                if ghost == 0 {
                    seg(pen, v, pq + dir * 0.06, q + dir * 0.06, th * thin * 0.35, look.core, g * 1.8);
                }
            }
            prev = Some(q);
        }
    }
    if flat {
        // A disc spins: a ring inside the crescent.
        circle(pen.fx, pos, Vec3::Y, half * 0.6, 16, 0.04, look.core, glow, t * 20.0);
    }
    if pen.fx.rng.f() < 0.7 {
        let f = pen.fx.rng.f() * 2.0 - 1.0;
        let q = pos + span * f * half;
        let rd = pen.fx.rng.dir();
        pen.mote(look, q, -dir * 3.0 + rd, 0.35, 0.0);
    }
}

/// A hand-held grinding sphere (`bolt` = a crackling hand instead). `r` = its radius.
pub fn orb(pen: &mut Pen, v: &View, look: &Look, c: Vec3, r: f32, t: f32, bolt: bool, glow: f32) {
    if bolt {
        for k in 0..4u32 {
            let d = Vec3::new(h01((t * 30.0) as u32 ^ k * 11) - 0.5, h01((t * 30.0) as u32 ^ k * 29) - 0.5, h01((t * 30.0) as u32 ^ k * 47) - 0.5).normalize_or(Vec3::Y);
            zig(pen.fx, c, c + d * (r * 2.8), 4, 0.15 + look.jitter * 0.3, (t * 40.0) as u32 ^ k.wrapping_mul(977), 0.03, if k % 2 == 0 { look.core } else { look.main }, glow * 2.0);
        }
        flash(pen, c, r * 1.5, look.core, glow * 0.6, 0.05);
        return;
    }
    // Three rings on turning axes + a hot core cube: reads as a spinning ball of wind / energy.
    for k in 0..3 {
        let a = t * (9.0 + k as f32 * 3.0) + k as f32 * 2.1;
        let n = Vec3::new(a.cos(), (k as f32 - 1.0) * 0.8, a.sin()).normalize();
        circle(pen.fx, c, n, r * (0.8 + 0.12 * k as f32), 14, 0.03 * r.max(0.3) * 2.0, if k == 1 { look.core } else { look.main }, glow * 1.3, t * 12.0);
    }
    cbox(pen, v, c, Quat::from_rotation_y(t * 8.0) * Quat::from_rotation_x(t * 5.0), Vec3::splat(r * 0.7), look.core, glow * 1.6);
    // Wind tearing round it: motes flung tangentially off the surface.
    for k in 0..2 {
        let a = t * 30.0 + k as f32 * PI;
        let d = Vec3::new(a.cos(), (t * 7.0 + k as f32).sin() * 0.5, a.sin()).normalize();
        let tan = Vec3::Y.cross(d).normalize_or(d);
        pen.mote(look, c + d * r, tan * 6.0, 0.2, 0.0);
    }
}

/// Spiral shreds flung off a grinding contact (called per frame while it grinds).
pub fn grind_shreds(pen: &mut Pen, look: &Look, c: Vec3, axis: Vec3, t: f32) {
    let (u, w) = basis(axis);
    for k in 0..3 {
        let a = t * 25.0 + k as f32 * TAU / 3.0;
        let d = u * a.cos() + w * a.sin();
        let tangent = axis.cross(d).normalize_or(d);
        pen.mote(look, c + d * 0.35, tangent * 9.0 + d * 3.0, 0.25, 0.0);
    }
}

/// A leap's landing preview: a dotted arc from → to (apex `apex` m) and a ring at the landing that
/// closes as the jumper arrives (`u` 0..1 of the flight).
#[allow(clippy::too_many_arguments)]
pub fn arc_preview(pen: &mut Pen, v: &View, look: &Look, from: Vec3, to: Vec3, apex: f32, ground: Option<Vec3>, u: f32, t: f32) {
    // apex < 0: the jumper's own view — only the landing ring (the arc would cross the lens).
    let own = apex < 0.0;
    let top = from.y.max(to.y) + apex.abs().clamp(0.5, 40.0);
    let n = 24;
    let at = |f: f32| {
        let mut p = from.lerp(to, f);
        // A parabola through from / to peaking at `top` (approximately at the middle).
        let base = from.y + (to.y - from.y) * f;
        p.y = base + (top - base.max(from.y.min(to.y))) * 4.0 * f * (1.0 - f);
        p
    };
    for i in (0..n).step_by(2) {
        let f0 = i as f32 / n as f32;
        if f0 < u || own {
            continue;
        }
        seg(pen, v, at(f0), at(f0 + 1.0 / n as f32), 0.03, look.main, look.glow * 0.5);
    }
    let g = ground.unwrap_or(to);
    let p = 0.7 + 0.3 * (t * 8.0).sin();
    circle(pen.fx, g + Vec3::Y * 0.07, Vec3::Y, 2.2 * (1.0 - 0.6 * u), 24, 0.06, look.main, look.glow * p, t);
    circle(pen.fx, g + Vec3::Y * 0.07, Vec3::Y, 0.6, 12, 0.04, look.core, look.glow * 1.3 * p, -t * 2.0);
}

/// A roulette wheel over a head: `of` pips spinning, decelerating onto `pick` (`u` 0..1).
pub fn wheel(pen: &mut Pen, v: &View, look: &Look, c: Vec3, of: u8, pick: u8, u: f32, glow: f32) {
    let n = of.clamp(2, 12) as usize;
    let r = 0.45;
    let (right, up) = (v.right, v.up);
    // Ease-out spin: 3 turns, landing with `pick` at the top.
    let e = 1.0 - (1.0 - u.clamp(0.0, 1.0)).powi(3);
    let spin = (1.0 - e) * 3.0 * TAU;
    let landed = u >= 0.98;
    for i in 0..n {
        let a = PI * 0.5 + (i as f32 - pick as f32) / n as f32 * TAU + spin;
        let q = c + (right * a.cos() + up * a.sin()) * r;
        let hot = landed && i == pick as usize;
        cbox(pen, v, q, Quat::IDENTITY, Vec3::splat(if hot { 0.14 } else { 0.08 }), if hot { look.core } else { look.main }, glow * if hot { 2.5 } else { 0.9 });
    }
    // The pointer.
    let top = c + up * (r + 0.18);
    seg(pen, v, top, top - up * 0.12, 0.04, look.core, glow * 1.5);
    circle(pen.fx, c, (c - v.cam).normalize_or(v.fwd), r, 24, 0.012, look.main, glow * 0.5, 0.0);
}

/// A lone portal end on a surface (its partner not open yet): an oval rim + a dim fill.
pub fn portal_oval(pen: &mut Pen, v: &View, c: Vec3, normal: Vec3, color: [f32; 3], t: f32, glow: f32) {
    let n = normal.normalize_or(Vec3::Y);
    let upv = if n.y.abs() > 0.8 { Vec3::Z } else { Vec3::Y };
    let side = upv.cross(n).normalize_or(Vec3::X);
    let up = n.cross(side).normalize_or(Vec3::Y);
    let (rw, rh) = (0.75, 1.15);
    let segs = 28;
    let c = c + n * 0.04;
    let mut prev = c + side * rw;
    for i in 1..=segs {
        let a = i as f32 / segs as f32 * TAU;
        let wob = 1.0 + 0.03 * (a * 5.0 + t * 6.0).sin();
        let q = c + (side * a.cos() * rw + up * a.sin() * rh) * wob;
        seg(pen, v, prev, q, 0.07, color, glow * 1.3);
        prev = q;
    }
    for k in 1..3 {
        let f = 1.0 - k as f32 * 0.28 - (t * 0.8).fract() * 0.28;
        let mut prev = c + side * rw * f;
        for i in 1..=16 {
            let a = i as f32 / 16.0 * TAU;
            let q = c + (side * a.cos() * rw + up * a.sin() * rh) * f;
            seg(pen, v, prev, q, 0.025, color, glow * 0.4);
            prev = q;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_quickens_as_time_runs_out() {
        // Same phase count over a second: the last second ticks faster.
        let crossings = |left: f32| (0..1000).filter(|i| {
            let (a, b) = (pulse(*i as f32 / 1000.0, left), pulse((*i + 1) as f32 / 1000.0, left));
            (a - 0.8).signum() != (b - 0.8).signum()
        }).count();
        assert!(crossings(0.5) > crossings(3.0));
    }

    #[test]
    fn strike_kind_follows_the_style_not_the_word() {
        let mut l = Look::default();
        assert_eq!(strike_kind(&l, false, Element::Light), StrikeKind::Blast, "a blast is a blast");
        assert_eq!(strike_kind(&l, true, Element::Psychic), StrikeKind::Slash, "a hard-light cut is a slash");
        assert_eq!(strike_kind(&l, true, Element::Electric), StrikeKind::Bolt);
        l.body = Body::Bolt;
        assert_eq!(strike_kind(&l, true, Element::Light), StrikeKind::Bolt);
        l.body = Body::Halo;
        assert_eq!(strike_kind(&l, true, Element::Light), StrikeKind::Holy);
        l.body = Body::Void;
        assert_eq!(strike_kind(&l, true, Element::Shadow), StrikeKind::Void);
    }

    #[test]
    fn slash_dirs_are_stable_and_distinct() {
        let a = slash_dirs(42);
        assert_eq!(a, slash_dirs(42), "telegraph and landing agree");
        assert!(a[0].dot(a[1]).abs() < 0.97, "two different cuts");
        assert!((a[0].length() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn hue_wheel_is_bright() {
        for i in 0..12 {
            let c = hue(i as f32 / 12.0);
            assert!(c.iter().cloned().fold(0.0, f32::max) > 0.99);
        }
    }
}
