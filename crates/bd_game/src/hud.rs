//! Minimal HUD: a crosshair dot and the F3 performance overlay.
//!
//! [`FrameTimes`] records every frame's real duration; the overlay shows fps, frame time, **p99**,
//! 1 % low and a hitch counter (> 16 ms frames), plus debris / entity / tick / remesh counts.

use crate::debris::DebrisPool;
use crate::player::AnchorPreview;
use crate::render::voxel_render::RemeshQueue;
use crate::state::{AppState, InGameMenu};
use crate::tick::TickClock;
use crate::util::FrameStats;
use bevy::picking::Pickable;
use bevy::prelude::*;
use std::collections::VecDeque;

/// Rolling frame-time history (ms) + lifetime hitch count.
#[derive(Resource, Debug, Default)]
pub struct FrameTimes {
    pub recent: VecDeque<f32>,
    pub hitches_total: u64,
    pub frames_total: u64,
}

impl FrameTimes {
    pub const WINDOW: usize = 1000;
}

#[derive(Resource, Default)]
pub struct PerfOverlay {
    pub visible: bool,
}

#[derive(Component)]
struct PerfText;
#[derive(Component)]
struct Crosshair;
#[derive(Component)]
struct AnchorRing;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameTimes>()
            .init_resource::<PerfOverlay>()
            .add_systems(Startup, spawn_hud)
            .add_systems(First, record_frame_time)
            .add_systems(Update, (toggle_overlay, update_overlay, update_crosshair));
    }
}

fn record_frame_time(time: Res<Time<Real>>, mut ft: ResMut<FrameTimes>) {
    let ms = time.delta_secs() * 1000.0;
    if ms <= 0.0 {
        return;
    }
    ft.frames_total += 1;
    if ms > FrameStats::HITCH_MS {
        ft.hitches_total += 1;
    }
    if ft.recent.len() >= FrameTimes::WINDOW {
        ft.recent.pop_front();
    }
    ft.recent.push_back(ms);
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Name::new("PerfOverlay"),
        PerfText,
        Text::new(""),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb(0.85, 1.0, 0.88)),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, left: px(10), top: px(8), ..default() },
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(60),
    ));
    // Anchor ring: opens around the dot when a tether would stick.
    commands.spawn((
        Name::new("AnchorRing"),
        AnchorRing,
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(14),
            height: px(14),
            margin: UiRect { left: px(-7), top: px(-7), ..default() },
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(7)),
            ..default()
        },
        BorderColor::all(ACCENT),
        BackgroundColor(Color::NONE),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(40),
    ));
    // Centre dot: always visible in game (motion comfort + aim reference).
    commands
        .spawn((
            Name::new("Crosshair"),
            Crosshair,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(50),
                width: px(6),
                height: px(6),
                margin: UiRect { left: px(-3), top: px(-3), ..default() },
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(3)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.95)),
            Visibility::Hidden,
            Pickable::IGNORE,
            GlobalZIndex(40),
        ));
}

fn toggle_overlay(keys: Res<ButtonInput<KeyCode>>, mut o: ResMut<PerfOverlay>) {
    if keys.just_pressed(KeyCode::F3) {
        o.visible = !o.visible;
    }
}

#[allow(clippy::too_many_arguments)]
fn update_overlay(
    time: Res<Time<Real>>,
    overlay: Res<PerfOverlay>,
    ft: Res<FrameTimes>,
    pool: Res<DebrisPool>,
    clock: Res<TickClock>,
    queue: Res<RemeshQueue>,
    entities: Query<()>,
    mut text: Query<(&mut Text, &mut Visibility), With<PerfText>>,
    mut since: Local<f32>,
    mut scratch: Local<Vec<f32>>,
) {
    let Ok((mut text, mut vis)) = text.single_mut() else { return };
    let want = if overlay.visible { Visibility::Visible } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if !overlay.visible {
        return;
    }
    *since += time.delta_secs();
    if *since < 0.25 && !overlay.is_changed() {
        return;
    }
    *since = 0.0;
    // Last ~2 s of frames.
    let take = ft.recent.len().min(600);
    let s = FrameStats::compute(ft.recent.iter().rev().take(take).copied(), &mut scratch);
    text.0 = format!(
        "{:>4.0} fps  {:>5.2} ms  p99 {:>5.2} ms  1% low {:>4.0}  max {:>5.1} ms  hitches {}\n\
         debris {:>5}  entities {}  ticks/frame {}  remesh queue {}",
        s.avg_fps,
        s.avg_ms,
        s.p99_ms,
        s.low1_fps,
        s.max_ms,
        ft.hitches_total,
        pool.live(),
        entities.iter().count(),
        clock.ticks_this_frame,
        queue.pending_len(),
    );
}

/// Centre dot always visible in game; the corps-coloured ring appears around it when a tether would
/// anchor (dimmer when found by aim assist rather than the exact ray).
fn update_crosshair(
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    preview: Res<AnchorPreview>,
    mut dot: Query<&mut Visibility, (With<Crosshair>, Without<AnchorRing>)>,
    mut ring: Query<(&mut Visibility, &mut BorderColor), (With<AnchorRing>, Without<Crosshair>)>,
) {
    let show = *state.get() == AppState::InGame && !menu.0;
    let vis = |on: bool| if on { Visibility::Visible } else { Visibility::Hidden };
    for mut v in &mut dot {
        if *v != vis(show) {
            *v = vis(show);
        }
    }
    for (mut v, mut border) in &mut ring {
        let on = show && preview.valid;
        if *v != vis(on) {
            *v = vis(on);
        }
        let want = BorderColor::all(ACCENT.with_alpha(if preview.assisted { 0.5 } else { 1.0 }));
        if *border != want {
            *border = want;
        }
    }
}

const ACCENT: Color = Color::srgb(0.3, 1.0, 0.45);
