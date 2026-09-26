//! Collision world for movement: the voxel grid + caller-supplied oriented boxes (construct walls,
//! platforms, sloped ramps, optionally other players). Swept hull tests (never tunnel), depenetration
//! and rays for tethers.
//!
//! Everything here is allocation-free: a per-tick [`Broadphase`] caches the solid voxels and colliders
//! around the player in fixed-size arrays.

use bd_core::voxel::{Aabb, Material, VoxelQuery, voxel_min};
use bd_core::{ConstructId, IVec3, PlayerId, Quat, VOXEL_SIZE, Vec3};
use glam::Mat3;
use serde::{Deserialize, Serialize};

/// Gap kept between the hull and anything it touches (m). Keeps grid-aligned sliding free of
/// internal-edge snags and makes contact numerically robust.
pub(crate) const SKIN: f32 = 0.002;
/// Overlap deeper than this is treated as penetration (must be < SKIN).
const OVERLAP_EPS: f32 = 2e-4;

/// Identity of a dynamic collider (stable across ticks so hooks can follow it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColliderId {
    /// A construct part (a construct may have several boxes).
    Construct { id: ConstructId, part: u16 },
    Player(PlayerId),
    Other(u32),
}

/// What a tether hook is anchored to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnchorTarget {
    Voxel(IVec3),
    Collider(ColliderId),
}

/// An oriented box supplied by the caller each tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
    pub center: Vec3,
    pub half: Vec3,
    pub rot: Quat,
    /// Linear velocity (m/s): moving platforms carry you, moving walls shove you, momentum inherits.
    pub vel: Vec3,
    pub id: ColliderId,
    /// Blocks movement.
    pub solid: bool,
    /// Hooks can anchor to it.
    pub anchorable: bool,
}

impl Collider {
    pub fn new(center: Vec3, half: Vec3, rot: Quat, id: ColliderId) -> Self {
        Self { center, half, rot, vel: Vec3::ZERO, id, solid: true, anchorable: true }
    }

    pub fn with_vel(mut self, vel: Vec3) -> Self {
        self.vel = vel;
        self
    }

    /// A flat-topped ramp slab. `foot` = centre of the low edge of the top surface; it ascends toward
    /// `yaw` (same convention as the view: yaw 0 = −Z) at `angle` radians over `length` metres of slope.
    pub fn ramp(foot: Vec3, yaw: f32, angle: f32, length: f32, width: f32, thickness: f32, id: ColliderId) -> Self {
        let (sy, cy) = yaw.sin_cos();
        let h = Vec3::new(-sy, 0.0, -cy);
        let right = Vec3::new(cy, 0.0, -sy);
        let (sa, ca) = angle.sin_cos();
        let d = h * ca + Vec3::Y * sa;
        let n = -h * sa + Vec3::Y * ca;
        let rot = Quat::from_mat3(&Mat3::from_cols(right, n, -d));
        let center = foot + d * (length * 0.5) - n * (thickness * 0.5);
        Self::new(center, Vec3::new(width * 0.5, thickness * 0.5, length * 0.5), rot, id)
    }

    #[inline]
    pub fn axes(&self) -> [Vec3; 3] {
        let m = Mat3::from_quat(self.rot);
        [m.x_axis, m.y_axis, m.z_axis]
    }

    /// World-space bounding box.
    pub fn bounds(&self) -> Aabb {
        let a = self.axes();
        let e = a[0].abs() * self.half.x + a[1].abs() * self.half.y + a[2].abs() * self.half.z;
        Aabb::from_center_half(self.center, e)
    }

    #[inline]
    pub fn to_local(&self, p: Vec3) -> Vec3 {
        self.rot.inverse() * (p - self.center)
    }

    #[inline]
    pub fn to_world(&self, l: Vec3) -> Vec3 {
        self.center + self.rot * l
    }
}

/// Everything the controller collides with this tick.
pub struct CollisionWorld<'a, V: VoxelQuery + ?Sized> {
    pub voxels: &'a V,
    pub colliders: &'a [Collider],
    /// Colliders with this id are ignored (pass the stepping player's own hull id).
    pub ignore: Option<ColliderId>,
}

