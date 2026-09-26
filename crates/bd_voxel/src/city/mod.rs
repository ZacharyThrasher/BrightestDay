//! The **city kit**: parametric, seeded generators for urban maps (Miami today; Tokyo/NYC later should
//! be mostly data — new specs, palettes and a layout function in `crate::maps`).
//!
//! Everything works in metres (+X east, +Z north, Y up) on a [`City`] builder that wraps the
//! [`VoxelWorld`] being generated plus deterministic randomness. Generators never read global state,
//! use only `+ - * / sqrt floor` and the crate's polynomial trig, so a seed builds the same city on
//! every machine.
//!
//! Structural rules the kit follows (so destruction always behaves):
//! - The city stands on an indestructible bedrock layer ([`City::land`]); every building, street, prop
//!   and bridge is 6-connected down to it or to an indestructible core, so nothing floats and nothing
//!   collapses spontaneously the first time something nearby is carved.
//! - Tall things carry an indestructible slender core ([`mat::TOWER_CORE`]): towers can lose chunks,
//!   floors and whole facades, but never vanish. Bridge piers are cores too.
//! - Buildings are hollow shells with floor slabs (never solid), so blasting a facade reveals floors
//!   and the mesh stays cheap.
//!
//! | module | what |
//! |---|---|
//! | [`tower`] | glass skyscrapers: mullion grid, setbacks, balconies, open sky-lobbies, crowns, cores |
//! | [`deco`] | art-deco hotels: pastel stucco, rounded corners, eyebrows, neon fin signs, porches, awnings |
//! | [`street`] | street networks: asphalt, lane lines, crosswalks, sidewalks/curbs (0.5 m step-ups) |
//! | [`nature`] | palms (curved trunks, frond clusters), lifeguard towers |
//! | [`bridge`] | causeways on core piers, through-arch and cable-stayed spans |
//! | [`ship`] | a walkable cruise ship: hull, terraced decks, bridge wings, funnel, pool |
//! | [`props`] | cars, billboards, water towers, containers, gantry cranes, street lights, stairs |
//! | [`stadium`] | an elliptical seating bowl with pitch, tunnels and light towers |

pub mod bridge;
pub mod deco;
pub mod nature;
pub mod props;
pub mod ship;
pub mod stadium;
pub mod street;
pub mod tower;

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{AIR, Material};
use glam::{IVec3, Vec2, Vec3};

use crate::material::{mat, material_def};
use crate::noise::{Rng, TAU, dcos, dsin};
use crate::world::VoxelWorld;

pub use bridge::{BridgeSpec, BridgeStyle};
pub use deco::HotelSpec;
pub use nature::{LifeguardSpec, PalmSpec};
pub use props::{BillboardSpec, CarSpec};
pub use ship::ShipSpec;
pub use stadium::StadiumSpec;
pub use street::{StreetNet, StreetSpec};
pub use tower::{Crown, TowerSpec};

pub(crate) const VS: f32 = VOXEL_SIZE;

/// Voxel index containing metre coordinate `m`.
#[inline]
pub fn vx(m: f32) -> i32 {
    (m / VS).floor() as i32
}

/// Centre of voxel `v` in metres.
#[inline]
pub fn vc(v: IVec3) -> Vec3 {
    (v.as_vec3() + Vec3::splat(0.5)) * VS
}

/// Smallest voxel index whose centre is ≥ `m`.
#[inline]
fn lo_idx(m: f32) -> i32 {
    (m / VS - 0.5).ceil() as i32
}

/// Largest voxel index whose centre is < `m` (for metre-aligned bounds).
#[inline]
fn hi_idx(m: f32) -> i32 {
    (m / VS - 0.5).ceil() as i32 - 1
}

/// Axis-aligned rectangle on the ground plane (x, z) in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const fn new(x0: f32, z0: f32, x1: f32, z1: f32) -> Self {
        Self { min: Vec2::new(x0, z0), max: Vec2::new(x1, z1) }
    }
    pub fn from_center(c: Vec2, half: Vec2) -> Self {
        Self { min: c - half, max: c + half }
    }
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }
    pub fn half(&self) -> Vec2 {
        (self.max - self.min) * 0.5
    }
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }
    /// Shrink by `d` on every side (negative grows).
    pub fn inset(&self, d: f32) -> Self {
        Self { min: self.min + d, max: self.max - d }
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x < self.max.x && p.y >= self.min.y && p.y < self.max.y
    }
    /// Signed distance to the rectangle with corners rounded by `r` (negative inside).
    pub fn sdf(&self, p: Vec2, r: f32) -> f32 {
        let h = self.half();
        let r = r.min(h.x).min(h.y).max(0.0);
        let q = (p - self.center()).abs() - (h - r);
        q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - r
    }
}

