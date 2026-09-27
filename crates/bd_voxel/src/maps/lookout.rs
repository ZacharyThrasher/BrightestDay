//! **The Lookout** — a white-tiled sky temple floating far above a sea of clouds (DESIGN.md "Maps";
//! feature `lookout`). PRIVATE USE: a fan map of Dragon Ball's Kami's Lookout (see
//! THIRD_PARTY_NOTICES.md); the geometry, layout and voxel art are Brightest Day's own.
//!
//! One big disc (r 56 m, floor 72 m) with nothing around it but sky: every edge is a Smash KO over
//! the void (40 m); the cloud sea far below is drawn by the client (`present::lookout`). No map
//! mechanics — layout, look and flow only (tethers, destruction, the void do the rest).
//!
//! ```text
//!                         N  R7 (high rock, 88)
//!        R2 (80) ·    north garden · palms · beds    · R1 (80)
//!              ┌────── PALACE: wings 79 · hall roof 82 · drum 90 · dome 96.5 ──────┐
//!   W  R4 (70) │ grove: palms,  ║  terrace 73, portico, front beds  ║  TIME CHAMBER   │ R3 (70)  E
//!              │ hedged beds    ║                                   ║  (73.5, roof 81) │
//!              └──── COURTYARD: open tiles, the mosaic (0, −18), four stone lanterns ───┘
//!        R6 (76) ·          rim promenade (r 48–55, a clear bhop ring)          · R5 (76)
//!                     S4 · S3 · S2 · S1 floating stair ↓  KORIN'S TOWER (roof 65, below the rim)
//! ```
//!
//! - **Courtyard** (south half): wide open tiles for big summons and beam fights — the only cover is
//!   four stone lanterns round the mosaic and the hedged beds at its edges.
//! - **Palace** (north): a 1 m terrace; the hall (doors S ×3, N, E/W into the wings; blue glass
//!   windows) with a 23 m atrium up into the drum and dome; wings with outside stairs to their
//!   roofs (79) and short stairs on to the hall roof (82); four corner turrets and the drum as
//!   tether anchors; the dome (96.5) is a tiny, coverless, exposed perch.
//! - **Time chamber** (east): a domed hall on a stepped plinth, three doorways (W grand door, N, E)
//!   into a glowing white room with two pillars and two low blocks (close quarters); outside stairs
//!   on the south face to its roof (81).
//! - **Grove** (west): palms and hedged flower beds (cover), topiary blocks; small gardens north
//!   and south-east too.
//! - **Rim promenade**: the outer ring (r 48–55) is kept clear all the way round (bhop loop).
//! - **Void / recovery**: floating rocks around the rim (low 70 m perches, 76 / 80 / 88 m anchors),
//!   the underside dome + long spire (tether anchors when knocked off), and **Korin's tower**
//!   rising out of the clouds south of the rim: its roof (65 m) catches falls and four floating
//!   stair rocks lead back up to the rim by jumps.
//! - **Destruction**: tiles and everything on the disc break; an indestructible plate 2 m under
//!   the floor (and slender cores in the drum columns, turrets, tower and rocks) keeps the disc —
//!   and every tether anchor — in the sky. Nothing holes through to the void.
//! - Top = star KO (40 m over the world top), sides 24 m past the world edge.

// Generator style: several parallel tables indexed together.
#![allow(clippy::too_many_arguments, clippy::needless_range_loop)]

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::ArenaInfo;
use crate::city::{City, PalmSpec, vx};
use crate::material::mat;
use crate::noise::{PI, TAU, datan2, dcos, dsin, hash3, unit, value2, value3};
use crate::world::VoxelWorld;

/// World dimensions in voxels (192 × 128 × 192 m).
pub const LOOKOUT_SIZE_VOXELS: IVec3 = IVec3::new(384, 256, 384);
/// Walking height of the disc (the tile tops).
pub const FLOOR_Y: f32 = 72.0;
/// Top of the indestructible plate (one voxel layer) under the whole disc.
pub const PLATE_Y: f32 = 70.0;
/// Disc radius (to the outer face of the kerb).
pub const DISC_R: f32 = 56.0;
/// Underside: the bottom of the rim band, and the depth of the underside dome under the centre.
pub const RIM_BOTTOM: f32 = 69.0;
pub const UNDER_DEPTH: f32 = 15.0;
/// The spire hangs from the underside's lowest point down to its tip (well under the KO line).
pub const SPIRE_TOP: f32 = RIM_BOTTOM - UNDER_DEPTH;
pub const SPIRE_TIP: f32 = 18.0;
/// Feet below this = KO.
pub const VOID_Y: f32 = 40.0;
/// Where the client draws the top of the cloud sea (visual only; far under the KO line).
pub const CLOUD_Y: f32 = 8.0;

/// The rim promenade: kept clear of props between these radii (a bhop ring round the disc).
pub const PROMENADE: (f32, f32) = (48.0, 55.0);

/// The courtyard mosaic centre and radius.
pub const MOSAIC: Vec2 = Vec2::new(0.0, -18.0);
pub const MOSAIC_R: f32 = 12.0;
/// Stone lanterns round the mosaic (cover in the open courtyard).
pub const LANTERNS: [Vec2; 4] = [Vec2::new(-11.5, -6.5), Vec2::new(11.5, -6.5), Vec2::new(-11.5, -29.5), Vec2::new(11.5, -29.5)];

/// Palace: terrace (x0, z0, x1, z1) and its top; the hall box and roof; the drum / dome on the hall.
pub const TERRACE: [f32; 4] = [-27.0, 8.0, 27.0, 36.0];
pub const TERRACE_Y: f32 = 73.0;
pub const HALL: [f32; 4] = [-13.0, 12.0, 13.0, 30.0];
pub const HALL_ROOF: f32 = 82.0;
pub const DRUM: Vec2 = Vec2::new(0.0, 21.0);
pub const DRUM_R: f32 = 6.5;
pub const DRUM_TOP: f32 = 90.0;
pub const DOME_TOP: f32 = DRUM_TOP + DRUM_R;
pub const FINIAL_TOP: f32 = 101.5;
/// Wings (east box; the west one is mirrored) and their roof.
pub const WING: [f32; 4] = [13.0, 15.0, 23.0, 27.0];
pub const WING_ROOF: f32 = 79.0;
/// Corner turrets (on the hall corners) and their top (a small dome sits on it).
pub const TURRET_R: f32 = 2.5;
pub const TURRET_TOP: f32 = 86.0;
/// The portico in front of the south face (slab top).
pub const PORTICO_TOP: f32 = 80.0;

/// Time chamber: footprint (x0, z0, x1, z1), floor, roof top, dome.
pub const CHAMBER: [f32; 4] = [27.0, -15.0, 41.0, -1.0];
pub const CHAMBER_FLOOR: f32 = 73.5;
pub const CHAMBER_ROOF: f32 = 81.0;
pub const CHAMBER_C: Vec2 = Vec2::new(34.0, -8.0);

