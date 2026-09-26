//! Hyperline preview (feature `hyperline`): side elevation, top plan, car cross-sections, raycast
//! shots (roof level, a three-quarter exterior, inside a car) and mesh statistics
//! (vs Archipelago with `--compare`).
//!
//! ```text
//! cargo run -p bd_voxel --features hyperline --example hyperline_preview --release -- [seed] [--out PATH] [--compare]
//! ```
//! Default output: `docs/hyperline_preview.png`; every view is also written to
//! `target/hyperline/preview/`. Overlays: spawns red, surf lines cyan, slipstream footprint yellow,
//! route waypoints magenta.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::hyperline::{self, HALF_W, NOSE_X, ROOF_Y, TAIL_X, VOID_Y, generate_hyperline};
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{ChunkMesh, MapInfo, Mesher, VoxelWorld, generate_arena, material_def};
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

fn put(img: &mut RgbImage, x: i32, y: i32, c: Rgb<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, c);
    }
}

fn dot(img: &mut RgbImage, cx: i32, cy: i32, r: i32, c: Rgb<u8>) {
    for y in -r..=r {
        for x in -r..=r {
            if x * x + y * y <= r * r {
                put(img, cx + x, cy + y, c);
            }
        }
    }
}

fn seg(img: &mut RgbImage, a: (f32, f32), b: (f32, f32), c: Rgb<u8>) {
    let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) as i32).max(1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        put(img, (a.0 + (b.0 - a.0) * t) as i32, (a.1 + (b.1 - a.1) * t) as i32, c);
    }
}

const SKY_TOP: [f32; 3] = [60.0, 140.0, 210.0];
const SKY_BOT: [f32; 3] = [150.0, 215.0, 240.0];

fn sky_px(t: f32) -> Rgb<u8> {
    px([SKY_TOP[0] + (SKY_BOT[0] - SKY_TOP[0]) * t, SKY_TOP[1] + (SKY_BOT[1] - SKY_TOP[1]) * t, SKY_TOP[2] + (SKY_BOT[2] - SKY_TOP[2]) * t], 1.0)
}

/// Side elevation (looking from +Z): x → right, y → up; nearest solid along z.
fn elevation(w: &VoxelWorld, s: u32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (wv, hv) = ((hi.x - lo.x) as u32, (hi.y - lo.y) as u32);
    let mut img = RgbImage::new(wv * s, hv * s);
    for x in lo.x..hi.x {
        for y in lo.y..hi.y {
            let mut col = sky_px((hi.y - y) as f32 / (hi.y - lo.y) as f32);
            for z in (lo.z..hi.z).rev() {
                let v = IVec3::new(x, y, z);
                if w.get(v) != AIR {
                    let depth = (hi.z - z) as f32 / (hi.z - lo.z) as f32;
                    col = px(voxel_rgb(w, v), 1.05 - depth * 0.9);
                    break;
                }
            }
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel((x - lo.x) as u32 * s + dx, (hi.y - 1 - y) as u32 * s + dy, col);
                }
            }
        }
    }
    img
}

/// Top plan (looking down): x → right, z → up; top solid, shaded by height.
fn plan(w: &VoxelWorld, info: &MapInfo, s: u32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (wv, hv) = ((hi.x - lo.x) as u32, (hi.z - lo.z) as u32);
    let mut img = RgbImage::new(wv * s, hv * s);
    for x in lo.x..hi.x {
        for z in lo.z..hi.z {
            let col = match w.top_solid_y(x, z) {
                Some(y) => px(voxel_rgb(w, IVec3::new(x, y, z)), 0.45 + 0.6 * (y as f32 * 0.5 / 16.0)),
                None => Rgb([20, 40, 70]),
            };
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel((x - lo.x) as u32 * s + dx, (hi.z - 1 - z) as u32 * s + dy, col);
                }
            }
        }
    }
    let to = |p: Vec3| (((p.x / 0.5) as i32 - lo.x) as f32 * s as f32, ((hi.z as f32 - p.z / 0.5) * s as f32));
    if let Some(sl) = info.slipstream {
        let (a, b) = (to(Vec3::new(sl.min.x, 0.0, sl.min.y)), to(Vec3::new(sl.max.x, 0.0, sl.max.y)));
        let y = Rgb([255, 220, 40]);
        seg(&mut img, (a.0, a.1), (b.0, a.1), y);
        seg(&mut img, (a.0, b.1), (b.0, b.1), y);
        seg(&mut img, (a.0, a.1), (a.0, b.1), y);
        seg(&mut img, (b.0, a.1), (b.0, b.1), y);
    }
    for l in &info.surf_lines {
        seg(&mut img, to(l.a), to(l.b), Rgb([60, 240, 255]));
    }
    for r in &info.routes {
        for p in &r.waypoints {
            let (x, y) = to(*p);
            dot(&mut img, x as i32, y as i32, 2, Rgb([255, 60, 220]));
        }
    }
    for sp in &info.spawns {
        let (x, y) = to(*sp);
        dot(&mut img, x as i32, y as i32, 4, Rgb([255, 40, 40]));
    }
    img
}

