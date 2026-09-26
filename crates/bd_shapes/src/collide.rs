//! Collision proxies: a handful of oriented boxes / capsules per construct fitted to the voxels of each
//! part (PCA-oriented, split where the fit is loose). The sim collides with these — never per voxel —
//! transforming each by its part's `pose` matrix.
//!
//! Space: metres, **model rest pose** (the same space as the part's voxels), so
//! `pose[part_index] * proxy` is the posed proxy.

use bd_core::shape::PartId;
use glam::{Mat3, Quat, Vec3};

use crate::anim::PartKind;
use crate::voxelize::{MAX_VOXELS, VoxelModel};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Proxy {
    /// Oriented box: centre, rotation, half extents.
    Obb { center: Vec3, rot: Quat, half: Vec3 },
    /// Segment `a`–`b` swept by `radius`.
    Capsule { a: Vec3, b: Vec3, radius: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionProxy {
    pub part: PartId,
    /// Index into `VoxelModel::parts` (and the pose array).
    pub part_index: usize,
    pub shape: Proxy,
}

/// Proxies with the default budget: 12 (24 for kaiju-sized models).
pub fn collision_proxies(model: &VoxelModel) -> Vec<CollisionProxy> {
    let budget = if model.voxel_count > MAX_VOXELS { 24 } else { 12 };
    collision_proxies_budget(model, budget)
}

struct Cluster {
    vs: f32,
    part_index: usize,
    pts: Vec<Vec3>,
    center: Vec3,
    axes: [Vec3; 3],
    half: Vec3,
}

impl Cluster {
    fn fit(vs: f32, part_index: usize, pts: Vec<Vec3>) -> Cluster {
        let n = pts.len().max(1) as f32;
        let mean = pts.iter().copied().sum::<Vec3>() / n;
        let mut cov = [[0.0f32; 3]; 3];
        for p in &pts {
            let d = *p - mean;
            let a = d.to_array();
            for (r, row) in cov.iter_mut().enumerate() {
                for (c, v) in row.iter_mut().enumerate() {
                    *v += a[r] * a[c];
                }
            }
        }
        let axes = jacobi_axes(cov);
        let hv = vs * 0.5;
        let mut mn = Vec3::splat(f32::MAX);
        let mut mx = Vec3::splat(f32::MIN);
        for p in &pts {
            let d = *p - mean;
            let l = Vec3::new(d.dot(axes[0]), d.dot(axes[1]), d.dot(axes[2]));
            mn = mn.min(l);
            mx = mx.max(l);
        }
        let mid = (mn + mx) * 0.5;
        let center = mean + axes[0] * mid.x + axes[1] * mid.y + axes[2] * mid.z;
        let half = (mx - mn) * 0.5 + Vec3::splat(hv);
        Cluster { vs, part_index, pts, center, axes, half }
    }
    fn volume(&self) -> f32 {
        8.0 * self.half.x * self.half.y * self.half.z
    }
    /// Empty space inside the box (m³) — what a split would win.
    fn waste(&self) -> f32 {
        let vv = self.vs.powi(3);
        (self.volume() - self.pts.len() as f32 * vv).max(0.0)
    }
    fn split(self) -> (Cluster, Cluster) {
        let k = if self.half.x >= self.half.y && self.half.x >= self.half.z { 0 } else if self.half.y >= self.half.z { 1 } else { 2 };
        let ax = self.axes[k];
        let c = self.center.dot(ax);
        let (a, b): (Vec<Vec3>, Vec<Vec3>) = self.pts.iter().partition(|p| p.dot(ax) < c);
        (Cluster::fit(self.vs, self.part_index, a), Cluster::fit(self.vs, self.part_index, b))
    }
    fn proxy(&self) -> Proxy {
        let mut idx = [0usize, 1, 2];
        idx.sort_by(|&a, &b| self.half[b].total_cmp(&self.half[a]));
        let (h0, h1, h2) = (self.half[idx[0]], self.half[idx[1]], self.half[idx[2]]);
        if h0 > 2.2 * h1 && h1 < 1.35 * h2 {
            let r = (h1 + h2) * 0.5;
            let ax = self.axes[idx[0]];
            let e = (h0 - r).max(0.0);
            return Proxy::Capsule { a: self.center - ax * e, b: self.center + ax * e, radius: r };
        }
        let m = Mat3::from_cols(self.axes[0], self.axes[1], self.axes[2]);
        let rot = Quat::from_mat3(&m).normalize();
        Proxy::Obb { center: self.center, rot, half: self.half }
    }
}

/// Symmetric 3×3 eigenvectors (Jacobi). Returns a right-handed orthonormal basis.
fn jacobi_axes(mut a: [[f32; 3]; 3]) -> [Vec3; 3] {
    let mut v = [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..24 {
        let (mut p, mut q, mut big) = (0, 1, 0.0f32);
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            if a[i][j].abs() > big {
                big = a[i][j].abs();
                p = i;
                q = j;
            }
        }
        if big < 1e-9 {
            break;
        }
        let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let t = if theta == 0.0 { 1.0 } else { t };
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        let mut b = a;
        for k in 0..3 {
            b[k][p] = c * a[k][p] - s * a[k][q];
            b[k][q] = s * a[k][p] + c * a[k][q];
        }
        a = b;
        for k in 0..3 {
            b[p][k] = c * a[p][k] - s * a[q][k];
            b[q][k] = s * a[p][k] + c * a[q][k];
        }
        a = b;
        for row in v.iter_mut() {
            let (vp, vq) = (row[p], row[q]);
            row[p] = c * vp - s * vq;
            row[q] = s * vp + c * vq;
        }
    }
    let x = Vec3::new(v[0][0], v[1][0], v[2][0]).normalize_or(Vec3::X);
    let y = Vec3::new(v[0][1], v[1][1], v[2][1]).normalize_or(Vec3::Y);
    let z = x.cross(y).normalize_or(Vec3::Z);
    let y = z.cross(x);
    [x, y, z]
}

/// Fit at most ~`budget` proxies (at least one per solid, non-flame part).
pub fn collision_proxies_budget(model: &VoxelModel, budget: usize) -> Vec<CollisionProxy> {
    let mut clusters: Vec<Cluster> = model
        .parts
        .iter()
        .enumerate()
        .filter(|(i, p)| !p.voxels.is_empty() && !matches!(model.rig.kinds.get(*i), Some(PartKind::Flame | PartKind::Seat)))
        .map(|(i, p)| Cluster::fit(p.voxel_size, i, p.voxels.iter().map(|(v, _)| p.voxel_center(*v)).collect()))
        .collect();
    let vv = model.voxel_size.powi(3);
    while clusters.len() < budget {
        let Some((k, w)) = clusters
            .iter()
            .enumerate()
            .filter(|(_, c)| c.pts.len() >= 8)
            .map(|(k, c)| (k, c.waste()))
            .max_by(|a, b| a.1.total_cmp(&b.1))
        else {
            break;
        };
        // Stop once the loosest box is already tight enough (< 40 voxels of air or mostly full).
        let c = &clusters[k];
        if w < 40.0 * vv || w < 0.35 * c.volume() {
            break;
        }
        let c = clusters.swap_remove(k);
        let (a, b) = c.split();
        if a.pts.is_empty() || b.pts.is_empty() {
            clusters.push(if a.pts.is_empty() { b } else { a });
            break;
        }
        clusters.push(a);
        clusters.push(b);
    }
    clusters.sort_by_key(|c| c.part_index);
    clusters
        .iter()
        .map(|c| CollisionProxy { part: model.parts[c.part_index].id, part_index: c.part_index, shape: c.proxy() })
        .collect()
}
