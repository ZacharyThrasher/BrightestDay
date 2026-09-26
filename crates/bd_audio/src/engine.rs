//! The game-thread half: resolves sounds and cues, applies micro-variation (shuffle bags that never
//! repeat a take back to back, ± cents, ± dB), enforces cooldowns, and sends allocation-free
//! commands to the [`Mixer`] over a lock-free ring. Nothing here blocks on the audio thread.

use std::sync::Arc;

use glam::Vec3;

use crate::bank::{SoundBank, SoundId};
use crate::cue::{Bus, CueParams, CueTable};
use crate::dsp::{Rng, finite_or};
use crate::mix::{DuckSpec, MixConfig, MixParams};
use crate::mixer::{self, AudioSettings, Cmd, Garbage, Listener, Mixer, MusicCommand, MusicSource, PlaySpec, RtConfig, Telemetry};
use crate::reverb::ReverbParams;
use crate::ring::{Consumer, Producer};

/// Handle of a playing voice (loops: keep it to move / stop the sound).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceHandle(pub u32);

/// How a sound is emitted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Emit {
    /// A point in the world (panned, attenuated, occluded, reverberant).
    At(Vec3),
    /// The local player's own body/ring: centred, present, never muffled as "remote".
    Own,
    /// 2D interface / feedback sound.
    Ui,
}

/// Optional per-play parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayOpts {
    pub emit: Emit,
    /// Event gain (dB) on top of the cue.
    pub gain_db: f32,
    /// Pitch multiplier on top of the micro-variation.
    pub pitch: f32,
    /// Sequence step (1-based) for `sequence` sounds (KO streak); None = shuffle bag.
    pub step: Option<u8>,
    /// 0..1 from an occlusion raycast.
    pub occlusion: f32,
    /// Added to the cue priority (threats aimed at the local player, etc.).
    pub priority_bonus: f32,
    /// Override the cue's bus.
    pub bus: Option<Bus>,
}

impl Default for PlayOpts {
    fn default() -> Self {
        Self { emit: Emit::Ui, gain_db: 0.0, pitch: 1.0, step: None, occlusion: 0.0, priority_bonus: 0.0, bus: None }
    }
}

impl PlayOpts {
    pub fn at(pos: Vec3) -> Self {
        Self { emit: Emit::At(pos), ..Default::default() }
    }
    pub fn own() -> Self {
        Self { emit: Emit::Own, ..Default::default() }
    }
    pub fn ui() -> Self {
        Self::default()
    }
    pub fn gain_db(mut self, g: f32) -> Self {
        self.gain_db = g;
        self
    }
    /// Linear gain convenience (callers used to pass linear multipliers).
    pub fn gain(mut self, g: f32) -> Self {
        self.gain_db = crate::dsp::lin_to_db(g.max(0.0));
        self
    }
    pub fn pitch(mut self, p: f32) -> Self {
        self.pitch = p;
        self
    }
    pub fn step(mut self, s: u8) -> Self {
        self.step = Some(s);
        self
    }
    pub fn occlusion(mut self, o: f32) -> Self {
        self.occlusion = o;
        self
    }
    pub fn priority(mut self, bonus: f32) -> Self {
        self.priority_bonus = bonus;
        self
    }
    pub fn bus(mut self, b: Bus) -> Self {
        self.bus = Some(b);
        self
    }
}

/// A recorded engine call (offline comparison renders replay these through another mixer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Recorded {
    Play { t: f64, sound: SoundId, opts: PlayOpts, handle: u32 },
    Update { t: f64, handle: u32, pos: Vec3, gain_db: f32, pitch: f32, occlusion: f32 },
    Stop { t: f64, handle: u32, fade: f32 },
}

/// Shuffle bag over up to 32 variations: plays each once per round, never the same take twice
/// in a row (also across round boundaries).
#[derive(Clone, Copy, Debug, Default)]
struct Bag {
    order: [u8; 32],
    len: u8,
    pos: u8,
    last: u8,
}

impl Bag {
    fn next(&mut self, n: usize, rng: &mut Rng) -> usize {
        let n = n.min(32);
        if n <= 1 {
            return 0;
        }
        if self.len as usize != n || self.pos >= self.len {
            for i in 0..n {
                self.order[i] = i as u8;
            }
            for i in (1..n).rev() {
                let j = rng.below(i + 1);
                self.order.swap(i, j);
            }
            if self.len as usize == n && self.order[0] == self.last {
                self.order.swap(0, n - 1);
            }
            self.len = n as u8;
            self.pos = 0;
        }
        let v = self.order[self.pos as usize];
        self.pos += 1;
        self.last = v;
        v as usize
    }
}

