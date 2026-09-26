//! **Cybertron** — a raised city plate on the metal planet, under open space (feature `cybertron`).
//!
//! Private-use fan map (Transformers places / insignia are Hasbro's; see THIRD_PARTY_NOTICES.md —
//! replace before any public release). Gunmetal plates with glowing energon (cyan) and amber seams,
//! concentric ring motifs, two rival cities and the Well of AllSparks between them.
//!
//! Layout (metres; +X east, +Z north, Y up; street level [`STREET`] = 24 m, the plate's rim steps down
//! in two 2 m tiers and then drops into the void — the Sea of Rust haze):
//!
//! ```text
//!            ════ North Expressway (deck 36 m) ═══ RUST GAP (jump over the rust bay) ════
//!   Sea of Rust bay (N, dunes + fallen colossus + giant hand, climb back up the rust drifts)
//!  IACON (W)                           WELL OF ALLSPARKS (0,0)                    KAON (E)
//!  boulevard │ Matrix apse │ Optimus │ HALL OF RECORDS (6 Primes) │ lip ring, ring deck 33 m, │ GLADIATOR PIT │ Megatron
//!  x ≈ -107  │   -96..-92  │  -88    │   -84 .. -36               │ crown ring 58 m           │ (64,0), 8 tiers │ on the east wall
//!            ════ South Expressway (deck 36 m) ═════════════════════════════════════════
//!   space bridge (Iacon gate SW corner ⇄ Kaon gate NE corner) — a bonus shortcut
//! ```
//!
//! - **Competitive shape.** The layout is 180° rotation-symmetric through the Well wherever it
//!   matters (lanes, stairs, expressways, surf lines, cover, spire/tower footprints); the two cities
//!   differ in dressing. Three ways between the cities: the central lane around the Well (lip ring or
//!   the elevated ring deck), the two expressways (flanks, 12 m up, open decks — fast but exposed:
//!   no cover, visible from both cities and the crown), and street level through the outskirts.
//!   The Well's crown ring (58 m) and Iacon's spires are the swing anchors; nothing high has cover.
//! - **KOs.** Void below [`VOID_Y`]: off the plate's rim (the Sea of Rust), down the Well of
//!   AllSparks (a 20 m-wide shaft through the plate), off the far end of the rust bay. Star KO above
//!   the world top. Normal gravity everywhere (competitive: launches read the same as other maps).
//! - **Destruction.** Streets, walls, towers, statues and decks break; an indestructible frame slab
//!   under the whole plate (12 m), the Well lip, pylon/tower cores, gate frames and the transforming
//!   panels (which are sim colliders, not voxels) never do.
//! - **Transforming terrain** ([`MapMover`], flavour, never required): the Well Span (a split
//!   bascule bridge across the Well), the Gallery Span (across the Hall of Records at wall-top
//!   height), the Pit Span (over the gladiator pit), and two expressway drop ramps. Folded they are
//!   decoration; extended they ADD a route. The sim telegraphs every change, never moves a piece
//!   with anyone on it, and only solidifies it when its volume is clear.
//! - **Space bridge** ([`MapPortal`]): two ring gates, indestructible frames, a permanent portal pair.

#![allow(clippy::too_many_arguments)]

use std::sync::OnceLock;

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Quat, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, SurfLine, vx};
use crate::mechanics::{MapMechanics, MapMover, MapPortal, MoverPart};
use crate::city::City;
use crate::material::mat;
use crate::noise::{PI, TAU, datan2, dcos, dsin, fbm2, hash3, unit, value2};
use crate::world::VoxelWorld;

/// World size in voxels (256 × 96 × 176 m).
pub const CYBERTRON_SIZE_VOXELS: IVec3 = IVec3::new(512, 192, 352);
/// Street level (feet height on the plate).
pub const STREET: f32 = 24.0;
/// Falling below this is the void KO (the plate's frame slab is at 12 m).
pub const VOID_Y: f32 = 10.0;
/// Plate half extents (m) before the rim tiers.
pub const PLATE_X: f32 = 120.0;
pub const PLATE_Z: f32 = 80.0;
/// The Well of AllSparks: shaft radius (the lip ring walk runs to [`LIP_R`]).
pub const WELL_R: f32 = 10.0;
pub const LIP_R: f32 = 15.0;
/// The elevated ring deck around the Well.
pub const RING_R0: f32 = 19.0;
pub const RING_R1: f32 = 22.0;
pub const RING_TOP: f32 = 33.0;
/// The crown ring (swing anchor over the Well).
pub const CROWN_R: f32 = 14.0;
pub const CROWN_Y: f32 = 58.0;
/// Expressway deck top.
pub const EXPRESS_TOP: f32 = 36.0;
/// Hall of Records (Iacon): x range, inner half width, wall top.
pub const HALL_X0: f32 = -92.0;
pub const HALL_X1: f32 = -36.0;
pub const HALL_HALF: f32 = 11.0;
pub const HALL_TOP: f32 = 46.0;
/// Kaon's gladiator pit centre, floor radius / height and outer wall radius.
pub const PIT: Vec2 = Vec2::new(64.0, 0.0);
pub const PIT_FLOOR_R: f32 = 13.0;
pub const PIT_FLOOR_Y: f32 = 16.0;
pub const PIT_WALL_R: f32 = 27.0;
/// Space-bridge gates (portal centre, disc radius).
pub const GATE_IACON: Vec3 = Vec3::new(-96.0, STREET + 6.0, -40.0);
pub const GATE_KAON: Vec3 = Vec3::new(96.0, STREET + 6.0, 40.0);
pub const GATE_R: f32 = 6.0;

// ------------------------------------------------------------------------------------------------
// Baked icons (statues, relics) — `cargo run -p bd_shapes --example cybertron_bake --release`.

/// A baked icon: voxels `(x, y, z, tint)` from the minimum corner, +Y up, front towards +Z; tint 0
/// Primary, 1 Accent, 2 Dark, 3 Core.
pub struct Icon {
    pub name: String,
    pub size: IVec3,
    pub voxels: Vec<(IVec3, u8)>,
}

static ICON_BIN: &[u8] = include_bytes!("cybertron_icons.bin");

/// All baked icons (parsed once).
pub fn icons() -> &'static [Icon] {
    static ICONS: OnceLock<Vec<Icon>> = OnceLock::new();
    ICONS.get_or_init(|| {
        let b = ICON_BIN;
        assert!(&b[0..4] == b"CYBI", "cybertron_icons.bin: bad magic");
        let n = u16::from_le_bytes([b[4], b[5]]) as usize;
        let mut i = 6;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let len = b[i] as usize;
            let name = String::from_utf8_lossy(&b[i + 1..i + 1 + len]).into_owned();
            i += 1 + len;
            let size = IVec3::new(b[i] as i32, b[i + 1] as i32, b[i + 2] as i32);
            let count = u32::from_le_bytes([b[i + 3], b[i + 4], b[i + 5], b[i + 6]]) as usize;
            i += 7;
            let mut voxels = Vec::with_capacity(count);
            for k in 0..count {
                let q = &b[i + k * 4..i + k * 4 + 4];
                voxels.push((IVec3::new(q[0] as i32, q[1] as i32, q[2] as i32), q[3]));
            }
            i += count * 4;
            out.push(Icon { name, size, voxels });
        }
        out
    })
}

fn icon(name: &str) -> &'static Icon {
    icons().iter().find(|i| i.name == name).unwrap_or_else(|| panic!("cybertron icon {name:?} missing (re-run cybertron_bake)"))
}

/// Which way an icon's front faces after stamping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    PosZ,
    NegZ,
    PosX,
    NegX,
    /// Lying on its back, front up (+Y), head towards −Z.
    Up,
}

/// Stamp an icon with its bottom centre at `base` (metres), mapping tints to `palette`. Only fills
/// air. Returns the world-space AABB (voxels, inclusive).
fn stamp(c: &mut City, ic: &Icon, base: Vec3, face: Face, palette: [Material; 4]) -> (IVec3, IVec3) {
    let s = ic.size;
    let (mut lo, mut hi) = (IVec3::MAX, IVec3::MIN);
    let o = IVec3::new(vx(base.x), vx(base.y), vx(base.z));
    for &(p, t) in &ic.voxels {
        // Centre the footprint on the origin, then rotate.
        let l = IVec3::new(p.x - s.x / 2, p.y, p.z - s.z / 2);
        let r = match face {
            Face::PosZ => l,
            Face::NegZ => IVec3::new(-l.x, l.y, -l.z),
            Face::PosX => IVec3::new(l.z, l.y, -l.x),
            Face::NegX => IVec3::new(-l.z, l.y, l.x),
            // Front (+Z) up: z → y, y → −z (the head's top points to −Z); keep y ≥ 0.
            Face::Up => IVec3::new(l.x, p.z, -(p.y - s.y / 2)),
        };
        let w = o + r;
        if c.w.get(w) == AIR {
            c.w.set(w, palette[(t as usize).min(3)]);
        }
        lo = lo.min(w);
        hi = hi.max(w);
    }
    (lo, hi)
}

