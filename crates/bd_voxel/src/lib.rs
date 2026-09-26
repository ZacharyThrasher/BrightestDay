//! bd_voxel — the destructible terrain of Brightest Day (see docs/DESIGN.md).
//!
//! - [`VoxelWorld`]: bounded, chunked (32³) `u8`-material world; implements
//!   [`bd_core::voxel::VoxelQuery`] so the sim's collision/tethers/raycasts run against it directly.
//! - [`material`]: the material table (colour, jitter, hardness, debris class, glow).
//! - [`carve`]: deterministic, replicable destruction ops ([`CarveOp`]) and structural collapse
//!   ([`FallingChunk`]).
//! - [`mesh`]: chunk → plain vertex arrays with culled faces, baked AO and colour jitter.
//! - [`arena`]: the seeded Archipelago arena and a flat movement lab.
//! - [`tutorial`]: the movement tutorial course (stations + peak run).
//! - [`city`]: the reusable city kit (towers, deco hotels, streets, palms, bridges, ship, props).
//! - [`maps`]: themed maps built from the kits ([`generate_miami`], [`generate_orbit`]).
//!
//! Typical host/client flow:
//! ```
//! use bd_voxel::*;
//! let (mut world, info) = generate_flat_test(96.0);
//! let op = CarveOp { shape: CarveShape::Sphere { center: info.center, radius: 2.0 }, power: 1.0, seed: 7 };
//! let outcome = world.apply_carve(&op); // identical on every client
//! assert!(!outcome.removed.is_empty());
//! let mut mesher = Mesher::new();
//! let mut mesh = ChunkMesh::default();
//! for chunk in world.drain_dirty() {
//!     mesher.mesh_chunk_into(&world, chunk, &mut mesh); // upload `mesh` for `chunk`
//! }
//! ```

pub mod arena;
pub mod carve;
pub mod interact;
pub mod city;
pub mod material;
pub mod maps;
pub mod mechanics;
pub mod mesh;
pub mod noise;
pub mod slipstream;
pub mod tutorial;
pub mod world;

pub use arena::{ARENA_SIZE_VOXELS, ArenaInfo, GravityProfile, SurfLine, WaterSheet, generate_arena, generate_flat_test};
pub use carve::{CarveOp, CarveOutcome, CarveShape, CollapseLimits, FallingChunk, RemovedVoxel};
pub use mechanics::{MapMechanics, MapMover, MapPortal, MoverPart};
pub use interact::{Interactables, RouletteWheel, SlotMachine};
pub use material::{DebrisClass, MATERIALS, MaterialDef, mat, material_def};
pub use maps::{District, MIAMI_SIZE_VOXELS, MapInfo, ORBIT_SIZE_VOXELS, Route, RouteKind, generate_miami, generate_orbit};
#[cfg(feature = "backrooms")]
pub use maps::{BACKROOMS_SIZE_VOXELS, generate_backrooms};
#[cfg(feature = "casino")]
pub use maps::{CASINO_SIZE_VOXELS, generate_casino};
#[cfg(feature = "giant")]
pub use maps::{GIANT_SIZE_VOXELS, generate_giant};
#[cfg(feature = "lair")]
pub use maps::lair::{LAIR_SIZE_VOXELS, generate_lair};
#[cfg(feature = "mirror")]
pub use maps::{MIRROR_SIZE_VOXELS, generate_mirror};
#[cfg(feature = "sakura")]
pub use maps::{SAKURA_SIZE_VOXELS, generate_sakura};
#[cfg(feature = "valley")]
pub use maps::{VALLEY_SIZE_VOXELS, generate_valley};
pub use slipstream::{SlipZone, Slipstream};
pub use mesh::{ChunkMesh, Mesher, mesh_chunk};
pub use tutorial::{Medals, Station, TutorialInfo, generate_tutorial, medals_from_bot};
pub use world::{CHUNK_SIZE, CHUNK_VOLUME, VoxelWorld, chunk_of, chunk_origin};
