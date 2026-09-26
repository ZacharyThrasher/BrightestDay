//! Held items. Origin = grip (the hand), item points −Z, **strike direction = −Y** (edges, hammer faces
//! and axe bits face −Y so every swing curve can lead with them).

use bd_core::mechanics::Mechanic::*;
use bd_core::shape::PrimKind;
use glam::{Quat, Vec3};

use super::G;
use crate::build::{A, C, D, P, odd, v3};
use crate::parts::exhaust;

pub(super) fn hammer(g: &mut G) {
    let l = g.l;
    let heavy = g.take(Blunt) | g.take(Heavy);
    let k = if heavy { 1.2 } else { 1.0 };
    // Sledge or maul; a war hammer (spike + point) when the recipe pierces.
    let variant = if g.has(Pierce) { 1 } else if g.r.pick(2) == 0 { 0 } else { 2 };
    let hd = odd(l * 0.09);
    let behind = (l * 0.13).max(1.5);
    let reach = l - behind;
    let head_y = (l * g.r.range(0.5, 0.64) * k).max(4.0);
    let head_z = odd((l * g.r.range(0.32, 0.42) * k).max(3.0));
    let head_x = odd((head_z * g.r.range(0.95, 1.3)).max(3.0));
    let hz = -(reach - head_z * 0.5);
    let b = &mut g.b;
    // Handle, grip, pommel.
    let h_end = hz + head_z * 0.5;
    b.cz(v3(0.0, 0.0, (behind + h_end) * 0.5), hd, behind - h_end, P);
    b.cz(v3(0.0, 0.0, (behind - l * 0.14) * 0.5), hd, behind + l * 0.14, D);
    b.bx(v3(0.0, 0.0, behind), v3(hd + 2.0, hd + 2.0, 1.0), A);
    match variant {
        0 => {
            // Sledge: block with flared banded ends and a glowing strike face.
            b.bx(v3(0.0, 0.0, hz), v3(head_x, head_y, head_z), P);
            b.sym(|b, s| {
                b.bx(v3(0.0, s * (head_y * 0.5 - 1.0), hz), v3(head_x + 2.0, 1.0, head_z + 2.0), A);
            });
            b.bx(v3(0.0, -head_y * 0.5 + 0.5, hz), v3(head_x - 2.0, 1.0, head_z - 2.0), C);
        }
        1 => {
            // War hammer: heavy striking block (−Y), back spike (+Y), top point.
            let blk = head_y * 0.62;
            b.bx(v3(0.0, -head_y * 0.5 + blk * 0.5, hz), v3(head_x, blk, head_z), P);
            b.bx(v3(0.0, -head_y * 0.5 + 1.0, hz), v3(head_x + 2.0, 1.0, head_z + 2.0), A);
            b.bx(v3(0.0, -head_y * 0.5 + 0.5, hz), v3(head_x - 2.0, 1.0, head_z - 2.0), C);
            let sb = -head_y * 0.5 + blk;
            b.cone_to(v3(0.0, sb - 0.5, hz), v3(0.0, sb + head_y * 0.55, hz + head_z * 0.35), head_z, A);
            b.cone_to(v3(0.0, sb + head_y * 0.25, hz + head_z * 0.15), v3(0.0, sb + head_y * 0.55, hz + head_z * 0.35), head_z * 0.5, C);
            b.cone_to(v3(0.0, 0.0, hz - head_z * 0.5 + 0.5), v3(0.0, 0.0, hz - head_z * 0.5 - l * 0.14), head_z * 0.8, A);
        }
        _ => {
            // Mallet / maul: fat cylinder with caps.
            let d = head_z + 2.0;
            b.cy(v3(0.0, 0.0, hz), d, head_y, P);
            b.sym(|b, s| {
                b.cy(v3(0.0, s * (head_y * 0.5 - 0.5), hz), d + 2.0, 1.0, A);
            });
            b.cy(v3(0.0, -head_y * 0.5 + 0.5, hz), d - 2.0, 1.0, C);
            b.cy(v3(0.0, 0.0, hz), d + 1.0, 1.0, D);
        }
    }
    b.bx(v3(0.0, 0.0, h_end + 0.5), v3(hd + 2.0, hd + 2.0, 1.0), A);
    let top = head_y * 0.5;
    if g.take(Thrust) {
        // Rocket hammer: exhaust on the back face (+Y) drives the strike (−Y).
        let r = (head_z * 0.3).max(1.0);
        exhaust(&mut g.b, 0, v3(0.0, top + 0.5, hz), Vec3::Y, r);
    }
    g.handle(Pierce);
    if g.take(Blade) {
        g.b.bx(v3(0.0, top + l * 0.06, hz), v3(1.0, l * 0.14, head_z + 2.0), C);
    }
    if g.take(Guard) {
        g.b.cz(v3(0.0, 0.0, -l * 0.16), (l * 0.3).max(3.0), 1.0, A);
    }
}

