//! **Air jump** (owner: "add a double jump") — one per airtime — plus [`ExtraMoves`], the state it
//! shares with the dodge ([`super::dodge`]) on [`PlayerState::extra`].
//!
//! The rules (all of them):
//! * A *fresh* Jump press while airborne jumps again: vertical speed becomes at least
//!   `air_jump_speed` (× `jump_mult`; a bit under the ground jump) and the horizontal velocity turns
//!   `air_jump_redirect` of the way toward WASD — the same speed, never more, so an air jump can
//!   never feed a bunny-hop.
//! * Bhop stays sacred: holding Jump never air-jumps (only a press does), and a press whose
//!   ballistic path meets walkable ground within the jump buffer is a buffered hop, not an air jump
//!   (as is one within `air_jump_delay` of leaving the ground: the tail of a wheel / double tap).
//! * Not while a hook is attached (the rope owns you: release first — momentum is kept — then
//!   jump), not while tumbling (Smash hitstun: that press is the tech input), not while holding
//!   charge.
//! * Restored on landing, on a tether attach, when hit (`World::apply_hit`: Smash recovery) and on
//!   touching a wall or steep / surf-ramp surface in the air (owner: "emergent, like CS surfing"),
//!   except the same wall again (normal within `wall_refresh_same_wall_lockout`° of the last
//!   refreshing wall) until ground, another wall or a tether attach: chimney kicks and surf lines
//!   work, single-wall climbing doesn't. The air dodge comes back with it. A worn Jump construct
//!   (wing flap, cape dash, parachute, jetpack: `constructs::worn::pre_move`) takes the press instead.
//!
//! * **Extra air jumps** (`MoveModifiers::extra_air_jumps`, powers: Gear Fourth): after the air
//!   jump is spent, that many more presses jump again the same way (same rules, same speed); they
//!   come back with it.
//!
//! Host and client prediction run it inside `step_player` with identical inputs.

use bd_core::Vec3;
use bd_core::input::{PlayerInput, buttons};
use bd_core::voxel::VoxelQuery;
use serde::{Deserialize, Serialize};

use super::controller::Inp;
use super::params::MoveParams;
use super::state::{MoveEvents, PlayerState};
use super::world::{Broadphase, CollisionWorld};

/// [`ExtraMoves::since_dodge`] saturates here (long ago).
pub const SINCE_DODGE_CAP: f32 = 10.0;

/// Air jump + dodge state (part of [`PlayerState`]; replicated to the owner for prediction).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtraMoves {
    /// The air jump was spent this airtime.
    pub air_jump_used: bool,
    /// The air dodge was spent this airtime.
    pub air_dodge_used: bool,
    /// Seconds of dodge burst left (0 = not dodging).
    pub dodge_left: f32,
    /// Seconds until the next dodge.
    pub dodge_cd: f32,
    /// Seconds since the last dodge started (saturates at [`SINCE_DODGE_CAP`]); drives the
    /// intangibility window (`crate::dodge`).
    pub since_dodge: f32,
    /// Unit horizontal dodge direction.
    pub dodge_dir: Vec3,
    /// Horizontal velocity the dodge rides on (and hands back at the end).
    pub dodge_base: Vec3,
    /// Normal of the last wall that refreshed the air jump (zero = none since ground / tether).
    pub last_wall: Vec3,
    /// Normal of the surf face touched last tick (zero = not surfing; `super::surf`).
    pub surf_n: Vec3,
    /// EXTENDS: seconds until the next pole vault (`constructs::extend::pre_move`, predicted).
    pub vault_cd: f32,
    /// Extra air jumps (`MoveModifiers::extra_air_jumps`) spent this airtime.
    pub extra_air_jumps_used: u8,
    /// LEGENDARY: recall flight + heavy-blade leap counters / state (`constructs::legend::pre_move`,
    /// predicted). Default unless the player holds a legendary-kit item.
    pub legend: crate::constructs::legend::LegendMoves,
}

