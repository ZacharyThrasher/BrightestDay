//! **Orbit** — a ring of asteroid islands around a wrecked space station, in low orbit over a giant
//! planet (DESIGN.md "Maps: Orbit").
//!
//! Layout (metres; +X east, +Z north, Y up; everything floats over the void — falling below
//! [`VOID_Y`] is the re-entry blast zone, climbing out through the top is "launched into space"):
//!
//! ```text
//!                 Glacier (NW, low 28)          stone 90°        Crystal Spires (NE, high 52)
//!                        ·  150°: derelict truss (swing-only gap)        ·
//!   Dockyard (W, 42) ══catwalk══ HUB ROCK (40) + station hub ══rock bridge══ Regolith Flats (E, 36)
//!                        ·  ring of hull (r 30, top 53.5) · spine truss (y 58–61.5, x ±58) + solar wings
//!                 Comms Array (SW, high 50)   270°: derelict module (swing-only gap)   Crash Site (SE, low 26)
//! ```
//!
//! - **The station** (centre, the contested landmark): a hollow hub drum (r 7.5, 40 → 60 m) with four
//!   ground doors on the axes (the E–W lane runs straight through it), a mezzanine at 53.5 m opening
//!   onto four spokes out to a broken **hull ring** (r 30, a walkable running track in the sky, broken
//!   at E and W), the **spine truss** (x ±58 at 58–61.5 m, a narrow catwalk on top) carrying four
//!   **solar wings** (tilted panels over the E/W lanes: tether anchors, also landable), and the
//!   **antenna mast** (an indestructible lifeline up to 92 m — into the zero-g band).
//! - **The spine** (bhop lane, ~165 m): Regolith Flats (E) → sloped rock bridge → hub doors → steel
//!   catwalk → Dockyard (W).
//! - Six satellites at ~66 m alternating high/low: Flats (E, 36), Crystal Spires (NE, 52, needles to
//!   ~80 m), Glacier (NW, 28, ice shards + arch), Dockyard (W, 42, crane, habitat module, shuttle,
//!   containers), Comms Array (SW, 50, lattice tower to 80 m + dishes, overhanging shelf), Crash Site
//!   (SE, 26, scorched crater + a fallen solar wing ramp).
//! - Stepping stones at 30°/90°/210°/330°; the 150° and 270° gaps (~28 m) have **no** stepping stone —
//!   only a derelict wreck floating overhead: crossable only by swinging.
//! - Sky anchors: floating rocks at 60–68 m over the hub↔satellite gaps, a few outer ones.
//! - Gravity ([`GRAVITY`]): normal below 76 m (every walkable surface is well below), smoothly fading to
//!   ~0.08 g at 104 m. Tops of the spires, the comms tower and the mast poke into the band.
//! - Every rock has a small indestructible core; station parts carry slender structural cores
//!   ([`mat::TOWER_CORE`]) so wings, ring arcs and the mast can be shot apart but never vanish.

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, Gen, GravityProfile, polar, vx};
use crate::city::City;
use crate::material::mat;
use crate::noise::{PI, TAU, datan2, dcos, dsin, fbm2, smoothstep, value2, value3};
use crate::world::VoxelWorld;

/// World dimensions for Orbit, in voxels (192 × 112 × 192 m): the extra height pushes the top blast
/// zone up so a space KO still needs a real launch through the zero-g band.
pub const ORBIT_SIZE_VOXELS: IVec3 = IVec3::new(384, 224, 384);

/// Gravity by altitude: full on the islands, fading to near zero-g above the station.
pub const GRAVITY: GravityProfile = GravityProfile { full_below: 76.0, zero_above: 104.0, min_scale: 0.08 };

/// Falling below this is the re-entry burn-up KO.
pub const VOID_Y: f32 = 2.0;
/// Hub rock deck height (station floor).
pub const HUB_TOP: f32 = 40.0;
pub const HUB_R: f32 = 7.5;
/// Hub roof top (the roof slab is 60–61).
pub const HUB_ROOF: f32 = 60.0;
/// Mezzanine / spoke / ring walking height.
pub const RING_TOP: f32 = 53.5;
pub const RING_R: f32 = 30.0;
/// Spine truss: chords 58–61, catwalk plate on top at 61–61.5.
pub const SPINE_TOP: f32 = 61.5;
pub const SPINE_X: f32 = 58.0;
pub const MAST_TOP: f32 = 92.0;
/// Spoke directions (degrees; 0 = +X, 90 = +Z).
const SPOKES: [f32; 4] = [50.0, 130.0, 230.0, 310.0];
/// Ring arcs that survived (degrees): broken at E and W, over the lanes.
const RING_ARCS: [(f32, f32); 2] = [(16.0, 164.0), (196.0, 344.0)];

// ------------------------------------------------------------------------------------------------
// Asteroids

#[derive(Clone, Copy, Debug, PartialEq)]
enum Crust {
    /// Grey-tan dust with basalt patches.
    Regolith,
    /// Water ice with dusty patches.
    Ice,
    /// Bare dark rock with dust in the hollows.
    Basalt,
}

#[derive(Clone, Copy, Debug)]
struct Rock {
    center: Vec2,
    top: f32,
    rx: f32,
    rz: f32,
    rot: f32,
    depth: f32,
    hills: f32,
    slope: Vec2,
    /// Normalised radius inside which hills are suppressed (flat lanes / decks).
    flat: f32,
    crust: Crust,
    crystals: Material,
    core: bool,
    rim: f32,
    rim_start: f32,
    /// Extra noisy rock lumps hung under the island (asteroid silhouette, not a cone).
    lumps: u32,
}

impl Rock {
    fn new(center: Vec2, top: f32, r: f32) -> Self {
        Self {
            center,
            top,
            rx: r,
            rz: r,
            rot: 0.0,
            depth: r * 0.9,
            hills: 0.6,
            slope: Vec2::ZERO,
            flat: 0.0,
            crust: Crust::Regolith,
            crystals: mat::CRYSTAL_CYAN,
            core: true,
            rim: 1.2,
            rim_start: 0.8,
            lumps: 3,
        }
    }
}

