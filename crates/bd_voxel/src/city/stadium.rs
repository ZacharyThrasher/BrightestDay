//! A stadium bowl: an elliptical pitch ringed by seating terraces (0.5 m risers — walkable stairs all
//! the way up, so the bowl is a skate-park for bhops and slides), open ends on the long axis so play
//! flows through, tunnels under the side stands, an outer wall with a parapet, and four light towers
//! (tall anchors).

use bd_core::voxel::Material;
use glam::{Vec2, Vec3};

use super::{City, VS};
use crate::material::mat;
use crate::noise::datan2;

#[derive(Clone, Debug)]
pub struct StadiumSpec {
    pub center: Vec2,
    /// Pitch half-extents (ellipse radii, m).
    pub pitch: Vec2,
    /// Number of seating rows (each 1 m deep, 0.5 m rise).
    pub rows: i32,
    pub base_y: f32,
    /// Seat colours cycled around the bowl.
    pub seats: Vec<Material>,
    pub wall: Material,
    /// Half-width of the open ends on the long (x) axis (0 = closed bowl).
    pub open_ends: f32,
    pub light_h: f32,
}

impl StadiumSpec {
    pub fn outer(&self) -> Vec2 {
        self.pitch + Vec2::splat(self.rows as f32 + 1.0)
    }
}

impl City {
    pub fn stadium(&mut self, s: &StadiumSpec) {
        let c = s.center;
        let outer = s.outer();
        let rows = s.rows;
        let top_y = s.base_y + rows as f32 * VS;
        let lo = Vec3::new(c.x - outer.x - 1.0, s.base_y, c.y - outer.y - 1.0);
        let hi = Vec3::new(c.x + outer.x + 1.0, top_y + 2.0, c.y + outer.y + 1.0);
        let seats = s.seats.clone();
        let pitch = s.pitch;
        self.fill_with(lo, hi, |p, _, _| {
            let q = Vec2::new(p.x - c.x, p.z - c.y);
            // Radial distance beyond the pitch ellipse, in metres (approx: scale by the minor radius).
            let e = ((q.x / pitch.x) * (q.x / pitch.x) + (q.y / pitch.y) * (q.y / pitch.y)).sqrt();
            let dist = (e - 1.0) * pitch.y;
            let rel = p.y - s.base_y;
            if dist < 0.0 {
                // Pitch: grass with white lines (halfway line + 10 m yard lines) at ground level.
                return None;
            }
            // Open ends.
            if s.open_ends > 0.0 && q.y.abs() < s.open_ends && q.x.abs() > pitch.x * 0.6 {
                return None;
            }
            let row = dist.floor() as i32; // 1 m per row
            if row < rows {
                let h = (row + 1) as f32 * VS;
                if rel >= h {
                    return None;
                }
                // Tunnels under the side stands (on the short axis).
                if q.x.abs() < 2.0 && rel < 3.0 {
                    return None;
                }
                if rel >= h - VS {
                    let ang = datan2(q.y, q.x);
                    let sector = ((ang + 3.2) * 2.5).floor() as usize;
                    return Some(if row % 5 == 4 { mat::CONCRETE } else { seats[sector % seats.len()] });
                }
                return Some(mat::CONCRETE);
            }
            // Outer wall + parapet.
            if row == rows && rel < rows as f32 * VS + 1.5 {
                if q.x.abs() < 2.0 && rel < 3.0 {
                    return None;
                }
                return Some(if rel >= rows as f32 * VS + 1.0 { mat::NEON_CYAN } else { s.wall });
            }
            None
        });
        // Pitch markings.
        let (plo, phi) = (Vec3::new(c.x - pitch.x, s.base_y - VS, c.y - pitch.y), Vec3::new(c.x + pitch.x, s.base_y, c.y + pitch.y));
        self.fill_with(plo, phi, |p, _, old| {
            if old == bd_core::voxel::AIR {
                return None;
            }
            let q = Vec2::new(p.x - c.x, p.z - c.y);
            let e = (q.x / pitch.x) * (q.x / pitch.x) + (q.y / pitch.y) * (q.y / pitch.y);
            if e > 1.0 {
                return None;
            }
            let line = q.x.abs() < 0.25 || (q.x.rem_euclid(10.0) < VS && q.y.abs() < pitch.y * 0.6) || (e > 0.93);
            Some(if line { mat::PAINT_WHITE } else { mat::GRASS })
        });
        // Light towers on the diagonals.
        for (sx, sz) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            let p = c + Vec2::new(sx * (outer.x * 0.72), sz * (outer.y * 0.72));
            let y0 = s.base_y - VS;
            self.fill(Vec3::new(p.x - 0.5, y0, p.y - 0.5), Vec3::new(p.x + 0.5, s.base_y + s.light_h, p.y + 0.5), mat::STEEL);
            let head = Vec3::new(p.x - sx * 0.5, s.base_y + s.light_h, p.y - sz * 0.5);
            self.fill(head - Vec3::new(2.5, 0.0, 2.5), head + Vec3::new(2.5, 2.5, 2.5), mat::STEEL);
            self.fill(
                head - Vec3::new(2.0, -0.5, 2.0) - Vec3::new(sx, 0.0, sz) * 0.5,
                head + Vec3::new(2.0, 2.0, 2.0) - Vec3::new(sx, 0.0, sz) * 0.5,
                mat::NEON_AMBER,
            );
        }
    }
}
