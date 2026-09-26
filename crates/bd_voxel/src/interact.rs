//! Map **interactables**: optional, flavour-only toys a map can declare in
//! [`ArenaInfo::interactables`](crate::ArenaInfo::interactables). The sim runs them
//! (`bd_sim::world::interactables`: host-authoritative, deterministic); the client draws them from
//! the same generated data. Everything here is plain geometry — no rules, odds or timings (those
//! live in the sim's tuning, `assets/tuning/combat.ron` → `casino`).
//!
//! Owner map rule: flavour must *enhance* what players already do, never force them to play around
//! it. Nothing here blocks a route, and ignoring every interactable is always a valid way to play.

use glam::Vec3;

/// Everything interactive a map declares (empty on every map but High Roller).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Interactables {
    /// Big "high roller" slot machines: pull the lever by hitting it or yanking it with a tether.
    pub slots: Vec<SlotMachine>,
    /// A roulette wheel whose lit pocket region ("house spin") pays a small bonus.
    pub roulette: Option<RouletteWheel>,
}

impl Interactables {
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty() && self.roulette.is_none()
    }
}

/// One slot machine (the cabinet itself is indestructible terrain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotMachine {
    /// Centre of the lever's knob (the pull trigger).
    pub lever: Vec3,
    /// Trigger radius around the knob (m): hits, blasts and tether hooks within it pull.
    pub radius: f32,
    /// Centre of the reel window (where the client draws the spinning reels).
    pub reels: Vec3,
    /// Unit horizontal vector out of the machine's face (towards the player pulling it).
    pub facing: Vec3,
    /// Reel window half-extents (m): along the face's right vector and up.
    pub window: [f32; 2],
}

/// A (giant) roulette wheel: `pockets` equal wedges of the annulus `r_inner..r_outer` around
/// `center`, pocket `k` spanning angles `zero_angle + k·τ/pockets .. zero_angle + (k+1)·τ/pockets`
/// (radians, measured like `atan2(z, x)`, counter-clockwise seen from above). `colors[k]`: 0 green,
/// 1 red, 2 black (the classic sequence).
#[derive(Clone, Debug, PartialEq)]
pub struct RouletteWheel {
    pub center: Vec3,
    pub r_inner: f32,
    pub r_outer: f32,
    /// Standing in a pocket = feet within this height band (m, absolute).
    pub y_min: f32,
    pub y_max: f32,
    pub zero_angle: f32,
    /// Pocket labels in wheel order ("0", "28", "9", … "00").
    pub labels: Vec<&'static str>,
    pub colors: Vec<u8>,
}

impl RouletteWheel {
    pub fn pockets(&self) -> usize {
        self.labels.len()
    }

    /// Angle (radians) of the middle of pocket `k`.
    pub fn pocket_angle(&self, k: usize) -> f32 {
        let n = self.pockets().max(1) as f32;
        self.zero_angle + (k as f32 + 0.5) * std::f32::consts::TAU / n
    }

    /// Which pocket (if any) contains the horizontal position of `p` (height not checked).
    pub fn pocket_at(&self, p: Vec3) -> Option<usize> {
        let d = Vec3::new(p.x - self.center.x, 0.0, p.z - self.center.z);
        let r = d.length();
        if r < self.r_inner || r > self.r_outer || self.pockets() == 0 {
            return None;
        }
        let tau = std::f32::consts::TAU;
        let a = (crate::noise::datan2(d.z, d.x) - self.zero_angle).rem_euclid(tau);
        let k = ((a / tau) * self.pockets() as f32).floor() as usize;
        Some(k.min(self.pockets() - 1))
    }

    /// Is `p` (feet) standing in the region of `half_width` pockets either side of pocket `k`?
    pub fn in_region(&self, p: Vec3, k: usize, half_width: usize) -> bool {
        if p.y < self.y_min || p.y > self.y_max {
            return false;
        }
        let n = self.pockets();
        let Some(at) = self.pocket_at(p) else { return false };
        let d = (at + n - k % n.max(1)) % n.max(1);
        d.min(n - d) <= half_width
    }
}

/// American wheel order (0 … 00) and colours — the classic pattern players recognise.
pub const AMERICAN_WHEEL: [(&str, u8); 38] = [
    ("0", 0),
    ("28", 2),
    ("9", 1),
    ("26", 2),
    ("30", 1),
    ("11", 2),
    ("7", 1),
    ("20", 2),
    ("32", 1),
    ("17", 2),
    ("5", 1),
    ("22", 2),
    ("34", 1),
    ("15", 2),
    ("3", 1),
    ("24", 2),
    ("36", 1),
    ("13", 2),
    ("1", 1),
    ("00", 0),
    ("27", 1),
    ("10", 2),
    ("25", 1),
    ("29", 2),
    ("12", 1),
    ("8", 2),
    ("19", 1),
    ("31", 2),
    ("18", 1),
    ("6", 2),
    ("21", 1),
    ("33", 2),
    ("16", 1),
    ("4", 2),
    ("23", 1),
    ("35", 2),
    ("14", 1),
    ("2", 2),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel() -> RouletteWheel {
        RouletteWheel {
            center: Vec3::new(0.0, 38.0, 0.0),
            r_inner: 10.0,
            r_outer: 20.0,
            y_min: 37.0,
            y_max: 40.0,
            zero_angle: 0.0,
            labels: AMERICAN_WHEEL.iter().map(|p| p.0).collect(),
            colors: AMERICAN_WHEEL.iter().map(|p| p.1).collect(),
        }
    }

    #[test]
    fn pockets_cover_the_ring() {
        let w = wheel();
        for k in 0..w.pockets() {
            let a = w.pocket_angle(k);
            let p = Vec3::new(15.0 * a.cos(), 38.0, 15.0 * a.sin());
            assert_eq!(w.pocket_at(p), Some(k));
            assert!(w.in_region(p, k, 0));
        }
        assert_eq!(w.pocket_at(Vec3::new(5.0, 38.0, 0.0)), None);
        assert_eq!(w.pocket_at(Vec3::new(25.0, 38.0, 0.0)), None);
    }

    #[test]
    fn regions_wrap_and_respect_height() {
        let w = wheel();
        let a = w.pocket_angle(37);
        let p = Vec3::new(15.0 * a.cos(), 38.0, 15.0 * a.sin());
        assert!(w.in_region(p, 0, 1), "region wraps past 00 → 0");
        assert!(!w.in_region(p, 5, 2));
        assert!(!w.in_region(p + Vec3::Y * 5.0, 37, 0), "above the pockets");
        let reds = AMERICAN_WHEEL.iter().filter(|p| p.1 == 1).count();
        assert_eq!(reds, 18);
    }
}
