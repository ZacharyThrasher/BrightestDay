//! Shared software rasteriser for the bd_shapes example tools (no GPU): a z-buffered canvas, an
//! orthographic camera, per-vertex-coloured voxel meshes (corps palette or painted costume colours) and
//! a 5×7 pixel font. Used by `costume_gallery` and `phrase` (`#[path = "raster/mod.rs"] mod raster;`).
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

use bd_core::mechanics::{Form, FormCategory, Recipe};
use bd_core::shape::ShapeSpec;
use bd_shapes::costume::{self, Rgb};
use bd_shapes::*;
use glam::{Affine3A, IVec3, Vec3};

pub const BG: [u8; 3] = [18, 20, 26];

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 3]>,
    pub z: Vec<f32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, px: vec![BG; w * h], z: vec![f32::INFINITY; w * h] }
    }
    pub fn clear_depth(&mut self) {
        self.z.fill(f32::INFINITY);
    }
    pub fn put(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }
    pub fn rect(&mut self, x0: i32, y0: i32, w: i32, h: i32, c: [u8; 3]) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.put(x, y, c);
            }
        }
    }
    /// Write a PNG (parent directories are created).
    pub fn write_png(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut img = image::RgbImage::new(self.w as u32, self.h as u32);
        for (i, p) in self.px.iter().enumerate() {
            img.put_pixel((i % self.w) as u32, (i / self.w) as u32, image::Rgb(*p));
        }
        img.save(path).map_err(|e| format!("{}: {e}", path.display()))
    }
    /// [`Self::write_png`], printing "wrote <path>" (gallery tools; panics on failure).
    pub fn save(&self, path: &Path) {
        self.write_png(path).expect("save png");
        println!("wrote {}", path.display());
    }
}

#[derive(Clone, Copy)]
pub struct Cam {
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
}

impl Cam {
    pub fn from_eye(eye_dir: Vec3) -> Cam {
        let fwd = -eye_dir.normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        let up = right.cross(fwd).normalize();
        Cam { right, up, fwd }
    }
    pub fn proj(&self, p: Vec3) -> (f32, f32, f32) {
        (p.dot(self.right), p.dot(self.up), p.dot(self.fwd))
    }
}

/// Corps colour (green lantern) palette for unpainted voxels (primary, accent, dark, core).
pub const PAL: [[f32; 3]; 4] = [[0.2, 0.82, 0.38], [0.62, 1.0, 0.68], [0.05, 0.3, 0.13], [0.93, 1.0, 0.95]];

pub const FRONT: Vec3 = Vec3::new(0.0, 0.12, -1.0);
pub const THREE_Q: Vec3 = Vec3::new(0.75, 0.22, -0.8);

/// A meshed model with a colour per mesh vertex, ready to draw.
pub struct Dressed {
    pub label: String,
    pub sub: String,
    pub model: VoxelModel,
    pub meshes: Vec<PartMesh>,
    pub colors: Vec<Vec<[f32; 4]>>,
}

impl Dressed {
    /// `paint` = per-part voxel colours (painted costumes), blended `lantern` (0..1) toward the corps
    /// colour like the construct shader does; unpainted voxels use [`PAL`] by tint.
    pub fn new(label: String, sub: String, model: VoxelModel, paint: Option<&[HashMap<IVec3, Rgb>]>, lantern: f32) -> Dressed {
        let meshes = mesh_model(&model);
        let colors = meshes
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let cols = costume::mesh_colors(m, model.voxel_size, &PAL, paint.and_then(|p| p.get(i)));
                if lantern <= 0.0 {
                    return cols;
                }
                let k = lantern.min(1.0);
                cols.into_iter().map(|c| [c[0] * (1.0 - k) + PAL[0][0] * k, c[1] * (1.0 - k) + PAL[0][1] * k, c[2] * (1.0 - k) + PAL[0][2] * k, 1.0]).collect()
            })
            .collect();
        Dressed { label, sub, model, meshes, colors }
    }
}

/// How much a summoned costume's palette is pulled toward the lantern colour (the construct shader).
pub const SUMMON_LANTERN: f32 = 0.28;

/// A summoned costume construct (the recipe's model is a `costume:` id, `shape` its curated geometry
/// as the sim keeps it): built and painted like the client (`present/constructs.rs`).
pub fn summon_dressed(label: String, sub: String, r: &Recipe, shape: &ShapeSpec) -> Option<Dressed> {
    let (norm, model) = build_icon_model(r, shape);
    let paint = costume::summon::summon_colors(r, &norm, &model)?;
    Some(Dressed::new(label, sub, model, Some(&paint), SUMMON_LANTERN))
}

