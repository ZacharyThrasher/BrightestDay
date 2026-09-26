//! Cut-ins (a tiny live 3D window over the HUD) + the summon ping that uses them.
//!
//! # Cut-in API (other presentation code builds against this)
//! Write a [`CutIn`] message from any `Update` system:
//! ```ignore
//! fn my_ultimate(mut cut: MessageWriter<CutIn>, /* … */) {
//!     cut.write(CutIn::alert(CutInTarget::Entity { entity: caster, radius: 2.0 }, corps_rgb, 0.8, Some("DOMAIN EXPANSION".into()))
//!         .with_framing(CutInFraming::Dramatic { low_angle: true, orbit_speed: 0.6 })
//!         .with_duration(1.6)
//!         .with_priority(5));
//! }
//! ```
//! * [`CutInTarget`]: `Entity { entity, radius }` — when the entity is a construct (`ConstructVisual`)
//!   the framing comes from its model bounds, otherwise from its `GlobalTransform` + `radius`; the
//!   entity and its mesh descendants (≤ 256) are drawn in the window **by adding the preview render
//!   layer to the real entities** while it shows (no copies). `Point { pos, radius }` frames a spot
//!   (only useful with `show_world`).
//! * [`CutInFraming`]: `Auto` (from the viewer's side, slightly above, slow drift) or
//!   `Dramatic { low_angle, orbit_speed }` (closer, optionally from below, orbiting at rad/s).
//! * [`CutInStyle`]: `Info` (thin corps-colour frame) or `Alert { severity 0..1 }` (bigger window,
//!   pulsing frame, hazard trim in `color`, `label` stamped under the picture).
//! * Placement: `Auto` (Info: right edge; Alert: top centre) or `Screen(v)` (frame centre in 0..1
//!   screen units). Summon pings place theirs next to the edge arrow.
//! * `show_world = true` also renders the normal world (layer 0) in the window — much more
//!   expensive (full visibility pass + sun shadow cascades for that view); default false = only the
//!   target on a transparent background over a dark panel.
//! * **One cut-in at a time**: a new one replaces the current one when it ranks at least as high
//!   (`Alert` > `Info`, then `priority`); lower ones are dropped. Pop-in 0.14 s, fade/shrink 0.18 s
//!   at the end of `duration`. Hidden while the pause menu is open.
//!
//! The window is a second `Camera3d` (order 10, own `Viewport`, `RenderLayers` = [`PREVIEW_LAYER`],
//! no Hdr / bloom / fog / MSAA / shadows / light clusters, tonemapped in the material shader) that
//! is spawned once and **inactive** (`is_active = false`) whenever no cut-in shows. It clears only
//! its own LDR target to transparent and is composited over the window with premultiplied-alpha
//! blending (the gameplay camera is HDR, so their intermediate textures are not shared). UI stays
//! on the gameplay camera (`IsDefaultUiCamera`); bd_fx's word/shout billboards ignore inset cameras.
//!
//! # Summon ping
//! The **local** player's own free-standing constructs (creatures, vehicles, structures; never held /
//! worn / thrown): if the new construct is off-screen or behind terrain during its first second, a
//! corps-coloured arrow + ping ring pops in at the screen edge pointing toward it (or sits over the
//! spot when it is merely behind a wall) for ~1.5 s, fading early once it is seen, plus an `Info`
//! cut-in of the construct next to the arrow (`UiPrefs::summon_preview`; off = arrow only).
//! Recalled constructs (fell off the map, blinked back to their summoner) get a small hard-light
//! blink-in; the local player's own recall reuses the ping.
//!
//! Dev: `BD_SUMMON_PREVIEW_FORCE=1` keeps a preview up permanently (perf measurement);
//! `BD_SUMMON_TEST=phrase@yaw_deg@secs[;...]` casts `phrase` toward look-yaw + `yaw_deg` after `secs`
//! in game (`!turn@deg@secs` turns the view, `!alert@0@secs` sends an Alert cut-in of the newest own construct); `BD_SUMMON_PING_STATS=1`
//! prints the CPU cost of the ping + cut-in systems; `BD_GPU_DIAG=1` per-pass GPU timings.

use super::constructs::ConstructEntities;
use super::sound::Sfx3d;
use super::{bv, corps_rgb, gv, PresentSet};
use crate::arena::ArenaWorld;
use crate::camera::{CameraUpdateSet, PlayerCamera, ViewAnchor};
use crate::lobby::prefs::UiPrefs;
use crate::sim_bridge::{ActiveWorld, Cmd, GameEvents, SimCommand, WorldViews};
use crate::state::{AppState, InGameMenu};
use bd_core::mechanics::{Form, FormCategory};
use bd_core::ConstructId;
use bd_fx::{BurstFx, BurstKind, ConstructVisual};
use bd_sim::GameEvent;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, ClearColorConfig, Viewport};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::ecs::system::SystemParam;
use bevy::light::cluster::ClusterConfig;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::render::view::Msaa;
use bevy::transform::TransformSystems;
use bevy::ui::IsDefaultUiCamera;
use bevy::window::PrimaryWindow;

/// Render layer seen only by the cut-in camera.
pub const PREVIEW_LAYER: usize = 23;

/// Seconds after the spawn during which an unseen construct triggers the ping.
const WATCH: f32 = 1.0;
/// On-screen time that counts as "seen" (no ping afterwards).
const SEEN_OK: f32 = 0.25;
/// Ping lifetime / pop-in / fade-out.
const SHOW: f32 = 1.5;
const POP: f32 = 0.14;
const FADE: f32 = 0.18;
/// Visible this long while pinged ⇒ fade out early.
const VIS_EARLY: f32 = 0.12;
/// Info window interior (logical px) and frame border.
const PV_W: f32 = 160.0;
const PV_H: f32 = 120.0;
const BORDER: f32 = 2.0;
/// Alert window interior, border, hazard trim and label bar heights.
const AL_W: f32 = 224.0;
const AL_H: f32 = 140.0;
const AL_BORDER: f32 = 3.0;
const TRIM: f32 = 8.0;
const LABEL: f32 = 22.0;
/// Arrow inset from the screen edge (logical px).
const EDGE: f32 = 34.0;
/// Cut-in camera vertical FOV.
const PV_FOV: f32 = 30.0;
/// Most entities one cut-in puts on the preview layer.
const MAX_LAYERED: usize = 256;

