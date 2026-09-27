//! POWER-UPS presentation (`bd_sim::pickups`, `MatchRules::powerups`). Reads only the replicated
//! pickup list (`World::pickups.items`, the snapshot's copy on clients) and `GameEvent::Pickup`, so
//! practice, host and client look the same. **No text, no HUD**: every kind reads by look and sound.
//!
//! * **Beacon** — while a pickup is telegraphed, a tall pillar of light in its colour stands where it
//!   will appear (seen across the map), brightening until it lands; a faint one stays over a lying
//!   pickup and over a carrier (the carrier is the target).
//! * **Giant** — a huge crimson egg-heart that thumps lub-dub (scale pulse) with a sub-bass heartbeat.
//! * **Awakening** — a golden orb wrapped in three swirling rings, rising and turning (a rising hum).
//! * **Ultimate** — a violet star of crossed spikes that spins and crackles.
//! * **Held** — the carrier's hand carries a small copy that glows in the pickup's colour (for the
//!   local first-person player: low in the right of the view — "you're loaded").
//! * `--scenario pickups` — one of each lying in view, a beacon, a carrier and a loaded hand
//!   (screenshots; see [`scenario`]).

use super::PresentSet;
use super::sound::{Sfx3d, SfxLoopEmitter};
use crate::camera::PlayerCamera;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use crate::tick::FrameSet;
use bd_sim::GameEvent;
use bd_sim::pickups::{GateKind, PickupMoment, PickupPhase};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub struct PickupsPlugin;

impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PickupVisuals>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, pickup_events.in_set(PresentSet::Events))
            .add_systems(Update, sync.in_set(FrameSet::Present).after(PresentSet::Sync));
        scenario::build(app);
    }
}

/// Linear colour of a kind (bloom-bright).
pub fn kind_rgb(k: GateKind) -> LinearRgba {
    match k {
        GateKind::Giant => LinearRgba::rgb(1.6, 0.06, 0.12),
        GateKind::Awakening => LinearRgba::rgb(1.5, 1.05, 0.25),
        GateKind::Ultimate => LinearRgba::rgb(0.75, 0.3, 1.6),
    }
}

#[derive(Component)]
struct PickupRoot;

/// One visual per live pickup id.
struct Vis {
    root: Entity,
    /// The kind's body (scaled / spun each frame).
    body: Entity,
    rings: [Option<Entity>; 3],
    beacon: Entity,
    beacon_mat: Handle<StandardMaterial>,
    kind: GateKind,
    seen: bool,
    t: f32,
    next_beat: f32,
}

#[derive(Resource, Default)]
struct PickupVisuals {
    by_id: HashMap<u16, Vis>,
    meshes: Option<Meshes>,
}

#[derive(Clone)]
struct Meshes {
    sphere: Handle<Mesh>,
    ring: Handle<Mesh>,
    spike: Handle<Mesh>,
    pillar: Handle<Mesh>,
    mats: [Handle<StandardMaterial>; 3],
    shells: [Handle<StandardMaterial>; 3],
}

fn glow(color: LinearRgba, k: f32, alpha: Option<f32>) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba { alpha: alpha.unwrap_or(1.0), ..color }),
        emissive: color * k,
        unlit: true,
        alpha_mode: if alpha.is_some() { AlphaMode::Add } else { AlphaMode::Opaque },
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

