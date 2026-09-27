//! First person: one glowing gem per held power set on the ring hand (a row across the knuckles,
//! just above the lantern ring), in that power's aura colour. Cooling = small + dim, refilling
//! grows and brightens it, ready = full glow + a flare; a form burns (flicker) inside a ring that
//! drains with its time left. Placed from `ViewmodelHands::local` (the right fist in camera
//! space), so it follows sway / recoil / swings without touching the viewmodel's internals.
//!
//! Single source for the wordless-affordance pass: extend these gems (and read `PowerHudState`)
//! for the ring gauge rather than adding a second one.

use super::derive::SLOTS;
use super::PowerHudState;
use crate::camera::PlayerCamera;
use crate::viewmodel::ViewmodelHands;
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Form-ring fill steps (one prebuilt arc mesh per step).
const ARC_STEPS: usize = 16;
/// Camera-space offset of the gem row from the right fist point (up, toward the camera).
const ROW_OFFSET: Vec3 = Vec3::new(0.006, 0.066, 0.0);

/// `BD_GEM_OFFSET=x,y,z` (dev tuning): overrides [`ROW_OFFSET`].
fn row_offset() -> Vec3 {
    static O: std::sync::OnceLock<Vec3> = std::sync::OnceLock::new();
    *O.get_or_init(|| {
        std::env::var("BD_GEM_OFFSET")
            .ok()
            .and_then(|s| {
                let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
            })
            .unwrap_or(ROW_OFFSET)
    })
}
const SPACING: f32 = 0.019;
const GEM_R: f32 = 0.0072;

#[derive(Component)]
struct GemRoot;

struct GemRig {
    gem: Entity,
    ring: Entity,
    gem_mat: Handle<StandardMaterial>,
    ring_mat: Handle<StandardMaterial>,
    key: [f32; 4],
    ring_key: [f32; 3],
    arc: usize,
}

#[derive(Resource)]
struct Gems {
    root: Entity,
    rigs: Vec<GemRig>,
    arcs: Vec<Handle<Mesh>>,
    t: f32,
}

pub fn build(app: &mut App) {
    app.add_systems(Update, (attach, draw).chain().in_set(crate::present::PresentSet::Sync));
}

/// A flat annulus sector in the XY plane from 12 o'clock clockwise (seen from +Z), `frac` of a turn.
fn arc_mesh(frac: f32) -> Mesh {
    let segs = ((48.0 * frac).ceil() as usize).max(1);
    let (r0, r1) = (0.78, 1.0);
    let mut pos = Vec::with_capacity((segs + 1) * 2);
    let mut uv = Vec::with_capacity((segs + 1) * 2);
    for i in 0..=segs {
        let a = std::f32::consts::TAU * frac * i as f32 / segs as f32;
        let (s, c) = a.sin_cos();
        pos.push([s * r0, c * r0, 0.0]);
        pos.push([s * r1, c * r1, 0.0]);
        uv.push([i as f32 / segs as f32, 0.0]);
        uv.push([i as f32 / segs as f32, 1.0]);
    }
    let mut idx = Vec::with_capacity(segs * 6);
    for i in 0..segs as u32 {
        let b = i * 2;
        idx.extend_from_slice(&[b, b + 1, b + 2, b + 1, b + 3, b + 2]);
    }
    let n = pos.len();
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; n]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(idx));
    m
}

fn attach(
    mut commands: Commands,
    gems: Option<Res<Gems>>,
    cams: Query<Entity, With<PlayerCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if gems.is_some() {
        return;
    }
    let Ok(cam) = cams.single() else { return };
    let gem_mesh = meshes.add(Sphere::new(1.0).mesh().ico(0).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(8, 6)));
    let arcs: Vec<Handle<Mesh>> = (0..=ARC_STEPS).map(|k| meshes.add(arc_mesh((k.max(1) as f32 / ARC_STEPS as f32).min(1.0)))).collect();
    let root = commands.spawn((Name::new("PowerGems"), GemRoot, Transform::default(), Visibility::Hidden)).id();
    commands.entity(cam).add_child(root);
    let mut rigs = Vec::with_capacity(SLOTS);
    for _ in 0..SLOTS {
        let gem_mat = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(1.0, 1.0, 1.0), unlit: true, ..default() });
        let ring_mat = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(2.0, 2.0, 2.0), unlit: true, cull_mode: None, ..default() });
        let gem = commands.spawn((Mesh3d(gem_mesh.clone()), MeshMaterial3d(gem_mat.clone()), Transform::from_scale(Vec3::splat(GEM_R)), Visibility::Hidden, NotShadowCaster)).id();
        let ring = commands.spawn((Mesh3d(arcs[ARC_STEPS].clone()), MeshMaterial3d(ring_mat.clone()), Transform::from_scale(Vec3::splat(GEM_R * 2.2)), Visibility::Hidden, NotShadowCaster)).id();
        commands.entity(root).add_children(&[gem, ring]);
        rigs.push(GemRig { gem, ring, gem_mat, ring_mat, key: [-1.0; 4], ring_key: [-1.0; 3], arc: ARC_STEPS });
    }
    commands.insert_resource(Gems { root, rigs, arcs, t: 0.0 });
}

