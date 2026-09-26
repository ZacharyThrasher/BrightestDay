//! **Ember** — a volcanic world at dusk: dark basalt islands between rivers and lakes of glowing lava,
//! a mining facility on stilts over the main river and a twin-spired fortress on the ridge
//! (DESIGN.md "Maps"; feature `ember`). Private use only — the look is inspired by a well-known film
//! lava planet (see THIRD_PARTY_NOTICES.md); names, layout and kit are our own.
//!
//! **The lava is the void.** Lava covers the whole floor of the world (a glowing, indestructible
//! voxel sheet whose top is at [`LAVA_TOP`]); the bottom blast zone [`VOID_Y`] sits just above it,
//! so dropping into any river, lake, pit or rivulet is a KO through the normal blast-zone rules —
//! no map-specific sim code. Top = star KO (40 m over the world top); sides are far past the cliff
//! ring (anything launched that far lands in lava first).
//!
//! Layout (metres; +X east, +Z north, Y up):
//!
//! ```text
//!   N   ring of jagged basalt teeth (r ≈ 88, backdrop + tether anchors, pointy: no perches)
//!       RIDGE (26) ── FORTRESS (plinth 27, balcony 40, perch 52, spires 60/64) ── RIDGE (26)
//!        ash ramp ↑         lavafall down the cliff into the pool          ↑ ash ramp      surf E ↘
//!   CRAG ══neck══ NORTH BANK (9): rocks, the lavafall pool, two vents ══ stones ∙ ∙      DUNES
//!   (W, 20)  arch ║   stairs ║   FACILITY: deck 12, processor roof 20, stack 36   ║ stairs  stones ∙  (E, 11)
//!   vent  ~~~~~~~~~~~~~~~~~ main lava river (z −15..5) ~ collector arms (26 m) ~ ∙ ~~~~~~~~
//!   surf W ↘ ══neck══ SOUTH PLAIN (7): faceted rocks, the Fang, two lava rivulets ══causeway══
//! ```
//!
//! - **Tiers** (competitive read): low ground = South Plain 7 / North Bank 9 / facility deck 12
//!   (fast, lots of cover, but lava close by); mid = processor roof 20 / Crag 20 / Dunes 11 + dunes;
//!   high = Ridge 26 and the fortress balcony 40 (sightlines over everything, but exposed, flat, and
//!   backing onto the outer lava). The perch between the spires (52 m) is tiny, coverless and
//!   tether-only.
//! - **Routes** North ↔ South: the facility (two stair catwalks each side + the deck, or through the
//!   processor), the west rock arch, the east stepping stones, swings on the collector arms; via
//!   the Crag (necks both sides) or the Dunes (stones + causeway); two surf lines over the lava
//!   (Ridge → Dunes, Crag → South Plain). North Bank ↔ Ridge: two ash ramps, the cliff (tether),
//!   the fortress's ledge ladders.
//! - **Destruction**: every rock, deck, wall and spire is breakable; slender indestructible cores
//!   (stilts, collector-arm spines, fortress corner posts, spire spines) keep the landmarks — and
//!   their tether anchors — standing. The lava floor is indestructible.
//! - **Mood / perf**: all light is emissive voxels (lava, seams, window slits); the
//!   lava shades from a dark crust at the shores to bright veins mid-stream. Red-orange overcast sky,
//!   warm haze.

// Generator style: several parallel tables indexed together.
#![allow(clippy::too_many_arguments, clippy::needless_range_loop)]

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, SurfLine};
use crate::city::City;
use crate::material::mat;
use crate::noise::{PI, TAU, dcos, dsin, fbm2, hash3, smoothstep, unit, value2, value3};
use crate::world::VoxelWorld;

/// World dimensions in voxels (192 × 80 × 192 m).
pub const EMBER_SIZE_VOXELS: IVec3 = IVec3::new(384, 160, 384);
/// Top of the lava sheet (one voxel layer on the world floor).
pub const LAVA_TOP: f32 = 0.5;
/// Feet below this = KO (touching the lava).
pub const VOID_Y: f32 = 1.0;
/// Plateau heights.
pub const NB_TOP: f32 = 9.0;
pub const RIDGE_TOP: f32 = 26.0;
pub const SP_TOP: f32 = 7.0;
pub const WC_TOP: f32 = 20.0;
pub const ED_TOP: f32 = 11.0;
pub const FI_TOP: f32 = 5.0;
/// Facility: deck walking height, processor roof, stack top.
pub const DECK_Y: f32 = 12.0;
pub const ROOF_Y: f32 = 20.0;
pub const STACK_TOP: f32 = 36.0;
/// Main river centre line under the facility (it meanders further out).
pub const RIVER_Z: f32 = -5.0;
/// Fortress: centre, plinth top, balcony (roof), perch, spire tips (west / east).
pub const FORT: Vec2 = Vec2::new(0.0, 63.0);
pub const FORT_BASE: f32 = 27.0;
pub const BALCONY_Y: f32 = 40.0;
pub const PERCH_Y: f32 = 52.0;
pub const SPIRE_W_TOP: f32 = 64.0;
pub const SPIRE_E_TOP: f32 = 60.0;
/// Island centres.
pub const WC_C: Vec2 = Vec2::new(-72.0, 0.0);
pub const ED_C: Vec2 = Vec2::new(72.0, -6.0);
pub const FI_C: Vec2 = Vec2::new(0.0, -5.0);
/// The pool the lavafall pours into (a KO pit in the North Bank) and two smaller vents.
pub const POOL_C: Vec2 = Vec2::new(0.0, 40.0);
pub const POOL_R: f32 = 5.5;
pub const VENTS: [Vec2; 2] = [Vec2::new(-28.0, 34.0), Vec2::new(28.0, 34.0)];
pub const VENT_R: f32 = 2.6;
/// Lava rivulets across the South Plain (polylines, half width).
pub const RIVULETS: [[Vec2; 4]; 2] = [
    [Vec2::new(-22.0, -12.0), Vec2::new(-26.0, -30.0), Vec2::new(-19.0, -46.0), Vec2::new(-25.0, -72.0)],
    [Vec2::new(20.0, -12.0), Vec2::new(27.0, -32.0), Vec2::new(21.0, -48.0), Vec2::new(27.0, -72.0)],
];
pub const RIVULET_HW: f32 = 1.8;
/// Rock slabs over the rivulets (walkable, lava flows underneath): centre, radius.
pub const RIVULET_BRIDGES: [(Vec2, f32); 4] =
    [(Vec2::new(-24.0, -24.0), 3.0), (Vec2::new(-21.5, -53.0), 3.0), (Vec2::new(23.5, -24.0), 3.0), (Vec2::new(24.0, -58.0), 3.0)];
/// Facility catwalk stair x positions (±).
pub const CATWALK_X: f32 = 15.0;
/// Facility deck extent (x0, z0, x1, z1) and the processor footprint.
pub const DECK: [f32; 4] = [-20.0, -13.0, 20.0, 3.0];
pub const PROCESSOR: [f32; 4] = [-6.0, -9.0, 6.0, -1.0];
/// Collector-arm heads (hanging over the river): the tether anchors of the arm swings.
pub const ARM_HEAD_X: f32 = 31.5;
pub const ARM_TOP_Y: f32 = 26.0;
/// West rock arch (NB ↔ SP over the river) along x = `ARCH_X`, from z `ARCH_Z.0` (north) to `.1`.
pub const ARCH_X: f32 = -40.0;
pub const ARCH_Z: (f32, f32) = (9.0, -25.0);
pub const ARCH_RISE: f32 = 5.0;
/// East stepping stones across the river (centre, top) and North Bank → Dunes.
pub const STONES_RIVER: [(Vec2, f32); 3] = [(Vec2::new(41.0, 6.0), 8.5), (Vec2::new(42.0, -1.5), 8.0), (Vec2::new(40.5, -9.0), 7.5)];
pub const STONES_DUNES: [(Vec2, f32); 2] = [(Vec2::new(53.0, 14.0), 9.5), (Vec2::new(57.5, 9.5), 10.0)];
pub const STONE_R: f32 = 2.2;

