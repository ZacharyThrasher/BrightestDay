//! Valley of the End preview (feature `valley`): a top-down plan (water sheets, spawns, surf lines,
//! routes), two sections, sunlit raycast shots (the classic view of the statues and the falls,
//! statue close-ups, the gorge, the view off the lip) and mesh statistics (`--compare` adds
//! Archipelago / Miami / Orbit triangle counts).
//!
//! ```text
//! cargo run -p bd_voxel --features valley --example valley_preview --release -- [seed] [--out PATH] [--compare]
//! ```
//! Default output: `docs/valley_preview.png`; every view is also written to `target/valley/preview/`.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::valley::{self, WATER_Y, generate_valley, statue_specs};
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{ChunkMesh, MapInfo, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Vec3};
use image::{Rgb, RgbImage};

fn srgb_lin(c: f32) -> f32 {
    (c / 255.0).clamp(0.0, 1.0).powf(2.2)
}
fn lin_srgb(c: f32) -> u8 {
    (c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8
}
fn v3(c: [f32; 3]) -> Vec3 {
    Vec3::new(c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2))
}

fn blit(dst: &mut RgbImage, src: &RgbImage, ox: u32, oy: u32) {
    for (x, y, p) in src.enumerate_pixels() {
        if ox + x < dst.width() && oy + y < dst.height() {
            dst.put_pixel(ox + x, oy + y, *p);
        }
    }
}

fn dot(img: &mut RgbImage, cx: i32, cy: i32, r: i32, c: Rgb<u8>) {
    for y in -r..=r {
        for x in -r..=r {
            if x * x + y * y <= r * r {
                let (px, py) = (cx + x, cy + y);
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    img.put_pixel(px as u32, py as u32, c);
                }
            }
        }
    }
}

fn line(img: &mut RgbImage, a: (f32, f32), b: (f32, f32), c: Rgb<u8>) {
    let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) as i32).max(1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        dot(img, (a.0 + (b.0 - a.0) * t) as i32, (a.1 + (b.1 - a.1) * t) as i32, 1, c);
    }
}

fn albedo(w: &VoxelWorld, v: IVec3) -> Vec3 {
    let d = material_def(w.get(v));
    let b = 1.0 + d.jitter * signed(hash_v(v, 0x00c0_10e5)) * 1.3;
    Vec3::new(srgb_lin(d.color[0] as f32 * b), srgb_lin(d.color[1] as f32 * b), srgb_lin(d.color[2] as f32 * b))
}

/// Top-down plan (north up, 2 px per voxel column): top material with height relief, water
/// sheets, the KO water, spawns (red), surf lines (cyan), routes (yellow).
fn plan_image(w: &VoxelWorld, info: &MapInfo) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (nx, nz) = ((hi.x - lo.x) as u32, (hi.z - lo.z) as u32);
    let k = 2u32;
    let mut img = RgbImage::new(nx * k, nz * k);
    let to_px = |x: f32, z: f32| (((x / 0.5) as i32 - lo.x) as f32 * k as f32, ((hi.z - 1 - (z / 0.5) as i32) as f32) * k as f32);
    let water = v3(info.water_color);
    for iz in 0..nz as i32 {
        for ix in 0..nx as i32 {
            let (x, z) = (lo.x + ix, lo.z + iz);
            let p = Vec3::new((x as f32 + 0.5) * 0.5, 0.0, (z as f32 + 0.5) * 0.5);
            let top = w.top_solid_y(x, z);
            let ty = top.map_or(0.0, |y| (y + 1) as f32 * 0.5);
            let sheet = info.water_sheets.iter().filter(|s| s.covers(p, 0.0) && s.center.y > ty).map(|s| s.center.y).fold(None, |a: Option<f32>, y| Some(a.map_or(y, |a| a.max(y))));
            let c = if let Some(sy) = sheet {
                water * (0.8 + sy / 120.0)
            } else if let Some(y) = top {
                let v = IVec3::new(x, y, z);
                let a = albedo(w, v);
                let east = w.top_solid_y(x + 1, z).unwrap_or(0);
                let relief = 1.0 + 0.08 * (y - east).clamp(-4, 4) as f32;
                if ty < WATER_Y + 0.6 { water * 0.45 } else { a * relief * (0.55 + ty / 140.0) }
            } else {
                water * 0.45
            };
            let px = Rgb([lin_srgb(c.x), lin_srgb(c.y), lin_srgb(c.z)]);
            let (cx, cy) = (ix as u32 * k, (nz - 1 - iz as u32) * k);
            for dy in 0..k {
                for dx in 0..k {
                    img.put_pixel(cx + dx, cy + dy, px);
                }
            }
        }
    }
    for l in &info.surf_lines {
        line(&mut img, to_px(l.a.x, l.a.z), to_px(l.b.x, l.b.z), Rgb([60, 240, 255]));
    }
    for r in &info.routes {
        for p in r.waypoints.windows(2) {
            line(&mut img, to_px(p[0].x, p[0].z), to_px(p[1].x, p[1].z), Rgb([255, 220, 60]));
        }
    }
    for s in &info.spawns {
        let (x, y) = to_px(s.x, s.z);
        dot(&mut img, x as i32, y as i32, 5, Rgb([255, 40, 40]));
        dot(&mut img, x as i32, y as i32, 2, Rgb([255, 255, 255]));
    }
    img
}

