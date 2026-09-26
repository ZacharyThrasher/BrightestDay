//! Ember preview (feature `ember`): a shaded top-down map (spawns, routes, surf lines), two sections
//! and raycast shots, plus mesh statistics (`--compare` adds Archipelago / Miami / Orbit).
//!
//! ```text
//! cargo run -p bd_voxel --features ember --example ember_preview --release -- [seed] [--out PATH] [--compare]
//! ```
//! Default output: `docs/ember_preview.png`; every view is also written to `target/ember/preview/`.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::ember::{self, generate_ember};
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{ChunkMesh, MapInfo, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Vec3};
use image::{Rgb, RgbImage};

fn srgb_lin(c: f32) -> f32 {
    (c / 255.0).powf(2.2)
}
fn lin_srgb(c: f32) -> u8 {
    (c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8
}

fn voxel_rgb(w: &VoxelWorld, v: IVec3) -> [f32; 3] {
    let d = material_def(w.get(v));
    let b = 1.0 + d.jitter * signed(hash_v(v, 0x00c0_10e5)) * 1.3;
    [d.color[0] as f32 * b, d.color[1] as f32 * b, d.color[2] as f32 * b]
}

fn px(c: [f32; 3], k: f32) -> Rgb<u8> {
    Rgb([(c[0] * k).clamp(0.0, 255.0) as u8, (c[1] * k).clamp(0.0, 255.0) as u8, (c[2] * k).clamp(0.0, 255.0) as u8])
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

/// Top-down map (north up), hill-shaded by height; routes cyan, surf lines magenta, spawns red.
fn plan_image(w: &VoxelWorld, info: &MapInfo, s: u32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (nx, nz) = ((hi.x - lo.x) as u32, (hi.z - lo.z) as u32);
    let mut img = RgbImage::from_pixel(nx * s, nz * s, Rgb([0, 0, 0]));
    let top = |x: i32, z: i32| w.top_solid_y(x, z).unwrap_or(0);
    for z in lo.z..hi.z {
        for x in lo.x..hi.x {
            let y = top(x, z);
            let v = IVec3::new(x, y, z);
            let d = material_def(w.get(v));
            let slope = (top(x - 1, z) - y) as f32 * 0.08 + (top(x, z + 1) - y) as f32 * 0.08;
            let k = if d.emissive >= 0.5 { 1.0 } else { (0.55 + y as f32 * 0.006 - slope).clamp(0.25, 1.4) };
            let c = px(voxel_rgb(w, v), k);
            let (ix, iz) = ((x - lo.x) as u32, (hi.z - 1 - z) as u32);
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel(ix * s + dx, iz * s + dy, c);
                }
            }
        }
    }
    let to_px = |p: Vec3| (((p.x / 0.5) as i32 - lo.x) * s as i32, ((hi.z - 1 - (p.z / 0.5) as i32) * s as i32));
    for r in &info.routes {
        for win in r.waypoints.windows(2) {
            for k in 0..=120 {
                let (a, b) = to_px(win[0].lerp(win[1], k as f32 / 120.0));
                dot(&mut img, a, b, 1, Rgb([60, 230, 255]));
            }
        }
    }
    for l in &info.surf_lines {
        for k in 0..=160 {
            let (a, b) = to_px(l.a.lerp(l.b, k as f32 / 160.0));
            dot(&mut img, a, b, 2, Rgb([255, 80, 230]));
        }
    }
    for sp in &info.spawns {
        let (a, b) = to_px(*sp);
        dot(&mut img, a, b, 6, Rgb([255, 255, 255]));
        dot(&mut img, a, b, 5, Rgb([230, 30, 40]));
    }
    img
}

