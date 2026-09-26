//! Backrooms decor: "weird stuff everywhere", seeded and capped.
//!
//! Props are static terrain voxels (so they collide, break and anchor tethers like everything else)
//! built from shared models — each model is a fixed list of voxels written once per placement:
//! - **Icon models** ([`super::icons`]): baked offline from the construct icon/shape library
//!   (`cargo run -p bd_shapes --example backrooms_bake`): the lexicon resolves a phrase (rubber duck,
//!   traffic cone, vending machine, shopping cart, teddy bear, grand piano …), `bd_shapes` builds
//!   and voxelises it at 0.5 m, and each tint (Primary / Accent / Dark / Core) maps to a terrain
//!   material per prop. Scaled up a little on purpose (liminal oversize).
//! - **Kit furniture** (below): office chairs, desks, water coolers, filing cabinets, arcade
//!   cabinets, beds, pool loungers, floats, lifeguard chairs, playground structures, parked cars.
//!
//! Rules: props hug a *solid* wall of a walkable room (never a doorway, window or stair/shaft
//! mouth), one per cell, never within [`SPAWN_CLEAR`] of a spawn or on an authored route; at most
//! [`MAX_PER_ROOM`] per room (kit-dependent) and [`super::MAX_PROPS`] in total. Big set pieces (sky-room
//! duck, playground structures, garage cars) have their own slots with the same exclusions.

use bd_core::voxel::{AIR, Material};
use glam::{IVec3, Vec3};

use super::{CELL, CellRect, GRID0, Kit, LEVELS, MAX_PROPS, N, PlanView, floor_v, line};
use crate::city::{CarSpec, City};
use crate::material::mat;
use crate::noise::Rng;

/// Props keep this far (m, horizontal) from every spawn.
pub const SPAWN_CLEAR: f32 = 3.5;
/// Per-room cap for wall props.
pub const MAX_PER_ROOM: usize = 3;

/// A prop model in local voxels: `u` along the wall, `v` up, `w` out from the wall (w = 0 touches
/// it). Built from boxes.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub name: &'static str,
    /// Footprint along the wall, height, depth (voxels).
    pub size: IVec3,
    pub voxels: Vec<(IVec3, Material)>,
}

impl Model {
    fn new(name: &'static str) -> Self {
        Self { name, ..Default::default() }
    }
    /// Fill box `[a, b]` inclusive (u, v, w).
    fn b(mut self, a: (i32, i32, i32), b: (i32, i32, i32), m: Material) -> Self {
        for w in a.2..=b.2 {
            for v in a.1..=b.1 {
                for u in a.0..=b.0 {
                    self.voxels.retain(|(p, _)| *p != IVec3::new(u, v, w));
                    self.voxels.push((IVec3::new(u, v, w), m));
                }
            }
        }
        self.fit()
    }
    fn fit(mut self) -> Self {
        let mut hi = IVec3::ZERO;
        for (p, _) in &self.voxels {
            hi = hi.max(*p + IVec3::ONE);
        }
        self.size = hi;
        self
    }
}

// ---- kit furniture ------------------------------------------------------------------------------

fn office_chair() -> Model {
    Model::new("office chair")
        .b((0, 0, 1), (2, 0, 1), mat::PLASTIC_BLACK)
        .b((1, 0, 0), (1, 0, 2), mat::PLASTIC_BLACK)
        .b((1, 1, 1), (1, 1, 1), mat::STEEL)
        .b((0, 2, 0), (2, 2, 2), mat::PLASTIC_BLACK)
        .b((0, 3, 0), (2, 5, 0), mat::PLASTIC_BLACK)
}

fn desk() -> Model {
    Model::new("desk")
        .b((0, 0, 0), (0, 1, 0), mat::STEEL)
        .b((4, 0, 0), (4, 1, 0), mat::STEEL)
        .b((0, 0, 2), (0, 1, 2), mat::STEEL)
        .b((4, 0, 2), (4, 1, 2), mat::STEEL)
        .b((0, 2, 0), (4, 2, 2), mat::WOOD)
        .b((1, 3, 0), (2, 4, 0), mat::PLASTIC_BLACK)
        .b((1, 4, 0), (2, 4, 0), mat::NEON_CYAN)
}

