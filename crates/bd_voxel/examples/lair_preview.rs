//! Turtle Lair preview (feature `lair`): plans of the sewer and street levels, two sections,
//! raycast shots (pit under the light shaft, dojo, a sewer tunnel with water, skate hall, the
//! chasm, the rooftops) and mesh statistics (vs Archipelago / Miami / Orbit with `--compare`).
//!
//! ```text
//! cargo run -p bd_voxel --features lair --example lair_preview --release -- [seed] [--out PATH] [--compare]
//! ```
//! Default output: `docs/lair_preview.png`; every view is also written to `target/lair/preview/`.
//! Plans: walls dark, floors by material, void black, spawns red, props outlined yellow, surf
//! lines magenta, routes cyan.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::lair::{self, CHASM, EDGE, FLOOR, STREET, generate_lair, props_for};
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

/// Top-down plan of the storey whose floor is at `floor` (knee height slice + what's below).
fn plan_image(w: &VoxelWorld, info: &MapInfo, props: &[lair::props::Placed], floor: f32, s: u32) -> RgbImage {
    let (x0, x1) = ((-EDGE / 0.5) as i32, (EDGE / 0.5) as i32 - 1);
    let n = (x1 - x0 + 1) as u32;
    let mut img = RgbImage::from_pixel(n * s, n * s, Rgb([0, 0, 0]));
    let yf = (floor / 0.5) as i32;
    for z in x0..=x1 {
        for x in x0..=x1 {
            let knee = IVec3::new(x, yf + 2, z);
            let c = if w.get(knee) != AIR {
                let head = w.get(IVec3::new(x, yf + 5, z)) != AIR;
                px(voxel_rgb(w, knee), if head { 0.4 } else { 0.8 })
            } else {
                match (0..=yf + 1).rev().find(|&y| w.get(IVec3::new(x, y, z)) != AIR) {
                    Some(y) if y >= yf - 4 => px(voxel_rgb(w, IVec3::new(x, y, z)), if y >= yf - 1 { 1.05 } else { 0.8 }),
                    Some(_) => Rgb([50, 58, 80]),
                    None => Rgb([0, 0, 0]),
                }
            };
            let (ix, iz) = ((x - x0) as u32, (x1 - z) as u32);
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel(ix * s + dx, iz * s + dy, c);
                }
            }
        }
    }
    let to_px = |p: Vec3| (((p.x / 0.5).floor() as i32 - x0) * s as i32, ((x1 - (p.z / 0.5).floor() as i32) * s as i32));
    let near = |y: f32| y > floor - 3.0 && y < floor + 8.0;
    for pr in props {
        if !near(pr.min.y) {
            continue;
        }
        let (a, b) = (to_px(Vec3::new(pr.min.x, 0.0, pr.max.z)), to_px(Vec3::new(pr.max.x, 0.0, pr.min.z)));
        for xx in a.0..=b.0 {
            for yy in [a.1, b.1] {
                dot(&mut img, xx, yy, 0, Rgb([250, 220, 60]));
            }
        }
        for yy in a.1..=b.1 {
            for xx in [a.0, b.0] {
                dot(&mut img, xx, yy, 0, Rgb([250, 220, 60]));
            }
        }
    }
    for l in &info.surf_lines {
        if near(l.a.y.min(l.b.y)) || near(l.a.y.max(l.b.y)) {
            for k in 0..=100 {
                let (a, b) = to_px(l.a.lerp(l.b, k as f32 / 100.0));
                dot(&mut img, a, b, 1, Rgb([255, 60, 220]));
            }
        }
    }
    for r in &info.routes {
        if r.waypoints.iter().all(|p| near(p.y)) {
            for win in r.waypoints.windows(2) {
                for k in 0..=100 {
                    let (a, b) = to_px(win[0].lerp(win[1], k as f32 / 100.0));
                    dot(&mut img, a, b, 1, Rgb([60, 230, 255]));
                }
            }
        }
    }
    for sp in &info.spawns {
        if near(sp.y) {
            let (a, b) = to_px(*sp);
            dot(&mut img, a, b, 5, Rgb([255, 255, 255]));
            dot(&mut img, a, b, 4, Rgb([230, 30, 40]));
        }
    }
    img
}

