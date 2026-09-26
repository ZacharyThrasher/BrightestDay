//! **Turtle Lair** — a vaulted sewer lair under a strip of night-time New York rooftops
//! (DESIGN.md "Maps"; feature `lair`). Private use only: Teenage Mutant Ninja Turtles places/props
//! (Paramount/Nickelodeon) — see THIRD_PARTY_NOTICES.md; replace before any public release.
//!
//! Three layers on a 112 × 112 m block of earth over the void (+X east, +Z north, metres):
//!
//! ```text
//!  STREET (34 m)  tenement rooftops (42–52 m, water towers, fire escapes) · the avenue (z −2..12)
//!                 bridges the SUBWAY CHASM · west street with two open manholes · storm-drain
//!                 culvert (the surf chute) at the west edge · subway stairs on the east strip
//!  SEWERS (12 m)  ┌ T_N (shallow channel) ────────────────────────────┐ bridge ║ east
//!                 │ T_W   ┌ spoke N ┐                                  │   ║   platform
//!                 │ (dry, │  LAIR HUB (vault 30 m, sunken pit under   │   ║   (subway
//!                 │ bhop) │  the manhole light shaft; dojo, lab,      │ garage ═══ station,
//!                 │  green spoke W  arcade, garage, drop-in deck) ───┘   ║   stairs up)
//!                 │       └ SKATE HALL (hard-light half-pipe) ┐          ║
//!                 └ T_S (deep channel) ──────────────────────────────────┘ bridge
//! ```
//! - **Hub.** A 44 × 34 m brick barrel vault on indestructible ribs. The sunken bean-bag pit sits
//!   under the open central manhole (moonlight ring + light pool); a scrap contraption hangs from
//!   the vault on chains (tether anchor, falls if you shoot the chains). North band: dojo (tatami,
//!   weapon rack, tires, bag, dummies, Splinter's tree) and lab; a water channel with three arched
//!   bridges; west: arcade corner and the green-lit tunnel; east: workshop and the garage bay that
//!   opens onto the chasm; south: a mezzanine you "drop in" from onto the skate hall's deck.
//! - **Skate / surf.** The skate hall's half-pipe walls are two level hard-light surf lines
//!   ([`ArenaInfo::surf_lines`]) with voxel decks behind them; the storm-drain chute is a long
//!   descending surf line from a street-level culvert down into the hall, dumping riders straight
//!   through the pipe.
//! - **Sewers.** 7 m arched tunnels on indestructible rib rings: T_N (shallow channel), T_S (deep
//!   channel), T_W (dry — the 70 m bhop lane), junction chambers with ledge-lined manhole shafts up
//!   to the west street, big pipes as rails; brick bridges over the chasm to the station platform.
//! - **KOs.** The subway chasm (x 38..46) and the block edges drop to the void ([`VOID_Y`]);
//!   launched over the roofs is a star KO; far off the sides is a side KO (tether saves apply).
//!   Water is never lethal (voxel channels you can walk out of).
//! - **Destruction.** Brick, earth, buildings and props break; ribs, arches, bridge ribs, platform
//!   columns, bridge girders, building cores and the bedrock don't.
//! - **Light.** Emissive only: moonlight shafts, warm lamps, TV / arcade / pizza-sign neon, mutagen
//!   green in the west tunnel and the lab; cool blue night ambience and teal fog.

// Generator style: many small fixed-coordinate loops.
#![allow(clippy::too_many_arguments)]

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, SurfLine};
use crate::city::{CarSpec, City};
use crate::material::mat;
use crate::noise::{Rng, hash3, unit, value2};
use crate::world::VoxelWorld;

#[path = "lair_icons.rs"]
pub mod icons;
#[path = "lair_props.rs"]
pub mod props;

/// World dimensions in voxels (112 × 64 × 112 m).
pub const LAIR_SIZE_VOXELS: IVec3 = IVec3::new(224, 128, 224);
/// Half the block's footprint (m): the block spans −EDGE..EDGE in x and z.
pub const EDGE: f32 = 56.0;
/// Sewer walk level (m).
pub const FLOOR: f32 = 12.0;
/// Street level (m).
pub const STREET: f32 = 34.0;
/// Falling below this is the void KO.
pub const VOID_Y: f32 = 3.0;
/// The subway chasm (x range, m): open from the sky to the void along the whole block.
pub const CHASM: (f32, f32) = (38.0, 46.0);

/// The lair hub (x0, z0, x1, z1) and its vault.
pub const HUB: [f32; 4] = [-22.0, -10.0, 22.0, 24.0];
const HUB_WALL: f32 = 21.0;
const HUB_CROWN: f32 = 30.0;
/// Indestructible vault ribs (x).
const HUB_RIBS: [f32; 6] = [-20.0, -12.0, -4.0, 4.0, 12.0, 20.0];
/// The sunken pit (x0, z0, x1, z1) and its floor.
pub const PIT: [f32; 4] = [-6.0, 1.0, 6.0, 13.0];
pub const PIT_FLOOR: f32 = 10.5;
/// The central manhole (light shaft) over the pit.
pub const MANHOLE: Vec2 = Vec2::new(0.0, 7.0);
pub const MANHOLE_R: f32 = 1.75;
/// The hub's water channel (z range) and its water surface.
const CHANNEL_Z: (f32, f32) = (14.0, 17.0);
const CHANNEL_WATER: f32 = 10.5;
/// Bridges over the hub channel (x centres, 3 m wide).
const CHANNEL_BRIDGES: [f32; 3] = [-15.0, 0.0, 15.0];
/// The drop-in mezzanine against the hub's south wall (x0, z0, x1, z1) at deck height.
const MEZZ: [f32; 4] = [-6.0, -10.0, 6.0, -7.0];

/// The skate hall (x0, z0, x1, z1) and its vault.
pub const HALL: [f32; 4] = [-26.0, -30.0, 26.0, -12.0];
const HALL_WALL: f32 = 20.0;
const HALL_CROWN: f32 = 25.0;
/// Half-pipe: deck height, extent along x, ridge lines (z), face angle and slope width.
pub const DECK_Y: f32 = 16.5;
pub const PIPE_X: f32 = 13.0;
pub const RIDGE_N: f32 = -14.5;
pub const RIDGE_S: f32 = -27.5;
const PIPE_FACE_DEG: f32 = 60.0;
const PIPE_WIDTH: f32 = 5.2;

/// Storm-drain chute: ridge line from the culvert at the west edge down into the skate hall.
pub const CHUTE_Z: f32 = -21.0;
pub const CHUTE_TOP: Vec3 = Vec3::new(-55.0, 35.0, CHUTE_Z);
pub const CHUTE_END: Vec3 = Vec3::new(-27.0, 16.5, CHUTE_Z);
/// Half width of the box culvert, and how far its floor lies under the ridge (below the faces'
/// bottom edges: a gutter you can walk down if you fall off the line).
const CHUTE_R: f32 = 3.5;
const CHUTE_BELOW: f32 = 4.4;
const CHUTE_FACE_DEG: f32 = 58.0;
const CHUTE_WIDTH: f32 = 4.0;

/// Tunnels: centre lines, half width, wall top and crown above the floor.
pub const T_N_Z: f32 = 36.0;
pub const T_S_Z: f32 = -42.0;
pub const T_W_X: f32 = -44.0;
const T_HW: f32 = 3.5;
const T_WALL: f32 = 4.0;
const T_CROWN: f32 = 7.5;
/// Junction chambers (with manhole shafts up to the west street).
pub const JUNCTIONS: [Vec2; 2] = [Vec2::new(T_W_X, T_N_Z), Vec2::new(T_W_X, T_S_Z)];
const SHAFT_HALF: f32 = 2.5;
/// The south spoke (skate hall → T_S) runs along z at this x, just east of the south deck.
const SPOKE_S_X: f32 = 16.0;

/// The east strip: station platform (x0, z0, x1, z1) with its ceiling, and the stair well.
pub const PLATFORM: [f32; 4] = [46.0, -50.0, 54.5, 50.0];
const PLATFORM_CEIL: f32 = 20.0;
pub const STAIRWELL: [f32; 4] = [47.0, -24.0, 55.0, -8.0];

/// Surface: the avenue (z range) and the west street (x range).
pub const AVENUE: (f32, f32) = (-2.0, 12.0);
pub const WEST_ST: (f32, f32) = (-48.0, -36.0);

/// A tenement: footprint (x0, z0, x1, z1) and roof height.
#[derive(Clone, Copy, Debug)]
pub struct Building {
    pub rect: [f32; 4],
    pub roof: f32,
}

/// The billboard on the north-west roof (base centre).
const ROOF_SIGN: Vec3 = Vec3::new(-33.0, 0.0, T_N_Z - 4.5);

/// The rooftops.
pub const BUILDINGS: [Building; 8] = [
    Building { rect: [-36.0, 16.0, -16.0, 56.0], roof: 46.0 },
    Building { rect: [-12.0, 20.0, 10.0, 56.0], roof: 52.0 },
    Building { rect: [14.0, 16.0, 38.0, 56.0], roof: 42.0 },
    Building { rect: [-36.0, -56.0, -16.0, -6.0], roof: 44.0 },
    Building { rect: [-12.0, -56.0, 10.0, -10.0], roof: 50.0 },
    Building { rect: [14.0, -56.0, 38.0, -6.0], roof: 46.0 },
    Building { rect: [46.0, 20.0, 56.0, 56.0], roof: 50.0 },
    Building { rect: [46.0, -56.0, 56.0, -26.0], roof: 44.0 },
];

const VS: f32 = 0.5;

#[inline]
fn vx(m: f32) -> i32 {
    (m / VS).floor() as i32
}

/// Centre of voxel `v` in metres.
#[inline]
fn vc(v: IVec3) -> Vec3 {
    (v.as_vec3() + Vec3::splat(0.5)) * VS
}

/// Hub vault ceiling height at `z`.
pub fn hub_ceiling(z: f32) -> f32 {
    let (zc, half) = ((HUB[1] + HUB[3]) * 0.5, (HUB[3] - HUB[1]) * 0.5);
    let k = ((z - zc) / half).clamp(-1.0, 1.0);
    HUB_WALL + (HUB_CROWN - HUB_WALL) * (1.0 - k * k).sqrt()
}

/// Chute ridge height at `x` (m).
pub fn chute_ridge(x: f32) -> f32 {
    CHUTE_TOP.y + (x - CHUTE_TOP.x) * (CHUTE_END.y - CHUTE_TOP.y) / (CHUTE_END.x - CHUTE_TOP.x)
}

// ------------------------------------------------------------------------------------------------
// Carved spaces

