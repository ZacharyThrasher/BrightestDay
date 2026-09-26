//! Structures. Origin = base centre on the ground, front face toward −Z (the caster).

use bd_core::mechanics::Mechanic::*;
use bd_core::shape::PrimKind;
use glam::{Quat, Vec3};

use super::G;
use crate::build::{A, C, D, P, odd, v3};
use crate::parts::{basis_quat, spring};

pub(super) fn wall(g: &mut G) {
    let w = odd(g.l);
    let h = (g.l * g.r.range(0.55, 0.7)).round().max(4.0);
    let t = odd((g.l * 0.09).max(2.0));
    let variant = g.r.pick(3); // 0 castle, 1 energy barrier, 2 palisade
    let b = &mut g.b;
    match variant {
        0 => {
            b.bx(v3(0.0, h * 0.5 - 0.5, 0.0), v3(w, h, t), P);
            // Mortar rows + staggered joints (paint-by-overlap on the same outline).
            let mut row = 0;
            let mut y = 2.0;
            while y < h - 1.0 {
                b.bx(v3(0.0, y, 0.0), v3(w, 1.0, t), D);
                let off = if row % 2 == 0 { 0.0 } else { 2.0 };
                let mut x = -w * 0.5 + off + 1.0;
                while x < w * 0.5 {
                    b.bx(v3(x.round(), y - 1.0, 0.0), v3(1.0, 1.0, t), D);
                    x += 4.0;
                }
                y += 3.0;
                row += 1;
            }
            // Crenellations.
            b.bx(v3(0.0, h - 0.5, 0.0), v3(w + 2.0, 1.0, t + 2.0), A);
            let n = ((w / 4.0) as i32).max(2);
            for i in 0..n {
                let x = -w * 0.5 + 1.5 + i as f32 * (w - 3.0) / (n - 1).max(1) as f32;
                b.bx(v3(x.round(), h + 1.0, 0.0), v3(3.0, 2.0, t + 2.0), P);
            }
            b.bx(v3(0.0, h * 0.5, -t * 0.5), v3(3.0, 1.0, 1.0), C);
        }
        1 => {
            b.bx(v3(0.0, h * 0.5 - 0.5, 0.0), v3(w, h, t), P);
            let panels = ((w / 5.0) as i32).max(2);
            for i in 0..=panels {
                let x = -w * 0.5 + 0.5 + i as f32 * (w - 1.0) / panels as f32;
                b.bx(v3(x.round(), h * 0.5 - 0.5, 0.0), v3(1.0, h, t + 1.0), D);
            }
            b.bx(v3(0.0, h - 0.5, 0.0), v3(w, 1.0, t + 1.0), C);
            b.bx(v3(0.0, 0.0, 0.0), v3(w + 2.0, 1.0, t + 2.0), D);
            b.bx(v3(0.0, h * 0.5, 0.0), v3(w, 1.0, t + 1.0), A);
        }
        _ => {
            let n = ((w / 2.0) as i32).max(3);
            for i in 0..n {
                let x = -w * 0.5 + 1.0 + i as f32 * 2.0;
                let hh = h - (i % 2) as f32;
                b.cy(v3(x, hh * 0.5 - 0.5, 0.0), 2.0, hh, if i % 2 == 0 { P } else { A });
                b.cone_to(v3(x, hh - 0.5, 0.0), v3(x, hh + 2.0, 0.0), 2.0, P);
            }
            b.bx(v3(0.0, h * 0.3, -1.0), v3(w, 1.0, 1.0), D);
            b.bx(v3(0.0, h * 0.7, -1.0), v3(w, 1.0, 1.0), D);
            b.bx(v3(0.0, h * 0.5, -1.5), v3(1.0, 1.0, 1.0), C);
        }
    }
    g.handle(Barrier);
}

