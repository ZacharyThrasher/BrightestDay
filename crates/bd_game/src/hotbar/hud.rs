//! HOTBAR on the power strip (wordless) + the Tab editor.
//!
//! * **Slots** sit in the strip's icon row right of your held powers: a disc in the move's element
//!   colour with its STYLE glyph (`move_fx.ron`), a clockwise cooldown sweep (the host's word:
//!   `ToolboxSnap` for borrowed / kit / described / twisted moves, the power gem for a held move),
//!   white flares when it fires / is bound / is ready again, greyed while you can't afford it, whole
//!   SPARK pips under it (its price now, staleness included), its key's digit — the only character.
//!   A pinned slot wears a small gold stud; a slot whose thing is still out (a construct you called)
//!   burns an outer ring. Constructs / signatures in a slot show the ring glyph / a star in your
//!   corps colour. Empty slots are faint rings. The old borrowed-move chips fold into this (one bar).
//! * **Tab** (the scoreboard key, held): NO always-on names — a tray of recent moves + your held
//!   powers' moves shows beside the bar, the cursor freed (keyboard play goes on:
//!   `input::SoftCursor`). Hovering one slot / tray chip pops its compact tooltip (owner: "only have
//!   a tooltip come up when they hover over the actual ability" — `power_hud::labels`, which reuses
//!   this module's freed cursor + hit-testing). Drag slot → slot swaps, slot → off the bar clears,
//!   tray → slot binds; click-click swaps, a double click / right-click pins. Keyboard: Tab + slot
//!   key selects, another slot key swaps, the same one pins, Delete clears.
//!
//! Nodes are spawned once; a frame writes a material / text only when its quantised value changed.

use super::fire::{held_sel, phrase_of, present};
use super::{Entry, Hotbar, SlotBind, SlotState, SLOTS};
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::power_hud::derive::glyph;
use crate::power_hud::strip::{IconMaterial, IconParams, StripRow};
use crate::power_hud::PowerHudState;
use crate::present::powers::movefx;
use crate::sim_bridge::{ActiveWorld, ViewKeys};
use crate::state::{AppState, InGameMenu};
use bd_sim::powers::chain::ToolboxSnap;
use bd_sim::powers::toolbox::MoveRef;
use bd_sim::powers::{MoveDef, MoveOp, PlayerPower};
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

const ICON: f32 = 32.0;
const CHIP: f32 = 26.0;
const TRAY: usize = 8;
const PIPS: usize = 6;
const PIP: f32 = 5.0;
/// Past this many (logical) px a press is a drag, not a click.
const DRAG_PX: f32 = 7.0;
/// Two clicks on one slot within this long pin it.
const DOUBLE_CLICK: f32 = 0.35;

// ------------------------------------------------------------------------------------------ editing

/// Where a drag started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Src {
    Slot(usize),
    Tray(usize),
}

/// What the cursor is over when a drag ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over {
    Slot(usize),
    Tray(usize),
    Nothing,
}

/// The Tab editor's state (other systems read `on`: slot keys edit instead of fire).
#[derive(Resource, Default, Debug)]
pub struct TabEdit {
    pub on: bool,
    /// Keyboard / click selection.
    pub sel: Option<usize>,
    /// A drag in progress: where from, the press point, and whether it moved past `DRAG_PX`.
    pub drag: Option<(Src, Vec2, bool)>,
    /// Cursor (logical px) while editing.
    pub cursor: Option<Vec2>,
    pub hover: Option<Over>,
    /// The tray (recent moves not on the bar, then held powers' moves), rebuilt while editing.
    pub tray: Vec<Entry>,
    last_click: Option<(usize, f32)>,
    /// Dev / screenshots: hold Tab and drive the cursor.
    pub dev: Option<DevTab>,
}

/// Scripted Tab (the hotbar scenario): the editor shows, a drag is drawn at `cursor`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DevTab {
    pub cursor: Option<Vec2>,
    /// Put the cursor on this node's centre instead (drag demos).
    pub target: Option<Over>,
    pub drag: Option<Src>,
}

/// A drop (pure; the tests drive it). `moved` = the press travelled (a click never clears a slot).
pub fn drop_on(bar: &mut Hotbar, tray: &[Entry], src: Src, over: Over, moved: bool) {
    match (src, over) {
        (Src::Slot(i), Over::Slot(k)) if i != k => bar.swap(i, k),
        (Src::Slot(i), Over::Nothing | Over::Tray(_)) if moved => bar.clear(i),
        (Src::Tray(j), Over::Slot(k)) => {
            if let Some(e) = tray.get(j) {
                bar.place(k, e);
            }
        }
        _ => {}
    }
}