fn vis(on: bool) -> Visibility {
    if on { Visibility::Inherited } else { Visibility::Hidden }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    time: Res<Time>,
    st: Res<PowerHudState>,
    hands: Res<ViewmodelHands>,
    gems: Option<ResMut<Gems>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
    mut meshes_q: Query<&mut Mesh3d>,
) {
    let Some(mut g) = gems else { return };
    let g = &mut *g;
    g.t += time.delta_secs();
    let hud = &st.0;
    let show = hands.active && hud.count > 0;
    if let Ok((mut tf, mut v)) = q.get_mut(g.root) {
        if *v != vis(show) {
            *v = vis(show);
        }
        if show {
            tf.translation = hands.local[1] + row_offset();
        }
    }
    if !show {
        return;
    }
    let n = hud.count.max(1) as f32;
    let mut col = 0.0;
    for (k, rig) in g.rigs.iter_mut().enumerate() {
        let gem = &hud.gems[k];
        let on = gem.power.is_some();
        // Row position: held gems packed and centred, in display-slot order.
        let x = (col - (n - 1.0) * 0.5) * SPACING;
        if on {
            col += 1.0;
        }
        let burning = gem.form.is_some();
        let flick = if burning { 1.0 + 0.14 * (g.t * 23.0 + k as f32).sin() * (g.t * 9.0).cos() } else { 1.0 };
        let size = GEM_R * (if gem.active { 1.2 } else { 1.0 }) * (0.5 + 0.5 * gem.shown) * (1.0 + 0.7 * gem.flash) * flick;
        if let Ok((mut tf, mut v)) = q.get_mut(rig.gem) {
            if *v != vis(on) {
                *v = vis(on);
            }
            tf.translation = Vec3::new(x, 0.0, 0.0);
            tf.scale = Vec3::splat(size);
            tf.rotation = Quat::from_rotation_y(g.t * 0.8 + k as f32) * Quat::from_rotation_x(0.6);
        }
        if on {
            // Brightness: dim while cooling, full when ready, a white flare on the edge.
            let c = gem.look.color;
            let ready = gem.shown;
            let glow = if hud.drained { 0.3 } else if gem.is_ready { 12.0 } else { 0.35 + 1.6 * ready * ready };
            let white = gem.flash * 10.0;
            let base = if burning { gem.look.form_color } else { c };
            let key = [base[0] * glow + white, base[1] * glow + white, base[2] * glow + white, if gem.is_ready { 1.0 } else { 0.35 }];
            if key.iter().zip(rig.key.iter()).map(|(a, b)| (a - b).abs()).sum::<f32>() > 0.05
                && let Some(mut m) = mats.get_mut(&rig.gem_mat)
            {
                rig.key = key;
                m.base_color = Color::linear_rgb(base[0] * key[3], base[1] * key[3], base[2] * key[3]);
                m.emissive = LinearRgba::rgb(key[0], key[1], key[2]);
            }
        }
        // Form ring: drains with the form's time left.
        let form = gem.form.filter(|_| on);
        if let Ok((mut tf, mut v)) = q.get_mut(rig.ring) {
            if *v != vis(form.is_some()) {
                *v = vis(form.is_some());
            }
            if form.is_some() {
                tf.translation = Vec3::new(x, 0.0, 0.0);
                tf.scale = Vec3::splat(GEM_R * 2.1 * (if gem.active { 1.2 } else { 1.0 }) * (1.0 + 0.05 * (g.t * 11.0).sin()));
            }
        }
        if let Some(f) = form {
            let step = ((f * ARC_STEPS as f32).ceil() as usize).clamp(1, ARC_STEPS);
            if step != rig.arc
                && let Ok(mut m) = meshes_q.get_mut(rig.ring)
            {
                rig.arc = step;
                m.0 = g.arcs[step].clone();
            }
            let fc = gem.look.form_color;
            let rk = [fc[0] * 5.0 + 1.5, fc[1] * 5.0 + 1.3, fc[2] * 5.0 + 0.8];
            if rk.iter().zip(rig.ring_key.iter()).map(|(a, b)| (a - b).abs()).sum::<f32>() > 0.05
                && let Some(mut m) = mats.get_mut(&rig.ring_mat)
            {
                rig.ring_key = rk;
                m.base_color = Color::linear_rgb(fc[0], fc[1], fc[2]);
                m.emissive = LinearRgba::rgb(rk[0], rk[1], rk[2]);
            }
        }
    }
}
