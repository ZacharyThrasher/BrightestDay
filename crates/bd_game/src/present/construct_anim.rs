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
//! | PilotApproach | the local owner could board it right now (`World::board_candidate`: own, boardable, within reach, looked at) |
//! | PilotEnter / Exit | `Seated` / `Ejected` |
//! | Hit | `ConstructHit` (direction from the hit point through the construct) |
//! | Charge | owner's `pflags::CHARGING` |
//! | Attack | creature telegraphs (`CreatureAttack`), held swings (`HeldFx::SwingStart`…) |
//! | Dismiss | `ConstructEnd::Dismiss` inserted (collapse plays during bd_fx's dissolve) |
//!
//! Also here (PILOTING teach-by-feel): the construct the Transform key would board right now gets a
//! soft pulsing glow on its seat / cockpit ([`BoardGlow`]; no text) — exactly the sim's
//! `World::board_candidate`, so the glow and the key always agree.
//!
//! Also here (weight pass): footfalls of walking constructs → camera shake by distance + dust +
//! a stomp cue for kaiju-sized feet. Sound hooks are `Sfx3d` cue names (missing cues are silent):
//! `assemble_click` (pitch rises per stage), `assemble_lock`, `hatch_open`, `hatch_close`,
//! `rotor_spinup`, `exhaust_puff`, `kaiju_step`, `footstep_heavy`; plus a `rotor_loop`
//! [`SfxLoopEmitter`] on a child of every construct with spinning rotors (gain / pitch follow the
//! spin, so it swells in behind `rotor_spinup`; cut on dismiss like the other construct loops).
//!
//! The summon assembly is exposed to other effects through `bd_fx::ConstructAssembly` (the
//! shout-words hook: letter voxels can stream into the ring beam and land as the next brick).

use super::constructs::{SimConstruct, hand_transform};
use super::sound::{Sfx3d, SfxLoopEmitter};
use super::{bq, bv, gv};
use crate::camera::PlayerCamera;
use crate::juice::CameraShakeEvent;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_core::ConstructId;
use bd_core::mechanics::FormCategory;
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
        scenario::build(app); // `--scenario piloting` (dev frames, docs/piloting/)
        app.init_resource::<AnimInbox>()
            .init_resource::<BoardGlow>()
            .init_resource::<FocusDesat>()
            .add_systems(Update, (collect_events, animate_constructs, board_glow).chain().after(super::PresentSet::Sync))
            .add_systems(Update, (focus_desat, archetype_fx).after(super::PresentSet::Sync));
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
    /// Child entity carrying the `rotor_loop` voice.
    rotor_loop: Option<Entity>,
    /// Where the rotors are (model space; from the spin-up cue, else the model centre).
    rotor_at: Option<Vec3>,
}

/// Marks the `rotor_loop` child of a construct.
#[derive(Component)]
struct RotorLoop;

/// Events for constructs, gathered once per frame (entity may still be building its model).
#[derive(Resource, Default)]
struct AnimInbox {
    pending: HashMap<ConstructId, Vec<(AnimEvent, f32)>>,
}

