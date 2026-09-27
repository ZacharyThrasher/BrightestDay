//! `Pounce` — a homing lunge (docs/toolbox/OPS.md): you fly at the enemy nearest your crosshair,
//! strike on contact and bounce up off them, ending airborne so it chains into itself or into an
//! aerial. Sonic's homing attack, Dante's Stinger, the Halo energy-sword lunge, a biotic charge, a
//! Spartan charge — same verb, different numbers / bodies / styles.
//!
//! Control: the target is picked once at the start (crosshair ray, else the nearest enemy inside
//! the `cone` assist); the launch is ONE velocity set (host and prediction agree on everything but
//! the contact, exactly like a Reel), aimed straight at their centre with a little lift for the
//! flight time. No target = a straight dash along the look that strikes the first enemy met.
//! A stun or KO, or `secs`, ends it; a dodge sees it coming (the strike is `Melee`).

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use super::{OpEvent, flat};
use crate::combat::{HitInfo, HitKind};
use crate::powers::PowerError;
use crate::world::World;

/// Longest a pounce flies (s).
pub const MAX_SECS: f32 = 1.2;
pub const MIN_SPEED: f32 = 8.0;
pub const MAX_SPEED: f32 = 60.0;
/// Hull-to-hull distance that counts as contact (m).
const REACH: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PouncePhase {
    Start,
    /// Struck `target` (the bounce follows).
    Hit,
    /// Flew out its time without touching anyone.
    Whiff,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PounceSpec {
    pub range: f32,
    pub speed: f32,
    pub cone: f32,
    pub damage: f32,
    pub knockback: f32,
    pub element: Element,
    pub up: f32,
    pub bounce: f32,
    pub secs: f32,
}

/// A lunge in flight (`OpState::pounces`, indexed by player index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pounce {
    pub owner: PlayerId,
    pub target: Option<PlayerId>,
    pub t: f32,
    pub secs: f32,
    pub dir: Vec3,
    pub spec: PounceSpec,
}

/// How long a pounce with this spec is in its active phase at most (chains wait on it).
pub fn active_secs(range: f32, speed: f32, secs: f32) -> f32 {
    let speed = speed.clamp(MIN_SPEED, MAX_SPEED);
    let cap = if secs > 0.0 { secs } else { range.clamp(1.0, 40.0) / speed + 0.2 };
    cap.clamp(0.1, MAX_SECS)
}

impl World {
    pub(crate) fn op_pounce(&mut self, i: usize, s: PounceSpec) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (_, look) = self.op_eye_look(i);
        let base = self.cfg.move_params.clone();
        let me = self.players[i].hull(&base).center();
        let speed = if s.speed.is_finite() { s.speed.clamp(MIN_SPEED, MAX_SPEED) } else { 30.0 };
        let range = if s.range.is_finite() { s.range.clamp(1.0, 40.0) } else { 14.0 };
        let target = self.op_aimed_player(i, range, s.cone);
        let (dir, flight, tid) = match target {
            Some(k) => {
                let c = self.players[k].hull(&base).center();
                let d = c - me;
                let len = d.length().max(0.1);
                (d / len, len / speed, Some(self.players[k].id))
            }
            None => {
                let d = Vec3::new(look.x, look.y.clamp(-0.6, 0.6), look.z).normalize_or(flat(look, Vec3::NEG_Z));
                (d, range / speed, None)
            }
        };
        let cap = if s.secs > 0.0 { s.secs.clamp(0.1, MAX_SECS) } else { MAX_SECS };
        let secs = (flight + 0.2).min(cap).max(0.1);
        // Aim straight at them; lift for the flight time so gravity doesn't drop the lunge short.
        let lift = (base.gravity.abs() * flight.min(0.8) * 0.5).min(12.0);
        {
            let mv = &mut self.players[i].mv;
            mv.vel = dir * speed + Vec3::Y * lift;
            mv.grounded = false;
            mv.tumble = 0.0;
        }
        self.powers.ops.pounces[i] = Some(Pounce { owner: id, target: tid, t: 0.0, secs, dir, spec: s });
        self.op_event(OpEvent::Pounce { player: id, phase: PouncePhase::Start, target: tid, pos: me, dir });
        Ok(())
    }

    pub(crate) fn step_pounces(&mut self, dt: f32) {
        let base = self.cfg.move_params.clone();
        for i in 0..MAX_PLAYERS.min(self.players.len()) {
            let Some(mut pc) = self.powers.ops.pounces[i] else { continue };
            let p = &self.players[i];
            if p.id != pc.owner || !p.alive || p.status.stunned() {
                self.powers.ops.pounces[i] = None;
                continue;
            }
            pc.t += dt;
            let my = p.hull(&base);
            let me = my.center();
            // Contact: the chosen target first, else the first enemy met on the way.
            let mut hit: Option<usize> = None;
            for (k, q) in self.players.iter().enumerate() {
                if k == i || !q.alive {
                    continue;
                }
                let h = q.hull(&base);
                let gap = crate::geom::closest_point(&h, me).distance(crate::geom::closest_point(&my, h.center()));
                if gap <= REACH {
                    let preferred = pc.target == Some(q.id);
                    if hit.is_none() || preferred {
                        hit = Some(k);
                    }
                    if preferred {
                        break;
                    }
                }
            }
            if let Some(k) = hit {
                self.powers.ops.pounces[i] = None;
                let s = pc.spec;
                let tid = self.players[k].id;
                let c = self.players[k].hull(&base).center();
                let push = (c - me).normalize_or(pc.dir) + Vec3::Y * s.up.clamp(0.0, 2.0);
                let dmg = if s.damage.is_finite() { s.damage.clamp(0.0, 60.0) } else { 0.0 };
                let kb = if s.knockback.is_finite() { s.knockback.clamp(0.0, 60.0) } else { 0.0 };
                let h = HitInfo::new(tid, Some(pc.owner), HitKind::Melee, dmg, kb, push, c).with_element(s.element).with_impact(1.4);
                self.apply_hit(h);
                let mv = &mut self.players[i].mv;
                if s.bounce > 0.0 {
                    // Off them and up: airborne, facing them — the next homing attack is ready.
                    mv.vel = -flat(pc.dir, Vec3::NEG_Z) * 2.5 + Vec3::Y * s.bounce.min(24.0);
                    mv.grounded = false;
                } else {
                    // Stop where you struck (a thrust, not a bounce).
                    mv.vel *= 0.2;
                }
                self.op_event(OpEvent::Pounce { player: pc.owner, phase: PouncePhase::Hit, target: Some(tid), pos: c, dir: pc.dir });
                continue;
            }
            if pc.t >= pc.secs {
                self.powers.ops.pounces[i] = None;
                self.op_event(OpEvent::Pounce { player: pc.owner, phase: PouncePhase::Whiff, target: None, pos: me, dir: pc.dir });
                continue;
            }
            self.powers.ops.pounces[i] = Some(pc);
        }
    }
}
