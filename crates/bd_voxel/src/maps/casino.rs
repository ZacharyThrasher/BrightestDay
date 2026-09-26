//! **High Roller** — a Vegas casino on top of a tower (DESIGN.md "Maps"; feature `casino`).
//!
//! Layout (metres; +X east, +Z north, Y up). The building is 104 × 92 m; its hall floor is at
//! [`FLOOR_Y`] = 40 m, 10 m over the void line ([`VOID_Y`]); a neon skyline glows far below.
//!
//! ```text
//!   roof 59 m:  pool deck (E) · glass atrium dome over the wheel · rooftop bar (W) · HIGH ROLLER sign (N)
//!   hall 40–58: ┌──────────── N balcony 49 m (slots / tables under it) ─────────────┐
//!               │ W balcony │           ○ chandeliers (ring r 24, hubs 54 m)   777 │
//!               │ grand ═══ │        ◎ GIANT ROULETTE (felt r ≤ 27.5, 41 m)    $$$ │ cashier
//!               │ stair     │   rim · banked track · 38 numbered pockets · cone   │
//!               └──────────── S balcony 49 m ─────────────────────────────────────┘
//! ```
//!
//! - **The wheel is the arena floor**: a raised felt table (step ring at 40.5, felt at 41), a gold
//!   rim (41.5), the banked ball track terracing down (41 → 38.5), 38 red / black / green pockets at
//!   38 m with 0.5 m gold frets (walkable), the cone rising to 43 m and the indestructible spindle
//!   cross (tether anchors, a small exposed perch). A hard-light **surf halo** (24 ridge segments,
//!   ridge 47 m, r 19.5) floats over the track — never blocks walking, surf laps from the chip-stack
//!   steps on the felt.
//! - **Hall**: U-shaped mezzanine (N / S / W, 49 m) on marble columns, a grand staircase (W),
//!   stairs from the NE / SW balcony corners up through roof hatches, crystal chandeliers (tether
//!   anchors; hubs + chains indestructible, crystals breakable), slot rows, card / craps tables,
//!   giant chip stacks and dice, the cashier cage, neon suits and a "777" wall sign.
//! - **Gambling (optional)**: four big indestructible "high roller" slot machines (floor E / W,
//!   balconies N / S) and the roulette house spin — declared in `ArenaInfo::interactables`, run by
//!   `bd_sim::world::interactables`.
//! - **KOs**: the facade is indestructible frame + big breakable windows (both storeys) — a hard hit
//!   out of the building is a side / void KO (tether save applies); off the roof edge (1 m parapet)
//!   too; up through the breakable roof / atrium glass is a star KO. The hall floor sits on an
//!   indestructible plate (no void holes inside).
//! - **Destruction**: tables, chip stacks, decor slots, glass, crystals, rails, carpet, roof slab
//!   break; walls, columns, frames, the spindle and the big slot machines don't.
//! - **Light**: warm gold + neon emissive voxels only (no point lights).

#![allow(clippy::too_many_arguments)]

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, SurfLine};
use crate::city::City;
use crate::interact::{AMERICAN_WHEEL, Interactables, RouletteWheel, SlotMachine};
use crate::material::mat;
use crate::noise::{Rng, TAU, datan2, dcos, dsin};
use crate::world::VoxelWorld;

#[path = "casino_props.rs"]
pub mod props;

use props::{Frame, chandelier, chip_stack, column, cylinder, die, slot_big, slot_decor};

/// World dimensions in voxels: 176 × 84 × 176 m.
pub const CASINO_SIZE_VOXELS: IVec3 = IVec3::new(352, 168, 352);
/// Falling below this is a void KO (10 m under the hall floor).
pub const VOID_Y: f32 = 30.0;
/// Hall floor (top of the carpet).
pub const FLOOR_Y: f32 = 40.0;
/// Mezzanine / balcony floor.
pub const MEZZ_Y: f32 = 49.0;
/// Hall ceiling (underside of the roof slab) and the roof deck.
pub const CEIL_Y: f32 = 58.0;
pub const ROOF_Y: f32 = 59.0;
/// Interior half-extents of the hall; walls are 1 m thick outside them.
pub const HALL_X: f32 = 51.0;
pub const HALL_Z: f32 = 45.0;
/// Roulette table: felt top, felt edge, the step ring outside it.
pub const FELT_Y: f32 = 41.0;
pub const FELT_R: f32 = 27.5;
pub const STEP_R: f32 = 28.5;
/// Wheel radii: rim, ball track, pockets, cone, spindle base.
pub const RIM_R: f32 = 22.0;
pub const TRACK_R: f32 = 17.0;
pub const POCKET_R: f32 = 9.0;
pub const CONE_R: f32 = 2.5;
pub const POCKET_Y: f32 = 38.0;
/// The surf halo over the track.
pub const HALO_R: f32 = 19.5;
pub const HALO_Y: f32 = 47.0;
pub const HALO_SEGMENTS: usize = 24;
/// Glass atrium over the wheel (roof opening radius).
pub const ATRIUM_R: f32 = 12.0;
/// Chandelier ring (six over the felt edge) and hub height.
pub const CHANDELIER_R: f32 = 24.0;
pub const HUB_Y: f32 = 54.0;
/// Balconies (x0, z0, x1, z1).
pub const BALCONY_N: [f32; 4] = [-HALL_X, 36.0, HALL_X, HALL_Z];
pub const BALCONY_S: [f32; 4] = [-HALL_X, -HALL_Z, HALL_X, -36.0];
pub const BALCONY_W: [f32; 4] = [-HALL_X, -36.0, -43.0, 36.0];
/// Grand staircase footprint (x0, z0, x1, z1): rises west from the floor to the W balcony.
pub const GRAND_STAIR: [f32; 4] = [-43.0, -5.0, -30.0, 5.0];
/// The pool on the roof (x0, z0, x1, z1).
pub const POOL: [f32; 4] = [20.0, -12.0, 42.0, 12.0];

const VS: f32 = 0.5;

