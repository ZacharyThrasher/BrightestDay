//! Creatures. Origin = feet centre, facing −Z, left = −X. Every creature gets real parts with pivots so
//! `anim` can drive them: Head (+Jaw), Arm*, Leg*, Wing*, chained Tail segments.

use bd_core::mechanics::Mechanic::*;
use bd_core::shape::{PartId, PartRole, PrimKind};
use glam::{Quat, Vec3};

use super::G;
use crate::build::{A, C, D, P, odd, v3};
use crate::parts::{HornStyle, TailSpec, TailTip, WingStyle, eyes, exhaust, horns, jaw, leg, pincer, tail, wing_pair};

/// Common mechanic add-ons for creatures with a body/head. `shoulder` = wing roots (x = half spacing),
/// `rump` = tail base, `head` = (part, top-front point, head width) for horns.
struct Anchors {
    shoulder: Vec3,
    wing_span: f32,
    wing_style: WingStyle,
    rump: Vec3,
    rump_r: f32,
    back_top: Vec3,
    head: Option<(PartId, Vec3, f32)>,
    size: f32,
}

fn creature_extras(g: &mut G, an: &Anchors) {
    if g.take(Flies) | g.take(Glide) {
        if !g.b.spec.parts.iter().any(|p| matches!(p.role, PartRole::WingLeft | PartRole::WingRight)) {
            wing_pair(&mut g.b, 0, an.shoulder, an.wing_span, an.wing_style, 18.0);
        }
    }
    if g.take(ArticulatedTail) && !g.b.spec.parts.iter().any(|p| p.role == PartRole::Tail) {
        tail(
            &mut g.b,
            0,
            &TailSpec {
                start: an.rump,
                dir: v3(0.0, -0.2, 1.0),
                segments: 5,
                length: an.size * 0.8,
                r0: an.rump_r,
                r1: (an.rump_r * 0.3).max(0.6),
                bend: Quat::from_rotation_x(-0.12),
                tip: TailTip::Spike,
                plates: Some((A, 3)),
                flat_y: 1.0,
            },
        );
    }
    if let Some((hp, top, hw)) = an.head {
        if g.take(Charge) {
            let cur = g.b.cur;
            g.b.on(hp);
            horns(&mut g.b, top, (hw * 0.35).max(1.0), (an.size * 0.22).max(3.0), HornStyle::Bull);
            g.b.on(cur);
        }
    }
    if g.take(Pierce) {
        // Spines along the back.
        for i in 0..4 {
            let p = an.back_top + v3(0.0, 0.0, i as f32 * an.size * 0.1);
            g.b.on(0).cone_to(p - Vec3::Y, p + v3(0.0, an.size * 0.14, an.size * 0.05), (an.size * 0.07).max(1.5), C);
        }
    }
    if g.take(Thrust) {
        let r = (an.size * 0.06).max(1.0);
        g.b.on(0);
        let at = an.back_top + v3(0.0, -an.size * 0.1, an.size * 0.05);
        exhaust(&mut g.b, 0, at + v3(-r * 1.5, 0.0, 0.0), v3(0.0, -0.6, 1.0), r);
        g.b.on(0);
        exhaust(&mut g.b, 0, at + v3(r * 1.5, 0.0, 0.0), v3(0.0, -0.6, 1.0), r);
    }
    g.b.on(0);
}

// ----------------------------------------------------------------------------------------- biped

