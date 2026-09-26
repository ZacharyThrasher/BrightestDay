//! Glass skyscrapers.
//!
//! A tower is a hollow shell: a one-voxel curtain wall (glass between a mullion grid, a spandrel band
//! at every floor line), interior floor slabs every `floor_h`, and an indestructible slender core from
//! the bedrock to the roof. Options: setbacks (terraces with parapets = ledges at varied heights),
//! rounded corners, wrap-around balconies (Brickell condos), open "sky lobby" floors you can swing
//! through, a glass lobby with doors on every side (run-through), and a crown.

use bd_core::voxel::Material;
use glam::{IVec3, Vec2, Vec3};

use super::{City, Rect, VS, vx};
use crate::material::mat;
use crate::noise::hash3;

/// What sits on the roof.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Crown {
    /// Parapet + a rooftop plant room.
    Flat,
    /// Glowing band around the top floor and the parapet.
    NeonBand(Material),
    /// A steel mast rising from the core, neon tip.
    Spire { height: f32, tip: Material },
    /// A raised helipad with painted markings.
    Helipad,
    /// Art-deco stepped crown: three shrinking tiers with neon edges.
    Stepped { neon: Material },
}

#[derive(Clone, Debug)]
pub struct TowerSpec {
    pub rect: Rect,
    /// Ground (sidewalk) level the lobby stands on.
    pub base_y: f32,
    pub floors: i32,
    /// Metres per floor (slab to slab). 4 m by default.
    pub floor_h: f32,
    /// Setbacks: (first floor of the tier, inset per side in metres, cumulative). Sorted by floor.
    pub setbacks: Vec<(i32, f32)>,
    pub corner_r: f32,
    pub glass: Material,
    /// Second glass tint scattered over panels (reflections), with probability.
    pub glass_alt: Option<(Material, f32)>,
    /// Sky-reflection gradient: panels above this fraction of the height (with a ragged, per-panel
    /// edge) use this glass — e.g. the upper floors mirroring a sunset.
    pub glass_top: Option<(Material, f32)>,
    pub frame: Material,
    pub spandrel: Material,
    /// Height of the spandrel band at each floor line (m; 1.0 = solid-looking bands, 0.5 = sleek
    /// all-glass curtain wall).
    pub spandrel_h: f32,
    /// Mullion spacing in voxels.
    pub mullion_every: i32,
    /// Wrap-around balcony slabs (1.5 m) with a 1 m rail on every floor above the lobby.
    pub balconies: bool,
    pub balcony_rail: Material,
    /// Floors that are open-air (columns only) — swing-through gaps.
    pub open_floors: Vec<i32>,
    /// Door openings in the middle of every side of the lobby.
    pub lobby_doors: bool,
    pub crown: Crown,
    /// Half-width of the indestructible core (m).
    pub core_half: f32,
}

impl TowerSpec {
    pub fn new(rect: Rect, base_y: f32, floors: i32) -> Self {
        Self {
            rect,
            base_y,
            floors: floors.max(1),
            floor_h: 4.0,
            setbacks: Vec::new(),
            corner_r: 0.0,
            glass: mat::GLASS_TEAL,
            glass_alt: None,
            glass_top: None,
            frame: mat::MULLION,
            spandrel: mat::CONCRETE,
            spandrel_h: 1.0,
            mullion_every: 4,
            balconies: false,
            balcony_rail: mat::STUCCO_WHITE,
            open_floors: Vec::new(),
            lobby_doors: true,
            crown: Crown::Flat,
            core_half: 1.0,
        }
    }

    pub fn height(&self) -> f32 {
        self.floors as f32 * self.floor_h
    }

    /// Roof surface height (top of the roof slab).
    pub fn roof_y(&self) -> f32 {
        self.base_y + self.height() + VS
    }

    /// Cumulative inset of the tier containing floor `k`.
    pub fn inset(&self, k: i32) -> f32 {
        self.setbacks.iter().filter(|(f, _)| *f <= k).map(|(_, i)| *i).fold(0.0, f32::max)
    }

    /// Footprint of floor `k`.
    pub fn footprint(&self, k: i32) -> Rect {
        self.rect.inset(self.inset(k))
    }

    /// Top-tier footprint (what the roof covers).
    pub fn roof_rect(&self) -> Rect {
        self.footprint(self.floors - 1)
    }
}

/// Wall shell thickness test: voxel centres with `-SHELL < d <= 0` are wall (one voxel on straight
/// faces, watertight on rounded corners).
const SHELL: f32 = 0.7;

