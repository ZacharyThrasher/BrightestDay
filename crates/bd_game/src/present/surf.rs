//! Surf lines (sim: `ArenaInfo::surf_lines` → `bd_sim::movement::surf_line` colliders).
//!
//! * **Lines** — only the rideable top of each face is drawn, exactly on its collider (what you see
//!   is what you ride): a faint one-sided hard-light fill plus bright edge lines (ridge, lip, ends).
//!   The fill fades per vertex with grazing view angle and distance, so looking down a run (or into
//!   a curl of stacked lines) stays clear while the edges keep every face readable at a glance.
//!   Two merged meshes for the whole map, rebuilt whenever the active world's lines change.
//! * **Wind rush** (every map except the space maps, which keep their own lines): a few thin
//!   streaks rush past the camera in the periphery along the velocity, scaled with speed — faint
//!   from sprint + bhop speed (~13 m/s), strong on tether swings, launches and fast surfs. One small
//!   pool of additive bars, comfort-gated (off without speed FOV; scaled by FOV kicks). The wind
//!   loop (`audio.rs` own loops) already follows speed in loudness and pitch.
//! * `--scenario surf [--surf-line N]` — record / screenshot scene (see [`scenario`]).

use super::{bq, bv};
use crate::camera::PlayerCamera;
use crate::player::{LocalPlayer, MoveTuning, PlayerSim};
use crate::settings::ComfortSettings;
use crate::sim_bridge::ActiveWorld;
use crate::state::AppState;
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

const STREAKS: usize = 12;
/// Fill faces are cut along the run into pieces at most this long (m) so the per-vertex fade reads.
const FILL_SEG: f32 = 4.0;
/// Fill opacity seen face-on and near.
const FILL_ALPHA: f32 = 0.26;
/// Edge line thickness (m).
const EDGE: f32 = 0.07;

pub struct SurfPlugin;

impl Plugin for SurfPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurfDraw>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (sync_lines, fade_fill, wind_rush).chain().in_set(crate::tick::FrameSet::Present));
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
    /// The merged fill mesh and its (position, face normal) per vertex, for the view fade.
    fill: Option<Handle<Mesh>>,
    fill_verts: Vec<(Vec3, Vec3)>,
    /// Camera position the fill was last faded for.
    faded_at: Option<Vec3>,
    streak_mat: Option<Handle<StandardMaterial>>,
    level: f32,
}

type SurfEntities<'w, 's> = Query<'w, 's, Entity, Or<(With<SurfSlab>, With<SurfStreak>)>>;

fn clear(mut commands: Commands, mut draw: ResMut<SurfDraw>, q: SurfEntities) {
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
    let (mut fpos, mut fidx, mut verts) = (Vec::new(), Vec::new(), Vec::new());
    let (mut epos, mut enrm, mut eidx) = (Vec::new(), Vec::new(), Vec::new());
    for (i, l) in lines.iter().enumerate() {
        for (k, c) in bd_sim::movement::surf_line(l.a, l.b, l.face_deg, l.width, 1.5, i as u32 * 2).iter().enumerate() {
            let (h, rot) = (bv(c.half), bq(c.rot));
            // Collider axes: X across the slope (ridge -> lip), Y = the ride normal, Z along the run.
            let (f, n, z) = (rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z);
            let top = bv(c.center) + n * h.y;
            let (ridge, lip) = (top - f * h.x, top + f * h.x);
            let segs = ((2.0 * h.z) / FILL_SEG).ceil().max(1.0) as u32;
            let base = fpos.len() as u32;
            for sgi in 0..=segs {
                let t = -h.z + 2.0 * h.z * sgi as f32 / segs as f32;
                for p in [ridge + z * t, lip + z * t] {
                    fpos.push(p.to_array());
                    verts.push((p, n));
                }
            }
            for sgi in 0..segs {
                let b = base + sgi * 2;
                fidx.extend_from_slice(&[b, b + 3, b + 1, b, b + 2, b + 3]);
            }
            // Edges sit a hair above the face; the ridge is shared by the pair, so draw it once.
            let (r0, r1, l0, l1) = (ridge - z * h.z, ridge + z * h.z, lip - z * h.z, lip + z * h.z);
            let lift = n * (EDGE * 0.5);
            if k == 0 {
                edge_box(r0 + lift, r1 + lift, &mut epos, &mut enrm, &mut eidx);
            }
            for (a, b) in [(l0, l1), (r0, l0), (r1, l1)] {
                edge_box(a + lift, b + lift, &mut epos, &mut enrm, &mut eidx);
            }
        }
    }
    draw.fill = None;
    draw.faded_at = None;
    if !fpos.is_empty() {
        let nrm: Vec<[f32; 3]> = verts.iter().map(|(_, n)| n.to_array()).collect();
        let col = vec![[1.0, 1.0, 1.0, FILL_ALPHA]; fpos.len()];
        let fill = meshes.add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, fpos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
                .with_inserted_indices(Indices::U32(fidx)),
        );
        let fill_mat = mats.add(StandardMaterial {
            base_color: Color::srgba(0.62, 0.92, 1.0, 1.0),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        commands.spawn((SurfSlab, Mesh3d(fill.clone()), MeshMaterial3d(fill_mat), Transform::IDENTITY, NotShadowCaster, NotShadowReceiver));
        let edges = meshes.add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, epos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, enrm)
                .with_inserted_indices(Indices::U32(eidx)),
        );
        let edge_mat = mats.add(StandardMaterial {
            base_color: Color::LinearRgba(LinearRgba::rgb(1.1, 2.6, 3.4)),
            unlit: true,
            ..default()
        });
        commands.spawn((SurfSlab, Mesh3d(edges), MeshMaterial3d(edge_mat), Transform::IDENTITY, NotShadowCaster, NotShadowReceiver));
        draw.fill = Some(fill);
    }
    draw.fill_verts = verts;
    draw.lines = Some(lines.clone());
}