#[inline]
fn vi(m: f32) -> i32 {
    (m / VS).floor() as i32
}

fn in_rect(r: [f32; 4], x: f32, z: f32) -> bool {
    x >= r[0] && x < r[2] && z >= r[1] && z < r[3]
}

/// Is (x, z) over a balcony slab?
pub fn in_balcony(x: f32, z: f32) -> bool {
    in_rect(BALCONY_N, x, z) || in_rect(BALCONY_S, x, z) || in_rect(BALCONY_W, x, z)
}

/// The garish carpet: crimson field, gold diamond lattice, teal medallions.
fn carpet(x: i32, z: i32, alt: bool) -> Material {
    let n = if alt { 12 } else { 16 };
    let (u, v) = (x.rem_euclid(n), z.rem_euclid(n));
    let h = n / 2;
    let (du, dv) = ((u - h).abs(), (v - h).abs());
    let s = du + dv;
    if s == h - 1 {
        mat::CASINO_CARPET_GOLD
    } else if s <= 1 {
        mat::CASINO_CARPET_TEAL
    } else if s == 3 && (du == 0 || dv == 0) {
        mat::CASINO_CARPET_GOLD
    } else if (du == h || dv == h) && (u + v) % 2 == 0 {
        if alt { mat::CASINO_CARPET_GOLD } else { mat::CASINO_CARPET_TEAL }
    } else {
        mat::CASINO_CARPET
    }
}

/// Hall ceiling underside: dark lacquer, gold coffer grid, crystal downlights, amber coves at walls.
fn ceiling(x: i32, z: i32) -> Material {
    let (px, pz) = ((x as f32 + 0.5) * VS, (z as f32 + 0.5) * VS);
    if px.abs() > HALL_X - 1.5 || pz.abs() > HALL_Z - 1.5 {
        return mat::NEON_AMBER;
    }
    let (u, v) = (x.rem_euclid(12), z.rem_euclid(12));
    if u == 0 || v == 0 {
        mat::GOLD_TRIM
    } else if (u == 6 || u == 5) && (v == 6 || v == 5) {
        mat::CRYSTAL_DROP
    } else {
        mat::ROULETTE_BLACK
    }
}

/// The wheel pocket at angle `a` (radians, like `atan2(z, x)`).
fn pocket_of(a: f32) -> usize {
    let n = AMERICAN_WHEEL.len();
    let t = (a - ZERO_ANGLE).rem_euclid(TAU) / TAU;
    ((t * n as f32).floor() as usize).min(n - 1)
}

/// Pocket "0" is centred due east.
const ZERO_ANGLE: f32 = -TAU / 76.0;

/// Is voxel-centre (x, z) at angle `a`, radius `r`, part of the label painted in its pocket?
fn pocket_digit(r: f32, a: f32) -> bool {
    let n = AMERICAN_WHEEL.len() as f32;
    let k = pocket_of(a);
    let label = AMERICAN_WHEEL[k].0;
    let mid = ZERO_ANGLE + (k as f32 + 0.5) * TAU / n;
    let mut da = a - mid;
    while da > TAU * 0.5 {
        da -= TAU;
    }
    while da < -TAU * 0.5 {
        da += TAU;
    }
    // Tangential coordinate (m), centred; glyph columns 0..3 across 1.5 m.
    let u = da * r;
    let col = ((u + 0.75) / VS).floor() as i32;
    if !(0..3).contains(&col) {
        return false;
    }
    // Digits stacked along the radius, the first one outermost; rows 0..5 over 2.5 m, top outward.
    let digits: Vec<char> = label.chars().collect();
    let (r_top, pitch) = (16.25f32, 3.0f32);
    for (i, d) in digits.iter().enumerate() {
        let top = r_top - pitch * i as f32;
        let row = ((top - r) / VS).floor() as i32;
        if (0..5).contains(&row) {
            let bits = props::digit_bits(*d)[row as usize];
            return bits & (4 >> col) != 0;
        }
    }
    false
}

