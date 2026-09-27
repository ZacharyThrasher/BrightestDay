//! Domain expansions, drawn (owner: "right now it just makes a brick dome … kind of lame"). Generic:
//! any named domain looks like its LOOK (`bd_sim::signature::domain::DomainLook` — its technique,
//! else its element); Malevolent Shrine (blade) and Unlimited Void (psychic) are showcases of the
//! same rules. One material, [`DomainMaterial`] (`domain.wgsl`):
//!
//! * **Expansion** (the dome's forming time, ~1.5 s): a black ink tide spreads from the caster's feet
//!   over the ground and the barrier closes in from the rim to the top like an eyelid, a glowing seam
//!   at its edge — everyone near the edge SEES it closing (and can dash out). The caster strikes a
//!   hand sign (viewmodel / body poses read [`expanding`]).
//! * **Outside**: a pitch-black sphere with pulsing element-coloured veins (no voxels, no bricks).
//! * **Inside** (cameras inside only; the same sphere's back faces): the domain's sky, its floor over
//!   the terrain, the fog / light tint, ambient particles, low-poly silhouettes at the rim (shrine,
//!   spires, hands…) — a rewritten world. Outside cameras see only the orb.
//! * **Pulses**: a marker ring at the victim's feet for the telegraph, then the look's strike (slashes,
//!   flame pillar, lightning, frost, grabbing hands, current, spikes, the void's white-noise overload
//!   for the victim — control is never touched), clash sparks where two domains push, shards when it
//!   shatters.
//! * Cost: 4 meshes + a few silhouettes per domain, fixed particle budgets, no per-frame allocation;
//!   every pipeline compiles during Loading ([`warmup`]). Comfort: brightness follows "flashes", shakes
//!   go through `juice`.

use super::constructs::SimConstruct;
use super::signature::element_rgb;
use super::{bv, corps_rgb};
use crate::camera::PlayerCamera;
use crate::juice::{CameraShakeEvent, ScreenFlash};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use bd_core::mechanics::{Element, Form};
use bd_core::{ConstructId, PlayerId};
use bd_fx::{Emit, FxParticles, PFlags, PKind};
use bd_sim::GameEvent;
use bd_sim::signature::domain::{DOMAIN_MIN_RADIUS, DomainEvent, DomainLook, look_of};
use bevy::asset::{RenderAssetUsages, load_internal_asset, uuid_handle};
use bevy::light::{GlobalAmbientLight, NotShadowCaster};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{DistanceFog, FogFalloff, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;

// ------------------------------------------------------------------------------------ material

const DOMAIN_SHADER: Handle<Shader> = uuid_handle!("5b8e2c71-3d49-4a6f-9e10-8c2f7a4d6b93");

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct DomainParams {
    pub color: Vec4,
    pub center: Vec4,
    pub misc: Vec4,
    pub extra: Vec4,
}

/// A domain's barrier / floor / ink / silhouettes (`domain.wgsl`; drawn double-sided).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct DomainMaterial {
    #[uniform(0)]
    pub params: DomainParams,
    pub alpha_mode: AlphaMode,
}

