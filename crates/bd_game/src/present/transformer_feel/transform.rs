//! THE TRANSFORM (COMBAT.md §5d-bis): what you hear and feel while the panels move.
//!
//! * **Sound**: a size-classed servo bed (`tf_seq_{robot,alt}_{s,m,l}`: small suits zippy + high,
//!   6 m weighty, 10 m+ deep with a sub) carries the pitch contour; the clicks / clunks are NOT baked
//!   in — each panel's lock plays one (`transform_panel_click` pitched by size, `tf_panel_clunk` for
//!   big bodies) exactly when bd_fx shows it lock, so the cascade is the picture. The stand-up plays
//!   the size-classed final lock (`tf_lock_{s,m,l}`); the vehicle settling plays a lower one. Combat
//!   transforms (`HeldFx::Transform` |strength| > 1.2) are a "snap" (`tf_snap_{robot,alt}`) with only
//!   a few clicks on top. Mid-air: `tf_air_whoosh` + spiral streaks.
//! * **Camera** (only the local pilot / suit wearer): a short FOV punch, a small shake and a roll
//!   twitch on the snap and the final lock — FOV punch / shake ride the comfort sliders (`juice`),
//!   the roll twitch needs the comfort camera lean (`ComfortSettings::roll_on`) and scales by shake.
//!   Big bodies shake the ground for everyone near the stand-up (by size and distance).
//! * **Sparks**: size-scaled spark showers that fall and bounce at every panel lock (on top of
//!   bd_fx's lock sparks), a dust ring when a big robot stands up.
//! * **Autonomous transforms**: a summoned / follower transformer changes mode on the host without a
//!   `HeldFx::Transform` (only `GameEvent::Transformed`); the client now plays the full sequence for
//!   it too (and re-syncs a visual that disagrees with the replicated mode for > 1.2 s).

use super::super::constructs::ConstructEntities;
use super::super::sound::Sfx3d;
use super::super::bv;
use super::{Jit, TfFeel, size_class};
use crate::camera::PlayerCamera;
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents};
use bd_fx::{BurstFx, BurstKind, Emit, FxParticles, PFlags, PKind, TransformFx};
use bd_shapes::TransformEventKind;
use bd_sim::GameEvent;
use bd_sim::constructs::components::XMode;
use bd_sim::events::HeldFx;
use bevy::prelude::*;

/// A fast combat transform (`HeldFx::Transform` |strength| above this).
pub const SNAP_SPEED: f32 = 1.2;

/// Local camera roll twitch (radians, decaying; see [`twitch_camera`]).
#[derive(Resource, Default)]
pub struct CamTwitch {
    t: f32,
    dur: f32,
    amp: f32,
}

impl CamTwitch {
    pub fn kick(&mut self, deg: f32, dur: f32) {
        if self.t < self.dur && self.amp.abs() > deg.abs() {
            return;
        }
        // Alternate the side so repeated kicks don't drift one way.
        let side = if self.amp > 0.0 { -1.0 } else { 1.0 };
        self.amp = side * deg.to_radians();
        self.dur = dur.max(0.05);
        self.t = 0.0;
    }
}

fn bed(to_robot: bool, class: usize) -> &'static str {
    match (to_robot, class) {
        (true, 0) => "tf_seq_robot_s",
        (true, 1) => "tf_seq_robot_m",
        (true, _) => "tf_seq_robot_l",
        (false, 0) => "tf_seq_alt_s",
        (false, 1) => "tf_seq_alt_m",
        (false, _) => "tf_seq_alt_l",
    }
}

fn final_lock(class: usize) -> &'static str {
    ["tf_lock_s", "tf_lock_m", "tf_lock_l"][class.min(2)]
}

