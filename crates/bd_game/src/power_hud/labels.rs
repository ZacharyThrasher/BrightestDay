//! HOVER TOOLTIP over YOUR power icons (owner feedback, verbatim: "this is very confusing--idk what
//! ability is what, and I don't know what all the things under rinnegan mean. Maybe only have a
//! tooltip come up when they hover over the actual ability?"). The one place powers are named.
//!
//! While Tab is held there is NO always-on name row and NO big power card — just icons (`strip`,
//! `hotbar::hud`) plus the scoreboard. Tab already frees the cursor for hotbar drag-and-drop
//! (`hotbar::hud::TabEdit`); this module reuses that SAME cursor + hit-testing to find the one icon
//! under the mouse and pop ONE compact tooltip beside it:
//! * a **hotbar slot** or **tray / recent chip** (`hotbar::hud::TabEdit::hover`, plus the tray list):
//!   its bound name, a one-line plain description derived from its move's op (`bind_desc`), how to
//!   fire it in words ("Press 3", "Drag to a slot" — never a glyph), its spark cost and cooldown.
//! * a **held power's icon** (this module's own hit-test against the strip's icon rects): the
//!   power's name / current form burning large, then its moves listed compactly, one per line in
//!   plain words ("Tap G — Almighty Push: shove everyone away") — derived from the data alone (no
//!   per-power authoring), so a rework never goes stale.
//! * the **carried map pickup**: its name + a one-line description of what it unlocks.
//!
//! Only what you HOLD (never what could be obtained): the mystery tenet. Nodes are spawned once;
//! text / display only change while a tooltip is actually showing (Tab dwell time is short).
//!
//! Dev: `BD_TAB_TEST=1` holds Tab (screenshots).

use super::derive::PowerHud;
use super::strip::Strip;
use super::PowerHudState;
use crate::console::RingLexicon;
use crate::hotbar::hud::{bind_fill, bind_sparks, Over, TabEdit};
use crate::hotbar::{Hotbar, SlotBind};
use crate::lobby::prefs::{Action, KeyBinds};
use crate::sim_bridge::{ActiveWorld, ViewKeys};
use crate::state::{AppState, InGameMenu};
use bd_sim::pickups::GateKind;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Longest power label (chars) before it is shortened.
const TAG_CHARS: usize = 28;
/// Longest hotbar-slot / tray-entry title (chars) before it is shortened (bound phrases run longer
/// than a power's own name, and the tooltip has room).
const BIND_TAG_CHARS: usize = 40;
/// Tooltip card width (px; definite so wrapped lines size it consistently).
const CARD_W: f32 = 300.0;
/// Held-power move lines shown before the list is capped (every power ships well under this).
const MAX_LINES: usize = 8;
const HEADER_H: f32 = 24.0;
const DESC_H: f32 = 16.0;
const META_H: f32 = 16.0;
const LINE_H: f32 = 16.0;
const PAD: f32 = 14.0;

pub fn build(app: &mut App) {
    app.add_systems(Startup, attach).add_systems(Update, update.in_set(crate::present::PresentSet::Sync));
}

// ------------------------------------------------------------------------------------------- UI

/// The one tooltip's spawned entities (spawned once; text / display flip per frame).
#[derive(Resource)]
struct Tooltip {
    root: Entity,
    title: Entity,
    desc: Entity,
    meta: Entity,
    /// Fixed pool of line rows (a held power's move list); unused ones stay hidden.
    lines: Vec<Entity>,
}

fn attach(mut commands: Commands) {
    let mut title = Entity::PLACEHOLDER;
    let mut desc = Entity::PLACEHOLDER;
    let mut meta = Entity::PLACEHOLDER;
    let mut lines: Vec<Entity> = Vec::with_capacity(MAX_LINES);
    let root = commands
        .spawn((
            Name::new("PowerTooltip"),
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                padding: UiRect::new(px(9), px(9), px(6), px(7)),
                max_width: px(CARD_W),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.012, 0.035, 0.92)),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.22)),
            Pickable::IGNORE,
            GlobalZIndex(120),
        ))
        .with_children(|r| {
            title = r.spawn((Text::new(""), TextFont::from_font_size(15.0), TextColor(Color::WHITE), TextLayout::default(), Pickable::IGNORE)).id();
            desc = r
                .spawn((Node { display: Display::None, ..default() }, Text::new(""), TextFont::from_font_size(12.0), TextColor(Color::srgba(0.86, 0.9, 1.0, 0.88)), TextLayout::default(), Pickable::IGNORE))
                .id();
            meta = r.spawn((Node { display: Display::None, ..default() }, Text::new(""), TextFont::from_font_size(11.0), TextColor(Color::srgba(0.7, 0.8, 0.95, 0.8)), Pickable::IGNORE)).id();
            for _ in 0..MAX_LINES {
                lines.push(
                    r.spawn((
                        Node { display: Display::None, ..default() },
                        Text::new(""),
                        TextFont::from_font_size(12.0),
                        TextColor(Color::srgba(0.88, 0.92, 1.0, 0.92)),
                        TextLayout::default(),
                        Pickable::IGNORE,
                    ))
                    .id(),
                );
            }
        })
        .id();
    commands.insert_resource(Tooltip { root, title, desc, meta, lines });
}

