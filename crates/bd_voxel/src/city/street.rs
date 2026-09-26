//! Street networks.
//!
//! Streets are cut *into* land whose surface is at sidewalk level (`road_y + 0.5`): the road part is
//! recessed one voxel, so every curb is a 0.5 m step-up (≤ the 0.55 m step height: running over a curb
//! never costs speed). Markings are painted flush (no geometry): dashed lane lines, a double yellow
//! centre line (or a raised planted median), parking bays and zebra crossings next to every
//! intersection. Intersections stay plain asphalt.

use glam::{IVec3, Vec2};

use super::{City, Rect, VS, vc, vx};
use crate::material::mat;

#[derive(Clone, Copy, Debug)]
pub struct StreetSpec {
    pub rect: Rect,
    /// Direction of travel is along X (else along Z).
    pub along_x: bool,
    /// Road surface height; sidewalks are one voxel higher.
    pub road_y: f32,
    /// Sidewalk width on each side (inside `rect`).
    pub sidewalk: f32,
    /// Raised planted median width in the middle (0 = painted double yellow line).
    pub median: f32,
    /// Parking lane width on each side of the carriageway (0 = none).
    pub parking: f32,
    /// Lane width (for dashed lane lines).
    pub lane_w: f32,
}

impl StreetSpec {
    pub fn new(rect: Rect, along_x: bool, road_y: f32) -> Self {
        Self { rect, along_x, road_y, sidewalk: 3.0, median: 0.0, parking: 0.0, lane_w: 3.5 }
    }

    /// (across offset from the rect's low edge, along coordinate, across width).
    fn frame(&self, p: Vec2) -> (f32, f32, f32) {
        if self.along_x { (p.y - self.rect.min.y, p.x, self.rect.size().y) } else { (p.x - self.rect.min.x, p.y, self.rect.size().x) }
    }

    fn dir(&self) -> Vec2 {
        if self.along_x { Vec2::X } else { Vec2::Y }
    }

    /// Is `p` on the recessed carriageway (not sidewalk, not median)?
    pub fn on_road(&self, p: Vec2) -> bool {
        if !self.rect.contains(p) {
            return false;
        }
        let (a, _, w) = self.frame(p);
        if a < self.sidewalk || a >= w - self.sidewalk {
            return false;
        }
        (a - w * 0.5).abs() >= self.median * 0.5
    }

    fn on_median(&self, p: Vec2) -> bool {
        if !self.rect.contains(p) || self.median <= 0.0 {
            return false;
        }
        let (a, _, w) = self.frame(p);
        (a - w * 0.5).abs() < self.median * 0.5
    }
}

/// A set of streets built together (so intersections resolve cleanly).
#[derive(Clone, Debug, Default)]
pub struct StreetNet {
    pub streets: Vec<StreetSpec>,
}

impl StreetNet {
    pub fn add(&mut self, s: StreetSpec) -> &mut Self {
        self.streets.push(s);
        self
    }
    pub fn on_any_road(&self, p: Vec2) -> bool {
        self.streets.iter().any(|s| s.on_road(p))
    }
}

impl City {
    /// Cut and paint a street network into the land (whose surface must be at sidewalk level).
    pub fn streets(&mut self, net: &StreetNet) {
        // Pass 1: recess carriageways, pave sidewalks, plant medians.
        for s in &net.streets {
            let (x0, x1) = (vx(s.rect.min.x + 0.01), vx(s.rect.max.x - 0.01));
            let (z0, z1) = (vx(s.rect.min.y + 0.01), vx(s.rect.max.y - 0.01));
            let ry = vx(s.road_y); // sidewalk surface voxel (road surface voxel is ry - 1)
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let c = vc(IVec3::new(x, 0, z));
                    let p = Vec2::new(c.x, c.z);
                    if s.on_road(p) {
                        self.w.set(IVec3::new(x, ry, z), bd_core::voxel::AIR);
                        self.w.set(IVec3::new(x, ry - 1, z), mat::ASPHALT);
                    } else if !net.on_any_road(p) && self.w.get(IVec3::new(x, ry, z)) != bd_core::voxel::AIR {
                        let med = s.on_median(p);
                        let (a, _, w) = s.frame(p);
                        let edge = if med { ((a - w * 0.5).abs() - s.median * 0.5).abs() < VS } else { false };
                        let m = if med && !edge { mat::GRASS } else { mat::CONCRETE };
                        self.w.set(IVec3::new(x, ry, z), m);
                    }
                }
            }
        }
        // Pass 2: markings where exactly one carriageway covers the voxel.
        for (si, s) in net.streets.iter().enumerate() {
            let (x0, x1) = (vx(s.rect.min.x + 0.01), vx(s.rect.max.x - 0.01));
            let (z0, z1) = (vx(s.rect.min.y + 0.01), vx(s.rect.max.y - 0.01));
            let ry = vx(s.road_y) - 1;
            let e = s.dir();
            let others = |q: Vec2| net.streets.iter().enumerate().any(|(j, o)| j != si && o.on_road(q));
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let c = vc(IVec3::new(x, 0, z));
                    let p = Vec2::new(c.x, c.z);
                    if !s.on_road(p) || others(p) {
                        continue;
                    }
                    let (a, along, w) = s.frame(p);
                    let mid = w * 0.5;
                    let near_int = [1.0f32, -1.0].iter().any(|&sg| others(p + e * sg * 3.5) && !others(p + e * sg * 0.5));
                    let m = if near_int {
                        // Zebra crossing: 1 m stripes across the carriageway.
                        (a.rem_euclid(2.0) < 1.0).then_some(mat::PAINT_WHITE)
                    } else if s.median <= 0.0 && (a - mid).abs() < 0.5 {
                        Some(mat::PAINT_YELLOW)
                    } else {
                        let curb_in = s.sidewalk + s.parking;
                        let off = (a - mid).abs();
                        let edge = mid - curb_in; // carriageway half-width (from centre)
                        if s.parking > 0.0 && off > edge {
                            // Parking bays: a line every 6 m.
                            (along.rem_euclid(6.0) < VS).then_some(mat::PAINT_WHITE)
                        } else if s.parking > 0.0 && (off - edge).abs() < 0.25 {
                            Some(mat::PAINT_WHITE)
                        } else {
                            // Dashed lane lines between lanes of the same direction.
                            let inner = off - s.median * 0.5;
                            let lane = inner / s.lane_w;
                            let near_line = (lane - lane.round()).abs() * s.lane_w < 0.25 && lane.round() >= 1.0 && off < edge - 0.5;
                            (near_line && along.rem_euclid(6.0) < 3.0).then_some(mat::PAINT_WHITE)
                        }
                    };
                    if let Some(m) = m {
                        self.w.set(IVec3::new(x, ry, z), m);
                    }
                }
            }
        }
    }
}