/// The two surf lines (smooth hard light over the lava): Ridge (SE corner) → Dunes, Crag → South
/// Plain. Each starts buried in the plateau you run off and ends ~10 m above the landing.
pub const SURF_EAST: SurfLine = SurfLine { a: Vec3::new(52.0, 33.0, 50.0), b: Vec3::new(68.0, 21.0, -2.0), face_deg: 60.0, width: 8.0 };
pub const SURF_WEST: SurfLine = SurfLine { a: Vec3::new(-70.0, 27.0, -8.0), b: Vec3::new(-40.0, 17.0, -42.0), face_deg: 60.0, width: 8.0 };

/// Spawn targets (x, z, plateau top): two per big plateau, mirrored east/west; never on the
/// facility (the contested middle) or the fortress.
const SPAWN_TARGETS: [(f32, f32, f32); 9] = [
    (-38.0, 58.0, RIDGE_TOP),
    (38.0, 58.0, RIDGE_TOP),
    (-24.0, 22.0, NB_TOP),
    (24.0, 22.0, NB_TOP),
    (-34.0, -46.0, SP_TOP),
    (34.0, -46.0, SP_TOP),
    (0.0, -60.0, SP_TOP),
    (-74.0, 6.0, WC_TOP),
    (74.0, -10.0, ED_TOP),
];

// ------------------------------------------------------------------------------------------------
// Shapes

/// Signed distance to a rounded rectangle (negative inside).
fn rrect(p: Vec2, min: Vec2, max: Vec2, r: f32) -> f32 {
    let c = (min + max) * 0.5;
    let h = (max - min) * 0.5;
    let q = (p - c).abs() - (h - Vec2::splat(r));
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - r
}

/// Approximate signed distance to an axis-aligned ellipse (metres, negative inside).
fn ellipse(p: Vec2, c: Vec2, rx: f32, rz: f32) -> f32 {
    let d = (p - c) / Vec2::new(rx, rz);
    (d.length() - 1.0) * rx.min(rz)
}

/// Distance from `p` to segment `a–b` and the projection parameter (0..1).
fn seg(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    ((p - (a + ab * t)).length(), t)
}

fn polyline(p: Vec2, pts: &[Vec2]) -> f32 {
    pts.windows(2).map(|w| seg(p, w[0], w[1]).0).fold(f32::INFINITY, f32::min)
}

/// Centre line of the main river (straight under the facility, meandering beyond).
pub fn river_z(x: f32) -> f32 {
    RIVER_Z + 3.5 * dsin(x * 0.05) * smoothstep(20.0, 40.0, x.abs())
}

fn river_hw(x: f32, seed: u32) -> f32 {
    10.0 + 1.5 * value2(x * 0.04, 0.5, seed ^ 5) * smoothstep(18.0, 30.0, x.abs())
}

/// What a terrain column is (surface materials / details).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Plain,
    Ridge,
    Crag,
    Dunes,
    /// Ramps, necks, causeways: smooth ash paths.
    Path,
    /// The rock under the facility.
    Isle,
}

#[derive(Clone, Copy, Debug)]
struct Col {
    top: f32,
    /// > 0: a slab over lava (rivulet bridge) starting at this height.
    bottom: f32,
    kind: Kind,
}

fn take(best: &mut Option<Col>, top: f32, kind: Kind) {
    if best.is_none_or(|b| top > b.top) {
        *best = Some(Col { top, bottom: 0.0, kind });
    }
}

/// The terrain heightfield: plateau top at (x, z), or `None` where the lava shows.
fn column(p: Vec2, seed: u32) -> Option<Col> {
    let (x, z) = (p.x, p.y);
    let warp = 2.4 * fbm2(x * 0.055, z * 0.055, seed, 3);
    let hills = fbm2(x * 0.035, z * 0.035, seed ^ 1, 3);
    let mut best: Option<Col> = None;

    // North Bank.
    if rrect(p, Vec2::new(-48.0, 4.0), Vec2::new(48.0, 50.0), 6.0) + warp < 0.0 {
        take(&mut best, NB_TOP + 0.6 * hills, Kind::Plain);
    }
    // Ridge (the fortress plinth flattens its middle later).
    if rrect(p, Vec2::new(-60.0, 44.0), Vec2::new(60.0, 82.0), 8.0) + warp * 1.2 < 0.0 {
        take(&mut best, RIDGE_TOP + 0.8 * fbm2(x * 0.05, z * 0.05, seed ^ 2, 3), Kind::Ridge);
    }
    // Ash ramps North Bank → Ridge on both flanks (smooth: 17 m over 32 m).
    for s in [-1.0f32, 1.0] {
        let xin = x * s;
        if (38.0..48.0).contains(&xin) && (14.0..48.0).contains(&z) {
            let t = ((z - 14.0) / 32.0).clamp(0.0, 1.0);
            take(&mut best, NB_TOP + (RIDGE_TOP - NB_TOP) * t, Kind::Path);
        }
    }
    // South Plain.
    if rrect(p, Vec2::new(-52.0, -68.0), Vec2::new(52.0, -6.0), 8.0) + warp < 0.0 {
        let dune = 0.6 * (0.5 + 0.5 * fbm2(x * 0.09, z * 0.05, seed ^ 3, 2));
        take(&mut best, SP_TOP + 0.4 * hills + dune, Kind::Plain);
    }
    // Crag (W, high, jagged).
    if ellipse(p, WC_C, 13.0, 28.0) + warp * 0.8 < 0.0 {
        take(&mut best, WC_TOP + 1.6 * fbm2(x * 0.08, z * 0.08, seed ^ 4, 3), Kind::Crag);
    }
    // Dunes (E, ash dunes).
    if ellipse(p, ED_C, 14.0, 30.0) + warp * 0.8 < 0.0 {
        let dune = 1.8 * (0.5 + 0.5 * fbm2(x * 0.07, z * 0.045, seed ^ 6, 3));
        take(&mut best, ED_TOP + dune, Kind::Dunes);
    }
    // Necks / causeway: sloped ash paths.
    let necks: [(Vec2, f32, Vec2, f32); 3] = [
        (Vec2::new(-64.0, 18.0), WC_TOP, Vec2::new(-44.0, 18.0), NB_TOP),
        (Vec2::new(-68.0, -22.0), WC_TOP, Vec2::new(-46.0, -24.0), SP_TOP),
        (Vec2::new(64.0, -28.0), ED_TOP, Vec2::new(46.0, -28.0), SP_TOP),
    ];
    for (a, ta, b, tb) in necks {
        let (d, t) = seg(p, a, b);
        if d + warp * 0.25 < 3.2 {
            take(&mut best, ta + (tb - ta) * t, Kind::Path);
        }
    }

    // ---- lava cuts ----
    // Main river (between the Crag and the Dunes); the facility rock stands in it.
    if x.abs() < 56.0 && (z - river_z(x)).abs() < river_hw(x, seed) {
        if (p - FI_C).length() + warp * 0.3 < 8.0 {
            return Some(Col { top: FI_TOP + 0.3 * hills, bottom: 0.0, kind: Kind::Isle });
        }
        return None;
    }
    // Rivulets across the South Plain (bridged by rock slabs in four places).
    if z < -8.0 {
        for r in &RIVULETS {
            if polyline(p, r) + 0.3 * warp < RIVULET_HW {
                let bridged = RIVULET_BRIDGES.iter().any(|(c, rad)| (p - *c).length() < *rad);
                return match (bridged, best) {
                    (true, Some(b)) => Some(Col { bottom: b.top - 2.5, ..b }),
                    _ => None,
                };
            }
        }
    }
    // Pits to the lava in the North Bank: the lavafall pool and two vents.
    if (p - POOL_C).length() + 0.4 * warp < POOL_R || VENTS.iter().any(|v| (p - *v).length() + 0.3 * warp < VENT_R) {
        return None;
    }
    best
}

