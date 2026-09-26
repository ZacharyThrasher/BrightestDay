//! Construct entities: `ConstructVisual` → cached per-part meshes (worker threads) → child part
//! entities posed every frame with `bd_shapes::pose_layered` (allocation-free).

use std::collections::HashMap;
use std::sync::Arc;

use bd_shapes::{AnimState, ShapeKey, VoxelModel};
use bevy::camera::visibility::VisibilitySystems;
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};

use crate::effects::{ShatterScratch, shatter_model};
use crate::gpu::{AnchorGpu, ConstructGpu, FxUpload, MAX_ANCHORS, MAX_SLOTS};
use crate::material::FxMaterials;
use crate::meshing::{BuiltModel, VoxelColors, build_model, build_model_colored};
use crate::palette::Palette;
use crate::particles::{Emit, EmitSystems, FxParticles, PFlags, PKind};
use crate::util::{aff, fin, v3};
use crate::{FxSet, FxSettings, FxStats};

// ------------------------------------------------------------------------------------ public API

/// Up to 4 layered animations (see `bd_shapes::pose_layered`). Copy, allocation-free.
#[derive(Clone, Copy, Debug)]
pub struct AnimLayers {
    layers: [(AnimState, f32); 4],
    len: u8,
}

impl Default for AnimLayers {
    fn default() -> Self {
        // Locomotion at speed 0 == Idle but keeps machine wheel/crank phase.
        Self::single(AnimState::Locomotion { speed: 0.0, distance: 0.0, airborne: false }, 0.0)
    }
}

impl AnimLayers {
    pub fn single(state: AnimState, t: f32) -> Self {
        AnimLayers { layers: [(state, t); 4], len: 1 }
    }
    /// Replace the layers (extra layers beyond 4 are ignored).
    pub fn set(&mut self, layers: &[(AnimState, f32)]) {
        let n = layers.len().min(4);
        self.layers[..n].copy_from_slice(&layers[..n]);
        self.len = n as u8;
    }
    pub fn as_slice(&self) -> &[(AnimState, f32)] {
        &self.layers[..self.len as usize]
    }
}

/// A hard-light construct. Put it on an entity with a `Transform` = the construct's pivot in world
/// space (grip for held items, feet-centre for creatures/structures; model faces −Z).
#[derive(Component, Clone)]
#[require(Transform, Visibility)]
pub struct ConstructVisual {
    pub model: Arc<VoxelModel>,
    /// Mesh-cache key (same phrase ⇒ same meshes). `None` caches by the `Arc` pointer instead.
    pub key: Option<ShapeKey>,
    /// Owner's corps colour, linear RGB (`bd_core::corps::Corps::color()`).
    pub corps_color: [f32; 3],
    /// Recipe accent colour (defaults to a lighter corps colour).
    pub accent: Option<[f32; 3]>,
    /// Animation layers evaluated every frame (the game advances the times).
    pub layers: AnimLayers,
    /// 0 → 1 while forming (voxels grow in from `ring`); 1 = fully formed.
    pub forming: f32,
    /// 1 = pristine … 0 = about to shatter (flicker, cracks below ~0.8).
    pub integrity: f32,
    /// Extra white flash 0..1 (clashes also flash automatically, see [`crate::ClashFx`]).
    pub flash: f32,
    /// 0..1 darken/fade (e.g. owner out of willpower).
    pub dim: f32,
    /// World position of the owner's ring: forming origin + energy-arc source.
    pub ring: Option<Vec3>,
    /// Draw the ring → construct energy arc (while forming / controlling).
    pub arc: bool,
    /// Body opacity for the translucent style (0.2 ghostly … 1 solid).
    pub opacity: f32,
    /// Cosmetic seed (shimmer phase, crack pattern).
    pub seed: u32,
    /// 0..1 heat glow (minigun barrels, overcharged weapons): orange-white bloom.
    pub heat: f32,
    /// Per-voxel colours (terrain animated by a graft keeps its materials, tinted with hard light).
    /// Only read when the model is first meshed; pair it with a per-construct model (`key: None`).
    pub colors: Option<Arc<VoxelColors>>,
}

