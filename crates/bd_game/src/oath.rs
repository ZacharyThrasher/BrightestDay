//! Oath recital (F): one **section** of the corps oath from `assets/oaths.ron` as a typing test —
//! or spoken (COMBAT.md §2).
//!
//! * Sections are the oath's authored lines (`bd_sim::willpower::oath_sections`, shared with the
//!   sim so lengths always agree). Each recital takes the next section in rotation for that corps
//!   (never the same one twice in a row; remembered for the session): `Cmd::OathBegin { section }`.
//!   The host looks the length up in its own table — the client never sends one.
//! * Opens only in play with the console closed; sets the TYPING bit (movement keys are text; aim /
//!   mouse tethers / momentum stay live).
//! * Letters light up in the corps colour as they are typed; a wrong key flashes the expected letter
//!   and shakes the line (the letter must still be typed). Case and punctuation are forgiving:
//!   punctuation is skipped automatically, letters match case-insensitively.
//! * Progress goes to the sim every few letters (`OathProgress`, improves the partial refill if a hit
//!   interrupts). On the last letter: `OathResult { accuracy, seconds }` and a PERFECT / GOOD / SLOW
//!   performance word. Esc cancels (no refill). `OathInterrupted` from the sim closes it.
//! * **Voice** (when voice is available): before the first letter, hold the voice key ≥
//!   [`VOICE_HOLD_SECS`] to speak the section instead (a quick tap still types its letter). The
//!   transcript is scored by [`voice_match`] (accuracy) and timed by the SPEAKING time (voice key
//!   down → release, never Whisper's latency): the same refill formula, speed bonus and grade as
//!   typing — same skill ceiling (owner). And a normal push-to-talk utterance that recites a section of your own oath
//!   ([`voice_match::YELL_MATCH`]) is an oath recital, not a cast ([`OathVoice::claim`], checked by
//!   voice_ui before the ring resolver): it goes through the same `OathBegin` → `OathResult`.
//!   Either way the result waits until `oath_min_secs(len)` has passed since `OathBegin` (the
//!   host's plausibility floor), vulnerable and unable to cast like any recital.

pub mod voice_match;

use crate::audio::PlaySfx;
use crate::console::corps_color;
use crate::input::UiFocus;
use crate::lobby::UiSet;
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::sim_bridge::{ActiveWorld, Cmd, GameEvents, SimCommand};
use crate::state::{AppState, InGameMenu};
use crate::voice_ui::VoiceUi;
use bd_core::corps::Corps;
use bd_sim::GameEvent;
use bd_sim::willpower::{MAX_OATH_SECTIONS, oath_min_secs, oath_sections, parse_oath_ron};
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::picking::Pickable;
use bevy::prelude::*;
use std::collections::HashMap;

/// Hold the voice key this long (before the first typed letter) to recite by voice.
pub const VOICE_HOLD_SECS: f32 = 0.2;
/// Give up waiting for a transcript this long after the voice key was released.
const VOICE_TIMEOUT_SECS: f32 = 12.0;

/// Characters per second that earn the full refill (mirrors `bd_sim::willpower`).
fn target_secs(len: usize) -> f32 {
    bd_sim::willpower::oath_target_secs(len.min(u16::MAX as usize) as u16)
}

fn len16(len: usize) -> u16 {
    len.min(u16::MAX as usize) as u16
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grade {
    Perfect,
    Good,
    Slow,
}

impl Grade {
    pub fn of(accuracy: f32, seconds: f32, len: usize) -> Grade {
        let pace = target_secs(len) / seconds.max(0.01);
        if accuracy >= 0.97 && pace >= 1.0 {
            Grade::Perfect
        } else if accuracy >= 0.85 && pace >= 0.7 {
            Grade::Good
        } else {
            Grade::Slow
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Grade::Perfect => "perfect",
            Grade::Good => "good",
            Grade::Slow => "slow",
        }
    }
    pub fn word(self) -> &'static str {
        match self {
            Grade::Perfect => "PERFECT",
            Grade::Good => "GOOD",
            Grade::Slow => "SLOW",
        }
    }
}

/// Oath sections by corps name (assets/oaths.ron → `bd_sim::willpower::oath_sections`).
#[derive(Resource)]
pub struct OathTexts {
    by_corps: HashMap<String, Vec<String>>,
    fallback: Vec<String>,
}

impl Default for OathTexts {
    fn default() -> Self {
        Self { by_corps: HashMap::new(), fallback: vec!["In brightest day, in blackest night, no evil shall escape my sight.".into()] }
    }
}

