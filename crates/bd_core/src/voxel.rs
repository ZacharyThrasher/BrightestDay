//! Minimal voxel query contract + geometry helpers shared by the sim (collision, tethers) and the voxel
//! crate (which implements [`VoxelQuery`] for the real chunked world). Keeping the trait here lets the
//! simulation be unit-tested against tiny hand-built grids.

use glam::{IVec3, Vec3};

use crate::VOXEL_SIZE;

/// Material id stored per voxel. 0 is always air.
pub type Material = u8;
pub const AIR: Material = 0;

pub trait VoxelQuery {
    /// Material at voxel coordinate `v` (out of bounds = AIR).
    fn material(&self, v: IVec3) -> Material;

    #[inline]
    fn is_solid(&self, v: IVec3) -> bool {
        self.material(v) != AIR
    }
}

/// Voxel coordinate containing world point `p`.
#[inline]
pub fn world_to_voxel(p: Vec3) -> IVec3 {
    (p / VOXEL_SIZE).floor().as_ivec3()
}

/// World-space minimum corner of voxel `v`.
#[inline]
pub fn voxel_min(v: IVec3) -> Vec3 {
    v.as_vec3() * VOXEL_SIZE
}

/// World-space centre of voxel `v`.
#[inline]
pub fn voxel_center(v: IVec3) -> Vec3 {
    (v.as_vec3() + Vec3::splat(0.5)) * VOXEL_SIZE
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_center_half(center: Vec3, half: Vec3) -> Self {
        Self { min: center - half, max: center + half }
    }
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn half(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }
    pub fn translate(&self, d: Vec3) -> Self {
        Self { min: self.min + d, max: self.max + d }
    }
    pub fn intersects(&self, o: &Aabb) -> bool {
        self.min.cmplt(o.max).all() && self.max.cmpgt(o.min).all()
    }
    /// Inclusive range of voxel coordinates this box overlaps.
    pub fn voxel_range(&self) -> (IVec3, IVec3) {
        let eps = Vec3::splat(1e-4);
        (world_to_voxel(self.min + eps), world_to_voxel(self.max - eps))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// The solid voxel that was hit.
    pub voxel: IVec3,
    /// World-space hit point on the voxel surface.
    pub point: Vec3,
    /// Outward face normal of the hit face (axis aligned).
    pub normal: IVec3,
    /// Distance along the ray.
    pub t: f32,
}

/// Amanatides–Woo DDA through the voxel grid. `dir` need not be normalised; `max_dist` is in metres
/// along the normalised direction.
pub fn raycast(world: &impl VoxelQuery, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let mut v = world_to_voxel(origin);
    if world.is_solid(v) {
        return Some(RayHit { voxel: v, point: origin, normal: IVec3::ZERO, t: 0.0 });
    }
    let step = IVec3::new(
        if dir.x > 0.0 { 1 } else { -1 },
        if dir.y > 0.0 { 1 } else { -1 },
        if dir.z > 0.0 { 1 } else { -1 },
    );
    let inv = Vec3::new(
        if dir.x != 0.0 { 1.0 / dir.x.abs() } else { f32::INFINITY },
        if dir.y != 0.0 { 1.0 / dir.y.abs() } else { f32::INFINITY },
        if dir.z != 0.0 { 1.0 / dir.z.abs() } else { f32::INFINITY },
    );
    let t_delta = inv * VOXEL_SIZE;
    let vmin = voxel_min(v);
    let first = |o: f32, lo: f32, d: f32, inv: f32| {
        if d == 0.0 {
            f32::INFINITY
        } else if d > 0.0 {
            (lo + VOXEL_SIZE - o) * inv
        } else {
            (o - lo) * inv
        }
    };
    let mut t_max = Vec3::new(
        first(origin.x, vmin.x, dir.x, inv.x),
        first(origin.y, vmin.y, dir.y, inv.y),
        first(origin.z, vmin.z, dir.z, inv.z),
    );
    loop {
        let (axis, t) = if t_max.x < t_max.y && t_max.x < t_max.z {
            (0, t_max.x)
        } else if t_max.y < t_max.z {
            (1, t_max.y)
        } else {
            (2, t_max.z)
        };
        if t > max_dist {
            return None;
        }
        let mut normal = IVec3::ZERO;
        match axis {
            0 => {
                v.x += step.x;
                t_max.x += t_delta.x;
                normal.x = -step.x;
            }
            1 => {
                v.y += step.y;
                t_max.y += t_delta.y;
                normal.y = -step.y;
            }
            _ => {
                v.z += step.z;
                t_max.z += t_delta.z;
                normal.z = -step.z;
            }
        }
        if world.is_solid(v) {
            return Some(RayHit { voxel: v, point: origin + dir * t, normal, t });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Floor;
    impl VoxelQuery for Floor {
        fn material(&self, v: IVec3) -> Material {
            if v.y < 0 { 1 } else { AIR }
        }
    }

    #[test]
    fn ray_hits_floor() {
        let hit = raycast(&Floor, Vec3::new(0.3, 5.0, 0.3), Vec3::NEG_Y, 100.0).unwrap();
        assert_eq!(hit.normal, IVec3::Y);
        assert!((hit.point.y - 0.0).abs() < 1e-4);
        assert!((hit.t - 5.0).abs() < 1e-4);
    }

    #[test]
    fn ray_misses_when_short() {
        assert!(raycast(&Floor, Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y, 4.0).is_none());
    }
}
