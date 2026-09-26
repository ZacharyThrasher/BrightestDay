//! The audio-thread half: a software mixer rendering fixed 64-frame blocks.
//!
//! ```text
//!  voices ──► [pitch/resample] ─► [LPF: air · occlusion · head-shadow · carve] ─► [HPF carve]
//!         ──► equal-power pan ─► bus (Impacts|Weapons|Foley|Constructs|Ambience|Feedback|Ui)
//!                               └► reverb send (World voices)
//!  World buses + shared Reverb return ─► World group: HDR window gain · duck · sidechain
//!                                        (Feedback key: broadband + 4.5 kHz shelf) · LPF
//!  Music source ─► HPF 60 Hz · 7.5 kHz notch · duck · LPF ─┐
//!  World + Feedback + Ui + Music ─► night compressor ─► mono? ─► master ─► soft-knee limiter
//! ```
//!
//! Real-time rules: no allocation, locks or I/O in [`Mixer::process`] (checked by
//! `tests/no_alloc.rs`); every parameter is smoothed per block and ramped per sample; voices
//! beyond the real-voice budget or below the HDR window floor are *virtual* (their playhead keeps
//! moving; they come back mid-sound with a short fade).

use std::sync::Arc;

use glam::Vec3;

use crate::bank::{PCM_SCALE, SoundBank, SoundId};
use crate::cue::{Bus, CueParams, NUM_BUSES, WORLD_BUSES};
use crate::dsp::{Biquad, Ramp, Svf, coef, db_to_lin, finite_or, flush, lerp_hz, lin_to_db, smoothstep};
use crate::dynamics::{Compressor, Limiter};
use crate::mix::{DuckSpec, HdrConfig, MixConfig, MixParams, MusicCarve, OPEN_HZ, SidechainConfig};
use crate::reverb::{Reverb, ReverbParams};
use crate::ring::{Consumer, Producer, ring};

/// Frames per internal block (1.33 ms at 48 kHz): the control rate of every parameter.
pub const BLOCK: usize = 64;
const MAX_DUCKS: usize = 4;
/// Upper bound on sounds tracked for per-sound instance limits.
pub const MAX_SOUNDS: usize = 2048;
/// Commands applied per block at most (a burst spreads over a few blocks instead of spiking one).
const MAX_CMDS_PER_BLOCK: usize = 256;
const TELEMETRY_EVERY: u32 = 8;

/// Where the ears are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub pos: Vec3,
    /// Unit look direction.
    pub fwd: Vec3,
    /// Unit up.
    pub up: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self { pos: Vec3::ZERO, fwd: Vec3::NEG_Z, up: Vec3::Y }
    }
}

/// Player-facing audio settings (the settings UI edits these).
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Linear 0..1 volumes.
    pub master: f32,
    pub world: f32,
    pub ambience: f32,
    /// Hit confirms, KO, streak, UI.
    pub feedback: f32,
    pub music: f32,
    /// "Reduced dynamic range" (night mode): narrower HDR window + master compression.
    pub night_mode: bool,
    pub mono: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { master: 0.9, world: 1.0, ambience: 1.0, feedback: 1.0, music: 0.7, night_mode: false, mono: false }
    }
}

/// Music commands forwarded to the music source (bd_music's `MusicPlayer` in the client).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MusicCommand {
    /// Switch track (on the next bar line); `track` is the music library's track index.
    Play { track: u16, skip_intro: bool },
    Stop,
    /// 0..1: stems fade in/out on bar lines.
    Intensity(f32),
    /// Tempo multiplier (sudden death / final stock), ramped.
    Tempo { multiplier: f32, ramp_secs: f32 },
    /// Beat/bar-quantised stinger (index into the music crate's stinger kinds).
    Stinger(u8),
}

/// A music generator that runs on the audio thread. Must not allocate, lock or block in `render`.
pub trait MusicSource: Send {
    /// Overwrite `out` (interleaved stereo, `out.len() / 2` frames at the mixer's sample rate).
    fn render(&mut self, out: &mut [f32]);
    fn command(&mut self, cmd: MusicCommand);
    /// The current track carries real sub-bass (808 remix variants): the Music bus then opens its
    /// high-pass down to ~32 Hz and ducks the music's lows under impacts / explosions instead.
    fn sub_bass(&self) -> bool {
        false
    }
}

/// Everything a new voice needs (resolved on the game thread).
#[derive(Clone, Copy, Debug)]
pub struct PlaySpec {
    pub id: u32,
    pub sound: SoundId,
    pub var: u16,
    pub cue: CueParams,
    /// World position (ignored when `spatial` is false).
    pub pos: Vec3,
    pub spatial: bool,
    /// The local player's own sound (centred, more present, never muffled by `remote_*`).
    pub local: bool,
    /// Per-play gain on top of the cue (dB): event gain + jitter + own-sound trim.
    pub gain_db: f32,
    /// Playback rate relative to the file (pitch · file_rate / out_rate).
    pub rate: f32,
    pub occlusion: f32,
    pub priority: f32,
    pub looping: bool,
}

pub enum Cmd {
    Play(PlaySpec),
    Update { id: u32, pos: Vec3, gain_db: f32, pitch: f32, occlusion: f32 },
    Stop { id: u32, fade: f32 },
    StopAll { fade: f32 },
    Listener(Listener),
    Mix(MixParams),
    Reverb(ReverbParams),
    Duck(DuckSpec),
    Settings(AudioSettings),
    Config(RtConfig),
    Bank(Arc<SoundBank>),
    SetMusic(Box<dyn MusicSource>),
    Music(MusicCommand),
}

/// Values handed back to the game thread to be freed there (never deallocate on the audio thread).
pub enum Garbage {
    Bank(Arc<SoundBank>),
    Music(Box<dyn MusicSource>),
}

/// The part of [`MixConfig`] the audio thread uses (plain data).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtConfig {
    pub output_gain_db: f32,
    pub hdr: HdrConfig,
    pub sidechain: SidechainConfig,
    pub max_real: usize,
    pub fade: f32,
    pub music_carve: MusicCarve,
}