/// Game-thread audio API.
pub struct AudioEngine {
    tx: Producer<Cmd>,
    tel_rx: Consumer<Telemetry>,
    garbage_rx: Consumer<Garbage>,
    bank: Arc<SoundBank>,
    cues: Vec<CueParams>,
    bags: Vec<Bag>,
    last_play: Vec<f64>,
    cfg: MixConfig,
    rng: Rng,
    next_id: u32,
    time: f64,
    out_rate: u32,
    listener: Listener,
    telemetry: Telemetry,
    /// Commands dropped because the ring was full (should stay 0).
    pub dropped: u32,
    /// Plays dropped by cooldowns this session.
    pub cooled: u32,
    record: Option<Vec<Recorded>>,
}

impl AudioEngine {
    /// Build the engine and its audio-thread [`Mixer`] (hand that to [`crate::output`] or render
    /// it offline). `sample_rate` is the output rate.
    pub fn new(bank: Arc<SoundBank>, cues: &CueTable, cfg: MixConfig, sample_rate: u32, seed: u64) -> (AudioEngine, Mixer) {
        let (mixer, tx, tel_rx, garbage_rx) = mixer::build(&cfg, sample_rate);
        let resolved = cues.resolve_bank(&bank);
        let n = bank.len();
        let mut e = AudioEngine {
            tx,
            tel_rx,
            garbage_rx,
            bank: bank.clone(),
            cues: resolved,
            bags: vec![Bag::default(); n],
            last_play: vec![f64::NEG_INFINITY; n],
            cfg,
            rng: Rng::new(seed),
            next_id: 1,
            time: 0.0,
            out_rate: sample_rate,
            listener: Listener::default(),
            telemetry: Telemetry::default(),
            dropped: 0,
            cooled: 0,
            record: None,
        };
        e.send(Cmd::Bank(bank));
        e.send(Cmd::Mix(e.cfg.base_params()));
        (e, mixer)
    }

    fn send(&mut self, c: Cmd) -> bool {
        match self.tx.push(c) {
            Ok(()) => true,
            Err(_) => {
                self.dropped += 1;
                false
            }
        }
    }

    pub fn bank(&self) -> &Arc<SoundBank> {
        &self.bank
    }
    pub fn config(&self) -> &MixConfig {
        &self.cfg
    }
    pub fn sound(&self, name: &str) -> Option<SoundId> {
        self.bank.id(name)
    }
    pub fn cue(&self, id: SoundId) -> Option<&CueParams> {
        self.cues.get(id.0 as usize)
    }
    pub fn listener(&self) -> Listener {
        self.listener
    }
    /// Seconds of game time (advanced by [`AudioEngine::advance`]).
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Advance engine time (cooldowns) and collect telemetry / garbage. Call once per frame.
    pub fn advance(&mut self, dt: f32) {
        self.time += finite_or(dt, 0.0).max(0.0) as f64;
        while let Some(t) = self.tel_rx.pop() {
            self.telemetry = t;
        }
        // Freed here, on the game thread.
        while self.garbage_rx.pop().is_some() {}
    }
    pub fn telemetry(&self) -> Telemetry {
        self.telemetry
    }

    /// Play by name (unknown names are ignored — a missing sound is never an error).
    pub fn play(&mut self, name: &str, opts: PlayOpts) -> Option<VoiceHandle> {
        let id = self.bank.id(name)?;
        self.play_id(id, opts)
    }

