//! The sound bank: every file of `assets/sfx/manifest.ron`, decoded once at load into shared PCM,
//! plus a precomputed short-term loudness envelope per file (the HDR window and the voice limiter
//! read it on the audio thread instead of metering live signals).
//!
//! The bank is immutable once built and shared with the audio thread as an `Arc<SoundBank>`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use crate::dsp::Biquad;

/// Frames per loudness-envelope step (10 ms at 48 kHz).
pub const ENV_HOP: usize = 480;
/// Envelope window in hops (50 ms): short-term enough to follow transients, long enough to be a
/// loudness rather than a peak.
const ENV_WIN: usize = 5;

/// Index of a sound in the bank (stable for a given manifest: sounds are sorted by name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SoundId(pub u16);

/// One decoded file.
#[derive(Clone, Debug)]
pub struct Pcm {
    pub channels: u8,
    pub rate: u32,
    /// Interleaved 16-bit samples (half the memory of f32; scaled by [`PCM_SCALE`] when read).
    pub data: Arc<[i16]>,
    pub frames: usize,
    /// K-weighted mean-square loudness (≈ LUFS) per `ENV_HOP` frames, 50 ms window.
    pub env_db: Arc<[f32]>,
    /// Loudest envelope value.
    pub max_db: f32,
}

/// i16 → f32 scale.
pub const PCM_SCALE: f32 = 1.0 / 32768.0;

impl Pcm {
    /// Build from interleaved float samples (quantised to 16 bit; computes the loudness envelope).
    pub fn new(channels: u8, rate: u32, data: Vec<f32>, looping: bool) -> Self {
        let q: Vec<i16> = data.iter().map(|&x| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16).collect();
        Self::from_i16(channels, rate, q, looping)
    }
    /// Build from interleaved 16-bit samples.
    pub fn from_i16(channels: u8, rate: u32, data: Vec<i16>, looping: bool) -> Self {
        let ch = channels.max(1) as usize;
        let frames = data.len() / ch;
        let env = loudness_envelope(&data, ch, looping);
        let max_db = env.iter().copied().fold(-120.0f32, f32::max);
        Self { channels: ch as u8, rate, data: data.into(), frames, env_db: env.into(), max_db }
    }
    /// Sample `i` (interleaved index) as f32.
    #[inline]
    pub fn sample(&self, i: usize) -> f32 {
        self.data.get(i).map_or(0.0, |&s| s as f32 * PCM_SCALE)
    }
    /// Loudness at frame `f` (looping wraps).
    #[inline]
    pub fn loudness_at(&self, f: usize) -> f32 {
        let n = self.env_db.len();
        if n == 0 {
            return -120.0;
        }
        let i = f / ENV_HOP;
        if i < n { self.env_db[i] } else { self.env_db[i % n] }
    }
}

/// A named sound: its variations (or ordered sequence steps) and metadata.
#[derive(Clone, Debug)]
pub struct SoundData {
    pub name: String,
    pub category: String,
    pub looping: bool,
    /// `files` are ordered steps (KO streak), not random variations.
    pub sequence: bool,
    pub stereo: bool,
    /// Manifest loudness target (LUFS) — informational.
    pub lufs: f32,
    /// Manifest linear trim.
    pub volume: f32,
    pub pitch: (f32, f32),
    pub vars: Vec<Pcm>,
}

#[derive(Debug)]
pub enum BankError {
    Io(PathBuf, String),
    Parse(String),
    Decode(PathBuf, String),
}

impl std::fmt::Display for BankError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BankError::Io(p, e) => write!(f, "{}: {e}", p.display()),
            BankError::Parse(e) => write!(f, "manifest: {e}"),
            BankError::Decode(p, e) => write!(f, "decode {}: {e}", p.display()),
        }
    }
}

impl std::error::Error for BankError {}

#[derive(Deserialize)]
struct ManifestEntry {
    files: Vec<String>,
    #[serde(default)]
    category: String,
    #[serde(default = "one")]
    volume: f32,
    #[serde(default)]
    looping: bool,
    #[serde(default = "unit_pitch")]
    pitch: (f32, f32),
    #[serde(default)]
    lufs: f32,
    #[serde(default)]
    sequence: bool,
    #[serde(default)]
    stereo: bool,
}

