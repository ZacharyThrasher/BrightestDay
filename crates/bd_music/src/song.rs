//! Song files (`assets/music/*.ron`), remix variants (`assets/music/remix/*.ron` + the shared
//! `remix/kit.ron`), the shared instrument bank (`instruments.ron`), the stinger bank
//! (`stingers.ron`) and their compiled, render-ready form ([`MusicLibrary`]).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::notation::{self, ChordAt, Note, PartCtx, WHOLE};
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

/// Head-nod genres a remix variant can be written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RemixStyle {
    Trap,
    Synthwave,
    House,
    DrumAndBass,
    LoFi,
    Phonk,
    ChipEdm,
}

impl RemixStyle {
    pub const ALL: [RemixStyle; 7] = [
        RemixStyle::Trap,
        RemixStyle::Synthwave,
        RemixStyle::House,
        RemixStyle::DrumAndBass,
        RemixStyle::LoFi,
        RemixStyle::Phonk,
        RemixStyle::ChipEdm,
    ];
    pub fn name(self) -> &'static str {
        match self {
            RemixStyle::Trap => "trap",
            RemixStyle::Synthwave => "synthwave",
            RemixStyle::House => "house",
            RemixStyle::DrumAndBass => "dnb",
            RemixStyle::LoFi => "lofi",
            RemixStyle::Phonk => "phonk",
            RemixStyle::ChipEdm => "chipedm",
        }
    }
    pub fn from_name(s: &str) -> Option<RemixStyle> {
        RemixStyle::ALL.iter().copied().find(|x| x.name() == s)
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
    pub porta: bool,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub fm_index_end: Option<f32>,
    pub fm_decay: f32,
    pub fm_detune: f32,
    pub fm_feedback: f32,
    pub lp_hz: f32,
    pub lp_env: f32,
    pub lp_time: f32,
    pub res: f32,
    pub svf: bool,
    pub drive: f32,
    pub unison: u8,
    pub detune: f32,
    pub spread: f32,
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
            porta: d.porta,
            fm_ratio: d.fm_ratio,
            fm_index: d.fm_index,
            fm_index_end: None,
            fm_decay: d.fm_decay,
            fm_detune: d.fm_detune,
            fm_feedback: d.fm_feedback,
            lp_hz: d.lp_hz,
            lp_env: d.lp_env,
            lp_time: d.lp_time,
            res: d.res,
            svf: d.svf,
            drive: d.drive,
            unison: d.unison,
            detune: d.detune,
            spread: d.spread,
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
        let finite = [
            self.duty,
            self.attack,
            self.decay,
            self.sustain,
            self.release,
            self.vol,
            self.fm_index,
            self.fm_ratio,
            self.res,
            self.drive,
            self.detune,
            self.spread,
        ];
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
            porta: self.porta,
            fm_ratio: self.fm_ratio.max(0.01),
            fm_index: self.fm_index.max(0.0),
            fm_index_end: self.fm_index_end.unwrap_or(self.fm_index).max(0.0),
            fm_decay: self.fm_decay,
            fm_detune: self.fm_detune,
            fm_feedback: self.fm_feedback,
            lp_hz: self.lp_hz.max(0.0),
            lp_env: self.lp_env,
            lp_time: self.lp_time,
            res: self.res.clamp(0.0, 1.0),
            svf: self.svf,
            drive: self.drive.clamp(0.0, 1.0),
            unison: self.unison.clamp(1, crate::synth::MAX_UNISON as u8),
            detune: self.detune.clamp(0.0, 200.0),
            spread: self.spread.clamp(0.0, 1.0),
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
    /// Default part (a template tiled over the section) for sections that don't write this
    /// channel: chord-degree templates for tonal channels (needs the section's `chords`), beats for
    /// drum channels.
    #[serde(default)]
    pub r#gen: Option<String>,
}

fn one() -> f32 {
    1.0
}

/// What an automation lane moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum AutoParam {
    Cutoff,
    Res,
}

