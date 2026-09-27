//! `Haul { .. }` — cast a hook and drag what it bites back to you (docs/toolbox/OPS.md): Māui
//! fishing the islands up out of the sea with Manaiakalani, a telekinetic rock-rip, an earthbender
//! pulling a slab to hand.
//!
//! The hook goes out along the aim (`range`):
//! * an enemy under it (crosshair or a small assist cone) is dragged to your feet (`speed`), a
//!   light `damage` on the bite — a set-up, never a lock (their tether breaks, they can dodge on
//!   arrival);
//! * otherwise it bites the terrain it lands on: a chunk (`radius`) is TORN OUT (a real crater) and
//!   comes flying at you as a boulder of earth — every enemy on its line eats `damage` and is
//!   knocked along it — then it thuds down in front of you as fresh cover (an ordinary boulder
//!   from then on: it rolls, blocks, can be grabbed, dissolves like any object).
//!
//! The chunk is a `driven` construct while it flies (constructs/mod.rs), released to physics on
//! arrival. Deterministic, allocation-free per tick; nothing runs unless a move uses it.

use bd_core::mechanics::{Element, Form, Mechanic, ModelSource, Recipe};
use bd_core::{ConstructId, MAX_PLAYERS, PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::seeker::Struck;
use super::{OpEvent, flat};
use crate::combat::{CarveRequest, HitInfo, HitKind};
use crate::constructs::{AnimState, Loco, SpawnParams};
use crate::events::{GameEvent, GoneReason};
use crate::movement::{ColliderId, CollisionWorld, raycast_world};
use crate::powers::PowerError;
use crate::willpower::validate_recipe;
use crate::world::World;

pub const MAX_RANGE: f32 = 60.0;
/// Chunk radius bounds (m).
pub const MIN_RADIUS: f32 = 0.8;
pub const MAX_RADIUS: f32 = 3.5;
/// Arc height of the flight (m).
const ARC: f32 = 3.0;

/// A beat of a haul (presentation: the cast line, the bite, the tear, the thud).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HaulPhase {
    /// The hook bit a player (`target`), who is being dragged in.
    Player,
    /// The hook bit terrain at `from`: the chunk tears loose and flies to `to` over `secs`.
    Tear,
    /// The chunk struck someone on its way.
    Strike,
    /// The chunk landed.
    Land,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Haul {
    pub owner: PlayerId,
    cid: ConstructId,
    from: Vec3,
    to: Vec3,
    t: f32,
    secs: f32,
    damage: f32,
    struck: Struck,
    last: Vec3,
}

impl World {
    pub(crate) fn op_haul(&mut self, i: usize, range: f32, radius: f32, speed: f32, damage: f32) -> Result<(), PowerError> {
        let owner = self.players[i].id;
        let range = if range.is_finite() && range > 0.0 { range.clamp(3.0, MAX_RANGE) } else { 40.0 };
        let radius = if radius.is_finite() && radius > 0.0 { radius.clamp(MIN_RADIUS, MAX_RADIUS) } else { 2.0 };
        let speed = if speed.is_finite() && speed > 0.0 { speed.clamp(8.0, 60.0) } else { 28.0 };
        let damage = if damage.is_finite() { damage.clamp(0.0, 40.0) } else { 0.0 };
        let (eye, look) = self.op_eye_look(i);
        let base = self.cfg.move_params.clone();
        let me = self.players[i].hull(&base).center();
        // A player on the line: drag them in.
        if let Some(k) = self.op_aimed_player(i, range, 6.0) {
            let q = &mut self.players[k];
            let from = q.hull(&base).center();
            let to = (me + flat(look, Vec3::NEG_Z) * 1.8 - from).normalize_or(-look);
            q.mv.release_tethers();
            q.mv.extra.cancel_dodge(&mut q.mv.vel);
            q.mv.vel = to * speed + Vec3::Y * 4.0;
            q.mv.grounded = false;
            q.mv.tumble = q.mv.tumble.max(0.3);
            let tid = q.id;
            self.apply_hit(HitInfo::new(tid, Some(owner), HitKind::RingShove, damage * 0.5, 0.0, to, from));
            self.op_event(OpEvent::Haul { player: owner, phase: HaulPhase::Player, target: Some(tid), from, to: me, secs: 0.0 });
            return Ok(());
        }
        match self.powers.ops.hauls[i] {
            Some(h) if h.owner == owner => return Err(PowerError::Busy),
            // A departed player's chunk in this slot: dropped where it is.
            Some(h) => {
                if let Some(c) = self.constructs.get_mut(h.cid) {
                    c.driven = false;
                }
                self.powers.ops.hauls[i] = None;
            }
            None => {}
        }
        if self.constructs.len() >= crate::world::MAX_CONSTRUCTS - 8 {
            return Err(PowerError::Busy);
        }
        // Terrain only (voxels): constructs are not islands.
        let Some(hit) = bd_core::voxel::raycast(&self.voxels, eye, look, range) else { return Err(PowerError::NoTarget) };
        let n = if hit.normal == bd_core::IVec3::ZERO { -look } else { hit.normal.as_vec3() };
        let bite = hit.point - n * (radius * 0.45);
        self.request_carve(CarveRequest { center: bite, radius, power: 2.5 });
        // Where it lands: in front of you, on your level (the first clear spot along the way back).
        let fl = flat(look, Vec3::NEG_Z);
        let feet = self.players[i].mv.pos;
        let to = {
            let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(ColliderId::Player(owner));
            let want = radius + 2.2;
            let d = raycast_world(&cw, me, fl, want + radius, false).map_or(want, |h| (h.t - radius - 0.2).max(radius * 0.6));
            Vec3::new(me.x + fl.x * d, feet.y + radius * 0.9, me.z + fl.z * d)
        };
        let from = hit.point + n * 0.2;
        let secs = ((to - from).length() / speed).clamp(0.2, 2.5);
        let recipe = validate_recipe(
            Recipe {
                name: "torn earth".into(),
                form: Form::Boulder,
                mechanics: vec![Mechanic::Heavy],
                element: Element::Earth,
                scale: radius * 2.0,
                mass: 200.0 * radius * radius,
                count: 1,
                accent: None,
                model: ModelSource::Procedural { seed: 0x4A01 },
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
        let tick = self.tick;
        let cid = self.constructs.spawn(SpawnParams {
            owner,
            recipe: recipe.clone(),
            shape: None,
            pos: from,
            rot: Quat::IDENTITY,
            vel: Vec3::ZERO,
            target: None,
            cost: 0.0,
            integrity: 120.0 + 40.0 * radius,
            forming: 0.0,
            life: 14.0,
            tick,
            colossal: false,
            model: None,
        });
        if let Some(c) = self.constructs.get_mut(cid) {
            c.parent = Some(cid);
            c.driven = true;
            c.comp.launched = true;
            c.set_anim(AnimState::Move, tick);
        }
        self.inject_event(GameEvent::ConstructSpawned { id: cid, owner, recipe: Box::new(recipe), pos: from, rot: Quat::IDENTITY, forming: 0.0, cost: 0.0 });
        self.powers.ops.hauls[i] = Some(Haul { owner, cid, from, to, t: 0.0, secs, damage, struck: Struck::default(), last: from });
        self.op_event(OpEvent::Haul { player: owner, phase: HaulPhase::Tear, target: None, from, to, secs });
        Ok(())
    }

    pub(crate) fn step_hauls(&mut self, dt: f32) {
        for i in 0..MAX_PLAYERS.min(self.powers.ops.hauls.len()) {
            if let Some(h) = self.powers.ops.hauls[i] {
                let next = self.step_haul(i, h, dt);
                self.powers.ops.hauls[i] = next;
            }
        }
    }

    fn step_haul(&mut self, i: usize, mut h: Haul, dt: f32) -> Option<Haul> {
        let alive = self.constructs.get(h.cid).is_some_and(|c| c.gone.is_none());
        if !alive || !self.players.get(i).is_some_and(|p| p.id == h.owner) {
            if let Some(c) = self.constructs.get_mut(h.cid) {
                c.driven = false;
                if c.gone.is_none() && !self.players.get(i).is_some_and(|p| p.id == h.owner) {
                    c.gone = Some(GoneReason::Spent);
                }
            }
            return None;
        }
        h.t += dt;
        let u = (h.t / h.secs).clamp(0.0, 1.0);
        // Ease-in (the rope takes the strain), then a straight hard pull; a low arc over the ground.
        let e = u * u * (3.0 - 2.0 * u);
        let pos = h.from + (h.to - h.from) * e + Vec3::Y * (ARC * 4.0 * u * (1.0 - u));
        let dir = (pos - h.last).normalize_or((h.to - h.from).normalize_or(Vec3::Y));
        // Strikes on the way.
        let base = self.cfg.move_params.clone();
        let r = self.constructs.get(h.cid).map_or(1.0, |c| c.half.max_element());
        for k in 0..self.players.len().min(MAX_PLAYERS) {
            let q = &self.players[k];
            if k == i || !q.alive || h.struck.has(q.id) {
                continue;
            }
            let hull = crate::geom::inflate(&q.hull(&base), r * 0.8);
            let Some((f, _)) = crate::geom::segment_aabb(h.last, pos, &hull) else { continue };
            h.struck.add(q.id);
            let point = h.last + (pos - h.last) * f;
            let tid = q.id;
            let hit = HitInfo::new(tid, Some(h.owner), HitKind::Thrown, h.damage, 12.0 + h.damage * 0.6, dir + Vec3::Y * 0.3, point).with_element(Element::Earth).with_construct(h.cid).with_impact(1.5);
            if self.apply_hit(hit).applied {
                self.op_event(OpEvent::Haul { player: h.owner, phase: HaulPhase::Strike, target: Some(tid), from: point, to: h.to, secs: 0.0 });
            }
        }
        h.last = pos;
        let done = u >= 1.0;
        if let Some(c) = self.constructs.get_mut(h.cid) {
            c.pos = pos;
            c.prev_pos = pos;
            c.rot = Quat::from_rotation_y(h.t * 2.0) * Quat::from_rotation_x(h.t * 1.3);
            c.vel = dir * ((h.to - h.from).length() / h.secs);
            if done {
                // Handed to physics: a boulder that thuds down and settles as cover.
                c.driven = false;
                c.vel = Vec3::new(dir.x * 2.0, -4.0, dir.z * 2.0);
                c.body.loco = Loco::Ballistic;
                c.body.gravity = 1.0;
            }
        }
        if done {
            self.op_event(OpEvent::Haul { player: h.owner, phase: HaulPhase::Land, target: None, from: pos, to: pos, secs: 0.0 });
            return None;
        }
        Some(h)
    }
}
