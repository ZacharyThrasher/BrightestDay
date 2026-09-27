//! First / third person as first-class views (CAMERA worker; DESIGN.md "Motion comfort").
//!
//! * **Toggle** ([`Action::ToggleView`], F5): a quick dolly between the eye and the shoulder
//!   ([`TRANSITION_SECS`], smoothstep; the rotation never changes, so nothing lurches). Comfort
//!   preset = an instant cut. The view is a persisted pref ([`CameraPrefs::third`], Settings →
//!   "DEFAULT VIEW"); `UiPrefs::third_person` mirrors the *live* view for older readers
//!   (viewmodel). Hands ⇄ body swap at [`BODY_SWAP`] of the dolly. Host rule
//!   `MatchRules::allow_third_person` (default on) forces first person (vehicle seats keep their
//!   chase camera). `--third-person` forces it for a run without touching prefs.
//! * **Over the shoulder**: [`solve_boom`] pulls the camera in against voxels (lateral eye → pivot
//!   ray + a 5-ray cone behind the pivot + a final never-inside-a-voxel guarantee) and enemy /
//!   world constructs. Pull-in is immediate at the hard limit and eased ahead of it by a wider
//!   "soft" cone; it eases back out. Shoulder swap ([`Action::Shoulder`], B) slides sides; a shoulder
//!   jammed against a wall auto-swaps (and swaps back once clear). High speed stretches the boom a
//!   little (comfort-gated with speed FOV). ADS tightens it over the shoulder.
//! * **Aim convergence — the crosshair is the truth.** The camera ray (voxels, other players'
//!   rendered hulls = the lag-compensated view, enemy constructs; anything nearer than the
//!   character is ignored) finds the target point; shots leave the *eye* (where hitscan and tether
//!   queries start; muzzles are cosmetic offsets of it) toward that point. [`correct_aim`] writes
//!   the converged yaw/pitch into the tick's `PlayerInput` — only on ticks where aim matters
//!   (fire / alt / ADS held, a tether press), so movement (which reads yaw) is identical to first
//!   person otherwise. Host, prediction and lag compensation all consume that one input, so parity
//!   holds by construction. [`AimRay`] carries the same solution to casts (console) and summons.
//!   Near-wall parallax: if the eye's or hand's path to the point is blocked close to the character,
//!   the crosshair turns amber and a marker shows where the shot will actually land.
//! * **The body**: the local player's `WorldViews` entry is replaced every frame by the predicted
//!   state (client mode would otherwise draw it at the delayed host position) and faces the aim
//!   point; [`ThirdPersonHands`] lets the tether shell start ropes at the body's hands.
//! * [`ViewMode`] / [`AimRay`] are read by players, constructs, charge, director, console.

use crate::arena::ArenaWorld;
use crate::camera::{CameraRig, CameraUpdateSet, PlayerCamera, ViewAnchor};
use crate::input::{LocalInputState, LookAngles, UiFocus};
use crate::juice::CameraShake;
use crate::lobby::prefs::{Action, KeyBinds, UiPrefs};
use crate::player::{LocalPlayer, MoveTuning, PlayerSim};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, BridgeSet, WorldViews};
use crate::state::{AppState, InGameMenu};
use crate::tick::{FrameSet, SimSet, SimTick, TickClock};
use crate::util::{core_v3, v3};
use bd_core::PlayerId;
use bd_core::input::buttons;
use bd_core::voxel::{VoxelQuery, raycast, world_to_voxel};
use bd_sim::movement::HookPhase;
use bd_sim::replicate::pflags;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::PrimaryWindow;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------------------------ tuning

/// Hip over-the-shoulder offset (look space: +x right, +y up) and boom length.
const SHOULDER: Vec3 = Vec3::new(0.8, 0.4, 0.0);
const BOOM: f32 = 3.0;
/// Aiming down sights: tighter over the shoulder.
const SHOULDER_ADS: Vec3 = Vec3::new(0.85, 0.34, 0.0);
const BOOM_ADS: f32 = 1.9;
/// Extra boom at high speed (bhop / swing / launch read better), comfort-gated.
const SPEED_BOOM: f32 = 0.9;
/// 1P ⇄ 3P dolly duration.
pub const TRANSITION_SECS: f32 = 0.17;
/// Dolly fraction where the first-person hands hand over to the body.
pub const BODY_SWAP: f32 = 0.3;
/// Camera stand-off from surfaces (near plane 0.05 + safety).
const PAD: f32 = 0.28;
/// Hard-collision cone radius at the camera end.
const CONE_R: f32 = 0.2;
/// Wider "soft" cone that starts the pull-in before the hard limit bites.
const SOFT_R: f32 = 0.55;
const SOFT_PAD: f32 = 0.55;
/// How far past the crosshair surface a player / construct target point is pushed (so the
/// converged ray from the eye goes *through* the target, not along its edge).
const TARGET_DEPTH: f32 = 0.3;
/// Crosshair ray reach.
const AIM_RANGE: f32 = 300.0;
/// Near-wall parallax: a blocked path counts when it stops this far short of the aim point.
const BLOCK_SLACK: f32 = 0.35;

// ------------------------------------------------------------------------------------ prefs

/// Persisted camera preferences (inside `UiPrefs::camera`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraPrefs {
    /// Third person by default (and live: F5 writes it). `None` = never chosen: migrates the old
    /// comfort toggle (`UiPrefs::third_person`) on first run.
    pub third: Option<bool>,
    /// Over the left shoulder.
    pub shoulder_left: bool,
    /// ADS: press to toggle instead of hold.
    pub ads_toggle: bool,
    /// ADS look sensitivity (1 = matched to the zoom, so the crosshair sweeps the same screen
    /// distance per count).
    pub ads_sens: f32,
    /// ADS zoom speed (1 = default; the comfort preset makes it instant).
    pub ads_speed: f32,
}

impl Default for CameraPrefs {
    fn default() -> Self {
        Self { third: None, shoulder_left: false, ads_toggle: false, ads_sens: 1.0, ads_speed: 1.0 }
    }
}

impl CameraPrefs {
    pub fn ads_sens(&self) -> f32 {
        if self.ads_sens.is_finite() { self.ads_sens.clamp(0.2, 2.5) } else { 1.0 }
    }
    pub fn ads_speed(&self) -> f32 {
        if self.ads_speed.is_finite() { self.ads_speed.clamp(0.25, 4.0) } else { 1.0 }
    }
}

/// Comfort: camera moves become cuts (the comfort preset turns speed FOV and FOV kicks off).
pub fn comfort_cut(c: &ComfortSettings) -> bool {
    c.is_comfort_preset() || (!c.speed_fov && c.fov_kick <= 0.0)
}

// ------------------------------------------------------------------------------------ resources

/// Which view the local player has this frame.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ViewMode {
    /// Third-person body / camera is showing (the dolly is past [`BODY_SWAP`]).
    pub pref_third: bool,
    /// Seated in a cockpit / vehicle.
    pub piloting: bool,
    /// Boom length for piloting (m; eased toward the construct's size — a transform changes it).
    pub pilot_dist: f32,
    /// PILOTING: over-the-shoulder offset from the seat (m; right of the body) and the height offset
    /// from the seat (m; negative = below it, framing the body rather than the sky).
    pub pilot_side: f32,
    pub pilot_up: f32,
    /// GIANTS: seated in a titan (cockpit walker): a head-height cockpit view, no boom.
    pub cockpit: bool,
    /// A director camera owns the view (soak viewer, brawl bench).
    pub director: bool,
    /// Dolly 0 (eye) … 1 (over the shoulder), eased.
    pub blend: f32,
    /// Host rule: the shoulder camera is allowed.
    pub allowed: bool,
    /// `--third-person` (runtime only; F5 clears it).
    pub cli_third: bool,
}

