//! bd_fx — Bevy plugin: hard-light construct rendering + construct VFX. See docs/DESIGN.md (Impact,
//! Performance budget) and docs/COMBAT.md §3 (construct lifecycle).
//!
//! # Integration (bd_game)
//! ```ignore
//! app.add_plugins(bd_fx::BdFxPlugin);
//! // loading phase: pre-compile every pipeline
//! app.add_warmup(|w| for e in bd_fx::spawn_warmup(w) { w.entity_mut(e).insert(WarmupItem); });
//! ```
//! * **Constructs** — spawn an entity with [`ConstructVisual`] + `Transform` (the construct's pivot in
//!   world space). bd_fx meshes the model on a worker thread (cached per [`bd_shapes::ShapeKey`]),
//!   spawns one child per part and poses them every frame from [`ConstructVisual::layers`]. Drive
//!   `forming` (0→1), `integrity`, `flash`, `dim`, `ring`, `arc` from the game each frame (or add
//!   [`AutoForm`]). End it by inserting [`ConstructEnd`] (shatter / dismiss / remove) — bd_fx plays the
//!   effect and despawns the entity.
//! * **One-shots** — write [`ClashFx`], [`BurstFx`], [`ShatterModelFx`], [`ZapFx`], guns
//!   ([`MuzzleFx`], [`CasingFx`], [`BeamFx`], [`ImpactFx`]) and [`SpawnOnomatopoeia`] messages.
//! * **Attached effects** — components [`TrailFx`] (element trails), [`SmokeFx`], [`ForceFieldFx`],
//!   [`ArcFx`], [`ChainFx`], [`PortalFx`], [`ProjectileFx`], [`BarrelSpin`] (+ `ConstructVisual::heat`),
//!   [`TransformFx`] / [`CombineFx`] (Transformers). Machine-kit smokestacks puff automatically.
//! * **Onomatopoeia** — [`OnomatopoeiaSettings`] (Off / BigHitsOnly / FullComic, En / Manga, rate
//!   limit), word table `assets/fx/onomatopoeia.ron` ([`OnomatopoeiaTable`]).
//! * **Quality** — [`FxSettings`] (translucency on/off, particle budget, shatter cube cap);
//!   [`FxStats`] for an F3 overlay.
//!
//! # Rendering
//! * Construct parts: one custom [`ConstructMaterial`] (WGSL `construct.wgsl`) shared by every part
//!   of every construct; per-construct state lives in one storage buffer indexed by the part's
//!   `MeshTag`, so all constructs share one pipeline + bind group (no per-construct materials).
//! * Particles: one GPU ring buffer of analytic particles (motion evaluated in the vertex shader:
//!   ballistic / stream-to-voxel / orbit / rise / flash / static segments). The CPU only writes new
//!   particles; the pool is drawn as a handful of batched instanced-cube entities.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod arcs;
mod assembly;
mod aura;
pub mod beacon; // tracking primitives: lantern beacon pillar, x-ray strokes, tapered strokes
mod construct;
mod effects;
mod extras;
mod font;
mod gpu;
mod guns;
mod material;
mod meshing;
mod onomatopoeia;
pub mod outline; // player outlines: inverted-hull material + params buffer (OutlinePlugin)
pub mod palette;
mod particles;
mod shout;
pub mod text_budget; // world-text budget & placement (comic words, move titles): assets/fx/text_budget.ron
mod transformers;
mod util;
mod warmup;