impl<'a, V: VoxelQuery + ?Sized> CollisionWorld<'a, V> {
    pub fn new(voxels: &'a V, colliders: &'a [Collider]) -> Self {
        Self { voxels, colliders, ignore: None }
    }

    pub fn ignoring(mut self, id: ColliderId) -> Self {
        self.ignore = Some(id);
        self
    }

    #[inline]
    fn skip(&self, c: &Collider) -> bool {
        self.ignore == Some(c.id)
    }
}

// ------------------------------------------------------------------------------------------ sweeps

#[derive(Clone, Copy, Debug)]
pub(crate) struct Hit {
    /// Fraction of the displacement at first contact.
    pub t: f32,
    /// Contact normal pointing out of the obstacle toward the hull.
    pub normal: Vec3,
    /// Surface velocity of the obstacle.
    pub vel: Vec3,
}

/// Swept AABB vs AABB. Returns (t, normal) of first contact within [0, 1].
#[inline]
fn sweep_aabb(a: &Aabb, d: Vec3, b: &Aabb) -> Option<(f32, Vec3)> {
    let mut t_in = f32::NEG_INFINITY;
    let mut t_out = f32::INFINITY;
    let mut n = Vec3::ZERO;
    let mut min_pen = f32::INFINITY;
    let mut pen_n = Vec3::ZERO;
    for i in 0..3 {
        let (amin, amax, bmin, bmax, di) = (a.min[i], a.max[i], b.min[i], b.max[i], d[i]);
        // Penetration bookkeeping for the initially-overlapping case.
        let push_neg = amax - bmin; // move A toward -axis
        let push_pos = bmax - amin; // move A toward +axis
        if push_neg < min_pen {
            min_pen = push_neg;
            pen_n = -axis(i);
        }
        if push_pos < min_pen {
            min_pen = push_pos;
            pen_n = axis(i);
        }
        if di == 0.0 {
            if amax <= bmin || amin >= bmax {
                return None;
            }
            continue;
        }
        let (t0, t1, ni) = if di > 0.0 {
            ((bmin - amax) / di, (bmax - amin) / di, -axis(i))
        } else {
            ((bmax - amin) / di, (bmin - amax) / di, axis(i))
        };
        if t0 > t_in {
            t_in = t0;
            n = ni;
        }
        if t1 < t_out {
            t_out = t1;
        }
    }
    if t_in >= t_out || t_in > 1.0 || t_out <= 0.0 {
        return None;
    }
    if t_in < 0.0 {
        // Already overlapping (numerical drift or something moved into us). Block only motion that
        // goes deeper; depenetration resolves the rest.
        if min_pen > OVERLAP_EPS && d.dot(pen_n) < 0.0 {
            return Some((0.0, pen_n));
        }
        return None;
    }
    Some((t_in, n))
}

#[inline]
fn axis(i: usize) -> Vec3 {
    match i {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    }
}

/// SAT axis set for an AABB vs an oriented box (3 + 3 + 9 edge crosses).
#[inline]
fn sat_axes(u: &[Vec3; 3]) -> [Vec3; 15] {
    let w = [Vec3::X, Vec3::Y, Vec3::Z];
    [
        w[0],
        w[1],
        w[2],
        u[0],
        u[1],
        u[2],
        w[0].cross(u[0]),
        w[0].cross(u[1]),
        w[0].cross(u[2]),
        w[1].cross(u[0]),
        w[1].cross(u[1]),
        w[1].cross(u[2]),
        w[2].cross(u[0]),
        w[2].cross(u[1]),
        w[2].cross(u[2]),
    ]
}

