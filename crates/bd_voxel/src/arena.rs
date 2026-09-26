//! Arena generation.
//!
//! [`generate_arena`] builds "the Archipelago": a hand-designed movement arena whose skeleton is fixed
//! (so it always plays well) and whose details (exact positions, heights, trees, rubble, noise) vary
//! with the seed. [`generate_flat_test`] builds a flat movement lab.
//!
//! Generation is deterministic across machines (integer hashes, IEEE `+ - * / sqrt floor`, and our
//! own polynomial trig), so every client can generate the map locally from the seed.
//!
//! ## Archipelago layout (metres; +X east, +Z north, Y up)
//! - **Heart** (0, 40, 0): central contested island. A giant vertical hard-light *lantern ring*
//!   (jade torus, top ≈ 73 m) straddles the east–west spine; you can run under its lower bar or
//!   swing through it. Ring of broken colonnade pillars for cover + low anchors. Deep inverted-cone
//!   underside to swing beneath.
//! - **The Spine**: a ~170 m east–west lane — Long Meadow (E, 36 m) → sloped land bridge → Heart →
//!   walkable stone arch → Ruins (W, 42 m). Built for bunny-hop speed.
//! - Six satellites alternating low/high around the Heart at ~66 m: Long Meadow (E, mid), Spire
//!   Garden (NE, high, crystal-tipped needles), Grove (NW, low, giant autumn trees whose canopies
//!   reach 60 m), Ruins (W, mid, hollow towers/walls), Crag (SW, high, terraces + overhanging shelf),
//!   Sandbar (SE, low, blossom trees).
//! - Outer loop: small stepping-stone islands between satellites (gaps of 8–14 m: good bhops or a
//!   quick tether), two of them carrying tall anchors.
//! - Sky anchors: floating rocks at 60–78 m over every Heart↔satellite gap, and big arches
//!   (NE↔E, SW↔SE) to swing under or surf over.
//! - Every island/rock has a small indestructible core; everything else can be carved.

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use crate::material::{MaterialDef, mat, material_def};
use crate::noise::{self, PI, Rng, TAU, datan2, dcos, dsin, fbm2, smoothstep, value2, value3};
use crate::world::VoxelWorld;

/// Everything the sim/client needs to know about a generated map besides its voxels.
#[derive(Clone, Debug, PartialEq)]
pub struct ArenaInfo {
    /// Player spawn points: feet positions standing on solid ground, with head room. ≥ 8, spread out.
    pub spawns: Vec<Vec3>,
    /// Playable volume in metres. Leaving it (or falling below `void_y`) is an out-of-bounds KO.
    pub bounds: Aabb,
    /// Falling below this height is a void KO.
    pub void_y: f32,
    /// The central contested landmark (spawns face it; bots/camera can use it).
    pub center: Vec3,
    /// Unit vector pointing *towards* the sun.
    pub sun_dir: Vec3,
    /// Sky colour at the zenith (sRGB 0–1).
    pub sky_zenith: [f32; 3],
    /// Sky colour at the horizon / below (sRGB 0–1).
    pub sky_horizon: [f32; 3],
    /// Distance fog colour (sRGB 0–1).
    pub fog_color: [f32; 3],
    /// Suggested ambient light level (0–1).
    pub ambient: f32,
    /// Map-defined gravity by altitude (Orbit: normal gravity on the islands, fading to ~zero-g
    /// above). `None` = full gravity everywhere (every other map; the sim skips the hook entirely).
    pub gravity: Option<GravityProfile>,
    /// SURF: CS-style surf lines (smooth hard-light prisms: the sim builds their colliders with
    /// `bd_sim::movement::surf_line`, the client draws them). Not voxels: a 0.5 m voxel staircase is
    /// walkable steps, not a surfable slope — and hard light can't be carved away.
    pub surf_lines: Vec<SurfLine>,
    /// Flat water surfaces that are not voxels (Valley of the End's river and upper pool): the client
    /// draws them and splashes; what's under them is the voxel bed. Empty on every other map (their
    /// water is a KO plane or solid voxels).
    pub water_sheets: Vec<WaterSheet>,
    /// WATER-WALK (chakra walking): the tops of `water_sheets` are ground for players — run, hop,
    /// slide on the river (`bd_sim::movement::push_water_colliders`, host and prediction alike).
    /// Constructs, projectiles and hooks ignore it. `false` everywhere but the Valley.
    pub water_walk: bool,
}

/// A flat water surface: a rectangle at height `center.y` spanning `half` metres along its local
/// x / z axes, turned `yaw` radians about +Y (glam `Quat::from_rotation_y`), over `depth` metres of
/// water down to the voxel bed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSheet {
    pub center: Vec3,
    pub half: Vec2,
    pub yaw: f32,
    pub depth: f32,
}

impl WaterSheet {
    /// Is the horizontal position of `p` over this sheet (within `margin` metres of its edges)?
    pub fn covers(&self, p: Vec3, margin: f32) -> bool {
        let q = glam::Quat::from_rotation_y(-self.yaw) * Vec3::new(p.x - self.center.x, 0.0, p.z - self.center.z);
        q.x.abs() <= self.half.x + margin && q.z.abs() <= self.half.y + margin
    }
}

/// A CS-style surf line: a triangular prism of hard light whose ridge runs from `a` to `b`
/// (descending when `b` is lower), each side a face `face_deg` from horizontal with `width` metres of
/// slope. Surf it on either face; the ends dump you onto whatever is below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfLine {
    pub a: Vec3,
    pub b: Vec3,
    pub face_deg: f32,
    pub width: f32,
}

/// Gravity by altitude — the one data-driven sim hook for maps (DESIGN.md "Maps: Orbit").
///
/// Full gravity at or below `full_below`, smoothly (Hermite) fading to `min_scale` at `zero_above`
/// and constant above. Pure IEEE `+ - * /` so the host, predicting clients and every generated copy
/// of the map agree bit-for-bit. Players get it through `MoveModifiers::gravity_scale`, constructs,
/// projectiles and (client-side) debris multiply their own gravity by it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GravityProfile {
    pub full_below: f32,
    pub zero_above: f32,
    /// Scale at and above `zero_above` (keep > 0 so anything adrift eventually sinks back).
    pub min_scale: f32,
}

impl GravityProfile {
    /// Gravity multiplier at height `y` (1 = normal).
    #[inline]
    pub fn scale_at(&self, y: f32) -> f32 {
        if y.is_nan() || y <= self.full_below {
            // Also catches NaN (never happens for live bodies; full gravity is the safe answer).
            return 1.0;
        }
        if y >= self.zero_above {
            return self.min_scale;
        }
        let span = self.zero_above - self.full_below;
        let t = if span > 0.0 { ((y - self.full_below) / span).min(1.0) } else { 1.0 };
        let s = t * t * (3.0 - 2.0 * t);
        1.0 + (self.min_scale - 1.0) * s
    }
}

impl ArenaInfo {
    /// Gravity multiplier at height `y` on this map (1 everywhere without a profile).
    #[inline]
    pub fn gravity_scale_at(&self, y: f32) -> f32 {
        self.gravity.map_or(1.0, |g| g.scale_at(y))
    }
}

const VS: f32 = VOXEL_SIZE;