/// `rotor_loop` level / pitch for a rotor spin factor (0 = silent at rest).
fn rotor_voice(spin: f32) -> SfxLoopEmitter {
    let s = spin.clamp(0.0, 1.5);
    SfxLoopEmitter { name: "rotor_loop", gain: 0.55 * (s / 0.55).min(1.0) * (0.8 + 0.2 * s), pitch: 0.7 + 0.4 * s }
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
    mut loops: Query<&mut SfxLoopEmitter, With<RotorLoop>>,
    mut feet: Local<Vec<Footfall>>,
    mut glow: ResMut<BoardGlow>,
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
    // PILOTING: the one construct the Transform key would board (same rule as the host).
    let board_target = active.local.and_then(|id| active.world.board_candidate(id));
    glow.at = None;
    for (e, sc, vis, tf, anim, overlay, forming, end, assembly) in &mut q {
        let model = &vis.model;
        let ptr = Arc::as_ptr(model) as usize;
        // (Re)build the animator for this model.
        if let Some(stale) = anim.as_ref().filter(|a| a.model_ptr != ptr).and_then(|a| a.rotor_loop) {
            commands.entity(stale).try_despawn();
        }
        let Some(mut anim) = anim.filter(|a| a.model_ptr == ptr) else {
            let mut a = ConstructAnimator::new(model, sc.id.0.wrapping_mul(2_654_435_761));
            if forming.is_some() && end.is_none() {
                a.event(AnimEvent::Summon);
            }
            commands.entity(e).try_insert((
                ConstructAnim { anim: a, model_ptr: ptr, prev_yaw: None, turn: 0.0, prev_dist: None, dismissing: false, rotor_loop: None, rotor_at: None },
                PartOverlay { locals: Vec::with_capacity(model.parts.len()) },
            ));
            continue;
        };
        let Some(mut overlay) = overlay else { continue };
        let a = &mut *anim;
        // A shout asked the later bricks to wait for its letters (bd_fx shout ↔ assembly hook).
        if let Some(asm) = assembly.as_deref() {
            a.anim.hold_stages(asm.hold_request);
        }
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
        // Pilot approach: the local owner could board it right now (hatches open, the seat glows).
        let pilot_near = board_target == Some(sc.id) && local.is_some_and(|lp| lp.id == sc.owner && lp.alive());
        if pilot_near {
            let seat = model.seat.map(|s| tf.transform_point(Vec3::from_array(s.pos.to_array()))).unwrap_or(chest);
            glow.at = Some((seat, (height * 0.12).clamp(0.35, 1.6)));
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
                    a.rotor_at = Some(bv(cue.pos));
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
        // Rotor loop: follows the spin (swells in behind `rotor_spinup`), cut on dismiss.
        match (a.anim.rotor_speed().filter(|_| end.is_none()), a.rotor_loop) {
            (Some(spin), None) if spin > 0.05 => {
                let at = a.rotor_at.unwrap_or_else(|| (bv(model.bounds.min) + bv(model.bounds.max)) * 0.5);
                let child = commands.spawn((Name::new("RotorLoop"), RotorLoop, Transform::from_translation(at), rotor_voice(spin))).id();
                commands.entity(e).add_child(child);
                a.rotor_loop = Some(child);
            }
            (Some(spin), Some(child)) => {
                if let Ok(mut l) = loops.get_mut(child) {
                    let v = rotor_voice(spin);
                    if (l.gain - v.gain).abs() > 0.01 || (l.pitch - v.pitch).abs() > 0.005 {
                        l.gain = v.gain;
                        l.pitch = v.pitch;
                    }
                }
            }
            (None, Some(child)) => {
                commands.entity(child).try_despawn();
                a.rotor_loop = None;
            }
            _ => {}
        }
        // Assembly hook for other effects (shout words).
        let asm = a.anim.assembly();
        if asm.active {
            let sched = a.anim.stage_schedule();
            let scale = tf.scale.max_element().clamp(0.05, 50.0);
            let st = ConstructAssembly {
                slots: sched.map(|(p, _, _)| world(p)),
                land_in: sched.map(|(_, s, _)| s),
                radius: sched.map(|(_, _, r)| r * scale),
                progress: asm.progress,
                stage: asm.stage,
                stages: asm.stages,
                ring: ring.map(|r| tf.transform_point(r)).unwrap_or(chest),
                next_world: asm.next.map(|(p, _)| world(p)),
                next_in: asm.next.map_or(0.0, |(_, s)| s),
                total: a.anim.assembly_seconds(),
                // Consumed above (hold_stages); requesters write it again each frame.
                hold_request: 0.0,
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

// ------------------------------------------------------------------------------------------ board glow

/// PILOTING teach-by-feel: where the "you can get in" glow sits this frame (seat / cockpit, world
/// space) and how big it is; `None` = nothing to board. Written by `animate_constructs`.
#[derive(Resource, Default)]
pub struct BoardGlow {
    pub at: Option<(Vec3, f32)>,
    level: f32,
    t: f32,
    entity: Option<Entity>,
    mat: Option<Handle<StandardMaterial>>,
}

#[derive(Component)]
struct BoardGlowMark;

/// A soft additive orb on the seat that breathes while you could board (fades in / out; no text).
fn board_glow(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut glow: ResMut<BoardGlow>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&mut Transform, &mut Visibility), With<BoardGlowMark>>,
) {
    let dt = time.delta_secs().min(0.1);
    glow.t += dt;
    let want = if glow.at.is_some() { 1.0 } else { 0.0 };
    glow.level += (want - glow.level) * (dt * 8.0).min(1.0);
    if glow.entity.is_none_or(|e| q.get(e).is_err()) {
        if glow.at.is_none() {
            return; // spawn lazily, the first time there is something to board
        }
        let mesh = meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(16, 12)));
        let mat = mats.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.9, 1.0, 0.0),
            emissive: LinearRgba::rgb(0.0, 0.0, 0.0),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        });
        let e = commands.spawn((BoardGlowMark, Mesh3d(mesh), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster)).id();
        glow.entity = Some(e);
        glow.mat = Some(mat);
        return;
    }
    let (Some(e), Some(mat)) = (glow.entity, glow.mat.clone()) else { return };
    let Ok((mut tf, mut vis)) = q.get_mut(e) else { return };
    let level = glow.level;
    if level < 0.02 {
        *vis = Visibility::Hidden;
        return;
    }
    if let Some((at, size)) = glow.at {
        tf.translation = at;
        tf.scale = Vec3::splat(size * (0.92 + 0.08 * (glow.t * 4.0).sin()));
    }
    *vis = Visibility::Visible;
    if let Some(mut m) = mats.get_mut(&mat) {
        let a = level * (0.22 + 0.1 * (glow.t * 4.0).sin().abs());
        m.base_color = Color::srgba(0.55, 0.9, 1.0, a);
        m.emissive = LinearRgba::rgb(0.9 * a, 2.2 * a, 3.0 * a);
    }
}

