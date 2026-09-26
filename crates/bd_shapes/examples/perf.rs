//! Timing + voxel-count table: cargo run -p bd_shapes --release --example perf
use std::time::Instant;

use bd_shapes::*;
use glam::Affine3A;

fn main() {
    println!("{:<11} {:>7} {:>5} {:>6} {:>9} {:>9} {:>8} {:>8} {:>7}", "form", "voxels", "parts", "vsize", "gen µs", "vox µs", "mesh µs", "pose µs", "proxies");
    let (mut tg, mut tv, mut tm) = (0.0, 0.0, 0.0);
    for f in ALL_FORMS {
        let r = demo_recipe(f, 1, &[]);
        let n = 20;
        let t0 = Instant::now();
        let mut spec = generate(&r);
        for _ in 1..n {
            spec = generate(&r);
        }
        let g = t0.elapsed().as_secs_f64() * 1e6 / n as f64;
        let t0 = Instant::now();
        let mut m = voxelize_capped(&spec, voxel_cap(f));
        for _ in 1..n {
            m = voxelize_capped(&spec, voxel_cap(f));
        }
        let v = t0.elapsed().as_secs_f64() * 1e6 / n as f64;
        let t0 = Instant::now();
        let mut meshes = mesh_model(&m);
        for _ in 1..n {
            meshes = mesh_model(&m);
        }
        let me = t0.elapsed().as_secs_f64() * 1e6 / n as f64;
        let mut out = vec![Affine3A::IDENTITY; m.parts.len()];
        let t0 = Instant::now();
        let k = 2000;
        for i in 0..k {
            let t = i as f32 * 0.008;
            pose_layered(&m, &[(AnimState::Locomotion { speed: 5.0, distance: t * 5.0, airborne: false }, t), (AnimState::Attack(Attack::Bite), t % 0.6)], &mut out);
        }
        let p = t0.elapsed().as_secs_f64() * 1e6 / k as f64;
        let px = collision_proxies(&m).len();
        let tris: usize = meshes.iter().map(|x| x.indices.len() / 3).sum();
        tg += g;
        tv += v;
        tm += me;
        println!("{:<11} {:>7} {:>5} {:>6.2} {:>9.1} {:>9.1} {:>8.1} {:>8.2} {:>7}  ({} tris)", format!("{f:?}"), m.voxel_count, m.parts.len(), m.voxel_size, g, v, me, p, px, tris);
    }
    println!("avg gen {:.1} µs, voxelize {:.1} µs, mesh {:.1} µs", tg / 39.0, tv / 39.0, tm / 39.0);
    extra();
}

fn time<T>(n: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let t0 = Instant::now();
    let mut v = f();
    for _ in 1..n {
        v = f();
    }
    (t0.elapsed().as_secs_f64() * 1e6 / n as f64, v)
}

/// Machine kit, costumes, transformers.
fn extra() {
    use bd_core::mechanics::{Feature::*, Form, Mechanic};
    let mut train = demo_recipe(Form::Vehicle, 7, &[Mechanic::Charge]);
    train.scale = 12.0;
    train.features = vec![Boiler, Cab, Pistons, Plow, Smokestack, Wheels];
    let (b, (_, m)) = time(20, || build_model(&train));
    let (me, meshes) = time(20, || mesh_model(&m));
    let tris: usize = meshes.iter().map(|x| x.indices.len() / 3).sum();
    let mut out = vec![Affine3A::IDENTITY; m.parts.len()];
    let (p, _) = time(5000, || pose(&m, AnimState::Locomotion { speed: 6.0, distance: 3.3, airborne: false }, 1.0, &mut out));
    println!("steam train: build {b:.0} µs, mesh {me:.0} µs, pose {p:.2} µs, {} voxels, {} parts, {tris} tris", m.voxel_count, m.parts.len());
    let mut knight = demo_recipe(Form::Biped, 1, &[Mechanic::Blade, Mechanic::Guard]);
    knight.name = "knight".into();
    knight.features = vec![Armor];
    let (c, spec) = time(50, || costume(&knight, 0, CostumeTweaks::default()));
    let (cm, cmod) = time(50, || costume_model(&spec));
    let mut out = vec![Affine3A::IDENTITY; cmod.parts.len()];
    let (cp, _) = time(5000, || pose(&cmod, AnimState::Locomotion { speed: 7.0, distance: 3.0, airborne: false }, 0.3, &mut out));
    println!("costume: dress {c:.0} µs, voxelize {cm:.0} µs, pose {cp:.2} µs, {} voxels, {} parts", cmod.voxel_count, cmod.parts.len());
    let mut car = demo_recipe(Form::Vehicle, 2, &[Mechanic::Transforms]);
    car.scale = 7.0;
    car.features = vec![Wheels, Cab];
    let (_, alt) = build_model(&car);
    let (tr, rig) = time(10, || transform_rig(&alt, 1));
    let mut out = vec![Affine3A::IDENTITY; rig.alt.parts.len()];
    let (tp, _) = time(5000, || pose_transform(&rig, 0.5, true, &mut out));
    println!("transformer: rig {tr:.0} µs, pose_transform {tp:.2} µs, {} panels", rig.panels.len() - 1);
}
