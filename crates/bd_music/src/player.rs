//! The real-time player: sample-exact integer sequencer, two crossfading track decks + one stinger
//! deck, adaptive stem gains, bar/beat quantisation. Everything is pre-allocated in
//! [`MusicPlayer::new`]; [`MusicPlayer::render`] / [`MusicPlayer::command`] never allocate.

use std::sync::Arc;

use crate::notation::PPQ;
use crate::song::{MAX_CHANNELS, MusicLibrary, Song, StingerKind, TrackId};
use crate::synth::{CONTROL, Drum, DrumVoice, StemFilter, Voice};

/// Number of output stems: base, drive, lead, stinger.
pub const OUT_STEMS: usize = 4;
/// Longest echo delay (seconds).
const MAX_ECHO_SECS: f32 = 1.2;
/// Stem fade-in time when a layer enters on a bar line (the downbeat must land).
const FADE_IN_SECS: f32 = 0.03;
/// Deck fade when a track is cut on a bar line.
const CUT_FADE_SECS: f32 = 0.25;

/// A command for the player. `Copy`, so it can travel through any lock-free queue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MusicCommand {
    /// Switch to a track on the next bar line of the current one (immediately if silent).
    Play { track: TrackId, skip_intro: bool },
    /// Stop on the next bar line (voices release naturally).
    Stop,
    /// 0 = calm (base only) … 0.15+ lead … 0.55+ drive (per-track thresholds). Applied on bar lines.
    SetIntensity(f32),
    /// Beat/bar-quantised musical punctuation.
    Stinger(StingerKind),
    /// Tempo multiplier (sudden death / final stock), ramped over `ramp_secs`. Pitch is unaffected.
    SetTempo { multiplier: f32, ramp_secs: f32 },
}

/// Snapshot of the player for UI / beat-synced visuals (cheap, `Copy`).
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
    sl: [[f32; CONTROL]; 3],
    sr_: [[f32; CONTROL]; 3],
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
            sl: [[0.0; CONTROL]; 3],
            sr_: [[0.0; CONTROL]; 3],
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

    fn apply_intensity(&mut self, song: &Song, intensity: f32, log: &mut DebugLog, at: u64) {
        let targets = song.layers.gains(intensity);
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
        if main && self.tick.is_multiple_of(song.beat_ticks) {
            log.beat(now);
            self.beat = (self.tick % song.bar_ticks) / song.beat_ticks.max(1);
            if let Some(k) = ctl.sting_pending.filter(|k| !k.on_bar()) {
                ctl.sting_at = Some((offset, k, self.cur_bpm));
                ctl.sting_pending = None;
            }
        }
        let Some(sec) = song.sections.get(self.section) else {
            self.stop(0.05);
            return;
        };
        let transpose = self.transpose;
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
                    if let Some(d) = ch.drum.get_mut(ch.drum_cur) {
                        d.hit(Drum::from_id(n.key), vel);
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
            self.render_span(pos, span, out_l, out_r, to_stinger);
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
            && (self.fade.cur <= 0.0 || !self.chans.iter().take(self.nchan).any(|c| c.active() || c.quiet < c.delay_len))
        {
            self.src = None;
        }
    }

    fn render_span(
        &mut self,
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
        for ch in self.chans.iter_mut().take(self.nchan) {
            let echo_live = ch.echo && ch.quiet < ch.delay_len + span;
            if !ch.active() && !echo_live {
                if ch.echo {
                    ch.quiet = ch.quiet.saturating_add(span);
                }
                continue;
            }
            let mono = &mut self.scratch[..span];
            if ch.drums {
                mono.fill(0.0);
                for d in &mut ch.drum {
                    d.render_add(mono, 1.0);
                }
            } else {
                ch.voice.render(mono);
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
        }
        // Stem gains / duck / deck fade, then into the output stems.
        let gain = self.gain;
        for i in 0..span {
            let f = self.fade.next() * gain;
            for s in 0..3 {
                let g = self.stem_gain[s].next() * self.duck[s].next() * f;
                let o = if to_stinger { 3 } else { s };
                out_l[o][pos + i] += self.sl[s][i] * if to_stinger { f } else { g };
                out_r[o][pos + i] += self.sr_[s][i] * if to_stinger { f } else { g };
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
}

impl MusicPlayer {
    /// Build a player. Allocates all voice/echo state up front (≈ 5.5 MB at 48 kHz, mostly the 1.2 s\n    /// echo lines of 3 decks × 8 channels); nothing is allocated afterwards.
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
        }
    }

    pub fn library(&self) -> &Arc<MusicLibrary> {
        &self.lib
    }

    pub fn sample_rate(&self) -> u32 {
        self.sr
    }

    /// Apply a command (never allocates; safe on the audio thread).
    pub fn command(&mut self, cmd: MusicCommand) {
        match cmd {
            MusicCommand::Play { track, skip_intro } => {
                if self.lib.track(track).is_none() {
                    return;
                }
                let d = &self.decks[self.active];
                if d.seq && d.src == Some(Src::Track(usize::from(track.0))) {
                    self.pending_play = None;
                    self.pending_stop = false;
                    return;
                }
                self.pending_play = Some((track, skip_intro));
                self.pending_stop = false;
            }
            MusicCommand::Stop => {
                self.pending_play = None;
                self.pending_stop = true;
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
    pub fn stop(&mut self) {
        self.command(MusicCommand::Stop);
    }

    pub fn status(&self) -> MusicStatus {
        let d = &self.decks[self.active];
        let track = match d.src {
            Some(Src::Track(i)) => Some(TrackId(i as u16)),
            _ => None,
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
        let d = &mut self.decks[deck];
        let pos = if skip_intro { song.loop_idx } else { 0 };
        d.start(Src::Track(usize::from(track.0)), song, pos, delay);
        let g = song.layers.gains(self.intensity);
        d.stem_gain = [Ramp::at(g[0]), Ramp::at(g[1]), Ramp::at(g[2])];
        d.update_rate(song, self.tempo.cur);
        d.update_echo();
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
