//! COMBO TARGET — a soft target for intentional strings (owner: no hard lock-on, no camera
//! tracking: the aim skill ceiling and FFA awareness stay; strings still connect).
//!
//! * **Pick** ([`pick`], pure; the client runs the same function on its own aim + view to predict
//!   the mark): when a toolbox command starts (a chain, a single move, a hotbar key — every
//!   `World::toolbox` call), the enemy under / nearest the crosshair inside a small cone
//!   ([`PICK_CONE_DEG`], [`PICK_RANGE`], clear voxel line of sight) becomes the command's target.
//!   A player NAMED in the command ("teleport behind ethan -> doomfist punch": a composed step's
//!   `Target::Player`, or an aim's `TargetHint::Named`) wins over the crosshair. The host picks
//!   against the hulls AS THE CASTER SAW THEM (lag-compensated history, like hit tests).
//! * **Steer**: every step fires with its aim swung onto the target's current body
//!   ([`World::combo_steer`]): movement ops land relative to them (a blink stops short of them, an
//!   untargeted "teleport behind" goes behind THEM), strikes / projectiles / spells aim at them. The
//!   camera is never touched.
//! * **Drop** ([`World::combo_tick`], per tick while one is live): the chain ended (the last step's
//!   active phase is over), the attacker LOOKED WELL AWAY (turned their view [`TURN_AWAY_DEG`] off
//!   where it was while the target was still within [`LOOK_AWAY_DEG`] of it — so a teleport that
//!   leaves the target behind you doesn't drop it, flicking away does), line of sight lost for more
//!   than [`LOS_GRACE`] s, KO / elimination of either, or out of [`KEEP_RANGE`]. The remaining steps
//!   then fire along the live aim (the command's crosshair re-applied to the look, as before).
//! * **Hold to focus** (`buttons::FOCUS`, bindable, unbound by default): while held the target
//!   persists across separate commands (a new command keeps it instead of re-picking; a named
//!   player still wins) and turning away doesn't drop it (line of sight still does).
//! * Replicated as `PlayerSnap::hunt` (target id + 1): the target sees a "hunted" mark and can
//!   counterplay; the attacker sees a subtle outline.
//!
//! Deterministic, allocation-free; nothing runs (or changes) for players who never use the toolbox.

use bd_core::voxel::{Aabb, VoxelQuery};
use bd_core::{PlayerId, Vec3};

use super::chain::ToolStep;
use super::compose::{Forced, Target};
use crate::world::{Aim, TargetHint, World};

/// Acquisition cone half-angle around the crosshair (deg).
pub const PICK_CONE_DEG: f32 = 10.0;
/// Farthest a target is picked (m, eye to body centre).
pub const PICK_RANGE: f32 = 70.0;
/// Farthest a live target is kept (m).
pub const KEEP_RANGE: f32 = 90.0;
/// A target further than this off the look is "out of view" (deg)…
pub const LOOK_AWAY_DEG: f32 = 55.0;
/// …and turning the view this far (deg) from where it was while they were in view drops them.
pub const TURN_AWAY_DEG: f32 = 40.0;
/// Line of sight may be lost this long (s) before the target drops.
pub const LOS_GRACE: f32 = 0.5;
/// Blinks toward the target stop this far (m) short of their body centre.
pub const BLINK_STOP: f32 = 2.2;

/// A live combo target (on `Toolbox::hunt`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hunt {
    pub target: PlayerId,
    /// Named in the words (not dropped by turning away).
    pub named: bool,
    /// Seconds line of sight has been lost.
    pub lost: f32,
    /// The attacker's look the last time the target was in view (turn-away reference).
    pub anchor: Vec3,
}

/// Why a combo target ended (tests / debug).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropWhy {
    ChainEnded,
    LookedAway,
    LostSight,
    Gone,
    Range,
}

/// The replicated byte (`PlayerSnap::hunt`): target id + 1, 0 = none.
pub fn wire(h: Option<&Hunt>) -> u8 {
    h.map_or(0, |h| h.target.0.wrapping_add(1))
}

/// Decode [`wire`].
pub fn unwire(b: u8) -> Option<PlayerId> {
    b.checked_sub(1).map(PlayerId)
}

