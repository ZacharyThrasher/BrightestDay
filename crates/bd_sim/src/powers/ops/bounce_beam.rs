//! `MoveOp::BounceBeam` — a trick-shot beam (docs/toolbox/OPS.md).
//!
//! An instant beam leaves your eyes / hand along the aim and REFLECTS off terrain (and solid
//! constructs) up to `bounces` times, each leg up to `length` m. Every enemy it crosses (`radius` m
//! thick) is struck once — `damage`, `knockback` along the leg, the `element`'s status — and every leg
//! after a bounce hits a little harder (the bank shot is the skill: Cyclops ricocheting an optic
//! blast around a corner). The first enemy construct on each leg is struck too; the beam goes on
//! (it is concussive, it doesn't carve). One `OpEvent::Zap` per leg draws it. Deterministic,
//! allocation-free; does nothing unless a move uses it.

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, Vec3};

use super::OpEvent;
use super::super::PowerError;
use crate::combat::{ConstructHit, HitInfo, HitKind};
use crate::geom;
use crate::movement::{ColliderId, CollisionWorld, raycast_world};
use crate::world::World;

/// Most reflections one beam makes.
pub const MAX_BOUNCES: u8 = 6;
/// Damage / knockback bonus per reflection before the hit (bank shots pay).
pub const BOUNCE_BONUS: f32 = 0.2;

/// `MoveOp::BounceBeam`'s numbers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BounceBeamArgs {
    pub length: f32,
    pub bounces: u8,
    pub radius: f32,
    pub damage: f32,
    pub knockback: f32,
    pub element: Element,
}

impl World {
    pub(crate) fn op_bounce_beam(&mut self, i: usize, a: BounceBeamArgs) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.op_eye_look(i);
        let base = self.cfg.move_params.clone();
        let fin = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
        let len = fin(a.length, 1.0, 60.0);
        let radius = fin(a.radius, 0.1, 2.0);
        let damage = fin(a.damage, 0.0, 60.0);
        let knockback = fin(a.knockback, 0.0, 60.0);
        let mut done = [false; MAX_PLAYERS];
        let mut p = eye + look * 0.3;
        let mut d = look.normalize_or(Vec3::NEG_Z);
        for leg in 0..=a.bounces.min(MAX_BOUNCES) {
            let hit = {
                let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(ColliderId::Player(id));
                raycast_world(&cw, p, d, len, false)
            };
            let end = hit.map_or(p + d * len, |h| h.point);
            let mult = 1.0 + BOUNCE_BONUS * leg as f32;
            // Enemies along this leg (each once per beam).
            let mut struck = [None; MAX_PLAYERS];
            for (k, q) in self.players.iter().enumerate() {
                if k == i || k >= MAX_PLAYERS || !q.alive || done[k] {
                    continue;
                }
                let hull = geom::inflate(&q.hull(&base), radius);
                if geom::segment_aabb(p, end, &hull).is_some() {
                    done[k] = true;
                    struck[k] = Some((q.id, q.hull(&base).center()));
                }
            }
            for (tid, c) in struck.iter().flatten().copied() {
                let push = Vec3::new(d.x, d.y.max(0.0) + 0.2, d.z);
                self.apply_hit(HitInfo::new(tid, Some(id), HitKind::Shot, damage * mult, knockback * mult, push, c).with_element(a.element).with_impact(if damage > 10.0 { 1.2 } else { 0.8 }));
            }
            // The first enemy construct on this leg.
            let ch = self.constructs.segment_query(p, end, radius, None).filter(|(c, _, _)| self.constructs.get(*c).is_some_and(|c| c.owner != id && !c.attached()));
            if let Some((cid, _, _)) = ch {
                if damage > 0.0 {
                    let point = self.constructs.get(cid).map_or(end, |c| c.center());
                    self.apply_construct_hit(ConstructHit { pierce: false, blunt: true, target: cid, source: Some(id), damage: damage * mult, impulse: d * knockback * mult, blade: false, point });
                }
            }
            self.op_event(OpEvent::Zap { player: id, from: p, to: end, element: a.element, target: None });
            let Some(h) = hit else { break };
            let n = h.normal.normalize_or(-d);
            let r = d - n * (2.0 * d.dot(n));
            d = r.normalize_or(-d);
            p = h.point + n * 0.08;
        }
        Ok(())
    }
}
