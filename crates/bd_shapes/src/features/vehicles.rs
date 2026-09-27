//! `Form::Vehicle` archetypes chosen from the recipe's features: locomotive (Boiler / Smokestack +
//! Wheels|Pistons), helicopter (Rotor), ship (Sails / Mast), plane (Jet / Propeller), tank (Treads /
//! Turret), motorcycle (Wheels + Exhaust, no Cab / Windows / Cockpit), speedboat (Propeller, no Fins /
//! Wheels), submarine (Propeller + Fins), crane truck (CraneArm), else car / truck / tank by seed. Origin = ground centre, front −Z.
//! Scale (`g.l`, units) = overall length. Features the archetype builds are marked handled; the rest
//! are placed generically afterwards.

use std::f32::consts::{FRAC_PI_2, PI};

use bd_core::mechanics::Feature::*;
use bd_core::mechanics::Mechanic::*;
use bd_core::shape::{Part, PartRole, PrimKind};
use glam::Vec3;

use super::kit::*;
use super::PuffSpec;
use crate::build::{A, C, D, P, odd, v3};


use crate::forms::G;
use crate::voxelize::SEAT_PART;

pub(crate) fn vehicle(g: &mut G) {
    // Seeded default look (drawn first so the plain car/truck/tank variety is stable).
    let variant = g.r.pick(3);
    if g.feat(Rotor) {
        helicopter(g);
    } else if g.feat(Sails) || g.feat(Mast) {
        ship(g);
    } else if g.feat(Boiler) || (g.feat(Smokestack) && (g.feat(Wheels) || g.feat(Pistons))) {
        locomotive(g);
    } else if g.feat(Treads) || (g.feat(Turret) && !g.feat(Wheels)) {
        tank(g);
    } else if g.feat(Jet) || (g.feat(Propeller) && (g.has(Flies) || g.has(Glide))) || (g.has(Flies) && !g.feat(Wheels)) {
        plane(g);
    } else if g.feat(Wheels) && g.feat(Exhaust) && !g.feat(Cab) && !g.feat(Windows) && !g.feat(Cockpit) {
        motorcycle(g);
    } else if g.feat(Propeller) && !g.feat(Fins) && !g.feat(Wheels) {
        speedboat(g);
    } else if g.feat(Propeller) || g.feat(Fins) {
        submarine(g);
    } else if g.feat(CraneArm) {
        car(g, 1);
    } else if variant == 2 && !g.feat(Wheels) {
        tank(g);
    } else {
        car(g, variant.min(1));
    }
    g.handle(Charge);
}

// ---------------------------------------------------------------------------------- locomotive