impl RtConfig {
    pub fn from_mix(c: &MixConfig) -> Self {
        Self {
            output_gain_db: c.output_gain_db.clamp(-40.0, 24.0),
            hdr: c.hdr,
            sidechain: c.sidechain,
            max_real: c.voices.max_real,
            fade: c.voices.fade,
            music_carve: c.music_carve,
        }
    }
}

/// Mixer state snapshot for the game thread (and the offline report).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Telemetry {
    pub frame: u64,
    pub real: u16,
    pub virt: u16,
    pub hdr_top_db: f32,
    pub hdr_gain_db: f32,
    pub sidechain_db: f32,
    pub duck_db: f32,
    pub limiter_db: f32,
    pub peak: f32,
    /// Plays rejected because every slot was busy with something more important.
    pub rejected: u32,
    /// Voices that lost their slot to a more important one.
    pub stolen: u32,
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    id: u32,
    sound: u16,
    var: u16,
    cue: CueParams,
    pos: Vec3,
    spatial: bool,
    local: bool,
    gain_db: f32,
    base_rate: f32,
    pitch: f32,
    rate: f32,
    looping: bool,
    priority: f32,
    occ_target: f32,
    occ: f32,
    /// Playhead (source frames).
    head: f64,
    /// Rendering (real) this block.
    real: bool,
    /// Wants to be real (decided each block).
    want_real: bool,
    fade: f32,
    stopping: bool,
    stop_rate: f32,
    fresh: bool,
    gl: Ramp,
    gr: Ramp,
    sl: Ramp,
    sr_: Ramp,
    lp: [Svf; 2],
    hp: [Svf; 2],
    loud: f32,
    key: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            id: 0,
            sound: 0,
            var: 0,
            cue: CueParams::default(),
            pos: Vec3::ZERO,
            spatial: false,
            local: false,
            gain_db: 0.0,
            base_rate: 1.0,
            pitch: 1.0,
            rate: 1.0,
            looping: false,
            priority: 0.0,
            occ_target: 0.0,
            occ: 0.0,
            head: 0.0,
            real: false,
            want_real: false,
            fade: 0.0,
            stopping: false,
            stop_rate: 0.0,
            fresh: true,
            gl: Ramp::default(),
            gr: Ramp::default(),
            sl: Ramp::default(),
            sr_: Ramp::default(),
            lp: [Svf::default(); 2],
            hp: [Svf::default(); 2],
            loud: -120.0,
            key: 0.0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct DuckEnv {
    active: bool,
    spec: DuckSpec,
    t: f32,
}

impl DuckEnv {
    /// Current depth fraction 0..1.
    fn amount(&self) -> f32 {
        let s = &self.spec;
        let t = self.t;
        if t < s.attack {
            t / s.attack.max(1e-4)
        } else if t < s.attack + s.hold {
            1.0
        } else {
            let r = ((t - s.attack - s.hold) / s.release.max(1e-3)).clamp(0.0, 1.0);
            (1.0 - r) * (1.0 - r)
        }
    }
    fn done(&self) -> bool {
        self.t >= self.spec.attack + self.spec.hold + self.spec.release
    }
}

/// Offline analysis taps (post-fader bus outputs, world group output, reverb return).
pub struct Stems {
    pub bus: [[f32; BLOCK * 2]; NUM_BUSES],
    pub world_out: [f32; BLOCK * 2],
    pub reverb: [f32; BLOCK * 2],
}

/// The audio-thread mixer. Create with [`crate::engine::AudioEngine::new`].
pub struct Mixer {
    sr: f32,
    rx: Consumer<Cmd>,
    tel_tx: Producer<Telemetry>,
    garbage_tx: Producer<Garbage>,
    bank: Option<Arc<SoundBank>>,
    voices: Box<[Voice]>,
    order: Box<[u16]>,
    inst: Box<[u8]>,
    bus_buf: Box<[[f32; BLOCK * 2]; NUM_BUSES]>,
    send_buf: [f32; BLOCK * 2],
    rev_buf: [f32; BLOCK * 2],
    world_buf: [f32; BLOCK * 2],
    music_buf: [f32; BLOCK * 2],
    out_block: [f32; BLOCK * 2],
    out_pos: usize,
    bus_gain: [Ramp; NUM_BUSES],
    bus_lp: [[Svf; 2]; NUM_BUSES],
    bus_hp: [[Svf; 2]; NUM_BUSES],
    mix_target: MixParams,
    mix: MixParams,
    cfg: RtConfig,
    reverb: Reverb,
    // HDR window.
    hdr_top: f32,
    hdr_hold: f32,
    hdr_gain_db: f32,
    // Sidechain.
    sc_env_db: f32,
    sc_amount: f32,
    ducks: [DuckEnv; MAX_DUCKS],
    world_gain: Ramp,
    world_shelf: [Biquad; 2],
    shelf_db_cur: f32,
    world_lp: [Svf; 2],
    music: Option<Box<dyn MusicSource>>,
    music_hp: [Svf; 4],
    music_notch: [Biquad; 2],
    music_lp: [Svf; 2],
    music_gain: Ramp,
    music_sent: (f32, f32),
    music_sub: bool,
    music_low_env: f32,
    music_low_cut: f32,
    music_lowshelf: [Biquad; 2],
    master_gain: Ramp,
    compressor: Compressor,
    limiter: Limiter,
    listener: Listener,
    settings: AudioSettings,
    frame: u64,
    blocks: u32,
    tel: Telemetry,
    stems: Option<Box<Stems>>,
}

