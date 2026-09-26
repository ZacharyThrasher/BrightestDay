//! **Giant's Room** — you are a 6 cm toy in a kid's bedroom scaled ×30 (DESIGN.md "Maps"; feature
//! `giant`). Everything is built in true proportion (1 real cm = 0.3 m; a 0.5 m voxel is 1.7 cm) so
//! the scale reads from anywhere: the floorboards are 4 m wide with 0.5 m gaps, the skirting board is
//! a 3 m wall, the bed is 15 m high, the door 60 m.
//!
//! Layout (metres; +X east, +Z north; floor at [`FLOOR_Y`], ceiling at [`CEIL_Y`]):
//!
//! ```text
//!  z=+52 ┌──── N wall: nightlight · light switch · DOOR (x 14..38) ─────────────────┐
//!        │ BED (headboard N)   NIGHT-        sneaker          TOY BOX (void pit)    │
//!        │ x -60..-31          STAND    ┌──────── track loop ────────────┐ bank   │
//!        │ pillows · duvet     x -31..-19│  CAT (in the sunbeam)   ramp ↑ │ (surf) │ WINDOW
//!        │ [void pit under]              │  RUG + tufts    BLOCK CASTLE    │        │ (E wall,
//!        │ blanket slope ↓ (surf)        │                                 │        │  side KO)
//!        │ BOOKSHELF      bank (surf) └──── kickers (W) ───────────────────┘ VENT   │ curtains
//!        │ x -60..-51            CHAIR        DESK (x 2..38) + launch ramp   SOCKS  │
//!  z=-52 └──── S wall: kid's drawing ─────────────────────────────────────────────┘
//!       x=-60                                                                x=+60
//! ```
//!
//! - **Islands** at every height, several ways between them: bed (duvet top ≈ 24 m; blanket slope
//!   from the floor, surf line down), nightstand (flush with the bed), desk (30.5 m; the toy-track
//!   launch ramp runs up to it), bookshelf (shelves every 9.5 m to 58 m; book stairs, wall-kicks,
//!   tethers), toy box rim (20 m; a block staircase), block castle (towers 6–10.5 m; a pencil bridge),
//!   sock mountain (≈ 15 m), the curled cat (≈ 14 m), window sill (35 m), static ceiling fan (66 m).
//!   High perches are exposed and close to the breakable ceiling: no safe dominant spot.
//! - **Toy track**: an orange loop round the rug (4 m wide incl. lips), two banked corners with
//!   surf lines (NE, SW), a kicker jump on the west straight and a launch ramp down from the desk.
//! - **KOs**: void pits (the open heating vent under the window, the dark gap under the bed, the toy
//!   box); the open window is a side blast zone (voxel world ends at the wall); the ceiling is soft
//!   plaster, so a hard upward launch punches through it into the sky → star KO (world top + margin).
//! - **Destruction**: toys, books, bedding, rug, curtains, floorboards and furniture tops break;
//!   walls, trim, furniture frames and the subfloor don't.
//! - **Perf**: the room shell is closed, so the world is flagged [`VoxelWorld::set_cull_outside`]
//!   (faces towards the air outside the world's extent are never meshed).

// Generator style: many small brushes with explicit metre coordinates.
#![allow(clippy::too_many_arguments)]

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, SurfLine};
use crate::city::City;
use crate::material::mat;
use crate::noise::{PI, Rng, TAU, dcos, dsin, hash3, unit, value2};
use crate::world::VoxelWorld;

#[path = "giant_icons.rs"]
pub mod icons;

/// World size in voxels (128 × 80 × 128 m, whole chunks): the room's walls fill out to the world's
/// extent (4 m east/west, 12 m north/south), so nothing outside them is ever seen or meshed.
pub const GIANT_SIZE_VOXELS: IVec3 = IVec3::new(256, 160, 256);
/// Real size × this = game size (a 1.8 m player is a 6 cm toy).
pub const SCALE: f32 = 30.0;
/// Floor surface (m).
pub const FLOOR_Y: f32 = 8.0;
/// Ceiling underside (m): 70 m clear = a 2.33 m real ceiling.
pub const CEIL_Y: f32 = 78.0;
/// Interior half-extents (m): 120 × 104 m = a 4 × 3.5 m real bedroom.
pub const HX: f32 = 60.0;
pub const HZ: f32 = 52.0;
/// Falling below this (into a pit) is the void KO.
pub const VOID_Y: f32 = 3.0;
/// Global cap on icon-library toys.
pub const MAX_TOYS: usize = 12;

/// An axis-aligned floor rectangle in metres `[x0, x1) × [z0, z1)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x0: f32,
    pub z0: f32,
    pub x1: f32,
    pub z1: f32,
}

impl Area {
    pub const fn new(x0: f32, z0: f32, x1: f32, z1: f32) -> Self {
        Self { x0, z0, x1, z1 }
    }
    pub fn contains(&self, x: f32, z: f32) -> bool {
        x >= self.x0 && x < self.x1 && z >= self.z0 && z < self.z1
    }
    pub fn center(&self) -> Vec2 {
        Vec2::new((self.x0 + self.x1) * 0.5, (self.z0 + self.z1) * 0.5)
    }
    pub fn grow(&self, d: f32) -> Area {
        Area::new(self.x0 - d, self.z0 - d, self.x1 + d, self.z1 + d)
    }
    pub fn overlaps(&self, o: &Area) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.z0 < o.z1 && o.z0 < self.z1
    }
}

pub const BED: Area = Area::new(-60.0, -8.0, -31.0, 52.0);
pub const MATTRESS_TOP: f32 = 23.0;
/// The dark gap under the bed has a void pit in its middle.
pub const UNDER_BED_PIT: Area = Area::new(-53.0, 2.0, -38.0, 40.0);
pub const NIGHTSTAND: Area = Area::new(-31.0, 40.0, -19.0, 52.0);
pub const NIGHTSTAND_TOP: f32 = 24.5;
/// The duvet dragged off the foot of the bed: a walkable slope down to the floor (+ a surf line).
pub const BLANKET: Area = Area::new(-50.0, -38.0, -36.0, -8.0);
pub const BOOKSHELF: Area = Area::new(-60.0, -50.0, -51.0, -26.0);
/// Shelf surfaces (m), bottom to top; the top of the case is [`BOOKSHELF_TOP`].
pub const SHELVES: [f32; 5] = [10.0, 19.5, 29.0, 38.5, 48.0];
pub const BOOKSHELF_TOP: f32 = 58.0;
pub const DESK: Area = Area::new(1.5, -52.0, 38.5, -33.5);
pub const DESK_TOP: f32 = 30.5;
pub const CHAIR: Area = Area::new(-12.0, -50.0, 0.0, -38.0);
pub const CHAIR_SEAT: f32 = 21.5;
pub const TOY_BOX: Area = Area::new(40.0, 38.0, 58.0, 50.0);
pub const TOY_BOX_RIM: f32 = 20.0;
pub const TOY_BOX_PIT: Area = Area::new(41.0, 39.0, 57.0, 49.0);
/// The heating vent under the window, its grille pulled off: a void pit.
pub const VENT: Area = Area::new(51.0, -4.5, 55.0, 4.5);
pub const RUG: Area = Area::new(-22.0, -18.0, 30.0, 22.0);
pub const CASTLE: Area = Area::new(5.0, -11.5, 23.0, 3.5);
pub const SOCKS: Area = Area::new(43.0, -51.0, 59.0, -35.0);
/// The open window in the east wall (x = [`HX`]): z and y ranges.
pub const WINDOW_Z: (f32, f32) = (-18.0, 18.0);
pub const WINDOW_Y: (f32, f32) = (35.0, 65.0);
pub const SILL_Y: f32 = 35.0;
/// Curtain rod (y) and the two curtain panels (z ranges).
pub const ROD_Y: f32 = 68.0;
pub const CURTAINS: [(f32, f32); 2] = [(-31.0, -18.0), (18.0, 31.0)];
/// Ceiling fan: hub centre (blades at y ≈ 66 m) and blade tip radius.
pub const FAN: Vec3 = Vec3::new(6.0, 66.0, 2.0);
pub const FAN_R: f32 = 20.0;
pub const DOOR: Area = Area::new(14.0, 51.5, 38.0, 52.0);
/// Toy-track loop centre line: straights at x = W / E and z = S / N, corner radius R.
pub const TRACK_W: f32 = -27.0;
pub const TRACK_E: f32 = 41.0;
pub const TRACK_S: f32 = -25.0;
pub const TRACK_N: f32 = 29.0;
pub const TRACK_R: f32 = 9.0;
/// Lane half-width (m) and lip width.
pub const LANE: f32 = 1.5;
pub const LIP: f32 = 0.5;
/// The launch ramp from the desk runs north along x = RAMP_X.
pub const RAMP_X: f32 = 32.0;

/// Unit vector towards the afternoon sun (in through the east window, a little from the south).
pub fn sun_dir() -> Vec3 {
    Vec3::new(0.80, 0.52, -0.14).normalize()
}

/// Where the sunbeam through the window's centre lands on the rug (the cat sleeps there).
pub fn sun_patch() -> Vec2 {
    let s = sun_dir();
    let c = Vec3::new(HX, (WINDOW_Y.0 + WINDOW_Y.1) * 0.5, (WINDOW_Z.0 + WINDOW_Z.1) * 0.5);
    let t = (c.y - (FLOOR_Y + 0.5)) / s.y;
    let p = c - s * t;
    Vec2::new(p.x, p.z)
}

/// The curled cat's centre on the floor (in the sun patch, clear of the castle).
pub fn cat_center() -> Vec2 {
    let p = sun_patch();
    Vec2::new(p.x.min(CASTLE.x0 - 9.5), p.y.clamp(RUG.z0 + 6.0, RUG.z1 - 3.0))
}

#[inline]
fn vx(m: f32) -> i32 {
    (m / 0.5).floor() as i32
}

#[inline]
fn vcen(v: IVec3) -> Vec3 {
    (v.as_vec3() + Vec3::splat(0.5)) * 0.5
}

/// Fill the metre box `[x0, x1) × [y0, y1) × [z0, z1)` (voxel centres inside).
fn bx(c: &mut City, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, m: Material) {
    c.fill(Vec3::new(x0, y0, z0), Vec3::new(x1, y1, z1), m);
}

/// Vertical cylinder (or tube with `inner` > 0) centred on (cx, cz).
fn cyl(c: &mut City, cx: f32, cz: f32, r: f32, inner: f32, y0: f32, y1: f32, m: Material) {
    c.fill_with(Vec3::new(cx - r - 0.5, y0, cz - r - 0.5), Vec3::new(cx + r + 0.5, y1, cz + r + 0.5), |p, _, _| {
        let d = Vec2::new(p.x - cx, p.z - cz).length();
        (d <= r && d >= inner).then_some(m)
    });
}

fn set(c: &mut City, v: IVec3, m: Material) {
    c.w.set_raw(v, m);
}

fn get(c: &City, v: IVec3) -> Material {
    c.w.get(v)
}

// ------------------------------------------------------------------------------------------------
// Shell: subfloor + floorboards, walls, skirting, ceiling, window, door, wall decor

