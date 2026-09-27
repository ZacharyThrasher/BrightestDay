//! MENU FEEL: value-row controls + the mouse side of the menu shell.
//!
//! Every `Row::Value` knob draws as the control that says how to change it:
//! - **slider** (track + fill + handle + readout) for continuous / wide numeric settings — click the
//!   track to jump, drag the handle, wheel over it, ←/→ (held: accelerating), d-pad;
//! - **toggle switch** for ON / OFF (→ = on, ← = off, click / Enter flips);
//! - **segmented control** for 2–4 named options (all visible, the chosen one lit; click one);
//! - **picker** (‹ value ›, clickable arrows) for long lists (maps, corps, accents).
//!
//! The kind is derived, not hand-listed: numeric knobs with a known range use [`spec`]; any other
//! knob is enumerated by stepping a scratch copy of the settings with `step_knob` (so knobs other
//! workers add pick a sensible control automatically). Values always move through `step_knob`, so
//! steps, clamps, save/load and host sync behave exactly as with the keyboard.
//!
//! Mouse input is hit-tested here against the laid-out nodes (from `CursorMoved`, so the UI script
//! harness drives it too) and arbitrated by [`nav::Arbiter`]: a stationary pointer never moves focus.

use super::nav::{self, Curve};
use super::{knob_text, menu_open, step_knob, Ctx, Knob, MenuIo, MenuPage, MenuRow, MenuState, Row, RowValue, SettingsTab, TabButton, MAX_VISIBLE};
use crate::audio::PlaySfx;
use crate::lobby::prefs::UiPrefs;
use crate::net::{NetRequest, NetRole};
use crate::settings::{Settings, FOV_MAX, FOV_MIN};
use crate::state::AppState;
use bd_sim::rules::{Scoring, UNLIMITED};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::window::{CursorLeft, CursorMoved, PrimaryWindow};

// ------------------------------------------------------------------------------------------ model

/// How a value row is drawn / driven.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Control {
    Slider { frac: f32, ticks: u32, map: SliderMap },
    Toggle(bool),
    Segmented { opts: Vec<String>, idx: usize },
    Picker,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SliderMap {
    /// A numeric knob with a known range ([`spec`]).
    Value { lo: f32, hi: f32, curve: Curve },
    /// An enumerated clamped stepper (every reachable value, in order).
    Steps(Vec<String>),
}

/// A numeric knob's current value and range (the same clamps as `step_knob`; tested).
#[derive(Clone, Copy, Debug)]
struct Spec {
    v: f32,
    lo: f32,
    hi: f32,
    curve: Curve,
    /// Whole-number steps: draw a tick per step when there are few.
    whole: bool,
}

fn spec(k: Knob, c: &Ctx) -> Option<Spec> {
    let s = &*c.settings;
    let p = &*c.prefs;
    let r = &p.game.rules;
    let pa = &p.game.party;
    let lin = |v: f32, lo: f32, hi: f32| Some(Spec { v, lo, hi, curve: Curve::Lin, whole: false });
    let log = |v: f32, lo: f32, hi: f32| Some(Spec { v, lo, hi, curve: Curve::Log, whole: false });
    let whole = |v: u32, lo: u32, hi: u32| Some(Spec { v: v as f32, lo: lo as f32, hi: hi as f32, curve: Curve::Lin, whole: true });
    match k {
        Knob::Sensitivity => log(s.sensitivity, 0.05, 20.0),
        Knob::Fov => lin(s.fov, FOV_MIN, FOV_MAX),
        Knob::AdsSens => lin(p.camera.ads_sens(), 0.2, 2.5),
        Knob::AdsSpeed => lin(p.camera.ads_speed(), 0.25, 4.0),
        Knob::OutlineWidth => lin(p.outline_width, 0.5, 2.0),
        Knob::Shake => lin(s.comfort.shake, 0.0, 2.0),
        Knob::FovKick => lin(s.comfort.fov_kick, 0.0, 2.0),
        Knob::LandingDip => lin(s.comfort.landing_dip, 0.0, 2.0),
        Knob::Flashes => lin(s.comfort.flashes, 0.0, 1.0),
        Knob::Vignette => lin(s.comfort.vignette, 0.0, 1.0),
        Knob::Volume => lin(s.master_volume, 0.0, 1.0),
        Knob::MusicVol => lin(s.audio.music, 0.0, 1.0),
        Knob::WorldVol => lin(s.audio.world, 0.0, 1.0),
        Knob::AmbienceVol => lin(s.audio.ambience, 0.0, 1.0),
        Knob::FeedbackVol => lin(s.audio.feedback, 0.0, 1.0),
        Knob::HeadScale => lin(p.costume.head_scale, 0.7, 1.35),
        Knob::Sparks => whole(r.willpower_segments() as u32, bd_sim::rules::WILLPOWER_SEGMENTS_MIN as u32, bd_sim::rules::WILLPOWER_SEGMENTS_MAX as u32),
        Knob::Lives => match r.scoring {
            Scoring::Stock { lives } => whole(lives.max(1) as u32, 1, super::STOCK_MAX_LIVES as u32),
            Scoring::TimedKos => None,
        },
        Knob::KoTarget => log(r.ko_target as f32, 1.0, 200.0),
        Knob::TimeLimit => log(r.time_limit, 60.0, 3600.0),
        Knob::Respawn => lin(r.respawn_secs, 0.0, 30.0),
        Knob::Shield => lin(r.spawn_shield_secs, 0.0, 10.0),
        Knob::Knockback => lin(r.knockback_scale, 0.0, 4.0),
        Knob::Gravity => lin(p.game.gravity_scale, 0.25, 3.0),
        Knob::GasRegen => lin(p.game.gas_regen_scale, 0.0, 4.0),
        Knob::Limit(l) => {
            let (lo, hi, unl) = l.range();
            if hi <= 1 {
                return None; // OFF / 1: a two-option picker
            }
            let mut lim = r.limits;
            let v = *l.get(&mut lim);
            // UNLIMITED is the notch past the max.
            whole(if v == UNLIMITED { hi as u32 + 1 } else { v as u32 }, lo as u32, hi as u32 + unl as u32)
        }
        Knob::Rounds => whole(pa.rounds as u32, 1, 15),
        Knob::RoundKo => log(pa.round_ko_target as f32, 1.0, 100.0),
        Knob::RoundTime => log(pa.round_time, 30.0, 1800.0),
        Knob::PtsKo => lin(pa.points_per_ko as f32, 0.0, 100.0),
        Knob::PtsWin => lin(pa.points_round_win as f32, 0.0, 100.0),
        Knob::PickSecs => lin(pa.pick_secs, 4.0, 60.0),
        _ => None,
    }
}