/// A floating asteroid island: walkable crust on top, basalt body, lumpy underside with glowing
/// crystal / ice clusters, and an indestructible core.
fn rock(g: &mut Gen, s: Rock) {
    let seed = g.seed();
    let reach = s.rx.max(s.rz) * 1.2 + 1.0;
    let (x0, x1) = (vx(s.center.x - reach), vx(s.center.x + reach));
    let (z0, z1) = (vx(s.center.y - reach), vx(s.center.y + reach));
    let (sn, cs) = (dsin(s.rot), dcos(s.rot));
    for vz in z0..=z1 {
        for vxx in x0..=x1 {
            let p = Vec2::new((vxx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5);
            let q = p - s.center;
            let l = Vec2::new(q.x * cs + q.y * sn, -q.x * sn + q.y * cs);
            let e = ((l.x / s.rx) * (l.x / s.rx) + (l.y / s.rz) * (l.y / s.rz)).sqrt();
            let d = e + 0.18 * fbm2(p.x * 0.05, p.y * 0.05, seed, 3);
            if d >= 1.0 {
                continue;
            }
            let hill_mask = if s.flat > 0.0 { smoothstep(s.flat, s.flat + 0.35, d) } else { 1.0 };
            let hills = s.hills * fbm2(p.x * 0.035, p.y * 0.035, seed ^ 1, 3) * hill_mask;
            let rim = smoothstep(s.rim_start, 1.0, d) * s.rim;
            let top = s.top + s.slope.dot(q) + hills - rim;
            let jag = 0.5 + 0.5 * fbm2(p.x * 0.11, p.y * 0.11, seed ^ 2, 2);
            let od = 1.0 - d;
            let depth = 1.5 + s.depth * od * od.sqrt().sqrt() * (0.65 + 0.7 * jag);
            let ty = vx(top);
            let by = vx(top - depth);
            let patch = fbm2(p.x * 0.09, p.y * 0.09, seed ^ 3, 2);
            let dust = 1 + (jag * 3.0) as i32;
            let (surf, sub) = match s.crust {
                Crust::Regolith if patch > 0.38 => (mat::BASALT, mat::BASALT),
                Crust::Regolith => (mat::REGOLITH, mat::REGOLITH),
                Crust::Ice if patch > 0.42 => (mat::REGOLITH, mat::ICE),
                Crust::Ice => (mat::ICE, mat::ICE),
                Crust::Basalt if patch < -0.25 => (mat::REGOLITH, mat::BASALT),
                Crust::Basalt => (mat::BASALT, mat::BASALT),
            };
            let deep_split = by + ((ty - by) as f32 * 0.45) as i32;
            g.w.fill_column(vxx, vz, by, deep_split, mat::DEEP_STONE);
            g.w.fill_column(vxx, vz, deep_split, ty - dust, mat::BASALT);
            g.w.fill_column(vxx, vz, ty - dust, ty, sub);
            g.w.fill_column(vxx, vz, ty, ty + 1, surf);
            // Glowing crystal / ice clusters on the underside.
            let cn = fbm2(p.x * 0.16, p.y * 0.16, seed ^ 4, 2);
            if cn > 0.4 && d < 0.85 {
                let len = 1 + ((cn - 0.4) * 16.0) as i32;
                g.w.fill_column(vxx, vz, by - len / 2, by + 2, s.crystals);
            }
        }
    }
    // Lumps: noisy boulders fused to the underside — reads as a tumbling asteroid, not a cone.
    for k in 0..s.lumps {
        let a = TAU * (k as f32 + g.rng.range(0.0, 0.8)) / s.lumps.max(1) as f32;
        let rr = g.rng.range(0.15, 0.5);
        let off = Vec2::new(dcos(a) * s.rx * rr, dsin(a) * s.rz * rr);
        let under = 1.5 + s.depth * (1.0 - rr) * 0.7;
        let r = s.rx.min(s.rz) * g.rng.range(0.24, 0.36);
        let c = Vec3::new(s.center.x + off.x, s.top - under, s.center.y + off.y);
        let ry = r * g.rng.range(0.7, 1.0);
        g.blob(c, Vec3::new(r, ry, r), mat::BASALT);
    }
    if s.core {
        let thick = 1.5 + s.depth * 0.7;
        let r = (thick * 0.16).clamp(0.75, 2.5);
        let c = Vec3::new(s.center.x, s.top - thick * 0.5 - 0.5, s.center.y);
        g.cores.push((c, r));
    }
}

/// A shallow impact crater (bowl + raised rim) in whatever is at (x, z). Keep craters off the lanes.
#[allow(clippy::too_many_arguments)]
fn crater(g: &mut Gen, x: f32, z: f32, from_y: f32, r: f32, depth: f32, floor: Material) {
    let reach = r * 1.35;
    for vz in vx(z - reach)..=vx(z + reach) {
        for vxx in vx(x - reach)..=vx(x + reach) {
            let p = Vec2::new((vxx as f32 + 0.5) * 0.5, (vz as f32 + 0.5) * 0.5);
            let d = (p - Vec2::new(x, z)).length() / r;
            let Some(top) = g.ground(p.x, p.y, from_y) else { continue };
            let ty = vx(top - 0.01);
            if d < 1.0 {
                let bowl = depth * (1.0 - d * d);
                let ny = vx(top - bowl - 0.01);
                for y in (ny + 1)..=ty {
                    g.w.set(IVec3::new(vxx, y, vz), AIR);
                }
                if d < 0.75 {
                    g.w.set(IVec3::new(vxx, ny, vz), floor);
                }
            } else if d < 1.3 && (1.3 - d) > 0.12 * value2(p.x * 0.5, p.y * 0.5, 91) + 0.1 {
                g.w.set(IVec3::new(vxx, ty + 1, vz), mat::REGOLITH);
            }
        }
    }
}

/// A hexagonal crystal / ice shard rising from the ground at (x, z), leaning along `lean`.
#[allow(clippy::too_many_arguments)]
fn shard(g: &mut Gen, x: f32, z: f32, from_y: f32, height: f32, radius: f32, lean: Vec2, m: Material, glow: Material) -> Vec3 {
    let Some(gy) = g.ground(x, z, from_y) else { return Vec3::new(x, from_y, z) };
    let base = Vec2::new(x, z);
    let reach = radius + lean.length() + 1.0;
    let lo = IVec3::new(vx(x - reach), vx(gy - 2.0), vx(z - reach));
    let hi = IVec3::new(vx(x + reach), vx(gy + height), vx(z + reach));
    g.w.fill_with(lo, hi, |v, _| {
        let p = (v.as_vec3() + Vec3::splat(0.5)) * 0.5;
        let t = ((p.y - gy) / height).clamp(0.0, 1.0);
        let c = base + lean * t;
        let q = Vec2::new(p.x, p.z) - c;
        let hex = q.x.abs().max((q.x * 0.5 + q.y * 0.866).abs()).max((q.x * 0.5 - q.y * 0.866).abs());
        let r = radius * (1.0 - t * 0.92);
        if hex > r {
            return None;
        }
        Some(if hex < r * 0.35 { glow } else { m })
    });
    Vec3::new(x + lean.x, gy + height, z + lean.y)
}

// ------------------------------------------------------------------------------------------------
// Station kit (on the city builder)

/// Fill an oriented box: centre `o`, orthonormal axes `u`, `v` (third axis `u × v`), half extents.
/// `f(local, voxel) -> Some(material)`.
fn obox(c: &mut City, o: Vec3, u: Vec3, v: Vec3, half: Vec3, mut f: impl FnMut(Vec3, IVec3) -> Option<Material>) {
    let n = u.cross(v);
    let ext = u.abs() * half.x + v.abs() * half.y + n.abs() * half.z + Vec3::splat(0.5);
    c.fill_with(o - ext, o + ext, |p, vv, _| {
        let d = p - o;
        let l = Vec3::new(d.dot(u), d.dot(v), d.dot(n));
        if l.x.abs() > half.x || l.y.abs() > half.y || l.z.abs() > half.z {
            return None;
        }
        f(l, vv)
    });
}

/// Horizontal frame along `a → b`: (along, side, up).
fn frame_h(a: Vec3, b: Vec3) -> (Vec3, Vec3, Vec3, f32) {
    let d = b - a;
    let len = d.length();
    let u = d / len;
    let side = Vec3::Y.cross(u).normalize();
    let up = u.cross(side);
    (u, side, up, len)
}

/// Square lattice girder from `a` to `b` (4 chords at ±`half`, rings + alternating diagonals every
/// `step` metres) out of 6-connected voxel lines.
fn truss(c: &mut City, a: Vec3, b: Vec3, half: f32, step: f32, m: Material) {
    let d = b - a;
    let len = d.length();
    let u = d / len;
    let r = if u.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let s1 = r.cross(u).normalize();
    let s2 = u.cross(s1);
    let k = [s1 * half + s2 * half, -s1 * half + s2 * half, -s1 * half - s2 * half, s1 * half - s2 * half];
    for o in k {
        c.line(a + o, b + o, m, false);
    }
    let n = ((len / step).floor() as i32).max(1);
    for i in 0..=n {
        let p = a + u * (len * i as f32 / n as f32);
        for j in 0..4 {
            c.line(p + k[j], p + k[(j + 1) % 4], m, false);
        }
        if i < n {
            let q = a + u * (len * (i + 1) as f32 / n as f32);
            for j in 0..4 {
                let (e0, e1) = if i % 2 == 0 { (k[j], k[(j + 1) % 4]) } else { (k[(j + 1) % 4], k[j]) };
                c.line(p + e0, q + e1, m, false);
            }
        }
    }
}

/// A flat panel: corner `o`, orthonormal in-plane axes `u` (length `lu`) and `v` (length `lv`), one
/// voxel thick (watertight at any tilt), `cell` metre grid of `frame` over `fill`.
#[allow(clippy::too_many_arguments)]
fn plate(c: &mut City, o: Vec3, u: Vec3, v: Vec3, lu: f32, lv: f32, cell: f32, fill: Material, frame: Material) {
    let centre = o + u * (lu * 0.5) + v * (lv * 0.5);
    obox(c, centre, u, v, Vec3::new(lu * 0.5, lv * 0.5, 0.36), |l, _| {
        let (s, t) = (l.x + lu * 0.5, l.y + lv * 0.5);
        let edge = s < 0.5 || t < 0.5 || s > lu - 0.5 || t > lv - 0.5;
        let grid = s.rem_euclid(cell) < 0.5 || t.rem_euclid(cell * 1.5) < 0.5;
        Some(if edge || grid { frame } else { fill })
    });
}

/// A horizontal hull module (cylinder `a → b`, radius `r`): ribbed plating, a lit window strip, a flat
/// floor, hatches in both end caps and a few torn breaches.
fn module(c: &mut City, a: Vec3, b: Vec3, r: f32, breaches: f32) {
    let seed = c.seed();
    let (u, side, up, len) = frame_h(a, b);
    let o = (a + b) * 0.5;
    let floor = -r * 0.55;
    obox(c, o, u, side, Vec3::new(len * 0.5, r, r), |l, _| {
        let rho = (l.y * l.y + l.z * l.z).sqrt();
        if rho > r {
            return None;
        }
        let s = l.x + len * 0.5;
        let cap = s < 0.75 || s > len - 0.75;
        if !cap && rho <= r - 1.0 {
            return Some(if l.z < floor { mat::STEEL } else { AIR });
        }
        // Hatches in the end caps (floor level, 2.4 m wide, 2.6 m tall).
        if cap && rho <= r - 0.8 && l.y.abs() < 1.2 && l.z >= floor && l.z < floor + 2.6 {
            return Some(AIR);
        }
        let w = o + u * l.x + side * l.y + up * l.z;
        if !cap && l.z > floor + 0.5 && value3(w.x * 0.21, w.y * 0.21, w.z * 0.21, seed) > 0.62 - breaches {
            return Some(AIR);
        }
        if s.rem_euclid(3.0) < 0.5 || cap {
            return Some(mat::STEEL);
        }
        if l.z.abs() < 0.6 && l.y.abs() > r - 1.2 {
            return Some(if ((s * 2.0) as i32) % 4 == 0 { mat::STATION_LIGHT } else { mat::GLASS_BLUE });
        }
        Some(if l.z < floor { mat::HAZARD_ORANGE } else { mat::HULL_PANEL })
    });
}

/// A parabolic dish: vertex at `vertex`, opening along `axis`, rim `radius`, `depth` deep, with a feed
/// arm and a light at the focus.
fn dish(c: &mut City, vertex: Vec3, axis: Vec3, radius: f32, depth: f32) {
    let ax = axis.normalize();
    let r = if ax.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let s1 = r.cross(ax).normalize();
    let s2 = ax.cross(s1);
    obox(c, vertex + ax * (depth * 0.5), s1, s2, Vec3::new(radius, radius, depth * 0.5 + 0.6), |l, _| {
        let rho = (l.x * l.x + l.y * l.y).sqrt();
        if rho > radius {
            return None;
        }
        let h = l.z + depth * 0.5;
        let want = depth * (rho / radius) * (rho / radius);
        if (h - want).abs() > 0.42 {
            return None;
        }
        Some(if rho > radius - 0.6 { mat::STEEL } else { mat::HULL_PANEL })
    });
    let focus = vertex + ax * (depth * 1.7);
    c.line(vertex, focus, mat::STEEL, false);
    c.fill(focus - Vec3::splat(0.5), focus + Vec3::splat(0.5), mat::STATION_LIGHT);
}

/// A straight beam `a → b` (walkable deck / spoke): `half_w` wide each side, `half_h` tall, with
/// `edge` material on the top edges.
fn beam(c: &mut City, a: Vec3, b: Vec3, half_w: f32, half_h: f32, body: Material, top: Material, edge: Material) {
    let (u, side, _up, len) = frame_h(a, b);
    obox(c, (a + b) * 0.5, u, side, Vec3::new(len * 0.5, half_w, half_h), |l, _| {
        if l.z > half_h - 0.5 {
            return Some(if l.y.abs() > half_w - 0.5 { edge } else { top });
        }
        Some(body)
    });
}

/// The hub drum: hull wall with four axis doors, a window band, running lights, torn upper breaches,
/// spoke doorways at the mezzanine, a mezzanine ring floor, the roof slab and four core columns.
fn hub(c: &mut City) {
    let seed = c.seed();
    let (r, base, roof) = (HUB_R, HUB_TOP, HUB_ROOF);
    let mezz = RING_TOP - base;
    c.fill_with(Vec3::new(-r - 0.5, base - 1.0, -r - 0.5), Vec3::new(r + 0.5, roof + 1.0, r + 0.5), |p, _, _| {
        let d = (p.x * p.x + p.z * p.z).sqrt();
        if d > r {
            return None;
        }
        let rel = p.y - base;
        if rel < 0.0 {
            return Some(if d > r - 1.0 { mat::HAZARD_ORANGE } else { mat::STEEL });
        }
        if rel >= roof - base {
            // Roof slab with lit hatches.
            let hatch = (p.x.abs() - 3.5).abs() < 0.75 && (p.z.abs() - 3.5).abs() < 0.75;
            return Some(if hatch { mat::STATION_LIGHT } else { mat::HULL_PANEL });
        }
        let ang = (datan2(p.z, p.x) / TAU * 360.0).rem_euclid(360.0);
        if d <= r - 1.0 {
            // Interior: open, with the mezzanine ring (hole in the middle for tethers / drops).
            if (mezz - 0.5..mezz).contains(&rel) && d > 3.2 {
                return Some(if d > r - 1.5 { mat::HAZARD_ORANGE } else { mat::STEEL });
            }
            return Some(AIR);
        }
        // Ground doors on the four axes (the E–W lane runs straight through).
        if rel < 4.0 && (p.z.abs() < 2.5 || p.x.abs() < 2.5) {
            return Some(AIR);
        }
        // Spoke doorways off the mezzanine.
        if (mezz..mezz + 3.5).contains(&rel) && SPOKES.iter().any(|s| (ang - s).abs() < 8.0) {
            return Some(AIR);
        }
        // Torn breaches in the upper hull (the wreck).
        if rel > mezz + 3.6 && value3(p.x * 0.16, p.y * 0.16, p.z * 0.16, seed) > 0.34 {
            return Some(AIR);
        }
        if (ang / 15.0).rem_euclid(1.0) < 0.14 {
            return Some(mat::STEEL);
        }
        if (5.0..6.5).contains(&rel) {
            return Some(mat::GLASS_BLUE);
        }
        if (mezz + 3.5..mezz + 4.0).contains(&rel) || rel < 0.5 {
            return Some(mat::STATION_LIGHT);
        }
        Some(mat::HULL_PANEL)
    });
    // Four structural core columns inside (off the lane axes).
    for k in 0..4 {
        let a = PI * 0.25 + PI * 0.5 * k as f32;
        let q = Vec2::new(dcos(a), dsin(a)) * 5.0;
        c.fill(Vec3::new(q.x - 0.5, base - 3.0, q.y - 0.5), Vec3::new(q.x + 0.5, roof + 1.0, q.y + 0.5), mat::TOWER_CORE);
        c.anchor(IVec3::new(vx(q.x - 0.25), vx(base + 6.0), vx(q.y - 0.25)));
    }
}

/// The broken hull ring (a walkable track in the sky) with its four spokes to the mezzanine.
fn ring(c: &mut City) {
    let seed = c.seed();
    let centre = Vec3::new(0.0, RING_TOP - 1.5, 0.0);
    let (hw, hh) = (1.75, 1.5);
    let ext = RING_R + hw + 0.5;
    c.fill_with(centre - Vec3::new(ext, hh + 0.5, ext), centre + Vec3::new(ext, hh + 0.5, ext), |p, _, _| {
        let q = p - centre;
        let rad = (q.x * q.x + q.z * q.z).sqrt();
        let dr = rad - RING_R;
        if dr.abs() > hw || q.y.abs() > hh {
            return None;
        }
        let cr = 0.6;
        let (ex, ey) = ((dr.abs() - (hw - cr)).max(0.0), (q.y.abs() - (hh - cr)).max(0.0));
        if ex * ex + ey * ey > cr * cr {
            return None;
        }
        let ang = (datan2(q.z, q.x) / TAU * 360.0).rem_euclid(360.0);
        let jag = 3.0 + 3.0 * value2(q.y * 0.6, ang * 0.3, seed);
        if !RING_ARCS.iter().any(|&(a0, a1)| ang > a0 + jag && ang < a1 - jag) {
            return None;
        }
        let along = (ang / 360.0 * TAU * RING_R).rem_euclid(4.0);
        if q.y > hh - 0.5 {
            return Some(if dr.abs() > hw - 0.5 { mat::HAZARD_ORANGE } else { mat::HULL_PANEL });
        }
        if along < 0.5 {
            return Some(mat::STEEL);
        }
        if dr > hw - 0.6 && q.y.abs() < 0.6 {
            return Some(if along < 1.5 { mat::STATION_LIGHT } else { mat::GLASS_BLUE });
        }
        Some(mat::HULL_PANEL)
    });
    for a in SPOKES {
        let d = Vec3::new(dcos(a.to_radians()), 0.0, dsin(a.to_radians()));
        let (p0, p1) = (d * (HUB_R - 0.6) + Vec3::Y * (RING_TOP - 1.25), d * (RING_R - 1.0) + Vec3::Y * (RING_TOP - 1.25));
        beam(c, p0, p1, 1.25, 1.25, mat::STEEL, mat::HULL_PANEL, mat::HAZARD_ORANGE);
        // A core node where the spoke meets the ring: the arc survives losing its spoke.
        let node = d * RING_R + Vec3::Y * (RING_TOP - 1.5);
        c.fill(node - Vec3::splat(0.5), node + Vec3::splat(0.5), mat::TOWER_CORE);
        c.anchor(IVec3::new(vx(node.x), vx(node.y), vx(node.z)));
    }
}

/// The spine truss with its catwalk, A-frame legs down to the hub rock, and four solar wings.
fn spine(c: &mut City) {
    let (y0, y1) = (SPINE_TOP - 3.25, SPINE_TOP - 0.75);
    // Structural core along the spine (the wings can be shot off it, the spine never vanishes).
    c.fill(Vec3::new(-SPINE_X, y0 + 1.0, -0.5), Vec3::new(SPINE_X, y0 + 1.5, 0.0), mat::TOWER_CORE);
    let mut x = -SPINE_X + 2.0;
    while x < SPINE_X {
        c.anchor(IVec3::new(vx(x), vx(y0 + 1.25), vx(-0.25)));
        x += 14.0;
    }
    truss(c, Vec3::new(-SPINE_X, (y0 + y1) * 0.5, 0.0), Vec3::new(SPINE_X, (y0 + y1) * 0.5, 0.0), 1.25, 3.0, mat::STEEL);
    // Catwalk plate on top (2.5 m wide, orange edges).
    c.fill_with(Vec3::new(-SPINE_X, SPINE_TOP - 0.5, -1.25), Vec3::new(SPINE_X, SPINE_TOP, 1.25), |p, _, _| {
        Some(if p.z.abs() > 0.75 { mat::HAZARD_ORANGE } else { mat::MULLION })
    });
    // A-frame legs from the hub rock rim up to the spine.
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            let foot = Vec3::new(sx * 17.0, HUB_TOP - 3.0, sz * 7.0);
            let top = Vec3::new(sx * 25.0, y0, sz * 1.0);
            c.tube(foot, top, 0.6, mat::STEEL, false);
        }
    }
    // Solar wings: two tilted panels each side of each spine end, on short booms.
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            let tilt: f32 = 0.2;
            let v = Vec3::new(0.0, dsin(tilt), sz * dcos(tilt));
            let u = Vec3::X;
            let (x_in, x_out) = (sx * 40.0, sx * 56.0);
            let o = Vec3::new(x_in.min(x_out), y0 - 0.25, sz * 2.5);
            plate(c, o, u, v, 16.0, 11.0, 2.0, mat::SOLAR_CELL, mat::STEEL);
            // Booms from the truss to the panel's outer edge.
            for bx in [x_in, (x_in + x_out) * 0.5, x_out] {
                let tip = Vec3::new(bx, y0 - 0.25, sz * 2.5) + v * 10.5;
                c.line(Vec3::new(bx, y0, sz * 1.25), tip, mat::STEEL, false);
            }
        }
    }
    // Running lights at the spine tips.
    for sx in [-1.0f32, 1.0] {
        let p = Vec3::new(sx * (SPINE_X - 0.5), SPINE_TOP, 0.0);
        c.fill(p - Vec3::new(0.5, 0.0, 0.5), p + Vec3::new(0.5, 1.0, 0.5), mat::STATION_LIGHT);
    }
}

