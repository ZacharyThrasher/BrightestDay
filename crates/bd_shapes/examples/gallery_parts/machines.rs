//! `--machines` (machine-kit test cases + animation strips) and `--features` (every feature on a
//! plain body) contact sheets.

use super::*;
use bd_core::mechanics::Feature;

pub fn lexicon(root: &std::path::Path) -> Option<bd_lexicon::Lexicon> {
    match bd_lexicon::Lexicon::load(root.join("assets/lexicon.bin")) {
        Ok(l) => Some(l),
        Err(e) => {
            println!("lexicon unavailable ({e:?}); using hand recipes");
            None
        }
    }
}

/// Resolve a phrase with the real lexicon (falls back to `fb`).
pub fn phrase(lex: Option<&bd_lexicon::Lexicon>, text: &str, fb: impl FnOnce() -> Recipe) -> Recipe {
    match lex {
        Some(l) => {
            let r = l.resolve(text);
            println!("{text:>30} => {:?} {:?} {:?} scale {}", r.recipe.form, r.recipe.mechanics, r.recipe.features, r.recipe.scale);
            r.recipe
        }
        None => fb(),
    }
}

fn hand(form: Form, scale: f32, mech: &[Mechanic], feats: &[Feature], seed: u32) -> Recipe {
    let mut r = demo_recipe(form, seed, mech);
    r.scale = scale;
    r.features = feats.to_vec();
    r.normalized()
}

fn build_r(label: &str, r: &Recipe) -> Built {
    build(label.to_string(), r)
}

/// Sheet with a per-item pose (Locomotion at a small distance so linkages are off their rest pose).
fn posed_sheet(items: &[(Built, Vec3)], tile: usize, cols: usize, title: &str) -> Canvas {
    let rows = items.len().div_ceil(cols);
    let head = 22;
    let mut c = Canvas::new(cols * tile, rows * tile + head);
    text(&mut c, 6, 7, title, [200, 220, 200]);
    for (k, (b, eye)) in items.iter().enumerate() {
        let x0 = (k % cols * tile) as i32;
        let y0 = (k / cols * tile + head) as i32;
        c.rect(x0 + 1, y0 + 1, tile as i32 - 2, tile as i32 - 2, [22, 25, 32]);
        c.frame(x0, y0, tile as i32, tile as i32, [40, 44, 54]);
        let cam = Cam::from_eye(*eye);
        let mut pose_v = vec![Affine3A::IDENTITY; b.model.parts.len()];
        pose_layered(&b.model, &[(AnimState::Locomotion { speed: 0.0, distance: 0.0, airborne: false }, 0.0), (AnimState::Aim { yaw: 0.35, pitch: 0.12 }, 0.0)], &mut pose_v);
        let it = Item { model: &b.model, meshes: &b.meshes, pose: pose_v, root: Affine3A::IDENTITY, pal: GREEN };
        let pts = world_points(&it);
        let fit = fit_points(&cam, &pts, x0 as f32, y0 as f32 + 12.0, tile as f32, tile as f32 - 26.0, 6.0);
        c.clear_depth();
        draw(&mut c, &cam, fit, &it, (x0 + 1, y0 + 1, x0 + tile as i32 - 1, y0 + tile as i32 - 1));
        text(&mut c, x0 + 4, y0 + 4, &b.label, [220, 235, 220]);
        let moving = b.model.machine.motions.iter().filter(|m| **m != Motion::None).count();
        let shared = b.model.instances.iter().enumerate().filter(|(i, s)| s.source != *i).count();
        let info = format!("{}V {}P {}M {}I", b.model.voxel_count, b.model.parts.len(), moving, shared);
        text(&mut c, x0 + 4, y0 + tile as i32 - 11, &info, [110, 130, 120]);
    }
    c
}

