//! **Mirrorfold** — a city folded over itself (feature `mirror`; DESIGN.md "Maps"). Private-use
//! homage to the folding-city dream / mirror-dimension look (see THIRD_PARTY_NOTICES); our own name,
//! layout and art. Gravity is normal everywhere: the impossibility is all geometry.
//!
//! Layout (metres; +X east, +Z north, Y up; street level [`STREET_Y`] = 14):
//!
//! ```text
//!            ╭──────────────────── curl (streets bend up into a half-pipe) ───────────────────╮
//!            │  ring road ─────────────────────────────────────────────────────────  ring road │
//!            │     NW brownstones (fire escape)  │ N ave │  NE library (open loggia)          │
//!            │        [clock fragment ▲ spawn]   │       │                                     │
//!            │  W ave ════════════ PLAZA (kaleidoscope inlay, fracture → void) ═══════ E ave  │
//!            │        [street island ▲ spawn]    │       │                                     │
//!            │     SW glass tower (sky lobby)    │ S ave │  SE bank (grand stair, roof)        │
//!            ╰──────────────────────────────────────────────────────────────────────────────────╯
//!              six towers hang upside-down from the sky above the blocks (bottoms 58–64 m)
//! ```
//!
//! - **The curl.** The inner city is a rounded rectangle ([`HALF_X`] × [`HALF_Z`], corner radius
//!   [`CORNER_R`]) ringed by an 8 m ring road; past its edge the street bends up a circular arc of
//!   radius [`CURL_R`] — asphalt, then bent facades — through vertical and on to [`LIP_DEG`] (an
//!   overhang ~33 m up, fringed with buildings sticking out of it at impossible angles). The part you
//!   ride is **smooth hard light**: per straight side and per 15° corner segment, four chords of the
//!   arc are [`SurfLine`]s (one walkable ~24° ramp, then ~48°, ~60°, ~72° surf faces), so you can
//!   carve the whole loop around the arena; the voxels underneath are only the look.
//! - **Inverted city.** Six towers hang from the world top ([`TOP_Y`]) with indestructible spines and
//!   spire tips: tether anchors everywhere, swing ring around the plaza, star KO through the open
//!   middle.
//! - **Fragments.** Kaleidoscope-cut chunks float between the roofs and the hanging towers: two
//!   spawn islands (a street chunk, a clock face) reached by stepping-stone cornices, floating stair
//!   flights up to the loggia / sky lobby, sideways fire escapes (wall-kick lattices), cornice shards,
//!   the library's missing corner, and two broken gable roofs whose ridges are descending surf chutes.
//! - **KOs.** Fractures in the plaza and two ring-road corners drop into the mirrored void (bottom
//!   blast zone [`VOID_Y`]); the side blast zones sit just behind the curl (punch through it and
//!   you're gone); star KO far above. The tether save applies as everywhere.
//! - **Perf.** Hollow buildings with sealed floors, a thin 1 m curl shell, no point lights (lit
//!   windows + seams are emissive voxels). The folded horizon beyond is a client backdrop
//!   (`present::mirror`), not voxels.

#![allow(clippy::too_many_arguments)]

use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Mat3, Vec2, Vec3};

use super::{District, MapInfo, Route, RouteKind};
use crate::arena::{ArenaInfo, SurfLine};
use crate::city::{CarSpec, City, Crown, Rect, TowerSpec, vc};
use crate::material::mat;
use crate::noise::{PI, Rng, datan2, dcos, dsin, hash3, unit};
use crate::world::VoxelWorld;

/// World dimensions in voxels (192 × 112 × 160 m).
pub const MIRROR_SIZE_VOXELS: IVec3 = IVec3::new(384, 224, 320);
/// Street (ring road, avenues) surface height.
pub const STREET_Y: f32 = 14.0;
/// Sidewalk / plaza / building base height.
pub const WALK_Y: f32 = 14.5;
/// Inner city half extents (where the curl starts) and corner radius.
pub const HALF_X: f32 = 58.0;
pub const HALF_Z: f32 = 42.0;
pub const CORNER_R: f32 = 18.0;
/// Radius of the curl's arc (street → vertical at `CURL_R` up / out).
pub const CURL_R: f32 = 22.0;
/// Where the overhang ends (degrees from the street).
pub const LIP_DEG: f32 = 120.0;
/// Curl shell thickness (m).
const SHELL: f32 = 1.0;
/// Ring road width inside the curl, and the sidewalk inside it.
pub const RING_W: f32 = 8.0;
const SIDEWALK: f32 = 3.0;
/// Falling below this is the void KO (the fractures are open to the world floor at 0).
pub const VOID_Y: f32 = 5.0;
/// World top: the inverted towers hang from here.
pub const TOP_Y: f32 = 112.0;
/// The central plaza (x, z).
pub const PLAZA: Rect = Rect::new(-16.0, -10.0, 16.0, 10.0);
/// Avenue half width (E–W along z = 0, N–S along x = 0).
pub const AVE_HALF: f32 = 5.0;
/// Corner segments per rounded corner (each its own set of surf lines, 15° apart).
pub const CORNER_SEGS: i32 = 6;
/// Curl chord breaks in degrees: `[0, 42]` is the walkable ramp, then three surf faces.
pub const CURL_BREAKS: [f32; 5] = [0.0, 42.0, 54.0, 66.0, 78.0];
/// Hard light floats this far in front of the voxel arc (no voxel stair ever pokes through).
const INSET: f32 = 0.6;
/// Straight half-lengths (x of the N/S sides, z of the E/W sides).
const CX: f32 = HALF_X - CORNER_R;
const CZ: f32 = HALF_Z - CORNER_R;

#[inline]
fn rad(d: f32) -> f32 {
    d * (PI / 180.0)
}

// ------------------------------------------------------------------------------------------------
// Perimeter frame

/// Where a horizontal point sits relative to the curl.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Perim {
    /// Signed distance outward from the inner city edge (negative inside).
    pub s: f32,
    /// Panel: 0 E, 1 N, 2 W, 3 S straights; `4 + corner·CORNER_SEGS + k` corner segments (corners
    /// 0 NE, 1 NW, 2 SW, 3 SE).
    pub panel: i32,
    /// Along-perimeter coordinate: metres along a straight, radians from the segment start on a corner.
    pub w: f32,
    /// Metres per unit of `w` at this point (1 on straights, the radius on corners).
    pub w_scale: f32,
    /// Outward unit normal (x, z).
    pub normal: Vec2,
}

/// Perimeter frame of (x, z).
pub fn perim(x: f32, z: f32) -> Perim {
    let (qx, qz) = (x.abs() - CX, z.abs() - CZ);
    if qx > 0.0 && qz > 0.0 {
        let r = (qx * qx + qz * qz).sqrt();
        let corner = match (x > 0.0, z > 0.0) {
            (true, true) => 0,
            (false, true) => 1,
            (false, false) => 2,
            (true, false) => 3,
        };
        // Global angle around the corner centre, counter-clockwise from +X.
        let psi = datan2(z - z.signum() * CZ, x - x.signum() * CX);
        let psi = if psi < 0.0 { psi + 2.0 * PI } else { psi };
        let local = (psi - corner as f32 * PI * 0.5).clamp(0.0, PI * 0.5);
        let seg = PI * 0.5 / CORNER_SEGS as f32;
        let k = ((local / seg) as i32).clamp(0, CORNER_SEGS - 1);
        Perim {
            s: r - CORNER_R,
            panel: 4 + corner * CORNER_SEGS + k,
            w: local - k as f32 * seg,
            w_scale: r.max(1.0),
            normal: Vec2::new(x.signum() * qx / r, z.signum() * qz / r),
        }
    } else if qx >= qz {
        let east = x > 0.0;
        Perim { s: x.abs() - HALF_X, panel: if east { 0 } else { 2 }, w: z, w_scale: 1.0, normal: Vec2::new(x.signum(), 0.0) }
    } else {
        let north = z > 0.0;
        Perim { s: z.abs() - HALF_Z, panel: if north { 1 } else { 3 }, w: x, w_scale: 1.0, normal: Vec2::new(0.0, z.signum()) }
    }
}

/// Distance (m) from a point to the nearest panel seam along the perimeter (straight ends, the
/// mirror line at the middle of each straight, corner segment boundaries).
fn seam_dist(p: &Perim) -> f32 {
    if p.panel < 4 {
        let half = if p.panel % 2 == 0 { CZ } else { CX };
        (half - p.w.abs()).min(p.w.abs())
    } else {
        let seg = PI * 0.5 / CORNER_SEGS as f32;
        p.w.min(seg - p.w) * p.w_scale
    }
}