/// The antenna mast on the hub roof: an indestructible column up into the zero-g band (the lifeline
/// for recoveries), crossbars, a dish and a beacon.
fn mast(c: &mut City) {
    let base = HUB_ROOF + 1.0;
    c.fill(Vec3::new(-0.5, base - 1.0, -0.5), Vec3::new(0.5, MAST_TOP, 0.5), mat::TOWER_CORE);
    c.anchor(IVec3::new(vx(0.0), vx(base + 10.0), vx(0.0)));
    let mut y = base + 5.0;
    let mut k = 0;
    while y < MAST_TOP - 3.0 {
        let (a, b) = if k % 2 == 0 { (Vec3::new(-2.0, y, 0.0), Vec3::new(2.0, y, 0.0)) } else { (Vec3::new(0.0, y, -2.0), Vec3::new(0.0, y, 2.0)) };
        c.fill(a.min(b) - Vec3::new(0.25, 0.0, 0.25), a.max(b) + Vec3::new(0.25, 0.5, 0.25), mat::STEEL);
        y += 5.0;
        k += 1;
    }
    dish(c, Vec3::new(0.6, base + 16.0, 0.0), Vec3::new(1.0, 0.7, 0.3), 3.2, 1.2);
    c.fill(Vec3::new(-0.75, MAST_TOP, -0.75), Vec3::new(0.75, MAST_TOP + 1.0, 0.75), mat::STATION_LIGHT);
}

