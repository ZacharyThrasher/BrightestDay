//! `check_invariants` (run every tick in debug builds, by tests and by the soak) and a
//! deterministic state hash (desync / determinism checks).

use bd_core::MAX_PLAYERS;
use bd_core::mechanics::FormCategory;
use bd_core::voxel::Aabb;
use bd_core::Vec3;

use super::World;
use crate::constructs::Slot;
use crate::rng::Fnv64;
use crate::space::LinkEnd;

/// Hard cap on live constructs (8 players × Chaos caps, with headroom).
pub const MAX_CONSTRUCTS: usize = 256;
/// Hard cap on live projectiles.
pub const MAX_PROJECTILES: usize = 512;
/// Entities may be this far outside the arena bounds before they must have been removed.
const BOUNDS_MARGIN: f32 = 80.0;

fn fin3(v: Vec3) -> bool {
    v.is_finite()
}

impl World {
    /// Everything that must always hold at the end of a tick. Returns a description of the first
    /// violation.
    pub fn check_invariants(&self) -> Result<(), String> {
        let b = self.arena.bounds;
        let outer = Aabb { min: b.min - Vec3::splat(BOUNDS_MARGIN), max: b.max + Vec3::splat(BOUNDS_MARGIN * 2.0) };
        let inside = |p: Vec3| p.cmpge(outer.min).all() && p.cmple(outer.max).all();
        let wmax = self.cfg.rules.willpower_max();
        // Players.
        if self.players.len() > MAX_PLAYERS {
            return Err(format!("{} players > {MAX_PLAYERS}", self.players.len()));
        }
        for w in self.players.windows(2) {
            if w[0].id >= w[1].id {
                return Err(format!("players not sorted/unique: {:?} {:?}", w[0].id, w[1].id));
            }
        }
        for p in &self.players {
            let s = &p.mv;
            if !(fin3(s.pos) && fin3(s.vel) && fin3(s.ground_vel) && fin3(s.ground_normal)) {
                return Err(format!("player {:?} non-finite movement state", p.id));
            }
            if s.hooks.iter().any(|h| !fin3(h.pos) || !fin3(h.vel) || !h.rope_len.is_finite()) {
                return Err(format!("player {:?} non-finite hook", p.id));
            }
            if !(p.health.is_finite() && (0.0..=crate::combat::MAX_HEALTH + 1e-3).contains(&p.health)) {
                return Err(format!("player {:?} health {}", p.id, p.health));
            }
            if !(p.willpower.is_finite() && (-1e-3..=wmax + 1e-3).contains(&p.willpower)) {
                return Err(format!("player {:?} willpower {}", p.id, p.willpower));
            }
            if !p.size.scale.is_finite() || !(0.4..=1.7).contains(&p.size.scale) {
                return Err(format!("player {:?} size {}", p.id, p.size.scale));
            }
            if p.alive && !inside(s.pos) {
                return Err(format!("player {:?} alive outside bounds at {}", p.id, s.pos));
            }
            if p.status.timers.iter().any(|t| !t.is_finite() || *t < 0.0) {
                return Err(format!("player {:?} bad status timer", p.id));
            }
            if let Some(seat) = p.seat {
                match self.constructs.get(seat.construct) {
                    Some(c) if c.pilot == Some(p.id) => {}
                    Some(_) => return Err(format!("player {:?} seated in {:?} whose pilot differs", p.id, seat.construct)),
                    None => return Err(format!("player {:?} seated in missing {:?}", p.id, seat.construct)),
                }
                if !p.alive {
                    return Err(format!("dead player {:?} seated", p.id));
                }
            }
            for r in [p.last_target, p.last_attacker.map(|a| a.0)].into_iter().flatten() {
                if self.pidx(r).is_none() {
                    return Err(format!("player {:?} references missing player {:?}", p.id, r));
                }
            }
        }
        // Constructs.
        if self.constructs.len() > MAX_CONSTRUCTS {
            return Err(format!("{} constructs > {MAX_CONSTRUCTS}", self.constructs.len()));
        }
        let cs = self.constructs.as_slice();
        for w in cs.windows(2) {
            if w[0].id.0 >= w[1].id.0 {
                return Err(format!("constructs not sorted/unique: {:?} {:?}", w[0].id, w[1].id));
            }
        }
        for c in cs {
            if c.gone.is_some() {
                return Err(format!("construct {:?} marked gone but not reaped", c.id));
            }
            if !(fin3(c.pos) && fin3(c.vel) && c.rot.is_finite() && fin3(c.half) && c.integrity.is_finite()) {
                return Err(format!("construct {:?} non-finite state", c.id));
            }
            if !inside(c.pos) && !c.attached() {
                return Err(format!("construct {:?} outside bounds at {}", c.id, c.pos));
            }
            if self.pidx(c.owner).is_none() {
                return Err(format!("construct {:?} owner {:?} missing", c.id, c.owner));
            }
            if let Some(pl) = c.pilot {
                match self.player(pl) {
                    Some(p) if p.seat.is_some_and(|s| s.construct == c.id) => {}
                    _ => return Err(format!("construct {:?} pilot {:?} not seated in it", c.id, pl)),
                }
            }
            if let Some(t) = c.target {
                if self.pidx(t).is_none() {
                    return Err(format!("construct {:?} targets missing player {:?}", c.id, t));
                }
            }
            if c.stuck_to.is_some_and(|t| self.pidx(t).is_none()) {
                return Err(format!("construct {:?} stuck to missing player", c.id));
            }
            let _ = FormCategory::Held;
        }
        // Per-owner slot caps.
        for p in &self.players {
            for slot in [Slot::Held, Slot::Worn, Slot::Structure, Slot::Object, Slot::Creature] {
                let mult = if matches!(slot, Slot::Held | Slot::Worn) { 1 } else { self.cfg.rules.cap_mult() };
                let n = self.constructs.in_slot(p.id, slot).count();
                if n > slot.limit() * mult {
                    return Err(format!("player {:?} has {n} constructs in {slot:?}", p.id));
                }
            }
            let colossal = self.constructs.owned_by(p.id).filter(|c| c.colossal).count();
            if colossal > 1 {
                return Err(format!("player {:?} has {colossal} colossal constructs", p.id));
            }
        }
        // Projectiles.
        if self.projectiles.len() > MAX_PROJECTILES {
            return Err(format!("{} projectiles", self.projectiles.len()));
        }
        for pr in &self.projectiles {
            if pr.dead || !fin3(pr.pos) || !fin3(pr.vel) || !inside(pr.pos) {
                return Err(format!("projectile {} bad state (dead={}, pos={})", pr.id, pr.dead, pr.pos));
            }
            if self.pidx(pr.spec.owner).is_none() {
                return Err(format!("projectile {} owner missing", pr.id));
            }
        }
        // Space.
        for pp in &self.portals {
            if pp.ends.iter().any(|e| !fin3(e.pos) || !fin3(e.normal) || !e.radius.is_finite()) {
                return Err(format!("portal {} non-finite", pp.id));
            }
            if self.pidx(pp.owner).is_none() {
                return Err(format!("portal {} owner missing", pp.id));
            }
        }
        for f in &self.fields {
            if !fin3(f.pos) || !f.strength.is_finite() || !f.radius.is_finite() {
                return Err(format!("field {} non-finite", f.id));
            }
            if self.pidx(f.owner).is_none() || f.construct.is_some_and(|c| self.constructs.get(c).is_none()) {
                return Err(format!("field {} dangling", f.id));
            }
        }
        for l in &self.links {
            for e in [l.a, l.b] {
                let ok = match e {
                    LinkEnd::Player(p) => self.player(p).is_some_and(|p| p.alive),
                    LinkEnd::Construct(c) => self.constructs.get(c).is_some(),
                    LinkEnd::Point(p) => fin3(p),
                };
                if !ok {
                    return Err(format!("link {} dangling end {e:?}", l.id));
                }
            }
        }
        Ok(())
    }

