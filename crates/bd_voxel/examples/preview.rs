//! Render the generated arena to a PNG so the layout can be reviewed without the game.
//!
//! ```text
//! cargo run -p bd_voxel --example preview --release -- [seed] [out.png] [--flat]
//! ```
//! Default output: `docs/arena_preview.png` (relative to the workspace root). Layout:
//! top row = two isometric views from opposite corners; bottom row = top-down height/material map
//! (10 m grid, spawns marked) and a side elevation looking north.
//! Set `PREVIEW_PARTS_DIR` to also write each view as its own full-resolution PNG.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::VOXEL_SIZE;
use bd_core::voxel::AIR;
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{ArenaInfo, VoxelWorld, generate_arena, generate_flat_test, material_def};
use glam::{IVec3, Vec3};
use image::{Rgb, RgbImage};

fn voxel_rgb(w: &VoxelWorld, v: IVec3) -> [f32; 3] {
    let m = w.get(v);
    let d = material_def(m);
    let b = 1.0 + d.jitter * signed(hash_v(v, 0x00c0_10e5)) * 1.3;
    [d.color[0] as f32 * b, d.color[1] as f32 * b, d.color[2] as f32 * b]
}

fn to_px(c: [f32; 3], k: f32) -> Rgb<u8> {
    Rgb([(c[0] * k).clamp(0.0, 255.0) as u8, (c[1] * k).clamp(0.0, 255.0) as u8, (c[2] * k).clamp(0.0, 255.0) as u8])
}

fn sky(y: u32, h: u32) -> Rgb<u8> {
    let t = y as f32 / h as f32;
    let top = [84.0, 148.0, 242.0];
    let bot = [204.0, 228.0, 255.0];
    Rgb([
        (top[0] + (bot[0] - top[0]) * t) as u8,
        (top[1] + (bot[1] - top[1]) * t) as u8,
        (top[2] + (bot[2] - top[2]) * t) as u8,
    ])
}

/// Column height map (top solid voxel y, or i32::MIN).
fn heights(w: &VoxelWorld) -> (IVec3, IVec3, Vec<i32>) {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let sx = (hi.x - lo.x) as usize;
    let mut h = vec![i32::MIN; sx * (hi.z - lo.z) as usize];
    for z in lo.z..hi.z {
        for x in lo.x..hi.x {
            if let Some(y) = w.top_solid_y(x, z) {
                h[(x - lo.x) as usize + (z - lo.z) as usize * sx] = y;
            }
        }
    }
    (lo, hi, h)
}