/// Curl polar coordinates of a point `s` out from the edge at height `y`: (distance from the arc
/// centre, angle from straight down in radians — 0 at the street, π/2 at the vertical wall).
#[inline]
pub fn curl_polar(s: f32, y: f32) -> (f32, f32) {
    let dy = CURL_R - (y - STREET_Y);
    ((s * s + dy * dy).sqrt(), datan2(s, dy))
}

/// Point on the curl at angle `theta` (rad), `inset` metres in front of the voxel arc: (s, y).
#[inline]
pub fn curl_point(theta: f32, inset: f32) -> Vec2 {
    let r = CURL_R - inset;
    Vec2::new(r * dsin(theta), STREET_Y + CURL_R - r * dcos(theta))
}

/// The point `s` metres out from the inner edge at perimeter angle... helper for bots/tests: world
/// position on the curl at `theta` (deg) for a given outward normal base point.
pub fn curl_world(base: Vec2, normal: Vec2, theta_deg: f32, inset: f32) -> Vec3 {
    let p = curl_point(rad(theta_deg), inset);
    Vec3::new(base.x + normal.x * p.x, p.y, base.y + normal.y * p.x)
}

/// A ring of points around the arena on the curl at `theta_deg`, `n` per straight / corner
/// segment-ish (counter-clockwise from the east side's south end), for routes and bots.
pub fn curl_ring(theta_deg: f32, inset: f32, step: f32) -> Vec<Vec3> {
    let mut out = Vec::new();
    // Walk the inner edge counter-clockwise: E straight (z −CZ → CZ), NE corner, N straight (x CX →
    // −CX), NW corner, W straight, SW corner, S straight, SE corner.
    let sides: [(Vec2, Vec2, Vec2); 4] = [
        (Vec2::new(HALF_X, -CZ), Vec2::new(HALF_X, CZ), Vec2::X),
        (Vec2::new(CX, HALF_Z), Vec2::new(-CX, HALF_Z), Vec2::Y),
        (Vec2::new(-HALF_X, CZ), Vec2::new(-HALF_X, -CZ), Vec2::NEG_X),
        (Vec2::new(-CX, -HALF_Z), Vec2::new(CX, -HALF_Z), Vec2::NEG_Y),
    ];
    let centres = [Vec2::new(CX, CZ), Vec2::new(-CX, CZ), Vec2::new(-CX, -CZ), Vec2::new(CX, -CZ)];
    for (i, (a, b, n)) in sides.iter().enumerate() {
        let len = a.distance(*b);
        let k = (len / step).ceil().max(1.0) as i32;
        for j in 0..k {
            let base = a.lerp(*b, j as f32 / k as f32);
            out.push(curl_world(base, *n, theta_deg, inset));
        }
        let c = centres[i];
        let arc = CORNER_R * PI * 0.5;
        let k = (arc / step).ceil().max(1.0) as i32;
        for j in 0..k {
            let psi = (i as f32 + j as f32 / k as f32) * PI * 0.5;
            let n = Vec2::new(dcos(psi), dsin(psi));
            out.push(curl_world(c + n * CORNER_R, n, theta_deg, inset));
        }
    }
    out
}