fn locomotive(g: &mut G) {
    let fast = g.has(Fast);
    let l = g.l.max(12.0);
    let tender = l >= 30.0;
    let lc = if tender { (l * 0.74).round() } else { l };
    let zf = -l * 0.5;
    let zb = zf + lc;
    // Drivers: 3 coupled per side, crank radius ~half the wheel radius, quartered sides. The driver
    // size is solved so cylinder + crosshead + rod + 3 drivers fit the loco length (11 + 8.2·r).
    // (upper bound kept ≥ the lower: `clamp` panics when min > max, e.g. lc just over 12)
    let rw_fit = ((lc - 8.0) / 7.2).clamp(1.5, (lc * 0.12).max(1.5));
    let dd = odd(2.0 * rw_fit).max(3.0);
    let rw = dd * 0.5;
    let ay = (dd + 1.0) * 0.5;
    let rc = ((rw * 0.55) * 2.0).round().max(2.0) * 0.5;
    let sp = dd + 1.0;
    let xw = (lc * 0.13).round().max(3.0);
    let cyl_d = odd((dd * 0.6).max(3.0));
    // Front-to-back layout: cylinder, crosshead, main (front) driver, two coupled drivers.
    let cyl_len = 2.0 * rc + 3.0;
    let cyl_back = zf + 0.5 + cyl_len;
    let cross_z = cyl_back + rc + 1.0;
    let rod_len = rw + rc + 1.5;
    let z0 = (cross_z + rod_len).round();
    let zs = [z0, z0 + sp, z0 + 2.0 * sp];
    let bd = odd((dd + 2.0).max(lc * 0.2));
    let rb_y = ay + rw + 0.5;
    let by = (rb_y + bd * 0.5 - 1.5).round();
    let top = by + bd * 0.5;
    let cab_l = (lc * 0.3).round().max(4.0);
    let cab_z0 = zb - cab_l;
    let bw = 2.0 * xw + 1.0;

    let b = &mut g.b;
    // Frame, running boards with valance, buffer beam + buffers.
    b.bx(v3(0.0, (ay + rb_y) * 0.5, (zf + 1.0 + zb) * 0.5), v3(2.0 * xw - 3.0, rb_y - ay + 1.0, zb - zf - 1.0), D);
    b.bx(v3(0.0, rb_y, (zf + 1.0 + zb) * 0.5), v3(bw + 6.0, 1.0, zb - zf - 1.0), D);
    b.bx(v3(0.0, rb_y + 0.5, (zf + 1.0 + zb) * 0.5), v3(bw + 6.2, 1.0, zb - zf - 1.2), A);
    b.bx(v3(0.0, ay + 1.0, zf + 0.5), v3(bw + 6.0, 3.0, 1.0), A);
    b.sym(|b, s| {
        b.cz(v3(s * (xw + 1.0), ay + 1.0, zf - 0.5), 2.0, 2.0, D);
        b.cz(v3(s * (xw + 1.0), ay + 1.0, zf - 1.5), 3.0, 1.0, A);
    });
    // Boiler with smokebox door, domes, whistle; firebox glow under the cab.
    boiler(b, v3(0.0, by, zf + 2.0), v3(0.0, by, cab_z0 + 1.0), bd, true);
    if fast {
        b.sym(|b, s| {
            b.bar(v3(s * (bd * 0.5 + 0.2), by, zf + 4.0), v3(s * (bd * 0.5 + 0.2), by, cab_z0), 1.0, C);
        });
    }
    let dome = v3(0.0, top - 0.5, (zf + cab_z0) * 0.5 + 1.0);
    b.el(dome, v3(bd * 0.5, bd * 0.6, bd * 0.5), A);
    b.cy(dome + Vec3::Y * (bd * 0.25), (bd * 0.3).max(1.0), 1.0, C);
    b.el(v3(0.0, top - 0.5, cab_z0 - 2.0), v3(bd * 0.4, bd * 0.4, bd * 0.4), P);
    b.cy(v3(0.0, top + 1.0, cab_z0 - 0.5), 1.0, 2.0, A);
    b.bx(v3(0.0, ay + 1.0, cab_z0 + 1.0), v3(2.0 * xw - 4.0, 2.0, 2.0), C);
    // Cab + coal bunker (tank engines) + windows.
    let cab_h = top + 2.0 - rb_y;
    cab(b, v3(0.0, rb_y + cab_h * 0.5, cab_z0 + cab_l * 0.5), bw + 1.0, cab_h, cab_l);
    g.handle_feat(Cab);
    g.handle_feat(Windows);
    // Headlamp on the smokebox.
    headlight(&mut g.b, v3(0.0, top + 1.0, zf + 2.5), Vec3::NEG_Z, (bd * 0.35).max(1.5));
    g.handle_feat(Headlight);
    if g.take_feat(Plow) | g.has(Charge) {
        plow(&mut g.b, 0.0, zf - 1.5, 0.5, ay + 0.5, bw + 3.0);
    }

    // Drivers (quartered: left pins at 0 rad, right at 90°) + crank-slider linkages.
    let mut main = None;
    for s in [-1.0f32, 1.0] {
        let boss = if s < 0.0 { 0.0 } else { FRAC_PI_2 };
        let mut ws = Vec::new();
        for &z in &zs {
            let c = v3(s * xw, ay, z);
            let id = wheel(&mut g.b, 0, c, dd, 1.0, WheelStyle::Driver, boss);
            ws.push((id, c));
        }
        if s > 0.0 {
            main = ws.first().map(|w| w.0);
        }
        crank_side(&mut g.b, 0, &ws, 0, s * (xw + 1.0), s, boss, rc, cross_z, cyl_d);
    }
    // Leading pony truck under the cylinders.
    let ld = odd((dd * 0.6).max(3.0));
    if lc >= 22.0 {
        let lz = (zf + cyl_back) * 0.5 + 1.0;
        for s in [-1.0f32, 1.0] {
            wheel(&mut g.b, 0, v3(s * (xw - 1.0), (ld + 1.0) * 0.5, lz), ld, 1.0, WheelStyle::Spoked, 0.0);
        }
    }
    // Smokestack chuffs 4× per driver revolution; cylinder cocks puff steam low.
    let stack_h = (lc * 0.13).round().max(3.0);
    smokestack(&mut g.b, 0, v3(0.0, top - 1.0, zf + 4.0), stack_h, odd((bd * 0.42).max(3.0)), main, 4.0, 0.6);
    g.handle_feat(Smokestack);
    for s in [-1.0f32, 1.0] {
        g.b.puff(PuffSpec {
            part: 0,
            at: v3(s * (xw + 2.0), ay - 1.5, zf + 2.0),
            dir: v3(s * 0.8, -0.1, -0.6).normalize(),
            size: 1.5,
            per_rev: 2.0,
            wheel: main,
            idle_hz: 0.0,
            per_m: 0.0,
        });
    }
    if g.take_feat(Bell) {
        bell(&mut g.b, 0, v3(0.0, top, (zf + cab_z0) * 0.5 - bd * 0.6), (bd * 0.3).max(2.0));
    }
    g.handle_feat(Boiler);
    g.handle_feat(Pistons);
    g.handle_feat(Wheels);

    // Tender: coal bunker on two 2-axle bogies.
    if tender {
        let tz0 = zb + 1.5;
        let tz1 = l * 0.5;
        let td = odd((dd * 0.7).max(3.0));
        let ty = (td + 1.0) * 0.5;
        let tl = tz1 - tz0;
        let th = top - 1.0 - (td + 1.0);
        let b = &mut g.b;
        b.bar(v3(0.0, ay + 1.0, zb), v3(0.0, ay + 1.0, tz0), 1.0, D);
        b.bx(v3(0.0, td + 1.5, (tz0 + tz1) * 0.5), v3(bw + 3.0, 1.0, tl), D);
        b.bx(v3(0.0, td + 2.0 + th * 0.5, (tz0 + tz1) * 0.5), v3(bw + 1.0, th, tl - 1.0), P);
        b.bx(v3(0.0, td + 2.0 + th * 0.35, (tz0 + tz1) * 0.5), v3(bw + 1.6, 1.0, tl - 1.0), if fast { C } else { A });
        for k in 0..3 {
            let z = tz0 + tl * (0.25 + 0.25 * k as f32);
            b.el(v3(0.0, td + 2.0 + th, z), v3(bw - 1.0, 3.0, tl * 0.32), D);
        }
        b.rivets(v3(-(bw + 1.0) * 0.5 - 0.3, td + 3.0, tz0 + 1.0), v3(-(bw + 1.0) * 0.5 - 0.3, td + 3.0, tz1 - 1.0), 2.0, D);
        b.rivets(v3((bw + 1.0) * 0.5 + 0.3, td + 3.0, tz0 + 1.0), v3((bw + 1.0) * 0.5 + 0.3, td + 3.0, tz1 - 1.0), 2.0, D);
        b.bx(v3(0.0, td + 1.0 + th, tz1 - 0.5), v3(2.0, 2.0, 1.0), C);
        let bog = [tz0 + td * 0.5 + 1.0, tz0 + td * 1.5 + 2.0, tz1 - td * 1.5 - 2.0, tz1 - td * 0.5 - 1.0];
        for (i, &z) in bog.iter().enumerate() {
            for s in [-1.0f32, 1.0] {
                wheel(&mut g.b, 0, v3(s * xw, ty, z), td, 1.0, WheelStyle::Spoked, i as f32);
            }
        }
        for zz in [(bog[0] + bog[1]) * 0.5, (bog[2] + bog[3]) * 0.5] {
            g.b.sym(|b, s| {
                b.bx(v3(s * (xw + 1.0), ty, zz), v3(1.0, 2.0, td * 2.0 + 1.0), D);
            });
        }
    }
    g.handle(Fast);
    g.handle(Heavy);
}