fn posed(d: &Dressed) -> Vec<Affine3A> {
    let mut pose_v = vec![Affine3A::IDENTITY; d.model.parts.len()];
    pose(&d.model, AnimState::Idle, 0.0, &mut pose_v);
    pose_v
}

/// Draw `d` from `eye` into the tile at a fixed `scale` (px per model unit) with the feet on the
/// tile's ground line; models taller than the tile are fitted instead.
#[allow(clippy::too_many_arguments)]
pub fn draw_view(c: &mut Canvas, d: &Dressed, eye: Vec3, x0: i32, y0: i32, w: i32, h: i32, scale: f32) {
    let height = d.model.bounds.max.y - d.model.bounds.min.y;
    let scale = scale.min((h as f32 - 24.0) / height.max(0.1));
    let ox = x0 as f32 + w as f32 * 0.5;
    let oy = (y0 + h) as f32 - 10.0;
    draw_with(c, d, &Cam::from_eye(eye), &posed(d), scale, ox, oy, (x0, y0, x0 + w, y0 + h));
}

/// Draw `d` in a given pose (`pose_v`, one per part) from `eye` at a fixed `scale` (px per model
/// unit), the model origin (feet) `ground` px above the tile's bottom (ANIMATION PASS frame sheets).
#[allow(clippy::too_many_arguments)]
pub fn draw_posed(c: &mut Canvas, d: &Dressed, pose_v: &[Affine3A], eye: Vec3, x0: i32, y0: i32, w: i32, h: i32, scale: f32, ground: f32) {
    let ox = x0 as f32 + w as f32 * 0.5;
    let oy = (y0 + h) as f32 - ground;
    draw_with(c, d, &Cam::from_eye(eye), pose_v, scale, ox, oy, (x0, y0, x0 + w, y0 + h));
}