/// Tab + slot key (or a click): select; another slot swaps; the same one again pins.
pub fn key_edit(bar: &mut Hotbar, sel: &mut Option<usize>, k: usize) {
    match *sel {
        None => *sel = Some(k),
        Some(s) if s == k => {
            bar.toggle_pin(k);
            *sel = None;
        }
        Some(s) => {
            bar.swap(s, k);
            *sel = None;
        }
    }
}

/// Your held powers' moves as tray entries (the phrase each is called by).
fn held_entries(active: Option<&ActiveWorld>) -> Vec<Entry> {
    let mut out = Vec::new();
    let Some(p) = active.and_then(|a| a.local_player()) else { return out };
    for h in p.power.held() {
        let has_own = h.def.tier(h.tier).is_some_and(|t| !t.moves.is_empty());
        for (k, m) in h.def.moves_at(h.tier).iter().enumerate().take(4) {
            if m.ops.iter().all(|o| matches!(o, MoveOp::Transform | MoveOp::Tier(_))) {
                continue;
            }
            let mv = MoveRef::new(h.id, if has_own { h.tier } else { 0 }, k as u8);
            let Some(phrase) = phrase_of(mv) else { continue };
            let bind = SlotBind::Move { mv, kit: false };
            let (def, power) = present(&bind);
            out.push(Entry { def, power, ..Entry::new(&phrase, bind) });
        }
    }
    out
}

fn tab_test() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("BD_TAB_TEST").ok().and_then(|v| v.parse::<f32>().ok()).is_some_and(|v| v > 0.0))
}

/// Tab held → edit mode (soft cursor), mouse drag-drop + keyboard fallback.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tab_edit(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    focus: Res<crate::input::UiFocus>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    prefs: Res<UiPrefs>,
    active: Option<Res<ActiveWorld>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    nodes: Option<Res<Nodes>>,
    hits: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut soft: ResMut<crate::input::SoftCursor>,
    mut edit: ResMut<TabEdit>,
    mut bar: ResMut<Hotbar>,
) {
    let alive = active.as_ref().and_then(|a| a.local_player()).is_some_and(|p| p.alive);
    let in_game = *state.get() == AppState::InGame && !menu.0 && alive && prefs.power_hud;
    let dev = edit.dev;
    let tab = dev.is_some() || (!focus.typing && binds.pressed(Action::Scoreboard, &keys, &mouse)) || tab_test();
    let on = in_game && tab && nodes.is_some();
    if on != edit.on {
        edit.on = on;
        edit.sel = None;
        edit.drag = None;
        edit.hover = None;
        edit.last_click = None;
        if on {
            let extra = held_entries(active.as_deref());
            edit.tray = bar.tray(&extra, TRAY);
        }
    }
    // Scripted Tab keeps the real cursor locked (screenshots / tests).
    let want_soft = on && dev.is_none() && !tab_test();
    if soft.0 != want_soft {
        soft.0 = want_soft;
    }
    if !on {
        edit.cursor = None;
        return;
    }
    let Some(n) = nodes else { return };
    // cursor → what it is over (physical px for the hit test, logical for the ghost)
    let scale = window.as_ref().map_or(1.0, |w| w.scale_factor());
    let dev_at = dev.and_then(|d| d.target).and_then(|o| {
        let e = match o {
            Over::Slot(k) => n.slots.get(k).map(|s| s.icon),
            Over::Tray(j) => n.tray.get(j).map(|s| s.icon),
            Over::Nothing => None,
        }?;
        hits.get(e).ok().map(|(_, t)| t.affine().translation / scale)
    });
    let (cursor, phys) = match dev_at.or(dev.and_then(|d| d.cursor)) {
        Some(c) => (Some(c), Some(c * scale)),
        None => (window.as_ref().and_then(|w| w.cursor_position()), window.as_ref().and_then(|w| w.physical_cursor_position())),
    };
    edit.cursor = cursor;
    let hit = |e: Entity| phys.is_some_and(|p| hits.get(e).is_ok_and(|(c, t)| c.contains_point(*t, p)));
    let over = match (0..SLOTS).find(|&k| hit(n.slots[k].icon)) {
        Some(k) => Over::Slot(k),
        None => match (0..edit.tray.len().min(TRAY)).find(|&j| hit(n.tray[j].icon)) {
            Some(j) => Over::Tray(j),
            None => Over::Nothing,
        },
    };
    edit.hover = Some(over);
    if let Some(d) = dev {
        edit.drag = d.drag.map(|s| (s, cursor.unwrap_or_default(), true));
        return;
    }
    let now = time.elapsed_secs();
    // keyboard fallback: Tab + slot keys / Delete
    for (k, a) in [Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4].into_iter().enumerate() {
        if binds.just_pressed(a, &keys, &mouse) {
            let mut sel = edit.sel;
            key_edit(&mut bar, &mut sel, k);
            edit.sel = sel;
        }
    }
    if keys.any_just_pressed([KeyCode::Delete, KeyCode::Backspace])
        && let Some(s) = edit.sel.take()
    {
        bar.clear(s);
    }
    // mouse
    if mouse.just_pressed(MouseButton::Right)
        && let Over::Slot(k) = over
    {
        bar.toggle_pin(k);
    }
    if mouse.just_pressed(MouseButton::Left) {
        edit.drag = match over {
            Over::Slot(k) if !bar.slots[k].is_empty() => Some((Src::Slot(k), cursor.unwrap_or_default(), false)),
            Over::Slot(k) => {
                // an empty slot: a click completes a selection swap
                if edit.sel.is_some() {
                    let mut sel = edit.sel;
                    key_edit(&mut bar, &mut sel, k);
                    edit.sel = sel;
                }
                None
            }
            Over::Tray(j) => Some((Src::Tray(j), cursor.unwrap_or_default(), false)),
            Over::Nothing => {
                edit.sel = None;
                None
            }
        };
    }
    if let (Some((src, start, moved)), Some(c)) = (edit.drag, cursor)
        && !moved
        && c.distance(start) > DRAG_PX
    {
        edit.drag = Some((src, start, true));
    }
    if mouse.just_released(MouseButton::Left)
        && let Some((src, _, moved)) = edit.drag.take()
    {
        if moved {
            let tray = std::mem::take(&mut edit.tray);
            drop_on(&mut bar, &tray, src, over, true);
            let extra = held_entries(active.as_deref());
            edit.tray = bar.tray(&extra, TRAY);
            edit.sel = None;
        } else if let Src::Slot(k) = src {
            // a click: select / swap / (double click) pin
            let double = edit.last_click.is_some_and(|(j, t)| j == k && now - t < DOUBLE_CLICK);
            if double {
                edit.sel = None;
                bar.toggle_pin(k);
                edit.last_click = None;
            } else {
                let mut sel = edit.sel;
                if sel == Some(k) {
                    sel = None; // a second single click on the same slot just deselects
                } else {
                    key_edit(&mut bar, &mut sel, k);
                }
                edit.sel = sel;
                edit.last_click = Some((k, now));
            }
        }
    }
}

