//! Fixed 120 Hz simulation tick with render interpolation.
//!
//! Every frame, [`run_sim_ticks`] (in [`FrameSet::Sim`], inside `Update`) adds the real frame time
//! to an accumulator and runs the [`SimTick`] schedule once per whole `DT` (`bd_core::DT`), capped at
//! [`TickClock::max_ticks_per_frame`] (excess time is dropped: no spiral of death after a hitch).
//! Afterwards [`TickClock::alpha`] ∈ [0,1) is how far the *render* time is between the previous and
//! the current tick: render systems draw `prev.lerp(curr, alpha)`.
//!
//! Inside `SimTick`, systems are ordered by [`SimSet`]:
//! `Input` (build this tick's `PlayerInput`) → `Simulate` (bd_sim step) → `Post` (events, net send).
//!
//! Mouse look is **not** on the tick: yaw/pitch are applied every rendered frame (see `input`).

use bd_core::{DT, TICK_HZ};
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimTick;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimSet {
    Input,
    Simulate,
    Post,
}

/// Frame-level ordering inside `Update`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum FrameSet {
    /// Sample devices into `LocalInputState` (keyboard, buttons, wheel).
    Input,
    /// Run 0..N sim ticks.
    Sim,
    /// Everything that reads simulation results for this frame (VFX, juice, debris, HUD).
    Present,
}

#[derive(Resource, Debug, Clone)]
pub struct TickClock {
    /// Number of ticks simulated so far.
    pub tick: u64,
    /// Unsimulated time, seconds (always < DT after `run_sim_ticks`).
    pub accumulator: f64,
    /// Interpolation fraction between the previous and the current tick for rendering.
    pub alpha: f32,
    /// Ticks run during the current frame.
    pub ticks_this_frame: u32,
    /// Spiral-of-death guard: at most this many ticks per frame; extra time is discarded.
    pub max_ticks_per_frame: u32,
    /// Multiplies real time before it is accumulated (hit-stop / slow-mo; 0 freezes the sim).
    pub time_scale: f32,
    /// Stop ticking entirely (e.g. while in the title menu).
    pub paused: bool,
    /// Total time discarded by the spiral-of-death guard (diagnostics).
    pub dropped_secs: f64,
}

impl Default for TickClock {
    fn default() -> Self {
        Self {
            tick: 0,
            accumulator: 0.0,
            alpha: 0.0,
            ticks_this_frame: 0,
            max_ticks_per_frame: 8,
            time_scale: 1.0,
            paused: false,
            dropped_secs: 0.0,
        }
    }
}

impl TickClock {
    pub const DT: f32 = DT;
    pub const HZ: u32 = TICK_HZ;

    /// Advances the accumulator by `real_dt` and returns how many ticks to run.
    pub fn advance(&mut self, real_dt: f64) -> u32 {
        if self.paused {
            self.ticks_this_frame = 0;
            return 0;
        }
        let dt = DT as f64;
        // A frame longer than 250 ms is a hitch (alt-tab, loading); never try to catch that up.
        self.accumulator += real_dt.clamp(0.0, 0.25) * self.time_scale.max(0.0) as f64;
        let mut n = (self.accumulator / dt).floor() as u32;
        if n > self.max_ticks_per_frame {
            let excess = (n - self.max_ticks_per_frame) as f64 * dt;
            self.dropped_secs += excess;
            self.accumulator -= excess;
            n = self.max_ticks_per_frame;
        }
        self.accumulator -= n as f64 * dt;
        self.accumulator = self.accumulator.max(0.0);
        self.alpha = (self.accumulator / dt).clamp(0.0, 1.0) as f32;
        self.ticks_this_frame = n;
        n
    }
}

pub struct TickPlugin;

impl Plugin for TickPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(SimTick)
            .init_resource::<TickClock>()
            .configure_sets(Update, (FrameSet::Input, FrameSet::Sim, FrameSet::Present).chain())
            .configure_sets(SimTick, (SimSet::Input, SimSet::Simulate, SimSet::Post).chain())
            .add_systems(Update, run_sim_ticks.in_set(FrameSet::Sim));
    }
}

/// Exclusive system: runs the `SimTick` schedule N times.
pub fn run_sim_ticks(world: &mut World) {
    let real_dt = world.resource::<Time<Real>>().delta_secs_f64();
    let n = world.resource_mut::<TickClock>().advance(real_dt);
    for _ in 0..n {
        world.run_schedule(SimTick);
        world.resource_mut::<TickClock>().tick += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_and_interpolates() {
        let mut c = TickClock::default();
        assert_eq!(c.advance(0.5 / 120.0), 0);
        assert!((c.alpha - 0.5).abs() < 1e-4);
        assert_eq!(c.advance(1.0 / 120.0), 1);
        assert!((c.alpha - 0.5).abs() < 1e-4);
    }

    #[test]
    fn spiral_of_death_guard() {
        let mut c = TickClock::default();
        let n = c.advance(0.2);
        assert_eq!(n, c.max_ticks_per_frame);
        assert!(c.accumulator < DT as f64);
        assert!(c.dropped_secs > 0.0);
    }

    #[test]
    fn time_scale_freezes() {
        let mut c = TickClock { time_scale: 0.0, ..Default::default() };
        assert_eq!(c.advance(0.1), 0);
    }
}
