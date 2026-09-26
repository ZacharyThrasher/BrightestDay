//! A docked cruise ship: a walkable vertical playground.
//!
//! Hollow hull (white, navy stripe and boot-top, porthole rows, an inner deck), a teak main deck with a
//! perimeter rail (gaps for gangways), a stacked superstructure whose levels step back fore and aft
//! (terraced decks = ledges at every height), promenades along both sides with lifeboats hanging
//! overhead, a sun deck with a pool, a bridge with wings that overhang the water, a mast, and a raked
//! red funnel with "whale-tail" fins — the ship's tallest anchor.

use bd_core::voxel::Material;
use glam::{IVec3, Vec2, Vec3};

use super::{City, VS, vx};
use crate::material::mat;
use crate::noise::smoothstep;

#[derive(Clone, Debug)]
pub struct ShipSpec {
    /// Midship (x, z).
    pub center: Vec2,
    /// +1: bow points +Z; −1: bow points −Z.
    pub bow: f32,
    pub length: f32,
    pub beam: f32,
    pub keel_y: f32,
    /// Main deck surface height.
    pub deck_y: f32,
    pub levels: i32,
    pub level_h: f32,
    pub bow_len: f32,
    pub hull: Material,
    pub stripe: Material,
    pub funnel: Material,
    /// Along-ship positions (m from midship, + towards the bow) of rail gaps for gangways…
    pub gangways: Vec<f32>,
    /// …on this side (−1 = −X side, +1 = +X side).
    pub gangway_side: f32,
}

impl ShipSpec {
    pub fn new(center: Vec2, length: f32, beam: f32, keel_y: f32, deck_y: f32) -> Self {
        Self {
            center,
            bow: 1.0,
            length,
            beam,
            keel_y,
            deck_y,
            levels: 3,
            level_h: 4.0,
            bow_len: 16.0,
            hull: mat::HULL_WHITE,
            stripe: mat::HULL_NAVY,
            funnel: mat::PAINT_RED,
            gangways: Vec::new(),
            gangway_side: -1.0,
        }
    }
    /// Sun-deck surface height.
    pub fn top_y(&self) -> f32 {
        self.deck_y + self.levels as f32 * self.level_h
    }
    /// Along-ship extent `[aft, fore]` of superstructure level `i` (m from midship).
    pub fn level_span(&self, i: i32) -> (f32, f32) {
        let l2 = self.length * 0.5;
        (-l2 + 5.0 + i as f32 * 4.5, l2 - self.bow_len - 2.0 - i as f32 * 2.5)
    }
    /// Half-width of the superstructure.
    pub fn house_hw(&self) -> f32 {
        self.beam * 0.5 - 2.0
    }
    /// World position from ship-local (across x, height y, along a).
    pub fn world(&self, x: f32, y: f32, a: f32) -> Vec3 {
        Vec3::new(self.center.x + x, y, self.center.y + a * self.bow)
    }
    /// Along-ship position of the funnel centre.
    pub fn funnel_a(&self) -> f32 {
        -self.length * 0.5 + 17.0
    }
}

