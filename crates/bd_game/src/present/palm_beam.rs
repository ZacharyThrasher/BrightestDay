//! Palm energy (`Form::Hands`: kamehameha, final flash, galick gun, repulsor…) and held energy balls
//! — energy never looks like a gun (owner: "final flash gave me a super powered gun"; "make the beams
//! way more saturated and glowy … physical objects").
//!
//! * **Charge orb**: the Hands construct's voxel visual is hidden; a bright orb between the caster's
//!   palms in the ENERGY colour grows with the replicated charge and lights the hands and face. A
//!   small energy ball held in first person is drawn the same way (its world body would fill the
//!   view). Both feed the shared presence GATHER (`presence::gather::GatherFx`: size × charge).
//! * **Beams** are the sim's physical beams (`GameEvent::PalmBeam`, `bd_sim::signature::palm`): the
//!   root follows the caster's hands, the head races out, a clash ends both at the clash point.
//!   Layered shells in [`EnergyMaterial`] (`energy.wgsl`): a white-hot CORE (the centre third), the
//!   saturated element BODY with streaks flowing away from the palms and a brighter silhouette, and an
//!   additive HALO — all HDR so the camera's bloom catches them; a flare cone out of the hands (a
//!   beam straight down the caster's view never covers the screen); capped point lights along it
//!   so the ground and walls glow its colour; crackling edge sparks, rings racing down it, a flare
//!   where it hits, the tunnel wall glowing its colour for ~1.5 s where it bores.
//! * **Clash**: an unstable two-tone energy ball at the junction, growing with the heat, a big shared
//!   light, screen tremble for both casters (and anyone near), a rising two-tone roar; the sim's
//!   ordinary explosion (and a flash here) when it detonates.
//! * Comfort: brightness follows "flashes"; shake / FOV via `juice` (comfort-gated there).
//!
//! Colours: `present::signature::element_rgb` (the one element table); Light — the corps colour
//! elsewhere — reads as ki blue for energy ([`energy_rgb`]).

use super::constructs::SimConstruct;
use super::players::BodyHands;
use super::presence::gather::GatherFx;
use super::signature::element_rgb;
use super::sound::Sfx3d;
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
use bd_sim::signature::palm::{BeamPhase, BeamView};
use bd_sim::weapons::WeaponPhase;
use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::transform::TransformSystems;

// ------------------------------------------------------------------------------------ material

const ENERGY_SHADER: Handle<Shader> = uuid_handle!("7d2e4b1a-5c93-4e8f-b0a6-2f1c9d8e3a55");

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct EnergyParams {
    /// rgb = element colour, a = opacity.
    pub color: Vec4,
    /// rgb = hot colour (core tint / clash second colour), a = HDR intensity.
    pub hot: Vec4,
    /// x = time, y = mode (0 body, 1 halo, 2 core, 3 clash), z = flicker, w = length (m).
    pub misc: Vec4,
}

/// A palm-beam shell / clash ball (`energy.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct EnergyMaterial {
    #[uniform(0)]
    pub params: EnergyParams,
    pub alpha_mode: AlphaMode,
}