// ------------------------------------------------------------------------------------------ nodes

#[derive(Clone)]
pub(crate) struct SlotNodes {
    holder: Entity,
    icon: Entity,
    mat: Handle<IconMaterial>,
    digit: Entity,
    pin: Entity,
    pips: [Entity; PIPS],
}

/// TOTAL LIMIT pips (the lobby offers up to 8).
const LIMIT_PIPS: usize = 8;

#[derive(Resource)]
pub(crate) struct Nodes {
    root: Entity,
    slots: [SlotNodes; SLOTS],
    /// TOTAL LIMIT: one small bar per allowed thing (filled = out), + the dismiss key's cap.
    limit_row: Entity,
    limit: [Entity; LIMIT_PIPS],
    limit_key: Entity,
    tray_root: Entity,
    tray: [SlotNodes; TRAY],
    ghost: (Entity, Handle<IconMaterial>),
}

pub fn build(app: &mut App) {
    app.init_resource::<TabEdit>()
        .init_resource::<Cache>()
        .add_systems(Update, tab_edit.in_set(crate::lobby::UiSet::Input).before(super::fire::slot_keys))
        .add_systems(Update, (attach, draw).chain().in_set(crate::present::PresentSet::Sync));
}

fn mk_mat(mats: &mut Assets<IconMaterial>, seed: f32) -> Handle<IconMaterial> {
    mats.add(IconMaterial { params: IconParams { b: Vec4::new(0.0, 0.0, 0.0, 0.0), c: Vec4::new(1.0, 1.0, 1.0, seed), ..default() } })
}