pub(super) fn sword(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(4);
    let (grip_f, w, guard_w) = match variant {
        0 => (0.2, 3.0, 5.0 + g.r.range(0.0, 2.0).round() * 2.0),
        1 => (0.26, 2.0, 0.0),
        2 => (0.24, 3.0, 9.0 + g.r.range(0.0, 2.0).round() * 2.0),
        _ => (0.19, 3.0, 5.0),
    };
    let grip = (l * grip_f).max(2.0);
    let zb = grip * 0.6;
    let zg = -grip * 0.4 - 0.5;
    let blen = l * g.r.range(0.95, 1.05) - grip;
    let ztip = zg - blen;
    let b = &mut g.b;
    // Grip + pommel.
    b.cz(v3(0.0, 0.0, (zb + zg) * 0.5), 1.0, zb - zg, D);
    if variant == 1 {
        b.bx(v3(0.0, 0.0, zb + 0.5), v3(1.0, 1.0, 1.0), A);
    } else {
        b.bx(v3(0.0, 0.0, zb + 0.5), v3(3.0, 3.0, 1.0), A);
    }
    match variant {
        1 | 3 => {
            // Curved blade (katana / scimitar): segments bending toward +Y (spine), edge at −Y.
            let n = 5;
            let curve = if variant == 1 { 0.09 } else { 0.16 };
            let pts: Vec<Vec3> = (0..=n)
                .map(|i| {
                    let f = i as f32 / n as f32;
                    v3(0.0, curve * blen * f * f, zg - 0.5 - blen * f)
                })
                .collect();
            for i in 0..n {
                let (a, c) = (pts[i], pts[i + 1]);
                let d = (c - a).normalize();
                let nrm = Vec3::X.cross(d).normalize();
                let wi = if variant == 3 { w + (i as f32 / n as f32 * 2.0).round() } else { w };
                let off = nrm * ((wi - 1.0) * 0.5);
                b.segx(PrimKind::Box, a + off * 0.0, c + off * 0.0, 1.0, 0.4, P);
                b.seg(PrimKind::Box, a - d * 0.4, c + d * 0.4, 1.0, wi, P);
                b.seg(PrimKind::Box, a - off - d * 0.4, c - off + d * 0.4, 1.0, 1.0, C);
            }
            let tip = pts[n];
            let d = (pts[n] - pts[n - 1]).normalize();
            b.cone_to(tip - d * 0.5, tip + d * (w + 1.0) + Vec3::NEG_Y * 0.0, w + 0.5, C);
            if variant == 1 {
                b.cz(v3(0.0, 0.0, zg), w + 3.0, 1.0, D);
                b.cz(v3(0.0, 0.0, zg), w + 1.0, 1.0, A);
            } else {
                b.bx(v3(0.0, 0.0, zg), v3(3.0, guard_w, 1.0), A);
                b.bx(v3(0.0, -guard_w * 0.5, zg + 1.0), v3(1.0, 1.0, 2.0), A);
            }
        }
        _ => {
            // Straight double-edged blade with a glowing edge on both sides and a diamond tip.
            let base_w = if variant == 2 { w + 2.0 } else { w };
            b.bx(v3(0.0, 0.0, zg - 0.5 - blen * 0.5), v3(1.0, w, blen), P);
            if variant == 2 {
                b.bx(v3(0.0, 0.0, zg - 0.5 - blen * 0.12), v3(1.0, base_w, blen * 0.24), P);
            }
            b.sym(|b, s| {
                b.bx(v3(0.0, s * (w - 1.0) * 0.5, zg - 0.5 - blen * 0.5), v3(1.0, 1.0, blen), C);
            });
            b.bxr(v3(0.0, 0.0, ztip - 0.5), v3(1.0, w * 0.75, w * 0.75), v3(45.0, 0.0, 0.0), C);
            // Cross-guard.
            b.bx(v3(0.0, 0.0, zg), v3(3.0, w + guard_w, 1.0), A);
            if variant == 2 {
                b.sym(|b, s| {
                    let y0 = s * (w + guard_w) * 0.5;
                    b.segx(PrimKind::Box, v3(0.0, y0, zg), v3(0.0, y0 + s * 1.5, zg - 2.5), 1.5, 0.3, A);
                });
            }
            b.bx(v3(0.0, 0.0, zg), v3(3.0, 1.0, 1.0), C);
        }
    }
    g.handle(Blade);
    if g.take(Pierce) {
        g.b.cone_to(v3(0.0, 0.0, ztip), v3(0.0, 0.0, ztip - l * 0.15), w, C);
    }
    if g.take(Guard) {
        g.b.el(v3(0.0, 0.0, zg + grip * 0.35), v3(5.0, 5.0, grip * 0.9), A);
        g.b.cut_el(v3(0.0, 0.0, zg + grip * 0.35), v3(3.0, 3.0, grip * 0.9 + 1.0));
        g.b.cz(v3(0.0, 0.0, (zb + zg) * 0.5), 1.0, zb - zg, D);
    }
}