fn water_cooler() -> Model {
    Model::new("water cooler").b((0, 0, 0), (1, 2, 1), mat::HULL_WHITE).b((0, 3, 0), (1, 4, 1), mat::GLASS_BLUE)
}

fn filing_cabinet() -> Model {
    Model::new("filing cabinet")
        .b((0, 0, 0), (1, 4, 1), mat::STEEL)
        .b((0, 1, 1), (1, 1, 1), mat::MULLION)
        .b((0, 3, 1), (1, 3, 1), mat::MULLION)
}

fn potted_plant() -> Model {
    Model::new("potted plant").b((0, 0, 0), (1, 1, 1), mat::TERRACOTTA).b((0, 2, 0), (1, 4, 1), mat::LEAVES_GREEN).b((0, 5, 0), (0, 5, 0), mat::LEAVES_GREEN)
}

fn crt_tv() -> Model {
    Model::new("crt tv")
        .b((0, 0, 0), (2, 1, 1), mat::DARK_WOOD)
        .b((0, 2, 0), (2, 4, 2), mat::PLASTIC_BLACK)
        .b((0, 2, 2), (2, 4, 2), mat::NEON_CYAN)
        .b((1, 5, 1), (1, 5, 1), mat::STEEL)
}

fn arcade() -> Model {
    let screen = mat::NEON_PINK;
    Model::new("arcade cabinet")
        .b((0, 0, 0), (2, 6, 1), mat::PLASTIC_BLACK)
        .b((0, 2, 2), (2, 2, 3), mat::PLASTIC_BLACK)
        .b((0, 3, 2), (2, 3, 2), mat::PAINT_RED)
        .b((0, 4, 1), (2, 5, 1), screen)
        .b((0, 6, 1), (2, 6, 1), mat::NEON_AMBER)
}

fn bed() -> Model {
    Model::new("bed")
        .b((0, 0, 0), (4, 0, 7), mat::DARK_WOOD)
        .b((0, 1, 0), (4, 1, 7), mat::HULL_WHITE)
        .b((0, 1, 3), (4, 1, 7), mat::HOTEL_CARPET)
        .b((1, 2, 1), (3, 2, 1), mat::HULL_WHITE)
        .b((0, 1, 0), (4, 3, 0), mat::DARK_WOOD)
}

fn lamp() -> Model {
    Model::new("lamp").b((0, 0, 0), (1, 1, 1), mat::DARK_WOOD).b((0, 2, 0), (1, 2, 1), mat::DARK_WOOD).b((0, 3, 0), (1, 4, 1), mat::NEON_AMBER)
}

fn lounger() -> Model {
    Model::new("pool lounger").b((0, 0, 1), (2, 0, 6), mat::HULL_WHITE).b((0, 1, 1), (2, 1, 6), mat::STUCCO_SKY).b((0, 2, 5), (2, 3, 6), mat::STUCCO_SKY)
}

fn lifeguard_chair() -> Model {
    Model::new("lifeguard chair")
        .b((0, 0, 1), (0, 4, 1), mat::WOOD)
        .b((2, 0, 1), (2, 4, 1), mat::WOOD)
        .b((0, 0, 3), (0, 4, 3), mat::WOOD)
        .b((2, 0, 3), (2, 4, 3), mat::WOOD)
        .b((0, 5, 1), (2, 5, 3), mat::WOOD)
        .b((0, 6, 1), (2, 7, 1), mat::PAINT_RED)
}

fn cone() -> Model {
    Model::new("traffic cone (kit)").b((0, 0, 0), (1, 0, 1), mat::HAZARD_ORANGE).b((0, 1, 0), (0, 2, 0), mat::HAZARD_ORANGE).b((0, 2, 0), (0, 2, 0), mat::PAINT_WHITE)
}

fn luggage_cart() -> Model {
    Model::new("luggage cart")
        .b((0, 0, 0), (3, 0, 1), mat::CRYSTAL_GOLD)
        .b((0, 1, 0), (0, 5, 0), mat::CRYSTAL_GOLD)
        .b((3, 1, 0), (3, 5, 0), mat::CRYSTAL_GOLD)
        .b((0, 5, 0), (3, 5, 0), mat::CRYSTAL_GOLD)
        .b((1, 1, 0), (2, 2, 1), mat::PAINT_RED)
        .b((1, 3, 0), (2, 3, 1), mat::HULL_NAVY)
}

