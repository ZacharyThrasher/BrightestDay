//! CYBERTRON: the transforming panels (sim: `bd_sim::world::map_mech`, map data:
//! `ArenaInfo::mechanics.movers`). Any map with movers gets this; only Cybertron has them.
//!
//! * **Panels** — each part is a gunmetal plate with a glowing seam strip, posed every frame from the
//!   replicated mover state (`World::map_mech`) and eased so the fold reads as one smooth mechanical
//!   motion (the sim only makes a panel solid once it has fully locked in place).
//! * **Telegraph** — ~2 s before any move (`WarnExtend` / `WarnRetract`) the seams pulse amber and
//!   a hard-light ghost of the extended pose shows where a new bridge / ramp will appear.
//! * **Sound** — our own mechanical cues: panel clicks through the warning, a faster click run
//!   while it moves (`transform_panel_click`), a lock clunk (`assemble_lock`) when it seats.

use super::bv;
use super::sound::Sfx3d;
use crate::sim_bridge::{ActiveWorld, GameEvents};
use crate::state::AppState;
use bd_sim::events::GameEvent;
use bd_sim::world::map_mech::MoverPhase;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

pub struct CybertronPlugin;

impl Plugin for CybertronPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoverDraw>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (mover_sounds, sync_movers, pose_movers).chain().in_set(super::PresentSet::Sync));
    }
}

#[derive(Component)]
struct MoverPanel {
    mover: usize,
    part: usize,
}

#[derive(Component)]
struct MoverGhost {
    mover: usize,
    part: usize,
}

#[derive(Resource, Default)]
struct MoverDraw {
    /// Movers the entities were built for (rebuilt when the map changes).
    built: Option<Vec<bd_voxel::MapMover>>,
    /// Per-mover seam material (pulses on warnings) and the shared ghost material.
    seams: Vec<Handle<StandardMaterial>>,
    ghost: Option<Handle<StandardMaterial>>,
    /// Scheduled clicks: (seconds from now, position, pitch, name).
    pending: Vec<(f32, Vec3, f32, &'static str)>,
}

type MoverEntities<'w, 's> = Query<'w, 's, Entity, Or<(With<MoverPanel>, With<MoverGhost>)>>;

fn clear(mut commands: Commands, mut draw: ResMut<MoverDraw>, q: MoverEntities) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
    *draw = MoverDraw::default();
}

