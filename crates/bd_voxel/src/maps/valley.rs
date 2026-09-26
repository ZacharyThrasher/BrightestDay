//! **Valley of the End** — a river gorge between layered cliffs, a tall waterfall between two
//! colossal facing statues, forest and mountains beyond (DESIGN.md "Maps"; feature `valley`).
//! Modelled on the Naruto location for private use (THIRD_PARTY_NOTICES.md).
//!
//! Layout (metres; +X east, +Z north = upstream, Y up):
//!
//! ```text
//!  z 112 ┌────────────── mountains ── inlet ── mountains ──────────────┐
//!        │  forest     UPPER POOL (water, y 58) + two islets   forest   │  plateau y ≈ 64
//!  z  58 │ ═══════ headwall ══════[ FALLS 30 m wide ]══════ headwall ═══ │
//!        │  cliffs  PEDESTAL+STATUE  (plunge hole)  PEDESTAL+STATUE cliffs│  statue bay (floor 15)
//!  z  30 │  L2 42 ┐ L1 28 ┐ bank 15  RIVER (water, y 14)  bank ┌ L1 ┌ L2   │
//!        │    ramps / chimneys / shelves / surf lines along the curved walls │
//!  z −86 │  ───────────── natural stone arch (L2 to L2) ─────────────     │
//!  z −96 │  ~~~~~~~~~~~~~ downstream lip: the river falls into the void ~~ │
//!  z−112 └────────────────────────────────────────────────────────────────┘
//!        x −80                          0                              80
//! ```
//!
//! - **Gorge.** The river meanders (±8 m) down a 52 m-wide gorge floor (74 m in the statue bay);
//!   the walls step back in three tiers — riser to the L1 ledge (28 m), riser to the L2 ledge
//!   (42 m), riser to the forested plateau (64 m) — so the cliff tops are 70–100 m apart. Every
//!   wall carries horizontal strata bands that bulge and recede (0.5–1 m lips, like the reference's
//!   layered rock). Each side has a walkable switchback of three ramps from the bank to the top,
//!   two wall-kick chimneys (2.5 m slots cut to the plateau), two overhanging shelves at the top
//!   edge (tether anchors), and a surf line hugging its curved wall.
//! - **Falls.** 30 m wide, from the upper pool's lip (58 m) down past the gorge floor into the
//!   plunge hole: indestructible white-water voxels, so the face is a wall (touching it refreshes
//!   the air jump); rock buttresses flank it (kick between the falls and a buttress to climb it)
//!   and two rocks jut out of it as rest ledges. The plunge hole at its foot drops to the KO water
//!   ([`WATER_Y`]): fall down the falls and you're gone unless a tether saves you.
//! - **Statues** ([`statues`]): on pedestals either side of the plunge hole, facing each other,
//!   ~55 m tall — the west one with a spiky mane, the east one with long straight hair.
//! - **Water-walk.** The river and the upper pool are [`WaterSheet`]s (not voxels) with
//!   `ArenaInfo::water_walk` on: players run on them (chakra walking); constructs sink to the bed.
//! - **KOs.** Void: the plunge hole, and the downstream lip where the gorge floor ends (both drop to
//!   the KO water at 3 m). Sides: past the mountain rim (8 m outside the world). Top: star KO.

// Generator style: long parameter lists for terrain helpers.
#![allow(clippy::too_many_arguments)]

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, Gen, SurfLine, WaterSheet, vcenter};
use crate::material::mat;
use crate::noise::{TAU, datan2, dcos, dsin, fbm2, hash3, smoothstep, unit, value2};
use crate::world::VoxelWorld;

#[path = "valley_statues.rs"]
pub mod statues;

use statues::{Hair, StatueSpec};

/// World dimensions in voxels (160 × 96 × 224 m).
pub const VALLEY_SIZE_VOXELS: IVec3 = IVec3::new(320, 192, 448);
/// The KO water under everything (plunge hole, downstream drop): the client draws it; feet below
/// [`VOID_Y`] is a void KO.
pub const WATER_Y: f32 = 3.0;
pub const VOID_Y: f32 = 2.5;
/// River surface (water sheet top) and its depth down to the pebble bed.
pub const RIVER_Y: f32 = 14.0;
pub const RIVER_DEPTH: f32 = 1.5;
/// Gorge floor (banks) and the tier ledges.
pub const BANK_Y: f32 = 15.0;
pub const L1_Y: f32 = 28.0;
pub const L2_Y: f32 = 42.0;
/// Plateau / cliff tops.
pub const TOP_Y: f32 = 64.0;
/// Upper pool surface (= the falls' lip).
pub const POOL_Y: f32 = 58.0;
/// Front (south) face of the falls, and the headwall line behind it.
pub const FALLS_Z: f32 = 57.0;
pub const HEAD_Z: f32 = 58.5;
/// The falls span `|x| < FALLS_HALF`.
pub const FALLS_HALF: f32 = 15.0;
/// Plunge hole (ellipse) at the falls' foot.
pub const PLUNGE_C: Vec2 = Vec2::new(0.0, 47.0);
pub const PLUNGE_R: Vec2 = Vec2::new(13.0, 11.0);
/// The gorge floor ends here (the river pours into the void).
pub const LIP_Z: f32 = -96.0;
/// Statues: feet on the pedestal tops at (±STATUE_X, PEDESTAL_TOP, STATUE_Z).
pub const STATUE_X: f32 = 26.0;
pub const STATUE_Z: f32 = 43.0;
pub const PEDESTAL_TOP: f32 = 18.0;
/// Pedestal footprint: `|x|` in `PED_X`, z in `PED_Z`.
pub const PED_X: (f32, f32) = (18.5, 33.5);
pub const PED_Z: (f32, f32) = (31.0, 55.0);
/// Natural stone arch (L2 ledge to L2 ledge) across the gorge at this z.
pub const ARCH_Z: f32 = -86.0;

const VS: f32 = 0.5;

// ---------------------------------------------------------------------------------------------
// Gorge geometry (pure functions of position: shared by the generator, tests, previews, bots)

