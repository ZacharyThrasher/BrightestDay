//! bd_music — Brightest Day's adaptive chiptune soundtrack.
//!
//! Public-domain classical pieces in our own chip arrangements, authored as text
//! (`assets/music/*.ron`, a small MML dialect — see [`notation`] and the crate README), synthesised
//! in real time by a deterministic, band-limited chip synth ([`synth`]) and played by an adaptive
//! sequencer ([`MusicPlayer`]) that outputs three layered stems (base / drive / lead) plus a
//! stinger bus.
//!
//! # Integration contract (bd_audio owns the player on the audio thread)
//! ```ignore
//! // load (any thread, allocates):
//! let lib = Arc::new(bd_music::MusicLibrary::load_dir(Path::new("assets/music"))?);
//! let menu = lib.track_id("menu").unwrap();
//! let mut player = bd_music::MusicPlayer::new(lib, 48_000);
//! // audio thread, per callback (no allocation, no locks, no syscalls):
//! for cmd in queue.drain() { player.command(cmd); }          // MusicCommand is Copy
//! player.render(&mut music_bus_interleaved_stereo);            // or render_stems(..)
//! ```
//! * [`MusicCommand::Play`] switches on the next bar line (seamless handoff); `SetIntensity`
//!   fades stems in/out on bar lines; `Stinger` lands on the next beat (KO, streak) or bar
//!   (match point, last standing, victory, defeat); `SetTempo` ramps the tempo without changing pitch.
//! * Output is interleaved stereo at the player's sample rate, already high-passed (60 Hz) and
//!   low-passed (11.5 kHz); at intensity 1 tracks are calibrated to ≈ -23 LUFS integrated.
//! * [`MusicPlayer::status`] gives bar/beat/tempo for beat-synced visuals.

pub mod analysis;
pub mod notation;
pub mod player;
pub mod song;
pub mod synth;

pub use player::{AdaptPhase, DebugLog, MusicCommand, MusicPlayer, MusicStatus, OUT_STEMS, Style};
pub use song::{MusicError, MusicLibrary, RemixStyle, Song, Stem, StingerKind, TrackId, TrackInfo};

/// `assets/music` inside this repository (for tools and tests).
pub fn default_music_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/music")
}
