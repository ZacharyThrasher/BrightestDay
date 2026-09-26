//! Procedural acoustics from the voxel world itself.
//!
//! * [`ListenerSpace`]: 16 rays around the listener, a few per frame (amortised, cached), estimate
//!   how enclosed the space is, its mean free path, whether there is a ceiling, and the dominant
//!   distant reflecting surface. That drives the shared reverb (size, decay, wetness, early
//!   reflections, slapback). Because it reads the live voxels, a room *opens up* as its walls are
//!   smashed and closes when someone builds a wall — on every map, with no authored zones.
//! * [`occlusion`]: DDA through the voxels between listener and source; material-aware (leaves and
//!   glass let sound through), with a raised second ray so sound spills over low walls.
//! * [`SpacePreset`]: per-map baseline and limits (`assets/audio/maps.ron`).

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{Material, VoxelQuery, raycast, world_to_voxel};
use bd_voxel::{DebrisClass, material_def};
use glam::{IVec3, Vec3};
use serde::Deserialize;

use crate::dsp::{lerp, smoothstep};
use crate::reverb::ReverbParams;

pub const RAYS: usize = 16;
/// Ray length for the space estimate (m).
pub const SPACE_RANGE: f32 = 64.0;

/// Per-map acoustic baseline (open-air vs enclosed extremes) — the estimate interpolates.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct SpacePreset {
    /// Reverb when fully open (sky all around).
    pub open: ReverbParams,
    /// Reverb when fully enclosed (sealed room); size/decay also scale with the mean free path.
    pub enclosed: ReverbParams,
    /// Slapback level when one distant surface dominates (dB at full confidence).
    pub slap_db: f32,
    /// Vacuum: no reverb at all (Orbit above the atmosphere) — the director also switches snapshot.
    pub vacuum_above_y: Option<f32>,
}

impl Default for SpacePreset {
    fn default() -> Self {
        Self {
            open: ReverbParams {
                size: 1.4,
                rt60: 0.9,
                damping_hz: 5000.0,
                predelay_ms: 20.0,
                wet_db: -22.0,
                er_db: -30.0,
                slap_ms: 0.0,
                slap_db: -60.0,
            },
            enclosed: ReverbParams {
                size: 0.5,
                rt60: 0.8,
                damping_hz: 7000.0,
                predelay_ms: 6.0,
                wet_db: -9.0,
                er_db: -12.0,
                slap_ms: 0.0,
                slap_db: -60.0,
            },
            slap_db: -14.0,
            vacuum_above_y: None,
        }
    }
}

impl SpacePreset {
    /// 0..1 how far into vacuum a listener at height `y` is (10 m transition), for
    /// [`crate::director::MixInputs::vacuum`]. Maps without a vacuum line return 0.
    pub fn vacuum(&self, y: f32) -> f32 {
        self.vacuum_above_y.map_or(0.0, |v| smoothstep(v - 10.0, v + 10.0, y))
    }
}

/// What the rays found.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpaceEstimate {
    /// 0 = open sky all around, 1 = sealed.
    pub enclosure: f32,
    /// Mean distance to the nearest surface over all rays (misses count as the range), m.
    pub mean_free_path: f32,
    /// Something overhead within ~25 m.
    pub ceiling: bool,
    /// Dominant far reflector distance (m) and how confident (0..1) — drives slapback.
    pub slap_dist: f32,
    pub slap_conf: f32,
}

/// Amortised listener-space estimator.
pub struct ListenerSpace {
    dirs: [Vec3; RAYS],
    weight: [f32; RAYS],
    dist: [f32; RAYS],
    next: usize,
    filled: usize,
    pub preset: SpacePreset,
    est: SpaceEstimate,
    smoothed: ReverbParams,
    have: bool,
}

