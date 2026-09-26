//! Hook-and-strike (owner: "how could we easily enable players to seek each other?"): a tether
//! fired at a **player** hooks them. One rule set (COMBAT.md "Hook-and-strike"):
//!
//! * **Hook.** With the match rule `tether_hooks_players` on (default), every hookable player
//!   (alive, no spawn shield, not in a cockpit) is a non-solid, hook-anchorable box for the other
//!   players' tethers — the same hook projectile, the same crosshair query (its small aim-assist
//!   cone included). The host tests each shooter against the boxes *as that shooter saw them*
//!   (lag compensation, `view_tick`).
//! * **Reel.** A rope on a player pulls **you** toward them (rope pull + reel, like any anchor) but
//!   is never a rigid rope: the victim is never towed and never tows you.
//! * **Slam.** Arrive within `arrive_dist` holding a melee weapon: the rope lets go and your next
//!   swing (started within `slam_window`) is a slam — its hits get `slam_knockback` × knockback,
//!   extra lift and juice. One swing, consumed.
//! * **Escape — the rope breaks when:** the victim dodges ([`hook_breaks_on_dodge`]; a dodging
//!   player can't be hooked either) or boosts, the victim fires a tether of their own, the rope is
//!   longer than `max_range`, the line of sight is lost for `los_grace`, or the victim's swing
//!   cuts through the rope. (Plus the obvious: either player KO'd, the hooker lets go.)
//!
//! The too-long rule depends only on the hooker's own state, so client prediction applies it too
//! ([`local_rules`]); the victim-driven breaks arrive as snapshot corrections.
//! Events: [`crate::events::GameEvent::Assist`] with an [`AssistEvent`].

use bd_core::input::buttons;
use bd_core::voxel::{Aabb, raycast};
use bd_core::{DT, PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::movement::{AnchorTarget, Collider, ColliderId, Hook, HookPhase, MoveParams, PlayerState};
use crate::rng::SimRng;
use crate::world::{Player, World};

/// Hook-and-strike numbers (`combat.ron` → `assist.hook`). Never shown to players.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HookTuning {
    /// Players' hook boxes are their hulls grown by this (m): generous, like a projectile radius.
    pub radius: f32,
    /// A rope to a player longer than this snaps (m; tethers reach 120 m on terrain).
    pub max_range: f32,
    /// Seconds a rope may stay out of sight before it snaps.
    pub los_grace: f32,
    /// A victim's blade passing within its radius + this of the rope cuts it (m).
    pub cut_margin: f32,
    /// The victim's boost (dodge) breaks the rope.
    pub breaks_on_boost: bool,
    /// The victim firing a tether of their own breaks the rope.
    pub breaks_on_tether: bool,
    /// Holding a melee weapon and this close (chest to the victim's hull): the rope lets go and the
    /// next swing slams (m).
    pub arrive_dist: f32,
    /// Seconds after arriving in which a started swing becomes the slam.
    pub slam_window: f32,
    /// Slam hits: knockback ×, extra upward lift added to the direction, juice ×.
    pub slam_knockback: f32,
    pub slam_lift: f32,
    pub slam_impact: f32,
    /// Bots with a melee weapon try a hook-and-strike at this rate (per second) on a visible target
    /// between `bot_min_dist` and `bot_max_dist`, at most once per `bot_cooldown`.
    pub bot_rate: f32,
    pub bot_min_dist: f32,
    pub bot_max_dist: f32,
    pub bot_cooldown: f32,
}