/// Build the mixer and the game-thread end of its queues.
pub(crate) fn build(cfg: &MixConfig, sample_rate: u32) -> (Mixer, Producer<Cmd>, Consumer<Telemetry>, Consumer<Garbage>) {
    let (cmd_tx, cmd_rx) = ring::<Cmd>(4096);
    let (tel_tx, tel_rx) = ring::<Telemetry>(64);
    let (garbage_tx, garbage_rx) = ring::<Garbage>(64);
    let sr = sample_rate.max(8000) as f32;
    let total = cfg.voices.max_total.clamp(8, 1024);
    let base = cfg.base_params();
    let mut m = Mixer {
        sr,
        rx: cmd_rx,
        tel_tx,
        garbage_tx,
        bank: None,
        voices: vec![Voice::default(); total].into_boxed_slice(),
        order: vec![0u16; total].into_boxed_slice(),
        inst: vec![0u8; MAX_SOUNDS].into_boxed_slice(),
        bus_buf: Box::new([[0.0; BLOCK * 2]; NUM_BUSES]),
        send_buf: [0.0; BLOCK * 2],
        rev_buf: [0.0; BLOCK * 2],
        world_buf: [0.0; BLOCK * 2],
        music_buf: [0.0; BLOCK * 2],
        out_block: [0.0; BLOCK * 2],
        out_pos: BLOCK,
        bus_gain: [Ramp::new(1.0); NUM_BUSES],
        bus_lp: [[Svf::new(OPEN_HZ, 0.707, sr); 2]; NUM_BUSES],
        bus_hp: [[Svf::new(10.0, 0.707, sr); 2]; NUM_BUSES],
        mix_target: base,
        mix: base,
        cfg: RtConfig::from_mix(cfg),
        reverb: Reverb::new(sr),
        hdr_top: cfg.hdr.threshold_db,
        hdr_hold: 0.0,
        hdr_gain_db: 0.0,
        sc_env_db: -120.0,
        sc_amount: 0.0,
        ducks: [DuckEnv::default(); MAX_DUCKS],
        world_gain: Ramp::new(1.0),
        world_shelf: [Biquad::default(); 2],
        shelf_db_cur: 0.0,
        world_lp: [Svf::new(OPEN_HZ, 0.707, sr); 2],
        music: None,
        music_hp: [Svf::new(60.0, 0.707, sr); 4],
        music_notch: [Biquad::default(); 2],
        music_lp: [Svf::new(OPEN_HZ, 0.707, sr); 2],
        music_gain: Ramp::new(0.0),
        music_sent: (-1.0, -1.0),
        music_sub: false,
        music_low_env: -120.0,
        music_low_cut: 0.0,
        music_lowshelf: [Biquad::default(); 2],
        master_gain: Ramp::new(0.0),
        compressor: Compressor::new(&cfg.night, sr),
        limiter: Limiter::new(&cfg.limiter, sr),
        listener: Listener::default(),
        settings: AudioSettings::default(),
        frame: 0,
        blocks: 0,
        tel: Telemetry::default(),
        stems: None,
    };
    m.reverb.set_params(cfg.reverb);
    m.reverb.snap_to_target();
    m.set_music_carve();
    for s in m.world_shelf.iter_mut() {
        s.set_high_shelf(cfg.sidechain.shelf_hz, 0.0, sr);
    }
    (m, cmd_tx, tel_rx, garbage_rx)
}

impl Mixer {
    pub fn sample_rate(&self) -> u32 {
        self.sr as u32
    }

    /// Output latency added by the mixer itself (limiter look-ahead + one block), in frames.
    pub fn latency_frames(&self) -> usize {
        self.limiter.latency() + BLOCK
    }