impl ConstructVisual {
    pub fn new(model: Arc<VoxelModel>, corps_color: [f32; 3]) -> Self {
        ConstructVisual {
            model,
            key: None,
            corps_color,
            accent: None,
            layers: AnimLayers::default(),
            forming: 1.0,
            integrity: 1.0,
            flash: 0.0,
            dim: 0.0,
            ring: None,
            arc: false,
            opacity: 0.72,
            seed: 0,
            heat: 0.0,
            colors: None,
        }
    }
    pub fn with_key(mut self, key: ShapeKey) -> Self {
        self.key = Some(key);
        self
    }
    pub fn with_colors(mut self, colors: Arc<VoxelColors>) -> Self {
        self.colors = Some(colors);
        self
    }
    pub fn with_accent(mut self, accent: [f32; 3]) -> Self {
        self.accent = Some(accent);
        self
    }
    /// Start in the forming state (forming = 0).
    pub fn forming_from(mut self, ring: Vec3) -> Self {
        self.forming = 0.0;
        self.ring = Some(ring);
        self.arc = true;
        self
    }
}

/// Convenience: bd_fx advances `ConstructVisual::forming` 0 → 1 over `duration` seconds (and turns
/// the arc off at the end), then removes this component.
#[derive(Component, Clone, Copy, Debug)]
pub struct AutoForm {
    pub duration: f32,
    pub elapsed: f32,
}

impl AutoForm {
    pub fn new(duration: f32) -> Self {
        AutoForm { duration, elapsed: 0.0 }
    }
}

/// Insert to end a construct. bd_fx plays the effect and despawns the entity (and its parts).
#[derive(Component, Clone, Debug)]
pub enum ConstructEnd {
    /// Shatter into its own voxels. `impulse` = velocity (m/s) added to the shards (the hit's
    /// push); `point` = hit point (world) for the radial burst; `floor` = height shards settle on.
    Shatter { impulse: Vec3, point: Option<Vec3>, floor: Option<f32> },
    /// Voxels dissolve back into the ring over `duration` (dismiss / expire).
    Dismiss { duration: f32 },
    /// Despawn silently.
    Remove,
}

/// Part entities of a construct (children), for attaching things to parts.
#[derive(Component, Debug, Default, Clone)]
pub struct ConstructParts {
    /// (model part index, entity)
    pub parts: Vec<(usize, Entity)>,
}

/// Mesh-cache key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModelKey {
    Shape(ShapeKey),
    Ptr(usize),
    /// bd_fx-internal models (portal rings, glyphs), keyed by name + size.
    Named(&'static str, u32),
}

impl ModelKey {
    pub fn of(v: &ConstructVisual) -> Self {
        match &v.key {
            Some(k) => ModelKey::Shape(k.clone()),
            None => ModelKey::Ptr(Arc::as_ptr(&v.model) as usize),
        }
    }
}

#[derive(Clone)]
pub struct CachedPart {
    pub mesh: Handle<Mesh>,
    pub offset: Vec3,
}

/// Cached part meshes per model (same phrase ⇒ same meshes; identical parts share one mesh).
#[derive(Resource, Default)]
pub struct ConstructMeshCache {
    models: HashMap<ModelKey, Arc<Vec<Option<CachedPart>>>>,
    pending: HashMap<ModelKey, Task<BuiltModel>>,
    by_content: HashMap<u64, Handle<Mesh>>,
    /// Keeps `Ptr`-keyed models alive so their address cannot be reused by another model.
    keepalive: HashMap<ModelKey, Arc<VoxelModel>>,
    /// Meshed, waiting for their upload slot (see [`MAX_UPLOAD_VERTS_PER_FRAME`]), oldest first.
    uploading: Vec<(ModelKey, BuiltModel)>,
}