fn door_frame() -> Model {
    // A door to nowhere: frame + a door left ajar.
    Model::new("door to nowhere")
        .b((0, 0, 0), (0, 8, 0), mat::DARK_WOOD)
        .b((4, 0, 0), (4, 8, 0), mat::DARK_WOOD)
        .b((0, 8, 0), (4, 8, 0), mat::DARK_WOOD)
        .b((1, 0, 1), (1, 7, 3), mat::WOOD)
        .b((1, 3, 3), (1, 3, 3), mat::PAINT_YELLOW)
}

// ---- icon models (baked from the shape library) -------------------------------------------------

/// Terrain materials for an icon's four tints (Primary, Accent, Dark, Core).
fn icon_model(name: &str, palette: [Material; 4]) -> Option<Model> {
    let icon = super::icons::ICONS.iter().find(|i| i.name == name)?;
    let mut m = Model { name: icon.name, ..Default::default() };
    for &(x, y, z, t) in icon.voxels {
        m.voxels.push((IVec3::new(x as i32, y as i32, z as i32), palette[(t as usize).min(3)]));
    }
    Some(m.fit())
}

fn icon_or(name: &str, palette: [Material; 4], fallback: fn() -> Model) -> Model {
    icon_model(name, palette).unwrap_or_else(fallback)
}

/// Wall-prop palette per kit.
fn kit_models(kit: Kit) -> Vec<Model> {
    let vending = || icon_or("vending machine", [mat::PAINT_RED, mat::NEON_CYAN, mat::PLASTIC_BLACK, mat::NEON_AMBER], vending_kit);
    let cone_i = || icon_or("traffic cone", [mat::HAZARD_ORANGE, mat::PAINT_WHITE, mat::PLASTIC_BLACK, mat::PAINT_WHITE], cone);
    let cart = || icon_or("shopping cart", [mat::STEEL, mat::PAINT_RED, mat::PLASTIC_BLACK, mat::MULLION], luggage_cart);
    match kit {
        Kit::Office => vec![office_chair(), desk(), water_cooler(), filing_cabinet(), potted_plant(), vending(), crt_tv(), cone_i()],
        Kit::Pool => vec![lounger(), lounger(), lifeguard_chair(), potted_plant(), vending()],
        Kit::Garage => vec![cone_i(), cone_i(), cart(), vending()],
        Kit::Hotel => vec![bed(), lamp(), crt_tv(), luggage_cart(), potted_plant()],
        Kit::Playground => vec![arcade(), arcade(), icon_or("teddy bear", [mat::TERRACOTTA, mat::STUCCO_PEACH, mat::DARK_WOOD, mat::PLASTIC_BLACK], lamp), vending()],
        Kit::Sky => vec![office_chair(), door_frame()],
        _ => vec![],
    }
}

fn vending_kit() -> Model {
    Model::new("vending machine (kit)")
        .b((0, 0, 0), (2, 5, 1), mat::PAINT_RED)
        .b((0, 2, 2), (1, 5, 2), mat::NEON_CYAN)
        .b((2, 2, 2), (2, 3, 2), mat::PLASTIC_BLACK)
        .b((0, 0, 2), (2, 1, 2), mat::PLASTIC_BLACK)
}

// ---- placement ------------------------------------------------------------------------------------

/// Where a model goes: the cell, the wall side (0 −x, 1 +x, 2 −z, 3 +z), base floor voxel y.
struct Slot {
    level: usize,
    cell: (i32, i32),
    dir: usize,
}

/// World voxel for local (u, v, w) of a model placed against wall `dir` of a cell whose inner face
/// starts at `inner` (the first air voxel next to the wall, on the across axis), centred along the
/// cell at `start` (first voxel along).
fn to_world(dir: usize, inner: i32, start: i32, span: i32, size: IVec3, y0: i32, p: IVec3) -> IVec3 {
    let u = start + (span - size.x) / 2 + p.x;
    let ur = start + (span - size.x) / 2 + (size.x - 1 - p.x);
    match dir {
        0 => IVec3::new(inner + p.z, y0 + p.y, u),
        1 => IVec3::new(inner - p.z, y0 + p.y, ur),
        2 => IVec3::new(ur, y0 + p.y, inner + p.z),
        _ => IVec3::new(u, y0 + p.y, inner - p.z),
    }
}

