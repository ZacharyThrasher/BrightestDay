//! Palm energy (`Form::Hands`: kamehameha, final flash, galick gun, repulsor…) — energy attacks
//! never look like guns (owner: "final flash gave me a super powered gun, not the iconic beam").
//!
//! * **No weapon model**: the Hands construct's own voxel visual is hidden. What shows is a glowing
//!   orb between the caster's palms in the ELEMENT colour (Light = ki blue, Electric = yellow-white,
//!   Psychic = purple…): a small ember while held, GROWING as PRIMARY charges it (the replicated
//!   weapon byte, so everyone sees it), with a light that lights the hands.
//! * **Where**: first person, between the two viewmodel palms (`ViewmodelHands`, this frame's camera);
//!   everyone else, in front of the body's hands (`BodyHands`; `present::players` poses both arms).
//! * **Release** (`GameEvent::Hitscan` of a Hands holder; the sim fires on RELEASE or at full charge):
//!   a THICK sustained beam from the palms to where it struck — a cone flaring out of the hands, then a
//!   white-hot core in a coloured glow — for 0.55 s (plain) … 1.0 s (an ultimate's caliber), width by
//!   caliber (`sqrt(scale / default)`), snapping on and tapering off, sparks streaming along it and a
//!   flare where it hits. Its root follows the hands; its end stays on the struck point.
//! * **Juice**: the caster's view kicks (viewmodel palm thrust), camera shake and an FOV punch
//!   (`juice` applies the comfort settings); the beam's brightness follows comfort "flashes"; nearby
//!   players feel a distance-scaled shake. (No gamepad rumble API exists; the shake is the rumble.)
//!
//! Cost: 3 entities per orb (core, glow, light), 4 per live beam, bounded particles.

use super::constructs::SimConstruct;
use super::players::BodyHands;
use super::signature::element_rgb;
use super::{bv, corps_rgb};
use crate::camera::PlayerCamera;
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use crate::viewmodel::{HandsPublishSet, ViewmodelHands};
use bd_core::mechanics::{Element, Form};
use bd_core::{ConstructId, PlayerId};
use bd_fx::{Emit, FxParticles, PFlags, PKind};
use bd_sim::GameEvent;
use bd_sim::weapons::WeaponPhase;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// Energy colour of an element (linear RGB). Light is ki blue (not the corps colour: the beam reads
/// as energy, the orb is the caster's own); the rest follow the signature palette.
pub fn energy_rgb(e: Element, corps: [f32; 3]) -> [f32; 3] {
    match e {
        Element::Light => [0.35, 0.7, 1.0],
        Element::Electric => [1.0, 0.95, 0.55],
        Element::Shadow => [0.45, 0.1, 0.8],
        _ => element_rgb(e, corps),
    }
}

/// Beam look from the Hands construct's scale: (caliber 1..2.5, radius m, seconds).
pub fn beam_look(scale: f32) -> (f32, f32, f32) {
    let d = bd_sim::constructs::forms::form_spec(Form::Hands).default_scale.max(0.1);
    let k = if scale.is_finite() { (scale / d).sqrt().clamp(1.0, 2.5) } else { 1.0 };
    let t = (k - 1.0) / 1.5;
    (k, 0.22 + 0.9 * t, 0.55 + 0.45 * t)
}

/// Orb radius for a charge level 0..1 (1P = viewmodel scale, else world scale).
pub fn orb_radius(charge: f32, caliber: f32, first_person: bool) -> f32 {
    let c = if charge.is_finite() { charge.clamp(0.0, 1.0) } else { 0.0 };
    if first_person { 0.012 + 0.05 * c * (0.7 + 0.3 * caliber) } else { (0.06 + 0.3 * c) * caliber.sqrt() }
}

/// Width envelope of a beam at `t` of `dur`: snaps on (60 ms), holds with a flicker, tapers the last 35 %.
pub fn beam_envelope(t: f32, dur: f32) -> f32 {
    if !(t.is_finite() && dur > 0.0) || t >= dur {
        return 0.0;
    }
    let on = (t / 0.06).min(1.0);
    let off = ((dur - t) / (dur * 0.35)).min(1.0);
    on * off.powf(0.7) * (1.0 + 0.06 * (t * 55.0).sin())
}

