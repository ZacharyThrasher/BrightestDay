//! Turtle Lair decor: authored, capped set dressing (plus a little seeded jitter).
//!
//! Props are static terrain voxels (they collide, break and anchor tethers like everything else),
//! written only into air. Two sources, as in the Backrooms:
//! - **Icon models** ([`super::icons`]): baked offline from the construct icon library
//!   (`cargo run -p bd_shapes --example lair_bake --release`) — the pizza sign, skateboards, the four
//!   brothers' weapons on the dojo rack, Krang's android body, mutagen canisters, mousers, the party
//!   wagon in the garage, the Shellraiser on the station platform, manhole covers. Each tint
//!   (Primary / Accent / Dark / Core) maps to a terrain material per prop.
//! - **Kit furniture** (below): arcade cabinets, pinball, jukebox, CRT stacks, desks + screens,
//!   workbenches, pizza boxes, bean bags, tires, a punching bag, wooden dummies, Splinter's shrine
//!   tree, oil drums, benches, and the scrap contraption hung from the vault.
//!
//! Rules: every prop is skipped if it would come within [`SPAWN_CLEAR`] of a spawn or sit on an
//! authored ground route; at most [`MAX_PROPS`] in total. Hanging things hang from the vault on
//! 6-connected chains/ropes (so they're anchored, and fall if you shoot them down).

use bd_core::voxel::{AIR, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{FLOOR, HUB, PIT_FLOOR, PLATFORM, STREET, hub_ceiling};
use crate::city::City;
use crate::maps::{Route, RouteKind};
use crate::material::mat;
use crate::noise::Rng;

/// Props keep this far (m, horizontal) from every spawn.
pub const SPAWN_CLEAR: f32 = 3.0;
/// Global prop cap.
pub const MAX_PROPS: usize = 90;

/// A placed prop (for tests and previews): name + metre AABB.
#[derive(Clone, Debug)]
pub struct Placed {
    pub name: &'static str,
    pub min: Vec3,
    pub max: Vec3,
}

/// A prop model in local voxels: `u` across (right), `v` up, `w` forward (w = 0 is the back).
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub name: &'static str,
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
                    let p = IVec3::new(u, v, w);
                    self.voxels.retain(|(q, _)| *q != p);
                    self.voxels.push((p, m));
                }
            }
        }
        self.fit()
    }
    fn fit(mut self) -> Self {
        let lo = self.voxels.iter().fold(IVec3::splat(i32::MAX), |a, (p, _)| a.min(*p));
        if lo.x != i32::MAX {
            for (p, _) in &mut self.voxels {
                *p -= lo;
            }
        }
        self.size = self.voxels.iter().fold(IVec3::ZERO, |a, (p, _)| a.max(*p + IVec3::ONE));
        self
    }
    /// An icon from the baked library with its tints mapped to terrain materials.
    fn icon(name: &'static str, palette: [Material; 4]) -> Option<Self> {
        let icon = super::icons::ICONS.iter().find(|i| i.name == name)?;
        let mut m = Model::new(icon.name);
        for &(x, y, z, t) in icon.voxels {
            m.voxels.push((IVec3::new(x as i32, y as i32, z as i32), palette[(t as usize).min(3)]));
        }
        Some(m.fit())
    }
    /// Stand a flat / long model up: its z (forward) becomes up and its y (thickness) forward.
    fn stand_up(mut self) -> Self {
        for (p, _) in &mut self.voxels {
            *p = IVec3::new(p.x, p.z, p.y);
        }
        self.fit()
    }
    /// Turn a quarter: forward becomes right.
    fn quarter(mut self) -> Self {
        for (p, _) in &mut self.voxels {
            *p = IVec3::new(p.z, p.y, -p.x);
        }
        self.fit()
    }
}

/// Which way a model's front (+w) faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Face {
    PX,
    NX,
    PZ,
    NZ,
}

/// World voxel for model voxel `p` placed with its back-centre at voxel `o`, facing `f`.
fn to_world(o: IVec3, size: IVec3, p: IVec3, f: Face) -> IVec3 {
    let u = p.x - size.x / 2;
    let (v, w) = (p.y, p.z);
    o + match f {
        Face::PZ => IVec3::new(u, v, w),
        Face::NZ => IVec3::new(-u, v, -w),
        Face::PX => IVec3::new(w, v, -u),
        Face::NX => IVec3::new(-w, v, u),
    }
}

fn vox(p: Vec3) -> IVec3 {
    (p / 0.5).floor().as_ivec3()
}

