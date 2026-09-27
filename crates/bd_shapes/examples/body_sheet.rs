//! Player-body animation frame sheets (ANIMATION PASS), software-rasterised (no GPU).
//!
//! cargo run -p bd_shapes --example body_sheet --release -- [--before] [--seq] [--names naruto,knight]
//!     [--out PATH]
//!
//! Default: every player moment the client draws (run, strafe, air by vertical speed, crouch, boost,
//! landing, air jump, dodge, release, typing, oath, charge, respawn) as the NEW layer stack
//! (`present::body_motion`) → docs/animation/body_after.png. `--before`: the same moments with the
//! layer stack the client drew before this pass → docs/animation/body_before.png. `--seq`: the
//! one-shot actions over time → docs/animation/body_sequences.png.

use std::path::PathBuf;

use bd_core::mechanics::{Form, Recipe};
use bd_shapes::costume::{self, look};
use bd_shapes::*;
use glam::{Affine3A, Vec3};

#[path = "raster/mod.rs"]
mod raster;
use raster::{Canvas, Dressed, draw_posed, text, text_w};

type Layers = Vec<(AnimState, f32)>;

fn loco(speed: f32, air: bool) -> (AnimState, f32) {
    (AnimState::Locomotion { speed, distance: speed * 0.37, airborne: air }, 0.37)
}

fn motion(m: BodyMotion) -> (AnimState, f32) {
    (AnimState::Motion(m), 0.37)
}

fn mo() -> BodyMotion {
    BodyMotion::default()
}

/// (label, before, after). "Before" = what `present/players.rs` drew for the moment before this pass.
fn moments() -> Vec<(&'static str, Layers, Layers)> {
    let air = |fwd: f32, vy: f32, turn: f32| {
        vec![loco(0.0, false), motion(BodyMotion { fwd, vy, turn, air: 1.0, ..mo() })]
    };
    vec![
        ("IDLE", vec![loco(0.0, false)], vec![loco(0.0, false), motion(mo())]),
        ("RUN 8", vec![loco(8.0, false)], vec![loco(8.0, false), motion(BodyMotion { fwd: 8.0, ..mo() })]),
        ("BHOP RUN 24", vec![loco(24.0, false)], vec![loco(24.0, false), motion(BodyMotion { fwd: 24.0, ..mo() })]),
        ("STRAFE R", vec![loco(7.0, false)], vec![loco(7.0, false), motion(BodyMotion { side: 7.0, ..mo() })]),
        ("BACKPEDAL", vec![loco(6.0, false)], vec![loco(6.0, false), motion(BodyMotion { fwd: -6.0, ..mo() })]),
        ("CROUCH", vec![loco(0.0, false)], vec![loco(0.0, false), motion(BodyMotion { crouch: 1.0, ..mo() })]),
        ("HOP RISE", vec![loco(12.0, true)], air(12.0, 8.0, 0.0)),
        ("AIRSTRAFE L", vec![loco(20.0, true)], air(20.0, 0.0, 4.5)),
        ("FALL 12", vec![loco(6.0, true)], air(6.0, -12.0, 0.0)),
        ("PLUNGE 26", vec![loco(4.0, true)], air(4.0, -26.0, 0.0)),
        ("GAS BOOST", vec![loco(20.0, true)], vec![loco(0.0, false), motion(BodyMotion { fwd: 20.0, air: 1.0, boost: 1.0, ..mo() })]),
        (
            "LAND HARD",
            vec![loco(0.0, false)],
            vec![loco(0.0, false), motion(mo()), (AnimState::Action { kind: BodyAction::Land, dir: Vec3::NEG_Z, strength: 1.25 }, 0.1)],
        ),
        (
            "AIR JUMP",
            vec![loco(10.0, true)],
            vec![loco(0.0, false), motion(BodyMotion { fwd: 10.0, vy: 6.0, air: 1.0, ..mo() }), (AnimState::Action { kind: BodyAction::AirJump, dir: Vec3::NEG_Z, strength: 1.0 }, 0.14)],
        ),
        (
            "DODGE R",
            vec![loco(14.0, false)],
            vec![loco(14.0, false), motion(BodyMotion { side: 14.0, ..mo() }), (AnimState::Action { kind: BodyAction::Dodge, dir: Vec3::X, strength: 1.0 }, 0.09)],
        ),
        (
            "TETHER LET GO",
            vec![loco(22.0, true)],
            vec![loco(0.0, false), motion(BodyMotion { fwd: 22.0, vy: 4.0, air: 1.0, ..mo() }), (AnimState::Action { kind: BodyAction::Release, dir: Vec3::NEG_Z, strength: 1.0 }, 0.16)],
        ),
        ("TYPING", vec![loco(0.0, false)], vec![loco(0.0, false), motion(mo()), (AnimState::Channel { kind: Channel::Typing, w: 1.0 }, 0.6)]),
        ("OATH", vec![loco(0.0, false)], vec![loco(0.0, false), motion(mo()), (AnimState::Channel { kind: Channel::Oath, w: 1.0 }, 0.6)]),
        ("CHARGE", vec![loco(0.0, false)], vec![loco(0.0, false), motion(mo()), (AnimState::Channel { kind: Channel::Charge, w: 1.0 }, 0.6)]),
        ("RESPAWN", vec![loco(0.0, false)], vec![loco(0.0, false), (AnimState::Forming { duration: 0.55 }, 0.2)]),
    ]
}

