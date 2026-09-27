//! The local player's own moments: respawn facing, hop-chain feedback (felt, not read: a soft chime
//! rising a pentatonic step per consecutive clean hop, capped, subtle), own-KO camera.

use super::sound::Sfx3d;
use super::{bv, PresentSet};
use crate::arena::ArenaWorld;
use crate::input::LookAngles;
use crate::player::FrameMoveEvents;
use crate::sim_bridge::{ActiveWorld, GameEvents};
use bd_sim::GameEvent;
use bevy::prelude::*;

pub struct LocalPlugin;

impl Plugin for LocalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (respawn_facing, hop_chime, warp_face_arm).in_set(PresentSet::Events));
        app.add_systems(Update, warp_face_ease.after(PresentSet::Events));
        app.init_resource::<WarpFace>();
    }
}

// ------------------------------------------------------------------ warp arrival camera assist

/// TOOLBOX QA (PRESENTATION's ask): after a WarpTo / Relocate arrival ("instant transmission",
/// "teleport behind him") the camera eases toward the enemy you arrived on — a short nudge on the
/// local look (exactly like mouse input, so prediction-safe), never forced: any mouse movement ends
/// it at once, and it gives up after [`WARP_FACE_SECS`].
#[derive(Resource, Default)]
struct WarpFace {
    /// (target yaw, target pitch, seconds left)
    want: Option<(f32, f32, f32)>,
}

/// Longest the assist runs (s) and its ease rate (1/s).
const WARP_FACE_SECS: f32 = 0.35;
const WARP_FACE_RATE: f32 = 14.0;
/// Only an enemy this close to the arrival point is "the one you warped to" (m).
const WARP_FACE_NEAR: f32 = 7.0;

fn warp_face_arm(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut face: ResMut<WarpFace>) {
    use bd_sim::powers::{MoveOp, PowerEvent, PowerTable};
    let Some(active) = active else {
        events.clear();
        return;
    };
    let Some(me) = active.local else {
        events.clear();
        return;
    };
    let mut arrived: Option<bd_core::Vec3> = None;
    let mut warp = false;
    for ev in events.read() {
        match &ev.event {
            GameEvent::Power(PowerEvent::Blinked { player, to, .. }) if *player == me => arrived = Some(*to),
            GameEvent::Power(PowerEvent::Fired { player, mv, .. }) if *player == me => {
                warp |= mv
                    .resolve(PowerTable::get())
                    .is_some_and(|(_, m)| m.ops.iter().any(|o| matches!(o, MoveOp::WarpTo { .. } | MoveOp::Relocate { .. })));
            }
            _ => {}
        }
    }
    let (Some(to), true) = (arrived, warp) else { return };
    let w = &active.world;
    let Some(p) = w.player(me) else { return };
    let eye = to + (p.eye(w.params()) - p.mv.pos);
    let target = w
        .players
        .iter()
        .filter(|q| q.id != me && q.alive)
        .map(|q| q.mv.pos + bd_core::Vec3::Y * 1.2)
        .filter(|c| c.distance(to) <= WARP_FACE_NEAR)
        .min_by(|a, b| a.distance(to).total_cmp(&b.distance(to)));
    let Some(c) = target else { return };
    let d = (c - eye).normalize_or(bd_core::Vec3::NEG_Z);
    if !d.is_finite() {
        return;
    }
    face.want = Some(((-d.x).atan2(-d.z), d.y.asin().clamp(-0.6, 0.5), WARP_FACE_SECS));
}

fn warp_face_ease(time: Res<Time>, motion: Res<bevy::input::mouse::AccumulatedMouseMotion>, mut face: ResMut<WarpFace>, mut look: ResMut<LookAngles>) {
    let Some((yaw, pitch, left)) = face.want else { return };
    // the player's own hand always wins
    if motion.delta.length_squared() > 4.0 || left <= 0.0 {
        face.want = None;
        return;
    }
    let dt = time.delta_secs().min(0.05);
    let k = 1.0 - (-WARP_FACE_RATE * dt).exp();
    let tau = std::f32::consts::TAU;
    let dy = (yaw - look.yaw + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    look.yaw = (look.yaw + dy * k).rem_euclid(tau);
    look.pitch += (pitch - look.pitch) * k;
    face.want = if dy.abs() < 0.01 && (pitch - look.pitch).abs() < 0.01 { None } else { Some((yaw, pitch, left - dt)) };
}

/// Faces the arena centre on respawn. The first spawn honours `--look yaw,pitch` instead (dev shots:
/// map/feature workers frame screenshots with it).
fn respawn_facing(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    arena: Option<Res<ArenaWorld>>,
    cli: Res<crate::cli::CliArgs>,
    mut cli_used: Local<bool>,
    mut look: ResMut<LookAngles>,
) {
    let (Some(active), Some(arena)) = (active, arena) else {
        events.clear();
        return;
    };
    for ev in events.read() {
        if let GameEvent::Respawn { player, pos, .. } = &ev.event
            && active.local == Some(*player)
        {
            if let Some(l) = cli.look.filter(|_| !*cli_used) {
                *cli_used = true;
                look.set_degrees(l.x, l.y);
                continue;
            }
            let eye = bv(*pos) + Vec3::Y * 1.6;
            let d = (arena.info.center - eye).normalize_or(Vec3::NEG_Z);
            look.yaw = (-d.x).atan2(-d.z);
            look.pitch = d.y.asin().clamp(-0.4, 0.2);
        }
    }
}

/// Minor-pentatonic steps (semitones) for hop chains 2..=7.
const STEPS: [f32; 6] = [0.0, 3.0, 5.0, 7.0, 10.0, 12.0];

fn hop_chime(active: Option<Res<ActiveWorld>>, events: Res<FrameMoveEvents>, comfort: Res<crate::settings::ComfortSettings>, mut sfx: MessageWriter<Sfx3d>) {
    if active.is_none() {
        return;
    }
    for ev in &events.0 {
        if ev.jumped && ev.hop_chain >= 2 {
            let k = (ev.hop_chain as usize - 2).min(STEPS.len() - 1);
            let pitch = 2f32.powf((STEPS[k] + 12.0) / 12.0) * 0.5;
            // Subtle; quieter still with the comfort preset's reduced kicks.
            let gain = 0.22 + 0.04 * k as f32;
            let gain = if comfort.fov_kick <= 0.0 { gain * 0.7 } else { gain };
            sfx.write(Sfx3d::ui("ui_move").pitch(pitch).gain(gain).exempt());
        }
    }
}