/// PICK RULE (pure, host + client): among `candidates` (id, body hull), the one the crosshair ray
/// (`eye`, `dir`) is on, else the one nearest the crosshair inside [`PICK_CONE_DEG`], within
/// [`PICK_RANGE`] and with a clear line (`clear(from, to)`). Ties: nearer, then lower id.
pub fn pick(eye: Vec3, dir: Vec3, candidates: impl IntoIterator<Item = (PlayerId, Aabb)>, clear: impl Fn(Vec3, Vec3) -> bool) -> Option<PlayerId> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO || !eye.is_finite() {
        return None;
    }
    let cos = PICK_CONE_DEG.to_radians().cos();
    let mut best: Option<(f32, f32, PlayerId)> = None;
    for (id, hull) in candidates {
        let c = (hull.min + hull.max) * 0.5;
        let d = c - eye;
        let len = d.length();
        if !len.is_finite() || !(0.3..=PICK_RANGE).contains(&len) {
            continue;
        }
        // on the body (slightly fat) = dead centre, else the angle to its middle
        let on = crate::geom::segment_aabb(eye, eye + dir * PICK_RANGE, &crate::geom::inflate(&hull, 0.3)).is_some();
        let dot = if on { 1.0 } else { d.dot(dir) / len };
        if dot < cos || !clear(eye, c) {
            continue;
        }
        let better = match best {
            None => true,
            Some((bd, bl, bid)) => dot > bd + 1e-6 || ((dot - bd).abs() <= 1e-6 && (len < bl - 1e-4 || ((len - bl).abs() <= 1e-4 && id.0 < bid.0))),
        };
        if better {
            best = Some((dot, len, id));
        }
    }
    best.map(|b| b.2)
}

/// Voxel line of sight between two points (the last half metre is the body itself).
pub fn clear_line<W: VoxelQuery + ?Sized>(voxels: &W, a: Vec3, b: Vec3) -> bool {
    let d = b - a;
    let len = d.length();
    if len < 0.6 {
        return true;
    }
    bd_core::voxel::raycast(voxels, a, d / len, len - 0.5).is_none()
}

/// The player a command names (the first composed step with a player target, else a named hint).
pub fn named_in(steps: &[ToolStep], aim: Option<&Aim>) -> Option<PlayerId> {
    for s in steps {
        if let ToolStep::Composed(c) = s
            && let Target::Player(id) = c.target
        {
            return Some(id);
        }
    }
    match aim.and_then(|a| a.target_hint) {
        Some(TargetHint::Named { player, .. }) => Some(player),
        _ => None,
    }
}

impl World {
    /// A target hunt of player `i` may keep / take: alive, in the match, not you, in range.
    fn combo_valid(&self, i: usize, t: PlayerId) -> Option<usize> {
        let k = self.pidx(t)?;
        let (p, q) = (&self.players[i], &self.players[k]);
        (k != i && q.alive && !q.eliminated && (q.mv.pos - p.mv.pos).length() <= KEEP_RANGE).then_some(k)
    }

    /// A new command's target for player `i` (called by `World::toolbox` with the command's
    /// validated crosshair ray): named > held focus > the crosshair pick. Sets / clears the hunt.
    pub(crate) fn combo_acquire(&mut self, i: usize, steps: &[ToolStep], aim: Option<&Aim>) {
        let look = self.players[i].look();
        let focus = self.players[i].input.held(bd_core::input::buttons::FOCUS);
        let named = named_in(steps, aim).filter(|&t| self.combo_valid(i, t).is_some());
        let prev = self.players[i].power.toolbox.hunt.filter(|h| self.combo_valid(i, h.target).is_some());
        let hunt = if let Some(t) = named {
            Some(Hunt { target: t, named: true, lost: 0.0, anchor: look })
        } else if focus && prev.is_some() {
            prev
        } else {
            let (eye, dir) = match aim {
                Some(a) => self.resolve_aim(i, a),
                None => (self.players[i].eye(&self.cfg.move_params), look),
            };
            let me = self.players[i].id;
            let view = self.players[i].view_tick;
            let base = &self.cfg.move_params;
            let cands = self.players.iter().filter(|q| q.id != me && q.alive && !q.eliminated).map(|q| {
                // as the caster saw them (the same rewind hit tests use)
                let hull = view.and_then(|vt| self.history.hull_at(q.id, vt)).unwrap_or_else(|| q.hull(base));
                (q.id, hull)
            });
            pick(eye, dir, cands, |a, b| clear_line(&self.voxels, a, b)).map(|t| Hunt { target: t, named: false, lost: 0.0, anchor: look })
        };
        self.players[i].power.toolbox.hunt = hunt;
    }

    /// The live combo target's body centre for player `i` (None = no valid target).
    pub(crate) fn combo_point(&self, i: usize) -> Option<Vec3> {
        let h = self.players.get(i)?.power.toolbox.hunt?;
        let k = self.combo_valid(i, h.target)?;
        Some(self.players[k].hull(&self.cfg.move_params).center())
    }

    /// The live combo target as a composed-move target (VERB GRAMMAR: an untargeted described step
    /// goes for it; "him" / "same" fall back to it before the crosshair).
    pub(crate) fn combo_forced(&self, i: usize) -> Option<Forced> {
        let h = self.players.get(i)?.power.toolbox.hunt?;
        self.combo_valid(i, h.target).map(|_| Forced::Player(h.target))
    }