// --------------------------------------------------------------------------------------- tank

fn tank(g: &mut G) {
    let l = g.l.max(10.0);
    let w = odd(l * 0.62);
    let th = odd((l * 0.26).max(3.0));
    let tw = odd((l * 0.17).max(2.0)).max(3.0);
    let tx = (w - tw) * 0.5;
    let hull_y0 = th * 0.5;
    let hh = (l * 0.16).max(2.0);
    let deck = th + hh * 0.5;
    for s in [-1.0f32, 1.0] {
        tread(&mut g.b, 0, s * tx, -l * 0.47, l * 0.47, th, tw, s);
    }
    g.handle_feat(Treads);
    let b = &mut g.b;
    // Hull between the treads, upper hull over them, sloped glacis, engine deck.
    b.bx(v3(0.0, (hull_y0 + deck) * 0.5, 0.0), v3(w - 2.0 * tw, deck - hull_y0, l * 0.84), P);
    b.bx(v3(0.0, th + 0.5, 0.02 * l), v3(w + 1.0, hh, l * 0.8), P);
    b.bxr(v3(0.0, th + 0.5, -l * 0.42), v3(w - 1.0, hh * 1.1, l * 0.14), v3(-38.0, 0.0, 0.0), A);
    b.bx(v3(0.0, th + hh * 0.5 + 0.5, 0.02 * l), v3(w + 1.0, 1.0, l * 0.8), D);
    for k in 0..3 {
        b.bx(v3(0.0, th + hh * 0.5 + 1.0, l * 0.3 + k as f32 * 1.5), v3(w * 0.5, 1.0, 1.0), D);
    }
    let armor = g.take_feat(Armor);
    if armor {
        // Side skirts over the upper tread + riveted glacis plate.
        let b = &mut g.b;
        for s in [-1.0f32, 1.0] {
            plate(b, v3(s * (w * 0.5 + 0.6), th * 0.75, 0.0), v3(1.0, th * 0.45, l * 0.85), Vec3::X * s, A);
            b.rivets(v3(s * (w * 0.5 + 1.2), th * 0.75, -l * 0.4), v3(s * (w * 0.5 + 1.2), th * 0.75, l * 0.4), 2.0, D);
        }
    }
    // Exhaust stacks at the back (diesel puffs even when parked).
    let ex = g.take_feat(Exhaust);
    for s in [-1.0f32, 1.0] {
        exhaust_pipe(&mut g.b, 0, v3(s * w * 0.3, th + hh * 0.5 + 1.5, l * 0.43), v3(0.0, 0.6, 1.0), 1.0, ex);
    }
    if g.take_feat(Headlight) {
        g.b.sym(|b, s| headlight(b, v3(s * w * 0.35, th + hh * 0.5 + 0.5, -l * 0.44), Vec3::NEG_Z, 1.5));
    }
    // Turret + gun.
    let has_turret = g.take_feat(Turret) | g.take_feat(Barrels) | g.take(Shoots) | true;
    let _ = has_turret;
    let tpos = v3(0.0, th + hh * 0.5 + 1.0, l * 0.04);
    let tws = odd(w * 0.56);
    let (yaw, _) = turret(&mut g.b, 0, tpos, tws, l * 0.38, (l * 0.15).max(2.5), l * 0.55, odd((l * 0.055).max(1.0)), g.r.chance(0.25));
    if g.take_feat(Antenna) {
        antenna(&mut g.b, yaw, tpos + v3(tws * 0.35, (l * 0.15).max(2.5), l * 0.12), (l * 0.3).max(4.0));
    }
    // Front: drill (drill tank) or dozer blade.
    if g.take_feat(Drill) {
        let base = v3(0.0, th * 0.9, -l * 0.47);
        g.b.sym(|b, s| {
            b.bar(v3(s * w * 0.3, th, -l * 0.35), base + v3(s * 1.5, 0.0, 0.5), 1.5, D);
        });
        drill(&mut g.b, 0, base, Vec3::NEG_Z, l * 0.38, (l * 0.12).max(2.0));
        g.handle(Pierce);
        g.handle(Spin);
    } else if g.has(Charge) || g.take_feat(Plow) {
        plow(&mut g.b, 0.0, -l * 0.47, 0.5, th + 1.0, w);
    }
    g.handle(Heavy);
}

// --------------------------------------------------------------------------------- helicopter

