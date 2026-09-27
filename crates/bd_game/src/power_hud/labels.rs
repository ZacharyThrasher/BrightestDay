//! Tab labels over YOUR power icons (owner: "when a player presses tab, show the names of the
//! powers/abilities they have … put a label that appears above their power icon when they tab"
//! + "give a suggestion on how it functions"). The one place powers are named.
//!
//! While the scoreboard key (Tab) is held, a small card pops above the strip for each held power,
//! in the strip's slot order and wearing the icon's own colour + glyph chip:
//! * the power's name, or its active tier / form's ("SUPER SAIYAN BLUE", "NIKA · GEAR FIFTH"),
//!   burning in the form's aura colour;
//! * usage chips derived from the data alone (no per-power authoring): per move, its input — a
//!   keycap from YOUR power bind (tap / HOLD), a speech bubble for a shouted move (say or type it:
//!   one path; the phrase itself is never shown: mystery) — and a verb from a small closed table
//!   mapped from the move's op (PROJECTILE, BEAM, BLAST, STRIKE, PULL, DASH, SHIELD, DODGE,
//!   SUMMON, FORM, AURA, GRAB, TRAP); then a FORM shout for a tiered power and its passives
//!   ("ALWAYS").
//! * a carried map pickup gets a small card of its own (GIANT / AWAKENING / ULTIMATE);
//! * the borrowed moves on the strip's chips get one card listing them (chip glyph + name, in chip
//!   order) — in the row with the others, so names never pile onto a power card.
//!
//! Only what you HOLD (never what could be obtained). The row lives inside the strip's root (so
//! it rides the strip's position and pop) and still shows when the strip is switched off. Rebuilt
//! only when what it shows changes; the fade just rewrites alphas while it runs.
//!
//! Dev: `BD_TAB_TEST=1` holds Tab (screenshots).

use super::derive::{self, Gem};
use super::strip::{IconMaterial, IconParams};
use super::PowerHudState;
use crate::lobby::prefs::{Action, KeyBinds};
use crate::state::{AppState, InGameMenu};
use bd_sim::pickups::GateKind;
use bevy::picking::Pickable;
use bevy::prelude::*;

/// Longest power label (chars) before it is shortened.
const TAG_CHARS: usize = 28;
/// Power card width (px; definite so its wrapped usage rows size it): three fit at 1280.
const CARD_W: f32 = 292.0;
const CHIP: f32 = 15.0;
/// Fade in / out (s).
const FADE_IN: f32 = 0.08;
const FADE_OUT: f32 = 0.16;

/// Marks the strip's root (the label row is inserted as its first child).
#[derive(Component)]
pub struct StripRoot;

#[derive(Component)]
struct LabelRow;

/// Base colours of a label node (the fade scales their alpha).
#[derive(Component, Clone, Copy, Default)]
struct Fade {
    text: Option<Color>,
    bg: Option<Color>,
    border: Option<Color>,
}

#[derive(Resource, Default)]
struct Labels {
    row: Option<Entity>,
    sig: u64,
    built: bool,
    fade: f32,
    applied: f32,
    /// Materials and their base alpha (faded with the rest).
    mats: Vec<(Handle<IconMaterial>, f32)>,
    /// One shared glyph material per verb.
    verb_mats: Vec<Handle<IconMaterial>>,
}

pub fn build(app: &mut App) {
    app.init_resource::<Labels>().add_systems(Update, (attach, update).chain().in_set(crate::present::PresentSet::Sync));
}

fn tab_test() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("BD_TAB_TEST").ok().and_then(|v| v.parse::<f32>().ok()).is_some_and(|v| v > 0.0))
}

/// Put the (hidden) label row above the strip's icons.
fn attach(mut commands: Commands, roots: Query<Entity, Added<StripRoot>>, mut labels: ResMut<Labels>) {
    for root in &roots {
        let row = commands
            .spawn((
                Name::new("PowerTabLabels"),
                LabelRow,
                Node { display: Display::None, flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, justify_content: JustifyContent::Center, column_gap: px(8), margin: UiRect::bottom(px(6)), ..default() },
                Pickable::IGNORE,
            ))
            .id();
        commands.entity(root).insert_children(0, &[row]);
        labels.row = Some(row);
        labels.built = false;
    }
}

fn hash(sig: &mut u64, v: u64) {
    *sig = (*sig ^ v).wrapping_mul(1099511628211);
}