/// Build the table + wheel columns (everything within [`STEP_R`] of the centre).
fn build_wheel(c: &mut City) {
    let r_max = vi(STEP_R) + 1;
    let base = vi(37.0);
    let top_clear = vi(FLOOR_Y + 1.0);
    for z in -r_max..=r_max {
        for x in -r_max..=r_max {
            let (px, pz) = ((x as f32 + 0.5) * VS, (z as f32 + 0.5) * VS);
            let r = (px * px + pz * pz).sqrt();
            if r >= STEP_R {
                continue;
            }
            let a = datan2(pz, px);
            // (surface height, top material, body material)
            let (surf, top, body): (f32, Material, Material) = if r >= FELT_R {
                (FLOOR_Y + 0.5, mat::GOLD_TRIM, mat::DARK_WOOD)
            } else if r >= RIM_R + 1.0 {
                // Felt; a gold pinstripe and the betting layouts (N and S).
                let stripe = (26.25..26.75).contains(&r);
                let layout = px.abs() < 9.0 && (23.75..27.0).contains(&pz.abs()) && (x.rem_euclid(3) == 0 || (pz.abs() - 25.4).abs() < 0.26);
                (FELT_Y, if stripe || layout { mat::GOLD_TRIM } else { mat::FELT_GREEN }, mat::DARK_WOOD)
            } else if r >= RIM_R {
                (FELT_Y + 0.5, mat::GOLD_TRIM, mat::DARK_WOOD)
            } else if r >= TRACK_R {
                // The banked ball track: six 0.5 m terraces from 41 down to 38.5.
                let step = ((RIM_R - r) / (5.0 / 6.0)).floor().min(5.0);
                let h = FELT_Y - step * 0.5;
                // Eight gold deflector diamonds (small bumps).
                let da = (a / (TAU / 8.0) - 0.5).rem_euclid(1.0);
                let bump = (da - 0.5).abs() * (TAU / 8.0) * r < 0.5 && (r - 19.5).abs() < 0.5;
                (if bump { h + 0.5 } else { h }, if bump { mat::GOLD_TRIM } else { mat::WOOD }, mat::DARK_WOOD)
            } else if r >= POCKET_R {
                // Pockets: frets on the boundaries (0.5 m gold ridges), an inner ring, labels.
                let n = AMERICAN_WHEEL.len() as f32;
                let t = ((a - ZERO_ANGLE) / (TAU / n)).rem_euclid(1.0);
                let fret = t.min(1.0 - t) * (TAU / n) * r < 0.3;
                if fret || r < POCKET_R + 0.5 {
                    (POCKET_Y + 0.5, mat::GOLD_TRIM, mat::DARK_WOOD)
                } else if pocket_digit(r, a) {
                    (POCKET_Y, mat::CHIP_WHITE, mat::DARK_WOOD)
                } else {
                    let col = match AMERICAN_WHEEL[pocket_of(a)].1 {
                        0 => mat::ROULETTE_GREEN,
                        1 => mat::ROULETTE_RED,
                        _ => mat::ROULETTE_BLACK,
                    };
                    (POCKET_Y, col, mat::DARK_WOOD)
                }
            } else if r >= CONE_R {
                // The cone: 0.5 m terraces every 0.72 m up to 43 m, eight gold spokes.
                let h = POCKET_Y + ((POCKET_R - r) / 0.72).ceil().max(1.0) * 0.5;
                let da = (a / (TAU / 8.0)).rem_euclid(1.0);
                let spoke = da.min(1.0 - da) * (TAU / 8.0) * r < 0.26;
                (h, if spoke { mat::GOLD_TRIM } else { mat::DARK_WOOD }, mat::DARK_WOOD)
            } else {
                (POCKET_Y + 5.0, mat::SLOT_TRIM, mat::CASINO_PILLAR)
            };
            let ty = vi(surf) - 1;
            c.w.fill_column(x, z, base, top_clear, AIR);
            c.w.fill_column(x, z, base, ty, body);
            c.w.set(IVec3::new(x, ty, z), top);
        }
    }
    // Spindle + cross handle + knobs (indestructible gold: anchors, a small exposed perch).
    let s0 = POCKET_Y + 5.0;
    cylinder(c, 0.0, 0.0, 1.0, s0, s0 + 3.5, |_, _, _| Some(mat::SLOT_TRIM));
    c.fill(Vec3::new(-4.5, s0 + 2.5, -0.25), Vec3::new(4.5, s0 + 3.0, 0.25), mat::SLOT_TRIM);
    c.fill(Vec3::new(-0.25, s0 + 2.5, -4.5), Vec3::new(0.25, s0 + 3.0, 4.5), mat::SLOT_TRIM);
    for (dx, dz) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        c.w.fill_sphere(Vec3::new(dx * 4.5, s0 + 2.75, dz * 4.5), 0.75, mat::SLOT_TRIM, false);
    }
    c.w.fill_sphere(Vec3::new(0.0, s0 + 4.0, 0.0), 0.75, mat::SLOT_TRIM, false);
}

/// Exterior shell: indestructible walls with breakable window bands, the parapet, the base plate
/// and the tower core below.
fn build_shell(c: &mut City) {
    let (ox, oz) = (HALL_X + 1.0, HALL_Z + 1.0);
    // Tower core below the casino (indestructible, lit window stripes).
    c.fill_with(Vec3::new(-12.0, 0.0, -12.0), Vec3::new(12.0, 36.5, 12.0), |p, v, _| {
        let outer = p.x.abs() > 11.5 || p.z.abs() > 11.5;
        Some(if outer && v.y.rem_euclid(6) == 2 && (v.x + v.z).rem_euclid(4) != 0 { mat::SLOT_TRIM } else { mat::CASINO_WALL })
    });
    // Base plate (indestructible) + floor fill + carpet.
    c.fill(Vec3::new(-ox, 36.5, -oz), Vec3::new(ox, 37.0, oz), mat::TOWER_CORE);
    c.fill(Vec3::new(-HALL_X, 37.0, -HALL_Z), Vec3::new(HALL_X, FLOOR_Y - VS, HALL_Z), mat::CONCRETE);
    c.fill_with(Vec3::new(-HALL_X, FLOOR_Y - VS, -HALL_Z), Vec3::new(HALL_X, FLOOR_Y, HALL_Z), |_, v, _| Some(carpet(v.x, v.z, false)));
    for k in (-3..=3).map(|k| k as f32 * 16.0) {
        for m in (-2..=2).map(|m| m as f32 * 16.0) {
            c.anchor(IVec3::new(vi(k), vi(36.75), vi(m)));
        }
    }
    // Walls: posts every 8 m, window bands on both storeys, a lip over the roof.
    c.fill_with(Vec3::new(-ox, 37.0, -oz), Vec3::new(ox, ROOF_Y + 1.0, oz), |p, _, _| {
        let inside = p.x.abs() < HALL_X && p.z.abs() < HALL_Z;
        if inside {
            return None;
        }
        let outer = p.x.abs() > HALL_X + 0.5 || p.z.abs() > HALL_Z + 0.5;
        let along = if p.x.abs() >= HALL_X { p.z } else { p.x };
        let post = (along + 200.0).rem_euclid(8.0) < 1.5 || (p.x.abs() >= HALL_X && p.z.abs() >= HALL_Z);
        let band = (41.0..47.5).contains(&p.y) || (50.0..57.0).contains(&p.y);
        if band && !post {
            return Some(if outer { mat::GLASS_BLUE } else { AIR });
        }
        if outer && (58.0..58.5).contains(&p.y) {
            return Some(mat::NEON_PINK);
        }
        if outer && post && band && (p.y - 44.0).abs() < 0.3 {
            return Some(mat::SLOT_TRIM);
        }
        Some(mat::CASINO_WALL)
    });
}