/// Korin's tower: centre, roof deck top, roof radius.
pub const KORIN: Vec2 = Vec2::new(0.0, -76.0);
pub const KORIN_TOP: f32 = 65.0;
pub const KORIN_ROOF_R: f32 = 7.0;
/// The floating stair from Korin's roof back up to the rim: (centre, top, radius) — 1.5 m rises
/// (one jump each), zig-zagging, the last one a step below the rim.
pub const KORIN_STAIR: [(Vec2, f32, f32); 4] = [
    (Vec2::new(-2.5, -66.9), 66.5, 2.0),
    (Vec2::new(2.5, -63.9), 68.0, 2.0),
    (Vec2::new(-2.5, -60.9), 69.5, 2.0),
    (Vec2::new(1.5, -58.0), 71.0, 2.0),
];

/// Floating rocks round the rim: (angle deg from +X toward +Z, radius from the centre, top, size).
pub const ROCKS: [(f32, f32, f32, f32); 7] = [
    (40.0, 68.0, 80.0, 4.0),
    (140.0, 68.0, 80.0, 4.0),
    (-12.0, 66.0, 70.0, 3.5),
    (192.0, 66.0, 70.0, 3.5),
    (-52.0, 70.0, 76.0, 3.0),
    (232.0, 70.0, 76.0, 3.0),
    (90.0, 72.0, 88.0, 3.0),
];

/// Spawn targets (x, z) on the floor: mirrored east/west, two on the centre line; all at floor
/// height, round the outside of the disc, never on the palace / chamber / rocks.
const SPAWN_TARGETS: [(f32, f32); 10] = [
    (0.0, -46.0),
    (0.0, 46.0),
    (-28.0, -38.0),
    (28.0, -38.0),
    (-47.0, 6.0),
    (47.0, 6.0),
    (-40.0, 26.0),
    (40.0, 26.0),
    (-31.0, 36.0),
    (31.0, 36.0),
];

/// The floating rock centres and tops (world metres) — for tests, bots and the preview.
pub fn rocks() -> Vec<(Vec3, f32)> {
    ROCKS
        .iter()
        .map(|&(a, r, top, s)| {
            let t = a * (PI / 180.0);
            (Vec3::new(dcos(t) * r, top, dsin(t) * r), s)
        })
        .collect()
}

/// Is (x, z) on the disc (inside the kerb's outer face)?
pub fn in_disc(p: Vec2) -> bool {
    p.length() <= DISC_R
}

/// Height of the underside at radius `r` (the rim band's bottom at the edge, the dome's lowest
/// point at the centre).
pub fn underside_y(r: f32) -> f32 {
    let t = (r / DISC_R).clamp(0.0, 1.0);
    RIM_BOTTOM - UNDER_DEPTH * (1.0 - t * t).max(0.0).sqrt()
}

// ------------------------------------------------------------------------------------------------
// The disc

/// Floor tile at a (world) voxel column: the grid, the checkerboard of patterned tiles, the mosaic,
/// sand paths and the rim bands.
fn tile_mat(v: IVec3, p: Vec2) -> Material {
    let r = p.length();
    // Rim bands: a gold inlay ring, then sand tiles out to the kerb.
    if r > 54.0 {
        return mat::LK_TILE_SAND;
    }
    if r > 53.0 && r <= 53.5 {
        return mat::LK_GOLD;
    }
    // The mosaic: teal centre, an eight-point sand star outlined in teal, a triple ring.
    let dm = (p - MOSAIC).length();
    if dm < MOSAIC_R {
        if dm >= MOSAIC_R - 0.5 || (10.0..10.5).contains(&dm) {
            return mat::LK_TILE_TEAL;
        }
        if dm >= 10.5 {
            return mat::LK_TILE_SAND;
        }
        if dm < 2.0 {
            return mat::LK_TILE_TEAL;
        }
        let a = datan2(p.y - MOSAIC.y, p.x - MOSAIC.x);
        let f = (a / TAU * 8.0).rem_euclid(1.0);
        let rs = 3.5 + 5.5 * (1.0 - (f - 0.5).abs() * 2.0);
        if dm < rs - 0.5 {
            return mat::LK_TILE_SAND;
        }
        if dm < rs {
            return mat::LK_TILE_TEAL;
        }
        return mat::LK_TILE;
    }
    // Sand paths: mosaic → palace steps, → time chamber, → grove.
    let path = (p.x.abs() < 2.5 && p.y > MOSAIC.y + MOSAIC_R - 1.0 && p.y < TERRACE[1])
        || ((p.y - CHAMBER_C.y).abs() < 2.0 && p.x > 8.0 && p.x < CHAMBER[0] - 2.0)
        || ((p.y - MOSAIC.y).abs() < 2.0 && p.x < -MOSAIC_R + 1.0 && p.x > -30.0);
    if path {
        return mat::LK_TILE_SAND;
    }
    // 4 m tile grid with grout lines; every other tile carries a small sand diamond.
    let (gx, gz) = (v.x.rem_euclid(8), v.z.rem_euclid(8));
    if gx == 0 || gz == 0 {
        return mat::LK_GROUT;
    }
    let (tx, tz) = (v.x.div_euclid(8), v.z.div_euclid(8));
    if (tx + tz).rem_euclid(2) == 0 && (gx - 4).abs() + (gz - 4).abs() <= 1 {
        return mat::LK_TILE_SAND;
    }
    mat::LK_TILE
}

