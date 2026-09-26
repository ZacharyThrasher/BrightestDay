//! High Roller preview (feature `casino`): floor / balcony / roof plans, an east–west section,
//! raycast shots (the wheel from the balcony, the floor, the roof deck, a slot machine) and mesh
//! statistics (with `--compare`: vs Archipelago / Miami / Backrooms).
//!
//! ```text
//! cargo run -p bd_voxel --features casino --example casino_preview --release -- [seed] [--out PATH] [--compare]
//! ```
//! Default output: `docs/casino_preview.png`; every view is also written to `target/casino/preview/`.
//! Plans: height-shaded materials, spawns red, big slot levers yellow, the surf halo magenta,
//! routes cyan, chandelier hubs white.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::casino::{self, CEIL_Y, FLOOR_Y, MEZZ_Y, ROOF_Y, generate_casino};
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

/// Top-down plan: the highest solid at or below `from_y` in each column (north up), shaded by
/// height relative to `level_y`.
fn plan(w: &VoxelWorld, info: &MapInfo, from_y: f32, level_y: f32, half: f32, s: u32) -> RgbImage {
    let (x0, x1) = ((-half / 0.5) as i32, (half / 0.5) as i32);
    let n = (x1 - x0) as u32;
    let mut img = RgbImage::from_pixel(n * s, n * s, Rgb([6, 4, 12]));
    let ytop = (from_y / 0.5) as i32;
    for z in x0..x1 {
        for x in x0..x1 {
            let hit = (0..=ytop).rev().find(|&y| w.get(IVec3::new(x, y, z)) != AIR);
            let c = match hit {
                Some(y) => {
                    let h = (y + 1) as f32 * 0.5;
                    let k = (0.75 + (h - level_y) * 0.05).clamp(0.35, 1.25);
                    px(voxel_rgb(w, IVec3::new(x, y, z)), k)
                }
                None => Rgb([0, 0, 0]),
            };
            let (ix, iz) = ((x - x0) as u32, (x1 - 1 - z) as u32);
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel(ix * s + dx, iz * s + dy, c);
                }
            }
        }
    }
    let to_px = |p: Vec3| (((p.x / 0.5) as i32 - x0) * s as i32, ((x1 - 1 - (p.z / 0.5) as i32) * s as i32));
    for l in &info.surf_lines {
        for k in 0..=40 {
            let (a, b) = to_px(l.a.lerp(l.b, k as f32 / 40.0));
            dot(&mut img, a, b, 1, Rgb([255, 60, 220]));
        }
    }
    for r in &info.routes {
        if r.name == "roulette_surf" {
            continue;
        }
        for win in r.waypoints.windows(2) {
            for k in 0..=120 {
                let (a, b) = to_px(win[0].lerp(win[1], k as f32 / 120.0));
                dot(&mut img, a, b, 1, Rgb([60, 230, 255]));
            }
        }
        for an in &r.anchors {
            let (a, b) = to_px(*an);
            dot(&mut img, a, b, 3, Rgb([255, 255, 255]));
        }
    }
    for m in &info.interactables.slots {
        let (a, b) = to_px(m.lever);
        dot(&mut img, a, b, 4, Rgb([255, 230, 40]));
    }
    for sp in &info.spawns {
        if (sp.y - level_y).abs() < 4.5 {
            let (a, b) = to_px(*sp);
            dot(&mut img, a, b, 5, Rgb([255, 255, 255]));
            dot(&mut img, a, b, 4, Rgb([230, 30, 40]));
        }
    }
    img
}

/// Vertical section through z = `at` (looking north).
fn section(w: &VoxelWorld, at: i32, s: u32) -> RgbImage {
    let (a0, a1) = (-112, 112);
    let (y0, y1) = (66, 150);
    let mut img = RgbImage::from_pixel(((a1 - a0) as u32) * s, ((y1 - y0) as u32) * s, Rgb([14, 8, 22]));
    for y in y0..y1 {
        for a in a0..a1 {
            let v = IVec3::new(a, y, at);
            let c = if w.get(v) != AIR {
                px(voxel_rgb(w, v), 0.95)
            } else {
                let mut c = Rgb([22, 14, 30]);
                for d in 1..16 {
                    let q = v + IVec3::new(0, 0, d);
                    if w.get(q) != AIR {
                        c = px(voxel_rgb(w, q), 0.55 - d as f32 * 0.025);
                        break;
                    }
                }
                c
            };
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel((a - a0) as u32 * s + dx, (y1 - 1 - y) as u32 * s + dy, c);
                }
            }
        }
    }
    img
}

