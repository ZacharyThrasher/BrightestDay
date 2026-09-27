//! PORTALS: seamless crossing for the local camera (owner: step through a portal with no pop and
//! never a frame of the wrong side).
//!
//! * **The tick you pass** ([`local_crossing`], `SimSet::Post`, host / practice / client prediction
//!   alike): the view turns with the portal (LookAngles rotated by the pass — the next input carries
//!   it, so host and prediction agree), the interpolation start is carried to the exit side, and the
//!   sim's push clear of the exit face (the body jumps ~0.8 m: its leading face triggers the pass and
//!   it comes out with its back face clear) is taken out of the *camera* path as an eye offset that
//!   decays over ~70 ms ([`apply_eye_offset`]): the eye keeps its speed through the plane.
//! * **Fold** ([`fold_camera`], after both rigs placed the camera): a camera that is behind a face the
//!   body is in front of (the smoothed eye still "entering", a third-person boom reaching back
//!   through the exit) is drawn from the partner's side, mapped through the pair. There the entry
//!   disc shows the exit's view — which is exactly what the unfolded camera sees — so the switch is
//!   invisible, and the portal shader ray-casts the opening when the eye is within a hand's width of
//!   it (the near plane never cuts the window open).
//! * Third person: the boom passes through the exit disc and continues on the entry side
//!   (`camera3p` asks [`portal_boom`]), so the camera stays a smooth boom-length behind you, looking
//!   through the entry at you coming out, and never pokes into the wall behind the exit.
//! * Cybertron's space bridge is an ordinary pair: all of this applies.

use super::portal_view::PairGeo;
use crate::camera::{CameraUpdateSet, PlayerCamera, ViewAnchor};
use crate::input::LookAngles;
use crate::player::{LocalPlayer, PlayerSim};
use crate::sim_bridge::ActiveWorld;
use crate::state::AppState;
use crate::tick::{SimSet, SimTick};
use crate::util::{core_v3, v3};
use bd_core::DT;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// Eye-offset decay time constant (s).
const EYE_TAU: f32 = 0.07;

/// The local camera's portal state.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PortalCross {
    /// Camera-only offset (world, exit side) that hides the sim's push, decaying to zero.
    pub eye_offset: Vec3,
    /// The pair / end the camera was drawn through this frame (folded), if any.
    pub folded: Option<(u16, u8)>,
    /// Local passes so far (dev / tests).
    pub passes: u32,
    /// The body's hull centre after the previous tick (the bridge snaps `PlayerSim::prev` over long
    /// jumps, so the pass is detected from this).
    last: Option<Vec3>,
}

pub struct PortalCrossPlugin;

