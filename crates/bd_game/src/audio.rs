//! Sound: the whole client audio path on bd_audio — one software mixer (buses, shared reverb from
//! voxel-ray room estimation, occlusion, HDR window, snapshots, ambience, music) on one output
//! stream. See crates/bd_audio for the engine and `assets/audio/*.ron` for the mix.
//!
//! * Call sites keep their API: [`PlaySfx`] (the local player's own / 2D sounds) and
//!   [`crate::present::sound::Sfx3d`] / [`SfxLoopEmitter`] / [`MixDuck`] (world sounds) are
//!   forwarded to [`bd_audio::AudioEngine`] every frame.
//! * Local movement (jump, land, tethers, boost, wall hits, footsteps) is voiced from
//!   `FrameMoveEvents`; own loops (wind, boost, reel, slide) follow the local player's state.
//! * Everyone else's footsteps are synthesised from the replicated views (the voice limiter keeps
//!   the nearest three); charge auras and shout words are voiced here too.
//! * Every game event feeds the mix director (combat heat, heavy-hit / KO / final-blow ducks,
//!   danger heartbeat), the ambience (explosions scare the birds) and the soundtrack (stingers).
//! * Power cues ([`PowerCue`], `present::powers::ascend`): an AWAKENING → the streak stinger (the
//!   caster) / a legendary sting at the spot (anyone else); every Ascend burst → a whoomp + sub hit;
//!   a form's music layer ("drums": Gear Fifth) → the soundtrack's overlay layer on / off
//!   ([`power_cue_plan`]).
//! * Franchise themes ([`themes`]): the LOCAL player's own power uses (moves, forms, gains, swaps)
//!   of a franchise swell its original theme in on the music's theme layer (bd_audio's
//!   `ThemeDirector`: keep-alive, minimum hold, re-entry cooldown); Settings → Sound → Franchise
//!   themes turns it off.
//! * Loading: `sfx/` is decoded on a worker thread during the loading screen. With `--record` the
//!   mixer runs offline in lock-step with the fixed capture clock and writes `<record>.wav`
//!   (nothing is played aloud); otherwise one cpal output stream.

use crate::arena::{ArenaWorld, MapKind};
use crate::camera::PlayerCamera;
use crate::player::{FrameMoveEvents, LocalPlayer, PlayerSim};
use crate::present::gv;
use crate::present::powers::ascend::PowerCue;
use crate::present::sound::{MixDuck, Sfx3d, SfxLoopEmitter};
use crate::settings::Settings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::{AppState, InGameMenu};
use crate::tick::FrameSet;
use crate::util::v3;
use crate::warmup::LoadingBlockers;
use bd_audio::acoustics::ListenerSpace;
use bd_audio::game::{SimAudio, SimCtx, corps_key};
use bd_audio::{
    AmbienceDirector, AudioEngine, AudioSettings, EmitterMap, MapAudioTable, MixConfig, MixDirector, MixInputs, PlayOpts, Scene,
    VoiceHandle,
};
use bd_core::PlayerId;
use bd_core::input::buttons;
use bd_sim::GameEvent;
use bd_sim::replicate::pflags;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};

/// Play a one-shot as the local player / 2D. `gain` multiplies (linear), `pitch` multiplies the
/// cue's micro-variation.
#[derive(Message, Clone, Debug)]
pub struct PlaySfx {
    pub name: Cow<'static, str>,
    pub gain: f32,
    pub pitch: f32,
}

impl PlaySfx {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self { name: name.into(), gain: 1.0, pitch: 1.0 }
    }
    pub fn gain(mut self, g: f32) -> Self {
        self.gain = g;
        self
    }
    pub fn pitch(mut self, p: f32) -> Self {
        self.pitch = p;
        self
    }
}

/// Settings → Sound (persisted in `settings.ron` under `audio`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioPrefs {
    pub world: f32,
    pub ambience: f32,
    pub feedback: f32,
    pub music: f32,
    /// Reduced dynamic range.
    pub night_mode: bool,
    pub mono: bool,
    pub music_style: MusicStylePref,
    /// Franchise themes: using a franchise's powers swells its (original) theme in over the map
    /// music. Missing in older settings files = on.
    pub franchise_themes: bool,
}

impl Default for AudioPrefs {
    fn default() -> Self {
        Self {
            world: 1.0,
            ambience: 1.0,
            feedback: 1.0,
            music: 0.7,
            night_mode: false,
            mono: false,
            music_style: MusicStylePref::Classic,
            franchise_themes: true,
        }
    }
}

impl AudioPrefs {
    pub fn sanitized(mut self) -> Self {
        let c = |v: f32, d: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
        self.world = c(self.world, 1.0);
        self.ambience = c(self.ambience, 1.0);
        self.feedback = c(self.feedback, 1.0);
        self.music = c(self.music, 0.7);
        self
    }
}

/// Settings → Music style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MusicStylePref {
    #[default]
    Classic,
    Remixes,
    Shuffle,
}

impl MusicStylePref {
    pub fn next(self, dir: i32) -> Self {
        let all = [Self::Classic, Self::Remixes, Self::Shuffle];
        let i = all.iter().position(|s| *s == self).unwrap_or(0) as i32;
        all[(i + dir).rem_euclid(3) as usize]
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "CLASSIC",
            Self::Remixes => "REMIXES",
            Self::Shuffle => "SHUFFLE",
        }
    }
}