impl City {
    pub fn tower(&mut self, s: &TowerSpec) {
        let seed = self.seed();
        let fh = s.floor_h;
        let h = s.height();
        let c = s.rect.center();
        let grow = if s.balconies { 1.6 } else { 0.1 };
        let lo = Vec3::new(s.rect.min.x - grow, s.base_y, s.rect.min.y - grow);
        let hi = Vec3::new(s.rect.max.x + grow, s.base_y + h + 1.5, s.rect.max.y + grow);
        let neon_top = match s.crown {
            Crown::NeonBand(m) => Some(m),
            _ => None,
        };
        let top_k = s.floors;
        self.fill_with(lo, hi, |p, v, _| {
            let rel = p.y - s.base_y;
            let q = Vec2::new(p.x, p.z);
            // Core.
            if (p.x - c.x).abs() < s.core_half && (p.z - c.y).abs() < s.core_half && rel < h + VS {
                return Some(mat::TOWER_CORE);
            }
            let k = (rel / fh).floor() as i32;
            let within = rel - k as f32 * fh;
            let kk = k.min(top_k - 1);
            let ins = s.inset(kk);
            let ins_below = if k == 0 { ins } else { s.inset(k - 1) };
            let r = (s.corner_r - ins).max(0.0);
            let rect = s.rect.inset(ins);
            let d = rect.sdf(q, r);
            let d_below = if ins_below == ins { d } else { s.rect.inset(ins_below).sdf(q, (s.corner_r - ins_below).max(0.0)) };

            if k >= top_k {
                // Roof slab + parapet.
                if within < VS {
                    return (d_below <= 0.0).then_some(mat::ROOF_DARK);
                }
                if within < 1.5 && d > -SHELL && d <= 0.0 {
                    return Some(neon_top.unwrap_or(s.frame));
                }
                return None;
            }
            let open = s.open_floors.contains(&k);
            let wall = d > -SHELL && d <= 0.0;
            // Which face (for the mullion grid / doors): x-facing if the x distance dominates.
            let rh = rect.half();
            let dq = (q - rect.center()).abs() - rh;
            let x_face = dq.x > dq.y;
            let along_i = if x_face { v.z } else { v.x };
            let along_off = if x_face { p.z - c.y } else { p.x - c.x };

            if within < VS && k >= 1 {
                // Floor line.
                if d <= -SHELL {
                    return Some(mat::CONCRETE);
                }
                if wall {
                    return Some(match neon_top {
                        Some(n) if k == top_k - 1 => n,
                        _ => s.spandrel,
                    });
                }
                if d_below <= 0.0 {
                    return Some(mat::ROOF_DARK); // setback terrace
                }
                if s.balconies && !open && d <= 1.5 {
                    return Some(mat::CONCRETE);
                }
                return None;
            }
            if wall {
                if k == 0 {
                    // Lobby: tall glass, doors mid-side.
                    if s.lobby_doors && along_off.abs() < 2.0 && within < 3.5 {
                        return None;
                    }
                    return Some(if along_i.rem_euclid(s.mullion_every * 2) == 0 { s.frame } else { s.glass });
                }
                if open {
                    return (along_i.rem_euclid(s.mullion_every * 2) == 0 || within < s.spandrel_h).then_some(s.frame);
                }
                if within < s.spandrel_h {
                    return Some(match neon_top {
                        Some(n) if k == top_k - 1 => n,
                        _ => s.spandrel,
                    });
                }
                if along_i.rem_euclid(s.mullion_every) == 0 {
                    return Some(s.frame);
                }
                let panel_hash = |salt: u32| {
                    let panel = along_i.div_euclid(s.mullion_every);
                    (hash3(panel, k, if x_face { 1 } else { 2 }, seed ^ salt) >> 8) as f32 * (1.0 / 16_777_216.0)
                };
                if let Some((top, frac)) = s.glass_top
                    && rel / h > frac + 0.25 * (panel_hash(0x7a11) - 0.5)
                {
                    return Some(top);
                }
                if let Some((alt, prob)) = s.glass_alt {
                    let panel = along_i.div_euclid(s.mullion_every);
                    let hsh = hash3(panel, k, if x_face { 1 } else { 2 }, seed);
                    if (hsh >> 8) as f32 * (1.0 / 16_777_216.0) < prob {
                        return Some(alt);
                    }
                }
                return Some(s.glass);
            }
            // Parapet on a setback terrace (lower tier's edge, 1 m).
            if d > 0.0 && d_below > -SHELL && d_below <= 0.0 && ins_below < ins && within < 1.5 {
                return Some(s.frame);
            }
            // Balcony rail.
            if s.balconies && k >= 1 && !open && d > 1.0 && d <= 1.5 && within < 1.5 && d_below > 0.0 {
                return Some(s.balcony_rail);
            }
            None
        });
        // Closed floors (render-only sealing): generous box so skybridge-linked floors seal together.
        for k in 1..s.floors {
            if s.open_floors.contains(&k) {
                continue;
            }
            let y0 = s.base_y + k as f32 * fh;
            let seed = Vec3::new(c.x + s.core_half + 0.75, y0 + 1.25, c.y);
            let lo = Vec3::new(s.rect.min.x - 40.0, y0 - 1.0, s.rect.min.y - 40.0);
            let hi = Vec3::new(s.rect.max.x + 40.0, y0 + fh + 1.0, s.rect.max.y + 40.0);
            self.interior(seed, lo, hi);
        }
        // Core down to the bedrock + anchors.
        let cl = Vec3::new(c.x - s.core_half, 0.0, c.y - s.core_half);
        let ch = Vec3::new(c.x + s.core_half, s.base_y, c.y + s.core_half);
        self.fill(cl, ch, mat::TOWER_CORE);
        let mut y = 0.0;
        while y < s.base_y + h {
            self.anchor(IVec3::new(vx(c.x), vx(y), vx(c.y)));
            y += 8.0;
        }
        self.crown(s);
    }