// ------------------------------------------------------------------------------------------------
// Insignia (voxel mosaics). Left halves (13 columns; column 12 is the centre line), top row first.

const AUTOBOT: [&str; 25] = [
    "..........###",
    "..........###",
    "##........###",
    "###.......###",
    "####......###",
    "#####....####",
    "######..#####",
    "#############",
    "#############",
    "#####.....###",
    "######....###",
    "#######...###",
    "#############",
    "##.##########",
    "##..#########",
    "###..########",
    "####..###...#",
    "#####..##.#.#",
    ".#####..#####",
    "..#####..####",
    "...#####..###",
    "....#####.###",
    ".....########",
    "......#######",
    "........#####",
];

const DECEPTICON: [&str; 25] = [
    "............#",
    "...........##",
    "#..........##",
    "##........###",
    "###.......###",
    "####......###",
    "#####....####",
    "######...####",
    "#######..####",
    "########.####",
    "#############",
    ".####......##",
    "..####.....##",
    "...####....##",
    "....#########",
    ".....########",
    "......###.###",
    ".......##.###",
    "........#.###",
    ".........####",
    "..........###",
    "...........##",
    "............#",
    ".............",
    ".............",
];

/// Is cell (col 0..25, row 0..25) of an insignia filled?
pub fn insignia_cell(rows: &[&str; 25], col: i32, row: i32) -> bool {
    if !(0..25).contains(&col) || !(0..25).contains(&row) {
        return false;
    }
    let c = if col < 13 { col } else { 24 - col };
    rows[row as usize].as_bytes()[c as usize] == b'#'
}