/// Draw `d` from `eye` fitted (and centred) into the tile with `margin` px around it.
#[allow(clippy::too_many_arguments)]
pub fn draw_fit(c: &mut Canvas, d: &Dressed, eye: Vec3, x0: i32, y0: i32, w: i32, h: i32, margin: f32) {
    let cam = Cam::from_eye(eye);
    let pose_v = posed(d);
    let (mut mnx, mut mny, mut mxx, mut mxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (pi, m) in d.meshes.iter().enumerate() {
        for p in m.positions.iter().step_by(2) {
            let (x, y, _) = cam.proj(pose_v[pi].transform_point3(Vec3::from_array(*p)));
            mnx = mnx.min(x);
            mxx = mxx.max(x);
            mny = mny.min(y);
            mxy = mxy.max(y);
        }
    }
    if mnx > mxx {
        return;
    }
    let scale = ((w as f32 - 2.0 * margin) / (mxx - mnx).max(0.01)).min((h as f32 - 2.0 * margin) / (mxy - mny).max(0.01));
    let ox = x0 as f32 + w as f32 * 0.5 - (mnx + mxx) * 0.5 * scale;
    let oy = y0 as f32 + h as f32 * 0.5 + (mny + mxy) * 0.5 * scale;
    draw_with(c, d, &cam, &pose_v, scale, ox, oy, (x0, y0, x0 + w, y0 + h));
}

#[allow(clippy::too_many_arguments)]
fn draw_with(c: &mut Canvas, d: &Dressed, cam: &Cam, pose_v: &[Affine3A], scale: f32, ox: f32, oy: f32, clip: (i32, i32, i32, i32)) {
    c.clear_depth();
    let light = Vec3::new(-0.45, 0.8, -0.55).normalize();
    for (pi, m) in d.meshes.iter().enumerate() {
        let tr = pose_v[pi];
        let nm = tr.matrix3;
        let sv: Vec<(f32, f32, f32)> = m
            .positions
            .iter()
            .map(|p| {
                let (x, y, z) = cam.proj(tr.transform_point3(Vec3::from_array(*p)));
                (ox + x * scale, oy - y * scale, z)
            })
            .collect();
        for tri in m.indices.chunks(3) {
            let (a, b, e) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let n = (nm * Vec3::from_array(m.normals[a])).normalize_or(Vec3::Y);
            if n.dot(-cam.fwd) <= 0.0 {
                continue;
            }
            let lit = 0.55 + 0.5 * n.dot(light).max(0.0) + 0.08 * n.dot(-cam.fwd);
            let col = d.colors[pi][a];
            raster(c, [sv[a], sv[b], sv[e]], [m.uvs[a], m.uvs[b], m.uvs[e]], [col[0], col[1], col[2]], lit, clip);
        }
    }
}

pub fn raster(c: &mut Canvas, v: [(f32, f32, f32); 3], uv: [[f32; 2]; 3], base: [f32; 3], lit: f32, clip: (i32, i32, i32, i32)) {
    let (cx0, cy0, cx1, cy1) = clip;
    let minx = v.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor().max(cx0 as f32) as i32;
    let maxx = v.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil().min(cx1 as f32 - 1.0) as i32;
    let miny = v.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(cy0 as f32) as i32;
    let maxy = v.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().min(cy1 as f32 - 1.0) as i32;
    let area = (v[1].0 - v[0].0) * (v[2].1 - v[0].1) - (v[2].0 - v[0].0) * (v[1].1 - v[0].1);
    if area.abs() < 1e-6 {
        return;
    }
    for y in miny..=maxy {
        for x in minx..=maxx {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let w0 = ((v[1].0 - px) * (v[2].1 - py) - (v[2].0 - px) * (v[1].1 - py)) / area;
            let w1 = ((v[2].0 - px) * (v[0].1 - py) - (v[0].0 - px) * (v[2].1 - py)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let z = w0 * v[0].2 + w1 * v[1].2 + w2 * v[2].2;
            let idx = y as usize * c.w + x as usize;
            if z >= c.z[idx] {
                continue;
            }
            c.z[idx] = z;
            let u = w0 * uv[0][0] + w1 * uv[1][0] + w2 * uv[2][0];
            let vv = w0 * uv[0][1] + w1 * uv[1][1] + w2 * uv[2][1];
            let e = u.min(1.0 - u).min(vv).min(1.0 - vv);
            let edge = if e < 0.07 { 0.86 } else { 1.0 };
            let k = lit * edge;
            let to8 = |f: f32| (f.powf(1.0 / 1.1).clamp(0.0, 1.0) * 255.0) as u8;
            c.px[idx] = [to8(base[0] * k), to8(base[1] * k), to8(base[2] * k)];
        }
    }
}

/// A 3/4 camera that suits the form (held items from the side, worn from behind, the rest 3/4 front).
pub fn eye_for(form: Form) -> Vec3 {
    match form {
        Form::Shield | Form::Gauntlet => return Vec3::new(0.9, 0.45, -1.0),
        Form::Disc => return Vec3::new(0.5, 1.3, -0.5),
        Form::Fish | Form::Rocket => return Vec3::new(1.0, 0.35, -0.4),
        _ => {}
    }
    match form.category() {
        FormCategory::Held => Vec3::new(1.0, 0.45, -0.3),
        FormCategory::Worn => Vec3::new(0.9, 0.55, 1.1),
        _ => Vec3::new(0.95, 0.6, -1.1),
    }
}

// -------------------------------------------------------------------------------------- font

pub fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 17, 25, 21, 19, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [31, 2, 4, 2, 1, 17, 14],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 30, 1, 1, 17, 14],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 12, 4, 8],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '#' => [10, 10, 31, 10, 31, 10, 10],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '\'' => [4, 4, 8, 0, 0, 0, 0],
        '"' => [10, 10, 0, 0, 0, 0, 0],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '%' => [24, 25, 2, 4, 8, 19, 3],
        '>' => [8, 4, 2, 1, 2, 4, 8],
        '<' => [2, 4, 8, 16, 8, 4, 2],
        '*' => [0, 21, 14, 31, 14, 21, 0],
        _ => [0; 7],
    }
}

/// Text at (x, y), `k` px per font pixel.
pub fn text(c: &mut Canvas, x: i32, y: i32, s: &str, col: [u8; 3], k: i32) {
    let mut cx = x;
    for ch in s.chars() {
        let g = glyph(ch);
        for (r, bits) in g.iter().enumerate() {
            for b in 0..5 {
                if bits & (1 << (4 - b)) != 0 {
                    c.rect(cx + b * k, y + r as i32 * k, k, k, col);
                }
            }
        }
        cx += 6 * k;
    }
}

pub fn text_w(s: &str, k: i32) -> i32 {
    s.chars().count() as i32 * 6 * k - k
}
