//! Summon-assembly hook: the game (bd_game `present/construct_anim.rs`) keeps this component on
//! every construct entity while its role-ordered assembly plays (see
//! `bd_shapes::behaviours::ConstructAnimator`), so other effects can meet it — e.g. the shout
//! words' letter voxels can stream into the ring beam and become the bricks: target
//! [`ConstructAssembly::next_world`] (the slot of the next brick to land, world space) and arrive
//! within [`ConstructAssembly::next_in`] seconds; everything streams out of
//! [`ConstructAssembly::ring`].

use bevy::prelude::*;

/// Live assembly state of a construct (world space). Removed when the assembly is over.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ConstructAssembly {
    /// 0..1 over the whole assembly.
    pub progress: f32,
    /// Stage landing next / total stages (core, limbs, upper, tools, lock).
    pub stage: u8,
    pub stages: u8,
    /// Beam source: the caster's ring hand (or the construct's own centre when unknown).
    pub ring: Vec3,
    /// World position of the next brick to land (the stage's slot centroid) and seconds until it
    /// lands. `None` once everything has landed.
    pub next_world: Option<Vec3>,
    pub next_in: f32,
    /// Seconds the whole assembly takes for this construct.
    pub total: f32,
    /// Per stage (`..stages`): the slot centroid (world), seconds until its bricks land (≤ 0 =
    /// already landed) and the slot spread radius (m) — so streamed voxels can land *as* the bricks.
    pub slots: [Vec3; 5],
    pub land_in: [f32; 5],
    pub radius: [f32; 5],
    /// Written by effects that want the later stages to wait for them (the shout asks for its
    /// letters to arrive as bricks): seconds to push the not-yet-started stages back. The game
    /// consumes it each frame (`ConstructAnimator::hold_stages`, capped) and resets it to 0.
    pub hold_request: f32,
}

impl ConstructAssembly {
    /// Stages still to land at least `lead` seconds from now: `(stage, slot, seconds, radius)`.
    pub fn pending(&self, lead: f32) -> impl Iterator<Item = (usize, Vec3, f32, f32)> + '_ {
        (0..(self.stages as usize).min(5))
            .filter(move |&k| self.land_in[k] >= lead && self.slots[k].is_finite())
            .map(|k| (k, self.slots[k], self.land_in[k], self.radius[k]))
    }
}