impl ConstructMeshCache {
    /// Start meshing a model now (e.g. loadout slots at match start). No-op if cached/pending.
    pub fn prewarm(&mut self, key: ModelKey, model: &Arc<VoxelModel>) {
        self.prewarm_colored(key, model, None);
    }
    /// [`Self::prewarm`] with per-voxel colours.
    pub fn prewarm_colored(&mut self, key: ModelKey, model: &Arc<VoxelModel>, colors: Option<Arc<VoxelColors>>) {
        if self.models.contains_key(&key) || self.pending.contains_key(&key) || self.uploading.iter().any(|(k, _)| *k == key) {
            return;
        }
        if matches!(key, ModelKey::Ptr(_)) {
            self.keepalive.insert(key.clone(), model.clone());
        }
        let m = model.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move { build_model_colored(&m, colors.as_deref()) });
        self.pending.insert(key, task);
    }
    pub fn get(&self, key: &ModelKey) -> Option<&Arc<Vec<Option<CachedPart>>>> {
        self.models.get(key)
    }
    pub fn is_ready(&self, key: &ModelKey) -> bool {
        self.models.contains_key(key)
    }
    /// Synchronous meshing (small bd_fx-internal models).
    pub(crate) fn insert_now(&mut self, key: ModelKey, model: &VoxelModel, meshes: &mut Assets<Mesh>) -> Arc<Vec<Option<CachedPart>>> {
        if let Some(m) = self.models.get(&key) {
            return m.clone();
        }
        let built = build_model(model);
        let parts = Arc::new(self.adopt(built, meshes));
        self.models.insert(key, parts.clone());
        parts
    }
    fn adopt(&mut self, built: BuiltModel, meshes: &mut Assets<Mesh>) -> Vec<Option<CachedPart>> {
        built
            .parts
            .into_iter()
            .map(|p| {
                p.map(|bp| {
                    let mesh = self.by_content.entry(bp.content).or_insert_with(|| meshes.add(bp.mesh)).clone();
                    CachedPart { mesh, offset: bp.offset }
                })
            })
            .collect()
    }
    /// Drop every cached mesh (e.g. between matches).
    pub fn clear(&mut self) {
        self.models.clear();
        self.uploading.clear();
        self.by_content.clear();
        self.keepalive.clear();
    }
    pub fn len(&self) -> usize {
        self.models.len()
    }
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

// ------------------------------------------------------------------------------------ slots

/// Construct-param slot of an entity (index into the construct storage buffer = part `MeshTag`).
#[derive(Component, Debug, Clone, Copy)]
pub struct FxSlot(pub u32);

/// Anchor slot of an entity (particles follow it).
#[derive(Component, Debug, Clone, Copy)]
pub struct FxAnchor(pub u32);

#[derive(Resource, Default)]
pub struct SlotAlloc {
    free: Vec<u32>,
    next: u32,
    anchor_free: std::collections::VecDeque<(u32, f64)>,
    anchor_next: u32,
    pub now: f64,
}

impl SlotAlloc {
    pub fn slot(&mut self) -> u32 {
        if let Some(s) = self.free.pop() {
            return s;
        }
        let s = self.next.min(MAX_SLOTS as u32 - 1);
        self.next = (self.next + 1).min(MAX_SLOTS as u32);
        s
    }
    pub fn anchor(&mut self) -> u32 {
        // Anchors are reused only after particles referencing them died (≤ 3 s).
        if let Some(&(a, t)) = self.anchor_free.front()
            && self.now - t > 3.0
        {
            self.anchor_free.pop_front();
            return a;
        }
        if self.anchor_next == 0 {
            self.anchor_next = 1; // 0 = world
        }
        let a = self.anchor_next.min(MAX_ANCHORS as u32 - 1);
        self.anchor_next = (self.anchor_next + 1).min(MAX_ANCHORS as u32);
        a
    }
}

