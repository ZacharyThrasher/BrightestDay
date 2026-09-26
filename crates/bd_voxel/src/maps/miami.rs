//! **Miami** — a Vice-City-sunset city arena built from the [`crate::city`] kit.
//!
//! Layout (metres; +X east = the ocean, +Z north; everything stands on bedrock at y = 0, streets at
//! 5.0, sidewalks 5.5, sea level 3.0 — the whole city is ringed by water, and water is a KO):
//!
//! ```text
//!  x: -96 ········ -36 ─canal─ -28 ··quay·· -11 ~ship~ 5.5 ~~bay~~ 22 ··hotels·· 45 OceanDr 63 park 71 beach 92 ~sea
//!  z=+96 ┌ Brickell towers ┐        ┌ port ┐                    ┌ deco strip ─ Ocean Drive ─ palms ─ sand ┐
//!   +60  ├─ N causeway (canal bridge → quay → steel through-arch over the bay) ───────────────────────┤
//!   +20  ├─ port bridge ─┤   crane    │  cruise │                  │                                  │
//!     0  │ Brickell Ave  │ containers │  ship   │ (centre)         ├─ neon gateway alley ─────────────┤
//!   -8   │  (swing       │   crane    │         │                  │                                  │
//!   -61  ├─ S causeway (canal bridge → quay → cable-stayed span over the palm island) ───────────────┤
//!   -96  └ stadium bowl  ┘            └        ┘                  └                                  ┘
//! ```
//!
//! - **Brickell** (W): eight glass towers 40–84 m in two columns flanking Brickell Ave (a 16 m street
//!   with a palm median) and three cross streets — the swing canyon. Street gaps 16–22 m; ledges at
//!   varied heights (setback terraces, podium roofs, balconies, crowns); four glass skybridges cross
//!   the avenue at 21–34 m (overhead swing anchors every ~40 m); billboards and water towers on roofs.
//!   A stadium bowl closes the south end.
//! - **The port** (centre): a quay with container stacks (cover/parkour) and two gantry cranes whose
//!   booms reach over the docked **cruise ship** — the central contested landmark (decks at 12/16/20/24 m,
//!   funnel to ~35 m, gangways from the quay).
//! - **Ocean Drive** (E): a 190 m dead-straight boulevard — the bhop lane — between a strip of pastel
//!   art-deco hotels (porches to vault, awnings to slide under, neon fin signs to hook) and Lummus Park
//!   (two palm rows), then **the beach** (open sand, lifeguard towers) down to the sea.
//! - Crossings: the **north causeway** (steel through-arch with lit hangers + cross braces: a risky
//!   lane over water with things to swing on), the **south causeway** (cable-stayed from a pylon on a
//!   palm island), the short **port bridge**, plus swing routes (cranes, ship, skybridges). Every pair
//!   of districts is linked by at least two routes.

use bd_core::voxel::Aabb;
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::ArenaInfo;
use crate::city::*;
use crate::material::mat;
use crate::world::VoxelWorld;

/// World dimensions for Miami, in voxels (192 × 96 × 192 m).
pub const MIAMI_SIZE_VOXELS: IVec3 = IVec3::new(384, 192, 384);

pub const WATER_Y: f32 = 3.0;
pub const ROAD_Y: f32 = 5.0;
pub const SIDE_Y: f32 = 5.5;

// ---- district geometry (metres) -----------------------------------------------------------------
const BRICKELL: Rect = Rect::new(-96.0, -96.0, -36.0, 96.0);
const QUAY: Rect = Rect::new(-28.0, -76.0, -11.0, 76.0);
const ISLAND: Rect = Rect::new(-3.0, -73.0, 11.0, -51.0);
const BEACH_ISLE: Rect = Rect::new(22.0, -96.0, 96.0, 96.0);
/// Brickell Ave (N–S).
const AVE: Rect = Rect::new(-74.0, -67.0, -58.0, 96.0);
/// Cross streets (z ranges) in Brickell; the N/S ones continue as causeways, z = 20 as the port bridge.
const CROSS: [(f32, f32); 4] = [(-67.0, -55.0), (-24.0, -12.0), (15.0, 25.0), (54.0, 66.0)];
const N_CAUSEWAY_Z: f32 = 60.0;
const S_CAUSEWAY_Z: f32 = -61.0;
const PORT_BRIDGE_Z: f32 = 20.0;
/// Hotel lots along Ocean Drive: x range and front (east face).
const HOTEL_X: (f32, f32) = (26.0, 42.0);
const OCEAN_DRIVE: Rect = Rect::new(45.0, -96.0, 63.0, 96.0);
const PARK_X: (f32, f32) = (63.0, 71.0);
const SAND_X: (f32, f32) = (71.0, 88.0);
const SHIP_CENTER: Vec2 = Vec2::new(-2.5, 0.0);

