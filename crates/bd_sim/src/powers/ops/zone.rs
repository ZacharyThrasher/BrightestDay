//! `Zone { radius, secs, .. }` — a lingering area (at you, at the crosshair or in front) that works
//! on every ENEMY inside it each tick: a `status` kept on them, a `slow` (0..0.9 of their speed
//! taken away — thick air, never a freeze), `dps` damage, and/or `lockout` (no power moves inside:
//! an EMP, an anti-magic field). `follow` keeps it centred on you. Mei's Blizzard (Chill + slow),
//! Faceless Void's Chronosphere (a heavy slow, NOT a freeze), Bangalore's smoke (Shadow: blind
//! vignette), Sombra's EMP (lockout), a poison cloud (dps). Drawn as a strength-0 force field
//! (the ROOM bubble) so every client already sees its edge.

use bd_core::{PlayerId, Tick, Vec3};

use super::{MAX_ZONES, OpEvent};
use crate::combat::{HitInfo, HitKind};
use crate::powers::{At, PowerError};
use crate::status::StatusKind;
use crate::world::World;

pub const MAX_SECS: f32 = 12.0;
/// Damage ticks every this many sim ticks (4 per second).
const DPS_TICKS: Tick = 30;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ZoneSpec {
    pub radius: f32,
    pub secs: f32,
    pub status: Option<StatusKind>,
    pub slow: f32,
    pub dps: f32,
    pub lockout: bool,
    pub at: At,
    pub dist: f32,
    pub follow: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Zone {
    owner: PlayerId,
    spec: ZoneSpec,
    pos: Vec3,
    left: f32,
    field: Option<u16>,
}

impl World {
    pub(crate) fn op_zone(&mut self, i: usize, spec: ZoneSpec) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let spec = ZoneSpec {
            radius: super::fclamp(spec.radius, 1.0, 24.0),
            secs: super::fclamp(spec.secs, 0.2, MAX_SECS),
            slow: super::fclamp(spec.slow, 0.0, 0.9),
            dps: super::fclamp(spec.dps, 0.0, 20.0),
            ..spec
        };
        let at = if spec.follow { At::Me } else { spec.at };
        let pos = self.op_at_point(i, at, spec.dist);
        let field = self.create_field(id, pos, spec.radius, 0.0, spec.secs, None);
        // Full: the oldest zone ends.
        let slot = match self.powers.ops.zones.iter().position(Option::is_none) {
            Some(s) => s,
            None => {
                self.powers.ops.zones.rotate_left(1);
                MAX_ZONES - 1
            }
        };
        self.powers.ops.zones[slot] = Some(Zone { owner: id, spec, pos, left: spec.secs, field });
        self.op_event(OpEvent::Zone { player: id, field, pos, radius: spec.radius, secs: spec.secs, status: spec.status, slow: spec.slow, lockout: spec.lockout });
        Ok(())
    }

    pub(crate) fn step_zones(&mut self, dt: f32) {
        let base = self.cfg.move_params.clone();
        let run = base.run_speed;
        let dps_tick = self.tick % DPS_TICKS == 0;
        for z in 0..MAX_ZONES {
            let Some(mut zone) = self.powers.ops.zones[z] else { continue };
            zone.left -= dt;
            let owner = self.pidx(zone.owner).filter(|&k| self.players[k].alive);
            if zone.left <= 0.0 || owner.is_none() {
                self.powers.ops.zones[z] = None;
                continue;
            }
            if let (true, Some(k)) = (zone.spec.follow, owner) {
                zone.pos = self.players[k].hull(&base).center();
                if let Some(f) = zone.field.and_then(|id| self.fields.iter_mut().find(|f| f.id == id)) {
                    f.pos = zone.pos;
                }
            }
            self.powers.ops.zones[z] = Some(zone);
            let r2 = zone.spec.radius * zone.spec.radius;
            let cap = run * (1.0 - zone.spec.slow);
            for k in 0..self.players.len() {
                let q = &self.players[k];
                if q.id == zone.owner || !q.alive || (q.hull(&base).center() - zone.pos).length_squared() > r2 {
                    continue;
                }
                // Never a stun field (no freezes: owner rule).
                if let Some(s) = zone.spec.status.filter(|s| *s != StatusKind::Stun) {
                    let left = q.status.timers[s as usize];
                    if left < 0.15 {
                        self.apply_status(k, s, 0.3, Some(zone.owner));
                    }
                }
                if zone.spec.lockout {
                    self.lock_out(k, 0.2);
                }
                if zone.spec.slow > 0.0 {
                    let mv = &mut self.players[k].mv;
                    let h = Vec3::new(mv.vel.x, 0.0, mv.vel.z);
                    let len = h.length();
                    if len > cap && len > 1e-3 {
                        // Ease toward the cap (thick air), never a hard stop.
                        let want = cap + (len - cap) * 0.6;
                        mv.vel.x *= want / len;
                        mv.vel.z *= want / len;
                    }
                    if mv.vel.y > 0.0 {
                        mv.vel.y *= 1.0 - zone.spec.slow * 0.08;
                    }
                }
                if zone.spec.dps > 0.0 && dps_tick {
                    let (tid, c) = (self.players[k].id, self.players[k].hull(&base).center());
                    let hit = HitInfo::new(tid, Some(zone.owner), HitKind::Explosion, zone.spec.dps * DPS_TICKS as f32 * bd_core::DT, 0.0, Vec3::Y, c).with_impact(0.3);
                    self.apply_hit(hit);
                }
            }
        }
    }
}