struct Orb {
    id: ConstructId,
    owner: PlayerId,
    color: [f32; 3],
    caliber: f32,
    /// Smoothed charge 0..1 and "beam live" glow.
    charge: f32,
    core: Entity,
    glow: Entity,
    light: Entity,
    mats: [Handle<StandardMaterial>; 2],
    pos: Vec3,
    seen: bool,
}

struct Beam {
    owner: PlayerId,
    construct: ConstructId,
    end: Vec3,
    color: [f32; 3],
    radius: f32,
    dur: f32,
    t: f32,
    parts: [Entity; 4],
    mats: [Handle<StandardMaterial>; 3],
    spark: f32,
    ring: f32,
    mine: bool,
}

#[derive(Resource, Default)]
struct PalmFx {
    orbs: Vec<Orb>,
    beams: Vec<Beam>,
    /// Beams to start: (owner, construct, end, radius, secs, colour).
    queue: Vec<(PlayerId, ConstructId, Vec3, f32, f32, [f32; 3])>,
}

#[derive(Resource)]
struct PalmAssets {
    sphere: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    cone: Handle<Mesh>,
}

#[derive(Component)]
struct PalmPart;

pub struct PalmBeamPlugin;

impl Plugin for PalmBeamPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<GameEvents>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .init_resource::<PalmFx>()
            .init_resource::<ViewmodelHands>()
            .init_resource::<BodyHands>()
            .add_systems(Startup, assets)
            .add_systems(
                Update,
                (
                    palm_events.in_set(super::PresentSet::Events),
                    // After the construct sync (it rewrites visibility every frame).
                    track_palms.after(super::PresentSet::Sync),
                ),
            )
            // After the camera + the viewmodel hands are placed this frame.
            .add_systems(PostUpdate, place_palms.after(HandsPublishSet).before(TransformSystems::Propagate))
            .add_systems(OnExit(AppState::InGame), clear);
        use crate::warmup::WarmupAppExt;
        app.add_warmup(warmup);
    }
}

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(PalmAssets {
        sphere: meshes.add(Sphere::new(1.0).mesh().uv(24, 16)),
        cylinder: meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(24)),
        cone: meshes.add(Cone::new(1.0, 1.0).mesh().resolution(24)),
    });
}

fn energy_mat(col: [f32; 3], alpha: f32, add: bool, hot: f32) -> StandardMaterial {
    let c = if add { col } else { [0.55 + col[0] * 0.45, 0.55 + col[1] * 0.45, 0.55 + col[2] * 0.45] };
    StandardMaterial {
        base_color: Color::linear_rgba(c[0], c[1], c[2], alpha),
        emissive: LinearRgba::rgb(c[0] * hot, c[1] * hot, c[2] * hot),
        unlit: true,
        alpha_mode: if add { AlphaMode::Add } else { AlphaMode::Blend },
        ..default()
    }
}

/// Loading: the beam / orb materials compile before play (no first-kamehameha hitch).
fn warmup(world: &mut World) {
    let cone = world.resource_mut::<Assets<Mesh>>().add(Cone::new(0.3, 0.6).mesh().resolution(12));
    let cyl = world.resource_mut::<Assets<Mesh>>().add(Cylinder::new(0.3, 0.6).mesh().resolution(12));
    for (add, mesh) in [(false, cone), (true, cyl)] {
        let m = world.resource_mut::<Assets<StandardMaterial>>().add(energy_mat([0.35, 0.7, 1.0], 0.8, add, 6.0));
        world.spawn((
            Name::new("palm beam warmup"),
            Mesh3d(mesh),
            MeshMaterial3d(m),
            Transform::default(),
            NotShadowCaster,
            crate::warmup::WarmupItem,
        ));
    }
}

fn clear(mut commands: Commands, mut f: ResMut<PalmFx>, parts: Query<Entity, With<PalmPart>>) {
    f.orbs.clear();
    f.beams.clear();
    f.queue.clear();
    for e in &parts {
        commands.entity(e).despawn();
    }
}

/// Release → a beam from the holder's palms to the struck point.
fn palm_events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut f: ResMut<PalmFx>) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    for ev in events.read() {
        if let GameEvent::Hitscan { owner, end, .. } = &ev.event
            && let Some(c) = active.world.constructs.held_by(*owner).filter(|c| c.recipe.form == Form::Hands)
            && f.queue.len() < 16
        {
            let (_, radius, secs) = beam_look(c.recipe.scale);
            let color = energy_rgb(c.recipe.element, corps_rgb(active.corps_of(*owner)));
            f.queue.push((*owner, c.id, bv(*end), radius, secs, color));
        }
    }
}

