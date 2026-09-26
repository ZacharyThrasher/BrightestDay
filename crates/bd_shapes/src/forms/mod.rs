//! Procedural generators: `Recipe` → `ShapeSpec`, one per `Form`.
//!
//! Scale: `recipe.scale` metres is the form's characteristic size — length for held items / objects /
//! serpents / fish, height for other creatures, span for wings, width for walls & platforms, height
//! for pillars & cages, diameter for domes, balls & trampolines. The generator quantises it to
//! [`SCALE_QUANTUM`] so results can be cached by [`ShapeKey`].
//!
//! `ModelSource::Procedural { seed }` varies proportions/variants deterministically within the form.
//! `ModelSource::Icon(id)` is resolved by bd_lexicon; if you call `generate` with one anyway you get the
//! procedural shape seeded by the id's hash.

mod creature;
mod held;
mod object;
mod structure;
mod worn;

use bd_core::mechanics::{Feature, Form, FormCategory, Mechanic, ModelSource, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::CONSTRUCT_VOXEL_SIZE;

use crate::attach::{attach_mechanic_parts_filtered, spec_bounds_of_part};
use crate::build::B;
use crate::rng::{Rng, hash_str};
use crate::voxelize::{MAX_VOXELS, MAX_VOXELS_KAIJU, VoxelModel, voxelize_capped};

/// Scale is quantised to this many metres before generation (cache-friendly).
pub const SCALE_QUANTUM: f32 = 0.125;

pub(crate) struct G<'a> {
    pub r: Rng,
    pub rec: &'a Recipe,
    /// Characteristic size in voxels.
    pub l: f32,
    pub b: B,
    /// Mechanics whose visible parts the form generator already built (skipped by the generic attach).
    pub handled: Vec<Mechanic>,
    /// Features the form generator already built (skipped by the generic feature placement).
    pub handled_features: Vec<Feature>,
}

impl G<'_> {
    pub fn has(&self, m: Mechanic) -> bool {
        self.rec.mechanics.contains(&m)
    }
    pub fn handle(&mut self, m: Mechanic) {
        if !self.handled.contains(&m) {
            self.handled.push(m);
        }
    }
    /// `has` + mark handled.
    pub fn take(&mut self, m: Mechanic) -> bool {
        let h = self.has(m);
        if h {
            self.handle(m);
        }
        h
    }
    pub fn feat(&self, f: Feature) -> bool {
        self.rec.features.contains(&f)
    }
    pub fn handle_feat(&mut self, f: Feature) {
        if !self.handled_features.contains(&f) {
            self.handled_features.push(f);
        }
    }
    /// `feat` + mark handled.
    pub fn take_feat(&mut self, f: Feature) -> bool {
        let h = self.feat(f);
        if h {
            self.handle_feat(f);
        }
        h
    }
}

pub(crate) fn seed_of(model: &ModelSource) -> u32 {
    match model {
        ModelSource::Procedural { seed } => *seed,
        ModelSource::Icon(id) => hash_str(id),
    }
}

/// Largest characteristic size generated (no-limits colossal constructs); larger scales are clamped.
pub const MAX_SCALE: f32 = 120.0;

pub fn quantize_scale(scale: f32) -> f32 {
    let s = if scale.is_finite() && scale > 0.0 { scale.min(MAX_SCALE) } else { 1.0 };
    ((s / SCALE_QUANTUM).round().max(1.0)) * SCALE_QUANTUM
}

