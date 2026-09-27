//! `Ram { .. }` — a committed charge along the look. The body of the move is the charge itself,
//! so the hit lands where the charge ARRIVES (not where it started, which is what an
//! `Impulse` + `Shove` pair does):
//!
//! * **pin** (Reinhardt's Charge, Raiden's Torpedo, a bull): the first enemy met is scooped onto
//!   the front of the charge and carried; a wall ends it with a **slam** (`damage + wall`, a dent in
//!   the wall, they crumple off it). Running out of charge drops them with `damage`. The carried
//!   player escapes by **dodging** or **tethering out** (never a lock: owner rule).
//! * **punch** (default: Doomfist's Rocket Punch, Falcon Punch, a shoulder check): the first enemy
//!   met eats `damage` / `knockback` and the ram stops there; the knockback does the rest — a
//!   player knocked into a wall gets the world's wall splat (bigger when they are hurt).
//! * **through** (Genji's Swift Strike, a bicycle kick, a spinning bird kick): every enemy along
//!   the path is struck once and the charge keeps going.
//!
//! `steer` (°/s) lets the charge follow the crosshair (0 = locked in at the start); `pitch` 0..1 is
//! how much of the look's up/down the charge follows (0 = a ground charge, 1 = a flying dash);
//! `keep` 0..1 is the momentum you keep at the end (0 = plant your feet). A hard hit, a stun or your
//! own dodge ends it. Enemy constructs in the way take `damage` and are shoved aside.

use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId, Vec3};

use super::{OpEvent, RamPhase, flat};
use crate::combat::{CarveRequest, ConstructHit, HitInfo, HitKind};
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_SECS: f32 = 2.5;
const MAX_SPEED: f32 = 60.0;

/// The authored numbers (see `MoveOp::Ram`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RamSpec {
    pub speed: f32,
    pub secs: f32,
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub pin: bool,
    pub through: bool,
    pub wall: f32,
    pub pitch: f32,
    pub steer: f32,
    pub keep: f32,
    pub element: Element,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ram {
    pub owner: PlayerId,
    spec: RamSpec,
    dir: Vec3,
    t: f32,
    last: Vec3,
    /// Pinned player.
    victim: Option<PlayerId>,
    /// Player indices already struck (through rams).
    struck: u8,
    /// Ticks in a row the charge made (almost) no progress.
    stalled: u8,
    /// The last construct battered (each is hit once per ram).
    bashed: Option<ConstructId>,
}

fn ram_dir(look: Vec3, pitch: f32) -> Vec3 {
    let f = flat(look, Vec3::NEG_Z);
    let p = pitch.clamp(0.0, 1.0);
    (f * (1.0 - p) + look.normalize_or(f) * p).normalize_or(f)
}

impl World {
    pub(crate) fn op_ram(&mut self, i: usize, spec: RamSpec) -> Result<(), PowerError> {
        let id = self.players[i].id;
        if self.powers.ops.rams[i].is_some_and(|r| r.owner == id) {
            return Err(PowerError::Busy);
        }
        let spec = RamSpec {
            speed: spec.speed.clamp(4.0, MAX_SPEED),
            secs: spec.secs.clamp(0.05, MAX_SECS),
            damage: spec.damage.clamp(0.0, 60.0),
            knockback: spec.knockback.clamp(0.0, 80.0),
            up: spec.up.clamp(0.0, 2.0),
            wall: spec.wall.clamp(0.0, 60.0),
            steer: spec.steer.clamp(0.0, 720.0),
            keep: spec.keep.clamp(0.0, 1.0),
            ..spec
        };
        let (_, look) = self.op_eye_look(i);
        let dir = ram_dir(look, spec.pitch);
        let p = &mut self.players[i];
        p.mv.release_tethers();
        p.mv.extra.cancel_dodge(&mut p.mv.vel);
        p.mv.vel = if spec.pitch > 0.0 { dir * spec.speed } else { dir * spec.speed + Vec3::Y * p.mv.vel.y.max(0.0) };
        if dir.y > 0.05 {
            p.mv.grounded = false;
        }
        let pos = p.mv.pos;
        self.powers.ops.rams[i] = Some(Ram { owner: id, spec, dir, t: 0.0, last: pos, victim: None, struck: 0, stalled: 0, bashed: None });
        self.op_event(OpEvent::Ram { player: id, phase: RamPhase::Start, target: None, pos, dir });
        Ok(())
    }

    pub(crate) fn step_rams(&mut self, dt: f32) {
        for i in 0..self.powers.ops.rams.len() {
            if let Some(r) = self.powers.ops.rams[i] {
                self.step_ram(i, r, dt);
            }
        }
    }