impl ViewMode {
    pub fn third_person(&self) -> bool {
        (self.pref_third || self.piloting) && !self.director
    }
}

/// The crosshair ray this frame (world space).
#[derive(Resource, Debug, Clone, Copy)]
pub struct AimRay {
    /// Where shots/casts start (the local eye).
    pub origin: Vec3,
    /// Unit direction from `origin` to `point` (converged).
    pub dir: Vec3,
    /// What the crosshair is on (terrain / player / construct, or far out).
    pub point: Vec3,
    /// The camera position the ray was cast from.
    pub camera: Vec3,
    /// Near-wall parallax: where the shot would really stop, if short of `point`.
    pub blocked: Option<Vec3>,
}

impl Default for AimRay {
    fn default() -> Self {
        Self { origin: Vec3::ZERO, dir: Vec3::NEG_Z, point: Vec3::NEG_Z * 200.0, camera: Vec3::ZERO, blocked: None }
    }
}

/// Third person: the local body's hand positions this frame (tether ropes start here).
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ThirdPersonHands {
    pub active: bool,
    pub world: [Vec3; 2],
}

#[derive(Resource)]
pub(crate) struct CamState {
    /// Raw dolly progress 0..1.
    t: f32,
    /// Eased shoulder side (-1 left … +1 right).
    side: f32,
    /// Auto-swapped away from the preferred shoulder (it is jammed against a wall).
    auto_swap: bool,
    block_t: f32,
    clear_t: f32,
    /// Collision-eased boom limit (m; infinite when unobstructed).
    boom: f32,
    /// Smoothed speed stretch.
    speed_ext: f32,
    /// Camera offset from the eye in look space (last placement; per-tick aim uses it).
    offset: Vec3,
    /// A fresh match: snap the dolly.
    snap: bool,
}

impl Default for CamState {
    fn default() -> Self {
        Self {
            t: 0.0,
            side: 1.0,
            auto_swap: false,
            block_t: 0.0,
            clear_t: 0.0,
            boom: f32::INFINITY,
            speed_ext: 0.0,
            offset: Vec3::ZERO,
            snap: true,
        }
    }
}

#[derive(Component)]
struct BlockedDot;
#[derive(Component)]
struct BlockedMark;

pub struct Camera3pPlugin;

impl Plugin for Camera3pPlugin {
    fn build(&self, app: &mut App) {
        super::camera_shots::build(app);
        app.add_plugins(super::ads::AdsPlugin)
            .init_resource::<ViewMode>()
            .init_resource::<AimRay>()
            .init_resource::<CamState>()
            .init_resource::<ThirdPersonHands>()
            .add_systems(Startup, spawn_indicator)
            .add_systems(OnEnter(AppState::InGame), |mut st: ResMut<CamState>| st.snap = true)
            .add_systems(Update, update_mode.in_set(FrameSet::Input))
            .add_systems(Update, override_local_view.in_set(FrameSet::Present).after(BridgeSet::Views).before(super::PresentSet::Events))
            .add_systems(Update, anchor_preview_3p.in_set(FrameSet::Present))
            .add_systems(
                PostUpdate,
                (place_camera, update_indicator)
                    .chain()
                    .run_if(in_state(AppState::InGame))
                    .after(CameraUpdateSet)
                    .before(super::summon_ping::CutInSet)
                    .before(TransformSystems::Propagate)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_systems(SimTick, correct_aim.after(SimSet::Input).before(SimSet::Simulate));
    }
}

// ------------------------------------------------------------------------------------ pure geometry

/// Result of [`solve_boom`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoomSolve {
    /// Shoulder pivot (pulled toward the eye if the shoulder is inside a wall).
    pub pivot: Vec3,
    /// Longest boom (m, along the look's back vector) that clips nothing.
    pub hard: f32,
    /// Where a wider cone would like the camera (≤ `hard` is never required; ease toward it).
    pub soft: f32,
    /// How much of the shoulder offset fits (1 = all; small = jammed against a wall).
    pub shoulder_frac: f32,
}

#[inline]
fn ray_t<V: VoxelQuery + ?Sized>(v: &V, from: Vec3, dir: Vec3, max: f32) -> Option<f32> {
    raycast(v, core_v3(from), core_v3(dir), max).map(|h| h.t)
}

#[inline]
fn solid<V: VoxelQuery + ?Sized>(v: &V, p: Vec3) -> bool {
    v.is_solid(world_to_voxel(core_v3(p)))
}

/// Is the camera's near-plane neighbourhood of `p` clear of solid voxels?
fn clear_at<V: VoxelQuery + ?Sized>(v: &V, p: Vec3) -> bool {
    const E: f32 = 0.08;
    !solid(v, p) && [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z].iter().all(|d| !solid(v, p + *d * E))
}

/// Over-the-shoulder placement for `eye`: `offset` is the world-space shoulder offset, `back` the
/// unit vector behind the view, `boom` the wanted length. The camera is `pivot + back * hard`.
/// Guarantees: the returned camera point (and its near-plane neighbourhood) is never inside a solid
/// voxel as long as the eye is not.
pub fn solve_boom<V: VoxelQuery + ?Sized>(v: &V, eye: Vec3, offset: Vec3, back: Vec3, up: Vec3, boom: f32) -> BoomSolve {
    // 1. Lateral: eye → shoulder pivot.
    let len = offset.length();
    let (mut pivot, mut frac) = (eye + offset, 1.0);
    if len > 1e-3 {
        let d = offset / len;
        if let Some(t) = ray_t(v, eye, d, len + PAD) {
            frac = ((t - PAD) / len).clamp(0.0, 1.0);
            pivot = eye + offset * frac;
        }
    }
    if !clear_at(v, pivot) {
        pivot = eye;
        frac = 0.0;
    }
    // 2. Boom: a cone of rays from the pivot to the corners of the camera's end.
    let right = back.cross(up).normalize_or(Vec3::X);
    let up = right.cross(back).normalize_or(Vec3::Y);
    let cone = |r: f32, pad: f32| -> f32 {
        let mut best = boom;
        if boom <= 1e-3 {
            return 0.0;
        }
        for o in [Vec3::ZERO, right * r, -right * r, up * r, -up * r] {
            let end = pivot + back * boom + o;
            let d = end - pivot;
            let l = d.length();
            if l < 1e-4 {
                continue;
            }
            let dir = d / l;
            if let Some(t) = ray_t(v, pivot, dir, l + pad) {
                best = best.min(t * dir.dot(back) - pad);
            }
        }
        best.clamp(0.0, boom)
    };
    let mut hard = cone(CONE_R, PAD);
    let soft = cone(SOFT_R, SOFT_PAD).min(hard);
    // 3. Never end inside a voxel (thin geometry the rays slipped past, diagonal seams).
    while hard > 0.0 && !clear_at(v, pivot + back * hard) {
        hard = (hard - 0.1).max(0.0);
    }
    BoomSolve { pivot, hard, soft, shoulder_frac: frac }
}

/// Where along the camera ray the target search starts: the character's depth. Anything between
/// the camera and the character (a wall behind you, someone standing at your back) never becomes
/// the aim point.
pub fn ray_start(cam: Vec3, fwd: Vec3, eye: Vec3) -> f32 {
    (eye - cam).dot(fwd).max(0.0)
}