impl Material for DomainMaterial {
    fn fragment_shader() -> ShaderRef {
        DOMAIN_SHADER.into()
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
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The barrier is one sphere seen from both sides: outside the orb, inside the domain's sky.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

const BARRIER: f32 = 0.0;
const FLOOR: f32 = 1.0;
const INK: f32 = 2.0;
const SHAPE: f32 = 3.0;

fn look_id(l: DomainLook) -> f32 {
    DomainLook::ALL.iter().position(|x| *x == l).unwrap_or(0) as f32
}

/// The domain's colour: the element's, with the showcase looks tuned (blade: blood red; void: white-violet).
pub fn look_rgb(l: DomainLook, element: Element, corps: [f32; 3]) -> [f32; 3] {
    match l {
        DomainLook::Blade => [1.0, 0.06, 0.04],
        DomainLook::Void => [0.75, 0.6, 1.0],
        DomainLook::Light => [1.0, 0.85, 0.45],
        _ => element_rgb(element, corps),
    }
}

/// Fog inside (colour, visibility m) by look.
fn inside_fog(l: DomainLook) -> ([f32; 3], f32) {
    match l {
        DomainLook::Blade => ([0.18, 0.0, 0.0], 70.0),
        DomainLook::Void => ([0.01, 0.01, 0.03], 140.0),
        DomainLook::Fire => ([0.45, 0.12, 0.02], 60.0),
        DomainLook::Water => ([0.2, 0.45, 0.6], 90.0),
        DomainLook::Electric => ([0.05, 0.05, 0.12], 70.0),
        DomainLook::Shadow => ([0.0, 0.0, 0.01], 35.0),
        DomainLook::Ice => ([0.55, 0.7, 0.8], 80.0),
        DomainLook::Light => ([0.9, 0.8, 0.6], 110.0),
        DomainLook::Earth => ([0.2, 0.08, 0.03], 60.0),
        DomainLook::Wind => ([0.7, 0.8, 0.95], 150.0),
        DomainLook::Poison => ([0.12, 0.25, 0.04], 45.0),
        DomainLook::Atomic => ([0.1, 0.35, 0.08], 70.0),
    }
}

// ------------------------------------------------------------------------------------ state

struct DomainVis {
    id: ConstructId,
    look: DomainLook,
    color: [f32; 3],
    forming: f32,
    t: f32,
    sealed: bool,
    center: Vec3,
    radius: f32,
    barrier: Entity,
    floor: Entity,
    ink: Entity,
    shapes: [Entity; 10],
    mats: [Handle<DomainMaterial>; 4],
    heat: f32,
    flash: f32,
    gone: Option<f32>,
}

#[derive(Resource, Default)]
struct Domains {
    live: Vec<DomainVis>,
    /// (id, owner, look, element, forming secs) spawned this frame.
    spawned: Vec<(ConstructId, PlayerId, DomainLook, Element, f32, f32)>,
    events: Vec<DomainEvent>,
    /// Inside a domain: the fog / ambient we replaced (restored on leaving).
    saved: Option<(Color, FogFalloff, Color, f32)>,
    /// Void overload on the local player: seconds left.
    noise: f32,
    ambient_credit: f32,
}

#[derive(Resource)]
struct DomainAssets {
    sphere: Handle<Mesh>,
    disc: Handle<Mesh>,
    cone: Handle<Mesh>,
    cube: Handle<Mesh>,
    noise: Handle<Image>,
}

#[derive(Component)]
struct DomainPart;

#[derive(Component)]
struct NoiseOverlay;

/// Is `owner` striking the hand sign of a domain that is expanding right now (poses read this)?
pub fn expanding(active: &ActiveWorld, owner: PlayerId) -> bool {
    active
        .world
        .constructs
        .iter()
        .any(|c| c.owner == owner && c.recipe.form == Form::Dome && c.recipe.scale >= DOMAIN_MIN_RADIUS && c.gone.is_none() && !c.active())
}

pub struct DomainPlugin;

impl Plugin for DomainPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, DOMAIN_SHADER, "domain.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<DomainMaterial>::default())
            .add_message::<GameEvents>()
            .add_message::<CameraShakeEvent>()
            .add_message::<ScreenFlash>()
            .init_resource::<Domains>()
            .add_systems(Startup, assets)
            .add_systems(
                Update,
                (
                    domain_events.in_set(super::PresentSet::Events),
                    // After the construct sync (it rewrites visibility every frame).
                    (hide_domes, place_domains, pulses, inside_world).chain().after(super::PresentSet::Sync),
                ),
            )
            .add_systems(OnExit(AppState::InGame), clear);
        use crate::warmup::WarmupAppExt;
        app.add_warmup(warmup);
    }
}

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>) {
    let noise = Image::new_fill(
        Extent3d { width: 96, height: 54, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(DomainAssets {
        sphere: meshes.add(Sphere::new(1.0).mesh().uv(64, 40)),
        disc: meshes.add(Circle::new(1.0).mesh().resolution(96)),
        cone: meshes.add(Cone::new(1.0, 1.0).mesh().resolution(8)),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        noise: images.add(noise),
    });
}

fn material(color: [f32; 3], mode: f32, look: DomainLook, alpha: AlphaMode) -> DomainMaterial {
    DomainMaterial {
        params: DomainParams {
            color: Vec4::new(color[0], color[1], color[2], 1.0),
            center: Vec4::new(0.0, 0.0, 0.0, 1.0),
            misc: Vec4::new(0.0, mode, 0.0, look_id(look)),
            extra: Vec4::new(0.0, 0.0, 1.0, 0.0),
        },
        alpha_mode: alpha,
    }
}

/// Loading: every domain pipeline compiles before play (no first-domain hitch).
fn warmup(world: &mut World) {
    let sphere = world.resource_mut::<Assets<Mesh>>().add(Sphere::new(0.4).mesh().uv(12, 8));
    for (mode, alpha) in [(BARRIER, AlphaMode::Opaque), (FLOOR, AlphaMode::Blend), (INK, AlphaMode::Blend), (SHAPE, AlphaMode::Opaque)] {
        let m = world.resource_mut::<Assets<DomainMaterial>>().add(material([0.8, 0.1, 0.1], mode, DomainLook::Blade, alpha));
        world.spawn((
            Name::new("domain warmup"),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(m),
            Transform::default(),
            NotShadowCaster,
            crate::warmup::WarmupItem,
        ));
    }
}

fn clear(mut commands: Commands, mut d: ResMut<Domains>, parts: Query<Entity, Or<(With<DomainPart>, With<NoiseOverlay>)>>) {
    d.live.clear();
    d.spawned.clear();
    d.events.clear();
    d.saved = None;
    d.noise = 0.0;
    for e in &parts {
        commands.entity(e).despawn();
    }
}

fn domain_events(mut events: MessageReader<GameEvents>, mut d: ResMut<Domains>) {
    for ev in events.read() {
        match &ev.event {
            GameEvent::ConstructSpawned { id, owner, recipe, forming, .. }
                if recipe.form == Form::Dome && recipe.scale >= DOMAIN_MIN_RADIUS =>
            {
                if d.spawned.len() < 16 {
                    d.spawned.push((*id, *owner, look_of(recipe), recipe.element, *forming, recipe.scale));
                }
            }
            GameEvent::Domain(e) if d.events.len() < 128 => d.events.push(*e),
            _ => {}
        }
    }
}

/// The dome's own voxel model never shows (the orb is the barrier).
fn hide_domes(d: Res<Domains>, mut q: Query<(&SimConstruct, &mut Visibility), Without<DomainPart>>) {
    if d.live.is_empty() && d.spawned.is_empty() {
        return;
    }
    for (sc, mut vis) in &mut q {
        if sc.form == Form::Dome
            && (d.live.iter().any(|l| l.id == sc.id) || d.spawned.iter().any(|s| s.0 == sc.id))
            && *vis != Visibility::Hidden
        {
            *vis = Visibility::Hidden;
        }
    }
}

type PartQ<'w, 's> =
    Query<'w, 's, (&'static mut Transform, &'static mut Visibility), (With<DomainPart>, Without<SimConstruct>, Without<PlayerCamera>)>;

/// Barrier (eyelid → orb), ink tide, floor, silhouettes: spawned, placed, animated.
#[allow(clippy::too_many_arguments)]
fn place_domains(
    mut commands: Commands,
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    assets: Option<Res<DomainAssets>>,
    mut d: ResMut<Domains>,
    mut mats: ResMut<Assets<DomainMaterial>>,
    mut parts: PartQ,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    let (Some(active), Some(assets)) = (active, assets) else { return };
    let d = &mut *d;
    let dt = time.delta_secs().min(0.05);
    let now = time.elapsed_secs();
    let bright = 0.5 + 0.5 * comfort.flashes.clamp(0.0, 1.0);
    let eye = cam.single().map_or(Vec3::ZERO, |c| c.translation());
    // New domains.
    for (id, owner, look, element, forming, scale) in std::mem::take(&mut d.spawned) {
        if d.live.iter().any(|l| l.id == id) || d.live.len() >= 8 {
            continue;
        }
        let color = look_rgb(look, element, corps_rgb(active.corps_of(owner)));
        let m = [
            mats.add(material(color, BARRIER, look, AlphaMode::Opaque)),
            mats.add(material(color, FLOOR, look, AlphaMode::Blend)),
            mats.add(material(color, INK, look, AlphaMode::Blend)),
            mats.add(material(color, SHAPE, look, AlphaMode::Opaque)),
        ];
        let part = |commands: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<DomainMaterial>, name: &'static str| {
            commands
                .spawn((
                    Name::new(name),
                    DomainPart,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_scale(Vec3::splat(0.01)),
                    Visibility::Hidden,
                    NotShadowCaster,
                ))
                .id()
        };
        let barrier = part(&mut commands, &assets.sphere, &m[0], "DomainBarrier");
        let floor = part(&mut commands, &assets.disc, &m[1], "DomainFloor");
        let ink = part(&mut commands, &assets.disc, &m[2], "DomainInk");
        let shape_mesh =
            if matches!(look, DomainLook::Blade | DomainLook::Light | DomainLook::Atomic) { &assets.cube } else { &assets.cone };
        let shapes: [Entity; 10] = std::array::from_fn(|_| part(&mut commands, shape_mesh, &m[3], "DomainSilhouette"));
        let center = views.construct(id).map_or(Vec3::ZERO, |c| c.pos);
        d.live.push(DomainVis {
            id,
            look,
            color,
            forming: forming.max(0.2),
            t: 0.0,
            sealed: false,
            center,
            radius: scale,
            barrier,
            floor,
            ink,
            shapes,
            mats: m,
            heat: 0.0,
            flash: 0.0,
            gone: None,
        });
    }
    // Reports.
    for e in &d.events {
        match *e {
            DomainEvent::Sealed { id, .. } => {
                if let Some(l) = d.live.iter_mut().find(|l| l.id == id) {
                    l.sealed = true;
                }
            }
            DomainEvent::Clash { a, b, .. } => {
                for l in d.live.iter_mut().filter(|l| l.id == a || l.id == b) {
                    l.heat = 1.0;
                }
            }
            DomainEvent::Shatter { id, .. } => {
                if let Some(l) = d.live.iter_mut().find(|l| l.id == id) {
                    l.gone = l.gone.or(Some(0.0));
                }
            }
            DomainEvent::Strike { id, .. } => {
                if let Some(l) = d.live.iter_mut().find(|l| l.id == id) {
                    l.flash = 1.0;
                }
            }
            _ => {}
        }
    }
    let mut i = 0;
    while i < d.live.len() {
        let l = &mut d.live[i];
        l.t += dt;
        l.heat = (l.heat - dt * 1.5).max(0.0);
        l.flash = (l.flash - dt * 3.0).max(0.0);
        if let Some(c) = views.construct(l.id) {
            l.center = c.pos;
            let scale = active.world.constructs.get(l.id).map_or(l.radius, |x| x.recipe.scale);
            l.radius = scale * c.size.clamp(0.1, 5.0);
        } else if l.gone.is_none() && l.t > 0.5 {
            l.gone = Some(0.0);
        }
        if let Some(g) = l.gone.as_mut() {
            *g += dt;
        }
        let fade = l.gone.map_or(1.0, |g| 1.0 - g / 0.35);
        if fade <= 0.0 {
            let l = d.live.swap_remove(i);
            for e in [l.barrier, l.floor, l.ink].iter().chain(l.shapes.iter()) {
                commands.entity(*e).try_despawn();
            }
            for m in &l.mats {
                mats.remove(m);
            }
            continue;
        }
        let prog = if l.sealed { 1.0 } else { (l.t / l.forming).clamp(0.0, 1.0) };
        let r = l.radius;
        let inside = l.sealed && eye.distance(l.center) < r;
        // Barrier: the sphere, closing then sealed; a shatter swells and fades it.
        let swell = 1.0 + l.gone.map_or(0.0, |g| g * 0.6);
        set(&mut parts, l.barrier, Transform::from_translation(l.center).with_scale(Vec3::splat(r * swell)), true);
        // Ink: spreads with the expansion, fades once it has sealed.
        let ink_fade = if l.sealed { (1.0 - (l.t - l.forming) / 1.0).clamp(0.0, 1.0) } else { 1.0 };
        let ink_tf = Transform::from_translation(l.center + Vec3::Y * 0.06)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(r));
        set(&mut parts, l.ink, ink_tf, ink_fade > 0.01 && l.gone.is_none());
        // Floor + silhouettes: the inside world, for inside cameras only.
        let floor_tf = Transform::from_translation(l.center + Vec3::Y * 0.04)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(r));
        set(&mut parts, l.floor, floor_tf, inside);
        for (k, e) in l.shapes.iter().enumerate() {
            let a = k as f32 / 10.0 * std::f32::consts::TAU + l.id.0 as f32 * 0.37;
            let at = l.center + Vec3::new(a.cos(), 0.0, a.sin()) * r * 0.9;
            let (size, lift) = match l.look {
                // A shrine-like silhouette: wide, stepped.
                DomainLook::Blade => (Vec3::new(3.0, 5.0 + (k % 3) as f32 * 2.0, 3.0), 0.5),
                DomainLook::Earth => (Vec3::new(1.4, 9.0 + (k % 4) as f32 * 3.0, 1.4), 0.5),
                DomainLook::Shadow => (Vec3::new(0.5, 7.0 + (k % 3) as f32 * 2.5, 0.5), 0.5),
                DomainLook::Ice => (Vec3::new(1.0, 4.0 + (k % 2) as f32 * 3.0, 1.0), 0.5),
                DomainLook::Light => (Vec3::new(1.2, 12.0, 1.2), 0.5),
                DomainLook::Atomic => (Vec3::new(2.0, 6.0, 2.0), 0.5),
                _ => (Vec3::new(1.2, 2.5 + (k % 3) as f32, 1.2), 0.5),
            };
            let show = inside
                && !matches!(l.look, DomainLook::Void | DomainLook::Water | DomainLook::Wind)
                && (l.look != DomainLook::Blade || k % 2 == 0);
            let tf = Transform::from_translation(at + Vec3::Y * size.y * lift).with_rotation(Quat::from_rotation_y(a)).with_scale(size);
            set(&mut parts, *e, tf, show);
        }
        for (j, m) in l.mats.iter().enumerate() {
            if let Some(mut mat) = mats.get_mut(m) {
                let p = &mut mat.params;
                p.center = Vec4::new(l.center.x, l.center.y, l.center.z, r * swell);
                p.misc.x = now;
                p.misc.z = if j == 2 { prog } else { prog };
                p.color.w = bright;
                p.extra = Vec4::new(l.flash, l.heat, if j == 2 { ink_fade * fade } else { fade }, 0.0);
            }
        }
        i += 1;
    }
    d.events.clear();
}

