//! Every construct as a bd_fx [`ConstructVisual`].
//!
//! * Source of truth: [`WorldViews::constructs`] (interpolated transform + anim state) and the
//!   construct store of `ActiveWorld::world` (recipe / curated shape / extracted model — present in
//!   both authoritative and replica worlds), so late joins and resyncs just work. Events only
//!   trigger moments (forming, flash, shatter, clash, attacks, swings, transforms).
//! * Models are built on worker threads (`bd_shapes::build_model` / `voxelize_capped`), cached by
//!   `ShapeKey` (same phrase = same model; bd_fx caches the meshes by the same key). A construct is
//!   drawn as soon as its model is ready; forming covers the wait.
//! * Anim: creatures/vehicles → `Locomotion { speed, distance }` (distance accumulated here, so
//!   stopped vehicles keep their wheel phase) + an `Attack` overlay from `CreatureAttack`; held items
//!   ride the owner's hand with the `bd_shapes` swing curve from `HeldFx::SwingStart`; worn items sit
//!   on the back; suits wrap the pilot; Transformers pose through bd_fx `TransformFx` synced to
//!   `HeldFx::Transform` (sparks follow `sequence_events`).
//! * The local player's held construct in first person is placed at UI's
//!   `viewmodel::FirstPersonHeld::grip` (UI animates it); own worn items are hidden in first person.
//! * Loops: vehicle engines, minigun spin, beams, rockets in flight (per entity, see `sound`).

use super::camera3p::ViewMode;
use super::impact::HitFreeze;
use super::sound::{Sfx3d, SfxLoopEmitter};
use super::{bv, corps_rgb, PresentSet};
use crate::camera::PlayerCamera;
use crate::juice::CameraShakeEvent;
use crate::sim_bridge::{ActiveWorld, ConstructView, GameEvents, PlayerView, WorldViews};
use crate::state::AppState;
use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::ConstructId;
use bd_fx::{AutoForm, ClashFx, ConstructEnd, ConstructVisual, SpawnOnomatopoeia, TransformFx};
use bd_shapes::{AnimState, Attack, ShapeKey, SwingCurve, SwingKind, TransformRig, VoxelModel};
use bd_sim::constructs::ai::{AttackKind, Clip};
use bd_sim::events::{GoneReason, HeldFx};
use bd_sim::replicate::cflags;
use bd_sim::weapons::{GunArchetype, WeaponPhase};
use bd_sim::GameEvent;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use std::sync::Arc;

/// A built model (+ a Transformer rig when the recipe transforms).
#[derive(Clone)]
pub struct BuiltModel {
    pub model: Arc<VoxelModel>,
    pub key: Option<ShapeKey>,
    pub rig: Option<Arc<TransformRig>>,
    /// Per-voxel colours (terrain animated by a graft).
    pub colors: Option<Arc<bd_fx::VoxelColors>>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum ModelSrc {
    Recipe(ShapeKey),
    /// Curated icon shape (+ alt shape): cached by recipe key + spec content, so the second
    /// "mjolnir" reuses the first one's model and meshes.
    Icon(ShapeKey, u64),
    /// Extracted terrain / slice halves: per construct.
    Construct(ConstructId),
}

fn hash_spec(h: &mut impl std::hash::Hasher, s: &ShapeSpec) {
    use std::hash::Hash;
    s.prims.len().hash(h);
    for p in &s.prims {
        p.kind.hash(h);
        p.tint.hash(h);
        p.part.hash(h);
        p.subtract.hash(h);
        for v in [p.center, p.size, p.rot_deg] {
            v.x.to_bits().hash(h);
            v.y.to_bits().hash(h);
            v.z.to_bits().hash(h);
        }
    }
    for p in &s.parts {
        p.id.hash(h);
        p.parent.hash(h);
        p.role.hash(h);
        p.pivot.x.to_bits().hash(h);
        p.pivot.y.to_bits().hash(h);
        p.pivot.z.to_bits().hash(h);
    }
}

fn icon_hash(shape: &ShapeSpec, alt: Option<&ShapeSpec>, name: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    hash_spec(&mut h, shape);
    if let Some(a) = alt {
        hash_spec(&mut h, a);
    }
    // Transformer rigs are seeded by the name.
    name.hash(&mut h);
    h.finish()
}

#[derive(Resource, Default)]
pub struct ModelCache {
    ready: HashMap<ModelSrc, BuiltModel>,
    pending: HashMap<ModelSrc, Task<BuiltModel>>,
}

impl ModelCache {
    pub fn len(&self) -> usize {
        self.ready.len()
    }
}

/// Per-construct presentation state.
#[derive(Component)]
pub struct SimConstruct {
    pub id: ConstructId,
    pub owner: bd_core::PlayerId,
    pub form: Form,
    pub category: FormCategory,
    distance: f32,
    time: f32,
    flash: f32,
    attack: Option<(Attack, f32)>,
    swing: Option<(Arc<SwingCurve>, f32)>,
    robot: bool,
    last_pos: Vec3,
    loop_kind: Option<&'static str>,
}

/// ConstructId → entity.
#[derive(Resource, Default)]
pub struct ConstructEntities(pub HashMap<ConstructId, Entity>);

/// Swing curves by (form, kind).
#[derive(Resource, Default)]
struct SwingCache(HashMap<(Form, u8), Arc<SwingCurve>>);

pub struct ConstructsPlugin;

impl Plugin for ConstructsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModelCache>()
            .init_resource::<ConstructEntities>()
            .init_resource::<SwingCache>()
            .init_resource::<Provisionals>()
            .add_systems(OnExit(AppState::InGame), clear_constructs)
            .add_systems(Update, (predicted_casts, construct_events, sync_constructs).chain().in_set(PresentSet::Sync))
            .add_systems(Update, prewarm_models.after(sync_constructs));
    }
}

