//! Blocky comic voxel font generated in code: A–Z, 0–9, `! ? - ' .` and a katakana subset for
//! manga-style SFX (with dakuten ゛ / handakuten ゜ and the small ッ). Glyphs are bold (strokes
//! dilated to 2 voxels), slightly italic, extruded 2 voxels, with a recessed Dark outline and a light
//! Accent top band — built as `VoxelModel`s so they mesh with the hard-light construct shader and
//! shatter through the normal shatter path.

use bd_core::shape::{Prim, PrimKind, ShapeSpec, Tint};

/// 5×7 Latin bitmaps ('#' = filled), top row first.
const LATIN: &[(char, [&str; 7])] = &[
    ('A', [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
    ('B', ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."]),
    ('C', [".####", "#....", "#....", "#....", "#....", "#....", ".####"]),
    ('D', ["###..", "#..#.", "#...#", "#...#", "#...#", "#..#.", "###.."]),
    ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####"]),
    ('F', ["#####", "#....", "#....", "####.", "#....", "#....", "#...."]),
    ('G', [".####", "#....", "#....", "#..##", "#...#", "#...#", ".####"]),
    ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
    ('I', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####"]),
    ('J', ["..###", "....#", "....#", "....#", "#...#", "#...#", ".###."]),
    ('K', ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"]),
    ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####"]),
    ('M', ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"]),
    ('N', ["#...#", "##..#", "#.#.#", "#.#.#", "#..##", "#...#", "#...#"]),
    ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ('P', ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."]),
    ('Q', [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"]),
    ('R', ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"]),
    ('S', [".####", "#....", "#....", ".###.", "....#", "....#", "####."]),
    ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."]),
    ('U', ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ('V', ["#...#", "#...#", "#...#", "#...#", ".#.#.", ".#.#.", "..#.."]),
    ('W', ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"]),
    ('X', ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"]),
    ('Y', ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."]),
    ('Z', ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"]),
    ('0', [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."]),
    ('1', ["..#..", ".##..", "#.#..", "..#..", "..#..", "..#..", "#####"]),
    ('2', [".###.", "#...#", "....#", "..##.", ".#...", "#....", "#####"]),
    ('3', ["####.", "....#", "....#", ".###.", "....#", "....#", "####."]),
    ('4', ["#..#.", "#..#.", "#..#.", "#####", "...#.", "...#.", "...#."]),
    ('5', ["#####", "#....", "#....", "####.", "....#", "#...#", ".###."]),
    ('6', ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."]),
    ('7', ["#####", "....#", "...#.", "..#..", "..#..", "..#..", "..#.."]),
    ('8', [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."]),
    ('9', [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."]),
    ('!', ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#.."]),
    ('?', [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#.."]),
    ('-', [".....", ".....", ".....", ".###.", ".....", ".....", "....."]),
    ('\'', ["..#..", "..#..", ".....", ".....", ".....", ".....", "....."]),
    ('.', [".....", ".....", ".....", ".....", ".....", ".....", "..#.."]),
];

/// 7×7 katakana base shapes.
const KANA: &[(char, [&str; 7])] = &[
    ('ト', ["..#....", "..#....", "..##...", "..#.##.", "..#....", "..#....", "..#...."]),
    ('コ', [".#####.", ".....#.", ".....#.", ".....#.", ".....#.", ".....#.", ".#####."]),
    ('ハ', ["..#.#..", "..#..#.", ".#...#.", ".#....#", "#.....#", "#.....#", "......."]),
    ('ス', [".#####.", ".....#.", "....#..", "...#...", "..#.#..", ".#...#.", "#.....#"]),
    ('カ', ["..#....", "#######", "..#...#", "..#...#", ".#....#", ".#....#", "#...##."]),
    ('キ', ["...#...", ".#####.", "...#...", "#######", "...#...", "...#...", "...#..."]),
    ('ン', ["#......", ".#....#", "......#", ".....#.", "....#..", "..##...", "##....."]),
    ('ー', [".......", ".......", ".......", "#######", ".......", ".......", "......."]),
    ('シ', ["##.....", "..#...#", "##....#", "..#..#.", "....#..", "..##...", "##....."]),
    ('サ', [".#...#.", "#######", ".#...#.", ".#...#.", ".....#.", "....#..", "..##..."]),
    ('ホ', ["...#...", "#######", "...#...", ".#.#.#.", "#..#..#", "...#...", "..##..."]),
    ('フ', ["#######", "......#", "......#", ".....#.", "....#..", "..##...", "##....."]),
    ('ヒ', ["#......", "#...##.", "###....", "#......", "#......", "#......", ".######"]),
    ('ク', ["..#....", ".######", ".#....#", "#....#.", "....#..", "...#...", ".##...."]),
    ('ッ', [".......", ".......", "#.#.#..", "#.#.#..", "....#..", "...#...", ".##...."]),
];

/// Composite kana: (char, base, mark) with mark 1 = dakuten ゛, 2 = handakuten ゜.
const KANA_MARKED: &[(char, char, u8)] = &[
    ('ド', 'ト', 1),
    ('ゴ', 'コ', 1),
    ('バ', 'ハ', 1),
    ('パ', 'ハ', 2),
    ('ズ', 'ス', 1),
    ('ガ', 'カ', 1),
    ('ジ', 'シ', 1),
    ('ザ', 'サ', 1),
    ('ボ', 'ホ', 1),
    ('ポ', 'ホ', 2),
    ('ブ', 'フ', 1),
    ('ビ', 'ヒ', 1),
    ('ピ', 'ヒ', 2),
    ('グ', 'ク', 1),
];

/// Every supported character.
pub fn all_chars() -> Vec<char> {
    let mut v: Vec<char> = LATIN.iter().map(|(c, _)| *c).collect();
    v.extend(KANA.iter().map(|(c, _)| *c));
    v.extend(KANA_MARKED.iter().map(|(c, _, _)| *c));
    v
}

/// Glyph pixel grid: `rows[y][x]`, y = 0 top.
pub struct GlyphBits {
    pub w: usize,
    pub h: usize,
    pub px: Vec<bool>,
    /// Pixels belonging to a diacritic mark (drawn in the Core tint).
    pub mark: Vec<bool>,
}

impl GlyphBits {
    fn new(w: usize, h: usize) -> Self {
        GlyphBits { w, h, px: vec![false; w * h], mark: vec![false; w * h] }
    }
    fn set(&mut self, x: i32, y: i32, mark: bool) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let i = y as usize * self.w + x as usize;
            self.px[i] = true;
            self.mark[i] |= mark;
        }
    }
    pub fn get(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h && self.px[y as usize * self.w + x as usize]
    }
}

fn bold(rows: &[&str; 7], w: usize, extra_h: usize) -> GlyphBits {
    // Consistent 1-voxel strokes (the comic weight comes from the extrusion, not dilation).
    let mut g = GlyphBits::new(w, 7 + extra_h);
    for (y, r) in rows.iter().enumerate() {
        for (x, ch) in r.chars().enumerate() {
            if ch == '#' {
                g.set(x as i32, (y + extra_h) as i32, false);
            }
        }
    }
    g
}

/// Bitmap for a character (None = unsupported; ' ' = None but has an advance).
pub fn glyph_bits(c: char) -> Option<GlyphBits> {
    let c = c.to_ascii_uppercase();
    if let Some((_, rows)) = LATIN.iter().find(|(k, _)| *k == c) {
        return Some(bold(rows, 5, 0));
    }
    if let Some((_, rows)) = KANA.iter().find(|(k, _)| *k == c) {
        let mut g = bold(rows, 7, 1);
        // Kana get two extra columns so marked/unmarked glyphs share metrics.
        let mut w = GlyphBits::new(g.w + 3, g.h);
        for y in 0..g.h {
            for x in 0..g.w {
                if g.get(x as i32, y as i32) {
                    w.set(x as i32, y as i32, false);
                }
            }
        }
        g = w;
        return Some(g);
    }
    if let Some((_, base, mark)) = KANA_MARKED.iter().find(|(k, _, _)| *k == c) {
        let mut g = glyph_bits(*base)?;
        let right = g.w as i32 - 1;
        if *mark == 1 {
            // ゛ two slanted ticks, top-right.
            for (x, y) in [(right - 2, 0), (right - 2, 1), (right, 0), (right, 1)] {
                g.set(x, y, true);
            }
        } else {
            // ゜ small ring.
            for (x, y) in [(right - 2, 0), (right - 1, 0), (right, 0), (right - 2, 1), (right, 1), (right - 2, 2), (right - 1, 2), (right, 2)] {
                g.set(x, y, true);
            }
        }
        return Some(g);
    }
    None
}

/// Italic shear applied to glyph meshes: x += ITALIC · y (metres, about the glyph centre).
pub const ITALIC: f32 = 0.2;

/// Advance (in voxels) of a space.
pub const SPACE_ADVANCE: f32 = 4.0;

/// Glyph → voxel shape spec (construct-voxel units): faces +Z, baseline at y = 0, left edge x = 0.
/// Italic: rows shear right by ~1 voxel per 3 rows of height.
pub fn glyph_spec(g: &GlyphBits) -> ShapeSpec {
    let mut prims = Vec::new();
    let h = g.h as i32;
    // The italic slant is a smooth shear applied to the glyph mesh (see [`ITALIC`]), so strokes stay
    // straight; no voxel steps here.
    let shear = |_y: i32| -> i32 { 0 };
    let unit = |x: f32, y: f32, z: f32, tint: Tint| Prim {
        kind: PrimKind::Box,
        center: glam::Vec3::new(x, y, z),
        size: glam::Vec3::ONE,
        rot_deg: glam::Vec3::ZERO,
        tint,
        part: 0,
        subtract: false,
    };
    // Front: 2 voxels deep (z = 0, 1). Top band lighter (comic shine), marks hot.
    for y in 0..h {
        for x in 0..g.w as i32 {
            if !g.get(x, y) {
                continue;
            }
            let i = y as usize * g.w + x as usize;
            let tint = if g.mark[i] {
                Tint::Core
            } else if y <= 1 || (y as usize) < g.h / 4 {
                Tint::Accent
            } else {
                Tint::Primary
            };
            let py = (h - 1 - y) as f32;
            let mut xs = [x + shear(y), x + shear(y + 1)];
            if xs[0] == xs[1] || y + 1 >= h || !g.get(x, y + 1) {
                xs[1] = xs[0];
            }
            for (k, px) in xs.iter().enumerate() {
                if k == 1 && xs[1] == xs[0] {
                    break;
                }
                prims.push(unit(*px as f32, py, 1.0, tint));
                prims.push(unit(*px as f32, py, 0.0, Tint::Primary));
            }
        }
    }
    // Comic extrusion, one step behind in Dark: the silhouette plus a copy shifted one voxel
    // right/down — but only on *exterior* cells (flood-filled from the border), so counters of
    // A/O/D/0 stay clean and O never reads as a slashed 0.
    let (pw, ph) = (g.w as i32 + 3, h + 3);
    let idx = |x: i32, y: i32| ((y + 1) * pw + (x + 1)) as usize;
    let mut outside = vec![false; (pw * ph) as usize];
    let mut stack = vec![(-1i32, -1i32)];
    outside[idx(-1, -1)] = true;
    while let Some((x, y)) = stack.pop() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < -1 || ny < -1 || nx > g.w as i32 + 1 || ny > h + 1 {
                continue;
            }
            if !outside[idx(nx, ny)] && !g.get(nx, ny) {
                outside[idx(nx, ny)] = true;
                stack.push((nx, ny));
            }
        }
    }
    for y in 0..=h {
        for x in 0..=g.w as i32 {
            let body = g.get(x, y);
            let drop = g.get(x - 1, y - 1) && outside[idx(x, y)];
            if !(body || drop) {
                continue;
            }
            let yc = y.clamp(0, h - 1);
            let px = (x + shear(yc)) as f32;
            let py = (h - 1 - y) as f32;
            prims.push(unit(px, py, -1.0, Tint::Dark));
        }
    }    ShapeSpec { prims, parts: vec![] }
}
