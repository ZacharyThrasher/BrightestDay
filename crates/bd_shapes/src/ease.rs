//! Easing curves + a tiny scalar keyframe track. Everything here is pure `f32` math.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ease {
    #[default]
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    /// Accelerating: wind-up release, heavy swings.
    InCubic,
    /// Decelerating: coiling into an anticipation hold, settling.
    OutCubic,
    InOutCubic,
    InOutSine,
    /// Overshoots ~10% then settles (follow-through).
    OutBack,
    /// Pulls back ~10% before going (anticipation).
    InBack,
    /// Springy settle.
    OutElastic,
    /// Holds the start value, then jumps (impact frames).
    Step,
}

impl Ease {
    pub fn apply(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Ease::Linear => u,
            Ease::InQuad => u * u,
            Ease::OutQuad => 1.0 - (1.0 - u) * (1.0 - u),
            Ease::InOutQuad => {
                if u < 0.5 { 2.0 * u * u } else { 1.0 - (-2.0 * u + 2.0).powi(2) * 0.5 }
            }
            Ease::InCubic => u * u * u,
            Ease::OutCubic => 1.0 - (1.0 - u).powi(3),
            Ease::InOutCubic => {
                if u < 0.5 { 4.0 * u * u * u } else { 1.0 - (-2.0 * u + 2.0).powi(3) * 0.5 }
            }
            Ease::InOutSine => 0.5 - 0.5 * (std::f32::consts::PI * u).cos(),
            Ease::OutBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (u - 1.0).powi(3) + c1 * (u - 1.0).powi(2)
            }
            Ease::InBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * u * u * u - c1 * u * u
            }
            Ease::OutElastic => {
                if u <= 0.0 {
                    0.0
                } else if u >= 1.0 {
                    1.0
                } else {
                    let c4 = std::f32::consts::TAU / 3.0;
                    2f32.powf(-10.0 * u) * ((u * 10.0 - 0.75) * c4).sin() + 1.0
                }
            }
            Ease::Step => {
                if u >= 1.0 { 1.0 } else { 0.0 }
            }
        }
    }
}

/// Keyframe: (time, value, ease used to arrive at this key from the previous one).
#[derive(Clone, Copy, Debug)]
pub struct K(pub f32, pub f32, pub Ease);

/// Sample a scalar track (clamped at both ends).
pub fn track(t: f32, keys: &[K]) -> f32 {
    if keys.is_empty() {
        return 0.0;
    }
    if t <= keys[0].0 {
        return keys[0].1;
    }
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t <= b.0 {
            let span = (b.0 - a.0).max(1e-6);
            let e = b.2.apply((t - a.0) / span);
            return a.1 + (b.1 - a.1) * e;
        }
    }
    keys[keys.len() - 1].1
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Damped spring response to a unit impulse at t=0 (peaks ≈1 quickly, rings, settles to 0).
pub fn spring_impulse(t: f32, freq_hz: f32, damping: f32) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    let w = std::f32::consts::TAU * freq_hz;
    (-damping * t).exp() * (w * t).sin()
}