/// What the cards show: per display slot (power, tier, form, active), drained, the pickup, the
/// power key.
fn signature(gems: &[Gem], drained: bool, pickup: Option<GateKind>, key: &str, moves: &[Option<super::borrowed::Chip>]) -> u64 {
    let mut sig = 0xcbf2_9ce4_8422_2325u64;
    for c in moves {
        hash(&mut sig, c.map_or(0, |c| (c.mv as *const _ as u64) ^ (c.glyph as u64) << 56));
    }
    for g in gems {
        hash(&mut sig, g.power.map_or(0, |p| p as u64 + 1) << 16 | (g.tier as u64) << 8 | (g.transformed as u64) << 1 | g.active as u64);
    }
    hash(&mut sig, drained as u64 | pickup.map_or(0, |k| k as u64 + 1) << 4);
    hash(&mut sig, key.bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64)));
    sig
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    focus: Res<crate::input::UiFocus>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    st: Res<PowerHudState>,
    borrowed: Option<Res<super::borrowed::BorrowedHud>>,
    mut labels: ResMut<Labels>,
    mut mats: ResMut<Assets<IconMaterial>>,
    mut nodes: Query<&mut Node, With<LabelRow>>,
    mut faded: Query<(&Fade, Option<&mut TextColor>, Option<&mut BackgroundColor>, Option<&mut BorderColor>)>,
) {
    let Some(row) = labels.row else { return };
    let hud = &st.0;
    let tab = (!focus.typing && binds.pressed(Action::Scoreboard, &keys, &mouse)) || tab_test();
    let pickup = hud.pickup.map(|p| p.0);
    let moves: &[Option<super::borrowed::Chip>] = borrowed.as_ref().map_or(&[], |b| &b.chips[..]);
    let has_moves = moves.iter().any(|c| c.is_some());
    let want = tab && *state.get() == AppState::InGame && !menu.0 && (hud.count > 0 || pickup.is_some() || has_moves);
    let dt = time.delta_secs().clamp(0.0, 0.1);
    labels.fade = if want { (labels.fade + dt / FADE_IN).min(1.0) } else { (labels.fade - dt / FADE_OUT).max(0.0) };
    let visible = labels.fade > 0.0;
    if let Ok(mut n) = nodes.get_mut(row) {
        let d = if visible { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    }
    if !visible {
        return;
    }
    // Rebuild on change (only while it shows; fading out keeps the last cards).
    let key = binds.get(Action::Power).label();
    let sig = signature(&hud.gems, hud.drained, pickup, &key, moves);
    if want && (!labels.built || sig != labels.sig) {
        labels.sig = sig;
        labels.built = true;
        labels.mats.clear();
        commands.entity(row).despawn_related::<Children>();
        let fade = labels.fade;
        let mut cx = Cx { fade, mats: &mut mats, labels: &mut labels, key: &key, drained: hud.drained };
        commands.entity(row).with_children(|r| {
            // In the strip's slot order (the icons never jump around; neither do the cards).
            for (k, g) in hud.gems.iter().enumerate() {
                let Some(id) = g.power else { continue };
                let Some(def) = bd_sim::powers::PowerTable::get().def(id) else { continue };
                let (label, form) = power_label(def, g.tier, g.transformed);
                let hints = power_hints(def, id, g.tier, g.transformed, if g.active { 0 } else { 1 });
                power_card(r, &mut cx, k, &label, form, g.look, &hints);
            }
            if let Some(kind) = pickup {
                pickup_card(r, &mut cx, kind);
            }
            if has_moves {
                moves_card(r, &mut cx, moves);
            }
        });
        labels.applied = fade;
        return;
    }
    // Fade: rewrite alphas only while it moves.
    if (labels.fade - labels.applied).abs() > 1e-3 {
        let f = labels.fade;
        labels.applied = f;
        for (fd, t, bg, b) in &mut faded {
            if let (Some(c), Some(mut t)) = (fd.text, t) {
                t.0 = c.with_alpha(c.alpha() * f);
            }
            if let (Some(c), Some(mut bg)) = (fd.bg, bg) {
                bg.0 = c.with_alpha(c.alpha() * f);
            }
            if let (Some(c), Some(mut b)) = (fd.border, b) {
                *b = BorderColor::all(c.with_alpha(c.alpha() * f));
            }
        }
        let all: Vec<(Handle<IconMaterial>, f32)> = labels.mats.iter().cloned().chain(labels.verb_mats.iter().map(|h| (h.clone(), VERB_ALPHA))).collect();
        for (h, a) in all {
            if let Some(mut m) = mats.get_mut(&h) {
                m.params.color.w = a * f;
            }
        }
    }
}

/// Build context.
struct Cx<'a, 'b> {
    fade: f32,
    mats: &'a mut Assets<IconMaterial>,
    labels: &'b mut Labels,
    key: &'a str,
    drained: bool,
}

const VERB_ALPHA: f32 = 0.9;
const VERB_RGB: [f32; 3] = [0.78, 0.84, 0.95];

impl Cx<'_, '_> {
    fn text(&self, r: &mut ChildSpawnerCommands, s: &str, size: f32, c: Color) {
        r.spawn((Text::new(s.to_string()), TextFont::from_font_size(size), TextColor(c.with_alpha(c.alpha() * self.fade)), Fade { text: Some(c), ..default() }, Pickable::IGNORE));
    }

    fn icon(&mut self, p: IconParams) -> Handle<IconMaterial> {
        let a = p.color.w;
        let h = self.mats.add(IconMaterial { params: IconParams { color: Vec4::new(p.color.x, p.color.y, p.color.z, a * self.fade), ..p } });
        self.labels.mats.push((h.clone(), a));
        h
    }

    fn verb_mat(&mut self, v: Verb) -> Handle<IconMaterial> {
        if self.labels.verb_mats.is_empty() {
            let mats = &mut *self.mats;
            self.labels.verb_mats = Verb::ALL
                .iter()
                .map(|v| {
                    mats.add(IconMaterial {
                        params: IconParams {
                            color: Vec4::new(VERB_RGB[0], VERB_RGB[1], VERB_RGB[2], VERB_ALPHA),
                            a: Vec4::new(v.glyph() as f32, 1.0, -1.0, 0.0),
                            b: Vec4::new(1.0, 0.0, 0.0, 0.0),
                            c: Vec4::new(1.0, 1.0, 1.0, 0.0),
                        },
                    })
                })
                .collect();
        }
        let k = Verb::ALL.iter().position(|x| *x == v).unwrap_or(0);
        let h = self.labels.verb_mats[k].clone();
        if let Some(mut m) = self.mats.get_mut(&h) {
            m.params.color.w = VERB_ALPHA * self.fade;
        }
        h
    }
}

fn lin(c: [f32; 3]) -> Color {
    Color::linear_rgb(c[0], c[1], c[2])
}

/// The card frame: dark, an accent underline in the power's colour.
fn card<'a>(r: &'a mut ChildSpawnerCommands, cx: &Cx, accent: Color, width: Option<f32>) -> EntityCommands<'a> {
    let bg = Color::srgba(0.0, 0.01, 0.03, 0.78);
    let border = accent.with_alpha(0.85);
    r.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            padding: UiRect::new(px(7), px(7), px(4), px(5)),
            border: UiRect::bottom(px(2)),
            border_radius: BorderRadius::all(px(3)),
            width: width.map_or(Val::Auto, |w| px(w)),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(bg.with_alpha(bg.alpha() * cx.fade)),
        BorderColor::all(border.with_alpha(border.alpha() * cx.fade)),
        Fade { bg: Some(bg), border: Some(border), ..default() },
        Pickable::IGNORE,
    ))
}