fn helicopter(g: &mut G) {
    let l = g.l.max(10.0);
    let bw = odd((l * 0.3).max(3.0));
    let bh = (l * 0.3).max(3.0);
    let bl = l * 0.5;
    let by = (l * 0.08).max(1.5) + bh * 0.5 + 0.5;
    let cz = -l * 0.14;
    let b = &mut g.b;
    // Cabin, glass nose, tail boom, fin, stabiliser, engine hump.
    b.el(v3(0.0, by, cz), v3(bw, bh, bl), P);
    b.el(v3(0.0, by - bh * 0.25, cz), v3(bw + 0.5, bh * 0.5, bl * 0.9), A);
    let nose = v3(0.0, by + bh * 0.08, cz - bl * 0.3);
    canopy(b, nose, bw - 1.0, bh * 0.36, bl * 0.42);
    g.handle_feat(Cockpit);
    let b = &mut g.b;
    let boom0 = v3(0.0, by + bh * 0.15, cz + bl * 0.4);
    let boom1 = v3(0.0, by + bh * 0.3, l * 0.46);
    b.seg(PrimKind::Cylinder, boom0, boom0.lerp(boom1, 0.5), (bh * 0.4).max(2.0), (bh * 0.4).max(2.0), P);
    b.seg(PrimKind::Cylinder, boom0.lerp(boom1, 0.5), boom1, (bh * 0.25).max(1.5), (bh * 0.25).max(1.5), P);
    b.bar(boom0.lerp(boom1, 0.2), boom1, 1.0, A);
    fin(b, boom1, boom1 + v3(0.0, l * 0.2, l * 0.06), l * 0.12, Vec3::X);
    b.flat(PrimKind::Box, boom1 + v3(-l * 0.12, 0.0, -1.0), boom1 + v3(l * 0.12, 0.0, -1.0), 2.5, 1.0, Vec3::Y, A);
    let hump = v3(0.0, by + bh * 0.5, cz + bl * 0.1);
    b.bx(hump, v3(bw * 0.6, 2.0, bl * 0.45), A);
    b.bx(hump + v3(0.0, 0.5, bl * 0.2), v3(bw * 0.4, 1.0, 2.0), D);
    if g.take_feat(Windows) {
        let b = &mut g.b;
        b.sym(|b, s| window_row(b, s * (bw * 0.5 - 0.2), by + bh * 0.1, cz - bl * 0.05, cz + bl * 0.3, 2.0));
    }
    // Skids.
    let b = &mut g.b;
    b.sym(|b, s| {
        let x = s * (bw * 0.45 + 0.5);
        b.bar(v3(x, 0.5, cz - bl * 0.45), v3(x, 0.5, cz + bl * 0.45), 1.0, D);
        b.bar(v3(x, 0.5, cz - bl * 0.45), v3(x, 1.8, cz - bl * 0.6), 1.0, D);
        for z in [cz - bl * 0.25, cz + bl * 0.25] {
            b.bar(v3(x, 0.5, z), v3(s * bw * 0.3, by - bh * 0.4, z), 1.0, D);
        }
    });
    // Rotors.
    let hub = v3(0.0, by + bh * 0.5 + 2.5, cz + bl * 0.05);
    g.b.cy(hub - Vec3::Y * 1.2, 1.5, 2.5, D);
    rotor(&mut g.b, 0, hub, Vec3::Y, l * 0.5, 4, 13.0, P);
    rotor(&mut g.b, 0, boom1 + v3(1.5, l * 0.07, 0.5), Vec3::X, (l * 0.13).max(2.0), 3, 24.0, A);
    g.handle_feat(Rotor);
    if g.take(Shoots) {
        g.b.sym(|b, s| {
            let root = v3(s * bw * 0.4, by - bh * 0.1, cz + bl * 0.05);
            b.flat(PrimKind::Box, root, root + Vec3::X * (s * l * 0.15), 2.0, 1.0, Vec3::Y, D);
            barrel(b, root + v3(s * l * 0.13, -1.0, bl * 0.1), Vec3::NEG_Z, l * 0.2, 1.0);
        });
    }
    let b = &mut g.b;
    b.puff(PuffSpec { part: 0, at: hump + v3(0.0, 1.0, bl * 0.25), dir: v3(0.0, 0.5, 1.0).normalize(), size: 1.2, per_rev: 0.0, wheel: None, idle_hz: 1.4, per_m: 0.0 });
    g.handle_feat(Fins);
    g.handle(Flies);
    g.handle(Glide);
    g.handle(Spin);
    g.handle(Thrust);
}

// --------------------------------------------------------------------------------------- ship