/// Swept AABB vs oriented box (swept separating-axis test).
fn sweep_obb(a: &Aabb, d: Vec3, c: &Collider) -> Option<(f32, Vec3)> {
    let ca = a.center();
    let ha = a.half();
    let u = c.axes();
    let hb = c.half;
    let mut t_in = f32::NEG_INFINITY;
    let mut t_out = f32::INFINITY;
    let mut n = Vec3::ZERO;
    let mut min_pen = f32::INFINITY;
    let mut pen_n = Vec3::ZERO;
    for l in sat_axes(&u) {
        let len2 = l.length_squared();
        if len2 < 1e-6 {
            continue;
        }
        let l = l / len2.sqrt();
        let ra = ha.x * l.x.abs() + ha.y * l.y.abs() + ha.z * l.z.abs();
        let rb = hb.x * u[0].dot(l).abs() + hb.y * u[1].dot(l).abs() + hb.z * u[2].dot(l).abs();
        let s = (c.center - ca).dot(l);
        let r = ra + rb;
        let v = d.dot(l);
        let pen = r - s.abs();
        if pen < min_pen {
            min_pen = pen;
            pen_n = if s > 0.0 { -l } else { l };
        }
        if v.abs() < 1e-9 {
            if s.abs() >= r {
                return None;
            }
            continue;
        }
        let (t0, t1, ni) = if v > 0.0 { ((s - r) / v, (s + r) / v, -l) } else { ((s + r) / v, (s - r) / v, l) };
        if t0 > t_in {
            t_in = t0;
            n = ni;
        }
        if t1 < t_out {
            t_out = t1;
        }
    }
    if t_in >= t_out || t_in > 1.0 || t_out <= 0.0 {
        return None;
    }
    if t_in < 0.0 {
        if min_pen > OVERLAP_EPS && d.dot(pen_n) < 0.0 {
            return Some((0.0, pen_n));
        }
        return None;
    }
    Some((t_in, n))
}

/// Penetration of an AABB into an oriented box: (depth, push normal) along the min-overlap axis.
fn obb_penetration(a: &Aabb, c: &Collider) -> Option<(f32, Vec3)> {
    let ca = a.center();
    let ha = a.half();
    let u = c.axes();
    let hb = c.half;
    let mut best = (f32::INFINITY, Vec3::ZERO);
    for l in sat_axes(&u) {
        let len2 = l.length_squared();
        if len2 < 1e-6 {
            continue;
        }
        let l = l / len2.sqrt();
        let ra = ha.x * l.x.abs() + ha.y * l.y.abs() + ha.z * l.z.abs();
        let rb = hb.x * u[0].dot(l).abs() + hb.y * u[1].dot(l).abs() + hb.z * u[2].dot(l).abs();
        let s = (c.center - ca).dot(l);
        let pen = ra + rb - s.abs();
        if pen <= OVERLAP_EPS {
            return None;
        }
        // Mild preference for pushing up (standing on things beats being shoved sideways).
        let score = if l.y.abs() > 0.7 && s < 0.0 { pen * 0.7 } else { pen };
        if score < best.0 {
            best = (score, if s > 0.0 { -l } else { l });
        }
    }
    // Recompute the true depth along the chosen axis.
    let l = best.1;
    let ra = ha.x * l.x.abs() + ha.y * l.y.abs() + ha.z * l.z.abs();
    let rb = hb.x * u[0].dot(l).abs() + hb.y * u[1].dot(l).abs() + hb.z * u[2].dot(l).abs();
    let s = (c.center - ca).dot(l);
    Some((ra + rb - s.abs(), l))
}

/// Penetration of an AABB into a voxel box (with an up-bias so being "inside a step" pops you on top).
fn aabb_penetration(a: &Aabb, b: &Aabb, up_limit: f32) -> Option<(f32, Vec3)> {
    let mut best = (f32::INFINITY, f32::INFINITY, Vec3::ZERO);
    for i in 0..3 {
        let neg = a.max[i] - b.min[i];
        let pos = b.max[i] - a.min[i];
        if neg <= OVERLAP_EPS || pos <= OVERLAP_EPS {
            return None;
        }
        let score_pos = if i == 1 && pos <= up_limit { pos * 0.5 } else { pos };
        if neg < best.0 {
            best = (neg, neg, -axis(i));
        }
        if score_pos < best.0 {
            best = (score_pos, pos, axis(i));
        }
    }
    Some((best.1, best.2))
}

// -------------------------------------------------------------------------------------- broadphase