/// A carved volume. `inside(p, e)` = inside the volume grown by `e` metres (e = 0: the air; e = 1:
/// the air plus its 1 m brick lining).
#[derive(Clone, Copy, Debug)]
enum Space {
    /// Arched tunnel along x or z: across centre `c`, along range, half width, wall top and crown
    /// height above `floor`.
    Tunnel { along_x: bool, c: f32, a0: f32, a1: f32, hw: f32, wall: f32, crown: f32, floor: f32 },
    /// Room with a barrel vault running along x (profile across z).
    Vault { x0: f32, x1: f32, z0: f32, z1: f32, floor: f32, wall: f32, crown: f32 },
    /// Round chamber with a dome.
    Round { c: Vec2, r: f32, floor: f32, wall: f32, crown: f32 },
    /// Box.
    Cut { min: Vec3, max: Vec3 },
    /// Capsule tube.
    Tube { a: Vec3, b: Vec3, r: f32 },
    /// The chute's box culvert: a strip along x following the chute ridge, from `below` under it to
    /// `above` over it.
    Culvert { x0: f32, x1: f32, cz: f32, hw: f32, below: f32, above: f32 },
    /// The chute's open top: everything above the culvert floor in a strip along x.
    Trench { x0: f32, x1: f32, cz: f32, hw: f32 },
}

impl Space {
    fn inside(&self, p: Vec3, e: f32) -> bool {
        match *self {
            Space::Tunnel { along_x, c, a0, a1, hw, wall, crown, floor } => {
                let (al, ac) = if along_x { (p.x, p.z) } else { (p.z, p.x) };
                if al < a0 - e || al > a1 + e {
                    return false;
                }
                arch_profile((ac - c).abs(), p.y - floor, hw, wall, crown, e)
            }
            Space::Vault { x0, x1, z0, z1, floor, wall, crown } => {
                if p.x < x0 - e || p.x > x1 + e {
                    return false;
                }
                arch_profile((p.z - (z0 + z1) * 0.5).abs(), p.y - floor, (z1 - z0) * 0.5, wall - floor, crown - floor, e)
            }
            Space::Round { c, r, floor, wall, crown } => {
                arch_profile(Vec2::new(p.x - c.x, p.z - c.y).length(), p.y - floor, r, wall - floor, crown - floor, e)
            }
            Space::Cut { min, max } => (p.cmpge(min - e).all()) && (p.cmplt(max + e).all()),
            Space::Tube { a, b, r } => {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                (p - (a + ab * t)).length() < r + e
            }
            Space::Culvert { x0, x1, cz, hw, below, above } => {
                let r = chute_ridge(p.x);
                p.x >= x0 - e && p.x < x1 + e && (p.z - cz).abs() < hw + e && p.y > r - below - e && p.y < r + above + e
            }
            Space::Trench { x0, x1, cz, hw } => {
                p.x >= x0 - e && p.x < x1 && (p.z - cz).abs() < hw + e && p.y > chute_ridge(p.x) - CHUTE_BELOW - e
            }
        }
    }

    /// Voxel box to scan (inclusive), grown by 1.5 m.
    fn bounds(&self) -> (Vec3, Vec3) {
        let g = Vec3::splat(1.5);
        match *self {
            Space::Tunnel { along_x, c, a0, a1, hw, crown, floor, .. } => {
                let (lo, hi) = if along_x {
                    (Vec3::new(a0, floor, c - hw), Vec3::new(a1, floor + crown, c + hw))
                } else {
                    (Vec3::new(c - hw, floor, a0), Vec3::new(c + hw, floor + crown, a1))
                };
                (lo - g, hi + g)
            }
            Space::Vault { x0, x1, z0, z1, floor, crown, .. } => (Vec3::new(x0, floor, z0) - g, Vec3::new(x1, crown, z1) + g),
            Space::Round { c, r, floor, crown, .. } => (Vec3::new(c.x - r, floor, c.y - r) - g, Vec3::new(c.x + r, crown, c.y + r) + g),
            Space::Cut { min, max } => (min - g, max + g),
            Space::Tube { a, b, r } => (a.min(b) - Vec3::splat(r) - g, a.max(b) + Vec3::splat(r) + g),
            Space::Culvert { x0, x1, cz, hw, below, above } => {
                (Vec3::new(x0, chute_ridge(x1) - below, cz - hw) - g, Vec3::new(x1, chute_ridge(x0) + above, cz + hw) + g)
            }
            Space::Trench { x0, x1, cz, hw } => {
                (Vec3::new(x0, chute_ridge(x1) - CHUTE_BELOW, cz - hw) - g, Vec3::new(x1, 64.0, cz + hw) + g)
            }
        }
    }
}

/// Arched cross-section: `u` = distance from the centre line, `h` = height above the floor.
#[inline]
fn arch_profile(u: f32, h: f32, hw: f32, wall: f32, crown: f32, e: f32) -> bool {
    let hwe = hw + e;
    if u >= hwe || h < -e {
        return false;
    }
    let k = u / hwe;
    h < wall + (crown - wall + e) * (1.0 - k * k).max(0.0).sqrt()
}

fn spaces() -> Vec<Space> {
    let f = FLOOR;
    let mut s = vec![
        // Lair hub + garage + skate hall.
        Space::Vault { x0: HUB[0], x1: HUB[2], z0: HUB[1], z1: HUB[3], floor: f, wall: HUB_WALL, crown: HUB_CROWN },
        Space::Vault { x0: HUB[2] - 0.5, x1: CHASM.0 + 0.5, z0: -4.0, z1: 16.0, floor: f, wall: 18.0, crown: 22.0 },
        Space::Vault { x0: HALL[0], x1: HALL[2], z0: HALL[1], z1: HALL[3], floor: f, wall: HALL_WALL, crown: HALL_CROWN },
        // Tunnels.
        Space::Tunnel { along_x: true, c: T_N_Z, a0: T_W_X, a1: CHASM.0 + 0.5, hw: T_HW, wall: T_WALL, crown: T_CROWN, floor: f },
        Space::Tunnel { along_x: true, c: T_S_Z, a0: T_W_X, a1: CHASM.0 + 0.5, hw: T_HW, wall: T_WALL, crown: T_CROWN, floor: f },
        Space::Tunnel { along_x: false, c: T_W_X, a0: T_S_Z, a1: T_N_Z, hw: T_HW, wall: T_WALL, crown: T_CROWN, floor: f },
        // Spokes: north (hub → T_N), west (the green tunnel, hub → T_W), south (hall → T_S).
        Space::Tunnel { along_x: false, c: 0.0, a0: HUB[3] - 0.5, a1: T_N_Z - 2.0, hw: 3.0, wall: 4.0, crown: 6.5, floor: f },
        Space::Tunnel { along_x: true, c: 5.0, a0: T_W_X + 2.0, a1: HUB[0] + 0.5, hw: 3.0, wall: 3.5, crown: 6.0, floor: f },
        Space::Tunnel { along_x: false, c: SPOKE_S_X, a0: T_S_Z + 2.0, a1: HALL[1] + 0.5, hw: 3.0, wall: 4.0, crown: 6.5, floor: f },
        // East strip: station platform + stair well (open to the sky).
        Space::Cut { min: Vec3::new(PLATFORM[0], f, PLATFORM[1]), max: Vec3::new(PLATFORM[2], PLATFORM_CEIL, PLATFORM[3]) },
        Space::Cut { min: Vec3::new(STAIRWELL[0], f, STAIRWELL[1]), max: Vec3::new(STAIRWELL[2], 70.0, STAIRWELL[3]) },
        // Storm-drain chute + its open culvert at the top.
        Space::Culvert { x0: -57.0, x1: HALL[0] + 0.4, cz: CHUTE_Z, hw: CHUTE_R, below: CHUTE_BELOW, above: 3.5 },
        Space::Trench { x0: -57.0, x1: -45.5, cz: CHUTE_Z, hw: CHUTE_R },
        // The subway chasm, sky to void.
        Space::Cut { min: Vec3::new(CHASM.0, -8.0, -70.0), max: Vec3::new(CHASM.1, 80.0, 70.0) },
        // Central manhole (light shaft) up to the avenue.
        Space::Tube { a: Vec3::new(MANHOLE.x, HUB_CROWN - 2.0, MANHOLE.y), b: Vec3::new(MANHOLE.x, STREET + 6.0, MANHOLE.y), r: MANHOLE_R },
    ];
    for j in JUNCTIONS {
        s.push(Space::Round { c: j, r: 6.0, floor: f, wall: 17.0, crown: 21.0 });
        s.push(Space::Cut {
            min: Vec3::new(j.x - SHAFT_HALF, 17.0, j.y - SHAFT_HALF),
            max: Vec3::new(j.x + SHAFT_HALF, 70.0, j.y + SHAFT_HALF),
        });
    }
    s
}

// ------------------------------------------------------------------------------------------------
// Materials

fn brick(v: IVec3, seed: u32) -> Material {
    let along = (v.x + v.z + (v.y & 1)) >> 1;
    if hash3(along, v.y, 7, seed) % 100 < 28 { mat::SEWER_BRICK_DARK } else { mat::SEWER_BRICK }
}

fn mossy(v: IVec3, seed: u32) -> Material {
    let h = hash3(v.x, v.y, v.z, seed ^ 0x6d05);
    if h % 100 < 45 { mat::MOSSY_BRICK } else { brick(v, seed) }
}

fn earth(v: IVec3, seed: u32) -> Material {
    let y = v.y as f32 * VS;
    if y > STREET - 5.0 {
        return mat::DIRT;
    }
    if value2(v.x as f32 * 0.07 + y * 0.3, v.z as f32 * 0.07, seed) > 0.62 { mat::STONE } else { mat::DEEP_STONE }
}

fn is_earth(m: Material) -> bool {
    matches!(m, mat::DEEP_STONE | mat::STONE | mat::DIRT)
}

// ------------------------------------------------------------------------------------------------
// Build passes

fn build_block(c: &mut City, seed: u32) {
    let (lo, hi) = (vx(-EDGE), vx(EDGE) - 1);
    let top = vx(STREET) - 1;
    for z in lo..=hi {
        for x in lo..=hi {
            let mx = (x as f32 + 0.5) * VS;
            if mx >= CHASM.0 && mx < CHASM.1 {
                continue;
            }
            c.w.set(IVec3::new(x, 0, z), mat::SEWER_ARCH);
            for y in 1..=top {
                let v = IVec3::new(x, y, z);
                c.w.set(v, earth(v, seed));
            }
        }
    }
    // Collapse-search hints along the bedrock.
    let mut z = lo;
    while z <= hi {
        let mut x = lo;
        while x <= hi {
            c.anchor(IVec3::new(x, 0, z));
            x += 24;
        }
        z += 24;
    }
}

fn carve_spaces(c: &mut City, spaces: &[Space], seed: u32) {
    // 1. Linings (only into untouched earth, so crossing spaces stay open).
    for s in spaces {
        let (lo, hi) = s.bounds();
        c.fill_with(lo, hi, |p, v, old| (is_earth(old) && s.inside(p, 1.0) && !s.inside(p, 0.0)).then(|| brick(v, seed)));
    }
    // 2. Air.
    for s in spaces {
        let (lo, hi) = s.bounds();
        c.fill_with(lo, hi, |p, _, old| (old != AIR && s.inside(p, 0.0)).then_some(AIR));
    }
}