/// One-shot actions over time (after only).
fn sequences() -> Vec<(&'static str, Vec<(f32, Layers)>)> {
    let ts = |d: f32| (0..6).map(move |k| d * k as f32 / 5.0);
    let act = |kind: BodyAction, dir: Vec3, strength: f32, base: Vec<(AnimState, f32)>| -> Vec<(f32, Layers)> {
        let d = kind.duration(strength);
        ts(d)
            .map(|t| {
                let mut l = base.clone();
                l.push((AnimState::Action { kind, dir, strength }, t));
                (t, l)
            })
            .collect()
    };
    let air = vec![loco(0.0, false), motion(BodyMotion { fwd: 10.0, vy: 3.0, air: 1.0, ..mo() })];
    let ground = vec![loco(0.0, false), motion(mo())];
    vec![
        ("LAND SOFT", act(BodyAction::Land, Vec3::NEG_Z, 0.4, ground.clone())),
        ("LAND HARD", act(BodyAction::Land, Vec3::NEG_Z, 1.3, ground.clone())),
        ("AIR JUMP FWD", act(BodyAction::AirJump, Vec3::NEG_Z, 1.0, air.clone())),
        ("AIR JUMP SIDE", act(BodyAction::AirJump, Vec3::X, 1.0, air.clone())),
        ("DODGE LEFT", act(BodyAction::Dodge, Vec3::NEG_X, 1.0, ground.clone())),
        ("RELEASE FAST", act(BodyAction::Release, Vec3::NEG_Z, 1.0, air.clone())),
        (
            "RESPAWN",
            (0..6)
                .map(|k| {
                    let t = 0.55 * k as f32 / 5.0;
                    (t, vec![loco(0.0, false), (AnimState::Forming { duration: 0.55 }, t)])
                })
                .collect(),
        ),
    ]
}

fn dressed(name: &str) -> Dressed {
    let tweaks = CostumeTweaks::default();
    if let Some(f) = look::find(name) {
        let c = costume::character_costume(f.index, 0, tweaks);
        let pm = costume::costume_model_painted(&c);
        return Dressed::new(name.to_uppercase(), String::new(), pm.model, Some(pm.colors.as_slice()), 0.0);
    }
    let mut r = Recipe::unknown(name);
    r.form = Form::Biped;
    let spec = costume_with(&r, None, None, 3, tweaks);
    Dressed::new(name.to_uppercase(), String::new(), costume_model(&spec), None, 0.0)
}

fn posed(d: &Dressed, layers: &[(AnimState, f32)]) -> Vec<Affine3A> {
    let mut p = vec![Affine3A::IDENTITY; d.model.parts.len()];
    pose_layered(&d.model, layers, &mut p);
    p
}

const EYE: Vec3 = Vec3::new(0.85, 0.3, -0.75);
const SIDE: Vec3 = Vec3::new(1.0, 0.12, 0.0);
const TILE: i32 = 132;
const TH: i32 = 190;
const SCALE: f32 = 62.0;

