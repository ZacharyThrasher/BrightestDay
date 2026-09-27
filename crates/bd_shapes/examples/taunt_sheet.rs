//! TAUNT & VOICE TELL frame sheet, software-rasterised (no GPU).
//!
//! cargo run -p bd_shapes --example taunt_sheet --release -- [--names naruto,knight] [--out PATH]
//!
//! Each wordless taunt over time as the OBSERVER sees it (front three-quarter: how everyone else reads
//! you) and from the taunter's own THIRD-PERSON camera (behind the shoulder), then the two casting
//! tells side by side: TYPING (ring fist to the face, head bowed, ring motes) vs VOICE (the same body
//! language + corps sound-wave rings rolling off the mouth) → docs/animation/taunt_voice_sheet.png.
//! Poses are the game's own (`bd_shapes::anim_body` `Channel` layer on the same base / motion stack
//! `present::body_motion` draws); the motes / rings are drawn where `present::body_motion` /
//! `present::taunt` emit them.

use std::path::PathBuf;

use bd_core::mechanics::{Form, Recipe};
use bd_shapes::anim_body::taunt_duration;
use bd_shapes::costume::{self, look};
use bd_shapes::*;
use glam::{Affine3A, Vec3};

#[path = "raster/mod.rs"]
mod raster;
use raster::{Cam, Canvas, Dressed, draw_posed, text, text_w};

type Layers = Vec<(AnimState, f32)>;

const TILE: i32 = 128;
const TH: i32 = 196;
const SCALE: f32 = 58.0;
const GROUND: f32 = 18.0;
/// Observer: front three-quarter. Own 3P camera: behind, over the right shoulder.
const OBSERVER: Vec3 = Vec3::new(0.7, 0.22, -0.85);
const OWN_3P: Vec3 = Vec3::new(0.38, 0.4, 1.0);
/// Corps light (the default palette's green lantern) for the drawn motes / rings.
const CORPS: [u8; 3] = [90, 240, 130];

fn base() -> Layers {
    vec![
        (AnimState::Locomotion { speed: 0.0, distance: 0.0, airborne: false }, 0.37),
        (AnimState::Motion(BodyMotion::default()), 0.37),
    ]
}

fn with(kind: Channel, t: f32) -> Layers {
    let mut l = base();
    l.push((AnimState::Channel { kind, w: 1.0 }, t));
    l
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

/// Screen position of a model-space point in a tile drawn by `draw_posed`.
fn screen(eye: Vec3, x0: i32, y0: i32, p: Vec3) -> (i32, i32) {
    let cam = Cam::from_eye(eye);
    let (x, y, _) = cam.proj(p);
    let ox = x0 as f32 + TILE as f32 * 0.5;
    let oy = (y0 + 14 + TH - 14) as f32 - GROUND;
    ((ox + x * SCALE).round() as i32, (oy - y * SCALE).round() as i32)
}

/// Centre of the highest voxels of the parts of `kind` (head / ring fist) in model space.
fn part_top(d: &Dressed, p: &[Affine3A], want: impl Fn(PartKind) -> bool, lowest: bool) -> Option<Vec3> {
    let mut best: Option<Vec3> = None;
    for (i, part) in d.model.parts.iter().enumerate() {
        if !want(d.model.rig.kinds[i]) {
            continue;
        }
        for (v, _) in &part.voxels {
            let q = p[i].transform_point3(part.voxel_center(*v));
            let better = match best {
                None => true,
                Some(b) => if lowest { q.y < b.y } else { q.y > b.y },
            };
            if better {
                best = Some(q);
            }
        }
    }
    best
}

fn dot(c: &mut Canvas, (x, y): (i32, i32), col: [u8; 3], r: i32) {
    c.rect(x - r / 2, y - r / 2, r.max(1), r.max(1), col);
}

/// Ring motes curling up off the ring fist (typing / voice / oath: `body_motion`).
fn motes(c: &mut Canvas, d: &Dressed, p: &[Affine3A], eye: Vec3, x0: i32, y0: i32) {
    // The ring fist: the lowest voxel of the raised right arm is its end.
    let Some(fist) = part_top(d, p, |k| k == PartKind::Arm(1), false) else { return };
    for k in 0..7 {
        let a = k as f32 * 2.399;
        let q = fist + Vec3::new(a.cos() * 0.1, 0.05 + 0.09 * k as f32, a.sin() * 0.1);
        dot(c, screen(eye, x0, y0, q), CORPS, 3);
    }
}

/// Voice: sound-wave rings rolling forward off the mouth (`present::taunt::tell_fx`).
fn rings(c: &mut Canvas, d: &Dressed, p: &[Affine3A], eye: Vec3, x0: i32, y0: i32) {
    let Some(head) = part_top(d, p, |k| k == PartKind::Head, false) else { return };
    let mouth = head + Vec3::new(0.0, -0.28, -0.22);
    for (k, (dist, r)) in [(0.15f32, 0.1f32), (0.45, 0.24), (0.8, 0.38)].iter().enumerate() {
        let fade = [255u8, 200, 140][k];
        let col = [(CORPS[0] as u32 * fade as u32 / 255) as u8, (CORPS[1] as u32 * fade as u32 / 255) as u8, (CORPS[2] as u32 * fade as u32 / 255) as u8];
        let n = 22;
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let q = mouth + Vec3::new(a.cos() * r, a.sin() * r, -dist);
            dot(c, screen(eye, x0, y0, q), col, 2);
        }
    }
}