/// Everything audio on the game thread.
#[derive(Resource)]
pub struct Audio {
    pub engine: AudioEngine,
    pub director: MixDirector,
    pub sim: SimAudio,
    space: ListenerSpace,
    amb: Option<AmbienceDirector>,
    /// World the ambience was scanned from (pointer identity) and its map.
    amb_for: Option<(usize, MapKind)>,
    maps: MapAudioTable,
    mix: MixConfig,
    #[cfg(feature = "music")]
    music: Option<bd_audio::music::MusicDirector>,
    loops: HashMap<Entity, (VoiceHandle, &'static str)>,
    loop_rr: usize,
    own: [Option<VoiceHandle>; 4],
    own_level: [f32; 4],
    stride: f32,
    remote_prev: HashMap<PlayerId, bd_core::Vec3>,
    ear: Vec3,
    prev_ear: Vec3,
    settings: Option<(f32, AudioPrefs)>,
    alive_players: usize,
    match_point_fired: bool,
    ended: bool,
    /// Orbital strike loops (telegraph hum / beam) per emitter construct.
    strikes: Vec<(bd_core::ConstructId, VoiceHandle)>,
    /// Hook rope reel loops per (hooker, victim).
    reels: Vec<(PlayerId, PlayerId, VoiceHandle, bool)>,
    ads_active: bool,
    /// Domain drone loops per construct.
    domains: Vec<(bd_core::ConstructId, VoiceHandle)>,
    /// Main-thread cost of the audio systems per frame (µs), logged every 1024 frames.
    timing: (Option<std::time::Instant>, Vec<f32>),
}

type Loaded = Result<(bd_audio::AudioAssets, Option<MusicLib>), String>;

#[cfg(feature = "music")]
type MusicLib = std::sync::Arc<bd_music::MusicLibrary>;
#[cfg(not(feature = "music"))]
type MusicLib = ();

#[derive(Resource)]
struct AudioLoading(Mutex<Option<Receiver<Loaded>>>);

/// Where the mixer's output goes.
enum Sink {
    Device(#[allow(dead_code)] bd_audio::output::AudioOutput),
    Offline(OfflineCapture),
    None,
}

/// `--record`: the mixer rendered in lock-step with the fixed capture clock into a WAV.
struct OfflineCapture {
    mixer: bd_audio::Mixer,
    path: PathBuf,
    samples: Vec<i16>,
    t_game: f64,
    frames_left: usize,
    fps: f32,
    written: bool,
    carry: f64,
}

pub struct AudioFxPlugin;

impl Plugin for AudioFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySfx>()
            // Audio reads cues from features whose plugins may not be added yet; registering a
            // message twice is a no-op, reading an unregistered one panics at startup.
            .add_message::<crate::present::tracking::TrackingCue>()
            .add_message::<crate::present::signature::SignatureCue>()
            .add_message::<bd_fx::ShoutSfx>()
            .add_message::<crate::sim_bridge::GameEvents>()
            .add_message::<PowerCue>()
            // `BD_PERF_OFF=audio`: no audio at all (A/B perf checks).
            .add_systems(Startup, start_load.run_if(|| !crate::present::perf_off("audio")))
            .add_systems(Update, finish_load.run_if(resource_exists::<AudioLoading>))
            .add_systems(OnExit(AppState::InGame), leave_game)
            .add_systems(
                Update,
                (timing_begin, forward_messages, local_movement, world_audio, hooks_audio, power_audio, apply_settings, timing_end)
                    .chain()
                    .after(FrameSet::Present)
                    .run_if(resource_exists::<Audio>),
            )
            .add_systems(Last, offline_capture);
        // Franchise themes: after the frame's events are out, beside the other audio systems.
        #[cfg(feature = "music")]
        app.add_systems(Update, themes::franchise_themes.after(FrameSet::Present).after(apply_settings).run_if(resource_exists::<Audio>));
    }
}

fn start_load(mut commands: Commands, mut blockers: ResMut<LoadingBlockers>) {
    let (tx, rx) = channel::<Loaded>();
    let dir = crate::assets_dir();
    let spawned = std::thread::Builder::new().name("audio-load".into()).spawn(move || {
        let t0 = std::time::Instant::now();
        let r = bd_audio::AudioAssets::load(&dir).map_err(|e| e.to_string()).map(|a| {
            #[cfg(feature = "music")]
            let music = match bd_music::MusicLibrary::load_dir(&dir.join("music")) {
                Ok(l) => Some(std::sync::Arc::new(l)),
                Err(e) => {
                    eprintln!("music: {e}");
                    None
                }
            };
            #[cfg(not(feature = "music"))]
            let music = None;
            (a, music)
        });
        info!("audio: assets loaded in {:.0} ms", t0.elapsed().as_secs_f32() * 1000.0);
        let _ = tx.send(r);
    });
    if spawned.is_ok() {
        blockers.audio_pending = true;
        commands.insert_resource(AudioLoading(Mutex::new(Some(rx))));
    }
}

/// `--record` target → `<file>.wav` / `<dir>/audio.wav`.
fn record_wav_path(world: &World) -> Option<PathBuf> {
    let rec = world.get_resource::<crate::practice::SimArgs>()?.record.clone()?;
    Some(if rec.extension().is_some() { rec.with_extension("wav") } else { rec.join("audio.wav") })
}

fn record_fps() -> f32 {
    let a: Vec<String> = std::env::args().collect();
    a.windows(2)
        .find(|w| w[0] == "--record-fps")
        .and_then(|w| w[1].parse::<f32>().ok())
        .filter(|f| (1.0..=240.0).contains(f))
        .unwrap_or(30.0)
}

fn finish_load(world: &mut World) {
    let got = {
        let Some(l) = world.get_resource::<AudioLoading>() else { return };
        let Ok(mut g) = l.0.lock() else { return };
        match g.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(r)) => {
                g.take();
                Some(r)
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => None,
            _ => {
                g.take();
                Some(Err("audio loader thread died".into()))
            }
        }
    };
    let Some(result) = got else { return };
    world.remove_resource::<AudioLoading>();
    if let Some(mut b) = world.get_resource_mut::<LoadingBlockers>() {
        b.audio_pending = false;
    }
    let (assets, music_lib) = match result {
        Ok(v) => v,
        Err(e) => {
            warn!("audio: {e}; running silent");
            return;
        }
    };
    for w in &assets.warnings {
        warn!("audio: {w}");
    }
    let record = record_wav_path(world);
    let rate = if record.is_some() { 48_000 } else { bd_audio::output::device_rate().map(|(r, _)| r).unwrap_or(48_000) };
    #[allow(unused_mut)]
    let (mut engine, mixer) = AudioEngine::new(assets.bank.clone(), &assets.cues, assets.mix.clone(), rate, 0xb1d);
    #[cfg(feature = "music")]
    let music = music_lib.map(|lib| {
        mixer_set_music(&mut engine, &lib, rate);
        bd_audio::music::MusicDirector::new(lib, bd_audio::music::MusicStyle::Classic, 0x6d)
    });
    #[cfg(not(feature = "music"))]
    let _ = music_lib;
    let sink = if let Some(path) = record {
        let args = world.get_resource::<crate::practice::SimArgs>().cloned().unwrap_or_default();
        let fps = record_fps();
        let frames = (args.record_secs.max(0.5) * fps).round() as usize;
        Sink::Offline(OfflineCapture {
            mixer,
            path,
            samples: Vec::new(),
            t_game: 0.0,
            frames_left: frames,
            fps,
            written: false,
            carry: 0.0,
        })
    } else {
        match bd_audio::output::start(mixer, 256) {
            Ok(o) => {
                info!("audio: output {} Hz, {} ch, buffer {:?}", o.sample_rate, o.channels, o.buffer_frames);
                Sink::Device(o)
            }
            Err(e) => {
                warn!("audio: no output ({e}); running silent");
                Sink::None
            }
        }
    };
    world.insert_non_send(sink);
    let maps = assets.maps.clone();
    let space = ListenerSpace::new(maps.get("archipelago").space);
    world.insert_resource(Audio {
        engine,
        director: MixDirector::new(),
        sim: SimAudio::new(),
        space,
        amb: None,
        amb_for: None,
        maps,
        mix: assets.mix.clone(),
        #[cfg(feature = "music")]
        music,
        loops: HashMap::default(),
        loop_rr: 0,
        own: [None; 4],
        own_level: [0.0; 4],
        stride: 0.0,
        remote_prev: HashMap::default(),
        ear: Vec3::ZERO,
        prev_ear: Vec3::ZERO,
        settings: None,
        alive_players: 0,
        match_point_fired: false,
        ended: false,
        strikes: Vec::new(),
        reels: Vec::new(),
        ads_active: false,
        domains: Vec::new(),
        timing: (None, Vec::with_capacity(1024)),
    });
    info!("audio: {} sounds, {:.0} MB PCM, {} Hz", assets.bank.len(), assets.bank.bytes() as f32 / 1e6, rate);
}

