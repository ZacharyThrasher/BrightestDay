//! `LockOn { secs, range, damage, .. }` — a telegraphed channel (Cassidy's Deadeye, a sniper's
//! focus, Soldier's visor read as a burst): everyone hears it start. For up to `secs` you move at a
//! walk while every enemy inside your view cone (`angle`°, default 35) and in the clear builds a
//! mark; the mark is worth more the longer they stayed in your sights (full at 80 % of `secs`).
//! Pull the trigger (primary fire) to shoot every marked target at once — early = less — or it
//! fires itself at the end. A hard hit or a stun on you **breaks** it (nothing fires): the
//! counterplay is to hit the gunslinger, break line of sight, or dodge the final shot.

use bd_core::input::buttons;
use bd_core::{MAX_PLAYERS, PlayerId, Vec3};

use super::{LockPhase, OpEvent};
use crate::combat::{HitInfo, HitKind};
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_SECS: f32 = 6.0;
/// Walking speed cap while channelling (m/s).
const WALK: f32 = 2.8;
/// The trigger can't be pulled before this (s): no instant snap shots.
const MIN_FIRE: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Channel {
    pub owner: PlayerId,
    secs: f32,
    range: f32,
    damage: f32,
    knockback: f32,
    cos: f32,
    t: f32,
    ids: [Option<PlayerId>; MAX_PLAYERS],
    marks: [f32; MAX_PLAYERS],
    /// Marks already announced (bit 0: half, bit 1: full) per player index.
    told: [u8; MAX_PLAYERS],
}

impl World {
    pub(crate) fn op_lock_on(&mut self, i: usize, secs: f32, range: f32, damage: f32, knockback: f32, angle: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        if self.powers.ops.lock_ons[i].is_some_and(|c| c.owner == id) {
            return Err(PowerError::Busy);
        }
        let secs = super::fclamp(secs, 0.3, MAX_SECS);
        let angle = if angle <= 0.0 { 35.0 } else { super::fclamp(angle, 5.0, 80.0) };
        let c = Channel {
            owner: id,
            secs,
            range: super::fclamp(range, 5.0, 80.0),
            damage: super::fclamp(damage, 0.0, 120.0),
            knockback: super::fclamp(knockback, 0.0, 80.0),
            cos: angle.to_radians().cos(),
            t: 0.0,
            ids: [None; MAX_PLAYERS],
            marks: [0.0; MAX_PLAYERS],
            told: [0; MAX_PLAYERS],
        };
        self.powers.ops.lock_ons[i] = Some(c);
        self.op_event(OpEvent::LockOn { player: id, phase: LockPhase::Start, target: None, frac: 0.0, secs, hits: 0 });
        Ok(())
    }

    /// Player `i` was hit hard / stunned: their channel breaks (nothing fires).
    pub(crate) fn break_lock_on(&mut self, i: usize) {
        let owner = self.players.get(i).map(|p| p.id);
        if let Some(c) = self.powers.ops.lock_ons.get_mut(i).filter(|c| c.is_some_and(|c| Some(c.owner) == owner)).and_then(|c| c.take()) {
            self.op_event(OpEvent::LockOn { player: c.owner, phase: LockPhase::Broken, target: None, frac: 0.0, secs: 0.0, hits: 0 });
        }
    }

    pub(crate) fn step_lock_ons(&mut self, dt: f32) {
        let base = self.cfg.move_params.clone();
        for i in 0..self.powers.ops.lock_ons.len() {
            let Some(mut c) = self.powers.ops.lock_ons[i] else { continue };
            let ok = self.players.get(i).is_some_and(|p| p.id == c.owner && p.alive);
            if !ok {
                self.powers.ops.lock_ons[i] = None;
                continue;
            }
            if self.players[i].status.stunned() {
                self.break_lock_on(i);
                continue;
            }
            c.t += dt;
            let p = &self.players[i];
            let (eye, look) = (p.eye(&base), p.look());
            let trigger = p.pressed & buttons::PRIMARY != 0 && c.t >= MIN_FIRE;
            let full = c.secs * 0.8;
            let mut tell = [None::<(PlayerId, f32)>; MAX_PLAYERS];
            for (k, q) in self.players.iter().enumerate().take(MAX_PLAYERS) {
                if k == i || !q.alive {
                    continue;
                }
                if c.ids[k] != Some(q.id) {
                    c.ids[k] = Some(q.id);
                    c.marks[k] = 0.0;
                    c.told[k] = 0;
                }
                let d = q.hull(&base).center() - eye;
                let len = d.length();
                if len > c.range || len < 0.5 || d.dot(look) / len < c.cos {
                    continue;
                }
                if bd_core::voxel::raycast(&self.voxels, eye, d / len, len - 0.4).is_some() {
                    continue;
                }
                c.marks[k] += dt;
                let frac = (c.marks[k] / full).min(1.0);
                let bit = if frac >= 1.0 { 2 } else if frac >= 0.5 { 1 } else { 0 };
                if bit != 0 && c.told[k] & bit == 0 {
                    c.told[k] |= bit;
                    tell[k] = Some((q.id, frac));
                }
            }
            // Walk while you aim.
            let mv = &mut self.players[i].mv;
            let h = Vec3::new(mv.vel.x, 0.0, mv.vel.z);
            if h.length() > WALK {
                let k = WALK / h.length();
                mv.vel.x *= k;
                mv.vel.z *= k;
            }
            for (tid, frac) in tell.iter().flatten().copied() {
                self.op_event(OpEvent::LockOn { player: c.owner, phase: LockPhase::Mark, target: Some(tid), frac, secs: c.secs, hits: 0 });
            }
            if !(trigger || c.t >= c.secs) {
                self.powers.ops.lock_ons[i] = Some(c);
                continue;
            }
            // FIRE: every marked target still in the clear.
            self.powers.ops.lock_ons[i] = None;
            let mut hits = 0u8;
            for k in 0..MAX_PLAYERS.min(self.players.len()) {
                let Some(tid) = c.ids[k] else { continue };
                if c.marks[k] < 0.05 || self.players[k].id != tid || !self.players[k].alive {
                    continue;
                }
                let tc = self.players[k].hull(&base).center();
                let d = tc - eye;
                let len = d.length();
                if len < 0.1 || bd_core::voxel::raycast(&self.voxels, eye, d / len, len - 0.4).is_some() {
                    continue;
                }
                let frac = (c.marks[k] / full).min(1.0);
                // Worth little early, everything when full (the famous "it's ready" moment).
                let w = frac * frac * (3.0 - 2.0 * frac);
                let hit = HitInfo::new(tid, Some(c.owner), HitKind::Shot, c.damage * w, c.knockback * w, d / len + Vec3::Y * 0.2, tc).with_impact(1.0 + w);
                if self.apply_hit(hit).applied {
                    hits = hits.saturating_add(1);
                }
            }
            self.op_event(OpEvent::LockOn { player: c.owner, phase: LockPhase::Fire, target: None, frac: 1.0, secs: c.t, hits });
        }
    }
}