// ------------------------------------------------------------------------------------------------
// Terrain fill

/// Near-lava rock band: basalt shot through with glowing magma seams (sells the underglow).
fn body_mat(v: IVec3, band_off: f32, seed: u32) -> Material {
    let ym = (v.y as f32 + 0.5) * 0.5;
    if ym < 2.8 {
        let p = 0.28 + (2.8 - ym) * 0.18;
        return if unit(hash3(v.x, v.y, v.z, seed ^ 0x5ea)) < p { mat::MAGMA_SEAM } else { mat::VOLCANIC };
    }
    let band = (ym * 0.55 + band_off).floor() as i32;
    if band.rem_euclid(3) == 0 { mat::VOLCANIC_LIGHT } else { mat::VOLCANIC }
}

/// Fill the heightfield and the lava sheet. Returns the per-column "land" mask (for lava shading).
fn build_terrain(c: &mut City, seed: u32) {
    let (lo, hi) = (c.w.voxel_min(), c.w.voxel_max());
    let (nx, nz) = ((hi.x - lo.x) as usize, (hi.z - lo.z) as usize);
    let mut land = vec![false; nx * nz];
    for iz in 0..nz {
        for ix in 0..nx {
            let (vx, vz) = (lo.x + ix as i32, lo.z + iz as i32);
            let p = Vec2::new((vx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5);
            let Some(col) = column(p, seed) else { continue };
            land[iz * nx + ix] = col.bottom <= 0.0;
            let ty = ((col.top + 0.01) / 0.5).floor() as i32 - 1;
            let by = if col.bottom > 0.0 { (col.bottom / 0.5).floor() as i32 } else { 1 };
            let band_off = 0.9 * value2(p.x * 0.04, p.y * 0.04, seed ^ 11);
            let patch = fbm2(p.x * 0.09, p.y * 0.09, seed ^ 12, 2);
            let (surf, sub, depth) = match col.kind {
                Kind::Plain if patch > 0.05 => (mat::ASH, mat::ASH, 2),
                Kind::Plain => (mat::VOLCANIC, mat::VOLCANIC, 1),
                Kind::Ridge if patch > 0.25 => (mat::ASH, mat::ASH, 1),
                Kind::Ridge => (mat::VOLCANIC_LIGHT, mat::VOLCANIC, 1),
                Kind::Crag if patch < -0.3 => (mat::ASH, mat::VOLCANIC, 1),
                Kind::Crag => (mat::VOLCANIC_LIGHT, mat::VOLCANIC_LIGHT, 1),
                Kind::Dunes => (mat::ASH, mat::ASH, 3),
                Kind::Path => (mat::ASH, mat::VOLCANIC, 1),
                Kind::Isle => (mat::VOLCANIC, mat::VOLCANIC, 1),
            };
            for y in by..ty.saturating_sub(depth - 1) {
                let v = IVec3::new(vx, y, vz);
                c.w.set_raw(v, body_mat(v, band_off, seed));
            }
            if ty >= by {
                c.w.fill_column(vx, vz, (ty - depth + 1).max(by), ty, sub);
                c.w.set_raw(IVec3::new(vx, ty, vz), surf);
            }
        }
    }
    // Lava sheet on the whole floor, shaded by distance to the shore (chamfer distance transform)
    // and a flow-stretched noise: dark crust at the banks and in floating plates, hot veins mid-stream.
    const INF: f32 = 1e9;
    let mut d: Vec<f32> = land.iter().map(|&l| if l { 0.0 } else { INF }).collect();
    let s2 = std::f32::consts::SQRT_2;
    for iz in 0..nz {
        for ix in 0..nx {
            let i = iz * nx + ix;
            let mut m = d[i];
            if ix > 0 {
                m = m.min(d[i - 1] + 1.0);
            }
            if iz > 0 {
                m = m.min(d[i - nx] + 1.0);
                if ix > 0 {
                    m = m.min(d[i - nx - 1] + s2);
                }
                if ix + 1 < nx {
                    m = m.min(d[i - nx + 1] + s2);
                }
            }
            d[i] = m;
        }
    }
    for iz in (0..nz).rev() {
        for ix in (0..nx).rev() {
            let i = iz * nx + ix;
            let mut m = d[i];
            if ix + 1 < nx {
                m = m.min(d[i + 1] + 1.0);
            }
            if iz + 1 < nz {
                m = m.min(d[i + nx] + 1.0);
                if ix + 1 < nx {
                    m = m.min(d[i + nx + 1] + s2);
                }
                if ix > 0 {
                    m = m.min(d[i + nx - 1] + s2);
                }
            }
            d[i] = m;
        }
    }
    for iz in 0..nz {
        for ix in 0..nx {
            let (vx, vz) = (lo.x + ix as i32, lo.z + iz as i32);
            let p = Vec2::new((vx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5);
            let dm = d[iz * nx + ix] * 0.5;
            let flow = fbm2(p.x * 0.07, p.y * 0.16, seed ^ 21, 3);
            let fleck = value2(p.x * 0.35, p.y * 0.7, seed ^ 22);
            let m = if dm < 0.6 || (dm < 1.6 && fleck > 0.45) || (dm > 3.0 && flow > 0.4) {
                mat::LAVA_CRUST
            } else if flow < -0.42 || (dm > 3.0 && fleck > 0.78) {
                mat::LAVA_HOT
            } else {
                mat::LAVA
            };
            c.w.set_raw(IVec3::new(vx, 0, vz), m);
        }
    }
    // Collapse-search hints on the (indestructible) lava floor every 16 m.
    for vz in (lo.z..hi.z).step_by(32) {
        for vx in (lo.x..hi.x).step_by(32) {
            c.anchor(IVec3::new(vx, 0, vz));
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Rocks, teeth, arches

/// A faceted, angular rock (a convex polytope of random planes) on the ground at (x, z): about `r`
/// wide and `h` tall; facets alternate dark / light basalt so the planes read. Returns its top.
fn jag_rock(c: &mut City, x: f32, z: f32, from_y: f32, r: f32, h: f32) -> Option<Vec3> {
    let g = c.ground(x, z, from_y)?;
    let mut planes: Vec<(Vec3, f32)> = Vec::with_capacity(9);
    let a0 = c.rng.range(0.0, TAU);
    for k in 0..5 {
        let a = a0 + TAU * k as f32 / 5.0 + c.rng.range(-0.35, 0.35);
        let n = Vec3::new(dcos(a), c.rng.range(0.1, 0.55), dsin(a)).normalize();
        planes.push((n, r * c.rng.range(0.7, 1.0)));
    }
    for _ in 0..3 {
        let a = c.rng.range(0.0, TAU);
        let k = c.rng.range(0.25, 0.9);
        let n = Vec3::new(dcos(a) * k, 1.0, dsin(a) * k).normalize();
        planes.push((n, r * c.rng.range(0.85, 1.05)));
    }
    let base = Vec3::new(x, g - 0.25 * h, z);
    let ys = r / h.max(0.1);
    let reach = r * 1.6;
    let lo = Vec3::new(x - reach, g - 1.0, z - reach);
    let hi = Vec3::new(x + reach, g + h * 1.3, z + reach);
    let mut top = g;
    c.fill_with(lo, hi, |p, _, old| {
        let q = (p - base) * Vec3::new(1.0, ys, 1.0);
        let mut worst = f32::NEG_INFINITY;
        let mut face = 0;
        for (k, (n, o)) in planes.iter().enumerate() {
            let s = q.dot(*n) - o;
            if s > 0.0 {
                return None;
            }
            if s > worst {
                worst = s;
                face = k;
            }
        }
        if old != AIR && p.y < g {
            return None;
        }
        top = top.max(p.y + 0.25);
        Some(if face % 2 == 0 { mat::VOLCANIC } else { mat::VOLCANIC_LIGHT })
    });
    Some(Vec3::new(x, top, z))
}

/// A tall jagged basalt tooth rising from `base_y` (tapered hexagonal prism with a lean and a
/// seam-lit foot). Its tip is too thin to stand on — a tether anchor, not a perch. Returns the tip.
fn tooth(c: &mut City, x: f32, z: f32, base_y: f32, height: f32, radius: f32, lean: Vec2) -> Vec3 {
    let seed = c.seed();
    let rot = c.rng.range(0.0, TAU);
    let (sr, cr) = (dsin(rot), dcos(rot));
    let reach = radius + lean.length() + 1.0;
    let lo = Vec3::new(x - reach, base_y - 1.0, z - reach);
    let hi = Vec3::new(x + reach, base_y + height, z + reach);
    let b = Vec2::new(x, z);
    c.fill_with(lo, hi, |p, v, _| {
        let t = ((p.y - base_y) / height).clamp(0.0, 1.0);
        let q0 = Vec2::new(p.x, p.z) - (b + lean * t * t);
        let q = Vec2::new(q0.x * cr + q0.y * sr, -q0.x * sr + q0.y * cr);
        let hex = q.x.abs().max((q.x * 0.5 + q.y * 0.866).abs()).max((q.x * 0.5 - q.y * 0.866).abs());
        let wob = 1.0 + 0.18 * value3(p.x * 0.4, p.y * 0.25, p.z * 0.4, seed);
        let r = radius * (1.0 - 0.9 * t) * wob;
        if hex > r {
            return None;
        }
        if p.y < 2.8 && unit(hash3(v.x, v.y, v.z, seed)) < 0.35 {
            return Some(mat::MAGMA_SEAM);
        }
        let band = (p.y * 0.45 + 0.7 * value2(p.x * 0.2, p.z * 0.2, seed ^ 7)).floor() as i32;
        Some(if band.rem_euclid(3) == 0 { mat::VOLCANIC_LIGHT } else { mat::VOLCANIC })
    });
    Vec3::new(x + lean.x, base_y + height, z + lean.y)
}

/// A walkable natural rock arch along x = `ax` from z `z0` (height `y0`) to `z1` (`y1`), rising
/// `rise` over the chord, ~4 m wide, ash on top.
fn rock_arch(c: &mut City, ax: f32, z0: f32, y0: f32, z1: f32, y1: f32, rise: f32, half_w: f32) {
    let seed = c.seed();
    let (za, zb) = (z0.min(z1), z0.max(z1));
    c.fill_with(Vec3::new(ax - half_w - 1.0, 0.5, za), Vec3::new(ax + half_w + 1.0, y0.max(y1) + rise + 1.0, zb), |p, _, old| {
        let t = (p.z - z0) / (z1 - z0);
        let s = dsin(PI * t);
        let top = y0 + (y1 - y0) * t + rise * s;
        let thick = 1.8 + 3.0 * (1.0 - s) * (1.0 - s);
        // Straight-sided walkable top, flaring a little lower down.
        let edge = half_w + 0.4 * value2(p.z * 0.3, 1.5, seed) + 0.35 * (top - 1.0 - p.y).max(0.0);
        if (p.x - ax).abs() > edge {
            return None;
        }
        if p.y > top || p.y < top - thick {
            return None;
        }
        if old != AIR && p.y < top - 1.0 {
            return None;
        }
        Some(if p.y > top - 0.5 {
            mat::ASH
        } else if p.y > top - 1.0 {
            mat::VOLCANIC
        } else {
            mat::VOLCANIC_LIGHT
        })
    });
}

/// A basalt stepping stone rising out of the lava: a noisy column of radius `r` with a flat top.
fn stone(c: &mut City, at: Vec2, top: f32, r: f32) {
    let seed = c.seed();
    c.fill_with(Vec3::new(at.x - r - 1.0, 0.5, at.y - r - 1.0), Vec3::new(at.x + r + 1.0, top, at.y + r + 1.0), |p, v, _| {
        let d = (Vec2::new(p.x, p.z) - at).length();
        let flare = 1.0 + 0.25 * (1.0 - p.y / top).max(0.0);
        if d > r * flare * (1.0 + 0.12 * value2(p.x * 0.6, p.z * 0.6, seed)) {
            return None;
        }
        if p.y > top - 0.5 {
            return Some(mat::VOLCANIC_LIGHT);
        }
        Some(body_mat(v, 0.3, seed))
    });
}

/// A shallow glowing vent crater (standable; magma floor) at (x, z).
fn vent_crater(c: &mut City, x: f32, z: f32, from_y: f32, r: f32, depth: f32) {
    let Some(top) = c.ground(x, z, from_y) else { return };
    c.fill_with(Vec3::new(x - r * 1.4, top - depth - 1.0, z - r * 1.4), Vec3::new(x + r * 1.4, top + 1.0, z + r * 1.4), |p, _, old| {
        let d = (Vec2::new(p.x, p.z) - Vec2::new(x, z)).length() / r;
        if d >= 1.0 {
            if d < 1.35 && p.y > top - 0.1 && p.y < top + 0.5 && old == AIR {
                return Some(mat::VOLCANIC_LIGHT);
            }
            return None;
        }
        let floor = top - depth * (1.0 - d * d);
        if p.y >= floor {
            return (old != AIR).then_some(AIR);
        }
        if p.y > floor - 0.5 {
            return Some(mat::MAGMA_SEAM);
        }
        None
    });
}

// ------------------------------------------------------------------------------------------------
// The mining facility

fn facility(c: &mut City) {
    let [dx0, dz0, dx1, dz1] = DECK;
    let [px0, pz0, px1, pz1] = PROCESSOR;
    let deck = DECK_Y;
    // Foundation: the processor stands on a rock pillar from the island.
    c.fill(Vec3::new(px0 + 1.0, FI_TOP - 1.0, pz0 + 1.0), Vec3::new(px1 - 1.0, deck - 0.5, pz1 - 1.0), mat::VOLCANIC);
    // Stilts (indestructible) + cross bracing.
    let xs = [dx0 + 0.5, -10.0, 10.0, dx1 - 0.5];
    let zs = [dz0 + 0.5, dz1 - 0.5];
    for &x in &xs {
        for &z in &zs {
            c.fill(Vec3::new(x - 0.5, 0.5, z - 0.5), Vec3::new(x + 0.5, deck - 0.5, z + 0.5), mat::TOWER_CORE);
            c.anchor(IVec3::new(((x - 0.5) / 0.5) as i32, 4, ((z - 0.5) / 0.5) as i32));
        }
        c.line(Vec3::new(x, 2.5, zs[0]), Vec3::new(x, deck - 1.0, zs[1]), mat::STEEL, true);
        c.line(Vec3::new(x, 2.5, zs[1]), Vec3::new(x, deck - 1.0, zs[0]), mat::STEEL, true);
    }
    for w in xs.windows(2) {
        for &z in &zs {
            c.line(Vec3::new(w[0], 3.0, z), Vec3::new(w[1], deck - 1.0, z), mat::STEEL, true);
        }
    }
    // Deck plate: scorched plating on a steel grid, hazard stripes on the edges.
    c.fill_with(Vec3::new(dx0, deck - 0.5, dz0), Vec3::new(dx1, deck, dz1), |p, _, _| {
        let edge = p.x < dx0 + 0.5 || p.x > dx1 - 0.5 || p.z < dz0 + 0.5 || p.z > dz1 - 0.5;
        let grid = (p.x - dx0).rem_euclid(3.0) < 0.5 || (p.z - dz0).rem_euclid(3.0) < 0.5;
        Some(if edge {
            mat::HAZARD_ORANGE
        } else if grid {
            mat::STEEL
        } else {
            mat::SCORCHED
        })
    });
    // Processor: a hollow block on the deck, doors W / E / N, lit window band, core posts.
    c.fill_with(Vec3::new(px0, deck, pz0), Vec3::new(px1, ROOF_Y, pz1), |p, _, _| {
        let wall = p.x < px0 + 1.0 || p.x > px1 - 1.0 || p.z < pz0 + 1.0 || p.z > pz1 - 1.0;
        let roof = p.y > ROOF_Y - 1.0;
        if !wall && !roof {
            return Some(AIR);
        }
        let rel = p.y - deck;
        let zc = (pz0 + pz1) * 0.5;
        // Doors: east/west through the long axis, north onto the north catwalks.
        if !roof && rel < 4.5 && ((p.x < px0 + 1.0 || p.x > px1 - 1.0) && (p.z - zc).abs() < 2.0) {
            return Some(AIR);
        }
        if !roof && rel < 4.5 && p.z > pz1 - 1.0 && p.x.abs() < 2.0 {
            return Some(AIR);
        }
        let corner = (p.x < px0 + 0.5 || p.x > px1 - 0.5) && (p.z < pz0 + 0.5 || p.z > pz1 - 0.5);
        if corner {
            return Some(mat::TOWER_CORE);
        }
        if roof {
            return Some(if p.y > ROOF_Y - 0.5 && (p.x.rem_euclid(2.0) < 0.5) { mat::STEEL } else { mat::SCORCHED });
        }
        if (5.0..6.0).contains(&rel) && (p.x.rem_euclid(2.0) < 1.0 || p.z.rem_euclid(2.0) < 1.0) {
            return Some(mat::NEON_AMBER);
        }
        Some(if (rel - 0.5).rem_euclid(2.5) < 0.5 { mat::STEEL } else { mat::SCORCHED })
    });
    // Roof stairs up the south face (bottom west, top at the roof's east end).
    c.stairs(Vec3::new(-10.0, deck, pz0 - 1.5), Vec2::X, ROOF_Y, 2.0, 1.0, mat::STEEL);
    // Stack on the roof (indestructible spine, glowing lip).
    let stack = Vec2::new(-3.0, -5.0);
    c.fill_with(Vec3::new(stack.x - 2.0, ROOF_Y, stack.y - 2.0), Vec3::new(stack.x + 2.0, STACK_TOP, stack.y + 2.0), |p, _, _| {
        let d = (Vec2::new(p.x, p.z) - stack).length();
        let r = 1.7 - 0.25 * ((p.y - ROOF_Y) / (STACK_TOP - ROOF_Y));
        if d > r {
            return None;
        }
        if p.y > STACK_TOP - 1.0 {
            return Some(if d < r - 0.6 { mat::MAGMA_SEAM } else { mat::NEON_AMBER });
        }
        Some(if (p.y - ROOF_Y).rem_euclid(3.0) < 0.5 { mat::STEEL } else { mat::SCORCHED })
    });
    c.fill(
        Vec3::new(stack.x - 0.25, ROOF_Y - 1.0, stack.y - 0.25),
        Vec3::new(stack.x + 0.25, STACK_TOP - 1.0, stack.y + 0.25),
        mat::TOWER_CORE,
    );
    // Collector arms: girders from the roof out over the river, a hot scoop head hanging from each.
    for s in [-1.0f32, 1.0] {
        let a = Vec3::new(s * (px1 + 0.5), ROOF_Y - 0.5, RIVER_Z);
        let b = Vec3::new(s * ARM_HEAD_X, ARM_TOP_Y, RIVER_Z);
        truss(c, a, b, 1.0, 2.5, mat::STEEL);
        c.line(a, b, mat::TOWER_CORE, false);
        let h = Vec3::new(s * ARM_HEAD_X, ARM_TOP_Y, RIVER_Z);
        c.line(h, h - Vec3::Y * 2.0, mat::TOWER_CORE, false);
        c.fill_with(h + Vec3::new(-1.75, -6.0, -2.0), h + Vec3::new(1.75, -1.0, 2.0), |p, _, _| {
            let rel = p.y - (h.y - 6.0);
            // Tapered scoop: narrower at the bottom, glowing ore inside the lip.
            let w = 1.0 + rel * 0.18;
            if (p.x - h.x).abs() > w || (p.z - h.z).abs() > w + 0.25 {
                return None;
            }
            Some(if rel < 0.5 {
                mat::MAGMA_SEAM
            } else if rel > 4.5 {
                mat::STEEL
            } else {
                mat::HAZARD_ORANGE
            })
        });
    }
    // Catwalk stairs: North Bank (9) → deck, South Plain (7) → deck.
    for s in [-1.0f32, 1.0] {
        c.stairs(Vec3::new(s * CATWALK_X, NB_TOP, 13.0), Vec2::NEG_Y, deck, 2.5, 1.6, mat::STEEL);
        c.stairs(Vec3::new(s * CATWALK_X, SP_TOP, -25.0), Vec2::Y, deck, 2.5, 1.2, mat::STEEL);
    }
    // Containers for cover (one stacked).
    c.container(Vec3::new(-19.0, deck, -4.0), false, mat::HAZARD_ORANGE);
    c.container(Vec3::new(13.5, deck, -8.5), true, mat::SCORCHED);
    c.container(Vec3::new(14.5, deck + 2.5, -8.5), true, mat::HAZARD_ORANGE);
}

/// Square lattice girder from `a` to `b` out of 6-connected voxel lines.
fn truss(c: &mut City, a: Vec3, b: Vec3, half: f32, step: f32, m: Material) {
    let d = b - a;
    let len = d.length();
    let u = d / len;
    let r = if u.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let s1 = r.cross(u).normalize();
    let s2 = u.cross(s1);
    let k = [s1 * half + s2 * half, -s1 * half + s2 * half, -s1 * half - s2 * half, s1 * half - s2 * half];
    for o in k {
        c.line(a + o, b + o, m, false);
    }
    let n = ((len / step).floor() as i32).max(1);
    for i in 0..=n {
        let p = a + u * (len * i as f32 / n as f32);
        for j in 0..4 {
            c.line(p + k[j], p + k[(j + 1) % 4], m, false);
        }
        if i < n {
            let q = a + u * (len * (i + 1) as f32 / n as f32);
            for j in 0..4 {
                let (e0, e1) = if i % 2 == 0 { (k[j], k[(j + 1) % 4]) } else { (k[(j + 1) % 4], k[j]) };
                c.line(p + e0, q + e1, m, false);
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The fortress

/// Keep half extents (x, z) at height `y` (it tapers toward the balcony).
pub fn keep_half(y: f32) -> Vec2 {
    let t = ((y - FORT_BASE) / (BALCONY_Y - FORT_BASE)).clamp(0.0, 1.0);
    Vec2::new(9.0 - 2.5 * t, 7.0 - 2.0 * t)
}

/// The ledge ladders up the keep: (top-centre, half width along x). South face climbs on the east
/// half, north face on the west half; 1.5 m apart vertically, zig-zagging 2.5 m sideways.
pub fn fortress_ledges() -> Vec<(Vec3, f32)> {
    let mut out = Vec::new();
    for (face, xs) in [(-1.0f32, 1.0f32), (1.0, -1.0)] {
        for k in 1..=8 {
            let y = FORT_BASE + 1.5 * k as f32;
            let h = keep_half(y - 0.25);
            let cx = xs * (h.x - if k % 2 == 1 { 1.5 } else { 4.0 });
            let cz = FORT.y + face * (h.y + 0.5);
            out.push((Vec3::new(FORT.x + cx, y, cz), 1.25));
        }
    }
    out
}

fn fortress(c: &mut City) {
    let f = FORT;
    // Plinth: a level paved platform over the ridge noise (dark steps down its south edge).
    c.fill_with(Vec3::new(f.x - 13.0, RIDGE_TOP - 3.0, f.y - 11.0), Vec3::new(f.x + 13.0, FORT_BASE, f.y + 11.0), |p, _, _| {
        let q = (Vec2::new(p.x, p.z) - f).abs();
        let step = (q.x - 11.0).max(q.y - 9.0).max(0.0);
        if p.y > FORT_BASE - 0.5 - step * 0.5 {
            return (step > 0.0).then_some(AIR);
        }
        Some(if p.y > FORT_BASE - 1.0 { mat::VOLCANIC_LIGHT } else { mat::VOLCANIC })
    });
    // Keep: tapered hollow block, doors N / S, glowing window slits, corner cores, roof = balcony.
    c.fill_with(Vec3::new(f.x - 10.0, FORT_BASE, f.y - 8.0), Vec3::new(f.x + 10.0, BALCONY_Y, f.y + 8.0), |p, _, _| {
        let h = keep_half(p.y);
        let q = Vec2::new(p.x - f.x, p.z - f.y);
        if q.x.abs() > h.x || q.y.abs() > h.y {
            return None;
        }
        let roof = p.y > BALCONY_Y - 1.5;
        let wall = q.x.abs() > h.x - 1.0 || q.y.abs() > h.y - 1.0;
        if !wall && !roof {
            return Some(AIR);
        }
        let corner = q.x.abs() > h.x - 0.5 && q.y.abs() > h.y - 0.5;
        if corner && !roof {
            return Some(mat::OBSIDIAN_CORE);
        }
        let rel = p.y - FORT_BASE;
        if !roof && rel < 5.5 && q.x.abs() < 2.0 && q.y.abs() > h.y - 1.0 {
            return Some(AIR);
        }
        // Window slits: E/W faces every 3 m, and flanking the doors.
        if !roof && (2.5..7.0).contains(&rel) {
            let ew = q.x.abs() > h.x - 1.0 && (q.y + 10.0).rem_euclid(3.0) < 0.5;
            let ns = q.y.abs() > h.y - 1.0 && (4.0..4.5).contains(&q.x.abs());
            if ew || ns {
                return Some(mat::MAGMA_SEAM);
            }
        }
        Some(if (rel - 0.25).rem_euclid(4.0) < 0.5 { mat::VOLCANIC } else { mat::OBSIDIAN })
    });
    // Balcony parapet (a 0.5 m lip you can step over).
    let hb = keep_half(BALCONY_Y);
    c.fill_with(Vec3::new(f.x - hb.x, BALCONY_Y, f.y - hb.y), Vec3::new(f.x + hb.x, BALCONY_Y + 0.5, f.y + hb.y), |p, _, _| {
        let q = Vec2::new(p.x - f.x, p.z - f.y);
        let rim = q.x.abs() > hb.x - 0.5 || q.y.abs() > hb.y - 0.5;
        (rim && (q.x.abs() > 2.0 || q.y.abs() < hb.y - 0.5)).then_some(mat::OBSIDIAN)
    });
    // Ledge ladders.
    for (top, hw) in fortress_ledges() {
        c.fill(Vec3::new(top.x - hw, top.y - 0.5, top.z - 0.5), Vec3::new(top.x + hw, top.y, top.z + 0.5), mat::OBSIDIAN);
    }
    // Twin spires: chamfered blades on the balcony, a glowing seam in the slot between them.
    for (cx, tip) in [(-1.75f32, SPIRE_W_TOP), (1.75, SPIRE_E_TOP)] {
        let (hx, hz) = (1.5f32, 2.6f32);
        let span = tip - BALCONY_Y;
        c.fill_with(
            Vec3::new(f.x + cx - hx - 0.5, BALCONY_Y - 0.5, f.y - hz - 0.5),
            Vec3::new(f.x + cx + hx + 0.5, tip, f.y + hz + 0.5),
            |p, _, _| {
                let t = ((p.y - BALCONY_Y) / span).clamp(0.0, 1.0);
                let s = (1.0 - t) * (1.0 + 0.3 * t);
                let q = Vec2::new((p.x - f.x - cx).abs() / hx, (p.z - f.y).abs() / hz);
                if q.x > s || q.y > s || q.x + q.y > 1.4 * s {
                    return None;
                }
                Some(if (p.y * 0.5).floor() as i32 % 5 == 0 { mat::VOLCANIC } else { mat::OBSIDIAN })
            },
        );
        c.fill(
            Vec3::new(f.x + cx - 0.25, BALCONY_Y - 1.0, f.y - 0.25),
            Vec3::new(f.x + cx + 0.25, tip - 1.5, f.y + 0.25),
            mat::OBSIDIAN_CORE,
        );
    }
    c.fill(Vec3::new(f.x - 0.5, BALCONY_Y, f.y - 1.5), Vec3::new(f.x + 0.5, PERCH_Y - 1.0, f.y + 1.5), mat::MAGMA_SEAM);
    // The perch: a small ledge jutting south from the slot (tether up; coverless).
    c.fill(Vec3::new(f.x - 1.5, PERCH_Y - 0.5, f.y - 3.5), Vec3::new(f.x + 1.5, PERCH_Y, f.y - 0.5), mat::OBSIDIAN);
    c.anchor(IVec3::new(((f.x - 1.75) / 0.5).floor() as i32, ((BALCONY_Y + 4.0) / 0.5) as i32, (f.y / 0.5).floor() as i32));
}

/// Cliff z (south face of the ridge) at x = 0: the first column north of the North Bank whose top
/// is near the ridge height.
fn cliff_z(seed: u32) -> f32 {
    let mut z = 40.0;
    while z < 70.0 {
        if column(Vec2::new(0.25, z + 0.25), seed).is_some_and(|c| c.top > RIDGE_TOP - 2.0) {
            return z;
        }
        z += 0.5;
    }
    44.0
}

/// The lavafall: a glowing curtain from a cave mouth in the ridge cliff under the fortress down
/// into the pool (a KO pit) at its foot.
fn lavafall(c: &mut City, seed: u32) {
    let cz = cliff_z(seed);
    // Cave mouth.
    c.fill(Vec3::new(-2.0, RIDGE_TOP - 5.0, cz), Vec3::new(2.0, RIDGE_TOP - 2.0, cz + 2.0), AIR);
    c.fill(Vec3::new(-2.0, RIDGE_TOP - 5.5, cz), Vec3::new(2.0, RIDGE_TOP - 5.0, cz + 2.5), mat::MAGMA_SEAM);
    c.fill(Vec3::new(-2.0, RIDGE_TOP - 5.0, cz + 2.0), Vec3::new(2.0, RIDGE_TOP - 2.0, cz + 2.5), mat::LAVA_HOT);
    // Curtain.
    c.fill_with(Vec3::new(-1.5, LAVA_TOP, cz - 1.0), Vec3::new(1.5, RIDGE_TOP - 5.0, cz), |p, v, _| {
        let hot = p.x.abs() < 0.6 || unit(hash3(v.x, v.y, v.z, seed ^ 0xfa11)) < 0.2;
        Some(if hot { mat::LAVA_HOT } else { mat::LAVA })
    });
}

// ------------------------------------------------------------------------------------------------

/// Where decor must not go: spawn targets, stair mouths, route lines, surf landings, the fortress.
fn keep_clear(p: Vec2) -> bool {
    let near = |q: Vec2, r: f32| (p - q).length() < r;
    SPAWN_TARGETS.iter().any(|&(x, z, _)| near(Vec2::new(x, z), 5.5))
        || [-CATWALK_X, CATWALK_X].iter().any(|&x| (p.x - x).abs() < 3.5 && (p.y > 2.0 && p.y < 18.0 || p.y < -12.0 && p.y > -32.0))
        || (p.x - ARCH_X).abs() < 4.0 && p.y > -30.0 && p.y < 14.0
        || (rrect(p, FORT - Vec2::new(14.0, 12.0), FORT + Vec2::new(14.0, 12.0), 0.0) < 0.0)
        || near(Vec2::new(SURF_EAST.b.x, SURF_EAST.b.z), 8.0)
        || near(Vec2::new(SURF_WEST.b.x, SURF_WEST.b.z), 8.0)
        || near(Vec2::new(SURF_EAST.a.x, SURF_EAST.a.z), 6.0)
        || near(Vec2::new(SURF_WEST.a.x, SURF_WEST.a.z), 6.0)
        || RIVULET_BRIDGES.iter().any(|(c, r)| near(*c, r + 2.0))
        || (p.x.abs() > 36.0 && p.x.abs() < 50.0 && p.y > 12.0 && p.y < 50.0)
        || near(POOL_C, POOL_R + 3.0)
        || VENTS.iter().any(|v| near(*v, VENT_R + 2.5))
}

/// Faceted rocks scattered in an area (skipping lava, keep-clear zones and each other).
fn scatter_rocks(c: &mut City, seed: u32, area: [f32; 4], count: usize, top: f32, r: (f32, f32), h: (f32, f32), placed: &mut Vec<Vec2>) {
    let mut tries = 0;
    let mut n = 0;
    while n < count && tries < count * 20 {
        tries += 1;
        let p = Vec2::new(c.rng.range(area[0], area[2]), c.rng.range(area[1], area[3]));
        if keep_clear(p) || placed.iter().any(|q| (p - *q).length() < 7.0) {
            continue;
        }
        // Solid ground all around (not a shore or a rivulet edge).
        let ok = [(0.0, 0.0), (3.5, 0.0), (-3.5, 0.0), (0.0, 3.5), (0.0, -3.5)]
            .iter()
            .all(|&(dx, dz)| column(p + Vec2::new(dx, dz), seed).is_some_and(|col| col.bottom <= 0.0 && (col.top - top).abs() < 4.0));
        if !ok {
            continue;
        }
        let rr = c.rng.range(r.0, r.1);
        let hh = c.rng.range(h.0, h.1);
        if jag_rock(c, p.x, p.y, top + 6.0, rr, hh).is_some() {
            placed.push(p);
            n += 1;
        }
    }
}

/// Build Ember. Deterministic for a given seed (the skeleton is fixed; details vary).
pub fn generate_ember(seed: u64) -> (VoxelWorld, MapInfo) {
    let s32 = (seed as u32) ^ 0x00e3_be00;
    let mut c = City::new(VoxelWorld::centered(EMBER_SIZE_VOXELS), seed ^ 0x656d_6265_7200);
    build_terrain(&mut c, s32);

    // ---- landmarks ------------------------------------------------------------------------------
    facility(&mut c);
    fortress(&mut c);
    lavafall(&mut c, s32);
    rock_arch(&mut c, ARCH_X, ARCH_Z.0, NB_TOP, ARCH_Z.1, SP_TOP, ARCH_RISE, 2.2);
    for (at, top) in STONES_RIVER.iter().chain(STONES_DUNES.iter()) {
        stone(&mut c, *at, *top, STONE_R);
    }

    // ---- rocks, needles, teeth ------------------------------------------------------------------
    let mut placed: Vec<Vec2> = Vec::new();
    scatter_rocks(&mut c, s32, [-48.0, -64.0, 48.0, -20.0], 16, SP_TOP, (1.6, 3.2), (2.2, 5.0), &mut placed);
    scatter_rocks(&mut c, s32, [-44.0, 10.0, 44.0, 38.0], 8, NB_TOP, (1.5, 2.8), (2.0, 4.2), &mut placed);
    scatter_rocks(&mut c, s32, [-56.0, 50.0, 56.0, 76.0], 6, RIDGE_TOP, (1.4, 2.6), (1.8, 3.6), &mut placed);
    scatter_rocks(&mut c, s32, [62.0, -32.0, 82.0, 18.0], 4, ED_TOP + 1.0, (1.5, 2.5), (2.0, 3.5), &mut placed);
    scatter_rocks(&mut c, s32, [-82.0, -22.0, -62.0, 22.0], 3, WC_TOP, (1.4, 2.4), (2.0, 3.0), &mut placed);
    // The Fang: a tall needle in the South Plain (anchor, not a perch).
    if let Some(g) = c.ground(0.0, -46.0, SP_TOP + 4.0) {
        tooth(&mut c, 0.0, -46.0, g - 1.0, 19.0, 3.0, Vec2::new(0.6, -1.2));
    }
    // Crag needles + vent.
    for (dx, dz, h) in [(-4.0, 14.0, 13.0), (5.0, -12.0, 11.0), (-6.0, -20.0, 9.0)] {
        let (x, z) = (WC_C.x + dx, WC_C.y + dz);
        if let Some(g) = c.ground(x, z, WC_TOP + 4.0) {
            let lean = Vec2::new(dx, dz).normalize_or_zero() * 1.2;
            tooth(&mut c, x, z, g - 1.0, h, 2.2, lean);
        }
    }
    vent_crater(&mut c, WC_C.x + 1.0, WC_C.y - 3.0, WC_TOP + 4.0, 3.5, 1.5);
    // Dunes arch (anchor over the dunes, walkable top).
    {
        let seed = c.seed();
        let (a, b) = (Vec3::new(ED_C.x - 6.0, ED_TOP, ED_C.y - 18.0), Vec3::new(ED_C.x + 6.0, ED_TOP, ED_C.y - 4.0));
        let n = 80;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let s = dsin(PI * t);
            let p = a + (b - a) * t + Vec3::Y * 8.0 * s;
            let r = 1.3 * (1.0 + 0.9 * (1.0 - s) * (1.0 - s)) * (1.0 + 0.12 * value2(t * 9.0, 0.0, seed));
            c.w.fill_sphere(p, r, mat::VOLCANIC_LIGHT, true);
        }
    }
    // Ring of teeth around the basin (backdrop + far anchors).
    let n_teeth = 26;
    for k in 0..n_teeth {
        let ang = TAU * k as f32 / n_teeth as f32 + c.rng.range(-0.08, 0.08);
        let r = c.rng.range(84.0, 90.0);
        let (x, z) = (dcos(ang) * r, dsin(ang) * r);
        let h = c.rng.range(16.0, 34.0) + if z > 60.0 { 8.0 } else { 0.0 };
        let rad = c.rng.range(2.6, 4.4);
        let lean = Vec2::new(dcos(ang), dsin(ang)) * c.rng.range(0.5, 2.5);
        tooth(&mut c, x, z, 0.5, h, rad, lean);
    }

    // ---- spawns ---------------------------------------------------------------------------------
    let mut spawns: Vec<Vec3> = Vec::new();
    for (x, z, top) in SPAWN_TARGETS {
        if let Some(s) = c.spawn_near(x, z, top + 6.0, &spawns, 12.0) {
            spawns.push(s);
        }
    }

    let (world, pruned) = c.finish_counted();
    let wb = world.bounds();
    let center = Vec3::new(0.0, DECK_Y, RIVER_Z);
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb {
            min: Vec3::new(wb.min.x - 24.0, VOID_Y, wb.min.z - 24.0),
            max: Vec3::new(wb.max.x + 24.0, wb.max.y + 40.0, wb.max.z + 24.0),
        },
        void_y: VOID_Y,
        center,
        // A low sun smothered by ash: long warm light from the south-west, behind the fortress's
        // silhouette from the plains.
        sun_dir: Vec3::new(-0.35, 0.36, -0.86).normalize(),
        sky_zenith: [0.16, 0.05, 0.06],
        sky_horizon: [0.86, 0.34, 0.16],
        fog_color: [0.46, 0.17, 0.10],
        ambient: 0.30,
        gravity: None,
        surf_lines: vec![SURF_EAST, SURF_WEST],
        water_sheets: Vec::new(),
        water_walk: false,
        slipstream: None,
        mechanics: Default::default(),
    };
    let d = |name: &'static str, x: f32, y: f32, z: f32, area: [f32; 4]| District { name, center: Vec3::new(x, y, z), area };
    let districts = vec![
        d("facility", 0.0, DECK_Y, RIVER_Z, [DECK[0], DECK[1], DECK[2], DECK[3]]),
        d("north_bank", 0.0, NB_TOP, 24.0, [-48.0, 5.0, 48.0, 44.0]),
        d("ridge", 0.0, RIDGE_TOP, 60.0, [-60.0, 44.0, 60.0, 82.0]),
        d("fortress", FORT.x, BALCONY_Y, FORT.y, [FORT.x - 9.0, FORT.y - 7.0, FORT.x + 9.0, FORT.y + 7.0]),
        d("south_plain", 0.0, SP_TOP, -40.0, [-52.0, -68.0, 52.0, -15.0]),
        d("crag", WC_C.x, WC_TOP, WC_C.y, [WC_C.x - 13.0, WC_C.y - 28.0, WC_C.x + 13.0, WC_C.y + 28.0]),
        d("dunes", ED_C.x, ED_TOP, ED_C.y, [ED_C.x - 14.0, ED_C.y - 30.0, ED_C.x + 14.0, ED_C.y + 30.0]),
    ];
    let info = MapInfo {
        name: "Ember",
        arena,
        // No water: the lava is voxels (and the void).
        water_y: -1000.0,
        water_color: [1.0, 0.40, 0.10],
        water_deep: [0.40, 0.06, 0.02],
        sun_color: [1.0, 0.55, 0.30],
        sky_mid: [0.52, 0.15, 0.09],
        fog_start: 30.0,
        fog_end: 230.0,
        neon_boost: 1.25,
        districts,
        routes: routes(),
        pruned_voxels: pruned,
    };
    (world, info)
}

fn routes() -> Vec<Route> {
    let ledges = fortress_ledges();
    let hb = keep_half(BALCONY_Y);
    vec![
        Route {
            name: "facility_cross",
            from: "north_bank",
            to: "south_plain",
            kind: RouteKind::Ground,
            waypoints: vec![
                Vec3::new(CATWALK_X, NB_TOP, 18.0),
                Vec3::new(CATWALK_X, DECK_Y, 2.0),
                Vec3::new(CATWALK_X, DECK_Y, -12.0),
                Vec3::new(CATWALK_X, SP_TOP, -26.0),
                Vec3::new(CATWALK_X, SP_TOP, -32.0),
            ],
            anchors: vec![Vec3::new(ARM_HEAD_X, ARM_TOP_Y - 6.0, RIVER_Z)],
        },
        Route {
            name: "west_arch",
            from: "north_bank",
            to: "south_plain",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(ARCH_X, NB_TOP, 16.0), Vec3::new(ARCH_X, SP_TOP, -30.0)],
            anchors: vec![],
        },
        Route {
            name: "surf_east",
            from: "ridge",
            to: "dunes",
            kind: RouteKind::Mixed,
            waypoints: vec![Vec3::new(46.0, RIDGE_TOP, 56.0), SURF_EAST.a, SURF_EAST.b, Vec3::new(70.0, ED_TOP + 1.0, -10.0)],
            anchors: vec![],
        },
        Route {
            name: "surf_west",
            from: "crag",
            to: "south_plain",
            kind: RouteKind::Mixed,
            waypoints: vec![Vec3::new(-74.0, WC_TOP, 2.0), SURF_WEST.a, SURF_WEST.b, Vec3::new(-36.0, SP_TOP, -46.0)],
            anchors: vec![],
        },
        Route {
            name: "fortress_climb",
            from: "ridge",
            to: "fortress",
            kind: RouteKind::Mixed,
            waypoints: std::iter::once(Vec3::new(ledges[0].0.x, FORT_BASE, ledges[0].0.z - 2.5))
                .chain(ledges.iter().take(8).map(|l| l.0))
                .chain(std::iter::once(Vec3::new(FORT.x + 3.0, BALCONY_Y, FORT.y - hb.y + 1.5)))
                .collect(),
            anchors: vec![Vec3::new(FORT.x - 1.75, SPIRE_W_TOP - 4.0, FORT.y), Vec3::new(FORT.x + 1.75, SPIRE_E_TOP - 4.0, FORT.y)],
        },
        Route {
            name: "arm_swing",
            from: "facility",
            to: "north_bank",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(DECK[2] - 1.5, DECK_Y, RIVER_Z), Vec3::new(38.0, NB_TOP, 16.0)],
            anchors: vec![Vec3::new(ARM_HEAD_X, ARM_TOP_Y - 6.0, RIVER_Z)],
        },
    ]
}

/// The heightfield at (x, z): plateau top, or `None` over lava (for tests and the preview).
pub fn terrain_top(x: f32, z: f32, seed: u64) -> Option<f32> {
    column(Vec2::new(x, z), (seed as u32) ^ 0x00e3_be00).map(|c| c.top)
}