/// Transform / Transformed events → sequence sounds, the snap, the mid-air whoosh, camera kicks.
#[allow(clippy::too_many_arguments)]
pub fn events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    map: Res<ConstructEntities>,
    mut q: Query<(&mut TfFeel, &mut TransformFx, &GlobalTransform)>,
    mut sfx: MessageWriter<Sfx3d>,
    mut fov: MessageWriter<FovPunch>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut twitch: ResMut<CamTwitch>,
    mut fx: Option<ResMut<FxParticles>>,
    time: Res<Time<Real>>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let dt = time.delta_secs().min(0.1);
    for ev in events.read() {
        match &ev.event {
            GameEvent::HeldFx { id, fx: HeldFx::Transform, pos, strength, .. } => {
                let Some(&e) = map.0.get(id) else { continue };
                let Ok((mut f, _, gt)) = q.get_mut(e) else { continue };
                let to_robot = *strength > 0.0;
                let speed = strength.abs().max(1.0);
                f.since_held = 0.0;
                begin(&mut f, to_robot, speed, bv(*pos), gt, &mut sfx, &mut fov, &mut shake, &mut twitch, fx.as_deref_mut(), time.elapsed_secs());
            }
            GameEvent::Transformed { id, robot, combined: false } => {
                let Some(&e) = map.0.get(id) else { continue };
                let Ok((mut f, mut tfx, gt)) = q.get_mut(e) else { continue };
                // A pilot's / a combat transform already restarted it (and announced itself).
                if f.since_held < 0.6 || (tfx.to_robot == *robot) {
                    continue;
                }
                tfx.restart_at(*robot, 1.0);
                let at = super::centre_of(&active, *id, gt.translation());
                begin(&mut f, *robot, 1.0, at, gt, &mut sfx, &mut fov, &mut shake, &mut twitch, fx.as_deref_mut(), time.elapsed_secs());
            }
            _ => {}
        }
    }
    // Re-sync: a visual that disagrees with the replicated mode (joined mid-match, a missed event)
    // transforms to match once the sim has been settled in that mode for a moment.
    for (mut f, mut tfx, gt) in &mut q {
        let Some(c) = active.world.constructs.get(f.id) else { continue };
        let x = c.comp.xform;
        let want = x.want == XMode::Robot;
        if x.busy || x.combined || x.hub.is_some() || !tfx.t.is_finite() || tfx.t < 1.0 || want == tfx.to_robot || f.since_held < 1.5 || f.age < 1.0 {
            f.mismatch = 0.0;
            continue;
        }
        // Mode mismatch: wait for it to persist (a piloted toggle mirrors the mode a tick later).
        f.mismatch += dt;
        if f.mismatch > 1.2 {
            f.mismatch = 0.0;
            tfx.restart_at(want, 1.0);
            f.since_held = 0.0;
            let at = super::centre_of(&active, f.id, gt.translation());
            begin(&mut f, want, 1.0, at, gt, &mut sfx, &mut fov, &mut shake, &mut twitch, fx.as_deref_mut(), time.elapsed_secs());
        }
    }
}

/// A sequence starts: the bed (or the snap), the mid-air whoosh, the local kick for a snap.
#[allow(clippy::too_many_arguments)]
fn begin(
    f: &mut TfFeel,
    to_robot: bool,
    speed: f32,
    at: Vec3,
    gt: &GlobalTransform,
    sfx: &mut MessageWriter<Sfx3d>,
    fov: &mut MessageWriter<FovPunch>,
    shake: &mut MessageWriter<CameraShakeEvent>,
    twitch: &mut CamTwitch,
    fx: Option<&mut FxParticles>,
    now: f32,
) {
    let class = size_class(f.height);
    let pitch = f.pitch();
    let w = f.weight();
    f.seq_speed = speed;
    f.clicks_this_seq = 0;
    f.last_t = 0.0;
    f.last_to_robot = to_robot;
    if speed > SNAP_SPEED {
        // The combat snap: a ratchet zip + a hard lock, pitched by size (never slower than the panels).
        let name = if to_robot { "tf_snap_robot" } else { "tf_snap_alt" };
        sfx.write(Sfx3d::at(name, at).pitch((pitch * 0.9 + 0.1).clamp(0.6, 1.5)).gain((0.8 + 0.15 * w).min(1.25)));
        if f.local {
            fov.write(FovPunch { degrees: if to_robot { -2.6 } else { 3.6 } });
            shake.write(CameraShakeEvent { trauma: 0.16, dir: Vec3::ZERO });
            twitch.kick(1.4, 0.26);
        }
        if !to_robot {
            f.burst_t = 0.6; // the vehicle burst: speed lines (combat.rs)
        }
    } else {
        sfx.write(Sfx3d::at(bed(to_robot, class), at).pitch(1.0).gain((0.85 + 0.1 * w).min(1.2)));
        if class == 2 {
            // Giants: the first plates part with a deep groan of mass shifting.
            sfx.write(Sfx3d::at("tf_panel_clunk", at).pitch((pitch * 0.8).clamp(0.5, 1.0)).gain(0.9));
        }
        if f.local {
            fov.write(FovPunch { degrees: -1.0 });
            shake.write(CameraShakeEvent { trauma: 0.05 + 0.03 * w.min(2.0), dir: Vec3::ZERO });
        }
    }
    // Mid-air: the body spins its panels through the air — a whoosh and spiral streaks.
    if !f.grounded {
        sfx.write(Sfx3d::at("tf_air_whoosh", at).pitch((pitch * 1.05).clamp(0.6, 1.4)).gain(0.9));
        if let Some(fx) = fx {
            spiral(fx, gt.translation() + Vec3::Y * f.height * 0.45, f.height, f.vel, now);
        }
    }
}