impl Default for ExtraMoves {
    fn default() -> Self {
        Self {
            air_jump_used: false,
            air_dodge_used: false,
            dodge_left: 0.0,
            dodge_cd: 0.0,
            since_dodge: SINCE_DODGE_CAP,
            dodge_dir: Vec3::ZERO,
            dodge_base: Vec3::ZERO,
            last_wall: Vec3::ZERO,
            surf_n: Vec3::ZERO,
            vault_cd: 0.0,
            extra_air_jumps_used: 0,
            legend: crate::constructs::legend::LegendMoves::default(),
        }
    }
}

impl ExtraMoves {
    /// Give back the air jump and the air dodge (landing, tether attach, hit).
    #[inline]
    pub fn restore(&mut self) {
        self.air_jump_used = false;
        self.air_dodge_used = false;
        self.extra_air_jumps_used = 0;
    }

    /// Spend every air jump of this airtime (a worn construct owns Jump in the air: wing flap, cape
    /// dash, parachute, jetpack).
    #[inline]
    pub fn spend_air_jumps(&mut self) {
        self.air_jump_used = true;
        self.extra_air_jumps_used = u8::MAX;
    }

    /// An air jump (the base one or an extra one) is left for this airtime.
    #[inline]
    pub fn air_jump_left(&self, extra: u8) -> bool {
        !self.air_jump_used || self.extra_air_jumps_used < extra
    }

    /// In the dodge burst this tick.
    #[inline]
    pub fn dodging(&self) -> bool {
        self.dodge_left > 0.0
    }

    /// Stop a dodge burst without its exit carry (an external push / launch takes over): `vel` gets
    /// back the horizontal velocity the dodge rode on, so the burst never sticks.
    #[inline]
    pub fn cancel_dodge(&mut self, vel: &mut Vec3) {
        if self.dodging() {
            vel.x = self.dodge_base.x;
            vel.z = self.dodge_base.z;
            self.dodge_left = 0.0;
        }
    }

    /// Was the player intangible `ago` seconds before now (0 = now)? True during the first
    /// `iframes` seconds of a dodge.
    #[inline]
    pub fn intangible_at(&self, ago: f32, iframes: f32) -> bool {
        let age = self.since_dodge - ago.max(0.0);
        age >= 0.0 && age < iframes
    }

    /// Replace non-finite values (hostile snapshots) with defaults.
    pub(crate) fn sanitize(&mut self) {
        let d = Self::default();
        let f = |v: f32, d: f32| if v.is_finite() { v } else { d };
        self.dodge_left = f(self.dodge_left, 0.0).clamp(0.0, 10.0);
        self.dodge_cd = f(self.dodge_cd, 0.0).clamp(0.0, 60.0);
        self.since_dodge = f(self.since_dodge, d.since_dodge).clamp(0.0, SINCE_DODGE_CAP);
        if !self.dodge_dir.is_finite() || !self.dodge_base.is_finite() {
            self.dodge_dir = Vec3::ZERO;
            self.dodge_base = Vec3::ZERO;
            self.dodge_left = 0.0;
        }
        if !self.last_wall.is_finite() {
            self.last_wall = Vec3::ZERO;
        }
        if !self.surf_n.is_finite() {
            self.surf_n = Vec3::ZERO;
        }
    }
}