/// Inlay an insignia into the floor (top voxel at feet height `y`) centred on (x, z): `up` is the
/// in-plane direction of the insignia's top. Cells are `k` voxels.
fn floor_insignia(c: &mut City, rows: &[&str; 25], centre: Vec2, y: f32, up: Vec2, k: i32, m: Material, bg: Material) {
    let right = Vec2::new(-up.y, up.x);
    let half = 12.5 * k as f32 * 0.5 + 1.0;
    let vy = vx(y) - 1;
    for vz in vx(centre.y - half)..=vx(centre.y + half) {
        for vxx in vx(centre.x - half)..=vx(centre.x + half) {
            let p = Vec2::new((vxx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5) - centre;
            let (u, v) = (p.dot(right) / (0.5 * k as f32) + 12.5, 12.5 - p.dot(up) / (0.5 * k as f32));
            let (col, row) = (u.floor() as i32, v.floor() as i32);
            let at = IVec3::new(vxx, vy, vz);
            if c.w.get(at) == AIR {
                continue;
            }
            if insignia_cell(rows, col, row) {
                c.w.set(at, m);
            } else if (-1..=25).contains(&col) && (-1..=25).contains(&row) {
                c.w.set(at, bg);
            }
        }
    }
}

/// A vertical insignia on the plane x = `x` (facing ±X) or z = `z`, bottom-centre `base`, `k`-voxel cells.
fn wall_insignia(c: &mut City, rows: &[&str; 25], base: Vec3, along_x: bool, k: i32, m: Material, bg: Material) {
    let n = 25 * k;
    let o = IVec3::new(vx(base.x), vx(base.y), vx(base.z));
    for row in 0..n {
        for col in 0..n {
            let d = col - n / 2;
            let at = if along_x { o + IVec3::new(d, n - 1 - row, 0) } else { o + IVec3::new(0, n - 1 - row, d) };
            let filled = insignia_cell(rows, col / k, row / k);
            c.w.set(at, if filled { m } else { bg });
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Plate

/// Signed distance inside the plate's outline (m, positive inside), including the rust bay.
pub fn plate_inside(x: f32, z: f32) -> f32 {
    // Chamfered rectangle.
    let (ax, az) = (x.abs(), z.abs());
    let d_rect = (PLATE_X - ax).min(PLATE_Z - az);
    let d_chamfer = (PLATE_X + PLATE_Z - 22.0 - ax - az) * std::f32::consts::FRAC_1_SQRT_2;
    let jag = 1.4 * value2(x * 0.09, z * 0.09, 0x0c1b);
    let mut d = d_rect.min(d_chamfer) + jag;
    // The rust bay bites into the north edge (|x| < 30).
    if z > 40.0 {
        let bay_z = 54.0 + 24.0 * (x / 30.0) * (x / 30.0);
        d = d.min(bay_z - z + 0.8 * value2(x * 0.2, 7.0, 0x0ba7));
    }
    d
}

/// Street material pattern: gunmetal plates on a 6 m grid with trim seams, concentric energon /
/// amber rings around the Well and the pit, radial seams off the Well lip, scorch and rust patches.
fn street_mat(x: f32, z: f32, seed: u32) -> Material {
    let r = (x * x + z * z).sqrt();
    let glow = if x < 0.0 { mat::ENERGON } else { mat::SEAM_ORANGE };
    for rr in [30.0f32, 42.0, 56.0] {
        if (r - rr).abs() < 0.26 {
            return glow;
        }
    }
    if (LIP_R..34.0).contains(&r) {
        let a = (datan2(z, x) / TAU * 16.0).rem_euclid(1.0);
        if (a - 0.5).abs() * TAU / 16.0 * r < 0.25 {
            return glow;
        }
    }
    let rp = (Vec2::new(x, z) - PIT).length();
    if (PIT_WALL_R + 6.0..PIT_WALL_R + 6.5).contains(&rp) {
        return mat::KAON_RED;
    }
    if x.rem_euclid(6.0) < 0.5 || z.rem_euclid(6.0) < 0.5 {
        return mat::CYBER_TRIM;
    }
    let n = fbm2(x * 0.06, z * 0.06, seed, 3);
    if n > 0.52 {
        return mat::SCORCHED;
    }
    let cell = hash3((x / 6.0).floor() as i32, 0, (z / 6.0).floor() as i32, seed ^ 5);
    if x < -30.0 && unit(cell) < 0.5 {
        mat::CYBER_PLATE_LIGHT
    } else {
        mat::CYBER_PLATE
    }
}

/// The plate: frame slab at 12 m, gunmetal body, street surface with two rim tiers, the Well shaft,
/// the gladiator pit bowl and the rust bay cut-out.
fn plate(c: &mut City) {
    let seed = c.seed();
    let frame_v = vx(12.0);
    let (x0, x1) = (vx(-PLATE_X - 2.0), vx(PLATE_X + 2.0));
    let (z0, z1) = (vx(-PLATE_Z - 2.0), vx(PLATE_Z + 2.0));
    for vz in z0..=z1 {
        for vxx in x0..=x1 {
            let (x, z) = ((vxx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5);
            let d = plate_inside(x, z);
            if d <= 0.0 {
                continue;
            }
            let r = (x * x + z * z).sqrt();
            if r < WELL_R {
                continue;
            }
            let mut top = if d > 6.0 {
                STREET
            } else if d > 3.0 {
                STREET - 2.0
            } else {
                STREET - 4.0
            };
            let mut surf = if top < STREET {
                if fbm2(x * 0.08, z * 0.08, seed ^ 11, 2) > 0.1 { mat::RUST_DUST } else { mat::CYBER_PLATE }
            } else {
                street_mat(x, z, seed)
            };
            // Gladiator pit bowl.
            let rp = (Vec2::new(x, z) - PIT).length();
            if rp < PIT_WALL_R {
                let (t, m) = pit_surface(x, z, rp);
                top = t;
                surf = m;
            }
            let ty = vx(top) - 1;
            // Solid down to the world floor (its underside is never meshed): layered plates with
            // trim bands on the cliffs, the indestructible frame slab at 12 m.
            for y in 0..ty {
                let m = if y == frame_v {
                    mat::CYBER_FRAME
                } else if y.rem_euclid(8) == 0 {
                    mat::CYBER_TRIM
                } else if y == 36 {
                    if x < 0.0 { mat::ENERGON } else { mat::SEAM_ORANGE }
                } else {
                    mat::CYBER_PLATE
                };
                c.w.set_raw(IVec3::new(vxx, y, vz), m);
            }
            c.w.set(IVec3::new(vxx, ty, vz), surf);
            // The Well's shaft wall: energon bands every 3 m, indestructible glowing lip.
            if r < WELL_R + 0.75 {
                for y in 0..ty {
                    let m = if y == frame_v { mat::CYBER_FRAME } else if y.rem_euclid(6) == 0 { mat::ENERGON } else { mat::CYBER_TRIM };
                    c.w.set_raw(IVec3::new(vxx, y, vz), m);
                }
                c.w.set(IVec3::new(vxx, ty, vz), mat::ENERGON_FRAME);
            } else if r < WELL_R + 1.25 {
                c.w.set(IVec3::new(vxx, ty, vz), mat::ENERGON_FRAME);
            }
        }
    }
    // Collapse anchors along the frame slab every 16 m.
    let mut z = -PLATE_Z + 8.0;
    while z < PLATE_Z {
        let mut x = -PLATE_X + 8.0;
        while x < PLATE_X {
            c.anchor(IVec3::new(vx(x), frame_v, vx(z)));
            x += 16.0;
        }
        z += 16.0;
    }
}

/// Pit bowl surface at distance `rp` from the pit centre: floor, eight 1 m tiers, four stair aisles.
fn pit_surface(x: f32, z: f32, rp: f32) -> (f32, Material) {
    if rp < PIT_FLOOR_R {
        let m = if rp > PIT_FLOOR_R - 0.6 { mat::KAON_RED } else if ((x - PIT.x).abs() + (z - PIT.y).abs()) % 4.0 < 0.5 { mat::CYBER_TRIM } else { mat::CYBER_PLATE };
        return (PIT_FLOOR_Y, m);
    }
    let t = (rp - PIT_FLOOR_R) / (PIT_WALL_R - PIT_FLOOR_R);
    let ang = (datan2(z - PIT.y, x - PIT.x) / TAU * 360.0).rem_euclid(360.0);
    let aisle = [45.0f32, 135.0, 225.0, 315.0].iter().any(|a| {
        let d = ((ang - a + 540.0).rem_euclid(360.0) - 180.0).abs();
        d / 360.0 * TAU * rp < 1.6
    });
    if aisle {
        // Half-metre steps.
        let h = (t * 16.0).floor().min(15.0) * 0.5 + 0.5;
        return (PIT_FLOOR_Y + h, if ((t * 16.0).fract()) < 0.2 { mat::SEAM_ORANGE } else { mat::CYBER_PLATE });
    }
    let k = (t * 8.0).floor().min(7.0);
    let lip = (t * 8.0).fract() > 0.82;
    (PIT_FLOOR_Y + 1.0 + k, if lip { mat::CYBER_TRIM } else { mat::CYBER_PLATE })
}

// ------------------------------------------------------------------------------------------------
// Generic brushes

/// Solid wedge ramp along `a → b` (horizontal centreline, metres), feet height `ya → yb`, `width`
/// wide, filled down to `floor_y`. Heights quantise to 0.5 m steps (walkable).
fn ramp(c: &mut City, a: Vec2, b: Vec2, ya: f32, yb: f32, width: f32, floor_y: f32, top: Material, body: Material, edge: Material) {
    let d = b - a;
    let len = d.length();
    let u = d / len;
    let side = Vec2::new(-u.y, u.x);
    let reach = width * 0.5 + 1.0;
    let (lo, hi) = (a.min(b) - Vec2::splat(reach), a.max(b) + Vec2::splat(reach));
    for vz in vx(lo.y)..=vx(hi.y) {
        for vxx in vx(lo.x)..=vx(hi.x) {
            let p = Vec2::new((vxx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5) - a;
            let (s, t) = (p.dot(u), p.dot(side));
            if s < 0.0 || s > len || t.abs() > width * 0.5 {
                continue;
            }
            let h = ya + (yb - ya) * (s / len);
            let ty = vx(h + 0.01) - 1;
            let by = vx(floor_y);
            if ty < by {
                continue;
            }
            c.w.fill_column(vxx, vz, by, ty, body);
            c.w.set(IVec3::new(vxx, ty, vz), if t.abs() > width * 0.5 - 0.5 { edge } else { top });
        }
    }
}

/// A column (tower core / pylon) with an indestructible 1×1 core, `half` metres half width.
fn pylon(c: &mut City, x: f32, z: f32, y0: f32, y1: f32, half: f32, body: Material, band: Material) {
    c.fill_with(Vec3::new(x - half, y0, z - half), Vec3::new(x + half, y1, z + half), |p, _, _| {
        Some(if (p.y - y0).rem_euclid(4.0) < 0.5 { band } else { body })
    });
    c.fill(Vec3::new(x - 0.5, y0 - 1.0, z - 0.5), Vec3::new(x, y1, z), mat::CYBER_FRAME);
    c.anchor(IVec3::new(vx(x - 0.25), vx(y0 + 1.0), vx(z - 0.25)));
}

/// Energon cube stack (glowing pink cubes with dark frames), `n` cubes, 1.5 m each.
fn cube_stack(c: &mut City, x: f32, z: f32, n: u32, seed: u32) {
    let Some(g) = c.ground(x, z, STREET + 4.0) else { return };
    for k in 0..n {
        let off = Vec2::new(if k % 2 == 1 { 0.75 } else { 0.0 }, if k % 3 == 2 { 0.75 } else { 0.0 });
        let (cx, cz) = if k < 3 { (x + (k as f32 - 1.0) * 1.6, z) } else { (x + off.x - 0.8 + (k as f32 - 3.0) * 1.6, z + off.y) };
        let y = if k < 3 { g } else { g + 1.5 };
        let _ = seed;
        c.fill_with(Vec3::new(cx - 0.75, y, cz - 0.75), Vec3::new(cx + 0.75, y + 1.5, cz + 0.75), |p, _, _| {
            let e = |v: f32, cc: f32| (v - cc).abs() > 0.5;
            let edges = (e(p.x, cx) as u8) + (e(p.y, y + 0.75) as u8) + (e(p.z, cz) as u8);
            Some(if edges >= 2 { mat::CYBER_TRIM } else { mat::ENERGON_PINK })
        });
    }
}

// ------------------------------------------------------------------------------------------------
// The Well of AllSparks

fn well(c: &mut City) {
    // The lip ring walk is part of the plate. The elevated ring deck (r 19–22, top 33 m) on six
    // pylons, energon edges, frame nodes so arcs survive losing a pylon.
    let (r0, r1) = (RING_R0, RING_R1);
    let ext = r1 + 1.0;
    c.fill_with(Vec3::new(-ext, RING_TOP - 1.5, -ext), Vec3::new(ext, RING_TOP, ext), |p, _, _| {
        let r = (p.x * p.x + p.z * p.z).sqrt();
        if !(r0..r1).contains(&r) {
            return None;
        }
        let edge = r < r0 + 0.5 || r > r1 - 0.5;
        Some(if p.y > RING_TOP - 0.5 && edge { mat::ENERGON } else if p.y > RING_TOP - 0.5 { mat::CYBER_PLATE_LIGHT } else { mat::CYBER_TRIM })
    });
    for k in 0..6 {
        let a = PI / 6.0 + TAU * k as f32 / 6.0;
        let rm = (r0 + r1) * 0.5;
        let (x, z) = (dcos(a) * rm, dsin(a) * rm);
        pylon(c, x, z, STREET, RING_TOP - 1.5, 1.0, mat::CYBER_PLATE, mat::ENERGON);
    }
    // Four radial stairs up to the ring deck (rotation-symmetric, off the lanes and the axes).
    for k in 0..4 {
        let a = (22.5 + 90.0 * k as f32).to_radians();
        let d = Vec2::new(dcos(a), dsin(a));
        ramp(c, d * 34.0, d * (r1 - 0.25), STREET, RING_TOP, 3.0, STREET - 1.0, mat::CYBER_PLATE, mat::CYBER_PLATE, mat::ENERGON);
    }
    // Crown ring: a square-section torus at 58 m on four slender pylons from r 26 on the diagonals.
    let cy = CROWN_Y;
    c.fill_with(Vec3::new(-CROWN_R - 1.5, cy - 1.0, -CROWN_R - 1.5), Vec3::new(CROWN_R + 1.5, cy + 1.0, CROWN_R + 1.5), |p, _, _| {
        let r = (p.x * p.x + p.z * p.z).sqrt();
        if (r - CROWN_R).abs() > 0.9 || (p.y - cy).abs() > 0.9 {
            return None;
        }
        Some(if r < CROWN_R - 0.4 { mat::ENERGON } else if p.y > cy + 0.4 { mat::CYBER_PLATE_LIGHT } else { mat::CYBER_PLATE })
    });
    for k in 0..4 {
        let a = PI / 4.0 + PI / 2.0 * k as f32;
        let d = Vec3::new(dcos(a), 0.0, dsin(a));
        let foot = d * 26.0 + Vec3::Y * STREET;
        let top = d * CROWN_R + Vec3::Y * cy;
        c.tube(foot, top, 0.8, mat::CYBER_PLATE, false);
        c.line(foot - Vec3::Y, top, mat::CYBER_FRAME, false);
        c.fill(top - Vec3::splat(0.75), top + Vec3::splat(0.75), mat::CYBER_FRAME);
        c.anchor(IVec3::new(vx(top.x), vx(top.y), vx(top.z)));
        // Energon collar where the pylon meets the street.
        c.fill(foot - Vec3::new(1.5, 0.0, 1.5), foot + Vec3::new(1.5, 1.0, 1.5), mat::ENERGON);
    }
    // The AllSpark far below (glowing in the void under the shaft; indestructible, unreachable).
    let al = icon("allspark");
    stamp(c, al, Vec3::new(0.0, 0.5, 0.0), Face::PosZ, [mat::CYBER_FRAME, mat::ENERGON_FRAME, mat::CYBER_FRAME, mat::ENERGON_FRAME]);
    // Well Span abutments: short frame sockets on the lip where the leaves hinge (E/W).
    for sx in [-1.0f32, 1.0] {
        c.fill(Vec3::new(sx * WELL_R - 0.75, STREET - 1.0, -3.0), Vec3::new(sx * WELL_R + 0.75, STREET, 3.0), mat::EMBER_FRAME);
        c.fill(Vec3::new(sx * (WELL_R + 1.0) - 0.5, STREET, -3.5), Vec3::new(sx * (WELL_R + 1.0) + 0.5, STREET + 1.0, -2.5), mat::EMBER_FRAME);
        c.fill(Vec3::new(sx * (WELL_R + 1.0) - 0.5, STREET, 2.5), Vec3::new(sx * (WELL_R + 1.0) + 0.5, STREET + 1.0, 3.5), mat::EMBER_FRAME);
    }
}

// ------------------------------------------------------------------------------------------------
// Iacon

fn hall_of_records(c: &mut City) {
    let (x0, x1, h) = (HALL_X0, HALL_X1, HALL_HALF);
    // Floor: polished blue-steel, energon edge runners, a centre runner.
    c.fill_with(Vec3::new(x0, STREET - 0.5, -h), Vec3::new(x1, STREET, h), |p, _, _| {
        Some(if p.z.abs() > h - 0.5 || p.z.abs() < 0.25 { mat::ENERGON } else if (p.x - x0).rem_euclid(7.0) < 0.5 { mat::CYBER_TRIM } else { mat::CYBER_PLATE_LIGHT })
    });
    floor_insignia(c, &AUTOBOT, Vec2::new(-55.0, 0.0), STREET, Vec2::new(-1.0, 0.0), 1, mat::INSIGNIA_RED, mat::CYBER_TRIM);
    // Walls (z = ±11..±13), 22 m tall, window slits every 4 m, indestructible pilasters every 8 m.
    for sz in [-1.0f32, 1.0] {
        let (za, zb) = if sz > 0.0 { (h, h + 2.0) } else { (-h - 2.0, -h) };
        c.fill_with(Vec3::new(x0 - 2.0, STREET, za), Vec3::new(x1, HALL_TOP, zb), |p, _, _| {
            let s = p.x - x0;
            let rel = p.y - STREET;
            if rel > 21.5 {
                return Some(if (p.z.abs() - h - 1.0).abs() < 0.3 { mat::ENERGON } else { mat::CYBER_PLATE_LIGHT });
            }
            if s.rem_euclid(4.0) < 0.5 && (3.0..18.0).contains(&rel) && s > 2.0 {
                return Some(mat::ENERGON);
            }
            if (rel - 19.5).abs() < 0.26 {
                return Some(mat::ENERGON);
            }
            Some(if s.rem_euclid(8.0) < 0.5 { mat::CYBER_PLATE } else { mat::CYBER_PLATE_LIGHT })
        });
        let mut x = x0 + 4.0;
        while x < x1 {
            let z = sz * (h + 1.0);
            c.fill(Vec3::new(x - 0.5, STREET - 1.0, z - 0.5), Vec3::new(x, HALL_TOP - 1.0, z), mat::CYBER_FRAME);
            c.anchor(IVec3::new(vx(x - 0.25), vx(STREET + 4.0), vx(z - 0.25)));
            x += 8.0;
        }
        // Side doors near the east end.
        c.fill_with(Vec3::new(x1 - 8.0, STREET, za), Vec3::new(x1 - 3.0, STREET + 6.0, zb), |_, _, old| (old != mat::CYBER_FRAME).then_some(AIR));
    }
    // Roof ribs: arched transverse beams every 9 m (tether anchors over the gallery).
    let mut x = x0 + 8.0;
    while x < x1 {
        c.curve(Vec3::new(x, HALL_TOP - 1.0, -h - 1.0), Vec3::new(x, HALL_TOP + 7.0, 0.0), Vec3::new(x, HALL_TOP - 1.0, h + 1.0), 0.75, 0.75, mat::CYBER_PLATE_LIGHT, false);
        c.line(Vec3::new(x, HALL_TOP - 1.5, -h - 1.0), Vec3::new(x, HALL_TOP - 1.5, h + 1.0), mat::ENERGON, true);
        x += 9.0;
    }
    // East entrance arch with the Autobot crest facing the Well.
    c.fill(Vec3::new(x1 - 1.0, STREET + 14.0, -h), Vec3::new(x1, HALL_TOP, h), mat::CYBER_PLATE_LIGHT);
    wall_insignia(c, &AUTOBOT, Vec3::new(x1 + 0.1, STREET + 15.5, 0.0), false, 1, mat::INSIGNIA_RED, mat::CYBER_TRIM);
    // West end: the Matrix chamber (apse behind Optimus), 7 m radius half-dome, 10 m tall.
    let ac = Vec2::new(x0, 0.0);
    c.fill_with(Vec3::new(x0 - 8.5, STREET - 0.5, -8.5), Vec3::new(x0 + 0.5, STREET + 11.5, 8.5), |p, _, _| {
        let q = Vec2::new(p.x, p.z) - ac;
        let r = q.length();
        let rel = p.y - STREET;
        if p.x > x0 + 0.25 {
            return None;
        }
        let dome = (r * r + (rel - 3.0).max(0.0).powi(2)).sqrt();
        if rel < 0.0 {
            return Some(if r < 1.6 { mat::ENERGON } else { mat::CYBER_PLATE_LIGHT });
        }
        if dome < 6.5 {
            return Some(AIR);
        }
        if dome < 8.0 {
            return Some(if (dome - 7.0).abs() < 0.26 || (datan2(q.y, q.x) * 8.0 / PI).rem_euclid(1.0) < 0.12 { mat::ENERGON } else { mat::CYBER_PLATE_LIGHT });
        }
        None
    });
    // The Matrix of Leadership on its pedestal.
    c.fill(Vec3::new(x0 - 4.5, STREET, -1.0), Vec3::new(x0 - 2.5, STREET + 1.5, 1.0), mat::CYBER_TRIM);
    let mx = icon("matrix");
    stamp(c, mx, Vec3::new(x0 - 3.5, STREET + 1.5, 0.0), Face::PosX, [mat::CRYSTAL_GOLD, mat::ENERGON, mat::CYBER_TRIM, mat::ENERGON]);
    // Statues. Optimus Prime at the west end facing down the gallery, three Primes a side.
    let light = [mat::CYBER_PLATE_LIGHT, mat::CYBER_PLATE, mat::CYBER_TRIM, mat::ENERGON];
    c.fill(Vec3::new(x0 + 0.5, STREET, -6.5), Vec3::new(x0 + 9.0, STREET + 1.0, 6.5), mat::CYBER_TRIM);
    stamp(c, icon("optimus"), Vec3::new(x0 + 4.5, STREET + 1.0, 0.0), Face::PosX, light);
    let names = [["rodimus", "ironhide", "prowl"], ["primal", "ratchet", "rodimus"]];
    for (si, sz) in [1.0f32, -1.0].into_iter().enumerate() {
        for (k, sx) in [-76.0f32, -62.0, -48.0].into_iter().enumerate() {
            let z = sz * (h - 3.5);
            c.fill(Vec3::new(sx - 5.5, STREET, z - 3.0), Vec3::new(sx + 5.5, STREET + 1.0, z + 3.0), mat::CYBER_TRIM);
            let face = if sz > 0.0 { Face::NegZ } else { Face::PosZ };
            let name = if si == 1 && k == 2 { "primal" } else { names[si][k] };
            stamp(c, icon(name), Vec3::new(sx, STREET + 1.0, z), face, light);
        }
    }
}

/// An Iacon spire: stepped square tower, blue-steel with energon corner seams and setback bands.
fn spire(c: &mut City, x: f32, z: f32, height: f32, half: f32) {
    let top = STREET + height;
    c.fill_with(Vec3::new(x - half, STREET, z - half), Vec3::new(x + half, top, z + half), |p, _, _| {
        let rel = p.y - STREET;
        let step = (rel / 12.0).floor();
        let hw = half - step * 1.0;
        let (dx, dz) = ((p.x - x).abs(), (p.z - z).abs());
        if dx > hw || dz > hw || hw < 1.0 {
            return None;
        }
        let corner = dx > hw - 0.6 && dz > hw - 0.6;
        if corner || (rel.rem_euclid(12.0) > 11.5) {
            return Some(mat::ENERGON);
        }
        if (dx > hw - 0.5 || dz > hw - 0.5) && (p.x + p.z).rem_euclid(3.0) < 0.5 && rel.rem_euclid(4.0) > 1.0 {
            return Some(mat::GLASS_BLUE);
        }
        Some(mat::CYBER_PLATE_LIGHT)
    });
    c.fill(Vec3::new(x - 0.5, STREET - 1.0, z - 0.5), Vec3::new(x, top + 6.0, z), mat::CYBER_FRAME);
    c.fill(Vec3::new(x - 0.5, top + 5.0, z - 0.5), Vec3::new(x, top + 6.0, z), mat::ENERGON_FRAME);
    c.anchor(IVec3::new(vx(x - 0.25), vx(STREET + 4.0), vx(z - 0.25)));
}

/// A slender energon obelisk (Iacon's counterpart of Kaon's smokestacks).
fn obelisk(c: &mut City, x: f32, z: f32, h: f32) {
    c.fill_with(Vec3::new(x - 1.5, STREET, z - 1.5), Vec3::new(x + 1.5, STREET + h, z + 1.5), |p, _, _| {
        let t = (p.y - STREET) / h;
        let hw = 1.5 * (1.0 - t * 0.7);
        if (p.x - x).abs() > hw || (p.z - z).abs() > hw {
            return None;
        }
        Some(if ((p.x - x).abs() < 0.5 && (p.z - z).abs() < 0.5) || (p.y * 2.0).rem_euclid(5.0) < 0.5 { mat::ENERGON } else { mat::CYBER_PLATE_LIGHT })
    });
    c.fill(Vec3::new(x - 0.5, STREET - 1.0, z - 0.5), Vec3::new(x, STREET + 2.0, z), mat::CYBER_FRAME);
    c.anchor(IVec3::new(vx(x - 0.25), vx(STREET), vx(z - 0.25)));
}

// ------------------------------------------------------------------------------------------------
// Kaon

fn gladiator_pit(c: &mut City) {
    let seed = c.seed();
    floor_insignia(c, &DECEPTICON, PIT, PIT_FLOOR_Y, Vec2::new(1.0, 0.0), 1, mat::INSIGNIA_PURPLE, mat::CYBER_TRIM);
    // Outer wall r 27–29.5, 6 m tall, jagged spikes, three gates (W, N, S); the east side is
    // Megatron's balcony.
    let (w0, w1) = (PIT_WALL_R, PIT_WALL_R + 2.5);
    c.fill_with(Vec3::new(PIT.x - w1 - 1.0, STREET, PIT.y - w1 - 1.0), Vec3::new(PIT.x + w1 + 5.0, STREET + 13.0, PIT.y + w1 + 1.0), |p, _, _| {
        let q = Vec2::new(p.x, p.z) - PIT;
        let r = q.length();
        let rel = p.y - STREET;
        let ang = (datan2(q.y, q.x) / TAU * 360.0).rem_euclid(360.0);
        let ad = |a: f32| ((ang - a + 540.0).rem_euclid(360.0) - 180.0).abs();
        // Megatron's balcony (east): a wider wall block, 6 m.
        if ad(0.0) < 16.0 && (w0..w1 + 5.0).contains(&r) && rel < 6.0 {
            return Some(if rel > 5.5 && r < w0 + 0.5 { mat::KAON_RED } else { mat::CYBER_PLATE });
        }
        if !(w0..w1).contains(&r) {
            return None;
        }
        // Gates.
        if [90.0f32, 180.0, 270.0].iter().any(|&a| ad(a) / 360.0 * TAU * r < 4.0) && rel < 5.5 {
            return None;
        }
        let spike_k = (ang / 12.0).round();
        let spike_a = spike_k * 12.0;
        let sh = 6.0 + 5.0 * unit(hash3(spike_k as i32, 3, 9, seed));
        let in_spike = ad(spike_a) / 360.0 * TAU * r < 1.2 * (1.0 - (rel - 6.0) / (sh - 6.0)).max(0.0);
        if rel >= 6.0 && !in_spike {
            return None;
        }
        if (rel - 4.5).abs() < 0.26 {
            return Some(mat::KAON_RED);
        }
        Some(if rel > 6.0 { mat::STATUE_DARK } else if (ang * 0.5).rem_euclid(6.0) < 0.6 { mat::CYBER_TRIM } else { mat::CYBER_PLATE })
    });
    // Gate lintels (3 m of wall over each gate) are part of the fill above rel 5.5; crest over the
    // west gate facing the Well.
    wall_insignia(c, &DECEPTICON, Vec3::new(PIT.x - w1 - 0.1, STREET + 6.0, PIT.y), false, 1, mat::INSIGNIA_PURPLE, mat::CYBER_TRIM);
    c.fill(Vec3::new(PIT.x - w1, STREET + 6.0, -6.5), Vec3::new(PIT.x - w0, STREET + 18.5, 6.5), mat::CYBER_PLATE);
    // Frame cores in the wall every 30°.
    for k in 0..12 {
        let a = (15.0 + 30.0 * k as f32).to_radians();
        let p = PIT + Vec2::new(dcos(a), dsin(a)) * (w0 + 1.25);
        c.fill(Vec3::new(p.x - 0.5, STREET - 1.0, p.y - 0.5), Vec3::new(p.x, STREET + 5.0, p.y), mat::CYBER_FRAME);
        c.anchor(IVec3::new(vx(p.x - 0.25), vx(STREET + 2.0), vx(p.y - 0.25)));
    }
    // Four cover pillars on the pit floor.
    for k in 0..4 {
        let a = (45.0 + 90.0 * k as f32).to_radians();
        let p = PIT + Vec2::new(dcos(a), dsin(a)) * 8.0;
        c.fill_with(Vec3::new(p.x - 1.0, PIT_FLOOR_Y, p.y - 1.0), Vec3::new(p.x + 1.0, PIT_FLOOR_Y + 3.0, p.y + 1.0), |q, _, _| {
            Some(if q.y > PIT_FLOOR_Y + 2.5 { mat::KAON_RED } else { mat::CYBER_PLATE })
        });
    }
    // Megatron, towering over the east wall, facing into the pit.
    let dark = [mat::STATUE_DARK, mat::CYBER_TRIM, mat::CYBER_TRIM, mat::KAON_RED];
    stamp(c, icon("megatron"), Vec3::new(PIT.x + w1 + 1.5, STREET + 6.0, PIT.y), Face::NegX, dark);
}

/// A jagged Kaon tower: tapering, spiked crown, red window strips, amber bands.
fn kaon_tower(c: &mut City, x: f32, z: f32, height: f32, half: f32) {
    let seed = c.seed();
    let top = STREET + height;
    c.fill_with(Vec3::new(x - half - 1.0, STREET, z - half - 1.0), Vec3::new(x + half + 1.0, top + 8.0, z + half + 1.0), |p, _, _| {
        let rel = p.y - STREET;
        let t = rel / height;
        let hw = half * (1.0 - 0.35 * t) + 0.6 * value2(p.y * 0.3, x, seed);
        let (dx, dz) = ((p.x - x).abs(), (p.z - z).abs());
        if rel > height {
            // Crown spikes at the four corners.
            let sp = (rel - height) / 8.0;
            let cw = hw * 0.35 * (1.0 - sp);
            let near_corner = (dx - hw * 0.7).abs() < cw && (dz - hw * 0.7).abs() < cw;
            return near_corner.then_some(mat::STATUE_DARK);
        }
        if dx > hw || dz > hw {
            return None;
        }
        if (rel - height * 0.5).abs() < 0.3 || (rel - height * 0.8).abs() < 0.3 {
            return Some(mat::SEAM_ORANGE);
        }
        if (dx > hw - 0.5 || dz > hw - 0.5) && (p.x - p.z).rem_euclid(4.0) < 0.5 && rel.rem_euclid(5.0) > 1.5 {
            return Some(mat::KAON_RED);
        }
        Some(if rel.rem_euclid(6.0) < 0.5 { mat::CYBER_TRIM } else { mat::CYBER_PLATE })
    });
    c.fill(Vec3::new(x - 0.5, STREET - 1.0, z - 0.5), Vec3::new(x, top, z), mat::CYBER_FRAME);
    c.anchor(IVec3::new(vx(x - 0.25), vx(STREET + 4.0), vx(z - 0.25)));
}

/// A smokestack with a glowing furnace rim.
fn smokestack(c: &mut City, x: f32, z: f32, h: f32) {
    c.fill_with(Vec3::new(x - 2.5, STREET, z - 2.5), Vec3::new(x + 2.5, STREET + h, z + 2.5), |p, _, _| {
        let r = ((p.x - x).powi(2) + (p.z - z).powi(2)).sqrt();
        let rel = p.y - STREET;
        if r > 2.4 || (r < 1.4 && rel > 3.0) {
            return None;
        }
        Some(if rel > h - 1.0 { mat::KAON_RED } else if rel.rem_euclid(5.0) < 0.5 { mat::SEAM_ORANGE } else { mat::STATUE_DARK })
    });
    c.fill(Vec3::new(x - 2.0, STREET - 1.0, z + 1.5), Vec3::new(x - 1.5, STREET + h - 2.0, z + 2.0), mat::CYBER_FRAME);
    c.anchor(IVec3::new(vx(x - 1.75), vx(STREET + 2.0), vx(z + 1.75)));
}

// ------------------------------------------------------------------------------------------------
// Expressways

/// Centreline z of the north expressway at `x` (the south one is its 180° rotation).
pub fn express_z(x: f32) -> f32 {
    64.0 - 10.0 * (x / 104.0) * (x / 104.0)
}

/// Deck top at `x` along an expressway: flat 36 m over |x| ≤ 72, ramps down to street at |x| = 104,
/// — `None` inside the 8 m Rust Gap (north only: a flat-lip jump over the rust bay).
pub fn express_top(x: f32, north: bool) -> Option<f32> {
    let ax = x.abs();
    if ax > 104.0 {
        return None;
    }
    if north && ax < 4.0 {
        return None;
    }
    Some(if ax <= 72.0 { EXPRESS_TOP } else { EXPRESS_TOP - (EXPRESS_TOP - STREET) * (ax - 72.0) / 32.0 })
}

fn expressways(c: &mut City) {
    let half_w = 6.0;
    for north in [true, false] {
        let s = if north { 1.0 } else { -1.0 };
        let glow = |x: f32| if (x * s) < 0.0 { mat::ENERGON } else { mat::SEAM_ORANGE };
        for vxx in vx(-104.0)..=vx(104.0) {
            let x = (vxx as f32 + 0.5) * 0.5;
            let Some(h) = express_top(x * s, north) else { continue };
            let zc = s * express_z(x * s);
            let ty = vx(h + 0.01) - 1;
            let ramp_part = (x * s).abs() > 72.0;
            for vz in vx(zc - half_w)..=vx(zc + half_w) {
                let z = (vz as f32 + 0.5) * 0.5;
                let t = (z - zc).abs();
                if t > half_w {
                    continue;
                }
                let bottom = if ramp_part { vx(STREET - 1.0) } else { ty - 2 };
                let edge = t > half_w - 0.5;
                let lane = (t - 2.0).abs() < 0.25 && x.rem_euclid(6.0) < 3.0;
                c.w.fill_column(vxx, vz, bottom.max(vx(STREET - 1.0)), ty, if ramp_part { mat::CYBER_PLATE } else { mat::CYBER_TRIM });
                c.w.set(IVec3::new(vxx, ty, vz), if edge { glow(x) } else if lane { mat::PAINT_WHITE } else { mat::ASPHALT });
            }
        }
        // Pylons under the flat part every 12 m (frame cores anchor the deck).
        let mut x: f32 = -66.0;
        while x <= 66.0 {
            if !(north && x.abs() < 8.0) {
                let z = s * express_z(x);
                pylon(c, x, z, STREET, EXPRESS_TOP - 1.0, 1.25, mat::CYBER_PLATE, glow(x));
            }
            x += 12.0;
        }
    }
    // Street-level rim boulevards joining the expressway ends (the vehicle loop).
    for sx in [-1.0f32, 1.0] {
        let x0 = sx * 107.0;
        c.fill_with(Vec3::new(x0 - 6.0, STREET - 0.5, -56.0), Vec3::new(x0 + 6.0, STREET, 56.0), |p, _, old| {
            if old == AIR {
                return None;
            }
            let t = (p.x - x0).abs();
            Some(if t > 5.5 { if sx < 0.0 { mat::ENERGON } else { mat::SEAM_ORANGE } } else if (t - 2.0).abs() < 0.25 && p.z.rem_euclid(6.0) < 3.0 { mat::PAINT_WHITE } else { mat::ASPHALT })
        });
    }
}

// ------------------------------------------------------------------------------------------------
// The Sea of Rust bay (north) and the fallen colossus

/// Dune surface height in the bay (m), `None` where the dunes have poured off into the void.
fn dune_top(x: f32, z: f32) -> Option<f32> {
    let mut h = 15.5 - 0.24 * (z - 54.0) + 1.4 * fbm2(x * 0.07, z * 0.07, 0xd00e, 3);
    // Rust drifts piled against the bay walls: a climb back up to street level on both sides.
    let drift = (x.abs() - 8.0).max(0.0) * 0.55 * (1.0 - ((z - 60.0) / 16.0).clamp(0.0, 1.0));
    h += drift;
    let h = h.min(STREET - 0.5);
    (h > VOID_Y + 0.5).then_some(h)
}

fn rust_bay(c: &mut City) {
    for vz in vx(40.0)..=vx(87.5) {
        for vxx in vx(-36.0)..=vx(36.0) {
            let (x, z) = ((vxx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5);
            if plate_inside(x, z) > 0.0 {
                continue;
            }
            let Some(h) = dune_top(x, z) else { continue };
            let ty = vx(h + 0.01) - 1;
            c.w.fill_column(vxx, vz, 0, ty, mat::RUST_DUNE);
            c.w.set(IVec3::new(vxx, ty, vz), mat::RUST_DUST);
        }
    }
    // The fallen colossus: a giant Decepticon head lying on its back, half sunk in the rust, with a
    // tunnel bored through it (climb it, run through it).
    let head = icon("colossus_head");
    let hc = Vec3::new(-9.0, 9.5, 66.0);
    let (lo, hi) = stamp(c, head, hc, Face::Up, [mat::STATUE_DARK, mat::CYBER_TRIM, mat::RUST_DUNE, mat::CYBER_TRIM]);
    let mid_y = ((lo.y + hi.y) / 2) as f32 * 0.5 + 1.0;
    c.fill_with(Vec3::new(lo.x as f32 * 0.5 - 1.0, mid_y - 1.5, hc.z - 1.75), Vec3::new(hi.x as f32 * 0.5 + 1.5, mid_y + 1.5, hc.z + 1.75), |_, _, old| {
        (old != AIR && old != mat::CYBER_FRAME).then_some(AIR)
    });
    c.fill(Vec3::new(hc.x - 0.5, 10.0, hc.z + 3.0), Vec3::new(hc.x, 12.0, hc.z + 3.5), mat::CYBER_FRAME);
    c.anchor(IVec3::new(vx(hc.x - 0.25), vx(11.0), vx(hc.z + 3.25)));
    // A giant hand reaching out of the rust (east side of the bay): four curled fingers + thumb.
    let base = Vec3::new(13.0, 11.0, 68.0);
    for (k, (dx, len)) in [(-3.0f32, 7.0f32), (-1.0, 9.0), (1.0, 8.5), (3.0, 6.5)].into_iter().enumerate() {
        let a = base + Vec3::new(dx, 0.0, 0.0);
        let mid = a + Vec3::new(0.0, len * 0.65, -0.5);
        let tip = a + Vec3::new(dx * 0.1, len, -2.5 - k as f32 * 0.2);
        c.tube(a, mid, 0.8, mat::CYBER_PLATE, false);
        c.tube(mid, tip, 0.7, mat::CYBER_PLATE, false);
        c.fill(mid - Vec3::splat(0.75), mid + Vec3::splat(0.75), mat::CYBER_TRIM);
    }
    c.tube(base + Vec3::new(-4.5, 0.0, 1.5), base + Vec3::new(-6.5, 4.5, -0.5), 0.8, mat::CYBER_PLATE, false);
    c.fill(base - Vec3::new(4.5, 2.0, 1.5), base + Vec3::new(4.5, 1.0, 2.5), mat::CYBER_PLATE);
    c.fill(base - Vec3::new(0.5, 2.0, 0.5), base + Vec3::new(0.0, 0.5, 0.0), mat::CYBER_FRAME);
    c.anchor(IVec3::new(vx(base.x - 0.25), vx(base.y - 1.0), vx(base.z - 0.25)));
}

// ------------------------------------------------------------------------------------------------
// Space bridge gates

fn gate(c: &mut City, centre: Vec3, normal_x: f32, glow: Material) {
    let (r_in, r_out) = (GATE_R + 0.25, GATE_R + 1.75);
    c.fill_with(centre - Vec3::new(1.0, r_out + 0.5, r_out + 0.5), centre + Vec3::new(1.0, r_out + 0.5, r_out + 0.5), |p, _, _| {
        let q = Vec2::new(p.y - centre.y, p.z - centre.z);
        let r = q.length();
        if !(r_in..r_out).contains(&r) {
            return None;
        }
        let a = (datan2(q.y, q.x) / TAU * 24.0).rem_euclid(1.0);
        Some(if r < r_in + 0.6 || a < 0.18 { glow } else { mat::CYBER_FRAME })
    });
    // Base plinth and side buttresses.
    let foot = centre.y - GATE_R - 1.0;
    c.fill(Vec3::new(centre.x - 3.0, STREET - 0.5, centre.z - 8.5), Vec3::new(centre.x + 3.0, foot.max(STREET), centre.z + 8.5), mat::CYBER_FRAME);
    for sz in [-1.0f32, 1.0] {
        c.fill(Vec3::new(centre.x - 1.5, STREET, centre.z + sz * 8.0 - 1.0), Vec3::new(centre.x + 1.5, centre.y, centre.z + sz * 8.0 + 1.0), mat::CYBER_FRAME);
    }
    c.anchor(IVec3::new(vx(centre.x), vx(STREET), vx(centre.z)));
    let _ = normal_x;
}

// ------------------------------------------------------------------------------------------------
// Transforming terrain + space bridge data (see bd_sim world/map_mech.rs)

/// The five transforming pieces. Extended poses are the solid (route-adding) ones.
pub fn movers() -> Vec<MapMover> {
    let mut out = Vec::new();
    // Well Span: two 9 m leaves across the Well along X at street level, folded = raised upright
    // over the shaft edge (hinged at the lip).
    let mut parts = Vec::new();
    for sx in [-1.0f32, 1.0] {
        // Leaf from the middle (0.25 m) to the abutment's inner face (9.2 m).
        let half = Vec3::new(4.475, 0.4, 2.5);
        let centre = Vec3::new(sx * 4.725, STREET - 0.4, 0.0);
        parts.push(MoverPart {
            center: centre,
            half,
            rot: Quat::IDENTITY,
            hinge: Vec3::new(sx * WELL_R, STREET, 0.0),
            hinge_axis: Vec3::Z,
            fold_angle: -sx * (PI * 0.5 - 0.08),
            fold_offset: Vec3::ZERO,
        });
    }
    out.push(MapMover { name: "well_span", parts, folded_secs: 20.0, extended_secs: 16.0, offset_secs: 4.0 });
    // Gallery Span: two halves slide out of the Hall of Records' wall tops to meet over the gallery.
    let mut parts = Vec::new();
    for sz in [-1.0f32, 1.0] {
        let half = Vec3::new(2.5, 0.4, HALL_HALF * 0.5 + 0.25);
        parts.push(MoverPart {
            center: Vec3::new(-43.5, HALL_TOP - 0.4, sz * (HALL_HALF * 0.5 - 0.25)),
            half,
            rot: Quat::IDENTITY,
            hinge: Vec3::ZERO,
            hinge_axis: Vec3::Y,
            fold_angle: 0.0,
            fold_offset: Vec3::new(0.0, 0.0, sz * (HALL_HALF + 0.5)),
        });
    }
    out.push(MapMover { name: "gallery_span", parts, folded_secs: 18.0, extended_secs: 18.0, offset_secs: 12.0 });
    // Pit Span: a bridge across the gladiator pit (N gate ⇄ S gate) that rises out of the pit floor.
    let parts = vec![MoverPart {
        center: Vec3::new(PIT.x, STREET - 0.4, PIT.y),
        // Rim to rim: from the top tier's inner edge on the north to the south (flush with the street).
        half: Vec3::new(2.5, 0.4, PIT_WALL_R - (PIT_WALL_R - PIT_FLOOR_R) / 8.0 - 0.05),
        rot: Quat::IDENTITY,
        hinge: Vec3::ZERO,
        hinge_axis: Vec3::Y,
        fold_angle: 0.0,
        fold_offset: Vec3::new(0.0, -(STREET - PIT_FLOOR_Y) - 0.9, 0.0),
    }];
    out.push(MapMover { name: "pit_span", parts, folded_secs: 22.0, extended_secs: 14.0, offset_secs: 18.0 });
    // Expressway drop ramps: a 24 m panel hinged at the deck's inner edge, folded upright; extended
    // it slopes down 30° to the street (Kaon north at x = +36, Iacon south at x = −36).
    for s in [1.0f32, -1.0] {
        let x = 36.0 * s;
        let edge_z = s * (express_z(36.0) - 6.0);
        let len = 24.0;
        let slope = std::f32::consts::FRAC_PI_6; // 30°
        let dir = -s; // towards the city (inwards)
        let (sn, cs) = (dsin(slope), dcos(slope));
        let hinge = Vec3::new(x, EXPRESS_TOP, edge_z);
        let centre = hinge + Vec3::new(0.0, -sn * len * 0.5, dir * cs * len * 0.5) + Vec3::new(0.0, -0.4 * cs, 0.0);
        // Tilt about X so the panel's local +Z runs down the slope.
        let rot = Quat::from_rotation_x(dir * slope);
        parts_push_ramp(&mut out, s, hinge, centre, rot, len, dir, slope);
    }
    out
}

fn parts_push_ramp(out: &mut Vec<MapMover>, s: f32, hinge: Vec3, centre: Vec3, rot: Quat, len: f32, dir: f32, slope: f32) {
    let parts = vec![MoverPart {
        center: centre,
        half: Vec3::new(3.0, 0.4, len * 0.5),
        rot,
        hinge,
        hinge_axis: Vec3::X,
        // Fold up to (nearly) vertical above the deck edge.
        fold_angle: -dir * (PI * 0.5 + slope - 0.1),
        fold_offset: Vec3::ZERO,
    }];
    out.push(MapMover { name: if s > 0.0 { "kaon_drop_ramp" } else { "iacon_drop_ramp" }, parts, folded_secs: 16.0, extended_secs: 20.0, offset_secs: if s > 0.0 { 0.0 } else { 10.0 } });
}

/// The space bridge: Iacon gate (SW, facing +X) ⇄ Kaon gate (NE, facing −X).
pub fn portals() -> Vec<MapPortal> {
    vec![MapPortal { a: GATE_IACON, a_normal: Vec3::X, b: GATE_KAON, b_normal: Vec3::NEG_X, radius: GATE_R }]
}

// ------------------------------------------------------------------------------------------------

fn dbg_prune(c: &mut City, what: &str) {
    if std::env::var("CYB_PRUNE").is_ok() {
        eprintln!("prune after {what}: {}", c.w.prune_unanchored());
    }
}

/// Build Cybertron. Deterministic for a given seed (the layout is fixed; details vary).
pub fn generate_cybertron(seed: u64) -> (VoxelWorld, MapInfo) {
    let mut c = City::new(VoxelWorld::centered(CYBERTRON_SIZE_VOXELS), seed ^ 0x0c1b_e270_0000);
    plate(&mut c);
    dbg_prune(&mut c, "plate(&mut c);");
    rust_bay(&mut c);
    dbg_prune(&mut c, "rust_bay(&mut c);");
    well(&mut c);
    dbg_prune(&mut c, "well(&mut c);");
    hall_of_records(&mut c);
    dbg_prune(&mut c, "hall_of_records(&mut c);");
    // Iacon spires / obelisks and their Kaon tower / smokestack rotations (180° about the Well).
    let spires: [(f32, f32, f32, f32); 5] = [(-72.0, 36.0, 62.0, 5.0), (-48.0, 34.0, 46.0, 4.0), (-86.0, -32.0, 70.0, 5.5), (-62.0, -38.0, 54.0, 4.5), (-26.0, -46.0, 42.0, 4.0)];
    for &(x, z, h, hw) in &spires {
        spire(&mut c, x, z, h, hw);
        kaon_tower(&mut c, -x, -z, h * 0.8, hw + 0.5);
    }
    for &(x, z, h) in &[(-40.0f32, 48.0f32, 26.0f32), (-96.0, 24.0, 22.0), (-30.0, 30.0, 18.0)] {
        obelisk(&mut c, x, z, h);
        smokestack(&mut c, -x, -z, h + 8.0);
    }
    gladiator_pit(&mut c);
    dbg_prune(&mut c, "gladiator_pit(&mut c);");
    expressways(&mut c);
    dbg_prune(&mut c, "expressways(&mut c);");
    gate(&mut c, GATE_IACON, 1.0, mat::ENERGON_FRAME);
    gate(&mut c, GATE_KAON, -1.0, mat::EMBER_FRAME);
    // Low cover on the central lane approaches (rotation-symmetric).
    for (x, z) in [(30.0f32, 8.0f32), (30.0, -8.0), (-30.0, 8.0), (-30.0, -8.0)] {
        let glow = if x < 0.0 { mat::ENERGON } else { mat::SEAM_ORANGE };
        c.fill_with(Vec3::new(x - 3.0, STREET, z - 0.75), Vec3::new(x + 3.0, STREET + 1.5, z + 0.75), |p, _, _| {
            Some(if p.y > STREET + 1.0 && (p.z - z).abs() < 0.3 { glow } else { mat::CYBER_PLATE })
        });
    }
    // Energon cube stacks (cover, off spawns and lanes).
    let seed32 = c.seed();
    for &(x, z, n) in &[(-86.0f32, 24.0f32, 5u32), (-24.0, -30.0, 3), (86.0, -24.0, 5), (24.0, 30.0, 3), (-100.0, -52.0, 4), (100.0, 52.0, 4)] {
        cube_stack(&mut c, x, z, n, seed32);
    }

    // Housings: clear breakable terrain inside every extended panel (the ramp foot sits in a slot
    // flush with the street, the gallery halves slide out of the wall tops).
    for m in movers() {
        for p in &m.parts {
            let (ctr, q) = p.pose(1.0);
            let inv = q.inverse();
            let r = p.half.length() + 0.5;
            c.fill_with(ctr - Vec3::splat(r), ctr + Vec3::splat(r), |pt, _, old| {
                let l = inv * (pt - ctr);
                let inside = l.x.abs() <= p.half.x + 0.05 && l.y.abs() <= p.half.y + 0.05 && l.z.abs() <= p.half.z + 0.05;
                (inside && old != AIR && !crate::material::is_indestructible(old)).then_some(AIR)
            });
        }
    }

    // ---- Spawns: five per city, spread, on open street (never on the stairs / gates / spans) ----
    let targets: [(f32, f32); 10] = [
        (-64.0, 0.0),
        (-80.0, 24.0),
        (-104.0, -20.0),
        (-50.0, -24.0),
        (-20.0, 44.0),
        (104.0, 20.0),
        (88.0, -32.0),
        (44.0, 30.0),
        (20.0, -44.0),
        (104.0, -8.0),
    ];
    let mut spawns: Vec<Vec3> = Vec::new();
    for (x, z) in targets {
        if let Some(s) = c.spawn_near(x, z, STREET + 3.0, &spawns, 14.0) {
            spawns.push(s);
        }
    }

    let (world, pruned) = c.finish_counted();
    let wb = world.bounds();
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb { min: Vec3::new(wb.min.x - 14.0, VOID_Y, wb.min.z - 14.0), max: Vec3::new(wb.max.x + 14.0, wb.max.y + 34.0, wb.max.z + 14.0) },
        void_y: VOID_Y,
        center: Vec3::new(0.0, STREET, 0.0),
        // A low amber sun raking across the plates.
        sun_dir: Vec3::new(0.55, 0.42, -0.35).normalize(),
        sky_zenith: [0.006, 0.008, 0.024],
        sky_horizon: [0.30, 0.16, 0.10],
        fog_color: [0.10, 0.06, 0.05],
        ambient: 0.30,
        gravity: None,
        surf_lines: surf_lines(),
        water_sheets: Vec::new(),
        water_walk: false,
        interactables: Default::default(),
        slipstream: None,
        mechanics: MapMechanics { movers: movers(), portals: portals() },
    };
    let districts = vec![
        District { name: "well", center: Vec3::new(0.0, STREET, LIP_R - 1.0), area: [-RING_R1, -RING_R1, RING_R1, RING_R1] },
        District { name: "hall_of_records", center: Vec3::new(-64.0, STREET, 0.0), area: [HALL_X0, -HALL_HALF, HALL_X1, HALL_HALF] },
        District { name: "iacon", center: Vec3::new(-70.0, STREET, 24.0), area: [-120.0, -80.0, -30.0, 80.0] },
        District { name: "kaon", center: Vec3::new(88.0, STREET, -32.0), area: [30.0, -80.0, 120.0, 80.0] },
        District { name: "gladiator_pit", center: Vec3::new(PIT.x, PIT_FLOOR_Y, PIT.y), area: [PIT.x - PIT_WALL_R, -PIT_WALL_R, PIT.x + PIT_WALL_R, PIT_WALL_R] },
        District { name: "sea_of_rust", center: Vec3::new(0.0, 15.0, 62.0), area: [-30.0, 54.0, 30.0, 88.0] },
    ];
    let info = MapInfo {
        name: "Cybertron",
        arena,
        water_y: -1000.0,
        water_color: [0.10, 0.30, 0.62],
        water_deep: [0.02, 0.06, 0.18],
        sun_color: [1.0, 0.82, 0.62],
        sky_mid: [0.05, 0.04, 0.10],
        fog_start: 200.0,
        fog_end: 900.0,
        neon_boost: 1.3,
        districts,
        routes: routes(),
        pruned_voxels: pruned,
    };
    (world, info)
}

/// Two surf lines, 180° symmetric: off the Well's ring deck (run off its outer edge onto the ridge)
/// out over the street toward each expressway — a fast exit from the centre. The north one sails
/// out over the rust bay.
pub fn surf_lines() -> Vec<SurfLine> {
    vec![
        SurfLine { a: Vec3::new(0.0, RING_TOP - 0.5, -(RING_R1 + 1.0)), b: Vec3::new(0.0, 30.5, -50.0), face_deg: 60.0, width: 7.0 },
        SurfLine { a: Vec3::new(0.0, RING_TOP - 0.5, RING_R1 + 1.0), b: Vec3::new(0.0, 30.5, 50.0), face_deg: 60.0, width: 7.0 },
    ]
}

fn routes() -> Vec<Route> {
    let s = STREET;
    vec![
        Route {
            name: "central_lane",
            from: "hall_of_records",
            to: "gladiator_pit",
            kind: RouteKind::Ground,
            // Down the gallery, round the Well's lip (north side), to the pit's west gate.
            waypoints: vec![
                Vec3::new(-80.0, s, 0.0),
                Vec3::new(-36.0, s, 0.0),
                Vec3::new(-16.0, s, 6.0),
                Vec3::new(0.0, s, 12.5),
                Vec3::new(16.0, s, 6.0),
                Vec3::new(PIT.x - PIT_WALL_R - 3.0, s, 0.0),
            ],
            anchors: vec![Vec3::new(0.0, CROWN_Y - 1.0, CROWN_R)],
        },
        Route {
            name: "north_expressway",
            from: "iacon",
            to: "kaon",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(-107.0, s, 50.0), Vec3::new(-104.0, s, express_z(104.0)), Vec3::new(-72.0, EXPRESS_TOP, express_z(72.0)), Vec3::new(-14.0, EXPRESS_TOP, express_z(14.0)), Vec3::new(14.0, EXPRESS_TOP, express_z(14.0)), Vec3::new(72.0, EXPRESS_TOP, express_z(72.0)), Vec3::new(104.0, s, express_z(104.0))],
            anchors: vec![],
        },
        Route {
            name: "well_swing",
            from: "iacon",
            to: "kaon",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(-30.0, s, 0.0), Vec3::new(30.0, s, 0.0)],
            anchors: vec![Vec3::new(0.0, CROWN_Y - 1.0, 0.0)],
        },
        Route {
            name: "surf_south",
            from: "well",
            to: "kaon",
            kind: RouteKind::Mixed,
            waypoints: vec![Vec3::new(-0.8, RING_TOP, -20.5), Vec3::new(0.0, STREET, -52.0)],
            anchors: vec![],
        },
        Route {
            name: "space_bridge",
            from: "iacon",
            to: "kaon",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(GATE_IACON.x + 12.0, s, GATE_IACON.z), Vec3::new(GATE_KAON.x - 12.0, s, GATE_KAON.z)],
            anchors: vec![],
        },
    ]
}
