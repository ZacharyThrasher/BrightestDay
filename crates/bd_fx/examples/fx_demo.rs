//! bd_fx demo + perf harness.
//!
//! cargo run -p bd_fx --example fx_demo --release -- [--stress] [--screenshot PATH] [--shot-at SECS]
//!     [--quit-after SECS] [--view overview|hammer|kaiju|mech|portal|guns|words|train] [--solid] [--day]
//!     [--manga] [--still] [--angle RAD] [--size WxH]
//!
//! Gallery: constructs cycle forming → active (animated) → clash → cracks → shatter, plus a portal
//! pair, a chain, attract/repel fields, element trails, a smoking locomotive, guns and onomatopoeia.
//! `--stress`: 70 animated constructs (2 kaiju) with continuous shatters/re-forms, guns and words;
//! prints frame-time avg / p50 / p99 / max at exit.
//! Keys: T toggle translucency, M manga words, Space shatter everything, LMB-drag orbit, wheel zoom.

use std::path::PathBuf;
use std::sync::Arc;

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_fx::*;
use bd_shapes::{AnimState, ShapeKey, VoxelModel, build_model, demo_recipe};
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy::render::view::screenshot::{Capturing, Screenshot, save_to_disk};
use bevy::window::{PresentMode, WindowResolution};

#[derive(Resource, Clone, Debug)]
struct Args {
    stress: bool,
    screenshot: Option<PathBuf>,
    shot_at: Option<f32>,
    quit_after: Option<f32>,
    view: String,
    solid: bool,
    day: bool,
    manga: bool,
    still: bool,
    angle: Option<f32>,
    size: (u32, u32),
}

fn parse_args() -> Args {
    let mut a = Args { stress: false, screenshot: None, shot_at: None, quit_after: None, view: "overview".into(), solid: false, day: false, manga: false, still: false, angle: None, size: (1600, 900) };
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let next = |i: usize| v.get(i + 1).cloned().unwrap_or_default();
        match v[i].as_str() {
            "--stress" => a.stress = true,
            "--screenshot" => {
                a.screenshot = Some(PathBuf::from(next(i)));
                i += 1;
            }
            "--shot-at" => {
                a.shot_at = next(i).parse().ok();
                i += 1;
            }
            "--quit-after" => {
                a.quit_after = next(i).parse().ok();
                i += 1;
            }
            "--view" => {
                a.view = next(i);
                i += 1;
            }
            "--angle" => {
                a.angle = next(i).parse().ok();
                i += 1;
            }
            "--size" => {
                let s = next(i);
                if let Some((w, h)) = s.split_once('x') {
                    a.size = (w.parse().unwrap_or(1600), h.parse().unwrap_or(900));
                }
                i += 1;
            }
            "--solid" => a.solid = true,
            "--day" => a.day = true,
            "--manga" => a.manga = true,
            "--still" => a.still = true,
            _ => {}
        }
        i += 1;
    }
    a
}

fn main() {
    let args = parse_args();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "bd_fx demo".into(),
            resolution: WindowResolution::new(args.size.0, args.size.1),
            present_mode: PresentMode::AutoNoVsync,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(BdFxPlugin)
    .insert_resource(FxSettings { translucent: !args.solid, ..default() })
    .insert_resource(OnomatopoeiaSettings { language: if args.manga { OnoLanguage::Manga } else { OnoLanguage::En }, mode: OnoMode::FullComic, hold_scale: if args.view == "words" { 8.0 } else { 1.0 }, ..default() })
    .insert_resource(ClearColor(if args.day { Color::linear_rgb(0.62, 0.8, 1.0) } else { Color::linear_rgb(0.012, 0.016, 0.03) }))
    .insert_resource(args.clone())
    .init_resource::<Models>()
    .init_resource::<Perf>()
    .init_resource::<Orbit>()
    .add_systems(Startup, (setup_scene, setup_models).chain())
    .add_systems(PostStartup, |world: &mut World| {
        // Pipeline warm-up exactly as bd_game's loading phase would do it (parked in front of the camera).
        let ents = bd_fx::spawn_warmup(world);
        for (i, e) in ents.iter().enumerate() {
            world.entity_mut(*e).insert((Transform::from_xyz(i as f32 * 1.5, 1.0, 26.0), WarmupItem));
        }
    })
    .add_systems(Update, despawn_warmup)
    .add_systems(Startup, setup_gallery.after(setup_models).run_if(|a: Res<Args>| !a.stress && a.view != "words"))
    .add_systems(Startup, setup_stress.after(setup_models).run_if(|a: Res<Args>| a.stress))
    .add_systems(Update, (orbit_camera, drive_anims, cycle_gallery, gallery_extras, guns_demo, keys, stress_churn, automation, words_demo, transformer_demo));
    app.run();
}

// ------------------------------------------------------------------------------------ models

struct ModelDef {
    model: Arc<VoxelModel>,
    key: ShapeKey,
    recipe: Recipe,
}

