//! `Surge { .. }` — a travelling wall front (docs/toolbox/OPS.md): Poseidon's tidal wave, Set's
//! sandstorm, a lava flow, an avalanche, a stampede.
//!
//! A wall of `element` (`width` m wide, two thirds as tall) rises in front of the caster and ROLLS
//! OUT along the flat aim for `dist` m at `speed` m/s, hugging the ground (it rides up and down
//! slopes; a cliff taller than itself breaks it early). Every enemy it overtakes is struck once
//! (`damage`, element status) and CARRIED: while inside the front they ride it at its speed, lifted
//! a little — surfing a tidal wave, not a tap — for at most `MAX_CARRY` s, then the crest throws
//! them clear. `push` is how hard it batters constructs and bursts at
//! the break. Jumping over the crest (feet above it),
//! dodging through it or hanging on a tether escapes. Loose enemy constructs it meets are battered along. At the end it
//! breaks with a splash (a knockback-only burst of its element).
//!
//! The wall is a `driven` construct (constructs/mod.rs) — visible through ordinary snapshots, never
//! solid (the op does the pushing). Deterministic, allocation-free per tick; nothing runs unless a
//! move uses it.

use bd_core::mechanics::{Element, Form, Mechanic, ModelSource, Recipe};
use bd_core::{ConstructId, MAX_PLAYERS, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use super::seeker::Struck;
use super::{OpEvent, fclamp, flat};
use crate::combat::{ConstructHit, Explosion, HitInfo, HitKind};
use crate::constructs::{AnimState, SpawnParams};
use crate::events::{GameEvent, GoneReason};
use crate::geom;
use crate::powers::PowerError;
use crate::willpower::validate_recipe;
use crate::world::World;

pub const MAX_WIDTH: f32 = 12.0;
pub const MAX_DIST: f32 = 60.0;
/// Front thickness (m) that catches people (the wall itself is thin).
const CATCH_DEPTH: f32 = 2.2;
const MAX_BATTERED: usize = 8;
/// Longest one enemy rides the front (s): then it throws them clear (never a lock).
pub const MAX_CARRY: f32 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurgePhase {
    /// Rose at `pos` heading along `dir` (`width`, `speed`, `dist` let clients draw the whole run).
    Start,
    /// Swept `target` up.
    Catch,
    /// Broke (end of the run or against a cliff) at `pos`.
    Break,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Surge {
    pub owner: PlayerId,
    cid: ConstructId,
    /// Feet-centre of the front and its heading (flat).
    pos: Vec3,
    dir: Vec3,
    speed: f32,
    left: f32,
    width: f32,
    height: f32,
    push: f32,
    damage: f32,
    element: Element,
    struck: Struck,
    /// Seconds each rider has been carried, by id.
    carried: [Option<(PlayerId, f32)>; MAX_PLAYERS],
    battered: [Option<ConstructId>; MAX_BATTERED],
}

/// The authored numbers (see `MoveOp::Surge`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurgeSpec {
    pub width: f32,
    pub dist: f32,
    pub speed: f32,
    pub push: f32,
    pub damage: f32,
    pub element: Element,
}

impl World {
    pub(crate) fn op_surge(&mut self, i: usize, spec: SurgeSpec) -> Result<(), PowerError> {
        let owner = self.players[i].id;
        match self.powers.ops.surges[i] {
            Some(s) if s.owner == owner => return Err(PowerError::Busy),
            // A departed player's wave in this slot: it collapses.
            Some(s) => {
                if let Some(c) = self.constructs.get_mut(s.cid) {
                    if c.gone.is_none() {
                        c.gone = Some(GoneReason::Spent);
                    }
                }
                self.powers.ops.surges[i] = None;
            }
            None => {}
        }
        if self.constructs.len() >= crate::world::MAX_CONSTRUCTS - 8 {
            return Err(PowerError::Busy);
        }
        let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() && v > 0.0 { v.clamp(lo, hi) } else { d };
        let width = fin(spec.width, 8.0, 2.0, MAX_WIDTH);
        let speed = fin(spec.speed, 14.0, 3.0, 40.0);
        let dist = fin(spec.dist, 30.0, 4.0, MAX_DIST);
        // Carried at the front's own speed (a little more at most): faster would fling them out of it.
        let push = fin(spec.push, speed, 0.0, 50.0).max(speed);
        let (_, look) = self.op_eye_look(i);
        let dir = flat(look, Vec3::NEG_Z);
        let feet = self.players[i].mv.pos;
        let start = self.surge_ground(feet + dir * 2.5, feet.y, width * 0.667).unwrap_or(feet + dir * 2.5);
        let recipe = validate_recipe(
            Recipe {
                name: match spec.element {
                    Element::Water => "tidal wave".into(),
                    Element::Earth => "sandstorm".into(),
                    Element::Fire => "lava flow".into(),
                    Element::Ice => "avalanche".into(),
                    Element::Wind => "gale front".into(),
                    _ => "surge".into(),
                },
                form: Form::Wall,
                mechanics: vec![Mechanic::Repel],
                element: spec.element,
                scale: width,
                mass: 400.0,
                count: 1,
                accent: None,
                model: ModelSource::Procedural { seed: 0x5E6E },
                payload: None,
                features: vec![],
                grants: None,
                temperament: Default::default(),
                intensity: 0.0,
                mount: None,
                opening: None,
            },
            &self.cfg.rules,
        );
        let rot = geom::yaw_rotation(dir);
        let tick = self.tick;
        let cid = self.constructs.spawn(SpawnParams {
            owner,
            recipe: recipe.clone(),
            shape: None,
            pos: start,
            rot,
            vel: dir * speed,
            target: None,
            cost: 0.0,
            integrity: 600.0,
            forming: 0.0,
            life: dist / speed + 2.0,
            tick,
            colossal: false,
            model: None,
        });
        let height = self.constructs.get(cid).map_or(width * 0.667, |c| c.half.y * 2.0);
        if let Some(c) = self.constructs.get_mut(cid) {
            c.parent = Some(cid);
            c.driven = true;
            c.set_anim(AnimState::Move, tick);
        }
        self.inject_event(GameEvent::ConstructSpawned { id: cid, owner, recipe: Box::new(recipe), pos: start, rot, forming: 0.0, cost: 0.0 });
        self.powers.ops.surges[i] = Some(Surge {
            owner,
            cid,
            pos: start,
            dir,
            speed,
            left: dist,
            width,
            height,
            push,
            damage: fclamp(spec.damage, 0.0, 40.0),
            element: spec.element,
            struck: Struck::default(),
            carried: [None; MAX_PLAYERS],
            battered: [None; MAX_BATTERED],
        });
        self.op_event(OpEvent::Surge { player: owner, phase: SurgePhase::Start, target: None, pos: start, dir, width, speed, dist });
        Ok(())
    }

    /// Ground under `p` (searching from `height` above the reference `y` down), for the wave to ride.
    fn surge_ground(&self, p: Vec3, y: f32, height: f32) -> Option<Vec3> {
        let top = Vec3::new(p.x, y + height.max(2.0), p.z);
        bd_core::voxel::raycast(&self.voxels, top, Vec3::NEG_Y, height.max(2.0) + 8.0).map(|h| Vec3::new(p.x, h.point.y, p.z))
    }

    pub(crate) fn step_surges(&mut self, dt: f32) {
        for i in 0..MAX_PLAYERS.min(self.powers.ops.surges.len()) {
            if let Some(s) = self.powers.ops.surges[i] {
                let next = self.step_surge(i, s, dt);
                self.powers.ops.surges[i] = next;
            }
        }
    }

    fn break_surge(&mut self, s: &Surge) {
        if let Some(c) = self.constructs.get_mut(s.cid) {
            if c.gone.is_none() {
                c.gone = Some(GoneReason::Spent);
            }
        }
        let owner = self.pidx(s.owner).map(|_| s.owner);
        self.explode(Explosion {
            pos: s.pos + s.dir * 1.0 + Vec3::Y * s.height * 0.4,
            radius: s.width * 0.45,
            damage: 0.0,
            knockback: 10.0 + s.push * 0.3,
            source: owner,
            construct: None,
            element: s.element,
            carve_radius: 0.0,
            self_damage: false,
        });
        self.op_event(OpEvent::Surge { player: s.owner, phase: SurgePhase::Break, target: None, pos: s.pos, dir: s.dir, width: s.width, speed: s.speed, dist: 0.0 });
    }

    fn step_surge(&mut self, i: usize, mut s: Surge, dt: f32) -> Option<Surge> {
        let alive = self.constructs.get(s.cid).is_some_and(|c| c.gone.is_none());
        if !alive || !self.players.get(i).is_some_and(|p| p.id == s.owner) {
            if let Some(c) = self.constructs.get_mut(s.cid) {
                if c.gone.is_none() {
                    c.gone = Some(GoneReason::Spent);
                }
            }
            return None;
        }
        let step = s.speed * dt;
        // A cliff taller than the wave in front breaks it.
        let chest = s.pos + Vec3::Y * (s.height * 0.75);
        if bd_core::voxel::raycast(&self.voxels, chest, s.dir, step + 0.6).is_some() {
            self.break_surge(&s);
            return None;
        }
        let next = s.pos + s.dir * step;
        let ground = self.surge_ground(next, s.pos.y, s.height);
        // Over the void / off an edge: it pours on at the same height and breaks at the end.
        s.pos = match ground {
            Some(g) if (g.y - s.pos.y).abs() <= s.height * 0.8 => Vec3::new(next.x, s.pos.y + (g.y - s.pos.y) * (10.0 * dt).min(1.0), next.z),
            _ => next,
        };
        s.left -= step;
        // Sweep: players in the front box (width × height × CATCH_DEPTH, centred just ahead).
        let base = self.cfg.move_params.clone();
        let right = Vec3::new(-s.dir.z, 0.0, s.dir.x);
        let mid = s.pos + s.dir * (CATCH_DEPTH * 0.3);
        for k in 0..self.players.len().min(MAX_PLAYERS) {
            let q = &self.players[k];
            if k == i || !q.alive || q.shielded() || q.seat.is_some() {
                continue;
            }
            let c = q.hull(&base).center();
            let d = c - mid;
            let feet = q.mv.pos.y;
            let inside = d.dot(s.dir).abs() <= CATCH_DEPTH * 0.5 + 0.4 && d.dot(right).abs() <= s.width * 0.5 + 0.4 && feet <= s.pos.y + s.height && c.y >= s.pos.y - 1.0;
            // A dodge through it, or a tether holding them, and it rolls past.
            if !inside || q.mv.extra.dodging() || q.mv.tethered() {
                continue;
            }
            let tid = q.id;
            // Ridden long enough: the front throws them clear and lets go.
            let ride = s.carried.iter().flatten().find(|(id, _)| *id == tid).map_or(0.0, |r| r.1);
            if ride >= MAX_CARRY {
                continue;
            }
            match s.carried.iter_mut().find(|r| r.is_some_and(|(id, _)| id == tid)) {
                Some(r) => *r = Some((tid, ride + dt)),
                None => {
                    if let Some(r) = s.carried.iter_mut().find(|r| r.is_none()) {
                        *r = Some((tid, dt));
                    }
                }
            }
            if !s.struck.has(tid) {
                s.struck.add(tid);
                let hit = HitInfo::new(tid, Some(s.owner), HitKind::Melee, s.damage, 6.0, s.dir + Vec3::Y * 0.4, c).with_element(s.element).with_construct(s.cid).with_impact(1.2);
                self.apply_hit(hit);
                self.op_event(OpEvent::Surge { player: s.owner, phase: SurgePhase::Catch, target: Some(tid), pos: c, dir: s.dir, width: s.width, speed: s.speed, dist: 0.0 });
            }
            // Carried: at least the wave's speed along it, a little lift, no footing.
            let q = &mut self.players[k];
            if !q.alive {
                continue;
            }
            // Riding the front: exactly its speed along it (never flung out ahead of it).
            let along = q.mv.vel.dot(s.dir);
            q.mv.vel += s.dir * (s.speed - along);
            q.mv.vel.y = q.mv.vel.y.max(2.5);
            q.mv.grounded = false;
            if ride == 0.0 {
                q.mv.tumble = q.mv.tumble.max(0.15);
            }
            if ride + dt >= MAX_CARRY {
                // Thrown clear off the crest.
                q.mv.vel += s.dir * 4.0 + Vec3::Y * 6.0;
            }
        }
        // Loose enemy constructs in the front are battered along once.
        let mut found = [None; MAX_BATTERED];
        let mut n = 0;
        for c in self.constructs.iter() {
            if n >= MAX_BATTERED {
                break;
            }
            if c.id == s.cid || c.owner == s.owner || c.gone.is_some() || c.attached() || c.driven || s.battered.contains(&Some(c.id)) {
                continue;
            }
            let d = c.center() - mid;
            if d.dot(s.dir).abs() <= CATCH_DEPTH && d.dot(right).abs() <= s.width * 0.5 && d.y <= s.height && d.y >= -1.5 {
                found[n] = Some(c.id);
                n += 1;
            }
        }
        for cid in found.iter().flatten().copied() {
            if let Some(slot) = s.battered.iter_mut().find(|b| b.is_none()) {
                *slot = Some(cid);
            }
            self.apply_construct_hit(ConstructHit { pierce: false, blunt: true, target: cid, source: Some(s.owner), damage: 8.0 + s.damage, impulse: s.dir * (s.push + 6.0) + Vec3::Y * 3.0, blade: false, point: mid });
        }
        if let Some(c) = self.constructs.get_mut(s.cid) {
            c.pos = s.pos;
            c.prev_pos = s.pos;
            c.vel = s.dir * s.speed;
        }
        if s.left <= 0.0 {
            self.break_surge(&s);
            return None;
        }
        Some(s)
    }
}