/// Aim convergence: the unit direction from `origin` (the eye / muzzle) to the crosshair point.
/// Falls back to the camera direction when the point is not in front of `origin` (within 60°).
pub fn converge(origin: Vec3, point: Vec3, cam_fwd: Vec3) -> Vec3 {
    let to = point - origin;
    let len = to.length();
    if !len.is_finite() || len < 0.05 || to.dot(cam_fwd) < 0.5 * len { cam_fwd } else { to / len }
}

/// Slab test: entry / exit distances of a ray through an AABB.
pub fn ray_aabb(o: Vec3, d: Vec3, min: Vec3, max: Vec3) -> Option<(f32, f32)> {
    let inv = d.recip();
    let t1 = (min - o) * inv;
    let t2 = (max - o) * inv;
    let tmin = t1.min(t2).max_element();
    let tmax = t1.max(t2).min_element();
    if tmin.is_nan() || tmax.is_nan() || tmax < 0.0 || tmin > tmax { None } else { Some((tmin.max(0.0), tmax)) }
}

/// What the camera ray is on, as a target point. `players` are other players' rendered hulls
/// (min, max). Returns the point and whether it is on a body / construct (vs terrain / sky).
#[allow(clippy::too_many_arguments)]
pub fn crosshair_point<V: VoxelQuery + ?Sized>(
    v: &V,
    constructs: Option<(&bd_sim::constructs::ConstructStore, Option<PlayerId>)>,
    players: &[(Vec3, Vec3)],
    cam: Vec3,
    fwd: Vec3,
    start: f32,
    range: f32,
) -> (Vec3, bool) {
    let from = cam + fwd * start;
    let mut best = ray_t(v, from, fwd, range).unwrap_or(range);
    let mut body = false;
    let mut exit = best;
    for (min, max) in players {
        if let Some((t0, t1)) = ray_aabb(from, fwd, *min, *max)
            && t0 < best
        {
            best = t0;
            exit = t1;
            body = true;
        }
    }
    if let Some((store, skip)) = constructs {
        let end = from + fwd * best;
        if let Some((_, f, _)) = store.segment_query(core_v3(from), core_v3(end), 0.0, skip) {
            let t = f * best;
            if t < best {
                best = t;
                exit = t + TARGET_DEPTH * 2.0;
                body = true;
            }
        }
    }
    let t = if body { (best + TARGET_DEPTH).min((best + exit) * 0.5).max(best) } else { best };
    (from + fwd * t, body)
}

/// Near-wall parallax: does the straight path `from → to` stop on terrain well short of `to`?
/// Returns where it stops.
pub fn path_blocked<V: VoxelQuery + ?Sized>(v: &V, from: Vec3, to: Vec3) -> Option<Vec3> {
    let d = to - from;
    let len = d.length();
    if !len.is_finite() || len <= BLOCK_SLACK + 0.05 {
        return None;
    }
    let dir = d / len;
    ray_t(v, from, dir, len - BLOCK_SLACK).map(|t| from + dir * t)
}

/// Yaw / pitch (input convention) of a unit direction.
pub fn yaw_pitch(d: Vec3) -> (f32, f32) {
    ((-d.x).atan2(-d.z), d.y.clamp(-1.0, 1.0).asin())
}

#[inline]
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Other living players' hulls as the local player sees them (the lag-compensated view).
fn player_hulls(views: &WorldViews, out: &mut Vec<(Vec3, Vec3)>) {
    out.clear();
    for p in views.players.iter().filter(|p| !p.is_local && p.alive() && p.seat.is_none_or(|_| p.has(pflags::SUIT))) {
        let s = p.size.max(0.2);
        let h = if p.has(pflags::CROUCHED) { 1.1 } else { 1.8 } * s;
        let half = Vec3::new(0.4 * s, h * 0.5, 0.4 * s);
        let c = p.pos + Vec3::Y * h * 0.5;
        out.push((c - half, c + half));
    }
}

// ------------------------------------------------------------------------------------ piloting framing

fn id_local(a: &ActiveWorld) -> PlayerId {
    a.local.unwrap_or(PlayerId(u8::MAX))
}

/// Does the segment `a`→`b` pass through the box `[lo, hi]` (slab test)?
fn segment_hits_box(a: Vec3, b: Vec3, lo: Vec3, hi: Vec3) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for k in 0..3 {
        let (o, v, l, h) = (a[k], d[k], lo[k], hi[k]);
        if v.abs() < 1e-6 {
            if o < l || o > h {
                return false;
            }
            continue;
        }
        let (mut ta, mut tb) = ((l - o) / v, (h - o) / v);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        t0 = t0.max(ta);
        t1 = t1.min(tb);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// PILOTING FRAMING (TRANSFORMERS FEEL; owner: the fight must read in third person): the boom
/// `(dist, side, up)` for a standing body with the nearest enemy in front of it kept VISIBLE —
/// when the line from the camera to them would pass through your own robot, the boom slides
/// further over the shoulder (then rises) until it clears (Devastation-style: your robot on the
/// left third, the enemy at the centre-right). Lying bodies (vehicles) keep the chase boom.
fn frame_fight(c: &bd_sim::constructs::Construct, eye: Vec3, look: &LookAngles, base: (f32, f32, f32), views: &WorldViews) -> (f32, f32, f32) {
    let (dist, side, up) = base;
    let h = c.half.y * 2.0;
    if h < c.half.x.max(c.half.z) * 2.0 * 0.9 {
        return base;
    }
    let b = c.bounds();
    let pad = 0.25;
    let (lo, hi) = (v3(b.min) - Vec3::splat(pad), v3(b.max) + Vec3::splat(pad));
    let center = (lo + hi) * 0.5;
    let rot = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0);
    let fwd = rot * Vec3::NEG_Z;
    let flat_fwd = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
    let r = rot * Vec3::X;
    let right = Vec3::new(r.x, 0.0, r.z).normalize_or_zero();
    let back = -fwd;
    // The enemy that matters: the nearest living other player within ~24 m, in front of the view.
    let mut target: Option<(f32, Vec3, f32)> = None;
    for p in views.players.iter().filter(|p| !p.is_local && p.alive()) {
        let s = p.size.max(0.2);
        let mid = p.pos + Vec3::Y * 0.9 * s;
        let d = mid - center;
        let dist_t = Vec3::new(d.x, 0.0, d.z).length();
        if dist_t > 24.0 || Vec3::new(d.x, 0.0, d.z).normalize_or_zero().dot(flat_fwd) < 0.2 {
            continue;
        }
        if target.is_none_or(|t| dist_t < t.0) {
            target = Some((dist_t, mid, 1.8 * s));
        }
    }
    let Some((_, mid, tall)) = target else { return base };
    let cam = |s: f32, u: f32| eye + right * s + Vec3::Y * (u + dist * 0.12) + back * dist;
    let clear = |s: f32, u: f32| {
        let p = cam(s, u);
        // The body and the head of the target, and the point under them (their feet on the ground).
        [mid, mid + Vec3::Y * tall * 0.45, mid - Vec3::Y * tall * 0.45].iter().all(|t| !segment_hits_box(p, *t, lo, hi))
    };
    let step = (0.22 * h).max(0.6);
    for k in 0..10 {
        let s = side + step * k as f32;
        let u = up + if k >= 5 { (k - 4) as f32 * 0.12 * h } else { 0.0 };
        if clear(s, u) {
            return (dist, s, u);
        }
    }
    (dist * 1.1, side + step * 5.0, up + 0.6 * h)
}

// ------------------------------------------------------------------------------------ systems

