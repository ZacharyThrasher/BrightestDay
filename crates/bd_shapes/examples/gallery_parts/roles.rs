//! `--roles`: construct animation by part role × semantic event (bd_shapes `roles` + `behaviours`).
//! Contact sheets (docs/ + target/anim/):
//! * `anim_roles_machines.png` — tank (assembly, turret aim, fire/recoil, hatch open/shut), car
//!   (steer), helicopter (rotor spin-up, ghosted previous frames show the speed);
//! * `anim_roles_creatures.png` — mech (canopy opens for the pilot, seat revealed, shuts), dragon
//!   (head look, roar, blink, hit), shark, sword blade sliding out;
//! * `anim_roles_icons.png` — 8 curated icons through the *inference* (rest vs. every event at
//!   once) + a census of inferred roles over every icon pack (stdout);
//! * `anim_assembly.png` — summon assembly frames for tank / mech / dragon (the ring marked).

use super::*;
use bd_core::mechanics::Feature;
use bd_shapes::behaviours::{AnimDrive, AnimEvent, ConstructAnimator};
use bd_shapes::roles::Role;

const FR: usize = 8;
const DT: f32 = 1.0 / 120.0;

fn hand(form: Form, scale: f32, mech: &[Mechanic], feats: &[Feature], seed: u32) -> Recipe {
    let mut r = demo_recipe(form, seed, mech);
    r.scale = scale;
    r.features = feats.to_vec();
    r.normalized()
}

fn built(label: &str, r: &Recipe) -> Built {
    let mut b = build(label.to_string(), r);
    b.model.weight = weight_of(r);
    b
}

/// One scripted frame request: time (s) + caption.
struct Shot(f32, String);

fn shot(t: f32, s: &str) -> Shot {
    Shot(t, s.to_string())
}

/// Script: called every 1/120 s step with (t_prev, t, animator) → the drive for this step.
type Script<'a> = dyn FnMut(f32, f32, &mut ConstructAnimator) -> AnimDrive + 'a;

fn mark(c: &mut Canvas, cam: &Cam, fit: Fit, p: Vec3, col: [u8; 3], clip: (i32, i32, i32, i32)) {
    let (x, y, _) = cam.proj(p);
    let sx = (fit.ox + x * fit.scale) as i32;
    let sy = (fit.oy - y * fit.scale) as i32;
    for yy in sy - 2..=sy + 2 {
        for xx in sx - 2..=sx + 2 {
            if xx > clip.0 && yy > clip.1 && xx < clip.2 - 1 && yy < clip.3 - 1 {
                c.put(xx, yy, col);
            }
        }
    }
}