/// Indestructible rib rings every 8 m along the tunnels and ribs + pilasters in the vaults.
fn build_frame(c: &mut City, spaces: &[Space]) {
    // Keys / ribs only where no other space overlaps (no ribs floating across junctions).
    let alone = |i: usize, p: Vec3| spaces.iter().enumerate().all(|(k, o)| k == i || !o.inside(p, 0.0));
    for (i, s) in spaces.iter().enumerate() {
        match *s {
            Space::Tunnel { along_x, a0, a1, .. } => {
                let mut a = (a0 / 8.0).ceil() * 8.0;
                while a <= a1 {
                    let (lo, hi) = s.bounds();
                    let (lo, hi) = if along_x {
                        (Vec3::new(a, lo.y, lo.z), Vec3::new(a + VS, hi.y, hi.z))
                    } else {
                        (Vec3::new(lo.x, lo.y, a), Vec3::new(hi.x, hi.y, a + VS))
                    };
                    c.fill_with(lo, hi, |p, _, old| {
                        let shell = old != AIR && s.inside(p, 1.0) && !s.inside(p, 0.0) && p.y > FLOOR;
                        // A 0.5 m key under the arch (above head height only: no snags).
                        let key = old == AIR && s.inside(p, 0.0) && !s.inside(p, -0.5) && p.y > FLOOR + T_WALL && alone(i, p);
                        (shell || key).then_some(mat::SEWER_ARCH)
                    });
                    a += 8.0;
                }
            }
            Space::Vault { x0, x1, z0, z1, floor, .. } => {
                let mut x = (x0 / 8.0).ceil() * 8.0 - 4.0;
                if x < x0 + 1.0 {
                    x += 8.0;
                }
                let ribs: Vec<f32> = if x0 == HUB[0] {
                    HUB_RIBS.to_vec()
                } else {
                    let mut v = Vec::new();
                    while x < x1 - 1.0 {
                        v.push(x);
                        x += 8.0;
                    }
                    v
                };
                for rx in ribs {
                    let (lo, hi) = s.bounds();
                    c.fill_with(Vec3::new(rx - 0.5, lo.y, lo.z), Vec3::new(rx + 0.5, hi.y, hi.z), |p, _, old| {
                        let shell = old != AIR && s.inside(p, 1.0) && !s.inside(p, 0.0) && p.y > floor;
                        let rib = old == AIR && s.inside(p, 0.0) && !s.inside(p, -0.75) && p.y > floor + 6.0 && alone(i, p);
                        // Pilasters: 0.5 m proud of the long walls, full height.
                        let pil = old == AIR && s.inside(p, 0.0) && ((p.z - z0) < 0.5 || (z1 - p.z) < 0.5) && alone(i, p);
                        (shell || rib || pil).then_some(mat::SEWER_ARCH)
                    });
                }
            }
            _ => {}
        }
    }
}

/// Paint the hub / hall / tunnel floors, then cut the hub's pit and channel.
fn build_floors(c: &mut City, seed: u32) {
    // Hub: worn concrete with brick borders; garage: concrete with oil stains.
    c.fill_with(Vec3::new(HUB[0], FLOOR - VS, HUB[1]), Vec3::new(CHASM.0, FLOOR, HUB[3]), |p, v, old| {
        if old == AIR || old == mat::SEWER_ARCH {
            return None;
        }
        let border = (p.x - HUB[0]) < 1.0 || (HUB[2] - p.x) < 1.0 || (p.z - HUB[1]) < 1.0 || (HUB[3] - p.z) < 1.0;
        Some(if border && p.x < HUB[2] {
            brick(v, seed)
        } else if value2(p.x * 0.35, p.z * 0.35, seed ^ 0x0f10) > 0.7 {
            mat::GARAGE_CONCRETE
        } else {
            mat::CONCRETE
        })
    });
    // Pit: three 0.5 m steps down to the floor, rug + moonlight pool under the manhole.
    let [px0, pz0, px1, pz1] = PIT;
    c.fill(Vec3::new(px0, PIT_FLOOR, pz0), Vec3::new(px1, FLOOR, pz1), AIR);
    // Step rings (0.5 m risers, 1 m treads) down to an 8 × 8 m floor.
    c.fill(Vec3::new(px0, FLOOR - 1.0, pz0), Vec3::new(px1, FLOOR - VS, pz1), mat::SEWER_BRICK_DARK);
    c.fill(Vec3::new(px0 + 1.0, FLOOR - 1.0, pz0 + 1.0), Vec3::new(px1 - 1.0, FLOOR - VS, pz1 - 1.0), AIR);
    c.fill(Vec3::new(px0, PIT_FLOOR, pz0), Vec3::new(px1, PIT_FLOOR + VS, pz1), mat::SEWER_BRICK);
    c.fill(Vec3::new(px0 + 2.0, PIT_FLOOR, pz0 + 2.0), Vec3::new(px1 - 2.0, PIT_FLOOR + VS, pz1 - 2.0), AIR);
    c.fill_with(Vec3::new(px0, PIT_FLOOR - VS, pz0), Vec3::new(px1, PIT_FLOOR, pz1), |p, v, _| {
        let d = Vec2::new(p.x - MANHOLE.x, p.z - MANHOLE.y).length();
        let ring = (2.6..3.0).contains(&d);
        let rug = (p.x - px0).min(px1 - p.x).min(p.z - pz0).min(pz1 - p.z) > 2.2;
        Some(if d < 1.6 || ring {
            mat::MOONLIGHT
        } else if rug {
            if (v.x + v.z).rem_euclid(6) == 0 { mat::HOTEL_CARPET_GOLD } else { mat::HOTEL_CARPET }
        } else {
            mat::GARAGE_CONCRETE
        })
    });
    // Channel across the hub (west wall → east wall), water 1.5 m down.
    let (cz0, cz1) = CHANNEL_Z;
    c.fill(Vec3::new(HUB[0], CHANNEL_WATER, cz0), Vec3::new(HUB[2], FLOOR, cz1), AIR);
    c.fill(Vec3::new(HUB[0], CHANNEL_WATER - VS, cz0), Vec3::new(HUB[2], CHANNEL_WATER, cz1), mat::SEWER_WATER);
    c.fill_with(Vec3::new(HUB[0], CHANNEL_WATER - 1.5, cz0 - 0.5), Vec3::new(HUB[2], CHANNEL_WATER - VS, cz1 + 0.5), |_, v, _| {
        Some(brick(v, seed))
    });
    // Coping stones along the channel edges.
    for z in [cz0 - 0.5, cz1] {
        c.fill(Vec3::new(HUB[0], FLOOR - VS, z), Vec3::new(HUB[2], FLOOR, z + 0.5), mat::STONE);
    }
    // Humped brick bridges with an arch over the water.
    for bx in CHANNEL_BRIDGES {
        c.fill_with(Vec3::new(bx - 1.5, CHANNEL_WATER, cz0 - 1.0), Vec3::new(bx + 1.5, FLOOR + VS, cz1 + 1.0), |p, v, _| {
            let t = ((p.z - (cz0 + cz1) * 0.5) / ((cz1 - cz0) * 0.5 + 1.0)).abs();
            let deck = if t < 0.55 { FLOOR + VS } else { FLOOR };
            let under = CHANNEL_WATER + 0.5 + 0.6 * (1.0 - t * t).max(0.0);
            let rail = (p.x - (bx - 1.5)) < 0.5 || (bx + 1.5 - p.x) < 0.5;
            if p.y >= deck {
                return None;
            }
            if p.y < under && t < 0.8 {
                return None;
            }
            Some(if rail { mat::SEWER_ARCH } else { brick(v, seed) })
        });
    }
    // Waterfall from a big pipe mouth in the west wall into the channel.
    let wz = (cz0 + cz1) * 0.5;
    c.fill(Vec3::new(HUB[0] - 0.5, 15.5, wz - 1.5), Vec3::new(HUB[0] + 0.5, 17.5, wz + 1.5), mat::TEAL_PIPE);
    c.fill(Vec3::new(HUB[0] - 0.5, 16.0, wz - 1.0), Vec3::new(HUB[0] + 0.5, 17.0, wz + 1.0), AIR);
    c.fill(Vec3::new(HUB[0], CHANNEL_WATER, wz - 1.0), Vec3::new(HUB[0] + 0.5, 16.0, wz + 1.0), mat::SEWER_WATER);
    // East end: a grate.
    c.fill_with(Vec3::new(HUB[2] - 0.5, CHANNEL_WATER, cz0), Vec3::new(HUB[2], FLOOR, cz1), |_, v, _| {
        Some(if v.z % 2 == 0 { mat::STEEL } else { mat::SEWER_ARCH })
    });
}

