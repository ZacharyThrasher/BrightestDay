//! WILLPOWER = LAUNCH readability (host rule `MatchRules::willpower_launch`, `bd_sim::launch`): every
//! player's hard-light body glows with their LAUNCH POWER, so the whole lobby sees who is "loaded".
//! Below the ×1 point nothing changes (a drained player just looks normal — the % colours stay the
//! read); above it the body brightens (bloom) and gets a faint white sheen, and a nearly full bar
//! breathes. Reads only replicated willpower (`PlayerSnap::willpower`) + the replicated rules and
//! tuning, so host and clients agree. No-op while the rule is off. Comfort (`flashes` = 0): no
//! breathing, the steady glow stays.

use super::PresentSet;
use super::players::PlayerBody;
use crate::settings::ComfortSettings;
use crate::sim_bridge::ActiveWorld;
use crate::tick::FrameSet;
use bd_fx::ConstructVisual;
use bevy::prelude::*;

/// Emissive boost at full launch power (× the body palette; the ×1 point is 1.0).
const BOOST_MAX: f32 = 0.7;
/// White sheen (`ConstructVisual::flash`) at full launch power.
const SHEEN_MAX: f32 = 0.06;

pub struct LaunchGlowPlugin;

impl Plugin for LaunchGlowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, loaded_glow.in_set(FrameSet::Present).after(PresentSet::Sync));
    }
}

/// How loaded a launch power is: 0 at / below the ×1 notch → 1 at a full bar.
pub fn loaded(power: f32, mid: f32) -> f32 {
    if !power.is_finite() || mid >= 1.0 { 0.0 } else { ((power - mid) / (1.0 - mid)).clamp(0.0, 1.0) }
}

fn loaded_glow(active: Option<Res<ActiveWorld>>, comfort: Res<ComfortSettings>, time: Res<Time<Real>>, mut bodies: Query<(&PlayerBody, &mut ConstructVisual)>) {
    let Some(a) = active else { return };
    let w = &a.world;
    if !bd_sim::launch::active(w.rules()) {
        return;
    }
    let t = &w.cfg.tuning.launch;
    let mid = bd_sim::launch::mid_power01(t);
    let calm = comfort.flashes <= 0.01;
    let now = time.elapsed_secs();
    for (b, mut cv) in &mut bodies {
        let Some(p) = w.player(b.id).filter(|p| p.alive) else { continue };
        let k = loaded(bd_sim::launch::power01(t, w.willpower_frac(b.id)), mid);
        if k < 0.01 {
            continue;
        }
        let breathe = if calm || k < 0.85 { 0.0 } else { 0.12 * (now * 5.0 + p.id.0 as f32).sin() };
        let boost = 1.0 + BOOST_MAX * k * (1.0 + breathe);
        let c = cv.corps_color;
        cv.corps_color = [c[0] * boost, c[1] * boost, c[2] * boost];
        if let Some(ac) = cv.accent {
            cv.accent = Some([ac[0] * boost, ac[1] * boost, ac[2] * boost]);
        }
        cv.flash = cv.flash.max(SHEEN_MAX * k);
    }
}

#[cfg(test)]
mod tests {
    use super::loaded;

    #[test]
    fn only_above_the_notch() {
        assert_eq!(loaded(0.3, 0.56), 0.0);
        assert_eq!(loaded(1.0, 0.56), 1.0);
        assert!(loaded(0.78, 0.56) > 0.4 && loaded(0.78, 0.56) < 0.6);
        assert_eq!(loaded(f32::NAN, 0.5), 0.0);
    }
}
