//! **SLIPSTREAM** — the Hyperline train's wind on players (DESIGN.md "Maps: Hyperline"; the data and
//! the zone maths live in `bd_voxel::slipstream`).
//!
//! Applied once per tick right after movement (after portals / fields), identically by the host
//! (`World::step_movement`) and by client prediction (`World::predict_player`), so prediction never
//! diverges. Rules on top of [`Slipstream::accel`]:
//!
//! * **touching the train** — grounded, riding a surf face, or slamming a wall this tick: no push;
//! * **tethered** — an attached hook scales the push by `tether_scale` (reel in against it: the
//!   tether save is the skill lifeline, never a trap);
//! * everything else gets the full push (tumbling launches included: that's how knock-offs travel).
//!
//! Only players: constructs, projectiles and debris don't feel it (a construct platform built off
//! the side stays put — a creative save, which is the point).

use bd_core::Vec3;
use bd_voxel::Slipstream;

use crate::movement::{MoveEvents, PlayerState};

/// Is the player in contact with something this tick (no wind)?
#[inline]
pub fn touching(st: &PlayerState, ev: &MoveEvents) -> bool {
    st.grounded || ev.wall_hit.is_some() || st.extra.surf_n != Vec3::ZERO
}

/// The acceleration (m/s²) the slipstream puts on this player now (after the contact / tether rules).
pub fn accel(s: &Slipstream, st: &PlayerState, ev: &MoveEvents) -> Vec3 {
    if touching(st, ev) || !st.pos.is_finite() || !st.vel.is_finite() {
        return Vec3::ZERO;
    }
    let a = s.accel(st.pos, st.vel);
    if st.tethered() { a * s.tether_scale } else { a }
}

/// Apply one tick of the wind (no-op without a slipstream — every map but Hyperline).
#[inline]
pub fn apply(s: Option<&Slipstream>, st: &mut PlayerState, ev: &MoveEvents, dt: f32) {
    let Some(s) = s else { return };
    let a = accel(s, st, ev);
    if a != Vec3::ZERO {
        st.vel += a * dt;
    }
}
