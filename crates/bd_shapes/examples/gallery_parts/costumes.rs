//! `--costumes`: 24 phrases resolved by the real lexicon and dressed on the humanoid rig (two views
//! each, "turntable"), plus the player animation set on two costumes.

use super::*;

const PHRASES: [&str; 24] = [
    "knight with a fox head", "robot pirate", "shark", "toaster", "eeyore", "goku", "dragon", "astronaut", "cactus", "pikachu", "samurai", "wizard",
    "steampunk", "ninja", "octopus", "cowboy", "viking", "bee", "skeleton", "knight", "pirate", "dinosaur", "mushroom", "tv head",
];

struct Dressed {
    label: String,
    model: VoxelModel,
    meshes: Vec<PartMesh>,
    vol_ratio: f32,
}

fn resolve(lex: Option<&bd_lexicon::Lexicon>, text: &str) -> (Recipe, Option<ShapeSpec>) {
    match lex {
        Some(l) => {
            let r = l.resolve(text);
            (r.recipe, r.shape)
        }
        None => {
            let mut r = Recipe::unknown(text);
            r.form = Form::Biped;
            (r, None)
        }
    }
}

fn dress(lex: Option<&bd_lexicon::Lexicon>, phrase: &str, seed: u32) -> Dressed {
    // "X with a Y head" → outfit X, head donor Y (the game's costume UI does the same split).
    let (body, head) = match phrase.strip_suffix(" head").and_then(|p| p.split_once(" with a ")) {
        Some((x, y)) => (x.to_string(), Some(y.to_string())),
        None => (phrase.to_string(), None),
    };
    let (rec, shape) = resolve(lex, &body);
    let donor = head.map(|h| resolve(lex, &h));
    let spec = costume_with(&rec, shape.as_ref(), donor.as_ref().map(|(r, s)| CostumeHead { recipe: r, shape: s.as_ref() }), seed, CostumeTweaks::default());
    let model = costume_model(&spec);
    let meshes = mesh_model(&model);
    let b = spec_bounds(&spec);
    let s = (b.max - b.min + Vec3::ONE) * COSTUME_UNIT;
    let vol_ratio = s.x * s.y * s.z / (0.8 * 1.8 * 0.8);
    println!(
        "{:26} {:?}/{:?} feats {:?} -> voxels {:5} parts {:2} gait {:?} bbox {:.2}x{:.2}x{:.2} m vol {:.2}x hull",
        phrase, rec.form, rec.mechanics, rec.features, model.voxel_count, model.parts.len(), model.rig.gait, s.x, s.y, s.z, vol_ratio
    );
    Dressed { label: phrase.to_uppercase(), model, meshes, vol_ratio }
}

#[allow(clippy::too_many_arguments)]
fn draw_view(c: &mut Canvas, d: &Dressed, pose: &[Affine3A], eye: Vec3, x0: i32, y0: i32, w: i32, h: i32, fit_pts: Option<&[Vec3]>) {
    let cam = Cam::from_eye(eye);
    let it = Item { model: &d.model, meshes: &d.meshes, pose: pose.to_vec(), root: Affine3A::IDENTITY, pal: GREEN };
    let pts = match fit_pts {
        Some(p) => p.to_vec(),
        None => {
            let mut p = world_points(&it);
            p.push(Vec3::ZERO);
            p.push(Vec3::new(0.0, 1.9, 0.0));
            p
        }
    };
    let fit = fit_points(&cam, &pts, x0 as f32, y0 as f32, w as f32, h as f32, 6.0);
    c.clear_depth();
    let gy = fit.oy.round() as i32;
    for x in x0 + 2..x0 + w - 2 {
        c.put(x, gy, [48, 56, 60]);
    }
    draw(c, &cam, fit, &it, (x0 + 1, y0 + 1, x0 + w - 1, y0 + h - 1));
}