#[allow(clippy::too_many_arguments)]
fn power_card(r: &mut ChildSpawnerCommands, cx: &mut Cx, k: usize, label: &str, form: bool, look: derive::Look, hints: &[Hint]) {
    let accent = lin(if form { look.form_color } else { look.color });
    card(r, cx, accent, Some(CARD_W)).with_children(|c| {
        // Name row: the icon's chip + the name (a form burns in its aura colour).
        c.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(5), ..default() }, Pickable::IGNORE)).with_children(|n| {
            let (col, f) = (look.color, look.form_color);
            let h = cx.icon(IconParams {
                color: Vec4::new(col[0], col[1], col[2], 1.0),
                a: Vec4::new(look.glyph as f32, 1.0, if form { 1.0 } else { -1.0 }, 0.0),
                b: Vec4::new(0.0, 0.0, cx.drained as u8 as f32, 0.0),
                c: Vec4::new(f[0], f[1], f[2], k as f32 * 1.7),
            });
            n.spawn((Node { width: px(CHIP), height: px(CHIP), flex_shrink: 0.0, ..default() }, MaterialNode(h), Pickable::IGNORE));
            let tc = if cx.drained {
                Color::srgba(0.6, 0.6, 0.6, 0.7)
            } else if form {
                lin(look.form_color).mix(&Color::WHITE, 0.3)
            } else {
                Color::WHITE.with_alpha(0.92)
            };
            cx.text(n, label, 13.0, tc);
        });
        // Usage: input + verb per move, then passives (wraps inside the card).
        c.spawn((Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(7), row_gap: px(3), ..default() }, Pickable::IGNORE)).with_children(|u| {
            for h in hints {
                u.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(3), flex_shrink: 0.0, ..default() }, Pickable::IGNORE)).with_children(|g| {
                    input_chip(g, cx, h.input);
                    verb_chip(g, cx, h.verb);
                });
            }
        });
    });
}

