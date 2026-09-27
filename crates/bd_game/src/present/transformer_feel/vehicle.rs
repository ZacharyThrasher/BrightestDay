//! VEHICLE MODE per archetype: what the alt mode sounds like and how it throws itself around.
//!
//! * **Engine voice** by the archetype's flavour (`TfFeel::voice`): trucks keep the base diesel
//!   (`vehicle_engine_loop`), cars a V8 (`engine_car_loop`), bikes a revving twin (`engine_bike_loop`),
//!   jets a turbine whine (`engine_jet_loop`), tanks a tread clank (`engine_tank_loop`), helicopters
//!   the rotor (`rotor_loop`); beasts none. Pitched by speed and size, faded out in robot mode (a
//!   robot keeps a low idle hum), cross-faded through the transformation.
//! * **Drift**: the sim's drift state is host-only, so the slide is read from the replicated motion —
//!   horizontal velocity vs the facing. Sliding on wheels = tyre smoke off the rear + a screech
//!   (`tyre_screech_loop`) by the slip; treads throw dust instead.
//! * **Boost trails**: near top speed, tail-light streaks (afterburner flame for jets).
//!   The drift-boost burst itself (`HeldFx::Thrust`) is in `combat::events`.

use super::super::sound::SfxLoopEmitter;
use super::{Jit, TfFeel, Voice};
use crate::sim_bridge::ActiveWorld;
use bd_fx::{BurstFx, BurstKind, ConstructEnd, Emit, FxParticles, PFlags, PKind, TransformFx};
use bevy::prelude::*;

/// Marks an engine / screech loop child of a transformer.
#[derive(Component)]
pub struct TfLoop;

fn engine_name(v: Voice) -> Option<&'static str> {
    Some(match v {
        Voice::Car | Voice::Boat => "engine_car_loop",
        Voice::Bike => "engine_bike_loop",
        Voice::Jet => "engine_jet_loop",
        Voice::Tread => "engine_tank_loop",
        Voice::Rotor => "rotor_loop",
        Voice::Diesel | Voice::Train | Voice::Beast => return None,
    })
}

/// Speed at which the engine voice is "flat out" (m/s).
const TOP: f32 = 26.0;

/// Engine gain / pitch for `v` at speed fraction `s` (0..1+).
fn engine_level(v: Voice, s: f32) -> (f32, f32) {
    let s = s.clamp(0.0, 1.3);
    match v {
        Voice::Car => (0.3 + 0.55 * s, 0.72 + 0.85 * s),
        Voice::Boat => (0.3 + 0.5 * s, 0.6 + 0.6 * s),
        Voice::Bike => (0.28 + 0.6 * s, 0.78 + 1.1 * s),
        Voice::Jet => (0.35 + 0.5 * s, 0.82 + 0.5 * s),
        Voice::Tread => (0.35 + 0.45 * s, 0.78 + 0.5 * s),
        Voice::Rotor => (0.45 + 0.3 * s, 0.9 + 0.25 * s),
        _ => (0.0, 1.0),
    }
}

/// How much of the vehicle is "on" (1 = alt mode, 0 = robot), through the sequence.
fn alt_amount(tfx: &TransformFx) -> f32 {
    let t = if tfx.t.is_finite() { tfx.t.clamp(0.0, 1.0) } else { 1.0 };
    if tfx.to_robot { 1.0 - t } else { t }
}

#[allow(clippy::type_complexity)]
pub fn voice(
    mut commands: Commands,
    mut q: Query<(Entity, &mut TfFeel, &TransformFx, Option<&mut SfxLoopEmitter>, Option<&ConstructEnd>), Without<TfLoop>>,
    mut loops: Query<&mut SfxLoopEmitter, With<TfLoop>>,
) {
    for (e, mut f, tfx, base, end) in &mut q {
        let alt = alt_amount(tfx);
        let spd = Vec3::new(f.vel.x, 0.0, f.vel.z).length() + f.vel.y.abs() * 0.5;
        let s = spd / TOP;
        let size_p = f.pitch().sqrt();
        // The base loop (constructs.rs: `vehicle_engine_loop` on every Vehicle-form construct).
        if let Some(mut b) = base {
            match f.voice {
                Voice::Diesel | Voice::Train => {
                    // Trucks: the diesel, dropping to a low idle rumble while standing as a robot.
                    let idle = 0.12;
                    b.gain = b.gain * (idle + (1.0 - idle) * alt);
                    b.pitch = b.pitch * (0.8 + 0.2 * alt) * size_p.clamp(0.75, 1.2);
                }
                _ => b.gain = 0.0,
            }
        }
        // Our archetype voice.
        let Some(name) = engine_name(f.voice).filter(|_| end.is_none()) else {
            if let Some(c) = f.engine.take() {
                commands.entity(c).try_despawn();
            }
            continue;
        };
        let (g, p) = engine_level(f.voice, s);
        let robot_hum = 0.08;
        let gain = g * (robot_hum + (1.0 - robot_hum) * alt);
        let pitch = (p * size_p).clamp(0.4, 2.4) * (0.85 + 0.15 * alt);
        match f.engine {
            None => {
                let child = commands.spawn((Name::new("TfEngine"), TfLoop, Transform::default(), SfxLoopEmitter { name, gain, pitch })).id();
                commands.entity(e).add_child(child);
                f.engine = Some(child);
            }
            Some(c) => {
                if let Ok(mut l) = loops.get_mut(c) {
                    if (l.gain - gain).abs() > 0.005 || (l.pitch - pitch).abs() > 0.004 {
                        l.gain = gain;
                        l.pitch = pitch;
                    }
                } else {
                    f.engine = None;
                }
            }
        }
    }
}

