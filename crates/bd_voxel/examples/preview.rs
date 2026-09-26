//! Render the generated arena to a PNG so the layout can be reviewed without the game.
//!
//! ```text
//! cargo run -p bd_voxel --example preview --release -- [seed] [out.png] [--flat | --tutorial | --map miami]
//! ```
//! Default output: `docs/arena_preview.png` (`docs/tutorial_preview.png` with `--tutorial`,
//! `docs/miami_preview.png` with `--map miami`; relative to the workspace root). City maps also get a
//! water plane, their sky palette and a bottom row of three sun-lit perspective "hero shots".
//! Red markers = spawns (tutorial: station starts). Layout:
//! top row = two isometric views from opposite corners; bottom row = top-down height/material map
//! (10 m grid, spawns marked) and a side elevation looking north.
//! Set `PREVIEW_PARTS_DIR` to also write each view as its own full-resolution PNG.

use std::path::PathBuf;
use std::time::Instant;

use bd_core::VOXEL_SIZE;
use bd_core::voxel::AIR;
use bd_voxel::noise::{hash_v, signed};
use bd_voxel::{ArenaInfo, MapInfo, VoxelWorld, generate_arena, generate_flat_test, generate_miami, material_def};
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

/// Presentation settings: the default blue sky for the Archipelago, or a city map's palette + water.
#[derive(Clone)]
struct Look {
    /// Sky stops (top, middle, bottom) in 0–255 sRGB; `mid == None` = two-stop gradient.
    sky_top: [f32; 3],
    sky_mid: Option<[f32; 3]>,
    sky_bot: [f32; 3],
    water: Option<(f32, [f32; 3])>,
    /// Per-face colour grade (top, +X, +Z) multiplied into iso faces.
    grade: Option<[[f32; 3]; 3]>,
}

impl Look {
    fn default_sky() -> Self {
        Self { sky_top: [84.0, 148.0, 242.0], sky_mid: None, sky_bot: [204.0, 228.0, 255.0], water: None, grade: None }
    }
    fn from_map(m: &MapInfo) -> Self {
        let c = |a: [f32; 3]| [a[0] * 255.0, a[1] * 255.0, a[2] * 255.0];
        Self {
            sky_top: c(m.sky_zenith),
            sky_mid: Some(c(m.sky_mid)),
            sky_bot: c(m.sky_horizon),
            water: Some((m.water_y, c(m.water_color))),
            grade: Some([[1.04, 0.95, 0.86], [1.0, 0.86, 0.84], [0.84, 0.80, 0.98]]),
        }
    }
}

thread_local! {
    static LOOK: std::cell::RefCell<Look> = std::cell::RefCell::new(Look::default_sky());
}