fn spawn_slot(commands: &mut Commands, mats: &mut Assets<IconMaterial>, size: f32, seed: f32, digit: bool) -> SlotNodes {
    let mat = mk_mat(mats, seed);
    let icon = commands.spawn((Node { width: px(size), height: px(size), flex_shrink: 0.0, ..default() }, MaterialNode(mat.clone()), Pickable::IGNORE)).id();
    let digit = commands
        .spawn((
            Node { position_type: PositionType::Absolute, right: px(-3), bottom: px(-4), display: if digit { Display::Flex } else { Display::None }, ..default() },
            Text::new(""),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
            TextShadow { offset: Vec2::new(1.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.9) },
            Pickable::IGNORE,
        ))
        .id();
    let pin = commands
        .spawn((
            Node { position_type: PositionType::Absolute, right: px(0), top: px(0), width: px(6), height: px(6), border_radius: BorderRadius::MAX, display: Display::None, ..default() },
            BackgroundColor(Color::srgb(1.0, 0.82, 0.3)),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(icon).add_children(&[digit, pin]);
    let pips = std::array::from_fn(|_| {
        commands.spawn((Node { width: px(PIP), height: px(PIP), border_radius: BorderRadius::MAX, display: Display::None, ..default() }, BackgroundColor(Color::WHITE), Pickable::IGNORE)).id()
    });
    let pip_row = commands.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(2), height: px(PIP), margin: UiRect::top(px(2)), ..default() }, Pickable::IGNORE)).id();
    commands.entity(pip_row).add_children(&pips);
    let holder = commands
        .spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, Pickable::IGNORE))
        .id();
    commands.entity(holder).add_children(&[icon, pip_row]);
    SlotNodes { holder, icon, mat, digit, pin, pips }
}

/// Spawn the bar once, into the strip's icon row (after the held powers / pickup).
fn attach(mut commands: Commands, rows: Query<Entity, Added<StripRow>>, mut mats: ResMut<Assets<IconMaterial>>) {
    for row in &rows {
        let root = commands
            .spawn((Name::new("Hotbar"), Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexStart, column_gap: px(6), margin: UiRect::left(px(8)), ..default() }, Pickable::IGNORE))
            .id();
        let slots: [SlotNodes; SLOTS] = std::array::from_fn(|k| spawn_slot(&mut commands, &mut mats, ICON, 5.3 + k as f32, true));
        for s in &slots {
            commands.entity(root).add_child(s.holder);
        }
        let limit: [Entity; LIMIT_PIPS] = std::array::from_fn(|_| {
            commands.spawn((Node { width: px(4), height: px(10), border_radius: BorderRadius::all(px(2)), display: Display::None, ..default() }, BackgroundColor(Color::WHITE), Pickable::IGNORE)).id()
        });
        let limit_key = commands
            .spawn((
                Node { display: Display::None, margin: UiRect::left(px(3)), ..default() },
                Text::new(""),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(1.0, 0.8, 0.45)),
                TextShadow { offset: Vec2::new(1.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.9) },
                Pickable::IGNORE,
            ))
            .id();
        let limit_row = commands
            .spawn((Name::new("HotbarLimit"), Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(2), margin: UiRect { left: px(4), top: px(11), ..default() }, display: Display::None, ..default() }, Pickable::IGNORE))
            .id();
        commands.entity(limit_row).add_children(&limit);
        commands.entity(limit_row).add_child(limit_key);
        commands.entity(root).add_child(limit_row);
        let tray_root = commands
            .spawn((
                Name::new("HotbarTray"),
                Node { position_type: PositionType::Absolute, left: percent(100), top: px(3), margin: UiRect::left(px(14)), flex_direction: FlexDirection::Row, align_items: AlignItems::FlexStart, column_gap: px(6), display: Display::None, ..default() },
                Pickable::IGNORE,
            ))
            .id();
        let tray: [SlotNodes; TRAY] = std::array::from_fn(|k| spawn_slot(&mut commands, &mut mats, CHIP, 9.1 + k as f32, false));
        for s in &tray {
            commands.entity(tray_root).add_child(s.holder);
        }
        commands.entity(root).add_child(tray_root);
        commands.entity(row).add_child(root);
        let gm = mk_mat(&mut mats, 0.0);
        let ghost = commands
            .spawn((Name::new("HotbarGhost"), Node { position_type: PositionType::Absolute, width: px(ICON), height: px(ICON), display: Display::None, ..default() }, MaterialNode(gm.clone()), GlobalZIndex(95), Pickable::IGNORE))
            .id();
        commands.insert_resource(Nodes { root, slots, limit_row, limit, limit_key, tray_root, tray, ghost: (ghost, gm) });
    }
}

// ------------------------------------------------------------------------------------------ drawing

/// Per-slot values recomputed a few times a second (spark prices need the lexicon / world).
#[derive(Resource, Default)]
struct Cache {
    since: f32,
    labels: [String; SLOTS],
    sparks: [u32; SLOTS],
    fill_prev: [f32; SLOTS],
}

