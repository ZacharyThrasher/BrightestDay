//! Backrooms preview (feature `backrooms`): per-level floor plans, two sections, raycast interior
//! shots and mesh statistics (triangles per level, vs Archipelago / Miami with `--compare`).
//!
//! ```text
//! cargo run -p bd_voxel --features backrooms --example backrooms_preview --release -- [seed] [--out PATH] [--compare]
//! ```
//! Default output: `docs/backrooms_preview.png`; every view is also written to
//! `target/backrooms/preview/`. Plans: walls dark, floors by material, holes (to the storey below)
//! slate, void pits black, sealed interiors (culled until opened) hatched purple, spawns red,
//! props outlined, routes cyan.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::backrooms::{self, CELL, GRID0, Kit, LEVELS, N, floor_v, floor_y, generate_backrooms, level_at, plan_for};
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{CarveOp, CarveShape, ChunkMesh, MapInfo, Mesher, VoxelWorld, material_def};
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

/// Top-down plan of level `l` (north up).
fn plan_image(w: &VoxelWorld, info: &MapInfo, l: usize, s: u32) -> RgbImage {
    let (x0, x1) = (GRID0 - 1, GRID0 + CELL * N + 1);
    let n = (x1 - x0 + 1) as u32;
    let mut img = RgbImage::from_pixel(n * s, n * s, Rgb([12, 12, 16]));
    let yf = floor_v(l);
    for z in x0..=x1 {
        for x in x0..=x1 {
            let knee = IVec3::new(x, yf + 2, z);
            let c = if w.get(knee) != AIR {
                let head = w.get(IVec3::new(x, yf + 5, z)) != AIR;
                px(voxel_rgb(w, knee), if head { 0.45 } else { 0.8 })
            } else if w.is_enclosed_air(knee) {
                if (x + z).rem_euclid(4) == 0 { Rgb([90, 60, 120]) } else { Rgb([40, 30, 54]) }
            } else {
                let mut hit = None;
                for y in (yf - 2..=yf + 1).rev() {
                    if w.get(IVec3::new(x, y, z)) != AIR {
                        hit = Some(y);
                        break;
                    }
                }
                match hit {
                    Some(y) => {
                        let k = if y >= yf { 1.08 } else { 0.95 };
                        px(voxel_rgb(w, IVec3::new(x, y, z)), k)
                    }
                    None => {
                        // Hole: the storey below (slate) or the void (black).
                        let deep = (0..yf - 2).rev().find(|&y| w.get(IVec3::new(x, y, z)) != AIR);
                        match deep {
                            Some(y) if y > 2 => Rgb([60, 70, 92]),
                            _ => Rgb([0, 0, 0]),
                        }
                    }
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
    let to_px = |p: Vec3| (((p.x / 0.5) as i32 - x0) * s as i32, ((x1 - (p.z / 0.5) as i32) * s as i32));
    for r in &info.routes {
        if r.waypoints.iter().all(|p| level_at(p.y + 1.0) == Some(l)) {
            for win in r.waypoints.windows(2) {
                for k in 0..=100 {
                    let p = win[0].lerp(win[1], k as f32 / 100.0);
                    let (a, b) = to_px(p);
                    dot(&mut img, a, b, 1, Rgb([60, 230, 255]));
                }
            }
        }
    }
    for sp in &info.spawns {
        if level_at(sp.y + 1.0) == Some(l) {
            let (a, b) = to_px(*sp);
            dot(&mut img, a, b, 5, Rgb([255, 255, 255]));
            dot(&mut img, a, b, 4, Rgb([230, 30, 40]));
        }
    }
    img
}

/// Vertical section: `along_x` = the plane z = `at` (looking north), else x = `at`.
fn section(w: &VoxelWorld, along_x: bool, at: i32, s: u32) -> RgbImage {
    let (a0, a1) = (-80, 79);
    let (y0, y1) = (0, 104);
    let mut img = RgbImage::from_pixel(((a1 - a0 + 1) as u32) * s, ((y1 - y0 + 1) as u32) * s, Rgb([30, 29, 24]));
    for y in y0..=y1 {
        for a in a0..=a1 {
            let v = if along_x { IVec3::new(a, y, at) } else { IVec3::new(at, y, a) };
            let c = if w.get(v) != AIR {
                px(voxel_rgb(w, v), 0.95)
            } else if w.is_enclosed_air(v) {
                Rgb([54, 40, 70])
            } else {
                // Look a few voxels behind the plane for depth.
                let mut c = Rgb([36, 36, 30]);
                for d in 1..12 {
                    let q = if along_x { v + IVec3::new(0, 0, d) } else { v + IVec3::new(d, 0, 0) };
                    if w.get(q) != AIR {
                        c = px(voxel_rgb(w, q), 0.55 - d as f32 * 0.03);
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

// ---- interior raycast shots ---------------------------------------------------------------------

struct Look {
    fog: Vec3,
    fog_end: f32,
    neon: f32,
}

fn shade(w: &VoxelWorld, look: &Look, eye: Vec3, dir: Vec3) -> Vec3 {
    let Some(h) = bd_core::voxel::raycast(w, eye, dir, 160.0) else { return look.fog };
    let mdef = material_def(w.get(h.voxel));
    let b = 1.0 + mdef.jitter * signed(hash_v(h.voxel, 0x00c0_10e5)) * 1.3;
    let alb = Vec3::new(srgb_lin(mdef.color[0] as f32 * b), srgb_lin(mdef.color[1] as f32 * b), srgb_lin(mdef.color[2] as f32 * b));
    let n = h.normal.as_vec3();
    // Flat "fluorescent" ambient: floors brightest, ceilings dimmest; cheap AO from the 4 in-plane
    // neighbours of the air voxel in front of the face.
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
    let mut c = alb * (0.62 * face * ao);
    if mdef.emissive > 0.0 {
        let k = if mdef.emissive >= 0.5 { 1.0 + look.neon * ((mdef.emissive - 0.5) * 2.0) * 1.6 } else { 1.0 };
        c = c.lerp(alb * k, mdef.emissive.min(1.0));
    }
    let f = (h.t / look.fog_end).clamp(0.0, 1.0);
    c.lerp(look.fog, 1.0 - (-3.0 * f * f).exp())
}

fn shot(w: &VoxelWorld, look: &Look, eye: Vec3, target: Vec3, fov: f32, wpx: u32, hpx: u32) -> RgbImage {
    let fwd = (target - eye).normalize();
    // The same view in game: feet = eye - 1.6 m, yaw 0 = -Z (degrees).
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

// ---- mesh stats -----------------------------------------------------------------------------------

/// Triangles per storey (by the air side of each face), plus exterior / roof, for a map.
fn mesh_stats(w: &VoxelWorld, by_level: bool) -> (usize, [usize; LEVELS + 2], f64) {
    let t = Instant::now();
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    let mut bins = [0usize; LEVELS + 2];
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(w, c, &mut mesh);
        total += mesh.triangle_count();
        if by_level {
            for tri in mesh.indices.as_chunks::<3>().0 {
                let (a, b, cc) = (mesh.positions[tri[0] as usize], mesh.positions[tri[1] as usize], mesh.positions[tri[2] as usize]);
                let n = mesh.normals[tri[0] as usize];
                let p = Vec3::new(
                    (a[0] + b[0] + cc[0]) / 3.0 + n[0] * 0.1,
                    (a[1] + b[1] + cc[1]) / 3.0 + n[1] * 0.1,
                    (a[2] + b[2] + cc[2]) / 3.0 + n[2] * 0.1,
                );
                let inside = p.x.abs() < 38.0 && p.z.abs() < 38.0;
                let bin = match level_at(p.y) {
                    Some(l) if inside => l,
                    _ if inside && p.y > 49.0 => LEVELS,
                    _ => LEVELS + 1,
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
        .unwrap_or_else(|| root.join("docs/backrooms_preview.png"));
    SEED.with(|s| *s.borrow_mut() = seed);
    let parts = root.join("target/backrooms/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (world, info) = generate_backrooms(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    let plan = plan_for(seed);
    println!(
        "Backrooms seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {} spawns, {} pruned voxels, {} anchors",
        world.solid_count(),
        info.spawns.len(),
        info.pruned_voxels,
        world.anchor_hints().len()
    );
    let mut kits: Vec<(usize, Kit, usize)> = Vec::new();
    for r in &plan.rooms {
        match kits.iter_mut().find(|k| k.0 == r.level && k.1 == r.kit) {
            Some(k) => k.2 += 1,
            None => kits.push((r.level, r.kit, 1)),
        }
    }
    kits.sort_by_key(|k| (k.0, k.1));
    for l in 0..LEVELS {
        let s: Vec<String> = kits.iter().filter(|k| k.0 == l).map(|k| format!("{} {}", k.1.name(), k.2)).collect();
        println!("  L{l} ({:>4.0} m): {}", floor_y(l), s.join(", "));
    }
    println!(
        "  stairs {}, shafts {}, pits {}, windows {}, closed hotel rooms {}",
        plan.stairs.len(),
        plan.shafts.len(),
        plan.pits.len(),
        plan.windows.len(),
        plan.rooms.iter().filter(|r| r.closed).count()
    );
    let (tris, bins, ms) = mesh_stats(&world, true);
    println!("  mesh: {tris} triangles total ({ms:.0} ms to mesh)");
    for (l, b) in bins.iter().enumerate().take(LEVELS) {
        println!("    L{l}: {b} triangles");
    }
    println!("    roof: {}  exterior: {}", bins[LEVELS], bins[LEVELS + 1]);
    if args.iter().any(|a| a == "--compare") {
        let (a, _) = bd_voxel::generate_arena(1);
        let (m, _) = bd_voxel::generate_miami(1);
        let (o, _) = bd_voxel::generate_orbit(1);
        for (name, w) in [("archipelago", &a), ("miami", &m), ("orbit", &o)] {
            let (t, _, _) = mesh_stats(w, false);
            println!("  compare: {name}: {t} triangles");
        }
    }
    for (i, s) in info.spawns.iter().enumerate() {
        println!("  spawn {i}: ({:.1}, {:.1}, {:.1}) L{:?}", s.x, s.y, s.z, level_at(s.y + 1.0));
    }
    for r in &info.routes {
        println!("  route {}: {:?} anchors {:?}", r.name, r.waypoints, r.anchors);
    }

    // ---- images -----------------------------------------------------------------------------------
    let t = Instant::now();
    let plans: Vec<RgbImage> = (0..LEVELS).map(|l| plan_image(&world, &info, l, 3)).collect();
    for (l, p) in plans.iter().enumerate() {
        p.save(parts.join(format!("plan_L{l}.png"))).unwrap();
    }
    let hall = backrooms::HALL.center();
    let sec_x = section(&world, true, (hall.y / 0.5) as i32, 3);
    let sec_z = section(&world, false, (hall.x / 0.5) as i32 + 12, 3);
    sec_x.save(parts.join("section_ew.png")).unwrap();
    sec_z.save(parts.join("section_ns.png")).unwrap();

    let fog = |c: [f32; 3]| Vec3::new(c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2));
    let look = Look { fog: fog(info.fog_color), fog_end: info.fog_end, neon: info.neon_boost };
    let (wpx, hpx) = (640, 360);
    let mut shots: Vec<(String, RgbImage)> = Vec::new();
    // Largest walkable room per (level, kit): look along its long axis from one end.
    let add_room_shot = |shots: &mut Vec<(String, RgbImage)>, level: usize, kit: Kit, name: &str| {
        let best = plan
            .rooms
            .iter()
            .filter(|r| r.level == level && r.kit == kit && r.walkable && !r.hotel_room)
            .max_by_key(|r| r.rect.w() * r.rect.h());
        let Some(r) = best else { return };
        let c = r.rect.center();
        let (hw, hh) = (r.rect.w() as f32 * 2.0, r.rect.h() as f32 * 2.0);
        let y = floor_y(level) + 1.7;
        let (eye, tgt) = if hw >= hh {
            (Vec3::new(c.x - hw + 1.2, y, c.y - hh * 0.3), Vec3::new(c.x + hw, y - 0.3, c.y + hh * 0.2))
        } else {
            (Vec3::new(c.x - hw * 0.3, y, c.y - hh + 1.2), Vec3::new(c.x + hw * 0.2, y - 0.3, c.y + hh))
        };
        shots.push((name.to_string(), shot(&world, &look, eye, tgt, 78.0, wpx, hpx)));
    };
    add_room_shot(&mut shots, 4, Kit::Office, "office_L4");
    add_room_shot(&mut shots, 0, Kit::Pool, "pool_L0");
    add_room_shot(&mut shots, 0, Kit::Garage, "garage_L0");
    add_room_shot(&mut shots, 4, Kit::Playground, "playground_L4");
    if let Some(r) = info.route("hotel_bhop") {
        let (a, b) = (r.waypoints[0], r.waypoints[1]);
        shots.push(("hotel_L3".into(), shot(&world, &look, a + Vec3::new(0.0, 1.7, 0.0), b + Vec3::new(0.0, 1.2, 0.0), 74.0, wpx, hpx)));
    }
    if let Some(r) = info.route("sky_swing") {
        let (a, b) = (r.waypoints[0], r.waypoints[1]);
        shots.push(("sky_L2".into(), shot(&world, &look, a + Vec3::new(0.0, 1.7, 0.0), b + Vec3::new(0.0, 6.0, 0.0), 84.0, wpx, hpx)));
    }
    // Punch-through: blow a hole through the slab over a garage spawn-free spot and look up.
    let mut carved = world.clone_for_preview();
    if let Some(r) =
        plan.rooms.iter().filter(|r| r.level == 1 && r.kit == Kit::Office && r.walkable).max_by_key(|r| r.rect.w() * r.rect.h())
    {
        let c = r.rect.center();
        let slab = floor_y(2) - 0.5;
        let op = CarveOp { shape: CarveShape::Sphere { center: Vec3::new(c.x, slab, c.y), radius: 2.6 }, power: 3.0, seed: 7 };
        carved.apply_carve(&op);
        let op2 = CarveOp { shape: CarveShape::Sphere { center: Vec3::new(c.x, floor_y(1) + 5.2, c.y), radius: 2.4 }, power: 3.0, seed: 8 };
        carved.apply_carve(&op2);
        let eye = Vec3::new(c.x - 2.2, floor_y(1) + 1.6, c.y - 0.3);
        shots.push(("punch_L1_up".into(), shot(&carved, &look, eye, Vec3::new(c.x, floor_y(2) + 5.0, c.y), 80.0, wpx, hpx)));
    }
    for (n, s) in &shots {
        s.save(parts.join(format!("shot_{n}.png"))).unwrap();
    }

    // ---- composite --------------------------------------------------------------------------------
    let pw: u32 = plans.iter().map(|p| p.width()).sum::<u32>() + 4 * 6;
    let ph = plans[0].height();
    let sw = sec_x.width() + sec_z.width() + 6;
    let sh = sec_x.height();
    let cols = 3u32;
    let rows = (shots.len() as u32).div_ceil(cols);
    let width = pw.max(sw).max(cols * (wpx + 6));
    let height = ph + 6 + sh + 6 + rows * (hpx + 6);
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([16, 16, 20]));
    let mut x = 0;
    for p in &plans {
        blit(&mut canvas, p, x, 0);
        x += p.width() + 6;
    }
    blit(&mut canvas, &sec_x, 0, ph + 6);
    blit(&mut canvas, &sec_z, sec_x.width() + 6, ph + 6);
    for (k, (_, s)) in shots.iter().enumerate() {
        let (cx, cy) = (k as u32 % cols, k as u32 / cols);
        blit(&mut canvas, s, cx * (wpx + 6), ph + 6 + sh + 6 + cy * (hpx + 6));
    }
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    canvas.save(&out).unwrap();
    println!("rendered in {:.1} s -> {} (parts in {})", t.elapsed().as_secs_f64(), out.display(), parts.display());
}

trait ClonePreview {
    fn clone_for_preview(&self) -> VoxelWorld;
}

impl ClonePreview for VoxelWorld {
    /// Regenerating is the simplest deep copy (VoxelWorld isn't Clone).
    fn clone_for_preview(&self) -> VoxelWorld {
        let (w, _) = generate_backrooms(SEED.with(|s| *s.borrow()));
        let _ = self;
        w
    }
}

thread_local! {
    static SEED: std::cell::RefCell<u64> = const { std::cell::RefCell::new(1) };
}