fn one() -> f32 {
    1.0
}
fn unit_pitch() -> (f32, f32) {
    (1.0, 1.0)
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default = "default_sr")]
    sample_rate: u32,
    sounds: BTreeMap<String, ManifestEntry>,
}

fn default_sr() -> u32 {
    48000
}

/// All sounds, immutable after load.
#[derive(Debug, Default)]
pub struct SoundBank {
    pub sample_rate: u32,
    pub sounds: Vec<SoundData>,
    names: HashMap<String, SoundId>,
}

impl SoundBank {
    pub fn empty(sample_rate: u32) -> Self {
        Self { sample_rate, sounds: Vec::new(), names: HashMap::new() }
    }

    /// Load `<assets>/sfx/manifest.ron` (paths inside are relative to `<assets>`), decoding every
    /// file on worker threads. Missing/corrupt files are skipped with an entry in `warnings`.
    pub fn load(assets: &Path, warnings: &mut Vec<String>) -> Result<Self, BankError> {
        Self::load_manifest(assets, &assets.join("sfx").join("manifest.ron"), warnings)
    }

    /// Load a manifest whose file paths are relative to `root`.
    pub fn load_manifest(root: &Path, manifest: &Path, warnings: &mut Vec<String>) -> Result<Self, BankError> {
        let text = std::fs::read_to_string(manifest).map_err(|e| BankError::Io(manifest.to_path_buf(), e.to_string()))?;
        let m: Manifest = ron::from_str(&text).map_err(|e| BankError::Parse(e.to_string()))?;
        // Flatten (sound, file) jobs and decode in parallel.
        let jobs: Vec<(usize, usize, PathBuf, bool)> = m
            .sounds
            .values()
            .enumerate()
            .flat_map(|(si, e)| e.files.iter().enumerate().map(move |(fi, f)| (si, fi, root.join(f), e.looping)))
            .collect();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
        let chunk = jobs.len().div_ceil(threads).max(1);
        let mut decoded: Vec<(usize, usize, Result<Pcm, BankError>)> = Vec::with_capacity(jobs.len());
        std::thread::scope(|s| {
            let handles: Vec<_> = jobs
                .chunks(chunk)
                .map(|part| s.spawn(move || part.iter().map(|(si, fi, p, lp)| (*si, *fi, decode_ogg(p, *lp))).collect::<Vec<_>>()))
                .collect();
            for h in handles {
                if let Ok(v) = h.join() {
                    decoded.extend(v);
                }
            }
        });
        decoded.sort_by_key(|(si, fi, _)| (*si, *fi));
        let mut bank = SoundBank::empty(m.sample_rate);
        let mut per_sound: Vec<Vec<Pcm>> = (0..m.sounds.len()).map(|_| Vec::new()).collect();
        for (si, _, r) in decoded {
            match r {
                Ok(p) => per_sound[si].push(p),
                Err(e) => warnings.push(e.to_string()),
            }
        }
        for ((name, e), vars) in m.sounds.into_iter().zip(per_sound) {
            if vars.is_empty() {
                warnings.push(format!("sound {name}: no playable files"));
            }
            bank.push(SoundData {
                name,
                category: e.category,
                looping: e.looping,
                sequence: e.sequence,
                stereo: e.stereo,
                lufs: e.lufs,
                volume: e.volume,
                pitch: e.pitch,
                vars,
            });
        }
        Ok(bank)
    }

    /// Add a sound (tests / procedural sources). Returns its id.
    pub fn push(&mut self, s: SoundData) -> SoundId {
        let id = SoundId(self.sounds.len() as u16);
        self.names.insert(s.name.clone(), id);
        self.sounds.push(s);
        id
    }

    /// Convenience for tests: a mono sound from samples.
    pub fn push_mono(&mut self, name: &str, category: &str, samples: Vec<f32>, looping: bool) -> SoundId {
        let pcm = Pcm::new(1, self.sample_rate, samples, looping);
        self.push(SoundData {
            name: name.to_string(),
            category: category.to_string(),
            looping,
            sequence: false,
            stereo: false,
            lufs: pcm.max_db,
            volume: 1.0,
            pitch: (1.0, 1.0),
            vars: vec![pcm],
        })
    }

