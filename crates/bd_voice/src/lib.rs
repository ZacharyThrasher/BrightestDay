//! bd_voice — experimental push-to-talk speech input (for fun). See docs/DESIGN.md and README.md.
//!
//! Hold the voice key → mic audio is captured (with a pre-roll so the first syllable survives),
//! release → a short tail is captured, the clip is resampled to 16 kHz mono, silence-trimmed,
//! and transcribed by Whisper (whisper.cpp, CPU only) on a background thread. The cleaned-up
//! lowercase text is meant to be fed to the same `bd_lexicon` resolver as typed input.
//!
//! Everything is non-blocking for the game thread:
//! ```ignore
//! let voice = VoiceInput::new(VoiceConfig { initial_prompt: lexicon_hint, ..default() })?;
//! // every frame:
//! voice.set_push_to_talk(keys.pressed(KeyCode::KeyV));
//! while let Some(ev) = voice.poll() {
//!     match ev {
//!         VoiceEvent::Level(l) => ring_pulse = l,
//!         VoiceEvent::Final { text, .. } => cast(&text),
//!         _ => {}
//!     }
//! }
//! ```
//!
//! Without the `whisper` cargo feature this crate is a dependency-free stub:
//! [`VoiceInput::new`] returns [`VoiceError::Unavailable`] and the game simply hides voice mode.

pub mod dsp;
pub mod text;

#[cfg(feature = "whisper")]
mod engine;

use std::path::PathBuf;

/// `true` when the crate was built with the `whisper` feature (real implementation).
pub const ENABLED: bool = cfg!(feature = "whisper");

/// Default model location, relative to the game's working directory (fetched by
/// `tools/voice/fetch_model.ps1`).
pub const DEFAULT_MODEL_PATH: &str = "assets/models/ggml-base.en-q5_1.bin";

/// Configuration for [`VoiceInput::new`].
#[derive(Clone, Debug)]
pub struct VoiceConfig {
    /// ggml Whisper model file. English-only models (`*.en`) are strongly preferred.
    pub model_path: PathBuf,
    /// Biasing prompt: a short comma-separated list of likely words, e.g.
    /// `"Mjolnir, Pikachu, Godzilla, Charizard, hammer, shield."`. Keep it under ~100 words
    /// (whisper.cpp keeps at most 224 prompt tokens and each token costs decode time).
    pub initial_prompt: String,
    /// CPU threads for inference. 0 = auto (half the logical CPUs, clamped to 2..=8).
    pub threads: usize,
    /// Open the default input device. `false` = only [`VoiceInput::submit_clip`] works
    /// (tests/tools, or a game that captures audio itself).
    pub capture_mic: bool,
    /// Audio kept from *before* the key went down.
    pub pre_roll_ms: u32,
    /// Audio still captured *after* the key is released.
    pub tail_ms: u32,
    /// Clips whose detected speech is shorter than this are discarded.
    pub min_speech_ms: u32,
    /// Holding longer than this auto-releases (Whisper is tuned here for short commands).
    pub max_clip_ms: u32,
    /// Whisper encoder window in 20 ms frames (1500 = the model's native 30 s window). Smaller
    /// is much faster; it is raised automatically to fit the clip. Measured on base.en: 256
    /// (5.1 s) transcribes short commands as well as 1500 at ~1/6 the cost, while "just fit
    /// the clip" (~100–150) makes Whisper truncate and loop. 0 = 1500.
    pub audio_ctx: u16,
    /// ggml flash attention on CPU.
    pub flash_attn: bool,
    /// Run one dummy inference after loading so the first real cast is not slower.
    pub warm_up: bool,
    /// Let whisper.cpp print its (verbose) logs to stderr. Off = silenced.
    pub whisper_logs: bool,
    /// Energy VAD tuning.
    pub vad: dsp::VadParams,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from(DEFAULT_MODEL_PATH),
            initial_prompt: String::new(),
            threads: 0,
            capture_mic: true,
            pre_roll_ms: 150,
            tail_ms: 200,
            min_speech_ms: 250,
            max_clip_ms: 8_000,
            audio_ctx: 256,
            flash_attn: false,
            warm_up: true,
            whisper_logs: false,
            vad: dsp::VadParams::default(),
        }
    }
}

impl VoiceConfig {
    /// Thread count actually used (resolves `threads == 0`).
    pub fn resolved_threads(&self) -> usize {
        if self.threads > 0 {
            return self.threads;
        }
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        (n / 2).clamp(2, 8)
    }
}

/// Why a clip produced no transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardReason {
    /// Detected speech shorter than `min_speech_ms` (a tap, a click, a cough).
    TooShort,
    /// VAD found no speech, or Whisper returned nothing usable.
    NoSpeech,
}

/// Events drained with [`VoiceInput::poll`].
#[derive(Clone, Debug, PartialEq)]
pub enum VoiceEvent {
    /// The model finished loading (and warming up); voice casts are now fast.
    Ready,
    /// Push-to-talk went down and capture started.
    Listening,
    /// Smoothed mic level 0..1 while listening (~30 Hz), for the diegetic ring pulse.
    Level(f32),
    /// Clip captured; Whisper is running.
    Transcribing,
    /// Cleaned transcript (lowercase, no punctuation), latency in ms from key release to result
    /// (includes the capture tail) and Whisper's confidence 0..1 (mean token probability of the
    /// text tokens; low = likely misheard). The game only logs it (local intent log).
    Final { text: String, ms: u32, confidence: f32 },
    /// The clip was thrown away; nothing will be cast.
    Discarded(DiscardReason),
    /// Something went wrong (model missing, device lost, …). Voice may be unusable.
    Error(String),
}

/// Errors from [`VoiceInput::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceError {
    /// Built without the `whisper` feature.
    Unavailable,
    /// No default input device, or it could not be opened.
    Audio(String),
    /// Model file missing (loading itself happens in the background and reports via
    /// [`VoiceEvent::Error`]).
    Model(String),
}

impl std::fmt::Display for VoiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VoiceError::Unavailable => write!(f, "voice input not compiled in (bd_voice `whisper` feature off)"),
            VoiceError::Audio(e) => write!(f, "microphone error: {e}"),
            VoiceError::Model(e) => write!(f, "speech model error: {e}"),
        }
    }
}

impl std::error::Error for VoiceError {}

#[cfg(feature = "whisper")]
pub use engine::VoiceInput;

#[cfg(not(feature = "whisper"))]
pub use stub::VoiceInput;

#[cfg(not(feature = "whisper"))]
mod stub {
    use super::*;

    /// No-op stand-in used when the `whisper` feature is off.
    pub struct VoiceInput {
        _private: (),
    }

    impl VoiceInput {
        /// Always `Err(VoiceError::Unavailable)`.
        pub fn new(_config: VoiceConfig) -> Result<Self, VoiceError> {
            Err(VoiceError::Unavailable)
        }
        pub fn set_push_to_talk(&self, _held: bool) {}
        pub fn poll(&self) -> Option<VoiceEvent> {
            None
        }
        pub fn submit_clip(&self, _samples: Vec<f32>, _sample_rate: u32) {}
        pub fn is_ready(&self) -> bool {
            false
        }
        pub fn mic_info(&self) -> Option<(&str, u32)> {
            None
        }
        pub fn set_prompt(&self, _prompt: &str) {}
    }
}

#[allow(dead_code)]
fn _assert_send_sync() {
    fn check<T: Send + Sync>() {}
    check::<VoiceInput>(); // must be usable as a Bevy `Resource`
}