/// A screen-space smoke cube (depth tested at its centre).
fn smoke(c: &mut Canvas, cam: &Cam, fit: Fit, p: Vec3, size: f32, shade: f32, clip: (i32, i32, i32, i32)) {
    let (x, y, z) = cam.proj(p);
    let sx = fit.ox + x * fit.scale;
    let sy = fit.oy - y * fit.scale;
    let h = (size * fit.scale * 0.5).max(1.5);
    let col = [(150.0 * shade) as u8, (158.0 * shade) as u8, (165.0 * shade) as u8];
    for yy in (sy - h) as i32..=(sy + h) as i32 {
        for xx in (sx - h) as i32..=(sx + h) as i32 {
            if xx < clip.0 || yy < clip.1 || xx >= clip.2 || yy >= clip.3 {
                continue;
            }
            let idx = yy as usize * c.w + xx as usize;
            if z < c.z[idx] {
                let edge = (xx as f32 - (sx - h)).min((sx + h) - xx as f32).min(yy as f32 - (sy - h)).min((sy + h) - yy as f32) < 1.0;
                c.px[idx] = if edge { [(col[0] as f32 * 0.7) as u8, (col[1] as f32 * 0.7) as u8, (col[2] as f32 * 0.7) as u8] } else { col };
            }
        }
    }
}

type DriveFn = fn(f32) -> (f32, f32);

/// 8-frame strip of a machine: `drive(t) -> (speed, distance)`, frames at `t = dur·f/8`; smoke puffs
/// from `machine_puffs` are simulated and drawn.
#[allow(clippy::too_many_arguments)]
fn strip(label: &str, b: &Built, eye: Vec3, tile: usize, dur: f32, drive: DriveFn, aim: Option<fn(f32) -> (f32, f32)>) -> Canvas {
    strip_fit(label, b, eye, tile, dur, drive, aim, None)
}

#[allow(clippy::too_many_arguments)]
fn strip_fit(label: &str, b: &Built, eye: Vec3, tile: usize, dur: f32, drive: DriveFn, aim: Option<fn(f32) -> (f32, f32)>, keep: Option<fn(Vec3) -> bool>) -> Canvas {
    let frames = 8usize;
    let head = 14;
    let mut c = Canvas::new(frames * tile, tile + head);
    text(&mut c, 4, 3, &format!("{}  ({:.2}S)", label, dur), [220, 235, 220]);
    let cam = Cam::from_eye(eye);
    let n = b.model.parts.len();
    let pose_at = |t: f32| {
        let (speed, dist) = drive(t);
        let mut p = vec![Affine3A::IDENTITY; n];
        let mut layers = vec![(AnimState::Locomotion { speed, distance: dist, airborne: false }, t)];
        if let Some(a) = aim {
            let (yaw, pitch) = a(t);
            layers.push((AnimState::Aim { yaw, pitch }, t));
        }
        pose_layered(&b.model, &layers, &mut p);
        p
    };
    let mut pts = Vec::new();
    let mut poses = Vec::new();
    for f in 0..frames {
        let t = dur * f as f32 / frames as f32;
        let p = pose_at(t);
        pts.extend(world_points(&Item { model: &b.model, meshes: &b.meshes, pose: p.clone(), root: Affine3A::IDENTITY, pal: GREEN }));
        poses.push((t, p));
    }
    pts.push(Vec3::ZERO);
    if let Some(k) = keep {
        pts.retain(|p| k(*p));
    }
    let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32, 10.0);
    // Puff history: (birth time, pos, dir, size, strength).
    let mut puffs: Vec<(f32, Puff)> = Vec::new();
    let dt = 1.0 / 60.0;
    let mut tt = -1.5f32;
    let mut prev = {
        let (s, d) = drive(tt);
        FeatureDrive { time: tt, speed: s, distance: d }
    };
    for (f, (t, p)) in poses.into_iter().enumerate() {
        while tt < t {
            tt += dt;
            let (s, d) = drive(tt);
            let now = FeatureDrive { time: tt, speed: s, distance: d };
            let mut out = Vec::new();
            let pp = pose_at(tt);
            machine_puffs(&b.model, &pp, &prev, &now, &mut out);
            puffs.extend(out.into_iter().map(|x| (tt, x)));
            prev = now;
        }
        let x0 = (f * tile) as i32;
        let ty = head as i32;
        c.rect(x0 + 1, ty + 1, tile as i32 - 2, tile as i32 - 2, [22, 25, 32]);
        c.frame(x0, ty, tile as i32, tile as i32, [40, 44, 54]);
        let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + ty as f32 };
        let gy = fit.oy.round() as i32;
        for x in x0 + 2..x0 + tile as i32 - 2 {
            c.put(x, gy, [48, 56, 60]);
        }
        c.clear_depth();
        let clip = (x0 + 1, ty + 1, x0 + tile as i32 - 1, ty + tile as i32 - 1);
        draw(&mut c, &cam, fit, &Item { model: &b.model, meshes: &b.meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN }, clip);
        for (born, pf) in &puffs {
            let age = t - born;
            if !(0.0..1.2).contains(&age) {
                continue;
            }
            // World drift: launch along dir, rise, drift back as the machine moves on.
            let (speed, _) = drive(t);
            let pos = pf.pos + pf.dir * (age * 2.5 * pf.strength) + Vec3::Y * (age * 1.2) + Vec3::Z * (age * speed * 0.9);
            smoke(&mut c, &cam, fit, pos, pf.size * (1.0 + age * 1.6), 1.0 - age * 0.5, clip);
        }
        text(&mut c, x0 + 4, ty + tile as i32 - 11, &format!("{:.2}", t), [140, 160, 150]);
    }
    c
}