/// Drift smoke / screech and boost trails (alt mode on the ground).
#[allow(clippy::type_complexity)]
pub fn drift(
    mut commands: Commands,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    mut q: Query<(Entity, &mut TfFeel, &TransformFx, &GlobalTransform, Option<&ConstructEnd>), Without<TfLoop>>,
    mut loops: Query<&mut SfxLoopEmitter, With<TfLoop>>,
    mut bursts: MessageWriter<BurstFx>,
    mut fx: Option<ResMut<FxParticles>>,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.1);
    for (e, mut f, tfx, gt, end) in &mut q {
        let alt = alt_amount(tfx);
        let (_, rot, pos) = gt.to_scale_rotation_translation();
        let fwd = rot * Vec3::NEG_Z;
        let fwd = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
        let hv = Vec3::new(f.vel.x, 0.0, f.vel.z);
        let spd = hv.length();
        let on_ground = f.grounded && alt > 0.95 && end.is_none();
        let slide = if on_ground && spd > 7.0 && fwd != Vec3::ZERO { hv.normalize().cross(fwd).y.abs() } else { 0.0 };
        let target = ((slide - 0.22) / 0.5).clamp(0.0, 1.0) * (spd / 18.0).min(1.0);
        f.slip += (target - f.slip) * (dt * 10.0).min(1.0);
        let half = active.world.constructs.get(f.id).map_or(Vec3::splat(1.5), |c| super::super::bv(c.half));
        let right = Vec3::new(-fwd.z, 0.0, fwd.x);
        let rear = pos - fwd * half.z.max(half.x) * 0.8 + Vec3::Y * 0.2;
        // Screech (wheels only).
        let want_screech = f.voice.wheels() && f.slip > 0.05;
        match (want_screech, f.screech) {
            (true, None) => {
                let child = commands.spawn((Name::new("TfScreech"), TfLoop, Transform::default(), SfxLoopEmitter { name: "tyre_screech_loop", gain: 0.0, pitch: 1.0 })).id();
                commands.entity(e).add_child(child);
                f.screech = Some(child);
            }
            (_, Some(c)) => {
                if let Ok(mut l) = loops.get_mut(c) {
                    let g = (f.slip * 0.9).min(0.9);
                    let p = (0.85 + 0.3 * f.slip) * f.pitch().sqrt().clamp(0.8, 1.2);
                    if (l.gain - g).abs() > 0.005 || (l.pitch - p).abs() > 0.004 {
                        l.gain = g;
                        l.pitch = p;
                    }
                    if g < 0.01 && !want_screech {
                        commands.entity(c).try_despawn();
                        f.screech = None;
                    }
                } else {
                    f.screech = None;
                }
            }
            _ => {}
        }
        // Smoke / dust off the rear while sliding.
        if f.slip > 0.08 && (f.voice.wheels() || f.voice == Voice::Tread) {
            f.smoke_credit += dt * 16.0 * f.slip;
            let w = f.weight();
            while f.smoke_credit >= 1.0 {
                f.smoke_credit -= 1.0;
                let side = if f.swing_side { 1.0 } else { -1.0 };
                f.swing_side = !f.swing_side;
                let at = rear + right * side * half.x * 0.9;
                let (kind, color) = if f.voice == Voice::Tread { (BurstKind::Dust, None) } else { (BurstKind::Smoke, Some([0.82, 0.82, 0.85])) };
                bursts.write(BurstFx { kind, pos: at, dir: -hv.normalize_or_zero() * 0.4 + Vec3::Y * 0.6, scale: (0.35 + 0.35 * w).clamp(0.3, 2.0), color });
            }
        } else {
            f.smoke_credit = 0.0;
        }
        // Boost trails near top speed (tail-light streaks; jets: afterburner flame).
        let Some(fx) = fx.as_deref_mut() else { continue };
        if on_ground || (f.voice == Voice::Jet && alt > 0.95 && end.is_none()) {
            let fast = ((spd - 17.0) / 8.0).clamp(0.0, 1.0);
            if fast <= 0.0 {
                f.trail_credit = 0.0;
                continue;
            }
            f.trail_credit += dt * 30.0 * fast;
            let mut j = Jit(f.id.0.wrapping_mul(747_796_405) ^ (time.elapsed_secs() * 60.0) as u32 | 1);
            while f.trail_credit >= 1.0 {
                f.trail_credit -= 1.0;
                if f.voice == Voice::Jet {
                    let at = pos - fwd * half.z * 1.05 + Vec3::Y * half.y * 0.2;
                    fx.emit(Emit {
                        flags: PFlags::STRETCH | PFlags::PLAIN,
                        pos: at + j.dir() * 0.15,
                        vel: -fwd * j.range(10.0, 18.0) + f.vel * 0.3,
                        size: 0.12 * half.y.max(0.6),
                        color: if j.f() < 0.3 { [1.0, 0.9, 0.6] } else { [1.0, 0.45, 0.1] },
                        emissive: 5.0,
                        life: j.range(0.08, 0.16),
                        gravity: 0.0,
                        drag: 4.0,
                        spin: 3.0,
                        seed: j.0,
                        ..default()
                    });
                } else {
                    for side in [-1.0f32, 1.0] {
                        let at = rear + right * side * half.x * 0.7 + Vec3::Y * half.y * 0.3;
                        fx.emit(Emit {
                            kind: PKind::Beam,
                            flags: PFlags::PLAIN,
                            pos: at - hv * 0.02,
                            vel: -hv * 0.04,
                            size: 0.05 * half.y.max(0.5).sqrt(),
                            color: [1.0, 0.18, 0.08],
                            emissive: 3.0,
                            life: 0.2,
                            ..default()
                        });
                    }
                }
            }
        } else {
            f.trail_credit = 0.0;
        }
    }
}