#[cfg(feature = "music")]
fn mixer_set_music(engine: &mut AudioEngine, lib: &std::sync::Arc<bd_music::MusicLibrary>, rate: u32) {
    engine.set_music(Box::new(bd_audio::music::ChipMusic::new(lib.clone(), rate)));
}

fn map_key(k: MapKind) -> &'static str {
    match k {
        MapKind::Arena => "archipelago",
        MapKind::Tutorial => "tutorial",
        MapKind::Miami => "miami",
        MapKind::Orbit => "orbit",
        MapKind::Backrooms => "backrooms",
        MapKind::Sakura => "sakura",
        MapKind::Ember => "ember",
        MapKind::Mirrorfold => "mirror",
        MapKind::Casino => "casino",
        MapKind::Valley => "valley",
        MapKind::Cybertron => "cybertron",
        MapKind::Lair => "lair",
        MapKind::GiantsRoom => "giant",
        MapKind::Hyperline => "hyperline",
        MapKind::Lookout => "lookout",
        // TRAINING FIELD: a practice tool, not a themed map — plain arena music.
        MapKind::Training => "archipelago",
    }
}

fn db(g: f32) -> f32 {
    if g.is_finite() && g > 1e-6 { 20.0 * g.log10() } else { -120.0 }
}

fn alias(name: &str) -> &str {
    match name {
        "shutdown" => "ko_streak_end",
        "void" => "ko_accent_void",
        n => n,
    }
}

/// Call-site messages → engine (+ occlusion for world one-shots, budgeted).
#[allow(clippy::too_many_arguments)]
fn forward_messages(
    mut audio: ResMut<Audio>,
    mut play: MessageReader<PlaySfx>,
    mut world: MessageReader<Sfx3d>,
    mut duck: ResMut<MixDuck>,
    arena: Option<Res<ArenaWorld>>,
    added: Query<(Entity, &SfxLoopEmitter), Added<SfxLoopEmitter>>,
    emitters: Query<(Entity, &SfxLoopEmitter, &GlobalTransform)>,
    mut removed: RemovedComponents<SfxLoopEmitter>,
) {
    let off = crate::present::perf_off("audio");
    let a = &mut *audio;
    let ear = a.ear;
    for e in play.read() {
        if !off {
            a.engine.play(alias(&e.name), PlayOpts::own().gain_db(db(e.gain)).pitch(e.pitch));
        }
    }
    let vox = arena.as_ref().map(|w| w.read());
    let mut budget = 16u32;
    for e in world.read() {
        if off {
            continue;
        }
        let mut o = match e.pos {
            Some(p) => {
                let mut o = PlayOpts::at(gv(p));
                if budget > 0
                    && let Some(v) = vox.as_deref()
                    && p.distance(ear) < 120.0
                {
                    budget -= 1;
                    o = o.occlusion(bd_audio::occlusion(v, gv(ear), gv(p)));
                }
                o
            }
            None => PlayOpts::ui(),
        };
        o = o.gain_db(db(e.gain)).pitch(e.pitch);
        if let Some(s) = e.step {
            o = o.step(s);
        }
        a.engine.play(alias(&e.name), o);
    }
    if duck.pending > 0.0 {
        let mut d = a.mix.duck("ko_confirm");
        d.hold = d.hold.max(duck.pending - d.attack - d.release);
        a.engine.duck(d);
        duck.pending = 0.0;
    }
    // Loops: start / follow / stop with their entity.
    for (e, em) in &added {
        if let Some((h, _)) = a.loops.remove(&e) {
            a.engine.stop(h, 0.0);
        }
        if let Some(h) = a.engine.play(em.name, PlayOpts::at(gv(ear)).gain_db(-120.0)) {
            a.loops.insert(e, (h, em.name));
        }
    }
    for e in removed.read() {
        if let Some((h, _)) = a.loops.remove(&e) {
            a.engine.stop(h, 0.0);
        }
    }
    let n = a.loops.len().max(1);
    let rr = a.loop_rr;
    a.loop_rr = a.loop_rr.wrapping_add(1);
    for (i, (e, em, gt)) in emitters.iter().enumerate() {
        let Some(&(h, name)) = a.loops.get(&e) else { continue };
        if name != em.name {
            a.engine.stop(h, 0.05);
            a.loops.remove(&e);
            continue;
        }
        let p = gt.translation();
        // Occlusion for a few loops per frame (round robin); others keep their last value.
        let occ = if (i + rr) % n < 4 { vox.as_deref().map_or(0.0, |v| bd_audio::occlusion(v, gv(ear), gv(p))) } else { f32::NAN };
        a.engine.update(h, gv(p), db(em.gain), em.pitch.clamp(0.25, 4.0), occ);
    }
}

