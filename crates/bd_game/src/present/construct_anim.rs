//! Construct animation by **part role × semantic event** (never by word): every construct gets a
//! `bd_shapes::ConstructAnimator` whose per-part deltas ride on top of the sim-driven clip layers
//! (bd_fx `PartOverlay` → `pose_layered_with`). Turrets track the aim, barrels recoil, wheels steer,
//! rotors spin up, hatches open for the pilot and clunk shut, heads look at the nearest enemy, eyes
//! blink, jaws roar, blades slide out — for any combination of parts (see `bd_shapes::behaviours`).
//!
//! Events are derived client-side from what is already replicated — no new network traffic:
//!
//! | event | source |
//! |---|---|
//! | Summon | a construct entity appears still forming (`AutoForm`: casts, predicted casts) |
//! | Aim | the seated pilot's look (`PlayerView::seat`), else the nearest living enemy ≤ 60 m |
//! | Fire | `ShotFired { construct }`, `HeldFx::PilotAttack`, creature `Spit` impact |
//! | Move | view speed + yaw rate from the interpolated transform |
//! | PilotApproach | the local owner is unseated, near and looking at the seat of a pilotable construct |
//! | PilotEnter / Exit | `Seated` / `Ejected` |
//! | Hit | `ConstructHit` (direction from the hit point through the construct) |
//! | Charge | owner's `pflags::CHARGING` |
//! | Attack | creature telegraphs (`CreatureAttack`), held swings (`HeldFx::SwingStart`…) |
//! | Dismiss | `ConstructEnd::Dismiss` inserted (collapse plays during bd_fx's dissolve) |
//!
//! Also here (weight pass): footfalls of walking constructs → camera shake by distance + dust +
//! a stomp cue for kaiju-sized feet. Sound hooks are `Sfx3d` cue names (missing cues are silent):
//! `assemble_click` (pitch rises per stage), `assemble_lock`, `hatch_open`, `hatch_close`,
//! `rotor_spinup`, `exhaust_puff`, `kaiju_step`, `footstep_heavy`.
//!
//! The summon assembly is exposed to other effects through `bd_fx::ConstructAssembly` (the
//! shout-words hook: letter voxels can stream into the ring beam and land as the next brick).

use super::constructs::{SimConstruct, hand_transform};
use super::sound::Sfx3d;
use super::{bq, bv, gv};
use crate::camera::PlayerCamera;
use crate::juice::CameraShakeEvent;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_core::ConstructId;
use bd_core::mechanics::{FormCategory, Mechanic};
use bd_fx::{AutoForm, BurstFx, BurstKind, ConstructAssembly, ConstructEnd, ConstructVisual, PartOverlay};
use bd_shapes::{AnimDrive, AnimEvent, AnimState, ConstructAnimator, CueKind, Footfall};
use bd_sim::GameEvent;
use bd_sim::constructs::ai::{AttackKind, Clip, phase};
use bd_sim::events::HeldFx;
use bd_sim::replicate::{cflags, pflags};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::sync::Arc;

pub struct ConstructAnimPlugin;

impl Plugin for ConstructAnimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimInbox>().add_systems(Update, (collect_events, animate_constructs).chain().after(super::PresentSet::Sync));
    }
}

/// Role-behaviour state of one construct entity.
#[derive(Component)]
pub struct ConstructAnim {
    pub anim: ConstructAnimator,
    model_ptr: usize,
    prev_yaw: Option<f32>,
    turn: f32,
    prev_dist: Option<f32>,
    dismissing: bool,
}

/// Events for constructs, gathered once per frame (entity may still be building its model).
#[derive(Resource, Default)]
struct AnimInbox {
    pending: HashMap<ConstructId, Vec<(AnimEvent, f32)>>,
}

fn yaw_of(q: Quat) -> f32 {
    let f = q * Vec3::NEG_Z;
    (-f.x).atan2(-f.z)
}