/// Isometric raster. `flip` views from the opposite corner.
fn iso(w: &VoxelWorld, info: &ArenaInfo, flip: bool, a: f32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let b = a * 1.15;
    // View transform: rotate 180° about Y when flipped.
    let tf = |v: IVec3| -> IVec3 { if flip { IVec3::new(lo.x + hi.x - 1 - v.x, v.y, lo.z + hi.z - 1 - v.z) } else { v } };
    let proj = |x: f32, y: f32, z: f32| -> (f32, f32) {
        ((x - lo.x as f32) * a - (z - lo.z as f32) * a, ((x - lo.x as f32) + (z - lo.z as f32)) * a * 0.5 - y * b)
    };
    let (sx, sz) = ((hi.x - lo.x) as f32, (hi.z - lo.z) as f32);
    let x_off = sz * a + 2.0;
    let y_off = hi.y as f32 * b + 2.0;
    let width = ((sx + sz) * a + 4.0) as u32;
    let height = ((sx + sz) * a * 0.5 + hi.y as f32 * b + 4.0) as u32;
    
    // Sprite: which face each pixel of a unit cube's footprint belongs to (0 top, 1 +X, 2 +Z).
    let quads: [[Vec3; 4]; 3] = [
        [Vec3::new(0., 1., 0.), Vec3::new(1., 1., 0.), Vec3::new(1., 1., 1.), Vec3::new(0., 1., 1.)],
        [Vec3::new(1., 0., 0.), Vec3::new(1., 1., 0.), Vec3::new(1., 1., 1.), Vec3::new(1., 0., 1.)],
        [Vec3::new(0., 0., 1.), Vec3::new(1., 0., 1.), Vec3::new(1., 1., 1.), Vec3::new(0., 1., 1.)],
    ];
    let pr = |p: Vec3| ((p.x - p.z) * a, (p.x + p.z) * a * 0.5 - p.y * b);
    let (smin_x, smin_y) = (-a - 1.0, -b - 1.0);
    let (sw, sh) = ((2.0 * a + 3.0).ceil() as i32, (a + b + 3.0).ceil() as i32);
    let mut sprite: Vec<(i32, i32, u8)> = Vec::new();
    for py in 0..sh {
        for px in 0..sw {
            let (x, y) = (smin_x + px as f32 + 0.5, smin_y + py as f32 + 0.5);
            for (fi, q) in quads.iter().enumerate() {
                let pts: Vec<(f32, f32)> = q.iter().map(|p| pr(*p)).collect();
                let mut sign = 0.0f32;
                let mut inside = true;
                for i in 0..4 {
                    let (ax, ay) = pts[i];
                    let (bx, by) = pts[(i + 1) % 4];
                    let c = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
                    if c.abs() < 1e-6 {
                        continue;
                    }
                    if sign == 0.0 {
                        sign = c.signum();
                    } else if c.signum() != sign {
                        inside = false;
                        break;
                    }
                }
                if inside {
                    sprite.push((px + smin_x as i32, py + smin_y as i32, fi as u8));
                    break;
                }
            }
        }
    }

    let mut img = RgbImage::from_fn(width, height, |_, y| sky(y, height));
    let (mut bx0, mut by0, mut bx1, mut by1) = (i32::MAX, i32::MAX, 0i32, 0i32);
    let mut depth = vec![i32::MIN; (width * height) as usize];
    let (hlo, _, hmap) = heights(w);
    let hsx = (hi.x - lo.x) as usize;
    let col_top = |x: i32, z: i32| hmap[(x - hlo.x) as usize + (z - hlo.z) as usize * hsx];
    for c in w.nonempty_chunks() {
        let o = c * 32;
        for ly in 0..32 {
            for lz in 0..32 {
                for lx in 0..32 {
                    let v = o + IVec3::new(lx, ly, lz);
                    if w.get(v) == AIR {
                        continue;
                    }
                    let t = tf(v);
                    // Neighbours towards the viewer in view space.
                    let vis = |d: IVec3| {
                        let n = t + d;
                        w.get(tf(n)) == AIR
                    };
                    let (top_open, x_open, z_open) = (vis(IVec3::Y), vis(IVec3::X), vis(IVec3::Z));
                    if !(top_open || x_open || z_open) {
                        continue;
                    }
                    let (ox, oy) = proj(t.x as f32, t.y as f32, t.z as f32);
                    let (ox, oy) = ((ox + x_off).round() as i32, (oy + y_off).round() as i32);
                    let d = t.x + t.y + t.z;
                    let base = voxel_rgb(w, v);
                    let shadowed = v.y < col_top(v.x, v.z);
                    let glow = material_def(w.get(v)).emissive;
                    for &(sx, sy, f) in &sprite {
                        let (px, py) = (ox + sx, oy + sy);
                        if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                            continue;
                        }
                        let i = (px as u32 + py as u32 * width) as usize;
                        if depth[i] > d {
                            continue;
                        }
                        depth[i] = d;
                        bx0 = bx0.min(px);
                        by0 = by0.min(py);
                        bx1 = bx1.max(px);
                        by1 = by1.max(py);
                        let mut k = [1.0, 0.8, 0.64][f as usize];
                        if f == 0 && shadowed {
                            k *= 0.62;
                        }
                        k = k + (1.0 - k) * glow * 0.7;
                        img.put_pixel(px as u32, py as u32, to_px(base, k));
                    }
                }
            }
        }
    }
    // Spawns: red diamonds.
    for s in &info.spawns {
        let v = tf(bd_core::voxel::world_to_voxel(*s));
        let (px, py) = proj(v.x as f32 + 0.5, v.y as f32 + 2.0, v.z as f32 + 0.5);
        marker(&mut img, (px + x_off) as i32, (py + y_off) as i32, 5);
    }
    // Crop to the drawn content (+ margin), re-painting the sky gradient over the new height.
    if bx0 > bx1 {
        return img;
    }
    let m = 16;
    let (x0, y0) = ((bx0 - m).max(0) as u32, (by0 - m).max(0) as u32);
    let (x1, y1) = (((bx1 + m) as u32).min(width - 1), ((by1 + m) as u32).min(height - 1));
    let crop = image::imageops::crop_imm(&img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
    let ch = crop.height();
    RgbImage::from_fn(crop.width(), ch, |x, y| {
        let p = *crop.get_pixel(x, y);
        if p == sky(y + y0, height) { sky(y, ch) } else { p }
    })
}

