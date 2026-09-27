//! CLICK TO CONFIRM — the crosshair affordance while a ghost waits: a tiny mouse glyph right of the
//! crosshair whose left button breathes in the corps colour (click = make it real), and for a chain
//! one pip per step under the spark gauge. No words (mystery tenet); the cost is the gauge's own
//! bite (`power_hud` cost preview). Fades with the ghost's timeout. Nothing is allocated per frame.

use super::ConfirmGate;
use crate::console::corps_color;
use crate::lobby::prefs::UiPrefs;
use crate::sim_bridge::ActiveWorld;
use crate::state::{AppState, InGameMenu};
use bd_sim::powers::toolbox::MAX_CHAIN;
use bevy::picking::Pickable;
use bevy::prelude::*;

pub(super) fn build(app: &mut App) {
    app.add_systems(Startup, spawn).add_systems(Update, draw.in_set(crate::lobby::UiSet::Draw));
}

#[derive(Component)]
struct Root;
#[derive(Component)]
struct Glyph;
#[derive(Component)]
struct GlyphButton;
#[derive(Component)]
struct Pip(u8);

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Name::new("ConfirmAffordance"),
            Root,
            Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), width: px(0), height: px(0), ..default() },
            Visibility::Hidden,
            Pickable::IGNORE,
            GlobalZIndex(41),
        ))
        .with_children(|r| {
            // The mouse: a rounded body, a seam, the left button lit.
            r.spawn((
                Glyph,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(30),
                    top: px(-11),
                    width: px(14),
                    height: px(21),
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(7)),
                    ..default()
                },
                BorderColor::all(Color::WHITE),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
            ))
            .with_children(|m| {
                m.spawn((
                    GlyphButton,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        width: px(5.5),
                        height: px(8),
                        border_radius: BorderRadius { top_left: px(6), ..BorderRadius::ZERO },
                        ..default()
                    },
                    BackgroundColor(Color::WHITE),
                ));
                // the seam under the buttons
                m.spawn((Node { position_type: PositionType::Absolute, left: px(0), top: px(8), width: px(11), height: px(1), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.5))));
            });
            // Chain pips (centred under the gauge).
            for k in 0..MAX_CHAIN {
                r.spawn((
                    Pip(k as u8),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(62),
                        width: px(7),
                        height: px(7),
                        border: UiRect::all(px(1.5)),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BorderColor::all(Color::WHITE),
                    BackgroundColor(Color::NONE),
                    Visibility::Hidden,
                ));
            }
        });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    time: Res<Time<Real>>,
    gate: Res<ConfirmGate>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    prefs: Res<UiPrefs>,
    active: Option<Res<ActiveWorld>>,
    mut root: Query<&mut Visibility, (With<Root>, Without<Pip>)>,
    mut glyph: Query<&mut BorderColor, (With<Glyph>, Without<Pip>)>,
    mut button: Query<&mut BackgroundColor, (With<GlyphButton>, Without<Pip>)>,
    mut pips: Query<(&Pip, &mut Node, &mut Visibility, &mut BorderColor, &mut BackgroundColor), (Without<Root>, Without<Glyph>, Without<GlyphButton>)>,
    mut was: Local<bool>,
) {
    let p = gate.pending.as_ref().filter(|_| *state.get() == AppState::InGame && !menu.0);
    let show = p.is_some();
    if !show && !*was {
        return;
    }
    *was = show;
    let vis = if show { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        if *v != vis {
            *v = vis;
        }
    }
    let Some(p) = p else { return };
    let corps = active.as_ref().and_then(|a| a.local_player().map(|p| p.corps)).unwrap_or(prefs.corps);
    let c = corps_color(corps);
    let t = time.elapsed_secs();
    let fade = p.life() * (p.age / 0.12).min(1.0);
    let breathe = 0.55 + 0.45 * (t * 4.0).sin().abs();
    for mut b in &mut glyph {
        *b = BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.8 * fade));
    }
    for mut bg in &mut button {
        bg.0 = c.with_alpha(breathe * fade);
    }
    let n = p.steps();
    let n = if n > 1 { n.min(MAX_CHAIN) } else { 0 };
    let gap = 11.0;
    for (pip, mut node, mut v, mut b, mut bg) in &mut pips {
        let k = pip.0 as usize;
        let want = if k < n { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
        if k >= n {
            continue;
        }
        let x = (k as f32 - (n as f32 - 1.0) * 0.5) * gap - 3.5;
        if node.left != px(x) {
            node.left = px(x);
        }
        // a ripple runs along the pips: the order they will fire in
        let wave = 0.5 + 0.5 * ((t * 3.0 - k as f32 * 0.6).sin());
        *b = BorderColor::all(c.with_alpha(0.95 * fade));
        bg.0 = c.with_alpha(0.55 * wave * fade);
    }
}
