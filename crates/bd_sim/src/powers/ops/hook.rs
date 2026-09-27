//! `Hook { .. }` — a visible hook / spear / dart that TRAVELS (`speed` m/s, up to `range` m): a
//! skillshot the victim can see coming and dodge through, unlike `Yank`'s instant grab. The first
//! enemy it meets takes `damage` (+ `status` for `secs`); with `pull` > 0 they are towed to a stop
//! right in front of you (Scorpion's "get over here", Roadhog, Blitzcrank, Pudge, Thresh) — a
//! short tow they can break with a dodge. An enemy construct in the way eats the hook (a minion
//! block). With `anchor`, a hook that bites terrain reels YOU there instead (a grapple).
//! With `pull` = 0 it is a dart (Ana's sleep dart, Sova's bolt, a kunai): the hit and the status.

use bd_core::{PlayerId, Vec3};

use super::{HookPhase, OpEvent, flat};
use crate::combat::{ConstructHit, HitInfo, HitKind};
use crate::powers::PowerError;
use crate::status::StatusKind;
use crate::world::World;

pub const MAX_RANGE: f32 = 50.0;
pub const MIN_SPEED: f32 = 10.0;
pub const MAX_SPEED: f32 = 140.0;
/// Longest tow / reel (s) — the caught player is in front of you by then or let go.
pub const TOW_SECS: f32 = 0.55;
/// Hook head radius (m) against player hulls.
const HEAD: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HookSpec {
    pub range: f32,
    pub speed: f32,
    pub pull: f32,
    pub damage: f32,
    pub status: Option<StatusKind>,
    pub secs: f32,
    pub anchor: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Flying,
    Towing { target: PlayerId, t: f32 },
    Reeling { point: Vec3, t: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HookLine {
    pub owner: PlayerId,
    spec: HookSpec,
    pos: Vec3,
    dir: Vec3,
    flown: f32,
    phase: Phase,
}

impl World {
    pub(crate) fn op_hook(&mut self, i: usize, spec: HookSpec) -> Result<(), PowerError> {
        let id = self.players[i].id;
        if self.powers.ops.hooks[i].is_some_and(|h| h.owner == id) {
            return Err(PowerError::Busy);
        }
        let spec = HookSpec {
            range: spec.range.clamp(1.0, MAX_RANGE),
            speed: spec.speed.clamp(MIN_SPEED, MAX_SPEED),
            pull: spec.pull.clamp(0.0, 45.0),
            damage: spec.damage.clamp(0.0, 40.0),
            secs: spec.secs.clamp(0.0, 1.2),
            ..spec
        };
        let (eye, look) = self.op_eye_look(i);
        let dir = look.normalize_or(Vec3::NEG_Z);
        let pos = eye + dir * 0.4;
        self.powers.ops.hooks[i] = Some(HookLine { owner: id, spec, pos, dir, flown: 0.0, phase: Phase::Flying });
        self.op_event(OpEvent::Hook { player: id, phase: HookPhase::Fired, target: None, pos, dir, speed: spec.speed, range: spec.range });
        Ok(())
    }

    pub(crate) fn step_hooks(&mut self, dt: f32) {
        for i in 0..self.powers.ops.hooks.len() {
            if let Some(h) = self.powers.ops.hooks[i] {
                let next = self.step_hook(i, h, dt);
                self.powers.ops.hooks[i] = next;
            }
        }
    }

    fn hook_end(&mut self, h: &HookLine, pos: Vec3, target: Option<PlayerId>) -> Option<HookLine> {
        self.op_event(OpEvent::Hook { player: h.owner, phase: HookPhase::End, target, pos, dir: h.dir, speed: 0.0, range: 0.0 });
        None
    }

    fn step_hook(&mut self, i: usize, mut h: HookLine, dt: f32) -> Option<HookLine> {
        let base = self.cfg.move_params.clone();
        let ok = self.players.get(i).is_some_and(|p| p.id == h.owner && p.alive && !p.status.stunned());
        if !ok {
            return None;
        }
        let me = self.players[i].hull(&base).center();
        match h.phase {
            Phase::Flying => {
                let step = (h.spec.speed * dt).min(h.spec.range - h.flown).max(0.0);
                let a = h.pos;
                let b = a + h.dir * step;
                let wall = bd_core::voxel::raycast(&self.voxels, a, h.dir, step).map(|w| (w.t, w.point));
                let wall_t = wall.map_or(f32::INFINITY, |w| w.0);
                // Players along this step (a dodging one lets it pass).
                let mut best: Option<(usize, f32)> = None;
                for (k, q) in self.players.iter().enumerate() {
                    if k == i || !q.alive {
                        continue;
                    }
                    let hull = crate::geom::inflate(&q.hull(&base), HEAD);
                    if let Some((t, _)) = crate::geom::segment_aabb(a, b, &hull) {
                        let d = t * step;
                        if d <= wall_t && best.is_none_or(|x| d < x.1) {
                            best = Some((k, d));
                        }
                    }
                }
                // An enemy construct in the way eats the hook.
                let block = self.constructs.segment_query(a, b, HEAD, Some(h.owner)).filter(|(c, _, _)| self.constructs.get(*c).is_some_and(|c| c.parent != Some(c.id)));
                let block_t = block.map_or(f32::INFINITY, |b| b.1 * step);
                if let Some((k, d)) = best.filter(|b| b.1 <= block_t) {
                    let tid = self.players[k].id;
                    let point = a + h.dir * d;
                    let hit = HitInfo::new(tid, Some(h.owner), HitKind::Thrown, h.spec.damage, 0.0, h.dir, point).with_impact(0.9);
                    if self.apply_hit(hit).applied {
                        if let Some(s) = h.spec.status.filter(|_| h.spec.secs > 0.0) {
                            self.apply_status(k, s, h.spec.secs, Some(h.owner));
                        }
                        self.op_event(OpEvent::Hook { player: h.owner, phase: HookPhase::Caught, target: Some(tid), pos: point, dir: h.dir, speed: 0.0, range: 0.0 });
                        if h.spec.pull <= 0.0 {
                            return None;
                        }
                        let v = &mut self.players[k].mv;
                        v.release_tethers();
                        v.extra.cancel_dodge(&mut v.vel);
                        h.pos = point;
                        h.phase = Phase::Towing { target: tid, t: 0.0 };
                        return Some(h);
                    }
                    // Dodged: the hook flies on through them.
                }
                if let Some((cid, t, _normal)) = block.filter(|b| b.1 * step <= wall_t) {
                    let point = a + (b - a) * t;
                    if h.spec.damage > 0.0 {
                        self.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: cid, source: Some(h.owner), damage: h.spec.damage, impulse: h.dir * 4.0, blade: false, point });
                    }
                    return self.hook_end(&h, point, None);
                }
                if let Some((_, point)) = wall {
                    if h.spec.anchor && h.spec.pull > 0.0 {
                        let p = &mut self.players[i].mv;
                        p.release_tethers();
                        h.pos = point;
                        h.phase = Phase::Reeling { point, t: 0.0 };
                        self.op_event(OpEvent::Hook { player: h.owner, phase: HookPhase::Anchored, target: None, pos: point, dir: h.dir, speed: 0.0, range: 0.0 });
                        return Some(h);
                    }
                    return self.hook_end(&h, point, None);
                }
                h.pos = b;
                h.flown += step;
                if h.flown >= h.spec.range - 1e-3 {
                    return self.hook_end(&h, b, None);
                }
                Some(h)
            }
            Phase::Towing { target, t } => {
                let Some(k) = self.pidx(target).filter(|&k| self.players[k].alive) else { return self.hook_end(&h, me, None) };
                let t = t + dt;
                let look = self.players[i].look();
                let stop = me + flat(look, Vec3::NEG_Z) * 1.7;
                let v = &mut self.players[k];
                let vc = v.hull(&base).center();
                let d = stop - vc;
                // A dodge (or a tether of their own) breaks the tow: the escape.
                if v.mv.extra.dodging() || v.mv.tethered() {
                    return self.hook_end(&h, vc, Some(target));
                }
                if d.length() < 1.1 || t >= TOW_SECS {
                    let my_vel = self.players[i].mv.vel;
                    let v = &mut self.players[k];
                    v.mv.vel = my_vel * 0.5 + Vec3::Y * 1.5;
                    v.mv.tumble = 0.0;
                    return self.hook_end(&h, vc, Some(target));
                }
                v.mv.vel = d.normalize_or_zero() * h.spec.pull + Vec3::Y * 1.2;
                v.mv.grounded = false;
                v.mv.tumble = v.mv.tumble.max(0.1);
                h.pos = vc;
                h.phase = Phase::Towing { target, t };
                Some(h)
            }
            Phase::Reeling { point, t } => {
                let t = t + dt;
                let d = point - me;
                let p = &mut self.players[i].mv;
                if d.length() < 1.4 || t >= TOW_SECS * 1.5 || p.extra.dodging() {
                    p.vel *= 0.6;
                    return self.hook_end(&h, point, None);
                }
                p.vel = d.normalize_or_zero() * h.spec.pull + Vec3::Y * 1.5;
                p.grounded = false;
                p.tumble = 0.0;
                h.phase = Phase::Reeling { point, t };
                Some(h)
            }
        }
    }
}