pub(super) fn biped(g: &mut G) {
    let h = g.l;
    let variant = g.r.pick(4);
    let stomp = g.take(Stomp) | g.has(Heavy);
    // Proportions.
    let (leg_f, torso_f, head_f, tw_f, aw_f) = match variant {
        0 => (0.42, 0.36, 0.2, 0.42, 0.14),  // knight / robot
        1 => (0.34, 0.42, 0.14, 0.56, 0.22), // golem
        2 => (0.44, 0.3, 0.28, 0.32, 0.1),   // lanky alien
        _ => (0.4, 0.38, 0.19, 0.52, 0.17),  // brawler
    };
    let leg_h = (h * leg_f * g.r.range(0.88, 1.12)).round().max(2.0);
    let torso_h = (h * torso_f * g.r.range(0.9, 1.1)).round().max(2.0);
    let head_h = (h * head_f * g.r.range(0.85, 1.2)).round().max(2.0);
    let arm_k = g.r.range(0.85, 1.2);
    let crest = g.r.pick(4); // 0 none, 1 fin crest, 2 ears/horn stubs, 3 antenna
    let lw = (h * if stomp { 0.22 } else { 0.17 }).round().max(1.0);
    let lx = 0.5 + lw * 0.5;
    let tw = odd(h * tw_f * g.r.range(0.88, 1.15)).max(lw * 2.0 + 1.0);
    let td = odd(h * 0.22).max(lw);
    let aw = (h * aw_f).round().max(1.0);
    let hip_y = leg_h;
    let neck_y = hip_y + 1.0 + torso_h;

    // Torso (root).
    let b = &mut g.b;
    b.bx(v3(0.0, hip_y + 0.5, 0.0), v3(tw - 2.0, 2.0, td), D);
    b.bx(v3(0.0, hip_y + 1.5, 0.0), v3(tw - 2.0, 1.0, td + 1.0), A);
    let tc = v3(0.0, hip_y + 1.0 + torso_h * 0.5, 0.0);
    match variant {
        1 => {
            b.el(tc + v3(0.0, 0.5, 0.5), v3(tw, torso_h + 2.0, td + 2.0), P);
        }
        3 => {
            b.bx(tc + v3(0.0, torso_h * 0.15, 0.0), v3(tw, torso_h * 0.7, td), P);
            b.bx(tc - v3(0.0, torso_h * 0.3, 0.0), v3(tw - 2.0, torso_h * 0.4, td), P);
        }
        _ => {
            b.bx(tc, v3(tw, torso_h, td), P);
        }
    }
    // Chest plate + lantern emblem.
    b.bx(tc + v3(0.0, torso_h * 0.1, -td * 0.5), v3(tw - 2.0, torso_h * 0.45, 1.0), A);
    b.bx(tc + v3(0.0, torso_h * 0.12, -td * 0.5 - 0.5), v3(1.0, 1.0, 1.0), C);

    // Legs.
    for s in [-1.0f32, 1.0] {
        let role = if s < 0.0 { PartRole::LegBackLeft } else { PartRole::LegBackRight };
        let hip = v3(s * lx, hip_y, 0.0);
        leg(&mut g.b, 0, role, hip, None, v3(s * lx, 0.0, 0.0), lw, lw + if stomp { 2.0 } else { 1.0 }, Vec3::NEG_Z, PrimKind::Box);
        if stomp {
            g.b.bx(v3(s * lx, 0.5, -lw * 0.5 - 1.0), v3(lw + 2.0, 1.0, 2.0), C);
        }
    }

    // Arms.
    let arm_len = arm_k
        * match variant {
            1 => torso_h + leg_h * 0.6,
            2 => torso_h + leg_h * 0.3,
            _ => torso_h * 1.05,
        };
    let sh_y = neck_y - aw * 0.5;
    let grab = g.take(Grab);
    let blade = g.take(Blade);
    let guard = g.take(Guard);
    for s in [-1.0f32, 1.0] {
        let role = if s < 0.0 { PartRole::ArmLeft } else { PartRole::ArmRight };
        let ax = s * (tw * 0.5 + aw * 0.5);
        let b = &mut g.b;
        b.part(0, v3(ax, sh_y, 0.0), role);
        b.bx(v3(ax, sh_y - arm_len * 0.5 + aw * 0.5, 0.0), v3(aw, arm_len, aw), P);
        let hand = v3(ax, sh_y - arm_len + aw * 0.5, 0.0);
        let hs = if variant == 1 { aw + 2.0 } else { aw + 1.0 };
        b.bx(hand, v3(hs, hs, hs), D);
        if variant != 2 {
            b.bx(v3(ax, sh_y + 0.5, 0.0), v3(aw + 2.0, 2.0, aw + 2.0), A);
        }
        if grab {
            for k in [-1.0f32, 0.0, 1.0] {
                let base = hand + v3(k * hs * 0.3, -hs * 0.4, -hs * 0.4);
                b.cone_to(base, base + v3(k * 0.5, -h * 0.08, -h * 0.06), 1.4, C);
            }
        }
        if blade {
            b.flat(PrimKind::Box, v3(ax + s * aw * 0.5, sh_y - arm_len * 0.3, 0.0), v3(ax + s * aw * 0.5, sh_y - arm_len * 0.95, -h * 0.12), 1.0, h * 0.1, Vec3::X, C);
        }
        if guard && s < 0.0 {
            b.cx(v3(ax - aw * 0.5 - 0.5, sh_y - arm_len * 0.5, 0.0), (h * 0.3).max(3.0), 1.0, A);
            b.cx(v3(ax - aw * 0.5 - 1.0, sh_y - arm_len * 0.5, 0.0), 1.0, 1.0, C);
        }
    }

    // Head.
    let hp = g.b.part(0, v3(0.0, neck_y, 0.0), PartRole::Head);
    let breath = g.take(Breath);
    let b = &mut g.b;
    let hw = odd(head_h * if variant == 2 { 1.1 } else { 0.9 }).max(3.0);
    let hd = odd(head_h * 0.9).max(3.0);
    let hc = v3(0.0, neck_y + 0.5 + head_h * 0.5, -0.5);
    b.bx(v3(0.0, neck_y + 0.5, 0.0), v3(odd(hw * 0.5), 1.0, odd(hd * 0.5)), D);
    match variant {
        2 => {
            b.el(hc + v3(0.0, head_h * 0.15, 0.0), v3(hw + 1.0, head_h * 1.3, hd + 1.0), P);
            b.sym(|b, s| {
                b.bx(hc + v3(s * hw * 0.25, 0.0, -hd * 0.5), v3(2.0, 1.0, 1.0), C);
            });
            b.seg(PrimKind::Box, hc + v3(0.0, head_h * 0.7, 0.0), hc + v3(0.0, head_h * 1.3, 0.8), 1.0, 1.0, A);
            b.bx(hc + v3(0.0, head_h * 1.35, 0.8), v3(1.0, 1.0, 1.0), C);
        }
        0 => {
            b.bx(hc, v3(hw, head_h, hd), P);
            b.bx(hc + v3(0.0, 0.0, -hd * 0.5), v3(hw, 1.0, 1.0), D);
            eyes(b, hc + v3(0.0, 0.0, -hd * 0.5 - 0.1), (hw * 0.25).max(1.0), 1.0);
            b.bx(hc + v3(0.0, head_h * 0.5 + 0.5, 0.5), v3(1.0, 2.0, hd), A);
        }
        _ => {
            b.bx(hc, v3(hw, head_h, hd), P);
            eyes(b, hc + v3(0.0, head_h * 0.1, -hd * 0.5), (hw * 0.25).max(1.0), 1.0);
            b.bx(hc + v3(0.0, head_h * 0.5 - 0.5, -hd * 0.5 + 0.5), v3(hw, 1.0, 1.0), D);
        }
    }
    match crest {
        1 => {
            b.bx(hc + v3(0.0, head_h * 0.5 + 1.0, 0.5), v3(1.0, 2.0, hd), A);
        }
        2 => {
            b.sym(|b, sd| {
                b.cone_to(hc + v3(sd * hw * 0.4, head_h * 0.4, 0.0), hc + v3(sd * hw * 0.7, head_h * 0.9, 0.5), 1.5, A);
            });
        }
        3 => {
            b.seg(PrimKind::Box, hc + v3(hw * 0.3, head_h * 0.5, 0.0), hc + v3(hw * 0.4, head_h * 1.2, 0.5), 1.0, 1.0, D);
            b.bx(hc + v3(hw * 0.4, head_h * 1.25, 0.5), v3(1.0, 1.0, 1.0), C);
        }
        _ => {}
    }
    if breath {
        jaw(b, hp, hc + v3(0.0, -head_h * 0.25, hd * 0.3), hd * 0.9 + 1.0, hw - 1.0, (head_h * 0.35).max(1.0), 22.0, true);
        g.b.on(hp).bx(hc + v3(0.0, -head_h * 0.2, -hd * 0.3), v3(hw - 2.0, 1.0, 2.0), C);
    }
    g.b.on(0);
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(tw * 0.25 + 0.5, sh_y, td * 0.5 + 0.5),
            wing_span: h * 0.62,
            wing_style: if variant == 2 { WingStyle::Bat } else { WingStyle::Feather },
            rump: v3(0.0, hip_y + 1.0, td * 0.5),
            rump_r: (td * 0.3).max(1.0),
            back_top: v3(0.0, neck_y - 1.0, td * 0.5),
            head: Some((hp, hc + v3(0.0, head_h * 0.5, -hd * 0.25), hw)),
            size: h,
        },
    );
}

// ----------------------------------------------------------------------------------------- kaiju