/// Spiral streaks around a body transforming in the air (hard-light white, short).
fn spiral(fx: &mut FxParticles, centre: Vec3, h: f32, vel: Vec3, now: f32) {
    let r = (h * 0.45).clamp(0.6, 8.0);
    let mut j = Jit((now * 1000.0) as u32 | 1);
    for k in 0..10 {
        let a = k as f32 / 10.0 * std::f32::consts::TAU + j.range(0.0, 0.4);
        let y = j.range(-0.45, 0.45) * h;
        let p = centre + Vec3::new(a.cos() * r, y, a.sin() * r);
        let tangent = Vec3::new(-a.sin(), 0.15, a.cos()) * (r * 2.5);
        fx.emit(Emit {
            kind: PKind::Beam,
            flags: PFlags::PLAIN,
            pos: p,
            vel: tangent * 0.35 + vel * 0.05,
            size: 0.05 * r.sqrt(),
            color: [0.85, 0.95, 1.0],
            emissive: 2.5,
            life: 0.22,
            delay: 0.02 * k as f32,
            ..default()
        });
    }
}

/// Panel events crossed this frame → the synced click cascade, spark showers, the final lock.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn cascade(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut q: Query<(&mut TfFeel, &TransformFx, &GlobalTransform)>,
    mut sfx: MessageWriter<Sfx3d>,
    mut fov: MessageWriter<FovPunch>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut bursts: MessageWriter<BurstFx>,
    mut twitch: ResMut<CamTwitch>,
    mut fx: Option<ResMut<FxParticles>>,
) {
    if active.is_none() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let eye = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    for (mut f, tfx, gt) in &mut q {
        f.click_gap = (f.click_gap - dt).max(0.0);
        let t1 = if tfx.t.is_finite() { tfx.t.clamp(0.0, 1.0) } else { 1.0 };
        // Restarted (a new sequence) or reversed: start over from 0.
        if tfx.to_robot != f.last_to_robot || t1 + 1e-4 < f.last_t {
            f.last_t = 0.0;
            f.last_to_robot = tfx.to_robot;
            f.clicks_this_seq = 0;
            if tfx.speed <= SNAP_SPEED {
                f.seq_speed = tfx.speed;
            }
        }
        let t0 = f.last_t;
        f.last_t = t1;
        if t1 <= t0 || f.age < 0.2 {
            continue;
        }
        let root = gt.affine();
        let scale = gt.scale().y.abs().max(1e-3);
        let class = size_class(f.height);
        let pitch = f.pitch();
        let w = f.weight();
        let snap = tfx.speed > SNAP_SPEED;
        let near = eye.distance(gt.translation());
        let audible = near < 90.0 + 30.0 * w;
        let mut j = Jit(f.id.0.wrapping_mul(2_654_435_761) ^ (t1 * 9973.0) as u32 | 1);
        for e in tfx.events() {
            let et = if tfx.to_robot { e.time } else { 1.0 - e.time };
            if !(et > t0 && et <= t1) {
                continue;
            }
            let wp = root.transform_point3(Vec3::from_array(e.pos.to_array()));
            match e.kind {
                TransformEventKind::Lock => {
                    // One click per lock, throttled so a fast sequence stays a ratchet, not a smear.
                    let gap = if snap { 0.06 } else { [0.014, 0.02, 0.032][class] };
                    let budget = if snap { 3 } else { 40 };
                    if audible && f.click_gap <= 0.0 && f.clicks_this_seq < budget {
                        f.click_gap = gap;
                        f.clicks_this_seq += 1;
                        let jit = j.range(0.94, 1.06);
                        if class == 2 {
                            sfx.write(Sfx3d::at("tf_panel_clunk", wp).pitch((pitch * jit).clamp(0.5, 1.2)).gain(0.75));
                        } else {
                            sfx.write(Sfx3d::at("transform_panel_click", wp).pitch((pitch * jit).clamp(0.7, 1.6)).gain(if class == 0 { 0.5 } else { 0.65 }));
                        }
                    }
                    if let Some(fx) = fx.as_deref_mut() {
                        shower(fx, wp, w, scale, &mut j, snap);
                    }
                }
                // The unfold beat: plates unlatching outside-in — a lighter, higher tick run.
                TransformEventKind::Unlatch if !snap && audible => {
                    if f.click_gap <= 0.0 && j.f() < 0.75 {
                        f.click_gap = [0.012, 0.018, 0.028][class];
                        sfx.write(Sfx3d::at("transform_panel_click", wp).pitch((pitch * 1.25 * j.range(0.95, 1.05)).clamp(0.8, 1.8)).gain(0.32));
                    }
                }
                TransformEventKind::HeadUp => {
                    if audible {
                        sfx.write(Sfx3d::at("tf_eye_zing", wp).pitch((0.8 + 0.2 * pitch).clamp(0.8, 1.3)).gain(0.7));
                    }
                }
                TransformEventKind::StandUp if tfx.to_robot => {
                    let at = gt.translation();
                    sfx.write(Sfx3d::at(final_lock(class), at + Vec3::Y * f.height * 0.5).pitch((0.92 + 0.08 * pitch).clamp(0.85, 1.12)).gain((0.9 + 0.15 * w).min(1.4)));
                    stand_kick(&mut f, at, w, near, &mut fov, &mut shake, &mut twitch, &mut bursts, snap);
                }
                _ => {}
            }
        }
        // The vehicle settles: a lower lock + hiss as the last plates close (not for a combat snap —
        // the burst is already moving).
        if !tfx.to_robot && t0 < 0.97 && t1 >= 0.97 && !snap {
            let at = gt.translation();
            sfx.write(Sfx3d::at(final_lock(class), at + Vec3::Y * f.height * 0.2).pitch((0.8 * (0.92 + 0.08 * pitch)).clamp(0.7, 1.0)).gain((0.7 + 0.1 * w).min(1.1)));
            if f.local {
                fov.write(FovPunch { degrees: 2.0 });
                shake.write(CameraShakeEvent { trauma: 0.07, dir: Vec3::ZERO });
            }
        }
    }
}