pub fn write_slot(up: &mut FxUpload, slot: u32, data: ConstructGpu) {
    let i = slot as usize;
    if up.constructs.len() <= i {
        up.constructs.resize(i + 1, ConstructGpu::default());
    }
    up.constructs[i] = data;
}

pub fn write_anchor(up: &mut FxUpload, anchor: u32, data: AnchorGpu) {
    let i = anchor as usize;
    if up.anchors.len() <= i {
        up.anchors.resize(i + 1, AnchorGpu::IDENTITY);
    }
    up.anchors[i] = data;
}

pub fn construct_gpu(pal: &Palette, integrity: f32, reveal: f32, flash: f32, dim: f32, voxel: f32, seed: u32, opacity: f32, dissolving: bool, origin: Vec3, inv_radius: f32) -> ConstructGpu {
    let c = |i: usize| {
        let s = pal.slot(i);
        [s[0], s[1], s[2], 0.0]
    };
    ConstructGpu {
        primary: c(0),
        accent: c(1),
        dark: c(2),
        core: c(3),
        state: [fin(integrity, 1.0).clamp(0.0, 1.0), fin(reveal, 1.0).clamp(0.0, 1.0), fin(flash, 0.0).clamp(0.0, 1.0), fin(dim, 0.0).clamp(0.0, 1.0)],
        misc: [voxel, (seed % 100_000) as f32, fin(opacity, 0.7).clamp(0.05, 1.0), if dissolving { 1.0 } else { 0.0 }],
        origin: [origin.x, origin.y, origin.z, inv_radius],
    }
}

// ------------------------------------------------------------------------------------ runtime

#[derive(Component)]
pub(crate) struct ConstructRuntime {
    key: ModelKey,
    model_ptr: usize,
    pub pose: Vec<glam::Affine3A>,
    parts: Vec<(Entity, usize, Vec3)>,
    spawned: bool,
    translucent: bool,
    prev_pos: Vec3,
    pub vel: Vec3,
    prev_forming: f32,
    form_rate: f32,
    pub dissolve: Option<(f32, f32)>,
    stream_credit: f32,
    /// Cumulative voxel counts per part (random voxel sampling).
    cum: Vec<usize>,
    /// Automatic flash (clashes), decays.
    pub auto_flash: f32,
    /// Reveal value last written (arcs use it).
    pub reveal: f32,
    /// Machine-kit drive last frame (smokestack puffs).
    pub prev_drive: Option<bd_shapes::FeatureDrive>,
}

impl ConstructRuntime {
    pub(crate) fn cum_slice(&self) -> &[usize] {
        &self.cum
    }
    /// Entity rendering model part index.
    pub(crate) fn part_entity(&self, index: usize) -> Option<Entity> {
        self.parts.iter().find(|(_, i, _)| *i == index).map(|(e, _, _)| *e)
    }
}

fn cumulative(model: &VoxelModel) -> Vec<usize> {
    let mut acc = 0;
    model
        .parts
        .iter()
        .map(|p| {
            acc += p.voxels.len();
            acc
        })
        .collect()
}

/// Pick a random voxel: (part index, model-space rest centre).
pub(crate) fn random_voxel(model: &VoxelModel, cum: &[usize], r: u32) -> Option<(usize, glam::Vec3)> {
    let total = *cum.last()?;
    if total == 0 {
        return None;
    }
    let k = r as usize % total;
    let pi = cum.partition_point(|&c| c <= k);
    let part = model.parts.get(pi)?;
    let before = if pi == 0 { 0 } else { cum[pi - 1] };
    let (v, _) = part.voxels.get(k - before)?;
    Some((pi, model.voxel_center(*v)))
}

pub fn build(app: &mut App) {
    app.init_resource::<ConstructMeshCache>().init_resource::<SlotAlloc>().add_observer(free_slot).add_observer(free_anchor);
    app.add_systems(
        PostUpdate,
        (
            (init_constructs, poll_mesh_tasks, spawn_parts, apply_translucency, handle_end).chain().in_set(FxSet::Lifecycle),
            (advance_constructs, pose_constructs).chain().in_set(FxSet::Pose).in_set(PoseSystems),
            (write_construct_params, emit_streams).chain().in_set(EmitSystems).before(VisibilitySystems::CheckVisibility),
        ),
    );
}