/// Beach-island surface: bayside walk / hotel lots / sidewalks at 5.5, park grass, then sand sloping
/// to wet sand under the sea.
fn beach_isle_top(x: f32, _z: f32) -> (f32, bd_core::voxel::Material) {
    if x < PARK_X.0 {
        (SIDE_Y, mat::CONCRETE)
    } else if x < PARK_X.1 {
        (SIDE_Y, mat::GRASS)
    } else if x < SAND_X.1 {
        let t = (x - SAND_X.0) / (SAND_X.1 - SAND_X.0);
        (SIDE_Y - 1.5 * t, mat::SAND)
    } else {
        let t = (x - SAND_X.1) / 8.0;
        (4.0 - 2.0 * t, mat::SAND_WET)
    }
}

/// Build Miami. Deterministic for a given seed (the skeleton is fixed; details vary).
pub fn generate_miami(seed: u64) -> (VoxelWorld, MapInfo) {
    let mut c = City::new(VoxelWorld::centered(MIAMI_SIZE_VOXELS), seed ^ 0x006d_6961_6d69);

    // ---- land --------------------------------------------------------------------------------
    c.land(BRICKELL, mat::STONE, |x, _| if x > -42.0 { (SIDE_Y, mat::TERRACOTTA) } else { (SIDE_Y, mat::CONCRETE) });
    c.land(QUAY, mat::STONE, |x, _| if !(-27.5..-11.5).contains(&x) { (SIDE_Y, mat::PAINT_YELLOW) } else { (SIDE_Y, mat::CONCRETE) });
    c.land(ISLAND, mat::SAND, |x, z| {
        let (ex, ez) = ((x - 4.0) / 7.0, (z + 62.0) / 11.0);
        let e = ex * ex + ez * ez;
        if e > 0.75 { (4.5, mat::SAND) } else { (SIDE_Y, mat::GRASS) }
    });
    c.land(BEACH_ISLE, mat::STONE, beach_isle_top);
    // Bedrock under the ship.
    c.bedrock(Rect::new(-11.0, -46.0, 6.0, 50.0));

    // ---- streets -----------------------------------------------------------------------------
    let mut net = StreetNet::default();
    net.add(StreetSpec { median: 2.0, sidewalk: 3.0, lane_w: 4.0, ..StreetSpec::new(AVE, false, ROAD_Y) });
    for (z0, z1) in CROSS {
        let w = z1 - z0;
        net.add(StreetSpec {
            sidewalk: if w > 10.5 { 3.0 } else { 2.0 },
            ..StreetSpec::new(Rect::new(-96.0, z0, -36.0, z1), true, ROAD_Y)
        });
    }
    // Causeway roads across the quay.
    net.add(StreetSpec { sidewalk: 2.0, ..StreetSpec::new(Rect::new(-28.0, N_CAUSEWAY_Z - 6.0, -11.0, N_CAUSEWAY_Z + 6.0), true, ROAD_Y) });
    net.add(StreetSpec { sidewalk: 2.0, ..StreetSpec::new(Rect::new(-28.0, S_CAUSEWAY_Z - 5.0, -11.0, S_CAUSEWAY_Z + 5.0), true, ROAD_Y) });
    // Ocean Drive and the causeway landings on the beach island.
    net.add(StreetSpec { sidewalk: 3.0, parking: 2.5, lane_w: 3.5, ..StreetSpec::new(OCEAN_DRIVE, false, ROAD_Y) });
    net.add(StreetSpec { sidewalk: 2.0, ..StreetSpec::new(Rect::new(22.0, N_CAUSEWAY_Z - 6.0, 48.0, N_CAUSEWAY_Z + 6.0), true, ROAD_Y) });
    net.add(StreetSpec { sidewalk: 2.0, ..StreetSpec::new(Rect::new(22.0, S_CAUSEWAY_Z - 5.0, 48.0, S_CAUSEWAY_Z + 5.0), true, ROAD_Y) });
    c.streets(&net);

    // ---- bridges ----------------------------------------------------------------------------
    // Canal crossings (Brickell ↔ quay).
    for (z, w) in [(N_CAUSEWAY_Z, 12.0), (PORT_BRIDGE_Z, 10.0), (S_CAUSEWAY_Z, 10.0)] {
        c.bridge(&BridgeSpec { pier_every: 5.0, ..BridgeSpec::new(Vec2::new(-37.0, z), Vec2::new(-27.0, z), ROAD_Y, w) });
    }
    // North causeway: steel through-arch over the bay.
    c.bridge(&BridgeSpec {
        style: BridgeStyle::Arch { t0: 0.04, t1: 0.96, rise: 22.0, hanger: mat::NEON_CYAN, neon: mat::NEON_PINK },
        pier_every: 11.0,
        ..BridgeSpec::new(Vec2::new(-12.0, N_CAUSEWAY_Z), Vec2::new(23.0, N_CAUSEWAY_Z), ROAD_Y, 12.0)
    });
    // South causeway: cable-stayed from a pylon on the palm island.
    let s_len = 35.0;
    c.bridge(&BridgeSpec {
        style: BridgeStyle::CableStay { t: (4.0 + 12.0) / s_len, top_y: 44.0, cable: mat::NEON_PINK },
        pier_every: 9.0,
        ..BridgeSpec::new(Vec2::new(-12.0, S_CAUSEWAY_Z), Vec2::new(23.0, S_CAUSEWAY_Z), ROAD_Y, 10.0)
    });

    // ---- Brickell towers ------------------------------------------------------------------------
    let wx = (-92.0, -77.0);
    let ex = (-55.0, -44.0);
    let mk = |x: (f32, f32), z: (f32, f32), floors: i32| TowerSpec::new(Rect::new(x.0, z.0, x.1, z.1), SIDE_Y, floors);
    let towers = vec![
        // West column (south → north).
        TowerSpec {
            balconies: true,
            corner_r: 3.0,
            crown: Crown::Flat,
            glass_alt: Some((mat::GLASS_BLUE, 0.2)),
            ..mk(wx, (-53.0, -37.0), 14)
        },
        TowerSpec {
            setbacks: vec![(12, 1.5), (17, 3.0)],
            glass_alt: Some((mat::GLASS_BLUE, 0.25)),
            open_floors: vec![10],
            crown: Crown::NeonBand(mat::NEON_CYAN),
            ..mk(wx, (-9.0, 11.0), 21)
        },
        // Sunset-mirror towers: cool glass below, the upper floors reflecting the sunset (ragged gradient),
        // thin white floor lines and wide mullions so the warm glass reads as a mirror, not brick.
        TowerSpec {
            glass: mat::GLASS_TEAL,
            glass_top: Some((mat::GLASS_SUNSET, 0.4)),
            glass_alt: Some((mat::GLASS_BLUE, 0.15)),
            setbacks: vec![(13, 2.0)],
            open_floors: vec![8],
            crown: Crown::Helipad,
            spandrel: mat::STUCCO_WHITE,
            spandrel_h: 0.5,
            frame: mat::STUCCO_WHITE,
            mullion_every: 6,
            ..mk(wx, (28.0, 50.0), 17)
        },
        TowerSpec {
            glass: mat::GLASS_BLUE,
            glass_top: Some((mat::GLASS_SUNSET, 0.3)),
            crown: Crown::Flat,
            spandrel: mat::STUCCO_WHITE,
            spandrel_h: 0.5,
            frame: mat::STUCCO_WHITE,
            mullion_every: 6,
            ..mk(wx, (70.0, 88.0), 11)
        },
        // East column.
        TowerSpec { crown: Crown::Flat, glass_alt: Some((mat::GLASS_SUNSET, 0.12)), ..mk(ex, (-53.0, -37.0), 10) },
        TowerSpec {
            mullion_every: 3,
            crown: Crown::Stepped { neon: mat::NEON_PINK },
            spandrel: mat::STUCCO_WHITE,
            ..mk(ex, (-9.0, 11.0), 16)
        },
        TowerSpec {
            glass: mat::GLASS_BLUE,
            corner_r: 4.0,
            balconies: true,
            crown: Crown::Spire { height: 11.0, tip: mat::NEON_PINK },
            ..mk(ex, (28.0, 50.0), 19)
        },
        TowerSpec {
            glass: mat::GLASS_BLUE,
            glass_alt: Some((mat::GLASS_SUNSET, 0.2)),
            crown: Crown::NeonBand(mat::NEON_PINK),
            ..mk(ex, (70.0, 88.0), 13)
        },
    ];
    for t in &towers {
        c.tower(t);
    }
    // Skybridges across the avenue (row: z centre, floor index) — overhead swing anchors.
    let skybridges = [(-45.0, 5), (1.0, 7), (39.0, 6), (79.0, 4)];
    for &(z, k) in &skybridges {
        skybridge(&mut c, wx.1, ex.0, z, SIDE_Y + k as f32 * 4.0);
    }
    // Podium hotels (row B, both columns) — low roofs = ledges; water towers & billboards on top.
    let podw = HotelSpec {
        fin: Some((0.5, 4.0)),
        neon: mat::NEON_CYAN,
        awning: Some(mat::PAINT_RED),
        ..HotelSpec::new(Rect::new(-92.0, -35.0, -77.0, -26.0), Facing::PosX, SIDE_Y, 3, mat::STUCCO_PEACH)
    };
    c.hotel(&podw);
    let pode = HotelSpec {
        ziggurat: true,
        neon: mat::NEON_PINK,
        ..HotelSpec::new(Rect::new(-55.0, -35.0, -44.0, -26.0), Facing::NegX, SIDE_Y, 4, mat::STUCCO_MINT)
    };
    c.hotel(&pode);
    c.water_tower(Vec3::new(-88.0, podw.roof_y(), -30.5), 2.0, 3.5);
    c.water_tower(Vec3::new(-84.5, towers[3].roof_y(), 84.0), 2.2, 4.0);
    c.water_tower(Vec3::new(-47.5, towers[4].roof_y(), -49.0), 1.8, 3.0);
    let sunset_art = [mat::PAINT_YELLOW, mat::STUCCO_PEACH, mat::STUCCO_PINK, mat::STUCCO_LILAC];
    c.billboard(&BillboardSpec {
        base: Vec3::new(-49.5, towers[4].roof_y(), -39.5),
        normal: Vec2::new(0.0, 1.0),
        width: 10.0,
        height: 5.0,
        leg_h: 2.5,
        bands: sunset_art,
        neon: mat::NEON_PINK,
    });
    c.billboard(&BillboardSpec {
        base: Vec3::new(-84.5, towers[3].roof_y(), 76.0),
        normal: Vec2::new(1.0, 0.0),
        width: 12.0,
        height: 5.0,
        leg_h: 3.0,
        bands: [mat::STUCCO_SKY, mat::STUCCO_MINT, mat::STUCCO_WHITE, mat::STUCCO_PINK],
        neon: mat::NEON_CYAN,
    });

    // Stadium bowl at the south end of Brickell.
    c.stadium(&StadiumSpec {
        center: Vec2::new(-66.0, -82.0),
        pitch: Vec2::new(16.0, 5.0),
        rows: 8,
        base_y: SIDE_Y,
        seats: vec![mat::STUCCO_SKY, mat::PAINT_YELLOW, mat::STUCCO_PINK, mat::STUCCO_MINT],
        wall: mat::STUCCO_WHITE,
        open_ends: 4.0,
        light_h: 26.0,
    });

    // Avenue median: compact palms (crowns kept inside the median so the swing lanes over both
    // carriageways stay clear) + bayfront palms + street lights.
    let mut z = -48.0;
    while z < 94.0 {
        if !CROSS.iter().any(|&(z0, z1)| z > z0 - 2.5 && z < z1 + 2.5) {
            let h = c.rng.range(6.5, 7.5);
            c.palm(&PalmSpec { fronds: 7, frond_len: 2.2, trunk_r: 0.4, ..PalmSpec::new(Vec3::new(-66.0, SIDE_Y, z), h) });
        }
        z += 13.0;
    }
    let mut z = -90.0;
    while z < 94.0 {
        if !CROSS.iter().any(|&(z0, z1)| z > z0 - 2.0 && z < z1 + 2.0) {
            let h = c.rng.range(8.0, 11.0);
            let lean = Vec2::new(c.rng.range(0.8, 2.2), c.rng.range(-0.6, 0.6));
            c.palm(&PalmSpec { lean, ..PalmSpec::new(Vec3::new(-39.0, SIDE_Y, z), h) });
            c.street_light(Vec3::new(-41.5, SIDE_Y, z + 6.0), Vec2::X, 6.0);
        }
        z += 13.0;
    }
    for z in [-40.0f32, -2.0, 36.0, 76.0] {
        c.street_light(Vec3::new(-72.5, SIDE_Y, z), Vec2::X, 7.0);
        c.street_light(Vec3::new(-59.5, SIDE_Y, z + 10.0), Vec2::NEG_X, 7.0);
    }

    // ---- the port ----------------------------------------------------------------------------
    let mut ship = ShipSpec::new(SHIP_CENTER, 88.0, 16.0, 0.5, 12.0);
    ship.levels = 4;
    ship.gangways = vec![-16.5, 17.5];
    c.cruise_ship(&ship);
    // Gangways: stairs along the quay edge up to the deck rail gaps.
    for g in [-16.5f32, 17.5] {
        let start = Vec3::new(-13.0, SIDE_Y, g - 14.0);
        let top = c.stairs(start, Vec2::Y, ship.deck_y, 3.0, 1.0, mat::HULL_WHITE);
        c.fill(Vec3::new(-14.5, SIDE_Y, top.z), Vec3::new(-10.0, ship.deck_y, top.z + 3.0), mat::HULL_WHITE);
    }
    // Cranes: legs across the quay, booms reaching over the ship.
    c.gantry_crane(-27.0, -14.0, 36.0, SIDE_Y, 32.0, 4.0, mat::PAINT_RED);
    c.gantry_crane(-27.0, -14.0, -8.0, SIDE_Y, 32.0, 4.0, mat::STUCCO_SKY);
    // Container stacks (cover + parkour steps).
    let colors = [mat::PAINT_RED, mat::HULL_NAVY, mat::PAINT_YELLOW, mat::STUCCO_MINT, mat::STUCCO_SKY, mat::HULL_WHITE, mat::STUCCO_PINK];
    let stacks: [(f32, f32, i32); 12] = [
        (-26.5, -50.0, 3),
        (-26.5, -43.0, 2),
        (-23.5, -50.0, 1),
        (-26.5, -36.0, 1),
        (-20.0, -46.0, 2),
        (-26.5, 42.0, 2),
        (-26.5, 49.0, 3),
        (-23.5, 49.0, 1),
        (-20.0, 44.5, 1),
        (-26.5, 3.0, 2),
        (-23.5, 3.0, 1),
        (-26.5, 14.0, 1),
    ];
    for (i, &(x, z, n)) in stacks.iter().enumerate() {
        for k in 0..n {
            let col = colors[(i * 3 + k as usize * 2 + (seed as usize % 5)) % colors.len()];
            c.container(Vec3::new(x, SIDE_Y + k as f32 * 2.5, z), false, col);
        }
    }
    // Harbour billboard facing Brickell, on the quay.
    c.billboard(&BillboardSpec {
        base: Vec3::new(-24.0, SIDE_Y, -24.0),
        normal: Vec2::NEG_X,
        width: 12.0,
        height: 6.0,
        leg_h: 9.0,
        bands: sunset_art,
        neon: mat::NEON_CYAN,
    });

    // ---- the palm island ----------------------------------------------------------------------
    for (x, z, h) in [(-0.5, -55.0, 9.0), (9.0, -54.5, 10.5), (-0.5, -69.5, 8.5), (8.5, -70.0, 11.0)] {
        let lean = Vec2::new(c.rng.range(-1.5, 1.5), c.rng.range(-1.5, 1.5));
        if let Some(g) = c.ground(x, z, 12.0) {
            c.palm(&PalmSpec { lean, ..PalmSpec::new(Vec3::new(x, g, z), h) });
        }
    }

    // ---- Ocean Drive: deco hotels ----------------------------------------------------------------
    struct H {
        z: (f32, f32),
        floors: i32,
        body: bd_core::voxel::Material,
        trim: bd_core::voxel::Material,
        neon: bd_core::voxel::Material,
        awning: Option<bd_core::voxel::Material>,
        fin: Option<(f32, f32)>,
        zig: bool,
        balc: bool,
    }
    let hotels = [
        H {
            z: (-95.0, -82.0),
            floors: 3,
            body: mat::STUCCO_PEACH,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_CYAN,
            awning: Some(mat::PAINT_RED),
            fin: Some((0.5, 5.0)),
            zig: false,
            balc: false,
        },
        H {
            z: (-80.0, -68.0),
            floors: 2,
            body: mat::STUCCO_MINT,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_PINK,
            awning: Some(mat::STUCCO_PINK),
            fin: None,
            zig: true,
            balc: false,
        },
        H {
            z: (-54.0, -40.0),
            floors: 4,
            body: mat::STUCCO_LILAC,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_PINK,
            awning: None,
            fin: Some((0.25, 6.0)),
            zig: false,
            balc: true,
        },
        H {
            z: (-38.0, -23.0),
            floors: 3,
            body: mat::STUCCO_LEMON,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_CYAN,
            awning: Some(mat::STUCCO_SKY),
            fin: None,
            zig: true,
            balc: false,
        },
        H {
            z: (-21.0, -6.0),
            floors: 5,
            body: mat::STUCCO_PINK,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_AMBER,
            awning: Some(mat::HULL_NAVY),
            fin: Some((0.5, 6.0)),
            zig: false,
            balc: true,
        },
        H {
            z: (6.0, 19.0),
            floors: 3,
            body: mat::STUCCO_SKY,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_PINK,
            awning: Some(mat::PAINT_YELLOW),
            fin: None,
            zig: true,
            balc: false,
        },
        H {
            z: (21.0, 36.0),
            floors: 4,
            body: mat::STUCCO_WHITE,
            trim: mat::STUCCO_PINK,
            neon: mat::NEON_CYAN,
            awning: None,
            fin: Some((0.75, 5.0)),
            zig: false,
            balc: true,
        },
        H {
            z: (38.0, 52.0),
            floors: 2,
            body: mat::STUCCO_PINK,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_CYAN,
            awning: Some(mat::STUCCO_MINT),
            fin: None,
            zig: true,
            balc: false,
        },
        H {
            z: (68.0, 82.0),
            floors: 4,
            body: mat::STUCCO_MINT,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_PINK,
            awning: None,
            fin: Some((0.5, 6.0)),
            zig: false,
            balc: true,
        },
        H {
            z: (84.0, 95.0),
            floors: 3,
            body: mat::STUCCO_PEACH,
            trim: mat::STUCCO_WHITE,
            neon: mat::NEON_AMBER,
            awning: Some(mat::STUCCO_SKY),
            fin: None,
            zig: true,
            balc: false,
        },
    ];
    let mut hotel_specs = Vec::new();
    for h in &hotels {
        let s = HotelSpec {
            trim: h.trim,
            neon: h.neon,
            awning: h.awning,
            fin: h.fin,
            ziggurat: h.zig,
            balconies: h.balc,
            round_r: if h.floors >= 4 { 3.5 } else { 2.5 },
            ..HotelSpec::new(Rect::new(HOTEL_X.0, h.z.0, HOTEL_X.1, h.z.1), Facing::PosX, SIDE_Y, h.floors, h.body)
        };
        c.hotel(&s);
        hotel_specs.push(s);
    }
    // Neon gateway over the mid alley (the Ocean Drive end).
    neon_gateway(&mut c, 43.5, 0.0, 10.0);
    c.billboard(&BillboardSpec {
        base: Vec3::new(24.0, SIDE_Y, 0.0),
        normal: Vec2::NEG_X,
        width: 9.0,
        height: 5.0,
        leg_h: 8.0,
        bands: sunset_art,
        neon: mat::NEON_PINK,
    });

    // Parked cars on Ocean Drive (parking lanes only: the carriageway is the bhop lane).
    let car_cols =
        [mat::STUCCO_PINK, mat::STUCCO_MINT, mat::HULL_WHITE, mat::PAINT_RED, mat::STUCCO_SKY, mat::PAINT_YELLOW, mat::STUCCO_LILAC];
    for (lane_x, dz) in [(49.25f32, 0.0f32), (58.75, 5.0)] {
        let mut z = -90.0 + dz;
        while z < 90.0 {
            let near_cross = [N_CAUSEWAY_Z, S_CAUSEWAY_Z].iter().any(|&cz| (z - cz).abs() < 10.0);
            if !near_cross && c.rng.chance(0.55) {
                let col = *c.rng.pick(&car_cols);
                let conv = c.rng.chance(0.4);
                c.car(&CarSpec { base: Vec3::new(lane_x, ROAD_Y, z), along_x: false, body: col, convertible: conv });
            }
            z += 6.0 * c.rng.int(1, 3) as f32;
        }
    }
    // A few cars on the causeways and Brickell cross streets (hop-over cover).
    for (x, zz, ax) in [
        (-20.0f32, N_CAUSEWAY_Z - 2.5, true),
        (8.0, N_CAUSEWAY_Z + 2.5, true),
        (-3.0, S_CAUSEWAY_Z + 2.5, true),
        (-85.0, -18.0, true),
        (-50.0, 20.0, true),
        (-68.5, 32.0, false),
        (-69.0, -30.0, false),
    ] {
        let col = *c.rng.pick(&car_cols);
        c.car(&CarSpec { base: Vec3::new(x, ROAD_Y, zz), along_x: ax, body: col, convertible: false });
    }

    // Lummus Park: the low wall (0.5 m, gaps), two palm rows, street lights.
    let mut z: f32 = -95.0;
    while z < 95.0 {
        if z.rem_euclid(12.0) < 8.0 {
            c.fill(Vec3::new(62.5, SIDE_Y, z), Vec3::new(63.0, SIDE_Y + VS_M, z + 0.5), mat::STUCCO_WHITE);
        }
        z += 0.5;
    }
    let mut z = -92.0;
    let mut i = 0;
    while z < 94.0 {
        let near_cross = [N_CAUSEWAY_Z, S_CAUSEWAY_Z].iter().any(|&cz| (z - cz).abs() < 2.0);
        if !near_cross {
            let h = c.rng.range(10.0, 14.5);
            let lean = Vec2::new(c.rng.range(0.5, 2.8), c.rng.range(-1.2, 1.2));
            c.palm(&PalmSpec { lean, ..PalmSpec::new(Vec3::new(65.5, SIDE_Y, z), h) });
            if i % 2 == 1 {
                let h2 = c.rng.range(8.0, 12.0);
                let lean2 = Vec2::new(c.rng.range(1.0, 3.0), c.rng.range(-1.5, 1.5));
                c.palm(&PalmSpec { lean: lean2, ..PalmSpec::new(Vec3::new(69.0, SIDE_Y, z + 4.5), h2) });
            }
        }
        if i % 2 == 0 && (z + 2.0).abs() > 6.0 {
            c.street_light(Vec3::new(61.5, SIDE_Y, z + 2.0), Vec2::NEG_X, 6.5);
        }
        z += 9.0;
        i += 1;
    }
    // Bayside walk palms.
    let mut z = -88.0;
    while z < 92.0 {
        if [N_CAUSEWAY_Z, S_CAUSEWAY_Z, 0.0].iter().all(|&cz| (z - cz).abs() > 8.0) {
            let h = c.rng.range(8.0, 10.5);
            let lean = Vec2::new(c.rng.range(-2.0, -0.6), c.rng.range(-0.6, 0.6));
            c.palm(&PalmSpec { lean, ..PalmSpec::new(Vec3::new(24.0, SIDE_Y, z), h) });
        }
        z += 14.0;
    }

    // ---- the beach -----------------------------------------------------------------------------
    let guard_cols = [
        (mat::STUCCO_PINK, mat::STUCCO_SKY, mat::PAINT_YELLOW),
        (mat::PAINT_YELLOW, mat::PAINT_RED, mat::STUCCO_MINT),
        (mat::STUCCO_SKY, mat::STUCCO_WHITE, mat::STUCCO_PINK),
        (mat::STUCCO_LILAC, mat::PAINT_YELLOW, mat::STUCCO_SKY),
        (mat::STUCCO_MINT, mat::STUCCO_PINK, mat::PAINT_RED),
    ];
    for (k, z) in [-78.0f32, -42.0, -8.0, 28.0, 66.0].iter().enumerate() {
        let x = 80.0 + c.rng.range(-1.0, 1.0);
        let g = c.ground(x, *z, 10.0).unwrap_or(SIDE_Y);
        let (body, stripe, roof) = guard_cols[k];
        c.lifeguard(&LifeguardSpec { base: Vec3::new(x, g, *z), facing: Vec2::X, body, stripe, roof });
    }
    // Beach palms (clusters near the park edge).
    for k in 0..7 {
        let z = -85.0 + k as f32 * 27.0 + c.rng.range(-4.0, 4.0);
        let x = c.rng.range(72.5, 76.0);
        let g = c.ground(x, z, 10.0).unwrap_or(SIDE_Y);
        let h = c.rng.range(9.0, 13.0);
        let lean = Vec2::new(c.rng.range(1.5, 4.0), c.rng.range(-2.0, 2.0));
        c.palm(&PalmSpec { lean, ..PalmSpec::new(Vec3::new(x, g, z), h) });
    }

    let mut spawns_in: Vec<Vec3> = Vec::new();
    let world_anchor_count = c.anchors.len();
    let _ = world_anchor_count;

    // ---- finish ---------------------------------------------------------------------------------
    let spawn_targets: [(f32, f32); 12] = [
        (-66.0, 84.0),  // Brickell Ave north
        (-84.0, 20.0),  // Brickell west cross street
        (-47.0, -18.0), // Brickell east cross street
        (-66.0, -82.0), // stadium pitch
        (-19.0, -22.0), // quay south
        (-19.0, 27.0),  // quay north
        (4.0, -67.0),   // palm island
        (46.5, 78.0),   // Ocean Drive north sidewalk
        (46.5, -44.0),  // Ocean Drive south sidewalk
        (80.0, 46.0),   // beach north
        (80.0, -24.0),  // beach south
        (78.0, -92.0),  // beach far south
    ];
    for (x, z) in spawn_targets {
        if let Some(s) = c.spawn_near(x, z, SIDE_Y + 3.0, &spawns_in, 12.0) {
            spawns_in.push(s);
        }
    }
    let (world, pruned) = c.finish_counted();
    let wb = world.bounds();
    let void_y = WATER_Y - 1.0;
    let center = Vec3::new(SHIP_CENTER.x, ship.top_y(), SHIP_CENTER.y - 4.0);
    let arena = ArenaInfo {
        spawns: spawns_in,
        bounds: Aabb {
            min: Vec3::new(wb.min.x - 24.0, void_y, wb.min.z - 24.0),
            max: Vec3::new(wb.max.x + 24.0, wb.max.y + 40.0, wb.max.z + 24.0),
        },
        void_y,
        center,
        // Warm sun low over the ocean (east-south-east), long shadows across the strip.
        sun_dir: Vec3::new(0.82, 0.26, -0.46).normalize(),
        sky_zenith: [0.30, 0.16, 0.46],
        sky_horizon: [1.0, 0.62, 0.42],
        fog_color: [0.96, 0.58, 0.60],
        ambient: 0.46,
        gravity: None,
        // SURF: Causeway Chute — off the north Brickell tower's roof (jump south from its edge) down
        // the z = 60 cross street, over the avenue, onto the north causeway's canal bridge.
        surf_lines: vec![crate::arena::SurfLine {
            a: Vec3::new(-94.0, towers[3].roof_y() + 4.0, N_CAUSEWAY_Z),
            b: Vec3::new(-40.0, 18.0, N_CAUSEWAY_Z),
            face_deg: 60.0,
            width: 8.0,
        }],
        water_sheets: Vec::new(),
        water_walk: false,
        interactables: Default::default(),
        slipstream: None,
        mechanics: Default::default(),
    };
    let districts = vec![
        District { name: "brickell", center: Vec3::new(-66.0, ROAD_Y, 20.0), area: [-96.0, -67.0, -36.0, 96.0] },
        District { name: "stadium", center: Vec3::new(-66.0, SIDE_Y, -82.0), area: [-96.0, -96.0, -36.0, -67.0] },
        District { name: "port", center: Vec3::new(-19.0, SIDE_Y, 0.0), area: [-28.0, -76.0, -11.0, 76.0] },
        District { name: "cruise_ship", center, area: [-11.0, -46.0, 6.0, 50.0] },
        District {
            name: "palm_island",
            center: Vec3::new(4.0, SIDE_Y, -62.0),
            area: [ISLAND.min.x, ISLAND.min.y, ISLAND.max.x, ISLAND.max.y],
        },
        District { name: "ocean_drive", center: Vec3::new(54.0, ROAD_Y, 0.0), area: [22.0, -96.0, 63.0, 96.0] },
        District { name: "beach", center: Vec3::new(80.0, 4.5, 0.0), area: [63.0, -96.0, 92.0, 96.0] },
    ];
    let routes = routes(&ship);
    let info = MapInfo {
        name: "Miami",
        arena,
        water_y: WATER_Y,
        water_color: [0.10, 0.62, 0.66],
        water_deep: [0.06, 0.20, 0.36],
        sun_color: [1.0, 0.70, 0.42],
        sky_mid: [0.98, 0.40, 0.58],
        fog_start: 70.0,
        fog_end: 320.0,
        neon_boost: 1.4,
        districts,
        routes,
        pruned_voxels: pruned,
    };
    (world, info)
}