fn set_display(nodes: &mut Query<&mut Node>, e: Entity, on: bool) {
    if let Ok(mut n) = nodes.get_mut(e) {
        let d = if on { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    }
}

fn set_text(texts: &mut Query<&mut Text>, e: Entity, s: &str) {
    if let Ok(mut t) = texts.get_mut(e)
        && t.0 != s
    {
        t.0 = s.to_string();
    }
}

// --------------------------------------------------------------------------------------- hover

/// The one thing under the cursor right now (Tab held, hit-tested against the hotbar's own
/// slot / tray hover — `TabEdit::hover` — plus this module's own test against the strip's icons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    /// A held power's strip icon (its display slot index).
    Power(usize),
    Pickup,
    /// A hotbar slot (keys 1-4).
    Slot(usize),
    /// A tray chip (recent moves / your held powers' moves, shown while Tab is held).
    Tray(usize),
}

fn hit(hits: &Query<(&ComputedNode, &UiGlobalTransform)>, e: Entity, p: Vec2) -> bool {
    hits.get(e).is_ok_and(|(c, t)| c.contains_point(*t, p))
}

/// What's hovered: the hotbar's own hover (slot / tray) takes priority (it already excludes a
/// drag-in-progress); otherwise this module hit-tests the held-power icons + the pickup icon
/// itself, using the SAME freed cursor (`TabEdit::cursor`, physical via the window's scale).
fn resolve_target(edit: &TabEdit, bar: &Hotbar, hud: &PowerHud, strip: &Strip, hits: &Query<(&ComputedNode, &UiGlobalTransform)>, scale: f32) -> Option<Target> {
    if !edit.on || edit.drag.is_some() {
        return None;
    }
    match edit.hover {
        Some(Over::Slot(k)) if bar.slots.get(k).is_some_and(|s| !s.is_empty()) => return Some(Target::Slot(k)),
        Some(Over::Tray(j)) if j < edit.tray.len() => return Some(Target::Tray(j)),
        _ => {}
    }
    let p = edit.cursor? * scale;
    for (k, (e, _)) in strip.icons.iter().enumerate() {
        if hud.gems[k].power.is_some() && hit(hits, *e, p) {
            return Some(Target::Power(k));
        }
    }
    if hud.pickup.is_some() && hit(hits, strip.pickup.0, p) {
        return Some(Target::Pickup);
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn update(
    edit: Res<TabEdit>,
    bar: Res<Hotbar>,
    st: Res<PowerHudState>,
    strip: Option<Res<Strip>>,
    active: Option<Res<ActiveWorld>>,
    keys: Res<ViewKeys>,
    binds: Res<KeyBinds>,
    lex: Option<Res<RingLexicon>>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    hits: Query<(&ComputedNode, &UiGlobalTransform)>,
    tip: Option<Res<Tooltip>>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
) {
    let Some(tip) = tip else { return };
    let hide = |nodes: &mut Query<&mut Node>| set_display(nodes, tip.root, false);
    if *state.get() != AppState::InGame || menu.0 {
        hide(&mut nodes);
        return;
    }
    let scale = window.as_ref().map_or(1.0, |w| w.scale_factor());
    let (Some(strip), Some(a)) = (strip.as_deref(), active.as_deref()) else {
        hide(&mut nodes);
        return;
    };
    let Some(target) = resolve_target(&edit, &bar, &st.0, strip, &hits, scale) else {
        hide(&mut nodes);
        return;
    };
    let held = a.local_player().map(|p| &p.power);
    let tb = keys.curr.own.as_ref().and_then(|o| o.toolbox).filter(|_| keys.valid);
    let Some(content) = build_content(target, &bar, &st, a, held, tb, lex.as_deref(), &binds, &edit.tray) else {
        hide(&mut nodes);
        return;
    };
    set_text(&mut texts, tip.title, &content.title);
    set_display(&mut nodes, tip.desc, !content.desc.is_empty());
    set_text(&mut texts, tip.desc, &content.desc);
    set_display(&mut nodes, tip.meta, !content.meta.is_empty());
    set_text(&mut texts, tip.meta, &content.meta);
    for (i, &le) in tip.lines.iter().enumerate() {
        let on = i < content.lines.len();
        set_display(&mut nodes, le, on);
        if on {
            set_text(&mut texts, le, &content.lines[i]);
        }
    }
    // Beside the cursor, clamped inside the window (never overlaps the edge, never truncated).
    let (ww, wh) = window.as_ref().map_or((1280.0, 720.0), |w| (w.width(), w.height()));
    let est_h = HEADER_H + if content.desc.is_empty() { 0.0 } else { DESC_H } + if content.meta.is_empty() { 0.0 } else { META_H } + content.lines.len() as f32 * LINE_H + PAD;
    let c = edit.cursor.unwrap_or_default();
    // Above the hovered icon (never covering the hotbar / power strip it points at).
    let x = (c.x - CARD_W * 0.5).min((ww - CARD_W - 8.0).max(8.0)).max(8.0);
    let y = (c.y - est_h - 36.0).min((wh - est_h - 8.0).max(8.0)).max(8.0);
    if let Ok(mut n) = nodes.get_mut(tip.root) {
        let (l, t) = (px(x), px(y));
        if n.left != l || n.top != t {
            n.left = l;
            n.top = t;
        }
    }
    set_display(&mut nodes, tip.root, true);
}

// ---------------------------------------------------------------------------------- content (pure)

/// What a tooltip shows: a title, an optional one-line description, a "how to fire it / cost /
/// cooldown" meta line, and/or (a held power's icon) a compact per-move list.
#[derive(Clone, Debug, Default, PartialEq)]
struct TooltipContent {
    title: String,
    desc: String,
    meta: String,
    lines: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn build_content(
    target: Target,
    bar: &Hotbar,
    st: &PowerHudState,
    a: &ActiveWorld,
    held: Option<&bd_sim::powers::PlayerPower>,
    tb: Option<bd_sim::powers::chain::ToolboxSnap>,
    lex: Option<&RingLexicon>,
    binds: &KeyBinds,
    tray: &[crate::hotbar::Entry],
) -> Option<TooltipContent> {
    let hud = &st.0;
    match target {
        Target::Power(k) => {
            let g = hud.gems[k];
            let id = g.power?;
            let def = bd_sim::powers::PowerTable::get().def(id)?;
            let key = binds.get(Action::Power).label();
            let cooldown_secs = def.moves_at(g.tier).first().map_or(0.0, |m| m.cooldown);
            Some(power_content(def, id, g.tier, g.transformed, if g.active { 0 } else { 1 }, &key, g.shown, cooldown_secs))
        }
        Target::Pickup => hud.pickup.map(|(kind, life)| pickup_content(kind, life)),
        Target::Slot(k) => {
            let s = bar.slots.get(k)?;
            if s.is_empty() {
                return None;
            }
            let (fill, uses) = bind_fill(&s.bind, st, held, tb);
            let sparks = bind_sparks(&s.bind, &s.phrase, s.def, a, lex, uses);
            let key = binds.get([Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4][k]).label();
            let cooldown_secs = s.def.map_or(0.0, |d| d.cooldown);
            Some(bind_content(title_text(&s.phrase), &s.bind, s.def, s.power, &format!("Press {key}"), sparks, Some(fill), cooldown_secs))
        }
        Target::Tray(j) => {
            let e = tray.get(j)?;
            let (fill, uses) = bind_fill(&e.bind, st, held, tb);
            let sparks = bind_sparks(&e.bind, &e.phrase, e.def, a, lex, uses);
            let cooldown_secs = e.def.map_or(0.0, |d| d.cooldown);
            Some(bind_content(title_text(&e.phrase), &e.bind, e.def, e.power, "Drag to a slot", sparks, Some(fill), cooldown_secs))
        }
    }
}

/// A hotbar slot / tray entry's tooltip: its bound name, a plain description, how to fire it, its
/// spark cost and cooldown state.
fn bind_content(
    title: String,
    bind: &SlotBind,
    def: Option<&'static bd_sim::powers::MoveDef>,
    power: Option<&'static bd_sim::powers::PowerDef>,
    fire: &str,
    sparks: u32,
    fill: Option<f32>,
    cooldown_secs: f32,
) -> TooltipContent {
    let desc = bind_desc(bind, def, power);
    let cost = if sparks == 0 { "Free".to_string() } else { format!("{sparks} spark{}", if sparks == 1 { "" } else { "s" }) };
    let cd = match fill {
        Some(f) if f >= 0.999 => "Ready".to_string(),
        Some(f) => format!("{:.1}s", (cooldown_secs * (1.0 - f)).max(0.0)),
        None => String::new(),
    };
    let meta = [fire.to_string(), cost, cd].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("   |   ");
    TooltipContent { title, desc, meta, lines: Vec::new() }
}

/// A bind's one-line plain description, derived from its move's first op (never authored per
/// power) — a granted power names itself, a plain construct stays generic (mystery tenet).
fn bind_desc(bind: &SlotBind, def: Option<&'static bd_sim::powers::MoveDef>, power: Option<&'static bd_sim::powers::PowerDef>) -> String {
    match bind {
        SlotBind::Empty => String::new(),
        SlotBind::Phrase { .. } => match power {
            Some(p) => format!("Grants {}", short_name(p.name.trim())),
            None => "Casts your bound phrase".to_string(),
        },
        _ => def.and_then(move_verb).map(|v| verb_desc(v).to_string()).unwrap_or_else(|| "Fires a move".to_string()),
    }
}

/// A held power's icon tooltip: its name / current form (large), then its moves compactly, one
/// per line in plain words ("Tap G — Almighty Push: shove everyone away").
fn power_content(def: &'static bd_sim::powers::PowerDef, id: u16, tier: u8, transformed: bool, slot: usize, key: &str, ready: f32, cooldown_secs: f32) -> TooltipContent {
    let (title, _) = power_label(def, tier, transformed);
    let meta = if ready >= 0.999 { "Ready".to_string() } else { format!("{:.1}s", (cooldown_secs * (1.0 - ready)).max(0.0)) };
    let hints = power_hints(def, id, tier, transformed, slot);
    let lines = hints.iter().take(MAX_LINES).map(|h| format!("{} — {}: {}", input_text(h.input, key), h.name, lower_first(verb_desc(h.verb)))).collect();
    TooltipContent { title, desc: String::new(), meta, lines }
}

fn pickup_content(kind: GateKind, life: f32) -> TooltipContent {
    let title = match kind {
        GateKind::Giant => "GIANT",
        GateKind::Awakening => "AWAKENING",
        GateKind::Ultimate => "ULTIMATE",
    }
    .to_string();
    let desc = match kind {
        GateKind::Giant => "Unlocks your giant form",
        GateKind::Awakening => "Unlocks your strongest transformation",
        GateKind::Ultimate => "Unlocks your ultimate move",
    }
    .to_string();
    TooltipContent { title, desc, meta: format!("{:.0}% left", (life * 100.0).clamp(0.0, 100.0)), lines: Vec::new() }
}

fn title_text(phrase: &str) -> String {
    let t = phrase.trim();
    if t.is_empty() { "UNBOUND".to_string() } else { clip(&t.to_uppercase(), BIND_TAG_CHARS) }
}

/// How a move / passive is triggered, spelled out in words (never a glyph).
fn input_text(i: Input, key: &str) -> String {
    match i {
        Input::Tap => format!("Press {key}"),
        Input::Hold => format!("Hold {key}"),
        Input::Shout => "Say it".to_string(),
        Input::Always => "Always on".to_string(),
    }
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A short, plain, generic one-liner for verb `v` (never per-power authoring).
fn verb_desc(v: Verb) -> &'static str {
    match v {
        Verb::Projectile => "Fires a projectile",
        Verb::Beam => "Fires a beam",
        Verb::Blast => "Blasts the area around you",
        Verb::Strike => "A melee strike",
        Verb::Pull => "Pulls enemies toward you",
        Verb::Dash => "Dashes or teleports",
        Verb::Shield => "Raises a shield",
        Verb::Dodge => "Dodges automatically",
        Verb::Summon => "Summons a construct",
        Verb::Form => "Changes your form",
        Verb::Aura => "A passive effect",
        Verb::Grab => "Grabs the target",
        Verb::Trap => "Traps the target",
    }
}

// ------------------------------------------------------------------------- derivation (pure)

/// What a move / passive DOES, from a small closed table mapped from its op type (never authored
/// per power).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verb {
    Projectile,
    Beam,
    Blast,
    Strike,
    Pull,
    Dash,
    Shield,
    Dodge,
    Summon,
    Form,
    Aura,
    Grab,
    Trap,
}

impl Verb {
    const ALL: [Verb; 13] =
        [Verb::Projectile, Verb::Beam, Verb::Blast, Verb::Strike, Verb::Pull, Verb::Dash, Verb::Shield, Verb::Dodge, Verb::Summon, Verb::Form, Verb::Aura, Verb::Grab, Verb::Trap];
}

/// How a move is triggered (shout = say OR type the phrase: voice and typing are one path).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Input {
    /// Tap the power key.
    Tap,
    /// Hold the power key.
    Hold,
    /// Shout it (the phrase itself is never shown: mystery).
    Shout,
    /// Passive: always on while held.
    Always,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Hint {
    input: Input,
    verb: Verb,
    /// The move's own name (or a generic label for a form-shout / passive).
    name: String,
}

/// The verb of a cast recipe (by what it becomes).
fn recipe_verb(r: &bd_sim::powers::RecipeDef) -> Verb {
    use bd_core::mechanics::{Form, FormCategory, Mechanic as M};
    let has = |m: M| r.mech.contains(&m);
    if has(M::Beam) || has(M::Breath) {
        return Verb::Beam;
    }
    match r.form.category() {
        FormCategory::Creature => Verb::Summon,
        _ if has(M::Trap) || r.form == Form::Cage || (has(M::Sticky) && has(M::Explodes)) => Verb::Trap,
        _ if has(M::Grab) || has(M::Constrict) => Verb::Grab,
        _ if has(M::Pull) => Verb::Pull,
        FormCategory::Structure => Verb::Shield,
        FormCategory::Worn => Verb::Dash,
        _ if r.form == Form::Shield || has(M::Barrier) => Verb::Shield,
        FormCategory::Held if !has(M::Shoots) && !matches!(r.form, Form::Gun | Form::Bow | Form::Cannon) => Verb::Strike,
        _ => Verb::Projectile,
    }
}

/// The verb of a move: its first op's.
fn move_verb(m: &bd_sim::powers::MoveDef) -> Option<Verb> {
    use bd_sim::powers::MoveOp as O;
    Some(match m.ops.first()? {
        O::Cast { recipe, .. } => recipe_verb(recipe),
        O::Impulse { .. } | O::Blink { .. } | O::Reel { .. } | O::Portal { .. } | O::Swap { .. } => Verb::Dash,
        O::Shove { .. } => Verb::Strike,
        O::Field { strength, .. } if *strength > 0.0 => Verb::Pull,
        O::Field { .. } | O::Explode { .. } | O::SlamOnLand { .. } => Verb::Blast,
        O::Room { .. } | O::Weigh { .. } => Verb::Trap,
        O::Link { .. } => Verb::Grab,
        O::Carve { pull: true, .. } | O::Yank { .. } | O::Singularity { .. } => Verb::Pull,
        O::Carve { .. } => Verb::Beam,
        O::TimeStop { .. } | O::SelfStatus { .. } => Verb::Aura,
        O::Restore { .. } => Verb::Shield,
        O::Transform | O::Tier(_) => Verb::Form,
        // Generic ops (bd_sim powers/ops, docs/toolbox/OPS.md).
        O::Rewind { .. } => Verb::Dash,
        O::Ram { .. } => Verb::Strike,
        O::Hook { pull, .. } if *pull > 0.0 => Verb::Pull,
        O::Hook { .. } | O::LockOn { .. } => Verb::Projectile,
        O::Grab { .. } => Verb::Grab,
        O::Deflect { .. } | O::Counter { .. } | O::Mend { .. } => Verb::Shield,
        O::Lockout { .. } | O::Zone { .. } | O::Capture { .. } | O::Afflict { .. } | O::GazeStun { .. } | O::Banish { .. } | O::Contagion { .. } => Verb::Trap,
        // (TOOLBOX QA: the toolbox wave's generic ops, by what they look like to the player)
        O::Leap { .. }
        | O::Pounce { .. }
        | O::Evade { .. }
        | O::BlinkCarry { .. }
        | O::Transpose { .. }
        | O::MarkRecall { .. }
        | O::WarpTo { .. }
        | O::Relocate { .. }
        | O::PassThrough { .. }
        | O::PlacePortal { .. } => Verb::Dash,
        O::Phase { .. } => Verb::Dodge,
        O::Finisher { .. } | O::Drill { .. } | O::Rush { .. } | O::Barrage { .. } | O::TagStrike { .. } | O::Tag { .. } => Verb::Strike,
        O::Seeker { .. } | O::Salvo { .. } => Verb::Projectile,
        O::BounceBeam { .. } | O::Wave { .. } => Verb::Beam,
        O::DelayedHit { .. } | O::Erupt { .. } | O::Surge { .. } => Verb::Blast,
        O::Haul { .. } => Verb::Pull,
        O::Disarm { .. } => Verb::Grab,
        O::RaiseFallen { .. } => Verb::Summon,
        O::SelfResize { .. } => Verb::Form,
        #[allow(unreachable_patterns)]
        _ => Verb::Aura,
    })
}

/// The verb of a passive effect.
fn effect_verb(e: &bd_sim::powers::PowerEffect) -> Verb {
    use bd_sim::powers::PowerEffect as E;
    match e {
        E::AutoDodge { .. } => Verb::Dodge,
        E::ProjectileSlowField { .. } => Verb::Shield,
        _ => Verb::Aura,
    }
}

/// The name of a passive effect, for its tooltip line (generic; a passive isn't a specific move).
fn passive_name(e: &bd_sim::powers::PowerEffect) -> &'static str {
    use bd_sim::powers::PowerEffect as E;
    match e {
        E::AutoDodge { .. } => "Auto-Dodge",
        E::ProjectileSlowField { .. } => "Slow Field",
        _ => "Passive",
    }
}

/// How power `def` (at `tier`, `transformed`, in held `slot`: 0 = active) is used: per move its
/// input, verb and own name — the active power's first two moves are the power key (tap / hold),
/// the rest (and another held power's moves, when they have a shout) are shouted — then a FORM
/// shout for a tiered power, then its passives ("always"). Derived from the data alone.
fn power_hints(def: &'static bd_sim::powers::PowerDef, id: u16, tier: u8, transformed: bool, slot: usize) -> Vec<Hint> {
    let tier_shouts = def.tier(tier).map(|t| t.shouts.as_slice()).unwrap_or(&[]);
    let shoutable = |k: usize| tier_shouts.iter().chain(def.shouts.iter()).any(|(_, s)| *s as usize == k);
    let mut out = Vec::new();
    for (k, m) in def.moves_at(tier).iter().enumerate() {
        let Some(verb) = move_verb(m) else { continue };
        let input = match k {
            0 | 1 if slot == 0 || !shoutable(k) => {
                if k == 0 {
                    Input::Tap
                } else {
                    Input::Hold
                }
            }
            _ => Input::Shout,
        };
        out.push(Hint { input, verb, name: m.name.trim().to_string() });
    }
    // Tiers are picked by saying their name.
    if !def.tiers.is_empty() && !out.iter().any(|h| h.verb == Verb::Form) {
        out.push(Hint { input: Input::Shout, verb: Verb::Form, name: "Transform".to_string() });
    }
    // Passives: a stand / clone at your side, then effects (one line per verb).
    let view = bd_sim::powers::HeldView { id, def, tier, transformed, slot };
    let passives = def.companion.as_ref().map(|_| (Verb::Summon, "Companion".to_string())).into_iter().chain(view.effects().map(|e| (effect_verb(e), passive_name(e).to_string())));
    for (verb, name) in passives {
        if out.iter().any(|h| h.input == Input::Always && h.verb == verb) {
            continue;
        }
        out.push(Hint { input: Input::Always, verb, name });
    }
    out
}

/// Normalised for "does the form name already say the power's name" (Super Saiyan Blue ⊃ Super
/// Saiyan, Kaio-ken x10 ⊃ Kaio-ken).
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// "Tori Tori no Mi, Model: Phoenix" → "Phoenix" (else the name).
fn short_name(name: &str) -> &str {
    name.split_once("Model:").map_or(name, |(_, m)| m.trim())
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect::<String>().trim_end().to_string();
        t.push('…');
        t
    }
}

