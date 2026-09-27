//! PORTALS: see-through windows (owner: "make our portals allow full vision through … without
//! causing hitching/stuttering/frame drops").
//!
//! * **Window**: the entry disc shows the view from the exit. A portal-view camera mirrors the
//!   gameplay camera's pose relative to the entry, carried to the exit by the pair's rotation
//!   (`bd_sim::space::portal_rotation`), renders into an HDR picture, and the disc samples it at the
//!   fragment's *screen* position (bd_fx `portal.wgsl`) — it lines up like a window. An oblique near
//!   plane on the exit (Bevy's `PerspectiveProjection::near_clip_plane`) hides whatever is behind
//!   the exit (the wall it hangs on). The back face and far/culled portals keep the swirl; a portal
//!   seen *through* a portal is a swirl (portal cameras see only the swirl disc: no recursion).
//! * **No hitching** (hard requirement):
//!   - [`MAX_VIEWS`] pictures + cameras are created once at startup (fixed size: half the window,
//!     capped), never resized: dynamic resolution is the camera *viewport* inside the picture.
//!   - During Loading both cameras render the warm-up scene, so every pipeline they need (their view
//!     key differs from the gameplay camera's: HDR without MSAA / tonemapping / bloom) compiles
//!     behind the loading screen.
//!   - Live views are capped at [`MAX_VIEWS`], chosen by screen coverage (+ the one you are about to
//!     step through); the rest show the swirl. Small / far portals redraw every other frame.
//!   - Resolution follows coverage (¼…½ of the window) and a frame-time budget ([`BUDGET_MS`]):
//!     over budget it steps down, then drops to one live view.
//!   - No per-frame allocation (fixed arrays, reused buffers). With no portal on screen both cameras
//!     are inactive (zero cost).
//!   - `Settings::portal_views` ("PORTAL VIEWS", Video) off = swirl only, cameras never run.
//! * Dev: `BD_PORTAL_STATS=1` prints the live views / resolution / redraws every 2 s.

use super::camera3p::place_camera;
use super::{bq, bv, gv};
use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::settings::Settings;
use crate::sim_bridge::ActiveWorld;
use crate::state::AppState;
use bd_fx::PortalFx;
use bd_sim::space::{PORTAL_STYLE_DOOR, PortalEnd, PortalPair};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraProjection, Hdr, RenderTarget, Viewport};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::light::cluster::ClusterConfig;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::Msaa;
use bevy::transform::TransformSystems;
use bevy::window::PrimaryWindow;

/// Layer only the gameplay camera sees (it already carries the outline layer): the window disc.
pub const PORTAL_MAIN_LAYER: usize = super::outline::OUTLINE_LAYER;
/// Layer only the portal-view cameras see: the swirl disc (a portal inside a portal).
pub const PORTAL_VIEW_LAYER: usize = 26;
/// Layer nothing is on: an idle portal camera keeps its targets alive rendering it.
const PORTAL_IDLE_LAYER: usize = 27;
/// `BD_PORTAL_KEEPALIVE=1` (experiment, off by default): idle cameras keep their targets alive
/// while a portal exists. Measured: halves a new view's first-frame cost but costs ~0.7 ms every
/// frame a portal merely exists (Cybertron's bridge always does), so the default is truly idle.
fn keep_alive() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("BD_PORTAL_KEEPALIVE").is_ok_and(|v| v != "0"))
}
/// Live portal views at most.
pub const MAX_VIEWS: usize = 2;
/// Frame-time budget: live views may add this much (ms) to the frame time measured without them
/// (and never push it past `BUDGET_MS` when it was below); above it they lose resolution, then one
/// view. A vsync-capped frame never trips it (the cap hides the cost as long as it fits).
pub const BUDGET_MS: f32 = 7.5;
pub const BUDGET_EXTRA_MS: f32 = 2.0;
/// Below this screen coverage a portal is not worth a live view.
const MIN_COVERAGE: f32 = 0.002;
/// Below this coverage (or beyond `FAR_M`) a live view redraws every other frame.
const SMALL_COVERAGE: f32 = 0.04;
const FAR_M: f32 = 45.0;
/// A view whose portal left the screen stays set up this long (half rate, lowest resolution): a
/// quick look away and back costs nothing new.
const GRACE: f32 = 1.5;
/// Portals within this distance just past a screen edge get their view set up ahead of time.
const ANTICIPATE_M: f32 = 60.0;
/// Swirl → window blend time (s).
const FADE_IN: f32 = 0.12;
/// A new view's first frames widen its field of view step by step (the window shows the swirl
/// meanwhile): Bevy sets a view up for everything it sees on first sight (visibility, pipeline
/// specialisation, binning, shadow cascades) — ~4 ms of CPU in one frame when a portal opened in
/// full view; growing the frustum spreads it over a few frames.
const RAMP: [f32; 6] = [0.1, 0.22, 0.36, 0.52, 0.7, 0.86];
/// Frames of in-game warm-up after entering a match (tiny viewport, the gameplay pose).
const WARM_FRAMES: u32 = 40;

