//! The chip synth: band-limited oscillators (4-point BLEP pulse / saw), triangle, the two-operator
//! "hard light" FM voice, and NES-style LFSR noise drums. Everything here is `Copy` state that renders
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
    /// Pure sine (sub-free "glass" pads, echo voices).
    Sine,
    /// "Hard light": two detuned 2-operator FM pairs, index envelope, band-limited index.
    Fm,
}

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
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub fm_index_end: f32,
    pub fm_decay: f32,
    pub fm_detune: f32,
    pub fm_feedback: f32,
    pub lp_hz: f32,
    pub lp_env: f32,
    pub lp_time: f32,
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
            fm_ratio: 1.0,
            fm_index: 1.0,
            fm_index_end: 1.0,
            fm_decay: 0.3,
            fm_detune: 0.0,
            fm_feedback: 0.0,
            lp_hz: 0.0,
            lp_env: 0.0,
            lp_time: 0.1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Attack,
    Decay,
    Release,
    Off,
}

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
    duty: f32,
    index: f32,
    lp_a: f32,
    lp_z: f32,
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
            duty: 0.25,
            index: 0.0,
            lp_a: 1.0,
            lp_z: 0.0,
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
    /// oscillator phase, so legato notes never click. `glide` slides from the previous pitch.
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
        if glide && was_active {
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
        }
        let sr = self.sr;
        self.peak = (vel * inst.vol).max(0.0);
        self.sus = self.peak * inst.sustain.clamp(0.0, 1.0);
        self.att_step = if inst.attack > 0.0 { self.peak / (inst.attack * sr) } else { self.peak.max(1e-6) };
        self.dec_coef = if inst.decay > 0.0 { (-6.9 / (inst.decay * sr)).exp() } else { 0.0 };
        self.rel_coef = (-6.9 / (inst.release.max(0.002) * sr)).exp();
        self.stage = if self.env < self.peak { Stage::Attack } else { Stage::Decay };
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
    /// vibrato, pitch drop, duty sweep, FM index envelope, filter envelope.
    fn control(&mut self, span_secs: f32) {
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
        if inst.lp_hz > 0.0 {
            let fc = inst.lp_hz * (1.0 + inst.lp_env * (-self.age / inst.lp_time.max(1e-3)).exp());
            let fc = fc.min(self.sr * 0.45);
            self.lp_a = 1.0 - (-TAU * fc * inv_sr).exp();
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

    /// Render (overwrite) `out` with this voice's mono signal (at most [`CONTROL`] samples).
    pub fn render(&mut self, out: &mut [f32]) {
        if self.stage == Stage::Off {
            out.fill(0.0);
            return;
        }
        let span = out.len() as f32 / self.sr;
        self.control(span);
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
                    let x = (y - dc) * self.env_step() * gain;
                    self.lp_z += lp_a * (x - self.lp_z);
                    *o = self.lp_z;
                }
            }
            Wave::Saw => {
                let dt = self.dt[0];
                for o in out.iter_mut() {
                    let t = self.phase[0];
                    let y = 2.0 * t - 1.0 - 2.0 * edge(t, dt);
                    self.phase[0] = wrap(t + dt);
                    let x = y * self.env_step() * gain;
                    self.lp_z += lp_a * (x - self.lp_z);
                    *o = self.lp_z;
                }
            }
            Wave::Triangle => {
                let dt = self.dt[0];
                for o in out.iter_mut() {
                    let t = self.phase[0];
                    let y = 1.0 - 4.0 * (t - 0.5).abs();
                    self.phase[0] = wrap(t + dt);
                    let x = y * self.env_step() * gain * 1.25;
                    self.lp_z += lp_a * (x - self.lp_z);
                    *o = self.lp_z;
                }
            }
            Wave::Sine => {
                let dt = self.dt[0];
                for o in out.iter_mut() {
                    let t = self.phase[0];
                    self.phase[0] = wrap(t + dt);
                    *o = sin_turns(t) * self.env_step() * gain * 1.2;
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
                    let x = y * 0.6 * self.env_step() * gain;
                    self.lp_z += lp_a * (x - self.lp_z);
                    *o = self.lp_z;
                }
            }
        }
    }
}

#[inline]
fn wrap(x: f32) -> f32 {
    if x >= 1.0 { x - x.floor() } else { x }
}

// ------------------------------------------------------------------------------------------------
// Drums
// ------------------------------------------------------------------------------------------------

/// The drum kit of a noise channel. Letters in the notation: k s h o c t m x n.
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
            _ => Drum::Blip,
        }
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

#[derive(Clone, Copy, Debug)]
struct DrumRecipe {
    tone: f32,
    tone_hz: f32,
    tone_end_hz: f32,
    tone_sweep: f32,
    tone_decay: f32,
    noise: f32,
    noise_idx: f32,
    noise_end_idx: f32,
    noise_sweep: f32,
    noise_decay: f32,
    short_mode: bool,
    /// Extra re-triggers of the noise envelope (clap), seconds apart.
    flams: u8,
}