fn build_shell(c: &mut City, seed: u32) {
    let (wx, wz) = (GIANT_SIZE_VOXELS.x as f32 * 0.25, GIANT_SIZE_VOXELS.z as f32 * 0.25);
    // Indestructible subfloor to the world bottom (its underside is never meshed), boards on top.
    bx(c, -wx, 0.0, -wz, wx, FLOOR_Y - 0.5, wz, mat::SUBFLOOR);
    let (x0, x1) = (vx(-HX), vx(HX));
    let (z0, z1) = (vx(-HZ), vx(HZ));
    let yb = vx(FLOOR_Y - 0.5);
    for z in z0..z1 {
        // Boards run east–west, 4 m (8 voxels, 13 cm real) wide; the 8th row is the gap.
        let row = (z - z0).rem_euclid(8);
        let board = (z - z0).div_euclid(8);
        if row == 7 {
            continue;
        }
        // Butt joints every 36 m (1.2 m real), staggered per board.
        let off = (hash3(board, 0, 0, seed ^ 0x0b0a_4d00) % 72) as i32;
        for x in x0..x1 {
            if (x - x0 + off).rem_euclid(72) == 0 {
                continue;
            }
            let (mx, mz) = (x as f32 * 0.5, z as f32 * 0.5);
            // Long sinuous grain streaks along the board + the odd knot.
            let g = value2(mx * 0.035, mz * 0.9 + board as f32 * 13.1, seed ^ 0x6a41) + 0.35 * value2(mx * 0.2, mz * 2.0, seed ^ 0x77);
            let knot = unit(hash3(x / 18, board, 7, seed)) < 0.05 && ((x % 18) - 9).abs() < 2 && row > 1 && row < 6;
            let m = if g > 0.42 || knot { mat::FLOORBOARD_GRAIN } else { mat::FLOORBOARD };
            set(c, IVec3::new(x, yb, z), m);
        }
    }
    // Walls (indestructible paint) to the world's extent, and the ceiling ring they carry.
    bx(c, -wx, 0.0, -wz, -HX, CEIL_Y + 2.0, wz, mat::WALL_PAINT);
    bx(c, HX, 0.0, -wz, wx, CEIL_Y + 2.0, wz, mat::WALL_PAINT);
    bx(c, -HX, 0.0, -wz, HX, CEIL_Y + 2.0, -HZ, mat::WALL_PAINT);
    bx(c, -HX, 0.0, HZ, HX, CEIL_Y + 2.0, wz, mat::WALL_PAINT);
    // Soft plaster ceiling (1 m); a crown moulding along every wall (indestructible anchor ledge).
    bx(c, -HX, CEIL_Y, -HZ, HX, CEIL_Y + 2.0, HZ, mat::CEILING_PLASTER);
    for (a, b, d, e) in [(-HX, -HZ, -HX + 0.5, HZ), (HX - 0.5, -HZ, HX, HZ), (-HX, -HZ, HX, -HZ + 0.5), (-HX, HZ - 0.5, HX, HZ)] {
        bx(c, a, CEIL_Y - 1.0, b, d, CEIL_Y, e, mat::TRIM);
    }
    // Skirting boards: 3 m tall (10 cm), 0.5 m proud of the wall.
    for (a, b, d, e) in [(-HX, -HZ, -HX + 0.5, HZ), (HX - 0.5, -HZ, HX, HZ), (-HX, -HZ, HX, -HZ + 0.5), (-HX, HZ - 0.5, HX, HZ)] {
        bx(c, a, FLOOR_Y, b, d, FLOOR_Y + 3.0, e, mat::TRIM);
    }
    // Collapse hints: the wall tops (the ceiling heads there) and the subfloor.
    let mut k = -HX as i32;
    while k <= HX as i32 {
        for z in [-HZ - 1.0, HZ + 0.5] {
            c.anchor(IVec3::new(vx(k as f32), vx(CEIL_Y + 0.5), vx(z)));
            c.anchor(IVec3::new(vx(k as f32), vx(FLOOR_Y - 1.0), vx(z * 0.5)));
        }
        k += 8;
    }
    let mut k = -HZ as i32;
    while k <= HZ as i32 {
        for x in [-HX - 1.0, HX + 0.5] {
            c.anchor(IVec3::new(vx(x), vx(CEIL_Y + 0.5), vx(k as f32)));
        }
        k += 8;
    }

    // ---- the open window (east wall) ----
    let (wz0, wz1) = WINDOW_Z;
    let (wy0, wy1) = WINDOW_Y;
    bx(c, HX, wy0, wz0, wx, wy1, wz1, AIR);
    // Frame: trim reveal around the opening, the sill and apron, and a middle sash rail.
    c.fill_with(Vec3::new(HX, wy0 - 0.5, wz0 - 0.5), Vec3::new(wx, wy1 + 0.5, wz1 + 0.5), |_, _, old| (old != AIR).then_some(mat::TRIM));
    bx(c, HX - 5.0, SILL_Y - 1.0, wz0 - 2.0, HX, SILL_Y, wz1 + 2.0, mat::TRIM);
    bx(c, HX - 0.5, SILL_Y - 4.0, wz0 - 1.0, HX, SILL_Y - 1.0, wz1 + 1.0, mat::TRIM);
    bx(c, HX + 1.0, 50.0, wz0, HX + 2.0, 51.0, wz1, mat::TRIM);
    bx(c, HX + 1.0, wy0, -0.25, HX + 2.0, wy1, 0.25, mat::TRIM);

    // ---- the door (north wall, closed; a dark gap under it) ----
    let (dx0, dx1, dtop) = (DOOR.x0, DOOR.x1, FLOOR_Y + 60.0);
    bx(c, dx0, FLOOR_Y, HZ, dx1, dtop, HZ + 1.0, mat::FURNITURE_FRAME);
    // Panel grooves (4 panels).
    for (a, b, y0, y1) in [(dx0 + 3.0, dx0 + 10.5, 14.0, 34.0), (dx1 - 10.5, dx1 - 3.0, 14.0, 34.0), (dx0 + 3.0, dx0 + 10.5, 38.0, 64.0), (dx1 - 10.5, dx1 - 3.0, 38.0, 64.0)] {
        for (p0, p1) in [((a, y0), (b, y0 + 0.5)), ((a, y1 - 0.5), (b, y1)), ((a, y0), (a + 0.5, y1)), ((b - 0.5, y0), (b, y1))] {
            bx(c, p0.0, p0.1, HZ, p1.0, p1.1, HZ + 0.5, AIR);
        }
    }
    bx(c, dx0, FLOOR_Y, HZ, dx1, FLOOR_Y + 0.5, HZ + 2.0, AIR);
    bx(c, dx0, FLOOR_Y - 0.5, HZ + 2.0, dx1, FLOOR_Y + 1.0, HZ + 2.5, mat::VOID_BLACK);
    // Architrave (casing) round the door, proud of the wall.
    bx(c, dx0 - 1.5, FLOOR_Y, HZ - 0.5, dx0, dtop + 1.5, HZ, mat::TRIM);
    bx(c, dx1, FLOOR_Y, HZ - 0.5, dx1 + 1.5, dtop + 1.5, HZ, mat::TRIM);
    bx(c, dx0 - 1.5, dtop, HZ - 0.5, dx1 + 1.5, dtop + 1.5, HZ, mat::TRIM);
    // Door knob + rose.
    c.w.fill_sphere(Vec3::new(dx1 - 3.0, FLOOR_Y + 27.0, HZ - 1.2), 1.1, mat::BRASS, true);
    cyl_z(c, dx1 - 3.0, FLOOR_Y + 27.0, 1.4, HZ - 0.5, HZ, mat::BRASS);
    // Light switch plate by the door (1.2 m real → 36 m up).
    bx(c, dx1 + 4.0, 42.0, HZ - 0.5, dx1 + 6.5, 46.0, HZ, mat::TRIM);
    bx(c, dx1 + 5.0, 44.0, HZ - 1.0, dx1 + 5.5, 45.0, HZ - 0.5, mat::PAINT_WHITE);

    // ---- wall socket + moon nightlight (north wall, low) ----
    bx(c, -8.0, 10.5, HZ - 0.5, -5.0, 15.5, HZ, mat::TRIM);
    c.fill_with(Vec3::new(-9.0, 10.5, HZ - 1.5), Vec3::new(-4.0, 16.0, HZ - 0.5), |p, _, _| {
        let (dx, dy) = (p.x + 6.5, p.y - 13.0);
        let moon = dx * dx + dy * dy <= 2.4 && (dx - 0.8) * (dx - 0.8) + (dy - 0.4) * (dy - 0.4) > 1.2;
        moon.then_some(mat::NEON_AMBER)
    });

    // ---- wall clock above the bed (west wall), stopped at ten past four ----
    let (cz, cy, r) = (20.0, 56.0, 4.5);
    c.fill_with(Vec3::new(-HX, cy - r - 0.5, cz - r - 0.5), Vec3::new(-HX + 1.0, cy + r + 0.5, cz + r + 0.5), |p, _, _| {
        let d = Vec2::new(p.y - cy, p.z - cz).length();
        if d > r {
            None
        } else if d > r - 0.6 {
            Some(mat::PLASTIC_RED)
        } else {
            Some(mat::PILLOW)
        }
    });
    for h in 0..12 {
        let a = h as f32 * TAU / 12.0;
        // Seen from the room (looking −x) clockwise runs from +y towards −z.
        let (y, z) = (cy + dcos(a) * 3.5, cz - dsin(a) * 3.5);
        set(c, IVec3::new(vx(-HX + 1.25), vx(y), vx(z)), mat::PLASTIC_BLACK);
    }
    for (frac, len) in [(4.0 / 12.0 + 10.0 / 720.0, 2.2), (10.0 / 60.0, 3.3)] {
        let a = frac * TAU;
        let tip = Vec3::new(-HX + 1.25, cy + dcos(a) * len, cz - dsin(a) * len);
        c.line(Vec3::new(-HX + 1.25, cy, cz), tip, mat::PLASTIC_BLACK, false);
    }
    set(c, IVec3::new(vx(-HX + 1.25), vx(cy), vx(cz)), mat::BRASS);

    // ---- a kid's crayon drawing taped above the desk (south wall) ----
    let (ax0, ay0) = (12.0, 40.0);
    bx(c, ax0, ay0, -HZ, ax0 + 9.0, ay0 + 6.5, -HZ + 0.5, mat::BOOK_PAGES);
    c.fill_with(Vec3::new(ax0, ay0, -HZ), Vec3::new(ax0 + 9.0, ay0 + 6.5, -HZ + 0.5), |p, _, _| {
        let (u, v) = (p.x - ax0, p.y - ay0);
        let sun = (u - 7.2) * (u - 7.2) + (v - 5.0) * (v - 5.0) <= 1.1;
        let grass = v < 1.1;
        let house = (2.0..5.5).contains(&u) && (1.0..3.6).contains(&v);
        let roof = v >= 3.6 && v < 5.4 && (u - 3.75).abs() < 2.2 - (v - 3.6) * 1.1;
        if sun {
            Some(mat::PLASTIC_YELLOW)
        } else if roof {
            Some(mat::PLASTIC_RED)
        } else if house {
            Some(if (3.2..4.2).contains(&u) && v < 2.6 { mat::DARK_WOOD } else { mat::HAZARD_ORANGE })
        } else if grass {
            Some(mat::PLASTIC_GREEN)
        } else {
            None
        }
    });
    for (x, y) in [(ax0, ay0 + 6.0), (ax0 + 8.5, ay0 + 6.0)] {
        bx(c, x, y, -HZ + 0.5, x + 0.5, y + 0.5, -HZ + 1.0, mat::PAINT_WHITE);
    }
}

/// Disc facing ±z (door rose, alarm clock face): centre (x, y), radius r, z range.
fn cyl_z(c: &mut City, x: f32, y: f32, r: f32, z0: f32, z1: f32, m: Material) {
    c.fill_with(Vec3::new(x - r - 0.5, y - r - 0.5, z0), Vec3::new(x + r + 0.5, y + r + 0.5, z1), |p, _, _| {
        (Vec2::new(p.x - x, p.y - y).length() <= r).then_some(m)
    });
}

// ------------------------------------------------------------------------------------------------
// Bed, nightstand, blanket slope

