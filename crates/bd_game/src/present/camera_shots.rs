//! `BD_CAM_SHOT=<scene>` (practice; dev screenshots of the camera work, never set in play):
//!
//! | scene | map | what |
//! |---|---|---|
//! | `gun` | any | a gun in hand (`BD_CAM_GUN=railgun|pistol|shotgun`, default railgun), a bot 9 m ahead; the view steps 1P hip → 3P hip → 3P ADS → 1P ADS every `BD_CAM_STEP` s (default 1.5) starting at 3 s |
//! | `interior` | miami | third person with the back to a Brickell lobby's glass wall (camera collision) |
//! | `wall` | miami | third person hugging a tower corner: the camera sees past it, the eye is blocked (parallax indicator) |
//!
//! Pair with `--screenshot target/camera/x.png --screenshot-at 3.2,4.7,6.2,7.7 --quit-after 8.5`,
//! `BD_HIDDEN_WINDOW=1` and `BD_UI_PREFS=<scratch file>` (so the owner's prefs are untouched).

use super::ads::AdsScript;
use super::camera3p::{AimRay, ViewMode};
use crate::input::LookAngles;
use crate::sim_bridge::ActiveWorld;
use crate::state::AppState;
use crate::tick::FrameSet;
use bd_core::mechanics::{Element, Form, Mechanic};
use bevy::prelude::*;

#[derive(Resource, Debug)]
struct Shot {
    scene: String,
    t: f32,
    placed: bool,
    yaw: f32,
    pitch: f32,
    feet: bd_core::Vec3,
}

pub fn build(app: &mut App) {
    let Ok(scene) = std::env::var("BD_CAM_SHOT") else { return };
    info!("camera shots: scene {scene}");
    app.insert_resource(Shot { scene, t: 0.0, placed: false, yaw: 0.0, pitch: 0.0, feet: bd_core::Vec3::ZERO })
        .add_systems(Update, drive.before(FrameSet::Input).run_if(in_state(AppState::InGame)).run_if(resource_exists::<ActiveWorld>));
}