#[allow(clippy::too_many_arguments)]
fn update_mode(
    time: Res<Time<Real>>,
    args: Res<crate::practice::SimArgs>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Option<Res<KeyBinds>>,
    focus: Res<UiFocus>,
    menu: Res<InGameMenu>,
    state: Res<State<AppState>>,
    comfort: Res<ComfortSettings>,
    prefs: Option<ResMut<UiPrefs>>,
    active: Option<Res<ActiveWorld>>,
    (views, look): (Res<WorldViews>, Res<LookAngles>),
    mut mode: ResMut<ViewMode>,
    mut st: ResMut<CamState>,
    mut rig: ResMut<CameraRig>,
    mut cli_once: Local<bool>,
) {
    if !*cli_once {
        *cli_once = true;
        mode.cli_third = args.third_person;
    }
    let playing = *state.get() == AppState::InGame && !menu.0 && !focus.typing;
    let mut want = mode.cli_third;
    if let Some(mut prefs) = prefs {
        // Migrate the old comfort-only toggle once.
        if prefs.camera.third.is_none() {
            let t = prefs.third_person;
            prefs.camera.third = Some(t);
        }
        if let Some(b) = binds.as_deref()
            && playing
        {
            if b.just_pressed(Action::ToggleView, &keys, &mouse) && mode.allowed {
                let now = prefs.camera.third.unwrap_or(false) || mode.cli_third;
                prefs.camera.third = Some(!now);
                mode.cli_third = false;
            }
            if b.just_pressed(Action::Shoulder, &keys, &mouse) {
                prefs.camera.shoulder_left = !prefs.camera.shoulder_left;
                st.auto_swap = false;
            }
        }
        want |= prefs.camera.third.unwrap_or(false);
        // Mirror the live view for readers of the old field (viewmodel hides the hands on it).
        let live = mode.pref_third && !mode.director;
        if prefs.third_person != live {
            prefs.third_person = live;
        }
    }
    mode.allowed = active.as_ref().is_none_or(|a| a.world.cfg.rules.allow_third_person);
    want &= mode.allowed;

    let seat = active.as_ref().and_then(|a| a.local).and_then(|id| views.player(id)).and_then(|v| v.seat.filter(|_| !v.has(pflags::SUIT)));
    mode.piloting = seat.is_some();
    if let (Some((cid, _)), Some(a)) = (seat, active.as_ref()) {
        // PILOTING camera: a standing body (robot mode, mech, beast) gets an over-the-shoulder boom at
        // ~1.6× its height, framing it at the side of the screen; a lying body (vehicle mode, car,
        // jet) a chase boom at ~1.2× its length. Terrain pulls the boom in (`solve_boom`).
        let (dist, side, up) = match a.world.constructs.get(cid) {
            Some(c) => {
                let h = c.half.y * 2.0;
                let len = c.half.x.max(c.half.z) * 2.0;
                let base = if h >= len * 0.9 { ((1.6 * h).clamp(6.0, 22.0), c.half.x + 1.2, -0.3 * h) } else { ((1.2 * len + 3.0).clamp(6.0, 24.0), 0.0, 0.6) };
                // FRAMING (TRANSFORMERS FEEL): never hide the fight behind your own robot.
                let eye = views.player(id_local(a)).map(|v| v.pos + Vec3::Y * 1.6 * v.size.max(0.2));
                match eye {
                    Some(eye) => frame_fight(c, eye, &look, base, &views),
                    None => base,
                }
            }
            None => (8.0, 0.0, 1.0),
        };
        let dt = time.delta_secs().min(0.1);
        let k = if mode.pilot_dist <= 0.0 { 1.0 } else { 1.0 - (-4.0 * dt).exp() };
        mode.pilot_dist += (dist - mode.pilot_dist) * k;
        mode.pilot_side += (side - mode.pilot_side) * k;
        mode.pilot_up += (up - mode.pilot_up) * k;
        mode.cockpit = a.world.constructs.get(cid).is_some_and(bd_sim::constructs::titan::walks);
    } else {
        mode.cockpit = false;
        mode.pilot_dist = 0.0;
    }

    // The dolly.
    let target = if want { 1.0 } else { 0.0 };
    let dt = time.delta_secs().min(0.1);
    if st.snap || comfort_cut(&comfort) {
        st.t = target;
        st.snap = false;
    } else {
        let step = dt / TRANSITION_SECS;
        st.t = if st.t < target { (st.t + step).min(target) } else { (st.t - step).max(target) };
    }
    mode.blend = smoothstep(st.t);
    mode.pref_third = mode.blend >= BODY_SWAP;

    let want_rig = if mode.director {
        CameraRig::Director
    } else if mode.piloting || mode.blend > 0.0 {
        CameraRig::ThirdPerson
    } else {
        CameraRig::FirstPerson
    };
    if *rig != want_rig {
        *rig = want_rig;
    }
}