fn set(parts: &mut PartQ, e: Entity, tf: Transform, show: bool) {
    if let Ok((mut t, mut v)) = parts.get_mut(e) {
        if show {
            *t = tf;
        }
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// Pulse telegraphs and strikes, clash sparks, shatter shards (particles, fixed budgets).
#[allow(clippy::too_many_arguments)]
fn pulses(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    comfort: Res<ComfortSettings>,
    mut d: ResMut<Domains>,
    mut fx: ResMut<FxParticles>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut flash: MessageWriter<ScreenFlash>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let bright = 0.5 + 0.5 * comfort.flashes.clamp(0.0, 1.0);
    let local = active.local;
    for ev in events.read() {
        let GameEvent::Domain(e) = &ev.event else { continue };
        let color_of = |id: ConstructId, d: &Domains| d.live.iter().find(|l| l.id == id).map_or([1.0, 0.2, 0.2], |l| l.color);
        match *e {
            DomainEvent::Telegraph { id, pos, radius, secs, .. } => {
                let c = color_of(id, &d);
                let p = bv(pos) + Vec3::Y * 0.1;
                for k in 0..28 {
                    let a = k as f32 / 28.0 * std::f32::consts::TAU;
                    let off = Vec3::new(a.cos(), 0.0, a.sin());
                    fx.emit(Emit {
                        kind: PKind::Ballistic,
                        flags: PFlags::PLAIN,
                        pos: p + off * radius,
                        vel: -off * (radius / secs.max(0.1)) * 0.8,
                        size: 0.14,
                        color: c,
                        emissive: 5.0 * bright,
                        life: secs.max(0.1),
                        gravity: 0.0,
                        drag: 0.0,
                        ..default()
                    });
                }
            }
            DomainEvent::Strike { id, look, pos, radius, victim } => {
                let c = color_of(id, &d);
                strike_fx(&mut fx, look, bv(pos), radius, c, bright, ev.tick as u32);
                if victim.is_some() && victim == local {
                    shake.write(CameraShakeEvent { trauma: 0.3, dir: Vec3::ZERO });
                    if look == DomainLook::Void {
                        // Sensory overload: white noise + a white wash (the player keeps full control).
                        d.noise = 1.4;
                        flash.write(ScreenFlash { color: Color::WHITE, intensity: 0.35, duration: 0.5, vignette: false });
                    }
                }
            }
            DomainEvent::Clash { point, a, b, .. } => {
                let (ca, cb) = (color_of(a, &d), color_of(b, &d));
                let p = bv(point);
                for k in 0..10 {
                    let h = |j: u32| crate::util::hash3(ev.tick as i32 + k, j as i32, 5);
                    let dir = Vec3::new(h(1) - 0.5, h(2) - 0.2, h(3) - 0.5).normalize_or(Vec3::Y);
                    fx.emit(Emit {
                        kind: PKind::Ballistic,
                        flags: PFlags::STRETCH | PFlags::PLAIN,
                        pos: p + Vec3::Y * 2.0,
                        vel: dir * 18.0,
                        size: 0.12,
                        color: if k % 2 == 0 { ca } else { cb },
                        emissive: 8.0 * bright,
                        life: 0.3,
                        gravity: 4.0,
                        drag: 1.0,
                        ..default()
                    });
                }
            }
            DomainEvent::Shatter { id, center, radius, .. } => {
                let c = color_of(id, &d);
                let p = bv(center);
                for k in 0..120 {
                    let h = |j: u32| crate::util::hash3(k, j as i32, id.0 as i32);
                    let dir = Vec3::new(h(1) - 0.5, h(2) * 0.8, h(3) - 0.5).normalize_or(Vec3::Y);
                    fx.emit(Emit {
                        kind: PKind::Ballistic,
                        flags: PFlags::PLAIN,
                        pos: p + dir * radius,
                        vel: dir * (6.0 + 10.0 * h(4)),
                        size: 0.25 + 0.3 * h(5),
                        color: if k % 3 == 0 { c } else { [0.02, 0.02, 0.03] },
                        emissive: if k % 3 == 0 { 5.0 * bright } else { 0.0 },
                        life: 1.2,
                        gravity: 9.0,
                        drag: 0.3,
                        ..default()
                    });
                }
                shake.write(CameraShakeEvent { trauma: 0.35, dir: Vec3::ZERO });
            }
            _ => {}
        }
    }
}

/// One sure-hit strike's look, generic by look.
fn strike_fx(fx: &mut FxParticles, look: DomainLook, p: Vec3, radius: f32, c: [f32; 3], bright: f32, seed: u32) {
    let h = |k: u32, j: u32| crate::util::hash3((seed ^ k.wrapping_mul(97)) as i32, j as i32, 23);
    let mut emit = |pos: Vec3, vel: Vec3, size: f32, col: [f32; 3], life: f32, gravity: f32, stretch: bool| {
        fx.emit(Emit {
            kind: PKind::Ballistic,
            flags: if stretch { PFlags::STRETCH | PFlags::PLAIN } else { PFlags::PLAIN },
            pos,
            vel,
            size,
            color: col,
            emissive: 7.0 * bright,
            life,
            gravity,
            drag: 1.0,
            ..default()
        });
    };
    match look {
        // Invisible slashes: thin crossing cuts, white-red.
        DomainLook::Blade => {
            for k in 0..3u32 {
                let a = h(k, 1) * std::f32::consts::TAU;
                let dir = Vec3::new(a.cos(), (h(k, 2) - 0.5) * 0.6, a.sin()).normalize_or(Vec3::X);
                for s in 0..10 {
                    let f = s as f32 / 9.0 - 0.5;
                    emit(
                        p + Vec3::Y * (0.4 + h(k, 3) * 1.4) + dir * f * radius * 2.0,
                        dir * 30.0,
                        0.05,
                        [1.0, 0.85, 0.85],
                        0.12,
                        0.0,
                        true,
                    );
                }
            }
        }
        DomainLook::Fire | DomainLook::Atomic => {
            for k in 0..40u32 {
                let off = Vec3::new(h(k, 1) - 0.5, 0.0, h(k, 2) - 0.5) * radius;
                emit(p + off, Vec3::Y * (8.0 + 10.0 * h(k, 3)), 0.25, c, 0.6, -2.0, false);
            }
        }
        DomainLook::Electric => {
            let top = p + Vec3::Y * 14.0;
            for s in 0..16 {
                let f = s as f32 / 15.0;
                let jag = Vec3::new(h(s, 1) - 0.5, 0.0, h(s, 2) - 0.5) * 1.2;
                emit(top.lerp(p, f) + jag, Vec3::NEG_Y * 20.0, 0.12, [0.8, 0.85, 1.0], 0.15, 0.0, true);
            }
        }
        DomainLook::Ice | DomainLook::Void => {
            for k in 0..30u32 {
                let a = h(k, 1) * std::f32::consts::TAU;
                let dir = Vec3::new(a.cos(), 0.3 + h(k, 2), a.sin()).normalize();
                emit(p + Vec3::Y * 0.5, dir * 6.0, 0.14, if look == DomainLook::Void { [1.0, 1.0, 1.0] } else { c }, 0.5, 2.0, false);
            }
        }
        DomainLook::Shadow => {
            // Grabbing hands: dark fingers rising around the victim.
            for k in 0..6u32 {
                let a = k as f32 / 6.0 * std::f32::consts::TAU;
                let base = p + Vec3::new(a.cos(), 0.0, a.sin()) * radius * 0.7;
                for s in 0..6 {
                    emit(base + Vec3::Y * (s as f32 * 0.35), (p - base) * 0.6 + Vec3::Y * 2.0, 0.18, [0.05, 0.0, 0.08], 0.5, 0.0, false);
                }
            }
        }
        DomainLook::Earth => {
            for k in 0..8u32 {
                let off = Vec3::new(h(k, 1) - 0.5, 0.0, h(k, 2) - 0.5) * radius * 1.5;
                emit(p + off, Vec3::Y * 16.0, 0.4, [0.3, 0.2, 0.15], 0.35, 30.0, true);
            }
        }
        _ => {
            // Water current / wind gust / light / poison: a swirl.
            for k in 0..32u32 {
                let a = k as f32 / 32.0 * std::f32::consts::TAU;
                let off = Vec3::new(a.cos(), 0.0, a.sin()) * radius;
                emit(p + off + Vec3::Y * 0.6, Vec3::new(-a.sin(), 0.4, a.cos()) * 7.0, 0.16, c, 0.5, 0.0, true);
            }
        }
    }
}

/// Inside a sealed domain: fog + ambient light rewritten, ambient particles, the void's noise overlay.
#[allow(clippy::too_many_arguments)]
fn inside_world(
    mut commands: Commands,
    time: Res<Time>,
    comfort: Res<ComfortSettings>,
    assets: Option<Res<DomainAssets>>,
    mut d: ResMut<Domains>,
    mut fog: Query<&mut DistanceFog, With<PlayerCamera>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut fx: ResMut<FxParticles>,
    mut images: ResMut<Assets<Image>>,
    mut overlay: Query<(Entity, &mut ImageNode), With<NoiseOverlay>>,
) {
    let dt = time.delta_secs().min(0.05);
    let d = &mut *d;
    let eye = cam.single().map_or(Vec3::ZERO, |c| c.translation());
    let inside = d
        .live
        .iter()
        .find(|l| l.sealed && l.gone.is_none() && eye.distance(l.center) < l.radius)
        .map(|l| (l.look, l.color, l.center, l.radius));
    match (inside, d.saved.clone()) {
        (Some((look, color, center, radius)), saved) => {
            let (fc, vis) = inside_fog(look);
            for mut f in &mut fog {
                if saved.is_none() {
                    d.saved = Some((f.color, f.falloff.clone(), ambient.color, ambient.brightness));
                }
                f.color = Color::linear_rgb(fc[0], fc[1], fc[2]);
                f.falloff = FogFalloff::from_visibility_squared(vis);
            }
            if d.saved.is_some() {
                ambient.color = Color::linear_rgb(0.5 + color[0] * 0.5, 0.5 + color[1] * 0.5, 0.5 + color[2] * 0.5);
                ambient.brightness = d.saved.as_ref().map_or(ambient.brightness, |s| s.3 * 0.8);
            }
            // Ambient particles around the viewer, by look.
            d.ambient_credit += dt * 36.0;
            let n = (d.ambient_credit as usize).min(8);
            d.ambient_credit -= n as f32;
            let t = time.elapsed_secs();
            for k in 0..n {
                let h = |j: u32| crate::util::hash3((t * 997.0) as i32 + k as i32, j as i32, 71);
                let off = Vec3::new(h(1) - 0.5, h(2) * 0.6 - 0.1, h(3) - 0.5) * 24.0;
                let pos = eye + off;
                if pos.distance(center) > radius {
                    continue;
                }
                let (vel, col, size, grav) = match look {
                    DomainLook::Fire | DomainLook::Atomic => (Vec3::Y * 2.5, color, 0.08, -0.5),
                    DomainLook::Ice => (Vec3::NEG_Y * 1.2, [0.9, 0.95, 1.0], 0.07, 0.0),
                    DomainLook::Electric => (Vec3::new(h(4) - 0.5, 0.0, h(5) - 0.5) * 8.0, [0.8, 0.85, 1.0], 0.05, 0.0),
                    DomainLook::Blade => (Vec3::new(0.3, -0.4, 0.2), [0.9, 0.08, 0.05], 0.07, 0.0),
                    DomainLook::Void => (Vec3::ZERO, [1.0, 1.0, 1.0], 0.05, 0.0),
                    DomainLook::Wind => (Vec3::new(9.0, 0.0, 2.0), [1.0, 1.0, 1.0], 0.05, 0.0),
                    DomainLook::Poison => (Vec3::Y * 0.8, [0.4, 0.9, 0.2], 0.1, 0.0),
                    _ => (Vec3::Y * 0.3, color, 0.07, 0.0),
                };
                fx.emit(Emit {
                    kind: PKind::Ballistic,
                    flags: PFlags::PLAIN,
                    pos,
                    vel,
                    size,
                    color: col,
                    emissive: 2.5,
                    life: 2.0,
                    gravity: grav,
                    drag: 0.2,
                    ..default()
                });
            }
        }
        (None, Some((fc, falloff, ac, ab))) => {
            for mut f in &mut fog {
                f.color = fc;
                f.falloff = falloff.clone();
            }
            ambient.color = ac;
            ambient.brightness = ab;
            d.saved = None;
        }
        (None, None) => {}
    }
    // The void's overload: animated white noise over the view (comfort: halved with flashes off).
    d.noise = (d.noise - dt).max(0.0);
    let Some(assets) = assets else { return };
    if d.noise > 0.0 {
        let a = (d.noise / 1.4).min(1.0) * (0.25 + 0.35 * comfort.flashes.clamp(0.0, 1.0));
        if let Some(mut img) = images.get_mut(&assets.noise)
            && let Some(data) = img.data.as_mut()
        {
            let seed = (time.elapsed_secs() * 60.0) as u32;
            for (k, px) in data.chunks_exact_mut(4).enumerate() {
                let v = (crate::util::hash3(k as i32, seed as i32, 9) * 255.0) as u8;
                px.copy_from_slice(&[v, v, v, (a * 255.0) as u8]);
            }
        }
        if overlay.is_empty() {
            commands.spawn((
                Name::new("DomainNoise"),
                NoiseOverlay,
                ImageNode::new(assets.noise.clone()),
                Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                Pickable::IGNORE,
                GlobalZIndex(35),
            ));
        }
    } else {
        for (e, _) in &mut overlay {
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_look_has_an_id_colour_and_fog() {
        for (i, l) in DomainLook::ALL.iter().enumerate() {
            assert_eq!(look_id(*l), i as f32);
            let (c, vis) = inside_fog(*l);
            assert!(vis > 10.0 && c.iter().all(|x| x.is_finite()));
        }
        let [r, g, b] = look_rgb(DomainLook::Blade, Element::Shadow, [0.0; 3]);
        assert!(r > 0.9 && g < 0.1 && b < 0.1, "blade is blood red");
        let [r, _, b] = look_rgb(DomainLook::Void, Element::Psychic, [0.0; 3]);
        assert!(b > 0.9 && r > 0.5, "void is white-violet");
    }
}
