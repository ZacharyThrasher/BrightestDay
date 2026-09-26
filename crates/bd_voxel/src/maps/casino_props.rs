//! High Roller prop kit: slot machines (decor + the big indestructible "high roller" machines),
//! card / craps tables, giant poker-chip stacks and dice, crystal chandeliers, marble columns, pixel
//! neon text and card-suit signs, loungers, the neon skyline. Generic art — no real brands.
//!
//! Everything is built in metres on the [`City`] builder with axis-aligned local frames
//! ([`Frame`]: `u` = the viewer's right when facing the front, `v` = depth into the prop, `y` up).

use bd_core::voxel::Material;
use glam::{Vec2, Vec3};

use crate::city::City;
use crate::interact::SlotMachine;
use crate::material::mat;
use crate::noise::{TAU, datan2, hash3, unit};

const VS: f32 = 0.5;

/// An axis-aligned local frame: origin `o` (front centre at the base), `f` = unit axis out of the
/// front (±X or ±Z), the viewer (facing the front) looks along −f.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub o: Vec3,
    pub f: Vec2,
}

impl Frame {
    pub fn new(o: Vec3, f: Vec2) -> Self {
        Self { o, f }
    }
    /// The viewer's right (unit, horizontal).
    pub fn right(&self) -> Vec2 {
        Vec2::new(self.f.y, -self.f.x)
    }
    /// World point of local (u right, y up, v depth into the prop).
    pub fn at(&self, u: f32, y: f32, v: f32) -> Vec3 {
        let r = self.right();
        Vec3::new(self.o.x + r.x * u - self.f.x * v, self.o.y + y, self.o.z + r.y * u - self.f.y * v)
    }
    /// Fill the local box `[u0,u1) × [y0,y1) × [v0,v1)`.
    #[allow(clippy::too_many_arguments)]
    pub fn fill(&self, c: &mut City, u0: f32, y0: f32, v0: f32, u1: f32, y1: f32, v1: f32, m: Material) {
        let (a, b) = (self.at(u0, y0, v0), self.at(u1, y1, v1));
        c.fill(a.min(b), a.max(b), m);
    }
    pub fn f3(&self) -> Vec3 {
        Vec3::new(self.f.x, 0.0, self.f.y)
    }
}

/// Fill a vertical cylinder (voxel centres within `r` of the axis) from `y0` to `y1`, choosing the
/// material per voxel: `pick(dist_from_axis, angle, voxel_y_index)`.
pub fn cylinder(c: &mut City, cx: f32, cz: f32, r: f32, y0: f32, y1: f32, mut pick: impl FnMut(f32, f32, i32) -> Option<Material>) {
    c.fill_with(Vec3::new(cx - r, y0, cz - r), Vec3::new(cx + r, y1, cz + r), |p, v, _| {
        let (dx, dz) = (p.x - cx, p.z - cz);
        let d = (dx * dx + dz * dz).sqrt();
        if d > r {
            return None;
        }
        pick(d, datan2(dz, dx), v.y)
    });
}

// ------------------------------------------------------------------------------------ chips + dice

/// A stack of `n` giant poker chips (0.5 m each, slightly misaligned), returns the top height.
pub fn chip_stack(c: &mut City, base: Vec3, r: f32, n: i32, palette: &[Material], seed: u32) -> f32 {
    let mut y = base.y;
    for i in 0..n {
        let h = hash3(i, n, 7, seed);
        let off = Vec2::new((unit(h) - 0.5) * 0.5, (unit(h >> 8) - 0.5) * 0.5);
        let col = palette[((seed as usize) + i as usize * 3 + (h >> 20) as usize) % palette.len()];
        let top = i == n - 1;
        let (cx, cz) = (base.x + off.x, base.z + off.y);
        cylinder(c, cx, cz, r, y, y + VS, |d, a, _| {
            let seg = ((a + TAU) / TAU * 16.0) as i32;
            if d > r - 0.55 {
                // Edge spots: white every other segment.
                Some(if seg % 2 == 0 { mat::CHIP_WHITE } else { col })
            } else if top && d > r * 0.5 && d < r * 0.5 + 0.5 {
                Some(mat::CHIP_WHITE)
            } else {
                Some(col)
            }
        });
        y += VS;
    }
    y
}

