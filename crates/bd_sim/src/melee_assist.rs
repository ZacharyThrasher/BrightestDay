//! Melee assist — **no hard lock-on** (owner: "for melee weapons/non guns, should we have a
//! targetting/lock on system? what would be the best way to do that for our game style"; then:
//! "Destiny sword is a great target"). A lock-on would take the camera away and flatten movement
//! skill, so instead a swing has a **lunge**:
//!
//! * When a held **melee** swing *starts* (primary, combo steps, a released heavy — not
//!   secondaries, not guns, not the whip) the sim looks for an enemy inside that weapon's **lunge
//!   range** and a **cone** around the aim, with a clear line of sight (terrain and solid
//!   constructs block it). Hulls are the ones the swinger saw (lag compensation, `view_tick`).
//! * Beyond the weapon's natural reach: a committed, fast **dash** ([`LungeDash`]) at the target's
//!   intercept point that stops just in front of them (so the swing's active window lands on
//!   arrival), on the ground or in the air. It only ever raises your speed toward them to
//!   `dash_speed` (a faster bhop / swing keeps its own speed), ends with a short committed stop
//!   (`exit_speed`), and has a `cooldown`. Range scales with the weapon: a sword reaches ~4.5 m,
//!   long / Heavy / giant weapons dash shorter and slower but hit heavier.
//! * The swing's **hitbox** (never the camera) turns toward the target by at most `aim_max_deg`,
//!   easing in over the windup. A lunge swing that connects gets heavier juice
//!   (`connect_impact`: hit-stop, shake) while the melee momentum bonus keeps using the speed you
//!   had *before* the lunge — the lunge carries you, your movement earns the bonus.
//! * Skill stays: out-moving it (a boost / tether out of the cone, spacing past its range), baiting
//!   the cooldown. Bots swing through the same code, so they get it too.
//!
//! The dash lives in the movement state (`PlayerState::lunge`, replicated in `OwnState`), stepped
//! by [`dash_pre_move`] on the host and in client prediction alike. [`swing_target`] is the exact
//! query the client uses for the in-range weapon glow. Numbers: [`LungeTuning`]
//! (`assets/tuning/combat.ron` → `assist.lunge`); hook-and-strike is [`crate::hook`].

use bd_core::mechanics::{Form, Mechanic};
use bd_core::voxel::Aabb;
use bd_core::{ConstructId, PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::constructs::Construct;
use crate::geom;
use crate::movement::{MoveParams, PlayerState};

/// Both assist systems' numbers (`combat.ron` → `assist`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistTuning {
    /// Melee lunge (dash + hitbox aim nudge + connect juice).
    pub lunge: LungeTuning,
    /// Hook-and-strike: tethers hook players (`crate::hook`).
    pub hook: crate::hook::HookTuning,
}

impl AssistTuning {
    pub fn validated(mut self) -> Self {
        self.lunge = self.lunge.validated();
        self.hook = self.hook.validated();
        self
    }
}

/// How far a form lunges beyond its natural reach (× [`LungeTuning::extra`]). Forms without a row
/// (whip, guns, bows, staffs…) never lunge.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormReach {
    pub form: Form,
    pub mult: f32,
}

