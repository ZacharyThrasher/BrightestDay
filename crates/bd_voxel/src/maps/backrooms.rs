//! **Backrooms** — five stacked levels of liminal room kits, all interconnected, every floor, ceiling
//! and wall breakable (DESIGN.md "Maps"; feature `backrooms`).
//!
//! The building fills the map: a 76 × 76 m footprint on a 19 × 19 grid of 4 m cells (walls on the
//! grid lines), five storeys of 8 m (1 m slab + 7 m clear) from 10 m to the roof at 50 m, standing
//! on an indestructible black basement. Layout per level (cells; +X east, +Z north):
//!
//! ```text
//!  L4 (42 m)  office maze ("level 0": carpet, wallpaper, drop ceiling)  │ dark playground
//!  L3 (34 m)  ═══════ hotel: 76 m spine corridor along the north side (the bhop lane) ═══════
//!             playground (W)  │  [ sky room, upper half: no floor ]  │  office (E)
//!  L2 (26 m)  office (W)      │  [ SKY ROOM 28 × 28 × 15 m ]         │  hotel, spine along z (E)
//!             pool (S) / office (N) around the sky room
//!  L1 (18 m)  office + a long corridor  │  poolrooms (SE) / office (NE)
//!  L0 (10 m)  parking garage with VOID PITS  │  poolrooms (E)
//! ```
//!
//! - **Seeded maze.** Zones are fixed (jittered by a cell); inside them rooms are grown as rectangles
//!   and joined by a random spanning tree of doorways plus extra loops, so every level is one
//!   connected piece. Office rooms are joined by full-height gaps under the drop ceiling (the maze
//!   look), other kits by 3.5 × 4.5 m doorways (arched in the poolrooms), hotel rooms by narrow
//!   doors — most of them *closed* (sealed rooms: render-culled until someone blows the door in).
//! - **Vertical links.** Two stairwells per storey (a 2-wide ramp out of the garage), two elevator
//!   shafts through every level (drop down, tether up), the double-height sky room, and anything a
//!   player punches through: slabs are 1 m of ordinary material over an indestructible ring.
//! - **KOs.** Pits in the garage floor drop into the void (bottom blast zone at [`VOID_Y`]); the
//!   roof is breakable, so a launch up through the storeys is a star KO; the outer walls are
//!   breakable between indestructible frame posts and carry big windows, so a hard hit out of the
//!   building is a side (or void) KO — the tether save still holds you on the facade.
//! - **Perf.** Office/hotel drop ceilings seal a plenum per room and closed hotel rooms are sealed
//!   ([`VoxelWorld::seal_enclosed_air`]): their faces are never meshed until opened. Light comes from
//!   emissive panels in the voxels (no point lights); the client's haze fog limits what reads.
//! - **Decor.** Seeded, capped props hug solid walls (never in doorways, stair mouths, spawn
//!   circles or the authored routes): see [`props`](self::props).

// Generator style: grid loops index several per-level tables at once.
#![allow(clippy::needless_range_loop, clippy::type_complexity)]

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::ArenaInfo;
use crate::city::City;
use crate::material::mat;
use crate::noise::{Rng, hash3, unit, value2};
use crate::world::VoxelWorld;

#[path = "backrooms_icons.rs"]
pub mod icons;
#[path = "backrooms_props.rs"]
pub mod props;

/// World dimensions in voxels (80 × 64 × 80 m). The building (76 m) fills it: the side blast zones
/// are just outside the facade, the top one 40 m over the world top (54 m above the roof).
pub const BACKROOMS_SIZE_VOXELS: IVec3 = IVec3::new(160, 128, 160);
/// Storeys.
pub const LEVELS: usize = 5;
/// Cells per side (4 m each).
pub const N: i32 = 19;
/// Voxels per cell edge (grid-line wall voxel + 7 interior).
pub const CELL: i32 = 8;
/// Voxel x / z of grid line 0 (the west / south facade).
pub const GRID0: i32 = -76;
/// Voxel y of the level-0 floor surface (first air voxel above the slab).
pub const FLOOR0: i32 = 20;
/// Floor-to-floor height in voxels (8 m).
pub const STOREY: i32 = 16;
/// Clear height in voxels (7 m): floor surface .. slab underside.
pub const CLEAR: i32 = 14;
/// Drop-ceiling height in voxels above the floor (5 m) in offices and hotel corridors.
pub const DROP: i32 = 10;
/// Doorway height in voxels (4.5 m).
pub const DOOR: i32 = 9;
/// Falling below this is the void KO (the pits' bottoms are at 0.5 m).
pub const VOID_Y: f32 = 4.0;
/// Roof surface height (m).
pub const ROOF_Y: f32 = 50.0;
/// The sky room (level 2, double height into level 3), in cells.
pub const HALL: CellRect = CellRect { x0: 6, z0: 6, x1: 13, z1: 13 };
/// Interior columns stand on these grid-line intersections (both axes).
const COLUMN_LINES: [i32; 4] = [3, 8, 11, 16];
/// Global prop cap (see [`props`]).
pub const MAX_PROPS: usize = 150;

/// Voxel y of level `l`'s floor surface.
#[inline]
pub fn floor_v(l: usize) -> i32 {
    FLOOR0 + STOREY * l as i32
}

/// Floor height of level `l` in metres (10, 18, 26, 34, 42; `LEVELS` = the roof, 50).
#[inline]
pub fn floor_y(l: usize) -> f32 {
    floor_v(l) as f32 * 0.5
}

/// The level whose storey contains height `y` (m); `None` below the lowest floor's slab or above
/// the roof slab.
pub fn level_at(y: f32) -> Option<usize> {
    let v = (y / 0.5).floor() as i32;
    (0..LEVELS).find(|&l| v >= floor_v(l) - 2 && v < floor_v(l) + STOREY - 2)
}

/// Voxel coordinate of grid line `i` (x or z).
#[inline]
pub fn line(i: i32) -> i32 {
    GRID0 + CELL * i
}

/// Centre of cell (i, j) in metres (x, z).
#[inline]
pub fn cell_center(i: i32, j: i32) -> Vec2 {
    Vec2::new((line(i) as f32 + 4.5) * 0.5, (line(j) as f32 + 4.5) * 0.5)
}

/// Cell containing metre coordinate (x, z) (may be outside `0..N`).
#[inline]
pub fn cell_of(x: f32, z: f32) -> (i32, i32) {
    let f = |m: f32| ((m / 0.5).floor() as i32 - GRID0).div_euclid(CELL);
    (f(x), f(z))
}

#[inline]
fn ci(i: i32, j: i32) -> usize {
    (i + N * j) as usize
}

/// Axis-aligned rectangle of cells `[x0, x1) × [z0, z1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRect {
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
}

impl CellRect {
    pub const fn new(x0: i32, z0: i32, x1: i32, z1: i32) -> Self {
        Self { x0, z0, x1, z1 }
    }
    pub fn contains(&self, i: i32, j: i32) -> bool {
        i >= self.x0 && i < self.x1 && j >= self.z0 && j < self.z1
    }
    pub fn w(&self) -> i32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> i32 {
        self.z1 - self.z0
    }
    fn cells(self) -> impl Iterator<Item = (i32, i32)> {
        (self.z0..self.z1).flat_map(move |j| (self.x0..self.x1).map(move |i| (i, j)))
    }
    /// Interior voxel range (inclusive) in x and z: everything strictly between the bounding lines.
    fn vox(&self) -> (i32, i32, i32, i32) {
        (line(self.x0) + 1, line(self.x1) - 1, line(self.z0) + 1, line(self.z1) - 1)
    }
    /// Centre in metres (x, z).
    pub fn center(&self) -> Vec2 {
        Vec2::new((line(self.x0) + line(self.x1)) as f32 * 0.25 + 0.25, (line(self.z0) + line(self.z1)) as f32 * 0.25 + 0.25)
    }
    fn overlaps(&self, o: &CellRect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.z0 < o.z1 && o.z0 < self.z1
    }
}

/// The room kits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kit {
    /// Yellow wallpaper, moist carpet, drop ceiling with fluorescent panels.
    Office,
    /// White tile, shallow glowing water, arches.
    Pool,
    /// Concrete, pillars, parking lines, cars; void pits on level 0.
    Garage,
    /// Burgundy carpet, wainscot, long corridors and (mostly closed) rooms.
    Hotel,
    /// Dark rubber, glow stars, ball pits, slides, climbing frames.
    Playground,
    /// The impossible room: grass floor, painted daylight sky, clouds.
    Sky,
    Stair,
    Shaft,
}

