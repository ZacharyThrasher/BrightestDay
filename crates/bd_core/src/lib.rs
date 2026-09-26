//! Shared types for Brightest Day. This crate is the contract between the simulation, the lexicon,
//! the network layer and the game client. Keep it small, dependency-light and stable.

pub mod corps;
pub mod input;
pub mod mechanics;
pub mod voxel;

pub use glam::{IVec3, Quat, Vec2, Vec3};

/// Fixed simulation rate. Movement, tethers, constructs and combat all step at this rate.
pub const TICK_HZ: u32 = 120;
/// Fixed simulation timestep in seconds.
pub const DT: f32 = 1.0 / TICK_HZ as f32;

/// Edge length of a terrain voxel in metres.
pub const VOXEL_SIZE: f32 = 0.5;
/// Edge length of a construct voxel in metres (constructs are finer than terrain).
pub const CONSTRUCT_VOXEL_SIZE: f32 = 0.25;

/// Maximum simultaneous players.
pub const MAX_PLAYERS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PlayerId(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ConstructId(pub u32);