/// Enumeration cap (either way): longer steppers stay pickers.
const CAP: usize = 64;

/// A scratch `Ctx` over copies (enumeration / seeking never touch the real settings).
fn scratch<'s, 'n: 's>(c: &Ctx<'n>, s: &'s mut Settings, p: &'s mut UiPrefs) -> Ctx<'s> {
    Ctx { settings: s, prefs: p, net: c.net, voice: c.voice, in_game: c.in_game }
}

fn text_of(k: Knob, c: &Ctx, s: &mut Settings, p: &mut UiPrefs) -> String {
    knob_text(k, &scratch(c, s, p)).1
}

/// Every value a knob can take by stepping.
struct Options {
    /// In step order (cycles: a stable rotation).
    opts: Vec<String>,
    /// The current one.
    idx: usize,
    /// A stepper with ends (else a cycle).
    clamped: bool,
    /// The current value sits outside the cycle (a CUSTOM preset) and was appended last.
    stand_in: bool,
}

fn enumerate(k: Knob, c: &Ctx) -> Option<Options> {
    let (mut s, mut p) = (c.settings.clone(), c.prefs.clone());
    let cur = text_of(k, c, &mut s, &mut p);
    let mut back = vec![cur.clone()];
    let mut cycle_at = None;
    let mut stuck = false;
    for _ in 0..CAP {
        step_knob(k, &mut scratch(c, &mut s, &mut p), -1, false);
        let t = text_of(k, c, &mut s, &mut p);
        if back.last() == Some(&t) {
            stuck = true;
            break;
        }
        if let Some(i) = back.iter().position(|x| *x == t) {
            cycle_at = Some(i);
            break;
        }
        back.push(t);
    }
    if stuck {
        let mut list: Vec<String> = back.into_iter().rev().collect();
        let idx = list.len() - 1;
        let (mut s, mut p) = (c.settings.clone(), c.prefs.clone());
        for _ in 0..CAP {
            step_knob(k, &mut scratch(c, &mut s, &mut p), 1, false);
            let t = text_of(k, c, &mut s, &mut p);
            if list.last() == Some(&t) || list.contains(&t) {
                return Some(Options { opts: list, idx, clamped: true, stand_in: false });
            }
            list.push(t);
        }
        return None;
    }
    let at = cycle_at?;
    let mut cyc: Vec<String> = back[at..].iter().rev().cloned().collect();
    // A stable order whatever the current value: OFF first, else LEFT, else the default value.
    let first = ["OFF", "LEFT"].iter().find_map(|w| cyc.iter().position(|x| x == w)).or_else(|| {
        let (mut ds, mut dp) = (Settings::default(), UiPrefs::default());
        let d = text_of(k, c, &mut ds, &mut dp);
        cyc.iter().position(|x| *x == d)
    });
    if let Some(f) = first {
        cyc.rotate_left(f);
    }
    let stand_in = !cyc.contains(&cur);
    if stand_in {
        cyc.push(cur.clone());
    }
    let idx = cyc.iter().position(|x| *x == cur).unwrap_or(0);
    Some(Options { opts: cyc, idx, clamped: false, stand_in })
}

/// Widest the row's control may be (row 600 px − bar, gaps, label).
fn seg_fits(label: &str, opts: &[String]) -> bool {
    let label_w = label.chars().count() as f32 * 12.6 + 40.0;
    let segs: f32 = opts.iter().map(|o| seg_label(o).chars().count() as f32 * 9.0 + 21.0).sum();
    label_w + segs <= 590.0
}

/// Segment caption ("2 / 4" variants show just the number).
fn seg_label(o: &str) -> &str {
    o.split(" / ").next().unwrap_or(o)
}

pub(super) fn classify(k: Knob, c: &Ctx) -> Control {
    if let Some(sp) = spec(k, c) {
        let ticks = if sp.whole && sp.hi - sp.lo <= 20.0 { (sp.hi - sp.lo) as u32 } else { 0 };
        return Control::Slider { frac: nav::to_frac(sp.v, sp.lo, sp.hi, sp.curve), ticks, map: SliderMap::Value { lo: sp.lo, hi: sp.hi, curve: sp.curve } };
    }
    let Some(Options { opts, idx, clamped, stand_in }) = enumerate(k, c) else { return Control::Picker };
    let n = opts.len();
    if n == 2 && opts.iter().any(|o| o == "ON") && opts.iter().any(|o| o == "OFF") {
        return Control::Toggle(opts[idx] == "ON");
    }
    if clamped && n > 5 {
        let ticks = if n <= 21 { n as u32 - 1 } else { 0 };
        return Control::Slider { frac: idx as f32 / (n - 1) as f32, ticks, map: SliderMap::Steps(opts) };
    }
    // 2–4 named options (+ a CUSTOM stand-in) → all visible.
    let named = n - stand_in as usize;
    if (2..=4).contains(&named) && seg_fits(&knob_text(k, c).0, &opts) {
        return Control::Segmented { opts, idx };
    }
    Control::Picker
}

// ------------------------------------------------------------------------------------------ changes

fn commit(c: &mut Ctx, s: Settings, p: UiPrefs) {
    if *c.settings != s {
        *c.settings = s;
    }
    if *c.prefs != p {
        *c.prefs = p;
    }
}