/// Which way a building's front faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    PosX,
    NegX,
    PosZ,
    NegZ,
}

impl Facing {
    /// Outward unit normal of the front.
    pub fn normal(self) -> Vec2 {
        match self {
            Facing::PosX => Vec2::X,
            Facing::NegX => Vec2::NEG_X,
            Facing::PosZ => Vec2::Y,
            Facing::NegZ => Vec2::NEG_Y,
        }
    }
}

/// Local frame of a rectangular lot seen from its front: `u` runs along the front (0 at the left
/// corner as seen from the street, `width` at the right), `v` is depth (0 at the front face, `depth` at
/// the back, negative in front of the building).
#[derive(Clone, Copy, Debug)]
pub struct LotFrame {
    pub rect: Rect,
    pub facing: Facing,
    pub width: f32,
    pub depth: f32,
}

impl LotFrame {
    pub fn new(rect: Rect, facing: Facing) -> Self {
        let s = rect.size();
        let (width, depth) = match facing {
            Facing::PosX | Facing::NegX => (s.y, s.x),
            Facing::PosZ | Facing::NegZ => (s.x, s.y),
        };
        Self { rect, facing, width, depth }
    }
    /// World (x, z) → local (u, v).
    pub fn local(&self, p: Vec2) -> Vec2 {
        let (a, b) = (self.rect.min, self.rect.max);
        match self.facing {
            Facing::PosX => Vec2::new(b.y - p.y, b.x - p.x),
            Facing::NegX => Vec2::new(p.y - a.y, p.x - a.x),
            Facing::PosZ => Vec2::new(p.x - a.x, b.y - p.y),
            Facing::NegZ => Vec2::new(b.x - p.x, p.y - a.y),
        }
    }
    /// Local (u, v) → world (x, z).
    pub fn world(&self, l: Vec2) -> Vec2 {
        let (a, b) = (self.rect.min, self.rect.max);
        match self.facing {
            Facing::PosX => Vec2::new(b.x - l.y, b.y - l.x),
            Facing::NegX => Vec2::new(a.x + l.y, a.y + l.x),
            Facing::PosZ => Vec2::new(a.x + l.x, b.y - l.y),
            Facing::NegZ => Vec2::new(b.x - l.x, a.y + l.y),
        }
    }
    /// World-space rect covering local `u0..u1`, `v0..v1`.
    pub fn world_rect(&self, u0: f32, v0: f32, u1: f32, v1: f32) -> Rect {
        let a = self.world(Vec2::new(u0, v0));
        let b = self.world(Vec2::new(u1, v1));
        Rect { min: a.min(b), max: a.max(b) }
    }
}

/// The city builder: the world being generated plus deterministic randomness.
pub struct City {
    pub w: VoxelWorld,
    pub rng: Rng,
    /// Registered indestructible anchor voxels (also pushed to the world as collapse hints).
    pub anchors: Vec<IVec3>,
}

impl City {
    pub fn new(w: VoxelWorld, seed: u64) -> Self {
        Self { w, rng: Rng::new(seed), anchors: Vec::new() }
    }

    pub fn seed(&mut self) -> u32 {
        self.rng.next_u32()
    }

    /// Register an indestructible voxel as a collapse-search anchor hint.
    pub fn anchor(&mut self, v: IVec3) {
        if material_def(self.w.get(v)).indestructible {
            self.w.add_anchor_hint(v);
            self.anchors.push(v);
        }
    }

    // ----- metre-space brushes ------------------------------------------------------------------

