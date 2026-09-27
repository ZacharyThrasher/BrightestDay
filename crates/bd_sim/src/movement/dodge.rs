//! **Dodge** (owner: "a quick dodge button") — a short burst along WASD (backward with no keys) on
//! top of the velocity you already had, with a brief intangibility window (`crate::dodge`).
//!
//! * The burst covers `dodge_distance` over `dodge_time` *added to* the pre-dodge horizontal
//!   velocity (held fixed during the burst: no friction, no strafing). Afterwards that velocity is
//!   handed back plus `dodge_exit_speed` along the dodge, capped at max(pre-dodge speed, run speed):
//!   a dodge never raises sustained speed, so it can't be chained into a bhop gain.
//! * One cooldown (`dodge_cooldown`) for every dodge; in the air also once per airtime (restored
//!   with the air jump: landing, tether attach, a wall touch, being hit — a hit also clears the cooldown).
//! * Not while holding charge or typing; a stun drops the button (`stunned_input`). Any jump ends the
//!   burst early (the exit velocity is applied first).
//! * The victim side of hook-and-strike reads [`super::ExtraMoves::dodging`] (`hook::hook_breaks_on_dodge`).

use bd_core::Vec3;
use bd_core::input::{PlayerInput, buttons};

use super::controller::Inp;
use super::params::MoveParams;
use super::state::{MoveEvents, PlayerState};

/// Section 6b of `step_player` (after the gas boost, before the ropes / speed caps): timers, start,
/// burst. `jumped`: a ground or air jump happened this tick.
pub(crate) fn step(s: &mut PlayerState, inp: &Inp, input: &PlayerInput, p: &MoveParams, dt: f32, jumped: bool, ev: &mut MoveEvents) {
    let (air, tumbling) = (!s.grounded, s.in_hitstun()); // LAUNCH FEEL: actionable after hitstun
    let x = &mut s.extra;
    x.since_dodge = (x.since_dodge + dt).min(super::air_jump::SINCE_DODGE_CAP);
    x.dodge_cd = (x.dodge_cd - dt).max(0.0);
    if x.dodging() {
        x.dodge_left -= dt;
        if jumped || x.dodge_left <= 0.5 * dt {
            end(s, p);
        } else {
            burst(s, p);
        }
        return;
    }
    let want = inp.pressed(buttons::DODGE) && !input.held(buttons::TYPING) && !crate::charge::holding(input);
    if !want || x.dodge_cd > 0.0 || (air && x.air_dodge_used) || tumbling || p.dodge_time.is_nan() || p.dodge_time <= 0.0 {
        return;
    }
    let dir = if inp.wish_h != Vec3::ZERO { inp.wish_h } else { -inp.fwd_h };
    x.dodge_dir = dir;
    x.dodge_base = Vec3::new(s.vel.x, 0.0, s.vel.z);
    x.dodge_left = p.dodge_time;
    x.dodge_cd = p.dodge_cooldown;
    x.since_dodge = 0.0;
    if air {
        x.air_dodge_used = true;
    }
    ev.dodged = true;
    burst(s, p);
}

/// Burst velocity: the kept velocity plus the dodge speed along the dodge.
fn burst(s: &mut PlayerState, p: &MoveParams) {
    let x = s.extra;
    let v = x.dodge_base + x.dodge_dir * (p.dodge_distance / p.dodge_time.max(1e-3));
    s.vel.x = v.x;
    s.vel.z = v.z;
}

/// End the burst now: hand back the kept velocity plus the exit carry, never faster than
/// max(pre-dodge speed, run speed).
pub(crate) fn end(s: &mut PlayerState, p: &MoveParams) {
    let x = &mut s.extra;
    x.dodge_left = 0.0;
    let base = x.dodge_base;
    let cap = base.length().max(p.run_speed * s.mods.speed_mult.max(0.0));
    let mut v = base + x.dodge_dir * p.dodge_exit_speed.max(0.0);
    let l = v.length();
    if l > cap && l > 0.0 {
        v *= cap / l;
    }
    s.vel.x = v.x;
    s.vel.z = v.z;
}