/// Lunge numbers. Never shown to players.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LungeTuning {
    /// Master switch for the dash (the aim nudge follows `aim_max_deg`).
    pub enabled: bool,
    /// Natural reach = item length (× Grow/Shrink reach) + this (hull + swing radius), metres.
    pub reach_pad: f32,
    /// Lunge range beyond the natural reach (× form row × weight ÷ length penalty), metres…
    pub extra: f32,
    /// …the total (eye to the target's hull) clamped to this band.
    pub range_min: f32,
    pub range_max: f32,
    /// Items longer than `long_len` (giant weapons) lunge `extra / (1 + (len − long_len) · long_penalty)`
    /// and dash at `dash_speed / (1 + (len − long_len) · long_slow)`.
    pub long_len: f32,
    pub long_penalty: f32,
    pub long_slow: f32,
    /// Half-angle of the acquisition cone around the aim (degrees).
    pub cone_deg: f32,
    /// Dash speed toward the target (m/s): a committed burst (a faster approach keeps its speed).
    pub dash_speed: f32,
    /// Longest dash (s).
    pub dash_max_secs: f32,
    /// Speed toward the target kept when the dash ends (m/s; the committed stop). A faster
    /// approach speed from before the lunge is kept instead.
    pub exit_speed: f32,
    /// Sideways / vertical velocity kept per dash tick (0 = a dead-straight line).
    pub lateral_keep: f32,
    /// The dash stops at natural reach × this from the target's centre, clamped…
    pub stop_frac: f32,
    pub stop_min: f32,
    pub stop_max: f32,
    /// Largest vertical component of the dash direction (air lunges go up / down this much).
    pub max_up: f32,
    /// Seconds between lunges (from the dash start).
    pub cooldown: f32,
    /// Range `extra` × this for Heavy weapons (and dash speed × `heavy_speed`), × `fast_mult` for Fast.
    pub heavy_mult: f32,
    pub heavy_speed: f32,
    pub fast_mult: f32,
    pub forms: Vec<FormReach>,
    /// Lunge swings that connect: juice weight × this (hit-stop, shake, sparks)…
    pub connect_impact: f32,
    /// …Heavy weapons × this on top…
    pub connect_heavy_impact: f32,
    /// …knockback × this.
    pub connect_knockback: f32,
    /// The melee momentum bonus of a lunge swing uses your closing speed from before the dash.
    pub keep_closing_speed: bool,
    /// Hitbox aim nudge: at most this many degrees toward the target…
    pub aim_max_deg: f32,
    /// …eased in over this many seconds from the swing start…
    pub aim_ramp_secs: f32,
    /// …and dropped after this long (or when the swing ends).
    pub aim_hold_secs: f32,
    /// A target that drifted further than this off the aim (degrees) gets no nudge.
    pub aim_release_deg: f32,
}

impl Default for LungeTuning {
    fn default() -> Self {
        Self {
            enabled: true,
            reach_pad: 0.9,
            extra: 2.6,
            range_min: 2.5,
            range_max: 5.5,
            long_len: 1.4,
            long_penalty: 0.6,
            long_slow: 0.25,
            cone_deg: 20.0,
            dash_speed: 26.0,
            dash_max_secs: 0.22,
            exit_speed: 5.0,
            lateral_keep: 0.3,
            stop_frac: 0.7,
            stop_min: 1.2,
            stop_max: 2.2,
            max_up: 0.75,
            cooldown: 0.7,
            heavy_mult: 0.8,
            heavy_speed: 0.8,
            fast_mult: 1.15,
            forms: vec![
                FormReach { form: Form::Sword, mult: 1.0 },
                FormReach { form: Form::Spear, mult: 1.0 },
                FormReach { form: Form::Axe, mult: 0.9 },
                FormReach { form: Form::Hammer, mult: 0.8 },
                FormReach { form: Form::Gauntlet, mult: 0.95 },
                FormReach { form: Form::Shield, mult: 0.7 },
            ],
            connect_impact: 1.4,
            connect_heavy_impact: 1.25,
            connect_knockback: 1.0,
            keep_closing_speed: true,
            aim_max_deg: 12.0,
            aim_ramp_secs: 0.1,
            aim_hold_secs: 0.45,
            aim_release_deg: 45.0,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl LungeTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.reach_pad = fin(self.reach_pad, d.reach_pad, 0.0, 3.0);
        self.extra = fin(self.extra, d.extra, 0.0, 12.0);
        self.range_min = fin(self.range_min, d.range_min, 0.0, 15.0);
        self.range_max = fin(self.range_max, d.range_max, self.range_min, 20.0);
        self.long_len = fin(self.long_len, d.long_len, 0.1, 20.0);
        self.long_penalty = fin(self.long_penalty, d.long_penalty, 0.0, 10.0);
        self.long_slow = fin(self.long_slow, d.long_slow, 0.0, 10.0);
        self.cone_deg = fin(self.cone_deg, d.cone_deg, 0.0, 60.0);
        self.dash_speed = fin(self.dash_speed, d.dash_speed, 0.0, 80.0);
        self.dash_max_secs = fin(self.dash_max_secs, d.dash_max_secs, 0.0, 1.0);
        self.exit_speed = fin(self.exit_speed, d.exit_speed, 0.0, 80.0);
        self.lateral_keep = fin(self.lateral_keep, d.lateral_keep, 0.0, 1.0);
        self.stop_frac = fin(self.stop_frac, d.stop_frac, 0.0, 2.0);
        self.stop_min = fin(self.stop_min, d.stop_min, 0.0, 5.0);
        self.stop_max = fin(self.stop_max, d.stop_max, self.stop_min, 8.0);
        self.max_up = fin(self.max_up, d.max_up, 0.0, 1.0);
        self.cooldown = fin(self.cooldown, d.cooldown, 0.0, 10.0);
        self.heavy_mult = fin(self.heavy_mult, d.heavy_mult, 0.0, 3.0);
        self.heavy_speed = fin(self.heavy_speed, d.heavy_speed, 0.0, 3.0);
        self.fast_mult = fin(self.fast_mult, d.fast_mult, 0.0, 3.0);
        let mut forms: Vec<FormReach> = Vec::with_capacity(self.forms.len());
        for r in &self.forms {
            if !forms.iter().any(|f| f.form == r.form) {
                forms.push(FormReach { form: r.form, mult: fin(r.mult, 0.0, 0.0, 3.0) });
            }
        }
        self.forms = forms;
        self.connect_impact = fin(self.connect_impact, d.connect_impact, 0.0, 5.0);
        self.connect_heavy_impact = fin(self.connect_heavy_impact, d.connect_heavy_impact, 0.0, 5.0);
        self.connect_knockback = fin(self.connect_knockback, d.connect_knockback, 0.0, 5.0);
        self.aim_max_deg = fin(self.aim_max_deg, d.aim_max_deg, 0.0, 30.0);
        self.aim_ramp_secs = fin(self.aim_ramp_secs, d.aim_ramp_secs, 0.0, 2.0);
        self.aim_hold_secs = fin(self.aim_hold_secs, d.aim_hold_secs, 0.0, 3.0);
        self.aim_release_deg = fin(self.aim_release_deg, d.aim_release_deg, 0.0, 90.0);
        self
    }
}

/// A melee weapon's reach this swing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingReach {
    /// Natural reach (m, eye to the target's hull): no dash needed inside it.
    pub reach: f32,
    /// Lunge range (m, eye to the nearest point of the target's hull).
    pub range: f32,
    /// cos of the cone half-angle.
    pub cos_cone: f32,
    /// Dash speed for this weapon (m/s).
    pub dash_speed: f32,
    /// Heavy weapon (bigger connect juice).
    pub heavy: bool,
}