pub(super) fn kaiju(g: &mut G) {
    let h = g.l;
    let variant = g.r.pick(3);
    let breath = g.take(Breath);
    g.handle(ArticulatedTail);
    g.handle(Stomp);
    let plate_tint = if breath { C } else { A };
    let th = g.r.range(0.95, 1.12); // thigh bulk
    let snout = g.r.range(0.9, 1.25);
    let leg_y = h * 0.36;
    let hip_x = h * 0.15;

    // Torso (root): pear-shaped, leaning forward.
    let b = &mut g.b;
    b.elr(v3(0.0, h * 0.55, h * 0.03), v3(h * 0.42, h * 0.52, h * 0.38), v3(-12.0, 0.0, 0.0), P);
    b.elr(v3(0.0, h * 0.43, h * 0.06), v3(h * 0.46, h * 0.3, h * 0.4), v3(0.0, 0.0, 0.0), P);
    // Belly plate with ridges.
    b.elr(v3(0.0, h * 0.53, -h * 0.1), v3(h * 0.3, h * 0.42, h * 0.16), v3(-12.0, 0.0, 0.0), A);
    for i in 0..5 {
        let y = h * (0.4 + i as f32 * 0.065);
        b.bxr(v3(0.0, y, -h * 0.15 - (i as f32 * 0.012 * h)), v3(h * 0.26, 1.0, h * 0.1), v3(-12.0, 0.0, 0.0), D);
    }
    // Neck.
    b.elr(v3(0.0, h * 0.78, -h * 0.1), v3(h * 0.24, h * 0.2, h * 0.22), v3(-35.0, 0.0, 0.0), P);

    // Dorsal plates along the spine (root part) — the kaiju read.
    let spine: Vec<Vec3> = (0..6)
        .map(|i| {
            let f = i as f32 / 5.0;
            v3(0.0, h * (0.84 - 0.36 * f), h * (0.02 + 0.2 * f))
        })
        .collect();
    for (i, p) in spine.iter().enumerate() {
        let f = i as f32 / 5.0;
        let s = h * (0.12 + 0.07 * (std::f32::consts::PI * (0.2 + 0.8 * f)).sin());
        let tilt = -20.0 - 30.0 * f; // lean back along the spine
        match variant {
            1 => {
                b.cor(*p + v3(0.0, s * 0.3, s * 0.2), v3(s * 0.4, s, s * 0.4), v3(tilt + 20.0, 0.0, 0.0), plate_tint);
            }
            _ => {
                b.bxr(*p + v3(0.0, s * 0.25, s * 0.25), v3(h * 0.03 + 1.0, s, s), v3(45.0 + tilt, 0.0, 0.0), plate_tint);
                b.sym(|b, sd| {
                    b.bxr(*p + v3(sd * h * 0.07, s * 0.05, s * 0.2), v3(h * 0.025 + 1.0, s * 0.6, s * 0.6), v3(45.0 + tilt, 0.0, sd * 18.0), plate_tint);
                });
            }
        }
    }
    if variant == 2 {
        // Armoured back shell.
        b.elr(v3(0.0, h * 0.62, h * 0.12), v3(h * 0.4, h * 0.34, h * 0.3), v3(-20.0, 0.0, 0.0), A);
    }

    // Legs: massive thighs, pillar shins, clawed feet.
    for s in [-1.0f32, 1.0] {
        let role = if s < 0.0 { PartRole::LegBackLeft } else { PartRole::LegBackRight };
        let hip = v3(s * hip_x, leg_y, h * 0.04);
        g.b.part(0, hip, role);
        let b = &mut g.b;
        b.el(hip + v3(s * h * 0.02, -h * 0.07, -h * 0.02), v3(h * 0.2 * th, h * 0.3, h * 0.24 * th), P);
        let knee = v3(s * (hip_x + h * 0.03), h * 0.14, h * 0.0);
        let ankle = v3(s * (hip_x + h * 0.03), h * 0.03, h * 0.04);
        b.segx(PrimKind::Cylinder, knee, ankle, h * 0.13, h * 0.02, P);
        b.bx(v3(s * (hip_x + h * 0.03), h * 0.035, -h * 0.03), v3(h * 0.17, h * 0.07, h * 0.24), D);
        for k in [-1.0f32, 0.0, 1.0] {
            let base = v3(s * (hip_x + h * 0.03) + k * h * 0.055, h * 0.03, -h * 0.14);
            b.cone_to(base, base + v3(k * h * 0.01, -h * 0.01, -h * 0.06), h * 0.045, C);
        }
    }

    // Small arms held forward.
    for s in [-1.0f32, 1.0] {
        let role = if s < 0.0 { PartRole::ArmLeft } else { PartRole::ArmRight };
        let sh = v3(s * h * 0.17, h * 0.68, -h * 0.1);
        g.b.part(0, sh, role);
        let b = &mut g.b;
        let el = sh + v3(s * h * 0.04, -h * 0.1, -h * 0.05);
        let hand = el + v3(-s * h * 0.01, -h * 0.02, -h * 0.1);
        b.segx(PrimKind::Cylinder, sh, el, h * 0.075, h * 0.02, P);
        b.segx(PrimKind::Cylinder, el, hand, h * 0.06, h * 0.01, P);
        b.el(hand, Vec3::splat(h * 0.07), D);
        for k in [-1.0f32, 0.0, 1.0] {
            let base = hand + v3(k * h * 0.02, -h * 0.02, -h * 0.02);
            b.cone_to(base, base + v3(0.0, -h * 0.04, -h * 0.03), h * 0.025, C);
        }
    }

    // Head + jaw.
    let neck_top = v3(0.0, h * 0.84, -h * 0.13);
    let hp = g.b.part(0, neck_top, PartRole::Head);
    let b = &mut g.b;
    let hc = v3(0.0, h * 0.885, -h * 0.25);
    let hl = h * 0.22 * snout;
    b.el(hc + v3(0.0, 0.0, h * 0.03), v3(h * 0.17, h * 0.13, h * 0.16), P);
    b.elr(hc + v3(0.0, -h * 0.005, -hl * 0.35), v3(h * 0.13, h * 0.09, hl), v3(8.0, 0.0, 0.0), P);
    b.bx(hc + v3(0.0, h * 0.045, -h * 0.03), v3(h * 0.15, h * 0.03, h * 0.08), D);
    eyes(b, hc + v3(0.0, h * 0.025, -h * 0.085), h * 0.055, (h * 0.025).max(1.0));
    // Upper teeth.
    for k in -2..=2 {
        let base = hc + v3(k as f32 * h * 0.022, -h * 0.045, -hl * 0.35 - hl * 0.3);
        b.cone_to(base, base + v3(0.0, -h * 0.03, 0.0), h * 0.018 + 0.6, C);
    }
    match variant {
        1 => horns(b, hc + v3(0.0, h * 0.05, -hl * 0.4), 0.0, h * 0.14, HornStyle::Rhino),
        2 => horns(b, hc + v3(0.0, h * 0.06, h * 0.02), h * 0.05, h * 0.12, HornStyle::Devil),
        _ => {}
    }
    if breath {
        b.el(hc + v3(0.0, -h * 0.03, -h * 0.05), v3(h * 0.08, h * 0.03, h * 0.12), C);
    }
    jaw(b, hp, hc + v3(0.0, -h * 0.035, h * 0.04), hl * 0.95, h * 0.11, h * 0.045, if breath { 14.0 } else { 5.0 }, true);

    // Long multi-segment tail (plated), sweeping to the ground and curling.
    g.b.on(0);
    tail(
        &mut g.b,
        0,
        &TailSpec {
            start: v3(0.0, h * 0.4, h * 0.2),
            dir: v3(0.0, -0.42, 1.0),
            segments: 7,
            length: h * 1.0,
            r0: h * 0.12,
            r1: h * 0.028,
            bend: Quat::from_rotation_x(-0.075) * Quat::from_rotation_y(0.1),
            tip: TailTip::Spike,
            plates: Some((plate_tint, 5)),
            flat_y: 1.0,
        },
    );
    g.b.on(0);
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(h * 0.12, h * 0.72, h * 0.12),
            wing_span: h * 0.7,
            wing_style: WingStyle::Bat,
            rump: v3(0.0, h * 0.4, h * 0.2),
            rump_r: h * 0.1,
            back_top: v3(0.0, h * 0.74, h * 0.14),
            head: Some((hp, hc + v3(0.0, h * 0.06, -h * 0.02), h * 0.15)),
            size: h,
        },
    );
}

// ------------------------------------------------------------------------------------- quadruped