impl Kit {
    fn drop_ceiling(self) -> bool {
        matches!(self, Kit::Office | Kit::Hotel)
    }
    /// Same-kit neighbours merge into one open space (no walls between their rooms).
    fn open_plan(self) -> bool {
        matches!(self, Kit::Garage | Kit::Playground | Kit::Sky)
    }
    pub fn name(self) -> &'static str {
        match self {
            Kit::Office => "office",
            Kit::Pool => "poolrooms",
            Kit::Garage => "garage",
            Kit::Hotel => "hotel",
            Kit::Playground => "playground",
            Kit::Sky => "sky room",
            Kit::Stair => "stairwell",
            Kit::Shaft => "shaft",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Normal,
    Corridor,
    HotelRoom,
    /// Stairwell, lower storey (the steps).
    StairLow,
    /// Stairwell, upper storey (a walled hole the steps arrive through).
    StairHigh,
    Shaft,
    /// Upper half of the double-height sky room (no floor).
    HallUpper,
}

#[derive(Clone, Debug)]
struct Room {
    level: usize,
    kit: Kit,
    role: Role,
    rect: CellRect,
    /// Hotel room behind a closed door (sealed; render-culled until opened).
    closed: bool,
}

impl Room {
    fn no_floor(&self) -> bool {
        matches!(self.role, Role::StairHigh | Role::HallUpper) || (self.role == Role::Shaft && self.level > 0)
    }
    fn has_drop(&self) -> bool {
        self.kit.drop_ceiling() && !self.closed
    }
    /// Takes part in the per-level door graph.
    fn in_graph(&self) -> bool {
        matches!(self.role, Role::Normal | Role::Corridor) && !self.closed
    }
    /// A player can stand / spawn / find props here.
    fn walkable(&self) -> bool {
        matches!(self.role, Role::Normal | Role::Corridor) || (self.role == Role::HotelRoom && !self.closed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Wall,
    /// No wall at all (same room / open plan).
    Open,
    /// Full-width opening up to the drop ceiling (office maze).
    Gap,
    /// Full-width 4.5 m doorway.
    Door,
    /// Narrow hotel door, open / closed.
    HotelDoor,
    HotelClosed,
}

/// A grid edge: `vertical` = on line x = line(i) between cells (i-1, j) and (i, j); otherwise on
/// line z = line(j) between (i, j-1) and (i, j).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeRef {
    vertical: bool,
    i: i32,
    j: i32,
}

impl EdgeRef {
    fn between(a: (i32, i32), b: (i32, i32)) -> Self {
        if a.1 == b.1 { EdgeRef { vertical: true, i: a.0.max(b.0), j: a.1 } } else { EdgeRef { vertical: false, i: a.0, j: a.1.max(b.1) } }
    }
    /// The two cells (minus side, plus side).
    fn cells(&self) -> ((i32, i32), (i32, i32)) {
        if self.vertical { ((self.i - 1, self.j), (self.i, self.j)) } else { ((self.i, self.j - 1), (self.i, self.j)) }
    }
}

#[derive(Clone)]
struct Level {
    kit: Vec<Kit>,
    room: Vec<u32>,
    reserved: Vec<bool>,
    /// Stair mouths / shaft doors: keep clear (no pits, props or closed rooms).
    keep_clear: Vec<bool>,
    /// No props (routes).
    no_props: Vec<bool>,
    ev: Vec<Edge>,
    eh: Vec<Edge>,
}

const NO_ROOM: u32 = u32::MAX;

impl Level {
    fn new() -> Self {
        let nn = (N * N) as usize;
        Self {
            kit: vec![Kit::Office; nn],
            room: vec![NO_ROOM; nn],
            reserved: vec![false; nn],
            keep_clear: vec![false; nn],
            no_props: vec![false; nn],
            ev: vec![Edge::Wall; nn],
            eh: vec![Edge::Wall; nn],
        }
    }
    fn edge(&self, e: EdgeRef) -> Edge {
        if e.vertical { self.ev[ci(e.i, e.j)] } else { self.eh[ci(e.i, e.j)] }
    }
    fn set_edge(&mut self, e: EdgeRef, k: Edge) {
        if e.vertical {
            self.ev[ci(e.i, e.j)] = k;
        } else {
            self.eh[ci(e.i, e.j)] = k;
        }
    }
    fn room_at(&self, i: i32, j: i32) -> Option<u32> {
        if !(0..N).contains(&i) || !(0..N).contains(&j) {
            return None;
        }
        let r = self.room[ci(i, j)];
        (r != NO_ROOM).then_some(r)
    }
}

#[derive(Clone, Copy, Debug)]
struct Stair {
    /// Lower level (the steps); the hole is in `level + 1`'s floor.
    level: usize,
    rect: CellRect,
    /// Rising along x (else z), towards + (else −).
    along_x: bool,
    up: i32,
}

impl Stair {
    fn len_cells(&self) -> i32 {
        if self.along_x { self.rect.w() } else { self.rect.h() }
    }
    /// (cells inside at the low end, landing cells outside it) — and likewise for the high end.
    fn ends(&self) -> (Vec<((i32, i32), (i32, i32))>, Vec<((i32, i32), (i32, i32))>) {
        let r = self.rect;
        let mut low = Vec::new();
        let mut high = Vec::new();
        if self.along_x {
            let (lo_in, lo_out, hi_in, hi_out) =
                if self.up > 0 { (r.x0, r.x0 - 1, r.x1 - 1, r.x1) } else { (r.x1 - 1, r.x1, r.x0, r.x0 - 1) };
            for j in r.z0..r.z1 {
                low.push(((lo_in, j), (lo_out, j)));
                high.push(((hi_in, j), (hi_out, j)));
            }
        } else {
            let (lo_in, lo_out, hi_in, hi_out) =
                if self.up > 0 { (r.z0, r.z0 - 1, r.z1 - 1, r.z1) } else { (r.z1 - 1, r.z1, r.z0, r.z0 - 1) };
            for i in r.x0..r.x1 {
                low.push(((i, lo_in), (i, lo_out)));
                high.push(((i, hi_in), (i, hi_out)));
            }
        }
        (low, high)
    }
}

#[derive(Clone, Copy, Debug)]
struct Window {
    level: usize,
    /// 0: west (x = line 0), 1: east, 2: south, 3: north.
    side: u8,
    /// Cell index along the facade.
    k: i32,
}

struct Plan {
    levels: Vec<Level>,
    rooms: Vec<Room>,
    stairs: Vec<Stair>,
    shafts: Vec<(i32, i32)>,
    pits: Vec<CellRect>,
    windows: Vec<Window>,
    /// The long hotel corridor on level 3 (the bhop lane) and the level-1 office corridor.
    spines: Vec<(usize, CellRect)>,
}

impl Plan {
    fn room_of(&self, l: usize, i: i32, j: i32) -> Option<&Room> {
        self.levels[l].room_at(i, j).map(|r| &self.rooms[r as usize])
    }
    fn is_pit(&self, l: usize, i: i32, j: i32) -> bool {
        l == 0 && self.pits.iter().any(|p| p.contains(i, j))
    }
}

// ------------------------------------------------------------------------------------------------
// Planning

/// Zone kits per level (before the reserved rooms are cut out). Also returns the hotel zones
/// (rect, spine along x).
fn zone_kits(l: usize, rng: &mut Rng) -> (Vec<Kit>, Vec<(CellRect, bool)>) {
    let mut k = vec![Kit::Office; (N * N) as usize];
    let mut hotels = Vec::new();
    let h = HALL;
    let j1 = rng.int(-1, 2);
    let j2 = rng.int(-1, 2);
    for j in 0..N {
        for i in 0..N {
            k[ci(i, j)] = match l {
                0 => {
                    if i < 11 + j1 {
                        Kit::Garage
                    } else {
                        Kit::Pool
                    }
                }
                1 => {
                    if i < 10 + j1 || j >= 11 + j2 {
                        Kit::Office
                    } else {
                        Kit::Pool
                    }
                }
                2 => {
                    if h.contains(i, j) {
                        Kit::Sky
                    } else if i >= h.x1 {
                        Kit::Hotel
                    } else if i < h.x0 || j >= h.z1 {
                        Kit::Office
                    } else {
                        Kit::Pool
                    }
                }
                3 => {
                    if j >= h.z1 {
                        Kit::Hotel
                    } else if h.contains(i, j) {
                        Kit::Sky
                    } else if i < h.x0 {
                        Kit::Playground
                    } else {
                        Kit::Office
                    }
                }
                _ => {
                    if i < 12 + j1 {
                        Kit::Office
                    } else {
                        Kit::Playground
                    }
                }
            };
        }
    }
    match l {
        2 => hotels.push((CellRect::new(h.x1, 0, N, N), false)),
        3 => hotels.push((CellRect::new(0, h.z1, N, N), true)),
        _ => {}
    }
    (k, hotels)
}

fn shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        let j = rng.int(0, i as i32 + 1) as usize;
        v.swap(i, j);
    }
}

struct Dsu(Vec<u32>);
impl Dsu {
    fn new(n: usize) -> Self {
        Dsu((0..n as u32).collect())
    }
    fn find(&mut self, mut a: u32) -> u32 {
        while self.0[a as usize] != a {
            let p = self.0[self.0[a as usize] as usize];
            self.0[a as usize] = p;
            a = p;
        }
        a
    }
    fn union(&mut self, a: u32, b: u32) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        self.0[ra.max(rb) as usize] = ra.min(rb);
        true
    }
}

fn in_grid(c: (i32, i32)) -> bool {
    (0..N).contains(&c.0) && (0..N).contains(&c.1)
}