// ------------------------------------------------------------------------------------ public API

/// What a cut-in shows.
#[allow(dead_code)] // public API (signature moves build against it)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CutInTarget {
    /// An entity (construct root, player costume root, …) and its mesh descendants.
    Entity { entity: Entity, radius: f32 },
    /// A world spot (pair with `show_world`).
    Point { pos: Vec3, radius: f32 },
}

/// How the cut-in camera frames the target.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CutInFraming {
    #[default]
    Auto,
    /// Closer; `low_angle` looks up at the target; orbits at `orbit_speed` rad/s.
    Dramatic { low_angle: bool, orbit_speed: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CutInStyle {
    #[default]
    Info,
    /// Bigger window, pulsing frame, hazard trim, label. `severity` 0..1 (pulse rate / intensity).
    Alert { severity: f32 },
}

#[allow(dead_code)] // public API (signature moves build against it)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CutInPlacement {
    /// Info: right edge; Alert: top centre.
    #[default]
    Auto,
    /// Frame centre in 0..1 screen units.
    Screen(Vec2),
}

/// Request a cut-in (see the module docs).
#[derive(Message, Clone, Debug)]
pub struct CutIn {
    pub target: CutInTarget,
    pub framing: CutInFraming,
    pub style: CutInStyle,
    pub label: Option<String>,
    /// Linear RGB (e.g. `present::corps_rgb`).
    pub color: [f32; 3],
    /// Seconds, including pop-in and fade-out.
    pub duration: f32,
    pub priority: u8,
    pub placement: CutInPlacement,
    pub show_world: bool,
}

#[allow(dead_code)] // public API (signature moves build against it)
impl CutIn {
    pub fn info(target: CutInTarget, color: [f32; 3], duration: f32) -> Self {
        Self { target, framing: CutInFraming::Auto, style: CutInStyle::Info, label: None, color, duration, priority: 0, placement: CutInPlacement::Auto, show_world: false }
    }
    pub fn alert(target: CutInTarget, color: [f32; 3], severity: f32, label: Option<String>) -> Self {
        Self { style: CutInStyle::Alert { severity: severity.clamp(0.0, 1.0) }, label, ..Self::info(target, color, 1.6) }
    }
    pub fn with_framing(mut self, f: CutInFraming) -> Self {
        self.framing = f;
        self
    }
    pub fn with_duration(mut self, secs: f32) -> Self {
        self.duration = secs;
        self
    }
    pub fn with_priority(mut self, p: u8) -> Self {
        self.priority = p;
        self
    }
    pub fn with_placement(mut self, p: CutInPlacement) -> Self {
        self.placement = p;
        self
    }
    pub fn with_world(mut self, on: bool) -> Self {
        self.show_world = on;
        self
    }
    fn rank(&self) -> (u8, u8) {
        (matches!(self.style, CutInStyle::Alert { .. }) as u8, self.priority)
    }
}

/// Systems that read [`CutIn`] messages (PostUpdate). Anything in `Update` may write them.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CutInSet;

pub struct SummonPingPlugin;

impl Plugin for SummonPingPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CutIn>()
            .init_resource::<SummonPing>()
            .init_resource::<CutIns>()
            .add_systems(PostStartup, setup)
            .add_systems(Update, (ping_events.in_set(PresentSet::Events), test_casts.before(crate::tick::FrameSet::Input)))
            .add_systems(
                PostUpdate,
                (
                    (summon_ping, update_cutins).chain().in_set(CutInSet).run_if(in_state(AppState::InGame)),
                )
                    .after(CameraUpdateSet)
                    .before(bevy::camera::CameraUpdateSystems)
                    .before(TransformSystems::Propagate)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_systems(OnExit(AppState::InGame), clear_all)
            .add_systems(OnExit(AppState::Loading), clear_all);
        if stats_on() || env_on("BD_GPU_DIAG") {
            // Dev measurement only: `BD_GPU_DIAG=1` (+ `WGPU_SETTINGS_PRIO=functionality` for GPU
            // timestamps) records per-pass render timings; both print a cumulative line every 5 s.
            if env_on("BD_GPU_DIAG") {
                app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
            }
            app.add_systems(Update, dev_report.run_if(in_state(AppState::InGame)));
        }
    }
}

// ------------------------------------------------------------------------------------ state