impl ListenerSpace {
    pub fn new(preset: SpacePreset) -> Self {
        // 10 horizontal-ish, 4 raised, 1 up, 1 down (the ground is always there: low weight).
        let mut dirs = [Vec3::Y; RAYS];
        let mut weight = [1.0; RAYS];
        for (i, d) in dirs.iter_mut().take(10).enumerate() {
            let a = i as f32 / 10.0 * std::f32::consts::TAU;
            *d = Vec3::new(a.cos(), 0.08, a.sin()).normalize();
        }
        for i in 0..4 {
            let a = (i as f32 + 0.5) / 4.0 * std::f32::consts::TAU;
            dirs[10 + i] = Vec3::new(a.cos(), 0.9, a.sin()).normalize();
            weight[10 + i] = 0.8;
        }
        dirs[14] = Vec3::Y;
        weight[14] = 1.2;
        dirs[15] = Vec3::NEG_Y;
        weight[15] = 0.3;
        let s = preset.open;
        Self {
            dirs,
            weight,
            dist: [SPACE_RANGE; RAYS],
            next: 0,
            filled: 0,
            preset,
            est: SpaceEstimate::default(),
            smoothed: s,
            have: false,
        }
    }

    /// Cast `rays` rays (round-robin) from `ear` and update the estimate. ~4 per frame keeps the
    /// whole sphere fresh every 4 frames at a few microseconds.
    pub fn step<W: VoxelQuery + ?Sized>(&mut self, world: &W, ear: Vec3, rays: usize) {
        if !ear.is_finite() {
            return;
        }
        for _ in 0..rays.min(RAYS) {
            let i = self.next;
            self.dist[i] = raycast(world, ear, self.dirs[i], SPACE_RANGE).map_or(SPACE_RANGE, |h| h.t.max(0.0));
            self.next = (self.next + 1) % RAYS;
            self.filled = (self.filled + 1).min(RAYS);
        }
        self.est = self.estimate();
    }

    /// Recompute the estimate from the cached ray distances.
    fn estimate(&self) -> SpaceEstimate {
        let mut enc = 0.0;
        let mut wsum = 0.0;
        let mut mfp = 0.0;
        for i in 0..RAYS {
            let d = self.dist[i];
            // A hit within 8 m encloses fully, fading out by 40 m.
            let e = 1.0 - smoothstep(8.0, 40.0, d);
            enc += e * self.weight[i];
            wsum += self.weight[i];
            mfp += d;
        }
        let ceiling = self.dist[14] < 25.0;
        // Slapback: horizontal rays hitting 10..60 m away (a canyon wall, a cliff) in open-ish space.
        let mut far = [0.0f32; 10];
        let mut nf = 0;
        let mut near_hits = 0;
        for &d in &self.dist[..10] {
            if (10.0..60.0).contains(&d) {
                far[nf] = d;
                nf += 1;
            } else if d < 10.0 {
                near_hits += 1;
            }
        }
        let (slap_dist, slap_conf) = if nf > 0 {
            far[..nf].sort_unstable_by(f32::total_cmp);
            let med = far[nf / 2];
            // Strong when a few directions hit something far while the rest is open / not close.
            (med, (nf as f32 / 4.0).min(1.0) * (1.0 - near_hits as f32 / 10.0))
        } else {
            (0.0, 0.0)
        };
        SpaceEstimate { enclosure: (enc / wsum).clamp(0.0, 1.0), mean_free_path: mfp / RAYS as f32, ceiling, slap_dist, slap_conf }
    }

    pub fn estimate_now(&self) -> SpaceEstimate {
        self.est
    }