/// A strip of `FR` frames of `b` driven by `script`. `ghost` draws the two previous 1/60 s poses
/// dimmed (rotor speed); `marks` returns world points to mark (ring, aim target) per shot time.
#[allow(clippy::too_many_arguments)]
fn strip(
    label: &str,
    b: &Built,
    eye: Vec3,
    shots: &[Shot],
    script: &mut Script,
    ghost: bool,
    marks: &dyn Fn(f32) -> Vec<(Vec3, [u8; 3])>,
    tile: usize,
    margin_up: f32,
) -> Canvas {
    let head = 14;
    let n = b.model.parts.len();
    let mut c = Canvas::new(FR * tile, tile + head);
    text(&mut c, 4, 3, label, [220, 235, 220]);
    let cam = Cam::from_eye(eye);
    let rest = Item { model: &b.model, meshes: &b.meshes, pose: vec![Affine3A::IDENTITY; n], root: Affine3A::IDENTITY, pal: GREEN };
    let mut pts = world_points(&rest);
    let bb = &b.model.bounds;
    let sz = (bb.max - bb.min).max_element();
    pts.push(Vec3::new(bb.max.x, bb.max.y + margin_up * sz, bb.max.z));
    pts.push(Vec3::new(bb.min.x - 0.1 * sz, bb.min.y, bb.min.z - 0.1 * sz));
    let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32 - 12.0, 10.0);
    let mut anim = ConstructAnimator::new(&b.model, 7);
    let mut t = 0.0f32;
    let mut dist = 0.0f32;
    let mut history: Vec<Vec<Affine3A>> = Vec::new();
    let mut drive = AnimDrive::default();
    for (f, s) in shots.iter().enumerate().take(FR) {
        while t < s.0 - 1e-6 {
            let prev = t;
            t += DT;
            drive = script(prev, t, &mut anim);
            drive.time = t;
            dist += drive.speed * DT;
            anim.update(&drive, DT);
            if ghost {
                let mut p = vec![Affine3A::IDENTITY; n];
                pose_layered_with(
                    &b.model,
                    &[(AnimState::Locomotion { speed: drive.speed, distance: dist, airborne: drive.airborne }, t)],
                    Some(anim.locals()),
                    &mut p,
                );
                history.push(p);
                if history.len() > 6 {
                    history.remove(0);
                }
            }
        }
        let x0 = (f * tile) as i32;
        c.rect(x0 + 1, head as i32 + 1, tile as i32 - 2, tile as i32 - 2, [22, 25, 32]);
        c.frame(x0, head as i32, tile as i32, tile as i32, [40, 44, 54]);
        let clip = (x0 + 1, head as i32 + 1, x0 + tile as i32 - 1, head as i32 + tile as i32 - 1);
        let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + head as f32 + 12.0 };
        let gy = fit.oy.round() as i32;
        for x in x0 + 2..x0 + tile as i32 - 2 {
            c.put(x, gy, [48, 56, 60]);
        }
        c.clear_depth();
        if ghost {
            for (k, p) in history.iter().rev().skip(1).step_by(2).take(2).enumerate() {
                let dim = [[0.1, 0.3, 0.16], [0.07, 0.2, 0.11]][k.min(1)];
                draw(
                    &mut c,
                    &cam,
                    fit,
                    &Item { model: &b.model, meshes: &b.meshes, pose: p.clone(), root: Affine3A::IDENTITY, pal: [dim; 4] },
                    clip,
                );
            }
            c.clear_depth();
        }
        let mut p = vec![Affine3A::IDENTITY; n];
        pose_layered_with(
            &b.model,
            &[(AnimState::Locomotion { speed: drive.speed, distance: dist, airborne: drive.airborne }, t)],
            Some(anim.locals()),
            &mut p,
        );
        draw(&mut c, &cam, fit, &Item { model: &b.model, meshes: &b.meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN }, clip);
        for (m, col) in marks(s.0) {
            mark(&mut c, &cam, fit, m, col, clip);
        }
        text(&mut c, x0 + 4, head as i32 + 4, &s.1, [200, 225, 205]);
        text(&mut c, x0 + 4, head as i32 + tile as i32 - 11, &format!("{:.0}MS", t * 1e3), [140, 160, 150]);
    }
    c
}

fn roles_summary(m: &VoxelModel) -> String {
    let r = bd_shapes::roles::analyze(m);
    let mut names: Vec<(String, usize)> = Vec::new();
    for p in &r.parts {
        if matches!(p.role, Role::None | Role::Seat | Role::Support | Role::Limb | Role::Cloth) {
            continue;
        }
        let nm = format!("{}{}", p.role.name(), if p.inferred { "+" } else { "" });
        match names.iter_mut().find(|(n, _)| *n == nm) {
            Some((_, k)) => *k += 1,
            None => names.push((nm, 1)),
        }
    }
    if names.is_empty() {
        return "NO ROLES (STATIC)".into();
    }
    names.iter().map(|(n, k)| if *k > 1 { format!("{k} {n}") } else { n.clone() }).collect::<Vec<_>>().join(" ")
}

fn ring_for(b: &Built) -> Vec3 {
    let bb = &b.model.bounds;
    let s = (bb.max - bb.min).max_element();
    Vec3::new(bb.max.x + 0.4 * s, bb.min.y + 0.9 * (bb.max.y - bb.min.y).max(1.0), bb.max.z + 0.6 * s)
}

