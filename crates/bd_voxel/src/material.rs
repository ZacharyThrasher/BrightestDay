//! Terrain materials: id → look, toughness and debris "feel".
//!
//! The id is the `u8` stored per voxel ([`bd_core::voxel::Material`]); 0 is always air. The table is
//! static and shared by every client, so ids are part of the network contract — only ever append.

use bd_core::voxel::Material;
use serde::{Deserialize, Serialize};

/// What kind of stuff a voxel is when it breaks. The client picks debris sounds/particles from this;
/// the sim may use it for small gameplay differences (e.g. leaves don't hurt when they fall on you).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DebrisClass {
    Stone,
    Dirt,
    Wood,
    Leaves,
    Crystal,
    Grass,
    Sand,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialDef {
    pub name: &'static str,
    /// Base colour, sRGB 0–255.
    pub color: [u8; 3],
    /// Per-voxel brightness jitter amplitude (0.1 = ±10 %). Gives the chunky Cube-World look.
    pub jitter: f32,
    /// Resistance to carving. A carve of `power` p removes this material fully when `p >= hardness`;
    /// weaker carves bite a smaller radius, and carves below ~15 % of the hardness do nothing.
    /// Reference points: leaves 0.25, dirt 0.5, stone 1.0, crystal 1.6.
    pub hardness: f32,
    /// Never removed by carving (island cores). Also the anchor for structural-collapse detection.
    pub indestructible: bool,
    pub debris: DebrisClass,
    /// 0 = normal lit surface, 1 = fully self-lit. The mesher bakes less AO into emissive voxels
    /// and writes this value to the per-vertex `ChunkMesh::emissive` channel; the client adds
    /// glow/bloom from it (neon ≥ 0.5 blooms by the map's `neon_boost`).
    pub emissive: f32,
}

