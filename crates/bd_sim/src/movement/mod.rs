//! Player movement: a pure, deterministic, allocation-free character controller.
//!
//! * **Quake/Source core** — ground accelerate/friction/stop-speed, air-accelerate with the small air
//!   wish-speed cap (air-strafing gains speed), bunny-hop (jump is checked before friction; a fresh
//!   press inside the jump buffer is a friction-free "perfect hop", holding jump auto-hops with a small
//!   friction tax), coyote time, crouch-jump (legs tuck), crouch-slide with entry burst + cooldown,
//!   Source ramp-slide / ramp launch and surfing on steep construct ramps, horizontal soft cap.
//! * **AoTTG tethers** — two independent hook projectiles (hold = stay attached), inextensible
//!   ratcheting ropes with a gentle pull, reel (button + wheel notches), release keeps all momentum.
//! * **Gas boost** — burst on press + sustained thrust, regenerating meter.
//! * **Hooks for constructs** — [`MoveModifiers`] (in the state) and [`PlayerState::apply_impulse`].
//!
//! Entry point: [`step_player`]. Host and client call it with identical inputs at `bd_core::DT`.
//! Crosshair helper: [`query_anchor`].

mod controller;
mod params;
mod state;
mod tether;
pub mod testkit;
mod world;

#[cfg(test)]
mod tests;

pub use controller::step_player;
pub use params::MoveParams;
pub use state::{Hand, Hook, HookPhase, LandEvent, MoveEvents, MoveModifiers, PlayerState, WallHit};
pub use tether::{AnchorQuery, hand_pos, query_anchor};
pub use world::{AnchorTarget, Collider, ColliderId, CollisionWorld, WorldRayHit, raycast_world};