/// Place the gameplay camera (third person / the dolly), apply ADS zoom, solve the crosshair.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn place_camera(
    time: Res<Time<Real>>,
    view: (Res<ViewMode>, Res<CameraRig>, Res<ViewAnchor>, Res<LookAngles>, Res<CameraShake>, Res<crate::camera::ViewRoll>),
    comfort: Res<ComfortSettings>,
    ads: Res<super::ads::AdsState>,
    prefs: Option<Res<UiPrefs>>,
    arena: Option<Res<ArenaWorld>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    tuning: Res<MoveTuning>,
    mut st: ResMut<CamState>,
    mut aim: ResMut<AimRay>,
    mut cam: Query<(&mut Transform, &mut Projection), With<PlayerCamera>>,
    mut hulls: Local<Vec<(Vec3, Vec3)>>,
    mut portal_pairs: Local<Vec<super::portal_view::PairGeo>>,
) {
    let (mode, rig, anchor, look, shake, roll) = view;
    let Ok((mut tf, mut proj)) = cam.single_mut() else { return };
    let guard = if active.is_none() { arena.as_ref().map(|a| a.read()) } else { None };
    let vox: &dyn VoxelQuery = match (active.as_deref(), guard.as_deref()) {
        (Some(a), _) => &a.world.voxels,
        (None, Some(g)) => g,
        (None, None) => return,
    };
    let dt = time.delta_secs().min(0.1);
    let rot = look.rotation();
    let fwd = rot * Vec3::NEG_Z;
    let back = -fwd;
    let up = rot * Vec3::Y;
    let local = active.as_ref().and_then(|a| a.local);
    let cut = comfort_cut(&comfort);

    if *rig == CameraRig::ThirdPerson {
        let (offset, boom) = if mode.piloting && mode.cockpit {
            // GIANTS: the titan cockpit — the pilot rides the head; look out over its brow.
            let f = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
            (f * 0.9 - Vec3::Y * 0.4, 0.05)
        } else if mode.piloting {
            let r = rot * Vec3::X;
            let flat_right = Vec3::new(r.x, 0.0, r.z).normalize_or_zero();
            (flat_right * mode.pilot_side + Vec3::Y * (mode.pilot_up + mode.pilot_dist * 0.12), mode.pilot_dist)
        } else {
            // Shoulder side: preferred, or auto-swapped away from a wall.
            let preferred = if prefs.as_ref().is_some_and(|p| p.camera.shoulder_left) { -1.0 } else { 1.0 };
            let lateral = |side: f32| {
                let o = rot * Vec3::new(SHOULDER.x * side, 0.0, 0.0);
                let l = o.length();
                ray_t(vox, anchor.eye, o / l, l + PAD).map_or(1.0, |t| ((t - PAD) / l).clamp(0.0, 1.0))
            };
            if mode.blend > 0.95 {
                let (fp, fa) = (lateral(preferred), lateral(-preferred));
                if !st.auto_swap {
                    st.block_t = if fp < 0.5 && fa > 0.9 { st.block_t + dt } else { 0.0 };
                    if st.block_t > 0.12 {
                        st.auto_swap = true;
                        st.clear_t = 0.0;
                    }
                } else {
                    st.clear_t = if fp > 0.9 || fa < fp { st.clear_t + dt } else { 0.0 };
                    if st.clear_t > 0.5 {
                        st.auto_swap = false;
                        st.block_t = 0.0;
                    }
                }
            }
            let side_t = if st.auto_swap { -preferred } else { preferred };
            st.side = if cut { side_t } else { st.side + (side_t - st.side) * (1.0 - (-10.0 * dt).exp()) };
            // Speed stretch (comfort-gated like speed FOV).
            let speed = anchor.velocity.length();
            let ext = if comfort.speed_fov { ((speed - 12.0) / 25.0).clamp(0.0, 1.0) * SPEED_BOOM } else { 0.0 };
            st.speed_ext += (ext - st.speed_ext) * (1.0 - (-3.0 * dt).exp());
            let a = ads.blend;
            let sh = SHOULDER.lerp(SHOULDER_ADS, a);
            let b = mode.blend;
            (rot * Vec3::new(sh.x * st.side, sh.y, 0.0) * b, (BOOM + (BOOM_ADS - BOOM) * a + st.speed_ext * (1.0 - a)) * b)
        };
        let solve = solve_boom(vox, anchor.eye, offset, back, up, boom);
        let mut hard = solve.hard;
        // World constructs (not attached ones; not the vehicle we sit in).
        if let Some(a) = active.as_ref()
            && hard > 0.0
        {
            let skip = if mode.piloting { local } else { None };
            let end = solve.pivot + back * (hard + PAD);
            if let Some((_, f, _)) = a.world.constructs.segment_query(core_v3(solve.pivot), core_v3(end), CONE_R, skip) {
                hard = hard.min(f * (hard + PAD) - PAD).max(0.0);
            }
        }
        let mut soft = solve.soft.min(hard);
        // PORTALS: a boom that passes through a portal face goes on from the partner's side
        // (`portal_cross::fold_camera` draws the camera there): the wall behind the exit you just
        // came out of never pulls the camera in.
        if let Some(a) = active.as_ref().filter(|a| !a.world.portals.is_empty()) {
            portal_pairs.clear();
            portal_pairs.extend(a.world.portals.iter().map(super::portal_view::PairGeo::from_sim));
            if let Some(l) = super::portal_cross::portal_boom(&portal_pairs, solve.pivot, back, boom, PAD, |o, d, m| ray_t(vox, o, d, m))
                && l > hard
            {
                hard = l;
                soft = l;
            }
        }
        // `st.boom` is the collision limit (∞ = unobstructed, so the dolly / ADS / speed stretch
        // follow exactly). Obstructed: ease toward the soft cone quickly, back out slowly; the hard
        // limit always clamps at once (never clips).
        if soft >= boom - 1e-3 {
            if st.boom.is_finite() {
                st.boom += (boom + 0.5 - st.boom) * (1.0 - (-4.0 * dt).exp());
                if st.boom >= boom {
                    st.boom = f32::INFINITY;
                }
            }
        } else {
            let cur = st.boom.min(boom);
            let rate = if cur > soft { 14.0 } else { 4.0 };
            st.boom = cur + (soft - cur) * (1.0 - (-rate * dt).exp());
        }
        st.boom = st.boom.min(hard);
        if hard >= boom - 1e-3 && soft >= boom - 1e-3 && st.boom >= boom {
            st.boom = f32::INFINITY;
        }
        let pos = solve.pivot + back * boom.min(st.boom).clamp(0.0, hard);
        st.offset = rot.inverse() * (pos - anchor.eye);
        tf.translation = pos + rot * shake.offset;
        // Strafe / swing lean (camera.rs) about the view axis: aim and the boom are untouched.
        tf.rotation = rot * roll.quat() * shake.rotation;
    } else {
        st.offset = Vec3::ZERO;
        st.boom = f32::INFINITY;
    }

    // ADS zoom (both views) on top of the base + speed FOV the camera module applied.
    if ads.blend > 0.0
        && let Projection::Perspective(p) = proj.as_mut()
    {
        p.fov = 2.0 * ((p.fov * 0.5).tan() / ads.view_zoom.max(1.0)).atan();
    }

    // Crosshair: from the camera (without shake), past the character.
    let cam_pos = anchor.eye + rot * st.offset;
    let origin = anchor.eye;
    let third = st.offset.length_squared() > 1e-4;
    let (point, dir, blocked) = if third {
        player_hulls(&views, &mut hulls);
        let cs = active.as_ref().map(|a| (&a.world.constructs, local));
        let (point, _) = crosshair_point(vox, cs, &hulls, cam_pos, fwd, ray_start(cam_pos, fwd, origin), AIM_RANGE);
        let dir = converge(origin, point, fwd);
        // Hand path (projectiles / hooks leave the hands) and eye path (hitscan).
        let p = active.as_ref().map_or(&tuning.0, |a| &a.world.cfg.move_params);
        let ho = v3(p.hand_offset);
        let (sy, cy) = look.yaw.sin_cos();
        let right = Vec3::new(cy, 0.0, -sy);
        let hand = origin + right * ho.x + Vec3::Y * ho.y + dir * ho.z;
        let blocked = path_blocked(vox, origin, point).or_else(|| path_blocked(vox, hand, point));
        (point, dir, blocked)
    } else {
        let point = ray_t(vox, origin, fwd, AIM_RANGE).map_or(origin + fwd * 200.0, |t| origin + fwd * t);
        (point, fwd, None)
    };
    *aim = AimRay { origin, dir, point, camera: cam_pos, blocked };
}

/// Third person: aim the tick's input from the eye at the crosshair point — only on ticks where
/// aim matters, so movement (which reads yaw) is identical to first person otherwise.
#[allow(clippy::too_many_arguments)]
fn correct_aim(
    mode: Res<ViewMode>,
    st: Res<CamState>,
    active: Option<Res<ActiveWorld>>,
    arena: Option<Res<ArenaWorld>>,
    views: Res<WorldViews>,
    tuning: Res<MoveTuning>,
    sim: Query<&PlayerSim, With<LocalPlayer>>,
    mut input: ResMut<LocalInputState>,
    mut hulls: Local<Vec<(Vec3, Vec3)>>,
) {
    if st.offset.length_squared() < 1e-4 || mode.director {
        return;
    }
    let cur = input.current;
    let prev = input.history.iter().rev().nth(1).map_or(0, |p| p.buttons);
    if !aim_matters(cur.buttons, prev) {
        return;
    }
    let Ok(sim) = sim.single() else { return };
    let params = active.as_ref().map_or(&tuning.0, |a| &a.world.cfg.move_params);
    let eye = v3(sim.state.eye(params));
    let rot = Quat::from_euler(EulerRot::YXZ, cur.yaw, cur.pitch, 0.0);
    let fwd = rot * Vec3::NEG_Z;
    let cam = eye + rot * st.offset;
    let guard = if active.is_none() { arena.as_ref().map(|a| a.read()) } else { None };
    let vox: &dyn VoxelQuery = match (active.as_deref(), guard.as_deref()) {
        (Some(a), _) => &a.world.voxels,
        (None, Some(g)) => g,
        (None, None) => return,
    };
    player_hulls(&views, &mut hulls);
    let cs = active.as_ref().map(|a| (&a.world.constructs, a.local));
    let (point, _) = crosshair_point(vox, cs, &hulls, cam, fwd, ray_start(cam, fwd, eye), AIM_RANGE);
    let (yaw, pitch) = yaw_pitch(converge(eye, point, fwd));
    let pitch = pitch.clamp(-crate::input::PITCH_LIMIT_DEG.to_radians(), crate::input::PITCH_LIMIT_DEG.to_radians());
    input.current.yaw = yaw.rem_euclid(std::f32::consts::TAU);
    input.current.pitch = pitch;
    let c = input.current;
    if let Some(last) = input.history.back_mut() {
        last.yaw = c.yaw;
        last.pitch = c.pitch;
    }
}