fn sky(y: u32, h: u32) -> Rgb<u8> {
    let t = y as f32 / h as f32;
    LOOK.with(|l| {
        let l = l.borrow();
        let (a, b, k) = match l.sky_mid {
            None => (l.sky_top, l.sky_bot, t),
            Some(m) if t < 0.55 => (l.sky_top, m, t / 0.55),
            Some(m) => (m, l.sky_bot, (t - 0.55) / 0.45),
        };
        Rgb([(a[0] + (b[0] - a[0]) * k) as u8, (a[1] + (b[1] - a[1]) * k) as u8, (a[2] + (b[2] - a[2]) * k) as u8])
    })
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
    let look = LOOK.with(|l| l.borrow().clone());
    let water_layer = look.water.map(|(wy, _)| (wy / VOXEL_SIZE).floor() as i32 - 1);
    for c in w.nonempty_chunks() {
        let o = c * 32;
        for ly in 0..32 {
            for lz in 0..32 {
                for lx in 0..32 {
                    let v = o + IVec3::new(lx, ly, lz);
                    if w.get(v) == AIR || water_layer.is_some_and(|wl| v.y <= wl) {
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
                        let col = match look.grade {
                            Some(g) if glow < 0.5 => {
                                let gf = g[f as usize];
                                [base[0] * gf[0], base[1] * gf[1], base[2] * gf[2]]
                            }
                            _ => base,
                        };
                        img.put_pixel(px as u32, py as u32, to_px(col, k));
                    }
                }
            }
        }
    }
    // Water plane: top faces at the water layer wherever the world is open there.
    if let (Some(wl), Some((_, wc))) = (water_layer, look.water) {
        for z in lo.z..hi.z {
            for x in lo.x..hi.x {
                let v = IVec3::new(x, wl, z);
                if w.get(v) != AIR {
                    continue;
                }
                let t = tf(v);
                let (ox, oy) = proj(t.x as f32, t.y as f32, t.z as f32);
                let (ox, oy) = ((ox + x_off).round() as i32, (oy + y_off).round() as i32);
                let d = t.x + t.y + t.z;
                let sparkle = 1.0 + 0.06 * signed(hash_v(IVec3::new(x / 3, 0, z / 2), 0x0a7e));
                // Lighter shallows along seawalls/beaches.
                let near = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z].iter().any(|&n| w.get(v + n * 2 + IVec3::Y) != AIR);
                let k = if near { 1.25 } else { sparkle };
                for &(sx, sy, f) in &sprite {
                    if f != 0 {
                        continue;
                    }
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
                    img.put_pixel(px as u32, py as u32, to_px(wc, k));
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
            let water = LOOK.with(|l| l.borrow().water);
            let submerged = water.is_some_and(|(wy, _)| y != i32::MIN && ((y + 1) as f32 * VOXEL_SIZE) <= wy);
            let px_col = if let (Some((_, wc)), true) = (water, y == i32::MIN || submerged) {
                let wx = x as f32 * VOXEL_SIZE;
                let wz = z as f32 * VOXEL_SIZE;
                let grid = wx.rem_euclid(10.0) < VOXEL_SIZE || wz.rem_euclid(10.0) < VOXEL_SIZE;
                to_px(
                    wc,
                    if submerged {
                        1.15
                    } else if grid {
                        0.8
                    } else {
                        0.7
                    },
                )
            } else if y == i32::MIN {
                // Void: dark blue with grid.
                let wx = x as f32 * VOXEL_SIZE;
                let wz = z as f32 * VOXEL_SIZE;
                if wx.rem_euclid(10.0) < VOXEL_SIZE || wz.rem_euclid(10.0) < VOXEL_SIZE { Rgb([40, 52, 86]) } else { Rgb([22, 28, 52]) }
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
                let cc =
                    [c[0] * k * (1.0 - mix) + fog[0] * mix, c[1] * k * (1.0 - mix) + fog[1] * mix, c[2] * k * (1.0 - mix) + fog[2] * mix];
                let p = to_px(cc, 1.0);
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(px * scale + dx, py * scale + dy, p);
                    }
                }
            }
        }
    }
    // Water: tint everything below sea level.
    if let Some((wy, wc)) = LOOK.with(|l| l.borrow().water) {
        let wpy = ((hi.y as f32 - wy / VOXEL_SIZE) as u32 * scale).min(sy * scale - 1);
        for py in wpy..sy * scale {
            for x in 0..sx * scale {
                let p = img.get_pixel(x, py).0;
                let m = |a: u8, b: f32| (a as f32 * 0.25 + b * 0.75) as u8;
                img.put_pixel(x, py, Rgb([m(p[0], wc[0]), m(p[1], wc[1]), m(p[2], wc[2])]));
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
    let pos: Vec<&String> =
        args.iter().enumerate().filter(|(i, a)| !a.starts_with("--") && !(*i > 0 && args[i - 1] == "--map")).map(|(_, a)| a).collect();
    let seed: u64 = pos.first().and_then(|s| s.parse().ok()).unwrap_or(1);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let map = args.iter().position(|a| a == "--map").and_then(|i| args.get(i + 1)).cloned();
    let default_name = if args.iter().any(|a| a == "--tutorial") {
        "docs/tutorial_preview.png"
    } else if map.as_deref() == Some("miami") {
        "docs/miami_preview.png"
    } else {
        "docs/arena_preview.png"
    };
    let out = pos.get(1).map(PathBuf::from).unwrap_or_else(|| root.join(default_name));

    let t = Instant::now();
    let tutorial = args.iter().any(|a| a == "--tutorial");
    let mut city: Option<MapInfo> = None;
    let (world, info) = if let Some(name) = map.as_deref() {
        let (w, m) = match name {
            "miami" => generate_miami(seed),
            other => panic!("unknown map {other:?} (known: miami)"),
        };
        LOOK.with(|l| *l.borrow_mut() = Look::from_map(&m));
        let a = m.arena.clone();
        city = Some(m);
        (w, a)
    } else if tutorial {
        // Markers = station start points.
        let (w, t) = bd_voxel::generate_tutorial();
        let (_, mut info) = generate_flat_test(64.0);
        info.spawns = t.stations.iter().map(|s| s.start).collect();
        info.void_y = t.void_y;
        (w, info)
    } else if flat {
        generate_flat_test(128.0)
    } else {
        generate_arena(seed)
    };
    if let Some(m) = &city {
        println!(
            "{}: water at {} m, {} districts, {} routes, {} pruned voxels",
            m.name,
            m.water_y,
            m.districts.len(),
            m.routes.len(),
            m.pruned_voxels
        );
    }
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
    #[allow(unused_mut)]
    let mut canvas = RgbImage::from_pixel(w0.max(top.width() + side.width()), h0 + h1, Rgb([16, 18, 28]));
    blit(&mut canvas, &iso_a, 0, 0);
    blit(&mut canvas, &iso_b, iso_a.width(), 0);
    blit(&mut canvas, &top, 0, h0);
    blit(&mut canvas, &side, top.width(), h0 + (h1 - side.height()) / 2);
    if let Some(m) = &city {
        let shots = hero_shots(&world, m);
        let hh = shots.iter().map(|s| s.height()).max().unwrap_or(0);
        let ww: u32 = shots.iter().map(|s| s.width()).sum();
        let mut bigger = RgbImage::from_pixel(canvas.width().max(ww), canvas.height() + hh, Rgb([16, 18, 28]));
        blit(&mut bigger, &canvas, 0, 0);
        let mut x = 0;
        for s in &shots {
            blit(&mut bigger, s, x, canvas.height());
            x += s.width();
        }
        canvas = bigger;
        if let Ok(dir) = std::env::var("PREVIEW_PARTS_DIR") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).ok();
            for (i, s) in shots.iter().enumerate() {
                s.save(dir.join(format!("hero_{i}.png"))).unwrap();
            }
        }
    }
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

// ------------------------------------------------------------------------------------------------
// Perspective hero shots (city maps): DDA ray per pixel, sun + shadow ray, sky gradient, water plane
// with sky reflection and sun glint, distance fog, emissive neon.

fn srgb_lin(c: f32) -> f32 {
    (c / 255.0).max(0.0).powf(2.2)
}

fn lin_srgb(c: f32) -> u8 {
    (c.max(0.0).powf(1.0 / 2.2) * 255.0).clamp(0.0, 255.0) as u8
}

fn sky_lin(m: &MapInfo, dir: Vec3) -> Vec3 {
    let sun = m.arena.sun_dir;
    let l = |a: [f32; 3]| Vec3::new(a[0].powf(2.2), a[1].powf(2.2), a[2].powf(2.2));
    let t = dir.y.clamp(-0.2, 1.0);
    let (zen, mid, hor) = (l(m.arena.sky_zenith), l(m.sky_mid), l(m.arena.sky_horizon));
    let mut c = if t > 0.18 { mid.lerp(zen, ((t - 0.18) / 0.6).min(1.0)) } else { hor.lerp(mid, (t.max(0.0) / 0.18).min(1.0)) };
    let cs = dir.dot(sun).max(0.0);
    let sc = l(m.sun_color);
    c += sc * (cs.powf(8.0) * 0.35 + cs.powf(64.0) * 0.8);
    if cs > 0.9994 {
        c = Vec3::new(1.6, 1.3, 0.9);
    }
    c
}

fn shade_ray(w: &VoxelWorld, m: &MapInfo, eye: Vec3, dir: Vec3) -> Vec3 {
    let sun = m.arena.sun_dir;
    let sc = Vec3::new(m.sun_color[0].powf(2.2), m.sun_color[1].powf(2.2), m.sun_color[2].powf(2.2)) * 2.2;
    let amb = Vec3::new(0.55, 0.42, 0.62) * m.arena.ambient;
    let fog = Vec3::new(m.arena.fog_color[0].powf(2.2), m.arena.fog_color[1].powf(2.2), m.arena.fog_color[2].powf(2.2));
    let hit = bd_core::voxel::raycast(w, eye, dir, 420.0);
    let t_water = if dir.y < -1e-4 { (m.water_y - eye.y) / dir.y } else { f32::INFINITY };
    let (col, dist) = match hit {
        Some(h) if h.t < t_water => {
            let mdef = material_def(w.get(h.voxel));
            let b = 1.0 + mdef.jitter * signed(hash_v(h.voxel, 0x00c0_10e5)) * 1.3;
            let alb = Vec3::new(srgb_lin(mdef.color[0] as f32 * b), srgb_lin(mdef.color[1] as f32 * b), srgb_lin(mdef.color[2] as f32 * b));
            let n = h.normal.as_vec3();
            let ndl = n.dot(sun).max(0.0);
            let lit = if ndl > 0.0 {
                let o = h.point + n * 0.01;
                if bd_core::voxel::raycast(w, o, sun, 220.0).is_some() { 0.0 } else { ndl }
            } else {
                0.0
            };
            let sky_fill = 0.75 + 0.25 * n.y;
            let mut c = alb * (amb * sky_fill + sc * lit);
            if mdef.emissive > 0.0 {
                c = c.lerp(alb * (1.0 + m.neon_boost), mdef.emissive.min(1.0));
            }
            (c, h.t)
        }
        _ if t_water.is_finite() && t_water < 600.0 => {
            let wc = Vec3::new(m.water_color[0].powf(2.2), m.water_color[1].powf(2.2), m.water_color[2].powf(2.2));
            let deep = Vec3::new(m.water_deep[0].powf(2.2), m.water_deep[1].powf(2.2), m.water_deep[2].powf(2.2));
            let p = eye + dir * t_water;
            let ripple = 0.03 * ((p.x * 0.9).sin() * (p.z * 1.3).cos() + (p.x * 0.37 + p.z * 0.21).sin());
            let r = Vec3::new(dir.x + ripple, -dir.y, dir.z - ripple).normalize();
            let fres = 0.08 + 0.7 * (1.0 + dir.y).powf(5.0).min(1.0);
            let base = deep.lerp(wc, (-dir.y * 2.5).clamp(0.0, 1.0) * 0.6 + 0.2);
            let mut c = base * (0.6 + 0.4 * m.arena.ambient) + sky_lin(m, r) * fres;
            // Reflect the city: a short ray upward from the water.
            if let Some(h) = bd_core::voxel::raycast(w, p + Vec3::Y * 0.01, r, 200.0) {
                let mdef = material_def(w.get(h.voxel));
                let alb = Vec3::new(srgb_lin(mdef.color[0] as f32), srgb_lin(mdef.color[1] as f32), srgb_lin(mdef.color[2] as f32));
                let e = if mdef.emissive > 0.5 { 1.4 } else { 0.35 };
                c = c.lerp(alb * e + base * 0.3, 0.45 * (1.0 - fres));
            }
            (c, t_water)
        }
        _ => return sky_lin(m, dir),
    };
    let f = ((dist - m.fog_start) / (m.fog_end - m.fog_start)).clamp(0.0, 1.0);
    col.lerp(fog, f * f * 0.85)
}

fn perspective(w: &VoxelWorld, m: &MapInfo, eye: Vec3, target: Vec3, fov_deg: f32, wpx: u32, hpx: u32) -> RgbImage {
    let fwd = (target - eye).normalize();
    let right = fwd.cross(Vec3::Y).normalize();
    let up = right.cross(fwd);
    let tan = (fov_deg.to_radians() * 0.5).tan();
    let aspect = wpx as f32 / hpx as f32;
    let rows: Vec<Vec<Rgb<u8>>> = std::thread::scope(|s| {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as u32;
        let chunk = hpx.div_ceil(threads);
        let handles: Vec<_> = (0..threads)
            .map(|ti| {
                s.spawn(move || {
                    let mut out = Vec::new();
                    for y in ti * chunk..((ti + 1) * chunk).min(hpx) {
                        let mut row = Vec::with_capacity(wpx as usize);
                        for x in 0..wpx {
                            let u = ((x as f32 + 0.5) / wpx as f32 * 2.0 - 1.0) * tan * aspect;
                            let v = (1.0 - (y as f32 + 0.5) / hpx as f32 * 2.0) * tan;
                            let d = (fwd + right * u + up * v).normalize();
                            let c = shade_ray(w, m, eye, d);
                            // Gentle filmic tonemap.
                            let tm = |c: f32| c / (1.0 + c * 0.25);
                            row.push(Rgb([lin_srgb(tm(c.x)), lin_srgb(tm(c.y)), lin_srgb(tm(c.z))]));
                        }
                        out.push(row);
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    RgbImage::from_fn(wpx, hpx, |x, y| rows[y as usize][x as usize])
}

/// Three establishing shots: skyline from the beach, down Ocean Drive, over Brickell towards the sun.
fn hero_shots(w: &VoxelWorld, m: &MapInfo) -> Vec<RgbImage> {
    let (wpx, hpx) = (760, 440);
    vec![
        perspective(w, m, Vec3::new(95.0, 22.0, -100.0), Vec3::new(-45.0, 34.0, 25.0), 52.0, wpx, hpx),
        perspective(w, m, Vec3::new(56.0, 9.0, -98.0), Vec3::new(50.0, 11.0, 0.0), 64.0, wpx, hpx),
        perspective(w, m, Vec3::new(-66.0, 38.0, 100.0), Vec3::new(10.0, 12.0, -40.0), 70.0, wpx, hpx),
    ]
}