fn make_plan(rng: &mut Rng) -> Plan {
    let mut levels: Vec<Level> = (0..LEVELS).map(|_| Level::new()).collect();
    let mut hotel_zones: Vec<(usize, CellRect, bool)> = Vec::new();
    for (l, lv) in levels.iter_mut().enumerate() {
        let (k, hz) = zone_kits(l, rng);
        lv.kit = k;
        hotel_zones.extend(hz.into_iter().map(|(r, ax)| (l, r, ax)));
    }
    let in_hotel = |l: usize, i: i32, j: i32, hz: &[(usize, CellRect, bool)]| hz.iter().any(|(hl, r, _)| *hl == l && r.contains(i, j));
    let mut rooms: Vec<Room> = Vec::new();
    let add_room = |rooms: &mut Vec<Room>, levels: &mut [Level], l: usize, kit: Kit, role: Role, rect: CellRect, closed: bool| -> u32 {
        let id = rooms.len() as u32;
        rooms.push(Room { level: l, kit, role, rect, closed });
        for (i, j) in rect.cells() {
            let lv = &mut levels[l];
            lv.room[ci(i, j)] = id;
            lv.kit[ci(i, j)] = kit;
            lv.reserved[ci(i, j)] = true;
        }
        id
    };

    // ---- the sky room (level 2) and its upper half (level 3) ------------------------------------
    add_room(&mut rooms, &mut levels, 2, Kit::Sky, Role::Normal, HALL, false);
    add_room(&mut rooms, &mut levels, 3, Kit::Sky, Role::HallUpper, HALL, false);

    // ---- elevator shafts through every level ------------------------------------------------------
    let mut shafts: Vec<(i32, i32)> = Vec::new();
    let mut shaft_doors: Vec<(usize, EdgeRef)> = Vec::new();
    {
        let mut cands: Vec<(i32, i32)> = (2..N - 2).flat_map(|j| (2..N - 2).map(move |i| (i, j))).collect();
        shuffle(&mut cands, rng);
        for c in cands {
            if shafts.len() == 2 {
                break;
            }
            let ok = (0..LEVELS).all(|l| !levels[l].reserved[ci(c.0, c.1)] && !in_hotel(l, c.0, c.1, &hotel_zones))
                && !HALL.contains(c.0, c.1)
                && (c.0 - HALL.x0 + 1).abs().min((c.0 - HALL.x1).abs()) + (c.1 - HALL.z0).abs().min((c.1 - HALL.z1).abs()) > 0
                && shafts.iter().all(|s| (s.0 - c.0).abs() + (s.1 - c.1).abs() >= 9);
            if !ok {
                continue;
            }
            // A door on every level to a free, non-hotel neighbour.
            let mut doors = Vec::new();
            for l in 0..LEVELS {
                let mut nb = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                shuffle(&mut nb, rng);
                let pick = nb
                    .iter()
                    .map(|d| (c.0 + d.0, c.1 + d.1))
                    .find(|&n| in_grid(n) && !levels[l].reserved[ci(n.0, n.1)] && !in_hotel(l, n.0, n.1, &hotel_zones));
                match pick {
                    Some(n) => doors.push((l, n)),
                    None => break,
                }
            }
            if doors.len() < LEVELS {
                continue;
            }
            for l in 0..LEVELS {
                add_room(&mut rooms, &mut levels, l, Kit::Shaft, Role::Shaft, CellRect::new(c.0, c.1, c.0 + 1, c.1 + 1), false);
            }
            for (l, n) in doors {
                levels[l].keep_clear[ci(n.0, n.1)] = true;
                shaft_doors.push((l, EdgeRef::between(c, n)));
            }
            shafts.push(c);
        }
    }

    // ---- stairwells: two per storey --------------------------------------------------------------
    let mut stairs: Vec<Stair> = Vec::new();
    for l in 0..LEVELS - 1 {
        let mut cands: Vec<Stair> = Vec::new();
        for (width, len) in [(1, 3), (2, 4)] {
            for along_x in [true, false] {
                for up in [1, -1] {
                    let (w, h) = if along_x { (len, width) } else { (width, len) };
                    for z0 in 0..=N - h {
                        for x0 in 0..=N - w {
                            cands.push(Stair { level: l, rect: CellRect::new(x0, z0, x0 + w, z0 + h), along_x, up });
                        }
                    }
                }
            }
        }
        shuffle(&mut cands, rng);
        // The garage ramp first on level 0 (2 wide), then ordinary 1-wide stairwells.
        let mut chosen: Vec<Stair> = Vec::new();
        // Spread is relaxed pass by pass so a crowded storey still gets its two stairwells.
        for (pass, spread) in [(0, 30.0), (1, 30.0), (2, 18.0), (3, 8.0)] {
            for s in &cands {
                if chosen.len() == 2 {
                    break;
                }
                let wide = s.rect.w().min(s.rect.h()) == 2;
                let want_wide = l == 0 && pass == 0 && chosen.is_empty();
                if wide != want_wide {
                    continue;
                }
                let cells: Vec<(i32, i32)> = s.rect.cells().collect();
                let free = |lv: usize, c: (i32, i32)| {
                    let x = ci(c.0, c.1);
                    !levels[lv].reserved[x] && !levels[lv].keep_clear[x] && !in_hotel(lv, c.0, c.1, &hotel_zones)
                };
                if !cells.iter().all(|&c| free(l, c) && free(l + 1, c)) {
                    continue;
                }
                if want_wide && !cells.iter().all(|&c| levels[0].kit[ci(c.0, c.1)] == Kit::Garage) {
                    continue;
                }
                let (low, high) = s.ends();
                let land_ok = |lv: usize, outs: &[((i32, i32), (i32, i32))]| {
                    outs.iter().all(|&(_, o)| in_grid(o) && free(lv, o) && !HALL.contains(o.0, o.1) || (lv == 2 && HALL.contains(o.0, o.1)))
                        && outs.iter().all(|&(_, o)| in_grid(o))
                };
                if !land_ok(l, &low) || !land_ok(l + 1, &high) {
                    continue;
                }
                // Spread: away from the other stairwells of this storey and from the ones below.
                let c = s.rect.center();
                if chosen.iter().chain(stairs.iter().filter(|o| o.level + 1 == l)).any(|o| o.rect.center().distance(c) < spread) {
                    continue;
                }
                // Keep a cell between stairwells of this storey (their mouths stay clear).
                if chosen.iter().any(|o| CellRect::new(o.rect.x0 - 1, o.rect.z0 - 1, o.rect.x1 + 1, o.rect.z1 + 1).overlaps(&s.rect)) {
                    continue;
                }
                chosen.push(*s);
            }
            if chosen.len() == 2 {
                break;
            }
        }
        debug_assert!(!chosen.is_empty(), "no stairwell between levels {l} and {}", l + 1);
        for s in chosen {
            add_room(&mut rooms, &mut levels, l, Kit::Stair, Role::StairLow, s.rect, false);
            add_room(&mut rooms, &mut levels, l + 1, Kit::Stair, Role::StairHigh, s.rect, false);
            let (low, high) = s.ends();
            for &(_, o) in &low {
                levels[l].keep_clear[ci(o.0, o.1)] = true;
            }
            for &(_, o) in &high {
                levels[l + 1].keep_clear[ci(o.0, o.1)] = true;
            }
            stairs.push(s);
        }
    }

    // ---- void pits in the garage floor (level 0) --------------------------------------------------
    let mut pits: Vec<CellRect> = Vec::new();
    {
        let sizes = [(2, 2), (1, 2), (2, 1), (1, 1)];
        let mut tries = 0;
        while pits.len() < 4 && tries < 400 {
            tries += 1;
            let (w, h) = sizes[pits.len() % sizes.len()];
            let x0 = rng.int(1, N - w);
            let z0 = rng.int(1, N - h);
            let r = CellRect::new(x0, z0, x0 + w, z0 + h);
            // A clear ring of garage around it (stays walkable) and away from other pits.
            let ring = CellRect::new(x0 - 1, z0 - 1, x0 + w + 1, z0 + h + 1);
            let ok = ring.cells().all(|(i, j)| {
                in_grid((i, j))
                    && levels[0].kit[ci(i, j)] == Kit::Garage
                    && !levels[0].reserved[ci(i, j)]
                    && !levels[0].keep_clear[ci(i, j)]
            }) && pits.iter().all(|p| !CellRect::new(p.x0 - 1, p.z0 - 1, p.x1 + 1, p.z1 + 1).overlaps(&r));
            if ok {
                pits.push(r);
            }
        }
    }

    // ---- corridors: the level-3 hotel spine is carved by the hotel generator; offices get one ----
    let mut spines: Vec<(usize, CellRect)> = Vec::new();
    {
        // Level 1: a 2-wide office corridor along x through the office zone.
        let z0 = 8 + rng.int(0, 2);
        let mut x1 = 0;
        while x1 < N && levels[1].kit[ci(x1, z0)] == Kit::Office && levels[1].kit[ci(x1, z0 + 1)] == Kit::Office {
            x1 += 1;
        }
        let mut x0 = 0;
        // Stop at anything reserved (stairs / shafts) — the corridor is the longest free run.
        let mut best = (0, 0);
        let mut run_start = None;
        for i in 0..=x1 {
            let free = i < x1 && !levels[1].reserved[ci(i, z0)] && !levels[1].reserved[ci(i, z0 + 1)];
            match (free, run_start) {
                (true, None) => run_start = Some(i),
                (false, Some(s)) => {
                    if i - s > best.1 - best.0 {
                        best = (s, i);
                    }
                    run_start = None;
                }
                _ => {}
            }
        }
        x0 = x0.max(best.0);
        if best.1 - x0 >= 5 {
            let r = CellRect::new(x0, z0, best.1, z0 + 2);
            add_room(&mut rooms, &mut levels, 1, Kit::Office, Role::Corridor, r, false);
            spines.push((1, r));
        }
    }

    // ---- hotels -----------------------------------------------------------------------------------
    for &(l, zone, along_x) in &hotel_zones {
        // Spine: 2 cells wide, centred on the zone's short axis; room rows either side; cross
        // corridors through the row that faces the rest of the level.
        let (s0, s1) = if along_x { (zone.z0, zone.z1) } else { (zone.x0, zone.x1) };
        let (a0, a1) = if along_x { (zone.x0, zone.x1) } else { (zone.z0, zone.z1) };
        let sp0 = s0 + (s1 - s0 - 2) / 2;
        let mk = |a0: i32, a1: i32, b0: i32, b1: i32| if along_x { CellRect::new(a0, b0, a1, b1) } else { CellRect::new(b0, a0, b1, a1) };
        let spine = mk(a0, a1, sp0, sp0 + 2);
        if spine.cells().any(|(i, j)| levels[l].reserved[ci(i, j)]) {
            continue; // never happens: nothing is placed in hotel zones
        }
        add_room(&mut rooms, &mut levels, l, Kit::Hotel, Role::Corridor, spine, false);
        if l == 3 {
            spines.push((l, spine));
        }
        // Rows: (b0, b1, faces_interior).
        let outer_lo = s0 == 0;
        let outer_hi = s1 == N;
        let rows = [(s0, sp0, !outer_lo), (sp0 + 2, s1, !outer_hi)];
        for (b0, b1, interior) in rows {
            if b1 <= b0 {
                continue;
            }
            // Cross corridors run through the row to the rest of the level: only where the cell
            // beyond the row is ordinary floor (not a stairwell, shaft or the sky-room void).
            let beyond = if b0 == s0 { s0 - 1 } else { s1 };
            let mut last_cross = a0 - 10;
            let mut a = a0;
            while a < a1 {
                let out = if along_x { (a, beyond) } else { (beyond, a) };
                let free_beyond = in_grid(out) && !levels[l].reserved[ci(out.0, out.1)];
                let cross = interior && free_beyond && a - last_cross >= 4 && a > a0 && a < a1 - 1;
                if cross {
                    last_cross = a;
                }
                let r = mk(a, a + 1, b0, b1);
                if r.cells().any(|(i, j)| levels[l].reserved[ci(i, j)]) {
                    a += 1;
                    continue;
                }
                if cross {
                    add_room(&mut rooms, &mut levels, l, Kit::Hotel, Role::Corridor, r, false);
                } else {
                    let closed = rng.chance(0.62);
                    add_room(&mut rooms, &mut levels, l, Kit::Hotel, Role::HotelRoom, r, closed);
                }
                a += 1;
            }
        }
    }

    // ---- grow rectangular rooms in every remaining cell ---------------------------------------
    for l in 0..LEVELS {
        let mut order: Vec<(i32, i32)> = (0..N).flat_map(|j| (0..N).map(move |i| (i, j))).collect();
        shuffle(&mut order, rng);
        for (i, j) in order {
            if levels[l].room[ci(i, j)] != NO_ROOM {
                continue;
            }
            let kit = levels[l].kit[ci(i, j)];
            let (lo, hi) = match kit {
                Kit::Office => (1, 5),
                Kit::Pool => (2, 6),
                Kit::Garage => (4, 12),
                Kit::Playground => (3, 8),
                _ => (1, 4),
            };
            let (tw, th) = (rng.int(lo, hi), rng.int(lo, hi));
            let mut r = CellRect::new(i, j, i + 1, j + 1);
            let free = |levels: &[Level], c: (i32, i32)| {
                in_grid(c) && levels[l].room[ci(c.0, c.1)] == NO_ROOM && levels[l].kit[ci(c.0, c.1)] == kit
            };
            let mut dirs = [0u8, 1, 2, 3];
            loop {
                shuffle(&mut dirs, rng);
                let mut grew = false;
                for d in dirs {
                    let (cand, cells): (CellRect, Vec<(i32, i32)>) = match d {
                        0 if r.w() < tw => (CellRect { x1: r.x1 + 1, ..r }, (r.z0..r.z1).map(|z| (r.x1, z)).collect()),
                        1 if r.w() < tw => (CellRect { x0: r.x0 - 1, ..r }, (r.z0..r.z1).map(|z| (r.x0 - 1, z)).collect()),
                        2 if r.h() < th => (CellRect { z1: r.z1 + 1, ..r }, (r.x0..r.x1).map(|x| (x, r.z1)).collect()),
                        3 if r.h() < th => (CellRect { z0: r.z0 - 1, ..r }, (r.x0..r.x1).map(|x| (x, r.z0 - 1)).collect()),
                        _ => continue,
                    };
                    if cells.iter().all(|&c| free(&levels, c)) {
                        r = cand;
                        grew = true;
                        break;
                    }
                }
                if !grew {
                    break;
                }
            }
            add_room(&mut rooms, &mut levels, l, kit, Role::Normal, r, false);
        }
    }

    // ---- edges ------------------------------------------------------------------------------------
    let all_edges: Vec<EdgeRef> = (0..N)
        .flat_map(|j| (0..N).flat_map(move |i| [EdgeRef { vertical: true, i, j }, EdgeRef { vertical: false, i, j }]))
        .filter(|e| if e.vertical { e.i >= 1 } else { e.j >= 1 })
        .collect();
    let mut dsu = Dsu::new(rooms.len());
    for l in 0..LEVELS {
        let lv = &mut levels[l];
        for &e in &all_edges {
            let (a, b) = e.cells();
            let (ra, rb) = (lv.room[ci(a.0, a.1)], lv.room[ci(b.0, b.1)]);
            if ra == rb {
                lv.set_edge(e, Edge::Open);
                continue;
            }
            let (qa, qb) = (&rooms[ra as usize], &rooms[rb as usize]);
            // Open plan: garage / playground rooms merge; hotel corridors join each other.
            let open = (qa.kit == qb.kit && qa.kit.open_plan() && qa.role == qb.role)
                || (qa.kit == Kit::Hotel && qb.kit == Kit::Hotel && qa.role == Role::Corridor && qb.role == Role::Corridor);
            if open {
                lv.set_edge(e, Edge::Open);
                dsu.union(ra, rb);
            }
        }
    }
    // Stair and shaft doors.
    for s in &stairs {
        let (low, high) = s.ends();
        for (inn, out) in low {
            levels[s.level].set_edge(EdgeRef::between(inn, out), Edge::Door);
        }
        for (inn, out) in high {
            levels[s.level + 1].set_edge(EdgeRef::between(inn, out), Edge::Door);
        }
    }
    for &(l, e) in &shaft_doors {
        levels[l].set_edge(e, Edge::Door);
    }
    // Hotel rooms: one door onto the corridor they face.
    for (id, room) in rooms.iter().enumerate() {
        if room.role != Role::HotelRoom {
            continue;
        }
        let lv = &levels[room.level];
        let mut best: Option<EdgeRef> = None;
        for (i, j) in room.rect.cells() {
            for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (i + d.0, j + d.1);
                if let Some(r) = lv.room_at(n.0, n.1)
                    && r as usize != id
                    && rooms[r as usize].role == Role::Corridor
                    && rooms[r as usize].kit == Kit::Hotel
                {
                    let e = EdgeRef::between((i, j), n);
                    if best.is_none_or(|b| e < b) {
                        best = Some(e);
                    }
                }
            }
        }
        if let Some(e) = best {
            let k = if room.closed { Edge::HotelClosed } else { Edge::HotelDoor };
            levels[room.level].set_edge(e, k);
        }
    }
    // Spanning tree of doorways per level, plus loops.
    for l in 0..LEVELS {
        let lv = &levels[l];
        let mut pairs: Vec<(u32, u32, EdgeRef)> = Vec::new();
        for &e in &all_edges {
            let (a, b) = e.cells();
            let (ra, rb) = (lv.room[ci(a.0, a.1)], lv.room[ci(b.0, b.1)]);
            if ra == rb || lv.edge(e) != Edge::Wall {
                continue;
            }
            if !rooms[ra as usize].in_graph() || !rooms[rb as usize].in_graph() {
                continue;
            }
            pairs.push((ra.min(rb), ra.max(rb), e));
        }
        pairs.sort();
        // Group by room pair.
        let mut groups: Vec<(u32, u32, Vec<EdgeRef>)> = Vec::new();
        for (a, b, e) in pairs {
            match groups.last_mut() {
                Some(g) if g.0 == a && g.1 == b => g.2.push(e),
                _ => groups.push((a, b, vec![e])),
            }
        }
        shuffle(&mut groups, rng);
        let mut doors: Vec<(EdgeRef, Edge)> = Vec::new();
        for (a, b, edges) in &groups {
            let (qa, qb) = (&rooms[*a as usize], &rooms[*b as usize]);
            let tree = dsu.union(*a, *b);
            let both_office = qa.kit == Kit::Office && qb.kit == Kit::Office;
            let p_extra = if both_office { 0.5 } else { 0.22 };
            if !tree && !rng.chance(p_extra) {
                continue;
            }
            let kind = if both_office { Edge::Gap } else { Edge::Door };
            let mut es = edges.clone();
            shuffle(&mut es, rng);
            let n = if es.len() >= 3 && (both_office || rng.chance(0.4)) { 2 } else { 1 };
            for e in es.into_iter().take(n) {
                doors.push((e, kind));
            }
        }
        for (e, k) in doors {
            levels[l].set_edge(e, k);
        }
    }
    // Balconies: doorways from level 3 into the air over the sky room.
    {
        let lv = &levels[3];
        let mut cands: Vec<EdgeRef> = Vec::new();
        for &e in &all_edges {
            let (a, b) = e.cells();
            let (ra, rb) = (lv.room[ci(a.0, a.1)], lv.room[ci(b.0, b.1)]);
            let (qa, qb) = (&rooms[ra as usize], &rooms[rb as usize]);
            if (qa.role == Role::HallUpper) != (qb.role == Role::HallUpper) {
                let other = if qa.role == Role::HallUpper { qb } else { qa };
                if other.in_graph() {
                    cands.push(e);
                }
            }
        }
        shuffle(&mut cands, rng);
        let mut picked: Vec<EdgeRef> = Vec::new();
        for e in cands {
            if picked.len() >= 4 {
                break;
            }
            if picked.iter().any(|p| p.vertical == e.vertical && (p.i - e.i).abs() + (p.j - e.j).abs() < 3) {
                continue;
            }
            picked.push(e);
        }
        for e in picked {
            levels[3].set_edge(e, Edge::Door);
        }
    }

    // ---- windows: big panes on the facade, a few per level ---------------------------------------
    let mut windows: Vec<Window> = Vec::new();
    for l in 0..LEVELS {
        let mut cands: Vec<Window> = Vec::new();
        for side in 0..4u8 {
            for k in 1..N - 1 {
                let (i, j) = match side {
                    0 => (0, k),
                    1 => (N - 1, k),
                    2 => (k, 0),
                    _ => (k, N - 1),
                };
                let Some(r) = levels[l].room_at(i, j) else { continue };
                let room = &rooms[r as usize];
                if room.walkable() && !matches!(room.kit, Kit::Stair | Kit::Shaft) {
                    cands.push(Window { level: l, side, k });
                }
            }
        }
        shuffle(&mut cands, rng);
        let mut picked: Vec<Window> = Vec::new();
        for w in cands {
            if picked.len() >= 3 {
                break;
            }
            if picked.iter().any(|p| p.side == w.side) {
                continue;
            }
            picked.push(w);
        }
        windows.extend(picked);
    }

    // Routes stay clear of props.
    for &(l, r) in &spines {
        for (i, j) in r.cells() {
            levels[l].no_props[ci(i, j)] = true;
        }
    }
    let mid = (HALL.z0 + HALL.z1) / 2;
    for i in HALL.x0..HALL.x1 {
        for j in mid - 1..=mid + 1 {
            levels[2].no_props[ci(i, j)] = true;
        }
    }

    Plan { levels, rooms, stairs, shafts, pits, windows, spines }
}

