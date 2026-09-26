//! Cybertron preview (feature `cybertron`): a top-down plan (spawns, transforming pieces in their
//! extended pose, space-bridge gates, surf lines, 10 m grid) and raytraced hero shots (space sky,
//! sun + shadow rays, emissive energon, the movers drawn as translucent hard light).
//!
//! ```text
//! cargo run -p bd_voxel --features cybertron --example cybertron_preview --release -- [seed] [--out PATH] [--folded]
//! ```
//! Default output `docs/cybertron_preview.png`; each view is also written to `target/cybertron/preview/`.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::{AIR, raycast};
use bd_voxel::maps::cybertron::{self, STREET, generate_cybertron};
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{ChunkMesh, MapInfo, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Quat, Vec3};
use image::{Rgb, RgbImage};

fn srgb_lin(c: f32) -> f32 {
    (c / 255.0).max(0.0).powf(2.2)
}
fn lin_srgb(c: f32) -> u8 {
    (c.max(0.0).powf(1.0 / 2.2) * 255.0).clamp(0.0, 255.0) as u8
}
fn blit(dst: &mut RgbImage, src: &RgbImage, ox: u32, oy: u32) {
    for (x, y, p) in src.enumerate_pixels() {
        if ox + x < dst.width() && oy + y < dst.height() {
            dst.put_pixel(ox + x, oy + y, *p);
        }
    }
}
fn dot(img: &mut RgbImage, cx: i32, cy: i32, r: i32, c: Rgb<u8>) {
    for dy in -r - 1..=r + 1 {
        for dx in -r - 1..=r + 1 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
                continue;
            }
            let m = dx.abs() + dy.abs();
            if m <= r {
                img.put_pixel(x as u32, y as u32, c);
            } else if m <= r + 1 {
                img.put_pixel(x as u32, y as u32, Rgb([255, 255, 255]));
            }
        }
    }
}

/// Mover boxes in the chosen pose: (centre, half, rotation).
fn mover_boxes(info: &MapInfo, t: f32) -> Vec<(Vec3, Vec3, Quat)> {
    let mut out = Vec::new();
    for m in &info.arena.mechanics.movers {
        for p in &m.parts {
            let (c, q) = p.pose(t);
            out.push((c, p.half, q));
        }
    }
    out
}

fn top_down(w: &VoxelWorld, info: &MapInfo, s: u32, boxes: &[(Vec3, Vec3, Quat)]) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (sx, sz) = ((hi.x - lo.x) as u32, (hi.z - lo.z) as u32);
    let mut img = RgbImage::new(sx * s, sz * s);
    let top_at = |x: i32, z: i32| w.top_solid_y(x, z);
    for pz in 0..sz {
        for px in 0..sx {
            let (x, z) = (lo.x + px as i32, hi.z - 1 - pz as i32);
            let wx = (x as f32 + 0.5) * 0.5;
            let wz = (z as f32 + 0.5) * 0.5;
            let mut col = match top_at(x, z) {
                None => {
                    if wx.rem_euclid(10.0) < 0.5 || wz.rem_euclid(10.0) < 0.5 { [40.0, 30.0, 34.0] } else { [22.0, 14.0, 18.0] }
                }
                Some(y) => {
                    let d = material_def(w.get(IVec3::new(x, y, z)));
                    let nb = top_at(x - 1, z + 1).unwrap_or(y);
                    let shade = (1.0 + (y - nb) as f32 * 0.08).clamp(0.6, 1.35);
                    let tint = 0.55 + 0.6 * (y as f32 / hi.y as f32);
                    let k = shade * tint + d.emissive * 0.4;
                    [d.color[0] as f32 * k, d.color[1] as f32 * k, d.color[2] as f32 * k]
                }
            };
            // Movers (extended pose): bright hard-light overlay.
            let p = Vec3::new(wx, 200.0, wz);
            for (c, h, q) in boxes {
                let l = q.inverse() * (Vec3::new(p.x, c.y, p.z) - *c);
                if l.x.abs() <= h.x && l.z.abs() <= h.z.max(h.y) {
                    col = [col[0] * 0.4 + 255.0 * 0.6, col[1] * 0.4 + 230.0 * 0.6, col[2] * 0.4 + 90.0 * 0.6];
                }
            }
            let px_c = Rgb([col[0].clamp(0.0, 255.0) as u8, col[1].clamp(0.0, 255.0) as u8, col[2].clamp(0.0, 255.0) as u8]);
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel(px * s + dx, pz * s + dy, px_c);
                }
            }
        }
    }
    let to_px = |p: Vec3| (((p.x / 0.5).floor() as i32 - lo.x) * s as i32, (hi.z - 1 - (p.z / 0.5).floor() as i32) * s as i32);
    for l in &info.arena.surf_lines {
        for k in 0..=40 {
            let p = l.a.lerp(l.b, k as f32 / 40.0);
            let (x, y) = to_px(p);
            dot(&mut img, x, y, 1, Rgb([120, 255, 160]));
        }
    }
    for pp in &info.arena.mechanics.portals {
        for (c, n) in [(pp.a, pp.a_normal), (pp.b, pp.b_normal)] {
            let (x, y) = to_px(c);
            dot(&mut img, x, y, 7, Rgb([80, 220, 255]));
            let (x2, y2) = to_px(c + n * 8.0);
            dot(&mut img, x2, y2, 3, Rgb([80, 220, 255]));
        }
    }
    for sp in &info.arena.spawns {
        let (x, y) = to_px(*sp);
        dot(&mut img, x, y, 6, Rgb([235, 30, 40]));
    }
    img
}

