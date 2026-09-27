//! `assets/tuning/presence.ron` — every presence knob, per tier and per element. Hot-reloaded
//! (mtime poll, like juice.ron); a parse error keeps the previous values.

use super::tier::{Tier, TierKnobs};
use bd_core::mechanics::Element;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// How strongly one tier plays each beat. Index = [`Tier::index`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TierFeel {
    // ---- GATHER (overlaps the forming windup; never adds latency) ----
    /// Motes drawn in per second at the peak of the gather (ramps up k²).
    pub gather_rate: f32,
    /// Shell the motes start from (m), × √size (≤ ×2): a few metres for a sword, across the map for
    /// a Signature (the far ones are sampled from the surroundings: leaves, droplets, embers, grit, sky).
    pub gather_radius: f32,
    /// Travel time of the farthest gather motes (s; capped by the window).
    pub gather_life: f32,
    /// Seconds before the manifest the gather starts (clamped to the forming time).
    pub gather_secs: f32,
    /// Gather sound ("" = none; the assembly's own construct_form covers small casts).
    pub gather_sfx: String,
    pub gather_gain: f32,
    // ---- MANIFEST ----
    /// Element-coloured emissive surge on the construct (0..1) and how fast it decays (s).
    pub surge: f32,
    pub surge_secs: f32,
    /// Shockwave ring particles (0 = none) and their outward speed (m/s, × sqrt(size)).
    pub ring: u32,
    pub ring_speed: f32,
    /// Element burst scale (bd_fx BurstFx; 0 = none).
    pub burst: f32,
    pub manifest_sfx: String,
    pub manifest_gain: f32,
    /// Caster only: FOV punch (degrees) and camera trauma (comfort applies in juice).
    pub fov_punch: f32,
    pub caster_shake: f32,
    /// A brief flash light at the manifest (lumens; 0 = none) and its range (m).
    pub flash_light: f32,
    pub flash_range: f32,
    // ---- MATERIAL ----
    /// Element surface strength (0..1.5) at full LOD.
    pub material: f32,
    // ---- PRESENCE ----
    /// Idle element motes per second around the construct (0 = none).
    pub idle_rate: f32,
    /// A standing light in the element colour: intensity (lumens; 0 = none) and range (m).
    pub light: f32,
    pub light_range: f32,
    /// Idle hum gain (linear; 0 = none). Pitch comes from size.
    pub hum_gain: f32,
    /// OTHER players feel it within this radius (m): manifest shake and, when it moves, a rumble.
    pub feel_radius: f32,
    /// Peak trauma for a bystander at the centre (falls off linearly to `feel_radius`).
    pub feel_trauma: f32,
    /// Continuous travel rumble (trauma per second at the centre) while it moves; 0 = none.
    pub travel_rumble: f32,
    // ---- IMPACT / EXIT ----
    /// Element motes a construct dissolves into when it ends.
    pub exit_motes: u32,
    /// Multiplier on hit juice from this construct's hits (read by impact via [`super::PresenceIndex`]).
    pub impact_mult: f32,
    /// Priority of this tier's voices / lights / motes when the pools are full (higher wins).
    pub priority: f32,
}

impl Default for TierFeel {
    fn default() -> Self {
        Self::normal()
    }
}