/// Vertical section: `along_x` = the plane z = `at` (looking north), else x = `at` (looking east).
fn section(w: &VoxelWorld, along_x: bool, at: i32, s: u32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (a0, a1) = if along_x { (lo.x, hi.x - 1) } else { (lo.z, hi.z - 1) };
    let (y0, y1) = (0, 139);
    let mut img = RgbImage::from_pixel(((a1 - a0 + 1) as u32) * s, ((y1 - y0 + 1) as u32) * s, Rgb([60, 22, 16]));
    for y in y0..=y1 {
        for a in a0..=a1 {
            let v = if along_x { IVec3::new(a, y, at) } else { IVec3::new(at, y, a) };
            let c = if w.get(v) != AIR {
                px(voxel_rgb(w, v), 0.95)
            } else {
                let mut c = Rgb([70, 28, 20]);
                for d in 1..16 {
                    let q = if along_x { v + IVec3::new(0, 0, d) } else { v + IVec3::new(d, 0, 0) };
                    if w.get(q) != AIR {
                        let e = material_def(w.get(q)).emissive;
                        c = px(voxel_rgb(w, q), (0.55 - d as f32 * 0.025).max(e));
                        break;
                    }
                }
                c
            };
            let col = if along_x { a - a0 } else { a1 - a };
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel(col as u32 * s + dx, (y1 - y) as u32 * s + dy, c);
                }
            }
        }
    }
    img
}

struct Look {
    fog: Vec3,
    zenith: Vec3,
    horizon: Vec3,
    sun: Vec3,
    fog_start: f32,
    fog_end: f32,
    neon: f32,
}

fn sky(look: &Look, dir: Vec3) -> Vec3 {
    let t = dir.y.clamp(0.0, 1.0).sqrt();
    look.horizon.lerp(look.zenith, t)
}

