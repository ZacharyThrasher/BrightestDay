//! Space constructs + player powers (COMBAT.md §3/§5h).
//!
//! * Portals, force fields and links are synced from the replicated snapshot (`ViewKeys::curr`),
//!   so they are right in every mode and after a resync: bd_fx `PortalFx` (both ends), `ForceFieldFx`
//!   (strength 0 = Ope Ope **ROOM**: a translucent corps dome), `ChainFx` between the link ends.
//! * Power moments (`GameEvent::Power`): fruit sparkle, the stretching rubber arm of an incanted
//!   delivery, blink afterimages (a fading copy of the body), logia scatter, swaps, stand rush
//!   punches, overdose, transformations, and the **time-stop grey** (world desaturates for
//!   everyone while any player holds `TIME_STOP`; the user glows — they are the one who's fast).

pub mod ascend;
pub mod costume;
mod dev;
pub mod juice;

use super::players::PlayerBody;
use super::sound::Sfx3d;
use super::{bv, corps_rgb, PresentSet};
use crate::camera::PlayerCamera;
use crate::sim_bridge::{ActiveWorld, GameEvents, ViewKeys, WorldViews};
use crate::state::AppState;
use bd_core::{ConstructId, PlayerId};
use bd_fx::{BurstFx, BurstKind, ChainFx, ConstructEnd, ConstructVisual, ForceFieldFx, PortalFx, SpawnOnomatopoeia, ZapFx};
use bd_sim::powers::{power_flags, PowerEvent};
use bd_sim::replicate::LinkEndSnap;
use bd_sim::GameEvent;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;

#[derive(Resource, Default)]
struct SpaceEntities {
    portals: HashMap<u16, [Entity; 2]>,
    fields: HashMap<u16, (Entity, Option<Entity>)>,
    links: HashMap<u16, Entity>,
}

#[derive(Component)]
struct RubberArm {
    player: PlayerId,
    construct: ConstructId,
    t: f32,
}

#[derive(Resource)]
struct PowerAssets {
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
}

/// Current grey amount (0..1) for the time-stop post effect.
#[derive(Resource, Default)]
pub struct TimeStopGrey(pub f32);

pub struct PowersPlugin;

impl Plugin for PowersPlugin {
    fn build(&self, app: &mut App) {
        dev::build(app);
        app.add_plugins((ascend::AscendPlugin, juice::PowerJuicePlugin, costume::CostumeTintPlugin))
            .init_resource::<SpaceEntities>()
            .init_resource::<TimeStopGrey>()
            .add_systems(Startup, setup)
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (power_events, sync_space, rubber_arms, time_stop_grey).chain().in_set(PresentSet::Sync));
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(PowerAssets { cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)), sphere: meshes.add(Sphere::new(1.0).mesh().ico(4).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(32, 16))) });
}

