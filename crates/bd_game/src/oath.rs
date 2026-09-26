//! Oath recital (F): the corps oath from `assets/oaths.ron` as a typing test (COMBAT.md §2).
//!
//! * Opens only in play with the console closed; sends `Cmd::OathBegin`, sets the TYPING bit
//!   (movement keys are text; aim / mouse tethers / momentum stay live).
//! * Letters light up in the corps colour as they are typed; a wrong key flashes the expected letter
//!   and shakes the line (the letter must still be typed). Case and punctuation are forgiving:
//!   punctuation is skipped automatically, letters match case-insensitively.
//! * Progress goes to the sim every few letters (`OathProgress`, improves the partial refill if a hit
//!   interrupts). On the last letter: `OathResult { accuracy, seconds }` and a PERFECT / GOOD / SLOW
//!   performance word. Esc cancels (no refill). `OathInterrupted` from the sim closes it.

use crate::audio::PlaySfx;
use crate::console::corps_color;
use crate::input::UiFocus;
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::lobby::UiSet;
use crate::sim_bridge::{ActiveWorld, Cmd, GameEvents, SimCommand};
use crate::state::{AppState, InGameMenu};
use bd_core::corps::Corps;
use bd_sim::GameEvent;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::picking::Pickable;
use bevy::prelude::*;
use std::collections::HashMap;