#[derive(Resource, Default)]
struct Models {
    defs: Vec<(&'static str, ModelDef)>,
}

impl Models {
    fn get(&self, name: &str) -> &ModelDef {
        &self.defs.iter().find(|(n, _)| *n == name).expect("model").1
    }
}

fn def(form: Form, seed: u32, mechs: &[Mechanic], scale: Option<f32>, feats: &[Feature], element: Element) -> ModelDef {
    let mut r = demo_recipe(form, seed, mechs);
    if let Some(s) = scale {
        r.scale = s;
    }
    r.features = feats.to_vec();
    r.features.sort();
    r.element = element;
    let (_, m) = build_model(&r);
    ModelDef { key: ShapeKey::of(&r), model: Arc::new(m), recipe: r }
}

fn setup_models(mut models: ResMut<Models>) {
    let d = &mut models.defs;
    d.push(("hammer", def(Form::Hammer, 1, &[], None, &[], Element::Light)));
    d.push(("sword", def(Form::Sword, 5, &[], None, &[], Element::Light)));
    d.push(("kaiju", def(Form::Kaiju, 1, &[], Some(11.0), &[], Element::Atomic)));
    d.push(("mech", def(Form::Biped, 3, &[Mechanic::Piloted], Some(4.2), &[], Element::Light)));
    d.push(("quad", def(Form::Quadruped, 1, &[], Some(2.4), &[], Element::Fire)));
    d.push(("serpent", def(Form::Serpent, 5, &[], Some(5.0), &[], Element::Electric)));
    d.push(("bird", def(Form::Bird, 1, &[Mechanic::Flies], Some(2.0), &[], Element::Ice)));
    d.push(("spider", def(Form::Arachnid, 5, &[], Some(2.0), &[], Element::Poison)));
    d.push(("blob", def(Form::Blob, 1, &[], Some(1.8), &[], Element::Shadow)));
    d.push(("train", def(Form::Vehicle, 2, &[], Some(5.0), &[Feature::Wheels, Feature::Smokestack, Feature::Boiler, Feature::Pistons], Element::Light)));
    d.push(("gun", def(Form::Gun, 1, &[], Some(1.6), &[], Element::Light)));
    d.push(("wall", def(Form::Wall, 1, &[], Some(4.0), &[], Element::Light)));
    d.push(("biped", def(Form::Biped, 1, &[], Some(2.6), &[], Element::Water)));
    for (n, m) in d.iter() {
        info!("model {n}: {} voxels, {} parts", m.model.voxel_count, m.model.parts.len());
    }
}

// ------------------------------------------------------------------------------------ scene

#[derive(Component)]
struct DemoCamera;

#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
    target: Vec3,
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit { yaw: 0.6, pitch: 0.32, dist: 34.0, target: Vec3::new(0.0, 3.0, 0.0) }
    }
}

