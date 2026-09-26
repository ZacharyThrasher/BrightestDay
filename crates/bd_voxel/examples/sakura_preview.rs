//! Sakura Keep preview (feature `sakura`): a shaded top-down map with spawns / routes / surf lines,
//! a section through the keep, raycast shots (spawn street, first gate, A hall, garden, temple, keep
//! interior, rooftops) and mesh statistics (vs Archipelago / Miami with `--compare`).
//!
//! ```text
//! cargo run -p bd_voxel --features sakura --example sakura_preview --release -- [seed] [--out PATH] [--compare]
//! ```
//! Default output: `docs/sakura_preview.png`; every view is also written to `target/sakura/preview/`.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::sakura::{self, CastleFrame, HALL_HU, HALL_HV, HALL_TOP, generate_sakura, px_to_m};
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{ChunkMesh, MapInfo, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Vec2, Vec3};
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

/// Shaded top-down view (north up): top voxel colour, hillshade from the height map, void black.
fn top_view(w: &VoxelWorld, info: &MapInfo, s: u32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (nx, nz) = ((hi.x - lo.x) as u32, (hi.z - lo.z) as u32);
    let mut img = RgbImage::from_pixel(nx * s, nz * s, Rgb([8, 8, 14]));
    let top = |x: i32, z: i32| w.top_solid_y(x, z);
    for z in lo.z..hi.z {
        for x in lo.x..hi.x {
            let c = match top(x, z) {
                Some(y) => {
                    let e = top(x - 1, z).unwrap_or(0);
                    let n = top(x, z + 1).unwrap_or(0);
                    let shade = 1.0 + ((y - e) as f32 * 0.08 + (y - n) as f32 * 0.08).clamp(-0.35, 0.35) + (y as f32 - 30.0) * 0.004;
                    px(voxel_rgb(w, IVec3::new(x, y, z)), shade)
                }
                None => Rgb([6, 6, 12]),
            };
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
                let p = win[0].lerp(win[1], k as f32 / 120.0);
                let (a, b) = to_px(p);
                dot(&mut img, a, b, 1, Rgb([60, 230, 255]));
            }
        }
    }
    for l in &info.surf_lines {
        for k in 0..=120 {
            let p = l.a.lerp(l.b, k as f32 / 120.0);
            let (a, b) = to_px(p);
            dot(&mut img, a, b, 2, Rgb([140, 255, 120]));
        }
    }
    for sp in &info.spawns {
        let (a, b) = to_px(*sp);
        dot(&mut img, a, b, 6, Rgb([255, 255, 255]));
        dot(&mut img, a, b, 4, Rgb([230, 30, 40]));
    }
    img
}