impl TierFeel {
    fn tiny() -> Self {
        TierFeel {
            gather_rate: 60.0,
            gather_radius: 1.2,
            gather_life: 0.12,
            gather_secs: 0.10,
            gather_sfx: String::new(),
            gather_gain: 0.0,
            surge: 0.35,
            surge_secs: 0.08,
            ring: 0,
            ring_speed: 0.0,
            burst: 0.0,
            manifest_sfx: String::new(),
            manifest_gain: 0.0,
            fov_punch: 0.0,
            caster_shake: 0.0,
            flash_light: 0.0,
            flash_range: 0.0,
            material: 0.35,
            idle_rate: 0.0,
            light: 0.0,
            light_range: 0.0,
            hum_gain: 0.0,
            feel_radius: 0.0,
            feel_trauma: 0.0,
            travel_rumble: 0.0,
            exit_motes: 6,
            impact_mult: 1.0,
            priority: 1.0,
        }
    }
    fn normal() -> Self {
        TierFeel {
            gather_rate: 110.0,
            gather_radius: 2.5,
            gather_life: 0.2,
            gather_secs: 0.15,
            surge: 0.6,
            surge_secs: 0.12,
            ring: 14,
            ring_speed: 6.0,
            burst: 0.0,
            manifest_sfx: "presence_shing".into(),
            manifest_gain: 0.8,
            fov_punch: 0.0,
            material: 0.55,
            idle_rate: 1.5,
            hum_gain: 0.35,
            exit_motes: 14,
            priority: 2.0,
            ..Self::tiny()
        }
    }
    fn big() -> Self {
        TierFeel {
            gather_rate: 220.0,
            gather_radius: 8.0,
            gather_life: 0.45,
            gather_secs: 0.45,
            gather_sfx: "presence_gather_big".into(),
            gather_gain: 0.9,
            surge: 0.9,
            surge_secs: 0.2,
            ring: 32,
            ring_speed: 12.0,
            burst: 0.9,
            manifest_sfx: "presence_manifest_big".into(),
            manifest_gain: 1.0,
            fov_punch: 1.5,
            caster_shake: 0.08,
            flash_light: 400_000.0,
            flash_range: 14.0,
            material: 0.8,
            idle_rate: 6.0,
            light: 60_000.0,
            light_range: 9.0,
            hum_gain: 0.6,
            feel_radius: 25.0,
            feel_trauma: 0.1,
            travel_rumble: 0.0,
            exit_motes: 36,
            impact_mult: 1.15,
            priority: 4.0,
            ..Self::normal()
        }
    }
    fn titan() -> Self {
        TierFeel {
            gather_rate: 420.0,
            gather_radius: 22.0,
            gather_life: 0.9,
            gather_secs: 1.0,
            gather_sfx: "presence_gather_titan".into(),
            gather_gain: 1.0,
            surge: 1.0,
            surge_secs: 0.3,
            ring: 64,
            ring_speed: 22.0,
            burst: 1.6,
            manifest_sfx: "presence_manifest_titan".into(),
            manifest_gain: 1.0,
            fov_punch: 3.5,
            caster_shake: 0.18,
            flash_light: 1_500_000.0,
            flash_range: 30.0,
            material: 1.0,
            idle_rate: 16.0,
            light: 200_000.0,
            light_range: 18.0,
            hum_gain: 0.9,
            feel_radius: 55.0,
            feel_trauma: 0.28,
            travel_rumble: 0.35,
            exit_motes: 80,
            impact_mult: 1.3,
            priority: 8.0,
            ..Self::big()
        }
    }
    fn signature() -> Self {
        TierFeel {
            gather_rate: 520.0,
            gather_radius: 32.0,
            gather_life: 1.1,
            gather_secs: 1.2,
            ring: 80,
            ring_speed: 26.0,
            burst: 2.0,
            fov_punch: 4.0,
            caster_shake: 0.2,
            material: 1.15,
            idle_rate: 20.0,
            light: 260_000.0,
            light_range: 22.0,
            hum_gain: 1.0,
            feel_radius: 70.0,
            feel_trauma: 0.32,
            travel_rumble: 0.45,
            exit_motes: 100,
            impact_mult: 1.4,
            priority: 10.0,
            ..Self::titan()
        }
    }
}

/// Per-element knobs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ElementFeel {
    /// Mote colour (linear rgb); `None` = the element's grade colour (Light: the caster's corps).
    pub color: Option<[f32; 3]>,
    /// × mote rates (gather / idle / exit).
    pub motes: f32,
    /// × material strength.
    pub material: f32,
    /// Hum loop cue.
    pub hum: String,
    /// × the standing light's intensity.
    pub light: f32,
}

impl Default for ElementFeel {
    fn default() -> Self {
        ElementFeel { color: None, motes: 1.0, material: 1.0, hum: "presence_hum_light".into(), light: 1.0 }
    }
}

/// Global caps (8 players × many constructs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Budget {
    /// Standing presence lights (pooled, shadowless) + manifest flash lights.
    pub max_lights: usize,
    pub max_flash_lights: usize,
    /// Hum voices at once (highest tier × nearest win; the rest fade out).
    pub max_hums: usize,
    /// Presence particle spawns per frame across every construct (gather + idle + motion).
    pub max_motes_per_frame: u32,
    /// Material / idle-mote LOD: full strength within `lod_full` m of the camera, gone at `lod_zero`
    /// (× sqrt(size), so a kaiju keeps its look from across the map).
    pub lod_full: f32,
    pub lod_zero: f32,
    /// Hums are audible within this (m, × sqrt(size)).
    pub hum_range: f32,
    /// Surroundings sampling for wide gathers: downward rays per frame (all gathers) and samples
    /// kept per gather.
    pub max_rays_per_frame: u32,
    pub surround_samples: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Budget { max_lights: 4, max_flash_lights: 2, max_hums: 6, max_motes_per_frame: 700, lod_full: 22.0, lod_zero: 70.0, hum_range: 40.0, max_rays_per_frame: 24, surround_samples: 32 }
    }
}

/// Mechanic motion signatures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionFeel {
    /// Heavy: a landing faster than this (m/s down) cracks the ground (dust ring + grit).
    pub heavy_land_speed: f32,
    /// Fast: speed streaks above this (m/s).
    pub fast_speed: f32,
    pub fast_rate: f32,
    /// Flies: downdraft dust when lower than this above the ground (m, × size).
    pub downdraft_height: f32,
    pub downdraft_rate: f32,
    /// Spin: air-distortion orbit streaks per second.
    pub spin_rate: f32,
    /// Explodes: the unstable pulse — pulses per second at spawn → at the end of its life, and the
    /// surge peak.
    pub unstable_hz: [f32; 2],
    pub unstable_surge: f32,
}