fn roll_radius(m: &VoxelModel) -> f32 {
    // Largest rolling wheel (the drivers).
    m.machine
        .motions
        .iter()
        .filter_map(|x| if let Motion::Spin { roll_k, .. } = x { (*roll_k > 0.0).then_some(1.0 / roll_k) } else { None })
        .fold(0.3, f32::max)
}

pub fn run(docs: &std::path::Path, tile: usize, lex: Option<&bd_lexicon::Lexicon>, machines: bool, features: bool, only: Option<String>) {
    use Feature::*;
    use Mechanic::*;
    if machines {
        let train = phrase(lex, "steam powered train", || hand(Form::Vehicle, 12.0, &[Charge, Fast, Heavy], &[Boiler, Cab, Pistons, Plow, Smokestack, Wheels], 7));
        let heli = phrase(lex, "helicopter", || hand(Form::Vehicle, 4.0, &[Shoots, Flies, Spin], &[Rotor, Cockpit, Windows], 3));
        let tank = phrase(lex, "tank", || hand(Form::Vehicle, 4.0, &[Shoots, Charge, Heavy], &[Treads, Turret, Barrels, Armor], 5));
        let ship = phrase(lex, "pirate ship", || hand(Form::Vehicle, 40.0, &[Charge], &[Sails, Mast, Barrels], 2));
        let dragon = phrase(lex, "clockwork dragon", || hand(Form::Quadruped, 6.0, &[ArticulatedTail, Flies, Breath], &[Gears], 4));
        let drill_tank = phrase(lex, "drill tank", || hand(Form::Vehicle, 4.0, &[Pierce, Shoots, Charge, Spin, Heavy], &[Treads, Turret, Barrels, Drill, Armor], 5));
        let mech_k = phrase(lex, "steampunk mech", || hand(Form::Kaiju, 8.0, &[Shoots, Stomp, Heavy], &[Pistons, Gears, Boiler, Smokestack, Pipes, Cockpit, Armor], 6));
        let mech_b = hand(Form::Biped, 6.0, &[Piloted], &[Boiler, Pipes, Gears, Smokestack, Pistons], 11);
        let bird_gears = hand(Form::Bird, 3.0, &[Flies], &[Gears], 2);
        let serpent_gears = hand(Form::Serpent, 8.0, &[Flies], &[Gears], 3);
        let jet = phrase(lex, "fighter jet", || hand(Form::Vehicle, 8.0, &[Flies, Fast], &[Jet, Fins, Cockpit], 2));
        let crane = hand(Form::Vehicle, 7.0, &[], &[CraneArm, Wheels, Headlight], 1);
        let monster = hand(Form::Vehicle, 5.0, &[Heavy, Charge], &[Wheels, Exhaust, Saw, Headlight], 2);
        let submarine = phrase(lex, "submarine", || hand(Form::Vehicle, 8.0, &[], &[Propeller, Fins, Windows], 2));
        let radar = hand(Form::Pillar, 6.0, &[], &[Dish, Antenna, Bell, Chains], 1);
        let steamball = hand(Form::Boulder, 3.0, &[], &[Wheels, Boiler, Smokestack, Pistons, Pipes], 1);
        let eye_side = Vec3::new(1.0, 0.35, -0.55);
        let items: Vec<(Built, Vec3)> = vec![
            (build_r("STEAM POWERED TRAIN", &train), Vec3::new(1.0, 0.3, -0.45)),
            (build_r("HELICOPTER", &heli), eye_side),
            (build_r("TANK", &tank), eye_side),
            (build_r("PIRATE SHIP", &ship), Vec3::new(1.0, 0.35, -0.7)),
            (build_r("CLOCKWORK DRAGON", &dragon), Vec3::new(1.0, 0.45, -0.6)),
            (build_r("DRILL TANK", &drill_tank), eye_side),
            (build_r("STEAMPUNK MECH (LEXICON)", &mech_k), Vec3::new(0.8, 0.3, 1.0)),
            (build_r("STEAMPUNK MECH (PILOTED)", &mech_b), Vec3::new(0.9, 0.3, 1.0)),
            (build_r("CLOCKWORK BIRD", &bird_gears), eye_side),
            (build_r("CLOCKWORK SERPENT", &serpent_gears), Vec3::new(0.6, 1.0, -0.6)),
            (build_r("FIGHTER JET", &jet), Vec3::new(1.0, 0.6, -0.6)),
            (build_r("CRANE TRUCK", &crane), eye_side),
            (build_r("MONSTER SAW TRUCK", &monster), eye_side),
            (build_r("SUBMARINE", &submarine), eye_side),
            (build_r("RADAR TOWER", &radar), eye_side),
            (build_r("STEAM BOULDER", &steamball), eye_side),
        ];
        if only.as_deref() == Some("train") {
            let trainb = &items[0].0;
            let rev = std::f32::consts::TAU * roll_radius(&trainb.model);
            let d = rev / 6.0;
            let a = strip("TRAIN SIDE", trainb, Vec3::new(1.0, 0.08, 0.0), 300, d, |t| (6.0, 6.0 * t), None);
            let b2 = strip_fit("TRAIN CLOSE", trainb, Vec3::new(1.0, 0.1, 0.05), 300, d, |t| (6.0, 6.0 * t), None, Some(|p: Vec3| p.z < 0.3 && p.y < 3.0));
            let c3 = strip("TRAIN 3/4", trainb, Vec3::new(1.0, 0.35, -0.8), 300, d, |t| (6.0, 6.0 * t), None);
            stack(vec![a, b2, c3]).save(&docs.join("gallery_machines_only.png"));
            return;
        }
        if let Some(o) = &only {
            let sel: Vec<(Built, Vec3)> = items.into_iter().filter(|(b, _)| o.split(',').any(|w| b.label.to_lowercase().contains(w))).collect();
            posed_sheet(&sel, tile, 4, "MACHINES (FILTERED)").save(&docs.join("gallery_machines_only.png"));
            return;
        }
        for (b, _) in &items {
            let moving = b.model.machine.motions.iter().filter(|m| **m != Motion::None).count();
            println!("{:28} voxels {:5} parts {:3} moving {:3} vsize {:.2} puffs {}", b.label, b.model.voxel_count, b.model.parts.len(), moving, b.model.voxel_size, b.model.machine.puffs.len());
        }
        let sheet = posed_sheet(&items, tile.max(220), 4, "MACHINE KIT - LEXICON PHRASES + HAND RECIPES   (V VOXELS, P PARTS, M MOVING, I SHARED MESHES)");
        let st = tile.max(220);
        let trainb = &items[0].0;
        for (i, m) in trainb.model.machine.motions.iter().enumerate() {
            if let Motion::Spin { roll_k, .. } = m {
                println!("part {i} id {} role {:?} pivot {:?} R {:.3} vox {}", trainb.model.parts[i].id, trainb.model.parts[i].role, trainb.model.parts[i].pivot, 1.0 / roll_k, trainb.model.parts[i].voxels.len());
            }
        }
        let rev = std::f32::consts::TAU * roll_radius(&trainb.model);
        let speed = 6.0;
        // One driver revolution at 6 m/s.
        let train_dur = rev / speed;
        let s_train = strip(&format!("STEAM TRAIN RUNNING 6 M/S - ONE DRIVER REVOLUTION ({:.2} M)", rev), trainb, Vec3::new(1.0, 0.08, 0.0), st, train_dur, |t| (6.0, 6.0 * t), None);
        let s_train_q = strip("STEAM TRAIN 3/4 VIEW", trainb, Vec3::new(1.0, 0.35, -0.8), st, train_dur, |t| (6.0, 6.0 * t), None);
        let s_train_c = strip_fit("STEAM TRAIN CLOSE-UP: CRANK-SLIDER (RIGHT SIDE LEADS LEFT BY 90 DEG)", trainb, Vec3::new(1.0, 0.1, 0.05), st, train_dur, |t| (6.0, 6.0 * t), None, Some(|p: Vec3| p.z < 0.3 && p.y < 3.0));
        let s_heli = strip("HELICOPTER ROTORS (IDLE)", &items[1].0, Vec3::new(1.0, 0.6, -0.5), st, 0.24, |_| (0.0, 0.0), None);
        let s_tank = strip("TANK: TREADS 3 M/S + TURRET TRACKING AIM", &items[2].0, Vec3::new(1.0, 0.45, -0.6), st, 1.6, |t| (3.0, 3.0 * t), Some(|t| ((t * 2.2).sin() * 1.2, 0.15 + 0.1 * (t * 3.0).sin())));
        let s_drag = strip("CLOCKWORK DRAGON WALK (GEARS TICK + MESH)", &items[4].0, Vec3::new(1.0, 0.35, -0.3), st, 1.0, |t| (2.0, 2.0 * t), None);
        let s_mech = strip("STEAMPUNK MECH WALK (PISTONS, STACKS PUFF)", &items[7].0, Vec3::new(1.0, 0.3, 0.6), st, 1.0, |t| (3.0, 3.0 * t), None);
        let s_ship = strip("PIRATE SHIP UNDER SAIL 8 M/S", &items[3].0, Vec3::new(1.0, 0.3, -0.3), st, 3.0, |t| (8.0, 8.0 * t), None);
        stack(vec![sheet, s_train, s_train_c, s_train_q, s_heli, s_tank, s_drag, s_mech, s_ship]).save(&docs.join("gallery_machines.png"));
    }
    if features {
        let items: Vec<(Built, Vec3)> = bd_shapes::ALL_FEATURES
            .iter()
            .map(|f| {
                let r = hand(Form::Blob0, 3.0, &[], &[*f], 1);
                (build_r(&format!("{f:?}").to_uppercase(), &r), Vec3::new(1.0, 0.5, -0.8))
            })
            .chain([Form::Quadruped, Form::Biped, Form::Hammer, Form::Wall].into_iter().map(|form| {
                let r = hand(form, suggested_scale(form), &[], &bd_shapes::ALL_FEATURES, 1);
                (build_r(&format!("{form:?} + ALL").to_uppercase(), &r), Vec3::new(1.0, 0.5, -0.8))
            }))
            .collect();
        posed_sheet(&items, tile, 8, "EVERY FEATURE ON A BLOB (+ ALL FEATURES ON 4 FORMS)").save(&docs.join("gallery_features.png"));
    }
}