const MAX_CELLS: i64 = 2048;
const MAX_BOXES: usize = 256;
const MAX_COLS: usize = 48;
/// Extra room gathered around each query so the step-up, ground probe and snap of the same tick reuse
/// the cache.
const PAD: Vec3 = Vec3::new(0.3, 0.7, 0.3);

/// Per-tick cache of nearby solid voxels (merged into boxes: runs along x, then identical runs along z)
/// and solid collider indices.
pub(crate) struct Broadphase {
    region: Aabb,
    valid: bool,
    boxes: [Aabb; MAX_BOXES],
    nb: usize,
    cols: [u16; MAX_COLS],
    nc: usize,
}

#[inline]
fn contains(outer: &Aabb, inner: &Aabb) -> bool {
    outer.min.cmple(inner.min).all() && outer.max.cmpge(inner.max).all()
}

#[inline]
fn union(a: &Aabb, b: &Aabb) -> Aabb {
    Aabb { min: a.min.min(b.min), max: a.max.max(b.max) }
}

impl Broadphase {
    pub fn new() -> Self {
        Self {
            region: Aabb { min: Vec3::ZERO, max: Vec3::ZERO },
            valid: false,
            boxes: [Aabb { min: Vec3::ZERO, max: Vec3::ZERO }; MAX_BOXES],
            nb: 0,
            cols: [0; MAX_COLS],
            nc: 0,
        }
    }

    /// Gather once for the whole tick: the hull, where it is heading, plus room for step-up/snap.
    pub fn prime<V: VoxelQuery + ?Sized>(&mut self, w: &CollisionWorld<V>, hull: Aabb, disp: Vec3) {
        let swept = union(&hull, &hull.translate(disp));
        let pad = Vec3::new(0.45, 0.75, 0.45);
        let _ = self.gather(w, Aabb { min: swept.min - pad, max: swept.max + pad });
    }

    fn gather<V: VoxelQuery + ?Sized>(&mut self, w: &CollisionWorld<V>, region: Aabb) -> bool {
        self.valid = false;
        let (lo, hi) = region.voxel_range();
        let n = hi - lo + IVec3::ONE;
        if n.x <= 0 || n.y <= 0 || n.z <= 0 {
            return false;
        }
        if n.x as i64 * n.y as i64 * n.z as i64 > MAX_CELLS {
            return false;
        }
        self.nb = 0;
        let vs = Vec3::splat(VOXEL_SIZE);
        for y in lo.y..=hi.y {
            // Boxes of this layer are candidates for merging along z (exact coordinates: k * 0.5).
            let layer_start = self.nb;
            for z in lo.z..=hi.z {
                let mut x = lo.x;
                while x <= hi.x {
                    if !w.voxels.is_solid(IVec3::new(x, y, z)) {
                        x += 1;
                        continue;
                    }
                    let x0 = x;
                    while x + 1 <= hi.x && w.voxels.is_solid(IVec3::new(x + 1, y, z)) {
                        x += 1;
                    }
                    let run = Aabb { min: voxel_min(IVec3::new(x0, y, z)), max: voxel_min(IVec3::new(x, y, z)) + vs };
                    x += 1;
                    // Extend an identical run from the previous row along z instead of adding a box.
                    let mut merged = false;
                    for b in &mut self.boxes[layer_start..self.nb] {
                        if b.min.x == run.min.x && b.max.x == run.max.x && b.max.z == run.min.z {
                            b.max.z = run.max.z;
                            merged = true;
                            break;
                        }
                    }
                    if merged {
                        continue;
                    }
                    if self.nb == MAX_BOXES {
                        return false;
                    }
                    self.boxes[self.nb] = run;
                    self.nb += 1;
                }
            }
        }
        self.nc = 0;
        for (i, c) in w.colliders.iter().enumerate() {
            if !c.solid || w.skip(c) || self.nc == MAX_COLS {
                continue;
            }
            if c.bounds().intersects(&region) {
                self.cols[self.nc] = i as u16;
                self.nc += 1;
            }
        }
        self.region = region;
        self.valid = true;
        true
    }

