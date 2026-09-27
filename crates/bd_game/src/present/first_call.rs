//! FIRST CALL / BURN ON DEATH presentation (`bd_sim::first_call`). Reads only the replicated state
//! (`World::first_call.snap`: claims, burns, rival / echo marks — the snapshot's copy on clients) and
//! `GameEvent::FirstCall`, so practice, host and client look the same. **No text anywhere.**
//!
//! * **Sigils** — every identity has a glyph: a 3×3 dot pattern from its key inside a frame by its
//!   kind (ring = character, square = power, diamond = named icon, spiked ring = signature, dot =
//!   a generic thing). The claimer's glyph in their corps colour pops over whoever uses a claimed
//!   thing (a rival's use shows the owner's frame around the rival's dots) and over constructs made
//!   from it.
//! * **Rival** — where a rival version appears, the two corps colours clash: two counter-spinning
//!   rings, shards of both colours, a clash + a low sting. The owner also gets an edge-of-screen
//!   chevron in the rival's colour pointing at them (their rival just appeared) and a UI sting.
//!   Rival constructs wear a darkened palette under their caller's colour, glowing red eyes on
//!   characters (the evil twin).
//! * **Echo** — ashen: grey, thinner, flickering constructs; a little ash puff when one is called.
//! * **Fresh** (BURN ON DEATH) — a small spark off your ring when you call something new this life.
//! * **Crumble** — on respawn with newly burned tools, ash flakes shed from your ring.
//! * **Tab** — your collection: claims in your colour, burns ashen (glanceable, next to the board).
//! * **Hotbar** — each slot's `SlotState` (fresh / burned / claimed) from the same state.
//! * `--scenario first_call` — a claim, a rival version (both views), an echo (frame sheet).

use super::PresentSet;
use super::sound::Sfx3d;
use crate::camera::PlayerCamera;
use crate::hotbar::{Hotbar, SlotBind, SlotState};
use crate::lobby::UiSet;
use crate::lobby::prefs::{Action, KeyBinds};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::{AppState, InGameMenu};
use crate::tick::FrameSet;
use bd_core::{ConstructId, PlayerId};
use bd_sim::GameEvent;
use bd_sim::first_call::keys::{CallKey, CallKind, Identities, composed_key};
use bd_sim::first_call::{BurnScope, CallEvent, mark};
use bd_sim::powers::PowerEvent;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::picking::Pickable;
use bevy::prelude::*;

pub struct FirstCallPlugin;

impl Plugin for FirstCallPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CallMeshes>()
            .init_resource::<EdgeMarks>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, call_events.in_set(PresentSet::Events))
            .add_systems(Update, (animate, draw_edge_marks).chain().in_set(FrameSet::Present).after(PresentSet::Sync))
            .add_systems(Update, hotbar_states.before(PresentSet::Sync).before(UiSet::Draw))
            .add_systems(Update, draw_collection.in_set(UiSet::Draw));
        scenario::build(app);
    }
}

// ------------------------------------------------------------------------------------ colours

/// A player's corps colour (linear, bloom-bright).
fn corps_rgb(w: &bd_sim::World, id: PlayerId) -> LinearRgba {
    let c = w.player(id).map_or([0.8, 0.8, 0.8], |p| p.corps.color());
    LinearRgba::rgb(c[0], c[1], c[2])
}

const ASH: LinearRgba = LinearRgba::rgb(0.32, 0.3, 0.29);

fn boost(c: LinearRgba, k: f32) -> LinearRgba {
    LinearRgba::rgb(c.red * k, c.green * k, c.blue * k)
}

/// The glyph of a key: 9 dots (at least three lit), deterministic.
pub fn glyph_bits(key: CallKey) -> u16 {
    let mut b = ((key ^ (key >> 17) ^ (key >> 41)) & 0x1FF) as u16;
    let mut k = key;
    while b.count_ones() < 3 {
        k = k.rotate_left(9) ^ 0x9E37_79B9_7F4A_7C15;
        b |= 1 << (k % 9);
    }
    b
}

// ------------------------------------------------------------------------------------ fx entities

#[derive(Resource, Default)]
struct CallMeshes {
    m: Option<Meshes>,
}

#[derive(Clone)]
struct Meshes {
    ring: Handle<Mesh>,
    cube: Handle<Mesh>,
    bar: Handle<Mesh>,
}

fn meshes(r: &mut CallMeshes, mesh: &mut Assets<Mesh>) -> Meshes {
    if let Some(m) = &r.m {
        return m.clone();
    }
    let m = Meshes {
        ring: mesh.add(Torus::new(0.46, 0.5)),
        cube: mesh.add(Cuboid::new(1.0, 1.0, 1.0)),
        bar: mesh.add(Cuboid::new(1.0, 0.07, 0.07)),
    };
    r.m = Some(m.clone());
    m
}

fn glow(c: LinearRgba) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba { alpha: 1.0, ..c }),
        emissive: c * 2.2,
        unlit: true,
        alpha_mode: AlphaMode::Add,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

/// What an effect follows.
#[derive(Clone, Copy)]
enum Follow {
    /// Fixed in the world.
    World,
    /// Above a player (offset up, m).
    Player(PlayerId, f32),
    /// Above a construct (offset up, m).
    Construct(ConstructId, f32),
}

/// A short-lived effect root (sigil, ring, shard, flake).
#[derive(Component)]
struct CallFx {
    t: f32,
    life: f32,
    follow: Follow,
    /// Faces the camera.
    billboard: bool,
    vel: Vec3,
    gravity: f32,
    spin: Vec3,
    /// Scale from → to over the life (ease out), times `pop` at the start.
    grow: (f32, f32),
    mats: Vec<(Handle<StandardMaterial>, LinearRgba)>,
    base: Vec3,
}