/// Step `k` (first in `dir`, then the other way) until it reads `want`. `Some(game)` if it changed.
fn select_text(k: Knob, c: &mut Ctx, want: &str, dir: i32) -> Option<bool> {
    let dir = if dir < 0 { -1 } else { 1 };
    let before = knob_text(k, c).1;
    if before == want {
        return None;
    }
    for d in [dir, -dir] {
        let (mut s, mut p) = (c.settings.clone(), c.prefs.clone());
        let mut last = before.clone();
        for _ in 0..CAP {
            let game = step_knob(k, &mut scratch(c, &mut s, &mut p), d, false);
            let t = text_of(k, c, &mut s, &mut p);
            if t == want {
                commit(c, s, p);
                return Some(game);
            }
            if t == last || t == before {
                break; // stuck or wrapped
            }
            last = t;
        }
    }
    None
}

/// Move a numeric knob as close to `target` as its steps allow (coarse, then fine).
fn seek(k: Knob, c: &mut Ctx, target: f32) -> Option<bool> {
    let start = spec(k, c)?.v;
    let (mut s, mut p) = (c.settings.clone(), c.prefs.clone());
    let mut game = false;
    for fine in [false, true] {
        for _ in 0..800 {
            let v = spec(k, &scratch(c, &mut s, &mut p))?.v;
            let d = target - v;
            if d.abs() < 1e-5 {
                break;
            }
            let (mut s2, mut p2) = (s.clone(), p.clone());
            game = step_knob(k, &mut scratch(c, &mut s2, &mut p2), if d > 0.0 { 1 } else { -1 }, fine);
            let nv = spec(k, &scratch(c, &mut s2, &mut p2))?.v;
            if (target - nv).abs() < d.abs() - 1e-6 {
                (s, p) = (s2, p2);
            } else {
                break;
            }
        }
    }
    let end = spec(k, &scratch(c, &mut s, &mut p))?.v;
    if (end - start).abs() > 1e-6 {
        commit(c, s, p);
        Some(game)
    } else {
        None
    }
}

