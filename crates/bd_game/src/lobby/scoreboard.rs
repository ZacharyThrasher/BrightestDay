//! Tab scoreboard (hold) + the end-of-match screen.
//!
//! * A compact competitive stats table, one row per player sorted by placement: lives / points,
//!   KOs, falls, self-KOs, damage dealt / taken, best KO streak, longest combo, accuracy,
//!   constructs cast, ping. Numbers only (no awards, no phrases): KOs / falls / lives / points come
//!   from the replicated world, the rest from `bd_sim::match_stats` (host-authoritative, carried to
//!   clients in the lobby info; practice observes its own world). The local row is tinted, the
//!   leader wears a small voxel crown.
//! * Once a match is over: the next-map vote (rules: `net::map_vote`) — a card per offered map
//!   (default 2) + RANDOM, votes as pips in each voter's corps colour (hidden until I voted: until
//!   then only who voted shows, as filled squares by the countdown), my locked pick stamped,
//!   RANDOM's roulette when it wins, a countdown the host can click to start now, and a host RULES
//!   button (the Match Rules page; the countdown waits while it is open). Practice: every map, a
//!   click is the pick (the same map = rematch).
//!
//! Rebuilt only when something shown changes.
//!
//! Dev (`--screenshot` runs): `BD_MATCH_SECS=<s>` ends an authoritative (host / practice) live
//! match after `s` seconds (a real timeout); `BD_TAB_TEST=1` shows the board as if Tab were held.

use crate::console::corps_color;
use crate::lobby::prefs::{Action, KeyBinds};
use crate::lobby::UiSet;
use crate::net::map_vote;
use crate::net::proto::{NetMapKind, VotePick};
use crate::net::{NetRequest, NetRole, NetStatus};
use crate::sim_bridge::{ActiveWorld, SimRole};
use crate::state::{AfterLoading, AppState, InGameMenu};
use crate::tick::{SimSet, SimTick};
use bd_core::PlayerId;
use bd_sim::match_stats::{MatchStats, StatLine};
use bevy::picking::Pickable;
use bevy::prelude::*;

#[derive(Component)]
struct BoardRoot;
#[derive(Component)]
struct VoteRoot;
#[derive(Component)]
struct CodeTag;
#[derive(Component)]
struct MapCard(VotePick);
/// Host: start the next match now.
#[derive(Component)]
struct GoNow;
/// Host: open the Match Rules page.
#[derive(Component)]
struct RulesButton;
#[derive(Component)]
struct CountText;

/// Practice (offline) stats: the host keeps its own in `HostCore`, clients get the host's.
#[derive(Resource, Default)]
struct LocalStats(MatchStats);

const GOLD: Color = Color::srgb(1.0, 0.8, 0.25);
/// Stat columns: header, width.
const COL_W: f32 = 54.0;
const CARD_W: f32 = 118.0;
const CARD_H: f32 = 56.0;
const CARDS_PER_ROW: usize = 7;
/// Cards when only a few are on offer (the default: 2 + RANDOM).
const BIG_CARD_W: f32 = 196.0;
const BIG_CARD_H: f32 = 104.0;

pub struct ScoreboardPlugin;

impl Plugin for ScoreboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::lobby::feed::FeedPlugin) // FREEDOM: kill feed
            .init_resource::<LocalStats>()
            .init_resource::<MyPick>()
            .add_systems(Startup, spawn_code_tag)
            .add_systems(SimTick, observe_local.in_set(SimSet::Post))
            .add_systems(OnExit(AppState::InGame), |mut s: ResMut<LocalStats>| s.0 = MatchStats::default())
            .add_systems(Update, (vote_clicks, draw_board, draw_vote, tick_count, animate_vote, draw_code_tag).chain().in_set(UiSet::Draw))
            .add_systems(Update, dev_match_secs.run_if(|| env_f32("BD_MATCH_SECS").is_some()));
    }
}

fn env_f32(name: &'static str) -> Option<f32> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Vec<(&'static str, Option<f32>)>>> = std::sync::OnceLock::new();
    let m = CACHE.get_or_init(Default::default);
    let mut m = m.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, v)) = m.iter().find(|(n, _)| *n == name) {
        return *v;
    }
    let v = std::env::var(name).ok().and_then(|v| v.parse().ok());
    m.push((name, v));
    v
}

