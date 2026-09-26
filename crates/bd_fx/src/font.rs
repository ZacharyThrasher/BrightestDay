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
    ('C', [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."]),
    ('D', ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."]),
    ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####"]),
    ('F', ["#####", "#....", "#....", "####.", "#....", "#....", "#...."]),
    ('G', [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".###."]),
    ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
    ('I', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####"]),
    ('J', ["..###", "...#.", "...#.", "...#.", "#..#.", "#..#.", ".##.."]),
    ('K', ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"]),
    ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####"]),
    ('M', ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"]),
    ('N', ["#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#"]),
    ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ('P', ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."]),
    ('Q', [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"]),
    ('R', ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"]),
    ('S', [".####", "#....", "#....", ".###.", "....#", "....#", "####."]),
    ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."]),
    ('U', ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ('V', ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."]),
    ('W', ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#."]),
    ('X', ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"]),
    ('Y', ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."]),
    ('Z', ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"]),
    ('0', [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."]),
    ('1', ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."]),
    ('2', [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"]),
    ('3', ["####.", "....#", "....#", ".###.", "....#", "....#", "####."]),
    ('4', ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."]),
    ('5', ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."]),
    ('6', [".###.", "#....", "#....", "####.", "#...#", "#...#", ".###."]),
    ('7', ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."]),
    ('8', [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."]),
    ('9', [".###.", "#...#", "#...#", ".####", "....#", "....#", ".###."]),
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
    // Bold: every pixel also fills its right neighbour.
    let mut g = GlyphBits::new(w + 1, 7 + extra_h);
    for (y, r) in rows.iter().enumerate() {
        for (x, ch) in r.chars().enumerate() {
            if ch == '#' {
                g.set(x as i32, (y + extra_h) as i32, false);
                g.set(x as i32 + 1, (y + extra_h) as i32, false);
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

/// Advance (in voxels) of a space.
pub const SPACE_ADVANCE: f32 = 4.0;

/// Glyph → voxel shape spec (construct-voxel units): faces +Z, baseline at y = 0, left edge x = 0.
/// Italic: rows shear right by ~1 voxel per 3 rows of height.
pub fn glyph_spec(g: &GlyphBits) -> ShapeSpec {
    let mut prims = Vec::new();
    let h = g.h as i32;
    let shear = |y: i32| -> i32 { (h - 1 - y) / 3 };
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
            let px = (x + shear(y)) as f32;
            let py = (h - 1 - y) as f32;
            prims.push(unit(px, py, 1.0, tint));
            prims.push(unit(px, py, 0.0, Tint::Primary));
        }
    }
    // Comic extrusion: the silhouette plus a copy shifted one voxel right/down, one step behind,
    // Dark. Keeps counters (A, O, R…) open while giving every letter a heavy drop edge.
    for y in 0..=h {
        for x in 0..=g.w as i32 {
            if !(g.get(x, y) || g.get(x - 1, y - 1)) {
                continue;
            }
            let yc = y.clamp(0, h - 1);
            let px = (x + shear(yc)) as f32;
            let py = (h - 1 - y) as f32;
            prims.push(unit(px, py, -1.0, Tint::Dark));
        }
    }    ShapeSpec { prims, parts: vec![] }
}