/// Third person: the tether ring on the crosshair answers for the *converged* ray (the one a tether
/// press will fire along), not the eye's straight-ahead ray.
fn anchor_preview_3p(
    st: Res<CamState>,
    aim: Res<AimRay>,
    tuning: Res<MoveTuning>,
    active: Option<Res<ActiveWorld>>,
    arena: Option<Res<ArenaWorld>>,
    mut preview: ResMut<crate::player::AnchorPreview>,
) {
    if st.offset.length_squared() < 1e-4 {
        return;
    }
    let params = active.as_ref().map_or(&tuning.0, |a| &a.world.cfg.move_params);
    let q = match (active.as_deref(), arena.as_deref()) {
        (Some(a), _) => {
            let cw = bd_sim::movement::CollisionWorld::new(&a.world.voxels, &[]);
            bd_sim::movement::query_anchor(&cw, core_v3(aim.origin), core_v3(aim.dir), params)
        }
        (None, Some(ar)) => {
            let w = ar.read();
            let cw = bd_sim::movement::CollisionWorld::new(&*w, &[]);
            bd_sim::movement::query_anchor(&cw, core_v3(aim.origin), core_v3(aim.dir), params)
        }
        _ => None,
    };
    *preview = match q {
        Some(q) => crate::player::AnchorPreview { valid: true, assisted: q.assisted, point: v3(q.point), dist: q.dist },
        None => crate::player::AnchorPreview::default(),
    };
}

/// Ticks whose look direction aims something: firing / alt / ADS held, or a tether press.
pub fn aim_matters(buttons_now: u16, buttons_prev: u16) -> bool {
    let held = buttons::PRIMARY | buttons::SECONDARY | buttons::ADS;
    let pressed = buttons_now & !buttons_prev;
    buttons_now & held != 0 || pressed & (buttons::TETHER_L | buttons::TETHER_R) != 0
}

/// The local player's `WorldViews` entry = the predicted state this frame (client mode would draw
/// the delayed host position), facing the converged aim; hands for the rope shell.
#[allow(clippy::too_many_arguments)]
fn override_local_view(
    clock: Res<TickClock>,
    mode: Res<ViewMode>,
    look: Res<LookAngles>,
    aim: Res<AimRay>,
    active: Option<Res<ActiveWorld>>,
    prefs: Option<Res<UiPrefs>>,
    sim: Query<&PlayerSim, With<LocalPlayer>>,
    mut views: ResMut<WorldViews>,
    mut hands: ResMut<ThirdPersonHands>,
) {
    hands.active = false;
    let Ok(sim) = sim.single() else { return };
    let Some(a) = active else {
        // Tutorial (no sim world, so no views): the local body is a synthetic view in third person.
        if mode.third_person() {
            let v = tutorial_view(sim, clock.alpha, yaw_pitch(aim.dir), prefs.as_ref().map_or(bd_core::corps::Corps::Willpower, |p| p.corps));
            let (eye, rot, _) = super::constructs::body_frames(&v);
            hands.world = [0, 1].map(|k| eye + rot * Vec3::new(if k == 0 { -0.35 } else { 0.35 }, -0.3, -0.2));
            hands.active = true;
            views.players.push(v);
        }
        return;
    };
    let Some(id) = a.local else { return };
    let Ok(i) = views.players.binary_search_by_key(&id, |p| p.id) else { return };
    let v = &mut views.players[i];
    if !v.alive() || v.seat.is_some() {
        return;
    }
    let alpha = clock.alpha;
    let pos = sim.render_pos(alpha);
    // A respawn / teleport the snapshot already knows but prediction hasn't caught: keep the view.
    if pos.distance_squared(v.pos) > 30.0 * 30.0 {
        return;
    }
    v.pos = pos;
    v.vel = v3(sim.state.vel);
    let (yaw, pitch) = if mode.third_person() { yaw_pitch(aim.dir) } else { (look.yaw, look.pitch) };
    v.yaw = yaw;
    v.pitch = pitch;
    let s = &sim.state;
    let mut f = v.flags & !(pflags::GROUNDED | pflags::CROUCHED | pflags::SLIDING | pflags::HOOK_L_ATTACHED | pflags::HOOK_R_ATTACHED);
    for (on, bit) in [
        (s.grounded, pflags::GROUNDED),
        (s.crouched, pflags::CROUCHED),
        (s.sliding, pflags::SLIDING),
        (s.hooks[0].attached(), pflags::HOOK_L_ATTACHED),
        (s.hooks[1].attached(), pflags::HOOK_R_ATTACHED),
    ] {
        if on {
            f |= bit;
        }
    }
    v.flags = f;
    for h in 0..2 {
        let hk = s.hooks[h];
        v.hooks[h] = match hk.phase {
            HookPhase::Idle => None,
            HookPhase::Flying if sim.prev.hooks[h].phase == HookPhase::Flying => Some(v3(sim.prev.hooks[h].pos).lerp(v3(hk.pos), alpha)),
            _ => Some(v3(hk.pos)),
        };
    }
    if mode.third_person() {
        let (eye, rot, _) = super::constructs::body_frames(v);
        let sz = v.size;
        hands.world = [0, 1].map(|k| eye + rot * Vec3::new(if k == 0 { -0.35 } else { 0.35 } * sz, -0.3 * sz, -0.2));
        hands.active = true;
    }
}

/// Id of the tutorial's synthetic local body (no sim world there; nothing else uses ids then).
pub const TUTORIAL_LOCAL: PlayerId = PlayerId(0);

/// The tutorial's local player as a view (third person draws the body from `WorldViews`).
pub fn tutorial_view(sim: &PlayerSim, alpha: f32, (yaw, pitch): (f32, f32), corps: bd_core::corps::Corps) -> crate::sim_bridge::PlayerView {
    let s = &sim.state;
    let mut flags = pflags::ALIVE;
    for (on, bit) in [
        (s.grounded, pflags::GROUNDED),
        (s.crouched, pflags::CROUCHED),
        (s.sliding, pflags::SLIDING),
        (s.tumble > 0.0, pflags::TUMBLING),
        (s.hooks[0].attached(), pflags::HOOK_L_ATTACHED),
        (s.hooks[1].attached(), pflags::HOOK_R_ATTACHED),
    ] {
        if on {
            flags |= bit;
        }
    }
    let hooks = [0, 1].map(|h| match s.hooks[h].phase {
        HookPhase::Idle => None,
        HookPhase::Flying if sim.prev.hooks[h].phase == HookPhase::Flying => Some(v3(sim.prev.hooks[h].pos).lerp(v3(s.hooks[h].pos), alpha)),
        _ => Some(v3(s.hooks[h].pos)),
    });
    crate::sim_bridge::PlayerView {
        id: TUTORIAL_LOCAL,
        corps,
        is_local: true,
        pos: sim.render_pos(alpha),
        vel: v3(s.vel),
        yaw,
        pitch,
        flags,
        damage_pct: 0.0,
        launch: 0.0,
        stocks: 255,
        willpower: 100.0,
        status: 0,
        size: 1.0,
        hooks,
        seat: None,
        power: Default::default(),
        tell: 0,
    }
}

