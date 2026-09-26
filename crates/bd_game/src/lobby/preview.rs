//! Costume creator turntable: the costume for the current phrase / seed / tweaks, meshed from
//! `bd_shapes::costume_model` and shown to the right of the menu (a child of the camera, so it
//! stays put while the backdrop orbits). The player turns / tilts / zooms it
//! (`lobby::costume_view`); idle, it spins slowly. Rebuilt 150 ms after the last change.

use crate::camera::PlayerCamera;
use crate::console::RingLexicon;
use crate::lobby::costume_view::CostumeView;
use crate::lobby::prefs::UiPrefs;
use crate::menu::{costume_page_open, MenuState};
use crate::state::{AppState, InGameMenu};
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

#[derive(Component)]
pub struct PreviewRoot;
/// Rotated / scaled node between the root (the on-screen pivot) and the parts.
#[derive(Component)]
pub struct PreviewSpin;
#[derive(Component)]
struct PreviewPart;

/// Where the model's feet sat in camera space before free rotation (the default framing).
const BASE: Vec3 = Vec3::new(1.25, -0.95, -3.6);
const FACING: f32 = std::f32::consts::PI;

/// Root (pivot) and spin transforms for the current view. `center` / `height` are the model's
/// bounds (model space); the view turns around its centre, and zooming in drifts the focus up
/// toward the chest / head. At the default view this is exactly the old framing.
fn placement(view: &CostumeView, center: Vec3, height: f32, scale: f32) -> (Transform, Transform) {
    let pivot = (BASE + Quat::from_rotation_y(FACING) * center) * view.zoom;
    let focus = center + Vec3::Y * height * 0.22 * view.zoom_t();
    let rot = view.rotation(FACING);
    (Transform::from_translation(pivot), Transform { translation: -(rot * (focus * scale)), rotation: rot, scale: Vec3::splat(scale) })
}

#[derive(Default)]
pub struct PreviewState {
    built: Option<String>,
    changed_at: f32,
    pending: Option<String>,
    mat: Option<Handle<StandardMaterial>>,
    pop: f32,
    center: Vec3,
    height: f32,
}

fn key_of(p: &UiPrefs) -> String {
    let c = &p.costume;
    format!("{}|{}|{:?}|{}|{}|{:?}", c.phrase.trim().to_lowercase(), c.seed, c.accent, c.head_scale, c.hide, p.corps)
}

pub fn part_mesh(pm: &bd_shapes::PartMesh, palette: &[[f32; 3]; 4]) -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pm.positions.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, pm.normals.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, pm.colors(palette));
    m.insert_indices(Indices::U32(pm.indices.clone()));
    m
}

#[allow(clippy::too_many_arguments)]
pub fn costume_preview(
    mut commands: Commands,
    time: Res<Time<Real>>,
    menu: Res<MenuState>,
    state: Res<State<AppState>>,
    pause: Res<InGameMenu>,
    prefs: Res<UiPrefs>,
    lex: Option<Res<RingLexicon>>,
    view: Res<CostumeView>,
    cams: Query<Entity, With<PlayerCamera>>,
    mut roots: Query<(Entity, &mut Transform), With<PreviewRoot>>,
    mut spins: Query<&mut Transform, (With<PreviewSpin>, Without<PreviewRoot>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut st: Local<PreviewState>,
) {
    let open = costume_page_open(&menu, state.get(), &pause);
    if !open {
        if !roots.is_empty() {
            for (e, _) in &roots {
                commands.entity(e).despawn();
            }
            st.built = None;
        }
        return;
    }
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    st.pop = (st.pop + dt * 4.0).min(1.0);
    let key = key_of(&prefs);
    if st.pending.as_ref() != Some(&key) && st.built.as_ref() != Some(&key) {
        st.pending = Some(key.clone());
        st.changed_at = now;
    }
    // View (turn / tilt / zoom from `CostumeView`) and a little pop when it rebuilds.
    let e = 1.0 - (1.0 - st.pop).powi(3);
    let (root_tf, spin_tf) = placement(&view, st.center, st.height, 0.85 + 0.15 * e);
    for (_, mut tf) in &mut roots {
        *tf = root_tf;
    }
    for mut tf in &mut spins {
        *tf = spin_tf;
    }
    let Some(lex) = lex else { return };
    let ready = st.pending.is_some() && (now - st.changed_at > 0.15 || st.built.is_none());
    if !ready {
        return;
    }
    let Ok(cam) = cams.single() else { return };
    st.built = st.pending.take();
    st.pop = 0.0;
    for (e, _) in &roots {
        commands.entity(e).despawn();
    }
    let spec = crate::lobby::costume::costume_spec(&lex.0, &prefs.costume);
    let model = bd_shapes::costume_model(&spec);
    let parts = bd_shapes::mesh_model(&model);
    let cc = prefs.corps.color();
    let accent = crate::lobby::costume::costume_accent(&lex.0, &prefs.costume).unwrap_or([cc[0] * 0.5 + 0.5, cc[1] * 0.5 + 0.5, cc[2] * 0.5 + 0.5]);
    let palette = [cc.map(|x| x * 0.85 + 0.05), accent, cc.map(|x| x * 0.25 + 0.03), [1.0, 1.0, 0.95]];
    let mat = st.mat.get_or_insert_with(|| mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.6, emissive: LinearRgba::rgb(0.05, 0.05, 0.05), ..default() })).clone();
    // Model bounds: the view turns around the centre.
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in parts.iter().flat_map(|p| p.positions.iter()) {
        lo = lo.min(Vec3::from(*p));
        hi = hi.max(Vec3::from(*p));
    }
    (st.center, st.height) = if lo.x <= hi.x { ((lo + hi) * 0.5, hi.y - lo.y) } else { (Vec3::Y * 0.9, 1.8) };
    let (root_tf, spin_tf) = placement(&view, st.center, st.height, 0.85);
    let root = commands.spawn((Name::new("CostumePreview"), PreviewRoot, root_tf, Visibility::Inherited)).id();
    commands.entity(cam).add_child(root);
    let spin = commands.spawn((PreviewSpin, spin_tf, Visibility::Inherited)).id();
    commands.entity(root).add_child(spin);
    for pm in parts.iter().filter(|p| !p.is_empty()) {
        let mesh = meshes.add(part_mesh(pm, &palette));
        let child = commands.spawn((PreviewPart, Mesh3d(mesh), MeshMaterial3d(mat.clone()), Transform::default(), NotShadowCaster)).id();
        commands.entity(spin).add_child(child);
    }
    // A soft key light so the preview reads against any backdrop (front-right-above in camera
    // space; it does not turn with the model, so a back view is lit too).
    let light = commands.spawn((PointLight { intensity: 60_000.0, range: 8.0, color: Color::WHITE, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(1.5, 2.2 - st.center.y, 1.5))).id();
    commands.entity(root).add_child(light);
}
