//! Worn items. Origin = the wearer's upper back (between the shoulder blades); the wearer faces −Z,
//! so worn things sit at +Z and hang/extend from there.

use bd_core::mechanics::Mechanic::*;
use bd_core::shape::{PartRole, PrimKind};
use glam::Vec3;

use super::G;
use crate::build::{A, C, D, P, odd, v3};
use crate::parts::{WingStyle, exhaust, wing_pair};

pub(super) fn wings(g: &mut G) {
    let span = g.l * 0.5;
    let style = match g.r.pick(4) {
        0 => WingStyle::Feather,
        1 => WingStyle::Bat,
        2 => WingStyle::Jet,
        _ => WingStyle::Butterfly,
    };
    let b = &mut g.b;
    b.bx(v3(0.0, -1.0, 1.5), v3(3.0, 4.0, 1.0), D);
    b.bx(v3(0.0, 0.0, 2.0), v3(1.0, 1.0, 1.0), C);
    wing_pair(b, 0, v3(1.5, 0.5, 2.0), span, style, 28.0);
    g.handle(Flies);
    g.handle(Glide);
}

pub(super) fn parachute(g: &mut G) {
    let l = g.l;
    let ram_air = g.r.chance(0.4);
    let hc = l * 0.85;
    let rr = l * 0.5;
    let b = &mut g.b;
    // Harness.
    b.bx(v3(0.0, -0.5, 1.5), v3(3.0, 3.0, 1.0), D);
    b.bx(v3(0.0, 1.5, 1.5), v3(3.0, 1.0, 1.0), A);
    let canopy = b.part(0, v3(0.0, 1.0, 1.5), PartRole::Other);
    let _ = canopy;
    let harness = v3(0.0, 1.5, 1.5);
    let mut line_ends: Vec<Vec3> = Vec::new();
    if ram_air {
        let n = 7;
        let depth = (l * 0.3).max(3.0);
        for i in 0..n {
            let f = i as f32 / (n - 1) as f32;
            let a = (f - 0.5) * 1.6;
            let c = v3(rr * a.sin(), hc + rr * 0.35 * a.cos(), 1.5);
            let t = if i % 2 == 0 { P } else { A };
            b.bxr(c, v3(rr * 1.7 / n as f32 + 1.0, 2.0, depth), v3(0.0, 0.0, -a.to_degrees()), t);
            if i % 2 == 0 {
                line_ends.push(c + v3(0.0, -1.0, 0.0));
            }
        }
        b.sym(|b, s| {
            b.bx(v3(s * rr * 0.72, hc + rr * 0.24, 1.5), v3(1.0, 3.0, depth + 1.0), D);
        });
    } else {
        // Faceted round canopy: apex → mid ring → rim, alternating gores.
        let n = 8;
        let ch = l * 0.34;
        let apex = v3(0.0, hc + ch, 1.5);
        let ring = |r: f32, y: f32, i: usize| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU + 0.2;
            v3(r * a.cos(), y, 1.5 + r * a.sin())
        };
        for i in 0..n {
            let m0 = ring(rr * 0.62, hc + ch * 0.72, i);
            let m1 = ring(rr * 0.62, hc + ch * 0.72, i + 1);
            let r0 = ring(rr, hc, i);
            let r1 = ring(rr, hc, i + 1);
            let t = if i % 2 == 0 { P } else { A };
            let n1 = (m0 - apex).cross(m1 - apex).normalize_or(Vec3::Y);
            b.fan(apex, m0, m1, 0.0, 1.0, n1, t);
            let n2 = (r0 - m0).cross(m1 - m0).normalize_or(Vec3::Y);
            b.fan(m0, r0, r1, 0.0, 1.0, n2, t);
            b.fan(r1, m0, m1, 0.0, 1.0, n2, t);
            if i % 2 == 0 {
                line_ends.push(r0);
            }
        }
        b.bx(apex + v3(0.0, 0.5, 0.0), v3(3.0, 1.0, 3.0), C);
    }
    for e in &line_ends {
        b.seg(PrimKind::Box, harness, *e, 1.0, 1.0, D);
    }
    g.b.on(0);
    g.handle(SlowFall);
}

