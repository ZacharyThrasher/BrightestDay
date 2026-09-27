//! The real-time player: sample-exact integer sequencer, two crossfading track decks + one stinger
//! deck, adaptive stem gains, bar/beat quantisation, and for remix variants: resonant stem
//! filters with automation lanes, the explore → build → DROP state machine, sidechain pump, fx
//! (risers / rolls / impacts) and lo-fi colour. Everything is pre-allocated in
//! [`MusicPlayer::new`]; [`MusicPlayer::render`] / [`MusicPlayer::command`] never allocate.

use std::sync::Arc;

use crate::notation::PPQ;
use crate::overlay::{OverlayId, Overlays};
use crate::song::{Adaptive, AutoParam, AutoTarget, Lofi, MAX_CHANNELS, MusicLibrary, Pump, RemixStyle, Song, StingerKind, TrackId};
use crate::synth::{CONTROL, Drum, DrumVoice, StemFilter, Svf, Voice, sin_turns};

/// Number of output stems: base, drive, lead, stinger.
pub const OUT_STEMS: usize = 4;
/// Longest echo delay (seconds).
const MAX_ECHO_SECS: f32 = 1.2;
/// Stem fade-in time when a layer enters on a bar line (the downbeat must land).
const FADE_IN_SECS: f32 = 0.03;
/// Deck fade when a track is cut on a bar line.
const CUT_FADE_SECS: f32 = 0.25;
/// "Filter fully open".
const OPEN_HZ: f32 = 21_000.0;
/// Width of supersaw side signals in the channel pan.
const SIDE_W: f32 = 0.7;

/// Which versions of the soundtrack play (a game setting: "Music: Classic chip / Remixes / Shuffle").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// The chip arrangements.
    #[default]
    Classic,
    /// Remix variants: the given genre when a track has one, otherwise its remixes in rotation
    /// (`None` = rotate through all of them). Tracks without remixes play classic.
    Remix(Option<RemixStyle>),
    /// Every `Play` picks at random (deterministic sequence) between the classic and its remixes.
    Shuffle,
}

/// A command for the player. `Copy`, so it can travel through any lock-free queue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MusicCommand {
    /// Switch to a track on the next bar line of the current one (immediately if silent). A
    /// classic track id is resolved through the current [`Style`]; a remix id plays that remix.
    Play { track: TrackId, skip_intro: bool },
    /// Stop on the next bar line (voices release naturally).
    Stop,
    /// 0 = calm (base only) … 0.15+ lead … 0.55+ drive (per-track thresholds). Applied on bar lines.
    /// Remixes: crossing into combat starts the riser/build and the DROP lands on a bar line.
    SetIntensity(f32),
    /// Beat/bar-quantised musical punctuation.
    Stinger(StingerKind),
    /// Tempo multiplier (sudden death / final stock), ramped over `ramp_secs`. Pitch is unaffected.
    SetTempo { multiplier: f32, ramp_secs: f32 },
    /// Choose classic / remix / shuffle. If a track is playing and resolves differently, the player
    /// switches to the new version on the next bar line.
    SetStyle(Style),
    /// Turn a named overlay layer ([`crate::overlay`], e.g. "drums") on / off: it locks to the
    /// playing track's 16ths and fades in / out.
    Overlay { id: OverlayId, on: bool },
}

/// Where a remix is in its explore → build → drop cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdaptPhase {
    /// Classic track (no adaptive filter).
    #[default]
    Off,
    /// Exploring: filtered / thinned.
    Calm,
    /// Riser + roll + filter sweep, the drop lands on a coming bar line.
    Build,
    /// After the drop: everything open.
    Combat,
}

/// Snapshot of the player for UI / beat-synced visuals / the mixer (cheap, `Copy`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicStatus {
    pub track: Option<TrackId>,
    pub playing: bool,
    /// Bar lines passed since the track started.
    pub bars: u32,
    /// Beat within the current bar (0-based).
    pub beat: u32,
    pub section: u16,
    /// Effective tempo (bpm × multiplier).
    pub bpm: f32,
    pub tempo_multiplier: f32,
    pub intensity: f32,
    /// Current stem gains [base, drive, lead].
    pub stem_gains: [f32; 3],
    pub stinger: Option<StingerKind>,
    /// Samples rendered so far.
    pub sample: u64,
    /// Times the current track wrapped back to its loop point.
    pub loops: u32,
    pub style: Style,
    pub phase: AdaptPhase,
    /// The playing variant uses the sub band (808s): lower the music bus high-pass to `hpf_hz`
    /// and sidechain the music sub under impacts / explosions.
    pub sub_bass: bool,
    pub hpf_hz: f32,
    /// Overlay layers switched on (bit `i` = [`OverlayId`] `i`).
    pub overlays: u8,
}

/// Fixed-size logs for tests and the offline report (no allocation).
#[derive(Clone, Copy, Debug)]
pub struct DebugLog {
    /// Absolute sample of each main-deck bar line (ring buffer, `bar_n` total written).
    pub bars: [u64; 256],
    pub bar_n: usize,
    /// Absolute sample of each stem-target change (ring, `change_n` total).
    pub target_changes: [u64; 64],
    pub change_n: usize,
    /// Sample where the current track first reached its loop point / last wrapped back to it.
    pub loop_start: Option<u64>,
    pub last_wrap: Option<u64>,
    /// Sample where the last stinger started.
    pub stinger_start: Option<u64>,
    pub beats: [u64; 64],
    pub beat_n: usize,
    /// Note-ons of the main deck in the bar that ended at `bars[i]` (same ring indexing).
    pub bar_notes: [u32; 256],
    cur_notes: u32,
    /// Samples where builds started and drops landed (rings of 16).
    pub builds: [u64; 16],
    pub build_n: usize,
    pub drops: [u64; 16],
    pub drop_n: usize,
}

impl Default for DebugLog {
    fn default() -> Self {
        Self {
            bars: [0; 256],
            bar_n: 0,
            target_changes: [0; 64],
            change_n: 0,
            loop_start: None,
            last_wrap: None,
            stinger_start: None,
            beats: [0; 64],
            beat_n: 0,
            bar_notes: [0; 256],
            cur_notes: 0,
            builds: [0; 16],
            build_n: 0,
            drops: [0; 16],
            drop_n: 0,
        }
    }
}

