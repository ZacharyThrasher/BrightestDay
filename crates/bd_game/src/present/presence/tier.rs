//! PRESENCE TIER: how much gravitas a construct gets. The pure function (and its tests) lives in
//! `bd_sim::presence_tier` so the phrase preview tool (bd_shapes examples) can show it too; this
//! path re-exports it unchanged for the presence presenter, its tuning and the readability survey.

pub use bd_sim::presence_tier::*;