pub(super) fn quadruped(g: &mut G) {
    let h = g.l;
    let arch = g.r.pick(5); // 0 canine, 1 bovine, 2 equine, 3 feline, 4 ursine/rodent
    let r = &mut g.r;
    let (leg_f, body_hf, body_lf, neck_f, snout_f, ear_f, tail_f) = match arch {
        0 => (r.range(0.36, 0.44), r.range(0.26, 0.3), r.range(1.0, 1.2), r.range(0.15, 0.25), r.range(0.28, 0.38), r.range(0.14, 0.22), r.range(0.35, 0.5)),
        1 => (r.range(0.3, 0.36), r.range(0.36, 0.42), r.range(1.1, 1.3), r.range(0.05, 0.1), r.range(0.22, 0.3), r.range(0.06, 0.1), r.range(0.3, 0.45)),
        2 => (r.range(0.44, 0.5), r.range(0.26, 0.3), r.range(1.05, 1.2), r.range(0.3, 0.38), r.range(0.34, 0.42), r.range(0.1, 0.16), r.range(0.35, 0.5)),
        3 => (r.range(0.3, 0.36), r.range(0.24, 0.28), r.range(1.15, 1.35), r.range(0.1, 0.16), r.range(0.18, 0.24), r.range(0.1, 0.16), r.range(0.6, 0.8)),
        _ => (r.range(0.24, 0.3), r.range(0.38, 0.46), r.range(0.9, 1.05), r.range(0.04, 0.08), r.range(0.16, 0.24), r.range(0.1, 0.14), r.range(0.1, 0.18)),
    };
    let stomp = g.take(Stomp) | g.has(Heavy);
    let leg_h = (h * leg_f).round().max(2.0);
    let body_h = (h * body_hf).max(3.0);
    let body_l = (h * body_lf).max(5.0);
    let body_w = odd(body_h * if arch == 1 || arch == 4 { 1.05 } else { 0.8 }).max(3.0);
    let lw = (h * if stomp { 0.2 } else if arch == 2 || arch == 3 { 0.12 } else { 0.15 }).round().max(1.0);
    let by = leg_h + body_h * 0.5 - 0.5;
    let lx = (body_w * 0.5 - lw * 0.5).max(1.0);
    let lz = body_l * 0.34;

    let b = &mut g.b;
    // Body.
    b.el(v3(0.0, by, 0.0), v3(body_w, body_h, body_l), P);
    b.el(v3(0.0, by + body_h * 0.05, -body_l * 0.25), v3(body_w + 1.0, body_h * 1.08, body_l * 0.5), P);
    if arch == 1 {
        b.el(v3(0.0, by + body_h * 0.4, -body_l * 0.2), v3(body_w * 0.8, body_h * 0.6, body_l * 0.35), P);
    }
    // Back stripe / saddle.
    b.el(v3(0.0, by + body_h * 0.45, 0.0), v3(odd(body_w * 0.35), 1.0, body_l * 0.7), A);
    // Belly.
    b.el(v3(0.0, by - body_h * 0.42, 0.0), v3(body_w * 0.6, 1.0, body_l * 0.6), D);

    // Legs.
    for (i, (s, zs)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)].into_iter().enumerate() {
        let role = match i {
            0 => PartRole::LegFrontLeft,
            1 => PartRole::LegFrontRight,
            2 => PartRole::LegBackLeft,
            _ => PartRole::LegBackRight,
        };
        let hip = v3(s * lx, leg_h + 0.5, zs * lz);
        let w = if zs > 0.0 && arch != 4 { lw + 1.0 } else { lw };
        leg(&mut g.b, 0, role, hip, None, v3(s * lx, 0.0, zs * lz), w, (lw + 1.0).max(2.0), Vec3::NEG_Z, PrimKind::Box);
        if zs > 0.0 && arch != 4 {
            // Haunch.
            g.b.el(hip + v3(0.0, body_h * 0.1, 0.0), v3(w + 1.0, body_h * 0.6, (w + 2.0).max(3.0)), P);
        }
    }

    // Head + neck (one part pivoting at the neck base).
    let neck_base = v3(0.0, by + body_h * 0.25, -body_l * 0.44);
    let neck_len = (h * neck_f).max(1.0);
    let neck_dir = if arch == 2 { v3(0.0, 1.0, -0.55) } else { v3(0.0, 0.7, -1.0) }.normalize();
    let head_base = neck_base + neck_dir * neck_len;
    let hp = g.b.part(0, neck_base, PartRole::Head);
    let hs = (h * 0.24).max(3.0);
    let snout = (h * snout_f).max(2.0);
    let hw = odd(hs * if arch == 1 || arch == 4 { 1.1 } else { 0.85 });
    let b = &mut g.b;
    if neck_len > 1.5 {
        b.segx(PrimKind::Ellipsoid, neck_base, head_base, (hs * 0.8).max(2.0), 1.0, P);
        if arch == 2 {
            // Mane.
            b.seg(PrimKind::Box, neck_base + v3(0.0, hs * 0.4, 1.0), head_base + v3(0.0, hs * 0.45, 0.8), 1.0, 2.0, A);
        }
    }
    let hc = head_base + v3(0.0, hs * 0.1, -hs * 0.2);
    b.el(hc, v3(hw, hs, hs), P);
    let head_tilt = if arch == 2 { 35.0 } else { 12.0 };
    let sd = Quat::from_rotation_x(-(head_tilt as f32).to_radians()) * Vec3::NEG_Z;
    let sc = hc + sd * (hs * 0.35 + snout * 0.5) + v3(0.0, -hs * 0.15, 0.0);
    let snout_w = odd(hw * if arch == 3 { 0.7 } else { 0.6 });
    b.q(PrimKind::Box, sc, v3(snout_w, hs * 0.5, snout), Quat::from_rotation_x(-(head_tilt as f32).to_radians()), P);
    b.bx(sc + sd * snout * 0.5 + v3(0.0, hs * 0.1, 0.0), v3(snout_w.min(3.0), 1.0, 1.0), D);
    eyes(b, hc + v3(0.0, hs * 0.15, -hs * 0.45), (hw * 0.28).max(1.0), 1.0);
    // Ears.
    let ear = (h * ear_f).max(1.0);
    b.sym(|b, s| {
        let base = hc + v3(s * hw * 0.3, hs * 0.4, hs * 0.1);
        match arch {
            0 | 3 => {
                b.cone_to(base, base + v3(s * ear * 0.2, ear, ear * 0.15), (ear * 0.7).max(1.5), A);
            }
            1 => {
                b.seg(PrimKind::Box, base + v3(0.0, -hs * 0.2, 0.0), base + v3(s * ear * 1.6, -hs * 0.15, 0.2), 1.0, 2.0, A);
            }
            2 => {
                b.cone_to(base, base + v3(0.0, ear, 0.5), 1.5, A);
            }
            _ => {
                b.cz(base + v3(0.0, 0.3, 0.0), (ear * 1.6).max(2.0), 1.0, A);
            }
        }
    });
    let breath = g.take(Breath);
    let grab = g.take(Grab);
    let jaw_open = if breath { 24.0 } else if grab { 12.0 } else { 6.0 };
    let hinge = hc + v3(0.0, -hs * 0.3, -hs * 0.1);
    jaw(&mut g.b, hp, hinge, snout + hs * 0.35, (snout_w - 1.0).max(1.0), (hs * 0.25).max(1.0), jaw_open, breath || grab || arch == 0 || arch == 3);
    if breath {
        g.b.on(hp).bx(hinge + v3(0.0, 0.3, -1.5), v3((snout_w - 2.0).max(1.0), 1.0, 2.0), C);
    }
    let head_top = hc + v3(0.0, hs * 0.45, -hs * 0.1);
    if arch == 1 && !g.has(Charge) {
        let style = if g.r.chance(0.5) { HornStyle::Bull } else { HornStyle::Ram };
        g.b.on(hp);
        horns(&mut g.b, head_top, (hw * 0.35).max(1.0), (h * 0.14).max(2.0), style);
    }

    // Tail.
    g.b.on(0);
    let tail_len = (h * tail_f).max(2.0);
    let long_tail = g.take(ArticulatedTail);
    let (dir, bend, tip, segs) = match arch {
        0 => (v3(0.0, 0.8, 1.0), Quat::from_rotation_x(0.2), TailTip::None, 2),
        1 => (v3(0.0, -0.5, 1.0), Quat::from_rotation_x(0.35), TailTip::Rattle, 2),
        2 => (v3(0.0, -0.6, 1.0), Quat::from_rotation_x(0.25), TailTip::None, 2),
        3 => (v3(0.0, 0.2, 1.0), Quat::from_rotation_x(-0.35), TailTip::None, 3),
        _ => (v3(0.0, 0.4, 1.0), Quat::IDENTITY, TailTip::None, 1),
    };
    let tr = if arch == 2 { (h * 0.07).max(1.0) } else { (h * 0.05).max(0.6) };
    tail(
        &mut g.b,
        0,
        &TailSpec {
            start: v3(0.0, by + body_h * 0.3, body_l * 0.46),
            dir: if long_tail { v3(0.0, 0.3, 1.0) } else { dir },
            segments: if long_tail { 5 } else { segs },
            length: if long_tail { h * 1.1 } else { tail_len },
            r0: if long_tail { (h * 0.09).max(1.0) } else { tr },
            r1: if long_tail { (h * 0.04).max(0.6) } else { tr * 0.8 },
            bend: if long_tail { Quat::from_rotation_x(-0.15) } else { bend },
            tip: if long_tail { TailTip::Club } else { tip },
            plates: if long_tail { Some((A, 4)) } else { None },
            flat_y: 1.0,
        },
    );
    g.b.on(0);
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(body_w * 0.3, by + body_h * 0.4, -body_l * 0.15),
            wing_span: h * 0.9,
            wing_style: if arch == 2 { WingStyle::Feather } else { WingStyle::Bat },
            rump: v3(0.0, by, body_l * 0.45),
            rump_r: (h * 0.06).max(1.0),
            back_top: v3(0.0, by + body_h * 0.5, -body_l * 0.2),
            head: Some((hp, head_top, hw)),
            size: h,
        },
    );
}

