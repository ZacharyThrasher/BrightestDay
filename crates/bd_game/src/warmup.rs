//! Loading phase: pre-warm every render pipeline so nothing compiles mid-match.
//!
//! While in [`AppState::Loading`] an opaque loading screen covers the window and every entity
//! tagged [`WarmupItem`] is parked in front of the camera, so the main pass, the shadow pass and
//! the post-process chain specialise and compile their pipelines. Loading ends when:
//! the arena is meshed, at least [`MIN_FRAMES`] frames were drawn, and Bevy's `PipelineCache` has
//! had no waiting pipelines for [`STABLE_FRAMES`] frames (or after [`TIMEOUT_SECS`]).
//!
//! Other modules register warm-up content with [`WarmupAppExt::add_warmup`]:
//! ```ignore
//! app.add_warmup(|world: &mut World| {
//!     let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::default());
//!     let mat = world.resource_mut::<Assets<ConstructMaterial>>().add(ConstructMaterial::default());
//!     world.spawn((WarmupItem, Mesh3d(mesh), MeshMaterial3d(mat)));
//! });
//! ```
//! Warm-up entities are despawned automatically when loading ends.

use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::render::voxel_render::RemeshQueue;
use crate::state::{AfterLoading, AppState};
use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::{Render, RenderApp};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub const MIN_FRAMES: u32 = 20;
pub const STABLE_FRAMES: u32 = 10;
pub const TIMEOUT_SECS: f32 = 40.0;

pub type WarmupFn = fn(&mut World);

#[derive(Resource, Default)]
pub struct WarmupRegistry(Vec<WarmupFn>);

pub trait WarmupAppExt {
    /// Register a function that spawns warm-up content (tag it with [`WarmupItem`]).
    fn add_warmup(&mut self, f: WarmupFn) -> &mut Self;
}

impl WarmupAppExt for App {
    fn add_warmup(&mut self, f: WarmupFn) -> &mut Self {
        self.init_resource::<WarmupRegistry>();
        self.world_mut().resource_mut::<WarmupRegistry>().0.push(f);
        self
    }
}

/// Parked in front of the camera during loading, despawned afterwards.
#[derive(Component, Default)]
pub struct WarmupItem;

/// Other systems can hold loading open (e.g. world generation). Loading finishes only when every
/// registered blocker is false.
#[derive(Resource, Default, Debug)]
pub struct LoadingBlockers {
    pub arena_pending: bool,
    pub audio_pending: bool,
}

/// Pipeline-cache status mirrored from the render world.
#[derive(Resource, Clone, Default)]
pub struct PipelineStatus {
    waiting: Arc<AtomicU32>,
    total: Arc<AtomicU32>,
}

impl PipelineStatus {
    pub fn waiting(&self) -> u32 {
        self.waiting.load(Ordering::Relaxed)
    }
    pub fn total(&self) -> u32 {
        self.total.load(Ordering::Relaxed)
    }
}

#[derive(Resource, Default, Debug)]
pub struct LoadingProgress {
    pub frames: u32,
    pub stable: u32,
    pub last_total: u32,
    pub secs: f32,
    /// 0..1 for the loading bar.
    pub fraction: f32,
    pub done: bool,
}

#[derive(Component)]
struct LoadingScreen;
#[derive(Component)]
struct LoadingBar;

pub struct WarmupPlugin;

impl Plugin for WarmupPlugin {
    fn build(&self, app: &mut App) {
        let status = PipelineStatus::default();
        app.init_resource::<WarmupRegistry>()
            .init_resource::<LoadingBlockers>()
            .init_resource::<LoadingProgress>()
            .insert_resource(status.clone())
            .add_warmup(builtin_warmup)
            .add_systems(OnEnter(AppState::Loading), (spawn_loading_screen, run_warmup_fns))
            .add_systems(OnExit(AppState::Loading), cleanup)
            .add_systems(Update, track_loading.run_if(in_state(AppState::Loading)))
            .add_systems(
                PostUpdate,
                park_warmup_items.after(CameraUpdateSet).before(bevy::transform::TransformSystems::Propagate),
            );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(status).add_systems(Render, mirror_pipeline_status);
        }
    }
}

fn mirror_pipeline_status(cache: Res<PipelineCache>, status: Res<PipelineStatus>) {
    status.waiting.store(cache.waiting_pipelines().count() as u32, Ordering::Relaxed);
    status.total.store(cache.pipelines().count() as u32, Ordering::Relaxed);
}

fn run_warmup_fns(world: &mut World) {
    let fns: Vec<WarmupFn> = world.resource::<WarmupRegistry>().0.clone();
    for f in fns {
        f(world);
    }
}

/// Built-in warm-up: the generic StandardMaterial variants gameplay code is likely to use first
/// (emissive hard light, unlit, alpha-blended) so their first appearance never hitches.
fn builtin_warmup(world: &mut World) {
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(0.3, 0.3, 0.3));
    let mats = {
        let mut m = world.resource_mut::<Assets<StandardMaterial>>();
        [
            m.add(StandardMaterial { base_color: Color::srgb(0.2, 1.0, 0.35), emissive: LinearRgba::rgb(0.6, 3.0, 1.0), ..default() }),
            m.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() }),
            m.add(StandardMaterial {
                base_color: Color::srgba(0.2, 1.0, 0.35, 0.4),
                alpha_mode: AlphaMode::Blend,
                emissive: LinearRgba::rgb(0.3, 1.5, 0.5),
                ..default()
            }),
            m.add(StandardMaterial { base_color: Color::srgb(0.8, 0.8, 0.8), alpha_mode: AlphaMode::Add, ..default() }),
        ]
    };
    for mat in mats {
        world.spawn((WarmupItem, Mesh3d(mesh.clone()), MeshMaterial3d(mat)));
    }
}

