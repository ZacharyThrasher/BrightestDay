//! Song files (`assets/music/*.ron`), the shared instrument bank (`instruments.ron`), the stinger
//! bank (`stingers.ron`) and their compiled, render-ready form ([`MusicLibrary`]).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::notation::{self, Note, PartCtx, WHOLE};
use crate::synth::{Instrument, Wave};

/// Maximum channels per song (a deck pre-allocates this many).
pub const MAX_CHANNELS: usize = 8;

/// The three adaptive layers every track is split into (+ the stinger bus at the output).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
pub enum Stem {
    /// Bass + harmony: always present.
    #[default]
    Base,
    /// Drums + arps: comes in at combat intensity.
    Drive,
    /// Melody: comes in above the exploration threshold.
    Lead,
}

impl Stem {
    pub const ALL: [Stem; 3] = [Stem::Base, Stem::Drive, Stem::Lead];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Stem::Base => "base",
            Stem::Drive => "drive",
            Stem::Lead => "lead",
        }
    }
}

// ------------------------------------------------------------------------------------------------
// File schema
// ------------------------------------------------------------------------------------------------

/// Instrument definition as authored (every field optional; see [`Instrument`] for meaning).
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InstrumentDef {
    pub wave: Wave,
    pub duty: f32,
    pub duty_end: Option<f32>,
    pub duty_time: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub vol: f32,
    pub gate: f32,
    pub vib_depth: f32,
    pub vib_rate: f32,
    pub vib_delay: f32,
    pub arp: Vec<i8>,
    pub arp_rate: f32,
    pub pitch_drop: f32,
    pub pitch_time: f32,
    pub glide: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub fm_index_end: Option<f32>,
    pub fm_decay: f32,
    pub fm_detune: f32,
    pub fm_feedback: f32,
    pub lp_hz: f32,
    pub lp_env: f32,
    pub lp_time: f32,
}

impl Default for InstrumentDef {
    fn default() -> Self {
        let d = Instrument::default();
        Self {
            wave: d.wave,
            duty: d.duty,
            duty_end: None,
            duty_time: d.duty_time,
            attack: d.attack,
            decay: d.decay,
            sustain: d.sustain,
            release: d.release,
            vol: d.vol,
            gate: d.gate,
            vib_depth: d.vib_depth,
            vib_rate: d.vib_rate,
            vib_delay: d.vib_delay,
            arp: Vec::new(),
            arp_rate: d.arp_rate,
            pitch_drop: d.pitch_drop,
            pitch_time: d.pitch_time,
            glide: d.glide,
            fm_ratio: d.fm_ratio,
            fm_index: d.fm_index,
            fm_index_end: None,
            fm_decay: d.fm_decay,
            fm_detune: d.fm_detune,
            fm_feedback: d.fm_feedback,
            lp_hz: d.lp_hz,
            lp_env: d.lp_env,
            lp_time: d.lp_time,
        }
    }
}

