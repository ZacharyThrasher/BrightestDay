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
    // ---- Turtle Lair (sewers + night city) ----
    /// Night-blue sewer brick and its darker courses (breakable).
    pub const SEWER_BRICK: Material = 80;
    pub const SEWER_BRICK_DARK: Material = 81;
    /// Indestructible load-bearing sewer arches, ribs and frame.
    pub const SEWER_ARCH: Material = 82;
    /// Murky teal sewer water (faintly self-lit, like the pools).
    pub const SEWER_WATER: Material = 83;
    /// Rusty iron pipe / scrap.
    pub const RUST: Material = 84;
    /// Straw tatami mat (dojo floor).
    pub const TATAMI: Material = 85;
    /// Mutagen-green glow (ooze, green-lit tunnel, lab screens).
    pub const MUTAGEN: Material = 86;
    /// Cool moonlight (manhole light shafts, the light pool under them).
    pub const MOONLIGHT: Material = 87;
    /// Red-brown city brick (tenements above).
    pub const CITY_BRICK: Material = 88;
    /// Teal-painted pipe.
    pub const TEAL_PIPE: Material = 89;
    // ---- Cybertron (metal planet: gunmetal plates, energon, rust) ----
    /// Dark gunmetal plating (streets, walls) and a lighter blue-steel plate (Iacon, statues).
    pub const CYBER_PLATE: Material = 90;
    pub const CYBER_PLATE_LIGHT: Material = 91;
    /// Near-black trim / panel seams (unlit).
    pub const CYBER_TRIM: Material = 92;
    /// Glowing energon: cyan-blue conduits and seams (full glow).
    pub const ENERGON: Material = 93;
    /// Amber-orange energy seams (full glow).
    pub const SEAM_ORANGE: Material = 94;
    /// Kaon's red-orange furnace glow (full glow).
    pub const KAON_RED: Material = 95;
    /// Energon-cube pink (glowing, soft).
    pub const ENERGON_PINK: Material = 96;
    /// Sea of Rust dust and its darker drifts.
    pub const RUST_DUST: Material = 97;
    pub const RUST_DUNE: Material = 98;
    /// Indestructible gunmetal frame (city cores, gate frames, pylons).
    pub const CYBER_FRAME: Material = 99;
    /// Indestructible glowing energon frame (space-bridge rings, Well lip).
    pub const ENERGON_FRAME: Material = 100;
    /// Indestructible glowing amber frame (Kaon gate, transforming-panel warning light).
    pub const EMBER_FRAME: Material = 101;
    /// Indestructible transforming-panel plate (moving terrain is armoured).
    pub const TRANSFORM_PLATE: Material = 102;
    /// Autobot-red and Decepticon-purple insignia inlays (faint glow).
    pub const INSIGNIA_RED: Material = 103;
    pub const INSIGNIA_PURPLE: Material = 104;
    /// Dark-bronze statue metal (Kaon).
    pub const STATUE_DARK: Material = 105;
    // ---- Giant's Room (toy-scale bedroom, ×30) ----
    /// Honey-oak floorboards (breakable top layer) and their big grain stripes.
    pub const FLOORBOARD: Material = 106;
    pub const FLOORBOARD_GRAIN: Material = 107;
    /// Indestructible subfloor (also the dark gaps between boards).
    pub const SUBFLOOR: Material = 108;
    /// The rug: dusty-blue pile, lighter tufts, cream border.
    pub const RUG: Material = 109;
    pub const RUG_TUFT: Material = 110;
    pub const RUG_BORDER: Material = 111;
    /// Indestructible painted wall and gloss-white trim (skirting, frames, ceiling ring).
    pub const WALL_PAINT: Material = 112;
    pub const TRIM: Material = 113;
    /// Ceiling plaster (soft: a hard upward launch punches through → star KO).
    pub const CEILING_PLASTER: Material = 114;
    /// Furniture: indestructible oak frame and its breakable top veneer.
    pub const FURNITURE_FRAME: Material = 115;
    pub const FURNITURE_TOP: Material = 116;
    /// Bedding.
    pub const MATTRESS: Material = 117;
    pub const DUVET: Material = 118;
    pub const DUVET_STAR: Material = 119;
    pub const PILLOW: Material = 120;
    /// Book spines and page edges.
    pub const BOOK_RED: Material = 121;
    pub const BOOK_BLUE: Material = 122;
    pub const BOOK_GREEN: Material = 123;
    pub const BOOK_PURPLE: Material = 124;
    pub const BOOK_PAGES: Material = 125;
    /// Toy plastic / painted blocks.
    pub const PLASTIC_RED: Material = 126;
    pub const PLASTIC_BLUE: Material = 127;
    pub const PLASTIC_YELLOW: Material = 128;
    pub const PLASTIC_GREEN: Material = 129;
    /// The sleeping cat.
    pub const CAT_GINGER: Material = 130;
    pub const CAT_CREAM: Material = 131;
    /// Sunlit curtain fabric.
    pub const CURTAIN: Material = 132;
    /// Laptop screen glow.
    pub const SCREEN_GLOW: Material = 133;
    /// Brass (curtain rod, clock rim, door knob).
    pub const BRASS: Material = 134;
    /// Knit sock.
    pub const SOCK_WHITE: Material = 135;
    // ---- Valley of the End (gorge, falls, statues) ----
    /// Mossy green-grey cliff stone, its darker strata band and pale ledge band.
    pub const VALLEY_CLIFF: Material = 136;
    pub const VALLEY_STRATA: Material = 137;
    pub const VALLEY_LEDGE: Material = 138;
    /// Moss on ledges and banks (grass-like).
    pub const VALLEY_MOSS: Material = 139;
    /// The statues: sage stone and its shaded grooves / folds.
    pub const STATUE_STONE: Material = 140;
    pub const STATUE_SHADE: Material = 141;
    /// Indestructible statue core (legs, torso, pedestal heart): statues never fully vanish.
    pub const STATUE_CORE: Material = 142;
    /// The waterfall face: white water and its blue streaks (indestructible, faintly self-lit).
    pub const WATERFALL: Material = 143;
    pub const WATERFALL_BLUE: Material = 144;
    /// River-bed pebbles.
    pub const RIVER_STONE: Material = 145;
    /// Dark pine / forest canopy.
    pub const PINE: Material = 146;
    /// Indestructible gorge bedrock and cliff cores.
    pub const VALLEY_CORE: Material = 147;
    // ---- Hyperline (the endless train) ----
    /// Indestructible pearl-white train frame (underframe, posts, ribs, nose, keel housings).
    pub const HYPER_FRAME: Material = 148;
    /// White roof / wall panels with a faint blue cast (breakable: holes into the cars).
    pub const HYPER_PANEL: Material = 149;
    // ---- Ember (volcanic world) ----
    /// Molten lava (the lava sheet is indestructible: it is the floor and the KO surface).
    pub const LAVA: Material = 150;
    /// White-hot lava veins.
    pub const LAVA_HOT: Material = 151;
    /// Dark cooling crust floating on the lava (still faintly glowing).
    pub const LAVA_CRUST: Material = 152;
    /// Grey volcanic ash (soft).
    pub const ASH: Material = 153;
    /// Dark warm basalt and its lighter jagged facets.
    pub const VOLCANIC: Material = 154;
    pub const VOLCANIC_LIGHT: Material = 155;
    /// Rock cracked with glowing magma (seams, vents, window slits; breakable).
    pub const MAGMA_SEAM: Material = 156;
    /// Black glassy obsidian (fortress).
    pub const OBSIDIAN: Material = 157;
    /// Indestructible fortress corner posts and spire spines.
    pub const OBSIDIAN_CORE: Material = 158;
    // ---- High Roller (casino) ----
    /// Patterned casino carpet: crimson field, gold and teal motif.
    pub const CASINO_CARPET: Material = 159;
    pub const CASINO_CARPET_GOLD: Material = 160;
    pub const CASINO_CARPET_TEAL: Material = 161;
    /// Table felt (cards, craps, the roulette layout).
    pub const FELT_GREEN: Material = 162;
    /// Roulette pockets.
    pub const ROULETTE_RED: Material = 163;
    pub const ROULETTE_BLACK: Material = 164;
    pub const ROULETTE_GREEN: Material = 165;
    /// Polished white marble and its grey vein.
    pub const MARBLE: Material = 166;
    pub const MARBLE_VEIN: Material = 167;
    /// Gilded trim (faintly self-lit so it glints in the warm light).
    pub const GOLD_TRIM: Material = 168;
    /// Indestructible damask wall panels / facade frame.
    pub const CASINO_WALL: Material = 169;
    /// Indestructible marble columns, slab rings and the roulette spindle.
    pub const CASINO_PILLAR: Material = 170;
    /// Indestructible cabinets of the big "high roller" slot machines.
    pub const SLOT_CABINET: Material = 171;
    /// Indestructible glowing gold trim / reel window of those machines.
    pub const SLOT_TRIM: Material = 172;
    /// Giant poker chips.
    pub const CHIP_WHITE: Material = 173;
    pub const CHIP_BLACK: Material = 174;
    pub const CHIP_PURPLE: Material = 175;
    /// Giant dice: ivory and translucent casino red.
    pub const DICE_IVORY: Material = 176;
    pub const DICE_RED: Material = 177;
    /// Red velvet (rope barriers, booths, cage curtain).
    pub const VELVET: Material = 178;
    pub const NEON_VIOLET: Material = 179;
    pub const NEON_GREEN: Material = 180;
    /// Chandelier crystal drops (bright, breakable).
    pub const CRYSTAL_DROP: Material = 181;
    // ---- Mirrorfold (the folded city) ----
    /// Pale warm limestone facade, its cool violet-grey shade (rustication / back of the fold) and
    /// the bright cornice / trim stone.
    pub const MIRROR_STONE: Material = 182;
    pub const MIRROR_SHADE: Material = 183;
    pub const MIRROR_CORNICE: Material = 184;
    /// Dark blue-violet window glass and a warm lit window (faint glow).
    pub const MIRROR_WINDOW: Material = 185;
    pub const MIRROR_WINDOW_LIT: Material = 186;
    /// Fracture seams: thin cyan-white hard-light lines where the city was cut (self-lit).
    pub const MIRROR_SEAM: Material = 187;
    /// Mirror-glass shards (crack walls, fragment faces): pale reflective crystal, faint glint.
    pub const MIRROR_SHARD: Material = 188;
    /// Plaza paving stone.
    pub const MIRROR_PAVING: Material = 189;
    /// Indestructible dark slate core (curl ribs, inverted-tower spines, fragment hearts).
    pub const MIRROR_CORE: Material = 190;
    // ---- Sakura Keep (Japanese castle town) ----
    /// Blue-grey roof tile and the pale ridge / eave-edge tiles.
    pub const SK_ROOF_TILE: Material = 191;
    pub const SK_ROOF_RIDGE: Material = 192;
    /// White lime plaster (house and castle walls).
    pub const SK_PLASTER: Material = 193;
    /// Vermilion lacquer (temple walls, pillars, torii) and gold trim (faint sheen).
    pub const SK_LACQUER: Material = 194;
    pub const SK_GOLD: Material = 195;
    /// Street pavers and their darker joints / steps.
    pub const SK_PAVER: Material = 196;
    pub const SK_PAVER_DARK: Material = 197;
    /// Raked gravel (garden paths, the courtyard).
    pub const SK_GRAVEL: Material = 198;
    /// Shoji paper panels, lit from behind (soft glow; tears easily).
    pub const SK_SHOJI: Material = 199;
    /// Red paper lanterns and warm lantern light (full glow).
    pub const SK_LANTERN: Material = 200;
    pub const SK_LANTERN_WARM: Material = 201;
    /// Dressed castle stone (the keep's base, plinths, stone lanterns).
    pub const SK_KEEP_STONE: Material = 202;
    /// Indestructible castle stone (the keep base's core, the plateau's bedrock).
    pub const SK_KEEP_CORE: Material = 203;
    /// Indestructible red lacquer columns (the keep hall's four pillars).
    pub const SK_PILLAR_CORE: Material = 204;
    /// Koi-pond water (dark teal, faintly self-lit) and a koi.
    pub const SK_KOI_WATER: Material = 205;
    pub const SK_KOI: Material = 206;
    /// Cherry bark, pale blossom (fallen petals, canopy highlights), pine needles, moss.
    pub const SK_BARK: Material = 207;
    pub const SK_BLOSSOM_LIGHT: Material = 208;
    pub const SK_PINE: Material = 209;
    pub const SK_MOSS: Material = 210;
    /// The plateau's cliff rock.
    pub const SK_CLIFF: Material = 211;
    /// The keep's painted panel: indigo wave paint and ink.
    pub const SK_PAINT_BLUE: Material = 212;
    pub const SK_INK: Material = 213;
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
pub static MATERIALS: [MaterialDef; 214] = [
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
    // ---- Turtle Lair (80..) ----
    def("sewer brick", [76, 96, 114], 0.09, 1.0, D::Stone),
    def("dark sewer brick", [52, 66, 86], 0.08, 1.0, D::Stone),
    MaterialDef {
        name: "sewer arch",
        color: [64, 74, 96],
        jitter: 0.06,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    glow(def("sewer water", [40, 148, 136], 0.06, 0.4, D::Crystal), 0.3),
    def("rusty iron", [150, 86, 54], 0.10, 1.4, D::Stone),
    def("tatami", [192, 182, 118], 0.05, 0.5, D::Dirt),
    glow(def("mutagen glow", [110, 255, 90], 0.05, 0.6, D::Crystal), 1.0),
    glow(def("moonlight", [196, 226, 255], 0.02, 0.6, D::Crystal), 1.0),
    def("city brick", [134, 68, 56], 0.08, 1.0, D::Stone),
    def("teal pipe", [58, 128, 124], 0.06, 1.4, D::Stone),
    // ---- Cybertron (90..) ----
    def("gunmetal plate", [66, 72, 88], 0.06, 1.3, D::Stone),
    def("blue-steel plate", [118, 130, 154], 0.05, 1.3, D::Stone),
    def("gunmetal trim", [34, 38, 50], 0.04, 1.5, D::Stone),
    glow(def("energon", [56, 196, 255], 0.05, 0.8, D::Crystal), 1.0),
    glow(def("amber seam", [255, 142, 40], 0.05, 1.0, D::Crystal), 1.0),
    glow(def("kaon red", [255, 64, 34], 0.05, 1.0, D::Crystal), 1.0),
    glow(def("energon pink", [255, 96, 206], 0.05, 0.6, D::Crystal), 0.9),
    def("rust dust", [156, 82, 44], 0.12, 0.45, D::Sand),
    def("rust dune", [104, 56, 38], 0.10, 0.5, D::Sand),
    MaterialDef {
        name: "cybertron frame",
        color: [52, 56, 70],
        jitter: 0.04,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    MaterialDef {
        name: "energon frame",
        color: [70, 206, 255],
        jitter: 0.03,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Crystal,
        emissive: 1.0,
    },
    MaterialDef {
        name: "ember frame",
        color: [255, 120, 30],
        jitter: 0.03,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Crystal,
        emissive: 1.0,
    },
    MaterialDef {
        name: "transforming plate",
        color: [104, 112, 134],
        jitter: 0.04,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    glow(def("autobot red", [220, 40, 44], 0.03, 1.2, D::Stone), 0.35),
    glow(def("decepticon purple", [150, 64, 226], 0.03, 1.2, D::Stone), 0.45),
    def("dark statue metal", [70, 58, 60], 0.05, 1.4, D::Stone),
    // ---- Giant's Room (106..) ----
    def("floorboard", [198, 150, 98], 0.05, 1.3, D::Wood),
    def("floorboard grain", [176, 128, 80], 0.05, 1.3, D::Wood),
    MaterialDef {
        name: "subfloor",
        color: [92, 62, 40],
        jitter: 0.04,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Wood,
        emissive: 0.0,
    },
    def("rug", [74, 112, 152], 0.07, 0.45, D::Dirt),
    def("rug tuft", [94, 136, 178], 0.10, 0.4, D::Dirt),
    def("rug border", [232, 214, 172], 0.05, 0.45, D::Dirt),
    MaterialDef {
        name: "wall paint",
        color: [184, 210, 228],
        jitter: 0.02,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    MaterialDef {
        name: "trim",
        color: [246, 242, 234],
        jitter: 0.02,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Wood,
        emissive: 0.0,
    },
    def("ceiling plaster", [246, 243, 236], 0.02, 0.35, D::Dirt),
    MaterialDef {
        name: "furniture frame",
        color: [180, 128, 80],
        jitter: 0.05,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Wood,
        emissive: 0.0,
    },
    def("furniture top", [190, 138, 88], 0.05, 1.2, D::Wood),
    def("mattress", [236, 236, 244], 0.03, 0.5, D::Dirt),
    def("duvet", [90, 134, 212], 0.05, 0.4, D::Dirt),
    def("duvet star", [252, 212, 92], 0.04, 0.4, D::Dirt),
    def("pillow", [248, 244, 232], 0.03, 0.3, D::Dirt),
    def("red book", [196, 54, 60], 0.06, 0.6, D::Wood),
    def("blue book", [54, 90, 172], 0.06, 0.6, D::Wood),
    def("green book", [60, 142, 92], 0.06, 0.6, D::Wood),
    def("purple book", [124, 74, 152], 0.06, 0.6, D::Wood),
    def("book pages", [240, 232, 208], 0.03, 0.6, D::Wood),
    def("red plastic", [228, 48, 46], 0.04, 0.7, D::Stone),
    def("blue plastic", [40, 108, 222], 0.04, 0.7, D::Stone),
    def("yellow plastic", [250, 204, 34], 0.04, 0.7, D::Stone),
    def("green plastic", [52, 178, 84], 0.04, 0.7, D::Stone),
    def("ginger fur", [228, 142, 64], 0.08, 0.5, D::Dirt),
    def("cream fur", [248, 224, 184], 0.06, 0.5, D::Dirt),
    def("curtain", [238, 178, 72], 0.05, 0.35, D::Dirt),
    glow(def("laptop screen", [124, 192, 255], 0.03, 0.5, D::Crystal), 0.85),
    def("brass", [214, 174, 82], 0.04, 1.6, D::Stone),
    def("knit sock", [236, 234, 228], 0.06, 0.3, D::Dirt),
    // ---- Valley of the End (136..) ----
    def("valley cliff", [118, 136, 114], 0.08, 1.1, D::Stone),
    def("valley strata", [88, 104, 94], 0.07, 1.2, D::Stone),
    def("valley ledge", [156, 164, 136], 0.07, 1.0, D::Stone),
    def("valley moss", [92, 146, 76], 0.12, 0.55, D::Grass),
    def("statue stone", [150, 166, 138], 0.05, 1.2, D::Stone),
    def("statue shade", [110, 126, 106], 0.05, 1.2, D::Stone),
    MaterialDef {
        name: "statue core",
        color: [104, 118, 100],
        jitter: 0.05,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    MaterialDef {
        name: "waterfall",
        color: [226, 242, 250],
        jitter: 0.10,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Crystal,
        emissive: 0.35,
    },
    MaterialDef {
        name: "waterfall streak",
        color: [150, 204, 232],
        jitter: 0.08,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Crystal,
        emissive: 0.25,
    },
    def("river stone", [118, 124, 116], 0.10, 0.9, D::Stone),
    def("pine", [42, 106, 60], 0.14, 0.25, D::Leaves),
    MaterialDef {
        name: "valley bedrock",
        color: [70, 80, 76],
        jitter: 0.06,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },    // ---- Hyperline (148..) ----
    MaterialDef {
        name: "hyperline frame",
        color: [226, 232, 240],
        jitter: 0.03,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    def("hyperline panel", [236, 242, 250], 0.035, 1.1, D::Stone),
    // ---- Ember (150..) ----
    MaterialDef {
        name: "lava",
        color: [255, 92, 18],
        jitter: 0.12,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Crystal,
        emissive: 1.0,
    },
    MaterialDef {
        name: "hot lava",
        color: [255, 200, 80],
        jitter: 0.08,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Crystal,
        emissive: 1.0,
    },
    MaterialDef {
        name: "lava crust",
        color: [112, 30, 16],
        jitter: 0.14,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.45,
    },
    def("ash", [106, 100, 98], 0.07, 0.45, D::Sand),
    def("volcanic rock", [48, 42, 46], 0.09, 1.2, D::Stone),
    def("light volcanic rock", [86, 76, 76], 0.09, 1.2, D::Stone),
    glow(def("magma seam", [255, 78, 26], 0.10, 1.0, D::Stone), 0.85),
    def("obsidian", [30, 26, 36], 0.05, 1.5, D::Crystal),
    MaterialDef {
        name: "obsidian core",
        color: [24, 20, 30],
        jitter: 0.04,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    // ---- High Roller casino (159..) ----
    def("casino carpet", [138, 18, 38], 0.07, 0.5, D::Dirt),
    def("casino carpet gold", [212, 158, 50], 0.07, 0.5, D::Dirt),
    def("casino carpet teal", [22, 104, 112], 0.07, 0.5, D::Dirt),
    def("felt", [24, 118, 62], 0.05, 0.45, D::Dirt),
    def("roulette red", [186, 20, 30], 0.04, 0.9, D::Wood),
    def("roulette black", [30, 26, 32], 0.04, 0.9, D::Wood),
    def("roulette green", [18, 150, 72], 0.04, 0.9, D::Wood),
    def("marble", [234, 230, 222], 0.04, 1.2, D::Stone),
    def("marble vein", [194, 186, 178], 0.05, 1.2, D::Stone),
    glow(def("gold trim", [238, 186, 70], 0.06, 1.0, D::Stone), 0.3),
    MaterialDef {
        name: "casino wall",
        color: [104, 26, 38],
        jitter: 0.05,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    MaterialDef {
        name: "casino pillar",
        color: [226, 220, 210],
        jitter: 0.04,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    MaterialDef {
        name: "slot cabinet",
        color: [176, 22, 44],
        jitter: 0.04,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    MaterialDef {
        name: "slot trim",
        color: [255, 204, 90],
        jitter: 0.05,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Crystal,
        emissive: 0.6,
    },
    def("chip white", [240, 238, 232], 0.03, 0.7, D::Wood),
    def("chip black", [34, 32, 38], 0.03, 0.7, D::Wood),
    def("chip purple", [118, 52, 170], 0.03, 0.7, D::Wood),
    def("dice ivory", [244, 238, 222], 0.02, 0.9, D::Crystal),
    glow(def("dice red", [210, 24, 40], 0.03, 0.9, D::Crystal), 0.2),
    def("velvet", [124, 14, 34], 0.06, 0.4, D::Dirt),
    glow(def("neon violet", [186, 80, 255], 0.05, 0.4, D::Crystal), 1.0),
    glow(def("neon green", [70, 255, 140], 0.05, 0.4, D::Crystal), 1.0),
    glow(def("crystal drop", [255, 246, 222], 0.08, 0.5, D::Crystal), 0.75),
    // ---- Mirrorfold (182..) ----
    def("mirror limestone", [206, 196, 178], 0.06, 1.0, D::Stone),
    def("mirror shade stone", [128, 124, 150], 0.06, 1.1, D::Stone),
    def("mirror cornice", [232, 226, 210], 0.04, 1.0, D::Stone),
    def("mirror window", [44, 54, 92], 0.10, 0.6, D::Crystal),
    glow(def("mirror lit window", [255, 204, 132], 0.10, 0.6, D::Crystal), 0.55),
    glow(def("mirror seam", [182, 244, 255], 0.05, 1.2, D::Crystal), 1.0),
    glow(def("mirror shard", [170, 200, 236], 0.12, 1.2, D::Crystal), 0.12),
    def("mirror paving", [176, 170, 162], 0.07, 1.0, D::Stone),
    MaterialDef {
        name: "mirror core",
        color: [62, 58, 82],
        jitter: 0.05,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    // ---- Sakura Keep (191..) ----
    def("roof tile", [66, 74, 94], 0.06, 0.9, D::Stone),
    def("ridge tile", [150, 152, 160], 0.04, 1.0, D::Stone),
    def("lime plaster", [240, 234, 220], 0.03, 0.7, D::Stone),
    def("vermilion lacquer", [196, 46, 36], 0.05, 0.9, D::Wood),
    glow(def("gold trim", [226, 176, 66], 0.05, 1.0, D::Stone), 0.15),
    def("street paver", [172, 166, 154], 0.06, 1.0, D::Stone),
    def("dark paver", [128, 124, 120], 0.06, 1.0, D::Stone),
    def("raked gravel", [220, 214, 196], 0.05, 0.45, D::Sand),
    glow(def("shoji paper", [248, 238, 212], 0.03, 0.3, D::Dirt), 0.4),
    glow(def("paper lantern", [255, 110, 56], 0.05, 0.3, D::Dirt), 1.0),
    glow(def("lantern light", [255, 210, 140], 0.04, 0.4, D::Crystal), 1.0),
    def("castle stone", [156, 150, 140], 0.08, 1.4, D::Stone),
    MaterialDef {
        name: "castle core",
        color: [118, 112, 106],
        jitter: 0.07,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Stone,
        emissive: 0.0,
    },
    MaterialDef {
        name: "keep pillar",
        color: [176, 40, 32],
        jitter: 0.04,
        hardness: f32::INFINITY,
        indestructible: true,
        debris: D::Wood,
        emissive: 0.0,
    },
    glow(def("koi pond", [40, 112, 112], 0.06, 0.4, D::Crystal), 0.12),
    def("koi", [250, 128, 40], 0.08, 0.3, D::Crystal),
    def("cherry bark", [70, 48, 46], 0.08, 0.8, D::Wood),
    def("pale blossom", [255, 208, 228], 0.10, 0.25, D::Leaves),
    def("pine needles", [44, 98, 68], 0.12, 0.25, D::Leaves),
    def("moss", [98, 130, 72], 0.10, 0.5, D::Grass),
    def("cliff rock", [112, 106, 108], 0.08, 1.2, D::Stone),
    def("wave indigo", [40, 82, 150], 0.05, 0.8, D::Stone),
    def("ink", [36, 34, 40], 0.04, 0.9, D::Stone),
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