// ------------------------------------------------------------------------------------------ transformer combat

/// TRANSFORMER COMBAT "focus" (Devastation's perfect-dodge feel WITHOUT slowing anyone): a perfect
/// dodge (`HeldFx::Parry`, strength ≥ 2, the local pilot's transformer) desaturates the dodger's
/// view for ~0.5 s (the construct's own chrome flash is `constructs.rs`'s Parry flash). Presentation
/// only; comfort: off when flashes are off, scaled by the flashes setting. Restores the base grade.
#[derive(Resource, Default)]
pub struct FocusDesat {
    t: f32,
    written: bool,
}

const FOCUS_SECS: f32 = 0.5;

fn focus_desat(
    time: Res<Time<Real>>,
    comfort: Res<crate::settings::ComfortSettings>,
    active: Option<Res<ActiveWorld>>,
    mut events: MessageReader<GameEvents>,
    mut st: ResMut<FocusDesat>,
    mut cams: Query<&mut bevy::render::view::ColorGrading, With<PlayerCamera>>,
) {
    let local = active.as_ref().and_then(|a| a.local);
    for ev in events.read() {
        if let GameEvent::HeldFx { fx: HeldFx::Parry, owner, strength, .. } = &ev.event
            && *strength >= 2.0
            && local == Some(*owner)
        {
            st.t = FOCUS_SECS;
        }
    }
    if st.t <= 0.0 {
        if st.written {
            st.written = false;
            for mut cg in &mut cams {
                cg.global.post_saturation = 1.04;
            }
        }
        return;
    }
    st.t = (st.t - time.delta_secs()).max(0.0);
    let k = comfort.flashes.clamp(0.0, 1.0);
    if k <= 0.01 {
        return;
    }
    let u = st.t / FOCUS_SECS;
    // Snap grey, ease back.
    let dip = if u > 0.85 { (1.0 - u) / 0.15 } else { u / 0.85 };
    for mut cg in &mut cams {
        cg.global.post_saturation = 1.04 * (1.0 - 0.7 * k * dip);
    }
    st.written = true;
}