// ------------------------------------------------------------------------------------------------
// Materials

fn floor_mat(kit: Kit, v: IVec3, seed: u32) -> Material {
    let (x, z) = (v.x, v.z);
    match kit {
        Kit::Office => {
            if value2(x as f32 * 0.09, z as f32 * 0.09, seed) > 0.32 {
                mat::CARPET_DAMP
            } else {
                mat::CARPET
            }
        }
        Kit::Pool => {
            if x.rem_euclid(4) == 0 || z.rem_euclid(4) == 0 {
                mat::POOL_GROUT
            } else {
                mat::POOL_TILE
            }
        }
        Kit::Garage | Kit::Stair => mat::GARAGE_CONCRETE,
        Kit::Hotel => {
            if (x + z).rem_euclid(8) == 0 || (x - z).rem_euclid(8) == 0 {
                mat::HOTEL_CARPET_GOLD
            } else {
                mat::HOTEL_CARPET
            }
        }
        Kit::Playground => {
            if (x.div_euclid(4) + z.div_euclid(4)).rem_euclid(2) == 0 {
                mat::PLAY_MAT
            } else {
                mat::HULL_NAVY
            }
        }
        Kit::Sky => mat::GRASS,
        Kit::Shaft => {
            if (x + z).rem_euclid(4) < 2 {
                mat::PAINT_YELLOW
            } else {
                mat::ROOF_DARK
            }
        }
    }
}