/// Dev / bench: force the live views off (A/B in one run) without touching the saved settings.
#[derive(Resource, Default)]
pub struct PortalViewsOff(pub bool);

/// Which end of which pair a `PortalFx` entity draws (inserted by `powers::sync_space`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalEndRef {
    pub id: u16,
    pub end: u8,
}

#[derive(Component)]
struct PortalViewCam(usize);

/// Portal-view state (pictures, slot assignment, resolution controller, stats).
#[derive(Resource)]
pub struct PortalViews {
    /// Picture size (physical px).
    pub size: UVec2,
    /// What each slot shows: (pair id, end index looked into).
    slots: [Option<(u16, u8)>; MAX_VIEWS],
    /// Seconds since the slot got its portal (fade in).
    age: [f32; MAX_VIEWS],
    /// Frames rendered since the slot got its portal (the view "grows in", see [`RAMP`]).
    ramp: [u32; MAX_VIEWS],
    /// Seconds the slot's portal has not been wanted (off screen): released after [`GRACE`].
    idle: [f32; MAX_VIEWS],
    /// Budget factor 0.5..1 (resolution) and the live-view cap it allows.
    budget: f32,
    cap: usize,
    ema_ms: f32,
    /// Frame time while no view was live (the baseline the budget adds to).
    base_ms: f32,
    /// In-game warm-up frames left (entering the match: what the match spawns — bodies, viewmodel,
    /// map props — gets its portal-view pipelines before any portal opens).
    warm: u32,
    frame: u32,
    stats: ViewStats,
}

#[derive(Default, Clone, Copy, Debug)]
struct ViewStats {
    frames: u32,
    live_frames: u32,
    redraws: u32,
    px: u64,
}