fn marker(img: &mut RgbImage, cx: i32, cy: i32, r: i32) {
    for dy in -r - 1..=r + 1 {
        for dx in -r - 1..=r + 1 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
                continue;
            }
            let m = dx.abs() + dy.abs();
            if m <= r {
                img.put_pixel(x as u32, y as u32, Rgb([235, 30, 40]));
            } else if m <= r + 1 {
                img.put_pixel(x as u32, y as u32, Rgb([255, 255, 255]));
            }
        }
    }
}

/// Top-down: material colour × hillshade × height tint, 10 m grid, spawns.
fn top_down(w: &VoxelWorld, info: &ArenaInfo, scale: u32) -> RgbImage {
    let (lo, hi, h) = heights(w);
    let (sx, sz) = ((hi.x - lo.x) as u32, (hi.z - lo.z) as u32);
    let hy = hi.y as f32;
    let mut img = RgbImage::new(sx * scale, sz * scale);
    for pz in 0..sz {
        for px in 0..sx {
            // North (+Z) up.
            let (x, z) = (lo.x + px as i32, hi.z - 1 - pz as i32);
            let at = |x: i32, z: i32| -> i32 {
                if x < lo.x || z < lo.z || x >= hi.x || z >= hi.z {
                    return i32::MIN;
                }
                h[(x - lo.x) as usize + (z - lo.z) as usize * sx as usize]
            };
            let y = at(x, z);
            let px_col = if y == i32::MIN {
                // Void: dark blue with grid.
                let wx = x as f32 * VOXEL_SIZE;
                let wz = z as f32 * VOXEL_SIZE;
                if wx.rem_euclid(10.0) < VOXEL_SIZE || wz.rem_euclid(10.0) < VOXEL_SIZE {
                    Rgb([40, 52, 86])
                } else {
                    Rgb([22, 28, 52])
                }
            } else {
                let c = voxel_rgb(w, IVec3::new(x, y, z));
                let nb = at(x - 1, z + 1);
                let slope = if nb == i32::MIN { 0.0 } else { (y - nb) as f32 };
                let shade = (1.0 + slope * 0.08).clamp(0.6, 1.35);
                let tint = 0.45 + 0.75 * (y as f32 / hy);
                to_px(c, shade * tint)
            };
            for dy in 0..scale {
                for dx in 0..scale {
                    img.put_pixel(px * scale + dx, pz * scale + dy, px_col);
                }
            }
        }
    }
    for s in &info.spawns {
        let v = bd_core::voxel::world_to_voxel(*s);
        marker(&mut img, (v.x - lo.x) * scale as i32, (hi.z - 1 - v.z) * scale as i32, 5);
    }
    // 10 m scale bar, bottom-left.
    let bar = (10.0 / VOXEL_SIZE) as u32 * scale;
    for x in 10..10 + bar {
        for y in (sz * scale - 16)..(sz * scale - 12) {
            img.put_pixel(x, y, Rgb([255, 255, 255]));
        }
    }
    img
}

