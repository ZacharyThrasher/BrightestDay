//! Costume heads: human faces + hair, headgear (name hints), creature heads by archetype, curated
//! icon heads, and mini constructs worn as helmets. Units: 1/16 m. Skull = `rig::HC` ± `rig::HS/2`.

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::{PartRole, PrimKind, ShapeSpec};
use glam::{Quat, Vec3};

use super::{Beast, Cx, HEAD, Headgear, Hints, acc, rig};
use crate::build::{A, B, C, D, P, v3};
use crate::rng::Rng;

const FACE_Z: f32 = rig::HC.z - rig::HS.z * 0.5;
const TOP_Y: f32 = rig::HC.y + rig::HS.y * 0.5;
const BACK_Z: f32 = rig::HC.z + rig::HS.z * 0.5;

fn eyes(b: &mut B, y: f32, z: f32, dx: f32, w: f32, h: f32) {
    b.sym(|b, s| {
        b.bx(v3(s * dx, y, z), v3(w, h, 0.8), C);
    });
}

/// Plain human skull with face (eyes, brows, mouth).
fn skull(b: &mut B) {
    let hc = rig::HC;
    b.bx(hc, rig::HS, A);
    b.bx(hc + v3(0.0, -0.3, 0.0), rig::HS + v3(-1.2, -1.6, 0.6), A);
    eyes(b, 23.8, FACE_Z - 0.2, 2.1, 1.3, 2.0);
    b.bx(v3(0.0, 21.2, FACE_Z - 0.2), v3(2.4, 0.7, 0.6), D);
    b.sym(|b, s| {
        b.bx(v3(s * 2.1, 25.4, FACE_Z - 0.2), v3(2.0, 0.6, 0.6), D);
    });
}

fn hair(b: &mut B, style: u32) {
    let hc = rig::HC;
    match style {
        0 => {
            b.bx(v3(0.0, TOP_Y + 0.2, hc.z + 0.4), v3(9.6, 2.4, 9.2), D);
            b.bx(v3(0.0, 24.5, BACK_Z - 0.2), v3(9.6, 6.0, 1.6), D);
            b.bx(v3(-2.0, 26.6, FACE_Z + 0.2), v3(5.0, 1.4, 1.2), D);
        }
        1 => {
            b.bx(v3(0.0, TOP_Y, hc.z + 0.5), v3(9.4, 2.0, 9.0), D);
            for (i, x) in [-3.0f32, -1.0, 1.0, 3.0].into_iter().enumerate() {
                let base = v3(x, TOP_Y + 0.5, hc.z + (i % 2) as f32);
                b.cone_to(base, base + v3(x * 0.4, 3.5, 1.5), 2.6, D);
            }
        }
        2 => {
            b.bx(v3(0.0, TOP_Y + 0.2, hc.z + 0.3), v3(9.8, 2.4, 9.4), D);
            b.bx(v3(0.0, 22.5, BACK_Z + 0.2), v3(9.8, 10.0, 1.8), D);
            b.sym(|b, s| {
                b.bx(v3(s * 4.9, 23.0, hc.z + 1.5), v3(1.2, 8.0, 5.0), D);
            });
        }
        3 => {
            b.bx(v3(0.0, TOP_Y + 1.5, hc.z + 0.5), v3(2.0, 3.5, 9.0), A);
            b.sym(|b, s| {
                b.bx(v3(s * 3.5, TOP_Y - 0.2, hc.z + 0.5), v3(2.0, 1.0, 8.0), D);
            });
        }
        _ => {
            b.bx(v3(0.0, TOP_Y - 1.4, hc.z), v3(9.8, 1.6, 9.2), A);
            b.bx(v3(0.0, TOP_Y - 1.4, FACE_Z - 0.3), v3(1.6, 1.6, 0.6), C);
            b.bx(v3(0.0, TOP_Y + 0.1, hc.z + 0.5), v3(9.2, 1.4, 8.6), D);
        }
    }
}

/// Human head: skull + hair, or headgear (which may replace the skull entirely).
pub(super) fn human_head(cx: &mut Cx, gear: Option<Headgear>, gear2: Option<Headgear>, recipe: &Recipe) {
    use Headgear::*;
    let replaces = matches!(gear, Some(Robot | Skeleton | Tv | Toaster | Cactus | Pumpkin));
    cx.b.on(HEAD);
    if !replaces {
        skull(&mut cx.b);
    }
    cx.b.on(0);
    let style = cx.r.pick(5);
    match gear {
        None => {
            cx.group(acc::HEAD, v3(0.0, TOP_Y, 0.0), |b, _| {
                b.on(HEAD);
                hair(b, style);
            });
        }
        Some(g) => headgear(cx, g, recipe, false),
    }
    if let Some(g2) = gear2
        && (!matches!(g2, Robot | Skeleton | Tv | Toaster | Cactus | Pumpkin) || gear.is_none())
    {
        headgear(cx, g2, recipe, true);
    }
    if replaces && cx.hide & acc::HEAD != 0 {
        cx.b.on(HEAD);
        skull(&mut cx.b);
        cx.b.on(0);
    }
}