    /// Make sure the cache covers `need`. Returns false if the region is too big to cache.
    fn ensure<V: VoxelQuery + ?Sized>(&mut self, w: &CollisionWorld<V>, need: &Aabb) -> bool {
        if self.valid && contains(&self.region, need) {
            return true;
        }
        let padded = Aabb { min: need.min - PAD, max: need.max + PAD };
        self.gather(w, padded) || self.gather(w, *need)
    }

    /// Earliest contact of `hull` moving by `d`.
    pub fn sweep<V: VoxelQuery + ?Sized>(&mut self, w: &CollisionWorld<V>, hull: Aabb, d: Vec3) -> Option<Hit> {
        self.sweep_depth(w, hull, d, 0)
    }

    fn sweep_depth<V: VoxelQuery + ?Sized>(&mut self, w: &CollisionWorld<V>, hull: Aabb, d: Vec3, depth: u32) -> Option<Hit> {
        let swept = union(&hull, &hull.translate(d));
        let need = Aabb { min: swept.min - Vec3::splat(SKIN * 2.0), max: swept.max + Vec3::splat(SKIN * 2.0) };
        if self.ensure(w, &need) {
            return self.sweep_cached(w, &hull, d);
        }
        if depth >= 12 {
            // Pathologically dense region: refuse to move rather than risk tunnelling.
            let n = -d.normalize_or_zero();
            return Some(Hit { t: 0.0, normal: n, vel: Vec3::ZERO });
        }
        // Too big to cache in one go: split the sweep.
        let half = d * 0.5;
        if let Some(h) = self.sweep_depth(w, hull, half, depth + 1) {
            return Some(Hit { t: h.t * 0.5, ..h });
        }
        self.sweep_depth(w, hull.translate(half), half, depth + 1).map(|h| Hit { t: 0.5 + h.t * 0.5, ..h })
    }

    fn sweep_cached<V: VoxelQuery + ?Sized>(&self, w: &CollisionWorld<V>, hull: &Aabb, d: Vec3) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for b in &self.boxes[..self.nb] {
            if let Some((t, n)) = sweep_aabb(hull, d, b) {
                if best.is_none_or(|h| t < h.t) {
                    best = Some(Hit { t, normal: n, vel: Vec3::ZERO });
                }
            }
        }
        for &i in &self.cols[..self.nc] {
            let c = &w.colliders[i as usize];
            if let Some((t, n)) = sweep_obb(hull, d, c) {
                if best.is_none_or(|h| t < h.t) {
                    best = Some(Hit { t, normal: n, vel: c.vel });
                }
            }
        }
        best
    }

    /// Does `hull` overlap anything solid?
    pub fn overlaps<V: VoxelQuery + ?Sized>(&mut self, w: &CollisionWorld<V>, hull: Aabb) -> bool {
        let need = Aabb { min: hull.min - Vec3::splat(0.05), max: hull.max + Vec3::splat(0.05) };
        if !self.ensure(w, &need) {
            return true;
        }
        for b in &self.boxes[..self.nb] {
            if aabb_penetration(&hull, b, 0.0).is_some() {
                return true;
            }
        }
        for &i in &self.cols[..self.nc] {
            if obb_penetration(&hull, &w.colliders[i as usize]).is_some() {
                return true;
            }
        }
        false
    }

    /// Push the hull out of anything it overlaps. Returns the last push (normal, pusher velocity).
    pub fn depenetrate<V: VoxelQuery + ?Sized>(
        &mut self,
        w: &CollisionWorld<V>,
        pos: &mut Vec3,
        half_w: f32,
        height: f32,
        up_limit: f32,
    ) -> Option<(Vec3, Vec3)> {
        let mut last = None;
        for _ in 0..8 {
            let hull = super::state::hull_at(*pos, half_w, height);
            let need = Aabb { min: hull.min - Vec3::splat(0.05), max: hull.max + Vec3::splat(0.05) };
            if !self.ensure(w, &need) {
                return last;
            }
            let mut push: Option<(f32, Vec3, Vec3)> = None;
            for b in &self.boxes[..self.nb] {
                if let Some((d, n)) = aabb_penetration(&hull, b, up_limit) {
                    push = Some((d, n, Vec3::ZERO));
                    break;
                }
            }
            if push.is_none() {
                for &i in &self.cols[..self.nc] {
                    let c = &w.colliders[i as usize];
                    if let Some((d, n)) = obb_penetration(&hull, c) {
                        push = Some((d, n, c.vel));
                        break;
                    }
                }
            }
            match push {
                Some((d, n, v)) => {
                    *pos += n * (d + SKIN);
                    last = Some((n, v));
                }
                None => break,
            }
        }
        last
    }
}