/// Archetype flavour for transformer combat moments (combat transforms, drift boosts): tyre smoke /
/// exhaust / tread dust, jet afterburner flash, rotor wash (combat.ron `transformer.archetypes[].fx`,
/// the same archetype the host fights with).
fn archetype_fx(active: Option<Res<ActiveWorld>>, mut events: MessageReader<GameEvents>, mut bursts: MessageWriter<BurstFx>) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    for ev in events.read() {
        let GameEvent::HeldFx { id, fx, pos, dir, strength, .. } = &ev.event else { continue };
        let combat = match fx {
            HeldFx::Transform => strength.abs() > 1.2,
            HeldFx::Thrust => true,
            _ => false,
        };
        if !combat {
            continue;
        }
        let Some(c) = active.world.constructs.get(*id).filter(|c| c.recipe.has(bd_core::mechanics::Mechanic::Transforms)) else { continue };
        let a = active.world.cfg.tuning.transformer.archetype_of(&c.recipe);
        let size = (c.half.max_element() * 0.6).clamp(0.6, 4.0);
        let at = bv(*pos) - Vec3::Y * bv(c.half).y * 0.6;
        let d = bv(*dir);
        let (kind, color) = match a.fx.as_str() {
            "afterburner" => (BurstKind::Flash, Some([1.0, 0.55, 0.15])),
            "rotor_wash" => (BurstKind::Dust, Some([0.8, 0.85, 0.9])),
            "tread_dust" => (BurstKind::Dust, None),
            "exhaust" => (BurstKind::Smoke, Some([0.25, 0.25, 0.28])),
            _ => (BurstKind::Smoke, None),
        };
        bursts.write(BurstFx { kind, pos: at, dir: -d, scale: size, color });
        if a.fx == "afterburner" {
            bursts.write(BurstFx { kind: BurstKind::Element(bd_core::mechanics::Element::Fire), pos: bv(*pos), dir: -d, scale: size * 0.7, color: None });
        }
    }
}

// ------------------------------------------------------------------------------------------ scenario

/// `--scenario piloting --pilot-shot truck|transform|robot|starscream|board` (practice): dev frames
/// of piloting (docs/piloting/). You cast the real lexicon's "optimus prime" (default-piloted: you
/// spawn seated in the truck) or "starscream" on open ground; bots stop thinking and the others are
/// benched; the script drives your input. `transform`: a kick off the ground, then the Transform key
/// mid-air (robot + landing slam); `robot`: transformed, walking; `board`: a follower Optimus you walk
/// up to (the seat glows), then board with the Transform key.
pub mod scenario {
    use crate::console::RingLexicon;
    use crate::input::{InputOverride, LookAngles};
    use crate::sim_bridge::ActiveWorld;
    use bd_core::input::buttons;
    use bd_core::{ConstructId, PlayerId, Vec3};
    use bevy::ecs::schedule::common_conditions::resource_exists;
    use bevy::prelude::{App, IntoScheduleConfigs, Res, ResMut, Resource, Time, Update, info, warn};
    use bevy::time::Real;

    #[derive(Resource, Clone, Debug, Default)]
    pub struct PilotScenario {
        shot: String,
        t: f32,
        placed: bool,
        id: Option<ConstructId>,
        dir: Vec3,
        step: u32,
        /// Combat shots: the enemy dummy standing in front.
        dummy: Option<PlayerId>,
    }