impl Default for MotionFeel {
    fn default() -> Self {
        MotionFeel {
            heavy_land_speed: 6.0,
            fast_speed: 12.0,
            fast_rate: 40.0,
            downdraft_height: 5.0,
            downdraft_rate: 24.0,
            spin_rate: 30.0,
            unstable_hz: [1.5, 9.0],
            unstable_surge: 0.6,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresenceTuning {
    /// Master switch (false = the pre-presence look).
    pub enabled: bool,
    pub tier: TierKnobs,
    /// [Tiny, Normal, Big, Titan, Signature].
    pub tiers: [TierFeel; 5],
    pub elements: Vec<(Element, ElementFeel)>,
    pub motion: MotionFeel,
    /// Continuous gathers of hold-to-grow energy objects ([`super::gather::GatherFx`]).
    pub hold: super::gather::HoldGather,
    pub budget: Budget,
}

impl Default for PresenceTuning {
    fn default() -> Self {
        let el = |e: Element, hum: &str, motes: f32, material: f32, light: f32| (e, ElementFeel { color: None, motes, material, hum: hum.into(), light });
        PresenceTuning {
            enabled: true,
            tier: TierKnobs::default(),
            tiers: [TierFeel::tiny(), TierFeel::normal(), TierFeel::big(), TierFeel::titan(), TierFeel::signature()],
            elements: vec![
                el(Element::Light, "presence_hum_light", 1.0, 0.8, 1.0),
                el(Element::Fire, "presence_hum_fire", 1.2, 1.0, 1.3),
                el(Element::Electric, "presence_hum_electric", 1.0, 1.0, 1.2),
                el(Element::Ice, "presence_hum_ice", 0.9, 1.0, 0.8),
                el(Element::Water, "presence_hum_water", 1.0, 1.0, 0.8),
                el(Element::Wind, "presence_hum_wind", 1.3, 0.9, 0.6),
                el(Element::Earth, "presence_hum_earth", 1.0, 1.0, 0.7),
                el(Element::Poison, "presence_hum_poison", 1.0, 1.0, 0.8),
                el(Element::Psychic, "presence_hum_psychic", 1.0, 1.0, 1.0),
                el(Element::Shadow, "presence_hum_shadow", 0.9, 1.0, 0.5),
                el(Element::Atomic, "presence_hum_atomic", 1.0, 1.0, 1.3),
            ],
            motion: MotionFeel::default(),
            hold: super::gather::HoldGather::default(),
            budget: Budget::default(),
        }
    }
}

impl PresenceTuning {
    pub fn feel(&self, t: Tier) -> &TierFeel {
        &self.tiers[t.index().min(self.tiers.len() - 1)]
    }
    pub fn element(&self, e: Element) -> ElementFeel {
        self.elements.iter().find(|(k, _)| *k == e).map(|(_, f)| f.clone()).unwrap_or_default()
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct PresenceTuningRes(pub PresenceTuning);

fn tuning_path() -> PathBuf {
    crate::assets_dir().join("tuning").join("presence.ron")
}

pub fn load_tuning() -> PresenceTuning {
    match std::fs::read_to_string(tuning_path()) {
        Ok(text) => ron::from_str::<PresenceTuning>(&text).unwrap_or_else(|e| {
            eprintln!("presence tuning: failed to parse {}: {e}; using defaults", tuning_path().display());
            PresenceTuning::default()
        }),
        Err(_) => PresenceTuning::default(),
    }
}

pub fn hot_reload(time: Res<Time<Real>>, mut tuning: ResMut<PresenceTuningRes>, mut last: Local<(f32, Option<SystemTime>)>) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let Ok(modified) = std::fs::metadata(tuning_path()).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(modified),
        Some(prev) if prev != modified => {
            last.1 = Some(modified);
            if let Ok(text) = std::fs::read_to_string(tuning_path()) {
                match ron::from_str::<PresenceTuning>(&text) {
                    Ok(t) => {
                        info!("presence tuning reloaded");
                        tuning.0 = t;
                    }
                    Err(e) => warn!("presence tuning: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_file_parses_and_matches_defaults() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tuning/presence.ron");
        let text = std::fs::read_to_string(&path).expect("assets/tuning/presence.ron");
        let t: PresenceTuning = ron::from_str(&text).expect("parse");
        assert_eq!(t, PresenceTuning::default(), "presence.ron drifted from the Rust defaults (regenerate it)");
    }

    #[test]
    fn tiers_escalate() {
        let t = PresenceTuning::default();
        for w in t.tiers.windows(2) {
            assert!(w[1].gather_rate >= w[0].gather_rate);
            assert!(w[1].material >= w[0].material);
            assert!(w[1].feel_radius >= w[0].feel_radius);
            assert!(w[1].priority > w[0].priority);
        }
        // Contrast: small casts add nothing to the audio beyond a shing.
        assert!(t.tiers[0].manifest_sfx.is_empty() && t.tiers[0].hum_gain == 0.0);
    }

    /// `cargo test -p bd_game presence_ron_dump -- --ignored --nocapture` prints the defaults.
    #[test]
    #[ignore]
    fn presence_ron_dump() {
        let cfg = ron::ser::PrettyConfig::new().depth_limit(4).struct_names(false);
        println!("{}", ron::ser::to_string_pretty(&PresenceTuning::default(), cfg).unwrap());
    }
}