#[allow(clippy::only_used_in_recursion)]
fn headgear(cx: &mut Cx, g: Headgear, recipe: &Recipe, secondary: bool) {
    use Headgear::*;
    let hc = rig::HC;
    let top = v3(0.0, TOP_Y, hc.z);
    // A secondary hint only adds its small signature item (robot pirate = robot head + eyepatch).
    if secondary {
        match g {
            Pirate => cx.group(acc::FACE, v3(0.0, 24.0, FACE_Z), |b, _| {
                b.on(HEAD);
                b.bx(v3(-2.1, 23.8, FACE_Z - 0.9), v3(2.6, 2.6, 0.8), D);
                b.bar(v3(-4.9, 25.8, FACE_Z - 0.6), v3(4.9, 22.8, FACE_Z - 0.6), 0.7, D);
                b.bx(v3(0.0, TOP_Y + 1.2, hc.z), v3(12.0, 1.0, 10.0), D);
                b.bx(v3(0.0, TOP_Y + 2.4, hc.z), v3(8.6, 2.6, 7.6), D);
                b.bx(v3(0.0, TOP_Y + 2.4, FACE_Z + 0.4), v3(1.6, 1.6, 0.6), C);
            }),
            Cowboy | Wizard | Viking | Samurai | Knight | Chef | Crown | Cap => headgear(cx, g, recipe, false),
            _ => {}
        }
        return;
    }
    match g {
        Knight => cx.group(acc::HEAD, top, |b, r| {
            b.on(HEAD);
            b.bx(hc + v3(0.0, 0.4, 0.0), v3(10.4, 10.2, 9.8), A);
            b.bx(hc + v3(0.0, 5.2, 0.0), v3(9.0, 1.0, 8.4), A);
            b.bx(v3(0.0, 24.0, FACE_Z - 0.8), v3(8.0, 0.9, 0.8), D);
            b.bx(v3(0.0, 24.0, FACE_Z - 1.0), v3(5.0, 0.6, 0.6), C);
            b.bx(v3(0.0, 21.5, FACE_Z - 0.8), v3(1.0, 3.5, 0.8), D);
            for x in [-2.0f32, 2.0] {
                for y in [20.8f32, 22.0] {
                    b.bx(v3(x, y, FACE_Z - 0.8), v3(0.6, 0.6, 0.6), D);
                }
            }
            // Plume: a cloth part that streams with speed.
            let pv = v3(0.0, TOP_Y + 1.2, hc.z + 1.0);
            b.part(HEAD, pv, PartRole::Cloth);
            let n = 3 + r.pick(2) as usize;
            for i in 0..n {
                let f = i as f32 / n as f32;
                b.el(pv + v3(0.0, 1.2 + 1.4 * f, 1.2 + 2.2 * f), v3(1.6, 2.2, 2.4), if i + 1 == n { C } else { P });
            }
        }),
        Samurai => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.el(v3(0.0, TOP_Y - 0.5, hc.z), v3(11.0, 6.0, 10.4), D);
            b.bx(v3(0.0, TOP_Y - 1.8, hc.z), v3(11.2, 1.0, 10.6), A);
            b.bxr(v3(0.0, 22.4, BACK_Z - 0.4), v3(12.0, 5.0, 1.2), v3(-25.0, 0.0, 0.0), D);
            b.sym(|b, s| {
                b.bxr(v3(s * 5.6, 22.8, hc.z), v3(1.2, 5.0, 8.0), v3(0.0, 0.0, s * 18.0), D);
                b.flat(PrimKind::Box, v3(s * 1.2, TOP_Y - 0.5, FACE_Z - 0.8), v3(s * 4.2, TOP_Y + 4.5, FACE_Z - 1.4), 1.2, 0.8, Vec3::Z, A);
            });
            b.bx(v3(0.0, TOP_Y - 0.4, FACE_Z - 1.0), v3(2.0, 2.0, 0.8), C);
            b.bx(v3(0.0, 21.2, FACE_Z - 0.6), v3(7.0, 3.0, 0.8), D);
        }),
        Viking => {
            cx.group(acc::HEAD, top, |b, _| {
                b.on(HEAD);
                b.el(v3(0.0, TOP_Y - 0.6, hc.z), v3(10.6, 6.4, 10.0), A);
                b.bx(v3(0.0, TOP_Y - 2.4, hc.z), v3(10.8, 1.2, 10.2), D);
                b.bx(v3(0.0, TOP_Y - 1.4, FACE_Z - 0.4), v3(1.2, 4.0, 0.8), D);
                b.sym(|b, s| {
                    let a = v3(s * 4.8, TOP_Y - 1.0, hc.z);
                    let m = a + v3(s * 2.8, 1.2, -0.5);
                    b.segx(PrimKind::Cylinder, a, m, 2.2, 0.4, A);
                    b.cone_to(m, m + v3(s * 0.8, 4.0, -1.0), 2.2, A);
                    b.cone_to(m + v3(s * 0.5, 2.4, -0.6), m + v3(s * 0.8, 4.0, -1.0), 1.2, C);
                });
            });
            cx.group(acc::FACE, v3(0.0, 21.0, FACE_Z), |b, _| {
                b.on(HEAD);
                b.bx(v3(0.0, 20.8, FACE_Z - 0.6), v3(7.4, 3.6, 1.6), A);
                b.bx(v3(0.0, 18.4, FACE_Z - 0.4), v3(4.4, 2.6, 1.4), A);
                b.sym(|b, s| {
                    b.bx(v3(s * 2.2, 17.4, FACE_Z - 0.2), v3(1.2, 2.0, 1.2), D);
                });
            });
        }
        Wizard => {
            cx.group(acc::HEAD, top, |b, _| {
                b.on(HEAD);
                b.cy(v3(0.0, TOP_Y + 0.4, hc.z), 14.0, 1.0, D);
                b.cy(v3(0.0, TOP_Y + 1.2, hc.z), 9.0, 1.2, A);
                b.cone_to(v3(0.0, TOP_Y + 0.8, hc.z), v3(0.0, TOP_Y + 6.5, hc.z + 1.0), 9.0, P);
                b.cone_to(v3(0.0, TOP_Y + 6.0, hc.z + 1.0), v3(0.0, TOP_Y + 9.0, hc.z + 3.5), 3.6, P);
                b.bx(v3(1.6, TOP_Y + 3.8, FACE_Z + 1.8), v3(1.2, 1.2, 0.8), C);
            });
            cx.group(acc::FACE, v3(0.0, 21.0, FACE_Z), |b, _| {
                b.on(HEAD);
                b.bx(v3(0.0, 20.4, FACE_Z - 0.6), v3(7.0, 4.4, 1.4), C);
                b.cone_to(v3(0.0, 18.6, FACE_Z - 0.5), v3(0.0, 13.8, FACE_Z - 1.4), 5.0, C);
            });
        }
        Pirate => {
            cx.group(acc::HEAD, top, |b, _| {
                b.on(HEAD);
                b.bxr(v3(0.0, TOP_Y + 0.6, hc.z), v3(10.0, 1.0, 10.0), v3(0.0, 45.0, 0.0), D);
                b.bx(v3(0.0, TOP_Y + 0.6, hc.z), v3(12.4, 1.0, 7.0), D);
                b.bx(v3(0.0, TOP_Y + 2.2, hc.z), v3(8.8, 2.6, 7.8), D);
                b.bx(v3(0.0, TOP_Y + 1.6, FACE_Z - 0.8), v3(9.0, 2.0, 0.8), D);
                b.bx(v3(0.0, TOP_Y + 2.0, FACE_Z - 1.3), v3(1.6, 1.6, 0.6), C);
                b.bx(v3(0.0, TOP_Y + 0.9, hc.z), v3(12.6, 0.6, 7.2), A);
            });
            cx.group(acc::FACE, v3(0.0, 23.0, FACE_Z), |b, _| {
                b.on(HEAD);
                b.bx(v3(-2.1, 23.8, FACE_Z - 0.6), v3(2.6, 2.6, 0.8), D);
                b.bar(v3(-4.8, 26.0, FACE_Z - 0.4), v3(4.8, 23.0, FACE_Z - 0.4), 0.7, D);
                b.bx(v3(0.0, 20.2, FACE_Z - 0.4), v3(6.0, 1.6, 1.0), D);
            });
        }
        Cowboy => {
            cx.group(acc::HEAD, top, |b, _| {
                b.on(HEAD);
                b.cy(v3(0.0, TOP_Y + 0.3, hc.z), 15.0, 1.0, D);
                b.sym(|b, s| {
                    b.bxr(v3(s * 6.4, TOP_Y + 1.0, hc.z), v3(3.0, 1.0, 12.0), v3(0.0, 0.0, s * 35.0), D);
                });
                b.bx(v3(0.0, TOP_Y + 2.6, hc.z), v3(8.4, 4.0, 8.0), D);
                b.bx(v3(0.0, TOP_Y + 4.6, hc.z), v3(1.2, 1.0, 8.2), P);
                b.bx(v3(0.0, TOP_Y + 1.2, hc.z), v3(8.8, 1.0, 8.4), A);
            });
            cx.group(acc::FACE, v3(0.0, 19.5, FACE_Z), |b, _| {
                b.on(HEAD);
                b.bx(v3(0.0, 19.4, FACE_Z + 0.6), v3(9.4, 1.4, 7.0), A);
                b.cone_to(v3(0.0, 19.8, FACE_Z - 0.2), v3(0.0, 16.8, FACE_Z - 0.4), 4.4, A);
            });
        }
        Astronaut => {
            cx.group(acc::HEAD, hc, |b, _| {
                b.on(HEAD);
                b.el(hc + v3(0.0, 0.2, 0.2), v3(12.4, 12.0, 11.8), A);
                b.el(hc + v3(0.0, 0.2, -4.6), v3(9.0, 7.4, 2.4), C);
                b.cy(v3(0.0, 19.6, 0.0), 8.0, 1.4, D);
                b.bx(v3(4.6, TOP_Y + 0.8, hc.z + 1.0), v3(1.0, 3.0, 1.0), D);
                b.bx(v3(4.6, TOP_Y + 2.6, hc.z + 1.0), Vec3::ONE, C);
            });
            cx.group(acc::BACK, v3(0.0, 15.0, 2.5), |b, _| {
                b.bx(v3(0.0, 15.0, 4.8), v3(8.0, 8.0, 4.0), A);
                b.bx(v3(0.0, 15.0, 7.0), v3(6.0, 6.0, 0.6), D);
                for y in [13.0f32, 15.0, 17.0] {
                    b.bx(v3(-1.8, y, 7.3), Vec3::ONE, C);
                }
                b.bar(v3(3.0, 18.0, 4.0), v3(4.8, 20.0, 1.0), 1.0, D);
            });
        }
        Ninja => {
            cx.group(acc::HEAD, top, |b, _| {
                b.on(HEAD);
                b.bx(hc + v3(0.0, 0.2, 0.1), v3(9.8, 9.6, 9.2), D);
                b.bx(v3(0.0, 23.9, FACE_Z - 0.4), v3(8.4, 2.4, 0.6), P);
                eyes(b, 23.9, FACE_Z - 0.8, 2.1, 1.6, 1.2);
                b.bx(v3(0.0, 26.2, hc.z), v3(10.2, 1.2, 9.6), A);
                b.bx(v3(0.0, 26.2, FACE_Z - 0.6), v3(2.0, 1.2, 0.6), C);
                let pv = v3(0.0, 26.2, BACK_Z + 0.4);
                b.part(HEAD, pv, PartRole::Cloth);
                b.sym(|b, s| {
                    b.flat(PrimKind::Box, pv, pv + v3(s * 1.6, -2.4, 4.2), 1.2, 0.6, Vec3::X, A);
                });
            });
        }
        Robot => cx.group(acc::HEAD, top, |b, r| {
            b.on(HEAD);
            b.bx(hc, v3(10.0, 9.0, 9.0), A);
            b.bx(hc + v3(0.0, 0.0, 0.0), v3(10.4, 1.0, 9.4), D);
            b.bx(v3(0.0, 24.0, FACE_Z - 0.4), v3(8.0, 2.0, 0.6), D);
            let eye_style = r.pick(2);
            if eye_style == 0 {
                b.bx(v3(0.0, 24.0, FACE_Z - 0.6), v3(6.4, 1.0, 0.6), C);
            } else {
                eyes(b, 24.0, FACE_Z - 0.7, 2.2, 1.6, 1.6);
            }
            for x in [-2.4f32, -0.8, 0.8, 2.4] {
                b.bx(v3(x, 21.2, FACE_Z - 0.4), v3(0.8, 1.2, 0.6), D);
            }
            b.sym(|b, s| {
                b.cx(v3(s * 5.4, 23.6, hc.z), 3.0, 1.0, D);
                b.cx(v3(s * 5.9, 23.6, hc.z), 1.0, 1.0, C);
            });
            let base = v3(2.4, TOP_Y + 0.5, hc.z);
            b.part(HEAD, base, PartRole::Other);
            b.bar(base, base + v3(0.0, 4.0, 0.0), 0.8, D);
            b.bx(base + v3(0.0, 4.4, 0.0), Vec3::splat(1.4), C);
        }),
        Skeleton => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.bx(hc + v3(0.0, 0.8, 0.0), v3(9.0, 7.6, 8.4), A);
            b.bx(v3(0.0, 20.4, hc.z - 1.0), v3(7.0, 2.4, 6.0), A);
            b.sym(|b, s| {
                b.bx(v3(s * 2.1, 23.8, FACE_Z - 0.2), v3(2.6, 2.6, 0.8), D);
                b.bx(v3(s * 2.1, 23.6, FACE_Z - 0.3), v3(1.0, 1.0, 0.8), C);
            });
            b.bx(v3(0.0, 22.0, FACE_Z - 0.2), v3(1.0, 1.2, 0.8), D);
            for x in [-2.0f32, -1.0, 0.0, 1.0, 2.0] {
                b.bx(v3(x, 20.8, FACE_Z + 0.3), v3(0.6, 1.4, 0.8), C);
            }
        }),
        Spiky => cx.group(acc::HEAD, top, |b, r| {
            b.on(HEAD);
            b.bx(v3(0.0, TOP_Y, hc.z + 0.4), v3(9.8, 2.4, 9.4), D);
            b.bx(v3(0.0, 24.2, BACK_Z), v3(9.8, 6.0, 1.6), D);
            let n = 7 + r.pick(3) as usize;
            for i in 0..n {
                let a = -1.2 + 2.4 * i as f32 / (n - 1) as f32;
                let dir = v3(a.sin() * 0.9, a.cos() * 1.0, 0.5 + 0.3 * (i % 2) as f32).normalize();
                let base = v3(a.sin() * 3.0, TOP_Y + 0.6, hc.z + 1.0);
                b.cone_to(base, base + dir * (5.5 + (i % 3) as f32 * 1.2), 3.2, D);
            }
            for x in [-2.5f32, 0.0, 2.5] {
                let base = v3(x, 26.8, FACE_Z + 1.0);
                b.cone_to(base, base + v3(x * 0.2, -2.6, -1.6), 2.0, D);
            }
        }),
        Chef => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.cy(v3(0.0, TOP_Y + 2.0, hc.z), 8.6, 4.0, C);
            b.el(v3(0.0, TOP_Y + 5.4, hc.z), v3(11.0, 4.6, 10.4), C);
            b.cy(v3(0.0, TOP_Y + 0.4, hc.z), 9.6, 1.0, A);
        }),
        Crown => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.cy(v3(0.0, TOP_Y + 0.8, hc.z), 9.4, 2.0, A);
            for i in 0..6 {
                let a = i as f32 / 6.0 * std::f32::consts::TAU;
                let p = v3(a.sin() * 4.2, TOP_Y + 2.0, hc.z - a.cos() * 4.2);
                b.cone_to(p, p + Vec3::Y * 2.4, 1.6, A);
            }
            b.bx(v3(0.0, TOP_Y + 1.0, FACE_Z - 0.2), v3(1.4, 1.4, 0.6), C);
        }),
        Goggles => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.el(v3(0.0, TOP_Y - 0.2, hc.z + 0.4), v3(10.0, 4.4, 9.6), D);
            b.bx(v3(0.0, 25.6, hc.z), v3(9.8, 1.0, 9.2), D);
            b.sym(|b, s| {
                b.cz(v3(s * 2.2, 26.0, FACE_Z - 0.6), 3.4, 1.2, A);
                b.cz(v3(s * 2.2, 26.0, FACE_Z - 1.1), 2.2, 0.6, C);
                b.cx(v3(s * 4.9, 23.4, hc.z), 2.2, 0.6, D);
            });
        }),
        Cap => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.el(v3(0.0, TOP_Y + 0.2, hc.z + 0.3), v3(10.0, 4.0, 9.6), D);
            b.bx(v3(0.0, TOP_Y - 0.6, FACE_Z - 1.8), v3(8.0, 0.8, 4.0), D);
            b.bx(v3(0.0, TOP_Y + 0.8, FACE_Z + 0.3), v3(1.4, 1.4, 0.6), C);
        }),
        Tv => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.bx(hc + v3(0.0, 0.5, 0.5), v3(12.0, 9.6, 7.6), P);
            b.bx(hc + v3(-0.8, 0.5, -3.3), v3(9.0, 7.4, 0.8), D);
            b.bx(hc + v3(-0.8, 0.5, -3.6), v3(8.0, 6.4, 0.6), C);
            b.bx(hc + v3(-1.6, 1.4, -3.9), v3(1.2, 1.6, 0.6), D);
            b.bx(hc + v3(0.4, 1.4, -3.9), v3(1.2, 1.6, 0.6), D);
            b.bx(hc + v3(-0.6, -0.8, -3.9), v3(3.4, 0.6, 0.6), D);
            b.bx(hc + v3(5.0, 2.0, -3.4), v3(1.2, 1.2, 0.8), A);
            b.bx(hc + v3(5.0, -0.6, -3.4), v3(1.2, 1.2, 0.8), A);
            let t = v3(0.0, TOP_Y + 1.0, hc.z + 0.5);
            b.sym(|b, s| {
                b.bar(t, t + v3(s * 3.0, 4.6, 0.6), 0.7, D);
                b.bx(t + v3(s * 3.1, 4.8, 0.6), Vec3::ONE, C);
            });
        }),
        Toaster => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.bx(hc + v3(0.0, 0.0, 0.2), v3(11.0, 8.8, 7.2), A);
            b.bx(hc + v3(0.0, -4.0, 0.2), v3(11.4, 0.8, 7.6), D);
            for z in [-1.2f32, 1.6] {
                b.bx(v3(0.0, TOP_Y - 0.1, hc.z + z), v3(7.6, 0.6, 1.4), D);
                b.bx(v3(0.0, TOP_Y + 1.2, hc.z + z), v3(6.4, 3.0, 1.0), P);
                b.bx(v3(0.0, TOP_Y + 2.6, hc.z + z), v3(6.6, 0.8, 1.2), D);
            }
            b.bx(v3(5.8, 24.8, hc.z), v3(1.0, 1.0, 2.4), D);
            b.bx(v3(-5.8, 22.0, hc.z - 1.0), v3(0.8, 1.6, 1.6), C);
            eyes(b, 23.2, FACE_Z + 0.4, 2.2, 1.4, 1.4);
        }),
        Mushroom => cx.group(acc::HEAD, top, |b, r| {
            b.on(HEAD);
            let y = TOP_Y + 0.8;
            b.el(v3(0.0, y + 0.8, hc.z), v3(16.0, 7.0, 15.0), A);
            b.el(v3(0.0, y - 1.6, hc.z), v3(13.0, 1.2, 12.0), D);
            eyes(b, 23.2, FACE_Z + 0.4, 2.1, 1.3, 2.0);
            for i in 0..5 {
                let a = i as f32 * 1.3 + r.f();
                let p = v3(a.sin() * 4.5, y + 3.2 - (i % 2) as f32 * 1.0, hc.z + a.cos() * 4.5);
                b.el(p, v3(2.4, 1.4, 2.4), C);
            }
        }),
        Cactus => cx.group(acc::HEAD, top, |b, r| {
            b.on(HEAD);
            b.bx(hc + v3(0.0, 1.2, 0.0), v3(8.6, 11.0, 8.2), P);
            for x in [-2.6f32, 0.0, 2.6] {
                b.bx(v3(x, 24.7, FACE_Z - 0.1), v3(0.8, 10.0, 0.6), A);
            }
            b.sym(|b, s| {
                b.bx(v3(s * 5.8, 23.2, hc.z), v3(3.0, 2.6, 2.6), P);
                b.bx(v3(s * 6.6, 26.4, hc.z), v3(2.6, 5.0, 2.6), P);
            });
            for i in 0..10 {
                let y = 20.0 + (i as f32 * 1.1) % 10.0;
                let x = -4.0 + 8.0 * r.f();
                b.bx(v3(x, y, FACE_Z - 0.6), Vec3::splat(0.6), C);
            }
            eyes(b, 23.0, FACE_Z - 0.3, 2.0, 1.2, 1.8);
            b.el(v3(0.0, TOP_Y + 2.0, hc.z), v3(3.6, 2.0, 3.6), A);
            b.bx(v3(0.0, TOP_Y + 2.6, hc.z), Vec3::ONE, C);
        }),
        Pumpkin => cx.group(acc::HEAD, top, |b, _| {
            b.on(HEAD);
            b.el(hc + v3(0.0, 0.3, 0.0), v3(12.0, 10.0, 11.0), A);
            for x in [-3.0f32, 0.0, 3.0] {
                b.el(hc + v3(x, 0.3, 0.0), v3(1.0, 10.2, 11.2), D);
            }
            b.cy(v3(0.0, TOP_Y + 1.6, hc.z), 1.6, 2.4, D);
            b.sym(|b, s| {
                b.cone_to(v3(s * 2.3, 23.2, FACE_Z - 0.6), v3(s * 2.3, 25.2, FACE_Z - 0.6), 2.4, C);
            });
            b.bx(v3(0.0, 21.2, FACE_Z - 0.7), v3(5.0, 1.2, 0.6), C);
            for x in [-1.6f32, 0.8] {
                b.bx(v3(x, 21.9, FACE_Z - 0.7), v3(0.8, 0.6, 0.6), C);
            }
        }),
    }
}