fn pip_on(value: u8, gx: i32, gy: i32) -> bool {
    let corner_a = (gx == 0 && gy == 0) || (gx == 2 && gy == 2);
    let corner_b = (gx == 2 && gy == 0) || (gx == 0 && gy == 2);
    let center = gx == 1 && gy == 1;
    match value {
        1 => center,
        2 => corner_a,
        3 => corner_a || center,
        4 => corner_a || corner_b,
        5 => corner_a || corner_b || center,
        _ => gx != 1,
    }
}

/// A giant die: a cube of `size` voxels (5 or 7) with its min corner at `min`, pips on every face
/// (opposite faces sum to 7; `top` on +Y).
pub fn die(c: &mut City, min: Vec3, size: i32, top: u8, body: Material, pip: Material) {
    let top = top.clamp(1, 6);
    // A valid side pair for this top.
    let sides: Vec<u8> = (1..=6).filter(|v| *v != top && *v != 7 - top).collect();
    let a = sides[0];
    let b = sides.iter().copied().find(|v| *v != a && *v != 7 - a).unwrap_or(3);
    let s = size.max(3);
    let step = (s - 3) / 2;
    let grid = |i: i32| -> Option<i32> {
        let k = i - 1;
        if k >= 0 && k % step.max(1) == 0 && k / step.max(1) <= 2 { Some(k / step.max(1)) } else { None }
    };
    let (x0, y0, z0) = ((min.x / VS).floor() as i32, (min.y / VS).floor() as i32, (min.z / VS).floor() as i32);
    for dy in 0..s {
        for dz in 0..s {
            for dx in 0..s {
                let edge = |i: i32| i == 0 || i == s - 1;
                let on_face = edge(dx) || edge(dy) || edge(dz);
                let mut m = body;
                if on_face {
                    // Which face (skip the rounded-off edges: two coordinates on the boundary).
                    let n_edges = edge(dx) as i32 + edge(dy) as i32 + edge(dz) as i32;
                    if n_edges == 1 {
                        let (val, g1, g2) = if dy == s - 1 {
                            (top, dx, dz)
                        } else if dy == 0 {
                            (7 - top, dx, dz)
                        } else if dx == s - 1 {
                            (a, dz, dy)
                        } else if dx == 0 {
                            (7 - a, dz, dy)
                        } else if dz == s - 1 {
                            (b, dx, dy)
                        } else {
                            (7 - b, dx, dy)
                        };
                        if let (Some(gx), Some(gy)) = (grid(g1), grid(g2))
                            && pip_on(val, gx, gy)
                        {
                            m = pip;
                        }
                    } else if n_edges == 3 {
                        continue; // corners knocked off: reads as a rounded die
                    }
                }
                c.w.set(glam::IVec3::new(x0 + dx, y0 + dy, z0 + dz), m);
            }
        }
    }
}

// ------------------------------------------------------------------------------------ slots

