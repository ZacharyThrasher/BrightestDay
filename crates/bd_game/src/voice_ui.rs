//! Voice casting (hold V): bd_voice push-to-talk → `Lexicon::resolve_spoken` → the same cast path as
//! the console ([`crate::console::Caster`]). Voice is a first-class way to play (north star: "excitedly
//! yelling out anything").
//!
//! * The engine starts once the lexicon is loaded (its `bias_prompt` primes Whisper with the ring's
//!   vocabulary, then your corps oath, then — last, where Whisper weighs it most — the other players'
//!   display names, so "drop a meteor on Ethan" is heard as *Ethan*; re-primed without reloading the
//!   model whenever the roster changes) and only if the `voice` setting is on. Built without the `voice` cargo feature (or no
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
    /// Seconds since push-to-talk last went down.
    since_ptt: f32,
    /// How long push-to-talk was held last time (the utterance's speaking time).
    last_hold: f32,
    /// The prompt without player names (lexicon bias + your oath) and the roster it was last sent with.
    prompt_base: String,
    roster: Vec<String>,
    /// STREAMING: the beats of the current press (voice_fluency).
    beats: crate::voice_fluency::Beats,
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
            .add_systems(Update, (start_voice, voice_roster, voice_frame).chain().in_set(UiSet::Input))
            .add_systems(Update, draw_voice.in_set(UiSet::Draw));
    }
}