/// Try to place `m` against the wall of `s`. Returns false (writing nothing) if it doesn't fit.
fn try_place(c: &mut City, s: &Slot, m: &Model, spawns: &[Vec3]) -> bool {
    let (i, j) = s.cell;
    let yf = floor_v(s.level);
    // Across-axis scan from the wall line inward to the first air voxel at knee height.
    let (along0, span) = if s.dir < 2 { (line(j) + 1, 7) } else { (line(i) + 1, 7) };
    let mid = along0 + 3;
    let (from, step) = match s.dir {
        0 => (line(i), 1),
        1 => (line(i + 1), -1),
        2 => (line(j), 1),
        _ => (line(j + 1), -1),
    };
    let at = |across: i32, y: i32| if s.dir < 2 { IVec3::new(across, y, mid) } else { IVec3::new(mid, y, across) };
    let mut inner = None;
    for k in 0..4 {
        if c.w.get(at(from + step * k, yf + 2)) == AIR {
            inner = Some(from + step * k);
            break;
        }
    }
    let Some(inner) = inner else { return false };
    // Floor: the pool deck is one voxel up.
    let mut y0 = yf;
    if c.w.get(at(inner, yf)) != AIR {
        y0 = yf + 1;
    }
    if m.size.x > span || m.size.z > 6 {
        return false;
    }
    // Everything the model occupies must be air, with solid floor under its footprint, clear of
    // spawns.
    for &(p, _) in &m.voxels {
        let v = to_world(s.dir, inner, along0, span, m.size, y0, p);
        if c.w.get(v) != AIR {
            return false;
        }
        if p.y == 0 && c.w.get(v - IVec3::Y) == AIR {
            return false;
        }
        let wp = (v.as_vec3() + Vec3::splat(0.5)) * 0.5;
        if spawns.iter().any(|sp| Vec3::new(sp.x - wp.x, 0.0, sp.z - wp.z).length() < SPAWN_CLEAR && (sp.y - wp.y).abs() < 4.0) {
            return false;
        }
    }
    // Leave head room in front (never seal a nook) — the voxel column in front of the model at
    // chest height must be air.
    let front = to_world(s.dir, inner, along0, span, m.size, y0, IVec3::new(m.size.x / 2, 2, m.size.z));
    if c.w.get(front) != AIR {
        return false;
    }
    for &(p, mm) in &m.voxels {
        let v = to_world(s.dir, inner, along0, span, m.size, y0, p);
        c.w.set(v, mm);
    }
    true
}

fn spawn_near(spawns: &[Vec3], p: Vec3, r: f32) -> bool {
    spawns.iter().any(|s| Vec3::new(s.x - p.x, 0.0, s.z - p.z).length() < r && (s.y - p.y).abs() < 4.0)
}