/// A decor slot machine (destructible), 1.5 × 1 × 3 m, front at the frame origin.
pub fn slot_decor(c: &mut City, fr: Frame, style: u32) {
    let body = [mat::PAINT_RED, mat::CHIP_PURPLE, mat::PLASTIC_BLUE, mat::STEEL, mat::ROULETTE_BLACK][(style % 5) as usize];
    let neon = [mat::NEON_PINK, mat::NEON_CYAN, mat::NEON_AMBER, mat::NEON_VIOLET, mat::NEON_GREEN][((style / 5) % 5) as usize];
    fr.fill(c, -0.75, 0.0, 0.0, 0.75, 0.5, 1.0, mat::ROULETTE_BLACK);
    fr.fill(c, -0.75, 0.5, 0.0, 0.75, 2.5, 1.0, body);
    // Button deck sticking out, screen above it, glowing topper.
    fr.fill(c, -0.75, 0.5, -0.5, 0.75, 1.0, 0.0, mat::GOLD_TRIM);
    fr.fill(c, -0.5, 1.0, 0.0, 0.5, 2.0, 0.5, mat::SCREEN_GLOW);
    fr.fill(c, -0.5, 2.5, 0.0, 0.5, 3.0, 1.0, neon);
}

/// A big "high roller" machine: indestructible 4.5 × 2.5 × 6 m cabinet with a recessed reel window
/// and a lever mount on the viewer's right. Returns the interactable (lever trigger, reel window).
pub fn slot_big(c: &mut City, fr: Frame) -> SlotMachine {
    let (w, d, h) = (2.25f32, 2.5f32, 6.0f32);
    fr.fill(c, -w, 0.0, 0.0, w, 0.5, d, mat::SLOT_TRIM);
    fr.fill(c, -w, 0.5, 0.0, w, 5.0, d, mat::SLOT_CABINET);
    fr.fill(c, -w, 5.0, 0.0, w, h, d, mat::SLOT_TRIM);
    // Marquee crown + a light bar across the front.
    fr.fill(c, -w + 0.5, h, 0.5, w - 0.5, h + 0.5, d - 0.5, mat::SLOT_CABINET);
    fr.fill(c, -w, 4.5, -0.5, w, 5.0, 0.0, mat::SLOT_TRIM);
    // Reel window: recessed 0.5 m, gold frame, dark back.
    fr.fill(c, -1.75, 2.0, 0.0, 1.75, 3.5, 0.5, bd_core::voxel::AIR);
    fr.fill(c, -1.75, 2.0, 0.5, 1.75, 3.5, 1.0, mat::ROULETTE_BLACK);
    fr.fill(c, -2.25, 1.5, 0.0, -1.75, 4.0, 0.5, mat::SLOT_TRIM);
    fr.fill(c, 1.75, 1.5, 0.0, 2.25, 4.0, 0.5, mat::SLOT_TRIM);
    fr.fill(c, -1.75, 1.5, 0.0, 1.75, 2.0, 0.5, mat::SLOT_TRIM);
    fr.fill(c, -1.75, 3.5, 0.0, 1.75, 4.0, 0.5, mat::SLOT_TRIM);
    // Button deck.
    fr.fill(c, -w, 1.0, -0.5, w, 1.5, 0.0, mat::SLOT_TRIM);
    // Lever mount: a gold plate on the right side (the client draws the arm + knob swinging on it).
    fr.fill(c, w, 2.0, 0.75, w + 0.5, 3.5, 1.75, mat::SLOT_TRIM);
    let lever = fr.at(w + 1.0, 3.4, 1.25);
    SlotMachine { lever, radius: 1.3, reels: fr.at(0.0, 2.75, 0.5), facing: fr.f3(), window: [1.75, 0.75] }
}

// ------------------------------------------------------------------------------------ tables

/// Blackjack half-moon table (dealer on the straight side, players around the arc facing −f).
pub fn card_table(c: &mut City, fr: Frame) {
    let o = fr.o;
    let r = 2.4;
    let f = fr.f;
    c.fill_with(Vec3::new(o.x - r, o.y, o.z - r), Vec3::new(o.x + r, o.y + 1.0, o.z + r), |p, _, _| {
        let d = Vec2::new(p.x - o.x, p.z - o.z);
        let dist = d.length();
        // The half disc on the players' side (−f is the dealer's back).
        if dist > r || d.dot(f) < -0.1 {
            return None;
        }
        let top = p.y > o.y + 0.5;
        Some(if !top {
            if dist < 0.9 { mat::DARK_WOOD } else { return None }
        } else if dist > r - 0.5 {
            mat::DARK_WOOD
        } else if dist > r - 1.0 && dist < r - 0.5 {
            mat::GOLD_TRIM
        } else {
            mat::FELT_GREEN
        })
    });
    // The dealer's chip tray.
    fr.fill(c, -1.0, 1.0, 0.0, 1.0, 1.5, 0.5, mat::CHIP_BLACK);
}