// --------------------------------------------------------------------------------------- serpent

pub(super) fn serpent(g: &mut G) {
    let len = g.l;
    let variant = g.r.pick(3); // 0 snake, 1 cobra (raised hood), 2 sea-serpent/dragon (fins)
    g.handle(ArticulatedTail);
    g.handle(Constrict);
    let d = (len * g.r.range(0.1, 0.14)).max(3.0);
    let n = 9usize;
    let head_l = d * 1.7;
    let body_len = len - head_l;
    let sl = body_len / n as f32;
    let raised = variant == 1;
    // Root segment: behind the head. Rest pose: gentle S on the ground.
    let z0 = -len * 0.5 + head_l;
    let root_start = if raised { v3(0.0, len * 0.28, z0 + sl * 0.5) } else { v3(0.0, d * 0.5, z0) };
    let root_dir = if raised { v3(0.0, -0.9, 0.55).normalize() } else { v3(0.18, 0.0, 1.0).normalize() };
    let b = &mut g.b;
    let q0 = crate::parts::basis_quat(root_dir, Vec3::Y);
    let root_end = root_start + root_dir * (if raised { len * 0.3 } else { sl });
    b.q(PrimKind::Ellipsoid, (root_start + root_end) * 0.5, v3(d, (root_end - root_start).length() * 1.35, d), q0, P);
    if raised {
        // Hood.
        let hc = root_start + root_dir * (d * 0.9);
        b.q(PrimKind::Ellipsoid, hc + v3(0.0, 0.0, 0.3), v3(d * 3.0, d * 2.4, 1.5), Quat::from_rotation_x(-0.3), A);
        b.q(PrimKind::Ellipsoid, hc + v3(0.0, 0.0, -0.2), v3(d * 2.2, d * 1.6, 1.0), Quat::from_rotation_x(-0.3), P);
        b.sym(|b, s| {
            b.bx(hc + v3(s * d * 0.6, d * 0.1, -0.5), v3(1.0, 1.0, 1.0), C);
        });
    }
    // Body chain (rest pose wiggle).
    let mut pos = root_end;
    let mut prev = 0;
    for k in 0..n {
        let f = k as f32 / n as f32;
        let yaw = 0.25 * ((k as f32) * 1.1 + if raised { 0.8 } else { 1.9 }).sin();
        let dir = Quat::from_rotation_y(yaw) * Vec3::Z;
        let dir = v3(dir.x, 0.0, dir.z).normalize();
        let r = (d * 0.5 * (1.0 - 0.7 * f)).max(0.9);
        let id = g.b.part(prev, pos, PartRole::Tail);
        let b = &mut g.b;
        let end = pos + dir * sl;
        let q = crate::parts::basis_quat(dir, Vec3::Y);
        let c = (pos + end) * 0.5;
        let cy = if raised || k > 0 { r.max(0.5) } else { c.y };
        let c = v3(c.x, cy, c.z);
        b.q(PrimKind::Ellipsoid, c, v3((2.0 * r).max(1.0), sl * 1.8, (2.0 * r).max(1.0)), q, P);
        if r > 1.0 {
            b.q(PrimKind::Ellipsoid, c + v3(0.0, r * 0.75, 0.0), v3(r.max(1.0), sl * 0.5, 1.0), q, A);
        }
        if variant == 2 && k % 2 == 0 && k < n - 2 {
            b.flat(PrimKind::Cone, c + v3(0.0, r * 0.6, 0.0), c + v3(0.0, r + d * 0.5, sl * 0.3), sl * 0.8, 1.0, Vec3::X, A);
        }
        prev = id;
        pos = v3(end.x, if raised || k > 0 { r.max(0.5) } else { end.y }, end.z);
    }
    // Tip.
    let tip_tint = if variant == 0 { A } else { C };
    if variant == 0 && g.r.chance(0.5) {
        for i in 0..3 {
            g.b.el(pos + v3(0.0, 0.0, i as f32 * 1.2), v3(1.5, 1.5, 1.5), if i % 2 == 0 { A } else { C });
        }
    } else {
        g.b.cone_to(pos - v3(0.0, 0.0, 0.5), pos + v3(0.0, 0.0, d * 0.8 + 1.0), 1.2, tip_tint);
    }

    // Head (child of root) + jaw.
    let hp = g.b.part(0, root_start, PartRole::Head);
    let breath = g.take(Breath);
    let b = &mut g.b;
    let hdir = if raised { v3(0.0, -0.2, -1.0).normalize() } else { v3(-0.1, 0.0, -1.0).normalize() };
    let hq = crate::parts::basis_quat(Vec3::Y, -hdir);
    let hc = root_start + hdir * (head_l * 0.4) + v3(0.0, d * 0.05, 0.0);
    b.q(PrimKind::Ellipsoid, hc, v3(d * 1.35, d * 0.8, head_l), hq, P);
    b.q(PrimKind::Box, hc + v3(0.0, d * 0.35, 0.0), v3(odd(d * 0.5), 1.0, head_l * 0.6), hq, A);
    b.sym(|b, s| {
        b.bx(hc + v3(s * d * 0.45, d * 0.25, 0.0) + hdir * (head_l * 0.12), v3(1.0, 1.0, 1.0), C);
    });
    if variant == 2 {
        horns(b, hc + v3(0.0, d * 0.35, 0.0) - hdir * head_l * 0.3, d * 0.3, d * 1.3, HornStyle::Devil);
    }
    let hinge = hc - hdir * (head_l * 0.4) + v3(0.0, -d * 0.2, 0.0);
    let jid = jaw(b, hp, hinge, head_l * 0.9, (d * 0.9).max(1.0), (d * 0.3).max(1.0), if breath { 30.0 } else { 10.0 }, true);
    // Forked tongue.
    g.b.on(jid);
    let tip = hinge + hdir * (head_l * 1.05) + v3(0.0, 0.3, 0.0);
    g.b.sym(|b, s| {
        b.seg(PrimKind::Box, tip - hdir * 1.5, tip + hdir * 1.2 + v3(s * 0.8, 0.0, 0.0), 1.0, 1.0, C);
    });
    g.b.on(0);
    let mid = root_end;
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(d * 0.4, mid.y + d * 0.3, mid.z),
            wing_span: len * 0.35,
            wing_style: WingStyle::Bat,
            rump: pos,
            rump_r: 1.0,
            back_top: mid + v3(0.0, d * 0.5, 0.0),
            head: Some((hp, hc + v3(0.0, d * 0.4, 0.0) - hdir * head_l * 0.2, d)),
            size: len * 0.5,
        },
    );
}

// ------------------------------------------------------------------------------------------ bird