// ------------------------------------------------------------------------------ creature heads

pub(super) fn beast_head(cx: &mut Cx, beast: Beast, h: &Hints, donor: &Recipe) {
    use Beast::*;
    let hc = rig::HC;
    let b = &mut cx.b;
    let r = &mut cx.r;
    let fz = FACE_Z;
    b.on(HEAD);
    match beast {
        Canine => {
            b.bx(hc + v3(0.0, 0.3, 0.6), v3(8.6, 8.2, 7.6), P);
            // Snout + nose + mouth line.
            b.bx(v3(0.0, 21.8, fz - 1.8), v3(4.6, 3.6, 4.4), P);
            b.bx(v3(0.0, 20.6, fz - 1.6), v3(4.0, 1.4, 4.0), if h.fox { C } else { A });
            b.bx(v3(0.0, 23.2, fz - 3.8), v3(1.8, 1.4, 1.0), D);
            if h.fox {
                b.sym(|b, s| {
                    b.el(v3(s * 3.0, 21.2, fz + 0.2), v3(3.4, 3.4, 3.0), C);
                });
            }
            eyes(b, 24.6, fz + 0.4, 2.2, 1.4, 1.2);
            b.sym(|b, s| {
                let base = v3(s * 2.6, TOP_Y - 0.5, hc.z + 1.0);
                b.cone_to(base, base + v3(s * 1.0, 5.0, 0.4), 3.4, P);
                b.cone_to(base + v3(0.0, 0.5, -0.6), base + v3(s * 0.9, 4.0, -0.2), 1.8, if h.fox { D } else { A });
            });
        }
        Equine => {
            b.bx(hc + v3(0.0, 0.8, 0.6), v3(8.0, 8.0, 7.6), P);
            b.bxr(v3(0.0, 21.2, fz - 2.6), v3(5.0, 5.0, 6.4), v3(18.0, 0.0, 0.0), P);
            b.bxr(v3(0.0, 20.2, fz - 3.6), v3(5.2, 2.6, 4.6), v3(18.0, 0.0, 0.0), A);
            b.sym(|b, s| {
                b.bx(v3(s * 1.3, 20.6, fz - 5.9), v3(1.0, 1.0, 0.6), D);
            });
            b.sym(|b, s| {
                b.bx(v3(s * 3.9, 24.8, fz + 1.4), v3(0.8, 1.6, 1.6), C);
            });
            let el = if h.long_ears { 6.6 } else { 4.4 };
            let droop = if h.long_ears { -0.3 } else { 0.0 };
            b.sym(|b, s| {
                let base = v3(s * 2.6, TOP_Y, hc.z + 1.6);
                b.flat(PrimKind::Ellipsoid, base, base + v3(s * (1.6 - droop * 4.0), el, 0.8), 2.6, 1.4, Vec3::Z, P);
                b.flat(PrimKind::Ellipsoid, base + v3(0.0, 0.8, -0.5), base + v3(s * (1.5 - droop * 4.0), el * 0.85, 0.3), 1.2, 1.0, Vec3::Z, D);
            });
            // Mane.
            b.bx(v3(0.0, TOP_Y + 0.6, hc.z + 1.6), v3(1.6, 1.6, 6.0), D);
            b.bx(v3(0.0, 23.0, BACK_Z + 0.4), v3(1.6, 6.0, 1.4), D);
            if h.horn {
                b.cone_to(v3(0.0, TOP_Y, fz + 1.4), v3(0.0, TOP_Y + 6.0, fz - 1.2), 2.2, C);
            }
        }
        Bovine => {
            b.bx(hc + v3(0.0, 0.5, 0.4), v3(9.4, 8.4, 8.0), P);
            b.bx(v3(0.0, 21.0, fz - 1.0), v3(7.0, 4.4, 2.8), A);
            b.sym(|b, s| {
                b.bx(v3(s * 1.6, 21.4, fz - 2.5), v3(1.2, 1.2, 0.6), D);
                b.bx(v3(s * 5.4, 24.6, hc.z), v3(2.8, 1.4, 2.4), P);
                let base = v3(s * 4.4, TOP_Y - 1.0, hc.z - 0.5);
                let m = base + v3(s * 2.8, 0.6, -0.4);
                b.segx(PrimKind::Cylinder, base, m, 2.2, 0.4, A);
                b.cone_to(m, m + v3(s * 0.6, 3.4, -1.0), 2.2, A);
                b.cone_to(m + v3(s * 0.4, 2.0, -0.6), m + v3(s * 0.6, 3.4, -1.0), 1.2, C);
            });
            eyes(b, 24.4, fz - 0.1, 2.4, 1.4, 1.4);
            b.bx(v3(0.0, TOP_Y + 0.4, hc.z), v3(4.0, 1.4, 3.0), D);
        }
        Feline => {
            b.bx(hc + v3(0.0, 0.2, 0.3), v3(9.4, 8.2, 8.0), P);
            b.bx(v3(0.0, 21.6, fz - 1.0), v3(4.4, 2.8, 2.4), A);
            b.bx(v3(0.0, 22.8, fz - 2.3), v3(1.4, 1.0, 0.6), D);
            b.sym(|b, s| {
                let base = v3(s * 3.0, TOP_Y - 0.6, hc.z + 0.4);
                b.cone_to(base, base + v3(s * 1.2, 4.2, 0.2), 3.6, P);
                b.cone_to(base + v3(0.0, 0.4, -0.6), base + v3(s * 1.1, 3.4, -0.4), 1.8, A);
                for k in [0.0f32, 1.0] {
                    b.flat(PrimKind::Box, v3(s * 2.0, 21.8 - k, fz - 1.8), v3(s * 6.0, 22.4 - k * 1.6, fz - 1.4), 0.6, 0.5, Vec3::Z, C);
                }
            });
            eyes(b, 24.6, fz - 0.1, 2.3, 1.8, 1.6);
        }
        Ursine => {
            b.el(hc + v3(0.0, 0.2, 0.2), v3(10.4, 9.4, 9.0), P);
            b.el(v3(0.0, 21.8, fz - 0.8), v3(5.0, 3.6, 3.6), A);
            b.bx(v3(0.0, 22.8, fz - 2.4), v3(2.0, 1.4, 0.8), D);
            b.sym(|b, s| {
                b.cz(v3(s * 3.8, TOP_Y + 0.2, hc.z + 1.0), 3.8, 1.6, P);
                b.cz(v3(s * 3.8, TOP_Y + 0.2, hc.z + 0.3), 2.0, 0.6, A);
            });
            eyes(b, 24.4, fz + 0.1, 2.2, 1.3, 1.3);
        }
        Rodent => {
            b.el(hc + v3(0.0, 0.3, 0.2), v3(10.0, 9.4, 9.0), P);
            b.el(v3(0.0, 21.8, fz - 0.4), v3(4.0, 2.6, 2.6), A);
            b.bx(v3(0.0, 22.6, fz - 1.6), v3(1.2, 0.8, 0.6), D);
            let el = if h.long_ears { 6.6 } else { 4.0 };
            b.sym(|b, s| {
                let base = v3(s * 2.8, TOP_Y - 0.8, hc.z + 0.6);
                b.flat(PrimKind::Ellipsoid, base, base + v3(s * 2.4, el, 0.4), 2.8, 1.4, Vec3::Z, P);
                if h.cheeks {
                    b.flat(PrimKind::Box, base + v3(s * 2.0, el * 0.72, 0.0), base + v3(s * 2.4, el, 0.4), 1.8, 1.6, Vec3::Z, D);
                    b.cz(v3(s * 3.6, 21.8, fz + 0.2), 2.6, 0.8, C);
                }
            });
            eyes(b, 24.2, fz + 0.2, 2.2, 1.4, 1.8);
        }
        Bird => {
            b.el(hc + v3(0.0, 0.3, 0.3), v3(9.6, 9.6, 9.2), P);
            b.cone_to(v3(0.0, 22.8, fz + 0.6), v3(0.0, 21.8, fz - 4.4), 4.2, A);
            b.bx(v3(0.0, 21.8, fz - 1.4), v3(3.6, 0.6, 3.0), D);
            b.el(v3(0.0, 21.0, fz + 1.2), v3(6.4, 3.4, 3.0), A);
            eyes(b, 24.8, fz + 0.6, 2.4, 1.6, 1.8);
            for i in 0..3 {
                let base = v3(0.0, TOP_Y - 0.2, hc.z + i as f32 * 1.4);
                b.flat(PrimKind::Box, base, base + v3(0.0, 3.6 - i as f32 * 0.6, 2.2), 1.8, 0.8, Vec3::X, A);
            }
        }
        Shark => {
            b.el(hc + v3(0.0, 0.4, -0.6), v3(9.4, 9.2, 10.4), P);
            b.el(v3(0.0, 20.6, fz - 0.2), v3(8.4, 3.0, 5.4), A);
            b.bx(v3(0.0, 21.2, fz - 1.8), v3(7.0, 0.8, 2.6), D);
            for x in [-2.4f32, -0.8, 0.8, 2.4] {
                b.cone_to(v3(x, 21.8, fz - 2.4), v3(x, 20.6, fz - 2.6), 1.0, C);
            }
            b.sym(|b, s| {
                b.bx(v3(s * 4.3, 24.2, fz + 2.0), v3(0.8, 1.2, 1.2), C);
                for k in 0..3 {
                    b.bx(v3(s * 4.7, 22.0, hc.z + 0.5 + k as f32 * 1.1), v3(0.6, 2.4, 0.6), D);
                }
            });
            b.flat(PrimKind::Cone, v3(0.0, TOP_Y - 0.6, hc.z + 0.6), v3(0.0, TOP_Y + 5.0, hc.z + 3.2), 5.0, 1.0, Vec3::X, P);
        }
        Dragon | Dino => {
            let dragon = beast == Dragon;
            b.bx(hc + v3(0.0, 0.8, 0.8), v3(8.4, 8.0, 7.6), P);
            b.bx(v3(0.0, 22.6, fz - 2.2), v3(5.6, 3.6, 6.0), P);
            b.bx(v3(0.0, 24.6, fz - 0.4), v3(7.8, 1.2, 3.0), A);
            b.bx(v3(0.0, 20.0, fz - 1.8), v3(5.2, 1.8, 5.6), A);
            b.bx(v3(0.0, 21.2, fz - 1.8), v3(5.0, 0.6, 5.4), D);
            b.sym(|b, s| {
                b.bx(v3(s * 1.4, 23.8, fz - 5.0), v3(0.8, 0.8, 0.6), D);
                for k in 0..3 {
                    b.cone_to(v3(s * 2.2, 21.4, fz - 4.4 + k as f32 * 1.6), v3(s * 2.2, 20.4, fz - 4.4 + k as f32 * 1.6), 1.0, C);
                }
                b.bx(v3(s * 2.8, 25.4, fz + 0.2), v3(1.8, 0.8, 0.6), C);
            });
            if dragon {
                b.sym(|b, s| {
                    let base = v3(s * 2.6, TOP_Y, hc.z + 1.6);
                    b.cone_to(base, base + v3(s * 1.4, 3.2, 4.8), 2.2, A);
                    b.cone_to(base + v3(s * 1.0, 2.2, 3.4), base + v3(s * 1.4, 3.2, 4.8), 1.2, C);
                });
                for k in 0..3 {
                    let p = v3(0.0, TOP_Y - 0.4 - k as f32 * 2.2, BACK_Z + 0.2);
                    b.cone_to(p, p + v3(0.0, 1.6, 2.2), 1.8, A);
                }
            } else {
                for k in 0..3 {
                    b.el(v3(0.0, TOP_Y - 0.2, hc.z - 1.0 + k as f32 * 2.2), v3(1.6, 1.4, 1.6), A);
                }
            }
        }
        Insect => {
            b.el(hc + v3(0.0, 0.2, 0.2), v3(9.0, 8.6, 8.4), P);
            b.sym(|b, s| {
                b.el(v3(s * 3.4, 24.0, fz + 1.2), v3(3.6, 4.6, 4.0), C);
                b.el(v3(s * 3.6, 24.2, fz + 0.8), v3(2.4, 3.2, 2.8), D);
                let base = v3(s * 1.8, TOP_Y - 0.4, fz + 1.5);
                let mid = base + v3(s * 1.6, 4.4, -1.4);
                b.bar(base, mid, 0.8, D);
                b.bar(mid, mid + v3(s * 1.4, 1.2, -1.8), 0.8, D);
                b.el(mid + v3(s * 1.5, 1.3, -1.9), Vec3::splat(1.6), C);
                b.flat(PrimKind::Box, v3(s * 1.0, 20.6, fz), v3(s * 0.2, 19.6, fz - 2.0), 1.2, 1.0, Vec3::Y, A);
            });
        }
        Serpent => {
            b.el(hc + v3(0.0, -0.2, -0.4), v3(9.4, 7.4, 10.6), P);
            b.bx(v3(0.0, TOP_Y - 1.6, hc.z), v3(3.6, 1.0, 8.4), A);
            b.sym(|b, s| {
                b.bx(v3(s * 2.8, 24.0, fz - 0.2), v3(1.8, 0.8, 0.6), C);
                b.cone_to(v3(s * 1.8, 21.4, fz - 0.6), v3(s * 1.8, 19.2, fz - 0.8), 1.2, C);
            });
            b.bar(v3(0.0, 21.0, fz - 1.0), v3(0.0, 21.0, fz - 3.0), 0.6, C);
            b.sym(|b, s| {
                b.bar(v3(0.0, 21.0, fz - 3.0), v3(s * 0.8, 21.0, fz - 4.0), 0.6, C);
            });
        }
        Octopus => {
            b.el(hc + v3(0.0, 1.6, 0.6), v3(10.0, 11.6, 10.0), P);
            b.el(hc + v3(0.0, 4.2, 1.6), v3(6.0, 5.0, 6.0), A);
            b.sym(|b, s| {
                b.el(v3(s * 2.4, 23.6, fz + 0.4), v3(2.8, 2.8, 1.6), C);
                b.bx(v3(s * 2.4, 23.4, fz - 0.4), v3(1.4, 0.8, 0.6), D);
            });
            // Tentacle beard (static curls).
            for (i, x) in [-3.0f32, -1.2, 0.6, 2.4].into_iter().enumerate() {
                let top = v3(x, 21.4, fz + 0.4);
                let mid = top + v3(x * 0.2, -3.0, -0.8 - (i % 2) as f32 * 0.6);
                let tip = mid + v3(x * 0.3, -2.0, 0.6);
                b.segx(PrimKind::Box, top, mid, 1.6, 0.3, P);
                b.segx(PrimKind::Box, mid, tip, 1.2, 0.2, P);
                b.bx(tip + v3(x * 0.1, 0.0, 0.3), v3(1.2, 1.0, 1.0), A);
                b.bx(mid + v3(0.0, 0.0, -0.8), Vec3::splat(0.6), C);
            }
        }
        Spider => {
            b.el(hc + v3(0.0, 0.2, 0.4), v3(9.6, 8.8, 9.2), P);
            for (x, y, sz) in [(-1.2f32, 24.8f32, 1.6f32), (1.2, 24.8, 1.6), (-3.0, 25.6, 1.2), (3.0, 25.6, 1.2), (-2.4, 23.2, 1.0), (2.4, 23.2, 1.0), (-0.8, 26.6, 0.8), (0.8, 26.6, 0.8)] {
                b.bx(v3(x, y, fz - 0.2), v3(sz, sz, 0.8), C);
            }
            b.sym(|b, s| {
                b.cone_to(v3(s * 1.4, 21.6, fz - 0.4), v3(s * 0.8, 19.0, fz - 1.4), 1.8, A);
            });
            for k in 0..3 {
                b.bx(v3(0.0, TOP_Y - 0.2, hc.z - 1.0 + k as f32 * 2.0), v3(3.0, 0.8, 1.0), D);
            }
        }
    }
    b.on(0);
    let _ = (r, donor);
}