/// Cross-section at x (looking toward the nose): z → right, y → up.
fn section(w: &VoxelWorld, info: &MapInfo, x_m: f32, s: u32) -> RgbImage {
    let (zl, zh) = (-7.0f32, 7.0f32);
    let (yl, yh) = (2.0f32, 18.0f32);
    let (wv, hv) = (((zh - zl) / 0.5) as u32, ((yh - yl) / 0.5) as u32);
    let mut img = RgbImage::new(wv * s, hv * s);
    let x = (x_m / 0.5).floor() as i32;
    for iz in 0..wv {
        for iy in 0..hv {
            let z = ((zl / 0.5) as i32) + iz as i32;
            let y = ((yl / 0.5) as i32) + iy as i32;
            let v = IVec3::new(x, y, z);
            let col = if w.get(v) != AIR { px(voxel_rgb(w, v), 1.0) } else { sky_px(0.3 + 0.5 * iy as f32 / hv as f32) };
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel((wv - 1 - iz) * s + dx, (hv - 1 - iy) * s + dy, col);
                }
            }
        }
    }
    // Surf faces of lines covering this x.
    let to = |z: f32, y: f32| (((zh - z) / 0.5) * s as f32, ((yh - y) / 0.5) * s as f32);
    for l in info.surf_lines.iter().filter(|l| x_m >= l.a.x.min(l.b.x) && x_m <= l.a.x.max(l.b.x)) {
        let side = l.a.z.signum();
        let (st, ct) = l.face_deg.to_radians().sin_cos();
        let end = (l.a.z + side * l.width * ct, l.a.y - l.width * st);
        seg(&mut img, to(l.a.z, l.a.y), to(end.0, end.1), Rgb([60, 240, 255]));
    }
    img
}

// ---- raycast shots -------------------------------------------------------------------------------

fn shade(w: &VoxelWorld, eye: Vec3, dir: Vec3) -> Vec3 {
    let sky = |d: Vec3| {
        let t = (d.y * 0.5 + 0.5).clamp(0.0, 1.0);
        let a = Vec3::new(srgb_lin(SKY_BOT[0]), srgb_lin(SKY_BOT[1]), srgb_lin(SKY_BOT[2]));
        let b = Vec3::new(srgb_lin(SKY_TOP[0]), srgb_lin(SKY_TOP[1]), srgb_lin(SKY_TOP[2]));
        a.lerp(b, t)
    };
    let Some(h) = bd_core::voxel::raycast(w, eye, dir, 400.0) else { return sky(dir) };
    let mdef = material_def(w.get(h.voxel));
    let b = 1.0 + mdef.jitter * signed(hash_v(h.voxel, 0x00c0_10e5)) * 1.3;
    let alb = Vec3::new(srgb_lin(mdef.color[0] as f32 * b), srgb_lin(mdef.color[1] as f32 * b), srgb_lin(mdef.color[2] as f32 * b));
    let n = h.normal.as_vec3();
    let sun = Vec3::new(0.35, 0.86, 0.36).normalize();
    let lit = 0.45 + 0.55 * n.dot(sun).max(0.0);
    let air = h.voxel + h.normal;
    let (t1, t2) = if n.x != 0.0 {
        (IVec3::Y, IVec3::Z)
    } else if n.y != 0.0 {
        (IVec3::X, IVec3::Z)
    } else {
        (IVec3::X, IVec3::Y)
    };
    let occ = [t1, -t1, t2, -t2].iter().filter(|&&t| w.get(air + t) != AIR).count() as f32;
    let mut c = alb * (0.85 * lit * (1.0 - occ * 0.09));
    if mdef.emissive > 0.0 {
        c = c.lerp(alb * (1.0 + 1.6 * mdef.emissive), mdef.emissive.min(1.0));
    }
    let f = (h.t / 300.0).clamp(0.0, 1.0);
    c.lerp(sky(Vec3::Y * 0.1), f * f)
}