pub(super) fn bird(g: &mut G) {
    let h = g.l;
    let variant = g.r.pick(4); // 0 eagle, 1 owl, 2 swallow, 3 bat
    g.handle(Flies);
    g.handle(Glide);
    let bw = (h * if variant == 1 { 0.55 } else { 0.42 }).max(3.0);
    let bh = (h * if variant == 1 { 0.6 } else { 0.42 }).max(3.0);
    let bl = (h * if variant == 1 { 0.6 } else { 0.8 }).max(4.0);
    let by = h * 0.55;
    let b = &mut g.b;
    b.elr(v3(0.0, by, 0.0), v3(bw, bh, bl), v3(-10.0, 0.0, 0.0), P);
    b.elr(v3(0.0, by - bh * 0.15, -bl * 0.1), v3(bw * 0.8, bh * 0.6, bl * 0.6), v3(-10.0, 0.0, 0.0), if variant == 3 { D } else { A });
    // Legs (tucked under).
    for s in [-1.0f32, 1.0] {
        let role = if s < 0.0 { PartRole::LegBackLeft } else { PartRole::LegBackRight };
        let hip = v3(s * (bw * 0.25).max(1.0), by - bh * 0.3, bl * 0.05);
        leg(&mut g.b, 0, role, hip, None, v3(s * (bw * 0.25).max(1.0), 0.0, bl * 0.1), 1.0, 2.0, Vec3::NEG_Z, PrimKind::Box);
        let b = &mut g.b;
        b.cone_to(v3(s * (bw * 0.25).max(1.0), 0.5, bl * 0.1 - 1.5), v3(s * (bw * 0.25).max(1.0), 0.0, bl * 0.1 - 2.5), 1.2, C);
    }
    // Wings.
    let (span, style) = match variant {
        0 => (h * g.r.range(1.25, 1.45), WingStyle::Feather),
        1 => (h * 1.0, WingStyle::Feather),
        2 => (h * 1.5, WingStyle::Feather),
        _ => (h * 1.25, WingStyle::Bat),
    };
    wing_pair(&mut g.b, 0, v3(bw * 0.35, by + bh * 0.25, -bl * 0.08), span, style, 14.0);
    // Tail.
    g.b.on(0);
    if variant == 3 {
        // bats: no tail fan.
    } else {
        tail(
            &mut g.b,
            0,
            &TailSpec {
                start: v3(0.0, by, bl * 0.4),
                dir: v3(0.0, -0.1, 1.0),
                segments: 1,
                length: (h * if variant == 2 { 0.5 } else { 0.3 }).max(2.0),
                r0: (bw * 0.25).max(0.8),
                r1: 0.6,
                bend: Quat::IDENTITY,
                tip: TailTip::Fan,
                plates: None,
                flat_y: 0.5,
            },
        );
        if variant == 2 {
            // Forked swallow tail.
            let b = &mut g.b;
            let base = v3(0.0, by, bl * 0.5 + h * 0.3);
            b.sym(|b, s| {
                b.flat(PrimKind::Box, base, base + v3(s * h * 0.12, 0.0, h * 0.45), 1.5, 1.0, Vec3::Y, A);
            });
        }
    }
    // Head.
    let neck = v3(0.0, by + bh * 0.3, -bl * 0.35);
    let hp = g.b.part(0, neck, PartRole::Head);
    let breath = g.take(Breath);
    let b = &mut g.b;
    let hs = (h * if variant == 1 { 0.45 } else { 0.32 }).max(3.0);
    let hc = neck + v3(0.0, hs * 0.35, -hs * 0.3);
    b.el(hc, v3(hs, hs, hs), if variant == 0 { A } else { P });
    let beak_base = hc + v3(0.0, -hs * 0.05, -hs * 0.45);
    match variant {
        3 => {
            b.sym(|b, s| {
                b.cone_to(hc + v3(s * hs * 0.3, hs * 0.3, 0.0), hc + v3(s * hs * 0.5, hs * 0.95, 0.3), 1.5, A);
            });
            eyes(b, hc + v3(0.0, hs * 0.1, -hs * 0.48), (hs * 0.22).max(1.0), 1.0);
            jaw(b, hp, hc + v3(0.0, -hs * 0.25, 0.0), hs * 0.55, (hs * 0.6).max(1.0), 1.0, if breath { 25.0 } else { 8.0 }, true);
        }
        _ => {
            let bl2 = (hs * if variant == 1 { 0.35 } else { 0.7 }).max(1.5);
            b.cone_to(beak_base, beak_base + v3(0.0, -bl2 * 0.15, -bl2), (hs * 0.45).max(1.5), A);
            if variant == 0 {
                b.bx(beak_base + v3(0.0, -1.0, -bl2 + 0.5), v3(1.0, 1.0, 1.0), A);
            }
            let es = if variant == 1 { 2.0 } else { 1.0 };
            eyes(b, hc + v3(0.0, hs * 0.12, -hs * 0.46), (hs * 0.25).max(1.0), es);
            if variant == 1 {
                b.sym(|b, s| {
                    b.cone_to(hc + v3(s * hs * 0.3, hs * 0.35, 0.0), hc + v3(s * hs * 0.45, hs * 0.8, 0.2), 1.5, A);
                });
            }
            if breath {
                jaw(b, hp, beak_base + v3(0.0, -0.5, 0.5), bl2, (hs * 0.3).max(1.0), 1.0, 25.0, false);
            }
        }
    }
    g.b.on(0);
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(bw * 0.35, by + bh * 0.25, 0.0),
            wing_span: span,
            wing_style: style,
            rump: v3(0.0, by, bl * 0.45),
            rump_r: (bw * 0.2).max(0.8),
            back_top: v3(0.0, by + bh * 0.5, 0.0),
            head: Some((hp, hc + v3(0.0, hs * 0.45, 0.0), hs)),
            size: h,
        },
    );
}

// ------------------------------------------------------------------------------------------ fish

pub(super) fn fish(g: &mut G) {
    let len = g.l;
    let variant = g.r.pick(4); // 0 shark, 1 goldfish, 2 piranha, 3 anglerfish
    let (bh_f, bw_f, bl_f) = match variant {
        0 => (0.26, 0.2, 0.62),
        1 => (0.42, 0.24, 0.5),
        2 => (0.46, 0.2, 0.52),
        _ => (0.42, 0.34, 0.5),
    };
    let bh = (len * bh_f).max(3.0);
    let bw = odd(len * bw_f).max(3.0);
    let bl = (len * bl_f).max(4.0);
    let by = bh * 0.5 + len * 0.1;
    let zc = -len * 0.12;
    let b = &mut g.b;
    b.el(v3(0.0, by, zc), v3(bw, bh, bl), P);
    b.el(v3(0.0, by - bh * 0.22, zc), v3(bw - 0.5, bh * 0.5, bl * 0.9), A);
    let nose_z = zc - bl * 0.5;
    if variant == 0 {
        b.cone_to(v3(0.0, by + bh * 0.05, nose_z + bl * 0.2), v3(0.0, by + bh * 0.08, nose_z - len * 0.1), bh * 0.8, P);
        for i in 0..3 {
            b.bx(v3(0.0, by, nose_z + bl * 0.3 + i as f32 * 1.2), v3(bw + 0.5, bh * 0.35, 1.0), D);
        }
    }
    if variant == 3 {
        // Lure.
        let base = v3(0.0, by + bh * 0.45, nose_z + bl * 0.2);
        b.seg(PrimKind::Box, base, base + v3(0.0, len * 0.18, -len * 0.08), 1.0, 1.0, A);
        b.seg(PrimKind::Box, base + v3(0.0, len * 0.18, -len * 0.08), base + v3(0.0, len * 0.15, -len * 0.2), 1.0, 1.0, A);
        b.el(base + v3(0.0, len * 0.14, -len * 0.22), v3(2.0, 2.0, 2.0), C);
    }
    // Dorsal fin.
    let dh = len * if variant == 0 { 0.28 } else { 0.16 };
    let dz = zc - bl * 0.05;
    b.flat(PrimKind::Cone, v3(0.0, by + bh * 0.35, dz), v3(0.0, by + bh * 0.4 + dh, dz + dh * 0.5), (bl * 0.35).max(2.0), 1.0, Vec3::X, A);
    // Pectoral fins.
    b.sym(|b, s| {
        let base = v3(s * bw * 0.45, by - bh * 0.15, zc - bl * 0.15);
        b.flat(PrimKind::Box, base, base + v3(s * len * 0.12, -len * 0.08, len * 0.08), (len * 0.07).max(1.5), 1.0, v3(0.0, 1.0, 0.0), A);
    });
    // Eyes.
    let ey = by + bh * 0.12;
    let ez = nose_z + bl * 0.16;
    let es = if variant == 1 { 2.0 } else { 1.0 };
    b.sym(|b, s| {
        b.bx(v3(s * bw * 0.45, ey, ez), v3(1.0, es, es), C);
    });
    // Tail: peduncle segments + fin.
    tail(
        &mut g.b,
        0,
        &TailSpec {
            start: v3(0.0, by, zc + bl * 0.42),
            dir: v3(0.0, if variant == 0 { 0.05 } else { 0.0 }, 1.0),
            segments: 2,
            length: len * 0.25,
            r0: (bw * 0.35).max(1.0),
            r1: 0.6,
            bend: Quat::IDENTITY,
            tip: TailTip::FishFin,
            plates: None,
            flat_y: (bh / bw * 0.5).clamp(1.0, 2.5),
        },
    );
    // Mouth.
    let hp = g.b.part(0, v3(0.0, by, zc), PartRole::Head);
    let breath = g.take(Breath);
    let grab = g.take(Grab);
    let big = variant == 2 || variant == 3 || variant == 0;
    let jl = bl * if big { 0.35 } else { 0.2 };
    let hinge = v3(0.0, by - bh * 0.15, nose_z + jl * 0.9);
    jaw(&mut g.b, hp, hinge, jl, odd(bw * 0.8), (bh * 0.18).max(1.0), if breath { 28.0 } else if big { 14.0 } else { 6.0 }, big || grab);
    g.b.on(0);
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(bw * 0.4, by + bh * 0.3, zc - bl * 0.05),
            wing_span: len * 0.45,
            wing_style: WingStyle::Jet,
            rump: v3(0.0, by, zc + bl * 0.5),
            rump_r: 1.0,
            back_top: v3(0.0, by + bh * 0.5, zc - bl * 0.2),
            head: Some((hp, v3(0.0, by + bh * 0.45, nose_z + bl * 0.15), bw)),
            size: len * 0.6,
        },
    );
}

