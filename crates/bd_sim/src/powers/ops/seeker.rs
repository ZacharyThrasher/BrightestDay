//! `Seeker { .. }` — the weapon that never misses (docs/toolbox/OPS.md): Gungnir, the Sudarshan
//! Chakra, Brahmastra, a whistled arrow.
//!
//! The move throws its `recipe` as a real, visible construct (a spear, a disc, an arrow) at the
//! enemy nearest the crosshair — the crosshair ray first, else anyone inside a generous `cone`, with
//! NO line of sight needed. It then flies a route of its own: a turn-rate-limited pursuit that looks
//! `LOOKAHEAD` ahead and bends over or around cover (up first, then sideways), so it visibly arcs
//! round the pillar you hid behind. Every enemy its tip crosses is struck once (`damage`,
//! `knockback`, `up`, `element`); after a strike it chains to the nearest unstruck enemy within
//! `chain` m until `hits` strikes are spent, then flies home and is caught. Nobody targeted = it
//! flies straight along the aim for `range` m and comes back.
//!
//! Counterplay (owner rule: nothing is inescapable): a DODGE through the tip makes it miss you (it
//! moves on); an enemy construct in its path (a wall, a barrier) takes the blow and turns it home.
//!
//! The body is a `driven`, thrown-flagged construct (constructs/mod.rs): no controller / physics touches it; this
//! file writes its pose every tick and clients see the pose through ordinary construct snapshots.
//! Deterministic, allocation-free per tick (one slot per player index); nothing runs unless a
//! move uses it.

use bd_core::mechanics::{Element, Mechanic};
use bd_core::{ConstructId, MAX_PLAYERS, PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::{OpEvent, fclamp};
use crate::combat::{ConstructHit, HitInfo, HitKind};
use crate::constructs::{AnimState, SpawnParams};
use crate::events::{GameEvent, GoneReason};
use crate::geom;
use crate::powers::{PowerError, RecipeDef};
use crate::willpower::validate_recipe;
use crate::world::World;

/// Seconds out before it turns home no matter what.
pub const MAX_OUT: f32 = 5.0;
/// Seconds after which a seeker that never made it home dissolves where it is.
pub const MAX_LIFE: f32 = 9.0;
/// Turn rate (rad/s): a visible arc, never a right-angle snap.
pub const TURN_RATE: f32 = 7.0;
/// Obstacle look-ahead (m) at most.
pub const LOOKAHEAD: f32 = 9.0;
/// Caught when this close to the thrower's hand (m).
pub const CATCH_DIST: f32 = 1.3;
pub const MAX_HITS: u8 = 6;

/// Players already struck, by id (indices shift when someone leaves). Fixed size, no heap.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Struck([Option<PlayerId>; MAX_PLAYERS]);

impl Struck {
    pub fn has(&self, id: PlayerId) -> bool {
        self.0.contains(&Some(id))
    }
    pub fn add(&mut self, id: PlayerId) {
        if !self.has(id) {
            if let Some(slot) = self.0.iter_mut().find(|s| s.is_none()) {
                *slot = Some(id);
            }
        }
    }
}

/// A beat of a seeker's flight (presentation: the launch whistle, the strike, the catch).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeekPhase {
    Launch,
    /// Struck `target`.
    Strike,
    /// `target` dodged through it.
    Dodged,
    /// Bent onto the next enemy (`target`).
    Chain,
    /// An enemy construct took the blow.
    Blocked,
    /// Turned for home.
    Return,
    /// Back in the hand.
    Catch,
}

/// The authored numbers (see `MoveOp::Seeker`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SeekerSpec {
    pub recipe: RecipeDef,
    pub range: f32,
    pub cone: f32,
    pub speed: f32,
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub element: Element,
    pub hits: u8,
    pub chain: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Seeker {
    pub owner: PlayerId,
    cid: ConstructId,
    /// Grip position (construct origin) and flight direction.
    pos: Vec3,
    dir: Vec3,
    speed: f32,
    /// Grip → tip (m).
    tip: f32,
    t: f32,
    target: Option<PlayerId>,
    hits_left: u8,
    /// Players already struck.
    struck: Struck,
    returning: bool,
    /// Seconds spent making no headway against terrain.
    stuck: f32,
    damage: f32,
    knockback: f32,
    up: f32,
    element: Element,
    chain: f32,
    /// Straight-line flight with no target: where it turns back.
    reach: f32,
    travelled: f32,
}

fn hand_of(eye: Vec3, look: Vec3) -> Vec3 {
    let right = look.cross(Vec3::Y).normalize_or(Vec3::X);
    eye + right * 0.3 - Vec3::Y * 0.3 + look * 0.4
}

