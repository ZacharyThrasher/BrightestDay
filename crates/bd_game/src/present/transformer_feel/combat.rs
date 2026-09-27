//! ROBOT COMBAT JUICE + BEAST / DINO (COMBAT.md §5d-quater).
//!
//! * **Swings** (`HeldFx::PilotAttack` / `Uppercut` in robot mode): heavy archetypes (a shoulder-step
//!   dash: trucks, tanks, construction, dinos) swing with a servo-driven heavy whoosh
//!   (`robot_swing_heavy`), light ones (cars, bikes, jets, helicopters) a fast snap
//!   (`robot_swing_fast`) — pitched by size. The robot's arm plays a clip on the swing (an `Attack`
//!   layer on the biped robot rig, alternating sides; a stomp clip for ground pounds).
//!   Blaster shots (a `ShotFired` from the same construct) are left to the gun presentation.
//! * **Connects** (`Hit` from a transformer, melee kinds): a metal layer on top of the normal hit
//!   (`robot_punch_heavy` / `robot_punch_fast`); impact ≥ 2 (finishers, counters, heavy slams — the
//!   impact-frames worker's automatic HeavyWeapon frame fires on these) adds the heavy metallic
//!   crunch (`robot_finisher_crunch`) and a punch-in for the local attacker.
//! * **Transform attack burst**: after a combat transform into the vehicle, speed lines stream off
//!   the body while it moves fast. **Boost-step / drift boost** (`Lunge` / `Thrust`): a burst sound
//!   (`drift_boost_burst`), the local FOV surge. **Perfect dodge** (`Parry` ≥ 2): a bright metallic
//!   sting (`tf_focus_sting`) on top of the chrome flash + desaturation already there.
//! * **Beast / dino** (archetype flavour "dino" / "beast"): a roar as the beast mode arrives
//!   (`kaiju_roar` pitched by size), creature clips' sounds (`CreatureAttack`): roar on the
//!   telegraph of a tail sweep / breath / pounce, `beast_tail_whoosh` on the sweep, `pounce_whoosh`
//!   on the charge, `beast_chomp` on a bite's impact. Roars have a per-construct cooldown.

use super::super::constructs::ConstructEntities;
use super::super::sound::Sfx3d;
use super::super::bv;
use super::{Jit, TfFeel, Voice};
use crate::juice::FovPunch;
use crate::sim_bridge::{ActiveWorld, GameEvents};
use bd_core::ConstructId;
use bd_fx::{BurstFx, BurstKind, ConstructVisual, Emit, FxParticles, PFlags, PKind};
use bd_shapes::{AnimState, Attack};
use bd_sim::GameEvent;
use bd_sim::combat::HitKind;
use bd_sim::constructs::ai::{AttackKind, phase};
use bd_sim::events::HeldFx;
use bevy::prelude::*;

