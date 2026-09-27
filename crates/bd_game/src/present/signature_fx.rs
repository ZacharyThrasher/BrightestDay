//! Signature moments (`GameEvent::SignatureFx`, `bd_sim::signature::SigFx`) — the Hollow Purple
//! look, generic for any fusion windup / erasing orb:
//!
//! * **The orb is imaginary mass, not a voxel model**: every erasing orb (`orb::is_erasing_orb` on its
//!   spawn) is drawn as a sphere with [`ImaginaryMaterial`] (`imaginary.wgsl`) in its ELEMENT colour —
//!   Psychic: a near-black violet heart with slow swirling bands and a bright magenta / white fresnel
//!   rim — plus a faint shimmering halo shell. The construct's own voxel visual is hidden.
//! * **Merge windup** (`SigFx::Merge { a, b, secs }`): an orb of each element forms at either hand
//!   (blue attract, red repel), grows, and is pulled into the gathering orb; the last 30 % is a tight,
//!   brightening implosion (streaks rushing inward) that ends in the orb's emissive spike.
//! * **Release** (`SigFx::Launch`): an ~80 ms emissive spike AT the orb, a radial shockwave ring
//!   (particles in the plane across the flight), a short FOV punch for the caster. Nothing covers the
//!   screen; comfort (flashes off) drops the spike.
//! * **Travel**: streaks sucked INTO the orb from all around it, the tunnel wall glowing violet (a ring
//!   of embers every metre, fading over ~1.5 s), and [`EraseSinks`] makes the terrain debris of its
//!   carves fly into the orb instead of spraying out (present::terrain). Nearby players feel a subtle,
//!   distance-scaled shake (juice applies the comfort setting).
//! * **Erase / End**: small implosions where something is deleted; the orb collapses at the end.
//!
//! No camera takeover, no freeze. Cost: ≤ 2 spheres per orb, bounded particle rates.

use super::constructs::SimConstruct;
use super::signature::{Cue, SignatureCue, element_rgb};
use super::{bv, corps_rgb};
use crate::camera::PlayerCamera;
use crate::juice::{CameraShakeEvent, FovPunch, ScreenFlash};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId};
use bd_fx::{AuraBurstFx, BurstFx, Emit, FxParticles, PFlags, PKind};
use bd_sim::GameEvent;
use bd_sim::signature::SigFx;
use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

// ------------------------------------------------------------------------------------ material

const IMAGINARY_SHADER: Handle<Shader> = uuid_handle!("3c0a9e57-8b21-4f6d-a5c4-6e1f2d9b7a13");

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct ImaginaryParams {
    /// rgb = core colour, a = opacity.
    pub core: Vec4,
    /// rgb = rim colour, w = fresnel power.
    pub rim: Vec4,
    /// x = time, y = spike 0..1, z = mode (0 core, 1 halo), w = swirl speed.
    pub misc: Vec4,
}

/// The erasing-orb sphere ("imaginary mass").
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ImaginaryMaterial {
    #[uniform(0)]
    pub params: ImaginaryParams,
    pub alpha_mode: AlphaMode,
}

