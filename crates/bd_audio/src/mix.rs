//! Mixer parameters, snapshots and their blending (`assets/audio/mix.ron`, hot-reloadable).
//!
//! A [`MixParams`] is the complete set of continuous mixer parameters. Snapshots (Exploration,
//! Combat, Danger, RingOpen, Space, …) are *partial* parameter sets; [`MixParams::blend`] moves
//! the unset fields nowhere and the set ones towards the snapshot by a weight, in perceptual
//! units (gains in dB, filter cutoffs in log-frequency). The game-side [`crate::director`]
//! computes the weights; the audio thread smooths whatever arrives, so there is no zipper noise.

use std::collections::HashMap;

use serde::Deserialize;

use crate::cue::{Bus, NUM_BUSES};
use crate::dsp::{lerp, lerp_hz};
use crate::reverb::ReverbParams;

pub const OPEN_HZ: f32 = 20000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BusParams {
    pub gain_db: f32,
    pub lpf_hz: f32,
    pub hpf_hz: f32,
    /// Offset on every voice's reverb send (dB).
    pub send_db: f32,
}

impl Default for BusParams {
    fn default() -> Self {
        Self { gain_db: 0.0, lpf_hz: OPEN_HZ, hpf_hz: 10.0, send_db: 0.0 }
    }
}

/// Every continuous mixer parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixParams {
    pub bus: [BusParams; NUM_BUSES],
    /// World group (all world buses + reverb return).
    pub world_gain_db: f32,
    pub world_lpf_hz: f32,
    /// Applied per voice to everything that is not the local player's own sound (Space: the
    /// vacuum leaves only body-conducted sound).
    pub remote_gain_db: f32,
    pub remote_lpf_hz: f32,
    /// Reverb return offset (dB) on top of the acoustics estimate.
    pub reverb_db: f32,
    /// Music intensity 0..1 and tempo multiplier (sent to the music source).
    pub music_intensity: f32,
    pub music_tempo: f32,
}

impl Default for MixParams {
    fn default() -> Self {
        Self {
            bus: [BusParams::default(); NUM_BUSES],
            world_gain_db: 0.0,
            world_lpf_hz: OPEN_HZ,
            remote_gain_db: 0.0,
            remote_lpf_hz: OPEN_HZ,
            reverb_db: 0.0,
            music_intensity: 0.3,
            music_tempo: 1.0,
        }
    }
}

/// Partial bus settings in a snapshot.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct BusSpec {
    pub gain_db: Option<f32>,
    pub lpf_hz: Option<f32>,
    pub hpf_hz: Option<f32>,
    pub send_db: Option<f32>,
}

/// A snapshot: a partial [`MixParams`] plus how fast the director fades it in and out.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Snapshot {
    pub fade_in: f32,
    pub fade_out: f32,
    pub buses: HashMap<Bus, BusSpec>,
    pub world_gain_db: Option<f32>,
    pub world_lpf_hz: Option<f32>,
    pub remote_gain_db: Option<f32>,
    pub remote_lpf_hz: Option<f32>,
    pub reverb_db: Option<f32>,
    pub music_intensity: Option<f32>,
    pub music_tempo: Option<f32>,
}

impl MixParams {
    /// Move towards `s` by weight `w` (0 = unchanged, 1 = the snapshot's values where it sets them).
    pub fn blend(&self, s: &Snapshot, w: f32) -> MixParams {
        let w = if w.is_finite() { w.clamp(0.0, 1.0) } else { 0.0 };
        let mut o = *self;
        if w <= 0.0 {
            return o;
        }
        let g = |a: f32, b: Option<f32>| b.map_or(a, |b| lerp(a, b, w));
        let f = |a: f32, b: Option<f32>| b.map_or(a, |b| lerp_hz(a, b, w));
        for (bus, spec) in &s.buses {
            let p = &mut o.bus[bus.index()];
            p.gain_db = g(p.gain_db, spec.gain_db);
            p.lpf_hz = f(p.lpf_hz, spec.lpf_hz);
            p.hpf_hz = f(p.hpf_hz, spec.hpf_hz);
            p.send_db = g(p.send_db, spec.send_db);
        }
        o.world_gain_db = g(o.world_gain_db, s.world_gain_db);
        o.world_lpf_hz = f(o.world_lpf_hz, s.world_lpf_hz);
        o.remote_gain_db = g(o.remote_gain_db, s.remote_gain_db);
        o.remote_lpf_hz = f(o.remote_lpf_hz, s.remote_lpf_hz);
        o.reverb_db = g(o.reverb_db, s.reverb_db);
        o.music_intensity = g(o.music_intensity, s.music_intensity);
        o.music_tempo = g(o.music_tempo, s.music_tempo);
        o
    }
    /// Largest perceptual difference to `o` (dB or semitone-ish units): used to skip redundant sends.
    pub fn distance(&self, o: &MixParams) -> f32 {
        let hz = |a: f32, b: f32| (a.max(1.0) / b.max(1.0)).log2().abs() * 12.0;
        let mut d = 0.0f32;
        for (a, b) in self.bus.iter().zip(o.bus.iter()) {
            d = d.max((a.gain_db - b.gain_db).abs()).max(hz(a.lpf_hz, b.lpf_hz)).max(hz(a.hpf_hz, b.hpf_hz));
            d = d.max((a.send_db - b.send_db).abs());
        }
        d.max((self.world_gain_db - o.world_gain_db).abs())
            .max(hz(self.world_lpf_hz, o.world_lpf_hz))
            .max((self.remote_gain_db - o.remote_gain_db).abs())
            .max(hz(self.remote_lpf_hz, o.remote_lpf_hz))
            .max((self.reverb_db - o.reverb_db).abs())
            .max((self.music_intensity - o.music_intensity).abs() * 20.0)
            .max((self.music_tempo - o.music_tempo).abs() * 50.0)
    }
}