fn recipe(d: Drum) -> DrumRecipe {
    let base = DrumRecipe {
        tone: 0.0,
        tone_hz: 100.0,
        tone_end_hz: 100.0,
        tone_sweep: 0.05,
        tone_decay: 0.1,
        noise: 0.0,
        noise_idx: 4.0,
        noise_end_idx: 4.0,
        noise_sweep: 0.05,
        noise_decay: 0.1,
        short_mode: false,
        flams: 0,
    };
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
        Drum::Blip => {
            DrumRecipe { noise: 0.35, noise_idx: 5.0, noise_end_idx: 7.0, noise_sweep: 0.06, noise_decay: 0.07, short_mode: true, ..base }
        }
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
    tone_phase: f32,
    tone_amp: f32,
    tone_coef: f32,
    noise_amp: f32,
    noise_coef: f32,
    lfsr: u16,
    noise_acc: f32,
    noise_out: f32,
    kill: f32,
    flam_left: u8,
    flam_t: f32,
    /// 0.8 ms onset ramp: keeps the punch, removes the step click.
    att: f32,
    att_step: f32,
}

impl DrumVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sr: sample_rate,
            r: recipe(Drum::Hat),
            active: false,
            age: 0.0,
            vel: 0.0,
            tone_phase: 0.0,
            tone_amp: 0.0,
            tone_coef: 0.0,
            noise_amp: 0.0,
            noise_coef: 0.0,
            lfsr: 1,
            noise_acc: 0.0,
            noise_out: 1.0,
            kill: 1.0,
            flam_left: 0,
            flam_t: 0.0,
            att: 1.0,
            att_step: 1.0 / (0.0008 * sample_rate),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn hit(&mut self, d: Drum, vel: f32) {
        let r = recipe(d);
        self.r = r;
        self.active = true;
        self.age = 0.0;
        self.vel = vel;
        self.tone_phase = 0.0;
        self.tone_amp = r.tone * vel;
        self.tone_coef = (-6.9 / (r.tone_decay.max(1e-3) * self.sr)).exp();
        self.noise_amp = r.noise * vel;
        self.noise_coef = (-6.9 / (r.noise_decay.max(1e-3) * self.sr)).exp();
        self.kill = 1.0;
        self.flam_left = r.flams;
        self.flam_t = 0.0;
        self.att = 0.0;
        // LFSR keeps running between hits (like the chip); never let it reach 0.
        if self.lfsr == 0 {
            self.lfsr = 1;
        }
    }

    /// Start a ~12 ms fade-out (the noise channel was re-triggered).
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
        let tone_hz = r.tone_hz * (r.tone_end_hz / r.tone_hz).powf(1.0 - (1.0 - st) * (1.0 - st));
        let tone_dt = tone_hz * inv_sr;
        let nt = (self.age / r.noise_sweep.max(1e-3)).min(1.0);
        let clock = noise_clock(r.noise_idx + (r.noise_end_idx - r.noise_idx) * nt);
        let steps_per_sample = clock * inv_sr;
        let g = gain * VOICE_GAIN;
        let kill = self.kill;
        for o in out.iter_mut() {
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
            let tone = if self.tone_amp > 1e-5 {
                let s = sin_turns(self.tone_phase) * self.tone_amp;
                self.tone_phase = wrap(self.tone_phase + tone_dt);
                self.tone_amp *= self.tone_coef;
                s
            } else {
                0.0
            };
            let nz = self.noise_out * self.noise_amp;
            self.noise_amp *= self.noise_coef;
            self.tone_amp *= kill;
            self.noise_amp *= kill;
            self.att = (self.att + self.att_step).min(1.0);
            *o += (tone + nz) * g * self.att;
        }
        self.age += span;
        if self.flam_left > 0 {
            self.flam_t += span;
            if self.flam_t >= 0.011 {
                self.flam_t = 0.0;
                self.flam_left -= 1;
                self.noise_amp = self.noise_amp.max(r.noise * self.vel * 0.85);
            }
        }
        if self.tone_amp < 1e-5 && self.noise_amp < 1e-5 && self.flam_left == 0 {
            self.active = false;
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Filters
// ------------------------------------------------------------------------------------------------

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

/// Per-stem output conditioning: 12 dB/oct high-pass at 60 Hz (no sub) and a 4th-order Butterworth
/// low-pass at 11.5 kHz (keeps any residual aliasing / noise-drum fizz out of the UI 12 kHz+ band).
#[derive(Clone, Copy, Debug)]
pub struct StemFilter {
    hp: Biquad,
    lp1: Biquad,
    lp2: Biquad,
}

impl StemFilter {
    pub fn new(sr: f32) -> Self {
        let fc = 11_500.0f32.min(sr * 0.42);
        Self {
            hp: Biquad::highpass(60.0, std::f32::consts::FRAC_1_SQRT_2, sr),
            lp1: Biquad::lowpass(fc, 0.541_196_1, sr),
            lp2: Biquad::lowpass(fc, 1.306_563, sr),
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
}