/// The look of a bind: (colour, glyph).
fn look_of(e_def: Option<&'static bd_sim::powers::MoveDef>, power: Option<&'static bd_sim::powers::PowerDef>, bind: &SlotBind, table: Option<&movefx::table::MoveFx>, corps: [f32; 3]) -> ([f32; 3], u8) {
    // a power's name (it grants the power): the power's own colour, a hexagon
    if let (SlotBind::Phrase { .. }, Some(p)) = (bind, power) {
        return (crate::power_hud::derive::vivid(crate::power_hud::derive::identity_color(p)), glyph::HEXAGON);
    }
    match (bind, e_def, table) {
        (SlotBind::Empty, ..) => ([0.55, 0.58, 0.66], glyph::CIRCLE),
        (SlotBind::Phrase { signature }, ..) => (corps, if *signature { glyph::STAR4 } else { glyph::RING }),
        (_, Some(mv), Some(t)) => {
            let look = movefx::resolve_look(t, mv, power, corps);
            (crate::power_hud::derive::vivid(look.main), t.def(look.style).glyph)
        }
        _ => (corps, glyph::DIAMOND),
    }
}

/// FIRST CALL / BURN ON DEATH (bd_sim `first_call`, host rules): how the slot's thing comes out for
/// you now, read from the replicated claims / burns (the host gates the key's command exactly like
/// the phrase, so this is presentation only).
fn slot_state(s: &super::Slot, a: &ActiveWorld, lex: Option<&crate::console::RingLexicon>) -> SlotState {
    use bd_sim::first_call::{gate, keys};
    let r = a.world.rules();
    let (fc, burn) = (r.first_call, r.burn.on());
    let Some(me) = a.local.filter(|_| fc || burn) else { return SlotState::Fresh };
    let of_twist = |t: &bd_sim::powers::twist::TwistStep| match &t.a.base {
        bd_sim::powers::twist::TwistBase::Move { mv, .. } => gate::move_ident(*mv).map(|i| i.key),
        bd_sim::powers::twist::TwistBase::Composed(c) => Some(keys::composed_key(&bd_sim::powers::twist::split_composed(*c).0)),
        _ => None,
    };
    let key = match &s.bind {
        SlotBind::Empty => None,
        SlotBind::Move { mv, .. } => gate::move_ident(*mv).map(|i| i.key),
        SlotBind::Composed(c) => Some(keys::composed_key(c)),
        SlotBind::Twist(t) => of_twist(t),
        SlotBind::Phrase { signature } => lex.and_then(|l| {
            let res = l.0.resolve(&s.phrase);
            gate::recipe_ident(&res.recipe, *signature || res.signature.is_some()).map(|i| i.key)
        }),
    };
    let Some(key) = key else { return SlotState::Fresh };
    let v = a.world.first_call.snap.verdict(me, key, fc, burn);
    if v.rival.is_some() {
        SlotState::Claimed
    } else if v.echo {
        SlotState::Burned
    } else {
        SlotState::Fresh
    }
}

/// Whole sparks `bind` (bound as `phrase`, presenting as `def`) costs now (0 = free / unknown).
/// Shared by a bar slot and a tray entry alike (the tooltip prices a tray entry the same way).
pub(crate) fn bind_sparks(bind: &SlotBind, phrase: &str, def: Option<&'static MoveDef>, a: &ActiveWorld, lex: Option<&crate::console::RingLexicon>, uses: u32) -> u32 {
    let w = &a.world;
    if w.rules().infinite_willpower() {
        return 0;
    }
    let bar = w.cfg.rules.pool_segments();
    let t = &w.rules().sparks(&w.cfg.tuning.willpower.sparks);
    let seg = w.rules().segment_size().max(1e-3);
    match (bind, def) {
        (SlotBind::Move { .. } | SlotBind::Composed(_) | SlotBind::Twist(_), Some(mv)) => bd_sim::powers::chain::move_sparks(t, mv, uses, bar),
        (SlotBind::Phrase { signature }, _) => {
            let (Some(lex), Some(me)) = (lex, a.local) else { return 0 };
            let res = lex.0.resolve(phrase);
            let cost = if *signature || res.signature.is_some() { w.preview_signature_cost(me, res.recipe).1 } else { w.preview_cost(me, res.recipe).1 };
            (cost / seg).ceil().max(0.0) as u32
        }
        _ => 0,
    }
}