/// Hide every Hands construct's voxel visual; keep one orb per live Hands construct.
#[allow(clippy::too_many_arguments)]
fn track_palms(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    assets: Option<Res<PalmAssets>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut f: ResMut<PalmFx>,
    mut q: Query<(&SimConstruct, &mut Visibility), Without<PalmPart>>,
) {
    let (Some(active), Some(assets)) = (active, assets) else { return };
    for o in f.orbs.iter_mut() {
        o.seen = false;
    }
    for (sc, mut vis) in &mut q {
        if sc.form != Form::Hands {
            continue;
        }
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        if let Some(o) = f.orbs.iter_mut().find(|o| o.id == sc.id) {
            o.seen = true;
            continue;
        }
        let Some(c) = active.world.constructs.get(sc.id) else { continue };
        let color = energy_rgb(c.recipe.element, corps_rgb(active.corps_of(sc.owner)));
        let (caliber, ..) = beam_look(c.recipe.scale);
        let m_core = mats.add(energy_mat(color, 0.95, false, 8.0));
        let m_glow = mats.add(energy_mat(color, 0.4, true, 5.0));
        let part = |mesh: &Handle<Mesh>, m: &Handle<StandardMaterial>| {
            (
                PalmPart,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(m.clone()),
                Transform::from_scale(Vec3::splat(0.01)),
                Visibility::Hidden,
                NotShadowCaster,
            )
        };
        let core = commands.spawn((Name::new("PalmOrb"), part(&assets.sphere, &m_core))).id();
        let glow = commands.spawn((Name::new("PalmOrbGlow"), part(&assets.sphere, &m_glow))).id();
        let light = commands
            .spawn((
                Name::new("PalmOrbLight"),
                PalmPart,
                PointLight {
                    color: Color::linear_rgb(color[0], color[1], color[2]),
                    intensity: 0.0,
                    range: 6.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        f.orbs.push(Orb {
            id: sc.id,
            owner: sc.owner,
            color,
            caliber,
            charge: 0.0,
            core,
            glow,
            light,
            mats: [m_core, m_glow],
            pos: Vec3::ZERO,
            seen: true,
        });
    }
    let mut i = 0;
    while i < f.orbs.len() {
        if f.orbs[i].seen {
            i += 1;
            continue;
        }
        let o = f.orbs.swap_remove(i);
        for e in [o.core, o.glow, o.light] {
            commands.entity(e).try_despawn();
        }
    }
}

type Cams<'w, 's> = Query<'w, 's, &'static Transform, (With<PlayerCamera>, Without<PalmPart>, Without<SimConstruct>)>;

type PartQuery<'w, 's> =
    Query<'w, 's, (&'static mut Transform, &'static mut Visibility), (With<PalmPart>, Without<SimConstruct>, Without<PlayerCamera>)>;

/// Orbs between the palms, beams from them (PostUpdate: this frame's camera and hands).
#[allow(clippy::too_many_arguments)]
fn place_palms(
    mut commands: Commands,
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    (views, hands, body_hands, comfort): (Res<WorldViews>, Res<ViewmodelHands>, Res<BodyHands>, Res<ComfortSettings>),
    assets: Option<Res<PalmAssets>>,
    mut f: ResMut<PalmFx>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut fx: ResMut<FxParticles>,
    mut parts: PartQuery,
    mut lights: Query<&mut PointLight, With<PalmPart>>,
    cam: Cams,
    constructs: Query<(&SimConstruct, &Transform), Without<PalmPart>>,
    (mut shake, mut fov): (MessageWriter<CameraShakeEvent>, MessageWriter<FovPunch>),
) {
    let (Some(active), Some(assets)) = (active, assets) else { return };
    let f = &mut *f;
    if f.orbs.is_empty() && f.beams.is_empty() && f.queue.is_empty() {
        return;
    }
    let dt = time.delta_secs().min(0.05);
    let now = time.elapsed_secs();
    let cam = cam.single().ok().copied().unwrap_or_default();
    let eye = cam.translation;
    let local = active.local;
    let bright = 0.45 + 0.55 * comfort.flashes.clamp(0.0, 1.0);
    // Where a holder's palms are this frame.
    let palm_point = |owner: PlayerId, id: ConstructId| -> (Vec3, bool) {
        if Some(owner) == local && hands.active {
            let mid = (hands.world[0] + hands.world[1]) * 0.5;
            return (mid + cam.forward() * 0.07 + cam.up() * 0.015, true);
        }
        let aim = views.player(owner).map(|v| Quat::from_euler(EulerRot::YXZ, v.yaw, v.pitch, 0.0) * Vec3::NEG_Z).unwrap_or(Vec3::NEG_Z);
        if let Some(h) = body_hands.0.get(&owner) {
            return (*h + aim * 0.25, false);
        }
        let at = constructs.iter().find(|(sc, _)| sc.id == id).map(|(_, tf)| tf.translation);
        let body = views.player(owner).map(|v| v.pos + Vec3::Y * 1.2 * v.size.max(0.2));
        (at.or(body).unwrap_or(Vec3::ZERO) + aim * 0.3, false)
    };

    // --- orbs
    for o in f.orbs.iter_mut() {
        let (pos, first) = palm_point(o.owner, o.id);
        o.pos = pos;
        let (charging, level) = views.construct(o.id).map_or((false, 0.0), |c| {
            let phase = WeaponPhase::from_u8(c.weapon & 7);
            (phase == WeaponPhase::Charging, (c.weapon >> 3) as f32 / 31.0)
        });
        let firing = f.beams.iter().any(|b| b.construct == o.id && b.t < b.dur);
        let want = if firing {
            if first { 0.3 } else { 0.8 }
        } else if charging {
            level.max(0.05)
        } else {
            0.0
        };
        o.charge += (want - o.charge) * (1.0 - (-14.0 * dt).exp());
        let r = orb_radius(o.charge, o.caliber, first) * (1.0 + 0.08 * (now * 23.0).sin() * o.charge);
        let full = charging && level >= 0.99;
        for (e, k) in [(o.core, 1.0), (o.glow, 1.9 + 0.3 * (now * 9.0).sin())] {
            if let Ok((mut tf, mut vis)) = parts.get_mut(e) {
                tf.translation = pos;
                tf.scale = Vec3::splat((r * k).max(0.002));
                let v = if r > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
                if *vis != v {
                    *vis = v;
                }
            }
        }
        let hot = (4.0 + 14.0 * o.charge + if full { 6.0 * (now * 30.0).sin().abs() } else { 0.0 }) * bright;
        if let Some(mut m) = mats.get_mut(&o.mats[0]) {
            let c = o.color;
            m.emissive = LinearRgba::rgb((0.55 + c[0] * 0.45) * hot, (0.55 + c[1] * 0.45) * hot, (0.55 + c[2] * 0.45) * hot);
        }
        if let Ok((mut tf, mut vis)) = parts.get_mut(o.light) {
            tf.translation = pos;
            let v = if o.charge > 0.02 { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != v {
                *vis = v;
            }
        }
        if let Ok(mut l) = lights.get_mut(o.light) {
            l.intensity = o.charge * if first { 9_000.0 } else { 60_000.0 * o.caliber } * bright;
            l.range = if first { 2.5 } else { 4.0 + 4.0 * o.caliber };
        }
        // Charging: energy is gathered INTO the palms.
        if charging && o.charge > 0.1 {
            let n = if first { 1 } else { (o.charge * 3.0) as usize + 1 };
            let reach = if first { 0.35 } else { 1.2 * o.caliber.sqrt() };
            for k in 0..n {
                let a = now * 13.0 + k as f32 * 2.1;
                let off = Vec3::new(a.cos(), (a * 1.7).sin() * 0.6, (a * 0.8).sin()).normalize_or(Vec3::Y) * reach;
                fx.emit(Emit {
                    kind: PKind::Ballistic,
                    flags: PFlags::STRETCH | PFlags::PLAIN,
                    pos: pos + off,
                    vel: -off * 4.0,
                    size: if first { 0.006 } else { 0.05 },
                    color: o.color,
                    emissive: 4.0 * bright,
                    life: 0.22,
                    gravity: 0.0,
                    drag: 0.0,
                    ..default()
                });
            }
        }
    }

    // --- start queued beams
    for (owner, construct, end, radius, secs, color) in std::mem::take(&mut f.queue) {
        if f.beams.len() >= 12 {
            break;
        }
        let mine = Some(owner) == local;
        let (from, first) =
            f.orbs.iter().find(|o| o.id == construct).map_or_else(|| palm_point(owner, construct), |o| (o.pos, mine && hands.active));
        let core_a = if first { 0.6 } else { 0.9 };
        let mk = |mats: &mut Assets<StandardMaterial>| {
            [
                mats.add(energy_mat(color, core_a, false, 10.0 * bright)),
                mats.add(energy_mat(color, 0.35, true, 6.0 * bright)),
                mats.add(energy_mat(color, 0.9, false, 16.0 * bright)),
            ]
        };
        let m = mk(&mut mats);
        let spawn = |commands: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>| {
            commands
                .spawn((
                    Name::new("PalmBeam"),
                    PalmPart,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_scale(Vec3::splat(0.001)),
                    Visibility::Hidden,
                    NotShadowCaster,
                ))
                .id()
        };
        let parts_e = [
            spawn(&mut commands, &assets.cone, &m[0]),
            spawn(&mut commands, &assets.cylinder, &m[0]),
            spawn(&mut commands, &assets.cylinder, &m[1]),
            spawn(&mut commands, &assets.sphere, &m[2]),
        ];
        debug!("palm beam: {owner:?} {:.1} m, radius {radius:.2}, {secs:.2} s", from.distance(end));
        f.beams.push(Beam {
            owner,
            construct,
            end,
            color,
            radius,
            dur: secs,
            t: 0.0,
            parts: parts_e,
            mats: m,
            spark: 0.0,
            ring: 1.0,
            mine,
        });
        // Juice: the caster's kick (the viewmodel thrusts), everyone near feels it.
        let k = (radius / 1.1).clamp(0.2, 1.0);
        if mine {
            shake.write(CameraShakeEvent { trauma: 0.25 + 0.3 * k, dir: Vec3::ZERO });
            fov.write(FovPunch { degrees: 2.0 + 4.0 * k });
        } else {
            let d = eye.distance(from).min(eye.distance(end));
            if d < 40.0 {
                shake.write(CameraShakeEvent { trauma: (1.0 - d / 40.0) * 0.3 * k, dir: Vec3::ZERO });
            }
        }
        let _ = first;
    }

    // --- live beams
    let mut i = 0;
    while i < f.beams.len() {
        let b = &mut f.beams[i];
        b.t += dt;
        if b.t >= b.dur {
            let b = f.beams.swap_remove(i);
            for e in b.parts {
                commands.entity(e).try_despawn();
            }
            for m in &b.mats {
                mats.remove(m);
            }
            continue;
        }
        let from = f.orbs.iter().find(|o| o.id == b.construct).map_or_else(|| palm_point(b.owner, b.construct).0, |o| o.pos);
        let to = b.end;
        let len = from.distance(to);
        let dir = (to - from).normalize_or(Vec3::NEG_Z);
        let env = beam_envelope(b.t, b.dur);
        let r = b.radius * env;
        // Flare out of the palms over the first metres (a cone), then full width to the end.
        let flare = (len * 0.3).clamp(0.5, 5.0).min(len);
        let rest = (len - flare).max(0.0);
        let cone_rot = Quat::from_rotation_arc(Vec3::Y, -dir);
        let rot = Quat::from_rotation_arc(Vec3::Y, dir);
        let place = [
            (from + dir * flare * 0.5, cone_rot, Vec3::new(r, flare, r)),
            (from + dir * (flare + rest * 0.5), rot, Vec3::new(r, rest.max(0.001), r)),
            // The glow only past the flare (in first person it would otherwise wash over the view).
            (from + dir * (flare + rest * 0.5), rot, Vec3::new(r * 1.7, rest.max(0.001), r * 1.7)),
            (to, Quat::IDENTITY, Vec3::splat((r * 2.2).max(0.001))),
        ];
        for (e, (p, q, s)) in b.parts.iter().zip(place) {
            if let Ok((mut tf, mut vis)) = parts.get_mut(*e) {
                *tf = Transform { translation: p, rotation: q, scale: s };
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
        }
        // Sparks streaming along it and a spray where it hits.
        b.spark += dt * (60.0 + 120.0 * b.radius) * env.min(1.0);
        let n = (b.spark as usize).min(24);
        b.spark -= n as f32;
        let seed = (b.t * 1000.0) as u32 ^ b.owner.0 as u32;
        let (s1, s2) = dir.any_orthonormal_pair();
        for k in 0..n {
            let h = |j: u32| crate::util::hash3((seed.wrapping_add(k as u32 * 7)) as i32, j as i32, 91);
            let along = h(1) * len;
            let a = h(2) * std::f32::consts::TAU;
            let out = s1 * a.cos() + s2 * a.sin();
            let at_end = k % 3 == 0;
            let p = if at_end { to } else { from + dir * along + out * r * 1.1 };
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::STRETCH | PFlags::PLAIN,
                pos: p,
                vel: if at_end { (out + -dir * 0.6) * (6.0 + 8.0 * h(3)) } else { dir * 14.0 + out * 3.0 },
                size: 0.05 + 0.08 * b.radius,
                color: b.color,
                emissive: 5.0 * bright,
                life: 0.25 + 0.2 * h(4),
                gravity: if at_end { 6.0 } else { 0.0 },
                drag: 1.5,
                ..default()
            });
        }
        // Energy pouring OUT of the palms: a ring of streaks leaves the hands every ~50 ms and races
        // down the beam (reads with depth even looking straight along it).
        b.ring += dt;
        if b.ring >= 0.05 && env > 0.3 {
            b.ring = 0.0;
            let rr = r * 1.25;
            for k in 0..10 {
                let a = k as f32 / 10.0 * std::f32::consts::TAU + b.t * 7.0;
                let out = s1 * a.cos() + s2 * a.sin();
                fx.emit(Emit {
                    kind: PKind::Ballistic,
                    flags: PFlags::STRETCH | PFlags::PLAIN,
                    pos: from + dir * flare.min(1.5) + out * rr * 0.5,
                    vel: dir * 45.0 + out * rr * 0.6,
                    size: 0.04 + 0.06 * b.radius,
                    color: b.color,
                    emissive: 6.0 * bright,
                    life: (len / 45.0).clamp(0.1, 0.6),
                    gravity: 0.0,
                    drag: 0.0,
                    ..default()
                });
            }
        }
        // A rumble while it pours out (the caster; anyone standing near the struck point).
        let near = if b.mine { 0.0 } else { eye.distance(to) };
        if b.mine || near < 20.0 {
            let k = if b.mine { 0.9 } else { 1.0 - near / 20.0 };
            shake.write(CameraShakeEvent { trauma: k * (0.5 + b.radius) * dt, dir: Vec3::ZERO });
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ultimates_are_thicker_and_longer_than_plain_palm_beams() {
        let d = bd_sim::constructs::forms::form_spec(Form::Hands).default_scale;
        let plain = beam_look(d);
        let ult = beam_look(d * 2.5 * 2.5);
        assert!((plain.0 - 1.0).abs() < 1e-4 && (ult.0 - 2.5).abs() < 1e-3);
        assert!(ult.1 > plain.1 * 3.0, "thick: {} vs {}", ult.1, plain.1);
        assert!(ult.2 >= 0.95 && plain.2 >= 0.5 && ult.2 <= 1.0, "0.5–1.0 s: {} / {}", plain.2, ult.2);
        assert_eq!(beam_look(f32::NAN).0, 1.0);
    }

    #[test]
    fn envelope_snaps_on_holds_and_tapers() {
        assert_eq!(beam_envelope(0.0, 1.0), 0.0);
        assert!(beam_envelope(0.1, 1.0) > 0.9);
        assert!(beam_envelope(0.5, 1.0) > 0.9);
        assert!(beam_envelope(0.95, 1.0) < 0.35);
        assert_eq!(beam_envelope(1.0, 1.0), 0.0);
        assert_eq!(beam_envelope(f32::NAN, 1.0), 0.0);
    }

    #[test]
    fn orb_grows_with_charge_and_energy_colours_read() {
        assert!(orb_radius(1.0, 1.0, false) > orb_radius(0.0, 1.0, false) * 4.0);
        assert!(orb_radius(1.0, 2.5, true) < 0.1, "first person stays between the palms");
        // Kamehameha (Light) is blue, Final Flash (Electric) yellow-white, Galick Gun (Psychic) purple.
        let [r, g, b] = energy_rgb(Element::Light, [0.0, 1.0, 0.0]);
        assert!(b > r && b > 0.9 && g < b);
        let [r, g, b] = energy_rgb(Element::Electric, [0.0; 3]);
        assert!(r > 0.9 && g > 0.9 && b > 0.4);
        let [r, g, b] = energy_rgb(Element::Psychic, [0.0; 3]);
        assert!(b > g && r > g);
    }
}