/// Seconds an arm / stomp clip layer is kept on the robot.
fn clip_secs(a: Attack) -> f32 {
    match a {
        Attack::Stomp => 0.75,
        _ => 0.42,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    map: Res<ConstructEntities>,
    mut q: Query<&mut TfFeel>,
    mut sfx: MessageWriter<Sfx3d>,
    mut fov: MessageWriter<FovPunch>,
    mut bursts: MessageWriter<BurstFx>,
    mut shots: Local<Vec<ConstructId>>,
    mut roars: Local<Vec<(ConstructId, f32)>>,
    time: Res<Time<Real>>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let now = time.elapsed_secs();
    roars.retain(|(_, t)| now - *t < 3.0);
    let evs: Vec<&GameEvents> = events.read().collect();
    shots.clear();
    for ev in &evs {
        if let GameEvent::ShotFired { construct: Some(id), .. } = &ev.event {
            shots.push(*id);
        }
    }
    let local = active.local;
    for ev in &evs {
        match &ev.event {
            GameEvent::HeldFx { id, owner, fx, pos, strength, .. } => {
                let Some(&e) = map.0.get(id) else { continue };
                let Ok(mut f) = q.get_mut(e) else { continue };
                let p = bv(*pos);
                let pitch = f.pitch();
                let w = f.weight();
                match fx {
                    HeldFx::PilotAttack | HeldFx::Uppercut if f.robot && !shots.contains(id) => {
                        let heavy = f.heavy || f.voice == Voice::Beast && w >= 1.2;
                        let name = if heavy { "robot_swing_heavy" } else { "robot_swing_fast" };
                        let k = strength.clamp(0.5, 2.6);
                        sfx.write(Sfx3d::at(name, p).pitch((pitch * (1.08 - 0.06 * k)).clamp(0.55, 1.5)).gain((0.55 + 0.2 * k).min(1.1)));
                        f.swing_side = !f.swing_side;
                        f.clip = Some((Attack::Swipe, 0.0));
                    }
                    HeldFx::GroundPound | HeldFx::TransformSlam => {
                        if f.robot {
                            f.clip = Some((Attack::Stomp, clip_secs(Attack::Stomp) * 0.55));
                        }
                        if *strength >= 1.6 || *fx == HeldFx::TransformSlam {
                            sfx.write(Sfx3d::at("robot_finisher_crunch", p).pitch((pitch * 0.95).clamp(0.55, 1.3)).gain((0.7 + 0.2 * w).min(1.25)));
                            bursts.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: Vec3::Y, scale: (0.8 * w + 0.4 * strength.min(3.0)).clamp(0.6, 3.5), color: None });
                        }
                    }
                    HeldFx::DiveStart if f.robot => {
                        sfx.write(Sfx3d::at("tf_air_whoosh", p).pitch((pitch * 0.8).clamp(0.5, 1.2)).gain(0.8));
                    }
                    HeldFx::Lunge => {
                        sfx.write(Sfx3d::at("drift_boost_burst", p).pitch((pitch * 1.15).clamp(0.7, 1.7)).gain(0.55));
                        if local == Some(*owner) && f.local {
                            fov.write(FovPunch { degrees: 1.6 });
                        }
                        f.burst_t = f.burst_t.max(0.25);
                    }
                    HeldFx::Thrust => {
                        sfx.write(Sfx3d::at("drift_boost_burst", p).pitch((pitch * 0.95).clamp(0.6, 1.4)).gain(0.9));
                        if f.local {
                            fov.write(FovPunch { degrees: 4.0 });
                        }
                        f.burst_t = f.burst_t.max(0.5);
                    }
                    HeldFx::Parry if *strength >= 2.0 => {
                        if local == Some(*owner) {
                            sfx.write(Sfx3d::ui("tf_focus_sting").gain(0.85));
                        } else {
                            sfx.write(Sfx3d::at("tf_focus_sting", p).gain(0.6));
                        }
                    }
                    HeldFx::Transform if *strength < 0.0 && f.voice == Voice::Beast => {
                        // The beast arrives: roar as the last plates close.
                        f.roar_in = Some((0.62 / strength.abs().max(1.0)).max(0.12));
                    }
                    _ => {}
                }
            }
            GameEvent::Transformed { id, robot: false, combined: false } => {
                let Some(&e) = map.0.get(id) else { continue };
                if let Ok(mut f) = q.get_mut(e)
                    && f.voice == Voice::Beast
                    && f.roar_in.is_none()
                {
                    f.roar_in = Some(0.62);
                }
            }
            GameEvent::Hit { construct: Some(id), kind, point, impact, source, .. } if matches!(kind, HitKind::Melee | HitKind::HeavyMelee | HitKind::Creature | HitKind::Crush) => {
                let Some(&e) = map.0.get(id) else { continue };
                let Ok(f) = q.get(e) else { continue };
                let p = bv(*point);
                let pitch = f.pitch();
                let k = impact.strength;
                let name = if f.heavy { "robot_punch_heavy" } else { "robot_punch_fast" };
                sfx.write(Sfx3d::at(name, p).pitch((pitch * 0.9 + 0.1).clamp(0.6, 1.4)).gain((0.5 + 0.25 * k).min(1.15)));
                if k >= 2.0 {
                    sfx.write(Sfx3d::at("robot_finisher_crunch", p).pitch((pitch * 1.05).clamp(0.6, 1.3)).gain((0.8 + 0.15 * (k - 2.0)).min(1.3)));
                    if f.local || (local.is_some() && *source == local) {
                        fov.write(FovPunch { degrees: -2.4 });
                    }
                }
            }
            GameEvent::CreatureAttack { id, kind, phase: ph, .. } => {
                let Some(&e) = map.0.get(id) else { continue };
                let Ok(mut f) = q.get_mut(e) else { continue };
                let Some(c) = active.world.constructs.get(*id) else { continue };
                let p = bv(c.center());
                let pitch = f.pitch();
                let w = f.weight();
                let k = AttackKind::from_u8(*kind);
                if *ph == phase::TELEGRAPH && matches!(k, AttackKind::TailSweep | AttackKind::Breath | AttackKind::Charge | AttackKind::Stomp) && !roars.iter().any(|(r, _)| r == id) && f.roar_in.is_none() && f.since_roar > 2.5 {
                    roars.push((*id, now));
                    f.since_roar = 0.0;
                    sfx.write(Sfx3d::at("kaiju_roar", p).pitch((pitch * 1.1).clamp(0.6, 1.5)).gain((0.55 + 0.2 * w).min(1.1)));
                } else if *ph == phase::ACTIVE {
                    match k {
                        AttackKind::TailSweep => {
                            sfx.write(Sfx3d::at("beast_tail_whoosh", p).pitch((pitch * 1.05).clamp(0.55, 1.4)).gain(0.9));
                        }
                        AttackKind::Charge | AttackKind::Swoop => {
                            sfx.write(Sfx3d::at("pounce_whoosh", p).pitch(pitch.clamp(0.6, 1.4)).gain(0.85));
                        }
                        _ => {}
                    }
                } else if *ph == phase::IMPACT && matches!(k, AttackKind::Bite | AttackKind::Grab) {
                    let head = p + bv(c.forward()) * c.half.z.max(c.half.x);
                    sfx.write(Sfx3d::at("beast_chomp", head).pitch((pitch * 1.05).clamp(0.6, 1.5)).gain((0.7 + 0.2 * w).min(1.2)));
                }
            }
            _ => {}
        }
    }
}