/// Reach of a held melee item (`form`, grip-to-tip length `item_len`, Heavy / Fast, wielder body
/// scale). `None` = this form never lunges.
pub fn swing_reach(t: &LungeTuning, form: Form, item_len: f32, heavy: bool, fast: bool, size: f32) -> Option<SwingReach> {
    let m = t.forms.iter().find(|f| f.form == form)?.mult;
    if m <= 0.0 {
        return None;
    }
    // Same reach scaling as the swing's hitbox (held.rs `item_points`): Grow ×1.4 max, Shrink.
    let s = if size.is_finite() { size } else { 1.0 };
    let k = if s > 1.0 { s.min(1.4) } else { s.max(0.25) };
    let len = if item_len.is_finite() { item_len.max(0.3) } else { 1.0 } * k;
    let reach = len + t.reach_pad;
    let long = (len - t.long_len).max(0.0);
    let weight = if heavy {
        t.heavy_mult
    } else if fast {
        t.fast_mult
    } else {
        1.0
    };
    let extra = t.extra * m * weight / (1.0 + long * t.long_penalty);
    let range = (reach + extra).clamp(t.range_min, t.range_max).max(reach);
    let dash_speed = t.dash_speed * if heavy { t.heavy_speed } else { 1.0 } / (1.0 + long * t.long_slow);
    Some(SwingReach { reach, range, cos_cone: t.cone_deg.to_radians().cos(), dash_speed, heavy })
}

/// [`swing_reach`] for a held construct (guns never lunge).
pub fn reach_of(t: &LungeTuning, c: &Construct, size: f32) -> Option<SwingReach> {
    if crate::constructs::gun::is_gun(c) {
        return None;
    }
    swing_reach(t, c.recipe.form, c.half.z * 2.0, c.has(Mechanic::Heavy), c.has(Mechanic::Fast), size)
}

/// The enemy a swing would chase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingTarget {
    pub id: PlayerId,
    /// Hull centre (the dash / nudge aims here).
    pub point: Vec3,
    /// Eye to the nearest point of the hull (m).
    pub dist: f32,
    /// cos of the angle between the aim and the hull centre.
    pub cos: f32,
}