/// Vertical section across x at `z` (3 px per voxel).
fn section(w: &VoxelWorld, z_m: f32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let k = 2u32;
    let z = (z_m / 0.5).floor() as i32;
    let (nx, ny) = ((hi.x - lo.x) as u32, (hi.y - lo.y) as u32);
    let mut img = RgbImage::from_pixel(nx * k, ny * k, Rgb([150, 190, 235]));
    for iy in 0..ny as i32 {
        for ix in 0..nx as i32 {
            let v = IVec3::new(lo.x + ix, lo.y + iy, z);
            let c = if w.get(v) == AIR {
                if (iy as f32 + 0.5) * 0.5 < WATER_Y { Vec3::new(0.05, 0.2, 0.3) } else { continue }
            } else {
                albedo(w, v) * 0.9
            };
            let px = Rgb([lin_srgb(c.x), lin_srgb(c.y), lin_srgb(c.z)]);
            for dy in 0..k {
                for dx in 0..k {
                    img.put_pixel(ix as u32 * k + dx, (ny - 1 - iy as u32) * k + dy, px);
                }
            }
        }
    }
    img
}

struct Look<'a> {
    info: &'a MapInfo,
    sun: Vec3,
    fog: Vec3,
    zen: Vec3,
    hor: Vec3,
    water: Vec3,
}

fn sky(look: &Look, dir: Vec3) -> Vec3 {
    let t = dir.y.clamp(0.0, 1.0).powf(0.6);
    let s = look.hor.lerp(look.zen, t);
    s + Vec3::splat(4.0) * dir.dot(look.sun).max(0.0).powf(400.0)
}

fn shade(w: &VoxelWorld, look: &Look, eye: Vec3, dir: Vec3) -> Vec3 {
    let hit = bd_core::voxel::raycast(w, eye, dir, 500.0);
    let t_vox = hit.map_or(f32::INFINITY, |h| h.t);
    // Water: the sheets (flat, y = top) and the KO water plane.
    let mut t_water = f32::INFINITY;
    if dir.y < -1e-4 {
        for s in &look.info.water_sheets {
            let t = (s.center.y - eye.y) / dir.y;
            if t > 0.0 && t < t_water && s.covers(eye + dir * t, 0.0) {
                t_water = t;
            }
        }
        let t = (WATER_Y - eye.y) / dir.y;
        if t > 0.0 && t < t_water {
            t_water = t;
        }
    }
    let fogged = |c: Vec3, t: f32| {
        let f = ((t - look.info.fog_start) / (look.info.fog_end - look.info.fog_start)).clamp(0.0, 1.0);
        c.lerp(look.fog, f * f)
    };
    if t_water < t_vox {
        let p = eye + dir * t_water;
        let fres = 0.15 + 0.6 * (1.0 + dir.y).powf(5.0);
        let refl = sky(look, Vec3::new(dir.x, -dir.y, dir.z));
        let ripple = 0.9 + 0.1 * (p.x * 1.3 + p.z * 0.7).sin() * (p.z * 1.1).sin();
        let c = look.water * 0.55 * ripple + refl * fres * 0.5;
        return fogged(c, t_water);
    }
    let Some(h) = hit else { return sky(look, dir) };
    let mdef = material_def(w.get(h.voxel));
    let alb = albedo(w, h.voxel);
    let n = h.normal.as_vec3();
    let p = h.point + n * 0.01;
    let lit = n.dot(look.sun).max(0.0);
    let shadow = if lit > 0.0 && bd_core::voxel::raycast(w, p, look.sun, 200.0).is_some() { 0.0 } else { 1.0 };
    let air = h.voxel + h.normal;
    let (t1, t2) = if n.x != 0.0 {
        (IVec3::Y, IVec3::Z)
    } else if n.y != 0.0 {
        (IVec3::X, IVec3::Z)
    } else {
        (IVec3::X, IVec3::Y)
    };
    let occ = [t1, -t1, t2, -t2].iter().filter(|&&t| w.get(air + t) != AIR).count() as f32;
    let ao = 1.0 - occ * 0.1;
    let amb = 0.30 + 0.12 * n.y;
    let mut c = alb * (amb * ao + 1.05 * lit * shadow);
    if mdef.emissive > 0.0 {
        c = c.lerp(alb * 1.1, mdef.emissive.min(1.0));
    }
    fogged(c, h.t)
}

