//! Mirrorfold preview (feature `mirror`): top-down plan, two sections, raycast hero shots (with the
//! hard-light surf faces drawn translucent) and mesh statistics by area.
//!
//! ```text
//! cargo run -p bd_voxel --features mirror --example mirror_preview --release -- [seed] [--out PATH]
//! ```
//! Default output: `docs/mirror_preview.png`; every view is also written to `target/mirror/preview/`.
//! Plan: height-shaded material colours, fractures black, spawns red, surf lines cyan, hanging-tower
//! footprints violet.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::voxel::AIR;
use bd_voxel::maps::mirror::{self, CURL_R, HALF_X, HALF_Z, STREET_Y, TOP_Y, generate_mirror};
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{ChunkMesh, MapInfo, Mesher, SurfLine, VoxelWorld, material_def};
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

fn line(img: &mut RgbImage, a: (f32, f32), b: (f32, f32), c: Rgb<u8>) {
    let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) as i32).max(1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        if x >= 0.0 && y >= 0.0 && (x as u32) < img.width() && (y as u32) < img.height() {
            img.put_pixel(x as u32, y as u32, c);
        }
    }
}

/// Top-down plan (north up), `s` pixels per voxel. Ignores voxels above `below_y` (so the hanging
/// towers don't hide the city), outlines them instead.
fn plan(w: &VoxelWorld, info: &MapInfo, s: u32, below_y: f32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (nx, nz) = ((hi.x - lo.x) as u32, (hi.z - lo.z) as u32);
    let mut img = RgbImage::from_pixel(nx * s, nz * s, Rgb([8, 8, 14]));
    let top = (below_y / 0.5) as i32;
    for z in lo.z..hi.z {
        for x in lo.x..hi.x {
            let mut col = None;
            for y in (0..top).rev() {
                if w.get(IVec3::new(x, y, z)) != AIR {
                    col = Some(y);
                    break;
                }
            }
            let c = match col {
                Some(y) => {
                    let k = 0.55 + 0.6 * (y as f32 * 0.5 - STREET_Y).clamp(0.0, 40.0) / 40.0;
                    px(voxel_rgb(w, IVec3::new(x, y, z)), k)
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
    let to_px = |p: Vec3| (((p.x / 0.5) as i32 - lo.x) as f32 * s as f32, ((hi.z - 1) as f32 - p.z / 0.5) * s as f32);
    for l in &info.surf_lines {
        line(&mut img, to_px(l.a), to_px(l.b), Rgb([90, 240, 255]));
    }
    for t in mirror::tower_tips() {
        let (x, y) = to_px(t);
        dot(&mut img, x as i32, y as i32, 4, Rgb([190, 110, 255]));
    }
    for sp in &info.spawns {
        let (x, y) = to_px(*sp);
        dot(&mut img, x as i32, y as i32, 5, Rgb([255, 255, 255]));
        dot(&mut img, x as i32, y as i32, 4, Rgb([230, 30, 40]));
    }
    img
}

/// Vertical section: `along_x` = the plane z = `at` voxels (looking north), else x = `at`.
fn section(w: &VoxelWorld, along_x: bool, at: i32, s: u32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (a0, a1) = if along_x { (lo.x, hi.x - 1) } else { (lo.z, hi.z - 1) };
    let (y0, y1) = (0, hi.y - 1);
    let mut img = RgbImage::from_pixel(((a1 - a0 + 1) as u32) * s, ((y1 - y0 + 1) as u32) * s, Rgb([20, 20, 30]));
    for y in y0..=y1 {
        for a in a0..=a1 {
            let v = if along_x { IVec3::new(a, y, at) } else { IVec3::new(at, y, a) };
            let c = if w.get(v) != AIR {
                px(voxel_rgb(w, v), 0.95)
            } else if w.is_enclosed_air(v) {
                Rgb([54, 40, 70])
            } else {
                let mut c = Rgb([26, 26, 38]);
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

struct Face {
    a: Vec3,
    d: Vec3,
    f: Vec3,
    n: Vec3,
    len: f32,
    width: f32,
}

fn faces(lines: &[SurfLine]) -> Vec<Face> {
    let mut out = Vec::new();
    for l in lines {
        let d = (l.b - l.a).normalize();
        let r = d.cross(Vec3::Y).normalize();
        let up = r.cross(d).normalize();
        let (st, ct) = l.face_deg.to_radians().sin_cos();
        for s in [1.0f32, -1.0] {
            let n = r * (s * st) + up * ct;
            let f = r * (s * ct) - up * st;
            out.push(Face { a: l.a, d, f, n, len: (l.b - l.a).length(), width: l.width });
        }
    }
    out
}

fn sky(dir: Vec3, info: &MapInfo) -> Vec3 {
    let z = Vec3::from_array(info.sky_zenith.map(|c| srgb_lin(c * 255.0)));
    let m = Vec3::from_array(info.sky_mid.map(|c| srgb_lin(c * 255.0)));
    let h = Vec3::from_array(info.sky_horizon.map(|c| srgb_lin(c * 255.0)));
    let t = dir.y.clamp(-1.0, 1.0);
    let base = if t > 0.3 { m.lerp(z, ((t - 0.3) / 0.7).min(1.0)) } else { h.lerp(m, ((t + 0.1) / 0.4).clamp(0.0, 1.0)) };
    // Stand-in for the client's folded horizon: a bright rift slit overhead along X.
    let slit = (1.0 - (dir.z.abs() / 0.12)).max(0.0) * t.max(0.0);
    base + Vec3::new(1.4, 1.35, 1.2) * slit * slit
}

fn shade(w: &VoxelWorld, info: &MapInfo, faces: &[Face], eye: Vec3, dir: Vec3) -> Vec3 {
    let fog = Vec3::from_array(info.fog_color.map(|c| srgb_lin(c * 255.0)));
    let sun = info.sun_dir;
    let hit = bd_core::voxel::raycast(w, eye, dir, 400.0);
    let mut c = match &hit {
        None => sky(dir, info),
        Some(h) => {
            let mdef = material_def(w.get(h.voxel));
            let b = 1.0 + mdef.jitter * signed(hash_v(h.voxel, 0x00c0_10e5)) * 1.3;
            let alb = Vec3::new(srgb_lin(mdef.color[0] as f32 * b), srgb_lin(mdef.color[1] as f32 * b), srgb_lin(mdef.color[2] as f32 * b));
            let n = h.normal.as_vec3();
            let p = h.point + n * 0.01;
            let lit = if n.dot(sun) > 0.0 && bd_core::voxel::raycast(w, p, sun, 200.0).is_none() { n.dot(sun) } else { 0.0 };
            let amb = Vec3::new(0.42, 0.42, 0.55) * (0.6 + 0.4 * n.y.max(0.0));
            let mut c = alb * (amb * info.ambient + Vec3::new(1.0, 0.9, 0.76) * lit * 0.9);
            if mdef.emissive > 0.0 {
                c = c.lerp(alb * (1.0 + mdef.emissive * 1.5), mdef.emissive.min(1.0));
            }
            let f = (h.t / info.fog_end).clamp(0.0, 1.0);
            c.lerp(fog, 1.0 - (-3.0 * f * f).exp())
        }
    };
    // Hard-light surf faces in front of the hit.
    let tmax = hit.as_ref().map_or(400.0, |h| h.t);
    let mut tint = 0.0f32;
    for fc in faces {
        let den = dir.dot(fc.n);
        if den.abs() < 1e-5 {
            continue;
        }
        let t = (fc.a - eye).dot(fc.n) / den;
        if t <= 0.0 || t >= tmax {
            continue;
        }
        let q = eye + dir * t - fc.a;
        let (u, v) = (q.dot(fc.d), q.dot(fc.f));
        if (0.0..=fc.len).contains(&u) && (0.0..=fc.width).contains(&v) {
            tint = 1.0 - (1.0 - tint) * 0.66;
        }
    }
    if tint > 0.0 {
        c = c.lerp(Vec3::new(0.35, 0.8, 1.0), tint * 0.8) + Vec3::new(0.02, 0.08, 0.12) * tint;
    }
    c
}

#[allow(clippy::too_many_arguments)]
fn shot(w: &VoxelWorld, info: &MapInfo, faces: &[Face], eye: Vec3, target: Vec3, fov: f32, wpx: u32, hpx: u32) -> RgbImage {
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
                            let c = shade(w, info, faces, eye, (fwd + right * u + up * v).normalize());
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

/// Triangles: total, then by area (curl / hanging towers / city + fragments / other), ms.
fn mesh_stats(w: &VoxelWorld) -> (usize, [usize; 4], f64) {
    let t = Instant::now();
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    let mut bins = [0usize; 4];
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(w, c, &mut mesh);
        total += mesh.triangle_count();
        for tri in mesh.indices.as_chunks::<3>().0 {
            let (a, b, cc) = (mesh.positions[tri[0] as usize], mesh.positions[tri[1] as usize], mesh.positions[tri[2] as usize]);
            let p = Vec3::new((a[0] + b[0] + cc[0]) / 3.0, (a[1] + b[1] + cc[1]) / 3.0, (a[2] + b[2] + cc[2]) / 3.0);
            let s = mirror::perim(p.x, p.z).s;
            let bin = if s >= -0.1 && p.y > STREET_Y - 2.0 {
                0
            } else if p.y > 55.0 {
                1
            } else if p.y > STREET_Y - 1.0 {
                2
            } else {
                3
            };
            bins[bin] += 1;
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
        .unwrap_or_else(|| root.join("docs/mirror_preview.png"));
    let parts = root.join("target/mirror/preview");
    std::fs::create_dir_all(&parts).ok();

    let t = Instant::now();
    let (world, info) = generate_mirror(seed);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "Mirrorfold seed {seed}: generated in {gen_ms:.0} ms, {} solid voxels, {} spawns, {} surf lines, {} pruned voxels",
        world.solid_count(),
        info.spawns.len(),
        info.surf_lines.len(),
        info.pruned_voxels
    );
    for (i, s) in info.spawns.iter().enumerate() {
        println!("  spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    let (tris, bins, ms) = mesh_stats(&world);
    println!(
        "  triangles: {tris} (curl {}, hanging towers {}, city+fragments {}, other {}) meshed in {ms:.0} ms",
        bins[0], bins[1], bins[2], bins[3]
    );

    let plan_img = plan(&world, &info, 3, 56.0);
    let sec_x = section(&world, true, 0, 2);
    let sec_z = section(&world, false, 0, 2);
    let fcs = faces(&info.surf_lines);
    let (sw, sh) = (760u32, 430u32);
    println!("shots:");
    let shots = [
        // Plaza looking up at the hanging towers and the lip.
        shot(&world, &info, &fcs, Vec3::new(-6.0, STREET_Y + 2.1, -4.0), Vec3::new(30.0, 60.0, 16.0), 90.0, sw, sh),
        // Riding the east curl, looking north along the surf band into the NE corner.
        shot(
            &world,
            &info,
            &fcs,
            Vec3::new(HALF_X + 14.0, STREET_Y + 7.0, -18.0),
            Vec3::new(HALF_X + 4.0, STREET_Y + 10.0, 40.0),
            85.0,
            sw,
            sh,
        ),
        // Over the north ring road, looking across the city to the far curl.
        shot(&world, &info, &fcs, Vec3::new(-10.0, 44.0, HALF_Z + 6.0), Vec3::new(8.0, 20.0, -HALF_Z), 80.0, sw, sh),
        // From the south-west corner of the curl lip: overview.
        shot(&world, &info, &fcs, Vec3::new(-HALF_X - 6.0, 52.0, -HALF_Z - 6.0), Vec3::new(10.0, 18.0, 8.0), 75.0, sw, sh),
    ];
    let _ = (CURL_R, TOP_Y, HALF_Z);
    plan_img.save(parts.join("plan.png")).ok();
    sec_x.save(parts.join("section_z0.png")).ok();
    sec_z.save(parts.join("section_x0.png")).ok();
    for (i, s) in shots.iter().enumerate() {
        s.save(parts.join(format!("shot{i}.png"))).ok();
    }
    // Composite: plan left; sections stacked; shots grid below.
    let pw = plan_img.width();
    let cw = pw + sec_x.width().max(sec_z.width()) / 2 + 20;
    let cw = cw.max(sw * 2 + 10);
    let top_h = plan_img.height().max(sec_x.height() / 2 + sec_z.height() / 2 + 10);
    let ch = top_h + 20 + sh * 2 + 10;
    let mut comp = RgbImage::from_pixel(cw, ch, Rgb([12, 12, 18]));
    blit(&mut comp, &plan_img, 0, 0);
    let half = |im: &RgbImage| image::imageops::resize(im, im.width() / 2, im.height() / 2, image::imageops::FilterType::Triangle);
    blit(&mut comp, &half(&sec_x), pw + 20, 0);
    blit(&mut comp, &half(&sec_z), pw + 20, sec_x.height() / 2 + 10);
    for (i, s) in shots.iter().enumerate() {
        blit(&mut comp, s, (i as u32 % 2) * (sw + 10), top_h + 20 + (i as u32 / 2) * (sh + 10));
    }
    comp.save(&out).expect("write preview");
    println!("wrote {}", out.display());
}