/// The curl's hard-light surf lines: per straight side and per corner segment, one line per chord
/// of [`CURL_BREAKS`], ridges horizontal, the plaza-facing face (`surf_line`'s right face) up.
pub fn curl_surf_lines() -> Vec<SurfLine> {
    let mut out = Vec::new();
    for i in 0..4 {
        let t0 = rad(CURL_BREAKS[i]);
        let t1 = rad(CURL_BREAKS[i + 1]);
        let lo = curl_point(t0, if i == 0 { -0.3 } else { INSET });
        let hi = curl_point(t1, INSET);
        let dv = hi - lo;
        let face_deg = datan2(dv.y, dv.x) * (180.0 / PI);
        let width = dv.length() + 0.5;
        let y = hi.y;
        let r = hi.x; // outward offset of the ridge
        // Straights (ridge direction = (−n.z, n.x) so the right face looks inward).
        let ext = 0.3;
        out.push(SurfLine { a: Vec3::new(HALF_X + r, y, -CZ - ext), b: Vec3::new(HALF_X + r, y, CZ + ext), face_deg, width });
        out.push(SurfLine { a: Vec3::new(CX + ext, y, HALF_Z + r), b: Vec3::new(-CX - ext, y, HALF_Z + r), face_deg, width });
        out.push(SurfLine { a: Vec3::new(-HALF_X - r, y, CZ + ext), b: Vec3::new(-HALF_X - r, y, -CZ - ext), face_deg, width });
        out.push(SurfLine { a: Vec3::new(-CX - ext, y, -HALF_Z - r), b: Vec3::new(CX + ext, y, -HALF_Z - r), face_deg, width });
        // Corners, counter-clockwise.
        let centres = [Vec2::new(CX, CZ), Vec2::new(-CX, CZ), Vec2::new(-CX, -CZ), Vec2::new(CX, -CZ)];
        for (q, c) in centres.iter().enumerate() {
            for k in 0..CORNER_SEGS {
                let seg = PI * 0.5 / CORNER_SEGS as f32;
                let (pa, pb) = (q as f32 * PI * 0.5 + k as f32 * seg, q as f32 * PI * 0.5 + (k + 1) as f32 * seg);
                let rr = CORNER_R + r;
                let a = *c + Vec2::new(dcos(pa), dsin(pa)) * rr;
                let b = *c + Vec2::new(dcos(pb), dsin(pb)) * rr;
                out.push(SurfLine { a: Vec3::new(a.x, y, a.y), b: Vec3::new(b.x, y, b.y), face_deg, width });
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------
// Ground: land, streets, plaza, fractures

/// Fracture polylines (x, z) and their widths: the plaza crack and two ring-road corner gaps.
pub fn fractures() -> Vec<(Vec<Vec2>, f32)> {
    let d = Vec2::new(0.70710677, 0.70710677);
    let (a, b) = (CORNER_R - RING_W - SIDEWALK + 0.6, CORNER_R - 0.8);
    let ne = Vec2::new(CX, CZ);
    vec![
        (
            vec![
                Vec2::new(-17.5, -1.0),
                Vec2::new(-10.0, -5.5),
                Vec2::new(-4.0, -3.5),
                Vec2::new(2.0, -7.5),
                Vec2::new(7.0, -6.0),
                Vec2::new(11.5, -11.0),
            ],
            2.6,
        ),
        (vec![ne + d * a, ne + d * b], 2.6),
        (vec![-ne - d * a, -ne - d * b], 2.6),
    ]
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Distance past the edge of the nearest fracture (negative = inside a gap).
pub fn fracture_dist(p: Vec2) -> f32 {
    let mut best = f32::INFINITY;
    for (pts, w) in fractures() {
        for s in pts.windows(2) {
            best = best.min(seg_dist(p, s[0], s[1]) - w * 0.5);
        }
    }
    best
}

fn on_avenue(x: f32, z: f32) -> bool {
    (z.abs() <= AVE_HALF && x.abs() >= PLAZA.max.x) || (x.abs() <= AVE_HALF && z.abs() >= PLAZA.max.y)
}

/// Surface of an inner-city column: (top height, material).
fn ground_top(x: f32, z: f32, pr: &Perim) -> (f32, Material) {
    let s = pr.s;
    if s > -RING_W {
        // Ring road with a dashed centre line.
        let dash = (s + RING_W * 0.5).abs() < 0.25 && (pr.w * pr.w_scale.min(CORNER_R)).rem_euclid(6.0) < 3.0;
        return (STREET_Y, if dash { mat::PAINT_WHITE } else { mat::ASPHALT });
    }
    if on_avenue(x, z) {
        let along = if z.abs() <= AVE_HALF && x.abs() >= PLAZA.max.x { x } else { z };
        let across = if z.abs() <= AVE_HALF && x.abs() >= PLAZA.max.x { z } else { x };
        let dash = across.abs() < 0.25 && along.rem_euclid(6.0) < 3.0;
        return (STREET_Y, if dash { mat::PAINT_YELLOW } else { mat::ASPHALT });
    }
    if s > -RING_W - SIDEWALK {
        return (WALK_Y, if s > -RING_W - 0.5 { mat::MIRROR_CORNICE } else { mat::CONCRETE });
    }
    if PLAZA.contains(Vec2::new(x, z)) {
        return (WALK_Y, plaza_inlay(x, z));
    }
    // Paving with a 2 m joint grid.
    let joint = x.rem_euclid(2.0) < 0.5 || z.rem_euclid(2.0) < 0.5;
    (WALK_Y, if joint { mat::CONCRETE } else { mat::MIRROR_PAVING })
}

/// The plaza's kaleidoscope inlay: eight mirrored wedges, glowing rays and rings.
fn plaza_inlay(x: f32, z: f32) -> Material {
    let r = (x * x + z * z).sqrt();
    let a = datan2(z, x) + PI; // 0..2π
    let wedge = PI / 8.0;
    let k = (a / wedge) as i32;
    let fa = a - k as f32 * wedge; // within wedge
    let off = fa.min(wedge - fa) * r; // metres to the nearest ray
    if (5.5..=6.0).contains(&r) || (9.0..=9.5).contains(&r) {
        return mat::MIRROR_CORNICE;
    }
    if (1.5..9.0).contains(&r) && off < 0.26 && k % 2 == 0 {
        return mat::MIRROR_SEAM;
    }
    if r < 9.5 {
        return if k % 2 == 0 { mat::MIRROR_PAVING } else { mat::MIRROR_SHADE };
    }
    let joint = x.rem_euclid(2.0) < 0.5 || z.rem_euclid(2.0) < 0.5;
    if joint { mat::CONCRETE } else { mat::MIRROR_PAVING }
}

fn build_ground(c: &mut City) {
    let (lo, hi) = (c.w.voxel_min(), c.w.voxel_max());
    for vz in lo.z..hi.z {
        for vxx in lo.x..hi.x {
            let p = vc(IVec3::new(vxx, 0, vz));
            let pr = perim(p.x, p.z);
            if pr.s > 0.0 {
                continue;
            }
            let fd = fracture_dist(Vec2::new(p.x, p.z));
            if fd < 0.0 {
                continue; // open to the void
            }
            let (top, m) = ground_top(p.x, p.z, &pr);
            let ty = (top / 0.5).round() as i32 - 1;
            c.w.fill_column(vxx, vz, 0, 1, mat::TOWER_CORE);
            let near = fd < 0.5;
            // Fracture walls glint (mirror shards) for the top few metres.
            let shard_from = if fd < 1.0 { ty - 6 } else { i32::MAX };
            c.w.fill_column(vxx, vz, 1, shard_from.min(ty).max(1), mat::MIRROR_SHADE);
            if shard_from < ty {
                c.w.fill_column(vxx, vz, shard_from.max(1), ty, mat::MIRROR_SHARD);
            }
            c.w.set(IVec3::new(vxx, ty, vz), if near { mat::MIRROR_SEAM } else { m });
        }
    }
    // Collapse hints along the bedrock every 16 m.
    for z in (-40..=40).step_by(16) {
        for x in (-56..=56).step_by(16) {
            let v = IVec3::new(x * 2, 0, z * 2);
            if c.w.get(v) == mat::TOWER_CORE {
                c.anchor(v);
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The curl

/// A building sticking out of the overhang toward the curl's centre.
#[derive(Clone, Copy, Debug)]
struct CurlBlock {
    panel: i32,
    /// Panel coordinate range (see [`Perim::w`]).
    lo: f32,
    hi: f32,
    /// Angle range (rad).
    th0: f32,
    th1: f32,
    depth: f32,
}

fn curl_blocks(rng: &mut Rng) -> Vec<CurlBlock> {
    let mut out = Vec::new();
    for panel in 0..4 {
        let half = if panel % 2 == 0 { CZ } else { CX };
        let mut w = -half + rng.range(1.0, 5.0);
        while w < half - 6.0 {
            let len = rng.range(6.0, 10.0).min(half - 1.0 - w);
            // Keep the mirror seam at w = 0 clear.
            if w < 0.0 && w + len > -0.8 {
                w = 0.8;
                continue;
            }
            let th0 = rng.range(96.0, 104.0);
            out.push(CurlBlock {
                panel,
                lo: w,
                hi: w + len,
                th0: rad(th0),
                th1: rad((th0 + rng.range(8.0, 15.0)).min(118.0)),
                depth: rng.range(2.0, 5.0),
            });
            w += len + rng.range(3.0, 8.0);
        }
    }
    let seg = PI * 0.5 / CORNER_SEGS as f32;
    for corner in 0..4 {
        for k in 0..CORNER_SEGS {
            if rng.chance(0.4) {
                continue;
            }
            let th0 = rng.range(96.0, 104.0);
            out.push(CurlBlock {
                panel: 4 + corner * CORNER_SEGS + k,
                lo: seg * 0.15,
                hi: seg * 0.85,
                th0: rad(th0),
                th1: rad((th0 + rng.range(8.0, 16.0)).min(118.0)),
                depth: rng.range(2.0, 5.0),
            });
        }
    }
    out
}

/// Facade material at facade coordinates (`a` across, `v` up, metres) — windows every 3 m × 4 m.
fn facade(a: f32, v: f32, key: u32) -> Material {
    let (ca, cv) = ((a / 3.0).floor() as i32, (v / 4.0).floor() as i32);
    let (fa, fv) = (a - ca as f32 * 3.0, v - cv as f32 * 4.0);
    if cv.rem_euclid(3) == 0 && fv < 0.5 {
        return mat::MIRROR_CORNICE;
    }
    if (0.75..2.25).contains(&fa) && (1.0..3.0).contains(&fv) {
        return if unit(hash3(ca, cv, key as i32, 0x3c6e_f372)) < 0.12 { mat::MIRROR_WINDOW_LIT } else { mat::MIRROR_WINDOW };
    }
    mat::MIRROR_STONE
}

fn curl_surface_mat(pr: &Perim, theta: f32) -> Material {
    let v = theta * CURL_R; // metres of arc from the street
    let a = pr.w * pr.w_scale;
    if seam_dist(pr) < 0.3 {
        return mat::MIRROR_SEAM;
    }
    if theta > rad(LIP_DEG - 1.3) {
        return mat::MIRROR_SEAM;
    }
    if theta > rad(LIP_DEG - 3.5) {
        return mat::MIRROR_CORNICE;
    }
    if theta < rad(35.0) {
        // The street bending up: asphalt with a dashed line along the curl.
        let dash = (v - 3.0).abs() < 0.3 && a.rem_euclid(6.0) < 3.0;
        return if dash { mat::PAINT_WHITE } else { mat::ASPHALT };
    }
    if theta < rad(40.0) {
        return mat::CONCRETE;
    }
    facade(a + 100.0, v - rad(40.0) * CURL_R, pr.panel as u32)
}

fn build_curl(c: &mut City, blocks: &[CurlBlock]) {
    let (lo, hi) = (c.w.voxel_min(), c.w.voxel_max());
    let y0 = ((STREET_Y - SHELL - 1.0) / 0.5) as i32;
    let y1 = ((STREET_Y + CURL_R * 1.55 + SHELL + 1.0) / 0.5) as i32;
    let lip = rad(LIP_DEG);
    for vz in lo.z..hi.z {
        for vxx in lo.x..hi.x {
            let pc = vc(IVec3::new(vxx, 0, vz));
            let pr = perim(pc.x, pc.z);
            if pr.s < 0.0 || pr.s > CURL_R + SHELL + 0.5 {
                continue;
            }
            let my_blocks: Vec<&CurlBlock> = blocks.iter().filter(|b| b.panel == pr.panel).collect();
            for vy in y0..y1 {
                let y = (vy as f32 + 0.5) * 0.5;
                let (rho, theta) = curl_polar(pr.s, y);
                if theta > lip {
                    continue;
                }
                let mut m = None;
                if (CURL_R..=CURL_R + SHELL).contains(&rho) {
                    let depth = rho - CURL_R;
                    m = Some(if depth < 0.55 {
                        curl_surface_mat(&pr, theta)
                    } else if seam_dist(&pr) < 0.5 || (pr.panel < 4 && (pr.w + 200.0).rem_euclid(12.0) < 0.6) {
                        mat::MIRROR_CORE // ribs: the curl never collapses
                    } else {
                        mat::MIRROR_SHADE
                    });
                } else if rho < CURL_R {
                    for b in &my_blocks {
                        if theta < b.th0 || theta > b.th1 || pr.w < b.lo || pr.w > b.hi || rho < CURL_R - b.depth {
                            continue;
                        }
                        let df = rho - (CURL_R - b.depth);
                        let du = (pr.w - b.lo).min(b.hi - pr.w) * pr.w_scale;
                        let dv = (theta - b.th0).min(b.th1 - theta) * rho;
                        let edges = (df < 0.5) as u8 + (du < 0.5) as u8 + (dv < 0.5) as u8;
                        m = Some(if edges >= 2 {
                            mat::MIRROR_CORNICE
                        } else if df <= du && df <= dv {
                            facade(pr.w * pr.w_scale + 50.0, theta * CURL_R, pr.panel as u32 + 97)
                        } else if du <= dv {
                            facade(theta * CURL_R, rho, pr.panel as u32 + 31)
                        } else {
                            facade(pr.w * pr.w_scale, rho, pr.panel as u32 + 57)
                        });
                        break;
                    }
                }
                if let Some(m) = m {
                    c.w.set(IVec3::new(vxx, vy, vz), m);
                }
            }
        }
    }
    // Anchor hints on the ribs (mid-height of each straight's centre rib and each corner seam).
    for pr_pt in curl_ring(90.0, -SHELL + 0.25, 12.0) {
        let v = (pr_pt / 0.5).floor().as_ivec3();
        c.anchor(v);
    }
}

// ------------------------------------------------------------------------------------------------
// Buildings (upright on the plaza, or hanging from the sky)

#[derive(Clone, Copy, Debug, PartialEq)]
enum Style {
    Stone,
    Brick,
    Glass,
}

#[derive(Clone, Copy, Debug)]
struct Bldg {
    rect: Rect,
    /// Ground it stands on (upright) or the height it hangs from (inverted).
    base_y: f32,
    height: f32,
    floor_h: f32,
    hang: bool,
    style: Style,
    open_floor: Option<i32>,
    doors: bool,
    /// Stepped crown over the last `crown` metres (two 1 m setbacks).
    crown: f32,
    /// Indestructible spire past the roof (hanging towers: a tether hook at the very bottom).
    spire: f32,
    /// Leave the parapet open inside these rectangles (stair arrivals, jump-off edges).
    parapet_gaps: [Option<Rect>; 2],
}

impl Bldg {
    fn new(rect: Rect, base_y: f32, height: f32, style: Style) -> Self {
        Self {
            rect,
            base_y,
            height,
            floor_h: 4.0,
            hang: false,
            style,
            open_floor: None,
            doors: true,
            crown: 0.0,
            spire: 0.0,
            parapet_gaps: [None; 2],
        }
    }
    /// Roof surface height of an upright building.
    fn roof_y(&self) -> f32 {
        self.base_y + self.height + 0.5
    }
    /// Lowest point of a hanging one (spire tip).
    fn bottom_y(&self) -> f32 {
        self.base_y - self.height - 1.5 - self.spire
    }
    fn inset(&self, t: f32) -> f32 {
        if self.crown <= 0.0 {
            0.0
        } else if t > self.height - self.crown * 0.5 {
            2.0
        } else if t > self.height - self.crown {
            1.0
        } else {
            0.0
        }
    }
}

fn building(c: &mut City, b: &Bldg, key: u32) {
    let h = b.height;
    let fh = b.floor_h;
    let cc = b.rect.center();
    let (ylo, yhi) = if b.hang { (b.bottom_y(), b.base_y) } else { (b.base_y, b.base_y + h + 1.5) };
    let lo = Vec3::new(b.rect.min.x - 0.5, ylo, b.rect.min.y - 0.5);
    let hi = Vec3::new(b.rect.max.x + 0.5, yhi, b.rect.max.y + 0.5);
    let (body, trim) = match b.style {
        Style::Stone => (mat::MIRROR_STONE, mat::MIRROR_CORNICE),
        Style::Brick => (mat::CITY_BRICK, mat::MIRROR_CORNICE),
        Style::Glass => (mat::MIRROR_WINDOW, mat::MULLION),
    };
    c.fill_with(lo, hi, |p, _, _| {
        let t = if b.hang { b.base_y - p.y } else { p.y - b.base_y };
        if t < 0.0 {
            return None;
        }
        let q = Vec2::new(p.x, p.z);
        // Spine / spire.
        let core = (p.x - cc.x).abs() < 1.0 && (p.z - cc.y).abs() < 1.0;
        if core && (t <= h + 0.5 || (b.hang && t <= h + 1.5 + b.spire)) {
            return Some(mat::MIRROR_CORE);
        }
        let ins = b.inset(t.min(h - 0.01));
        let rect = b.rect.inset(ins);
        let d = rect.sdf(q, 0.0);
        if t >= h {
            // Roof slab + parapet (the bottom end of a hanging tower).
            if t < h + 0.5 {
                return (d <= 0.0).then_some(if b.hang { mat::MIRROR_SHADE } else { mat::ROOF_DARK });
            }
            if t < h + 1.5 && d > -0.5 && d <= 0.0 {
                if b.parapet_gaps.iter().flatten().any(|g| g.contains(q)) {
                    return None;
                }
                return Some(trim);
            }
            return None;
        }
        let k = (t / fh).floor() as i32;
        let tf = t - k as f32 * fh;
        let wall = d > -0.5 && d <= 0.0;
        // Ledges: belt course over the ground floor, cornice under the roof, each setback.
        let ledge = d > 0.0 && d <= 0.5 && ((4.0..4.5).contains(&t) || (h - 1.0..h - 0.5).contains(&t));
        if ledge && b.style != Style::Glass {
            return Some(trim);
        }
        if d > 0.0 {
            return None;
        }
        if !wall {
            // Interior: floor slabs (the setback terraces too).
            if tf < 0.5 && (k > 0 || b.hang) {
                return Some(mat::CONCRETE);
            }
            return None;
        }
        // Facade coordinate along this wall.
        let dq = (q - rect.center()).abs() - rect.half();
        let x_face = dq.x > dq.y;
        let (a, len) = if x_face { (p.z - rect.min.y, rect.max.y - rect.min.y) } else { (p.x - rect.min.x, rect.max.x - rect.min.x) };
        let side_key = key.wrapping_mul(7) + x_face as u32 * 2 + (if x_face { p.x > cc.x } else { p.z > cc.y }) as u32;
        if b.open_floor == Some(k) && tf >= 0.5 {
            // Loggia / sky lobby: square piers every 6 m, otherwise open.
            let pier = a.rem_euclid(6.0) < 1.0 || a > len - 1.0;
            return pier.then_some(trim);
        }
        if tf < 0.5 {
            return Some(if b.style == Style::Glass { mat::MIRROR_SHADE } else { trim });
        }
        if !b.hang && b.doors && k == 0 && (a - len * 0.5).abs() < 1.75 && tf < 4.0 {
            return None;
        }
        Some(match b.style {
            Style::Glass => {
                if a.rem_euclid(2.0) < 0.5 {
                    trim
                } else if unit(hash3((a / 2.0) as i32, k, side_key as i32, 0x0bad_5eed)) < 0.3 {
                    mat::GLASS_BLUE
                } else {
                    body
                }
            }
            Style::Stone | Style::Brick => {
                let quoin = a < 1.0 || a > len - 1.0;
                if quoin {
                    if (t * 2.0).floor() as i32 % 2 == 0 { trim } else { body }
                } else if k == 0 && !b.hang {
                    // Rusticated ground floor with tall arched windows.
                    let ca = (a / 6.0).floor();
                    let fa = a - ca * 6.0;
                    if (1.5..4.5).contains(&fa) && (1.0..3.5).contains(&tf) && (a - len * 0.5).abs() > 2.5 {
                        mat::MIRROR_WINDOW
                    } else if (tf * 2.0).floor() as i32 % 2 == 0 {
                        mat::MIRROR_SHADE
                    } else {
                        body
                    }
                } else {
                    let step = if b.style == Style::Brick { 2.5 } else { 3.0 };
                    let ca = (a / step).floor() as i32;
                    let fa = a - ca as f32 * step;
                    if (step * 0.25..step * 0.75).contains(&fa) && (1.0..3.0).contains(&tf) {
                        if unit(hash3(ca, k, side_key as i32, 0x51ed_270b)) < 0.14 { mat::MIRROR_WINDOW_LIT } else { mat::MIRROR_WINDOW }
                    } else if (h - 1.5..h).contains(&t) {
                        trim
                    } else {
                        body
                    }
                }
            }
        })
    });
    // Seal the closed floors (render-only culling until something opens them).
    let floors = (h / fh) as i32;
    for k in 0..floors {
        if b.open_floor == Some(k) || (k == 0 && b.doors && !b.hang) {
            continue;
        }
        let tmid = k as f32 * fh + fh * 0.5 + 0.25;
        let y = if b.hang { b.base_y - tmid } else { b.base_y + tmid };
        let seed = Vec3::new(cc.x + 1.75, y, cc.y + 1.75);
        let r = b.rect.inset(b.inset(tmid));
        c.interior(seed, Vec3::new(r.min.x - 1.0, y - fh, r.min.y - 1.0), Vec3::new(r.max.x + 1.0, y + fh, r.max.y + 1.0));
    }
    let core_v = (Vec3::new(cc.x, if b.hang { b.base_y - 1.0 } else { b.base_y + 1.0 }, cc.y) / 0.5).floor().as_ivec3();
    c.anchor(core_v);
}

/// The six towers hanging from the sky (and their spire tips: the lowest anchors).
fn hanging_towers() -> Vec<Bldg> {
    let spec = |x: f32, z: f32, hx: f32, hz: f32, bottom: f32, style: Style, crown: f32| {
        let spire = 4.0;
        let height = TOP_Y - bottom - 1.5 - spire;
        Bldg {
            rect: Rect::from_center(Vec2::new(x, z), Vec2::new(hx, hz)),
            base_y: TOP_Y,
            height,
            floor_h: 4.0,
            hang: true,
            style,
            open_floor: None,
            doors: false,
            crown,
            spire,
            parapet_gaps: [None; 2],
        }
    };
    vec![
        spec(35.0, 14.0, 6.0, 6.0, 60.0, Style::Stone, 0.0),
        spec(0.0, 29.0, 6.5, 5.0, 64.0, Style::Glass, 0.0),
        spec(-35.0, 14.0, 5.5, 6.5, 58.0, Style::Stone, 8.0),
        spec(-35.0, -14.0, 6.0, 6.0, 62.0, Style::Glass, 0.0),
        spec(0.0, -29.0, 6.5, 5.0, 60.0, Style::Stone, 8.0),
        spec(35.0, -14.0, 5.5, 6.5, 64.0, Style::Brick, 8.0),
    ]
}

/// Spire tips of the hanging towers (tether targets for bots and tests).
pub fn tower_tips() -> Vec<Vec3> {
    hanging_towers().iter().map(|b| Vec3::new(b.rect.center().x, b.bottom_y() + 0.25, b.rect.center().y)).collect()
}

/// Footprints (centre, half extents) of the hanging towers (the client continues them visually
/// above the world top).
pub fn hanging_tower_footprints() -> Vec<(Vec2, Vec2)> {
    hanging_towers().iter().map(|b| (b.rect.center(), b.rect.half())).collect()
}

/// Bottom-face centres of the hanging towers (just under the roof slab end).
pub fn tower_bottoms() -> Vec<Vec3> {
    hanging_towers()
        .iter()
        .map(|b| Vec3::new(b.rect.center().x + b.rect.half().x * 0.5, b.base_y - b.height - 0.5, b.rect.center().y))
        .collect()
}

// The city blocks.
const LIBRARY: Rect = Rect::new(19.0, 12.0, 44.0, 28.0);
const BROWN_W: Rect = Rect::new(-44.0, 12.0, -35.0, 28.0);
const BROWN_M: Rect = Rect::new(-35.0, 12.0, -27.0, 28.0);
const BROWN_E: Rect = Rect::new(-27.0, 12.0, -19.0, 28.0);
const GLASS_TOWER: Rect = Rect::new(-44.0, -28.0, -19.0, -12.0);
const BANK: Rect = Rect::new(19.0, -28.0, 44.0, -12.0);
/// The library's fractured-off corner (it floats nearby as a fragment).
const LIBRARY_CUT: (Vec3, Vec3) = (Vec3::new(36.0, 28.5, 21.5), Vec3::new(44.5, 36.0, 28.5));

fn build_blocks(c: &mut City) -> BlockInfo {
    // NE: the library — five stone floors, an open loggia on the third, a corner torn away.
    let mut lib = Bldg::new(LIBRARY, WALK_Y, 20.0, Style::Stone);
    lib.open_floor = Some(2);
    building(c, &lib, 1);
    let (cl, ch) = LIBRARY_CUT;
    c.fill(cl, ch, AIR);
    // Seams along the cut.
    c.fill_with(cl - Vec3::splat(0.5), ch + Vec3::splat(0.5), |p, _, old| {
        let inside = p.cmpge(cl).all() && p.cmple(ch).all();
        (old != AIR && !inside && old != mat::MIRROR_CORE).then_some(mat::MIRROR_SEAM)
    });

    // NW: three brownstones (12 / 16 / 12 m) with a fire escape up the east one's south face.
    let mut bw = Bldg::new(BROWN_W, WALK_Y, 12.0, Style::Brick);
    bw.doors = true;
    building(c, &bw, 2);
    building(c, &Bldg::new(BROWN_M, WALK_Y, 16.0, Style::Stone), 3);
    let mut be = Bldg::new(BROWN_E, WALK_Y, 12.0, Style::Brick);
    // Open parapet where the fire escape arrives (south) and along the east edge (jump-off toward
    // the cornice stones to the clock).
    be.parapet_gaps = [Some(Rect::new(-26.0, 11.0, -21.0, 13.0)), Some(Rect::new(-20.0, 15.0, -18.0, 27.0))];
    building(c, &be, 4);
    // Fire escape: three steel flights in two lanes, landings at both ends.
    let lane_a = (10.5, 12.0);
    let lane_b = (9.0, 10.5);
    let steel = mat::STEEL;
    c.stairs(Vec3::new(-27.0, WALK_Y, (lane_a.0 + lane_a.1) * 0.5), Vec2::X, WALK_Y + 4.0, lane_a.1 - lane_a.0, 1.0, steel);
    c.fill(Vec3::new(-19.0, WALK_Y + 3.5, lane_b.0), Vec3::new(-17.5, WALK_Y + 4.0, lane_a.1), steel);
    c.fill(Vec3::new(-19.0, WALK_Y, lane_b.0), Vec3::new(-18.5, WALK_Y + 3.5, lane_b.0 + 0.5), steel); // post
    c.stairs(Vec3::new(-19.0, WALK_Y + 4.0, (lane_b.0 + lane_b.1) * 0.5), Vec2::NEG_X, WALK_Y + 8.0, lane_b.1 - lane_b.0, 1.0, steel);
    c.fill(Vec3::new(-28.5, WALK_Y + 7.5, lane_b.0), Vec3::new(-27.0, WALK_Y + 8.0, lane_a.1), steel);
    c.fill(Vec3::new(-28.5, WALK_Y, lane_b.0), Vec3::new(-28.0, WALK_Y + 7.5, lane_b.0 + 0.5), steel); // post
    c.stairs(Vec3::new(-27.0, WALK_Y + 8.0, (lane_a.0 + lane_a.1) * 0.5), Vec2::X, WALK_Y + 12.5, lane_a.1 - lane_a.0, 0.9, steel);

    // SW: a glass tower from the city kit (sky lobby on the fourth floor).
    let mut gt = TowerSpec::new(GLASS_TOWER, WALK_Y, 7);
    gt.glass = mat::GLASS_BLUE;
    gt.glass_alt = Some((mat::MIRROR_WINDOW, 0.35));
    gt.frame = mat::MULLION;
    gt.spandrel = mat::MIRROR_SHADE;
    gt.spandrel_h = 0.5;
    gt.open_floors = vec![3];
    gt.crown = Crown::NeonBand(mat::MIRROR_SEAM);
    gt.core_half = 1.0;
    c.tower(&gt);

    // SE: the bank — two tall stone floors, grand stair up its west side to the roof.
    let mut bank = Bldg::new(BANK, WALK_Y, 10.0, Style::Stone);
    bank.floor_h = 5.0;
    // Open along the west edge (jump-off toward the stones) and where the grand stair arrives.
    bank.parapet_gaps = [Some(Rect::new(18.0, -28.5, 20.0, -11.0)), Some(Rect::new(18.0, -13.0, 22.0, -11.0))];
    building(c, &bank, 5);
    c.stairs(Vec3::new(17.25, WALK_Y, -27.5), Vec2::Y, bank.roof_y(), 2.5, 0.7, mat::MIRROR_CORNICE);

    BlockInfo { library_roof: lib.roof_y(), brown_roof: be.roof_y(), bank_roof: bank.roof_y(), glass_roof: gt.roof_y() }
}

struct BlockInfo {
    library_roof: f32,
    brown_roof: f32,
    bank_roof: f32,
    glass_roof: f32,
}

fn build_props(c: &mut City, rng: &mut Rng) {
    // Street lights along the avenues and the ring road's inner kerb.
    for k in [-40.0f32, -28.0, 28.0, 40.0] {
        c.street_light(Vec3::new(k, WALK_Y, 6.0), Vec2::NEG_Y, 5.0);
        c.street_light(Vec3::new(k, WALK_Y, -6.0), Vec2::Y, 5.0);
    }
    for k in [-25.0f32, 25.0] {
        c.street_light(Vec3::new(6.0, WALK_Y, k), Vec2::NEG_X, 5.0);
        c.street_light(Vec3::new(-6.0, WALK_Y, k), Vec2::X, 5.0);
    }
    // Parked cars (cover / hop blocks) on the ring road and avenues.
    let bodies = [mat::PAINT_RED, mat::HULL_NAVY, mat::STUCCO_WHITE, mat::PAINT_YELLOW, mat::ROOF_DARK];
    let cars = [
        (Vec3::new(52.0, STREET_Y, 20.0), false),
        (Vec3::new(-52.0, STREET_Y, -20.0), false),
        (Vec3::new(-12.0, STREET_Y, 36.5), true),
        (Vec3::new(12.0, STREET_Y, -36.5), true),
        (Vec3::new(30.0, STREET_Y, 2.5), true),
        (Vec3::new(-30.0, STREET_Y, -2.5), true),
        (Vec3::new(2.5, STREET_Y, 22.0), false),
        (Vec3::new(-2.5, STREET_Y, -22.0), false),
    ];
    for (base, along_x) in cars {
        let body = *rng.pick(&bodies);
        c.car(&CarSpec { base, along_x, body, convertible: rng.chance(0.3) });
    }
    // Pocket-plaza kiosks and planters (waist-high cover).
    for (x, z) in [(12.0f32, 20.0f32), (-12.0, -20.0)] {
        c.fill(Vec3::new(x - 1.5, WALK_Y, z - 1.0), Vec3::new(x + 1.5, WALK_Y + 2.5, z + 1.0), mat::MIRROR_SHADE);
        c.fill(Vec3::new(x - 2.0, WALK_Y + 2.5, z - 1.5), Vec3::new(x + 2.0, WALK_Y + 3.0, z + 1.5), mat::MIRROR_CORNICE);
    }
    for (x, z) in [(-12.0f32, 16.0f32), (12.0, -16.0), (-24.0, 8.0), (24.0, -8.0), (24.0, 8.0), (-24.0, -8.0)] {
        c.fill(Vec3::new(x - 1.5, WALK_Y, z - 0.75), Vec3::new(x + 1.5, WALK_Y + 1.0, z + 0.75), mat::MIRROR_CORNICE);
        c.fill(Vec3::new(x - 1.0, WALK_Y + 1.0, z - 0.25), Vec3::new(x + 1.0, WALK_Y + 1.5, z + 0.25), mat::LEAVES_GREEN);
    }
    // Plaza: a low ring basin around a leaning mirror obelisk (breaks the centre sightline).
    c.fill_with(Vec3::new(-4.0, WALK_Y, -4.0), Vec3::new(4.0, WALK_Y + 1.0, 4.0), |p, _, _| {
        let r = (p.x * p.x + p.z * p.z).sqrt();
        (3.0..3.6).contains(&r).then_some(mat::MIRROR_CORNICE)
    });
    let rot = rot3(20.0, 12.0, 6.0);
    shape(c, Vec3::new(0.0, WALK_Y + 3.0, 0.0), Vec3::new(0.9, 3.5, 0.9), rot, |l, h| {
        let edge = (l.x.abs() > h.x - 0.4) as u8 + (l.z.abs() > h.z - 0.4) as u8;
        Some(if edge == 2 { mat::MIRROR_SEAM } else { mat::MIRROR_SHARD })
    });
    // Keep its foot planted: a stone plinth under it.
    c.fill(Vec3::new(-1.5, WALK_Y, -1.5), Vec3::new(1.5, WALK_Y + 0.5, 1.5), mat::MIRROR_CORE);
}

// ------------------------------------------------------------------------------------------------
// Fragments

/// Deterministic rotation: yaw about Y, then pitch about local X, then roll about local Z (degrees).
fn rot3(yaw: f32, pitch: f32, roll: f32) -> Mat3 {
    let (sy, cy) = (dsin(rad(yaw)), dcos(rad(yaw)));
    let (sp, cp) = (dsin(rad(pitch)), dcos(rad(pitch)));
    let (sr, cr) = (dsin(rad(roll)), dcos(rad(roll)));
    let ry = Mat3::from_cols(Vec3::new(cy, 0.0, -sy), Vec3::Y, Vec3::new(sy, 0.0, cy));
    let rx = Mat3::from_cols(Vec3::X, Vec3::new(0.0, cp, sp), Vec3::new(0.0, -sp, cp));
    let rz = Mat3::from_cols(Vec3::new(cr, sr, 0.0), Vec3::new(-sr, cr, 0.0), Vec3::Z);
    ry * rx * rz
}

/// Paint an oriented shape: every voxel whose centre, in the box's local frame, lies within `half`
/// gets `f(local, half)` (None = leave). Returns the number of voxels set.
fn shape(c: &mut City, center: Vec3, half: Vec3, rot: Mat3, mut f: impl FnMut(Vec3, Vec3) -> Option<Material>) -> usize {
    let r = half.length() + 0.5;
    let inv = rot.transpose();
    let mut n = 0;
    c.fill_with(center - Vec3::splat(r), center + Vec3::splat(r), |p, _, _| {
        let l = inv * (p - center);
        if l.x.abs() <= half.x && l.y.abs() <= half.y && l.z.abs() <= half.z {
            let m = f(l, half);
            n += m.is_some() as usize;
            m
        } else {
            None
        }
    });
    n
}

/// Pin a fragment: its heart voxel becomes indestructible core (so it floats forever).
fn pin(c: &mut City, at: Vec3) {
    let v = (at / 0.5).floor().as_ivec3();
    if c.w.get(v) != AIR {
        c.w.set(v, mat::MIRROR_CORE);
        c.anchor(v);
    }
}

/// A floating slab with a seam-lit fracture rim: `top` style on its top face, stone below.
fn slab(c: &mut City, center: Vec3, half: Vec3, rot: Mat3, top: Material) {
    shape(c, center, half, rot, |l, h| {
        let rim = (l.x.abs() > h.x - 0.5) as u8 + (l.z.abs() > h.z - 0.5) as u8 + (l.y.abs() > h.y - 0.5) as u8;
        Some(if rim >= 2 {
            mat::MIRROR_SEAM
        } else if l.y > h.y - 0.5 {
            top
        } else if l.y < -h.y + 0.5 {
            mat::MIRROR_SHARD
        } else {
            mat::MIRROR_SHADE
        })
    });
    pin(c, center);
}

/// A floating facade chunk (window grid on its faces).
fn facade_chunk(c: &mut City, center: Vec3, half: Vec3, rot: Mat3, key: u32) {
    shape(c, center, half, rot, |l, h| {
        let rim = (l.x.abs() > h.x - 0.5) as u8 + (l.z.abs() > h.z - 0.5) as u8 + (l.y.abs() > h.y - 0.5) as u8;
        Some(if rim >= 2 { mat::MIRROR_SEAM } else { facade(l.x + h.x + l.z, l.y + h.y, key) })
    });
    pin(c, center);
}

/// A steel fire-escape lattice (platforms every 3 m, rails, stringers), rotated arbitrarily.
fn fire_escape(c: &mut City, center: Vec3, half: Vec3, rot: Mat3) {
    shape(c, center, half, rot, |l, h| {
        let fy = (l.y + h.y).rem_euclid(3.0);
        let platform = fy < 0.5;
        let rail = (l.z > h.z - 0.5) && (fy < 0.5 || (1.0..1.5).contains(&fy));
        let post = (l.x.abs() > h.x - 0.5) && (l.z.abs() > h.z - 0.5);
        let stringer = {
            // A diagonal stair between platforms (inner lane).
            let k = ((l.y + h.y) / 3.0).floor();
            let along = if (k as i32) % 2 == 0 { l.x + h.x } else { h.x - l.x };
            let rise = (l.y + h.y) - k * 3.0;
            l.z < -h.z + 1.25 && (rise - along * 3.0 / (2.0 * h.x)).abs() < 0.35
        };
        let spine = l.x.abs() < 0.5 && l.z < -h.z + 0.5;
        (platform || rail || post || stringer || spine).then_some(mat::STEEL)
    });
    pin(c, center + rot * Vec3::new(0.0, 0.0, -half.z + 0.25));
}

/// A clock face disc (spawn island).
fn clock(c: &mut City, center: Vec3, radius: f32, thick: f32) {
    c.fill_with(center - Vec3::new(radius, thick, radius), center + Vec3::new(radius, 0.0, radius), |p, _, _| {
        let (dx, dz) = (p.x - center.x, p.z - center.z);
        let r = (dx * dx + dz * dz).sqrt();
        if r > radius {
            return None;
        }
        // Tapered underside (a lens), rim + hour marks + hands in seams on top.
        let depth = thick * (1.0 - (r / radius) * (r / radius) * 0.6);
        if p.y < center.y - depth {
            return None;
        }
        if p.y < center.y - 0.5 {
            return Some(if r > radius - 0.6 { mat::MIRROR_SEAM } else { mat::MIRROR_SHARD });
        }
        if r > radius - 0.6 {
            return Some(mat::MIRROR_CORNICE);
        }
        let a = datan2(dz, dx) + PI;
        let hour = (a / (PI / 6.0) + 0.5).floor() * (PI / 6.0);
        if r > radius - 1.4 && ((a - hour).abs() * r) < 0.3 {
            return Some(mat::MIRROR_SEAM);
        }
        // Hands at ~10:10.
        let hand = |ang: f32, len: f32| {
            let d = Vec2::new(dcos(ang), dsin(ang));
            let q = Vec2::new(dx, dz);
            let t = q.dot(d);
            t > 0.0 && t < len && (q - d * t).length() < 0.3
        };
        if hand(rad(150.0), radius * 0.55) || hand(rad(30.0), radius * 0.8) {
            return Some(mat::MIRROR_SEAM);
        }
        Some(mat::MIRROR_CORNICE)
    });
    pin(c, center - Vec3::Y * 1.25);
}

/// The two surf chutes: broken gable roofs floating over the north / south ring road, their ridges
/// descending hard-light surf lines.
pub fn chute_lines() -> [SurfLine; 2] {
    [
        SurfLine { a: Vec3::new(-44.0, 44.0, 37.0), b: Vec3::new(-6.0, 27.0, 37.0), face_deg: 58.0, width: 6.0 },
        SurfLine { a: Vec3::new(44.0, 44.0, -37.0), b: Vec3::new(6.0, 27.0, -37.0), face_deg: 58.0, width: 6.0 },
    ]
}

/// Voxel gable under a surf line (inset so no voxel pokes through the hard light).
fn gable(c: &mut City, l: &SurfLine) {
    let d = (l.b - l.a).normalize();
    let r = d.cross(Vec3::Y).normalize();
    let up = r.cross(d).normalize();
    let len = (l.b - l.a).length();
    let (st, ct) = (dsin(rad(l.face_deg)), dcos(rad(l.face_deg)));
    let depth = l.width * st;
    let half_w = l.width * ct;
    let lo = l.a.min(l.b) - Vec3::new(half_w + 1.0, depth + 1.0, half_w + 1.0);
    let hi = l.a.max(l.b) + Vec3::new(half_w + 1.0, 1.0, half_w + 1.0);
    let inset = 0.45;
    c.fill_with(lo, hi, |p, _, _| {
        let q = p - l.a;
        let t = q.dot(d);
        if !(0.3..len - 0.3).contains(&t) {
            return None;
        }
        let x = q.dot(r);
        let y = q.dot(up);
        // Below both faces by `inset` and above the flat underside.
        let below = -y - x.abs() * (st / ct);
        if below * ct < inset || -y > depth * 0.8 {
            return None;
        }
        let end = t < 1.0 || t > len - 1.0;
        Some(if end {
            mat::MIRROR_SEAM
        } else if below * ct < inset + 0.5 {
            if ((t / 1.0).floor() as i32 + (x.abs() * 2.0).floor() as i32) % 3 == 0 { mat::MIRROR_CORNICE } else { mat::MIRROR_SHADE }
        } else {
            mat::MIRROR_STONE
        })
    });
    // Pins every ~8 m along the ridge core.
    let mut t = 3.0;
    while t < len - 2.0 {
        pin(c, l.a + d * t - up * 1.5);
        t += 8.0;
    }
}

struct Frags {
    south_island: Vec3,
    north_clock: Vec3,
    stones_s: Vec<Vec3>,
    stones_n: Vec<Vec3>,
}

fn build_fragments(c: &mut City, blocks: &BlockInfo) -> Frags {
    let id = rot3(0.0, 0.0, 0.0);
    // South spawn island: a torn-out street chunk with a lamp post, reached from the bank roof.
    let island_top = blocks.bank_roof + 4.5;
    let island = Vec3::new(0.0, island_top - 1.25, -24.0);
    shape(c, island, Vec3::new(5.0, 1.25, 4.0), id, |l, h| {
        let rim = (l.x.abs() > h.x - 0.5) as u8 + (l.z.abs() > h.z - 0.5) as u8;
        Some(if l.y > h.y - 0.5 {
            if rim >= 1 {
                mat::MIRROR_SEAM
            } else if l.z.abs() < 0.25 && l.x.rem_euclid(3.0) < 1.5 {
                mat::PAINT_YELLOW
            } else {
                mat::ASPHALT
            }
        } else if rim >= 1 {
            mat::MIRROR_SHARD
        } else {
            mat::MIRROR_SHADE
        })
    });
    pin(c, island);
    c.street_light(Vec3::new(4.0, island_top, -27.0), Vec2::NEG_X, 4.0);
    // Stepping stones: bank roof → S1 → S2 → island (each 1.5 m up, ≤ 3.5 m gap: single jumps).
    let s1 = Vec3::new(13.5, blocks.bank_roof + 1.5, -24.0);
    let s2 = Vec3::new(8.0, s1.y + 1.5, -23.0);
    slab(c, s1 - Vec3::Y * 0.75, Vec3::new(2.0, 0.75, 2.0), rot3(12.0, 0.0, 0.0), mat::MIRROR_CORNICE);
    slab(c, s2 - Vec3::Y * 0.75, Vec3::new(1.5, 0.75, 2.0), rot3(-8.0, 0.0, 0.0), mat::MIRROR_CORNICE);
    // North spawn island: a clock face, reached from the brownstones' roof.
    let clock_top = blocks.brown_roof + 4.5;
    let clock_c = Vec3::new(0.0, clock_top, 24.0);
    clock(c, clock_c, 5.0, 2.5);
    let s3 = Vec3::new(-13.5, blocks.brown_roof + 1.5, 22.0);
    let s4 = Vec3::new(-8.0, s3.y + 1.5, 23.0);
    slab(c, s3 - Vec3::Y * 0.75, Vec3::new(2.0, 0.75, 2.0), rot3(-10.0, 0.0, 0.0), mat::MIRROR_CORNICE);
    slab(c, s4 - Vec3::Y * 0.75, Vec3::new(1.5, 0.75, 2.0), rot3(6.0, 0.0, 0.0), mat::MIRROR_CORNICE);

    // Floating stair flights: SW pocket plaza → the glass tower's sky lobby; NE → library loggia.
    let lobby_y = WALK_Y + 12.0 + 0.5;
    c.stairs(Vec3::new(-8.0, WALK_Y + 1.5, -20.0), Vec2::NEG_X, lobby_y, 2.5, 0.5, mat::MIRROR_CORNICE);
    pin(c, Vec3::new(-15.0, lobby_y - 3.0, -20.0));
    let loggia_y = WALK_Y + 8.0 + 0.5;
    c.stairs(Vec3::new(8.0, WALK_Y + 1.5, 20.0), Vec2::X, loggia_y, 2.5, 1.0, mat::MIRROR_CORNICE);
    pin(c, Vec3::new(14.0, loggia_y - 2.0, 20.0));

    // Sideways fire escapes over the east / west avenues (wall-kick lattices).
    fire_escape(c, Vec3::new(44.0, 32.0, 0.0), Vec3::new(4.0, 6.0, 1.25), rot3(20.0, 0.0, 78.0));
    fire_escape(c, Vec3::new(-44.0, 32.0, 0.0), Vec3::new(4.0, 6.0, 1.25), rot3(200.0, 0.0, -78.0));

    // Cornice shards scattered up toward the hanging towers (sky stepping stones).
    let shards = [
        (Vec3::new(18.0, 40.0, 2.0), (25.0, 18.0, -10.0)),
        (Vec3::new(-18.0, 40.0, -2.0), (205.0, 18.0, -10.0)),
        (Vec3::new(10.0, 47.0, 12.0), (60.0, -20.0, 25.0)),
        (Vec3::new(-10.0, 47.0, -12.0), (240.0, -20.0, 25.0)),
        (Vec3::new(24.0, 52.0, -30.0), (10.0, 30.0, 5.0)),
        (Vec3::new(-24.0, 52.0, 30.0), (190.0, 30.0, 5.0)),
    ];
    for (i, (p, (y, pi, ro))) in shards.iter().enumerate() {
        facade_chunk(c, *p, Vec3::new(3.0, 1.0, 1.5), rot3(*y, *pi, *ro), 300 + i as u32);
    }
    // The library's missing corner, drifting above it.
    let (cl, ch) = LIBRARY_CUT;
    facade_chunk(c, Vec3::new(30.0, blocks.library_roof + 11.0, 20.0), (ch - cl) * 0.5, rot3(35.0, 22.0, -18.0), 77);
    // Its twin over the glass tower: a torn slab of curtain wall.
    shape(c, Vec3::new(-30.0, blocks.glass_roof + 7.0, -20.0), Vec3::new(4.5, 3.0, 0.75), rot3(-30.0, -25.0, 15.0), |l, h| {
        let rim = (l.x.abs() > h.x - 0.5) as u8 + (l.y.abs() > h.y - 0.5) as u8;
        Some(if rim >= 1 {
            mat::MIRROR_SEAM
        } else if (l.x + h.x).rem_euclid(2.0) < 0.5 {
            mat::MULLION
        } else {
            mat::GLASS_BLUE
        })
    });
    pin(c, Vec3::new(-30.0, blocks.glass_roof + 7.0, -20.0));

    // Surf chutes.
    for l in chute_lines() {
        gable(c, &l);
    }
    Frags { south_island: island + Vec3::Y * 1.25, north_clock: clock_c, stones_s: vec![s1, s2], stones_n: vec![s3, s4] }
}

// ------------------------------------------------------------------------------------------------
// Spawns, routes, info

fn pick_spawns(c: &City, fr: &Frags, blocks: &BlockInfo) -> Vec<Vec3> {
    let wanted: [(f32, f32, f32); 10] = [
        (-10.0, 5.0, WALK_Y + 2.0),
        (11.0, 4.0, WALK_Y + 2.0),
        (-23.0, 20.0, blocks.brown_roof + 2.0),
        (32.0, -20.0, blocks.bank_roof + 2.0),
        (54.0, -12.0, STREET_Y + 2.0),
        (-54.0, 12.0, STREET_Y + 2.0),
        (18.0, 38.0, STREET_Y + 2.0),
        (-18.0, -38.0, STREET_Y + 2.0),
        (fr.south_island.x - 1.5, fr.south_island.z, fr.south_island.y + 2.0),
        (fr.north_clock.x + 1.5, fr.north_clock.z - 1.0, fr.north_clock.y + 2.0),
    ];
    let mut out: Vec<Vec3> = Vec::new();
    for (x, z, from) in wanted {
        if let Some(s) = c.spawn_near(x, z, from, &out, 10.0)
            && (s.y - (from - 2.0)).abs() < 1.6
        {
            out.push(s);
        }
    }
    out
}

fn routes(fr: &Frags, blocks: &BlockInfo) -> Vec<Route> {
    let ring = curl_ring(60.0, INSET + 0.9, 6.0);
    let tips = tower_tips();
    vec![
        Route { name: "curl_loop", from: "curl", to: "curl", kind: RouteKind::Mixed, waypoints: ring, anchors: vec![] },
        Route {
            name: "tower_swing",
            from: "bank",
            to: "brownstones",
            kind: RouteKind::Swing,
            waypoints: vec![Vec3::new(36.0, blocks.bank_roof, -20.0), Vec3::new(-23.0, blocks.brown_roof, 20.0)],
            anchors: vec![tips[5], tips[0], tips[1], tips[2]],
        },
        Route {
            name: "fragment_hop",
            from: "bank",
            to: "street_island",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(25.0, blocks.bank_roof, -24.0), fr.stones_s[0], fr.stones_s[1], fr.south_island],
            anchors: vec![],
        },
        Route {
            name: "clock_hop",
            from: "brownstones",
            to: "clock",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(-23.0, blocks.brown_roof, 22.0), fr.stones_n[0], fr.stones_n[1], fr.north_clock],
            anchors: vec![],
        },
        Route {
            name: "avenue_launch",
            from: "plaza",
            to: "curl",
            kind: RouteKind::Ground,
            waypoints: vec![Vec3::new(17.0, STREET_Y, 0.0), Vec3::new(HALF_X, STREET_Y, 0.0)],
            anchors: vec![],
        },
    ]
}

/// Build Mirrorfold. Deterministic for a given seed (the layout is fixed; details vary).
pub fn generate_mirror(seed: u64) -> (VoxelWorld, MapInfo) {
    let mut rng = Rng::new(seed ^ 0x6d69_7272_6f72);
    let mut c = City::new(VoxelWorld::centered(MIRROR_SIZE_VOXELS), seed ^ 0x666f_6c64);
    build_ground(&mut c);
    let cb = curl_blocks(&mut rng);
    build_curl(&mut c, &cb);
    let blocks = build_blocks(&mut c);
    build_props(&mut c, &mut rng);
    for (i, t) in hanging_towers().iter().enumerate() {
        building(&mut c, t, 40 + i as u32);
    }
    let fr = build_fragments(&mut c, &blocks);
    let spawns = pick_spawns(&c, &fr, &blocks);
    let (world, pruned) = c.finish_counted();

    let mut surf_lines = curl_surf_lines();
    surf_lines.extend(chute_lines());
    let side = CURL_R + SHELL + 2.0;
    let arena = ArenaInfo {
        spawns,
        bounds: Aabb { min: Vec3::new(-HALF_X - side, VOID_Y, -HALF_Z - side), max: Vec3::new(HALF_X + side, 150.0, HALF_Z + side) },
        void_y: VOID_Y,
        center: Vec3::new(0.0, WALK_Y, 0.0),
        // High, warm key light from the rift overhead; cool violet sky in the shadows.
        sun_dir: Vec3::new(0.30, 0.88, 0.36).normalize(),
        sky_zenith: [0.16, 0.15, 0.30],
        sky_horizon: [0.86, 0.84, 0.80],
        fog_color: [0.58, 0.58, 0.68],
        ambient: 0.55,
        gravity: None,
        surf_lines,
        water_sheets: Vec::new(),
        water_walk: false,
        slipstream: None,
        mechanics: Default::default(),
        interactables: Default::default(),
    };
    let districts = vec![
        District { name: "plaza", center: Vec3::new(0.0, WALK_Y, 6.0), area: [PLAZA.min.x, PLAZA.min.y, PLAZA.max.x, PLAZA.max.y] },
        District {
            name: "library",
            center: Vec3::new(31.0, blocks.library_roof, 20.0),
            area: [LIBRARY.min.x, LIBRARY.min.y, LIBRARY.max.x, LIBRARY.max.y],
        },
        District {
            name: "brownstones",
            center: Vec3::new(-23.0, blocks.brown_roof, 20.0),
            area: [BROWN_W.min.x, BROWN_W.min.y, BROWN_E.max.x, BROWN_E.max.y],
        },
        District {
            name: "glass_tower",
            center: Vec3::new(-31.0, blocks.glass_roof, -20.0),
            area: [GLASS_TOWER.min.x, GLASS_TOWER.min.y, GLASS_TOWER.max.x, GLASS_TOWER.max.y],
        },
        District { name: "bank", center: Vec3::new(31.0, blocks.bank_roof, -20.0), area: [BANK.min.x, BANK.min.y, BANK.max.x, BANK.max.y] },
        District { name: "street_island", center: fr.south_island, area: [-5.0, -28.0, 5.0, -20.0] },
        District { name: "clock", center: fr.north_clock, area: [-5.0, 19.0, 5.0, 29.0] },
        District {
            name: "curl",
            center: Vec3::new(HALF_X + 12.0, STREET_Y + 6.0, 0.0),
            area: [-HALF_X - CURL_R, -HALF_Z - CURL_R, HALF_X + CURL_R, HALF_Z + CURL_R],
        },
    ];
    let routes = routes(&fr, &blocks);
    let info = MapInfo {
        name: "Mirrorfold",
        arena,
        water_y: -1000.0,
        water_color: [0.40, 0.50, 0.70],
        water_deep: [0.10, 0.10, 0.20],
        sun_color: [1.0, 0.90, 0.76],
        sky_mid: [0.52, 0.50, 0.66],
        fog_start: 50.0,
        fog_end: 300.0,
        neon_boost: 0.85,
        districts,
        routes,
        pruned_voxels: pruned,
    };
    (world, info)
}
