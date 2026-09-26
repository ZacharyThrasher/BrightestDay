//! Signature moments (`GameEvent::SignatureFx`, `bd_sim::signature::SigFx`) — the Hollow Purple
//! look, generic for any fusion windup / erasing orb:
//!
//! * **Merge windup** (`SigFx::Merge { a, b, secs }`): an orb of each element forms at either of the
//!   caster's hands (blue attract, red repel for Hollow Purple), grows, and is pulled to the centre;
//!   at the end they MERGE — a violent flash, a charge-aura burst + shockwave (`bd_fx::AuraBurstFx`,
//!   the charge worker's particles), a small FOV punch for the caster and a gentle flash for anyone
//!   near (both comfort-gated in juice).
//! * **Launch** (`SigFx::Launch`): a release shockwave; the orb (the construct itself, a big
//!   hard-light Ball) gets a thick element trail; `ult_fire` + `purple_travel_loop` start.
//! * **Erase** (`SigFx::Erase`): element burst + `erase_impact` where it deleted something (the tunnel
//!   itself is the ordinary replicated carves). **End**: the loop stops, a last burst.
//!
//! No camera takeover, no freeze. Cheap: two unlit spheres per windup, message-driven bursts.

use super::constructs::ConstructEntities;
use super::signature::{Cue, SignatureCue, element_rgb};
use super::{bv, corps_rgb};
use crate::camera::PlayerCamera;
use crate::juice::{CameraShakeEvent, FovPunch, ScreenFlash};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId};
use bd_fx::{AuraBurstFx, BurstFx, BurstKind, TrailFx};
use bd_sim::GameEvent;
use bd_sim::signature::SigFx;
use bevy::prelude::*;

/// One fusion windup in progress.
struct Fusion {
    id: ConstructId,
    owner: PlayerId,
    a: Element,
    b: Element,
    t: f32,
    secs: f32,
    orbs: [Entity; 2],
    mats: [Handle<StandardMaterial>; 2],
}

#[derive(Resource, Default)]
struct Fusions {
    live: Vec<Fusion>,
    queue: Vec<(ConstructId, PlayerId, SigFx, Vec3, Vec3)>,
    /// Orbs still in flight waiting for their construct entity to get a trail.
    trail_wanted: Vec<(ConstructId, Element, f32)>,
}

#[derive(Resource)]
struct FusionAssets {
    sphere: Handle<Mesh>,
}

#[derive(Component)]
struct FusionOrb;

pub struct SignatureFxPlugin;

impl Plugin for SignatureFxPlugin {
    fn build(&self, app: &mut App) {
        // Messages this plugin uses that other plugins own (idempotent).
        app.add_message::<BurstFx>()
            .add_message::<AuraBurstFx>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<ScreenFlash>()
            .add_message::<SignatureCue>()
            .add_message::<GameEvents>()
            .init_resource::<Fusions>()
            .add_systems(Startup, assets)
            .add_systems(
                Update,
                (
                    read_events.in_set(super::PresentSet::Events),
                    (apply_fx, animate_fusions, trails).chain().in_set(super::PresentSet::Sync),
                ),
            )
            .add_systems(OnExit(AppState::InGame), clear);
    }
}

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FusionAssets {
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(16, 12))),
    });
}

fn clear(mut commands: Commands, mut f: ResMut<Fusions>, orbs: Query<Entity, With<FusionOrb>>) {
    f.live.clear();
    f.queue.clear();
    f.trail_wanted.clear();
    for e in &orbs {
        commands.entity(e).despawn();
    }
}

fn read_events(mut events: MessageReader<GameEvents>, mut f: ResMut<Fusions>) {
    for ev in events.read() {
        if let GameEvent::SignatureFx { id, owner, fx, pos, dir } = &ev.event
            && f.queue.len() < 64
        {
            f.queue.push((*id, *owner, *fx, bv(*pos), bv(*dir)));
        }
    }
}

/// Where the caster's hands are in the world: (left, right, centre in front of the chest). The same
/// for every viewer (first person sees them at the bottom corners, third person / others at the body).
fn hands(owner: PlayerId, views: &WorldViews) -> Option<(Vec3, Vec3, Vec3)> {
    let v = views.player(owner)?;
    let rot = Quat::from_euler(EulerRot::YXZ, v.yaw, 0.0, 0.0);
    let (r, f) = (rot * Vec3::X, rot * Vec3::NEG_Z);
    let base = v.pos + Vec3::Y * 1.25 * v.size.max(0.2) + f * 0.55;
    Some((base - r * 0.65, base + r * 0.65, base + f * 0.45))
}