/// Space sky: stars, a blue-violet nebula band, Cybertron's two moons; rust haze below the horizon.
fn sky(info: &MapInfo, d: Vec3) -> Vec3 {
    let sun = info.arena.sun_dir;
    if d.y < 0.02 {
        let t = (-d.y * 4.0).min(1.0);
        return Vec3::new(0.10, 0.045, 0.03).lerp(Vec3::new(0.02, 0.01, 0.01), t);
    }
    let mut c = Vec3::new(0.004, 0.005, 0.012);
    let q = (d * 400.0).floor().as_ivec3();
    let h = hash_v(q, 77);
    if h % 997 < 3 {
        c += Vec3::splat(0.6 + (h % 7) as f32 * 0.1);
    }
    let band = (1.0 - (d.dot(Vec3::new(0.3, 0.6, 0.74).normalize())).abs() * 2.2).max(0.0);
    let n = 0.5 + 0.5 * signed(hash_v((d * 18.0).floor().as_ivec3(), 3));
    c += Vec3::new(0.05, 0.07, 0.16) * band * (0.5 + 0.5 * n) + Vec3::new(0.06, 0.02, 0.08) * band * band;
    for (dir, r, col) in [(Vec3::new(-0.5, 0.45, 0.74), 0.045, Vec3::new(0.30, 0.28, 0.32)), (Vec3::new(0.25, 0.62, 0.74), 0.028, Vec3::new(0.35, 0.22, 0.14))] {
        let m = dir.normalize();
        if d.dot(m) > (1.0 - r * r * 0.5) {
            c = col;
        }
    }
    let cs = d.dot(sun).max(0.0);
    c += Vec3::new(1.0, 0.7, 0.4) * (cs.powf(64.0) * 0.6);
    if cs > 0.9995 {
        c = Vec3::new(2.0, 1.6, 1.1);
    }
    c
}

