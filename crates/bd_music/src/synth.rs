//! The synth: band-limited oscillators (4-point BLEP pulse / saw / supersaw), triangle, sine/808,
//! the two-operator "hard light" FM voice, resonant SVFs, NES-style LFSR noise drums and a modern kit. Everything here is `Copy` state that renders
//! into caller-provided slices — no allocation, no locks, fully deterministic.

use serde::Deserialize;

/// Longest span rendered between two control-rate updates (pitch, vibrato, arps, duty, FM index).
pub const CONTROL: usize = 32;

/// Base amplitude of one full-volume voice before instrument / channel / song gain.
pub const VOICE_GAIN: f32 = 0.22;

const TAU: f32 = std::f32::consts::TAU;

/// `sin(2π·x)` for `x` in turns (any value). 9th-order odd polynomial on a folded quarter wave:
/// max error ≈ 4e-6 (-108 dB), no table, no allocation.
#[inline]
pub fn sin_turns(x: f32) -> f32 {
    let mut y = x - x.round(); // [-0.5, 0.5]
    if y > 0.25 {
        y = 0.5 - y;
    } else if y < -0.25 {
        y = -0.5 - y;
    }
    let z = y * TAU;
    let z2 = z * z;
    z * (1.0 + z2 * (-1.0 / 6.0 + z2 * (1.0 / 120.0 + z2 * (-1.0 / 5040.0 + z2 * (1.0 / 362_880.0)))))
}

/// 4-point BLEP residual (integrated cubic B-spline minus the ideal step) for a unit upward step,
/// `x` = signed distance in samples from the step. Much lower aliasing than 2-point PolyBLEP.
#[inline]
fn blep4(x: f32) -> f32 {
    let a = x.abs();
    let r = if a >= 2.0 {
        0.0
    } else if a >= 1.0 {
        let u = 2.0 - a;
        u * u * u * u * (1.0 / 24.0)
    } else {
        let t = -a;
        let t3 = t * t * t;
        1.0 / 24.0 + (4.0 * t - 2.0 * t3 - 0.75 * t3 * t + 2.75) * (1.0 / 6.0)
    };
    if x < 0.0 { r } else { -r }
}

/// BLEP correction for a step that occurs at phase 0 of a phase ramp `t` in [0,1) with increment
/// `dt`: covers the two samples after the last step and the two before the next one.
#[inline]
fn edge(t: f32, dt: f32) -> f32 {
    let mut c = 0.0;
    let since = t / dt;
    if since < 2.0 {
        c += blep4(since);
    }
    let until = (1.0 - t) / dt;
    if until < 2.0 {
        c += blep4(-until);
    }
    c
}

#[inline]
pub fn midi_hz(key: f32) -> f32 {
    440.0 * ((key - 69.0) / 12.0).exp2()
}

/// Oscillator shape of a tonal instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
pub enum Wave {
    /// NES-style pulse (duty 12.5 / 25 / 50 %, sweepable), 4-point-BLEP band-limited, DC-free.
    #[default]
    Pulse,
    /// Smooth triangle (the chip bass).
    Triangle,
    /// Band-limited saw (synthwave bass / brass).
    Saw,
    /// Pure sine (sub-free "glass" pads, echo voices; with `pitch_drop` + `drive` it is the 808).
    Sine,
    /// "Hard light": two detuned 2-operator FM pairs, index envelope, band-limited index.
    Fm,
    /// `unison` (1..=7) detuned band-limited saws with stereo `spread` — dual saw at 2, supersaw at 5-7.
    Supersaw,
}

/// Most saws a supersaw voice may stack (the CPU cap).
pub const MAX_UNISON: usize = 7;

/// A compiled instrument (all times in seconds). `Copy` so voices can hold it by value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instrument {
    pub wave: Wave,
    pub duty: f32,
    pub duty_end: f32,
    pub duty_time: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub vol: f32,
    /// Fraction of the written note length before release (articulation).
    pub gate: f32,
    pub vib_depth: f32,
    pub vib_rate: f32,
    pub vib_delay: f32,
    pub arp: [i8; 8],
    pub arp_len: u8,
    pub arp_rate: f32,
    pub pitch_drop: f32,
    pub pitch_time: f32,
    pub glide: f32,
    /// Every note slides from the previous one while the voice is sounding (808 slides).
    pub porta: bool,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub fm_index_end: f32,
    pub fm_decay: f32,
    pub fm_detune: f32,
    pub fm_feedback: f32,
    /// Low-pass cutoff (0 = off), its envelope amount / time.
    pub lp_hz: f32,
    pub lp_env: f32,
    pub lp_time: f32,
    /// Resonance 0..1; > 0 (or `svf`) switches the one-pole to a resonant 12 dB/oct SVF.
    pub res: f32,
    pub svf: bool,
    /// Soft saturation 0..1 (reads on small speakers; peak-normalised).
    pub drive: f32,
    pub unison: u8,
    /// Supersaw detune, total spread in cents.
    pub detune: f32,
    /// Supersaw stereo spread 0..1.
    pub spread: f32,
}

impl Default for Instrument {
    fn default() -> Self {
        Self {
            wave: Wave::Pulse,
            duty: 0.25,
            duty_end: 0.25,
            duty_time: 0.0,
            attack: 0.004,
            decay: 0.25,
            sustain: 0.7,
            release: 0.08,
            vol: 1.0,
            gate: 0.92,
            vib_depth: 0.0,
            vib_rate: 5.5,
            vib_delay: 0.25,
            arp: [0; 8],
            arp_len: 0,
            arp_rate: 60.0,
            pitch_drop: 0.0,
            pitch_time: 0.03,
            glide: 0.06,
            porta: false,
            fm_ratio: 1.0,
            fm_index: 1.0,
            fm_index_end: 1.0,
            fm_decay: 0.3,
            fm_detune: 0.0,
            fm_feedback: 0.0,
            lp_hz: 0.0,
            lp_env: 0.0,
            lp_time: 0.1,
            res: 0.0,
            svf: false,
            drive: 0.0,
            unison: 1,
            detune: 0.0,
            spread: 0.0,
        }
    }
}