fn ship(g: &mut G) {
    let charge = g.has(Charge);
    let l = g.l.max(12.0);
    let w = odd(l * 0.28);
    let hd = (l * 0.17).max(3.0);
    let b = &mut g.b;
    // Hull: lower half of an ellipsoid (carved right away so only the hull is cut), keel stripe.
    b.el(v3(0.0, hd, 0.0), v3(w, 2.0 * hd, l), P);
    b.cut_bx(v3(0.0, hd * 2.0 + 0.5, 0.0), v3(w + 4.0, 2.0 * hd, l + 4.0));
    b.el(v3(0.0, hd * 0.55, 0.0), v3(w + 0.6, 1.0, l * 0.95), A);
    b.el(v3(0.0, hd + 0.5, 0.0), v3(w - 1.0, 1.0, l - 2.0), D);
    // Stern castle with glowing windows + lantern, forecastle, bowsprit, rudder.
    let sc = v3(0.0, hd + l * 0.04 + 1.0, l * 0.36);
    b.bx(sc, v3(w * 0.8, l * 0.08, l * 0.2), P);
    b.bx(sc + Vec3::Y * (l * 0.04 + 0.5), v3(w * 0.84, 1.0, l * 0.21), A);
    window_row(b, 0.0, sc.y, -w * 0.3, w * 0.3, (l * 0.03).max(1.0));
    let sw = (l * 0.03).max(1.0);
    for k in 0..3 {
        b.bx(v3((k as f32 - 1.0) * w * 0.22, sc.y, sc.z + l * 0.1 + 0.3), v3(sw, sw, 1.2), C);
    }
    b.cy(sc + v3(0.0, l * 0.04 + 2.0, l * 0.08), 1.5, 3.0, C);
    b.bx(v3(0.0, hd + l * 0.02 + 0.5, -l * 0.36), v3(w * 0.6, l * 0.04, l * 0.12), P);
    let bow = v3(0.0, hd + 1.0, -l * 0.48);
    let tip = bow + v3(0.0, l * 0.1, -l * 0.18);
    b.bar(bow, tip, (l * 0.02).max(1.0), D);
    if charge {
        b.cone_to(bow + v3(0.0, -hd * 0.3, 0.0), bow + v3(0.0, -hd * 0.3, -l * 0.12), (hd * 0.6).max(2.0), C);
    }
    b.flat(PrimKind::Box, v3(0.0, hd * 0.6, l * 0.48), v3(0.0, hd * 0.1, l * 0.52), (l * 0.06).max(1.5), 1.0, Vec3::X, D);
    // Gunports + cannons along both sides.
    if g.take_feat(Barrels) | g.take(Shoots) {
        let n = ((l / 10.0).round() as usize).clamp(2, 8);
        for k in 0..n {
            let z = -l * 0.3 + l * 0.6 * (k as f32 + 0.5) / n as f32;
            g.b.sym(|b, s| {
                let x = s * (w * 0.5 - 0.5);
                b.bx(v3(x, hd * 0.8, z), v3(1.2, (l * 0.035).max(1.5), (l * 0.035).max(1.5)), D);
                barrel(b, v3(x, hd * 0.8, z), Vec3::X * s, (l * 0.035).max(1.5), (l * 0.018).max(1.0));
            });
        }
    }
    // Masts with square sails.
    let nm = if l < 24.0 { 1 } else if l < 60.0 { 2 } else { 3 };
    let zs: &[f32] = match nm {
        1 => &[-0.02],
        2 => &[-0.16, 0.12],
        _ => &[-0.22, -0.02, 0.18],
    };
    for (k, &z) in zs.iter().enumerate() {
        let main = (nm == 1 || k == nm / 2) as i32 as f32;
        let h = l * (0.46 + 0.1 * main);
        let yards = if g.feat(Sails) { if l < 20.0 { 1 } else { 2 + (main as usize) } } else { 0 };
        mast(&mut g.b, 0, v3(0.0, hd + 1.0, l * z), h, w * 1.7, yards, k as f32 * 1.3);
    }
    g.handle_feat(Sails);
    g.handle_feat(Mast);
    if g.take_feat(Chains) {
        chain(&mut g.b, 0, v3(w * 0.4, hd + 0.5, -l * 0.42), 4, (l * 0.03).max(1.5), true);
    }
    g.handle_feat(Windows);
}

// -------------------------------------------------------------------------------------- plane

fn plane(g: &mut G) {
    let l = g.l.max(10.0);
    let fd = odd((l * 0.16).max(3.0));
    let fy = fd * 0.5 + (l * 0.1).max(1.5);
    let b = &mut g.b;
    b.cz(v3(0.0, fy, 0.0), fd, l * 0.7, P);
    b.cone_to(v3(0.0, fy, -l * 0.35), v3(0.0, fy, -l * 0.5), fd, A);
    b.cone_to(v3(0.0, fy, l * 0.35), v3(0.0, fy + fd * 0.2, l * 0.5), fd, P);
    b.el(v3(0.0, fy - fd * 0.3, 0.0), v3(fd + 0.4, fd * 0.3, l * 0.7), A);
    // Wings + tail.
    let span = l * 0.55;
    b.sym(|b, s| {
        let y = fy - fd * 0.2;
        let lead0 = v3(s * fd * 0.4, y, -l * 0.18);
        let trail0 = v3(s * fd * 0.4, y, l * 0.12);
        let tip0 = v3(s * span, y + 0.5, l * 0.08);
        let tip1 = v3(s * span, y + 0.5, l * 0.16);
        b.fan(trail0, lead0, tip0, 0.0, 1.0, Vec3::Y, P);
        b.fan(tip0, trail0, tip1, 0.0, 1.0, Vec3::Y, P);
        b.flat(PrimKind::Box, lead0, tip0, 1.3, 1.6, Vec3::Y, A);
        b.bx(tip1 + v3(0.0, 0.5, 0.0), Vec3::ONE, C);
        let t0 = v3(s * fd * 0.3, fy, l * 0.36);
        b.fan(t0 + v3(0.0, 0.0, l * 0.12), t0, v3(s * l * 0.2, fy, l * 0.48), 0.0, 1.0, Vec3::Y, P);
    });
    fin(b, v3(0.0, fy + fd * 0.4, l * 0.38), v3(0.0, fy + l * 0.18, l * 0.47), l * 0.1, Vec3::X);
    g.handle_feat(Fins);
    canopy(&mut g.b, v3(0.0, fy + fd * 0.45, -l * 0.18), (fd - 1.0).max(1.0), (fd * 0.35).max(1.0), l * 0.16);
    g.handle_feat(Cockpit);
    if g.take_feat(Windows) {
        g.b.sym(|b, s| window_row(b, s * fd * 0.5, fy + 0.5, -l * 0.08, l * 0.28, 1.0));
    }
    // Engines.
    if g.take_feat(Jet) | g.take_feat(Turbine) | !g.feat(Propeller) {
        for s in [-1.0f32, 1.0] {
            let c = v3(s * span * 0.45, fy - fd * 0.2 - (fd * 0.35).max(1.5), -l * 0.08);
            g.b.bar(c + Vec3::Y * 1.5, c + Vec3::Y * 0.5, 1.0, D);
            jet(&mut g.b, 0, c + Vec3::NEG_Z * (l * 0.08), c + Vec3::Z * (l * 0.1), (fd * 0.35).max(1.5));
        }
    }
    if g.take_feat(Propeller) {
        rotor(&mut g.b, 0, v3(0.0, fy, -l * 0.5 - 0.5), Vec3::NEG_Z, (l * 0.16).max(2.5), 3, 20.0, P);
    }
    if g.take_feat(Wheels) {
        for (x, z) in [(-fd * 0.8, -l * 0.02), (fd * 0.8, -l * 0.02), (0.0, -l * 0.34)] {
            g.b.bar(v3(x, fy - fd * 0.4, z), v3(x, 1.5, z), 1.0, D);
            wheel(&mut g.b, 0, v3(x, 1.5, z), 3.0, 1.0, WheelStyle::Tire, 0.0);
        }
    }
    g.handle(Flies);
    g.handle(Glide);
    g.handle(Thrust);
}

