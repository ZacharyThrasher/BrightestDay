//! Extension point for the machine kit (`bd_core::mechanics::Feature`: wheels, treads, pistons, gears,
//! rotors, smokestacks…). NOT implemented yet — a follow-up engineer owns this module.
//!
//! Hooks:
//! * [`attach_features`] is called by `forms::generate` after the form generator and the generic
//!   mechanic attach. Add feature assemblies to the spec here (use `crate::build::B::from_spec`, the
//!   helpers in `crate::parts`, and `crate::attach::spec_bounds*` for placement).
//! * [`FeatureRig`] / [`animate_features`] are called by `anim` for every pose: drive feature parts
//!   from construct speed/distance/time. Rigid parts only; identify them by `PartModel::role` +
//!   a convention of your choosing (e.g. a dedicated `PartRole` once bd_core adds one).

use bd_core::mechanics::{Feature, Form};
use bd_core::shape::ShapeSpec;

use crate::anim::Local;
use crate::voxelize::VoxelModel;

/// Add visible feature assemblies to `spec`. Currently a no-op.
pub fn attach_features(_spec: &mut ShapeSpec, _features: &[Feature], _form: Form) {}

/// Inputs available to feature rigs every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FeatureDrive {
    /// Seconds (monotonic, construct lifetime).
    pub time: f32,
    /// Ground speed m/s.
    pub speed: f32,
    /// Metres travelled (wheel/tread phase).
    pub distance: f32,
}

/// Additive local transform for part `index` (identity = untouched). Currently always `None`.
pub fn animate_features(_model: &VoxelModel, _index: usize, _drive: &FeatureDrive) -> Option<Local> {
    None
}