fn taper(z: f32) -> f32 {
    1.0 - smoothstep(12.0, 32.0, z)
}

/// X of the gorge / river centre line at `z` (meanders ±8 m downstream of the statue bay).
pub fn centre_x(z: f32) -> f32 {
    8.0 * dsin((z - 30.0) * (TAU / 150.0)) * taper(z)
}

/// Half-width of the gorge floor (cliff foot to cliff foot) at `z`: 26 m, 37 m in the statue bay.
pub fn half_width(z: f32) -> f32 {
    26.0 + 11.0 * smoothstep(-8.0, 28.0, z)
}

/// Half-width of the river at `z` (6–9 m).
pub fn river_half(z: f32) -> f32 {
    7.5 + 1.5 * value2(z * 0.06, 3.7, 0x5249)
}

/// Distance from the cliff foot to the L2 wall (`e1`) and to the cliff-top edge (`e2`) on `side`
/// (−1 west, +1 east): the L1 ledge is 4–6 m wide, the L2 ledge 4–7 m.
pub fn tier_edges(z: f32, side: f32) -> (f32, f32) {
    let w1 = 4.0 + 2.0 * (0.5 + 0.5 * value2(z * 0.045, side * 7.1, 0x4c31));
    let w2 = 4.0 + 3.0 * (0.5 + 0.5 * value2(z * 0.04, side * 3.3, 0x4c32));
    (w1, w1 + w2)
}

/// X of the cliff foot on `side` at `z`.
pub fn wall_x(z: f32, side: f32) -> f32 {
    centre_x(z) + side * half_width(z)
}

/// A walkable ramp along a tier (`tier` 1: bank → L1 on the L1 ledge band; 2: L1 → L2 on the L2
/// band; 3: L2 → top, a strip cut into the plateau edge), rising from `z_lo` to `z_hi`.
#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    pub side: f32,
    pub tier: u8,
    pub z_lo: f32,
    pub z_hi: f32,
}

impl Ramp {
    fn t(&self, z: f32) -> Option<f32> {
        let (a, b) = (self.z_lo.min(self.z_hi), self.z_lo.max(self.z_hi));
        (z >= a && z <= b).then(|| (z - self.z_lo) / (self.z_hi - self.z_lo))
    }
    fn heights(&self) -> (f32, f32) {
        match self.tier {
            1 => (BANK_Y, L1_Y),
            2 => (L1_Y, L2_Y),
            _ => (L2_Y, TOP_Y),
        }
    }
    /// Surface height at `z` (None outside the ramp).
    pub fn height(&self, z: f32) -> Option<f32> {
        let (a, b) = self.heights();
        self.t(z).map(|t| a + (b - a) * t)
    }
}

/// The switchbacks: bank → L1 → L2 → top on each side (ends adjacent, ~26–30° slopes).
pub const RAMPS: [Ramp; 6] = [
    Ramp { side: -1.0, tier: 1, z_lo: -12.0, z_hi: -40.0 },
    Ramp { side: -1.0, tier: 2, z_lo: -44.0, z_hi: -76.0 },
    Ramp { side: -1.0, tier: 3, z_lo: -78.0, z_hi: -32.0 },
    Ramp { side: 1.0, tier: 1, z_lo: -62.0, z_hi: -34.0 },
    Ramp { side: 1.0, tier: 2, z_lo: -30.0, z_hi: 2.0 },
    Ramp { side: 1.0, tier: 3, z_lo: 4.0, z_hi: -42.0 },
];
/// Width of the tier-3 ramp strip cut into the plateau edge.
const RAMP3_W: f32 = 4.0;

/// Wall-kick chimneys: (side, z centre). 2.5 m slots from the bank to the plateau.
pub const CHIMNEYS: [(f32, f32); 4] = [(-1.0, 8.0), (-1.0, -92.0), (1.0, 16.0), (1.0, -76.0)];
const CHIMNEY_HALF: f32 = 1.25;
/// Overhanging shelves at the cliff-top edge: (side, z).
pub const SHELVES: [(f32, f32); 4] = [(-1.0, -2.0), (-1.0, -58.0), (1.0, 30.0), (1.0, -70.0)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Void,
    RiverBed,
    Bank,
    Ledge,
    Plateau,
    PoolBed,
    Shore,
}

/// Base terrain column: surface height (m, top of the highest solid; ≤ 0.5 = bedrock only) and
/// what kind of ground it is.
fn column(x: f32, z: f32, seed: u32) -> (f32, Zone) {
    if z >= HEAD_Z {
        return upper(x, z, seed);
    }
    // Plunge hole at the falls' foot.
    let q = (Vec2::new(x, z) - PLUNGE_C) / PLUNGE_R;
    if q.length_squared() < 1.0 {
        return (0.5, Zone::Void);
    }
    let g = centre_x(z);
    let side = if x < g { -1.0 } else { 1.0 };
    let d = (x - g).abs();
    let w0 = half_width(z);
    let s = d - w0;
    // Chimney slots (full height, floor at bank level).
    let (e1, e2) = tier_edges(z, side);
    for &(cs, cz) in &CHIMNEYS {
        if cs == side && (z - cz).abs() < CHIMNEY_HALF && s > -0.5 && s < e2 + 3.0 {
            return (BANK_Y, Zone::Bank);
        }
    }
    if s < 0.0 {
        if z < LIP_Z {
            return (0.5, Zone::Void);
        }
        let rh = river_half(z);
        if d < rh && z < 36.0 {
            return (RIVER_Y - RIVER_DEPTH, Zone::RiverBed);
        }
        let bay = smoothstep(20.0, 34.0, z);
        let bank = 14.5 + 1.5 * smoothstep(rh, rh + 10.0, d) + 0.4 * value2(x * 0.15, z * 0.15, seed ^ 7);
        let h = bank + (BANK_Y - bank) * bay;
        return ((h / VS).round() * VS, Zone::Bank);
    }
    let ramp = |tier: u8| RAMPS.iter().find(|r| r.side == side && r.tier == tier).and_then(|r| r.height(z));
    if s < e1 {
        return (ramp(1).unwrap_or(L1_Y), Zone::Ledge);
    }
    if s < e2 {
        return (ramp(2).unwrap_or(L2_Y), Zone::Ledge);
    }
    if s < e2 + RAMP3_W
        && let Some(h) = ramp(3)
    {
        return (h, Zone::Ledge);
    }
    (plateau(x, z, s - e2, seed), Zone::Plateau)
}

