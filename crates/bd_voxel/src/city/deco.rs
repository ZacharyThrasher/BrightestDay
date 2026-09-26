//! Art-deco / streamline-moderne hotels (Ocean Drive).
//!
//! A hollow pastel stucco box on a lot, built in the lot's local frame ([`LotFrame`]: `u` along the
//! front, `v` depth): rounded front corners with wrap-around ribbon windows, "eyebrow" ledges over
//! every window band, a white central pier, a raised front porch with a low wall (vault it), a striped
//! awning (slide under it, mount it from the porch wall), small front balconies, a stepped ziggurat
//! parapet and a vertical fin sign outlined in neon that rises above the roof (a tether anchor). The
//! lobby has doors front and back, so the strip can be run through.

use bd_core::voxel::Material;
use glam::{Vec2, Vec3};

use super::{City, Facing, LotFrame, Rect, VS};
use crate::material::mat;

#[derive(Clone, Debug)]
pub struct HotelSpec {
    pub rect: Rect,
    pub facing: Facing,
    pub base_y: f32,
    pub floors: i32,
    pub floor_h: f32,
    pub body: Material,
    pub trim: Material,
    pub glass: Material,
    pub neon: Material,
    pub awning: Option<Material>,
    /// Front corner radius (m).
    pub round_r: f32,
    pub eyebrows: bool,
    /// Vertical fin sign: position along the front (0..1) and how far it rises above the roof.
    pub fin: Option<(f32, f32)>,
    pub ziggurat: bool,
    pub porch: bool,
    pub balconies: bool,
}

impl HotelSpec {
    pub fn new(rect: Rect, facing: Facing, base_y: f32, floors: i32, body: Material) -> Self {
        Self {
            rect,
            facing,
            base_y,
            floors: floors.max(1),
            floor_h: 3.5,
            body,
            trim: mat::STUCCO_WHITE,
            glass: mat::GLASS_BLUE,
            neon: mat::NEON_PINK,
            awning: None,
            round_r: 3.0,
            eyebrows: true,
            fin: None,
            ziggurat: false,
            porch: true,
            balconies: false,
        }
    }
    pub fn height(&self) -> f32 {
        self.floors as f32 * self.floor_h
    }
    /// Roof surface height.
    pub fn roof_y(&self) -> f32 {
        self.base_y + self.height() + VS
    }
}

