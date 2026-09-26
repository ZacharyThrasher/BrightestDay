//! Costume accessories from mechanics, features and element (each in an [`acc`] slot so the lobby
//! can hide it and the guardrail can shrink/drop it). Units: 1/16 m.

use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_core::shape::{PartRole, PrimKind};
use glam::{Quat, Vec3};

use super::{ARM_L, ARM_R, Cx, HEAD, Headgear, Hints, LEG_L, LEG_R, acc, rig};
use crate::build::{A, C, D, P, v3};
use crate::parts::{WingStyle, wing_pair};

const TOP_Y: f32 = rig::HC.y + rig::HS.y * 0.5;
const FACE_Z: f32 = rig::HC.z - rig::HS.z * 0.5;
const BACK: Vec3 = Vec3::new(0.0, 15.0, 2.6);
const ARM_X: f32 = 5.5;

fn arm_x(s: f32) -> f32 {
    s * ARM_X
}

pub(super) fn accessories(cx: &mut Cx, r: &Recipe, h: &Hints, beast: bool) {
    use Feature::*;
    use Mechanic::*;
    let f = |x: Feature| r.features.contains(&x);
    let m = |x: Mechanic| r.has(x);
    let gear = h.gear;
    let mut back_items = 0;
    let back_ok = |n: &mut i32| {
        *n += 1;
        *n <= 2
    };

    // --- shoulders / torso armour
    if f(Armor) || gear == Some(Headgear::Knight) || gear == Some(Headgear::Samurai) {
        let samurai = gear == Some(Headgear::Samurai);
        cx.group(acc::SHOULDERS, v3(0.0, 18.0, 0.0), |b, _| {
            for (id, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
                b.on(id);
                if samurai {
                    b.bxr(v3(arm_x(s) + s * 0.6, 17.0, 0.0), v3(4.2, 4.0, 4.4), v3(0.0, 0.0, s * 12.0), D);
                    b.bx(v3(arm_x(s) + s * 1.2, 16.0, 0.0), v3(3.6, 0.6, 4.6), A);
                } else {
                    b.el(v3(arm_x(s), 18.8, 0.0), v3(5.4, 3.4, 5.0), A);
                    b.bx(v3(arm_x(s) + s * 0.4, 17.6, 0.0), v3(4.8, 0.8, 4.8), D);
                    b.bx(v3(arm_x(s) + s * 2.4, 18.6, 0.0), Vec3::splat(0.8), C);
                }
            }
            b.on(0);
            b.bx(v3(0.0, 15.2, -2.8), v3(7.4, 6.4, 0.8), A);
            b.bx(v3(0.0, 15.8, -3.2), v3(3.0, 3.0, 0.6), D);
            b.bx(v3(0.0, 15.8, -3.4), v3(1.4, 1.4, 0.6), C);
        });
    }
    if f(Spikes) || (m(Pierce) && !beast) {
        cx.group(acc::SHOULDERS, v3(0.0, 19.0, 0.0), |b, _| {
            for (id, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
                b.on(id);
                for k in 0..2 {
                    let p = v3(arm_x(s) + s * 0.4, 19.4, -1.0 + k as f32 * 2.0);
                    b.cone_to(p, p + v3(s * 1.2, 2.6, 0.0), 1.6, C);
                }
            }
        });
    }
    if f(Gears) {
        cx.group(acc::SHOULDERS, v3(-7.6, 16.0, 0.0), |b, _| {
            let c = v3(-ARM_X - 1.9, 16.4, 0.0);
            b.part(ARM_L, c, PartRole::Wheel);
            crate::features::ring(b, c, Vec3::X, 2.2, 1.0, 1.0, D);
            for i in 0..8 {
                let d = crate::features::around(Vec3::X, i as f32 / 8.0 * std::f32::consts::TAU);
                b.flat(PrimKind::Box, c + d * 2.2, c + d * 3.2, 1.0, 1.0, Vec3::X, A);
            }
            b.cx(c, 1.0, 1.6, C);
        });
    }
    if f(Dish) {
        cx.group(acc::SHOULDERS, v3(6.0, 19.0, 0.0), |b, _| {
            b.on(ARM_R);
            b.bar(v3(ARM_X, 19.0, 1.0), v3(ARM_X, 22.0, 1.4), 0.8, D);
            let c = v3(ARM_X, 23.4, 1.4);
            b.q(PrimKind::Ellipsoid, c, v3(5.0, 5.0, 1.0), Quat::from_rotation_x(-0.5), A);
            b.bx(c + v3(0.0, 0.6, -1.2), Vec3::ONE, C);
        });
    }
    if f(Turret) || f(Barrels) || r.form == Form::Cannon {
        cx.group(acc::SHOULDERS, v3(6.0, 19.0, 0.0), |b, _| {
            b.on(ARM_R);
            let c = v3(ARM_X, 20.6, 0.5);
            b.bx(c, v3(3.4, 2.6, 3.4), D);
            b.cz(c + v3(0.0, 0.2, -3.0), 1.6, 5.0, P);
            b.cz(c + v3(0.0, 0.2, -5.6), 2.4, 0.8, A);
            b.cz(c + v3(0.0, 0.2, -6.1), 1.0, 0.6, C);
        });
    }

    // --- back
    if (f(Boiler) || f(Smokestack) && f(Pipes)) && back_ok(&mut back_items) {
        let stack = f(Smokestack);
        cx.group(acc::BACK, BACK, |b, _| {
            let c = v3(0.0, 14.6, 5.4);
            b.cy(c, 5.6, 8.4, P);
            for y in [-2.8f32, 0.0, 2.8] {
                b.cy(c + Vec3::Y * y, 6.2, 0.6, A);
            }
            b.cy(c + Vec3::Y * 4.4, 4.6, 0.8, D);
            b.bx(c + v3(0.0, -2.6, 2.8), v3(2.4, 1.6, 0.6), C);
            b.cz(c + v3(2.2, 2.2, 2.6), 1.6, 0.6, C);
            b.bx(c + v3(0.0, 0.0, -2.9), v3(4.0, 7.0, 1.0), D);
            if stack {
                b.cy(c + Vec3::Y * 7.0, 2.4, 5.0, D);
                b.cone_to(c + Vec3::Y * 10.4, c + Vec3::Y * 8.2, 4.2, P);
                b.cy(c + Vec3::Y * 10.0, 1.4, 0.6, C);
            }
        });
    } else if f(Smokestack) {
        cx.group(acc::HEAD, v3(0.0, TOP_Y, 0.0), |b, _| {
            b.on(HEAD);
            b.cy(v3(0.0, TOP_Y + 0.5, rig::HC.z), 10.4, 1.0, D);
            b.cy(v3(0.0, TOP_Y + 3.0, rig::HC.z), 5.0, 5.0, D);
            b.cone_to(v3(0.0, TOP_Y + 7.0, rig::HC.z), v3(0.0, TOP_Y + 4.6, rig::HC.z), 7.4, P);
            b.cy(v3(0.0, TOP_Y + 6.6, rig::HC.z), 3.0, 0.8, C);
        });
    }
    if f(Pipes) {
        cx.group(acc::ARMS, v3(0.0, 17.0, 2.6), |b, _| {
            for (id, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
                b.on(0);
                b.segx(PrimKind::Cylinder, v3(s * 1.6, 17.2, 3.4), v3(s * 4.6, 18.6, 1.6), 1.2, 0.3, A);
                b.on(id);
                b.segx(PrimKind::Cylinder, v3(arm_x(s) + s * 1.6, 18.2, 1.0), v3(arm_x(s) + s * 1.6, 12.8, 0.8), 1.0, 0.3, A);
                b.el(v3(arm_x(s) + s * 1.6, 15.4, 0.9), Vec3::splat(1.6), D);
            }
        });
    }
    let jet = m(Thrust) || f(Jet) || r.form == Form::Jetpack;
    if jet && back_ok(&mut back_items) {
        cx.group(acc::BACK, BACK, |b, _| {
            b.bx(v3(0.0, 15.4, 3.4), v3(5.0, 5.0, 1.2), D);
            for s in [-1.0f32, 1.0] {
                let c = v3(s * 2.0, 14.6, 4.8);
                b.cy(c, 2.8, 6.4, P);
                b.cone_to(c + Vec3::Y * 4.6, c + Vec3::Y * 3.0, 2.8, A);
                b.cy(c - Vec3::Y * 3.6, 2.2, 1.0, D);
                let fl = b.part(0, c - Vec3::Y * 4.0, PartRole::Exhaust);
                let _ = fl;
                b.cone_to(c - Vec3::Y * 4.0, c - Vec3::Y * 7.4, 1.8, C);
                b.on(0);
            }
        });
    }
    if f(Turbine) && back_ok(&mut back_items) {
        cx.group(acc::BACK, BACK, |b, _| {
            let c = v3(0.0, 15.0, 4.6);
            b.cz(c, 7.0, 2.4, P);
            b.cz(c + Vec3::Z * 1.4, 7.6, 0.6, A);
            b.part(0, c + Vec3::Z * 1.6, PartRole::Wheel);
            for i in 0..6 {
                let d = crate::features::around(Vec3::Z, i as f32 / 6.0 * std::f32::consts::TAU);
                b.flat(PrimKind::Box, c + Vec3::Z * 1.6, c + Vec3::Z * 1.6 + d * 2.8, 1.2, 0.6, Vec3::Z, D);
            }
            b.cz(c + Vec3::Z * 1.8, 1.2, 0.8, C);
        });
    }
    if f(Exhaust) && back_ok(&mut back_items) {
        cx.group(acc::BACK, BACK, |b, _| {
            for s in [-1.0f32, 1.0] {
                let base = v3(s * 2.4, 12.0, 3.2);
                b.segx(PrimKind::Cylinder, base, base + v3(0.0, 8.0, 1.4), 1.6, 0.2, D);
                b.part(0, base + v3(0.0, 8.6, 1.5), PartRole::Exhaust);
                b.cone_to(base + v3(0.0, 8.0, 1.4), base + v3(0.0, 11.0, 1.8), 1.6, C);
                b.on(0);
            }
        });
    }
    let flies = m(Flies) || r.form == Form::Wings || r.form == Form::Bird;
    if flies && back_ok(&mut back_items) {
        let style = match h.beast {
            Some(super::Beast::Dragon | super::Beast::Dino) => WingStyle::Bat,
            Some(super::Beast::Insect) => WingStyle::Insect,
            _ if r.form == Form::Insect => WingStyle::Insect,
            _ => WingStyle::Feather,
        };
        cx.group(acc::BACK, v3(0.0, 17.0, 2.8), |b, _| {
            wing_pair(b, 0, v3(1.5, 17.2, 3.0), 7.0, style, 20.0);
        });
    }
    let cape = m(Glide) || r.form == Form::Cape;
    if cape && !flies && back_ok(&mut back_items) {
        cx.group(acc::BACK, v3(0.0, 18.0, 2.8), |b, _| {
            let p0 = v3(0.0, 18.6, 3.0);
            let a = b.part(0, p0, PartRole::Cloth);
            b.bx(p0 + v3(0.0, -2.6, 0.3), v3(9.0, 5.4, 0.8), A);
            b.bx(v3(0.0, 18.6, 2.6), v3(9.4, 1.0, 1.2), D);
            let p1 = p0 + v3(0.0, -5.4, 0.3);
            b.part(a, p1, PartRole::Cloth);
            b.bx(p1 + v3(0.0, -2.8, 0.1), v3(9.6, 5.6, 0.8), A);
            b.bx(p1 + v3(0.0, -5.6, 0.1), v3(9.6, 0.8, 0.8), D);
        });
    }
    let sword = m(Blade) && !beast;
    let samurai = gear == Some(Headgear::Samurai);
    if sword && (samurai || back_ok(&mut back_items)) {
        if samurai {
            cx.group(acc::HIP, v3(-5.0, 11.0, 0.0), |b, _| {
                b.bxr(v3(-5.2, 10.6, 0.0), v3(1.0, 1.0, 11.0), v3(-15.0, 0.0, 0.0), D);
                b.bxr(v3(-5.2, 11.9, -5.6), v3(0.8, 0.8, 3.4), v3(-15.0, 0.0, 0.0), A);
                b.bx(v3(-5.2, 11.5, -3.8), v3(1.6, 1.6, 0.6), C);
            });
        } else {
            cx.group(acc::BACK, BACK, |b, _| {
                let a = v3(-3.6, 9.6, 3.2);
                let e = v3(3.4, 22.6, 3.2);
                b.flat(PrimKind::Box, a, e, 1.8, 0.8, Vec3::Z, C);
                b.flat(PrimKind::Box, a.lerp(e, 0.02), a.lerp(e, 0.72), 2.4, 1.0, Vec3::Z, D);
                let g = a.lerp(e, 0.75);
                b.flat(PrimKind::Box, g + v3(-2.0, 1.2, 0.0), g + v3(2.0, -1.2, 0.0), 1.0, 1.2, Vec3::Z, A);
                b.bar(g, e, 1.0, D);
                b.bx(e, Vec3::splat(1.4), A);
            });
        }
    }
    if gear == Some(Headgear::Wizard) && back_ok(&mut back_items) {
        cx.group(acc::BACK, BACK, |b, _| {
            let a = v3(3.8, 4.0, 3.4);
            let e = v3(-3.0, 27.0, 3.4);
            b.bar(a, e, 1.0, D);
            b.el(e + v3(-0.3, 1.4, 0.0), Vec3::splat(2.6), C);
            b.bx(e + v3(-0.2, -0.4, 0.0), v3(2.4, 1.0, 1.4), A);
        });
    }
    if (f(Sails) || f(Mast)) && back_ok(&mut back_items) {
        cx.group(acc::BACK, BACK, |b, _| {
            let base = v3(-2.4, 11.0, 3.4);
            let top = v3(-2.4, 30.0, 3.4);
            b.bar(base, top, 0.8, D);
            b.part(0, top, PartRole::Cloth);
            b.bx(top + v3(0.0, -1.8, 2.6), v3(0.8, 3.4, 4.8), A);
            b.bx(top + v3(0.0, -1.8, 2.6), v3(1.0, 1.4, 1.4), C);
        });
    }
    if f(CraneArm) && back_ok(&mut back_items) {
        cx.group(acc::BACK, BACK, |b, _| {
            let base = v3(3.0, 17.0, 3.4);
            let knee = v3(4.6, 24.0, 2.0);
            let tip = v3(5.4, 24.4, -3.6);
            b.bar(base, knee, 1.4, P);
            b.bar(knee, tip, 1.2, P);
            b.el(knee, Vec3::splat(2.0), A);
            b.bar(tip, tip - Vec3::Y * 2.4, 0.6, D);
            b.bx(tip - Vec3::Y * 2.8, Vec3::ONE, C);
        });
    }
    if f(Fins) && !matches!(h.beast, Some(super::Beast::Shark)) {
        cx.group(acc::TAIL, BACK, |b, _| {
            b.flat(PrimKind::Cone, v3(0.0, 13.0, 2.6), v3(0.0, 19.4, 5.4), 4.6, 1.0, Vec3::X, A);
        });
    }

    // --- arms
    if m(Guard) || m(Barrier) {
        cx.group(acc::LEFT_ARM, v3(-7.6, 12.0, 0.0), |b, _| {
            b.on(ARM_L);
            let c = v3(-ARM_X - 2.0, 12.6, -0.4);
            b.cx(c, 8.0, 1.0, A);
            b.cx(c + v3(-0.4, 0.0, 0.0), 8.6, 0.6, D);
            b.cx(c + v3(-0.6, 0.0, 0.0), 2.4, 0.8, C);
            b.bx(c + v3(-0.6, 0.0, 0.0), v3(0.6, 6.6, 1.0), D);
        });
    } else if f(Saw) {
        cx.group(acc::LEFT_ARM, v3(-7.6, 12.0, 0.0), |b, _| {
            let c = v3(-ARM_X - 2.0, 12.4, -0.4);
            b.part(ARM_L, c, PartRole::Wheel);
            b.cx(c, 6.0, 0.6, P);
            for i in 0..10 {
                let d = crate::features::around(Vec3::X, i as f32 / 10.0 * std::f32::consts::TAU);
                b.flat(PrimKind::Box, c + d * 2.6, c + d * 3.6, 0.8, 0.6, Vec3::X, C);
            }
            b.cx(c, 1.6, 1.2, D);
        });
    }
    if m(Grab) && !beast {
        cx.group(acc::ARMS, v3(0.0, 9.0, 0.0), |b, _| {
            for (id, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
                b.on(id);
                for k in [-1.0f32, 0.0, 1.0] {
                    let base = v3(arm_x(s) + k * 1.0, rig::HAND_Y - 1.4, -1.0);
                    b.cone_to(base, base + v3(0.0, -2.0, -1.2), 1.0, C);
                }
            }
        });
    }
    if f(Drill) {
        cx.group(acc::ARMS, v3(6.0, 9.0, 0.0), |b, _| {
            let base = v3(ARM_X, rig::HAND_Y - 1.2, 0.0);
            b.part(ARM_R, base, PartRole::Wheel);
            b.cone_to(base, base - Vec3::Y * 6.0, 3.8, P);
            for k in 0..5 {
                let f = k as f32 / 5.0;
                let d = crate::features::around(Vec3::Y, f * 9.0);
                b.bx(base - Vec3::Y * (6.0 * f + 0.4) + d * (1.8 * (1.0 - f)), Vec3::splat(0.8), A);
            }
            b.cy(base + Vec3::Y * 0.4, 4.2, 1.0, D);
        });
    }

    // --- hips
    let shoots = m(Shoots) && !beast && gear != Some(Headgear::Wizard);
    if shoots {
        cx.group(acc::HIP, v3(5.0, 10.0, 0.0), |b, _| {
            b.bx(v3(4.6, 9.6, 1.2), v3(1.4, 3.6, 2.4), D);
            b.bx(v3(4.5, 11.6, 0.2), v3(1.2, 1.4, 2.8), A);
            b.bx(v3(4.5, 12.2, -0.8), v3(1.0, 1.0, 1.0), C);
        });
    }
    if m(Pull) && !beast {
        cx.group(acc::HIP, v3(-5.0, 10.0, 0.0), |b, _| {
            crate::features::ring(b, v3(-4.6, 9.6, 0.4), Vec3::X, 2.0, 0.8, 0.8, A);
            crate::features::ring(b, v3(-4.8, 9.4, 0.4), Vec3::X, 1.4, 0.8, 0.8, P);
        });
    }
    if m(Explodes) {
        cx.group(acc::HIP, v3(0.0, 10.8, 0.0), |b, _| {
            for x in [-3.0f32, 0.0, 3.0] {
                b.el(v3(x, 10.4, -3.2), Vec3::splat(2.2), D);
                b.bx(v3(x + 0.4, 11.8, -3.2), Vec3::splat(0.6), C);
            }
        });
    }
    if f(Chains) {
        cx.group(acc::HIP, v3(0.0, 10.8, 0.0), |b, _| {
            for i in 0..5 {
                let x = -4.0 + i as f32 * 2.0;
                crate::attach::link(b, v3(x, 10.4, -2.9), 1.4, i % 2 == 1, if i % 2 == 0 { A } else { D });
            }
            crate::attach::link(b, v3(4.8, 8.6, -2.4), 1.4, false, A);
        });
    }

    // --- face / head
    if m(Fast) {
        cx.group(acc::FACE, v3(0.0, 19.0, 0.0), |b, _| {
            b.bx(v3(0.0, 19.2, 0.0), v3(6.0, 1.6, 5.2), A);
            let p = v3(1.6, 19.0, 2.8);
            b.part(0, p, PartRole::Cloth);
            b.flat(PrimKind::Box, p, p + v3(1.4, -3.0, 4.0), 1.8, 0.6, Vec3::X, A);
        });
    }
    if f(Bell) {
        cx.group(acc::FACE, v3(0.0, 18.0, -3.0), |b, _| {
            b.bx(v3(0.0, 19.2, 0.0), v3(5.6, 0.8, 5.0), D);
            b.cone_to(v3(0.0, 16.0, -3.2), v3(0.0, 18.8, -3.0), 3.2, A);
            b.bx(v3(0.0, 15.8, -3.2), Vec3::splat(0.8), C);
        });
    }
    if (f(Windows) || f(Cockpit)) && gear.is_none() {
        cx.group(acc::FACE, v3(0.0, 24.0, FACE_Z), |b, _| {
            b.on(HEAD);
            b.bx(v3(0.0, 24.0, FACE_Z - 0.4), v3(9.6, 1.0, 0.6), D);
            b.sym(|b, s| {
                b.cz(v3(s * 2.1, 24.0, FACE_Z - 0.6), 3.0, 0.8, A);
                b.cz(v3(s * 2.1, 24.0, FACE_Z - 0.9), 2.0, 0.6, C);
            });
        });
    }
    if f(Headlight) {
        cx.group(acc::HEAD, v3(0.0, 26.0, FACE_Z), |b, _| {
            b.on(HEAD);
            b.bx(v3(0.0, 26.2, rig::HC.z), v3(9.8, 1.0, 9.0), D);
            b.cz(v3(0.0, 26.4, FACE_Z - 0.8), 2.6, 1.2, A);
            b.cz(v3(0.0, 26.4, FACE_Z - 1.3), 1.8, 0.6, C);
        });
    }
    if f(Antenna) && gear != Some(Headgear::Robot) {
        cx.group(acc::HEAD, v3(0.0, TOP_Y, 0.0), |b, _| {
            let base = v3(-2.6, TOP_Y + 0.4, rig::HC.z + 1.0);
            b.part(HEAD, base, PartRole::Other);
            b.bar(base, base + v3(-0.6, 5.0, 0.4), 0.7, D);
            b.bx(base + v3(-0.6, 5.4, 0.4), Vec3::ONE, C);
        });
    }
    if f(Propeller) || f(Rotor) {
        cx.group(acc::HEAD, v3(0.0, TOP_Y, 0.0), |b, _| {
            b.on(HEAD);
            b.el(v3(0.0, TOP_Y + 0.2, rig::HC.z), v3(9.8, 3.4, 9.4), A);
            b.bar(v3(0.0, TOP_Y + 1.2, rig::HC.z), v3(0.0, TOP_Y + 2.8, rig::HC.z), 0.8, D);
            let hub = v3(0.0, TOP_Y + 3.0, rig::HC.z);
            b.part(HEAD, hub, PartRole::Wheel);
            for s in [-1.0f32, 1.0] {
                b.flat(PrimKind::Box, hub, hub + v3(s * 5.0, 0.0, 0.0), 1.6, 0.6, Vec3::Y, if s < 0.0 { P } else { C });
            }
            b.bx(hub, v3(1.2, 1.0, 1.2), D);
        });
    }
    let horns_ok = gear.is_none() && !beast;
    if m(Charge) && horns_ok {
        cx.group(acc::HEAD, v3(0.0, TOP_Y, 0.0), |b, _| {
            b.on(HEAD);
            b.sym(|b, s| {
                let base = v3(s * 3.0, TOP_Y - 0.6, rig::HC.z - 1.0);
                b.cone_to(base, base + v3(s * 1.6, 3.4, -0.8), 2.0, A);
            });
        });
    }

    // --- feet
    if f(Wheels) {
        cx.group(acc::FEET, v3(0.0, 0.0, 0.0), |b, _| {
            for (id, s) in [(LEG_L, -1.0f32), (LEG_R, 1.0)] {
                let x = s * rig::HIP_X;
                b.on(id);
                b.bx(v3(x, 0.4, -0.5), v3(3.0, 0.8, 5.4), A);
                for z in [-2.2f32, 1.2] {
                    let c = v3(x, -0.6, z - 0.5);
                    b.part(id, c, PartRole::Wheel);
                    b.cx(c, 2.0, 2.6, D);
                    b.cx(c, 0.8, 3.0, C);
                }
            }
        });
    } else if f(Treads) {
        cx.group(acc::FEET, v3(0.0, 0.0, 0.0), |b, _| {
            for (id, s) in [(LEG_L, -1.0f32), (LEG_R, 1.0)] {
                let x = s * rig::HIP_X;
                b.on(id);
                b.bx(v3(x, 0.2, -0.5), v3(4.6, 1.6, 7.0), D);
                for z in [-3.0f32, -1.5, 0.0, 1.5] {
                    b.bx(v3(x, -0.4, z), v3(4.8, 0.6, 0.6), A);
                }
            }
        });
    } else if m(Bounce) {
        cx.group(acc::FEET, v3(0.0, 0.0, 0.0), |b, _| {
            for (id, s) in [(LEG_L, -1.0f32), (LEG_R, 1.0)] {
                let x = s * rig::HIP_X;
                b.on(id);
                for k in 0..3 {
                    b.cy(v3(x + if k % 2 == 0 { 0.4 } else { -0.4 }, -0.4 - k as f32 * 1.0, -0.4), 2.6, 0.8, if k % 2 == 0 { A } else { C });
                }
            }
        });
    } else if m(Heavy) || m(Stomp) {
        cx.group(acc::FEET, v3(0.0, 0.0, 0.0), |b, _| {
            for (id, s) in [(LEG_L, -1.0f32), (LEG_R, 1.0)] {
                let x = s * rig::HIP_X;
                b.on(id);
                b.bx(v3(x, 1.4, -0.9), v3(5.2, 2.8, 6.6), D);
                b.bx(v3(x, 3.2, -0.4), v3(5.4, 1.0, 5.6), A);
            }
        });
    }
    if f(Pistons) {
        cx.group(acc::FEET, v3(0.0, 5.0, 0.0), |b, _| {
            for (id, s) in [(LEG_L, -1.0f32), (LEG_R, 1.0)] {
                let x = s * (rig::HIP_X + 2.0);
                b.on(id);
                b.cy(v3(x, 7.0, 0.8), 1.4, 3.6, D);
                b.cy(v3(x, 4.2, 0.8), 0.8, 3.0, C);
                b.cy(v3(x, 8.8, 0.8), 1.8, 0.6, A);
            }
        });
    }

    if !beast || r.element == Element::Fire {
        element_tell(cx, r.element);
    }
}