pub(super) fn jetpack(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3);
    let td = odd(l * 0.42).max(3.0);
    let th = l * 0.8;
    let yc = -l * 0.25;
    let b = &mut g.b;
    b.bx(v3(0.0, yc, 1.5), v3(td * 2.0 + 1.0, th * 0.85, 1.0), D);
    b.sym(|b, s| {
        b.bx(v3(s * (td * 0.5 + 0.5), 1.5, -0.5), v3(1.0, 1.0, 4.0), D);
    });
    let mut nozzles = Vec::new();
    match variant {
        1 => {
            // One big rocket with fins.
            let d = td + 2.0;
            b.cy(v3(0.0, yc, 2.5 + d * 0.5), d, th, P);
            b.el(v3(0.0, yc + th * 0.5, 2.5 + d * 0.5), v3(d, d * 1.2, d), A);
            b.sym(|b, s| {
                b.flat(PrimKind::Box, v3(s * d * 0.4, yc - th * 0.2, 2.5 + d * 0.5), v3(s * (d * 0.5 + 2.0), yc - th * 0.5, 2.5 + d * 0.5), 2.0, 1.0, Vec3::Z, A);
            });
            b.cy(v3(0.0, yc, 2.5 + d * 0.5), d + 1.0, 1.0, C);
            nozzles.push(v3(0.0, yc - th * 0.5, 2.5 + d * 0.5));
        }
        _ => {
            b.sym(|b, s| {
                let c = v3(s * (td * 0.5 + 0.5), yc, 2.5 + td * 0.5);
                b.cy(c, td, th, P);
                b.el(c + v3(0.0, th * 0.5, 0.0), v3(td, td, td), A);
                b.cy(c + v3(0.0, th * 0.15, 0.0), td + 1.0, 1.0, A);
                b.cy(c - v3(0.0, th * 0.15, 0.0), td + 1.0, 1.0, C);
            });
            for s in [-1.0f32, 1.0] {
                nozzles.push(v3(s * (td * 0.5 + 0.5), yc - th * 0.5, 2.5 + td * 0.5));
            }
            if variant == 2 {
                wing_pair(&mut g.b, 0, v3(td + 0.5, yc + th * 0.2, 2.5 + td * 0.5), l * 0.55, WingStyle::Jet, 5.0);
                g.b.on(0);
                g.handle(Glide);
            }
        }
    }
    for n in nozzles {
        g.b.on(0);
        exhaust(&mut g.b, 0, n, Vec3::NEG_Y, (td * 0.3).max(1.0));
    }
    g.b.on(0);
    g.handle(Thrust);
    g.handle(Flies);
}

pub(super) fn cape(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3); // 0 royal, 1 tattered, 2 mantle (short, high collar)
    let len = if variant == 2 { l * 0.7 } else { l * 1.15 };
    let w0 = odd(l * 0.62).max(3.0);
    let b = &mut g.b;
    b.bx(v3(0.0, 0.5, 1.0), v3(w0 + 2.0, if variant == 2 { 3.0 } else { 1.0 }, 2.0), A);
    b.sym(|b, s| {
        b.bx(v3(s * (w0 * 0.5 + 0.5), 0.5, 0.0), v3(1.0, 1.0, 1.0), C);
    });
    let n = 4;
    let seg = len / n as f32;
    let mut parent = 0;
    for k in 0..n {
        let f0 = k as f32 / n as f32;
        let y0 = -f0 * len;
        let z = 1.5 + k as f32 * 0.7;
        let w = w0 + (f0 * l * 0.3).round() * 2.0;
        parent = b.part(parent, v3(0.0, y0, z), PartRole::Other);
        b.bx(v3(0.0, y0 - seg * 0.5, z + 0.5), v3(w, seg + 0.5, 1.0), P);
        b.bx(v3(0.0, y0 - seg * 0.5, z - 0.5), v3(w - 2.0, seg + 0.5, 1.0), D);
        if variant == 0 {
            b.sym(|b, s| {
                b.bx(v3(s * (w * 0.5 - 0.5), y0 - seg * 0.5, z + 0.5), v3(1.0, seg + 0.5, 1.0), A);
            });
        }
        if k + 1 == n {
            let yb = y0 - seg;
            match variant {
                1 => {
                    let cuts = ((w / 3.0) as i32).max(2);
                    for i in 0..cuts {
                        let x = -w * 0.5 + (i as f32 + 0.5) * w / cuts as f32;
                        b.cut_el(v3(x, yb, z), v3(2.0, seg * 0.9, 4.0));
                    }
                }
                _ => {
                    b.bx(v3(0.0, yb + 0.5, z + 0.5), v3(w, 1.0, 1.0), A);
                    b.bx(v3(0.0, yb - 0.5, z + 0.5), v3(1.0, 1.0, 1.0), C);
                }
            }
        }
    }
    g.b.on(0);
    g.handle(Glide);
}