fn clear_constructs(mut commands: Commands, mut map: ResMut<ConstructEntities>) {
    for (_, e) in map.0.drain() {
        commands.entity(e).try_despawn();
    }
}

fn model_src(c: &bd_sim::constructs::Construct) -> ModelSrc {
    if c.model.is_some() || c.slice.is_some() {
        ModelSrc::Construct(c.id)
    } else if let Some(s) = c.shape.as_deref() {
        ModelSrc::Icon(ShapeKey::of(&c.recipe), icon_hash(s, c.alt_shape.as_deref(), &c.recipe.name))
    } else {
        ModelSrc::Recipe(ShapeKey::of(&c.recipe))
    }
}

fn start_build(cache: &mut ModelCache, src: ModelSrc, c: &bd_sim::constructs::Construct) {
    if cache.ready.contains_key(&src) || cache.pending.contains_key(&src) {
        return;
    }
    let task = AsyncComputeTaskPool::get().spawn(build_task(src.clone(), c.recipe.clone(), c.shape.as_deref().cloned(), c.alt_shape.as_deref().cloned(), c.model.clone()));
    cache.pending.insert(src, task);
}

/// The model build for `src` (worker thread).
fn build_task(src: ModelSrc, recipe: Recipe, shape: Option<ShapeSpec>, alt: Option<ShapeSpec>, terrain: Option<Arc<bd_sim::constructs::VoxelModel>>) -> impl std::future::Future<Output = BuiltModel> + Send + 'static {
    let key = match &src {
        ModelSrc::Recipe(k) => Some(k.clone()),
        ModelSrc::Icon(..) | ModelSrc::Construct(_) => None,
    };
    async move {
        // Terrain lifted by a graft: its own voxels (materials tinted with hard light) + the
        // phrase's hard-light limbs.
        if let Some(t) = terrain {
            let (m, colors) = super::animated_terrain::terrain_construct_model(&recipe, &t);
            return BuiltModel { model: Arc::new(m), key: None, rig: None, colors: Some(Arc::new(colors)) };
        }
        let transforms = recipe.has(Mechanic::Transforms);
        let rig = match (&shape, &alt) {
            (Some(s), Some(a)) if transforms => Some(Arc::new(bd_shapes::transform_rig_icon(&recipe, s, a))),
            _ => None,
        };
        let rig = rig.or_else(|| {
            transforms.then(|| {
                let m = match &shape {
                    Some(s) => bd_shapes::build_icon_model(&recipe, s).1,
                    None => bd_shapes::build_model(&recipe).1,
                };
                Arc::new(bd_shapes::transform_rig(&m, bd_sim::rng::word_hash(&recipe.name) as u32))
            })
        });
        let model = match (&rig, &shape) {
            (Some(r), _) => Arc::new(r.alt.clone()),
            (None, Some(s)) => Arc::new(bd_shapes::build_icon_model(&recipe, s).1),
            (None, None) => Arc::new(bd_shapes::build_model(&recipe).1),
        };
        BuiltModel { model, key: if rig.is_some() { None } else { key }, rig, colors: None }
    }
}

// ------------------------------------------------------------------------------ match-start prewarm

/// DESIGN.md "Loadout slots are pre-built at match start": the bots' vocabulary and the local
/// loadout phrases get their models built (worker threads) and meshed + uploaded (bd_fx, budgeted
/// per frame) before anyone casts them, so a first cast never waits on — or uploads — a fresh model
/// mid-fight. Re-runs when the match rules (scale limits) or the loadout change.
#[derive(Default)]
struct Prewarm {
    rules: Option<bd_sim::MatchRules>,
    loadout: Option<[String; 4]>,
    /// Loadout phrases resolving on a worker (the lexicon is big; never on the frame).
    resolving: Option<Task<Vec<(ModelSrc, Recipe, Option<ShapeSpec>, Option<ShapeSpec>)>>>,
    /// Models to hand to bd_fx once built.
    mesh: Vec<ModelSrc>,
}

fn src_for(recipe: &Recipe, shape: Option<&ShapeSpec>, alt: Option<&ShapeSpec>) -> ModelSrc {
    match shape {
        Some(s) => ModelSrc::Icon(ShapeKey::of(recipe), icon_hash(s, alt, &recipe.name)),
        None => ModelSrc::Recipe(ShapeKey::of(recipe)),
    }
}

