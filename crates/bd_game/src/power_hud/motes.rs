//! Third person and OTHER players: small motes orbiting each player, one per held power, in its
//! aura colour — bright when ready, small and dim while cooling, a flare when it comes back, bigger
//! and faster (flickering) while a form burns. Opponents read your readiness: telegraphed power.
//! Reads the replicated `ReadySnap` of every player (the same on host and client). Entities are
//! pooled per player; materials are written only when their quantised look changes.

use super::derive::{identity_color, SLOTS};
use crate::camera::CameraRig;
use crate::sim_bridge::{ViewKeys, WorldViews};
use crate::state::AppState;
use bd_sim::powers::PowerTable;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

const MOTE_R: f32 = 0.08;

struct MoteRig {
    player: u8,
    root: Entity,
    motes: [Entity; SLOTS],
    mats: [Handle<StandardMaterial>; SLOTS],
    power: [Option<u16>; SLOTS],
    ready: [bool; SLOTS],
    flash: [f32; SLOTS],
    key: [[f32; 4]; SLOTS],
    seen: bool,
}

#[derive(Resource)]
struct Motes {
    rigs: Vec<MoteRig>,
    mesh: Handle<Mesh>,
    t: f32,
}

pub fn build(app: &mut App) {
    app.add_systems(Startup, setup).add_systems(OnExit(AppState::InGame), clear).add_systems(Update, draw.in_set(crate::present::PresentSet::Sync));
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mesh = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(12, 8)));
    commands.insert_resource(Motes { rigs: Vec::with_capacity(bd_core::MAX_PLAYERS), mesh, t: 0.0 });
}

fn clear(mut commands: Commands, motes: Option<ResMut<Motes>>) {
    let Some(mut m) = motes else { return };
    for r in m.rigs.drain(..) {
        commands.entity(r.root).try_despawn();
    }
}

fn vis(on: bool) -> Visibility {
    if on { Visibility::Inherited } else { Visibility::Hidden }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    views: Res<WorldViews>,
    keys: Res<ViewKeys>,
    rig: Res<CameraRig>,
    motes: Option<ResMut<Motes>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
) {
    let Some(mut m) = motes else { return };
    let m = &mut *m;
    let dt = time.delta_secs();
    m.t += dt;
    for r in &mut m.rigs {
        r.seen = false;
    }
    for v in &views.players {
        let Some(row) = keys.curr.players.binary_search_by_key(&v.id.0, |p| p.id).ok().map(|i| &keys.curr.players[i]) else { continue };
        let i = match m.rigs.iter().position(|r| r.player == v.id.0) {
            Some(i) => i,
            None => {
                let root = commands.spawn((Name::new("PowerMotes"), Transform::default(), Visibility::Hidden)).id();
                let mats_: [Handle<StandardMaterial>; SLOTS] =
                    std::array::from_fn(|_| mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(1.0, 1.0, 1.0), unlit: true, ..default() }));
                let motes_: [Entity; SLOTS] = std::array::from_fn(|k| {
                    let e = commands.spawn((Mesh3d(m.mesh.clone()), MeshMaterial3d(mats_[k].clone()), Transform::from_scale(Vec3::splat(MOTE_R)), Visibility::Hidden, NotShadowCaster)).id();
                    commands.entity(root).add_child(e);
                    e
                });
                m.rigs.push(MoteRig { player: v.id.0, root, motes: motes_, mats: mats_, power: [None; SLOTS], ready: [true; SLOTS], flash: [0.0; SLOTS], key: [[-1.0; 4]; SLOTS], seen: true });
                m.rigs.len() - 1
            }
        };
        let r = &mut m.rigs[i];
        r.seen = true;
        let held: [Option<u16>; SLOTS] = {
            let mut h = [None; SLOTS];
            for (k, (id, _, _)) in row.power.held().enumerate().take(SLOTS) {
                h[k] = Some(id);
            }
            h
        };
        let n = held.iter().flatten().count();
        let show = v.alive() && n > 0 && !(v.is_local && *rig == CameraRig::FirstPerson);
        let Ok((mut tf, mut vv)) = q.get_mut(r.root) else { continue };
        if *vv != vis(show) {
            *vv = vis(show);
        }
        if !show {
            r.power = [None; SLOTS];
            continue;
        }
        tf.translation = v.pos + Vec3::Y * (1.05 * v.size);
        let radius = 0.62 * v.size;
        for k in 0..SLOTS {
            let on = held[k].is_some();
            let is_ready = row.ready.is_ready(k);
            if held[k] != r.power[k] {
                r.power[k] = held[k];
                r.ready[k] = true;
                r.flash[k] = if on { 1.0 } else { 0.0 };
                r.key[k] = [-1.0; 4];
            } else if on && is_ready && !r.ready[k] {
                r.flash[k] = 1.0;
            }
            r.ready[k] = is_ready;
            r.flash[k] = (r.flash[k] - dt / 0.5).max(0.0);
            let form = row.ready.form(k).is_some();
            let ready = row.ready.ready(k);
            let Ok((mut mt, mut mv)) = q.get_mut(r.motes[k]) else { continue };
            if *mv != vis(on) {
                *mv = vis(on);
            }
            let Some(id) = held[k] else { continue };
            // Orbit: evenly spaced, a tilted plane, faster while a form burns.
            let speed = if form { 2.6 } else { 1.3 };
            let a = m.t * speed + k as f32 * std::f32::consts::TAU / n as f32 + v.id.0 as f32;
            let (s, c) = a.sin_cos();
            mt.translation = Vec3::new(c * radius, 0.18 * s * v.size + 0.08 * (m.t * 2.0 + k as f32).sin(), s * radius);
            let flick = if form { 1.0 + 0.2 * (m.t * 19.0 + k as f32).sin() } else { 1.0 };
            let base = if is_ready { 1.0 } else { 0.45 + 0.35 * ready };
            mt.scale = Vec3::splat(MOTE_R * v.size * base * flick * (1.0 + 0.9 * r.flash[k]) * if form { 1.35 } else { 1.0 });
            let col = PowerTable::get().def(id).map_or([1.0; 3], identity_color);
            let glow = if is_ready { 5.0 } else { 0.25 + 0.9 * ready };
            let w = r.flash[k] * 8.0;
            let key = [col[0] * glow + w, col[1] * glow + w, col[2] * glow + w, if is_ready { 1.0 } else { 0.3 }];
            if key.iter().zip(r.key[k].iter()).map(|(a, b)| (a - b).abs()).sum::<f32>() > 0.08
                && let Some(mut mm) = mats.get_mut(&r.mats[k])
            {
                r.key[k] = key;
                // Unlit: the HDR base colour blooms.
                mm.base_color = Color::linear_rgb(key[0] * 0.5, key[1] * 0.5, key[2] * 0.5);
                mm.emissive = LinearRgba::rgb(key[0], key[1], key[2]);
            }
        }
    }
    // Players that left.
    m.rigs.retain(|r| {
        if !r.seen {
            commands.entity(r.root).try_despawn();
        }
        r.seen
    });
}