/// The local player's own movement sounds + loops.
fn local_movement(
    mut audio: ResMut<Audio>,
    time: Res<Time>,
    events: Res<FrameMoveEvents>,
    player: Query<&PlayerSim, With<LocalPlayer>>,
    inputs: Res<crate::input::LocalInputState>,
    state: Res<State<AppState>>,
) {
    let a = &mut *audio;
    let in_game = *state.get() == AppState::InGame;
    for ev in &events.0 {
        a.sim.local_move(ev, &mut a.engine);
    }
    let Ok(sim) = player.single() else {
        for h in a.own.iter_mut().filter_map(Option::take) {
            a.engine.stop(h, 0.2);
        }
        return;
    };
    let s = &sim.state;
    // Footsteps from distance walked on the ground.
    if s.grounded && !s.sliding {
        let d = v3(s.pos - sim.prev.pos);
        // Once per tick (frames without a tick add nothing; `max(1)` made steps faster at high fps).
        // viewmodel_anim's hand bob mirrors this rule so the bob lands on the footstep.
        a.stride += Vec2::new(d.x, d.z).length() * events.0.len() as f32;
        if a.stride > 2.6 {
            a.stride = 0.0;
            a.engine.play("footstep", PlayOpts::own());
        }
    } else {
        a.stride = 1.8;
    }
    // Own loops: wind (speed), boost, reel, slide.
    let speed = v3(s.vel).length();
    let reeling = s.tethered() && (inputs.current.held(buttons::REEL) || inputs.current.reel_notches != 0);
    let dt = time.delta_secs();
    const NAMES: [&str; 4] = ["wind_loop", "boost_loop", "tether_reel_loop", "slide_loop"];
    for (i, name) in NAMES.iter().enumerate() {
        let (target, pitch) = match i {
            0 => {
                let k = ((speed - 6.0) / 30.0).clamp(0.0, 1.0);
                (k * k * 1.2, 0.8 + k * 0.6)
            }
            1 => (if s.boosting { 1.0 } else { 0.0 }, 1.0),
            2 => (if reeling { 1.0 } else { 0.0 }, 1.0),
            _ => (if s.sliding { (speed / 14.0).clamp(0.3, 1.0) } else { 0.0 }, 0.9 + (speed / 40.0).min(0.3)),
        };
        let target = if in_game { target } else { 0.0 };
        let rate = if target > a.own_level[i] { 25.0 } else { 8.0 };
        a.own_level[i] += (target - a.own_level[i]) * (1.0 - (-rate * dt).exp());
        match a.own[i] {
            None if a.own_level[i] > 0.01 => a.own[i] = a.engine.play(name, PlayOpts::own().gain_db(db(a.own_level[i]))),
            Some(h) if a.own_level[i] < 0.005 && target == 0.0 => {
                a.engine.stop(h, 0.05);
                a.own[i] = None;
            }
            Some(h) => {
                let ear = a.ear;
                a.engine.update(h, gv(ear), db(a.own_level[i]), pitch, 0.0);
            }
            None => {}
        }
    }
}

/// Listener, acoustics, events → mix / ambience / music, remote footsteps, charge, shout.
#[allow(clippy::too_many_arguments)]
fn world_audio(
    mut audio: ResMut<Audio>,
    time: Res<Time>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    arena: Option<Res<ArenaWorld>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    console: Option<Res<crate::console::RingConsole>>,
    mut events: MessageReader<GameEvents>,
    mut shouts: MessageReader<bd_fx::ShoutSfx>,
) {
    let a = &mut *audio;
    let dt = time.delta_secs();
    a.sim.begin_frame(dt, &mut a.engine);
    let (ear, fwd, up) =
        cam.single().map(|t| (t.translation(), t.forward().as_vec3(), t.up().as_vec3())).unwrap_or((Vec3::ZERO, Vec3::NEG_Z, Vec3::Y));
    a.prev_ear = a.ear;
    a.ear = ear;
    let vel = if dt > 0.0 { (ear - a.prev_ear) / dt } else { Vec3::ZERO };
    let vel = if vel.length() < 200.0 { vel } else { Vec3::ZERO };
    a.engine.set_listener(gv(ear), gv(fwd), gv(up));

    let in_game = *state.get() == AppState::InGame;
    let local = active.as_ref().and_then(|w| w.local);
    let me = local.and_then(|id| views.player(id));
    let map = arena.as_ref().map(|w| w.info.kind).unwrap_or_default();
    let key = map_key(map);

    // Acoustics + ambience from the live voxels.
    if let Some(arena) = arena.as_ref() {
        let vox = arena.read();
        let id = std::sync::Arc::as_ptr(&arena.world) as usize;
        if a.amb_for != Some((id, map)) {
            if let Some(mut old) = a.amb.take() {
                old.stop(&mut a.engine);
            }
            let t0 = std::time::Instant::now();
            let b = vox.bounds();
            let emitters = EmitterMap::scan(&*vox, b.min, b.max, arena.info.water_y, 4.0);
            let m = a.maps.get(key);
            a.space = ListenerSpace::new(m.space);
            a.amb = Some(AmbienceDirector::new(m, emitters, 0x5eed));
            a.amb_for = Some((id, map));
            info!("audio: {key} ambience scanned in {:.1} ms", t0.elapsed().as_secs_f32() * 1000.0);
        }
        a.space.step(&*vox, gv(ear), 4);
        let rv = a.space.reverb(dt);
        a.engine.set_reverb(rv);
        if let Some(amb) = a.amb.as_mut() {
            amb.enclosure = a.space.estimate_now().enclosure;
            amb.update(dt, gv(ear), gv(vel), &mut a.engine, Some(&*vox));
        }

        // Events: the mix reacts (presenters voice them through the shims).
        let pos_of = |id: PlayerId| views.player(id).map(|p| gv(p.pos) + bd_core::Vec3::Y * 0.9);
        let corps_of = |id: PlayerId| views.player(id).map(|p| corps_key(p.corps));
        let ctx = SimCtx {
            voxels: &*vox,
            ear: gv(ear),
            local,
            pos_of: &pos_of,
            corps_of: &corps_of,
            local_move_predicted: true,
            match_point: false,
        };
        for ev in events.read() {
            match &ev.event {
                // The KO duck comes from the presenter's MixDuck request.
                GameEvent::Ko { killer, streak, .. } => {
                    #[cfg(feature = "music")]
                    if let Some(m) = a.music.as_mut()
                        && killer.is_some()
                        && *killer == local
                    {
                        let k = if *streak >= 2 { bd_music::StingerKind::Streak } else { bd_music::StingerKind::Ko };
                        m.stinger(k, &mut a.engine);
                    }
                    let _ = (killer, streak);
                }
                GameEvent::ConstructRecalled { to, .. } => {
                    a.engine.play("recall_blink", PlayOpts::at(*to));
                }
                GameEvent::OrbitalStrike { id, phase, point, .. } => {
                    use bd_sim::signature::StrikePhase as S;
                    if let Some(i) = a.strikes.iter().position(|s| s.0 == *id) {
                        let (_, h) = a.strikes.swap_remove(i);
                        a.engine.stop(h, if *phase == S::End { 0.4 } else { 0.05 });
                    }
                    let name = match phase {
                        S::Telegraph => Some("strike_telegraph"),
                        S::Strike => Some("strike_beam_loop"),
                        S::End => None,
                    };
                    if let Some(n) = name
                        && let Some(h) = a.engine.play(n, PlayOpts::at(*point))
                    {
                        a.strikes.push((*id, h));
                    }
                }
                GameEvent::Charge { player, kind, .. } => {
                    if let Some(p) = pos_of(*player) {
                        a.sim.charge(&ctx, *player, *kind, p, &mut a.engine);
                    }
                }
                e => a.sim.react(e, &ctx, &mut a.engine, &mut a.director, a.amb.as_mut()),
            }
            #[cfg(feature = "music")]
            if let (Some(m), Some(w)) = (a.music.as_mut(), active.as_ref()) {
                music_moments(&ev.event, w, &mut a.alive_players, &mut a.match_point_fired, m, &mut a.engine);
            }
        }
        a.sim.charge_frame(local, &pos_of, &mut a.engine);

        // Everyone else's footsteps from the replicated views.
        for p in views.players.iter().filter(|p| !p.is_local) {
            let pos = gv(p.pos);
            let prev = a.remote_prev.insert(p.id, pos).unwrap_or(pos);
            let moved = Vec2::new(pos.x - prev.x, pos.z - prev.z).length();
            let grounded = p.flags & pflags::GROUNDED != 0 && p.flags & pflags::ALIVE != 0 && p.flags & pflags::SLIDING == 0;
            a.sim.footsteps(&ctx, p.id, pos, if moved < 3.0 { moved } else { 0.0 }, grounded, &mut a.engine);
        }
    } else {
        events.clear();
    }

    // Shout words (bd_fx hook).
    for s in shouts.read() {
        use bd_fx::ShoutPhase as P;
        let name = match s.phase {
            P::Erupt => "shout_erupt",
            P::Slam => "shout_slam",
            P::Release { .. } => "shout_release",
            P::Fizzle => "shout_fizzle",
        };
        let o = if s.local { PlayOpts::own() } else { PlayOpts::at(gv(s.pos)) };
        a.engine.play(name, o.gain_db(db((0.6 + 0.4 * s.intensity).clamp(0.3, 1.5))));
    }

    // Mix state.
    let alive = me.is_none_or(|p| p.flags & pflags::ALIVE != 0);
    let vacuum = if map == MapKind::Orbit { a.space.preset.vacuum(ear.y) } else { 0.0 };
    let ring_open = console.is_some_and(|c| c.open);
    let inputs = MixInputs {
        scene: if in_game { Scene::InGame } else { Scene::Menu },
        damage_pct: me.map_or(0.0, |p| p.damage_pct),
        alive,
        ring_open,
        vacuum,
        sudden_death: false,
        paused: in_game && menu.0,
    };
    a.director.update(dt, &inputs, &a.mix, &mut a.engine);

    #[cfg(feature = "music")]
    if let Some(m) = a.music.as_mut() {
        let (results, party, final_stock) = match active.as_ref() {
            Some(w) if in_game => {
                let ws = &w.world;
                let ended = ws.match_state.phase == bd_sim::MatchPhase::Ended;
                let won = ended && ws.match_state.winner.is_some() && ws.match_state.winner == local;
                let final_stock = match ws.rules().scoring {
                    bd_sim::rules::Scoring::Stock { .. } => me.is_some_and(|p| p.stocks == 1),
                    _ => ws.match_state.live() && ws.match_state.time_left(ws.rules(), &ws.cfg.timings) < 60.0,
                };
                (ended.then_some(won), ws.party.is_some(), final_stock)
            }
            _ => (None, false, false),
        };
        let mi = bd_audio::music::MusicInputs {
            scene: inputs.scene,
            map: key,
            combat: a.director.weights().combat,
            final_stock,
            sudden_death: false,
            party,
            results,
        };
        m.update(dt, &mi, &mut a.engine);
    }
    a.engine.advance(dt);
}