fn setup_scene(mut commands: Commands, args: Res<Args>, mut orbit: ResMut<Orbit>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let (target, dist, yaw, pitch) = match args.view.as_str() {
        "hammer" => (Vec3::new(-9.0, 1.8, 6.0), 5.5, 0.5, 0.2),
        "kaiju" => (Vec3::new(0.0, 6.0, -14.0), 26.0, 0.3, 0.12),
        "mech" => (Vec3::new(9.0, 2.2, 6.0), 8.0, -0.4, 0.15),
        "portal" => (Vec3::new(-14.0, 1.8, -4.0), 10.0, 0.9, 0.12),
        "guns" => (Vec3::new(12.5, 1.0, 16.0), 9.5, 0.0, 0.3),
        "words" => (Vec3::new(0.0, 2.0, 7.0), 9.0, 0.0, 0.1),
        "train" => (Vec3::new(14.0, 1.5, -6.0), 10.0, -0.9, 0.2),
        "creatures" => (Vec3::new(0.0, 1.5, 6.0), 12.0, 0.1, 0.25),
        "transform" => (Vec3::new(-7.0, 1.5, 14.0), 7.0, 0.6, 0.2),
        "combiner" => (Vec3::new(-2.0, 3.0, 24.0), 16.0, 0.3, 0.2),
        _ => (Vec3::new(0.0, 3.0, 0.0), if args.stress { 60.0 } else { 36.0 }, 0.6, 0.35),
    };
    *orbit = Orbit { yaw: args.angle.unwrap_or(yaw), pitch, dist, target };
    let (fog_col, sun_lux, ambient) = if args.day {
        (Color::linear_rgb(0.55, 0.72, 0.95), 3200.0, 520.0)
    } else {
        (Color::linear_rgb(0.015, 0.02, 0.04), 1400.0, 260.0)
    };
    commands.spawn((
        DemoCamera,
        Camera3d::default(),
        Hdr,
        Tonemapping::KhronosPbrNeutral,
        Bloom { intensity: 0.22, low_frequency_boost: 0.6, prefilter: BloomPrefilter { threshold: 1.0, threshold_softness: 0.35 }, ..Bloom::NATURAL },
        Msaa::Sample4,
        DistanceFog { color: fog_col, falloff: FogFalloff::from_visibility_squared(if args.day { 520.0 } else { 180.0 }), ..default() },
        Transform::from_xyz(0.0, 8.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(bevy::light::GlobalAmbientLight { color: Color::linear_rgb(0.6, 0.7, 1.0), brightness: ambient, ..default() });
    commands.spawn((
        DirectionalLight { illuminance: sun_lux, shadow_maps_enabled: true, color: if args.day { Color::srgb(1.0, 0.96, 0.88) } else { Color::srgb(0.7, 0.78, 1.0) }, ..default() },
        Transform::from_xyz(20.0, 40.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Ground: dark voxel-ish slab with a faint grid of lighter tiles.
    let ground = mats.add(StandardMaterial { base_color: if args.day { Color::srgb(0.36, 0.42, 0.34) } else { Color::srgb(0.2, 0.21, 0.25) }, perceptual_roughness: 0.9, ..default() });
    let tile = mats.add(StandardMaterial { base_color: if args.day { Color::srgb(0.42, 0.48, 0.4) } else { Color::srgb(0.26, 0.27, 0.32) }, perceptual_roughness: 0.85, ..default() });
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(400.0, 1.0, 400.0))), MeshMaterial3d(ground), Transform::from_xyz(0.0, -0.5, 0.0)));
    let tmesh = meshes.add(Cuboid::new(1.9, 0.04, 1.9));
    for x in -20..20 {
        for z in -20..20 {
            if (x + z) % 2 == 0 {
                commands.spawn((Mesh3d(tmesh.clone()), MeshMaterial3d(tile.clone()), Transform::from_xyz(x as f32 * 2.0 + 1.0, 0.0, z as f32 * 2.0 + 1.0)));
            }
        }
    }
}

fn orbit_camera(
    args: Res<Args>,
    time: Res<Time>,
    mut orbit: ResMut<Orbit>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cam: Query<&mut Transform, With<DemoCamera>>,
) {
    if !args.still && args.angle.is_none() {
        orbit.yaw += time.delta_secs() * 0.07;
    }
    if buttons.pressed(MouseButton::Left) {
        orbit.yaw -= motion.delta.x * 0.005;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.004).clamp(-0.1, 1.4);
    }
    orbit.dist = (orbit.dist * (1.0 - scroll.delta.y * 0.08)).clamp(2.0, 150.0);
    let o = &*orbit;
    let dir = Vec3::new(o.yaw.sin() * o.pitch.cos(), o.pitch.sin(), o.yaw.cos() * o.pitch.cos());
    for mut t in &mut cam {
        *t = Transform::from_translation(o.target + dir * o.dist).looking_at(o.target, Vec3::Y);
    }
}

// ------------------------------------------------------------------------------------ gallery

/// Animation driver for demo constructs.
#[derive(Component, Clone, Copy)]
struct Anim {
    kind: AnimKind,
    t: f32,
    dist: f32,
    speed: f32,
    base: Transform,
}

#[derive(Clone, Copy, PartialEq)]
enum AnimKind {
    Idle,
    Walk,
    /// Held item: float + slow spin + bob.
    Held,
    /// Swing back and forth (clash partner).
    Swing(f32),
}

/// Gallery slot that re-spawns its construct in a loop.
#[derive(Component)]
struct Slot {
    model: &'static str,
    corps: Corps,
    at: Transform,
    kind: AnimKind,
    ring: Vec3,
    form_time: f32,
    active: f32,
    phase: f32,
    current: Option<Entity>,
    clashed: bool,
    shattered: bool,
    element: Option<Element>,
    id: u64,
}

fn slot(model: &'static str, corps: Corps, at: Transform, kind: AnimKind, form_time: f32, active: f32, offset: f32) -> Slot {
    let ring = at.translation + at.rotation * Vec3::new(0.0, 1.6, 3.0);
    Slot { model, corps, at, kind, ring, form_time, active, phase: -offset, current: None, clashed: false, shattered: false, element: None, id: 0 }
}

fn setup_gallery(mut commands: Commands) {
    let y = |deg: f32| Quat::from_rotation_y(deg.to_radians());
    let mut s = vec![
        slot("hammer", Corps::Willpower, Transform::from_xyz(-9.6, 1.6, 6.0).with_rotation(y(90.0)), AnimKind::Swing(1.0), 0.2, 4.5, 0.0),
        slot("sword", Corps::Rage, Transform::from_xyz(-8.4, 1.6, 6.0).with_rotation(y(-90.0)), AnimKind::Swing(-1.0), 0.2, 4.5, 0.0),
        slot("kaiju", Corps::Willpower, Transform::from_xyz(0.0, 0.0, -14.0).with_rotation(y(20.0)), AnimKind::Walk, 1.2, 7.0, 0.8),
        slot("mech", Corps::Hope, Transform::from_xyz(9.0, 0.0, 6.0).with_rotation(y(-30.0)), AnimKind::Walk, 0.6, 5.0, 1.6),
        slot("quad", Corps::Greed, Transform::from_xyz(-3.0, 0.0, 7.0).with_rotation(y(40.0)), AnimKind::Walk, 0.6, 5.5, 2.2),
        slot("serpent", Corps::Fear, Transform::from_xyz(3.5, 0.0, 9.0).with_rotation(y(-60.0)), AnimKind::Walk, 0.6, 5.0, 3.0),
        slot("bird", Corps::Compassion, Transform::from_xyz(-5.5, 2.5, 1.0).with_rotation(y(10.0)), AnimKind::Walk, 0.6, 5.0, 1.2),
        slot("spider", Corps::Love, Transform::from_xyz(5.5, 0.0, 1.5).with_rotation(y(-20.0)), AnimKind::Walk, 0.6, 5.0, 2.6),
        slot("blob", Corps::Life, Transform::from_xyz(0.5, 0.0, 3.0), AnimKind::Idle, 0.6, 5.0, 0.4),
        slot("train", Corps::Rage, Transform::from_xyz(14.0, 0.0, -6.0).with_rotation(y(70.0)), AnimKind::Walk, 0.6, 7.0, 0.5),
        slot("wall", Corps::Compassion, Transform::from_xyz(-14.0, 0.0, 6.0).with_rotation(y(90.0)), AnimKind::Idle, 0.25, 5.0, 1.0),
    ];
    for (i, sl) in s.iter_mut().enumerate() {
        sl.element = match sl.model {
            "quad" => Some(Element::Fire),
            "serpent" => Some(Element::Electric),
            "bird" => Some(Element::Ice),
            "spider" => Some(Element::Poison),
            "blob" => Some(Element::Shadow),
            "biped" => Some(Element::Water),
            _ => None,
        };
        sl.id = i as u64 * 1000;
    }
    for sl in s {
        commands.spawn(sl);
    }
    // Portal pair.
    let p1 = Transform::from_xyz(-16.0, 1.6, -2.0).with_rotation(y(60.0));
    let p2 = Transform::from_xyz(-12.0, 1.6, -7.0).with_rotation(y(20.0));
    commands.spawn((PortalFx::new(2.8, Corps::Hope.color()), p1, PortalOpen(0.0)));
    commands.spawn((PortalFx { accent: Some([1.0, 0.6, 0.1]), ..PortalFx::new(2.8, Corps::Greed.color()) }, p2, PortalOpen(-0.6)));
    // Chain (Link) between two anchors that sway.
    commands.spawn((ChainFx::new(Vec3::new(-6.0, 3.5, -6.0), Vec3::new(-1.0, 0.6, -5.0), Corps::Willpower.color()), ChainDemo));
    // Force fields.
    commands.spawn((ForceFieldFx { radius: 2.4, attract: true, color: Corps::Love.color(), intensity: 1.0 }, Transform::from_xyz(6.0, 1.6, -5.0)));
    commands.spawn((ForceFieldFx { radius: 2.4, attract: false, color: Corps::Fear.color(), intensity: 1.0 }, Transform::from_xyz(10.5, 1.6, -3.0)));
    // Gun on a turntable.
    commands.spawn(GunDemo { t: 0.0, shots: 0, entity: None });
    // Transformer (vehicle <-> robot) + a 3-vehicle combiner.
    let alt = def(Form::Vehicle, 4, &[], Some(3.2), &[Feature::Wheels], Element::Light);
    let rig = Arc::new(bd_shapes::transform_rig(&alt.model, 4));
    let xv = ConstructVisual::new(Arc::new(rig.alt.clone()), Corps::Hope.color());
    commands.spawn((xv, Transform::from_xyz(-7.0, 0.0, 14.0).with_rotation(Quat::from_rotation_y(0.8)), TransformFx::new(rig, true), XformDemo { hold: 0.0 }));
    let rigs: Vec<bd_shapes::TransformRig> = (0..3).map(|i| bd_shapes::transform_rig(&def(Form::Vehicle, 10 + i, &[], Some(3.0), &[Feature::Wheels], Element::Light).model, 10 + i)).collect();
    let crig = Arc::new(bd_shapes::combine_rigs(&rigs));
    let base = Transform::from_xyz(-2.0, 0.0, 24.0);
    let mut sources = Vec::new();
    for (i, r) in rigs.iter().enumerate() {
        let corps = [Corps::Rage, Corps::Greed, Corps::Fear][i];
        sources.push(commands.spawn((ConstructVisual::new(Arc::new(r.alt.clone()), corps.color()), base)).id());
    }
    commands.spawn((CombineFx::new(sources.clone(), Corps::Greed.color()), CombinerDemo { rig: crig, sources, base, t: 0.0 }));
}

#[derive(Component)]
struct XformDemo {
    hold: f32,
}

#[derive(Component)]
struct CombinerDemo {
    rig: Arc<bd_shapes::CombinerRig>,
    sources: Vec<Entity>,
    base: Transform,
    t: f32,
}

fn transformer_demo(
    time: Res<Time>,
    mut xs: Query<(&mut TransformFx, &mut XformDemo)>,
    mut combos: Query<(&mut CombinerDemo, &mut CombineFx)>,
    mut tfs: Query<&mut Transform, With<ConstructVisual>>,
    mut pose: Local<Vec<glam::Affine3A>>,
) {
    let dt = time.delta_secs();
    for (mut x, mut d) in &mut xs {
        if x.t >= 1.0 {
            d.hold += dt;
            if d.hold > 1.2 {
                d.hold = 0.0;
                let to = !x.to_robot;
                x.restart(to);
            }
        }
    }
    for (mut c, mut fx) in &mut combos {
        c.t += dt / bd_shapes::COMBINE_SECONDS;
        if c.t > 1.8 {
            c.t = 0.0;
        }
        let t = c.t.min(1.0);
        fx.t = t;
        pose.resize(c.sources.len(), Default::default());
        bd_shapes::pose_combine(&c.rig, t, &mut pose);
        let base = c.base.compute_affine();
        for (i, e) in c.sources.iter().enumerate() {
            if let Ok(mut tf) = tfs.get_mut(*e) {
                let a = base * bevy::math::Affine3A::from_cols_array(&pose[i].to_cols_array());
                let (s, r, p) = a.to_scale_rotation_translation();
                *tf = Transform { translation: p, rotation: r, scale: s };
            }
        }
    }
}

#[derive(Component)]
struct PortalOpen(f32);

#[derive(Component)]
struct ChainDemo;

#[derive(Component)]
struct GunDemo {
    t: f32,
    shots: u32,
    entity: Option<Entity>,
}

fn spawn_construct(commands: &mut Commands, models: &Models, name: &str, corps: Corps, at: Transform, ring: Option<Vec3>, form_time: f32, kind: AnimKind, element: Option<Element>, seed: u32) -> Entity {
    let d = models.get(name);
    let mut v = ConstructVisual::new(d.model.clone(), corps.color()).with_key(d.key.clone());
    if let Some(acc) = d.recipe.accent {
        v.accent = Some(acc);
    }
    v.seed = seed;
    if let Some(r) = ring {
        v = v.forming_from(r);
    }
    let mut e = commands.spawn((v, at, Anim { kind, t: 0.0, dist: 0.0, speed: if kind == AnimKind::Walk { 1.6 } else { 0.0 }, base: at }));
    if ring.is_some() {
        e.insert(AutoForm::new(form_time));
    }
    if let Some(el) = element {
        e.insert(TrailFx::new(el).rate(if el == Element::Electric { 60.0 } else { 35.0 }));
    }
    e.id()
}

fn drive_anims(time: Res<Time>, mut q: Query<(&mut Anim, &mut ConstructVisual, &mut Transform)>) {
    let dt = time.delta_secs();
    for (mut a, mut v, mut tf) in &mut q {
        a.t += dt;
        let t = a.t;
        match a.kind {
            // Stopped: Locomotion at speed 0 keeps wheel/crank phase (Idle would snap it to 0).
            AnimKind::Idle => v.layers = AnimLayers::single(AnimState::Locomotion { speed: 0.0, distance: a.dist, airborne: false }, t),
            AnimKind::Walk => {
                a.dist += a.speed * dt;
                let (speed, dist) = (a.speed, a.dist);
                v.layers = AnimLayers::single(AnimState::Locomotion { speed, distance: dist, airborne: false }, t);
                // Walk in a slow circle around the base position (in place for big ones).
                let r = 0.0;
                let _ = r;
            }
            AnimKind::Held => {
                v.layers = AnimLayers::single(AnimState::Idle, t);
                tf.rotation = a.base.rotation * Quat::from_rotation_y(t * 0.8);
                tf.translation = a.base.translation + Vec3::Y * (t * 2.0).sin() * 0.1;
            }
            AnimKind::Swing(side) => {
                v.layers = AnimLayers::single(AnimState::Idle, t);
                // Wind-up and strike toward the partner every 1.5 s.
                let ph = (t % 1.5) / 1.5;
                let strike = if ph < 0.6 { -0.9 * (ph / 0.6) } else { -0.9 + 2.2 * ((ph - 0.6) / 0.15).min(1.0) - 1.3 * ((ph - 0.75) / 0.25).clamp(0.0, 1.0) };
                tf.rotation = a.base.rotation * Quat::from_rotation_x(-strike * 0.9) * Quat::from_rotation_z(side * 0.1);
                tf.translation = a.base.translation;
            }
        }
    }
}

fn cycle_gallery(
    mut commands: Commands,
    time: Res<Time>,
    models: Res<Models>,
    mut slots: Query<&mut Slot>,
    mut vis: Query<(&mut ConstructVisual, &GlobalTransform)>,
    mut clash: MessageWriter<ClashFx>,
    mut words: MessageWriter<SpawnOnomatopoeia>,
    mut bursts: MessageWriter<BurstFx>,
) {
    let dt = time.delta_secs();
    // Hammer/sword pair clash together.
    let mut pair: Vec<(Entity, [f32; 3], Vec3)> = Vec::new();
    for mut s in &mut slots {
        s.phase += dt;
        if s.phase < 0.0 {
            continue;
        }
        let total = s.form_time + s.active;
        if s.current.is_none() {
            let e = spawn_construct(&mut commands, &models, s.model, s.corps, s.at, Some(s.ring), s.form_time.max(0.15), s.kind, s.element, s.id as u32);
            s.current = Some(e);
            s.clashed = false;
            s.shattered = false;
            s.id += 1;
            continue;
        }
        let Some(e) = s.current else { continue };
        let Ok((mut v, gt)) = vis.get_mut(e) else {
            s.current = None;
            continue;
        };
        let life = s.phase;
        // Integrity decays in the last 2 s (cracks, flicker).
        let left = total - life;
        v.integrity = (left / 2.0).clamp(0.12, 1.0);
        if matches!(s.kind, AnimKind::Swing(_)) {
            pair.push((e, s.corps.color(), gt.translation()));
        }
        if !s.clashed && life > total * 0.55 && !matches!(s.kind, AnimKind::Swing(_)) {
            s.clashed = true;
            let p = gt.translation() + Vec3::Y * 1.2;
            clash.write(ClashFx { point: p, normal: Vec3::X, colors: [s.corps.color(), [1.0, 1.0, 1.0]], strength: 0.8, constructs: [Some(e), None] });
            words.write(SpawnOnomatopoeia { event_id: s.id, ..SpawnOnomatopoeia::key("clash", p, 0.9, s.corps.color()) });
        }
        if !s.shattered && life >= total {
            s.shattered = true;
            let b = &models.get(s.model).model.bounds;
            let size = (b.max - b.min).length();
            let p = gt.translation() + Vec3::Y * (size * 0.3).min(4.0);
            commands.entity(e).insert(ConstructEnd::Shatter { impulse: Vec3::new(1.5, 1.0, 0.0), point: Some(p + Vec3::new(-1.0, 0.0, 0.0)), floor: Some(0.0) });
            let key = if s.model == "kaiju" { "kaiju_stomp" } else if s.model == "wall" { "stone" } else if size > 4.0 { "hit_heavy" } else { "shatter" };
            words.write(SpawnOnomatopoeia { event_id: s.id, dir: Vec3::X, ..SpawnOnomatopoeia::key(key, p, (size / 4.0).clamp(0.6, 2.0), s.corps.color()) });
            bursts.write(BurstFx { kind: BurstKind::Dust, pos: gt.translation(), dir: Vec3::Y, scale: (size / 3.0).clamp(0.5, 3.0), color: None });
        }
        if life >= total + 1.2 {
            s.current = None;
            s.phase = 0.0;
        }
    }
    // Swing pair: clash when both strike (every 1.5 s at ~0.7).
    if pair.len() == 2 {
        let t = time.elapsed_secs();
        let prev = ((t - dt) % 1.5) / 1.5;
        let ph = (t % 1.5) / 1.5;
        if prev < 0.7 && ph >= 0.7 {
            let p = (pair[0].2 + pair[1].2) * 0.5 + Vec3::Y * 1.3;
            clash.write(ClashFx { point: p, normal: Vec3::X, colors: [pair[0].1, pair[1].1], strength: 1.0, constructs: [Some(pair[0].0), Some(pair[1].0)] });
            words.write(SpawnOnomatopoeia { event_id: (t * 10.0) as u64, dir: Vec3::Y, ..SpawnOnomatopoeia::key("clash", p, 1.0, pair[0].1) });
        }
    }
}

fn gallery_extras(time: Res<Time>, mut portals: Query<(&mut PortalFx, &mut PortalOpen)>, mut chains: Query<&mut ChainFx, With<ChainDemo>>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mut p, mut o) in &mut portals {
        o.0 += dt;
        p.open = (o.0 / 0.6).clamp(0.0, 1.0);
    }
    for mut c in &mut chains {
        c.a = Vec3::new(-6.0 + (t * 0.7).sin() * 1.2, 3.5 + (t * 1.3).sin() * 0.4, -6.0);
        c.b = Vec3::new(-1.0, 0.6 + (t * 0.9).sin().abs() * 1.5, -5.0 + (t * 0.5).cos());
        c.integrity = 0.6 + 0.4 * (t * 0.3).cos();
    }
}