fn shot(w: &VoxelWorld, look: &Look, eye: Vec3, target: Vec3, fov: f32, wpx: u32, hpx: u32) -> RgbImage {
    let fwd = (target - eye).normalize();
    let yaw = (-fwd.x).atan2(-fwd.z).to_degrees();
    let pitch = fwd.y.asin().to_degrees();
    println!("  in-game: --pos {:.1},{:.1},{:.1} --look {yaw:.0},{pitch:.0}", eye.x, eye.y - 1.6, eye.z);
    let right = fwd.cross(Vec3::Y).normalize();
    let up = right.cross(fwd);
    let tan = (fov.to_radians() * 0.5).tan();
    let aspect = wpx as f32 / hpx as f32;
    let rows: Vec<Vec<Rgb<u8>>> = std::thread::scope(|s| {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8) as u32;
        let chunk = hpx.div_ceil(threads);
        let hs: Vec<_> = (0..threads)
            .map(|ti| {
                s.spawn(move || {
                    let mut out = Vec::new();
                    for y in ti * chunk..((ti + 1) * chunk).min(hpx) {
                        let mut row = Vec::with_capacity(wpx as usize);
                        for x in 0..wpx {
                            let u = ((x as f32 + 0.5) / wpx as f32 * 2.0 - 1.0) * tan * aspect;
                            let v = (1.0 - (y as f32 + 0.5) / hpx as f32 * 2.0) * tan;
                            let c = shade(w, look, eye, (fwd + right * u + up * v).normalize());
                            let tm = |c: f32| c / (1.0 + c * 0.3);
                            row.push(Rgb([lin_srgb(tm(c.x)), lin_srgb(tm(c.y)), lin_srgb(tm(c.z))]));
                        }
                        out.push(row);
                    }
                    out
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    RgbImage::from_fn(wpx, hpx, |x, y| rows[y as usize][x as usize])
}

fn mesh_stats(w: &VoxelWorld) -> (usize, f64) {
    let t = Instant::now();
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(w, c, &mut mesh);
        total += mesh.triangle_count();
    }
    (total, t.elapsed().as_secs_f64() * 1e3)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = args.iter().find(|a| !a.starts_with("--")).and_then(|s| s.parse().ok()).unwrap_or(1);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("docs/valley_preview.png"));
    let parts = root.join("target/valley/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (world, info) = generate_valley(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "Valley of the End seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {} spawns, {} pruned voxels, {} anchors, {} water sheets",
        world.solid_count(),
        info.spawns.len(),
        info.pruned_voxels,
        world.anchor_hints().len(),
        info.water_sheets.len()
    );
    let (tris, ms) = mesh_stats(&world);
    println!("  mesh: {tris} triangles ({ms:.0} ms to mesh)");
    if args.iter().any(|a| a == "--compare") {
        let (a, _) = bd_voxel::generate_arena(1);
        let (m, _) = bd_voxel::generate_miami(1);
        let (o, _) = bd_voxel::generate_orbit(1);
        for (name, w) in [("archipelago", &a), ("miami", &m), ("orbit", &o)] {
            let (t, _) = mesh_stats(w);
            println!("  compare: {name}: {t} triangles");
        }
    }
    if args.iter().any(|a| a == "--faces") {
        // Exposed faces by material (≈ triangles / 2): where the budget goes.
        let mut by = vec![0usize; 256];
        let (lo, hi) = (world.voxel_min(), world.voxel_max());
        for c in world.nonempty_chunks().collect::<Vec<_>>() {
            let o = bd_voxel::chunk_origin(c);
            for y in 0..32 {
                for z in 0..32 {
                    for x in 0..32 {
                        let v = o + IVec3::new(x, y, z);
                        let m = world.get(v);
                        if m == AIR {
                            continue;
                        }
                        for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                            let q = v + d;
                            if q.cmpge(lo).all() && q.cmplt(hi).all() && world.get(q) == AIR {
                                by[m as usize] += 1;
                            }
                        }
                    }
                }
            }
        }
        let mut v: Vec<(usize, usize)> = by.iter().copied().enumerate().filter(|x| x.1 > 0).collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        for (m, n) in v {
            println!("  faces {:>8} {}", n, material_def(m as u8).name);
        }
    }
    for (i, s) in info.spawns.iter().enumerate() {
        println!("  spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    for r in &info.routes {
        println!("  route {}: {:?} anchors {:?}", r.name, r.waypoints.first().zip(r.waypoints.last()), r.anchors);
    }
    for (k, l) in info.surf_lines.iter().enumerate() {
        println!("  surf {k}: {:?} -> {:?}", l.a, l.b);
    }
    for s in statue_specs() {
        println!("  statue {:?}: head {:?}, shoulders {:?} / {:?}", s.hair, s.head_top(), s.pauldron_top(1.0), s.pauldron_top(-1.0));
    }

    let t = Instant::now();
    let plan = plan_image(&world, &info);
    plan.save(parts.join("plan.png")).unwrap();
    let sec_a = section(&world, valley::STATUE_Z);
    let sec_b = section(&world, -40.0);
    sec_a.save(parts.join("section_statues.png")).unwrap();
    sec_b.save(parts.join("section_gorge.png")).unwrap();

    let look = Look {
        info: &info,
        sun: info.sun_dir.normalize(),
        fog: v3(info.fog_color),
        zen: v3(info.sky_zenith),
        hor: v3(info.sky_horizon),
        water: v3(info.water_color),
    };
    let (wpx, hpx) = (800, 450);
    let [sw, se] = statue_specs();
    let views: Vec<(&str, Vec3, Vec3, f32)> = vec![
        ("classic", Vec3::new(0.0, 26.0, -24.0), Vec3::new(0.0, 44.0, 50.0), 72.0),
        ("classic_high", Vec3::new(0.0, 52.0, -46.0), Vec3::new(0.0, 40.0, 50.0), 70.0),
        ("statue_west", Vec3::new(-2.0, 50.0, 24.0), sw.world(0.0, 38.0, 0.0), 70.0),
        ("statue_east", Vec3::new(2.0, 50.0, 24.0), se.world(0.0, 38.0, 0.0), 70.0),
        ("gorge_downstream", Vec3::new(0.0, 36.0, 26.0), Vec3::new(0.0, 22.0, -90.0), 80.0),
        ("off_the_lip", Vec3::new(0.0, 60.5, 62.0), Vec3::new(0.0, 30.0, 10.0), 80.0),
        ("west_rim", Vec3::new(-56.0, 68.0, -40.0), Vec3::new(10.0, 40.0, 30.0), 75.0),
        ("head_perch", se.head_top() + Vec3::new(0.0, 1.7, 0.0), sw.world(0.0, 45.0, 0.0), 80.0),
        ("falls_climb", Vec3::new(13.0, 16.6, 50.0), Vec3::new(0.0, 45.0, 57.0), 80.0),
    ];
    let mut shots: Vec<(String, RgbImage)> = Vec::new();
    for (name, eye, target, fov) in views {
        shots.push((name.to_string(), shot(&world, &look, eye, target, fov, wpx, hpx)));
    }
    for (n, s) in &shots {
        s.save(parts.join(format!("shot_{n}.png"))).unwrap();
    }

    // ---- composite ---------------------------------------------------------------------------------
    let cols = 3u32;
    let rows = (shots.len() as u32).div_ceil(cols);
    let shots_w = cols * (wpx + 6);
    let right_h = rows * (hpx + 6);
    let sec_h = sec_a.height() + sec_b.height() + 12;
    let width = plan.width() + 6 + shots_w.max(sec_a.width());
    let height = plan.height().max(right_h + sec_h);
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([16, 16, 20]));
    blit(&mut canvas, &plan, 0, 0);
    let ox = plan.width() + 6;
    for (k, (_, s)) in shots.iter().enumerate() {
        let (cx, cy) = (k as u32 % cols, k as u32 / cols);
        blit(&mut canvas, s, ox + cx * (wpx + 6), cy * (hpx + 6));
    }
    blit(&mut canvas, &sec_a, ox, right_h + 6);
    blit(&mut canvas, &sec_b, ox, right_h + 12 + sec_a.height());
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    canvas.save(&out).unwrap();
    println!("rendered in {:.1} s -> {} (parts in {})", t.elapsed().as_secs_f64(), out.display(), parts.display());
}