/// HDR loudness window (DICE-style). Loudness is in LUFS-ish dB (file envelope + gains).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct HdrConfig {
    pub enabled: bool,
    /// Window top rests here; louder sounds push the window up.
    pub threshold_db: f32,
    /// Fraction of the excess above threshold removed from the World (1 = pure window, the
    /// loudest sound always lands at the threshold; <1 lets loud events stay a bit louder).
    pub ratio: f32,
    /// Window height: voices quieter than top − range are culled (virtual) until it is quiet again.
    pub range_db: f32,
    /// How fast the window top falls back (dB/s) after `hold` seconds.
    pub release_db_s: f32,
    pub hold: f32,
    /// Night mode: range and ratio used when "reduced dynamic range" is on.
    pub night_range_db: f32,
    pub night_ratio: f32,
}

impl Default for HdrConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_db: -16.0,
            ratio: 0.7,
            range_db: 30.0,
            release_db_s: 14.0,
            hold: 0.08,
            night_range_db: 24.0,
            night_ratio: 1.0,
        }
    }
}

/// Feedback → World sidechain (critical cues carve a hole in the world for a moment).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct SidechainConfig {
    /// Broadband duck of World (and Music) at full key level (dB, negative).
    pub depth_db: f32,
    /// Extra high-shelf cut on World above `shelf_hz` (dB, negative): clears 5-12 kHz for the cue.
    pub shelf_db: f32,
    pub shelf_hz: f32,
    /// Key level where ducking starts / reaches full depth (dBFS on the Feedback bus).
    pub threshold_db: f32,
    pub knee_db: f32,
    pub attack: f32,
    pub release: f32,
}

impl Default for SidechainConfig {
    fn default() -> Self {
        Self { depth_db: -4.5, shelf_db: -6.0, shelf_hz: 4500.0, threshold_db: -42.0, knee_db: 12.0, attack: 0.004, release: 0.16 }
    }
}

/// An explicit duck envelope (heavy hit, KO confirm, final blow).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct DuckSpec {
    pub depth_db: f32,
    /// World low-pass at full depth (Hz; 20000 = none) — the final-blow "suck".
    pub lpf_hz: f32,
    pub attack: f32,
    pub hold: f32,
    pub release: f32,
    /// Also duck the music.
    pub music: bool,
    /// Music duck depth (dB) if different from `depth_db` (the music is a bed: duck it less).
    pub music_db: Option<f32>,
}

impl Default for DuckSpec {
    fn default() -> Self {
        Self { depth_db: -6.0, lpf_hz: OPEN_HZ, attack: 0.01, hold: 0.1, release: 0.25, music: true, music_db: None }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct LimiterConfig {
    pub ceiling_db: f32,
    pub knee_db: f32,
    pub lookahead_ms: f32,
    pub release: f32,
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self { ceiling_db: -1.0, knee_db: 3.0, lookahead_ms: 1.5, release: 0.12 }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct VoiceConfig {
    /// Rendered voices at once (the rest are virtual: tracked, silent, resumable).
    pub max_real: usize,
    /// Voice slots (real + virtual).
    pub max_total: usize,
    /// Fade when a voice goes real ↔ virtual or is stolen (s).
    pub fade: f32,
    /// Own sounds: trim (dB) — slightly more present than everyone else's.
    pub own_gain_db: f32,
    /// Own sounds: priority bonus.
    pub own_priority: f32,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self { max_real: 48, max_total: 192, fade: 0.012, own_gain_db: 1.5, own_priority: 10.0 }
    }
}

/// Music bus carve: out of the sub (< ~60 Hz) and the 5-12 kHz feedback band.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct MusicCarve {
    pub hpf_hz: f32,
    pub notch_hz: f32,
    pub notch_q: f32,
    pub notch_db: f32,
}

impl Default for MusicCarve {
    fn default() -> Self {
        Self { hpf_hz: 60.0, notch_hz: 7500.0, notch_q: 0.6, notch_db: -8.0 }
    }
}

/// Night mode master compressor.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct NightConfig {
    pub threshold_db: f32,
    pub ratio: f32,
    pub makeup_db: f32,
}