fn guns_demo(
    mut commands: Commands,
    time: Res<Time>,
    models: Res<Models>,
    args: Res<Args>,
    mut q: Query<&mut GunDemo>,
    mut projectiles: Query<(Entity, &mut Transform, &Bullet), Without<ConstructVisual>>,
    mut muzzle: MessageWriter<MuzzleFx>,
    mut casing: MessageWriter<CasingFx>,
    mut beam: MessageWriter<BeamFx>,
    mut impact: MessageWriter<ImpactFx>,
    mut words: MessageWriter<SpawnOnomatopoeia>,
    mut zap: MessageWriter<ZapFx>,
) {
    let dt = time.delta_secs();
    // Clear lane at z = 16, firing along +X.
    let base = Vec3::new(8.0, 1.4, 16.0);
    let yaw = -std::f32::consts::FRAC_PI_2;
    let color = Corps::Willpower.color();
    for mut g in &mut q {
        if g.entity.is_none() {
            let at = Transform::from_translation(base).with_rotation(Quat::from_rotation_y(yaw));
            g.entity = Some(spawn_construct(&mut commands, &models, "gun", Corps::Willpower, at, None, 0.2, AnimKind::Held, None, 7));
        }
        g.t += dt;
        let dir = (Quat::from_rotation_y(yaw) * Vec3::NEG_Z).normalize();
        let muzzle_pos = base + dir * 0.9 + Vec3::Y * 0.35;
        let period = if args.stress { 0.35 } else if args.view == "guns" { 0.45 } else { 0.7 };
        if g.t >= period {
            g.t = 0.0;
            g.shots += 1;
            let kind = if args.view == "guns" { [3, 0, 3, 1, 3, 2][(g.shots % 6) as usize] } else { g.shots % 4 };
            muzzle.write(MuzzleFx { pos: muzzle_pos, dir, color, scale: if kind == 3 { 1.8 } else { 1.0 } });
            let side = Quat::from_rotation_y(yaw) * Vec3::X;
            casing.write(CasingFx { pos: base + Vec3::Y * 0.4, vel: side * 2.2 + Vec3::Y * 2.8, color, floor: Some(0.0), scale: 1.0 });
            let hit = muzzle_pos + dir * 9.0 - Vec3::Y * muzzle_pos.y;
            match kind {
                3 => {
                    beam.write(BeamFx { from: muzzle_pos, to: hit, color, width: 0.12 });
                    impact.write(ImpactFx { pos: hit, normal: Vec3::Y, surface: Some([0.3, 0.3, 0.34]), color, scale: 1.6 });
                    words.write(SpawnOnomatopoeia { event_id: g.shots as u64, ..SpawnOnomatopoeia::key("railgun", hit + Vec3::Y, 0.9, color) });
                }
                2 => {
                    zap.write(ZapFx { from: muzzle_pos, to: hit + Vec3::Y * 0.3, color: [0.6, 0.85, 1.0], duration: 0.25 });
                }
                _ => {
                    let kind = if kind == 1 { ProjectileKind::Rocket } else { ProjectileKind::Slug };
                    commands.spawn((ProjectileFx::new(kind, color), Transform::from_translation(muzzle_pos), Bullet { vel: dir * if kind == ProjectileKind::Rocket { 14.0 } else { 40.0 } - Vec3::Y * 1.5, life: 1.0, rocket: kind == ProjectileKind::Rocket }));
                    if g.shots % 8 == 0 {
                        words.write(SpawnOnomatopoeia { event_id: g.shots as u64, ..SpawnOnomatopoeia::key("gun", muzzle_pos + Vec3::Y * 0.6, 0.8, color) });
                    }
                }
            }
        }
    }
    for (e, mut tf, b) in &mut projectiles {
        tf.translation += b.vel * dt;
        if tf.translation.y <= 0.05 {
            impact.write(ImpactFx { pos: Vec3::new(tf.translation.x, 0.02, tf.translation.z), normal: Vec3::Y, surface: Some([0.3, 0.3, 0.34]), color, scale: if b.rocket { 2.5 } else { 1.0 } });
            if b.rocket {
                words.write(SpawnOnomatopoeia { event_id: e.to_bits(), ..SpawnOnomatopoeia::key("explosion", tf.translation + Vec3::Y, 1.2, color) });
            }
            commands.entity(e).despawn();
        }
    }
    let _ = &time;
}

