//! Tab scoreboard (hold): minimal — corps bar, name, KOs, falls (and party points / stock lives),
//! match clock + scoring line ("STOCK · 3 LIVES" / "FIRST TO 10 KOS").
//! Rebuilt only while held and only when the numbers change.

use crate::console::corps_color;
use crate::lobby::prefs::{Action, KeyBinds};
use crate::lobby::UiSet;
use crate::sim_bridge::ActiveWorld;
use crate::state::{AppState, InGameMenu};
use bevy::picking::Pickable;
use bevy::prelude::*;

#[derive(Component)]
struct BoardRoot;
#[derive(Component)]
struct CodeTag;

pub struct ScoreboardPlugin;

impl Plugin for ScoreboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_code_tag).add_systems(Update, (draw_board, draw_code_tag).in_set(UiSet::Draw));
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_board(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    focus: Res<crate::input::UiFocus>,
    active: Option<Res<ActiveWorld>>,
    roots: Query<Entity, With<BoardRoot>>,
    mut last_sig: Local<u64>,
) {
    let held = *state.get() == AppState::InGame && !menu.0 && !focus.typing && binds.pressed(Action::Scoreboard, &keys, &mouse);
    let Some(a) = active.filter(|_| held) else {
        if !roots.is_empty() {
            for e in &roots {
                commands.entity(e).despawn();
            }
            *last_sig = 0;
        }
        return;
    };
    let w = &a.world;
    let mut rows = w.scoreboard();
    let secs = w.time_left().max(0.0) as u64;
    let stock = match w.rules().scoring {
        bd_sim::rules::Scoring::Stock { lives } if w.party().is_none() => Some(lives.max(1)),
        _ => None,
    };
    let lives_of = |id: bd_core::PlayerId| w.player(id).map(|p| if p.eliminated { 0 } else { p.stocks.unwrap_or(0) });
    if stock.is_some() {
        // Stock: most lives first (eliminated last), then the usual KO order.
        rows.sort_by_key(|r| std::cmp::Reverse(lives_of(r.player).unwrap_or(0)));
    }
    // Cheap signature: rebuild only when something shown changed.
    let mut sig: u64 = 1469598103934665603 ^ secs;
    for r in &rows {
        sig = (sig ^ ((r.player.0 as u64) << 32 | (r.kos as u64) << 16 | r.deaths as u64)).wrapping_mul(1099511628211);
        sig = (sig ^ lives_of(r.player).unwrap_or(0) as u64).wrapping_mul(1099511628211);
    }
    if let Some(p) = w.party() {
        for s in &p.players {
            sig = (sig ^ s.points as u64).wrapping_mul(1099511628211);
        }
    }
    if sig == *last_sig && !roots.is_empty() {
        return;
    }
    *last_sig = sig;
    for e in &roots {
        commands.entity(e).despawn();
    }
    let party = w.party();
    commands
        .spawn((
            Name::new("Scoreboard"),
            BoardRoot,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(18),
                width: px(520),
                margin: UiRect::left(px(-260)),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::all(px(14)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.01, 0.03, 0.72)),
            Pickable::IGNORE,
            GlobalZIndex(75),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new(format!("{}:{:02}", secs / 60, secs % 60)),
                TextFont::from_font_size(16.0),
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.55)),
                Node { align_self: AlignSelf::Center, ..default() },
            ));
            let scoring = match (stock, party) {
                (Some(l), _) => format!("STOCK · {l} {}", if l == 1 { "LIFE" } else { "LIVES" }),
                (None, Some(_)) => "PARTY".to_string(),
                (None, None) => format!("FIRST TO {} KOS", w.rules().ko_target),
            };
            root.spawn((
                Text::new(scoring),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.35)),
                Node { margin: UiRect::bottom(px(6)), align_self: AlignSelf::Center, ..default() },
            ));
            let header = |r: &mut ChildSpawnerCommands| {
                r.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(10), ..default() }).with_children(|h| {
                    h.spawn(Node { width: px(4), ..default() });
                    h.spawn((Text::new(""), Node { flex_grow: 1.0, ..default() }));
                    for label in stock.map(|_| "LIVES").into_iter().chain(["KO", "FALLS"]).chain(party.map(|_| "PTS")) {
                        h.spawn((Text::new(label), TextFont::from_font_size(12.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.4)), Node { width: px(56), ..default() }, TextLayout::justify(Justify::Right)));
                    }
                });
            };
            header(root);
            for r in &rows {
                let (name, corps) = w.player(r.player).map_or((format!("P{}", r.player.0), bd_core::corps::Corps::from_index(r.player.0)), |p| (p.name.clone(), p.corps));
                let c = corps_color(corps);
                let me = a.local == Some(r.player);
                let pts = party.and_then(|p| p.player(r.player)).map(|p| p.points);
                root.spawn((
                    Node { flex_direction: FlexDirection::Row, column_gap: px(10), align_items: AlignItems::Center, padding: UiRect::vertical(px(3)), ..default() },
                    BackgroundColor(if me { c.with_alpha(0.10) } else { Color::NONE }),
                ))
                .with_children(|row| {
                    row.spawn((Node { width: px(4), height: px(22), ..default() }, BackgroundColor(c)));
                    row.spawn((Text::new(name.to_uppercase()), TextFont::from_font_size(19.0), TextColor(Color::WHITE.with_alpha(if me { 1.0 } else { 0.8 })), Node { flex_grow: 1.0, ..default() }));
                    let lives = stock.and(lives_of(r.player)).map(|l| if l == 0 { "OUT".to_string() } else { l.to_string() });
                    for v in lives.into_iter().chain([Some(r.kos), Some(r.deaths), pts].into_iter().flatten().map(|v| v.to_string())) {
                        row.spawn((Text::new(v), TextFont::from_font_size(19.0), TextColor(c.mix(&Color::WHITE, 0.5)), Node { width: px(56), ..default() }, TextLayout::justify(Justify::Right)));
                    }
                });
            }
        });
}

fn spawn_code_tag(mut commands: Commands) {
    commands.spawn((
        Name::new("LobbyCodeTag"),
        CodeTag,
        Text::new(""),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
        TextShadow { offset: Vec2::new(0.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.6) },
        Node { position_type: PositionType::Absolute, right: px(18), top: px(14), ..default() },
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(40),
    ));
}

/// Host during warmup (or while Tab is held): the lobby code, small, top-right.
#[allow(clippy::too_many_arguments)]
fn draw_code_tag(
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    net: Option<Res<crate::net::NetStatus>>,
    active: Option<Res<ActiveWorld>>,
    mut q: Query<(&mut Text, &mut Visibility), With<CodeTag>>,
) {
    let Ok((mut t, mut vis)) = q.single_mut() else { return };
    let code = net.as_ref().filter(|n| n.role == crate::net::NetRole::Host).and_then(|n| n.lobby_code.clone().or_else(|| n.lan.first().cloned()));
    let warmup = active.as_ref().is_some_and(|a| a.world.match_state.phase == bd_sim::MatchPhase::Warmup);
    let tab = binds.pressed(Action::Scoreboard, &keys, &mouse);
    let show = *state.get() == AppState::InGame && !menu.0 && code.is_some() && (warmup || tab);
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if let Some(code) = code.filter(|_| show) {
        let s = code.to_uppercase();
        if t.0 != s {
            t.0 = s;
        }
    }
}
