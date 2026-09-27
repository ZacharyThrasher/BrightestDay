//! LEGENDARY: the world-side halves of the legendary gun rules (`constructs/legend.rs`, COMBAT.md §5l):
//! **needles** that home without letting go, stick into whoever they hit and stack toward a
//! supercombine burst, and the **BFG orb**'s tendrils that zap every enemy near its path. Both ride
//! `projectile::effects` bits, so they cost nothing for any other round.

use bd_core::voxel::raycast;
use bd_core::{DT, PlayerId, Vec3};

use super::World;
use crate::combat::{Explosion, HitInfo, HitKind};
use crate::events::{GameEvent, HeldFx};
use crate::projectile::{Projectile, ProjectileSpawn, effects};

impl World {
    /// Per tick for a round carrying legendary effects (before its homing / contact test).
    pub(super) fn legend_projectile(&mut self, p: &mut Projectile) {
        if p.spec.effects & effects::NEEDLE != 0 {
            let t = &self.cfg.tuning.legendary;
            if p.home.is_none() && p.age <= 0.3 {
                self.needle_seek(p);
            }
            if p.home.is_some() {
                p.home_turn = t.needle_turn_deg.to_radians();
            }
        }
        if p.spec.effects & effects::TENDRILS != 0 {
            self.bfg_tendrils(p);
        }
    }

    /// A needle fired at nobody in particular finds the nearest enemy ahead (clear line).
    fn needle_seek(&self, p: &mut Projectile) {
        let t = &self.cfg.tuning.legendary;
        let dir = p.vel.normalize_or_zero();
        if dir == Vec3::ZERO {
            return;
        }
        let base = &self.cfg.move_params;
        let owner = p.spec.owner;
        let cands = self.players.iter().filter(|q| q.id != owner && q.alive && !q.shielded()).map(|q| (q.id, q.hull(base)));
        let clear = |a: Vec3, b: Vec3| {
            let d = b - a;
            let len = d.length();
            len < 1e-3 || raycast(&self.voxels, a, d / len, len).is_none()
        };
        if let Some((id, _)) = crate::aim_assist::pick_target(p.pos, dir, t.needle_cone_deg.to_radians().cos(), t.needle_range, cands, clear) {
            p.home = Some(id);
        }
    }

    /// The BFG orb zaps every enemy within `bfg_zap_range` in clear sight, every `bfg_zap_period`.
    fn bfg_tendrils(&mut self, p: &Projectile) {
        let t = self.cfg.tuning.legendary;
        let period = t.bfg_zap_period.max(DT);
        if ((p.age - DT) / period).floor() >= (p.age / period).floor() {
            return;
        }
        let mut hits: [(PlayerId, Vec3, f32); bd_core::MAX_PLAYERS] = [(PlayerId(0), Vec3::ZERO, 0.0); bd_core::MAX_PLAYERS];
        let mut n = 0;
        for q in &self.players {
            if q.id == p.spec.owner || !q.alive || q.shielded() || n >= hits.len() {
                continue;
            }
            let at = q.hull(&self.cfg.move_params).center();
            let d = (at - p.pos).length();
            if d > t.bfg_zap_range || d < 1e-3 || raycast(&self.voxels, p.pos, (at - p.pos) / d, d).is_some() {
                continue;
            }
            hits[n] = (q.id, at, d);
            n += 1;
        }
        let s = p.spec;
        for &(id, at, d) in &hits[..n] {
            let dir = (at - p.pos) / d;
            let mut hit = HitInfo::new(id, Some(s.owner), HitKind::Chain, t.bfg_zap_dmg, 3.0, dir, at).with_element(s.element).with_closing(0.0).with_impact(0.8);
            hit.construct = s.construct;
            self.apply_hit(hit);
            if let Some(cid) = s.construct {
                self.events.push(GameEvent::HeldFx { id: cid, owner: s.owner, fx: HeldFx::BfgZap, pos: p.pos, dir, strength: d });
            }
        }
    }

    /// A needle stuck into `target` at `point`: count it on its gun; enough of them supercombine.
    pub(super) fn needle_stick(&mut self, s: &ProjectileSpawn, target: PlayerId, point: Vec3, dir: Vec3) {
        let Some(cid) = s.construct else { return };
        let t = self.cfg.tuning.legendary;
        let Some(c) = self.constructs.get_mut(cid) else { return };
        let owner = c.owner;
        let Some(count) = crate::constructs::legend::needle_stick(&mut c.held, target, &t) else { return };
        self.events.push(GameEvent::HeldFx { id: cid, owner, fx: HeldFx::NeedleStick, pos: point, dir, strength: count as f32 });
        if count >= t.needle_count {
            let at = self.player(target).map_or(point, |p| p.hull(&self.cfg.move_params).center());
            self.explode(Explosion {
                pos: at,
                radius: t.super_radius,
                damage: t.super_dmg,
                knockback: t.super_kb,
                source: Some(owner),
                construct: Some(cid),
                element: s.element,
                carve_radius: 0.8,
                self_damage: false,
            });
            self.events.push(GameEvent::HeldFx { id: cid, owner, fx: HeldFx::Supercombine, pos: at, dir, strength: t.super_radius });
        }
    }
}