fn tile(c: &mut Canvas, d: &Dressed, layers: &[(AnimState, f32)], eye: Vec3, x0: i32, y0: i32, label: &str, tint: [u8; 3]) {
    c.rect(x0 + 1, y0 + 1, TILE - 2, TH - 2, [24, 27, 35]);
    // Ground line.
    c.rect(x0 + 8, y0 + TH - 30, TILE - 16, 1, [60, 66, 80]);
    let p = posed(d, layers);
    draw_posed(c, d, &p, eye, x0, y0 + 14, TILE, TH - 14, SCALE, 16.0);
    let lw = text_w(label, 1);
    text(c, x0 + (TILE - lw) / 2, y0 + 4, label, tint, 1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let val = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let root = std::env::var("BD_ROOT").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let dir = root.join("docs/animation");
    let _ = std::fs::create_dir_all(&dir);
    let names: Vec<String> = val("--names").map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_else(|| vec!["naruto".into(), "knight".into()]);
    let bodies: Vec<Dressed> = names.iter().map(|n| dressed(n)).collect();

    if has("--seq") {
        let seqs = sequences();
        let cols = 6;
        let head = 26;
        let mut c = Canvas::new((cols * TILE + 120) as usize, (seqs.len() as i32 * TH * bodies.len() as i32 + head) as usize);
        text(&mut c, 8, 8, "ANIMATION PASS - ONE-SHOT BODY ACTIONS OVER TIME (3/4 VIEW; FLIPS FROM THE SIDE)", [215, 225, 235], 1);
        let mut row = 0;
        for d in &bodies {
            for (label, frames) in &seqs {
                let y0 = head + row * TH;
                text(&mut c, 6, y0 + TH / 2 - 8, label, [230, 230, 236], 1);
                text(&mut c, 6, y0 + TH / 2 + 4, &d.label, [120, 130, 145], 1);
                let eye = if label.contains("AIR JUMP FWD") || label.contains("RELEASE") { SIDE } else { EYE };
                for (k, (t, l)) in frames.iter().enumerate() {
                    tile(&mut c, d, l, eye, 120 + k as i32 * TILE, y0, &format!("{:.0} MS", t * 1000.0), [150, 170, 160]);
                }
                row += 1;
            }
        }
        let out = val("--out").map(PathBuf::from).unwrap_or(dir.join("body_sequences.png"));
        c.save(&out);
        println!("wrote {}", out.display());
        return;
    }

    if has("--probe") {
        // Lowest posed voxel (m) per moment: feet must stay on the ground line (0).
        for d in &bodies {
            println!("{}: leg_len {:.2} height {:.2} kinds {:?}", d.label, d.model.rig.leg_len, d.model.rig.height, d.model.rig.kinds);
            for (label, _, a) in moments() {
                let p = posed(d, &a);
                let mut lo = f32::MAX;
                for (i, part) in d.model.parts.iter().enumerate() {
                    for (v, _) in &part.voxels {
                        lo = lo.min(p[i].transform_point3(part.voxel_center(*v)).y);
                    }
                }
                println!("  {label:14} min y {lo:+.3}");
            }
        }
        return;
    }
    let before = has("--before");
    let ms = moments();
    let head = 26;
    let mut c = Canvas::new(ms.len() * TILE as usize, (bodies.len() as i32 * TH + head) as usize);
    let title = if before {
        "BEFORE - PLAYER BODY MOMENTS AS DRAWN BEFORE THE ANIMATION PASS (SAME LAYER STACK AS PRESENT/PLAYERS.RS)"
    } else {
        "AFTER - PLAYER BODY MOMENTS WITH THE MOTION / ACTION / CHANNEL LAYERS (PRESENT/BODY_MOTION.RS)"
    };
    text(&mut c, 8, 8, title, [215, 225, 235], 1);
    for (r, d) in bodies.iter().enumerate() {
        for (k, (label, b, a)) in ms.iter().enumerate() {
            let l = if before { b } else { a };
            let tint = if before { [200, 170, 150] } else { [160, 230, 170] };
            tile(&mut c, d, l, EYE, k as i32 * TILE, head + r as i32 * TH, label, tint);
        }
    }
    let out = val("--out").map(PathBuf::from).unwrap_or(dir.join(if before { "body_before.png" } else { "body_after.png" }));
    c.save(&out);
    println!("wrote {}", out.display());
}