// -------------------------------------------------------------------------------------- arachnid

pub(super) fn arachnid(g: &mut G) {
    let h = g.l;
    let scorpion = g.has(ArticulatedTail) || g.r.pick(3) == 2;
    let tarantula = !scorpion && g.r.chance(0.5);
    let b = &mut g.b;
    let ct = v3(0.0, h * 0.42, -h * 0.2);
    b.el(ct, v3(h * 0.46, h * 0.3, h * 0.46), P);
    let ab = if scorpion { v3(0.0, h * 0.38, h * 0.25) } else { v3(0.0, h * 0.55, h * 0.45) };
    if scorpion {
        for i in 0..4 {
            b.el(ab + v3(0.0, 0.0, i as f32 * h * 0.14 - h * 0.1), v3(h * 0.5 - i as f32 * h * 0.04, h * 0.26, h * 0.22), if i % 2 == 0 { P } else { A });
        }
    } else {
        b.el(ab, v3(h * 0.72, h * 0.62, h * 0.85), P);
        // Marking (hourglass / stripe).
        b.el(ab + v3(0.0, h * 0.28, -h * 0.05), v3(h * 0.2, 1.0, h * 0.45), C);
        b.el(ab + v3(0.0, h * 0.3, h * 0.1), v3(h * 0.36, 1.0, h * 0.12), A);
    }
    // Eyes cluster.
    let front = ct + v3(0.0, h * 0.08, -h * 0.22);
    eyes(b, front, (h * 0.07).max(1.0), 1.0);
    eyes(b, front + v3(0.0, 1.0, 0.5), (h * 0.13).max(1.5), 1.0);
    // Legs.
    let lw = if tarantula { 1.6 } else { 1.0 };
    let angles: [f32; 4] = [-0.9, -0.3, 0.25, 0.8];
    for s in [-1.0f32, 1.0] {
        for (i, a) in angles.iter().enumerate() {
            let role = match (i < 2, s < 0.0) {
                (true, true) => PartRole::LegFrontLeft,
                (true, false) => PartRole::LegFrontRight,
                (false, true) => PartRole::LegBackLeft,
                (false, false) => PartRole::LegBackRight,
            };
            let hip = ct + v3(s * h * 0.2, 0.0, (i as f32 - 1.5) * h * 0.1);
            let dir = v3(s * a.cos(), 0.0, a.sin());
            let knee = hip + dir * (h * 0.45) + v3(0.0, h * 0.42, 0.0);
            let foot = v3(hip.x, 0.0, hip.z) + dir * (h * 1.0);
            leg(&mut g.b, 0, role, hip, Some(knee), foot, lw, 0.0, dir, PrimKind::Box);
            g.b.bx(foot + v3(0.0, 0.5, 0.0), v3(1.0, 1.0, 1.0), D);
        }
    }
    // Fangs (jaw).
    let jid = g.b.part(0, ct + v3(0.0, -h * 0.05, -h * 0.2), PartRole::Jaw);
    g.b.sym(|b, s| {
        let base = ct + v3(s * h * 0.07, -h * 0.05, -h * 0.22);
        b.cone_to(base, base + v3(0.0, -h * 0.18, -h * 0.05), 1.6, C);
    });
    let _ = jid;
    g.b.on(0);
    if scorpion {
        g.handle(ArticulatedTail);
        g.handle(Grab);
        pincer(&mut g.b, 0, ct + v3(-h * 0.2, 0.0, -h * 0.15), -1.0, h * 0.75);
        pincer(&mut g.b, 0, ct + v3(h * 0.2, 0.0, -h * 0.15), 1.0, h * 0.75);
        g.b.on(0);
        tail(
            &mut g.b,
            0,
            &TailSpec {
                start: ab + v3(0.0, h * 0.05, h * 0.35),
                dir: v3(0.0, 0.35, 1.0),
                segments: 6,
                length: h * 1.5,
                r0: h * 0.12,
                r1: h * 0.07,
                bend: Quat::from_rotation_x(-0.5),
                tip: TailTip::Stinger,
                plates: None,
                flat_y: 1.0,
            },
        );
    } else if g.take(Grab) {
        pincer(&mut g.b, 0, ct + v3(-h * 0.2, 0.0, -h * 0.15), -1.0, h * 0.6);
        pincer(&mut g.b, 0, ct + v3(h * 0.2, 0.0, -h * 0.15), 1.0, h * 0.6);
    }
    g.b.on(0);
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(h * 0.15, h * 0.6, -h * 0.1),
            wing_span: h * 0.9,
            wing_style: WingStyle::Insect,
            rump: ab + v3(0.0, 0.0, h * 0.4),
            rump_r: h * 0.1,
            back_top: ab + v3(0.0, h * 0.3, 0.0),
            head: None,
            size: h,
        },
    );
}

// ---------------------------------------------------------------------------------------- insect