    /// Combat frame-strip shots (TRANSFORMER COMBAT): the phrase each one casts.
    fn combat_phrase(shot: &str) -> Option<&'static str> {
        Some(match shot {
            "combo" => "optimus prime",
            "flyby" => "starscream",
            "tankcombo" => "a transforming tank",
            "helicombo" => "a transforming helicopter",
            "bikecombo" => "arcee",
            _ => return None,
        })
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "piloting") {
            return;
        }
        let shot = args.windows(2).find(|w| w[0] == "--pilot-shot").map(|w| w[1].clone()).unwrap_or_else(|| "truck".into());
        info!("piloting scenario: {shot}");
        app.insert_resource(PilotScenario { shot, ..Default::default() }).add_systems(Update, script.run_if(resource_exists::<ActiveWorld>));
    }

    /// The most open, flat ground near `me`: a spot with a clear lane (samples every 4 m for 60 m,
    /// 6 m wide, within 2 m of the start height, nothing overhead) — (stand-at, lane direction).
    fn stage(w: &bd_sim::World, me: Vec3) -> Option<(Vec3, Vec3)> {
        let top = |x: f32, z: f32| bd_core::voxel::raycast(&w.voxels, Vec3::new(x, 150.0, z), Vec3::NEG_Y, 170.0).map(|h| h.point.y);
        let mut cands: Vec<Vec3> = vec![me];
        for r in [12.0f32, 24.0, 36.0] {
            for k in 0..16 {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                cands.push(me + Vec3::new(a.cos() * r, 0.0, a.sin() * r));
            }
        }
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
                    for o in [-3.0f32, 0.0, 3.0] {
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

    fn face(look: &mut LookAngles, d: Vec3, pitch: f32) {
        look.yaw = (-d.x).atan2(-d.z);
        look.pitch = pitch;
    }

    fn cast(active: &mut ActiveWorld, lex: &bd_lexicon::Lexicon, me: PlayerId, phrase: &str, dir: Vec3) -> Option<ConstructId> {
        let it = lex.intent(phrase, &[], false);
        let r = it.resolution;
        info!("piloting scenario: \"{phrase}\" -> {} pilot {:?}", r.signature(), it.pilot);
        let eye = active.world.player(me).map(|p| p.eye(active.world.params()))?;
        match active.world.cast_shaped(me, r.recipe, r.shape, r.alt_shape, bd_sim::Aim::new(eye, (dir + Vec3::new(0.0, -0.35, 0.0)).normalize_or_zero())) {
            Ok(id) => {
                if let Some(c) = active.world.constructs.get_mut(id) {
                    c.life = 1e6;
                }
                Some(id)
            }
            Err(e) => {
                warn!("piloting scenario: cast failed {e:?}");
                None
            }
        }
    }

    fn script(
        time: Res<Time<Real>>,
        lex: Option<Res<RingLexicon>>,
        mut sc: ResMut<PilotScenario>,
        mut active: ResMut<ActiveWorld>,
        mut look: ResMut<LookAngles>,
        mut ov: ResMut<InputOverride>,
        cam: bevy::prelude::Query<&bevy::prelude::GlobalTransform, bevy::prelude::With<crate::camera::PlayerCamera>>,
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
                    warn!("piloting scenario: no stage found");
                    sc.placed = true;
                }
                return;
            };
            active.bots.clear();
            active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Infinite;
            let others: Vec<PlayerId> = active.world.players.iter().map(|p| p.id).filter(|id| *id != me).collect();
            let combat = combat_phrase(&sc.shot).is_some();
            for (k, id) in others.into_iter().enumerate() {
                if combat && k == 0 {
                    sc.dummy = Some(id); // the combat shots keep one enemy standing still as the target
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
            face(&mut look, dir, -0.12);
            let phrase = match sc.shot.as_str() {
                s if combat_phrase(s).is_some() => combat_phrase(s).unwrap_or("optimus prime"),
                "starscream" => "starscream",
                "board" => "optimus prime that follows me",
                _ => "optimus prime",
            };
            sc.id = cast(&mut active, &lex.0, me, phrase, dir);
            sc.dir = dir;
            sc.placed = true;
            sc.t = 1.0;
            return;
        }
        let Some(id) = sc.id.filter(|id| active.world.constructs.get(*id).is_some()) else { return };
        let t = sc.t;
        if (t * 4.0).floor() != ((t - dt) * 4.0).floor() {
            if let Some(c) = active.world.constructs.get(id) {
                let seat = active.world.player(me).and_then(|p| p.seat).map(|s| s.construct);
                let cam_d = cam.single().map(|g| (crate::util::core_v3(g.translation()) - c.center()).length()).unwrap_or(-1.0);
                info!("piloting scenario t={t:.1}: pos {:?} vel {:?} vehicle {} xform {:?} seat {:?} grounded {} half {:?} cam {cam_d:.1} m xatk {:?} step {} hits {}", c.pos, c.vel, c.held.vehicle, c.comp.xform.mode, seat, c.grounded, c.half, c.brawl.xatk, c.brawl.step, c.brawl.last_xattack_hits);
            }
        }
        ov.active = true;
        ov.held = 0;
        ov.move_x = 0;
        ov.move_z = 0;
        let dir = sc.dir;
        match sc.shot.as_str() {
            // Cruise down the lane in the truck (third person).
            "truck" => {
                face(&mut look, dir, -0.5);
                ov.move_z = if t > 1.6 { 1 } else { 0 };
            }
            // Drive, get kicked into the air, press Transform mid-jump: the robot lands in a slam.
            "transform" => {
                face(&mut look, dir, -0.5);
                ov.move_z = 1;
                if t > 2.4 && sc.step == 0 {
                    sc.step = 1;
                    if let Some(c) = active.world.constructs.get_mut(id) {
                        c.vel.y = 11.0;
                        c.grounded = false;
                    }
                }
                if t > 2.65 && sc.step == 1 {
                    sc.step = 2;
                    active.world.toggle_transform(me);
                }
            }
            // Transformed on the ground, then walking the robot.
            "robot" => {
                face(&mut look, dir, -0.45);
                if t > 1.4 && sc.step == 0 {
                    sc.step = 1;
                    active.world.toggle_transform(me);
                }
                ov.move_z = if t > 2.8 { 1 } else { 0 };
            }
            // The jet: throttle up and climb along the lane.
            "starscream" => {
                face(&mut look, dir, if t < 2.0 { 0.0 } else { 0.28 });
                ov.move_z = 1;
            }
            // Stand beside your follower Optimus looking at it (the seat glows), then board it with the
            // Transform key.
            "board" => {
                let Some(c) = active.world.constructs.get(id) else { return };
                let (center, half, cpos, grounded) = (c.center(), c.half, c.pos, c.grounded || c.body.supported);
                if let Some(c) = active.world.constructs.get_mut(id) {
                    c.vel.x = 0.0;
                    c.vel.z = 0.0;
                }
                if sc.step == 0 && t > 2.2 && grounded {
                    sc.step = 1;
                    let side = Vec3::new(dir.z, 0.0, -dir.x);
                    let at = cpos + side * (half.x + 1.6);
                    let gy = bd_core::voxel::raycast(&active.world.voxels, Vec3::new(at.x, cpos.y + 6.0, at.z), Vec3::NEG_Y, 20.0).map_or(cpos.y, |h| h.point.y);
                    if let Some(p) = active.world.player_mut(me) {
                        p.mv.pos = Vec3::new(at.x, gy + 0.05, at.z);
                        p.mv.vel = Vec3::ZERO;
                    }
                }
                let eye = active.world.player(me).map(|p| p.eye(active.world.params())).unwrap_or(my_pos);
                let to = center - eye;
                look.yaw = (-to.x).atan2(-to.z);
                look.pitch = (to.y / to.length().max(0.1)).asin();
                if t > 4.8 && sc.step == 1 {
                    sc.step = 2;
                    let ok = active.world.toggle_transform(me);
                    info!("piloting scenario: board key -> {ok}");
                }
            }
            // TRANSFORMER COMBAT strips: robot mode, a dummy in front, a light string, then Transform
            // (the archetype's transform attack) — the finisher follows by itself.
            s if combat_phrase(s).is_some() => {
                face(&mut look, dir, -0.18);
                let Some(dummy) = sc.dummy else { return };
                let (center, half, robot) = {
                    let Some(c) = active.world.constructs.get(id) else { return };
                    (c.center(), c.half, !c.held.vehicle)
                };
                if t > 1.2 && sc.step == 0 {
                    sc.step = 1;
                    if !robot {
                        active.world.toggle_transform(me);
                    }
                }
                // The dummy stands in front of the robot until the transform attack starts.
                let attacking = active.world.constructs.get(id).is_some_and(|c| c.brawl.xatk != bd_sim::constructs::brawl::XPhase::None);
                if t > 2.4 && !attacking && sc.step < 3 {
                    let at = center + dir * (half.x.max(half.z) + 1.6);
                    let gy = bd_core::voxel::raycast(&active.world.voxels, Vec3::new(at.x, center.y + 6.0, at.z), Vec3::NEG_Y, 30.0).map_or(my_pos.y, |h| h.point.y);
                    if let Some(p) = active.world.player_mut(dummy) {
                        p.alive = true;
                        p.mv.pos = Vec3::new(at.x, gy + 0.05, at.z);
                        p.mv.vel = Vec3::ZERO;
                        p.damage_pct = 0.0;
                        p.spawn_shield = 0.0;
                    }
                }
                // Four taps of LMB (0.3 s apart), then Transform.
                let taps = [2.6f32, 2.9, 3.2, 3.5];
                ov.held = if taps.iter().any(|&k| t > k && t < k + 0.05) { buttons::PRIMARY } else { 0 };
                if t > 3.65 && sc.step == 1 {
                    sc.step = 3;
                    let ok = active.world.toggle_transform(me);
                    info!("piloting scenario: transform attack -> {ok}");
                }
            }
            _ => {}
        }
    }
}