pub fn run(docs: &std::path::Path, lex: Option<&bd_lexicon::Lexicon>, only: Option<String>) {
    let list: Vec<&str> = PHRASES.iter().copied().filter(|p| only.as_deref().is_none_or(|o| o.split(',').any(|w| p.contains(w)))).collect();
    let dressed: Vec<Dressed> = list.iter().map(|p| dress(lex, p, 0)).collect();
    let (vw, th) = (170i32, 230i32);
    let cols = 6usize;
    let tw = vw * 2;
    let rows = dressed.len().div_ceil(cols);
    let head = 22;
    let mut c = Canvas::new(cols * tw as usize, rows * th as usize + head);
    text(&mut c, 6, 7, "COSTUMES - LEXICON PHRASE -> HUMANOID RIG (FRONT 3/4 + BACK 3/4)   1/16 M VOXELS, GUARDRAIL <= 1.2X HULL", [200, 220, 200]);
    for (k, d) in dressed.iter().enumerate() {
        let x0 = (k % cols) as i32 * tw;
        let y0 = (k / cols) as i32 * th + head as i32;
        c.rect(x0 + 1, y0 + 1, tw - 2, th - 2, [22, 25, 32]);
        c.frame(x0, y0, tw, th, [40, 44, 54]);
        let mut pose_v = vec![Affine3A::IDENTITY; d.model.parts.len()];
        pose(&d.model, AnimState::Idle, 0.6, &mut pose_v);
        draw_view(&mut c, d, &pose_v, Vec3::new(0.55, 0.28, -1.0), x0, y0 + 12, vw, th - 26, None);
        draw_view(&mut c, d, &pose_v, Vec3::new(-0.8, 0.3, 0.9), x0 + vw, y0 + 12, vw, th - 26, None);
        text(&mut c, x0 + 4, y0 + 4, &d.label, [220, 235, 220]);
        text(&mut c, x0 + 4, y0 + th - 11, &format!("{}V {}P VOL {:.2}X", d.model.voxel_count, d.model.parts.len(), d.vol_ratio), [110, 130, 120]);
    }
    // Player animation set on two costumes.
    let anims: Vec<(&str, AnimState, f32)> = vec![
        ("IDLE", AnimState::Idle, 0.9),
        ("RUN 7M/S", AnimState::Locomotion { speed: 7.0, distance: 1.3, airborne: false }, 0.4),
        ("RUN 7M/S B", AnimState::Locomotion { speed: 7.0, distance: 1.9, airborne: false }, 0.5),
        ("AIR", AnimState::Locomotion { speed: 7.0, distance: 0.0, airborne: true }, 0.3),
        ("SLIDE", AnimState::Slide { speed: 9.0 }, 0.3),
        ("TETHER FIRE", AnimState::TetherFire { side: 1 }, 0.12),
        ("SWING", AnimState::Swing { side: -1, lean: 0.4 }, 0.8),
        ("CAST ANT", AnimState::Cast, CAST_TIMING.anticipation),
        ("CAST THRUST", AnimState::Cast, CAST_TIMING.anticipation + CAST_TIMING.active + 0.02),
        ("HIT REACT", AnimState::HitReact { dir: Vec3::new(1.0, 0.0, 0.3), strength: 1.0 }, 0.06),
        ("TUMBLE", AnimState::Tumble { spin: 9.0 }, 0.25),
    ];
    let pick: Vec<&Dressed> = dressed.iter().filter(|d| d.label.contains("FOX") || d.label == "DRAGON" || d.label == "WIZARD").take(3).collect();
    let mut strips = vec![c];
    for d in pick {
        let fw = 150i32;
        let mut s = Canvas::new(anims.len() * fw as usize, 200);
        text(&mut s, 4, 3, &format!("PLAYER ANIMS: {}", d.label), [220, 235, 220]);
        // Common fit so poses share a scale.
        let mut all = Vec::new();
        let mut poses = Vec::new();
        for (_, a, t) in &anims {
            let mut p = vec![Affine3A::IDENTITY; d.model.parts.len()];
            pose(&d.model, *a, *t, &mut p);
            let it = Item { model: &d.model, meshes: &d.meshes, pose: p.clone(), root: Affine3A::IDENTITY, pal: GREEN };
            all.extend(world_points(&it));
            poses.push(p);
        }
        all.push(Vec3::ZERO);
        for (i, ((label, _, _), p)) in anims.iter().zip(poses).enumerate() {
            let x0 = i as i32 * fw;
            s.rect(x0 + 1, 15, fw - 2, 183, [22, 25, 32]);
            s.frame(x0, 14, fw, 185, [40, 44, 54]);
            let cam = Cam::from_eye(Vec3::new(1.0, 0.25, -0.55));
            let f0 = fit_points(&cam, &all, 0.0, 0.0, fw as f32, 170.0, 8.0);
            let fit = Fit { scale: f0.scale, ox: f0.ox + x0 as f32, oy: f0.oy + 16.0 };
            s.clear_depth();
            let it = Item { model: &d.model, meshes: &d.meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN };
            let gy = fit.oy.round() as i32;
            for x in x0 + 2..x0 + fw - 2 {
                s.put(x, gy, [48, 56, 60]);
            }
            draw(&mut s, &cam, fit, &it, (x0 + 1, 15, x0 + fw - 1, 198));
            text(&mut s, x0 + 4, 187, label, [140, 160, 150]);
        }
        strips.push(s);
    }
    let out = if only.is_some() { docs.join("gallery_costumes_only.png") } else { docs.join("gallery_costumes.png") };
    stack(strips).save(&out);
}