impl Material for ImaginaryMaterial {
    fn fragment_shader() -> ShaderRef {
        IMAGINARY_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}

/// (core, rim) colours of an orb of `e` (Psychic: deep violet + magenta-white).
fn orb_colors(e: Element, corps: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    match e {
        Element::Psychic => ([0.2, 0.02, 0.42], [1.0, 0.42, 1.0]),
        Element::Shadow => ([0.03, 0.0, 0.06], [0.6, 0.2, 1.0]),
        _ => {
            let c = element_rgb(e, corps);
            ([c[0] * 0.3, c[1] * 0.3, c[2] * 0.3], [0.45 + c[0] * 0.55, 0.45 + c[1] * 0.55, 0.45 + c[2] * 0.55])
        }
    }
}

// ------------------------------------------------------------------------------------ state

/// Live erasing orbs in flight: (position, velocity, radius). present::terrain sucks the debris of
/// carves near one INTO it (the tunnel vanishes into the orb).
#[derive(Resource, Default)]
pub struct EraseSinks(pub Vec<(Vec3, Vec3, f32)>);

impl EraseSinks {
    /// The orb that carved near `p`: (where it will be in a moment, its velocity).
    pub fn sink(&self, p: Vec3) -> Option<(Vec3, Vec3)> {
        self.0
            .iter()
            .filter(|(c, _, r)| c.distance(p) < r * 2.0 + 3.0)
            .min_by(|a, b| a.0.distance_squared(p).total_cmp(&b.0.distance_squared(p)))
            .map(|&(c, v, _)| (c, v))
    }
}

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

/// One erasing orb's visuals.
struct OrbVis {
    id: ConstructId,
    element: Element,
    rim: [f32; 3],
    radius: f32,
    windup: f32,
    t: f32,
    launched: bool,
    ended: Option<f32>,
    spike: f32,
    core: Entity,
    halo: Entity,
    mats: [Handle<ImaginaryMaterial>; 2],
    last: Option<Vec3>,
    vel: Vec3,
    wall_from: Option<Vec3>,
    suck_credit: f32,
}

#[derive(Resource, Default)]
struct Fusions {
    live: Vec<Fusion>,
    orbs: Vec<OrbVis>,
    queue: Vec<(ConstructId, PlayerId, SigFx, Vec3, Vec3)>,
    /// (id, owner, element, radius, windup) of freshly spawned erasing orbs.
    spawned: Vec<(ConstructId, PlayerId, Element, f32, f32)>,
    gone: Vec<ConstructId>,
    /// Construct ids whose voxel visual stays hidden (live orbs + a moment after they end).
    hidden: Vec<(ConstructId, f32)>,
}

#[derive(Resource)]
struct FusionAssets {
    sphere: Handle<Mesh>,
    smooth: Handle<Mesh>,
}

#[derive(Component)]
struct FusionOrb;

pub struct SignatureFxPlugin;

impl Plugin for SignatureFxPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, IMAGINARY_SHADER, "imaginary.wgsl", Shader::from_wgsl);
        // Messages this plugin uses that other plugins own (idempotent).
        app.add_plugins(MaterialPlugin::<ImaginaryMaterial>::default())
            .add_message::<BurstFx>()
            .add_message::<AuraBurstFx>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<ScreenFlash>()
            .add_message::<SignatureCue>()
            .add_message::<GameEvents>()
            .init_resource::<Fusions>()
            .init_resource::<EraseSinks>()
            .add_systems(Startup, assets)
            .add_systems(
                Update,
                (
                    read_events.in_set(super::PresentSet::Events),
                    (apply_fx, animate_fusions).chain().in_set(super::PresentSet::Sync),
                    // After the construct sync (it rewrites visibility every frame).
                    animate_orbs.after(super::PresentSet::Sync),
                ),
            )
            .add_systems(OnExit(AppState::InGame), clear);
        // No first-use hitch: both materials compile during Loading.
        use crate::warmup::WarmupAppExt;
        app.add_warmup(warmup_fx);
    }
}

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FusionAssets {
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(16, 12))),
        smooth: meshes.add(Sphere::new(1.0).mesh().uv(48, 32)),
    });
}

/// Loading: one fusion orb + one marker segment + one imaginary sphere (pipeline pre-warm).
fn warmup_fx(world: &mut World) {
    let sphere = world.resource_mut::<Assets<Mesh>>().add(Sphere::new(0.3).mesh().uv(16, 12));
    let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(0.4, 0.05, 0.2));
    let mat = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
        base_color: Color::linear_rgba(0.5, 0.2, 1.0, 0.9),
        emissive: LinearRgba::rgb(1.0, 0.4, 2.0),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    for mesh in [sphere.clone(), cube] {
        world.spawn((
            Name::new("signature warmup"),
            Mesh3d(mesh),
            MeshMaterial3d(mat.clone()),
            Transform::default(),
            bevy::light::NotShadowCaster,
            crate::warmup::WarmupItem,
        ));
    }
    let (core, rim) = orb_colors(Element::Psychic, [1.0; 3]);
    for mode in [0.0, 1.0] {
        let m = world.resource_mut::<Assets<ImaginaryMaterial>>().add(imaginary(core, rim, mode));
        world.spawn((
            Name::new("signature warmup (imaginary)"),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(m),
            Transform::default(),
            bevy::light::NotShadowCaster,
            crate::warmup::WarmupItem,
        ));
    }
}

