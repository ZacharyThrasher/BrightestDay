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
mod build;
pub mod collide;
pub mod ease;
pub mod features;
pub mod forms;
pub mod mesh;
mod parts;
mod rng;
pub mod swing;
pub mod voxelize;

pub use anim::{AnimState, Attack, AttackTiming, Gait, Local, PartKind, Rig, attack_timing, blend_poses, pose, pose_layered, rest_pose};
pub use attach::{SpecBounds, attach_mechanic_parts, chain_spec, portal_ring_spec, spec_bounds, spec_bounds_of_part};
pub use build::quat_from_rot_deg;
pub use collide::{CollisionProxy, Proxy, collision_proxies, collision_proxies_budget};
pub use ease::Ease;
pub use forms::{ALL_FORMS, SCALE_QUANTUM, ShapeKey, build_model, demo_recipe, generate, quantize_scale, suggested_scale, voxel_cap, voxel_size_hint};
pub use mesh::{PartMesh, chain_link_mesh, chain_transform, mesh_model, mesh_part};
pub use swing::{SwingCurve, SwingKey, SwingKind, SwingPhase, swing_curve};
pub use voxelize::{MAX_VOXELS, MAX_VOXELS_KAIJU, PartModel, SEAT_PART, Seat, VoxelModel, WeakKind, WeakPoint, scale_spec, voxel_center_m, voxelize, voxelize_at, voxelize_capped};
