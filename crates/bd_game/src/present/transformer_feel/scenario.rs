//! `--scenario tffeel --tf-shot follow|beast|drift|walk` (practice): dev frames for the transformer
//! feel pass. Casts the real lexicon's phrase on open ground, benches the bots (one enemy dummy stays
//! as the target), and scripts the input / camera:
//! * `follow`: a follower "optimus prime" drives at a dummy 34 m away, transforms (autonomous: only a
//!   `Transformed` event) and walks in to fight — you watch side-on from 16 m.
//! * `beast`: a follower Grimlock stalks the dummy in T-rex mode (beast clips + footfalls), then fights.
//! * `drift`: you drive Bumblebee, carve a hard drift (Dodge held while turning), release for the boost.
//! * `walk`: you drive Optimus, transform on the ground, walk the robot (third person).
//!
//! Logs `tffeel t=…` lines (mode, speed, TransformFx progress) for the frame sheets.

use crate::console::RingLexicon;
use crate::input::{InputOverride, LookAngles};
use crate::sim_bridge::ActiveWorld;
use bd_core::input::buttons;
use bd_core::{ConstructId, PlayerId, Vec3};
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::prelude::{App, IntoScheduleConfigs, Query, Res, ResMut, Resource, Time, Update, info, warn};
use bevy::time::Real;

#[derive(Resource, Clone, Debug, Default)]
pub struct TfScenario {
    shot: String,
    t: f32,
    placed: bool,
    id: Option<ConstructId>,
    dir: Vec3,
    from: Vec3,
    step: u32,
    dummy: Option<PlayerId>,
}

pub fn build(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "tffeel") {
        return;
    }
    let shot = args.windows(2).find(|w| w[0] == "--tf-shot").map(|w| w[1].clone()).unwrap_or_else(|| "follow".into());
    info!("tffeel scenario: {shot}");
    app.insert_resource(TfScenario { shot, ..Default::default() }).add_systems(Update, script.run_if(resource_exists::<ActiveWorld>));
}

/// Flat open ground near `me` with a clear 60 m lane: (stand-at, lane direction).
fn stage(w: &bd_sim::World, me: Vec3) -> Option<(Vec3, Vec3)> {
    let top = |x: f32, z: f32| bd_core::voxel::raycast(&w.voxels, Vec3::new(x, 150.0, z), Vec3::NEG_Y, 170.0).map(|h| h.point.y);
    let mut cands: Vec<Vec3> = vec![me];
    for gx in -8..=8 {
        for gz in -8..=8 {
            cands.push(Vec3::new(gx as f32 * 10.0, 0.0, gz as f32 * 10.0));
        }
    }
    let mut best: Option<(u32, Vec3, Vec3)> = None;
    for c in cands {
        let Some(y0) = top(c.x, c.z) else { continue };
        if bd_core::voxel::raycast(&w.voxels, Vec3::new(c.x, y0 + 1.0, c.z), Vec3::Y, 30.0).is_some() {
            continue;
        }
        for k in 0..8 {
            let a = k as f32 / 8.0 * std::f32::consts::TAU;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            let mut score = 0;
            for s in 0..15 {
                let p = c + d * (s as f32 * 4.0);
                for o in [-6.0f32, -3.0, 0.0, 3.0, 6.0] {
                    let q = p + Vec3::new(-d.z, 0.0, d.x) * o;
                    if top(q.x, q.z).is_some_and(|y| (y - y0).abs() < 1.0) {
                        score += 1;
                    }
                }
            }
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, Vec3::new(c.x, y0 + 0.05, c.z), d));
            }
        }
    }
    best.map(|b| (b.1, b.2))
}

fn look_at(look: &mut LookAngles, eye: Vec3, at: Vec3) {
    let to = at - eye;
    look.yaw = (-to.x).atan2(-to.z);
    look.pitch = (to.y / to.length().max(0.1)).clamp(-0.9, 0.9).asin();
}

fn ground(w: &bd_sim::World, at: Vec3) -> f32 {
    bd_core::voxel::raycast(&w.voxels, Vec3::new(at.x, at.y + 8.0, at.z), Vec3::NEG_Y, 40.0).map_or(at.y, |h| h.point.y)
}