pub(super) fn axe(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3);
    let behind = (l * 0.12).max(1.5);
    let reach = l - behind;
    let hw = odd((l * g.r.range(0.2, 0.28)).max(3.0));
    let h = (l * g.r.range(0.38, 0.5)).max(4.0);
    let w = (l * g.r.range(0.55, 0.75)).max(5.0);
    let zh = -reach + hw * 0.5 + 0.5;
    let b = &mut g.b;
    b.cz(v3(0.0, 0.0, (behind - reach) * 0.5), 1.0, behind + reach, P);
    b.cz(v3(0.0, 0.0, (behind - l * 0.15) * 0.5), 1.0, behind + l * 0.15, D);
    b.bx(v3(0.0, 0.0, behind + 0.5), v3(3.0, 3.0, 1.0), A);
    let bit = |b: &mut crate::build::B, s: f32, hh: f32, ww: f32| {
        // Fan widening away from the haft, straight glowing edge with a convex bulge.
        b.el(v3(0.0, s * hh, zh), v3(1.0, hh * 2.0, ww), P);
        b.cut_bx(v3(0.0, s * (hh + 50.0), zh), v3(ww + 8.0, 100.0, ww + 8.0));
        b.el(v3(0.0, s * hh, zh), v3(1.0, (hh * 0.3).max(1.6), ww + 1.0), C);
    };
    bit(b, -1.0, h, w);
    match variant {
        0 => {}
        1 => bit(b, 1.0, h * 0.8, w * 0.85),
        _ => {
            b.cone_to(v3(0.0, 0.5, zh), v3(0.0, h * 0.7, zh + 1.0), hw * 0.8, A);
        }
    }
    b.bx(v3(0.0, 0.0, zh), v3(3.0, 3.0, hw), D);
    b.bx(v3(0.0, 0.0, zh - hw * 0.5 - 0.5), v3(1.0, 1.0, 1.0), A);
    g.handle(Blade);
    if g.take(Pierce) {
        g.b.cone_to(v3(0.0, 0.0, zh - hw * 0.5), v3(0.0, 0.0, zh - hw * 0.5 - l * 0.18), 3.0, C);
    }
    if g.take(Guard) {
        g.b.cz(v3(0.0, 0.0, -l * 0.15), 5.0, 1.0, A);
    }
}

pub(super) fn spear(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3);
    let zb = l * 0.38;
    let zt = -l * 0.62;
    let hl = (l * g.r.range(0.22, 0.3)).max(4.0);
    let zc = zt + hl; // collar
    let b = &mut g.b;
    b.cz(v3(0.0, 0.0, (zb + zc) * 0.5), 1.0, zb - zc, P);
    b.cz(v3(0.0, 0.0, 0.0), 1.0, l * 0.14, D);
    b.cone_to(v3(0.0, 0.0, zb - 1.0), v3(0.0, 0.0, zb + 1.5), 2.0, D);
    b.cz(v3(0.0, 0.0, zc + 0.5), 3.0, 1.5, A);
    b.bx(v3(0.0, -1.5, zc + 1.0), v3(1.0, 2.0, 1.0), A);
    match variant {
        0 => {
            // Leaf blade.
            b.el(v3(0.0, 0.0, zt + hl * 0.5), v3(1.0, (hl * 0.5).max(3.0), hl), P);
            b.el(v3(0.0, 0.0, zt + hl * 0.5), v3(1.0, 1.0, hl), A);
            b.cone_to(v3(0.0, 0.0, zt + hl * 0.4), v3(0.0, 0.0, zt - 1.0), 3.0, C);
            b.sym(|b, s| {
                b.seg(PrimKind::Box, v3(0.0, s * 1.2, zt + hl * 0.75), v3(0.0, s * 0.3, zt), 1.0, 1.0, C);
            });
        }
        1 => {
            // Winged pike: long spike + crossbar lugs.
            b.cone_to(v3(0.0, 0.0, zc), v3(0.0, 0.0, zt - 1.0), 3.0, C);
            b.bx(v3(0.0, 0.0, zc - 0.5), v3(1.0, 5.0, 1.0), A);
            b.sym(|b, s| {
                b.cone_to(v3(0.0, s * 2.0, zc - 0.5), v3(0.0, s * 3.5, zc - 2.0), 1.5, A);
            });
        }
        _ => {
            // Trident.
            let sp = (hl * 0.35).max(2.0).round();
            b.bx(v3(0.0, 0.0, zc - 0.5), v3(1.0, sp * 2.0 + 1.0, 1.0), A);
            for y in [-sp, 0.0, sp] {
                let len = if y == 0.0 { hl + 1.0 } else { hl * 0.8 };
                b.bx(v3(0.0, y, zc - 0.5 - len * 0.5), v3(1.0, 1.0, len), P);
                b.cone_to(v3(0.0, y, zc - len + 0.5), v3(0.0, y, zc - len - 1.5), 2.0, C);
            }
        }
    }
    g.handle(Pierce);
    g.handle(Blade);
}