/// Clip layers on the robot (arm swings, stomps), beast roars due, transform-attack speed lines.
#[allow(clippy::type_complexity)]
pub fn layers(
    time: Res<Time<Real>>,
    mut q: Query<(&mut TfFeel, &mut ConstructVisual, &GlobalTransform)>,
    mut sfx: MessageWriter<Sfx3d>,
    mut fx: Option<ResMut<FxParticles>>,
) {
    let dt = time.delta_secs().min(0.1);
    for (mut f, mut vis, gt) in &mut q {
        f.since_roar += dt;
        // Beast roar as the beast mode arrives.
        if let Some(t) = f.roar_in.as_mut() {
            *t -= dt;
            if *t <= 0.0 {
                f.roar_in = None;
                f.since_roar = 0.0;
                let pitch = f.pitch();
                let w = f.weight();
                sfx.write(Sfx3d::at("kaiju_roar", gt.translation() + Vec3::Y * f.height * 0.5).pitch((pitch * 1.1).clamp(0.6, 1.5)).gain((0.7 + 0.2 * w).min(1.2)));
            }
        }
        // Arm / stomp clip on top of the construct's own layers (sync_constructs rewrote them this frame).
        if let Some((a, t)) = f.clip {
            let t = t + dt;
            if t > clip_secs(a) || !f.robot {
                f.clip = None;
            } else {
                f.clip = Some((a, t));
                let cur = vis.layers.as_slice();
                if cur.len() < bd_shapes::MAX_LAYERS {
                    let mut buf = [(AnimState::Idle, 0.0f32); bd_shapes::MAX_LAYERS];
                    buf[..cur.len()].copy_from_slice(cur);
                    let n = cur.len();
                    // Alternate sides: a mirrored swipe reads as a left / right combo.
                    buf[n] = (AnimState::Attack(a), t);
                    vis.layers.set(&buf[..n + 1]);
                }
            }
        }
        // Transform-attack burst / boost: speed lines streaming off the body.
        if f.burst_t > 0.0 {
            f.burst_t = (f.burst_t - dt).max(0.0);
            let spd = f.vel.length();
            if spd > 8.0
                && let Some(fx) = fx.as_deref_mut()
            {
                let back = -f.vel / spd;
                let r = (f.height * 0.35).clamp(0.5, 6.0);
                let mut j = Jit(f.id.0.wrapping_mul(0x9E37_79B9) ^ (time.elapsed_secs() * 97.0) as u32 | 1);
                let side = back.cross(Vec3::Y).normalize_or(Vec3::X);
                let up = side.cross(back).normalize_or(Vec3::Y);
                for _ in 0..3 {
                    let a = j.range(0.0, std::f32::consts::TAU);
                    let o = (side * a.cos() + up * a.sin()) * r * j.range(0.7, 1.3);
                    let len = (spd * 0.08).clamp(0.8, 6.0);
                    fx.emit(Emit {
                        kind: PKind::Beam,
                        flags: PFlags::PLAIN,
                        pos: gt.translation() + Vec3::Y * f.height * 0.35 + o + back * len * 0.5,
                        vel: back * len,
                        size: 0.035 * r.sqrt(),
                        color: [0.95, 0.97, 1.0],
                        emissive: 2.2,
                        life: 0.14,
                        ..default()
                    });
                }
            }
        }
    }
}