/// Wall face material: `yrel` = voxels above the floor, `along` = the coordinate along the wall.
fn wall_mat(room: &Room, yrel: i32, along: i32) -> Material {
    match room.kit {
        Kit::Office => {
            if yrel == 0 || along.rem_euclid(3) == 0 {
                mat::WALLPAPER_STRIPE
            } else {
                mat::WALLPAPER
            }
        }
        Kit::Pool => {
            if along.rem_euclid(4) == 0 || yrel.rem_euclid(4) == 0 {
                mat::POOL_GROUT
            } else {
                mat::POOL_TILE
            }
        }
        Kit::Garage => {
            if yrel == 2 {
                mat::PAINT_YELLOW
            } else {
                mat::GARAGE_CONCRETE
            }
        }
        Kit::Hotel => match yrel {
            0..=2 => mat::DARK_WOOD,
            3 => mat::WOOD,
            _ => mat::HOTEL_WALL,
        },
        Kit::Playground => match yrel {
            3 | 4 => [mat::PAINT_RED, mat::PAINT_YELLOW, mat::STUCCO_SKY, mat::STUCCO_MINT][along.div_euclid(4).rem_euclid(4) as usize],
            _ => mat::PLAY_MAT,
        },
        Kit::Sky => {
            if room.role == Role::Normal && yrel <= 2 {
                mat::LEAVES_GREEN
            } else {
                mat::SKY_PAINT
            }
        }
        Kit::Stair => {
            if yrel == 2 {
                mat::PAINT_YELLOW
            } else {
                mat::GARAGE_CONCRETE
            }
        }
        Kit::Shaft => {
            if yrel <= 1 {
                if (along + yrel).rem_euclid(4) < 2 { mat::PAINT_YELLOW } else { mat::ROOF_DARK }
            } else {
                mat::GARAGE_CONCRETE
            }
        }
    }
}

/// Slab-underside material for a room without a drop ceiling.
fn ceil_mat(room: &Room) -> Material {
    match room.kit {
        Kit::Pool => mat::POOL_TILE,
        Kit::Playground => mat::PLAY_MAT,
        Kit::Sky => mat::SKY_PAINT,
        _ => mat::GARAGE_CONCRETE,
    }
}

// ------------------------------------------------------------------------------------------------
// Building

/// Shell, basement, slabs (floors + ceilings + lights) and the roof.
fn build_slabs(c: &mut City, p: &Plan, seed: u32) {
    let (lo, hi) = (line(0) - 1, line(N) + 1);
    // Basement: solid indestructible black under everything.
    c.w.fill_box(IVec3::new(lo, 0, lo), IVec3::new(hi, floor_v(0) - 3, hi), mat::VOID_BLACK);
    for s in 0..=LEVELS {
        let yt = floor_v(s) - 1; // slab top voxel (floor of level s)
        let yb = floor_v(s) - 2; // slab underside (ceiling of level s - 1)
        for z in lo..=hi {
            for x in lo..=hi {
                let perimeter = x <= line(0) || x >= line(N) || z <= line(0) || z >= line(N);
                if perimeter {
                    c.w.set(IVec3::new(x, yt, z), mat::SHELL);
                    c.w.set(IVec3::new(x, yb, z), mat::SHELL);
                    continue;
                }
                let (i, j) = ((x - GRID0).div_euclid(CELL).min(N - 1), (z - GRID0).div_euclid(CELL).min(N - 1));
                // Above: the floor.
                let top = if s < LEVELS {
                    let above = p.room_of(s, i, j);
                    let hole = above.is_some_and(|r| r.no_floor() && in_room_interior(r, x, z)) || (s == 0 && pit_interior(p, x, z));
                    if hole { None } else { Some(above.map_or(mat::GARAGE_CONCRETE, |r| floor_mat(r.kit, IVec3::new(x, yt, z), seed))) }
                } else {
                    Some(mat::ROOF_DARK)
                };
                // Below: the ceiling of level s - 1.
                let below = if s > 0 { p.room_of(s - 1, i, j) } else { None };
                let bottom = match below {
                    _ if top.is_none() => None,
                    Some(r) if !r.has_drop() => Some(ceiling_light(r, x, z, i, j, seed).unwrap_or_else(|| ceil_mat(r))),
                    Some(_) => Some(mat::GARAGE_CONCRETE),
                    None => Some(mat::VOID_BLACK),
                };
                if let Some(m) = top {
                    c.w.set(IVec3::new(x, yt, z), m);
                }
                if let Some(m) = bottom {
                    c.w.set(IVec3::new(x, yb, z), m);
                }
            }
        }
    }
    // Pits: chutes through the basement down to the black floor (y = 0 stays).
    for r in &p.pits {
        let (x0, x1, z0, z1) = r.vox();
        c.w.fill_box(IVec3::new(x0, 1, z0), IVec3::new(x1, floor_v(0) - 1, z1), AIR);
        // Hazard ring on the floor around it.
        for z in z0 - 2..=z1 + 2 {
            for x in x0 - 2..=x1 + 2 {
                let inside = x >= x0 && x <= x1 && z >= z0 && z <= z1;
                if inside {
                    continue;
                }
                let v = IVec3::new(x, floor_v(0) - 1, z);
                if c.w.get(v) != AIR && c.w.get(v) != mat::SHELL {
                    c.w.set(v, if (x + z).rem_euclid(4) < 2 { mat::HAZARD_ORANGE } else { mat::ROOF_DARK });
                }
            }
        }
    }
}

/// The sky room's pond (and the giant duck floating in it), metres (x, z).
pub fn pond_spot() -> Vec2 {
    let (_, x1, z0, _) = HALL.vox();
    Vec2::new(x1 as f32 * 0.5 - 7.0, z0 as f32 * 0.5 + 6.0)
}

fn in_room_interior(r: &Room, x: i32, z: i32) -> bool {
    let (x0, x1, z0, z1) = r.rect.vox();
    x >= x0 && x <= x1 && z >= z0 && z <= z1
}

fn pit_interior(p: &Plan, x: i32, z: i32) -> bool {
    p.pits.iter().any(|r| {
        let (x0, x1, z0, z1) = r.vox();
        x >= x0 && x <= x1 && z >= z0 && z <= z1
    })
}

/// Light panels in a slab underside (rooms without a drop ceiling). `None` = plain ceiling.
fn ceiling_light(r: &Room, x: i32, z: i32, i: i32, j: i32, seed: u32) -> Option<Material> {
    let (lx, lz) = (x - line(i), z - line(j)); // 0 = on the line, 1..7 interior
    let dead = unit(hash3(i, r.level as i32, j, seed ^ 0xdead)) < 0.1;
    let lamp = if dead { mat::FLUORESCENT_DEAD } else { mat::FLUORESCENT };
    match r.kit {
        Kit::Pool => ((2..=5).contains(&lx) && (3..=4).contains(&lz)).then_some(lamp),
        Kit::Garage => {
            let sodium = (i + j).rem_euclid(3) == 0;
            (lz == 4 && (2..=6).contains(&lx)).then_some(if dead {
                mat::FLUORESCENT_DEAD
            } else if sodium {
                mat::NEON_AMBER
            } else {
                mat::FLUORESCENT
            })
        }
        Kit::Stair | Kit::Shaft => ((3..=4).contains(&lx) && (3..=4).contains(&lz)).then_some(mat::FLUORESCENT),
        Kit::Playground => {
            let h = hash3(x, r.level as i32, z, seed ^ 0x57a2);
            (unit(h) < 0.018 && lx != 0 && lz != 0)
                .then(|| [mat::NEON_CYAN, mat::NEON_AMBER, mat::NEON_PINK, mat::CLOUD][(h >> 8) as usize % 4])
        }
        _ => None,
    }
}

/// Wall material for the room on one side of an edge at a voxel.
fn edge_wall(p: &Plan, l: usize, cell: (i32, i32), yrel: i32, along: i32) -> Material {
    match p.room_of(l, cell.0, cell.1) {
        Some(r) => wall_mat(r, yrel, along),
        None => mat::GARAGE_CONCRETE,
    }
}

