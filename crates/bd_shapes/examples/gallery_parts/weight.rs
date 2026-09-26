//! `--weight`: mass-scaled motion — light sword vs heavy hammer swings, normal vs heavy kaiju stomp,
//! hit reactions light → heavy, a launch, and cape follow-through on a sudden stop.

use super::*;

const FR: usize = 8;

fn frame(c: &mut Canvas, x0: i32, y0: i32, tile: i32) -> (i32, i32, i32, i32) {
    c.rect(x0 + 1, y0 + 1, tile - 2, tile - 2, [22, 25, 32]);
    c.frame(x0, y0, tile, tile, [40, 44, 54]);
    (x0 + 1, y0 + 1, x0 + tile - 1, y0 + tile - 1)
}

fn swing_row(label: &str, form: Form, weight: f32, tile: usize) -> Canvas {
    let head = 14;
    let mut c = Canvas::new(FR * tile, tile + head);
    let curve = swing_curve_weighted(form, SwingKind::Primary, weight);
    let d = curve.duration();
    text(&mut c, 4, 3, &format!("{label}  W {weight:.1}  ANT {:.0}MS ACT {:.0}MS REC {:.0}MS", curve.anticipation * 1e3, curve.active * 1e3, curve.recovery * 1e3), [220, 235, 220]);
    let mut r = demo_recipe(form, 1, &[]);
    r.scale = suggested_scale(form);
    let b = build("x".into(), &r);
    let man = voxelize(&mannequin_spec(false));
    let man_mesh = mesh_model(&man);
    let hand = Affine3A::from_translation(Vec3::new(0.35, 1.45, -0.1));
    let (a0, a1) = curve.active_window();
    let ts = [0.0, a0 * 0.5, a0, a0 + (a1 - a0) * 0.5, a1, a1 + (d - a1) * 0.2, a1 + (d - a1) * 0.5, d];
    let cam = Cam::from_eye(Vec3::new(1.0, 0.25, 0.1));
    let n = b.model.parts.len();
    // Fixed framing across rows so light/heavy compare at the same scale.
    let pts = vec![Vec3::new(0.0, -0.2, -3.2), Vec3::new(0.0, 4.2, 3.2), Vec3::ZERO];
    let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32, 6.0);
    for (f, &t) in ts.iter().enumerate() {
        let x0 = (f * tile) as i32;
        let clip = frame(&mut c, x0, head as i32, tile as i32);
        let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + head as f32 };
        c.clear_depth();
        if f > 0 {
            let prev = Item { model: &b.model, meshes: &b.meshes, pose: vec![Affine3A::IDENTITY; n], root: hand * curve.sample(ts[f - 1]), pal: [[0.1, 0.25, 0.14]; 4] };
            draw(&mut c, &cam, fit, &prev, clip);
            c.clear_depth();
        }
        draw(&mut c, &cam, fit, &Item { model: &man, meshes: &man_mesh, pose: vec![Affine3A::IDENTITY; man.parts.len()], root: Affine3A::IDENTITY, pal: GREY }, clip);
        draw(&mut c, &cam, fit, &Item { model: &b.model, meshes: &b.meshes, pose: vec![Affine3A::IDENTITY; n], root: hand * curve.sample(t), pal: GREEN }, clip);
        let (p, col) = match curve.phase(t) {
            SwingPhase::Anticipation => ("ANT", [230, 200, 90]),
            SwingPhase::Active => ("ACT", [255, 90, 90]),
            _ => ("REC", [120, 170, 230]),
        };
        text(&mut c, x0 + 4, head as i32 + tile as i32 - 11, &format!("{:.0}MS", t * 1e3), [140, 160, 150]);
        text(&mut c, x0 + tile as i32 - 24, head as i32 + tile as i32 - 11, p, col);
    }
    c
}

fn stomp_row(label: &str, weight: f32, tile: usize) -> Canvas {
    let head = 14;
    let mut c = Canvas::new(FR * tile, tile + head);
    let r = demo_recipe(Form::Kaiju, 1, &[Mechanic::Stomp]);
    let mut b = build("k".into(), &r);
    b.model.weight = weight;
    let tm = attack_timing(&b.model, Attack::Stomp);
    text(&mut c, 4, 3, &format!("{label}  W {weight:.1}  ANT {:.0}MS ACT {:.0}MS REC {:.0}MS", tm.anticipation * 1e3, tm.active * 1e3, tm.recovery * 1e3), [220, 235, 220]);
    let (a, i) = (tm.anticipation, tm.impact());
    let ts = [0.0, a * 0.45, a * 0.85, a + (i - a) * 0.5, i, i + tm.recovery * 0.12, i + tm.recovery * 0.4, tm.total()];
    let cam = Cam::from_eye(Vec3::new(1.0, 0.3, -0.6));
    let n = b.model.parts.len();
    let mut pts = world_points(&Item { model: &b.model, meshes: &b.meshes, pose: vec![Affine3A::IDENTITY; n], root: Affine3A::IDENTITY, pal: GREEN });
    pts.push(Vec3::new(0.0, b.model.rig.height * 1.25, 0.0));
    let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32, 10.0);
    for (f, &t) in ts.iter().enumerate() {
        let x0 = (f * tile) as i32;
        let clip = frame(&mut c, x0, head as i32, tile as i32);
        let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + head as f32 };
        let mut p = vec![Affine3A::IDENTITY; n];
        pose(&b.model, AnimState::Attack(Attack::Stomp), t, &mut p);
        c.clear_depth();
        draw(&mut c, &cam, fit, &Item { model: &b.model, meshes: &b.meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN }, clip);
        text(&mut c, x0 + 4, head as i32 + tile as i32 - 11, &format!("{:.0}MS", t * 1e3), [140, 160, 150]);
    }
    c
}