    /// STEER: swing the move aim being fired for player `i` (`powers.move_aim`) onto the combo
    /// target (origin kept: the eye, or the validated 3P crosshair origin). No-op without one.
    pub(crate) fn combo_steer(&mut self, i: usize) {
        let Some(pt) = self.combo_point(i) else { return };
        let origin = match self.powers.move_aim {
            Some((k, o, _)) if k == i => o,
            _ => self.players[i].eye(&self.cfg.move_params),
        };
        let d = pt - origin;
        if d.length_squared() > 0.01 {
            self.powers.move_aim = Some((i, origin, d.normalize()));
        }
    }

    /// The aim a step fires with, steered onto the combo target (for steps that take an `Aim`:
    /// casts, twists). `base` = the live aim (its origin is kept). A cast also names the target
    /// (`TargetHint::Player`) when it has no hint of its own, so creatures / homing go for them.
    pub(crate) fn combo_aim(&self, i: usize, base: Option<Aim>) -> Option<Aim> {
        let Some(pt) = self.combo_point(i) else { return base };
        let target = self.players[i].power.toolbox.hunt.map(|h| h.target);
        let origin = base.map_or_else(|| self.players[i].eye(&self.cfg.move_params), |a| self.resolve_aim(i, &a).0);
        let d = pt - origin;
        if d.length_squared() <= 0.01 {
            return base;
        }
        let mut a = Aim::new(origin, d.normalize());
        if let Some(b) = base {
            a.modifier = b.modifier;
            a.target_hint = b.target_hint;
        }
        if a.target_hint.is_none() {
            a.target_hint = target.map(TargetHint::Player);
        }
        Some(a)
    }

    /// A blink of `want` m along `dir` for player `i`: stop [`BLINK_STOP`] short of a combo target
    /// that lies ahead on the way (you land in front of them, not through / past them).
    pub(crate) fn combo_blink_cap(&self, i: usize, from: Vec3, dir: Vec3, want: f32) -> f32 {
        let Some(pt) = self.combo_point(i) else { return want };
        let d = pt - from;
        let along = d.dot(dir);
        if along <= 0.0 {
            return want;
        }
        want.min((along - BLINK_STOP).max(0.0))
    }

    /// Per tick for player `i` (from `step_toolbox`): the drop rules (module docs). Returns why a
    /// target was dropped this tick.
    pub(crate) fn combo_tick(&mut self, i: usize, dt: f32) -> Option<DropWhy> {
        let h = self.players[i].power.toolbox.hunt?;
        let p = &self.players[i];
        let focus = p.input.held(bd_core::input::buttons::FOCUS);
        let why = 'why: {
            if !p.alive {
                break 'why Some(DropWhy::Gone);
            }
            let Some(k) = self.pidx(h.target).filter(|&k| k != i && self.players[k].alive && !self.players[k].eliminated) else {
                break 'why Some(DropWhy::Gone);
            };
            let tb = &p.power.toolbox;
            if tb.queue.is_empty() && !tb.busy() && !focus && !self.op_busy(p.id) {
                break 'why Some(DropWhy::ChainEnded);
            }
            let eye = p.eye(&self.cfg.move_params);
            let c = self.players[k].hull(&self.cfg.move_params).center();
            if (self.players[k].mv.pos - p.mv.pos).length() > KEEP_RANGE {
                break 'why Some(DropWhy::Range);
            }
            let tb = &mut self.players[i].power.toolbox;
            let Some(hunt) = tb.hunt.as_mut() else { break 'why None };
            if clear_line(&self.voxels, eye, c) {
                hunt.lost = 0.0;
            } else {
                hunt.lost += dt;
                if hunt.lost > LOS_GRACE {
                    break 'why Some(DropWhy::LostSight);
                }
            }
            let look = self.players[i].look();
            let to = (c - eye).normalize_or(look);
            let hunt = self.players[i].power.toolbox.hunt.as_mut()?;
            if look.dot(to) >= LOOK_AWAY_DEG.to_radians().cos() {
                hunt.anchor = look;
            } else if !hunt.named && !focus && look.dot(hunt.anchor) < TURN_AWAY_DEG.to_radians().cos() {
                break 'why Some(DropWhy::LookedAway);
            }
            None
        };
        if why.is_some() {
            self.players[i].power.toolbox.hunt = None;
        }
        why
    }

    /// Read-only: player `player`'s combo target (tests / host UI; clients read `PlayerSnap::hunt`).
    pub fn combo_target(&self, player: PlayerId) -> Option<PlayerId> {
        self.player(player).and_then(|p| p.power.toolbox.hunt).map(|h| h.target)
    }
}