/// Pick the enemy a swing from `eye` along `look` would chase: inside `r.range` and the cone (or
/// under the crosshair), best-aimed first, then nearest; `clear(eye, point)` = line of sight.
/// Deterministic: ties keep the earlier candidate.
pub fn swing_target(
    eye: Vec3,
    look: Vec3,
    r: &SwingReach,
    candidates: impl IntoIterator<Item = (PlayerId, Aabb)>,
    mut clear: impl FnMut(Vec3, Vec3) -> bool,
) -> Option<SwingTarget> {
    let look = look.normalize_or_zero();
    if look == Vec3::ZERO || !eye.is_finite() {
        return None;
    }
    let mut best: Option<(SwingTarget, f32)> = None;
    for (id, hull) in candidates {
        if !hull.min.is_finite() || !hull.max.is_finite() {
            continue;
        }
        let c = hull.center();
        let to = c - eye;
        let dc = to.length();
        if dc < 1e-4 {
            continue;
        }
        let dist = geom::dist_sq_point_aabb(&hull, eye).sqrt();
        if dist > r.range {
            continue;
        }
        let cos = to.dot(look) / dc;
        let under = geom::segment_aabb(eye, eye + look * r.range, &hull).is_some();
        if cos < r.cos_cone && !under {
            continue;
        }
        let score = (1.0 - cos) * 10.0 + dist / r.range.max(0.1);
        if best.is_some_and(|(_, s)| score >= s) {
            continue;
        }
        if !clear(eye, c) {
            continue;
        }
        best = Some((SwingTarget { id, point: c, dist, cos }, score));
    }
    best.map(|b| b.0)
}

/// A lunge dash in progress (part of the movement state: host and client prediction step it the
/// same way, see [`dash_pre_move`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LungeDash {
    /// Seconds of dash left (0 = none).
    pub t: f32,
    /// Where the dash is headed (the target's intercept point, chest height).
    pub to: Vec3,
    /// Dash speed (m/s).
    pub speed: f32,
    /// Speed toward `to` kept when the dash ends.
    pub exit: f32,
    /// Stop this far from `to` (m).
    pub stop: f32,
    /// Sideways / vertical velocity kept per tick.
    pub lateral: f32,
}

impl LungeDash {
    #[inline]
    pub fn active(&self) -> bool {
        self.t > 0.0
    }
}

/// Plan a dash for a wielder (chest `chest`, velocity `vel`) at a target (hull centre `target`,
/// velocity `target_vel`): aims at the intercept point, stops at arm's length. `None` when already
/// within the stop distance.
pub fn plan_dash(t: &LungeTuning, r: &SwingReach, chest: Vec3, vel: Vec3, target: Vec3, target_vel: Vec3) -> Option<LungeDash> {
    let stop = (r.reach * t.stop_frac).clamp(t.stop_min, t.stop_max);
    let gap = (target - chest).length() - stop;
    if gap.is_nan() || gap <= 0.05 || r.dash_speed <= 0.1 || !vel.is_finite() {
        return None;
    }
    let secs = (gap / r.dash_speed).min(t.dash_max_secs);
    let tv = if target_vel.is_finite() { target_vel } else { Vec3::ZERO };
    let mut to = target + tv * secs;
    // Vertical limit: a steep target is approached at `max_up`, never straight up.
    let d = to - chest;
    let h = Vec3::new(d.x, 0.0, d.z).length();
    let max_rise = t.max_up / (1.0 - t.max_up * t.max_up).max(1e-4).sqrt();
    if h > 1e-3 && d.y.abs() > h * max_rise {
        to.y = chest.y + d.y.signum() * h * max_rise;
    }
    let dir = (to - chest).normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let exit = t.exit_speed.max(vel.dot(dir));
    Some(LungeDash { t: t.dash_max_secs, to, speed: r.dash_speed, exit, stop, lateral: t.lateral_keep })
}