/// Forested plateau (with mountains rising toward the world edges); `inset` = metres from the
/// cliff-top edge (hills fade in away from it).
fn plateau(x: f32, z: f32, inset: f32, seed: u32) -> f32 {
    let hills = 2.2 * fbm2(x * 0.03, z * 0.03, seed ^ 0x11, 3) * smoothstep(0.0, 8.0, inset);
    let mtn = (18.0 + 5.0 * fbm2(x * 0.02, z * 0.02, seed ^ 0x12, 2)) * smoothstep(64.0, 79.0, x.abs().max(z - 30.0));
    (TOP_Y + hills + mtn).min(93.0)
}

/// North of the headwall: the upper pool basin, its shore, islets, the inlet and the plateau.
fn upper(x: f32, z: f32, seed: u32) -> (f32, Zone) {
    let dp = ((x / 27.0) * (x / 27.0) + ((z - 77.0) / 19.0) * ((z - 77.0) / 19.0)).sqrt();
    let lip = x.abs() <= FALLS_HALF && z < 66.0;
    let inlet = x.abs() < 5.0 + 1.5 * value2(z * 0.1, 0.0, seed ^ 3) && z > 90.0;
    let bed = POOL_Y - 1.5;
    // Islets.
    let islet = |c: Vec2, r: f32, top: f32| {
        let d = (Vec2::new(x, z) - c).length() / r;
        (d < 1.0).then(|| bed + (top - bed) * (1.0 - d * d).sqrt())
    };
    let isl = islet(Vec2::new(-9.0, 81.0), 4.5, 61.0).or_else(|| islet(Vec2::new(10.0, 71.0), 3.5, 60.5));
    if let Some(h) = isl {
        return (((h.max(POOL_Y + 0.5)) / VS).round() * VS, Zone::Shore);
    }
    if dp < 1.0 || lip || inlet {
        return (bed, Zone::PoolBed);
    }
    let shore = POOL_Y + 0.5 + (TOP_Y - POOL_Y - 0.5) * smoothstep(1.0, 1.45, dp);
    let flat = plateau(x, z, 20.0, seed);
    let k = smoothstep(1.3, 1.7, dp);
    let h = shore + (flat - shore) * k;
    ((h / VS).round() * VS, if k > 0.5 { Zone::Plateau } else { Zone::Shore })
}

/// Strata: horizontal bands (1.5 m) that bulge (+) or recede (−) by whole voxels along the walls.
fn strata_offset(y: i32, along: f32, seed: u32) -> i32 {
    let jit = (2.0 * value2(along * 0.02, 0.5, seed ^ 0x51)) as i32;
    let b = (y + jit).div_euclid(3);
    const TABLE: [i32; 8] = [0, 1, -1, 2, 0, 1, 0, -1];
    let base = TABLE[(hash3(b, 0, 0, seed ^ 0x5354) & 7) as usize];
    base + (value2(along * 0.05, b as f32 * 1.7, seed ^ 0x52) * 0.9).round() as i32
}

/// Wall material by height band.
fn strata_mat(y: i32, along: f32, seed: u32) -> Material {
    let jit = (1.6 * value2(along * 0.03, 1.5, seed ^ 0x53)) as i32;
    let b = (y + jit).div_euclid(3);
    const TABLE: [Material; 7] =
        [mat::VALLEY_CLIFF, mat::VALLEY_STRATA, mat::VALLEY_CLIFF, mat::VALLEY_LEDGE, mat::VALLEY_CLIFF, mat::VALLEY_STRATA, mat::VALLEY_CLIFF];
    TABLE[(hash3(b, 1, 0, seed ^ 0x5355) % 7) as usize]
}

fn surface_mat(zone: Zone, x: f32, z: f32, h: f32, seed: u32) -> Material {
    let n = value2(x * 0.21, z * 0.21, seed ^ 0x61);
    match zone {
        Zone::Void => mat::VALLEY_CORE,
        Zone::RiverBed | Zone::PoolBed => mat::RIVER_STONE,
        Zone::Bank => {
            let d = (x - centre_x(z)).abs() - river_half(z);
            if d < 2.5 || n > 0.55 { mat::RIVER_STONE } else { mat::VALLEY_MOSS }
        }
        Zone::Ledge => {
            if n > 0.35 {
                mat::VALLEY_LEDGE
            } else {
                mat::VALLEY_MOSS
            }
        }
        Zone::Plateau | Zone::Shore => {
            if h > 84.0 || (h > 74.0 && n > 0.0) {
                mat::VALLEY_LEDGE
            } else {
                mat::VALLEY_MOSS
            }
        }
    }
}

/// The heightfield + zones on the voxel column grid.
struct Field {
    x0: i32,
    z0: i32,
    nx: i32,
    nz: i32,
    h: Vec<f32>,
    zone: Vec<Zone>,
}

impl Field {
    fn new(w: &VoxelWorld, seed: u32) -> Self {
        let (lo, hi) = (w.voxel_min(), w.voxel_max());
        let (nx, nz) = (hi.x - lo.x, hi.z - lo.z);
        let mut h = Vec::with_capacity((nx * nz) as usize);
        let mut zone = Vec::with_capacity((nx * nz) as usize);
        for iz in 0..nz {
            for ix in 0..nx {
                let (x, z) = (((lo.x + ix) as f32 + 0.5) * VS, ((lo.z + iz) as f32 + 0.5) * VS);
                let (hh, zz) = column(x, z, seed);
                h.push(hh);
                zone.push(zz);
            }
        }
        Self { x0: lo.x, z0: lo.z, nx, nz, h, zone }
    }
    #[inline]
    fn idx(&self, vx: i32, vz: i32) -> usize {
        let ix = (vx - self.x0).clamp(0, self.nx - 1);
        let iz = (vz - self.z0).clamp(0, self.nz - 1);
        (ix + self.nx * iz) as usize
    }
    #[inline]
    fn h(&self, vx: i32, vz: i32) -> f32 {
        self.h[self.idx(vx, vz)]
    }
}