pub(super) fn ramp(g: &mut G) {
    let len = g.l;
    let w = odd(len * g.r.range(0.5, 0.65));
    let h = (len * g.r.range(0.32, 0.45)).max(3.0);
    let curved = g.r.chance(0.35);
    let b = &mut g.b;
    // Rises toward +Z (away from the caster): run up it and launch.
    b.bx(v3(0.0, h * 0.5 - 0.5, 0.0), v3(w, h, len), P);
    let alpha = (h / len).atan();
    if curved {
        // Quarter-pipe: carve with a cylinder along X.
        let r = len * 1.05;
        b.cut(PrimKind::Cylinder, v3(0.0, r - 0.5, -len * 0.5), v3(r * 2.0, w + 4.0, r * 2.0), v3(0.0, 0.0, 90.0));
    } else {
        let nrm = v3(0.0, alpha.cos(), -alpha.sin());
        let big = len * 2.0;
        let c = v3(0.0, h * 0.5 - 0.5, 0.0) + nrm * (big * 0.5);
        b.cut_bxr(c, v3(w + 4.0, big, big * 2.0), v3(-alpha.to_degrees(), 0.0, 0.0));
    }
    // Chevrons (Core) on the running surface, pointing up-slope.
    let n = 3;
    for i in 0..n {
        let f = 0.25 + i as f32 * 0.22;
        let z = -len * 0.5 + len * f;
        let y = if curved {
            let r = len * 1.05;
            let dz = z + len * 0.5;
            r - 0.5 - (r * r - dz * dz).max(0.0).sqrt()
        } else {
            (z + len * 0.5) * (h / len) - 0.5
        };
        b.sym(|b, s| {
            let a = v3(s * w * 0.3, y + 0.6, z - len * 0.06);
            let c2 = v3(0.0, y + 0.6 + len * 0.05, z + len * 0.06);
            b.seg(PrimKind::Box, a, c2, 1.0, 1.0, C);
        });
    }
    // Rails + lip.
    b.sym(|b, s| {
        b.bxr(v3(s * (w * 0.5 + 0.5), h * 0.5 - 0.5, 0.0), v3(1.0, 1.0, len * 1.02 / alpha.cos()), v3(-alpha.to_degrees(), 0.0, 0.0), D);
    });
    b.bx(v3(0.0, 0.0, 0.0), v3(w + 2.0, 1.0, len), D);
    b.bx(v3(0.0, h - 0.5, len * 0.5 - 0.5), v3(w, 1.0, 1.0), A);
    g.handle(Launch);
}

pub(super) fn pillar(g: &mut G) {
    let h = g.l;
    let variant = g.r.pick(3); // 0 column, 1 obelisk, 2 crystal spire
    let d = odd((h * g.r.range(0.14, 0.2)).max(3.0));
    let b = &mut g.b;
    match variant {
        0 => {
            b.bx(v3(0.0, 0.5, 0.0), v3(d + 4.0, 2.0, d + 4.0), D);
            b.bx(v3(0.0, 2.0, 0.0), v3(d + 2.0, 1.0, d + 2.0), A);
            b.cy(v3(0.0, h * 0.5, 0.0), d, h - 4.0, P);
            for i in 0..4 {
                let a = i as f32 * std::f32::consts::FRAC_PI_2 + 0.785;
                b.cy(v3(a.cos() * d * 0.45, h * 0.5, a.sin() * d * 0.45), 1.0, h - 5.0, A);
            }
            b.bx(v3(0.0, h - 1.5, 0.0), v3(d + 2.0, 1.0, d + 2.0), A);
            b.bx(v3(0.0, h - 0.5, 0.0), v3(d + 4.0, 1.0, d + 4.0), P);
            b.cy(v3(0.0, h + 0.5, 0.0), d - 1.0, 1.0, C);
        }
        1 => {
            b.bx(v3(0.0, 0.5, 0.0), v3(d + 4.0, 2.0, d + 4.0), D);
            b.cor(v3(0.0, h * 0.45, 0.0), v3(d * 1.35, h * 1.3, d * 1.35), v3(0.0, 45.0, 0.0), P);
            b.cut_bx(v3(0.0, h * 0.85 + 50.0, 0.0), v3(100.0, 100.0, 100.0));
            b.cor(v3(0.0, h * 0.85 + d * 0.4, 0.0), v3(d * 0.9, d * 0.8, d * 0.9), v3(0.0, 45.0, 0.0), C);
            b.bx(v3(0.0, h * 0.4, 0.0), v3(d + 1.0, 1.0, d + 1.0), A);
        }
        _ => {
            b.cy(v3(0.0, 1.0, 0.0), d + 3.0, 2.0, D);
            b.cor(v3(0.0, h * 0.5, 0.0), v3(d, h, d), v3(0.0, 0.0, 0.0), P);
            for i in 0..3 {
                let a = i as f32 * std::f32::consts::TAU / 3.0;
                let base = v3(a.cos() * d * 0.5, 0.5, a.sin() * d * 0.5);
                b.cone_to(base, base + v3(a.cos() * d * 0.4, h * 0.4, a.sin() * d * 0.4), d * 0.5, A);
            }
            b.cor(v3(0.0, h * 0.85, 0.0), v3(d * 0.45, h * 0.3, d * 0.45), v3(0.0, 0.0, 0.0), C);
        }
    }
}