/// `BD_MATCH_SECS`: a live authoritative match times out after that many seconds.
fn dev_match_secs(active: Option<ResMut<ActiveWorld>>) {
    let (Some(mut a), Some(secs)) = (active, env_f32("BD_MATCH_SECS")) else { return };
    if a.role != SimRole::Client && a.world.match_state.live() && a.world.cfg.rules.time_limit != secs {
        a.world.cfg.rules.time_limit = secs;
    }
}

fn observe_local(active: Option<Res<ActiveWorld>>, mut stats: ResMut<LocalStats>) {
    if let Some(a) = active.filter(|a| a.role == SimRole::Offline) {
        stats.0.observe_events(&a.tick_events, &a.world);
    }
}

/// Is the match over (FFA results / party podium)?
fn match_over(w: &bd_sim::World) -> bool {
    crate::net::host::match_over(w)
}

/// Placement order: party points, else stock lives, else the FFA scoreboard (KOs, falls, id).
fn placement(w: &bd_sim::World) -> Vec<PlayerId> {
    let mut rows = w.scoreboard();
    let lives_of = |id: PlayerId| w.player(id).map_or(0, |p| if p.eliminated { 0 } else { p.stocks.unwrap_or(0) });
    if let Some(p) = w.party() {
        rows.sort_by_key(|r| std::cmp::Reverse(p.player(r.player).map_or(0, |s| s.points)));
    } else if matches!(w.rules().scoring, bd_sim::rules::Scoring::Stock { .. }) {
        rows.sort_by_key(|r| std::cmp::Reverse(lives_of(r.player)));
    }
    rows.into_iter().map(|r| r.player).collect()
}

fn stat_lines(a: &ActiveWorld, net: Option<&NetStatus>, local: &LocalStats) -> Vec<StatLine> {
    match a.role {
        SimRole::Offline => local.0.lines_for(&a.world),
        _ => net.map(|n| n.match_stats.clone()).unwrap_or_default(),
    }
}

fn hash(sig: &mut u64, v: u64) {
    *sig = (*sig ^ v).wrapping_mul(1099511628211);
}