/// Put a slider at track position `f` (0..1).
fn slide_to(k: Knob, c: &mut Ctx, map: &SliderMap, f: f32) -> Option<bool> {
    match map {
        SliderMap::Value { lo, hi, curve } => seek(k, c, nav::from_frac(f, *lo, *hi, *curve)),
        SliderMap::Steps(opts) => {
            let n = opts.len().max(2);
            let t = ((f.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize).min(opts.len() - 1);
            let cur = knob_text(k, c).1;
            let ci = opts.iter().position(|o| *o == cur).unwrap_or(0);
            select_text(k, c, &opts[t], if t >= ci { 1 } else { -1 })
        }
    }
}

/// ←/→ (and right-click / wheel): sliders and pickers step (`times` for held keys); a toggle goes
/// → on / ← off; a segmented control moves one option and stops at the ends.
pub(super) fn nudge(k: Knob, c: &mut Ctx, dir: i32, fine: bool, times: u32) -> Option<bool> {
    match classify(k, c) {
        Control::Toggle(on) => {
            if (dir > 0) == on {
                return None;
            }
            Some(step_knob(k, c, dir, fine))
        }
        Control::Segmented { opts, idx } => {
            let j = idx as i32 + dir.signum();
            if j < 0 || j as usize >= opts.len() {
                return None;
            }
            select_text(k, c, &opts[j as usize], dir)
        }
        _ => {
            let before = knob_text(k, c).1;
            let mut game = false;
            for _ in 0..times.max(1) {
                game = step_knob(k, c, dir, fine);
            }
            (knob_text(k, c).1 != before).then_some(game)
        }
    }
}

/// Enter / click on the row: toggles flip, segmented / pickers advance (wrapping); a slider only
/// takes focus (its track does the rest).
pub(super) fn activate_value(k: Knob, c: &mut Ctx) -> Option<bool> {
    if matches!(classify(k, c), Control::Slider { .. }) {
        return None;
    }
    let before = knob_text(k, c).1;
    let game = step_knob(k, c, 1, false);
    (knob_text(k, c).1 != before).then_some(game)
}

/// After a change: redraw, and a host re-sends the lobby rules.
pub(super) fn changed(menu: &mut MenuState, io: &mut MenuIo, game: bool) {
    menu.rebuild = true;
    if game && io.net.as_ref().is_some_and(|n| n.role == NetRole::Host) {
        io.req.write(NetRequest::SetSettings(io.prefs.game.clone()));
    }
}

// ------------------------------------------------------------------------------------------ nav state

/// MENU FEEL state (arbitration, repeats, drag, hover, focus memory).
#[derive(Resource, Default)]
pub struct MenuNav {
    pub(super) arb: nav::Arbiter,
    pub(super) keys: nav::Keys,
    drag: Option<Drag>,
    /// (page, tab, cursor, slide-in) of the focused row as drawn: a rebuild keeps it settled.
    sel: Option<(MenuPage, SettingsTab, usize, f32)>,
    hover: Option<Hit>,
    flash: (usize, f32),
    wheel_acc: f32,
    tick_t: f32,
    last: Option<(MenuPage, usize, usize, usize)>,
    saved: Vec<(MenuPage, usize, usize)>,
}

impl MenuNav {
    /// Initial slide-in for row `i` (the focused row keeps its offset across rebuilds).
    pub(super) fn row_pad(&self, page: MenuPage, tab: SettingsTab, cursor: usize, i: usize) -> f32 {
        match self.sel {
            Some((p, t, c, pad)) if p == page && t == tab && c == cursor && i == cursor => pad,
            _ => 0.0,
        }
    }
}

struct Drag {
    row: usize,
    knob: Knob,
    map: SliderMap,
    /// Track hit area, logical px.
    left: f32,
    width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Tab(SettingsTab),
    Slider(usize),
    Seg(usize, usize),
    Arrow(usize, i32),
    Row(usize),
}

impl Hit {
    fn row(self) -> Option<usize> {
        match self {
            Hit::Tab(_) => None,
            Hit::Slider(r) | Hit::Seg(r, _) | Hit::Arrow(r, _) | Hit::Row(r) => Some(r),
        }
    }
}

// ------------------------------------------------------------------------------------------ drawing

#[derive(Component)]
pub(super) struct SliderHit {
    row: usize,
    knob: Knob,
    map: SliderMap,
}
#[derive(Component)]
pub(super) struct SliderFill(usize);
#[derive(Component)]
pub(super) struct SliderHandle(usize);
#[derive(Component)]
pub(super) struct SegOption {
    row: usize,
    idx: usize,
    knob: Knob,
    text: String,
    on: bool,
}
#[derive(Component)]
pub(super) struct SegText {
    row: usize,
    idx: usize,
    on: bool,
}
#[derive(Component)]
pub(super) struct PickArrow {
    row: usize,
    knob: Knob,
    dir: i32,
}

/// Slider hit area width / inner padding (px): the track is the hit area minus the padding.
const SL_W: f32 = 230.0;
const SL_PAD: f32 = 9.0;
const HANDLE: f32 = 14.0;
const HANDLE_FOCUS: f32 = 19.0;

fn dim(a: f32) -> Color {
    Color::srgba(1.0, 1.0, 1.0, a)
}

/// The control on the right of a value row (`value` = the knob's text; `value_color` its colour).
pub(super) fn spawn_control(r: &mut ChildSpawnerCommands, i: usize, k: Knob, ctl: &Control, value: String, value_color: Color, glow: Color) {
    match ctl {
        Control::Slider { frac, ticks, map } => {
            r.spawn((SliderHit { row: i, knob: k, map: map.clone() }, Node { width: px(SL_W), height: percent(100), flex_shrink: 0.0, ..default() }))
                .with_children(|s| {
                    s.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(SL_PAD),
                            right: px(SL_PAD),
                            top: percent(50),
                            height: px(4),
                            margin: UiRect::top(px(-2)),
                            border_radius: BorderRadius::all(px(2)),
                            ..default()
                        },
                        BackgroundColor(dim(0.16)),
                        Pickable::IGNORE,
                    ))
                    .with_children(|t| {
                        t.spawn((
                            SliderFill(i),
                            Node { position_type: PositionType::Absolute, left: px(0), top: px(0), bottom: px(0), width: percent(frac * 100.0), border_radius: BorderRadius::all(px(2)), ..default() },
                            BackgroundColor(glow.with_alpha(0.6)),
                            Pickable::IGNORE,
                        ));
                        for j in 1..*ticks {
                            t.spawn((
                                Node { position_type: PositionType::Absolute, left: percent(j as f32 * 100.0 / *ticks as f32), top: px(-3), width: px(1), height: px(10), ..default() },
                                BackgroundColor(dim(0.2)),
                                Pickable::IGNORE,
                            ));
                        }
                    });
                    s.spawn((
                        SliderHandle(i),
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(SL_PAD + frac * (SL_W - 2.0 * SL_PAD)),
                            top: percent(50),
                            width: px(HANDLE),
                            height: px(HANDLE),
                            margin: UiRect { left: px(-HANDLE / 2.0), top: px(-HANDLE / 2.0), ..default() },
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(Color::WHITE),
                        BorderColor::all(glow),
                        BoxShadow::new(Color::NONE, px(0), px(0), px(1), px(10)),
                        Pickable::IGNORE,
                    ));
                });
            r.spawn((RowValue(i), Text::new(value), TextFont::from_font_size(18.0), TextColor(value_color), TextLayout::justify(Justify::Right), Node { width: px(92), flex_shrink: 0.0, ..default() }));
        }
        Control::Toggle(on) => {
            r.spawn((
                Node {
                    width: px(42),
                    height: px(22),
                    padding: UiRect::all(px(2)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::MAX,
                    align_items: AlignItems::Center,
                    justify_content: if *on { JustifyContent::FlexEnd } else { JustifyContent::FlexStart },
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(if *on { glow.with_alpha(0.75) } else { dim(0.06) }),
                BorderColor::all(if *on { glow } else { dim(0.3) }),
                Pickable::IGNORE,
            ))
            .with_children(|t| {
                t.spawn((Node { width: px(16), height: px(16), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(if *on { Color::WHITE } else { dim(0.55) }), Pickable::IGNORE));
            });
            r.spawn((RowValue(i), Text::new(value), TextFont::from_font_size(16.0), TextColor(value_color), Node { width: px(40), flex_shrink: 0.0, ..default() }));
        }
        Control::Segmented { opts, idx } => {
            r.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(3), flex_shrink: 0.0, ..default() }, Pickable::IGNORE)).with_children(|g| {
                for (j, o) in opts.iter().enumerate() {
                    let on = j == *idx;
                    g.spawn((
                        SegOption { row: i, idx: j, knob: k, text: o.clone(), on },
                        Node { padding: UiRect::axes(px(9), px(3)), border: UiRect::bottom(px(2)), border_radius: BorderRadius::top(px(3)), ..default() },
                        BackgroundColor(if on { glow.with_alpha(0.22) } else { dim(0.04) }),
                        BorderColor::all(if on { glow } else { Color::NONE }),
                    ))
                    .with_children(|b| {
                        b.spawn((SegText { row: i, idx: j, on }, Text::new(seg_label(o)), TextFont::from_font_size(15.0), TextColor(if on { Color::WHITE } else { dim(0.45) }), Pickable::IGNORE));
                    });
                }
            });
        }
        Control::Picker => {
            let arrow = |dir: i32| (PickArrow { row: i, knob: k, dir }, Text::new(if dir < 0 { "<" } else { ">" }), TextFont::from_font_size(20.0), TextColor(dim(0.3)), TextLayout::justify(Justify::Center), Node { width: px(26), flex_shrink: 0.0, ..default() });
            r.spawn(arrow(-1));
            r.spawn((RowValue(i), Text::new(value), TextFont::from_font_size(20.0), TextColor(value_color), TextLayout::justify(Justify::Center), Node { min_width: px(120), flex_shrink: 0.0, ..default() }));
            r.spawn(arrow(1));
        }
    }
}

// ------------------------------------------------------------------------------------------ mouse

/// Pointer (logical px) inside a laid-out node, grown by `grow` logical px: normalized position
/// (−0.5..0.5 across the node).
fn hit(node: &ComputedNode, t: &UiGlobalTransform, p: Vec2, grow: Vec2) -> Option<Vec2> {
    let inv = node.inverse_scale_factor();
    if inv <= 0.0 || node.size.cmple(Vec2::ZERO).any() {
        return None;
    }
    let local = t.try_inverse()?.transform_point2(p / inv);
    let half = node.size * 0.5 + grow / inv;
    (local.x.abs() <= half.x && local.y.abs() <= half.y).then(|| local / node.size)
}