/// Fill the terrain from the field: bedrock at y = 0, strata walls, surface materials.
fn terrain(g: &mut Gen, f: &Field, seed: u32) {
    let ny = g.w.voxel_max().y;
    let mut col: Vec<Material> = vec![AIR; ny as usize];
    for iz in 0..f.nz {
        for ix in 0..f.nx {
            let (vx, vz) = (f.x0 + ix, f.z0 + iz);
            let (x, z) = ((vx as f32 + 0.5) * VS, (vz as f32 + 0.5) * VS);
            let i = (ix + f.nx * iz) as usize;
            let (h, zone) = (f.h[i], f.zone[i]);
            // Outward (into-the-rock) axis for the strata offsets and the along-wall coordinate.
            let g0 = centre_x(z);
            let (dx, dz, along) = if z > 52.0 && (x - g0).abs() < 40.0 {
                (0, 1, x)
            } else if x < g0 {
                (-1, 0, z)
            } else {
                (1, 0, z)
            };
            let mut lo_h = h;
            let mut hi_h = h;
            for k in 1..=3 {
                for sgn in [-1, 1] {
                    let hh = f.h(vx + dx * k * sgn, vz + dz * k * sgn);
                    lo_h = lo_h.min(hh);
                    hi_h = hi_h.max(hh);
                }
            }
            let top_v = ((hi_h / VS).ceil() as i32).min(ny);
            col[..top_v.max(1) as usize].fill(AIR);
            col[0] = mat::VALLEY_CORE;
            if hi_h - lo_h < 1.0 {
                let t = ((h / VS).round() as i32).min(ny);
                for y in 1..t {
                    col[y as usize] = mat::VALLEY_CLIFF;
                }
            } else {
                for y in 1..top_v {
                    let o = strata_offset(y, along, seed);
                    let hs = f.h(vx + dx * o, vz + dz * o);
                    if (y as f32 + 0.5) * VS < hs {
                        col[y as usize] = strata_mat(y, along, seed);
                    }
                }
            }
            // Surface voxels: anything solid with air above gets the zone's surface material if it's
            // at (or near) the base height; strata lips keep their band colour.
            let mut top = 0;
            for y in (1..top_v).rev() {
                if col[y as usize] != AIR {
                    top = y;
                    break;
                }
            }
            if top > 0 && ((top + 1) as f32 * VS - h).abs() < 0.6 && zone != Zone::Void {
                col[top as usize] = surface_mat(zone, x, z, h, seed);
            }
            // Emit runs.
            let mut y = 0;
            while y < top_v.max(1) {
                let m = col[y as usize];
                let mut e = y + 1;
                while e < top_v && col[e as usize] == m {
                    e += 1;
                }
                if m != AIR {
                    g.w.fill_column(vx, vz, y, e, m);
                }
                y = e;
            }
        }
    }
    // Collapse hints along the bedrock.
    let (lo, hi) = (g.w.voxel_min(), g.w.voxel_max());
    let mut z = lo.z;
    while z < hi.z {
        let mut x = lo.x;
        while x < hi.x {
            g.w.add_anchor_hint(IVec3::new(x, 0, z));
            x += 32;
        }
        z += 32;
    }
}

fn fill_box(g: &mut Gen, min: Vec3, max: Vec3, mut m: impl FnMut(Vec3) -> Material) {
    let lo = (min / VS).floor().as_ivec3();
    let hi = (max / VS).ceil().as_ivec3() - IVec3::ONE;
    g.w.fill_with(lo, hi, |v, _| Some(m(vcenter(v))));
}

/// Pedestals (with stairs down to the bank), the falls' buttresses, the falls, their rest ledges
/// and the downstream lip's falls.
fn falls_and_pedestals(g: &mut Gen, seed: u32) {
    // Buttresses either side of the falls (flat faces: kick between one and the falls to climb).
    for sx in [-1.0f32, 1.0] {
        let (a, b) = (sx * FALLS_HALF, sx * (FALLS_HALF + 3.5));
        fill_box(g, Vec3::new(a.min(b), 0.5, 53.0), Vec3::new(a.max(b), POOL_Y + 0.5, HEAD_Z + 0.5), |p| {
            if p.y > POOL_Y { mat::VALLEY_MOSS } else { strata_mat((p.y / VS) as i32, p.z, seed) }
        });
    }
    // Pedestals: dressed blocks with shaded joints; an indestructible heart.
    for sx in [-1.0f32, 1.0] {
        let (a, b) = (sx * PED_X.0, sx * PED_X.1);
        fill_box(g, Vec3::new(a.min(b), 0.5, PED_Z.0), Vec3::new(a.max(b), PEDESTAL_TOP, PED_Z.1), |p| {
            let joint = (p.y / 2.0).rem_euclid(1.0) < 0.25 || ((p.x + p.z + (p.y / 2.0).floor() * 1.5) / 3.0).rem_euclid(1.0) < 0.17;
            if p.y > PEDESTAL_TOP - VS {
                mat::VALLEY_LEDGE
            } else if joint {
                mat::STATUE_SHADE
            } else {
                mat::VALLEY_LEDGE
            }
        });
        let c = Vec3::new(sx * STATUE_X, 0.0, STATUE_Z);
        fill_box(g, Vec3::new(c.x - 4.0, 0.5, c.z - 8.0), Vec3::new(c.x + 4.0, PEDESTAL_TOP - 1.5, c.z + 8.0), |_| mat::STATUE_CORE);
        g.w.add_anchor_hint(((c + Vec3::Y * 4.0) / VS).floor().as_ivec3());
        // Stairs: the west statue's on its south face, the east statue's on its north face (the
        // free strip in front of each statue's lowered hand), 0.5 m risers.
        for k in 0..6 {
            let top = BANK_Y + 0.5 * (k + 1) as f32;
            let (z0, z1) = if sx < 0.0 {
                (PED_Z.0 - 6.0 + k as f32, PED_Z.0)
            } else {
                (PED_Z.1, (PED_Z.1 + 3.5 - 0.5 * k as f32).min(HEAD_Z))
            };
            let xc = sx * (STATUE_X - 2.0);
            fill_box(g, Vec3::new(xc - 2.5, 0.5, z0), Vec3::new(xc + 2.5, top, z1), |p| {
                if p.y > top - VS { mat::VALLEY_LEDGE } else { mat::VALLEY_CLIFF }
            });
        }
    }
    // The falls: a 1.5 m curtain of white water from the pool's lip down to the bedrock.
    fill_box(g, Vec3::new(-FALLS_HALF, 0.5, FALLS_Z), Vec3::new(FALLS_HALF, POOL_Y, HEAD_Z), |p| {
        let streak = value2(p.x * 0.7, p.y * 0.05 + p.x * 0.13, seed ^ 0x77) > 0.35;
        if streak { mat::WATERFALL_BLUE } else { mat::WATERFALL }
    });
    // Rest ledges jutting out of the falls (like the reference's two rocks).
    for (c, r) in [(Vec3::new(-5.0, 40.0, FALLS_Z - 0.6), 2.3f32), (Vec3::new(6.0, 28.0, FALLS_Z - 0.6), 2.5)] {
        rock(g, c, Vec3::new(r * 1.2, r * 0.75, r), seed);
    }
    // Downstream lip: the river pours over into the void.
    let gl = centre_x(LIP_Z);
    let rh = river_half(LIP_Z);
    fill_box(g, Vec3::new(gl - rh, 0.5, LIP_Z - 1.5), Vec3::new(gl + rh, RIVER_Y, LIP_Z), |p| {
        if value2(p.x * 0.7, p.y * 0.1, seed ^ 0x78) > 0.3 { mat::WATERFALL_BLUE } else { mat::WATERFALL }
    });
}

