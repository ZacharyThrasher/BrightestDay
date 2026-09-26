//! Per-tick player input. This is exactly what a client sends to the host each tick and what the
//! movement code consumes, so client prediction and host simulation see identical data.

use serde::{Deserialize, Serialize};

/// Button bits for [`PlayerInput::buttons`].
pub mod buttons {
    pub const JUMP: u16 = 1 << 0;
    pub const CROUCH: u16 = 1 << 1;
    pub const BOOST: u16 = 1 << 2;
    pub const TETHER_L: u16 = 1 << 3;
    pub const TETHER_R: u16 = 1 << 4;
    pub const REEL: u16 = 1 << 5;
    pub const PRIMARY: u16 = 1 << 6;
    pub const SECONDARY: u16 = 1 << 7;
    /// Ring console or oath is open: movement keys are ignored, momentum/aim/tethers stay live.
    pub const TYPING: u16 = 1 << 8;
    /// Held: charge (power up) — slowly refills willpower; can't act; feet planted. (v1.4)
    pub const CHARGE: u16 = 1 << 9;
    /// Held: aim down sights (a held gun's aim button). Accuracy only (tighter pellet spread);
    /// never read by movement. (v1.4, camera)
    pub const ADS: u16 = 1 << 10;
    /// Press: dodge — a short intangible burst along WASD (backward with no keys). (movement)
    pub const DODGE: u16 = 1 << 11;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerInput {
    /// Monotonic per-client input sequence number (for reconciliation).
    pub seq: u32,
    /// View yaw in radians (0 = looking down -Z, positive = turning left / counter-clockwise from above).
    pub yaw: f32,
    /// View pitch in radians (positive = looking up), clamped to about ±89°.
    pub pitch: f32,
    /// Strafe axis: -1 left, +1 right.
    pub move_x: i8,
    /// Forward axis: +1 forward, -1 back.
    pub move_z: i8,
    pub buttons: u16,
    /// Mouse-wheel reel notches accumulated this tick (positive = reel in).
    pub reel_notches: i8,
}

impl PlayerInput {
    #[inline]
    pub fn held(&self, b: u16) -> bool {
        self.buttons & b != 0
    }

    /// Unit look direction from yaw/pitch.
    pub fn look_dir(&self) -> glam::Vec3 {
        look_dir(self.yaw, self.pitch)
    }
}

/// Unit look direction for a yaw/pitch pair (yaw 0 looks down -Z).
pub fn look_dir(yaw: f32, pitch: f32) -> glam::Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    glam::Vec3::new(-sy * cp, sp, -cy * cp)
}