fn prewarm_one(cache: &mut ModelCache, st: &mut Prewarm, src: ModelSrc, recipe: Recipe, shape: Option<ShapeSpec>, alt: Option<ShapeSpec>) {
    if !cache.ready.contains_key(&src) && !cache.pending.contains_key(&src) {
        let task = AsyncComputeTaskPool::get().spawn(build_task(src.clone(), recipe, shape, alt, None));
        cache.pending.insert(src.clone(), task);
    }
    if !st.mesh.contains(&src) {
        st.mesh.push(src);
    }
}

fn prewarm_models(
    active: Option<Res<ActiveWorld>>,
    lex: Option<Res<crate::console::RingLexicon>>,
    prefs: Option<Res<crate::lobby::prefs::UiPrefs>>,
    mut cache: ResMut<ModelCache>,
    mut meshes: ResMut<bd_fx::ConstructMeshCache>,
    mut st: Local<Prewarm>,
) {
    let st = &mut *st;
    let rules = active.as_ref().map(|a| a.world.rules().clone()).or_else(|| prefs.as_ref().map(|p| p.game.rules.clone())).unwrap_or_default();
    if st.rules.as_ref() != Some(&rules) {
        let w = bd_sim::bots::word_list();
        for r in w.melee.iter().chain(&w.ranged).chain(&w.summon).chain(&w.thrown).chain(&w.defence).chain(&w.mobility) {
            let r = bd_sim::willpower::validate_recipe(r.clone(), &rules);
            prewarm_one(&mut cache, st, ModelSrc::Recipe(ShapeKey::of(&r)), r, None, None);
        }
        st.rules = Some(rules.clone());
        st.loadout = None;
    }
    if let (Some(lex), Some(prefs)) = (lex.as_ref(), prefs.as_ref())
        && st.resolving.is_none()
        && st.loadout.as_ref() != Some(&prefs.loadout)
    {
        st.loadout = Some(prefs.loadout.clone());
        let (lex, phrases, rules) = (lex.0.clone(), prefs.loadout.clone(), rules.clone());
        st.resolving = Some(AsyncComputeTaskPool::get().spawn(async move {
            phrases
                .iter()
                .filter(|p| !p.trim().is_empty())
                .map(|p| lex.resolve(p.trim()))
                .filter(|res| res.kind != bd_lexicon::ResolutionKind::Graft)
                .map(|res| {
                    let r = bd_sim::willpower::validate_recipe(res.recipe, &rules);
                    (src_for(&r, res.shape.as_ref(), res.alt_shape.as_ref()), r, res.shape, res.alt_shape)
                })
                .collect()
        }));
    }
    if let Some(t) = st.resolving.as_mut()
        && let Some(list) = check_ready(t)
    {
        st.resolving = None;
        for (src, r, shape, alt) in list {
            prewarm_one(&mut cache, st, src, r, shape, alt);
        }
    }
    // Built → bd_fx meshing (the same Arc / key a live construct will use, so it hits the cache).
    if !st.mesh.is_empty() {
        let cache = &*cache;
        st.mesh.retain(|src| match cache.ready.get(src) {
            Some(b) => {
                let key = match &b.key {
                    Some(k) => bd_fx::ModelKey::Shape(k.clone()),
                    None => bd_fx::ModelKey::Ptr(Arc::as_ptr(&b.model) as usize),
                };
                meshes.prewarm(key, &b.model);
                false
            }
            None => cache.pending.contains_key(src),
        });
    }
}

fn attack_of(kind: u8, aim: Vec3) -> Attack {
    match AttackKind::from_u8(kind).clip() {
        Clip::Bite => Attack::Bite,
        Clip::Swipe => Attack::Swipe,
        Clip::Charge => Attack::Charge,
        Clip::Stomp => Attack::Stomp,
        Clip::TailSweep => Attack::TailSweep,
        Clip::Breath => Attack::Breath { aim: super::gv(aim) },
        Clip::Lash => Attack::Lash,
    }
}