fn build_bed(c: &mut City, seed: u32) {
    let b = BED;
    let base = FLOOR_Y + 5.0; // 17 cm under-bed gap
    // Legs, rails, slat board (frame: indestructible).
    for (x, z) in [(b.x0, b.z0), (b.x1 - 2.0, b.z0), (b.x0, b.z1 - 2.0), (b.x1 - 2.0, b.z1 - 2.0), (b.x1 - 2.0, 21.0), (b.x0, 21.0)] {
        bx(c, x, FLOOR_Y, z, x + 2.0, base + 3.0, z + 2.0, mat::FURNITURE_FRAME);
    }
    bx(c, b.x0, base, b.z0, b.x1, base + 1.0, b.z1, mat::FURNITURE_FRAME);
    for (x0, z0, x1, z1) in [(b.x0, b.z0, b.x1, b.z0 + 1.0), (b.x1 - 1.0, b.z0, b.x1, b.z1), (b.x0, b.z1 - 1.0, b.x1, b.z1)] {
        bx(c, x0, base, z0, x1, base + 3.0, z1, mat::FURNITURE_FRAME);
    }
    for k in 0..5 {
        c.anchor(IVec3::new(vx(b.x1 - 1.0), vx(base), vx(b.z0 + 12.0 * k as f32)));
    }
    // Mattress with rounded vertical edges.
    let (m0, m1) = (base + 1.0, MATTRESS_TOP);
    c.fill_with(Vec3::new(b.x0, m0, b.z0 + 0.5), Vec3::new(b.x1 - 0.5, m1, b.z1 - 2.0), |p, _, _| {
        let dx = (b.x0 + 2.0 - p.x).max(p.x - (b.x1 - 2.5)).max(0.0);
        let dz = (b.z0 + 2.5 - p.z).max(p.z - (b.z1 - 4.0)).max(0.0);
        let dy = (p.y - (m1 - 1.5)).max(0.0);
        let r2 = dx * dx + dz * dz + dy * dy;
        if r2 > 4.0 {
            return None;
        }
        // A piping line round the top edge.
        Some(if (p.y - (m1 - 1.25)).abs() < 0.3 && (dx > 1.2 || dz > 1.2 || p.x > b.x1 - 1.5) { mat::DUVET } else { mat::MATTRESS })
    });
    // Headboard: rounded top corners, destructible top rail.
    let (hz0, hz1, htop) = (b.z1 - 2.0, b.z1, FLOOR_Y + 30.0);
    c.fill_with(Vec3::new(b.x0, base, hz0), Vec3::new(b.x1, htop, hz1), |p, _, _| {
        let rx = (b.x0 + 4.0 - p.x).max(p.x - (b.x1 - 4.0)).max(0.0);
        let ry = (p.y - (htop - 4.0)).max(0.0);
        if rx * rx + ry * ry > 16.0 {
            return None;
        }
        let top = rx * rx + (ry + 0.5) * (ry + 0.5) > 16.0 || p.y > htop - 0.5;
        Some(if top { mat::FURNITURE_TOP } else { mat::FURNITURE_FRAME })
    });

    // Duvet: covers the lower two thirds, folds, star print; drapes over the east side.
    let dz1 = 32.0;
    let mut rng = Rng::new(seed as u64 ^ 0xd07e);
    let stars: Vec<Vec2> = (0..22).map(|_| Vec2::new(rng.range(b.x0 + 2.0, b.x1 - 2.0), rng.range(b.z0 + 1.0, dz1 - 2.0))).collect();
    let star = |x: f32, z: f32| {
        stars.iter().any(|s| {
            let d = Vec2::new(x - s.x, z - s.y);
            let r = d.length();
            if r > 1.3 {
                return false;
            }
            let a = crate::noise::datan2(d.y, d.x);
            // Five points: radius swings between 0.55 and 1.3.
            let k = (dcos(a * 5.0) + 1.0) * 0.5;
            r <= 0.55 + 0.75 * k
        })
    };
    let (vx0, vx1) = (vx(b.x0), vx(b.x1 + 1.0));
    for z in vx(b.z0 - 0.5)..vx(dz1) {
        for x in vx0..vx1 {
            let (mx, mz) = (x as f32 * 0.5 + 0.25, z as f32 * 0.5 + 0.25);
            let fold = 0.6 + 0.9 * (value2(mx * 0.12, mz * 0.05, seed ^ 0xf01d) * 0.5 + 0.5);
            let edge_z = dz1 - mz;
            let thick = if edge_z < 2.0 { (fold * edge_z / 2.0).max(0.5) } else { fold };
            let top = MATTRESS_TOP + thick;
            if mx >= b.x1 - 0.5 {
                // Drape over the east edge down to 6 m above the floor, a little wavy.
                let hang = MATTRESS_TOP - 6.0 - 1.5 * (value2(mz * 0.3, 1.0, seed) * 0.5 + 0.5);
                for y in vx(hang)..vx(top) {
                    set(c, IVec3::new(x, y, z), mat::DUVET);
                }
                continue;
            }
            for y in vx(MATTRESS_TOP)..vx(top).max(vx(MATTRESS_TOP) + 1) {
                set(c, IVec3::new(x, y, z), mat::DUVET);
            }
            if star(mx, mz) {
                let ty = vx(top).max(vx(MATTRESS_TOP) + 1) - 1;
                set(c, IVec3::new(x, ty, z), mat::DUVET_STAR);
            }
        }
    }
    // Pillow mountain: two plump pillows at the head, one leaning on the other.
    c.w.fill_ellipsoid(Vec3::new(-46.0, MATTRESS_TOP + 1.5, 42.0), Vec3::new(11.0, 3.6, 6.8), mat::PILLOW, true);
    c.w.fill_ellipsoid(Vec3::new(-41.0, MATTRESS_TOP + 4.5, 37.0), Vec3::new(9.0, 3.0, 5.8), mat::PILLOW, true);
    // Under-bed: a void pit lined in black ("the dark gap under the bed").
    carve_pit(c, UNDER_BED_PIT);
    // A lost sock and a dust bunny down there (clear of the pit).
    sock(c, Vec3::new(-57.0, FLOOR_Y, 44.0), Vec2::new(0.2, -1.0), mat::BOOK_RED, seed ^ 5);
    c.w.fill_ellipsoid(Vec3::new(-35.5, FLOOR_Y + 0.6, 8.0), Vec3::new(1.4, 1.0, 1.8), mat::SOCK_WHITE, true);

    // Nightstand (flush with the bed), drawer + knob, short legs.
    let n = NIGHTSTAND;
    bx(c, n.x0, FLOOR_Y + 1.5, n.z0, n.x1, NIGHTSTAND_TOP - 0.5, n.z1, mat::FURNITURE_FRAME);
    bx(c, n.x0 - 0.5, NIGHTSTAND_TOP - 0.5, n.z0 - 0.5, n.x1 + 0.5, NIGHTSTAND_TOP, n.z1, mat::FURNITURE_TOP);
    for (x, z) in [(n.x0, n.z0), (n.x1 - 1.5, n.z0), (n.x0, n.z1 - 1.5), (n.x1 - 1.5, n.z1 - 1.5)] {
        bx(c, x, FLOOR_Y, z, x + 1.5, FLOOR_Y + 1.5, z + 1.5, mat::FURNITURE_FRAME);
    }
    for (a, b2) in [((n.x0 + 1.0, 14.0), (n.x1 - 1.0, 14.5)), ((n.x0 + 1.0, 21.5), (n.x1 - 1.0, 22.0))] {
        bx(c, a.0, a.1, n.z0 - 0.5, b2.0, b2.1, n.z0, mat::FURNITURE_TOP);
    }
    bx(c, (n.x0 + n.x1) * 0.5 - 0.5, 18.0, n.z0 - 1.0, (n.x0 + n.x1) * 0.5 + 0.5, 19.0, n.z0, mat::BRASS);
    c.anchor(IVec3::new(vx(n.x0 + 3.0), vx(FLOOR_Y + 5.0), vx(n.z0 + 3.0)));
    // Alarm clock (10 cm → 3 m) and a cocoa mug on top.
    let (ax, az, ay) = (-27.0, 47.0, NIGHTSTAND_TOP);
    cyl_z(c, ax, ay + 1.9, 1.6, az - 0.75, az + 0.75, mat::PLASTIC_RED);
    cyl_z(c, ax, ay + 1.9, 1.15, az - 1.25, az - 0.75, mat::PILLOW);
    set(c, IVec3::new(vx(ax), vx(ay + 2.5), vx(az - 1.6)), mat::PLASTIC_BLACK);
    set(c, IVec3::new(vx(ax + 0.5), vx(ay + 2.0), vx(az - 1.6)), mat::PLASTIC_BLACK);
    for dx in [-1.1f32, 1.1] {
        c.w.fill_sphere(Vec3::new(ax + dx, ay + 3.6, az), 0.55, mat::BRASS, true);
        bx(c, ax + dx * 0.9 - 0.25, ay, az - 0.25, ax + dx * 0.9 + 0.25, ay + 0.5, az + 0.25, mat::PLASTIC_BLACK);
    }
    mug(c, Vec2::new(-21.8, 49.0), NIGHTSTAND_TOP, mat::STUCCO_MINT);
}

/// The duvet dragged off the foot of the bed: a solid, folded slope to the floor.
fn build_blanket(c: &mut City, seed: u32) {
    let a = BLANKET;
    let (top_z, top_y, low_z, low_y) = (a.z1, MATTRESS_TOP + 1.0, a.z0, FLOOR_Y + 0.5);
    for z in vx(a.z0)..vx(a.z1 + 0.5) {
        for x in vx(a.x0)..vx(a.x1) {
            let (mx, mz) = (x as f32 * 0.5 + 0.25, z as f32 * 0.5 + 0.25);
            let t = ((mz - low_z) / (top_z - low_z)).clamp(0.0, 1.0);
            // Ragged side edges near the floor.
            let side = (mx - a.x0).min(a.x1 - mx);
            if side < 1.5 * (1.0 - t) * (value2(mz * 0.4, mx, seed) * 0.5 + 0.5) {
                continue;
            }
            let fold = 0.7 * value2(mx * 0.18, mz * 0.07, seed ^ 0xb1a0);
            let h = low_y + (top_y - low_y) * t + fold * t.min(0.8);
            let yt = vx(h).max(vx(FLOOR_Y) + 1);
            for y in vx(FLOOR_Y)..yt {
                set(c, IVec3::new(x, y, z), mat::DUVET);
            }
            if unit(hash3(x / 5, 0, z / 5, seed ^ 0x57a2)) < 0.18 && (x % 5 == 2) && (z % 5 == 2) {
                set(c, IVec3::new(x, yt - 1, z), mat::DUVET_STAR);
            }
        }
    }
}

/// Carve a void pit through the floor, its shaft lined in black.
fn carve_pit(c: &mut City, a: Area) {
    let (x0, x1, z0, z1) = (vx(a.x0), vx(a.x1), vx(a.z0), vx(a.z1));
    for z in z0 - 1..=z1 {
        for x in x0 - 1..=x1 {
            let inside = x >= x0 && x < x1 && z >= z0 && z < z1;
            for y in 0..vx(FLOOR_Y) {
                let v = IVec3::new(x, y, z);
                if inside {
                    set(c, v, AIR);
                } else if get(c, v) != AIR {
                    set(c, v, mat::VOID_BLACK);
                }
            }
        }
    }
}

/// A cocoa mug: tube + handle + cocoa surface.
fn mug(c: &mut City, at: Vec2, y: f32, m: Material) {
    cyl(c, at.x, at.y, 1.45, 0.0, y, y + 0.5, m);
    cyl(c, at.x, at.y, 1.45, 0.9, y + 0.5, y + 3.0, m);
    cyl(c, at.x, at.y, 0.95, 0.0, y + 0.5, y + 2.0, mat::DARK_WOOD);
    for (dy, dx) in [(0.5, 1.75), (0.5, 2.25), (1.0, 2.25), (1.5, 2.25), (2.0, 2.25), (2.5, 2.25), (2.5, 1.75)] {
        set(c, IVec3::new(vx(at.x + dx), vx(y + dy), vx(at.y)), m);
    }
}

// ------------------------------------------------------------------------------------------------
// Bookshelf

fn build_bookshelf(c: &mut City, seed: u32) {
    let b = BOOKSHELF;
    // Sides, plinth, shelves (frame below, breakable top), top board (overhanging).
    bx(c, b.x0, FLOOR_Y, b.z0, b.x1, BOOKSHELF_TOP - 2.0, b.z0 + 1.0, mat::FURNITURE_FRAME);
    bx(c, b.x0, FLOOR_Y, b.z1 - 1.0, b.x1, BOOKSHELF_TOP - 2.0, b.z1, mat::FURNITURE_FRAME);
    bx(c, b.x0, FLOOR_Y, b.z0, b.x1 - 0.5, SHELVES[0] - 1.0, b.z1, mat::FURNITURE_FRAME);
    for &s in &SHELVES {
        bx(c, b.x0, s - 1.0, b.z0, b.x1, s - 0.5, b.z1, mat::FURNITURE_FRAME);
        bx(c, b.x0, s - 0.5, b.z0 + 1.0, b.x1, s, b.z1 - 1.0, mat::FURNITURE_TOP);
    }
    bx(c, b.x0, BOOKSHELF_TOP - 2.0, b.z0 - 0.5, b.x1 + 0.5, BOOKSHELF_TOP - 0.5, b.z1 + 0.5, mat::FURNITURE_FRAME);
    bx(c, b.x0, BOOKSHELF_TOP - 0.5, b.z0 - 0.5, b.x1 + 0.5, BOOKSHELF_TOP, b.z1 + 0.5, mat::FURNITURE_TOP);
    for &s in &SHELVES {
        c.anchor(IVec3::new(vx(b.x0 + 3.0), vx(s - 1.0), vx(b.z0 + 0.5)));
    }
    let covers = [mat::BOOK_RED, mat::BOOK_BLUE, mat::BOOK_GREEN, mat::BOOK_PURPLE, mat::PLASTIC_YELLOW, mat::DARK_WOOD];
    let mut rng = Rng::new(seed as u64 ^ 0xb00c);
    for (k, &s) in SHELVES.iter().enumerate() {
        let clear = if k + 1 < SHELVES.len() { SHELVES[k + 1] - 1.0 - s } else { BOOKSHELF_TOP - 2.0 - s };
        let mut z = b.z0 + 1.0;
        // Book staircase (flat stacks rising 1.5 m a step) at alternating ends of the lower shelves.
        let stair_left = k % 2 == 0;
        let stair_len = if k < 4 { 4 } else { 0 };
        let tread = 2.5;
        let (stair_z0, stair_z1) = if stair_left { (b.z0 + 1.0, b.z0 + 1.0 + tread * stair_len as f32) } else { (b.z1 - 1.0 - tread * stair_len as f32, b.z1 - 1.0) };
        for step in 0..stair_len {
            let (za, zb) = if stair_left {
                (stair_z0 + tread * step as f32, stair_z0 + tread * (step + 1) as f32)
            } else {
                (stair_z1 - tread * (step + 1) as f32, stair_z1 - tread * step as f32)
            };
            let h = 1.5 * (step + 1) as f32;
            let mut y = s;
            let mut i = 0;
            while y < s + h - 0.01 {
                let th = if h - (y - s) >= 1.0 && rng.chance(0.5) { 1.0 } else { 0.5 };
                let cov = covers[rng.int(0, covers.len() as i32) as usize];
                let inset = rng.range(0.0, 0.8);
                let d = rng.range(5.5, 6.8);
                flat_book(c, b.x0, b.x0 + d, za + inset * 0.5, zb - inset * 0.5, y, y + th, cov);
                y += th;
                i += 1;
            }
            let _ = i;
        }
        // Standing books in the rest of the shelf (occasional gaps, one empty stretch per shelf).
        let gap_at = rng.range(b.z0 + 6.0, b.z1 - 8.0);
        while z < b.z1 - 1.0 {
            if z >= stair_z0 - 0.01 && z < stair_z1 - 0.01 {
                z = stair_z1;
                continue;
            }
            if (z - gap_at).abs() < 2.0 && k > 0 {
                z += 0.5;
                continue;
            }
            let w: f32 = [0.5, 1.0, 1.0, 1.5][rng.int(0, 4) as usize];
            let w = w.min(b.z1 - 1.0 - z).min(if z < stair_z0 { stair_z0 - z } else { f32::INFINITY });
            if w < 0.5 {
                z += 0.5;
                continue;
            }
            let h = rng.range(5.0, (clear - 1.2).min(7.5)).max(4.0);
            let d = rng.range(5.0, 6.8);
            let cov = covers[rng.int(0, covers.len() as i32) as usize];
            standing_book(c, b.x0, b.x0 + d, z, z + w, s, s + h, cov);
            z += w;
            if rng.chance(0.15) {
                z += 0.5;
            }
        }
    }
    // A fallen book on the floor: the first step up to the bottom shelf.
    flat_book(c, b.x1, b.x1 + 6.5, -41.0, -35.0, FLOOR_Y, FLOOR_Y + 1.0, mat::BOOK_BLUE);
}