// ---------------------------------------------------------------------------------- car/truck

fn car(g: &mut G, variant: u32) {
    let l = g.l.max(8.0);
    let w = odd(l * 0.42);
    let wheel_d = odd(l * 0.2).max(3.0);
    let axle_y = (wheel_d + 1.0) * 0.5;
    let chassis_y = axle_y + 1.0;
    let front = -l * 0.5;
    let truck = variant == 1;
    let crane_f = g.feat(CraneArm);
    let body_h = if truck { 4.0 } else { 3.0 };
    let b = &mut g.b;
    b.bx(v3(0.0, chassis_y + body_h * 0.5 - 0.5, 0.0), v3(w, body_h, l), P);
    b.bx(v3(0.0, chassis_y - 0.5, 0.0), v3(w + 1.0, 1.0, l + 1.0), D);
    let crane_deck;
    if truck {
        b.bx(v3(0.0, chassis_y + body_h + 1.5, front + l * 0.2), v3(w - 2.0, 3.0, l * 0.3), P);
        b.bx(v3(0.0, chassis_y + body_h + 2.0, front + l * 0.08), v3(w - 2.0, 1.0, 1.0), C);
        if crane_f {
            crane_deck = Some(v3(0.0, chassis_y + body_h, l * 0.18));
        } else {
            b.bx(v3(0.0, chassis_y + body_h + 2.5, l * 0.18), v3(w, 5.0, l * 0.58), A);
            b.bx(v3(0.0, chassis_y + body_h + 2.5, l * 0.18), v3(w + 1.0, 1.0, l * 0.58), D);
            crane_deck = None;
        }
    } else {
        let cab_l = l * 0.45;
        b.bx(v3(0.0, chassis_y + body_h + 1.0, l * 0.05), v3(w - 2.0, 2.0, cab_l), A);
        b.bxr(v3(0.0, chassis_y + body_h + 0.5, l * 0.05 - cab_l * 0.55), v3(w - 2.0, 1.0, 3.0), v3(-40.0, 0.0, 0.0), C);
        b.bx(v3(0.0, chassis_y + body_h + 1.0, l * 0.05), v3(w, 1.0, 1.0), P);
        b.bx(v3(0.0, chassis_y + body_h + 2.5, l * 0.05), v3(w - 2.0, 1.0, cab_l - 2.0), P);
        crane_deck = None;
    }
    g.handle_feat(Cab);
    g.handle_feat(Windows);
    let hl = g.take_feat(Headlight);
    g.b.sym(|b, s| {
        if hl {
            headlight(b, v3(s * (w * 0.5 - 1.0), chassis_y + body_h * 0.5, front - 0.5), Vec3::NEG_Z, 1.5);
        } else {
            b.bx(v3(s * (w * 0.5 - 1.0), chassis_y + body_h * 0.5, front - 0.5), v3(1.0, 1.0, 1.0), C);
        }
        b.bx(v3(s * (w * 0.5 - 1.0), chassis_y + body_h * 0.5, -front + 0.5), v3(1.0, 1.0, 1.0), A);
    });
    // Wheels roll with distance.
    let big = g.feat(Wheels) && g.has(Heavy);
    let wd = if big { odd(wheel_d * 1.4) } else { wheel_d };
    for (x, z) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let c = v3(x * (w * 0.5), if big { (wd + 1.0) * 0.5 } else { axle_y }, z * (l * 0.5 - wheel_d * 0.6));
        wheel(&mut g.b, 0, c, wd, 2.0, WheelStyle::Tire, x + z);
    }
    g.handle_feat(Wheels);
    if g.take_feat(Exhaust) {
        exhaust_pipe(&mut g.b, 0, v3(w * 0.3, chassis_y - 0.5, -front + 1.0), Vec3::Z, 1.0, true);
    }
    if let Some(deck) = crane_deck {
        crane(&mut g.b, 0, deck, l * 0.7);
        g.handle_feat(CraneArm);
        g.handle_feat(Chains);
    }
    if g.has(Charge) && !g.feat(Plow) && !g.feat(Drill) && !g.feat(Saw) {
        let b = &mut g.b;
        b.bx(v3(0.0, chassis_y + 1.0, front - 1.5), v3(w + 2.0, 3.0, 1.0), A);
        b.sym(|b, s| {
            b.cone_to(v3(s * w * 0.3, chassis_y + 1.0, front - 2.0), v3(s * w * 0.3, chassis_y + 1.0, front - 5.0), 2.0, C);
        });
    }
    let _ = PI;
}

// --------------------------------------------------------------------------------- motorcycle

