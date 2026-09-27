//! READABILITY — "information lives where the eyes already are" (owner: "willpower bars … it should
//! be visual; also give a way for players to see how high damage other players are … quickly
//! digestible … players shouldn't feel they have to break their sight of the action to check").
//!
//! Nothing here sits in a screen corner. Every read lives at the crosshair or on the players:
//! * **Heat** ([`heat_rgb`]): ONE Smash damage colour ramp (white → yellow → orange → red → deep
//!   crimson) shared by every damage read — the heat band inside other players' outlines
//!   (`present::outline`), the body tint (`present::players`), the compass pips
//!   (`present::tracking`), the over-head numbers ([`numbers`]), your own crosshair dot (`hud`),
//!   the ring hand and the screen-edge cracks (`viewmodel`). Never hue alone: the band widens and
//!   breathes faster as % rises (always < 3 Hz), and from [`HeatTuning::ko_pct`] the body smokes and
//!   throws sparks ([`smoke`]) — a finisher is obvious from across the map.
//! * **Willpower** lives around the crosshair (`power_hud::gauge`, the lantern-band gauge of whole
//!   SPARKS; `bd_sim::rules::MatchRules::willpower_segments`) and on the ring hand (glow = willpower,
//!   one pulse per spark spent).
//!
//! All numbers: `assets/tuning/readability.ron` ([`ReadabilityTuning`], hot-reloaded; a unit test
//! keeps it equal to the Rust defaults).

#[cfg(test)]
mod cost_survey;
pub mod numbers;
pub mod smoke;

use std::path::PathBuf;
use std::time::SystemTime;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------------------------ tuning

/// The damage colour ramp and its non-hue cues.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeatTuning {
    /// (damage %, sRGB colour) stops, ascending. The Smash ramp: white → yellow → orange → red →
    /// deep crimson.
    pub stops: Vec<(f32, [f32; 3])>,
    /// The heat band (inside the corps outline) fades in from `band_from` % (full by `band_full`);
    /// below it the thin dark inner line is kept.
    pub band_from: f32,
    pub band_full: f32,
    /// Heat band width (px at 1× outline width) at `band_from` … at `ko_pct` and beyond.
    pub band_px_min: f32,
    pub band_px_max: f32,
    /// The band breathes from `pulse_from` %: `pulse_hz_min` there … `pulse_hz_max` at `ko_pct`
    /// (always under the 3 Hz photosensitivity floor; static with screen flashes off).
    pub pulse_from: f32,
    pub pulse_hz_min: f32,
    pub pulse_hz_max: f32,
    /// KO range: smoke + sparks off the body, the deepest pulse.
    pub ko_pct: f32,
    /// Brightest channel of the band (linear): under the bloom threshold like the corps line.
    pub band_luma: f32,
}

impl Default for HeatTuning {
    fn default() -> Self {
        Self {
            stops: vec![
                (0.0, [1.0, 1.0, 1.0]),
                (35.0, [1.0, 0.92, 0.3]),
                (70.0, [1.0, 0.58, 0.1]),
                (110.0, [1.0, 0.16, 0.06]),
                (150.0, [0.66, 0.0, 0.1]),
            ],
            band_from: 8.0,
            band_full: 25.0,
            band_px_min: 1.6,
            band_px_max: 3.4,
            pulse_from: 70.0,
            pulse_hz_min: 0.8,
            pulse_hz_max: 2.4,
            ko_pct: 125.0,
            band_luma: 0.9,
        }
    }
}

