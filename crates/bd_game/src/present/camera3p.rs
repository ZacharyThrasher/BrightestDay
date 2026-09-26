//! Third-person over-the-shoulder camera rig (DESIGN.md "Motion comfort"; COMBAT.md piloting).
//!
//! * Used automatically while seated in a cockpit / vehicle (farther back, scaled by the vehicle),
//!   and as a comfort option: `lobby::prefs::UiPrefs::third_person` (UI exposes it) or
//!   `--third-person`.
//! * Collision-aware: the boom is ray-cast through the voxels from the shoulder pivot; the camera
//!   snaps in instantly when something is in the way and eases back out (no clipping, no pumping).
//! * Never rolls; shake still applies. The body (costume) is drawn by `players` in this mode.
//! * [`AimRay`]: the crosshair ray from the *camera* (both modes). In third person the tick's
//!   `PlayerInput` yaw/pitch are corrected so the eye aims at the crosshair point — projectiles,
//!   ring blasts and casts (UI should cast along `AimRay`) go where the crosshair is.

use crate::arena::ArenaWorld;
use crate::camera::{CameraRig, CameraUpdateSet, PlayerCamera, ViewAnchor};
use crate::input::{LocalInputState, LookAngles};
use crate::juice::CameraShake;
use crate::sim_bridge::{ActiveWorld, WorldViews};
use crate::tick::{SimSet, SimTick};
use crate::util::{core_v3, v3};
use bd_core::voxel::raycast;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// Which view the local player has this frame.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ViewMode {
    /// Comfort preference (UI) or `--third-person`.
    pub pref_third: bool,
    /// Seated in a cockpit / vehicle.
    pub piloting: bool,
    /// Boom length for piloting (m).
    pub pilot_dist: f32,
    /// A director camera owns the view (soak viewer, brawl bench).
    pub director: bool,
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
    /// Unit direction from `origin` to `point`.
    pub dir: Vec3,
    /// What the crosshair is on (terrain hit, or 200 m out).
    pub point: Vec3,
    /// The camera position the ray was cast from.
    pub camera: Vec3,
}

impl Default for AimRay {
    fn default() -> Self {
        Self { origin: Vec3::ZERO, dir: Vec3::NEG_Z, point: Vec3::NEG_Z * 200.0, camera: Vec3::ZERO }
    }
}

#[derive(Resource, Default)]
struct Boom {
    dist: f32,
}

const SHOULDER: Vec3 = Vec3::new(0.55, 0.18, 0.0);
const BOOM: f32 = 3.2;

pub struct Camera3pPlugin;

impl Plugin for Camera3pPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewMode>()
            .init_resource::<AimRay>()
            .init_resource::<Boom>()
            .add_systems(Update, update_mode.in_set(super::PresentSet::Sync))
            .add_systems(PostUpdate, place_camera.after(CameraUpdateSet).before(TransformSystems::Propagate))
            .add_systems(SimTick, correct_aim.after(SimSet::Input).before(SimSet::Simulate));
    }
}

fn update_mode(
    args: Res<crate::practice::SimArgs>,
    prefs: Option<Res<crate::lobby::prefs::UiPrefs>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut mode: ResMut<ViewMode>,
    mut rig: ResMut<CameraRig>,
) {
    mode.pref_third = args.third_person || prefs.is_some_and(|p| p.third_person);
    let seat = active.as_ref().and_then(|a| a.local).and_then(|id| views.player(id)).and_then(|v| v.seat.filter(|_| !v.has(bd_sim::replicate::pflags::SUIT)));
    mode.piloting = seat.is_some();
    if let (Some((cid, _)), Some(a)) = (seat, active.as_ref()) {
        let size = a.world.constructs.get(cid).map_or(3.0, |c| c.half.max_element() * 2.0);
        mode.pilot_dist = (size * 1.6 + 3.0).clamp(5.0, 30.0);
    }
    let want = if mode.director {
        CameraRig::Director
    } else if mode.third_person() {
        CameraRig::ThirdPerson
    } else {
        CameraRig::FirstPerson
    };
    if *rig != want {
        *rig = want;
    }
}

#[allow(clippy::too_many_arguments)]
fn place_camera(
    time: Res<Time<Real>>,
    mode: Res<ViewMode>,
    rig: Res<CameraRig>,
    anchor: Res<ViewAnchor>,
    look: Res<LookAngles>,
    shake: Res<CameraShake>,
    arena: Option<Res<ArenaWorld>>,
    mut boom: ResMut<Boom>,
    mut aim: ResMut<AimRay>,
    mut cam: Query<&mut Transform, With<PlayerCamera>>,
) {
    let Ok(mut tf) = cam.single_mut() else { return };
    let rot = look.rotation();
    let fwd = rot * Vec3::NEG_Z;
    if *rig == CameraRig::ThirdPerson {
        let want = if mode.piloting { mode.pilot_dist } else { BOOM };
        let pivot = anchor.eye + rot * SHOULDER * if mode.piloting { 0.0 } else { 1.0 } + Vec3::Y * if mode.piloting { want * 0.25 } else { 0.0 };
        let back = -fwd;
        // Collision: pull in to the first solid voxel along the boom (instantly), ease back out.
        let mut max = want;
        if let Some(a) = arena.as_ref()
            && let Some(hit) = raycast(&*a.read(), core_v3(pivot), core_v3(back), want + 0.4)
        {
            max = (v3(hit.point).distance(pivot) - 0.35).max(0.25);
        }
        let dt = time.delta_secs().min(0.1);
        boom.dist = if max < boom.dist { max } else { boom.dist + (max - boom.dist) * (1.0 - (-4.0 * dt).exp()) };
        tf.translation = pivot + back * boom.dist + rot * shake.offset;
        tf.rotation = rot * shake.rotation;
    } else {
        boom.dist = if mode.piloting { mode.pilot_dist } else { BOOM };
    }
    // Crosshair ray from the camera.
    let cam_pos = tf.translation;
    let dir = tf.rotation * Vec3::NEG_Z;
    let point = arena
        .as_ref()
        .and_then(|a| raycast(&*a.read(), core_v3(cam_pos), core_v3(dir), 300.0))
        .map(|h| v3(h.point))
        .unwrap_or(cam_pos + dir * 200.0);
    let origin = anchor.eye;
    // Never aim backwards at something between the camera and the eye.
    let to = point - origin;
    let d = if to.dot(fwd) > 0.5 { to.normalize() } else { fwd };
    *aim = AimRay { origin, dir: d, point, camera: cam_pos };
}

/// Third person: aim the tick's input from the eye at the crosshair point.
fn correct_aim(mode: Res<ViewMode>, aim: Res<AimRay>, mut input: ResMut<LocalInputState>) {
    if !mode.third_person() {
        return;
    }
    let d = aim.dir;
    let yaw = (-d.x).atan2(-d.z);
    let pitch = d.y.clamp(-1.0, 1.0).asin();
    input.current.yaw = yaw;
    input.current.pitch = pitch;
    if let Some(last) = input.history.back_mut() {
        last.yaw = yaw;
        last.pitch = pitch;
    }
}