impl Default for NightConfig {
    fn default() -> Self {
        Self { threshold_db: -26.0, ratio: 3.0, makeup_db: 6.0 }
    }
}

/// `assets/audio/mix.ron`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct MixConfig {
    /// Fixed master makeup (dB) before the limiter: sets the overall playback level.
    pub output_gain_db: f32,
    pub hdr: HdrConfig,
    pub sidechain: SidechainConfig,
    pub limiter: LimiterConfig,
    pub voices: VoiceConfig,
    pub music_carve: MusicCarve,
    pub night: NightConfig,
    /// Named ducks ("heavy_hit", "ko_confirm", "final_blow").
    pub ducks: HashMap<String, DuckSpec>,
    /// Neutral starting point every snapshot blends from (bus trims live here).
    pub base: Snapshot,
    pub snapshots: HashMap<String, Snapshot>,
    /// Default reverb when no acoustics estimate is available.
    pub reverb: ReverbParams,
}

impl Default for MixConfig {
    fn default() -> Self {
        Self {
            output_gain_db: 0.0,
            hdr: HdrConfig::default(),
            sidechain: SidechainConfig::default(),
            limiter: LimiterConfig::default(),
            voices: VoiceConfig::default(),
            music_carve: MusicCarve::default(),
            night: NightConfig::default(),
            ducks: HashMap::new(),
            base: Snapshot::default(),
            snapshots: HashMap::new(),
            reverb: ReverbParams::default(),
        }
    }
}

impl MixConfig {
    pub fn from_ron(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| e.to_string())
    }
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_ron(&t)
    }
    /// The base parameters (defaults + `base`).
    pub fn base_params(&self) -> MixParams {
        MixParams::default().blend(&self.base, 1.0)
    }
    pub fn duck(&self, name: &str) -> DuckSpec {
        self.ducks.get(name).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> Snapshot {
        let mut s = Snapshot { world_lpf_hz: Some(500.0), world_gain_db: Some(-12.0), ..Default::default() };
        s.buses.insert(Bus::Ambience, BusSpec { gain_db: Some(-6.0), ..Default::default() });
        s
    }

    #[test]
    fn blend_endpoints_are_exact() {
        let base = MixParams::default();
        let s = snap();
        assert_eq!(base.blend(&s, 0.0), base);
        let full = base.blend(&s, 1.0);
        assert!((full.world_lpf_hz - 500.0).abs() < 0.01);
        assert!((full.world_gain_db - -12.0).abs() < 1e-5);
        assert!((full.bus[Bus::Ambience.index()].gain_db - -6.0).abs() < 1e-5);
        // Unset fields are untouched.
        assert_eq!(full.remote_lpf_hz, base.remote_lpf_hz);
        assert_eq!(full.bus[Bus::Impacts.index()], base.bus[Bus::Impacts.index()]);
    }

    #[test]
    fn blend_is_perceptual_and_monotonic() {
        let base = MixParams::default();
        let s = snap();
        let half = base.blend(&s, 0.5);
        // Log-frequency midpoint of 20 kHz and 500 Hz is 3162 Hz; dB midpoint of 0 and -12 is -6.
        assert!((half.world_lpf_hz - 3162.3).abs() < 1.0, "{}", half.world_lpf_hz);
        assert!((half.world_gain_db - -6.0).abs() < 1e-4);
        let mut prev = f32::INFINITY;
        for i in 0..=10 {
            let p = base.blend(&s, i as f32 / 10.0);
            assert!(p.world_lpf_hz <= prev + 1e-3);
            prev = p.world_lpf_hz;
        }
        // Garbage weights never produce garbage params.
        assert_eq!(base.blend(&s, f32::NAN), base);
        assert_eq!(base.blend(&s, 7.0), full_of(&base, &s));
    }

    fn full_of(b: &MixParams, s: &Snapshot) -> MixParams {
        b.blend(s, 1.0)
    }
}