/// A tiny voxel crown (the leader's mark).
fn crown(r: &mut ChildSpawnerCommands) {
    r.spawn((Node { width: px(14), height: px(12), flex_direction: FlexDirection::Column, justify_content: JustifyContent::FlexEnd, ..default() }, Pickable::IGNORE))
        .with_children(|c| {
            c.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, height: px(4), ..default() }).with_children(|t| {
                for _ in 0..3 {
                    t.spawn((Node { width: px(3), height: px(4), ..default() }, BackgroundColor(GOLD)));
                }
            });
            c.spawn((Node { height: px(5), ..default() }, BackgroundColor(GOLD)));
        });
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
    net: Option<Res<NetStatus>>,
    local: Res<LocalStats>,
    roots: Query<Entity, With<BoardRoot>>,
    mut last_sig: Local<u64>,
) {
    // The board doubles as the results screen while an FFA match is over (party has its podium).
    let ended = active.as_ref().is_some_and(|a| a.world.match_state.phase == bd_sim::MatchPhase::Ended && a.world.party().is_none());
    let tab = (!focus.typing && binds.pressed(Action::Scoreboard, &keys, &mouse)) || env_f32("BD_TAB_TEST").is_some_and(|v| v > 0.0);
    let held = *state.get() == AppState::InGame && !menu.0 && (tab || ended);
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
    let order = placement(w);
    let lines = stat_lines(&a, net.as_deref(), &local);
    let online = net.as_ref().is_some_and(|n| n.role != NetRole::Offline) && a.role != SimRole::Offline;
    let stock = match w.rules().scoring {
        bd_sim::rules::Scoring::Stock { lives } if w.party().is_none() => Some(lives.max(1)),
        _ => None,
    };
    let party = w.party();
    let secs = w.time_left().max(0.0) as u64;
    let lives_of = |id: PlayerId| w.player(id).map(|p| if p.eliminated { 0 } else { p.stocks.unwrap_or(0) });
    let ping_of = |id: PlayerId| net.as_ref().and_then(|n| n.players.iter().find(|p| p.id == id)).map(|p| if p.is_host { None } else { Some(p.rtt_ms) });
    let line_of = |id: PlayerId| lines.iter().find(|l| l.player == id).copied().unwrap_or(StatLine { player: id, ..Default::default() });
    // Cheap signature: rebuild only when something shown changed.
    let mut sig: u64 = 1469598103934665603 ^ (if ended { 0 } else { secs }) ^ ((ended as u64) << 40) ^ ((online as u64) << 41);
    for &id in &order {
        let p = w.player(id);
        hash(&mut sig, (id.0 as u64) << 32 | (p.map_or(0, |p| p.kos) as u64) << 16 | p.map_or(0, |p| p.deaths) as u64);
        hash(&mut sig, lives_of(id).unwrap_or(0) as u64 | (party.and_then(|q| q.player(id)).map_or(0, |s| s.points) as u64) << 8);
        let l = line_of(id);
        hash(&mut sig, (l.dealt as u64) << 32 | l.taken as u64);
        hash(&mut sig, (l.hits as u64) << 32 | l.attacks as u64);
        hash(&mut sig, (l.self_kos as u64) << 48 | (l.best_streak as u64) << 32 | (l.best_combo as u64) << 16 | l.constructs as u64);
        hash(&mut sig, ping_of(id).flatten().map_or(0, |p| p as u64 / 5 + 1));
    }
    if sig == *last_sig && !roots.is_empty() {
        return;
    }
    *last_sig = sig;
    for e in &roots {
        commands.entity(e).despawn();
    }
    // Columns: (header, value per player).
    type Col<'a> = (&'static str, Box<dyn Fn(PlayerId) -> String + 'a>);
    let dash = || "-".to_string();
    let mut cols: Vec<Col> = Vec::new();
    if stock.is_some() {
        cols.push(("LIVES", Box::new(|id| match lives_of(id) {
            Some(0) | None => "OUT".into(),
            Some(l) => l.to_string(),
        })));
    }
    if let Some(p) = party {
        cols.push(("PTS", Box::new(move |id| p.player(id).map_or(0, |s| s.points).to_string())));
    }
    cols.push(("KO", Box::new(|id| w.player(id).map_or(0, |p| p.kos).to_string())));
    cols.push(("FALL", Box::new(|id| w.player(id).map_or(0, |p| p.deaths).to_string())));
    cols.push(("SELF", Box::new(|id| line_of(id).self_kos.to_string())));
    cols.push(("DMG", Box::new(|id| line_of(id).dealt.to_string())));
    cols.push(("TAKEN", Box::new(|id| line_of(id).taken.to_string())));
    cols.push(("STRK", Box::new(|id| line_of(id).best_streak.to_string())));
    cols.push(("COMBO", Box::new(|id| line_of(id).best_combo.to_string())));
    cols.push(("ACC", Box::new(move |id| line_of(id).accuracy().map_or_else(dash, |a| format!("{a}%")))));
    cols.push(("CAST", Box::new(|id| line_of(id).constructs.to_string())));
    if online {
        cols.push(("PING", Box::new(move |id| ping_of(id).flatten().map_or_else(dash, |p| p.to_string()))));
    }
    let width = 4.0 + 14.0 + 170.0 + cols.len() as f32 * (COL_W + 8.0) + 40.0;
    // Leader: first place once anyone has scored (or the match is over).
    let scored = |id: PlayerId| w.player(id).is_some_and(|p| p.kos > 0) || party.and_then(|p| p.player(id)).is_some_and(|s| s.points > 0);
    let leader = order.first().copied().filter(|id| ended || scored(*id));
    commands
        .spawn((
            Name::new("Scoreboard"),
            BoardRoot,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(if ended { 6.0 } else { 16.0 }),
                width: px(width),
                max_width: percent(98),
                margin: UiRect::left(px(-width / 2.0)),
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                padding: UiRect::all(px(12)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.01, 0.03, 0.74)),
            Pickable::IGNORE,
            GlobalZIndex(75),
        ))
        .with_children(|root| {
            if !ended {
                root.spawn((
                    Text::new(format!("{}:{:02}", secs / 60, secs % 60)),
                    TextFont::from_font_size(16.0),
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.55)),
                    Node { align_self: AlignSelf::Center, margin: UiRect::bottom(px(4)), ..default() },
                ));
            }
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
            // Header.
            root.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|h| {
                h.spawn(Node { width: px(4), ..default() });
                h.spawn(Node { width: px(14), ..default() });
                h.spawn(Node { flex_grow: 1.0, min_width: px(120), ..default() });
                for (label, _) in &cols {
                    h.spawn((Text::new(*label), TextFont::from_font_size(11.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.4)), Node { width: px(COL_W), ..default() }, TextLayout::justify(Justify::Right)));
                }
            });
            for &id in &order {
                let (name, corps) = w.player(id).map_or((format!("P{}", id.0), bd_core::corps::Corps::from_index(id.0)), |p| (p.name.clone(), p.corps));
                let c = corps_color(corps);
                let me = a.local == Some(id);
                let lead = leader == Some(id);
                let out = stock.is_some() && lives_of(id) == Some(0);
                root.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(8),
                        align_items: AlignItems::Center,
                        padding: UiRect::vertical(px(3)),
                        border: UiRect::left(px(if lead { 2.0 } else { 0.0 })),
                        ..default()
                    },
                    BorderColor::all(c.with_alpha(0.9)),
                    BackgroundColor(if me {
                        c.with_alpha(0.16)
                    } else if lead {
                        c.with_alpha(0.07)
                    } else {
                        Color::NONE
                    }),
                ))
                .with_children(|row| {
                    row.spawn((Node { width: px(4), height: px(22), ..default() }, BackgroundColor(c)));
                    if lead {
                        crown(row);
                    } else {
                        row.spawn(Node { width: px(14), ..default() });
                    }
                    let alpha = if out { 0.4 } else if me { 1.0 } else { 0.82 };
                    row.spawn((
                        Text::new(name.to_uppercase()),
                        TextFont::from_font_size(18.0),
                        TextColor(Color::WHITE.with_alpha(alpha)),
                        Node { flex_grow: 1.0, min_width: px(120), height: px(22), overflow: Overflow::clip(), ..default() },
                    ));
                    let vc = c.mix(&Color::WHITE, 0.55).with_alpha(if out { 0.45 } else { 1.0 });
                    for (k, (_, f)) in cols.iter().enumerate() {
                        // KOs (or the mode's score) stand out; the rest are quieter.
                        let main = k == 0;
                        row.spawn((
                            Text::new(f(id)),
                            TextFont::from_font_size(if main { 18.0 } else { 16.0 }),
                            TextColor(if main { vc } else { vc.with_alpha(vc.alpha() * 0.82) }),
                            Node { width: px(COL_W), ..default() },
                            TextLayout::justify(Justify::Right),
                        ));
                    }
                });
            }
        });
}

