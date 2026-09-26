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
    /// 0 = normal lit surface, 1 = fully self-lit. The mesher bakes less AO into emissive voxels;
    /// the client may add glow/bloom for these.
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
pub static MATERIALS: [MaterialDef; 21] = [
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