#[derive(Component)]
pub struct SummonPreviewCam;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum PingUi {
    Arrow,
    Ring,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum CutUi {
    Frame,
    TopTrim,
    Spacer,
    Bottom,
    Stripe,
    Label,
}

struct Track {
    id: ConstructId,
    color: [f32; 3],
    /// Seconds since the spawn / recall.
    t: f32,
    /// On-screen seconds before any ping.
    seen: f32,
    /// Seconds since the ping appeared.
    shown: Option<f32>,
    /// Continuous on-screen seconds while pinged.
    vis_run: f32,
    /// Seconds since the fade-out began.
    fade: Option<f32>,
    /// Where to point before the construct's view exists.
    fallback: Vec3,
    /// The cut-in was requested.
    offered: bool,
}

/// Where the summon arrow is this frame (the summon cut-in sits next to it).
#[derive(Clone, Copy)]
struct ArrowAnchor {
    arrow: Vec2,
    /// Unit vector from the arrow toward the screen interior.
    push: Vec2,
}

#[derive(Resource, Default)]
pub struct SummonPing {
    track: Option<Track>,
    anchor: Option<ArrowAnchor>,
    /// CPU cost samples of the ping + cut-in systems (µs), `BD_SUMMON_PING_STATS=1`.
    cost_us: Vec<f32>,
    /// Frames the cut-in camera was active.
    pub active_frames: u64,
}

struct ActiveCut {
    cut: CutIn,
    t: f32,
    fade: Option<f32>,
    /// Owned by the summon ping (placement follows its arrow; its fade is driven by the ping).
    summon: Option<ConstructId>,
    last_centre: Option<(Vec3, f32)>,
}

#[derive(Resource, Default)]
pub struct CutIns {
    active: Option<ActiveCut>,
    /// Entities we put on [`PREVIEW_LAYER`].
    layered: Vec<Entity>,
    /// Smoothed frame centre (logical px).
    frame_c: Option<Vec2>,
    label: Option<String>,
    scratch: Vec<Entity>,
}

impl CutIns {
    /// Offer a cut-in: replaces the current one if it ranks at least as high (or that one fades).
    fn offer(&mut self, cut: CutIn, summon: Option<ConstructId>) -> bool {
        let ok = match &self.active {
            None => true,
            Some(a) => a.fade.is_some() || cut.rank() >= a.cut.rank(),
        };
        if ok {
            let keep_pos = self.active.as_ref().is_some_and(|a| a.fade.is_none() && a.summon.is_some() && summon.is_some());
            if !keep_pos {
                self.frame_c = None;
            }
            self.active = Some(ActiveCut { cut, t: 0.0, fade: None, summon, last_centre: None });
        }
        ok
    }
}

fn env_on(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

fn force() -> bool {
    static F: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *F.get_or_init(|| env_on("BD_SUMMON_PREVIEW_FORCE"))
}

fn stats_on() -> bool {
    static F: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *F.get_or_init(|| env_on("BD_SUMMON_PING_STATS"))
}

/// Constructs that get a ping: free-standing summons, not things in hand / on the body / thrown.
fn eligible(form: Form) -> bool {
    !matches!(form.category(), FormCategory::Held | FormCategory::Worn) && !matches!(form, Form::Rocket | Form::Bomb | Form::Ball)
}

// ------------------------------------------------------------------------------------ setup

fn setup(mut commands: Commands, cams: Query<Entity, With<PlayerCamera>>) {
    // The cut-in camera has a higher order: keep UI on the gameplay camera.
    for e in &cams {
        commands.entity(e).insert(IsDefaultUiCamera);
    }
    commands.spawn((
        Name::new("CutInCamera"),
        SummonPreviewCam,
        Camera3d::default(),
        Camera {
            order: 10,
            is_active: false,
            // Own LDR target: clear it to transparent, composite premultiplied over the window.
            clear_color: ClearColorConfig::Custom(Color::NONE),
            output_mode: CameraOutputMode::Write { blend_state: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING), clear_color: ClearColorConfig::None },
            ..default()
        },
        Projection::Perspective(PerspectiveProjection { fov: PV_FOV.to_radians(), near: 0.05, ..default() }),
        Msaa::Off,
        // LDR: tonemapped in the material shader (matches the gameplay look), no post pass.
        Tonemapping::KhronosPbrNeutral,
        DebandDither::Disabled,
        // One light cluster (never zero-sized; the hard-light material is unlit anyway).
        ClusterConfig::Single,
        // The hard-light shader reads the view's fog uniform: bind one that never fogs (alpha 0).
        DistanceFog { color: Color::NONE, directional_light_color: Color::NONE, directional_light_exponent: 1.0, falloff: FogFalloff::Linear { start: 1.0e6, end: 2.0e6 } },
        RenderLayers::layer(PREVIEW_LAYER),
        Transform::default(),
    ));
    let hidden = Visibility::Hidden;
    commands.spawn((
        Name::new("SummonPingArrow"),
        PingUi::Arrow,
        Node { position_type: PositionType::Absolute, width: px(16.0), height: px(16.0), border: UiRect { top: px(3.5), right: px(3.5), ..default() }, ..default() },
        BorderColor::all(Color::NONE),
        UiTransform::default(),
        Pickable::IGNORE,
        GlobalZIndex(46),
        hidden,
    ));
    commands.spawn((
        Name::new("SummonPingRing"),
        PingUi::Ring,
        Node { position_type: PositionType::Absolute, width: px(20.0), height: px(20.0), border: UiRect::all(px(2.0)), border_radius: BorderRadius::MAX, ..default() },
        BorderColor::all(Color::NONE),
        UiTransform::default(),
        Pickable::IGNORE,
        GlobalZIndex(45),
        hidden,
    ));
    // Frame: [hazard trim] [picture] [label bar]; the camera viewport sits over the picture spacer.
    let bar = |kind: CutUi, h: f32| {
        (kind, Node { width: percent(100), height: px(h), overflow: Overflow::clip(), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::NONE))
    };
    let stripes = |p: &mut ChildSpawnerCommands| {
        for i in 0..14 {
            p.spawn((
                CutUi::Stripe,
                Node { position_type: PositionType::Absolute, left: px(i as f32 * 18.0 - 12.0), top: px(-10.0), width: px(8.0), height: px(44.0), ..default() },
                UiTransform { rotation: Rot2::degrees(40.0), ..default() },
                BackgroundColor(Color::NONE),
            ));
        }
    };
    commands
        .spawn((
            Name::new("CutInFrame"),
            CutUi::Frame,
            Node {
                position_type: PositionType::Absolute,
                width: px(PV_W + 2.0 * BORDER),
                height: px(PV_H + 2.0 * BORDER),
                border: UiRect::all(px(BORDER)),
                border_radius: BorderRadius::all(px(3.0)),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                ..default()
            },
            BorderColor::all(Color::NONE),
            BackgroundColor(Color::NONE),
            UiTransform::default(),
            Pickable::IGNORE,
            GlobalZIndex(44),
            hidden,
        ))
        .with_children(|f| {
            f.spawn(bar(CutUi::TopTrim, 0.0)).with_children(stripes);
            f.spawn((CutUi::Spacer, Node { width: percent(100), flex_grow: 1.0, ..default() }, BackgroundColor(Color::NONE)));
            f.spawn(bar(CutUi::Bottom, 0.0)).with_children(|b| {
                stripes(b);
                b.spawn((CutUi::Label, Text::new(""), TextFont::from_font_size(15.0), TextColor(Color::WHITE), TextLayout::justify(Justify::Center)));
            });
        });
}

fn clear_all(
    mut commands: Commands,
    mut st: ResMut<SummonPing>,
    mut cuts: ResMut<CutIns>,
    mut cam: Query<&mut Camera, With<SummonPreviewCam>>,
    mut ui: Query<&mut Visibility, Or<(With<PingUi>, With<CutUi>)>>,
) {
    st.track = None;
    st.anchor = None;
    cuts.active = None;
    cuts.frame_c = None;
    for e in cuts.layered.drain(..) {
        if let Ok(mut ec) = commands.get_entity(e) {
            ec.try_remove::<RenderLayers>();
        }
    }
    for mut c in &mut cam {
        if c.is_active {
            c.is_active = false;
        }
    }
    for mut v in &mut ui {
        *v = Visibility::Hidden;
    }
}

// ------------------------------------------------------------------------------------ events

fn ping_events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut st: ResMut<SummonPing>,
    mut burst: MessageWriter<BurstFx>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    for ev in events.read() {
        match &ev.event {
            GameEvent::ConstructSpawned { id, owner, recipe, pos, .. } => {
                if active.local != Some(*owner) || !eligible(recipe.form) {
                    continue;
                }
                if views.construct(*id).is_some_and(|c| c.attached()) {
                    continue;
                }
                st.track = Some(new_track(*id, corps_rgb(active.corps_of(*owner)), bv(*pos)));
            }
            GameEvent::ConstructRecalled { id, owner, to, .. } => {
                let p = bv(*to);
                let col = corps_rgb(active.corps_of(*owner));
                let size = views.construct(*id).map_or(1.0, |c| c.size.clamp(0.5, 4.0));
                // Hard-light blink-in: a white-hot flash and corps shards at the arrival point.
                burst.write(BurstFx { kind: BurstKind::Flash, pos: p + Vec3::Y * 0.6 * size, dir: Vec3::ZERO, scale: 1.2 * size, color: Some(col) });
                burst.write(BurstFx { kind: BurstKind::Shards, pos: p + Vec3::Y * 0.6 * size, dir: Vec3::Y, scale: 1.5 * size, color: Some(col) });
                sfx.write(Sfx3d::at("construct_form", p).pitch(1.35).gain(0.8));
                if active.local == Some(*owner) {
                    st.track = Some(new_track(*id, col, p));
                }
            }
            _ => {}
        }
    }
}