impl City {
    pub fn hotel(&mut self, s: &HotelSpec) {
        let f = LotFrame::new(s.rect, s.facing);
        let (wd, dp) = (f.width, f.depth);
        let r = s.round_r.min(wd * 0.5).min(dp * 0.5);
        let fh = s.floor_h;
        let h = s.height();
        let top_k = s.floors;
        let fin_h = s.fin.map_or(0.0, |(_, e)| e);
        let fin_u = s.fin.map(|(t, _)| wd * t);
        let zig_h = if s.ziggurat { 3.0 } else { 0.0 };
        let wr = f.world_rect(-1.0, -4.0, wd + 1.0, dp);
        let lo = Vec3::new(wr.min.x, s.base_y, wr.min.y);
        let hi = Vec3::new(wr.max.x, s.base_y + h + 1.5 + fin_h.max(zig_h), wr.max.y);
        let mid = wd * 0.5;
        // Local signed distance to the body (front corners rounded).
        let sdf = move |u: f32, v: f32| -> f32 {
            if v < r && (u < r || u > wd - r) {
                let cu = if u < r { r } else { wd - r };
                return (Vec2::new(u, v) - Vec2::new(cu, r)).length() - r;
            }
            let q = Vec2::new((u - mid).abs() - mid, (v - dp * 0.5).abs() - dp * 0.5);
            q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
        };
        self.fill_with(lo, hi, |p, _, _| {
            let l = f.local(Vec2::new(p.x, p.z));
            let (u, v) = (l.x, l.y);
            let rel = p.y - s.base_y;
            let k = (rel / fh).floor() as i32;
            let within = rel - k as f32 * fh;
            let d = sdf(u, v);
            let wall = d > -0.7 && d <= 0.0;
            let corner = v < r && (u < r || u > wd - r);

            // Fin sign (front, rises above the roof).
            if let Some(fu) = fin_u
                && (u - fu).abs() < 0.5
                && (-2.5..0.5).contains(&v)
                && rel >= fh
                && rel < h + 1.5 + fin_h
            {
                let edge = v < -2.0 || rel >= h + 1.0 + fin_h || rel < fh + VS;
                let letter = (-2.0..-1.0).contains(&v) && (rel - fh).rem_euclid(1.5) < 1.0 && rel > fh + 0.75 && rel < h + fin_h;
                return Some(if edge || letter { s.neon } else { s.trim });
            }
            if k >= top_k {
                // Roof, parapet, ziggurat.
                if within < VS {
                    return (d <= 0.0).then_some(mat::CONCRETE);
                }
                let du = (u - mid).abs();
                let zh = if !s.ziggurat || v >= 1.0 || v < 0.0 {
                    0.0
                } else if du < wd * 0.07 {
                    3.0
                } else if du < wd * 0.13 {
                    2.0
                } else if du < wd * 0.2 {
                    1.0
                } else {
                    0.0
                };
                let top = 1.5 + zh;
                if within < top && wall {
                    if zh > 0.0 && within >= top - VS {
                        return Some(s.trim);
                    }
                    if zh > 0.0 && v < VS && (within - 1.5 - zh * 0.5).abs() < 0.25 {
                        return Some(s.neon);
                    }
                    return Some(s.body);
                }
                return None;
            }
            if within < VS && k >= 1 && d <= -0.7 {
                return Some(mat::CONCRETE);
            }
            if wall {
                let front = v < 1.0 || corner;
                let back = v > dp - 1.0;
                // Coordinate along this face for window rhythm.
                let along = if front || back { u } else { v };
                if k == 0 {
                    if (front || back) && (u - mid).abs() < if back { 1.5 } else { 2.0 } && within < 3.0 {
                        return None; // doors (run-through lobby)
                    }
                    if front && (0.5..2.75).contains(&within) && along.rem_euclid(4.0) > 1.0 && !corner {
                        return Some(s.glass);
                    }
                    return Some(if within >= 3.0 { s.trim } else { s.body });
                }
                if front && (u - mid).abs() < 1.0 {
                    return Some(s.trim); // central pier
                }
                if within < VS {
                    return Some(s.body);
                }
                if (1.0..2.5).contains(&within) && (corner || along.rem_euclid(3.0) > 0.75) {
                    return Some(s.glass);
                }
                return Some(s.body);
            }
            if d > 0.0 {
                // Eyebrows over every window band (front + sides).
                let eyebrow_k = k >= 1 || s.awning.is_none();
                if s.eyebrows && eyebrow_k && d <= 1.0 && (2.5..3.0).contains(&within) && v < dp - 1.0 {
                    let top_band = k == top_k - 1 && v < 0.5;
                    return Some(if top_band && d > 0.5 { s.neon } else { s.trim });
                }
                // Front balconies either side of the pier.
                if s.balconies && k >= 1 && (-1.5..0.0).contains(&v) && !corner {
                    let du = (u - mid).abs();
                    if du > 1.5 && du < wd * 0.36 {
                        if within < VS {
                            return Some(s.trim);
                        }
                        if within < 1.0 && v < -1.0 {
                            return Some(s.trim);
                        }
                    }
                }
                if k == 0 && v < 0.0 {
                    // Porch: raised floor + low front wall with a gap for the steps.
                    if s.porch && v >= -3.0 && u >= 0.0 && u < wd && !corner {
                        if within < VS {
                            return Some(s.trim);
                        }
                        if within < 1.0 && v < -2.5 && (u - mid).abs() > 1.5 {
                            return Some(s.body);
                        }
                    }
                    // Awning: striped canopy at 3 m + valance.
                    if let Some(a) = s.awning
                        && v >= -3.5
                        && u >= 0.5
                        && u < wd - 0.5
                        && !corner
                    {
                        let stripe = if (u / 1.0).floor() as i32 % 2 == 0 { a } else { mat::STUCCO_WHITE };
                        if (3.0..3.5).contains(&within) || ((2.5..3.0).contains(&within) && v < -3.0) {
                            return Some(stripe);
                        }
                    }
                }
            }
            None
        });
    }
}