pub(super) fn shield(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3);
    let dd = odd(l);
    let blade = g.take(Blade);
    let rim = if blade { C } else { D };
    let b = &mut g.b;
    // Arm strap behind the plate.
    b.bx(v3(0.0, 0.0, 0.0), v3(1.0, 3.0, 1.0), D);
    match variant {
        0 => {
            // Round: rim, concentric rings, boss.
            b.cz(v3(0.0, 0.0, -1.5), dd, 2.0, rim);
            b.cz(v3(0.0, 0.0, -2.5), dd - 2.0, 2.0, P);
            b.cz(v3(0.0, 0.0, -3.0), (dd * 0.68).round(), 1.0, A);
            b.cz(v3(0.0, 0.0, -3.0), (dd * 0.68).round() - 3.0, 1.0, P);
            b.el(v3(0.0, 0.0, -3.0), v3(dd * 0.3, dd * 0.3, 3.0), A);
            b.bx(v3(0.0, 0.0, -4.0), v3(1.0, 1.0, 1.0), C);
        }
        1 => {
            // Kite (heater): square top, point below, cross emblem.
            let w = odd(dd * 0.8);
            let th = dd * 0.42;
            let ty = dd * 0.5 - th * 0.5;
            let tip = -dd * 0.55;
            b.bx(v3(0.0, ty, -1.5), v3(w, th, 2.0), rim);
            b.cor(v3(0.0, (ty - th * 0.5 + tip) * 0.5, -1.5), v3(w, ty - th * 0.5 - tip, 2.0), v3(0.0, 0.0, 180.0), rim);
            b.bx(v3(0.0, ty, -2.5), v3(w - 2.0, th - 2.0, 2.0), P);
            b.cor(v3(0.0, (ty - th * 0.5 + tip) * 0.5 + 0.5, -2.5), v3(w - 2.0, ty - th * 0.5 - tip - 1.0, 2.0), v3(0.0, 0.0, 180.0), P);
            b.bx(v3(0.0, ty - th * 0.5 + 0.5, -2.5), v3(w - 2.0, 1.0, 2.0), P);
            b.bx(v3(0.0, 0.0, -3.5), v3(1.0, dd * 0.75, 1.0), C);
            b.bx(v3(0.0, ty * 0.6, -3.5), v3(w * 0.6, 1.0, 1.0), C);
        }
        _ => {
            // Tower: tall slab, rounded top, stripe.
            let w = odd(dd * 0.62);
            b.bx(v3(0.0, -dd * 0.08, -1.5), v3(w, dd * 0.84, 2.0), rim);
            b.cz(v3(0.0, dd * 0.34, -1.5), w, 2.0, rim);
            b.bx(v3(0.0, -dd * 0.08, -2.5), v3(w - 2.0, dd * 0.84 - 2.0, 2.0), P);
            b.cz(v3(0.0, dd * 0.34, -2.5), w - 2.0, 2.0, P);
            b.bx(v3(0.0, 0.0, -3.5), v3(1.0, dd * 0.8, 1.0), A);
            b.bx(v3(0.0, dd * 0.15, -3.5), v3(w - 2.0, 1.0, 1.0), C);
        }
    }
    g.handle(Guard);
    g.handle(Barrier);
    if g.take(Pierce) {
        g.b.cone_to(v3(0.0, 0.0, -3.5), v3(0.0, 0.0, -3.5 - l * 0.3), 3.0, C);
    }
}