/// An automation lane: a filter sweep across part (or all) of a section.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoDef {
    /// `base`, `drive`, `lead`, `all` (every stem) or a channel name.
    pub target: String,
    #[serde(default = "cutoff")]
    pub param: AutoParam,
    pub from: f32,
    pub to: f32,
    /// Start (bars into the section) and length (bars; default: to the end of the section).
    #[serde(default)]
    pub start: f32,
    #[serde(default)]
    pub bars: Option<f32>,
}

fn cutoff() -> AutoParam {
    AutoParam::Cutoff
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
    /// Semitone transpose of every tonal part (and the chords) in this section.
    #[serde(default)]
    pub transpose: i32,
    /// Chord progression (enables chord-degree parts and channel `gen` templates).
    #[serde(default)]
    pub chords: Option<String>,
    /// Channels whose parts are written in the base track's meter (`remeter_from`), stretched to
    /// this song's meter by doubling each bar's first beat.
    #[serde(default)]
    pub remeter: Vec<String>,
    /// Channels silent in this section (overrides `gen`).
    #[serde(default)]
    pub mute: Vec<String>,
    /// Swing override for this section.
    #[serde(default)]
    pub swing: Option<f32>,
    #[serde(default)]
    pub auto: Vec<AutoDef>,
    #[serde(default)]
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

/// Sidechain pump: kicks duck the listed stems.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PumpDef {
    pub depth_db: f32,
    pub release: f32,
    pub stems: Vec<Stem>,
}

impl Default for PumpDef {
    fn default() -> Self {
        Self { depth_db: 6.0, release: 0.18, stems: vec![Stem::Base, Stem::Lead] }
    }
}

/// Remix intensity behaviour: filtered/thinned when exploring, riser + build into a DROP (the
/// filter snaps open) on a bar line when intensity crosses into combat.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdaptiveDef {
    /// Intensity at which the build starts (default: the `drive` layer threshold).
    pub combat_at: Option<f32>,
    pub explore_cutoff: f32,
    pub explore_res: f32,
    /// Drive-stem gain while exploring (filtered drums instead of none).
    pub explore_drive: f32,
    pub build_bars: u32,
    pub build_cutoff: f32,
    pub build_res: f32,
    pub riser: bool,
    pub roll: bool,
    pub impact: bool,
    /// Filter the lead stem too while exploring.
    pub filter_lead: bool,
}

impl Default for AdaptiveDef {
    fn default() -> Self {
        Self {
            combat_at: None,
            explore_cutoff: 900.0,
            explore_res: 0.25,
            explore_drive: 0.55,
            build_bars: 2,
            build_cutoff: 5000.0,
            build_res: 0.55,
            riser: true,
            roll: true,
            impact: true,
            filter_lead: false,
        }
    }
}

/// Lo-fi colour.
#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct LofiDef {
    /// Bit depth of the crush (0 = off; 8..16 useful).
    pub bits: f32,
    /// Sample-and-hold factor (1 = off).
    pub downsample: u32,
    /// Vinyl crackle amount 0..1.
    pub crackle: f32,
    /// Tape wow depth (cents) and rate (Hz).
    pub wow: f32,
    pub wow_rate: f32,
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
    /// Key signature (remixes default to their base track's).
    #[serde(default)]
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
    // ---- remix variants ----
    /// The classic track this remixes (its patterns and song instruments are inherited).
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    /// Uses the sub band (808s): the mixer must lower its music high-pass (see README).
    #[serde(default)]
    pub sub_bass: Option<bool>,
    /// Suggested / own high-pass corner (default 60 Hz, 32 Hz with `sub_bass`).
    #[serde(default)]
    pub hpf_hz: Option<f32>,
    /// Swing amount 0..0.75 of `swing_unit` (16 = 16ths, 8 = 8ths).
    #[serde(default)]
    pub swing: f32,
    #[serde(default)]
    pub swing_unit: Option<u32>,
    #[serde(default)]
    pub pump: Option<PumpDef>,
    #[serde(default)]
    pub adaptive: Option<AdaptiveDef>,
    #[serde(default)]
    pub lofi: Option<LofiDef>,
    /// Meter of `remeter` parts (default: the base track's meter).
    #[serde(default)]
    pub remeter_from: Option<(u32, u32)>,
}