/// Material ids. Append-only (network + save contract).
pub mod mat {
    use bd_core::voxel::Material;
    pub const AIR: Material = 0;
    pub const GRASS: Material = 1;
    pub const DIRT: Material = 2;
    pub const STONE: Material = 3;
    pub const DEEP_STONE: Material = 4;
    pub const SAND: Material = 5;
    pub const WOOD: Material = 6;
    pub const LEAVES_GREEN: Material = 7;
    pub const LEAVES_ORANGE: Material = 8;
    pub const LEAVES_RED: Material = 9;
    pub const LEAVES_PINK: Material = 10;
    pub const CRYSTAL_CYAN: Material = 11;
    pub const CRYSTAL_VIOLET: Material = 12;
    pub const RUIN_BRICK: Material = 13;
    pub const MOSSY_BRICK: Material = 14;
    pub const JADE: Material = 15;
    pub const CORE: Material = 16;
    pub const TERRACOTTA: Material = 17;
    pub const DARK_WOOD: Material = 18;
    pub const LEAVES_GOLD: Material = 19;
    pub const CRYSTAL_GOLD: Material = 20;
    // ---- city kit (Miami & future cities) ----
    pub const ASPHALT: Material = 21;
    pub const PAINT_WHITE: Material = 22;
    pub const PAINT_YELLOW: Material = 23;
    pub const CONCRETE: Material = 24;
    pub const GLASS_TEAL: Material = 25;
    pub const GLASS_BLUE: Material = 26;
    pub const GLASS_SUNSET: Material = 27;
    pub const MULLION: Material = 28;
    /// Indestructible structural core (tower cores, bridge piers, city bedrock).
    pub const TOWER_CORE: Material = 29;
    pub const STUCCO_WHITE: Material = 30;
    pub const STUCCO_PINK: Material = 31;
    pub const STUCCO_MINT: Material = 32;
    pub const STUCCO_LEMON: Material = 33;
    pub const STUCCO_LILAC: Material = 34;
    pub const STUCCO_PEACH: Material = 35;
    pub const STUCCO_SKY: Material = 36;
    pub const NEON_PINK: Material = 37;
    pub const NEON_CYAN: Material = 38;
    pub const NEON_AMBER: Material = 39;
    pub const PALM_TRUNK: Material = 40;
    pub const PALM_FROND: Material = 41;
    pub const SAND_WET: Material = 42;
    pub const POOL_WATER: Material = 43;
    pub const HULL_WHITE: Material = 44;
    pub const HULL_NAVY: Material = 45;
    pub const PAINT_RED: Material = 46;
    pub const STEEL: Material = 47;
    pub const ROOF_DARK: Material = 48;
    // ---- Orbit (asteroids + station wreck) ----
    /// Grey-tan asteroid dust (walkable tops).
    pub const REGOLITH: Material = 49;
    /// Dark asteroid rock.
    pub const BASALT: Material = 50;
    /// Pale blue water ice (slightly self-lit so it reads against space).
    pub const ICE: Material = 51;
    /// Station hull plating.
    pub const HULL_PANEL: Material = 52;
    /// Solar-array cells (deep blue, faint sheen).
    pub const SOLAR_CELL: Material = 53;
    /// Hazard / marking paint.
    pub const HAZARD_ORANGE: Material = 54;
    /// Station running lights (full glow).
    pub const STATION_LIGHT: Material = 55;
    /// Burnt, blackened hull (re-entry scorch, blast damage).
    pub const SCORCHED: Material = 56;
    // ---- Backrooms (liminal room kits) ----
    /// Mustard office wallpaper and its faint stripe.
    pub const WALLPAPER: Material = 57;
    pub const WALLPAPER_STRIPE: Material = 58;
    /// Moist office carpet and its damp patches.
    pub const CARPET: Material = 59;
    pub const CARPET_DAMP: Material = 60;
    /// Drop-ceiling tile (soft: punches through easily).
    pub const CEILING_TILE: Material = 61;
    /// Fluorescent ceiling panel (full glow) and a dead one.
    pub const FLUORESCENT: Material = 62;
    pub const FLUORESCENT_DEAD: Material = 63;
    /// Poolrooms: white tile and pale grout.
    pub const POOL_TILE: Material = 64;
    pub const POOL_GROUT: Material = 65;
    /// Parking-garage concrete (darker than the city's).
    pub const GARAGE_CONCRETE: Material = 66;
    /// Hotel corridor carpet (burgundy) and its gold pattern; cream hotel wall.
    pub const HOTEL_CARPET: Material = 67;
    pub const HOTEL_CARPET_GOLD: Material = 68;
    pub const HOTEL_WALL: Material = 69;
    /// Dark playground rubber matting.
    pub const PLAY_MAT: Material = 70;
    /// Ball-pit balls (very soft).
    pub const BALL_RED: Material = 71;
    pub const BALL_BLUE: Material = 72;
    pub const BALL_YELLOW: Material = 73;
    pub const BALL_GREEN: Material = 74;
    /// The impossible room: painted daylight sky and clouds (self-lit).
    pub const SKY_PAINT: Material = 75;
    pub const CLOUD: Material = 76;
    /// The void under the building (indestructible, near black).
    pub const VOID_BLACK: Material = 77;
    /// Indestructible building frame (outer posts, slab-edge ring, columns, parapet).
    pub const SHELL: Material = 78;
    /// Black plastic (office chairs, bins, cabinets).
    pub const PLASTIC_BLACK: Material = 79;
}

const fn def(name: &'static str, color: [u8; 3], jitter: f32, hardness: f32, debris: DebrisClass) -> MaterialDef {
    MaterialDef { name, color, jitter, hardness, indestructible: false, debris, emissive: 0.0 }
}

const fn glow(mut d: MaterialDef, emissive: f32) -> MaterialDef {
    d.emissive = emissive;
    d
}

use DebrisClass as D;