impl Material for EnergyMaterial {
    fn fragment_shader() -> ShaderRef {
        ENERGY_SHADER.into()
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

const BODY: f32 = 0.0;
const HALO: f32 = 1.0;
const CORE: f32 = 2.0;
const CLASH: f32 = 3.0;

fn energy(col: [f32; 3], hot: [f32; 3], mode: f32, alpha: f32, intensity: f32) -> EnergyMaterial {
    EnergyMaterial {
        params: EnergyParams {
            color: Vec4::new(col[0], col[1], col[2], alpha),
            hot: Vec4::new(hot[0], hot[1], hot[2], intensity),
            misc: Vec4::new(0.0, mode, 1.0, 1.0),
        },
        alpha_mode: if mode == HALO { AlphaMode::Add } else { AlphaMode::Blend },
    }
}

// ------------------------------------------------------------------------------------ look numbers

/// Energy colour of an element (linear RGB): the signature table, except Light — the corps colour
/// elsewhere — which reads as vivid ki blue for energy.
pub fn energy_rgb(e: Element, corps: [f32; 3]) -> [f32; 3] {
    match e {
        Element::Light => [0.1, 0.45, 1.0],
        _ => element_rgb(e, corps),
    }
}

/// Saturate a colour (push it away from its grey) for the beam body.
pub fn saturate(c: [f32; 3], k: f32) -> [f32; 3] {
    let g = (c[0] + c[1] + c[2]) / 3.0;
    let f = |x: f32| (g + (x - g) * k).max(0.0);
    [f(c[0]), f(c[1]), f(c[2])]
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
    if first_person { 0.014 + 0.05 * c * (0.7 + 0.3 * caliber) } else { (0.08 + 0.3 * c) * caliber.sqrt() }
}

/// Width envelope: snaps on (60 ms), a little flicker; `fade` 1 → 0 as it ends.
pub fn beam_envelope(t: f32, fade: f32) -> f32 {
    if !t.is_finite() || t <= 0.0 {
        return 0.0;
    }
    (t / 0.06).min(1.0) * fade.clamp(0.0, 1.0) * (1.0 + 0.05 * (t * 57.0).sin())
}

// ------------------------------------------------------------------------------------ state

/// A glowing orb between someone's palms (palm energy charging, or a small ball held in 1P).
struct Orb {
    id: ConstructId,
    owner: PlayerId,
    color: [f32; 3],
    element: Element,
    caliber: f32,
    charge: f32,
    core: Entity,
    glow: Entity,
    light: Entity,
    mats: [Handle<StandardMaterial>; 2],
    pos: Vec3,
    seen: bool,
    /// A held energy ball (first person): (diameter m, growth 0..1).
    ball: Option<(f32, f32)>,
}

struct BeamVis {
    id: u32,
    owner: PlayerId,
    construct: Option<ConstructId>,
    view: BeamView,
    prev_len: f32,
    age: f32,
    t: f32,
    origin: Vec3,
    dir: Vec3,
    color: [f32; 3],
    /// core, body cone, body, halo cone, halo, core cone, end flare
    parts: [Entity; 7],
    lights: [Entity; 2],
    mats: [Handle<EnergyMaterial>; 4],
    ending: Option<f32>,
    spark: f32,
    ring: f32,
    wall: f32,
    mine: bool,
}

struct ClashVis {
    point: Vec3,
    heat: f32,
    radius: f32,
    colors: ([f32; 3], [f32; 3]),
    ball: Entity,
    halo: Entity,
    light: Entity,
    mats: [Handle<EnergyMaterial>; 2],
    t: f32,
    roar: f32,
    seen: bool,
    mine: bool,
    gone: Option<f32>,
}

#[derive(Resource, Default)]
struct PalmFx {
    orbs: Vec<Orb>,
    beams: Vec<BeamVis>,
    clashes: Vec<ClashVis>,
    /// Beam views to apply (from events).
    queue: Vec<BeamView>,
    /// Detonations to flash: (point, radius).
    blasts: Vec<(Vec3, f32)>,
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
        load_internal_asset!(app, ENERGY_SHADER, "energy.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<EnergyMaterial>::default())
            .add_message::<GameEvents>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<GatherFx>()
            .add_message::<Sfx3d>()
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
            .add_systems(PostUpdate, (place_orbs, place_beams, place_clashes).chain().after(HandsPublishSet).before(TransformSystems::Propagate))
            .add_systems(OnExit(AppState::InGame), clear);
        use crate::warmup::WarmupAppExt;
        app.add_warmup(warmup);
    }
}

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(PalmAssets {
        sphere: meshes.add(Sphere::new(1.0).mesh().uv(28, 18)),
        cylinder: meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(28)),
        cone: meshes.add(Cone::new(1.0, 1.0).mesh().resolution(28)),
    });
}

fn orb_mat(col: [f32; 3], alpha: f32, add: bool, hot: f32) -> StandardMaterial {
    let c = if add { col } else { [0.6 + col[0] * 0.4, 0.6 + col[1] * 0.4, 0.6 + col[2] * 0.4] };
    StandardMaterial {
        base_color: Color::linear_rgba(c[0], c[1], c[2], alpha),
        emissive: LinearRgba::rgb(c[0] * hot, c[1] * hot, c[2] * hot),
        unlit: true,
        alpha_mode: if add { AlphaMode::Add } else { AlphaMode::Blend },
        ..default()
    }
}

/// Loading: every beam / orb material compiles before play (no first-kamehameha hitch).
fn warmup(world: &mut World) {
    let cone = world.resource_mut::<Assets<Mesh>>().add(Cone::new(0.3, 0.6).mesh().resolution(12));
    let cyl = world.resource_mut::<Assets<Mesh>>().add(Cylinder::new(0.3, 0.6).mesh().resolution(12));
    for (mode, mesh) in [(BODY, cone.clone()), (HALO, cyl.clone()), (CORE, cyl.clone()), (CLASH, cone)] {
        let m = world.resource_mut::<Assets<EnergyMaterial>>().add(energy([0.1, 0.45, 1.0], [1.0; 3], mode, 0.8, 4.0));
        world.spawn((Name::new("palm beam warmup"), Mesh3d(mesh), MeshMaterial3d(m), Transform::default(), NotShadowCaster, crate::warmup::WarmupItem));
    }
    for add in [false, true] {
        let m = world.resource_mut::<Assets<StandardMaterial>>().add(orb_mat([0.1, 0.45, 1.0], 0.8, add, 6.0));
        world.spawn((Name::new("palm orb warmup"), Mesh3d(cyl.clone()), MeshMaterial3d(m), Transform::default(), NotShadowCaster, crate::warmup::WarmupItem));
    }
}

fn clear(mut commands: Commands, mut f: ResMut<PalmFx>, parts: Query<Entity, With<PalmPart>>) {
    *f = PalmFx::default();
    for e in &parts {
        commands.entity(e).despawn();
    }
}

/// The sim's beam reports.
fn palm_events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut f: ResMut<PalmFx>) {
    if active.is_none() {
        events.clear();
        return;
    }
    for ev in events.read() {
        if let GameEvent::PalmBeam(v) = &ev.event
            && f.queue.len() < 256
        {
            f.queue.push(*v);
        }
    }
}