/// Mezzanine + stairs on the hub's south wall, the drop-in arch, the hall's decks and floors.
fn build_skate(c: &mut City, seed: u32) {
    let [mx0, mz0, mx1, mz1] = MEZZ;
    c.fill_with(Vec3::new(mx0, FLOOR, mz0), Vec3::new(mx1, DECK_Y, mz1), |_, v, _| Some(brick(v, seed)));
    // Stairs up to it from both sides (0.5 m risers every metre).
    for k in 0..9 {
        let top = FLOOR + VS * (k + 1) as f32;
        let (w0, e0) = (mx0 - 9.0 + k as f32, mx1 + 8.0 - k as f32);
        c.fill_with(Vec3::new(w0, FLOOR, mz0), Vec3::new(w0 + 1.0, top, mz1), |_, v, _| Some(brick(v, seed)));
        c.fill_with(Vec3::new(e0, FLOOR, mz0), Vec3::new(e0 + 1.0, top, mz1), |_, v, _| Some(brick(v, seed)));
    }
    // Steel nosing on the mezzanine edge.
    c.fill(Vec3::new(mx0, DECK_Y - VS, mz1 - VS), Vec3::new(mx1, DECK_Y, mz1), mat::STEEL);
    // Drop-in arch through the wall onto the north deck.
    c.fill_with(Vec3::new(-4.0, DECK_Y, HALL[3] - 0.6), Vec3::new(4.0, DECK_Y + 5.5, HUB[1] + 0.6), |p, _, _| {
        let k = p.x / 4.0;
        (p.y < DECK_Y + 4.0 + 1.5 * (1.0 - k * k).max(0.0).sqrt()).then_some(AIR)
    });
    // Floor-level arches into the hall's flat ends.
    for (a, b) in [(-21.0f32, -16.0f32), (16.0, 21.0)] {
        let cx = (a + b) * 0.5;
        c.fill_with(Vec3::new(a, FLOOR, HALL[3] - 0.6), Vec3::new(b, FLOOR + 7.0, HUB[1] + 0.6), |p, _, _| {
            let k = (p.x - cx) / ((b - a) * 0.5);
            (p.y < FLOOR + 5.0 + 2.0 * (1.0 - k * k).max(0.0).sqrt()).then_some(AIR)
        });
    }
    // Hall floor: dark concrete with paint lines; the trough gets a coping stripe.
    c.fill_with(Vec3::new(HALL[0], FLOOR - VS, HALL[1]), Vec3::new(HALL[2], FLOOR, HALL[3]), |p, _, old| {
        if old == AIR || old == mat::SEWER_ARCH {
            return None;
        }
        let stripe = (p.x.abs() - PIPE_X).abs() < 0.3 && p.z < RIDGE_N && p.z > RIDGE_S;
        Some(if stripe { mat::PAINT_YELLOW } else { mat::GARAGE_CONCRETE })
    });
    // Decks behind the hard-light walls (their outer faces sit inside these).
    for (z0, z1) in [(RIDGE_N, HALL[3] + 0.5), (HALL[1] - 0.5, RIDGE_S)] {
        c.fill_with(Vec3::new(-PIPE_X, FLOOR, z0), Vec3::new(PIPE_X, DECK_Y, z1), |p, v, _| {
            let lip = p.y > DECK_Y - VS && ((p.z - RIDGE_N).abs() < 0.5 || (p.z - RIDGE_S).abs() < 0.5);
            Some(if lip {
                mat::STEEL
            } else if p.y > DECK_Y - VS {
                mat::WOOD
            } else {
                brick(v, seed)
            })
        });
    }
    // Stairs up to the south deck from the west flat end (along the south wall).
    for k in 0..9 {
        let top = FLOOR + VS * (k + 1) as f32;
        let x0 = -PIPE_X - 9.0 + k as f32;
        c.fill_with(Vec3::new(x0, FLOOR, HALL[1] - 0.5), Vec3::new(x0 + 1.0, top, HALL[1] + 2.5), |_, v, _| Some(brick(v, seed)));
    }
    // Graffiti blotches on the hall walls (flush paint).
    let (lo, hi) = (Vec3::new(HALL[0] - 1.0, FLOOR + 1.0, HALL[1] - 1.0), Vec3::new(HALL[2] + 1.0, HALL_WALL, HALL[3] + 1.0));
    let paints = [mat::NEON_PINK, mat::PAINT_RED, mat::STUCCO_MINT, mat::PAINT_YELLOW, mat::STUCCO_LILAC, mat::STUCCO_SKY];
    c.fill_with(lo, hi, |p, v, old| {
        if !matches!(old, mat::SEWER_BRICK | mat::SEWER_BRICK_DARK) {
            return None;
        }
        let n = value2(p.x * 0.18 + p.z * 0.18, p.y * 0.3, seed ^ 0x6a2f);
        (n > 0.66).then(|| paints[(hash3(v.x >> 3, v.y >> 3, v.z >> 3, seed) % paints.len() as u32) as usize])
    });
    // Chute mouth: a steel lip ring on the hall wall.
    let r = chute_ridge(HALL[0]);
    c.fill_with(
        Vec3::new(HALL[0] - 1.0, r - CHUTE_BELOW - 0.8, CHUTE_Z - CHUTE_R - 0.8),
        Vec3::new(HALL[0], r + 3.5 + 0.8, CHUTE_Z + CHUTE_R + 0.8),
        |_, _, old| (old != AIR).then_some(mat::RUST),
    );
}

/// Tunnel floors, channels, bridges over the chasm, the station platform and the stair well.
fn build_sewers(c: &mut City, seed: u32) {
    let fb = FLOOR - VS;
    // Channels: T_N shallow (water 1 m down), T_S deep (1.5 m down, with step notches).
    for (cz, water) in [(T_N_Z, FLOOR - 1.0), (T_S_Z, FLOOR - 1.5)] {
        let (x0, x1) = (T_W_X + 7.0, CHASM.0 - 2.0);
        c.fill(Vec3::new(x0, water, cz - 1.5), Vec3::new(x1, FLOOR, cz + 1.5), AIR);
        c.fill(Vec3::new(x0, water - VS, cz - 1.5), Vec3::new(x1, water, cz + 1.5), mat::SEWER_WATER);
        c.fill_with(Vec3::new(x0 - 0.5, water - 1.5, cz - 2.0), Vec3::new(x1 + 0.5, water - VS, cz + 2.0), |_, v, _| Some(brick(v, seed)));
        for z in [cz - 2.0, cz + 1.5] {
            c.fill(Vec3::new(x0, fb, z), Vec3::new(x1, FLOOR, z + 0.5), mat::STONE);
        }
        // End walls of the channel.
        for x in [x0 - 0.5, x1] {
            c.fill_with(Vec3::new(x, water - VS, cz - 1.5), Vec3::new(x + 0.5, FLOOR, cz + 1.5), |_, v, _| Some(brick(v, seed)));
        }
        if water < FLOOR - 1.2 {
            // Step notches every 12 m on both sides (0.5 m blocks halfway up).
            let mut x = x0 + 6.0;
            while x < x1 - 2.0 {
                for (z0, z1) in [(cz - 1.5, cz - 0.5), (cz + 0.5, cz + 1.5)] {
                    c.fill(Vec3::new(x, water, z0), Vec3::new(x + 1.5, water + 1.0, z1), mat::STONE);
                }
                x += 12.0;
            }
        }
    }
    // Walkway paint: a worn stripe near each tunnel wall.
    // Bridges over the chasm: brick decks on indestructible edge ribs, no parapets.
    for (z0, z1) in [(T_N_Z - 2.5, T_N_Z + 2.5), (1.0, 6.0), (T_S_Z - 2.5, T_S_Z + 2.5)] {
        c.fill_with(Vec3::new(CHASM.0 - 0.5, FLOOR - 2.5, z0), Vec3::new(CHASM.1 + 0.5, FLOOR, z1), |p, v, _| {
            let t = ((p.x - (CHASM.0 + CHASM.1) * 0.5) / ((CHASM.1 - CHASM.0) * 0.5 + 0.5)).abs();
            let under = FLOOR - 2.5 + 1.5 * (1.0 - t * t).max(0.0).sqrt();
            if p.y < under {
                return None;
            }
            let edge = (p.z - z0) < 0.5 || (z1 - p.z) < 0.5;
            Some(if edge && p.y < FLOOR - VS {
                mat::SEWER_ARCH
            } else if p.y >= FLOOR - VS {
                mat::GARAGE_CONCRETE
            } else {
                brick(v, seed)
            })
        });
    }
    // Station platform: tiled floor with a yellow edge, subway-tile walls, columns, lights.
    let [px0, pz0, px1, pz1] = PLATFORM;
    c.fill_with(Vec3::new(px0, fb, pz0), Vec3::new(px1, FLOOR, pz1), |p, _, _| {
        Some(if p.x < px0 + 0.5 {
            mat::PAINT_YELLOW
        } else if (vx(p.x) + vx(p.z)).rem_euclid(2) == 0 {
            mat::POOL_TILE
        } else {
            mat::POOL_GROUT
        })
    });
    c.fill_with(Vec3::new(px1, FLOOR, pz0 - 1.0), Vec3::new(px1 + 0.5, PLATFORM_CEIL, pz1 + 1.0), |p, _, _| {
        let band = (p.y - (FLOOR + 3.0)).abs() < 0.3;
        Some(if band { mat::HULL_NAVY } else { mat::POOL_TILE })
    });
    for (z0, z1) in [(pz0 - 0.5, pz0), (pz1, pz1 + 0.5)] {
        c.fill(Vec3::new(px0, FLOOR, z0), Vec3::new(px1, PLATFORM_CEIL, z1), mat::POOL_TILE);
    }
    let mut z = pz0 + 4.0;
    while z < pz1 - 2.0 {
        let in_well = z > STAIRWELL[1] - 1.0 && z < STAIRWELL[3] + 1.0;
        if !in_well {
            c.fill(Vec3::new(px0, fb, z - 0.5), Vec3::new(px0 + 1.0, PLATFORM_CEIL + 0.5, z + 0.5), mat::SEWER_ARCH);
        }
        // Fluorescent strips down the ceiling (one in six dead).
        let dead = unit(hash3(vx(z), 3, 5, seed)) < 0.17;
        if !in_well {
            c.fill(
                Vec3::new(px0 + 4.0, PLATFORM_CEIL - VS, z - 1.5),
                Vec3::new(px0 + 5.0, PLATFORM_CEIL, z + 1.5),
                if dead { mat::FLUORESCENT_DEAD } else { mat::FLUORESCENT },
            );
        }
        z += 8.0;
    }
    // The stair well: four switchback flights, 11 steps of 0.5 m each, landings at both ends.
    let [sx0, sz0, sx1, sz1] = STAIRWELL;
    let xm = (sx0 + sx1) * 0.5;
    let (lane_a, lane_b) = ((xm, sx1), (sx0, xm));
    let (run0, run1) = (sz0 + 2.5, sz1 - 2.5);
    let flights = [(lane_a, true, FLOOR), (lane_b, false, FLOOR + 5.5), (lane_a, true, FLOOR + 11.0), (lane_b, false, FLOOR + 16.5)];
    for (k, &((x0, x1), north, base)) in flights.iter().enumerate() {
        for s in 0..11 {
            let top = base + VS * (s + 1) as f32;
            let (z0, z1) = if north { (run0 + s as f32, run0 + s as f32 + 1.0) } else { (run1 - s as f32 - 1.0, run1 - s as f32) };
            // The ground flight is solid; the others are 1 m slabs.
            let bottom = if k == 0 { FLOOR } else { top - 1.0 };
            c.fill_with(Vec3::new(x0, bottom, z0), Vec3::new(x1, top, z1), |p, v, _| {
                Some(if p.y > top - VS && (s % 2 == 0) { mat::PAINT_YELLOW } else { brick(v, seed) })
            });
        }
        // Landing at the flight's top end (both lanes).
        let ly = base + 5.5;
        let (z0, z1) = if north { (run1, sz1) } else { (sz0, run0) };
        c.fill_with(Vec3::new(sx0, ly - 1.0, z0), Vec3::new(sx1, ly, z1), |_, v, _| Some(brick(v, seed)));
    }
    // Street-level railing around the well (gap to the south where the top landing exits).
    let top = STREET;
    for (a, b) in [
        (Vec3::new(sx0 - 0.5, top, sz0), Vec3::new(sx0, top + 1.0, sz1 + 0.5)),
        (Vec3::new(sx0 - 0.5, top, sz1), Vec3::new(sx1, top + 1.0, sz1 + 0.5)),
    ] {
        c.fill(a, b, mat::STEEL);
    }
    // Green subway globes on the railing ends.
    c.fill(Vec3::new(sx0 - 0.5, top + 1.0, sz1), Vec3::new(sx0, top + 1.5, sz1 + 0.5), mat::MUTAGEN);
}