fn build_slabs(c: &mut City) {
    // Balconies: carpet on top, dark wood under, a gold + neon fascia on the open edge.
    for b in [BALCONY_N, BALCONY_S, BALCONY_W] {
        c.fill_with(Vec3::new(b[0], MEZZ_Y - 1.0, b[1]), Vec3::new(b[2], MEZZ_Y, b[3]), |p, v, _| {
            Some(if p.y > MEZZ_Y - VS { carpet(v.x, v.z, true) } else { mat::DARK_WOOD })
        });
    }
    // Fascia: the slab's open edges (N/S face the hall along z = ±36; W along x = −43).
    c.fill_with(Vec3::new(-HALL_X, MEZZ_Y - 1.5, -36.5), Vec3::new(HALL_X, MEZZ_Y - 0.5, 36.5), |p, v, old| {
        let edge_ns = (p.z.abs() - 36.25).abs() < 0.26 && p.x >= -43.5;
        let edge_w = (p.x + 43.25).abs() < 0.26 && p.z.abs() < 36.0;
        if !(edge_ns || edge_w) {
            return None;
        }
        if p.y < MEZZ_Y - 1.0 {
            return Some(mat::GOLD_TRIM);
        }
        let seg = (v.x + v.z).div_euclid(8).rem_euclid(3);
        let _ = old;
        Some([mat::NEON_PINK, mat::NEON_CYAN, mat::NEON_AMBER][seg as usize])
    });
    // Roof slab: ceiling pattern under, teak deck on top; the atrium and stair hatches open.
    c.fill_with(Vec3::new(-HALL_X, CEIL_Y, -HALL_Z), Vec3::new(HALL_X, ROOF_Y, HALL_Z), |p, v, _| {
        if p.x * p.x + p.z * p.z < ATRIUM_R * ATRIUM_R || roof_hatch(p.x, p.z) {
            return None;
        }
        Some(if p.y < CEIL_Y + VS { ceiling(v.x, v.z) } else { deck(v.x, v.z) })
    });
}

/// Roof deck: big travertine tiles (stone: spawnable, unlike wood decking).
fn deck(x: i32, z: i32) -> Material {
    if (x.div_euclid(4) + z.div_euclid(4)).rem_euclid(2) == 0 { mat::MARBLE } else { mat::MARBLE_VEIN }
}

/// Roof openings over the balcony stairs (NE and SW).
fn roof_hatch(x: f32, z: f32) -> bool {
    (x >= 41.0 && x < 49.0 && z >= 39.5 && z < 43.5) || (x >= -49.0 && x < -41.0 && z >= -43.5 && z < -39.5)
}

fn build_stairs(c: &mut City) {
    // Grand staircase: floor → W balcony, marble with a red runner and gold banisters.
    let (x0, z0, x1, z1) = (GRAND_STAIR[0], GRAND_STAIR[1], GRAND_STAIR[2], GRAND_STAIR[3]);
    let steps = ((MEZZ_Y - FLOOR_Y) / VS) as i32;
    let run = (x1 - x0) / steps as f32;
    c.stairs(Vec3::new(x1, FLOOR_Y, (z0 + z1) * 0.5), Vec2::new(-1.0, 0.0), MEZZ_Y, z1 - z0, run, mat::MARBLE);
    // Runner: recolour the top voxel of each step in the middle 5 m.
    for x in vi(x0)..vi(x1) {
        for z in vi(-2.5)..vi(2.5) {
            if let Some(ty) = (vi(FLOOR_Y)..vi(MEZZ_Y + 0.5)).rev().find(|&y| c.w.get(IVec3::new(x, y, z)) != AIR) {
                c.w.set(IVec3::new(x, ty, z), mat::VELVET);
            }
        }
    }
    // Banisters: a gold rail 1 m above the steps along both sides.
    for side in [z0 + 0.25, z1 - 0.25] {
        for k in 0..steps {
            let x = x1 - (k as f32 + 0.5) * run;
            let y = FLOOR_Y + (k + 1) as f32 * VS;
            c.fill(Vec3::new(x - run * 0.5, y, side - 0.25), Vec3::new(x + run * 0.5, y + 1.0, side + 0.25), mat::GOLD_TRIM);
        }
    }
    // Balcony → roof: NE (rising east) and SW (rising west), 4 m wide.
    c.stairs(Vec3::new(34.0, MEZZ_Y, 41.5), Vec2::new(1.0, 0.0), ROOF_Y, 4.0, 0.75, mat::MARBLE);
    c.stairs(Vec3::new(-34.0, MEZZ_Y, -41.5), Vec2::new(-1.0, 0.0), ROOF_Y, 4.0, 0.75, mat::MARBLE);
}

fn build_columns_and_rails(c: &mut City) {
    let xs = [-36.0f32, -24.0, -12.0, 0.0, 12.0, 24.0, 36.0];
    for &x in &xs {
        column(c, x, 36.5, FLOOR_Y, CEIL_Y);
        column(c, -x, -36.5, FLOOR_Y, CEIL_Y);
    }
    for &z in &[-26.0f32, -14.0, 14.0, 26.0] {
        column(c, -43.75, z, FLOOR_Y, CEIL_Y);
    }
    // Open gold balustrade (posts every 2 m, a rail at 1 m) along the open balcony edges — see-
    // through and hop-over-able; not at the grand staircase mouth.
    c.fill_with(Vec3::new(-HALL_X, MEZZ_Y, -36.5), Vec3::new(HALL_X, MEZZ_Y + 1.0, 36.5), |p, v, old| {
        if old != AIR {
            return None;
        }
        let edge_ns = (p.z.abs() - 36.25).abs() < 0.26 && p.x >= -43.5;
        let edge_w = (p.x + 43.25).abs() < 0.26 && p.z.abs() < 36.0 && p.z.abs() > 5.5;
        if !(edge_ns || edge_w) {
            return None;
        }
        let along = if edge_ns { v.x } else { v.z };
        (p.y > MEZZ_Y + VS || along.rem_euclid(4) == 0).then_some(mat::GOLD_TRIM)
    });
}

