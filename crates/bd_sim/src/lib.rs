//! bd_sim — see docs/DESIGN.md.
//!
//! Deterministic 120 Hz simulation. Modules:
//! - [`movement`]: the player character controller (Quake/Source ground+air movement, slide,
//!   AoTTG-style dual tethers, gas boost) as a pure `step_player` function shared by host and client.

pub mod movement;