    /// End player `i`'s ram (drops a pinned victim where it is).
    pub(crate) fn end_ram(&mut self, i: usize, phase: RamPhase) {
        let Some(r) = self.powers.ops.rams[i].take() else { return };
        if let Some(p) = self.players.get_mut(i).filter(|p| p.id == r.owner) {
            let keep = r.spec.keep;
            p.mv.vel.x *= keep;
            p.mv.vel.z *= keep;
            if r.spec.pitch > 0.0 {
                p.mv.vel.y *= keep;
            }
            let pos = p.mv.pos;
            self.op_event(OpEvent::Ram { player: r.owner, phase, target: r.victim, pos, dir: r.dir });
        }
    }

    fn step_ram(&mut self, i: usize, mut r: Ram, dt: f32) {
        let base = self.cfg.move_params.clone();
        let valid = self.players.get(i).is_some_and(|p| p.id == r.owner && p.alive && !p.status.stunned());
        if !valid {
            self.powers.ops.rams[i] = None;
            return;
        }
        // Your own dodge cancels the charge (the dodge takes over).
        if self.players[i].mv.extra.dodging() {
            self.end_ram(i, RamPhase::End);
            return;
        }
        r.t += dt;
        // Steer toward the crosshair.
        let look = self.players[i].look();
        let want = ram_dir(look, r.spec.pitch);
        if r.spec.steer > 0.0 {
            let max = (r.spec.steer * dt).to_radians();
            let ang = r.dir.angle_between(want);
            r.dir = if ang <= max || !ang.is_finite() { want } else { r.dir.lerp(want, max / ang).normalize_or(r.dir) };
        }
        let me = self.players[i].hull(&base);
        let center = me.center();
        let pos = self.players[i].mv.pos;
        let progress = (pos - r.last).dot(r.dir);
        r.last = pos;
        r.stalled = if r.t > 0.06 && progress < r.spec.speed * dt * 0.3 { r.stalled.saturating_add(1) } else { 0 };
        // With someone on the front, the wall meets THEM first (they are ~1.3 m ahead of you).
        let lead = if r.victim.is_some() { 1.3 } else { 0.0 };
        let wall_ahead = bd_core::voxel::raycast(&self.voxels, center + Vec3::Y * 0.3, r.dir, me.half().x + 0.45 + lead).map(|h| (h.point, h.normal.as_vec3()));
        let blocked = r.stalled >= 2 || (r.t > 0.04 && wall_ahead.is_some());

        // The carried player.
        if let Some(vid) = r.victim {
            let k = self.pidx(vid).filter(|&k| self.players[k].alive);
            let escaped = k.is_none_or(|k| {
                let v = &self.players[k].mv;
                v.extra.dodging() || v.tethered()
            });
            if escaped {
                r.victim = None;
                if let Some(k) = k {
                    self.op_event(OpEvent::Ram { player: r.owner, phase: RamPhase::Escaped, target: Some(vid), pos: self.players[k].mv.pos, dir: r.dir });
                }
            } else if let Some(k) = k {
                if blocked {
                    // SLAM: into the wall they go.
                    let normal = wall_ahead.map_or(-r.dir, |w| w.1);
                    let point = wall_ahead.map_or(center + r.dir, |w| w.0);
                    let vc = self.players[k].hull(&base).center();
                    let hit = HitInfo::new(vid, Some(r.owner), HitKind::HeavyMelee, r.spec.damage + r.spec.wall, r.spec.knockback * 0.45, normal + Vec3::Y * 0.6, vc)
                        .with_element(r.spec.element)
                        .with_impact(2.2);
                    self.apply_hit(hit);
                    self.request_carve(CarveRequest { center: point, radius: 1.3, power: 1.2 });
                    self.op_event(OpEvent::Ram { player: r.owner, phase: RamPhase::Slam, target: Some(vid), pos: point, dir: r.dir });
                    self.powers.ops.rams[i] = Some(r);
                    self.end_ram(i, RamPhase::End);
                    if let Some(p) = self.players.get_mut(i) {
                        p.mv.vel = -r.dir * 2.0 + Vec3::Y * 2.0;
                    }
                    return;
                }
                let my_vel = self.players[i].mv.vel;
                let v = &mut self.players[k];
                let front = flat(r.dir, Vec3::NEG_Z) * (me.half().x + 0.75) + r.dir * 0.2;
                v.mv.pos = pos + front + Vec3::Y * 0.15;
                v.mv.vel = my_vel;
                v.mv.grounded = false;
                v.mv.tumble = v.mv.tumble.max(0.15);
            }
        } else if blocked {
            let point = wall_ahead.map_or(center + r.dir, |w| w.0);
            self.op_event(OpEvent::Ram { player: r.owner, phase: RamPhase::Bonk, target: None, pos: point, dir: r.dir });
            self.powers.ops.rams[i] = Some(r);
            self.end_ram(i, RamPhase::End);
            return;
        }

        // Contact: who is on the front of the charge?
        if r.victim.is_none() {
            let reach = center + r.dir * (me.half().x + 0.5);
            let mut met: Option<(usize, f32)> = None;
            for (k, q) in self.players.iter().enumerate() {
                if k == i || !q.alive || k >= 8 || r.struck & (1 << k) != 0 {
                    continue;
                }
                let h = crate::geom::inflate(&q.hull(&base), 0.35);
                let c = crate::geom::closest_point(&h, reach);
                let d = (c - reach).length();
                if d <= me.half().x + 0.25 && (h.center() - center).dot(r.dir) > -0.2 && met.is_none_or(|m| d < m.1) {
                    met = Some((k, d));
                }
            }
            if let Some((k, _)) = met {
                let tid = self.players[k].id;
                let vc = self.players[k].hull(&base).center();
                r.struck |= 1 << k;
                if r.spec.pin {
                    let touch = HitInfo::new(tid, Some(r.owner), HitKind::Melee, r.spec.damage * 0.25, 0.0, r.dir, vc).with_element(r.spec.element);
                    if self.apply_hit(touch).applied {
                        let v = &mut self.players[k];
                        v.mv.release_tethers();
                        v.mv.extra.cancel_dodge(&mut v.mv.vel);
                        r.victim = Some(tid);
                        self.op_event(OpEvent::Ram { player: r.owner, phase: RamPhase::Pinned, target: Some(tid), pos: vc, dir: r.dir });
                    }
                } else {
                    let hit = HitInfo::new(tid, Some(r.owner), HitKind::HeavyMelee, r.spec.damage, r.spec.knockback, r.dir + Vec3::Y * r.spec.up, vc)
                        .with_element(r.spec.element)
                        .with_impact(1.6);
                    let landed = self.apply_hit(hit).applied;
                    if landed {
                        self.op_event(OpEvent::Ram { player: r.owner, phase: RamPhase::Hit, target: Some(tid), pos: vc, dir: r.dir });
                        if !r.spec.through {
                            self.powers.ops.rams[i] = Some(r);
                            self.end_ram(i, RamPhase::End);
                            return;
                        }
                    }
                }
            }
        }

        // Enemy constructs in the way are battered aside (walls may still stop you).
        let id = r.owner;
        if r.spec.damage > 0.0 {
            let reach = center + r.dir * (me.half().x + 0.8);
            let hit = self.constructs.segment_query(center, reach, 0.6, Some(id));
            if let Some((cid, t, _normal)) = hit.filter(|h| Some(h.0) != r.bashed) {
                let point = center + (reach - center) * t;
                r.bashed = Some(cid);
                self.apply_construct_hit(ConstructHit { pierce: false, blunt: true, target: cid, source: Some(id), damage: r.spec.damage * 0.5, impulse: r.dir * r.spec.knockback.max(8.0), blade: false, point });
            }
        }

        // Drive.
        let p = &mut self.players[i];
        if r.spec.pitch > 0.0 {
            p.mv.vel = r.dir * r.spec.speed;
        } else {
            p.mv.vel.x = r.dir.x * r.spec.speed;
            p.mv.vel.z = r.dir.z * r.spec.speed;
        }
        p.mv.tumble = 0.0;
        if r.t >= r.spec.secs {
            // Out of charge: whoever is on the front is thrown off it.
            if let Some(vid) = r.victim
                && let Some(k) = self.pidx(vid)
            {
                let vc = self.players[k].hull(&base).center();
                let hit = HitInfo::new(vid, Some(id), HitKind::HeavyMelee, r.spec.damage, r.spec.knockback, r.dir + Vec3::Y * (0.3 + r.spec.up), vc).with_element(r.spec.element).with_impact(1.6);
                self.apply_hit(hit);
                self.op_event(OpEvent::Ram { player: id, phase: RamPhase::Hit, target: Some(vid), pos: vc, dir: r.dir });
            }
            self.powers.ops.rams[i] = Some(r);
            self.end_ram(i, RamPhase::End);
            return;
        }
        self.powers.ops.rams[i] = Some(r);
    }

    /// A hard hit (or a stun) on player `ti` breaks their ram / lock-on / grab.
    pub(crate) fn interrupt_on_hit(&mut self, ti: usize, hit: &HitInfo) {
        if hit.source == Some(hit.target) {
            return;
        }
        let hard = hit.knockback >= 12.0 || hit.element == Element::Electric;
        if !hard {
            return;
        }
        let id = self.players[ti].id;
        if self.powers.ops.rams[ti].is_some_and(|r| r.owner == id) {
            // Stopped in its tracks: the pinned player is simply let go.
            if let Some(r) = self.powers.ops.rams[ti].as_mut() {
                r.victim = None;
                r.spec.keep = 0.0;
            }
            self.end_ram(ti, RamPhase::End);
        }
        self.break_lock_on(ti);
        self.break_grab(ti);
    }
}