/// Fraction of `d` that can be travelled toward a hit at `t` while staying `SKIN` away from it.
#[inline]
pub(crate) fn safe_t(t: f32, d: Vec3, n: Vec3) -> f32 {
    let dn = -d.dot(n);
    if dn <= 1e-9 {
        return t.max(0.0);
    }
    (t - SKIN / dn).max(0.0)
}

// -------------------------------------------------------------------------------------------- rays

/// A ray hit against voxels or colliders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldRayHit {
    pub t: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub target: AnchorTarget,
    /// Hooks can anchor here (voxels always; colliders per their flag). A hook ray that hits a solid
    /// non-anchorable collider is blocked by it.
    pub anchorable: bool,
}

/// `bd_core::voxel::raycast` wants a sized `impl VoxelQuery`; this adapts `?Sized` worlds.
struct VoxRef<'a, V: ?Sized>(&'a V);
impl<V: VoxelQuery + ?Sized> VoxelQuery for VoxRef<'_, V> {
    #[inline]
    fn material(&self, v: IVec3) -> Material {
        self.0.material(v)
    }
}

fn ray_obb(origin: Vec3, dir: Vec3, c: &Collider, max: f32) -> Option<(f32, Vec3)> {
    let o = c.to_local(origin);
    let d = c.rot.inverse() * dir;
    let mut t_in = f32::NEG_INFINITY;
    let mut t_out = f32::INFINITY;
    let mut n = Vec3::ZERO;
    for i in 0..3 {
        let h = c.half[i];
        if d[i].abs() < 1e-9 {
            if o[i] < -h || o[i] > h {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d[i];
        let (mut t0, mut t1) = ((-h - o[i]) * inv, (h - o[i]) * inv);
        let mut ni = -axis(i);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
            ni = axis(i);
        }
        if t0 > t_in {
            t_in = t0;
            n = ni;
        }
        t_out = t_out.min(t1);
    }
    if t_in > t_out || t_out < 0.0 || t_in > max {
        return None;
    }
    if t_in < 0.0 {
        return Some((0.0, -dir));
    }
    Some((t_in, c.rot * n))
}

/// Nearest hit along a ray (unit `dir`) within `max_dist`. With `hook = true` the ray stops at anything
/// solid *or* anchorable (check [`WorldRayHit::anchorable`]); otherwise only at solid things.
pub fn raycast_world<V: VoxelQuery + ?Sized>(
    w: &CollisionWorld<V>,
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
    hook: bool,
) -> Option<WorldRayHit> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO || !(max_dist > 0.0) {
        return None;
    }
    let mut best = bd_core::voxel::raycast(&VoxRef(w.voxels), origin, dir, max_dist).map(|h| WorldRayHit {
        t: h.t,
        point: h.point,
        normal: if h.normal == IVec3::ZERO { -dir } else { h.normal.as_vec3() },
        target: AnchorTarget::Voxel(h.voxel),
        anchorable: true,
    });
    for c in w.colliders {
        if w.skip(c) || !(c.solid || (hook && c.anchorable)) {
            continue;
        }
        let lim = best.map_or(max_dist, |b| b.t);
        if let Some((t, n)) = ray_obb(origin, dir, c, lim) {
            if best.is_none_or(|b| t < b.t) {
                best = Some(WorldRayHit {
                    t,
                    point: origin + dir * t,
                    normal: n,
                    target: AnchorTarget::Collider(c.id),
                    anchorable: c.anchorable,
                });
            }
        }
    }
    best
}