/// What the vote panel shows (None = hidden).
struct VoteShow {
    /// The cards: offered maps (+ RANDOM).
    cards: Vec<VotePick>,
    /// Picks this player may see (empty while hidden).
    votes: Vec<(PlayerId, VotePick)>,
    /// Who has voted (always shown).
    voted: Vec<PlayerId>,
    locked: bool,
    /// Decided (RANDOM's roulette lands on it when `rolled`).
    result: Option<NetMapKind>,
    rolled: bool,
    current: NetMapKind,
    deadline: Option<f64>,
    host: bool,
    practice: bool,
}

fn vote_show(state: &AppState, menu: &InGameMenu, a: Option<&ActiveWorld>, net: Option<&NetStatus>, current: &crate::arena::CurrentMap) -> Option<VoteShow> {
    let a = a?;
    if *state != AppState::InGame || menu.0 || !match_over(&a.world) {
        return None;
    }
    match a.role {
        // Practice: every map, a click is the pick.
        SimRole::Offline => Some(VoteShow {
            cards: NetMapKind::ALL.into_iter().map(VotePick::Map).collect(),
            votes: Vec::new(),
            voted: Vec::new(),
            locked: false,
            result: None,
            rolled: false,
            current: NetMapKind::from_map_kind(current.0),
            deadline: None,
            host: false,
            practice: true,
        }),
        _ => {
            let n = net?;
            let v = n.vote.as_ref()?;
            let mut cards: Vec<VotePick> = v.offers.iter().copied().map(VotePick::Map).collect();
            if v.random {
                cards.push(VotePick::Random);
            }
            Some(VoteShow {
                cards,
                votes: v.votes.clone(),
                voted: v.voted.clone(),
                locked: v.locked,
                result: v.result,
                rolled: v.rolled,
                current: n.map,
                deadline: Some(v.deadline),
                host: n.role == NetRole::Host,
                practice: false,
            })
        }
    }
}