fn build_chandeliers(c: &mut City) {
    for k in 0..6 {
        let a = k as f32 * TAU / 6.0;
        chandelier(c, Vec3::new(dcos(a) * CHANDELIER_R, CEIL_Y, dsin(a) * CHANDELIER_R), HUB_Y, 2.6);
    }
    for (x, z) in [(40.0f32, 22.0f32), (-40.0, -22.0), (40.0, -30.0), (-40.0, 30.0)] {
        chandelier(c, Vec3::new(x, CEIL_Y, z), HUB_Y + 0.5, 1.9);
    }
}

fn build_atrium(c: &mut City) {
    let (rx, ry) = (ATRIUM_R + 0.5, 7.0f32);
    c.fill_with(Vec3::new(-rx - 1.0, ROOF_Y - 1.0, -rx - 1.0), Vec3::new(rx + 1.0, ROOF_Y + ry + 0.5, rx + 1.0), |p, _, _| {
        let h = (p.x * p.x + p.z * p.z).sqrt();
        if p.y < ROOF_Y {
            // Gold ring at the opening's edge (in the roof slab).
            return (h >= ATRIUM_R - 0.5 && h < ATRIUM_R + 0.5).then_some(mat::SLOT_TRIM);
        }
        let dy = p.y - ROOF_Y;
        let q = (h / rx) * (h / rx) + (dy / ry) * (dy / ry);
        let qi = (h / (rx - 0.6)) * (h / (rx - 0.6)) + (dy / (ry - 0.6)) * (dy / (ry - 0.6));
        if q > 1.0 || qi <= 1.0 {
            return None;
        }
        let a = datan2(p.z, p.x);
        let da = (a / (TAU / 8.0)).rem_euclid(1.0);
        let rib = da.min(1.0 - da) * (TAU / 8.0) * h.max(0.5) < 0.3 || h < 1.5 || dy < 0.5;
        Some(if rib { mat::SLOT_TRIM } else { mat::GLASS_SUNSET })
    });
}

/// Mirror a point through the centre (the hall is 180° symmetric where it matters).
fn rot(p: Vec2) -> Vec2 {
    -p
}

fn build_floor_props(c: &mut City, slots: &mut Vec<SlotMachine>) {
    // Decor slot rows under the N / S balconies (facing the hall), groups of four.
    let mut style = 0u32;
    for gx in [-36.0f32, -12.0, 12.0, 30.0] {
        for k in 0..4 {
            let x = gx + k as f32 * 1.5;
            slot_decor(c, Frame::new(Vec3::new(x, FLOOR_Y, 43.5), Vec2::new(0.0, -1.0)), style);
            slot_decor(c, Frame::new(Vec3::new(-x, FLOOR_Y, -43.5), Vec2::new(0.0, 1.0)), style + 7);
            style += 3;
        }
    }
    // Back-to-back slot islands in the E / W zones.
    for k in 0..3 {
        let z = 14.0 + k as f32 * 1.5;
        slot_decor(c, Frame::new(Vec3::new(37.5, FLOOR_Y, z), Vec2::new(1.0, 0.0)), style);
        slot_decor(c, Frame::new(Vec3::new(36.5, FLOOR_Y, z), Vec2::new(-1.0, 0.0)), style + 1);
        slot_decor(c, Frame::new(Vec3::new(-37.5, FLOOR_Y, -z), Vec2::new(-1.0, 0.0)), style + 2);
        slot_decor(c, Frame::new(Vec3::new(-36.5, FLOOR_Y, -z), Vec2::new(1.0, 0.0)), style + 3);
        style += 5;
    }
    // Tables under the balconies: blackjack · craps · blackjack (and the mirror).
    for s in [1.0f32, -1.0] {
        props::card_table(c, Frame::new(Vec3::new(-24.0 * s, FLOOR_Y, 40.5 * s), Vec2::new(0.0, -s)));
        props::card_table(c, Frame::new(Vec3::new(24.0 * s, FLOOR_Y, 40.5 * s), Vec2::new(0.0, -s)));
        props::craps_table(c, Frame::new(Vec3::new(0.0, FLOOR_Y, 38.5 * s), Vec2::new(0.0, -s)));
    }
    props::card_table(c, Frame::new(Vec3::new(38.0, FLOOR_Y, -6.0), Vec2::new(-1.0, 0.0)));
    props::card_table(c, Frame::new(Vec3::new(-38.0, FLOOR_Y, 10.0), Vec2::new(1.0, 0.0)));
    // The big machines: floor E / W.
    slots.push(slot_big(c, Frame::new(Vec3::new(48.0, FLOOR_Y, -20.0), Vec2::new(-1.0, 0.0))));
    slots.push(slot_big(c, Frame::new(Vec3::new(-48.0, FLOOR_Y, 20.0), Vec2::new(1.0, 0.0))));
    // Cashier cage (E wall): marble counter, gold bars, chips behind.
    c.fill(Vec3::new(45.0, FLOOR_Y, -7.0), Vec3::new(46.0, FLOOR_Y + 1.5, 7.0), mat::MARBLE);
    c.fill(Vec3::new(45.0, FLOOR_Y + 1.5, -7.0), Vec3::new(46.0, FLOOR_Y + 2.0, 7.0), mat::GOLD_TRIM);
    c.fill(Vec3::new(45.0, FLOOR_Y + 5.0, -7.0), Vec3::new(51.0, FLOOR_Y + 5.5, 7.0), mat::DARK_WOOD);
    for k in 0..15 {
        let z = -7.0 + k as f32;
        c.fill(Vec3::new(45.25, FLOOR_Y + 2.0, z), Vec3::new(45.75, FLOOR_Y + 5.0, z + 0.5), mat::GOLD_TRIM);
    }
    for (k, z) in [-4.5f32, -1.5, 1.5, 4.5].iter().enumerate() {
        chip_stack(c, Vec3::new(49.0, FLOOR_Y, *z), 0.9, 3 + k as i32, &[mat::CHIP_BLACK, mat::CHIP_PURPLE, mat::PAINT_RED], 11 + k as u32);
    }
    // Giant dice piles and chip stacks on the floor (180° pairs).
    for (p, s) in [(Vec2::new(40.0, -24.0), 1u32), (rot(Vec2::new(40.0, -24.0)), 2)] {
        die(c, Vec3::new(p.x - 1.75, FLOOR_Y, p.y - 1.75), 7, 5, mat::DICE_RED, mat::CHIP_WHITE);
        die(c, Vec3::new(p.x - 1.0, FLOOR_Y + 3.5, p.y - 1.25), 5, (s + 1) as u8, mat::DICE_IVORY, mat::ROULETTE_BLACK);
        die(c, Vec3::new(p.x + 2.25, FLOOR_Y, p.y - 0.5), 5, (s + 3) as u8, mat::DICE_IVORY, mat::ROULETTE_BLACK);
    }
    let pal = [mat::PAINT_RED, mat::CHIP_BLACK, mat::CHIP_PURPLE, mat::PLASTIC_BLUE, mat::PLASTIC_GREEN];
    for (k, p) in [Vec2::new(33.0, -14.0), Vec2::new(45.0, 24.0), Vec2::new(31.0, 22.0)].iter().enumerate() {
        for (j, q) in [*p, rot(*p)].iter().enumerate() {
            chip_stack(c, Vec3::new(q.x, FLOOR_Y, q.y), 1.4, [3, 6, 4][k], &pal, (k * 2 + j) as u32 * 31 + 5);
        }
    }
    // Chip-stack steps on the felt: 1.5 → 3 → 4.5 m, up to the surf halo (four clusters).
    for q in 0..4 {
        let a = TAU / 8.0 + q as f32 * TAU / 4.0;
        for (i, (da, rr, n, cr)) in [(-0.10f32, 25.6f32, 3, 1.2f32), (0.0, 25.4, 6, 1.3), (0.11, 24.6, 9, 1.4)].iter().enumerate() {
            let b = a + da;
            chip_stack(c, Vec3::new(dcos(b) * rr, FELT_Y, dsin(b) * rr), *cr, *n, &pal, (q * 3 + i) as u32 * 17 + 3);
        }
    }
}