/// Smooth saturator (tanh Padé), exact ±1 beyond |x| = 3.
#[inline]
pub fn soft(x: f32) -> f32 {
    if x >= 3.0 {
        1.0
    } else if x <= -3.0 {
        -1.0
    } else {
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Attack,
    Decay,
    Release,
    Off,
}

const UNISON_PHASES: [f32; MAX_UNISON] = [0.0, 0.31, 0.62, 0.13, 0.84, 0.47, 0.71];

/// One monophonic tonal voice (a chip channel). Chords are played as classic chip arpeggios.
#[derive(Clone, Copy, Debug)]
pub struct Voice {
    inst: Instrument,
    sr: f32,
    // pitch
    chord: [f32; 4],
    chord_n: u8,
    arp_idx: u8,
    arp_timer: f32,
    from_key: f32,
    cur_key: f32,
    glide_t: f32,
    glide_dur: f32,
    age: f32,
    // oscillators
    phase: [f32; 2],
    mphase: [f32; 2],
    mfb: [f32; 2],
    dt: [f32; 2],
    mdt: [f32; 2],
    uphase: [f32; MAX_UNISON],
    udt: [f32; MAX_UNISON],
    upan: [f32; MAX_UNISON],
    duty: f32,
    index: f32,
    // filter / shaping
    lp_a: f32,
    lp_z: f32,
    lp_z2: f32,
    use_svf: bool,
    svf: [Svf; 2],
    sat_norm: f32,
    // envelope
    stage: Stage,
    env: f32,
    peak: f32,
    sus: f32,
    att_step: f32,
    dec_coef: f32,
    rel_coef: f32,
}

impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            inst: Instrument::default(),
            sr: sample_rate,
            chord: [60.0; 4],
            chord_n: 1,
            arp_idx: 0,
            arp_timer: 0.0,
            from_key: 60.0,
            cur_key: 60.0,
            glide_t: 0.0,
            glide_dur: 0.0,
            age: 0.0,
            phase: [0.0, 0.37],
            mphase: [0.0, 0.61],
            mfb: [0.0; 2],
            dt: [0.0; 2],
            mdt: [0.0; 2],
            uphase: UNISON_PHASES,
            udt: [0.0; MAX_UNISON],
            upan: [0.0; MAX_UNISON],
            duty: 0.25,
            index: 0.0,
            lp_a: 1.0,
            lp_z: 0.0,
            lp_z2: 0.0,
            use_svf: false,
            svf: [Svf::default(); 2],
            sat_norm: 1.0,
            stage: Stage::Off,
            env: 0.0,
            peak: 0.0,
            sus: 0.0,
            att_step: 1.0,
            dec_coef: 0.0,
            rel_coef: 0.0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Off
    }

    /// Start a note (or chord arpeggio). Re-triggers from the current envelope level and keeps the
    /// oscillator phase, so legato notes never click. `glide` slides from the previous pitch
    /// (instruments with `porta` always do while the voice is sounding).
    pub fn note_on(&mut self, inst: &Instrument, keys: &[f32], vel: f32, glide: bool) {
        let n = keys.len().clamp(1, 4);
        let was_active = self.is_active();
        self.inst = *inst;
        for (i, slot) in self.chord.iter_mut().enumerate() {
            *slot = keys.get(i.min(n - 1)).copied().unwrap_or(60.0);
        }
        self.chord_n = n as u8;
        self.arp_idx = 0;
        self.arp_timer = 0.0;
        if (glide || inst.porta) && was_active {
            self.from_key = self.cur_key;
            self.glide_dur = inst.glide.max(0.005);
        } else {
            self.from_key = self.chord[0];
            self.glide_dur = 0.0;
        }
        self.glide_t = 0.0;
        self.age = 0.0;
        if !was_active {
            self.env = 0.0;
            self.lp_z = 0.0;
            self.lp_z2 = 0.0;
            self.svf = [Svf::default(); 2];
        }
        let sr = self.sr;
        self.peak = (vel * inst.vol).max(0.0);
        self.sus = self.peak * inst.sustain.clamp(0.0, 1.0);
        self.att_step = if inst.attack > 0.0 { self.peak / (inst.attack * sr) } else { self.peak.max(1e-6) };
        self.dec_coef = if inst.decay > 0.0 { (-6.9 / (inst.decay * sr)).exp() } else { 0.0 };
        self.rel_coef = (-6.9 / (inst.release.max(0.002) * sr)).exp();
        self.stage = if self.env < self.peak { Stage::Attack } else { Stage::Decay };
        self.sat_norm = if inst.drive > 0.0 { 1.0 / soft(1.0 + 3.0 * inst.drive) } else { 1.0 };
    }

    pub fn note_off(&mut self) {
        if self.stage != Stage::Off {
            self.stage = Stage::Release;
        }
    }

    /// Fast fade (used when a deck is cut): release over ~15 ms regardless of the instrument.
    pub fn kill(&mut self) {
        if self.stage != Stage::Off {
            self.stage = Stage::Release;
            self.rel_coef = self.rel_coef.min((-6.9 / (0.015 * self.sr)).exp());
        }
    }

    /// Control-rate update (once per span of at most [`CONTROL`] samples): arp step, glide,
    /// vibrato, pitch drop, duty sweep, FM index envelope, filter envelope, unison detune.
    fn control(&mut self, span_secs: f32, pitch_mod_cents: f32) {
        let inst = &self.inst;
        // Arpeggio: chord notes cycle; a single note may cycle the instrument's arp macro.
        let steps = if self.chord_n > 1 { self.chord_n } else { inst.arp_len };
        if steps > 1 && inst.arp_rate > 0.0 {
            let period = 1.0 / inst.arp_rate;
            while self.arp_timer >= period {
                self.arp_timer -= period;
                self.arp_idx = (self.arp_idx + 1) % steps;
            }
            self.arp_timer += span_secs;
        }
        let target = if self.chord_n > 1 {
            self.chord[(self.arp_idx as usize).min(3)]
        } else if inst.arp_len > 0 {
            self.chord[0] + f32::from(inst.arp[(self.arp_idx as usize).min(7)])
        } else {
            self.chord[0]
        };
        let mut key = if self.glide_t < self.glide_dur {
            let x = self.glide_t / self.glide_dur;
            let x = x * x * (3.0 - 2.0 * x);
            self.from_key + (target - self.from_key) * x
        } else {
            target
        };
        self.glide_t += span_secs;
        self.cur_key = key;
        if inst.vib_depth > 0.0 && self.age > inst.vib_delay {
            let t = self.age - inst.vib_delay;
            let ramp = (t / 0.2).min(1.0);
            key += inst.vib_depth * 0.01 * ramp * sin_turns(inst.vib_rate * t);
        }
        if inst.pitch_drop != 0.0 {
            key += inst.pitch_drop * (-self.age / inst.pitch_time.max(1e-3)).exp();
        }
        if pitch_mod_cents != 0.0 {
            key += pitch_mod_cents * 0.01;
        }
        let hz = midi_hz(key).min(self.sr * 0.45);
        let inv_sr = 1.0 / self.sr;
        match inst.wave {
            Wave::Fm => {
                let det = (inst.fm_detune / 1200.0).exp2();
                let f = [hz / det.sqrt(), hz * det.sqrt()];
                self.dt = f.map(|x| x * inv_sr);
                self.mdt = f.map(|x| x * inst.fm_ratio * inv_sr);
                let k = (-self.age / inst.fm_decay.max(1e-3)).exp();
                let idx = inst.fm_index_end + (inst.fm_index - inst.fm_index_end) * k;
                // Band limit (Carson): keep carrier + (I+1)·fm below ~10 kHz.
                let fm = (hz * inst.fm_ratio).max(1.0);
                let max_idx = ((10_000.0 - hz) / fm - 1.0).max(0.0);
                self.index = idx.clamp(0.0, max_idx);
            }
            Wave::Supersaw => {
                let n = usize::from(inst.unison).clamp(1, MAX_UNISON);
                for i in 0..n {
                    let pos = if n == 1 { 0.0 } else { i as f32 / (n - 1) as f32 * 2.0 - 1.0 };
                    let ratio = (pos * inst.detune * 0.5 / 1200.0).exp2();
                    self.udt[i] = (hz * ratio).min(self.sr * 0.45) * inv_sr;
                    self.upan[i] = pos * inst.spread;
                }
                self.dt = [hz * inv_sr; 2];
            }
            _ => {
                self.dt = [hz * inv_sr; 2];
            }
        }
        if inst.duty_time > 0.0 {
            let x = (self.age / inst.duty_time).min(1.0);
            self.duty = inst.duty + (inst.duty_end - inst.duty) * x;
        } else {
            self.duty = inst.duty;
        }
        self.duty = self.duty.clamp(0.05, 0.95);
        self.use_svf = inst.lp_hz > 0.0 && (inst.svf || inst.res > 0.0);
        if inst.lp_hz > 0.0 {
            let fc = inst.lp_hz * (1.0 + inst.lp_env * (-self.age / inst.lp_time.max(1e-3)).exp());
            let fc = fc.min(self.sr * 0.45);
            if self.use_svf {
                for f in &mut self.svf {
                    f.set(fc, inst.res, self.sr);
                }
                self.lp_a = 1.0;
            } else {
                self.lp_a = 1.0 - (-TAU * fc * inv_sr).exp();
            }
        } else {
            self.lp_a = 1.0;
        }
    }

    #[inline]
    fn env_step(&mut self) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.env += self.att_step;
                if self.env >= self.peak {
                    self.env = self.peak;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.env = self.sus + (self.env - self.sus) * self.dec_coef;
            }
            Stage::Release => {
                self.env *= self.rel_coef;
                if self.env < 1e-5 {
                    self.env = 0.0;
                    self.stage = Stage::Off;
                }
            }
            Stage::Off => {}
        }
        self.env
    }

    /// Saturation (on the enveloped, not yet scaled signal).
    #[inline]
    fn shape(&self, x: f32) -> f32 {
        if self.inst.drive > 0.0 { soft(x * (1.0 + 3.0 * self.inst.drive)) * self.sat_norm } else { x }
    }

    /// The voice filter (one-pole, or resonant SVF), channel 0 = mid, 1 = side.
    #[inline]
    fn filt(&mut self, x: f32, lp_a: f32) -> f32 {
        if self.use_svf {
            self.svf[0].lp(x)
        } else {
            self.lp_z += lp_a * (x - self.lp_z);
            self.lp_z
        }
    }

    /// Render (overwrite) `out` with this voice's mono signal (at most [`CONTROL`] samples).
    pub fn render(&mut self, out: &mut [f32]) {
        let mut side = [0.0f32; CONTROL];
        let n = out.len().min(CONTROL);
        self.render_stereo(&mut out[..n], &mut side[..n], 0.0);
    }

    /// Render the mid signal into `out` and (supersaw only, returns `true`) the stereo side signal
    /// into `side`. `pitch_mod_cents` detunes the whole voice (lo-fi wow).
    pub fn render_stereo(&mut self, out: &mut [f32], side: &mut [f32], pitch_mod_cents: f32) -> bool {
        if self.stage == Stage::Off {
            out.fill(0.0);
            return false;
        }
        let span = out.len() as f32 / self.sr;
        self.control(span, pitch_mod_cents);
        self.age += span;
        let gain = VOICE_GAIN;
        let lp_a = self.lp_a;
        match self.inst.wave {
            Wave::Pulse => {
                let (dt, d) = (self.dt[0], self.duty);
                let dc = 2.0 * d - 1.0;
                for o in out.iter_mut() {
                    let t = self.phase[0];
                    let mut y = if t < d { 1.0 } else { -1.0 };
                    y += 2.0 * edge(t, dt);
                    let mut t2 = t - d;
                    if t2 < 0.0 {
                        t2 += 1.0;
                    }
                    y -= 2.0 * edge(t2, dt);
                    self.phase[0] = wrap(t + dt);
                    let e = self.env_step();
                    let x = self.shape((y - dc) * e) * gain;
                    *o = self.filt(x, lp_a);
                }
            }
            Wave::Saw => {
                let dt = self.dt[0];
                for o in out.iter_mut() {
                    let t = self.phase[0];
                    let y = 2.0 * t - 1.0 - 2.0 * edge(t, dt);
                    self.phase[0] = wrap(t + dt);
                    let e = self.env_step();
                    let x = self.shape(y * e) * gain;
                    *o = self.filt(x, lp_a);
                }
            }
            Wave::Triangle => {
                let dt = self.dt[0];
                for o in out.iter_mut() {
                    let t = self.phase[0];
                    let y = 1.0 - 4.0 * (t - 0.5).abs();
                    self.phase[0] = wrap(t + dt);
                    let e = self.env_step();
                    let x = self.shape(y * e) * gain * 1.25;
                    *o = self.filt(x, lp_a);
                }
            }
            Wave::Sine => {
                let dt = self.dt[0];
                for o in out.iter_mut() {
                    let t = self.phase[0];
                    self.phase[0] = wrap(t + dt);
                    let e = self.env_step();
                    let x = self.shape(sin_turns(t) * e) * gain * 1.2;
                    *o = self.filt(x, lp_a);
                }
            }
            Wave::Fm => {
                let idx = self.index * (1.0 / TAU);
                let fb = self.inst.fm_feedback * (1.0 / TAU);
                for o in out.iter_mut() {
                    let mut y = 0.0;
                    for i in 0..2 {
                        let m = sin_turns(self.mphase[i] + fb * self.mfb[i]);
                        self.mfb[i] = m;
                        y += sin_turns(self.phase[i] + idx * m);
                        self.phase[i] = wrap(self.phase[i] + self.dt[i]);
                        self.mphase[i] = wrap(self.mphase[i] + self.mdt[i]);
                    }
                    let e = self.env_step();
                    let x = self.shape(y * 0.6 * e) * gain;
                    *o = self.filt(x, lp_a);
                }
            }
            Wave::Supersaw => {
                let n = usize::from(self.inst.unison).clamp(1, MAX_UNISON);
                let norm = 1.0 / (n as f32).sqrt();
                for (o, s) in out.iter_mut().zip(side.iter_mut()) {
                    let e = self.env_step();
                    let (mut m, mut sd) = (0.0, 0.0);
                    for i in 0..n {
                        let t = self.uphase[i];
                        let dt = self.udt[i];
                        let y = 2.0 * t - 1.0 - 2.0 * edge(t, dt);
                        self.uphase[i] = wrap(t + dt);
                        m += y;
                        sd += y * self.upan[i];
                    }
                    let x = self.shape(m * norm * e) * gain;
                    let xs = sd * norm * e * gain;
                    *o = self.filt(x, lp_a);
                    *s = if self.use_svf {
                        self.svf[1].lp(xs)
                    } else {
                        self.lp_z2 += lp_a * (xs - self.lp_z2);
                        self.lp_z2
                    };
                }
                return true;
            }
        }
        false
    }
}