fn tile(c: &mut Canvas, d: &Dressed, layers: &[(AnimState, f32)], eye: Vec3, x0: i32, y0: i32, label: &str, tint: [u8; 3]) -> Vec<Affine3A> {
    c.rect(x0 + 1, y0 + 1, TILE - 2, TH - 2, [24, 27, 35]);
    c.rect(x0 + 8, y0 + TH - 1 - GROUND as i32, TILE - 16, 1, [60, 66, 80]);
    let p = posed(d, layers);
    draw_posed(c, d, &p, eye, x0, y0 + 14, TILE, TH - 14, SCALE, GROUND);
    let lw = text_w(label, 1);
    text(c, x0 + (TILE - lw) / 2, y0 + 4, label, tint, 1);
    p
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let val = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let root = std::env::var("BD_ROOT").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let names: Vec<String> = val("--names").map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_else(|| vec!["naruto".into(), "knight".into()]);
    let bodies: Vec<Dressed> = names.iter().map(|n| dressed(n)).collect();
    if args.iter().any(|a| a == "--probe") {
        // Highest point of each arm and the lowest voxel per taunt over time (m, model space).
        for d in &bodies {
            for kind in [Channel::Flex, Channel::Beckon, Channel::Shrug, Channel::Bow, Channel::Dance] {
                let dur = if kind == Channel::Dance { 1.8 } else { taunt_duration(kind) };
                for k in 0..=8 {
                    let t = dur * k as f32 / 8.0;
                    let p = posed(d, &with(kind, t));
                    let l = part_top(d, &p, |x| x == PartKind::Arm(-1), false).map_or(0.0, |v| v.y);
                    let r = part_top(d, &p, |x| x == PartKind::Arm(1), false).map_or(0.0, |v| v.y);
                    let lo = part_top(d, &p, |_| true, true).map_or(0.0, |v| v.y);
                    println!("{} {kind:?} t {t:.2}: arm L {l:.2} R {r:.2} low {lo:+.2}", d.label);
                }
            }
        }
        return;
    }

    let taunts = [
        ("FLEX (TAP 1)", Channel::Flex),
        ("BECKON (TAP 2)", Channel::Beckon),
        ("SHRUG (TAP 3)", Channel::Shrug),
        ("BOW (TAP 4)", Channel::Bow),
        ("DANCE (HOLD)", Channel::Dance),
    ];
    // Observer frames (fractions of the taunt) + the taunter's own 3P view at its peak.
    let obs_u = [0.12f32, 0.32, 0.5, 0.66, 0.9];
    let own_u = [0.32f32, 0.6];
    let label_w = 132;
    let cols = obs_u.len() + own_u.len();
    let head = 28;
    let rows_per_body = taunts.len() + 1;
    let w = label_w + cols as i32 * TILE + 12;
    let h = head + (rows_per_body * bodies.len()) as i32 * TH + 16;
    let mut c = Canvas::new(w as usize, h as usize);
    text(&mut c, 8, 8, "TAUNT & VOICE TELL - WORDLESS TAUNTS (OBSERVER VIEW / OWN 3P VIEW) AND VOICE VS TYPING CASTING TELLS", [215, 225, 235], 1);
    let obs_x = label_w;
    let own_x = label_w + obs_u.len() as i32 * TILE + 6;
    text(&mut c, obs_x + 4, head - 10, "WHAT EVERYONE ELSE SEES (FRONT 3/4)", [150, 200, 230], 1);
    text(&mut c, own_x + 4, head - 10, "YOUR OWN 3P CAMERA", [230, 200, 150], 1);
    let mut row = 0;
    for d in &bodies {
        for (label, kind) in taunts {
            let y0 = head + row * TH;
            let dur = if kind == Channel::Dance { 1.8 } else { taunt_duration(kind) };
            text(&mut c, 6, y0 + TH / 2 - 10, label, [235, 235, 240], 1);
            text(&mut c, 6, y0 + TH / 2 + 2, &d.label, [120, 130, 145], 1);
            text(&mut c, 6, y0 + TH / 2 + 14, &format!("{:.2} S", dur), [120, 130, 145], 1);
            for (k, u) in obs_u.iter().enumerate() {
                let t = dur * u;
                tile(&mut c, d, &with(kind, t), OBSERVER, obs_x + k as i32 * TILE, y0, &format!("{:.0} MS", t * 1000.0), [150, 200, 230]);
            }
            for (k, u) in own_u.iter().enumerate() {
                let t = dur * u;
                tile(&mut c, d, &with(kind, t), OWN_3P, own_x + k as i32 * TILE, y0, &format!("{:.0} MS", t * 1000.0), [230, 200, 150]);
            }
            row += 1;
        }
        // Voice vs typing: the same body language (voice == typing), voice adds the rings.
        let y0 = head + row * TH;
        text(&mut c, 6, y0 + TH / 2 - 16, "CASTING TELLS", [235, 235, 240], 1);
        text(&mut c, 6, y0 + TH / 2 - 4, "TYPING | VOICE", [120, 130, 145], 1);
        text(&mut c, 6, y0 + TH / 2 + 8, &d.label, [120, 130, 145], 1);
        let ty = with(Channel::Typing, 0.6);
        let p = tile(&mut c, d, &ty, OBSERVER, obs_x, y0, "TYPING", [200, 210, 220]);
        motes(&mut c, d, &p, OBSERVER, obs_x, y0);
        let p = tile(&mut c, d, &ty, OBSERVER, obs_x + TILE, y0, "VOICE", [140, 240, 170]);
        motes(&mut c, d, &p, OBSERVER, obs_x + TILE, y0);
        rings(&mut c, d, &p, OBSERVER, obs_x + TILE, y0);
        // Voice mid-sentence from a second angle (the rings keep rolling forward off the mouth).
        let rear = Vec3::new(-0.9, 0.3, -0.5);
        let p = tile(&mut c, d, &ty, rear, obs_x + 2 * TILE, y0, "TYPING (L)", [200, 210, 220]);
        motes(&mut c, d, &p, rear, obs_x + 2 * TILE, y0);
        let p = tile(&mut c, d, &ty, rear, obs_x + 3 * TILE, y0, "VOICE (L)", [140, 240, 170]);
        motes(&mut c, d, &p, rear, obs_x + 3 * TILE, y0);
        rings(&mut c, d, &p, rear, obs_x + 3 * TILE, y0);
        let p = tile(&mut c, d, &ty, OWN_3P, own_x, y0, "TYPING 3P", [230, 200, 150]);
        motes(&mut c, d, &p, OWN_3P, own_x, y0);
        let p = tile(&mut c, d, &ty, OWN_3P, own_x + TILE, y0, "VOICE 3P", [140, 240, 170]);
        motes(&mut c, d, &p, OWN_3P, own_x + TILE, y0);
        rings(&mut c, d, &p, OWN_3P, own_x + TILE, y0);
        row += 1;
    }
    let out = val("--out").map(PathBuf::from).unwrap_or(root.join("docs/animation/taunt_voice_sheet.png"));
    c.save(&out);
}