/// Section 4b of `step_player` (right after the ground jump). `jumped`: the ground / coyote jump
/// already took this tick.
#[allow(clippy::too_many_arguments)]
pub(crate) fn air_jump<V: VoxelQuery + ?Sized>(
    s: &mut PlayerState,
    inp: &Inp,
    input: &PlayerInput,
    world: &CollisionWorld<V>,
    p: &MoveParams,
    jumped: bool,
    ev: &mut MoveEvents,
) {
    if jumped
        || !inp.pressed(buttons::JUMP)
        || s.grounded
        || !s.extra.air_jump_left(s.mods.extra_air_jumps)
        || s.tethered()
        || s.tumble > 0.0
        || s.air_time < p.air_jump_delay
        || crate::charge::holding(input)
    {
        return;
    }
    if landing_soon(s, world, p) {
        return;
    }
    if s.extra.dodging() {
        super::dodge::end(s, p);
    }
    if s.extra.air_jump_used {
        s.extra.extra_air_jumps_used = s.extra.extra_air_jumps_used.saturating_add(1);
    }
    s.extra.air_jump_used = true;
    s.jump_buffer = 0.0;
    let h = Vec3::new(s.vel.x, 0.0, s.vel.z);
    let r = redirect(h, inp.wish_h, p.air_jump_redirect);
    s.vel.x = r.x;
    s.vel.z = r.z;
    s.vel.y = s.vel.y.max(p.air_jump_speed * s.mods.jump_mult);
    ev.air_jumped = true;
}

/// Turn horizontal `h` by `frac` of its angle to `wish` (unit or zero). The result's length is `h`'s
/// (never longer).
pub fn redirect(h: Vec3, wish: Vec3, frac: f32) -> Vec3 {
    let sp = h.length();
    let frac = if frac.is_finite() { frac.clamp(0.0, 1.0) } else { 0.0 };
    if sp < 1e-3 || wish == Vec3::ZERO || frac <= 0.0 {
        return h;
    }
    // Signed angle from h to wish about +Y; rotating h about +Y by it lands on wish.
    let e = h.cross(wish).y.atan2(h.dot(wish));
    let (sn, cs) = (e * frac).sin_cos();
    let v = Vec3::new(h.x * cs + h.z * sn, 0.0, h.z * cs - h.x * sn);
    let l = v.length();
    if l > sp { v * (sp / l) } else { v }
}

/// Will the player touch walkable ground within the jump buffer on the current ballistic path?
/// (Two chords of the parabola; a private cache so the tick's own broadphase is untouched.)
fn landing_soon<V: VoxelQuery + ?Sized>(s: &PlayerState, world: &CollisionWorld<V>, p: &MoveParams) -> bool {
    let t = p.jump_buffer;
    if t.is_nan() || t <= 0.0 {
        return false;
    }
    let g = p.gravity * s.mods.gravity_scale;
    let at = |k: f32| s.vel * k - Vec3::Y * (0.5 * g * k * k);
    let (a, b) = (at(0.5 * t), at(t));
    let hull = s.hull(p);
    let mut bp = Broadphase::new();
    match bp.sweep(world, hull, a) {
        Some(h) => h.normal.y >= p.walkable_normal_y,
        None => bp.sweep(world, hull.translate(a), b - a).is_some_and(|h| h.normal.y >= p.walkable_normal_y),
    }
}

/// End of `step_player`: give back the air jump + air dodge on landing and on a tether attach
/// (which also clear the same-wall lockout), or on touching a wall / steep surface in the air this
/// tick (`wall`: the slide move's non-walkable contact) unless it is the last refreshing wall again.
pub(crate) fn refresh(s: &mut PlayerState, wall: Option<super::state::WallHit>, p: &MoveParams, ev: &mut MoveEvents) {
    let x = &mut s.extra;
    if s.grounded || ev.tether_attached.iter().any(|a| *a) {
        x.restore();
        x.last_wall = Vec3::ZERO;
        return;
    }
    let Some(w) = wall.filter(|_| p.wall_refresh) else { return };
    // Nothing to give back: no refresh (and no lockout), so touching a wall early costs nothing.
    if !(x.air_jump_used || x.air_dodge_used || x.extra_air_jumps_used > 0) || !w.normal.is_finite() {
        return;
    }
    let lock = p.wall_refresh_same_wall_lockout;
    if lock > 0.0 && x.last_wall != Vec3::ZERO && w.normal.dot(x.last_wall) >= lock.to_radians().cos() {
        return;
    }
    x.restore();
    x.last_wall = w.normal;
    ev.wall_refresh = Some(w.normal);
}
