//! Bridges and causeways: a road deck (asphalt, dashed centre line, raised sidewalks with a low
//! barrier that has gaps — a risky lane over water) on indestructible piers, plus optional twin
//! through-arches with lit hangers or a cable-stayed pylon. Arches, cross-braces, pylons and stays are
//! all swing anchors over the water.

use bd_core::voxel::Material;
use glam::{IVec3, Vec2, Vec3};

use super::{City, VS, vx};
use crate::material::mat;
use crate::noise::{PI, dsin, smoothstep};

#[derive(Clone, Copy, Debug)]
pub enum BridgeStyle {
    Plain,
    /// Twin steel through-arches over `t0..t1` (fractions of the length), `rise` above the road, with
    /// hangers of `hanger` material and a neon strip along the top of each rib.
    Arch {
        t0: f32,
        t1: f32,
        rise: f32,
        hanger: Material,
        neon: Material,
    },
    /// An A-frame pylon at fraction `t` rising to `top_y` with fanned stay cables.
    CableStay {
        t: f32,
        top_y: f32,
        cable: Material,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct BridgeSpec {
    /// Centreline endpoints (x, z). The deck runs from `a` to `b`.
    pub a: Vec2,
    pub b: Vec2,
    pub road_y: f32,
    pub width: f32,
    pub sidewalk: f32,
    pub thickness: f32,
    pub pier_every: f32,
    /// Leave 4 m gaps every 10 m in the edge barrier.
    pub barrier_gaps: bool,
    pub style: BridgeStyle,
}

impl BridgeSpec {
    pub fn new(a: Vec2, b: Vec2, road_y: f32, width: f32) -> Self {
        Self { a, b, road_y, width, sidewalk: 2.0, thickness: 1.5, pier_every: 12.0, barrier_gaps: true, style: BridgeStyle::Plain }
    }
}

impl City {
    pub fn bridge(&mut self, s: &BridgeSpec) {
        let len = (s.b - s.a).length();
        let e = (s.b - s.a) / len;
        let n = Vec2::new(-e.y, e.x);
        let hw = s.width * 0.5;
        let (lo2, hi2) = {
            let corners = [s.a + n * (hw + 2.0), s.a - n * (hw + 2.0), s.b + n * (hw + 2.0), s.b - n * (hw + 2.0)];
            let mut lo = corners[0];
            let mut hi = corners[0];
            for c in corners {
                lo = lo.min(c);
                hi = hi.max(c);
            }
            (lo, hi)
        };
        let ry = s.road_y;
        let bottom = ry - s.thickness;
        // Deck, sidewalks, barrier, piers.
        self.fill_with(Vec3::new(lo2.x, 0.0, lo2.y), Vec3::new(hi2.x, ry + 1.0, hi2.y), |p, _, _| {
            let q = Vec2::new(p.x, p.z) - s.a;
            let t = q.dot(e);
            let ac = q.dot(n);
            if !(0.0..len).contains(&t) || ac.abs() >= hw {
                return None;
            }
            let edge = ac.abs() >= hw - s.sidewalk;
            if p.y < bottom {
                // Piers (indestructible) every `pier_every`.
                let pm = t.rem_euclid(s.pier_every);
                return (pm < 1.5 && ac.abs() < hw - 1.0).then_some(mat::TOWER_CORE);
            }
            if p.y < ry - VS {
                return Some(mat::CONCRETE);
            }
            if p.y < ry {
                if edge {
                    return Some(mat::CONCRETE);
                }
                let dash = ac.abs() < 0.5 && t.rem_euclid(6.0) < 3.0;
                return Some(if dash { mat::PAINT_WHITE } else { mat::ASPHALT });
            }
            if p.y < ry + VS {
                return edge.then_some(mat::CONCRETE); // sidewalk (0.5 m step-up)
            }
            // Barrier (0.5 m above the sidewalk), with gaps.
            let gap = s.barrier_gaps && t.rem_euclid(10.0) < 4.0;
            (ac.abs() >= hw - VS && !gap).then_some(mat::STUCCO_WHITE)
        });
        // Anchor hints on the piers.
        let mut t = 0.75;
        while t < len {
            let c = s.a + e * t;
            let v = IVec3::new(vx(c.x), vx(bottom - 1.0).max(0), vx(c.y));
            self.anchor(v);
            t += s.pier_every;
        }
        match s.style {
            BridgeStyle::Plain => {}
            BridgeStyle::Arch { t0, t1, rise, hanger, neon } => {
                let (s0, s1) = (t0 * len, t1 * len);
                let rib_y = |sp: f32| ry - 1.0 + rise * dsin(PI * ((sp - s0) / (s1 - s0)).clamp(0.0, 1.0));
                for sg in [-1.0f32, 1.0] {
                    let off = sg * (hw - 0.5);
                    let mut sp = s0;
                    while sp <= s1 {
                        let c2 = s.a + e * sp + n * off;
                        let y = rib_y(sp);
                        self.w.fill_sphere(Vec3::new(c2.x, y, c2.y), 0.8, mat::STEEL, false);
                        self.w.fill_sphere(Vec3::new(c2.x, y + 0.75, c2.y), 0.3, neon, false);
                        sp += 0.25;
                    }
                    // Hangers every 4 m.
                    let mut sp = s0 + 4.0;
                    while sp < s1 - 3.0 {
                        let c2 = s.a + e * sp + n * off;
                        let y = rib_y(sp);
                        if y > ry + 2.0 {
                            self.line(Vec3::new(c2.x, ry + 0.75, c2.y), Vec3::new(c2.x, y - 0.5, c2.y), hanger, true);
                        }
                        sp += 4.0;
                    }
                }
                // Cross braces between the ribs where they're high (overhead anchors).
                let mut sp = s0 + 4.0;
                while sp < s1 - 3.0 {
                    let y = rib_y(sp);
                    if y > ry + 7.0 {
                        let a = s.a + e * sp + n * (hw - 0.5);
                        let b = s.a + e * sp - n * (hw - 0.5);
                        self.tube(Vec3::new(a.x, y, a.y), Vec3::new(b.x, y, b.y), 0.5, mat::STEEL, false);
                    }
                    sp += 6.0;
                }
            }
            BridgeStyle::CableStay { t, top_y, cable } => {
                let sp = t * len;
                let base = s.a + e * sp;
                // Pylon legs (A-frame), core inside a white shell.
                let leg_off = |y: f32| {
                    let k = smoothstep(ry + 6.0, top_y - 8.0, y);
                    (hw + 1.25) * (1.0 - k) + 0.75 * k
                };
                let mut y = 0.25;
                while y < top_y {
                    for sg in [-1.0f32, 1.0] {
                        let c2 = base + n * sg * leg_off(y);
                        self.fill(
                            Vec3::new(c2.x - 1.0, y - 0.25, c2.y - 1.0),
                            Vec3::new(c2.x + 1.0, y + 0.25, c2.y + 1.0),
                            mat::STUCCO_WHITE,
                        );
                        self.fill(
                            Vec3::new(c2.x - 0.5, y - 0.25, c2.y - 0.5),
                            Vec3::new(c2.x + 0.5, y + 0.25, c2.y + 0.5),
                            mat::TOWER_CORE,
                        );
                    }
                    y += VS;
                }
                // Cross beam under the deck and a lit tip.
                let a = base + n * (hw + 1.25);
                let b = base - n * (hw + 1.25);
                let (lo, hi) = (a.min(b), a.max(b));
                self.fill(Vec3::new(lo.x - 1.0, bottom - 1.5, lo.y - 1.0), Vec3::new(hi.x + 1.0, bottom, hi.y + 1.0), mat::STUCCO_WHITE);
                self.fill(Vec3::new(base.x - 1.0, top_y, base.y - 1.0), Vec3::new(base.x + 1.0, top_y + 2.0, base.y + 1.0), mat::NEON_PINK);
                let pv = IVec3::new(vx(base.x), vx(top_y * 0.5), vx(base.y));
                self.anchor(pv);
                // Fanned stays, both planes, both directions.
                for i in 0..6 {
                    let hy = top_y - 3.0 - i as f32 * 2.5;
                    let reach = 8.0 + i as f32 * 5.5;
                    for sg in [-1.0f32, 1.0] {
                        let top = base + n * sg * leg_off(hy);
                        for dir in [-1.0f32, 1.0] {
                            let tsp = sp + dir * reach;
                            if !(0.0..len).contains(&tsp) {
                                continue;
                            }
                            let foot = s.a + e * tsp + n * sg * (hw - 0.25);
                            self.line(Vec3::new(top.x, hy, top.y), Vec3::new(foot.x, ry + 0.75, foot.y), cable, true);
                        }
                    }
                }
            }
        }
    }
}