/// A book lying flat: pages on three sides, spine facing +x at `x1`.
fn flat_book(c: &mut City, x0: f32, x1: f32, z0: f32, z1: f32, y0: f32, y1: f32, cover: Material) {
    c.fill_with(Vec3::new(x0, y0, z0), Vec3::new(x1, y1, z1), |p, _, _| {
        let spine = p.x > x1 - 0.5;
        let face = p.y < y0 + 0.5 || p.y > y1 - 0.5;
        Some(if spine || face { cover } else { mat::BOOK_PAGES })
    });
}

/// A book standing on a shelf, spine facing +x at `x1`; the top shows page edges between covers.
fn standing_book(c: &mut City, x0: f32, x1: f32, z0: f32, z1: f32, y0: f32, y1: f32, cover: Material) {
    c.fill_with(Vec3::new(x0, y0, z0), Vec3::new(x1, y1, z1), |p, _, _| {
        let spine = p.x > x1 - 0.5;
        let side = p.z < z0 + 0.5 || p.z > z1 - 0.5;
        let top = p.y > y1 - 0.5;
        Some(if spine || side || !top { cover } else { mat::BOOK_PAGES })
    });
    // A band on the spine.
    let band = y1 - 1.5;
    bx(c, x1 - 0.5, band, z0, x1, band + 0.5, z1, mat::BOOK_PAGES);
}

// ------------------------------------------------------------------------------------------------
// Desk, chair, desk things

fn build_desk(c: &mut City, seed: u32) {
    let d = DESK;
    let under = DESK_TOP - 1.0;
    for (x, z) in [(d.x0 + 0.5, d.z0), (d.x1 - 2.0, d.z0), (d.x0 + 0.5, d.z1 - 2.0), (d.x1 - 2.0, d.z1 - 2.0)] {
        bx(c, x, FLOOR_Y, z, x + 1.5, under, z + 1.5, mat::FURNITURE_FRAME);
        c.anchor(IVec3::new(vx(x + 0.5), vx(FLOOR_Y + 2.0), vx(z + 0.5)));
    }
    // Apron (a skirt under the top) on the back and sides.
    bx(c, d.x0 + 0.5, under - 1.5, d.z0, d.x1 - 0.5, under, d.z0 + 0.5, mat::FURNITURE_FRAME);
    bx(c, d.x0 + 0.5, under - 1.5, d.z0, d.x0 + 1.0, under, d.z1 - 0.5, mat::FURNITURE_FRAME);
    bx(c, d.x1 - 1.0, under - 1.5, d.z0, d.x1 - 0.5, under, d.z1 - 0.5, mat::FURNITURE_FRAME);
    bx(c, d.x0, under, d.z0, d.x1, DESK_TOP - 0.5, d.z1, mat::FURNITURE_FRAME);
    bx(c, d.x0, DESK_TOP - 0.5, d.z0, d.x1, DESK_TOP, d.z1, mat::FURNITURE_TOP);
    let y = DESK_TOP;
    // Desk lamp (back right): base, two arm segments, shade, glowing bulb.
    cyl(c, 34.5, -48.5, 2.5, 0.0, y, y + 1.0, mat::PLASTIC_YELLOW);
    let (j0, j1, j2) = (Vec3::new(34.5, y + 1.0, -48.5), Vec3::new(34.5, y + 11.0, -47.0), Vec3::new(28.5, y + 14.0, -44.5));
    c.line(j0, j1, mat::STEEL, false);
    c.line(j1, j2, mat::STEEL, false);
    c.line(j0 + Vec3::X * 0.5, j1 + Vec3::X * 0.5, mat::STEEL, false);
    c.w.fill_sphere(j1, 0.7, mat::BRASS, false);
    let shade = Vec3::new(26.5, y + 12.5, -43.5);
    c.fill_with(shade - Vec3::new(3.5, 3.0, 3.5), shade + Vec3::new(3.5, 2.5, 3.5), |p, _, _| {
        let h = p.y - (shade.y - 2.5); // 0 at the rim … 4.5 at the top
        if !(0.0..=4.5).contains(&h) {
            return None;
        }
        let r = 3.1 - h * 0.45;
        let d = Vec2::new(p.x - shade.x, p.z - shade.z).length();
        (d <= r && (d > r - 0.6 || h > 4.0)).then_some(mat::PLASTIC_YELLOW)
    });
    c.line(j2, shade + Vec3::Y * 2.0, mat::STEEL, false);
    c.w.fill_sphere(shade - Vec3::Y * 0.6, 1.05, mat::FLUORESCENT, false);
    c.line(shade - Vec3::Y * 0.6, shade + Vec3::Y * 2.0, mat::STEEL, true);
    // Laptop (open): base with keyboard, screen tilted back with a glowing face.
    let (lx0, lx1, lz0, lz1) = (7.0, 18.0, -45.0, -38.0);
    c.fill_with(Vec3::new(lx0, y, lz0), Vec3::new(lx1, y + 0.5, lz1), |p, _, _| {
        let keys = p.x > lx0 + 1.0 && p.x < lx1 - 1.0 && p.z > lz0 + 1.0 && p.z < lz0 + 4.0;
        let pad = (p.x - (lx0 + lx1) * 0.5).abs() < 1.8 && p.z > lz0 + 4.5 && p.z < lz1 - 0.5;
        Some(if keys { mat::PLASTIC_BLACK } else if pad { mat::HULL_WHITE } else { mat::STEEL })
    });
    for k in 0..14 {
        let yy = y + 0.5 + k as f32 * 0.5;
        let back = lz0 - (k as f32 * 0.15).floor() * 0.5;
        bx(c, lx0, yy, back - 0.5, lx1, yy + 0.5, back, mat::STEEL);
        let border = k < 1 || k > 12;
        if border {
            bx(c, lx0, yy, back, lx1, yy + 0.5, back + 0.5, mat::STEEL);
        } else {
            bx(c, lx0, yy, back, lx0 + 0.5, yy + 0.5, back + 0.5, mat::STEEL);
            bx(c, lx1 - 0.5, yy, back, lx1, yy + 0.5, back + 0.5, mat::STEEL);
            bx(c, lx0 + 0.5, yy, back, lx1 - 0.5, yy + 0.5, back + 0.5, mat::SCREEN_GLOW);
        }
    }
    // Pencil cup with pencils and crayons, tips up.
    let cup = Vec2::new(5.0, -48.5);
    cyl(c, cup.x, cup.y, 1.35, 0.0, y, y + 0.5, mat::PLASTIC_BLUE);
    cyl(c, cup.x, cup.y, 1.35, 0.8, y + 0.5, y + 3.0, mat::PLASTIC_BLUE);
    for (k, (dx, dz, h)) in [(0.6f32, 0.4f32, 6.2f32), (-0.7, 0.9, 5.6), (1.4, -1.2, 5.9), (-1.3, -0.9, 5.2)].iter().enumerate() {
        let a = Vec3::new(cup.x, y + 0.5, cup.y);
        let b = Vec3::new(cup.x + dx, y + h, cup.y + dz);
        pencil(c, a, b, if k == 3 { mat::PLASTIC_GREEN } else { mat::PAINT_YELLOW }, k == 3);
    }
    // Loose pencil, eraser, coins, a stack of three books and a mug.
    pencil(c, Vec3::new(11.5, y + 0.25, -36.0), Vec3::new(16.5, y + 0.25, -35.0), mat::PAINT_YELLOW, false);
    bx(c, 20.0, y, -38.0, 21.5, y + 0.5, -37.0, mat::STUCCO_PINK);
    cyl(c, 21.0, -46.5, 1.0, 0.0, y, y + 0.5, mat::TERRACOTTA);
    cyl(c, 22.2, -45.0, 1.0, 0.0, y, y + 0.5, mat::STEEL);
    let mut rng = Rng::new(seed as u64 ^ 0xde5c);
    let covers = [mat::BOOK_RED, mat::BOOK_GREEN, mat::BOOK_PURPLE];
    for k in 0..3 {
        let (ox, oz) = (rng.range(-0.5, 0.5), rng.range(-0.5, 0.5));
        flat_book(c, 22.0 + ox, 29.0 + ox, -45.0 + oz, -38.5 + oz, y + k as f32, y + k as f32 + 1.0, covers[k]);
    }
    mug(c, Vec2::new(36.0, -38.0), y, mat::STUCCO_MINT);
    // Waste basket under the desk with a couple of paper balls.
    cyl(c, 30.0, -46.0, 3.5, 0.0, FLOOR_Y, FLOOR_Y + 0.5, mat::PLASTIC_GREEN);
    cyl(c, 30.0, -46.0, 3.5, 3.0, FLOOR_Y + 0.5, FLOOR_Y + 8.0, mat::PLASTIC_GREEN);
    for (x, z) in [(29.0, -47.0), (31.0, -45.0)] {
        c.w.fill_sphere(Vec3::new(x, FLOOR_Y + 1.5, z), 1.2, mat::BOOK_PAGES, true);
    }

    // Chair (west of the desk, facing it): legs, rungs, seat, slatted back on the west side.
    let ch = CHAIR;
    let seat = CHAIR_SEAT;
    for (x, z) in [(ch.x0, ch.z0), (ch.x1 - 1.0, ch.z0), (ch.x0, ch.z1 - 1.0), (ch.x1 - 1.0, ch.z1 - 1.0)] {
        bx(c, x, FLOOR_Y, z, x + 1.0, seat - 1.0, z + 1.0, mat::FURNITURE_FRAME);
    }
    for (x0, z0, x1, z1) in [(ch.x0, ch.z0, ch.x0 + 0.5, ch.z1), (ch.x1 - 0.5, ch.z0, ch.x1, ch.z1), (ch.x0, ch.z0, ch.x1, ch.z0 + 0.5), (ch.x0, ch.z1 - 0.5, ch.x1, ch.z1)] {
        bx(c, x0, FLOOR_Y + 4.0, z0, x1, FLOOR_Y + 4.5, z1, mat::FURNITURE_FRAME);
    }
    bx(c, ch.x0, seat - 1.0, ch.z0, ch.x1, seat - 0.5, ch.z1, mat::FURNITURE_FRAME);
    bx(c, ch.x0, seat - 0.5, ch.z0, ch.x1, seat, ch.z1, mat::FURNITURE_TOP);
    for z in [ch.z0, ch.z1 - 1.0] {
        bx(c, ch.x0, seat, z, ch.x0 + 1.0, seat + 11.0, z + 1.0, mat::FURNITURE_FRAME);
    }
    for yy in [seat + 5.0, seat + 7.5, seat + 10.0] {
        bx(c, ch.x0, yy, ch.z0 + 1.0, ch.x0 + 1.0, yy + 1.0, ch.z1 - 1.0, mat::FURNITURE_TOP);
    }
    c.anchor(IVec3::new(vx(ch.x0), vx(FLOOR_Y + 1.0), vx(ch.z0)));
}

/// A pencil from `a` (eraser end) to `b` (sharpened tip): 1-voxel body, ferrule, eraser, cone, lead.
fn pencil(c: &mut City, a: Vec3, b: Vec3, body: Material, crayon: bool) {
    let len = (b - a).length();
    let dir = (b - a) / len.max(1e-3);
    c.line(a, b, body, false);
    if crayon {
        c.line(b - dir * 1.0, b, mat::PLASTIC_GREEN, false);
        c.line(a + dir * 1.0, a + dir * 1.5, mat::BOOK_PAGES, false);
        return;
    }
    c.line(a, a + dir * 0.5, mat::STUCCO_PINK, false);
    c.line(a + dir * 0.5, a + dir * 1.0, mat::STEEL, false);
    c.line(b - dir * 1.2, b - dir * 0.4, mat::WOOD, false);
    c.line(b - dir * 0.4, b, mat::PLASTIC_BLACK, false);
}

// ------------------------------------------------------------------------------------------------
// Toy box, vent, sneakers, socks, crayons