fn build_balcony_props(c: &mut City, slots: &mut Vec<SlotMachine>) {
    slots.push(slot_big(c, Frame::new(Vec3::new(-16.0, MEZZ_Y, 42.0), Vec2::new(0.0, -1.0))));
    slots.push(slot_big(c, Frame::new(Vec3::new(16.0, MEZZ_Y, -42.0), Vec2::new(0.0, 1.0))));
    // Velvet lounge booths + a few decor slots along the back walls.
    let mut style = 40u32;
    for s in [1.0f32, -1.0] {
        for x in [-40.0f32, -30.0, 4.0, 22.0] {
            c.fill(Vec3::new(x * s - 2.0, MEZZ_Y, 43.5 * s - 0.75), Vec3::new(x * s + 2.0, MEZZ_Y + 1.0, 43.5 * s + 0.75), mat::VELVET);
            c.fill(Vec3::new(x * s - 0.75, MEZZ_Y, 40.5 * s - 0.75), Vec3::new(x * s + 0.75, MEZZ_Y + 1.0, 40.5 * s + 0.75), mat::DARK_WOOD);
        }
        for k in 0..4 {
            let x = (-6.0 + k as f32 * 1.5) * s;
            slot_decor(c, Frame::new(Vec3::new(x, MEZZ_Y, 43.5 * s), Vec2::new(0.0, -s)), style);
            style += 3;
        }
    }
    // Neon card suits hanging under the balcony fascia, one per bay, facing the hall.
    for (k, x) in [-30.0f32, -6.0, 18.0].iter().enumerate() {
        for s in [1.0f32, -1.0] {
            let origin = Vec3::new(x * s - 1.75 * s, MEZZ_Y - 5.0, 36.0 * s);
            props::suit(c, k as u32 + if s < 0.0 { 2 } else { 0 }, origin, Vec2::new(s, 0.0), Vec2::new(0.0, -s), 0.5, [mat::NEON_PINK, mat::NEON_CYAN, mat::NEON_VIOLET][k]);
        }
    }
    // "777" over the cashier (E wall, facing the hall) + two suits.
    props::text(c, "777", Vec3::new(50.5, MEZZ_Y + 0.5, -8.5), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), 1.0, mat::NEON_AMBER);
    props::suit(c, 1, Vec3::new(50.5, MEZZ_Y + 2.0, -18.0), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), 0.5, mat::NEON_PINK);
    props::suit(c, 0, Vec3::new(50.5, MEZZ_Y + 2.0, 14.5), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), 0.5, mat::NEON_CYAN);
}