/// Small element tell: flame crest, lightning bolt, ice shards, psychic halo, atomic spines.
pub(super) fn element_tell(cx: &mut Cx, e: Element) {
    match e {
        Element::Fire => cx.group(acc::AURA, v3(0.0, TOP_Y, 0.0), |b, _| {
            let base = v3(0.0, TOP_Y + 0.4, rig::HC.z + 0.8);
            b.part(HEAD, base, PartRole::Exhaust);
            b.cone_to(base, base + v3(0.0, 4.6, 1.6), 3.4, C);
            b.cone_to(base + v3(1.6, 0.0, 0.8), base + v3(1.8, 3.0, 2.2), 2.0, C);
            b.cone_to(base + v3(-1.6, 0.0, 0.8), base + v3(-1.8, 3.2, 2.0), 2.0, C);
        }),
        Element::Electric => cx.group(acc::AURA, BACK, |b, _| {
            let pts = [v3(1.6, 20.4, 3.0), v3(-1.0, 16.8, 3.0), v3(1.2, 15.6, 3.0), v3(-1.6, 11.6, 3.0)];
            for w in pts.windows(2) {
                b.segx(PrimKind::Box, w[0], w[1], 1.2, 0.3, C);
            }
        }),
        Element::Ice => cx.group(acc::AURA, v3(0.0, 19.0, 0.0), |b, _| {
            for (id, s) in [(ARM_L, -1.0f32), (ARM_R, 1.0)] {
                b.on(id);
                let p = v3(arm_x(s), 19.2, 0.0);
                b.cone_to(p, p + v3(s * 0.8, 3.2, 0.4), 1.6, C);
                b.cone_to(p + v3(0.0, 0.0, 1.2), p + v3(s * 1.6, 2.0, 1.8), 1.2, C);
            }
        }),
        Element::Psychic => cx.group(acc::AURA, v3(0.0, TOP_Y + 2.0, 0.0), |b, _| {
            b.on(HEAD);
            crate::features::ring(b, v3(0.0, TOP_Y + 2.4, rig::HC.z), Vec3::Y, 4.0, 0.6, 0.8, C);
        }),
        Element::Atomic => cx.group(acc::AURA, BACK, |b, _| {
            for k in 0..3 {
                let p = v3(0.0, 18.0 - k as f32 * 2.8, 2.6);
                b.cone_to(p, p + v3(0.0, 1.6, 2.4), 2.0, C);
            }
        }),
        _ => {}
    }
}