pub(super) fn gauntlet(g: &mut G) {
    let l = g.l;
    let k = if g.take(Heavy) | g.take(Blunt) { 1.2 } else { 1.0 };
    let fw = odd(l * 0.62 * k).max(3.0);
    let fh = odd(l * 0.5 * k).max(3.0);
    let fd = (l * 0.46 * k).round().max(3.0);
    let zc = -fd * 0.3;
    let zf = zc - fd * 0.5;
    let b = &mut g.b;
    b.bx(v3(0.0, 0.0, zc), v3(fw, fh, fd), P);
    // Finger ridges + knuckles.
    let nfing = ((fw + 1.0) / 2.0) as i32;
    for i in 0..nfing {
        let x = -fw * 0.5 + 0.5 + i as f32 * 2.0;
        b.bx(v3(x, -0.5, zf - 0.5), v3(1.0, fh - 1.0, 1.0), P);
        b.bx(v3(x, fh * 0.5 - 0.5, zf - 0.5), v3(1.0, 1.0, 1.0), A);
    }
    // Thumb.
    b.bx(v3(-fw * 0.5 - 0.5, -0.5, zc - 0.5), v3(1.0, 2.0, fd - 1.0), P);
    // Wrist + flared cuff.
    let cl = (l - fd).max(2.0);
    b.bx(v3(0.0, 0.0, zc + fd * 0.5 + 0.5), v3(fw - 2.0, fh - 2.0, 1.0), D);
    b.bx(v3(0.0, 0.0, zc + fd * 0.5 + 1.0 + cl * 0.5), v3(fw + 2.0, fh + 1.0, cl), A);
    b.bx(v3(0.0, 0.0, zc + fd * 0.5 + 1.0 + cl - 0.5), v3(fw + 2.0, fh + 1.0, 1.0), D);
    // The ring: a Core band on the middle finger.
    b.bx(v3(-fw * 0.5 + 0.5 + (nfing / 2) as f32 * 2.0, 0.0, zf - 0.5), v3(1.0, 1.0, 1.0), C);
    b.bx(v3(0.0, (fh + 1.0) * 0.5, zc + fd * 0.5 + 1.0 + cl * 0.5), v3(1.0, 1.0, 1.0), C);
    let grab = g.take(Grab);
    let pierce = g.take(Pierce);
    for i in 0..nfing {
        let x = -fw * 0.5 + 0.5 + i as f32 * 2.0;
        if grab {
            g.b.cone_to(v3(x, -0.5, zf - 1.0), v3(x, -1.5, zf - 1.0 - l * 0.3), 1.6, C);
        }
        if pierce {
            g.b.cone_to(v3(x, fh * 0.5, zf), v3(x, fh * 0.5 + l * 0.18, zf - l * 0.12), 1.6, C);
        }
    }
}

pub(super) fn whip(g: &mut G) {
    let l = g.l;
    let b = &mut g.b;
    b.cz(v3(0.0, 0.0, 0.0), 1.0, 4.0, D);
    b.bx(v3(0.0, 0.0, 2.5), v3(3.0, 3.0, 1.0), A);
    b.cz(v3(0.0, 0.0, -2.5), 3.0, 1.0, A);
    let n = (((l - 4.0) / 2.2).round() as usize).clamp(5, 12);
    let seg = (l - 4.0).max(5.0) / n as f32;
    let mut p = v3(0.0, 0.0, -2.5);
    let mut dir = Vec3::NEG_Z;
    let droop = Quat::from_rotation_x(-0.16);
    let mut parent = 0;
    for i in 0..n {
        parent = b.part(parent, p, bd_core::shape::PartRole::Other);
        let q = p + dir * seg;
        let t = if i + 1 == n { C } else if i % 3 == 2 { A } else { P };
        b.segx(PrimKind::Box, p, q, 1.0, 0.35, t);
        p = q;
        dir = (droop * dir).normalize();
    }
    b.cone_to(p - dir * 0.5, p + dir * 2.0, 1.5, C);
    g.handle(Pull);
}