impl City {
    pub fn cruise_ship(&mut self, s: &ShipSpec) {
        let l2 = s.length * 0.5;
        let b2 = s.beam * 0.5;
        let dy = s.deck_y;
        let lh = s.level_h;
        let top = s.top_y();
        let hhw = s.house_hw();
        let rake = 4.0;
        // Hull half-width at (along a, height y).
        let hull_hw = move |a: f32, y: f32| -> f32 {
            let hf = ((y - s.keel_y) / (dy - s.keel_y)).clamp(0.0, 1.0);
            let bow_start = l2 - s.bow_len;
            let mut hw = b2 * (0.78 + 0.22 * smoothstep(0.0, 0.45, hf));
            if a > bow_start {
                let t = (a - bow_start) / (s.bow_len + rake * hf);
                if t >= 1.0 {
                    return -1.0;
                }
                hw *= 1.0 - t * t;
            } else if a < -l2 + 3.0 {
                let t = (-l2 + 3.0 - a) / 3.0;
                if t >= 1.0 {
                    return -1.0;
                }
                hw *= (1.0 - t * t).sqrt();
            }
            hw
        };
        let spans: Vec<(f32, f32)> = (0..s.levels).map(|i| s.level_span(i)).collect();
        let in_level = |i: i32, a: f32, x: f32| -> bool {
            if i < 0 || i >= s.levels {
                return false;
            }
            let (a0, a1) = spans[i as usize];
            a >= a0 && a < a1 && x.abs() < hhw
        };
        let pool_a = {
            let (a0, a1) = spans[(s.levels - 1) as usize];
            (a0 + (a1 - a0) * 0.45, a0 + (a1 - a0) * 0.75)
        };
        let lo = s.world(-b2 - 2.0, s.keel_y, -l2 - 1.0);
        let hi = s.world(b2 + 2.0, top + 1.5, l2 + rake + 1.0);
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let bridge_a = spans[(s.levels - 1) as usize].1;
        self.fill_with(lo, hi, |p, _, _| {
            let x = p.x - s.center.x;
            let a = (p.z - s.center.y) * s.bow;
            let y = p.y;
            if y < dy {
                // Hull.
                let hw = hull_hw(a, y);
                if hw < 0.0 || x.abs() > hw {
                    return None;
                }
                // Shell: the outer band in plan, the keel, and any voxel whose neighbour below / fore / aft
                // is outside the hull (bow rake, stern cap) so the hull is watertight.
                let out = |h: f32| h < 0.0 || x.abs() > h - 0.2;
                let shell = x.abs() > hw - 0.7
                    || y < s.keel_y + VS
                    || out(hull_hw(a, y - VS))
                    || out(hull_hw(a - VS, y))
                    || out(hull_hw(a + VS, y));
                if !shell {
                    // Inner deck + main deck slab.
                    if (dy - 4.5..dy - 4.0).contains(&y) || y >= dy - VS {
                        return Some(mat::WOOD);
                    }
                    return None;
                }
                if y < 3.5 {
                    return Some(s.stripe); // boot-top
                }
                if (dy - 2.5..dy - 1.5).contains(&y) {
                    return Some(s.stripe);
                }
                let port_row = (dy - 5.5..dy - 5.0).contains(&y) || (dy - 7.5..dy - 7.0).contains(&y);
                if port_row && a.rem_euclid(2.0) < VS && a > -l2 + 3.0 && a < l2 - s.bow_len * 0.5 {
                    return Some(mat::GLASS_BLUE);
                }
                return Some(s.hull);
            }
            let rel = y - dy;
            let lvl = (rel / lh).floor() as i32;
            let within = rel - lvl as f32 * lh;
            // Bridge wings: overhang the hull on the top level's floor.
            if lvl == s.levels - 1 && within < VS && a >= bridge_a - 4.0 && a < bridge_a && x.abs() < b2 + 1.5 {
                return Some(mat::WOOD);
            }
            if lvl == s.levels - 1
                && (VS..1.0).contains(&within)
                && a >= bridge_a - 4.0
                && a < bridge_a
                && x.abs() >= b2 + 1.0
                && x.abs() < b2 + 1.5
            {
                return Some(s.hull);
            }
            // Main-deck rail (with gangway gaps).
            if lvl == 0 && within < VS {
                let hw = hull_hw(a, dy - 0.01);
                if hw > 0.0 && x.abs() <= hw && x.abs() > hw - 0.7 && !in_level(0, a, x) {
                    let gap = x * s.gangway_side > 0.0 && s.gangways.iter().any(|g| (a - g).abs() < 1.5);
                    return (!gap).then_some(s.hull);
                }
            }
            if lvl >= s.levels {
                // Sun deck (roof of the top level) + rail around it.
                if within < 1.0 && in_level(s.levels - 1, a, x) {
                    let (a0, a1) = spans[(s.levels - 1) as usize];
                    let edge = x.abs() > hhw - VS || a < a0 + VS || a >= a1 - VS;
                    if within < VS {
                        return Some(if edge { s.hull } else { mat::WOOD });
                    }
                    return edge.then_some(s.hull);
                }
                return None;
            }
            // Floor slabs: level i's floor covers level i and (as a terrace) level i-1.
            if within < VS && lvl >= 1 && (in_level(lvl, a, x) || in_level(lvl - 1, a, x)) {
                if !in_level(lvl, a, x) {
                    // Terrace: rail on the outer edge of the level below.
                    return Some(mat::WOOD);
                }
                return Some(mat::WOOD);
            }
            if lvl >= 1 && (VS..1.0).contains(&within) && in_level(lvl - 1, a, x) && !in_level(lvl, a, x) {
                let (a0, a1) = spans[(lvl - 1) as usize];
                let edge = x.abs() > hhw - VS || a < a0 + VS || a >= a1 - VS;
                return edge.then_some(s.hull);
            }
            if in_level(lvl, a, x) {
                let (a0, a1) = spans[lvl as usize];
                let wall = x.abs() > hhw - VS || a < a0 + VS || a >= a1 - VS;
                if !wall {
                    return None;
                }
                let front = a >= a1 - VS;
                if front && lvl == s.levels - 1 && (1.0..3.0).contains(&within) {
                    return Some(mat::GLASS_TEAL); // bridge windows
                }
                if (1.0..3.0).contains(&within) {
                    let divider = if front || a < a0 + VS { x.rem_euclid(3.0) < VS } else { a.rem_euclid(3.0) < VS };
                    if !divider {
                        return Some(mat::GLASS_BLUE);
                    }
                }
                // Doors onto the promenades / terraces.
                if !front && a >= a0 + VS && within < 3.0 && ((a - (a0 + a1) * 0.5).abs() < 1.0) {
                    return None;
                }
                return Some(s.hull);
            }
            // Lifeboats hanging over the promenades (overhead cover, low anchors).
            if lvl == 0 && (2.5..3.5).contains(&within) && x.abs() >= hhw && x.abs() < hhw + 1.5 {
                let (a0, a1) = spans[0];
                if a > a0 + 2.0 && a < a1 - 2.0 && (a - a0).rem_euclid(7.0) < 5.0 {
                    return Some(if within >= 3.0 { s.hull } else { mat::PAINT_YELLOW });
                }
            }
            None
        });
        // The closed hull holds (below / above the inner deck).
        for y in [s.keel_y + 1.5, dy - 2.25] {
            let seed = s.world(0.0, y, 0.0);
            let (a, b) = (s.world(-b2 - 1.0, s.keel_y - 1.0, -l2 - 2.0), s.world(b2 + 1.0, dy + 1.0, l2 + rake + 2.0));
            self.interior(seed, a.min(b), a.max(b));
        }
        // Pool on the sun deck (flush), deck chairs row.
        let (pa0, pa1) = pool_a;
        let p0 = s.world(-hhw + 2.5, top - VS, pa0);
        let p1 = s.world(hhw - 2.5, top, pa1);
        self.fill(p0.min(p1), p0.max(p1), mat::POOL_WATER);
        // Funnel: raked ellipse, red with a navy cap, whale-tail fins.
        let fa = s.funnel_a();
        let fh = 11.0;
        let flo = s.world(-5.0, top, fa - 9.0);
        let fhi = s.world(5.0, top + fh + 1.0, fa + 7.0);
        let stripe = s.stripe;
        let funnel = s.funnel;
        let hull = s.hull;
        self.fill_with(flo.min(fhi), flo.max(fhi), |p, _, _| {
            let up = p.y - top;
            let x = p.x - s.center.x;
            let a = (p.z - s.center.y) * s.bow - (fa - up * 0.3);
            let k = 1.0 - 0.15 * (up / fh);
            let e = (x / (3.0 * k)) * (x / (3.0 * k)) + (a / (5.0 * k)) * (a / (5.0 * k));
            if up < fh && e <= 1.0 {
                if up > fh - 1.5 {
                    return Some(stripe);
                }
                if (up - 5.0).abs() < 0.5 {
                    return Some(hull);
                }
                return Some(funnel);
            }
            // Whale-tail fins (sideways wings near the top).
            if (fh - 3.5..fh - 2.5).contains(&up) && x.abs() < 6.5 && (-3.5..0.5).contains(&a) {
                return Some(funnel);
            }
            None
        });
        // Mast over the bridge (radar crossbar, amber tip).
        let mb = s.world(0.0, top, bridge_a - 3.0);
        self.fill(Vec3::new(mb.x - 0.25, top, mb.z - 0.25), Vec3::new(mb.x + 0.25, top + 8.5, mb.z + 0.25), mat::STEEL);
        self.fill(Vec3::new(mb.x - 2.5, top + 6.0, mb.z - 0.25), Vec3::new(mb.x + 2.5, top + 6.5, mb.z + 0.25), mat::STEEL);
        self.fill(Vec3::new(mb.x - 0.25, top + 8.5, mb.z - 0.25), Vec3::new(mb.x + 0.25, top + 9.5, mb.z + 0.25), mat::NEON_AMBER);
        // Anchors along the keel.
        let mut a = -l2 + 4.0;
        while a < l2 - 4.0 {
            let w = s.world(0.0, 0.0, a);
            self.anchor(IVec3::new(vx(w.x), 0, vx(w.z)));
            a += 16.0;
        }
        let _ = Vec2::ZERO;
    }
}