/// Logical-px left edge and width of a node (for drags).
fn span(node: &ComputedNode, t: &UiGlobalTransform) -> (f32, f32) {
    let inv = node.inverse_scale_factor();
    ((t.translation.x - node.size.x * 0.5) * inv, node.size.x * inv)
}

type HitQuery<'w, 's> = Query<
    'w,
    's,
    (&'static ComputedNode, &'static UiGlobalTransform, Option<&'static MenuRow>, Option<&'static TabButton>, Option<&'static SliderHit>, Option<&'static SegOption>, Option<&'static PickArrow>),
    Or<(With<MenuRow>, With<TabButton>, With<SliderHit>, With<SegOption>, With<PickArrow>)>,
>;

/// Mouse: hover focus (arbitrated), clicks, slider drags, wheel (slider under the pointer, else
/// the list's view).
#[allow(clippy::too_many_arguments)]
pub(super) fn menu_mouse(
    mut moved: MessageReader<CursorMoved>,
    mut left: MessageReader<CursorLeft>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time<Real>>,
    state: Res<State<AppState>>,
    costume_view: Option<Res<crate::lobby::costume_view::CostumeView>>,
    hits: HitQuery,
    mut menu: ResMut<MenuState>,
    mut nav: ResMut<MenuNav>,
    mut io: MenuIo,
) {
    let dt = time.delta_secs();
    let nav = &mut *nav;
    nav.flash.1 = (nav.flash.1 - dt).max(0.0);
    nav.tick_t += dt;
    let moved_to = moved.read().last().map(|e| e.position);
    if left.read().count() > 0 {
        nav.arb.pointer_left();
    }
    if !menu_open(state.get(), &io.pause) || menu.capture.is_some() {
        if let Some(p) = moved_to {
            nav.arb.pos = Some(p);
        }
        nav.drag = None;
        nav.hover = None;
        return;
    }
    let hover_ok = moved_to.is_some_and(|p| nav.arb.pointer_moved(p));
    let in_game = *state.get() == AppState::InGame;
    let menu = &mut *menu;
    let fine = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    // Dragging a slider: follows the pointer until release (focus stays on it).
    if let Some(d) = nav.drag.as_ref() {
        if !mouse.pressed(MouseButton::Left) || menu.cursor != d.row {
            nav.drag = None;
        } else {
            if let Some(p) = moved_to {
                let f = drag_frac(d, p.x);
                let (k, map) = (d.knob, d.map.clone());
                if let Some(game) = slide_to(k, &mut io.ctx(in_game), &map, f) {
                    changed(menu, &mut io, game);
                    tick(nav, &mut io, f);
                }
            }
            return;
        }
    }

    let pos = nav.arb.pos.or_else(|| windows.single().ok().and_then(|w| w.cursor_position()));
    let Some(pos) = pos else {
        nav.hover = None;
        return;
    };
    let rows = io.rows(menu.page, menu.tab, in_game);
    // Hit test: controls first, then whole rows (grown over the gaps between them).
    let mut on: Option<Hit> = None;
    let mut on_row: Option<usize> = None;
    let mut slider_span = None;
    for (node, t, row, tab, slider, seg, arrow) in &hits {
        if let Some(r) = row {
            if hit(node, t, pos, Vec2::new(0.0, 2.5)).is_some() {
                on_row = Some(r.0);
            }
            continue;
        }
        let grow = if arrow.is_some() { Vec2::new(4.0, 6.0) } else { Vec2::new(0.0, 2.0) };
        if hit(node, t, pos, grow).is_none() {
            continue;
        }
        on = if let Some(tb) = tab {
            Some(Hit::Tab(tb.0))
        } else if let Some(s) = slider {
            slider_span = Some((span(node, t), s.knob, s.map.clone()));
            Some(Hit::Slider(s.row))
        } else if let Some(s) = seg {
            Some(Hit::Seg(s.row, s.idx))
        } else {
            arrow.map(|a| Hit::Arrow(a.row, a.dir))
        };
    }
    let target = on.or(on_row.map(Hit::Row));
    nav.hover = if nav.arb.mouse_owns() { target } else { None };

    // Hover moves focus only when the pointer moved (and owns focus).
    if hover_ok
        && let Some(r) = target.and_then(Hit::row)
        && rows.get(r).is_some_and(|x| x.selectable())
        && menu.cursor != r
    {
        menu.cursor = r;
        io.sfx.write(PlaySfx::new("ui_move").gain(0.35));
    }

    let lmb = mouse.just_pressed(MouseButton::Left);
    let rmb = mouse.just_pressed(MouseButton::Right);
    if (lmb || rmb) && target.is_some() {
        nav.arb.click();
        nav.hover = target;
    }
    if lmb && let Some(h) = target {
        if let Some(r) = h.row()
            && rows.get(r).is_some_and(|x| x.selectable())
        {
            menu.cursor = r;
        }
        match h {
            Hit::Tab(t) => {
                if menu.page == MenuPage::Settings && menu.tab != t {
                    menu.set_tab(t);
                    io.sfx.write(PlaySfx::new("ui_select"));
                }
            }
            Hit::Slider(r) => {
                if let Some(((left, width), knob, map)) = slider_span.clone() {
                    let d = Drag { row: r, knob, map: map.clone(), left, width };
                    let f = drag_frac(&d, pos.x);
                    nav.drag = Some(d);
                    if let Some(game) = slide_to(knob, &mut io.ctx(in_game), &map, f) {
                        changed(menu, &mut io, game);
                    }
                    nav.tick_t = 1.0;
                    tick(nav, &mut io, f);
                }
            }
            Hit::Seg(r, j) => {
                if let Some(s) = hits.iter().find_map(|(.., seg, _)| seg.filter(|s| s.row == r && s.idx == j)) {
                    let (k, want, cur) = (s.knob, s.text.clone(), hits.iter().filter_map(|(.., seg, _)| seg).find(|s| s.row == r && s.on).map_or(0, |s| s.idx));
                    if let Some(game) = select_text(k, &mut io.ctx(in_game), &want, if j >= cur { 1 } else { -1 }) {
                        changed(menu, &mut io, game);
                    }
                    io.sfx.write(PlaySfx::new("ui_select"));
                    nav.flash = (r, 0.18);
                }
            }
            Hit::Arrow(r, dir) => {
                if let Some(a) = hits.iter().find_map(|(.., arrow)| arrow.filter(|a| a.row == r && a.dir == dir)) {
                    if let Some(game) = nudge(a.knob, &mut io.ctx(in_game), dir, fine, 1) {
                        changed(menu, &mut io, game);
                    }
                    io.sfx.write(PlaySfx::new("ui_move"));
                }
            }
            Hit::Row(r) => {
                if let Some(&row) = rows.get(r)
                    && row.selectable()
                {
                    super::activate(row, menu, &mut io, state.get());
                    io.sfx.write(PlaySfx::new("ui_select"));
                    nav.flash = (r, 0.18);
                }
            }
        }
    } else if rmb
        && let Some(r) = target.and_then(Hit::row)
        && let Some(Row::Value(k)) = rows.get(r).copied()
    {
        // Right-click steps down.
        menu.cursor = r;
        if let Some(game) = nudge(k, &mut io.ctx(in_game), -1, fine, 1) {
            changed(menu, &mut io, game);
            io.sfx.write(PlaySfx::new("ui_move"));
        }
    }

    // Wheel: a slider under the pointer adjusts; elsewhere a long list scrolls its view.
    let dy = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if dy == 0.0 {
        nav.wheel_acc = 0.0;
        return;
    }
    nav.wheel_acc += dy;
    let steps = nav.wheel_acc.trunc() as i32;
    if steps == 0 {
        return;
    }
    nav.wheel_acc -= steps as f32;
    let wheel_on_preview = menu.page == MenuPage::Costume && costume_view.is_some_and(|v| v.pointer_on_preview);
    if let Some(Hit::Slider(r)) = on
        && let Some((_, knob, _)) = slider_span
    {
        nav.arb.click();
        menu.cursor = r;
        if let Some(game) = nudge(knob, &mut io.ctx(in_game), steps.signum(), fine, steps.unsigned_abs()) {
            changed(menu, &mut io, game);
            let f = match classify(knob, &io.ctx(in_game)) {
                Control::Slider { frac, .. } => frac,
                _ => 0.5,
            };
            tick(nav, &mut io, f);
        }
    } else if rows.len() > MAX_VISIBLE && !wheel_on_preview {
        let lo = usize::from(rows.first() == Some(&Row::Tabs));
        let sel: Vec<bool> = rows.iter().map(|r| r.selectable() && *r != Row::Tabs).collect();
        let (c, s) = nav::wheel(&sel, menu.cursor, menu.scroll, MAX_VISIBLE - lo, lo, -steps);
        if (c, s) != (menu.cursor, menu.scroll) {
            menu.cursor = c;
            menu.scroll = s;
            menu.rebuild = true;
        }
    }
}

fn drag_frac(d: &Drag, x: f32) -> f32 {
    let pad = SL_PAD / SL_W;
    (((x - d.left) / d.width.max(1.0) - pad) / (1.0 - 2.0 * pad)).clamp(0.0, 1.0)
}

/// Slider notch click, pitched by position (throttled while dragging).
fn tick(nav: &mut MenuNav, io: &mut MenuIo, f: f32) {
    if nav.tick_t >= 0.045 {
        nav.tick_t = 0.0;
        io.sfx.write(PlaySfx::new("ui_move").gain(0.5).pitch(0.85 + 0.4 * f));
    }
}

/// Back to a page: focus returns to the row that led away from it (not the top).
pub(super) fn restore_focus(mut menu: ResMut<MenuState>, mut nav: ResMut<MenuNav>) {
    let (page, depth) = (menu.page, menu.stack.len());
    if let Some((p, c, s, d)) = nav.last
        && p != page
    {
        nav.drag = None;
        if depth > d {
            nav.saved.retain(|x| x.0 != p);
            nav.saved.push((p, c, s));
        } else if depth < d
            && menu.cursor == 0
            && let Some(&(_, c2, s2)) = nav.saved.iter().find(|x| x.0 == page)
        {
            menu.cursor = c2;
            menu.scroll = s2;
            menu.rebuild = true;
        }
    }
    nav.last = Some((page, menu.cursor, menu.scroll, depth));
}

/// Focus / hover visuals of the controls (rows themselves: `style_rows`). Hover tints only show
/// while the mouse owns focus.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn style_controls(
    menu: Res<MenuState>,
    prefs: Res<UiPrefs>,
    mut nav: ResMut<MenuNav>,
    mut rows: Query<(&MenuRow, &Node, &mut BackgroundColor), (Without<SegOption>, Without<TabButton>, Without<SliderFill>, Without<SliderHandle>)>,
    mut fills: Query<(&SliderFill, &mut BackgroundColor), (Without<MenuRow>, Without<SegOption>, Without<TabButton>, Without<SliderHandle>)>,
    mut handles: Query<(&SliderHandle, &mut Node, &mut BoxShadow), Without<MenuRow>>,
    mut segs: Query<(&SegOption, &mut BackgroundColor), (Without<MenuRow>, Without<TabButton>, Without<SliderFill>, Without<SliderHandle>)>,
    mut seg_texts: Query<(&SegText, &mut TextColor), Without<PickArrow>>,
    mut arrows: Query<(&PickArrow, &mut TextColor), Without<SegText>>,
    mut tabs: Query<(&TabButton, &mut BackgroundColor), (Without<MenuRow>, Without<SegOption>, Without<SliderFill>, Without<SliderHandle>)>,
) {
    let glow = crate::console::corps_color(prefs.corps);
    let cur = menu.cursor;
    let hover = nav.hover;
    let (flash_row, flash) = nav.flash;
    let mut sel = None;
    for (row, node, mut bg) in &mut rows {
        if row.0 == cur {
            sel = Some(if let Val::Px(x) = node.padding.left { x } else { 0.0 });
        }
        if flash > 0.0 && row.0 == flash_row {
            bg.0 = glow.with_alpha(0.08 + 0.3 * flash / 0.18);
        }
    }
    nav.sel = sel.map(|pad| (menu.page, menu.tab, cur, pad));
    for (f, mut bg) in &mut fills {
        let want = glow.with_alpha(if f.0 == cur { 0.95 } else { 0.5 });
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (h, mut node, mut shadow) in &mut handles {
        let focus = h.0 == cur || hover == Some(Hit::Slider(h.0));
        let size = px(if focus { HANDLE_FOCUS } else { HANDLE });
        if node.width != size {
            let s = if focus { HANDLE_FOCUS } else { HANDLE };
            node.width = size;
            node.height = size;
            node.margin = UiRect { left: px(-s / 2.0), top: px(-s / 2.0), ..default() };
        }
        let want = if h.0 == cur { glow.with_alpha(0.7) } else { Color::NONE };
        if shadow.0.first().is_some_and(|s| s.color != want) {
            shadow.0[0].color = want;
        }
    }
    for (s, mut bg) in &mut segs {
        let hovered = hover == Some(Hit::Seg(s.row, s.idx));
        let want = match (s.on, hovered) {
            (true, _) => glow.with_alpha(if s.row == cur { 0.34 } else { 0.18 }),
            (false, true) => dim(0.14),
            (false, false) => dim(0.04),
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (s, mut c) in &mut seg_texts {
        let hovered = hover == Some(Hit::Seg(s.row, s.idx));
        let want = if s.on { Color::WHITE } else if hovered { dim(0.9) } else if s.row == cur { dim(0.6) } else { dim(0.4) };
        if c.0 != want {
            c.0 = want;
        }
    }
    for (a, mut c) in &mut arrows {
        let want = if hover == Some(Hit::Arrow(a.row, a.dir)) {
            Color::WHITE
        } else if a.row == cur {
            glow.mix(&Color::WHITE, 0.25)
        } else {
            dim(0.25)
        };
        if c.0 != want {
            c.0 = want;
        }
    }
    for (t, mut bg) in &mut tabs {
        let want = if t.0 == menu.tab {
            glow.with_alpha(if cur == 0 { 0.35 } else { 0.18 })
        } else if hover == Some(Hit::Tab(t.0)) {
            dim(0.1)
        } else {
            Color::NONE
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{page_rows, Lim};
    use super::*;
    use crate::settings::Settings;

    fn ctx<'a>(s: &'a mut Settings, p: &'a mut UiPrefs) -> Ctx<'a> {
        Ctx { settings: s, prefs: p, net: None, voice: true, in_game: false }
    }

    /// Every value row on every settings tab / rules / costume page.
    fn all_value_knobs(p: &mut UiPrefs) -> Vec<Knob> {
        let mut s = Settings::default();
        let mut out = Vec::new();
        let mut push = |rows: Vec<Row>| {
            for r in rows {
                if let Row::Value(k) = r
                    && !out.contains(&k)
                {
                    out.push(k);
                }
            }
        };
        for t in SettingsTab::ALL {
            push(page_rows(MenuPage::Settings, t, &ctx(&mut s, p)));
        }
        for page in [MenuPage::Rules, MenuPage::HostSetup, MenuPage::Customize, MenuPage::Costume] {
            push(page_rows(page, SettingsTab::Gameplay, &ctx(&mut s, p)));
        }
        let mut party = p.clone();
        party.game.mode = bd_sim::settings::GameMode::Party;
        push(page_rows(MenuPage::Rules, SettingsTab::Gameplay, &ctx(&mut s, &mut party)));
        out
    }

    fn rules_prefs() -> UiPrefs {
        let mut p = UiPrefs::default();
        p.game.rules.scoring = Scoring::Stock { lives: 3 };
        p.game.rules.scale = bd_sim::ScaleLimit::NoLimits;
        p.game.rules.powers_enabled = true;
        p
    }

    #[test]
    fn controls_match_the_setting() {
        let mut s = Settings::default();
        let mut p = rules_prefs();
        let c = ctx(&mut s, &mut p);
        let kind = |k: Knob| match classify(k, &c) {
            Control::Slider { .. } => "slider",
            Control::Toggle(_) => "toggle",
            Control::Segmented { .. } => "segmented",
            Control::Picker => "picker",
        };
        for k in [Knob::Volume, Knob::MusicVol, Knob::WorldVol, Knob::AmbienceVol, Knob::FeedbackVol, Knob::Sensitivity, Knob::Fov, Knob::AdsSens, Knob::AdsSpeed, Knob::OutlineWidth, Knob::HeadScale, Knob::Flashes, Knob::Shake, Knob::Sparks, Knob::Respawn, Knob::Shield, Knob::Knockback, Knob::TimeLimit, Knob::KoTarget, Knob::Lives, Knob::Limit(Lim::Structures), Knob::Limit(Lim::Total)] {
            assert_eq!(kind(k), "slider", "{k:?}");
        }
        for k in [Knob::InvertY, Knob::Vsync, Knob::Fullscreen, Knob::Mono, Knob::Comfort, Knob::Powers, Knob::VoidKo, Knob::RandomLook, Knob::Outlines] {
            assert_eq!(kind(k), "toggle", "{k:?}");
        }
        for k in [Knob::ThirdPerson, Knob::Shoulder, Knob::AdsMode, Knob::Ono, Knob::Shout, Knob::ReduceFlashes, Knob::EnemyPercent, Knob::Mode, Knob::Scale, Knob::Willpower, Knob::Scoring, Knob::Caps, Knob::Limit(Lim::Held)] {
            assert_eq!(kind(k), "segmented", "{k:?}");
        }
        for k in [Knob::Map, Knob::Corps, Knob::Accent] {
            assert_eq!(kind(k), "picker", "{k:?}");
        }
    }

    #[test]
    fn segmented_order_is_stable_and_every_option_selects() {
        let mut s = Settings::default();
        let mut p = rules_prefs();
        let knobs = all_value_knobs(&mut p);
        for k in knobs {
            let Control::Segmented { opts, .. } = classify(k, &ctx(&mut s, &mut p)) else { continue };
            for (j, o) in opts.iter().enumerate() {
                if o == "CUSTOM" {
                    continue; // a stand-in, not an option
                }
                let mut c = ctx(&mut s, &mut p);
                select_text(k, &mut c, o, 1);
                let Control::Segmented { opts: now, idx } = classify(k, &c) else { panic!("{k:?} stopped being segmented at {o}") };
                assert_eq!(knob_text(k, &c).1, *o, "{k:?}");
                // The chosen one is lit; the named options keep their order.
                assert_eq!(now[idx], *o, "{k:?}");
                assert_eq!(now.get(j), Some(o), "{k:?}");
                let named = |v: &Vec<String>| v.iter().filter(|x| *x != "CUSTOM").cloned().collect::<Vec<_>>();
                assert_eq!(named(&now), named(&opts), "{k:?}: order moved when picking {o}");
            }
        }
    }

    #[test]
    fn sliders_reach_exactly_the_existing_clamps() {
        let mut s = Settings::default();
        let mut p = rules_prefs();
        let knobs = all_value_knobs(&mut p);
        let mut n = 0;
        for k in knobs {
            let Control::Slider { map, .. } = classify(k, &ctx(&mut s, &mut p)) else { continue };
            n += 1;
            let mut c = ctx(&mut s, &mut p);
            slide_to(k, &mut c, &map, 1.0);
            let top = knob_text(k, &c).1;
            // One more step up changes nothing: the slider's end is the stepper's clamp.
            step_knob(k, &mut c, 1, false);
            assert_eq!(knob_text(k, &c).1, top, "{k:?} slider max below the clamp");
            slide_to(k, &mut c, &map, 0.0);
            let bottom = knob_text(k, &c).1;
            step_knob(k, &mut c, -1, false);
            assert_eq!(knob_text(k, &c).1, bottom, "{k:?} slider min above the clamp");
            if let Some(sp) = spec(k, &c) {
                assert!((sp.v - sp.lo).abs() < 1e-4, "{k:?} bottom {} vs {}", sp.v, sp.lo);
            }
            let Control::Slider { frac, .. } = classify(k, &c) else { unreachable!() };
            assert!(frac.abs() < 1e-4, "{k:?} {frac}");
            // Midway lands on a real step between the ends.
            slide_to(k, &mut c, &map, 0.5);
            let Control::Slider { frac, .. } = classify(k, &c) else { unreachable!() };
            assert!((0.2..=0.8).contains(&frac), "{k:?} mid {frac}");
        }
        assert!(n >= 25, "only {n} sliders");
    }

    #[test]
    fn seeking_hits_the_step_lattice() {
        let mut s = Settings::default();
        let mut p = UiPrefs::default();
        let mut c = ctx(&mut s, &mut p);
        seek(Knob::Volume, &mut c, 0.33);
        assert!((c.settings.master_volume - 0.35).abs() < 1e-4, "{}", c.settings.master_volume);
        seek(Knob::Sensitivity, &mut c, 2.37);
        assert!((c.settings.sensitivity - 2.37).abs() < 1e-4, "fine steps: {}", c.settings.sensitivity);
        seek(Knob::Fov, &mut c, 101.4);
        assert_eq!(c.settings.fov, 101.0);
        assert_eq!(seek(Knob::Fov, &mut c, 101.2), None, "no change reported");
    }

    #[test]
    fn keys_move_toggles_and_segments_directionally() {
        let mut s = Settings::default();
        let mut p = UiPrefs::default();
        let mut c = ctx(&mut s, &mut p);
        c.settings.vsync = false;
        assert_eq!(nudge(Knob::Vsync, &mut c, -1, false, 1), None, "← on OFF stays off");
        assert!(nudge(Knob::Vsync, &mut c, 1, false, 1).is_some());
        assert!(c.settings.vsync);
        assert_eq!(nudge(Knob::Vsync, &mut c, 1, false, 1), None, "→ on ON stays on");
        // Segmented: stop at the ends, no wrap.
        let Control::Segmented { opts, .. } = classify(Knob::Ono, &c) else { panic!() };
        select_text(Knob::Ono, &mut c, &opts[0], 1);
        assert_eq!(nudge(Knob::Ono, &mut c, -1, false, 1), None);
        for _ in 0..opts.len() + 2 {
            nudge(Knob::Ono, &mut c, 1, false, 1);
        }
        assert_eq!(knob_text(Knob::Ono, &c).1, *opts.last().unwrap());
        // Enter on a slider only focuses it.
        let v = c.settings.master_volume;
        assert_eq!(activate_value(Knob::Volume, &mut c), None);
        assert_eq!(c.settings.master_volume, v);
        // Held keys step `times`.
        c.settings.master_volume = 0.5;
        nudge(Knob::Volume, &mut c, 1, false, 3);
        assert!((c.settings.master_volume - 0.65).abs() < 1e-4);
    }

    #[test]
    fn game_knobs_still_report_host_sync() {
        let mut s = Settings::default();
        let mut p = rules_prefs();
        let mut c = ctx(&mut s, &mut p);
        assert_eq!(seek(Knob::Respawn, &mut c, 7.0), Some(true), "lobby rule → host re-sends");
        assert_eq!(c.prefs.game.rules.respawn_secs, 7.0);
        assert_eq!(seek(Knob::Volume, &mut c, 0.1), Some(false), "local setting");
        let Control::Segmented { opts, .. } = classify(Knob::Mode, &c) else { panic!() };
        let other = opts.iter().find(|o| **o != knob_text(Knob::Mode, &c).1).unwrap().clone();
        assert_eq!(select_text(Knob::Mode, &mut c, &other, 1), Some(true));
    }

    #[test]
    fn drag_maps_the_track_not_the_padding() {
        let d = Drag { row: 0, knob: Knob::Volume, map: SliderMap::Value { lo: 0.0, hi: 1.0, curve: Curve::Lin }, left: 100.0, width: SL_W };
        assert_eq!(drag_frac(&d, 100.0), 0.0);
        assert_eq!(drag_frac(&d, 100.0 + SL_PAD), 0.0);
        assert!((drag_frac(&d, 100.0 + SL_W / 2.0) - 0.5).abs() < 1e-4);
        assert_eq!(drag_frac(&d, 100.0 + SL_W - SL_PAD), 1.0);
        assert_eq!(drag_frac(&d, 9999.0), 1.0);
    }
}