const VS_M: f32 = 0.5;

/// A glass skybridge across Brickell Ave between the two tower columns at floor height `y`.
fn skybridge(c: &mut City, x0: f32, x1: f32, z: f32, y: f32) {
    let (z0, z1) = (z - 2.5, z + 2.5);
    c.fill_with(Vec3::new(x0 - 0.5, y, z0), Vec3::new(x1 + 0.5, y + 4.0, z1), |p, v, _| {
        let rel = p.y - y;
        let edge_z = p.z < z0 + VS_M || p.z >= z1 - VS_M;
        if rel < VS_M {
            return Some(mat::CONCRETE);
        }
        if rel >= 4.0 - VS_M {
            return Some(mat::ROOF_DARK);
        }
        if edge_z {
            return Some(if v.x.rem_euclid(4) == 0 || rel < 1.0 { mat::MULLION } else { mat::GLASS_TEAL });
        }
        // Hollow: this also opens doorways through both towers' walls.
        Some(bd_core::voxel::AIR)
    });
    // Underside truss (the swing anchor line) with a neon strip.
    c.fill(Vec3::new(x0 + 0.5, y - 1.0, z - 0.75), Vec3::new(x1 - 0.5, y, z + 0.75), mat::MULLION);
    c.fill(Vec3::new(x0 + 0.5, y - 1.0, z - 0.25), Vec3::new(x1 - 0.5, y - 0.5, z + 0.25), mat::NEON_CYAN);
}