/// Vertical section through the keep along its v axis (garden on the right).
fn keep_section(w: &VoxelWorld, s: u32) -> RgbImage {
    let f = CastleFrame::new();
    let (a0, a1) = (-70i32, 70i32); // metres along v * 2
    let (y0, y1) = (0, 124);
    let mut img = RgbImage::from_pixel(((a1 - a0 + 1) as u32) * s, ((y1 - y0 + 1) as u32) * s, Rgb([200, 214, 236]));
    for y in y0..=y1 {
        for a in a0..=a1 {
            let p = f.world(Vec2::new(0.0, a as f32 * 0.5));
            let v = IVec3::new((p.x / 0.5).floor() as i32, y, (p.y / 0.5).floor() as i32);
            let c = if w.get(v) != AIR {
                px(voxel_rgb(w, v), 0.95)
            } else if w.is_enclosed_air(v) {
                Rgb([150, 120, 170])
            } else {
                let mut c = Rgb([200, 214, 236]);
                for d in 1..16 {
                    let q = f.world(Vec2::new(d as f32 * 0.5, a as f32 * 0.5));
                    let qv = IVec3::new((q.x / 0.5).floor() as i32, y, (q.y / 0.5).floor() as i32);
                    if w.get(qv) != AIR {
                        c = px(voxel_rgb(w, qv), 0.6 - d as f32 * 0.025);
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

// ---- raycast shots --------------------------------------------------------------------------------

struct Look {
    sun: Vec3,
    sky_hi: Vec3,
    sky_lo: Vec3,
    fog: Vec3,
    fog_end: f32,
    neon: f32,
}

fn lin3(c: [f32; 3]) -> Vec3 {
    Vec3::new(c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2))
}

fn shade(w: &VoxelWorld, look: &Look, eye: Vec3, dir: Vec3) -> Vec3 {
    let sky = look.sky_lo.lerp(look.sky_hi, dir.y.clamp(0.0, 1.0).powf(0.6));
    let Some(h) = bd_core::voxel::raycast(w, eye, dir, 260.0) else { return sky };
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
    // Sun with a hard shadow ray.
    let p = h.point + n * 0.02;
    let lit = n.dot(look.sun).max(0.0) * if bd_core::voxel::raycast(w, p, look.sun, 120.0).is_some() { 0.0 } else { 1.0 };
    let amb = 0.34 + 0.12 * n.y.max(0.0);
    let mut c = alb * ((amb + lit * 0.95) * ao);
    if mdef.emissive > 0.0 {
        let k = if mdef.emissive >= 0.5 { 1.0 + look.neon * ((mdef.emissive - 0.5) * 2.0) * 1.6 } else { 1.0 };
        c = c.lerp(alb * k * 1.2, mdef.emissive.min(1.0));
    }
    let f = (h.t / look.fog_end).clamp(0.0, 1.0);
    c.lerp(look.fog, f * f * 0.8)
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
        .unwrap_or_else(|| root.join("docs/sakura_preview.png"));
    let parts = root.join("target/sakura/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (world, info) = generate_sakura(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "Sakura Keep seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {} spawns, {} pruned voxels, {} anchors",
        world.solid_count(),
        info.spawns.len(),
        info.pruned_voxels,
        world.anchor_hints().len()
    );
    let (tris, ms) = mesh_tris(&world);
    println!("  mesh: {tris} triangles total ({ms:.0} ms to mesh)");
    if args.iter().any(|a| a == "--compare") {
        let (a, _) = bd_voxel::generate_arena(1);
        let (m, _) = bd_voxel::generate_miami(1);
        for (name, w) in [("archipelago", &a), ("miami", &m)] {
            let (t, _) = mesh_tris(w);
            println!("  compare: {name}: {t} triangles");
        }
    }
    for (i, s) in info.spawns.iter().enumerate() {
        println!("  spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    for r in &info.routes {
        println!("  route {}: {:?} anchors {:?}", r.name, r.waypoints, r.anchors);
    }
    for (k, l) in info.surf_lines.iter().enumerate() {
        println!("  surf {k}: {:?} -> {:?} ({:.0} m, drop {:.1} m)", l.a, l.b, (l.b - l.a).length(), l.a.y - l.b.y);
    }

    let t = Instant::now();
    let top = top_view(&world, &info, 2);
    top.save(parts.join("top.png")).unwrap();
    let sec = keep_section(&world, 4);
    sec.save(parts.join("section_keep.png")).unwrap();

    let look = Look {
        sun: info.sun_dir,
        sky_hi: lin3(info.sky_zenith),
        sky_lo: lin3(info.sky_horizon),
        fog: lin3(info.fog_color),
        fog_end: info.fog_end,
        neon: info.neon_boost,
    };
    let (wpx, hpx) = (640, 360);
    let f = CastleFrame::new();
    let at = |p: (f32, f32), up: f32| {
        let m = px_to_m(p);
        let g = sakura::site_ground(m).map_or(14.0, |g| g.0);
        Vec3::new(m.x, g + up, m.y)
    };
    let mut shots: Vec<(String, RgbImage)> = Vec::new();
    let mut add = |name: &str, eye: Vec3, tgt: Vec3, fov: f32| {
        println!(" shot {name}:");
        shots.push((name.to_string(), shot(&world, &look, eye, tgt, fov, wpx, hpx)));
    };
    add("spawn_street", at((150.0, 690.0), 1.7), at((330.0, 600.0), 3.0), 80.0);
    add("first_gate", at((265.0, 500.0), 1.7), at((245.0, 330.0), 5.0), 80.0);
    add("a_hall", at((190.0, 345.0), 1.7), at((175.0, 262.0), 5.0), 84.0);
    add("garden", at((398.0, 338.0), 2.2), at((520.0, 260.0), 6.0), 82.0);
    add("temple", at((470.0, 400.0), 1.8), at((560.0, 370.0), 4.0), 80.0);
    let k0 = f.world(Vec2::new(0.0, HALL_HV - 2.0));
    let k1 = f.world(Vec2::new(0.0, -HALL_HV));
    add("keep_hall", Vec3::new(k0.x, sakura::layout::H_CASTLE + 1.7, k0.y), Vec3::new(k1.x, sakura::layout::H_CASTLE + 9.0, k1.y), 88.0);
    let kb = f.world(Vec2::new(HALL_HU - 2.0, HALL_HV - 2.0));
    add("keep_balcony", Vec3::new(kb.x, sakura::BALCONY_2 + 1.7, kb.y), Vec3::new(k1.x, sakura::BALCONY_1, k1.y), 90.0);
    add("rooftops", at((150.0, 420.0), 9.5), at((330.0, 300.0), 8.0), 84.0);
    let ov = f.world(Vec2::new(-30.0, 70.0));
    add("keep_outside", Vec3::new(ov.x, 30.0, ov.y), Vec3::new(f.world(Vec2::ZERO).x, HALL_TOP, f.world(Vec2::ZERO).y), 70.0);
    for (n, s) in &shots {
        s.save(parts.join(format!("shot_{n}.png"))).unwrap();
    }

    // ---- composite --------------------------------------------------------------------------------
    let cols = 3u32;
    let rows = (shots.len() as u32).div_ceil(cols);
    let width = (top.width() + 6 + sec.width()).max(cols * (wpx + 6));
    let head = top.height().max(sec.height());
    let height = head + 6 + rows * (hpx + 6);
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([16, 16, 20]));
    blit(&mut canvas, &top, 0, 0);
    blit(&mut canvas, &sec, top.width() + 6, 0);
    for (k, (_, s)) in shots.iter().enumerate() {
        let (cx, cy) = (k as u32 % cols, k as u32 / cols);
        blit(&mut canvas, s, cx * (wpx + 6), head + 6 + cy * (hpx + 6));
    }
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    canvas.save(&out).unwrap();
    println!("rendered in {:.1} s -> {} (parts in {})", t.elapsed().as_secs_f64(), out.display(), parts.display());
}