struct Look {
    fog: Vec3,
    fog_end: f32,
    neon: f32,
    sky: Vec3,
}

fn shade(w: &VoxelWorld, look: &Look, eye: Vec3, dir: Vec3) -> Vec3 {
    let Some(h) = bd_core::voxel::raycast(w, eye, dir, 200.0) else { return look.sky };
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
    let face = 0.78 + 0.16 * n.y.max(0.0) - 0.1 * (-n.y).max(0.0);
    // Warm interior ambient.
    let mut c = alb * Vec3::new(0.66, 0.6, 0.52) * (face * ao);
    if mdef.emissive > 0.0 {
        let k = if mdef.emissive >= 0.5 { 1.0 + look.neon * ((mdef.emissive - 0.5) * 2.0) * 1.6 } else { 1.0 + mdef.emissive };
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

/// Triangles: total and binned (hall interior / roof / outside the building).
fn mesh_stats(w: &VoxelWorld, bins_on: bool) -> (usize, [usize; 4], f64) {
    let t = Instant::now();
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    let mut bins = [0usize; 4];
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(w, c, &mut mesh);
        total += mesh.triangle_count();
        if bins_on {
            for tri in mesh.indices.as_chunks::<3>().0 {
                let (a, b, cc) = (mesh.positions[tri[0] as usize], mesh.positions[tri[1] as usize], mesh.positions[tri[2] as usize]);
                let n = mesh.normals[tri[0] as usize];
                let p = Vec3::new(
                    (a[0] + b[0] + cc[0]) / 3.0 + n[0] * 0.1,
                    (a[1] + b[1] + cc[1]) / 3.0 + n[1] * 0.1,
                    (a[2] + b[2] + cc[2]) / 3.0 + n[2] * 0.1,
                );
                let inside = p.x.abs() < 51.0 && p.z.abs() < 45.0;
                let bin = if !inside {
                    3
                } else if p.y < FLOOR_Y - 3.0 {
                    3
                } else if p.y < MEZZ_Y - 1.0 {
                    0
                } else if p.y < CEIL_Y {
                    1
                } else {
                    2
                };
                bins[bin] += 1;
            }
        }
    }
    (total, bins, t.elapsed().as_secs_f64() * 1e3)
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
        .unwrap_or_else(|| root.join("docs/casino_preview.png"));
    let parts = root.join("target/casino/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (world, info) = generate_casino(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "High Roller seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {} spawns, {} pruned voxels, {} anchors, {} surf segments, {} slot machines",
        world.solid_count(),
        info.spawns.len(),
        info.pruned_voxels,
        world.anchor_hints().len(),
        info.surf_lines.len(),
        info.interactables.slots.len()
    );
    let (tris, bins, ms) = mesh_stats(&world, true);
    println!("  mesh: {tris} triangles total ({ms:.0} ms to mesh)");
    println!("    hall floor band: {}  balcony band: {}  roof: {}  outside/below: {}", bins[0], bins[1], bins[2], bins[3]);
    if args.iter().any(|a| a == "--compare") {
        let (a, _) = bd_voxel::generate_arena(1);
        let (m, _) = bd_voxel::generate_miami(1);
        for (name, w) in [("archipelago", &a), ("miami", &m)] {
            let (t, _, _) = mesh_stats(w, false);
            println!("  compare: {name}: {t} triangles");
        }
    }
    for (i, s) in info.spawns.iter().enumerate() {
        println!("  spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    for (i, m) in info.interactables.slots.iter().enumerate() {
        println!("  slot {i}: lever {:.2?} reels {:.2?} facing {:.1?}", m.lever, m.reels, m.facing);
    }
    for r in &info.routes {
        println!("  route {}: {} waypoints, anchors {:?}", r.name, r.waypoints.len(), r.anchors);
    }

    let floor = plan(&world, &info, FLOOR_Y + 3.0, FLOOR_Y, 56.0, 2);
    let balc = plan(&world, &info, MEZZ_Y + 3.0, MEZZ_Y, 56.0, 2);
    let roof = plan(&world, &info, 84.0, ROOF_Y, 88.0, 1);
    let sec = section(&world, 0, 3);
    floor.save(parts.join("plan_floor.png")).unwrap();
    balc.save(parts.join("plan_balcony.png")).unwrap();
    roof.save(parts.join("plan_roof.png")).unwrap();
    sec.save(parts.join("section_ew.png")).unwrap();

    let lin = |c: [f32; 3]| Vec3::new(c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2));
    let look = Look { fog: lin(info.fog_color), fog_end: info.fog_end, neon: info.neon_boost, sky: lin(info.sky_horizon) };
    let (wpx, hpx) = (640, 360);
    let mut shots: Vec<(String, RgbImage)> = vec![
        ("wheel_from_balcony".into(), shot(&world, &look, Vec3::new(-8.0, MEZZ_Y + 1.7, 38.0), Vec3::new(0.0, casino::POCKET_Y, 0.0), 80.0, wpx, hpx)),
        ("floor_aisle".into(), shot(&world, &look, Vec3::new(-46.0, FLOOR_Y + 1.7, 30.0), Vec3::new(10.0, FLOOR_Y + 3.0, 10.0), 80.0, wpx, hpx)),
        ("in_the_bowl".into(), shot(&world, &look, Vec3::new(14.0, casino::POCKET_Y + 1.7, -6.0), Vec3::new(-20.0, FLOOR_Y + 4.0, 10.0), 84.0, wpx, hpx)),
        ("roof_deck".into(), shot(&world, &look, Vec3::new(46.0, ROOF_Y + 1.7, -30.0), Vec3::new(0.0, ROOF_Y + 4.0, 20.0), 80.0, wpx, hpx)),
    ];
    if let Some(m) = info.interactables.slots.first() {
        let eye = m.reels + m.facing * 7.0 + Vec3::new(0.0, -0.9, 0.0) + Vec3::new(m.facing.z, 0.0, -m.facing.x) * -2.0;
        shots.push(("slot_machine".into(), shot(&world, &look, eye, m.reels, 70.0, wpx, hpx)));
    }
    shots.push(("grand_stair".into(), shot(&world, &look, Vec3::new(-20.0, FLOOR_Y + 1.7, -12.0), Vec3::new(-43.0, MEZZ_Y, 0.0), 80.0, wpx, hpx)));
    for (n, s) in &shots {
        s.save(parts.join(format!("shot_{n}.png"))).unwrap();
    }

    // Composite: plans on top, section, shots grid.
    let cols = 3u32;
    let rows = (shots.len() as u32).div_ceil(cols);
    let top_w = floor.width() + balc.width() + roof.width() + 12;
    let top_h = floor.height().max(roof.height());
    let width = top_w.max(sec.width()).max(cols * (wpx + 6));
    let height = top_h + 6 + sec.height() + 6 + rows * (hpx + 6);
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([16, 12, 20]));
    blit(&mut canvas, &floor, 0, 0);
    blit(&mut canvas, &balc, floor.width() + 6, 0);
    blit(&mut canvas, &roof, floor.width() + balc.width() + 12, 0);
    blit(&mut canvas, &sec, 0, top_h + 6);
    for (k, (_, s)) in shots.iter().enumerate() {
        let (cx, cy) = (k as u32 % cols, k as u32 / cols);
        blit(&mut canvas, s, cx * (wpx + 6), top_h + 6 + sec.height() + 6 + cy * (hpx + 6));
    }
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    canvas.save(&out).unwrap();
    println!("wrote {} ({}x{})", out.display(), canvas.width(), canvas.height());
}
