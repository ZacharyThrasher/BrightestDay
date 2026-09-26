//! Party mode UI (COMBAT.md §5g, DESIGN.md "Party mode UI").
//!
//! * **Standings strip** (round end / pick): corps-coloured chips, best first, minimal.
//! * **Pick screen**: 2 / 3 / 4 hard-light cards — an evocative NAME only (≤ 3 words; "the mystery is
//!   the fun": no descriptions), a rarity-keyed card look, number keys to pick, a draining timer bar,
//!   everyone's lock-in dots. Boom or Bust is a card with a coin.
//! * **Boom or Bust coin**: spins, decelerates, lands gold (Legendary) or purple-black (Cursed) with
//!   its own sting.
//! * **Reveal**: who took what (names only). **Podium** at party end with each player's stack.
//!
//! Source of truth is the replicated `World::party()` state (works for host and client); the
//! `Party*` events only trigger rebuilds and animations.

use crate::audio::PlaySfx;
use crate::console::corps_color;
use crate::lobby::prefs::{Action, KeyBinds};
use crate::lobby::UiSet;
use crate::sim_bridge::{ActiveWorld, Cmd, GameEvents, SimCommand};
use crate::state::{AppState, InGameMenu};
use bd_core::corps::Corps;
use bd_core::PlayerId;
use bd_sim::party::{PartyCard, PartyPhase, PartyStanding, Rarity};
use bd_sim::GameEvent;
use bevy::picking::Pickable;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct PartyUi {
    pub phase: Option<PartyPhase>,
    phase_t: f32,
    phase_len: f32,
    round: u8,
    rounds: u8,
    /// (local picked this phase) card index.
    picked: Option<u8>,
    locked: Vec<PlayerId>,
    coin: Option<Coin>,
    reveal: Vec<bd_sim::party::PartyPick>,
    podium: Vec<PartyStanding>,
    dirty: bool,
    pick_anim: f32,
}

#[derive(Clone, Copy)]
struct Coin {
    boom: bool,
    t: f32,
    effect: Option<u16>,
    stung: bool,
    player: PlayerId,
}

const COIN_SPIN: f32 = 1.7;

#[derive(Component)]
struct PartyRoot;
#[derive(Component)]
struct TimerBar;
#[derive(Component)]
struct CardNode(u8);
#[derive(Component)]
struct CoinNode;
#[derive(Component)]
struct CoinFace;
#[derive(Component)]
struct CoinLabel;

pub struct PartyUiPlugin;

impl Plugin for PartyUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartyUi>()
            .add_systems(Update, (party_events, party_keys).chain().in_set(UiSet::Input))
            .add_systems(Update, (rebuild_party, animate_party).chain().in_set(UiSet::Draw))
            .add_systems(OnExit(AppState::InGame), |mut p: ResMut<PartyUi>| {
                *p = PartyUi { dirty: true, ..default() };
            });
    }
}

