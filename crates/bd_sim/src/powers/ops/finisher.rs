//! `MoveOp::Finisher` (docs/toolbox/OPS.md): a lunge + front-cone strike that CRITS on every target
//! YOU hit within the last `window` seconds — the combo-timing crit.
//!
//! Black Flash is the showcase: thrown cold it is a good punch; thrown right after a connect
//! (a jab, a clap-swap, a pull, another Black Flash) the space around the fist bends and it hits
//! × `mult` with black-red lightning. Chains ("divergent fist -> black flash -> black flash") are
//! how you land it on purpose. The crit is announced as `PowerEvent::Crit` with the move's `fx`.
//! Deterministic, allocation-free.

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, TICK_HZ, Tick, Vec3};

use super::super::{CritFx, PowerError, PowerEvent};
use crate::combat::{ConstructHit, HitInfo, HitKind};
use crate::events::GameEvent;
use crate::geom;
use crate::world::World;

/// A finisher's numbers (`MoveOp::Finisher`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct FinisherOp {
    pub lunge: f32,
    pub range: f32,
    pub angle: f32,
    pub damage: f32,
    pub knockback: f32,
    pub element: Element,
    pub up: f32,
    pub window: f32,
    pub mult: f32,
    pub fx: CritFx,
}

impl World {
    pub(crate) fn op_finisher(&mut self, i: usize, op: FinisherOp) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.op_eye_look(i);
        let flat = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
        if op.lunge != 0.0 {
            self.players[i].mv.apply_impulse(flat * op.lunge.clamp(-40.0, 40.0), 0.0);
        }
        let range = op.range.clamp(0.5, 12.0);
        let cos = op.angle.clamp(1.0, 179.0).to_radians().cos();
        let origin = eye + look * 0.4;
        let window = (op.window.clamp(0.0, 3.0) * TICK_HZ as f32) as Tick;
        let mult = op.mult.clamp(1.0, 4.0);
        let base = self.cfg.move_params.clone();
        let in_cone = |p: Vec3| {
            let d = p - origin;
            let len = d.length();
            len <= range && (len < 0.6 || d.dot(look) / len >= cos)
        };
        let tick = self.tick;
        let mut targets = [None; MAX_PLAYERS];
        let mut n = 0;
        for q in &self.players {
            if q.id == id || !q.alive || n >= targets.len() {
                continue;
            }
            let h = q.hull(&base);
            if in_cone(geom::closest_point(&h, origin + look * range.min(1.5))) {
                let combo = q.last_attacker.is_some_and(|(s, t)| s == id && tick.saturating_sub(t) <= window);
                targets[n] = Some((q.id, h.center(), combo));
                n += 1;
            }
        }
        let damage = op.damage.clamp(0.0, 100.0);
        let knockback = op.knockback.clamp(0.0, 80.0);
        for (tid, c, combo) in targets.iter().flatten().copied() {
            let push = (look + (c - origin).normalize_or_zero()) * 0.5 + Vec3::Y * op.up.clamp(0.0, 2.0);
            let (dm, km) = if combo { (mult, 1.0 + (mult - 1.0) * 0.5) } else { (1.0, 1.0) };
            let mut hit = HitInfo::new(tid, Some(id), HitKind::Melee, damage * dm, knockback * km, push, c).with_element(op.element).with_impact(if combo { 1.0 + mult * 0.5 } else { 1.2 });
            hit.crit = combo;
            if self.apply_hit(hit).applied && combo {
                self.inject_event(GameEvent::Power(PowerEvent::Crit { attacker: id, target: tid, point: c, fx: op.fx, mult }));
            }
        }
        // Enemy constructs in the cone take the plain hit.
        let mut chits = [None; 8];
        let mut m = 0;
        for c in self.constructs.iter() {
            if m >= chits.len() {
                break;
            }
            if c.owner != id && c.active() && !c.attached() && c.parent != Some(c.id) && in_cone(c.center()) {
                chits[m] = Some(c.id);
                m += 1;
            }
        }
        if damage > 0.0 {
            for cid in chits.iter().flatten().copied() {
                self.apply_construct_hit(ConstructHit { pierce: false, blunt: true, target: cid, source: Some(id), damage, impulse: look * knockback, blade: false, point: origin + look });
            }
        }
        Ok(())
    }
}