// ------------------------------------------------------------------------------------ crosshair

/// Amber crosshair overlay + a marker where a blocked shot really lands.
const BLOCK_COLOR: Color = Color::srgb(1.0, 0.55, 0.12);

fn spawn_indicator(mut commands: Commands) {
    commands.spawn((
        Name::new("CrosshairBlocked"),
        BlockedDot,
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(8),
            height: px(8),
            margin: UiRect { left: px(-4), top: px(-4), ..default() },
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        BackgroundColor(BLOCK_COLOR),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(41),
    ));
    commands.spawn((
        Name::new("CrosshairBlockedMark"),
        BlockedMark,
        Node {
            position_type: PositionType::Absolute,
            width: px(10),
            height: px(10),
            margin: UiRect { left: px(-5), top: px(-5), ..default() },
            border: UiRect::all(px(2)),
            ..default()
        },
        BorderColor::all(BLOCK_COLOR),
        BackgroundColor(Color::NONE),
        UiTransform { rotation: Rot2::degrees(45.0), ..default() },
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(41),
    ));
}

#[allow(clippy::type_complexity)]
fn update_indicator(
    aim: Res<AimRay>,
    menu: Res<InGameMenu>,
    rig: Res<CameraRig>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Query<(&Transform, &Projection), With<PlayerCamera>>,
    mut dot: Query<&mut Visibility, (With<BlockedDot>, Without<BlockedMark>)>,
    mut mark: Query<(&mut Node, &mut Visibility), (With<BlockedMark>, Without<BlockedDot>)>,
) {
    let on = aim.blocked.is_some() && !menu.0 && *rig == CameraRig::ThirdPerson;
    let vis = |b: bool| if b { Visibility::Visible } else { Visibility::Hidden };
    for mut v in &mut dot {
        if *v != vis(on) {
            *v = vis(on);
        }
    }
    // Project the stop point (this frame's camera).
    let screen = match (aim.blocked, cam.single()) {
        (Some(p), Ok((tf, Projection::Perspective(pp)))) if on => {
            let local = tf.rotation.inverse() * (p - tf.translation);
            if local.z < -0.05 {
                let (w, h) = (window.width(), window.height());
                let ty = (pp.fov * 0.5).tan();
                let tx = ty * w / h.max(1.0);
                let nx = local.x / (-local.z * tx);
                let ny = local.y / (-local.z * ty);
                (nx.abs() < 1.2 && ny.abs() < 1.2).then(|| Vec2::new((nx * 0.5 + 0.5) * w, (0.5 - ny * 0.5) * h))
            } else {
                None
            }
        }
        _ => None,
    };
    for (mut node, mut v) in &mut mark {
        let want = vis(screen.is_some());
        if *v != want {
            *v = want;
        }
        if let Some(s) = screen {
            node.left = px(s.x);
            node.top = px(s.y);
        }
    }
}