#[inline]
pub(crate) fn vx(m: f32) -> i32 {
    (m / VS).floor() as i32
}

#[inline]
pub(crate) fn vcenter(v: IVec3) -> Vec3 {
    (v.as_vec3() + Vec3::splat(0.5)) * VS
}

pub(crate) fn is_foliage(m: Material) -> bool {
    matches!(material_def(m).debris, crate::material::DebrisClass::Leaves | crate::material::DebrisClass::Wood)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Surface {
    Grass,
    /// Grass with a sandy rim.
    Beach,
    /// Bare stone with moss/grass patches.
    Rocky,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IslandSpec {
    pub(crate) center: Vec2,
    pub(crate) top: f32,
    pub(crate) rx: f32,
    pub(crate) rz: f32,
    pub(crate) rot: f32,
    /// Extra thickness at the centre (the inverted-cone underside), metres.
    pub(crate) depth: f32,
    pub(crate) hills: f32,
    /// Height change per metre along x/z (sloped land bridges).
    pub(crate) slope: Vec2,
    /// Normalised radius inside which hills are suppressed (flat arena floors).
    pub(crate) flat: f32,
    pub(crate) surface: Surface,
    pub(crate) crystals: Material,
    pub(crate) core: bool,
    /// How far the surface drops towards the edge (m), starting at normalised radius `rim_start`.
    /// A large rim with an early start turns a plateau into a smooth walkable mound.
    pub(crate) rim: f32,
    pub(crate) rim_start: f32,
}

impl IslandSpec {
    pub(crate) fn new(center: Vec2, top: f32, r: f32) -> Self {
        Self {
            center,
            top,
            rx: r,
            rz: r,
            rot: 0.0,
            depth: r * 0.9,
            hills: 0.8,
            slope: Vec2::ZERO,
            flat: 0.0,
            surface: Surface::Grass,
            crystals: mat::CRYSTAL_CYAN,
            core: true,
            rim: 1.2,
            rim_start: 0.8,
        }
    }
}

/// Generation context: the world being built plus deterministic randomness.
pub(crate) struct Gen {
    pub(crate) w: VoxelWorld,
    pub(crate) rng: Rng,
    /// Indestructible cores, placed last so later fills can't overwrite them: (centre, radius).
    pub(crate) cores: Vec<(Vec3, f32)>,
}

impl Gen {
    pub(crate) fn new(w: VoxelWorld, seed: u64) -> Self {
        Self { w, rng: Rng::new(seed), cores: Vec::new() }
    }

    pub(crate) fn seed(&mut self) -> u32 {
        self.rng.next_u32()
    }

    /// Top of the first non-foliage solid at or below `from_y` in the column containing (x, z).
    pub(crate) fn ground(&self, x: f32, z: f32, from_y: f32) -> Option<f32> {
        let (cx, cz) = (vx(x), vx(z));
        let lo = self.w.voxel_min().y;
        (lo..=vx(from_y)).rev().find_map(|y| {
            let m = self.w.get(IVec3::new(cx, y, cz));
            (m != AIR && !is_foliage(m)).then(|| (y + 1) as f32 * VS)
        })
    }

    // ----- islands -------------------------------------------------------------------------------

    pub(crate) fn island(&mut self, s: IslandSpec) {
        let seed = self.seed();
        let reach = s.rx.max(s.rz) * 1.2 + 1.0;
        let (x0, x1) = (vx(s.center.x - reach), vx(s.center.x + reach));
        let (z0, z1) = (vx(s.center.y - reach), vx(s.center.y + reach));
        let (sn, cs) = (dsin(s.rot), dcos(s.rot));
        for vz in z0..=z1 {
            for vxx in x0..=x1 {
                let p = Vec2::new((vxx as f32 + 0.5) * VS, (vz as f32 + 0.5) * VS);
                let q = p - s.center;
                let l = Vec2::new(q.x * cs + q.y * sn, -q.x * sn + q.y * cs);
                let e = ((l.x / s.rx) * (l.x / s.rx) + (l.y / s.rz) * (l.y / s.rz)).sqrt();
                let warp = 0.16 * fbm2(p.x * 0.05, p.y * 0.05, seed, 3);
                let d = e + warp;
                if d >= 1.0 {
                    continue;
                }
                let hill_mask = if s.flat > 0.0 { smoothstep(s.flat, s.flat + 0.35, d) } else { 1.0 };
                let hills = s.hills * fbm2(p.x * 0.035, p.y * 0.035, seed ^ 1, 3) * hill_mask;
                let rim = smoothstep(s.rim_start, 1.0, d) * s.rim;
                let top = s.top + s.slope.dot(q) + hills - rim;
                let jag = 0.5 + 0.5 * fbm2(p.x * 0.11, p.y * 0.11, seed ^ 2, 2);
                let od = 1.0 - d;
                let depth = 1.5 + s.depth * od * od.sqrt().sqrt() * (0.7 + 0.6 * jag);
                let ty = vx(top);
                let by = vx(top - depth);
                let dirt = 2 + (jag * 3.0) as i32;
                let (surf, sub) = match s.surface {
                    Surface::Grass => (mat::GRASS, mat::DIRT),
                    Surface::Beach if d > 0.72 + 0.1 * jag => (mat::SAND, mat::SAND),
                    Surface::Beach => (mat::GRASS, mat::DIRT),
                    Surface::Rocky => {
                        if fbm2(p.x * 0.09, p.y * 0.09, seed ^ 3, 2) > 0.1 {
                            (mat::GRASS, mat::DIRT)
                        } else {
                            (mat::STONE, mat::STONE)
                        }
                    }
                };
                let deep_split = by + ((ty - by) as f32 * 0.45) as i32;
                self.w.fill_column(vxx, vz, by, deep_split, mat::DEEP_STONE);
                self.w.fill_column(vxx, vz, deep_split, ty - dirt, mat::STONE);
                self.w.fill_column(vxx, vz, ty - dirt, ty, sub);
                self.w.fill_column(vxx, vz, ty, ty + 1, surf);
                // Glowing crystal clusters on the underside.
                let cn = fbm2(p.x * 0.16, p.y * 0.16, seed ^ 4, 2);
                if cn > 0.42 && d < 0.85 {
                    let len = 1 + ((cn - 0.42) * 14.0) as i32;
                    self.w.fill_column(vxx, vz, by - len / 2, by + 2, s.crystals);
                }
            }
        }
        if s.core {
            let thick = 1.5 + s.depth * 0.7;
            let r = (thick * 0.16).clamp(0.75, 2.5);
            let c = Vec3::new(s.center.x, s.top - thick * 0.5 - 0.5, s.center.y);
            self.cores.push((c, r));
        }
    }

    // ----- structures ----------------------------------------------------------------------------

    /// A tapering stone needle. Returns the tip position.
    pub(crate) fn spire(&mut self, x: f32, z: f32, from_y: f32, height: f32, radius: f32, tip: Option<Material>) -> Vec3 {
        let Some(g) = self.ground(x, z, from_y) else { return Vec3::new(x, from_y, z) };
        let seed = self.seed();
        let lean = Vec2::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0)) * height * 0.07;
        let top = g + height;
        let reach = radius * 1.8 + lean.length() + 1.0;
        let lo = IVec3::new(vx(x - reach), vx(g - 4.0), vx(z - reach));
        let hi = IVec3::new(vx(x + reach), vx(top), vx(z + reach));
        let base = Vec2::new(x, z);
        self.w.fill_with(lo, hi, |v, _| {
            let p = vcenter(v);
            let t = ((p.y - g) / height).clamp(0.0, 1.0);
            let c = base + lean * t * t;
            let flare = 1.0 + 0.6 * (1.0 - t * 6.0).max(0.0);
            let r = radius * (1.0 - 0.8 * t) * flare * (1.0 + 0.22 * value3(p.x * 0.35, p.y * 0.35, p.z * 0.35, seed));
            if (Vec2::new(p.x, p.z) - c).length() > r {
                return None;
            }
            Some(match tip {
                Some(m) if t > 0.8 => m,
                _ => {
                    let band = (p.y * 0.6 + 0.8 * value2(p.x * 0.2, p.z * 0.2, seed ^ 7)).floor() as i32;
                    if band.rem_euclid(3) == 0 { mat::DEEP_STONE } else { mat::STONE }
                }
            })
        });
        Vec3::new(x + lean.x, top, z + lean.y)
    }

    /// Noisy leaf blob (only fills air).
    pub(crate) fn blob(&mut self, c: Vec3, r: Vec3, m: Material) {
        let seed = self.seed();
        let lo = ((c - r * 1.3) / VS).floor().as_ivec3();
        let hi = ((c + r * 1.3) / VS).floor().as_ivec3();
        self.w.fill_with(lo, hi, |v, old| {
            if old != AIR {
                return None;
            }
            let p = vcenter(v);
            let d = (p - c) / r;
            let e = d.length() + 0.24 * value3(p.x * 0.3, p.y * 0.3, p.z * 0.3, seed);
            (e < 1.0).then_some(m)
        });
    }

    /// Sweep spheres along a quadratic Bézier.
    pub(crate) fn sweep(&mut self, a: Vec3, ctrl: Vec3, b: Vec3, r0: f32, r1: f32, m: Material, only_air: bool) {
        let len = (ctrl - a).length() + (b - ctrl).length();
        let n = ((len / 0.3) as i32).max(2);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let p = a * (1.0 - t) * (1.0 - t) + ctrl * 2.0 * t * (1.0 - t) + b * t * t;
            let r = r0 + (r1 - r0) * t;
            self.w.fill_sphere(p, r, m, only_air);
        }
    }

    /// A big tree: curved trunk with root flare, a few branches, noisy leaf canopies.
    pub(crate) fn tree(&mut self, x: f32, z: f32, from_y: f32, height: f32, trunk_r: f32, leaves: Material, canopy_r: f32) {
        let Some(g) = self.ground(x, z, from_y) else { return };
        let wood = if self.rng.chance(0.3) { mat::DARK_WOOD } else { mat::WOOD };
        let base = Vec3::new(x, g - 1.5, z);
        let top =
            Vec3::new(x + self.rng.range(-1.0, 1.0) * height * 0.08, g + height * 0.72, z + self.rng.range(-1.0, 1.0) * height * 0.08);
        let ctrl = (base + top) * 0.5 + Vec3::new(self.rng.range(-1.0, 1.0), 0.0, self.rng.range(-1.0, 1.0)) * height * 0.08;
        // Root flare, then trunk.
        self.w.fill_sphere(Vec3::new(x, g - 0.5, z), trunk_r * 2.0, wood, false);
        let n = ((height / 0.25) as i32).max(4);
        let bez = |t: f32| base * (1.0 - t) * (1.0 - t) + ctrl * 2.0 * t * (1.0 - t) + top * t * t;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let flare = 1.0 + 0.9 * (1.0 - t * 5.0).max(0.0) * (1.0 - t * 5.0).max(0.0);
            self.w.fill_sphere(bez(t), trunk_r * (1.0 - 0.45 * t) * flare, wood, false);
        }
        // Branches → side canopies.
        let nb = self.rng.int(3, 6);
        let a0 = self.rng.range(0.0, TAU);
        for k in 0..nb {
            let t0 = self.rng.range(0.5, 0.85);
            let start = bez(t0);
            let ang = a0 + TAU * k as f32 / nb as f32 + self.rng.range(-0.4, 0.4);
            let dir = Vec3::new(dcos(ang), self.rng.range(0.5, 0.9), dsin(ang)).normalize();
            let len = height * self.rng.range(0.22, 0.32);
            let end = start + dir * len;
            let mid = start + dir * len * 0.5 + Vec3::Y * len * 0.1;
            self.sweep(start, mid, end, trunk_r * 0.5, trunk_r * 0.28, wood, false);
            let cr = canopy_r * self.rng.range(0.55, 0.75);
            self.blob(end + Vec3::Y * cr * 0.3, Vec3::new(cr, cr * 0.62, cr), leaves);
        }
        self.blob(top + Vec3::Y * canopy_r * 0.3, Vec3::new(canopy_r, canopy_r * 0.62, canopy_r), leaves);
    }

    /// Hollow ruined tower with a jagged broken top, windows, a door and a broken mid floor.
    pub(crate) fn tower(&mut self, x: f32, z: f32, from_y: f32, radius: f32, height: f32, square: bool, door_to: Vec2) {
        let Some(g) = self.ground(x, z, from_y) else { return };
        let seed = self.seed();
        let c = Vec2::new(x, z);
        let door_dir = (door_to - c).normalize_or_zero();
        let wall = 1.0;
        let floor_h = if height > 14.0 { self.rng.range(8.5, 10.5) } else { f32::INFINITY };
        let lo = IVec3::new(vx(x - radius - 1.0), vx(g - 2.5), vx(z - radius - 1.0));
        let hi = IVec3::new(vx(x + radius + 1.0), vx(g + height), vx(z + radius + 1.0));
        self.w.fill_with(lo, hi, |v, _| {
            let p = vcenter(v);
            let rel = p.y - g;
            let q = Vec2::new(p.x, p.z) - c;
            let dist = if square { q.x.abs().max(q.y.abs()) } else { q.length() };
            if dist > radius {
                return None;
            }
            let hmax = height * (0.68 + 0.32 * (0.5 + 0.5 * value2(p.x * 0.16, p.z * 0.16, seed)));
            if rel > hmax {
                return if rel < height && dist < radius - wall { Some(AIR) } else { None };
            }
            let brick =
                if rel < 1.5 || value3(p.x * 0.25, p.y * 0.25, p.z * 0.25, seed ^ 3) > 0.3 { mat::MOSSY_BRICK } else { mat::RUIN_BRICK };
            if dist < radius - wall {
                // Interior.
                if rel < 0.0 {
                    return Some(brick);
                }
                let f = rel - floor_h;
                if (0.0..0.5).contains(&f) && value2(p.x * 0.3, p.z * 0.3, seed ^ 5) < 0.3 {
                    return Some(mat::RUIN_BRICK);
                }
                return Some(AIR);
            }
            // Wall: door, windows.
            if rel >= 0.0 && rel < 3.0 && q.normalize_or_zero().dot(door_dir) > 0.9 {
                return Some(AIR);
            }
            let wy = (rel - 2.0).rem_euclid(6.0);
            if rel > 3.0 && (1.5..3.4).contains(&wy) {
                let s = (datan2(q.y, q.x) / TAU * 8.0).rem_euclid(1.0);
                if (0.36..0.64).contains(&s) {
                    return Some(AIR);
                }
            }
            Some(brick)
        });
    }

    /// Square broken pillar (cover / low anchor). `lying` lays it on its side along `lying_dir`.
    pub(crate) fn pillar(&mut self, x: f32, z: f32, from_y: f32, height: f32, half: f32) {
        let Some(g) = self.ground(x, z, from_y) else { return };
        let seed = self.seed();
        let lo = IVec3::new(vx(x - half), vx(g - 2.0), vx(z - half));
        let hi = IVec3::new(vx(x + half) - 1, vx(g + height), vx(z + half) - 1);
        self.w.fill_with(lo, hi, |v, _| {
            let p = vcenter(v);
            let rel = p.y - g;
            let jag = height - 1.5 * (0.5 + 0.5 * value2(p.x * 0.9, p.z * 0.9, seed));
            if rel > jag {
                return None;
            }
            // Plinth and capital-ish bands.
            let m = if rel < 1.0 || value3(p.x * 0.3, p.y * 0.3, p.z * 0.3, seed) > 0.35 { mat::MOSSY_BRICK } else { mat::RUIN_BRICK };
            Some(m)
        });
    }

    /// A fallen column segment lying on the ground (cover).
    pub(crate) fn fallen_pillar(&mut self, x: f32, z: f32, from_y: f32, len: f32, along_x: bool) {
        let Some(g) = self.ground(x, z, from_y) else { return };
        let (hx, hz) = if along_x { (len * 0.5, 0.75) } else { (0.75, len * 0.5) };
        let lo = IVec3::new(vx(x - hx), vx(g - 0.5), vx(z - hz));
        let hi = IVec3::new(vx(x + hx) - 1, vx(g + 1.5) - 1, vx(z + hz) - 1);
        self.w.fill_box(lo, hi, mat::RUIN_BRICK);
    }

    /// Ruined wall segment from `a` to `b` with a jagged top and occasional breaches.
    pub(crate) fn wall(&mut self, a: Vec2, b: Vec2, from_y: f32, height: f32, thick: f32) {
        let seed = self.seed();
        let len = (b - a).length();
        let n = (len / (VS * 0.5)) as i32;
        let dir = (b - a) / len;
        let side = Vec2::new(-dir.y, dir.x);
        let mut done = std::collections::BTreeSet::new();
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let breach = value2(t * len * 0.25, 0.5, seed) < -0.45;
            if breach {
                continue;
            }
            let h = height * (0.55 + 0.45 * (0.5 + 0.5 * value2(t * len * 0.35, 3.5, seed ^ 1)));
            let mut s = -thick * 0.5;
            while s <= thick * 0.5 {
                let p = a + dir * t * len + side * s;
                let key = (vx(p.x), vx(p.y));
                s += VS * 0.5;
                if !done.insert(key) {
                    continue;
                }
                let Some(g) = self.ground(p.x, p.y, from_y) else { continue };
                let (y0, y1) = (vx(g - 1.5), vx(g + h));
                for y in y0..=y1 {
                    let v = IVec3::new(key.0, y, key.1);
                    let m = if y - vx(g) < 2 || noise::unit(noise::hash_v(v, seed)) < 0.25 { mat::MOSSY_BRICK } else { mat::RUIN_BRICK };
                    self.w.set(v, m);
                }
            }
        }
    }

    pub(crate) fn boulder(&mut self, x: f32, z: f32, from_y: f32, r: f32) {
        let Some(g) = self.ground(x, z, from_y) else { return };
        let seed = self.seed();
        let c = Vec3::new(x, g + r * 0.3, z);
        let rad = Vec3::new(r * self.rng.range(0.9, 1.3), r * 0.8, r * self.rng.range(0.9, 1.3));
        let lo = ((c - rad * 1.3) / VS).floor().as_ivec3();
        let hi = ((c + rad * 1.3) / VS).floor().as_ivec3();
        self.w.fill_with(lo, hi, |v, old| {
            let p = vcenter(v);
            let e = ((p - c) / rad).length() + 0.2 * value3(p.x * 0.4, p.y * 0.4, p.z * 0.4, seed);
            if e >= 1.0 {
                return None;
            }
            let top_moss = (p.y - c.y) > rad.y * 0.4 && value2(p.x * 0.5, p.z * 0.5, seed ^ 9) > -0.1;
            let _ = old;
            Some(if top_moss { mat::GRASS } else { mat::STONE })
        });
    }

    /// Natural stone arch from `a` to `b` (ends buried), rising `rise` metres above the chord.
    /// The top is grassed so it reads as walkable.
    pub(crate) fn arch(&mut self, a: Vec3, b: Vec3, rise: f32, thick: f32) {
        let seed = self.seed();
        let len = (b - a).length();
        let n = (len / 0.35) as i32;
        let mut lo = IVec3::MAX;
        let mut hi = IVec3::MIN;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let s = dsin(PI * t);
            let p = a + (b - a) * t + Vec3::Y * rise * s;
            let foot = 1.0 - s;
            let r = thick * (1.0 + 0.8 * foot * foot) * (1.0 + 0.15 * value2(t * 9.0, 0.0, seed));
            let (plo, phi) = (((p - r) / VS).floor().as_ivec3(), ((p + r) / VS).floor().as_ivec3());
            lo = lo.min(plo);
            hi = hi.max(phi);
            self.w.fill_with(plo, phi, |v, _| {
                let q = vcenter(v);
                if (q - p).length() > r {
                    return None;
                }
                let band = (q.y * 0.9 + 0.6 * value2(q.x * 0.15, q.z * 0.15, seed)).floor() as i32;
                Some(match band.rem_euclid(4) {
                    0 => mat::TERRACOTTA,
                    1 => mat::DEEP_STONE,
                    _ => mat::STONE,
                })
            });
        }
        // Grass the exposed top of the arch (only the arch's own upper voxels).
        let min_y = vx(a.y.min(b.y) + 1.0);
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                for y in (min_y..=hi.y).rev() {
                    let v = IVec3::new(x, y, z);
                    let m = self.w.get(v);
                    if m == AIR {
                        continue;
                    }
                    if matches!(m, mat::STONE | mat::DEEP_STONE | mat::TERRACOTTA) {
                        self.w.set(v, mat::GRASS);
                    }
                    break;
                }
            }
        }
    }

    /// The Heart's landmark: a vertical hard-light lantern ring (torus in the YZ plane) with gold
    /// bands, held up by two slanted pylons.
    pub(crate) fn lantern_ring(&mut self, c: Vec3, major: f32, minor: f32, ground_y: f32) {
        let lo = ((c - Vec3::new(minor + 1.0, major + minor + 1.0, major + minor + 1.0)) / VS).floor().as_ivec3();
        let hi = ((c + Vec3::new(minor + 1.0, major + minor + 1.0, major + minor + 1.0)) / VS).floor().as_ivec3();
        self.w.fill_with(lo, hi, |v, _| {
            let q = vcenter(v) - c;
            let rin = (q.y * q.y + q.z * q.z).sqrt() - major;
            let d = (rin * rin + q.x * q.x).sqrt();
            if d > minor {
                return None;
            }
            let s = (datan2(q.y, q.z) / TAU * 12.0).rem_euclid(1.0);
            Some(if !(0.07..0.93).contains(&s) { mat::CRYSTAL_GOLD } else { mat::JADE })
        });
        // Pylons from the ground to the ring's lower flanks.
        for side in [-1.0f32, 1.0] {
            let ang = -PI * 0.5 + side * 0.75;
            let attach = c + Vec3::new(0.0, dsin(ang) * major, dcos(ang) * major);
            let foot = Vec3::new(0.0, ground_y - 2.0, side * (major * 0.95));
            let mid = (attach + foot) * 0.5 + Vec3::new(0.0, -1.0, side * 1.5);
            self.sweep(foot, mid, attach, 1.6, 1.0, mat::JADE, false);
        }
    }

    /// Stamp the island cores (indestructible, replaces only solid voxels) and register anchors.
    pub(crate) fn place_cores(&mut self) {
        let cores = std::mem::take(&mut self.cores);
        for (c, r) in cores {
            let (lo, hi) = (((c - r) / VS).floor().as_ivec3(), ((c + r) / VS).floor().as_ivec3());
            let cv = (c / VS).floor().as_ivec3();
            self.w.fill_with(lo, hi, |v, old| {
                let inside = (vcenter(v) - c).length() <= r || v == cv;
                (inside && old != AIR).then_some(mat::CORE)
            });
            if self.w.get(cv) == mat::CORE {
                self.w.add_anchor_hint(cv);
            }
        }
    }

    /// Find a spawn near (x, z): standing on non-foliage ground with 3 m of clear head room, and at
    /// least `SPAWN_MIN_DIST` from every spawn in `taken`.
    pub(crate) fn spawn_near(&self, x: f32, z: f32, from_y: f32, taken: &[Vec3]) -> Option<Vec3> {
        for ring in 0..12 {
            let steps = if ring == 0 { 1 } else { 8 };
            for k in 0..steps {
                let a = TAU * k as f32 / steps as f32;
                let (px, pz) = (x + dcos(a) * ring as f32 * 1.5, z + dsin(a) * ring as f32 * 1.5);
                if let Some(p) = spawn_ok(&self.w, px, pz, from_y) {
                    if taken.iter().all(|t| t.distance(p) >= SPAWN_MIN_DIST) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }
}

/// Is (x, z) a valid spawn column? Returns the feet position if so.
pub(crate) fn spawn_ok(w: &VoxelWorld, x: f32, z: f32, from_y: f32) -> Option<Vec3> {
    let (cx, cz) = (vx(x), vx(z));
    let top = (w.voxel_min().y..=vx(from_y)).rev().find(|&y| w.get(IVec3::new(cx, y, cz)) != AIR)?;
    let ground: &MaterialDef = material_def(w.get(IVec3::new(cx, top, cz)));
    if is_foliage(w.get(IVec3::new(cx, top, cz))) || ground.indestructible {
        return None;
    }
    // Hull 0.8 m wide: check a 3×3 voxel footprint for head room and flat-ish footing.
    for dz in -1..=1 {
        for dx in -1..=1 {
            for dy in 1..=6 {
                if w.get(IVec3::new(cx + dx, top + dy, cz + dz)) != AIR {
                    return None;
                }
            }
            // Need ground under the whole footprint (not a ledge edge).
            if w.get(IVec3::new(cx + dx, top, cz + dz)) == AIR && w.get(IVec3::new(cx + dx, top - 1, cz + dz)) == AIR {
                return None;
            }
        }
    }
    Some(Vec3::new((cx as f32 + 0.5) * VS, (top + 1) as f32 * VS, (cz as f32 + 0.5) * VS))
}

/// Minimum distance between two spawn points (m).
const SPAWN_MIN_DIST: f32 = 12.0;

pub(crate) fn polar(angle_deg: f32, r: f32) -> Vec2 {
    let a = angle_deg * (PI / 180.0);
    Vec2::new(dcos(a) * r, dsin(a) * r)
}

/// World dimensions for the archipelago, in voxels (192 × 96 × 192 m).
pub const ARENA_SIZE_VOXELS: IVec3 = IVec3::new(384, 192, 384);

/// Build the Archipelago arena. Deterministic for a given seed.
pub fn generate_arena(seed: u64) -> (VoxelWorld, ArenaInfo) {
    let mut g = Gen::new(VoxelWorld::centered(ARENA_SIZE_VOXELS), seed);
    let j = |g: &mut Gen, amt: f32| Vec2::new(g.rng.range(-amt, amt), g.rng.range(-amt, amt));

    // ---- The Heart -------------------------------------------------------------------------------
    let heart_top = 40.0;
    g.island(IslandSpec { depth: 21.0, hills: 0.5, flat: 0.45, ..IslandSpec::new(Vec2::ZERO, heart_top, 26.0) });
    // Two rolling mounds (N/S of the spine) for height, line-of-sight breaks and ramp jumps.
    for side in [-1.0f32, 1.0] {
        let c = Vec2::new(g.rng.range(-4.0, 4.0), side * g.rng.range(14.0, 16.0));
        g.island(IslandSpec {
            rz: 8.0,
            depth: 6.0,
            hills: 0.4,
            rim: 3.5,
            rim_start: 0.25,
            core: false,
            ..IslandSpec::new(c, heart_top + 3.5, 11.0)
        });
    }
    // Undercroft: a hanging rock under the Heart, for swinging beneath it.
    g.island(IslandSpec {
        depth: 6.0,
        surface: Surface::Rocky,
        crystals: mat::CRYSTAL_VIOLET,
        ..IslandSpec::new(Vec2::new(6.0, -4.0), 16.0, 7.0)
    });

    // ---- Satellites ----------------------------------------------------------------------------
    // Long Meadow (E, mid): long open lane.
    let meadow_c = Vec2::new(68.0, 0.0) + j(&mut g, 2.0);
    let meadow_top = 36.0 + g.rng.range(-1.0, 1.0);
    g.island(IslandSpec { rz: 13.0, depth: 15.0, hills: 1.2, ..IslandSpec::new(meadow_c, meadow_top, 22.0) });
    // Sloped land bridge Heart → Meadow.
    let b0 = Vec2::new(22.0, 0.0);
    let b1 = Vec2::new(meadow_c.x - 18.0, meadow_c.y * 0.5);
    let bc = (b0 + b1) * 0.5;
    let bslope = (meadow_top - heart_top) / (b1.x - b0.x);
    g.island(IslandSpec {
        rx: (b1.x - b0.x) * 0.5 + 5.0,
        rz: 4.8,
        depth: 5.0,
        hills: 0.3,
        slope: Vec2::new(bslope, 0.0),
        core: false,
        ..IslandSpec::new(bc, heart_top + bslope * (bc.x - b0.x), 1.0)
    });

    // Spire Garden (NE, high).
    let spire_c = polar(60.0, 66.0) + j(&mut g, 2.5);
    let spire_top = 52.0 + g.rng.range(-1.0, 1.5);
    g.island(IslandSpec {
        depth: 14.0,
        surface: Surface::Rocky,
        crystals: mat::CRYSTAL_VIOLET,
        ..IslandSpec::new(spire_c, spire_top, 15.0)
    });

    // Grove (NW, low).
    let grove_c = polar(120.0, 66.0) + j(&mut g, 2.5);
    let grove_top = 28.0 + g.rng.range(-1.0, 1.0);
    g.island(IslandSpec { depth: 17.0, hills: 1.5, ..IslandSpec::new(grove_c, grove_top, 21.0) });

    // Ruins (W, mid).
    let ruins_c = Vec2::new(-68.0, 0.0) + j(&mut g, 2.0);
    let ruins_top = 42.0 + g.rng.range(-1.0, 1.0);
    g.island(IslandSpec { rz: 17.0, depth: 16.0, hills: 0.6, ..IslandSpec::new(ruins_c, ruins_top, 20.0) });

    // Crag (SW, high) + terrace + overhanging shelf pointing at the Heart.
    let crag_c = polar(240.0, 66.0) + j(&mut g, 2.5);
    let crag_top = 50.0 + g.rng.range(-1.0, 1.0);
    g.island(IslandSpec { depth: 16.0, surface: Surface::Rocky, crystals: mat::CRYSTAL_GOLD, ..IslandSpec::new(crag_c, crag_top, 15.0) });
    let terrace_c = crag_c + (crag_c.normalize() * 5.0) + j(&mut g, 1.5);
    g.island(IslandSpec { depth: 6.0, hills: 0.4, core: false, ..IslandSpec::new(terrace_c, crag_top + 5.0, 8.0) });
    let to_heart = -crag_c.normalize();
    let shelf_c = crag_c + to_heart * 17.0;
    g.island(IslandSpec {
        rx: 10.0,
        rz: 4.0,
        rot: datan2(to_heart.y, to_heart.x),
        depth: 2.5,
        hills: 0.2,
        surface: Surface::Rocky,
        core: false,
        ..IslandSpec::new(shelf_c, crag_top - 1.0, 1.0)
    });

    // Sandbar (SE, low).
    let sand_c = polar(300.0, 66.0) + j(&mut g, 2.5);
    let sand_top = 26.0 + g.rng.range(-1.0, 1.0);
    g.island(IslandSpec { depth: 15.0, hills: 0.9, surface: Surface::Beach, ..IslandSpec::new(sand_c, sand_top, 20.0) });

    // ---- Outer loop stepping stones --------------------------------------------------------------
    let steps: [(f32, f32, f32, f32); 6] = [
        (30.0, 66.0, 44.0, 5.5),
        (90.0, 80.0, 40.0, 7.0),
        (150.0, 66.0, 35.0, 5.5),
        (210.0, 66.0, 47.0, 5.5),
        (270.0, 78.0, 39.0, 7.0),
        (330.0, 66.0, 31.0, 5.5),
    ];
    let mut step_pos = Vec::new();
    for (i, &(ang, r, top, rad)) in steps.iter().enumerate() {
        let c = polar(ang, r) + j(&mut g, 1.5);
        let top = top + g.rng.range(-1.0, 1.0);
        let crystals = [mat::CRYSTAL_CYAN, mat::CRYSTAL_VIOLET, mat::CRYSTAL_GOLD][i % 3];
        g.island(IslandSpec { depth: rad * 1.3, hills: 0.3, crystals, ..IslandSpec::new(c, top, rad) });
        step_pos.push((c, top));
    }

    // ---- Sky anchors: floating rocks ------------------------------------------------------------
    for k in 0..6 {
        let ang = 30.0 + 60.0 * k as f32 + g.rng.range(-8.0, 8.0);
        let c = polar(ang, 38.0 + g.rng.range(-3.0, 3.0));
        let top = g.rng.range(60.0, 68.0);
        let r = g.rng.range(3.0, 4.8);
        g.island(IslandSpec {
            depth: r * 1.8,
            hills: 0.2,
            surface: Surface::Rocky,
            crystals: if k % 2 == 0 { mat::CRYSTAL_CYAN } else { mat::CRYSTAL_VIOLET },
            ..IslandSpec::new(c, top, r)
        });
    }
    for k in 0..5 {
        let ang = 72.0 * k as f32 + 12.0 + g.rng.range(-10.0, 10.0);
        let c = polar(ang, 84.0 + g.rng.range(-3.0, 2.0));
        let top = g.rng.range(66.0, 78.0);
        let r = g.rng.range(3.5, 5.5);
        g.island(IslandSpec {
            depth: r * 2.0,
            hills: 0.2,
            surface: Surface::Rocky,
            crystals: mat::CRYSTAL_GOLD,
            ..IslandSpec::new(c, top, r)
        });
    }

    // ---- Structures ----------------------------------------------------------------------------
    // Heart: lantern ring over the spine, colonnade, rubble.
    let ring_c = Vec3::new(0.0, heart_top + 17.5, 0.0);
    g.lantern_ring(ring_c, 13.5, 1.6, heart_top);
    for k in 0..8 {
        let a = 22.5 + 45.0 * k as f32 + g.rng.range(-6.0, 6.0);
        let p = polar(a, 17.0 + g.rng.range(-1.0, 1.5));
        let h = if k % 3 == 0 { g.rng.range(3.0, 5.0) } else { g.rng.range(7.0, 15.0) };
        g.pillar(p.x, p.y, heart_top + 10.0, h, 0.9);
    }
    // Low broken walls either side of the ring: cover for fights around the landmark.
    for side in [-1.0f32, 1.0] {
        let a = Vec2::new(side * g.rng.range(7.0, 9.0), g.rng.range(-9.0, -5.0));
        let b = a + Vec2::new(side * g.rng.range(0.0, 2.0), g.rng.range(8.0, 11.0));
        let h = g.rng.range(1.8, 2.6);
        g.wall(a, b, heart_top + 10.0, h, 1.0);
    }
    for k in 0..3 {
        let p = polar(g.rng.range(0.0, 360.0), g.rng.range(9.0, 20.0));
        let r1 = g.rng.range(4.0, 7.0);
        g.fallen_pillar(p.x, p.y, heart_top + 10.0, r1, k % 2 == 0);
    }
    for _ in 0..4 {
        let p = polar(g.rng.range(0.0, 360.0), g.rng.range(8.0, 21.0));
        if p.y.abs() > 4.0 {
            let r2 = g.rng.range(1.2, 2.2);
            g.boulder(p.x, p.y, heart_top + 10.0, r2);
        }
    }

    // Spine: walkable arch Heart → Ruins.
    let arch_a = Vec3::new(-21.0, heart_top - 1.5, 0.0);
    let arch_b = Vec3::new(ruins_c.x + 15.0, ruins_top - 1.5, ruins_c.y * 0.6);
    g.arch(arch_a, arch_b, 7.0, 2.3);

    // Meadow: a few trees on the flanks + cover rocks, the middle lane kept open.
    for k in 0..3 {
        let x = meadow_c.x - 12.0 + 12.0 * k as f32 + g.rng.range(-2.0, 2.0);
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let leaves = *g.rng.pick(&[mat::LEAVES_GREEN, mat::LEAVES_GOLD]);
        let r3 = g.rng.range(13.0, 17.0);
        g.tree(x, meadow_c.y + side * 9.0, meadow_top + 8.0, r3, 0.9, leaves, 4.0);
    }
    for _ in 0..4 {
        let p = meadow_c + Vec2::new(g.rng.range(-16.0, 16.0), g.rng.range(-8.0, 8.0));
        if (p.y - meadow_c.y).abs() > 3.0 {
            let r4 = g.rng.range(1.0, 1.8);
            g.boulder(p.x, p.y, meadow_top + 8.0, r4);
        }
    }

    // Spire Garden: tall crystal-tipped needles.
    for k in 0..4 {
        let a = 45.0 + 90.0 * k as f32 + g.rng.range(-20.0, 20.0);
        let p = spire_c + polar(a, g.rng.range(5.0, 9.5));
        let tip = [mat::CRYSTAL_CYAN, mat::CRYSTAL_VIOLET][k % 2];
        let r5 = g.rng.range(20.0, 32.0);
        let r6 = g.rng.range(1.6, 2.4);
        g.spire(p.x, p.y, spire_top + 8.0, r5, r6, Some(tip));
    }
    g.boulder(spire_c.x, spire_c.y, spire_top + 8.0, 1.6);

    // Grove: giant autumn trees.
    let grove_leaves = [mat::LEAVES_ORANGE, mat::LEAVES_RED, mat::LEAVES_GOLD];
    for k in 0..3 {
        let a = 90.0 + 120.0 * k as f32 + g.rng.range(-15.0, 15.0);
        let p = grove_c + polar(a, g.rng.range(8.0, 11.0));
        let r7 = g.rng.range(27.0, 33.0);
        g.tree(p.x, p.y, grove_top + 8.0, r7, 1.5, grove_leaves[k], 6.5);
    }
    for _ in 0..3 {
        let p = grove_c + polar(g.rng.range(0.0, 360.0), g.rng.range(2.0, 15.0));
        let r8 = g.rng.range(1.2, 2.0);
        g.boulder(p.x, p.y, grove_top + 8.0, r8);
    }

    // Ruins: a tall round tower, a squat square tower, broken walls.
    let tower_a = ruins_c + Vec2::new(-5.0, 7.0) + j(&mut g, 1.5);
    g.tower(tower_a.x, tower_a.y, ruins_top + 8.0, 4.5, 26.0, false, Vec2::ZERO);
    let tower_b = ruins_c + Vec2::new(2.0, -9.0) + j(&mut g, 1.5);
    g.tower(tower_b.x, tower_b.y, ruins_top + 8.0, 3.5, 15.0, true, Vec2::ZERO);
    for _ in 0..4 {
        let a = ruins_c + Vec2::new(g.rng.range(-14.0, 10.0), g.rng.range(-12.0, 12.0));
        let dir = polar(g.rng.range(0.0, 180.0), g.rng.range(5.0, 9.0));
        let r9 = g.rng.range(2.2, 3.6);
        g.wall(a, a + dir, ruins_top + 8.0, r9, 1.0);
    }

    // Crag: watchtower on the terrace.
    g.tower(terrace_c.x, terrace_c.y, crag_top + 14.0, 3.0, 13.0, true, Vec2::ZERO);
    g.spire(crag_c.x - to_heart.x * 9.0, crag_c.y - to_heart.y * 9.0, crag_top + 12.0, 14.0, 1.8, None);

    // Sandbar: blossom trees + beach rocks.
    for k in 0..4 {
        let a = 45.0 + 90.0 * k as f32 + g.rng.range(-20.0, 20.0);
        let p = sand_c + polar(a, g.rng.range(6.0, 11.0));
        let r10 = g.rng.range(15.0, 20.0);
        g.tree(p.x, p.y, sand_top + 8.0, r10, 1.0, mat::LEAVES_PINK, 4.5);
    }
    for _ in 0..4 {
        let p = sand_c + polar(g.rng.range(0.0, 360.0), g.rng.range(4.0, 16.0));
        let r11 = g.rng.range(1.0, 2.0);
        g.boulder(p.x, p.y, sand_top + 8.0, r11);
    }

    // Anchors on two stepping stones: a needle (N) and a big tree (S).
    let (n_c, n_top) = step_pos[1];
    g.spire(n_c.x, n_c.y, n_top + 6.0, 26.0, 1.8, Some(mat::CRYSTAL_CYAN));
    let (s_c, s_top) = step_pos[4];
    g.tree(s_c.x, s_c.y, s_top + 6.0, 24.0, 1.2, mat::LEAVES_GREEN, 5.0);

    // Big arches over the outer gaps: NE ↔ E, SW ↔ SE.
    let ne_foot = spire_c + (meadow_c - spire_c).normalize() * 11.0;
    let e_foot = meadow_c + Vec2::new(-4.0, 11.0);
    g.arch(Vec3::new(ne_foot.x, spire_top - 2.0, ne_foot.y), Vec3::new(e_foot.x, meadow_top - 2.0, e_foot.y), 14.0, 1.8);
    let sw_foot = crag_c + (sand_c - crag_c).normalize() * 11.0;
    let se_foot = sand_c + (crag_c - sand_c).normalize() * 12.0;
    g.arch(Vec3::new(sw_foot.x, crag_top - 2.0, sw_foot.y), Vec3::new(se_foot.x, sand_top - 2.0, se_foot.y), 16.0, 1.8);

    g.place_cores();

    // ---- Spawns: two per big satellite, one per high satellite, facing inwards -----------------
    let spawn_targets: [(Vec2, f32); 10] = [
        (meadow_c + Vec2::new(13.0, 5.0), meadow_top),
        (meadow_c + Vec2::new(2.0, -7.0), meadow_top),
        (spire_c + spire_c.normalize() * 3.0, spire_top),
        (grove_c + grove_c.normalize() * 7.0 + Vec2::new(4.0, 0.0), grove_top),
        (grove_c + grove_c.normalize() * 3.0 + Vec2::new(-6.0, -6.0), grove_top),
        (ruins_c + Vec2::new(-9.0, -2.0), ruins_top),
        (ruins_c + Vec2::new(4.0, 2.0), ruins_top),
        (crag_c + crag_c.normalize() * 1.0 + Vec2::new(-4.0, 3.0), crag_top),
        (sand_c + sand_c.normalize() * 4.0, sand_top),
        (sand_c + Vec2::new(-8.0, 5.0), sand_top),
    ];
    let mut spawns = Vec::new();
    for (p, top) in spawn_targets {
        if let Some(s) = g.spawn_near(p.x, p.y, top + 6.0, &spawns) {
            spawns.push(s);
        }
    }

    // ---- Surf lines (hard light; no RNG drawn, so the voxels are unchanged) --------------------------
    // Spire Slide: off the Spire Garden's edge (high, NE) down onto the Long Meadow lane (E) — surf
    // into a bhop. Crag Chute: off the Crag (high, SW) over the gap down to the Sandbar (low, SE).
    // Offset `side` metres to the outside of the arch that already spans each gap (the arches are
    // voxel staircases: walkable, not surfable; the line must not run through them).
    let surf_line = |from: Vec2, from_r: f32, from_top: f32, to: Vec2, to_r: f32, to_top: f32, side: f32| {
        let dir = (to - from).normalize();
        let mut out = Vec2::new(-dir.y, dir.x);
        if out.dot(from + to) < 0.0 {
            out = -out;
        }
        let a = from + dir * (from_r * 0.7) + out * side;
        let b = to - dir * (to_r + 4.0) + out * side;
        SurfLine { a: Vec3::new(a.x, from_top + 7.0, a.y), b: Vec3::new(b.x, to_top + 11.0, b.y), face_deg: 60.0, width: 8.0 }
    };
    let surf_lines = vec![
        surf_line(spire_c, 15.0, spire_top, meadow_c, 13.0, meadow_top, 12.0),
        surf_line(crag_c, 15.0, crag_top, sand_c, 20.0, sand_top, 9.0),
    ];

    let mut w = g.w;
    w.prune_unanchored();
    w.mark_all_dirty();
    let wb = w.bounds();
    let void_y = -4.0;
    let info = ArenaInfo {
        spawns,
        bounds: Aabb {
            min: Vec3::new(wb.min.x - 24.0, void_y, wb.min.z - 24.0),
            max: Vec3::new(wb.max.x + 24.0, wb.max.y + 40.0, wb.max.z + 24.0),
        },
        void_y,
        center: Vec3::new(0.0, heart_top, 0.0),
        sun_dir: Vec3::new(0.38, 0.82, 0.28).normalize(),
        sky_zenith: [0.33, 0.58, 0.95],
        sky_horizon: [0.80, 0.90, 1.0],
        fog_color: [0.74, 0.85, 0.98],
        ambient: 0.38,
        gravity: None,
        surf_lines,
        water_sheets: Vec::new(),
        water_walk: false,
    };
    (w, info)
}

/// A flat movement lab `size_m` metres square: 8 m of ground on an indestructible bedrock layer, with
/// pillars of increasing height, walls, 1-voxel stairs, a 45° stair ramp, a high beam between two
/// towers (swing tests), a pit, a tree, and 8 spawns around the middle.
pub fn generate_flat_test(size_m: f32) -> (VoxelWorld, ArenaInfo) {
    let sv = ((size_m / VS).ceil() as i32).max(64);
    let mut g = Gen::new(VoxelWorld::centered(IVec3::new(sv, 128, sv)), 1);
    let (lo, hi) = (g.w.voxel_min(), g.w.voxel_max());
    let ground = 8.0;
    let gy = vx(ground);
    g.w.fill_box(IVec3::new(lo.x, 0, lo.z), IVec3::new(hi.x - 1, 0, hi.z - 1), mat::CORE);
    g.w.fill_box(IVec3::new(lo.x, 1, lo.z), IVec3::new(hi.x - 1, gy - 4, hi.z - 1), mat::STONE);
    g.w.fill_box(IVec3::new(lo.x, gy - 3, lo.z), IVec3::new(hi.x - 1, gy - 2, hi.z - 1), mat::DIRT);
    g.w.fill_box(IVec3::new(lo.x, gy - 1, lo.z), IVec3::new(hi.x - 1, gy - 1, hi.z - 1), mat::GRASS);
    for x in [lo.x, 0, hi.x - 1] {
        for z in [lo.z, 0, hi.z - 1] {
            g.w.add_anchor_hint(IVec3::new(x, 0, z));
        }
    }
    let half = sv as f32 * VS * 0.5;
    let q = half * 0.5;
    let from = ground + 40.0;
    let bx = |g: &mut Gen, min: Vec3, max: Vec3, m: Material| {
        g.w.fill_box((min / VS).floor().as_ivec3(), (max / VS).ceil().as_ivec3() - IVec3::ONE, m);
    };

    // Row of pillars 2 → 24 m (north side).
    for (i, h) in [2.0, 4.0, 8.0, 12.0, 16.0, 24.0].iter().enumerate() {
        let x = -q + i as f32 * 6.0;
        bx(&mut g, Vec3::new(x, ground, q), Vec3::new(x + 1.5, ground + h, q + 1.5), mat::STONE);
    }
    // Walls (east): parallel pair for wall-jump/strafe tests.
    bx(&mut g, Vec3::new(q, ground, -6.0), Vec3::new(q + 1.0, ground + 4.0, 6.0), mat::RUIN_BRICK);
    bx(&mut g, Vec3::new(q + 5.0, ground, -6.0), Vec3::new(q + 6.0, ground + 6.0, 6.0), mat::RUIN_BRICK);
    // 1-voxel stairs (south-west): 0.5 m rise, 1 m run, to a 6 m platform.
    for i in 0..12 {
        let z = -q - i as f32;
        bx(&mut g, Vec3::new(-q, ground, z - 1.0), Vec3::new(-q + 4.0, ground + 0.5 * (i + 1) as f32, z), mat::STONE);
    }
    bx(&mut g, Vec3::new(-q, ground, -q - 18.0), Vec3::new(-q + 8.0, ground + 6.0, -q - 12.0), mat::STONE);
    // 45° stair ramp (0.5 rise per 0.5 run).
    for i in 0..16 {
        let x = -6.0 + i as f32 * 0.5;
        bx(&mut g, Vec3::new(x, ground, -q), Vec3::new(x + 0.5, ground + 0.5 * (i + 1) as f32, -q + 4.0), mat::DIRT);
    }
    // Two towers + a high beam for tether swings (south-east).
    bx(&mut g, Vec3::new(q - 12.0, ground, -q), Vec3::new(q - 9.0, ground + 30.0, -q + 3.0), mat::STONE);
    bx(&mut g, Vec3::new(q + 9.0, ground, -q), Vec3::new(q + 12.0, ground + 30.0, -q + 3.0), mat::STONE);
    bx(&mut g, Vec3::new(q - 9.0, ground + 26.0, -q), Vec3::new(q + 9.0, ground + 28.0, -q + 3.0), mat::STONE);
    // Pit (west): 8 m wide, 5 m deep.
    bx(&mut g, Vec3::new(-q - 4.0, ground - 5.0, -4.0), Vec3::new(-q + 4.0, ground, 4.0), AIR);
    // A tree to chop (collapse test), north-west.
    g.tree(-q - 8.0, q + 8.0, from, 14.0, 0.8, mat::LEAVES_GREEN, 3.5);

    let mut spawns = Vec::new();
    for k in 0..8 {
        let p = polar(45.0 * k as f32 + 22.5, 10.0);
        if let Some(s) = g.spawn_near(p.x, p.y, from, &spawns) {
            spawns.push(s);
        }
    }
    let mut w = g.w;
    w.prune_unanchored();
    w.mark_all_dirty();
    let wb = w.bounds();
    let info = ArenaInfo {
        spawns,
        bounds: Aabb { min: Vec3::new(wb.min.x, -4.0, wb.min.z), max: Vec3::new(wb.max.x, wb.max.y + 40.0, wb.max.z) },
        void_y: -4.0,
        center: Vec3::new(0.0, ground, 0.0),
        sun_dir: Vec3::new(0.38, 0.82, 0.28).normalize(),
        sky_zenith: [0.33, 0.58, 0.95],
        sky_horizon: [0.80, 0.90, 1.0],
        fog_color: [0.74, 0.85, 0.98],
        ambient: 0.38,
        gravity: None,
        surf_lines: Vec::new(),
        water_sheets: Vec::new(),
        water_walk: false,
    };
    (w, info)
}