#[derive(Component)]
struct Bullet {
    vel: Vec3,
    #[allow(dead_code)]
    life: f32,
    rocket: bool,
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<FxSettings>, mut ono: ResMut<OnomatopoeiaSettings>, mut commands: Commands, q: Query<Entity, (With<ConstructVisual>, Without<ConstructEnd>)>) {
    if keys.just_pressed(KeyCode::KeyT) {
        settings.translucent = !settings.translucent;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        ono.language = if ono.language == OnoLanguage::En { OnoLanguage::Manga } else { OnoLanguage::En };
    }
    if keys.just_pressed(KeyCode::Space) {
        for e in &q {
            commands.entity(e).insert(ConstructEnd::Shatter { impulse: Vec3::Y * 2.0, point: None, floor: Some(0.0) });
        }
    }
}

// ------------------------------------------------------------------------------------ stress

#[derive(Component)]
struct StressSlot {
    idx: usize,
}

#[derive(Resource)]
struct StressState {
    t: f32,
    next: usize,
    names: Vec<&'static str>,
    positions: Vec<Transform>,
    serial: u64,
}

const STRESS_N: usize = 70;

fn setup_stress(mut commands: Commands, models: Res<Models>) {
    let names = ["mech", "quad", "serpent", "bird", "spider", "blob", "biped", "hammer", "sword", "train", "wall", "biped", "quad", "spider"];
    let mut positions = Vec::new();
    for i in 0..STRESS_N {
        let (x, z) = ((i % 10) as f32 - 4.5, (i / 10) as f32 - 3.0);
        let mut p = Vec3::new(x * 6.5, 0.0, z * 6.5);
        if names[i % names.len()] == "bird" {
            p.y = 2.5;
        }
        if matches!(names[i % names.len()], "hammer" | "sword") {
            p.y = 1.5;
        }
        positions.push(Transform::from_translation(p).with_rotation(Quat::from_rotation_y(i as f32 * 0.7)));
    }
    // Two kaiju in the back.
    positions[5] = Transform::from_xyz(-14.0, 0.0, -30.0);
    positions[8] = Transform::from_xyz(16.0, 0.0, -30.0);
    let mut names_v: Vec<&'static str> = (0..STRESS_N).map(|i| names[i % names.len()]).collect();
    names_v[5] = "kaiju";
    names_v[8] = "kaiju";
    for i in 0..STRESS_N {
        let corps = Corps::ALL[i % 8];
        let el = [None, Some(Element::Fire), None, Some(Element::Electric), None, Some(Element::Ice), None][i % 7];
        let e = spawn_construct(&mut commands, &models, names_v[i], corps, positions[i], Some(positions[i].translation + Vec3::new(0.0, 1.6, 3.0)), 0.6, AnimKind::Walk, el, i as u32);
        commands.entity(e).insert(StressSlot { idx: i });
    }
    commands.insert_resource(StressState { t: 0.0, next: 0, names: names_v, positions, serial: 0 });
    commands.spawn(GunDemo { t: 0.0, shots: 0, entity: None });
    commands.spawn((ForceFieldFx { radius: 3.0, attract: true, color: Corps::Love.color(), intensity: 1.0 }, Transform::from_xyz(0.0, 2.0, 22.0)));
    commands.spawn((PortalFx::new(3.0, Corps::Hope.color()), Transform::from_xyz(-8.0, 1.8, 22.0)));
}