#[allow(clippy::too_many_arguments)]
fn apply_fx(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    assets: Option<Res<FusionAssets>>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut f: ResMut<Fusions>,
    mut bursts: MessageWriter<BurstFx>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut fov: MessageWriter<FovPunch>,
    mut cues: MessageWriter<SignatureCue>,
) {
    if f.queue.is_empty() {
        return;
    }
    let Some(active) = active else {
        f.queue.clear();
        return;
    };
    let eye = cam.single().map(|g| g.translation()).unwrap_or(Vec3::ZERO);
    let f = &mut *f;
    for (id, owner, fx, pos, dir) in std::mem::take(&mut f.queue) {
        let mine = active.local == Some(owner);
        let corps = corps_rgb(active.corps_of(owner));
        let near = eye.distance(pos) < 40.0;
        let cue =
            |c: Cue, element: Element| SignatureCue { cue: c, pos, severity: if near { 0.9 } else { 0.5 }, mine, element, construct: id };
        match fx {
            SigFx::Merge { a, b, secs } => {
                let Some(assets) = assets.as_ref() else { continue };
                let mut orbs = [Entity::PLACEHOLDER; 2];
                let mut hm: [Handle<StandardMaterial>; 2] = [Handle::default(), Handle::default()];
                for (k, e) in [a, b].into_iter().enumerate() {
                    let c = element_rgb(e, corps);
                    let m = mats.add(StandardMaterial {
                        base_color: Color::linear_rgba(c[0], c[1], c[2], 0.9),
                        emissive: LinearRgba::rgb(c[0] * 2.0, c[1] * 2.0, c[2] * 2.0),
                        unlit: true,
                        alpha_mode: AlphaMode::Add,
                        ..default()
                    });
                    orbs[k] = commands
                        .spawn((
                            Name::new("FusionOrb"),
                            FusionOrb,
                            Mesh3d(assets.sphere.clone()),
                            MeshMaterial3d(m.clone()),
                            Transform::from_translation(pos).with_scale(Vec3::splat(0.05)),
                            bevy::light::NotShadowCaster,
                        ))
                        .id();
                    hm[k] = m;
                }
                f.live.push(Fusion { id, owner, a, b, t: 0.0, secs: secs.max(0.2), orbs, mats: hm });
                cues.write(cue(Cue::UltCharge, a));
            }
            SigFx::Launch { radius, .. } => {
                let element = active.world.constructs.get(id).map_or(Element::Psychic, |c| c.recipe.element);
                let col = element_rgb(element, corps);
                bursts.write(BurstFx { kind: BurstKind::Flash, pos, dir, scale: 3.0 + radius, color: Some(col) });
                bursts.write(BurstFx { kind: BurstKind::Element(element), pos, dir, scale: 2.5 + radius, color: Some(col) });
                if near {
                    shake.write(CameraShakeEvent { trauma: if mine { 0.25 } else { 0.18 }, dir: dir.normalize_or_zero() });
                }
                if mine {
                    fov.write(FovPunch { degrees: 5.0 });
                }
                cues.write(cue(Cue::UltFire, element));
                cues.write(cue(Cue::OrbTravel { on: true }, element));
                f.trail_wanted.push((id, element, radius));
            }
            SigFx::Erase => {
                let element = active.world.constructs.get(id).map_or(Element::Psychic, |c| c.recipe.element);
                bursts.write(BurstFx { kind: BurstKind::Element(element), pos, dir, scale: 2.0, color: Some(element_rgb(element, corps)) });
                bursts.write(BurstFx { kind: BurstKind::Shards, pos, dir, scale: 1.5, color: None });
                cues.write(cue(Cue::EraseImpact, element));
            }
            SigFx::End => {
                bursts.write(BurstFx {
                    kind: BurstKind::Element(Element::Psychic),
                    pos,
                    dir,
                    scale: 2.0,
                    color: Some(element_rgb(Element::Psychic, corps)),
                });
                cues.write(cue(Cue::OrbTravel { on: false }, Element::Psychic));
                f.trail_wanted.retain(|t| t.0 != id);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn animate_fusions(
    mut commands: Commands,
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    comfort: Res<crate::settings::ComfortSettings>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut orbs: Query<&mut Transform, With<FusionOrb>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut f: ResMut<Fusions>,
    mut aura: MessageWriter<AuraBurstFx>,
    mut bursts: MessageWriter<BurstFx>,
    mut flash: MessageWriter<ScreenFlash>,
    mut fov: MessageWriter<FovPunch>,
    mut cues: MessageWriter<SignatureCue>,
) {
    if f.live.is_empty() {
        return;
    }
    let Some(active) = active else { return };
    let dt = time.delta_secs();
    let eye = cam.single().map_or(Vec3::ZERO, |c| c.translation());
    let mut i = 0;
    while i < f.live.len() {
        let fu = &mut f.live[i];
        fu.t += dt;
        let k = (fu.t / fu.secs).clamp(0.0, 1.0);
        let Some((l, r, c)) = hands(fu.owner, &views) else {
            i += 1;
            continue;
        };
        // They are pulled into the gathering orb (the construct itself) when it is there.
        let c = views.construct(fu.id).map_or(c, |v| v.pos);
        // Grow at the hands, then get pulled together (ease-in: it snaps shut at the end).
        let pull = k * k * k;
        let size = 0.2 + 0.5 * k.min(0.6) / 0.6;
        let wobble = (fu.t * 30.0).sin() * 0.02 * k;
        for (n, from) in [l, r].into_iter().enumerate() {
            if let Ok(mut tf) = orbs.get_mut(fu.orbs[n]) {
                tf.translation = from.lerp(c, pull) + Vec3::Y * wobble;
                tf.scale = Vec3::splat(size * (1.0 + 0.15 * (fu.t * 18.0 + n as f32 * 3.0).sin()));
            }
            if let Some(mut m) = mats.get_mut(&fu.mats[n]) {
                let g = 2.0 + 3.0 * k;
                let lin = m.base_color.to_linear();
                m.emissive = LinearRgba::rgb(lin.red * g, lin.green * g, lin.blue * g);
            }
        }
        if k < 1.0 {
            i += 1;
            continue;
        }
        // MERGE.
        let fu = f.live.swap_remove(i);
        for e in fu.orbs {
            commands.entity(e).despawn();
        }
        let mine = active.local == Some(fu.owner);
        let corps = corps_rgb(active.corps_of(fu.owner));
        let merged = mix(element_rgb(fu.a, corps), element_rgb(fu.b, corps));
        let feet = views.player(fu.owner).map_or(c - Vec3::Y * 1.3, |v| v.pos);
        aura.write(AuraBurstFx {
            pos: feet,
            color: merged,
            height: 1.8,
            scale: 2.0,
            flash: comfort.flashes > 0.0,
            broken: false,
            ground: None,
        });
        bursts.write(BurstFx { kind: BurstKind::Flash, pos: c, dir: Vec3::ZERO, scale: 3.0, color: Some(merged) });
        let near = eye.distance(c) < 30.0;
        if near {
            flash.write(ScreenFlash {
                color: Color::linear_rgb(merged[0], merged[1], merged[2]),
                intensity: if mine { 0.18 } else { 0.1 },
                duration: 0.18,
                vignette: false,
            });
        }
        if mine {
            fov.write(FovPunch { degrees: 4.0 });
        }
        cues.write(SignatureCue {
            cue: Cue::MergeFlash,
            pos: c,
            severity: if near { 1.0 } else { 0.5 },
            mine,
            element: fu.a,
            construct: fu.id,
        });
    }
}

fn mix(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [(a[0] + b[0]) * 0.6, (a[1] + b[1]) * 0.6, (a[2] + b[2]) * 0.6]
}

/// A thick element trail on each orb in flight (once its construct entity exists).
fn trails(mut commands: Commands, map: Res<ConstructEntities>, mut f: ResMut<Fusions>) {
    if f.trail_wanted.is_empty() {
        return;
    }
    f.trail_wanted.retain(|&(id, element, radius)| match map.0.get(&id) {
        Some(&e) => {
            let mut t = TrailFx::new(element);
            t.rate = 220.0;
            t.radius = radius;
            t.size = 2.5;
            commands.entity(e).try_insert(t);
            false
        }
        None => true,
    });
}