fn costume_row(label: &str, spec: &ShapeSpec, anim: impl Fn(f32) -> (AnimState, f32), ts: &[f32], tile: usize, eye: Vec3) -> Canvas {
    let head = 14;
    let mut c = Canvas::new(FR * tile, tile + head);
    text(&mut c, 4, 3, label, [220, 235, 220]);
    let m = costume_model(spec);
    let meshes = mesh_model(&m);
    let cam = Cam::from_eye(eye);
    let pts = vec![Vec3::new(-1.3, -0.2, -1.3), Vec3::new(1.3, 2.6, 1.3)];
    let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32, 6.0);
    for (f, &t) in ts.iter().enumerate().take(FR) {
        let x0 = (f * tile) as i32;
        let clip = frame(&mut c, x0, head as i32, tile as i32);
        let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + head as f32 };
        let gy = fit.oy.round() as i32;
        for x in x0 + 2..x0 + tile as i32 - 2 {
            c.put(x, gy, [48, 56, 60]);
        }
        let (a, tt) = anim(t);
        let mut p = vec![Affine3A::IDENTITY; m.parts.len()];
        pose(&m, a, tt, &mut p);
        c.clear_depth();
        draw(&mut c, &cam, fit, &Item { model: &m, meshes: &meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN }, clip);
        text(&mut c, x0 + 4, head as i32 + tile as i32 - 11, &format!("{:.0}MS", t * 1e3), [140, 160, 150]);
    }
    c
}

/// Cape knight runs, then stops dead at t = 0.5 s: the cape swings forward and snaps back.
fn follow_row(tile: usize) -> Canvas {
    let head = 14;
    let mut c = Canvas::new(FR * tile, tile + head);
    text(&mut c, 4, 3, "FOLLOW-THROUGH: CAPE + PLUME, SPRINT 8 M/S THEN DEAD STOP AT 0.50S (SECONDARYMOTION)", [220, 235, 220]);
    let mut r = demo_recipe(Form::Biped, 1, &[Mechanic::Glide]);
    r.name = "knight".into();
    let spec = costume(&r, 0, CostumeTweaks::default());
    let m = costume_model(&spec);
    let meshes = mesh_model(&m);
    let mut sec = SecondaryMotion::new(&m);
    let cam = Cam::from_eye(Vec3::new(1.0, 0.2, 0.05));
    let pts = vec![Vec3::new(0.0, -0.2, -1.4), Vec3::new(0.0, 2.4, 1.4)];
    let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32, 6.0);
    let pos_at = |t: f32| if t < 0.5 { -8.0 * t } else { -4.0 };
    let shots = [0.4f32, 0.5, 0.54, 0.58, 0.64, 0.72, 0.82, 1.0];
    let dt = 1.0 / 60.0;
    let mut t = 0.0f32;
    let mut dist = 0.0f32;
    for (f, &shot) in shots.iter().enumerate() {
        while t < shot {
            t += dt;
            dist = -pos_at(t);
            sec.update(Affine3A::from_translation(Vec3::new(0.0, 0.0, pos_at(t))), dt);
        }
        let speed = if t < 0.5 { 8.0 } else { 0.0 };
        let mut p = vec![Affine3A::IDENTITY; m.parts.len()];
        pose(&m, AnimState::Locomotion { speed, distance: dist, airborne: false }, t, &mut p);
        sec.apply(&m, &mut p);
        let x0 = (f * tile) as i32;
        let clip = frame(&mut c, x0, head as i32, tile as i32);
        let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + head as f32 };
        c.clear_depth();
        draw(&mut c, &cam, fit, &Item { model: &m, meshes: &meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN }, clip);
        text(&mut c, x0 + 4, head as i32 + tile as i32 - 11, &format!("{:.0}MS", t * 1e3), [140, 160, 150]);
    }
    c
}

pub fn run(docs: &std::path::Path) {
    let tile = 180;
    let mut rows = vec![
        swing_row("LIGHT SWORD", Form::Sword, 0.7, tile),
        swing_row("SWORD (AUTHORED)", Form::Sword, 1.0, tile),
        swing_row("HEAVY HAMMER", Form::Hammer, 2.0, tile),
        stomp_row("KAIJU STOMP AUTHORED", 1.0, tile),
        stomp_row("KAIJU STOMP HEAVY", 2.2, tile),
    ];
    let mut r = demo_recipe(Form::Biped, 1, &[Mechanic::Blade]);
    r.name = "knight".into();
    let spec = costume(&r, 0, CostumeTweaks::default());
    let hit_ts = [0.0, 0.03, 0.06, 0.1, 0.16, 0.24, 0.34, 0.5];
    for s in [0.3f32, 0.8, 1.4] {
        rows.push(costume_row(&format!("HIT REACT STRENGTH {s:.1} (HIT TRAVELS +X)"), &spec, move |t| (AnimState::HitReact { dir: Vec3::X, strength: s }, t), &hit_ts, tile, Vec3::new(0.25, 0.25, -1.0)));
    }
    rows.push(costume_row("LAUNCHED (TRAVEL +X, 10 RAD/S)", &spec, |t| (AnimState::Launched { dir: Vec3::X, spin: 10.0 }, t), &[0.0, 0.08, 0.16, 0.24, 0.32, 0.4, 0.48, 0.56], tile, Vec3::new(0.25, 0.25, -1.0)));
    rows.push(follow_row(tile));
    stack(rows).save(&docs.join("gallery_weight.png"));
}