/// A mossy boulder (only replaces air).
fn rock(g: &mut Gen, c: Vec3, r: Vec3, seed: u32) {
    let lo = ((c - r * 1.3) / VS).floor().as_ivec3();
    let hi = ((c + r * 1.3) / VS).floor().as_ivec3();
    g.w.fill_with(lo, hi, |v, old| {
        if old != AIR {
            return None;
        }
        let p = vcenter(v);
        let e = ((p - c) / r).length() + 0.2 * crate::noise::value3(p.x * 0.4, p.y * 0.4, p.z * 0.4, seed ^ 0x72);
        if e >= 1.0 {
            return None;
        }
        Some(if (p.y - c.y) > r.y * 0.45 && value2(p.x * 0.5, p.z * 0.5, seed ^ 0x73) > -0.2 { mat::VALLEY_MOSS } else { mat::VALLEY_CLIFF })
    });
}

/// A tall rock stack standing in the river (cover, tether anchor; topples if cut).
fn stack(g: &mut Gen, x: f32, z: f32, r: f32, top: f32, seed: u32) {
    let base = RIVER_Y - RIVER_DEPTH - 1.0;
    let lo = IVec3::new(((x - r * 1.4) / VS) as i32 - 1, (base / VS) as i32, ((z - r * 1.4) / VS) as i32 - 1);
    let hi = IVec3::new(((x + r * 1.4) / VS) as i32 + 1, (top / VS) as i32 + 1, ((z + r * 1.4) / VS) as i32 + 1);
    g.w.fill_with(lo, hi, |v, old| {
        if old != AIR {
            return None;
        }
        let p = vcenter(v);
        let t = ((p.y - base) / (top - base)).clamp(0.0, 1.0);
        let band = strata_offset(v.y, p.x + p.z, seed) as f32 * 0.35;
        let rr = r * (1.15 - 0.35 * t) + band + 0.4 * value2(p.x * 0.5 + p.y * 0.2, p.z * 0.5, seed ^ 0x74);
        let d = Vec2::new(p.x - x, p.z - z).length();
        if d >= rr || p.y > top - 1.2 * (d / r) {
            return None;
        }
        Some(if p.y > top - 1.0 { mat::VALLEY_MOSS } else { strata_mat(v.y, p.x + p.z, seed) })
    });
}

/// A pine: trunk and four stacked cones of needles.
fn pine(g: &mut Gen, x: f32, z: f32, height: f32) {
    let Some(ground) = g.ground(x, z, 95.0) else { return };
    let trunk_top = ground + height * 0.85;
    g.w.fill_with(
        IVec3::new(((x - 0.5) / VS) as i32, ((ground - 1.0) / VS) as i32, ((z - 0.5) / VS) as i32),
        IVec3::new(((x + 0.5) / VS) as i32, (trunk_top / VS) as i32, ((z + 0.5) / VS) as i32),
        |_, old| (old == AIR || crate::arena::is_foliage(old)).then_some(mat::DARK_WOOD),
    );
    let r0 = height * 0.26;
    for k in 0..4 {
        let kf = k as f32;
        let y0 = ground + height * (0.25 + 0.18 * kf);
        let r = r0 * (1.0 - 0.2 * kf);
        let hgt = height * 0.28;
        let lo = IVec3::new(((x - r) / VS) as i32 - 1, (y0 / VS) as i32, ((z - r) / VS) as i32 - 1);
        let hi = IVec3::new(((x + r) / VS) as i32 + 1, ((y0 + hgt) / VS) as i32, ((z + r) / VS) as i32 + 1);
        g.w.fill_with(lo, hi, |v, old| {
            if old != AIR {
                return None;
            }
            let p = vcenter(v);
            let t = ((p.y - y0) / hgt).clamp(0.0, 1.0);
            (Vec2::new(p.x - x, p.z - z).length() < r * (1.0 - t)).then_some(mat::PINE)
        });
    }
}