/// Creature extras: tail stub, back fins/plates, abdomen.
pub(super) fn beast_extras(cx: &mut Cx, beast: Beast, h: &Hints, donor: &Recipe) {
    use Beast::*;
    let pelvis = v3(0.0, 11.6, 2.4);
    let electric = donor.element == Element::Electric || h.cheeks;
    if donor.has(Mechanic::Charge) && !matches!(beast, Bovine | Dragon) {
        cx.group(acc::HEAD, v3(0.0, TOP_Y, 0.0), |b, _| {
            b.on(HEAD);
            b.sym(|b, s| {
                let base = v3(s * 2.6, TOP_Y - 0.4, rig::HC.z - 1.0);
                b.cone_to(base, base + v3(s * 1.2, 3.6, -0.6), 2.0, A);
            });
        });
    }
    let tail = |cx: &mut Cx, segs: &[(f32, f32, bd_core::shape::Tint)], dir: Vec3, bend: f32| {
        cx.group(acc::TAIL, pelvis, |b, _| {
            let mut p = pelvis;
            let mut d = dir.normalize();
            let mut parent = 0;
            for (len, w, t) in segs {
                let id = b.part(parent, p, PartRole::Tail);
                let e = p + d * *len;
                b.segx(PrimKind::Ellipsoid, p, e, *w, 0.6, *t);
                parent = id;
                p = e;
                d = (Quat::from_rotation_x(bend) * d).normalize();
            }
        });
    };
    match beast {
        Canine => {
            if h.fox {
                tail(cx, &[(2.2, 2.8, P), (2.4, 3.8, P), (1.8, 2.6, C)], v3(0.0, -0.3, 1.0), -0.35);
            } else {
                tail(cx, &[(2.0, 2.0, P), (2.4, 1.8, P)], v3(0.0, 0.3, 1.0), -0.3);
            }
        }
        Feline => tail(cx, &[(2.2, 1.4, P), (2.2, 1.2, P), (2.0, 1.2, A)], v3(0.0, -0.4, 1.0), -0.6),
        Equine | Bovine => tail(cx, &[(2.0, 1.2, P), (2.2, 2.0, D)], v3(0.0, -0.8, 1.0), 0.2),
        Ursine => tail(cx, &[(1.2, 2.4, P)], v3(0.0, 0.0, 1.0), 0.0),
        Rodent => {
            if electric {
                // Lightning-bolt tail.
                cx.group(acc::TAIL, pelvis, |b, _| {
                    b.part(0, pelvis, PartRole::Tail);
                    let pts = [pelvis, pelvis + v3(0.0, 1.8, 2.2), pelvis + v3(0.0, 1.2, 3.6), pelvis + v3(0.0, 4.2, 5.4), pelvis + v3(0.0, 3.6, 6.4)];
                    for w in pts.windows(2) {
                        b.segx(PrimKind::Box, w[0], w[1], 1.6, 0.4, A);
                    }
                    b.bx(pts[4] + v3(0.0, 1.8, 0.2), v3(1.0, 3.0, 2.6), A);
                    b.bx(pelvis + v3(0.0, 0.6, 1.2), v3(1.2, 1.4, 1.4), D);
                });
            } else {
                tail(cx, &[(2.2, 1.0, P), (2.2, 0.8, P)], v3(0.0, -0.2, 1.0), 0.4);
            }
        }
        Bird => cx.group(acc::TAIL, pelvis, |b, _| {
            b.part(0, pelvis, PartRole::Tail);
            for i in -1..=1 {
                let d = Quat::from_rotation_y(i as f32 * 0.35) * v3(0.0, -0.3, 1.0);
                b.flat(PrimKind::Box, pelvis, pelvis + d * 4.2, 1.8, 0.8, Vec3::Y, if i == 0 { A } else { P });
            }
        }),
        Shark => {
            tail(cx, &[(2.2, 2.4, P), (2.0, 1.6, P)], v3(0.0, -0.1, 1.0), 0.0);
            cx.group(acc::BACK, v3(0.0, 16.0, 2.5), |b, _| {
                b.flat(PrimKind::Cone, v3(0.0, 15.0, 2.6), v3(0.0, 19.6, 5.6), 5.0, 1.0, Vec3::X, P);
            });
        }
        Dragon | Dino => {
            tail(cx, &[(2.4, 3.2, P), (2.4, 2.4, P), (2.0, 1.6, A)], v3(0.0, -0.5, 1.0), 0.15);
            cx.group(acc::BACK, v3(0.0, 15.0, 2.5), |b, _| {
                for k in 0..3 {
                    let p = v3(0.0, 17.5 - k as f32 * 2.6, 2.6);
                    b.cone_to(p, p + v3(0.0, 1.4, 2.0), 2.0, A);
                }
            });
        }
        Insect => cx.group(acc::TAIL, pelvis, |b, _| {
            b.part(0, pelvis, PartRole::Tail);
            let c = pelvis + v3(0.0, -0.4, 2.6);
            b.el(c, v3(5.4, 5.0, 5.4), A);
            for z in [-0.9f32, 0.9] {
                b.el(c + v3(0.0, 0.0, z), v3(5.6, 5.2, 1.0), D);
            }
            b.cone_to(c + v3(0.0, -0.4, 2.2), c + v3(0.0, -1.0, 4.0), 1.4, C);
        }),
        Serpent => tail(cx, &[(2.4, 2.4, P), (2.2, 1.6, P), (1.6, 1.0, A)], v3(0.0, -0.6, 1.0), 0.3),
        Spider => cx.group(acc::BACK, v3(0.0, 14.0, 2.5), |b, _| {
            b.el(v3(0.0, 13.0, 5.4), v3(7.0, 6.4, 6.0), P);
            b.el(v3(0.0, 14.6, 7.6), v3(2.6, 2.6, 1.6), C);
        }),
        Octopus => {}
    }
}