/// My pick, as clicked (shown at once, before the host's echo; the stamp animates from `at`).
#[derive(Resource, Default)]
struct MyPick {
    pick: Option<VotePick>,
    at: f64,
}

/// The LOCKED stamp on my card (slams in).
#[derive(Component)]
struct Stamp {
    born: f64,
}

/// RANDOM's reveal: the card's name spins through the maps and lands on `result` at `end`.
#[derive(Component)]
struct Roulette {
    result: NetMapKind,
    end: f64,
}

const STAMP_SECS: f64 = 0.2;
/// The roulette shows the result for this long before the map loads.
const ROULETTE_LAND: f64 = 0.6;
/// Names the roulette flicks through.
const ROULETTE_STEPS: usize = 22;

fn pick_slot(p: VotePick) -> u64 {
    match p {
        VotePick::Map(k) => k.index() as u64,
        VotePick::Random => 63,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_vote(
    mut commands: Commands,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    active: Option<Res<ActiveWorld>>,
    net: Option<Res<NetStatus>>,
    current: Res<crate::arena::CurrentMap>,
    mut free: ResMut<crate::input::FreeCursor>,
    mut my: ResMut<MyPick>,
    roots: Query<Entity, With<VoteRoot>>,
    mut last_sig: Local<u64>,
) {
    let show = vote_show(state.get(), &menu, active.as_deref(), net.as_deref(), &current);
    if free.0 != show.is_some() {
        free.0 = show.is_some();
    }
    let (Some(v), Some(a)) = (show, active) else {
        if !roots.is_empty() {
            for e in &roots {
                commands.entity(e).despawn();
            }
            *last_sig = 0;
        }
        if my.pick.is_some() {
            *my = MyPick::default();
        }
        return;
    };
    // Mine: the host's word once it has it, else what I clicked (shown at once).
    let me = a.local;
    let server_mine = me.and_then(|me| v.votes.iter().find(|(p, _)| *p == me).map(|(_, k)| *k));
    let mine = server_mine.or(my.pick);
    let i_voted = mine.is_some() || me.is_some_and(|me| v.voted.contains(&me));
    let mut sig: u64 = 0xcbf2_9ce4_8422_2325 ^ (v.current.index() as u64) << 8 ^ (v.host as u64) << 16 ^ (v.practice as u64) << 17;
    hash(&mut sig, (v.locked as u64) << 1 | (v.rolled as u64) << 2 | v.result.map_or(0, |k| k.index() as u64 + 1) << 3);
    hash(&mut sig, mine.map_or(99, pick_slot));
    for c in &v.cards {
        hash(&mut sig, pick_slot(*c) | 1 << 12);
    }
    for (p, k) in &v.votes {
        hash(&mut sig, (p.0 as u64) << 8 | pick_slot(*k));
    }
    for p in &v.voted {
        hash(&mut sig, (p.0 as u64) << 20);
    }
    for p in &a.world.players {
        hash(&mut sig, (p.id.0 as u64) << 28 | 1 << 40);
    }
    if sig == *last_sig && !roots.is_empty() {
        return;
    }
    *last_sig = sig;
    for e in &roots {
        commands.entity(e).despawn();
    }
    let slot = |p: VotePick| match p {
        VotePick::Map(k) => k.index(),
        VotePick::Random => NetMapKind::ALL.len(),
    };
    let mut count = [0usize; NetMapKind::ALL.len() + 1];
    for (_, k) in &v.votes {
        count[slot(*k)] += 1;
    }
    let top = count.iter().copied().max().unwrap_or(0);
    let local_c = a.local_player().map(|p| corps_color(p.corps));
    // Few cards (the default: 2 + RANDOM) are big; the whole pool packs into a grid.
    let big = v.cards.len() <= 6;
    let (cw, ch) = if big { (BIG_CARD_W, BIG_CARD_H) } else { (CARD_W, CARD_H) };
    let per_row = if big { v.cards.len().max(1) } else { CARDS_PER_ROW };
    let width = per_row as f32 * (cw + 8.0) + if v.practice { 0.0 } else { 96.0 } + 24.0;
    // Once my (locked) pick is in, or the vote is decided, the other cards step back.
    let settled = v.result.is_some() || (v.locked && mine.is_some());
    let stamp_born = if server_mine.is_none() || my.pick == server_mine { my.at } else { 0.0 };
    commands
        .spawn((
            Name::new("NextMapVote"),
            VoteRoot,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                bottom: percent(4),
                width: px(width),
                max_width: percent(98),
                margin: UiRect::left(px(-width / 2.0)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(12),
                padding: UiRect::all(px(12)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.01, 0.03, 0.74)),
            GlobalZIndex(76),
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::Center,
                column_gap: px(8),
                row_gap: px(8),
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|grid| {
                for &card in &v.cards {
                    let n = count[slot(card)];
                    let picked = mine == Some(card);
                    let won = match (v.result, card) {
                        (Some(r), VotePick::Map(k)) => !v.rolled && r == k,
                        (Some(_), VotePick::Random) => v.rolled,
                        _ => false,
                    };
                    let lit = won || (v.result.is_none() && n > 0 && n == top);
                    let dim = settled && !picked && !won;
                    let is_current = card == VotePick::Map(v.current);
                    let border = if won {
                        GOLD
                    } else {
                        match (picked, local_c) {
                            (true, Some(c)) => c,
                            _ if is_current => Color::srgba(1.0, 1.0, 1.0, 0.35),
                            _ => Color::srgba(1.0, 1.0, 1.0, if dim { 0.04 } else { 0.1 }),
                        }
                    };
                    let bg = if won {
                        GOLD.with_alpha(0.22)
                    } else if lit {
                        Color::srgba(1.0, 1.0, 1.0, 0.16)
                    } else if picked {
                        local_c.unwrap_or(Color::WHITE).with_alpha(0.14)
                    } else {
                        Color::srgba(1.0, 1.0, 1.0, if dim { 0.02 } else { 0.05 })
                    };
                    let mut e = grid.spawn((
                        MapCard(card),
                        Node {
                            width: px(cw),
                            height: px(ch),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::SpaceBetween,
                            padding: UiRect::axes(px(if big { 12.0 } else { 8.0 }), px(if big { 10.0 } else { 6.0 })),
                            border: UiRect::all(px(if picked || won { 3.0 } else { 1.0 })),
                            ..default()
                        },
                        BorderColor::all(border),
                        BackgroundColor(bg),
                    ));
                    // A locked pick (or a decided vote) can't be clicked again.
                    if !settled || v.practice {
                        e.insert(Button);
                    }
                    e.with_children(|card_ui| {
                        let name_a = if dim {
                            0.35
                        } else if lit || picked {
                            0.97
                        } else {
                            0.78
                        };
                        let name = match card {
                            VotePick::Map(k) => k.name(),
                            VotePick::Random => "RANDOM",
                        };
                        let mut t = card_ui.spawn((
                            Text::new(name),
                            TextFont::from_font_size(if big { 17.0 } else { 12.0 }),
                            TextColor(if won { GOLD } else { Color::srgba(1.0, 1.0, 1.0, name_a) }),
                            Pickable::IGNORE,
                        ));
                        if let (VotePick::Random, Some(r), true) = (card, v.result, v.rolled) {
                            t.insert(Roulette { result: r, end: v.deadline.unwrap_or(0.0) });
                        }
                        if card == VotePick::Random && big && !v.rolled {
                            card_ui.spawn((
                                Text::new("?"),
                                TextFont::from_font_size(30.0),
                                TextColor(Color::srgba(1.0, 1.0, 1.0, if dim { 0.2 } else { 0.55 })),
                                Node { position_type: PositionType::Absolute, right: px(12), top: px(4), ..default() },
                                Pickable::IGNORE,
                            ));
                        }
                        let pip = if big { 12.0 } else { 10.0 };
                        card_ui.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(3), height: px(pip), ..default() }, Pickable::IGNORE)).with_children(|pips| {
                            for (p, _) in v.votes.iter().filter(|(_, x)| *x == card) {
                                let c = corps_color(a.corps_of(*p));
                                pips.spawn((Node { width: px(pip), height: px(pip), ..default() }, BackgroundColor(c), Pickable::IGNORE));
                            }
                        });
                        // My pick, locked in: a stamp slams onto the card.
                        if picked && v.locked && !v.practice {
                            let c = local_c.unwrap_or(Color::WHITE);
                            card_ui
                                .spawn((
                                    Stamp { born: stamp_born },
                                    Node {
                                        position_type: PositionType::Absolute,
                                        right: px(8),
                                        bottom: px(8),
                                        padding: UiRect::axes(px(6), px(2)),
                                        border: UiRect::all(px(2)),
                                        ..default()
                                    },
                                    BorderColor::all(c),
                                    BackgroundColor(c.with_alpha(0.18)),
                                    UiTransform { rotation: Rot2::degrees(-8.0), ..default() },
                                    Pickable::IGNORE,
                                ))
                                .with_children(|s| {
                                    s.spawn((Text::new("LOCKED"), TextFont::from_font_size(if big { 12.0 } else { 9.0 }), TextColor(c.mix(&Color::WHITE, 0.4)), Pickable::IGNORE));
                                });
                        }
                    });
                }
            });
            if !v.practice {
                root.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(8), width: px(84), ..default() }).with_children(|side| {
                    // Who has voted (not what): a filled square per voter, an outline per holdout.
                    side.spawn((
                        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, column_gap: px(3), row_gap: px(3), width: px(80), ..default() },
                        Pickable::IGNORE,
                    ))
                    .with_children(|row| {
                        for p in &a.world.players {
                            let c = corps_color(p.corps);
                            let done = v.voted.contains(&p.id) || (Some(p.id) == me && i_voted);
                            row.spawn((
                                Node { width: px(9), height: px(9), border: UiRect::all(px(1)), ..default() },
                                BorderColor::all(c.with_alpha(if done { 1.0 } else { 0.6 })),
                                BackgroundColor(if done { c } else { Color::NONE }),
                                Pickable::IGNORE,
                            ));
                        }
                    });
                    let mut go = side.spawn((
                        Node { width: px(72), height: px(72), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(2)), ..default() },
                        BorderColor::all(local_c.unwrap_or(Color::WHITE).with_alpha(if v.host { 0.9 } else { 0.35 })),
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04)),
                    ));
                    go.with_children(|b| {
                        b.spawn((CountText, Text::new(""), TextFont::from_font_size(34.0), TextColor(Color::WHITE), Pickable::IGNORE));
                    });
                    if v.host {
                        go.insert((GoNow, Button));
                        side.spawn((
                            RulesButton,
                            Button,
                            Node { width: px(72), height: px(24), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), ..default() },
                            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.05)),
                        ))
                        .with_children(|b| {
                            b.spawn((Text::new("RULES"), TextFont::from_font_size(11.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.75)), Pickable::IGNORE));
                        });
                    }
                });
            }
        });
}