/// The crosshair willpower gauge (`power_hud::gauge`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GaugeTuning {
    /// Arc radius and band thickness (px at a 1080-line window; scaled with the window height, never
    /// below `min_scale`).
    pub radius_px: f32,
    pub thickness_px: f32,
    pub min_scale: f32,
    /// Arc span under the crosshair (degrees, centred on 6 o'clock, filling left → right) and the gap
    /// between sparks (degrees).
    pub span_deg: f32,
    pub gap_deg: f32,
    /// Opacity when full and idle … while anything changes (drain, refill, preview, charge).
    pub idle_alpha: f32,
    pub active_alpha: f32,
    /// Seconds after the last change before it settles to `idle_alpha`, and the fade time.
    pub idle_after: f32,
    pub fade_secs: f32,
    /// Below this fraction of the pool (or under one spark) it dims and sputters.
    pub low_frac: f32,
    /// Spent sparks breaking off (s), a filled spark's glint (s).
    pub break_secs: f32,
    pub glint_secs: f32,
    /// Most ghost sparks drawn past the gauge's end for an unaffordable preview.
    pub ghost_max: u32,
    /// Gain of the soft tick when a spark fills.
    pub tick_gain: f32,
}

impl Default for GaugeTuning {
    fn default() -> Self {
        Self {
            radius_px: 40.0,
            thickness_px: 4.5,
            min_scale: 0.8,
            span_deg: 132.0,
            gap_deg: 3.2,
            idle_alpha: 0.28,
            active_alpha: 0.92,
            idle_after: 1.4,
            fade_secs: 0.35,
            low_frac: 0.2,
            break_secs: 0.28,
            glint_secs: 0.3,
            ghost_max: 6,
            tick_gain: 0.22,
        }
    }
}

/// Over-head damage numbers ([`numbers`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NumberTuning {
    /// Hidden beyond this distance (m).
    pub max_dist: f32,
    /// "Near the crosshair": within this angle (degrees) of the aim.
    pub near_deg: f32,
    /// Font size (px at a 1080-line window) at `near_m` … at `max_dist`.
    pub near_m: f32,
    pub font_near: f32,
    pub font_far: f32,
    /// Metres above the head.
    pub lift: f32,
    /// Fade in / out rate (per second).
    pub fade_per_sec: f32,
}

impl Default for NumberTuning {
    fn default() -> Self {
        Self { max_dist: 40.0, near_deg: 7.0, near_m: 6.0, font_near: 24.0, font_far: 14.0, lift: 0.45, fade_per_sec: 8.0 }
    }
}

/// KO-range smoke + sparks ([`smoke`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmokeTuning {
    /// Smoke puffs / sparks per second at `ko_pct`; both grow toward `max_mult`× by `ko_pct + 60`.
    pub smoke_per_sec: f32,
    pub sparks_per_sec: f32,
    pub max_mult: f32,
    /// Not for bodies farther than this (m) — the beacons carry them beyond.
    pub max_dist: f32,
}

impl Default for SmokeTuning {
    fn default() -> Self {
        Self { smoke_per_sec: 9.0, sparks_per_sec: 5.0, max_mult: 1.8, max_dist: 160.0 }
    }
}

/// Every readability presentation number (`assets/tuning/readability.ron`). Never shown.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadabilityTuning {
    pub heat: HeatTuning,
    pub gauge: GaugeTuning,
    pub numbers: NumberTuning,
    pub smoke: SmokeTuning,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct Readability(pub ReadabilityTuning);

fn tuning_path() -> PathBuf {
    crate::assets_dir().join("tuning").join("readability.ron")
}

fn load_tuning() -> ReadabilityTuning {
    let path = tuning_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => ron::from_str::<ReadabilityTuning>(&text).unwrap_or_else(|e| {
            eprintln!("readability tuning: failed to parse {}: {e}; using defaults", path.display());
            ReadabilityTuning::default()
        }),
        Err(_) => ReadabilityTuning::default(),
    }
}

fn hot_reload(time: Res<Time<Real>>, mut tuning: ResMut<Readability>, mut last: Local<(f32, Option<SystemTime>)>) {
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
                match ron::from_str::<ReadabilityTuning>(&text) {
                    Ok(t) => {
                        info!("readability tuning reloaded");
                        tuning.0 = t;
                    }
                    Err(e) => warn!("readability tuning: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}

pub struct ReadabilityPlugin;

impl Plugin for ReadabilityPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Readability(load_tuning())).add_systems(Update, hot_reload);
        smoke::build(app);
        numbers::build(app);
    }
}

