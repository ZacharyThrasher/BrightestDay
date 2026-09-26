//! Character costume gallery (software-rasterised, in-game costume resolution 1/16 m voxels).
//!
//! cargo run -p bd_shapes --example costume_gallery --release -- [--names gojo,vader] [--variants]
//!     [--before-after] [--derived N] [--out PATH]
//!
//! Default: every character in assets/costumes/characters.ron (first variant), front + 3/4 view with
//! the name under each → docs/costume_gallery.png.
//! `--variants`: every variant of every (or the named) character → docs/costume_variants.png.
//! `--before-after`: vocabulary costume (the old path) vs the character look for 6 characters →
//! docs/costume_before_after.png. `--derived N`: N humanoid icons through the derived tier →
//! docs/costume_derived.png.

use std::path::PathBuf;

use bd_core::mechanics::{Form, Recipe};
use bd_core::shape::ShapeSpec;
use bd_shapes::costume::{self, Costume, look};
use bd_shapes::*;
use glam::{Affine3A, Vec3};

const BG: [u8; 3] = [18, 20, 26];

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

/// Corps colour (green lantern) palette for unpainted voxels.
const PAL: [[f32; 3]; 4] = [[0.2, 0.82, 0.38], [0.62, 1.0, 0.68], [0.05, 0.3, 0.13], [0.93, 1.0, 0.95]];

struct Dressed {
    label: String,
    sub: String,
    model: VoxelModel,
    meshes: Vec<PartMesh>,
    colors: Vec<Vec<[f32; 4]>>,
}

fn prepare(label: String, sub: String, c: &Costume) -> Dressed {
    let pm = costume::costume_model_painted(c);
    let meshes = mesh_model(&pm.model);
    let colors = meshes.iter().enumerate().map(|(i, m)| costume::mesh_colors(m, pm.model.voxel_size, &PAL, pm.colors.get(i))).collect();
    Dressed { label, sub, model: pm.model, meshes, colors }
}

/// A summoned costume construct (`text` cast with a recipe of `scale` m), coloured like the
/// construct shader does: the costume palette blended 28 % toward the lantern (corps) colour.
fn prepare_summon(label: String, text: &str, scale: f32) -> Dressed {
    let mut r = Recipe::unknown(text);
    r.form = Form::Biped;
    r.scale = scale;
    let mut shape = None;
    assert!(costume::summon::apply(text, &mut r, &mut shape), "{text} is not a costume summon");
    let s = shape.expect("shape");
    let (norm, model) = build_icon_model(&r, &s);
    let paint = costume::summon::summon_colors(&r, &norm, &model).expect("colours");
    let meshes = mesh_model(&model);
    let colors = meshes
        .iter()
        .enumerate()
        .map(|(i, m)| {
            costume::mesh_colors(m, model.voxel_size, &PAL, paint.get(i))
                .into_iter()
                .map(|c| {
                    let k = 0.28;
                    [c[0] * (1.0 - k) + PAL[0][0] * k, c[1] * (1.0 - k) + PAL[0][1] * k, c[2] * (1.0 - k) + PAL[0][2] * k, 1.0]
                })
                .collect()
        })
        .collect();
    let sub = format!("SUMMON {:.0} M  {}V  {:.3} M VOXELS", scale, model.voxel_count, model.voxel_size);
    Dressed { label, sub, model, meshes, colors }
}

#[allow(clippy::too_many_arguments)]
fn draw_view(c: &mut Canvas, d: &Dressed, eye: Vec3, x0: i32, y0: i32, w: i32, h: i32, scale: f32) {
    let cam = Cam::from_eye(eye);
    let mut pose_v = vec![Affine3A::IDENTITY; d.model.parts.len()];
    pose(&d.model, AnimState::Idle, 0.0, &mut pose_v);
    // Fixed scale (all tiles share it) with the feet on a common ground line; big models (giant
    // summons) are fitted to the tile instead.
    let height = d.model.bounds.max.y - d.model.bounds.min.y;
    let scale = scale.min((h as f32 - 24.0) / height.max(0.1));
    let ox = x0 as f32 + w as f32 * 0.5;
    let oy = (y0 + h) as f32 - 10.0;
    c.clear_depth();
    let light = Vec3::new(-0.45, 0.8, -0.55).normalize();
    for (pi, m) in d.meshes.iter().enumerate() {
        let tr = pose_v[pi];
        let nm = tr.matrix3;
        let sv: Vec<(f32, f32, f32)> = m
            .positions
            .iter()
            .map(|p| {
                let wp = tr.transform_point3(Vec3::from_array(*p));
                let (x, y, z) = cam.proj(wp);
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
            raster(c, [sv[a], sv[b], sv[e]], [m.uvs[a], m.uvs[b], m.uvs[e]], [col[0], col[1], col[2]], lit, (x0, y0, x0 + w, y0 + h));
        }
    }
}

fn raster(c: &mut Canvas, v: [(f32, f32, f32); 3], uv: [[f32; 2]; 3], base: [f32; 3], lit: f32, clip: (i32, i32, i32, i32)) {
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
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '\'' => [4, 4, 8, 0, 0, 0, 0],
        _ => [0; 7],
    }
}