fn party_events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut ui: ResMut<PartyUi>, time: Res<Time>, mut sfx: MessageWriter<PlaySfx>) {
    let local = active.as_ref().and_then(|a| a.local);
    let dt = time.delta_secs();
    ui.phase_t += dt;
    ui.pick_anim += dt;
    if let Some(c) = ui.coin.as_mut() {
        c.t += dt;
    }
    for e in events.read() {
        match &e.event {
            GameEvent::PartyPhase { phase, round, rounds, secs } => {
                ui.phase = Some(*phase);
                ui.phase_t = 0.0;
                ui.phase_len = *secs;
                ui.round = *round;
                ui.rounds = *rounds;
                ui.dirty = true;
                if *phase == PartyPhase::Pick {
                    ui.picked = None;
                    ui.locked.clear();
                    ui.coin = None;
                    ui.reveal.clear();
                    sfx.write(PlaySfx::new("match_start").gain(0.5).pitch(1.2));
                }
                if *phase == PartyPhase::Round {
                    ui.coin = None;
                }
            }
            GameEvent::PartyLocked { player } => {
                if !ui.locked.contains(player) {
                    ui.locked.push(*player);
                    ui.dirty = true;
                }
            }
            GameEvent::PartyBoomOrBust { player, boom, effect } => {
                // Everyone sees the coin (it's a moment), but it's big for the flipper.
                ui.coin = Some(Coin { boom: *boom, t: 0.0, effect: *effect, stung: false, player: *player });
                ui.dirty = true;
                let _ = local;
            }
            GameEvent::PartyReveal { picks } => {
                ui.reveal = picks.clone();
                ui.dirty = true;
            }
            GameEvent::PartyEnd { podium } => {
                ui.podium = podium.clone();
                ui.phase = Some(PartyPhase::PartyEnd);
                ui.phase_t = 0.0;
                ui.dirty = true;
                sfx.write(PlaySfx::new("match_end").gain(0.8));
            }
            _ => {}
        }
    }
    // Coin sting on landing.
    if let Some(c) = ui.coin.as_mut()
        && c.t >= COIN_SPIN
        && !c.stung
    {
        c.stung = true;
        if c.boom {
            sfx.write(PlaySfx::new("ko_streak_end").gain(0.9).pitch(1.15));
            sfx.write(PlaySfx::new("cast_confirm").gain(0.7).pitch(0.8));
        } else {
            sfx.write(PlaySfx::new("shutdown").gain(0.9).pitch(0.75));
            sfx.write(PlaySfx::new("void").gain(0.5).pitch(0.7));
        }
        ui.dirty = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn party_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    active: Option<Res<ActiveWorld>>,
    mut ui: ResMut<PartyUi>,
    mut cmds: MessageWriter<SimCommand>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if ui.phase != Some(PartyPhase::Pick) || *state.get() != AppState::InGame || menu.0 || ui.picked.is_some() {
        return;
    }
    let Some(a) = active else { return };
    let Some(local) = a.local else { return };
    let n = a.world.party().and_then(|p| p.player(local)).map_or(0, |p| p.offer.len());
    let digit = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
    let slots = [Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4];
    for i in 0..n.min(4) {
        if keys.just_pressed(digit[i]) || binds.just_pressed(slots[i], &keys, &mouse) {
            ui.picked = Some(i as u8);
            ui.pick_anim = 0.0;
            ui.dirty = true;
            cmds.write(SimCommand::local(Cmd::PartyPick(i as u8)));
            sfx.write(PlaySfx::new("ui_select").gain(0.9));
            break;
        }
    }
}

fn rarity_colors(r: Rarity, corps: Color) -> (Color, Color) {
    match r {
        Rarity::Common => (corps, Color::srgba(0.01, 0.02, 0.05, 0.72).mix(&corps, 0.12)),
        Rarity::Legendary => (Color::srgb(1.0, 0.8, 0.25), Color::srgba(0.08, 0.05, 0.0, 0.78)),
        Rarity::Cursed => (Color::srgb(0.65, 0.3, 1.0), Color::srgba(0.08, 0.01, 0.12, 0.82)),
    }
}

fn name_of(a: &ActiveWorld, id: PlayerId) -> (String, Corps) {
    a.world.player(id).map_or((format!("P{}", id.0), Corps::from_index(id.0)), |p| (p.name.clone(), p.corps))
}

fn effect_name(a: &ActiveWorld, e: u16) -> String {
    a.world.party().and_then(|p| p.table.get(e)).map_or_else(|| "???".into(), |s| s.name.to_uppercase())
}

#[allow(clippy::too_many_arguments)]
fn rebuild_party(mut commands: Commands, mut ui: ResMut<PartyUi>, active: Option<Res<ActiveWorld>>, state: Res<State<AppState>>, roots: Query<Entity, With<PartyRoot>>) {
    if !ui.dirty {
        return;
    }
    ui.dirty = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(a) = active else { return };
    if *state.get() != AppState::InGame {
        return;
    }
    let Some(party) = a.world.party() else { return };
    let Some(phase) = ui.phase else { return };
    let local = a.local;
    let me_corps = local.map(|id| name_of(&a, id).1).unwrap_or(Corps::Willpower);
    let mc = corps_color(me_corps);

    let root = commands
        .spawn((
            Name::new("PartyUi"),
            PartyRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::top(px(28)),
                row_gap: px(18),
                ..default()
            },
            Pickable::IGNORE,
            GlobalZIndex(70),
        ))
        .id();

    // ---- standings strip (round end, pick, reveal)
    if matches!(phase, PartyPhase::RoundEnd | PartyPhase::Pick | PartyPhase::Reveal) {
        let standings = party.standings();
        commands.entity(root).with_children(|r| {
            r.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(6), ..default() }, Pickable::IGNORE)).with_children(|row| {
                row.spawn((
                    Text::new(format!("ROUND {}/{}", ui.round.max(1), ui.rounds.max(1))),
                    TextFont::from_font_size(14.0),
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
                    Node { margin: UiRect { right: px(10), top: px(5), ..default() }, ..default() },
                ));
                for s in standings.iter().take(8) {
                    let (name, corps) = name_of(&a, s.player);
                    let c = corps_color(corps);
                    let me = Some(s.player) == local;
                    row.spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: px(8),
                            padding: UiRect::axes(px(10), px(4)),
                            border: UiRect::bottom(px(if me { 3.0 } else { 2.0 })),
                            ..default()
                        },
                        BorderColor::all(c),
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                    ))
                    .with_children(|chip| {
                        chip.spawn((Text::new(format!("{}", s.place)), TextFont::from_font_size(15.0), TextColor(c)));
                        chip.spawn((Text::new(name.to_uppercase()), TextFont::from_font_size(15.0), TextColor(Color::WHITE.with_alpha(if me { 1.0 } else { 0.75 }))));
                        chip.spawn((Text::new(format!("{}", s.points)), TextFont::from_font_size(15.0), TextColor(c.mix(&Color::WHITE, 0.4))));
                    });
                }
            });
        });
    }

    // ---- pick cards
    if phase == PartyPhase::Pick {
        let me = local.and_then(|id| party.player(id));
        let offer = me.map(|p| p.offer).unwrap_or_default();
        let picked = ui.picked.or(me.and_then(|p| p.picked));
        commands.entity(root).with_children(|r| {
            r.spawn((Node { height: percent(14), ..default() }, Pickable::IGNORE));
            r.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(22), ..default() }, Pickable::IGNORE)).with_children(|row| {
                for i in 0..offer.len() as u8 {
                    let Some(card) = offer.card(i) else { continue };
                    let (title, rarity) = match card {
                        PartyCard::Effect(e) => (effect_name(&a, e), party.table.get(e).map_or(Rarity::Common, |s| s.rarity)),
                        PartyCard::BoomOrBust => ("BOOM OR BUST".to_string(), Rarity::Legendary),
                    };
                    let (edge, fill) = rarity_colors(rarity, mc);
                    let chosen = picked == Some(i);
                    let dim = picked.is_some() && !chosen;
                    row.spawn((
                        CardNode(i),
                        Node {
                            width: px(210),
                            height: px(290),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::SpaceBetween,
                            padding: UiRect::all(px(16)),
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::all(px(6)),
                            ..default()
                        },
                        BorderColor::all(edge.with_alpha(if dim { 0.25 } else { 1.0 })),
                        BackgroundColor(fill.with_alpha(if dim { 0.3 } else { fill.alpha() })),
                        BoxShadow::new(edge.with_alpha(if dim { 0.0 } else if chosen { 0.9 } else { 0.45 }), px(0), px(0), px(if chosen { 10.0 } else { 2.0 }), px(if chosen { 28.0 } else { 16.0 })),
                        UiTransform::default(),
                    ))
                    .with_children(|c| {
                        c.spawn((Text::new(format!("{}", i + 1)), TextFont::from_font_size(20.0), TextColor(edge.with_alpha(if dim { 0.3 } else { 0.9 }))));
                        if card == PartyCard::BoomOrBust {
                            // Half gold / half cursed coin emblem.
                            c.spawn((
                                Node { width: px(86), height: px(86), border: UiRect::all(px(3)), border_radius: BorderRadius::MAX, ..default() },
                                BorderColor::all(Color::srgb(1.0, 0.85, 0.3)),
                                BackgroundGradient::from(bevy::ui::LinearGradient::new(
                                    bevy::ui::LinearGradient::TO_RIGHT,
                                    vec![
                                        bevy::ui::ColorStop::new(Color::srgb(1.0, 0.78, 0.2), percent(50)),
                                        bevy::ui::ColorStop::new(Color::srgb(0.25, 0.05, 0.35), percent(50)),
                                    ],
                                )),
                            ));
                        } else {
                            // A shard of hard light (rarity keyed), no description.
                            c.spawn((
                                Node { width: px(56), height: px(56), ..default() },
                                BackgroundColor(edge.with_alpha(if dim { 0.1 } else { 0.55 })),
                                UiTransform { rotation: Rot2::degrees(45.0), ..default() },
                                BoxShadow::new(edge.with_alpha(0.6), px(0), px(0), px(4), px(18)),
                            ));
                        }
                        c.spawn((
                            Text::new(title),
                            TextFont::from_font_size(26.0),
                            TextColor(Color::WHITE.with_alpha(if dim { 0.3 } else { 1.0 })),
                            TextLayout::justify(Justify::Center),
                        ));
                    });
                }
            });
            // Timer bar + lock-in dots.
            r.spawn((Node { width: px(520), height: px(3), margin: UiRect::top(px(10)), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12)))).with_children(|b| {
                b.spawn((TimerBar, Node { width: percent(100), height: percent(100), ..default() }, BackgroundColor(mc)));
            });
            r.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(8), ..default() }, Pickable::IGNORE)).with_children(|dots| {
                for p in &party.players {
                    let (_, corps) = name_of(&a, p.id);
                    let c = corps_color(corps);
                    let locked = ui.locked.contains(&p.id) || p.picked.is_some();
                    dots.spawn((
                        Node { width: px(10), height: px(10), border: UiRect::all(px(2)), border_radius: BorderRadius::MAX, ..default() },
                        BorderColor::all(c),
                        BackgroundColor(if locked { c } else { Color::NONE }),
                    ));
                }
            });
        });
    }

    // ---- reveal
    if phase == PartyPhase::Reveal && !ui.reveal.is_empty() {
        commands.entity(root).with_children(|r| {
            r.spawn((Node { height: percent(18), ..default() }, Pickable::IGNORE));
            r.spawn((
                Node { flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::axes(px(22), px(14)), border_radius: BorderRadius::all(px(6)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.01, 0.03, 0.7)),
                Pickable::IGNORE,
            ))
            .with_children(|col| {
                for pk in &ui.reveal {
                    let (name, corps) = name_of(&a, pk.player);
                    let c = corps_color(corps);
                    let eff = pk.effect.map(|e| effect_name(&a, e)).unwrap_or_else(|| "—".into());
                    let rarity = pk.effect.and_then(|e| party.table.get(e)).map_or(Rarity::Common, |s| s.rarity);
                    let (edge, _) = rarity_colors(rarity, c);
                    col.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(14), width: px(480), align_items: AlignItems::Center, ..default() },)).with_children(|row| {
                        row.spawn((Node { width: px(4), height: px(22), ..default() }, BackgroundColor(c)));
                        row.spawn((Text::new(name.to_uppercase()), TextFont::from_font_size(20.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)), Node { width: px(200), ..default() }));
                        row.spawn((Text::new(eff), TextFont::from_font_size(22.0), TextColor(edge.mix(&Color::WHITE, 0.3))));
                    });
                }
            });
        });
    }

    // ---- Boom or Bust coin
    if let Some(coin) = ui.coin {
        let mine = Some(coin.player) == local;
        let size = if mine { 180.0 } else { 90.0 };
        commands.entity(root).with_children(|r| {
            r.spawn((
                Node { position_type: PositionType::Absolute, left: percent(50), top: percent(if mine { 42.0 } else { 70.0 }), margin: UiRect { left: px(-size / 2.0), top: px(-size / 2.0), ..default() }, flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(12), ..default() },
                Pickable::IGNORE,
            ))
            .with_children(|c| {
                c.spawn((
                    CoinNode,
                    CoinFace,
                    Node { width: px(size), height: px(size), border: UiRect::all(px(4)), border_radius: BorderRadius::MAX, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                    BorderColor::all(Color::srgb(1.0, 0.85, 0.3)),
                    BackgroundColor(Color::srgb(1.0, 0.78, 0.2)),
                    BoxShadow::new(Color::srgba(1.0, 0.8, 0.3, 0.6), px(0), px(0), px(6), px(30)),
                    UiTransform::default(),
                ))
                .with_children(|f| {
                    f.spawn((CoinLabel, Text::new(""), TextFont::from_font_size(size * 0.2), TextColor(Color::WHITE)));
                });
                if coin.t >= COIN_SPIN {
                    let label = coin.effect.map(|e| effect_name(&a, e)).unwrap_or_default();
                    c.spawn((Text::new(label), TextFont::from_font_size(if mine { 28.0 } else { 16.0 }), TextColor(if coin.boom { Color::srgb(1.0, 0.85, 0.35) } else { Color::srgb(0.75, 0.45, 1.0) })));
                }
            });
        });
    }

    // ---- podium
    if phase == PartyPhase::PartyEnd && !ui.podium.is_empty() {
        commands.entity(root).with_children(|r| {
            r.spawn((Node { height: percent(12), ..default() }, Pickable::IGNORE));
            r.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, column_gap: px(18), ..default() }, Pickable::IGNORE)).with_children(|row| {
                // 2nd, 1st, 3rd.
                let order = [1usize, 0, 2];
                for &k in &order {
                    let Some(s) = ui.podium.get(k) else { continue };
                    let (name, corps) = name_of(&a, s.player);
                    let c = corps_color(corps);
                    let h = match s.place {
                        1 => 220.0,
                        2 => 160.0,
                        _ => 120.0,
                    };
                    row.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(8), width: px(190), ..default() },)).with_children(|col| {
                        col.spawn((Text::new(name.to_uppercase()), TextFont::from_font_size(22.0), TextColor(Color::WHITE)));
                        for e in party.effects_of(s.player) {
                            col.spawn((Text::new(e.name.to_uppercase()), TextFont::from_font_size(12.0), TextColor(rarity_colors(e.rarity, c).0.with_alpha(0.8))));
                        }
                        col.spawn((
                            Node { width: px(150), height: px(h), justify_content: JustifyContent::Center, padding: UiRect::top(px(10)), border: UiRect::all(px(2)), ..default() },
                            BorderColor::all(c),
                            BackgroundColor(c.with_alpha(0.18)),
                            BoxShadow::new(c.with_alpha(0.5), px(0), px(0), px(2), px(22)),
                        ))
                        .with_children(|p| {
                            p.spawn((Text::new(format!("{}", s.place)), TextFont::from_font_size(48.0), TextColor(c.mix(&Color::WHITE, 0.4))));
                        });
                        col.spawn((Text::new(format!("{} PTS", s.points)), TextFont::from_font_size(15.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6))));
                    });
                }
            });
            // The rest (4th+), one line each.
            for s in ui.podium.iter().skip(3) {
                let (name, corps) = name_of(&a, s.player);
                r.spawn((Text::new(format!("{}  {}  {}", s.place, name.to_uppercase(), s.points)), TextFont::from_font_size(15.0), TextColor(corps_color(corps).mix(&Color::WHITE, 0.5))));
            }
        });
    }
}