/// Craps table: a 7 × 3.5 m tub with raised rails (good low cover); the long axis is the frame's
/// right vector.
pub fn craps_table(c: &mut City, fr: Frame) {
    let (hw, d) = (3.5f32, 3.5f32);
    fr.fill(c, -hw + 0.5, 0.0, 0.5, hw - 0.5, 0.5, d - 0.5, mat::DARK_WOOD);
    fr.fill(c, -hw, 0.5, 0.0, hw, 1.5, d, mat::DARK_WOOD);
    fr.fill(c, -hw + 0.5, 0.5, 0.5, hw - 0.5, 1.5, d - 0.5, bd_core::voxel::AIR);
    fr.fill(c, -hw + 0.5, 0.5, 0.5, hw - 0.5, 1.0, d - 0.5, mat::FELT_GREEN);
    fr.fill(c, -hw, 1.5, 0.0, hw, 2.0, 0.5, mat::GOLD_TRIM);
    fr.fill(c, -hw, 1.5, d - 0.5, hw, 2.0, d, mat::GOLD_TRIM);
    // A pair of dice on the felt.
    fr.fill(c, 1.0, 1.0, 1.5, 1.5, 1.5, 2.0, mat::DICE_RED);
    fr.fill(c, 1.5, 1.0, 2.0, 2.0, 1.5, 2.5, mat::DICE_RED);
}

// ------------------------------------------------------------------------------------ chandeliers + columns

/// A crystal chandelier hanging from the ceiling point `top` by an indestructible chain to its hub
/// at `hub_y`; gold rings + crystal drops (breakable). The hub is a tether anchor.
pub fn chandelier(c: &mut City, top: Vec3, hub_y: f32, r: f32) {
    let x = top.x;
    let z = top.z;
    // Chain + hub (indestructible gold): the chandelier never falls, only its crystals break.
    c.fill(Vec3::new(x - 0.25, hub_y, z - 0.25), Vec3::new(x + 0.25, top.y, z + 0.25), mat::SLOT_TRIM);
    c.fill(Vec3::new(x - 0.5, hub_y - 0.5, z - 0.5), Vec3::new(x + 0.5, hub_y + 0.5, z + 0.5), mat::SLOT_TRIM);
    let tiers = [(r, hub_y - 0.25), (r * 0.72, hub_y - 1.25), (r * 0.42, hub_y - 2.25)];
    for (k, &(tr, ty)) in tiers.iter().enumerate() {
        // Gold ring.
        cylinder(c, x, z, tr, ty - 0.25, ty + 0.25, |d, _, _| (d > tr - 0.55).then_some(mat::GOLD_TRIM));
        // Spokes to the axis (keeps the ring attached to the hub column).
        for s in 0..4 {
            let a = s as f32 * TAU / 4.0 + k as f32 * 0.4;
            let e = Vec3::new(x + crate::noise::dcos(a) * tr, ty, z + crate::noise::dsin(a) * tr);
            c.line(Vec3::new(x, ty, z), e, mat::GOLD_TRIM, true);
        }
        // Axis between tiers.
        c.fill(Vec3::new(x - 0.25, ty - 0.25, z - 0.25), Vec3::new(x + 0.25, hub_y, z + 0.25), mat::GOLD_TRIM);
        // Crystal drops around the ring.
        let n = ((TAU * tr / 0.9) as i32).max(4);
        for s in 0..n {
            let a = s as f32 * TAU / n as f32;
            let p = Vec3::new(x + crate::noise::dcos(a) * (tr - 0.25), ty - 0.25, z + crate::noise::dsin(a) * (tr - 0.25));
            let len = if s % 2 == 0 { 1.25 } else { 0.75 };
            c.line(p, p - Vec3::Y * len, mat::CRYSTAL_DROP, true);
        }
    }
    // Finial.
    c.fill(Vec3::new(x - 0.25, hub_y - 4.0, z - 0.25), Vec3::new(x + 0.25, hub_y - 2.25, z + 0.25), mat::CRYSTAL_DROP);
}