impl DebugLog {
    fn bar(&mut self, s: u64) {
        if let Some(slot) = self.bars.get_mut(self.bar_n % 256) {
            *slot = s;
        }
        if let Some(slot) = self.bar_notes.get_mut(self.bar_n % 256) {
            *slot = self.cur_notes;
        }
        self.cur_notes = 0;
        self.bar_n += 1;
    }
    fn beat(&mut self, s: u64) {
        if let Some(slot) = self.beats.get_mut(self.beat_n % 64) {
            *slot = s;
        }
        self.beat_n += 1;
    }
    fn change(&mut self, s: u64) {
        if let Some(slot) = self.target_changes.get_mut(self.change_n % 64) {
            *slot = s;
        }
        self.change_n += 1;
    }
    fn build(&mut self, s: u64) {
        if let Some(slot) = self.builds.get_mut(self.build_n % 16) {
            *slot = s;
        }
        self.build_n += 1;
    }
    fn drop_at(&mut self, s: u64) {
        if let Some(slot) = self.drops.get_mut(self.drop_n % 16) {
            *slot = s;
        }
        self.drop_n += 1;
    }
    /// Was `s` a logged bar line (among the retained ring entries)?
    pub fn is_bar(&self, s: u64) -> bool {
        let n = self.bar_n.min(256);
        self.bars.iter().take(n).any(|&b| b == s)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Ramp {
    cur: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    fn at(v: f32) -> Self {
        Self { cur: v, target: v, step: 0.0 }
    }
    fn set(&mut self, target: f32, samples: f32) {
        self.target = target;
        self.step = if samples <= 1.0 { target - self.cur } else { (target - self.cur) / samples };
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.cur != self.target {
            self.cur += self.step;
            if (self.step >= 0.0 && self.cur >= self.target) || (self.step <= 0.0 && self.cur <= self.target) {
                self.cur = self.target;
            }
        }
        self.cur
    }
    /// Advance `n` samples at once (control-rate use).
    fn skip(&mut self, n: usize) {
        if self.cur != self.target {
            self.cur += self.step * n as f32;
            if (self.step >= 0.0 && self.cur >= self.target) || (self.step <= 0.0 && self.cur <= self.target) {
                self.cur = self.target;
            }
        }
    }
}

/// Log-frequency cutoff sweep (control rate).
#[derive(Clone, Copy, Debug)]
struct Sweep {
    from: f32,
    to: f32,
    pos: f32,
    len: f32,
}

impl Sweep {
    fn at(v: f32) -> Self {
        Self { from: v, to: v, pos: 1.0, len: 1.0 }
    }
    fn value(&self) -> f32 {
        let x = (self.pos / self.len).min(1.0);
        self.from * (self.to / self.from).powf(x)
    }
    fn set(&mut self, to: f32, samples: f32) {
        self.from = self.value();
        self.to = to.max(20.0);
        self.pos = 0.0;
        self.len = samples.max(1.0);
    }
    fn advance(&mut self, n: usize) {
        self.pos = (self.pos + n as f32).min(self.len);
    }
}

struct Chan {
    drums: bool,
    voice: Voice,
    drum: [DrumVoice; 2],
    drum_cur: usize,
    stem: usize,
    gl: f32,
    gr: f32,
    vol: f32,
    inst: u8,
    echo: bool,
    echo_ticks: u32,
    echo_vol: f32,
    egl: f32,
    egr: f32,
    delay: Box<[f32]>,
    delay_len: usize,
    wpos: usize,
    /// Samples since anything non-silent was written to the delay line.
    quiet: usize,
    next: usize,
    off_in: u32,
    /// Channel automation lane (0 = no lane active).
    lane_cut: f32,
    lane_res: f32,
    svf: [Svf; 2],
}

impl Chan {
    fn new(sr: f32) -> Self {
        let n = ((sr * MAX_ECHO_SECS) as usize).max(16);
        Self {
            drums: false,
            voice: Voice::new(sr),
            drum: [DrumVoice::new(sr); 2],
            drum_cur: 0,
            stem: 0,
            gl: 0.7,
            gr: 0.7,
            vol: 1.0,
            inst: 0,
            echo: false,
            echo_ticks: 0,
            echo_vol: 0.0,
            egl: 0.0,
            egr: 0.0,
            delay: vec![0.0; n].into_boxed_slice(),
            delay_len: 1,
            wpos: 0,
            quiet: usize::MAX / 2,
            next: 0,
            off_in: 0,
            lane_cut: 0.0,
            lane_res: 0.0,
            svf: [Svf::default(); 2],
        }
    }
    fn active(&self) -> bool {
        if self.drums { self.drum.iter().any(|d| d.is_active()) } else { self.voice.is_active() }
    }
    fn release(&mut self) {
        self.voice.note_off();
        self.off_in = 0;
    }
    fn kill(&mut self) {
        self.voice.kill();
        for d in &mut self.drum {
            d.fade_out();
        }
        self.off_in = 0;
    }
}

fn pan_gains(pan: f32) -> (f32, f32) {
    let th = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (th.cos(), th.sin())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Src {
    Track(usize),
    Stinger,
}

/// Requests/results exchanged between the player and the main deck at bar/beat boundaries.
struct Ctl {
    sample0: u64,
    intensity: f32,
    tempo: f32,
    switch_pending: bool,
    sting_pending: Option<StingerKind>,
    switch_at: Option<usize>,
    sting_at: Option<(usize, StingerKind, f32)>,
    /// Main deck 16ths crossed this block: (sample offset, pattern step 0..32) — overlay layers.
    steps: [(usize, u8); 8],
    nsteps: usize,
}

struct Deck {
    sr: f32,
    thresh: u64,
    src: Option<Src>,
    seq: bool,
    held: bool,
    order_pos: usize,
    section: usize,
    tick: u32,
    override_section: Option<usize>,
    acc: u64,
    rate: u64,
    bpm_fixed: Option<f32>,
    transpose: i32,
    delay: usize,
    chans: [Chan; MAX_CHANNELS],
    nchan: usize,
    stem_gain: [Ramp; 3],
    duck: [Ramp; 3],
    fade: Ramp,
    gain: f32,
    bars: u32,
    beat: u32,
    loops: u32,
    cur_bpm: f32,
    scratch: [f32; CONTROL],
    side: [f32; CONTROL],
    sl: [[f32; CONTROL]; 3],
    sr_: [[f32; CONTROL]; 3],
    // ---- remix machinery ----
    adaptive: Option<Adaptive>,
    phase: AdaptPhase,
    build_left: u32,
    build_tick: u32,
    build_len: u32,
    cut: [Sweep; 3],
    res: [Ramp; 3],
    stem_svf: [[Svf; 2]; 3],
    use_filters: bool,
    lanes: bool,
    lane_cut: [f32; 3],
    lane_res: [f32; 3],
    fx: [DrumVoice; 3],
    fx_cur: usize,
    fx_buf: [f32; CONTROL],
    pump: Option<Pump>,
    pump_env: f32,
    pump_att: u32,
    pump_coef: f32,
    lofi: Option<Lofi>,
    wow_t: f32,
    rng: u32,
    pop: [f32; 2],
    ds_hold: [[f32; 2]; 3],
    ds_n: u32,
}

impl Deck {
    fn new(sr: f32) -> Self {
        Self {
            sr,
            thresh: 60 * (sr as u64) * 1000,
            src: None,
            seq: false,
            held: false,
            order_pos: 0,
            section: 0,
            tick: 0,
            override_section: None,
            acc: 0,
            rate: 1,
            bpm_fixed: None,
            transpose: 0,
            delay: 0,
            chans: std::array::from_fn(|_| Chan::new(sr)),
            nchan: 0,
            stem_gain: [Ramp::at(0.0); 3],
            duck: [Ramp::at(1.0); 3],
            fade: Ramp::at(1.0),
            gain: 1.0,
            bars: 0,
            beat: 0,
            loops: 0,
            cur_bpm: 120.0,
            scratch: [0.0; CONTROL],
            side: [0.0; CONTROL],
            sl: [[0.0; CONTROL]; 3],
            sr_: [[0.0; CONTROL]; 3],
            adaptive: None,
            phase: AdaptPhase::Off,
            build_left: 0,
            build_tick: 0,
            build_len: 1,
            cut: [Sweep::at(OPEN_HZ); 3],
            res: [Ramp::at(0.0); 3],
            stem_svf: [[Svf::default(); 2]; 3],
            use_filters: false,
            lanes: false,
            lane_cut: [OPEN_HZ; 3],
            lane_res: [0.0; 3],
            fx: [DrumVoice::new(sr); 3],
            fx_cur: 1,
            fx_buf: [0.0; CONTROL],
            pump: None,
            pump_env: 0.0,
            pump_att: 0,
            pump_coef: 0.0,
            lofi: None,
            wow_t: 0.0,
            rng: 0x2545_F491,
            pop: [0.0; 2],
            ds_hold: [[0.0; 2]; 3],
            ds_n: 0,
        }
    }

    fn idle(&self) -> bool {
        self.src.is_none()
    }

    fn configure(&mut self, song: &Song) {
        self.nchan = song.channels.len().min(MAX_CHANNELS);
        for (ch, spec) in self.chans.iter_mut().zip(song.channels.iter()) {
            ch.kill();
            ch.drums = spec.drums;
            ch.stem = spec.stem.index();
            let (l, r) = pan_gains(spec.pan);
            ch.gl = l;
            ch.gr = r;
            ch.vol = spec.vol;
            ch.inst = spec.inst;
            ch.next = 0;
            ch.off_in = 0;
            ch.lane_cut = 0.0;
            ch.lane_res = 0.0;
            match spec.echo {
                Some(e) => {
                    ch.echo = true;
                    ch.echo_ticks = e.ticks;
                    ch.echo_vol = e.vol;
                    let (l, r) = pan_gains(e.pan);
                    ch.egl = l;
                    ch.egr = r;
                    ch.delay.fill(0.0);
                    ch.quiet = usize::MAX / 2;
                }
                None => ch.echo = false,
            }
        }
        for ch in self.chans.iter_mut().skip(self.nchan) {
            ch.kill();
            ch.echo = false;
        }
        self.gain = song.gain;
        let m = &song.meta;
        self.adaptive = m.adaptive;
        self.pump = m.pump;
        self.pump_env = 0.0;
        self.pump_att = 0;
        self.pump_coef = m.pump.map_or(0.0, |p| (-6.9 / (p.release * self.sr)).exp());
        self.lofi = m.lofi;
        self.wow_t = 0.0;
        self.pop = [0.0; 2];
        self.ds_n = 0;
        self.lanes = song.sections.iter().any(|s| !s.autos.is_empty());
        let stem_lanes =
            song.sections.iter().flat_map(|s| s.autos.iter()).any(|a| matches!(a.target, AutoTarget::Stem(_) | AutoTarget::AllStems));
        self.use_filters = m.adaptive.is_some() || stem_lanes;
        self.stem_svf = [[Svf::default(); 2]; 3];
        self.lane_cut = [OPEN_HZ; 3];
        self.lane_res = [0.0; 3];
        for f in &mut self.fx {
            f.fade_out();
        }
    }

    fn start(&mut self, src: Src, song: &Song, order_pos: usize, delay: usize) {
        self.configure(song);
        self.src = Some(src);
        self.seq = true;
        self.held = false;
        self.order_pos = order_pos;
        self.tick = 0;
        self.override_section = None;
        self.acc = self.thresh;
        self.delay = delay;
        self.fade = Ramp::at(1.0);
        self.duck = [Ramp::at(1.0); 3];
        self.bars = 0;
        self.beat = 0;
        self.loops = 0;
        self.transpose = 0;
        self.bpm_fixed = None;
    }

    /// Adaptive phase + filters for a fresh start at `intensity`.
    fn init_adaptive(&mut self, intensity: f32) {
        self.phase = match self.adaptive {
            None => AdaptPhase::Off,
            Some(a) if intensity + 1e-6 >= a.combat_at => AdaptPhase::Combat,
            Some(_) => AdaptPhase::Calm,
        };
        for s in 0..3 {
            self.cut[s] = Sweep::at(self.phase_cutoff(s, self.phase));
            self.res[s] = Ramp::at(self.phase_res(s, self.phase));
        }
    }

    fn filtered_stem(&self, s: usize) -> bool {
        self.adaptive.is_some_and(|a| s != 2 || a.filter_lead)
    }

    fn phase_cutoff(&self, s: usize, p: AdaptPhase) -> f32 {
        match (self.adaptive, p) {
            (Some(a), AdaptPhase::Calm) if self.filtered_stem(s) => a.explore_cutoff,
            (Some(a), AdaptPhase::Build) if self.filtered_stem(s) => a.build_cutoff,
            _ => OPEN_HZ,
        }
    }

    fn phase_res(&self, s: usize, p: AdaptPhase) -> f32 {
        match (self.adaptive, p) {
            (Some(a), AdaptPhase::Calm) if self.filtered_stem(s) => a.explore_res,
            (Some(a), AdaptPhase::Build) if self.filtered_stem(s) => a.build_res,
            _ => 0.0,
        }
    }

    fn sweep_to_phase(&mut self, p: AdaptPhase, samples: f32) {
        for s in 0..3 {
            let (c, r) = (self.phase_cutoff(s, p), self.phase_res(s, p));
            self.cut[s].set(c, samples);
            self.res[s].set(r, samples);
        }
    }

    /// One-shot on the fx bus (alternating slots so re-hits fade instead of clicking).
    fn fx_hit(&mut self, d: Drum, vel: f32, len: f32) {
        if let Some(f) = self.fx.get_mut(self.fx_cur) {
            f.fade_out();
        }
        self.fx_cur = if self.fx_cur == 1 { 2 } else { 1 };
        if let Some(f) = self.fx.get_mut(self.fx_cur) {
            f.hit(d, vel, len, 0.0);
        }
    }

    /// Explore → build → drop at a main-deck bar line.
    fn adaptive_bar(&mut self, song: &Song, intensity: f32, log: &mut DebugLog, now: u64) {
        let Some(a) = self.adaptive else { return };
        let combat = intensity + 1e-6 >= a.combat_at;
        let bar_samples = song.bar_ticks as f32 * self.samples_per_tick();
        match self.phase {
            AdaptPhase::Calm | AdaptPhase::Off if combat => {
                if a.build_bars == 0 {
                    self.drop(&a, log, now);
                } else {
                    self.phase = AdaptPhase::Build;
                    self.build_left = a.build_bars;
                    self.build_tick = 0;
                    self.build_len = a.build_bars * song.bar_ticks;
                    let len = a.build_bars as f32 * bar_samples;
                    self.sweep_to_phase(AdaptPhase::Build, len);
                    if a.riser
                        && let Some(f) = self.fx.get_mut(0)
                    {
                        f.hit(Drum::Riser, 0.95, len / self.sr, 0.0);
                    }
                    log.build(now);
                }
            }
            AdaptPhase::Build if !combat => {
                self.phase = AdaptPhase::Calm;
                self.sweep_to_phase(AdaptPhase::Calm, bar_samples);
                if let Some(f) = self.fx.get_mut(0) {
                    f.fade_out();
                }
            }
            AdaptPhase::Build => {
                self.build_left = self.build_left.saturating_sub(1);
                if self.build_left == 0 {
                    self.drop(&a, log, now);
                }
            }
            AdaptPhase::Combat if !combat => {
                self.phase = AdaptPhase::Calm;
                self.sweep_to_phase(AdaptPhase::Calm, bar_samples);
            }
            _ => {}
        }
    }

    /// THE DROP: filter snaps open, riser cut, impact.
    fn drop(&mut self, a: &Adaptive, log: &mut DebugLog, now: u64) {
        self.phase = AdaptPhase::Combat;
        self.sweep_to_phase(AdaptPhase::Combat, 0.004 * self.sr);
        if let Some(f) = self.fx.get_mut(0) {
            f.fade_out();
        }
        if a.impact {
            self.fx_hit(Drum::Impact, 1.0, 1.0);
        }
        log.drop_at(now);
    }

    /// Stop sequencing; voices release naturally while the deck fades.
    fn stop(&mut self, fade_secs: f32) {
        self.seq = false;
        self.held = false;
        for ch in &mut self.chans {
            ch.release();
        }
        self.fade.set(0.0, fade_secs * self.sr);
    }

    fn update_rate(&mut self, song: &Song, tempo: f32) {
        let bpm = match self.bpm_fixed {
            Some(b) => b,
            None => {
                // At tick 0 the next section (not yet entered) sets the tempo of the coming tick.
                let sec = if self.tick == 0 {
                    self.override_section.or_else(|| song.order.get(self.order_pos).copied()).unwrap_or(self.section)
                } else {
                    self.section
                };
                song.sections.get(sec).map_or(song.bpm, |s| s.bpm_at(self.tick)) * tempo
            }
        };
        let bpm = bpm.clamp(10.0, 1000.0);
        self.cur_bpm = bpm;
        self.rate = ((f64::from(bpm) * 1000.0).round() as u64).max(1) * u64::from(PPQ);
    }

    fn samples_per_tick(&self) -> f32 {
        self.thresh as f32 / self.rate as f32
    }

    fn update_echo(&mut self) {
        let spt = self.samples_per_tick();
        for ch in self.chans.iter_mut().take(self.nchan) {
            if ch.echo {
                let n = (ch.echo_ticks as f32 * spt).round() as usize;
                ch.delay_len = n.clamp(1, ch.delay.len() - 1);
            }
        }
    }

    fn intensity_targets(&self, song: &Song, intensity: f32) -> [f32; 3] {
        match self.adaptive {
            Some(a) if self.phase != AdaptPhase::Combat => {
                let mut g = song.layers.gains(intensity.min(a.combat_at - 0.001).max(0.0));
                g[1] = g[1].max(a.explore_drive);
                g
            }
            _ => song.layers.gains(intensity),
        }
    }

    fn apply_intensity(&mut self, song: &Song, intensity: f32, log: &mut DebugLog, at: u64) {
        let targets = self.intensity_targets(song, intensity);
        let bar_samples = song.bar_ticks as f32 * self.samples_per_tick();
        let mut changed = false;
        for (g, &t) in self.stem_gain.iter_mut().zip(targets.iter()) {
            if (g.target - t).abs() > 1e-6 {
                changed = true;
                let secs_samples = if t > g.cur { FADE_IN_SECS * self.sr } else { bar_samples };
                g.set(t, secs_samples);
            }
        }
        if changed {
            log.change(at);
        }
    }

    fn samples_to_tick(&self) -> usize {
        if !self.seq || self.held {
            return usize::MAX;
        }
        if self.acc >= self.thresh {
            return 0;
        }
        (self.thresh - self.acc).div_ceil(self.rate) as usize
    }

    /// Process one sequencer tick at `offset` samples into the current block.
    fn tick(&mut self, song: &Song, main: bool, offset: usize, ctl: &mut Ctl, log: &mut DebugLog) {
        let now = ctl.sample0 + offset as u64;
        // Hold for a stinger (checked before anything is consumed so the tick replays on resume).
        let on_bar = self.tick.is_multiple_of(song.bar_ticks);
        if main
            && on_bar
            && let Some(k) = ctl.sting_pending.filter(|k| k.on_bar() && k.holds_track())
        {
            log.bar(now);
            self.held = true;
            for ch in &mut self.chans {
                ch.release();
            }
            ctl.sting_at = Some((offset, k, self.cur_bpm));
            ctl.sting_pending = None;
            self.acc = self.thresh;
            return;
        }
        if self.tick == 0 {
            self.acc = 0;
            let sec = self.override_section.or_else(|| song.order.get(self.order_pos).copied()).unwrap_or(0);
            self.section = sec;
            for ch in &mut self.chans {
                ch.next = 0;
            }
            if main && self.override_section.is_none() && self.order_pos == song.loop_idx {
                if log.loop_start.is_none() {
                    log.loop_start = Some(now);
                } else {
                    log.last_wrap = Some(now);
                }
            }
        } else {
            self.acc = self.acc.saturating_sub(self.thresh);
        }
        for ch in self.chans.iter_mut().take(self.nchan) {
            if ch.off_in > 0 {
                ch.off_in -= 1;
                if ch.off_in == 0 {
                    ch.voice.note_off();
                }
            }
        }
        if main && on_bar {
            log.bar(now);
            self.bars += 1;
            self.update_rate(song, ctl.tempo);
            self.update_echo();
            self.adaptive_bar(song, ctl.intensity, log, now);
            self.apply_intensity(song, ctl.intensity, log, now);
            if ctl.switch_pending {
                ctl.switch_at = Some(offset);
                ctl.switch_pending = false;
                self.stop(CUT_FADE_SECS);
                return;
            }
            if let Some(k) = ctl.sting_pending.filter(|k| k.ends_track()) {
                ctl.sting_pending = None;
                if k == StingerKind::Victory && song.ending.is_some() && self.override_section.is_none() {
                    // Jump straight into the track's own ending cadence, then stop.
                    self.override_section = song.ending;
                    self.tick = 0;
                    self.acc = self.thresh;
                    log.stinger_start = Some(now);
                    return self.tick(song, main, offset, ctl, log);
                }
                ctl.sting_at = Some((offset, k, self.cur_bpm));
                let beat_secs = song.beat_ticks as f32 * self.samples_per_tick() / self.sr;
                self.stop(beat_secs.max(0.05));
                return;
            }
        }
        // Overlay layers lock to the main deck's 16ths (two-bar pattern: odd bars 0..16, even 16..32).
        if main && self.tick.is_multiple_of(PPQ / 4) && ctl.nsteps < ctl.steps.len() {
            let in_bar = ((self.tick % song.bar_ticks.max(1)) / (PPQ / 4)).min(15) as u8;
            ctl.steps[ctl.nsteps] = (offset, in_bar + if self.bars % 2 == 0 { 16 } else { 0 });
            ctl.nsteps += 1;
        }
        if main && self.tick.is_multiple_of(song.beat_ticks) {
            log.beat(now);
            self.beat = (self.tick % song.bar_ticks) / song.beat_ticks.max(1);
            if let Some(k) = ctl.sting_pending.filter(|k| !k.on_bar()) {
                ctl.sting_at = Some((offset, k, self.cur_bpm));
                ctl.sting_pending = None;
            }
        }
        // Build-up snare roll: 8ths, then 16ths in the last bar, 32nds in the last half bar.
        if self.phase == AdaptPhase::Build && self.adaptive.is_some_and(|a| a.roll) {
            let remaining = self.build_len.saturating_sub(self.build_tick);
            let step = if remaining <= song.bar_ticks / 2 {
                PPQ / 8
            } else if remaining <= song.bar_ticks {
                PPQ / 4
            } else {
                PPQ / 2
            };
            if self.build_tick.is_multiple_of(step) {
                let vel = 0.2 + 0.7 * (self.build_tick as f32 / self.build_len.max(1) as f32);
                self.fx_hit(Drum::Snare2, vel, 0.1);
            }
            self.build_tick += 1;
        }
        let Some(sec) = song.sections.get(self.section) else {
            self.stop(0.05);
            return;
        };
        let transpose = self.transpose;
        let spt_secs = self.samples_per_tick() / self.sr;
        let mut kick = false;
        for (ci, ch) in self.chans.iter_mut().enumerate().take(self.nchan) {
            let Some(notes) = sec.parts.get(ci) else { continue };
            while let Some(n) = notes.get(ch.next) {
                if n.tick != self.tick {
                    break;
                }
                ch.next += 1;
                if main {
                    log.cur_notes += 1;
                }
                let vel = f32::from(n.vol) / 15.0;
                if ch.drums {
                    let cur = ch.drum_cur.min(1);
                    if let Some(d) = ch.drum.get_mut(cur) {
                        d.fade_out();
                    }
                    ch.drum_cur = 1 - cur;
                    let drum = Drum::from_id(n.key);
                    kick |= drum.is_kick();
                    if let Some(d) = ch.drum.get_mut(ch.drum_cur) {
                        d.hit(drum, vel, n.len as f32 * spt_secs, f32::from(n.pitch));
                    }
                } else {
                    let ii = if n.inst == u8::MAX { ch.inst } else { n.inst };
                    let Some(inst) = song.instruments.get(usize::from(ii)) else { continue };
                    let (keys, nk) = n.keys();
                    let mut kf = [0.0f32; 4];
                    for (d, &k) in kf.iter_mut().zip(keys.iter()) {
                        *d = f32::from(k) + transpose as f32;
                    }
                    ch.voice.note_on(inst, &kf[..nk], vel, n.glide);
                    let gate = (n.len as f32 * f32::from(n.q) / 8.0 * inst.gate).round() as u32;
                    ch.off_in = gate.clamp(1, n.len.max(1));
                }
            }
        }
        if kick && self.pump.is_some() {
            self.pump_att = (0.004 * self.sr) as u32;
        }
        self.tick += 1;
        if self.tick >= sec.len {
            self.tick = 0;
            if self.override_section.is_some() {
                self.stop(0.5);
                // Let the final notes ring out through their release instead of a hard fade.
                self.fade = Ramp::at(1.0);
                return;
            }
            self.order_pos += 1;
            if self.order_pos >= song.order.len() {
                self.order_pos = song.loop_idx;
                self.loops += 1;
            }
        }
        self.update_rate(song, ctl.tempo);
    }

    /// Render `n` frames of this deck and mix them into `out` (4 stems × L/R).
    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        song: &Song,
        main: bool,
        n: usize,
        ctl: &mut Ctl,
        log: &mut DebugLog,
        out_l: &mut [[f32; CONTROL]; OUT_STEMS],
        out_r: &mut [[f32; CONTROL]; OUT_STEMS],
        to_stinger: bool,
    ) {
        if self.src.is_none() {
            return;
        }
        if self.bpm_fixed.is_none() || self.rate <= 1 {
            self.update_rate(song, ctl.tempo);
        }
        let mut pos = 0;
        while pos < n {
            let mut span = (n - pos).min(CONTROL);
            if self.delay > 0 {
                span = span.min(self.delay);
            } else {
                let t = self.samples_to_tick();
                if t == 0 {
                    self.tick(song, main, pos, ctl, log);
                    continue;
                }
                span = span.min(t);
            }
            self.render_span(song, pos, span, out_l, out_r, to_stinger);
            if self.delay > 0 {
                self.delay -= span;
            } else if self.seq && !self.held {
                self.acc += self.rate * span as u64;
            }
            pos += span;
        }
        // Retire a finished deck.
        if !self.seq
            && !self.held
            && (self.fade.cur <= 0.0
                || !(self.chans.iter().take(self.nchan).any(|c| c.active() || c.quiet < c.delay_len)
                    || self.fx.iter().any(|f| f.is_active())))
        {
            self.src = None;
        }
    }

    /// Evaluate the section's automation lanes at the current tick.
    fn update_lanes(&mut self, song: &Song) {
        self.lane_cut = [OPEN_HZ; 3];
        self.lane_res = [0.0; 3];
        for ch in self.chans.iter_mut().take(self.nchan) {
            ch.lane_cut = 0.0;
            ch.lane_res = 0.0;
        }
        let Some(sec) = song.sections.get(self.section) else { return };
        for a in &sec.autos {
            let Some(v) = a.value_at(self.tick) else { continue };
            let stems = match a.target {
                AutoTarget::Stem(s) => s..s + 1,
                AutoTarget::AllStems => 0..3,
                AutoTarget::Channel(c) => {
                    if let Some(ch) = self.chans.get_mut(c) {
                        match a.param {
                            AutoParam::Cutoff => ch.lane_cut = v,
                            AutoParam::Res => ch.lane_res = v,
                        }
                    }
                    continue;
                }
            };
            for s in stems {
                match a.param {
                    AutoParam::Cutoff => self.lane_cut[s] = self.lane_cut[s].min(v),
                    AutoParam::Res => self.lane_res[s] = self.lane_res[s].max(v),
                }
            }
        }
    }

    fn render_span(
        &mut self,
        song: &Song,
        pos: usize,
        span: usize,
        out_l: &mut [[f32; CONTROL]; OUT_STEMS],
        out_r: &mut [[f32; CONTROL]; OUT_STEMS],
        to_stinger: bool,
    ) {
        for s in 0..3 {
            self.sl[s][..span].fill(0.0);
            self.sr_[s][..span].fill(0.0);
        }
        if self.lanes {
            self.update_lanes(song);
        }
        let wow = match self.lofi {
            Some(l) if l.wow > 0.0 => {
                let w = l.wow * sin_turns(l.wow_rate * self.wow_t) + 0.35 * l.wow * sin_turns(l.wow_rate * 2.7 * self.wow_t + 0.3);
                self.wow_t += span as f32 / self.sr;
                w
            }
            _ => 0.0,
        };
        for ch in self.chans.iter_mut().take(self.nchan) {
            let echo_live = ch.echo && ch.quiet < ch.delay_len + span;
            if !ch.active() && !echo_live {
                if ch.echo {
                    ch.quiet = ch.quiet.saturating_add(span);
                }
                continue;
            }
            let mono = &mut self.scratch[..span];
            let side = &mut self.side[..span];
            let stereo = if ch.drums {
                mono.fill(0.0);
                for d in &mut ch.drum {
                    d.render_add(mono, 1.0);
                }
                false
            } else {
                ch.voice.render_stereo(mono, side, wow)
            };
            if ch.lane_cut > 0.0 {
                for f in &mut ch.svf {
                    f.set(ch.lane_cut, ch.lane_res, self.sr);
                }
                for x in mono.iter_mut() {
                    *x = ch.svf[0].lp(*x);
                }
                if stereo {
                    for x in side.iter_mut() {
                        *x = ch.svf[1].lp(*x);
                    }
                }
            }
            let st = ch.stem.min(2);
            let (l, r) = (&mut self.sl[st][..span], &mut self.sr_[st][..span]);
            let (gl, gr, vol) = (ch.gl * ch.vol, ch.gr * ch.vol, ch.vol);
            if ch.echo {
                let len = ch.delay.len();
                let (egl, egr) = (ch.egl * ch.echo_vol, ch.egr * ch.echo_vol);
                for i in 0..span {
                    let s = mono[i];
                    let rp = (ch.wpos + len - ch.delay_len) % len;
                    let d = ch.delay[rp];
                    ch.delay[ch.wpos] = s * vol;
                    ch.wpos = (ch.wpos + 1) % len;
                    if s != 0.0 {
                        ch.quiet = 0;
                    } else {
                        ch.quiet = ch.quiet.saturating_add(1);
                    }
                    l[i] += s * gl + d * egl;
                    r[i] += s * gr + d * egr;
                }
            } else {
                for i in 0..span {
                    let s = mono[i];
                    l[i] += s * gl;
                    r[i] += s * gr;
                }
            }
            if stereo {
                let w = SIDE_W * vol;
                for i in 0..span {
                    l[i] += side[i] * w;
                    r[i] -= side[i] * w;
                }
            }
        }
        // Resonant stem filters (adaptive explore/build/drop + automation lanes).
        if self.use_filters {
            for s in 0..3 {
                let fc = self.cut[s].value().min(self.lane_cut[s]);
                let q = self.res[s].cur.max(self.lane_res[s]);
                self.cut[s].advance(span);
                self.res[s].skip(span);
                // Always run (even when open) so the state tracks and re-closing never clicks.
                let [fl, fr] = &mut self.stem_svf[s];
                fl.set(fc, q, self.sr);
                fr.set(fc, q, self.sr);
                for i in 0..span {
                    self.sl[s][i] = fl.lp(self.sl[s][i]);
                    self.sr_[s][i] = fr.lp(self.sr_[s][i]);
                }
            }
        }
        // Lo-fi colour: crush / sample-hold, vinyl crackle into the base stem.
        if let Some(lf) = self.lofi {
            let q = if lf.bits > 0.0 { (lf.bits - 1.0).exp2() } else { 0.0 };
            let ds = lf.downsample.max(1);
            let thresh = (lf.crackle * 14.0 / self.sr * 4_294_967_295.0) as u32;
            for i in 0..span {
                let hold = ds > 1 && !self.ds_n.is_multiple_of(ds);
                self.ds_n = self.ds_n.wrapping_add(1);
                for s in 0..3 {
                    let (mut l, mut r) = (self.sl[s][i], self.sr_[s][i]);
                    if hold {
                        l = self.ds_hold[s][0];
                        r = self.ds_hold[s][1];
                    } else {
                        if q > 0.0 {
                            l = (l * q).round() / q;
                            r = (r * q).round() / q;
                        }
                        self.ds_hold[s] = [l, r];
                    }
                    self.sl[s][i] = l;
                    self.sr_[s][i] = r;
                }
                if lf.crackle > 0.0 {
                    self.rng ^= self.rng << 13;
                    self.rng ^= self.rng >> 17;
                    self.rng ^= self.rng << 5;
                    if self.rng < thresh {
                        let a = (self.rng >> 8) as f32 / 16_777_216.0 * 0.012 * lf.crackle;
                        self.pop[0] += a;
                        self.pop[1] -= a * 0.6;
                    }
                    self.sl[0][i] += self.pop[0];
                    self.sr_[0][i] += self.pop[1];
                    self.pop[0] *= 0.55;
                    self.pop[1] *= 0.6;
                }
            }
        }
        // Fx bus (risers, build rolls, impacts): post-filter, into the drive stem.
        let fx_on = self.fx.iter().any(|f| f.is_active());
        if fx_on {
            self.fx_buf[..span].fill(0.0);
            for f in &mut self.fx {
                f.render_add(&mut self.fx_buf[..span], 0.9);
            }
        }
        // Stem gains / duck / pump / deck fade, then into the output stems.
        let gain = self.gain;
        let pump = self.pump;
        for i in 0..span {
            let f = self.fade.next() * gain;
            let p = match pump {
                Some(pm) => {
                    if self.pump_att > 0 {
                        self.pump_env += (1.0 - self.pump_env) / self.pump_att as f32;
                        self.pump_att -= 1;
                    } else {
                        self.pump_env *= self.pump_coef;
                    }
                    1.0 - pm.depth * self.pump_env
                }
                None => 1.0,
            };
            for s in 0..3 {
                let pg = if pump.is_some_and(|pm| pm.stems[s]) { p } else { 1.0 };
                let g = self.stem_gain[s].next() * self.duck[s].next() * f * pg;
                let o = if to_stinger { 3 } else { s };
                out_l[o][pos + i] += self.sl[s][i] * if to_stinger { f } else { g };
                out_r[o][pos + i] += self.sr_[s][i] * if to_stinger { f } else { g };
            }
            if fx_on && !to_stinger {
                out_l[1][pos + i] += self.fx_buf[i] * f;
                out_r[1][pos + i] += self.fx_buf[i] * f;
            }
        }
    }
}

/// The adaptive music player. Owned by the audio thread (see the crate docs for the contract).
pub struct MusicPlayer {
    lib: Arc<MusicLibrary>,
    sr: u32,
    decks: [Deck; 2],
    active: usize,
    sting: Deck,
    sting_kind: Option<StingerKind>,
    intensity: f32,
    tempo: Ramp,
    pending_play: Option<(TrackId, bool)>,
    pending_stop: bool,
    pending_sting: Option<StingerKind>,
    out_l: [[f32; CONTROL]; OUT_STEMS],
    out_r: [[f32; CONTROL]; OUT_STEMS],
    filt: [[StemFilter; 2]; OUT_STEMS],
    pos: usize,
    sample: u64,
    log: DebugLog,
    style: Style,
    requested: Option<TrackId>,
    rot: u32,
    rng: u32,
    overlays: Overlays,
}

impl MusicPlayer {
    /// Build a player. Allocates all voice/echo state up front (≈ 5.5 MB at 48 kHz, mostly the
    /// 1.2 s echo lines of 3 decks × 8 channels); nothing is allocated afterwards.
    pub fn new(lib: Arc<MusicLibrary>, sample_rate: u32) -> Self {
        let sr = sample_rate.clamp(8_000, 192_000);
        let f = sr as f32;
        Self {
            lib,
            sr,
            decks: [Deck::new(f), Deck::new(f)],
            active: 0,
            sting: Deck::new(f),
            sting_kind: None,
            intensity: 0.3,
            tempo: Ramp::at(1.0),
            pending_play: None,
            pending_stop: false,
            pending_sting: None,
            out_l: [[0.0; CONTROL]; OUT_STEMS],
            out_r: [[0.0; CONTROL]; OUT_STEMS],
            filt: [[StemFilter::new(f); 2]; OUT_STEMS],
            pos: CONTROL,
            sample: 0,
            log: DebugLog::default(),
            style: Style::Classic,
            requested: None,
            rot: 0,
            rng: 0x1234_5678,
            overlays: Overlays::new(f),
        }
    }

    pub fn library(&self) -> &Arc<MusicLibrary> {
        &self.lib
    }

    pub fn sample_rate(&self) -> u32 {
        self.sr
    }

    fn current_track(&self) -> Option<TrackId> {
        let d = &self.decks[self.active];
        match (d.seq, d.src) {
            (true, Some(Src::Track(i))) => Some(TrackId(i as u16)),
            _ => None,
        }
    }

    /// The track that actually plays for a request under the current style.
    fn resolve(&mut self, req: TrackId) -> TrackId {
        let base = self.lib.classic_of(req);
        if base != req {
            return req;
        }
        let vars = self.lib.variants_of(base);
        if vars.is_empty() {
            return base;
        }
        match self.style {
            Style::Classic => base,
            Style::Remix(pref) => {
                if let Some(p) = pref
                    && let Some(&v) = vars.iter().find(|v| self.lib.track(**v).and_then(|s| s.meta.style) == Some(p))
                {
                    return v;
                }
                self.rot = self.rot.wrapping_add(1);
                vars.get(self.rot as usize % vars.len()).copied().unwrap_or(base)
            }
            Style::Shuffle => {
                self.rng ^= self.rng << 13;
                self.rng ^= self.rng >> 17;
                self.rng ^= self.rng << 5;
                let k = self.rng as usize % (vars.len() + 1);
                if k == 0 { base } else { vars.get(k - 1).copied().unwrap_or(base) }
            }
        }
    }

    /// Apply a command (never allocates; safe on the audio thread).
    pub fn command(&mut self, cmd: MusicCommand) {
        match cmd {
            MusicCommand::Play { track, skip_intro } => {
                if self.lib.track(track).is_none() {
                    return;
                }
                let cur = self.current_track();
                if cur.is_some() && self.requested == Some(track) && self.pending_play.is_none() {
                    self.pending_stop = false;
                    return;
                }
                self.requested = Some(track);
                let t = self.resolve(track);
                if cur == Some(t) {
                    self.pending_play = None;
                    self.pending_stop = false;
                    return;
                }
                self.pending_play = Some((t, skip_intro));
                self.pending_stop = false;
            }
            MusicCommand::Stop => {
                self.pending_play = None;
                self.pending_stop = true;
                self.requested = None;
            }
            MusicCommand::SetIntensity(x) => {
                if x.is_finite() {
                    self.intensity = x.clamp(0.0, 1.0);
                }
            }
            MusicCommand::Stinger(k) => self.pending_sting = Some(k),
            MusicCommand::SetTempo { multiplier, ramp_secs } => {
                if multiplier.is_finite() {
                    let blocks = (ramp_secs.max(0.0) * self.sr as f32 / CONTROL as f32).max(1.0);
                    self.tempo.set(multiplier.clamp(0.25, 4.0), blocks);
                }
            }
            MusicCommand::Overlay { id, on } => self.overlays.set(id, on),
            MusicCommand::SetStyle(s) => {
                if s == self.style {
                    return;
                }
                self.style = s;
                if let (Some(req), Some(cur)) = (self.requested, self.current_track()) {
                    let t = self.resolve(req);
                    if t != cur {
                        self.pending_play = Some((t, true));
                        self.pending_stop = false;
                    }
                }
            }
        }
    }

    pub fn play(&mut self, track: TrackId) {
        self.command(MusicCommand::Play { track, skip_intro: false });
    }
    pub fn set_intensity(&mut self, x: f32) {
        self.command(MusicCommand::SetIntensity(x));
    }
    pub fn stinger(&mut self, k: StingerKind) {
        self.command(MusicCommand::Stinger(k));
    }
    pub fn set_tempo(&mut self, multiplier: f32, ramp_secs: f32) {
        self.command(MusicCommand::SetTempo { multiplier, ramp_secs });
    }
    pub fn set_style(&mut self, s: Style) {
        self.command(MusicCommand::SetStyle(s));
    }
    pub fn stop(&mut self) {
        self.command(MusicCommand::Stop);
    }

    pub fn status(&self) -> MusicStatus {
        let d = &self.decks[self.active];
        let (track, meta) = match d.src {
            Some(Src::Track(i)) => (Some(TrackId(i as u16)), self.lib.tracks.get(i).map(|s| s.meta)),
            _ => (None, None),
        };
        MusicStatus {
            track,
            playing: d.seq,
            bars: d.bars,
            beat: d.beat,
            section: d.section as u16,
            bpm: d.cur_bpm,
            tempo_multiplier: self.tempo.cur,
            intensity: self.intensity,
            stem_gains: [d.stem_gain[0].cur, d.stem_gain[1].cur, d.stem_gain[2].cur],
            stinger: if self.sting.idle() { None } else { self.sting_kind },
            sample: self.sample,
            loops: d.loops,
            style: self.style,
            phase: d.phase,
            sub_bass: meta.is_some_and(|m| m.sub_bass),
            hpf_hz: meta.map_or(60.0, |m| m.hpf_hz),
            overlays: self.overlays.mask(),
        }
    }

    pub fn debug_log(&self) -> &DebugLog {
        &self.log
    }

    /// Current stem gain targets [base, drive, lead] of the active track.
    pub fn stem_targets(&self) -> [f32; 3] {
        let d = &self.decks[self.active];
        [d.stem_gain[0].target, d.stem_gain[1].target, d.stem_gain[2].target]
    }

    fn song_of(lib: &MusicLibrary, src: Option<Src>) -> Option<&Song> {
        match src {
            Some(Src::Track(i)) => lib.tracks.get(i),
            Some(Src::Stinger) => Some(&lib.stingers.song),
            None => None,
        }
    }

    fn start_track(&mut self, deck: usize, track: TrackId, skip_intro: bool, delay: usize) {
        let lib = Arc::clone(&self.lib);
        let Some(song) = lib.track(track) else { return };
        let intensity = self.intensity;
        let d = &mut self.decks[deck];
        let pos = if skip_intro { song.loop_idx } else { 0 };
        d.start(Src::Track(usize::from(track.0)), song, pos, delay);
        d.init_adaptive(intensity);
        let g = d.intensity_targets(song, intensity);
        d.stem_gain = [Ramp::at(g[0]), Ramp::at(g[1]), Ramp::at(g[2])];
        d.update_rate(song, self.tempo.cur);
        d.update_echo();
        let sr = self.sr as f32;
        for stem in self.filt.iter_mut().take(3) {
            for f in stem {
                f.set_hpf(song.meta.hpf_hz, sr);
            }
        }
        self.log.loop_start = None;
        self.log.last_wrap = None;
    }

    fn start_stinger(&mut self, kind: StingerKind, delay: usize, bpm: Option<f32>) -> bool {
        let lib = Arc::clone(&self.lib);
        let bank = &lib.stingers;
        let Some(sec) = bank.sections.get(kind.index()).copied().flatten() else { return false };
        let (tonic, minor) = match self.decks[self.active].src {
            Some(Src::Track(i)) => lib.tracks.get(i).map_or((0, false), |s| (s.tonic, s.minor)),
            _ => (0, false),
        };
        let tr = bank.transpose_for(kind, tonic, minor);
        let d = &mut self.sting;
        d.start(Src::Stinger, &bank.song, 0, delay);
        d.init_adaptive(0.0);
        d.override_section = Some(sec);
        d.transpose = tr;
        let own = bank.song.sections.get(sec).map_or(bank.song.bpm, |s| s.bpm);
        // Overlay stingers follow the track's tempo so they land in time; big moments keep their own.
        d.bpm_fixed = Some(if kind.holds_track() || kind.ends_track() { own } else { bpm.unwrap_or(own) });
        d.stem_gain = [Ramp::at(1.0); 3];
        d.update_rate(&bank.song, 1.0);
        d.update_echo();
        self.sting_kind = Some(kind);
        self.log.stinger_start = Some(self.sample + delay as u64);
        true
    }

    fn process_block(&mut self) {
        for s in 0..OUT_STEMS {
            self.out_l[s].fill(0.0);
            self.out_r[s].fill(0.0);
        }
        // Tempo ramp (per block).
        let _ = self.tempo.next();
        let lib = Arc::clone(&self.lib);
        let a = self.active;
        let main_playing = self.decks[a].seq;
        // Silent → start immediately.
        if !main_playing {
            if let Some((t, skip)) = self.pending_play.take() {
                // Keep a still-releasing deck's tails: start on the other deck if this one is busy.
                let deck = if self.decks[a].idle() { a } else { 1 - a };
                self.start_track(deck, t, skip, 0);
                self.active = deck;
            }
            self.pending_stop = false;
        }
        let a = self.active;
        let main_playing = self.decks[a].seq;
        if !main_playing && let Some(k) = self.pending_sting.take() {
            let _ = self.start_stinger(k, 0, None);
        }
        // A track held under a stinger switches immediately when asked to.
        if self.decks[a].held && (self.pending_play.is_some() || self.pending_stop) {
            self.decks[a].stop(CUT_FADE_SECS);
            if let Some((t, skip)) = self.pending_play.take() {
                let b = 1 - a;
                self.start_track(b, t, skip, 0);
                self.active = b;
            }
            self.pending_stop = false;
        }
        let a = self.active;
        // Duck the lead/drive under overlay stingers.
        let overlay = !self.sting.idle() && self.sting_kind.is_some_and(|k| !k.holds_track() && !k.ends_track());
        let sr = self.sr as f32;
        for (s, target) in [(1usize, 0.85f32), (2, 0.5)] {
            let t = if overlay { target } else { 1.0 };
            let d = &mut self.decks[a].duck[s];
            if (d.target - t).abs() > 1e-6 {
                d.set(t, if overlay { 0.02 * sr } else { 0.3 * sr });
            }
        }
        let mut ctl = Ctl {
            sample0: self.sample,
            intensity: self.intensity,
            tempo: self.tempo.cur,
            switch_pending: self.pending_play.is_some() || self.pending_stop,
            sting_pending: self.pending_sting,
            switch_at: None,
            sting_at: None,
            steps: [(0, 0); 8],
            nsteps: 0,
        };
        {
            let deck = &mut self.decks[a];
            if let Some(song) = Self::song_of(&lib, deck.src) {
                deck.process(song, true, CONTROL, &mut ctl, &mut self.log, &mut self.out_l, &mut self.out_r, false);
            }
        }
        self.pending_sting = ctl.sting_pending;
        let b = 1 - a;
        if let Some(off) = ctl.switch_at {
            self.pending_stop = false;
            if let Some((t, skip)) = self.pending_play.take() {
                self.start_track(b, t, skip, off);
                self.active = b;
                // The new deck renders this block from `off`.
                let deck = &mut self.decks[b];
                if let Some(song) = Self::song_of(&lib, deck.src) {
                    let mut ctl2 = Ctl { switch_pending: false, sting_pending: self.pending_sting, sting_at: None, switch_at: None, ..ctl };
                    deck.process(song, true, CONTROL, &mut ctl2, &mut self.log, &mut self.out_l, &mut self.out_r, false);
                    self.pending_sting = ctl2.sting_pending;
                    if ctl2.sting_at.is_some() {
                        ctl.sting_at = ctl2.sting_at;
                    }
                }
            }
        } else {
            let deck = &mut self.decks[b];
            if let Some(song) = Self::song_of(&lib, deck.src) {
                let mut ctl2 = Ctl { switch_pending: false, sting_pending: None, sting_at: None, switch_at: None, ..ctl };
                deck.process(song, false, CONTROL, &mut ctl2, &mut self.log, &mut self.out_l, &mut self.out_r, false);
            }
        }
        if let Some((off, k, bpm)) = ctl.sting_at
            && !self.start_stinger(k, off, Some(bpm))
        {
            // No such stinger in the bank: never leave the track held.
            self.decks[self.active].held = false;
        }
        let was_playing = !self.sting.idle();
        if was_playing {
            let mut ctl3 = Ctl { switch_pending: false, sting_pending: None, sting_at: None, switch_at: None, ..ctl };
            let song = &lib.stingers.song;
            self.sting.process(song, false, CONTROL, &mut ctl3, &mut self.log, &mut self.out_l, &mut self.out_r, true);
        }
        // Overlay layers (drums…) on the main deck's 16ths, into the drive stem.
        let n = ctl.nsteps.min(ctl.steps.len());
        self.overlays.render(&ctl.steps[..n], &mut self.out_l[1], &mut self.out_r[1]);
        // Resume a held track once its stinger's sequence has finished.
        let main = &mut self.decks[self.active];
        if main.held && !self.sting.seq {
            main.held = false;
            main.acc = main.thresh;
        }
        // Output conditioning per stem.
        for s in 0..OUT_STEMS {
            let [fl, fr] = &mut self.filt[s];
            let mut bad = false;
            for i in 0..CONTROL {
                let l = fl.process(self.out_l[s][i]);
                let r = fr.process(self.out_r[s][i]);
                bad |= !(l.is_finite() && r.is_finite());
                self.out_l[s][i] = l;
                self.out_r[s][i] = r;
            }
            if bad {
                self.out_l[s].fill(0.0);
                self.out_r[s].fill(0.0);
                *fl = StemFilter::new(self.sr as f32);
                *fr = StemFilter::new(self.sr as f32);
            }
            fl.sanitize();
            fr.sanitize();
        }
        for d in self.decks.iter_mut().chain(std::iter::once(&mut self.sting)) {
            for stem in &mut d.stem_svf {
                for f in stem {
                    f.sanitize();
                }
            }
        }
        self.sample += CONTROL as u64;
    }

    /// Render interleaved stereo (`out.len() / 2` frames) — the sum of all stems through a gentle
    /// safety limiter. Never allocates; output is identical regardless of how calls are sized.
    pub fn render(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        let mut i = 0;
        while i < frames {
            if self.pos >= CONTROL {
                self.process_block();
                self.pos = 0;
            }
            let n = (CONTROL - self.pos).min(frames - i);
            for k in 0..n {
                let j = self.pos + k;
                let mut l = 0.0;
                let mut r = 0.0;
                for s in 0..OUT_STEMS {
                    l += self.out_l[s][j];
                    r += self.out_r[s][j];
                }
                if let Some(o) = out.get_mut(2 * (i + k)..2 * (i + k) + 2) {
                    o[0] = soft_clip(l);
                    o[1] = soft_clip(r);
                }
            }
            self.pos += n;
            i += n;
        }
        if out.len() % 2 == 1
            && let Some(last) = out.last_mut()
        {
            *last = 0.0;
        }
    }

    /// Render the four stems separately (each interleaved stereo, equal lengths): base, drive,
    /// lead, stinger. Post intensity gains, pre limiter. `render` == limiter(sum of these).
    pub fn render_stems(&mut self, base: &mut [f32], drive: &mut [f32], lead: &mut [f32], stinger: &mut [f32]) {
        let frames = base.len().min(drive.len()).min(lead.len()).min(stinger.len()) / 2;
        let mut outs: [&mut [f32]; OUT_STEMS] = [base, drive, lead, stinger];
        let mut i = 0;
        while i < frames {
            if self.pos >= CONTROL {
                self.process_block();
                self.pos = 0;
            }
            let n = (CONTROL - self.pos).min(frames - i);
            for (s, o) in outs.iter_mut().enumerate() {
                for k in 0..n {
                    let j = self.pos + k;
                    if let Some(d) = o.get_mut(2 * (i + k)..2 * (i + k) + 2) {
                        d[0] = self.out_l[s][j];
                        d[1] = self.out_r[s][j];
                    }
                }
            }
            self.pos += n;
            i += n;
        }
    }
}

/// Transparent below -2 dBFS, smooth knee to a hard ceiling of about -0.1 dBFS.
#[inline]
fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.794;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        (KNEE + (1.0 - KNEE) * 0.99 * over.tanh()).copysign(x)
    }
}