fn park_warmup_items(
    cam: Query<&Transform, (With<PlayerCamera>, Without<WarmupItem>)>,
    mut items: Query<&mut Transform, With<WarmupItem>>,
) {
    let Ok(cam) = cam.single() else { return };
    let n = items.iter().count().max(1);
    let cols = (n as f32).sqrt().ceil() as usize;
    for (i, mut t) in items.iter_mut().enumerate() {
        let (cx, cy) = ((i % cols) as f32, (i / cols) as f32);
        let local = Vec3::new((cx - cols as f32 * 0.5) * 0.45, (cy - cols as f32 * 0.5) * 0.45, -3.0);
        t.translation = cam.translation + cam.rotation * local;
    }
}

fn track_loading(
    time: Res<Time<Real>>,
    status: Res<PipelineStatus>,
    blockers: Res<LoadingBlockers>,
    queue: Res<RemeshQueue>,
    after: Res<AfterLoading>,
    mut progress: ResMut<LoadingProgress>,
    mut next: ResMut<NextState<AppState>>,
    mut bar: Query<&mut Node, With<LoadingBar>>,
) {
    let p = &mut *progress;
    p.frames += 1;
    p.secs += time.delta_secs();
    let total = status.total();
    if status.waiting() == 0 && total == p.last_total && p.frames > 2 {
        p.stable += 1;
    } else {
        p.stable = 0;
    }
    p.last_total = total;
    let world_ready = !blockers.arena_pending && !blockers.audio_pending && queue.is_idle();
    let pipelines_ready = p.stable >= STABLE_FRAMES && p.frames >= MIN_FRAMES;
    // Rough progress: world first, then pipelines settling.
    let world_part = if blockers.arena_pending { 0.1 } else if queue.is_idle() { 0.7 } else { 0.4 };
    let target = if world_ready { 0.7 + 0.3 * (p.stable as f32 / STABLE_FRAMES as f32).min(1.0) } else { world_part };
    p.fraction += (target - p.fraction) * 0.2;
    if let Ok(mut n) = bar.single_mut() {
        n.width = percent(p.fraction * 100.0);
    }
    if !p.done && ((world_ready && pipelines_ready) || p.secs > TIMEOUT_SECS) {
        p.done = true;
        if p.secs > TIMEOUT_SECS {
            warn!("loading timed out; continuing (waiting pipelines: {})", status.waiting());
        }
        info!("loading done in {:.2}s ({} frames, {} pipelines)", p.secs, p.frames, total);
        next.set(after.0.clone());
    }
}

fn spawn_loading_screen(mut commands: Commands, mut progress: ResMut<LoadingProgress>) {
    *progress = LoadingProgress::default();
    // UI pipelines the menus use that the loading screen itself doesn't (box shadows — the title's
    // hard-light voxels —, gradients, rounded corners): drawn once *under* the opaque loading screen
    // so they are compiled before the title appears (otherwise the logo can be missing for its
    // first frames while those pipelines compile).
    commands.spawn((
        Name::new("UiWarmup"),
        LoadingScreen,
        Node {
            position_type: PositionType::Absolute,
            left: px(40),
            top: px(40),
            width: px(24),
            height: px(24),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(Color::WHITE),
        BackgroundGradient::from(bevy::ui::LinearGradient::new(
            bevy::ui::LinearGradient::TO_RIGHT,
            vec![bevy::ui::ColorStop::new(Color::WHITE, percent(0)), bevy::ui::ColorStop::new(Color::BLACK.with_alpha(0.0), percent(100))],
        )),
        BoxShadow::new(Color::WHITE.with_alpha(0.6), px(0), px(0), px(1), px(12)),
        GlobalZIndex(99),
    ));
    commands
        .spawn((
            Name::new("LoadingScreen"),
            LoadingScreen,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(22),
                ..default()
            },
            BackgroundColor(Color::srgb(0.02, 0.03, 0.05)),
            GlobalZIndex(100),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("BRIGHTEST DAY"),
                TextFont::from_font_size(44.0),
                TextColor(Color::srgb(0.75, 1.0, 0.8)),
            ));
            p.spawn((
                Node { width: px(360), height: px(3), ..default() },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
            ))
            .with_children(|p| {
                p.spawn((
                    LoadingBar,
                    Node { width: percent(0), height: percent(100), ..default() },
                    BackgroundColor(Color::srgb(0.25, 1.0, 0.4)),
                ));
            });
        });
}

fn cleanup(mut commands: Commands, screens: Query<Entity, With<LoadingScreen>>, items: Query<Entity, With<WarmupItem>>) {
    for e in screens.iter().chain(items.iter()) {
        commands.entity(e).despawn();
    }
}