/// The construct posing systems (run [crate::BarrelSpin]-style overrides after this).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoseSystems;

fn free_slot(ev: On<Remove, FxSlot>, q: Query<&FxSlot>, mut alloc: ResMut<SlotAlloc>) {
    if let Ok(s) = q.get(ev.entity) {
        alloc.free.push(s.0);
    }
}

fn free_anchor(ev: On<Remove, FxAnchor>, q: Query<&FxAnchor>, mut alloc: ResMut<SlotAlloc>) {
    if let Ok(a) = q.get(ev.entity) {
        let now = alloc.now;
        alloc.anchor_free.push_back((a.0, now));
    }
}

fn init_constructs(
    mut commands: Commands,
    mut alloc: ResMut<SlotAlloc>,
    mut cache: ResMut<ConstructMeshCache>,
    settings: Res<FxSettings>,
    time: Res<Time>,
    new: Query<(Entity, &ConstructVisual, &Transform), Without<ConstructRuntime>>,
    mut changed: Query<(Entity, &ConstructVisual, &mut ConstructRuntime), Changed<ConstructVisual>>,
) {
    alloc.now = time.elapsed_secs_f64();
    for (e, v, tf) in &new {
        let key = ModelKey::of(v);
        cache.prewarm_colored(key.clone(), &v.model, v.colors.clone());
        let slot = alloc.slot();
        let anchor = alloc.anchor();
        commands.entity(e).insert((
            FxSlot(slot),
            FxAnchor(anchor),
            ConstructParts::default(),
            ConstructRuntime {
                key,
                model_ptr: Arc::as_ptr(&v.model) as usize,
                pose: vec![glam::Affine3A::IDENTITY; v.model.parts.len()],
                parts: Vec::new(),
                spawned: false,
                translucent: settings.translucent,
                prev_pos: tf.translation,
                vel: Vec3::ZERO,
                prev_forming: v.forming,
                form_rate: 0.0,
                dissolve: None,
                stream_credit: 0.0,
                cum: cumulative(&v.model),
                auto_flash: 0.0,
                reveal: v.forming,
                prev_drive: None,
            },
        ));
    }
    // Model swapped (e.g. the construct was re-cast in place): rebuild parts.
    for (e, v, mut rt) in &mut changed {
        let ptr = Arc::as_ptr(&v.model) as usize;
        let key = ModelKey::of(v);
        if ptr == rt.model_ptr && key == rt.key {
            continue;
        }
        for (pe, _, _) in rt.parts.drain(..) {
            commands.entity(pe).despawn();
        }
        cache.prewarm_colored(key.clone(), &v.model, v.colors.clone());
        rt.key = key;
        rt.model_ptr = ptr;
        rt.pose = vec![glam::Affine3A::IDENTITY; v.model.parts.len()];
        rt.cum = cumulative(&v.model);
        rt.spawned = false;
        commands.entity(e).insert(ConstructParts::default());
    }
}

/// Finished meshes handed to the GPU per frame (vertices; at least one model per frame), so a burst
/// of first casts (or match-start prewarm) never uploads megabytes in one frame.
pub const MAX_UPLOAD_VERTS_PER_FRAME: usize = 120_000;