fn start_voice(prefs: Res<UiPrefs>, lex: Option<Res<RingLexicon>>, oaths: Res<crate::oath::OathTexts>, mut v: ResMut<VoiceUi>) {
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
    // The ring's vocabulary, then the player's own corps oath (so a spoken recital transcribes as the
    // oath: "Sinestro's", "Abin Sur", the Compassion words). Player names are appended last by
    // `voice_roster` (Whisper keeps the prompt's tail under its token cap).
    // VOICE FLUENCY: the most-shouted character and move names go last (Whisper weighs the tail most)
    v.prompt_base = crate::voice_fluency::bias_prompt(&lex.0, &oaths.bias_words(prefs.corps, 24));
    {
        // build the sound-alike index off the game thread (the first spoken line would pay for it)
        let l = lex.0.clone();
        let _ = std::thread::Builder::new().name("voice-fluency-warm".into()).spawn(move || crate::voice_fluency::warm(&l));
    }
    v.roster.clear();
    let cfg = VoiceConfig {
        model_path: crate::assets_dir().join("models").join("ggml-base.en-q5_1.bin"),
        initial_prompt: v.prompt_base.clone(),
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

/// Re-prime Whisper with the other players' display names whenever the roster changes (join / leave /
/// rename): the names go last in the prompt, where Whisper weighs them most. No model reload.
fn voice_roster(active: Option<Res<crate::sim_bridge::ActiveWorld>>, mut v: ResMut<VoiceUi>) {
    let Some(input) = v.input.as_ref() else { return };
    let Some(a) = active.as_ref() else { return };
    let mut roster: Vec<String> = a
        .world
        .players
        .iter()
        .filter(|p| Some(p.id) != a.local)
        .map(|p| voice_name(&p.name))
        .filter(|n| !n.is_empty())
        .take(bd_core::MAX_PLAYERS)
        .collect();
    roster.sort();
    roster.dedup();
    if roster == v.roster {
        return;
    }
    let prompt = if roster.is_empty() { v.prompt_base.clone() } else { format!("{}, {}", v.prompt_base, roster.join(", ")) };
    input.set_prompt(&prompt);
    v.roster = roster;
}

/// A display name as it would be *said* ("xX_Nova_Xx" → "Nova", "DragonSlayer" → "Dragon Slayer",
/// "Alex_99" → "Alex 99"): what Whisper should expect to hear.
fn voice_name(raw: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    for c in raw.chars().take(32) {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev = None;
            continue;
        }
        if let Some(p) = prev
            && ((p.is_lowercase() && c.is_uppercase()) || (p.is_ascii_digit() != c.is_ascii_digit()))
            && !cur.is_empty()
        {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
        prev = Some(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    // gamertag noise ("xX", "Xx", "_") isn't said
    words.retain(|w| !w.chars().all(|c| matches!(c, 'x' | 'X')));
    words.join(" ")
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
    lex: Option<Res<RingLexicon>>,
    mut v: ResMut<VoiceUi>,
    mut caster: Caster,
    mut oath: crate::oath::OathVoice,
) {
    if v.input.is_none() {
        return;
    }
    let dt = time.delta_secs();
    let in_play = *state.get() == AppState::InGame && !menu.0 && !focus.typing;
    // The oath overlay holds push-to-talk itself while reciting by voice (it owns the keyboard).
    let held = (in_play && binds.pressed(Action::Voice, &keys, &mouse)) || oath.ptt();
    let v = &mut *v;
    if v.held && !held {
        // Speaking time of this utterance (a spoken oath is timed by it, not by Whisper's latency).
        v.last_hold = v.since_ptt;
    }
    v.since_ptt = if held && !v.held { 0.0 } else { (v.since_ptt + dt).min(600.0) };
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
                // STREAMING: one beat of a spoken chain ("teleport ... rasengan ... chidori"), heard
                // while the key is still down: a move fires at once, anything else waits for the rest
                VoiceEvent::Partial { text, ms, confidence } => {
                    debug!("voice beat: {text:?} ({ms} ms, confidence {confidence:.2})");
                    // (the oath overlay's recital is one piece: never fired beat by beat)
                    match lex.as_ref().filter(|_| !oath.ptt()) {
                        Some(l) => {
                            if let Some(line) = v.beats.beat(&l.0, &text) {
                                caster.cast_heard(&line, true, 0, Some(confidence));
                            }
                        }
                        None => v.beats.hold(&text),
                    }
                }
                VoiceEvent::Final { text, ms, confidence } => {
                    v.transcribing = false;
                    v.listening = false;
                    debug!("voice: {text:?} ({ms} ms, confidence {confidence:.2})");
                    let said = v.beats.finish(&text);
                    v.beats.reset();
                    // OATH HOOK (before the ring resolver): the overlay's spoken recital, or a section
                    // of your own oath yelled outside it, is an oath recital — not a cast.
                    let spoken = if v.held { v.since_ptt } else { v.last_hold };
                    if oath.claim(&said, spoken) {
                        continue;
                    }
                    // VOICE FLUENCY: the transcript → the line the console reads (voice == typing)
                    let line = match lex.as_ref() {
                        Some(l) => crate::voice_fluency::heard_line(&l.0, &said),
                        None => said,
                    };
                    caster.cast_heard(&line, true, 0, Some(confidence));
                }
                VoiceEvent::Discarded(_) => {
                    v.transcribing = false;
                    v.listening = false;
                    // the press's last stretch was silence: what the beats held back casts now
                    let said = v.beats.finish("");
                    let fired = v.beats.fired;
                    v.beats.reset();
                    let spoken = if v.held { v.since_ptt } else { v.last_hold };
                    if said.is_empty() {
                        if fired == 0 {
                            oath.failed();
                        }
                    } else if !oath.claim(&said, spoken) {
                        let line = match lex.as_ref() {
                            Some(l) => crate::voice_fluency::heard_line(&l.0, &said),
                            None => said,
                        };
                        caster.cast_heard(&line, true, 0, None);
                    }
                }
                VoiceEvent::Error(e) => {
                    warn!("voice: {e}");
                    v.transcribing = false;
                    v.listening = false;
                    v.beats.reset();
                    oath.failed();
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

#[cfg(test)]
mod tests {
    use super::voice_name;

    #[test]
    fn names_as_said() {
        assert_eq!(voice_name("xX_Nova_Xx"), "Nova");
        assert_eq!(voice_name("DragonSlayer"), "Dragon Slayer");
        assert_eq!(voice_name("Alex_99"), "Alex 99");
        assert_eq!(voice_name("Big Tony"), "Big Tony");
        assert_eq!(voice_name("Ethan"), "Ethan");
    }
}