impl PortalViews {
    /// Live views right now (for HUD / benches).
    pub fn live(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

pub struct PortalViewPlugin;

impl Plugin for PortalViewPlugin {
    fn build(&self, app: &mut App) {
        super::portal_bench::build(app);
        crate::warmup::WarmupAppExt::add_warmup(app, warmup_portal_models);
        app.init_resource::<PortalViewsOff>()
            .add_systems(PostStartup, setup)
            .add_systems(
                PostUpdate,
                (
                    update_views.run_if(in_state(AppState::InGame)).run_if(resource_exists::<PortalViews>),
                    warmup_views.run_if(in_state(AppState::Loading)).run_if(resource_exists::<PortalViews>),
                )
                    .after(CameraUpdateSet)
                    .after(place_camera)
                    .after(super::portal_cross::fold_camera)
                    .before(bevy::camera::CameraUpdateSystems)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(OnExit(AppState::Loading), sleep_views)
            .add_systems(OnEnter(AppState::InGame), |v: Option<ResMut<PortalViews>>| {
                if let Some(mut v) = v {
                    v.warm = WARM_FRAMES;
                }
            })
            .add_systems(OnExit(AppState::InGame), sleep_views);
    }
}

// ------------------------------------------------------------------------------------ geometry

/// A portal end in render (Bevy) types.
#[derive(Clone, Copy, Debug)]
pub struct EndGeo {
    pub pos: Vec3,
    /// Out of the face (the side you enter from).
    pub n: Vec3,
    pub up: Vec3,
    /// Half extents of the opening (ring: radius, radius).
    pub half: Vec2,
    pub door: bool,
}

impl EndGeo {
    pub fn from_sim(e: &PortalEnd) -> Self {
        let door = e.style == PORTAL_STYLE_DOOR;
        let half = if door { Vec2::new(e.radius * bd_sim::space::DOOR_ASPECT, e.radius) } else { Vec2::splat(e.radius) };
        EndGeo { pos: bv(e.pos), n: bv(e.normal), up: bv(e.up), half, door }
    }
    pub fn right(&self) -> Vec3 {
        self.up.cross(self.n)
    }
    pub fn side(&self, p: Vec3) -> f32 {
        (p - self.pos).dot(self.n)
    }
    /// Is the in-plane offset `lat` inside the opening (grown by `margin`)?
    pub fn inside(&self, lat: Vec3, margin: f32) -> bool {
        let (x, y) = (lat.dot(self.right()), lat.dot(self.up));
        if self.door {
            x.abs() <= self.half.x + margin && y.abs() <= self.half.y + margin
        } else {
            x * x + y * y <= (self.half.x + margin) * (self.half.x + margin)
        }
    }
    /// Does the segment `a → b` pass this face front-to-back inside the opening?
    pub fn crossed(&self, a: Vec3, b: Vec3, margin: f32) -> bool {
        let (da, db) = (self.side(a), self.side(b));
        if !(da >= 0.0 && db < 0.0) {
            return false;
        }
        let t = da / (da - db);
        let hit = a + (b - a) * t;
        self.inside(hit - self.pos - self.n * self.side(hit), margin)
    }
}

/// Rotation for a pass in through `from` and out of `to` (same as `bd_sim::space::portal_rotation`).
pub fn rotation(from: &EndGeo, to: &EndGeo) -> Quat {
    let frame = |e: &EndGeo| Mat3::from_cols(e.up.cross(e.n), e.up, e.n);
    let f = frame(to);
    let exit = Mat3::from_cols(-f.x_axis, f.y_axis, -f.z_axis);
    Quat::from_mat3(&(exit * frame(from).transpose())).normalize()
}

/// A portal pair seen from the render side.
#[derive(Clone, Copy, Debug)]
pub struct PairGeo {
    pub id: u16,
    pub ends: [EndGeo; 2],
}

impl PairGeo {
    pub fn from_sim(p: &PortalPair) -> Self {
        PairGeo { id: p.id, ends: [EndGeo::from_sim(&p.ends[0]), EndGeo::from_sim(&p.ends[1])] }
    }
    /// From a replicated snapshot (interpolation of remote bodies).
    pub fn from_snap(p: &bd_sim::replicate::PortalSnap) -> Self {
        let end = |e: &bd_sim::replicate::PortalEndSnap| {
            let mut pe = PortalEnd::new(e.pos.get(), e.normal.get(), e.up.get(), e.radius as f32 / 20.0);
            pe.style = e.style;
            EndGeo::from_sim(&pe)
        };
        PairGeo { id: p.id, ends: [end(&p.ends[0]), end(&p.ends[1])] }
    }
    /// Map a point entering end `i` to the other side.
    pub fn map_point(&self, i: usize, p: Vec3) -> Vec3 {
        let (a, b) = (&self.ends[i & 1], &self.ends[1 - (i & 1)]);
        b.pos + rotation(a, b) * (p - a.pos)
    }
    pub fn rot(&self, i: usize) -> Quat {
        rotation(&self.ends[i & 1], &self.ends[1 - (i & 1)])
    }
}

/// Visual interpolation across a portal pass between two ticks: `a` (before) and `b` (after, on the
/// exit side). If `b` mapped back through a pair continues `a` into its entry, move along the
/// entry-side path and come out of the exit once past the plane. Returns (position, the pass
/// rotation, already out on the exit side).
pub fn portal_lerp(a: Vec3, b: Vec3, t: f32, pairs: &[PairGeo], max_step: f32) -> Option<(Vec3, Quat, bool)> {
    for pp in pairs {
        for j in 0..2 {
            // `b` near end j (the exit): back through j lands behind the entry (1 - j).
            let back = pp.map_point(j, b);
            let entry = &pp.ends[1 - j];
            if (back - a).length() > max_step || !entry.crossed(a, back, 0.5) {
                continue;
            }
            let p = a.lerp(back, t);
            let rot = pp.rot(1 - j);
            return Some(if entry.side(p) >= 0.0 { (p, rot, false) } else { (pp.map_point(1 - j, p), rot, true) });
        }
    }
    None
}

// ------------------------------------------------------------------------------------ setup

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<bd_fx::PortalMaterial>>,
    fx: Option<Res<bd_fx::FxMaterials>>,
    window: Query<&Window, With<PrimaryWindow>>,
    main: Query<Entity, With<PlayerCamera>>,
) {
    // Half the window, capped (1/4..1/2 resolution is the viewport inside it).
    let (w, h) = window.single().map_or((1600, 900), |w| (w.physical_width().max(640), w.physical_height().max(360)));
    let aspect = w as f32 / h as f32;
    let tw = (w / 2).clamp(480, 1600);
    let th = ((tw as f32 / aspect).round() as u32).clamp(270, 1000);
    let size = UVec2::new(tw, th);
    let images_arr: [Handle<Image>; MAX_VIEWS] = std::array::from_fn(|_| images.add(Image::new_target_texture(tw, th, TextureFormat::Rgba16Float, None)));
    if let Some(fx) = fx {
        if let Some(mut m) = mats.get_mut(&fx.portal) {
            m.view_a = Some(images_arr[0].clone());
            m.view_b = Some(images_arr[1].clone());
        }
    }
    for (slot, img) in images_arr.iter().enumerate() {
        commands.spawn((
            Name::new(format!("PortalViewCamera{slot}")),
            PortalViewCam(slot),
            Camera3d::default(),
            Camera { order: -3 - slot as isize, is_active: false, ..default() },
            RenderTarget::Image(img.clone().into()),
            // Linear HDR, no tonemapping / bloom / MSAA: the gameplay camera's post chain applies
            // once to the window disc, exactly like the rest of its picture.
            Hdr,
            Tonemapping::None,
            DebandDither::Disabled,
            Msaa::Off,
            // One light cluster: never zero-sized whatever the viewport (cf. the cut-in camera).
            ClusterConfig::Single,
            DistanceFog::default(),
            RenderLayers::from_layers(&[0, PORTAL_VIEW_LAYER]),
            Projection::Perspective(PerspectiveProjection { near: 0.05, far: 2000.0, ..default() }),
            Transform::default(),
        ));
    }
    // The gameplay camera must see the window layer (outline also adds it; idempotent).
    for e in &main {
        commands.entity(e).insert(RenderLayers::from_layers(&[0, PORTAL_MAIN_LAYER]));
    }
    commands.insert_resource(PortalViews {
        size,
        slots: [None; MAX_VIEWS],
        age: [0.0; MAX_VIEWS],
        ramp: [0; MAX_VIEWS],
        idle: [0.0; MAX_VIEWS],
        budget: 1.0,
        cap: MAX_VIEWS,
        ema_ms: 6.0,
        base_ms: 6.0,
        warm: 0,
        frame: 0,
        stats: ViewStats::default(),
    });
}

/// Loading: build the default gate ring and doorway frame meshes (the model cache keeps them), so the
/// first portal of the match never voxelizes / meshes on the frame it opens.
fn warmup_portal_models(world: &mut World) {
    let r = bd_sim::space::PORTAL_RADIUS;
    let layers = Some([PORTAL_MAIN_LAYER, PORTAL_VIEW_LAYER]);
    let mut ring = PortalFx::new(r * 2.0 + 0.8, [0.2, 1.0, 0.35]);
    ring.view_layers = layers;
    let mut door = PortalFx::new(3.0, [0.2, 1.0, 0.35]);
    door.style = PortalFx::DOOR;
    door.door_aspect = bd_sim::space::DOOR_ASPECT;
    door.view_layers = layers;
    for fx in [ring, door] {
        world.spawn((Name::new("portal warmup"), crate::warmup::WarmupItem, fx, Transform::default()));
    }
}

/// Loading: both cameras render the warm-up scene from the gameplay pose (full picture), so their
/// pipelines compile behind the loading screen.
fn warmup_views(
    main: Query<(&Transform, &Projection, Option<&DistanceFog>), (With<PlayerCamera>, Without<PortalViewCam>)>,
    views: Res<PortalViews>,
    mut cams: Query<(&mut Camera, &mut Transform, &mut Projection, &mut DistanceFog), With<PortalViewCam>>,
    mut frames: Local<u32>,
) {
    let Ok((mtf, mproj, mfog)) = main.single() else { return };
    *frames += 1;
    for (mut c, mut tf, mut proj, mut fog) in &mut cams {
        *tf = *mtf;
        if let (Projection::Perspective(p), Projection::Perspective(m)) = (proj.as_mut(), mproj) {
            p.fov = m.fov;
            p.near = 0.05;
            // Exercise the oblique path too (same pipelines; the matrix is all that changes).
            p.near_clip_plane = if *frames % 2 == 0 { Vec4::new(0.0, 0.0, -1.0, -0.05) } else { Vec3::new(0.0, 0.2, -0.98).normalize().extend(-0.5) };
        }
        if let Some(f) = mfog {
            *fog = f.clone();
        }
        if c.viewport.as_ref().map(|v| v.physical_size) != Some(views.size) {
            c.viewport = Some(Viewport { physical_position: UVec2::ZERO, physical_size: views.size, ..default() });
        }
        if !c.is_active {
            c.is_active = true;
        }
    }
}

fn sleep_views(mut cams: Query<&mut Camera, With<PortalViewCam>>, mut views: Option<ResMut<PortalViews>>, mut fx: Query<&mut PortalFx>) {
    for mut c in &mut cams {
        if c.is_active {
            c.is_active = false;
        }
    }
    if let Some(v) = views.as_mut() {
        v.slots = [None; MAX_VIEWS];
    }
    for mut f in &mut fx {
        f.view = 0;
        f.view_mix = 0.0;
    }
}

// ------------------------------------------------------------------------------------ per frame

/// Screen coverage (0..1) of a disc of radius `r` at `c`, seen by a camera (`vp` = clip ← world).
fn coverage(cam: Vec3, fwd: Vec3, vp: &Mat4, c: Vec3, r: f32, focal_y: f32, aspect: f32, margin: f32) -> f32 {
    let d = c - cam;
    let dist = d.length();
    if dist < r * 1.05 {
        return 1.0;
    }
    // Behind the camera (with the sphere's extent).
    if d.dot(fwd) < -r {
        return 0.0;
    }
    let clip = *vp * c.extend(1.0);
    if clip.w > 1e-3 {
        let ndc = clip.truncate() / clip.w;
        let rr = r / clip.w * focal_y;
        // Entirely off screen?
        let m = 1.0 + margin;
        if ndc.x - rr / aspect > m || ndc.x + rr / aspect < -m || ndc.y - rr > m || ndc.y + rr < -m {
            return 0.0;
        }
        let area = std::f32::consts::PI * rr * rr / aspect / 4.0;
        return area.clamp(0.0, 1.0);
    }
    0.5
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_views(
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    off: Res<PortalViewsOff>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    active: Option<Res<ActiveWorld>>,
    main: Query<(&Transform, &Projection, Option<Ref<DistanceFog>>, &Camera), (With<PlayerCamera>, Without<PortalViewCam>)>,
    mut views: ResMut<PortalViews>,
    mut cams: Query<(&PortalViewCam, &mut Camera, &mut Transform, &mut Projection, &mut DistanceFog, &mut RenderLayers), Without<PlayerCamera>>,
    mut ends: Query<(&PortalEndRef, &mut PortalFx)>,
    mut pairs: Local<Vec<PairGeo>>,
    mut cand: Local<Vec<(f32, u16, u8, f32, f32)>>,
) {
    let dt = time.delta_secs().min(0.1);
    let v = &mut *views;
    v.frame = v.frame.wrapping_add(1);
    let ms = dt * 1000.0;
    v.ema_ms += (ms - v.ema_ms) * 0.08;
    pairs.clear();
    if let Some(a) = active.as_ref() {
        pairs.extend(a.world.portals.iter().map(PairGeo::from_sim));
    }
    let Ok((mtf, mproj, mfog, mcam)) = main.single() else { return };
    let enabled = settings.portal_views && !off.0 && !pairs.is_empty();
    // ---- pick the live views
    cand.clear();
    if enabled && let Projection::Perspective(mp) = mproj {
        let cam = mtf.translation;
        let fwd = mtf.forward().as_vec3();
        let aspect = mp.aspect_ratio.max(0.1);
        let focal_y = 1.0 / (mp.fov * 0.5).tan();
        let vp = mp.get_clip_from_view() * mtf.to_matrix().inverse();
        for pp in pairs.iter() {
            for (i, e) in pp.ends.iter().enumerate() {
                let s = e.side(cam);
                if s < -0.02 {
                    continue; // the back face: swirl
                }
                let r = e.half.max_element();
                let dist = (e.pos - cam).length();
                let mut cov = coverage(cam, fwd, &vp, e.pos, r, focal_y, aspect, 0.0);
                if cov < MIN_COVERAGE {
                    // About to come on screen (just past an edge, not far): set its view up ahead
                    // of time at the lowest cost (half rate, lowest resolution) so the first frame
                    // it shows never pays for a new view.
                    if dist < ANTICIPATE_M && coverage(cam, fwd, &vp, e.pos, r, focal_y, aspect, 0.7) > 0.0 {
                        cov = 0.0;
                    } else {
                        continue;
                    }
                }
                // About to step through (close, inside the opening): always live, top priority.
                let lat = cam - e.pos - e.n * s;
                let imminent = s < 3.0 && e.inside(lat, 0.6);
                let score = cov + if imminent { 10.0 } else { 0.0 };
                cand.push((score, pp.id, i as u8, cov, dist));
            }
        }
        cand.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    }
    // Budget: over → lower resolution, then one view; well under → recover.
    let live_now = v.slots.iter().filter(|s| s.is_some()).count();
    if live_now == 0 {
        v.base_ms += (ms - v.base_ms) * 0.05;
    }
    let limit = BUDGET_MS.max(v.base_ms + BUDGET_EXTRA_MS);
    if live_now > 0 && v.ema_ms > limit {
        v.budget = (v.budget - dt * 0.8).max(0.5);
        if v.budget <= 0.5 && v.ema_ms > limit * 1.15 {
            v.cap = 1;
        }
    } else if v.ema_ms < limit * 0.85 {
        v.budget = (v.budget + dt * 0.4).min(1.0);
        if v.budget >= 0.9 {
            v.cap = MAX_VIEWS;
        }
    }
    let want = cand.len().min(v.cap);
    // In-game warm-up (no portal wanted yet): a tiny viewport looking around from the gameplay pose
    // and from above the arena, so the map, bodies and props visible from anywhere get their
    // portal-view pipelines / per-view setup now, not on the frame a portal first shows them.
    if v.warm > 0 && want == 0 {
        v.warm -= 1;
        let on = v.warm > 0;
        let k = v.warm as usize;
        const DIRS: [Vec3; 6] = [Vec3::NEG_Z, Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Y, Vec3::Y];
        let centre = arena.as_ref().map_or(mtf.translation, |a| a.info.center + Vec3::Y * 30.0);
        for (i, (_, mut c, mut tf, mut proj, _, mut layers)) in cams.iter_mut().enumerate() {
            if *layers != RenderLayers::from_layers(&[0, PORTAL_VIEW_LAYER]) {
                *layers = RenderLayers::from_layers(&[0, PORTAL_VIEW_LAYER]);
            }
            let j = k * MAX_VIEWS + i;
            *tf = match j % 13 {
                0 => *mtf,
                n => {
                    let d = DIRS[(n - 1) % 6];
                    let from = if n <= 6 { mtf.translation } else { centre };
                    let up = if d.y.abs() > 0.5 { Vec3::Z } else { Vec3::Y };
                    Transform::from_translation(from).looking_to(d, up)
                }
            };
            if let Projection::Perspective(p) = proj.as_mut() {
                p.fov = 100f32.to_radians();
                p.near_clip_plane = Vec4::new(0.0, 0.0, -1.0, -p.near);
            }
            if on && c.viewport.as_ref().map(|v| v.physical_size) != Some(UVec2::new(64, 36)) {
                c.viewport = Some(Viewport { physical_position: UVec2::ZERO, physical_size: UVec2::new(64, 36), ..default() });
            }
            if c.is_active != on {
                c.is_active = on;
            }
        }
        return;
    }
    v.warm = 0;
    // Keep existing assignments that are still wanted (or went off screen less than `GRACE` ago,
    // while no wanted portal needs the slot); fill free slots with the rest.
    let wanted = |id: u16, end: u8, cand: &[(f32, u16, u8, f32, f32)]| cand[..want].iter().any(|c| c.1 == id && c.2 == end);
    let unserved = cand[..want].iter().filter(|c| !v.slots.contains(&Some((c.1, c.2)))).count();
    let mut need = unserved;
    for s in 0..MAX_VIEWS {
        if let Some((id, end)) = v.slots[s] {
            if wanted(id, end, &cand) {
                v.idle[s] = 0.0;
            } else {
                v.idle[s] += dt;
                let exists = pairs.iter().any(|p| p.id == id);
                if !exists || v.idle[s] > GRACE || need > 0 || !enabled {
                    v.slots[s] = None;
                    need = need.saturating_sub(1);
                }
            }
        }
    }
    for c in cand[..want].iter() {
        if v.slots.iter().any(|s| *s == Some((c.1, c.2))) {
            continue;
        }
        if let Some(s) = (0..MAX_VIEWS).find(|&s| v.slots[s].is_none()) {
            v.slots[s] = Some((c.1, c.2));
            v.age[s] = 0.0;
            v.ramp[s] = 0;
            v.idle[s] = 0.0;
        }
    }
    // ---- drive the cameras
    let win = mcam.physical_viewport_size().unwrap_or(UVec2::new(1600, 900)).max(UVec2::ONE);
    let win_aspect = win.x as f32 / win.y as f32;
    let mut uv_of: [Vec2; MAX_VIEWS] = [Vec2::ONE; MAX_VIEWS];
    v.stats.frames += 1;
    for (vc, mut c, mut tf, mut proj, mut fog, mut layers) in &mut cams {
        let s = vc.0;
        let Some((id, end)) = v.slots[s].filter(|_| enabled) else {
            // Idle with a portal in the world: keep the camera's GPU targets alive (Bevy frees view
            // textures unused for 3 frames; re-creating them is what made a portal's first live frame
            // cost a few ms). Every other frame it "renders" nothing: its layer holds no entity, the
            // sun ignores it (no shadow cascades), 16×16 viewport. No portal at all: fully inactive.
            let keep = enabled && keep_alive();
            let draw = keep && (v.frame + s as u32) % 2 == 0;
            if keep && *layers != RenderLayers::layer(PORTAL_IDLE_LAYER) {
                *layers = RenderLayers::layer(PORTAL_IDLE_LAYER);
            }
            if draw && c.viewport.as_ref().map(|v| v.physical_size) != Some(UVec2::splat(16)) {
                c.viewport = Some(Viewport { physical_position: UVec2::ZERO, physical_size: UVec2::splat(16), ..default() });
            }
            if c.is_active != draw {
                c.is_active = draw;
            }
            continue;
        };
        if *layers != RenderLayers::from_layers(&[0, PORTAL_VIEW_LAYER]) {
            *layers = RenderLayers::from_layers(&[0, PORTAL_VIEW_LAYER]);
        }
        let Some(pp) = pairs.iter().find(|p| p.id == id) else {
            c.is_active = false;
            continue;
        };
        let (cov, dist) = cand.iter().find(|x| x.1 == id && x.2 == end).map_or((0.0, 1000.0), |x| (x.3, x.4));
        let (e, f) = (&pp.ends[end as usize], &pp.ends[1 - end as usize]);
        // Pose: the gameplay camera carried from the entry to the exit.
        let rot = rotation(e, f);
        let vpos = f.pos + rot * (mtf.translation - e.pos);
        let vrot = (rot * mtf.rotation).normalize();
        tf.translation = vpos;
        tf.rotation = vrot;
        // Resolution: coverage × budget, ¼…½ of the window (the picture is ½ window).
        let base = (0.55 + cov * 1.8).clamp(0.5, 1.0);
        let scale = (base * v.budget).clamp(0.5, 1.0);
        let fit_w = (v.size.x as f32).min(v.size.y as f32 * win_aspect);
        let vw = ((fit_w * scale / 16.0).round() * 16.0).clamp(64.0, v.size.x as f32);
        let vh = ((vw / win_aspect).round()).clamp(36.0, v.size.y as f32);
        let vsize = UVec2::new(vw as u32, vh as u32);
        if c.viewport.as_ref().map(|v| v.physical_size) != Some(vsize) {
            c.viewport = Some(Viewport { physical_position: UVec2::ZERO, physical_size: vsize, ..default() });
        }
        uv_of[s] = Vec2::new(vw / v.size.x as f32, vh / v.size.y as f32);
        let grow = RAMP.get(v.ramp[s] as usize).copied().unwrap_or(1.0);
        if let (Projection::Perspective(p), Projection::Perspective(m)) = (proj.as_mut(), mproj) {
            p.fov = m.fov * grow;
            p.near = m.near.max(0.02);
            // Oblique near plane on the exit: keep only what is in front of it (just past the exit's
            // own disc, which would otherwise cover the view).
            let n_v = (vrot.inverse() * f.n).normalize();
            let q_v = vrot.inverse() * ((f.pos + f.n * 0.03) - vpos);
            let d = n_v.dot(q_v);
            p.near_clip_plane = if d > 0.06 { n_v.extend(-d) } else { Vec4::new(0.0, 0.0, -1.0, -p.near) };
        }
        if let Some(mf) = mfog.as_ref()
            && (mf.is_changed() || v.age[s] == 0.0)
        {
            *fog = (**mf).clone();
        }
        // Small / far portals redraw every other frame (the picture holds in between).
        let fresh = (v.ramp[s] as usize) <= RAMP.len();
        let half_rate = cov < SMALL_COVERAGE || dist > FAR_M;
        let draw = fresh || !half_rate || (v.frame + s as u32) % 2 == 0;
        if c.is_active != draw {
            c.is_active = draw;
        }
        if draw {
            v.ramp[s] = v.ramp[s].saturating_add(1);
            v.stats.redraws += 1;
            v.stats.px += (vw * vh) as u64;
        }
        v.age[s] += dt.max(1e-4);
    }
    if v.slots.iter().any(|s| s.is_some()) && enabled {
        v.stats.live_frames += 1;
    }
    // ---- tell the discs
    for (r, mut fx) in &mut ends {
        let slot = if enabled { v.slots.iter().position(|s| *s == Some((r.id, r.end))) } else { None };
        let (view, uv, mix) = match slot {
            // Swirl while the view grows in, then a quick blend to the window.
            Some(s) if (v.ramp[s] as usize) > RAMP.len() => {
                (s as u8 + 1, uv_of[s], (v.age[s] / FADE_IN).clamp(0.35, 1.0))
            }
            Some(_) => (0, Vec2::ONE, 0.0),
            None => (0, Vec2::ONE, 0.0),
        };
        if fx.view != view || fx.view_uv != uv || (fx.view_mix - mix).abs() > 1e-4 {
            fx.view = view;
            fx.view_uv = uv;
            fx.view_mix = mix;
        }
    }
    if stats_on() && v.stats.frames >= 240 {
        let s = v.stats;
        info!(
            "PORTAL VIEWS live {:.0}% of frames, {:.2} redraws/frame, {:.0} kpx/redraw, budget {:.2} cap {} ema {:.2} ms",
            s.live_frames as f32 * 100.0 / s.frames as f32,
            s.redraws as f32 / s.frames as f32,
            if s.redraws > 0 { s.px as f32 / s.redraws as f32 / 1000.0 } else { 0.0 },
            v.budget,
            v.cap,
            v.ema_ms
        );
        v.stats = ViewStats::default();
    }
    let _ = (bq, gv);
}

fn stats_on() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("BD_PORTAL_STATS").is_ok_and(|v| v != "0"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(pos: Vec3, n: Vec3) -> EndGeo {
        EndGeo { pos, n, up: Vec3::Y, half: Vec2::splat(2.4), door: false }
    }

    #[test]
    fn rotation_matches_the_sim() {
        let a = PortalEnd::new(bd_core::Vec3::ZERO, bd_core::Vec3::Y, bd_core::Vec3::Z, 1.5);
        let b = PortalEnd::new(bd_core::Vec3::new(20.0, 5.0, 0.0), bd_core::Vec3::X, bd_core::Vec3::Y, 1.5);
        let r_sim = bq(bd_sim::space::portal_rotation(&a, &b));
        let r = rotation(&EndGeo::from_sim(&a), &EndGeo::from_sim(&b));
        assert!(r_sim.angle_between(r) < 1e-4, "{r_sim} {r}");
    }

    #[test]
    fn interpolation_through_a_portal_stays_on_the_path() {
        let pp = PairGeo { id: 1, ends: [wall(Vec3::ZERO, Vec3::Z), wall(Vec3::new(30.0, 0.0, 0.0), Vec3::X)] };
        // Before: 0.3 m in front of end 0 moving −Z; after: 0.5 m out of end 1 (moving +X).
        let a = Vec3::new(0.0, 0.0, 0.3);
        let b = pp.map_point(0, Vec3::new(0.0, 0.0, -0.5));
        let (p0, _, out0) = portal_lerp(a, b, 0.2, &[pp], 4.0).expect("detected");
        assert!(!out0 && p0.z > 0.0, "still entering: {p0}");
        let (p1, _, out1) = portal_lerp(a, b, 0.9, &[pp], 4.0).expect("detected");
        assert!(out1 && p1.x > 30.0 && (p1.z).abs() < 0.2, "out of the exit: {p1}");
        // No portal between: none.
        assert!(portal_lerp(a, a + Vec3::X, 0.5, &[pp], 4.0).is_none());
    }

    #[test]
    fn coverage_is_sane() {
        let proj = PerspectiveProjection { fov: 1.2, aspect_ratio: 16.0 / 9.0, ..default() };
        let vp = proj.get_clip_from_view();
        let f = 1.0 / (0.6f32).tan();
        let near = coverage(Vec3::ZERO, Vec3::NEG_Z, &vp, Vec3::new(0.0, 0.0, -3.0), 2.4, f, 16.0 / 9.0, 0.0);
        let far = coverage(Vec3::ZERO, Vec3::NEG_Z, &vp, Vec3::new(0.0, 0.0, -80.0), 2.4, f, 16.0 / 9.0, 0.0);
        let behind = coverage(Vec3::ZERO, Vec3::NEG_Z, &vp, Vec3::new(0.0, 0.0, 30.0), 2.4, f, 16.0 / 9.0, 0.0);
        // Just past the right edge: off screen, but anticipated with a margin.
        let edge = Vec3::new(9.0, 0.0, -6.0);
        assert_eq!(coverage(Vec3::ZERO, Vec3::NEG_Z, &vp, edge, 1.0, f, 16.0 / 9.0, 0.0), 0.0);
        assert!(coverage(Vec3::ZERO, Vec3::NEG_Z, &vp, edge, 1.0, f, 16.0 / 9.0, 0.7) > 0.0);
        assert!(near > 0.3 && far < 0.01 && far > 0.0 && behind == 0.0, "{near} {far} {behind}");
    }
}