// ------------------------------------------------------------------------- icons / mini items

/// Uniformly fit `prims` into `size` (units), bottom-centred at `at` (z centred).
fn fit_prims(prims: &mut [bd_core::shape::Prim], size: Vec3, at: Vec3) {
    let mut mn = Vec3::splat(f32::MAX);
    let mut mx = Vec3::splat(f32::MIN);
    for p in prims.iter() {
        let e = crate::voxelize::PrimEval::new(p.kind, p.center, p.size, p.rot_deg);
        mn = mn.min(e.lo.as_vec3());
        mx = mx.max(e.hi.as_vec3());
    }
    if mn.x > mx.x {
        return;
    }
    let ext = (mx - mn + Vec3::ONE).max(Vec3::ONE);
    let k = (size / ext).min_element().clamp(0.02, 20.0);
    let bottom = v3((mn.x + mx.x) * 0.5, mn.y, (mn.z + mx.z) * 0.5);
    for p in prims.iter_mut() {
        p.center = at + (p.center - bottom) * k;
        p.size *= k;
    }
}

fn sane_prims(spec: &ShapeSpec, keep: impl Fn(u8) -> bool, part: u8) -> Vec<bd_core::shape::Prim> {
    spec.prims
        .iter()
        .filter(|p| !p.subtract && keep(p.part) && p.center.is_finite() && p.size.is_finite() && p.rot_deg.is_finite() && p.size.max_element() < 4096.0)
        .take(240)
        .map(|p| {
            let mut q = p.clone();
            q.part = part;
            q
        })
        .collect()
}