fn new_track(id: ConstructId, color: [f32; 3], fallback: Vec3) -> Track {
    Track { id, color, t: 0.0, seen: 0.0, shown: None, vis_run: 0.0, fade: None, fallback, offered: false }
}

// ------------------------------------------------------------------------------------ summon ping

/// World-space bounds centre + radius of a construct (rest pose).
fn bounds(tf: &Transform, v: &ConstructVisual) -> (Vec3, f32) {
    let b = &v.model.bounds;
    let (min, max) = (bv(b.min), bv(b.max));
    let s = tf.scale.abs().max_element().max(0.01);
    let c = tf.transform_point((min + max) * 0.5);
    let r = ((max - min).length() * 0.5 * s).max(0.3);
    (c, r)
}

/// Screen-space (logical px, y down) of a world point + its view-space position + in front.
fn project(cam: &Camera, view_from_world: &Mat4, p: Vec3, size: Vec2) -> (Vec2, Vec3, bool) {
    let v = view_from_world.transform_point3(p);
    let clip = cam.clip_from_view() * v.extend(1.0);
    let front = v.z < -0.05 && clip.w > 1e-4;
    let ndc = if front { clip.truncate() / clip.w } else { Vec3::ZERO };
    (Vec2::new((ndc.x + 1.0) * 0.5 * size.x, (1.0 - ndc.y) * 0.5 * size.y), v, front)
}

type MainCam<'w, 's> = Query<'w, 's, (&'static Camera, &'static Transform), (With<PlayerCamera>, Without<SummonPreviewCam>)>;
type ConstructRoots<'w, 's> = Query<'w, 's, (&'static Transform, &'static ConstructVisual), (Without<SummonPreviewCam>, Without<PlayerCamera>)>;

