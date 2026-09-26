//! bd_audio — Brightest Day's audio engine: one software mixer that makes the arena sound like
//! **one world that reacts to the players**.
//!
//! Two halves connected by lock-free rings:
//! * [`AudioEngine`] (game thread): name → sound, cue table ([`cue`]), micro-variation, cooldowns,
//!   listener, snapshots ([`director`]), ambience ([`ambience`]), acoustics ([`acoustics`]).
//! * [`Mixer`] (audio thread): voices → buses → shared reverb → World group (HDR window, ducking,
//!   Feedback sidechain) → music → master limiter. No allocation, locks or I/O while rendering.
//!
//! Output goes to one cpal stream ([`output`], feature `device`) or is rendered offline
//! (`examples/render_mix.rs`). Data: `assets/sfx/manifest.ron` (sounds), `assets/audio/cues.ron`
//! (per-sound mix behaviour), `assets/audio/mix.ron` (buses, HDR, ducks, snapshots),
//! `assets/audio/maps.ron` (per-map acoustics + ambience).
//!
//! ```no_run
//! use bd_audio::*;
//! use std::sync::Arc;
//! let assets = std::path::Path::new("assets");
//! let mut warnings = Vec::new();
//! let bank = Arc::new(SoundBank::load(assets, &mut warnings).unwrap());
//! let cues = CueTable::load(&assets.join("audio/cues.ron")).unwrap();
//! let mix = MixConfig::load(&assets.join("audio/mix.ron")).unwrap();
//! let (mut engine, mixer) = AudioEngine::new(bank, &cues, mix, 48000, 1);
//! # #[cfg(feature = "device")]
//! let _out = output::start(mixer, 256).unwrap();
//! engine.play("hit_heavy", PlayOpts::at(glam::Vec3::new(3.0, 1.0, -4.0)));
//! ```

pub mod acoustics;
pub mod ambience;
pub mod analysis;
pub mod bank;
pub mod cue;
pub mod director;
pub mod dsp;
pub mod dynamics;
pub mod engine;
pub mod mix;
pub mod mixer;
pub mod reverb;
pub mod ring;

#[cfg(feature = "device")]
pub mod output;

#[cfg(feature = "sim")]
pub mod game;

pub use acoustics::{ListenerSpace, SpaceEstimate, SpacePreset, occlusion};
pub use ambience::{AmbienceDirector, EmitterMap, MapAudio, MapAudioTable, Zone};
pub use bank::{Pcm, SoundBank, SoundData, SoundId};
pub use cue::{Bus, CueParams, CueTable};
pub use director::{MixDirector, MixInputs, Scene};
pub use engine::{AudioEngine, Emit, PlayOpts, VoiceHandle};
pub use mix::{DuckSpec, MixConfig, MixParams, Snapshot};
pub use mixer::{AudioSettings, BLOCK, Listener, Mixer, MusicCommand, MusicSource, Telemetry};
pub use reverb::ReverbParams;

/// Everything a client needs, loaded from an assets directory.
pub struct AudioAssets {
    pub bank: std::sync::Arc<SoundBank>,
    pub cues: CueTable,
    pub mix: MixConfig,
    pub maps: MapAudioTable,
    pub warnings: Vec<String>,
}

impl AudioAssets {
    /// Load `sfx/manifest.ron` (+ decode every file) and the `audio/*.ron` tables. Missing tables
    /// fall back to defaults (with a warning) — audio never stops the game.
    pub fn load(assets: &std::path::Path) -> Result<Self, bank::BankError> {
        let mut warnings = Vec::new();
        let bank = std::sync::Arc::new(SoundBank::load(assets, &mut warnings)?);
        let dir = assets.join("audio");
        let cues = CueTable::load(&dir.join("cues.ron")).unwrap_or_else(|e| {
            warnings.push(e);
            CueTable::default()
        });
        let mix = MixConfig::load(&dir.join("mix.ron")).unwrap_or_else(|e| {
            warnings.push(e);
            MixConfig::default()
        });
        let maps = MapAudioTable::load(&dir.join("maps.ron")).unwrap_or_else(|e| {
            warnings.push(e);
            MapAudioTable::default()
        });
        Ok(Self { bank, cues, mix, maps, warnings })
    }
}