fn pickup_card(r: &mut ChildSpawnerCommands, cx: &mut Cx, kind: GateKind) {
    let (col, glyph) = derive::pickup_look(kind);
    let name = match kind {
        GateKind::Giant => "GIANT",
        GateKind::Awakening => "AWAKENING",
        GateKind::Ultimate => "ULTIMATE",
    };
    card(r, cx, lin(col), None).with_children(|c| {
        c.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(5), ..default() }, Pickable::IGNORE)).with_children(|n| {
            let h = cx.icon(IconParams { color: Vec4::new(col[0], col[1], col[2], 1.0), a: Vec4::new(glyph as f32, 1.0, -1.0, 0.0), b: Vec4::new(3.0, 0.0, 0.0, 0.0), c: Vec4::new(1.0, 1.0, 1.0, 0.0) });
            n.spawn((Node { width: px(CHIP), height: px(CHIP), flex_shrink: 0.0, ..default() }, MaterialNode(h), Pickable::IGNORE));
            cx.text(n, name, 13.0, Color::WHITE.with_alpha(0.92));
        });
    });
}

/// The borrowed moves on the strip's chips: one row each (the chip's glyph in its colour + the
/// move's name), in chip order.
fn moves_card(r: &mut ChildSpawnerCommands, cx: &mut Cx, chips: &[Option<super::borrowed::Chip>]) {
    card(r, cx, Color::srgba(0.85, 0.9, 1.0, 0.8), None).with_children(|c| {
        for (k, ch) in chips.iter().enumerate() {
            let Some(ch) = ch else { continue };
            c.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(5), ..default() }, Pickable::IGNORE)).with_children(|n| {
                let col = ch.color;
                let h = cx.icon(IconParams {
                    color: Vec4::new(col[0], col[1], col[2], 1.0),
                    a: Vec4::new(ch.glyph as f32, 1.0, -1.0, 0.0),
                    b: Vec4::new(1.0, 0.0, 0.0, 0.0),
                    c: Vec4::new(1.0, 1.0, 1.0, 3.1 + k as f32),
                });
                n.spawn((Node { width: px(CHIP), height: px(CHIP), flex_shrink: 0.0, ..default() }, MaterialNode(h), Pickable::IGNORE));
                cx.text(n, &clip(&ch.mv.name.to_uppercase(), TAG_CHARS), 12.0, Color::WHITE.with_alpha(0.92));
            });
        }
    });
}

/// A verb chip: glyph + word.
fn verb_chip(r: &mut ChildSpawnerCommands, cx: &mut Cx, v: Verb) {
    let bg = Color::srgba(0.75, 0.85, 1.0, 0.09);
    let h = cx.verb_mat(v);
    r.spawn((
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(2), padding: UiRect::new(px(2), px(5), px(0), px(0)), height: px(15), flex_shrink: 0.0, border_radius: BorderRadius::all(px(3)), ..default() },
        BackgroundColor(bg.with_alpha(bg.alpha() * cx.fade)),
        Fade { bg: Some(bg), ..default() },
        Pickable::IGNORE,
    ))
    .with_children(|c| {
        c.spawn((Node { width: px(12), height: px(12), flex_shrink: 0.0, ..default() }, MaterialNode(h), Pickable::IGNORE));
        cx.text(c, v.word(), 10.0, Color::srgba(0.85, 0.9, 1.0, 0.9));
    });
}

