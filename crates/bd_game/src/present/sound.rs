//! World sound: positional one-shots + per-entity loops from `assets/sfx/manifest.ron`.
//!
//! * [`Sfx3d`] message: `name` (manifest key), optional world `pos` (panned + distance-attenuated;
//!   `None` = 2D), gain, pitch multiplier on top of the manifest's random pitch range, optional
//!   sequence `step` (KO streak: plays `files[step-1]` exactly instead of a random variation).
//! * [`SfxLoopEmitter`] component on any entity with a `GlobalTransform`: a looping sound follows it
//!   (engines, beams, minigun spin…); change `gain`/`pitch` every frame; removing the component or
//!   despawning the entity stops it (construct loops cut off abruptly on a KO).
//! * [`MixDuck`]: `duck(secs)` ducks every world sound (one-shots at spawn, loops live) so a KO
//!   streak note rings in negative space. `duck_exempt` sounds ignore it.
//!
//! The shell's `audio` module keeps voicing the local player's movement; this one only loads the
//! same files by path (Bevy dedups handles, so there is no second copy in memory).

use crate::assets_dir;
use crate::camera::PlayerCamera;
use crate::settings::Settings;
use crate::state::AppState;
use crate::util::Rng;
use bevy::audio::{AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, SpatialListener, Volume};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Deserialize, Debug, Clone)]
struct Entry {
    files: Vec<String>,
    #[serde(default = "one")]
    volume: f32,
    #[serde(default = "unit_pitch")]
    pitch: (f32, f32),
    #[serde(default)]
    sequence: bool,
}

fn one() -> f32 {
    1.0
}
fn unit_pitch() -> (f32, f32) {
    (1.0, 1.0)
}

#[derive(Deserialize, Debug)]
struct Manifest {
    sounds: std::collections::HashMap<String, Entry>,
}

struct Sound {
    handles: Vec<Handle<AudioSource>>,
    volume: f32,
    pitch: (f32, f32),
    sequence: bool,
}

#[derive(Resource, Default)]
pub struct WorldSfxBank {
    sounds: HashMap<String, Sound>,
}

impl WorldSfxBank {
    pub fn has(&self, name: &str) -> bool {
        self.sounds.contains_key(name)
    }
}

/// A one-shot in the world.
#[derive(Message, Clone, Debug)]
pub struct Sfx3d {
    pub name: Cow<'static, str>,
    pub pos: Option<Vec3>,
    pub gain: f32,
    pub pitch: f32,
    /// 1-based step of a `sequence` sound.
    pub step: Option<u8>,
    pub duck_exempt: bool,
}

impl Sfx3d {
    pub fn at(name: impl Into<Cow<'static, str>>, pos: Vec3) -> Self {
        Self { name: name.into(), pos: Some(pos), gain: 1.0, pitch: 1.0, step: None, duck_exempt: false }
    }
    pub fn ui(name: impl Into<Cow<'static, str>>) -> Self {
        Self { name: name.into(), pos: None, gain: 1.0, pitch: 1.0, step: None, duck_exempt: false }
    }
    pub fn gain(mut self, g: f32) -> Self {
        self.gain = g;
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
    pub fn exempt(mut self) -> Self {
        self.duck_exempt = true;
        self
    }
}

/// A looping sound attached to an entity.
#[derive(Component, Clone, Debug)]
pub struct SfxLoopEmitter {
    pub name: &'static str,
    pub gain: f32,
    pub pitch: f32,
}

impl SfxLoopEmitter {
    pub fn new(name: &'static str) -> Self {
        Self { name, gain: 1.0, pitch: 1.0 }
    }
}

/// The audio child playing an emitter's loop.
#[derive(Component)]
struct LoopVoice {
    owner: Entity,
    name: &'static str,
    base: f32,
    current: f32,
}

/// Mix duck (seconds remaining, depth).
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct MixDuck {
    pub remaining: f32,
}

impl MixDuck {
    pub fn duck(&mut self, secs: f32) {
        self.remaining = self.remaining.max(secs);
    }
    /// Current gain multiplier for ducked sounds.
    pub fn factor(&self) -> f32 {
        if self.remaining > 0.0 { 0.28 } else { 1.0 }
    }
}

#[derive(Resource)]
struct SfxRng(Rng);

/// Distance (m) at which a world sound is at half volume.
const HALF_DIST: f32 = 22.0;
/// Beyond this a one-shot is skipped.
const MAX_DIST: f32 = 160.0;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldSfxBank>()
            .init_resource::<MixDuck>()
            .insert_resource(SfxRng(Rng::new(0x5f3d)))
            .add_message::<Sfx3d>()
            .add_systems(Startup, (load_bank, add_listener))
            .add_systems(Update, (play_oneshots, sync_loops).chain().after(crate::tick::FrameSet::Present))
            .add_systems(OnExit(AppState::InGame), stop_loops);
    }
}