#[inline]
fn wrap(x: f32) -> f32 {
    if x >= 1.0 { x - x.floor() } else { x }
}

// ------------------------------------------------------------------------------------------------
// Drums
// ------------------------------------------------------------------------------------------------

/// The drum kit of a noise channel.
///
/// Chip kit (NES LFSR): `k s h o c t m x n` — kick, snare, closed hat, open hat, crash, low tom,
/// mid tom, clap, metallic blip.
/// Modern kit: `K` punchy synth kick, `S` snare (noise + band-pass + body), `X` clap, `G` gated
/// snare (holds for the written length), `H` / `O` closed / open hat (a hit on the same channel
/// chokes the previous one), `C` crash, `b` cowbell, `W` white-noise riser and `R` reverse cymbal
/// (both swell over the written length), `I` impact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Drum {
    Kick = 0,
    Snare = 1,
    Hat = 2,
    OpenHat = 3,
    Crash = 4,
    TomLow = 5,
    TomMid = 6,
    Clap = 7,
    Blip = 8,
    SynthKick = 9,
    Snare2 = 10,
    Clap2 = 11,
    Gated = 12,
    Hat2 = 13,
    OpenHat2 = 14,
    Crash2 = 15,
    Cowbell = 16,
    Riser = 17,
    RevCymbal = 18,
    Impact = 19,
}