/// The chasm: brick walls with indestructible pilasters, lamps, and two big pipes across (rails).
fn build_chasm(c: &mut City, seed: u32) {
    let (x0, x1) = CHASM;
    // Pilasters, 0.5 m proud of each wall, bedrock to street: in the chasm they fill the air, in
    // the wall they only replace solids (never an opening), and they stop short of the openings
    // (tunnel mouths, the garage, the station platform).
    let openings_w = [(T_N_Z - 4.0, T_N_Z + 4.0), (-4.5, 16.5), (T_S_Z - 4.0, T_S_Z + 4.0)];
    let mut z = -52.0;
    while z <= 52.0 {
        for west in [true, false] {
            let (a, b) = if west { (x0 - 0.5, x0 + 0.5) } else { (x1 - 0.5, x1 + 0.5) };
            let gap = if west {
                openings_w.iter().any(|&(o0, o1)| z + 0.5 > o0 && z - 0.5 < o1).then_some((FLOOR - 3.0, FLOOR + 8.5))
            } else {
                (z + 0.5 > PLATFORM[1] && z - 0.5 < PLATFORM[3]).then_some((FLOOR - 3.0, PLATFORM_CEIL + 0.5))
            };
            c.fill_with(Vec3::new(a, VS, z - 0.5), Vec3::new(b, STREET, z + 0.5), |p, _, old| {
                if gap.is_some_and(|(g0, g1)| p.y >= g0 && p.y < g1) {
                    return None;
                }
                let in_chasm = p.x >= x0 && p.x < x1;
                (in_chasm || old != AIR).then_some(mat::SEWER_ARCH)
            });
        }
        z += 12.0;
    }
    // Lamps down the walls (warm), every 12 m, staggered.
    let mut z = -46.0;
    let mut k = 0;
    while z <= 46.0 {
        let (x, y) = if k % 2 == 0 { (x0, 24.0) } else { (x1 - 0.5, 20.0) };
        c.fill_with(Vec3::new(x, y, z - 0.5), Vec3::new(x + 0.5, y + 1.0, z + 0.5), |_, _, _| Some(mat::NEON_AMBER));
        z += 12.0;
        k += 1;
    }
    // Two big pipes across the chasm (tether anchors, and rails for the brave).
    for (pz, m) in [(22.0, mat::TEAL_PIPE), (-22.0, mat::RUST)] {
        c.tube(Vec3::new(x0 - 1.5, 24.0, pz), Vec3::new(x1 + 1.5, 24.0, pz), 0.75, m, false);
        for x in [x0 - 1.0, x1 + 0.5] {
            c.fill(Vec3::new(x, 22.8, pz - 1.0), Vec3::new(x + 0.5, 25.2, pz + 1.0), mat::SEWER_ARCH);
        }
    }
    let _ = seed;
}

/// Junction chambers: ledge spirals up the shafts; the green west tunnel; tunnel pipes and lamps.
fn build_tunnel_dressing(c: &mut City, seed: u32) {
    for j in JUNCTIONS {
        // A square spiral stair (1.5 m wide, 0.5 m risers every metre of path) climbing the shaft
        // walls from the chamber floor to the street, around a 2 m open well you can drop down or
        // reel up through. Below the shaft it stands free in the chamber (every step overlaps the
        // next, so it's one piece down to the floor).
        let inner = SHAFT_HALF - 1.5;
        let path = SHAFT_HALF - 0.75;
        let per_rev = 8.0 * path;
        c.fill_with(
            Vec3::new(j.x - SHAFT_HALF, FLOOR, j.y - SHAFT_HALF),
            Vec3::new(j.x + SHAFT_HALF, STREET, j.y + SHAFT_HALF),
            |p, v, old| {
                let (dx, dz) = (p.x - j.x, p.z - j.y);
                if dx.abs().max(dz.abs()) < inner {
                    return None;
                }
                // Perimeter coordinate along the stair's centre path (west → north → east → south).
                let s = if dx.abs() >= dz.abs() {
                    if dx < 0.0 { dz.clamp(-path, path) + path } else { 4.0 * path + (path - dz.clamp(-path, path)) }
                } else if dz > 0.0 {
                    2.0 * path + (dx.clamp(-path, path) + path)
                } else {
                    6.0 * path + (path - dx.clamp(-path, path))
                };
                for rev in 0..4 {
                    let top = FLOOR + VS * ((s + per_rev * rev as f32).floor() + 1.0);
                    if top > STREET {
                        break;
                    }
                    if p.y < top && p.y >= top - 1.0 {
                        return Some(if p.y >= top - VS && ((s + per_rev * rev as f32).floor() as i32) % 2 == 0 {
                            mat::STEEL
                        } else {
                            brick(v, seed)
                        });
                    }
                }
                let _ = old;
                None
            },
        );
        c.fill(Vec3::new(j.x - 6.5, 15.0, j.y - 0.5), Vec3::new(j.x - 6.0, 16.0, j.y + 0.5), mat::NEON_AMBER);
        // Moonlight rim where the shaft meets the street.
        c.fill_with(
            Vec3::new(j.x - SHAFT_HALF - 0.5, STREET - VS, j.y - SHAFT_HALF - 0.5),
            Vec3::new(j.x + SHAFT_HALF + 0.5, STREET, j.y + SHAFT_HALF + 0.5),
            |_, _, old| (old != AIR).then_some(mat::HAZARD_ORANGE),
        );
    }
    // Wall lamps + a pipe run along one wall of each tunnel.
    let runs =
        [(true, T_N_Z, T_W_X + 7.0, CHASM.0 - 1.0), (true, T_S_Z, T_W_X + 7.0, CHASM.0 - 1.0), (false, T_W_X, T_S_Z + 7.0, T_N_Z - 7.0)];
    for (i, &(along_x, cc, a0, a1)) in runs.iter().enumerate() {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let off = cc + side * (T_HW - 0.35);
        let (pa, pb) = if along_x {
            (Vec3::new(a0, FLOOR + T_WALL + 0.6, off), Vec3::new(a1, FLOOR + T_WALL + 0.6, off))
        } else {
            (Vec3::new(off, FLOOR + T_WALL + 0.6, a0), Vec3::new(off, FLOOR + T_WALL + 0.6, a1))
        };
        c.tube(pa, pb, 0.45, if i == 1 { mat::RUST } else { mat::TEAL_PIPE }, true);
        let mut a = a0 + 3.0;
        let mut k = 0;
        while a < a1 {
            let s = if k % 2 == 0 { side } else { -side };
            let w = cc + s * (T_HW + 0.25);
            let (lo, hi) = if along_x {
                (Vec3::new(a - 0.5, FLOOR + 3.0, w - 0.25), Vec3::new(a + 0.5, FLOOR + 3.5, w + 0.25))
            } else {
                (Vec3::new(w - 0.25, FLOOR + 3.0, a - 0.5), Vec3::new(w + 0.25, FLOOR + 3.5, a + 0.5))
            };
            let behind = if along_x { Vec3::new(a, FLOOR + 3.25, w + s * 0.5) } else { Vec3::new(w + s * 0.5, FLOOR + 3.25, a) };
            if c.w.get_at(behind) != AIR {
                c.fill(lo, hi, mat::NEON_AMBER);
            }
            a += 12.0;
            k += 1;
        }
    }
    // The green west tunnel: mossy walls, mutagen strips at the wall base, a root growth.
    let (x0, x1) = (T_W_X + 3.5, HUB[0]);
    c.fill_with(Vec3::new(x0, FLOOR - VS, 5.0 - 4.5), Vec3::new(x1, FLOOR + 7.5, 5.0 + 4.5), |p, v, old| {
        if !matches!(old, mat::SEWER_BRICK | mat::SEWER_BRICK_DARK) {
            return None;
        }
        let base = p.y < FLOOR + 1.0 && p.y > FLOOR && ((p.z - 5.0).abs() > 2.9);
        if base && vx(p.x).rem_euclid(6) < 4 {
            return Some(mat::MUTAGEN);
        }
        Some(mossy(v, seed))
    });
    // Mutagen puddles on the floor.
    for (px, pz, r) in [(-30.0, 4.0, 1.2), (-35.5, 6.5, 0.9), (-26.0, 6.0, 0.7)] {
        c.fill_with(Vec3::new(px - r, FLOOR - VS, pz - r), Vec3::new(px + r, FLOOR, pz + r), |p, _, old| {
            (old != AIR && Vec2::new(p.x - px, p.z - pz).length() < r).then_some(mat::MUTAGEN)
        });
    }
    // Roots: a trunk out of the crown splitting down the north wall.
    let top = Vec3::new(-31.0, FLOOR + 6.2, 5.8);
    c.curve(top, Vec3::new(-31.5, FLOOR + 4.5, 7.2), Vec3::new(-33.5, FLOOR + 0.2, 7.9), 0.7, 0.35, mat::DARK_WOOD, true);
    c.curve(top, Vec3::new(-29.0, FLOOR + 5.0, 7.4), Vec3::new(-27.0, FLOOR + 1.5, 7.9), 0.55, 0.3, mat::DARK_WOOD, true);
    c.curve(top, Vec3::new(-32.0, FLOOR + 6.0, 6.8), Vec3::new(-36.0, FLOOR + 3.0, 7.9), 0.45, 0.25, mat::DARK_WOOD, true);
    c.curve(
        Vec3::new(-29.0, FLOOR + 5.0, 7.4),
        Vec3::new(-28.0, FLOOR + 3.0, 7.9),
        Vec3::new(-28.5, FLOOR + 0.2, 7.9),
        0.3,
        0.25,
        mat::WOOD,
        true,
    );
    for (p, r) in
        [(Vec3::new(-31.0, FLOOR + 5.8, 6.4), 1.0), (Vec3::new(-34.0, FLOOR + 3.5, 7.6), 0.7), (Vec3::new(-27.5, FLOOR + 2.8, 7.6), 0.6)]
    {
        c.w.fill_ellipsoid(p, Vec3::new(r * 1.3, r, r * 0.8), mat::LEAVES_GREEN, true);
    }
}