impl Default for HookTuning {
    fn default() -> Self {
        Self {
            radius: 0.3,
            max_range: 40.0,
            los_grace: 0.25,
            cut_margin: 0.3,
            breaks_on_boost: true,
            breaks_on_tether: true,
            arrive_dist: 3.0,
            slam_window: 1.0,
            slam_knockback: 1.5,
            slam_lift: 0.45,
            slam_impact: 1.6,
            bot_rate: 0.35,
            bot_min_dist: 8.0,
            bot_max_dist: 30.0,
            bot_cooldown: 6.0,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl HookTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.radius = fin(self.radius, d.radius, 0.0, 2.0);
        self.max_range = fin(self.max_range, d.max_range, 1.0, 200.0);
        self.los_grace = fin(self.los_grace, d.los_grace, 0.0, 5.0);
        self.cut_margin = fin(self.cut_margin, d.cut_margin, 0.0, 3.0);
        self.arrive_dist = fin(self.arrive_dist, d.arrive_dist, 0.0, 20.0);
        self.slam_window = fin(self.slam_window, d.slam_window, 0.0, 10.0);
        self.slam_knockback = fin(self.slam_knockback, d.slam_knockback, 0.0, 5.0);
        self.slam_lift = fin(self.slam_lift, d.slam_lift, 0.0, 3.0);
        self.slam_impact = fin(self.slam_impact, d.slam_impact, 0.0, 5.0);
        self.bot_rate = fin(self.bot_rate, d.bot_rate, 0.0, 10.0);
        self.bot_min_dist = fin(self.bot_min_dist, d.bot_min_dist, 0.0, 200.0);
        self.bot_max_dist = fin(self.bot_max_dist, d.bot_max_dist, self.bot_min_dist, 200.0);
        self.bot_cooldown = fin(self.bot_cooldown, d.bot_cooldown, 0.0, 120.0);
        self
    }
}

/// Why a rope on a player broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum HookBreak {
    /// The victim boosted (dodge).
    Dodged = 0,
    /// The victim fired a tether of their own.
    Countered = 1,
    /// Longer than `max_range`.
    TooFar = 2,
    /// Out of sight for `los_grace`.
    LostSight = 3,
    /// The victim's swing cut the rope.
    Cut = 4,
}

/// What [`crate::events::GameEvent::Assist`] reports (audio + VFX hooks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssistEvent {
    /// Melee magnetism lunged `player` toward `other`.
    Lunge,
    /// `player`'s tether (`hand` 0 = left, 1 = right) hooked `other` ("thwip").
    Hooked { hand: u8 },
    /// That rope broke (the victim escaped, or it got too long / lost sight).
    HookBroken { hand: u8, why: HookBreak },
    /// `player` reeled in to `other` with a melee weapon: the next swing slams.
    SlamReady,
    /// The slam connected on `other` at `pos`.
    Slam,
}

/// The player a hook is attached to, if any.
#[inline]
pub fn hooked_player(h: &Hook) -> Option<PlayerId> {
    match (h.phase, h.target) {
        (HookPhase::Attached, Some(AnchorTarget::Collider(ColliderId::Player(id)))) => Some(id),
        _ => None,
    }
}

/// Can tethers hook this player right now? (Not while dodging: the dodge is intangible.)
#[inline]
pub fn hookable(p: &Player) -> bool {
    p.alive && !p.eliminated && !p.shielded() && p.seat.is_none_or(|s| s.suit) && p.mv.pos.is_finite() && !hook_breaks_on_dodge(p)
}

/// DODGE HOOK (double-jump / dodge worker): is `p` dodging this tick (the short intangible burst)?
/// A dodge breaks any rope on them ([`HookBreak::Dodged`]) and they can't be hooked during it.
/// Wired to the dodge burst: `PlayerState::extra.dodging()` (see `crate::dodge`; the start tick is
/// also `MoveEvents::dodged`).
#[inline]
pub fn hook_breaks_on_dodge(p: &Player) -> bool {
    p.mv.extra.dodging() || p.move_events.dodged
}

/// A player's hook box: non-solid (never blocks movement), hook-anchorable.
pub fn player_collider(id: PlayerId, hull: Aabb, vel: Vec3, radius: f32) -> Collider {
    let mut c = Collider::new(hull.center(), hull.half() + Vec3::splat(radius.max(0.0)), Quat::IDENTITY, ColliderId::Player(id));
    c.vel = if vel.is_finite() { vel } else { Vec3::ZERO };
    c.solid = false;
    c.anchorable = true;
    c
}

/// Rules that depend only on the hooker's own movement state (host and client prediction apply
/// them at the same point): a rope on a player longer than `max_range` lets go. Returns the hands
/// that broke.
pub fn local_rules(s: &mut PlayerState, t: &HookTuning, p: &MoveParams) -> [bool; 2] {
    let mut out = [false; 2];
    let chest = s.chest(p);
    for (i, broke) in out.iter_mut().enumerate() {
        if hooked_player(&s.hooks[i]).is_some() && (s.hooks[i].pos - chest).length() > t.max_range {
            s.hooks[i] = Hook::default();
            *broke = true;
        }
    }
    out
}