// ------------------------------------------------------------------------------------ heat ramp

fn srgb_to_linear(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

/// The ramp colour at `pct` (sRGB, interpolated between stops). Never allocates.
pub fn heat_srgb(t: &HeatTuning, pct: f32) -> [f32; 3] {
    let p = if pct.is_finite() { pct.max(0.0) } else { 0.0 };
    let Some(first) = t.stops.first() else { return [1.0; 3] };
    if p <= first.0 {
        return first.1;
    }
    for w in t.stops.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if p <= b {
            let k = ((p - a) / (b - a).max(1e-4)).clamp(0.0, 1.0);
            return [ca[0] + (cb[0] - ca[0]) * k, ca[1] + (cb[1] - ca[1]) * k, ca[2] + (cb[2] - ca[2]) * k];
        }
    }
    t.stops.last().map_or([1.0; 3], |s| s.1)
}

/// [`heat_srgb`] in linear RGB.
pub fn heat_rgb(t: &HeatTuning, pct: f32) -> [f32; 3] {
    heat_srgb(t, pct).map(srgb_to_linear)
}

/// [`heat_srgb`] as a Bevy colour.
pub fn heat_color(t: &HeatTuning, pct: f32) -> Color {
    let c = heat_srgb(t, pct);
    Color::srgb(c[0], c[1], c[2])
}

fn smooth01(x: f32) -> f32 {
    let x = if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
    x * x * (3.0 - 2.0 * x)
}

/// How present the heat band is: 0 below `band_from` … 1 from `band_full`.
pub fn band_amount(t: &HeatTuning, pct: f32) -> f32 {
    smooth01((pct - t.band_from) / (t.band_full - t.band_from).max(1e-3))
}

/// 0 at `band_from` … 1 at `ko_pct` (drives band width).
pub fn severity(t: &HeatTuning, pct: f32) -> f32 {
    smooth01((pct - t.band_from) / (t.ko_pct - t.band_from).max(1e-3))
}

/// Heat band width (px at 1×).
pub fn band_px(t: &HeatTuning, pct: f32) -> f32 {
    t.band_px_min + (t.band_px_max - t.band_px_min) * severity(t, pct)
}

/// Breathing rate of the band (Hz; 0 = steady below `pulse_from`). Always < 3 Hz.
pub fn pulse_hz(t: &HeatTuning, pct: f32) -> f32 {
    if !pct.is_finite() || pct < t.pulse_from {
        return 0.0;
    }
    let k = ((pct - t.pulse_from) / (t.ko_pct - t.pulse_from).max(1e-3)).clamp(0.0, 1.0);
    (t.pulse_hz_min + (t.pulse_hz_max - t.pulse_hz_min) * k).clamp(0.0, 2.9)
}

/// In KO range (smoke, sparks).
pub fn ko_range(t: &HeatTuning, pct: f32) -> bool {
    pct.is_finite() && pct >= t.ko_pct
}

/// How far into KO range (0 at `ko_pct` … 1 at `ko_pct + 60`).
pub fn ko_depth(t: &HeatTuning, pct: f32) -> f32 {
    if !ko_range(t, pct) { 0.0 } else { ((pct - t.ko_pct) / 60.0).clamp(0.0, 1.0) }
}

/// The heat band colour (linear) at `pct` with breathing phase value `breath01` (0..1): the ramp
/// colour, brightest channel = `band_luma` (under the bloom knee), dimming to 55 % at the trough of
/// the breath.
pub fn band_rgb(t: &HeatTuning, pct: f32, breath01: f32) -> [f32; 3] {
    let c = heat_rgb(t, pct);
    let m = c[0].max(c[1]).max(c[2]).max(1e-3);
    let pulsing = pulse_hz(t, pct) > 0.0;
    let b = if pulsing { 0.55 + 0.45 * breath01.clamp(0.0, 1.0) } else { 1.0 };
    c.map(|x| x / m * t.band_luma * b)
}