pub use assembly::ConstructAssembly;
pub use construct::{AnimLayers, AutoForm, ConstructEnd, ConstructMeshCache, ConstructParts, ConstructVisual, CostumeTint, HardLightLook, ModelKey, PartOverlay};
pub use meshing::VoxelColors;
pub use arcs::{ArcFx, ZapFx};
pub use aura::{AuraBurstFx, ChargeAuraFx};
pub use effects::{BurstFx, BurstKind, ClashFx, ShatterModelFx, SmokeFx, TrailFx};
pub use extras::{ChainFx, ForceFieldFx, PortalFx, door_frame_spec};
pub use guns::{BarrelSpin, BeamFx, CasingFx, ImpactFx, MuzzleFx, PortalSplit, ProjectileFx, ProjectileKind};
pub use onomatopoeia::{OnoLanguage, OnoMode, OnoStyle, OnoWord, OnomatopoeiaSettings, OnomatopoeiaTable, SpawnOnomatopoeia};
pub use text_budget::TextBudget;
pub use shout::{SHOUT_MAX_CHARS, Shout, ShoutPhase, ShoutSettings, ShoutSfx, ShoutTable, layout_shout, sanitize_shout, shout_intensity, size_boost, typed_energy, voice_energy};
pub use material::{ConstructMaterial, FxMaterials, ParticleMaterial, PortalMaterial};
pub use particles::{Emit, FxParticles, PKind, PFlags};
pub use transformers::{CombineFx, TransformFx};
pub use warmup::spawn_warmup;

use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// System sets (all in `PostUpdate`). Game code that mutates [`ConstructVisual`] should run in
/// `Update` (or before [`FxSet::Pose`]).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum FxSet {
    /// Spawn/despawn part entities, finish mesh tasks, handle [`ConstructEnd`].
    Lifecycle,
    /// Apply `pose_layered` to part transforms (before transform propagation).
    Pose,
    /// After transform propagation: write GPU params/anchors, emit particles.
    Emit,
}

/// Quality/tuning knobs. Changing `translucent` re-materials every construct part.
#[derive(Resource, Debug, Clone)]
pub struct FxSettings {
    /// Premultiplied-alpha hard light (see-through, inner parts visible). `false` = opaque (fastest,
    /// perfect sorting).
    pub translucent: bool,
    /// 0..1 multiplier on continuous emitters (trails, smoke, fields, forming streams).
    pub particle_budget: f32,
    /// Shatter cube cap per construct; bigger models are clustered 2×2×2 (4×4×4 …) to fit.
    pub shatter_max_cubes: usize,
    /// Upper bound of forming-stream cubes per construct cast.
    pub stream_max_per_construct: usize,
    /// Hard cap on new particles per frame (everything beyond is dropped).
    pub max_spawn_per_frame: usize,
}

impl Default for FxSettings {
    fn default() -> Self {
        Self {
            translucent: true,
            particle_budget: 1.0,
            shatter_max_cubes: 3000,
            stream_max_per_construct: 260,
            max_spawn_per_frame: 12_000,
        }
    }
}

/// Live counters (for an F3 overlay / budget checks).
#[derive(Resource, Debug, Clone, Default)]
pub struct FxStats {
    pub constructs: usize,
    pub part_entities: usize,
    pub pending_mesh_tasks: usize,
    pub cached_models: usize,
    /// Particles spawned this frame / dropped by the per-frame cap.
    pub spawned: usize,
    pub dropped: usize,
    /// Particle batches currently drawn.
    pub visible_batches: usize,
}

pub struct BdFxPlugin;

impl Plugin for BdFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FxSettings>()
            .init_resource::<FxStats>()
            .configure_sets(
                PostUpdate,
                (
                    FxSet::Lifecycle.before(FxSet::Pose),
                    FxSet::Pose.before(TransformSystems::Propagate),
                    FxSet::Emit.after(TransformSystems::Propagate),
                ),
            );
        material::build(app);
        gpu::build(app);
        let mats = material::create(app.world_mut());
        app.insert_resource(mats);
        particles::build(app);
        construct::build(app);
        effects::build(app);
        arcs::build(app);
        aura::build(app);
        extras::build(app);
        guns::build(app);
        text_budget::build(app);
        onomatopoeia::build(app);
        shout::build(app);
        transformers::build(app);
    }

    fn finish(&self, app: &mut App) {
        gpu::finish(app);
    }
}
