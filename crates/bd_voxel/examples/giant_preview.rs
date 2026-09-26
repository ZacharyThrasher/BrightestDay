//! Giant's Room preview (feature `giant`): plan, two sections, raycast shots (sunlight with shadow
//! rays, the sunbeam through the window, warm haze) and mesh statistics.
//!
//! ```text
//! cargo run -p bd_voxel --features giant --example giant_preview --release -- [seed] [--out PATH] [--compare]
//! ```
//! Default output: `docs/giant_preview.png`; every view is also written to `target/giant/preview/`.
//! Plan: height-shaded top view, void pits black, spawns red, surf lines cyan, routes white.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::giant::{self, FLOOR_Y, HX, HZ, WINDOW_Y, WINDOW_Z, generate_giant, sun_dir};
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

/// Top-down plan (north up), shaded by height; pits black.
fn plan_image(w: &VoxelWorld, info: &MapInfo, s: u32) -> RgbImage {
    let (x0, x1, z0, z1) = ((-HX / 0.5) as i32 - 1, (HX / 0.5) as i32, (-HZ / 0.5) as i32 - 1, (HZ / 0.5) as i32);
    let (nx, nz) = ((x1 - x0 + 1) as u32, (z1 - z0 + 1) as u32);
    let mut img = RgbImage::from_pixel(nx * s, nz * s, Rgb([12, 12, 16]));
    let ceil = (giant::CEIL_Y / 0.5) as i32;
    for z in z0..=z1 {
        for x in x0..=x1 {
            // Highest solid below the ceiling (skip the ceiling itself, show the fan).
            let top = (0..ceil).rev().find(|&y| w.get(IVec3::new(x, y, z)) != AIR);
            let c = match top {
                Some(y) if y as f32 * 0.5 >= FLOOR_Y - 1.0 => {
                    let h = (y as f32 * 0.5 - FLOOR_Y) / 60.0;
                    px(voxel_rgb(w, IVec3::new(x, y, z)), 0.72 + h.clamp(0.0, 1.0) * 0.7)
                }
                _ => Rgb([0, 0, 0]),
            };
            let (ix, iz) = ((x - x0) as u32, (z1 - z) as u32);
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel(ix * s + dx, iz * s + dy, c);
                }
            }
        }
    }
    let to_px = |p: Vec3| (((p.x / 0.5) as i32 - x0) * s as i32, (z1 - (p.z / 0.5) as i32) * s as i32);
    for r in &info.routes {
        for win in r.waypoints.windows(2) {
            for k in 0..=60 {
                let (a, b) = to_px(win[0].lerp(win[1], k as f32 / 60.0));
                dot(&mut img, a, b, 0, Rgb([250, 250, 250]));
            }
        }
    }
    for l in &info.surf_lines {
        for k in 0..=80 {
            let (a, b) = to_px(l.a.lerp(l.b, k as f32 / 80.0));
            dot(&mut img, a, b, 1, Rgb([60, 230, 255]));
        }
    }
    for sp in &info.spawns {
        let (a, b) = to_px(*sp);
        dot(&mut img, a, b, 5, Rgb([255, 255, 255]));
        dot(&mut img, a, b, 4, Rgb([230, 30, 40]));
    }
    img
}