/// Turn `dir` toward `want` by at most `max` radians (both unit).
fn turn_toward(dir: Vec3, want: Vec3, max: f32) -> Vec3 {
    let a = dir.angle_between(want);
    if !a.is_finite() || a <= max || a < 1e-4 {
        return want;
    }
    let axis = dir.cross(want);
    let axis = if axis.length_squared() < 1e-8 { dir.any_orthonormal_vector() } else { axis.normalize() };
    (Quat::from_axis_angle(axis, max) * dir).normalize_or(want)
}

impl World {
    pub(crate) fn op_seeker(&mut self, i: usize, spec: SeekerSpec) -> Result<(), PowerError> {
        let owner = self.players[i].id;
        match self.powers.ops.seekers[i] {
            Some(s) if s.owner == owner => return Err(PowerError::Busy),
            // A departed player's seeker in this slot: its spear goes with it.
            Some(s) => {
                self.end_seeker(&s);
                self.powers.ops.seekers[i] = None;
            }
            None => {}
        }
        if self.constructs.len() >= crate::world::MAX_CONSTRUCTS - 8 {
            return Err(PowerError::Busy);
        }
        let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() && v > 0.0 { v.clamp(lo, hi) } else { d };
        let range = fin(spec.range, 45.0, 4.0, 80.0);
        let cone = fin(spec.cone, 25.0, 1.0, 45.0);
        let speed = fin(spec.speed, 34.0, 8.0, 70.0);
        let (eye, look) = self.op_eye_look(i);
        let target = self.seeker_pick(i, eye, look, range, cone).map(|k| self.players[k].id);
        // The body: the authored recipe, exactly as it looks (no wings bolted on), driven by the op
        // (never homing / returning / exploding on its own — the op decides everything).
        let mut r = spec.recipe.to_recipe();
        r.mechanics.retain(|m| !matches!(m, Mechanic::Returning | Mechanic::Homing | Mechanic::DropFromSky | Mechanic::Explodes | Mechanic::Swarm | Mechanic::Walks | Mechanic::Piloted));
        r.count = 1;
        if r.name.is_empty() {
            r.name = "seeker".into();
        }
        let recipe = validate_recipe(r, &self.cfg.rules);
        let pos = hand_of(eye, look);
        let rot = geom::look_rotation(look);
        let tick = self.tick;
        let cid = self.constructs.spawn(SpawnParams {
            owner,
            recipe: recipe.clone(),
            shape: None,
            pos,
            rot,
            vel: Vec3::ZERO,
            target,
            cost: 0.0,
            integrity: 400.0,
            forming: 0.0,
            life: MAX_LIFE + 1.0,
            tick,
            colossal: false,
            model: None,
        });
        let mut tip = 1.0;
        if let Some(c) = self.constructs.get_mut(cid) {
            c.parent = Some(cid);
            c.driven = true;
            // A held form (spear, disc, axe) out of the hand: replicated with its own pose.
            c.held.thrown = true;
            c.comp.launched = true;
            c.comp.dud = true;
            c.set_anim(AnimState::Move, tick);
            tip = (c.half.max_element() * 1.6).clamp(0.4, 3.0);
        }
        self.inject_event(GameEvent::ConstructSpawned { id: cid, owner, recipe: Box::new(recipe), pos, rot, forming: 0.0, cost: 0.0 });
        self.powers.ops.seekers[i] = Some(Seeker {
            owner,
            cid,
            pos,
            dir: look.normalize_or(Vec3::NEG_Z),
            speed,
            tip,
            t: 0.0,
            target,
            hits_left: spec.hits.clamp(1, MAX_HITS),
            struck: Struck::default(),
            returning: false,
            stuck: 0.0,
            damage: fclamp(spec.damage, 0.0, 60.0),
            knockback: fclamp(spec.knockback, 0.0, 70.0),
            up: fclamp(spec.up, 0.0, 2.0),
            element: spec.element,
            chain: fin(spec.chain, 18.0, 2.0, 40.0),
            reach: range,
            travelled: 0.0,
        });
        self.op_event(OpEvent::Seek { player: owner, phase: SeekPhase::Launch, target, pos, dir: look });
        Ok(())
    }

