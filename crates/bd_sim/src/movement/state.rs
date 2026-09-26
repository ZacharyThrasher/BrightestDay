//! Player movement state, modifiers and per-tick events.

use bd_core::Vec3;
use bd_core::voxel::Aabb;
use serde::{Deserialize, Serialize};

use super::params::MoveParams;
use super::world::AnchorTarget;

/// Which tether hand. Index into [`PlayerState::hooks`] with `hand as usize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hand {
    Left = 0,
    Right = 1,
}

impl Hand {
    pub const BOTH: [Hand; 2] = [Hand::Left, Hand::Right];
    pub fn button(self) -> u16 {
        match self {
            Hand::Left => bd_core::input::buttons::TETHER_L,
            Hand::Right => bd_core::input::buttons::TETHER_R,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookPhase {
    #[default]
    Idle,
    /// Projectile in flight (`Hook::pos` is the hook head).
    Flying,
    /// Anchored (`Hook::pos` is the world anchor point, refreshed every tick for moving targets).
    Attached,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hook {
    pub phase: HookPhase,
    /// Hook head while flying, world anchor point while attached.
    pub pos: Vec3,
    /// Flight velocity (flying) / anchor velocity (attached to a moving collider).
    pub vel: Vec3,
    /// Distance flown so far.
    pub travelled: f32,
    /// What the hook is anchored to.
    pub target: Option<AnchorTarget>,
    /// Anchor point in the target collider's local frame (unused for voxels).
    pub local: Vec3,
    /// Current rope length. Only ever shortens by itself (ratchet); wheel-down lets it out.
    pub rope_len: f32,
}

impl Hook {
    #[inline]
    pub fn attached(&self) -> bool {
        self.phase == HookPhase::Attached
    }
}

/// Hooks for constructs (wings, jetpack, parachute, "Fast"…) into base movement. Construct code writes
/// these every tick before `step_player`; they are part of the state so prediction replays them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveModifiers {
    /// Multiplies gravity (jetpack hover ≈ 0.3, 0 = zero-g).
    pub gravity_scale: f32,
    /// 0..1: redirect falling into forward (look) speed — wings / cape glide.
    pub glide: f32,
    /// Thrust (m/s²) toward the 3D wish direction (camera-relative WASD, or look if no keys), capped by
    /// `thrust_max_speed` Source-accelerate style. This is "flight".
    pub thrust: f32,
    pub thrust_max_speed: f32,
    /// If > 0: maximum fall speed (parachute ≈ 3).
    pub slow_fall: f32,
    /// Multiplies run/wish speed.
    pub speed_mult: f32,
    /// Multiplies air acceleration.
    pub air_control: f32,
    /// Multiplies jump speed.
    pub jump_mult: f32,
}

impl Default for MoveModifiers {
    fn default() -> Self {
        Self {
            gravity_scale: 1.0,
            glide: 0.0,
            thrust: 0.0,
            thrust_max_speed: 17.0,
            slow_fall: 0.0,
            speed_mult: 1.0,
            air_control: 1.0,
            jump_mult: 1.0,
        }
    }
}

/// Complete movement state of one player. `Copy`, fixed size, serde — send it in snapshots and restore
/// it for reconciliation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    /// Feet position (bottom centre of the hull).
    pub pos: Vec3,
    pub vel: Vec3,
    pub grounded: bool,
    pub ground_normal: Vec3,
    /// Velocity of the surface we stand on (moving constructs); movement is relative to it.
    pub ground_vel: Vec3,
    pub crouched: bool,
    pub sliding: bool,
    pub slide_cooldown: f32,
    pub jump_buffer: f32,
    /// Seconds since last grounded.
    pub air_time: f32,
    /// A coyote jump is still available (walked off a ledge, haven't jumped).
    pub coyote_ok: bool,
    pub gas: f32,
    pub gas_regen_delay: f32,
    pub boosting: bool,
    pub burst_cooldown: f32,
    /// Remaining tumble seconds (knockback): reduced air control until landing or tethering.
    pub tumble: f32,
    pub prev_buttons: u16,
    pub hooks: [Hook; 2],
    pub mods: MoveModifiers,
}

impl PlayerState {
    pub fn new(pos: Vec3, p: &MoveParams) -> Self {
        Self {
            pos,
            vel: Vec3::ZERO,
            grounded: false,
            ground_normal: Vec3::Y,
            ground_vel: Vec3::ZERO,
            crouched: false,
            sliding: false,
            slide_cooldown: 0.0,
            jump_buffer: 0.0,
            air_time: 0.0,
            coyote_ok: false,
            gas: p.gas_max,
            gas_regen_delay: 0.0,
            boosting: false,
            burst_cooldown: 0.0,
            tumble: 0.0,
            prev_buttons: 0,
            hooks: [Hook::default(); 2],
            mods: MoveModifiers::default(),
        }
    }

    #[inline]
    pub fn height(&self, p: &MoveParams) -> f32 {
        if self.crouched { p.crouch_height } else { p.stand_height }
    }

    /// Collision hull in world space.
    #[inline]
    pub fn hull(&self, p: &MoveParams) -> Aabb {
        hull_at(self.pos, p.hull_half_width, self.height(p))
    }

    /// Camera eye position.
    #[inline]
    pub fn eye(&self, p: &MoveParams) -> Vec3 {
        self.pos + Vec3::Y * if self.crouched { p.eye_crouch } else { p.eye_stand }
    }

    /// Point the ropes pull on.
    #[inline]
    pub fn chest(&self, p: &MoveParams) -> Vec3 {
        self.pos + Vec3::Y * (self.height(p) * 0.6)
    }

    #[inline]
    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    #[inline]
    pub fn horizontal_speed(&self) -> f32 {
        Vec3::new(self.vel.x, 0.0, self.vel.z).length()
    }

    pub fn tethered(&self) -> bool {
        self.hooks.iter().any(|h| h.attached())
    }

    /// External knockback/launch. `tumble_secs > 0` reduces air control until landing or tethering.
    /// Any upward component lifts the player off the ground.
    pub fn apply_impulse(&mut self, dv: Vec3, tumble_secs: f32) {
        if !dv.is_finite() {
            return;
        }
        self.vel += dv;
        if dv.y > 0.0 {
            self.grounded = false;
            self.sliding = false;
            self.coyote_ok = false;
        }
        if tumble_secs > 0.0 {
            self.tumble = self.tumble.max(tumble_secs);
        }
    }

    /// Drop both tethers (e.g. on death / stun). Momentum is kept.
    pub fn release_tethers(&mut self) {
        for h in &mut self.hooks {
            *h = Hook::default();
        }
    }
}

#[inline]
pub(crate) fn hull_at(pos: Vec3, half_w: f32, height: f32) -> Aabb {
    Aabb { min: pos - Vec3::new(half_w, 0.0, half_w), max: pos + Vec3::new(half_w, height, half_w) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LandEvent {
    /// Speed into the ground at touchdown (m/s). Drive camera dip / thud with it.
    pub impact_speed: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WallHit {
    /// Speed into the wall before clipping (m/s).
    pub speed: f32,
    pub normal: Vec3,
}

/// What happened this tick, for sounds / camera juice. Allocation-free.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveEvents {
    pub jumped: bool,
    /// The jump was a fresh press inside the buffer window (a "perfect", friction-free hop).
    pub perfect_hop: bool,
    pub landed: Option<LandEvent>,
    pub slide_start: bool,
    /// The slide started with the entry burst.
    pub slide_burst: bool,
    pub tether_fired: [bool; 2],
    pub tether_attached: [bool; 2],
    /// Released by the player (momentum kept) or the anchor broke.
    pub tether_released: [bool; 2],
    /// The anchor vanished (voxel carved / construct gone).
    pub tether_broken: [bool; 2],
    /// Hook flew its full range without finding anything.
    pub tether_missed: [bool; 2],
    pub wall_hit: Option<WallHit>,
    pub boost_start: bool,
    /// Boost press spent gas on the instant burst.
    pub boost_burst: bool,
    /// Vertical hull displacement from stepping up (+) / snapping down (−) this tick. The camera should
    /// smooth this out (e.g. offset the eye by −stepped and decay the offset over ~0.1 s).
    pub stepped: f32,
}