impl Plugin for PortalCrossPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PortalCross>()
            .add_systems(SimTick, local_crossing.in_set(SimSet::Post).run_if(resource_exists::<ActiveWorld>))
            .add_systems(OnEnter(AppState::InGame), |mut c: ResMut<PortalCross>| *c = PortalCross::default())
            .add_systems(Update, feed_debris.before(crate::debris::DebrisSimSet))
            .add_systems(PostUpdate, apply_eye_offset.before(CameraUpdateSet).run_if(in_state(AppState::InGame)))
            .add_systems(
                PostUpdate,
                fold_camera
                    .run_if(in_state(AppState::InGame))
                    .after(CameraUpdateSet)
                    .after(super::camera3p::place_camera)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Did the body go through a portal between `before` and `after` (hull centres)? → (pair, entry).
pub fn detect_pass(before: Vec3, after: Vec3, pairs: &[PairGeo]) -> Option<(PairGeo, usize)> {
    for pp in pairs {
        for j in 0..2 {
            let back = pp.map_point(j, after);
            let entry = 1 - j;
            if (back - before).length() < 3.0 && pp.ends[entry].crossed(before, back, 0.6) {
                return Some((*pp, entry));
            }
        }
    }
    None
}

fn local_crossing(
    active: Res<ActiveWorld>,
    mut look: ResMut<LookAngles>,
    mut cross: ResMut<PortalCross>,
    mut q: Query<&mut PlayerSim, With<LocalPlayer>>,
    mut pairs: Local<Vec<PairGeo>>,
) {
    let Ok(mut sim) = q.single_mut() else { return };
    let params = &active.world.cfg.move_params;
    let off_now = v3(sim.state.hull(params).center() - sim.state.pos);
    let c1 = v3(sim.state.pos) + off_now;
    let before = cross.last.replace(c1);
    if active.world.portals.is_empty() {
        return;
    }
    let Some(c0) = before else { return };
    pairs.clear();
    pairs.extend(active.world.portals.iter().map(PairGeo::from_sim));
    let Some((pp, entry)) = detect_pass(c0, c1, &pairs) else { return };
    let rot = pp.rot(entry);
    // The view turns with the portal.
    let dir = rot * look.forward();
    look.yaw = (-dir.x).atan2(-dir.z);
    look.pitch = dir.y.clamp(-1.0, 1.0).asin().clamp(-1.56, 1.56);
    // Interpolate on the exit side, along the path the body would have taken without the push.
    let c0x = pp.map_point(entry, c0);
    let natural = c0x + v3(sim.state.vel) * DT;
    let push = natural - c1;
    let prev_c = c0x - push;
    sim.prev.pos = core_v3(prev_c - off_now);
    sim.prev.vel = sim.state.vel;
    cross.eye_offset = (push + cross.eye_offset).clamp_length_max(2.0);
    cross.passes += 1;
}

/// Debris (client-only cubes) flies through portals too.
fn feed_debris(active: Option<Res<ActiveWorld>>, dp: Option<ResMut<crate::debris::DebrisPortals>>) {
    let Some(mut dp) = dp else { return };
    let n = active.as_ref().map_or(0, |a| a.world.portals.len());
    if n == 0 {
        if !dp.0.is_empty() {
            dp.0.clear();
        }
        return;
    }
    dp.0.clear();
    if let Some(a) = active {
        for p in &a.world.portals {
            let pp = PairGeo::from_sim(p);
            for i in 0..2 {
                let (e, f) = (&pp.ends[i], &pp.ends[1 - i]);
                dp.0.push(crate::debris::DebrisPortal { pos: e.pos, n: e.n, right: e.right(), up: e.up, half: e.half, door: e.door, exit_pos: f.pos, exit_n: f.n, rot: pp.rot(i) });
            }
        }
    }
}

fn apply_eye_offset(time: Res<Time<Real>>, mut cross: ResMut<PortalCross>, mut anchor: ResMut<ViewAnchor>) {
    if cross.eye_offset == Vec3::ZERO {
        return;
    }
    anchor.eye += cross.eye_offset;
    let k = (-time.delta_secs().min(0.1) / EYE_TAU).exp();
    cross.eye_offset *= k;
    if cross.eye_offset.length_squared() < 1e-6 {
        cross.eye_offset = Vec3::ZERO;
    }
}

/// Draw a camera that is behind a face the body is in front of from the partner's side.
pub fn fold_camera(
    active: Option<Res<ActiveWorld>>,
    anchor: Res<ViewAnchor>,
    mut cross: ResMut<PortalCross>,
    mut cam: Query<&mut Transform, With<PlayerCamera>>,
) {
    cross.folded = None;
    let Some(a) = active.filter(|a| !a.world.portals.is_empty()) else { return };
    let Ok(mut tf) = cam.single_mut() else { return };
    // The body's own eye (without the camera-only offset) is always on the right side.
    let eye = anchor.eye - cross.eye_offset;
    let c = tf.translation;
    for p in &a.world.portals {
        let pp = PairGeo::from_sim(p);
        for (i, e) in pp.ends.iter().enumerate() {
            if e.side(eye) >= -0.05 && e.side(c) < 0.0 && e.crossed(eye, c, 0.3) {
                tf.translation = pp.map_point(i, c);
                tf.rotation = (pp.rot(i) * tf.rotation).normalize();
                cross.folded = Some((pp.id, i as u8));
                return;
            }
        }
    }
}

/// Third person: if the boom from `pivot` along `back` (unit) for `boom` m passes through a portal
/// face front-to-back with open air up to it, the length it can reach through the partner (free
/// space measured on the other side with `ray`, which returns the hit distance). `None` = no portal
/// on the boom (use the normal solve).
pub fn portal_boom(pairs: &[PairGeo], pivot: Vec3, back: Vec3, boom: f32, pad: f32, ray: impl Fn(Vec3, Vec3, f32) -> Option<f32>) -> Option<f32> {
    for pp in pairs {
        for (i, e) in pp.ends.iter().enumerate() {
            let s = e.side(pivot);
            let dn = back.dot(e.n);
            if !(s > 0.05 && dn < -1e-3) {
                continue;
            }
            let tp = s / -dn;
            if tp >= boom {
                continue;
            }
            let hit = pivot + back * tp;
            if !e.inside(hit - e.pos - e.n * e.side(hit), -0.2) {
                continue;
            }
            if ray(pivot, back, tp).is_some() {
                continue; // blocked before the portal: the normal solve handles it
            }
            let q = pp.map_point(i, hit);
            let b2 = pp.rot(i) * back;
            let rest = boom - tp;
            let free = ray(q + b2 * 0.02, b2, rest + pad).map_or(rest, |t| (t - pad).max(0.0));
            return Some(tp + free);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::portal_view::EndGeo;
    use super::*;

    fn pair() -> PairGeo {
        let e = |pos: Vec3, n: Vec3| EndGeo { pos, n, up: Vec3::Y, half: Vec2::splat(2.4), door: false };
        PairGeo { id: 7, ends: [e(Vec3::new(0.0, 2.0, 0.0), Vec3::Z), e(Vec3::new(40.0, 2.0, -10.0), Vec3::X)] }
    }

    #[test]
    fn a_pass_is_detected_and_the_eye_path_stays_continuous() {
        let pp = pair();
        // Walking −Z into end 0 at 8 m/s: centre 0.35 m in front, then out of end 1 pushed clear.
        let before = Vec3::new(0.3, 1.9, 0.35);
        let after = pp.map_point(0, Vec3::new(0.3, 1.9, -0.45));
        let (p, entry) = detect_pass(before, after, &[pp]).expect("detected");
        assert_eq!((p.id, entry), (7, 0));
        assert!(detect_pass(before, before + Vec3::NEG_Z * 0.07, &[pp]).is_none(), "no pass without a jump");
        // The "natural" continuation mapped to the exit is behind the exit face (the camera is
        // then drawn folded from the entry side), within the push distance.
        let natural = pp.map_point(0, before) + pp.rot(0) * Vec3::new(0.0, 0.0, -8.0) * DT;
        assert!(pp.ends[1].side(natural) < 0.0 && pp.ends[1].side(after) > 0.0);
        // Folding that point back lands in front of the entry: continuous with `before`.
        let folded = pp.map_point(1, natural);
        assert!(pp.ends[0].side(folded) > 0.0 && (folded - before).length() < 0.2, "{folded} vs {before}");
    }

    #[test]
    fn third_person_boom_reaches_through_the_exit() {
        let pp = pair();
        // Just out of end 1 (facing +X), looking +X: the boom goes back (−X) through end 1.
        let pivot = Vec3::new(41.0, 2.0, -10.0);
        let open = |_: Vec3, _: Vec3, _: f32| None;
        let l = portal_boom(&[pp], pivot, Vec3::NEG_X, 3.0, 0.28, open).expect("through");
        assert!((l - 3.0).abs() < 1e-3, "{l}");
        // A wall 1 m behind the entry side limits it there.
        let wall = |o: Vec3, _: Vec3, _: f32| (o.x.abs() < 45.0 && o.z > 0.0).then_some(1.0);
        let l = portal_boom(&[pp], pivot, Vec3::NEG_X, 3.0, 0.28, wall).expect("through");
        assert!(l < 2.0 && l > 1.0, "{l}");
        // Boom not crossing any portal: None.
        assert!(portal_boom(&[pp], Vec3::new(10.0, 2.0, 10.0), Vec3::NEG_X, 3.0, 0.28, open).is_none());
    }
}
