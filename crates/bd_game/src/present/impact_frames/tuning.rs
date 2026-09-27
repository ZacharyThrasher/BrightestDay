//! `assets/tuning/impact_frames.ron`: the impact-frame rules and the style table (hot-reloaded,
//! mtime poll every 0.5 s like `juice.ron`). A new style is data only.

use bd_core::mechanics::Element;
use std::collections::HashMap;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// One look (a key in `styles`). Colours are sRGB 0..1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImpactStyle {
    /// The dark tone (shadows, speed lines, inverted silhouettes).
    pub ink: [f32; 3],
    /// The light tone.
    pub paper: [f32; 3],
    /// Lightning / rim / crack / panel colour (a request's `tint` replaces it).
    pub accent: [f32; 3],
    /// Luminance inversion (Black Flash: black bodies on a red-white world). Off with Reduce flashes.
    pub invert: bool,
    /// Tone split point on the compressed luminance (0..1).
    pub threshold: f32,
    /// Softness of the split (0 = a hard two-tone cut).
    pub softness: f32,
    /// Posterize levels (2 = duotone; 3-4 = sumi washes).
    pub levels: f32,
    /// Ink edge lines (0..1).
    pub edges: f32,
    /// Radial speed lines rushing to the impact (0..1) and how many of the angular slots draw one.
    pub lines: f32,
    pub line_density: f32,
    /// Bent lightning bolts around the impact (0..1); `lightning_ink_core` = black core, accent glow.
    pub lightning: f32,
    pub lightning_ink_core: bool,
    /// Screen crack radiating from the impact (0..1).
    pub crack: f32,
    /// Halftone dots (0..1).
    pub halftone: f32,
    /// A jagged comic panel burst behind the impact (0..1).
    pub panel: f32,
    /// Block glitch / scanline tearing (0..1).
    pub glitch: f32,
    /// Sumi brush: wobbling edges, paper grain, ink splats (0..1).
    pub brush: f32,
    /// Chromatic split at the screen edge (fraction of screen height).
    pub chroma: f32,
    /// Zoom toward the impact during the hold (0.06 = 6 %); it snaps back on release.
    pub zoom: f32,
    /// Edge vignette toward ink (0..1).
    pub vignette: f32,
    /// Output brightness of the treatment (pre-tonemap).
    pub exposure: f32,
    /// Hold-time multiplier (on top of the weight → frames mapping).
    pub hold_mult: f32,
    /// Extra render-side hit-stop on attacker + victim (s) while the frame holds.
    pub hitstop: f32,
    /// Sounds at the hit (during the hold) and on the snap back: (name, gain, pitch).
    pub sfx_in: Vec<(String, f32, f32)>,
    pub sfx_out: Vec<(String, f32, f32)>,
    /// A comic word (`assets/fx/onomatopoeia.ron` key) over the impact.
    pub word: Option<String>,
    /// World lightning (hard-light zaps) around the impact point, visible to everyone.
    pub zaps: u8,
}

impl Default for ImpactStyle {
    fn default() -> Self {
        Self {
            ink: [0.02, 0.02, 0.03],
            paper: [0.95, 0.94, 0.9],
            accent: [1.0, 1.0, 1.0],
            invert: false,
            threshold: 0.42,
            softness: 0.03,
            levels: 2.0,
            edges: 0.6,
            lines: 0.8,
            line_density: 0.45,
            lightning: 0.0,
            lightning_ink_core: false,
            crack: 0.0,
            halftone: 0.0,
            panel: 0.0,
            glitch: 0.0,
            brush: 0.0,
            chroma: 0.006,
            zoom: 0.05,
            vignette: 0.35,
            exposure: 1.25,
            hold_mult: 1.0,
            hitstop: 0.0,
            sfx_in: Vec::new(),
            sfx_out: Vec::new(),
            word: None,
            zaps: 0,
        }
    }
}

/// Default styles per trigger (keys into `styles`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TriggerStyles {
    pub r#move: String,
    pub crit: String,
    pub ko: String,
    pub signature: String,
    pub counter: String,
    pub heavy_weapon: String,
    pub beam_clash: String,
    pub domain: String,
    pub custom: String,
}