    /// Reverb target for the current estimate (not smoothed).
    pub fn target(&self) -> ReverbParams {
        let e = self.est;
        let p = &self.preset;
        // Enclosure is what makes a room: weight it towards "enclosed" when there is a ceiling.
        let t = (e.enclosure * if e.ceiling { 1.15 } else { 0.8 }).clamp(0.0, 1.0);
        let mut r = p.open.lerp(&p.enclosed, t);
        // Bigger free path → bigger, longer space (canyon / hall), small → closet.
        let size = (e.mean_free_path / 22.0).clamp(0.3, 1.8);
        r.size = lerp(r.size, size, t.max(0.35));
        r.rt60 *= lerp(0.75, 1.35, smoothstep(0.3, 1.6, size));
        r.predelay_ms = lerp(r.predelay_ms, (e.mean_free_path * 0.6).clamp(3.0, 40.0), t);
        if e.slap_conf > 0.05 && t < 0.85 {
            r.slap_ms = 2.0 * e.slap_dist / 343.0 * 1000.0;
            r.slap_db = p.slap_db + 20.0 * (e.slap_conf.max(0.05)).log10();
        } else {
            r.slap_db = -60.0;
        }
        r.sanitized()
    }

    /// Smoothed reverb parameters (call once per frame with the frame time).
    pub fn reverb(&mut self, dt: f32) -> ReverbParams {
        let target = self.target();
        if !self.have {
            self.smoothed = target;
            self.have = self.filled >= RAYS;
            return target;
        }
        let k = 1.0 - (-dt.max(0.0) / 0.35).exp();
        // Slapback delay jumps are gentler handled by the reverb glide; smooth everything here.
        self.smoothed = self.smoothed.lerp(&target, k);
        self.smoothed
    }
}

/// How much one voxel of `m` blocks sound (0 = transparent, 1 = solid rock).
pub fn transmission_loss(m: Material) -> f32 {
    match material_def(m).debris {
        DebrisClass::Leaves => 0.08,
        DebrisClass::Crystal => 0.45,
        DebrisClass::Wood => 0.7,
        _ => 1.0,
    }
}

/// Accumulated blocking along a segment through the voxel grid (DDA; at most `max_steps` voxels).
pub fn segment_loss<W: VoxelQuery + ?Sized>(world: &W, from: Vec3, to: Vec3, max_steps: u32) -> f32 {
    let d = to - from;
    let len = d.length();
    if !len.is_finite() || len < 1e-3 {
        return 0.0;
    }
    let dir = d / len;
    let mut v = world_to_voxel(from);
    let end = world_to_voxel(to);
    let step = IVec3::new(if dir.x > 0.0 { 1 } else { -1 }, if dir.y > 0.0 { 1 } else { -1 }, if dir.z > 0.0 { 1 } else { -1 });
    let inv = |c: f32| if c != 0.0 { 1.0 / c.abs() } else { f32::INFINITY };
    let t_delta = Vec3::new(inv(dir.x), inv(dir.y), inv(dir.z)) * VOXEL_SIZE;
    let vmin = v.as_vec3() * VOXEL_SIZE;
    let first = |o: f32, lo: f32, dc: f32| {
        if dc == 0.0 {
            f32::INFINITY
        } else if dc > 0.0 {
            (lo + VOXEL_SIZE - o) / dc.abs()
        } else {
            (o - lo) / dc.abs()
        }
    };
    let mut t_max = Vec3::new(first(from.x, vmin.x, dir.x), first(from.y, vmin.y, dir.y), first(from.z, vmin.z, dir.z));
    let mut loss = 0.0;
    for _ in 0..max_steps {
        let t = t_max.min_element();
        if t > len || v == end {
            break;
        }
        if t_max.x <= t_max.y && t_max.x <= t_max.z {
            v.x += step.x;
            t_max.x += t_delta.x;
        } else if t_max.y <= t_max.z {
            v.y += step.y;
            t_max.y += t_delta.y;
        } else {
            v.z += step.z;
            t_max.z += t_delta.z;
        }
        if v == end {
            break;
        }
        let m = world.material(v);
        if m != 0 {
            loss += transmission_loss(m);
            if loss > 6.0 {
                break;
            }
        }
    }
    loss
}