impl OathTexts {
    /// String keys (not `Corps`) so an oath for a corps the game doesn't have yet (e.g. Death)
    /// doesn't make the whole file fail to parse.
    pub fn from_ron(src: &str) -> Result<Self, String> {
        let map = parse_oath_ron(src)?;
        let by_corps = map.into_iter().map(|(k, v)| (k, oath_sections(&v))).filter(|(_, s)| !s.is_empty()).collect();
        Ok(Self { by_corps, ..default() })
    }
    fn load() -> Self {
        let path = crate::assets_dir().join("oaths.ron");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        Self::from_ron(&text).unwrap_or_else(|e| {
            warn!("oaths.ron: {e}");
            Self::default()
        })
    }
    /// Up to `n` distinctive words of `c`'s oath (common short words dropped, first-seen order,
    /// comma-separated) for Whisper's bias prompt, so a spoken recital transcribes as the oath
    /// ("Sinestro's", "Abin Sur", the Compassion words).
    pub fn bias_words(&self, c: Corps, n: usize) -> String {
        const COMMON: &[&str] = &[
            "the", "and", "all", "our", "your", "with", "who", "for", "from", "into", "that", "this", "those", "what", "when", "within",
            "shall", "let", "made", "full", "long", "seems", "take", "want", "join", "try", "stop", "look", "lay",
        ];
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<&str> = Vec::new();
        for s in self.sections(c) {
            for w in s.split(|ch: char| !(ch.is_alphanumeric() || ch == '\'')) {
                let w = w.trim_matches('\'');
                let lw = w.to_lowercase();
                if w.chars().count() < 3 || COMMON.contains(&lw.as_str()) || !seen.insert(lw) {
                    continue;
                }
                if out.len() >= n {
                    return out.join(", ");
                }
                out.push(w);
            }
        }
        out.join(", ")
    }
    /// The recital sections of `c`'s oath (never empty).
    pub fn sections(&self, c: Corps) -> &[String] {
        self.by_corps.get(c.name()).map(|v| v.as_slice()).unwrap_or(&self.fallback)
    }
}

/// The section after `last` in rotation (section 0 first). Never repeats while there are ≥ 2.
pub fn next_section(last: Option<u8>, count: usize) -> u8 {
    let n = count.clamp(1, MAX_OATH_SECTIONS);
    match last {
        None => 0,
        Some(l) => ((l as usize + 1) % n) as u8,
    }
}

/// Which section each corps recited last (session memory for the rotation).
#[derive(Resource, Default)]
pub struct OathRotation {
    last: [Option<u8>; 8],
}

impl OathRotation {
    /// Pick (and remember) the next section of `corps`'s oath.
    pub fn next(&mut self, corps: Corps, count: usize) -> u8 {
        let s = next_section(self.last[corps.index() as usize], count);
        self.last[corps.index() as usize] = Some(s);
        s
    }
    /// A section was recited some other way (yelled): the rotation continues after it.
    pub fn note(&mut self, corps: Corps, section: u8) {
        self.last[corps.index() as usize] = Some(section);
    }
}

/// Spoken-recital phase of the open overlay.
#[derive(Clone, Debug, Default, PartialEq)]
enum VoicePhase {
    /// Typing (or voice not in use).
    #[default]
    Off,
    /// Voice key down before the first letter: becomes `Listening` at [`VOICE_HOLD_SECS`], or types
    /// its (held-back) letter on a quick release.
    Arming { held: f32, letter: Option<String> },
    /// Speaking (key held) then waiting for Whisper (`released` = `t` at release). `t` = seconds
    /// since the key went down.
    Listening { t: f32, released: Option<f32> },
    /// Transcript scored: waiting out the host's plausibility floor, then `OathResult`.
    Sealing { accuracy: f32, seconds: f32 },
}

/// A yelled oath (outside the overlay) waiting to be sent by [`oath_keys`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct Yell {
    corps: Corps,
    section: u8,
    accuracy: f32,
    seconds: f32,
}

/// Recital state (read by the console / viewmodel).
#[derive(Resource, Default)]
pub struct OathState {
    pub open: bool,
    text: Vec<char>,
    /// Index of the next char to type.
    pos: usize,
    correct: u32,
    errors: u32,
    elapsed: f32,
    started: bool,
    reported: usize,
    flash: f32,
    shake: f32,
    corps: Option<Corps>,
    dirty: bool,
    /// Grade word shown after finishing (seconds left).
    grade: Option<(Grade, f32)>,
    /// Section being recited.
    section: u8,
    /// Seconds since `OathBegin` was sent (mirrors the host's plausibility clock).
    since_begin: f32,
    /// The overlay owns the keyboard (typing recital). A yelled oath shows the overlay without it.
    typing_focus: bool,
    voice: VoicePhase,
    /// Push-to-talk wanted by the overlay (read by voice_ui).
    ptt: bool,
    yell: Option<Yell>,
    /// Drop the next transcript (its recital was cancelled while Whisper ran); seconds left.
    ignore_voice: f32,
    /// Small line under the bar ("hold V to speak", "LISTENING…").
    hint: String,
    /// "Didn't catch that" (seconds left).
    missed: f32,
}

