//! Every movement tunable. `Default` holds the shipped tuning; the client may hot-reload a RON file
//! (`#[serde(default)]` means a partial file only overrides the fields it names).

use bd_core::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoveParams {
    // ---------------------------------------------------------------- hull
    /// Half the hull width (x and z). Hull is 0.8 m wide.
    pub hull_half_width: f32,
    pub stand_height: f32,
    pub crouch_height: f32,
    /// Eye height above the feet when standing / crouched.
    pub eye_stand: f32,
    pub eye_crouch: f32,
    /// Max ledge climbed without jumping (one terrain voxel + margin).
    pub step_height: f32,
    /// Allow step-up while airborne (lip assist: clipping a ledge edge mid-hop never kills speed).
    pub air_step: bool,
    /// Max lip climbed while airborne (small, so it never replaces crouch-jumping).
    pub air_step_height: f32,
    /// Minimum ground-normal y to count as walkable (0.7 ≈ 45.6°). Steeper surfaces are surfable.
    pub walkable_normal_y: f32,
    /// Moving up faster than this (m/s) *and* faster than `ramp_slide_speed` overall = airborne on
    /// ground contact (Source ramp-slide → ramp launch).
    pub ramp_launch_vy: f32,
    /// Below this speed you always stick to walkable ground (running over crests never hops).
    pub ramp_slide_speed: f32,

    // ---------------------------------------------------------------- gravity & jump
    pub gravity: f32,
    pub jump_speed: f32,
    /// A jump press is remembered this long before landing (and gives a friction-free "perfect hop").
    pub jump_buffer: f32,
    /// Landing grace (s): for this long after touching down, ground friction is scaled by
    /// `land_grace_friction`, so a hop pressed slightly late still keeps nearly all its speed.
    pub land_grace: f32,
    pub land_grace_friction: f32,
    /// Grace period to jump after walking off a ledge.
    pub coyote_time: f32,
    /// Holding jump re-hops automatically on landing.
    pub auto_hop: bool,
    /// Friction ticks applied on an auto (held) hop landing. A fresh press in the buffer window = 0.
    pub autohop_friction: f32,

    // ---------------------------------------------------------------- ground
    pub run_speed: f32,
    pub crouch_speed: f32,
    /// Source `sv_accelerate`.
    pub ground_accel: f32,
    /// Source `sv_friction`.
    pub friction: f32,
    /// Source `sv_stopspeed`.
    pub stop_speed: f32,

    // ---------------------------------------------------------------- air
    /// Source `sv_airaccelerate`.
    pub air_accel: f32,
    /// The classic 30 u/s air wish-speed cap. Small = air-strafing gains speed.
    pub air_wish_cap: f32,
    /// Air control multiplier while tumbling from knockback.
    pub tumble_air_control: f32,
    /// W-strafe assist (0 = off): airborne, holding only forward while turning the view, the velocity
    /// follows the view at up to this fraction of a synced strafer's turn rate and gains this fraction
    /// of the speed a synced A/D strafer gains for the same turn. Strafe-key input is unaffected.
    pub wstrafe_assist: f32,

    // ---------------------------------------------------------------- slide
    /// Crouching on the ground at or above this speed starts a slide.
    pub slide_min_speed: f32,
    pub slide_exit_speed: f32,
    /// Constant slide deceleration (m/s²) on flat ground.
    pub slide_friction: f32,
    /// Weak lateral steering while sliding.
    pub slide_steer_speed: f32,
    pub slide_steer_accel: f32,
    /// Entry burst (m/s) added along travel when a slide starts, if the cooldown is ready…
    pub slide_burst: f32,
    /// …and only if current speed is below this (the burst gets you into slide tier, never stacks).
    pub slide_burst_max_speed: f32,
    pub slide_cooldown: f32,
    /// Fraction of potential energy converted when sliding down/up voxel steps (voxel "slopes").
    pub slide_stair_gain: f32,

    // ---------------------------------------------------------------- speed limits
    /// Horizontal speed above which excess decays (only bites far above skilled speeds).
    pub soft_cap_speed: f32,
    pub soft_cap_linear: f32,
    pub soft_cap_quadratic: f32,
    /// Absolute speed limit (collision is swept, so this is about game feel, not tunnelling).
    pub hard_cap_speed: f32,
    /// Min into-surface speed that reports `MoveEvents::wall_hit`.
    pub wall_hit_min_speed: f32,

    // ---------------------------------------------------------------- tethers
    pub tether_range: f32,
    pub hook_speed: f32,
    pub tether_min_len: f32,
    /// Gentle constant pull toward the anchor while attached (m/s²).
    pub rope_pull: f32,
    /// Pull fades out within this distance of the minimum rope length.
    pub rope_pull_fade: f32,
    /// Each hook's pull is scaled by this while both are attached.
    pub dual_pull_scale: f32,
    /// Extra pull while REEL is held (m/s²)…
    pub reel_pull: f32,
    /// …applied only while closing on the anchor slower than this (a winch, not a rocket).
    pub reel_max_speed: f32,
    /// Per mouse-wheel notch: instant velocity toward the anchor (m/s)…
    pub reel_notch_impulse: f32,
    /// …and rope shortened (m). Negative notches let rope out by this much.
    pub reel_notch_len: f32,
    /// Max speed (m/s) at which a rope shorter than the current distance winds the player in.
    pub rope_retract_speed: f32,
    /// A tether pulling up harder than this fraction of gravity lifts you off the ground.
    pub tether_lift_frac: f32,
    /// Hand offset from the eye: x = sideways (mirrored per hand), y = up, z = forward.
    pub hand_offset: Vec3,
    /// Crosshair aim-assist cone (degrees) used when the centre ray finds no anchor.
    pub aim_assist_deg: f32,
    /// ASSIST HOOK (hook-and-strike): a rope on a *player* pulls along its direction tilted up by
    /// this much (a reel yanks you off the ground in a flat arc instead of dragging you).
    pub hook_player_lift: f32,

    // ---------------------------------------------------------------- gas boost
    pub gas_max: f32,
    /// Sustained thrust (m/s²) along look/wish while BOOST is held.
    pub boost_accel: f32,
    /// Instant burst (m/s) on boost press.
    pub boost_burst: f32,
    pub burst_cost: f32,
    pub burst_cooldown: f32,
    /// Gas per second while boosting.
    pub gas_drain: f32,
    pub gas_regen_ground: f32,
    pub gas_regen_air: f32,
    pub gas_regen_tethered: f32,
    pub gas_regen_delay: f32,
    /// Thrust tapers from full at this speed (along the boost direction)…
    pub boost_taper_start: f32,
    /// …down to `boost_taper_min` at this speed.
    pub boost_taper_end: f32,
    pub boost_taper_min: f32,

    // ---------------------------------------------------------------- modifiers
    /// Glide: fraction of fall speed redirected per second (scaled by `MoveModifiers::glide`).
    pub glide_rate: f32,
    pub glide_efficiency: f32,

    // ---------------------------------------------------------------- air jump & dodge (`air_jump.rs`, `dodge.rs`)
    /// One air jump per airtime: vertical speed is raised to at least this (m/s) × `jump_mult`.
    pub air_jump_speed: f32,
    /// Fraction (0..1) of the angle between the horizontal velocity and WASD that an air jump turns
    /// the velocity through. Speed is never added.
    pub air_jump_redirect: f32,
    /// No air jump this soon (s) after leaving the ground (a wheel / double-tap after a hop).
    pub air_jump_delay: f32,
    /// Dodge: distance (m) covered over `dodge_time` on top of the kept velocity.
    pub dodge_distance: f32,
    pub dodge_time: f32,
    /// Speed (m/s) carried along the dodge afterwards (total never above max(pre-dodge, run speed)).
    pub dodge_exit_speed: f32,
    /// Seconds between dodges (ground or air; an air dodge is also once per airtime).
    pub dodge_cooldown: f32,
    /// Seconds after the dodge starts during which attacks pass through (see `crate::dodge`).
    pub dodge_iframes: f32,
    /// Touching a wall / steep (unwalkable, surfable) surface in the air gives back the air jump
    /// and the air dodge (not the dodge cooldown).
    pub wall_refresh: bool,
    /// Degrees: a wall whose normal is within this of the last refreshing wall's doesn't refresh
    /// again until ground, another wall or a tether attach (no single-wall climbing). 0 = off.
    pub wall_refresh_same_wall_lockout: f32,
}

