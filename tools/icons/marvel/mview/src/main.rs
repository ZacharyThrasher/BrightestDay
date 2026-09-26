//! Contact sheets for bd_shapes, software-rasterised (no GPU).
//!
//! cargo run -p bd_shapes --example gallery --release -- [--forms] [--anims] [--icons] [--combos]
//!     [--filter held|worn|creature|structure|object|<FormName>] [--tile N] [--out PATH]
//!
//! Default: `--forms --anims` → docs/gallery_forms.png + docs/gallery_anims.png.

use std::path::PathBuf;

use bd_core::mechanics::{Form, FormCategory, Mechanic, ModelSource, Recipe};
use bd_core::shape::{PartRole, ShapeSpec};
use bd_shapes::*;
use glam::{Affine3A, Vec3};

// ------------------------------------------------------------------------------------ raster

const BG: [u8; 3] = [16, 18, 24];
const DEF_EYE: Vec3 = Vec3::new(1.0, 0.45, -0.75);

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[u8; 3]>,
    z: Vec<f32>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, px: vec![BG; w * h], z: vec![f32::INFINITY; w * h] }
    }
    fn clear_depth(&mut self) {
        self.z.fill(f32::INFINITY);
    }
    fn put(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }
    fn rect(&mut self, x0: i32, y0: i32, w: i32, h: i32, c: [u8; 3]) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.put(x, y, c);
            }
        }
    }
    fn frame(&mut self, x0: i32, y0: i32, w: i32, h: i32, c: [u8; 3]) {
        for x in x0..x0 + w {
            self.put(x, y0, c);
            self.put(x, y0 + h - 1, c);
        }
        for y in y0..y0 + h {
            self.put(x0, y, c);
            self.put(x0 + w - 1, y, c);
        }
    }
    fn save(&self, path: &PathBuf) {
        let mut img = image::RgbImage::new(self.w as u32, self.h as u32);
        for (i, p) in self.px.iter().enumerate() {
            img.put_pixel((i % self.w) as u32, (i / self.w) as u32, image::Rgb(*p));
        }
        img.save(path).expect("save png");
        println!("wrote {}", path.display());
    }
}

#[derive(Clone, Copy)]
struct Cam {
    right: Vec3,
    up: Vec3,
    fwd: Vec3,
}

impl Cam {
    fn from_eye(eye_dir: Vec3) -> Cam {
        let fwd = -eye_dir.normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        let up = right.cross(fwd).normalize();
        Cam { right, up, fwd }
    }
    fn proj(&self, p: Vec3) -> (f32, f32, f32) {
        (p.dot(self.right), p.dot(self.up), p.dot(self.fwd))
    }
}

#[derive(Clone, Copy)]
struct Fit {
    scale: f32,
    ox: f32,
    oy: f32,
}

type Palette = [[f32; 3]; 4];
const GREEN: Palette = [[0.2, 0.82, 0.38], [0.62, 1.0, 0.68], [0.05, 0.3, 0.13], [0.93, 1.0, 0.95]];
const GREY: Palette = [[0.32, 0.34, 0.38], [0.42, 0.44, 0.48], [0.22, 0.23, 0.26], [0.6, 0.6, 0.62]];

struct Item<'a> {
    #[allow(dead_code)]
    model: &'a VoxelModel,
    meshes: &'a [PartMesh],
    pose: Vec<Affine3A>,
    root: Affine3A,
    pal: Palette,
}

fn world_points(it: &Item) -> Vec<Vec3> {
    let mut v = Vec::new();
    for (i, m) in it.meshes.iter().enumerate() {
        let tr = it.root * it.pose[i];
        for p in m.positions.iter().step_by(4) {
            v.push(tr.transform_point3(Vec3::from_array(*p)));
        }
    }
    v
}