fn ray_obb(o: Vec3, d: Vec3, c: Vec3, h: Vec3, q: Quat) -> Option<f32> {
    let inv = q.inverse();
    let lo = inv * (o - c);
    let ld = inv * d;
    let (mut t0, mut t1) = (0.0f32, f32::INFINITY);
    for i in 0..3 {
        let (oo, dd, hh) = (lo[i], ld[i], h[i]);
        if dd.abs() < 1e-8 {
            if oo.abs() > hh {
                return None;
            }
            continue;
        }
        let (a, b) = ((-hh - oo) / dd, (hh - oo) / dd);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    (t0 <= t1).then_some(t0)
}

fn shade(w: &VoxelWorld, info: &MapInfo, boxes: &[(Vec3, Vec3, Quat)], eye: Vec3, dir: Vec3) -> Vec3 {
    let sun = info.arena.sun_dir;
    let sc = Vec3::new(info.sun_color[0].powf(2.2), info.sun_color[1].powf(2.2), info.sun_color[2].powf(2.2)) * 2.4;
    let amb = Vec3::new(0.30, 0.34, 0.52) * info.arena.ambient * 1.4;
    let hit = raycast(w, eye, dir, 480.0);
    let tbox = boxes.iter().filter_map(|(c, h, q)| ray_obb(eye, dir, *c, *h, *q)).fold(f32::INFINITY, f32::min);
    let tv = hit.as_ref().map_or(f32::INFINITY, |h| h.t);
    let mut col = match hit {
        Some(h) => {
            let m = material_def(w.get(h.voxel));
            let b = 1.0 + m.jitter * signed(hash_v(h.voxel, 0x00c0_10e5)) * 1.3;
            let alb = Vec3::new(srgb_lin(m.color[0] as f32 * b), srgb_lin(m.color[1] as f32 * b), srgb_lin(m.color[2] as f32 * b));
            let n = h.normal.as_vec3();
            let ndl = n.dot(sun).max(0.0);
            let lit = if ndl > 0.0 && raycast(w, h.point + n * 0.01, sun, 200.0).is_none() { ndl } else { 0.0 };
            let mut c = alb * (amb * (0.75 + 0.25 * n.y) + sc * lit);
            if m.emissive > 0.0 {
                c = c.lerp(alb * (1.2 + info.neon_boost * 1.5), m.emissive.min(1.0));
            }
            let f = ((h.t - info.fog_start) / (info.fog_end - info.fog_start)).clamp(0.0, 1.0);
            let fog = Vec3::new(info.arena.fog_color[0].powf(2.2), info.arena.fog_color[1].powf(2.2), info.arena.fog_color[2].powf(2.2));
            c.lerp(fog, f * 0.8)
        }
        None => sky(info, dir),
    };
    if tbox < tv {
        col = col * 0.35 + Vec3::new(0.95, 0.75, 0.25) * 0.65;
    }
    col
}

fn shot(w: &VoxelWorld, info: &MapInfo, boxes: &[(Vec3, Vec3, Quat)], eye: Vec3, target: Vec3, fov: f32, wpx: u32, hpx: u32) -> RgbImage {
    let fwd = (target - eye).normalize();
    let right = fwd.cross(Vec3::Y).normalize();
    let up = right.cross(fwd);
    let tan = (fov.to_radians() * 0.5).tan();
    let aspect = wpx as f32 / hpx as f32;
    let rows: Vec<Vec<Rgb<u8>>> = std::thread::scope(|s| {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as u32;
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
                            let c = shade(w, info, boxes, eye, (fwd + right * u + up * v).normalize());
                            let tm = |c: f32| c / (1.0 + c * 0.25);
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = args.iter().find(|a| !a.starts_with("--")).and_then(|s| s.parse().ok()).unwrap_or(1);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).map(PathBuf::from).unwrap_or_else(|| root.join("docs/cybertron_preview.png"));
    let folded = args.iter().any(|a| a == "--folded");
    let parts = root.join("target/cybertron/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (w, info) = generate_cybertron(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut tris = 0usize;
    for c in w.nonempty_chunks().collect::<Vec<_>>() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        tris += mesh.triangle_count();
    }
    println!(
        "Cybertron seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {tris} tris, {} spawns, {} pruned voxels, {} anchors, {} movers, {} portals",
        w.solid_count(),
        info.spawns.len(),
        info.pruned_voxels,
        w.anchor_hints().len(),
        info.arena.mechanics.movers.len(),
        info.arena.mechanics.portals.len()
    );
    for (i, s) in info.spawns.iter().enumerate() {
        println!("spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    let boxes = mover_boxes(&info, if folded { 0.0 } else { 1.0 });
    let plan = top_down(&w, &info, 2, &boxes);
    plan.save(parts.join("plan.png")).ok();

    let (wpx, hpx) = (720, 420);
    let s = STREET;
    let views: [(&str, Vec3, Vec3, f32); 6] = [
        ("hall_of_records", Vec3::new(-40.0, s + 5.0, -2.0), Vec3::new(-90.0, s + 10.0, 0.0), 72.0),
        ("gladiator_pit", Vec3::new(30.0, s + 14.0, -22.0), Vec3::new(66.0, s - 6.0, 2.0), 70.0),
        ("well_of_allsparks", Vec3::new(-24.0, s + 22.0, -30.0), Vec3::new(0.0, s - 4.0, 0.0), 74.0),
        ("sea_of_rust", Vec3::new(-28.0, s + 8.0, 44.0), Vec3::new(4.0, 12.0, 72.0), 72.0),
        ("expressway", Vec3::new(-90.0, cybertron::EXPRESS_TOP + 3.0, 50.0), Vec3::new(0.0, cybertron::EXPRESS_TOP, 66.0), 70.0),
        ("overview", Vec3::new(-150.0, 110.0, -130.0), Vec3::new(10.0, s, 5.0), 60.0),
    ];
    let mut shots = Vec::new();
    for (name, eye, target, fov) in views {
        let img = shot(&w, &info, &boxes, eye, target, fov, wpx, hpx);
        img.save(parts.join(format!("{name}.png"))).ok();
        shots.push(img);
    }
    let cols = 3u32;
    let rows = (shots.len() as u32).div_ceil(cols);
    let width = plan.width().max(wpx * cols);
    let mut canvas = RgbImage::from_pixel(width, plan.height() + hpx * rows, Rgb([12, 12, 18]));
    blit(&mut canvas, &plan, (width - plan.width()) / 2, 0);
    for (i, s) in shots.iter().enumerate() {
        blit(&mut canvas, s, (i as u32 % cols) * wpx, plan.height() + (i as u32 / cols) * hpx);
    }
    canvas.save(&out).expect("write png");
    println!("wrote {} ({:.1} s)", out.display(), t.elapsed().as_secs_f64());
    let _ = AIR;
}