/// Vertical section: `along_x` = the plane z = `at` (looking north), else x = `at` (looking east).
fn section(w: &VoxelWorld, along_x: bool, at: f32, s: u32) -> RgbImage {
    let (a0, a1) = if along_x { ((-HX / 0.5) as i32 - 8, (HX / 0.5) as i32 + 7) } else { ((-HZ / 0.5) as i32 - 8, (HZ / 0.5) as i32 + 7) };
    let (y0, y1) = (0, 159);
    let at = (at / 0.5) as i32;
    let mut img = RgbImage::from_pixel(((a1 - a0 + 1) as u32) * s, ((y1 - y0 + 1) as u32) * s, Rgb([30, 29, 24]));
    for y in y0..=y1 {
        for a in a0..=a1 {
            let v = if along_x { IVec3::new(a, y, at) } else { IVec3::new(at, y, a) };
            let c = if w.get(v) != AIR {
                px(voxel_rgb(w, v), 0.95)
            } else {
                let mut c = Rgb([40, 44, 52]);
                for d in 1..40 {
                    let q = if along_x { v + IVec3::new(0, 0, d) } else { v + IVec3::new(d, 0, 0) };
                    if w.get(q) != AIR {
                        c = px(voxel_rgb(w, q), 0.6 - d as f32 * 0.011);
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

// ---- raycast shots ---------------------------------------------------------------------------------

struct Look {
    fog: Vec3,
    fog_end: f32,
    neon: f32,
    sun: Vec3,
    sun_col: Vec3,
}

/// Is `p` inside the sunbeam prism (the window rectangle swept along −sun)? Ignores occluders.
fn in_beam(p: Vec3, sun: Vec3) -> bool {
    if p.x >= HX {
        return false;
    }
    let t = (HX - p.x) / sun.x;
    let q = p + sun * t;
    q.y > WINDOW_Y.0 && q.y < WINDOW_Y.1 && q.z > WINDOW_Z.0 && q.z < WINDOW_Z.1
}

fn shade(w: &VoxelWorld, look: &Look, eye: Vec3, dir: Vec3) -> Vec3 {
    let hit = bd_core::voxel::raycast(w, eye, dir, 260.0);
    let dist = hit.as_ref().map_or(260.0, |h| h.t);
    // Sunbeam in the air: march the view ray and add warm in-scatter where it's inside the beam.
    let mut beam = 0.0;
    let steps = 48;
    for k in 0..steps {
        let p = eye + dir * (dist * (k as f32 + 0.5) / steps as f32);
        if in_beam(p, look.sun) {
            beam += dist / steps as f32;
        }
    }
    let scatter = look.sun_col * (1.0 - (-beam * 0.012).exp()) * 0.55;
    let Some(h) = hit else { return look.fog + scatter };
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
    let sky = 0.36 + 0.14 * n.y.max(0.0) - 0.06 * (-n.y).max(0.0);
    let mut c = alb * (sky * ao);
    let ndl = n.dot(look.sun);
    if ndl > 0.0 {
        let from = h.point + n * 0.02;
        if bd_core::voxel::raycast(w, from, look.sun, 300.0).is_none() {
            c += alb * look.sun_col * (ndl * 1.25);
        }
    }
    if mdef.emissive > 0.0 {
        let k = if mdef.emissive >= 0.5 { 1.0 + look.neon * ((mdef.emissive - 0.5) * 2.0) * 1.6 } else { 1.0 };
        c = c.lerp(alb * k, mdef.emissive.min(1.0));
    }
    let f = (h.t / look.fog_end).clamp(0.0, 1.0);
    c.lerp(look.fog, 1.0 - (-3.0 * f * f).exp()) + scatter
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
        .unwrap_or_else(|| root.join("docs/giant_preview.png"));
    let parts = root.join("target/giant/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (world, info) = generate_giant(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "Giant's Room seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {} spawns, {} pruned voxels, {} anchors, {} surf lines",
        world.solid_count(),
        info.spawns.len(),
        info.pruned_voxels,
        world.anchor_hints().len(),
        info.surf_lines.len()
    );
    let (tris, ms) = mesh_stats(&world);
    println!("  mesh: {tris} triangles total ({ms:.0} ms to mesh)");
    if args.iter().any(|a| a == "--compare") {
        let (a, _) = bd_voxel::generate_arena(1);
        let (b, _) = bd_voxel::generate_backrooms(1);
        for (name, w) in [("archipelago", &a), ("backrooms", &b)] {
            let (t, _) = mesh_stats(w);
            println!("  compare: {name}: {t} triangles");
        }
    }
    for (i, s) in info.spawns.iter().enumerate() {
        println!("  spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    let p = giant::sun_patch();
    println!("  sun patch centre ({:.1}, {:.1}); cat at {:?}", p.x, p.y, giant::cat_center());

    let t = Instant::now();
    let plan = plan_image(&world, &info, 3);
    plan.save(parts.join("plan.png")).unwrap();
    let sec_x = section(&world, true, 10.0, 2);
    let sec_z = section(&world, false, -40.0, 2);
    sec_x.save(parts.join("section_ew.png")).unwrap();
    sec_z.save(parts.join("section_ns.png")).unwrap();

    let lin = |c: [f32; 3]| Vec3::new(c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2));
    let look = Look { fog: lin(info.fog_color) * 0.8, fog_end: info.fog_end, neon: info.neon_boost, sun: sun_dir(), sun_col: lin(info.sun_color) };
    let (wpx, hpx) = (640, 360);
    let f = FLOOR_Y;
    let shots: Vec<(&str, RgbImage)> = vec![
        ("carpet_to_bed", shot(&world, &look, Vec3::new(18.0, f + 1.1, 12.0), Vec3::new(-45.0, f + 12.0, 26.0), 80.0, wpx, hpx)),
        ("desk_down", shot(&world, &look, Vec3::new(24.0, giant::DESK_TOP + 1.7, -38.0), Vec3::new(-10.0, f, 10.0), 84.0, wpx, hpx)),
        ("track", shot(&world, &look, Vec3::new(-8.0, f + 2.1, giant::TRACK_S), Vec3::new(40.0, f + 1.0, giant::TRACK_S + 6.0), 80.0, wpx, hpx)),
        ("sunbeam", shot(&world, &look, Vec3::new(-30.0, f + 3.0, 4.0), Vec3::new(HX, 46.0, 0.0), 84.0, wpx, hpx)),
        ("bookshelf", shot(&world, &look, Vec3::new(-30.0, f + 8.0, -30.0), Vec3::new(-55.0, 30.0, -38.0), 80.0, wpx, hpx)),
        ("fan_overview", shot(&world, &look, Vec3::new(52.0, 72.0, 44.0), Vec3::new(-10.0, f, -10.0), 84.0, wpx, hpx)),
    ];
    for (n, s) in &shots {
        s.save(parts.join(format!("shot_{n}.png"))).unwrap();
    }

    // ---- composite ----
    let cols = 3u32;
    let rows = (shots.len() as u32).div_ceil(cols);
    let top_h = plan.height().max(sec_x.height() + 6 + sec_z.height());
    let width = (plan.width() + 6 + sec_x.width().max(sec_z.width())).max(cols * (wpx + 6));
    let height = top_h + 6 + rows * (hpx + 6);
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([16, 16, 20]));
    blit(&mut canvas, &plan, 0, 0);
    blit(&mut canvas, &sec_x, plan.width() + 6, 0);
    blit(&mut canvas, &sec_z, plan.width() + 6, sec_x.height() + 6);
    for (k, (_, s)) in shots.iter().enumerate() {
        let (cx, cy) = (k as u32 % cols, k as u32 / cols);
        blit(&mut canvas, s, cx * (wpx + 6), top_h + 6 + cy * (hpx + 6));
    }
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    canvas.save(&out).unwrap();
    println!("rendered in {:.1} s -> {} (parts in {})", t.elapsed().as_secs_f64(), out.display(), parts.display());
}