fn shade(w: &VoxelWorld, look: &Look, eye: Vec3, dir: Vec3) -> Vec3 {
    let Some(h) = bd_core::voxel::raycast(w, eye, dir, 300.0) else { return sky(look, dir) };
    let mdef = material_def(w.get(h.voxel));
    let b = 1.0 + mdef.jitter * signed(hash_v(h.voxel, 0x00c0_10e5)) * 1.3;
    let alb = Vec3::new(srgb_lin(mdef.color[0] as f32 * b), srgb_lin(mdef.color[1] as f32 * b), srgb_lin(mdef.color[2] as f32 * b));
    let n = h.normal.as_vec3();
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
    let p = eye + dir * h.t;
    let lit = bd_core::voxel::raycast(w, p + n * 0.05, look.sun, 120.0).is_none();
    let diffuse = n.dot(look.sun).max(0.0) * if lit { 1.0 } else { 0.0 };
    // Warm lava bounce from below on downward / side faces near the floor.
    let under = (1.0 - (p.y / 14.0).clamp(0.0, 1.0)) * (0.35 + 0.35 * (-n.y).max(0.0));
    let mut c = alb * (0.28 * ao + 0.7 * diffuse) + Vec3::new(0.9, 0.25, 0.06) * alb * under;
    if mdef.emissive > 0.0 {
        let k = if mdef.emissive >= 0.5 { 1.0 + look.neon * ((mdef.emissive - 0.5) * 2.0) * 1.6 } else { 1.0 };
        c = c.lerp(alb * k, mdef.emissive.min(1.0));
    }
    let f = ((h.t - look.fog_start) / (look.fog_end - look.fog_start)).clamp(0.0, 1.0);
    c.lerp(look.fog, 1.0 - (-2.5 * f * f).exp())
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
                            let tm = |c: f32| c / (1.0 + c * 0.35);
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

fn triangles(w: &VoxelWorld) -> (usize, f64) {
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
        .unwrap_or_else(|| root.join("docs/ember_preview.png"));
    let parts = root.join("target/ember/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (world, info) = generate_ember(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "Ember seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {} spawns, {} pruned voxels, {} anchors",
        world.solid_count(),
        info.spawns.len(),
        info.pruned_voxels,
        world.anchor_hints().len()
    );
    let (tris, ms) = triangles(&world);
    println!("  mesh: {tris} triangles ({ms:.0} ms to mesh)");
    if args.iter().any(|a| a == "--compare") {
        let (a, _) = bd_voxel::generate_arena(1);
        let (m, _) = bd_voxel::generate_miami(1);
        let (o, _) = bd_voxel::generate_orbit(1);
        for (name, w) in [("archipelago", &a), ("miami", &m), ("orbit", &o)] {
            println!("  compare: {name}: {} triangles", triangles(w).0);
        }
    }
    for (i, s) in info.spawns.iter().enumerate() {
        println!("  spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    for r in &info.routes {
        println!("  route {}: {:?}", r.name, r.waypoints);
    }

    let t = Instant::now();
    let plan = plan_image(&world, &info, 2);
    plan.save(parts.join("plan.png")).unwrap();
    let sec_ns = section(&world, false, 1, 2);
    let sec_ew = section(&world, true, (ember::RIVER_Z / 0.5) as i32, 2);
    sec_ns.save(parts.join("section_ns.png")).unwrap();
    sec_ew.save(parts.join("section_ew.png")).unwrap();

    let lin = |c: [f32; 3]| Vec3::new(c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2));
    let look = Look {
        fog: lin(info.fog_color),
        zenith: lin(info.sky_zenith),
        horizon: lin(info.sky_horizon),
        sun: info.sun_dir,
        fog_start: info.fog_start,
        fog_end: info.fog_end,
        neon: info.neon_boost,
    };
    let (wpx, hpx) = (640, 360);
    let f = ember::FORT;
    let views: [(&str, Vec3, Vec3, f32); 7] = [
        ("plain_to_fortress", Vec3::new(-14.0, ember::SP_TOP + 1.7, -60.0), Vec3::new(f.x, 36.0, f.y), 64.0),
        ("facility_from_bank", Vec3::new(-34.0, ember::NB_TOP + 2.2, 20.0), Vec3::new(0.0, 15.0, ember::RIVER_Z), 74.0),
        ("lavafall", Vec3::new(4.0, ember::NB_TOP + 1.7, 20.0), Vec3::new(0.0, 22.0, 44.0), 76.0),
        ("ridge_overlook", Vec3::new(-44.0, ember::RIDGE_TOP + 1.7, 58.0), Vec3::new(0.0, 8.0, -12.0), 76.0),
        ("surf_east", ember::SURF_EAST.a + Vec3::new(-3.0, 1.0, 2.0), ember::SURF_EAST.b, 80.0),
        ("crag_to_plain", Vec3::new(-76.0, ember::WC_TOP + 1.7, 14.0), Vec3::new(-20.0, 8.0, -40.0), 76.0),
        ("sw_spawn", info.spawns[4] + Vec3::Y * 1.6, info.center, 70.0),
    ];
    let mut shots: Vec<RgbImage> = Vec::new();
    for (name, eye, tgt, fov) in views {
        println!("shot {name}");
        let s = shot(&world, &look, eye, tgt, fov, wpx, hpx);
        s.save(parts.join(format!("shot_{name}.png"))).unwrap();
        shots.push(s);
    }

    // Composite: plan (left), sections under it; shots in a 2-wide grid on the right.
    let cols = 2u32;
    let gw = cols * (wpx + 6);
    let rows = (shots.len() as u32).div_ceil(cols);
    let left_h = plan.height() + 6 + sec_ns.height() + 6 + sec_ew.height();
    let width = plan.width().max(sec_ns.width()) + 6 + gw;
    let height = left_h.max(rows * (hpx + 6));
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([16, 12, 12]));
    blit(&mut canvas, &plan, 0, 0);
    blit(&mut canvas, &sec_ns, 0, plan.height() + 6);
    blit(&mut canvas, &sec_ew, 0, plan.height() + 6 + sec_ns.height() + 6);
    let ox = plan.width().max(sec_ns.width()) + 6;
    for (k, s) in shots.iter().enumerate() {
        let (cx, cy) = (k as u32 % cols, k as u32 / cols);
        blit(&mut canvas, s, ox + cx * (wpx + 6), cy * (hpx + 6));
    }
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    canvas.save(&out).unwrap();
    println!("rendered in {:.1} s -> {} (parts in {})", t.elapsed().as_secs_f64(), out.display(), parts.display());
}