/// The material table, indexed by id.
pub static MATERIALS: [MaterialDef; 80] = [
    def("air", [0, 0, 0], 0.0, 0.0, D::Dirt),
    def("grass", [98, 196, 64], 0.10, 0.55, D::Grass),
    def("dirt", [168, 112, 62], 0.09, 0.5, D::Dirt),
    def("stone", [128, 142, 166], 0.07, 1.0, D::Stone),
    def("deep stone", [92, 100, 128], 0.07, 1.35, D::Stone),
    def("sand", [238, 220, 160], 0.06, 0.4, D::Sand),
    def("wood", [128, 86, 50], 0.08, 0.8, D::Wood),
    def("green leaves", [58, 176, 70], 0.16, 0.25, D::Leaves),
    def("orange leaves", [246, 140, 36], 0.14, 0.25, D::Leaves),
    def("red leaves", [222, 64, 58], 0.14, 0.25, D::Leaves),
    def("blossom", [246, 150, 200], 0.12, 0.25, D::Leaves),
    glow(def("cyan crystal", [84, 236, 255], 0.10, 1.6, D::Crystal), 0.8),
    glow(def("violet crystal", [180, 104, 255], 0.10, 1.6, D::Crystal), 0.8),
    def("ruin brick", [214, 202, 176], 0.08, 1.1, D::Stone),
    def("mossy brick", [124, 168, 96], 0.12, 1.0, D::Stone),
    glow(def("lantern jade", [60, 222, 140], 0.06, 2.2, D::Crystal), 0.35),
    MaterialDef {
        name: "island core",
        color: [52, 40, 76],
        jitter: 0.08,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    def("terracotta", [222, 116, 72], 0.08, 0.9, D::Stone),
    def("dark wood", [84, 58, 40], 0.08, 0.9, D::Wood),
    def("gold leaves", [250, 204, 52], 0.12, 0.25, D::Leaves),
    glow(def("gold crystal", [255, 214, 90], 0.08, 1.6, D::Crystal), 0.8),
    // ---- city kit (21..) ----
    def("asphalt", [58, 58, 70], 0.05, 1.0, D::Stone),
    def("white paint", [236, 234, 226], 0.03, 1.0, D::Stone),
    def("yellow paint", [246, 204, 64], 0.04, 1.0, D::Stone),
    def("concrete", [206, 198, 190], 0.05, 1.0, D::Stone),
    glow(def("teal glass", [72, 186, 200], 0.05, 0.5, D::Crystal), 0.12),
    glow(def("blue glass", [84, 132, 196], 0.05, 0.5, D::Crystal), 0.12),
    glow(def("sunset glass", [255, 184, 150], 0.05, 0.5, D::Crystal), 0.22),
    def("mullion", [168, 176, 190], 0.04, 1.2, D::Stone),
    MaterialDef {
        name: "structural core",
        color: [104, 100, 118],
        jitter: 0.05,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    def("white stucco", [246, 242, 232], 0.04, 0.8, D::Stone),
    def("pink stucco", [250, 170, 196], 0.05, 0.8, D::Stone),
    def("mint stucco", [150, 232, 202], 0.05, 0.8, D::Stone),
    def("lemon stucco", [252, 232, 140], 0.05, 0.8, D::Stone),
    def("lilac stucco", [204, 176, 242], 0.05, 0.8, D::Stone),
    def("peach stucco", [254, 192, 146], 0.05, 0.8, D::Stone),
    def("sky stucco", [140, 212, 246], 0.05, 0.8, D::Stone),
    glow(def("pink neon", [255, 70, 170], 0.04, 0.6, D::Crystal), 1.0),
    glow(def("cyan neon", [50, 240, 255], 0.04, 0.6, D::Crystal), 1.0),
    glow(def("amber neon", [255, 176, 60], 0.04, 0.6, D::Crystal), 1.0),
    def("palm trunk", [176, 138, 96], 0.10, 0.7, D::Wood),
    def("palm frond", [92, 180, 70], 0.16, 0.25, D::Leaves),
    def("wet sand", [206, 180, 132], 0.05, 0.4, D::Sand),
    glow(def("pool water", [70, 214, 240], 0.05, 0.4, D::Crystal), 0.25),
    def("hull white", [244, 244, 248], 0.03, 1.2, D::Stone),
    def("hull navy", [40, 56, 120], 0.04, 1.2, D::Stone),
    def("red paint", [232, 64, 70], 0.05, 1.0, D::Stone),
    def("steel", [128, 138, 156], 0.05, 1.6, D::Stone),
    def("dark roof", [96, 92, 110], 0.06, 1.0, D::Stone),
    // ---- Orbit (49..) ----
    def("regolith", [168, 160, 150], 0.09, 0.6, D::Dirt),
    def("basalt", [78, 74, 90], 0.08, 1.2, D::Stone),
    glow(def("ice", [176, 222, 246], 0.07, 0.7, D::Crystal), 0.12),
    def("hull panel", [216, 220, 230], 0.04, 1.1, D::Stone),
    glow(def("solar cell", [40, 70, 170], 0.06, 0.5, D::Crystal), 0.18),
    def("hazard paint", [255, 136, 40], 0.04, 1.0, D::Stone),
    glow(def("station light", [255, 238, 204], 0.03, 0.6, D::Crystal), 1.0),
    def("scorched hull", [62, 58, 64], 0.08, 1.0, D::Stone),
    // ---- Backrooms (57..) ----
    def("wallpaper", [204, 188, 106], 0.04, 0.6, D::Stone),
    def("wallpaper stripe", [186, 169, 92], 0.04, 0.6, D::Stone),
    def("carpet", [156, 140, 90], 0.07, 0.45, D::Dirt),
    def("damp carpet", [122, 110, 70], 0.07, 0.45, D::Dirt),
    def("ceiling tile", [228, 222, 204], 0.03, 0.3, D::Dirt),
    glow(def("fluorescent panel", [255, 250, 230], 0.02, 0.4, D::Crystal), 1.0),
    def("dead fluorescent", [158, 160, 154], 0.03, 0.4, D::Crystal),
    def("pool tile", [232, 240, 242], 0.02, 0.8, D::Stone),
    def("pool grout", [176, 204, 214], 0.03, 0.8, D::Stone),
    def("garage concrete", [150, 150, 146], 0.06, 1.0, D::Stone),
    def("hotel carpet", [124, 32, 44], 0.06, 0.45, D::Dirt),
    def("hotel carpet gold", [190, 142, 62], 0.05, 0.45, D::Dirt),
    def("hotel wall", [230, 214, 178], 0.04, 0.6, D::Stone),
    def("play mat", [38, 40, 68], 0.06, 0.5, D::Dirt),
    def("red ball", [236, 64, 64], 0.10, 0.2, D::Sand),
    def("blue ball", [64, 116, 240], 0.10, 0.2, D::Sand),
    def("yellow ball", [250, 212, 64], 0.10, 0.2, D::Sand),
    def("green ball", [72, 204, 96], 0.10, 0.2, D::Sand),
    glow(def("sky paint", [120, 184, 246], 0.03, 0.6, D::Stone), 0.62),
    glow(def("cloud", [244, 247, 255], 0.03, 0.6, D::Stone), 0.7),
    MaterialDef {
        name: "void",
        color: [8, 8, 12],
        jitter: 0.02,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    MaterialDef {
        name: "backrooms frame",
        color: [140, 134, 118],
        jitter: 0.04,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    def("black plastic", [44, 44, 50], 0.05, 0.5, D::Stone),
];

/// Fallback for ids past the end of the table (loud magenta so bugs are visible).
static UNKNOWN: MaterialDef = def("unknown", [255, 0, 255], 0.0, 1.0, D::Stone);

/// Definition for a material id (never panics).
#[inline]
pub fn material_def(m: Material) -> &'static MaterialDef {
    MATERIALS.get(m as usize).unwrap_or(&UNKNOWN)
}

#[inline]
pub fn is_indestructible(m: Material) -> bool {
    material_def(m).indestructible
}

/// sRGB (0–255) → linear (0–1).
pub fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Linear (0–1) → sRGB (0–255).
pub fn linear_to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// Base colour of a material in linear RGB.
pub fn linear_color(m: Material) -> [f32; 3] {
    let c = material_def(m).color;
    [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2])]
}