/// Is (x, z) within `r` of any point in `pts` (horizontal)?
fn near(pts: &[Vec2], x: f32, z: f32, r: f32) -> bool {
    pts.iter().any(|p| p.distance(Vec2::new(x, z)) < r)
}

/// Forest on the plateau, bushes along the cliff-top edges, a few trees on the banks and islets.
fn vegetation(g: &mut Gen, f: &Field, keep_clear: &[Vec2], seed: u32) {
    let zone_at = |x: f32, z: f32| f.zone[f.idx((x / VS).floor() as i32, (z / VS).floor() as i32)];
    let h_at = |x: f32, z: f32| f.h((x / VS).floor() as i32, (z / VS).floor() as i32);
    // Plateau forest on a jittered 10 m grid.
    let mut k = 0u32;
    let mut z = -106.0;
    while z < 108.0 {
        let mut x = -76.0;
        while x < 76.0 {
            k += 1;
            let jx = x + 8.0 * (unit(hash3(k as i32, 1, 0, seed ^ 0x7431)) - 0.5);
            let jz = z + 8.0 * (unit(hash3(k as i32, 2, 0, seed ^ 0x7431)) - 0.5);
            x += 13.0;
            let h = h_at(jx, jz);
            if zone_at(jx, jz) != Zone::Plateau || h > TOP_Y + 12.0 || near(keep_clear, jx, jz, 9.0) {
                continue;
            }
            // Keep a clear margin at the cliff-top edge (bushes go there) and off the ramps.
            let side = if jx < centre_x(jz) { -1.0 } else { 1.0 };
            let (_, e2) = tier_edges(jz, side);
            let s = (jx - centre_x(jz)).abs() - half_width(jz);
            if jz < HEAD_Z && s < e2 + RAMP3_W + 3.0 {
                continue;
            }
            let roll = unit(hash3(k as i32, 3, 0, seed ^ 0x7431));
            if roll < 0.45 {
                continue;
            }
            if roll < 0.75 {
                pine(g, jx, jz, 9.0 + 7.0 * unit(hash3(k as i32, 4, 0, seed)));
            } else {
                let leaves = if roll < 0.9 { mat::LEAVES_GREEN } else { mat::PINE };
                g.tree(jx, jz, 95.0, 11.0 + 5.0 * unit(hash3(k as i32, 5, 0, seed)), 0.8, leaves, 4.0);
            }
        }
        z += 13.0;
    }
    // Bushes along the cliff-top edges (the reference's shrubs), skipping ramps and chimneys.
    for side in [-1.0f32, 1.0] {
        let mut z = -104.0;
        let mut n = 0;
        while z < HEAD_Z - 2.0 {
            n += 1;
            let zz = z + 3.0 * (unit(hash3(n, side as i32, 7, seed)) - 0.5);
            z += 6.5;
            if unit(hash3(n, side as i32, 8, seed)) < 0.35 {
                continue;
            }
            let (_, e2) = tier_edges(zz, side);
            let x = wall_x(zz, side) + side * (e2 + 1.2);
            let on_ramp = RAMPS.iter().any(|r| r.side == side && r.tier == 3 && r.height(zz).is_some());
            let at_chimney = CHIMNEYS.iter().any(|&(cs, cz)| cs == side && (zz - cz).abs() < 4.0);
            if on_ramp || at_chimney || near(keep_clear, x, zz, 6.0) {
                continue;
            }
            let Some(gy) = g.ground(x, zz, 95.0) else { continue };
            let r = 1.3 + 1.0 * unit(hash3(n, side as i32, 9, seed));
            let leaves = if unit(hash3(n, side as i32, 10, seed)) < 0.5 { mat::LEAVES_GREEN } else { mat::PINE };
            g.blob(Vec3::new(x, gy + r * 0.5, zz), Vec3::new(r * 1.3, r * 0.9, r * 1.3), leaves);
        }
    }
    // A few trees at the cliff feet on the banks (clear of the surf lines), and a pine on an islet.
    for (k, &(side, z)) in [(-1.0f32, -50.0f32), (1.0, 4.0), (-1.0, -74.0), (1.0, -86.0)].iter().enumerate() {
        let x = wall_x(z, side) - side * 3.0;
        if near(keep_clear, x, z, 7.0) {
            continue;
        }
        g.tree(x, z, BANK_Y + 6.0, 12.0 + 2.0 * k as f32 % 5.0, 0.8, mat::LEAVES_GREEN, 4.0);
    }
    pine(g, -9.0, 81.0, 9.0);
}

/// Surf lines along the curved gorge walls (descending downstream).
pub fn surf_lines() -> Vec<SurfLine> {
    let line = |side: f32, z0: f32, y0: f32, z1: f32, y1: f32, inset: f32| {
        let a = Vec3::new(wall_x(z0, side) - side * inset, y0, z0);
        let b = Vec3::new(wall_x(z1, side) - side * inset, y1, z1);
        SurfLine { a, b, face_deg: 60.0, width: 8.0 }
    };
    vec![line(-1.0, 22.0, 48.0, -40.0, 28.0, 5.0), line(1.0, -10.0, 46.0, -72.0, 27.0, 8.5)]
}

/// Walkable water: the river (segments following the meander, from the plunge hole's rim to the
/// downstream lip), the upper pool and its inlet.
pub fn water_sheets() -> Vec<WaterSheet> {
    let mut out = Vec::new();
    let (z_top, n) = (35.0f32, 11);
    let seg = (z_top - LIP_Z) / n as f32;
    for i in 0..n {
        let hi = z_top - seg * i as f32;
        let lo = hi - seg;
        let zc = (hi + lo) * 0.5;
        let last = i == n - 1;
        let slope = if last { 0.0 } else { centre_x(zc + 0.5) - centre_x(zc - 0.5) };
        let len = seg * 0.5 * (1.0 + slope * slope).sqrt();
        out.push(WaterSheet {
            center: Vec3::new(centre_x(zc), RIVER_Y, zc),
            half: Vec2::new(12.5, if last { seg * 0.5 } else { len + 0.6 }),
            yaw: datan2(slope, 1.0),
            depth: RIVER_DEPTH,
        });
    }
    // The pool (behind the headwall) and the strip over the falls' lip (only as wide as the falls).
    out.push(WaterSheet { center: Vec3::new(0.0, POOL_Y, (HEAD_Z + 96.0) * 0.5), half: Vec2::new(27.0, (96.0 - HEAD_Z) * 0.5), yaw: 0.0, depth: 1.5 });
    out.push(WaterSheet { center: Vec3::new(0.0, POOL_Y, (FALLS_Z + HEAD_Z) * 0.5), half: Vec2::new(FALLS_HALF, (HEAD_Z - FALLS_Z) * 0.5), yaw: 0.0, depth: 1.5 });
    out.push(WaterSheet { center: Vec3::new(0.0, POOL_Y, 103.0), half: Vec2::new(7.0, 9.0), yaw: 0.0, depth: 1.5 });
    out
}