/// A floating derelict held by a small core (swing anchor over a swing-only gap).
fn derelict_core(c: &mut City, p: Vec3) {
    c.fill(p - Vec3::splat(1.5), p + Vec3::splat(1.5), mat::HULL_PANEL);
    c.fill(p - Vec3::splat(0.5), p + Vec3::splat(0.5), mat::TOWER_CORE);
    c.anchor(IVec3::new(vx(p.x), vx(p.y), vx(p.z)));
}

// ------------------------------------------------------------------------------------------------

/// Build Orbit. Deterministic for a given seed (the skeleton is fixed; details vary).
pub fn generate_orbit(seed: u64) -> (VoxelWorld, MapInfo) {
    let mut g = Gen::new(VoxelWorld::centered(ORBIT_SIZE_VOXELS), seed ^ 0x6f72_6269_7400);
    let j = |g: &mut Gen, amt: f32| Vec2::new(g.rng.range(-amt, amt), g.rng.range(-amt, amt));

    // ---- Hub rock ----------------------------------------------------------------------------
    rock(&mut g, Rock { rz: 22.0, depth: 22.0, hills: 0.4, flat: 0.5, lumps: 5, crystals: mat::CRYSTAL_VIOLET, ..Rock::new(Vec2::ZERO, HUB_TOP, 25.0) });

    // ---- Satellites ----------------------------------------------------------------------------
    // Regolith Flats (E): the long bhop lane.
    let flats_c = Vec2::new(68.0, 0.0) + Vec2::new(g.rng.range(-1.5, 1.5), 0.0);
    let flats_top = 36.0;
    rock(&mut g, Rock { rz: 13.0, depth: 15.0, hills: 0.5, flat: 0.25, lumps: 4, ..Rock::new(flats_c, flats_top, 22.0) });
    // Sloped rock bridge hub → flats.
    let b0 = Vec2::new(22.0, 0.0);
    let b1 = Vec2::new(flats_c.x - 18.0, 0.0);
    let bc = (b0 + b1) * 0.5;
    let bslope = (flats_top - HUB_TOP) / (b1.x - b0.x);
    rock(
        &mut g,
        Rock {
            rx: (b1.x - b0.x) * 0.5 + 5.0,
            rz: 4.8,
            depth: 5.0,
            hills: 0.2,
            slope: Vec2::new(bslope, 0.0),
            core: false,
            lumps: 0,
            flat: 0.9,
            ..Rock::new(bc, HUB_TOP + bslope * (bc.x - b0.x), 1.0)
        },
    );

    // Crystal Spires (NE, high).
    let spires_c = polar(60.0, 66.0) + j(&mut g, 2.0);
    let spires_top = 52.0;
    rock(&mut g, Rock { depth: 15.0, crust: Crust::Basalt, crystals: mat::CRYSTAL_VIOLET, ..Rock::new(spires_c, spires_top, 15.0) });

    // Glacier (NW, low).
    let glacier_c = polar(120.0, 66.0) + j(&mut g, 2.0);
    let glacier_top = 28.0;
    rock(&mut g, Rock { depth: 17.0, hills: 1.0, crust: Crust::Ice, crystals: mat::ICE, ..Rock::new(glacier_c, glacier_top, 20.0) });

    // Dockyard (W, mid).
    let dock_c = Vec2::new(-68.0, 0.0) + Vec2::new(g.rng.range(-1.5, 1.5), 0.0);
    let dock_top = 42.0;
    rock(&mut g, Rock { rx: 20.0, rz: 17.0, depth: 16.0, hills: 0.3, flat: 0.55, lumps: 4, ..Rock::new(dock_c, dock_top, 20.0) });

    // Comms Array (SW, high) + an overhanging shelf toward the hub.
    let comms_c = polar(240.0, 66.0) + j(&mut g, 2.0);
    let comms_top = 50.0;
    rock(&mut g, Rock { depth: 16.0, crust: Crust::Basalt, crystals: mat::CRYSTAL_GOLD, ..Rock::new(comms_c, comms_top, 15.0) });
    let to_hub = -comms_c.normalize();
    let shelf_c = comms_c + to_hub * 16.0;
    rock(
        &mut g,
        Rock {
            rx: 9.0,
            rz: 4.0,
            rot: datan2(to_hub.y, to_hub.x),
            depth: 2.5,
            hills: 0.1,
            crust: Crust::Basalt,
            core: false,
            lumps: 0,
            ..Rock::new(shelf_c, comms_top - 1.0, 1.0)
        },
    );

    // Crash Site (SE, low).
    let crash_c = polar(300.0, 66.0) + j(&mut g, 2.0);
    let crash_top = 26.0;
    rock(&mut g, Rock { depth: 15.0, hills: 0.7, flat: 0.3, ..Rock::new(crash_c, crash_top, 20.0) });

    // ---- Stepping stones (none at 150° / 270°: swing-only gaps) --------------------------------
    let steps: [(f32, f32, f32, f32); 4] = [(30.0, 64.0, 44.0, 5.5), (90.0, 80.0, 40.0, 7.0), (210.0, 64.0, 46.0, 5.5), (330.0, 64.0, 31.0, 5.5)];
    let mut step_pos = Vec::new();
    for (i, &(ang, r, top, rad)) in steps.iter().enumerate() {
        let c = polar(ang, r) + j(&mut g, 1.2);
        let top = top + g.rng.range(-1.0, 1.0);
        let crystals = [mat::CRYSTAL_CYAN, mat::ICE, mat::CRYSTAL_GOLD, mat::CRYSTAL_VIOLET][i % 4];
        rock(&mut g, Rock { depth: rad * 1.4, hills: 0.2, crystals, lumps: 2, ..Rock::new(c, top, rad) });
        step_pos.push((c, top));
    }

    // ---- Sky anchors: floating rocks over the hub ↔ satellite gaps + a few outer ones ------------
    for k in 0..6 {
        let ang = 30.0 + 60.0 * k as f32 + g.rng.range(-6.0, 6.0);
        let c = polar(ang, 42.0 + g.rng.range(-2.0, 2.0));
        let top = g.rng.range(62.0, 68.0);
        let r = g.rng.range(3.0, 4.2);
        let crystals = if k % 2 == 0 { mat::CRYSTAL_CYAN } else { mat::ICE };
        rock(&mut g, Rock { depth: r * 1.8, hills: 0.2, crust: Crust::Basalt, crystals, lumps: 1, ..Rock::new(c, top, r) });
    }
    for k in 0..3 {
        let ang = 120.0 * k as f32 + 5.0 + g.rng.range(-8.0, 8.0);
        let c = polar(ang, 86.0 + g.rng.range(-2.0, 1.0));
        let top = g.rng.range(64.0, 72.0);
        let r = g.rng.range(3.5, 5.0);
        rock(&mut g, Rock { depth: r * 2.0, hills: 0.2, crust: Crust::Basalt, crystals: mat::CRYSTAL_GOLD, lumps: 2, ..Rock::new(c, top, r) });
    }

    // ---- Natural details ---------------------------------------------------------------------
    // Flats: flank craters + basalt boulders; the centre lane (|z| < 4) stays smooth.
    for k in 0..4 {
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let x = flats_c.x - 14.0 + 9.0 * k as f32 + g.rng.range(-1.5, 1.5);
        let z = side * g.rng.range(8.0, 10.0);
        let r = g.rng.range(2.5, 3.5);
        crater(&mut g, x, z, flats_top + 4.0, r, 0.9, mat::BASALT);
    }
    for _ in 0..5 {
        let p = flats_c + Vec2::new(g.rng.range(-17.0, 17.0), g.rng.range(-10.0, 10.0));
        if p.y.abs() > 5.0 {
            let r = g.rng.range(1.0, 1.8);
            boulder(&mut g, p.x, p.y, flats_top + 8.0, r);
        }
    }
    shard(&mut g, flats_c.x + 16.0, flats_c.y + 8.0, flats_top + 6.0, 9.0, 1.6, Vec2::new(1.0, 1.5), mat::ICE, mat::CRYSTAL_CYAN);
    shard(&mut g, flats_c.x + 18.0, flats_c.y + 5.5, flats_top + 6.0, 5.0, 1.0, Vec2::new(1.5, 0.5), mat::ICE, mat::CRYSTAL_CYAN);
    // Hub rock: rubble around the station (off the lanes).
    for _ in 0..5 {
        let p = polar(g.rng.range(0.0, 360.0), g.rng.range(11.0, 20.0));
        if p.y.abs() > 4.5 && p.x.abs() > 4.5 {
            let r = g.rng.range(1.0, 1.8);
            boulder(&mut g, p.x, p.y, HUB_TOP + 8.0, r);
        }
    }

    // Crystal Spires: tall needles, tips reaching into the gravity band.
    for k in 0..4 {
        let a = 45.0 + 90.0 * k as f32 + g.rng.range(-18.0, 18.0);
        let p = spires_c + polar(a, g.rng.range(5.0, 9.0));
        let tip = [mat::CRYSTAL_CYAN, mat::CRYSTAL_VIOLET, mat::ICE, mat::CRYSTAL_VIOLET][k];
        let h = g.rng.range(20.0, 28.0);
        let r = g.rng.range(1.6, 2.3);
        g.spire(p.x, p.y, spires_top + 8.0, h, r, Some(tip));
    }
    boulder(&mut g, spires_c.x, spires_c.y, spires_top + 8.0, 1.5);

    // Glacier: a ring of ice shards + an ice arch across the island.
    for k in 0..6 {
        let a = 30.0 + 60.0 * k as f32 + g.rng.range(-12.0, 12.0);
        let p = glacier_c + polar(a, g.rng.range(9.0, 14.0));
        let h = if k % 2 == 0 { g.rng.range(10.0, 16.0) } else { g.rng.range(5.0, 8.0) };
        let lean = (p - glacier_c).normalize() * g.rng.range(1.0, 3.0);
        let r = g.rng.range(1.4, 2.4);
        shard(&mut g, p.x, p.y, glacier_top + 8.0, h, r, lean, mat::ICE, mat::CRYSTAL_CYAN);
    }
    crater(&mut g, glacier_c.x + 4.0, glacier_c.y - 3.0, glacier_top + 4.0, 4.0, 1.0, mat::ICE);

    // Crash site: the impact crater.
    crater(&mut g, crash_c.x, crash_c.y, crash_top + 4.0, 7.0, 1.6, mat::SCORCHED);

    // Stepping stone N carries a tall ice needle (an anchor between Spires and Glacier).
    let (n_c, n_top) = step_pos[1];
    shard(&mut g, n_c.x, n_c.y, n_top + 6.0, 18.0, 1.6, Vec2::new(0.0, -1.0), mat::ICE, mat::CRYSTAL_CYAN);

    g.place_cores();

    // ---- Station (city kit brushes) --------------------------------------------------------------
    let mut c = City::new(g.w, seed ^ 0x0057_a710);
    hub(&mut c);
    ring(&mut c);
    spine(&mut c);
    mast(&mut c);

    // West lane: a steel catwalk from the hub rock to the Dockyard (railings, under-truss).
    let cw_a = Vec3::new(-19.0, c.ground(-19.0, 0.0, HUB_TOP + 3.0).unwrap_or(HUB_TOP), 0.0);
    let cw_bx = dock_c.x + 17.0;
    let cw_b = Vec3::new(cw_bx, c.ground(cw_bx, 0.0, dock_top + 3.0).unwrap_or(dock_top), 0.0);
    beam(&mut c, cw_a - Vec3::Y * 0.5, cw_b - Vec3::Y * 0.5, 3.0, 0.5, mat::STEEL, mat::MULLION, mat::HAZARD_ORANGE);
    {
        let (u, _, _, len) = frame_h(cw_a, cw_b);
        truss(&mut c, cw_a - Vec3::Y * 2.5 + u * 2.0, cw_b - Vec3::Y * 2.5 - u * 2.0, 1.5, 3.0, mat::STEEL);
        let mut s = 2.0;
        while s < len - 1.0 {
            for sz in [-2.75f32, 2.75] {
                let p = cw_a + u * s + Vec3::new(0.0, 0.0, sz);
                c.fill(Vec3::new(p.x - 0.25, p.y, sz - 0.25), Vec3::new(p.x + 0.25, p.y + 1.5, sz + 0.25), mat::STEEL);
            }
            s += 3.0;
        }
        for sz in [-2.75f32, 2.75] {
            c.line(cw_a + u * 2.0 + Vec3::new(0.0, 1.25, sz), cw_a + u * (s - 3.0) + Vec3::new(0.0, 1.25, sz), mat::HAZARD_ORANGE, false);
        }
    }

    // Dockyard: habitat module (S), gantry crane + shuttle (N), container stacks.
    let dg = |c: &City, x: f32, z: f32| c.ground(x, z, dock_top + 4.0).unwrap_or(dock_top);
    let hab_y = dg(&c, dock_c.x, dock_c.y - 9.0) + 2.6;
    module(&mut c, Vec3::new(dock_c.x - 12.0, hab_y, dock_c.y - 9.5), Vec3::new(dock_c.x + 8.0, hab_y, dock_c.y - 9.0), 3.5, 0.08);
    let crane_ground = dg(&c, dock_c.x - 12.0, dock_c.y + 9.0).min(dg(&c, dock_c.x - 5.0, dock_c.y + 9.0));
    c.gantry_crane(dock_c.x - 14.0, dock_c.x - 6.0, dock_c.y + 9.0, crane_ground, crane_ground + 15.0, dock_c.x + 14.0, mat::HAZARD_ORANGE);
    let sh_y = dg(&c, dock_c.x + 1.0, dock_c.y + 9.0) + 2.0;
    module(&mut c, Vec3::new(dock_c.x - 4.0, sh_y, dock_c.y + 9.0), Vec3::new(dock_c.x + 7.0, sh_y, dock_c.y + 9.0), 2.2, 0.0);
    plate(&mut c, Vec3::new(dock_c.x - 1.0, sh_y - 0.5, dock_c.y + 3.0), Vec3::X, Vec3::Z, 5.0, 12.0, 2.5, mat::HULL_PANEL, mat::HAZARD_ORANGE);
    let cols = [mat::HULL_WHITE, mat::HAZARD_ORANGE, mat::HULL_NAVY, mat::PAINT_RED, mat::STUCCO_SKY];
    let stacks: [(f32, f32, i32); 4] = [(-18.0, -3.0, 2), (-18.0, 4.0, 1), (10.0, 4.0, 1), (9.0, -9.5, 2)];
    for (i, &(dx, dz, n)) in stacks.iter().enumerate() {
        let (x, z) = (dock_c.x + dx, dock_c.y + dz);
        let base = dg(&c, x + 1.25, z + 1.25);
        for k in 0..n {
            let col = cols[(i * 2 + k as usize + seed as usize) % cols.len()];
            c.container(Vec3::new(x, base - 0.5 + k as f32 * 2.5, z), false, col);
        }
    }

    // Comms Array: a lattice tower into the gravity band, two dishes, a platform.
    let cg = c.ground(comms_c.x, comms_c.y, comms_top + 6.0).unwrap_or(comms_top);
    let tower_base = Vec3::new(comms_c.x, cg - 1.0, comms_c.y) - Vec3::new(to_hub.x, 0.0, to_hub.y) * 3.0;
    let tower_top = Vec3::new(tower_base.x, 80.0, tower_base.z);
    truss(&mut c, tower_base, tower_top, 1.5, 3.0, mat::STEEL);
    c.fill(tower_base - Vec3::new(0.25, 0.0, 0.25), tower_top + Vec3::new(0.25, 0.0, 0.25), mat::TOWER_CORE);
    c.anchor(IVec3::new(vx(tower_base.x), vx(cg + 4.0), vx(tower_base.z)));
    c.fill(tower_top + Vec3::new(-0.75, 0.0, -0.75), tower_top + Vec3::new(0.75, 1.0, 0.75), mat::STATION_LIGHT);
    let hub_dir = Vec3::new(to_hub.x, 0.35, to_hub.y);
    dish(&mut c, tower_base + Vec3::Y * 16.0 + hub_dir.normalize() * 1.6, hub_dir, 4.0, 1.4);
    dish(&mut c, tower_base + Vec3::new(-to_hub.x * 1.6, 24.0, -to_hub.y * 1.6), Vec3::new(-to_hub.x, 1.6, -to_hub.y), 3.0, 1.0);
    plate(&mut c, tower_base + Vec3::new(-3.0, 20.0, -3.0), Vec3::X, Vec3::Z, 6.0, 6.0, 1.5, mat::MULLION, mat::HAZARD_ORANGE);

    // Crash Site: a fallen solar wing leaning out of the crater (a launch ramp toward the hub) and
    // scattered hull wreckage.
    let to_hub_c = -crash_c.normalize();
    let ramp_u = Vec3::new(-to_hub_c.y, 0.0, to_hub_c.x);
    let ramp_v = (Vec3::new(to_hub_c.x, 0.0, to_hub_c.y) * dcos(0.42) + Vec3::Y * dsin(0.42)).normalize();
    let ramp_foot = Vec3::new(crash_c.x, crash_top - 1.6, crash_c.y) - Vec3::new(to_hub_c.x, 0.0, to_hub_c.y) * 3.0 - ramp_u * 5.0;
    plate(&mut c, ramp_foot, ramp_u, ramp_v, 10.0, 17.0, 2.0, mat::SOLAR_CELL, mat::STEEL);
    let crash_g = c.ground(crash_c.x + 9.0, crash_c.y + 7.0, crash_top + 6.0).unwrap_or(crash_top);
    module(&mut c, Vec3::new(crash_c.x + 5.0, crash_g + 1.6, crash_c.y + 9.0), Vec3::new(crash_c.x + 14.0, crash_g + 2.4, crash_c.y + 4.0), 3.0, 0.2);
    let tg = c.ground(crash_c.x - 11.0, crash_c.y - 6.0, crash_top + 6.0).unwrap_or(crash_top);
    truss(&mut c, Vec3::new(crash_c.x - 15.0, tg + 1.2, crash_c.y - 3.0), Vec3::new(crash_c.x - 6.0, tg + 1.2, crash_c.y - 11.0), 1.2, 3.0, mat::SCORCHED);

    // Swing-only gaps: derelicts floating over the 150° and 270° gaps.
    let d150 = polar(150.0, 70.0);
    let d150y = 58.0;
    derelict_core(&mut c, Vec3::new(d150.x, d150y, d150.y));
    truss(&mut c, Vec3::new(d150.x - 7.0, d150y, d150.y - 3.0), Vec3::new(d150.x + 7.0, d150y, d150.y + 3.0), 1.25, 3.0, mat::STEEL);
    plate(&mut c, Vec3::new(d150.x - 5.0, d150y - 1.5, d150.y - 1.0), Vec3::X, Vec3::Z, 9.0, 7.0, 2.0, mat::SOLAR_CELL, mat::STEEL);
    let d270 = polar(270.0, 70.0);
    let d270y = 57.0;
    module(&mut c, Vec3::new(d270.x - 7.0, d270y, d270.y + 1.0), Vec3::new(d270.x + 7.0, d270y + 1.0, d270.y - 1.0), 3.0, 0.15);
    derelict_core(&mut c, Vec3::new(d270.x, d270y - 2.4, d270.y));

    // ---- Spawns: two per big satellite, one per high satellite, facing inwards ----------------
    let spawn_targets: [(Vec2, f32); 10] = [
        (flats_c + Vec2::new(12.0, 6.0), flats_top),
        (flats_c + Vec2::new(-2.0, -7.0), flats_top),
        (spires_c + spires_c.normalize() * 3.0, spires_top),
        (glacier_c + Vec2::new(3.0, 2.0), glacier_top),
        (glacier_c + glacier_c.normalize() * 6.0 + Vec2::new(-5.0, 0.0), glacier_top),
        (dock_c + Vec2::new(-4.0, 1.0), dock_top),
        (dock_c + Vec2::new(12.0, -1.5), dock_top),
        (comms_c + comms_c.normalize() * 5.0, comms_top),
        (crash_c + crash_c.normalize() * 11.0, crash_top),
        (crash_c + Vec2::new(-9.0, 7.0), crash_top),
    ];
    let mut spawns: Vec<Vec3> = Vec::new();
    for (p, top) in spawn_targets {
        if let Some(s) = c.spawn_near(p.x, p.y, top + 6.0, &spawns, 12.0) {
            spawns.push(s);
        }
    }

    let (world, pruned) = c.finish_counted();
    let wb = world.bounds();
    let center = Vec3::new(0.0, HUB_TOP, 0.0);
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb {
            min: Vec3::new(wb.min.x - 24.0, VOID_Y, wb.min.z - 24.0),
            max: Vec3::new(wb.max.x + 24.0, wb.max.y + 40.0, wb.max.z + 24.0),
        },
        void_y: VOID_Y,
        center,
        // A low, hard sun: long shadows across the rocks, a sharp terminator on the planet.
        sun_dir: Vec3::new(-0.62, 0.52, 0.42).normalize(),
        sky_zenith: [0.004, 0.006, 0.02],
        sky_horizon: [0.20, 0.42, 0.85],
        fog_color: [0.02, 0.035, 0.08],
        ambient: 0.34,
        gravity: Some(GRAVITY),
    };
    let districts = vec![
        District { name: "station", center, area: [-HUB_R, -HUB_R, HUB_R, HUB_R] },
        District { name: "hub_rock", center: Vec3::new(0.0, HUB_TOP, 14.0), area: [-25.0, -22.0, 25.0, 22.0] },
        District { name: "flats", center: Vec3::new(flats_c.x, flats_top, flats_c.y), area: [flats_c.x - 22.0, -13.0, flats_c.x + 22.0, 13.0] },
        district("spires", spires_c, spires_top, 15.0),
        district("glacier", glacier_c, glacier_top, 20.0),
        District { name: "dockyard", center: Vec3::new(dock_c.x, dock_top, dock_c.y), area: [dock_c.x - 20.0, -17.0, dock_c.x + 20.0, 17.0] },
        district("comms", comms_c, comms_top, 15.0),
        district("crash_site", crash_c, crash_top, 20.0),
    ];
    let routes = routes(flats_c, dock_c, comms_c, crash_c, cw_a, cw_b, Vec3::new(d270.x, d270y, d270.y), tower_top);
    let info = MapInfo {
        name: "Orbit",
        arena,
        // No sea: the "water" plane is far below everything (the client draws the planet instead).
        water_y: -1000.0,
        water_color: [0.10, 0.30, 0.62],
        water_deep: [0.02, 0.06, 0.18],
        sun_color: [1.0, 0.97, 0.92],
        sky_mid: [0.02, 0.05, 0.14],
        fog_start: 260.0,
        fog_end: 1400.0,
        neon_boost: 1.2,
        districts,
        routes,
        pruned_voxels: pruned,
    };
    (world, info)
}