/// The robot stands up: kick the local camera; big bodies thump the ground for everyone near.
#[allow(clippy::too_many_arguments)]
fn stand_kick(
    f: &mut TfFeel,
    at: Vec3,
    w: f32,
    near: f32,
    fov: &mut MessageWriter<FovPunch>,
    shake: &mut MessageWriter<CameraShakeEvent>,
    twitch: &mut CamTwitch,
    bursts: &mut MessageWriter<BurstFx>,
    snap: bool,
) {
    if f.local {
        fov.write(FovPunch { degrees: -(1.8 + 0.6 * w.min(2.0)) });
        shake.write(CameraShakeEvent { trauma: 0.14 + 0.07 * w.min(2.5), dir: Vec3::NEG_Y });
        if !snap {
            twitch.kick(0.9 + 0.3 * w.min(2.0), 0.3);
        }
    } else if w >= 1.3 {
        let k = (1.0 - near / (30.0 + 12.0 * w)).clamp(0.0, 1.0);
        if k > 0.0 {
            shake.write(CameraShakeEvent { trauma: (0.08 * w * k).min(0.35), dir: Vec3::NEG_Y });
        }
    }
    if w >= 0.9 && f.grounded {
        bursts.write(BurstFx { kind: BurstKind::Dust, pos: at, dir: Vec3::Y, scale: (0.6 * w).clamp(0.5, 3.0), color: None });
    }
}

/// Hot sparks that spray from a locking panel, fall and bounce (count and size by body weight).
fn shower(fx: &mut FxParticles, p: Vec3, w: f32, scale: f32, j: &mut Jit, snap: bool) {
    let n = ((2.0 + 5.0 * w) * if snap { 0.6 } else { 1.0 }).round().clamp(2.0, 14.0) as u32;
    let s = scale.sqrt().clamp(0.4, 4.0);
    let floor = p.y - 40.0;
    for i in 0..n {
        let d = j.dir();
        let vel = Vec3::new(d.x, d.y.abs() * 0.6 + 0.4, d.z) * j.range(2.5, 6.0) * s;
        let hot = if i % 3 == 0 { [1.0, 0.95, 0.8] } else { [1.0, 0.55, 0.15] };
        fx.emit(Emit {
            flags: PFlags::STRETCH | PFlags::PLAIN | PFlags::BOUNCE,
            pos: p,
            vel,
            size: 0.03 * s,
            color: hot,
            emissive: 4.0,
            life: j.range(0.35, 0.8),
            gravity: 1.6,
            drag: 1.2,
            spin: 3.0,
            seed: j.0,
            aux: floor,
            ..default()
        });
    }
}

/// The local roll twitch, applied about the view axis after the camera rig placed the camera
/// (the aim and the crosshair never change). Comfort: needs the camera lean on and shake > 0.
pub fn twitch_camera(time: Res<Time<Real>>, comfort: Res<ComfortSettings>, mut tw: ResMut<CamTwitch>, mut cams: Query<&mut Transform, With<PlayerCamera>>) {
    if tw.t >= tw.dur {
        return;
    }
    tw.t += time.delta_secs().min(0.05);
    let k = if comfort.roll_on() { comfort.shake.clamp(0.0, 1.0) } else { 0.0 };
    if k <= 0.0 {
        return;
    }
    let u = (tw.t / tw.dur).clamp(0.0, 1.0);
    // A fast lean out, a smaller overshoot back, settled.
    let a = tw.amp * k * (std::f32::consts::PI * 2.0 * u).sin() * (1.0 - u) * (1.0 - u);
    if !a.is_finite() || a.abs() < 1e-5 {
        return;
    }
    for mut t in &mut cams {
        t.rotation *= Quat::from_rotation_z(a);
    }
}