/// Decorate. Returns the number of props placed.
pub fn place_props(c: &mut City, v: &PlanView, spawns: &[Vec3], rng: &mut Rng) -> usize {
    let mut placed = 0usize;
    let mut used = vec![vec![false; (N * N) as usize]; LEVELS];
    let idx = |i: i32, j: i32| (i + N * j) as usize;

    // ---- set pieces ------------------------------------------------------------------------------
    // Sky room: a giant rubber duck in the pond corner, a door to nowhere, a lone chair.
    for (rid, room) in v.rooms.iter().enumerate() {
        if room.kit != Kit::Sky || !room.walkable || room.level != 2 {
            continue;
        }
        let _ = rid;
        let (x0, x1, z0, _) = vox(room.rect);
        let yf = floor_v(room.level);
        // The duck: from the icon library, scaled giant, floating in the pond.
        if let Some(duck) = icon_model("rubber duck (giant)", [mat::PAINT_YELLOW, mat::HAZARD_ORANGE, mat::PLASTIC_BLACK, mat::PLASTIC_BLACK]) {
            let (px, pz) = ((x1 + 1) as f32 * 0.5 - 7.0, z0 as f32 * 0.5 + 6.0);
            let o = IVec3::new((px / 0.5) as i32 - duck.size.x / 2, yf, (pz / 0.5) as i32 - duck.size.z / 2);
            for &(p, m) in &duck.voxels {
                let w = o + p;
                if c.w.get(w) == AIR {
                    c.w.set(w, m);
                }
            }
            placed += 1;
        }
        let _ = x0;
    }
    // Garage: parked cars in bays (city kit), cones around the pits.
    {
        let mut bays: Vec<(i32, i32)> = Vec::new();
        for j in 0..N {
            for i in 0..N {
                if let Some(cell) = v.cell(0, i, j)
                    && cell.kit == Kit::Garage
                    && cell.walkable
                    && !cell.keep_clear
                    && !cell.no_props
                    && !v.pits.iter().any(|p| CellRect::new(p.x0 - 1, p.z0 - 1, p.x1 + 1, p.z1 + 1).contains(i, j))
                {
                    bays.push((i, j));
                }
            }
        }
        shuffle(&mut bays, rng);
        let bodies = [mat::PAINT_RED, mat::HULL_NAVY, mat::HULL_WHITE, mat::STUCCO_MINT, mat::ROOF_DARK, mat::STUCCO_LEMON];
        let mut cars = 0;
        for (i, j) in bays {
            if cars >= 9 || placed >= MAX_PROPS {
                break;
            }
            let cc = super::cell_center(i, j);
            let base = Vec3::new(cc.x, super::floor_y(0), cc.y);
            if spawn_near(spawns, base, 6.0) || used[0][idx(i, j)] {
                continue;
            }
            // The car is 4.5 m long: it straddles into the next cell along its axis; keep that
            // cell free too.
            let along_x = rng.chance(0.5);
            let (ni, nj) = if along_x { (i + 1, j) } else { (i, j + 1) };
            if v.cell(0, ni, nj).is_none_or(|c| c.kit != Kit::Garage || c.keep_clear || c.no_props || !c.walkable) || used[0][idx(ni, nj)] {
                continue;
            }
            let base = base + if along_x { Vec3::new(2.0, 0.0, 0.0) } else { Vec3::new(0.0, 0.0, 2.0) };
            // Clear space check (the car's box must be air).
            let clear = (-5..=5).all(|du| {
                (-2..=2).all(|dv| {
                    (0..4).all(|dy| {
                        let off = if along_x { Vec3::new(du as f32 * 0.5, dy as f32 * 0.5, dv as f32 * 0.5) } else { Vec3::new(dv as f32 * 0.5, dy as f32 * 0.5, du as f32 * 0.5) };
                        c.w.get_at(base + off + Vec3::new(0.0, 0.25, 0.0)) == AIR
                    })
                })
            });
            if !clear {
                continue;
            }
            c.car(&CarSpec { base, along_x, body: bodies[rng.int(0, bodies.len() as i32) as usize], convertible: rng.chance(0.2) });
            used[0][idx(i, j)] = true;
            used[0][idx(ni, nj)] = true;
            cars += 1;
            placed += 1;
        }
    }
    // Playground structures: one per playground room (ball pit, slide tower, climbing frame).
    for room in v.rooms.iter() {
        if room.kit != Kit::Playground || !room.walkable || placed >= MAX_PROPS {
            continue;
        }
        let (x0, x1, z0, z1) = vox(room.rect);
        if x1 - x0 < 14 || z1 - z0 < 14 {
            continue;
        }
        let yf = floor_v(room.level);
        let (cx, cz) = ((x0 + x1) / 2 + rng.int(-2, 3), (z0 + z1) / 2 + rng.int(-2, 3));
        let centre = Vec3::new(cx as f32 * 0.5, yf as f32 * 0.5, cz as f32 * 0.5);
        if spawn_near(spawns, centre, 7.0) {
            continue;
        }
        let (ci_, cj_) = super::cell_of(centre.x, centre.z);
        if v.cell(room.level, ci_, cj_).is_some_and(|c| c.keep_clear || c.no_props) {
            continue;
        }
        match rng.int(0, 3) {
            0 => ball_pit(c, cx, cz, yf, rng),
            1 => slide(c, cx, cz, yf),
            _ => climbing_frame(c, cx, cz, yf),
        }
        placed += 1;
    }
    // Pool floats and ducks on the water.
    {
        let mut n = 0;
        for room in v.rooms.iter().filter(|r| r.kit == Kit::Pool && r.walkable) {
            if n >= 10 || placed >= MAX_PROPS {
                break;
            }
            let (x0, x1, z0, z1) = vox(room.rect);
            let yw = floor_v(room.level) - 1;
            // Find water voxels; put a ring float on one and a duck on another.
            let mut water: Vec<IVec3> = Vec::new();
            for z in (z0..=z1).step_by(3) {
                for x in (x0..=x1).step_by(3) {
                    let p = IVec3::new(x, yw, z);
                    if c.w.get(p) == mat::POOL_WATER && c.w.get(p + IVec3::Y) == AIR {
                        water.push(p);
                    }
                }
            }
            if water.is_empty() {
                continue;
            }
            let p = water[rng.int(0, water.len() as i32) as usize];
            if rng.chance(0.6) {
                // Ring float: a 3 × 3 torus of alternating stripes.
                for (k, (dx, dz)) in [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)].iter().enumerate() {
                    let q = p + IVec3::new(*dx, 1, *dz);
                    if c.w.get(q) == AIR && c.w.get(q - IVec3::Y) != AIR {
                        c.w.set(q, if k % 2 == 0 { mat::PAINT_RED } else { mat::HULL_WHITE });
                    }
                }
            } else if let Some(duck) = icon_model("rubber duck", [mat::PAINT_YELLOW, mat::HAZARD_ORANGE, mat::PLASTIC_BLACK, mat::PLASTIC_BLACK]) {
                let o = p + IVec3::new(-duck.size.x / 2, 1, -duck.size.z / 2);
                if duck.voxels.iter().all(|(q, _)| c.w.get(o + *q) == AIR) && duck.voxels.iter().filter(|(q, _)| q.y == 0).all(|(q, _)| c.w.get(o + *q - IVec3::Y) != AIR) {
                    for &(q, m) in &duck.voxels {
                        c.w.set(o + q, m);
                    }
                }
            }
            n += 1;
            placed += 1;
        }
    }

    // ---- wall props --------------------------------------------------------------------------------
    let mut rooms: Vec<usize> = (0..v.rooms.len()).collect();
    shuffle(&mut rooms, rng);
    for rid in rooms {
        if placed >= MAX_PROPS {
            break;
        }
        let room = v.rooms[rid];
        if !room.walkable || matches!(room.kit, Kit::Stair | Kit::Shaft) {
            continue;
        }
        let models = kit_models(room.kit);
        if models.is_empty() {
            continue;
        }
        let cap = match room.kit {
            Kit::Office => 2,
            Kit::Hotel if room.hotel_room => 2,
            Kit::Hotel => 1,
            Kit::Sky => 3,
            _ => MAX_PER_ROOM,
        };
        let area = (room.rect.w() * room.rect.h()) as usize;
        let cap = cap.min(1 + area / 3);
        let mut slots: Vec<Slot> = Vec::new();
        for j in room.rect.z0..room.rect.z1 {
            for i in room.rect.x0..room.rect.x1 {
                let Some(cell) = v.cell(room.level, i, j) else { continue };
                if !cell.walkable || cell.keep_clear || cell.no_props || used[room.level][idx(i, j)] {
                    continue;
                }
                for dir in 0..4 {
                    if v.solid_wall(room.level, i, j, dir) {
                        slots.push(Slot { level: room.level, cell: (i, j), dir });
                    }
                }
            }
        }
        shuffle(&mut slots, rng);
        let mut n = 0;
        for s in slots {
            if n >= cap || placed >= MAX_PROPS {
                break;
            }
            if used[s.level][idx(s.cell.0, s.cell.1)] {
                continue;
            }
            // Hotel rooms: the bed first.
            let m = if room.hotel_room && n == 0 { &models[0] } else { &models[rng.int(0, models.len() as i32) as usize] };
            if try_place(c, &s, m, spawns) {
                used[s.level][idx(s.cell.0, s.cell.1)] = true;
                n += 1;
                placed += 1;
            }
        }
    }
    placed
}