fn poll_mesh_tasks(mut cache: ResMut<ConstructMeshCache>, mut meshes: ResMut<Assets<Mesh>>, mut stats: ResMut<FxStats>) {
    let cache = &mut *cache;
    let done = &mut cache.uploading;
    cache.pending.retain(|k, task| {
        if task.is_finished()
            && let Some(built) = block_on(poll_once(task))
        {
            done.push((k.clone(), built));
            return false;
        }
        true
    });
    let mut budget = MAX_UPLOAD_VERTS_PER_FRAME;
    let mut first = true;
    while let Some((_, built)) = cache.uploading.first() {
        let verts: usize = built.parts.iter().flatten().map(|p| p.mesh.count_vertices()).sum();
        if !first && verts > budget {
            break;
        }
        first = false;
        budget = budget.saturating_sub(verts);
        let (k, built) = cache.uploading.remove(0);
        let parts = cache.adopt(built, &mut meshes);
        cache.models.insert(k, Arc::new(parts));
    }
    stats.pending_mesh_tasks = cache.pending.len() + cache.uploading.len();
    stats.cached_models = cache.models.len();
}

fn spawn_parts(
    mut commands: Commands,
    cache: Res<ConstructMeshCache>,
    mats: Res<FxMaterials>,
    mut q: Query<(Entity, &FxSlot, &mut ConstructRuntime, &mut ConstructParts)>,
) {
    for (e, slot, mut rt, mut cp) in &mut q {
        if rt.spawned {
            continue;
        }
        let Some(parts) = cache.get(&rt.key) else {
            continue;
        };
        let mat = mats.construct(rt.translucent);
        rt.parts.clear();
        cp.parts.clear();
        for (i, p) in parts.iter().enumerate() {
            let Some(p) = p else { continue };
            let pe = commands
                .spawn((
                    Mesh3d(p.mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    MeshTag(slot.0),
                    Transform::from_translation(p.offset),
                    NotShadowCaster,
                    ChildOf(e),
                ))
                .id();
            rt.parts.push((pe, i, p.offset));
            cp.parts.push((i, pe));
        }
        rt.spawned = true;
    }
}

fn apply_translucency(
    settings: Res<FxSettings>,
    mats: Res<FxMaterials>,
    mut q: Query<&mut ConstructRuntime>,
    mut parts: Query<&mut MeshMaterial3d<crate::ConstructMaterial>>,
) {
    if !settings.is_changed() {
        return;
    }
    let want = settings.translucent;
    for mut rt in &mut q {
        if rt.translucent == want {
            continue;
        }
        rt.translucent = want;
        for (pe, _, _) in &rt.parts {
            if let Ok(mut m) = parts.get_mut(*pe) {
                m.0 = mats.construct(want);
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct EndScratch {
    shatter: ShatterScratch,
}

fn handle_end(
    mut commands: Commands,
    mut fx: ResMut<FxParticles>,
    settings: Res<FxSettings>,
    mut scratch: Local<EndScratch>,
    mut q: Query<(Entity, &ConstructVisual, &ConstructEnd, &GlobalTransform, &mut ConstructRuntime), Added<ConstructEnd>>,
) {
    for (e, v, end, gt, mut rt) in &mut q {
        match end {
            ConstructEnd::Shatter { impulse, point, floor } => {
                let pal = Palette::new(v.corps_color, v.accent);
                let root = gt.affine();
                shatter_model(&mut fx, &settings, &v.model, &root, &rt.pose, &pal, *impulse + rt.vel, *point, *floor, 1.0, 1.0, 1.0, &mut scratch.shatter);
                commands.entity(e).despawn();
            }
            ConstructEnd::Dismiss { duration } => {
                rt.dissolve = Some((0.0, duration.max(0.05)));
            }
            ConstructEnd::Remove => {
                commands.entity(e).despawn();
            }
        }
    }
}

fn advance_constructs(
    mut commands: Commands,
    time: Res<Time>,
    mut auto: Query<(Entity, &mut ConstructVisual, &mut AutoForm)>,
    mut rts: Query<(Entity, &mut ConstructRuntime)>,
) {
    let dt = time.delta_secs();
    for (e, mut v, mut af) in &mut auto {
        af.elapsed += dt;
        let f = (af.elapsed / af.duration.max(1e-3)).min(1.0);
        v.forming = f;
        if f >= 1.0 {
            v.arc = false;
            commands.entity(e).remove::<AutoForm>();
        }
    }
    for (e, mut rt) in &mut rts {
        rt.auto_flash = (rt.auto_flash - dt * 6.0).max(0.0);
        if let Some((el, dur)) = rt.dissolve.as_mut() {
            *el += dt;
            if *el >= *dur + 0.05 {
                commands.entity(e).despawn();
            }
        }
    }
}

fn pose_constructs(q: Query<(&ConstructVisual, &mut ConstructRuntime, Option<&crate::transformers::TransformFx>)>, mut tfs: Query<&mut Transform, Without<ConstructRuntime>>) {
    for (v, mut rt, xf) in q {
        let rt = &mut *rt;
        if rt.pose.len() < v.model.parts.len() {
            rt.pose.resize(v.model.parts.len(), glam::Affine3A::IDENTITY);
        }
        match xf {
            Some(x) if x.rig.alt.parts.len() == v.model.parts.len() => crate::transformers::pose_override(x, &mut rt.pose),
            _ => bd_shapes::pose_layered(&v.model, v.layers.as_slice(), &mut rt.pose),
        }
        for (pe, i, off) in &rt.parts {
            let Some(p) = rt.pose.get(*i) else { continue };
            let m = aff(p) * bevy::math::Affine3A::from_translation(*off);
            if !m.is_finite() {
                continue;
            }
            if let Ok(mut tf) = tfs.get_mut(*pe) {
                let (s, r, t) = m.to_scale_rotation_translation();
                let nt = Transform { translation: t, rotation: r, scale: s };
                if *tf != nt {
                    *tf = nt;
                }
            }
        }
    }
}

/// World-space bounds radius of a model around `origin`.
fn reveal_radius(model: &VoxelModel, root: &bevy::math::Affine3A, origin: Vec3) -> f32 {
    let b = &model.bounds;
    let mut r: f32 = 0.25;
    for i in 0..8 {
        let c = glam::Vec3::new(
            if i & 1 == 0 { b.min.x } else { b.max.x },
            if i & 2 == 0 { b.min.y } else { b.max.y },
            if i & 4 == 0 { b.min.z } else { b.max.z },
        );
        let w = root.transform_point3(v3(c));
        r = r.max(w.distance(origin));
    }
    r
}

fn write_construct_params(
    time: Res<Time>,
    mut up: ResMut<FxUpload>,
    mut stats: ResMut<FxStats>,
    mut q: Query<(&ConstructVisual, &GlobalTransform, &FxSlot, &FxAnchor, &mut ConstructRuntime)>,
) {
    let dt = time.delta_secs().max(1e-4);
    let mut n = 0;
    let mut parts = 0;
    for (v, gt, slot, anchor, mut rt) in &mut q {
        n += 1;
        parts += rt.parts.len();
        let root = gt.affine();
        let pos: Vec3 = root.translation.into();
        let inst_vel = (pos - rt.prev_pos) / dt;
        rt.vel = if inst_vel.is_finite() && inst_vel.length() < 200.0 { rt.vel.lerp(inst_vel, 0.5) } else { Vec3::ZERO };
        rt.prev_pos = pos;
        let df = v.forming - rt.prev_forming;
        rt.form_rate = if df > 0.0 { df / dt } else { rt.form_rate * 0.9 };
        rt.prev_forming = v.forming;

        let pal = Palette::new(v.corps_color, v.accent);
        let origin = v.ring.unwrap_or(pos);
        let radius = reveal_radius(&v.model, &root, origin);
        let (reveal, dissolving) = match rt.dissolve {
            Some((el, dur)) => (v.forming.min(1.0) * (1.0 - (el / dur).min(1.0)), true),
            None => (v.forming, false),
        };
        rt.reveal = reveal;
        let flash = v.flash.max(rt.auto_flash);
        let mut g = construct_gpu(&pal, v.integrity, reveal, flash, v.dim, v.model.voxel_size, v.seed, v.opacity, dissolving, origin, 1.0 / radius);
        g.primary[3] = fin(v.heat, 0.0).clamp(0.0, 1.0);
        write_slot(&mut up, slot.0, g);
        write_anchor(&mut up, anchor.0, AnchorGpu::from_affine(&root, v.ring.unwrap_or(pos)));
    }
    stats.constructs = n;
    stats.part_entities = parts;
}

fn emit_streams(mut fx: ResMut<FxParticles>, settings: Res<FxSettings>, time: Res<Time>, mut q: Query<(&ConstructVisual, &GlobalTransform, &FxAnchor, &mut ConstructRuntime)>) {
    let dt = time.delta_secs();
    let budget = settings.particle_budget.clamp(0.0, 1.0);
    for (v, gt, anchor, mut rt) in &mut q {
        let rt = &mut *rt;
        let Some(ring) = v.ring else { continue };
        let total = (v.model.voxel_count.min(settings.stream_max_per_construct) as f32 * budget).max(8.0);
        let root = gt.affine();
        let scale = root.matrix3.x_axis.length().max(1e-3);
        let size = v.model.voxel_size * scale;
        let pal = Palette::new(v.corps_color, v.accent);
        if let Some((el, dur)) = rt.dissolve {
            // Voxels flow back to the ring.
            if el >= dur {
                continue;
            }
            rt.stream_credit += total * 0.8 * dt / dur;
            let radius = reveal_radius(&v.model, &root, ring);
            while rt.stream_credit >= 1.0 {
                rt.stream_credit -= 1.0;
                let r = fx.rng.next_u32();
                let Some((pi, c)) = random_voxel(&v.model, &rt.cum, r) else { break };
                let local = rt.pose.get(pi).map(|p| p.transform_point3(c)).unwrap_or(c);
                let d = root.transform_point3(v3(local)).distance(ring) / radius;
                let color = pal.slot(fx.rng.below(2));
                let seed = fx.rng.next_u32();
                let jitter = fx.rng.f();
                fx.emit(Emit {
                    kind: PKind::Stream,
                    flags: PFlags::REVERSE,
                    vel: v3(local),
                    size: size * 0.8,
                    color,
                    emissive: 2.2,
                    life: 0.18 + 0.25 * (1.0 - d.min(1.0)) + jitter * 0.1,
                    anchor: anchor.0,
                    seed,
                    ..default()
                });
            }
            continue;
        }
        if v.forming >= 1.0 || rt.form_rate <= 0.0 {
            rt.stream_credit = 0.0;
            continue;
        }
        rt.stream_credit += total * (rt.form_rate * dt).min(1.0);
        let radius = reveal_radius(&v.model, &root, ring);
        let mut guard = 0;
        while rt.stream_credit >= 1.0 && guard < 400 {
            guard += 1;
            rt.stream_credit -= 1.0;
            // Prefer voxels that have not appeared yet (same threshold as the shader, minus hash).
            let mut pick = None;
            for _ in 0..4 {
                let r = fx.rng.next_u32();
                let Some((pi, c)) = random_voxel(&v.model, &rt.cum, r) else { break };
                let local = rt.pose.get(pi).map(|p| p.transform_point3(c)).unwrap_or(c);
                let d = (root.transform_point3(v3(local)).distance(ring) / radius).min(1.0);
                let th = d * 0.78 + 0.11;
                let appear_at = (th + 0.15) / 1.3;
                pick = Some((local, appear_at));
                if appear_at > v.forming {
                    break;
                }
            }
            let Some((local, appear_at)) = pick else { break };
            let life = ((appear_at - v.forming) / rt.form_rate.max(0.05)).clamp(0.07, 0.8);
            let color = pal.slot(if fx.rng.f() < 0.8 { 0 } else { 3 });
            let seed = fx.rng.next_u32();
            fx.emit(Emit {
                kind: PKind::Stream,
                vel: v3(local),
                size: size * 0.85,
                color,
                emissive: 2.4,
                life,
                anchor: anchor.0,
                seed,
                ..default()
            });
        }
    }
}