pub(super) fn gun(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3);
    let b = &mut g.b;
    b.bxr(v3(0.0, -0.8, 0.4), v3(1.0, 3.0, 2.0), v3(-14.0, 0.0, 0.0), D);
    b.bx(v3(0.0, -0.5, -1.2), v3(1.0, 1.0, 1.0), D);
    match variant {
        0 => {
            // Blaster pistol.
            let bl = (l * 0.62).max(4.0);
            b.bx(v3(0.0, 1.5, 1.5 - bl * 0.5), v3(3.0, 2.0, bl), P);
            b.bx(v3(0.0, 2.5, 1.0 - bl * 0.3), v3(1.0, 1.0, bl * 0.5), A);
            b.cz(v3(0.0, 1.0, 1.5 - bl - 1.0), 1.0, 2.5, A);
            b.bx(v3(0.0, 1.0, 1.5 - bl - 2.5), v3(1.0, 1.0, 1.0), C);
            b.bx(v3(0.0, 1.5, 1.5 - bl * 0.5), v3(3.0, 1.0, 1.0), C);
        }
        1 => {
            // Rifle: long barrel, stock, scope.
            let bl = l * 0.95;
            b.bx(v3(0.0, 1.0, -1.0), v3(1.0, 2.0, 5.0), P);
            b.cz(v3(0.0, 1.5, -3.5 - bl * 0.3), 1.0, bl * 0.6, A);
            b.bx(v3(0.0, 1.5, -3.5 - bl * 0.6 - 0.5), v3(1.0, 1.0, 1.0), C);
            b.seg(PrimKind::Box, v3(0.0, 1.0, 1.5), v3(0.0, 0.0, 1.5 + l * 0.3), 1.0, 2.0, D);
            b.cz(v3(0.0, 3.0, -1.0), 1.0, 3.0, A);
            b.bx(v3(0.0, 3.0, -2.5), v3(1.0, 1.0, 1.0), C);
            b.bx(v3(0.0, -0.5, -2.5), v3(1.0, 2.0, 1.0), D);
        }
        _ => {
            // Ray gun: bulb body, ringed barrel, glowing orb muzzle.
            b.el(v3(0.0, 1.8, 0.0), v3(3.0, 4.0, 5.0), P);
            b.sym(|b, s| {
                b.bxr(v3(s * 1.5, 3.0, 1.5), v3(1.0, 2.0, 2.0), v3(-30.0, 0.0, 0.0), A);
            });
            let bl = (l * 0.45).max(3.0);
            b.cz(v3(0.0, 1.8, -2.0 - bl * 0.5), 1.0, bl, A);
            for i in 0..3 {
                b.cz(v3(0.0, 1.8, -2.5 - i as f32 * (bl / 3.0)), 3.0, 1.0, if i == 1 { C } else { D });
            }
            b.el(v3(0.0, 1.8, -3.0 - bl), v3(3.0, 3.0, 3.0), C);
        }
    }
    g.handle(Shoots);
}

pub(super) fn bow(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3);
    let half = l * 0.5;
    let depth = l * g.r.range(0.24, 0.32);
    let b = &mut g.b;
    if variant == 2 {
        // Crossbow: stock along Z, horizontal prod, bolt.
        b.bx(v3(0.0, -0.5, -l * 0.12), v3(1.0, 2.0, l * 0.62), D);
        b.bx(v3(0.0, -1.5, 0.8), v3(1.0, 2.0, 1.0), D);
        let z0 = -l * 0.4;
        let n = 4;
        let mut tips = [Vec3::ZERO; 2];
        for (si, s) in [-1.0f32, 1.0].into_iter().enumerate() {
            let mut prev = v3(0.0, 0.5, z0);
            for i in 1..=n {
                let f = i as f32 / n as f32;
                let p = v3(s * half * 0.75 * f, 0.5, z0 + depth * f * f);
                b.seg(PrimKind::Box, prev, p, 1.5, 1.0, if i == n { A } else { P });
                prev = p;
            }
            tips[si] = prev;
        }
        b.seg(PrimKind::Box, tips[0], tips[1], 1.0, 1.0, C);
        b.bx(v3(0.0, 1.0, z0 + depth * 0.5), v3(1.0, 1.0, l * 0.5), P);
        b.cone_to(v3(0.0, 1.0, z0 - 0.5), v3(0.0, 1.0, z0 - 2.5), 1.6, C);
        g.handle(bd_core::mechanics::Mechanic::Shoots);
        return;
    }
    let n = 6;
    let mut tips = [Vec3::ZERO; 2];
    for (si, s) in [-1.0f32, 1.0].into_iter().enumerate() {
        let mut prev = v3(0.0, 0.0, 0.0);
        for i in 1..=n {
            let f = i as f32 / n as f32;
            let mut z = depth * f * f;
            if variant == 1 {
                let k = ((f - 0.72) / 0.28).clamp(0.0, 1.0);
                z -= depth * 0.9 * k * k;
            }
            let p = v3(0.0, s * half * f, z);
            let wz = 2.0 - f;
            b.seg(PrimKind::Box, prev, p, 1.0, wz.max(1.0), if i == n { A } else { P });
            prev = p;
        }
        tips[si] = prev;
    }
    b.bx(v3(0.0, 0.0, 0.0), v3(1.0, 3.0, 2.0), D);
    b.sym(|b, s| {
        b.bx(v3(0.0, s * 2.0, 0.0), v3(1.0, 1.0, 2.0), A);
    });
    b.seg(PrimKind::Box, tips[0], tips[1], 1.0, 1.0, C);
    // Nocked arrow.
    let zs = tips[0].z;
    b.bx(v3(0.0, 0.0, (zs - l * 0.35) * 0.5), v3(1.0, 1.0, zs + l * 0.35), P);
    b.cone_to(v3(0.0, 0.0, -l * 0.35 + 0.5), v3(0.0, 0.0, -l * 0.35 - 2.0), 1.8, C);
    b.sym(|b, s| {
        b.bx(v3(0.0, s * 1.0, zs - 1.0), v3(1.0, 1.0, 2.0), A);
    });
    g.handle(Shoots);
}