#[allow(clippy::too_many_arguments)]
fn script(
    time: Res<Time<Real>>,
    lex: Option<Res<RingLexicon>>,
    mut sc: ResMut<TfScenario>,
    mut active: ResMut<ActiveWorld>,
    mut look: ResMut<LookAngles>,
    mut ov: ResMut<InputOverride>,
    fxs: Query<&bd_fx::TransformFx>,
    map: Res<super::super::constructs::ConstructEntities>,
) {
    let dt = time.delta_secs();
    sc.t += dt;
    if sc.t < 0.8 {
        return;
    }
    let Some((me, my_pos)) = active.local_player().filter(|p| p.alive).map(|p| (p.id, p.mv.pos)) else { return };
    if !sc.placed {
        let Some(lex) = lex else { return };
        let Some((from, dir)) = stage(&active.world, my_pos) else {
            if sc.t > 6.0 {
                warn!("tffeel scenario: no stage found");
                sc.placed = true;
            }
            return;
        };
        active.bots.clear();
        active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Infinite;
        let others: Vec<PlayerId> = active.world.players.iter().map(|p| p.id).filter(|id| *id != me).collect();
        for (k, id) in others.into_iter().enumerate() {
            if k == 0 {
                sc.dummy = Some(id);
                continue;
            }
            if let Some(p) = active.world.player_mut(id) {
                p.alive = false;
                p.respawn_timer = 1e6;
            }
        }
        if let Some(p) = active.world.player_mut(me) {
            p.mv.pos = from;
            p.mv.vel = Vec3::ZERO;
            p.spawn_shield = 0.0;
        }
        let phrase = match sc.shot.as_str() {
            "follow" => "optimus prime that follows me",
            "beast" => "grimlock that follows me",
            "drift" => "bumblebee",
            _ => "optimus prime",
        };
        let it = lex.0.intent(phrase, &[], false);
        let r = it.resolution;
        info!("tffeel scenario: \"{phrase}\" -> {} pilot {:?}", r.signature(), it.pilot);
        let eye = active.world.player(me).map(|p| p.eye(active.world.params())).unwrap_or(from);
        look.yaw = (-dir.x).atan2(-dir.z);
        look.pitch = -0.3;
        sc.id = match active.world.cast_shaped(me, r.recipe, r.shape, r.alt_shape, bd_sim::Aim::new(eye, (dir + Vec3::new(0.0, -0.35, 0.0)).normalize_or_zero())) {
            Ok(id) => {
                if let Some(c) = active.world.constructs.get_mut(id) {
                    c.life = 1e6;
                }
                Some(id)
            }
            Err(e) => {
                warn!("tffeel scenario: cast failed {e:?}");
                None
            }
        };
        sc.dir = dir;
        sc.from = from;
        sc.placed = true;
        sc.t = 1.0;
        return;
    }
    let Some(id) = sc.id.filter(|id| active.world.constructs.get(*id).is_some()) else { return };
    let t = sc.t;
    let dir = sc.dir;
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    if (t * 4.0).floor() != ((t - dt) * 4.0).floor()
        && let Some(c) = active.world.constructs.get(id)
    {
        let vis = map.0.get(&id).and_then(|e| fxs.get(*e).ok()).map(|f| (f.to_robot, f.t));
        info!(
            "tffeel t={t:.2}: pos {:?} speed {:.1} xform {:?} busy {} vis {:?} grounded {} half {:?}",
            c.pos,
            Vec3::new(c.vel.x, 0.0, c.vel.z).length(),
            c.comp.xform.mode,
            c.comp.xform.busy,
            vis,
            c.grounded,
            c.half
        );
    }
    ov.active = true;
    ov.held = 0;
    ov.move_x = 0;
    ov.move_z = 0;
    // The dummy target stands down the lane (followers engage it).
    if let Some(d) = sc.dummy {
        let far = if sc.shot == "beast" { 30.0 } else { 34.0 };
        let at = sc.from + dir * far + side * 2.0;
        let gy = ground(&active.world, at);
        if let Some(p) = active.world.player_mut(d) {
            p.alive = true;
            p.mv.pos = Vec3::new(at.x, gy + 0.05, at.z);
            p.mv.vel = Vec3::ZERO;
            p.damage_pct = 0.0;
            p.spawn_shield = 0.0;
        }
    }
    match sc.shot.as_str() {
        // Watch the follower side-on from a spot beside the lane, tracking it.
        "follow" | "beast" => {
            // Autonomous transforms (the host's mode machine: only a `Transformed` event reaches clients).
            let due = [(2.5f32, 1u32), (6.0, 2), (8.0, 3)];
            if let Some(&(_, s)) = due.iter().find(|(at, s)| t > *at && sc.step + 1 == *s)
                && let Some(c) = active.world.constructs.get_mut(id)
            {
                let ok = bd_sim::constructs::components::request_transform(c);
                info!("tffeel scenario: autonomous transform {s} -> {ok}");
                sc.step = s;
            }
            let Some(c) = active.world.constructs.get(id) else { return };
            let centre = c.center();
            // A hovering vantage 6 m up beside the lane (clear of trees / props).
            let base = sc.from + dir * 10.0 - side * 16.0;
            let gy = ground(&active.world, base);
            if let Some(p) = active.world.player_mut(me) {
                p.mv.pos = Vec3::new(base.x, gy + 6.0, base.z);
                p.mv.vel = Vec3::ZERO;
            }
            let eye = active.world.player(me).map(|p| p.eye(active.world.params())).unwrap_or(base);
            look_at(&mut look, eye, centre);
        }
        // Drive, carve a drift (Dodge held while turning), release: the drift boost.
        "drift" => {
            ov.move_z = 1;
            let yaw0 = (-dir.x).atan2(-dir.z);
            let turn = if t > 2.6 && t < 4.2 { (t - 2.6) * 1.5 } else if t >= 4.2 { 2.4 } else { 0.0 };
            look.yaw = yaw0 + turn;
            look.pitch = -0.35;
            ov.held = if t > 2.5 && t < 4.1 { buttons::DODGE } else { 0 };
        }
        // Drive a moment, transform on the ground, walk the robot.
        _ => {
            look.yaw = (-dir.x).atan2(-dir.z);
            look.pitch = -0.4;
            if t > 1.6 && sc.step == 0 {
                sc.step = 1;
                active.world.toggle_transform(me);
            }
            ov.move_z = if t > 3.0 { 1 } else { 0 };
        }
    }
}