impl OathState {
    fn skip_free(&mut self) {
        while self.pos < self.text.len() && !self.text[self.pos].is_alphanumeric() && self.text[self.pos] != ' ' {
            self.pos += 1;
        }
    }
    pub fn accuracy(&self) -> f32 {
        let n = self.correct + self.errors;
        if n == 0 { 1.0 } else { self.correct as f32 / n as f32 }
    }
    pub fn progress(&self) -> f32 {
        if self.text.is_empty() { 0.0 } else { self.pos as f32 / self.text.len() as f32 }
    }
    /// Reset for a new recital of `text` (section `section` of `corps`'s oath).
    fn start(&mut self, corps: Corps, section: u8, text: &str, typing: bool) {
        self.open = true;
        self.text = text.chars().collect();
        self.pos = 0;
        self.correct = 0;
        self.errors = 0;
        self.elapsed = 0.0;
        self.started = false;
        self.reported = 0;
        self.corps = Some(corps);
        self.grade = None;
        self.section = section;
        self.since_begin = 0.0;
        self.typing_focus = typing;
        self.voice = VoicePhase::Off;
        self.ptt = false;
        self.missed = 0.0;
        self.dirty = true;
        self.skip_free();
    }
    /// One typed character. Returns whether the section is complete.
    fn feed(&mut self, c: char, sfx: &mut MessageWriter<PlaySfx>) -> bool {
        let Some(&want) = self.text.get(self.pos) else { return true };
        self.started = true;
        if c.to_lowercase().eq(want.to_lowercase()) {
            self.pos += 1;
            self.correct += 1;
            self.skip_free();
            self.dirty = true;
            let pitch = 0.9 + 0.35 * self.progress();
            sfx.write(PlaySfx::new("type_key").gain(0.35).pitch(pitch));
        } else if c == ' ' && want != ' ' {
            // Stray space: ignore silently (forgiving).
        } else {
            self.errors += 1;
            self.flash = 1.0;
            self.shake = 1.0;
            self.dirty = true;
            sfx.write(PlaySfx::new("type_error").gain(0.5));
        }
        self.pos >= self.text.len()
    }
    fn listening(&self) -> bool {
        matches!(self.voice, VoicePhase::Listening { .. })
    }
}

/// Route a finished voice transcript (pure; see [`OathVoice::claim`]). `me` = the local player's
/// corps if alive. Returns `true` when the transcript was an oath recital (don't cast it).
fn route_transcript(o: &mut OathState, texts: &OathTexts, me: Option<Corps>, text: &str, measured: f32) -> bool {
    if o.ignore_voice > 0.0 {
        o.ignore_voice = 0.0;
        return true;
    }
    if o.open {
        let VoicePhase::Listening { t, released } = o.voice else { return false };
        let section: String = o.text.iter().collect();
        let accuracy = voice_match::similarity(text, &section);
        // Speaking time (key down → release; Whisper's latency is not the player's): same formula,
        // same speed bonus and grade as typing.
        o.voice = VoicePhase::Sealing { accuracy, seconds: released.unwrap_or(t).max(0.05) };
        o.ptt = false;
        o.dirty = true;
        return true;
    }
    // Outside the overlay: yell your oath.
    let Some(corps) = me else { return false };
    let sections = texts.sections(corps);
    let Some((section, accuracy)) = voice_match::yelled_section(text, sections) else { return false };
    let seconds = measured.max(0.05);
    o.yell = Some(Yell { corps, section: section as u8, accuracy, seconds });
    true
}

/// voice_ui's hook into the oath: push-to-talk for the overlay, and the transcript check that runs
/// **before** the ring resolver.
#[derive(SystemParam)]
pub struct OathVoice<'w> {
    state: ResMut<'w, OathState>,
    texts: Res<'w, OathTexts>,
    active: Option<Res<'w, ActiveWorld>>,
}

impl OathVoice<'_> {
    /// The oath overlay is listening: hold push-to-talk.
    pub fn ptt(&self) -> bool {
        self.state.ptt
    }
    /// A finished transcript (`measured` = speaking time: push-to-talk down → release). `true` = it was
    /// an oath recital (the overlay's spoken recital, or a section of your own oath yelled outside
    /// it) — don't cast it.
    pub fn claim(&mut self, text: &str, measured: f32) -> bool {
        let me = self.active.as_ref().and_then(|a| a.local_player()).filter(|p| p.alive).map(|p| p.corps);
        route_transcript(&mut self.state, &self.texts, me, text, measured)
    }
    /// Whisper produced nothing (discarded / error): the overlay goes back to typing.
    pub fn failed(&mut self) {
        let o = &mut *self.state;
        o.ignore_voice = 0.0;
        if o.open && o.listening() {
            o.voice = VoicePhase::Off;
            o.ptt = false;
            o.missed = 1.6;
            o.dirty = true;
        }
    }
}

#[derive(Component)]
struct OathRoot;
#[derive(Component)]
struct OathDone;
#[derive(Component)]
struct OathNext;
#[derive(Component)]
struct OathRest;
#[derive(Component)]
struct OathGrade;
#[derive(Component)]
struct OathBar;
#[derive(Component)]
struct OathHint;