#[allow(clippy::too_many_arguments)]
fn summon_ping(
    time: Res<Time<Real>>,
    mut st: ResMut<SummonPing>,
    mut cuts: ResMut<CutIns>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    map: Res<ConstructEntities>,
    arena: Option<Res<ArenaWorld>>,
    prefs: Res<UiPrefs>,
    menu: Res<InGameMenu>,
    window: Single<&Window, With<PrimaryWindow>>,
    main: MainCam,
    constructs: ConstructRoots,
    mut ui: Query<(&PingUi, &mut Node, &mut UiTransform, &mut BorderColor, &mut Visibility)>,
) {
    let t0 = std::time::Instant::now();
    let dt = time.delta_secs().min(0.1);
    let st = &mut *st;
    let forced = force();
    let size = Vec2::new(window.width(), window.height());
    st.anchor = None;

    // Forced (perf / screenshots): keep a construct tracked, the local player's first.
    if forced
        && st.track.as_ref().is_none_or(|t| views.construct(t.id).is_none())
        && let Some(a) = active.as_ref()
    {
        let pick = views.constructs.iter().filter(|c| !c.attached() && map.0.contains_key(&c.id)).min_by_key(|c| (a.local != Some(c.owner), c.id.0));
        st.track = pick.map(|c| {
            let mut t = new_track(c.id, corps_rgb(a.corps_of(c.owner)), c.pos);
            t.shown = Some(POP);
            t
        });
    }

    struct Show {
        arrow: Vec2,
        angle: f32,
        alpha: f32,
        pop: f32,
        color: [f32; 3],
        shown_t: f32,
    }
    let mut show: Option<Show> = None;
    let blocked = menu.0 || active.is_none();
    if let (Some(tr), Ok((cam, cam_tf)), false) = (st.track.as_mut(), main.single(), blocked) {
        tr.t += dt;
        let view = views.construct(tr.id);
        let ent = map.0.get(&tr.id).copied().and_then(|e| constructs.get(e).ok().map(|x| (e, x)));
        let gone = view.is_none() && tr.t > 0.5;
        let attached = view.is_some_and(|c| c.attached());
        let (centre, radius) = match (&ent, view) {
            (Some((_, (tf, v))), _) => bounds(tf, v),
            (None, Some(v)) => (v.pos + Vec3::Y * v.size.max(0.5), v.size.max(0.5) * 1.5),
            _ => (tr.fallback + Vec3::Y, 1.5),
        };
        let eye = cam_tf.translation;
        let view_from_world = cam_tf.to_matrix().inverse();
        // Visible = one of a few sample points in front, inside the margin and not behind terrain.
        let right = cam_tf.rotation * Vec3::X;
        let samples = [centre, centre + Vec3::Y * radius * 0.6, centre + right * radius * 0.6, centre - right * radius * 0.6];
        let world = arena.as_ref().map(|a| a.read());
        let mut visible = false;
        let mut in_frame_occluded = false;
        for p in samples {
            let (s, _, front) = project(cam, &view_from_world, p, size);
            let m = Vec2::new(size.x * 0.06, size.y * 0.07);
            if !front || s.x < m.x || s.x > size.x - m.x || s.y < m.y || s.y > size.y - m.y {
                continue;
            }
            let d = p - eye;
            let len = d.length();
            let clear = match world.as_deref() {
                Some(w) if len > 0.5 => bd_core::voxel::raycast(w, gv(eye), gv(d / len), (len - radius * 0.5).max(0.3)).is_none(),
                _ => true,
            };
            if clear {
                visible = true;
                break;
            }
            in_frame_occluded = true;
        }
        drop(world);
        if visible {
            if tr.shown.is_none() {
                tr.seen += dt;
            }
            tr.vis_run += dt;
        } else {
            tr.vis_run = 0.0;
        }
        if tr.shown.is_none() && !visible && tr.seen < SEEN_OK && tr.t < WATCH && ent.is_some() {
            tr.shown = Some(0.0);
        }
        if let Some(s) = tr.shown.as_mut() {
            *s += dt;
            let s = *s;
            if !forced && tr.fade.is_none() && (s > SHOW - FADE || tr.vis_run > VIS_EARLY || gone || attached) {
                tr.fade = Some(0.0);
            }
        }
        if let Some(f) = tr.fade.as_mut() {
            *f += dt;
        }
        // The summon's cut-in: requested once, faded with the ping.
        if let (Some(_), false, Some((e, _))) = (tr.shown, tr.offered, ent.as_ref())
            && (prefs.summon_preview || forced)
        {
            tr.offered = true;
            let cut = CutIn::info(CutInTarget::Entity { entity: *e, radius }, tr.color, if forced { f32::MAX } else { SHOW + 0.1 });
            cuts.offer(cut, Some(tr.id));
        }
        if tr.fade.is_some()
            && let Some(a) = cuts.active.as_mut()
            && a.summon == Some(tr.id)
            && a.fade.is_none()
        {
            a.fade = Some(0.0);
        }
        let done = match (tr.shown, tr.fade) {
            (_, Some(f)) => f >= FADE,
            (None, None) => tr.t >= WATCH || tr.seen >= SEEN_OK || gone || attached,
            _ => false,
        };
        if done {
            st.track = None;
        } else if let Some(s) = tr.shown {
            let (sp, vpos, front) = project(cam, &view_from_world, centre, size);
            let half = size * 0.5;
            let (arrow, angle, push) = if front && in_frame_occluded && !visible {
                // Behind a wall but in frame: a marker over the spot, preview beside it.
                let push = if sp.x > half.x { Vec2::NEG_X } else { Vec2::X };
                (sp - Vec2::Y * 16.0, std::f32::consts::FRAC_PI_2, push)
            } else {
                let mut d = if front { sp - half } else { Vec2::new(vpos.x, -vpos.y) };
                if d.length_squared() < 1e-6 {
                    d = Vec2::Y;
                }
                let d = d.normalize();
                let ex = (half.x - EDGE).max(1.0) / d.x.abs().max(1e-5);
                let ey = (half.y - EDGE).max(1.0) / d.y.abs().max(1e-5);
                let push = if ex < ey { Vec2::new(-d.x.signum(), 0.0) } else { Vec2::new(0.0, -d.y.signum()) };
                (half + d * ex.min(ey), d.y.atan2(d.x), push)
            };
            let fade_k = tr.fade.map_or(1.0, |f| (1.0 - f / FADE).clamp(0.0, 1.0));
            st.anchor = Some(ArrowAnchor { arrow, push });
            show = Some(Show { arrow, angle, alpha: (s / 0.08).min(1.0) * fade_k, pop: ease_out_back((s / POP).min(1.0)), color: tr.color, shown_t: s });
        }
    } else if blocked && let Some(tr) = st.track.as_mut() {
        // Menu open: the clock keeps running, nothing shows.
        tr.t += dt;
        if tr.t > SHOW + WATCH {
            st.track = None;
        }
    }

    for (kind, mut node, mut ut, mut border, mut vis) in &mut ui {
        let want = if show.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let Some(sh) = show.as_ref() else { continue };
        match kind {
            PingUi::Arrow => {
                let s = 16.0;
                node.left = px(sh.arrow.x - s * 0.5);
                node.top = px(sh.arrow.y - s * 0.5);
                // The chevron's top-right corner points along `angle` (UI rotation is clockwise).
                ut.rotation = Rot2::radians(sh.angle + std::f32::consts::FRAC_PI_4);
                ut.scale = Vec2::splat(0.6 + 0.4 * sh.pop);
                *border = BorderColor::all(hot(sh.color, sh.alpha, 0.35));
            }
            PingUi::Ring => {
                let k = ((sh.shown_t % 0.7) / 0.45).min(1.0);
                let e = 1.0 - (1.0 - k).powi(3);
                let s = 14.0 + 34.0 * e;
                node.width = px(s);
                node.height = px(s);
                node.left = px(sh.arrow.x - s * 0.5);
                node.top = px(sh.arrow.y - s * 0.5);
                *border = BorderColor::all(hot(sh.color, sh.alpha * (1.0 - k) * 0.9, 0.2));
            }
        }
    }
    if stats_on() {
        st.cost_us.push(t0.elapsed().as_secs_f32() * 1e6);
    }
}