/// Occlusion 0..1 between listener and source: the lesser of the direct path and a path raised
/// 2 m at both ends (sound spills over low walls and debris), mapped so one voxel of rock ≈ 0.45
/// and a wall ≥ 1.5 m ≈ 0.95.
pub fn occlusion<W: VoxelQuery + ?Sized>(world: &W, ear: Vec3, src: Vec3) -> f32 {
    if !ear.is_finite() || !src.is_finite() {
        return 0.0;
    }
    let steps = 400;
    let direct = segment_loss(world, ear, src, steps);
    if direct <= 0.0 {
        return 0.0;
    }
    let up = Vec3::Y * 2.0;
    let over = segment_loss(world, ear + up, src + up, steps) + 0.3; // longer path: a little loss
    let l = direct.min(over);
    1.0 - (-0.6 * l).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_voxel::mat;

    /// A hand-built grid.
    struct Grid {
        solid: Box<dyn Fn(IVec3) -> bool>,
    }
    impl VoxelQuery for Grid {
        fn material(&self, v: IVec3) -> Material {
            if (self.solid)(v) { mat::STONE } else { 0 }
        }
    }

    #[test]
    fn open_field_vs_sealed_room() {
        let floor = Grid { solid: Box::new(|v| v.y < 0) };
        let mut s = ListenerSpace::new(SpacePreset::default());
        s.step(&floor, Vec3::new(0.0, 1.6, 0.0), RAYS);
        let open = s.estimate_now();
        // A 10 m cube room.
        let room = Grid { solid: Box::new(|v| v.y < 0 || v.y > 12 || v.x.abs() > 10 || v.z.abs() > 10) };
        let mut r = ListenerSpace::new(SpacePreset::default());
        r.step(&room, Vec3::new(0.0, 1.6, 0.0), RAYS);
        let closed = r.estimate_now();
        assert!(open.enclosure < 0.15, "open {:?}", open);
        assert!(closed.enclosure > 0.8, "closed {:?}", closed);
        assert!(closed.ceiling && !open.ceiling);
        let (to, tc) = (s.target(), r.target());
        assert!(tc.wet_db > to.wet_db + 6.0, "room is wetter: {to:?} vs {tc:?}");
    }

    #[test]
    fn smashing_a_wall_opens_the_room() {
        use std::cell::Cell;
        use std::rc::Rc;
        let broken = Rc::new(Cell::new(false));
        let b = broken.clone();
        let room = Grid {
            solid: Box::new(move |v| {
                let wall = v.y < 0 || v.y > 12 || v.x.abs() > 10 || v.z.abs() > 10;
                // Smash: the whole +x half of the room is gone.
                wall && !(b.get() && v.x > 0 && v.y >= 0)
            }),
        };
        let mut s = ListenerSpace::new(SpacePreset::default());
        s.step(&room, Vec3::new(0.0, 1.6, 0.0), RAYS);
        let before = s.estimate_now().enclosure;
        broken.set(true);
        s.step(&room, Vec3::new(0.0, 1.6, 0.0), RAYS);
        let after = s.estimate_now().enclosure;
        assert!(after < before - 0.2, "{before} -> {after}");
    }

    #[test]
    fn walls_occlude_and_leaves_barely() {
        let wall = Grid { solid: Box::new(|v| v.x == 4 && v.y < 20) };
        let o = occlusion(&wall, Vec3::new(0.0, 1.6, 0.0), Vec3::new(5.0, 1.6, 0.0));
        let clear = occlusion(&wall, Vec3::new(0.0, 1.6, 0.0), Vec3::new(1.5, 1.6, 0.0));
        assert!(o > 0.3, "{o}");
        assert_eq!(clear, 0.0);
        struct Leafy;
        impl VoxelQuery for Leafy {
            fn material(&self, v: IVec3) -> Material {
                if v.x == 4 && v.y < 20 { mat::LEAVES_GREEN } else { 0 }
            }
        }
        let l = occlusion(&Leafy, Vec3::new(0.0, 1.6, 0.0), Vec3::new(5.0, 1.6, 0.0));
        assert!(l < o * 0.3, "leaves {l} vs stone {o}");
    }
}