pub(super) fn cannon(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3);
    let d = odd(l * g.r.range(0.42, 0.52)).max(3.0);
    let y = d * 0.5 + 1.0;
    let b = &mut g.b;
    match variant {
        0 => {
            let zb = l * 0.3;
            let zf = -l * 0.7;
            b.cz(v3(0.0, y, (zb + zf) * 0.5), d, zb - zf, P);
            b.cz(v3(0.0, y, zf + (zb - zf) * 0.25), d + 2.0, (zb - zf) * 0.5, P);
            b.el(v3(0.0, y, zb), v3(d + 1.0, d + 1.0, d * 0.8), D);
            b.el(v3(0.0, y, zb + d * 0.5), v3(2.0, 2.0, 2.0), D);
            b.cz(v3(0.0, y, zf + 0.75), d + 2.0, 1.5, A);
            b.cz(v3(0.0, y, zf + (zb - zf) * 0.5), d + 3.0, 1.0, A);
            b.bore(v3(0.0, y, zf + 1.0), (d - 2.0).max(1.0), 3.5);
        }
        1 => {
            // Bazooka tube.
            let zb = l * 0.45;
            let zf = -l * 0.55;
            b.cz(v3(0.0, y, (zb + zf) * 0.5), d - 1.0, zb - zf, P);
            b.cz(v3(0.0, y, zf + 0.5), d + 1.0, 1.5, A);
            b.cz(v3(0.0, y, zb - 0.5), d + 1.0, 1.5, D);
            b.cz(v3(0.0, y, 0.0), d + 0.5, 1.0, C);
            b.bx(v3(-d * 0.5 - 0.5, y + d * 0.5, -1.0), v3(1.0, 2.0, 2.0), A);
            b.bx(v3(0.0, y - d * 0.5 - 1.0, -l * 0.25), v3(1.0, 2.0, 1.0), D);
            b.bore(v3(0.0, y, zf + 1.0), (d - 3.0).max(1.0), 3.0);
            b.bore(v3(0.0, y, zb - 1.0), (d - 3.0).max(1.0), 3.0);
        }
        _ => {
            // Hand cannon: short barrel with a flared bell.
            let zb = l * 0.25;
            let zf = -l * 0.55;
            b.cz(v3(0.0, y, (zb + zf) * 0.5), d, zb - zf, P);
            b.cone_to(v3(0.0, y, zf), v3(0.0, y, zf + l * 0.45), d + 4.0, A);
            b.el(v3(0.0, y, zb), v3(d + 1.0, d + 1.0, d), D);
            b.bore(v3(0.0, y, zf + 1.0), d, 3.0);
        }
    }
    // Glowing charge at the bottom of the bore.
    let zf = match variant {
        0 => -l * 0.7,
        1 => -l * 0.55,
        _ => -l * 0.55,
    };
    b.cz(v3(0.0, y, zf + 3.0), (d - 2.0).max(1.0), 1.0, C);
    b.bx(v3(0.0, 0.5, 0.0), v3(1.0, 2.0, 2.0), D);
    g.handle(Shoots);
}

/// `cz` with a subtract flag (bore holes).
trait Bore {
    fn bore(&mut self, c: Vec3, d: f32, len: f32) -> &mut Self;
}
impl Bore for crate::build::B {
    fn bore(&mut self, c: Vec3, d: f32, len: f32) -> &mut Self {
        self.cut(PrimKind::Cylinder, c, v3(d, len, d), v3(90.0, 0.0, 0.0))
    }
}