/// Curated icon with a `Head` part: its head (+ jaw/horn children) replaces ours.
pub(super) fn icon_head(cx: &mut Cx, spec: &ShapeSpec) {
    let Some(hp) = spec.parts.iter().find(|p| p.role == PartRole::Head) else { return };
    let mut ids = vec![hp.id];
    loop {
        let n = ids.len();
        for p in &spec.parts {
            if ids.contains(&p.parent) && !ids.contains(&p.id) && p.id != 0 {
                ids.push(p.id);
            }
        }
        if ids.len() == n {
            break;
        }
    }
    let mut prims = sane_prims(spec, |p| ids.contains(&p), HEAD);
    if prims.is_empty() {
        cx.b.on(HEAD);
        skull(&mut cx.b);
        cx.b.on(0);
        return;
    }
    fit_prims(&mut prims, v3(11.0, 10.5, 13.0), v3(0.0, rig::NECK_Y + 0.6, rig::HC.z - 0.8));
    cx.b.spec.prims.extend(prims);
}

/// Objects become a mini helmet; held items are worn diagonally on the back.
pub(super) fn mini_construct(cx: &mut Cx, recipe: &Recipe, icon: Option<&ShapeSpec>) {
    let spec = match icon {
        Some(s) => s.clone(),
        None => {
            let mut r = recipe.clone();
            r.scale = crate::forms::suggested_scale(recipe.form);
            r.features.clear();
            crate::generate(&r)
        }
    };
    if recipe.form.category() == FormCategory::Held {
        cx.group(acc::BACK, v3(0.0, 15.0, 3.0), |b, _| {
            let mut prims = sane_prims(&spec, |_| true, 0);
            // Rotate to lie diagonally across the back (item points up-left), then fit.
            let q = Quat::from_rotation_z(-0.7) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            for p in &mut prims {
                p.center = q * p.center;
                p.rot_deg = crate::build::rot_deg_from_quat(q * crate::build::quat_from_rot_deg(p.rot_deg));
            }
            fit_prims(&mut prims, v3(13.0, 16.0, 3.0), v3(0.0, 9.0, 4.6));
            b.spec.prims.extend(prims);
        });
    } else {
        cx.group(acc::HEAD, v3(0.0, TOP_Y, 0.0), |b, _| {
            let mut prims = sane_prims(&spec, |_| true, HEAD);
            let tall = recipe.form == Form::Pillar;
            fit_prims(&mut prims, if tall { v3(8.0, 12.0, 8.0) } else { v3(11.5, 8.0, 11.0) }, v3(0.0, TOP_Y - 3.2, rig::HC.z + 0.2));
            b.spec.prims.extend(prims);
        });
    }
    let _ = Rng::new(0, 0);
}