/// Hub dressing that is part of the architecture: vault lights, pipe runs, manhole rim, lamps.
fn build_hub_dressing(c: &mut City, seed: u32) {
    // Moonlight rim around the manhole on the vault underside + the steel rim at the street.
    let hc = hub_ceiling(MANHOLE.y);
    c.fill_with(
        Vec3::new(MANHOLE.x - 3.0, hc - 1.5, MANHOLE.y - 3.0),
        Vec3::new(MANHOLE.x + 3.0, hc + 2.0, MANHOLE.y + 3.0),
        |p, _, old| {
            let d = Vec2::new(p.x - MANHOLE.x, p.z - MANHOLE.y).length();
            (old != AIR && d < MANHOLE_R + 1.0).then_some(mat::MOONLIGHT)
        },
    );
    c.fill_with(
        Vec3::new(MANHOLE.x - 3.0, STREET - 1.0, MANHOLE.y - 3.0),
        Vec3::new(MANHOLE.x + 3.0, STREET, MANHOLE.y + 3.0),
        |p, _, old| {
            let d = Vec2::new(p.x - MANHOLE.x, p.z - MANHOLE.y).length();
            (old != AIR && d < MANHOLE_R + 0.6).then_some(mat::STEEL)
        },
    );
    // String lights on the vault: two lines of warm bulbs following the ceiling.
    for z in [-3.0f32, 17.0] {
        let y = hub_ceiling(z);
        let mut x = HUB[0] + 1.5;
        while x < HUB[2] - 1.0 {
            let sag = 0.6 * (1.0 - ((x - HUB[0]).rem_euclid(8.0) - 4.0).abs() / 4.0);
            let v = (Vec3::new(x, y - 0.8 - sag, z) / VS).floor().as_ivec3();
            if c.w.get(v) == AIR {
                // Hung from the vault by a 1-voxel wire so it stays attached.
                let mut u = v;
                c.w.set(u, mat::NEON_AMBER);
                u.y += 1;
                while c.w.get(u) == AIR && u.y < vx(HUB_CROWN + 1.0) {
                    c.w.set(u, mat::PLASTIC_BLACK);
                    u.y += 1;
                }
            }
            x += 1.5;
        }
    }
    // Pipe runs along the vault (tether anchors / rails), on brackets at the ribs.
    for (pz, m, r) in [(-4.5f32, mat::TEAL_PIPE, 0.6f32), (18.5, mat::RUST, 0.6)] {
        let y = hub_ceiling(pz) - 1.6;
        c.tube(Vec3::new(HUB[0] - 0.5, y, pz), Vec3::new(HUB[2] + 0.5, y, pz), r, m, true);
        for rx in HUB_RIBS {
            c.fill(Vec3::new(rx - 0.5, y, pz - 0.5), Vec3::new(rx + 0.5, hub_ceiling(pz) + 0.5, pz + 0.5), mat::SEWER_ARCH);
        }
    }
    // A thick pipe along the north and south walls at 19 m.
    for pz in [HUB[1] + 0.7, HUB[3] - 0.7] {
        c.tube(Vec3::new(HUB[0], 19.0, pz), Vec3::new(HUB[2], 19.0, pz), 0.5, mat::TEAL_PIPE, true);
    }
    // ... broken where it would cross the drop-in arch (ends capped).
    c.fill_with(Vec3::new(-5.0, 18.0, HUB[1]), Vec3::new(5.0, 20.0, HUB[1] + 1.5), |_, _, old| (old == mat::TEAL_PIPE).then_some(AIR));
    // Wall lamps between the pilasters (the south ones high, clear of the mezzanine stairs).
    for x in [-16.0f32, -8.0, 8.0, 16.0] {
        c.fill(Vec3::new(x - 0.5, 17.0, HUB[3] - 0.5), Vec3::new(x + 0.5, 17.5, HUB[3]), mat::NEON_AMBER);
    }
    for x in [-10.0f32, 10.0] {
        c.fill(Vec3::new(x - 0.5, 20.0, HUB[1]), Vec3::new(x + 0.5, 20.5, HUB[1] + 0.5), mat::NEON_AMBER);
    }
    // Tatami platform (the dojo) and lab floor.
    c.fill_with(Vec3::new(-21.0, FLOOR, 18.0), Vec3::new(-6.0, FLOOR + VS, HUB[3] - 0.5), |p, _, _| {
        let border = ((p.x + 21.0).rem_euclid(3.0) < 0.5) || ((p.z - 18.0).rem_euclid(3.0) < 0.5);
        Some(if border { mat::DARK_WOOD } else { mat::TATAMI })
    });
    c.fill_with(Vec3::new(6.0, FLOOR - VS, 17.5), Vec3::new(HUB[2] - 0.5, FLOOR, HUB[3] - 0.5), |p, _, _| {
        Some(if (vx(p.x) + vx(p.z)).rem_euclid(2) == 0 { mat::HULL_WHITE } else { mat::PLASTIC_BLACK })
    });
    // Garage: roll-up door frame onto the chasm with hazard stripes, oil-stained floor.
    for z in [-4.0f32, 15.5] {
        c.fill_with(Vec3::new(CHASM.0 - 1.0, FLOOR, z), Vec3::new(CHASM.0 - 0.5, 20.0, z + 0.5), |p, _, _| {
            Some(if vx(p.y).rem_euclid(2) == 0 { mat::HAZARD_ORANGE } else { mat::PLASTIC_BLACK })
        });
    }
    let _ = seed;
}

// ------------------------------------------------------------------------------------------------
// Surface

fn build_streets(c: &mut City, seed: u32) {
    let st = vx(STREET) - 1;
    let (lo, hi) = (vx(-EDGE), vx(EDGE) - 1);
    let in_bld = |x: f32, z: f32| BUILDINGS.iter().any(|b| x >= b.rect[0] && x < b.rect[2] && z >= b.rect[1] && z < b.rect[3]);
    for z in lo..=hi {
        for x in lo..=hi {
            let v = IVec3::new(x, st, z);
            let old = c.w.get(v);
            if old == AIR || !is_earth(old) {
                continue;
            }
            let p = vc(v);
            let (ax0, ax1) = AVENUE;
            let (wx0, wx1) = WEST_ST;
            let road_av = p.z >= ax0 + 3.0 && p.z < ax1 - 3.0;
            let road_w = p.x >= wx0 + 3.0 && p.x < wx1 - 3.0;
            let road = road_av || road_w || (p.x >= CHASM.1);
            let m = if road {
                let centre_av = road_av && !road_w && (p.z - (ax0 + ax1) * 0.5).abs() < 0.3 && vx(p.x).rem_euclid(6) < 3;
                let centre_w = road_w && !road_av && (p.x - (wx0 + wx1) * 0.5).abs() < 0.3 && vx(p.z).rem_euclid(6) < 3;
                if centre_av || centre_w { mat::PAINT_YELLOW } else { mat::ASPHALT }
            } else if in_bld(p.x, p.z) {
                mat::CONCRETE
            } else {
                // Sidewalk: raised 0.5 m, slab joints.
                let sv = v + IVec3::Y;
                c.w.set(sv, if (x.rem_euclid(4) == 0) || (z.rem_euclid(4) == 0) { mat::MULLION } else { mat::CONCRETE });
                mat::CONCRETE
            };
            c.w.set(v, m);
        }
    }
    // Crosswalk stripes where the avenue meets the west street.
    c.fill_with(Vec3::new(WEST_ST.0 + 3.0, STREET - VS, AVENUE.0 + 3.0), Vec3::new(WEST_ST.1 - 3.0, STREET, AVENUE.1 - 3.0), |_, v, _| {
        Some(if v.x.rem_euclid(3) == 0 { mat::PAINT_WHITE } else { mat::ASPHALT })
    });
    // Jersey barriers where streets meet the block edge (low: you can still be knocked over).
    for (a, b) in [
        (Vec3::new(-EDGE, STREET, AVENUE.0), Vec3::new(-EDGE + 0.5, STREET + 1.0, AVENUE.1)),
        (Vec3::new(WEST_ST.0, STREET, -EDGE), Vec3::new(WEST_ST.1, STREET + 1.0, -EDGE + 0.5)),
        (Vec3::new(WEST_ST.0, STREET, EDGE - 0.5), Vec3::new(WEST_ST.1, STREET + 1.0, EDGE)),
        (Vec3::new(-EDGE, STREET, -EDGE), Vec3::new(-EDGE + 0.5, STREET + 1.0, EDGE)),
    ] {
        c.fill_with(a, b, |_, v, old| (old == AIR).then(|| if (v.x + v.z).rem_euclid(4) < 2 { mat::CONCRETE } else { mat::PAINT_RED }));
    }
    // The avenue bridge over the chasm: asphalt deck on indestructible girders (railings).
    let (ax0, ax1) = AVENUE;
    c.fill_with(Vec3::new(CHASM.0 - 0.5, STREET - 2.0, ax0), Vec3::new(CHASM.1 + 0.5, STREET, ax1), |p, _, _| {
        let girder = (p.z - ax0) < 1.0 || (ax1 - p.z) < 1.0;
        Some(if girder {
            mat::TOWER_CORE
        } else if p.y >= STREET - VS {
            mat::ASPHALT
        } else {
            mat::STEEL
        })
    });
    for z in [ax0, ax1 - 1.0] {
        c.fill_with(Vec3::new(CHASM.0 - 0.5, STREET, z), Vec3::new(CHASM.1 + 0.5, STREET + 1.5, z + 1.0), |p, v, _| {
            let post = v.x.rem_euclid(4) == 0 || p.y > STREET + 1.0;
            post.then_some(mat::STEEL)
        });
    }
    // The chute culvert: brick end wall at the block edge and a railing around the opening.
    c.fill_with(
        Vec3::new(-EDGE, chute_ridge(-EDGE) - CHUTE_BELOW - 1.0, CHUTE_Z - CHUTE_R - 1.0),
        Vec3::new(-EDGE + 0.5, STREET + 1.0, CHUTE_Z + CHUTE_R + 1.0),
        |_, v, _| Some(brick(v, seed)),
    );
    for z in [CHUTE_Z - CHUTE_R - 0.5, CHUTE_Z + CHUTE_R] {
        c.fill_with(Vec3::new(-EDGE, STREET, z), Vec3::new(-44.5, STREET + 1.0, z + 0.5), |p, v, old| {
            (old == AIR && p.y < STREET + 1.0 && (v.x.rem_euclid(3) == 0 || p.y >= STREET + 0.5)).then_some(mat::STEEL)
        });
    }
    // Street lights along the avenue and the west street.
    let mut x = -30.0;
    while x < 36.0 {
        if !(x > -3.0 && x < 3.0) {
            c.street_light(Vec3::new(x, STREET + VS, AVENUE.0 + 1.0), Vec2::Y, 6.0);
            c.street_light(Vec3::new(x + 6.0, STREET + VS, AVENUE.1 - 1.0), Vec2::NEG_Y, 6.0);
        }
        x += 16.0;
    }
    let mut z = -50.0;
    while z < 52.0 {
        let clear = (z - T_S_Z).abs() > 5.0 && (z - T_N_Z).abs() > 5.0 && (z - CHUTE_Z).abs() > 5.0;
        if clear && !(z > AVENUE.0 - 1.0 && z < AVENUE.1 + 1.0) {
            c.street_light(Vec3::new(WEST_ST.0 + 1.0, STREET + VS, z), Vec2::X, 6.0);
        }
        z += 16.0;
    }
    // Taxis and a van on the avenue, clear of the manhole.
    for (x, z, along) in [(-20.0f32, 3.0f32, true), (18.0, 7.5, true), (-42.0, -14.0, false), (30.0, 3.0, true)] {
        c.car(&CarSpec { base: Vec3::new(x, STREET, z), along_x: along, body: mat::PAINT_YELLOW, convertible: false });
    }
    // Hydrants.
    for (x, z) in [(-14.0f32, AVENUE.0 + 1.5), (12.0, AVENUE.1 - 1.5), (WEST_ST.1 - 1.5, 24.0)] {
        c.fill(Vec3::new(x, STREET + VS, z), Vec3::new(x + 0.5, STREET + 1.5, z + 0.5), mat::PAINT_RED);
    }
}