/// The two statues.
pub fn statue_specs() -> [StatueSpec; 2] {
    [
        StatueSpec { base: Vec3::new(-STATUE_X, PEDESTAL_TOP, STATUE_Z), facing: 1.0, hair: Hair::Spiky },
        StatueSpec { base: Vec3::new(STATUE_X, PEDESTAL_TOP, STATUE_Z), facing: -1.0, hair: Hair::Straight },
    ]
}

/// Spawn targets (x, z, search-from y): statue pedestals (the free strip in front of each statue),
/// banks, cliff tops (bay + downstream, both sides), the upper pool's shore. Roughly symmetric under
/// a half turn about the gorge axis, so no side is favoured.
fn spawn_targets() -> [(f32, f32, f32); 10] {
    [
        (-STATUE_X + 3.5, PED_Z.0 + 1.6, PEDESTAL_TOP + 4.0),
        (STATUE_X - 3.5, PED_Z.1 - 1.6, PEDESTAL_TOP + 4.0),
        (centre_x(-22.0) - 17.0, -22.0, BANK_Y + 4.0),
        (centre_x(-62.0) + 17.0, -62.0, BANK_Y + 4.0),
        (-56.0, 38.0, TOP_Y + 8.0),
        (56.0, 22.0, TOP_Y + 8.0),
        (-44.0, -64.0, TOP_Y + 8.0),
        (44.0, -48.0, TOP_Y + 8.0),
        (-30.0, 86.0, TOP_Y + 4.0),
        (30.0, 68.0, TOP_Y + 4.0),
    ]
}

/// Build the Valley of the End. Deterministic for a given seed.
pub fn generate_valley(seed: u64) -> (VoxelWorld, MapInfo) {
    let s32 = (seed as u32) ^ 0x7661_6c6c;
    let mut g = Gen::new(VoxelWorld::centered(VALLEY_SIZE_VOXELS), seed ^ 0x656e_6421);
    let field = Field::new(&g.w, s32);
    terrain(&mut g, &field, s32);
    falls_and_pedestals(&mut g, s32);
    for (k, s) in statue_specs().iter().enumerate() {
        statues::build(&mut g.w, s, s32 ^ k as u32);
    }
    // Rock stacks in the river, boulders on the banks, rocks on the plunge hole's rim.
    stack(&mut g, centre_x(-40.0) + 1.5, -40.0, 3.2, 38.0, s32);
    stack(&mut g, centre_x(-70.0) - 2.0, -70.0, 2.8, 33.0, s32 ^ 1);
    stack(&mut g, centre_x(10.0) + 2.5, 10.0, 2.4, 25.0, s32 ^ 2);
    let targets = spawn_targets();
    let keep: Vec<Vec2> = targets.iter().map(|t| Vec2::new(t.0, t.1)).collect();
    for k in 0..16 {
        let z = 30.0 - 8.0 * k as f32 + 3.0 * value2(k as f32 * 3.1, 0.0, s32 ^ 0x81);
        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
        let d = river_half(z) + 3.0 + 9.0 * (0.5 + 0.5 * value2(k as f32 * 1.7, 2.0, s32 ^ 0x82));
        let x = centre_x(z) + side * d;
        if near(&keep, x, z, 5.0) || (x - wall_x(z, side)).abs() < 2.0 {
            continue;
        }
        let r = 1.2 + 1.3 * (0.5 + 0.5 * value2(k as f32 * 2.3, 5.0, s32 ^ 0x83));
        if let Some(gy) = g.ground(x, z, BANK_Y + 6.0) {
            rock(&mut g, Vec3::new(x, gy + r * 0.25, z), Vec3::new(r * 1.2, r * 0.8, r), s32 ^ k);
        }
    }
    for k in 0..9 {
        let a = TAU * (k as f32 + 0.3) / 9.0;
        let p = PLUNGE_C + Vec2::new(dcos(a) * (PLUNGE_R.x + 1.2), dsin(a) * (PLUNGE_R.y + 1.2));
        if p.y > FALLS_Z - 3.0 {
            continue;
        }
        if let Some(gy) = g.ground(p.x, p.y, BANK_Y + 4.0) {
            rock(&mut g, Vec3::new(p.x, gy, p.y), Vec3::new(1.2, 0.8, 1.0), s32 ^ (0x90 + k));
        }
    }
    // Arch (L2 to L2) and the overhanging shelves.
    arch(&mut g, s32);
    for &(side, z) in &SHELVES {
        let (_, e2) = tier_edges(z, side);
        let edge = wall_x(z, side) + side * e2;
        let c = Vec3::new(edge - side * 3.0, TOP_Y - 2.4, z);
        let r = Vec3::new(5.0, 2.2, 4.0);
        rock(&mut g, c, r, s32 ^ 0xa0);
    }
    vegetation(&mut g, &field, &keep, s32);

    // Spawns.
    let mut spawns: Vec<Vec3> = Vec::new();
    for (x, z, from) in targets {
        if let Some(s) = g.spawn_near(x, z, from, &spawns) {
            spawns.push(s);
        }
    }

    let mut w = g.w;
    let pruned = w.prune_unanchored();
    // Nobody ever sees the terrain's faces toward the outside of the world (the mountain rim is the
    // edge; the side blast zones are 8 m beyond it): don't mesh them.
    w.set_cull_outside(true);
    w.mark_all_dirty();
    let wb = w.bounds();
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb {
            min: Vec3::new(wb.min.x - 8.0, VOID_Y, wb.min.z - 8.0),
            max: Vec3::new(wb.max.x + 8.0, wb.max.y + 36.0, wb.max.z + 8.0),
        },
        void_y: VOID_Y,
        center: Vec3::new(0.0, BANK_Y, 24.0),
        // Afternoon sun low in the south-west, behind the classic view (looking north up the gorge
        // at the falls), so the mist catches a rainbow.
        sun_dir: Vec3::new(-0.42, 0.62, -0.66).normalize(),
        sky_zenith: [0.20, 0.46, 0.92],
        sky_horizon: [0.72, 0.86, 0.98],
        fog_color: [0.70, 0.82, 0.93],
        ambient: 0.42,
        gravity: None,
        surf_lines: surf_lines(),
        water_sheets: water_sheets(),
        water_walk: true,
        slipstream: None,
        mechanics: Default::default(),
        interactables: Default::default(),
    };
    let districts = vec![
        District { name: "statue_bay", center: Vec3::new(0.0, BANK_Y, 40.0), area: [-37.0, 28.0, 37.0, 58.0] },
        District { name: "gorge", center: Vec3::new(0.0, BANK_Y, -30.0), area: [-34.0, -96.0, 34.0, 28.0] },
        District { name: "upper_pool", center: Vec3::new(0.0, POOL_Y, 77.0), area: [-30.0, 58.0, 30.0, 100.0] },
        District { name: "west_rim", center: Vec3::new(-52.0, TOP_Y, -20.0), area: [-76.0, -110.0, -34.0, 58.0] },
        District { name: "east_rim", center: Vec3::new(52.0, TOP_Y, -20.0), area: [34.0, -110.0, 76.0, 58.0] },
    ];
    let info = MapInfo {
        name: "Valley of the End",
        arena,
        water_y: WATER_Y,
        water_color: [0.30, 0.60, 0.62],
        water_deep: [0.08, 0.26, 0.30],
        sun_color: [1.0, 0.94, 0.80],
        sky_mid: [0.50, 0.70, 0.94],
        fog_start: 90.0,
        fog_end: 420.0,
        neon_boost: 0.0,
        districts,
        routes: routes(),
        pruned_voxels: pruned,
    };
    (w, info)
}