    /// The crosshair's enemy (terrain blocks the ray), else the enemy closest to the crosshair
    /// inside `cone`° — WITHOUT a line-of-sight check: the seeker finds its own way.
    fn seeker_pick(&self, i: usize, eye: Vec3, look: Vec3, range: f32, cone: f32) -> Option<usize> {
        if let Some(k) = self.op_aimed_player(i, range, 0.0) {
            return Some(k);
        }
        let base = &self.cfg.move_params;
        let cos = cone.to_radians().cos();
        let mut best: Option<(usize, f32)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive || q.shielded() {
                continue;
            }
            let d = q.hull(base).center() - eye;
            let len = d.length();
            if len > range || len < 0.3 {
                continue;
            }
            let dot = d.dot(look) / len;
            if dot >= cos && best.is_none_or(|b| dot > b.1) {
                best = Some((k, dot));
            }
        }
        best.map(|b| b.0)
    }

    pub(crate) fn step_seekers(&mut self, dt: f32) {
        for i in 0..MAX_PLAYERS.min(self.powers.ops.seekers.len()) {
            if let Some(s) = self.powers.ops.seekers[i] {
                let next = self.step_seeker(i, s, dt);
                self.powers.ops.seekers[i] = next;
            }
        }
    }

    fn end_seeker(&mut self, s: &Seeker) {
        if let Some(c) = self.constructs.get_mut(s.cid) {
            if c.gone.is_none() {
                c.gone = Some(GoneReason::Spent);
            }
        }
    }

    fn step_seeker(&mut self, i: usize, mut s: Seeker, dt: f32) -> Option<Seeker> {
        let alive_body = self.constructs.get(s.cid).is_some_and(|c| c.gone.is_none());
        let owner_ok = self.players.get(i).is_some_and(|p| p.id == s.owner);
        if !alive_body || !owner_ok {
            self.end_seeker(&s);
            return None;
        }
        s.t += dt;
        if s.t >= MAX_LIFE {
            self.end_seeker(&s);
            return None;
        }
        let base = self.cfg.move_params.clone();
        let (eye, look) = {
            let p = &self.players[i];
            (p.eye(&base), p.look())
        };
        let hand = hand_of(eye, look);
        // Home when the owner is down, the time is up, or the straight throw ran out.
        if !s.returning && (!self.players[i].alive || s.t >= MAX_OUT || (s.target.is_none() && s.travelled >= s.reach)) {
            s.returning = true;
            self.op_event(OpEvent::Seek { player: s.owner, phase: SeekPhase::Return, target: None, pos: s.pos, dir: s.dir });
        }
        // A target that died / left: the next one, or home.
        if !s.returning {
            if let Some(t) = s.target {
                if !self.players.iter().any(|q| q.id == t && q.alive) {
                    s.target = None;
                    let at = s.pos;
                    self.seeker_next(i, &mut s, at);
                }
            }
        }
        let tip = s.pos + s.dir * s.tip;
        let goal = if s.returning {
            hand
        } else if let Some(k) = s.target.and_then(|t| self.pidx(t)) {
            self.players[k].hull(&base).center()
        } else {
            tip + s.dir * 10.0
        };
        let speed = if s.returning { s.speed * 1.25 } else { s.speed };
        let step = speed * dt;
        if s.returning && (goal - s.pos).length() <= CATCH_DIST.max(step) {
            self.end_seeker(&s);
            self.op_event(OpEvent::Seek { player: s.owner, phase: SeekPhase::Catch, target: None, pos: hand, dir: s.dir });
            return None;
        }
        // Route: straight at the goal when the way is clear, else the first clear bend (up first).
        let to_goal = goal - tip;
        let dist = to_goal.length();
        let want = to_goal.normalize_or(s.dir);
        let ahead = (speed * 0.3).clamp(2.5, LOOKAHEAD).min(dist.max(0.5));
        let chosen = if self.seeker_clear(tip, want, ahead) { want } else { self.seeker_bend(tip, want, ahead) };
        s.dir = turn_toward(s.dir, chosen, TURN_RATE * dt);
        // Advance; terrain right in front (a thin gap the bend didn't see) holds it back a beat.
        let blocked = bd_core::voxel::raycast(&self.voxels, tip, s.dir, step + 0.25).is_some();
        let p0 = tip;
        if blocked && !s.returning {
            s.stuck += dt;
            // Pinned against terrain for too long: straight up and over.
            if s.stuck > 0.4 {
                s.dir = turn_toward(s.dir, Vec3::Y, TURN_RATE * 2.0 * dt);
            }
            if s.stuck > 1.2 {
                s.returning = true;
            }
        } else {
            // Home through anything: the recall is never stuck.
            s.stuck = 0.0;
            s.pos += s.dir * step;
            s.travelled += step;
        }
        let p1 = s.pos + s.dir * s.tip;
        // Strikes along the tip's path.
        if !s.returning || s.hits_left > 0 {
            self.seeker_strikes(i, &mut s, p0, p1, &base);
        }
        // An enemy construct in the way takes the blow and turns it home.
        if !s.returning {
            if let Some((cid, _, _)) = self.constructs.segment_query(p0, p1, 0.35, Some(s.owner)) {
                if cid != s.cid {
                    self.apply_construct_hit(ConstructHit { pierce: true, blunt: false, target: cid, source: Some(s.owner), damage: s.damage * 2.0, impulse: s.dir * s.knockback, blade: false, point: p1 });
                    s.returning = true;
                    self.op_event(OpEvent::Seek { player: s.owner, phase: SeekPhase::Blocked, target: None, pos: p1, dir: s.dir });
                }
            }
        }
        if let Some(c) = self.constructs.get_mut(s.cid) {
            c.pos = s.pos;
            c.prev_pos = s.pos;
            c.rot = geom::look_rotation(s.dir);
            c.vel = s.dir * speed;
            c.integrity = c.max_integrity;
        }
        Some(s)
    }

    fn seeker_clear(&self, from: Vec3, dir: Vec3, len: f32) -> bool {
        bd_core::voxel::raycast(&self.voxels, from, dir, len).is_none()
    }

    /// The first clear bend off `want`: up 30 / 55 / 80°, then sideways ±40 / ±75 / ±105°, then
    /// down 35° (fixed order: deterministic). Nothing clear = straight up.
    fn seeker_bend(&self, from: Vec3, want: Vec3, len: f32) -> Vec3 {
        let right = want.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(want).normalize_or(Vec3::Y);
        let rot = |axis: Vec3, deg: f32| (Quat::from_axis_angle(axis, deg.to_radians()) * want).normalize_or(want);
        let cands = [
            rot(right, 30.0),
            rot(right, 55.0),
            rot(right, 80.0),
            rot(up, 40.0),
            rot(up, -40.0),
            rot(up, 75.0),
            rot(up, -75.0),
            rot(up, 105.0),
            rot(up, -105.0),
            rot(right, -35.0),
        ];
        cands.into_iter().find(|d| self.seeker_clear(from, *d, len)).unwrap_or(Vec3::Y)
    }

    fn seeker_strikes(&mut self, i: usize, s: &mut Seeker, p0: Vec3, p1: Vec3, base: &crate::movement::MoveParams) {
        for k in 0..self.players.len().min(MAX_PLAYERS) {
            if s.hits_left == 0 {
                return;
            }
            let q = &self.players[k];
            if k == i || !q.alive || s.struck.has(q.id) {
                continue;
            }
            let hull = geom::inflate(&q.hull(base), 0.45);
            let Some((f, _)) = geom::segment_aabb(p0, p1, &hull) else { continue };
            let point = p0 + (p1 - p0) * f;
            let tid = q.id;
            s.struck.add(tid);
            let dir = s.dir + Vec3::Y * s.up;
            let hit = HitInfo::new(tid, Some(s.owner), HitKind::Thrown, s.damage, s.knockback, dir, point).with_element(s.element).with_construct(s.cid).with_impact(1.6);
            let landed = self.apply_hit(hit).applied;
            let was_target = s.target == Some(tid);
            if landed {
                s.hits_left = s.hits_left.saturating_sub(1);
                self.op_event(OpEvent::Seek { player: s.owner, phase: SeekPhase::Strike, target: Some(tid), pos: point, dir: s.dir });
            } else {
                self.op_event(OpEvent::Seek { player: s.owner, phase: SeekPhase::Dodged, target: Some(tid), pos: point, dir: s.dir });
            }
            if was_target || landed {
                self.seeker_next(i, s, point);
            }
        }
    }

    /// Bend onto the nearest living, unstruck enemy within `chain` m of `from`, else home.
    fn seeker_next(&mut self, i: usize, s: &mut Seeker, from: Vec3) {
        if s.hits_left == 0 {
            s.target = None;
            if !s.returning {
                s.returning = true;
                self.op_event(OpEvent::Seek { player: s.owner, phase: SeekPhase::Return, target: None, pos: from, dir: s.dir });
            }
            return;
        }
        let base = &self.cfg.move_params;
        let mut best: Option<(usize, f32)> = None;
        for (k, q) in self.players.iter().enumerate().take(MAX_PLAYERS) {
            if k == i || !q.alive || q.shielded() || s.struck.has(q.id) {
                continue;
            }
            let d = (q.hull(base).center() - from).length();
            if d <= s.chain && best.is_none_or(|b| d < b.1) {
                best = Some((k, d));
            }
        }
        match best {
            Some((k, _)) => {
                let t = self.players[k].id;
                s.target = Some(t);
                self.op_event(OpEvent::Seek { player: s.owner, phase: SeekPhase::Chain, target: Some(t), pos: from, dir: s.dir });
            }
            None => {
                s.target = None;
                if !s.returning {
                    s.returning = true;
                    self.op_event(OpEvent::Seek { player: s.owner, phase: SeekPhase::Return, target: None, pos: from, dir: s.dir });
                }
            }
        }
    }
}