#[allow(clippy::type_complexity)]
fn animate_party(
    ui: Res<PartyUi>,
    mut bars: Query<&mut Node, With<TimerBar>>,
    mut cards: Query<(&CardNode, &mut UiTransform), Without<CoinNode>>,
    mut coin: Query<(&mut UiTransform, &mut BackgroundColor, &mut BorderColor), (With<CoinNode>, Without<CardNode>)>,
    mut label: Query<&mut Text, With<CoinLabel>>,
) {
    if ui.phase == Some(PartyPhase::Pick) {
        let left = if ui.phase_len > 0.0 { (1.0 - ui.phase_t / ui.phase_len).clamp(0.0, 1.0) } else { 1.0 };
        for mut n in &mut bars {
            n.width = percent(left * 100.0);
        }
        for (c, mut tf) in &mut cards {
            // Cards float; the chosen one lifts.
            let bob = ((ui.phase_t * 1.6 + c.0 as f32 * 0.9).sin()) * 4.0;
            let chosen = ui.picked == Some(c.0);
            let lift = if chosen { -18.0 * (ui.pick_anim / 0.12).min(1.0) } else { 0.0 };
            let s = if chosen { 1.0 + 0.06 * (ui.pick_anim / 0.12).min(1.0) } else { 1.0 };
            tf.translation = Val2::px(0.0, bob + lift);
            tf.scale = Vec2::splat(s);
        }
    }
    if let Some(c) = ui.coin {
        let Ok((mut tf, mut bg, mut border)) = coin.single_mut() else { return };
        // Spin with deceleration (angle ~ t·(1 - t/2T) · ω), landing flat.
        let t = c.t.min(COIN_SPIN);
        let u = t / COIN_SPIN;
        let turns = 7.5;
        let ang = std::f32::consts::TAU * turns * (u * (2.0 - u)) + if c.boom { 0.0 } else { std::f32::consts::PI };
        let sx = ang.cos();
        tf.scale = Vec2::new(sx.abs().max(0.04), 1.0) * if c.t >= COIN_SPIN { 1.0 + 0.12 * (-(c.t - COIN_SPIN) * 6.0).exp() } else { 1.0 };
        tf.translation = Val2::px(0.0, -40.0 * (u * std::f32::consts::PI).sin());
        let gold_side = sx >= 0.0;
        let (face, edge) = if gold_side { (Color::srgb(1.0, 0.78, 0.2), Color::srgb(1.0, 0.9, 0.45)) } else { (Color::srgb(0.18, 0.03, 0.26), Color::srgb(0.6, 0.25, 0.95)) };
        bg.0 = face;
        *border = BorderColor::all(edge);
        if let Ok(mut l) = label.single_mut() {
            let want = if c.t >= COIN_SPIN { if c.boom { "BOOM" } else { "BUST" } } else { "" };
            if l.0 != want {
                l.0.clear();
                l.0.push_str(want);
            }
        }
    }
}
