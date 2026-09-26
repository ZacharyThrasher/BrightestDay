//! Small allocation-free geometry helpers for combat queries (swept projectiles, swing arcs, blast
//! radii). Movement has its own, more careful collision code in [`crate::movement`].

use bd_core::voxel::Aabb;
use bd_core::{Quat, Vec3};

/// Grow a box by `r` on every side.
#[inline]
pub fn inflate(a: &Aabb, r: f32) -> Aabb {
    Aabb { min: a.min - Vec3::splat(r), max: a.max + Vec3::splat(r) }
}

/// Closest point of `a` to `p`.
#[inline]
pub fn closest_point(a: &Aabb, p: Vec3) -> Vec3 {
    p.clamp(a.min, a.max)
}

#[inline]
pub fn dist_sq_point_aabb(a: &Aabb, p: Vec3) -> f32 {
    (closest_point(a, p) - p).length_squared()
}

#[inline]
fn axis(i: usize) -> Vec3 {
    match i {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    }
}

/// Segment `p0 → p1` against an axis-aligned box (slab test). Returns the entry fraction in
/// `[0, 1]` and the entry face normal (pointing out of the box). A segment that starts inside
/// returns `t = 0` with a normal opposing the motion.
pub fn segment_aabb(p0: Vec3, p1: Vec3, b: &Aabb) -> Option<(f32, Vec3)> {
    let d = p1 - p0;
    let mut t_in = 0.0f32;
    let mut t_out = 1.0f32;
    let mut n = -d.normalize_or_zero();
    for i in 0..3 {
        if d[i].abs() < 1e-9 {
            if p0[i] < b.min[i] || p0[i] > b.max[i] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d[i];
        let (mut t0, mut t1) = ((b.min[i] - p0[i]) * inv, (b.max[i] - p0[i]) * inv);
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
        if t_in > t_out {
            return None;
        }
    }
    Some((t_in, n))
}

/// Segment against an oriented box (`center`, half extents `half`, rotation `rot`), optionally
/// inflated by `r` (treats the segment as a swept sphere, approximately).
pub fn segment_obb(p0: Vec3, p1: Vec3, center: Vec3, half: Vec3, rot: Quat, r: f32) -> Option<(f32, Vec3)> {
    let inv = rot.inverse();
    let a = inv * (p0 - center);
    let b = inv * (p1 - center);
    let h = half + Vec3::splat(r);
    segment_aabb(a, b, &Aabb { min: -h, max: h }).map(|(t, n)| (t, rot * n))
}

/// Does a sphere overlap an oriented box?
pub fn sphere_obb(p: Vec3, r: f32, center: Vec3, half: Vec3, rot: Quat) -> bool {
    let l = rot.inverse() * (p - center);
    let c = l.clamp(-half, half);
    (c - l).length_squared() <= r * r
}

/// World AABB of an oriented box.
pub fn obb_bounds(center: Vec3, half: Vec3, rot: Quat) -> Aabb {
    let m = glam::Mat3::from_quat(rot);
    let e = m.x_axis.abs() * half.x + m.y_axis.abs() * half.y + m.z_axis.abs() * half.z;
    Aabb::from_center_half(center, e)
}

/// Rotation that makes local −Z point along `dir` with +Y as close to world up as possible
/// (yaw/pitch style, no roll).
pub fn look_rotation(dir: Vec3) -> Quat {
    let d = dir.normalize_or_zero();
    if d == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    let yaw = (-d.x).atan2(-d.z);
    let pitch = d.y.clamp(-1.0, 1.0).asin();
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)
}

/// Yaw-only rotation facing horizontal direction `dir` (−Z forward).
pub fn yaw_rotation(dir: Vec3) -> Quat {
    let d = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    if d == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_y((-d.x).atan2(-d.z))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_hits_box_face() {
        let b = Aabb { min: Vec3::splat(-1.0), max: Vec3::splat(1.0) };
        let (t, n) = segment_aabb(Vec3::new(-3.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0), &b).unwrap();
        assert!((t - 1.0 / 3.0).abs() < 1e-5);
        assert_eq!(n, -Vec3::X);
        assert!(segment_aabb(Vec3::new(-3.0, 2.0, 0.0), Vec3::new(3.0, 2.0, 0.0), &b).is_none());
    }

    #[test]
    fn look_rotation_matches_forward() {
        let d = Vec3::new(0.3, 0.4, -0.8).normalize();
        let q = look_rotation(d);
        assert!((q * Vec3::NEG_Z - d).length() < 1e-4);
    }

    #[test]
    fn obb_segment_rotated() {
        let rot = Quat::from_rotation_y(0.7);
        let hit = segment_obb(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -5.0), Vec3::ZERO, Vec3::splat(1.0), rot, 0.0);
        assert!(hit.is_some());
    }
}