/// A thin square bar from `a` to `b` (8 corners, 12 triangles) appended to a merged edge mesh.
fn edge_box(a: Vec3, b: Vec3, pos: &mut Vec<[f32; 3]>, nrm: &mut Vec<[f32; 3]>, idx: &mut Vec<u32>) {
    let d = (b - a).normalize_or(Vec3::Z);
    let u = d.any_orthonormal_vector() * (EDGE * 0.5);
    let v = d.cross(u);
    let base = pos.len() as u32;
    for p in [a, b] {
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let o = u * su + v * sv;
            pos.push((p + o).to_array());
            nrm.push(o.normalize_or(Vec3::Y).to_array());
        }
    }
    // Four sides (quad j: corners j, j+1 at `a` and the same at `b`) plus the two end caps.
    for j in 0..4u32 {
        let (p0, p1) = (base + j, base + (j + 1) % 4);
        let (q0, q1) = (p0 + 4, p1 + 4);
        idx.extend_from_slice(&[p0, p1, q1, p0, q1, q0]);
    }
    idx.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    idx.extend_from_slice(&[base + 4, base + 5, base + 6, base + 4, base + 6, base + 7]);
}

/// Fade the fill per vertex by view angle (grazing -> faint) and distance, when the camera moves.
fn fade_fill(mut draw: ResMut<SurfDraw>, cam: Query<&GlobalTransform, With<PlayerCamera>>, mut meshes: ResMut<Assets<Mesh>>) {
    let (Some(fill), Ok(cam)) = (draw.fill.clone(), cam.single()) else { return };
    let eye = cam.translation();
    if draw.faded_at.is_some_and(|p| p.distance_squared(eye) < 0.05 * 0.05) {
        return;
    }
    let col: Vec<[f32; 4]> = draw
        .fill_verts
        .iter()
        .map(|&(p, n)| {
            let d = p - eye;
            let dist = d.length().max(1e-3);
            let facing = (d.dot(n) / dist).abs();
            let ang = smooth(0.06, 0.45, facing);
            let near = 1.0 - smooth(30.0, 110.0, dist);
            [1.0, 1.0, 1.0, FILL_ALPHA * (0.1 + 0.9 * ang) * (0.2 + 0.8 * near)]
        })
        .collect();
    if let Some(mut m) = meshes.get_mut(&fill) {
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    }
    draw.faded_at = Some(eye);
}

#[cfg(test)]
mod tests {
    use super::rush_level;

    #[test]
    fn rush_is_quiet_at_run_speed_and_strong_on_swings() {
        assert_eq!(rush_level(10.0, false, false, false), 0.0);
        let hop = rush_level(18.0, false, false, false);
        assert!(hop > 0.0 && hop < 0.2, "bhop {hop}");
        assert!(rush_level(30.0, false, true, false) > 0.75);
        assert!(rush_level(30.0, false, false, true) > 0.75);
        assert!(rush_level(30.0, true, false, false) > rush_level(30.0, false, false, false));
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Wind-rush strength 0..1 for a speed and movement context (before comfort scaling).
fn rush_level(speed: f32, surfing: bool, swinging: bool, launched: bool) -> f32 {
    let r = |lo: f32, span: f32| ((speed - lo) / span).clamp(0.0, 1.0);
    let mut k = 0.6 * r(13.0, 27.0);
    if surfing {
        k = k.max(r(15.0, 25.0));
    }
    if swinging || launched {
        k = k.max(r(12.0, 22.0));
    }
    k
}

#[allow(clippy::too_many_arguments)]
fn wind_rush(
    mut commands: Commands,
    time: Res<Time<Real>>,
    comfort: Res<ComfortSettings>,
    tuning: Res<MoveTuning>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<crate::sim_bridge::WorldViews>,
    me: Query<&PlayerSim, With<LocalPlayer>>,
    cam: Query<&Transform, (With<PlayerCamera>, Without<SurfStreak>)>,
    mut streaks: Query<(&SurfStreak, &mut Transform, &mut Visibility)>,
    mut draw: ResMut<SurfDraw>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let comfort_k = if comfort.speed_fov { comfort.fov_kick.clamp(0.0, 1.0) } else { 0.0 };
    let space = arena.as_ref().is_some_and(|a| matches!(a.info.kind, crate::arena::MapKind::Orbit | crate::arena::MapKind::Cybertron));
    let (surfing, swinging, vel) = me
        .single()
        .map_or((false, false, Vec3::ZERO), |s| (bd_sim::movement::surfing(&s.state, &tuning.0), s.state.tethered(), bv(s.state.vel)));
    let view = active.as_ref().and_then(|a| a.local).and_then(|id| views.player(id));
    let alive = view.is_none_or(|v| v.alive());
    let launched = view.is_some_and(|v| v.has(bd_sim::replicate::pflags::LAUNCHED));
    let speed = vel.length();
    let want = if space || !alive { 0.0 } else { rush_level(speed, surfing, swinging, launched) * comfort_k };
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
        let eye = mid + side * 13.0 + bd_core::Vec3::Y * 1.0;
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