pub struct OathPlugin;

impl Plugin for OathPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(OathTexts::load())
            .init_resource::<OathState>()
            .init_resource::<OathRotation>()
            .add_systems(Startup, spawn_oath_ui)
            .add_systems(Update, (oath_keys, oath_events).chain().in_set(UiSet::Modal))
            .add_systems(Update, draw_oath.in_set(UiSet::Draw))
            .add_systems(OnExit(AppState::InGame), |mut o: ResMut<OathState>, mut f: ResMut<UiFocus>| {
                o.yell = None;
                if o.open {
                    close(&mut o, &mut f);
                }
            });
    }
}

#[allow(clippy::too_many_arguments)]
fn oath_keys(
    mut typed: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    (state, menu, time): (Res<State<AppState>>, Res<InGameMenu>, Res<Time<Real>>),
    (texts, mut rotation, voice): (Res<OathTexts>, ResMut<OathRotation>, Res<VoiceUi>),
    prefs: Res<UiPrefs>,
    console: Res<crate::console::RingConsole>,
    active: Option<Res<ActiveWorld>>,
    mut focus: ResMut<UiFocus>,
    mut oath: ResMut<OathState>,
    mut cmds: MessageWriter<SimCommand>,
    mut sfx: MessageWriter<PlaySfx>,
    mut words: Option<ResMut<Messages<bd_fx::SpawnOnomatopoeia>>>,
) {
    let dt = time.delta_secs();
    if let Some((_, t)) = oath.grade.as_mut() {
        *t -= dt;
        if *t <= 0.0 {
            oath.grade = None;
            oath.dirty = true;
        }
    }
    oath.ignore_voice = (oath.ignore_voice - dt).max(0.0);
    let in_play = *state.get() == AppState::InGame && !menu.0 && active.is_some();
    let me = active.as_ref().and_then(|a| a.local_player());
    if !oath.open {
        oath.ptt = false;
        // A yelled oath (voice_ui → OathVoice::claim): show it, seal it, same commands as typing.
        if let Some(y) = oath.yell.take() {
            if in_play && me.is_some_and(|p| p.alive && p.corps == y.corps) {
                let sections = texts.sections(y.corps);
                let section = (y.section as usize).min(sections.len() - 1);
                let o = &mut *oath;
                o.start(y.corps, section as u8, &sections[section], false);
                o.pos = o.text.len();
                o.voice = VoicePhase::Sealing { accuracy: y.accuracy, seconds: y.seconds };
                rotation.note(y.corps, section as u8);
                cmds.write(SimCommand::local(Cmd::OathBegin { section: section as u8 }));
            }
            return;
        }
        if in_play && !console.open && binds.just_pressed(Action::Oath, &keys, &mouse) && me.is_some_and(|p| p.alive) {
            let corps = me.map(|p| p.corps).unwrap_or(prefs.corps);
            let sections = texts.sections(corps);
            let section = rotation.next(corps, sections.len()).min(sections.len() as u8 - 1);
            oath.start(corps, section, &sections[section as usize], true);
            focus.typing = true;
            cmds.write(SimCommand::local(Cmd::OathBegin { section }));
            // The F that opened it is not a typed letter.
            if let Some(k) = binds.get(Action::Oath).key() {
                keys.clear_just_pressed(k);
            }
            typed.clear();
        }
        return;
    }
    if !in_play {
        typed.clear();
        close(&mut oath, &mut focus);
        cmds.write(SimCommand::local(Cmd::OathCancel));
        return;
    }
    let o = &mut *oath;
    o.since_begin += dt;
    o.flash = (o.flash - dt * 5.0).max(0.0);
    o.shake = (o.shake - dt * 7.0).max(0.0);
    o.missed = (o.missed - dt).max(0.0);
    if o.started {
        o.elapsed += dt;
    }
    // Voice: hold the voice key before the first letter to speak the section instead.
    let voice_key = binds.get(Action::Voice).key();
    let voice_down = binds.pressed(Action::Voice, &keys, &mouse);
    if o.typing_focus && voice.available() && !o.started && o.voice == VoicePhase::Off && binds.just_pressed(Action::Voice, &keys, &mouse) {
        o.voice = VoicePhase::Arming { held: 0.0, letter: None };
        // The recital really starts now: restart the host's clock (it only restarts before any
        // progress), so a pause after opening the overlay costs nothing — same as typing below.
        cmds.write(SimCommand::local(Cmd::OathBegin { section: o.section }));
        o.since_begin = 0.0;
    }
    let was_started = o.started;
    let mut finished = false;
    for ev in typed.read() {
        if ev.state != ButtonState::Pressed || finished {
            continue;
        }
        if ev.logical_key == Key::Escape && o.typing_focus {
            close(o, &mut focus);
            cmds.write(SimCommand::local(Cmd::OathCancel));
            sfx.write(PlaySfx::new("ui_back").gain(0.5));
            keys.clear_just_pressed(KeyCode::Escape);
            return;
        }
        if !o.typing_focus || matches!(o.voice, VoicePhase::Listening { .. } | VoicePhase::Sealing { .. }) {
            continue;
        }
        let text = ev.text.as_ref().map(|t| t.as_str()).or(match &ev.logical_key {
            Key::Character(c) => Some(c.as_str()),
            Key::Space => Some(" "),
            _ => None,
        });
        let Some(t) = text else { continue };
        if let VoicePhase::Arming { letter, .. } = &mut o.voice
            && voice_key == Some(ev.key_code)
        {
            // Held back: typed only if this turns out to be a tap.
            if letter.is_none() && !ev.repeat {
                *letter = Some(t.to_string());
            }
            continue;
        }
        for c in t.chars().filter(|c| !c.is_control()) {
            if o.feed(c, &mut sfx) {
                finished = true;
                break;
            }
        }
    }
    match &mut o.voice {
        VoicePhase::Arming { held, letter } => {
            *held += dt;
            if !voice_down {
                // A tap: it was a letter after all.
                let l = letter.take();
                o.voice = VoicePhase::Off;
                for c in l.iter().flat_map(|l| l.chars()).filter(|c| !c.is_control()) {
                    if !finished && o.feed(c, &mut sfx) {
                        finished = true;
                    }
                }
            } else if *held >= VOICE_HOLD_SECS {
                o.voice = VoicePhase::Listening { t: *held, released: None };
                o.dirty = true;
            }
        }
        VoicePhase::Listening { t, released } => {
            *t += dt;
            if released.is_none() && !voice_down {
                *released = Some(*t);
            }
            if released.is_some_and(|r| *t - r > VOICE_TIMEOUT_SECS) {
                o.voice = VoicePhase::Off;
                o.missed = 1.6;
                o.dirty = true;
            }
        }
        _ => {}
    }
    o.ptt = matches!(o.voice, VoicePhase::Listening { released: None, .. });
    if o.started && !was_started {
        // First typed letter: restart the host's clock here (typing is timed from the first key).
        cmds.write(SimCommand::local(Cmd::OathBegin { section: o.section }));
        o.since_begin = 0.0;
    }
    // Hint line under the bar.
    // (Built only when it changes: no per-frame allocation.)
    let hint: Option<&str> = match &o.voice {
        VoicePhase::Listening { released: None, .. } => Some("LISTENING…"),
        VoicePhase::Listening { .. } => Some("…"),
        VoicePhase::Sealing { .. } if !o.typing_focus => Some("OATH SPOKEN"),
        VoicePhase::Sealing { .. } => Some(""),
        _ if o.missed > 0.0 => Some("didn't catch that"),
        _ if o.typing_focus && voice.available() && !o.started => None,
        _ => Some(""),
    };
    match hint {
        Some(h) if h != o.hint => {
            o.hint.clear();
            o.hint.push_str(h);
            o.dirty = true;
        }
        None if !o.hint.starts_with("hold ") => {
            o.hint = format!("hold {} to speak it", binds.get(Action::Voice).label());
            o.dirty = true;
        }
        _ => {}
    }
    // Swallow game keys this frame (they are letters now).
    if o.typing_focus {
        for a in Action::ALL {
            if let Some(k) = binds.get(a).key() {
                keys.clear_just_pressed(k);
            }
        }
    }
    if o.pos >= o.reported + 12 && !finished {
        o.reported = o.pos;
        cmds.write(SimCommand::local(Cmd::OathProgress { correct_chars: len16(o.pos), accuracy: o.accuracy() }));
    }
    let len = o.text.len();
    let result = if finished {
        let acc = o.accuracy();
        let secs = o.elapsed.max(0.05);
        Some((acc, secs, Grade::of(acc, secs, len), false))
    } else if let VoicePhase::Sealing { accuracy, seconds } = o.voice {
        if o.reported < len {
            // Spoken in full: an interrupting hit refunds like a finished-but-unsealed recital.
            o.reported = len;
            o.pos = len;
            o.dirty = true;
            cmds.write(SimCommand::local(Cmd::OathProgress { correct_chars: len16(len), accuracy }));
        }
        // Never before the host's plausibility floor (whisper latency usually covers it).
        (o.since_begin >= oath_min_secs(len16(len))).then_some((accuracy, seconds, Grade::of(accuracy, seconds, len), true))
    } else {
        None
    };
    if let Some((acc, secs, g, voice)) = result {
        cmds.write(SimCommand::local(Cmd::OathResult { accuracy: acc, seconds: secs, voice }));
        sfx.write(PlaySfx::new("oath_complete").gain(0.8));
        // Performance word: bd_fx's hard-light voxel word near the crosshair when available,
        // else the overlay's own text word.
        if let Some(w) = words.as_mut() {
            let c = o.corps.unwrap_or(Corps::Willpower).color();
            w.write(bd_fx::SpawnOnomatopoeia::performance(g.key(), c));
        } else {
            o.grade = Some((g, 1.1));
        }
        close(o, &mut focus);
    }
}