fn meshes(vis: &mut PickupVisuals, mesh: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Meshes {
    if let Some(m) = &vis.meshes {
        return m.clone();
    }
    let m = Meshes {
        sphere: mesh.add(Sphere::new(0.5).mesh().ico(3).unwrap_or_else(|_| Sphere::new(0.5).mesh().uv(24, 16))),
        ring: mesh.add(Torus::new(0.62, 0.7)),
        spike: mesh.add(Cuboid::new(0.12, 0.12, 1.5)),
        pillar: mesh.add(Cylinder::new(0.35, 1.0)),
        mats: GateKind::ALL.map(|k| mats.add(glow(kind_rgb(k), 2.5, None))),
        shells: GateKind::ALL.map(|k| mats.add(glow(kind_rgb(k), 1.0, Some(0.28)))),
    };
    vis.meshes = Some(m.clone());
    m
}

fn spawn_vis(commands: &mut Commands, m: &Meshes, mats: &mut Assets<StandardMaterial>, kind: GateKind, pos: Vec3) -> Vis {
    let k = kind as usize;
    let root = commands.spawn((Name::new("Pickup"), PickupRoot, Transform::from_translation(pos), Visibility::default())).id();
    let body = commands.spawn((Transform::from_xyz(0.0, 1.3, 0.0), Visibility::default())).id();
    commands.entity(root).add_child(body);
    let mut rings = [None; 3];
    match kind {
        GateKind::Giant => {
            // An egg-heart: bright core inside a translucent shell.
            let core = commands.spawn((Mesh3d(m.sphere.clone()), MeshMaterial3d(m.mats[k].clone()), Transform::from_scale(Vec3::new(1.05, 1.4, 1.05)), NotShadowCaster, NotShadowReceiver)).id();
            let shell = commands.spawn((Mesh3d(m.sphere.clone()), MeshMaterial3d(m.shells[k].clone()), Transform::from_scale(Vec3::new(1.6, 2.1, 1.6)), NotShadowCaster, NotShadowReceiver)).id();
            commands.entity(body).add_children(&[core, shell]);
        }
        GateKind::Awakening => {
            let core = commands.spawn((Mesh3d(m.sphere.clone()), MeshMaterial3d(m.mats[k].clone()), Transform::from_scale(Vec3::splat(0.8)), NotShadowCaster, NotShadowReceiver)).id();
            let halo = commands.spawn((Mesh3d(m.sphere.clone()), MeshMaterial3d(m.shells[k].clone()), Transform::from_scale(Vec3::splat(1.5)), NotShadowCaster, NotShadowReceiver)).id();
            commands.entity(body).add_children(&[core, halo]);
            for (i, r) in rings.iter_mut().enumerate() {
                let e = commands
                    .spawn((Mesh3d(m.ring.clone()), MeshMaterial3d(m.mats[k].clone()), Transform::from_rotation(Quat::from_rotation_x(i as f32 * 1.05)), NotShadowCaster, NotShadowReceiver))
                    .id();
                commands.entity(body).add_child(e);
                *r = Some(e);
            }
        }
        GateKind::Ultimate => {
            let core = commands.spawn((Mesh3d(m.sphere.clone()), MeshMaterial3d(m.mats[k].clone()), Transform::from_scale(Vec3::splat(0.5)), NotShadowCaster, NotShadowReceiver)).id();
            commands.entity(body).add_child(core);
            // Six spikes along the three axes and the four diagonals of one plane: a star.
            let dirs = [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1.0, 1.0, 0.0), Vec3::new(1.0, -1.0, 0.0), Vec3::new(0.0, 1.0, 1.0), Vec3::new(0.0, 1.0, -1.0)];
            for d in dirs {
                let e = commands
                    .spawn((Mesh3d(m.spike.clone()), MeshMaterial3d(m.mats[k].clone()), Transform::from_rotation(Quat::from_rotation_arc(Vec3::Z, d.normalize())), NotShadowCaster, NotShadowReceiver))
                    .id();
                commands.entity(body).add_child(e);
            }
            let halo = commands.spawn((Mesh3d(m.sphere.clone()), MeshMaterial3d(m.shells[k].clone()), Transform::from_scale(Vec3::splat(1.3)), NotShadowCaster, NotShadowReceiver)).id();
            commands.entity(body).add_child(halo);
        }
    }
    let beacon_mat = mats.add(glow(kind_rgb(kind), 1.0, Some(0.0)));
    let beacon = commands
        .spawn((Mesh3d(m.pillar.clone()), MeshMaterial3d(beacon_mat.clone()), Transform::from_xyz(0.0, 40.0, 0.0).with_scale(Vec3::new(1.0, 80.0, 1.0)), NotShadowCaster, NotShadowReceiver))
        .id();
    commands.entity(root).add_child(beacon);
    if kind == GateKind::Awakening {
        commands.entity(root).insert(SfxLoopEmitter { name: "charge_loop", gain: 0.0, pitch: 1.25 });
    } else if kind == GateKind::Ultimate {
        commands.entity(root).insert(SfxLoopEmitter { name: "beam_loop", gain: 0.0, pitch: 0.7 });
    }
    Vis { root, body, rings, beacon, beacon_mat, kind, seen: true, t: 0.0, next_beat: 0.0 }
}