fn clear(mut commands: Commands, mut s: ResMut<SpaceEntities>, arms: Query<Entity, With<RubberArm>>) {
    for (_, es) in s.portals.drain() {
        for e in es {
            commands.entity(e).try_despawn();
        }
    }
    for (_, (e, d)) in s.fields.drain() {
        commands.entity(e).try_despawn();
        if let Some(d) = d {
            commands.entity(d).try_despawn();
        }
    }
    for (_, e) in s.links.drain() {
        commands.entity(e).try_despawn();
    }
    for e in &arms {
        commands.entity(e).try_despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn power_events(
    mut commands: Commands,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    assets: Res<PowerAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bodies: Query<(&PlayerBody, &ConstructVisual, &GlobalTransform)>,
    mut burst: MessageWriter<BurstFx>,
    mut zap: MessageWriter<ZapFx>,
    mut sfx: MessageWriter<Sfx3d>,
    mut ono: MessageWriter<SpawnOnomatopoeia>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let eye = cams.iter().next().map(|g| g.translation());
    for ev in events.read() {
        match &ev.event {
            GameEvent::PortalOpened { ends, owner, .. } => {
                let col = corps_rgb(active.corps_of(*owner));
                for e in ends {
                    sfx.write(Sfx3d::at("construct_form", bv(e.pos)).pitch(0.6));
                }
                let mut o = SpawnOnomatopoeia::key("portal", bv(ends[0].pos), 0.8, col);
                o.event_id = ev.tick;
                ono.write(o);
            }
            GameEvent::LinkCreated { link } => {
                let col = link.owner.map(|o| corps_rgb(active.corps_of(o))).unwrap_or([1.0; 3]);
                let p = match link.a {
                    bd_sim::space::LinkEnd::Point(p) => bv(p),
                    bd_sim::space::LinkEnd::Player(id) => views.player(id).map_or(Vec3::ZERO, |v| v.pos + Vec3::Y),
                    bd_sim::space::LinkEnd::Construct(c) => views.construct(c).map_or(Vec3::ZERO, |v| v.pos),
                };
                sfx.write(Sfx3d::at("tether_attach", p).pitch(0.7));
                let mut o = SpawnOnomatopoeia::key("link", p, 0.7, col);
                o.event_id = ev.tick ^ link.id as u64;
                ono.write(o);
            }
            GameEvent::LinkBroken { id, .. } => {
                let _ = id;
                sfx.write(Sfx3d::ui("construct_shatter").gain(0.3));
            }
            GameEvent::FieldCreated { field } => {
                let col = corps_rgb(active.corps_of(field.owner));
                let key = if field.strength > 0.0 { "attract" } else if field.strength < 0.0 { "repel" } else { "portal" };
                // A field around your own body (Almighty Push): no word in your face.
                if eye.is_none_or(|e| e.distance(bv(field.pos)) > 3.0) {
                    let mut o = SpawnOnomatopoeia::key(key, bv(field.pos), 0.8, col);
                    o.event_id = ev.tick ^ field.id as u64;
                    ono.write(o);
                }
            }
            GameEvent::Power(pe) => match pe {
                PowerEvent::FruitSpawned { pos, .. } => {
                    burst.write(BurstFx { kind: BurstKind::Flash, pos: bv(*pos), dir: Vec3::Y, scale: 0.6, color: Some([1.0, 0.85, 0.4]) });
                }
                PowerEvent::Gained { player, .. } => {
                    if let Some(v) = views.player(*player) {
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: v.pos + Vec3::Y, dir: Vec3::Y, scale: 1.0, color: Some(corps_rgb(v.corps)) });
                        sfx.write(Sfx3d::at("oath_complete", v.pos).gain(0.7));
                    }
                }
                PowerEvent::Overdose { pos, .. } => {
                    burst.write(BurstFx { kind: BurstKind::Flash, pos: bv(*pos), dir: Vec3::Y, scale: 2.5, color: None });
                    sfx.write(Sfx3d::at("explosion_big", bv(*pos)));
                }
                PowerEvent::Scattered { from, to, element, .. } => {
                    burst.write(BurstFx { kind: BurstKind::Element(*element), pos: bv(*from), dir: Vec3::Y, scale: 1.2, color: None });
                    burst.write(BurstFx { kind: BurstKind::Element(*element), pos: bv(*to), dir: Vec3::Y, scale: 1.0, color: None });
                }
                PowerEvent::Shrugged { point, .. } => {
                    burst.write(BurstFx { kind: BurstKind::Flash, pos: bv(*point), dir: Vec3::Y, scale: 0.4, color: Some([1.0; 3]) });
                }
                PowerEvent::Delivered { player, construct, arm, .. } => {
                    if *arm {
                        let col = corps_rgb(active.corps_of(*player));
                        let mat = materials.add(StandardMaterial {
                            base_color: Color::linear_rgb(col[0], col[1], col[2]),
                            emissive: LinearRgba::rgb(col[0] * 2.5, col[1] * 2.5, col[2] * 2.5),
                            ..default()
                        });
                        commands.spawn((RubberArm { player: *player, construct: *construct, t: 0.0 }, Mesh3d(assets.cube.clone()), MeshMaterial3d(mat), Transform::default()));
                        if let Some(v) = views.player(*player) {
                            sfx.write(Sfx3d::at("tether_fire", v.pos).pitch(0.6));
                        }
                    }
                }
                PowerEvent::Blinked { player, from, to, afterimage } => {
                    let col = corps_rgb(active.corps_of(*player));
                    zap.write(ZapFx { from: bv(*from) + Vec3::Y, to: bv(*to) + Vec3::Y, color: col, duration: 0.15 });
                    if *afterimage && let Some((_, vis, gt)) = bodies.iter().find(|(b, _, _)| b.id == *player) {
                        let mut ghost = ConstructVisual::new(vis.model.clone(), col);
                        ghost.opacity = 0.35;
                        ghost.layers = vis.layers;
                        let mut t = gt.compute_transform();
                        t.translation = bv(*from);
                        commands.spawn((ghost, t, ConstructEnd::Dismiss { duration: 0.6 }));
                    }
                    sfx.write(Sfx3d::at("boost_burst", bv(*to)).pitch(1.4).gain(0.6));
                }
                PowerEvent::Swapped { a_pos, b_pos, a, .. } => {
                    zap.write(ZapFx { from: bv(*a_pos) + Vec3::Y, to: bv(*b_pos) + Vec3::Y, color: corps_rgb(active.corps_of(*a)), duration: 0.2 });
                }
                PowerEvent::TimeStop { player, .. } => {
                    if let Some(v) = views.player(*player) {
                        burst.write(BurstFx { kind: BurstKind::Flash, pos: v.pos + Vec3::Y, dir: Vec3::Y, scale: 3.0, color: Some(corps_rgb(v.corps)) });
                    }
                    sfx.write(Sfx3d::ui("gun_railgun_charge").pitch(0.5).exempt());
                }
                PowerEvent::Transformed { player, .. } => {
                    if let Some(v) = views.player(*player) {
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: v.pos + Vec3::Y, dir: Vec3::Y, scale: 1.6, color: Some(corps_rgb(v.corps)) });
                        sfx.write(Sfx3d::at("transform_to_robot", v.pos).pitch(0.8));
                    }
                }
                PowerEvent::StandRush { point, player, .. } => {
                    let p = bv(*point);
                    burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::ZERO, scale: 0.5, color: Some(corps_rgb(active.corps_of(*player))) });
                    sfx.write(Sfx3d::at("hit_light", p).pitch(1.1));
                }
                PowerEvent::Restored { center, .. } => {
                    burst.write(BurstFx { kind: BurstKind::Shards, pos: bv(*center), dir: Vec3::Y, scale: 1.5, color: Some([0.9, 0.7, 1.0]) });
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn link_end(e: &LinkEndSnap, views: &WorldViews) -> Option<Vec3> {
    match *e {
        LinkEndSnap::Player(p) => views.player(PlayerId(p)).map(|v| v.pos + Vec3::Y * v.size),
        LinkEndSnap::Construct(c) => views.construct(ConstructId(c)).map(|v| v.pos),
        LinkEndSnap::Point(q) => Some(bv(q.get())),
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_space(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    keys: Res<ViewKeys>,
    views: Res<WorldViews>,
    assets: Res<PowerAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut s: ResMut<SpaceEntities>,
    mut portals: Query<(&mut Transform, &mut PortalFx)>,
    mut chains: Query<&mut ChainFx>,
    mut fields: Query<&mut Transform, (With<ForceFieldFx>, Without<PortalFx>)>,
) {
    let Some(active) = active else { return };
    if !keys.valid {
        return;
    }
    let snap = &keys.curr;
    // Portals.
    for p in &snap.portals {
        let col = corps_rgb(active.corps_of(PlayerId(p.owner)));
        let tf = |i: usize| {
            let e = &p.ends[i];
            let n = e.normal.get();
            let up = e.up.get();
            Transform::from_translation(bv(e.pos.get())).looking_to(bv(n), bv(up))
        };
        match s.portals.get(&p.id) {
            Some(es) => {
                for (i, e) in es.iter().enumerate() {
                    if let Ok((mut t, _)) = portals.get_mut(*e) {
                        *t = tf(i);
                    }
                }
            }
            None => {
                let es = [0, 1].map(|i| {
                    let e = &p.ends[i];
                    let r = e.radius as f32 / 20.0;
                    let door = e.style == bd_sim::space::PORTAL_STYLE_DOOR;
                    // Ring: outer diameter a little over the opening (the hole matches the sim's
                    // radius). Doorway: the opening's height.
                    let mut fx = PortalFx::new(if door { r * 2.0 } else { r * 2.0 * 1.12 }, col);
                    fx.open = 0.0;
                    if door {
                        fx.style = PortalFx::DOOR;
                        fx.door_aspect = bd_sim::space::DOOR_ASPECT;
                    }
                    // PORTALS: the window disc for the gameplay camera, the swirl for portal cameras.
                    fx.view_layers = Some([super::portal_view::PORTAL_MAIN_LAYER, super::portal_view::PORTAL_VIEW_LAYER]);
                    commands.spawn((fx, tf(i), super::portal_view::PortalEndRef { id: p.id, end: i as u8 })).id()
                });
                s.portals.insert(p.id, es);
            }
        }
    }
    for es in s.portals.values() {
        for e in es {
            if let Ok((_, mut fx)) = portals.get_mut(*e) {
                fx.open = (fx.open + 0.08).min(1.0);
            }
        }
    }
    let live: Vec<u16> = snap.portals.iter().map(|p| p.id).collect();
    s.portals.retain(|id, es| {
        let keep = live.contains(id);
        if !keep {
            for e in es.iter() {
                commands.entity(*e).try_despawn();
            }
        }
        keep
    });
    // Fields (+ ROOM dome).
    for f in &snap.fields {
        let col = corps_rgb(active.corps_of(PlayerId(f.owner)));
        let pos = bv(f.pos.get());
        let radius = f.radius as f32 / 4.0;
        match s.fields.get(&f.id) {
            Some((e, dome)) => {
                if let Ok(mut t) = fields.get_mut(*e) {
                    t.translation = pos;
                }
                if let Some(d) = dome {
                    commands.entity(*d).try_insert(Transform::from_translation(pos).with_scale(Vec3::splat(radius)));
                }
            }
            None => {
                let e = commands.spawn((ForceFieldFx { radius, attract: f.strength > 0, color: col, intensity: if f.strength == 0 { 0.4 } else { 1.0 } }, Transform::from_translation(pos))).id();
                let dome = (f.strength == 0).then(|| {
                    let mat = materials.add(StandardMaterial {
                        base_color: Color::linear_rgba(col[0], col[1], col[2], 0.10),
                        emissive: LinearRgba::rgb(col[0] * 0.6, col[1] * 0.6, col[2] * 0.6),
                        alpha_mode: AlphaMode::Blend,
                        cull_mode: None,
                        unlit: true,
                        ..default()
                    });
                    commands.spawn((Mesh3d(assets.sphere.clone()), MeshMaterial3d(mat), Transform::from_translation(pos).with_scale(Vec3::splat(radius)), bevy::light::NotShadowCaster)).id()
                });
                s.fields.insert(f.id, (e, dome));
            }
        }
    }
    let live: Vec<u16> = snap.fields.iter().map(|f| f.id).collect();
    s.fields.retain(|id, (e, d)| {
        let keep = live.contains(id);
        if !keep {
            commands.entity(*e).try_despawn();
            if let Some(d) = d {
                commands.entity(*d).try_despawn();
            }
        }
        keep
    });
    // Links.
    for l in &snap.links {
        let (Some(a), Some(b)) = (link_end(&l.a, &views), link_end(&l.b, &views)) else { continue };
        match s.links.get(&l.id) {
            Some(e) => {
                if let Ok(mut c) = chains.get_mut(*e) {
                    c.a = a;
                    c.b = b;
                }
            }
            None => {
                let owner = match l.a {
                    LinkEndSnap::Player(p) => Some(PlayerId(p)),
                    _ => None,
                };
                let col = owner.map(|o| corps_rgb(active.corps_of(o))).unwrap_or([0.9, 0.9, 1.0]);
                let e = commands.spawn(ChainFx::new(a, b, col)).id();
                s.links.insert(l.id, e);
            }
        }
    }
    let live: Vec<u16> = snap.links.iter().map(|l| l.id).collect();
    s.links.retain(|id, e| {
        let keep = live.contains(id);
        if !keep {
            commands.entity(*e).try_despawn();
        }
        keep
    });
}

fn rubber_arms(
    mut commands: Commands,
    time: Res<Time<Real>>,
    views: Res<WorldViews>,
    mut arms: Query<(Entity, &mut RubberArm, &mut Transform)>,
) {
    for (e, mut arm, mut tf) in &mut arms {
        arm.t += time.delta_secs();
        let (Some(p), Some(c)) = (views.player(arm.player), views.construct(arm.construct)) else {
            commands.entity(e).despawn();
            continue;
        };
        if arm.t > 0.9 {
            commands.entity(e).despawn();
            continue;
        }
        let hand = super::constructs::hand_transform(p).translation;
        let d = c.pos - hand;
        let len = d.length().max(0.05);
        let thick = 0.16 * p.size;
        *tf = Transform::from_translation(hand + d * 0.5).looking_to(d / len, Vec3::Y).with_scale(Vec3::new(thick, thick, len));
    }
}

fn time_stop_grey(
    time: Res<Time<Real>>,
    views: Res<WorldViews>,
    mut grey: ResMut<TimeStopGrey>,
    mut cams: Query<&mut ColorGrading, With<PlayerCamera>>,
) {
    let stopped = views.players.iter().any(|p| p.power.has(power_flags::TIME_STOP));
    let target = if stopped { 1.0 } else { 0.0 };
    let dt = time.delta_secs();
    let rate = if stopped { 10.0 } else { 3.0 };
    let before = grey.0;
    grey.0 += (target - grey.0) * (1.0 - (-rate * dt).exp());
    if (grey.0 - before).abs() < 1e-4 && grey.0 < 1e-3 {
        return;
    }
    for mut cg in &mut cams {
        cg.global.post_saturation = 1.04 * (1.0 - 0.9 * grey.0);
        cg.global.exposure = -0.15 * grey.0;
    }
}