/// The name of the form `def` is in (`tier` / `transformed`), if any: the tier's name, else the
/// transform's title, else the name of the move that transforms.
fn form_name(def: &bd_sim::powers::PowerDef, tier: u8, transformed: bool) -> Option<String> {
    if let Some(t) = def.tier(tier).filter(|t| !t.name.trim().is_empty()) {
        return Some(t.name.trim().to_string());
    }
    if !transformed {
        return None;
    }
    let title = def.transform.as_ref().and_then(|t| t.ascend.as_ref()).and_then(|a| a.title.clone()).filter(|s| !s.trim().is_empty());
    let mv = || def.moves.iter().find(|m| m.ops.iter().any(|o| matches!(o, bd_sim::powers::MoveOp::Transform))).map(|m| m.name.clone());
    title.or_else(mv).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// The label for power `def` in its current form: the power's name, or the form's name (alone when
/// it already carries the power's name, else "POWER · FORM"), shortened to [`TAG_CHARS`].
fn power_label(def: &bd_sim::powers::PowerDef, tier: u8, transformed: bool) -> (String, bool) {
    let name = def.name.trim();
    let short = short_name(name);
    let fits = |s: &str| s.chars().count() <= TAG_CHARS;
    let label = match form_name(def, tier, transformed) {
        Some(f) => {
            let nf = norm(&f);
            if nf.contains(&norm(name)) || nf.contains(&norm(short)) {
                return (clip(&f.to_uppercase(), TAG_CHARS), true);
            }
            [format!("{name} - {f}"), format!("{short} - {f}")].into_iter().find(|s| fits(s)).unwrap_or(f)
        }
        None if fits(name) => name.to_string(),
        None => short.to_string(),
    };
    let form = form_name(def, tier, transformed).is_some();
    (clip(&label.to_uppercase(), TAG_CHARS), form)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_sim::powers::PowerTable;

    fn pid(key: &str) -> u16 {
        PowerTable::get().index_of(key).unwrap_or_else(|| panic!("no power {key}"))
    }

    fn label(key: &str, tier: u8, tf: bool) -> (String, bool) {
        power_label(PowerTable::get().def(pid(key)).unwrap(), tier, tf)
    }

    fn hints(key: &str, tier: u8, tf: bool, slot: usize) -> Vec<(Input, Verb)> {
        let id = pid(key);
        power_hints(PowerTable::get().def(id).unwrap(), id, tier, tf, slot).into_iter().map(|h| (h.input, h.verb)).collect()
    }

    #[test]
    fn names_and_forms() {
        // Base: the power's own name, caps, no form.
        assert_eq!(label("rinnegan", 0, false), ("RINNEGAN".to_string(), false));
        assert_eq!(label("ultra_instinct", 0, false), ("ULTRA INSTINCT".to_string(), false));
        // A tier that already says the power's name stands alone, highlighted.
        let ss = PowerTable::get().def(pid("super_saiyan")).unwrap();
        let blue = ss.tiers.iter().position(|t| t.name == "Super Saiyan Blue").unwrap() as u8 + 1;
        assert_eq!(label("super_saiyan", blue, false), ("SUPER SAIYAN BLUE".to_string(), true));
        assert_eq!(label("ultra_instinct", 1, false).0, "MASTERED ULTRA INSTINCT");
        // A tier that doesn't: "POWER · TIER".
        assert_eq!(label("gomu_gomu", 4, false), ("GOMU GOMU NO MI - GEAR FIFTH".to_string(), true));
        // A transform: its title, with the model's short name (Gear 5).
        assert_eq!(label("nika", 0, true), ("NIKA - GEAR FIFTH".to_string(), true));
        // Long names shorten cleanly; nothing is ever longer than the cap.
        assert_eq!(label("tori_phoenix", 0, false).0, "PHOENIX");
        assert_eq!(label("tori_phoenix", 0, true).0, "PHOENIX FORM");
        for d in &PowerTable::get().powers {
            for (tier, tf) in [(0, false), (0, true), (1, false), (4, false)] {
                let (l, _) = power_label(d, tier, tf);
                assert!(!l.is_empty() && l.chars().count() <= TAG_CHARS, "{l:?}");
                assert_eq!(l, l.to_uppercase());
            }
        }
    }

    #[test]
    fn usage_hints_from_the_data() {
        use Input::*;
        use Verb::*;
        // Rinnegan (active): push = tap, planetary devastation = hold (a pulling core), universal
        // pull + truth-seeking orbs = shouts, and its passive read last. (The content rework added
        // more shouted paths after the orbs — the six paths — so only the head and the passive are
        // pinned: new shouted moves never break this.)
        let rin = hints("rinnegan", 0, false, 0);
        assert_eq!(rin[..4], [(Tap, Blast), (Hold, Pull), (Shout, Pull), (Shout, Summon)], "{rin:?}");
        assert_eq!(rin.last(), Some(&(Always, Aura)), "{rin:?}");
        assert!(rin[4..rin.len() - 1].iter().all(|(i, _)| *i == Shout), "slots 2.. are shouted: {rin:?}");
        // Kamehameha-bearers: a beam on hold; tiers are shouted forms; buffs are always on.
        assert_eq!(hints("super_saiyan", 0, false, 0), vec![(Tap, Dash), (Hold, Beam), (Shout, Form), (Always, Aura)]);
        assert_eq!(hints("ultra_instinct", 0, false, 0), vec![(Tap, Dash), (Hold, Beam), (Shout, Form), (Always, Dodge), (Always, Aura)]);
        // Gear 5 (Nika): the transform is its hold.
        assert_eq!(hints("nika", 0, false, 0), vec![(Tap, Projectile), (Hold, Form)]);
        // Shadow clones: summon on tap, rasengan on hold, the clone at your side always.
        // (the rasengan is a Drill now — driven INTO them, a strike; SHONEN CLASSICS reworked it)
        assert_eq!(hints("shadow_clone", 0, false, 0), vec![(Tap, Summon), (Hold, Strike), (Always, Summon)]);
        // Not the active power: its moves are shouted (the key fires the active one).
        assert_eq!(hints("super_saiyan", 0, false, 1)[..2], [(Shout, Dash), (Shout, Beam)]);
        // Every verb has its own short, plain, non-empty description.
        for v in Verb::ALL {
            let d = verb_desc(v);
            assert!(!d.is_empty() && d.chars().count() < 40, "{v:?}: {d:?}");
        }
        // Every power yields at least one hint, and every hint has a non-empty name.
        for (i, d) in PowerTable::get().powers.iter().enumerate() {
            let hs = power_hints(d, i as u16, 0, false, 0);
            assert!(!hs.is_empty(), "{} has no hint", d.id);
            assert!(hs.iter().all(|h| !h.name.trim().is_empty()), "{}: {hs:?}", d.id);
        }
    }

    #[test]
    fn power_tooltip_lines_are_short_plain_and_well_formed() {
        // Every held power's icon tooltip: a title, at least one line, and every line follows
        // "<how to fire> — <name>: <description>" with no doubled spaces / empty pieces.
        for (i, d) in PowerTable::get().powers.iter().enumerate() {
            let c = power_content(d, i as u16, 0, false, 0, "G", 1.0, 2.0);
            assert!(!c.title.is_empty(), "{}", d.id);
            assert!(!c.lines.is_empty(), "{}: no tooltip lines", d.id);
            for l in &c.lines {
                assert!(!l.is_empty() && !l.contains("  ") && l.contains(" — ") && l.contains(": "), "{}: {l:?}", d.id);
            }
        }
        // Cooling: a fractional-second countdown, not "Ready".
        let d = PowerTable::get().def(pid("rinnegan")).unwrap();
        let cooling = power_content(d, pid("rinnegan"), 0, false, 0, "G", 0.5, 4.0);
        assert_eq!(cooling.meta, "2.0s");
        let ready = power_content(d, pid("rinnegan"), 0, false, 0, "G", 1.0, 4.0);
        assert_eq!(ready.meta, "Ready");
    }

    #[test]
    fn bind_meta_joins_only_the_parts_that_are_there() {
        // A fresh, free, ready hotbar slot: every part shows, joined once each.
        let full = bind_content("HAMMER".into(), &SlotBind::Empty, None, None, "Press 1", 0, Some(1.0), 2.0);
        assert_eq!(full.meta, "Press 1   |   Free   |   Ready");
        // A tray chip (no live cooldown known): no trailing separator, no "None".
        let tray = bind_content("HAMMER".into(), &SlotBind::Empty, None, None, "Drag to a slot", 3, None, 2.0);
        assert_eq!(tray.meta, "Drag to a slot   |   3 sparks");
        // Singular / plural spark wording.
        let one = bind_content("X".into(), &SlotBind::Empty, None, None, "Press 2", 1, Some(0.5), 4.0);
        assert!(one.meta.contains("1 spark   ") || one.meta.ends_with("1 spark"), "{:?}", one.meta);
        assert!(!one.meta.contains("1 sparks"));
    }

    #[test]
    fn bind_desc_names_a_grant_never_a_plain_phrase() {
        let power = PowerTable::get().def(pid("rinnegan")).unwrap();
        assert_eq!(bind_desc(&SlotBind::Phrase { signature: false }, None, Some(power)), "Grants Rinnegan");
        assert_eq!(bind_desc(&SlotBind::Phrase { signature: false }, None, None), "Casts your bound phrase");
        assert_eq!(bind_desc(&SlotBind::Empty, None, None), "");
    }

    #[test]
    fn pickup_tooltip_has_a_title_and_description_for_every_kind() {
        for k in GateKind::ALL {
            let c = pickup_content(k, 0.42);
            assert!(!c.title.is_empty() && !c.desc.is_empty());
            assert_eq!(c.meta, "42% left");
        }
    }

    #[test]
    fn input_text_is_spelled_out_never_a_glyph() {
        assert_eq!(input_text(Input::Tap, "G"), "Press G");
        assert_eq!(input_text(Input::Hold, "G"), "Hold G");
        assert_eq!(input_text(Input::Shout, "G"), "Say it");
        assert_eq!(input_text(Input::Always, "G"), "Always on");
    }
}