    /// Fill interleaved output with `channels` channels (stereo in the first two; others silent).
    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        let ch = channels.max(1);
        for frame in out.chunks_mut(ch) {
            if self.out_pos >= BLOCK {
                self.render_block();
                self.out_pos = 0;
            }
            let l = self.out_block[2 * self.out_pos];
            let r = self.out_block[2 * self.out_pos + 1];
            self.out_pos += 1;
            match frame.len() {
                1 => frame[0] = 0.5 * (l + r),
                _ => {
                    frame[0] = l;
                    frame[1] = r;
                    for s in frame.iter_mut().skip(2) {
                        *s = 0.0;
                    }
                }
            }
        }
    }

    /// Render exactly one block and return it (interleaved stereo) — offline renderer / tests.
    pub fn render_block_into(&mut self, out: &mut [f32; BLOCK * 2]) {
        self.render_block();
        *out = self.out_block;
        self.out_pos = BLOCK;
    }

    /// Capture per-bus stems every block (offline analysis only; allocates once here).
    pub fn enable_stems(&mut self) {
        self.stems = Some(Box::new(Stems { bus: [[0.0; BLOCK * 2]; NUM_BUSES], world_out: [0.0; BLOCK * 2], reverb: [0.0; BLOCK * 2] }));
    }
    pub fn stems(&self) -> Option<&Stems> {
        self.stems.as_deref()
    }
    /// Current telemetry (offline / tests; the game thread reads the ring instead).
    pub fn telemetry(&self) -> Telemetry {
        self.tel
    }

    /// Test/diagnostic view of active voices: (id, real, selection key). Allocates into `out`.
    #[doc(hidden)]
    pub fn debug_voices(&self, out: &mut Vec<(u32, bool, f32)>) {
        out.clear();
        out.extend(self.voices.iter().filter(|v| v.active).map(|v| (v.id, v.real, v.key)));
    }

    fn drain_commands(&mut self) {
        for _ in 0..MAX_CMDS_PER_BLOCK {
            let Some(c) = self.rx.pop() else { break };
            self.apply(c);
        }
    }

    fn apply(&mut self, c: Cmd) {
        match c {
            Cmd::Play(p) => self.start_voice(p),
            Cmd::Update { id, pos, gain_db, pitch, occlusion } => {
                if let Some(v) = self.voices.iter_mut().find(|v| v.active && v.id == id) {
                    if pos.is_finite() {
                        v.pos = pos;
                    }
                    v.gain_db = finite_or(gain_db, v.gain_db).clamp(-120.0, 24.0);
                    v.pitch = finite_or(pitch, 1.0).clamp(0.05, 8.0);
                    v.occ_target = finite_or(occlusion, 0.0).clamp(0.0, 1.0);
                }
            }
            Cmd::Stop { id, fade } => {
                let rate = self.stop_rate(fade);
                if let Some(v) = self.voices.iter_mut().find(|v| v.active && v.id == id) {
                    v.stopping = true;
                    v.stop_rate = rate;
                }
            }
            Cmd::StopAll { fade } => {
                let rate = self.stop_rate(fade);
                for v in self.voices.iter_mut().filter(|v| v.active) {
                    v.stopping = true;
                    v.stop_rate = rate;
                }
            }
            Cmd::Listener(l) => {
                if l.pos.is_finite() && l.fwd.is_finite() && l.up.is_finite() {
                    let fwd = l.fwd.try_normalize().unwrap_or(Vec3::NEG_Z);
                    let up = l.up.try_normalize().unwrap_or(Vec3::Y);
                    self.listener = Listener { pos: l.pos, fwd, up };
                }
            }
            Cmd::Mix(m) => self.mix_target = m,
            Cmd::Reverb(r) => self.reverb.set_params(r),
            Cmd::Duck(d) => {
                let slot = match self.ducks.iter().position(|d| !d.active) {
                    Some(i) => i,
                    // Replace the one closest to finishing.
                    None => (0..MAX_DUCKS).max_by(|&a, &b| self.ducks[a].t.total_cmp(&self.ducks[b].t)).unwrap_or(0),
                };
                self.ducks[slot] = DuckEnv { active: true, spec: sanitize_duck(d), t: 0.0 };
            }
            Cmd::Settings(s) => self.settings = s,
            Cmd::Config(c) => {
                self.cfg = c;
                self.cfg.max_real = self.cfg.max_real.clamp(1, self.voices.len());
                self.set_music_carve();
            }
            Cmd::Bank(b) => {
                for v in self.voices.iter_mut() {
                    v.active = false;
                }
                if let Some(old) = self.bank.replace(b) {
                    self.discard(Garbage::Bank(old));
                }
            }
            Cmd::SetMusic(m) => {
                if let Some(old) = self.music.replace(m) {
                    self.discard(Garbage::Music(old));
                }
                self.music_sent = (-1.0, -1.0);
            }
            Cmd::Music(c) => {
                if let Some(m) = self.music.as_mut() {
                    m.command(c);
                }
            }
        }
    }

    fn discard(&mut self, g: Garbage) {
        if let Err(g) = self.garbage_tx.push(g) {
            // Never free on the audio thread: leak instead (only if the game stopped draining).
            std::mem::forget(g);
        }
    }

    fn stop_rate(&self, fade: f32) -> f32 {
        let f = finite_or(fade, 0.0).max(0.0);
        if f < 1e-3 { 1.0 } else { (BLOCK as f32 / (f * self.sr)).min(1.0) }
    }

    fn set_music_carve(&mut self) {
        let c = self.cfg.music_carve;
        for f in self.music_hp.iter_mut() {
            f.set(c.hpf_hz.max(10.0), 0.707, self.sr);
        }
        for b in self.music_notch.iter_mut() {
            b.set_peaking(c.notch_hz, c.notch_q, c.notch_db, self.sr);
        }
    }

    fn start_voice(&mut self, p: PlaySpec) {
        let Some(bank) = self.bank.as_ref() else { return };
        let Some(sound) = bank.sounds.get(p.sound.0 as usize) else { return };
        if sound.vars.get(p.var as usize).is_none() || (p.sound.0 as usize) >= MAX_SOUNDS {
            return;
        }
        // A free slot, or steal the least important voice if the newcomer beats it.
        let slot = match self.voices.iter().position(|v| !v.active) {
            Some(i) => i,
            None => {
                let mut worst = 0;
                let mut worst_key = f32::INFINITY;
                for (i, v) in self.voices.iter().enumerate() {
                    if v.key < worst_key {
                        worst_key = v.key;
                        worst = i;
                    }
                }
                if worst_key >= p.priority {
                    self.tel.rejected += 1;
                    return;
                }
                self.tel.stolen += 1;
                worst
            }
        };
        let sr = self.sr;
        let v = &mut self.voices[slot];
        *v = Voice {
            active: true,
            id: p.id,
            sound: p.sound.0,
            var: p.var,
            cue: p.cue,
            pos: if p.pos.is_finite() { p.pos } else { Vec3::ZERO },
            spatial: p.spatial,
            local: p.local,
            gain_db: finite_or(p.gain_db, 0.0).clamp(-120.0, 24.0),
            base_rate: finite_or(p.rate, 1.0).clamp(0.05, 8.0),
            pitch: 1.0,
            rate: finite_or(p.rate, 1.0).clamp(0.05, 8.0),
            looping: p.looping,
            priority: finite_or(p.priority, 50.0),
            occ_target: finite_or(p.occlusion, 0.0).clamp(0.0, 1.0),
            occ: finite_or(p.occlusion, 0.0).clamp(0.0, 1.0),
            key: finite_or(p.priority, 50.0),
            ..Voice::default()
        };
        v.hp = [Svf::new(20.0, 0.707, sr); 2];
        v.lp = [Svf::new(OPEN_HZ, 0.707, sr); 2];
    }

    fn render_block(&mut self) {
        self.drain_commands();
        let n = BLOCK;
        let sr = self.sr;
        let block_rate = sr / n as f32;
        let dt = n as f32 / sr;

        // ---- smooth mix parameters (per block, ~40 ms) ------------------------------------------
        let c = coef(0.04, block_rate);
        let t = self.mix_target;
        let m = &mut self.mix;
        for (a, b) in m.bus.iter_mut().zip(t.bus.iter()) {
            a.gain_db += (b.gain_db - a.gain_db) * c;
            a.lpf_hz = lerp_hz(a.lpf_hz, b.lpf_hz, c);
            a.hpf_hz = lerp_hz(a.hpf_hz, b.hpf_hz, c);
            a.send_db += (b.send_db - a.send_db) * c;
        }
        m.world_gain_db += (t.world_gain_db - m.world_gain_db) * c;
        m.world_lpf_hz = lerp_hz(m.world_lpf_hz, t.world_lpf_hz, c);
        m.remote_gain_db += (t.remote_gain_db - m.remote_gain_db) * c;
        m.remote_lpf_hz = lerp_hz(m.remote_lpf_hz, t.remote_lpf_hz, c);
        m.reverb_db += (t.reverb_db - m.reverb_db) * c;
        m.music_intensity = t.music_intensity;
        m.music_tempo = t.music_tempo;
        let mix = *m;

        // ---- ducks -------------------------------------------------------------------------------
        let mut duck_db = 0.0f32;
        let mut duck_music_db = 0.0f32;
        let mut duck_lpf = OPEN_HZ;
        for d in self.ducks.iter_mut().filter(|d| d.active) {
            let a = d.amount();
            duck_db += d.spec.depth_db * a;
            if d.spec.music {
                duck_music_db += d.spec.music_db.unwrap_or(d.spec.depth_db) * a;
            }
            duck_lpf = duck_lpf.min(lerp_hz(OPEN_HZ, d.spec.lpf_hz, a));
            d.t += dt;
            if d.done() {
                d.active = false;
            }
        }
        let duck_db = duck_db.max(-30.0);
        let duck_music_db = duck_music_db.max(-30.0);

        // ---- voices: spatialise, measure, prioritise ---------------------------------------------
        let hdr = self.cfg.hdr;
        let night = self.settings.night_mode;
        let range = if night { hdr.night_range_db } else { hdr.range_db };
        let ratio = if night { hdr.night_ratio } else { hdr.ratio };
        let floor = self.hdr_top - range;
        let listener = self.listener;
        let right = listener.fwd.cross(listener.up).try_normalize().unwrap_or(Vec3::X);
        let occ_c = coef(0.05, block_rate);
        let mut loudest = -120.0f32;
        let mut n_active = 0usize;
        let bank = self.bank.clone();
        let Some(bank) = bank else {
            self.finish_block(duck_db, duck_music_db, duck_lpf, &mix);
            return;
        };
        for (i, v) in self.voices.iter_mut().enumerate() {
            if !v.active {
                continue;
            }
            let Some(pcm) = bank.sounds.get(v.sound as usize).and_then(|s| s.vars.get(v.var as usize)) else {
                v.active = false;
                continue;
            };
            v.occ += (v.occ_target - v.occ) * occ_c;
            let sp = spatialise(v, &listener, right, &mix);
            v.loud = pcm.loudness_at(v.head as usize) + sp.total_db;
            if v.cue.hdr && !v.stopping {
                loudest = loudest.max(v.loud);
            }
            // Selection key: priority, then how far above the window floor it sits.
            // 3 dB hysteresis so a sound hovering at the floor doesn't flicker real ↔ virtual.
            let above = v.loud - floor + if v.real { 3.0 } else { 0.0 };
            v.key = if v.cue.hdr && above < 0.0 { -1000.0 + above } else { v.priority + above.clamp(-30.0, 40.0) * 0.5 }
                + if v.real { 2.0 } else { 0.0 }
                - if v.stopping { 500.0 } else { 0.0 };
            if sp.silent {
                v.key = -2000.0;
            }
            self.order[n_active] = i as u16;
            n_active += 1;
        }

        // HDR window top: instant attack, hold, then release at a fixed dB/s.
        if hdr.enabled {
            let target = loudest.max(hdr.threshold_db);
            if target >= self.hdr_top {
                self.hdr_top = target;
                self.hdr_hold = hdr.hold;
            } else if self.hdr_hold > 0.0 {
                self.hdr_hold -= dt;
            } else {
                self.hdr_top = (self.hdr_top - hdr.release_db_s * dt).max(target);
            }
        } else {
            self.hdr_top = hdr.threshold_db;
        }
        let hdr_gain_target = -(self.hdr_top - hdr.threshold_db).max(0.0) * ratio;
        // Fast attack (the loud sound starts this block), 60 ms release.
        let hc = if hdr_gain_target < self.hdr_gain_db { 1.0 } else { coef(0.06, block_rate) };
        self.hdr_gain_db += (hdr_gain_target - self.hdr_gain_db) * hc;

        // Sort by key and grant real voices.
        let voices = &self.voices;
        let order = &mut self.order[..n_active];
        order.sort_unstable_by(|&a, &b| voices[b as usize].key.total_cmp(&voices[a as usize].key));
        for &i in order.iter() {
            let s = voices[i as usize].sound as usize;
            self.inst[s] = 0;
        }
        let max_real = self.cfg.max_real.min(self.voices.len());
        let mut real = 0usize;
        for k in 0..n_active {
            let i = self.order[k] as usize;
            let v = &mut self.voices[i];
            let audible = v.key > -999.0;
            let s = v.sound as usize;
            if audible && real < max_real && self.inst[s] < v.cue.max_instances {
                v.want_real = true;
                real += 1;
                self.inst[s] += 1;
            } else {
                v.want_real = false;
            }
        }

        // ---- render ------------------------------------------------------------------------------
        for b in self.bus_buf.iter_mut() {
            b.fill(0.0);
        }
        self.send_buf.fill(0.0);
        let fade_rate = (n as f32 / (self.cfg.fade.max(0.001) * sr)).min(1.0);
        let mut n_real = 0u16;
        let mut n_virt = 0u16;
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            let Some(pcm) = bank.sounds.get(v.sound as usize).and_then(|s| s.vars.get(v.var as usize)) else {
                v.active = false;
                continue;
            };
            // Fade target: real → 1, virtual → 0, stopping → 0 at its own rate.
            let (fade_to, rate) = if v.stopping {
                (0.0, v.stop_rate)
            } else if v.want_real {
                (1.0, fade_rate)
            } else {
                (0.0, fade_rate)
            };
            if v.fresh {
                // A voice that starts real starts at full level: transients stay intact.
                v.fade = if v.want_real { 1.0 } else { 0.0 };
            } else if v.fade < fade_to {
                v.fade = (v.fade + rate).min(fade_to);
            } else {
                v.fade = (v.fade - rate).max(fade_to);
            }
            v.rate = v.base_rate * v.pitch;
            let frames = pcm.frames;
            // Render while audible or while ramping down from the last real block.
            let rendering = v.fade > 0.0 || v.real;
            if !rendering {
                // Virtual: the playhead keeps moving.
                v.fresh = false;
                v.head += v.rate as f64 * n as f64;
                if v.looping {
                    if frames > 0 {
                        v.head %= frames as f64;
                    }
                } else if v.head >= frames as f64 {
                    v.active = false;
                    continue;
                }
                if v.stopping {
                    v.active = false;
                    continue;
                }
                n_virt += 1;
                continue;
            }
            n_real += 1;
            let sp = spatialise(v, &listener, right, &mix);
            let bus = v.cue.bus.index();
            let g = db_to_lin(sp.total_db) * v.fade;
            let gs = if v.cue.bus.is_world() { db_to_lin(sp.send_db) * v.fade } else { 0.0 };
            let (pl, pr) = pan_gains(sp.pan);
            let ch = pcm.channels as usize;
            let (tl, tr) = (g * pl, g * pr);
            if v.fresh || !v.real {
                v.gl.settle(tl);
                v.gr.settle(tr);
                v.sl.settle(gs * pl);
                v.sr_.settle(gs * pr);
            } else {
                v.gl.to(tl, n);
                v.gr.to(tr, n);
                v.sl.to(gs * pl, n);
                v.sr_.to(gs * pr, n);
            }
            v.real = true;
            v.fresh = false;
            v.lp[0].set(sp.lpf, 0.707, sr);
            v.lp[1].set(sp.lpf, 0.707, sr);
            let use_hp = sp.hpf > 20.0;
            if use_hp {
                v.hp[0].set(sp.hpf, 0.707, sr);
                v.hp[1].set(sp.hpf, 0.707, sr);
            }
            let width = v.cue.width;
            let data: &[i16] = &pcm.data;
            let busb = &mut self.bus_buf[bus];
            let send = &mut self.send_buf;
            let mut head = v.head;
            let step = v.rate as f64;
            let mut ended = false;
            for f in 0..n {
                let i0 = head as usize;
                if i0 >= frames {
                    ended = true;
                    break;
                }
                let fr = (head - i0 as f64) as f32;
                let i1 = if i0 + 1 >= frames { if v.looping { 0 } else { i0 } } else { i0 + 1 };
                let (mut xl, mut xr);
                if ch == 1 {
                    let a = data[i0] as f32;
                    xl = (a + (data[i1] as f32 - a) * fr) * PCM_SCALE;
                    xr = xl;
                } else {
                    let a = data[i0 * ch] as f32;
                    let b = data[i0 * ch + 1] as f32;
                    xl = (a + (data[i1 * ch] as f32 - a) * fr) * PCM_SCALE;
                    xr = (b + (data[i1 * ch + 1] as f32 - b) * fr) * PCM_SCALE;
                    if sp.spatial {
                        let mono = 0.5 * (xl + xr);
                        xl = mono;
                        xr = mono;
                    } else {
                        let mid = 0.5 * (xl + xr);
                        xl = mid + (xl - mid) * width;
                        xr = mid + (xr - mid) * width;
                    }
                }
                xl = v.lp[0].lp(xl);
                if use_hp {
                    xl = v.hp[0].hp(xl);
                }
                if ch == 1 {
                    xr = xl;
                } else {
                    xr = v.lp[1].lp(xr);
                    if use_hp {
                        xr = v.hp[1].hp(xr);
                    }
                }
                let gl = v.gl.tick();
                let gr = v.gr.tick();
                let sl = v.sl.tick();
                let srr = v.sr_.tick();
                busb[2 * f] += xl * gl;
                busb[2 * f + 1] += xr * gr;
                send[2 * f] += xl * sl;
                send[2 * f + 1] += xr * srr;
                head += step;
                if head >= frames as f64 {
                    if v.looping {
                        head -= frames as f64;
                    } else {
                        ended = true;
                        break;
                    }
                }
            }
            v.gl.settle(tl);
            v.gr.settle(tr);
            v.sl.settle(gs * pl);
            v.sr_.settle(gs * pr);
            for s in v.lp.iter_mut().chain(v.hp.iter_mut()) {
                s.sanitize();
            }
            v.head = head;
            if ended || (v.stopping && v.fade <= 0.0) {
                v.active = false;
            } else if v.fade <= 0.0 {
                // Just faded out: virtual from the next block, filters restart clean when it returns.
                v.real = false;
                for s in v.lp.iter_mut().chain(v.hp.iter_mut()) {
                    s.reset();
                }
            }
        }
        self.tel.real = n_real;
        self.tel.virt = n_virt;
        self.finish_block(duck_db, duck_music_db, duck_lpf, &mix);
    }

    /// Buses → World group → master → limiter.
    fn finish_block(&mut self, duck_db: f32, duck_music_db: f32, duck_lpf: f32, mix: &MixParams) {
        let n = BLOCK;
        let sr = self.sr;
        let block_rate = sr / n as f32;
        let st = self.settings;
        let vol = |x: f32| lin_to_db(finite_or(x, 1.0).clamp(0.0, 2.0));
        // Bus faders + filters.
        for b in 0..NUM_BUSES {
            let bp = mix.bus[b];
            let user = match Bus::from_index(b) {
                Bus::Ambience => vol(st.ambience) + vol(st.world),
                Bus::Feedback | Bus::Ui => vol(st.feedback),
                Bus::Music => 0.0,
                _ => vol(st.world),
            };
            let g = db_to_lin(bp.gain_db + user);
            self.bus_gain[b].to(g, n);
            let lp_on = bp.lpf_hz < 19000.0;
            let hp_on = bp.hpf_hz > 20.0;
            if lp_on {
                self.bus_lp[b][0].set(bp.lpf_hz, 0.707, sr);
                self.bus_lp[b][1].set(bp.lpf_hz, 0.707, sr);
            }
            if hp_on {
                self.bus_hp[b][0].set(bp.hpf_hz, 0.707, sr);
                self.bus_hp[b][1].set(bp.hpf_hz, 0.707, sr);
            }
            let buf = &mut self.bus_buf[b];
            let ramp = &mut self.bus_gain[b];
            let [lpl, lpr] = &mut self.bus_lp[b];
            let [hpl, hpr] = &mut self.bus_hp[b];
            for f in 0..n {
                let gg = ramp.tick();
                let mut l = buf[2 * f];
                let mut r = buf[2 * f + 1];
                if lp_on {
                    l = lpl.lp(l);
                    r = lpr.lp(r);
                }
                if hp_on {
                    l = hpl.hp(l);
                    r = hpr.hp(r);
                }
                buf[2 * f] = l * gg;
                buf[2 * f + 1] = r * gg;
            }
            ramp.settle(g);
            for s in [lpl, lpr, hpl, hpr] {
                s.sanitize();
            }
        }

        // Sidechain key: Feedback bus level.
        let sc = self.cfg.sidechain;
        let key_peak = self.bus_buf[Bus::Feedback.index()].iter().fold(0.0f32, |a, &x| a.max(x.abs()));
        let key_db = lin_to_db(key_peak);
        let c = if key_db > self.sc_env_db { coef(sc.attack, block_rate) } else { coef(sc.release, block_rate) };
        self.sc_env_db += (key_db - self.sc_env_db) * c;
        self.sc_amount = smoothstep(sc.threshold_db, sc.threshold_db + sc.knee_db.max(0.1), self.sc_env_db);
        let sc_db = sc.depth_db * self.sc_amount;
        let shelf_db = sc.shelf_db * self.sc_amount;
        if (shelf_db - self.shelf_db_cur).abs() > 0.05 {
            self.shelf_db_cur = shelf_db;
            for s in self.world_shelf.iter_mut() {
                s.set_high_shelf(sc.shelf_hz, shelf_db, sr);
            }
        }

        // Reverb (World sends).
        self.rev_buf.fill(0.0);
        self.reverb.process(&self.send_buf, &mut self.rev_buf);
        let rev_gain = db_to_lin(mix.reverb_db);

        // World group.
        let world_db = mix.world_gain_db + self.hdr_gain_db + duck_db + sc_db;
        let wg = db_to_lin(world_db);
        self.world_gain.to(wg, n);
        let wl = mix.world_lpf_hz.min(duck_lpf);
        let wl_on = wl < 19000.0;
        if wl_on {
            self.world_lp[0].set(wl, 0.707, sr);
            self.world_lp[1].set(wl, 0.707, sr);
        }
        let shelf_on = self.shelf_db_cur < -0.05;
        for f in 0..n {
            let mut l = self.rev_buf[2 * f] * rev_gain;
            let mut r = self.rev_buf[2 * f + 1] * rev_gain;
            for b in 0..WORLD_BUSES {
                l += self.bus_buf[b][2 * f];
                r += self.bus_buf[b][2 * f + 1];
            }
            if shelf_on {
                l = self.world_shelf[0].process(l);
                r = self.world_shelf[1].process(r);
            }
            if wl_on {
                l = self.world_lp[0].lp(l);
                r = self.world_lp[1].lp(r);
            }
            let g = self.world_gain.tick();
            self.world_buf[2 * f] = l * g;
            self.world_buf[2 * f + 1] = r * g;
        }
        self.world_gain.settle(wg);
        for s in self.world_shelf.iter_mut() {
            s.sanitize();
        }
        for s in self.world_lp.iter_mut() {
            s.sanitize();
        }

        // Music.
        let has_music = self.music.is_some();
        if let Some(mus) = self.music.as_mut() {
            if self.music_sent.0 != mix.music_intensity {
                mus.command(MusicCommand::Intensity(mix.music_intensity));
                self.music_sent.0 = mix.music_intensity;
            }
            if self.music_sent.1 != mix.music_tempo {
                mus.command(MusicCommand::Tempo { multiplier: mix.music_tempo, ramp_secs: 1.5 });
                self.music_sent.1 = mix.music_tempo;
            }
            self.music_buf.fill(0.0);
            mus.render(&mut self.music_buf);
            let bp = mix.bus[Bus::Music.index()];
            // Fixed-level bed: ducked only by the explicit (heavy-hit / KO) ducks, never by every hit confirm.
            let mg = db_to_lin(bp.gain_db + vol(st.music) + duck_music_db);
            // Sub-bass variants: open the high-pass, and duck the music's lows under World impacts.
            let sub = mus.sub_bass();
            if sub != self.music_sub {
                self.music_sub = sub;
                let hz = if sub { 32.0 } else { self.cfg.music_carve.hpf_hz.max(10.0) };
                for f in self.music_hp.iter_mut() {
                    f.set(hz, 0.707, sr);
                }
            }
            if sub {
                let lowkey = self.bus_buf[Bus::Impacts.index()]
                    .iter()
                    .zip(self.bus_buf[Bus::Weapons.index()].iter())
                    .fold(0.0f32, |a, (x, y)| a.max((x + y).abs()));
                let k_db = lin_to_db(lowkey);
                let c = if k_db > self.music_low_env { coef(0.005, block_rate) } else { coef(0.25, block_rate) };
                self.music_low_env += (k_db - self.music_low_env) * c;
                let cut = -9.0 * smoothstep(-36.0, -18.0, self.music_low_env);
                if (cut - self.music_low_cut).abs() > 0.1 {
                    self.music_low_cut = cut;
                    for b in self.music_lowshelf.iter_mut() {
                        b.set_low_shelf(120.0, cut, sr);
                    }
                }
            }
            self.music_gain.to(mg, n);
            let lp_on = bp.lpf_hz < 19000.0;
            if lp_on {
                self.music_lp[0].set(bp.lpf_hz, 0.707, sr);
                self.music_lp[1].set(bp.lpf_hz, 0.707, sr);
            }
            for f in 0..n {
                let mut l = finite_or(self.music_buf[2 * f], 0.0);
                let mut r = finite_or(self.music_buf[2 * f + 1], 0.0);
                let (a, b) = (self.music_hp[0].hp(l), self.music_hp[1].hp(r));
                l = self.music_hp[2].hp(a);
                r = self.music_hp[3].hp(b);
                l = self.music_notch[0].process(l);
                r = self.music_notch[1].process(r);
                if self.music_sub {
                    l = self.music_lowshelf[0].process(l);
                    r = self.music_lowshelf[1].process(r);
                }
                if lp_on {
                    l = self.music_lp[0].lp(l);
                    r = self.music_lp[1].lp(r);
                }
                let g = self.music_gain.tick();
                self.music_buf[2 * f] = l * g;
                self.music_buf[2 * f + 1] = r * g;
            }
            self.music_gain.settle(mg);
            for s in self.music_hp.iter_mut().chain(self.music_lp.iter_mut()) {
                s.sanitize();
            }
            for s in self.music_notch.iter_mut() {
                s.sanitize();
            }
        }

        // Master.
        let fb = Bus::Feedback.index();
        let ui = Bus::Ui.index();
        for f in 0..n {
            let mut l = self.world_buf[2 * f] + self.bus_buf[fb][2 * f] + self.bus_buf[ui][2 * f];
            let mut r = self.world_buf[2 * f + 1] + self.bus_buf[fb][2 * f + 1] + self.bus_buf[ui][2 * f + 1];
            if has_music {
                l += self.music_buf[2 * f];
                r += self.music_buf[2 * f + 1];
            }
            self.out_block[2 * f] = l;
            self.out_block[2 * f + 1] = r;
        }
        if let Some(stems) = self.stems.as_mut() {
            for b in 0..NUM_BUSES {
                stems.bus[b] = self.bus_buf[b];
            }
            if has_music {
                stems.bus[Bus::Music.index()] = self.music_buf;
            }
            stems.world_out = self.world_buf;
            for f in 0..2 * n {
                stems.reverb[f] = self.rev_buf[f] * rev_gain;
            }
        }
        self.compressor.process(&mut self.out_block, if st.night_mode { 1.0 } else { 0.0 });
        if st.mono {
            for f in 0..n {
                let m = 0.5 * (self.out_block[2 * f] + self.out_block[2 * f + 1]);
                self.out_block[2 * f] = m;
                self.out_block[2 * f + 1] = m;
            }
        }
        let mg = finite_or(st.master, 1.0).clamp(0.0, 2.0) * db_to_lin(finite_or(self.cfg.output_gain_db, 0.0));
        self.master_gain.to(mg, n);
        for f in 0..n {
            let g = self.master_gain.tick();
            self.out_block[2 * f] *= g;
            self.out_block[2 * f + 1] *= g;
        }
        self.master_gain.settle(mg);
        self.limiter.process(&mut self.out_block);

        // Telemetry.
        self.frame += n as u64;
        self.blocks += 1;
        let peak = self.out_block.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
        self.tel.frame = self.frame;
        self.tel.hdr_top_db = self.hdr_top;
        self.tel.hdr_gain_db = self.hdr_gain_db;
        self.tel.sidechain_db = sc_db;
        self.tel.duck_db = duck_db;
        self.tel.limiter_db = self.limiter.last_gr_db;
        self.tel.peak = self.tel.peak.max(peak);
        if self.blocks.is_multiple_of(TELEMETRY_EVERY) {
            let _ = self.tel_tx.push(self.tel);
            self.tel.peak = 0.0;
        }
    }
}