/// The placer: exclusion checks + cap + bookkeeping.
struct Placer<'a> {
    spawns: &'a [Vec3],
    routes: &'a [Route],
    placed: Vec<Placed>,
}

impl Placer<'_> {
    fn blocked(&self, min: Vec3, max: Vec3) -> bool {
        let c = (min + max) * 0.5;
        let half = Vec2::new(max.x - min.x, max.z - min.z) * 0.5;
        // Horizontal distance from a point to the box footprint.
        let dist = |p: Vec3| {
            let d = (Vec2::new(p.x - c.x, p.z - c.z).abs() - half).max(Vec2::ZERO);
            d.length()
        };
        if self.spawns.iter().any(|s| s.y > min.y - 3.0 && s.y < max.y + 1.0 && dist(*s) < SPAWN_CLEAR) {
            return true;
        }
        for r in self.routes {
            if r.kind == RouteKind::Swing {
                // Keep the take-off and landing spots clear.
                if r.waypoints.iter().any(|w| w.y > min.y - 3.0 && w.y < max.y + 1.0 && dist(*w) < 2.5) {
                    return true;
                }
                continue;
            }
            for seg in r.waypoints.windows(2) {
                for k in 0..=40 {
                    let p = seg[0].lerp(seg[1], k as f32 / 40.0);
                    if p.y > min.y - 3.0 && p.y < max.y + 0.5 && dist(p) < 1.8 {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn ok(&self, name: &'static str, min: Vec3, max: Vec3) -> bool {
        self.placed.len() < MAX_PROPS && !self.blocked(min, max) && !name.is_empty()
    }

    /// Place `m` with its back-centre at metre point `at` (the first air voxel in front of a wall,
    /// on the floor), facing `f`. Writes into air only.
    fn model(&mut self, c: &mut City, m: &Model, at: Vec3, f: Face) -> bool {
        let o = vox(at);
        let (mut lo, mut hi) = (IVec3::splat(i32::MAX), IVec3::splat(i32::MIN));
        for (p, _) in &m.voxels {
            let v = to_world(o, m.size, *p, f);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        if m.voxels.is_empty() {
            return false;
        }
        let (min, max) = (lo.as_vec3() * 0.5, (hi + IVec3::ONE).as_vec3() * 0.5);
        if !self.ok(m.name, min, max) {
            return false;
        }
        for (p, mt) in &m.voxels {
            let v = to_world(o, m.size, *p, f);
            if c.w.get(v) == AIR {
                c.w.set(v, *mt);
            }
        }
        self.placed.push(Placed { name: m.name, min, max });
        true
    }

    /// Free-form prop drawn by `draw` inside the metre box `min..max`.
    fn custom(&mut self, c: &mut City, name: &'static str, min: Vec3, max: Vec3, draw: impl FnOnce(&mut City)) -> bool {
        if !self.ok(name, min, max) {
            return false;
        }
        draw(c);
        self.placed.push(Placed { name, min, max });
        true
    }
}

// ---- kit furniture ------------------------------------------------------------------------------

fn arcade(screen: Material, marquee: Material, body: Material) -> Model {
    Model::new("arcade cabinet")
        .b((0, 0, 0), (2, 6, 1), body)
        .b((0, 2, 2), (2, 2, 3), body)
        .b((0, 3, 2), (2, 3, 2), mat::PAINT_RED)
        .b((1, 3, 3), (1, 3, 3), mat::PAINT_YELLOW)
        .b((0, 4, 1), (2, 5, 1), screen)
        .b((0, 6, 1), (2, 6, 2), marquee)
}

fn pinball() -> Model {
    Model::new("pinball machine")
        .b((0, 0, 1), (0, 1, 1), mat::STEEL)
        .b((2, 0, 1), (2, 1, 1), mat::STEEL)
        .b((0, 0, 4), (0, 1, 4), mat::STEEL)
        .b((2, 0, 4), (2, 1, 4), mat::STEEL)
        .b((0, 2, 1), (2, 2, 4), mat::HULL_NAVY)
        .b((1, 2, 2), (1, 2, 3), mat::NEON_PINK)
        .b((0, 2, 0), (2, 5, 0), mat::HULL_NAVY)
        .b((0, 3, 0), (2, 4, 0), mat::NEON_CYAN)
}

fn jukebox() -> Model {
    Model::new("jukebox")
        .b((0, 0, 0), (2, 4, 1), mat::DARK_WOOD)
        .b((0, 1, 1), (2, 3, 1), mat::NEON_AMBER)
        .b((1, 4, 1), (1, 4, 1), mat::NEON_PINK)
        .b((0, 5, 0), (2, 5, 0), mat::NEON_PINK)
}

fn crt_stack() -> Model {
    Model::new("crt stack")
        .b((0, 0, 0), (4, 2, 2), mat::PLASTIC_BLACK)
        .b((1, 1, 2), (3, 2, 2), mat::NEON_CYAN)
        .b((1, 3, 0), (3, 5, 2), mat::STEEL)
        .b((2, 4, 2), (3, 5, 2), mat::MUTAGEN)
        .b((0, 3, 0), (0, 4, 1), mat::PLASTIC_BLACK)
        .b((4, 3, 0), (4, 4, 1), mat::PLASTIC_BLACK)
        .b((2, 6, 1), (2, 7, 1), mat::STEEL)
}

fn tv_on_crate() -> Model {
    Model::new("tv")
        .b((0, 0, 0), (4, 1, 2), mat::WOOD)
        .b((0, 2, 0), (4, 4, 2), mat::PLASTIC_BLACK)
        .b((1, 2, 2), (3, 4, 2), mat::NEON_CYAN)
        .b((1, 5, 1), (1, 6, 1), mat::STEEL)
        .b((3, 5, 1), (3, 6, 1), mat::STEEL)
}

fn desk(screens: [Material; 3]) -> Model {
    let mut m = Model::new("lab desk")
        .b((0, 0, 0), (0, 1, 0), mat::STEEL)
        .b((9, 0, 0), (9, 1, 0), mat::STEEL)
        .b((0, 0, 2), (0, 1, 2), mat::STEEL)
        .b((9, 0, 2), (9, 1, 2), mat::STEEL)
        .b((0, 2, 0), (9, 2, 2), mat::WOOD);
    for (k, s) in screens.iter().enumerate() {
        let u = 1 + 3 * k as i32;
        m = m.b((u, 3, 0), (u + 1, 4, 0), mat::PLASTIC_BLACK).b((u, 4, 1), (u + 1, 4, 1), *s).b((u, 3, 1), (u + 1, 3, 1), mat::PLASTIC_BLACK);
    }
    m
}

fn workbench() -> Model {
    Model::new("workbench")
        .b((0, 0, 0), (0, 1, 1), mat::DARK_WOOD)
        .b((7, 0, 0), (7, 1, 1), mat::DARK_WOOD)
        .b((0, 2, 0), (7, 2, 1), mat::WOOD)
        .b((1, 3, 0), (2, 3, 0), mat::PAINT_RED)
        .b((4, 3, 0), (4, 4, 0), mat::STEEL)
        .b((6, 3, 1), (6, 3, 1), mat::HAZARD_ORANGE)
}

fn pizza_boxes(n: i32) -> Model {
    let mut m = Model::new("pizza boxes");
    for k in 0..n {
        m = m.b((0, k, 0), (1, k, 1), if k % 3 == 2 { mat::PAINT_WHITE } else { mat::HOTEL_CARPET_GOLD });
    }
    m.b((0, n - 1, 0), (0, n - 1, 0), mat::PAINT_RED)
}

fn weapon_rack() -> Model {
    Model::new("weapon rack")
        .b((0, 0, 0), (0, 6, 0), mat::DARK_WOOD)
        .b((11, 0, 0), (11, 6, 0), mat::DARK_WOOD)
        .b((0, 2, 0), (11, 2, 0), mat::DARK_WOOD)
        .b((0, 6, 0), (11, 6, 0), mat::DARK_WOOD)
        .b((0, 0, 1), (11, 0, 1), mat::DARK_WOOD)
}

fn dummy() -> Model {
    Model::new("wooden dummy")
        .b((0, 0, 0), (2, 0, 2), mat::DARK_WOOD)
        .b((1, 1, 1), (1, 5, 1), mat::WOOD)
        .b((0, 3, 1), (2, 3, 1), mat::DARK_WOOD)
        .b((1, 4, 2), (1, 4, 3), mat::DARK_WOOD)
        .b((1, 2, 2), (1, 2, 3), mat::DARK_WOOD)
        .b((1, 6, 1), (1, 6, 1), mat::DARK_WOOD)
}

fn bench() -> Model {
    Model::new("bench").b((0, 0, 0), (0, 0, 1), mat::STEEL).b((5, 0, 0), (5, 0, 1), mat::STEEL).b((0, 1, 0), (5, 1, 1), mat::WOOD).b(
        (0, 2, 0),
        (5, 3, 0),
        mat::WOOD,
    )
}

fn cone() -> Model {
    Model::new("traffic cone").b((0, 0, 0), (1, 0, 1), mat::HAZARD_ORANGE).b((0, 1, 0), (0, 2, 0), mat::HAZARD_ORANGE).b(
        (0, 2, 0),
        (0, 2, 0),
        mat::PAINT_WHITE,
    )
}

fn icon_or(name: &'static str, pal: [Material; 4], fallback: fn() -> Model) -> Model {
    Model::icon(name, pal).unwrap_or_else(fallback)
}

// ---- placement ------------------------------------------------------------------------------------

/// Place the lair's decor. Returns what was placed (name + bounds).
pub fn place_props(c: &mut City, spawns: &[Vec3], routes: &[Route], rng: &mut Rng) -> Vec<Placed> {
    let mut p = Placer { spawns, routes, placed: Vec::new() };
    let f = FLOOR;
    let tat = FLOOR + 0.5;
    let (wx, nz) = (HUB[0] + 0.25, HUB[3] - 0.25);

    // ---- the pit: TV on a crate, five bean bags (brothers' colours + purple) round the light ----
    p.model(c, &tv_on_crate(), Vec3::new(0.0, PIT_FLOOR, 3.25), Face::PZ);
    let bags = [
        (Vec2::new(-3.1, 6.6), mat::BALL_GREEN),
        (Vec2::new(3.1, 6.4), mat::HAZARD_ORANGE),
        (Vec2::new(-2.7, 9.9), mat::BALL_RED),
        (Vec2::new(2.8, 9.8), mat::BALL_BLUE),
        (Vec2::new(0.1, 10.3), mat::STUCCO_LILAC),
    ];
    for (k, (b, m)) in bags.into_iter().enumerate() {
        let j = Vec2::new(rng.range(-0.2, 0.2), rng.range(-0.2, 0.2));
        let ctr = Vec3::new(b.x + j.x, PIT_FLOOR + 0.55, b.y + j.y);
        let r = Vec3::new(0.95, 0.65, 0.95 - 0.1 * (k % 2) as f32);
        p.custom(c, "bean bag", ctr - r, ctr + r, |c| c.w.fill_ellipsoid(ctr, r, m, true));
    }
    p.model(c, &pizza_boxes(3), Vec3::new(-0.5, PIT_FLOOR, 9.2), Face::NZ);

    // ---- dojo: weapon rack (icons), tires, bag, dummies, Splinter's shrine tree -------------------
    let rack_at = Vec3::new(-14.75, tat, nz);
    if p.model(c, &weapon_rack(), rack_at, Face::NZ) {
        let weapons = [
            ("katanas", mat::BALL_BLUE),
            ("sais", mat::PAINT_RED),
            ("bo staff", mat::STUCCO_LILAC),
            ("nunchucks", mat::HAZARD_ORANGE),
        ];
        for (k, (name, accent)) in weapons.into_iter().enumerate() {
            let pal = [mat::STEEL, accent, mat::DARK_WOOD, mat::HULL_WHITE];
            if let Some(m) = Model::icon(name, pal) {
                let m = m.stand_up();
                // In front of the rack, leaning on its lower bar.
                let x = rack_at.x + 2.2 - 1.4 * k as f32;
                let at = Vec3::new(x, tat, nz - 0.5);
                let o = vox(at);
                for (q, mt) in &m.voxels {
                    let v = to_world(o, m.size, *q, Face::NZ);
                    if c.w.get(v) == AIR {
                        c.w.set(v, *mt);
                    }
                }
            }
        }
    }
    for (x, z, top) in [(-6.9f32, 18.9f32, 15.2f32), (-6.9, 20.4, 14.6)] {
        let ceil = hub_ceiling(z);
        p.custom(c, "hanging tire", Vec3::new(x - 1.0, top - 1.8, z - 0.5), Vec3::new(x + 1.0, ceil, z + 0.5), |c| {
            let ctr = Vec3::new(x, top - 0.9, z);
            c.fill_with(ctr - Vec3::new(1.0, 1.0, 0.3), ctr + Vec3::new(1.0, 1.0, 0.3), |q, _, old| {
                let d = Vec2::new(q.x - ctr.x, q.y - ctr.y).length();
                (old == AIR && (0.45..0.95).contains(&d)).then_some(mat::PLASTIC_BLACK)
            });
            c.line(Vec3::new(x + 0.1, top - 0.1, z), Vec3::new(x + 0.1, ceil + 0.3, z), mat::DARK_WOOD, true);
        });
    }
    {
        let (x, z) = (-19.6f32, 19.4f32);
        let ceil = hub_ceiling(z);
        p.custom(c, "punching bag", Vec3::new(x - 0.7, tat + 1.0, z - 0.7), Vec3::new(x + 0.7, ceil, z + 0.7), |c| {
            c.fill_with(Vec3::new(x - 0.7, tat + 1.0, z - 0.7), Vec3::new(x + 0.7, tat + 3.5, z + 0.7), |q, _, old| {
                let d = Vec2::new(q.x - x, q.z - z).length();
                let band = (q.y - tat - 1.0).rem_euclid(1.0) < 0.5 && q.y > tat + 3.0;
                (old == AIR && d < 0.7).then_some(if band { mat::PLASTIC_BLACK } else { mat::PAINT_RED })
            });
            c.line(Vec3::new(x, tat + 3.4, z), Vec3::new(x, ceil + 0.3, z), mat::STEEL, true);
        });
    }
    p.model(c, &dummy(), Vec3::new(-9.0, tat, 23.2), Face::NZ);
    p.model(c, &dummy(), Vec3::new(-6.9, tat, 23.2), Face::NZ);
    {
        // Splinter's corner: a stone plinth with candles, a little cherry tree, a paper lantern.
        let (x, z) = (-20.2f32, 22.4f32);
        p.custom(c, "shrine tree", Vec3::new(x - 1.2, tat, z - 1.2), Vec3::new(x + 1.8, tat + 3.6, z + 1.1), |c| {
            c.fill(Vec3::new(x - 1.0, tat, z - 1.0), Vec3::new(x + 1.0, tat + 0.5, z + 1.0), mat::STONE);
            c.curve(Vec3::new(x, tat + 0.5, z), Vec3::new(x + 0.9, tat + 1.4, z - 0.2), Vec3::new(x + 0.2, tat + 2.4, z), 0.3, 0.25, mat::DARK_WOOD, true);
            for (o, r) in [(Vec3::new(0.2, 2.8, 0.0), 0.9), (Vec3::new(0.9, 2.5, -0.3), 0.6), (Vec3::new(-0.4, 2.4, 0.3), 0.55)] {
                c.w.fill_ellipsoid(Vec3::new(x, tat, z) + o, Vec3::new(r * 1.3, r * 0.8, r * 1.2), mat::LEAVES_PINK, true);
            }
            for (dx, dz) in [(-0.75f32, 0.75f32), (0.75, 0.75), (-0.75, -0.75)] {
                c.fill(Vec3::new(x + dx - 0.25, tat + 0.5, z + dz - 0.25), Vec3::new(x + dx + 0.25, tat + 1.0, z + dz + 0.25), mat::NEON_AMBER);
            }
        });
        let (lx, lz) = (-17.0f32, 22.6f32);
        let ceil = hub_ceiling(lz);
        p.custom(c, "paper lantern", Vec3::new(lx - 0.5, 16.0, lz - 0.5), Vec3::new(lx + 0.5, ceil, lz + 0.5), |c| {
            c.fill(Vec3::new(lx - 0.5, 16.0, lz - 0.5), Vec3::new(lx + 0.5, 17.0, lz + 0.5), mat::NEON_AMBER);
            c.fill(Vec3::new(lx - 0.5, 17.0, lz - 0.5), Vec3::new(lx + 0.5, 17.5, lz + 0.5), mat::DARK_WOOD);
            c.line(Vec3::new(lx, 17.25, lz), Vec3::new(lx, ceil + 0.3, lz), mat::DARK_WOOD, true);
        });
    }

    // ---- lab: desks + screens, a wall screen, Krang's body, canisters, mousers ----------------------
    p.model(c, &desk([mat::MUTAGEN, mat::NEON_CYAN, mat::MUTAGEN]), Vec3::new(8.0, f, nz), Face::NZ);
    p.model(c, &desk([mat::NEON_CYAN, mat::NEON_CYAN, mat::MUTAGEN]), Vec3::new(16.0, f, nz), Face::NZ);
    {
        let s = rng.next_u32();
        p.custom(c, "wall screen", Vec3::new(5.0, 15.0, nz - 0.5), Vec3::new(19.0, 18.0, nz + 0.5), move |c| {
            c.fill_with(Vec3::new(5.0, 15.0, HUB[3]), Vec3::new(19.0, 18.0, HUB[3] + 0.5), |q, v, old| {
                if old == AIR || old == mat::SEWER_ARCH || old == mat::NEON_AMBER || old == mat::TEAL_PIPE {
                    return None;
                }
                let edge = q.x < 5.5 || q.x > 18.5 || q.y < 15.5 || q.y > 17.5;
                let h = crate::noise::hash3(v.x, v.y, 1, s) % 10;
                Some(if edge {
                    mat::PLASTIC_BLACK
                } else if h < 5 {
                    mat::NEON_CYAN
                } else if h < 7 {
                    mat::MUTAGEN
                } else {
                    mat::HULL_NAVY
                })
            });
        });
    }
    if let Some(k) = Model::icon("krang android", [mat::STUCCO_PEACH, mat::NEON_PINK, mat::HULL_NAVY, mat::HULL_WHITE]) {
        let at = Vec3::new(20.0, f, 18.6);
        p.custom(c, "krang pedestal", Vec3::new(18.5, f, 17.6), Vec3::new(21.5, f + 0.5, 20.6), |c| {
            c.fill(Vec3::new(18.5, f, 17.6), Vec3::new(21.5, f + 0.5, 20.6), mat::STEEL);
        });
        p.model(c, &k, at + Vec3::Y * 0.5, Face::NX);
    }
    let canister = icon_or("ooze canister", [mat::STEEL, mat::MUTAGEN, mat::PLASTIC_BLACK, mat::MUTAGEN], cone);
    for z in [19.4f32, 21.0, 22.6] {
        p.model(c, &canister, Vec3::new(HUB[2] - 0.25, f, z), Face::NX);
    }
    let mouser = icon_or("mouser", [mat::HULL_NAVY, mat::HULL_WHITE, mat::PLASTIC_BLACK, mat::PAINT_RED], cone);
    p.model(c, &mouser, Vec3::new(7.0, f, 19.4), Face::PZ);
    p.model(c, &mouser.clone().quarter(), Vec3::new(8.6, f, 20.3), Face::PX);

    // ---- arcade corner (west wall): cabinets, pinball, jukebox, neon pizza sign, boxes, boards -----
    let cabinets = [
        (-2.9f32, mat::NEON_PINK, mat::NEON_AMBER),
        (-1.2, mat::NEON_CYAN, mat::NEON_PINK),
        (9.6, mat::MUTAGEN, mat::NEON_CYAN),
        (11.4, mat::NEON_AMBER, mat::NEON_PINK),
    ];
    for (z, screen, marquee) in cabinets {
        let body = if z < 0.0 { mat::PLASTIC_BLACK } else { mat::HULL_NAVY };
        p.model(c, &arcade(screen, marquee, body), Vec3::new(wx, f, z), Face::PX);
    }
    p.model(c, &pinball(), Vec3::new(wx, f, -4.9), Face::PX);
    p.model(c, &jukebox(), Vec3::new(wx, f, -7.0), Face::PX);
    if let Some(sign) = Model::icon("pizza sign", [mat::PAINT_YELLOW, mat::NEON_AMBER, mat::NEON_PINK, mat::NEON_AMBER]) {
        p.model(c, &sign.stand_up(), Vec3::new(wx, 16.0, 11.2), Face::PX);
    }
    p.model(c, &pizza_boxes(5), Vec3::new(-16.5, f, 12.4), Face::NZ);
    p.model(c, &pizza_boxes(4), Vec3::new(-15.4, f, 12.4), Face::NZ);
    let board = |a: Material| icon_or("skateboard", [a, mat::PLASTIC_BLACK, mat::PLASTIC_BLACK, mat::HULL_WHITE], cone);
    p.model(c, &board(mat::HAZARD_ORANGE), Vec3::new(-14.0, f, -3.2), Face::PX);
    p.model(c, &board(mat::BALL_GREEN).stand_up(), Vec3::new(wx, f, 13.2), Face::PX);

    // ---- workshop (east of the pit): bench, CRT clutter, scrap pile --------------------------------
    p.model(c, &workbench(), Vec3::new(15.0, f, -3.6), Face::PZ);
    p.model(c, &crt_stack(), Vec3::new(20.5, f, -3.6), Face::PZ);
    {
        let (x, z) = (19.0f32, 10.5f32);
        let s = rng.next_u32();
        p.custom(c, "scrap pile", Vec3::new(x - 1.8, f, z - 1.4), Vec3::new(x + 1.8, f + 1.6, z + 1.4), move |c| {
            let pal = [mat::RUST, mat::STEEL, mat::TEAL_PIPE, mat::HAZARD_ORANGE, mat::PLASTIC_BLACK];
            c.fill_with(Vec3::new(x - 1.8, f, z - 1.4), Vec3::new(x + 1.8, f + 1.6, z + 1.4), |q, v, old| {
                let d = Vec2::new((q.x - x) / 1.8, (q.z - z) / 1.4).length();
                let h = 1.5 * (1.0 - d);
                let pick = crate::noise::hash3(v.x, v.y, v.z, s);
                (old == AIR && q.y - f < h && pick % 7 != 0).then(|| pal[(pick >> 4) as usize % pal.len()])
            });
        });
    }
    contraption(c, &mut p);

    // ---- garage: the party wagon, tool wall, oil drums ------------------------------------------------
    if let Some(van) = Model::icon("party wagon", [mat::BALL_GREEN, mat::HAZARD_ORANGE, mat::PLASTIC_BLACK, mat::NEON_AMBER]) {
        p.model(c, &van, Vec3::new(26.5, f, 10.2), Face::PX);
    }
    p.custom(c, "tool wall", Vec3::new(25.0, 13.5, 15.0), Vec3::new(32.0, 16.5, 16.0), |c| {
        c.fill_with(Vec3::new(25.0, 13.5, 15.5), Vec3::new(32.0, 16.5, 16.0), |q, v, old| {
            if old != AIR {
                return None;
            }
            let tool = v.x.rem_euclid(3) == 0 && q.y > 14.0 && q.y < 16.0;
            Some(if tool { [mat::PAINT_RED, mat::STEEL, mat::HAZARD_ORANGE][(v.x / 3).rem_euclid(3) as usize] } else { mat::WOOD })
        });
    });
    for (k, (x, z)) in [(35.2f32, 14.2f32), (36.4, 12.9), (23.6, -2.9)].into_iter().enumerate() {
        let m = if k == 1 { mat::HULL_NAVY } else { mat::RUST };
        p.custom(c, "oil drum", Vec3::new(x - 0.55, f, z - 0.55), Vec3::new(x + 0.55, f + 1.5, z + 0.55), |c| {
            c.fill_with(Vec3::new(x - 0.55, f, z - 0.55), Vec3::new(x + 0.55, f + 1.5, z + 0.55), |q, _, old| {
                (old == AIR && Vec2::new(q.x - x, q.z - z).length() < 0.55).then_some(m)
            });
        });
    }

    // ---- skate hall: boards on the decks, cones at the flat ends -----------------------------------
    p.model(c, &board(mat::BALL_BLUE), Vec3::new(-6.0, super::DECK_Y, -12.6), Face::NZ);
    p.model(c, &board(mat::PAINT_RED), Vec3::new(7.0, super::DECK_Y, -29.6), Face::PZ);
    for (x, z) in [(22.0f32, -15.0f32), (23.5, -26.0), (-24.0, -14.0)] {
        p.model(c, &cone(), Vec3::new(x, f, z), Face::PZ);
    }

    // ---- junctions: junk -------------------------------------------------------------------------------
    let cart = icon_or("shopping cart", [mat::STEEL, mat::PAINT_RED, mat::PLASTIC_BLACK, mat::MULLION], cone);
    for (j, dz) in [(super::JUNCTIONS[0], 1.0f32), (super::JUNCTIONS[1], -1.0)] {
        p.model(c, &cart, Vec3::new(j.x - 4.6, f, j.y + dz * 3.6), Face::PX);
        p.model(c, &cone(), Vec3::new(j.x + 4.4, f, j.y + dz * 3.8), Face::NX);
    }

    // ---- station platform: the Shellraiser, benches ------------------------------------------------
    if let Some(car) = Model::icon("shellraiser", [mat::MOSSY_BRICK, mat::HAZARD_ORANGE, mat::PLASTIC_BLACK, mat::NEON_AMBER]) {
        p.model(c, &car, Vec3::new(PLATFORM[2] - 1.9, f, 37.2), Face::PZ);
    }
    let mut z = PLATFORM[1] + 6.0;
    while z < 30.0 {
        p.model(c, &bench(), Vec3::new(PLATFORM[2] - 0.25, f, z), Face::NX);
        z += 16.0;
    }

    // ---- streets: manhole covers pushed aside next to the open shafts ------------------------------
    if let Some(lid) = Model::icon("manhole cover", [mat::STEEL, mat::STEEL, mat::ROOF_DARK, mat::STEEL]) {
        for j in super::JUNCTIONS {
            p.model(c, &lid, Vec3::new(j.x + 4.0, STREET, j.y + 1.0), Face::PZ);
        }
        p.model(c, &lid, Vec3::new(super::MANHOLE.x + 3.2, STREET, super::MANHOLE.y - 2.0), Face::PZ);
    }
    p.placed
}

/// The scrap contraption hung from the vault over the workshop: a standable grating platform with
/// a tank, a propeller, a dish and lamps, on four chains (shoot them and it falls).
fn contraption(c: &mut City, p: &mut Placer) {
    let (cx, cz) = (12.5f32, -0.8f32);
    let (y0, y1) = (19.5f32, 23.0f32);
    let (hx, hz) = (2.0f32, 2.0f32);
    let top = hub_ceiling(cz + hz).min(hub_ceiling(cz - hz));
    p.custom(c, "scrap contraption", Vec3::new(cx - hx - 1.5, y0 - 1.0, cz - hz), Vec3::new(cx + hx + 1.5, top, cz + hz), |c| {
        // Grating floor + corner posts + top frame.
        c.fill_with(Vec3::new(cx - hx, y0, cz - hz), Vec3::new(cx + hx, y0 + 0.5, cz + hz), |_, v, _| {
            Some(if (v.x + v.z).rem_euclid(2) == 0 { mat::STEEL } else { mat::RUST })
        });
        for (dx, dz) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            let (px, pz) = (cx + dx * (hx - 0.25), cz + dz * (hz - 0.25));
            c.fill(Vec3::new(px - 0.25, y0, pz - 0.25), Vec3::new(px + 0.25, y1, pz + 0.25), mat::RUST);
            // Chains up to the vault.
            c.line(Vec3::new(px, y1 - 0.25, pz), Vec3::new(px, hub_ceiling(pz) + 0.3, pz), mat::STEEL, true);
        }
        for (a, b) in [
            (Vec3::new(cx - hx, y1 - 0.5, cz - hz), Vec3::new(cx + hx, y1, cz - hz + 0.5)),
            (Vec3::new(cx - hx, y1 - 0.5, cz + hz - 0.5), Vec3::new(cx + hx, y1, cz + hz)),
            (Vec3::new(cx - hx, y1 - 0.5, cz - hz), Vec3::new(cx - hx + 0.5, y1, cz + hz)),
            (Vec3::new(cx + hx - 0.5, y1 - 0.5, cz - hz), Vec3::new(cx + hx, y1, cz + hz)),
        ] {
            c.fill(a, b, mat::RUST);
        }
        // Tank on the grating, a propeller off the east end, a dish under the west end, lamps.
        c.tube(Vec3::new(cx - 1.2, y0 + 1.3, cz), Vec3::new(cx + 1.2, y0 + 1.3, cz), 0.8, mat::TEAL_PIPE, true);
        c.fill(Vec3::new(cx + hx, y0 + 1.0, cz - 0.25), Vec3::new(cx + hx + 0.5, y0 + 1.5, cz + 0.25), mat::STEEL);
        c.fill(Vec3::new(cx + hx + 0.5, y0 - 0.2, cz - 0.25), Vec3::new(cx + hx + 1.0, y0 + 2.7, cz + 0.25), mat::PAINT_RED);
        c.fill(Vec3::new(cx + hx + 0.5, y0 + 1.0, cz - 1.5), Vec3::new(cx + hx + 1.0, y0 + 1.5, cz + 1.5), mat::PAINT_RED);
        c.w.fill_ellipsoid(Vec3::new(cx - hx - 0.6, y0 + 0.2, cz), Vec3::new(0.5, 1.1, 1.1), mat::HULL_WHITE, true);
        for (dx, dz) in [(-1.0f32, -1.0f32), (1.0, 1.0)] {
            c.fill(
                Vec3::new(cx + dx * (hx - 0.5) - 0.25, y0 - 0.5, cz + dz * (hz - 0.5) - 0.25),
                Vec3::new(cx + dx * (hx - 0.5) + 0.25, y0, cz + dz * (hz - 0.5) + 0.25),
                mat::NEON_AMBER,
            );
        }
        c.fill(Vec3::new(cx - 0.5, y0 + 0.5, cz + hz - 1.0), Vec3::new(cx + 0.5, y0 + 1.0, cz + hz - 0.5), mat::HAZARD_ORANGE);
    });
}