fn load_bank(asset_server: Res<AssetServer>, mut bank: ResMut<WorldSfxBank>) {
    let path = assets_dir().join("sfx").join("manifest.ron");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let m: Manifest = match ron::from_str(&text) {
        Ok(m) => m,
        Err(e) => {
            warn!("world sfx: manifest parse error: {e}");
            return;
        }
    };
    for (name, e) in m.sounds {
        let handles = e.files.iter().map(|f| asset_server.load::<AudioSource>(f.clone())).collect();
        bank.sounds.insert(name, Sound { handles, volume: e.volume, pitch: e.pitch, sequence: e.sequence });
    }
}

fn add_listener(mut commands: Commands, cams: Query<Entity, With<PlayerCamera>>) {
    for e in &cams {
        commands.entity(e).insert(SpatialListener::new(0.3));
    }
}

fn play_oneshots(
    mut commands: Commands,
    time: Res<Time<Real>>,
    bank: Res<WorldSfxBank>,
    settings: Res<Settings>,
    mut duck: ResMut<MixDuck>,
    mut rng: ResMut<SfxRng>,
    mut events: MessageReader<Sfx3d>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut listener_missing: Query<Entity, (With<PlayerCamera>, Without<SpatialListener>)>,
) {
    duck.remaining = (duck.remaining - time.delta_secs()).max(0.0);
    for e in &mut listener_missing {
        commands.entity(e).insert(SpatialListener::new(0.3));
    }
    let ear = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    let master = settings.master_volume;
    let mut n = 0;
    for e in events.read() {
        let Some(s) = bank.sounds.get(e.name.as_ref()) else { continue };
        if s.handles.is_empty() || master <= 0.0 {
            continue;
        }
        // Never flood the mixer (a brawl can emit dozens per frame).
        n += 1;
        if n > 24 {
            break;
        }
        let r = &mut rng.0;
        let h = match (e.step, s.sequence) {
            (Some(k), _) => s.handles[(k.max(1) as usize - 1).min(s.handles.len() - 1)].clone(),
            _ => s.handles[(r.next_u32() as usize) % s.handles.len()].clone(),
        };
        let speed = (if s.sequence { 1.0 } else { r.range(s.pitch.0, s.pitch.1) }) * e.pitch;
        let mut gain = s.volume * e.gain;
        if !e.duck_exempt {
            gain *= duck.factor();
        }
        let mut settings = PlaybackSettings::DESPAWN.with_speed(speed.max(0.05));
        let mut tf = Transform::default();
        if let Some(p) = e.pos {
            let d = p.distance(ear);
            if d > MAX_DIST {
                continue;
            }
            gain *= 1.0 / (1.0 + d / HALF_DIST);
            settings = settings.with_spatial(true);
            tf.translation = p;
        }
        if gain < 0.01 {
            continue;
        }
        commands.spawn((AudioPlayer::new(h), settings.with_volume(Volume::Linear(gain)), tf));
    }
}

fn sync_loops(
    mut commands: Commands,
    time: Res<Time<Real>>,
    bank: Res<WorldSfxBank>,
    settings: Res<Settings>,
    duck: Res<MixDuck>,
    emitters: Query<(Entity, &SfxLoopEmitter, &GlobalTransform)>,
    added: Query<(Entity, &SfxLoopEmitter), Added<SfxLoopEmitter>>,
    mut voices: Query<(Entity, &mut LoopVoice, &mut Transform, Option<&mut AudioSink>)>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    let ear = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    for (owner, em) in &added {
        let Some(s) = bank.sounds.get(em.name) else { continue };
        let Some(h) = s.handles.first() else { continue };
        commands.spawn((
            LoopVoice { owner, name: em.name, base: s.volume, current: 0.0 },
            AudioPlayer::new(h.clone()),
            PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)).with_spatial(true),
            Transform::default(),
        ));
    }
    let dt = time.delta_secs();
    for (e, mut v, mut tf, sink) in &mut voices {
        let Ok((_, em, gt)) = emitters.get(v.owner) else {
            // Owner gone: cut off abruptly.
            commands.entity(e).despawn();
            continue;
        };
        if em.name != v.name {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation = gt.translation();
        let d = gt.translation().distance(ear);
        let target = em.gain.max(0.0) * v.base / (1.0 + d / HALF_DIST) * duck.factor();
        let rate = if target > v.current { 20.0 } else { 6.0 };
        v.current += (target - v.current) * (1.0 - (-rate * dt).exp());
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(v.current * settings.master_volume));
            sink.set_speed(em.pitch.clamp(0.25, 4.0));
        }
    }
}

fn stop_loops(mut commands: Commands, voices: Query<Entity, With<LoopVoice>>) {
    for e in &voices {
        commands.entity(e).despawn();
    }
}