#[allow(clippy::too_many_arguments)]
fn construct_events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    map: Res<ConstructEntities>,
    mut swings: ResMut<SwingCache>,
    mut q: Query<(&mut SimConstruct, &mut ConstructVisual, Option<&mut TransformFx>)>,
    mut commands: Commands,
    mut clash: MessageWriter<ClashFx>,
    mut sfx: MessageWriter<Sfx3d>,
    mut ono: MessageWriter<SpawnOnomatopoeia>,
    mut shake: MessageWriter<CameraShakeEvent>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut ended: Local<Vec<ConstructId>>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let eye = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    ended.clear();
    for ev in events.read() {
        match &ev.event {
            GameEvent::ConstructSpawned { owner, pos, .. } => {
                if !super::replayed_locally(&active, *owner) {
                    sfx.write(Sfx3d::at("construct_form", bv(*pos)).gain(0.8));
                }
            }
            GameEvent::ConstructHit { id, point, damage, .. } => {
                if let Some(&e) = map.0.get(id)
                    && let Ok((mut sc, _, _)) = q.get_mut(e)
                {
                    sc.flash = (sc.flash + 0.3 + damage / 40.0).min(1.0);
                }
                let _ = point;
            }
            GameEvent::ConstructShattered { id, impulse, pos } => {
                let p = bv(*pos);
                if let Some(&e) = map.0.get(id) {
                    commands.entity(e).try_insert(ConstructEnd::Shatter { impulse: bv(*impulse), point: Some(p), floor: None });
                    ended.push(*id);
                }
                sfx.write(Sfx3d::at("construct_shatter", p));
                let col = q.iter().find(|(s, _, _)| s.id == *id).map(|(_, v, _)| v.corps_color).unwrap_or([1.0; 3]);
                let mut o = SpawnOnomatopoeia::key("shatter", p, 0.9, col);
                o.event_id = ev.tick ^ id.0 as u64;
                ono.write(o);
            }
            GameEvent::ConstructGone { id, reason, .. } => {
                if let Some(&e) = map.0.get(id) {
                    let end = match reason {
                        GoneReason::Reset => ConstructEnd::Remove,
                        GoneReason::OwnerGone => ConstructEnd::Dismiss { duration: 0.2 },
                        GoneReason::Spent => ConstructEnd::Dismiss { duration: 0.25 },
                        _ => ConstructEnd::Dismiss { duration: 0.45 },
                    };
                    // KO: loops cut off abruptly.
                    commands.entity(e).remove::<SfxLoopEmitter>().try_insert(end);
                    ended.push(*id);
                    if matches!(reason, GoneReason::Dismissed | GoneReason::Expired)
                        && let Ok((_, v, _)) = q.get(e)
                    {
                        let _ = v;
                        if let Some(c) = active.world.constructs.get(*id) {
                            sfx.write(Sfx3d::at("construct_dismiss", bv(c.pos)).gain(0.7));
                        }
                    }
                }
            }
            GameEvent::Clash { a, b, point, strength } => {
                let p = bv(*point);
                let ea = map.0.get(a).copied();
                let eb = map.0.get(b).copied();
                let col = |e: Option<Entity>| e.and_then(|e| q.get(e).ok()).map(|(_, v, _)| v.corps_color).unwrap_or([1.0; 3]);
                let normal = match (ea.and_then(|e| q.get(e).ok()), eb.and_then(|e| q.get(e).ok())) {
                    (Some((sa, _, _)), Some((sb, _, _))) => (sa.last_pos - sb.last_pos).normalize_or(Vec3::Y),
                    _ => Vec3::Y,
                };
                clash.write(ClashFx { point: p, normal, colors: [col(ea), col(eb)], strength: *strength, constructs: [ea, eb] });
                sfx.write(Sfx3d::at("clash", p).gain((0.6 + strength * 0.4).min(1.3)));
                let mut o = SpawnOnomatopoeia::key("clash", p, strength.clamp(0.5, 1.6), col(ea));
                o.event_id = ev.tick ^ (a.0 as u64) << 8;
                ono.write(o);
            }
            GameEvent::CreatureAttack { id, kind, phase, aim, impact } => {
                let Some(&e) = map.0.get(id) else { continue };
                let Ok((mut sc, _, _)) = q.get_mut(e) else { continue };
                use bd_sim::constructs::ai::phase as ph;
                match *phase {
                    ph::TELEGRAPH => {
                        // Model-space aim for breath.
                        let rot = active.world.constructs.get(*id).map(|c| super::bq(c.rot)).unwrap_or(Quat::IDENTITY);
                        let local_aim = rot.inverse() * bv(*aim);
                        sc.attack = Some((attack_of(*kind, local_aim), 0.0));
                        sc.flash = sc.flash.max(0.35);
                        if sc.form == Form::Kaiju && AttackKind::from_u8(*kind) == AttackKind::Breath {
                            sfx.write(Sfx3d::at("kaiju_roar", sc.last_pos));
                        }
                    }
                    ph::IMPACT => {
                        if *impact >= 1.5 {
                            sfx.write(Sfx3d::at("kaiju_stomp", sc.last_pos).gain(1.2));
                            let mut o = SpawnOnomatopoeia::key("kaiju_stomp", sc.last_pos + Vec3::Y, *impact, [1.0; 3]);
                            o.event_id = ev.tick ^ id.0 as u64;
                            ono.write(o);
                            let d = eye.distance(sc.last_pos);
                            let k = (1.0 - d / 60.0).clamp(0.0, 1.0);
                            if k > 0.0 {
                                shake.write(CameraShakeEvent { trauma: 0.5 * k * impact.min(2.0) * 0.5, dir: Vec3::NEG_Y });
                            }
                        } else if *impact > 0.0 {
                            sfx.write(Sfx3d::at("hit_medium", sc.last_pos).gain(0.7));
                        }
                    }
                    ph::END => sc.attack = None,
                    _ => {}
                }
            }
            GameEvent::HeldFx { id, owner, fx, pos, strength, .. } => {
                let p = bv(*pos);
                let local_replay = super::replayed_locally(&active, *owner);
                let Some(&e) = map.0.get(id) else { continue };
                let Ok((mut sc, _, tfx)) = q.get_mut(e) else { continue };
                match fx {
                    HeldFx::SwingStart | HeldFx::Uppercut | HeldFx::Lunge => {
                        if local_replay {
                            continue;
                        }
                        let kind = match fx {
                            HeldFx::Uppercut | HeldFx::Lunge => SwingKind::Secondary,
                            _ if *strength >= 2.0 => SwingKind::Heavy,
                            _ => SwingKind::Primary,
                        };
                        let k = match kind {
                            SwingKind::Heavy => 1,
                            SwingKind::Secondary => 2,
                            _ => 0,
                        };
                        let form = sc.form;
                        let curve = swings.0.entry((form, k)).or_insert_with(|| Arc::new(bd_shapes::swing_curve(form, kind))).clone();
                        sc.swing = Some((curve, 0.0));
                    }
                    HeldFx::SwingImpact | HeldFx::DiveSlam | HeldFx::GroundPound | HeldFx::TransformSlam => {
                        sfx.write(Sfx3d::at(if *strength > 1.5 { "hit_heavy" } else { "hit_medium" }, p));
                    }
                    HeldFx::Parry => {
                        sfx.write(Sfx3d::at("shield_block", p).pitch(1.3));
                        sc.flash = 1.0;
                        if active.local == Some(*owner) {
                            ono.write(SpawnOnomatopoeia::performance("parry", corps_rgb(active.corps_of(*owner))));
                        }
                    }
                    HeldFx::Block => {
                        sfx.write(Sfx3d::at("shield_block", p));
                        sc.flash = sc.flash.max(0.6);
                    }
                    HeldFx::SwordLock => {
                        sfx.write(Sfx3d::at("clash", p));
                        sc.flash = 1.0;
                    }
                    HeldFx::Throw | HeldFx::Catch | HeldFx::Grab | HeldFx::GrabThrow | HeldFx::WhipYank | HeldFx::WhipLatch => {
                        if !local_replay {
                            sfx.write(Sfx3d::at("tether_fire", p).pitch(0.8).gain(0.7));
                        }
                    }
                    HeldFx::Flap | HeldFx::CapeDash | HeldFx::Thrust | HeldFx::Launch => {
                        sfx.write(Sfx3d::at("boost_burst", p).gain(0.6));
                    }
                    HeldFx::ChuteOpen => sfx.write_default_chute(p),
                    HeldFx::Transform => {
                        let to_robot = *strength > 0.0;
                        sc.robot = to_robot;
                        sfx.write(Sfx3d::at(if to_robot { "transform_to_robot" } else { "transform_to_vehicle" }, p));
                        match tfx {
                            Some(mut t) => t.restart(to_robot),
                            None => {
                                commands.entity(e).try_insert(PendingTransform(to_robot));
                            }
                        }
                    }
                    HeldFx::WeakPoint => {
                        sfx.write(Sfx3d::ui("hit_crit"));
                        sc.flash = 1.0;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// A transform requested before the rig was attached (resolved in `sync_constructs`).
#[derive(Component)]
struct PendingTransform(bool);

trait ChuteSfx {
    fn write_default_chute(&mut self, p: Vec3);
}
impl ChuteSfx for MessageWriter<'_, Sfx3d> {
    fn write_default_chute(&mut self, p: Vec3) {
        self.write(Sfx3d::at("construct_form", p).pitch(0.7).gain(0.6));
    }
}

/// Owner hand / back frames from a player view.
pub fn body_frames(v: &PlayerView) -> (Vec3, Quat, Quat) {
    let s = v.size;
    let eye = v.pos + Vec3::Y * 1.6 * s;
    let look = Quat::from_euler(EulerRot::YXZ, v.yaw, v.pitch, 0.0);
    let body = Quat::from_rotation_y(v.yaw);
    (eye, look, body)
}

pub fn hand_transform(v: &PlayerView) -> Transform {
    let (eye, look, _) = body_frames(v);
    let s = v.size;
    let pos = eye + look * (Vec3::new(0.32, -0.32, -0.42) * s);
    Transform::from_translation(pos).with_rotation(look)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_constructs(
    mut commands: Commands,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    freeze: Res<HitFreeze>,
    mode: Res<ViewMode>,
    fp_held: Option<Res<crate::viewmodel::FirstPersonHeld>>,
    mut cache: ResMut<ModelCache>,
    mut map: ResMut<ConstructEntities>,
    mut q: Query<(Entity, &mut SimConstruct, &mut ConstructVisual, &mut Transform, &mut Visibility, Option<&mut SfxLoopEmitter>, Option<&PendingTransform>, Option<&mut Blend>), Without<ConstructEnd>>,
    mut seen: Local<Vec<ConstructId>>,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.1);
    // Finish model builds.
    let mut done = Vec::new();
    for (k, t) in cache.pending.iter_mut() {
        if let Some(m) = check_ready(t) {
            done.push((k.clone(), m));
        }
    }
    for (k, m) in done {
        cache.pending.remove(&k);
        cache.ready.insert(k, m);
    }
    seen.clear();
    let first_person = !mode.third_person();
    for cv in &views.constructs {
        seen.push(cv.id);
        let Some(c) = active.world.constructs.get(cv.id) else { continue };
        let owner_view = views.player(cv.owner);
        let corps = corps_rgb(active.corps_of(cv.owner));
        match map.0.get(&cv.id).copied() {
            None => {
                let src = model_src(c);
                start_build(&mut cache, src.clone(), c);
                let Some(built) = cache.ready.get(&src).cloned() else { continue };
                let mut vis = ConstructVisual::new(built.model.clone(), corps);
                if let Some(k) = built.key.clone() {
                    vis = vis.with_key(k);
                }
                if let Some(cols) = built.colors.clone() {
                    vis = vis.with_colors(cols);
                }
                if let Some(a) = c.recipe.accent {
                    vis = vis.with_accent(Color::srgb(a[0], a[1], a[2]).to_linear().to_f32_array_no_alpha());
                }
                vis.seed = cv.id.0.wrapping_mul(2654435761);
                let tf = place(cv, owner_view, c.category);
                let mut ec = commands.spawn((
                    Name::new(format!("Construct{}", cv.id.0)),
                    SimConstruct {
                        id: cv.id,
                        owner: cv.owner,
                        form: c.recipe.form,
                        category: c.category,
                        distance: 0.0,
                        time: 0.0,
                        flash: 0.0,
                        attack: None,
                        swing: None,
                        robot: false,
                        last_pos: tf.translation,
                        loop_kind: None,
                    },
                    tf,
                ));
                if cv.flags & cflags::FORMING != 0 {
                    let ring = owner_view.map(|v| hand_transform(v).translation).unwrap_or(tf.translation);
                    vis = vis.forming_from(ring);
                    ec.insert(AutoForm::new(c.forming_t.max(0.12)));
                }
                if let Some(rig) = built.rig.clone() {
                    let mut t = TransformFx::new(rig, false);
                    t.t = 1.0;
                    ec.insert(t);
                }
                if let Some(l) = loop_for(c) {
                    ec.insert(SfxLoopEmitter { name: l, gain: 0.0, pitch: 1.0 });
                }
                ec.insert(vis);
                map.0.insert(cv.id, ec.id());
            }
            Some(e) => {
                let Ok((_, mut sc, mut vis, mut tf, mut visibility, looper, pending_tf, blend)) = q.get_mut(e) else { continue };
                if let Some(p) = pending_tf {
                    let to_robot = p.0;
                    commands.entity(e).remove::<PendingTransform>();
                    if let Some(rig) = cache.ready.get(&model_src(c)).and_then(|b| b.rig.clone()) {
                        commands.entity(e).try_insert(TransformFx::new(rig, to_robot));
                    }
                }
                sc.time += dt;
                sc.flash = (sc.flash - dt * 4.0).max(0.0);
                let frozen = freeze.construct(cv.id) || (cv.attached() && freeze.player(cv.owner));
                // Transform.
                let local_owned = active.local == Some(cv.owner);
                let mut hidden = false;
                if cv.attached() && local_owned && first_person {
                    match fp_held.as_deref() {
                        Some(h) if h.active && h.construct == Some(cv.id) => *tf = clamp_first_person(h.grip, &vis.model),
                        _ => hidden = c.recipe.form != Form::Parachute && c.category != FormCategory::Held,
                    }
                } else if !frozen {
                    let mut t = place(cv, owner_view, c.category);
                    if let Some((curve, st)) = sc.swing.as_mut() {
                        *st += dt;
                        let total = curve.anticipation + curve.active + curve.recovery;
                        if *st > total {
                            sc.swing = None;
                        } else {
                            let a = curve.sample(*st);
                            let m = Mat4::from(bevy::math::Affine3A::from_cols_array(&a.to_cols_array()));
                            t = Transform::from_matrix(t.to_matrix() * m);
                        }
                    }
                    // Adopted client prediction: blend from where it formed to the host's truth.
                    if let Some(mut b) = blend {
                        b.t += dt;
                        let k = (b.t / 0.1).clamp(0.0, 1.0);
                        let k = k * k * (3.0 - 2.0 * k);
                        t.translation = b.from.translation.lerp(t.translation, k);
                        t.rotation = b.from.rotation.slerp(t.rotation, k);
                        if k >= 1.0 {
                            commands.entity(e).remove::<Blend>();
                        }
                    }
                    *tf = t;
                }
                let want = if hidden { Visibility::Hidden } else { Visibility::Inherited };
                if *visibility != want {
                    *visibility = want;
                }
                // Anim layers.
                let moved = tf.translation.distance(sc.last_pos);
                if moved < 5.0 {
                    sc.distance += moved;
                }
                sc.last_pos = tf.translation;
                let airborne = !c.grounded;
                let loco = AnimState::Locomotion { speed: cv.speed, distance: sc.distance, airborne };
                let t = sc.time;
                let mut layers = [(loco, t); 2];
                let mut n = 1;
                if let Some((atk, at)) = sc.attack.as_mut() {
                    *at += dt;
                    layers[1] = (AnimState::Attack(*atk), *at);
                    n = 2;
                }
                vis.layers.set(&layers[..n]);
                vis.integrity = cv.integrity;
                vis.flash = sc.flash;
                // Owner out of willpower: the construct dims.
                vis.dim = owner_view.map_or(0.0, |v| if v.willpower < 5.0 { 0.45 } else { 0.0 });
                vis.arc = cv.flags & cflags::FORMING != 0;
                if let Some(mut l) = looper {
                    let (g, p) = loop_level(c.recipe.form, cv);
                    l.gain = g;
                    l.pitch = p;
                }
            }
        }
    }
    // Constructs no longer in the views (gone without an event, e.g. a resync) fade out.
    seen.sort_unstable();
    let stale: Vec<ConstructId> = map.0.keys().copied().filter(|id| seen.binary_search(id).is_err()).collect();
    for id in stale {
        if let Some(e) = map.0.remove(&id)
            && q.get(e).is_ok()
        {
            commands.entity(e).remove::<SfxLoopEmitter>().try_insert(ConstructEnd::Dismiss { duration: 0.3 });
        }
    }
}

/// First-person held items: the farthest a held model may reach from the hand (m, view space).
/// UI's viewmodel draws held items at a fixed fraction of world size; huge ones (a "giant hammer")
/// would still cover the view, so their scale is clamped by this screen-coverage budget.
const FP_MAX_REACH: f32 = 0.36;

fn clamp_first_person(grip: Transform, model: &VoxelModel) -> Transform {
    let b = &model.bounds;
    let reach = (0..8)
        .map(|i| {
            let c = bd_core::Vec3::new(
                if i & 1 == 0 { b.min.x } else { b.max.x },
                if i & 2 == 0 { b.min.y } else { b.max.y },
                if i & 4 == 0 { b.min.z } else { b.max.z },
            );
            c.length()
        })
        .fold(0.0f32, f32::max);
    let s = grip.scale.max_element();
    if !(reach.is_finite() && s.is_finite()) || reach * s <= FP_MAX_REACH || reach <= 0.0 {
        return grip;
    }
    Transform { scale: grip.scale * (FP_MAX_REACH / (reach * s)), ..grip }
}

/// Where a construct sits this frame.
fn place(cv: &ConstructView, owner: Option<&PlayerView>, category: FormCategory) -> Transform {
    if !cv.attached() {
        return Transform::from_translation(cv.pos).with_rotation(cv.rot).with_scale(Vec3::splat(cv.size.max(0.05)));
    }
    let Some(v) = owner else { return Transform::from_translation(cv.pos) };
    let (_, _, body) = body_frames(v);
    let s = v.size;
    if cv.flags & cflags::SUIT != 0 {
        return Transform::from_translation(v.pos).with_rotation(body).with_scale(Vec3::splat(s));
    }
    if category == FormCategory::Worn {
        return Transform::from_translation(v.pos + body * Vec3::new(0.0, 1.25 * s, 0.25 * s)).with_rotation(body);
    }
    hand_transform(v)
}

fn loop_for(c: &bd_sim::constructs::Construct) -> Option<&'static str> {
    let r = &c.recipe;
    if r.form == Form::Vehicle || r.has(Mechanic::Piloted) && c.category == FormCategory::Object {
        Some("vehicle_engine_loop")
    } else if bd_sim::weapons::archetype_for(r) == Some(GunArchetype::Minigun) {
        Some("gun_minigun_loop")
    } else if r.has(Mechanic::Beam) {
        Some("beam_loop")
    } else if r.form == Form::Rocket {
        Some("rocket_flyby_loop")
    } else {
        None
    }
}

fn loop_level(form: Form, cv: &ConstructView) -> (f32, f32) {
    match form {
        Form::Vehicle => (0.35 + (cv.speed / 25.0).min(0.8), 0.8 + (cv.speed / 30.0).min(0.9)),
        Form::Rocket => (((cv.speed - 5.0) / 20.0).clamp(0.0, 1.0), 1.0),
        Form::Gun => {
            let phase = WeaponPhase::from_u8(cv.weapon & 7);
            let level = (cv.weapon >> 3) as f32 / 31.0;
            match phase {
                WeaponPhase::Firing | WeaponPhase::SpinUp | WeaponPhase::SpinDown => (level, 0.6 + 0.5 * level),
                _ => (0.0, 1.0),
            }
        }
        _ => (if cv.anim == 3 { 0.9 } else { 0.0 }, 1.0),
    }
}

// ------------------------------------------------------------------------------ predicted casts

/// Client role: a provisional construct formed on the key press, waiting for the host.
#[derive(Component)]
struct Provisional(u32);

/// Blend an adopted provisional visual to the authoritative transform over ~100 ms.
#[derive(Component)]
struct Blend {
    from: Transform,
    t: f32,
}

#[derive(Resource, Default)]
struct Provisionals {
    /// (seq, recipe, aim, age) waiting for their model.
    pending: Vec<(u32, Recipe, bd_sim::Aim, f32)>,
    spawned: HashMap<u32, Entity>,
}

/// Where the sim will most likely put a fresh cast (approximation of `World::cast` placement).
fn predicted_place(recipe: &Recipe, aim: &bd_sim::Aim, local: Option<&PlayerView>, arena: Option<&crate::arena::ArenaWorld>) -> Transform {
    let origin = bv(aim.origin);
    let dir = bv(aim.dir).normalize_or(Vec3::NEG_Z);
    let yaw = Quat::from_rotation_y((-dir.x).atan2(-dir.z));
    match recipe.form.category() {
        FormCategory::Held | FormCategory::Worn => local.map(hand_transform).unwrap_or(Transform::from_translation(origin + dir)),
        FormCategory::Object => Transform::from_translation(origin + dir * 2.5).with_rotation(yaw),
        _ => {
            let hit = arena.and_then(|a| bd_core::voxel::raycast(&*a.read(), aim.origin, aim.dir, 60.0)).map(|h| bv(h.point));
            Transform::from_translation(hit.unwrap_or(origin + dir * 12.0)).with_rotation(yaw)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn predicted_casts(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut msgs: MessageReader<crate::net::PredictedCast>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    mut cache: ResMut<ModelCache>,
    mut prov: ResMut<Provisionals>,
    mut map: ResMut<ConstructEntities>,
    mut q: Query<(&mut SimConstruct, &Transform), With<Provisional>>,
) {
    use crate::net::PredictedCast as P;
    let Some(active) = active else {
        msgs.clear();
        return;
    };
    let local = active.local.and_then(|id| views.player(id));
    for m in msgs.read() {
        match m {
            P::Started { seq, recipe, aim } => prov.pending.push((*seq, (**recipe).clone(), *aim, 0.0)),
            P::Confirmed { seq, id, grafted } => {
                prov.pending.retain(|p| p.0 != *seq);
                if let Some(e) = prov.spawned.remove(seq) {
                    if *grafted {
                        commands.entity(e).try_insert(ConstructEnd::Dismiss { duration: 0.2 });
                    } else if let Ok((mut sc, tf)) = q.get_mut(e) {
                        sc.id = *id;
                        map.0.insert(*id, e);
                        commands.entity(e).remove::<Provisional>().try_insert(Blend { from: *tf, t: 0.0 });
                    }
                }
            }
            P::Rejected { seq, .. } => {
                prov.pending.retain(|p| p.0 != *seq);
                if let Some(e) = prov.spawned.remove(seq) {
                    commands.entity(e).try_insert(ConstructEnd::Dismiss { duration: 0.3 });
                }
            }
        }
    }
    let dt = time.delta_secs();
    let corps = active.local.map_or([1.0; 3], |l| corps_rgb(active.corps_of(l)));
    let mut i = 0;
    while i < prov.pending.len() {
        prov.pending[i].3 += dt;
        let (seq, recipe, aim, age) = prov.pending[i].clone();
        let src = ModelSrc::Recipe(ShapeKey::of(&recipe));
        if !cache.ready.contains_key(&src) && !cache.pending.contains_key(&src) {
            let r = recipe.clone();
            let key = ShapeKey::of(&r);
            let task = AsyncComputeTaskPool::get().spawn(async move {
                let model = Arc::new(bd_shapes::build_model(&r).1);
                BuiltModel { model, key: Some(key), rig: None, colors: None }
            });
            cache.pending.insert(src.clone(), task);
        }
        if age > 2.5 {
            prov.pending.swap_remove(i);
            continue;
        }
        let Some(built) = cache.ready.get(&src).cloned() else {
            i += 1;
            continue;
        };
        prov.pending.swap_remove(i);
        let tf = predicted_place(&recipe, &aim, local, arena.as_deref());
        let ring = local.map(|v| hand_transform(v).translation).unwrap_or(tf.translation);
        let mut vis = ConstructVisual::new(built.model.clone(), corps).forming_from(ring);
        if let Some(k) = built.key.clone() {
            vis = vis.with_key(k);
        }
        let e = commands
            .spawn((
                Name::new(format!("Provisional{seq}")),
                Provisional(seq),
                SimConstruct {
                    id: ConstructId(u32::MAX - seq),
                    owner: active.local.unwrap_or(bd_core::PlayerId(0)),
                    form: recipe.form,
                    category: recipe.form.category(),
                    distance: 0.0,
                    time: 0.0,
                    flash: 0.0,
                    attack: None,
                    swing: None,
                    robot: false,
                    last_pos: tf.translation,
                    loop_kind: None,
                },
                AutoForm::new(bd_sim::constructs::forming_secs(recipe.form).max(0.15)),
                vis,
                tf,
            ))
            .id();
        prov.spawned.insert(seq, e);
    }
}