fn step_secs() -> f32 {
    std::env::var("BD_CAM_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(1.5)
}

fn ground_at(w: &bd_sim::World, x: f32, z: f32, from_y: f32) -> Option<f32> {
    bd_core::voxel::raycast(&w.voxels, bd_core::Vec3::new(x, from_y, z), bd_core::Vec3::NEG_Y, 40.0).map(|h| h.point.y)
}

fn gun_recipe() -> bd_core::mechanics::Recipe {
    let kind = std::env::var("BD_CAM_GUN").unwrap_or_else(|_| "railgun".into());
    let mut r = match kind.as_str() {
        "pistol" => bd_sim::bots::recipe("hand cannon", Form::Gun, &[], Element::Light, 0.6),
        "shotgun" => bd_sim::bots::recipe("shotgun", Form::Gun, &[Mechanic::Shoots], Element::Light, 0.6),
        _ => bd_sim::bots::recipe("railgun", Form::Gun, &[Mechanic::Pierce], Element::Light, 0.9),
    };
    if kind == "shotgun" {
        r.count = 3;
    }
    r
}

#[allow(clippy::too_many_arguments)]
fn drive(
    time: Res<Time<Real>>,
    mut shot: ResMut<Shot>,
    mut active: ResMut<ActiveWorld>,
    mut look: ResMut<LookAngles>,
    mut mode: ResMut<ViewMode>,
    mut ads: ResMut<AdsScript>,
    aim: Res<AimRay>,
    mut logged: Local<bool>,
) {
    shot.t += time.delta_secs();
    let Some(me) = active.local else { return };
    if !shot.placed {
        if shot.t < 0.8 || !active.world.player(me).is_some_and(|p| p.alive) {
            return;
        }
        let Some(placed) = place(&shot.scene, &mut active) else {
            if shot.t > 6.0 {
                warn!("camera shots: could not place scene {}", shot.scene);
                shot.placed = true;
            }
            return;
        };
        (shot.feet, shot.yaw, shot.pitch) = placed;
        active.bots.clear();
        shot.placed = true;
        info!("camera shots: placed at {:?} yaw {:.0}°", shot.feet, shot.yaw.to_degrees());
    }
    // Hold the pose (nothing drives the local player in a hidden window; keep it pinned anyway).
    let feet = shot.feet;
    if let Some(p) = active.world.player_mut(me) {
        if p.mv.pos.distance(feet) > 0.05 {
            p.mv.pos = feet;
        }
        p.mv.vel = bd_core::Vec3::ZERO;
        p.spawn_shield = 0.0;
        p.damage_pct = 0.0;
    }
    look.yaw = shot.yaw;
    look.pitch = shot.pitch;
    match shot.scene.as_str() {
        "gun" => {
            // 1P hip → 3P hip → 3P ADS → 1P ADS.
            let k = ((shot.t - 2.3) / step_secs()).floor().max(0.0) as i32;
            let (third, aim_in) = match k {
                0 => (false, false),
                1 => (true, false),
                2 => (true, true),
                _ => (false, true),
            };
            mode.cli_third = third;
            ads.0 = Some(aim_in);
        }
        _ => {
            mode.cli_third = true;
            ads.0 = Some(false);
        }
    }
    if shot.t > 3.0 && !*logged {
        *logged = true;
        info!("camera shots: camera {:?} aim point {:?} blocked {:?}", aim.camera, aim.point, aim.blocked);
        println!("CAMSHOT camera={:?} origin={:?} aim={:?} blocked={:?}", aim.camera, aim.origin, aim.point, aim.blocked);
    }
}

/// Place the scene; returns (feet, yaw, pitch).
fn place(scene: &str, active: &mut ActiveWorld) -> Option<(bd_core::Vec3, f32, f32)> {
    let me = active.local?;
    let w = &mut active.world;
    let (feet, yaw, pitch, target) = match scene {
        "interior" => {
            // West Brickell tower lobby x ∈ (-92, -77), z ∈ (-53, -37): back 1.4 m off the west glass,
            // looking east into the lobby (the boom runs into the glass behind).
            let (x, z) = (-90.6, -41.5);
            let y = ground_at(w, x, z, 8.5)?;
            let t = ground_at(w, -86.0, -40.5, 8.5).map(|g| bd_core::Vec3::new(-86.0, g + 0.05, -40.5));
            (bd_core::Vec3::new(x, y + 0.05, z), -std::f32::consts::FRAC_PI_2, -0.12, t)
        }
        "wall" => {
            // East Brickell tower x ∈ (-55, -44), z ∈ (-53, -37): stand 1.2 m south of its south face,
            // the corner just right of the eye, looking north (+Z): right = -X, so the eye is 0.25 m
            // inside the face's edge and the right-shoulder camera sees past it.
            let (x, z) = (-54.75, -54.2);
            let y = ground_at(w, x, z, 12.0)?;
            (bd_core::Vec3::new(x, y + 0.05, z), std::f32::consts::PI, 0.0, None)
        }
        _ => {
            // Open ground with a bot 9 m ahead.
            let me_pos = w.player(me)?.mv.pos;
            let mut found = None;
            for step in 0..24 {
                let yaw = step as f32 * std::f32::consts::TAU / 24.0;
                let dir = bd_core::input::look_dir(yaw, 0.0);
                let p = me_pos + dir * 9.0;
                if let Some(g) = ground_at(w, p.x, p.z, me_pos.y + 10.0).filter(|g| (g - me_pos.y).abs() < 3.0) {
                    found = Some((yaw, bd_core::Vec3::new(p.x, g + 0.05, p.z)));
                    break;
                }
            }
            let (yaw, t) = found?;
            (me_pos, yaw, -0.06, Some(t))
        }
    };
    if let Some(p) = w.player_mut(me) {
        p.mv.pos = feet;
        p.mv.vel = bd_core::Vec3::ZERO;
    }
    // (Run with `--bots 1`: that bot is the target.)
    if let (Some(t), Some(id)) = (target, w.players.iter().map(|p| p.id).find(|id| *id != me))
        && let Some(p) = w.player_mut(id)
    {
        p.mv.pos = t;
        p.mv.vel = bd_core::Vec3::ZERO;
        p.spawn_shield = 0.0;
    }
    if scene == "gun" {
        let eye = feet + bd_core::Vec3::Y * 1.6;
        let _ = w.cast(me, gun_recipe(), None, bd_sim::Aim::new(eye, bd_core::input::look_dir(yaw, 0.0)));
    }
    Some((feet, yaw, pitch))
}