/// The disc: tiles, the stone under them, the plate, the underside dome with ribs, the rim bands and
/// the kerb; then the spire hanging under it.
fn build_disc(c: &mut City) {
    let r_v = vx(DISC_R) + 1;
    let ty = vx(FLOOR_Y) - 1;
    let py = vx(PLATE_Y) - 1;
    for vz in -r_v..=r_v {
        for vxx in -r_v..=r_v {
            let p = Vec2::new((vxx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5);
            let r = p.length();
            if r > DISC_R {
                continue;
            }
            let a = datan2(p.y, p.x);
            // Ribs: 16 radial bands and two rings stand 0.5 m proud of the underside.
            let rib_a = (a / TAU * 16.0).rem_euclid(1.0);
            let rib = (rib_a < 0.5 / (r.max(4.0) * TAU / 16.0) * 2.0) || (r - 40.0).abs() < 0.5 || (r - 24.0).abs() < 0.5;
            let by = vx(underside_y(r) - if rib && r < DISC_R - 1.0 { 0.5 } else { 0.0 });
            c.w.fill_column(vxx, vz, by, ty, if rib { mat::LK_UNDER_RIB } else { mat::LK_UNDERSIDE });
            // Stone under the tiles, the plate (inside the rim) and the rim's outer face bands.
            for y in py..ty {
                let m = if y == py && r <= DISC_R - 1.5 {
                    mat::LK_CORE
                } else if r > DISC_R - 0.5 && y == py + 1 {
                    mat::LK_GOLD
                } else {
                    mat::LK_WALL_SHADE
                };
                c.w.set_raw(IVec3::new(vxx, y, vz), m);
            }
            c.w.set_raw(IVec3::new(vxx, ty, vz), tile_mat(IVec3::new(vxx, ty, vz), p));
            // Kerb: a 0.5 m white lip with a gold top edge (a gap where Korin's stair lands).
            if r > DISC_R - 1.0 {
                let south_gap = p.y < 0.0 && p.x.abs() < 5.0;
                if !south_gap {
                    c.w.set_raw(IVec3::new(vxx, ty + 1, vz), if r > DISC_R - 0.5 { mat::LK_GOLD } else { mat::LK_WALL });
                }
            }
        }
    }
    // Collapse-search hints on the plate every 16 m.
    for vz in (-r_v..=r_v).step_by(32) {
        for vxx in (-r_v..=r_v).step_by(32) {
            c.anchor(IVec3::new(vxx, py, vz));
        }
    }
    // The spire: a tapering needle under the dome's lowest point, banded, gold collar at the top,
    // an indestructible core up into the plate.
    let top = SPIRE_TOP + 0.5;
    c.fill_with(Vec3::new(-8.5, SPIRE_TIP, -8.5), Vec3::new(8.5, top + 1.0, 8.5), |p, _, old| {
        if old != AIR {
            return None;
        }
        let t = ((p.y - SPIRE_TIP) / (top - SPIRE_TIP)).clamp(0.0, 1.0);
        let rad = 0.4 + 7.6 * t * (0.35 + 0.65 * t);
        let d = Vec2::new(p.x, p.z).length();
        if d > rad {
            return None;
        }
        if p.y > top - 1.5 {
            return Some(mat::LK_GOLD);
        }
        Some(if (p.y - SPIRE_TIP).rem_euclid(4.0) < 0.5 { mat::LK_UNDER_RIB } else { mat::LK_UNDERSIDE })
    });
    c.fill(Vec3::new(-0.25, SPIRE_TIP + 2.0, -0.25), Vec3::new(0.25, PLATE_Y, 0.25), mat::LK_CORE);
    c.anchor(IVec3::new(0, vx(SPIRE_TIP + 3.0), 0));
}

// ------------------------------------------------------------------------------------------------
// Small kit

/// A dome over `base` (its centre at the springing line): shell `thick` metres (0 = solid).
fn dome(c: &mut City, base: Vec3, r: f32, thick: f32, m: Material) {
    c.fill_with(Vec3::new(base.x - r, base.y, base.z - r), Vec3::new(base.x + r, base.y + r, base.z + r), |p, _, old| {
        let d = (p - base).length();
        if d > r {
            return None;
        }
        if thick > 0.0 && d < r - thick {
            return (old != AIR).then_some(AIR);
        }
        Some(m)
    });
}

/// A gold finial: a 1 m stem, a ball, a thin spike to `top`.
fn finial(c: &mut City, base: Vec3, top: f32) {
    let h = top - base.y;
    c.fill(Vec3::new(base.x - 0.5, base.y - 0.5, base.z - 0.5), Vec3::new(base.x + 0.5, base.y + h * 0.45, base.z + 0.5), mat::LK_GOLD);
    c.w.fill_sphere(Vec3::new(base.x, base.y + h * 0.55, base.z), 0.8, mat::LK_GOLD, false);
    c.line(Vec3::new(base.x, base.y + h * 0.55, base.z), Vec3::new(base.x, top, base.z), mat::LK_GOLD, false);
}

/// A raised garden bed on the floor over `rect` (x0, z0, x1, z1): white kerb, grass, flowers, a
/// hedge ring `hedge` metres tall (0 = none).
fn garden_bed(c: &mut City, rect: [f32; 4], hedge: f32, flower: Material, seed: u32) {
    let [x0, z0, x1, z1] = rect;
    let f = FLOOR_Y;
    c.fill_with(Vec3::new(x0, f, z0), Vec3::new(x1, f + 0.5 + hedge.max(0.5), z1), |p, v, _| {
        let edge = (p.x - x0).min(x1 - p.x).min(p.z - z0).min(z1 - p.z);
        let y = p.y - f;
        if edge < 0.5 {
            return (y < 0.5).then_some(mat::LK_WALL_SHADE);
        }
        if y < 0.5 {
            return Some(mat::GRASS);
        }
        if hedge > 0.0 && edge < 1.0 {
            return (y < 0.5 + hedge).then_some(mat::LEAVES_GREEN);
        }
        (y < 1.0 && unit(hash3(v.x, v.y, v.z, seed)) < 0.45).then_some(flower)
    });
}

/// A topiary block (clipped hedge cube) standing on the floor.
fn topiary(c: &mut City, at: Vec2, half: f32, h: f32) {
    c.fill(Vec3::new(at.x - half, FLOOR_Y, at.y - half), Vec3::new(at.x + half, FLOOR_Y + 0.5, at.y + half), mat::LK_WALL_SHADE);
    c.fill(Vec3::new(at.x - half + 0.25, FLOOR_Y + 0.5, at.y - half + 0.25), Vec3::new(at.x + half - 0.25, FLOOR_Y + h, at.y + half - 0.25), mat::LEAVES_GREEN);
}

/// A stone lantern (cover): base, shaft, a warm lamp box, a gold cap.
fn lantern(c: &mut City, at: Vec2) {
    let f = FLOOR_Y;
    c.fill(Vec3::new(at.x - 1.0, f, at.y - 1.0), Vec3::new(at.x + 1.0, f + 0.5, at.y + 1.0), mat::LK_WALL_SHADE);
    c.fill(Vec3::new(at.x - 0.75, f + 0.5, at.y - 0.75), Vec3::new(at.x + 0.75, f + 2.0, at.y + 0.75), mat::LK_WALL);
    c.fill(Vec3::new(at.x - 0.5, f + 2.0, at.y - 0.5), Vec3::new(at.x + 0.5, f + 3.0, at.y + 0.5), mat::SK_LANTERN_WARM);
    c.fill(Vec3::new(at.x - 1.0, f + 3.0, at.y - 1.0), Vec3::new(at.x + 1.0, f + 3.5, at.y + 1.0), mat::LK_GOLD);
}

// ------------------------------------------------------------------------------------------------
// The palace

fn terrace(c: &mut City) {
    let [x0, z0, x1, z1] = TERRACE;
    c.fill_with(Vec3::new(x0, FLOOR_Y, z0), Vec3::new(x1, TERRACE_Y, z1), |p, v, _| {
        let edge = (p.x - x0).min(x1 - p.x).min(p.z - z0).min(z1 - p.z);
        if edge < 1.0 && p.y > FLOOR_Y + 0.5 {
            return None; // the lower step ring
        }
        if p.y < TERRACE_Y - 0.5 {
            return Some(mat::LK_WALL_SHADE);
        }
        // Top: sand border, white tiles with grout.
        Some(if edge < 2.0 {
            mat::LK_TILE_SAND
        } else if v.x.rem_euclid(6) == 0 || v.z.rem_euclid(6) == 0 {
            mat::LK_GROUT
        } else {
            mat::LK_TILE
        })
    });
}

/// Is `p` (metres) inside a hall doorway? (Openings in the walls.)
fn hall_door(p: Vec3) -> bool {
    let [x0, z0, x1, z1] = HALL;
    let rel = p.y - TERRACE_Y;
    let south = p.z < z0 + 1.0;
    let north = p.z > z1 - 1.0;
    let side = p.x < x0 + 1.0 || p.x > x1 - 1.0;
    (south && ((p.x.abs() < 3.0 && rel < 6.0) || ((7.0..9.5).contains(&p.x.abs()) && rel < 4.0)))
        || (north && p.x.abs() < 2.5 && rel < 5.0)
        || (side && (p.z - DRUM.y).abs() < 1.5 && rel < 4.0)
}

fn hall(c: &mut City) {
    let [x0, z0, x1, z1] = HALL;
    let (y0, y1) = (TERRACE_Y, HALL_ROOF);
    c.fill_with(Vec3::new(x0, y0, z0), Vec3::new(x1, y1 + 0.5, z1), |p, _, _| {
        let wall = p.x < x0 + 1.0 || p.x > x1 - 1.0 || p.z < z0 + 1.0 || p.z > z1 - 1.0;
        let rel = p.y - y0;
        if p.y >= y1 {
            // Parapet: a low gold-capped lip round the roof edge.
            let lip = p.x < x0 + 0.5 || p.x > x1 - 0.5 || p.z < z0 + 0.5 || p.z > z1 - 0.5;
            return lip.then_some(mat::LK_GOLD);
        }
        let roof = p.y > y1 - 1.0;
        let dd = (Vec2::new(p.x, p.z) - DRUM).length();
        if roof && dd < DRUM_R - 1.0 {
            return Some(AIR); // the atrium opening up into the drum
        }
        if !wall && !roof {
            return Some(AIR);
        }
        if roof {
            return Some(if p.y > y1 - 0.5 && !wall { mat::LK_TILE } else { mat::LK_WALL });
        }
        if hall_door(p) {
            return Some(AIR);
        }
        // Gold frames round the doorways.
        let frame = [Vec3::X * 0.5, -Vec3::X * 0.5, Vec3::Y * 0.5, Vec3::Z * 0.5, -Vec3::Z * 0.5].iter().any(|d| hall_door(p + *d));
        if frame {
            return Some(mat::LK_GOLD);
        }
        // Gold band under the roof line, shaded plinth band.
        if rel > y1 - y0 - 1.5 {
            return Some(mat::LK_GOLD);
        }
        if rel < 0.5 {
            return Some(mat::LK_WALL_SHADE);
        }
        // Arched blue windows: every 4 m along the faces, 1 m wide, 2.5 m tall.
        let along = if p.x < x0 + 1.0 || p.x > x1 - 1.0 { p.z - z0 } else { p.x - x0 };
        let slot = along.rem_euclid(4.0);
        if (1.5..2.5).contains(&slot) && (2.5..5.0).contains(&rel) {
            let arch = rel < 4.5 || (slot - 2.0).abs() < 0.25;
            if arch {
                return Some(mat::LK_WINDOW);
            }
        }
        Some(mat::LK_WALL)
    });
    // Drum: a ring wall with eight open window slots, gold bands top and bottom.
    c.fill_with(Vec3::new(DRUM.x - DRUM_R, y1, DRUM.y - DRUM_R), Vec3::new(DRUM.x + DRUM_R, DRUM_TOP, DRUM.y + DRUM_R), |p, _, _| {
        let q = Vec2::new(p.x, p.z) - DRUM;
        let d = q.length();
        if d > DRUM_R || d < DRUM_R - 1.0 {
            return None;
        }
        let rel = p.y - y1;
        if rel < 0.5 || rel > DRUM_TOP - y1 - 0.5 {
            return Some(mat::LK_GOLD);
        }
        let a = datan2(q.y, q.x);
        // Window slots between the column cores (which stand at 45° + k·90°).
        let f = (a / TAU * 8.0).rem_euclid(1.0);
        if (2.0..5.5).contains(&rel) && (f - 0.5).abs() * (TAU / 8.0) * DRUM_R < 0.6 {
            return Some(AIR);
        }
        Some(mat::LK_WALL)
    });
    // Main dome + finial.
    dome(c, Vec3::new(DRUM.x, DRUM_TOP, DRUM.y), DRUM_R, 1.0, mat::LK_WALL);
    c.fill_with(Vec3::new(DRUM.x - DRUM_R, DRUM_TOP, DRUM.y - DRUM_R), Vec3::new(DRUM.x + DRUM_R, DRUM_TOP + 1.0, DRUM.y + DRUM_R), |p, _, old| {
        let d = (Vec2::new(p.x, p.z) - DRUM).length();
        (old != AIR && d > DRUM_R - 1.0).then_some(mat::LK_GOLD)
    });
    finial(c, Vec3::new(DRUM.x, DOME_TOP - 0.5, DRUM.y), FINIAL_TOP);
    // Columns under the drum ring (their cores hold the drum and dome up).
    for k in 0..4 {
        let a = PI * 0.25 + PI * 0.5 * k as f32;
        let at = DRUM + Vec2::new(dcos(a), dsin(a)) * (DRUM_R - 0.5);
        c.fill(Vec3::new(at.x - 0.75, y0, at.y - 0.75), Vec3::new(at.x + 0.75, y1 - 1.0, at.y + 0.75), mat::LK_WALL);
        c.fill(Vec3::new(at.x - 1.0, y0, at.y - 1.0), Vec3::new(at.x + 1.0, y0 + 0.5, at.y + 1.0), mat::LK_WALL_SHADE);
        c.fill(Vec3::new(at.x - 1.0, y1 - 1.5, at.y - 1.0), Vec3::new(at.x + 1.0, y1 - 1.0, at.y + 1.0), mat::LK_GOLD);
        c.fill(Vec3::new(at.x - 0.25, PLATE_Y - 0.5, at.y - 0.25), Vec3::new(at.x + 0.25, DRUM_TOP - 0.5, at.y + 0.25), mat::LK_CORE);
        c.anchor(IVec3::new(vx(at.x), vx(y0 + 2.0), vx(at.y)));
    }
    // Corner turrets.
    for (tx, tz) in [(x0, z0), (x1, z0), (x0, z1), (x1, z1)] {
        let at = Vec2::new(tx, tz);
        c.fill_with(Vec3::new(tx - TURRET_R, y0, tz - TURRET_R), Vec3::new(tx + TURRET_R, TURRET_TOP, tz + TURRET_R), |p, _, _| {
            let q = Vec2::new(p.x, p.z) - at;
            if q.length() > TURRET_R {
                return None;
            }
            let rel = p.y - y0;
            Some(if rel < 0.5 {
                mat::LK_WALL_SHADE
            } else if (p.y - (TURRET_TOP - 0.5)).abs() < 0.3 || (p.y - (y1 - 0.25)).abs() < 0.3 {
                mat::LK_GOLD
            } else if (6.0..8.0).contains(&rel) && q.length() > TURRET_R - 0.6 && (datan2(q.y, q.x) / TAU * 6.0).rem_euclid(1.0) < 0.3 {
                mat::LK_WINDOW
            } else {
                mat::LK_WALL
            })
        });
        dome(c, Vec3::new(tx, TURRET_TOP, tz), TURRET_R, 0.0, mat::LK_WALL);
        finial(c, Vec3::new(tx, TURRET_TOP + TURRET_R - 0.5, tz), TURRET_TOP + TURRET_R + 2.0);
        c.fill(Vec3::new(tx - 0.5, PLATE_Y - 0.5, tz - 0.5), Vec3::new(tx + 0.5, TURRET_TOP, tz + 0.5), mat::LK_CORE);
        c.anchor(IVec3::new(vx(tx), vx(y0 + 3.0), vx(tz)));
    }
}

fn wings(c: &mut City) {
    for s in [-1.0f32, 1.0] {
        let [wx0, z0, wx1, z1] = WING;
        let (x0, x1) = if s > 0.0 { (wx0, wx1) } else { (-wx1, -wx0) };
        let (y0, y1) = (TERRACE_Y, WING_ROOF);
        let cx = s * 18.0;
        c.fill_with(Vec3::new(x0, y0, z0), Vec3::new(x1, y1, z1), |p, _, old| {
            let wall = p.x < x0 + 1.0 || p.x > x1 - 1.0 || p.z < z0 + 1.0 || p.z > z1 - 1.0;
            let roof = p.y > y1 - 1.0;
            let rel = p.y - y0;
            if !wall && !roof {
                return Some(AIR);
            }
            if roof {
                return Some(if p.y > y1 - 0.5 && !wall { mat::LK_TILE } else { mat::LK_WALL });
            }
            let inner = if s > 0.0 { p.x < x0 + 1.0 } else { p.x > x1 - 1.0 };
            // Doors: north + south (centred on the wing), and into the hall.
            let ns = (p.z < z0 + 1.0 || p.z > z1 - 1.0) && (p.x - cx).abs() < 1.5 && rel < 4.0;
            let into_hall = inner && (p.z - DRUM.y).abs() < 1.5 && rel < 4.0;
            if ns || into_hall {
                return Some(AIR);
            }
            if inner && old != AIR {
                return None; // leave the hall's own wall (and its frames) alone
            }
            if rel > y1 - y0 - 1.5 {
                return Some(mat::LK_GOLD);
            }
            if rel < 0.5 {
                return Some(mat::LK_WALL_SHADE);
            }
            let ns_face = p.z < z0 + 1.0 || p.z > z1 - 1.0;
            if ns_face && (2.5..4.5).contains(&rel) && ((p.x - cx).abs() - 3.25).abs() < 0.5 {
                return Some(mat::LK_WINDOW);
            }
            Some(mat::LK_WALL)
        });
        // Little dome + finial.
        dome(c, Vec3::new(cx, y1, 20.5), 2.5, 0.0, mat::LK_WALL);
        finial(c, Vec3::new(cx, y1 + 2.0, 20.5), y1 + 4.5);
        // Outside stairs up the outer face (terrace → wing roof), then a short flight on the roof up
        // to the hall roof along the north edge.
        let ox = if s > 0.0 { x1 + 1.0 } else { x0 - 1.0 };
        c.stairs(Vec3::new(ox, TERRACE_Y, 12.5), Vec2::Y, WING_ROOF, 2.0, 0.75, mat::LK_WALL);
        c.stairs(Vec3::new(s * 17.5, WING_ROOF, 25.25), Vec2::new(-s, 0.0), HALL_ROOF, 2.5, 0.75, mat::LK_WALL);
    }
}

fn portico(c: &mut City) {
    let (z0, z1) = (8.5, HALL[1]);
    c.fill_with(Vec3::new(-10.75, PORTICO_TOP - 1.0, z0), Vec3::new(10.75, PORTICO_TOP, z1), |p, _, _| {
        let edge = p.z < z0 + 0.5 || p.x.abs() > 10.25;
        Some(if edge || p.y < PORTICO_TOP - 0.5 { mat::LK_GOLD } else { mat::LK_WALL })
    });
    for x in [-10.0f32, -4.5, 4.5, 10.0] {
        let z = 9.25;
        c.fill(Vec3::new(x - 0.75, TERRACE_Y, z - 0.75), Vec3::new(x + 0.75, PORTICO_TOP - 1.0, z + 0.75), mat::LK_WALL);
        c.fill(Vec3::new(x - 1.0, TERRACE_Y, z - 1.0), Vec3::new(x + 1.0, TERRACE_Y + 0.5, z + 1.0), mat::LK_WALL_SHADE);
        c.fill(Vec3::new(x - 1.0, PORTICO_TOP - 1.5, z - 1.0), Vec3::new(x + 1.0, PORTICO_TOP - 1.0, z + 1.0), mat::LK_GOLD);
    }
}

// ------------------------------------------------------------------------------------------------
// The time chamber

/// Is `p` inside one of the chamber's doorways?
fn chamber_door(p: Vec3) -> bool {
    let [x0, _, x1, z1] = CHAMBER;
    let rel = p.y - CHAMBER_FLOOR;
    let west = p.x < x0 + 1.0 && (p.z - CHAMBER_C.y).abs() < 2.0 && rel < 5.0;
    let north = p.z > z1 - 1.0 && (p.x - CHAMBER_C.x).abs() < 1.25 && rel < 3.5;
    let east = p.x > x1 - 1.0 && (p.z - CHAMBER_C.y).abs() < 1.25 && rel < 3.5;
    west || north || east
}

fn chamber(c: &mut City) {
    let [x0, z0, x1, z1] = CHAMBER;
    // Stepped plinth: three 0.5 m steps, each 1 m wide.
    for k in 0..3 {
        let o = 3.0 - k as f32;
        let y = FLOOR_Y + 0.5 * (k + 1) as f32;
        c.fill(Vec3::new(x0 - o, FLOOR_Y, z0 - o), Vec3::new(x1 + o, y, z1 + o), if k == 2 { mat::LK_TILE_SAND } else { mat::LK_WALL_SHADE });
    }
    let (y0, y1) = (CHAMBER_FLOOR, CHAMBER_ROOF);
    c.fill_with(Vec3::new(x0, y0 - 0.5, z0), Vec3::new(x1, y1 + 0.5, z1), |p, _, _| {
        let wall = p.x < x0 + 1.0 || p.x > x1 - 1.0 || p.z < z0 + 1.0 || p.z > z1 - 1.0;
        let rel = p.y - y0;
        if p.y >= y1 {
            let lip = p.x < x0 + 0.5 || p.x > x1 - 0.5 || p.z < z0 + 0.5 || p.z > z1 - 0.5;
            return lip.then_some(mat::LK_GOLD);
        }
        if rel < 0.0 {
            // The room's floor.
            return Some(if wall { mat::LK_WALL_SHADE } else { mat::LK_CHAMBER_WHITE });
        }
        let roof = p.y > y1 - 1.0;
        if !wall && !roof {
            return Some(AIR);
        }
        if roof {
            // Glowing ceiling inside, white roof on top.
            return Some(if p.y < y1 - 0.5 && !wall { mat::LK_CHAMBER_WHITE } else { mat::LK_WALL });
        }
        if chamber_door(p) {
            return Some(AIR);
        }
        // Inner skin glows white; the outside is palace plaster with a gold roof band.
        let inner = p.x > x0 + 0.5 && p.x < x1 - 0.5 && p.z > z0 + 0.5 && p.z < z1 - 0.5;
        if inner {
            return Some(mat::LK_CHAMBER_WHITE);
        }
        let frame = [Vec3::X * 0.5, -Vec3::X * 0.5, Vec3::Y * 0.5, Vec3::Z * 0.5, -Vec3::Z * 0.5].iter().any(|d| chamber_door(p + *d));
        if frame || rel > y1 - y0 - 1.5 {
            return Some(mat::LK_GOLD);
        }
        if rel < 0.5 {
            return Some(mat::LK_WALL_SHADE);
        }
        Some(mat::LK_WALL)
    });
    // Close-quarters furniture: two pillars (full height) and two low white blocks.
    let cc = CHAMBER_C;
    for (x, z) in [(cc.x - 3.0, cc.y + 4.0), (cc.x + 3.0, cc.y - 4.0)] {
        c.fill(Vec3::new(x - 0.75, y0, z - 0.75), Vec3::new(x + 0.75, y1 - 1.0, z + 0.75), mat::LK_CHAMBER_WHITE);
    }
    for (x, z) in [(cc.x + 3.5, cc.y + 4.5), (cc.x - 3.5, cc.y - 4.5)] {
        c.fill(Vec3::new(x - 1.25, y0, z - 1.0), Vec3::new(x + 1.25, y0 + 1.0, z + 1.0), mat::LK_WALL_SHADE);
    }
    // Dome + finial on the roof.
    dome(c, Vec3::new(CHAMBER_C.x, y1, CHAMBER_C.y), 4.5, 0.0, mat::LK_WALL);
    c.fill_with(Vec3::new(CHAMBER_C.x - 4.5, y1, CHAMBER_C.y - 4.5), Vec3::new(CHAMBER_C.x + 4.5, y1 + 0.5, CHAMBER_C.y + 4.5), |p, _, old| {
        let d = (Vec2::new(p.x, p.z) - CHAMBER_C).length();
        (old != AIR && d > 3.5).then_some(mat::LK_GOLD)
    });
    finial(c, Vec3::new(CHAMBER_C.x, y1 + 4.0, CHAMBER_C.y), y1 + 7.0);
    // Outside stairs along the south face up to the roof (bottom west, top east).
    c.stairs(Vec3::new(x0 + 0.5, y0, z0 - 1.25), Vec2::X, y1, 2.0, 0.8, mat::LK_WALL);
    // Corner spines (inside the walls) keep the chamber standing.
    for (cx, cz) in [(x0 + 0.5, z0 + 0.5), (x1 - 0.5, z1 - 0.5)] {
        c.fill(Vec3::new(cx - 0.25, PLATE_Y - 0.5, cz - 0.25), Vec3::new(cx + 0.25, y1 - 0.5, cz + 0.25), mat::LK_CORE);
        c.anchor(IVec3::new(vx(cx), vx(y0 + 2.0), vx(cz)));
    }
}

// ------------------------------------------------------------------------------------------------
// Korin's tower, the floating rocks

fn korin_tower(c: &mut City) {
    let k = KORIN;
    // Shaft: out of the clouds up to the bulb, red bands every 6 m.
    c.fill_with(Vec3::new(k.x - 4.5, 0.0, k.y - 4.5), Vec3::new(k.x + 4.5, 50.0, k.y + 4.5), |p, _, _| {
        let d = (Vec2::new(p.x, p.z) - k).length();
        let r = 3.5 + 0.8 * (1.0 - p.y / 50.0);
        if d > r {
            return None;
        }
        Some(if p.y.rem_euclid(6.0) < 0.5 { mat::LK_TOWER_RED } else { mat::LK_TOWER_STONE })
    });
    // The bulb (sanctuary): swells out and back, a ring of blue windows, a neck and a flat roof deck.
    c.fill_with(Vec3::new(k.x - 8.0, 50.0, k.y - 8.0), Vec3::new(k.x + 8.0, KORIN_TOP, k.y + 8.0), |p, _, _| {
        let q = Vec2::new(p.x, p.z) - k;
        let d = q.length();
        let r = if p.y < 62.0 {
            let t = (p.y - 50.0) / 12.0;
            3.5 + 4.0 * dsin(PI * t)
        } else if p.y < KORIN_TOP - 1.5 {
            4.5
        } else {
            KORIN_ROOF_R
        };
        if d > r {
            return None;
        }
        if p.y >= KORIN_TOP - 0.5 {
            return Some(if d > KORIN_ROOF_R - 0.5 { mat::LK_TOWER_RED } else { mat::LK_TOWER_STONE });
        }
        if p.y >= KORIN_TOP - 1.5 {
            return Some(mat::LK_TOWER_RED);
        }
        if (55.0..57.5).contains(&p.y) && d > r - 0.6 && (datan2(q.y, q.x) / TAU * 12.0).rem_euclid(1.0) < 0.45 {
            return Some(mat::LK_WINDOW);
        }
        Some(if (p.y - 52.0).abs() < 0.3 || (p.y - 60.0).abs() < 0.3 { mat::LK_TOWER_RED } else { mat::LK_TOWER_STONE })
    });
    // A little gold jar on the roof (standable cover, 1.5 m).
    c.w.fill_ellipsoid(Vec3::new(k.x + 3.0, KORIN_TOP + 0.6, k.y - 2.0), Vec3::new(0.9, 0.9, 0.9), mat::LK_GOLD, true);
    c.fill(Vec3::new(k.x + 2.75, KORIN_TOP, k.y - 2.25), Vec3::new(k.x + 3.25, KORIN_TOP + 0.5, k.y - 1.75), mat::LK_GOLD);
    // Spine.
    c.fill(Vec3::new(k.x - 0.25, 0.0, k.y - 0.25), Vec3::new(k.x + 0.25, KORIN_TOP - 1.0, k.y + 0.25), mat::LK_CORE);
    c.anchor(IVec3::new(vx(k.x), vx(30.0), vx(k.y)));
}

/// A floating rock: a flat grassy top of radius `r` at `top`, an earthy inverted cone under it
/// `depth` deep, a small indestructible core inside.
fn float_rock(c: &mut City, at: Vec2, top: f32, r: f32, depth: f32) {
    let seed = c.seed();
    c.fill_with(Vec3::new(at.x - r - 1.0, top - depth, at.y - r - 1.0), Vec3::new(at.x + r + 1.0, top, at.y + r + 1.0), |p, _, _| {
        let d = (Vec2::new(p.x, p.z) - at).length();
        let below = top - p.y;
        let t = (below / depth).clamp(0.0, 1.0);
        let wob = 1.0 + 0.14 * value3(p.x * 0.5, p.y * 0.4, p.z * 0.5, seed);
        let allowed = if below < 1.0 { r * (1.0 + 0.08 * value2(p.x * 0.7, p.z * 0.7, seed)) } else { r * (1.0 - t) * (1.15 - 0.35 * t) * wob };
        if d > allowed {
            return None;
        }
        Some(if below < 0.5 {
            mat::GRASS
        } else if below < 1.5 {
            mat::DIRT
        } else if (p.y * 0.7 + value2(p.x * 0.3, p.z * 0.3, seed)).rem_euclid(3.0) < 1.0 {
            mat::DEEP_STONE
        } else {
            mat::STONE
        })
    });
    let core_y = top - (depth * 0.3).min(2.0) - 0.5;
    c.fill(Vec3::new(at.x - 0.5, core_y - 0.5, at.y - 0.5), Vec3::new(at.x + 0.5, core_y + 0.5, at.y + 0.5), mat::CORE);
    c.anchor(IVec3::new(vx(at.x), vx(core_y), vx(at.y)));
}

// ------------------------------------------------------------------------------------------------
// Gardens

/// Where decor must not go: spawn targets, the promenade, doorways and stairs, the mosaic.
fn keep_clear(p: Vec2) -> bool {
    let r = p.length();
    SPAWN_TARGETS.iter().any(|&(x, z)| (p - Vec2::new(x, z)).length() < 6.0) || r > PROMENADE.0 - 1.5 || (p - MOSAIC).length() < MOSAIC_R + 2.0
}

fn gardens(c: &mut City) {
    let seed = c.seed();
    let flowers = [mat::LEAVES_RED, mat::LEAVES_GOLD, mat::LEAVES_PINK, mat::LEAVES_ORANGE];
    // Beds flanking the palace steps.
    garden_bed(c, [-21.0, 2.0, -9.0, 6.0], 1.0, flowers[(seed % 4) as usize], seed ^ 1);
    garden_bed(c, [9.0, 2.0, 21.0, 6.0], 1.0, flowers[((seed >> 3) % 4) as usize], seed ^ 2);
    // The grove (west): hedged beds between the palms.
    let grove_beds: [[f32; 4]; 5] = [
        [-38.0, -18.0, -30.0, -13.0],
        [-26.0, -12.0, -19.0, -6.0],
        [-40.0, 2.0, -33.0, 8.0],
        [-31.0, -31.0, -23.0, -26.0],
        [-26.0, 6.0, -20.0, 11.0],
    ];
    for (i, b) in grove_beds.iter().enumerate() {
        let h = if i % 2 == 0 { 1.0 } else { 1.5 };
        garden_bed(c, *b, h, flowers[(i + seed as usize) % 4], seed ^ (10 + i as u32));
    }
    for (at, half, h) in [(Vec2::new(-34.0, -4.0), 1.0, 2.0), (Vec2::new(-21.0, -20.0), 1.0, 2.0), (Vec2::new(-38.0, -26.0), 1.25, 2.5)] {
        topiary(c, at, half, h);
    }
    // North garden (behind the palace) and a south-east garden by the chamber.
    garden_bed(c, [-16.0, 39.0, -8.0, 44.0], 1.0, flowers[((seed >> 5) % 4) as usize], seed ^ 20);
    garden_bed(c, [8.0, 39.0, 16.0, 44.0], 1.0, flowers[((seed >> 7) % 4) as usize], seed ^ 21);
    garden_bed(c, [18.0, -30.0, 24.0, -24.0], 1.5, flowers[((seed >> 9) % 4) as usize], seed ^ 22);
    garden_bed(c, [30.0, -30.0, 36.0, -25.0], 1.0, flowers[((seed >> 11) % 4) as usize], seed ^ 23);
    topiary(c, Vec2::new(20.0, -15.0), 1.0, 2.0);
    // Palms: the grove, the north garden, the south-east corner (heights / leans vary by seed).
    let palms: [(f32, f32); 14] = [
        (-40.0, -22.0),
        (-34.0, -27.5),
        (-44.0, -13.0),
        (-29.0, -16.0),
        (-37.5, -8.0),
        (-27.0, -3.0),
        (-36.0, 12.0),
        (-20.0, -27.0),
        (-41.0, 12.0),
        (-19.0, 41.0),
        (19.0, 41.0),
        (26.0, -25.0),
        (37.0, -22.0),
        (22.0, -34.0),
    ];
    for (i, &(x, z)) in palms.iter().enumerate() {
        let p = Vec2::new(x, z);
        if keep_clear(p) {
            continue;
        }
        let h = c.rng.range(8.0, 12.5);
        let mut spec = PalmSpec::new(Vec3::new(x, FLOOR_Y, z), h);
        let a = c.rng.range(0.0, TAU);
        spec.lean = Vec2::new(dcos(a), dsin(a)) * c.rng.range(0.6, 2.2);
        spec.fronds = 7 + (i % 3) as i32;
        c.palm(&spec);
    }
    for at in LANTERNS {
        lantern(c, at);
    }
}

// ------------------------------------------------------------------------------------------------

/// Build the Lookout. Deterministic for a given seed (the layout is fixed; details vary).
pub fn generate_lookout(seed: u64) -> (VoxelWorld, MapInfo) {
    let mut c = City::new(VoxelWorld::centered(LOOKOUT_SIZE_VOXELS), seed ^ 0x6c6f_6f6b_6f75_7400);
    build_disc(&mut c);
    terrace(&mut c);
    hall(&mut c);
    wings(&mut c);
    portico(&mut c);
    chamber(&mut c);
    gardens(&mut c);
    korin_tower(&mut c);
    for (at, top, r) in KORIN_STAIR {
        float_rock(&mut c, at, top, r, 4.0);
    }
    for (p, s) in rocks() {
        float_rock(&mut c, Vec2::new(p.x, p.z), p.y, s, s * 2.2 + 2.0);
    }
    // Small debris chunks drifting further out (backdrop + far anchors).
    for k in 0..8 {
        let a = TAU * (k as f32 + 0.5) / 8.0 + c.rng.range(-0.15, 0.15);
        let r = c.rng.range(80.0, 88.0);
        let top = c.rng.range(62.0, 92.0);
        let s = c.rng.range(1.5, 2.5);
        float_rock(&mut c, Vec2::new(dcos(a) * r, dsin(a) * r), top, s, s * 2.0 + 1.0);
    }

    // ---- spawns ---------------------------------------------------------------------------------
    let mut spawns: Vec<Vec3> = Vec::new();
    for (x, z) in SPAWN_TARGETS {
        if let Some(s) = c.spawn_near(x, z, FLOOR_Y + 2.0, &spawns, 12.0) {
            spawns.push(s);
        }
    }

    let (world, pruned) = c.finish_counted();
    let wb = world.bounds();
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb {
            min: Vec3::new(wb.min.x - 24.0, VOID_Y, wb.min.z - 24.0),
            max: Vec3::new(wb.max.x + 24.0, wb.max.y + 40.0, wb.max.z + 24.0),
        },
        void_y: VOID_Y,
        center: Vec3::new(MOSAIC.x, FLOOR_Y, MOSAIC.y + 6.0),
        // High, bright sun from the south-east: the palace's front and the courtyard are lit, the
        // domes throw short shadows toward the grove.
        sun_dir: Vec3::new(0.38, 0.80, -0.46).normalize(),
        sky_zenith: [0.14, 0.36, 0.84],
        sky_horizon: [0.70, 0.84, 0.97],
        fog_color: [0.76, 0.86, 0.97],
        ambient: 0.55,
        gravity: None,
        surf_lines: Vec::new(),
        water_sheets: Vec::new(),
        water_walk: false,
        interactables: Default::default(),
        slipstream: None,
        mechanics: Default::default(),
    };
    let d = |name: &'static str, x: f32, y: f32, z: f32, area: [f32; 4]| District { name, center: Vec3::new(x, y, z), area };
    let districts = vec![
        d("courtyard", MOSAIC.x, FLOOR_Y, MOSAIC.y, [-30.0, -46.0, 30.0, 4.0]),
        d("palace", DRUM.x, TERRACE_Y, DRUM.y, [HALL[0], HALL[1], HALL[2], HALL[3]]),
        d("palace_roof", DRUM.x + 9.0, HALL_ROOF, DRUM.y, [HALL[0], HALL[1], HALL[2], HALL[3]]),
        d("time_chamber", CHAMBER_C.x, CHAMBER_FLOOR, CHAMBER_C.y, CHAMBER),
        d("grove", -33.0, FLOOR_Y, -10.0, [-48.0, -36.0, -18.0, 12.0]),
        d("north_garden", 0.0, FLOOR_Y, 44.0, [-30.0, 36.0, 30.0, 55.0]),
        d("korin", KORIN.x, KORIN_TOP, KORIN.y, [KORIN.x - KORIN_ROOF_R, KORIN.y - KORIN_ROOF_R, KORIN.x + KORIN_ROOF_R, KORIN.y + KORIN_ROOF_R]),
    ];
    let info = MapInfo {
        name: "The Lookout",
        arena,
        // No water: the clouds far below are drawn by the client.
        water_y: -1000.0,
        water_color: [0.9, 0.93, 1.0],
        water_deep: [0.6, 0.7, 0.9],
        sun_color: [1.0, 0.97, 0.90],
        sky_mid: [0.42, 0.62, 0.92],
        fog_start: 90.0,
        fog_end: 900.0,
        neon_boost: 0.2,
        districts,
        routes: routes(),
        pruned_voxels: pruned,
    };
    (world, info)
}