fn build_toy_box(c: &mut City, seed: u32) {
    let t = TOY_BOX;
    let rim = TOY_BOX_RIM;
    // Four 1 m walls (painted wood), no bottom: the inside is a pit into the void.
    for (x0, z0, x1, z1) in [(t.x0, t.z0, t.x1, t.z0 + 1.0), (t.x0, t.z1 - 1.0, t.x1, t.z1), (t.x0, t.z0, t.x0 + 1.0, t.z1), (t.x1 - 1.0, t.z0, t.x1, t.z1)] {
        bx(c, x0, FLOOR_Y, z0, x1, rim, z1, mat::PAINT_RED);
    }
    // Brass corner caps and a yellow band.
    for (x, z) in [(t.x0, t.z0), (t.x1 - 1.0, t.z0), (t.x0, t.z1 - 1.0), (t.x1 - 1.0, t.z1 - 1.0)] {
        bx(c, x, rim - 1.0, z, x + 1.0, rim, z + 1.0, mat::BRASS);
    }
    bx(c, t.x0, rim - 3.0, t.z0 - 0.5, t.x1, rim - 2.5, t.z0, mat::PLASTIC_YELLOW);
    // Lid standing open against the wall, on brass hinges.
    bx(c, t.x0, rim, t.z1, t.x1, rim + 12.0, t.z1 + 1.0, mat::PAINT_RED);
    for x in [t.x0 + 3.0, t.x1 - 4.0] {
        bx(c, x, rim - 0.5, t.z1 - 0.5, x + 1.0, rim + 0.5, t.z1 + 1.5, mat::BRASS);
    }
    carve_pit(c, TOY_BOX_PIT);
    // Block staircase up the front (1.5 m blocks, 1.5 m steps) to the rim.
    let cols = [mat::PLASTIC_BLUE, mat::PLASTIC_YELLOW, mat::PLASTIC_GREEN, mat::FURNITURE_TOP, mat::PLASTIC_RED];
    for k in 0..8 {
        let x = t.x0 + 2.0 + 1.5 * k as f32;
        for j in 0..=k {
            let m = cols[((k * 3 + j) as u32 ^ seed) as usize % cols.len()];
            block(c, x, FLOOR_Y + 1.5 * j as f32, t.z0 - 3.0, m);
            block(c, x, FLOOR_Y + 1.5 * j as f32, t.z0 - 1.5, m);
        }
    }
    // A ball balanced on the rim's corner.
    c.w.fill_sphere(Vec3::new(t.x0 + 2.0, rim + 2.2, t.z0 + 1.5), 2.3, mat::PLASTIC_BLUE, true);
}

/// A 1.5 m toy block (3 × 3 × 3 voxels) with its minimum corner at (x, y, z).
fn block(c: &mut City, x: f32, y: f32, z: f32, m: Material) {
    bx(c, x, y, z, x + 1.5, y + 1.5, z + 1.5, m);
}

fn build_vent(c: &mut City) {
    let v = VENT;
    // The hole itself into the void, a metal duct frame round it, the grille lying beside it.
    carve_pit(c, v);
    c.fill_with(Vec3::new(v.x0 - 0.5, FLOOR_Y - 0.5, v.z0 - 0.5), Vec3::new(v.x1 + 0.5, FLOOR_Y, v.z1 + 0.5), |_, _, old| (old != AIR).then_some(mat::TOWER_CORE));
    for z in 0..20 {
        let zz = v.z0 - 0.5 + z as f32 * 0.5;
        let m = if z % 2 == 0 { mat::STEEL } else { AIR };
        if m != AIR {
            bx(c, v.x0 - 8.0, FLOOR_Y, zz, v.x0 - 3.5, FLOOR_Y + 0.5, zz + 0.5, m);
        }
    }
    bx(c, v.x0 - 8.0, FLOOR_Y, v.z0 - 0.5, v.x0 - 7.5, FLOOR_Y + 0.5, v.z1 + 0.5, mat::STEEL);
    bx(c, v.x0 - 4.0, FLOOR_Y, v.z0 - 0.5, v.x0 - 3.5, FLOOR_Y + 0.5, v.z1 + 0.5, mat::STEEL);
}

/// A sock lying on the floor (or a heap): leg tube then a foot bent ~100°, stripes and heel/toe.
fn sock(c: &mut City, at: Vec3, dir: Vec2, stripe: Material, seed: u32) {
    let d = Vec3::new(dir.x, 0.0, dir.y).normalize_or(Vec3::X);
    let side = Vec3::new(-d.z, 0.0, d.x);
    let r = 0.9;
    let leg = 4.5;
    let foot = 2.8;
    let knee = at + d * leg;
    let n = 18;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let p = at + d * (leg * t) + Vec3::Y * r;
        let band = ((t * leg) / 0.9) as i32 % 2 == 0 && t < 0.35;
        c.w.fill_sphere(p, r, if band { stripe } else { mat::SOCK_WHITE }, false);
    }
    for i in 0..=10 {
        let t = i as f32 / 10.0;
        let p = knee + side * (foot * t) + Vec3::Y * r;
        let tip = t > 0.8;
        c.w.fill_sphere(p, r * (1.0 - 0.15 * t), if tip { stripe } else { mat::SOCK_WHITE }, false);
    }
    c.w.fill_sphere(knee + Vec3::Y * r - d * 0.3, r, stripe, false);
    let _ = seed;
}

fn build_socks(c: &mut City, seed: u32) {
    let s = SOCKS;
    let ctr = s.center();
    // The heap: a soft mound of knit, socks draped all over it.
    c.w.fill_ellipsoid(Vec3::new(ctr.x, FLOOR_Y + 1.0, ctr.y), Vec3::new(7.0, 5.5, 6.5), mat::SOCK_WHITE, true);
    let stripes = [mat::BOOK_RED, mat::DUVET, mat::PLASTIC_GREEN, mat::PLASTIC_YELLOW, mat::BOOK_PURPLE];
    let mut rng = Rng::new(seed as u64 ^ 0x50c5);
    for k in 0..12 {
        let a = rng.range(0.0, TAU);
        let rr = rng.range(0.0, 5.5);
        let (x, z) = (ctr.x + dcos(a) * rr, ctr.y + dsin(a) * rr * 0.9);
        // Rest on top of whatever is there.
        let y = (0..40).rev().map(|k| FLOOR_Y + k as f32 * 0.5).find(|&yy| c.w.get(IVec3::new(vx(x), vx(yy), vx(z))) != AIR).map_or(FLOOR_Y, |yy| yy + 0.5);
        let ang = rng.range(0.0, TAU);
        sock(c, Vec3::new(x - dcos(ang) * 2.0, y - 0.8, z - dsin(ang) * 2.0), Vec2::new(dcos(ang), dsin(ang)), stripes[k % stripes.len()], seed ^ k as u32);
    }
}

/// A big sneaker (27 cm real → 8 m) with an open collar you can drop into.
fn sneaker(c: &mut City, x0: f32, z0: f32) {
    let (len, wid) = (8.0f32, 3.2f32);
    let zc = z0 + wid * 0.5;
    // Height of the upper along the shoe (toe at +x): low toe, high heel/collar.
    let h = |u: f32| -> f32 {
        let t = (u / len).clamp(0.0, 1.0);
        2.0 + 1.6 * (1.0 - t) * (1.0 - t) + if t > 0.55 { 0.0 } else { 0.4 }
    };
    c.fill_with(Vec3::new(x0, FLOOR_Y, z0), Vec3::new(x0 + len, FLOOR_Y + 4.5, z0 + wid), |p, _, _| {
        let u = len - (p.x - x0); // 0 at the heel (+x end is the toe)
        let (y, dz) = (p.y - FLOOR_Y, (p.z - zc).abs());
        // Rounded toe and heel in plan.
        let tr = if u > len - 1.6 { wid * 0.5 * (1.0 - ((u - (len - 1.6)) / 1.6).powi(2)).max(0.0).sqrt() } else { wid * 0.5 };
        if dz > tr {
            return None;
        }
        if y < 0.5 {
            return Some(mat::HULL_WHITE);
        }
        let top = h(u);
        if y > top {
            return None;
        }
        let shell = dz > tr - 0.5 || y > top - 0.5 || u < 0.5 || u > len - 0.5;
        if !shell {
            return None;
        }
        // Collar opening: the top over the heel half is open.
        if y > top - 0.5 && u < len * 0.45 && dz < tr - 0.5 {
            return None;
        }
        let toe_cap = u > len - 1.5 && y < 1.6;
        let sole_side = y < 1.0;
        Some(if sole_side { mat::HULL_WHITE } else if toe_cap { mat::PAINT_WHITE } else { mat::PLASTIC_RED })
    });
    // Laces across the tongue.
    for k in 0..5 {
        let x = x0 + len * 0.45 + k as f32 * 0.6;
        let y = FLOOR_Y + h(len - (x - x0)) + 0.25;
        bx(c, x, y - 0.5, zc - 1.0, x + 0.5, y, zc + 1.0, mat::PAINT_WHITE);
    }
}

fn build_scatter(c: &mut City, seed: u32) {
    sneaker(c, -8.0, 38.0);
    sneaker(c, 1.5, 44.5);
    // Crayons spilled on the rug by the castle (9 cm → 2.5 m sticks).
    let cols = [mat::PLASTIC_RED, mat::PLASTIC_BLUE, mat::PLASTIC_YELLOW, mat::PLASTIC_GREEN, mat::BOOK_PURPLE, mat::HAZARD_ORANGE];
    let mut rng = Rng::new(seed as u64 ^ 0xc4a7);
    for k in 0..6 {
        let (x, z) = (rng.range(8.0, 22.0), rng.range(-17.0, -13.5));
        let a = rng.range(0.0, PI);
        let d = Vec3::new(dcos(a), 0.0, dsin(a));
        let p = Vec3::new(x, FLOOR_Y + 0.75, z);
        c.line(p, p + d * 2.5, cols[k], false);
        c.line(p + d * 0.6, p + d * 1.8, mat::BOOK_PAGES, false);
        c.line(p + d * 2.5, p + d * 3.0, cols[k], false);
    }
    // A couple of coins on the floor (a 5 cm coin is a 1.5 m disc you can stand on).
    cyl(c, -26.0, 30.0 + 4.0, 1.0, 0.0, FLOOR_Y, FLOOR_Y + 0.5, mat::TERRACOTTA);
    cyl(c, 46.0, -30.0, 1.0, 0.0, FLOOR_Y, FLOOR_Y + 0.5, mat::STEEL);
}

// ------------------------------------------------------------------------------------------------
// Rug, cat, castle

fn build_rug(c: &mut City, seed: u32) {
    let r = RUG;
    let y = vx(FLOOR_Y);
    for z in vx(r.z0)..vx(r.z1) {
        for x in vx(r.x0)..vx(r.x1) {
            let (mx, mz) = (x as f32 * 0.5 + 0.25, z as f32 * 0.5 + 0.25);
            let edge = (mx - r.x0).min(r.x1 - mx).min(mz - r.z0).min(r.z1 - mz);
            let m = if edge < 2.0 || (edge >= 3.5 && edge < 4.0) { mat::RUG_BORDER } else { mat::RUG };
            set(c, IVec3::new(x, y, z), m);
        }
    }
    let _ = seed;
}

/// Shag tufts on the rug (after everything that stands on it): dense patches, sparse elsewhere.
fn rug_tufts(c: &mut City, seed: u32) {
    let r = RUG;
    let y = vx(FLOOR_Y) + 1;
    let mut rng = Rng::new(seed as u64 ^ 0x70f7);
    let mut patches: Vec<(Vec2, f32)> = Vec::new();
    for _ in 0..200 {
        if patches.len() >= 6 {
            break;
        }
        let p = Vec2::new(rng.range(r.x0 + 6.0, r.x1 - 6.0), rng.range(r.z0 + 6.0, r.z1 - 6.0));
        if CASTLE.grow(4.0).contains(p.x, p.y) || p.distance(cat_center()) < 11.0 || patches.iter().any(|(q, _)| q.distance(p) < 12.0) {
            continue;
        }
        patches.push((p, rng.range(3.0, 5.5)));
    }
    for z in vx(r.z0 + 2.0)..vx(r.z1 - 2.0) {
        for x in vx(r.x0 + 2.0)..vx(r.x1 - 2.0) {
            let (mx, mz) = (x as f32 * 0.5 + 0.25, z as f32 * 0.5 + 0.25);
            let v = IVec3::new(x, y, z);
            if get(c, v) != AIR || get(c, v - IVec3::Y) != mat::RUG {
                continue;
            }
            let h = unit(hash3(x, 3, z, seed ^ 0x7f7f));
            let inside = patches.iter().map(|(p, rad)| 1.0 - Vec2::new(mx, mz).distance(*p) / rad).fold(0.0f32, f32::max);
            let (dense, tall) = if inside > 0.0 { (0.25 + 0.3 * inside, 0.35 + 0.4 * inside) } else { (0.025, 0.0) };
            if h < dense {
                let two = unit(hash3(x, 9, z, seed)) < tall;
                if two && get(c, v + IVec3::Y) == AIR {
                    set(c, v, mat::RUG);
                    set(c, v + IVec3::Y, mat::RUG_TUFT);
                } else {
                    set(c, v, mat::RUG_TUFT);
                }
            }
        }
    }
}