/// Sensible default characteristic size (metres) per form, tuned for readability at 0.25 m voxels.
/// Hard-light constructs are larger than life; below roughly these sizes detail stops reading.
pub fn suggested_scale(form: Form) -> f32 {
    use Form::*;
    match form {
        Hammer => 2.4,
        Sword => 2.4,
        Axe => 2.4,
        Spear => 3.2,
        Shield => 2.2,
        Gauntlet => 1.8,
        Whip => 3.5,
        Gun => 1.6,
        Bow => 2.2,
        Cannon => 2.2,
        Staff => 2.8,
        Disc => 1.5,
        Wings => 3.6,
        Parachute => 3.5,
        Jetpack => 1.2,
        Cape => 1.8,
        Biped => 2.6,
        Kaiju => 14.0,
        Quadruped => 2.2,
        Serpent => 6.0,
        Bird => 1.8,
        Fish => 3.0,
        Arachnid => 1.8,
        Insect => 2.0,
        Blob => 1.6,
        Wall => 6.0,
        Ramp => 5.0,
        Pillar => 6.0,
        Dome => 5.0,
        Cage => 2.6,
        Platform => 4.0,
        Trampoline => 3.0,
        Ball => 1.5,
        Anvil => 1.6,
        Bomb => 1.5,
        Boulder => 2.2,
        Rocket => 2.4,
        Vehicle => 4.5,
        Blob0 => 1.4,
    }
}

/// Every form, in declaration order (handy for galleries/tests).
pub const ALL_FORMS: [Form; 39] = {
    use Form::*;
    [
        Hammer, Sword, Axe, Spear, Shield, Gauntlet, Whip, Gun, Bow, Cannon, Staff, Disc, Wings, Parachute,
        Jetpack, Cape, Biped, Kaiju, Quadruped, Serpent, Bird, Fish, Arachnid, Insect, Blob, Wall, Ramp,
        Pillar, Dome, Cage, Platform, Trampoline, Ball, Anvil, Bomb, Boulder, Rocket, Vehicle, Blob0,
    ]
};

/// Expected voxel edge (m) for a form at a scale: 0.25 m up to ~16 m (32 m for kaiju), then doubling
/// (0.5, 1, 2 m) so colossal constructs stay inside the voxel caps (COMBAT.md §5e).
pub fn voxel_size_hint(form: Form, scale: f32) -> f32 {
    let k = crate::voxelize::voxel_k_for_extent(quantize_scale(scale) / CONSTRUCT_VOXEL_SIZE, voxel_cap(form));
    k as f32 * CONSTRUCT_VOXEL_SIZE
}

/// Voxel cap for a form.
pub fn voxel_cap(form: Form) -> usize {
    if form == Form::Kaiju { MAX_VOXELS_KAIJU } else { MAX_VOXELS }
}

/// Cache key: everything `generate` depends on. (Element, colour, mass, count and payload do not
/// change the shape, so they are deliberately not part of the key.)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapeKey {
    pub form: Form,
    /// Bit `m as u32` set for each mechanic.
    pub mechanics: u64,
    /// Bit `f as u32` set for each feature.
    pub features: u64,
    /// Scale in [`SCALE_QUANTUM`] steps.
    pub scale_q: u32,
    /// Expected voxel multiple (1, 2, 4, 8 = 0.25, 0.5, 1, 2 m) from [`voxel_size_hint`]. The model's
    /// actual `voxel_size` is a deterministic function of this key.
    pub voxel_k: u8,
    pub model: ModelSource,
}

impl ShapeKey {
    pub fn of(recipe: &Recipe) -> Self {
        let mechanics = recipe.mechanics.iter().fold(0u64, |acc, m| acc | (1u64 << (*m as u32 & 63)));
        let features = recipe.features.iter().fold(0u64, |acc, f| acc | (1u64 << (*f as u32 & 63)));
        ShapeKey {
            form: recipe.form,
            mechanics,
            features,
            scale_q: (quantize_scale(recipe.scale) / SCALE_QUANTUM).round() as u32,
            voxel_k: (voxel_size_hint(recipe.form, recipe.scale) / CONSTRUCT_VOXEL_SIZE).round() as u8,
            model: recipe.model.clone(),
        }
    }
}

/// Recipe → shape spec (construct-voxel units, see `bd_core::shape`). Pure and deterministic.
/// Drops the machine-kit sidecar: prefer [`build_model`] (or [`generate_machine`] +
/// [`crate::features::attach_machine`]) so wheels/pistons/rotors get their exact linked motion.
pub fn generate(recipe: &Recipe) -> ShapeSpec {
    generate_machine(recipe).0
}

