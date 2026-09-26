//! Surf lines (sim: `ArenaInfo::surf_lines` → `bd_sim::movement::surf_line` colliders).
//!
//! * **Lines** — each face drawn as a translucent hard-light slab exactly where its collider is (so
//!   what you see is what you ride), rebuilt whenever the active world's lines change.
//! * **Wind rush** — while the local player surfs faster than ~15 m/s: a few thin streaks rush past
//!   the camera along the velocity (the Orbit speed-line look), fading with speed. Comfort-gated
//!   (off without speed FOV / FOV kicks). Audio hook for the audio worker: `surf_loop`.

use super::{bq, bv};
use crate::camera::PlayerCamera;
use crate::player::{LocalPlayer, MoveTuning, PlayerSim};
use crate::settings::ComfortSettings;
use crate::sim_bridge::ActiveWorld;
use crate::state::AppState;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

const STREAKS: usize = 12;

pub struct SurfPlugin;

impl Plugin for SurfPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurfDraw>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (sync_lines, wind_rush).chain().in_set(crate::tick::FrameSet::Present));
    }
}

#[derive(Component)]
struct SurfSlab;

#[derive(Component)]
struct SurfStreak(usize);

#[derive(Resource, Default)]
struct SurfDraw {
    lines: Option<Vec<bd_voxel::SurfLine>>,
    streak_mat: Option<Handle<StandardMaterial>>,
    level: f32,
}

fn clear(mut commands: Commands, mut draw: ResMut<SurfDraw>, q: Query<Entity, Or<(With<SurfSlab>, With<SurfStreak>)>>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
    *draw = SurfDraw::default();
}

fn sync_lines(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    mut draw: ResMut<SurfDraw>,
    slabs: Query<Entity, With<SurfSlab>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Some(active) = active else { return };
    let lines = &active.world.arena.surf_lines;
    if draw.lines.as_ref() == Some(lines) {
        return;
    }
    for e in &slabs {
        commands.entity(e).try_despawn();
    }
    let mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.9, 1.0, 0.38),
        emissive: LinearRgba::rgb(0.18, 0.55, 0.8),
        perceptual_roughness: 0.3,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    for (i, l) in lines.iter().enumerate() {
        for c in bd_sim::movement::surf_line(l.a, l.b, l.face_deg, l.width, 1.5, i as u32 * 2) {
            let h = bv(c.half);
            commands.spawn((
                SurfSlab,
                Mesh3d(meshes.add(Cuboid::new(h.x * 2.0, h.y * 2.0, h.z * 2.0))),
                MeshMaterial3d(mat.clone()),
                Transform { translation: bv(c.center), rotation: bq(c.rot), ..default() },
                NotShadowCaster,
            ));
        }
    }
    draw.lines = Some(lines.clone());
}

#[allow(clippy::too_many_arguments)]
fn wind_rush(
    mut commands: Commands,
    time: Res<Time<Real>>,
    comfort: Res<ComfortSettings>,
    tuning: Res<MoveTuning>,
    me: Query<&PlayerSim, With<LocalPlayer>>,
    cam: Query<&Transform, (With<PlayerCamera>, Without<SurfStreak>)>,
    mut streaks: Query<(&SurfStreak, &mut Transform, &mut Visibility)>,
    mut draw: ResMut<SurfDraw>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let comfort_k = if comfort.speed_fov { comfort.fov_kick.clamp(0.0, 1.0) } else { 0.0 };
    let (surfing, vel) = me.single().map_or((false, Vec3::ZERO), |s| (bd_sim::movement::surfing(&s.state, &tuning.0), bv(s.state.vel)));
    let speed = vel.length();
    let want = if surfing { ((speed - 15.0) / 25.0).clamp(0.0, 1.0) * comfort_k } else { 0.0 };
    // Ease in quickly, out a little slower (contact flickers tick to tick on a face).
    let rate = if want > draw.level { 10.0 } else { 3.0 };
    draw.level += (want - draw.level) * (1.0 - (-rate * time.delta_secs()).exp());
    if draw.level < 0.01 && draw.streak_mat.is_none() {
        return;
    }
    let mat = draw
        .streak_mat
        .get_or_insert_with(|| {
            let m = mats.add(StandardMaterial {
                base_color: Color::srgba(0.85, 0.95, 1.0, 0.0),
                unlit: true,
                fog_enabled: false,
                alpha_mode: AlphaMode::Add,
                ..default()
            });
            let mesh = meshes.add(Cuboid::new(0.025, 0.025, 1.0));
            for k in 0..STREAKS {
                commands.spawn((
                    SurfStreak(k),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(m.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
            m
        })
        .clone();
    let k = draw.level;
    if let Some(mut m) = mats.get_mut(&mat) {
        m.base_color = Color::srgba(0.85, 0.95, 1.0, 0.22 * k);
    }
    let Ok(cam) = cam.single() else { return };
    let fwd = vel.normalize_or(cam.rotation * Vec3::NEG_Z);
    let a = if fwd.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let u = fwd.cross(a).normalize();
    let w = fwd.cross(u);
    let t = time.elapsed_secs();
    for (s, mut tf, mut vis) in &mut streaks {
        if k <= 0.02 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let i = s.0 as f32;
        let h = (i * 0.618_034).fract();
        let ang = i / STREAKS as f32 * std::f32::consts::TAU + h;
        let rad = 2.4 + 2.0 * (i * 0.371).fract();
        let phase = (h + t * (1.4 + speed * 0.03)).fract();
        tf.translation = cam.translation + fwd * (14.0 - 22.0 * phase) + (u * ang.cos() + w * ang.sin()) * rad;
        tf.rotation = Quat::from_rotation_arc(Vec3::Z, fwd);
        tf.scale = Vec3::new(1.0, 1.0, 1.2 + 3.0 * k);
    }
}