impl CallFx {
    fn new(life: f32, follow: Follow, base: Vec3) -> Self {
        Self {
            t: 0.0,
            life,
            follow,
            billboard: false,
            vel: Vec3::ZERO,
            gravity: 0.0,
            spin: Vec3::ZERO,
            grow: (1.0, 1.0),
            mats: vec![],
            base,
        }
    }
}

#[derive(Component)]
struct CallRoot;

/// Spawn a glyph (frame by kind + the key's dots) as a billboard. `frame` / `dots` colours.
#[allow(clippy::too_many_arguments)]
fn spawn_sigil(
    commands: &mut Commands,
    m: &Meshes,
    mats: &mut Assets<StandardMaterial>,
    key: CallKey,
    kind: CallKind,
    frame: LinearRgba,
    dots: LinearRgba,
    at: Vec3,
    follow: Follow,
    size: f32,
    life: f32,
) {
    let fm = mats.add(glow(frame));
    let dm = mats.add(glow(dots));
    let mut fx = CallFx::new(life, follow, at);
    fx.billboard = true;
    fx.grow = (size * 0.4, size);
    fx.mats = vec![(fm.clone(), frame), (dm.clone(), dots)];
    let root = commands
        .spawn((
            Name::new("CallSigil"),
            CallRoot,
            Transform::from_translation(at).with_scale(Vec3::splat(size * 0.4)),
            Visibility::default(),
            fx,
        ))
        .id();
    let mut kids: Vec<Entity> = Vec::new();
    let ring_t = |s: f32| Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(s));
    match kind {
        CallKind::Character => kids.push(
            commands.spawn((Mesh3d(m.ring.clone()), MeshMaterial3d(fm.clone()), ring_t(1.0), NotShadowCaster, NotShadowReceiver)).id(),
        ),
        CallKind::Signature => {
            kids.push(
                commands.spawn((Mesh3d(m.ring.clone()), MeshMaterial3d(fm.clone()), ring_t(1.0), NotShadowCaster, NotShadowReceiver)).id(),
            );
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::FRAC_PI_2;
                let d = Vec3::new(a.cos(), a.sin(), 0.0) * 0.62;
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(m.bar.clone()),
                            MeshMaterial3d(fm.clone()),
                            Transform::from_translation(d).with_rotation(Quat::from_rotation_z(a)).with_scale(Vec3::new(0.28, 1.0, 1.0)),
                            NotShadowCaster,
                            NotShadowReceiver,
                        ))
                        .id(),
                );
            }
        }
        CallKind::Power | CallKind::Icon => {
            let rot = if kind == CallKind::Icon { std::f32::consts::FRAC_PI_4 } else { 0.0 };
            for k in 0..4 {
                let a = rot + k as f32 * std::f32::consts::FRAC_PI_2;
                let n = Vec3::new(a.cos(), a.sin(), 0.0);
                kids.push(
                    commands
                        .spawn((
                            Mesh3d(m.bar.clone()),
                            MeshMaterial3d(fm.clone()),
                            Transform::from_translation(n * 0.5).with_rotation(Quat::from_rotation_z(a + std::f32::consts::FRAC_PI_2)),
                            NotShadowCaster,
                            NotShadowReceiver,
                        ))
                        .id(),
                );
            }
        }
        CallKind::Thing | CallKind::Move => kids.push(
            commands.spawn((Mesh3d(m.ring.clone()), MeshMaterial3d(fm.clone()), ring_t(0.55), NotShadowCaster, NotShadowReceiver)).id(),
        ),
    }
    let bits = glyph_bits(key);
    for i in 0..9 {
        if bits & (1 << i) == 0 {
            continue;
        }
        let (x, y) = ((i % 3) as f32 - 1.0, (i / 3) as f32 - 1.0);
        kids.push(
            commands
                .spawn((
                    Mesh3d(m.cube.clone()),
                    MeshMaterial3d(dm.clone()),
                    Transform::from_xyz(x * 0.2, y * 0.2, 0.0).with_scale(Vec3::splat(0.11)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id(),
        );
    }
    commands.entity(root).add_children(&kids);
}

/// The two-colour clash where a rival version appears.
fn spawn_clash(
    commands: &mut Commands,
    m: &Meshes,
    mats: &mut Assets<StandardMaterial>,
    rival: LinearRgba,
    owner: LinearRgba,
    at: Vec3,
    seed: u64,
) {
    for (k, (c, spin)) in
        [(boost(rival, 1.4), Vec3::new(0.0, 9.0, 3.0)), (boost(owner, 1.4), Vec3::new(4.0, -9.0, 0.0))].into_iter().enumerate()
    {
        let mat = mats.add(glow(c));
        let mut fx = CallFx::new(0.75, Follow::World, at);
        fx.spin = spin;
        fx.grow = (0.4, 3.2 - k as f32 * 0.5);
        fx.mats = vec![(mat.clone(), c)];
        let tilt = Quat::from_rotation_x(0.9 + k as f32 * 1.2);
        commands.spawn((
            Name::new("RivalClash"),
            CallRoot,
            Mesh3d(m.ring.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(at).with_rotation(tilt).with_scale(Vec3::splat(0.4)),
            NotShadowCaster,
            NotShadowReceiver,
            fx,
        ));
    }
    // shards of both colours, flung out in alternation
    let mut s = seed | 1;
    for i in 0..14 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let a = (i as f32 / 14.0) * std::f32::consts::TAU + (s % 100) as f32 * 0.01;
        let up = ((s >> 8) % 100) as f32 * 0.01 - 0.3;
        let dir = Vec3::new(a.cos(), up, a.sin()).normalize_or(Vec3::Y);
        let c = if i % 2 == 0 { rival } else { owner };
        let mat = mats.add(glow(boost(c, 1.6)));
        let mut fx = CallFx::new(0.6 + (s % 30) as f32 * 0.01, Follow::World, at);
        fx.vel = dir * (7.0 + ((s >> 16) % 50) as f32 * 0.08);
        fx.gravity = 6.0;
        fx.spin = Vec3::new(8.0, 11.0, 5.0);
        fx.grow = (0.16, 0.05);
        fx.mats = vec![(mat.clone(), c)];
        commands.spawn((
            CallRoot,
            Mesh3d(m.cube.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(at).with_scale(Vec3::splat(0.16)),
            NotShadowCaster,
            NotShadowReceiver,
            fx,
        ));
    }
}

/// Specks: `n` little cubes from `at` (ash flakes fall; sparks dart). `parent` = attach to it.
#[allow(clippy::too_many_arguments)]
fn spawn_specks(
    commands: &mut Commands,
    m: &Meshes,
    mats: &mut Assets<StandardMaterial>,
    c: LinearRgba,
    at: Vec3,
    n: u32,
    speed: f32,
    gravity: f32,
    size: f32,
    life: f32,
    seed: u64,
    parent: Option<Entity>,
) {
    let mat = mats.add(glow(c));
    let mut s = seed | 1;
    for _ in 0..n.min(40) {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let a = (s % 628) as f32 * 0.01;
        let dir = Vec3::new(a.cos(), ((s >> 10) % 100) as f32 * 0.012 - 0.2, a.sin()).normalize_or(Vec3::Y);
        let mut fx = CallFx::new(life * (0.7 + ((s >> 20) % 60) as f32 * 0.01), Follow::World, at);
        fx.vel = dir * speed * (0.5 + ((s >> 30) % 50) as f32 * 0.02);
        fx.gravity = gravity;
        fx.spin = Vec3::new(3.0, 5.0, 2.0);
        fx.grow = (size, size * 0.4);
        fx.mats = vec![(mat.clone(), c)];
        let e = commands
            .spawn((
                CallRoot,
                Mesh3d(m.cube.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(at).with_scale(Vec3::splat(size)),
                NotShadowCaster,
                NotShadowReceiver,
                fx,
            ))
            .id();
        if let Some(p) = parent {
            commands.entity(p).add_child(e);
        }
    }
}

// ------------------------------------------------------------------------------------ edge marks

/// The owner's "your rival just appeared" chevron (screen edge, the rival's colour).
#[derive(Resource, Default)]
struct EdgeMarks(Vec<EdgeMark>);

struct EdgeMark {
    at: Vec3,
    color: LinearRgba,
    t: f32,
    node: Option<Entity>,
}

#[derive(Component)]
struct EdgeNode;

fn clear(mut commands: Commands, q: Query<Entity, With<CallRoot>>, nodes: Query<Entity, With<EdgeNode>>, mut marks: ResMut<EdgeMarks>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
    for e in &nodes {
        commands.entity(e).try_despawn();
    }
    marks.0.clear();
}

// ------------------------------------------------------------------------------------ events

/// A player's ring hand in the world (third person / others).
fn hand_of(v: &crate::sim_bridge::PlayerView) -> Vec3 {
    let (s, c) = v.yaw.sin_cos();
    let right = Vec3::new(c, 0.0, -s);
    let fwd = Vec3::new(-s, 0.0, -c);
    v.pos + Vec3::Y * 1.05 * v.size.max(0.3) + right * 0.42 + fwd * 0.25
}

#[allow(clippy::too_many_arguments)]
fn call_events(
    mut commands: Commands,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mode: Res<super::camera3p::ViewMode>,
    cams: Query<(Entity, &GlobalTransform), With<PlayerCamera>>,
    mut cm: ResMut<CallMeshes>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut marks: ResMut<EdgeMarks>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let w = &active.world;
    if !w.call_rules_on() {
        events.clear();
        return;
    }
    let local = active.local;
    let first_person = !mode.third_person();
    let cam = cams.iter().next();
    let m = meshes(&mut cm, &mut mesh_assets);
    let snap = &w.first_call.snap;
    // where a player's ring is: the first-person view's ring corner, else their hand
    let ring_at = |id: PlayerId| -> (Vec3, Option<Entity>) {
        if Some(id) == local
            && first_person
            && let Some((e, _)) = cam
        {
            return (Vec3::new(0.26, -0.24, -0.62), Some(e));
        }
        (views.player(id).map(hand_of).unwrap_or(Vec3::ZERO), None)
    };
    for ev in events.read() {
        match &ev.event {
            GameEvent::FirstCall(c) => match *c {
                CallEvent::Claimed { player, key, kind } => {
                    let col = corps_rgb(w, player);
                    if let Some(v) = views.player(player) {
                        spawn_sigil(
                            &mut commands,
                            &m,
                            &mut mats,
                            key,
                            kind,
                            boost(col, 1.5),
                            boost(col, 1.5),
                            v.pos + Vec3::Y * 2.5,
                            Follow::Player(player, 2.5),
                            0.9,
                            1.6,
                        );
                    }
                    if Some(player) == local {
                        sfx.write(Sfx3d::ui("amb_glass_chime").gain(0.55).pitch(1.25));
                    }
                }
                CallEvent::Rival { player, owner, key, kind, pos } => {
                    let at = super::bv(pos);
                    let (rc, oc) = (corps_rgb(w, player), corps_rgb(w, owner));
                    spawn_clash(&mut commands, &m, &mut mats, rc, oc, at, key ^ ev.tick);
                    // the owner's frame around the rival's dots, over the rival
                    spawn_sigil(
                        &mut commands,
                        &m,
                        &mut mats,
                        key,
                        kind,
                        boost(oc, 1.5),
                        boost(rc, 1.5),
                        at + Vec3::Y * 1.4,
                        Follow::Player(player, 2.6),
                        0.9,
                        1.8,
                    );
                    sfx.write(Sfx3d::at("clash", at).gain(1.3).pitch(0.8));
                    sfx.write(Sfx3d::at("legendary_sting", at).gain(1.0).pitch(0.62));
                    if Some(owner) == local && owner != player {
                        // your rival just appeared
                        sfx.write(Sfx3d::ui("reveal_pulse").gain(0.9).pitch(0.7));
                        marks.0.push(EdgeMark { at, color: rc, t: 0.0, node: None });
                    }
                }
                CallEvent::Echo { player, .. } => {
                    let (at, parent) = ring_at(player);
                    spawn_specks(&mut commands, &m, &mut mats, ASH, at, 10, 1.2, 1.5, 0.05, 0.9, ev.tick ^ player.0 as u64, parent);
                    let p = views.player(player).map_or(Vec3::ZERO, hand_of);
                    sfx.write(Sfx3d::at("sputter", p).gain(0.6).pitch(0.75));
                    sfx.write(Sfx3d::at("mv_sand", p).gain(0.5).pitch(0.8));
                }
                CallEvent::Fresh { player, .. } => {
                    // subtle: only you see / hear your own fresh spark
                    if Some(player) == local {
                        let (at, parent) = ring_at(player);
                        spawn_specks(
                            &mut commands,
                            &m,
                            &mut mats,
                            boost(corps_rgb(w, player), 2.5),
                            at,
                            6,
                            2.2,
                            0.0,
                            0.025,
                            0.35,
                            ev.tick ^ 0xF00D,
                            parent,
                        );
                        sfx.write(Sfx3d::ui("presence_shing").gain(0.22).pitch(1.8));
                    }
                }
                CallEvent::Crumble { player, n } => {
                    let (at, parent) = ring_at(player);
                    spawn_specks(&mut commands, &m, &mut mats, ASH, at, 12 + n as u32 * 2, 0.8, 2.2, 0.045, 1.4, ev.tick ^ 0xA5A5, parent);
                    let p = views.player(player).map_or(Vec3::ZERO, hand_of);
                    sfx.write(Sfx3d::at("debris_patter", p).gain(0.55).pitch(1.3));
                    sfx.write(Sfx3d::at("mv_sand", p).gain(0.6).pitch(0.7));
                }
                CallEvent::Burned { .. } => {}
            },
            // a claimed move fired: the claimer's sigil over whoever used it
            GameEvent::Power(PowerEvent::Fired { player, mv, .. }) => {
                let Some(id) = Identities::shipped().of_move(*mv) else { continue };
                let Some(owner) = snap.owner_of(id.key) else { continue };
                let (oc, pc) = (corps_rgb(w, owner), corps_rgb(w, *player));
                if let Some(v) = views.player(*player) {
                    spawn_sigil(
                        &mut commands,
                        &m,
                        &mut mats,
                        id.key,
                        id.kind,
                        boost(oc, 1.4),
                        boost(pc, 1.4),
                        v.pos + Vec3::Y * 2.3,
                        Follow::Player(*player, 2.3),
                        0.55,
                        1.1,
                    );
                }
            }
            // a construct made from a claimed thing: the claimer's sigil over it
            GameEvent::ConstructSpawned { id, owner, recipe, pos, .. } => {
                let Some(ident) = Identities::shipped().of_recipe(recipe, false) else { continue };
                let Some(claimer) = snap.owner_of(ident.key) else { continue };
                let (oc, pc) = (corps_rgb(w, claimer), corps_rgb(w, *owner));
                let held =
                    matches!(recipe.form.category(), bd_core::mechanics::FormCategory::Held | bd_core::mechanics::FormCategory::Worn);
                let up = if held { 2.4 } else { recipe.scale.clamp(1.0, 12.0) + 0.8 };
                let follow = if held { Follow::Player(*owner, up) } else { Follow::Construct(*id, up) };
                spawn_sigil(
                    &mut commands,
                    &m,
                    &mut mats,
                    ident.key,
                    ident.kind,
                    boost(oc, 1.4),
                    boost(pc, 1.4),
                    super::bv(*pos) + Vec3::Y * up,
                    follow,
                    0.8,
                    1.8,
                );
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn animate(
    mut commands: Commands,
    time: Res<Time<Real>>,
    views: Res<WorldViews>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut q: Query<(Entity, &mut CallFx, &mut Transform, Option<&ChildOf>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs().min(0.1);
    let cam = cams.iter().next().copied();
    for (e, mut fx, mut tf, parent) in &mut q {
        fx.t += dt;
        if fx.t >= fx.life {
            commands.entity(e).try_despawn();
            continue;
        }
        let k = fx.t / fx.life.max(1e-3);
        let ease = 1.0 - (1.0 - k.min(1.0)).powi(3);
        // where it is: on its anchor (rising a little), or flying from where it started (a speck
        // parented to the camera flies in the camera's space)
        let v = fx.vel;
        fx.vel.y -= fx.gravity * dt;
        fx.base += v * dt;
        tf.translation = match fx.follow {
            Follow::World => fx.base,
            Follow::Player(p, up) => views.player(p).map_or(fx.base, |v| v.pos + Vec3::Y * up) + Vec3::Y * (0.25 * ease),
            Follow::Construct(c, up) => views.construct(c).map_or(fx.base, |v| v.pos + Vec3::Y * up) + Vec3::Y * (0.25 * ease),
        };
        let _ = parent;
        let (a, b) = fx.grow;
        tf.scale = Vec3::splat((a + (b - a) * ease).max(0.001));
        if fx.billboard {
            if let Some(c) = cam {
                tf.rotation = c.compute_transform().rotation;
            }
        } else if fx.spin != Vec3::ZERO {
            let s = fx.spin * dt;
            tf.rotation = Quat::from_euler(EulerRot::XYZ, s.x, s.y, s.z) * tf.rotation;
        }
        // fade the last 40 %
        let fade = if k > 0.6 { 1.0 - (k - 0.6) / 0.4 } else { 1.0 };
        for (h, c) in &fx.mats {
            if let Some(mut mat) = mats.get_mut(h) {
                mat.base_color = Color::LinearRgba(LinearRgba { alpha: fade, ..*c });
                mat.emissive = *c * (2.2 * fade);
            }
        }
    }
}

fn draw_edge_marks(
    mut commands: Commands,
    time: Res<Time<Real>>,
    cams: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut marks: ResMut<EdgeMarks>,
    mut nodes: Query<(&mut Node, &mut BackgroundColor, &mut UiTransform), With<EdgeNode>>,
) {
    let dt = time.delta_secs().min(0.1);
    let Some((cam, gt)) = cams.iter().next() else { return };
    let size = cam.logical_viewport_size().unwrap_or(Vec2::new(1280.0, 720.0));
    let mut keep = Vec::with_capacity(marks.0.len());
    for mut mk in marks.0.drain(..) {
        mk.t += dt;
        if mk.t > 1.8 {
            if let Some(n) = mk.node {
                commands.entity(n).try_despawn();
            }
            continue;
        }
        // direction on screen: project; behind the camera → mirror to the edge
        let fwd = gt.forward();
        let to = mk.at - gt.translation();
        let behind = to.dot(*fwd) < 0.0;
        let p = cam.world_to_viewport(gt, mk.at).ok();
        let centre = size * 0.5;
        let mut d = match p {
            Some(p) if !behind => p - centre,
            Some(p) => centre - p,
            None => {
                let r = gt.right().dot(to);
                let u = gt.up().dot(to);
                Vec2::new(r, -u)
            }
        };
        if d.length_squared() < 1.0 {
            d = Vec2::new(0.0, -1.0);
        }
        let inside = !behind && p.is_some_and(|p| p.x > 40.0 && p.y > 40.0 && p.x < size.x - 40.0 && p.y < size.y - 40.0);
        let at = if inside {
            p.unwrap_or(centre) - Vec2::splat(14.0)
        } else {
            let s = ((size.x * 0.5 - 36.0) / d.x.abs().max(1e-3)).min((size.y * 0.5 - 36.0) / d.y.abs().max(1e-3));
            centre + d * s - Vec2::splat(14.0)
        };
        let pulse = 1.0 + 0.25 * (mk.t * 14.0).sin().abs();
        let fade = if mk.t > 1.3 { 1.0 - (mk.t - 1.3) / 0.5 } else { 1.0 };
        let c = mk.color;
        let col = Color::LinearRgba(LinearRgba { red: c.red.min(1.0), green: c.green.min(1.0), blue: c.blue.min(1.0), alpha: 0.95 * fade });
        let angle = d.y.atan2(d.x);
        match mk.node.and_then(|n| nodes.get_mut(n).ok()) {
            Some((mut node, mut bg, mut ut)) => {
                node.left = px(at.x);
                node.top = px(at.y);
                *bg = BackgroundColor(col);
                ut.rotation = Rot2::radians(angle + std::f32::consts::FRAC_PI_4);
                ut.scale = Vec2::splat(pulse);
            }
            None => {
                let n = commands
                    .spawn((
                        Name::new("RivalEdge"),
                        EdgeNode,
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(at.x),
                            top: px(at.y),
                            width: px(28),
                            height: px(28),
                            border: UiRect::all(px(3)),
                            ..default()
                        },
                        BackgroundColor(col),
                        BorderColor::all(Color::srgba(0.02, 0.02, 0.03, 0.9)),
                        UiTransform {
                            rotation: Rot2::radians(angle + std::f32::consts::FRAC_PI_4),
                            scale: Vec2::splat(pulse),
                            ..default()
                        },
                        Pickable::IGNORE,
                        GlobalZIndex(70),
                    ))
                    .id();
                mk.node = Some(n);
            }
        }
        keep.push(mk);
    }
    marks.0 = keep;
}

// ------------------------------------------------------------------------------------ constructs

/// FIRST CALL HOOK (`present::constructs` sync): a rival version's darkened palette under its
/// caller's colour (red eyes on characters: the evil twin); an echo's ash. `t` = seconds (flicker).
pub fn construct_look(w: &bd_sim::World, id: ConstructId, owner_rgb: [f32; 3], vis: &mut bd_fx::ConstructVisual, t: f32) {
    let m = w.first_call.snap.mark_of(id);
    if m == 0 {
        return;
    }
    if m & mark::RIVAL != 0 {
        // the caller's own colour stays on the lines; the body goes dark and negative
        let inv = [1.0 - owner_rgb[0].min(1.0), 1.0 - owner_rgb[1].min(1.0), 1.0 - owner_rgb[2].min(1.0)];
        vis.accent = Some([inv[0] * 0.18 + 0.02, inv[1] * 0.18 + 0.02, inv[2] * 0.18 + 0.03]);
        vis.dim = vis.dim.max(0.28);
        vis.costume.eyes = Some(([3.0, 0.12, 0.08], 1.0));
        vis.costume.skin = Some(([0.16, 0.14, 0.18], 0.55));
    }
    if m & mark::ECHO != 0 {
        let a = [ASH.red, ASH.green, ASH.blue];
        vis.corps_color = [owner_rgb[0] * 0.25 + a[0] * 0.75, owner_rgb[1] * 0.25 + a[1] * 0.75, owner_rgb[2] * 0.25 + a[2] * 0.75];
        vis.accent = Some([a[0] * 0.8, a[1] * 0.8, a[2] * 0.8]);
        vis.opacity = vis.opacity.min(0.55);
        // a slow ashen flicker (the low-integrity path)
        let f = 0.45 + 0.1 * (t * 7.0 + id.0 as f32).sin();
        vis.integrity = vis.integrity.min(f);
        vis.costume.hair = Some(([0.3, 0.29, 0.28], 0.8));
    }
}

// ------------------------------------------------------------------------------------ hotbar

/// The identity a hotbar slot stands for (named moves by ref, twists by their first base, described
/// moves by verb + style under BURN everything, phrases by name: characters / icons / powers).
fn slot_key(w: &bd_sim::World, phrase: &str, bind: &SlotBind) -> Option<CallKey> {
    let ids = Identities::shipped();
    match bind {
        SlotBind::Empty => None,
        SlotBind::Move { mv, .. } => ids.of_move(*mv).map(|i| i.key),
        SlotBind::Twist(t) => match &t.a.base {
            bd_sim::powers::twist::TwistBase::Move { mv, .. } => ids.of_move(*mv).map(|i| i.key),
            bd_sim::powers::twist::TwistBase::Composed(s) => {
                (w.rules().burn == BurnScope::Everything).then(|| composed_key(&bd_sim::powers::twist::split_composed(*s).0))
            }
            _ => None,
        },
        SlotBind::Composed(s) => (w.rules().burn == BurnScope::Everything).then(|| composed_key(s)),
        SlotBind::Phrase { .. } => {
            let text = crate::hotbar::norm(phrase);
            // a character summon ("optimus prime", "giant goku"), else a named thing's name
            bd_shapes::costume::summon::summon_costume_text(&text, &text)
                .and_then(|c| bd_shapes::costume::grammar::parse(&c).and_then(|p| p.base))
                .and_then(|(ci, _)| ids.of_character(ci))
                .or_else(|| ids.of_name(&text))
                .map(|i| i.key)
        }
    }
}

/// POWER HOTBAR hook (docs/toolbox/HOTBAR.md): each slot's state for you, every frame.
fn hotbar_states(active: Option<Res<ActiveWorld>>, bar: Option<ResMut<Hotbar>>) {
    let (Some(active), Some(mut bar)) = (active, bar) else { return };
    let w = &active.world;
    let on = w.call_rules_on();
    let Some(me) = active.local else { return };
    for s in bar.slots.iter_mut() {
        let st = if !on {
            SlotState::Fresh
        } else {
            match slot_key(w, &s.phrase, &s.bind) {
                Some(k) => {
                    let v = w.call_verdict(me, k);
                    if v.echo {
                        SlotState::Burned
                    } else if v.rival.is_some() {
                        SlotState::Claimed
                    } else {
                        SlotState::Fresh
                    }
                }
                None => SlotState::Fresh,
            }
        };
        if s.state != st {
            s.state = st;
        }
    }
}

// ------------------------------------------------------------------------------------ Tab collection

#[derive(Component)]
struct CollectionRoot;

/// Most glyphs shown per row (claims, then burns).
const MAX_GLYPHS: usize = 18;

#[allow(clippy::too_many_arguments)]
fn draw_collection(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<KeyBinds>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    focus: Res<crate::input::UiFocus>,
    active: Option<Res<ActiveWorld>>,
    roots: Query<Entity, With<CollectionRoot>>,
    mut last: Local<u64>,
) {
    let tab =
        (!focus.typing && binds.pressed(Action::Scoreboard, &keys, &mouse)) || std::env::var("BD_TAB_TEST").is_ok_and(|v| v.trim() != "0");
    let show = *state.get() == AppState::InGame && !menu.0 && tab;
    let data = active.as_ref().filter(|a| show && a.world.call_rules_on()).and_then(|a| a.local.map(|me| (a, me)));
    let Some((a, me)) = data else {
        if !roots.is_empty() {
            for e in &roots {
                commands.entity(e).despawn();
            }
            *last = 0;
        }
        return;
    };
    let snap = &a.world.first_call.snap;
    let claims: Vec<(CallKey, CallKind)> = snap.claims_of(me).map(|c| (c.key, c.kind)).take(MAX_GLYPHS).collect();
    let burns: Vec<(CallKey, CallKind)> = snap.burns_of(me).map(|b| (b.key, b.kind)).take(MAX_GLYPHS).collect();
    let mut sig: u64 = 0xcbf2_9ce4_8422_2325 ^ claims.len() as u64 ^ ((burns.len() as u64) << 20);
    for (k, _) in claims.iter().chain(burns.iter()) {
        sig = (sig ^ k).wrapping_mul(0x100_0000_01b3);
    }
    if sig == *last && !roots.is_empty() {
        return;
    }
    *last = sig;
    for e in &roots {
        commands.entity(e).despawn();
    }
    if claims.is_empty() && burns.is_empty() {
        return;
    }
    let mine = a.world.player(me).map_or([1.0; 3], |p| p.corps.color());
    let mine = Color::linear_rgb(mine[0].min(1.0), mine[1].min(1.0), mine[2].min(1.0));
    let ash = Color::srgba(0.45, 0.43, 0.41, 0.55);
    commands
        .spawn((
            Name::new("CallCollection"),
            CollectionRoot,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                bottom: percent(16),
                margin: UiRect::left(px(-((MAX_GLYPHS as f32) * 15.0))),
                width: px(MAX_GLYPHS as f32 * 30.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6),
                ..default()
            },
            Pickable::IGNORE,
            GlobalZIndex(76),
        ))
        .with_children(|root| {
            for (list, color) in [(&claims, mine), (&burns, ash)] {
                if list.is_empty() {
                    continue;
                }
                root.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(6), ..default() }, Pickable::IGNORE)).with_children(
                    |row| {
                        for (k, kind) in list.iter() {
                            glyph_node(row, *k, *kind, color);
                        }
                    },
                );
            }
        });
}

/// One UI glyph: the frame by kind (rounded = character, square = power, diamond = icon, pill =
/// signature, small = thing) and the key's 3×3 dots.
fn glyph_node(p: &mut ChildSpawnerCommands, key: CallKey, kind: CallKind, color: Color) {
    let (radius, rot, size) = match kind {
        CallKind::Character => (percent(50), 0.0, 24.0),
        CallKind::Power => (px(2), 0.0, 22.0),
        CallKind::Icon => (px(2), std::f32::consts::FRAC_PI_4, 19.0),
        CallKind::Signature => (px(8), 0.0, 24.0),
        CallKind::Thing | CallKind::Move => (percent(50), 0.0, 16.0),
    };
    let bits = glyph_bits(key);
    p.spawn((
        Node {
            width: px(size),
            height: px(size),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(radius),
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::flex(3, 1.0),
            grid_template_rows: RepeatedGridTrack::flex(3, 1.0),
            padding: UiRect::all(px(2)),
            ..default()
        },
        BorderColor::all(color),
        BackgroundColor(Color::srgba(0.0, 0.01, 0.03, 0.6)),
        UiTransform { rotation: Rot2::radians(rot), ..default() },
        Pickable::IGNORE,
    ))
    .with_children(|g| {
        for i in 0..9 {
            let on = bits & (1 << i) != 0;
            g.spawn((
                Node { margin: UiRect::all(px(1)), ..default() },
                BackgroundColor(if on { color } else { Color::NONE }),
                Pickable::IGNORE,
            ));
        }
    });
}

// ------------------------------------------------------------------------------------ scenario

pub mod scenario {
    //! `--scenario first_call` (screenshots / the frame sheet `docs/toolbox/first_call_sheet.png`):
    //! FIRST CALL + BURN ON DEATH on, two players facing a patch of open ground.
    //! * `BD_FC_VIEW=rival` (default): the other player summons Optimus Prime first (claim + sigil);
    //!   ~1.2 s later YOU call him too → your evil twin (the clash of both colours, the owner's
    //!   sigil over it) beside the original.
    //! * `BD_FC_VIEW=owner`: you call him first; the other player calls him → you watch your rival
    //!   appear (the clash, your sigil over their twin, the chevron).
    //! * `BD_FC_VIEW=echo`: you call him, fall into the void, respawn (ash crumbles off the ring) and
    //!   call him again → the ashen echo.
    use super::*;
    use crate::input::LookAngles;
    use bd_core::mechanics::{Form, Recipe};

    #[derive(Resource, Default)]
    struct FcScenario {
        t: f32,
        step: u8,
        base: Option<(bd_core::Vec3, f32)>,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "first_call") {
            return;
        }
        app.init_resource::<FcScenario>().add_systems(Update, run.run_if(resource_exists::<ActiveWorld>));
    }

    fn optimus() -> (Recipe, Option<bd_core::shape::ShapeSpec>) {
        let mut r = Recipe::unknown("Optimus Prime");
        r.form = Form::Biped;
        r.scale = 2.6;
        r.mass = 400.0;
        r.element = bd_core::mechanics::Element::Fire;
        let mut shape = None;
        bd_shapes::costume::summon::apply("optimus prime", &mut r, &mut shape);
        (r.normalized(), shape)
    }

    fn ground(w: &bd_sim::World, p: bd_core::Vec3, near_y: f32) -> Option<bd_core::Vec3> {
        let hit = bd_core::voxel::raycast(&w.voxels, bd_core::Vec3::new(p.x, near_y + 6.0, p.z), bd_core::Vec3::NEG_Y, 14.0)?;
        ((hit.point.y - near_y).abs() < 3.0).then_some(hit.point)
    }

    fn view() -> String {
        std::env::var("BD_FC_VIEW").unwrap_or_else(|_| "rival".into())
    }

    fn run(
        mut sc: ResMut<FcScenario>,
        time: Res<Time<Real>>,
        mut active: ResMut<ActiveWorld>,
        mut look: ResMut<LookAngles>,
        mut mode: ResMut<crate::present::camera3p::ViewMode>,
    ) {
        if std::env::var("BD_FC_3P").is_ok_and(|v| v == "1") && !mode.pref_third {
            mode.pref_third = true;
        }
        sc.t += time.delta_secs();
        let Some(me) = active.local else { return };
        let other = active.world.players.iter().map(|p| p.id).find(|id| *id != me);
        let Some(o) = other else { return };
        let v = view();
        match sc.step {
            0 => {
                if sc.t < 1.5 || !active.local_player().is_some_and(|p| p.alive) {
                    return;
                }
                let Some(pos) = active.world.player(me).map(|p| p.mv.pos) else { return };
                // open ground: both summon spots and the other player's spot on the floor, in view
                let mut chosen = None;
                for step in 0..24 {
                    let yaw = step as f32 * std::f32::consts::TAU / 24.0;
                    let fwd = bd_core::input::look_dir(yaw, 0.0);
                    let side = bd_core::Vec3::new(-fwd.z, 0.0, fwd.x);
                    let spots = [pos + fwd * 11.0 - side * 3.2, pos + fwd * 11.0 + side * 3.2, pos + fwd * 6.0 + side * 4.5];
                    let ok = spots.iter().all(|q| ground(&active.world, *q, pos.y).is_some())
                        && spots.iter().all(|q| {
                            bd_core::voxel::raycast(
                                &active.world.voxels,
                                pos + bd_core::Vec3::Y * 1.6,
                                (*q + bd_core::Vec3::Y - pos - bd_core::Vec3::Y * 1.6).normalize_or(fwd),
                                10.5,
                            )
                            .is_none()
                        });
                    if ok {
                        chosen = Some(yaw);
                        break;
                    }
                }
                let Some(yaw) = chosen.or((sc.t > 6.0).then_some(look.yaw)) else { return };
                active.bots.clear();
                let w = &mut active.world;
                w.cfg.rules.first_call = true;
                w.cfg.rules.burn = BurnScope::Named;
                let fwd = bd_core::input::look_dir(yaw, 0.0);
                if let Some(p) = w.player_mut(o) {
                    p.mv.pos = pos + fwd * 6.0 + bd_core::Vec3::new(-fwd.z, 0.0, fwd.x) * 4.5;
                    p.mv.vel = bd_core::Vec3::ZERO;
                    p.input.yaw = yaw + std::f32::consts::PI;
                    p.spawn_shield = 0.0;
                }
                let max = w.cfg.rules.willpower_max();
                for p in w.players.iter_mut() {
                    p.willpower = max;
                }
                look.yaw = yaw;
                look.pitch = -0.1;
                sc.base = Some((pos, yaw));
                sc.step = 1;
                sc.t = 0.0;
                info!("first_call scenario: set up ({}), yaw {:.0}", view(), yaw.to_degrees());
            }
            1 if sc.t > 0.4 => {
                let Some((pos, yaw)) = sc.base else { return };
                let fwd = bd_core::input::look_dir(yaw, 0.0);
                let side = bd_core::Vec3::new(-fwd.z, 0.0, fwd.x);
                let (r, shape) = optimus();
                let first = if v == "rival" { o } else { me };
                let spot = pos + fwd * 11.0 - side * 3.2;
                let eye = active.world.player(first).map_or(pos, |p| p.mv.pos) + bd_core::Vec3::Y * 1.6;
                let r = active.world.cast(first, r, shape, bd_sim::Aim::new(eye, (spot - eye).normalize_or(fwd)));
                info!("first_call scenario: {first:?} calls Optimus Prime: {r:?}");
                sc.step = 2;
                sc.t = 0.0;
            }
            2 if sc.t > 1.2 => {
                let Some((pos, yaw)) = sc.base else { return };
                let fwd = bd_core::input::look_dir(yaw, 0.0);
                let side = bd_core::Vec3::new(-fwd.z, 0.0, fwd.x);
                match v.as_str() {
                    "echo" => {
                        // burn it: into the void, back at the spawn, then call it again
                        if let Some(p) = active.world.player_mut(me) {
                            p.mv.pos.y = -500.0;
                        }
                        sc.step = 3;
                    }
                    _ => {
                        let (r, shape) = optimus();
                        let second = if v == "rival" { me } else { o };
                        let spot = pos + fwd * 11.0 + side * 3.2;
                        let eye = active.world.player(second).map_or(pos, |p| p.mv.pos) + bd_core::Vec3::Y * 1.6;
                        let r = active.world.cast(second, r, shape, bd_sim::Aim::new(eye, (spot - eye).normalize_or(fwd)));
                        info!("first_call scenario: {second:?} calls him too: {r:?}");
                        sc.step = 9;
                    }
                }
                sc.t = 0.0;
            }
            3 => {
                // respawned: face the old spot and call him again
                if !active.local_player().is_some_and(|p| p.alive) || sc.t < 0.5 {
                    return;
                }
                let Some(pos) = active.local_player().map(|p| p.mv.pos) else { return };
                if let Some(p) = active.world.player_mut(me) {
                    p.willpower = 100.0;
                    p.spawn_shield = 0.0;
                }
                let fwd = bd_core::input::look_dir(look.yaw, 0.0);
                let eye = pos + bd_core::Vec3::Y * 1.6;
                let spot = pos + fwd * 9.0;
                let (r, shape) = optimus();
                let r = active.world.cast(me, r, shape, bd_sim::Aim::new(eye, (spot - eye).normalize_or(fwd)));
                info!("first_call scenario: the echo: {r:?}");
                sc.step = 9;
                sc.t = 0.0;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_distinct_and_never_empty() {
        let mut seen = std::collections::HashSet::new();
        for k in 0..2000u64 {
            let key = bd_sim::first_call::keys::name_key(&format!("thing {k}"));
            let b = glyph_bits(key);
            assert!(b.count_ones() >= 3 && b < 512);
            seen.insert(b);
        }
        assert!(seen.len() > 300, "{} distinct glyphs", seen.len());
    }

    #[test]
    fn a_character_phrase_slot_is_that_character() {
        let w = bd_sim::World::new(bd_sim::SimConfig { arena: bd_sim::ArenaSource::Flat { size_m: 64.0 }, ..Default::default() });
        let ids = Identities::shipped();
        let optimus = ids.of_name("optimus prime").map(|i| i.key);
        assert!(optimus.is_some());
        assert_eq!(slot_key(&w, "Optimus Prime", &SlotBind::Phrase { signature: false }), optimus);
        assert_eq!(slot_key(&w, "a flaming sword", &SlotBind::Phrase { signature: false }), None);
    }
}