/// Closest distance between segments `a0 → a1` and `b0 → b1`.
pub fn segment_distance(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> f32 {
    let d1 = a1 - a0;
    let d2 = b1 - b0;
    let r = a0 - b0;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t) = if a <= 1e-9 && e <= 1e-9 {
        (0.0, 0.0)
    } else if a <= 1e-9 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 1e-9 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom > 1e-9 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    ((a0 + d1 * s) - (b0 + d2 * t)).length()
}

// ------------------------------------------------------------------------------------------ bots

/// A bot's hook-and-strike attempt (see [`bot_hook_plan`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BotHook {
    /// Seconds left in the current attempt (0 = none).
    pub t: f32,
    /// Seconds until the next attempt may start.
    pub cd: f32,
    /// The right hook was fired this attempt.
    pub fired: bool,
}

/// Bots: now and then, with a melee weapon in hand and a visible target at mid range, hook the
/// target and reel in to slam it. Returns the point to aim at while an attempt runs. Consumes
/// randomness only while holding a melee weapon (matches without melee weapons are unchanged).
pub fn bot_hook_plan(
    bh: &mut BotHook,
    w: &World,
    me: &Player,
    target: Option<&Player>,
    eye: Vec3,
    can: bool,
    rng: &mut SimRng,
) -> Option<Vec3> {
    let t = &w.cfg.tuning.assist.hook;
    bh.cd = (bh.cd - DT).max(0.0);
    let base = w.params();
    let melee =
        w.constructs.held_by(me.id).and_then(|c| crate::melee_assist::reach_of(&w.cfg.tuning.assist.lunge, c, me.size.scale)).is_some();
    let Some(tp) = target.filter(|_| can && melee && w.rules().tether_hooks_players) else {
        bh.t = 0.0;
        return None;
    };
    let chest = tp.hull(base).center();
    let d = chest - eye;
    let dist = d.length();
    let hook = me.mv.hooks[1];
    if bh.t > 0.0 {
        bh.t -= DT;
        let over = (bh.fired && hook.phase == HookPhase::Idle) || !hookable(tp) || bh.t <= 0.0;
        if over {
            bh.t = 0.0;
            bh.cd = t.bot_cooldown;
            return None;
        }
        return Some(chest);
    }
    if bh.cd > 0.0 || hook.phase != HookPhase::Idle || !hookable(tp) || dist < t.bot_min_dist || dist > t.bot_max_dist.min(t.max_range) {
        return None;
    }
    let visible = raycast(&w.voxels, eye, d / dist.max(1e-4), dist).is_none();
    if visible && rng.chance(t.bot_rate * DT) {
        bh.t = 2.5;
        bh.fired = false;
        return Some(chest);
    }
    None
}

/// Buttons for a running attempt: fire the right tether once the aim is on the target, hold it,
/// reel while attached.
pub fn bot_hook_buttons(bh: &mut BotHook, me: &Player, aim_err: f32) -> u16 {
    if bh.t <= 0.0 {
        return 0;
    }
    let hook = me.mv.hooks[1];
    if hook.phase != HookPhase::Idle {
        bh.fired = true;
        return buttons::TETHER_R | if hook.attached() { buttons::REEL } else { 0 };
    }
    if !bh.fired && aim_err < 0.05 {
        return buttons::TETHER_R;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_distance_cases() {
        let d = segment_distance(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 1.0), Vec3::new(0.0, 1.0, 1.0));
        assert!((d - 1.0).abs() < 1e-5);
        let p = segment_distance(Vec3::ZERO, Vec3::X, Vec3::new(3.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0));
        assert!((p - 2.0).abs() < 1e-5);
        assert_eq!(segment_distance(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO), 0.0);
    }

    #[test]
    fn too_long_rope_lets_go() {
        let p = MoveParams::default();
        let mut s = PlayerState::new(Vec3::ZERO, &p);
        s.hooks[0] = Hook {
            phase: HookPhase::Attached,
            pos: Vec3::new(0.0, 1.0, -60.0),
            target: Some(AnchorTarget::Collider(ColliderId::Player(PlayerId(2)))),
            ..Hook::default()
        };
        s.hooks[1] = Hook {
            phase: HookPhase::Attached,
            pos: Vec3::new(0.0, 1.0, -60.0),
            target: Some(AnchorTarget::Voxel(bd_core::IVec3::ZERO)),
            ..Hook::default()
        };
        let t = HookTuning::default();
        assert_eq!(local_rules(&mut s, &t, &p), [true, false]);
        assert_eq!(s.hooks[0].phase, HookPhase::Idle);
        assert!(s.hooks[1].attached(), "terrain ropes are not affected");
    }
}
