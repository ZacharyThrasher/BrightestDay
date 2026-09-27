//! `MoveOp::Tag` / `MoveOp::TagStrike` (docs/toolbox/OPS.md): stick a tag on the aimed enemy, then
//! later hit — or reel in — EVERY enemy you tagged, wherever they are.
//!
//! Resonance (the nail, then the straw doll: hurts whoever you tagged from across the map) and
//! Bungee Gum (it sticks; later it snaps back and drags them to you) are the showcases. Tags live
//! `secs` (≤ 12 s), are per owner, and die with their target; a strike consumes them. `Transpose`
//! also swaps with your tagged enemy when nothing is aimed. Deterministic, allocation-free (a fixed
//! table in `OpState::tags`, only touched when a Tag / TagStrike runs; expiry is a tick stamp, so
//! nothing steps per tick).

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, PlayerId, TICK_HZ, Tick, Vec3};

use super::super::{PowerError, PowerEvent};
use super::OpEvent;
use crate::combat::{HitInfo, HitKind};
use crate::events::GameEvent;
use crate::world::World;

/// Most live tags in the world (the oldest is dropped beyond it).
pub const MAX_TAGS: usize = 32;

/// One tag: `owner` stuck it on `target`; it holds until tick `until`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tag {
    pub owner: PlayerId,
    pub target: PlayerId,
    pub until: Tick,
}

/// A strike's numbers (`MoveOp::TagStrike`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TagStrikeOp {
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub element: Element,
    pub pull: f32,
}

impl World {
    /// Player indices of the living enemies `owner` has a live tag on (oldest first).
    pub(crate) fn powers_tagged_by(&self, owner: PlayerId) -> impl Iterator<Item = usize> + '_ {
        let tick = self.tick;
        self.powers.ops.tags.iter().flatten().filter(move |t| t.owner == owner && t.until >= tick).filter_map(move |t| self.pidx(t.target).filter(|&k| self.players[k].alive))
    }

    pub(crate) fn op_tag(&mut self, i: usize, range: f32, cone: f32, secs: f32) -> Result<(), PowerError> {
        let Some(k) = self.op_aimed_player(i, range.clamp(1.0, 60.0), cone.clamp(0.0, 20.0)) else { return Err(PowerError::NoTarget) };
        let owner = self.players[i].id;
        let target = self.players[k].id;
        let secs = secs.clamp(0.5, 12.0);
        let tick = self.tick;
        let until = tick + (secs * TICK_HZ as f32) as Tick;
        // Same owner + target: refresh; else a free / expired slot; else the one ending soonest.
        let tags = &mut self.powers.ops.tags;
        let slot = tags
            .iter()
            .position(|t| t.is_some_and(|t| t.owner == owner && t.target == target))
            .or_else(|| tags.iter().position(|t| t.is_none_or(|t| t.until < tick)))
            .unwrap_or_else(|| tags.iter().enumerate().min_by_key(|(_, t)| t.map_or(0, |t| t.until)).map_or(0, |(k, _)| k));
        tags[slot] = Some(Tag { owner, target, until });
        // Credit (zero damage): the tag is a connect — it opens combo windows and names your target.
        let c = self.players[k].hull(&self.cfg.move_params).center();
        self.apply_hit(HitInfo::new(target, Some(owner), HitKind::RingShove, 0.0, 0.0, Vec3::Y, c));
        self.op_event(OpEvent::Tagged { player: owner, target, secs, strike: false });
        Ok(())
    }

    pub(crate) fn op_tag_strike(&mut self, i: usize, op: TagStrikeOp) -> Result<(), PowerError> {
        let owner = self.players[i].id;
        let mut hit = [None::<usize>; MAX_PLAYERS];
        let mut n = 0;
        for k in self.powers_tagged_by(owner) {
            if n < hit.len() && k != i && !hit[..n].contains(&Some(k)) {
                hit[n] = Some(k);
                n += 1;
            }
        }
        if n == 0 {
            return Err(PowerError::NoTarget);
        }
        for t in self.powers.ops.tags.iter_mut() {
            if t.is_some_and(|t| t.owner == owner) {
                *t = None;
            }
        }
        let base = self.cfg.move_params.clone();
        let me = self.players[i].hull(&base).center();
        let damage = op.damage.clamp(0.0, 60.0);
        let knockback = op.knockback.clamp(0.0, 60.0);
        let pull = op.pull.clamp(0.0, 50.0);
        for k in hit.iter().flatten().copied() {
            let target = self.players[k].id;
            let c = self.players[k].hull(&base).center();
            let away = (c - me).normalize_or(Vec3::Y);
            let dir = if pull > 0.0 { -away } else { away } + Vec3::Y * op.up.clamp(0.0, 2.0);
            if damage > 0.0 || knockback > 0.0 {
                let h = HitInfo::new(target, Some(owner), HitKind::Chain, damage, knockback, dir, c).with_element(op.element).with_impact(1.2);
                self.apply_hit(h);
            }
            if pull > 0.0 {
                let q = &mut self.players[k];
                if q.alive {
                    let from = q.hull(&base).center();
                    let to = (me - from).normalize_or(-away);
                    q.mv.release_tethers();
                    q.mv.extra.cancel_dodge(&mut q.mv.vel);
                    q.mv.vel = to * pull + Vec3::Y * 4.0;
                    q.mv.grounded = false;
                    q.mv.tumble = q.mv.tumble.max(0.25);
                    self.inject_event(GameEvent::Power(PowerEvent::Yanked { player: owner, target: Some(target), construct: None, from }));
                }
            }
            self.op_event(OpEvent::Tagged { player: owner, target, secs: 0.0, strike: true });
        }
        Ok(())
    }
}