    fn crown(&mut self, s: &TowerSpec) {
        let roof = s.roof_y();
        let rr = s.roof_rect();
        let c = rr.center();
        let half = rr.half();
        let plant = |city: &mut City, frac: f32, h: f32| {
            let ph = half * frac;
            city.fill(Vec3::new(c.x - ph.x, roof, c.y - ph.y), Vec3::new(c.x + ph.x, roof + h, c.y + ph.y), mat::ROOF_DARK);
            city.fill(
                Vec3::new(c.x - ph.x - VS, roof + h - VS, c.y - ph.y - VS),
                Vec3::new(c.x + ph.x + VS, roof + h, c.y + ph.y + VS),
                s.frame,
            );
        };
        match s.crown {
            Crown::Flat => plant(self, 0.35, 3.5),
            Crown::NeonBand(m) => {
                plant(self, 0.3, 3.0);
                let ph = half * 0.3;
                self.fill(
                    Vec3::new(c.x - ph.x - VS, roof + 2.0, c.y - ph.y - VS),
                    Vec3::new(c.x + ph.x + VS, roof + 2.5, c.y + ph.y + VS),
                    m,
                );
            }
            Crown::Spire { height, tip } => {
                plant(self, 0.3, 3.0);
                let top = (roof + height).min(95.0);
                self.fill(Vec3::new(c.x - 0.5, roof, c.y - 0.5), Vec3::new(c.x + 0.5, top - 1.5, c.y + 0.5), mat::STEEL);
                self.fill(Vec3::new(c.x - 0.5, top - 1.5, c.y - 0.5), Vec3::new(c.x + 0.5, top, c.y + 0.5), tip);
                // A crossbar halfway up (swing grip).
                let yb = roof + (top - roof) * 0.45;
                self.fill(Vec3::new(c.x - 2.0, yb, c.y - 0.25), Vec3::new(c.x + 2.0, yb + 0.5, c.y + 0.25), mat::STEEL);
            }
            Crown::Helipad => {
                let r = half.x.min(half.y) - 1.0;
                self.fill_with(Vec3::new(c.x - r, roof, c.y - r), Vec3::new(c.x + r, roof + VS, c.y + r), |p, _, _| {
                    let d = Vec2::new(p.x - c.x, p.z - c.y);
                    let l = d.length();
                    if l > r {
                        return None;
                    }
                    if l > r - 0.6 {
                        return Some(mat::PAINT_YELLOW);
                    }
                    // Painted "H".
                    let (ax, az) = (d.x.abs(), d.y.abs());
                    let hs = r * 0.45;
                    let leg = (ax > hs * 0.55 && ax < hs * 0.85 && az < hs) || (ax < hs * 0.6 && az < 0.3);
                    Some(if leg { mat::PAINT_WHITE } else { mat::ROOF_DARK })
                });
            }
            Crown::Stepped { neon } => {
                let mut y = roof;
                for (i, frac) in [0.8f32, 0.6, 0.4].iter().enumerate() {
                    let ph = half * *frac;
                    let th = 2.5 - i as f32 * 0.5;
                    self.fill(Vec3::new(c.x - ph.x, y, c.y - ph.y), Vec3::new(c.x + ph.x, y + th, c.y + ph.y), s.spandrel);
                    self.fill(Vec3::new(c.x - ph.x, y + th - VS, c.y - ph.y), Vec3::new(c.x + ph.x, y + th, c.y + ph.y), neon);
                    self.fill(
                        Vec3::new(c.x - ph.x + VS, y + th - VS, c.y - ph.y + VS),
                        Vec3::new(c.x + ph.x - VS, y + th, c.y + ph.y - VS),
                        s.spandrel,
                    );
                    y += th;
                }
                let top = (y + 6.0).min(95.0);
                self.fill(Vec3::new(c.x - 0.25, y, c.y - 0.25), Vec3::new(c.x + 0.25, top, c.y + 0.25), neon);
            }
        }
    }
}