impl Default for TriggerStyles {
    fn default() -> Self {
        Self {
            r#move: "ink".into(),
            crit: "ink".into(),
            ko: "ink".into(),
            signature: "holy".into(),
            counter: "comic".into(),
            heavy_weapon: "ink".into(),
            beam_clash: "glitch".into(),
            domain: "void".into(),
            custom: "ink".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImpactRules {
    /// Flash cap: at most this many frames in any 1 s window (clamped to ≤ 3, WCAG 2.3.1).
    pub max_per_sec: u8,
    /// Frames at least this far apart (s).
    pub min_gap: f32,
    /// The same attacker gets at most one frame per this many seconds (KO blows excepted).
    pub same_attacker_gap: f32,
    /// Reduce flashes = Reduced: at most one frame per this many seconds.
    pub reduced_gap: f32,
    /// Weight 0 → `.0` frames, weight 1 → `.1` frames of `frame_secs` each.
    pub hold_frames: (f32, f32),
    pub frame_secs: f32,
    /// Snap back (s): full / Reduced.
    pub release_secs: f32,
    pub reduced_release_secs: f32,
    /// Reduced: fade in (s) and the strength / contrast ceiling.
    pub reduced_fade_in: f32,
    pub reduced_strength: f32,
    pub reduced_contrast: f32,
    /// Bystanders within this many metres who can see the point get the bubble.
    pub bubble_range: f32,
    /// Bubble radius (m, world) and its strength.
    pub bubble_radius: f32,
    pub bubble_strength: f32,
    /// Burned-in silhouettes: pre-tonemap luminance where the hot key starts (bodies are flashed
    /// to emissive ~7 while the frame holds).
    pub hot_luma: f32,
    /// FOV zoom punch (degrees, negative = in) on the snap back, attacker / victim only.
    pub release_fov: f32,
    /// A move's `impact_frame` arms for this long, waiting for the move's hit (s).
    pub move_window: f32,
    /// `HitKind::HeavyMelee` at or above this `impact.strength` fires `HeavyWeapon`.
    pub heavy_melee_min_strength: f32,
    /// KO blows below this weight (knockback-derived) are skipped (a chip KO off the edge).
    pub ko_min_weight: f32,
    pub trigger_styles: TriggerStyles,
    /// `CritFx` name → style (`"BlackFlash": "black_flash"`).
    pub crit_styles: HashMap<String, String>,
    /// Signature hits by element (`Psychic: "void"` — Hollow Purple).
    pub element_styles: HashMap<Element, String>,
}

impl Default for ImpactRules {
    fn default() -> Self {
        Self {
            max_per_sec: 3,
            min_gap: 0.34,
            same_attacker_gap: 0.8,
            reduced_gap: 1.0,
            hold_frames: (2.0, 5.0),
            frame_secs: 1.0 / 30.0,
            release_secs: 0.14,
            reduced_release_secs: 0.4,
            reduced_fade_in: 0.1,
            reduced_strength: 0.55,
            reduced_contrast: 0.45,
            bubble_range: 45.0,
            bubble_radius: 4.5,
            bubble_strength: 0.9,
            hot_luma: 5.0,
            release_fov: -4.0,
            move_window: 1.2,
            heavy_melee_min_strength: 0.9,
            ko_min_weight: 0.35,
            trigger_styles: TriggerStyles::default(),
            crit_styles: HashMap::new(),
            element_styles: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ImpactFrameTuning {
    pub rules: ImpactRules,
    pub styles: HashMap<String, ImpactStyle>,
}

impl ImpactFrameTuning {
    /// The style for `key` (fallback: `fallback`, then any "ink", then the built-in default).
    pub fn style<'a>(&'a self, key: &str, fallback: &str) -> (&'a str, Option<&'a ImpactStyle>) {
        if let Some((k, s)) = self.styles.get_key_value(key) {
            return (k.as_str(), Some(s));
        }
        if let Some((k, s)) = self.styles.get_key_value(fallback) {
            return (k.as_str(), Some(s));
        }
        match self.styles.get_key_value("ink") {
            Some((k, s)) => (k.as_str(), Some(s)),
            None => ("", None),
        }
    }
}

#[derive(Resource, Default)]
pub struct ImpactFrameTuningRes(pub ImpactFrameTuning);

pub fn path() -> PathBuf {
    crate::assets_dir().join("tuning").join("impact_frames.ron")
}

pub fn parse(text: &str) -> Result<ImpactFrameTuning, String> {
    ron::from_str::<ImpactFrameTuning>(text).map_err(|e| e.to_string())
}

pub fn load() -> ImpactFrameTuning {
    match std::fs::read_to_string(path()) {
        Ok(text) => parse(&text).unwrap_or_else(|e| {
            eprintln!("impact frames: failed to parse {}: {e}; using defaults", path().display());
            ImpactFrameTuning::default()
        }),
        Err(_) => ImpactFrameTuning::default(),
    }
}

pub fn hot_reload(time: Res<Time<Real>>, mut tuning: ResMut<ImpactFrameTuningRes>, mut last: Local<(f32, Option<SystemTime>)>) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let Ok(modified) = std::fs::metadata(path()).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(modified),
        Some(prev) if prev != modified => {
            last.1 = Some(modified);
            if let Ok(text) = std::fs::read_to_string(path()) {
                match parse(&text) {
                    Ok(t) => {
                        info!("impact frame tuning reloaded ({} styles)", t.styles.len());
                        tuning.0 = t;
                    }
                    Err(e) => warn!("impact frames: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}
