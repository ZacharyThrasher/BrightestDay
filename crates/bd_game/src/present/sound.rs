//! World-sound call-site API (unchanged for the presenters), now voiced by bd_audio's mixer
//! through [`crate::audio`]:
//!
//! * [`Sfx3d`] message: `name` (manifest key), optional world `pos` (panned, distance model, air
//!   absorption, occlusion through the voxels, shared reverb; `None` = 2D feedback/UI), gain
//!   (linear multiplier → dB), pitch multiplier on top of the cue's micro-variation, optional
//!   sequence `step` (KO streak). `exempt()` is kept for source compatibility: critical cues are
//!   routed to the Feedback bus by `assets/audio/cues.ron`, which the mix never buries.
//! * [`SfxLoopEmitter`] component on any entity with a `GlobalTransform`: a looping voice follows
//!   it; change `gain`/`pitch` every frame; removing the component or despawning the entity stops
//!   it abruptly (construct loops cut off on a KO).
//! * [`MixDuck`]: `duck(secs)` requests the KO "negative space" duck (`ko_confirm` in
//!   `assets/audio/mix.ron`, held for `secs`).
//!
//! Also here: the `view_toggle` UI cue whenever the first / third person view flips in play (the
//! camera rig writes `CameraPrefs::third` on the toggle key; settings changes while paused are
//! silent).

use bevy::prelude::*;
use std::borrow::Cow;

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

/// KO negative-space duck request (consumed by [`crate::audio`] each frame).
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct MixDuck {
    /// Longest hold requested since the audio system last looked (seconds; 0 = none).
    pub pending: f32,
}

impl MixDuck {
    pub fn duck(&mut self, secs: f32) {
        if secs.is_finite() {
            self.pending = self.pending.max(secs.clamp(0.0, 3.0));
        }
    }
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MixDuck>().add_message::<Sfx3d>().add_systems(Update, view_toggle_cue);
    }
}

/// `view_toggle` when the first / third person view flips during play.
fn view_toggle_cue(
    prefs: Option<Res<crate::lobby::prefs::UiPrefs>>,
    state: Res<State<crate::state::AppState>>,
    menu: Res<crate::state::InGameMenu>,
    mut last: Local<Option<bool>>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(p) = prefs else { return };
    if !p.is_changed() && last.is_some() {
        return;
    }
    let third = p.camera.third.unwrap_or(p.third_person);
    let playing = *state.get() == crate::state::AppState::InGame && !menu.0;
    if last.replace(third).is_some_and(|was| was != third) && playing {
        sfx.write(Sfx3d::ui("view_toggle").gain(0.8).pitch(if third { 0.94 } else { 1.06 }));
    }
}