fn build_cat(c: &mut City) {
    let p = cat_center();
    let base = FLOOR_Y + 0.5;
    let body = Vec3::new(p.x, base + 2.0, p.y);
    let br = Vec3::new(6.5, 4.2, 5.2);
    // Body (flat underneath): ginger with tabby stripes across the back, cream belly on the south.
    c.fill_with(body - br, body + br, |q, _, _| {
        let d = (q - body) / br;
        if d.length_squared() > 1.0 || q.y < base {
            return None;
        }
        let stripe = (dsin((q.x - body.x) * 1.35 + (q.z - body.z) * 0.35) > 0.55) && q.y > body.y + 0.6;
        let belly = d.z < -0.45 && q.y < body.y + 1.2;
        Some(if belly { mat::CAT_CREAM } else if stripe { mat::TERRACOTTA } else { mat::CAT_GINGER })
    });
    // Head tucked at the east end, facing the window, chin on the paws.
    let head = Vec3::new(p.x + 5.2, base + 2.6, p.y - 2.8);
    c.fill_with(head - Vec3::splat(3.2), head + Vec3::splat(3.2), |q, _, _| {
        let d = q - head;
        if d.length() > 3.0 || q.y < base {
            return None;
        }
        let muzzle = d.x > 1.6 && d.y < 0.4;
        let stripe = d.y > 1.4 && (dsin(d.z * 2.4) > 0.5);
        Some(if muzzle { mat::CAT_CREAM } else if stripe { mat::TERRACOTTA } else { mat::CAT_GINGER })
    });
    for dz in [-1.6f32, 1.6] {
        // Ears: stacked, narrowing squares.
        for k in 0..3 {
            let s = 1.0 - k as f32 * 0.4;
            let yy = head.y + 2.6 + k as f32 * 0.5;
            let m = if k == 0 && s > 0.8 { mat::STUCCO_PINK } else { mat::CAT_GINGER };
            bx(c, head.x - s * 0.5, yy, head.z + dz - s * 0.5, head.x + s * 0.5 + 0.5, yy + 0.5, head.z + dz + s * 0.5 + 0.5, m);
        }
        // Closed eyes: little dark curves.
        c.line(Vec3::new(head.x + 2.75, head.y + 0.9, head.z + dz * 0.55 - 0.4), Vec3::new(head.x + 2.75, head.y + 0.9, head.z + dz * 0.55 + 0.4), mat::PLASTIC_BLACK, false);
    }
    set(c, IVec3::new(vx(head.x + 3.0), vx(head.y + 0.1), vx(head.z)), mat::STUCCO_PINK);
    // Front paws under the chin, the tail wrapped round the front.
    for dz in [-1.0f32, 1.2] {
        c.w.fill_ellipsoid(Vec3::new(head.x + 1.8, base + 0.6, head.z + dz - 1.5), Vec3::new(1.6, 0.7, 0.9), mat::CAT_CREAM, true);
    }
    c.curve(
        Vec3::new(p.x - 6.0, base + 0.9, p.y - 1.0),
        Vec3::new(p.x - 3.0, base + 0.9, p.y - 7.8),
        Vec3::new(p.x + 3.2, base + 0.9, p.y - 5.6),
        1.15,
        0.8,
        mat::CAT_GINGER,
        true,
    );
    c.w.fill_sphere(Vec3::new(p.x + 3.4, base + 0.8, p.y - 5.4), 0.8, mat::CAT_CREAM, false);
}

fn build_castle(c: &mut City, seed: u32) {
    let a = CASTLE;
    let (bw, bd) = (12, 10); // blocks
    let s = 1.5;
    let cols = [mat::PLASTIC_RED, mat::PLASTIC_BLUE, mat::PLASTIC_YELLOW, mat::PLASTIC_GREEN, mat::FURNITURE_TOP];
    let col = |i: i32, j: i32, k: i32| cols[(hash3(i, j, k, seed ^ 0xca57) % cols.len() as u32) as usize];
    let y0 = FLOOR_Y + 0.5; // on the rug
    let at = |i: i32, k: i32, j: i32| (a.x0 + i as f32 * s, y0 + k as f32 * s, a.z0 + j as f32 * s);
    let tower = |i: i32, j: i32| -> Option<i32> {
        match (i, j) {
            (0..=1, 0..=1) => Some(4),
            (10..=11, 0..=1) => Some(5),
            (0..=1, 8..=9) => Some(6),
            (10..=11, 8..=9) => Some(7),
            _ => None,
        }
    };
    let keep = |i: i32, j: i32| (4..=6).contains(&i) && (4..=6).contains(&j);
    for j in 0..bd {
        for i in 0..bw {
            let wall = i == 0 || j == 0 || i == bw - 1 || j == bd - 1;
            let h = if let Some(t) = tower(i, j) {
                t
            } else if keep(i, j) {
                5
            } else if wall {
                3
            } else {
                0
            };
            for k in 0..h {
                // Gate in the south wall: 2 blocks wide, 2 high.
                if j == 0 && (5..=6).contains(&i) && k < 2 {
                    continue;
                }
                let (x, y, z) = at(i, k, j);
                block(c, x, y, z, col(i, j, k));
            }
            // Merlons: small cubes on the outer edge of walls and towers.
            if h > 0 && (wall || tower(i, j).is_some()) && (i + j) % 2 == 0 && !keep(i, j) {
                let (x, y, z) = at(i, h, j);
                let (ox, oz) = (if i == 0 { 0.0 } else if i == bw - 1 { 1.0 } else { 0.5 }, if j == 0 { 0.0 } else if j == bd - 1 { 1.0 } else { 0.5 });
                bx(c, x + ox, y, z + oz, x + ox + 0.5, y + 0.5, z + oz + 0.5, col(i, j, 9));
            }
        }
    }
    // Keep roof: a red stepped pyramid.
    for k in 0..3 {
        let (x, y, z) = at(4, 5, 4);
        let inset = k as f32 * 1.0;
        bx(c, x + inset, y + k as f32 * 0.5, z + inset, x + 4.5 - inset, y + (k + 1) as f32 * 0.5, z + 4.5 - inset, mat::PLASTIC_RED);
    }
    // Arch block over the gate.
    let (gx, gy, gz) = at(5, 2, 0);
    bx(c, gx - 0.5, gy - 0.5, gz, gx + 3.5, gy, gz + 1.5, mat::PLASTIC_YELLOW);
    // Block steps up the west side to the wall walk (1.5 m each).
    for k in 0..3 {
        for m in 0..=k {
            let (x, y, z) = at(-3 + k, m, 4);
            block(c, x, y, z, col(-3 + k, 4, m));
            block(c, x, y, z + s, col(-3 + k, 5, m));
        }
    }
    // The pencil bridge: keep roof edge → NE tower top.
    let (kx, ky, kz) = at(7, 5, 7);
    let (tx, ty, tz) = at(10, 7, 8);
    pencil(c, Vec3::new(kx - 0.25, ky + 0.25, kz - 0.25), Vec3::new(tx + 0.75, ty + 0.25, tz + 0.75), mat::PAINT_YELLOW, false);
}

// ------------------------------------------------------------------------------------------------
// Curtains, fan

fn build_curtains(c: &mut City, seed: u32) {
    // Brass rod on brackets, finials.
    bx(c, HX - 3.0, ROD_Y, -34.0, HX - 2.0, ROD_Y + 1.0, 34.0, mat::BRASS);
    for z in [-33.0f32, 32.5] {
        bx(c, HX - 3.0, ROD_Y, z, HX, ROD_Y + 1.0, z + 0.5, mat::BRASS);
        c.w.fill_sphere(Vec3::new(HX - 2.5, ROD_Y + 0.5, if z < 0.0 { -34.5 } else { 34.5 }), 1.0, mat::BRASS, false);
    }
    // Two gathered panels hanging in folds, 2 m above the floor.
    for &(z0, z1) in &CURTAINS {
        for z in vx(z0)..vx(z1) {
            let mz = z as f32 * 0.5 + 0.25;
            let fold = |zz: f32| HX - 3.5 + 1.1 * dsin(zz * TAU / 3.5 + (seed % 7) as f32);
            let (xa, xb) = (fold(mz - 0.25).min(fold(mz + 0.25)), fold(mz - 0.25).max(fold(mz + 0.25)));
            for x in vx(xa)..=vx(xb) {
                for y in vx(FLOOR_Y + 2.0)..vx(ROD_Y) {
                    set(c, IVec3::new(x, y, z), mat::CURTAIN);
                }
            }
            // Header: tie the top into the rod.
            for x in vx(xa)..=vx(HX - 2.5) {
                set(c, IVec3::new(x, vx(ROD_Y - 0.5), z), mat::CURTAIN);
            }
        }
    }
}

fn build_fan(c: &mut City) {
    let f = FAN;
    // Indestructible mount plate in the plaster, canopy, downrod, motor, light bowl.
    bx(c, f.x - 1.5, CEIL_Y, f.z - 1.5, f.x + 1.5, CEIL_Y + 2.0, f.z + 1.5, mat::TRIM);
    c.anchor(IVec3::new(vx(f.x), vx(CEIL_Y + 0.5), vx(f.z)));
    cyl(c, f.x, f.z, 1.6, 0.0, CEIL_Y - 1.0, CEIL_Y, mat::TRIM);
    bx(c, f.x - 0.25, f.y + 3.5, f.z - 0.25, f.x + 0.25, CEIL_Y - 1.0, f.z + 0.25, mat::TRIM);
    cyl(c, f.x, f.z, 3.0, 0.0, f.y + 0.5, f.y + 3.5, mat::TRIM);
    cyl(c, f.x, f.z, 3.1, 2.4, f.y + 0.5, f.y + 1.0, mat::BRASS);
    c.fill_with(Vec3::new(f.x - 2.5, f.y - 2.0, f.z - 2.5), Vec3::new(f.x + 2.5, f.y + 0.5, f.z + 2.5), |p, _, _| {
        let d = Vec3::new(p.x - f.x, (p.y - (f.y + 0.5)) * 1.2, p.z - f.z);
        (d.length() <= 2.2).then_some(mat::PILLOW)
    });
    // Four wooden blades on steel irons, caught mid-turn (static).
    for k in 0..4 {
        let a = (15.0 + 90.0 * k as f32).to_radians();
        let (dx, dz) = (dcos(a), dsin(a));
        let n = Vec2::new(-dz, dx);
        c.fill_with(Vec3::new(f.x - FAN_R - 1.0, f.y - 0.5, f.z - FAN_R - 1.0), Vec3::new(f.x + FAN_R + 1.0, f.y + 1.0, f.z + FAN_R + 1.0), |p, _, _| {
            let q = Vec2::new(p.x - f.x, p.z - f.z);
            let along = q.x * dx + q.y * dz;
            let across = q.dot(n);
            if p.y < f.y + 0.5 {
                // Blade: 4.5 m wide, a little wider at the tip, rounded end.
                let half = 2.0 + 0.25 * ((along - 4.5) / (FAN_R - 4.5)).clamp(0.0, 1.0);
                let end = if along > FAN_R - half { ((along - (FAN_R - half)).powi(2) + across * across).sqrt() <= half } else { true };
                (along >= 4.5 && along <= FAN_R && across.abs() <= half && end).then_some(mat::FURNITURE_TOP)
            } else {
                (along >= 2.5 && along <= 6.0 && across.abs() <= 0.6).then_some(mat::STEEL)
            }
        });
    }
}

// ------------------------------------------------------------------------------------------------
// Toy track

#[derive(Clone, Copy)]
enum Seg {
    Line(Vec2, Vec2),
    /// Centre, radius, start / end angle (radians; x = cos, z = sin).
    Arc(Vec2, f32, f32, f32),
}

impl Seg {
    fn len(&self) -> f32 {
        match *self {
            Seg::Line(a, b) => a.distance(b),
            Seg::Arc(_, r, a0, a1) => r * (a1 - a0).abs(),
        }
    }
    /// Position and unit tangent at fraction `t`.
    fn at(&self, t: f32) -> (Vec2, Vec2) {
        match *self {
            Seg::Line(a, b) => (a.lerp(b, t), (b - a).normalize_or(Vec2::X)),
            Seg::Arc(c, r, a0, a1) => {
                let a = a0 + (a1 - a0) * t;
                let s = if a1 > a0 { 1.0 } else { -1.0 };
                (c + Vec2::new(dcos(a), dsin(a)) * r, Vec2::new(-dsin(a), dcos(a)) * s)
            }
        }
    }
}

/// The loop's pieces, counter-clockwise from the south straight (heading east).
pub fn track_loop() -> Vec<(Vec2, Vec2)> {
    let (w, e, s, n, r) = (TRACK_W, TRACK_E, TRACK_S, TRACK_N, TRACK_R);
    let segs = loop_segs(w, e, s, n, r);
    let mut out = Vec::new();
    for sg in segs {
        let k = (sg.len() / 4.0).ceil().max(1.0) as i32;
        for i in 0..k {
            out.push(sg.at(i as f32 / k as f32));
        }
    }
    out
}

fn loop_segs(w: f32, e: f32, s: f32, n: f32, r: f32) -> Vec<Seg> {
    let h = PI * 0.5;
    vec![
        Seg::Line(Vec2::new(w + r, s), Vec2::new(e - r, s)),
        Seg::Arc(Vec2::new(e - r, s + r), r, -h, 0.0),
        Seg::Line(Vec2::new(e, s + r), Vec2::new(e, n - r)),
        Seg::Arc(Vec2::new(e - r, n - r), r, 0.0, h),
        Seg::Line(Vec2::new(e - r, n), Vec2::new(w + r, n)),
        Seg::Arc(Vec2::new(w + r, n - r), r, h, PI),
        Seg::Line(Vec2::new(w, n - r), Vec2::new(w, s + r)),
        Seg::Arc(Vec2::new(w + r, s + r), r, PI, PI + h),
    ]
}