/// Recipe → (shape spec, machine-kit motion sidecar). Pure and deterministic.
pub fn generate_machine(recipe: &Recipe) -> (ShapeSpec, crate::features::MachineSpec) {
    let scale = quantize_scale(recipe.scale);
    let seed = seed_of(&recipe.model);
    let mut g = G {
        r: Rng::new(seed, recipe.form as u32 + 1),
        rec: recipe,
        l: scale / CONSTRUCT_VOXEL_SIZE,
        b: B::new(),
        handled: Vec::new(),
        handled_features: Vec::new(),
    };
    use Form::*;
    match recipe.form {
        Hammer => held::hammer(&mut g),
        Sword => held::sword(&mut g),
        Axe => held::axe(&mut g),
        Spear => held::spear(&mut g),
        Shield => held::shield(&mut g),
        Gauntlet => held::gauntlet(&mut g),
        Whip => held::whip(&mut g),
        Gun => held::gun(&mut g),
        Bow => held::bow(&mut g),
        Cannon => held::cannon(&mut g),
        Staff => held::staff(&mut g),
        Disc => held::disc(&mut g),
        Wings => worn::wings(&mut g),
        Parachute => worn::parachute(&mut g),
        Jetpack => worn::jetpack(&mut g),
        Cape => worn::cape(&mut g),
        Biped => creature::biped(&mut g),
        Kaiju => creature::kaiju(&mut g),
        Quadruped => creature::quadruped(&mut g),
        Serpent => creature::serpent(&mut g),
        Bird => creature::bird(&mut g),
        Fish => creature::fish(&mut g),
        Arachnid => creature::arachnid(&mut g),
        Insect => creature::insect(&mut g),
        Blob => creature::blob(&mut g),
        Wall => structure::wall(&mut g),
        Ramp => structure::ramp(&mut g),
        Pillar => structure::pillar(&mut g),
        Dome => structure::dome(&mut g),
        Cage => structure::cage(&mut g),
        Platform => structure::platform(&mut g),
        Trampoline => structure::trampoline(&mut g),
        Ball => object::ball(&mut g),
        Anvil => object::anvil(&mut g),
        Bomb => object::bomb(&mut g),
        Boulder => object::boulder(&mut g),
        Rocket => object::rocket(&mut g),
        Vehicle => object::vehicle(&mut g),
        Blob0 => object::blob0(&mut g),
    }
    let G { b, handled, handled_features, .. } = g;
    let mut spec = b.spec;
    let mut machine = b.machine;

    // Generic mechanic parts for everything the form generator did not build itself.
    let rest: Vec<Mechanic> = recipe.mechanics.iter().copied().filter(|m| !handled.contains(m)).collect();
    if !rest.is_empty() {
        let body = spec_bounds_of_part(&spec, 0).unwrap_or_else(|| crate::attach::spec_bounds(&spec));
        attach_mechanic_parts_filtered(&mut spec, &rest, body, recipe.form.category() == FormCategory::Held);
    }

    // Machine-kit feature assemblies (wheels, pistons, rotors...) the form did not build itself.
    crate::features::attach_features(&mut spec, &mut machine, &recipe.features, recipe.form, &handled_features);
    (spec, machine)
}

/// `generate` + `voxelize_capped` with the form's cap + the machine-kit motion rig.
pub fn build_model(recipe: &Recipe) -> (ShapeSpec, VoxelModel) {
    let (spec, machine) = generate_machine(recipe);
    let mut model = voxelize_capped(&spec, voxel_cap(recipe.form));
    crate::features::attach_machine(&mut model, &machine);
    (spec, model)
}

/// Convenience for tests/tools: a procedural recipe for `form` at its suggested scale.
pub fn demo_recipe(form: Form, seed: u32, mechanics: &[Mechanic]) -> Recipe {
    let mut m = mechanics.to_vec();
    m.sort();
    m.dedup();
    let mut r = Recipe::unknown(&format!("{form:?}"));
    r.form = form;
    r.mechanics = m;
    r.scale = suggested_scale(form);
    r.model = ModelSource::Procedural { seed };
    r
}