// ------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::voxel::Material;
    use std::collections::HashSet;

    /// A sparse voxel grid for tests.
    #[derive(Default)]
    struct Grid(HashSet<bd_core::IVec3>);

    impl VoxelQuery for Grid {
        fn material(&self, v: bd_core::IVec3) -> Material {
            if self.0.contains(&v) { 1 } else { 0 }
        }
    }

    impl Grid {
        fn fill_box(&mut self, min: Vec3, max: Vec3) {
            let a = world_to_voxel(core_v3(min));
            let b = world_to_voxel(core_v3(max));
            for x in a.x..=b.x {
                for y in a.y..=b.y {
                    for z in a.z..=b.z {
                        self.0.insert(bd_core::IVec3::new(x, y, z));
                    }
                }
            }
        }
    }

    fn frame(yaw: f32, pitch: f32) -> (Quat, Vec3, Vec3, Vec3) {
        let rot = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
        (rot, rot * Vec3::NEG_Z, rot * Vec3::Z, rot * Vec3::Y)
    }

    #[test]
    fn open_space_boom_is_full_length() {
        let g = Grid::default();
        let eye = Vec3::new(0.3, 10.2, 0.1);
        let (rot, _, back, up) = frame(0.3, -0.2);
        let s = solve_boom(&g, eye, rot * SHOULDER, back, up, BOOM);
        assert!((s.hard - BOOM).abs() < 1e-4 && (s.shoulder_frac - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wall_behind_pulls_the_camera_in_front_of_it() {
        let mut g = Grid::default();
        // A wall 1.5 m behind the eye (the view looks down -Z, so behind is +Z).
        g.fill_box(Vec3::new(-10.0, 0.0, 1.5), Vec3::new(10.0, 20.0, 2.5));
        let eye = Vec3::new(0.2, 10.1, 0.2);
        let (rot, _, back, up) = frame(0.0, 0.0);
        let s = solve_boom(&g, eye, rot * SHOULDER, back, up, BOOM);
        assert!(s.hard < 1.3 && s.hard > 0.5, "hard {}", s.hard);
        assert!(clear_at(&g, s.pivot + back * s.hard));
        assert!(s.soft <= s.hard);
    }

    #[test]
    fn jammed_shoulder_reports_and_stays_out_of_the_wall() {
        let mut g = Grid::default();
        // Wall hugging the right shoulder.
        g.fill_box(Vec3::new(0.5, 0.0, -10.0), Vec3::new(2.0, 20.0, 10.0));
        let eye = Vec3::new(0.2, 10.1, 0.25);
        let (rot, _, back, up) = frame(0.0, 0.0);
        let s = solve_boom(&g, eye, rot * SHOULDER, back, up, BOOM);
        assert!(s.shoulder_frac < 0.5, "frac {}", s.shoulder_frac);
        assert!(clear_at(&g, s.pivot + back * s.hard));
    }

    /// Fuzz: random voxel clutter, random eyes in air, random looks — the camera never ends inside
    /// (or within the near-plane neighbourhood of) a solid voxel.
    #[test]
    fn camera_never_ends_inside_a_solid_voxel() {
        let mut rng = crate::util::Rng::new(0xB0_0B);
        let mut total = 0;
        for world in 0..40 {
            let mut g = Grid::default();
            for _ in 0..(14 + world) {
                let c = Vec3::new(rng.range(-8.0, 8.0), rng.range(-3.0, 8.0), rng.range(-8.0, 8.0));
                let e = Vec3::new(rng.range(0.25, 1.5), rng.range(0.25, 1.5), rng.range(0.25, 1.5));
                g.fill_box(c - e, c + e);
            }
            // Thin sheets and diagonal seams too.
            for k in -12..12 {
                g.0.insert(bd_core::IVec3::new(k, k.rem_euclid(5), -k));
            }
            let mut tried = 0;
            for _ in 0..4000 {
                if tried >= 60 {
                    break;
                }
                let eye = Vec3::new(rng.range(-7.0, 7.0), rng.range(-3.0, 8.0), rng.range(-7.0, 7.0));
                if !clear_at(&g, eye) {
                    continue;
                }
                tried += 1;
                total += 1;
                let (rot, _, back, up) = frame(rng.range(0.0, std::f32::consts::TAU), rng.range(-1.5, 1.5));
                let side = if rng.f32() < 0.5 { -1.0 } else { 1.0 };
                let boom = rng.range(0.0, 4.5);
                let off = rot * Vec3::new(SHOULDER.x * side, SHOULDER.y, 0.0) * rng.range(0.0, 1.0);
                let s = solve_boom(&g, eye, off, back, up, boom);
                let cam = s.pivot + back * s.hard;
                assert!(clear_at(&g, cam), "world {world}: camera {cam:?} inside a voxel (eye {eye:?}, boom {boom}, hard {})", s.hard);
                assert!(s.hard <= boom + 1e-5 && s.hard >= 0.0);
                // Every eased boom ≤ hard is clear too (the camera is clamped to [0, hard]).
                for f in [0.25f32, 0.5, 0.75] {
                    let p = s.pivot + back * s.hard * f;
                    assert!(!solid(&g, p), "world {world}: boom point {p:?} solid");
                }
            }
        }
        assert!(total > 2000, "only {total} camera placements tested");
    }

    /// The crosshair is the truth: the converged ray from the eye passes through the camera's
    /// target point, for targets near and far, from either shoulder.
    #[test]
    fn convergence_hits_the_crosshair_point() {
        let g = Grid::default();
        let eye = Vec3::new(0.0, 1.6, 0.0);
        for side in [-1.0f32, 1.0] {
            for yaw in [0.0f32, 0.7, 2.5] {
                for pitch in [-0.6f32, 0.0, 0.4] {
                    let (rot, fwd, _, _) = frame(yaw, pitch);
                    let cam = eye + rot * Vec3::new(SHOULDER.x * side, SHOULDER.y, BOOM);
                    for dist in [2.0f32, 6.0, 25.0, 120.0] {
                        // A target box on the camera ray at `dist` from the camera.
                        let c = cam + fwd * (dist + BOOM);
                        let hull = [(c - Vec3::new(0.4, 0.9, 0.4), c + Vec3::new(0.4, 0.9, 0.4))];
                        let (p, body) = crosshair_point(&g, None, &hull, cam, fwd, ray_start(cam, fwd, eye), AIM_RANGE);
                        assert!(body, "target found");
                        let d = converge(eye, p, fwd);
                        // The eye ray enters the target box.
                        let hit = ray_aabb(eye, d, hull[0].0, hull[0].1);
                        assert!(hit.is_some(), "side {side} yaw {yaw} pitch {pitch} dist {dist}: converged ray misses");
                        // And ends at the same point the crosshair shows.
                        let t = (p - eye).dot(d);
                        assert!((eye + d * t).distance(p) < 1e-3);
                    }
                }
            }
        }
    }

    #[test]
    fn things_behind_the_character_never_become_the_aim_point() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let (rot, fwd, _, _) = frame(0.0, 0.0);
        let cam = eye + rot * Vec3::new(0.55, 0.18, 3.2);
        // A player between the camera and the eye.
        let c = cam + fwd * 1.5;
        let hull = [(c - Vec3::splat(0.4), c + Vec3::splat(0.4))];
        let (p, body) = crosshair_point(&Grid::default(), None, &hull, cam, fwd, ray_start(cam, fwd, eye), AIM_RANGE);
        assert!(!body);
        assert!(p.distance(eye) > 100.0);
        assert!(converge(eye, p, fwd).dot(fwd) > 0.99);
    }

    /// Near-wall parallax: the camera sees past a corner the character is hugging; the eye → point
    /// path is blocked close to the character and is reported, not silently eaten.
    #[test]
    fn near_wall_parallax_is_detected() {
        let mut g = Grid::default();
        // A wall to the eye's front-left edge: the shoulder camera (right) looks past it.
        g.fill_box(Vec3::new(-3.0, 0.0, -1.5), Vec3::new(0.2, 4.0, -1.0));
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let (rot, fwd, _, _) = frame(0.0, 0.0);
        let cam = eye + rot * Vec3::new(0.9, 0.18, 3.2);
        let (p, _) = crosshair_point(&g, None, &[], cam, fwd, ray_start(cam, fwd, eye), AIM_RANGE);
        // A point far down the corridor to the right of the wall.
        let p = if p.distance(cam) < 20.0 { cam + fwd * 40.0 - Vec3::X * 0.6 } else { p - Vec3::X * 0.6 };
        let blocked = path_blocked(&g, eye, p);
        assert!(blocked.is_some_and(|b| b.distance(eye) < 3.0), "{blocked:?}");
        // With a clear path nothing is reported.
        assert!(path_blocked(&Grid::default(), eye, p).is_none());
        // A path ending exactly on the aimed surface is not "blocked".
        let mut wall = Grid::default();
        wall.fill_box(Vec3::new(-5.0, 0.0, -10.5), Vec3::new(5.0, 5.0, -10.0));
        let (q, _) = crosshair_point(&wall, None, &[], cam, fwd, ray_start(cam, fwd, eye), AIM_RANGE);
        assert!(path_blocked(&wall, eye, q).is_none());
    }

    /// Perf budget: the whole per-frame third-person solve (2 lateral probes, the boom cone, the
    /// crosshair ray, both parallax probes) on the real Miami map. `cargo test -- --ignored
    /// camera_cost` prints µs per frame.
    #[test]
    #[ignore]
    fn camera_cost_on_miami() {
        let (w, info) = bd_voxel::maps::miami::generate_miami(1);
        let mut rng = crate::util::Rng::new(7);
        let n = 4000;
        let t0 = std::time::Instant::now();
        let mut sink = 0.0f32;
        for k in 0..n {
            let s = info.spawns[k % info.spawns.len()];
            let eye = v3(s) + Vec3::new(rng.range(-6.0, 6.0), 1.6, rng.range(-6.0, 6.0));
            let (rot, fwd, back, up) = frame(rng.range(0.0, std::f32::consts::TAU), rng.range(-0.8, 0.8));
            for side in [1.0f32, -1.0] {
                let o = rot * Vec3::new(SHOULDER.x * side, 0.0, 0.0);
                sink += ray_t(&w, eye, o.normalize(), SHOULDER.x + PAD).unwrap_or(0.0);
            }
            let b = solve_boom(&w, eye, rot * SHOULDER, back, up, BOOM + SPEED_BOOM);
            let cam = b.pivot + back * b.hard;
            let (p, _) = crosshair_point(&w, None, &[], cam, fwd, ray_start(cam, fwd, eye), AIM_RANGE);
            sink += path_blocked(&w, eye, p).map_or(0.0, |q| q.x) + path_blocked(&w, eye + Vec3::X * 0.3, p).map_or(0.0, |q| q.y);
        }
        let us = t0.elapsed().as_secs_f64() * 1e6 / n as f64;
        println!("CAMERA_COST {us:.1} us per frame (sink {sink})");
        assert!(us < 500.0, "{us} µs");
    }

    #[test]
    fn aim_only_corrects_ticks_that_aim() {
        assert!(!aim_matters(buttons::JUMP | buttons::BOOST, 0));
        assert!(aim_matters(buttons::PRIMARY, 0));
        assert!(aim_matters(buttons::ADS, buttons::ADS));
        assert!(aim_matters(buttons::TETHER_L, 0));
        assert!(!aim_matters(buttons::TETHER_L, buttons::TETHER_L), "holding a tether swings (movement)");
    }

    #[test]
    fn yaw_pitch_roundtrip() {
        for (y, p) in [(0.0f32, 0.0f32), (1.0, 0.3), (-2.0, -0.7)] {
            let d = crate::util::v3(bd_core::input::look_dir(y, p));
            let (y2, p2) = yaw_pitch(d);
            let d2 = crate::util::v3(bd_core::input::look_dir(y2, p2));
            assert!(d.distance(d2) < 1e-5);
        }
    }
}