fn four_four() -> (u32, u32) {
    (4, 4)
}

/// `remix/kit.ron`: style patterns and instruments shared by every remix.
#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct KitFile {
    pub instruments: BTreeMap<String, InstrumentDef>,
    pub patterns: BTreeMap<String, String>,
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

/// Where an automation lane applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoTarget {
    Stem(usize),
    AllStems,
    Channel(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Auto {
    pub target: AutoTarget,
    pub param: AutoParam,
    pub from: f32,
    pub to: f32,
    pub start: u32,
    pub end: u32,
}

impl Auto {
    /// Value at `tick` (cutoff interpolates in log-frequency), or None outside the lane.
    pub fn value_at(&self, tick: u32) -> Option<f32> {
        if tick < self.start || tick >= self.end {
            return None;
        }
        let x = (tick - self.start) as f32 / (self.end - self.start).max(1) as f32;
        Some(match self.param {
            AutoParam::Cutoff => self.from.max(20.0) * (self.to.max(20.0) / self.from.max(20.0)).powf(x),
            AutoParam::Res => self.from + (self.to - self.from) * x,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub len: u32,
    pub bpm: f32,
    pub bpm_end: f32,
    /// Notes per channel (index = channel index).
    pub parts: Vec<Vec<Note>>,
    pub autos: Vec<Auto>,
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

/// Compiled sidechain pump.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pump {
    /// Linear gain reduction at full duck (0..1).
    pub depth: f32,
    pub release: f32,
    pub stems: [bool; 3],
}

/// Compiled adaptive (explore → build → drop) behaviour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adaptive {
    pub combat_at: f32,
    pub explore_cutoff: f32,
    pub explore_res: f32,
    pub explore_drive: f32,
    pub build_bars: u32,
    pub build_cutoff: f32,
    pub build_res: f32,
    pub riser: bool,
    pub roll: bool,
    pub impact: bool,
    pub filter_lead: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lofi {
    pub bits: f32,
    pub downsample: u32,
    pub crackle: f32,
    pub wow: f32,
    pub wow_rate: f32,
}

/// Remix metadata (classic tracks: everything off, 60 Hz high-pass).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VariantMeta {
    pub style: Option<RemixStyle>,
    pub sub_bass: bool,
    pub hpf_hz: f32,
    pub pump: Option<Pump>,
    pub adaptive: Option<Adaptive>,
    pub lofi: Option<Lofi>,
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
    /// Base track name for remixes.
    pub base: Option<String>,
    /// File path relative to the music directory.
    pub file: String,
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
    pub meta: VariantMeta,
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

    /// Is this a remix variant?
    pub fn is_remix(&self) -> bool {
        self.info.base.is_some()
    }
}

fn beat_ticks(time: (u32, u32)) -> u32 {
    let (num, den) = time;
    let unit = WHOLE / den.max(1);
    if den == 8 && num % 3 == 0 && num > 3 { unit * 3 } else { unit }
}

fn bar_ticks_of(time: (u32, u32)) -> Result<u32, String> {
    let (num, den) = time;
    if num == 0 || !matches!(den, 1 | 2 | 4 | 8 | 16) {
        return Err(format!("bad time signature {num}/{den}"));
    }
    Ok(num * (WHOLE / den))
}

pub fn compile_song(name: &str, file: &SongFile, bank: &BTreeMap<String, InstrumentDef>, require_order: bool) -> Result<Song, String> {
    let bar_ticks = bar_ticks_of(file.time)?;
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
    let src_bar = match file.remeter_from {
        Some(t) => bar_ticks_of(t)?,
        None => bar_ticks,
    };
    let swing_unit = match file.swing_unit.unwrap_or(16) {
        8 => PPQ_EIGHTH,
        16 => PPQ_SIXTEENTH,
        other => return Err(format!("swing_unit must be 8 or 16, not {other}")),
    };
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
        let chords: Option<Vec<ChordAt>> = match &s.chords {
            Some(src) => Some(
                notation::parse_chords(src, &file.patterns, bar_ticks, s.bars, s.transpose)
                    .map_err(|e| format!("section {} / chords: {e}", s.name))?,
            ),
            None => None,
        };
        for n in s.parts.keys().chain(s.remeter.iter()).chain(s.mute.iter()) {
            if !channels.iter().any(|c| &c.name == n) {
                return Err(format!("section {}: unknown channel '{n}'", s.name));
            }
        }
        let swing_amt = s.swing.unwrap_or(file.swing);
        let mut parts = vec![Vec::new(); channels.len()];
        for (ci, (ch, def)) in channels.iter().zip(&file.channels).enumerate() {
            if s.mute.contains(&ch.name) {
                continue;
            }
            let drums = ch.drums;
            let explicit = s.parts.get(&ch.name);
            let gen_src = match (&def.r#gen, explicit) {
                (Some(g), None) if drums || chords.is_some() => Some(g),
                _ => None,
            };
            let Some(src) = explicit.or(gen_src) else { continue };
            let remeter = explicit.is_some() && s.remeter.contains(&ch.name);
            let part_bar = if remeter { src_bar } else { bar_ticks };
            let ctx = PartCtx {
                patterns: &file.patterns,
                key_sig,
                drums,
                bar_ticks: part_bar,
                transpose: if drums { 0 } else { s.transpose },
                inst_index: &inst_index,
                chords: chords.as_deref(),
            };
            let what = format!("section {} / {}", s.name, ch.name);
            let (mut notes, total) = if explicit.is_none() {
                // Tile the template over the section.
                let (_, l) = notation::parse_part(src, &ctx).map_err(|e| format!("{what} (gen): {e}"))?;
                if l == 0 || !len.is_multiple_of(l) || len / l > 256 {
                    return Err(format!("{what}: gen template is {l} ticks, which does not tile {len}"));
                }
                let tiled = format!("[{src}]{}", len / l);
                notation::parse_part(&tiled, &ctx).map_err(|e| format!("{what} (gen): {e}"))?
            } else {
                notation::parse_part(src, &ctx).map_err(|e| format!("{what}: {e}"))?
            };
            let (total, expect) = if remeter {
                notation::remeter(&mut notes, src_bar, bar_ticks);
                (total, s.bars * src_bar)
            } else {
                (total, len)
            };
            if total != expect {
                let diff = i64::from(total) - i64::from(expect);
                return Err(format!(
                    "{what}: part is {total} ticks, section is {expect} ({} bars); off by {diff} ticks ({:+.3} bars)",
                    s.bars,
                    diff as f64 / f64::from(part_bar)
                ));
            }
            if swing_amt > 0.0 {
                notation::swing(&mut notes, swing_amt, swing_unit);
            }
            if let Some(p) = parts.get_mut(ci) {
                *p = notes;
            }
        }
        let mut autos = Vec::new();
        for a in &s.auto {
            let target = match a.target.as_str() {
                "base" => AutoTarget::Stem(0),
                "drive" => AutoTarget::Stem(1),
                "lead" => AutoTarget::Stem(2),
                "all" => AutoTarget::AllStems,
                n => AutoTarget::Channel(
                    channels.iter().position(|c| c.name == n).ok_or_else(|| format!("section {}: auto target '{n}'", s.name))?,
                ),
            };
            if !(a.from.is_finite() && a.to.is_finite()) {
                return Err(format!("section {}: non-finite automation", s.name));
            }
            let start = (a.start.max(0.0) * bar_ticks as f32).round() as u32;
            let end = match a.bars {
                Some(b) => start + (b.max(0.0) * bar_ticks as f32).round() as u32,
                None => len,
            }
            .min(len);
            autos.push(Auto { target, param: a.param, from: a.from, to: a.to, start, end });
        }
        let bpm = s.bpm.unwrap_or(file.bpm);
        let bpm_end = s.accel_to.unwrap_or(bpm);
        if !(20.0..=400.0).contains(&bpm) || !(20.0..=400.0).contains(&bpm_end) {
            return Err(format!("section {}: bpm out of range", s.name));
        }
        sections.push(Section { name: s.name.clone(), len, bpm, bpm_end, parts, autos });
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
    let layers = Layers {
        lead: l.lead.unwrap_or(0.15).clamp(0.0, 1.0),
        drive: l.drive.unwrap_or(0.55).clamp(0.0, 1.0),
        base_floor: l.base_floor.unwrap_or(0.75).clamp(0.0, 1.0),
    };
    let style = match &file.style {
        Some(s) => Some(RemixStyle::from_name(s).ok_or_else(|| format!("unknown style '{s}'"))?),
        None => None,
    };
    let sub_bass = file.sub_bass.unwrap_or(false);
    let meta = VariantMeta {
        style,
        sub_bass,
        hpf_hz: file.hpf_hz.unwrap_or(if sub_bass { 32.0 } else { 60.0 }).clamp(15.0, 200.0),
        pump: file.pump.as_ref().map(|p| {
            let mut stems = [false; 3];
            for s in &p.stems {
                stems[s.index()] = true;
            }
            Pump { depth: 1.0 - 10f32.powf(-p.depth_db.clamp(0.0, 24.0) / 20.0), release: p.release.clamp(0.02, 1.0), stems }
        }),
        adaptive: file.adaptive.as_ref().map(|a| Adaptive {
            combat_at: a.combat_at.unwrap_or(layers.drive).clamp(0.0, 1.0),
            explore_cutoff: a.explore_cutoff.clamp(80.0, 20_000.0),
            explore_res: a.explore_res.clamp(0.0, 0.95),
            explore_drive: a.explore_drive.clamp(0.0, 1.0),
            build_bars: a.build_bars.min(8),
            build_cutoff: a.build_cutoff.clamp(80.0, 20_000.0),
            build_res: a.build_res.clamp(0.0, 0.95),
            riser: a.riser,
            roll: a.roll,
            impact: a.impact,
            filter_lead: a.filter_lead,
        }),
        lofi: file.lofi.as_ref().map(|f| Lofi {
            bits: f.bits.clamp(0.0, 24.0),
            downsample: f.downsample.clamp(1, 16),
            crackle: f.crackle.clamp(0.0, 1.0),
            wow: f.wow.clamp(0.0, 100.0),
            wow_rate: f.wow_rate.clamp(0.0, 10.0),
        }),
    };
    Ok(Song {
        info: TrackInfo {
            name: name.to_string(),
            title: file.title.clone(),
            composer: file.composer.clone(),
            work: file.work.clone(),
            source: file.source.clone(),
            role: file.role.clone(),
            key: file.key.clone(),
            base: file.base.clone(),
            file: format!("{name}.ron"),
        },
        bar_ticks,
        beat_ticks: beat_ticks(file.time),
        bpm: file.bpm,
        gain: 10f32.powf(file.gain_db / 20.0),
        tonic,
        minor,
        layers,
        meta,
        channels,
        instruments,
        instrument_names,
        sections,
        order,
        loop_idx,
        ending,
    })
}

const PPQ_SIXTEENTH: u32 = WHOLE / 16;
const PPQ_EIGHTH: u32 = WHOLE / 8;

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

/// Every compiled track (classic chip arrangements and their remix variants) plus the stinger
/// bank. Build once (off the audio thread), share via `Arc`.
#[derive(Clone, Debug)]
pub struct MusicLibrary {
    pub tracks: Vec<Song>,
    pub stingers: StingerBank,
    /// Remix variants of each track (index = track index; empty for remixes themselves).
    pub variants: Vec<Vec<TrackId>>,
    /// The classic track a remix is based on (index = track index).
    pub base_of: Vec<Option<TrackId>>,
}

/// Sources for [`MusicLibrary::from_sources_with_remixes`].
pub struct Sources<'a> {
    pub instruments: &'a str,
    pub stingers: &'a str,
    pub tracks: &'a [(&'a str, &'a str)],
    pub kit: &'a str,
    pub remixes: &'a [(&'a str, &'a str)],
}

impl MusicLibrary {
    /// Load `instruments.ron`, `stingers.ron`, every other `*.ron` (one track each, named after
    /// the file stem), and `remix/kit.ron` + `remix/*.ron` (remix variants) from a directory.
    pub fn load_dir(dir: &Path) -> Result<Self, MusicError> {
        let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| MusicError(format!("{}: {e}", p.display())));
        let list = |d: &Path, skip: &[&str]| -> Result<Vec<(String, String)>, MusicError> {
            let mut out = Vec::new();
            let Ok(entries) = std::fs::read_dir(d) else { return Ok(out) };
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("ron") {
                    continue;
                }
                let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
                if skip.contains(&stem.as_str()) {
                    continue;
                }
                out.push((stem, read(&p)?));
            }
            Ok(out)
        };
        let instruments = read(&dir.join("instruments.ron"))?;
        let stingers = read(&dir.join("stingers.ron"))?;
        let tracks = list(dir, &["instruments", "stingers"])?;
        let rdir = dir.join("remix");
        let kit = if rdir.join("kit.ron").exists() { read(&rdir.join("kit.ron"))? } else { String::from("()") };
        let remixes = list(&rdir, &["kit"])?;
        let t: Vec<(&str, &str)> = tracks.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let r: Vec<(&str, &str)> = remixes.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        Self::from_sources_with_remixes(&Sources { instruments: &instruments, stingers: &stingers, tracks: &t, kit: &kit, remixes: &r })
    }

    /// Build from in-memory sources (`(track name, song RON)`), classic tracks only.
    pub fn from_sources(instruments: &str, stingers: &str, tracks: &[(&str, &str)]) -> Result<Self, MusicError> {
        Self::from_sources_with_remixes(&Sources { instruments, stingers, tracks, kit: "()", remixes: &[] })
    }

    pub fn from_sources_with_remixes(src: &Sources) -> Result<Self, MusicError> {
        let opts = ron_options();
        let bank: BTreeMap<String, InstrumentDef> =
            opts.from_str(src.instruments).map_err(|e| MusicError(format!("instruments.ron: {e}")))?;
        let sfile: SongFile = opts.from_str(src.stingers).map_err(|e| MusicError(format!("stingers.ron: {e}")))?;
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
        // Classic tracks.
        let mut files: BTreeMap<String, SongFile> = BTreeMap::new();
        for (name, text) in src.tracks {
            let f: SongFile = opts.from_str(text).map_err(|e| MusicError(format!("{name}.ron: {e}")))?;
            files.insert((*name).to_string(), f);
        }
        let mut compiled: Vec<Song> = Vec::new();
        for (name, f) in &files {
            compiled.push(compile_song(name, f, &bank, true).map_err(|e| MusicError(format!("{name}.ron: {e}")))?);
        }
        // Remix variants: kit < base track < remix for both patterns and instruments.
        let kit: KitFile = opts.from_str(src.kit).map_err(|e| MusicError(format!("remix/kit.ron: {e}")))?;
        let mut rbank = bank.clone();
        for (k, v) in &kit.instruments {
            rbank.insert(k.clone(), v.clone());
        }
        for (name, text) in src.remixes {
            let mut f: SongFile = opts.from_str(text).map_err(|e| MusicError(format!("remix/{name}.ron: {e}")))?;
            let base_name = f.base.clone().ok_or_else(|| MusicError(format!("remix/{name}.ron: missing `base`")))?;
            let base = files.get(&base_name).ok_or_else(|| MusicError(format!("remix/{name}.ron: unknown base '{base_name}'")))?;
            let mut pats = kit.patterns.clone();
            pats.extend(base.patterns.clone());
            pats.extend(std::mem::take(&mut f.patterns));
            f.patterns = pats;
            let mut insts = base.instruments.clone();
            insts.extend(std::mem::take(&mut f.instruments));
            f.instruments = insts;
            if f.key.is_empty() {
                f.key = base.key.clone();
            }
            if f.remeter_from.is_none() && base.time != f.time {
                f.remeter_from = Some(base.time);
            }
            if f.composer.is_empty() {
                f.composer = base.composer.clone();
            }
            if f.work.is_empty() {
                f.work = base.work.clone();
            }
            if f.role.is_empty() {
                f.role = base.role.clone();
            }
            let mut song = compile_song(name, &f, &rbank, true).map_err(|e| MusicError(format!("remix/{name}.ron: {e}")))?;
            song.info.file = format!("remix/{name}.ron");
            compiled.push(song);
        }
        compiled.sort_by(|a, b| a.info.name.cmp(&b.info.name));
        if compiled.len() > usize::from(u16::MAX) {
            return Err(MusicError("too many tracks".into()));
        }
        let find = |n: &str| compiled.iter().position(|s| s.info.name == n).map(|i| TrackId(i as u16));
        let base_of: Vec<Option<TrackId>> = compiled.iter().map(|s| s.info.base.as_deref().and_then(find)).collect();
        let mut variants = vec![Vec::new(); compiled.len()];
        for (i, b) in base_of.iter().enumerate() {
            if let Some(b) = b {
                variants[usize::from(b.0)].push(TrackId(i as u16));
            }
        }
        Ok(Self { tracks: compiled, stingers: StingerBank { song: ssong, sections, keys }, variants, base_of })
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

    /// Remix variants of a (classic) track.
    pub fn variants_of(&self, id: TrackId) -> &[TrackId] {
        self.variants.get(usize::from(id.0)).map_or(&[], |v| v.as_slice())
    }

    /// The classic track a remix is based on (itself for classic tracks).
    pub fn classic_of(&self, id: TrackId) -> TrackId {
        self.base_of.get(usize::from(id.0)).copied().flatten().unwrap_or(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_drive_degree_templates_and_remeter_and_swing() {
        let bank: BTreeMap<String, InstrumentDef> = [("b".to_string(), InstrumentDef::default())].into_iter().collect();
        let text = r#"(
            title: "t", key: "C", time: (4, 4), bpm: 120.0, remeter_from: (3, 4), swing: 0.5,
            channels: [
                (name: "bass", inst: "b", gen: "o2 u4 R4 F4 R4 O4 |"),
                (name: "mel", inst: "b"),
                (name: "d", drums: true, gen: "K8 H8 p3 H8 !H8 S4 ?S4 |"),
            ],
            sections: [(name: "a", bars: 2, chords: "C | G7/B |", remeter: ["mel"], parts: { "mel": "c4 e4 g4 | d2. |" })],
            order: ["a"], loop_to: "a",
        )"#;
        let f: SongFile = ron_options().from_str(text).unwrap();
        let s = compile_song("t", &f, &bank, true).unwrap();
        let bass: Vec<u8> = s.sections[0].parts[0].iter().map(|n| n.key).collect();
        // o2 u4: roots from E2 (40) upward: C -> C3 (48), G7/B -> bass B2 (47) / root G2 (43)
        assert_eq!(bass, vec![48, 55, 48, 60, 47, 50, 47, 55]);
        let mel = &s.sections[0].parts[1];
        assert_eq!((mel[0].tick, mel[0].len), (0, 192)); // first beat doubled
        assert_eq!((mel[1].tick, mel[1].len), (192, 96));
        assert_eq!((mel[3].tick, mel[3].len), (384, 384)); // dotted half fills the 4/4 bar
        let d = &s.sections[0].parts[2];
        assert_eq!(d[1].tick, 48); // 16th swing leaves 8ths alone
        assert_eq!(d[2].pitch, 3);
        assert!(d[3].vol > d[0].vol && d[5].vol < d[4].vol);
    }
}