fn fit_points(cam: &Cam, pts: &[Vec3], x0: f32, y0: f32, w: f32, h: f32, margin: f32) -> Fit {
    let (mut mnx, mut mny, mut mxx, mut mxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts {
        let (x, y, _) = cam.proj(*p);
        mnx = mnx.min(x);
        mxx = mxx.max(x);
        mny = mny.min(y);
        mxy = mxy.max(y);
    }
    if mnx > mxx {
        return Fit { scale: 1.0, ox: x0 + w * 0.5, oy: y0 + h * 0.5 };
    }
    let sw = (mxx - mnx).max(0.01);
    let sh = (mxy - mny).max(0.01);
    let scale = ((w - 2.0 * margin) / sw).min((h - 2.0 * margin) / sh);
    let cx = (mnx + mxx) * 0.5;
    let cy = (mny + mxy) * 0.5;
    Fit { scale, ox: x0 + w * 0.5 - cx * scale, oy: y0 + h * 0.5 + cy * scale }
}

fn draw(c: &mut Canvas, cam: &Cam, fit: Fit, it: &Item, clip: (i32, i32, i32, i32)) {
    let light = Vec3::new(-0.45, 0.8, -0.35).normalize();
    for (pi, m) in it.meshes.iter().enumerate() {
        let tr = it.root * it.pose[pi];
        let nm = tr.matrix3;
        let sv: Vec<(f32, f32, f32)> = m
            .positions
            .iter()
            .map(|p| {
                let w = tr.transform_point3(Vec3::from_array(*p));
                let (x, y, z) = cam.proj(w);
                (fit.ox + x * fit.scale, fit.oy - y * fit.scale, z)
            })
            .collect();
        for tri in m.indices.chunks(3) {
            let (a, b, d) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let n = (nm * Vec3::from_array(m.normals[a])).normalize_or(Vec3::Y);
            if n.dot(-cam.fwd) <= 0.0 {
                continue;
            }
            let tint = m.tints[a] as usize;
            let lit = if tint == 3 { 1.0 } else { 0.42 + 0.62 * n.dot(light).max(0.0) + 0.1 * n.dot(-cam.fwd) };
            let base = it.pal[tint];
            raster_tri(c, [sv[a], sv[b], sv[d]], [m.shade[a], m.shade[b], m.shade[d]], [m.uvs[a], m.uvs[b], m.uvs[d]], base, lit, tint == 3, clip);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn raster_tri(c: &mut Canvas, v: [(f32, f32, f32); 3], sh: [f32; 3], uv: [[f32; 2]; 3], base: [f32; 3], lit: f32, core: bool, clip: (i32, i32, i32, i32)) {
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
            let ao = w0 * sh[0] + w1 * sh[1] + w2 * sh[2];
            let u = w0 * uv[0][0] + w1 * uv[1][0] + w2 * uv[2][0];
            let vv = w0 * uv[0][1] + w1 * uv[1][1] + w2 * uv[2][1];
            let e = u.min(1.0 - u).min(vv).min(1.0 - vv);
            let edge = if e < 0.09 { if core { 0.85 } else { 0.72 } } else { 1.0 };
            let k = if core { edge } else { lit * ao * edge };
            let col = [base[0] * k, base[1] * k, base[2] * k];
            c.px[idx] = [(col[0].clamp(0.0, 1.0) * 255.0) as u8, (col[1].clamp(0.0, 1.0) * 255.0) as u8, (col[2].clamp(0.0, 1.0) * 255.0) as u8];
        }
    }
}

// -------------------------------------------------------------------------------------- font

fn glyph(ch: char) -> [u8; 7] {
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
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '#' => [10, 10, 31, 10, 31, 10, 10],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        _ => [0; 7],
    }
}

fn text(c: &mut Canvas, x: i32, y: i32, s: &str, col: [u8; 3]) {
    let mut cx = x;
    for ch in s.chars() {
        let g = glyph(ch);
        for (r, bits) in g.iter().enumerate() {
            for b in 0..5 {
                if bits & (1 << (4 - b)) != 0 {
                    c.put(cx + b, y + r as i32, col);
                }
            }
        }
        cx += 6;
    }
}

// ----------------------------------------------------------------------------------- scenes

fn eye_for(form: Form) -> Vec3 {
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

fn recipe(form: Form, seed: u32, mech: &[Mechanic]) -> Recipe {
    demo_recipe(form, seed, mech)
}

/// A grey stand-in wearer for worn items / the swing strip (origin: upper back, units: voxels).
fn mannequin_spec(for_worn: bool) -> ShapeSpec {
    use bd_core::shape::{Prim, PrimKind, Tint};
    let mut s = ShapeSpec::default();
    let mut add = |c: Vec3, sz: Vec3| {
        s.prims.push(Prim { kind: PrimKind::Box, center: c, size: sz, rot_deg: Vec3::ZERO, tint: Tint::Primary, part: 0, subtract: false });
    };
    if for_worn {
        // Origin = upper back; body centre slightly in front.
        add(Vec3::new(0.0, -1.5, -0.5), Vec3::new(3.0, 4.0, 2.0));
        add(Vec3::new(0.0, 1.5, -0.5), Vec3::new(2.0, 2.0, 2.0));
        add(Vec3::new(-1.0, -5.5, -0.5), Vec3::new(1.0, 4.0, 1.0));
        add(Vec3::new(1.0, -5.5, -0.5), Vec3::new(1.0, 4.0, 1.0));
        add(Vec3::new(-2.0, -1.5, -0.5), Vec3::new(1.0, 4.0, 1.0));
        add(Vec3::new(2.0, -1.5, -0.5), Vec3::new(1.0, 4.0, 1.0));
    } else {
        // Origin = feet.
        add(Vec3::new(0.0, 4.5, 0.0), Vec3::new(3.0, 4.0, 2.0));
        add(Vec3::new(0.0, 7.5, 0.0), Vec3::new(2.0, 2.0, 2.0));
        add(Vec3::new(-1.0, 1.5, 0.0), Vec3::new(1.0, 4.0, 1.0));
        add(Vec3::new(1.0, 1.5, 0.0), Vec3::new(1.0, 4.0, 1.0));
    }
    s
}

struct Built {
    label: String,
    form: Form,
    model: VoxelModel,
    meshes: Vec<PartMesh>,
}

fn build(label: String, r: &Recipe) -> Built {
    let (_, model) = build_model(r);
    let meshes = mesh_model(&model);
    Built { label, form: r.form, model, meshes }
}

fn build_spec(label: String, form: Form, spec: &ShapeSpec) -> Built {
    let model = voxelize_capped(spec, voxel_cap(form));
    let meshes = mesh_model(&model);
    Built { label, form, model, meshes }
}

fn render_sheet(items: &[Built], tile: usize, cols: usize, title: &str, out: &PathBuf) {
    let rows = items.len().div_ceil(cols);
    let head = 22;
    let mut c = Canvas::new(cols * tile, rows * tile + head);
    text(&mut c, 6, 7, title, [200, 220, 200]);
    let man = voxelize(&mannequin_spec(true));
    let man_mesh = mesh_model(&man);
    for (k, b) in items.iter().enumerate() {
        let x0 = (k % cols * tile) as i32;
        let y0 = (k / cols * tile + head) as i32;
        c.rect(x0 + 1, y0 + 1, tile as i32 - 2, tile as i32 - 2, [22, 25, 32]);
        c.frame(x0, y0, tile as i32, tile as i32, [40, 44, 54]);
        let cam = Cam::from_eye(eye_for(b.form));
        let mut pose = vec![Affine3A::IDENTITY; b.model.parts.len()];
        rest_pose(&b.model, &mut pose);
        let it = Item { model: &b.model, meshes: &b.meshes, pose, root: Affine3A::IDENTITY, pal: GREEN };
        let mut pts = world_points(&it);
        let worn = b.form.category() == FormCategory::Worn;
        let man_it = Item { model: &man, meshes: &man_mesh, pose: vec![Affine3A::IDENTITY; man.parts.len()], root: Affine3A::IDENTITY, pal: GREY };
        if worn {
            pts.extend(world_points(&man_it));
        }
        let fit = fit_points(&cam, &pts, x0 as f32, y0 as f32 + 12.0, tile as f32, tile as f32 - 26.0, 6.0);
        c.clear_depth();
        let clip = (x0 + 1, y0 + 1, x0 + tile as i32 - 1, y0 + tile as i32 - 1);
        if worn {
            draw(&mut c, &cam, fit, &man_it, clip);
        }
        draw(&mut c, &cam, fit, &it, clip);
        text(&mut c, x0 + 4, y0 + 4, &b.label, [220, 235, 220]);
        let info = format!("{}V {}P", b.model.voxel_count, b.model.parts.len());
        text(&mut c, x0 + 4, y0 + tile as i32 - 11, &info, [110, 130, 120]);
    }
    c.save(out);
}

// ------------------------------------------------------------------------------------- anims

enum Strip {
    Creature { label: &'static str, recipe: Recipe, anim: fn(&VoxelModel, f32) -> (AnimState, f32), dur: fn(&VoxelModel) -> f32, timing: Option<fn(&VoxelModel) -> AttackTiming>, eye: Vec3 },
    Swing { label: &'static str, form: Form, kind: SwingKind, eye: Vec3 },
}

fn render_anims(tile: usize, out: &PathBuf, only: Option<&str>) {
    let frames = 8usize;
    let strips: Vec<Strip> = vec![
        Strip::Creature {
            label: "KAIJU STOMP",
            recipe: recipe(Form::Kaiju, 1, &[Mechanic::Stomp, Mechanic::ArticulatedTail]),
            anim: |_, t| (AnimState::Attack(Attack::Stomp), t),
            dur: |m| attack_timing(m, Attack::Stomp).total(),
            timing: Some(|m| attack_timing(m, Attack::Stomp)),
            eye: Vec3::new(1.0, 0.3, -0.6),
        },
        Strip::Creature {
            label: "QUADRUPED RUN (GALLOP 9 M/S)",
            recipe: recipe(Form::Quadruped, 1, &[]),
            anim: |m, t| {
                let v = 9.0;
                let _ = m;
                (AnimState::Locomotion { speed: v, distance: v * t, airborne: false }, t)
            },
            dur: |m| gait_period(m, 9.0),
            timing: None,
            eye: Vec3::new(1.0, 0.25, -0.25),
        },
        Strip::Creature {
            label: "QUADRUPED WALK (1.5 M/S)",
            recipe: recipe(Form::Quadruped, 2, &[]),
            anim: |_, t| (AnimState::Locomotion { speed: 1.5, distance: 1.5 * t, airborne: false }, t),
            dur: |m| gait_period(m, 1.5),
            timing: None,
            eye: Vec3::new(1.0, 0.25, -0.25),
        },
        Strip::Creature {
            label: "SERPENT SLITHER",
            recipe: recipe(Form::Serpent, 1, &[]),
            anim: |_, t| (AnimState::Locomotion { speed: 4.0, distance: 4.0 * t, airborne: false }, t),
            dur: |m| m.rig.length * 0.8 / 4.0,
            timing: None,
            eye: Vec3::new(0.35, 1.3, -0.4),
        },
        Strip::Swing { label: "HAMMER OVERHEAD", form: Form::Hammer, kind: SwingKind::Primary, eye: Vec3::new(1.0, 0.25, 0.1) },
        Strip::Swing { label: "SWORD COMBO 1 (TOP VIEW)", form: Form::Sword, kind: SwingKind::Combo(0), eye: Vec3::new(0.15, 1.0, 0.45) },
        Strip::Swing { label: "AXE CLEAVE (TOP VIEW)", form: Form::Axe, kind: SwingKind::Primary, eye: Vec3::new(0.15, 1.0, 0.45) },
        Strip::Swing { label: "SPEAR THRUST", form: Form::Spear, kind: SwingKind::Primary, eye: Vec3::new(1.0, 0.3, 0.1) },
        Strip::Swing { label: "GAUNTLET UPPERCUT", form: Form::Gauntlet, kind: SwingKind::Secondary, eye: Vec3::new(1.0, 0.3, 0.1) },
        Strip::Creature {
            label: "BIPED RUN (6 M/S)",
            recipe: recipe(Form::Biped, 1, &[]),
            anim: |_, t| (AnimState::Locomotion { speed: 6.0, distance: 6.0 * t, airborne: false }, t),
            dur: |m| gait_period(m, 6.0),
            timing: None,
            eye: Vec3::new(1.0, 0.3, -0.35),
        },
        Strip::Creature {
            label: "BIRD FLAP",
            recipe: recipe(Form::Bird, 1, &[]),
            anim: |_, t| (AnimState::Locomotion { speed: 3.0, distance: 3.0 * t, airborne: true }, t),
            dur: |m| 1.0 / (3.2 / m.rig.height.sqrt()).clamp(1.2, 5.0),
            timing: None,
            eye: Vec3::new(0.4, 0.3, -1.0),
        },
        Strip::Creature {
            label: "SPIDER WALK",
            recipe: recipe(Form::Arachnid, 1, &[]),
            anim: |_, t| (AnimState::Locomotion { speed: 3.0, distance: 3.0 * t, airborne: false }, t),
            dur: |m| gait_period(m, 3.0),
            timing: None,
            eye: DEF_EYE,
        },
        Strip::Creature {
            label: "KAIJU TAIL SWEEP",
            recipe: recipe(Form::Kaiju, 2, &[]),
            anim: |_, t| (AnimState::Attack(Attack::TailSweep), t),
            dur: |m| attack_timing(m, Attack::TailSweep).total(),
            timing: Some(|m| attack_timing(m, Attack::TailSweep)),
            eye: Vec3::new(0.6, 1.0, -0.6),
        },
        Strip::Creature {
            label: "KAIJU BREATH (AIM LEFT)",
            recipe: recipe(Form::Kaiju, 3, &[Mechanic::Breath]),
            anim: |_, t| (AnimState::Attack(Attack::Breath { aim: Vec3::new(-0.6, -0.2, -1.0) }), t),
            dur: |m| attack_timing(m, Attack::Breath { aim: Vec3::NEG_Z }).total(),
            timing: Some(|m| attack_timing(m, Attack::Breath { aim: Vec3::NEG_Z })),
            eye: DEF_EYE,
        },
        Strip::Creature {
            label: "BULL CHARGE",
            recipe: recipe(Form::Quadruped, 5, &[Mechanic::Charge]),
            anim: |_, t| (AnimState::Attack(Attack::Charge), t),
            dur: |m| attack_timing(m, Attack::Charge).total(),
            timing: Some(|m| attack_timing(m, Attack::Charge)),
            eye: Vec3::new(1.0, 0.3, -0.3),
        },
        Strip::Creature {
            label: "WOLF BITE",
            recipe: recipe(Form::Quadruped, 1, &[]),
            anim: |_, t| (AnimState::Attack(Attack::Bite), t),
            dur: |m| attack_timing(m, Attack::Bite).total(),
            timing: Some(|m| attack_timing(m, Attack::Bite)),
            eye: Vec3::new(1.0, 0.3, -0.3),
        },
        Strip::Creature {
            label: "BIPED SWIPE",
            recipe: recipe(Form::Biped, 3, &[]),
            anim: |_, t| (AnimState::Attack(Attack::Swipe), t),
            dur: |m| attack_timing(m, Attack::Swipe).total(),
            timing: Some(|m| attack_timing(m, Attack::Swipe)),
            eye: DEF_EYE,
        },
        Strip::Creature {
            label: "SLIME HOP",
            recipe: recipe(Form::Blob, 1, &[]),
            anim: |_, t| (AnimState::Locomotion { speed: 2.0, distance: 2.0 * t, airborne: false }, t),
            dur: |m| m.rig.height * 1.4 / 2.0,
            timing: None,
            eye: DEF_EYE,
        },
        Strip::Creature {
            label: "FORMING (BIPED)",
            recipe: recipe(Form::Biped, 2, &[Mechanic::Flies]),
            anim: |_, t| (AnimState::Forming { duration: 0.6 }, t),
            dur: |_| 0.7,
            timing: None,
            eye: DEF_EYE,
        },
        Strip::Creature {
            label: "HIT REACT",
            recipe: recipe(Form::Biped, 1, &[]),
            anim: |_, t| (AnimState::HitReact { dir: Vec3::new(1.0, 0.0, 0.3), strength: 1.0 }, t),
            dur: |_| 0.5,
            timing: None,
            eye: DEF_EYE,
        },
    ];
    let strips: Vec<Strip> = strips
        .into_iter()
        .filter(|s| {
            let l = match s {
                Strip::Creature { label, .. } | Strip::Swing { label, .. } => label.to_lowercase(),
            };
            only.is_none_or(|o| o.split(',').any(|w| l.contains(w)))
        })
        .collect();
    let label_w = 0;
    let head = 14;
    let rows = strips.len();
    let mut c = Canvas::new(label_w + frames * tile, rows * (tile + head) + 4);
    let man_feet = voxelize(&mannequin_spec(false));
    let man_mesh = mesh_model(&man_feet);
    for (r, s) in strips.iter().enumerate() {
        let y0 = (r * (tile + head)) as i32;
        match s {
            Strip::Creature { label, recipe, anim, dur, timing, eye } => {
                let b = build(label.to_string(), recipe);
                let d = dur(&b.model);
                let tm = timing.map(|f| f(&b.model));
                text(&mut c, 4, y0 + 3, &format!("{}  ({:.2}S)", label, d), [220, 235, 220]);
                let cam = Cam::from_eye(*eye);
                let n = b.model.parts.len();
                let mut poses = Vec::new();
                let mut pts = Vec::new();
                for f in 0..frames {
                    let t = match tm {
                        // Key moments: rest, anticipation build, coil, release, impact, follow-through, settle.
                        Some(tm) => {
                            let (a, i) = (tm.anticipation, tm.impact());
                            [0.0, a * 0.45, a * 0.85, a + (i - a) * 0.4, i, i + tm.recovery * 0.12, i + tm.recovery * 0.45, d][f.min(7)]
                        }
                        None => d * f as f32 / (frames - 1) as f32,
                    };
                    let (a, tt) = anim(&b.model, t);
                    let mut p = vec![Affine3A::IDENTITY; n];
                    pose(&b.model, a, tt, &mut p);
                    let it = Item { model: &b.model, meshes: &b.meshes, pose: p.clone(), root: Affine3A::IDENTITY, pal: GREEN };
                    pts.extend(world_points(&it));
                    poses.push((t, p));
                }
                // Keep the ground in frame.
                pts.push(Vec3::ZERO);
                let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32, 8.0);
                for (f, (t, p)) in poses.into_iter().enumerate() {
                    let x0 = (label_w + f * tile) as i32;
                    let ty = y0 + head as i32;
                    c.rect(x0 + 1, ty + 1, tile as i32 - 2, tile as i32 - 2, [22, 25, 32]);
                    c.frame(x0, ty, tile as i32, tile as i32, [40, 44, 54]);
                    let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + ty as f32 };
                    // Ground line through the origin.
                    let gy = fit.oy.round() as i32;
                    for x in x0 + 2..x0 + tile as i32 - 2 {
                        c.put(x, gy, [48, 56, 60]);
                    }
                    c.clear_depth();
                    let it = Item { model: &b.model, meshes: &b.meshes, pose: p, root: Affine3A::IDENTITY, pal: GREEN };
                    draw(&mut c, &cam, fit, &it, (x0 + 1, ty + 1, x0 + tile as i32 - 1, ty + tile as i32 - 1));
                    let phase = tm.map(|tm| {
                        if t < tm.anticipation {
                            ("ANT", [230, 200, 90])
                        } else if t < tm.anticipation + tm.active {
                            ("ACT", [255, 90, 90])
                        } else {
                            ("REC", [120, 170, 230])
                        }
                    });
                    text(&mut c, x0 + 4, ty + tile as i32 - 11, &format!("{:.2}", t), [140, 160, 150]);
                    if let Some((p, col)) = phase {
                        text(&mut c, x0 + tile as i32 - 24, ty + tile as i32 - 11, p, col);
                    }
                }
            }
            Strip::Swing { label, form, kind, eye } => {
                let rcp = recipe(*form, 1, &[]);
                let b = build(label.to_string(), &rcp);
                let curve = swing_curve(*form, *kind);
                let d = curve.duration();
                text(&mut c, 4, y0 + 3, &format!("{}  ANT {:.0}MS  ACT {:.0}MS  REC {:.0}MS", label, curve.anticipation * 1000.0, curve.active * 1000.0, curve.recovery * 1000.0), [220, 235, 220]);
                let cam = Cam::from_eye(*eye);
                // Hand at the right shoulder-ish of the mannequin (m).
                let hand = Affine3A::from_translation(Vec3::new(0.35, 1.45, -0.1));
                let n = b.model.parts.len();
                let man_it_pose = vec![Affine3A::IDENTITY; man_feet.parts.len()];
                // Sample frames: 2 anticipation, 3 active, 3 recovery for readability.
                let (a0, a1) = curve.active_window();
                let ts = [0.0, a0 * 0.6, a0, a0 + (a1 - a0) * 0.35, a0 + (a1 - a0) * 0.7, a1, a1 + (d - a1) * 0.3, d];
                let mut pts = Vec::new();
                let mut man_pts = world_points(&Item { model: &man_feet, meshes: &man_mesh, pose: man_it_pose.clone(), root: Affine3A::IDENTITY, pal: GREY });
                pts.append(&mut man_pts);
                for &t in &ts {
                    let it = Item { model: &b.model, meshes: &b.meshes, pose: vec![Affine3A::IDENTITY; n], root: hand * curve.sample(t), pal: GREEN };
                    pts.extend(world_points(&it));
                }
                let fit0 = fit_points(&cam, &pts, 0.0, 0.0, tile as f32, tile as f32, 8.0);
                for (f, &t) in ts.iter().enumerate() {
                    let x0 = (label_w + f * tile) as i32;
                    let ty = y0 + head as i32;
                    c.rect(x0 + 1, ty + 1, tile as i32 - 2, tile as i32 - 2, [22, 25, 32]);
                    c.frame(x0, ty, tile as i32, tile as i32, [40, 44, 54]);
                    let fit = Fit { scale: fit0.scale, ox: fit0.ox + x0 as f32, oy: fit0.oy + ty as f32 };
                    c.clear_depth();
                    let clip = (x0 + 1, ty + 1, x0 + tile as i32 - 1, ty + tile as i32 - 1);
                    // Ghost of the previous frame for arc reading.
                    if f > 0 {
                        let prev = Item { model: &b.model, meshes: &b.meshes, pose: vec![Affine3A::IDENTITY; n], root: hand * curve.sample(ts[f - 1]), pal: [[0.1, 0.25, 0.14]; 4] };
                        draw(&mut c, &cam, fit, &prev, clip);
                        c.clear_depth();
                    }
                    draw(&mut c, &cam, fit, &Item { model: &man_feet, meshes: &man_mesh, pose: man_it_pose.clone(), root: Affine3A::IDENTITY, pal: GREY }, clip);
                    draw(&mut c, &cam, fit, &Item { model: &b.model, meshes: &b.meshes, pose: vec![Affine3A::IDENTITY; n], root: hand * curve.sample(t), pal: GREEN }, clip);
                    let (p, col) = match curve.phase(t) {
                        SwingPhase::Anticipation => ("ANT", [230, 200, 90]),
                        SwingPhase::Active => ("ACT", [255, 90, 90]),
                        _ => ("REC", [120, 170, 230]),
                    };
                    text(&mut c, x0 + 4, ty + tile as i32 - 11, &format!("{:.0}MS", t * 1000.0), [140, 160, 150]);
                    text(&mut c, x0 + tile as i32 - 24, ty + tile as i32 - 11, p, col);
                }
            }
        }
    }
    c.save(out);
}

fn gait_period(m: &VoxelModel, speed: f32) -> f32 {
    // One full gait cycle at `speed`: sample distance until the root pose repeats — approximate with
    // the rig's analytic stride (same formula as anim.rs).
    let ll = m.rig.leg_len.max(0.1);
    let v_rel = speed / ll;
    let (stance, amp) = match m.rig.gait {
        Gait::Biped => (0.62 - 0.24 * smooth(2.0, 5.0, v_rel), (0.22 + 0.07 * v_rel).clamp(0.22, 0.75)),
        Gait::Quadruped => {
            let mode = if v_rel < 2.2 { 0 } else if v_rel < 5.0 { 1 } else { 2 };
            ([0.7, 0.5, 0.36][mode], [0.28, 0.4, 0.6][mode])
        }
        _ => (0.55, 0.32),
    };
    let reach = if m.rig.splayed { m.rig.legs.iter().map(|l| l.reach).fold(0.0, f32::max).max(ll * 0.5) } else { ll };
    let cycle = 2.0 * reach * amp.sin() / stance;
    cycle / speed
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ------------------------------------------------------------------------------------- main

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let val = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let root = std::env::var("BD_ROOT").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let docs = root.join("docs");
    let tile: usize = val("--tile").and_then(|s| s.parse().ok()).unwrap_or(180);
    let filter = val("--filter").map(|s| s.to_lowercase());
    let any_mode = has("--forms") || has("--anims") || has("--icons") || has("--combos");
    let do_forms = has("--forms") || !any_mode;
    let do_anims = has("--anims") || !any_mode;

    let matches = |f: Form| -> bool {
        match filter.as_deref() {
            None => true,
            Some("held") => f.category() == FormCategory::Held,
            Some("worn") => f.category() == FormCategory::Worn,
            Some("creature") => f.category() == FormCategory::Creature,
            Some("structure") => f.category() == FormCategory::Structure,
            Some("object") => f.category() == FormCategory::Object,
            Some(name) => format!("{f:?}").to_lowercase() == name,
        }
    };

    if do_forms {
        let mut items = Vec::new();
        for f in ALL_FORMS.iter().copied().filter(|f| matches(*f)) {
            for seed in [1u32, 5] {
                items.push(build(format!("{f:?} #{seed}"), &recipe(f, seed, &[])));
            }
        }
        if filter.is_none() {
            use Mechanic::*;
            let variants: Vec<(&str, Form, u32, Vec<Mechanic>)> = vec![
                ("HAMMER+THRUST", Form::Hammer, 1, vec![Thrust]),
                ("SERPENT+FLIES", Form::Serpent, 3, vec![Flies]),
                ("QUAD+CHARGE", Form::Quadruped, 1, vec![Charge]),
                ("KAIJU+BREATH", Form::Kaiju, 4, vec![Breath]),
                ("KAIJU #5", Form::Kaiju, 5, vec![]),
                ("SPIDER+TAIL", Form::Arachnid, 1, vec![ArticulatedTail]),
            ];
            for (l, f, s, m) in variants {
                items.push(build(l.to_string(), &recipe(f, s, &m)));
            }
        }
        let out = val("--out").map(PathBuf::from).unwrap_or(docs.join("gallery_forms.png"));
        render_sheet(&items, tile, 10, "BRIGHTEST DAY - BD_SHAPES FORMS (2 SEEDS EACH + MECHANIC VARIANTS)", &out);
    }
    if has("--combos") {
        use Mechanic::*;
        let combos: Vec<(&str, Form, Vec<Mechanic>)> = vec![
            ("FLYING HAMMER", Form::Hammer, vec![Flies]),
            ("BOULDER+WINGS", Form::Boulder, vec![Flies]),
            ("WALL WITH LEGS", Form::Wall, vec![Walks]),
            ("HOMING TRAMPOLINE", Form::Trampoline, vec![Homing, Walks]),
            ("STICKY ROCKET", Form::Rocket, vec![Sticky]),
            ("SPINNING PILLAR", Form::Pillar, vec![Spin]),
            ("SHARK WITH WINGS", Form::Fish, vec![Flies]),
            ("CHARGING WALL", Form::Wall, vec![Charge, Walks]),
            ("BREATHING ANVIL", Form::Anvil, vec![Breath]),
            ("GRABBY CAGE", Form::Cage, vec![Grab, Flies]),
            ("BOUNCY BOMB", Form::Bomb, vec![Bounce]),
            ("SPIKY BALL", Form::Ball, vec![Pierce, Explodes]),
            ("TAILED HAMMER", Form::Hammer, vec![ArticulatedTail]),
            ("SHOOTING DOME", Form::Dome, vec![Shoots, Walks]),
            ("BLADED PLATFORM", Form::Platform, vec![Blade, Thrust]),
            ("UMBRELLA STAFF", Form::Staff, vec![SlowFall]),
            ("MAGNET HAMMER", Form::Hammer, vec![Attract]),
            ("REPEL WALL", Form::Wall, vec![Repel]),
            ("SHRINK RAY", Form::Gun, vec![Shrink]),
            ("GROW CANNON", Form::Cannon, vec![Grow]),
            ("LINKED ANVIL", Form::Anvil, vec![Link]),
            ("PORTAL PILLAR", Form::Pillar, vec![Portal]),
            ("PILOTED MECH", Form::Biped, vec![Piloted]),
            ("RIDEABLE PEGASUS", Form::Quadruped, vec![Piloted, Flies]),
            ("PILOTED TANK", Form::Vehicle, vec![Piloted]),
            ("MAGNET+PORTAL BLOB", Form::Blob, vec![Attract, Portal, Repel]),
        ];
        let items: Vec<Built> = combos
            .into_iter()
            .map(|(l, f, m)| {
                let mut r = recipe(f, 1, &m);
                if f == Form::Fish {
                    r.model = ModelSource::Procedural { seed: 4 };
                }
                if l == "PILOTED MECH" {
                    r.scale = 12.0;
                }
                build(l.to_string(), &r)
            })
            .collect();
        let out = val("--out").map(PathBuf::from).unwrap_or(docs.join("gallery_combos.png"));
        render_sheet(&items, tile, 8, "MECHANIC COMPONENTS ON ANY FORM", &out);
    }
    if do_anims {
        let out = if has("--forms") { docs.join("gallery_anims.png") } else { val("--out").map(PathBuf::from).unwrap_or(docs.join("gallery_anims.png")) };
        let only = val("--strip").map(|s| s.to_lowercase());
        render_anims(if has("--tile") { tile } else { 170 }, &out, only.as_deref());
    }
    if has("--icons") {
        icons(&root, tile, val("--out").map(PathBuf::from).unwrap_or(docs.join("gallery_icons.png")));
    }
}


#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct IconEntry { names: Vec<String>, recipe: Recipe, shape: Option<ShapeSpec>, source: String }

fn icons(_root: &std::path::Path, tile: usize, out: PathBuf) {
    let path = std::env::var("ICON_FILE").expect("ICON_FILE");
    let only = std::env::var("ICON_ONLY").ok();
    let src = std::fs::read_to_string(&path).unwrap();
    let entries: Vec<IconEntry> = ron::from_str(&src).unwrap();
    let items: Vec<Built> = entries.iter().filter(|e| e.shape.is_some())
        .filter(|e| only.as_ref().map_or(true, |o| o.split(',').any(|x| x == e.names[0])))
        .map(|e| build_spec(e.names[0].chars().take(26).collect::<String>().to_uppercase(), e.recipe.form, e.shape.as_ref().unwrap()))
        .collect();
    for b in &items {
        println!("{:28} voxels {:6} parts {:3} gait {:?}", b.label, b.model.voxel_count, b.model.parts.len(), b.model.rig.gait);
    }
    render_sheet(&items, tile, 6, "MARVEL.RON", &out);
}

fn guess_form(spec: &ShapeSpec) -> Form {
    if spec.parts.iter().any(|p| matches!(p.role, PartRole::LegBackLeft | PartRole::LegFrontLeft)) { Form::Kaiju } else { Form::Hammer }
}

fn collect_specs(v: &ron::Value, label: &str, out: &mut Vec<(String, ShapeSpec)>) {
    use ron::Value;
    match v {
        Value::Map(m) => {
            let mut name = label.to_string();
            for (k, val) in m.iter() {
                if let (Value::String(k), Value::String(s)) = (k, val) {
                    if k == "id" || k == "name" {
                        name = s.clone();
                    }
                }
            }
            let has_prims = m.iter().any(|(k, _)| matches!(k, Value::String(s) if s == "prims"));
            if has_prims {
                let text = ron::to_string(v).unwrap_or_default();
                match ron::from_str::<ShapeSpec>(&text) {
                    Ok(spec) => {
                        out.push((name, spec));
                        return;
                    }
                    Err(e) => println!("spec {name}: {e}"),
                }
            }
            for (k, val) in m.iter() {
                let sub = if let Value::String(k) = k { if name.is_empty() { k.clone() } else { name.clone() } } else { name.clone() };
                let sub = if let (Value::String(k), Value::Map(_)) = (k, val) { if k == "spec" || k == "shape" { name.clone() } else { k.clone() } } else { sub };
                collect_specs(val, &sub, out);
            }
        }
        Value::Seq(s) => {
            for x in s {
                collect_specs(x, label, out);
            }
        }
        Value::Option(Some(b)) => collect_specs(b, label, out),
        _ => {}
    }
}