pub(super) fn staff(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(4);
    let zb = l * 0.35;
    let zt = -l * 0.65;
    if g.take(SlowFall) {
        // Umbrella canopy (built first: its cuts must not touch the shaft).
        let r = l * 0.42;
        let b = &mut g.b;
        b.el(v3(0.0, 0.0, zt + r * 0.3), v3(r * 2.0, r * 2.0, r * 1.2), P);
        b.cut_el(v3(0.0, 0.0, zt + r * 0.3 + 1.2), v3(r * 2.0 - 2.0, r * 2.0 - 2.0, r * 1.2 - 1.0));
        b.cut_bx(v3(0.0, 0.0, zt + r * 0.3 + 50.0), v3(r * 3.0, r * 3.0, 100.0));
        for i in 0..4 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            b.bxr(v3(0.0, 0.0, zt + r * 0.3 - r * 0.3), v3(1.0, r * 2.2, 1.0), v3(0.0, 0.0, a.to_degrees()), A);
        }
    }
    let b = &mut g.b;
    b.cz(v3(0.0, 0.0, (zb + zt) * 0.5), 1.0, zb - zt, P);
    b.cz(v3(0.0, 0.0, 0.0), 1.0, l * 0.12, D);
    b.bx(v3(0.0, 0.0, zb), v3(3.0, 3.0, 1.0), A);
    b.bx(v3(0.0, 0.0, zt + 0.5), v3(3.0, 3.0, 1.0), A);
    match variant {
        0 => {
            b.el(v3(0.0, 0.0, zt - 2.5), v3(5.0, 5.0, 5.0), C);
            for i in 0..3 {
                let a = i as f32 * std::f32::consts::TAU / 3.0 + 0.5;
                let (s, c) = a.sin_cos();
                let p0 = v3(c * 1.0, s * 1.0, zt);
                let p1 = v3(c * 3.2, s * 3.2, zt - 2.0);
                let p2 = v3(c * 1.5, s * 1.5, zt - 5.5);
                b.segx(PrimKind::Box, p0, p1, 1.0, 0.3, A);
                b.segx(PrimKind::Box, p1, p2, 1.0, 0.3, A);
            }
        }
        1 => {
            let c = v3(0.0, 0.0, zt - 3.5);
            b.cx(c, 8.0, 1.0, A);
            b.cut(PrimKind::Cylinder, c + v3(0.0, 0.0, -2.0), v3(6.5, 3.0, 6.5), v3(0.0, 0.0, 90.0));
            b.el(c + v3(0.0, 0.0, 0.5), v3(1.0, 3.0, 3.0), C);
        }
        2 => {
            b.bxr(v3(0.0, 0.0, zt - 3.0), v3(3.0, 3.0, 6.0), v3(0.0, 0.0, 45.0), C);
            b.sym(|b, s| {
                b.segx(PrimKind::Box, v3(s * 1.0, 0.0, zt), v3(s * 2.5, 0.0, zt - 2.5), 1.0, 0.3, A);
                b.segx(PrimKind::Box, v3(0.0, s * 1.0, zt), v3(0.0, s * 2.5, zt - 2.5), 1.0, 0.3, A);
            });
        }
        _ => {
            let c = v3(0.0, 0.0, zt - 3.0);
            b.cx(c, 7.0, 1.0, A);
            b.cut(PrimKind::Cylinder, c, v3(4.5, 3.0, 4.5), v3(0.0, 0.0, 90.0));
            b.bx(c + v3(0.0, 0.0, 0.0), v3(1.0, 1.0, 1.0), C);
            b.sym(|b, s| {
                b.bx(c + v3(0.0, s * 2.0, 1.5), v3(1.0, 1.0, 1.0), C);
            });
            b.cz(v3(0.0, 0.0, zt - 0.5), 1.0, 2.0, P);
        }
    }
    g.handle(Beam);
    g.handle(Shoots);
}

pub(super) fn disc(g: &mut G) {
    let l = g.l;
    let variant = g.r.pick(3);
    let d = odd(l);
    let c = v3(0.0, 0.0, -d * 0.5);
    let spin = g.take(Spin) | g.take(Blade);
    let b = &mut g.b;
    match variant {
        0 => {
            // Chakram ring.
            b.cy(c, d, 1.0, A);
            b.cy(c, d - 2.0, 1.0, P);
            b.cut(PrimKind::Cylinder, c, v3(d * 0.45, 3.0, d * 0.45), Vec3::ZERO);
            b.bx(c + v3(0.0, 0.0, d * 0.5 - 1.0), v3(1.0, 1.0, 1.0), C);
            b.bx(c - v3(0.0, 0.0, d * 0.5 - 1.0), v3(1.0, 1.0, 1.0), C);
        }
        1 => {
            // Shield disc with rim and hub.
            b.cy(c, d, 1.0, A);
            b.cy(c, d - 2.0, 1.0, P);
            b.cy(c, 3.0, 3.0, D);
            b.bx(c, v3(1.0, 3.0, 1.0), C);
        }
        _ => {
            // Shuriken.
            for i in 0..4 {
                let a = i as f32 * std::f32::consts::FRAC_PI_2 + 0.4;
                let dir = v3(a.cos(), 0.0, a.sin());
                b.flat(PrimKind::Cone, c, c + dir * d * 0.55, d * 0.32, 1.0, Vec3::Y, P);
            }
            b.cy(c, 3.0, 1.0, A);
            b.bx(c, v3(1.0, 1.0, 1.0), C);
        }
    }
    if spin {
        let n = 8;
        for i in 0..n {
            let a = i as f32 * std::f32::consts::TAU / n as f32;
            let dir = v3(a.cos(), 0.0, a.sin());
            let t = c + dir * (d * 0.5 + 0.5);
            b.flat(PrimKind::Cone, c + dir * (d * 0.5 - 1.0), t + v3(-dir.z, 0.0, dir.x) * 1.5, 2.0, 1.0, Vec3::Y, C);
        }
    }
    g.handle(Returning);
    g.handle(Ricochet);
}