fn routes() -> Vec<Route> {
    let promenade: Vec<Vec3> = (0..=12)
        .map(|k| {
            let a = -PI * 0.5 + PI * k as f32 / 12.0;
            Vec3::new(dcos(a) * 51.0, FLOOR_Y, dsin(a) * 51.0)
        })
        .collect();
    vec![
        Route {
            name: "palace_walk",
            from: "courtyard",
            to: "north_garden",
            kind: RouteKind::Ground,
            waypoints: vec![
                Vec3::new(0.0, FLOOR_Y, -40.0),
                Vec3::new(0.0, FLOOR_Y, 5.0),
                Vec3::new(0.0, TERRACE_Y, 10.0),
                Vec3::new(0.0, TERRACE_Y, 21.0),
                Vec3::new(0.0, TERRACE_Y, 33.0),
                Vec3::new(0.0, FLOOR_Y, 40.0),
            ],
            anchors: vec![Vec3::new(DRUM.x, DOME_TOP - 1.0, DRUM.y)],
        },
        Route {
            name: "chamber_walk",
            from: "courtyard",
            to: "time_chamber",
            kind: RouteKind::Ground,
            waypoints: vec![
                Vec3::new(12.0, FLOOR_Y, CHAMBER_C.y),
                Vec3::new(CHAMBER[0] - 4.0, FLOOR_Y, CHAMBER_C.y),
                Vec3::new(CHAMBER[0] + 2.0, CHAMBER_FLOOR, CHAMBER_C.y),
                Vec3::new(CHAMBER_C.x, CHAMBER_FLOOR, CHAMBER_C.y),
                Vec3::new(CHAMBER_C.x, CHAMBER_FLOOR, CHAMBER[3] - 2.0),
                Vec3::new(CHAMBER_C.x, FLOOR_Y, CHAMBER[3] + 5.0),
            ],
            anchors: vec![],
        },
        Route {
            name: "wing_climb",
            from: "courtyard",
            to: "palace_roof",
            kind: RouteKind::Ground,
            waypoints: vec![
                Vec3::new(WING[2] + 1.0, FLOOR_Y, 4.0),
                Vec3::new(WING[2] + 1.0, TERRACE_Y, 11.0),
                Vec3::new(WING[2] + 1.0, WING_ROOF, 21.0),
                Vec3::new(19.0, WING_ROOF, 25.25),
                Vec3::new(17.0, WING_ROOF, 25.25),
                Vec3::new(10.0, HALL_ROOF, 25.25),
                Vec3::new(9.0, HALL_ROOF, 21.0),
            ],
            anchors: vec![],
        },
        Route {
            name: "promenade",
            from: "courtyard",
            to: "north_garden",
            kind: RouteKind::Ground,
            waypoints: promenade,
            anchors: vec![],
        },
        Route {
            name: "korin_stair",
            from: "korin",
            to: "courtyard",
            kind: RouteKind::Mixed,
            waypoints: std::iter::once(Vec3::new(KORIN.x - 1.0, KORIN_TOP, KORIN.y + 3.0))
                .chain(KORIN_STAIR.iter().map(|(at, top, _)| Vec3::new(at.x, *top, at.y)))
                .chain(std::iter::once(Vec3::new(0.0, FLOOR_Y, -DISC_R + 3.0)))
                .collect(),
            anchors: vec![],
        },
        Route {
            name: "dome_tether",
            from: "courtyard",
            to: "palace_roof",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(9.0, FLOOR_Y, -8.0), Vec3::new(9.0, HALL_ROOF, 18.0)],
            anchors: vec![Vec3::new(HALL[2] - 1.0, TURRET_TOP + 1.5, HALL[1])],
        },
    ]
}