fn text(c: &mut Canvas, x: i32, y: i32, s: &str, col: [u8; 3], k: i32) {
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

fn text_w(s: &str, k: i32) -> i32 {
    s.chars().count() as i32 * 6 * k - k
}

const FRONT: Vec3 = Vec3::new(0.0, 0.12, -1.0);
const THREE_Q: Vec3 = Vec3::new(0.75, 0.22, -0.8);

/// Grid of dressed costumes: two views each, name (+ sub line) under the pair.
fn sheet(items: &[Dressed], cols: usize, title: &str, zoom: f32) -> Canvas {
    let (vw, vh) = ((150.0 * zoom) as i32, (230.0 * zoom) as i32);
    let tw = vw * 2 + 8;
    let th = vh + 40;
    let head = 30;
    let rows = items.len().div_ceil(cols);
    let mut c = Canvas::new(cols * tw as usize, rows * th as usize + head);
    text(&mut c, 8, 9, title, [215, 225, 235], 2);
    // Pixels per metre, shared by every tile so sizes compare (a 1.8 m figure ~160 px at zoom 1).
    let scale = 90.0f32 * zoom;
    for (i, d) in items.iter().enumerate() {
        let x0 = (i % cols) as i32 * tw;
        let y0 = (i / cols) as i32 * th + head as i32;
        c.rect(x0 + 2, y0 + 2, tw - 4, th - 4, [26, 29, 38]);
        draw_view(&mut c, d, FRONT, x0 + 4, y0 + 4, vw, vh, scale);
        draw_view(&mut c, d, THREE_Q, x0 + 4 + vw, y0 + 4, vw, vh, scale);
        let lw = text_w(&d.label, 2);
        text(&mut c, x0 + (tw - lw) / 2, y0 + vh + 8, &d.label, [240, 240, 244], 2);
        if !d.sub.is_empty() {
            let sw = text_w(&d.sub, 1);
            text(&mut c, x0 + (tw - sw) / 2, y0 + vh + 26, &d.sub, [130, 140, 155], 1);
        }
    }
    c
}

fn lexicon(root: &std::path::Path) -> Option<bd_lexicon::Lexicon> {
    bd_lexicon::Lexicon::load(root.join("assets/lexicon.bin")).ok()
}

fn resolve(lex: Option<&bd_lexicon::Lexicon>, text: &str) -> (Recipe, Option<ShapeSpec>) {
    match lex {
        Some(l) => {
            let r = l.resolve(text);
            (r.recipe, r.shape)
        }
        None => {
            let mut r = Recipe::unknown(text);
            r.form = Form::Biped;
            (r, None)
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let val = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let root = std::env::var("BD_ROOT").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let docs = root.join("docs");
    let t = look::table();
    if let Some(e) = &t.error {
        panic!("characters.ron: {e}");
    }
    let names: Option<Vec<String>> = val("--names").map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    let pick: Vec<usize> = match &names {
        Some(ns) => ns.iter().filter_map(|n| look::find(n).map(|f| f.index)).collect(),
        None => (0..t.chars.len()).collect(),
    };
    let tweaks = CostumeTweaks::default();
    let cols: usize = val("--cols").and_then(|s| s.parse().ok()).unwrap_or(6);
    let zoom: f32 = val("--zoom").and_then(|s| s.parse().ok()).unwrap_or(1.0);

    if has("--mix") {
        let phrases = [
            "gojo but bald", "donatello with a straw hat", "goku with naruto's headband", "red goku", "goku x luffy", "gojo but flibbertigibbet",
            "gojo with pink hair", "darth vader with a straw hat", "batman with a gold cape", "naruto x sasuke", "master chief but no helmet", "straw hat",
        ];
        let items: Vec<Dressed> = phrases
            .iter()
            .map(|ph| {
                let c = costume::dress(ph, &Recipe::unknown(ph), None, None, 0, tweaks);
                prepare(ph.to_uppercase(), c.character.clone().unwrap_or_default().to_uppercase(), &c)
            })
            .collect();
        let c = sheet(&items, cols.min(6), "MIX AND MATCH (BASE CHARACTER + WITH / BUT / COLOUR / MASHUP)", zoom);
        c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join("costume_mix.png")));
        return;
    }
    if has("--summons") {
        let items = vec![
            prepare_summon("GOKU".into(), "goku", 1.9),
            prepare_summon("NARUTO".into(), "naruto", 1.8),
            prepare_summon("GOJO".into(), "gojo", 1.9),
            prepare_summon("LUFFY".into(), "luffy", 1.8),
            prepare_summon("GIANT GOKU".into(), "giant goku", 16.0),
            prepare_summon("GIANT GOJO".into(), "giant gojo", 16.0),
        ];
        let c = sheet(&items, 3, "SUMMONED COSTUME CONSTRUCTS (HARD LIGHT: PALETTE BLENDED TOWARD THE LANTERN COLOUR)", zoom);
        c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join("costume_summons.png")));
        return;
    }
    if has("--before-after") {
        let lex = lexicon(&root);
        let list = ["gojo", "darth vader", "goku", "naruto", "spider-man", "master chief"];
        let mut items = Vec::new();
        for n in list {
            let (r, s) = resolve(lex.as_ref(), n);
            let before = Costume::plain(costume_with(&r, s.as_ref(), None, 0, tweaks));
            items.push(prepare(format!("{} (BEFORE)", n.to_uppercase()), format!("OLD: RESOLVED AS {}", r.name.to_uppercase()), &before));
            let after = costume::dress(n, &r, s.as_ref(), None, 0, tweaks);
            items.push(prepare(n.to_uppercase(), "NEW CHARACTER LOOK".into(), &after));
        }
        let c = sheet(&items, 4, "COSTUMES BEFORE / AFTER (OLD VOCABULARY PATH VS CHARACTER LOOK)", zoom);
        c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join("costume_before_after.png")));
        return;
    }
    if let Some(n) = val("--derived").and_then(|s| s.parse::<usize>().ok()) {
        let lex = lexicon(&root).expect("lexicon needed for --derived");
        let mut items = Vec::new();
        for (name, rec, spec) in humanoid_icons(&root, &lex).into_iter().take(n) {
            let c = costume::derived_costume(&rec, &spec, tweaks);
            items.push(prepare(name.to_uppercase(), "DERIVED FROM ICON".into(), &c));
        }
        let c = sheet(&items, cols, "DERIVED COSTUMES (HUMANOID ICONS WITHOUT A HAND-AUTHORED LOOK)", zoom);
        c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join("costume_derived.png")));
        return;
    }
    let mut items = Vec::new();
    for &i in &pick {
        let ch = &t.chars[i];
        let nv = if has("--variants") { ch.variant_count() } else { 1 };
        for v in 0..nv {
            let c = costume::character_costume(i, v, tweaks);
            let vol = {
                let b = spec_bounds(&c.spec);
                let s = (b.max - b.min + Vec3::ONE) * COSTUME_UNIT;
                s.x * s.y * s.z / (0.8 * 1.8 * 0.8)
            };
            let vname = ch.variants.get(v).map(|x| x.name.clone()).unwrap_or_default();
            let label = ch.names[0].to_uppercase();
            let pm = costume::costume_model_painted(&c);
            let sub = if has("--variants") || ch.variant_count() > 1 {
                format!("{} {}/{}  {}V {:.2}X", vname.to_uppercase(), v + 1, ch.variant_count(), pm.model.voxel_count, vol)
            } else {
                format!("{}V {:.2}X", pm.model.voxel_count, vol)
            };
            items.push(prepare(label, sub, &c));
        }
    }
    let default_out = if has("--variants") { "costume_variants.png" } else { "costume_gallery.png" };
    let title = format!("CHARACTER COSTUMES ({} LOOKS) - FRONT + 3/4 - 1/16 M VOXELS", items.len());
    let c = sheet(&items, cols, &title, zoom);
    c.save(&val("--out").map(PathBuf::from).unwrap_or(docs.join(default_out)));
}

/// Humanoid icons (Biped with head + arms) that have no hand-authored character look.
fn humanoid_icons(root: &std::path::Path, lex: &bd_lexicon::Lexicon) -> Vec<(String, Recipe, ShapeSpec)> {
    let mut files = vec![root.join("assets/icons.ron")];
    if let Ok(rd) = std::fs::read_dir(root.join("assets/icons")) {
        let mut v: Vec<PathBuf> =
            rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "ron")).collect();
        v.sort();
        files.extend(v);
    }
    let mut out = Vec::new();
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        for line in src.lines() {
            let l = line.trim();
            let Some(rest) = l.strip_prefix("names: [\"") else { continue };
            let Some(name) = rest.split('"').next() else { continue };
            if costume::grammar::parse(name).is_some() {
                continue;
            }
            let r = lex.resolve(name);
            if let Some(s) = r.shape
                && costume::is_character_icon(&r.recipe, &s)
                && !out.iter().any(|(n, _, _): &(String, Recipe, ShapeSpec)| n == name)
            {
                out.push((name.to_string(), r.recipe, s));
            }
        }
    }
    out
}