/// Step a dash before movement (host `step_movement` and `World::predict_player`): velocity toward
/// the dash point at dash speed (never slowing a faster approach), the rest damped; arriving (or
/// timing out) caps the approach speed to the exit speed — the committed stop. A launch (tumble)
/// cancels it.
pub fn dash_pre_move(s: &mut PlayerState, p: &MoveParams, dt: f32) {
    let d = s.lunge;
    if !d.active() {
        return;
    }
    if s.tumble > 0.0 || !d.to.is_finite() || !s.vel.is_finite() {
        s.lunge = LungeDash::default();
        return;
    }
    let chest = s.chest(p);
    let off = d.to - chest;
    let dist = off.length();
    let dir = if dist > 1e-4 { off / dist } else { Vec3::ZERO };
    let along = s.vel.dot(dir);
    if dist <= d.stop + 1e-3 || dir == Vec3::ZERO {
        if along > d.exit {
            s.vel -= dir * (along - d.exit);
        }
        s.lunge = LungeDash::default();
        return;
    }
    let lateral = s.vel - dir * along;
    // Never overshoot the stop point within one tick.
    let arrive = ((dist - d.stop) / dt.max(1e-4)).max(0.0);
    let want = along.max(d.speed).min(arrive.max(d.exit));
    s.vel = dir * want + lateral * d.lateral;
    if dir.y > 0.05 {
        s.grounded = false;
    }
    s.lunge.t = d.t - dt;
    if s.lunge.t <= 0.0 {
        // Out of time: the committed stop.
        let along = s.vel.dot(dir);
        if along > d.exit {
            s.vel -= dir * (along - d.exit);
        }
        s.lunge = LungeDash::default();
    }
}

/// Turn `look` toward `target` (seen from `eye`) by at most `max_rad`; targets further than
/// `release_rad` off the aim are ignored (they left the swing's front).
pub fn bias_look(look: Vec3, eye: Vec3, target: Vec3, max_rad: f32, release_rad: f32) -> Vec3 {
    let l = look.normalize_or_zero();
    let to = (target - eye).normalize_or_zero();
    if l == Vec3::ZERO || to == Vec3::ZERO || max_rad.is_nan() || max_rad <= 0.0 {
        return look;
    }
    let ang = l.dot(to).clamp(-1.0, 1.0).acos();
    if ang < 1e-5 || ang > release_rad {
        return look;
    }
    let axis = l.cross(to).normalize_or_zero();
    if axis == Vec3::ZERO {
        return look;
    }
    (Quat::from_axis_angle(axis, ang.min(max_rad)) * l).normalize_or(l)
}

/// Smooth 0→1 ramp (for the aim nudge ease-in).
#[inline]
pub fn ease(x: f32) -> f32 {
    let x = if x.is_finite() { x.clamp(0.0, 1.0) } else { 1.0 };
    x * x * (3.0 - 2.0 * x)
}