pub(super) fn dome(g: &mut G) {
    let dd = odd(g.l);
    let r = dd * 0.5;
    let hr = r * g.r.range(0.85, 1.1);
    let ribs = 2 + g.r.pick(3) as usize;
    let b = &mut g.b;
    b.el(v3(0.0, 0.0, 0.0), v3(dd, hr * 2.0, dd), P);
    b.cut_el(v3(0.0, 0.0, 0.0), v3(dd - 3.0, hr * 2.0 - 3.0, dd - 3.0));
    // Ribs (meridians) — their inner cuts only hit the hollow.
    for i in 0..ribs {
        let a = i as f32 * 180.0 / ribs as f32;
        let q = Quat::from_rotation_y(a.to_radians()) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        b.q(PrimKind::Cylinder, v3(0.0, 0.0, 0.0), v3(dd + 1.0, 1.0, hr * 2.0 + 1.0), q, A);
    }
    b.cy(v3(0.0, hr * 0.5, 0.0), dd * 0.87 + 1.0, 1.0, A);
    b.cut_el(v3(0.0, 0.0, 0.0), v3(dd - 2.2, hr * 2.0 - 2.2, dd - 2.2));
    b.cut_bx(v3(0.0, -50.0, 0.0), v3(300.0, 100.0, 300.0));
    b.cy(v3(0.0, 0.5, 0.0), dd + 2.0, 1.0, D);
    b.cut(PrimKind::Cylinder, v3(0.0, 0.5, 0.0), v3(dd - 3.0, 3.0, dd - 3.0), Vec3::ZERO);
    b.el(v3(0.0, hr, 0.0), v3(3.0, 3.0, 3.0), C);
    g.handle(Barrier);
    g.handle(Guard);
}

pub(super) fn cage(g: &mut G) {
    let h = g.l;
    let bird = g.r.chance(0.5);
    let b = &mut g.b;
    if bird {
        let r = odd(h * 0.75) * 0.5;
        let n = 10;
        b.cy(v3(0.0, 0.5, 0.0), r * 2.0 + 1.0, 1.0, D);
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let x = a.cos() * r;
            let z = a.sin() * r;
            b.seg(PrimKind::Box, v3(x, 0.5, z), v3(x, h * 0.7, z), 1.0, 1.0, P);
            b.seg(PrimKind::Box, v3(x, h * 0.7, z), v3(x * 0.15, h * 0.95, z * 0.15), 1.0, 1.0, P);
        }
        b.cy(v3(0.0, h * 0.35, 0.0), r * 2.0 + 1.0, 1.0, A);
        b.cut(PrimKind::Cylinder, v3(0.0, h * 0.35, 0.0), v3(r * 2.0 - 1.0, 3.0, r * 2.0 - 1.0), Vec3::ZERO);
        b.cy(v3(0.0, h * 0.7, 0.0), r * 2.0 + 1.0, 1.0, A);
        b.cut(PrimKind::Cylinder, v3(0.0, h * 0.7, 0.0), v3(r * 2.0 - 1.0, 3.0, r * 2.0 - 1.0), Vec3::ZERO);
        b.cx(v3(0.0, h + 0.5, 0.0), 3.0, 1.0, A);
        b.cut(PrimKind::Cylinder, v3(0.0, h + 0.5, 0.0), v3(1.0, 3.0, 1.0), v3(0.0, 0.0, 90.0));
        b.bx(v3(0.0, h * 0.95, 0.0), v3(1.0, 1.0, 1.0), C);
    } else {
        let w = odd(h * 0.8);
        let hw = w * 0.5;
        b.bx(v3(0.0, 0.5, 0.0), v3(w + 2.0, 1.0, w + 2.0), D);
        b.bx(v3(0.0, h - 0.5, 0.0), v3(w + 2.0, 1.0, w + 2.0), A);
        b.cut_bx(v3(0.0, h - 0.5, 0.0), v3(w - 2.0, 3.0, w - 2.0));
        let n = ((w / 2.0) as i32).max(2);
        for i in 0..=n {
            let t = -hw + i as f32 * w / n as f32;
            for (x, z) in [(t, -hw), (t, hw), (-hw, t), (hw, t)] {
                b.bx(v3(x.round(), h * 0.5, z.round()), v3(1.0, h - 1.0, 1.0), if i == 0 || i == n { D } else { P });
            }
        }
        b.bx(v3(0.0, h * 0.5, -hw), v3(3.0, 3.0, 1.0), C);
    }
    g.handle(Trap);
}