/// Corps colour lifted toward white by `w`, with alpha.
fn hot(c: [f32; 3], a: f32, w: f32) -> Color {
    Color::linear_rgba(c[0] * (1.0 - w) + w, c[1] * (1.0 - w) + w, c[2] * (1.0 - w) + w, a)
}

// ------------------------------------------------------------------------------------ cut-ins

#[derive(SystemParam)]
struct CutQueries<'w, 's> {
    main: MainCam<'w, 's>,
    preview: Query<'w, 's, (&'static mut Camera, &'static mut Transform, &'static mut Projection, &'static mut RenderLayers), (With<SummonPreviewCam>, Without<PlayerCamera>)>,
    roots: ConstructRoots<'w, 's>,
    globals: Query<'w, 's, &'static GlobalTransform>,
    children: Query<'w, 's, &'static Children>,
    meshes: Query<'w, 's, (), With<Mesh3d>>,
    layers: Query<'w, 's, (), With<RenderLayers>>,
    ui: Query<'w, 's, (&'static CutUi, &'static mut Node, &'static mut UiTransform, &'static mut BorderColor, &'static mut BackgroundColor, &'static mut Visibility)>,
    label: Query<'w, 's, (&'static mut Text, &'static mut TextColor), With<CutUi>>,
}

#[allow(clippy::too_many_arguments)]
fn update_cutins(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut reader: MessageReader<CutIn>,
    mut cuts: ResMut<CutIns>,
    mut st: ResMut<SummonPing>,
    menu: Res<InGameMenu>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut q: CutQueries,
) {
    let t0 = std::time::Instant::now();
    let dt = time.delta_secs().min(0.1);
    let cuts = &mut *cuts;
    for c in reader.read() {
        let mut c = c.clone();
        c.duration = if c.duration.is_finite() { c.duration.max(POP + FADE) } else { f32::MAX };
        cuts.offer(c, None);
    }
    let size = Vec2::new(window.width(), window.height());
    let sf = window.scale_factor();

    // ---- advance + resolve the active cut-in
    struct Frame {
        centre: Vec3,
        radius: f32,
        eye: Vec3,
        alpha: f32,
        scale: f32,
        t: f32,
    }
    let mut frame: Option<Frame> = None;
    let mut target_entity = None;
    let mut finished = false;
    if let Some(a) = cuts.active.as_mut() {
        a.t += dt;
        if a.fade.is_none() && a.t > a.cut.duration - FADE {
            a.fade = Some(0.0);
        }
        if let Some(f) = a.fade.as_mut() {
            *f += dt;
            finished = *f >= FADE;
        }
        let resolved = match a.cut.target {
            CutInTarget::Point { pos, radius } => Some((pos, radius.max(0.2))),
            CutInTarget::Entity { entity, radius } => {
                target_entity = Some(entity);
                if let Ok((tf, v)) = q.roots.get(entity) {
                    Some(bounds(tf, v))
                } else {
                    q.globals.get(entity).ok().map(|g| (g.translation(), radius.max(0.2)))
                }
            }
        };
        match resolved {
            Some(r) => a.last_centre = Some(r),
            None if a.fade.is_none() => a.fade = Some(0.0),
            None => {}
        }
        if let (Some((centre, radius)), Ok((_, cam_tf)), false) = (a.last_centre, q.main.single(), menu.0 || finished) {
            let fade_k = a.fade.map_or(1.0, |f| (1.0 - f / FADE).clamp(0.0, 1.0));
            let pop = ease_out_back((a.t / POP).min(1.0));
            frame = Some(Frame { centre, radius, eye: cam_tf.translation, alpha: (a.t / 0.08).min(1.0) * fade_k, scale: (0.55 + 0.45 * pop) * (0.35 + 0.65 * fade_k), t: a.t });
        }
    }
    if finished {
        cuts.active = None;
        cuts.frame_c = None;
    }
    let active = cuts.active.as_ref().filter(|_| frame.is_some());

    // ---- frame layout
    let (inner, border, top, bottom, alert) = match active.map(|a| a.cut.style) {
        Some(CutInStyle::Alert { severity }) => {
            let label = active.and_then(|a| a.cut.label.as_ref()).is_some();
            (Vec2::new(AL_W, AL_H), AL_BORDER, TRIM, if label { LABEL } else { TRIM }, Some(severity))
        }
        _ => (Vec2::new(PV_W, PV_H), BORDER, 0.0, 0.0, None),
    };
    let frame_size = inner + Vec2::new(2.0 * border, 2.0 * border + top + bottom);
    let mut rect: Option<(Vec2, f32)> = None;
    if let (Some(a), Some(fr)) = (active, frame.as_ref()) {
        let target = match (a.summon.and(st.anchor), a.cut.placement, alert) {
            (Some(an), _, _) => an.arrow + an.push * (frame_size.dot(an.push.abs()) * 0.5 + 24.0),
            (None, CutInPlacement::Screen(v), _) => v * size,
            (None, CutInPlacement::Auto, Some(_)) => Vec2::new(size.x * 0.5, size.y * 0.06 + frame_size.y * 0.5),
            (None, CutInPlacement::Auto, None) => Vec2::new(size.x - frame_size.x * 0.5 - 24.0, size.y * 0.5),
        };
        let lo = frame_size * 0.5 + Vec2::splat(8.0);
        let hi = (size - frame_size * 0.5 - Vec2::splat(8.0)).max(lo);
        let target = target.clamp(lo, hi);
        let c = match cuts.frame_c {
            Some(c) if fr.t > 0.02 => c.lerp(target, 1.0 - (-18.0 * dt).exp()),
            _ => target,
        };
        cuts.frame_c = Some(c);
        rect = Some((c, fr.scale));
    }
    let color = active.map_or([1.0; 3], |a| a.cut.color);
    let pulse = alert.map_or(0.0, |s| 0.5 + 0.5 * (frame.as_ref().map_or(0.0, |f| f.t) * (7.0 + 9.0 * s)).sin());
    let alpha = frame.as_ref().map_or(0.0, |f| f.alpha);
    for (kind, mut node, mut ut, mut bc, mut bg, mut vis) in &mut q.ui {
        let on = rect.is_some()
            && match kind {
                CutUi::TopTrim | CutUi::Bottom | CutUi::Stripe => alert.is_some(),
                CutUi::Label => alert.is_some() && active.and_then(|a| a.cut.label.as_ref()).is_some(),
                _ => true,
            };
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let Some((fc, scale)) = rect.filter(|_| on) else { continue };
        match kind {
            CutUi::Frame => {
                node.left = px(fc.x - frame_size.x * 0.5);
                node.top = px(fc.y - frame_size.y * 0.5);
                node.width = px(frame_size.x);
                node.height = px(frame_size.y);
                node.border = UiRect::all(px(border));
                ut.scale = Vec2::splat(scale);
                let w = if alert.is_some() { 0.1 + 0.5 * pulse } else { 0.15 };
                *bc = BorderColor::all(hot(color, alpha * 0.95, w));
                *bg = BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.5 * alpha));
            }
            CutUi::TopTrim => {
                node.height = px(top);
                *bg = BackgroundColor(Color::srgba(0.01, 0.01, 0.02, 0.85 * alpha));
            }
            CutUi::Bottom => {
                node.height = px(bottom);
                *bg = BackgroundColor(Color::srgba(0.01, 0.01, 0.02, 0.85 * alpha));
            }
            CutUi::Stripe => {
                *bg = BackgroundColor(hot(color, alpha * (0.75 + 0.25 * pulse), 0.1));
            }
            CutUi::Spacer | CutUi::Label => {}
        }
    }
    // Label text (stamped over the bottom trim).
    let want_label = active.and_then(|a| a.cut.label.clone()).filter(|_| alert.is_some());
    if want_label != cuts.label {
        cuts.label = want_label.clone();
        for (mut t, _) in &mut q.label {
            t.0 = want_label.clone().unwrap_or_default().to_uppercase();
        }
    }
    for (_, mut tc) in &mut q.label {
        let c = Color::linear_rgba(1.0, 1.0, 1.0, alpha);
        if tc.0 != c {
            tc.0 = c;
        }
    }

    // ---- camera
    let mut cam_on = false;
    if let (Some(a), Some(fr), Some((fc, scale)), Ok((mut pc, mut ptf, mut proj, mut layers))) = (active, frame.as_ref(), rect, q.preview.single_mut()) {
        // The picture spacer's centre relative to the frame centre (scaled with the frame).
        let off = Vec2::new(0.0, (top - bottom) * 0.5);
        let vc = fc + off * scale;
        let half = inner * scale * 0.5;
        let min = ((vc - half) * sf).round().max(Vec2::ZERO);
        let max = ((vc + half) * sf).round().min(Vec2::new(window.physical_width() as f32, window.physical_height() as f32));
        if max.x - min.x >= 4.0 && max.y - min.y >= 4.0 {
            cam_on = true;
            let vp = Viewport { physical_position: min.as_uvec2(), physical_size: (max - min).as_uvec2(), ..default() };
            if !same_vp(pc.viewport.as_ref(), &vp) {
                pc.viewport = Some(vp);
            }
            let want_layers = if a.cut.show_world { RenderLayers::from_layers(&[0, PREVIEW_LAYER]) } else { RenderLayers::layer(PREVIEW_LAYER) };
            if *layers != want_layers {
                *layers = want_layers;
            }
            // Frame from the viewer's side.
            let d = fr.centre - fr.eye;
            let flat = Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::NEG_Z);
            let (tilt, spin, close) = match a.cut.framing {
                CutInFraming::Auto => (0.32f32, 0.22, 1.08),
                CutInFraming::Dramatic { low_angle, orbit_speed } => (if low_angle { -0.22 } else { 0.18 }, orbit_speed, 0.85),
            };
            let flat = Quat::from_rotation_y(spin * fr.t) * flat;
            let dir = (flat * tilt.cos() - Vec3::Y * tilt.sin()).normalize();
            let dist = fr.radius / (PV_FOV.to_radians() * 0.5).sin() * close;
            ptf.translation = fr.centre - dir * dist;
            ptf.look_to(dir, Vec3::Y);
            if let Projection::Perspective(p) = proj.as_mut() {
                let near = ((dist - fr.radius * 1.3) * 0.5).max(0.05);
                if (p.near - near).abs() > 1e-3 {
                    p.near = near;
                }
                let fov = PV_FOV.to_radians();
                if (p.fov - fov).abs() > 1e-5 {
                    p.fov = fov;
                }
            }
        }
        if pc.is_active != cam_on {
            pc.is_active = cam_on;
        }
    } else if let Ok((mut pc, ..)) = q.preview.single_mut()
        && pc.is_active
    {
        pc.is_active = false;
    }
    if cam_on {
        st.active_frames += 1;
    }

    // ---- preview layer on the target and its mesh descendants (only while the camera draws)
    let mut want = std::mem::take(&mut cuts.scratch);
    want.clear();
    if let (true, Some(root)) = (cam_on, target_entity) {
        want.push(root);
        let mut i = 0;
        while i < want.len() && want.len() < MAX_LAYERED {
            if let Ok(ch) = q.children.get(want[i]) {
                want.extend(ch.iter().take(MAX_LAYERED - want.len()));
            }
            i += 1;
        }
        want.retain(|e| q.meshes.contains(*e));
    }
    cuts.layered.retain(|e| {
        if want.contains(e) {
            return true;
        }
        if let Ok(mut ec) = commands.get_entity(*e) {
            ec.try_remove::<RenderLayers>();
        }
        false
    });
    for e in &want {
        // Never touch an entity someone else put on a layer.
        if cuts.layered.contains(e) || q.layers.contains(*e) {
            continue;
        }
        commands.entity(*e).try_insert(RenderLayers::from_layers(&[0, PREVIEW_LAYER]));
        cuts.layered.push(*e);
    }
    cuts.scratch = want;

    if stats_on()
        && let Some(last) = st.cost_us.last_mut()
    {
        *last += t0.elapsed().as_secs_f32() * 1e6;
    }
}