fn assembly_strip(label: &str, b: &Built, eye: Vec3, tile: usize) -> Canvas {
    let total = ConstructAnimator::new(&b.model, 7).assembly_seconds();
    let ring = ring_for(b);
    let shots: Vec<Shot> = (0..FR)
        .map(|k| {
            shot(
                0.004 + total * (k as f32 / (FR - 2) as f32).min(1.0) + if k == FR - 1 { 0.25 } else { 0.0 },
                if k == 0 {
                    "SUMMON"
                } else if k == FR - 1 {
                    "LOCKED"
                } else {
                    ""
                },
            )
        })
        .collect();
    let mut summoned = false;
    let mut script = move |_p: f32, _t: f32, a: &mut ConstructAnimator| {
        if !summoned {
            summoned = true;
            a.event(AnimEvent::Summon);
        }
        AnimDrive { ring: Some(ring), ..Default::default() }
    };
    let marks = move |_t: f32| vec![(ring, [255, 240, 120])];
    strip(
        &format!("{label}: SUMMON ASSEMBLY {:.0}MS (PARTS STREAM IN FROM THE CASTER SIDE)", total * 1e3),
        b,
        eye,
        &shots,
        &mut script,
        false,
        &marks,
        tile,
        0.15,
    )
}

fn machines(tile: usize, rows: &mut Vec<Canvas>, asm: &mut Vec<Canvas>) {
    // ---------------------------------------------------------------- tank
    let tank = built("tank", &hand(Form::Vehicle, 7.0, &[], &[Feature::Treads, Feature::Turret], 2));
    println!("tank roles: {}", roles_summary(&tank.model));
    asm.push(assembly_strip("TANK", &tank, Vec3::new(1.0, 0.55, -0.9), tile));
    rows.push(assembly_strip("TANK", &tank, Vec3::new(1.0, 0.55, -0.9), tile));
    let shots = [
        shot(0.8, "IDLE SCAN"),
        shot(1.6, "AIM LEFT"),
        shot(2.4, "AIM RIGHT+UP"),
        shot(2.43, "FIRE +30MS"),
        shot(2.52, "+120MS"),
        shot(2.8, "+400MS"),
        shot(3.6, "PILOT NEAR"),
        shot(4.4, "SEATED: SHUT"),
    ];
    let mut script = |p: f32, t: f32, a: &mut ConstructAnimator| {
        let aim = if t < 1.0 {
            None
        } else if t < 1.8 {
            Some(Vec3::new(-1.0, 0.0, -0.4).normalize())
        } else {
            Some(Vec3::new(0.8, 0.35, -0.6).normalize())
        };
        if p < 2.4 && t >= 2.4 {
            a.event(AnimEvent::Fire { strength: 1.5 });
        }
        if p < 3.9 && t >= 3.9 {
            a.event(AnimEvent::PilotEnter);
        }
        AnimDrive { aim, pilot_near: (2.9..3.9).contains(&t), piloted: t >= 3.9, ..Default::default() }
    };
    rows.push(strip(
        "TANK: TURRET TRACKS THE AIM (LAG BY WEIGHT), BARREL RECOILS, HATCH OPENS FOR THE PILOT + CLUNKS SHUT",
        &tank,
        Vec3::new(1.0, 0.7, -0.5),
        &shots,
        &mut script,
        false,
        &|_| vec![],
        tile,
        0.3,
    ));
    // ---------------------------------------------------------------- car
    let car = built("car", &hand(Form::Vehicle, 5.0, &[], &[Feature::Wheels, Feature::Cab, Feature::Exhaust], 1));
    println!("car roles: {}", roles_summary(&car.model));
    let shots = [
        shot(0.08, "SUMMON"),
        shot(0.2, ""),
        shot(0.45, "LOCKED"),
        shot(1.2, "DRIVE"),
        shot(1.8, "TURN LEFT"),
        shot(2.2, ""),
        shot(2.9, "TURN RIGHT"),
        shot(3.8, "STOP"),
    ];
    let mut sum = false;
    let ring = ring_for(&car);
    let mut script = move |_p: f32, t: f32, a: &mut ConstructAnimator| {
        if !sum {
            sum = true;
            a.event(AnimEvent::Summon);
        }
        let speed = if (0.8..3.4).contains(&t) { 8.0 } else { 0.0 };
        let turn = if (1.4..2.3).contains(&t) {
            1.2
        } else if (2.5..3.1).contains(&t) {
            -1.2
        } else {
            0.0
        };
        AnimDrive { speed, turn, ring: Some(ring), ..Default::default() }
    };
    rows.push(strip(
        "CAR: ASSEMBLES, WHEELS ROLL, FRONT WHEELS STEER INTO TURNS",
        &car,
        Vec3::new(0.9, 0.9, -0.6),
        &shots,
        &mut script,
        false,
        &move |t| if t < 0.5 { vec![(ring, [255, 240, 120])] } else { vec![] },
        tile,
        0.2,
    ));
    // ---------------------------------------------------------------- helicopter
    let heli = built("heli", &hand(Form::Vehicle, 9.0, &[Mechanic::Flies], &[Feature::Rotor], 1));
    println!("helicopter roles: {}", roles_summary(&heli.model));
    let shots = [
        shot(0.1, "SUMMON +0.1S"),
        shot(0.35, "+0.35"),
        shot(0.7, "+0.7 SPIN-UP"),
        shot(1.2, "+1.2"),
        shot(2.0, "+2.0 IDLE"),
        shot(3.0, "THROTTLE"),
        shot(3.6, ""),
        shot(4.4, "CHARGE"),
    ];
    let mut sum = false;
    let mut script = move |_p: f32, t: f32, a: &mut ConstructAnimator| {
        if !sum {
            sum = true;
            a.event(AnimEvent::Summon);
        }
        let speed = if (2.5..4.0).contains(&t) { 14.0 } else { 0.0 };
        AnimDrive { speed, airborne: t > 2.5, charge: if t > 4.0 { 1.0 } else { 0.0 }, ..Default::default() }
    };
    rows.push(strip(
        "HELICOPTER: ROTORS SPIN UP FROM REST ON SUMMON, RISE WITH THROTTLE + CHARGE (GHOSTS = LAST 2 FRAMES @60FPS)",
        &heli,
        Vec3::new(0.8, 0.9, -0.7),
        &shots,
        &mut script,
        true,
        &|_| vec![],
        tile,
        0.1,
    ));
}