// ------------------------------------------------------------------------------------ gauge math

/// The cost bite a previewed phrase takes out of the crosshair gauge, in sparks (the gauge's unit:
/// `bd_sim` quantises every cast cost to whole sparks, so this is exact).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bite {
    /// Sparks the cast will spend (whole; 0 = nothing previewed / free).
    pub cost: f32,
    /// You can pay it: the flickering slice is `[have - cost, have)`.
    pub afford: bool,
    /// Flicker range (spark units along the gauge).
    pub lo: f32,
    pub hi: f32,
    /// Unaffordable: the missing sparks, drawn as ghosts from `have` (past the gauge's end when the
    /// cost is more than the whole gauge).
    pub missing: f32,
}

/// `have` / `cost` in spark units. A cast is affordable when `cost ≤ have` (1e-3 slack: the sim
/// pays with 1e-4 of slack, so a gauge showing exactly the cost is never wrong).
pub fn cost_bite(have: f32, cost: f32) -> Bite {
    let have = if have.is_finite() { have.max(0.0) } else { 0.0 };
    if !cost.is_finite() || cost <= 0.0 {
        return Bite::default();
    }
    if cost <= have + 1e-3 {
        Bite { cost, afford: true, lo: (have - cost).max(0.0), hi: have, missing: 0.0 }
    } else {
        Bite { cost, afford: false, lo: 0.0, hi: have, missing: cost - have }
    }
}

/// Whole sparks filled at `have` (spark units).
pub fn whole(have: f32) -> u32 {
    if have.is_finite() { (have.max(0.0) + 1e-3).floor() as u32 } else { 0 }
}