pub(super) fn platform(g: &mut G) {
    let w = odd(g.l);
    let dpt = odd(g.l * 0.8);
    let variant = g.r.pick(3); // 0 floating island, 1 hover disc, 2 tech pad
    let b = &mut g.b;
    match variant {
        1 => {
            b.cy(v3(0.0, -0.5, 0.0), w, 2.0, D);
            b.cy(v3(0.0, 0.0, 0.0), w - 2.0, 1.0, P);
            b.cy(v3(0.0, -1.5, 0.0), w * 0.6, 1.0, C);
            b.cy(v3(0.0, -1.5, 0.0), w * 0.6 - 2.0, 1.0, D);
        }
        2 => {
            b.bx(v3(0.0, -0.5, 0.0), v3(w, 2.0, dpt), D);
            b.bx(v3(0.0, 0.0, 0.0), v3(w - 2.0, 1.0, dpt - 2.0), P);
            b.sym(|b, s| {
                for zs in [-1.0f32, 1.0] {
                    b.cone_to(v3(s * (w * 0.5 - 1.0), -1.5, zs * (dpt * 0.5 - 1.0)), v3(s * (w * 0.5 - 1.0), -3.0, zs * (dpt * 0.5 - 1.0)), 2.0, C);
                }
            });
            b.bx(v3(0.0, 0.0, 0.0), v3(w - 2.0, 1.0, 1.0), A);
        }
        _ => {
            b.bx(v3(0.0, -0.5, 0.0), v3(w, 2.0, dpt), D);
            b.bx(v3(0.0, 0.0, 0.0), v3(w - 2.0, 1.0, dpt - 2.0), P);
            let depth = g.l * 0.45;
            b.cor(v3(0.0, -1.5 - depth * 0.5, 0.0), v3(w * 0.9, depth, dpt * 0.9), v3(180.0, 0.0, 0.0), A);
            b.cor(v3(0.0, -1.5 - depth * 0.85, 0.0), v3(w * 0.25, depth * 0.3, dpt * 0.25), v3(180.0, 0.0, 0.0), C);
        }
    }
}

pub(super) fn trampoline(g: &mut G) {
    let dd = odd(g.l);
    let r = dd * 0.5;
    let hgt = (g.l * 0.25).max(3.0).round();
    let b = &mut g.b;
    // Legs.
    for i in 0..4 {
        let a = i as f32 * std::f32::consts::FRAC_PI_2 + 0.785;
        let top = v3(a.cos() * (r - 1.0), hgt - 1.0, a.sin() * (r - 1.0));
        b.seg(PrimKind::Box, v3(top.x * 1.1, 0.0, top.z * 1.1), top, 1.0, 1.0, D);
    }
    // Frame ring.
    b.cy(v3(0.0, hgt, 0.0), dd, 2.0, A);
    b.cut(PrimKind::Cylinder, v3(0.0, hgt, 0.0), v3(dd - 3.0, 4.0, dd - 3.0), Vec3::ZERO);
    // Mat + bullseye (paint by overlap) + springs.
    let md = dd - 5.0;
    b.cy(v3(0.0, hgt - 0.5, 0.0), md, 1.0, P);
    b.cy(v3(0.0, hgt - 0.5, 0.0), (md * 0.55).round(), 1.0, C);
    b.cy(v3(0.0, hgt - 0.5, 0.0), (md * 0.55).round() - 2.0, 1.0, P);
    b.cy(v3(0.0, hgt - 0.5, 0.0), 1.0, 1.0, C);
    let n = 12;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let d = v3(a.cos(), 0.0, a.sin());
        b.seg(PrimKind::Box, d * (md * 0.5 - 0.3) + v3(0.0, hgt - 0.5, 0.0), d * (r - 1.2) + v3(0.0, hgt - 0.5, 0.0), 1.0, 1.0, C);
    }
    let _ = (Quat::IDENTITY, basis_quat(Vec3::Y, Vec3::Z));
    g.handle(Bounce);
    let _ = spring;
}