/// Vertical section: `along_x` = the plane z = `at` (looking north), else x = `at` (looking east).
fn section(w: &VoxelWorld, along_x: bool, at: f32, s: u32) -> RgbImage {
    let (a0, a1) = ((-EDGE / 0.5) as i32, (EDGE / 0.5) as i32 - 1);
    let (y0, y1) = (0, 127);
    let at = (at / 0.5).floor() as i32;
    let mut img = RgbImage::from_pixel(((a1 - a0 + 1) as u32) * s, ((y1 - y0 + 1) as u32) * s, Rgb([8, 14, 24]));
    for y in y0..=y1 {
        for a in a0..=a1 {
            let v = if along_x { IVec3::new(a, y, at) } else { IVec3::new(at, y, a) };
            let c = if w.get(v) != AIR {
                px(voxel_rgb(w, v), 0.95)
            } else {
                let mut c = Rgb([10, 18, 30]);
                for d in 1..16 {
                    let q = if along_x { v + IVec3::new(0, 0, d) } else { v + IVec3::new(d, 0, 0) };
                    if w.get(q) != AIR {
                        c = px(voxel_rgb(w, q), 0.55 - d as f32 * 0.025);
                        break;
                    }
                }
                c
            };
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel((a - a0) as u32 * s + dx, (y1 - y) as u32 * s + dy, c);
                }
            }
        }
    }
    img
}

// ---- raycast shots ------------------------------------------------------------------------------

struct Look {
    fog: Vec3,
    fog_end: f32,
    neon: f32,
    ambient: f32,
    sun: Vec3,
}

