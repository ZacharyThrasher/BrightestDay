//! `--transforms`: alt → robot strips (procedural panel split) + a 3-vehicle combiner.

use super::*;

fn rec(lex: Option<&bd_lexicon::Lexicon>, text: &str, form: Form, scale: f32, feats: &[bd_core::mechanics::Feature], seed: u32) -> Recipe {
    let mut r = super::machines::phrase(lex, text, || {
        let mut r = demo_recipe(form, seed, &[]);
        r.features = feats.to_vec();
        r
    });
    // Gallery size: big enough that panels read.
    r.scale = scale;
    r.normalized()
}

fn frame_tile(c: &mut Canvas, x0: i32, y0: i32, tile: i32) {
    c.rect(x0 + 1, y0 + 1, tile - 2, tile - 2, [22, 25, 32]);
    c.frame(x0, y0, tile, tile, [40, 44, 54]);
}

fn strip(label: &str, r: &Recipe, eye: Vec3, tile: usize) -> Canvas {
    let (_, alt) = build_model(r);
    let rig = transform_rig(&alt, 1);
    let alt_meshes = mesh_model(&rig.alt);
    let robot_meshes = mesh_model(&rig.robot);
    println!(
        "{label:28} panels {:2} alt voxels {:5} robot voxels {:5} robot parts {} gait {:?} height {:.2} m",
        rig.panels.len() - 1,
        rig.alt.voxel_count,
        rig.robot.voxel_count,
        rig.robot.parts.len(),
        rig.robot.rig.gait,
        rig.height
    );
    let ts = [0.0f32, 0.12, 0.26, 0.4, 0.55, 0.7, 0.85, 1.0];
    let frames = ts.len() + 2;
    let head = 14;
    let mut c = Canvas::new(frames * tile, tile + head);
    let ev = sequence_events(&rig);
    text(&mut c, 4, 3, &format!("{}  ({} PANELS, {:.1}S, {} EVENTS) -> ROBOT RUN + PUNCH", label, rig.panels.len() - 1, TRANSFORM_SECONDS, ev.len()), [220, 235, 220]);
    let cam = Cam::from_eye(eye);
    let n = rig.alt.parts.len();
    let mut poses = Vec::new();
    let mut pts = Vec::new();
    for &t in &ts {
        let mut p = vec![Affine3A::IDENTITY; n];
        pose_transform(&rig, t, true, &mut p);
        pts.extend(world_points(&Item { model: &rig.alt, meshes: &alt_meshes, pose: p.clone(), root: Affine3A::IDENTITY, pal: GREEN }));
        poses.push(p);
    }
    let mut run = vec![Affine3A::IDENTITY; rig.robot.parts.len()];
    pose(&rig.robot, AnimState::Locomotion { speed: 5.0, distance: 1.1, airborne: false }, 0.3, &mut run);
    let mut punch = vec![Affine3A::IDENTITY; rig.robot.parts.len()];
    pose(&rig.robot, AnimState::Attack(Attack::Swipe), 0.25, &mut punch);
    for p in [&run, &punch] {
        pts.extend(world_points(&Item { model: &rig.robot, meshes: &robot_meshes, pose: p.clone(), root: Affine3A::IDENTITY, pal: GREEN }));
    }
    pts.push(Vec3::ZERO);
    let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32, 8.0);
    for f in 0..frames {
        let x0 = (f * tile) as i32;
        let ty = head as i32;
        frame_tile(&mut c, x0, ty, tile as i32);
        let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + ty as f32 };
        let gy = fit.oy.round() as i32;
        for x in x0 + 2..x0 + tile as i32 - 2 {
            c.put(x, gy, [48, 56, 60]);
        }
        c.clear_depth();
        let clip = (x0 + 1, ty + 1, x0 + tile as i32 - 1, ty + tile as i32 - 1);
        if f < ts.len() {
            draw(&mut c, &cam, fit, &Item { model: &rig.alt, meshes: &alt_meshes, pose: poses[f].clone(), root: Affine3A::IDENTITY, pal: GREEN }, clip);
            // Event sparks near this time.
            for e in ev.iter().filter(|e| (e.time - ts[f]).abs() < 0.035 && e.kind != TransformEventKind::Hinge) {
                let (x, y, _) = cam.proj(e.pos);
                let (sx, sy) = ((fit.ox + x * fit.scale) as i32, (fit.oy - y * fit.scale) as i32);
                for d in -3..=3 {
                    c.put(sx + d, sy, [255, 240, 150]);
                    c.put(sx, sy + d, [255, 240, 150]);
                }
            }
            text(&mut c, x0 + 4, ty + tile as i32 - 11, &format!("T {:.2}", ts[f]), [140, 160, 150]);
        } else {
            let p = if f == ts.len() { &run } else { &punch };
            draw(&mut c, &cam, fit, &Item { model: &rig.robot, meshes: &robot_meshes, pose: p.clone(), root: Affine3A::IDENTITY, pal: GREEN }, clip);
            text(&mut c, x0 + 4, ty + tile as i32 - 11, if f == ts.len() { "ROBOT RUN" } else { "ROBOT PUNCH" }, [140, 160, 150]);
        }
    }
    c
}