impl Drum {
    pub fn from_letter(c: char) -> Option<Drum> {
        Some(match c {
            'k' => Drum::Kick,
            's' => Drum::Snare,
            'h' => Drum::Hat,
            'o' => Drum::OpenHat,
            'c' => Drum::Crash,
            't' => Drum::TomLow,
            'm' => Drum::TomMid,
            'x' => Drum::Clap,
            'n' => Drum::Blip,
            'K' => Drum::SynthKick,
            'S' => Drum::Snare2,
            'X' => Drum::Clap2,
            'G' => Drum::Gated,
            'H' => Drum::Hat2,
            'O' => Drum::OpenHat2,
            'C' => Drum::Crash2,
            'b' => Drum::Cowbell,
            'W' => Drum::Riser,
            'R' => Drum::RevCymbal,
            'I' => Drum::Impact,
            _ => return None,
        })
    }
    pub fn from_id(id: u8) -> Drum {
        match id {
            0 => Drum::Kick,
            1 => Drum::Snare,
            2 => Drum::Hat,
            3 => Drum::OpenHat,
            4 => Drum::Crash,
            5 => Drum::TomLow,
            6 => Drum::TomMid,
            7 => Drum::Clap,
            9 => Drum::SynthKick,
            10 => Drum::Snare2,
            11 => Drum::Clap2,
            12 => Drum::Gated,
            13 => Drum::Hat2,
            14 => Drum::OpenHat2,
            15 => Drum::Crash2,
            16 => Drum::Cowbell,
            17 => Drum::Riser,
            18 => Drum::RevCymbal,
            19 => Drum::Impact,
            _ => Drum::Blip,
        }
    }
    /// Kicks trigger the sidechain pump.
    pub fn is_kick(self) -> bool {
        matches!(self, Drum::Kick | Drum::SynthKick)
    }
}

/// NES noise timer periods (CPU cycles, NTSC) — index 0 = brightest.
const NES_NOISE_PERIODS: [f32; 16] =
    [4.0, 8.0, 16.0, 32.0, 64.0, 96.0, 128.0, 160.0, 202.0, 254.0, 380.0, 508.0, 762.0, 1016.0, 2034.0, 4068.0];
const NES_CPU_HZ: f32 = 1_789_773.0;