/// In-flight workers' audio hooks: tracking / hook ropes, ADS.
fn hooks_audio(
    mut audio: ResMut<Audio>,
    mut tracking: MessageReader<crate::present::tracking::TrackingCue>,
    reels: Option<Res<crate::present::tracking::HookReels>>,
    ads: Option<Res<crate::present::ads::AdsState>>,
    mut signature: MessageReader<crate::present::signature::SignatureCue>,
) {
    use crate::present::tracking::TrackingCueKind as K;
    let a = &mut *audio;
    // Signature spectacles (cut-in alarm, domains, ultimates).
    for s in signature.read() {
        use crate::present::signature::Cue as C;
        let at = if s.mine { PlayOpts::own() } else { PlayOpts::at(gv(s.pos)) };
        match s.cue {
            C::AlertDanger => {
                let sev = s.severity.clamp(0.0, 1.0);
                a.engine.play("alert_danger", PlayOpts::ui().gain_db(-6.0 + 6.0 * sev).pitch(1.0 + 0.12 * sev));
            }
            C::DomainHum { on } => {
                if let Some(i) = a.domains.iter().position(|d| d.0 == s.construct) {
                    let (_, h) = a.domains.swap_remove(i);
                    a.engine.stop(h, 0.3);
                }
                if on && let Some(h) = a.engine.play("domain_hum_loop", PlayOpts::at(gv(s.pos))) {
                    a.domains.push((s.construct, h));
                }
            }
            C::WorldDuck => {
                let mut d = a.mix.duck("heavy_hit");
                d.hold = 0.3;
                a.engine.duck(d);
            }
            C::MusicStinger =>
            {
                #[cfg(feature = "music")]
                if let Some(m) = a.music.as_mut() {
                    m.stinger(bd_music::StingerKind::Streak, &mut a.engine);
                }
            }
            c => {
                a.engine.play(c.name(), at);
            }
        }
    }
    for c in tracking.read() {
        let name = match c.kind {
            K::HookThwip => "hook_fire",
            K::HookSnap => "hook_break",
            K::SlamReady => "hook_attach",
            K::Slam => "hook_slam",
            K::Lunge => "lunge",
            K::RevealPing => "reveal_pulse",
            K::GlowOn => continue,
        };
        let o = if c.local && matches!(c.kind, K::Lunge | K::HookThwip) { PlayOpts::own() } else { PlayOpts::at(gv(c.pos)) };
        a.engine.play(name, o);
    }
    // Rope reel loops: one per live rope, pitched by closing speed.
    if let Some(r) = reels {
        for e in a.reels.iter_mut() {
            e.3 = false;
        }
        for rope in &r.0 {
            let pitch = 0.9 + (rope.closing / 30.0).clamp(0.0, 0.6);
            let gain = -6.0 + (rope.closing / 20.0).clamp(0.0, 1.0) * 6.0;
            if let Some(e) = a.reels.iter_mut().find(|e| e.0 == rope.hooker && e.1 == rope.victim) {
                e.3 = true;
                a.engine.update(e.2, gv(rope.pos), gain, pitch, f32::NAN);
            } else if let Some(h) = a.engine.play("hook_reel_loop", PlayOpts::at(gv(rope.pos)).gain_db(gain)) {
                a.reels.push((rope.hooker, rope.victim, h, true));
            }
        }
        let engine = &mut a.engine;
        a.reels.retain(|e| {
            if !e.3 {
                engine.stop(e.2, 0.08);
            }
            e.3
        });
    }
    if let Some(ads) = ads
        && ads.active != a.ads_active
    {
        a.ads_active = ads.active;
        a.engine.play(if ads.active { "ads_in" } else { "ads_out" }, PlayOpts::own());
    }
}