/// Neon gateway over an alley mouth: two pylons and a lit lintel across z = `zc` ± `half`.
fn neon_gateway(c: &mut City, x: f32, zc: f32, half: f32) {
    let top = SIDE_Y + 9.0;
    for sg in [-1.0f32, 1.0] {
        let z = zc + sg * half;
        c.fill(Vec3::new(x - 0.75, SIDE_Y - VS_M, z - 0.75), Vec3::new(x + 0.75, top, z + 0.75), mat::STUCCO_PINK);
        c.fill(Vec3::new(x - 0.75, top, z - 0.75), Vec3::new(x + 0.75, top + 2.0, z + 0.75), mat::NEON_CYAN);
    }
    c.fill(Vec3::new(x - 0.75, top - 2.5, zc - half), Vec3::new(x + 0.75, top, zc + half), mat::STUCCO_WHITE);
    c.fill(Vec3::new(x + 0.75, top - 2.0, zc - half + 1.0), Vec3::new(x + 1.25, top - 0.5, zc + half - 1.0), mat::NEON_PINK);
    c.fill(Vec3::new(x - 1.25, top - 2.0, zc - half + 1.0), Vec3::new(x - 0.75, top - 0.5, zc + half - 1.0), mat::NEON_PINK);
}

fn routes(ship: &ShipSpec) -> Vec<Route> {
    let deck = ship.deck_y;
    vec![
        Route {
            name: "ocean_drive_bhop",
            from: "ocean_drive",
            to: "ocean_drive",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(54.0, ROAD_Y, -92.0), Vec3::new(54.0, ROAD_Y, 92.0)],
            anchors: vec![],
        },
        Route {
            name: "brickell_canyon",
            from: "stadium",
            to: "brickell",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(-62.0, ROAD_Y, -62.0), Vec3::new(-62.0, 20.0, 0.0), Vec3::new(-62.0, ROAD_Y, 92.0)],
            anchors: vec![
                Vec3::new(-62.0, SIDE_Y + 19.0, -45.0),
                Vec3::new(-62.0, SIDE_Y + 27.0, 1.0),
                Vec3::new(-62.0, SIDE_Y + 23.0, 39.0),
                Vec3::new(-62.0, SIDE_Y + 15.0, 79.0),
            ],
        },
        Route {
            name: "north_causeway",
            from: "brickell",
            to: "ocean_drive",
            kind: RouteKind::Ground,
            waypoints: vec![
                Vec3::new(-48.0, ROAD_Y, N_CAUSEWAY_Z),
                Vec3::new(-20.0, ROAD_Y, N_CAUSEWAY_Z),
                Vec3::new(40.0, ROAD_Y, N_CAUSEWAY_Z),
            ],
            anchors: vec![Vec3::new(5.5, 26.0, N_CAUSEWAY_Z)],
        },
        Route {
            name: "south_causeway",
            from: "brickell",
            to: "ocean_drive",
            kind: RouteKind::Ground,
            waypoints: vec![
                Vec3::new(-48.0, ROAD_Y, S_CAUSEWAY_Z),
                Vec3::new(4.0, ROAD_Y, S_CAUSEWAY_Z),
                Vec3::new(40.0, ROAD_Y, S_CAUSEWAY_Z),
            ],
            anchors: vec![Vec3::new(4.0, 44.0, S_CAUSEWAY_Z)],
        },
        Route {
            name: "port_bridge",
            from: "brickell",
            to: "port",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(-48.0, ROAD_Y, PORT_BRIDGE_Z), Vec3::new(-20.0, SIDE_Y, PORT_BRIDGE_Z)],
            anchors: vec![],
        },
        Route {
            name: "gangway",
            from: "port",
            to: "cruise_ship",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(-13.0, SIDE_Y, 1.0), Vec3::new(-13.0, deck, 17.5), Vec3::new(-6.0, deck, 17.5)],
            anchors: vec![],
        },
        Route {
            name: "beach_to_ship",
            from: "beach",
            to: "cruise_ship",
            kind: RouteKind::Mixed,
            waypoints: vec![
                Vec3::new(80.0, 4.5, 0.0),
                Vec3::new(46.0, SIDE_Y, 0.0),
                Vec3::new(24.0, SIDE_Y, 0.0),
                Vec3::new(0.0, ship.top_y(), 0.0),
            ],
            anchors: vec![ship.world(ship.house_hw(), ship.top_y(), 0.0)],
        },
        Route {
            name: "crane_swing",
            from: "port",
            to: "cruise_ship",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(-24.0, SIDE_Y, -8.0), Vec3::new(-2.5, ship.top_y(), -8.0)],
            anchors: vec![Vec3::new(-8.0, 32.0, -8.0)],
        },
    ]
}