fn vox(r: CellRect) -> (i32, i32, i32, i32) {
    (line(r.x0) + 1, line(r.x1) - 1, line(r.z0) + 1, line(r.z1) - 1)
}

fn shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        let j = rng.int(0, i as i32 + 1) as usize;
        v.swap(i, j);
    }
}

// ---- playground structures -----------------------------------------------------------------------

fn ball_pit(c: &mut City, cx: i32, cz: i32, yf: i32, rng: &mut Rng) {
    let (hx, hz) = (rng.int(4, 7), rng.int(4, 7));
    let rim = [mat::PAINT_RED, mat::STUCCO_SKY, mat::PAINT_YELLOW][rng.int(0, 3) as usize];
    let balls = [mat::BALL_RED, mat::BALL_BLUE, mat::BALL_YELLOW, mat::BALL_GREEN];
    for z in cz - hz..=cz + hz {
        for x in cx - hx..=cx + hx {
            let edge = z == cz - hz || z == cz + hz || x == cx - hx || x == cx + hx;
            let h = if edge { 3 } else { 2 };
            for y in yf..yf + h {
                let v = IVec3::new(x, y, z);
                if c.w.get(v) != AIR {
                    continue;
                }
                let m = if edge {
                    rim
                } else {
                    balls[(crate::noise::hash3(x, y, z, 0xba11) % 4) as usize]
                };
                c.w.set(v, m);
            }
            // A bumpy top layer.
            if !edge && crate::noise::hash3(x, 7, z, 0xba12) % 3 == 0 {
                let v = IVec3::new(x, yf + 2, z);
                if c.w.get(v) == AIR {
                    c.w.set(v, balls[(crate::noise::hash3(x, 9, z, 0xba13) % 4) as usize]);
                }
            }
        }
    }
}