/// Solid brick tenements: lit windows, shopfronts, cornice + parapet, roof props, fire escapes.
fn build_buildings(c: &mut City, seed: u32) {
    for (bi, b) in BUILDINGS.iter().enumerate() {
        let [x0, z0, x1, z1] = b.rect;
        let (base, roof) = (STREET, b.roof);
        let body = if bi % 3 == 1 { mat::TERRACOTTA } else { mat::CITY_BRICK };
        let s = seed ^ (bi as u32).wrapping_mul(0x9e37);
        c.fill_with(Vec3::new(x0, base, z0), Vec3::new(x1, roof, z1), |p, _, old| {
            if old != AIR && !is_earth(old) && old != mat::CONCRETE && old != mat::MULLION {
                return None;
            }
            let edge = (p.x - x0) < VS || (x1 - p.x) < VS || (p.z - z0) < VS || (z1 - p.z) < VS;
            if !edge {
                return Some(body);
            }
            let along = if (p.x - x0) < VS || (x1 - p.x) < VS { p.z } else { p.x };
            let up = p.y - base;
            if up < 4.0 {
                // Shopfront: dark glass with a lit sign band.
                // Signs: a lit panel over every other shopfront, not a full-length band.
                let sign = up > 3.0 && up < 3.6 && (along + bi as f32 * 3.0).rem_euclid(10.0) < 4.0;
                let glass = up > 0.5 && up < 2.5 && (along - 0.5).rem_euclid(5.0) > 2.0;
                return Some(if sign {
                    [mat::NEON_PINK, mat::NEON_CYAN, mat::NEON_AMBER, mat::MUTAGEN]
                        [((along / 10.0).floor() as i32 + bi as i32).rem_euclid(4) as usize]
                } else if glass {
                    mat::GLASS_TEAL
                } else {
                    body
                });
            }
            let floor_k = ((up - 4.0) / 3.5).floor() as i32;
            let fy = (up - 4.0).rem_euclid(3.5);
            let col = (along - 1.0).rem_euclid(3.0);
            let window = fy > 1.0 && fy < 3.0 && col < 1.5 && (roof - p.y) > 1.5;
            if window {
                let h = hash3(vx(along) >> 2, floor_k, bi as i32, s);
                return Some(match h % 100 {
                    0..=13 => mat::NEON_AMBER,
                    14..=19 => mat::GLASS_SUNSET,
                    _ => mat::GLASS_BLUE,
                });
            }
            // Stone band under each window row.
            if fy < 0.5 { Some(mat::STONE) } else { Some(body) }
        });
        // Slender indestructible core.
        let (cx, cz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
        c.fill(Vec3::new(cx - 0.5, base - 2.0, cz - 0.5), Vec3::new(cx + 0.5, roof - 1.0, cz + 0.5), mat::TOWER_CORE);
        // Roof deck, cornice and parapet.
        c.fill(Vec3::new(x0 + 0.5, roof - VS, z0 + 0.5), Vec3::new(x1 - 0.5, roof, z1 - 0.5), mat::ROOF_DARK);
        // Cornice band: flush stone (an overhang would be a hidden lip for wall climbs and swings).
        c.fill_with(Vec3::new(x0, roof - 1.0, z0), Vec3::new(x1, roof, z1), |p, _, _| {
            let edge = (p.x - x0) < VS || (x1 - p.x) < VS || (p.z - z0) < VS || (z1 - p.z) < VS;
            edge.then_some(mat::STONE)
        });
        c.fill_with(Vec3::new(x0, roof, z0), Vec3::new(x1, roof + 1.0, z1), |p, _, _| {
            let edge = (p.x - x0) < VS || (x1 - p.x) < VS || (p.z - z0) < VS || (z1 - p.z) < VS;
            edge.then_some(body)
        });
        let mut rng = Rng::new((seed as u64) ^ (0x5eed_0000 + bi as u64));
        // Bulkhead (stair house) with a door.
        let (bx, bz) = (x0 + 3.0 + rng.range(0.0, (x1 - x0 - 9.0).max(0.1)), z0 + 3.0 + rng.range(0.0, (z1 - z0 - 9.0).max(0.1)));
        c.fill(Vec3::new(bx, roof, bz), Vec3::new(bx + 3.0, roof + 3.0, bz + 3.0), body);
        c.fill(Vec3::new(bx, roof + 3.0, bz), Vec3::new(bx + 3.0, roof + 3.5, bz + 3.0), mat::ROOF_DARK);
        c.fill(Vec3::new(bx + 1.0, roof, bz - 0.5), Vec3::new(bx + 2.0, roof + 2.0, bz), mat::DARK_WOOD);
        // AC units.
        for _ in 0..2 {
            let (ax, az) = (rng.range(x0 + 2.0, x1 - 4.0), rng.range(z0 + 2.0, z1 - 4.0));
            if (ax - bx).abs() < 4.0 && (az - bz).abs() < 4.0 {
                continue;
            }
            c.fill(Vec3::new(ax, roof, az), Vec3::new(ax + 2.0, roof + 1.5, az + 1.5), mat::STEEL);
            c.fill(Vec3::new(ax + 0.5, roof + 1.5, az + 0.5), Vec3::new(ax + 1.5, roof + 2.0, az + 1.0), mat::MULLION);
        }
        // Water tower on the taller roofs.
        if roof >= 46.0 {
            let (tx, tz) = (x1 - 5.0, z1 - 6.0);
            let tz = tz.min(EDGE - 5.0);
            c.water_tower(Vec3::new(tx, roof, tz), 2.2, 4.0);
            // A steel deck under the tank ties it to the legs (the kit's tank sits just inside them).
            c.fill_with(Vec3::new(tx - 3.0, roof + 2.5, tz - 3.0), Vec3::new(tx + 3.0, roof + 3.0, tz + 3.0), |q, _, _| {
                (Vec2::new(q.x - tx, q.z - tz).length() < 2.9).then_some(mat::STEEL)
            });
        }
        // Chimney stacks.
        let (hx, hz) = (x0 + 1.5, z1 - 3.0);
        c.fill(Vec3::new(hx, roof, hz), Vec3::new(hx + 1.0, roof + 2.5, hz + 1.5), body);
    }
    // The pizza billboard on the south-middle roof, facing the avenue.
    let b = BUILDINGS[4];
    c.billboard(&crate::city::BillboardSpec {
        base: Vec3::new(-1.0, b.roof, b.rect[3] - 2.5),
        normal: Vec2::Y,
        width: 12.0,
        height: 5.0,
        leg_h: 3.0,
        bands: [mat::PAINT_RED, mat::PAINT_YELLOW, mat::PAINT_YELLOW, mat::PAINT_RED],
        neon: mat::NEON_PINK,
    });
    // A neon billboard on the north-west roof, facing the west street: the high swing anchor for
    // anyone coming up the junction shafts.
    c.billboard(&crate::city::BillboardSpec {
        base: Vec3::new(ROOF_SIGN.x, BUILDINGS[0].roof, ROOF_SIGN.z),
        normal: Vec2::NEG_X,
        width: 10.0,
        height: 4.0,
        leg_h: 3.0,
        bands: [mat::HULL_NAVY, mat::BALL_GREEN, mat::BALL_GREEN, mat::HULL_NAVY],
        neon: mat::NEON_CYAN,
    });
    // Fire escapes on the avenue-facing facades.
    fire_escape(c, BUILDINGS[0], true, -32.0, BUILDINGS[0].roof);
    fire_escape(c, BUILDINGS[2], true, 18.0, BUILDINGS[2].roof);
    fire_escape(c, BUILDINGS[3], false, -32.0, BUILDINGS[3].roof);
    fire_escape(c, BUILDINGS[5], false, 18.0, BUILDINGS[5].roof);
    fire_escape(c, BUILDINGS[1], true, -8.0, BUILDINGS[1].roof);
    fire_escape(c, BUILDINGS[4], false, -8.0, BUILDINGS[4].roof);
}

/// A switchback fire escape on the facade facing the avenue: flights of 0.5 m steps in two strips
/// (inner / outer) so there's always head room, landings spanning both, up to the roof parapet.
fn fire_escape(c: &mut City, b: Building, south_face: bool, x_start: f32, roof: f32) {
    // Facade plane and outward direction.
    let (fz, out) = if south_face { (b.rect[1], -1.0f32) } else { (b.rect[3], 1.0f32) };
    let strip = |k: i32| -> (f32, f32) {
        // Inner strip 0..1.5 m, outer 1.5..3 m from the facade.
        let (a, b2) = if k % 2 == 0 { (0.0, 1.5) } else { (1.5, 3.0) };
        let (za, zb) = (fz + out * a, fz + out * b2);
        (za.min(zb), za.max(zb))
    };
    let run = 6.0;
    let mut y = STREET + VS;
    let mut k = 0;
    while y < roof + 0.5 {
        let (z0, z1) = strip(k);
        let east = k % 2 == 0;
        let steps = ((roof + 1.0 - y) / VS).min(12.0) as i32;
        for s in 0..steps {
            let top = y + VS * (s + 1) as f32;
            let x = if east { x_start + 1.5 + s as f32 * 0.5 } else { x_start + 1.5 + run - (s + 1) as f32 * 0.5 };
            // Two voxels thick so consecutive steps share a face (one piece, anchored).
            c.fill(Vec3::new(x, top - 1.0, z0), Vec3::new(x + 0.5, top, z1), mat::STEEL);
        }
        let ny = y + VS * steps as f32;
        // Landing (both strips) right where the flight ends.
        let lx = if east { x_start + 1.5 + VS * steps as f32 } else { x_start + run - VS * steps as f32 };
        let za = strip(0).0.min(strip(1).0);
        let zb = strip(0).1.max(strip(1).1);
        c.fill(Vec3::new(lx, ny - VS, za), Vec3::new(lx + 1.5, ny, zb), mat::STEEL);
        // Railing on the outer edge of the landing.
        let rz = if out < 0.0 { za } else { zb - VS };
        for px in [lx, lx + 1.0] {
            c.fill(Vec3::new(px, ny, rz), Vec3::new(px + 0.5, ny + 1.0, rz + VS), mat::PLASTIC_BLACK);
        }
        c.fill(Vec3::new(lx, ny + 1.0, rz), Vec3::new(lx + 1.5, ny + 1.5, rz + VS), mat::PLASTIC_BLACK);
        y = ny;
        k += 1;
        if steps < 12 {
            break;
        }
    }
    // Gap in the parapet onto the roof (the building side of the facade only).
    let (h0, h1) = if south_face { (fz, fz + 1.0) } else { (fz - 1.0, fz) };
    c.fill(Vec3::new(x_start - 0.5, roof, h0), Vec3::new(x_start + 9.5, roof + 1.0, h1), AIR);
}

// ------------------------------------------------------------------------------------------------
// Spawns, surf lines, routes, info

/// Authored spawn spots (feet, approximately): four in the lair, two tunnels, two on the station
/// platform, two on the roofs.
const SPAWN_SPOTS: [Vec3; 10] = [
    Vec3::new(-12.5, FLOOR + VS, 19.0),
    Vec3::new(11.5, FLOOR, 19.0),
    Vec3::new(-17.0, FLOOR, -2.0),
    Vec3::new(-21.0, FLOOR, -27.0),
    Vec3::new(T_W_X, FLOOR, -4.0),
    Vec3::new(22.0, FLOOR, T_N_Z - 2.5),
    Vec3::new(50.5, FLOOR, 25.0),
    Vec3::new(50.5, FLOOR, -38.0),
    Vec3::new(-27.0, 46.0, 36.0),
    Vec3::new(27.0, 46.0, -32.0),
];

fn pick_spawns(w: &VoxelWorld) -> Vec<Vec3> {
    let mut out: Vec<Vec3> = Vec::new();
    for s in SPAWN_SPOTS {
        let mut found = None;
        'ring: for ring in 0..8 {
            let n = if ring == 0 { 1 } else { 8 };
            for k in 0..n {
                let a = crate::noise::TAU * k as f32 / n as f32;
                let (x, z) = (s.x + crate::noise::dcos(a) * ring as f32 * 0.75, s.z + crate::noise::dsin(a) * ring as f32 * 0.75);
                if let Some(p) = crate::arena::spawn_ok(w, x, z, s.y + 2.0)
                    && (p.y - s.y).abs() < 1.2
                {
                    found = Some(p);
                    break 'ring;
                }
            }
        }
        if let Some(p) = found {
            out.push(p);
        }
    }
    out
}