fn combine_strip(recipes: &[(String, Recipe)], tile: usize) -> Canvas {
    let rigs: Vec<TransformRig> = recipes.iter().map(|(_, r)| transform_rig(&build_model(r).1, 1)).collect();
    let comb = combine_rigs(&rigs);
    let meshes: Vec<Vec<PartMesh>> = rigs.iter().map(|r| mesh_model(&r.alt)).collect();
    let giant_meshes = mesh_model(&comb.giant);
    println!("combiner: giant voxels {} parts {} gait {:?}", comb.giant.voxel_count, comb.giant.parts.len(), comb.giant.rig.gait);
    let ts = [0.0f32, 0.15, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0];
    let frames = ts.len() + 1;
    let head = 14;
    let mut c = Canvas::new(frames * tile, tile + head);
    let names: Vec<String> = recipes.iter().map(|r| r.0.clone()).collect();
    text(&mut c, 4, 3, &format!("COMBINER: {} -> GIANT ({:.1}S)", names.join(" + "), COMBINE_SECONDS), [220, 235, 220]);
    let cam = Cam::from_eye(Vec3::new(0.7, 0.3, -1.0));
    let mut all = Vec::new();
    let mut frames_p = Vec::new();
    for &t in &ts {
        let mut out = vec![Affine3A::IDENTITY; rigs.len()];
        pose_combine(&comb, t, &mut out);
        for (i, r) in rigs.iter().enumerate() {
            all.extend(world_points(&Item { model: &r.alt, meshes: &meshes[i], pose: vec![Affine3A::IDENTITY; r.alt.parts.len()], root: out[i], pal: GREEN }));
        }
        frames_p.push(out);
    }
    all.push(Vec3::ZERO);
    let fit0 = fit_points(&cam, &all, 0.0, 0.0, tile as f32, tile as f32, 8.0);
    for f in 0..frames {
        let x0 = (f * tile) as i32;
        let ty = head as i32;
        frame_tile(&mut c, x0, ty, tile as i32);
        let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + ty as f32 };
        c.clear_depth();
        let clip = (x0 + 1, ty + 1, x0 + tile as i32 - 1, ty + tile as i32 - 1);
        if f + 1 < ts.len() {
            for (i, r) in rigs.iter().enumerate() {
                let pal = [GREEN, [[0.3, 0.6, 0.95], [0.7, 0.85, 1.0], [0.08, 0.15, 0.35], [0.95, 0.97, 1.0]], [[0.95, 0.75, 0.2], [1.0, 0.9, 0.6], [0.35, 0.22, 0.05], [1.0, 1.0, 0.9]]][i % 3];
                draw(&mut c, &cam, fit, &Item { model: &r.alt, meshes: &meshes[i], pose: vec![Affine3A::IDENTITY; r.alt.parts.len()], root: frames_p[f][i], pal }, clip);
            }
            text(&mut c, x0 + 4, ty + tile as i32 - 11, &format!("T {:.2}", ts[f]), [140, 160, 150]);
        } else {
            let mut p = vec![Affine3A::IDENTITY; comb.giant.parts.len()];
            let (a, l) = if f + 1 == ts.len() { (AnimState::Idle, "T 1.00 GIANT") } else { (AnimState::Attack(Attack::Stomp), "GIANT STOMP") };
            pose(&comb.giant, a, 0.4, &mut p);
            draw(&mut c, &cam, fit, &Item { model: &comb.giant, meshes: &giant_meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN }, clip);
            text(&mut c, x0 + 4, ty + tile as i32 - 11, l, [140, 160, 150]);
        }
    }
    c
}

pub fn run(docs: &std::path::Path, lex: Option<&bd_lexicon::Lexicon>, only: Option<String>) {
    use bd_core::mechanics::Feature::*;
    let tile = 200;
    let eye = Vec3::new(1.0, 0.35, -0.75);
    let cases: Vec<(&str, Recipe)> = vec![
        ("TRUCK", rec(lex, "semi truck", Form::Vehicle, 7.0, &[Wheels, Cab, Exhaust], 2)),
        ("MUSCLE CAR", rec(lex, "muscle car", Form::Vehicle, 5.5, &[Wheels, Exhaust], 4)),
        ("JET", rec(lex, "fighter jet", Form::Vehicle, 7.0, &[Jet, Fins, Cockpit], 2)),
        ("TANK", rec(lex, "tank", Form::Vehicle, 6.0, &[Treads, Turret, Barrels], 5)),
        ("SHARK", rec(lex, "shark", Form::Fish, 5.0, &[Fins], 1)),
    ];
    let mut out = Vec::new();
    for (label, r) in &cases {
        if only.as_deref().is_some_and(|o| !label.to_lowercase().contains(o)) {
            continue;
        }
        out.push(strip(label, r, eye, tile));
    }
    if only.as_deref().is_none_or(|o| o.contains("comb")) {
        let trio: Vec<(String, Recipe)> = cases.iter().take(3).map(|(l, r)| (l.to_string(), r.clone())).collect();
        out.push(combine_strip(&trio, tile + 60));
    }
    let path = if only.is_some() { docs.join("gallery_transforms_only.png") } else { docs.join("gallery_transforms.png") };
    stack(out).save(&path);
}