impl InstrumentDef {
    pub fn compile(&self) -> Result<Instrument, String> {
        if self.arp.len() > 8 {
            return Err("arp macros hold at most 8 steps".into());
        }
        let mut arp = [0i8; 8];
        for (d, s) in arp.iter_mut().zip(&self.arp) {
            *d = *s;
        }
        let finite = [self.duty, self.attack, self.decay, self.sustain, self.release, self.vol, self.fm_index, self.fm_ratio];
        if finite.iter().any(|x| !x.is_finite()) {
            return Err("non-finite instrument parameter".into());
        }
        Ok(Instrument {
            wave: self.wave,
            duty: self.duty,
            duty_end: self.duty_end.unwrap_or(self.duty),
            duty_time: self.duty_time,
            attack: self.attack.max(0.0),
            decay: self.decay.max(0.0),
            sustain: self.sustain,
            release: self.release.max(0.002),
            vol: self.vol.max(0.0),
            gate: self.gate.clamp(0.05, 1.0),
            vib_depth: self.vib_depth,
            vib_rate: self.vib_rate,
            vib_delay: self.vib_delay,
            arp,
            arp_len: self.arp.len() as u8,
            arp_rate: self.arp_rate,
            pitch_drop: self.pitch_drop,
            pitch_time: self.pitch_time,
            glide: self.glide,
            fm_ratio: self.fm_ratio.max(0.01),
            fm_index: self.fm_index.max(0.0),
            fm_index_end: self.fm_index_end.unwrap_or(self.fm_index).max(0.0),
            fm_decay: self.fm_decay,
            fm_detune: self.fm_detune,
            fm_feedback: self.fm_feedback,
            lp_hz: self.lp_hz.max(0.0),
            lp_env: self.lp_env,
            lp_time: self.lp_time,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EchoDef {
    /// Delay in ticks (96 per quarter note), tempo-synced.
    pub ticks: u32,
    #[serde(default = "default_echo_vol")]
    pub vol: f32,
    /// Pan of the echo (default: mirrored from the channel).
    #[serde(default)]
    pub pan: Option<f32>,
}

fn default_echo_vol() -> f32 {
    0.35
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelDef {
    pub name: String,
    #[serde(default)]
    pub inst: String,
    #[serde(default)]
    pub stem: Stem,
    #[serde(default)]
    pub pan: f32,
    #[serde(default = "one")]
    pub vol: f32,
    #[serde(default)]
    pub drums: bool,
    #[serde(default)]
    pub echo: Option<EchoDef>,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionDef {
    pub name: String,
    pub bars: u32,
    #[serde(default)]
    pub bpm: Option<f32>,
    /// Linear accelerando to this tempo across the section.
    #[serde(default)]
    pub accel_to: Option<f32>,
    /// Key-signature override (e.g. a modulation).
    #[serde(default)]
    pub key: Option<String>,
    /// Semitone transpose of every tonal part in this section.
    #[serde(default)]
    pub transpose: i32,
    pub parts: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct LayerDef {
    /// Intensity at which the lead stem enters.
    pub lead: Option<f32>,
    /// Intensity at which the drive stem enters.
    pub drive: Option<f32>,
    /// Base-stem gain at intensity 0 (rises to 1 at intensity 1).
    pub base_floor: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SongFile {
    pub title: String,
    #[serde(default)]
    pub composer: String,
    #[serde(default)]
    pub work: String,
    /// Provenance of the notes (public-domain source).
    #[serde(default)]
    pub source: String,
    /// Where the game uses it.
    #[serde(default)]
    pub role: String,
    pub key: String,
    #[serde(default = "four_four")]
    pub time: (u32, u32),
    pub bpm: f32,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub layers: LayerDef,
    #[serde(default)]
    pub instruments: BTreeMap<String, InstrumentDef>,
    pub channels: Vec<ChannelDef>,
    #[serde(default)]
    pub patterns: BTreeMap<String, String>,
    pub sections: Vec<SectionDef>,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub loop_to: String,
    #[serde(default)]
    pub ending: Option<String>,
}

fn four_four() -> (u32, u32) {
    (4, 4)
}

// ------------------------------------------------------------------------------------------------
// Compiled form
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EchoSpec {
    pub ticks: u32,
    pub vol: f32,
    pub pan: f32,
}

#[derive(Clone, Debug)]
pub struct ChannelSpec {
    pub name: String,
    pub drums: bool,
    pub stem: Stem,
    pub pan: f32,
    pub vol: f32,
    pub inst: u8,
    pub echo: Option<EchoSpec>,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub len: u32,
    pub bpm: f32,
    pub bpm_end: f32,
    /// Notes per channel (index = channel index).
    pub parts: Vec<Vec<Note>>,
}

impl Section {
    pub fn bpm_at(&self, tick: u32) -> f32 {
        if self.bpm_end == self.bpm || self.len == 0 {
            self.bpm
        } else {
            self.bpm + (self.bpm_end - self.bpm) * (tick as f32 / self.len as f32)
        }
    }
}

/// Resolved intensity layering of a track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layers {
    pub lead: f32,
    pub drive: f32,
    pub base_floor: f32,
}

impl Layers {
    /// Target gain of each stem ([base, drive, lead]) at an intensity.
    pub fn gains(&self, intensity: f32) -> [f32; 3] {
        let i = intensity.clamp(0.0, 1.0);
        let base = self.base_floor + (1.0 - self.base_floor) * i;
        let drive = if i + 1e-6 >= self.drive {
            let x = if self.drive < 1.0 { (i - self.drive) / (1.0 - self.drive) } else { 1.0 };
            0.8 + 0.2 * x.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let lead = if i + 1e-6 >= self.lead { 1.0 } else { 0.0 };
        [base, drive, lead]
    }
}

/// Human-facing metadata of a track.
#[derive(Clone, Debug, Default)]
pub struct TrackInfo {
    pub name: String,
    pub title: String,
    pub composer: String,
    pub work: String,
    pub source: String,
    pub role: String,
    pub key: String,
}

/// A compiled song, ready for the sequencer.
#[derive(Clone, Debug)]
pub struct Song {
    pub info: TrackInfo,
    pub bar_ticks: u32,
    pub beat_ticks: u32,
    pub bpm: f32,
    pub gain: f32,
    pub tonic: i32,
    pub minor: bool,
    pub layers: Layers,
    pub channels: Vec<ChannelSpec>,
    pub instruments: Vec<Instrument>,
    pub instrument_names: Vec<String>,
    pub sections: Vec<Section>,
    pub order: Vec<usize>,
    pub loop_idx: usize,
    pub ending: Option<usize>,
}

impl Song {
    pub fn section_index(&self, name: &str) -> Option<usize> {
        self.sections.iter().position(|s| s.name == name)
    }

    /// Ticks of the intro (order entries before the loop point) and of one loop pass.
    pub fn structure_ticks(&self) -> (u64, u64) {
        let len = |i: &usize| self.sections.get(*i).map_or(0, |s| u64::from(s.len));
        let intro = self.order.iter().take(self.loop_idx).map(len).sum();
        let body = self.order.iter().skip(self.loop_idx).map(len).sum();
        (intro, body)
    }
}

fn beat_ticks(time: (u32, u32)) -> u32 {
    let (num, den) = time;
    let unit = WHOLE / den.max(1);
    if den == 8 && num % 3 == 0 && num > 3 { unit * 3 } else { unit }
}

pub fn compile_song(name: &str, file: &SongFile, bank: &BTreeMap<String, InstrumentDef>, require_order: bool) -> Result<Song, String> {
    let (num, den) = file.time;
    if num == 0 || !matches!(den, 1 | 2 | 4 | 8 | 16) {
        return Err(format!("bad time signature {num}/{den}"));
    }
    let bar_ticks = num * (WHOLE / den);
    if !(20.0..=400.0).contains(&file.bpm) {
        return Err(format!("bpm {} out of range", file.bpm));
    }
    // Instruments: shared bank + song-local (local wins).
    let mut merged: BTreeMap<&str, &InstrumentDef> = bank.iter().map(|(k, v)| (k.as_str(), v)).collect();
    for (k, v) in &file.instruments {
        merged.insert(k.as_str(), v);
    }
    if merged.len() > 250 {
        return Err("too many instruments".into());
    }
    let mut instrument_names = Vec::new();
    let mut instruments = Vec::new();
    for (k, v) in &merged {
        instruments.push(v.compile().map_err(|e| format!("instrument {k}: {e}"))?);
        instrument_names.push((*k).to_string());
    }
    let inst_index = |n: &str| instrument_names.iter().position(|x| x == n).map(|i| i as u8);

    if file.channels.is_empty() || file.channels.len() > MAX_CHANNELS {
        return Err(format!("songs need 1..={MAX_CHANNELS} channels"));
    }
    let mut channels = Vec::new();
    for c in &file.channels {
        if channels.iter().any(|x: &ChannelSpec| x.name == c.name) {
            return Err(format!("duplicate channel {}", c.name));
        }
        let inst =
            if c.drums { 0 } else { inst_index(&c.inst).ok_or_else(|| format!("channel {}: unknown instrument '{}'", c.name, c.inst))? };
        let pan = c.pan.clamp(-1.0, 1.0);
        channels.push(ChannelSpec {
            name: c.name.clone(),
            drums: c.drums,
            stem: c.stem,
            pan,
            vol: c.vol.clamp(0.0, 4.0),
            inst,
            echo: c.echo.as_ref().map(|e| EchoSpec {
                ticks: e.ticks.max(1),
                vol: e.vol.clamp(0.0, 1.0),
                pan: e.pan.unwrap_or(-pan * 0.8 - if pan == 0.0 { 0.4 } else { 0.0 }).clamp(-1.0, 1.0),
            }),
        });
    }

    let (song_sig, tonic, minor) = notation::key_signature(&file.key)?;
    let mut sections = Vec::new();
    for s in &file.sections {
        if sections.iter().any(|x: &Section| x.name == s.name) {
            return Err(format!("duplicate section {}", s.name));
        }
        if s.bars == 0 || s.bars > 512 {
            return Err(format!("section {}: bars must be 1..512", s.name));
        }
        let key_sig = match &s.key {
            Some(k) => notation::key_signature(k).map_err(|e| format!("section {}: {e}", s.name))?.0,
            None => song_sig,
        };
        let len = s.bars * bar_ticks;
        let mut parts = vec![Vec::new(); channels.len()];
        for (ch_name, src) in &s.parts {
            let ci = channels
                .iter()
                .position(|c| &c.name == ch_name)
                .ok_or_else(|| format!("section {}: unknown channel '{ch_name}'", s.name))?;
            let drums = channels.get(ci).is_some_and(|c| c.drums);
            let ctx = PartCtx {
                patterns: &file.patterns,
                key_sig,
                drums,
                bar_ticks,
                transpose: if drums { 0 } else { s.transpose },
                inst_index: &inst_index,
            };
            let (notes, total) = notation::parse_part(src, &ctx).map_err(|e| format!("section {} / {ch_name}: {e}", s.name))?;
            if total != len {
                let diff = i64::from(total) - i64::from(len);
                return Err(format!(
                    "section {} / {ch_name}: part is {total} ticks, section is {len} ({} bars); off by {diff} ticks ({:+.3} bars)",
                    s.name,
                    s.bars,
                    diff as f64 / f64::from(bar_ticks)
                ));
            }
            if let Some(p) = parts.get_mut(ci) {
                *p = notes;
            }
        }
        let bpm = s.bpm.unwrap_or(file.bpm);
        let bpm_end = s.accel_to.unwrap_or(bpm);
        if !(20.0..=400.0).contains(&bpm) || !(20.0..=400.0).contains(&bpm_end) {
            return Err(format!("section {}: bpm out of range", s.name));
        }
        sections.push(Section { name: s.name.clone(), len, bpm, bpm_end, parts });
    }
    let find = |n: &str| sections.iter().position(|s| s.name == n);
    let mut order = Vec::new();
    for o in &file.order {
        order.push(find(o).ok_or_else(|| format!("order: unknown section '{o}'"))?);
    }
    let loop_idx = if require_order {
        if order.is_empty() {
            return Err("order is empty".into());
        }
        let li = find(&file.loop_to).ok_or_else(|| format!("loop_to: unknown section '{}'", file.loop_to))?;
        order.iter().position(|&x| x == li).ok_or("loop_to section is not in the order list")?
    } else {
        0
    };
    let ending = match &file.ending {
        Some(e) => Some(find(e).ok_or_else(|| format!("ending: unknown section '{e}'"))?),
        None => None,
    };
    let l = &file.layers;
    Ok(Song {
        info: TrackInfo {
            name: name.to_string(),
            title: file.title.clone(),
            composer: file.composer.clone(),
            work: file.work.clone(),
            source: file.source.clone(),
            role: file.role.clone(),
            key: file.key.clone(),
        },
        bar_ticks,
        beat_ticks: beat_ticks(file.time),
        bpm: file.bpm,
        gain: 10f32.powf(file.gain_db / 20.0),
        tonic,
        minor,
        layers: Layers {
            lead: l.lead.unwrap_or(0.15).clamp(0.0, 1.0),
            drive: l.drive.unwrap_or(0.55).clamp(0.0, 1.0),
            base_floor: l.base_floor.unwrap_or(0.75).clamp(0.0, 1.0),
        },
        channels,
        instruments,
        instrument_names,
        sections,
        order,
        loop_idx,
        ending,
    })
}

// ------------------------------------------------------------------------------------------------
// Library
// ------------------------------------------------------------------------------------------------

/// Index of a track in the [`MusicLibrary`] (stable for the library's lifetime; sorted by name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrackId(pub u16);

/// Musical punctuation the game can fire; each lands quantised to the beat or bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StingerKind {
    /// A KO (Beethoven 5 "fate" motif). Next beat, overlays the music.
    Ko,
    /// A KO streak step (rising chip flourish). Next beat, overlays.
    Streak,
    /// Match point (Dies irae). Next bar; the track holds and resumes after it.
    MatchPoint,
    /// Last player standing (Bach, Toccata in D minor). Next bar; holds the track.
    LastStanding,
    /// Victory: the track's own ending cadence (or the fanfare). Next bar; music ends.
    Victory,
    /// Defeat jingle. Next bar; music ends.
    Defeat,
}

impl StingerKind {
    pub const ALL: [StingerKind; 6] = [
        StingerKind::Ko,
        StingerKind::Streak,
        StingerKind::MatchPoint,
        StingerKind::LastStanding,
        StingerKind::Victory,
        StingerKind::Defeat,
    ];
    pub fn name(self) -> &'static str {
        match self {
            StingerKind::Ko => "ko",
            StingerKind::Streak => "streak",
            StingerKind::MatchPoint => "match_point",
            StingerKind::LastStanding => "last_standing",
            StingerKind::Victory => "victory",
            StingerKind::Defeat => "defeat",
        }
    }
    pub fn index(self) -> usize {
        self as usize
    }
    /// Lands on the next bar line (true) or the next beat (false).
    pub fn on_bar(self) -> bool {
        !matches!(self, StingerKind::Ko | StingerKind::Streak)
    }
    /// Pauses the track at the bar line and resumes it afterwards.
    pub fn holds_track(self) -> bool {
        matches!(self, StingerKind::MatchPoint | StingerKind::LastStanding)
    }
    /// Ends the music.
    pub fn ends_track(self) -> bool {
        matches!(self, StingerKind::Victory | StingerKind::Defeat)
    }
}

/// Stingers: one song whose sections are named after [`StingerKind::name`].
#[derive(Clone, Debug)]
pub struct StingerBank {
    pub song: Song,
    pub sections: [Option<usize>; 6],
    /// (tonic, minor) each stinger is written in (from the section's `key`).
    pub keys: [(i32, bool); 6],
}

impl StingerBank {
    /// Semitone shift that puts stinger `kind` into the relative key of a track.
    pub fn transpose_for(&self, kind: StingerKind, track_tonic: i32, track_minor: bool) -> i32 {
        let (st, sm) = self.keys.get(kind.index()).copied().unwrap_or((0, false));
        let target = match (sm, track_minor) {
            (true, false) => track_tonic - 3,
            (false, true) => track_tonic + 3,
            _ => track_tonic,
        };
        let d = (target - st).rem_euclid(12);
        if d > 6 { d - 12 } else { d }
    }
}

#[derive(Debug)]
pub struct MusicError(pub String);

impl std::fmt::Display for MusicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MusicError {}

fn ron_options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

/// Every compiled track plus the stinger bank. Build once (off the audio thread), share via `Arc`.
#[derive(Clone, Debug)]
pub struct MusicLibrary {
    pub tracks: Vec<Song>,
    pub stingers: StingerBank,
}

impl MusicLibrary {
    /// Load `instruments.ron`, `stingers.ron` and every other `*.ron` (one track each, named after
    /// the file stem) from a directory.
    pub fn load_dir(dir: &Path) -> Result<Self, MusicError> {
        let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| MusicError(format!("{}: {e}", p.display())));
        let instruments = read(&dir.join("instruments.ron"))?;
        let stingers = read(&dir.join("stingers.ron"))?;
        let mut tracks = Vec::new();
        let entries = std::fs::read_dir(dir).map_err(|e| MusicError(format!("{}: {e}", dir.display())))?;
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("ron") {
                continue;
            }
            let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
            if stem == "instruments" || stem == "stingers" {
                continue;
            }
            tracks.push((stem, read(&p)?));
        }
        let refs: Vec<(&str, &str)> = tracks.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        Self::from_sources(&instruments, &stingers, &refs)
    }

    /// Build from in-memory sources (`(track name, song RON)`).
    pub fn from_sources(instruments: &str, stingers: &str, tracks: &[(&str, &str)]) -> Result<Self, MusicError> {
        let opts = ron_options();
        let bank: BTreeMap<String, InstrumentDef> = opts.from_str(instruments).map_err(|e| MusicError(format!("instruments.ron: {e}")))?;
        let sfile: SongFile = opts.from_str(stingers).map_err(|e| MusicError(format!("stingers.ron: {e}")))?;
        let ssong = compile_song("stingers", &sfile, &bank, false).map_err(|e| MusicError(format!("stingers.ron: {e}")))?;
        let mut sections = [None; 6];
        let mut keys = [(0, true); 6];
        for k in StingerKind::ALL {
            let idx = ssong.section_index(k.name());
            if let (Some(slot), Some(i)) = (sections.get_mut(k.index()), idx) {
                *slot = Some(i);
            }
            if let Some(def) = sfile.sections.iter().find(|s| s.name == k.name()) {
                let key = def.key.as_deref().unwrap_or(&sfile.key);
                let (_, t, m) = notation::key_signature(key).map_err(|e| MusicError(format!("stingers.ron {}: {e}", k.name())))?;
                if let Some(slot) = keys.get_mut(k.index()) {
                    *slot = (t, m);
                }
            }
        }
        let mut out = Vec::new();
        let mut sorted: Vec<&(&str, &str)> = tracks.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (name, text) in sorted {
            let f: SongFile = opts.from_str(text).map_err(|e| MusicError(format!("{name}.ron: {e}")))?;
            out.push(compile_song(name, &f, &bank, true).map_err(|e| MusicError(format!("{name}.ron: {e}")))?);
        }
        if out.len() > usize::from(u16::MAX) {
            return Err(MusicError("too many tracks".into()));
        }
        Ok(Self { tracks: out, stingers: StingerBank { song: ssong, sections, keys } })
    }

    pub fn track_id(&self, name: &str) -> Option<TrackId> {
        self.tracks.iter().position(|s| s.info.name == name).map(|i| TrackId(i as u16))
    }

    pub fn track(&self, id: TrackId) -> Option<&Song> {
        self.tracks.get(usize::from(id.0))
    }

    pub fn track_names(&self) -> impl Iterator<Item = &str> {
        self.tracks.iter().map(|s| s.info.name.as_str())
    }
}