fn collect_events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut inbox: ResMut<AnimInbox>) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let mut push = |id: ConstructId, e: AnimEvent| inbox.pending.entry(id).or_default().push((e, 0.0));
    for ev in events.read() {
        match &ev.event {
            GameEvent::ShotFired { construct: Some(id), recoil, .. } => {
                push(*id, AnimEvent::Fire { strength: (recoil / 6.0).clamp(0.5, 2.5) })
            }
            GameEvent::HeldFx { id, fx, strength, .. } => match fx {
                HeldFx::PilotAttack => push(*id, AnimEvent::Fire { strength: strength.clamp(0.5, 2.5) }),
                HeldFx::SwingStart | HeldFx::Uppercut | HeldFx::Lunge | HeldFx::DiveStart => push(*id, AnimEvent::Attack { roar: false }),
                _ => {}
            },
            GameEvent::CreatureAttack { id, kind, phase: ph, .. } => {
                let k = AttackKind::from_u8(*kind);
                if *ph == phase::TELEGRAPH {
                    let roar = matches!(k.clip(), Clip::Bite | Clip::Breath | Clip::Charge | Clip::Stomp);
                    push(*id, AnimEvent::Attack { roar });
                } else if *ph == phase::IMPACT && k == AttackKind::Spit {
                    push(*id, AnimEvent::Fire { strength: 1.0 });
                }
            }
            GameEvent::ConstructHit { id, damage, point, .. } => {
                if let Some(c) = active.world.constructs.get(*id) {
                    let rot = bq(c.rot);
                    let dir = rot.inverse() * (bv(c.center()) - bv(*point)).normalize_or(Vec3::Z);
                    push(*id, AnimEvent::Hit { dir: gv(dir), strength: (damage / 30.0).clamp(0.15, 1.6) });
                }
            }
            GameEvent::Seated { construct, .. } => push(*construct, AnimEvent::PilotEnter),
            GameEvent::Ejected { construct, .. } => push(*construct, AnimEvent::PilotExit),
            _ => {}
        }
    }
    // Events for constructs that never got an entity expire.
    inbox.pending.retain(|_, v| {
        v.retain(|(_, age)| *age < 1.0);
        !v.is_empty()
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn animate_constructs(
    mut commands: Commands,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut inbox: ResMut<AnimInbox>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut q: Query<(
        Entity,
        &SimConstruct,
        &ConstructVisual,
        &Transform,
        Option<&mut ConstructAnim>,
        Option<&mut PartOverlay>,
        Option<&AutoForm>,
        Option<&ConstructEnd>,
        Option<&mut ConstructAssembly>,
    )>,
    mut sfx: MessageWriter<Sfx3d>,
    mut bursts: MessageWriter<BurstFx>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut feet: Local<Vec<Footfall>>,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.1);
    let eye = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    for v in inbox.pending.values_mut() {
        for (_, age) in v.iter_mut() {
            *age += dt;
        }
    }
    let local = active.local.and_then(|id| views.player(id));
    for (e, sc, vis, tf, anim, overlay, forming, end, assembly) in &mut q {
        let model = &vis.model;
        let ptr = Arc::as_ptr(model) as usize;
        // (Re)build the animator for this model.
        let Some(mut anim) = anim.filter(|a| a.model_ptr == ptr) else {
            let mut a = ConstructAnimator::new(model, sc.id.0.wrapping_mul(2_654_435_761));
            if forming.is_some() && end.is_none() {
                a.event(AnimEvent::Summon);
            }
            commands.entity(e).try_insert((
                ConstructAnim { anim: a, model_ptr: ptr, prev_yaw: None, turn: 0.0, prev_dist: None, dismissing: false },
                PartOverlay { locals: Vec::with_capacity(model.parts.len()) },
            ));
            continue;
        };
        let Some(mut overlay) = overlay else { continue };
        let a = &mut *anim;
        // Events.
        if let Some(list) = inbox.pending.remove(&sc.id) {
            for (ev, _) in list {
                a.anim.event(ev);
            }
        }
        if let Some(ConstructEnd::Dismiss { duration }) = end
            && !a.dismissing
        {
            a.dismissing = true;
            a.anim.event(AnimEvent::Dismiss { duration: *duration });
        }
        // Drive.
        let cv = views.construct(sc.id);
        let c = active.world.constructs.get(sc.id);
        let owner = views.player(sc.owner);
        let (t, speed, dist, airborne) = match vis.layers.as_slice().first() {
            Some((AnimState::Locomotion { speed, distance, airborne }, t)) => (*t, *speed, *distance, *airborne),
            Some((_, t)) => (*t, 0.0, 0.0, false),
            None => (0.0, 0.0, 0.0, false),
        };
        let yaw = yaw_of(tf.rotation);
        if let Some(py) = a.prev_yaw
            && dt > 0.0
        {
            let d = (yaw - py + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            a.turn += ((d / dt).clamp(-12.0, 12.0) - a.turn) * (dt * 10.0).min(1.0);
        }
        a.prev_yaw = Some(yaw);
        let inv = tf.compute_affine().inverse();
        let to_model = |p: Vec3| inv.transform_point3(p);
        let rot_inv = tf.rotation.inverse();
        let attached = cv.is_some_and(|v| v.attached());
        let piloted = cv.is_some_and(|v| v.flags & cflags::PILOTED != 0);
        let height = (model.bounds.max.y - model.bounds.min.y) * tf.scale.y;
        let chest = tf.translation + Vec3::Y * (height * 0.6);
        // Aim: the pilot's look, else the nearest living enemy.
        let pilot = views.players.iter().find(|p| p.seat.is_some_and(|(id, _)| id == sc.id));
        let mut aim = None;
        let mut look = None;
        if !attached {
            if let Some(p) = pilot {
                let dir = Quat::from_euler(EulerRot::YXZ, p.yaw, p.pitch, 0.0) * Vec3::NEG_Z;
                aim = Some(rot_inv * dir);
                look = Some(to_model(chest + dir * 20.0));
            } else {
                let target = views
                    .players
                    .iter()
                    .filter(|p| p.id != sc.owner && p.alive())
                    .map(|p| (p.pos + Vec3::Y * 1.2 * p.size, p.pos.distance_squared(tf.translation)))
                    .filter(|(_, d2)| *d2 < 60.0 * 60.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(p, _)| p);
                if let Some(tp) = target {
                    aim = Some(rot_inv * (tp - chest).normalize_or(Vec3::NEG_Z));
                    look = Some(to_model(tp));
                }
            }
        }
        // Pilot approach: the local owner, unseated, close to and looking at the seat.
        let pilotable = model.seat.is_some() || c.is_some_and(|c| c.recipe.has(Mechanic::Piloted));
        let mut pilot_near = false;
        if pilotable
            && !piloted
            && let Some(lp) = local
            && lp.id == sc.owner
            && lp.seat.is_none()
            && lp.alive()
        {
            let seat = model.seat.map(|s| tf.transform_point(Vec3::from_array(s.pos.to_array()))).unwrap_or(chest);
            let leye = lp.pos + Vec3::Y * 1.6 * lp.size;
            let to = seat - leye;
            let reach = (0.35 * height).max(3.5);
            let dir = Quat::from_euler(EulerRot::YXZ, lp.yaw, lp.pitch, 0.0) * Vec3::NEG_Z;
            pilot_near = to.length() <= reach && dir.dot(to.normalize_or(Vec3::Y)) > 0.45;
        }
        let charge = owner.map_or(0.0, |o| if o.has(pflags::CHARGING) { 1.0 } else { 0.0 });
        let ring = owner.map(|o| to_model(hand_transform(o).translation));
        let drive = AnimDrive {
            time: t,
            speed,
            turn: a.turn,
            airborne,
            aim: aim.map(gv),
            look: look.map(gv),
            pilot_near,
            piloted,
            charge,
            ring: ring.map(gv),
        };
        a.anim.update(&drive, dt);
        overlay.locals.clear();
        overlay.locals.extend_from_slice(a.anim.locals());
        // Cues → sound / particles (world space).
        let world = |p: bd_core::Vec3| tf.transform_point(bv(p));
        for cue in a.anim.cues() {
            let p = world(cue.pos);
            match cue.kind {
                CueKind::AssembleClick => {
                    let pitch = 0.85 + 0.12 * cue.stage as f32;
                    sfx.write(Sfx3d::at("assemble_click", p).pitch(pitch).gain((0.5 + 0.25 * cue.strength).min(1.0)));
                }
                CueKind::AssembleLock => {
                    sfx.write(Sfx3d::at("assemble_lock", p).gain((0.6 + 0.3 * cue.strength).min(1.2)));
                }
                CueKind::HatchOpen => {
                    sfx.write(Sfx3d::at("hatch_open", p).gain(0.7));
                }
                CueKind::HatchClose => {
                    sfx.write(Sfx3d::at("hatch_close", p).gain((0.5 + 0.4 * cue.strength).min(1.2)));
                }
                CueKind::RotorSpinUp => {
                    sfx.write(Sfx3d::at("rotor_spinup", p).gain(0.7));
                }
                CueKind::ExhaustPuff => {
                    let dir = (tf.rotation * bv(cue.dir)).normalize_or(Vec3::Y);
                    bursts.write(BurstFx {
                        kind: BurstKind::Smoke,
                        pos: p,
                        dir,
                        scale: (0.4 + 0.6 * cue.strength) * tf.scale.x.clamp(0.3, 4.0),
                        color: None,
                    });
                    sfx.write(Sfx3d::at("exhaust_puff", p).gain(0.35 + 0.4 * cue.strength));
                }
            }
        }
        // Assembly hook for other effects (shout words).
        let asm = a.anim.assembly();
        if asm.active {
            let st = ConstructAssembly {
                progress: asm.progress,
                stage: asm.stage,
                stages: asm.stages,
                ring: ring.map(|r| tf.transform_point(r)).unwrap_or(chest),
                next_world: asm.next.map(|(p, _)| world(p)),
                next_in: asm.next.map_or(0.0, |(_, s)| s),
                total: a.anim.assembly_seconds(),
            };
            match assembly {
                Some(mut s) => *s = st,
                None => {
                    commands.entity(e).try_insert(st);
                }
            }
        } else if assembly.is_some() {
            commands.entity(e).remove::<ConstructAssembly>();
        }
        // Footfalls (weight pass): big feet shake the camera by distance and kick up dust.
        if sc.category == FormCategory::Creature && !attached && end.is_none() {
            if let Some(d0) = a.prev_dist {
                feet.clear();
                // Same (speed, distance) the pose layers use, so the event lines up with the visible step.
                bd_shapes::footfalls(model, speed, d0, dist, &mut feet);
                for f in feet.iter() {
                    let impact = f.impact * tf.scale.x.clamp(0.2, 20.0);
                    if impact < 0.8 {
                        continue;
                    }
                    let p = tf.transform_point(bv(f.pos));
                    bursts.write(BurstFx {
                        kind: BurstKind::Dust,
                        pos: p,
                        dir: Vec3::Y,
                        scale: (impact * 0.35).clamp(0.4, 3.0),
                        color: None,
                    });
                    if impact >= 2.5 {
                        sfx.write(Sfx3d::at("kaiju_step", p).gain((0.4 + 0.12 * impact).min(1.3)));
                        let k = (1.0 - eye.distance(p) / (25.0 + 6.0 * impact)).clamp(0.0, 1.0);
                        if k > 0.0 {
                            shake.write(CameraShakeEvent { trauma: (0.08 * impact * k).min(0.45), dir: Vec3::NEG_Y });
                        }
                    } else {
                        sfx.write(Sfx3d::at("footstep_heavy", p).gain(0.25 + 0.15 * impact));
                    }
                }
            }
            a.prev_dist = Some(dist);
        }
    }
}