fn creatures(lex: Option<&bd_lexicon::Lexicon>, tile: usize, rows: &mut Vec<Canvas>, asm: &mut Vec<Canvas>) {
    // ---------------------------------------------------------------- mech
    let mech = built("mech", &hand(Form::Biped, 12.0, &[Mechanic::Piloted], &[], 1));
    println!("mech roles: {}", roles_summary(&mech.model));
    asm.push(assembly_strip("MECH", &mech, Vec3::new(0.9, 0.45, -1.1), tile));
    let seat = mech.model.seat.map(|s| s.pos).unwrap_or(Vec3::ZERO);
    let shots = [
        shot(0.6, "IDLE"),
        shot(0.9, "PILOT APPROACH"),
        shot(1.1, ""),
        shot(1.5, "OPEN: SEAT"),
        shot(2.0, "CLIMB IN"),
        shot(2.12, ""),
        shot(2.35, "SHUT (CLUNK)"),
        shot(3.2, "EXIT: REOPENS"),
    ];
    let mut script = |p: f32, t: f32, a: &mut ConstructAnimator| {
        if p < 1.9 && t >= 1.9 {
            a.event(AnimEvent::PilotEnter);
        }
        if p < 3.0 && t >= 3.0 {
            a.event(AnimEvent::PilotExit);
        }
        AnimDrive { pilot_near: (0.75..1.9).contains(&t), piloted: (1.9..3.0).contains(&t), ..Default::default() }
    };
    rows.push(strip(
        "MECH: CHEST CANOPY SWINGS OPEN AS THE PILOT APPROACHES (SEAT = RED), STAYS OPEN WHILE THEY CLIMB IN, SLAMS SHUT",
        &mech,
        Vec3::new(1.0, 0.25, -0.55),
        &shots,
        &mut script,
        false,
        &move |_| vec![(seat, [255, 90, 90])],
        tile,
        0.05,
    ));
    // ---------------------------------------------------------------- dragon
    let dr = match lex {
        Some(l) => {
            let r = l.resolve("dragon");
            println!("dragon => {:?} {:?}", r.recipe.form, r.recipe.mechanics);
            match &r.shape {
                Some(s) => {
                    let (_, mut m) = build_icon_model(&r.recipe, s);
                    m.weight = weight_of(&r.recipe);
                    let meshes = mesh_model(&m);
                    Built { label: "dragon".into(), form: r.recipe.form, model: m, meshes }
                }
                None => built("dragon", &r.recipe),
            }
        }
        None => built("dragon", &hand(Form::Quadruped, 4.0, &[Mechanic::Flies, Mechanic::Breath], &[], 3)),
    };
    println!("dragon roles: {}", roles_summary(&dr.model));
    asm.push(assembly_strip("DRAGON", &dr, Vec3::new(0.95, 0.55, -1.0), tile));
    let bb = dr.model.bounds;
    let sz = (bb.max - bb.min).max_element();
    let tgt =
        |t: f32| -> Vec3 { if t < 1.3 { Vec3::new(-1.2 * sz, 0.4 * sz, -1.2 * sz) } else { Vec3::new(1.2 * sz, 0.9 * sz, -sz) } };
    let shots = [
        shot(0.5, "IDLE"),
        shot(1.2, "LOOK LEFT"),
        shot(2.0, "LOOK RIGHT+UP"),
        shot(2.25, "ROAR"),
        shot(2.5, ""),
        shot(3.02, "BLINK"),
        shot(3.55, "HIT FROM LEFT"),
        shot(3.7, ""),
    ];
    let mut script = |p: f32, t: f32, a: &mut ConstructAnimator| {
        if p < 2.1 && t >= 2.1 {
            a.event(AnimEvent::Attack { roar: true });
        }
        if p < 3.5 && t >= 3.5 {
            a.event(AnimEvent::Hit { dir: Vec3::X, strength: 1.2 });
        }
        let look = if t > 0.6 { Some(tgt(t)) } else { None };
        AnimDrive { look, aim: look.map(|l| l.normalize()), ..Default::default() }
    };
    rows.push(strip(
        "DRAGON: HEAD LOOKS AT THE NEAREST ENEMY (BLUE), JAW ROARS ON ATTACK, EYES BLINK, FLINCHES ON HITS",
        &dr,
        Vec3::new(0.55, 0.35, -1.0),
        &shots,
        &mut script,
        false,
        &|t| if t > 0.6 { vec![(tgt(t), [120, 170, 255])] } else { vec![] },
        tile,
        0.2,
    ));
    // ---------------------------------------------------------------- shark
    let shark = match lex {
        Some(l) => built("shark", &l.resolve("shark").recipe),
        None => built("shark", &hand(Form::Fish, 3.0, &[], &[], 4)),
    };
    println!("shark roles: {}", roles_summary(&shark.model));
    let shots = [
        shot(0.05, "SUMMON"),
        shot(0.2, ""),
        shot(0.4, "LOCKED"),
        shot(1.0, "LOOK"),
        shot(1.6, "BITE ROAR"),
        shot(1.75, ""),
        shot(2.6, "SWIM"),
        shot(3.0, ""),
    ];
    let mut sum = false;
    let mut script = move |p: f32, t: f32, a: &mut ConstructAnimator| {
        if !sum {
            sum = true;
            a.event(AnimEvent::Summon);
        }
        if p < 1.5 && t >= 1.5 {
            a.event(AnimEvent::Attack { roar: true });
        }
        AnimDrive {
            look: if t > 0.7 { Some(Vec3::new(-2.0, 0.5, -3.0)) } else { None },
            speed: if t > 2.2 { 5.0 } else { 0.0 },
            ..Default::default()
        }
    };
    rows.push(strip(
        "SHARK: ASSEMBLES, LOOKS, JAW OPENS ON THE BITE, SWIMS",
        &shark,
        Vec3::new(1.0, 0.5, -0.4),
        &shots,
        &mut script,
        false,
        &|_| vec![],
        tile,
        0.1,
    ));
    // ---------------------------------------------------------------- sword
    let sword = built("sword", &hand(Form::Sword, 1.2, &[], &[], 1));
    println!("sword roles: {}  grip {:?}", roles_summary(&sword.model), grip_point(&sword.model));
    rows.push(assembly_strip("SWORD (BLADE SLIDES OUT OF THE HILT)", &sword, Vec3::new(1.0, 0.45, -0.3), tile));
}

