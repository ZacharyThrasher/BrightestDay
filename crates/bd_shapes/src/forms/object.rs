//! Objects (thrown / dropped / launched). Origin = centre of mass; forward (flight / facing) = −Z.

use bd_core::mechanics::Mechanic::*;
use bd_core::shape::PrimKind;
use glam::Vec3;

use super::G;
use crate::build::{A, C, D, P, odd, v3};
use crate::parts::exhaust;

pub(super) fn ball(g: &mut G) {
    let d = odd(g.l);
    let variant = g.r.pick(3);
    let b = &mut g.b;
    b.el(Vec3::ZERO, Vec3::splat(d), P);
    match variant {
        0 => {
            // Seamed sports ball.
            b.cx(Vec3::ZERO, d + 0.4, 1.0, A);
            b.cz(Vec3::ZERO, d + 0.4, 1.0, A);
        }
        1 => {
            // Beach ball: bands + caps.
            b.cy(v3(0.0, d * 0.25, 0.0), d, d * 0.2, A);
            b.cy(v3(0.0, -d * 0.25, 0.0), d, d * 0.2, D);
            b.cy(v3(0.0, d * 0.5 - 0.5, 0.0), 1.0, 1.0, C);
        }
        _ => {
            // Bowling ball.
            for (x, y) in [(-1.0f32, 1.5f32), (1.0, 1.5), (0.0, 0.0)] {
                b.cut(PrimKind::Cylinder, v3(x, y, -d * 0.5), v3(1.0, 3.0, 1.0), v3(90.0, 0.0, 0.0));
            }
            b.el(v3(d * 0.2, d * 0.3, -d * 0.35), v3(1.0, 1.0, 1.0), C);
        }
    }
    b.el(v3(-d * 0.22, d * 0.3, -d * 0.3), v3(1.0, 1.0, 1.0), C);
    g.handle(Bounce);
}

pub(super) fn anvil(g: &mut G) {
    let l = g.l;
    let w = odd(l * 0.36);
    let b = &mut g.b;
    let top_y = l * 0.2;
    let base_y = -l * 0.3;
    // Foot, waist, face, horn, heel.
    b.bx(v3(0.0, base_y + 0.5, 0.0), v3(w + 2.0, 2.0, l * 0.62), D);
    b.bx(v3(0.0, base_y + 2.0, 0.0), v3(w, 2.0, l * 0.5), D);
    b.bx(v3(0.0, (base_y + top_y) * 0.5 + 0.5, 0.0), v3(odd(w * 0.6), top_y - base_y - 2.0, l * 0.34), P);
    b.bx(v3(0.0, top_y, l * 0.04), v3(w, l * 0.16, l * 0.62), P);
    b.bx(v3(0.0, top_y + l * 0.08 - 0.5, l * 0.04), v3(w, 1.0, l * 0.62), A);
    b.bx(v3(0.0, top_y + l * 0.08 - 0.5, l * 0.25), v3(1.0, 1.0, 1.0), D);
    b.cone_to(v3(0.0, top_y + l * 0.02, -l * 0.26), v3(0.0, top_y + l * 0.05, -l * 0.62), l * 0.2, P);
    b.bx(v3(0.0, top_y - l * 0.02, l * 0.39), v3(odd(w * 0.7), l * 0.1, l * 0.12), P);
    b.bx(v3(0.0, (base_y + top_y) * 0.5, -l * 0.17), v3(1.0, 1.0, 1.0), C);
    g.handle(DropFromSky);
    g.handle(Heavy);
    g.handle(Blunt);
}

pub(super) fn bomb(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3); // 0 cartoon, 1 dynamite bundle, 2 sea mine
    let b = &mut g.b;
    let top;
    match variant {
        1 => {
            let d = odd(l * 0.3);
            for (x, z) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                b.cy(v3(x * d * 0.5, 0.0, z * d * 0.5), d, l * 0.8, P);
            }
            b.cy(Vec3::ZERO, d * 2.2, 1.0, D);
            b.cy(v3(0.0, l * 0.25, 0.0), d * 2.2, 1.0, A);
            top = v3(0.0, l * 0.4, 0.0);
        }
        2 => {
            let d = odd(l * 0.7);
            b.el(Vec3::ZERO, Vec3::splat(d), P);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let dir = v3(a.cos(), if i % 2 == 0 { 0.4 } else { -0.4 }, a.sin()).normalize();
                b.cone_to(dir * d * 0.4, dir * (d * 0.5 + 2.0), 2.0, C);
            }
            b.cone_to(v3(0.0, d * 0.4, 0.0), v3(0.0, d * 0.5 + 2.0, 0.0), 2.0, C);
            top = v3(0.0, d * 0.5 + 1.5, 0.0);
            let _ = top;
            g.handle(Explodes);
            g.handle(Pierce);
            return;
        }
        _ => {
            let d = odd(l * 0.75);
            b.el(v3(0.0, -l * 0.08, 0.0), Vec3::splat(d), P);
            b.el(v3(-d * 0.2, d * 0.15, -d * 0.3), v3(2.0, 1.0, 1.0), A);
            b.cy(v3(0.0, d * 0.42, 0.0), odd(d * 0.4), 2.0, D);
            b.cy(v3(0.0, d * 0.52, 0.0), odd(d * 0.4) + 1.0, 1.0, A);
            top = v3(0.0, d * 0.55, 0.0);
        }
    }
    // Fuse (curly) + spark.
    let p1 = top + v3(0.0, l * 0.12, 0.3);
    let p2 = p1 + v3(l * 0.08, l * 0.08, 0.3);
    b.seg(PrimKind::Box, top, p1, 1.0, 1.0, D);
    b.seg(PrimKind::Box, p1, p2, 1.0, 1.0, D);
    b.bx(p2 + v3(0.5, 0.5, 0.0), v3(3.0, 1.0, 1.0), C);
    b.bx(p2 + v3(0.5, 0.5, 0.0), v3(1.0, 3.0, 1.0), C);
    b.bx(p2 + v3(0.5, 0.5, 0.0), v3(1.0, 1.0, 3.0), C);
    g.handle(Explodes);
}