fn district(name: &'static str, c: Vec2, top: f32, r: f32) -> District {
    District { name, center: Vec3::new(c.x, top, c.y), area: [c.x - r, c.y - r, c.x + r, c.y + r] }
}

/// Basalt boulder with a dusty top (cover).
fn boulder(g: &mut Gen, x: f32, z: f32, from_y: f32, r: f32) {
    let Some(gy) = g.ground(x, z, from_y) else { return };
    let seed = g.seed();
    let c = Vec3::new(x, gy + r * 0.3, z);
    let rad = Vec3::new(r * g.rng.range(0.9, 1.3), r * 0.8, r * g.rng.range(0.9, 1.3));
    let lo = ((c - rad * 1.3) / 0.5).floor().as_ivec3();
    let hi = ((c + rad * 1.3) / 0.5).floor().as_ivec3();
    g.w.fill_with(lo, hi, |v, _| {
        let p = (v.as_vec3() + Vec3::splat(0.5)) * 0.5;
        let e = ((p - c) / rad).length() + 0.22 * value3(p.x * 0.4, p.y * 0.4, p.z * 0.4, seed);
        if e >= 1.0 {
            return None;
        }
        let dusty = (p.y - c.y) > rad.y * 0.45 && value2(p.x * 0.5, p.z * 0.5, seed ^ 9) > 0.0;
        Some(if dusty { mat::REGOLITH } else { mat::BASALT })
    });
}