/// Every `*.ron` icon pack: (name, spec).
fn all_icon_specs(root: &std::path::Path) -> Vec<(String, ShapeSpec)> {
    let mut out = Vec::new();
    let mut files = vec![root.join("assets/icons.ron")];
    if let Ok(rd) = std::fs::read_dir(root.join("assets/icons")) {
        let mut v: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ron")).collect();
        v.sort();
        files.extend(v);
    }
    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct IconEntry {
        names: Vec<String>,
        #[serde(default)]
        shape: Option<ShapeSpec>,
    }
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        match ron::from_str::<Vec<IconEntry>>(&src) {
            Ok(v) => out.extend(v.into_iter().filter_map(|e| Some((e.names.first()?.clone(), e.shape?)))),
            Err(e) => println!("skip {} ({e})", f.display()),
        }
    }
    out
}

fn icons(root: &std::path::Path, lex: Option<&bd_lexicon::Lexicon>, tile: usize) -> Vec<Canvas> {
    let specs = all_icon_specs(root);
    println!("icon census over {} shaped icons:", specs.len());
    // Census: which roles the resolver / inference assigns across every icon.
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut inferred_models = 0;
    let mut touched_list: Vec<usize> = Vec::new();
    let mut parts_total = 0;
    for (si, (sname, spec)) in specs.iter().enumerate() {
        let m = voxelize_capped(spec, MAX_VOXELS);
        let r = bd_shapes::roles::analyze(&m);
        parts_total += m.parts.len();
        let mut any = false;
        for p in &r.parts {
            if p.inferred {
                any = true;
                let nm = p.role.name().to_string();
                match counts.iter_mut().find(|(n, _)| *n == nm) {
                    Some((_, k)) => *k += 1,
                    None => counts.push((nm, 1)),
                }
            }
        }
        inferred_models += any as usize;
        if any {
            touched_list.push(si);
            let what: Vec<String> = r.parts.iter().filter(|p| p.inferred).map(|p| p.role.name().to_string()).collect();
            println!("    inferred on {sname}: {what:?}");
        }
    }
    println!("  {} parts; inference fired on {} of {} icons: {:?}", parts_total, inferred_models, specs.len(), counts);
    // Pick 8 icons deterministically: the first 4 the inference touched (so the sheet shows it
    // acting) + 4 more at a fixed stride through the whole catalogue (so it shows it staying still).
    let mut picks: Vec<usize> = touched_list.iter().copied().take(4).collect();
    let stride = (specs.len() / 5).max(1);
    let mut k = 37;
    while picks.len() < 8 && !specs.is_empty() && k < specs.len() * 8 {
        let i = k % specs.len();
        k += stride + 1;
        if !picks.contains(&i) {
            picks.push(i);
        }
    }
    let mut rows = Vec::new();
    for chunk in picks.chunks(4) {
        let head = 26;
        let mut c = Canvas::new(FR * tile, tile + head);
        text(
            &mut c,
            4,
            3,
            "ICONS THROUGH ROLE INFERENCE (+ = INFERRED): REST | EVERY EVENT AT ONCE (AIM, FIRE, PILOT, DRIVE+TURN, HIT, CHARGE)",
            [220, 235, 220],
        );
        for (j, &i) in chunk.iter().enumerate() {
            let (name, spec) = &specs[i];
            let (recipe, shape) = match lex {
                Some(l) => {
                    let r = l.resolve(name);
                    (r.recipe, r.shape.unwrap_or_else(|| spec.clone()))
                }
                None => (Recipe::unknown(name), spec.clone()),
            };
            let (_, mut m) = build_icon_model(&recipe, &shape);
            m.weight = weight_of(&recipe);
            let meshes = mesh_model(&m);
            let summary = roles_summary(&m);
            println!("  icon {:28} {:?}: {}", name, recipe.form, summary);
            let b = Built { label: name.to_uppercase(), form: recipe.form, model: m, meshes };
            let eye = eye_for(b.form);
            for (fi, stim) in [false, true].into_iter().enumerate() {
                let n = b.model.parts.len();
                let mut anim = ConstructAnimator::new(&b.model, 3);
                let mut t = 0.0;
                let mut dist = 0.0;
                let mut drive = AnimDrive::default();
                while t < 1.4 {
                    let prev = t;
                    t += DT;
                    drive = if stim {
                        if prev < 0.9 && t >= 0.9 {
                            anim.event(AnimEvent::Fire { strength: 2.0 });
                            anim.event(AnimEvent::Hit { dir: Vec3::X, strength: 1.0 });
                            anim.event(AnimEvent::Attack { roar: true });
                        }
                        AnimDrive {
                            time: t,
                            speed: 6.0,
                            turn: 1.0,
                            aim: Some(Vec3::new(0.7, 0.3, -0.6).normalize()),
                            look: Some(Vec3::new(3.0, 2.0, -4.0)),
                            pilot_near: true,
                            charge: 1.0,
                            ..Default::default()
                        }
                    } else {
                        AnimDrive { time: t, ..Default::default() }
                    };
                    dist += drive.speed * DT;
                    anim.update(&drive, DT);
                }
                let x0 = ((j * 2 + fi) * tile) as i32;
                let y0 = head as i32;
                c.rect(x0 + 1, y0 + 1, tile as i32 - 2, tile as i32 - 2, [22, 25, 32]);
                c.frame(x0, y0, tile as i32, tile as i32, [40, 44, 54]);
                let cam = Cam::from_eye(eye);
                let rest =
                    Item { model: &b.model, meshes: &b.meshes, pose: vec![Affine3A::IDENTITY; n], root: Affine3A::IDENTITY, pal: GREEN };
                let pts = world_points(&rest);
                let fit = fit_points(&cam, &pts, x0 as f32, y0 as f32 + 12.0, tile as f32, tile as f32 - 36.0, 8.0);
                let mut p = vec![Affine3A::IDENTITY; n];
                pose_layered_with(
                    &b.model,
                    &[(AnimState::Locomotion { speed: drive.speed, distance: dist, airborne: false }, t)],
                    Some(anim.locals()),
                    &mut p,
                );
                c.clear_depth();
                let clip = (x0 + 1, y0 + 1, x0 + tile as i32 - 1, y0 + tile as i32 - 1);
                draw(&mut c, &cam, fit, &Item { model: &b.model, meshes: &b.meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN }, clip);
                let lbl: String = b.label.chars().take(22).collect();
                text(&mut c, x0 + 4, y0 + 4, &if fi == 0 { lbl } else { "ALL EVENTS".into() }, [220, 235, 220]);
                if fi == 0 {
                    let s: String = summary.chars().take(28).collect();
                    text(&mut c, x0 + 4, y0 + tile as i32 - 22, &s, [150, 200, 160]);
                    let s2: String = summary.chars().skip(28).take(28).collect();
                    text(&mut c, x0 + 4, y0 + tile as i32 - 12, &s2, [150, 200, 160]);
                }
            }
        }
        rows.push(c);
    }
    rows
}

pub fn run(root: &std::path::Path, docs: &std::path::Path, lex: Option<&bd_lexicon::Lexicon>) {
    let tile = 180;
    let out_dir = root.join("target/anim");
    let _ = std::fs::create_dir_all(&out_dir);
    let mut asm = Vec::new();
    let mut rows = Vec::new();
    machines(tile, &mut rows, &mut asm);
    let m = stack(rows);
    m.save(&docs.join("anim_roles_machines.png"));
    m.save(&out_dir.join("anim_roles_machines.png"));
    let mut rows = Vec::new();
    creatures(lex, tile, &mut rows, &mut asm);
    let c = stack(rows);
    c.save(&docs.join("anim_roles_creatures.png"));
    c.save(&out_dir.join("anim_roles_creatures.png"));
    let i = stack(icons(root, lex, tile));
    i.save(&docs.join("anim_roles_icons.png"));
    i.save(&out_dir.join("anim_roles_icons.png"));
    let a = stack(asm);
    a.save(&docs.join("anim_assembly.png"));
    a.save(&out_dir.join("anim_assembly.png"));
}