pub(super) fn insect(g: &mut G) {
    let h = g.l;
    let variant = g.r.pick(4); // 0 bee/wasp, 1 beetle, 2 ant, 3 dragonfly
    let b = &mut g.b;
    let y = h * 0.5;
    let th = v3(0.0, y, 0.0);
    let ts = (h * 0.34).max(3.0);
    b.el(th, v3(ts, ts, ts * 1.1), P);
    // Abdomen.
    let (al, aw) = match variant {
        3 => (h * 1.3, h * 0.2),
        1 => (h * 0.8, h * 0.6),
        2 => (h * 0.55, h * 0.45),
        _ => (h * 0.75, h * 0.42),
    };
    let ac = v3(0.0, y + if variant == 3 { 0.0 } else { h * 0.04 }, ts * 0.5 + al * 0.5);
    b.elr(ac, v3(aw, aw * 0.9, al), v3(if variant == 0 { -10.0 } else { 0.0 }, 0.0, 0.0), P);
    if variant == 0 || variant == 3 {
        let n = if variant == 3 { 5 } else { 3 };
        for i in 0..n {
            let f = (i as f32 + 0.8) / (n as f32 + 0.8);
            let z = ac.z - al * 0.5 + al * f;
            let rr = (1.0 - ((f - 0.5) * 2.0).powi(2)).max(0.0).sqrt() * aw;
            b.cz(v3(0.0, ac.y + (z - ac.z) * 0.17 * (variant == 0) as i32 as f32, z), rr + 0.8, 1.0, D);
        }
        if variant == 0 {
            b.cone_to(ac + v3(0.0, -h * 0.05, al * 0.45), ac + v3(0.0, -h * 0.12, al * 0.5 + h * 0.2), 1.5, C);
        }
    }
    if variant == 1 {
        // Elytra shell with centre seam.
        b.el(ac + v3(0.0, aw * 0.15, -al * 0.05), v3(aw + 1.0, aw * 0.85, al * 1.05), A);
        b.bx(ac + v3(0.0, aw * 0.55, 0.0), v3(1.0, 1.0, al * 0.9), D);
    }
    // Legs: 3 per side from the thorax.
    let spreads: [f32; 3] = [-0.7, 0.05, 0.7];
    for s in [-1.0f32, 1.0] {
        for (i, a) in spreads.iter().enumerate() {
            let role = match (i < 2, s < 0.0) {
                (true, true) => PartRole::LegFrontLeft,
                (true, false) => PartRole::LegFrontRight,
                (false, true) => PartRole::LegBackLeft,
                (false, false) => PartRole::LegBackRight,
            };
            let hip = th + v3(s * ts * 0.35, -ts * 0.2, (i as f32 - 1.0) * ts * 0.3);
            let dir = v3(s * a.cos(), 0.0, a.sin());
            let knee = hip + dir * (h * 0.35) + v3(0.0, h * 0.22, 0.0);
            let foot = v3(hip.x, 0.0, hip.z) + dir * (h * 0.75);
            leg(&mut g.b, 0, role, hip, Some(knee), foot, 1.0, 0.0, dir, PrimKind::Box);
        }
    }
    // Wings.
    let flies = g.take(Flies) | g.take(Glide);
    if variant == 0 || variant == 3 || flies {
        let span = if variant == 3 { h * 1.25 } else { h * 0.85 };
        wing_pair(&mut g.b, 0, v3(ts * 0.25, y + ts * 0.4, 0.0), span, WingStyle::Insect, 12.0);
        g.handle(Flies);
    }
    // Head + mandibles + antennae.
    let hp = g.b.part(0, th + v3(0.0, 0.0, -ts * 0.45), PartRole::Head);
    let charge = g.has(Charge);
    let b = &mut g.b;
    let hs = (h * if variant == 3 { 0.36 } else { 0.3 }).max(3.0);
    let hc = th + v3(0.0, h * 0.04, -ts * 0.45 - hs * 0.45);
    b.el(hc, v3(hs, hs, hs), P);
    let es = if variant == 3 { 2.0 } else { 1.0 };
    b.sym(|b, s| {
        b.bx(hc + v3(s * hs * 0.45, hs * 0.1, -hs * 0.2), v3(1.0, es, es), C);
    });
    b.sym(|b, s| {
        let base = hc + v3(s * hs * 0.2, hs * 0.4, -hs * 0.25);
        let mid = base + v3(s * h * 0.12, h * 0.3, -h * 0.12);
        b.seg(PrimKind::Box, base, mid, 1.0, 1.0, A);
        b.seg(PrimKind::Box, mid, mid + v3(s * h * 0.05, h * 0.02, -h * 0.2), 1.0, 1.0, A);
        b.bx(mid + v3(s * h * 0.05, h * 0.02, -h * 0.2), v3(1.0, 1.0, 1.0), C);
    });
    if variant == 1 && !charge {
        horns(b, hc + v3(0.0, hs * 0.2, -hs * 0.35), 0.0, h * 0.45, HornStyle::Rhino);
    }
    let jid = g.b.part(hp, hc + v3(0.0, -hs * 0.3, -hs * 0.3), PartRole::Jaw);
    let _ = jid;
    let big = variant == 2 || g.has(Grab);
    g.b.sym(|b, s| {
        let base = hc + v3(s * hs * 0.25, -hs * 0.3, -hs * 0.4);
        let l2 = if big { h * 0.3 } else { h * 0.16 };
        b.seg(PrimKind::Box, base, base + v3(s * l2 * 0.2, -l2 * 0.1, -l2 * 0.7), 1.2, 1.2, A);
        b.cone_to(base + v3(s * l2 * 0.2, -l2 * 0.1, -l2 * 0.7), base + v3(-s * l2 * 0.25, -l2 * 0.1, -l2 * 1.1), 1.3, C);
    });
    g.handle(Grab);
    g.b.on(0);
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(ts * 0.25, y + ts * 0.4, 0.0),
            wing_span: h * 0.9,
            wing_style: WingStyle::Insect,
            rump: ac + v3(0.0, 0.0, al * 0.5),
            rump_r: (aw * 0.2).max(0.8),
            back_top: th + v3(0.0, ts * 0.5, 0.0),
            head: Some((hp, hc + v3(0.0, hs * 0.45, -hs * 0.1), hs)),
            size: h,
        },
    );
}

// ------------------------------------------------------------------------------------------ blob

pub(super) fn blob(g: &mut G) {
    let h = g.l;
    let variant = g.r.pick(3); // 0 slime drop, 1 jelly cube, 2 lumpy
    let w = h * g.r.range(1.05, 1.35);
    let b = &mut g.b;
    match variant {
        1 => {
            b.bx(v3(0.0, h * 0.45, 0.0), v3(odd(w * 0.9), h * 0.9, odd(w * 0.9)), P);
            b.bx(v3(0.0, h * 0.45, 0.0), v3(odd(w * 0.9) + 1.0, h * 0.3, odd(w * 0.9) + 1.0), A);
        }
        2 => {
            b.el(v3(0.0, h * 0.4, 0.0), v3(w, h * 0.8, w), P);
            b.el(v3(-w * 0.25, h * 0.35, w * 0.2), v3(w * 0.6, h * 0.6, w * 0.6), P);
            b.el(v3(w * 0.3, h * 0.3, w * 0.15), v3(w * 0.5, h * 0.5, w * 0.55), A);
        }
        _ => {
            b.el(v3(0.0, h * 0.4, 0.0), v3(w, h * 0.85, w), P);
            b.el(v3(0.0, h * 0.12, 0.0), v3(w * 1.15, h * 0.3, w * 1.15), A);
        }
    }
    b.cut_bx(v3(0.0, -50.0, 0.0), v3(200.0, 100.0, 200.0));
    // The top lobe with the face is the "head" (it jiggles with lag when hopping).
    let hp = b.part(0, v3(0.0, h * 0.55, 0.0), PartRole::Head);
    match variant {
        0 => {
            b.cone_to(v3(0.0, h * 0.6, 0.0), v3(0.0, h * 1.25, h * 0.15), w * 0.55, P);
        }
        1 => {}
        _ => {
            b.el(v3(0.0, h * 0.75, 0.0), v3(w * 0.55, h * 0.5, w * 0.55), P);
        }
    }
    let fz = -w * 0.46;
    eyes(b, v3(0.0, h * 0.62, fz), (w * 0.17).max(1.0), (h * 0.14).round().max(1.0));
    b.bx(v3(0.0, h * 0.42, fz + 0.3), v3((w * 0.3).round().max(1.0), 1.0, 1.0), D);
    b.bx(v3(w * 0.2, h * 0.82, fz * 0.6), v3(2.0, 1.0, 1.0), C);
    let _ = hp;
    let breath = g.take(Breath);
    if breath {
        jaw(&mut g.b, hp, v3(0.0, h * 0.4, fz * 0.3), w * 0.5, odd(w * 0.4), 1.0, 25.0, true);
    }
    g.b.on(0);
    creature_extras(
        g,
        &Anchors {
            shoulder: v3(w * 0.3, h * 0.6, 0.0),
            wing_span: h * 0.8,
            wing_style: WingStyle::Bat,
            rump: v3(0.0, h * 0.3, w * 0.5),
            rump_r: (h * 0.1).max(1.0),
            back_top: v3(0.0, h * 0.85, w * 0.1),
            head: Some((hp, v3(0.0, h * 0.85, -w * 0.1), w * 0.6)),
            size: h,
        },
    );
}