/// Track surface top on the west straight's kicker jump (`None` = the gap).
fn kicker_height(z: f32) -> Option<f32> {
    let f = FLOOR_Y + 0.5;
    Some(if (10.0..16.0).contains(&z) {
        f + (16.0 - z) * 0.5
    } else if (7.0..10.0).contains(&z) {
        f + 3.0
    } else if (-1.0..7.0).contains(&z) {
        return None;
    } else if (-4.0..-1.0).contains(&z) {
        f + 3.0
    } else if (-10.0..-4.0).contains(&z) {
        f + (z + 10.0) * 0.5
    } else {
        f
    })
}

/// Launch-ramp surface top at z (desk top → floor run-out).
pub fn ramp_height(z: f32) -> f32 {
    let top = DESK_TOP + 0.5;
    if z < DESK.z1 {
        top
    } else {
        (top - (z - DESK.z1) * 0.5).max(FLOOR_Y + 0.5)
    }
}

#[derive(Clone, Copy)]
struct TrackCol {
    top: i32,
    lip: bool,
}

/// A rasterised track layer: per floor column, the surface voxel and whether it's a lip.
struct TrackGrid {
    x0: i32,
    z0: i32,
    nx: usize,
    nz: usize,
    cells: Vec<Option<TrackCol>>,
}

impl TrackGrid {
    fn new() -> Self {
        let (x0, z0) = (vx(-HX), vx(-HZ));
        let (nx, nz) = ((vx(HX) - x0) as usize, (vx(HZ) - z0) as usize);
        Self { x0, z0, nx, nz, cells: vec![None; nx * nz] }
    }
    fn idx(&self, x: i32, z: i32) -> Option<usize> {
        let (ix, iz) = (x - self.x0, z - self.z0);
        (ix >= 0 && iz >= 0 && (ix as usize) < self.nx && (iz as usize) < self.nz).then(|| iz as usize * self.nx + ix as usize)
    }
    fn get(&self, x: i32, z: i32) -> Option<TrackCol> {
        self.idx(x, z).and_then(|i| self.cells[i])
    }
    /// Stamp a cross-section at `p` (tangent `t`, surface top `h`). `bank` = the centre of a banked
    /// corner: no lip on its outside (the surf bank starts there).
    fn stamp(&mut self, p: Vec2, t: Vec2, h: f32, bank: Option<Vec2>) {
        let nrm = Vec2::new(-t.y, t.x);
        let top = vx(h - 0.25);
        let mut o = -(LANE + LIP);
        while o <= LANE + LIP + 1e-3 {
            let q = p + nrm * o;
            let lip = o.abs() > LANE && !bank.is_some_and(|ctr| q.distance(ctr) > p.distance(ctr));
            if let Some(i) = self.idx(vx(q.x), vx(q.y)) {
                match &mut self.cells[i] {
                    Some(tc) if tc.top > top => {}
                    Some(tc) if tc.top == top => tc.lip &= lip,
                    cell => *cell = Some(TrackCol { top, lip }),
                }
            }
            o += 0.25;
        }
    }
}

fn build_track(c: &mut City) {
    let mut ring = TrackGrid::new();
    let mut ramp = TrackGrid::new();
    let banked = [Vec2::new(TRACK_E - TRACK_R, TRACK_N - TRACK_R), Vec2::new(TRACK_W + TRACK_R, TRACK_S + TRACK_R)];
    for sg in loop_segs(TRACK_W, TRACK_E, TRACK_S, TRACK_N, TRACK_R) {
        let n = (sg.len() / 0.25).ceil() as i32;
        let bank = match sg {
            Seg::Arc(ctr, ..) => banked.iter().copied().find(|b| b.distance(ctr) < 0.1),
            _ => None,
        };
        for i in 0..=n {
            let (p, t) = sg.at(i as f32 / n as f32);
            let west = (p.x - TRACK_W).abs() < 0.01;
            let h = if west { kicker_height(p.y) } else { Some(FLOOR_Y + 0.5) };
            if let Some(h) = h {
                ring.stamp(p, t, h, bank);
            }
        }
    }
    // Launch ramp: from the desk top down to the floor, then north into the NE corner's lane.
    let (za, zb) = (DESK.z1 - 8.0, TRACK_N - TRACK_R + 8.5);
    let n = ((zb - za) / 0.25) as i32;
    for i in 0..=n {
        let z = za + (zb - za) * i as f32 / n as f32;
        ramp.stamp(Vec2::new(RAMP_X, z), Vec2::Y, ramp_height(z), None);
    }
    let fy = vx(FLOOR_Y);
    for (g, other, solid) in [(&ring, &ramp, true), (&ramp, &ring, false)] {
        for iz in 0..g.nz {
            for ix in 0..g.nx {
                let Some(tc) = g.cells[iz * g.nx + ix] else { continue };
                let (x, z) = (ix as i32 + g.x0, iz as i32 + g.z0);
                let below = if solid || tc.top <= fy + 1 { fy } else { tc.top - 1 };
                for y in below..=tc.top {
                    set(c, IVec3::new(x, y, z), mat::HAZARD_ORANGE);
                }
                // Where the two layers meet at the same level (the ramp's run-out joining the loop),
                // the lane wins over the lip so the junction is open.
                let open = other.get(x, z).is_some_and(|o| o.top == tc.top && !o.lip);
                if tc.lip && !open {
                    set(c, IVec3::new(x, tc.top + 1, z), mat::HAZARD_ORANGE);
                }
            }
        }
    }
    // Pylons of toy blocks under the launch ramp (up to the plate's underside).
    let cols = [mat::PLASTIC_BLUE, mat::PLASTIC_YELLOW, mat::PLASTIC_GREEN, mat::PLASTIC_RED, mat::FURNITURE_TOP];
    for (k, zc) in [-22.0f32, -10.0, 2.0].iter().enumerate() {
        let top = (vx(ramp_height(*zc) - 0.25) - 1) as f32 * 0.5;
        let n = ((top - FLOOR_Y) / 1.5).floor() as i32;
        for j in 0..n {
            // Offset every other course so the pylon is a (hard) climb.
            let ox = if j % 2 == 0 { -0.75 } else { 0.0 };
            block(c, RAMP_X - 1.5 + ox, FLOOR_Y + j as f32 * 1.5, zc - 0.75, cols[(k + j as usize) % cols.len()]);
            block(c, RAMP_X + ox, FLOOR_Y + j as f32 * 1.5, zc - 0.75, cols[(k + j as usize + 2) % cols.len()]);
        }
        let yb = FLOOR_Y + n as f32 * 1.5;
        if yb < top {
            bx(c, RAMP_X - 1.5, yb, zc - 0.75, RAMP_X + 1.5, top, zc + 0.75, cols[k]);
        }
    }
}

