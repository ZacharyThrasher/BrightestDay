//! bd_sim — the deterministic 120 Hz simulation of Brightest Day (see docs/DESIGN.md, COMBAT.md).
//!
//! Headless, allocation-free in steady state, host-authoritative; clients run the same code for
//! prediction. Modules:
//! - [`movement`]: the player character controller (Quake/Source ground+air movement, slide,
//!   AoTTG-style dual tethers, gas boost) as a pure `step_player` function shared by host and client.
//! - [`world`]: the authoritative [`World`] — players, combat, projectiles, constructs, match rules,
//!   one [`World::tick`] per `DT`, commands (`cast`, `dismiss`, oath, seats) between ticks.
//! - [`combat`]: hit data + formulas (knockback, momentum bonus, wall splat, juice).
//! - [`status`]: burn / poison / chill / stun / float / shadow / wet.
//! - [`willpower`]: recipe validation, costs, staleness, upkeep, oath recital.
//! - [`constructs`]: the construct store + interface (**placeholder behaviours** for now).
//! - [`space`]: portals, force fields, links, Shrink/Grow; [`extract`]: animating terrain.
//! - [`projectile`], [`history`] (lag compensation), [`rules`] (lobby rules + match state machine).
//! - [`events`]: everything clients need to hear about; [`replicate`]: compact snapshots + deltas.
//! - [`weapons`]: gun archetypes (DOOM feel) + the generic weapon state machine.
//! - [`bots`]: AI players for tests, practice and stress; [`fuzz`]: random recipes + chaos worlds.
//! - [`settings`]: every lobby knob + presets; [`party`]: Party mode (rounds, picks, effects).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod bots;
pub mod combat;
pub mod constructs;
pub mod events;
pub mod extract;
pub mod fuzz;
pub mod geom;
pub mod history;
pub mod movement;
pub mod party;
pub mod powers;
pub mod projectile;
pub mod replicate;
pub mod rng;
pub mod rules;
pub mod settings;
pub mod space;
pub mod status;
pub mod weapons;
pub mod willpower;
pub mod world;

pub use combat::{HitInfo, HitKind, HitResult, ImpactFx};
pub use events::GameEvent;
pub use replicate::{DeltaDecoder, DeltaEncoder, SnapshotDelta, WorldSnapshot};
pub use rules::{CapMode, MatchPhase, MatchRules, ScaleLimit, WillpowerMode};
pub use world::{Aim, ArenaSource, Player, SimConfig, TargetHint, World};