/// The input: a keycap (your actual power bind), "HOLD" + keycap, a speech bubble (shout: say or
/// type it), or "ALWAYS".
fn input_chip(r: &mut ChildSpawnerCommands, cx: &mut Cx, i: Input) {
    let f = cx.fade;
    let keycap = |r: &mut ChildSpawnerCommands, cx: &Cx| {
        let (border, bg) = (Color::srgba(1.0, 1.0, 1.0, 0.6), Color::srgba(1.0, 1.0, 1.0, 0.08));
        r.spawn((
            Node { padding: UiRect::axes(px(3), px(0)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(2)), height: px(14), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() },
            BorderColor::all(border.with_alpha(border.alpha() * f)),
            BackgroundColor(bg.with_alpha(bg.alpha() * f)),
            Fade { bg: Some(bg), border: Some(border), ..default() },
            Pickable::IGNORE,
        ))
        .with_children(|k| cx.text(k, cx.key, 10.0, Color::WHITE.with_alpha(0.9)));
    };
    let dim = Color::srgba(1.0, 1.0, 1.0, 0.55);
    match i {
        Input::Tap => keycap(r, cx),
        Input::Hold => {
            cx.text(r, "HOLD", 9.0, dim);
            keycap(r, cx);
        }
        Input::Always => cx.text(r, "ALWAYS", 9.0, dim),
        Input::Shout => {
            // A speech bubble: rounded body, three dots, a tail at the bottom-left.
            let white = Color::srgba(1.0, 1.0, 1.0, 0.85);
            let ink = Color::srgba(0.0, 0.02, 0.06, 0.9);
            r.spawn((Node { width: px(16), height: px(15), flex_shrink: 0.0, ..default() }, Pickable::IGNORE)).with_children(|b| {
                b.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(1),
                        width: px(16),
                        height: px(10),
                        border_radius: BorderRadius::all(px(4)),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        column_gap: px(2),
                        ..default()
                    },
                    BackgroundColor(white.with_alpha(white.alpha() * f)),
                    Fade { bg: Some(white), ..default() },
                    Pickable::IGNORE,
                ))
                .with_children(|d| {
                    for _ in 0..3 {
                        d.spawn((Node { width: px(2), height: px(2), ..default() }, BackgroundColor(ink.with_alpha(ink.alpha() * f)), Fade { bg: Some(ink), ..default() }, Pickable::IGNORE));
                    }
                });
                b.spawn((
                    Node { position_type: PositionType::Absolute, left: px(3), top: px(10), width: px(4), height: px(4), ..default() },
                    BackgroundColor(white.with_alpha(white.alpha() * f)),
                    Fade { bg: Some(white), ..default() },
                    Pickable::IGNORE,
                ));
            });
        }
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
    fn word(self) -> &'static str {
        match self {
            Verb::Projectile => "PROJECTILE",
            Verb::Beam => "BEAM",
            Verb::Blast => "BLAST",
            Verb::Strike => "STRIKE",
            Verb::Pull => "PULL",
            Verb::Dash => "DASH",
            Verb::Shield => "SHIELD",
            Verb::Dodge => "DODGE",
            Verb::Summon => "SUMMON",
            Verb::Form => "FORM",
            Verb::Aura => "AURA",
            Verb::Grab => "GRAB",
            Verb::Trap => "TRAP",
        }
    }

    /// Its tiny glyph (the power-icon SDF set; each verb its own shape).
    fn glyph(self) -> u8 {
        use crate::power_hud::derive::glyph as g;
        match self {
            Verb::Projectile => g::CIRCLE,
            Verb::Beam => g::DIAMOND,
            Verb::Blast => g::STAR4,
            Verb::Strike => g::BOLT,
            Verb::Pull => g::CRESCENT,
            Verb::Dash => g::CHEVRON_UP,
            Verb::Shield => g::HEXAGON,
            Verb::Dodge => g::CHEVRON_DOWN,
            Verb::Summon => g::CLAW,
            Verb::Form => g::TRIANGLE,
            Verb::Aura => g::RING,
            Verb::Grab => g::CROSS,
            Verb::Trap => g::X,
        }
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Hint {
    input: Input,
    verb: Verb,
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
        O::Deflect { .. } | O::Counter { .. } => Verb::Shield,
        O::Lockout { .. } | O::Zone { .. } => Verb::Trap,
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

/// How power `def` (at `tier`, `transformed`, in held `slot`: 0 = active) is used: per move its
/// input and verb — the active power's first two moves are the power key (tap / hold), the rest
/// (and another held power's moves, when they have a shout) are shouted — then a FORM shout for a
/// tiered power, then its passives ("always"). Derived from the data alone.
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
        out.push(Hint { input, verb });
    }
    // Tiers are picked by saying their name.
    if !def.tiers.is_empty() && !out.iter().any(|h| h.verb == Verb::Form) {
        out.push(Hint { input: Input::Shout, verb: Verb::Form });
    }
    // Passives: a stand / clone at your side, then effects (one chip per verb).
    let view = bd_sim::powers::HeldView { id, def, tier, transformed, slot };
    let passives = def.companion.as_ref().map(|_| Verb::Summon).into_iter().chain(view.effects().map(effect_verb));
    for verb in passives {
        let h = Hint { input: Input::Always, verb };
        if !out.contains(&h) {
            out.push(h);
        }
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
            [format!("{name} · {f}"), format!("{short} · {f}")].into_iter().find(|s| fits(s)).unwrap_or(f)
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
        assert_eq!(label("gomu_gomu", 4, false), ("GOMU GOMU NO MI · GEAR FIFTH".to_string(), true));
        // A transform: its title, with the model's short name (Gear 5).
        assert_eq!(label("nika", 0, true), ("NIKA · GEAR FIFTH".to_string(), true));
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
        // pull + truth-seeking orbs = shouts, and its passive read.
        assert_eq!(hints("rinnegan", 0, false, 0), vec![(Tap, Blast), (Hold, Pull), (Shout, Pull), (Shout, Summon), (Always, Aura)]);
        // Kamehameha-bearers: a beam on hold; tiers are shouted forms; buffs are always on.
        assert_eq!(hints("super_saiyan", 0, false, 0), vec![(Tap, Dash), (Hold, Beam), (Shout, Form), (Always, Aura)]);
        assert_eq!(hints("ultra_instinct", 0, false, 0), vec![(Tap, Dash), (Hold, Beam), (Shout, Form), (Always, Dodge), (Always, Aura)]);
        // Gear 5 (Nika): the transform is its hold.
        assert_eq!(hints("nika", 0, false, 0), vec![(Tap, Projectile), (Hold, Form)]);
        // Shadow clones: summon on tap, rasengan on hold, the clone at your side always.
        assert_eq!(hints("shadow_clone", 0, false, 0), vec![(Tap, Summon), (Hold, Projectile), (Always, Summon)]);
        // Not the active power: its moves are shouted (the key fires the active one).
        assert_eq!(hints("super_saiyan", 0, false, 1)[..2], [(Shout, Dash), (Shout, Beam)]);
        // Every verb has its own glyph.
        for (i, a) in Verb::ALL.iter().enumerate() {
            for b in &Verb::ALL[i + 1..] {
                assert_ne!(a.glyph(), b.glyph(), "{a:?} / {b:?}");
            }
        }
        // Every power yields at least one hint.
        for (i, d) in PowerTable::get().powers.iter().enumerate() {
            assert!(!power_hints(d, i as u16, 0, false, 0).is_empty(), "{} has no hint", d.id);
        }
    }

    #[test]
    fn signature_tracks_what_is_shown() {
        let mut g = [Gem::default(); derive::SLOTS];
        let a = signature(&g, false, None, "G", &[]);
        g[0].power = Some(3);
        let b = signature(&g, false, None, "G", &[]);
        g[0].tier = 2;
        let c = signature(&g, false, None, "G", &[]);
        assert!(a != b && b != c);
        assert_ne!(c, signature(&g, false, None, "F", &[]), "a rebind shows");
        assert_ne!(c, signature(&g, true, None, "G", &[]));
        assert_ne!(c, signature(&g, false, Some(GateKind::Giant), "G", &[]));
        g[0].ready = 0.3;
        assert_eq!(c, signature(&g, false, None, "G", &[]), "cooldowns don't rebuild");
        // A borrowed move on a chip adds / changes the moves card.
        let mv: &'static bd_sim::powers::MoveDef = Box::leak(Box::new(bd_sim::powers::MoveDef { name: "pistol".into(), ..Default::default() }));
        let chip = super::super::borrowed::Chip { mv, key: 0, color: [1.0; 3], glyph: 2, cd: 1.0, left: 0.0, flash: 0.0, stale: 0, idle: 0.0 };
        assert_ne!(c, signature(&g, false, None, "G", &[Some(chip)]), "a borrowed move shows");
        let worn = super::super::borrowed::Chip { stale: 2, left: 0.5, ..chip };
        assert_eq!(signature(&g, false, None, "G", &[Some(chip)]), signature(&g, false, None, "G", &[Some(worn)]), "wear / cooldown don't rebuild");
    }
}