struct Spatial {
    hpf: f32,
    total_db: f32,
    send_db: f32,
    pan: f32,
    lpf: f32,
    spatial: bool,
    silent: bool,
}

/// Distance, direction, occlusion and snapshot shaping of one voice for this block.
fn spatialise(v: &Voice, l: &Listener, right: Vec3, mix: &MixParams) -> Spatial {
    let cue = &v.cue;
    let bus = &mix.bus[cue.bus.index()];
    let mut db = cue.gain_db + v.gain_db;
    let mut send = db + cue.send_db + bus.send_db;
    let mut lpf = if cue.lpf_hz > 0.0 { cue.lpf_hz } else { OPEN_HZ };
    let mut pan = 0.0;
    let mut hpf = cue.hpf_hz;
    let mut silent = false;
    if v.spatial {
        let rel = v.pos - l.pos;
        let d = rel.length();
        let d = if d.is_finite() { d } else { cue.max_dist * 2.0 };
        if d >= cue.max_dist {
            silent = true;
        }
        let dd = d.max(cue.min_dist);
        let dist_db = -20.0 * cue.rolloff * (dd / cue.min_dist).log10();
        let edge = 1.0 - smoothstep(cue.max_dist * 0.8, cue.max_dist, d);
        let edge_db = lin_to_db(edge.max(1e-6));
        db += dist_db + edge_db;
        // The reverberant field falls off slower than the direct sound: far = wetter.
        send += dist_db * 0.55 + edge_db;
        let dir = if d > 1e-3 { rel / d } else { l.fwd };
        let x = dir.dot(right);
        let z = dir.dot(l.fwd);
        let near = smoothstep(0.3, 2.0, d);
        pan = x.clamp(-1.0, 1.0) * near;
        if cue.far_hpf_hz > 0.0 {
            hpf = hpf.max(cue.far_hpf_hz * smoothstep(cue.min_dist * 2.0, 45.0, d));
        }
        let behind = (-z).max(0.0) * near;
        // Air absorption: highs roll off with distance.
        let air = OPEN_HZ * (-d * cue.air / 110.0).exp();
        lpf = lpf.min(air.max(900.0));
        // Head shadow for sources behind.
        lpf = lpf.min(lerp_hz(OPEN_HZ, 5500.0, behind));
        db -= 2.0 * behind;
        // Occlusion: through walls = darker + quieter; the reverberant path suffers less.
        let occ = (v.occ * cue.occlusion).clamp(0.0, 1.0);
        if occ > 0.001 {
            db -= 16.0 * occ;
            send -= 7.0 * occ;
            lpf = lpf.min(lerp_hz(OPEN_HZ, 600.0, occ));
        }
    }
    if !v.local && cue.bus.is_world() {
        db += mix.remote_gain_db;
        send += mix.remote_gain_db;
        lpf = lpf.min(mix.remote_lpf_hz);
    }
    Spatial { hpf, total_db: db, send_db: send, pan, lpf, spatial: v.spatial, silent }
}

/// Equal-power pan law (−3 dB centre).
#[inline]
fn pan_gains(pan: f32) -> (f32, f32) {
    let th = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (th.cos(), th.sin())
}

fn sanitize_duck(mut d: DuckSpec) -> DuckSpec {
    d.depth_db = finite_or(d.depth_db, 0.0).clamp(-30.0, 0.0);
    d.lpf_hz = finite_or(d.lpf_hz, OPEN_HZ).clamp(100.0, OPEN_HZ);
    d.attack = finite_or(d.attack, 0.01).clamp(0.0, 2.0);
    d.hold = finite_or(d.hold, 0.1).clamp(0.0, 5.0);
    d.release = finite_or(d.release, 0.2).clamp(0.001, 10.0);
    d
}

#[allow(dead_code)]
fn _assert_send() {
    fn is_send<T: Send>() {}
    is_send::<Mixer>();
    let _ = flush;
}