impl Default for MoveParams {
    fn default() -> Self {
        Self {
            hull_half_width: 0.4,
            stand_height: 1.8,
            crouch_height: 1.1,
            eye_stand: 1.6,
            eye_crouch: 0.9,
            step_height: 0.55,
            air_step: true,
            air_step_height: 0.2,
            walkable_normal_y: 0.7,
            ramp_launch_vy: 3.5,
            ramp_slide_speed: 13.0,

            gravity: 25.0,
            jump_speed: 9.4,
            jump_buffer: 0.2,
            land_grace: 0.05,
            land_grace_friction: 0.15,
            coyote_time: 0.1,
            auto_hop: true,
            autohop_friction: 0.35,

            run_speed: 10.0,
            crouch_speed: 4.5,
            ground_accel: 11.0,
            friction: 6.0,
            stop_speed: 3.0,

            air_accel: 12.0,
            air_wish_cap: 0.8,
            tumble_air_control: 0.25,
            wstrafe_assist: 0.45,

            slide_min_speed: 8.0,
            slide_exit_speed: 4.5,
            slide_friction: 3.0,
            slide_steer_speed: 2.0,
            slide_steer_accel: 10.0,
            slide_burst: 5.0,
            slide_burst_max_speed: 15.0,
            slide_cooldown: 1.25,
            slide_stair_gain: 0.8,

            soft_cap_speed: 40.0,
            soft_cap_linear: 0.8,
            soft_cap_quadratic: 0.04,
            hard_cap_speed: 70.0,
            wall_hit_min_speed: 4.0,

            tether_range: 120.0,
            hook_speed: 260.0,
            tether_min_len: 1.5,
            rope_pull: 10.0,
            rope_pull_fade: 2.0,
            dual_pull_scale: 0.75,
            reel_pull: 45.0,
            reel_max_speed: 20.0,
            reel_notch_impulse: 2.5,
            reel_notch_len: 1.5,
            rope_retract_speed: 25.0,
            tether_lift_frac: 0.5,
            hand_offset: Vec3::new(0.35, -0.25, 0.2),
            aim_assist_deg: 3.0,
            hook_player_lift: 0.5,

            gas_max: 100.0,
            boost_accel: 30.0,
            boost_burst: 5.0,
            burst_cost: 12.0,
            burst_cooldown: 0.3,
            gas_drain: 32.0,
            gas_regen_ground: 35.0,
            gas_regen_air: 8.0,
            gas_regen_tethered: 16.0,
            gas_regen_delay: 0.5,
            boost_taper_start: 32.0,
            boost_taper_end: 55.0,
            boost_taper_min: 0.2,

            glide_rate: 2.5,
            glide_efficiency: 0.85,

            air_jump_speed: 8.0,
            air_jump_redirect: 0.3,
            air_jump_delay: 0.1,
            dodge_distance: 4.0,
            dodge_time: 0.15,
            dodge_exit_speed: 5.0,
            dodge_cooldown: 1.1,
            dodge_iframes: 0.13,
            wall_refresh: true,
            wall_refresh_same_wall_lockout: 30.0,
        }
    }
}

impl MoveParams {
    /// A copy with the body scaled by `s` (Shrink / Grow): hull width and heights, eye heights and
    /// step heights. Speeds and physics are unchanged (the sim scales speed via `MoveModifiers`).
    /// `s = 1.0` yields an identical copy, so unscaled players behave bit-for-bit as before.
    pub fn scaled(&self, s: f32) -> MoveParams {
        let mut p = self.clone();
        if s == 1.0 || !s.is_finite() || s <= 0.0 {
            return p;
        }
        p.hull_half_width *= s;
        p.stand_height *= s;
        p.crouch_height *= s;
        p.eye_stand *= s;
        p.eye_crouch *= s;
        p.step_height *= s.max(1.0);
        p.air_step_height *= s.max(1.0);
        p
    }
}