fn shade(w: &VoxelWorld, look: &Look, eye: Vec3, dir: Vec3) -> Vec3 {
    let Some(h) = bd_core::voxel::raycast(w, eye, dir, 200.0) else {
        // Night sky: a dark gradient.
        let t = dir.y.max(0.0);
        return look.fog.lerp(Vec3::new(0.004, 0.008, 0.03), t.sqrt());
    };
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
    let ao = 1.0 - occ * 0.09;
    // Cool ambient everywhere; moonlight only where the sky is visible along the moon direction.
    let p = h.point + n * 0.05;
    let lit = n.dot(look.sun).max(0.0) * if bd_core::voxel::raycast(w, p, look.sun, 120.0).is_none() { 1.0 } else { 0.0 };
    let amb = Vec3::new(0.30, 0.38, 0.55) * look.ambient * (0.8 + 0.2 * n.y.max(0.0));
    let mut c = alb * (amb * ao + Vec3::new(0.45, 0.55, 0.8) * lit * 0.6);
    if mdef.emissive > 0.0 {
        let k = if mdef.emissive >= 0.5 { 1.0 + look.neon * ((mdef.emissive - 0.5) * 2.0) * 1.6 } else { 1.0 };
        c = c.lerp(alb * k, mdef.emissive.min(1.0));
    }
    let f = (h.t / look.fog_end).clamp(0.0, 1.0);
    c.lerp(look.fog, 1.0 - (-3.0 * f * f).exp())
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

fn mesh_tris(w: &VoxelWorld) -> (usize, f64) {
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
        .unwrap_or_else(|| root.join("docs/lair_preview.png"));
    let parts = root.join("target/lair/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (world, info) = generate_lair(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    let props = props_for(seed);
    println!(
        "Turtle Lair seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {} spawns, {} props, {} pruned voxels, {} anchors",
        world.solid_count(),
        info.spawns.len(),
        props.len(),
        info.pruned_voxels,
        world.anchor_hints().len()
    );
    let (tris, ms) = mesh_tris(&world);
    println!("  mesh: {tris} triangles total ({ms:.0} ms to mesh)");
    if args.iter().any(|a| a == "--compare") {
        let (a, _) = bd_voxel::generate_arena(1);
        let (m, _) = bd_voxel::generate_miami(1);
        let (o, _) = bd_voxel::generate_orbit(1);
        for (name, w) in [("archipelago", &a), ("miami", &m), ("orbit", &o)] {
            println!("  compare: {name}: {} triangles", mesh_tris(w).0);
        }
    }
    for (i, s) in info.spawns.iter().enumerate() {
        println!("  spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    for r in &info.routes {
        println!("  route {}: {:?} anchors {:?}", r.name, r.waypoints, r.anchors);
    }
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for p in &props {
        match counts.iter_mut().find(|c| c.0 == p.name) {
            Some(c) => c.1 += 1,
            None => counts.push((p.name, 1)),
        }
    }
    println!("  props: {}", counts.iter().map(|(n, k)| format!("{n} ×{k}")).collect::<Vec<_>>().join(", "));

    // ---- images -----------------------------------------------------------------------------------
    let t = Instant::now();
    let plan_sewer = plan_image(&world, &info, &props, FLOOR, 3);
    let plan_street = plan_image(&world, &info, &props, STREET, 3);
    plan_sewer.save(parts.join("plan_sewers.png")).unwrap();
    plan_street.save(parts.join("plan_street.png")).unwrap();
    let sec_ew = section(&world, true, 5.0, 2);
    let sec_ns = section(&world, false, 0.0, 2);
    sec_ew.save(parts.join("section_ew.png")).unwrap();
    sec_ns.save(parts.join("section_ns.png")).unwrap();

    let lin = |c: [f32; 3]| Vec3::new(c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2));
    let look = Look { fog: lin(info.fog_color), fog_end: info.fog_end, neon: info.neon_boost, ambient: info.ambient, sun: info.sun_dir };
    let (wpx, hpx) = (640, 360);
    let e = 1.6;
    let views: [(&str, Vec3, Vec3, f32); 8] = [
        ("pit_light_shaft", Vec3::new(-12.0, FLOOR + e + 1.0, -3.0), Vec3::new(1.0, 17.0, 8.0), 84.0),
        ("dojo", Vec3::new(-5.0, FLOOR + e + 0.5, 12.0), Vec3::new(-18.0, FLOOR + 2.0, 22.0), 80.0),
        ("arcade_lab", Vec3::new(-2.0, FLOOR + e, 0.0), Vec3::new(-22.0, FLOOR + 3.5, 8.0), 80.0),
        ("sewer_tunnel", Vec3::new(-10.0, FLOOR + e, lair::T_N_Z - 2.5), Vec3::new(30.0, FLOOR + 0.5, lair::T_N_Z), 78.0),
        ("skate_hall", Vec3::new(-23.0, FLOOR + 4.5, -20.0), Vec3::new(12.0, FLOOR + 1.0, -21.0), 84.0),
        ("chasm", Vec3::new(42.0, FLOOR + e, 3.5), Vec3::new(42.0, FLOOR + 2.0, 40.0), 84.0),
        ("rooftops", Vec3::new(-26.0, 46.0 + e, 40.0), Vec3::new(20.0, 42.0, -20.0), 84.0),
        ("green_tunnel", Vec3::new(-39.0, FLOOR + e, 4.0), Vec3::new(-22.0, FLOOR + 1.5, 5.5), 80.0),
    ];
    let mut shots: Vec<(String, RgbImage)> = Vec::new();
    for (name, eye, tgt, fov) in views {
        println!(" {name}:");
        shots.push((name.to_string(), shot(&world, &look, eye, tgt, fov, wpx, hpx)));
    }
    for (n, s) in &shots {
        s.save(parts.join(format!("shot_{n}.png"))).unwrap();
    }

    // ---- composite: plans | stacked sections, then the shots ------------------------------------
    let top_h = plan_sewer.height().max(sec_ew.height() + 6 + sec_ns.height());
    let cols = 4u32;
    let rows = (shots.len() as u32).div_ceil(cols);
    let width = (plan_sewer.width() * 2 + 12 + sec_ew.width()).max(cols * (wpx + 6));
    let height = top_h + 6 + rows * (hpx + 6);
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([14, 16, 22]));
    blit(&mut canvas, &plan_sewer, 0, 0);
    blit(&mut canvas, &plan_street, plan_sewer.width() + 6, 0);
    let sx = plan_sewer.width() * 2 + 12;
    blit(&mut canvas, &sec_ew, sx, 0);
    blit(&mut canvas, &sec_ns, sx, sec_ew.height() + 6);
    for (k, (_, s)) in shots.iter().enumerate() {
        let (cx, cy) = (k as u32 % cols, k as u32 / cols);
        blit(&mut canvas, s, cx * (wpx + 6), top_h + 6 + cy * (hpx + 6));
    }
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    canvas.save(&out).unwrap();
    let _ = CHASM;
    println!("rendered in {:.1} s -> {} (parts in {})", t.elapsed().as_secs_f64(), out.display(), parts.display());
}