fn imaginary(core: [f32; 3], rim: [f32; 3], mode: f32) -> ImaginaryMaterial {
    let halo = mode > 0.5;
    ImaginaryMaterial {
        params: ImaginaryParams {
            core: Vec4::new(core[0], core[1], core[2], if halo { 0.55 } else { 0.97 }),
            rim: Vec4::new(rim[0], rim[1], rim[2], if halo { 1.6 } else { 2.6 }),
            misc: Vec4::new(0.0, 0.0, mode, 1.0),
        },
        alpha_mode: AlphaMode::Blend,
    }
}

fn clear(mut commands: Commands, mut f: ResMut<Fusions>, mut sinks: ResMut<EraseSinks>, orbs: Query<Entity, With<FusionOrb>>) {
    f.live.clear();
    f.orbs.clear();
    f.queue.clear();
    f.spawned.clear();
    f.gone.clear();
    f.hidden.clear();
    sinks.0.clear();
    for e in &orbs {
        commands.entity(e).despawn();
    }
}

fn read_events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut f: ResMut<Fusions>) {
    for ev in events.read() {
        match &ev.event {
            GameEvent::SignatureFx { id, owner, fx, pos, dir } if f.queue.len() < 64 => {
                f.queue.push((*id, *owner, *fx, bv(*pos), bv(*dir)));
            }
            GameEvent::ConstructSpawned { id, owner, recipe, forming, .. } if bd_sim::signature::orb::is_erasing_orb(recipe) => {
                if !f.orbs.iter().any(|o| o.id == *id) && f.spawned.len() < 16 {
                    f.spawned.push((*id, *owner, recipe.element, recipe.scale * 0.5, forming.max(0.05)));
                }
            }
            GameEvent::ConstructGone { id, .. } if active.is_some() => f.gone.push(*id),
            _ => {}
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

/// `n` streaks from a shell of `r0..r1` around `c` rushing into `target` over `life` seconds.
#[allow(clippy::too_many_arguments)]
fn implode(fx: &mut FxParticles, c: Vec3, target: Vec3, r0: f32, r1: f32, n: usize, life: f32, color: [f32; 3], size: f32) {
    for _ in 0..n {
        let r = &mut fx.rng;
        let dir = r.dir();
        let dist = r.range(r0, r1);
        let l = life * r.range(0.8, 1.2);
        let pos = c + dir * dist;
        fx.emit(Emit {
            kind: PKind::Ballistic,
            flags: PFlags::STRETCH | PFlags::PLAIN,
            pos,
            vel: (target - pos) / l,
            size,
            color,
            emissive: 4.0,
            life: l,
            gravity: 0.0,
            spin: 3.0,
            ..default()
        });
    }
}

/// A radial shockwave ring across `dir` at `c`.
fn shock_ring(fx: &mut FxParticles, c: Vec3, dir: Vec3, speed: f32, n: usize, color: [f32; 3]) {
    let (s1, s2) = dir.normalize_or(Vec3::Y).any_orthonormal_pair();
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let out = s1 * a.cos() + s2 * a.sin();
        fx.emit(Emit {
            kind: PKind::Ballistic,
            flags: PFlags::STRETCH | PFlags::PLAIN,
            pos: c + out * 0.5,
            vel: out * speed,
            size: 0.12,
            color,
            emissive: 5.0,
            life: 0.35,
            gravity: 0.0,
            drag: 3.5,
            spin: 2.5,
            ..default()
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_fx(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    assets: Option<Res<FusionAssets>>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    comfort: Res<crate::settings::ComfortSettings>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut imats: ResMut<Assets<ImaginaryMaterial>>,
    mut f: ResMut<Fusions>,
    mut fx: ResMut<FxParticles>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut fov: MessageWriter<FovPunch>,
    mut cues: MessageWriter<SignatureCue>,
) {
    let Some(active) = active else {
        f.queue.clear();
        f.spawned.clear();
        return;
    };
    let f = &mut *f;
    // Fresh orbs: the imaginary-mass sphere + halo (grow through the windup).
    if let Some(assets) = assets.as_ref() {
        for (id, owner, element, radius, windup) in std::mem::take(&mut f.spawned) {
            let (core, rim) = orb_colors(element, corps_rgb(active.corps_of(owner)));
            let mc = imats.add(imaginary(core, rim, 0.0));
            let mh = imats.add(imaginary(core, rim, 1.0));
            let spawn = |commands: &mut Commands, m: Handle<ImaginaryMaterial>, name: &'static str| {
                commands
                    .spawn((
                        Name::new(name),
                        FusionOrb,
                        Mesh3d(assets.smooth.clone()),
                        MeshMaterial3d(m),
                        Transform::from_scale(Vec3::splat(0.01)),
                        Visibility::Hidden,
                        bevy::light::NotShadowCaster,
                        bevy::light::NotShadowReceiver,
                    ))
                    .id()
            };
            let core_e = spawn(&mut commands, mc.clone(), "ImaginaryOrb");
            let halo_e = spawn(&mut commands, mh.clone(), "ImaginaryOrbHalo");
            f.hidden.push((id, f32::INFINITY));
            f.orbs.push(OrbVis {
                id,
                element,
                rim,
                radius: radius.max(0.5),
                windup,
                t: 0.0,
                launched: false,
                ended: None,
                spike: 0.0,
                core: core_e,
                halo: halo_e,
                mats: [mc, mh],
                last: None,
                vel: Vec3::ZERO,
                wall_from: None,
                suck_credit: 0.0,
            });
        }
    }
    if f.queue.is_empty() {
        return;
    }
    let eye = cam.single().map(|g| g.translation()).unwrap_or(Vec3::ZERO);
    let spike_ok = comfort.flashes > 0.0;
    for (id, owner, sfx, pos, dir) in std::mem::take(&mut f.queue) {
        let mine = active.local == Some(owner);
        let corps = corps_rgb(active.corps_of(owner));
        let near = eye.distance(pos) < 40.0;
        let cue =
            |c: Cue, element: Element| SignatureCue { cue: c, pos, severity: if near { 0.9 } else { 0.5 }, mine, element, construct: id };
        let orb = f.orbs.iter_mut().find(|o| o.id == id);
        match sfx {
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
            // Other energy balls (rasengan, spirit bomb, fireball…) are drawn as themselves; their
            // throw / burst read through their own model, the gather and the ordinary explosion.
            SigFx::Launch { .. } | SigFx::Erase | SigFx::End if orb.is_none() => {}
            SigFx::Launch { radius, .. } => {
                let (element, rim) = orb.as_ref().map_or((Element::Psychic, [1.0, 0.42, 1.0]), |o| (o.element, o.rim));
                if let Some(o) = orb {
                    o.launched = true;
                    // The release: an ~80 ms emissive surge at the orb (never a screen flash).
                    if spike_ok {
                        o.spike = 1.0;
                    }
                    o.wall_from = Some(pos);
                }
                shock_ring(&mut fx, pos, dir, 22.0 + radius * 4.0, 56, rim);
                if near {
                    shake.write(CameraShakeEvent { trauma: if mine { 0.2 } else { 0.14 }, dir: dir.normalize_or_zero() });
                }
                if mine {
                    fov.write(FovPunch { degrees: 4.5 });
                }
                cues.write(cue(Cue::UltFire, element));
                cues.write(cue(Cue::OrbTravel { on: true }, element));
            }
            SigFx::Erase => {
                let (element, rim, at) =
                    orb.as_ref().map_or((Element::Psychic, [1.0, 0.42, 1.0], pos), |o| (o.element, o.rim, o.last.unwrap_or(pos)));
                // What it deleted is pulled into it.
                implode(&mut fx, pos, at, 0.5, 1.8, 24, 0.2, rim, 0.14);
                cues.write(cue(Cue::EraseImpact, element));
            }
            SigFx::End => {
                let (element, rim) = orb.as_ref().map_or((Element::Psychic, [1.0, 0.42, 1.0]), |o| (o.element, o.rim));
                if let Some(o) = orb {
                    o.ended = Some(0.0);
                }
                implode(&mut fx, pos, pos, 2.0, 5.0, 48, 0.25, rim, 0.16);
                cues.write(cue(Cue::OrbTravel { on: false }, element));
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
    mut fx: ResMut<FxParticles>,
    mut fov: MessageWriter<FovPunch>,
    mut cues: MessageWriter<SignatureCue>,
) {
    if f.live.is_empty() {
        return;
    }
    let Some(active) = active else { return };
    let dt = time.delta_secs();
    let eye = cam.single().map_or(Vec3::ZERO, |c| c.translation());
    let f = &mut *f;
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
        let corps = corps_rgb(active.corps_of(fu.owner));
        let merged = mix(element_rgb(fu.a, corps), element_rgb(fu.b, corps));
        // Grow at the hands, hover, then snap together (k⁴: the last 30 % is the implosion).
        let pull = k * k * k * k;
        let size = (0.2 + 0.5 * k.min(0.6) / 0.6) * (1.0 - 0.6 * ((k - 0.8) / 0.2).clamp(0.0, 1.0));
        let wobble = (fu.t * 30.0).sin() * 0.02 * k;
        for (n, from) in [l, r].into_iter().enumerate() {
            if let Ok(mut tf) = orbs.get_mut(fu.orbs[n]) {
                tf.translation = from.lerp(c, pull) + Vec3::Y * wobble;
                tf.scale = Vec3::splat(size * (1.0 + 0.15 * (fu.t * 18.0 + n as f32 * 3.0).sin()));
            }
            if let Some(mut m) = mats.get_mut(&fu.mats[n]) {
                let g = 2.0 + 14.0 * k.powi(6);
                let lin = m.base_color.to_linear();
                m.emissive = LinearRgba::rgb(lin.red * g, lin.green * g, lin.blue * g);
            }
        }
        if k > 0.7 && k < 1.0 {
            // The implosion: streaks rushing into the merge point.
            let n = ((dt * 260.0) as usize).max(2);
            implode(&mut fx, c, c, 1.5, 3.5, n, 0.12, merged, 0.1);
        }
        if k < 1.0 {
            i += 1;
            continue;
        }
        // MERGE (the release follows in the same instant: the orb spikes, the ring goes out).
        let fu = f.live.swap_remove(i);
        for e in fu.orbs {
            commands.entity(e).despawn();
        }
        let mine = active.local == Some(fu.owner);
        implode(&mut fx, c, c, 0.6, 2.4, 40, 0.08, [1.0, 0.85, 1.0], 0.12);
        if let Some(o) = f.orbs.iter_mut().find(|o| o.id == fu.id)
            && comfort.flashes > 0.0
        {
            o.spike = 1.0;
        }
        let near = eye.distance(c) < 30.0;
        if mine {
            fov.write(FovPunch { degrees: 3.0 });
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

type OrbTfs = (&'static mut Transform, &'static mut Visibility);

/// The orb spheres: follow the construct, grow through the windup, spike, swirl; in flight: the
/// suck-in streaks, the glowing tunnel wall, the debris sinks, a subtle nearby shake; hide the
/// construct's own voxel visual.
#[allow(clippy::too_many_arguments)]
fn animate_orbs(
    mut commands: Commands,
    time: Res<Time>,
    views: Res<WorldViews>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut f: ResMut<Fusions>,
    mut sinks: ResMut<EraseSinks>,
    mut fx: ResMut<FxParticles>,
    mut imats: ResMut<Assets<ImaginaryMaterial>>,
    mut tfs: Query<OrbTfs, (With<FusionOrb>, Without<SimConstruct>)>,
    mut constructs: Query<(&SimConstruct, &mut Visibility), Without<FusionOrb>>,
    mut shake: MessageWriter<CameraShakeEvent>,
) {
    let f = &mut *f;
    sinks.0.clear();
    if f.orbs.is_empty() && f.hidden.is_empty() {
        return;
    }
    let dt = time.delta_secs().max(1e-4);
    let now = time.elapsed_secs();
    let eye = cam.single().map_or(Vec3::ZERO, |c| c.translation());
    for id in std::mem::take(&mut f.gone) {
        if let Some(o) = f.orbs.iter_mut().find(|o| o.id == id)
            && o.ended.is_none()
        {
            o.ended = Some(0.0);
        }
    }
    let mut i = 0;
    while i < f.orbs.len() {
        let o = &mut f.orbs[i];
        o.t += dt;
        o.spike = (o.spike - dt / 0.08).max(0.0);
        if let Some(e) = o.ended.as_mut() {
            *e += dt;
        }
        let pos = views.construct(o.id).map(|c| c.pos).or(o.last);
        let collapse = o.ended.map_or(1.0, |e| (1.0 - e / 0.18).max(0.0));
        if collapse <= 0.0 || pos.is_none() && o.t > 1.0 {
            let o = f.orbs.swap_remove(i);
            commands.entity(o.core).try_despawn();
            commands.entity(o.halo).try_despawn();
            if let Some(h) = f.hidden.iter_mut().find(|h| h.0 == o.id) {
                h.1 = 1.0; // keep the voxel visual hidden through its dismiss dissolve
            }
            continue;
        }
        let Some(pos) = pos else {
            i += 1;
            continue;
        };
        if let Some(last) = o.last {
            let v = (pos - last) / dt;
            if v.is_finite() {
                o.vel = o.vel.lerp(v, 0.5);
            }
        }
        o.last = Some(pos);
        // Size: gathers through the windup (a pulse), full once released, collapses at the end.
        let grow = if o.launched { 1.0 } else { (0.15 + 0.85 * (o.t / o.windup).min(1.0)).powf(0.7) };
        let breathe = 1.0 + 0.04 * (now * 7.0).sin() + 0.12 * o.spike;
        let r = o.radius * grow * breathe * collapse;
        for (e, k) in [(o.core, 1.0), (o.halo, 1.32)] {
            if let Ok((mut tf, mut vis)) = tfs.get_mut(e) {
                tf.translation = pos;
                tf.scale = Vec3::splat((r * k).max(0.01));
                tf.rotation = Quat::from_rotation_y(now * 0.4);
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
        }
        for m in &o.mats {
            if let Some(mut mat) = imats.get_mut(m) {
                mat.params.misc.x = now;
                mat.params.misc.y = o.spike;
            }
        }
        if o.launched && o.ended.is_none() {
            sinks.0.push((pos, o.vel, o.radius));
            let dir = o.vel.normalize_or(Vec3::NEG_Z);
            // Everything around it is being pulled in.
            o.suck_credit += dt * 240.0;
            let n = o.suck_credit as usize;
            o.suck_credit -= n as f32;
            let target = pos + o.vel * 0.2;
            implode(&mut fx, pos, target, o.radius * 1.6, o.radius * 3.2, n.min(12), 0.2, o.rim, 0.1);
            // The tunnel wall glows violet where it passed (a ring of embers every metre, ~1.5 s).
            let wr = o.radius.max(1.0);
            let wall = [o.rim[0] * 0.55, o.rim[1] * 0.35, o.rim[2]];
            let (s1, s2) = dir.any_orthonormal_pair();
            if let Some(mut from) = o.wall_from {
                let mut guard = 0;
                while from.distance(pos) >= 1.0 && guard < 8 {
                    guard += 1;
                    from += (pos - from).normalize_or_zero();
                    for k in 0..14 {
                        let a = k as f32 / 14.0 * std::f32::consts::TAU + from.x;
                        let off = s1 * a.cos() + s2 * a.sin();
                        fx.emit(Emit {
                            kind: PKind::Ballistic,
                            flags: PFlags::PLAIN,
                            pos: from + off * wr,
                            vel: -off * 0.4,
                            size: 0.16,
                            color: wall,
                            emissive: 2.6,
                            life: 1.5,
                            gravity: 0.0,
                            drag: 1.0,
                            ..default()
                        });
                    }
                }
                o.wall_from = Some(from);
            }
            // A subtle rumble near it (juice scales by the comfort shake setting).
            let d = eye.distance(pos);
            if d < 28.0 {
                shake.write(CameraShakeEvent { trauma: (1.0 - d / 28.0) * 0.9 * dt, dir: Vec3::ZERO });
            }
        }
        i += 1;
    }
    // The construct's own voxel visual never shows (the sphere is the orb).
    for h in f.hidden.iter_mut() {
        if h.1.is_finite() {
            h.1 -= dt;
        }
    }
    f.hidden.retain(|h| h.1 > 0.0);
    if !f.hidden.is_empty() {
        for (sc, mut vis) in &mut constructs {
            if f.hidden.iter().any(|h| h.0 == sc.id) && *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
    }
}