fn same_vp(a: Option<&Viewport>, b: &Viewport) -> bool {
    a.is_some_and(|a| a.physical_position == b.physical_position && a.physical_size == b.physical_size)
}

fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let y = x - 1.0;
    1.0 + c3 * y * y * y + c1 * y * y
}

// ------------------------------------------------------------------------------------ dev casts

/// `BD_SUMMON_TEST=phrase@yaw_deg@secs[;...]`: cast `phrase` toward the look yaw + `yaw_deg`
/// (horizontal) `secs` after entering the game; `!alert` sends an Alert cut-in of the newest own
/// construct instead — scripted screenshots of the ping / cut-ins.
#[allow(clippy::too_many_arguments)]
fn test_casts(
    time: Res<Time<Real>>,
    state: Res<State<AppState>>,
    active: Option<Res<ActiveWorld>>,
    lex: Option<Res<crate::console::RingLexicon>>,
    anchor: Res<ViewAnchor>,
    mut look: ResMut<crate::input::LookAngles>,
    map: Res<ConstructEntities>,
    views: Res<WorldViews>,
    mut cmds: MessageWriter<SimCommand>,
    mut cut: MessageWriter<CutIn>,
    mut st: Local<(f32, usize)>,
) {
    static PLAN: std::sync::OnceLock<Vec<(String, f32, f32)>> = std::sync::OnceLock::new();
    let plan = PLAN.get_or_init(|| {
        std::env::var("BD_SUMMON_TEST")
            .unwrap_or_default()
            .split(';')
            .filter_map(|s| {
                let mut it = s.split('@');
                let phrase = it.next()?.trim().to_string();
                let yaw: f32 = it.next()?.trim().parse().ok()?;
                let at: f32 = it.next()?.trim().parse().ok()?;
                (!phrase.is_empty()).then_some((phrase, yaw, at))
            })
            .collect()
    });
    if plan.is_empty() || *state.get() != AppState::InGame {
        return;
    }
    let (Some(lex), Some(active)) = (lex, active) else { return };
    st.0 += time.delta_secs();
    while let Some((phrase, yaw, at)) = plan.get(st.1) {
        if st.0 < *at {
            break;
        }
        st.1 += 1;
        if phrase == "!turn" {
            // Turn away after casting (the look yaw only; the camera follows next frame).
            look.yaw += yaw.to_radians();
            continue;
        }
        if phrase == "!alert" {
            let newest = views.constructs.iter().filter(|c| active.local == Some(c.owner)).max_by_key(|c| c.id.0).and_then(|c| map.0.get(&c.id));
            if let (Some(&e), Some(me)) = (newest, active.local) {
                let col = corps_rgb(active.corps_of(me));
                cut.write(CutIn::alert(CutInTarget::Entity { entity: e, radius: 2.0 }, col, 0.8, Some("Incoming".into())).with_framing(CutInFraming::Dramatic { low_angle: true, orbit_speed: 0.6 }).with_duration(3.0).with_priority(5));
            }
            continue;
        }
        let res = lex.0.resolve(phrase);
        let fwd = look.forward();
        let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::NEG_Z);
        let dir = (Quat::from_rotation_y(yaw.to_radians()) * flat - Vec3::Y * 0.15).normalize();
        let aim = bd_sim::Aim::new(gv(anchor.eye), gv(dir));
        info!("summon test: casting {phrase:?} at yaw {yaw} from eye {:?} look yaw {:.1} deg", anchor.eye, look.yaw.to_degrees());
        cmds.write(SimCommand::local(Cmd::Cast { recipe: Box::new(res.recipe), shape: res.shape, alt_shape: res.alt_shape, aim, incantation: false }));
    }
}