fn stress_churn(
    mut commands: Commands,
    time: Res<Time>,
    models: Res<Models>,
    state: Option<ResMut<StressState>>,
    q: Query<(Entity, &StressSlot, &GlobalTransform, &ConstructVisual), Without<ConstructEnd>>,
    mut clash: MessageWriter<ClashFx>,
    mut words: MessageWriter<SpawnOnomatopoeia>,
) {
    let Some(mut st) = state else { return };
    st.t += time.delta_secs();
    // One shatter + re-form every 0.2 s (5/s), kaiju included in the rotation.
    while st.t >= 0.2 {
        st.t -= 0.2;
        let idx = st.next % STRESS_N;
        st.next += 7;
        st.serial += 1;
        let serial = st.serial;
        for (e, s, gt, v) in &q {
            if s.idx != idx {
                continue;
            }
            let p = gt.translation() + Vec3::Y * 1.5;
            clash.write(ClashFx { point: p, normal: Vec3::X, colors: [v.corps_color, [1.0; 3]], strength: 0.8, constructs: [Some(e), None] });
            commands.entity(e).insert(ConstructEnd::Shatter { impulse: Vec3::new(2.0, 1.0, 0.0), point: Some(p), floor: Some(0.0) });
            words.write(SpawnOnomatopoeia { event_id: serial, ..SpawnOnomatopoeia::key("shatter", p, 1.0, v.corps_color) });
            let corps = Corps::ALL[(idx + serial as usize) % 8];
            let at = st.positions[idx];
            let ne = spawn_construct(&mut commands, &models, st.names[idx], corps, at, Some(at.translation + Vec3::new(0.0, 1.6, 3.0)), 0.6, AnimKind::Walk, None, serial as u32);
            commands.entity(ne).insert(StressSlot { idx });
        }
    }
}

