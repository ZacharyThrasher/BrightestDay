//! Voice casting (hold V): bd_voice push-to-talk → `Lexicon::resolve_spoken` → the same cast path as
//! the console ([`crate::console::Caster`]). Voice is a first-class way to play (north star: "excitedly
//! yelling out anything").
//!
//! * The engine starts once the lexicon is loaded (its `bias_prompt` primes Whisper with the ring's
//!   vocabulary) and only if the `voice` setting is on. Built without the `voice` cargo feature (or no
//!   mic / no model) everything stays hidden — no errors on screen.
//! * While listening, a small ring pulse at the bottom follows the mic level, and the viewmodel's
//!   ring breathes with it ([`VoiceUi::level`]). Transcribing: the pulse holds and spins.

use crate::console::{corps_color, Caster, RingLexicon};
use crate::input::UiFocus;
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::lobby::UiSet;
use crate::state::{AppState, InGameMenu};
use bd_voice::{VoiceConfig, VoiceEvent, VoiceInput};
use bevy::picking::Pickable;
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VoiceStatus {
    /// Not started (setting off, lexicon not loaded yet).
    #[default]
    Idle,
    /// Engine starting / model loading.
    Loading,
    Ready,
    /// Not compiled in, no microphone, no model… (hidden).
    Unavailable,
}

#[derive(Resource, Default)]
pub struct VoiceUi {
    input: Option<VoiceInput>,
    pub status: VoiceStatus,
    pub listening: bool,
    pub transcribing: bool,
    /// Smoothed mic level 0..1 (0 when not listening).
    pub level: f32,
    raw_level: f32,
    held: bool,
    spin: f32,
}

impl VoiceUi {
    #[allow(dead_code)]
    /// Voice can be used right now (hide every voice affordance otherwise).
    pub fn available(&self) -> bool {
        self.status == VoiceStatus::Ready
    }
}

#[derive(Component)]
struct VoiceRing;

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoiceUi>()
            .add_systems(Startup, spawn_voice_ui)
            .add_systems(Update, (start_voice, voice_frame).chain().in_set(UiSet::Input))
            .add_systems(Update, draw_voice.in_set(UiSet::Draw));
    }
}

fn start_voice(prefs: Res<UiPrefs>, lex: Option<Res<RingLexicon>>, mut v: ResMut<VoiceUi>) {
    if !prefs.voice {
        if v.input.is_some() {
            v.input = None;
            v.status = VoiceStatus::Idle;
        }
        return;
    }
    if v.status != VoiceStatus::Idle {
        return;
    }
    if !bd_voice::ENABLED {
        v.status = VoiceStatus::Unavailable;
        return;
    }
    let Some(lex) = lex else { return };
    let cfg = VoiceConfig {
        model_path: crate::assets_dir().join("models").join("ggml-base.en-q5_1.bin"),
        initial_prompt: lex.0.bias_prompt(60),
        ..Default::default()
    };
    match VoiceInput::new(cfg) {
        Ok(input) => {
            v.input = Some(input);
            v.status = VoiceStatus::Loading;
        }
        Err(e) => {
            info!("voice casting unavailable: {e}");
            v.status = VoiceStatus::Unavailable;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn voice_frame(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    focus: Res<UiFocus>,
    time: Res<Time<Real>>,
    mut v: ResMut<VoiceUi>,
    mut caster: Caster,
) {
    if v.input.is_none() {
        return;
    }
    let dt = time.delta_secs();
    let in_play = *state.get() == AppState::InGame && !menu.0 && !focus.typing;
    let held = in_play && binds.pressed(Action::Voice, &keys, &mouse);
    let v = &mut *v;
    if let Some(input) = v.input.as_ref() {
        if held != v.held {
            input.set_push_to_talk(held);
        }
        v.held = held;
        while let Some(ev) = input.poll() {
            match ev {
                VoiceEvent::Ready => v.status = VoiceStatus::Ready,
                VoiceEvent::Listening => {
                    v.listening = true;
                    v.transcribing = false;
                }
                VoiceEvent::Level(l) => v.raw_level = if l.is_finite() { l.clamp(0.0, 1.0) } else { 0.0 },
                VoiceEvent::Transcribing => {
                    v.listening = false;
                    v.transcribing = true;
                }
                VoiceEvent::Final { text, ms } => {
                    v.transcribing = false;
                    v.listening = false;
                    debug!("voice: {text:?} ({ms} ms)");
                    caster.cast(&text, true, 0);
                }
                VoiceEvent::Discarded(_) => {
                    v.transcribing = false;
                    v.listening = false;
                }
                VoiceEvent::Error(e) => {
                    warn!("voice: {e}");
                    v.transcribing = false;
                    v.listening = false;
                    if v.status != VoiceStatus::Ready {
                        v.status = VoiceStatus::Unavailable;
                        v.input = None;
                        return;
                    }
                }
            }
        }
    }
    if !v.held && !v.transcribing {
        v.listening = false;
        v.raw_level = 0.0;
    }
    // Fast attack, slow release: syllables pop, silence settles.
    let k = if v.raw_level > v.level { 1.0 - (-30.0 * dt).exp() } else { 1.0 - (-8.0 * dt).exp() };
    v.level += (v.raw_level - v.level) * k;
    if v.transcribing {
        v.spin += dt;
    }
}

fn spawn_voice_ui(mut commands: Commands) {
    commands.spawn((
        Name::new("VoiceRing"),
        VoiceRing,
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            bottom: percent(9),
            width: px(34),
            height: px(34),
            margin: UiRect::left(px(-17)),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BorderColor::all(Color::WHITE),
        BackgroundColor(Color::NONE),
        UiTransform::default(),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(50),
    ));
}

fn draw_voice(
    v: Res<VoiceUi>,
    prefs: Res<UiPrefs>,
    active: Option<Res<crate::sim_bridge::ActiveWorld>>,
    mut q: Query<(&mut Visibility, &mut UiTransform, &mut BorderColor, &mut BackgroundColor), With<VoiceRing>>,
) {
    let Ok((mut vis, mut tf, mut border, mut bg)) = q.single_mut() else { return };
    let show = v.listening || v.transcribing;
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if !show {
        return;
    }
    let corps = active.as_ref().and_then(|a| a.local_player().map(|p| p.corps)).unwrap_or(prefs.corps);
    let c = corps_color(corps);
    let s = 1.0 + v.level * 0.9;
    tf.scale = Vec2::splat(s);
    tf.rotation = Rot2::radians(v.spin * 6.0);
    *border = BorderColor::all(c.with_alpha(0.55 + 0.45 * v.level));
    bg.0 = c.with_alpha(0.08 + 0.25 * v.level);
    if v.transcribing {
        // Spinning broken ring while Whisper runs.
        *border = BorderColor { top: c, right: c.with_alpha(0.2), bottom: c.with_alpha(0.6), left: c.with_alpha(0.1) };
    }
}