fn close(o: &mut OathState, focus: &mut UiFocus) {
    if o.listening() {
        // Whisper is still running for this recital: its transcript must not become a cast.
        o.ignore_voice = VOICE_TIMEOUT_SECS;
    }
    o.voice = VoicePhase::Off;
    o.ptt = false;
    o.open = false;
    o.dirty = true;
    if o.typing_focus {
        focus.typing = false;
    }
    o.typing_focus = false;
}

/// Sim side: an interrupting hit or a rejection closes the overlay.
fn oath_events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    mut oath: ResMut<OathState>,
    mut focus: ResMut<UiFocus>,
) {
    let local = active.as_ref().and_then(|a| a.local);
    for e in events.read() {
        match &e.event {
            GameEvent::OathInterrupted { player, .. } | GameEvent::OathRejected { player, .. } if Some(*player) == local && oath.open => {
                close(&mut oath, &mut focus);
            }
            _ => {}
        }
    }
}

fn spawn_oath_ui(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Oath"),
            OathRoot,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(58),
                width: px(900),
                margin: UiRect::left(px(-450)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10),
                padding: UiRect::axes(px(28), px(14)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundGradient::from(bevy::ui::RadialGradient {
                stops: vec![
                    bevy::ui::ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.62), percent(0)),
                    bevy::ui::ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.0), percent(100)),
                ],
                ..default()
            }),
            UiTransform::default(),
            Visibility::Hidden,
            Pickable::IGNORE,
            GlobalZIndex(56),
        ))
        .with_children(|root| {
            root.spawn((
                OathDone,
                Text::new(""),
                TextFont::from_font_size(26.0),
                TextColor(Color::WHITE),
                TextShadow { offset: Vec2::ZERO, color: Color::srgba(0.2, 1.0, 0.35, 0.9) },
                TextLayout::justify(Justify::Center),
            ))
            .with_children(|t| {
                t.spawn((OathNext, TextSpan::new(""), TextFont::from_font_size(26.0), TextColor(Color::WHITE)));
                t.spawn((OathRest, TextSpan::new(""), TextFont::from_font_size(26.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.5))));
            });
            root.spawn((OathBar, Node { width: px(0), height: px(2), ..default() }, BackgroundColor(Color::WHITE)));
            root.spawn((
                OathHint,
                Text::new(""),
                TextFont::from_font_size(15.0),
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)),
                TextLayout::justify(Justify::Center),
            ));
        });
    commands.spawn((
        Name::new("OathGrade"),
        OathGrade,
        Text::new(""),
        TextFont::from_font_size(44.0),
        TextColor(Color::NONE),
        TextShadow { offset: Vec2::new(0.0, 2.0), color: Color::srgba(0.0, 0.0, 0.0, 0.5) },
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(400),
            margin: UiRect { left: px(-200), top: px(40), ..default() },
            ..default()
        },
        UiTransform::default(),
        Pickable::IGNORE,
        GlobalZIndex(56),
    ));
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_oath(
    mut oath: ResMut<OathState>,
    voice: Res<VoiceUi>,
    mut root: Query<(&mut Visibility, &mut UiTransform), (With<OathRoot>, Without<OathGrade>)>,
    mut done: Query<(&mut Text, &mut TextShadow, &mut TextColor), (With<OathDone>, Without<OathGrade>)>,
    mut next: Query<(&mut TextSpan, &mut TextColor), (With<OathNext>, Without<OathRest>, Without<OathDone>, Without<OathGrade>)>,
    mut rest: Query<&mut TextSpan, (With<OathRest>, Without<OathNext>)>,
    mut bar: Query<(&mut Node, &mut BackgroundColor), With<OathBar>>,
    mut grade: Query<(&mut Text, &mut TextColor, &mut UiTransform), (With<OathGrade>, Without<OathDone>, Without<OathRoot>)>,
    mut hint: Query<(&mut Text, &mut TextColor), (With<OathHint>, Without<OathDone>, Without<OathGrade>, Without<OathNext>)>,
) {
    let o = &mut *oath;
    let c = corps_color(o.corps.unwrap_or(Corps::Willpower));
    // Grade word pop (independent of the overlay).
    if let Ok((mut t, mut col, mut tf)) = grade.single_mut() {
        match o.grade {
            Some((g, left)) => {
                if t.0 != g.word() {
                    t.0.clear();
                    t.0.push_str(g.word());
                }
                let age = 1.1 - left;
                let s = if age < 0.12 { 0.6 + age / 0.12 * 0.55 } else { 1.15 - ((age - 0.12) * 0.6).min(0.15) };
                tf.scale = Vec2::splat(s);
                let a = (left / 0.35).min(1.0);
                col.0 = match g {
                    Grade::Perfect => c.mix(&Color::WHITE, 0.35),
                    Grade::Good => Color::WHITE,
                    Grade::Slow => Color::srgb(0.55, 0.62, 0.8),
                }
                .with_alpha(a);
            }
            None => {
                if col.0 != Color::NONE {
                    col.0 = Color::NONE;
                }
            }
        }
    }
    if o.open {
        // Error shake: a few px, decaying (cheap, per frame only while open).
        if let Ok((_, mut tf)) = root.single_mut() {
            let x = (o.shake * 40.0).sin() * 6.0 * o.shake;
            tf.translation = Val2::px(x, 0.0);
        }
        if let Ok((_, mut col)) = next.single_mut() {
            let err = Color::srgb(1.0, 0.25, 0.2);
            col.0 = Color::WHITE.mix(&err, o.flash);
        }
        // Listening: the hint breathes with the mic level, in the corps colour.
        if let Ok((_, mut col)) = hint.single_mut() {
            col.0 = if o.ptt {
                c.mix(&Color::WHITE, 0.3).with_alpha(0.6 + 0.4 * voice.level)
            } else if o.listening() {
                c.with_alpha(0.7)
            } else {
                Color::srgba(1.0, 1.0, 1.0, 0.45)
            };
        }
    }
    if !o.dirty {
        return;
    }
    o.dirty = false;
    for (mut v, _) in &mut root {
        *v = if o.open { Visibility::Inherited } else { Visibility::Hidden };
    }
    if !o.open {
        return;
    }
    let pos = o.pos.min(o.text.len());
    if let Ok((mut t, mut sh, mut col)) = done.single_mut() {
        t.0.clear();
        t.0.extend(o.text[..pos].iter());
        col.0 = c.mix(&Color::WHITE, 0.25);
        sh.color = c.with_alpha(0.9);
    }
    if let Ok((mut t, _)) = next.single_mut() {
        t.0.clear();
        if let Some(ch) = o.text.get(pos) {
            if *ch == ' ' {
                t.0.push('_');
            } else {
                t.0.push(*ch);
            }
        }
    }
    if let Ok(mut t) = rest.single_mut() {
        t.0.clear();
        if pos + 1 < o.text.len() {
            t.0.extend(o.text[pos + 1..].iter());
        }
    }
    if let Ok((mut n, mut bg)) = bar.single_mut() {
        n.width = px(420.0 * o.progress());
        bg.0 = c.with_alpha(0.9);
    }
    if let Ok((mut t, _)) = hint.single_mut()
        && t.0 != o.hint
    {
        t.0.clone_from(&o.hint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_sim::willpower::OathTable;

    #[test]
    fn grades() {
        let len = 110;
        let t = target_secs(len);
        assert_eq!(Grade::of(1.0, t * 0.9, len), Grade::Perfect);
        assert_eq!(Grade::of(0.9, t, len), Grade::Good);
        assert_eq!(Grade::of(0.6, t, len), Grade::Slow);
        assert_eq!(Grade::of(1.0, t * 3.0, len), Grade::Slow);
    }

    #[test]
    fn oaths_parse_into_sections_that_match_the_sim() {
        let o = OathTexts::load();
        let table = OathTable::default();
        for c in Corps::ALL {
            let secs = o.by_corps.get(c.name()).unwrap_or_else(|| panic!("no oath for {}", c.name()));
            assert!(secs.len() >= 2, "{}: {secs:?}", c.name());
            assert_eq!(secs.len(), table.section_count(c) as usize, "{}", c.name());
            for (k, s) in secs.iter().enumerate() {
                assert!(!s.contains('\n') && !s.contains("  "), "{} oath not collapsed: {s:?}", c.name());
                // The host's length for this section is exactly what the player types.
                assert_eq!(s.chars().count(), table.section_len(c, k as u8) as usize, "{} §{k}", c.name());
            }
        }
        // Death has no corps yet: parsed, ignored by the game.
        assert!(o.by_corps.contains_key("Death"));
    }

    #[test]
    fn bias_words_carry_the_oaths_odd_words() {
        let t = texts();
        let fear = t.bias_words(Corps::Fear, 24);
        assert!(fear.contains("Sinestro's") && !fear.contains("those"), "{fear}");
        let comp = t.bias_words(Corps::Compassion, 24);
        for w in ["lorek", "nakka", "Natromo", "tornek", "Abin", "Formorrow"] {
            assert!(comp.contains(w), "{w} missing from {comp}");
        }
        assert!(t.bias_words(Corps::Willpower, 3).split(", ").count() == 3);
    }

    #[test]
    fn sections_rotate_and_never_repeat() {
        let mut r = OathRotation::default();
        for n in 1..=MAX_OATH_SECTIONS {
            let mut r2 = OathRotation::default();
            let mut prev = None;
            let mut seen = [false; MAX_OATH_SECTIONS];
            for _ in 0..3 * n {
                let s = r2.next(Corps::Fear, n);
                assert!((s as usize) < n);
                if n >= 2 {
                    assert_ne!(Some(s), prev, "{n} sections");
                }
                seen[s as usize] = true;
                prev = Some(s);
            }
            assert!(seen[..n].iter().all(|s| *s), "every section comes up");
        }
        // Per corps, remembered across recitals; a yelled section moves the rotation past it.
        assert_eq!(r.next(Corps::Rage, 2), 0);
        assert_eq!(r.next(Corps::Hope, 2), 0);
        assert_eq!(r.next(Corps::Rage, 2), 1);
        r.note(Corps::Hope, 1);
        assert_eq!(r.next(Corps::Hope, 2), 0);
        // The count shrank (a different oaths file): still in range.
        r.note(Corps::Love, 3);
        assert!(r.next(Corps::Love, 2) < 2);
    }

    fn texts() -> OathTexts {
        OathTexts::from_ron(include_str!("../../../assets/oaths.ron")).unwrap()
    }

    #[test]
    fn yelling_your_oath_outside_the_overlay_is_a_recital_not_a_cast() {
        let t = texts();
        let mut o = OathState::default();
        // A construct phrase goes on to the ring resolver.
        assert!(!route_transcript(&mut o, &t, Some(Corps::Willpower), "giant hammer", 1.0));
        assert!(o.yell.is_none());
        // Your own oath's second line: claimed, section 1, timed by the speaking time.
        let spoken = "let those who worship evils might beware my power green lanterns light";
        assert!(route_transcript(&mut o, &t, Some(Corps::Willpower), spoken, 5.0));
        let y = o.yell.take().unwrap();
        assert_eq!((y.corps, y.section), (Corps::Willpower, 1));
        let len = t.sections(Corps::Willpower)[1].chars().count();
        // 77 chars in 5 s ≈ 15 chars/s: fast speech, faster than the target, still humanly plausible.
        assert!(y.accuracy > 0.95 && y.seconds == 5.0 && 5.0 < target_secs(len), "{y:?}");
        assert!(y.seconds >= oath_min_secs(len16(len)));
        // Spoken fast and clean it earns the same grade (and speed bonus) as fast typing.
        assert_eq!(Grade::of(y.accuracy, y.seconds, len), Grade::Perfect);
        // Another corps' oath is just words (cast as usual); so is anything when dead.
        assert!(!route_transcript(&mut o, &t, Some(Corps::Rage), spoken, 3.0));
        assert!(!route_transcript(&mut o, &t, None, spoken, 3.0));
        assert!(o.yell.is_none());
    }

    #[test]
    fn overlay_voice_recital_scores_the_open_section() {
        let t = texts();
        let mut o = OathState::default();
        let sec = t.sections(Corps::Fear)[1].clone();
        o.start(Corps::Fear, 1, &sec, true);
        // Typing mode: a transcript is not the overlay's (old behaviour).
        assert!(!route_transcript(&mut o, &t, Some(Corps::Fear), "in blackest day", 1.0));
        o.voice = VoicePhase::Listening { t: 6.4, released: Some(5.5) };
        assert!(route_transcript(
            &mut o,
            &t,
            Some(Corps::Fear),
            "let those who try to stop what's right burn like my power sinestro's might",
            4.0
        ));
        let VoicePhase::Sealing { accuracy, seconds } = o.voice else { panic!("{:?}", o.voice) };
        assert!(accuracy > 0.95, "{accuracy}");
        // Timed key-down → release (5.5 s), not until the transcript arrived (6.4 s).
        assert_eq!(seconds, 5.5);
        assert!(seconds >= oath_min_secs(len16(sec.chars().count())), "fast speech passes the host's floor");
        assert!(seconds < target_secs(sec.chars().count()), "a fast speaker beats the target time");
        // A slow speaker keeps their (slower) time.
        o.voice = VoicePhase::Listening { t: 30.0, released: Some(29.0) };
        assert!(route_transcript(&mut o, &t, Some(Corps::Fear), "burn like my power", 30.0));
        let VoicePhase::Sealing { accuracy, seconds } = o.voice else { panic!() };
        assert!(accuracy < 0.5 && seconds == 29.0, "{accuracy} {seconds}");
        // Still holding when Whisper's clip limit cut it: the time so far.
        o.voice = VoicePhase::Listening { t: 8.2, released: None };
        assert!(route_transcript(&mut o, &t, Some(Corps::Fear), "burn like my power", 8.2));
        assert!(matches!(o.voice, VoicePhase::Sealing { seconds, .. } if seconds == 8.2));
        // Cancelled while Whisper ran: the late transcript is dropped (neither oath nor cast).
        let mut focus = UiFocus::default();
        o.voice = VoicePhase::Listening { t: 1.0, released: Some(1.0) };
        close(&mut o, &mut focus);
        assert!(route_transcript(&mut o, &t, Some(Corps::Fear), "giant hammer", 1.0));
        assert!(o.yell.is_none());
        assert!(!route_transcript(&mut o, &t, Some(Corps::Fear), "giant hammer", 1.0));
    }
}