// ------------------------------------------------------------------------------------ automation

#[derive(Resource, Default)]
struct Perf {
    t: f32,
    samples: Vec<f32>,
    shot_taken: bool,
    shot_done_at: Option<f32>,
    exiting: bool,
    max_particles: usize,
}

fn automation(
    mut commands: Commands,
    args: Res<Args>,
    real: Res<Time<Real>>,
    mut perf: ResMut<Perf>,
    stats: Res<FxStats>,
    capturing: Query<(), With<Capturing>>,
    mut exit: MessageWriter<AppExit>,
) {
    let dt = real.delta_secs();
    perf.t += dt;
    let t = perf.t;
    if t > 3.0 && !perf.exiting {
        perf.samples.push(dt * 1000.0);
        if dt > 0.016 && std::env::var_os("BDFX_HITCH").is_some() {
            eprintln!("hitch t={t:.2}s {:.1}ms spawned={} batches={} constructs={}", dt * 1000.0, stats.spawned, stats.visible_batches, stats.constructs);
        }
        perf.max_particles = perf.max_particles.max(stats.visible_batches);
    }
    if let Some(path) = &args.screenshot {
        let at = args.shot_at.unwrap_or_else(|| args.quit_after.map(|q| (q - 0.6).max(0.5)).unwrap_or(4.0));
        if !perf.shot_taken && t >= at {
            perf.shot_taken = true;
            info!("saving screenshot to {}", path.display());
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
        }
        if perf.shot_taken && perf.shot_done_at.is_none() && capturing.is_empty() {
            perf.shot_done_at = Some(t + 0.4);
        }
    }
    let Some(q) = args.quit_after else { return };
    let shot_pending = args.screenshot.is_some() && perf.shot_done_at.is_none_or(|d| t < d);
    if !perf.exiting && t >= q && (!shot_pending || t > q + 5.0) {
        perf.exiting = true;
        let mut s = perf.samples.clone();
        s.sort_by(|a, b| a.total_cmp(b));
        let n = s.len().max(1);
        let avg = s.iter().sum::<f32>() / n as f32;
        let pct = |p: f32| s.get(((n as f32 - 1.0) * p) as usize).copied().unwrap_or(0.0);
        let max = s.last().copied().unwrap_or(0.0);
        let hitches = s.iter().filter(|v| **v > 16.0).count();
        println!(
            "PERF mode={} frames={} avg_ms={:.3} p50_ms={:.3} p99_ms={:.3} max_ms={:.3} hitches16={} avg_fps={:.1} constructs={} parts={} particle_batches_peak={} spawned_last={} dropped_last={}",
            if args.stress { "stress" } else { "gallery" },
            s.len(),
            avg,
            pct(0.5),
            pct(0.99),
            max,
            hitches,
            1000.0 / avg.max(1e-3),
            stats.constructs,
            stats.part_entities,
            perf.max_particles,
            stats.spawned,
            stats.dropped
        );
        exit.write(AppExit::Success);
    }
}