/// The countdown number (every frame; no rebuild).
fn tick_count(net: Option<Res<NetStatus>>, mut q: Query<&mut Text, With<CountText>>) {
    let Some(v) = net.as_ref().and_then(|n| n.vote.as_ref()) else { return };
    let s = (v.deadline - bd_net::now_secs()).max(0.0).ceil() as u32;
    let want = s.to_string();
    for mut t in &mut q {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

/// The LOCKED stamp slams in (big → settled) and RANDOM's roulette spins down onto its map.
fn animate_vote(
    mut stamps: Query<(&Stamp, &mut UiTransform)>,
    mut spins: Query<(&Roulette, &mut Text, &mut TextColor)>,
    mut sfx: MessageWriter<crate::audio::PlaySfx>,
    mut last_step: Local<Option<usize>>,
) {
    let now = bd_net::now_secs();
    for (s, mut tf) in &mut stamps {
        let k = ((now - s.born) / STAMP_SECS).clamp(0.0, 1.0) as f32;
        let scale = 1.0 + 1.4 * (1.0 - k) * (1.0 - k);
        if (tf.scale.x - scale).abs() > 1e-3 {
            tf.scale = Vec2::splat(scale);
        }
    }
    if spins.is_empty() {
        *last_step = None;
    }
    let n = NetMapKind::ALL.len();
    for (r, mut t, mut col) in &mut spins {
        let left = r.end - now;
        let landed = left <= ROULETTE_LAND;
        let (name, color) = if landed {
            (r.result.name(), GOLD)
        } else {
            // Ease-out over the spin: a fast flicker first, slowing down onto the result.
            let u = (1.0 - (left - ROULETTE_LAND) / (map_vote::ROULETTE_SECS - ROULETTE_LAND)).clamp(0.0, 1.0);
            let steps = ((ROULETTE_STEPS as f64 * (1.0 - (1.0 - u).powi(3))) as usize).min(ROULETTE_STEPS);
            let k = (r.result.index() + n * 4 - (ROULETTE_STEPS - steps) % n) % n;
            if *last_step != Some(steps) {
                *last_step = Some(steps);
                sfx.write(crate::audio::PlaySfx::new("ui_move"));
            }
            (NetMapKind::ALL[k].name(), Color::WHITE)
        };
        if t.0 != name {
            t.0 = name.to_string();
            if landed {
                sfx.write(crate::audio::PlaySfx::new("cast_confirm"));
            }
        }
        if col.0 != color {
            col.0 = color;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn vote_clicks(
    cards: Query<(&Interaction, &MapCard), Changed<Interaction>>,
    go: Query<&Interaction, (Changed<Interaction>, With<GoNow>)>,
    rules: Query<&Interaction, (Changed<Interaction>, With<RulesButton>)>,
    mut hover: Query<(&Interaction, &mut BackgroundColor), (Or<(With<MapCard>, With<GoNow>, With<RulesButton>)>, Changed<Interaction>)>,
    mut req: MessageWriter<NetRequest>,
    mut sfx: MessageWriter<crate::audio::PlaySfx>,
    mut active: Option<ResMut<ActiveWorld>>,
    mut current: ResMut<crate::arena::CurrentMap>,
    mut after: ResMut<AfterLoading>,
    mut next: ResMut<NextState<AppState>>,
    mut pause: ResMut<InGameMenu>,
    mut menu: ResMut<crate::menu::MenuState>,
    net: Option<Res<NetStatus>>,
    mut my: ResMut<MyPick>,
) {
    for (i, mut bg) in &mut hover {
        let a = bg.0.alpha();
        match i {
            Interaction::Hovered => bg.0.set_alpha((a + 0.08).min(0.4)),
            Interaction::None => bg.0.set_alpha((a - 0.08).max(0.04)),
            Interaction::Pressed => {}
        }
    }
    for (i, card) in &cards {
        if *i != Interaction::Pressed {
            continue;
        }
        match active.as_deref_mut().filter(|a| a.role == SimRole::Offline) {
            // Practice: the click is the pick (the same map = rematch now).
            Some(a) => {
                let VotePick::Map(k) = card.0 else { continue };
                sfx.write(crate::audio::PlaySfx::new("ui_select"));
                let want = k.map_kind();
                if want == current.0 {
                    a.world.rematch();
                } else {
                    current.0 = want;
                    after.0 = AppState::InGame;
                    next.set(AppState::Loading);
                }
            }
            None => {
                let v = net.as_ref().and_then(|n| n.vote.as_ref());
                let me = net.as_ref().and_then(|n| n.me);
                let already = my.pick.is_some() || me.is_some_and(|me| v.is_some_and(|v| v.voted.contains(&me)));
                // Locked: the first pick stands (the host would ignore another anyway).
                if v.is_none_or(|v| v.result.is_some() || (v.locked && already)) || my.pick == Some(card.0) {
                    continue;
                }
                let locked = v.is_some_and(|v| v.locked);
                sfx.write(crate::audio::PlaySfx::new(if locked { "cast_confirm" } else { "ui_select" }));
                *my = MyPick { pick: Some(card.0), at: bd_net::now_secs() };
                req.write(NetRequest::VoteMap(card.0));
            }
        }
    }
    if go.iter().any(|i| *i == Interaction::Pressed) {
        sfx.write(crate::audio::PlaySfx::new("ui_select"));
        req.write(NetRequest::NextMatchNow);
    }
    if rules.iter().any(|i| *i == Interaction::Pressed) {
        sfx.write(crate::audio::PlaySfx::new("ui_select"));
        pause.0 = true;
        menu.open_rules();
    }
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
    net: Option<Res<NetStatus>>,
    active: Option<Res<ActiveWorld>>,
    mut q: Query<(&mut Text, &mut Visibility), With<CodeTag>>,
) {
    let Ok((mut t, mut vis)) = q.single_mut() else { return };
    let code = net.as_ref().filter(|n| n.role == NetRole::Host).and_then(|n| n.lobby_code.clone().or_else(|| n.lan.first().cloned()));
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