/// Surf lines: the blanket slide and the two banked track corners (three chords each).
pub fn surf_lines() -> Vec<SurfLine> {
    let mut out = vec![SurfLine { a: Vec3::new(-43.0, 27.5, -9.0), b: Vec3::new(-43.0, 11.8, -37.0), face_deg: 55.0, width: 4.5 }];
    for (ctr, a0, h0) in [(Vec2::new(TRACK_E - TRACK_R, TRACK_N - TRACK_R), 0.0f32, 14.0f32), (Vec2::new(TRACK_W + TRACK_R, TRACK_S + TRACK_R), PI, 14.0)] {
        let r = 13.0;
        for k in 0..3 {
            let (t0, t1) = (a0 + k as f32 * PI / 6.0, a0 + (k + 1) as f32 * PI / 6.0);
            let p0 = ctr + Vec2::new(dcos(t0), dsin(t0)) * r;
            let p1 = ctr + Vec2::new(dcos(t1), dsin(t1)) * r;
            let d = (p1 - p0).normalize();
            let (y0, y1) = (h0 - 0.4 * k as f32, h0 - 0.4 * (k + 1) as f32);
            let (e0, e1) = (p0 - d * 1.0, p1 + d * 1.0);
            out.push(SurfLine { a: Vec3::new(e0.x, y0, e0.y), b: Vec3::new(e1.x, y1, e1.y), face_deg: 60.0, width: 6.5 });
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------
// Icon-library toys

/// (icon name, feet position, quarter turns about +Y, materials for Primary / Accent / Dark / Core).
type Toy = (&'static str, Vec3, u8, [Material; 4]);

fn toys() -> Vec<Toy> {
    use crate::material::mat::*;
    vec![
        ("rubber duck", Vec3::new(35.0, FLOOR_Y, 34.5), 3, [PLASTIC_YELLOW, HAZARD_ORANGE, PLASTIC_BLACK, PAINT_WHITE]),
        ("puzzle cube", Vec3::new(9.0, DESK_TOP, -48.5), 1, [PLASTIC_RED, PLASTIC_BLUE, PLASTIC_BLACK, PLASTIC_YELLOW]),
        ("yo-yo", Vec3::new(-13.0, FLOOR_Y + 0.5, -2.0), 1, [PLASTIC_RED, PLASTIC_YELLOW, PLASTIC_BLACK, PAINT_WHITE]),
        ("robot vacuum", Vec3::new(-20.0, FLOOR_Y, -47.5), 0, [PLASTIC_BLACK, STEEL, PLASTIC_BLACK, NEON_CYAN]),
    ]
}

/// A sitting teddy bear (25 cm → 7.5 m), facing +x from `at` (feet centre).
fn teddy(c: &mut City, at: Vec3) {
    use crate::material::mat::*;
    let e = |c: &mut City, p: Vec3, r: Vec3, m: Material| c.w.fill_ellipsoid(at + p, r, m, true);
    e(c, Vec3::new(1.6, 0.9, -1.3), Vec3::new(1.9, 0.9, 0.9), WOOD);
    e(c, Vec3::new(1.6, 0.9, 1.3), Vec3::new(1.9, 0.9, 0.9), WOOD);
    for dz in [-1.3f32, 1.3] {
        c.fill_with(at + Vec3::new(3.0, 0.1, dz - 0.8), at + Vec3::new(3.6, 1.7, dz + 0.8), |p, _, _| (Vec2::new(p.y - at.y - 0.9, p.z - at.z - dz).length() <= 0.75).then_some(CAT_CREAM));
    }
    e(c, Vec3::new(0.0, 2.6, 0.0), Vec3::new(2.0, 2.4, 2.1), WOOD);
    e(c, Vec3::new(1.4, 2.4, 0.0), Vec3::new(0.8, 1.6, 1.4), CAT_CREAM);
    e(c, Vec3::new(0.9, 3.1, -2.1), Vec3::new(0.8, 1.7, 0.7), WOOD);
    e(c, Vec3::new(0.9, 3.1, 2.1), Vec3::new(0.8, 1.7, 0.7), WOOD);
    e(c, Vec3::new(0.2, 6.1, 0.0), Vec3::new(1.8, 1.8, 1.9), WOOD);
    for dz in [-1.4f32, 1.4] {
        e(c, Vec3::new(0.0, 7.6, dz), Vec3::new(0.5, 0.7, 0.7), WOOD);
        c.w.set_raw(((at + Vec3::new(1.9, 6.6, dz * 0.45)) / 0.5).floor().as_ivec3(), PLASTIC_BLACK);
    }
    e(c, Vec3::new(1.7, 5.7, 0.0), Vec3::new(0.7, 0.6, 0.8), CAT_CREAM);
    c.w.set_raw(((at + Vec3::new(2.45, 5.9, 0.0)) / 0.5).floor().as_ivec3(), PLASTIC_BLACK);
    // A red bow tie.
    e(c, Vec3::new(1.6, 4.5, -0.6), Vec3::new(0.35, 0.5, 0.6), PLASTIC_RED);
    e(c, Vec3::new(1.6, 4.5, 0.6), Vec3::new(0.35, 0.5, 0.6), PLASTIC_RED);
}

/// A striped play ball of radius `r` resting on `y`.
fn ball(c: &mut City, x: f32, y: f32, z: f32, r: f32, cols: [Material; 3]) {
    let ctr = Vec3::new(x, y + r, z);
    c.fill_with(ctr - Vec3::splat(r + 0.5), ctr + Vec3::splat(r + 0.5), |p, _, old| {
        let d = p - ctr;
        if d.length() > r || old != AIR {
            return None;
        }
        if d.y.abs() > r * 0.85 {
            return Some(mat::PAINT_WHITE);
        }
        let a = crate::noise::datan2(d.z, d.x) + PI;
        Some(cols[((a / (TAU / 6.0)) as usize) % 3])
    });
}

/// A toy car (7.5 cm → 2.3 m long) on `y`, pointing along +x or +z.
fn toy_car(c: &mut City, x: f32, y: f32, z: f32, along_x: bool, body: Material) {
    let (l, w) = (2.5f32, 1.0f32);
    let (hx, hz) = if along_x { (l * 0.5, w * 0.5) } else { (w * 0.5, l * 0.5) };
    bx(c, x - hx, y + 0.5, z - hz, x + hx, y + 1.0, z + hz, body);
    let (cx, cz) = if along_x { (0.5, hz) } else { (hx, 0.5) };
    bx(c, x - cx, y + 1.0, z - cz, x + cx, y + 1.5, z + cz, mat::GLASS_BLUE);
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (px, pz) = if along_x { (x + sx * (hx - 0.25), z + sz * (hz - 0.25)) } else { (x + sx * (hx - 0.25), z + sz * (hz - 0.25)) };
        bx(c, px - 0.25, y, pz - 0.25, px + 0.25, y + 0.5, pz + 0.25, mat::PLASTIC_BLACK);
    }
}

/// Place baked toys (skipping any the bake didn't produce or that would collide); returns the count.
fn place_toys(c: &mut City, spawns: &[Vec3]) -> usize {
    let mut n = 0;
    for (name, at, turns, mats) in toys() {
        if n >= MAX_TOYS {
            break;
        }
        let Some(icon) = icons::ICONS.iter().find(|i| i.name == name) else { continue };
        let size = icon.voxels.iter().fold(IVec3::ZERO, |a, &(x, y, z, _)| a.max(IVec3::new(x as i32, y as i32, z as i32))) + IVec3::ONE;
        let (sx, sz) = if turns % 2 == 1 { (size.z, size.x) } else { (size.x, size.z) };
        let origin = IVec3::new(vx(at.x) - sx / 2, vx(at.y), vx(at.z) - sz / 2);
        // Keep clear of spawns.
        let ctr = vcen(origin) + Vec3::new(sx as f32 * 0.25, 0.0, sz as f32 * 0.25);
        if spawns.iter().any(|s| Vec2::new(s.x - ctr.x, s.z - ctr.z).length() < (sx.max(sz) as f32 * 0.25 + 3.0)) {
            continue;
        }
        for &(x, y, z, t) in icon.voxels {
            let (x, z) = (x as i32, z as i32);
            let (lx, lz) = match turns % 4 {
                0 => (x, z),
                1 => (z, size.x - 1 - x),
                2 => (size.x - 1 - x, size.z - 1 - z),
                _ => (size.z - 1 - z, x),
            };
            let v = origin + IVec3::new(lx, y as i32, lz);
            if get(c, v) == AIR {
                set(c, v, mats[(t as usize).min(3)]);
            }
        }
        n += 1;
    }
    n
}

// ------------------------------------------------------------------------------------------------
// Spawns, routes, info

/// Spawn targets (x, z, look-from height): bed, nightstand, desk, bookshelf, floor ×2, track, castle,
/// sock heap, rug.
const SPAWN_AT: [(f32, f32, f32); 10] = [
    (-45.0, -1.0, 32.0),
    (-23.0, 43.0, 32.0),
    (20.0, -41.0, 36.0),
    (-52.75, -44.0, 15.0),
    (26.0, 45.0, 12.0),
    (50.0, -25.0, 12.0),
    (10.0, TRACK_S, 12.0),
    (9.0, -8.0, 12.0),
    (50.0, -43.0, 24.0),
    (-14.0, -10.0, 12.0),
];

/// Feet position standing on breakable ground at (x, z) (first solid at or below `from_y`), with a
/// 1.5 m footprint fully supported and 3 m of head room. Like the city kit's check, but wooden
/// floors and furniture tops are fine here (the kit treats Wood as tree foliage).
fn spawn_ok(w: &VoxelWorld, x: f32, z: f32, from_y: f32) -> Option<Vec3> {
    let (cx, cz) = (vx(x), vx(z));
    let top = (0..=vx(from_y)).rev().find(|&y| w.get(IVec3::new(cx, y, cz)) != AIR)?;
    if crate::material::is_indestructible(w.get(IVec3::new(cx, top, cz))) {
        return None;
    }
    for dz in -1..=1 {
        for dx in -1..=1 {
            if (1..=6).any(|dy| w.get(IVec3::new(cx + dx, top + dy, cz + dz)) != AIR) || w.get(IVec3::new(cx + dx, top, cz + dz)) == AIR {
                return None;
            }
        }
    }
    Some(Vec3::new((cx as f32 + 0.5) * 0.5, (top + 1) as f32 * 0.5, (cz as f32 + 0.5) * 0.5))
}

fn pick_spawns(c: &City) -> Vec<Vec3> {
    let mut out: Vec<Vec3> = Vec::new();
    for &(x, z, from) in &SPAWN_AT {
        'rings: for ring in 0..10 {
            let steps = if ring == 0 { 1 } else { 8 };
            for k in 0..steps {
                let a = TAU * k as f32 / steps as f32;
                let (px, pz) = (x + dcos(a) * ring as f32 * 0.75, z + dsin(a) * ring as f32 * 0.75);
                if let Some(s) = spawn_ok(&c.w, px, pz, from)
                    && s.y < from
                    && out.iter().all(|t| t.distance(s) >= 12.0)
                {
                    out.push(s);
                    break 'rings;
                }
            }
        }
    }
    out
}

fn routes() -> Vec<Route> {
    let f = FLOOR_Y + 0.5;
    let mut out = Vec::new();
    // The track loop, counter-clockwise from the south straight (lane centre, kicker heights).
    let wp: Vec<Vec3> = track_loop().iter().map(|(p, _)| Vec3::new(p.x, if (p.x - TRACK_W).abs() < 0.01 { kicker_height(p.y).unwrap_or(f) } else { f }, p.y)).collect();
    out.push(Route { name: "track_loop", from: "track", to: "track", kind: RouteKind::Ground, waypoints: wp, anchors: vec![] });
    out.push(Route {
        name: "desk_ramp",
        from: "rug",
        to: "desk",
        kind: RouteKind::Ground,
        waypoints: vec![Vec3::new(RAMP_X, f, 18.0), Vec3::new(RAMP_X, f, 10.0), Vec3::new(RAMP_X, DESK_TOP + 0.5, DESK.z1), Vec3::new(RAMP_X - 6.0, DESK_TOP, -44.0)],
        anchors: vec![],
    });
    let bx0 = BOOKSHELF.x1 - 1.5;
    let bz = (BOOKSHELF.z0 + BOOKSHELF.z1) * 0.5;
    let mut climb = vec![Vec3::new(BOOKSHELF.x1 + 4.0, FLOOR_Y, bz)];
    climb.extend(SHELVES.iter().map(|&s| Vec3::new(bx0, s, bz)));
    climb.push(Vec3::new(bx0, BOOKSHELF_TOP, bz));
    out.push(Route {
        name: "bookshelf_climb",
        from: "floor",
        to: "bookshelf",
        kind: RouteKind::Mixed,
        waypoints: climb,
        anchors: SHELVES.iter().map(|&s| Vec3::new(BOOKSHELF.x1 - 0.25, s - 0.75, bz)).chain([Vec3::new(BOOKSHELF.x1, CEIL_Y, bz)]).collect(),
    });
    out.push(Route {
        name: "curtain_swing",
        from: "window",
        to: "window",
        kind: RouteKind::Swing,
        waypoints: vec![Vec3::new(HX - 3.0, SILL_Y, WINDOW_Z.0 - 0.5), Vec3::new(HX - 3.0, SILL_Y, WINDOW_Z.1 + 0.5)],
        anchors: vec![Vec3::new(HX - 2.5, ROD_Y, 0.0)],
    });
    let sl = surf_lines()[0];
    out.push(Route {
        name: "blanket_surf",
        from: "bed",
        to: "floor",
        kind: RouteKind::Mixed,
        waypoints: vec![Vec3::new(sl.a.x, MATTRESS_TOP + 1.0, BED.z0 + 4.0), Vec3::new(sl.b.x, FLOOR_Y, BLANKET.z0 - 6.0)],
        anchors: vec![],
    });
    out.push(Route {
        name: "bed_to_desk_swing",
        from: "bed",
        to: "desk",
        kind: RouteKind::Swing,
        waypoints: vec![Vec3::new(-40.0, MATTRESS_TOP + 1.0, -2.0), Vec3::new(12.0, DESK_TOP, -40.0)],
        anchors: vec![Vec3::new(-14.0, CEIL_Y, -22.0)],
    });
    out
}

/// Build the Giant's Room. Deterministic for a given seed (the layout is fixed; the seed varies
/// books, folds, tufts, socks and colours).
pub fn generate_giant(seed: u64) -> (VoxelWorld, MapInfo) {
    let (c, spawns) = build(seed);
    let (world, pruned) = c.finish_counted();
    info(world, spawns, pruned)
}

/// Everything before pruning: the builder and the spawns.
fn build(seed: u64) -> (City, Vec<Vec3>) {
    let s32 = (seed as u32) ^ 0x0917_a470;
    let mut w = VoxelWorld::centered(GIANT_SIZE_VOXELS);
    w.set_cull_outside(true);
    let mut c = City::new(w, seed ^ 0x6769_616e_74);
    build_shell(&mut c, s32);
    build_rug(&mut c, s32);
    build_bed(&mut c, s32);
    build_blanket(&mut c, s32);
    build_bookshelf(&mut c, s32);
    build_desk(&mut c, s32);
    build_toy_box(&mut c, s32);
    build_vent(&mut c);
    build_socks(&mut c, s32);
    build_track(&mut c);
    build_castle(&mut c, s32);
    build_cat(&mut c);
    build_scatter(&mut c, s32);
    build_curtains(&mut c, s32);
    build_fan(&mut c);
    rug_tufts(&mut c, s32);
    let spawns = pick_spawns(&c);
    let _toys = place_toys(&mut c, &spawns);
    // Hand-made toys (the icon library's procedural toys don't read at this size).
    teddy(&mut c, Vec3::new(-57.0, BOOKSHELF_TOP, -38.0));
    ball(&mut c, 48.0, FLOOR_Y, 10.0, 3.0, [mat::PLASTIC_RED, mat::PLASTIC_YELLOW, mat::PLASTIC_BLUE]);
    ball(&mut c, -27.0, FLOOR_Y, -46.0, 1.2, [mat::PLASTIC_GREEN, mat::PAINT_WHITE, mat::PLASTIC_GREEN]);
    toy_car(&mut c, 24.0, FLOOR_Y + 0.5, 14.0, false, mat::PLASTIC_BLUE);
    toy_car(&mut c, -35.0, FLOOR_Y, 20.0, true, mat::PLASTIC_RED);
    toy_car(&mut c, 26.0, DESK_TOP, -36.5, true, mat::PLASTIC_GREEN);
    (c, spawns)
}

/// Debug / tests: solid voxels generation left unconnected to anything indestructible (pruned).
pub fn debug_orphans(seed: u64) -> Vec<(IVec3, Material)> {
    let (c, _) = build(seed);
    let w = &c.w;
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let size = hi - lo;
    let idx = |v: IVec3| {
        let r = v - lo;
        (r.x + size.x * (r.z + size.z * r.y)) as usize
    };
    let mut seen = vec![false; (size.x * size.y * size.z) as usize];
    let mut stack = Vec::new();
    let all = || (lo.y..hi.y).flat_map(move |y| (lo.z..hi.z).flat_map(move |z| (lo.x..hi.x).map(move |x| IVec3::new(x, y, z))));
    for v in all() {
        if crate::material::is_indestructible(w.get(v)) {
            seen[idx(v)] = true;
            stack.push(v);
        }
    }
    while let Some(v) = stack.pop() {
        for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
            let n = v + d;
            if w.in_bounds(n) && !seen[idx(n)] && w.get(n) != AIR {
                seen[idx(n)] = true;
                stack.push(n);
            }
        }
    }
    all().filter(|&v| w.get(v) != AIR && !seen[idx(v)]).map(|v| (v, w.get(v))).collect()
}

fn info(world: VoxelWorld, spawns: Vec<Vec3>, pruned: usize) -> (VoxelWorld, MapInfo) {
    let wb = world.bounds();
    let center = Vec3::new(RUG.center().x, FLOOR_Y + 0.5, RUG.center().y);
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb { min: Vec3::new(wb.min.x - 25.0, VOID_Y, wb.min.z - 25.0), max: Vec3::new(wb.max.x + 25.0, wb.max.y + 40.0, wb.max.z + 25.0) },
        void_y: VOID_Y,
        center,
        sun_dir: sun_dir(),
        sky_zenith: [0.30, 0.52, 0.88],
        sky_horizon: [0.80, 0.86, 0.92],
        fog_color: [0.86, 0.80, 0.68],
        ambient: 0.5,
        gravity: None,
        surf_lines: surf_lines(),
        water_sheets: Vec::new(),
        water_walk: false,
        interactables: Default::default(),
        slipstream: None,
        mechanics: Default::default(),
    };
    let d = |name: &'static str, a: Area, y: f32| District { name, center: Vec3::new(a.center().x, y, a.center().y), area: [a.x0, a.z0, a.x1, a.z1] };
    let districts = vec![
        d("bed", BED, MATTRESS_TOP + 1.0),
        d("bookshelf", BOOKSHELF, SHELVES[0]),
        d("desk", DESK, DESK_TOP),
        d("toy_box", TOY_BOX, TOY_BOX_RIM),
        d("castle", CASTLE, FLOOR_Y + 0.5),
        d("rug", RUG, FLOOR_Y + 0.5),
        d("window", Area::new(HX - 6.0, WINDOW_Z.0, HX, WINDOW_Z.1), SILL_Y),
        d("track", Area::new(TRACK_W - 2.0, TRACK_S - 2.0, TRACK_E + 2.0, TRACK_N + 2.0), FLOOR_Y + 0.5),
    ];
    let info = MapInfo {
        name: "Giant's Room",
        arena,
        water_y: -1000.0,
        water_color: [0.3, 0.6, 0.8],
        water_deep: [0.1, 0.3, 0.5],
        sun_color: [1.0, 0.86, 0.64],
        sky_mid: [0.58, 0.72, 0.90],
        fog_start: 20.0,
        fog_end: 300.0,
        neon_boost: 0.8,
        districts,
        routes: routes(),
        pruned_voxels: pruned,
    };
    (world, info)
}