/// Hide every Hands construct's voxel visual (and a small ball held in the local first-person
/// hands); keep one orb per charging pair of hands / held small ball.
#[allow(clippy::too_many_arguments)]
fn track_palms(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    assets: Option<Res<PalmAssets>>,
    hands: Res<ViewmodelHands>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut f: ResMut<PalmFx>,
    mut q: Query<(&SimConstruct, &mut Visibility), Without<PalmPart>>,
) {
    let (Some(active), Some(assets)) = (active, assets) else { return };
    let local = active.local;
    for o in f.orbs.iter_mut() {
        o.seen = false;
    }
    for (sc, mut vis) in &mut q {
        let ball = if sc.form == Form::Ball && Some(sc.owner) == local && hands.active {
            active.world.constructs.get(sc.id).and_then(bd_sim::signature::orb::hold_view).filter(|h| !h.overhead).map(|h| (h.diameter, h.growth))
        } else {
            None
        };
        if sc.form != Form::Hands && ball.is_none() {
            continue;
        }
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        if let Some(o) = f.orbs.iter_mut().find(|o| o.id == sc.id) {
            o.seen = true;
            o.ball = ball;
            continue;
        }
        let Some(c) = active.world.constructs.get(sc.id) else { continue };
        let color = saturate(energy_rgb(c.recipe.element, corps_rgb(active.corps_of(sc.owner))), 1.25);
        let (caliber, ..) = beam_look(c.recipe.scale);
        let m_core = mats.add(orb_mat(color, 0.95, false, 14.0));
        let m_glow = mats.add(orb_mat(color, 0.55, true, 9.0));
        let part = |mesh: &Handle<Mesh>, m: &Handle<StandardMaterial>| {
            (PalmPart, Mesh3d(mesh.clone()), MeshMaterial3d(m.clone()), Transform::from_scale(Vec3::splat(0.01)), Visibility::Hidden, NotShadowCaster)
        };
        let core = commands.spawn((Name::new("PalmOrb"), part(&assets.sphere, &m_core))).id();
        let glow = commands.spawn((Name::new("PalmOrbGlow"), part(&assets.sphere, &m_glow))).id();
        let light = commands
            .spawn((
                Name::new("PalmOrbLight"),
                PalmPart,
                PointLight { color: Color::linear_rgb(color[0], color[1], color[2]), intensity: 0.0, range: 6.0, shadow_maps_enabled: false, ..default() },
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        f.orbs.push(Orb {
            id: sc.id,
            owner: sc.owner,
            color,
            element: c.recipe.element,
            caliber,
            charge: 0.0,
            core,
            glow,
            light,
            mats: [m_core, m_glow],
            pos: Vec3::ZERO,
            seen: true,
            ball,
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

type PartQuery<'w, 's> = Query<'w, 's, (&'static mut Transform, &'static mut Visibility), (With<PalmPart>, Without<SimConstruct>, Without<PlayerCamera>)>;
type Cams<'w, 's> = Query<'w, 's, &'static Transform, (With<PlayerCamera>, Without<PalmPart>, Without<SimConstruct>)>;

/// Where a caster's palms are this frame: (point, first person).
fn palm_point(owner: PlayerId, local: Option<PlayerId>, hands: &ViewmodelHands, body: &BodyHands, views: &WorldViews, cam: &Transform) -> Option<(Vec3, bool)> {
    if Some(owner) == local && hands.active {
        let mid = (hands.world[0] + hands.world[1]) * 0.5;
        return Some((mid + cam.forward() * 0.07 + cam.up() * 0.015, true));
    }
    let v = views.player(owner)?;
    let aim = Quat::from_euler(EulerRot::YXZ, v.yaw, v.pitch, 0.0) * Vec3::NEG_Z;
    match body.0.get(&owner) {
        Some(h) => Some((*h + aim * 0.25, false)),
        None => Some((v.pos + Vec3::Y * 1.3 * v.size.max(0.2) + aim * 0.5, false)),
    }
}

/// Charge orbs between the palms (+ the gather they feed).
#[allow(clippy::too_many_arguments)]
fn place_orbs(
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    (views, hands, body_hands, comfort): (Res<WorldViews>, Res<ViewmodelHands>, Res<BodyHands>, Res<ComfortSettings>),
    mut f: ResMut<PalmFx>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut fx: ResMut<FxParticles>,
    mut parts: PartQuery,
    mut lights: Query<&mut PointLight, With<PalmPart>>,
    cam: Cams,
    mut gather: MessageWriter<GatherFx>,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.05);
    let now = time.elapsed_secs();
    let cam = cam.single().ok().copied().unwrap_or_default();
    let local = active.local;
    let bright = 0.45 + 0.55 * comfort.flashes.clamp(0.0, 1.0);
    let f = &mut *f;
    for o in f.orbs.iter_mut() {
        let Some((pos, first)) = palm_point(o.owner, local, &hands, &body_hands, &views, &cam) else { continue };
        o.pos = pos;
        let (charging, level) = views.construct(o.id).map_or((false, 0.0), |c| {
            let phase = WeaponPhase::from_u8(c.weapon & 7);
            (phase == WeaponPhase::Charging, (c.weapon >> 3) as f32 / 31.0)
        });
        let firing = f.beams.iter().any(|b| b.construct == Some(o.id) && b.ending.is_none());
        let want = match o.ball {
            Some((_, g)) => 0.35 + 0.65 * g,
            None if firing => {
                if first {
                    0.3
                } else {
                    0.8
                }
            }
            None if charging => level.max(0.05),
            None => 0.0,
        };
        o.charge += (want - o.charge) * (1.0 - (-14.0 * dt).exp());
        let r = orb_radius(o.charge, o.caliber, first) * (1.0 + 0.08 * (now * 23.0).sin() * o.charge);
        let full = charging && level >= 0.99;
        for (e, k) in [(o.core, 1.0), (o.glow, 2.1 + 0.35 * (now * 9.0).sin())] {
            if let Ok((mut tf, mut vis)) = parts.get_mut(e) {
                tf.translation = pos;
                tf.scale = Vec3::splat((r * k).max(0.002));
                let v = if r > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
                if *vis != v {
                    *vis = v;
                }
            }
        }
        let hot = (6.0 + 22.0 * o.charge + if full { 10.0 * (now * 30.0).sin().abs() } else { 0.0 }) * bright;
        if let Some(mut m) = mats.get_mut(&o.mats[0]) {
            let c = o.color;
            m.emissive = LinearRgba::rgb((0.6 + c[0] * 0.4) * hot, (0.6 + c[1] * 0.4) * hot, (0.6 + c[2] * 0.4) * hot);
        }
        if let Ok((mut tf, mut vis)) = parts.get_mut(o.light) {
            // Lights the hands and the face in first person.
            tf.translation = if first { pos - cam.forward() * 0.12 } else { pos };
            let v = if o.charge > 0.02 { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != v {
                *vis = v;
            }
        }
        if let Ok(mut l) = lights.get_mut(o.light) {
            l.intensity = o.charge * if first { 30_000.0 } else { 120_000.0 * o.caliber } * bright;
            l.range = if first { 3.0 } else { 5.0 + 5.0 * o.caliber };
        }
        // The presence gather for charging palms (held balls feed it from presence::held_energy).
        if o.charge > 0.05 && o.ball.is_none() {
            let (size, charge) = (0.6 + 0.8 * o.caliber, o.charge);
            let world_at = if first { cam.translation + cam.forward() * 1.2 - cam.up() * 0.3 } else { pos };
            gather.write(GatherFx {
                at: world_at,
                element: o.element,
                owner: Some(o.owner),
                key: o.id.0,
                size,
                charge,
                color: (o.element == Element::Light).then_some(o.color),
            });
        }
        // Charging: energy crackles INTO the palms.
        if (charging || o.ball.is_some()) && o.charge > 0.1 {
            let n = if first { 1 } else { (o.charge * 3.0) as usize + 1 };
            let reach = if first { 0.3 } else { 1.2 * o.caliber.sqrt() };
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
                    emissive: 8.0 * bright,
                    life: 0.22,
                    gravity: 0.0,
                    drag: 0.0,
                    ..default()
                });
            }
        }
    }
}

/// The physical beams: shells, lights, sparks, wall glow; juice for the caster.
#[allow(clippy::too_many_arguments)]
fn place_beams(
    mut commands: Commands,
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    (views, hands, body_hands, comfort): (Res<WorldViews>, Res<ViewmodelHands>, Res<BodyHands>, Res<ComfortSettings>),
    assets: Option<Res<PalmAssets>>,
    mut f: ResMut<PalmFx>,
    mut emats: ResMut<Assets<EnergyMaterial>>,
    mut fx: ResMut<FxParticles>,
    mut parts: PartQuery,
    mut lights: Query<&mut PointLight, With<PalmPart>>,
    cam: Cams,
    (mut shake, mut fov): (MessageWriter<CameraShakeEvent>, MessageWriter<FovPunch>),
) {
    let (Some(active), Some(assets)) = (active, assets) else { return };
    let f = &mut *f;
    if f.beams.is_empty() && f.queue.is_empty() {
        return;
    }
    let dt = time.delta_secs().min(0.05);
    let now = time.elapsed_secs();
    let cam = cam.single().ok().copied().unwrap_or_default();
    let eye = cam.translation;
    let local = active.local;
    let bright = 0.45 + 0.55 * comfort.flashes.clamp(0.0, 1.0);

    // --- apply reports
    for v in std::mem::take(&mut f.queue) {
        if let Some(b) = f.beams.iter_mut().find(|b| b.id == v.id) {
            b.prev_len = b.view.len;
            b.view = v;
            b.age = 0.0;
            match v.phase {
                BeamPhase::End => b.ending = b.ending.or(Some(0.0)),
                BeamPhase::Detonate { point, radius } => {
                    b.ending = b.ending.or(Some(0.0));
                    f.blasts.push((bv(point), radius));
                }
                _ => {}
            }
            continue;
        }
        if matches!(v.phase, BeamPhase::End | BeamPhase::Detonate { .. }) || f.beams.len() >= 16 {
            continue;
        }
        let mine = Some(v.owner) == local;
        let color = saturate(energy_rgb(v.element, corps_rgb(active.corps_of(v.owner))), 1.35);
        let first = mine && hands.active;
        let body_a = if first { 0.7 } else { 0.92 };
        // Saturation first: the body stays near 1-2x (bloom catches it, tonemapping keeps the hue);
        // only the core is white-hot.
        let intensity = (1.5 + 0.9 * v.size) * bright;
        let m = [
            emats.add(energy(color, color, CORE, if first { 0.75 } else { 1.0 }, intensity * 3.5)),
            emats.add(energy(color, color, BODY, body_a, intensity)),
            emats.add(energy(color, color, HALO, if first { 0.3 } else { 0.55 }, intensity * 0.6)),
            emats.add(energy(color, [1.0; 3], BODY, 0.95, intensity * 1.6)),
        ];
        let spawn = |commands: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<EnergyMaterial>| {
            commands.spawn((Name::new("PalmBeam"), PalmPart, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_scale(Vec3::splat(0.001)), Visibility::Hidden, NotShadowCaster)).id()
        };
        let parts_e = [
            spawn(&mut commands, &assets.cylinder, &m[0]),
            spawn(&mut commands, &assets.cone, &m[1]),
            spawn(&mut commands, &assets.cylinder, &m[1]),
            spawn(&mut commands, &assets.cone, &m[2]),
            spawn(&mut commands, &assets.cylinder, &m[2]),
            spawn(&mut commands, &assets.cone, &m[0]),
            spawn(&mut commands, &assets.sphere, &m[3]),
        ];
        let light = |commands: &mut Commands| {
            commands
                .spawn((
                    Name::new("PalmBeamLight"),
                    PalmPart,
                    PointLight { color: Color::linear_rgb(color[0], color[1], color[2]), intensity: 0.0, range: 12.0, shadow_maps_enabled: false, ..default() },
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id()
        };
        let lights_e = [light(&mut commands), light(&mut commands)];
        let origin = palm_point(v.owner, local, &hands, &body_hands, &views, &cam).map_or(bv(v.origin), |p| p.0);
        f.beams.push(BeamVis {
            id: v.id,
            owner: v.owner,
            construct: v.construct,
            view: v,
            prev_len: 0.0,
            age: 0.0,
            t: 0.0,
            origin,
            dir: bv(v.dir).normalize_or(Vec3::NEG_Z),
            color,
            parts: parts_e,
            lights: lights_e,
            mats: m,
            ending: None,
            spark: 0.0,
            ring: 1.0,
            wall: 0.0,
            mine,
        });
        // Juice: the caster's kick (the viewmodel thrusts); everyone near feels it.
        let k = (v.radius / 1.5).clamp(0.2, 1.0);
        if mine {
            shake.write(CameraShakeEvent { trauma: 0.25 + 0.3 * k, dir: Vec3::ZERO });
            fov.write(FovPunch { degrees: 2.0 + 4.0 * k });
        } else if eye.distance(bv(v.origin)) < 40.0 {
            shake.write(CameraShakeEvent { trauma: (1.0 - eye.distance(bv(v.origin)) / 40.0) * 0.3 * k, dir: Vec3::ZERO });
        }
    }

    // --- draw
    let mut lit = 0;
    let mut i = 0;
    while i < f.beams.len() {
        let b = &mut f.beams[i];
        b.t += dt;
        b.age += dt;
        if let Some(e) = b.ending.as_mut() {
            *e += dt;
        }
        // No report for a while (a dropped end): let it go.
        if b.age > 0.6 && b.ending.is_none() {
            b.ending = Some(0.0);
        }
        let fade = b.ending.map_or(1.0, |e| 1.0 - e / 0.15);
        if fade <= 0.0 {
            let b = f.beams.swap_remove(i);
            for e in b.parts.iter().chain(b.lights.iter()) {
                commands.entity(*e).try_despawn();
            }
            for m in &b.mats {
                emats.remove(m);
            }
            continue;
        }
        let v = b.view;
        let (root, first) = palm_point(b.owner, local, &hands, &body_hands, &views, &cam).unwrap_or((bv(v.origin), false));
        let k_s = 1.0 - (-30.0 * dt).exp();
        b.origin = b.origin.lerp(root, k_s.max(0.6));
        let want_dir = bv(v.dir).normalize_or(b.dir);
        b.dir = b.dir.lerp(want_dir, 1.0 - (-20.0 * dt).exp()).normalize_or(want_dir);
        let (end, clash) = match v.phase {
            BeamPhase::Clash { point, .. } => (bv(point), true),
            _ => {
                // Extrapolate the head only while it is still racing out.
                let racing = v.len > b.prev_len + 0.01 && v.len < 159.0;
                let len = v.len + if racing { (v.speed * b.age).min(v.speed * 0.05) } else { 0.0 };
                (bv(v.origin) + bv(v.dir) * len, false)
            }
        };
        let from = b.origin;
        let seg = end - from;
        let len = seg.length().max(0.01);
        let dir = if clash { seg / len } else { b.dir };
        let to = if clash { end } else { from + dir * len };
        let env = beam_envelope(b.t, fade);
        let r = v.radius * env;
        let flare = (len * 0.3).clamp(0.4, 5.0).min(len);
        let rest = (len - flare).max(0.001);
        let cone_rot = Quat::from_rotation_arc(Vec3::Y, -dir);
        let rot = Quat::from_rotation_arc(Vec3::Y, dir);
        let mid = from + dir * (flare + rest * 0.5);
        let place = [
            (mid, rot, Vec3::new(r * 0.33, rest, r * 0.33)),
            (from + dir * flare * 0.5, cone_rot, Vec3::new(r, flare, r)),
            (mid, rot, Vec3::new(r, rest, r)),
            (from + dir * flare * 0.5, cone_rot, Vec3::new(r * 1.9, flare, r * 1.9)),
            (mid, rot, Vec3::new(r * 1.9, rest, r * 1.9)),
            (from + dir * flare * 0.5, cone_rot, Vec3::new(r * 0.33, flare, r * 0.33)),
            (to, Quat::from_rotation_y(now * 3.0), Vec3::splat((r * if clash { 0.01 } else { 1.6 }).max(0.001))),
        ];
        for (e, (p, q, s)) in b.parts.iter().zip(place) {
            if let Ok((mut tf, mut vis)) = parts.get_mut(*e) {
                *tf = Transform { translation: p, rotation: q, scale: s };
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
        }
        for m in &b.mats {
            if let Some(mut mat) = emats.get_mut(m) {
                mat.params.misc.x = now;
                mat.params.misc.w = rest;
            }
        }
        // Light the world along it (capped: the nearest few beams get lights).
        let lit_here = lit < 6;
        for (j, e) in b.lights.iter().enumerate() {
            let at = from + dir * len * if j == 0 { 0.25 } else { 0.8 };
            if let Ok((mut tf, mut vis)) = parts.get_mut(*e) {
                tf.translation = at;
                let want = if lit_here { Visibility::Inherited } else { Visibility::Hidden };
                if *vis != want {
                    *vis = want;
                }
            }
            if let Ok(mut l) = lights.get_mut(*e) {
                l.intensity = if lit_here { 400_000.0 * (0.4 + v.size) * env * bright } else { 0.0 };
                l.range = 10.0 + 14.0 * v.size;
            }
        }
        if lit_here {
            lit += 2;
        }
        let (s1, s2) = dir.any_orthonormal_pair();
        // Crackling edge sparks (jagged, short, hot) and energy rings racing away from the palms.
        b.spark += dt * (90.0 + 160.0 * v.radius) * env.min(1.0);
        let n = (b.spark as usize).min(30);
        b.spark -= n as f32;
        let seed = (b.t * 1000.0) as u32 ^ b.owner.0 as u32 ^ b.id;
        for kk in 0..n {
            let h = |j: u32| crate::util::hash3((seed.wrapping_add(kk as u32 * 7)) as i32, j as i32, 91);
            let a = h(2) * std::f32::consts::TAU;
            let out = s1 * a.cos() + s2 * a.sin();
            let tang = s1 * (-a.sin()) + s2 * a.cos();
            let at_end = kk % 4 == 0 && !clash;
            let p = if at_end { to } else { from + dir * (h(1) * len) + out * r * 1.05 };
            let hot = [0.7 + b.color[0] * 0.3, 0.7 + b.color[1] * 0.3, 0.7 + b.color[2] * 0.3];
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::STRETCH | PFlags::PLAIN,
                pos: p,
                vel: if at_end { (out - dir * 0.6) * (7.0 + 9.0 * h(3)) } else { out * (2.0 + 5.0 * h(3)) + tang * (h(5) - 0.5) * 14.0 + dir * 6.0 },
                size: 0.03 + 0.06 * v.radius,
                color: if kk % 2 == 0 { hot } else { b.color },
                emissive: 9.0 * bright,
                life: if at_end { 0.3 + 0.2 * h(4) } else { 0.08 + 0.08 * h(4) },
                gravity: if at_end { 6.0 } else { 0.0 },
                drag: 1.5,
                ..default()
            });
        }
        b.ring += dt;
        if b.ring >= 0.05 && env > 0.3 {
            b.ring = 0.0;
            for kk in 0..12 {
                let a = kk as f32 / 12.0 * std::f32::consts::TAU + b.t * 7.0;
                let out = s1 * a.cos() + s2 * a.sin();
                fx.emit(Emit {
                    kind: PKind::Ballistic,
                    flags: PFlags::STRETCH | PFlags::PLAIN,
                    pos: from + dir * flare.min(1.5) + out * r * 0.6,
                    vel: dir * 60.0 + out * r * 0.8,
                    size: 0.04 + 0.07 * v.radius,
                    color: b.color,
                    emissive: 10.0 * bright,
                    life: (len / 60.0).clamp(0.08, 0.5),
                    gravity: 0.0,
                    drag: 0.0,
                    ..default()
                });
            }
        }
        // Where it bores: the tunnel wall glows its colour (~1.5 s).
        b.wall += dt;
        if !clash && v.len < 159.0 && !(v.len > b.prev_len + 0.01) && b.wall >= 0.04 && env > 0.3 {
            b.wall = 0.0;
            let wall = [b.color[0] * 0.8, b.color[1] * 0.8, b.color[2] * 0.8];
            let wr = (v.radius * 1.2).max(0.6);
            for kk in 0..12 {
                let a = kk as f32 / 12.0 * std::f32::consts::TAU + b.t * 3.0;
                let off = s1 * a.cos() + s2 * a.sin();
                fx.emit(Emit {
                    kind: PKind::Ballistic,
                    flags: PFlags::PLAIN,
                    pos: to + off * wr,
                    vel: -off * 0.3,
                    size: 0.1 + 0.05 * v.radius,
                    color: wall,
                    emissive: 4.0 * bright,
                    life: 1.5,
                    gravity: 0.0,
                    drag: 1.0,
                    ..default()
                });
            }
        }
        // A rumble while it pours out (the caster; anyone standing near the struck point).
        let near = if b.mine { 0.0 } else { eye.distance(to) };
        if (b.mine || near < 20.0) && b.ending.is_none() {
            let k = if b.mine { 0.9 } else { 1.0 - near / 20.0 };
            shake.write(CameraShakeEvent { trauma: k * (0.4 + v.radius * 0.6) * dt, dir: Vec3::ZERO });
        }
        let _ = first;
        i += 1;
    }
}

/// Beam struggles: the unstable ball at the junction, a shared light, tremble, the roar.
#[allow(clippy::too_many_arguments)]
fn place_clashes(
    mut commands: Commands,
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    comfort: Res<ComfortSettings>,
    assets: Option<Res<PalmAssets>>,
    mut f: ResMut<PalmFx>,
    mut emats: ResMut<Assets<EnergyMaterial>>,
    mut parts: PartQuery,
    mut lights: Query<&mut PointLight, With<PalmPart>>,
    cam: Cams,
    (mut shake, mut fov, mut sfx): (MessageWriter<CameraShakeEvent>, MessageWriter<FovPunch>, MessageWriter<Sfx3d>),
) {
    let (Some(active), Some(assets)) = (active, assets) else { return };
    let f = &mut *f;
    if f.clashes.is_empty() && f.blasts.is_empty() && !f.beams.iter().any(|b| matches!(b.view.phase, BeamPhase::Clash { .. })) {
        return;
    }
    let dt = time.delta_secs().min(0.05);
    let now = time.elapsed_secs();
    let eye = cam.single().map_or(Vec3::ZERO, |c| c.translation);
    let local = active.local;
    let bright = 0.45 + 0.55 * comfort.flashes.clamp(0.0, 1.0);
    for c in f.clashes.iter_mut() {
        c.seen = false;
    }
    // Clash points reported by live beams (a pair shares one point).
    for b in f.beams.iter().filter(|b| b.ending.is_none()) {
        let BeamPhase::Clash { point, heat } = b.view.phase else { continue };
        let p = bv(point);
        let mine = Some(b.owner) == local;
        if let Some(c) = f.clashes.iter_mut().find(|c| c.point.distance(p) < 6.0 && c.gone.is_none()) {
            c.point = c.point.lerp(p, 0.5);
            c.heat = c.heat.max(heat);
            c.radius = c.radius.max(b.view.radius);
            if c.colors.1 == c.colors.0 {
                c.colors.1 = b.color;
            }
            c.mine |= mine;
            c.seen = true;
            continue;
        }
        let m_ball = emats.add(energy(b.color, b.color, CLASH, 0.96, 2.5 * bright));
        let m_halo = emats.add(energy(b.color, b.color, HALO, 0.6, 1.4 * bright));
        let spawn = |commands: &mut Commands, m: &Handle<EnergyMaterial>| {
            commands.spawn((Name::new("BeamClash"), PalmPart, Mesh3d(assets.sphere.clone()), MeshMaterial3d(m.clone()), Transform::from_translation(p).with_scale(Vec3::splat(0.01)), Visibility::Hidden, NotShadowCaster)).id()
        };
        let ball = spawn(&mut commands, &m_ball);
        let halo = spawn(&mut commands, &m_halo);
        let light = commands
            .spawn((
                Name::new("BeamClashLight"),
                PalmPart,
                PointLight { color: Color::linear_rgb(b.color[0], b.color[1], b.color[2]), intensity: 0.0, range: 30.0, shadow_maps_enabled: false, ..default() },
                Transform::from_translation(p),
                Visibility::Inherited,
            ))
            .id();
        f.clashes.push(ClashVis {
            point: p,
            heat,
            radius: b.view.radius,
            colors: (b.color, b.color),
            ball,
            halo,
            light,
            mats: [m_ball, m_halo],
            t: 0.0,
            roar: 0.0,
            seen: true,
            mine,
            gone: None,
        });
    }
    // Detonations: a flash (the sim's explosion does the crater / blast / sound).
    for (p, radius) in std::mem::take(&mut f.blasts) {
        let d = eye.distance(bv_f(p));
        if d < radius * 4.0 + 20.0 {
            fov.write(FovPunch { degrees: 5.0 * (1.0 - d / (radius * 4.0 + 20.0)) });
            shake.write(CameraShakeEvent { trauma: 0.6 * (1.0 - d / (radius * 4.0 + 20.0)), dir: Vec3::ZERO });
        }
        if let Some(c) = f.clashes.iter_mut().find(|c| c.point.distance(bv_f(p)) < 8.0) {
            c.gone = Some(0.0);
            c.heat = 1.0;
        }
    }
    let mut i = 0;
    while i < f.clashes.len() {
        let c = &mut f.clashes[i];
        c.t += dt;
        if !c.seen && c.gone.is_none() {
            c.gone = Some(0.0);
        }
        if let Some(g) = c.gone.as_mut() {
            *g += dt;
        }
        let collapse = c.gone.map_or(1.0, |g| 1.0 + g * 6.0 - (g / 0.25).powi(2) * 7.0);
        if collapse <= 0.0 {
            let c = f.clashes.swap_remove(i);
            for e in [c.ball, c.halo, c.light] {
                commands.entity(e).try_despawn();
            }
            for m in &c.mats {
                emats.remove(m);
            }
            continue;
        }
        let jitter = 1.0 + 0.12 * (now * 31.0).sin() * (now * 17.0).cos() + 0.1 * c.heat * (now * 53.0).sin();
        let r = c.radius * (1.6 + 2.4 * c.heat) * jitter * collapse;
        let wob = Vec3::new((now * 23.0).sin(), (now * 19.0).cos(), (now * 29.0).sin()) * 0.08 * r * c.heat;
        for (e, k) in [(c.ball, 1.0), (c.halo, 1.7)] {
            if let Ok((mut tf, mut vis)) = parts.get_mut(e) {
                tf.translation = c.point + wob;
                tf.scale = Vec3::splat((r * k).max(0.01));
                tf.rotation = Quat::from_rotation_y(now * 4.0) * Quat::from_rotation_x(now * 2.3);
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
        }
        if let Some(mut m) = emats.get_mut(&c.mats[0]) {
            let (a, b2) = c.colors;
            m.params.color = Vec4::new(a[0], a[1], a[2], 0.96);
            m.params.hot = Vec4::new(b2[0], b2[1], b2[2], (2.2 + 3.0 * c.heat) * bright);
            m.params.misc.x = now;
        }
        if let Some(mut m) = emats.get_mut(&c.mats[1]) {
            m.params.misc.x = now;
        }
        if let Ok((mut tf, _)) = parts.get_mut(c.light) {
            tf.translation = c.point;
        }
        if let Ok(mut l) = lights.get_mut(c.light) {
            l.intensity = 1_500_000.0 * (0.4 + c.heat) * collapse.min(1.0) * bright;
            l.range = 25.0 + 20.0 * c.heat;
        }
        // Screen tremble for the casters (and anyone near); juice applies comfort.
        let near = eye.distance(c.point);
        if c.gone.is_none() && (c.mine || near < 30.0) {
            let k = if c.mine { 1.0 } else { 1.0 - near / 30.0 };
            shake.write(CameraShakeEvent { trauma: k * (0.8 + 1.6 * c.heat) * dt, dir: Vec3::ZERO });
        }
        // A rising two-tone roar.
        c.roar -= dt;
        if c.roar <= 0.0 && c.gone.is_none() {
            c.roar = 0.22;
            sfx.write(Sfx3d::at("gun_railgun_fire", c.point).pitch(0.45 + 0.5 * c.heat).gain(0.55));
            sfx.write(Sfx3d::at("ring_blast", c.point).pitch(0.7 + 0.9 * c.heat).gain(0.5));
        }
        i += 1;
    }
}

fn bv_f(p: Vec3) -> Vec3 {
    p
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
        assert_eq!(beam_look(f32::NAN).0, 1.0);
    }

    #[test]
    fn envelope_snaps_on_and_fades() {
        assert_eq!(beam_envelope(0.0, 1.0), 0.0);
        assert!(beam_envelope(0.1, 1.0) > 0.9);
        assert!(beam_envelope(0.5, 0.2) < 0.25);
        assert_eq!(beam_envelope(f32::NAN, 1.0), 0.0);
    }

    #[test]
    fn energy_colours_are_saturated_and_read() {
        // Kamehameha (Light) vivid ki blue, Final Flash (Electric) golden, Galick Gun (Psychic) violet.
        let [r, g, b] = saturate(energy_rgb(Element::Light, [0.0, 1.0, 0.0]), 1.35);
        assert!(b > 0.9 && r < 0.2 && g < b * 0.6, "{r} {g} {b}");
        let [r, g, b] = saturate(energy_rgb(Element::Electric, [0.0; 3]), 1.35);
        assert!(r > 0.9 && g > 0.8 && b < 0.3, "golden {r} {g} {b}");
        let [r, g, b] = saturate(energy_rgb(Element::Psychic, [0.0; 3]), 1.35);
        assert!(b > r && r > g && g < 0.15, "violet {r} {g} {b}");
        assert!(orb_radius(1.0, 1.0, false) > orb_radius(0.0, 1.0, false) * 3.0);
        assert!(orb_radius(1.0, 2.5, true) < 0.1, "first person stays between the palms");
    }
}