fn slide(c: &mut City, cx: i32, cz: i32, yf: i32) {
    // A 3 m tower with stairs up the back and a slide down the front (+x).
    let top = 6;
    for z in cz - 2..=cz + 2 {
        for x in cx - 2..=cx + 2 {
            let corner = (x == cx - 2 || x == cx + 2) && (z == cz - 2 || z == cz + 2);
            if corner {
                for y in yf..yf + top + 3 {
                    set_air(c, IVec3::new(x, y, z), mat::PAINT_RED);
                }
            }
            set_air(c, IVec3::new(x, yf + top - 1, z), mat::PAINT_YELLOW);
            // Roof.
            set_air(c, IVec3::new(x, yf + top + 3, z), mat::STUCCO_SKY);
        }
    }
    // Stairs (−x side), 0.5 m risers.
    for k in 0..top - 1 {
        for z in cz - 1..=cz + 1 {
            for y in yf..=yf + k {
                set_air(c, IVec3::new(cx - 3 - (top - 2 - k), y, z), mat::STUCCO_MINT);
            }
        }
    }
    // Slide (+x side): a stepped chute with rails.
    for k in 0..top {
        let x = cx + 3 + k;
        let h = top - 1 - k;
        for z in cz - 1..=cz + 1 {
            if h > 0 {
                set_air(c, IVec3::new(x, yf + h - 1, z), mat::PAINT_YELLOW);
            }
        }
        for z in [cz - 2, cz + 2] {
            for y in yf..yf + h + 1 {
                set_air(c, IVec3::new(x, y, z), if y == yf + h { mat::PAINT_RED } else { AIR_MARK });
            }
        }
    }
}

/// Sentinel: skip (leave air).
const AIR_MARK: Material = AIR;

fn climbing_frame(c: &mut City, cx: i32, cz: i32, yf: i32) {
    let (h, r) = (7, 4);
    for z in cz - r..=cz + r {
        for x in cx - r..=cx + r {
            for y in yf..yf + h {
                let (ex, ez, ey) = ((x - cx + r) % 4 == 0, (z - cz + r) % 4 == 0, (y - yf) % 3 == 2);
                let bar = (ex && ez) || (ex && ey) || (ez && ey);
                if bar {
                    let m = if (x + y + z).rem_euclid(3) == 0 { mat::STUCCO_SKY } else if (x + z).rem_euclid(2) == 0 { mat::PAINT_RED } else { mat::PAINT_YELLOW };
                    set_air(c, IVec3::new(x, y, z), m);
                }
            }
        }
    }
}

fn set_air(c: &mut City, v: IVec3, m: Material) {
    if m != AIR && c.w.get(v) == AIR {
        c.w.set(v, m);
    }
}

#[allow(dead_code)]
fn cell_of_voxel(x: i32, z: i32) -> (i32, i32) {
    ((x - GRID0).div_euclid(CELL), (z - GRID0).div_euclid(CELL))
}