/// Characters per second that earn the full refill (mirrors `bd_sim::willpower`).
fn target_secs(len: usize) -> f32 {
    bd_sim::willpower::oath_target_secs(len.min(u16::MAX as usize) as u16)
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

/// Oath texts by corps (assets/oaths.ron).
#[derive(Resource, Default)]
pub struct OathTexts(pub HashMap<String, String>);

impl OathTexts {
    fn load() -> Self {
        let path = crate::assets_dir().join("oaths.ron");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        // String keys (not `Corps`) so an oath for a corps the game doesn't have yet (e.g. Death)
        // doesn't make the whole file fail to parse.
        let map = match ron::from_str::<HashMap<String, String>>(&text) {
            Ok(m) => m.into_iter().map(|(k, v)| (k, v.split_whitespace().collect::<Vec<_>>().join(" "))).collect(),
            Err(e) => {
                warn!("oaths.ron: {e}");
                HashMap::new()
            }
        };
        Self(map)
    }
    pub fn get(&self, c: Corps) -> String {
        self.0.get(c.name()).cloned().unwrap_or_else(|| "In brightest day, in blackest night, no evil shall escape my sight.".into())
    }
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

pub struct OathPlugin;

impl Plugin for OathPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(OathTexts::load())
            .init_resource::<OathState>()
            .add_systems(Startup, spawn_oath_ui)
            .add_systems(Update, (oath_keys, oath_events).chain().in_set(UiSet::Modal))
            .add_systems(Update, draw_oath.in_set(UiSet::Draw))
            .add_systems(OnExit(AppState::InGame), |mut o: ResMut<OathState>, mut f: ResMut<UiFocus>| {
                if o.open {
                    o.open = false;
                    o.dirty = true;
                    f.typing = false;
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
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    time: Res<Time<Real>>,
    texts: Res<OathTexts>,
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
    let in_play = *state.get() == AppState::InGame && !menu.0 && active.is_some();
    if !oath.open {
        if in_play && !console.open && binds.just_pressed(Action::Oath, &keys, &mouse) {
            let a = active.as_ref().map(|a| a.as_ref());
            let me = a.and_then(|a| a.local_player());
            if me.is_some_and(|p| p.alive) {
                let corps = me.map(|p| p.corps).unwrap_or(prefs.corps);
                let o = &mut *oath;
                o.open = true;
                o.text = texts.get(corps).chars().collect();
                o.pos = 0;
                o.correct = 0;
                o.errors = 0;
                o.elapsed = 0.0;
                o.started = false;
                o.reported = 0;
                o.corps = Some(corps);
                o.grade = None;
                o.dirty = true;
                o.skip_free();
                focus.typing = true;
                cmds.write(SimCommand::local(Cmd::OathBegin));
                // The F that opened it is not a typed letter.
                if let Some(k) = binds.get(Action::Oath).key() {
                    keys.clear_just_pressed(k);
                }
                typed.clear();
            }
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
    o.flash = (o.flash - dt * 5.0).max(0.0);
    o.shake = (o.shake - dt * 7.0).max(0.0);
    if o.started {
        o.elapsed += dt;
    }
    let mut finished = false;
    for ev in typed.read() {
        if ev.state != ButtonState::Pressed || finished {
            continue;
        }
        if ev.logical_key == Key::Escape {
            close(o, &mut focus);
            cmds.write(SimCommand::local(Cmd::OathCancel));
            sfx.write(PlaySfx::new("ui_back").gain(0.5));
            keys.clear_just_pressed(KeyCode::Escape);
            return;
        }
        let text = ev.text.as_ref().map(|t| t.as_str()).or(match &ev.logical_key {
            Key::Character(c) => Some(c.as_str()),
            Key::Space => Some(" "),
            _ => None,
        });
        let Some(t) = text else { continue };
        for c in t.chars().filter(|c| !c.is_control()) {
            let Some(&want) = o.text.get(o.pos) else { break };
            o.started = true;
            if c.to_lowercase().eq(want.to_lowercase()) {
                o.pos += 1;
                o.correct += 1;
                o.skip_free();
                o.dirty = true;
                let pitch = 0.9 + 0.35 * o.progress();
                sfx.write(PlaySfx::new("type_key").gain(0.35).pitch(pitch));
            } else if c == ' ' && want != ' ' {
                // Stray space: ignore silently (forgiving).
            } else {
                o.errors += 1;
                o.flash = 1.0;
                o.shake = 1.0;
                o.dirty = true;
                sfx.write(PlaySfx::new("type_error").gain(0.5));
            }
            if o.pos >= o.text.len() {
                finished = true;
                break;
            }
        }
    }
    // Swallow game keys this frame (they are letters now).
    for a in [Action::Dismiss, Action::Power, Action::Voice, Action::Console, Action::ConsoleAlt, Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4] {
        if let Some(k) = binds.get(a).key() {
            keys.clear_just_pressed(k);
        }
    }
    if o.pos >= o.reported + 12 && !finished {
        o.reported = o.pos;
        cmds.write(SimCommand::local(Cmd::OathProgress { correct_chars: o.pos.min(u16::MAX as usize) as u16, accuracy: o.accuracy() }));
    }
    if finished {
        let acc = o.accuracy();
        let secs = o.elapsed.max(0.05);
        let g = Grade::of(acc, secs, o.text.len());
        cmds.write(SimCommand::local(Cmd::OathResult { accuracy: acc, seconds: secs }));
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
    o.open = false;
    o.dirty = true;
    focus.typing = false;
}

/// Sim side: an interrupting hit or a rejection closes the overlay.
fn oath_events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut oath: ResMut<OathState>, mut focus: ResMut<UiFocus>) {
    let local = active.as_ref().and_then(|a| a.local);
    for e in events.read() {
        match &e.event {
            GameEvent::OathInterrupted { player, .. } | GameEvent::OathRejected { player, .. } if Some(*player) == local => {
                if oath.open {
                    close(&mut oath, &mut focus);
                }
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
        });
    commands.spawn((
        Name::new("OathGrade"),
        OathGrade,
        Text::new(""),
        TextFont::from_font_size(44.0),
        TextColor(Color::NONE),
        TextShadow { offset: Vec2::new(0.0, 2.0), color: Color::srgba(0.0, 0.0, 0.0, 0.5) },
        TextLayout::justify(Justify::Center),
        Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), width: px(400), margin: UiRect { left: px(-200), top: px(40), ..default() }, ..default() },
        UiTransform::default(),
        Pickable::IGNORE,
        GlobalZIndex(56),
    ));
}

#[allow(clippy::type_complexity)]
fn draw_oath(
    mut oath: ResMut<OathState>,
    mut root: Query<(&mut Visibility, &mut UiTransform), (With<OathRoot>, Without<OathGrade>)>,
    mut done: Query<(&mut Text, &mut TextShadow, &mut TextColor), (With<OathDone>, Without<OathGrade>)>,
    mut next: Query<(&mut TextSpan, &mut TextColor), (With<OathNext>, Without<OathRest>, Without<OathDone>, Without<OathGrade>)>,
    mut rest: Query<&mut TextSpan, (With<OathRest>, Without<OathNext>)>,
    mut bar: Query<(&mut Node, &mut BackgroundColor), With<OathBar>>,
    mut grade: Query<(&mut Text, &mut TextColor, &mut UiTransform), (With<OathGrade>, Without<OathDone>, Without<OathRoot>)>,
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn oaths_parse() {
        let o = OathTexts::load();
        for c in Corps::ALL {
            let s = o.0.get(c.name()).unwrap_or_else(|| panic!("no oath for {}", c.name()));
            assert!(!s.contains('\n') && !s.contains("  "), "{} oath not collapsed: {s:?}", c.name());
        }
    }
}