    pub fn play_id(&mut self, id: SoundId, opts: PlayOpts) -> Option<VoiceHandle> {
        let s = self.bank.get(id)?;
        let n = s.vars.len();
        if n == 0 {
            return None;
        }
        let mut cue = *self.cues.get(id.0 as usize)?;
        if let Some(b) = opts.bus {
            cue.bus = b;
        }
        // Cooldown (retrigger guard for sounds that would otherwise machine-gun).
        let last = self.last_play.get(id.0 as usize).copied().unwrap_or(f64::NEG_INFINITY);
        if cue.cooldown > 0.0 && self.time - last < cue.cooldown as f64 {
            self.cooled += 1;
            return None;
        }
        if let Some(l) = self.last_play.get_mut(id.0 as usize) {
            *l = self.time;
        }
        let var = match (opts.step, s.sequence) {
            (Some(k), _) => (k.max(1) as usize - 1).min(n - 1),
            (None, true) => 0,
            (None, false) => self.bags.get_mut(id.0 as usize).map(|b| b.next(n, &mut self.rng)).unwrap_or(0),
        };
        let pcm = &s.vars[var];
        let cents = if s.sequence || opts.step.is_some() { 0.0 } else { self.rng.range(-cue.pitch_cents, cue.pitch_cents) };
        let jitter = self.rng.range(-cue.gain_jitter_db, cue.gain_jitter_db);
        let pitch = finite_or(opts.pitch, 1.0).clamp(0.05, 8.0) * 2f32.powf(cents / 1200.0);
        let rate = pitch * pcm.rate as f32 / self.out_rate.max(1) as f32;
        let (pos, spatial, local) = match opts.emit {
            Emit::At(p) if p.is_finite() => (p, true, false),
            Emit::At(_) => return None,
            Emit::Own => (self.listener.pos, false, true),
            Emit::Ui => (self.listener.pos, false, false),
        };
        let own = local && cue.bus.is_world();
        let gain_db = opts.gain_db + jitter + if own { self.cfg.voices.own_gain_db } else { 0.0 };
        let priority = cue.priority + opts.priority_bonus + if own { self.cfg.voices.own_priority } else { 0.0 };
        let h = VoiceHandle(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let spec = PlaySpec {
            id: h.0,
            sound: id,
            var: var as u16,
            cue,
            pos,
            spatial,
            local,
            gain_db: finite_or(gain_db, 0.0),
            rate,
            occlusion: finite_or(opts.occlusion, 0.0).clamp(0.0, 1.0),
            priority,
            looping: s.looping,
        };
        if !self.send(Cmd::Play(spec)) {
            return None;
        }
        if let Some(r) = self.record.as_mut() {
            r.push(Recorded::Play { t: self.time, sound: id, opts, handle: h.0 });
        }
        Some(h)
    }

    /// Start/stop recording every play/update/stop (offline tools only; allocates).
    pub fn set_recording(&mut self, on: bool) {
        self.record = if on { Some(Vec::new()) } else { None };
    }
    pub fn take_recording(&mut self) -> Vec<Recorded> {
        self.record.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Move / re-level a voice (loops, moving emitters). `gain_db` replaces the per-play gain.
    pub fn update(&mut self, h: VoiceHandle, pos: Vec3, gain_db: f32, pitch: f32, occlusion: f32) {
        self.send(Cmd::Update { id: h.0, pos, gain_db, pitch, occlusion });
        if let Some(r) = self.record.as_mut() {
            r.push(Recorded::Update { t: self.time, handle: h.0, pos, gain_db, pitch, occlusion });
        }
    }
    pub fn stop(&mut self, h: VoiceHandle, fade: f32) {
        self.send(Cmd::Stop { id: h.0, fade });
        if let Some(r) = self.record.as_mut() {
            r.push(Recorded::Stop { t: self.time, handle: h.0, fade });
        }
    }
    pub fn stop_all(&mut self, fade: f32) {
        self.send(Cmd::StopAll { fade });
    }
    pub fn set_listener(&mut self, pos: Vec3, fwd: Vec3, up: Vec3) {
        let l = Listener { pos, fwd, up };
        if l != self.listener {
            self.listener = l;
            self.send(Cmd::Listener(l));
        }
    }
    pub fn set_mix(&mut self, m: MixParams) {
        self.send(Cmd::Mix(m));
    }
    pub fn set_reverb(&mut self, r: ReverbParams) {
        self.send(Cmd::Reverb(r));
    }
    pub fn duck(&mut self, d: DuckSpec) {
        self.send(Cmd::Duck(d));
    }
    /// A named duck from `mix.ron` (`heavy_hit`, `ko_confirm`, `final_blow`).
    pub fn duck_named(&mut self, name: &str) {
        let d = self.cfg.duck(name);
        self.duck(d);
    }
    pub fn set_settings(&mut self, s: AudioSettings) {
        self.send(Cmd::Settings(s));
    }
    /// Hot reload of `mix.ron` (and optionally `cues.ron`).
    pub fn reload(&mut self, cfg: MixConfig, cues: Option<&CueTable>) {
        if let Some(c) = cues {
            self.cues = c.resolve_bank(&self.bank);
        }
        self.send(Cmd::Config(RtConfig::from_mix(&cfg)));
        self.cfg = cfg;
    }
    /// Swap the whole sound bank (hot reload of `assets/sfx`). Playing voices stop; the old bank is
    /// handed back and freed here on the game thread.
    pub fn set_bank(&mut self, bank: Arc<SoundBank>, cues: &CueTable) {
        self.cues = cues.resolve_bank(&bank);
        self.bags = vec![Bag::default(); bank.len()];
        self.last_play = vec![f64::NEG_INFINITY; bank.len()];
        self.bank = bank.clone();
        self.send(Cmd::Bank(bank));
    }
    pub fn set_music(&mut self, m: Box<dyn MusicSource>) {
        self.send(Cmd::SetMusic(m));
    }
    pub fn music(&mut self, c: MusicCommand) {
        self.send(Cmd::Music(c));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffle_bag_covers_all_and_never_repeats_back_to_back() {
        let mut bag = Bag::default();
        let mut rng = Rng::new(1);
        let mut prev = usize::MAX;
        let mut counts = [0usize; 5];
        for _ in 0..1000 {
            let v = bag.next(5, &mut rng);
            assert_ne!(v, prev, "direct repeat");
            counts[v] += 1;
            prev = v;
        }
        assert!(counts.iter().all(|&c| c == 200), "{counts:?}");
        assert_eq!(bag.next(1, &mut rng), 0);
    }
}