/// Walls, posts, doors, outer walls, columns, windows and drop ceilings of level `l`.
fn build_walls(c: &mut City, p: &Plan, l: usize, seed: u32) {
    let lv = &p.levels[l];
    let yf = floor_v(l);
    let ys = yf..yf + CLEAR;
    // Interior edges.
    for j in 0..N {
        for i in 0..N {
            for vertical in [true, false] {
                if (vertical && i == 0) || (!vertical && j == 0) {
                    continue;
                }
                let e = EdgeRef { vertical, i, j };
                let kind = lv.edge(e);
                if kind == Edge::Open {
                    continue;
                }
                let (a, b) = e.cells();
                let (ka, kb) = (lv.kit[ci(a.0, a.1)], lv.kit[ci(b.0, b.1)]);
                let ra = p.room_of(l, a.0, a.1);
                let rb = p.room_of(l, b.0, b.1);
                let double = ka != kb || ra.map(|r| r.role) != rb.map(|r| r.role) && (ka == Kit::Sky || kb == Kit::Sky);
                let lpos = if vertical { line(i) } else { line(j) };
                let (s0, s1) = if vertical { (line(j) + 1, line(j) + 7) } else { (line(i) + 1, line(i) + 7) };
                let at =
                    |along: i32, across: i32, y: i32| if vertical { IVec3::new(across, y, along) } else { IVec3::new(along, y, across) };
                for y in ys.clone() {
                    let yrel = y - yf;
                    for s in s0..=s1 {
                        c.w.set(at(s, lpos, y), edge_wall(p, l, b, yrel, s));
                        if double {
                            c.w.set(at(s, lpos - 1, y), edge_wall(p, l, a, yrel, s));
                        }
                    }
                    // Close the inner layer's corner notch when the next edge along is double too.
                    if double {
                        for (s, n) in [(s0 - 1, -1), (s1 + 1, 1)] {
                            let next = if vertical { EdgeRef { vertical, i, j: j + n } } else { EdgeRef { vertical, i: i + n, j } };
                            let inside = if vertical { (0..N).contains(&(j + n)) } else { (0..N).contains(&(i + n)) };
                            if inside && lv.edge(next) != Edge::Open {
                                let (na, nb) = next.cells();
                                if lv.kit[ci(na.0, na.1)] != lv.kit[ci(nb.0, nb.1)] {
                                    c.w.set(at(s, lpos - 1, y), edge_wall(p, l, a, yrel, s));
                                }
                            }
                        }
                    }
                }
                // Openings.
                let layers: &[i32] = if double { &[0, -1] } else { &[0] };
                let pool = ka == Kit::Pool || kb == Kit::Pool;
                match kind {
                    Edge::Gap | Edge::Door => {
                        let top = if kind == Edge::Gap { DROP } else { DOOR };
                        for s in s0..=s1 {
                            let d = (s - (s0 + 3)).abs();
                            let arch = if pool && kind == Edge::Door { [0, 0, 1, 2][d.min(3) as usize] } else { 0 };
                            for y in yf..yf + top - arch {
                                for &o in layers {
                                    c.w.set(at(s, lpos + o, y), AIR);
                                }
                            }
                        }
                    }
                    Edge::HotelDoor | Edge::HotelClosed => {
                        for s in s0..=s1 {
                            let d = (s - (s0 + 3)).abs();
                            for y in yf..yf + 7 {
                                let yrel = y - yf;
                                let m = if d <= 1 && yrel < 6 {
                                    if kind == Edge::HotelClosed {
                                        if d == 1 && yrel == 2 && s > s0 + 3 { mat::PAINT_YELLOW } else { mat::DARK_WOOD }
                                    } else {
                                        AIR
                                    }
                                } else if d == 2 && yrel < 7 || d <= 2 && yrel == 6 {
                                    mat::WOOD
                                } else {
                                    continue;
                                };
                                for &o in layers {
                                    c.w.set(at(s, lpos + o, y), m);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // Posts at interior line intersections: solid where any incident edge has a wall.
    for jj in 1..N {
        for ii in 1..N {
            let inc = [
                EdgeRef { vertical: true, i: ii, j: jj - 1 },
                EdgeRef { vertical: true, i: ii, j: jj },
                EdgeRef { vertical: false, i: ii - 1, j: jj },
                EdgeRef { vertical: false, i: ii, j: jj },
            ];
            if inc.iter().all(|&e| lv.edge(e) == Edge::Open) {
                continue;
            }
            let cell = (ii, jj);
            for y in ys.clone() {
                c.w.set(IVec3::new(line(ii), y, line(jj)), edge_wall(p, l, cell, y - yf, 0));
            }
        }
    }
    // Outer walls: inner kit layer on the line, destructible concrete skin outside, indestructible
    // frame posts on every other line.
    for k in 0..=N {
        for side in 0..4u8 {
            let (fixed, outer) = match side {
                0 => (line(0), line(0) - 1),
                1 => (line(N), line(N) + 1),
                2 => (line(0), line(0) - 1),
                _ => (line(N), line(N) + 1),
            };
            let cell = match side {
                0 => (0, k.min(N - 1)),
                1 => (N - 1, k.min(N - 1)),
                2 => (k.min(N - 1), 0),
                _ => (k.min(N - 1), N - 1),
            };
            let at = |along: i32, across: i32, y: i32| if side < 2 { IVec3::new(across, y, along) } else { IVec3::new(along, y, across) };
            let post = line(k);
            let span = if k < N { post + 1..=post + 7 } else { post + 1..=post };
            for y in ys.clone() {
                let yrel = y - yf;
                let frame = k % 2 == 0;
                c.w.set(at(post, fixed, y), if frame { mat::SHELL } else { edge_wall(p, l, cell, yrel, post) });
                c.w.set(at(post, outer, y), if frame { mat::SHELL } else { mat::GARAGE_CONCRETE });
                for s in span.clone() {
                    c.w.set(at(s, fixed, y), edge_wall(p, l, cell, yrel, s));
                    c.w.set(at(s, outer, y), mat::GARAGE_CONCRETE);
                }
            }
            // Corners of the outer skin.
            if k == 0 || k == N {
                for y in ys.clone() {
                    c.w.set(at(post - 1, outer, y), mat::SHELL);
                    c.w.set(at(post + 1, outer, y), mat::SHELL);
                }
            }
        }
    }
    // Windows: a 3 × 3 m pane in the skin, the inner layer cut back.
    for w in p.windows.iter().filter(|w| w.level == l) {
        let (fixed, outer) = match w.side {
            0 | 2 => (line(0), line(0) - 1),
            _ => (line(N), line(N) + 1),
        };
        let at = |along: i32, across: i32, y: i32| if w.side < 2 { IVec3::new(across, y, along) } else { IVec3::new(along, y, across) };
        for s in line(w.k) + 2..=line(w.k) + 6 {
            for y in yf + 2..yf + 8 {
                c.w.set(at(s, fixed, y), AIR);
                c.w.set(at(s, outer, y), mat::GLASS_TEAL);
            }
        }
    }
    // Interior columns (indestructible), except inside the sky room, stairwells and pits.
    for &ii in &COLUMN_LINES {
        for &jj in &COLUMN_LINES {
            let cells = [(ii - 1, jj - 1), (ii, jj - 1), (ii - 1, jj), (ii, jj)];
            let rooms: Vec<Option<&Room>> = cells.iter().map(|&(i, j)| p.room_of(l, i, j)).collect();
            // Never inside a corridor (the bhop lanes stay clear) or the halls / stairwells / pits.
            let same = rooms.iter().all(|r| r.map(|r| r as *const Room) == rooms[0].map(|r| r as *const Room));
            let blocked = rooms.iter().any(|r| r.is_some_and(|r| matches!(r.kit, Kit::Sky | Kit::Stair | Kit::Shaft)))
                || (same && rooms[0].is_some_and(|r| r.role == Role::Corridor))
                || cells.iter().any(|&(i, j)| p.is_pit(l, i, j));
            if blocked {
                continue;
            }
            for y in ys.clone() {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        c.w.set(IVec3::new(line(ii) + dx, y, line(jj) + dz), mat::SHELL);
                    }
                }
            }
            c.anchor(IVec3::new(line(ii), yf + 4, line(jj)));
        }
    }
    // Frame posts are anchors too.
    for k in (0..=N).step_by(4) {
        c.anchor(IVec3::new(line(k), yf + 4, line(0)));
        c.anchor(IVec3::new(line(0), yf + 4, line(k)));
        c.anchor(IVec3::new(line(k), yf + 4, line(N)));
        c.anchor(IVec3::new(line(N), yf + 4, line(k)));
    }
    // Drop ceilings (offices, hotel corridors / open rooms) with light panels; sealed plenum above.
    for room in p.rooms.iter().filter(|r| r.level == l && r.has_drop()) {
        let (x0, x1, z0, z1) = room.rect.vox();
        let y = yf + DROP;
        for z in x_range(z0, z1) {
            for x in x_range(x0, x1) {
                let v = IVec3::new(x, y, z);
                if c.w.get(v) != AIR {
                    continue;
                }
                let (i, j) = ((x - GRID0).div_euclid(CELL), (z - GRID0).div_euclid(CELL));
                let (lx, lz) = (x - line(i), z - line(j));
                let m = match room.kit {
                    Kit::Hotel => {
                        if (3..=4).contains(&lx) && (3..=4).contains(&lz) && (i + j) % 2 == 0 {
                            mat::NEON_AMBER
                        } else {
                            mat::HOTEL_WALL
                        }
                    }
                    _ => {
                        let dead = unit(hash3(i, l as i32, j, seed ^ 0xdead)) < 0.1;
                        if (3..=4).contains(&lx) && (2..=5).contains(&lz) {
                            if dead { mat::FLUORESCENT_DEAD } else { mat::FLUORESCENT }
                        } else {
                            mat::CEILING_TILE
                        }
                    }
                };
                c.w.set(v, m);
            }
        }
        let mid = room.rect.center();
        c.interior(
            Vec3::new(mid.x, (y + 2) as f32 * 0.5 + 0.25, mid.y),
            Vec3::new(x0 as f32 * 0.5 - 0.6, y as f32 * 0.5, z0 as f32 * 0.5 - 0.6),
            Vec3::new(x1 as f32 * 0.5 + 1.1, (yf + CLEAR) as f32 * 0.5 + 0.1, z1 as f32 * 0.5 + 1.1),
        );
    }
    // Closed hotel rooms are sealed pockets.
    for room in p.rooms.iter().filter(|r| r.level == l && r.closed) {
        let (x0, x1, z0, z1) = room.rect.vox();
        let mid = room.rect.center();
        c.interior(
            Vec3::new(mid.x, (yf + 3) as f32 * 0.5, mid.y),
            Vec3::new(x0 as f32 * 0.5 - 0.6, yf as f32 * 0.5 - 0.6, z0 as f32 * 0.5 - 0.6),
            Vec3::new(x1 as f32 * 0.5 + 1.1, (yf + CLEAR) as f32 * 0.5 + 0.6, z1 as f32 * 0.5 + 1.1),
        );
    }
}

fn x_range(a: i32, b: i32) -> std::ops::RangeInclusive<i32> {
    a..=b
}

/// Roof: parapet + a few rooftop boxes.
fn build_roof(c: &mut City, seed: u32) {
    let y = floor_v(LEVELS);
    let (lo, hi) = (line(0) - 1, line(N) + 1);
    for k in lo..=hi {
        for yy in y..y + 2 {
            for (x, z) in [(k, lo), (k, line(0)), (k, hi), (k, line(N)), (lo, k), (line(0), k), (hi, k), (line(N), k)] {
                c.w.set(IVec3::new(x, yy, z), mat::SHELL);
            }
        }
    }
    let mut rng = Rng::new(seed as u64 ^ 0x0000_7200_f000);
    for _ in 0..6 {
        let (x, z) = (rng.range(-30.0, 30.0), rng.range(-30.0, 30.0));
        let (w, d) = (rng.range(2.0, 5.0), rng.range(2.0, 4.0));
        let h = rng.range(1.0, 2.5);
        let base = y as f32 * 0.5;
        c.fill(Vec3::new(x - w, base, z - d), Vec3::new(x + w, base + h, z + d), mat::STEEL);
        c.fill(Vec3::new(x - w * 0.5, base + h, z - d * 0.5), Vec3::new(x + w * 0.5, base + h + 0.5, z + d * 0.5), mat::MULLION);
    }
}

// ------------------------------------------------------------------------------------------------
// Kit features

fn build_features(c: &mut City, p: &Plan, seed: u32) {
    let mut rng = Rng::new(seed as u64 ^ 0x0fea_7000);
    // Stairs.
    for s in &p.stairs {
        let yf = floor_v(s.level);
        let (x0, x1, z0, z1) = s.rect.vox();
        let len = 8 * s.len_cells() - 1;
        let wide = s.rect.w().min(s.rect.h()) == 2;
        for z in z0..=z1 {
            for x in x0..=x1 {
                let t = match (s.along_x, s.up > 0) {
                    (true, true) => x - x0,
                    (true, false) => x1 - x,
                    (false, true) => z - z0,
                    (false, false) => z1 - z,
                };
                let h = ((t + 1) * STOREY) / len;
                if h <= 0 {
                    continue;
                }
                let hn = ((t + 2) * STOREY) / len;
                let across = if s.along_x { z - z0 } else { x - x0 };
                let width = if s.along_x { z1 - z0 } else { x1 - x0 };
                for y in yf..yf + h {
                    let top = y == yf + h - 1;
                    // Yellow edge lines on the ramp, yellow nosing on stair steps.
                    let m = if top && ((wide && (across <= 1 || across >= width - 1)) || (!wide && hn > h)) {
                        mat::PAINT_YELLOW
                    } else {
                        mat::GARAGE_CONCRETE
                    };
                    c.w.set(IVec3::new(x, y, z), m);
                }
            }
        }
    }
    for room in &p.rooms {
        let yf = floor_v(room.level);
        let (x0, x1, z0, z1) = room.rect.vox();
        match (room.kit, room.role) {
            (Kit::Pool, Role::Normal) => {
                // Raised deck; pools are the slab-top voxels turned to water, 0.5 m down.
                let (pw, ph) = (x1 - x0 + 1, z1 - z0 + 1);
                let inset = |n: i32, rng: &mut Rng| rng.int(3, (n / 4).max(4));
                let (ix0, ix1) = (inset(pw, &mut rng), inset(pw, &mut rng));
                let (iz0, iz1) = (inset(ph, &mut rng), inset(ph, &mut rng));
                let pool = (x0 + ix0, x1 - ix1, z0 + iz0, z1 - iz1);
                let has_pool = pool.1 - pool.0 >= 3 && pool.3 - pool.2 >= 3 && rng.chance(0.85);
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        let in_pool = has_pool && x >= pool.0 && x <= pool.1 && z >= pool.2 && z <= pool.3;
                        if in_pool {
                            c.w.set(IVec3::new(x, yf - 1, z), mat::POOL_WATER);
                        } else if c.w.get(IVec3::new(x, yf, z)) == AIR {
                            let edge = has_pool && (x >= pool.0 - 1 && x <= pool.1 + 1 && z >= pool.2 - 1 && z <= pool.3 + 1);
                            c.w.set(
                                IVec3::new(x, yf, z),
                                if edge { mat::POOL_GROUT } else { floor_mat(Kit::Pool, IVec3::new(x, yf, z), seed) },
                            );
                        }
                    }
                }
                // Tiled pillars at the room's inner grid points.
                for jj in room.rect.z0 + 1..room.rect.z1 {
                    for ii in room.rect.x0 + 1..room.rect.x1 {
                        if (ii + jj) % 2 != 0 {
                            continue;
                        }
                        let (px, pz) = (line(ii), line(jj));
                        let in_pool = has_pool && px >= pool.0 - 1 && px <= pool.1 + 1 && pz >= pool.2 - 1 && pz <= pool.3 + 1;
                        if in_pool && rng.chance(0.5) {
                            continue;
                        }
                        for y in yf..yf + CLEAR {
                            for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                                c.w.set(IVec3::new(px + dx, y, pz + dz), if (y - yf) % 4 == 0 { mat::POOL_GROUT } else { mat::POOL_TILE });
                            }
                        }
                    }
                }
            }
            (Kit::Garage, _) => {
                // Parking grid on the floor + concrete pillars every other line.
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        let v = IVec3::new(x, yf - 1, z);
                        let m = c.w.get(v);
                        if m != mat::GARAGE_CONCRETE {
                            continue;
                        }
                        let (lx, lz) = ((x - GRID0).rem_euclid(CELL), (z - GRID0).rem_euclid(CELL));
                        if lx == 0 && (2..=6).contains(&lz) {
                            c.w.set(v, mat::PAINT_WHITE);
                        } else if lz == 0 && (x - GRID0).div_euclid(CELL) % 3 == 1 && lx % 3 != 0 {
                            c.w.set(v, mat::PAINT_YELLOW);
                        }
                    }
                }
                for jj in room.rect.z0 + 1..room.rect.z1 {
                    for ii in room.rect.x0 + 1..room.rect.x1 {
                        if ii % 2 != 0 || jj % 2 != 0 || p.pits.iter().any(|r| r.x0 < ii && ii < r.x1 && r.z0 < jj && jj < r.z1) {
                            continue;
                        }
                        if COLUMN_LINES.contains(&ii) && COLUMN_LINES.contains(&jj) {
                            continue;
                        }
                        for y in yf..yf + CLEAR {
                            let yrel = y - yf;
                            for dz in 0..2 {
                                for dx in 0..2 {
                                    let m = if yrel <= 2 {
                                        if (yrel + dx + dz) % 2 == 0 { mat::PAINT_YELLOW } else { mat::ROOF_DARK }
                                    } else {
                                        mat::GARAGE_CONCRETE
                                    };
                                    c.w.set(IVec3::new(line(ii) + dx, y, line(jj) + dz), m);
                                }
                            }
                        }
                    }
                }
            }
            (Kit::Sky, Role::Normal) => {
                // Clouds hung from the painted sky (tether anchors), a sun, a pond.
                let top = floor_v(room.level + 2) - 3; // underside of level 4's slab
                let (cx, cz) = ((x0 + x1) / 2, (z0 + z1) / 2);
                for k in 0..7 {
                    let a = k as f32 * 0.9 + rng.range(0.0, 0.5);
                    let r = rng.range(5.0, 18.0);
                    let px = cx as f32 * 0.5 + r * crate::noise::dcos(a);
                    let pz = cz as f32 * 0.5 + r * crate::noise::dsin(a);
                    let rx = rng.range(2.5, 4.5);
                    let rz = rng.range(1.8, 3.2);
                    let ry = rng.range(0.9, 1.5);
                    let yc = top as f32 * 0.5 + 0.5 - ry * 0.4;
                    for (ox, oz, s) in [(0.0, 0.0, 1.0), (rx * 0.6, 0.3, 0.7), (-rx * 0.6, -0.2, 0.75)] {
                        c.w.fill_ellipsoid(Vec3::new(px + ox, yc, pz + oz), Vec3::new(rx * s, ry * s, rz * s), mat::CLOUD, true);
                    }
                }
                // The sun: a glowing disc on the ceiling near a corner.
                let (sx, sz) = (x0 as f32 * 0.5 + 6.0, z1 as f32 * 0.5 - 6.0);
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        let d = Vec2::new(x as f32 * 0.5 + 0.25 - sx, z as f32 * 0.5 + 0.25 - sz).length();
                        if d < 2.2 {
                            c.w.set(IVec3::new(x, top + 1, z), mat::NEON_AMBER);
                        }
                    }
                }
                // Pond (off the swing line).
                let (px, pz, pr) = (pond_spot().x, pond_spot().y, 3.6);
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        let d = Vec2::new(x as f32 * 0.5 + 0.25 - px, (z as f32 * 0.5 + 0.25 - pz) * 1.3).length();
                        if d < pr {
                            c.w.set(IVec3::new(x, yf - 1, z), mat::POOL_WATER);
                        } else if d < pr + 0.6 {
                            c.w.set(IVec3::new(x, yf - 1, z), mat::SAND);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Spawns, routes, info

fn pick_spawns(w: &VoxelWorld, p: &Plan, rng: &mut Rng) -> Vec<Vec3> {
    let mut spawns: Vec<Vec3> = Vec::new();
    for l in 0..LEVELS {
        let mut cands: Vec<Vec3> = Vec::new();
        for j in 0..N {
            for i in 0..N {
                let Some(r) = p.room_of(l, i, j) else { continue };
                if !matches!(r.role, Role::Normal | Role::Corridor) || matches!(r.kit, Kit::Stair | Kit::Shaft) {
                    continue;
                }
                if p.levels[l].keep_clear[ci(i, j)] || p.is_pit(l, i, j) {
                    continue;
                }
                let c = cell_center(i, j);
                // Not under the sky room's giant duck.
                if l == 2 && c.distance(pond_spot()) < 9.0 {
                    continue;
                }
                let from = floor_y(l) + 3.0;
                for (dx, dz) in [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    if let Some(s) = crate::arena::spawn_ok(w, c.x + dx, c.y + dz, from)
                        && s.y >= floor_y(l) - 0.1
                        && s.y <= floor_y(l) + 1.1
                    {
                        cands.push(s);
                        break;
                    }
                }
            }
        }
        if cands.is_empty() {
            continue;
        }
        // Two per level: a random one, then the farthest from everything chosen so far.
        let first = cands[rng.int(0, cands.len() as i32) as usize];
        let pick_far = |taken: &[Vec3], cands: &[Vec3]| -> Option<Vec3> {
            cands.iter().copied().filter(|c| taken.iter().all(|t| t.distance(*c) >= 12.0)).max_by(|a, b| {
                let da = taken.iter().map(|t| t.distance(*a)).fold(f32::INFINITY, f32::min);
                let db = taken.iter().map(|t| t.distance(*b)).fold(f32::INFINITY, f32::min);
                da.total_cmp(&db)
            })
        };
        if spawns.iter().all(|t| t.distance(first) >= 12.0) {
            spawns.push(first);
        } else if let Some(s) = pick_far(&spawns, &cands) {
            spawns.push(s);
        }
        if let Some(s) = pick_far(&spawns, &cands) {
            spawns.push(s);
        }
    }
    spawns
}

/// Build the Backrooms. Deterministic for a given seed.
pub fn generate_backrooms(seed: u64) -> (VoxelWorld, MapInfo) {
    let mut rng = Rng::new(seed ^ 0x6261_636b_726f_6f6d);
    let plan = make_plan(&mut rng);
    let mut c = City::new(VoxelWorld::centered(BACKROOMS_SIZE_VOXELS), seed ^ 0x6c69_6d69_6e61);
    let s32 = (seed as u32) ^ 0x00b4_c400;
    build_slabs(&mut c, &plan, s32);
    for l in 0..LEVELS {
        build_walls(&mut c, &plan, l, s32);
    }
    build_roof(&mut c, s32);
    build_features(&mut c, &plan, s32);
    // Anchor hints in the basement (collapse searches head for the nearest).
    for k in (0..=N).step_by(5) {
        for m in (0..=N).step_by(5) {
            c.anchor(IVec3::new(line(k), floor_v(0) - 4, line(m)));
        }
    }
    let spawns = pick_spawns(&c.w, &plan, &mut rng);
    let placed = props::place_props(&mut c, &plan_view(&plan), &spawns, &mut rng);
    let (world, pruned) = c.finish_counted();

    let wb = world.bounds();
    let sky = HALL.center();
    let center = Vec3::new(sky.x, floor_y(2), sky.y);
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb {
            min: Vec3::new(wb.min.x - 24.0, VOID_Y, wb.min.z - 24.0),
            max: Vec3::new(wb.max.x + 24.0, wb.max.y + 40.0, wb.max.z + 24.0),
        },
        void_y: VOID_Y,
        center,
        // Straight down through any hole punched in the roof: shafts of daylight.
        sun_dir: Vec3::new(0.22, 0.94, 0.26).normalize(),
        sky_zenith: [0.26, 0.25, 0.19],
        sky_horizon: [0.50, 0.47, 0.33],
        fog_color: [0.55, 0.51, 0.36],
        ambient: 0.62,
        gravity: None,
        surf_lines: Vec::new(),
        water_sheets: Vec::new(),
        water_walk: false,
        interactables: Default::default(),
        slipstream: None,
        mechanics: Default::default(),
    };
    let mut districts = Vec::new();
    let names = ["garage", "poolrooms", "sky_room", "hotel", "office"];
    for (l, name) in names.iter().enumerate() {
        let c = if l == 2 { sky } else { Vec2::ZERO };
        districts.push(District { name, center: Vec3::new(c.x, floor_y(l), c.y), area: [-38.0, -38.0, 38.0, 38.0] });
    }
    let routes = routes(&plan);
    let info = MapInfo {
        name: "Backrooms",
        arena,
        // No sea: nothing is water except the pools (voxels).
        water_y: -1000.0,
        water_color: [0.28, 0.82, 0.92],
        water_deep: [0.10, 0.36, 0.50],
        sun_color: [1.0, 0.95, 0.80],
        sky_mid: [0.40, 0.38, 0.27],
        fog_start: 12.0,
        fog_end: 70.0,
        neon_boost: 0.9,
        districts,
        routes,
        pruned_voxels: pruned,
    };
    let _ = placed;
    (world, info)
}

fn routes(p: &Plan) -> Vec<Route> {
    let mut out = Vec::new();
    if let Some(&(l, r)) = p.spines.iter().find(|(l, _)| *l == 3) {
        let (x0, x1, z0, z1) = r.vox();
        let zc = (z0 + z1 + 1) as f32 * 0.25;
        out.push(Route {
            name: "hotel_bhop",
            from: "hotel",
            to: "hotel",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(x0 as f32 * 0.5 + 1.5, floor_y(l), zc), Vec3::new((x1 + 1) as f32 * 0.5 - 1.5, floor_y(l), zc)],
            anchors: vec![],
        });
    }
    let (x0, x1, z0, z1) = HALL.vox();
    let zc = (z0 + z1 + 1) as f32 * 0.25;
    let top = (floor_v(4) - 2) as f32 * 0.5;
    out.push(Route {
        name: "sky_swing",
        from: "sky_room",
        to: "sky_room",
        kind: RouteKind::Swing,
        waypoints: vec![Vec3::new(x0 as f32 * 0.5 + 1.5, floor_y(2), zc), Vec3::new((x1 + 1) as f32 * 0.5 - 2.0, floor_y(2), zc)],
        anchors: vec![Vec3::new((x0 + x1 + 1) as f32 * 0.25 - 2.0, top, zc)],
    });
    if let Some(&(l, r)) = p.spines.iter().find(|(l, _)| *l == 1) {
        let (x0, x1, z0, z1) = r.vox();
        let zc = (z0 + z1 + 1) as f32 * 0.25;
        out.push(Route {
            name: "office_corridor",
            from: "poolrooms",
            to: "poolrooms",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(x0 as f32 * 0.5 + 1.5, floor_y(l), zc), Vec3::new((x1 + 1) as f32 * 0.5 - 1.5, floor_y(l), zc)],
            anchors: vec![],
        });
    }
    out
}

// ------------------------------------------------------------------------------------------------
// Introspection for props / tests / previews

/// A read-only view of the plan for the prop placer and the tests.
pub struct PlanView {
    /// Per level: per cell (index i + N·j) room info.
    pub cells: Vec<Vec<Option<CellInfo>>>,
    /// Walls: per level, `(cell, dir)` → is there a solid wall on that side (no opening)?
    walls: Vec<Vec<[bool; 4]>>,
    pub stairs: Vec<(usize, CellRect)>,
    pub shafts: Vec<(i32, i32)>,
    pub pits: Vec<CellRect>,
    pub windows: Vec<(usize, u8, i32)>,
    pub rooms: Vec<RoomInfo>,
}

#[derive(Clone, Copy, Debug)]
pub struct CellInfo {
    pub room: u32,
    pub kit: Kit,
    pub walkable: bool,
    pub keep_clear: bool,
    pub no_props: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct RoomInfo {
    pub level: usize,
    pub kit: Kit,
    pub rect: CellRect,
    pub walkable: bool,
    pub closed: bool,
    pub corridor: bool,
    pub hotel_room: bool,
}

impl PlanView {
    pub fn cell(&self, l: usize, i: i32, j: i32) -> Option<CellInfo> {
        if !in_grid((i, j)) {
            return None;
        }
        self.cells[l][ci(i, j)]
    }
    /// Solid wall (no opening, not a window) on side `dir` (0 −x, 1 +x, 2 −z, 3 +z) of cell (i, j).
    pub fn solid_wall(&self, l: usize, i: i32, j: i32, dir: usize) -> bool {
        in_grid((i, j)) && self.walls[l][ci(i, j)][dir]
    }
}

fn plan_view(p: &Plan) -> PlanView {
    let mut cells = Vec::new();
    let mut walls = Vec::new();
    for l in 0..LEVELS {
        let lv = &p.levels[l];
        let mut cl = vec![None; (N * N) as usize];
        let mut wl = vec![[false; 4]; (N * N) as usize];
        for j in 0..N {
            for i in 0..N {
                let Some(r) = lv.room_at(i, j) else { continue };
                let room = &p.rooms[r as usize];
                cl[ci(i, j)] = Some(CellInfo {
                    room: r,
                    kit: room.kit,
                    walkable: room.walkable() && !p.is_pit(l, i, j),
                    keep_clear: lv.keep_clear[ci(i, j)],
                    no_props: lv.no_props[ci(i, j)],
                });
                for (d, (di, dj)) in [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().enumerate() {
                    let n = (i + di, j + dj);
                    let solid = if in_grid(n) {
                        lv.edge(EdgeRef::between((i, j), n)) == Edge::Wall
                    } else {
                        // Facade: solid unless a window is there.
                        let side = match d {
                            0 => 0u8,
                            1 => 1,
                            2 => 2,
                            _ => 3,
                        };
                        let k = if d < 2 { j } else { i };
                        !p.windows.iter().any(|w| w.level == l && w.side == side && w.k == k)
                    };
                    wl[ci(i, j)][d] = solid;
                }
            }
        }
        cells.push(cl);
        walls.push(wl);
    }
    PlanView {
        cells,
        walls,
        stairs: p.stairs.iter().map(|s| (s.level, s.rect)).collect(),
        shafts: p.shafts.clone(),
        pits: p.pits.clone(),
        windows: p.windows.iter().map(|w| (w.level, w.side, w.k)).collect(),
        rooms: p
            .rooms
            .iter()
            .map(|r| RoomInfo {
                level: r.level,
                kit: r.kit,
                rect: r.rect,
                walkable: r.walkable(),
                closed: r.closed,
                corridor: r.role == Role::Corridor,
                hotel_room: r.role == Role::HotelRoom,
            })
            .collect(),
    }
}

/// The plan behind a seed (for tests and previews): rooms, stairs, shafts, pits, windows.
pub fn plan_for(seed: u64) -> PlanView {
    let mut rng = Rng::new(seed ^ 0x6261_636b_726f_6f6d);
    plan_view(&make_plan(&mut rng))
}
