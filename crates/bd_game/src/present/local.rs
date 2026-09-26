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
        app.add_systems(Update, (respawn_facing, hop_chime).in_set(PresentSet::Events));
    }
}

fn respawn_facing(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, arena: Option<Res<ArenaWorld>>, mut look: ResMut<LookAngles>) {
    let (Some(active), Some(arena)) = (active, arena) else {
        events.clear();
        return;
    };
    for ev in events.read() {
        if let GameEvent::Respawn { player, pos, .. } = &ev.event
            && active.local == Some(*player)
        {
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