fn build_roof(c: &mut City) {
    let (x0, z0, x1, z1) = (POOL[0], POOL[1], POOL[2], POOL[3]);
    // Pool: water surface 0.5 m below the deck, coping, tiled surround.
    c.fill_with(Vec3::new(x0 - 3.0, CEIL_Y - VS, z0 - 3.0), Vec3::new(x1 + 3.0, ROOF_Y, z1 + 3.0), |p, v, old| {
        let inside = p.x >= x0 && p.x < x1 && p.z >= z0 && p.z < z1;
        if p.y < CEIL_Y {
            return inside.then(|| ceiling(v.x, v.z));
        }
        if inside {
            return Some(if p.y < CEIL_Y + VS { mat::POOL_WATER } else { AIR });
        }
        if old == AIR {
            return None;
        }
        let coping = p.x >= x0 - 0.5 && p.x < x1 + 0.5 && p.z >= z0 - 0.5 && p.z < z1 + 0.5;
        (p.y >= CEIL_Y + VS).then_some(if coping { mat::MARBLE } else { mat::POOL_TILE })
    });
    // Loungers + parasols along the long sides.
    for k in 0..7 {
        let x = x0 + 1.5 + k as f32 * 3.2;
        props::lounger(c, Frame::new(Vec3::new(x, ROOF_Y, z1 + 1.0), Vec2::new(0.0, -1.0)));
        props::lounger(c, Frame::new(Vec3::new(x, ROOF_Y, z0 - 1.0), Vec2::new(0.0, 1.0)));
        if k % 2 == 1 {
            props::parasol(c, Vec3::new(x + 1.6, ROOF_Y, z1 + 2.2), 1.8, [mat::NEON_PINK, mat::PLASTIC_BLUE][k % 2]);
            props::parasol(c, Vec3::new(x + 1.6, ROOF_Y, z0 - 2.2), 1.8, mat::CHIP_PURPLE);
        }
    }
    // Palms in planters at the pool deck corners.
    for (x, z) in [(x0 - 3.5, z1 + 5.0), (x1 + 3.5, z1 + 5.0), (x0 - 3.5, z0 - 5.0), (x1 + 3.5, z0 - 5.0)] {
        c.fill(Vec3::new(x - 1.0, ROOF_Y, z - 1.0), Vec3::new(x + 1.0, ROOF_Y + 1.0, z + 1.0), mat::MARBLE);
        c.fill(Vec3::new(x - 0.75, ROOF_Y + 0.5, z - 0.75), Vec3::new(x + 0.75, ROOF_Y + 1.0, z + 0.75), mat::DIRT);
        let mut spec = crate::city::PalmSpec::new(Vec3::new(x, ROOF_Y + 1.0, z), 7.0);
        spec.lean = Vec2::new(if x > 30.0 { 0.8 } else { -0.8 }, 0.0);
        c.palm(&spec);
    }
    // Rooftop bar (W): round marble counter under a gold canopy on four posts.
    let (bx, bz) = (-30.0f32, 0.0f32);
    cylinder(c, bx, bz, 4.0, ROOF_Y, ROOF_Y + 1.5, |d, _, _| (d > 3.0).then_some(mat::MARBLE));
    cylinder(c, bx, bz, 4.0, ROOF_Y + 1.5, ROOF_Y + 2.0, |d, _, _| (d > 3.0).then_some(mat::GOLD_TRIM));
    for (dx, dz) in [(4.5f32, 4.5f32), (-4.5, 4.5), (4.5, -4.5), (-4.5, -4.5)] {
        c.fill(Vec3::new(bx + dx - 0.25, ROOF_Y, bz + dz - 0.25), Vec3::new(bx + dx + 0.25, ROOF_Y + 4.0, bz + dz + 0.25), mat::CASINO_PILLAR);
    }
    cylinder(c, bx, bz, 6.5, ROOF_Y + 4.0, ROOF_Y + 4.5, |d, a, _| {
        let seg = ((a + TAU) / TAU * 12.0) as i32;
        Some(if d > 6.0 { mat::NEON_VIOLET } else if seg % 2 == 0 { mat::VELVET } else { mat::CHIP_WHITE })
    });
    // The HIGH ROLLER sign on the north edge (indestructible posts, dark panel, neon letters).
    for x in [-17.0f32, 17.0] {
        c.fill(Vec3::new(x - 0.5, ROOF_Y, 43.0), Vec3::new(x + 0.5, ROOF_Y + 13.0, 44.5), mat::CASINO_PILLAR);
    }
    c.fill(Vec3::new(-17.0, ROOF_Y + 5.0, 43.5), Vec3::new(17.0, ROOF_Y + 11.0, 44.0), mat::ROULETTE_BLACK);
    c.fill_with(Vec3::new(-17.0, ROOF_Y + 5.0, 43.0), Vec3::new(17.0, ROOF_Y + 11.0, 43.5), |p, _, _| {
        (p.y < ROOF_Y + 5.5 || p.y > ROOF_Y + 10.5 || p.x.abs() > 16.5).then_some(mat::NEON_PINK)
    });
    props::text(c, "HIGH ROLLER", Vec3::new(16.25, ROOF_Y + 6.25, 43.5), Vec2::new(-1.0, 0.0), Vec2::new(0.0, -1.0), 0.5, mat::NEON_AMBER);
    // South roof: a giant pair of dice + chip stacks as cover.
    die(c, Vec3::new(6.0, ROOF_Y, -38.0), 7, 6, mat::DICE_RED, mat::CHIP_WHITE);
    die(c, Vec3::new(11.0, ROOF_Y, -35.5), 7, 1, mat::DICE_IVORY, mat::ROULETTE_BLACK);
    let pal = [mat::CHIP_BLACK, mat::PAINT_RED, mat::CHIP_PURPLE, mat::PLASTIC_GREEN];
    for (k, (x, z, n)) in [(-12.0f32, 30.0f32, 4), (-40.0, 22.0, 3), (40.0, -30.0, 5), (-20.0, -30.0, 3)].iter().enumerate() {
        chip_stack(c, Vec3::new(*x, ROOF_Y, *z), 1.5, *n, &pal, 70 + k as u32 * 13);
    }
}

fn build_skyline(c: &mut City, rng: &mut Rng) {
    for k in 0..16 {
        let a = k as f32 * TAU / 16.0 + rng.range(-0.12, 0.12);
        let r = rng.range(64.0, 80.0);
        let hw = rng.range(3.0, 5.0);
        let top = rng.range(12.0, 26.0);
        props::skyline_tower(c, dcos(a) * r, dsin(a) * r, hw, top, k as u32 * 7 + 1);
    }
}

fn pick_spawns(c: &City) -> Vec<Vec3> {
    let wants: [(f32, f32, f32); 10] = [
        (36.0, 30.0, FLOOR_Y),
        (-36.0, -30.0, FLOOR_Y),
        (40.0, 8.0, FLOOR_Y),
        (-40.0, -9.0, FLOOR_Y),
        (0.0, 25.0, FELT_Y),
        (0.0, -25.0, FELT_Y),
        (10.0, 40.0, MEZZ_Y),
        (-10.0, -40.0, MEZZ_Y),
        (32.0, 20.0, ROOF_Y),
        (-32.0, -20.0, ROOF_Y),
    ];
    let mut out: Vec<Vec3> = Vec::with_capacity(10);
    for (x, z, y) in wants {
        if let Some(p) = c.spawn_near(x, z, y + 1.5, &out, 10.0)
            && (p.y - y).abs() < 1.1
        {
            out.push(p);
        }
    }
    out
}