/// Per-player assist state (host-side; the client sees the results: the dash in its own movement
/// state, the events, the hits).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AssistState {
    /// Seconds until the next lunge.
    pub lunge_cd: f32,
    /// The swing in progress chases this player (hitbox aim nudge)…
    pub aim_target: Option<PlayerId>,
    /// …since this many seconds…
    pub aim_t: f32,
    /// …with this construct.
    pub swing: Option<ConstructId>,
    /// The swing that lunged (connect juice) and the closing speed from before the dash.
    pub lunge_swing: Option<ConstructId>,
    pub lunge_close: f32,
    pub lunge_heavy: bool,
    /// Hook-and-strike: seconds each hand's rope to a player has been out of sight.
    pub hook_los: [f32; 2],
    /// Arrived on a hooked player with a melee weapon: the next swing started within this many
    /// seconds is a slam.
    pub slam_t: f32,
    /// The swing carrying the slam (bonus knockback on its hits)…
    pub slam_swing: Option<ConstructId>,
    /// …and whether it already connected (the `Slam` event fires once).
    pub slam_hit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hull_at(p: Vec3) -> Aabb {
        Aabb { min: p - Vec3::new(0.4, 0.0, 0.4), max: p + Vec3::new(0.4, 1.8, 0.4) }
    }

    #[test]
    fn reach_scales_with_the_weapon() {
        let t = LungeTuning::default();
        let sword = swing_reach(&t, Form::Sword, 1.1, false, false, 1.0).unwrap();
        let hammer = swing_reach(&t, Form::Hammer, 1.2, false, false, 1.0).unwrap();
        let heavy = swing_reach(&t, Form::Hammer, 1.2, true, false, 1.0).unwrap();
        let giant = swing_reach(&t, Form::Hammer, 3.5, true, false, 1.0).unwrap();
        assert!((4.0..=5.0).contains(&sword.range), "sword lunges ~4-5 m: {}", sword.range);
        assert!(sword.range > hammer.range && hammer.range > heavy.range);
        assert!(giant.range - giant.reach < heavy.range - heavy.reach, "a giant hammer dashes shorter");
        assert!(giant.dash_speed < heavy.dash_speed && heavy.dash_speed < sword.dash_speed);
        assert!(swing_reach(&t, Form::Whip, 1.0, false, false, 1.0).is_none());
        assert!(swing_reach(&t, Form::Gun, 1.0, false, false, 1.0).is_none());
    }

    #[test]
    fn target_needs_range_cone_and_sight() {
        let t = LungeTuning::default();
        let r = swing_reach(&t, Form::Sword, 1.1, false, false, 1.0).unwrap();
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let near = hull_at(Vec3::new(0.0, 0.0, -3.5));
        let side = hull_at(Vec3::new(3.0, 0.0, -1.0));
        let far = hull_at(Vec3::new(0.0, 0.0, -9.0));
        let got = swing_target(eye, Vec3::NEG_Z, &r, [(PlayerId(2), side), (PlayerId(3), near), (PlayerId(4), far)], |_, _| true);
        assert_eq!(got.map(|g| g.id), Some(PlayerId(3)));
        assert!(swing_target(eye, Vec3::NEG_Z, &r, [(PlayerId(3), near)], |_, _| false).is_none(), "walls block");
        assert!(swing_target(eye, Vec3::X, &r, [(PlayerId(3), near)], |_, _| true).is_none(), "outside the cone");
    }

    #[test]
    fn dash_arrives_and_stops_committed() {
        let t = LungeTuning::default();
        let p = MoveParams::default();
        let r = swing_reach(&t, Form::Sword, 1.1, false, false, 1.0).unwrap();
        let mut s = PlayerState::new(Vec3::ZERO, &p);
        let target = Vec3::new(0.0, 1.0, -4.5);
        s.lunge = plan_dash(&t, &r, s.chest(&p), s.vel, target, Vec3::ZERO).unwrap();
        let dt = bd_core::DT;
        let mut ticks = 0;
        while s.lunge.active() && ticks < 100 {
            dash_pre_move(&mut s, &p, dt);
            s.pos += s.vel * dt;
            ticks += 1;
        }
        let d = (target - s.chest(&p)).length();
        assert!(d <= s.lunge.stop.max(t.stop_max) + 0.05, "stopped near the target: {d}");
        assert!(s.vel.length() <= t.exit_speed + 1e-3, "committed stop: {}", s.vel.length());
        assert!((ticks as f32) * dt <= t.dash_max_secs + 2.0 * dt);
        // A faster approach keeps its own speed (no stacking on a bhop).
        let mut f = PlayerState::new(Vec3::ZERO, &p);
        f.vel = Vec3::new(0.0, 0.0, -40.0);
        f.lunge = plan_dash(&t, &r, f.chest(&p), f.vel, Vec3::new(0.0, 1.0, -30.0), Vec3::ZERO).unwrap();
        dash_pre_move(&mut f, &p, dt);
        assert!(f.vel.length() <= 40.0 + 1e-3);
    }

    #[test]
    fn nudge_is_bounded() {
        let look = Vec3::NEG_Z;
        let eye = Vec3::ZERO;
        let tgt = Vec3::new(1.0, 0.0, -2.0);
        let b = bias_look(look, eye, tgt, 5f32.to_radians(), 1.0);
        assert!((b.dot(look).acos().to_degrees() - 5.0).abs() < 0.01);
        assert_eq!(bias_look(look, eye, Vec3::new(5.0, 0.0, 0.1), 0.2, 0.5), look, "behind: no nudge");
    }

    #[test]
    fn hostile_tuning_is_clamped() {
        let t = LungeTuning {
            dash_speed: f32::NAN,
            range_max: -3.0,
            forms: vec![FormReach { form: Form::Sword, mult: f32::INFINITY }],
            ..Default::default()
        }
        .validated();
        assert_eq!(t.dash_speed, LungeTuning::default().dash_speed);
        assert!(t.range_max >= t.range_min);
        assert_eq!(t.forms[0].mult, 0.0);
    }
}
