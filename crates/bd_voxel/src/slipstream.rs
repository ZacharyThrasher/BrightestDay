//! **SLIPSTREAM** — the one Hyperline map rule, as data (DESIGN.md "Maps: Hyperline").
//!
//! "The train stands still, the world moves": the voxel train never moves, the client scrolls an
//! endless city past it, and this wind makes the illusion physical. It only ever *adds* to what
//! players already do (knockback carries further; nothing is taken away):
//!
//! * standing on / touching the train (grounded, pressed to a wall, riding a surf face): nothing;
//! * inside the train's footprint *below* the roofs (in a car, in a gap, under the train): sheltered;
//! * airborne inside the footprint *above* the roofs: a gentle rearward drag ([`Slipstream::drag`]);
//! * outside the footprint (off the sides, behind the tail, ahead of the nose): a rearward +
//!   outward sweep that ramps from the drag at the footprint edge to full strength
//!   [`Slipstream::ramp`] metres out — a knock-off becomes a trip past the tail into the blast zones.
//!
//! The push fades out as the body's speed along it approaches a terminal airspeed (drag-like, no
//! runaway speeds). A body hanging on an attached tether feels it scaled by
//! [`Slipstream::tether_scale`], so reeling in wins (the tether save is the lifeline).
//!
//! The train runs along **+X** (the nose is at `max.x`); the wind blows toward −X. Everything here
//! is `+ - * /`, `min`/`max`: host and predicting clients agree bit-for-bit. The sim applies it to
//! players in `bd_sim::slipstream`; the client reads [`Slipstream::zone`] for wind effects.

use glam::{Vec2, Vec3};

/// The Hyperline wind (`ArenaInfo::slipstream`; `None` on every other map).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slipstream {
    /// Train footprint on the ground plane: `min = (tail x, min z)`, `max = (nose x, max z)` (m).
    pub min: Vec2,
    pub max: Vec2,
    /// Roof height (m): inside the footprint, airborne at or above this = the drag; below = sheltered.
    pub roof_y: f32,
    /// Rearward push (m/s²) airborne above the roofs (and at the footprint edge outside).
    pub drag: f32,
    /// Full-strength sweep outside the footprint: rearward (m/s²) and outward, away from the train's
    /// centre line (m/s²).
    pub sweep_back: f32,
    pub sweep_out: f32,
    /// Distance outside the footprint (m) over which the sweep ramps up (smoothstep).
    pub ramp: f32,
    /// Speeds (m/s) along the push at which it has faded to nothing (terminal airspeeds).
    pub terminal_back: f32,
    pub terminal_out: f32,
    /// Multiplier while a tether is attached.
    pub tether_scale: f32,
}

/// Where a point is relative to the train's wind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlipZone {
    /// In the footprint below the roofs (inside a car, in a gap, under the train).
    Sheltered,
    /// In the footprint above the roofs: the gentle drag.
    Roof,
    /// Outside the footprint; the ramp factor 0..1 (0 at the edge, 1 at `ramp` metres out).
    Sweep(f32),
}

#[inline]
fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 1 when not moving along the push, fading to 0 at `terminal` m/s along it (never above 1).
#[inline]
fn fade(speed_along: f32, terminal: f32) -> f32 {
    if terminal <= 0.0 {
        return 1.0;
    }
    (1.0 - speed_along / terminal).clamp(0.0, 1.0)
}

impl Slipstream {
    /// How far outside the footprint `p` is (m, Chebyshev: the larger of the x / z overhangs).
    #[inline]
    pub fn outside(&self, p: Vec3) -> f32 {
        let dx = (self.min.x - p.x).max(p.x - self.max.x).max(0.0);
        let dz = (self.min.y - p.z).max(p.z - self.max.y).max(0.0);
        dx.max(dz)
    }

    pub fn zone(&self, p: Vec3) -> SlipZone {
        let d = self.outside(p);
        if d > 0.0 {
            SlipZone::Sweep(smoothstep01(d / self.ramp.max(1e-3)))
        } else if p.y >= self.roof_y {
            SlipZone::Roof
        } else {
            SlipZone::Sheltered
        }
    }

    /// Acceleration (m/s²) on a free body at `pos` moving at `vel`, before the touching / tether
    /// rules (`bd_sim::slipstream` applies those).
    pub fn accel(&self, pos: Vec3, vel: Vec3) -> Vec3 {
        let (back, out) = match self.zone(pos) {
            SlipZone::Sheltered => return Vec3::ZERO,
            SlipZone::Roof => (self.drag, 0.0),
            SlipZone::Sweep(k) => (self.drag + (self.sweep_back - self.drag) * k, self.sweep_out * k),
        };
        // Rearward = −X.
        let a_back = back * fade(-vel.x, self.terminal_back);
        // Outward: proportional to the offset from the centre line (±1 at the sides, 0 dead astern).
        let cz = (self.min.y + self.max.y) * 0.5;
        let hz = ((self.max.y - self.min.y) * 0.5).max(1e-3);
        let side = ((pos.z - cz) / hz).clamp(-1.0, 1.0);
        let a_out = if side == 0.0 || out == 0.0 {
            0.0
        } else {
            let s = if side > 0.0 { 1.0 } else { -1.0 };
            out * side * fade(vel.z * s, self.terminal_out)
        };
        Vec3::new(-a_back, 0.0, a_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Slipstream {
        Slipstream {
            min: Vec2::new(-100.0, -4.5),
            max: Vec2::new(100.0, 4.5),
            roof_y: 13.0,
            drag: 1.5,
            sweep_back: 24.0,
            sweep_out: 14.0,
            ramp: 5.0,
            terminal_back: 60.0,
            terminal_out: 30.0,
            tether_scale: 0.35,
        }
    }

    #[test]
    fn zones_and_pushes() {
        let s = s();
        assert_eq!(s.accel(Vec3::new(0.0, 10.0, 0.0), Vec3::ZERO), Vec3::ZERO, "sheltered in a car");
        let roof = s.accel(Vec3::new(0.0, 14.0, 0.0), Vec3::ZERO);
        assert_eq!(roof, Vec3::new(-1.5, 0.0, 0.0));
        let far = s.accel(Vec3::new(0.0, 14.0, 12.0), Vec3::ZERO);
        assert_eq!(far, Vec3::new(-24.0, 0.0, 14.0));
        let left = s.accel(Vec3::new(0.0, 14.0, -12.0), Vec3::ZERO);
        assert_eq!(left.z, -14.0);
        let astern = s.accel(Vec3::new(-110.0, 14.0, 0.0), Vec3::ZERO);
        assert_eq!(astern, Vec3::new(-24.0, 0.0, 0.0));
        // Continuous at the edge: just outside ≈ the drag.
        let edge = s.accel(Vec3::new(0.0, 14.0, 4.51), Vec3::ZERO);
        assert!((edge.x + 1.5).abs() < 0.1 && edge.z.abs() < 0.1, "{edge:?}");
        // Terminal airspeed.
        assert_eq!(s.accel(Vec3::new(0.0, 14.0, 12.0), Vec3::new(-60.0, 0.0, 30.0)), Vec3::ZERO);
    }
}