/// Side elevation looking north (+Z): nearest voxel along +Z, depth-faded.
fn elevation(w: &VoxelWorld, info: &ArenaInfo, scale: u32) -> RgbImage {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let (sx, sy) = ((hi.x - lo.x) as u32, (hi.y - lo.y) as u32);
    let depth_span = (hi.z - lo.z) as f32;
    let mut img = RgbImage::from_fn(sx * scale, sy * scale, |_, y| sky(y, sy * scale));
    for py in 0..sy {
        let y = hi.y - 1 - py as i32;
        for px in 0..sx {
            let x = lo.x + px as i32;
            let hit = (lo.z..hi.z).find(|&z| w.get(IVec3::new(x, y, z)) != AIR);
            if let Some(z) = hit {
                let c = voxel_rgb(w, IVec3::new(x, y, z));
                let t = (z - lo.z) as f32 / depth_span;
                let k = 1.15 - 0.6 * t;
                let fog = [200.0, 222.0, 250.0];
                let mix = t * 0.45;
                let cc = [
                    c[0] * k * (1.0 - mix) + fog[0] * mix,
                    c[1] * k * (1.0 - mix) + fog[1] * mix,
                    c[2] * k * (1.0 - mix) + fog[2] * mix,
                ];
                let p = to_px(cc, 1.0);
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(px * scale + dx, py * scale + dy, p);
                    }
                }
            }
        }
    }
    // Void line.
    let vy = ((hi.y as f32 - info.void_y / VOXEL_SIZE) as u32 * scale).min(sy * scale - 1);
    for x in 0..sx * scale {
        img.put_pixel(x, vy, Rgb([200, 30, 40]));
    }
    for s in &info.spawns {
        let v = bd_core::voxel::world_to_voxel(*s);
        marker(&mut img, (v.x - lo.x) * scale as i32, (hi.y - 1 - v.y) * scale as i32, 4);
    }
    img
}

fn blit(dst: &mut RgbImage, src: &RgbImage, ox: u32, oy: u32) {
    for (x, y, p) in src.enumerate_pixels() {
        if ox + x < dst.width() && oy + y < dst.height() {
            dst.put_pixel(ox + x, oy + y, *p);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flat = args.iter().any(|a| a == "--flat");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let seed: u64 = pos.first().and_then(|s| s.parse().ok()).unwrap_or(1);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = pos.get(1).map(PathBuf::from).unwrap_or_else(|| root.join("docs/arena_preview.png"));

    let t = Instant::now();
    let (world, info) = if flat { generate_flat_test(128.0) } else { generate_arena(seed) };
    println!(
        "generated in {:.1} ms: {} solid voxels, {} spawns, {} anchors",
        t.elapsed().as_secs_f64() * 1e3,
        world.solid_count(),
        info.spawns.len(),
        world.anchor_hints().len()
    );

    let t = Instant::now();
    let iso_a = iso(&world, &info, false, 2.0);
    let iso_b = iso(&world, &info, true, 2.0);
    let top = top_down(&world, &info, 3);
    let side = elevation(&world, &info, 3);
    let (w0, h0) = (iso_a.width() + iso_b.width(), iso_a.height().max(iso_b.height()));
    let h1 = top.height().max(side.height());
    let mut canvas = RgbImage::from_pixel(w0.max(top.width() + side.width()), h0 + h1, Rgb([16, 18, 28]));
    blit(&mut canvas, &iso_a, 0, 0);
    blit(&mut canvas, &iso_b, iso_a.width(), 0);
    blit(&mut canvas, &top, 0, h0);
    blit(&mut canvas, &side, top.width(), h0 + (h1 - side.height()) / 2);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    canvas.save(&out).expect("write png");
    if let Ok(dir) = std::env::var("PREVIEW_PARTS_DIR") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).ok();
        iso_a.save(dir.join("iso_a.png")).unwrap();
        iso_b.save(dir.join("iso_b.png")).unwrap();
        top.save(dir.join("top.png")).unwrap();
        side.save(dir.join("side.png")).unwrap();
    }
    println!("rendered in {:.1} ms → {}", t.elapsed().as_secs_f64() * 1e3, out.display());
    for (i, s) in info.spawns.iter().enumerate() {
        println!("spawn {i}: ({:.1}, {:.1}, {:.1})", s.x, s.y, s.z);
    }
    let _ = Vec3::ZERO;
}