fn surf_halo() -> Vec<SurfLine> {
    let n = HALO_SEGMENTS;
    let overlap = 0.012;
    (0..n)
        .map(|k| {
            let a0 = k as f32 * TAU / n as f32 - overlap;
            let a1 = (k + 1) as f32 * TAU / n as f32 + overlap;
            SurfLine {
                a: Vec3::new(dcos(a0) * HALO_R, HALO_Y, dsin(a0) * HALO_R),
                b: Vec3::new(dcos(a1) * HALO_R, HALO_Y, dsin(a1) * HALO_R),
                face_deg: 55.0,
                width: 3.2,
            }
        })
        .collect()
}

/// The roulette wheel as the sim sees it (the house-spin pockets).
pub fn roulette() -> RouletteWheel {
    RouletteWheel {
        center: Vec3::new(0.0, POCKET_Y, 0.0),
        r_inner: POCKET_R + 0.5,
        r_outer: TRACK_R,
        y_min: POCKET_Y - 0.5,
        y_max: POCKET_Y + 1.5,
        zero_angle: ZERO_ANGLE,
        labels: AMERICAN_WHEEL.iter().map(|p| p.0).collect(),
        colors: AMERICAN_WHEEL.iter().map(|p| p.1).collect(),
    }
}

/// Build High Roller. Deterministic for a given seed (the seed only jitters the skyline and prop
/// details; the layout is fixed so it always plays the same).
pub fn generate_casino(seed: u64) -> (VoxelWorld, MapInfo) {
    let mut rng = Rng::new(seed ^ 0x0ca5_14e0_0000_0777);
    let mut c = City::new(VoxelWorld::centered(CASINO_SIZE_VOXELS), seed ^ 0x6869_6768_726f_6c6c);
    build_shell(&mut c);
    build_slabs(&mut c);
    build_wheel(&mut c);
    build_stairs(&mut c);
    build_columns_and_rails(&mut c);
    build_chandeliers(&mut c);
    build_atrium(&mut c);
    let mut slots = Vec::with_capacity(4);
    build_floor_props(&mut c, &mut slots);
    build_balcony_props(&mut c, &mut slots);
    build_roof(&mut c);
    build_skyline(&mut c, &mut rng);
    let spawns = pick_spawns(&c);
    let (world, pruned) = c.finish_counted();

    let wb = world.bounds();
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb {
            min: Vec3::new(wb.min.x - 24.0, VOID_Y, wb.min.z - 24.0),
            max: Vec3::new(wb.max.x + 24.0, wb.max.y + 40.0, wb.max.z + 24.0),
        },
        void_y: VOID_Y,
        center: Vec3::new(0.0, FELT_Y, 0.0),
        // A warm key light from high up (moonlit night outside; the hall glows on its own).
        sun_dir: Vec3::new(-0.35, 0.82, 0.45).normalize(),
        sky_zenith: [0.04, 0.03, 0.10],
        sky_horizon: [0.42, 0.14, 0.40],
        fog_color: [0.16, 0.08, 0.18],
        ambient: 0.6,
        gravity: None,
        surf_lines: surf_halo(),
        water_sheets: Vec::new(),
        water_walk: false,
        slipstream: None,
        mechanics: Default::default(),
        interactables: Interactables { slots, roulette: Some(roulette()) },
    };
    let districts = vec![
        District { name: "casino_floor", center: Vec3::new(38.0, FLOOR_Y, 30.0), area: [-HALL_X, -HALL_Z, HALL_X, HALL_Z] },
        District { name: "roulette", center: Vec3::new(0.0, POCKET_Y, 0.0), area: [-STEP_R, -STEP_R, STEP_R, STEP_R] },
        District { name: "balcony_n", center: Vec3::new(0.0, MEZZ_Y, 40.0), area: BALCONY_N },
        District { name: "balcony_s", center: Vec3::new(0.0, MEZZ_Y, -40.0), area: BALCONY_S },
        District { name: "balcony_w", center: Vec3::new(-47.0, MEZZ_Y, 0.0), area: BALCONY_W },
        District { name: "roof", center: Vec3::new(30.0, ROOF_Y, 20.0), area: [-HALL_X, -HALL_Z, HALL_X, HALL_Z] },
    ];
    let info = MapInfo {
        name: "High Roller",
        arena,
        water_y: -1000.0,
        water_color: [0.20, 0.70, 0.92],
        water_deep: [0.06, 0.26, 0.48],
        sun_color: [1.0, 0.84, 0.58],
        sky_mid: [0.20, 0.07, 0.26],
        fog_start: 40.0,
        fog_end: 190.0,
        neon_boost: 1.5,
        districts,
        routes: routes(),
        pruned_voxels: pruned,
    };
    (world, info)
}

fn routes() -> Vec<Route> {
    let hub = |k: usize| {
        let a = k as f32 * TAU / 6.0;
        Vec3::new(dcos(a) * CHANDELIER_R, HUB_Y, dsin(a) * CHANDELIER_R)
    };
    let ring: Vec<Vec3> = (0..=16)
        .map(|k| {
            let a = k as f32 * TAU / 16.0;
            Vec3::new(dcos(a) * HALO_R, HALO_Y, dsin(a) * HALO_R)
        })
        .collect();
    vec![
        Route {
            name: "floor_bhop",
            from: "casino_floor",
            to: "casino_floor",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(-48.0, FLOOR_Y, 32.0), Vec3::new(48.0, FLOOR_Y, 32.0)],
            anchors: vec![],
        },
        Route {
            name: "chandelier_swing",
            from: "balcony_n",
            to: "balcony_s",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(-18.0, MEZZ_Y, 40.0), Vec3::new(-18.0, MEZZ_Y, -40.0)],
            anchors: vec![hub(2), hub(4)],
        },
        Route { name: "roulette_surf", from: "roulette", to: "roulette", kind: RouteKind::Mixed, waypoints: ring, anchors: vec![] },
    ]
}

/// Voxel-free introspection for tests / the preview: the four big machines' frames.
pub fn big_slot_count() -> usize {
    4
}
