//! `Counter { secs, damage, .. }` — a short stance. The first attack that lands on you inside it is
//! **negated** and its attacker eats `damage` / `knockback` from you if they are within reach
//! ([`REACH`] m: Marth's Counter, a parry, Kenshi's riposte); from further away the hit is simply
//! shrugged. The stance ends on its answer. `damage` 0 = a **guard**: every hit (explosions too)
//! is shrugged for the whole window and nothing is answered — invincible startup (Shoryuken),
//! Wraith Form, a Zhonya's-style stasis flash. Grabs are NOT stopped by either (a grab beats a
//! block); dodges still work as ever.
//!
//! The deflect stance (`deflect.rs`) shares [`super::Stance`] and this hit hook.

use bd_core::mechanics::Element;
use bd_core::Vec3;

use super::{OpEvent, Stance, StanceKind};
use crate::combat::{HitInfo, HitKind};
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_SECS: f32 = 1.5;
/// A counter answers attackers this close (m); further away the hit is only shrugged.
pub const REACH: f32 = 7.0;

impl World {
    pub(crate) fn stance_mut(&mut self, i: usize) -> &mut Stance {
        let id = self.players[i].id;
        let s = self.powers.ops.stances[i].get_or_insert(Stance::new(id));
        if s.owner != id {
            *s = Stance::new(id);
        }
        s
    }

    pub(crate) fn op_counter(&mut self, i: usize, secs: f32, damage: f32, knockback: f32, up: f32, element: Element) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let secs = super::fclamp(secs, 0.05, MAX_SECS);
        let guard = damage <= 0.0 && knockback <= 0.0;
        let s = self.stance_mut(i);
        if guard {
            s.guard = s.guard.max(secs);
        } else {
            s.counter = secs;
            s.c_damage = super::fclamp(damage, 0.0, 60.0);
            s.c_knockback = super::fclamp(knockback, 0.0, 80.0);
            s.c_up = super::fclamp(up, 0.0, 2.0);
            s.c_element = element;
        }
        let kind = if guard { StanceKind::Guard } else { StanceKind::Counter };
        self.op_event(OpEvent::Stance { player: id, kind, secs, angle: 0.0 });
        Ok(())
    }

    pub(crate) fn step_stances(&mut self, dt: f32) {
        for i in 0..self.powers.ops.stances.len() {
            let Some(mut s) = self.powers.ops.stances[i] else { continue };
            let ok = self.players.get(i).is_some_and(|p| p.id == s.owner && p.alive);
            s.deflect = (s.deflect - dt).max(0.0);
            s.counter = (s.counter - dt).max(0.0);
            s.guard = (s.guard - dt).max(0.0);
            let live = ok && (s.deflect > 0.0 || s.counter > 0.0 || s.guard > 0.0);
            self.powers.ops.stances[i] = live.then_some(s);
            if live && s.deflect > 0.0 {
                self.step_deflect(i, s);
            }
        }
    }

    /// Guard / counter / deflect answer a hit on player `ti` (`None` = negated).
    pub(crate) fn stance_on_hit(&mut self, ti: usize, hit: HitInfo) -> Option<HitInfo> {
        let Some(s) = self.powers.ops.stances[ti] else { return Some(hit) };
        let me = &self.players[ti];
        if s.owner != me.id || hit.source == Some(hit.target) || self.powers.ops.unblockable {
            return Some(hit);
        }
        let center = me.hull(&self.cfg.move_params).center();
        let look = me.look();
        let attacker = hit.source.and_then(|a| self.pidx(a));
        if s.guard > 0.0 {
            self.op_event(OpEvent::Parried { player: s.owner, kind: StanceKind::Guard, attacker: hit.source, pos: hit.point, dir: Vec3::ZERO });
            return None;
        }
        // An area blast, a crush or your own wall impact can't be countered.
        let answerable = !matches!(hit.kind, HitKind::Explosion | HitKind::Crush | HitKind::WallSplat);
        if s.counter > 0.0 && answerable && !self.powers.ops.answering {
            if let Some(st) = self.powers.ops.stances[ti].as_mut() {
                st.counter = 0.0;
            }
            let mut dir = hit.impulse_dir.normalize_or(-look) * -1.0;
            if let Some(a) = attacker.filter(|&a| self.players[a].alive) {
                let ac = self.players[a].hull(&self.cfg.move_params).center();
                let d = ac - center;
                dir = d.normalize_or(look);
                if d.length() <= REACH && (s.c_damage > 0.0 || s.c_knockback > 0.0) {
                    let aid = self.players[a].id;
                    let answer = HitInfo::new(aid, Some(s.owner), HitKind::HeavyMelee, s.c_damage, s.c_knockback, dir + Vec3::Y * s.c_up, ac)
                        .with_element(s.c_element)
                        .with_impact(2.0);
                    self.powers.ops.answering = true;
                    self.apply_hit(answer);
                    self.powers.ops.answering = false;
                }
            }
            self.op_event(OpEvent::Parried { player: s.owner, kind: StanceKind::Counter, attacker: hit.source, pos: hit.point, dir });
            return None;
        }
        // Deflect: shots / thrown things / ring blasts from in front are turned aside (the rounds
        // themselves are sent back by the tick scan; this catches hitscan and fast rounds).
        if s.deflect > 0.0 && matches!(hit.kind, HitKind::Shot | HitKind::Thrown | HitKind::RingBlast) {
            let from = attacker.map_or(hit.point - hit.impulse_dir, |a| self.players[a].hull(&self.cfg.move_params).center());
            let to = (from - center).normalize_or(look);
            if to.dot(look) >= s.deflect_cos {
                self.op_event(OpEvent::Parried { player: s.owner, kind: StanceKind::Deflect, attacker: hit.source, pos: hit.point, dir: look });
                return None;
            }
        }
        Some(hit)
    }
}