/// A 1.5 m square marble column (indestructible core) with gold base + capital, `y0..y1`.
pub fn column(c: &mut City, x: f32, z: f32, y0: f32, y1: f32) {
    c.fill(Vec3::new(x - 0.75, y0, z - 0.75), Vec3::new(x + 0.75, y1, z + 0.75), mat::CASINO_PILLAR);
    c.fill(Vec3::new(x - 1.0, y0, z - 1.0), Vec3::new(x + 1.0, y0 + 0.5, z + 1.0), mat::GOLD_TRIM);
    c.fill(Vec3::new(x - 1.0, y1 - 1.0, z - 1.0), Vec3::new(x + 1.0, y1 - 0.5, z + 1.0), mat::GOLD_TRIM);
    // Fluting: vein stripes on the faces.
    c.fill_with(Vec3::new(x - 0.75, y0 + 0.5, z - 0.75), Vec3::new(x + 0.75, y1 - 1.0, z + 0.75), |p, v, old| {
        let edge = (p.x - x).abs() > 0.5 || (p.z - z).abs() > 0.5;
        (edge && old == mat::CASINO_PILLAR && (v.x + v.z).rem_euclid(2) == 0 && v.y.rem_euclid(6) < 4).then_some(mat::MARBLE_VEIN)
    });
}

// ------------------------------------------------------------------------------------ pixel text

/// 5 × 7 glyphs (rows top → bottom, bit 4 = leftmost column).
fn glyph(ch: char) -> Option<[u8; 7]> {
    Some(match ch {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '$' => [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04],
        '*' => [0x04, 0x15, 0x0E, 0x1F, 0x0E, 0x15, 0x04],
        _ => return None,
    })
}

/// Pixel text on a vertical plane: glyph pixels of `px` metres, the first glyph's bottom-left at
/// `origin`, advancing along `right` (a unit horizontal axis), `depth` voxels thick behind it
/// (along −normal where normal = the side the text is read from).
pub fn text(c: &mut City, s: &str, origin: Vec3, right: Vec2, normal: Vec2, px: f32, m: Material) -> f32 {
    let mut pen = 0.0;
    for ch in s.chars() {
        if let Some(g) = glyph(ch) {
            for (row, bits) in g.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0x10 >> col) != 0 {
                        let u = pen + col as f32 * px;
                        let y = (6 - row) as f32 * px;
                        let p = origin + Vec3::new(right.x * u, y, right.y * u);
                        let q = p + Vec3::new(right.x * px, px, right.y * px) - Vec3::new(normal.x, 0.0, normal.y) * VS;
                        c.fill(p.min(q), p.max(q), m);
                    }
                }
            }
        }
        pen += 6.0 * px;
    }
    pen - px
}

/// 7 × 7 card suits: 0 spade, 1 heart, 2 diamond, 3 club.
fn suit_bits(s: u32) -> [u8; 7] {
    match s % 4 {
        0 => [0x08, 0x1C, 0x3E, 0x7F, 0x7F, 0x08, 0x1C],
        1 => [0x36, 0x7F, 0x7F, 0x7F, 0x3E, 0x1C, 0x08],
        2 => [0x08, 0x1C, 0x3E, 0x7F, 0x3E, 0x1C, 0x08],
        _ => [0x1C, 0x1C, 0x6B, 0x7F, 0x6B, 0x08, 0x1C],
    }
}