#[allow(clippy::too_many_arguments)]
fn routes(flats_c: Vec2, dock_c: Vec2, comms_c: Vec2, crash_c: Vec2, cw_a: Vec3, cw_b: Vec3, derelict: Vec3, comms_tower: Vec3) -> Vec<Route> {
    let to_c = |c: Vec2, top: f32, toward: Vec2, d: f32| {
        let dir = (toward - c).normalize_or_zero();
        Vec3::new(c.x + dir.x * d, top, c.y + dir.y * d)
    };
    vec![
        Route {
            name: "spine_bhop",
            from: "flats",
            to: "dockyard",
            kind: RouteKind::Ground,
            waypoints: vec![
                Vec3::new(flats_c.x + 17.0, 36.0, 0.0),
                Vec3::new(HUB_R + 1.0, HUB_TOP, 0.0),
                Vec3::new(-HUB_R - 1.0, HUB_TOP, 0.0),
                cw_a,
                cw_b,
                Vec3::new(dock_c.x - 8.0, 42.0, 0.0),
            ],
            anchors: vec![Vec3::new(40.0, SPINE_TOP - 3.5, 0.0), Vec3::new(-40.0, SPINE_TOP - 3.5, 0.0)],
        },
        Route {
            name: "derelict_swing",
            from: "crash_site",
            to: "comms",
            kind: RouteKind::Swing,
            waypoints: vec![to_c(crash_c, 26.0, comms_c, 14.0), to_c(comms_c, 50.0, crash_c, 9.0)],
            anchors: vec![derelict - Vec3::Y * 3.0, comms_tower - Vec3::Y * 6.0],
        },
        Route {
            name: "ring_run",
            from: "station",
            to: "station",
            kind: RouteKind::Ground,
            waypoints: (0..=8).map(|k| {
                let a = (20.0 + 17.0 * k as f32).to_radians();
                Vec3::new(dcos(a) * RING_R, RING_TOP, dsin(a) * RING_R)
            }).collect(),
            anchors: vec![],
        },
        Route {
            name: "zero_g_recovery",
            from: "station",
            to: "station",
            kind: RouteKind::Mixed,
            waypoints: vec![Vec3::new(0.0, HUB_ROOF + 1.0, 4.5), Vec3::new(0.0, HUB_ROOF + 1.0, 4.5)],
            anchors: vec![Vec3::new(0.0, MAST_TOP - 6.0, 0.0)],
        },
    ]
}