fn shot(w: &VoxelWorld, eye: Vec3, target: Vec3, fov: f32, wpx: u32, hpx: u32) -> RgbImage {
    let fwd = (target - eye).normalize();
    let yaw = (-fwd.x).atan2(-fwd.z).to_degrees();
    let pitch = fwd.y.asin().to_degrees();
    println!("  shot: BD_SHOT_CAM={:.1},{:.1},{:.1},{yaw:.0},{pitch:.0}", eye.x, eye.y, eye.z);
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
                            let c = shade(w, eye, (fwd + right * u + up * v).normalize());
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
    let seed: u64 = args.iter().find_map(|a| a.parse().ok()).unwrap_or(1);
    let compare = args.iter().any(|a| a == "--compare");
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("docs/hyperline_preview.png"));
    let dir = PathBuf::from("target/hyperline/preview");
    std::fs::create_dir_all(&dir).ok();

    let t = Instant::now();
    let (w, info) = generate_hyperline(seed);
    println!("hyperline seed {seed}: generated in {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    println!("  spawns: {} (pruned voxels {})", info.spawns.len(), info.pruned_voxels);
    for s in &info.spawns {
        println!("    {:.2},{:.2},{:.2}", s.x, s.y, s.z);
    }
    println!("  surf lines: {}  routes: {}", info.surf_lines.len(), info.routes.len());
    let us = hyperline::units(seed);
    for u in &us {
        println!("  {:6} x {:7.1}..{:7.1}  gangway rear {} front {}  hatch {:?}", u.name(), u.x0, u.x1, u.gang_rear, u.gang_front, u.hatch());
    }
    let (tris, ms) = mesh_tris(&w);
    println!("  mesh: {tris} triangles ({ms:.0} ms)");
    if compare {
        let (a, _) = generate_arena(seed);
        println!("  compare: archipelago {} triangles", mesh_tris(&a).0);
    }

    let el = elevation(&w, 2);
    let pl = plan(&w, &info, 2);
    let car = &us[3];
    let sec_car = section(&w, &info, car.x0 + 11.25, 8);
    let sec_door = section(&w, &info, car.x0 + car.doors[0] + 0.25, 8);
    let gap_unit = us.iter().find(|u| u.gang_rear).unwrap_or(&us[1]);
    let sec_gap = section(&w, &info, gap_unit.x0 - 1.25, 8);
    let (sw, sh) = (640, 300);
    let mid = us[4].cx();
    let roof = shot(&w, Vec3::new(mid - 30.0, ROOF_Y + 1.6, 1.0), Vec3::new(mid + 40.0, ROOF_Y + 0.5, 0.0), 80.0, sw, sh);
    let three_q = shot(&w, Vec3::new(TAIL_X - 18.0, ROOF_Y + 14.0, 26.0), Vec3::new(TAIL_X + 70.0, ROOF_Y - 4.0, 0.0), 70.0, sw, sh);
    let inside = shot(&w, Vec3::new(us[2].x0 + 2.0, hyperline::FLOOR_Y + 1.6, -1.0), Vec3::new(us[2].x1 - 2.0, hyperline::FLOOR_Y + 1.5, 0.5), 80.0, sw, sh);
    let side = shot(&w, Vec3::new(us[3].x0 - 6.0, ROOF_Y - 2.0, 18.0), Vec3::new(us[3].cx(), ROOF_Y - 3.0, 0.0), 75.0, sw, sh);

    for (name, img) in [("elevation", &el), ("plan", &pl), ("section_car", &sec_car), ("section_door", &sec_door), ("section_gap", &sec_gap), ("roof", &roof), ("three_quarter", &three_q), ("inside", &inside), ("side", &side)] {
        img.save(dir.join(format!("{name}.png"))).ok();
    }

    // Contact sheet: elevation + plan on top, sections, then the four shots in a 2×2 grid.
    let pad = 8;
    let width = el.width().max(2 * sw + pad).max(sec_car.width() * 3 + pad * 2);
    let height = el.height() + pl.height() + sec_car.height() + 2 * sh + pad * 5;
    let mut sheet = RgbImage::from_pixel(width, height, Rgb([16, 22, 34]));
    let mut y = 0;
    blit(&mut sheet, &el, 0, y);
    y += el.height() + pad;
    blit(&mut sheet, &pl, 0, y);
    y += pl.height() + pad;
    blit(&mut sheet, &sec_car, 0, y);
    blit(&mut sheet, &sec_door, sec_car.width() + pad, y);
    blit(&mut sheet, &sec_gap, 2 * (sec_car.width() + pad), y);
    y += sec_car.height() + pad;
    blit(&mut sheet, &roof, 0, y);
    blit(&mut sheet, &three_q, sw + pad, y);
    y += sh + pad;
    blit(&mut sheet, &inside, 0, y);
    blit(&mut sheet, &side, sw + pad, y);
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p).ok();
    }
    sheet.save(&out).expect("write preview");
    println!("wrote {} (views in {})", out.display(), dir.display());
    let _ = (HALF_W, NOSE_X, VOID_Y);
}