/// A card-suit neon sign (7 × 7 pixels of `px` m), bottom-left at `origin`, plane like [`text`].
pub fn suit(c: &mut City, s: u32, origin: Vec3, right: Vec2, normal: Vec2, px: f32, m: Material) {
    for (row, bits) in suit_bits(s).iter().enumerate() {
        for col in 0..7 {
            if bits & (0x40 >> col) != 0 {
                let u = col as f32 * px;
                let y = (6 - row) as f32 * px;
                let p = origin + Vec3::new(right.x * u, y, right.y * u);
                let q = p + Vec3::new(right.x * px, px, right.y * px) - Vec3::new(normal.x, 0.0, normal.y) * VS;
                c.fill(p.min(q), p.max(q), m);
            }
        }
    }
}

/// 3 × 5 digits for the roulette pockets (rows top → bottom, bit 2 = leftmost).
pub fn digit_bits(d: char) -> [u8; 5] {
    match d {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 3, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 1, 2, 2],
        '8' => [7, 5, 7, 5, 7],
        _ => [7, 5, 7, 1, 7],
    }
}

// ------------------------------------------------------------------------------------ roof + skyline

/// A pool lounger (white frame, velvet cushion), long axis along the frame's depth.
pub fn lounger(c: &mut City, fr: Frame) {
    fr.fill(c, -0.5, 0.0, 0.0, 0.5, 0.5, 2.0, mat::PAINT_WHITE);
    fr.fill(c, -0.5, 0.5, 0.0, 0.5, 1.0, 0.5, mat::VELVET);
}

/// A parasol: pole + a flat canopy disc.
pub fn parasol(c: &mut City, base: Vec3, r: f32, canopy: Material) {
    c.fill(Vec3::new(base.x - 0.25, base.y, base.z - 0.25), Vec3::new(base.x + 0.25, base.y + 3.0, base.z + 0.25), mat::STEEL);
    cylinder(c, base.x, base.z, r, base.y + 3.0, base.y + 3.5, |d, a, _| {
        let seg = ((a + TAU) / TAU * 8.0) as i32;
        Some(if d > r - 0.5 && seg % 2 == 0 { mat::PAINT_WHITE } else { canopy })
    });
}

/// A neon-crowned skyline tower far below the casino (decor; tops stay under the void line).
pub fn skyline_tower(c: &mut City, cx: f32, cz: f32, hw: f32, top: f32, seed: u32) {
    let neon = [mat::NEON_PINK, mat::NEON_CYAN, mat::NEON_VIOLET, mat::NEON_AMBER, mat::NEON_GREEN][(seed % 5) as usize];
    c.fill(Vec3::new(cx - hw, 0.0, cz - hw), Vec3::new(cx + hw, VS, cz + hw), mat::TOWER_CORE);
    c.anchor(glam::IVec3::new((cx / VS).floor() as i32, 0, (cz / VS).floor() as i32));
    c.fill_with(Vec3::new(cx - hw, VS, cz - hw), Vec3::new(cx + hw, top, cz + hw), |p, v, _| {
        // Solid (no inner faces to mesh); only the outer layer is seen.
        let shell = (p.x - cx).abs() > hw - VS || (p.z - cz).abs() > hw - VS;
        if !shell && p.y < top - VS {
            return Some(mat::ROOF_DARK);
        }
        if p.y > top - 1.0 {
            return Some(neon);
        }
        // Lit windows in a loose grid.
        let win = v.y.rem_euclid(4) == 1 && (v.x + v.z).rem_euclid(3) != 0;
        Some(if win && unit(hash3(v.x, v.y, v.z, seed)) < 0.55 { mat::NEON_AMBER } else { mat::ROOF_DARK })
    });
}