pub(super) fn boulder(g: &mut G) {
    let l = g.l;
    let b = &mut g.b;
    b.el(Vec3::ZERO, v3(l, l * 0.85, l * 0.95), P);
    let n = 3 + g.r.pick(3);
    for _ in 0..n {
        let dir = v3(g.r.range(-1.0, 1.0), g.r.range(-0.6, 1.0), g.r.range(-1.0, 1.0)).normalize_or(Vec3::Y);
        let s = l * g.r.range(0.4, 0.62);
        g.b.elr(dir * l * 0.3, v3(s, s * g.r.range(0.7, 1.0), s), v3(g.r.range(0.0, 90.0), g.r.range(0.0, 90.0), 0.0), P);
    }
    // Facet patches + glowing cracks.
    for _ in 0..4 {
        let dir = v3(g.r.range(-1.0, 1.0), g.r.range(-0.3, 1.0), g.r.range(-1.0, 1.0)).normalize_or(Vec3::Y);
        let s = l * 0.3;
        g.b.el(dir * l * 0.42, v3(s, s * 0.6, s), D);
    }
    for _ in 0..2 {
        let dir = v3(g.r.range(-1.0, 1.0), g.r.range(0.0, 1.0), g.r.range(-1.0, -0.2)).normalize_or(Vec3::NEG_Z);
        let side = dir.cross(Vec3::Y).normalize_or(Vec3::X);
        g.b.seg(PrimKind::Box, dir * l * 0.45 - side * l * 0.2, dir * l * 0.45 + side * l * 0.15 + Vec3::Y * l * 0.1, 1.0, 1.0, C);
    }
    for _ in 0..2 {
        let dir = v3(g.r.range(-1.0, 1.0), g.r.range(-0.5, 0.8), g.r.range(-1.0, 1.0)).normalize_or(Vec3::X);
        g.b.cut_el(dir * l * 0.55, Vec3::splat(l * 0.22));
    }
    g.handle(Heavy);
}

pub(super) fn rocket(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3); // 0 missile, 1 cartoon rocket, 2 firework
    let d = odd(l * match variant {
        1 => 0.3,
        2 => 0.16,
        _ => 0.22,
    })
    .max(1.0);
    let zf = -l * 0.5;
    let zb = l * 0.38;
    let homing = g.take(Homing);
    let b = &mut g.b;
    let nose = (d * 1.6).max(2.0);
    b.cz(v3(0.0, 0.0, (zf + nose + zb) * 0.5), d, zb - zf - nose, P);
    b.cone_to(v3(0.0, 0.0, zf + nose), v3(0.0, 0.0, zf - 0.5), d, A);
    b.cz(v3(0.0, 0.0, zf + nose + 1.0), d + 1.0, 1.0, A);
    if variant == 1 {
        b.cx(v3(0.0, 0.0, zf + nose + l * 0.12), 3.0, d + 1.0, C);
    }
    // Fins.
    let fl = (l * if variant == 1 { 0.28 } else { 0.18 }).max(2.0);
    let n = if variant == 1 { 3 } else { 4 };
    for i in 0..n {
        let a = i as f32 * std::f32::consts::TAU / n as f32 + std::f32::consts::FRAC_PI_4 * (n == 4) as i32 as f32;
        let dir = v3(a.cos(), a.sin(), 0.0);
        let root = dir * (d * 0.5) + v3(0.0, 0.0, zb - fl * 0.4);
        b.flat(PrimKind::Box, root, root + dir * fl * 0.8 + v3(0.0, 0.0, fl * 0.5), fl * 0.6, 1.0, v3(-dir.y, dir.x, 0.0), D);
    }
    if variant == 2 {
        b.seg(PrimKind::Box, v3(d * 0.5 + 0.5, 0.0, zb), v3(d * 0.5 + 0.5, 0.0, zb + l * 0.5), 1.0, 1.0, D);
    }
    if homing {
        b.bx(v3(0.0, 0.0, zf - 0.5), v3(1.0, 1.0, 1.0), C);
        b.cz(v3(0.0, 0.0, zf + nose * 0.5), d + 1.0, 1.0, C);
    }
    exhaust(&mut g.b, 0, v3(0.0, 0.0, zb), Vec3::Z, (d * 0.4).max(1.0));
    g.b.on(0);
    g.handle(Thrust);
    g.handle(Explodes);
    g.handle(Flies);
}

pub(super) fn vehicle(g: &mut G) {
    crate::features::vehicles::vehicle(g);
}

pub(super) fn blob0(g: &mut G) {
    let l = g.l;
    let b = &mut g.b;
    b.el(Vec3::ZERO, v3(l, l * 0.85, l), P);
    let n = 3 + g.r.pick(2);
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU + g.r.range(0.0, 1.0);
        let dir = v3(a.cos(), g.r.range(-0.3, 0.6), a.sin()).normalize();
        let s = l * g.r.range(0.45, 0.6);
        g.b.el(dir * l * 0.3, Vec3::splat(s), if i % 2 == 0 { P } else { A });
    }
    g.b.el(v3(0.0, l * 0.2, -l * 0.4), v3(2.0, 2.0, 1.0), C);
    g.b.el(v3(l * 0.25, -l * 0.1, -l * 0.35), v3(1.0, 1.0, 1.0), C);
}
