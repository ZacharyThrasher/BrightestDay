//! bd_shapes — procedural construct shapes, voxeliser, mesher and procedural animation.
//!
//! Pipeline: `Recipe` → [`generate`] → `ShapeSpec` (construct-voxel units) → [`voxelize`] /
//! [`voxelize_capped`] → [`VoxelModel`] (parts of rest-pose voxels + a [`Rig`]) → [`mesh_part`] per part
//! for the client, [`collision_proxies`] for the sim, and every frame [`pose`] (per-part model-space
//! transforms) for both. Held items swing with [`SwingCurve`]s (timed data the sim sweeps hitboxes along).
//! See docs/COMBAT.md; conventions are documented in `bd_core::shape`, [`voxelize`](mod@voxelize) and
//! [`anim`].

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod anim;
pub mod attach;
pub mod behaviours;
mod build;
pub mod collide;
pub mod costume;
pub mod ease;
pub mod features;
pub mod forms;
pub mod mesh;
mod parts;
mod rng;
pub mod roles;
pub mod secondary;
pub mod swing;
pub mod transform;
pub mod voxelize;

pub use anim::{AnimState, Attack, AttackTiming, CAST_TIMING, Footfall, Gait, Local, PartKind, Rig, attack_timing, attack_timing_hw, blend_poses, footfalls, pose, pose_layered, pose_layered_with, rest_pose};
pub use behaviours::{AnimDrive, AnimEvent, Assembly, ConstructAnimator, Cue, CueKind};
pub use roles::{Role, Roles, grip_part, grip_point, support_point};
pub use secondary::SecondaryMotion;
pub use attach::{SpecBounds, attach_mechanic_parts, attach_mount, chain_spec, portal_ring_spec, spec_bounds, spec_bounds_of_part};
pub use build::quat_from_rot_deg;
pub use collide::{CollisionProxy, Proxy, collision_proxies, collision_proxies_budget};
pub use ease::Ease;
pub use forms::{ALL_FORMS, SCALE_QUANTUM, ShapeKey, build_icon_model, FINE_ICON_MAX_SCALE, FINE_ICON_VOXEL, icon_extent, icon_scale_factor, normalize_icon_spec, transform_rig_icon, weight_of, build_model, demo_recipe, generate, generate_machine, quantize_scale, suggested_scale, voxel_cap, voxel_size_hint};
pub use features::{ALL_FEATURES, FeatureDrive, MachineRig, MachineSpec, Motion, Puff, attach_feature_parts, attach_machine, machine_puffs};
pub use costume::{COSTUME_UNIT, CostumeHead, CostumeTweaks, acc, costume, costume_first_person_hands, costume_model, costume_with};
pub use mesh::{PartMesh, chain_link_mesh, chain_transform, mesh_model, mesh_part};
pub use transform::{component_count, COMBINE_SECONDS, CombinerRig, Panel, RobotRole, TRANSFORM_SECONDS, TransformEvent, TransformEventKind, TransformRig, combine_rigs, pose_combine, pose_transform, sequence_events, transform_rig, transform_rig_authored};
pub use swing::{SwingCurve, SwingKey, SwingKind, SwingPhase, swing_curve, swing_curve_weighted};
pub use voxelize::{MAX_VOXELS, MAX_VOXELS_KAIJU, PartInstance, PartModel, SEAT_PART, Seat, VoxelModel, WeakKind, WeakPoint, part_instances, scale_spec, voxel_center_m, voxelize, voxelize_at, voxelize_capped, voxelize_with_unit};