/// What one [`PowerCue`] sounds like.
#[derive(Clone, Debug, PartialEq)]
pub enum PowerSound {
    /// A soundtrack stinger, by `bd_music::StingerKind::name` (the existing ones).
    Stinger(&'static str),
    /// A one-shot: `at` = `None` for the local player's own (2D), else in the world.
    Sfx { name: &'static str, at: Option<Vec3>, gain_db: f32, pitch: f32 },
    /// A soundtrack overlay layer on / off ("drums").
    Overlay { layer: String, on: bool },
}

/// The audio a [`PowerCue`] maps to (pure: unit-tested).
pub fn power_cue_plan(c: &PowerCue) -> Vec<PowerSound> {
    match c {
        // The first time you take a form this round: the music answers (the caster), a sting at the
        // spot for everyone else (the aura + gather sounds are 3D already).
        PowerCue::Awakening { local: true, .. } => vec![PowerSound::Stinger("streak")],
        PowerCue::Awakening { pos, .. } => vec![PowerSound::Sfx { name: "legendary_sting", at: Some(*pos), gain_db: -8.0, pitch: 0.9 }],
        // Every Ascend burst: a low whoomp + a sub hit, deeper and louder for stronger forms.
        PowerCue::Burst { pos, intensity, local, .. } => {
            let k = if intensity.is_finite() { intensity.clamp(0.3, 3.0) } else { 1.0 };
            let at = if *local { None } else { Some(*pos) };
            let big = ((k - 0.3) / 2.7).clamp(0.0, 1.0);
            vec![
                PowerSound::Sfx { name: "ring_blast", at, gain_db: -4.0 + 4.0 * big, pitch: (0.78 - 0.22 * big).clamp(0.5, 0.9) },
                PowerSound::Sfx { name: "kaiju_stomp", at, gain_db: -10.0 + 5.0 * big, pitch: (1.3 - 0.25 * big).clamp(0.9, 1.4) },
            ]
        }
        PowerCue::Music { layer, on } => vec![PowerSound::Overlay { layer: layer.clone(), on: *on }],
    }
}

fn power_audio(mut audio: ResMut<Audio>, mut cues: MessageReader<PowerCue>) {
    let a = &mut *audio;
    for c in cues.read() {
        for s in power_cue_plan(c) {
            match s {
                PowerSound::Sfx { name, at, gain_db, pitch } => {
                    let o = match at {
                        Some(p) => PlayOpts::at(gv(p)),
                        None => PlayOpts::own(),
                    };
                    a.engine.play(name, o.gain_db(gain_db).pitch(pitch));
                }
                PowerSound::Stinger(name) => {
                    #[cfg(feature = "music")]
                    if let Some(m) = a.music.as_mut()
                        && let Some(&k) = bd_music::StingerKind::ALL.iter().find(|k| k.name() == name)
                    {
                        m.stinger(k, &mut a.engine);
                    }
                    #[cfg(not(feature = "music"))]
                    let _ = name;
                }
                PowerSound::Overlay { layer, on } => {
                    #[cfg(feature = "music")]
                    if let Some(m) = a.music.as_mut() {
                        m.overlay(&layer, on, &mut a.engine);
                    }
                    #[cfg(not(feature = "music"))]
                    let _ = (layer, on);
                }
            }
        }
    }
}

/// Match point / last standing stingers.
#[cfg(feature = "music")]
fn music_moments(
    e: &GameEvent,
    w: &ActiveWorld,
    alive: &mut usize,
    mp: &mut bool,
    m: &mut bd_audio::music::MusicDirector,
    engine: &mut AudioEngine,
) {
    match e {
        GameEvent::Ko { killer_kos, .. } => {
            let target = w.world.rules().ko_target;
            if !*mp && target > 1 && *killer_kos + 1 == target {
                *mp = true;
                m.stinger(bd_music::StingerKind::MatchPoint, engine);
            }
        }
        GameEvent::Eliminated { .. } => {
            let left = w.world.players.iter().filter(|p| !p.eliminated).count();
            if left == 2 && *alive != 2 {
                m.stinger(bd_music::StingerKind::LastStanding, engine);
            }
            *alive = left;
        }
        GameEvent::MatchPhase { phase: bd_sim::MatchPhase::Live, .. } => {
            *mp = false;
            *alive = w.world.players.len();
        }
        _ => {}
    }
}

fn timing_begin(mut audio: ResMut<Audio>) {
    audio.timing.0 = Some(std::time::Instant::now());
}

fn timing_end(mut audio: ResMut<Audio>) {
    let t = &mut audio.timing;
    if let Some(t0) = t.0.take() {
        t.1.push(t0.elapsed().as_secs_f32() * 1e6);
    }
    if t.1.len() >= 1024 {
        t.1.sort_by(f32::total_cmp);
        let mean = t.1.iter().sum::<f32>() / t.1.len() as f32;
        info!("audio: main thread {:.0} us mean, p99 {:.0} us, max {:.0} us (1024 frames)", mean, t.1[1013], t.1[1023]);
        t.1.clear();
    }
}

fn apply_settings(mut audio: ResMut<Audio>, settings: Res<Settings>) {
    let key = (settings.master_volume, settings.audio);
    if audio.settings == Some(key) {
        return;
    }
    audio.settings = Some(key);
    let p = settings.audio.sanitized();
    #[cfg(feature = "music")]
    {
        let a = &mut *audio;
        if let Some(m) = a.music.as_mut()
            && m.themes.enabled() != p.franchise_themes
        {
            m.set_themes_enabled(p.franchise_themes, &mut a.engine);
        }
    }
    audio.engine.set_settings(AudioSettings {
        master: settings.master_volume,
        world: p.world,
        ambience: p.ambience,
        feedback: p.feedback,
        music: p.music,
        night_mode: p.night_mode,
        mono: p.mono,
    });
    #[cfg(feature = "music")]
    if let Some(m) = audio.music.as_mut() {
        m.style = match p.music_style {
            MusicStylePref::Classic => bd_audio::music::MusicStyle::Classic,
            MusicStylePref::Remixes => bd_audio::music::MusicStyle::Remixes,
            MusicStylePref::Shuffle => bd_audio::music::MusicStyle::Shuffle,
        };
    }
}

fn leave_game(audio: Option<ResMut<Audio>>) {
    let Some(mut audio) = audio else { return };
    let a = &mut *audio;
    for (_, h) in a.domains.drain(..).chain(a.strikes.drain(..)) {
        a.engine.stop(h, 0.1);
    }
    for (_, _, h, _) in a.reels.drain(..) {
        a.engine.stop(h, 0.05);
    }
    for h in a.own.iter_mut().filter_map(Option::take) {
        a.engine.stop(h, 0.1);
    }
    for (_, (h, _)) in a.loops.drain() {
        a.engine.stop(h, 0.0);
    }
    #[cfg(feature = "music")]
    if let Some(m) = a.music.as_mut() {
        m.overlays_off(&mut a.engine);
        m.themes_off(&mut a.engine);
    }
    a.remote_prev.clear();
    a.ended = false;
}

/// `--record`: render exactly one captured frame of audio per frame after the capture settles
/// (mirrors `present::record`: fixed 1/fps clock, 1.5 s settle), then write the WAV.
fn offline_capture(sink: Option<NonSendMut<Sink>>, time: Res<Time>, state: Res<State<AppState>>, audio: Option<Res<Audio>>) {
    let Some(mut sink) = sink else { return };
    let Sink::Offline(cap) = &mut *sink else { return };
    let _ = audio;
    let sr = cap.mixer.sample_rate() as f64;
    // Keep the mixer running every frame (commands drain; state stays live) in lock-step.
    let secs = time.delta_secs_f64();
    let in_game = *state.get() == AppState::InGame;
    if in_game {
        cap.t_game += 1.0 / cap.fps as f64;
    }
    let exact = secs * sr + cap.carry;
    let n = exact.floor() as usize;
    cap.carry = exact - n as f64;
    let mut buf = vec![0.0f32; n * 2];
    cap.mixer.process(&mut buf, 2);
    let settled = in_game && cap.t_game >= 1.5;
    if settled && cap.frames_left > 0 {
        cap.frames_left -= 1;
        cap.samples.extend(buf.iter().map(|&s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16));
    }
    if settled && cap.frames_left == 0 && !cap.written {
        cap.written = true;
        match write_wav(&cap.path, cap.mixer.sample_rate(), &cap.samples) {
            Ok(()) => println!("RECORD_AUDIO {} ({:.1} s)", cap.path.display(), cap.samples.len() as f32 / 2.0 / sr as f32),
            Err(e) => warn!("record audio: {e}"),
        }
    }
}

fn write_wav(path: &std::path::Path, sr: u32, s: &[i16]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let data = (s.len() * 2) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&(sr * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data.to_le_bytes())?;
    for v in s {
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()
}

/// Franchise themes: which of the local player's power uses wake a theme, and which theme.
#[cfg(feature = "music")]
pub mod themes {
    use super::*;
    use bd_sim::powers::{PowerEvent, PowerTable, SHIPPED_FILES};
    use std::sync::OnceLock;

    /// The power a game event says `local` just used (a move, a form, a gain, a swap). Other
    /// players' uses never touch the local soundtrack.
    pub fn used_power(e: &GameEvent, local: Option<PlayerId>) -> Option<u16> {
        let (player, power) = match e {
            GameEvent::Power(
                PowerEvent::Move { player, power, .. }
                | PowerEvent::Gained { player, power, .. }
                | PowerEvent::Selected { player, power, .. },
            ) => (*player, *power),
            GameEvent::Power(PowerEvent::Ascend { player, power: Some(power), .. }) => (*player, *power),
            _ => return None,
        };
        (Some(player) == local).then_some(power)
    }

    /// Source file of every power in the shipped table (index = power id), e.g.
    /// `base_one_piece.ron` — the franchise table's per-file mapping. Built once by walking the
    /// shipped files in table order (exact: entries load in file order).
    pub fn power_files() -> &'static [&'static str] {
        static F: OnceLock<Vec<&'static str>> = OnceLock::new();
        F.get_or_init(|| {
            let table = PowerTable::get();
            let mut out = Vec::with_capacity(table.powers.len());
            let (mut fi, mut pos) = (0usize, 0usize);
            for p in &table.powers {
                let needle = format!("id: \"{}\"", p.id);
                let mut found = None;
                for (k, (name, text)) in SHIPPED_FILES.iter().enumerate().skip(fi) {
                    let from = if k == fi { pos } else { 0 };
                    if let Some(i) = text.get(from..).and_then(|t| t.find(&needle)) {
                        found = Some((k, from + i + needle.len(), *name));
                        break;
                    }
                }
                match found {
                    Some((k, at, name)) => {
                        (fi, pos) = (k, at);
                        out.push(name);
                    }
                    None => out.push(""),
                }
            }
            out
        })
    }

    /// The theme track (music library index) a power's franchise plays, if any.
    pub fn theme_of(m: &bd_audio::music::MusicDirector, power: u16) -> Option<u16> {
        let def = PowerTable::get().def(power)?;
        let file = power_files().get(usize::from(power)).copied().filter(|f| !f.is_empty());
        m.franchise_theme(&def.id, file, def.music.as_deref())
    }

    /// The local player's own power uses → the music director's theme layer.
    pub(super) fn franchise_themes(
        mut audio: ResMut<Audio>,
        mut events: MessageReader<GameEvents>,
        active: Option<Res<ActiveWorld>>,
        mut cache: Local<HashMap<u16, Option<u16>>>,
    ) {
        let local = active.as_ref().and_then(|w| w.local);
        let a = &mut *audio;
        let Some(m) = a.music.as_mut() else {
            events.clear();
            return;
        };
        if local.is_none() || !m.themes.enabled() {
            events.clear();
            return;
        }
        for ev in events.read() {
            let Some(power) = used_power(&ev.event, local) else { continue };
            let theme = *cache.entry(power).or_insert_with(|| theme_of(m, power));
            if let Some(t) = theme {
                m.theme_used(t, &mut a.engine);
            }
        }
    }
}

#[cfg(test)]
mod power_cue_tests {
    use super::*;

    #[test]
    fn power_cues_map_to_stingers_whoomps_and_the_drum_layer() {
        let pos = Vec3::new(3.0, 1.0, -4.0);
        let own = power_cue_plan(&PowerCue::Awakening { player: PlayerId(1), power: 7, tier: 4, local: true, pos });
        assert_eq!(own, vec![PowerSound::Stinger("streak")], "your awakening: the music answers");
        #[cfg(feature = "music")]
        assert!(bd_music::StingerKind::ALL.iter().any(|k| k.name() == "streak"), "an existing stinger");
        let theirs = power_cue_plan(&PowerCue::Awakening { player: PlayerId(2), power: 7, tier: 4, local: false, pos });
        assert!(matches!(theirs.as_slice(), [PowerSound::Sfx { at: Some(p), .. }] if *p == pos), "someone else's: at the spot");
        // Bursts: a whoomp + a sub hit, own (2D) for the caster, deeper for stronger forms.
        let small = power_cue_plan(&PowerCue::Burst { player: PlayerId(1), pos, intensity: 0.6, local: true });
        let big = power_cue_plan(&PowerCue::Burst { player: PlayerId(1), pos, intensity: 2.8, local: false });
        assert_eq!(small.len(), 2);
        let pitch = |v: &[PowerSound]| match v[0] {
            PowerSound::Sfx { pitch, at, .. } => (pitch, at),
            _ => panic!(),
        };
        assert_eq!(pitch(&small).1, None);
        assert_eq!(pitch(&big).1, Some(pos));
        assert!(pitch(&big).0 < pitch(&small).0);
        let names: Vec<_> = small.iter().filter_map(|s| if let PowerSound::Sfx { name, .. } = s { Some(*name) } else { None }).collect();
        assert_eq!(names, ["ring_blast", "kaiju_stomp"]);
        assert!(power_cue_plan(&PowerCue::Burst { player: PlayerId(1), pos, intensity: f32::NAN, local: true }).len() == 2);
        // Gear Fifth's drums: the overlay layer on / off.
        assert_eq!(power_cue_plan(&PowerCue::Music { layer: "drums".into(), on: true }), vec![PowerSound::Overlay { layer: "drums".into(), on: true }]);
        assert_eq!(power_cue_plan(&PowerCue::Music { layer: "drums".into(), on: false }), vec![PowerSound::Overlay { layer: "drums".into(), on: false }]);
        #[cfg(feature = "music")]
        assert!(bd_music::overlay_id("drums").is_some(), "the layer exists");
    }

    #[test]
    fn every_sound_a_power_cue_plays_is_in_the_cue_table() {
        let table = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/audio/cues.ron")).expect("cues.ron");
        for n in ["legendary_sting", "ring_blast", "kaiju_stomp"] {
            assert!(table.contains(&format!("\"{n}\"")), "{n} missing from cues.ron");
            let f = format!("{}/../../assets/sfx/{n}_1.ogg", env!("CARGO_MANIFEST_DIR"));
            assert!(std::path::Path::new(&f).exists(), "{f}");
        }
    }
}

#[cfg(all(test, feature = "music"))]
mod franchise_theme_tests {
    use super::themes::{power_files, theme_of, used_power};
    use super::*;
    use bd_sim::powers::{PowerEvent, PowerTable};

    fn director() -> bd_audio::music::MusicDirector {
        let lib = std::sync::Arc::new(bd_music::MusicLibrary::load_dir(&bd_music::default_music_dir()).expect("music loads"));
        bd_audio::music::MusicDirector::new(lib, bd_audio::music::MusicStyle::Classic, 1)
    }

    #[test]
    fn only_the_local_players_own_uses_count() {
        let me = Some(PlayerId(1));
        let mv = |p: u8| GameEvent::Power(PowerEvent::Move { player: PlayerId(p), power: 7, slot: 0 });
        assert_eq!(used_power(&mv(1), me), Some(7));
        assert_eq!(used_power(&mv(2), me), None, "someone else's move");
        assert_eq!(used_power(&mv(1), None), None, "no local player (menus, spectating)");
        let asc = |power| GameEvent::Power(PowerEvent::Ascend { player: PlayerId(1), power, tier: 1, transform: false, awakening: true });
        assert_eq!(used_power(&asc(Some(9)), me), Some(9));
        assert_eq!(used_power(&asc(None), me), None, "a plain power-up has no franchise");
        assert_eq!(
            used_power(&GameEvent::Power(PowerEvent::Lost { player: PlayerId(1), power: 7 }), me),
            None,
            "losing it is not using it"
        );
    }

    #[test]
    fn every_power_knows_its_file() {
        let t = PowerTable::get();
        let files = power_files();
        assert_eq!(files.len(), t.powers.len());
        assert!(files.iter().all(|f| !f.is_empty()), "every power found in its file");
        let file = |id: &str| files[usize::from(t.index_of(id).unwrap_or_else(|| panic!("{id}")))];
        assert_eq!(file("gomu_gomu"), "base_one_piece.ron");
        assert_eq!(file("kit_luffy"), "anime_classics.ron");
        assert_eq!(file("star_platinum"), "base_jojo.ron");
        assert_eq!(file("kit_optimus_prime"), "tv_cartoons.ron");
        assert_eq!(files.last().copied(), bd_sim::powers::SHIPPED_FILES.last().map(|f| f.0));
    }

    #[test]
    fn franchise_powers_resolve_to_their_themes() {
        let m = director();
        let t = PowerTable::get();
        let theme = |id: &str| theme_of(&m, t.index_of(id).unwrap_or_else(|| panic!("no power {id}")));
        let lib = bd_music::MusicLibrary::load_dir(&bd_music::default_music_dir()).expect("music");
        let name = |x: Option<u16>| x.and_then(|i| lib.track(bd_music::TrackId(i))).map(|s| s.info.name.clone());
        assert_eq!(name(theme("gomu_gomu")).as_deref(), Some("open_sea_jig"), "per-file (devil fruit)");
        assert_eq!(name(theme("kit_luffy")).as_deref(), Some("open_sea_jig"), "by id");
        assert_eq!(name(theme("kit_leonardo")).as_deref(), Some("sewer_surf_stomp"));
        assert_eq!(name(theme("mutagen_turtle")).as_deref(), Some("sewer_surf_stomp"));
        assert_eq!(name(theme("kit_optimus_prime")).as_deref(), Some("chrome_titans"));
        assert_eq!(name(theme("super_saiyan")).as_deref(), Some("sky_burst_overdrive"));
        assert_eq!(name(theme("kit_naruto")).as_deref(), Some("shadow_step_shinobi"));
        assert_eq!(name(theme("star_platinum")).as_deref(), Some("menace_disco_strut"));
        assert_eq!(name(theme("kryptonian")).as_deref(), Some("skyline_vanguard"));
        assert_eq!(name(theme("kit_jedi")).as_deref(), Some("distant_stars_march"));
        assert_eq!(name(theme("ssb_pikachu")).as_deref(), Some("tall_grass_trek"));
        assert_eq!(name(theme("adv_link")).as_deref(), Some("wildwood_valor"));
        assert_eq!(name(theme("waterbending")).as_deref(), Some("four_winds_accord"));
        assert_eq!(name(theme("myth_zeus")).as_deref(), Some("old_gods_thunder"));
        assert_eq!(theme("kit_ichigo"), None, "no theme for unmapped franchises");
        // Every mapped id in the table exists (typos would silently map nothing).
        for f in &lib.franchises {
            for id in f.ids.iter().filter(|i| !i.contains('*')) {
                assert!(t.index_of(id).is_some(), "franchises.ron: {}: no power '{id}'", f.name);
            }
            for file in &f.files {
                assert!(
                    bd_sim::powers::SHIPPED_FILES.iter().any(|(n, _)| n.trim_end_matches(".ron") == file),
                    "{}: no file {file}",
                    f.name
                );
            }
        }
    }

    #[test]
    fn the_setting_defaults_on_and_survives_old_settings_files() {
        assert!(AudioPrefs::default().franchise_themes);
        let old: AudioPrefs = ron::from_str("(music: 0.5, mono: true)").expect("old settings parse");
        assert!(old.franchise_themes, "missing = on");
        let off = AudioPrefs { franchise_themes: false, ..AudioPrefs::default() };
        let back: AudioPrefs = ron::from_str(&ron::to_string(&off).expect("ser")).expect("de");
        assert!(!back.franchise_themes, "round-trips");
    }
}