/// Wheels + Exhaust with no cab / windows / cockpit: a narrow two-wheeler — two in-line wheels, fork
/// and handlebars, fuel tank, seat, engine block and an exhaust along the right side. Front −Z.
fn motorcycle(g: &mut G) {
    let l = g.l.max(10.0);
    let wd = odd(l * 0.36).max(5.0);
    let r = wd * 0.5;
    let axle_y = r + 0.5;
    let bw = odd(l * 0.13).max(3.0);
    let (zf, zr) = (-l * 0.5 + r, l * 0.5 - r);
    let top = axle_y + r;
    let head = v3(0.0, top + wd * 0.35, zf + r * 0.55);
    let bar_y = head.y + 1.5;
    let b = &mut g.b;
    // Engine block (finned cylinders) + frame backbone between the wheels.
    let mid = (zf + zr) * 0.5 + l * 0.02;
    b.bx(v3(0.0, axle_y + 0.5, mid), v3(bw + 1.0, wd * 0.55, l * 0.3), D);
    b.bx(v3(0.0, axle_y + 0.5, mid), v3(bw + 2.0, 1.0, l * 0.18), A);
    b.bar(head, v3(0.0, top - 0.5, zr - r * 0.6), 1.5, D);
    // Tank, seat + seat cowl, rear fender with a tail light.
    b.el(v3(0.0, top + 1.0, -l * 0.07), v3(bw + 1.0, (wd * 0.5).max(2.5), l * 0.3), P);
    b.bx(v3(0.0, top + 1.0, l * 0.14), v3(bw, 1.5, l * 0.24), D);
    b.bx(v3(0.0, top + 1.5, l * 0.27), v3(bw, 2.0, l * 0.08), P);
    b.bx(v3(0.0, top + 0.5, zr), v3(bw, 1.0, wd * 0.9), P);
    b.bx(v3(0.0, top + 0.5, zr + r + 0.3), v3((bw - 1.0).max(1.0), 1.0, 1.0), C);
    // Fork legs down to the front axle, front fender, handlebars with grips.
    b.sym(|b, s| {
        b.bar(head + v3(s, 0.0, 0.0), v3(s, axle_y, zf), 1.0, A);
        b.bx(v3(s * (bw * 0.5 + 1.5), bar_y, head.z + 1.0), v3(1.0, 1.0, 2.0), D);
    });
    b.bx(v3(0.0, top + 0.5, zf), v3((bw - 1.0).max(1.0), 1.0, wd * 0.7), P);
    b.bar(head, v3(0.0, bar_y, head.z + 1.0), 1.0, D);
    b.bar(v3(-(bw * 0.5 + 1.0), bar_y, head.z + 1.0), v3(bw * 0.5 + 1.0, bar_y, head.z + 1.0), 1.0, A);
    // Fast: a sport fairing wrapped round the steering head (instead of the generic speed fins).
    if g.take(Fast) {
        let b = &mut g.b;
        b.el(head + v3(0.0, -0.8, -0.8), v3(bw + 1.0, (wd * 0.55).max(2.5), (r * 1.2).max(2.5)), A);
        b.bxr(head + v3(0.0, 1.2, -0.6), v3((bw - 1.0).max(1.0), 1.5, 1.0), v3(-30.0, 0.0, 0.0), C);
    }
    if g.take_feat(Headlight) {
        headlight(&mut g.b, head + v3(0.0, -0.5, -1.5), Vec3::NEG_Z, 1.5);
    } else {
        g.b.bx(head + v3(0.0, -0.5, -1.0), Vec3::ONE, C);
    }
    // In-line wheels roll with distance.
    for (z, boss) in [(zf, 0.0), (zr, 1.0)] {
        wheel(&mut g.b, 0, v3(0.0, axle_y, z), wd, 2.0, WheelStyle::Tire, boss);
    }
    g.handle_feat(Wheels);
    // Exhaust: a pipe from the engine along the right side, out past the rear axle.
    let ex = g.take_feat(Exhaust);
    let side = bw * 0.5 + 1.0;
    g.b.bar(v3(side - 0.5, axle_y, mid), v3(side, axle_y + 0.5, zr - r * 0.5), 1.0, D);
    exhaust_pipe(&mut g.b, 0, v3(side, axle_y + 0.5, zr + r * 0.3), v3(0.0, 0.25, 1.0), 1.0, ex);
    // The rider sits astride the saddle (no generic cockpit tub on top).
    seat(g, v3(0.0, top + 2.0, l * 0.12));
}

/// Declares the pilot seat (`SEAT_PART`) where the body wants it, so `Mechanic::Piloted` doesn't add
/// the generic cockpit tub on top.
fn seat(g: &mut G, at: Vec3) {
    if g.has(Piloted) && !g.b.spec.parts.iter().any(|p| p.id == SEAT_PART) {
        g.b.spec.parts.push(Part { id: SEAT_PART, parent: 0, pivot: at, role: PartRole::Other });
    }
}

// ---------------------------------------------------------------------------------- speedboat

