//! Surf lines (sim: `ArenaInfo::surf_lines` → `bd_sim::movement::surf_line` colliders).
//!
//! * **Lines** — each face drawn as a translucent hard-light slab exactly where its collider is (so
//!   what you see is what you ride), rebuilt whenever the active world's lines change.
//! * **Wind rush** — while the local player surfs faster than ~15 m/s: a few thin streaks rush past
//!   the camera along the velocity (the Orbit speed-line look), fading with speed. Comfort-gated
//!   (off without speed FOV / FOV kicks). Audio hook for the audio worker: `surf_loop`.
//! * `--scenario surf [--surf-line N]` — record / screenshot scene (see [`scenario`]).

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
        scenario::build(app);
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

/// `--scenario surf [--surf-line N]`: a player surfs a map line (reset every 4 s) while the camera
/// watches from the landing end — for `--record` clips of the surf lines.
pub mod scenario {
    use crate::input::LookAngles;
    use crate::sim_bridge::ActiveWorld;
    use crate::tick::{SimSet, SimTick};
    use bd_core::PlayerId;
    use bd_core::input::PlayerInput;
    use bevy::prelude::*;

    #[derive(Resource, Clone, Debug, Default)]
    pub struct SurfScenario {
        line: usize,
        placed: bool,
        t: f32,
        start_tick: u64,
        watch: bd_core::Vec3,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "surf") {
            return;
        }
        let line = args.windows(2).find(|w| w[0] == "--surf-line").and_then(|w| w[1].parse().ok()).unwrap_or(0);
        app.insert_resource(SurfScenario { line, ..default() })
            .add_systems(Update, place.run_if(resource_exists::<ActiveWorld>))
            .add_systems(SimTick, drive.in_set(SimSet::Input).run_if(resource_exists::<ActiveWorld>));
    }

    /// (face, ridge direction, start feet position) for line `k`.
    fn start_of(w: &bd_sim::World, k: usize) -> Option<(bd_sim::movement::Collider, bd_core::Vec3, bd_core::Vec3)> {
        let l = *w.arena.surf_lines.get(k)?;
        let face = bd_sim::movement::surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0)[0];
        let d = (l.b - l.a).normalize();
        let n = face.axes()[1];
        let point = l.a + (l.b - l.a) * 0.22 + face.axes()[0] * 2.5;
        let corner = bd_core::Vec3::new(-0.4 * n.x.signum(), 0.0, -0.4 * n.z.signum());
        Some((face, d, point - corner + n * 0.05))
    }

    fn place(mut sc: ResMut<SurfScenario>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
        sc.t += time.delta_secs();
        if sc.placed || sc.t < 0.5 {
            return;
        }
        let Some(me) = active.local else { return };
        let Some(l) = active.world.arena.surf_lines.get(sc.line).copied() else {
            warn!("surf scenario: no surf line {} on this map", sc.line);
            sc.placed = true;
            return;
        };
        // Camera: beside the line's lower half, looking across at it.
        let d = (l.b - l.a).normalize();
        let side = d.cross(bd_core::Vec3::Y).normalize();
        let mid = l.a + (l.b - l.a) * 0.62;
        let eye = mid + side * 22.0 + bd_core::Vec3::Y * 2.0;
        let world = &mut active.world;
        if let Some(p) = world.player_mut(me) {
            p.mv.pos = eye - bd_core::Vec3::Y * 1.6;
            p.mv.vel = bd_core::Vec3::ZERO;
        }
        sc.watch = eye - bd_core::Vec3::Y * 1.6;
        let to = (l.a + (l.b - l.a) * 0.5 - eye).normalize();
        look.yaw = (-to.x).atan2(-to.z);
        look.pitch = to.y.asin();
        active.bots.clear();
        sc.start_tick = active.world.tick;
        sc.placed = true;
        info!("surf scenario: line {} {:?} → {:?}", sc.line, l.a, l.b);
    }

    fn drive(sc: Res<SurfScenario>, mut active: ResMut<ActiveWorld>, look: Res<LookAngles>) {
        if !sc.placed {
            return;
        }
        let tick = active.world.tick;
        let k = tick.saturating_sub(sc.start_tick) % 480;
        let Some((face, d, start)) = start_of(&active.world, sc.line) else { return };
        let surfer = PlayerId(1);
        if k == 0
            && let Some(p) = active.world.player_mut(surfer)
        {
            p.mv.pos = start;
            p.mv.vel = d * 10.0;
            p.mv.grounded = false;
            p.mv.air_time = 0.5;
            p.spawn_shield = 0.0;
        }
        // Hold the strafe key that pushes into the face, view along the line.
        let n = face.axes()[1];
        let yaw = (-d.x).atan2(-d.z);
        let right = bd_core::Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let mx = if bd_core::Vec3::new(-n.x, 0.0, -n.z).dot(right) > 0.0 { 1 } else { -1 };
        active.remote_inputs.push((surfer, PlayerInput { seq: tick as u32, yaw, move_x: mx, ..Default::default() }));
        // The watcher hovers where it was put.
        if let Some(me) = active.local
            && let Some(p) = active.world.player_mut(me)
        {
            p.mv.pos = sc.watch;
            p.mv.vel = bd_core::Vec3::ZERO;
        }
        let _ = look;
    }
}