// ------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readability_ron_matches_the_rust_defaults() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/readability.ron");
        let text = std::fs::read_to_string(path).expect("assets/tuning/readability.ron");
        assert_eq!(ron::from_str::<ReadabilityTuning>(&text).expect("parses"), ReadabilityTuning::default());
    }

    fn lum(c: [f32; 3]) -> f32 {
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    #[test]
    fn heat_ramp_is_the_smash_ramp() {
        let t = HeatTuning::default();
        // White at 0, yellow, orange, red, deep crimson.
        assert_eq!(heat_srgb(&t, 0.0), [1.0, 1.0, 1.0]);
        let y = heat_srgb(&t, 35.0);
        assert!(y[0] > 0.95 && y[1] > 0.85 && y[2] < 0.4, "yellow {y:?}");
        let o = heat_srgb(&t, 70.0);
        assert!(o[0] > 0.95 && (0.4..0.7).contains(&o[1]) && o[2] < 0.2, "orange {o:?}");
        let r = heat_srgb(&t, 110.0);
        assert!(r[0] > 0.95 && r[1] < 0.25, "red {r:?}");
        let k = heat_srgb(&t, 400.0);
        assert!(k[0] < 0.8 && k[1] < 0.05, "crimson {k:?}");
        // Monotone: never gets brighter / more yellow as % rises (green channel falls).
        let mut last = heat_srgb(&t, 0.0);
        for p in 1..300 {
            let c = heat_srgb(&t, p as f32);
            assert!(c[1] <= last[1] + 1e-6, "{p}");
            assert!(lum(c) <= lum(last) + 1e-6, "{p}");
            last = c;
        }
        // Garbage in: white / clamped, never NaN.
        assert_eq!(heat_srgb(&t, f32::NAN), [1.0; 3]);
        assert_eq!(heat_srgb(&t, -5.0), [1.0; 3]);
        assert!(heat_rgb(&t, 1e9).iter().all(|c| c.is_finite()));
        // An empty ramp is white.
        assert_eq!(heat_srgb(&HeatTuning { stops: vec![], ..t.clone() }, 50.0), [1.0; 3]);
    }

    #[test]
    fn three_bots_read_apart_without_hue() {
        // The screenshot trio (20 %, 80 %, 160 %): distinct colour AND distinct non-hue cues.
        let t = HeatTuning::default();
        let (a, b, c) = (20.0, 80.0, 160.0);
        assert!(band_px(&t, a) < band_px(&t, b) && band_px(&t, b) < band_px(&t, c), "width grows");
        assert_eq!(pulse_hz(&t, a), 0.0, "low % is steady");
        assert!(pulse_hz(&t, b) > 0.0 && pulse_hz(&t, c) > pulse_hz(&t, b), "breathes faster with %");
        assert!(!ko_range(&t, b) && ko_range(&t, c), "only the KO-range body smokes");
        let (ca, cb, cc) = (heat_srgb(&t, a), heat_srgb(&t, b), heat_srgb(&t, c));
        let d = |x: [f32; 3], y: [f32; 3]| (x[0] - y[0]).abs() + (x[1] - y[1]).abs() + (x[2] - y[2]).abs();
        assert!(d(ca, cb) > 0.4 && d(cb, cc) > 0.4, "{ca:?} {cb:?} {cc:?}");
    }

    #[test]
    fn band_stays_comfort_safe_and_under_bloom() {
        let t = HeatTuning::default();
        for p in 0..600 {
            let p = p as f32;
            assert!(pulse_hz(&t, p) < 3.0);
            for b in [0.0, 0.5, 1.0] {
                let c = band_rgb(&t, p, b);
                assert!(c.iter().all(|x| x.is_finite() && *x <= t.band_luma + 1e-4), "{p} {c:?}");
            }
        }
        assert_eq!(band_amount(&t, 0.0), 0.0);
        assert_eq!(band_amount(&t, t.band_full), 1.0);
        assert!(band_px(&t, 0.0) >= t.band_px_min - 1e-4 && band_px(&t, 999.0) <= t.band_px_max + 1e-4);
        // Steady below the pulse threshold: breath phase doesn't matter.
        assert_eq!(band_rgb(&t, 40.0, 0.0), band_rgb(&t, 40.0, 1.0));
        assert_ne!(band_rgb(&t, 140.0, 0.0), band_rgb(&t, 140.0, 1.0));
    }

    #[test]
    fn cost_bite_affordable_and_not() {
        // 5.4 sparks, a 2-spark cast: the top two flicker.
        let b = cost_bite(5.4, 2.0);
        assert!(b.afford && b.missing == 0.0);
        assert!((b.lo - 3.4).abs() < 1e-5 && (b.hi - 5.4).abs() < 1e-5);
        // Exactly enough is affordable (the sim pays with slack).
        assert!(cost_bite(3.0, 3.0).afford);
        assert!(cost_bite(2.9995, 3.0).afford);
        // 2.5 sparks, a 5-spark cast: red over what you have, 2.5 missing as ghosts.
        let b = cost_bite(2.5, 5.0);
        assert!(!b.afford);
        assert_eq!((b.lo, b.hi), (0.0, 2.5));
        assert!((b.missing - 2.5).abs() < 1e-5);
        // More than the whole gauge: the ghosts run past its end.
        let b = cost_bite(8.0, 11.0);
        assert!(!b.afford && (b.missing - 3.0).abs() < 1e-5);
        // Nothing previewed / free / garbage.
        assert_eq!(cost_bite(4.0, 0.0), Bite::default());
        assert_eq!(cost_bite(4.0, f32::NAN), Bite::default());
        assert!(!cost_bite(f32::NAN, 1.0).afford);
        assert_eq!(whole(2.9996), 3);
        assert_eq!(whole(2.9), 2);
        assert_eq!(whole(-1.0), 0);
    }
}