fn noise_clock(period_idx: f32) -> f32 {
    let i = period_idx.clamp(0.0, 15.0);
    let lo = i.floor() as usize;
    let hi = (lo + 1).min(15);
    let f = i - lo as f32;
    let pl = NES_NOISE_PERIODS.get(lo).copied().unwrap_or(4068.0).ln();
    let ph = NES_NOISE_PERIODS.get(hi).copied().unwrap_or(4068.0).ln();
    NES_CPU_HZ / (pl + (ph - pl) * f).exp()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filt {
    None,
    Lp,
    Bp,
    Hp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrumEnv {
    /// Exponential decays (every chip drum, most modern ones).
    Decay,
    /// Swell over the written note length, then stop (risers).
    Rise,
    /// Full level for the written note length, then a fast gate (gated snare).
    Hold,
}

#[derive(Clone, Copy, Debug)]
struct DrumRecipe {
    tone: f32,
    tone_hz: f32,
    tone_end_hz: f32,
    tone_sweep: f32,
    tone_decay: f32,
    square: bool,
    tone2_hz: f32,
    noise: f32,
    noise_idx: f32,
    noise_end_idx: f32,
    noise_sweep: f32,
    noise_decay: f32,
    short_mode: bool,
    /// Extra re-triggers of the noise envelope (clap).
    flams: u8,
    flam_gap: f32,
    /// White (xorshift) noise instead of the NES LFSR.
    white: bool,
    filt: Filt,
    f_hz: f32,
    f_end_hz: f32,
    f_res: f32,
    /// Filter the tone too (cowbell), not only the noise.
    filter_tone: bool,
    env: DrumEnv,
    drive: f32,
    /// Short bright transient at the onset.
    click: f32,
}

fn recipe(d: Drum) -> DrumRecipe {
    let base = DrumRecipe {
        tone: 0.0,
        tone_hz: 100.0,
        tone_end_hz: 100.0,
        tone_sweep: 0.05,
        tone_decay: 0.1,
        square: false,
        tone2_hz: 0.0,
        noise: 0.0,
        noise_idx: 4.0,
        noise_end_idx: 4.0,
        noise_sweep: 0.05,
        noise_decay: 0.1,
        short_mode: false,
        flams: 0,
        flam_gap: 0.011,
        white: false,
        filt: Filt::None,
        f_hz: 1000.0,
        f_end_hz: 1000.0,
        f_res: 0.0,
        filter_tone: false,
        env: DrumEnv::Decay,
        drive: 0.0,
        click: 0.0,
    };
    let modern = DrumRecipe { white: true, ..base };
    match d {
        Drum::Kick => DrumRecipe {
            tone: 1.0,
            tone_hz: 170.0,
            tone_end_hz: 66.0,
            tone_sweep: 0.045,
            tone_decay: 0.22,
            noise: 0.22,
            noise_idx: 3.0,
            noise_end_idx: 9.0,
            noise_sweep: 0.012,
            noise_decay: 0.018,
            ..base
        },
        Drum::Snare => DrumRecipe {
            tone: 0.45,
            tone_hz: 230.0,
            tone_end_hz: 175.0,
            tone_sweep: 0.03,
            tone_decay: 0.07,
            noise: 0.75,
            noise_idx: 3.0,
            noise_end_idx: 6.0,
            noise_sweep: 0.12,
            noise_decay: 0.16,
            ..base
        },
        Drum::Hat => DrumRecipe { noise: 0.32, noise_idx: 1.0, noise_end_idx: 1.0, noise_decay: 0.035, ..base },
        Drum::OpenHat => DrumRecipe { noise: 0.28, noise_idx: 1.0, noise_end_idx: 2.0, noise_sweep: 0.2, noise_decay: 0.2, ..base },
        Drum::Crash => DrumRecipe { noise: 0.42, noise_idx: 2.0, noise_end_idx: 4.0, noise_sweep: 0.9, noise_decay: 1.1, ..base },
        Drum::TomLow => DrumRecipe {
            tone: 0.9,
            tone_hz: 150.0,
            tone_end_hz: 92.0,
            tone_sweep: 0.12,
            tone_decay: 0.22,
            noise: 0.12,
            noise_idx: 6.0,
            noise_end_idx: 9.0,
            noise_decay: 0.03,
            ..base
        },
        Drum::TomMid => DrumRecipe {
            tone: 0.85,
            tone_hz: 220.0,
            tone_end_hz: 140.0,
            tone_sweep: 0.1,
            tone_decay: 0.18,
            noise: 0.12,
            noise_idx: 5.0,
            noise_end_idx: 8.0,
            noise_decay: 0.03,
            ..base
        },
        Drum::Clap => DrumRecipe { noise: 0.6, noise_idx: 4.0, noise_end_idx: 5.0, noise_sweep: 0.1, noise_decay: 0.11, flams: 2, ..base },
        Drum::Blip => DrumRecipe { noise: 0.35, noise_idx: 5.0, noise_end_idx: 7.0, noise_sweep: 0.06, noise_decay: 0.07, short_mode: true, ..base },
        Drum::SynthKick => DrumRecipe {
            tone: 1.15,
            tone_hz: 210.0,
            tone_end_hz: 46.0,
            tone_sweep: 0.07,
            tone_decay: 0.5,
            drive: 0.55,
            click: 0.35,
            ..modern
        },
        Drum::Snare2 => DrumRecipe {
            tone: 0.5,
            tone_hz: 200.0,
            tone_end_hz: 178.0,
            tone_sweep: 0.03,
            tone_decay: 0.1,
            noise: 0.85,
            noise_decay: 0.2,
            filt: Filt::Bp,
            f_hz: 3200.0,
            f_end_hz: 2400.0,
            f_res: 0.2,
            drive: 0.25,
            ..modern
        },
        Drum::Clap2 => DrumRecipe {
            noise: 1.1,
            noise_decay: 0.17,
            flams: 3,
            flam_gap: 0.009,
            filt: Filt::Bp,
            f_hz: 1400.0,
            f_end_hz: 1200.0,
            f_res: 0.45,
            ..modern
        },
        Drum::Gated => DrumRecipe {
            tone: 0.45,
            tone_hz: 190.0,
            tone_end_hz: 170.0,
            tone_sweep: 0.03,
            tone_decay: 0.12,
            noise: 0.7,
            noise_decay: 1.0,
            filt: Filt::Bp,
            f_hz: 2000.0,
            f_end_hz: 1700.0,
            f_res: 0.2,
            env: DrumEnv::Hold,
            ..modern
        },
        Drum::Hat2 => DrumRecipe { noise: 0.42, noise_decay: 0.045, filt: Filt::Hp, f_hz: 7500.0, f_end_hz: 7500.0, f_res: 0.2, ..modern },
        Drum::OpenHat2 => {
            DrumRecipe { noise: 0.36, noise_decay: 0.32, filt: Filt::Hp, f_hz: 6500.0, f_end_hz: 6000.0, f_res: 0.15, ..modern }
        }
        Drum::Crash2 => {
            DrumRecipe { noise: 0.4, noise_decay: 1.5, filt: Filt::Hp, f_hz: 4200.0, f_end_hz: 3200.0, noise_sweep: 1.0, ..modern }
        }
        Drum::Cowbell => DrumRecipe {
            tone: 0.55,
            tone_hz: 587.0,
            tone_end_hz: 587.0,
            tone2_hz: 845.0,
            square: true,
            tone_decay: 0.28,
            filt: Filt::Bp,
            f_hz: 2100.0,
            f_end_hz: 2100.0,
            f_res: 0.3,
            filter_tone: true,
            ..modern
        },
        Drum::Riser => DrumRecipe {
            noise: 0.5,
            filt: Filt::Bp,
            f_hz: 300.0,
            f_end_hz: 9000.0,
            f_res: 0.55,
            env: DrumEnv::Rise,
            ..modern
        },
        Drum::RevCymbal => DrumRecipe {
            noise: 0.42,
            filt: Filt::Hp,
            f_hz: 2500.0,
            f_end_hz: 6000.0,
            f_res: 0.1,
            env: DrumEnv::Rise,
            ..modern
        },
        Drum::Impact => DrumRecipe {
            tone: 1.1,
            tone_hz: 120.0,
            tone_end_hz: 38.0,
            tone_sweep: 0.45,
            tone_decay: 1.6,
            noise: 0.55,
            noise_decay: 0.7,
            filt: Filt::Lp,
            f_hz: 3000.0,
            f_end_hz: 600.0,
            noise_sweep: 0.7,
            drive: 0.4,
            click: 0.3,
            ..modern
        },
    }
}

/// One drum hit being rendered (a noise channel keeps the current hit plus a fading tail).
#[derive(Clone, Copy, Debug)]
pub struct DrumVoice {
    sr: f32,
    r: DrumRecipe,
    active: bool,
    age: f32,
    vel: f32,
    len: f32,
    pitch: f32,
    tone_phase: f32,
    tone2_phase: f32,
    tone_amp: f32,
    tone_coef: f32,
    noise_amp: f32,
    noise_coef: f32,
    lfsr: u16,
    rng: u32,
    noise_acc: f32,
    noise_out: f32,
    kill: f32,
    flam_left: u8,
    flam_t: f32,
    click_amp: f32,
    click_coef: f32,
    svf: Svf,
    ended: bool,
    rise: f32,
    /// 0.8 ms onset ramp: keeps the punch, removes the step click.
    att: f32,
    att_step: f32,
    sat_norm: f32,
}

impl DrumVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sr: sample_rate,
            r: recipe(Drum::Hat),
            active: false,
            age: 0.0,
            vel: 0.0,
            len: 0.1,
            pitch: 1.0,
            tone_phase: 0.0,
            tone2_phase: 0.0,
            tone_amp: 0.0,
            tone_coef: 0.0,
            noise_amp: 0.0,
            noise_coef: 0.0,
            lfsr: 1,
            rng: 0x9E37_79B9,
            noise_acc: 0.0,
            noise_out: 1.0,
            kill: 1.0,
            flam_left: 0,
            flam_t: 0.0,
            click_amp: 0.0,
            click_coef: 0.0,
            svf: Svf::default(),
            ended: false,
            rise: 1.0,
            att: 1.0,
            att_step: 1.0 / (0.0008 * sample_rate),
            sat_norm: 1.0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Hit a drum with velocity `vel`, written length `len_secs` (risers / gated snare) and a
    /// pitch offset in semitones (tunes the body and the filters: trap-hat pitch ramps, toms).
    pub fn hit(&mut self, d: Drum, vel: f32, len_secs: f32, pitch_semis: f32) {
        let r = recipe(d);
        self.r = r;
        self.active = true;
        self.age = 0.0;
        self.vel = vel;
        self.len = len_secs.max(0.01);
        self.pitch = (pitch_semis / 12.0).exp2();
        self.tone_phase = 0.0;
        self.tone2_phase = 0.0;
        self.tone_amp = r.tone * vel;
        self.tone_coef = (-6.9 / (r.tone_decay.max(1e-3) * self.sr)).exp();
        self.noise_amp = r.noise * vel;
        self.noise_coef = (-6.9 / (r.noise_decay.max(1e-3) * self.sr)).exp();
        self.kill = 1.0;
        self.flam_left = r.flams;
        self.flam_t = 0.0;
        self.click_amp = r.click * vel;
        self.click_coef = (-6.9 / (0.0025 * self.sr)).exp();
        self.svf = Svf::default();
        self.ended = false;
        self.rise = if r.env == DrumEnv::Rise { 0.0 } else { 1.0 };
        self.att = 0.0;
        self.sat_norm = if r.drive > 0.0 { 1.0 / soft(1.0 + 3.0 * r.drive) } else { 1.0 };
        // LFSR keeps running between hits (like the chip); never let it reach 0.
        if self.lfsr == 0 {
            self.lfsr = 1;
        }
    }

    /// Start a ~12 ms fade-out (the channel was re-triggered: choke).
    pub fn fade_out(&mut self) {
        self.kill = (-6.9 / (0.012 * self.sr)).exp();
    }

    /// Mix (add) this hit into `out`.
    pub fn render_add(&mut self, out: &mut [f32], gain: f32) {
        if !self.active {
            return;
        }
        let r = self.r;
        let span = out.len() as f32 / self.sr;
        let inv_sr = 1.0 / self.sr;
        // Control-rate pitch sweeps.
        let st = (self.age / r.tone_sweep.max(1e-3)).min(1.0);
        let tone_hz = r.tone_hz * (r.tone_end_hz / r.tone_hz).powf(1.0 - (1.0 - st) * (1.0 - st)) * self.pitch;
        let tone_dt = (tone_hz * inv_sr).min(0.45);
        let tone2_dt = (r.tone2_hz * self.pitch * inv_sr).min(0.45);
        let nt = (self.age / r.noise_sweep.max(1e-3)).min(1.0);
        let clock = noise_clock(r.noise_idx + (r.noise_end_idx - r.noise_idx) * nt);
        let steps_per_sample = clock * inv_sr;
        if r.filt != Filt::None {
            let ft = if r.env == DrumEnv::Rise { (self.age / self.len).min(1.0) } else { nt };
            let fc = r.f_hz * (r.f_end_hz / r.f_hz).powf(ft) * self.pitch;
            self.svf.set(fc, r.f_res, self.sr);
        }
        // Envelope shape for risers / gated hits.
        match r.env {
            DrumEnv::Rise => {
                let x = (self.age / self.len).min(1.0);
                self.rise = x * x * x;
            }
            DrumEnv::Hold | DrumEnv::Decay => {}
        }
        if r.env != DrumEnv::Decay && !self.ended && self.age >= self.len {
            self.ended = true;
            self.kill = (-6.9 / (0.02 * self.sr)).exp();
        }
        let hold = r.env != DrumEnv::Decay;
        let g = gain * VOICE_GAIN;
        let kill = self.kill;
        let rise = self.rise;
        for o in out.iter_mut() {
            let nz_raw = if r.white {
                self.rng ^= self.rng << 13;
                self.rng ^= self.rng >> 17;
                self.rng ^= self.rng << 5;
                self.rng as i32 as f32 * (1.0 / 2_147_483_648.0)
            } else {
                // noise: clock the 15-bit LFSR, box-filter the steps inside this sample
                self.noise_acc += steps_per_sample;
                if self.noise_acc >= 1.0 {
                    let mut sum = 0.0;
                    let mut n = 0.0;
                    while self.noise_acc >= 1.0 {
                        let tap = if r.short_mode { 6 } else { 1 };
                        let fb = (self.lfsr ^ (self.lfsr >> tap)) & 1;
                        self.lfsr = (self.lfsr >> 1) | (fb << 14);
                        sum += if self.lfsr & 1 == 0 { 1.0 } else { -1.0 };
                        n += 1.0;
                        self.noise_acc -= 1.0;
                    }
                    self.noise_out = sum / n;
                }
                self.noise_out
            };
            let tone = if self.tone_amp > 1e-5 {
                let s = if r.square {
                    let mut y = square(self.tone_phase, tone_dt);
                    if r.tone2_hz > 0.0 {
                        y = 0.5 * (y + square(self.tone2_phase, tone2_dt));
                        self.tone2_phase = wrap(self.tone2_phase + tone2_dt);
                    }
                    y
                } else {
                    sin_turns(self.tone_phase)
                } * self.tone_amp;
                self.tone_phase = wrap(self.tone_phase + tone_dt);
                if !hold {
                    self.tone_amp *= self.tone_coef;
                }
                s
            } else {
                0.0
            };
            let nz = nz_raw * self.noise_amp;
            if !hold {
                self.noise_amp *= self.noise_coef;
            }
            self.tone_amp *= kill;
            self.noise_amp *= kill;
            self.att = (self.att + self.att_step).min(1.0);
            let mut x = match r.filt {
                Filt::None => tone + nz,
                _ if r.filter_tone => self.filter(tone + nz),
                _ => tone + self.filter(nz),
            };
            if self.click_amp > 1e-5 {
                x += nz_raw * self.click_amp;
                self.click_amp *= self.click_coef;
            }
            if r.drive > 0.0 {
                x = soft(x * (1.0 + 3.0 * r.drive)) * self.sat_norm;
            }
            *o += x * g * self.att * rise;
        }
        self.age += span;
        if self.flam_left > 0 {
            self.flam_t += span;
            if self.flam_t >= r.flam_gap {
                self.flam_t = 0.0;
                self.flam_left -= 1;
                self.noise_amp = self.noise_amp.max(r.noise * self.vel * 0.85);
            }
        }
        let quiet = self.tone_amp < 1e-5 && self.noise_amp < 1e-5 && self.flam_left == 0;
        if quiet && (r.env == DrumEnv::Decay || self.ended) {
            self.active = false;
        }
    }

    #[inline]
    fn filter(&mut self, x: f32) -> f32 {
        match self.r.filt {
            Filt::Lp => self.svf.lp(x),
            Filt::Bp => self.svf.bp(x),
            Filt::Hp => self.svf.hp(x),
            Filt::None => x,
        }
    }
}

/// Band-limited 50 % square (4-point BLEP), phase in turns.
#[inline]
fn square(t: f32, dt: f32) -> f32 {
    let mut y = if t < 0.5 { 1.0 } else { -1.0 };
    y += 2.0 * edge(t, dt);
    let mut t2 = t - 0.5;
    if t2 < 0.0 {
        t2 += 1.0;
    }
    y - 2.0 * edge(t2, dt)
}

// ------------------------------------------------------------------------------------------------
// Filters
// ------------------------------------------------------------------------------------------------

/// Resonant state-variable filter (Simper / Cytomic TPT form): stable under modulation, 12 dB/oct.
#[derive(Clone, Copy, Debug)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}

impl Default for Svf {
    /// Wide open (fc ≈ 20 kHz at 48 kHz, no resonance).
    fn default() -> Self {
        let mut s = Self { ic1: 0.0, ic2: 0.0, a1: 0.0, a2: 0.0, a3: 0.0, k: 2.0 };
        s.set(20_000.0, 0.0, 48_000.0);
        s
    }
}

impl Svf {
    /// Set cutoff (Hz) and resonance (0..1, self-oscillation is prevented).
    pub fn set(&mut self, fc: f32, res: f32, sr: f32) {
        let g = (std::f32::consts::PI * fc.clamp(10.0, sr * 0.45) / sr).tan();
        let k = 2.0 - 1.9 * res.clamp(0.0, 1.0);
        self.k = k;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }
    #[inline]
    fn tick(&mut self, x: f32) -> (f32, f32) {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1)
    }
    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.tick(x).0
    }
    #[inline]
    pub fn bp(&mut self, x: f32) -> f32 {
        self.tick(x).1
    }
    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        let k = self.k;
        let (lp, bp) = self.tick(x);
        x - k * bp - lp
    }
    /// Flush denormals / recover from any non-finite state.
    pub fn sanitize(&mut self) {
        for z in [&mut self.ic1, &mut self.ic2] {
            if !z.is_finite() || z.abs() < 1e-20 {
                *z = 0.0;
            }
        }
    }
}