    /// Fill every voxel whose centre lies in `[min, max)` (metres).
    pub fn fill(&mut self, min: Vec3, max: Vec3, m: Material) {
        let lo = IVec3::new(lo_idx(min.x), lo_idx(min.y), lo_idx(min.z));
        let hi = IVec3::new(hi_idx(max.x), hi_idx(max.y), hi_idx(max.z));
        if lo.cmpgt(hi).any() {
            return;
        }
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                self.w.fill_column(x, z, lo.y, hi.y + 1, m);
            }
        }
    }

    /// Like [`City::fill`] but only replaces air.
    pub fn fill_air(&mut self, min: Vec3, max: Vec3, m: Material) {
        self.fill_with(min, max, |_, _, old| (old == AIR).then_some(m));
    }

    /// Visit voxels with centres in `[min, max)`: `f(centre_m, voxel, old) -> Some(new)`.
    pub fn fill_with(&mut self, min: Vec3, max: Vec3, mut f: impl FnMut(Vec3, IVec3, Material) -> Option<Material>) {
        let lo = IVec3::new(lo_idx(min.x), lo_idx(min.y), lo_idx(min.z));
        let hi = IVec3::new(hi_idx(max.x), hi_idx(max.y), hi_idx(max.z));
        self.w.fill_with(lo, hi, |v, old| f(vc(v), v, old));
    }

    /// A one-voxel line from `a` to `b` that is 6-connected (every voxel the segment passes through),
    /// so thin cables/fronds stay attached for structural pruning and collapse.
    pub fn line(&mut self, a: Vec3, b: Vec3, m: Material, only_air: bool) {
        let mut v = (a / VS).floor().as_ivec3();
        let end = (b / VS).floor().as_ivec3();
        let d = b - a;
        let step = IVec3::new(d.x.signum() as i32, d.y.signum() as i32, d.z.signum() as i32);
        let inv = |c: f32| if c != 0.0 { (VS / c).abs() } else { f32::INFINITY };
        let t_delta = Vec3::new(inv(d.x), inv(d.y), inv(d.z));
        let first = |o: f32, vi: i32, dc: f32| {
            if dc > 0.0 {
                ((vi + 1) as f32 * VS - o) / dc
            } else if dc < 0.0 {
                (vi as f32 * VS - o) / dc
            } else {
                f32::INFINITY
            }
        };
        let mut t_max = Vec3::new(first(a.x, v.x, d.x), first(a.y, v.y, d.y), first(a.z, v.z, d.z));
        let put = |w: &mut VoxelWorld, v: IVec3| {
            if !only_air || w.get(v) == AIR {
                w.set(v, m);
            }
        };
        put(&mut self.w, v);
        let mut guard = 0;
        while v != end && guard < 4096 {
            guard += 1;
            if t_max.min_element() > 1.0 {
                break;
            }
            if t_max.x < t_max.y && t_max.x < t_max.z {
                v.x += step.x;
                t_max.x += t_delta.x;
            } else if t_max.y < t_max.z {
                v.y += step.y;
                t_max.y += t_delta.y;
            } else {
                v.z += step.z;
                t_max.z += t_delta.z;
            }
            put(&mut self.w, v);
        }
        // Guarantee the end voxel is reached with 6-connected steps.
        while v != end {
            let dd = end - v;
            if dd.x != 0 {
                v.x += dd.x.signum();
            } else if dd.y != 0 {
                v.y += dd.y.signum();
            } else {
                v.z += dd.z.signum();
            }
            put(&mut self.w, v);
        }
    }

    /// A tube from `a` to `b` (radius ≤ 0.25 m = a 6-connected voxel line).
    pub fn tube(&mut self, a: Vec3, b: Vec3, r: f32, m: Material, only_air: bool) {
        if r <= VS * 0.5 {
            self.line(a, b, m, only_air);
            return;
        }
        let len = (b - a).length();
        let n = ((len / (VS * 0.5)) as i32).max(1);
        for i in 0..=n {
            let p = a + (b - a) * (i as f32 / n as f32);
            self.w.fill_sphere(p, r, m, only_air);
        }
    }

    /// Quadratic Bézier tube with radius lerped `r0 → r1` (thin = 6-connected voxel polyline).
    #[allow(clippy::too_many_arguments)]
    pub fn curve(&mut self, a: Vec3, ctrl: Vec3, b: Vec3, r0: f32, r1: f32, m: Material, only_air: bool) {
        let len = (ctrl - a).length() + (b - ctrl).length();
        let n = ((len / 0.25) as i32).max(2);
        let bez = |t: f32| a * (1.0 - t) * (1.0 - t) + ctrl * 2.0 * t * (1.0 - t) + b * t * t;
        let mut prev = a;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let p = bez(t);
            let r = r0 + (r1 - r0) * t;
            if r <= VS * 0.5 {
                self.line(prev, p, m, only_air);
            } else {
                self.w.fill_sphere(p, r, m, only_air);
            }
            prev = p;
        }
    }

    // ----- ground -------------------------------------------------------------------------------

    /// Solid land over `rect`: indestructible bedrock at voxel y = 0, `body` up to the surface, and a
    /// surface voxel chosen per column by `top(x, z) -> (surface_y_m, material)`.
    pub fn land(&mut self, rect: Rect, body: Material, mut top: impl FnMut(f32, f32) -> (f32, Material)) {
        let (x0, x1) = (lo_idx(rect.min.x), hi_idx(rect.max.x));
        let (z0, z1) = (lo_idx(rect.min.y), hi_idx(rect.max.y));
        for z in z0..=z1 {
            for x in x0..=x1 {
                let p = vc(IVec3::new(x, 0, z));
                let (h, m) = top(p.x, p.z);
                let ty = vx(h + 0.01) - 1; // surface voxel
                self.w.fill_column(x, z, 0, 1, mat::TOWER_CORE);
                if ty >= 1 {
                    self.w.fill_column(x, z, 1, ty, body);
                    self.w.set(IVec3::new(x, ty, z), m);
                }
            }
        }
        // Collapse hints every 16 m along the bedrock.
        let step = (16.0 / VS) as i32;
        let mut z = z0;
        while z <= z1 {
            let mut x = x0;
            while x <= x1 {
                self.anchor(IVec3::new(x, 0, z));
                x += step;
            }
            z += step;
        }
    }

    /// Indestructible bedrock only (under ships, piers, islands built separately).
    pub fn bedrock(&mut self, rect: Rect) {
        self.fill(Vec3::new(rect.min.x, 0.0, rect.min.y), Vec3::new(rect.max.x, VS, rect.max.y), mat::TOWER_CORE);
        let c = rect.center();
        self.anchor(IVec3::new(vx(c.x), 0, vx(c.y)));
    }

    /// Top of the first non-foliage solid at or below `from_y` in the column containing (x, z).
    pub fn ground(&self, x: f32, z: f32, from_y: f32) -> Option<f32> {
        let (cx, cz) = (vx(x), vx(z));
        let lo = self.w.voxel_min().y;
        (lo..=vx(from_y)).rev().find_map(|y| {
            let m = self.w.get(IVec3::new(cx, y, cz));
            (m != AIR && !crate::arena::is_foliage(m)).then(|| (y + 1) as f32 * VS)
        })
    }

    /// Feet position of a spawn near (x, z) (standing on solid, non-foliage ground with head room),
    /// at least `min_dist` from every spawn in `taken`.
    pub fn spawn_near(&self, x: f32, z: f32, from_y: f32, taken: &[Vec3], min_dist: f32) -> Option<Vec3> {
        for ring in 0..14 {
            let steps = if ring == 0 { 1 } else { 8 };
            for k in 0..steps {
                let a = TAU * k as f32 / steps as f32;
                let (px, pz) = (x + dcos(a) * ring as f32 * 1.5, z + dsin(a) * ring as f32 * 1.5);
                if let Some(p) = crate::arena::spawn_ok(&self.w, px, pz, from_y)
                    && taken.iter().all(|t| t.distance(p) >= min_dist)
                {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Straight stairs from `a` (bottom, feet height) rising to `top_y`, heading along `dir` (unit,
    /// horizontal), `width` wide: 0.5 m risers every `run` metres (≥ 0.5; walkable with a 0.55 m step).
    /// Filled solid down to `a.y` so it's anchored. Returns the top landing's start point.
    pub fn stairs(&mut self, a: Vec3, dir: Vec2, top_y: f32, width: f32, run: f32, m: Material) -> Vec3 {
        let steps = ((top_y - a.y) / VS).round() as i32;
        let side = Vec2::new(-dir.y, dir.x);
        let run = run.max(VS);
        for s in 0..steps {
            let h = a.y + (s + 1) as f32 * VS;
            let c0 = Vec2::new(a.x, a.z) + dir * (s as f32 * run);
            let c1 = c0 + dir * run;
            let p0 = c0 - side * width * 0.5;
            let p1 = c1 + side * width * 0.5;
            let (lo, hi) = (p0.min(p1), p0.max(p1));
            self.fill(Vec3::new(lo.x, a.y - VS, lo.y), Vec3::new(hi.x, h, hi.y), m);
        }
        let e = Vec2::new(a.x, a.z) + dir * (steps as f32 * run);
        Vec3::new(e.x, top_y, e.y)
    }

    /// Finish generation: remove anything not connected to an indestructible voxel, mark all chunks
    /// dirty and hand back the world.
    pub fn finish(self) -> VoxelWorld {
        self.finish_counted().0
    }

    /// [`City::finish`] that also reports how many unanchored voxels were pruned.
    pub fn finish_counted(self) -> (VoxelWorld, usize) {
        let mut w = self.w;
        let n = w.prune_unanchored();
        w.mark_all_dirty();
        (w, n)
    }
}

/// Pick deterministically from a palette with a seed.
pub fn pick<T: Copy>(items: &[T], k: u32) -> T {
    items[(k as usize) % items.len()]
}