fn clear(mut commands: Commands, mut vis: ResMut<PickupVisuals>) {
    for (_, v) in vis.by_id.drain() {
        commands.entity(v.root).try_despawn();
    }
}

fn pickup_events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut sfx: MessageWriter<Sfx3d>) {
    if active.is_none() {
        events.clear();
        return;
    }
    for ev in events.read() {
        let GameEvent::Pickup { kind, moment, pos, .. } = &ev.event else { continue };
        let p = super::bv(*pos);
        match moment {
            // The shared timer: a ping everyone hears, then the kind's own voice when it lands.
            PickupMoment::Telegraph => {
                sfx.write(Sfx3d::at("summon_ping", p).gain(1.6).pitch(0.8));
            }
            PickupMoment::Appear => {
                let (n, pitch) = match kind {
                    GateKind::Giant => ("heartbeat", 0.5),
                    GateKind::Awakening => ("charge_full", 1.1),
                    GateKind::Ultimate => ("ult_charge", 0.9),
                };
                sfx.write(Sfx3d::at(n, p).gain(1.4).pitch(pitch));
            }
            PickupMoment::Take => {
                sfx.write(Sfx3d::at("legendary_sting", p).gain(1.0));
            }
            PickupMoment::Drop => {
                sfx.write(Sfx3d::at("sputter", p).gain(1.0).pitch(0.7));
            }
            PickupMoment::Spend => {
                sfx.write(Sfx3d::at("presence_gather_big", p).gain(1.2));
            }
            PickupMoment::Fade => {
                sfx.write(Sfx3d::at("mix_whoosh_out", p).gain(0.8));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync(
    mut commands: Commands,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mode: Res<super::camera3p::ViewMode>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    mut vis: ResMut<PickupVisuals>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut tf: Query<&mut Transform>,
    mut loops: Query<&mut SfxLoopEmitter>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else { return };
    let items = &active.world.pickups.items;
    if items.is_empty() && vis.by_id.is_empty() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let cam = cams.iter().next().copied();
    let third = mode.third_person();
    let m = meshes(&mut vis, &mut mesh_assets, &mut mats);
    for v in vis.by_id.values_mut() {
        v.seen = false;
    }
    for p in items {
        let v = vis.by_id.entry(p.id).or_insert_with(|| spawn_vis(&mut commands, &m, &mut mats, p.kind, super::bv(p.pos)));
        v.seen = true;
        v.t += dt;
        let t = v.t;
        // Where it is: on its spot, or in the carrier's hand.
        let carrier = if p.phase == PickupPhase::Held { p.holder.and_then(|h| views.player(h)) } else { None };
        let (root_pos, body_local, body_scale, beacon_alpha, beacon_h): (Vec3, Vec3, f32, f32, f32) = match (p.phase, carrier) {
            (PickupPhase::Telegraph, _) => {
                let left = (p.until.saturating_sub(active.world.tick)) as f32 * bd_core::DT;
                let k = (1.0 - left / active.world.cfg.tuning.pickups.telegraph.max(0.1)).clamp(0.0, 1.0);
                (super::bv(p.pos), Vec3::Y * 1.3, 0.0, 0.18 + 0.5 * k + 0.12 * (t * 9.0).sin().abs(), 80.0)
            }
            (PickupPhase::Ground, _) => (super::bv(p.pos), Vec3::Y * (1.3 + 0.15 * (t * 1.7).sin()), 1.0, 0.12, 60.0),
            (PickupPhase::Held, Some(c)) => {
                let first_person_local = c.is_local && !third;
                if first_person_local && let Some(cam) = cam {
                    // Low in the right of the view: the loaded hand.
                    let at = cam.translation() + cam.right() * 0.34 + cam.down() * 0.3 + cam.forward() * 0.75;
                    (at, Vec3::ZERO, 0.16, 0.0, 0.0)
                } else {
                    let (s, co) = c.yaw.sin_cos();
                    // Right hand, a little forward (yaw 0 looks down -Z).
                    let right = Vec3::new(co, 0.0, -s);
                    let fwd = Vec3::new(-s, 0.0, -co);
                    let hand = c.pos + Vec3::Y * 1.05 * c.size.max(0.3) + right * 0.45 + fwd * 0.25;
                    (hand, Vec3::ZERO, 0.32, 0.22, 24.0)
                }
            }
            (PickupPhase::Held, None) => (super::bv(p.pos), Vec3::ZERO, 0.0, 0.0, 0.0),
        };
        if let Ok(mut rt) = tf.get_mut(v.root) {
            rt.translation = root_pos;
        }
        // Body: per-kind motion.
        let (scale, rot) = match v.kind {
            GateKind::Giant => {
                // Lub-dub: two quick swells per beat.
                let ph = (t * 1.1).fract();
                let beat = (-((ph - 0.08) * 22.0).powi(2)).exp() + 0.7 * (-((ph - 0.28) * 22.0).powi(2)).exp();
                (1.0 + 0.16 * beat, Quat::from_rotation_y(t * 0.3))
            }
            GateKind::Awakening => (1.0 + 0.05 * (t * 3.0).sin(), Quat::from_rotation_y(t * 1.4)),
            GateKind::Ultimate => (1.0 + 0.08 * (t * 13.0).sin() * (t * 5.0).cos(), Quat::from_euler(EulerRot::YXZ, t * 2.2, t * 0.9, 0.0)),
        };
        if let Ok(mut bt) = tf.get_mut(v.body) {
            bt.translation = body_local;
            bt.rotation = rot;
            bt.scale = Vec3::splat(scale * body_scale);
        }
        for (i, r) in v.rings.iter().enumerate() {
            if let Some(r) = r
                && let Ok(mut rt) = tf.get_mut(*r)
            {
                rt.rotation = Quat::from_rotation_x(i as f32 * 1.05 + t * (1.6 + i as f32 * 0.7)) * Quat::from_rotation_z(t * (0.9 + i as f32 * 0.4));
                rt.translation = Vec3::Y * 0.25 * ((t * 2.0 + i as f32 * 2.1).sin());
            }
        }
        if let Ok(mut bt) = tf.get_mut(v.beacon) {
            bt.scale = Vec3::new(1.0, beacon_h.max(0.01), 1.0);
            bt.translation = Vec3::Y * (beacon_h * 0.5 + 1.0);
        }
        if let Some(mut mat) = mats.get_mut(&v.beacon_mat) {
            let c = kind_rgb(v.kind);
            mat.base_color = Color::LinearRgba(LinearRgba { alpha: beacon_alpha, ..c });
        }
        // Sound: the heart thumps (sub-bass), the others hum while they lie there.
        let lying = p.phase == PickupPhase::Ground;
        if v.kind == GateKind::Giant && lying {
            v.next_beat -= dt;
            if v.next_beat <= 0.0 {
                v.next_beat += 1.0 / 1.1;
                sfx.write(Sfx3d::at("heartbeat", super::bv(p.pos) + Vec3::Y).gain(1.3).pitch(0.5));
            }
        }
        if let Ok(mut l) = loops.get_mut(v.root) {
            l.gain = if lying { 0.55 } else { 0.0 };
        }
    }
    let gone: Vec<u16> = vis.by_id.iter().filter(|(_, v)| !v.seen).map(|(id, _)| *id).collect();
    for id in gone {
        if let Some(v) = vis.by_id.remove(&id) {
            commands.entity(v.root).try_despawn();
        }
    }
}

/// `--scenario pickups`: practice with the mode on; one of each kind lying in view, a beacon behind
/// them, a carrier bot and a loaded local hand. Screenshots only.
pub mod scenario {
    use super::*;
    use crate::input::LookAngles;
    use bd_core::PlayerId;
    use bd_sim::pickups::Pickup;

    #[derive(Resource, Default)]
    struct PickupScenario {
        placed: bool,
        t: f32,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "pickups") {
            return;
        }
        app.init_resource::<PickupScenario>().add_systems(Update, place.run_if(resource_exists::<ActiveWorld>));
    }

    fn ground(w: &bd_sim::World, p: bd_core::Vec3, near_y: f32) -> Option<bd_core::Vec3> {
        let hit = bd_core::voxel::raycast(&w.voxels, bd_core::Vec3::new(p.x, near_y + 6.0, p.z), bd_core::Vec3::NEG_Y, 14.0)?;
        ((hit.point.y - near_y).abs() < 4.0).then_some(hit.point)
    }

    fn place(mut sc: ResMut<PickupScenario>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
        sc.t += time.delta_secs();
        if sc.placed || sc.t < 0.5 {
            return;
        }
        let Some(me) = active.local_player().filter(|p| p.alive).map(|p| p.mv.pos) else { return };
        let local = active.local;
        let spots = [(4.2, -1.9), (5.2, 0.0), (4.2, 1.9), (13.0, 5.5), (6.5, 3.8)];
        let mut chosen = None;
        for step in 0..24 {
            let yaw = step as f32 * std::f32::consts::TAU / 24.0;
            let dir = bd_core::input::look_dir(yaw, 0.0);
            let side = bd_core::Vec3::new(-dir.z, 0.0, dir.x);
            let found: Vec<_> = spots.iter().filter_map(|(d, lat)| ground(&active.world, me + dir * *d + side * *lat, me.y)).collect();
            if found.len() == spots.len() {
                chosen = Some((yaw, found));
                break;
            }
        }
        let Some((yaw, g)) = chosen else {
            if sc.t > 6.0 {
                warn!("pickups scenario: no open ground around the spawn");
                sc.placed = true;
            }
            return;
        };
        let w = &mut active.world;
        w.cfg.rules.powerups = true;
        w.cfg.tuning.pickups.first_after = 9999.0;
        w.cfg.tuning.pickups.ground_life = 9999.0;
        w.cfg.tuning.pickups.held_life = 9999.0;
        w.cfg.tuning.pickups.telegraph = 9999.0;
        let until = w.tick + 1_000_000;
        let add = |id: u16, kind: GateKind, phase: PickupPhase, pos: bd_core::Vec3, holder: Option<PlayerId>| Pickup { id, kind, phase, pos, holder, until };
        let mut list = vec![
            add(1, GateKind::Giant, PickupPhase::Ground, g[0], None),
            add(2, GateKind::Awakening, PickupPhase::Ground, g[1], None),
            add(3, GateKind::Ultimate, PickupPhase::Ground, g[2], None),
            add(4, GateKind::Awakening, PickupPhase::Telegraph, g[3], None),
        ];
        // A carrier (the first other player) standing in view, and the local player's loaded hand.
        let other = w.players.iter().map(|p| p.id).find(|id| Some(*id) != local);
        if let Some(o) = other {
            if let Some(p) = w.player_mut(o) {
                p.mv.pos = g[4];
                p.mv.vel = bd_core::Vec3::ZERO;
            }
            list.push(add(5, GateKind::Giant, PickupPhase::Held, g[4], Some(o)));
        }
        if let Some(me) = local {
            list.push(add(6, GateKind::Ultimate, PickupPhase::Held, g[1], Some(me)));
        }
        w.pickups.items = list;
        active.bots.clear();
        look.yaw = yaw;
        look.pitch = -0.12;
        sc.placed = true;
        info!("pickups scenario: placed, yaw {:.0}°", yaw.to_degrees());
    }
}