/// Cooldown fill (0 fresh … 1 ready) + repeat count for `bind`: a move fired while its power is
/// HELD reads the power gem (never a second, borrowed cooldown to double-dip), everything else the
/// host's toolbox row. Shared by the bar (`draw`) and the hover tooltip (`power_hud::labels`).
pub(crate) fn bind_fill(bind: &SlotBind, hud: &PowerHudState, held: Option<&PlayerPower>, tb: Option<ToolboxSnap>) -> (f32, u32) {
    let held_mv = bind.move_ref().filter(|m| held.is_some_and(|p| held_sel(p, *m).is_some()));
    match (held_mv, bind.cd_key(), tb) {
        (Some(m), _, _) => (hud.0.gems.iter().find(|g| g.power == Some(m.power)).map_or(1.0, |g| g.shown), 0),
        (None, Some(key), Some(tb)) => tb.cds.iter().find(|c| c.0 == key).map_or((1.0, 0), |c| (1.0 - c.1 as f32 / 255.0, c.2 as u32)),
        _ => (1.0, 0),
    }
}

fn q(x: f32) -> f32 {
    (x * 96.0).round() / 96.0
}

#[allow(clippy::too_many_arguments)]
fn draw(
    time: Res<Time<Real>>,
    mut bar: ResMut<Hotbar>,
    edit: Res<TabEdit>,
    nodes: Option<Res<Nodes>>,
    (prefs, binds, state, menu, rig): (Res<UiPrefs>, Res<KeyBinds>, Res<State<AppState>>, Res<InGameMenu>, Res<crate::camera::CameraRig>),
    (active, keys, hud, lex, table, dismiss): (
        Option<Res<ActiveWorld>>,
        Res<ViewKeys>,
        Res<PowerHudState>,
        Option<Res<crate::console::RingLexicon>>,
        Option<Res<movefx::table::MoveFx>>,
        Option<Res<super::dismiss::DismissWindow>>,
    ),
    mut cache: ResMut<Cache>,
    mut mats: ResMut<Assets<IconMaterial>>,
    mut q_node: Query<&mut Node>,
    mut q_text: Query<(&mut Text, &mut TextColor)>,
    mut q_bg: Query<&mut BackgroundColor>,
) {
    let Some(n) = nodes else { return };
    let dt = time.delta_secs().min(0.1);
    let t_now = time.elapsed_secs();
    let local = active.as_ref().and_then(|a| a.local_player());
    let alive = local.is_some_and(|p| p.alive);
    let show = prefs.power_hud && *state.get() == AppState::InGame && !menu.0 && alive && *rig != crate::camera::CameraRig::Director;
    fn disp(q_node: &mut Query<&mut Node>, e: Entity, on: bool) {
        if let Ok(mut nd) = q_node.get_mut(e) {
            let d = if on { Display::Flex } else { Display::None };
            if nd.display != d {
                nd.display = d;
            }
        }
    }
    disp(&mut q_node, n.root, show);
    disp(&mut q_node, n.tray_root, show && edit.on);
    if !show {
        disp(&mut q_node, n.ghost.0, false);
        return;
    }
    let Some(a) = active.as_deref() else { return };
    let corps = local.map_or([0.3, 1.0, 0.45], |p| p.corps.color());
    let tb = keys.curr.own.as_ref().and_then(|o| o.toolbox).filter(|_| keys.valid);
    let held = local.map(|p| &p.power);
    let willpower = local.map_or(0.0, |p| p.willpower);
    let seg = a.world.rules().segment_size().max(1e-3);
    // spark prices: ~5 Hz
    cache.since += dt;
    let refresh = cache.since >= 0.2 || cache.labels[0].is_empty();
    if refresh {
        cache.since = 0.0;
    }
    let edit_on = edit.on;
    let dragging_slot = match edit.drag {
        Some((Src::Slot(k), _, true)) => Some(k),
        _ => None,
    };
    let dismiss_on = dismiss.as_ref().is_some_and(|d| d.open());
    for k in 0..SLOTS {
        let nd = &n.slots[k];
        // cooldown fill: held → the power's gem; else the host's toolbox row
        let (fill, uses, empty) = {
            let s = &bar.slots[k];
            let (f, u) = bind_fill(&s.bind, &hud, held, tb);
            (f, u, s.is_empty())
        };
        if refresh {
            let (sp, st) = {
                let s = &bar.slots[k];
                (if empty { 0 } else { bind_sparks(&s.bind, &s.phrase, s.def, a, lex.as_deref(), uses) }, slot_state(s, a, lex.as_deref()))
            };
            cache.sparks[k] = sp;
            if bar.slots[k].state != st {
                bar.slots[k].state = st;
            }
            // the slot key's own label (rebindable)
            let label: String = binds.get([Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4][k]).label().chars().take(2).collect();
            if cache.labels[k] != label {
                cache.labels[k] = label;
            }
        }
        if fill >= 0.999 && cache.fill_prev[k] < 0.999 && !empty {
            bar.flash[k] = bar.flash[k].max(0.8); // ready again
        }
        cache.fill_prev[k] = fill;
        let s = &bar.slots[k];
        let sparks = cache.sparks[k];
        let afford = sparks == 0 || willpower + 1e-3 >= sparks as f32 * seg;
        let (mut color, gl) = look_of(s.def, s.power, &s.bind, table.as_deref(), corps);
        let state_dim = s.state == SlotState::Burned;
        if s.state == SlotState::Claimed {
            // someone called it first: your rival version (pulled toward red, still bright)
            color = [color[0] * 0.5 + 0.5, color[1] * 0.35, color[2] * 0.35];
        }
        let live = !s.live.is_empty() && s.live.iter().any(|c| a.world.constructs.get(*c).is_some());
        let sel = edit_on && edit.sel == Some(k);
        let hover = edit_on && edit.hover == Some(Over::Slot(k)) && edit.drag.is_some();
        let pulse = if (dismiss_on || bar.refused > 0.0) && live { 0.5 + 0.5 * (t_now * 10.0).sin() } else { 0.0 };
        let flash = bar.flash[k].max(bar.bound[k]).max(if sel || hover { 0.55 } else { 0.0 }).max(pulse);
        let alpha = if s.is_empty() { if edit_on { 0.55 } else { 0.28 } } else if dragging_slot == Some(k) { 0.35 } else { 1.0 };
        let p = IconParams {
            color: Vec4::new(color[0], color[1], color[2], alpha),
            a: Vec4::new(gl as f32, q(if s.is_empty() { 0.0 } else { fill }), if live { 1.0 } else { -1.0 }, q(flash)),
            b: Vec4::new(0.0, sel as u8 as f32, (!afford || state_dim) as u8 as f32, 0.0),
            c: Vec4::new(color[0], color[1], color[2], 5.3 + k as f32),
        };
        if mats.get(&nd.mat).is_some_and(|m| m.params != p)
            && let Some(mut m) = mats.get_mut(&nd.mat)
        {
            m.params = p;
        }
        // digit: the slot key's own label (rebindable)
        if let Ok((mut t, mut c)) = q_text.get_mut(nd.digit) {
            if t.0 != cache.labels[k] {
                t.0.clone_from(&cache.labels[k]);
            }
            let want = Color::srgba(1.0, 1.0, 1.0, if s.is_empty() { 0.4 } else { 0.9 });
            if c.0 != want {
                c.0 = want;
            }
        }
        disp(&mut q_node, nd.pin, s.pinned);
        for (j, &e) in nd.pips.iter().enumerate() {
            let on = j < sparks as usize;
            disp(&mut q_node, e, on);
            if on && let Ok(mut bg) = q_bg.get_mut(e) {
                let want = if afford { Color::srgba(1.0, 0.93, 0.7, 0.9) } else { Color::srgba(1.0, 0.4, 0.25, 0.75) };
                if bg.0 != want {
                    bg.0 = want;
                }
            }
        }
    }
    // TOTAL LIMIT: pips (filled = things out) while the rule is on; on a refusal (or while the
    // dismiss window is open) they pulse warm and the dismiss key's cap shows beside them
    let cap = a.world.active_limit().map_or(0, |c| c as usize).min(LIMIT_PIPS);
    let count = a.local.map_or(0, |id| a.world.active_count(id) as usize);
    disp(&mut q_node, n.limit_row, cap > 0);
    let hot = bar.refused > 0.0 || dismiss_on;
    let throb = if hot { 0.55 + 0.45 * (t_now * 12.0).sin() } else { 1.0 };
    for (j, &e) in n.limit.iter().enumerate() {
        disp(&mut q_node, e, j < cap);
        if j < cap && let Ok(mut bg) = q_bg.get_mut(e) {
            let full = count >= cap;
            let want = match (j < count, full && hot) {
                (true, true) => Color::srgba(1.0, 0.55, 0.3, throb),
                (true, false) => Color::srgba(1.0, 0.95, 0.85, if full { 0.95 } else { 0.8 }),
                (false, _) => Color::srgba(1.0, 1.0, 1.0, 0.22),
            };
            if bg.0 != want {
                bg.0 = want;
            }
        }
    }
    disp(&mut q_node, n.limit_key, cap > 0 && hot);
    if cap > 0 && hot && let Ok((mut t, _)) = q_text.get_mut(n.limit_key) {
        let label: String = binds.get(Action::Dismiss).label().chars().take(5).collect();
        if t.0 != label {
            t.0 = label;
        }
    }
    // the tray (Tab)
    for j in 0..TRAY {
        let nd = &n.tray[j];
        let e = edit.tray.get(j).filter(|_| edit_on);
        disp(&mut q_node, nd.holder, e.is_some());
        let Some(e) = e else { continue };
        let (color, gl) = look_of(e.def, e.power, &e.bind, table.as_deref(), corps);
        let dragged = matches!(edit.drag, Some((Src::Tray(x), _, true)) if x == j);
        let p = IconParams {
            color: Vec4::new(color[0], color[1], color[2], if dragged { 0.35 } else { 0.95 }),
            a: Vec4::new(gl as f32, 1.0, -1.0, if edit.hover == Some(Over::Tray(j)) { 0.4 } else { 0.0 }),
            b: Vec4::new(1.0, 0.0, 0.0, 0.0),
            c: Vec4::new(1.0, 1.0, 1.0, 9.1 + j as f32),
        };
        if mats.get(&nd.mat).is_some_and(|m| m.params != p)
            && let Some(mut m) = mats.get_mut(&nd.mat)
        {
            m.params = p;
        }
    }
    // the drag ghost follows the cursor
    let ghost = match (edit.drag, edit.cursor) {
        (Some((src, _, true)), Some(c)) => {
            let (def, power, bind) = match src {
                Src::Slot(k) => (bar.slots[k].def, bar.slots[k].power, bar.slots[k].bind.clone()),
                Src::Tray(j) => edit.tray.get(j).map_or((None, None, SlotBind::Empty), |e| (e.def, e.power, e.bind.clone())),
            };
            Some((c, look_of(def, power, &bind, table.as_deref(), corps)))
        }
        _ => None,
    };
    disp(&mut q_node, n.ghost.0, ghost.is_some());
    if let Some((c, (color, gl))) = ghost {
        if let Ok(mut nd) = q_node.get_mut(n.ghost.0) {
            let (l, t) = (px(c.x - ICON * 0.5), px(c.y - ICON * 0.5));
            if nd.left != l || nd.top != t {
                nd.left = l;
                nd.top = t;
            }
        }
        let p = IconParams { color: Vec4::new(color[0], color[1], color[2], 0.9), a: Vec4::new(gl as f32, 1.0, -1.0, 0.3), b: Vec4::ZERO, c: Vec4::new(1.0, 1.0, 1.0, 0.0) };
        if let Some(mut m) = mats.get_mut(&n.ghost.1)
            && m.params != p
        {
            m.params = p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(n: u8) -> Entry {
        Entry::new(&format!("move {n}"), SlotBind::Move { mv: MoveRef::new(10 + n as u16, 0, 0), kit: false })
    }

    fn bar() -> Hotbar {
        let mut h = Hotbar::from_prefs(&["hammer".into(), "shield".into(), String::new(), String::new()], &[false; 4]);
        h.auto_bind(&mv(1)); // → slot 3
        h
    }

    #[test]
    fn drag_rules() {
        let mut h = bar();
        assert_eq!(h.slots[2].phrase, "move 1");
        let tray = vec![mv(7)];
        // slot → slot swaps
        drop_on(&mut h, &tray, Src::Slot(2), Over::Slot(0), true);
        assert_eq!((h.slots[0].phrase.as_str(), h.slots[2].phrase.as_str()), ("move 1", "hammer"));
        // a click that never moved never clears
        drop_on(&mut h, &tray, Src::Slot(0), Over::Nothing, false);
        assert_eq!(h.slots[0].phrase, "move 1");
        // dragged off the bar: cleared
        drop_on(&mut h, &tray, Src::Slot(0), Over::Nothing, true);
        assert!(h.slots[0].is_empty());
        // tray → slot binds
        drop_on(&mut h, &tray, Src::Tray(0), Over::Slot(3), true);
        assert_eq!(h.slots[3].phrase, "move 7");
        // tray → nowhere: nothing
        drop_on(&mut h, &tray, Src::Tray(0), Over::Nothing, true);
        assert_eq!(h.slots.iter().filter(|s| s.phrase == "move 7").count(), 1);
    }

    #[test]
    fn keyboard_select_swap_pin() {
        let mut h = bar();
        let mut sel = None;
        key_edit(&mut h, &mut sel, 2);
        assert_eq!(sel, Some(2));
        key_edit(&mut h, &mut sel, 0);
        assert_eq!(sel, None);
        assert_eq!(h.slots[0].phrase, "move 1", "swapped");
        key_edit(&mut h, &mut sel, 0);
        key_edit(&mut h, &mut sel, 0);
        assert!(h.slots[0].pinned, "the same key twice pins");
        key_edit(&mut h, &mut sel, 0);
        key_edit(&mut h, &mut sel, 0);
        assert!(!h.slots[0].pinned, "and unpins");
    }
}