/// `--view words`: a word every 0.6 s in front of the camera + performance words.
fn words_demo(args: Res<Args>, time: Res<Time>, mut t: Local<f32>, mut n: Local<u64>, mut words: MessageWriter<SpawnOnomatopoeia>, mut bursts: MessageWriter<BurstFx>) {
    if args.view != "words" {
        return;
    }
    *t += time.delta_secs();
    if *t < 0.6 {
        return;
    }
    *t = 0.0;
    *n += 1;
    const KEYS: [(&str, Corps, f32); 8] = [
        ("hit_heavy", Corps::Rage, 1.2),
        ("stone", Corps::Willpower, 0.9),
        ("explosion", Corps::Greed, 1.5),
        ("clash", Corps::Hope, 1.0),
        ("water", Corps::Love, 0.9),
        ("electric", Corps::Fear, 1.0),
        ("kaiju_stomp", Corps::Willpower, 2.0),
        ("hit_light", Corps::Compassion, 0.7),
    ];
    let (k, c, imp) = KEYS[(*n as usize) % KEYS.len()];
    let x = ((*n % 3) as f32 - 1.0) * 3.0;
    let p = Vec3::new(x, 1.2, 7.0);
    words.write(SpawnOnomatopoeia { event_id: *n, dir: Vec3::X, ..SpawnOnomatopoeia::key(k, p, imp, c.color()) });
    bursts.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::Y, scale: 1.0, color: Some(c.color()) });
    if *n % 4 == 0 {
        let key = ["perfect", "good", "parry", "slow"][((*n / 4) % 4) as usize];
        words.write(SpawnOnomatopoeia::performance(key, Corps::Willpower.color()));
    }
}
#[derive(Component)]
struct WarmupItem;

fn despawn_warmup(mut commands: Commands, time: Res<Time<Real>>, q: Query<Entity, With<WarmupItem>>) {
    if time.elapsed_secs() > 1.5 {
        for e in &q {
            commands.entity(e).despawn();
        }
    }
}