    /// Deterministic hash of the simulation state (players, constructs, projectiles, space,
    /// match). Terrain is covered separately by `voxels.content_hash()` (slower).
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv64::default();
        h.write_u64(self.tick);
        h.write(&[self.match_state.phase as u8]);
        let v3 = |h: &mut Fnv64, v: Vec3| {
            h.write_f32(v.x);
            h.write_f32(v.y);
            h.write_f32(v.z);
        };
        for p in &self.players {
            h.write(&[p.id.0, p.alive as u8]);
            v3(&mut h, p.mv.pos);
            v3(&mut h, p.mv.vel);
            h.write_f32(p.health);
            h.write_f32(p.willpower);
            h.write_f32(p.size.scale);
            h.write(&p.kos.to_le_bytes());
            h.write(&p.deaths.to_le_bytes());
        }
        for c in self.constructs.iter() {
            h.write(&c.id.0.to_le_bytes());
            v3(&mut h, c.pos);
            v3(&mut h, c.vel);
            h.write_f32(c.integrity);
            h.write(&[c.anim_state as u8]);
        }
        for pr in &self.projectiles {
            h.write(&pr.id.to_le_bytes());
            v3(&mut h, pr.pos);
        }
        for pp in &self.portals {
            h.write(&pp.id.to_le_bytes());
        }
        for f in &self.fields {
            h.write(&f.id.to_le_bytes());
            v3(&mut h, f.pos);
        }
        for l in &self.links {
            h.write(&l.id.to_le_bytes());
        }
        h.write_u64(self.carve_queue.len() as u64);
        // PARTY HOOK: party standings / effects / offers (nothing in FFA, so FFA hashes are unchanged).
        if let Some(p) = &self.party {
            p.hash_into(&mut h);
        }
        h.0
    }
}