    pub fn id(&self, name: &str) -> Option<SoundId> {
        self.names.get(name).copied()
    }
    pub fn get(&self, id: SoundId) -> Option<&SoundData> {
        self.sounds.get(id.0 as usize)
    }
    pub fn len(&self) -> usize {
        self.sounds.len()
    }
    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }
    /// Total decoded PCM bytes.
    pub fn bytes(&self) -> usize {
        self.sounds.iter().flat_map(|s| s.vars.iter()).map(|p| p.data.len() * 2).sum()
    }
}

/// Decode an Ogg Vorbis file to interleaved f32.
pub fn decode_ogg(path: &Path, looping: bool) -> Result<Pcm, BankError> {
    let f = std::fs::File::open(path).map_err(|e| BankError::Io(path.to_path_buf(), e.to_string()))?;
    let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::BufReader::new(f))
        .map_err(|e| BankError::Decode(path.to_path_buf(), e.to_string()))?;
    let ch = r.ident_hdr.audio_channels.max(1);
    let rate = r.ident_hdr.audio_sample_rate;
    let mut data: Vec<i16> = Vec::new();
    loop {
        match r.read_dec_packet_itl() {
            Ok(Some(pk)) => data.extend_from_slice(&pk),
            Ok(None) => break,
            Err(e) => return Err(BankError::Decode(path.to_path_buf(), e.to_string())),
        }
    }
    if data.is_empty() {
        return Err(BankError::Decode(path.to_path_buf(), "empty".into()));
    }
    Ok(Pcm::from_i16(ch, rate, data, looping))
}

/// BS.1770 K-weighting (48 kHz coefficients) + 50 ms mean-square, one value per hop, in dB.
pub fn loudness_envelope(data: &[i16], ch: usize, looping: bool) -> Vec<f32> {
    let frames = data.len() / ch.max(1);
    let hops = frames.div_ceil(ENV_HOP).max(1);
    let mut ms = vec![0.0f64; hops];
    // Loops: run the filters over the tail first so the start is warmed up (circular).
    for c in 0..ch {
        let (mut k1, mut k2) = kweight_filters();
        if looping {
            for f in frames.saturating_sub(4800)..frames {
                k2.process(k1.process(data[f * ch + c] as f32 * PCM_SCALE));
            }
        }
        for f in 0..frames {
            let y = k2.process(k1.process(data[f * ch + c] as f32 * PCM_SCALE)) as f64;
            ms[f / ENV_HOP] += y * y;
        }
    }
    for m in ms.iter_mut() {
        *m /= ENV_HOP as f64;
    }
    let mut out = vec![-120.0f32; hops];
    for (i, o) in out.iter_mut().enumerate() {
        let mut acc = 0.0;
        for k in 0..ENV_WIN {
            // Window centred-ish on the hop (looks slightly ahead so a transient registers at once).
            let j = i as isize + k as isize - 1;
            let j = if looping {
                j.rem_euclid(hops as isize) as usize
            } else if j < 0 || j >= hops as isize {
                continue;
            } else {
                j as usize
            };
            acc += ms[j];
        }
        *o = (-0.691 + 10.0 * (acc / ENV_WIN as f64 + 1e-12).log10()) as f32;
    }
    out
}

/// The two BS.1770 pre-filters at 48 kHz.
pub fn kweight_filters() -> (Biquad, Biquad) {
    (
        Biquad::from_coefs([1.535_124_9, -2.691_696_2, 1.198_392_8], [1.0, -1.690_659_3, 0.732_480_8]),
        Biquad::from_coefs([1.0, -2.0, 1.0], [1.0, -1.990_047_5, 0.990_072_25]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_of_full_scale_sine_is_about_minus_3_lufs() {
        let sr = 48000.0;
        let x: Vec<f32> = (0..48000).map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr).sin()).collect();
        let q: Vec<i16> = x.iter().map(|&s| (s * 32767.0) as i16).collect();
        let env = loudness_envelope(&q, 1, false);
        let mid = env[50];
        assert!((mid - -3.0).abs() < 0.8, "1 kHz full-scale sine ≈ -3 LUFS, got {mid}");
    }
}