/// Transposed direct form II biquad.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn lowpass(fc: f32, q: f32, sr: f32) -> Self {
        let w = TAU * fc / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - c) / 2.0 / a0,
            b1: (1.0 - c) / a0,
            b2: (1.0 - c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
    pub fn highpass(fc: f32, q: f32, sr: f32) -> Self {
        let w = TAU * fc / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + c) / 2.0 / a0,
            b1: -(1.0 + c) / a0,
            b2: (1.0 + c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
    /// Change the coefficients, keep the state (no click).
    pub fn retune_from(&mut self, other: &Biquad) {
        self.b0 = other.b0;
        self.b1 = other.b1;
        self.b2 = other.b2;
        self.a1 = other.a1;
        self.a2 = other.a2;
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    /// Flush denormals / recover from any non-finite state.
    pub fn sanitize(&mut self) {
        if !self.z1.is_finite() || self.z1.abs() < 1e-20 {
            self.z1 = 0.0;
        }
        if !self.z2.is_finite() || self.z2.abs() < 1e-20 {
            self.z2 = 0.0;
        }
    }
}

/// Per-stem output conditioning: 12 dB/oct high-pass (60 Hz for the chip tracks, ~32 Hz for
/// 808/sub remixes) and a 4th-order Butterworth low-pass at 11.5 kHz (keeps any residual aliasing
/// / noise-drum fizz out of the UI 12 kHz+ band).
#[derive(Clone, Copy, Debug)]
pub struct StemFilter {
    hp: Biquad,
    lp1: Biquad,
    lp2: Biquad,
    hp_hz: f32,
}

impl StemFilter {
    pub fn new(sr: f32) -> Self {
        let fc = 11_500.0f32.min(sr * 0.42);
        Self {
            hp: Biquad::highpass(60.0, std::f32::consts::FRAC_1_SQRT_2, sr),
            lp1: Biquad::lowpass(fc, 0.541_196_1, sr),
            lp2: Biquad::lowpass(fc, 1.306_563, sr),
            hp_hz: 60.0,
        }
    }
    /// Move the high-pass corner (keeps the filter state).
    pub fn set_hpf(&mut self, hz: f32, sr: f32) {
        if (hz - self.hp_hz).abs() > 0.01 {
            self.hp_hz = hz;
            self.hp.retune_from(&Biquad::highpass(hz.clamp(15.0, 200.0), std::f32::consts::FRAC_1_SQRT_2, sr));
        }
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.lp2.process(self.lp1.process(self.hp.process(x)))
    }
    pub fn sanitize(&mut self) {
        self.hp.sanitize();
        self.lp1.sanitize();
        self.lp2.sanitize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_is_accurate() {
        for i in 0..1000 {
            let x = i as f32 / 997.0 - 0.3;
            assert!((sin_turns(x) - (x * TAU).sin()).abs() < 2e-5, "{x}");
        }
    }

    #[test]
    fn pulse_is_dc_free_and_finite() {
        let mut v = Voice::new(48_000.0);
        let inst = Instrument { duty: 0.125, sustain: 1.0, ..Instrument::default() };
        v.note_on(&inst, &[69.0], 1.0, false);
        let mut buf = [0.0f32; CONTROL];
        let mut sum = 0.0f64;
        for _ in 0..3000 {
            v.render(&mut buf);
            for &x in &buf {
                assert!(x.is_finite());
                sum += f64::from(x);
            }
        }
        let mean = sum / (3000.0 * CONTROL as f64);
        assert!(mean.abs() < 1e-3, "dc {mean}");
    }

    #[test]
    fn supersaw_is_stereo_and_bounded() {
        let mut v = Voice::new(48_000.0);
        let inst = Instrument { wave: Wave::Supersaw, unison: 7, detune: 30.0, spread: 0.8, sustain: 1.0, ..Instrument::default() };
        v.note_on(&inst, &[57.0], 1.0, false);
        let (mut m, mut s) = ([0.0f32; CONTROL], [0.0f32; CONTROL]);
        let mut side_energy = 0.0;
        for _ in 0..500 {
            assert!(v.render_stereo(&mut m, &mut s, 0.0));
            for (a, b) in m.iter().zip(&s) {
                assert!(a.abs() < 1.0 && b.abs() < 1.0);
                side_energy += b * b;
            }
        }
        assert!(side_energy > 1.0);
    }

    #[test]
    fn resonant_svf_is_stable_under_sweeps() {
        let mut f = Svf::default();
        let mut x = 0.3f32;
        for i in 0..200_000 {
            if i % 32 == 0 {
                f.set(40.0 + (i % 9000) as f32 * 2.0, 0.95, 48_000.0);
            }
            x = -x;
            assert!(f.lp(x).abs() < 50.0);
        }
    }

    #[test]
    fn every_drum_ends_and_stays_finite() {
        for id in 0..20u8 {
            let d = Drum::from_id(id);
            let mut v = DrumVoice::new(48_000.0);
            v.hit(d, 1.0, 0.5, 3.0);
            let mut buf = [0.0f32; CONTROL];
            let mut n = 0;
            while v.is_active() && n < 48_000 * 4 / CONTROL {
                buf.fill(0.0);
                v.render_add(&mut buf, 1.0);
                assert!(buf.iter().all(|x| x.is_finite() && x.abs() < 1.0), "{d:?}");
                n += 1;
            }
            assert!(!v.is_active(), "{d:?} never ends");
        }
    }
}
