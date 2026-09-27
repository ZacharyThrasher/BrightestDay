//! `Deflect { secs, angle }` — for `secs`, enemy rounds and thrown things that come at you inside
//! a cone around your aim (`angle`° half-angle; 0 = the front half, 180 = all round) are sent back
//! **along your crosshair** at their own speed and become yours (Genji's Deflect, Fox's Reflector,
//! Yasuo's Wind Wall read as a stance). Aim is the skill: point it at the shooter. Hitscan / fast
//! hits from in front are simply turned aside (`counter.rs`, the shared stance hook).

use bd_core::Vec3;

use super::{OpEvent, Stance, StanceKind};
use crate::events::GameEvent;
use crate::powers::PowerError;
use crate::world::World;

pub const MAX_SECS: f32 = 2.5;
/// Rounds are caught this far from your centre (+ one tick of their own travel).
const CATCH: f32 = 2.6;

impl World {
    pub(crate) fn op_deflect(&mut self, i: usize, secs: f32, angle: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let secs = secs.clamp(0.1, MAX_SECS);
        let angle = if angle <= 0.0 { 90.0 } else { angle.min(180.0) };
        let s = self.stance_mut(i);
        s.deflect = secs;
        s.deflect_cos = angle.to_radians().cos();
        self.op_event(OpEvent::Stance { player: id, kind: StanceKind::Deflect, secs, angle });
        Ok(())
    }

    pub(crate) fn step_deflect(&mut self, i: usize, s: Stance) {
        let base = self.cfg.move_params.clone();
        let me = &self.players[i];
        let center = me.hull(&base).center();
        let aim = me.look();
        let id = s.owner;
        let tick = self.tick;
        let dt = bd_core::DT;
        let mut sent = [None::<(Vec3, Vec3)>; 4];
        let mut n = 0;
        let mut fired: [Option<GameEvent>; 16] = Default::default();
        let mut fired_n = 0;
        for pr in &mut self.projectiles {
            if fired_n >= fired.len() {
                break;
            }
            if pr.dead || pr.spec.owner == id {
                continue;
            }
            let d = pr.pos - center;
            let len = d.length();
            let speed = pr.vel.length();
            if len > CATCH + speed * dt * 1.5 || len < 1e-3 {
                continue;
            }
            // Incoming and inside the cone.
            if pr.vel.dot(d) >= 0.0 || d.dot(aim) / len < s.deflect_cos {
                continue;
            }
            let v = aim * speed.max(12.0) * 1.1;
            pr.vel = v;
            pr.pos = center + aim * 1.0;
            pr.spec.owner = id;
            pr.spec.construct = None;
            pr.spec.muzzle_speed = v.length();
            pr.lag = 0.0;
            pr.home = None;
            pr.age = 0.0;
            pr.travelled = 0.0;
            pr.origin = pr.pos;
            pr.vel0 = pr.vel;
            pr.segment_tick = tick;
            fired[fired_n] = Some(GameEvent::ProjectileFired { id: pr.id, owner: id, kind: pr.spec.kind, origin: pr.pos, vel: pr.vel });
            fired_n += 1;
            if n < sent.len() {
                sent[n] = Some((pr.pos, aim));
                n += 1;
            }
        }
        for e in fired.iter_mut().filter_map(Option::take) {
            self.inject_event(e);
        }
        // Thrown / launched enemy constructs coming at you.
        let mut turned = [None::<bd_core::ConstructId>; 4];
        let mut m = 0;
        for c in self.constructs.iter() {
            if m >= turned.len() {
                break;
            }
            if c.owner == id || !c.active() || c.attached() || c.parent == Some(c.id) || c.pilot.is_some() {
                continue;
            }
            let d = c.center() - center;
            let len = d.length();
            let speed = c.vel.length();
            if speed < 6.0 || len > CATCH + c.half.max_element() + speed * dt * 1.5 || len < 1e-3 {
                continue;
            }
            if c.vel.dot(d) >= 0.0 || d.dot(aim) / len < s.deflect_cos {
                continue;
            }
            turned[m] = Some(c.id);
            m += 1;
        }
        for cid in turned.iter().flatten().copied() {
            if let Some(c) = self.constructs.get_mut(cid) {
                let speed = c.vel.length().max(12.0) * 1.1;
                c.vel = aim * speed;
                c.owner = id;
                c.target = None;
                let pos = c.center();
                if n < sent.len() {
                    sent[n] = Some((pos, aim));
                    n += 1;
                }
            }
        }
        for (pos, dir) in sent.iter().flatten().copied() {
            self.op_event(OpEvent::Parried { player: id, kind: StanceKind::Deflect, attacker: None, pos, dir });
        }
    }
}