/// The map's surf lines: the two half-pipe walls and the storm-drain chute.
pub fn surf_lines() -> Vec<SurfLine> {
    vec![
        SurfLine {
            a: Vec3::new(-PIPE_X, DECK_Y, RIDGE_N),
            b: Vec3::new(PIPE_X, DECK_Y, RIDGE_N),
            face_deg: PIPE_FACE_DEG,
            width: PIPE_WIDTH,
        },
        SurfLine {
            a: Vec3::new(-PIPE_X, DECK_Y, RIDGE_S),
            b: Vec3::new(PIPE_X, DECK_Y, RIDGE_S),
            face_deg: PIPE_FACE_DEG,
            width: PIPE_WIDTH,
        },
        SurfLine { a: CHUTE_TOP, b: CHUTE_END, face_deg: CHUTE_FACE_DEG, width: CHUTE_WIDTH },
    ]
}

fn routes() -> Vec<Route> {
    let hub_anchor = Vec3::new(-4.0, hub_ceiling(5.0) - 0.9, 5.0);
    vec![
        Route {
            name: "west_tunnel_bhop",
            from: "sewers",
            to: "sewers",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(T_W_X, FLOOR, T_S_Z + 8.0), Vec3::new(T_W_X, FLOOR, T_N_Z - 8.0)],
            anchors: vec![],
        },
        Route {
            name: "platform_bhop",
            from: "platform",
            to: "platform",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(49.0, FLOOR, PLATFORM[1] + 4.0), Vec3::new(49.0, FLOOR, 34.0)],
            anchors: vec![],
        },
        Route {
            name: "lair_swing",
            from: "arcade",
            to: "workshop",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(-19.0, FLOOR, 5.0), Vec3::new(17.0, FLOOR, 5.0)],
            anchors: vec![hub_anchor],
        },
        Route {
            name: "drain_chute",
            from: "streets",
            to: "skate_hall",
            kind: RouteKind::Mixed,
            waypoints: vec![Vec3::new(CHUTE_TOP.x + 2.0, chute_ridge(CHUTE_TOP.x + 2.0), CHUTE_Z), Vec3::new(-16.0, FLOOR, CHUTE_Z)],
            anchors: vec![],
        },
        Route {
            name: "halfpipe",
            from: "skate_hall",
            to: "skate_hall",
            kind: RouteKind::Mixed,
            // Ridden along its length: in from the west flat end, carve a wall, out at the east end.
            waypoints: vec![Vec3::new(-HALL[2] + 4.0, FLOOR, RIDGE_N - 4.0), Vec3::new(PIPE_X + 4.0, FLOOR, RIDGE_N - 4.0)],
            anchors: vec![],
        },
        Route {
            name: "manhole_climb",
            from: "sewers",
            to: "rooftops",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(T_W_X, FLOOR, T_N_Z), Vec3::new(-31.0, BUILDINGS[0].roof, T_N_Z)],
            anchors: vec![
                Vec3::new(T_W_X + SHAFT_HALF + 0.25, STREET - 1.5, T_N_Z),
                Vec3::new(ROOF_SIGN.x - 0.6, BUILDINGS[0].roof + 3.0 + 3.2, ROOF_SIGN.z),
            ],
        },
    ]
}

/// Build the Turtle Lair. The layout is authored; `seed` only varies small details (lit windows,
/// brick shading, prop jitter). Deterministic for a given seed.
pub fn generate_lair(seed: u64) -> (VoxelWorld, MapInfo) {
    let mut rng = Rng::new(seed ^ 0x7475_7274_6c65);
    let mut c = City::new(VoxelWorld::centered(LAIR_SIZE_VOXELS), seed ^ 0x6c61_6972);
    let s32 = (seed as u32) ^ 0x0071_a1e0;
    let sp = spaces();
    build_block(&mut c, s32);
    carve_spaces(&mut c, &sp, s32);
    build_frame(&mut c, &sp);
    build_floors(&mut c, s32);
    build_skate(&mut c, s32);
    build_chasm(&mut c, s32);
    build_sewers(&mut c, s32);
    build_tunnel_dressing(&mut c, s32);
    build_hub_dressing(&mut c, s32);
    build_streets(&mut c, s32);
    build_buildings(&mut c, s32);
    let spawns = pick_spawns(&c.w);
    let routes = routes();
    let placed = props::place_props(&mut c, &spawns, &routes, &mut rng);
    let (world, pruned) = c.finish_counted();

    let wb = world.bounds();
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb { min: Vec3::new(-EDGE - 20.0, VOID_Y, -EDGE - 20.0), max: Vec3::new(EDGE + 20.0, wb.max.y + 36.0, EDGE + 20.0) },
        void_y: VOID_Y,
        center: Vec3::new(MANHOLE.x, FLOOR, MANHOLE.y),
        // A high moon: its light only reaches the streets and whatever gets blasted open.
        sun_dir: Vec3::new(-0.28, 0.9, 0.34).normalize(),
        sky_zenith: [0.02, 0.04, 0.10],
        sky_horizon: [0.10, 0.17, 0.27],
        fog_color: [0.05, 0.11, 0.14],
        ambient: 0.58,
        gravity: None,
        surf_lines: surf_lines(),
        water_sheets: Vec::new(),
        water_walk: false,
        interactables: Default::default(),
        slipstream: None,
        mechanics: Default::default(),
    };
    let d = |name, center: Vec3, area| District { name, center, area };
    let districts = vec![
        d("lair", Vec3::new(0.0, FLOOR, 7.0), [HUB[0], HUB[1], HUB[2], HUB[3]]),
        d("dojo", Vec3::new(-14.0, FLOOR, 21.0), [HUB[0], 17.0, -4.0, HUB[3]]),
        d("lab", Vec3::new(14.0, FLOOR, 21.0), [4.0, 17.0, HUB[2], HUB[3]]),
        d("arcade", Vec3::new(-17.0, FLOOR, 2.0), [HUB[0], -4.0, -8.0, 13.0]),
        d("workshop", Vec3::new(15.0, FLOOR, 4.0), [8.0, -4.0, HUB[2], 13.0]),
        d("garage", Vec3::new(30.0, FLOOR, 6.0), [HUB[2], -4.0, CHASM.0, 16.0]),
        d("skate_hall", Vec3::new(0.0, FLOOR, -21.0), [HALL[0], HALL[1], HALL[2], HALL[3]]),
        d("sewers", Vec3::new(T_W_X, FLOOR, 0.0), [-EDGE, -EDGE, CHASM.0, EDGE]),
        d("platform", Vec3::new(50.0, FLOOR, 0.0), [PLATFORM[0], PLATFORM[1], PLATFORM[2], PLATFORM[3]]),
        d("streets", Vec3::new(-20.0, STREET, 5.0), [-EDGE, -EDGE, EDGE, EDGE]),
        d("rooftops", Vec3::new(-26.0, 46.0, 36.0), [-EDGE, -EDGE, EDGE, EDGE]),
    ];
    let info = MapInfo {
        name: "Turtle Lair",
        arena,
        // No sea: the channels are voxels, the chasm is void.
        water_y: -1000.0,
        water_color: [0.16, 0.58, 0.54],
        water_deep: [0.04, 0.20, 0.22],
        sun_color: [0.62, 0.74, 1.0],
        sky_mid: [0.05, 0.09, 0.18],
        fog_start: 24.0,
        fog_end: 120.0,
        neon_boost: 1.0,
        districts,
        routes,
        pruned_voxels: pruned,
    };
    let _ = placed;
    (world, info)
}

/// Props placed for a seed (for tests / previews): name + metre AABB.
pub fn props_for(seed: u64) -> Vec<props::Placed> {
    let mut rng = Rng::new(seed ^ 0x7475_7274_6c65);
    let mut c = City::new(VoxelWorld::centered(LAIR_SIZE_VOXELS), seed ^ 0x6c61_6972);
    let s32 = (seed as u32) ^ 0x0071_a1e0;
    let sp = spaces();
    build_block(&mut c, s32);
    carve_spaces(&mut c, &sp, s32);
    build_frame(&mut c, &sp);
    build_floors(&mut c, s32);
    build_skate(&mut c, s32);
    build_chasm(&mut c, s32);
    build_sewers(&mut c, s32);
    build_tunnel_dressing(&mut c, s32);
    build_hub_dressing(&mut c, s32);
    build_streets(&mut c, s32);
    build_buildings(&mut c, s32);
    let spawns = pick_spawns(&c.w);
    props::place_props(&mut c, &spawns, &routes(), &mut rng)
}