/// The natural arch across the downstream gorge, from the west L2 ledge to the east one.
fn arch(g: &mut Gen, seed: u32) {
    let z = ARCH_Z;
    let mid = |side: f32| {
        let (e1, e2) = tier_edges(z, side);
        Vec3::new(wall_x(z, side) + side * (e1 + e2) * 0.5, L2_Y - 3.5, z)
    };
    let (a, b) = (mid(-1.0), mid(1.0));
    let n = ((b.x - a.x) / 0.4) as i32;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let p = a + (b - a) * t + Vec3::Y * (9.0 * 4.0 * t * (1.0 - t));
        let thick = 1.0 + 0.6 * (1.0 - 4.0 * t * (1.0 - t));
        let r = Vec3::new(1.2, 2.2 * thick, 3.2 + 0.4 * value2(t * 9.0, 0.0, seed ^ 0xab));
        let lo = ((p - r) / VS).floor().as_ivec3();
        let hi = ((p + r) / VS).ceil().as_ivec3();
        g.w.fill_with(lo, hi, |v, old| {
            if old != AIR {
                return None;
            }
            let q = vcenter(v);
            let e = ((q - p) / r).length_squared();
            (e < 1.0).then(|| if q.y > p.y + r.y * 0.55 { mat::VALLEY_MOSS } else { strata_mat(v.y, q.x, seed) })
        });
    }
}

/// Authored lines (bots, camera, route tests):
/// - `canyon_surf` — the west surf line (ridge ends).
/// - `falls_climb` — from the bank in the corner of the falls and the east buttress up to the pool.
/// - `statue_swing` — from the west statue's north shoulder guard across to the east statue
///   (anchor: the east statue's head).
/// - `river_run` — along the river on the water, statue bay to the downstream lip.
pub fn routes() -> Vec<Route> {
    let [w, e] = statue_specs();
    let lines = surf_lines();
    vec![
        Route {
            name: "canyon_surf",
            from: "statue_bay",
            to: "gorge",
            kind: RouteKind::Mixed,
            waypoints: vec![lines[0].a, lines[0].b],
            anchors: vec![],
        },
        Route {
            name: "falls_climb",
            from: "statue_bay",
            to: "upper_pool",
            kind: RouteKind::Mixed,
            waypoints: vec![Vec3::new(FALLS_HALF - 1.2, BANK_Y, FALLS_Z - 1.0), Vec3::new(8.0, POOL_Y, 64.0)],
            anchors: vec![],
        },
        Route {
            name: "statue_swing",
            from: "statue_bay",
            to: "statue_bay",
            kind: RouteKind::Swing,
            waypoints: vec![w.pauldron_top(1.0), e.pauldron_top(-1.0)],
            anchors: vec![e.head_top()],
        },
        Route {
            name: "river_run",
            from: "statue_bay",
            to: "gorge",
            kind: RouteKind::Ground,
            // Weaves round the rock stacks (z 10, −40, −70).
            waypoints: (0..=12)
                .map(|k| {
                    let z = 32.0 - 10.0 * k as f32;
                    let off = if z > 20.0 { 0.0 } else if z > -55.0 { -4.5 } else { 4.5 };
                    Vec3::new(centre_x(z) + off, RIVER_Y, z)
                })
                .collect(),
            anchors: vec![],
        },
    ]
}

/// Debug / preview helper: voxel column top at (x, z) in metres (None = nothing).
pub fn top_at(w: &VoxelWorld, x: f32, z: f32) -> Option<f32> {
    w.top_solid_y((x / VS).floor() as i32, (z / VS).floor() as i32).map(|y| (y + 1) as f32 * VS)
}