fn sync_movers(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    mut draw: ResMut<MoverDraw>,
    existing: MoverEntities,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Some(active) = active else { return };
    let movers = &active.world.arena.mechanics.movers;
    if draw.built.as_ref() == Some(movers) {
        return;
    }
    for e in &existing {
        commands.entity(e).try_despawn();
    }
    draw.seams.clear();
    draw.built = Some(movers.clone());
    if movers.is_empty() {
        return;
    }
    let plate = mats.add(StandardMaterial { base_color: Color::srgb(0.25, 0.28, 0.34), metallic: 0.55, perceptual_roughness: 0.45, ..default() });
    let ghost = mats.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.6, 0.2, 0.0),
        emissive: LinearRgba::rgb(1.6, 0.7, 0.15),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    draw.ghost = Some(ghost.clone());
    for (i, m) in movers.iter().enumerate() {
        let seam = mats.add(StandardMaterial { base_color: Color::srgb(0.2, 0.75, 1.0), emissive: LinearRgba::rgb(0.6, 2.4, 4.0), ..default() });
        draw.seams.push(seam.clone());
        for (k, p) in m.parts.iter().enumerate() {
            let h = bv(p.half);
            let (c, q) = p.pose(0.0);
            // The plate, a glowing runner strip down its length and two edge strips on top.
            commands
                .spawn((
                    MoverPanel { mover: i, part: k },
                    Mesh3d(meshes.add(Cuboid::new(h.x * 2.0, h.y * 2.0, h.z * 2.0))),
                    MeshMaterial3d(plate.clone()),
                    Transform { translation: bv(c), rotation: super::bq(q), ..default() },
                ))
                .with_children(|ch| {
                    let long_x = h.x >= h.z;
                    let (lx, lz) = if long_x { (h.x * 2.0 - 0.3, 0.18) } else { (0.18, h.z * 2.0 - 0.3) };
                    for off in [-0.8f32, 0.0, 0.8] {
                        let o = if long_x { Vec3::new(0.0, h.y + 0.02, off * (h.z - 0.35)) } else { Vec3::new(off * (h.x - 0.35), h.y + 0.02, 0.0) };
                        ch.spawn((Mesh3d(meshes.add(Cuboid::new(lx, 0.05, lz))), MeshMaterial3d(seam.clone()), Transform::from_translation(o), NotShadowCaster));
                    }
                });
            let (ce, qe) = p.pose(1.0);
            commands.spawn((
                MoverGhost { mover: i, part: k },
                Mesh3d(meshes.add(Cuboid::new(h.x * 2.0 + 0.1, h.y * 2.0 + 0.1, h.z * 2.0 + 0.1))),
                MeshMaterial3d(ghost.clone()),
                Transform { translation: bv(ce), rotation: super::bq(qe), ..default() },
                Visibility::Hidden,
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

/// Pose panels from the replicated state (eased per frame), pulse seams on warnings, show ghosts.
fn pose_movers(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    draw: Res<MoverDraw>,
    mut panels: Query<(&MoverPanel, &mut Transform), Without<MoverGhost>>,
    mut ghosts: Query<(&MoverGhost, &mut Visibility)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Some(active) = active else { return };
    let w = &active.world;
    let movers = &w.arena.mechanics.movers;
    if movers.is_empty() {
        return;
    }
    let t = time.elapsed_secs();
    let dt = time.delta_secs().min(0.1);
    for (mp, mut tf) in &mut panels {
        let (Some(m), Some(_)) = (movers.get(mp.mover), w.map_mech.movers.get(mp.mover)) else { continue };
        let Some(part) = m.parts.get(mp.part) else { continue };
        let open = w.map_mech.openness(mp.mover, w.tick);
        let (c, q) = part.pose(open);
        // Snapshots arrive at 30–60 Hz: ease toward the pose so the fold is one smooth motion.
        let k = 1.0 - (-22.0 * dt).exp();
        tf.translation = tf.translation.lerp(bv(c), k);
        tf.rotation = tf.rotation.slerp(super::bq(q), k);
    }
    for (i, s) in w.map_mech.movers.iter().enumerate() {
        let Some(h) = draw.seams.get(i) else { continue };
        let Some(mat) = mats.get_mut(h) else { continue };
        let pulse = 0.5 + 0.5 * (t * 9.0).sin();
        mat.emissive = match s.phase {
            MoverPhase::WarnExtend | MoverPhase::WarnRetract => LinearRgba::rgb(3.0 + 3.0 * pulse, 1.0 + pulse, 0.1),
            MoverPhase::Extending | MoverPhase::Retracting => LinearRgba::rgb(5.0, 1.8, 0.2),
            MoverPhase::Extended => LinearRgba::rgb(0.6, 2.4, 4.0),
            MoverPhase::Folded => LinearRgba::rgb(0.2, 0.8, 1.4),
        };
    }
    for (g, mut vis) in &mut ghosts {
        let show = w.map_mech.movers.get(g.mover).is_some_and(|s| matches!(s.phase, MoverPhase::WarnExtend | MoverPhase::Extending));
        *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
        let _ = g.part;
    }
    if let Some(gm) = draw.ghost.as_ref().and_then(|h| mats.get_mut(h)) {
        let pulse = 0.5 + 0.5 * (t * 6.0).sin();
        gm.emissive = LinearRgba::rgb(0.5 + 0.9 * pulse, 0.22 + 0.4 * pulse, 0.05);
    }
}

/// Panel sounds: clicks through the warning, a click run while moving, a clunk when it locks.
fn mover_sounds(
    time: Res<Time<Real>>,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    mut draw: ResMut<MoverDraw>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    for ev in events.read() {
        let GameEvent::MapMover { mover, phase, .. } = &ev.event else { continue };
        let Some(m) = active.world.arena.mechanics.movers.get(*mover as usize) else { continue };
        for p in &m.parts {
            let at = bv(p.pose(if matches!(phase, MoverPhase::WarnRetract | MoverPhase::Retracting | MoverPhase::Folded) { 1.0 } else { 0.0 }).0);
            match phase {
                MoverPhase::WarnExtend | MoverPhase::WarnRetract => {
                    for k in 0..4 {
                        draw.pending.push((k as f32 * 0.5, at, 0.72 + k as f32 * 0.04, "transform_panel_click"));
                    }
                }
                MoverPhase::Extending | MoverPhase::Retracting => {
                    for k in 0..8 {
                        draw.pending.push((k as f32 * 0.14, at, 0.9 + k as f32 * 0.04, "transform_panel_click"));
                    }
                }
                MoverPhase::Extended | MoverPhase::Folded => draw.pending.push((0.0, at, 0.8, "assemble_lock")),
            }
        }
    }
    let dt = time.delta_secs();
    let mut i = 0;
    while i < draw.pending.len() {
        draw.pending[i].0 -= dt;
        if draw.pending[i].0 <= 0.0 {
            let (_, at, pitch, name) = draw.pending.swap_remove(i);
            sfx.write(Sfx3d::at(name, at).pitch(pitch).gain(1.1));
        } else {
            i += 1;
        }
    }
}
