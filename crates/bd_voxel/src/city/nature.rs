//! Palms and beach furniture.
//!
//! Palms are the city's low tether anchors: a curved, ringed trunk (choppable — cut it and the crown
//! falls) topped by a coconut cluster and a spray of arching, drooping fronds (thin 6-connected
//! blades, so they read from a distance and catch hooks without blocking movement much).

use bd_core::voxel::Material;
use glam::{IVec3, Vec2, Vec3};

use super::{City, VS, vc};
use crate::material::mat;
use crate::noise::{PI, TAU, dcos, dsin};

#[derive(Clone, Copy, Debug)]
pub struct PalmSpec {
    /// Ground point at the foot of the trunk.
    pub base: Vec3,
    pub height: f32,
    /// Horizontal offset of the crown relative to the base (the lean).
    pub lean: Vec2,
    pub fronds: i32,
    pub frond_len: f32,
    pub trunk_r: f32,
}

impl PalmSpec {
    pub fn new(base: Vec3, height: f32) -> Self {
        Self { base, height, lean: Vec2::ZERO, fronds: 8, frond_len: height * 0.42, trunk_r: 0.45 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LifeguardSpec {
    /// Sand point under the middle of the hut.
    pub base: Vec3,
    /// Horizontal direction the hut looks (towards the sea).
    pub facing: Vec2,
    pub body: Material,
    pub stripe: Material,
    pub roof: Material,
}

impl City {
    /// A sphere where each voxel's material is chosen by `f(voxel)`.
    fn sphere_by(&mut self, c: Vec3, r: f32, f: impl Fn(IVec3) -> Material) {
        let lo = ((c - r) / VS).floor().as_ivec3();
        let hi = ((c + r) / VS).floor().as_ivec3();
        self.w.fill_with(lo, hi, |v, _| ((vc(v) - c).length() <= r || v == (c / VS).floor().as_ivec3()).then(|| f(v)));
    }

    pub fn palm(&mut self, s: &PalmSpec) {
        let b = s.base - Vec3::Y * 1.0;
        let top = s.base + Vec3::new(s.lean.x, s.height, s.lean.y);
        // The trunk curves: straight-ish near the ground, leaning out near the top.
        let ctrl = s.base + Vec3::new(s.lean.x * 0.1, s.height * 0.62, s.lean.y * 0.1);
        let n = ((s.height / 0.2) as i32).max(8);
        let ring = |v: IVec3| if (v.y as f32 * VS * 1.4).floor() as i32 % 3 == 0 { mat::WOOD } else { mat::PALM_TRUNK };
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let p = b * (1.0 - t) * (1.0 - t) + ctrl * 2.0 * t * (1.0 - t) + top * t * t;
            let flare = 1.0 + 0.6 * (1.0 - t * 8.0).max(0.0);
            self.sphere_by(p, s.trunk_r * (1.15 - 0.3 * t) * flare, ring);
        }
        // Coconuts.
        self.w.fill_sphere(top - Vec3::Y * 0.5, 0.7, mat::DARK_WOOD, true);
        // Fronds.
        let seed = self.seed();
        let a0 = (seed % 1000) as f32 * (TAU / 1000.0);
        for k in 0..s.fronds {
            let a = a0 + TAU * k as f32 / s.fronds as f32 + ((seed >> (k % 16)) & 7) as f32 * 0.04;
            let dir = Vec2::new(dcos(a), dsin(a));
            let side = Vec3::new(-dir.y, 0.0, dir.x);
            let len = s.frond_len * (0.85 + 0.3 * (((seed >> (k * 3 % 24)) & 7) as f32 / 7.0));
            let rise = if k % 3 == 0 { 0.45 } else { 0.3 };
            let m = if k % 4 == 3 { mat::LEAVES_GREEN } else { mat::PALM_FROND };
            let pt = |t: f32| top + Vec3::new(dir.x * len * t, len * (rise * t - 0.8 * t * t), dir.y * len * t);
            let steps = ((len / 0.3) as i32).max(6);
            let mut prev = top;
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                let p = pt(t);
                self.line(prev, p, m, true);
                // Blade: leaflets either side, drooping at the edges.
                let w = len * 0.13 * dsin(PI * t.min(0.95)) + 0.2;
                if i % 2 == 0 && w > 0.4 {
                    for sg in [-1.0f32, 1.0] {
                        let tip = p + side * sg * w - Vec3::Y * (w * 0.5);
                        self.line(p, tip, m, true);
                    }
                }
                prev = p;
            }
        }
    }

    /// A Miami lifeguard stand: bright hut on stilts, sea-facing window band, flag, ramp to the sand.
    pub fn lifeguard(&mut self, s: &LifeguardSpec) {
        let f = s.facing.normalize_or(Vec2::X);
        let side = Vec2::new(-f.y, f.x);
        let g = s.base.y;
        let c = Vec2::new(s.base.x, s.base.z);
        let deck_y = g + 2.5;
        let at = |u: f32, v: f32| c + f * u + side * v; // u: towards the sea, v: sideways
        let box_uv = |city: &mut City, u0: f32, u1: f32, v0: f32, v1: f32, y0: f32, y1: f32, m: Material| {
            let a = at(u0, v0);
            let b = at(u1, v1);
            let (lo, hi) = (a.min(b), a.max(b));
            city.fill(Vec3::new(lo.x, y0, lo.y), Vec3::new(hi.x, y1, hi.y), m);
        };
        // Stilts.
        for (u, v) in [(-1.5, -1.5), (-1.5, 1.0), (1.0, -1.5), (1.0, 1.0)] {
            box_uv(self, u, u + 0.5, v, v + 0.5, g - 1.0, deck_y, mat::STUCCO_WHITE);
        }
        // Deck with a sea-side porch.
        box_uv(self, -2.0, 3.0, -2.0, 2.0, deck_y, deck_y + VS, mat::WOOD);
        // Porch rail.
        box_uv(self, 2.5, 3.0, -2.0, 2.0, deck_y + VS, deck_y + 1.0, s.stripe);
        // Hut walls (hollow), sea window band, back door.
        let (h0, h1) = (deck_y + VS, deck_y + 3.0);
        let cu = 0.0;
        self.fill_with(
            {
                let a = at(-2.0, -2.0).min(at(1.5, 1.5));
                Vec3::new(a.x - 0.5, h0, a.y - 0.5)
            },
            {
                let b = at(-2.0, -2.0).max(at(1.5, 1.5));
                Vec3::new(b.x + 0.5, h1, b.y + 0.5)
            },
            |p, _, _| {
                let q = Vec2::new(p.x, p.z) - c;
                let (u, v) = (q.dot(f), q.dot(side));
                if !(-2.0..1.5).contains(&u) || !(-2.0..1.5).contains(&v) {
                    return None;
                }
                let wall = u < -1.5 || u >= 1.0 || v < -1.5 || v >= 1.0;
                if !wall {
                    return None;
                }
                let y = p.y - h0;
                if u >= 1.0 && (0.75..1.75).contains(&y) {
                    return Some(mat::GLASS_TEAL);
                }
                if u < -1.5 && (v - cu).abs() < 0.75 && y < 2.0 {
                    return None; // door
                }
                Some(if (y / 0.5).floor() as i32 % 3 == 1 { s.stripe } else { s.body })
            },
        );
        // Roof with overhang + neon-free bright trim.
        box_uv(self, -2.5, 2.5, -2.5, 2.0, h1, h1 + VS, s.roof);
        box_uv(self, -1.5, 1.5, -1.5, 1.0, h1 + VS, h1 + 1.0, s.roof);
        // Flag pole + flag.
        let pole = at(-1.8, 1.6);
        self.fill(Vec3::new(pole.x, h1 + VS, pole.y), Vec3::new(pole.x + VS, h1 + 3.5, pole.y + VS), mat::STEEL);
        let fl = at(-1.8, 1.6) + side * 0.5;
        let fl2 = fl + side * 1.0;
        let (lo, hi) = (fl.min(fl2), fl.max(fl2));
        self.fill(Vec3::new(lo.x, h1 + 2.5, lo.y), Vec3::new(hi.x + VS, h1 + 3.5, hi.y + VS), mat::PAINT_RED);
        // Ramp down to the sand at the back (0.5 m steps).
        let start = at(-2.0, 0.0);
        let back = -f;
        for i in 1..=5 {
            let y1 = deck_y - i as f32 * VS;
            let p0 = start + back * ((i - 1) as f32 * 0.9);
            let p1 = p0 + back * 0.9;
            let a = p0 - side * 0.75;
            let b = p1 + side * 0.75;
            let (lo, hi) = (a.min(b), a.max(b));
            self.fill(Vec3::new(lo.x, g - 0.5, lo.y), Vec3::new(hi.x, y1 + VS, hi.y), mat::WOOD);
        }
    }
}