/// Propeller on the water (no fins, no wheels, no flight): a speedboat — hull with a rounded bow,
/// chine stripe, gunwale, cockpit well with seats, raked windscreen, outboard motor + spinning
/// propeller at the stern. Front −Z.
fn speedboat(g: &mut G) {
    let l = g.l.max(12.0);
    let w = odd(l * 0.34).max(5.0);
    let hd = (l * 0.16).max(2.5);
    let y = hd * 0.5 + 0.5;
    let zs = l * 0.5;
    let b = &mut g.b;
    // Hull: square stern half + ellipsoid bow, chine stripe, gunwale, raised bow deck.
    b.bx(v3(0.0, y, l * 0.12), v3(w, hd, l * 0.76), P);
    b.el(v3(0.0, y, -l * 0.2), v3(w, hd, l * 0.6), P);
    b.el(v3(0.0, y - hd * 0.3, -l * 0.02), v3(w + 0.6, 1.0, l * 0.94), A);
    b.bx(v3(0.0, hd + 0.5, l * 0.14), v3(w + 0.4, 1.0, l * 0.72), D);
    b.el(v3(0.0, hd + 0.5, -l * 0.22), v3(w - 1.0, 1.5, l * 0.46), A);
    // Cockpit well (carved), two seats, raked windscreen with a frame.
    b.cut_bx(v3(0.0, hd + 0.5, l * 0.2), v3(w - 2.0, 2.0, l * 0.34));
    b.sym(|b, s| {
        b.bx(v3(s * (w * 0.25), hd, l * 0.16), v3((w * 0.3).max(1.0), 1.0, 1.5), D);
        b.bx(v3(s * (w * 0.25), hd + 1.0, l * 0.16 + 1.0), v3((w * 0.3).max(1.0), 2.0, 1.0), D);
    });
    b.bxr(v3(0.0, hd + 1.5, l * 0.01), v3(w - 1.0, 2.5, 1.0), v3(-35.0, 0.0, 0.0), C);
    b.bx(v3(0.0, hd + 2.6, l * 0.01 + 0.8), v3(w - 1.0, 1.0, 1.0), D);
    if g.take_feat(Windows) {
        g.b.sym(|b, s| window_row(b, s * (w * 0.5), y + 0.3, -l * 0.15, l * 0.3, 1.0));
    }
    if g.take_feat(Headlight) {
        headlight(&mut g.b, v3(0.0, hd + 1.0, -l * 0.44), Vec3::NEG_Z, 1.5);
    }
    // Fast: glowing speed stripes down both flanks (instead of the generic swept speed fins).
    if g.take(Fast) {
        g.b.sym(|b, s| {
            b.bx(v3(s * (w * 0.5 + 0.1), y + hd * 0.2, l * 0.02), v3(1.0, 1.0, l * 0.62), C);
        });
    }
    // Outboard motor on the transom: cowl, leg down into the water, spinning propeller.
    let mo = v3(0.0, hd + 1.0, zs + 1.2);
    let b = &mut g.b;
    b.bx(mo, v3(3.0, 3.5, 2.5), D);
    b.bx(mo + v3(0.0, 1.5, 0.0), v3(3.2, 1.0, 2.7), A);
    b.bar(mo + v3(0.0, -1.5, 0.3), v3(0.0, 1.0, zs + 1.5), 1.0, D);
    rotor(&mut g.b, 0, v3(0.0, 1.0, zs + 2.5), Vec3::Z, (hd * 0.45).max(1.5), 3, 26.0, A);
    g.handle_feat(Propeller);
    let ex = g.take_feat(Exhaust);
    exhaust_pipe(&mut g.b, 0, mo + v3(0.0, 1.0, 1.8), v3(0.0, 0.4, 1.0), 1.0, ex);
    // Wake spray off the stern when it moves.
    g.b.puff(PuffSpec { part: 0, at: v3(0.0, 0.5, zs + 1.0), dir: v3(0.0, 0.6, 1.0).normalize(), size: 1.4, per_rev: 0.0, wheel: None, idle_hz: 0.0, per_m: 0.5 });
    seat(g, v3(0.0, hd + 1.0, l * 0.2));
}

// ---------------------------------------------------------------------------------- submarine

/// Propeller without flight: a cigar-hulled submarine with conning tower, portholes and fins.
fn submarine(g: &mut G) {
    let l = g.l.max(10.0);
    let d = odd((l * 0.2).max(3.0));
    let y = d * 0.5 + 0.5;
    let b = &mut g.b;
    b.el(v3(0.0, y, 0.0), v3(d, d, l * 0.92), P);
    b.el(v3(0.0, y - d * 0.25, 0.0), v3(d + 0.4, d * 0.4, l * 0.85), A);
    let tower = v3(0.0, y + d * 0.5, -l * 0.08);
    b.bx(tower + Vec3::Y * (d * 0.3), v3((d * 0.4).max(2.0), d * 0.7, l * 0.16), P);
    b.bx(tower + Vec3::Y * (d * 0.65), v3((d * 0.4).max(2.0) + 1.0, 1.0, l * 0.16 + 1.0), D);
    b.cy(tower + v3(0.0, d * 0.9, -l * 0.04), 1.0, d * 0.5, D);
    b.bx(tower + v3(0.0, d * 1.15, -l * 0.04 - 0.5), v3(1.0, 1.0, 2.0), C);
    b.flat(PrimKind::Box, tower + v3(-d * 0.9, d * 0.35, 0.0), tower + v3(d * 0.9, d * 0.35, 0.0), 2.0, 1.0, Vec3::Y, A);
    g.handle_feat(Mast);
    if g.take_feat(Windows) | true {
        g.b.sym(|b, s| window_row(b, s * d * 0.5, y + 0.5, -l * 0.3, l * 0.25, 1.0));
    }
    let tail = v3(0.0, y, l * 0.46);
    fin(&mut g.b, tail - Vec3::Z * (l * 0.08), tail + v3(0.0, d * 0.8, l * 0.02), l * 0.08, Vec3::X);
    fin(&mut g.b, tail - Vec3::Z * (l * 0.08), tail + v3(0.0, -d * 0.8, l * 0.02), l * 0.08, Vec3::X);
    g.b.sym(|b, s| fin(b, tail - Vec3::Z * (l * 0.08), tail + v3(s * d * 0.9, 0.0, l * 0.02), l * 0.08, Vec3::Y));
    g.handle_feat(Fins);
    g.b.cz(tail + Vec3::Z * 1.0, 1.5, 2.0, D);
    rotor(&mut g.b, 0, tail + Vec3::Z * 2.5, Vec3::Z, (d * 0.55).max(2.0), 4, 12.0, A);
    g.handle_feat(Propeller);
    if g.take(Shoots) {
        g.b.sym(|b, s| {
            b.cyl(v3(s * d * 0.25, y - d * 0.1, -l * 0.46), Vec3::Z, 1.0, 1.0, C);
        });
    }
    g.handle(Charge);
}