// ------------------------------------------------------------------------------------ dev report

#[derive(Default)]
struct DevReport {
    in_game: f32,
    next: f32,
    last_batch: Option<std::time::Instant>,
    frames: u64,
    /// Per diagnostic path: summed ms since measuring began.
    gpu: std::collections::BTreeMap<String, f64>,
}

/// `BD_SUMMON_PING_STATS=1` / `BD_GPU_DIAG=1`: cumulative CPU cost of the ping + cut-in systems and,
/// when GPU timestamps are available, per-pass GPU ms per frame (both skip the first 3 s in game).
fn dev_report(time: Res<Time<Real>>, st: Res<SummonPing>, store: Option<Res<bevy::diagnostic::DiagnosticsStore>>, mut r: Local<DevReport>) {
    r.in_game += time.delta_secs();
    if r.in_game < 3.0 {
        return;
    }
    if let Some(store) = store.as_ref() {
        // Every render measurement synced this frame shares one timestamp.
        let newest = store.iter().filter_map(|d| d.measurement().map(|m| m.time)).max();
        if let Some(t) = newest
            && r.last_batch != Some(t)
        {
            r.last_batch = Some(t);
            r.frames += 1;
            for d in store.iter() {
                let path = d.path().as_str();
                if !(path.starts_with("render/") && path.ends_with("elapsed_gpu")) {
                    continue;
                }
                let sum: f64 = d.measurements().filter(|m| m.time == t).map(|m| m.value).sum();
                *r.gpu.entry(path.to_string()).or_default() += sum;
            }
        }
    }
    if r.in_game < r.next {
        return;
    }
    r.next = r.in_game + 5.0;
    if stats_on() && !st.cost_us.is_empty() {
        let mut v = st.cost_us.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        let avg = v.iter().sum::<f32>() / v.len() as f32;
        let p99 = v[((v.len() as f32 * 0.99) as usize).min(v.len() - 1)];
        println!("SUMMONPING frames={} preview_frames={} avg_us={avg:.1} p99_us={p99:.1} max_us={:.1}", v.len(), st.active_frames, v[v.len() - 1]);
    }
    if r.frames > 0 && !r.gpu.is_empty() {
        let n = r.frames as f64;
        let top: f64 = r.gpu.iter().filter(|(k, _)| k.matches('/').count() == 2).map(|(_, v)| v / n).sum();
        let list: Vec<String> = r.gpu.iter().map(|(k, v)| format!("{}={:.3}", k.trim_start_matches("render/").trim_end_matches("/elapsed_gpu"), v / n)).collect();
        println!("GPUDIAG frames={} top_level_ms={top:.3} {}", r.frames, list.join(" "));
    }
}
