//! Projectile stepping: swept sphere vs players (optionally rewound for lag compensation),
//! constructs and terrain — earliest contact wins, so nothing tunnels at 90 m/s. Portals and force
//! fields bend them; bounces and portal exits start a new replicated ballistic segment.

use bd_core::voxel::{VoxelQuery, raycast};
use bd_core::{DT, PlayerId, Vec3};

use super::World;
use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo, HitKind};
use crate::events::{GameEvent, Traveller};
use crate::geom;
use crate::history::MAX_REWIND_TICKS;
use crate::projectile::{Projectile, ProjectileKind, ProjectileSpawn, effects};
use crate::space::{GROW_SCALE, LINK_LIFETIME, LINK_MAX_LEN, LinkEnd, PORTAL_COOLDOWN, RESIZE_SECS, SHRINK_SCALE};

enum Contact {
    Player(PlayerId, Vec3),
    Construct(bd_core::ConstructId, Vec3, Vec3),
    Terrain(Vec3, Vec3),
}

impl World {
    /// Fire a projectile. With a `view_tick` (the shooter's interpolated client view) the shot is
    /// lag-compensated for its whole flight: other players are tested where the shooter's screen
    /// showed them (`tick − lag`, ≤ 250 ms), so what the shooter saw is what hits.
    /// Fire a projectile from a player's gun / ring: inherits 100 % of `inherit`'s velocity
    /// (guns are movement tools; COMBAT.md §4b).
    pub fn fire_projectile(&mut self, mut s: ProjectileSpawn, view_tick: Option<f64>, inherit: Option<PlayerId>) -> Option<u16> {
        if s.muzzle_speed <= 0.0 {
            s.muzzle_speed = s.vel.length();
        }
        if let Some(v) = inherit.and_then(|p| self.player(p)).map(|p| p.mv.vel).filter(|v| v.is_finite()) {
            s.vel += v;
        }
        self.spawn_projectile(s, view_tick)
    }

    pub fn spawn_projectile(&mut self, s: ProjectileSpawn, view_tick: Option<f64>) -> Option<u16> {
        if self.projectiles.len() >= super::MAX_PROJECTILES || !s.pos.is_finite() || !s.vel.is_finite() {
            return None;
        }
        let id = self.next_projectile;
        self.next_projectile = self.next_projectile.wrapping_add(1).max(1);
        let mut spec = s;
        spec.radius = spec.radius.clamp(0.02, 2.0);
        spec.life = spec.life.clamp(0.05, 10.0);
        if !spec.muzzle_speed.is_finite() || spec.muzzle_speed <= 0.0 {
            spec.muzzle_speed = s.vel.length();
        }
        let lag = view_tick
            .and_then(|v| self.history.clamp_view(v))
            .map_or(0.0, |v| (self.tick as f64 - v).clamp(0.0, MAX_REWIND_TICKS) as f32);
        let p = Projectile {
            id,
            spec,
            pos: s.pos,
            vel: s.vel,
            age: 0.0,
            origin: s.pos,
            vel0: s.vel,
            segment_tick: self.tick,
            lag,
            portal_cd: 0.0,
            travelled: 0.0,
            dead: false,
        };
        self.events.push(GameEvent::ProjectileFired { id, owner: s.owner, kind: s.kind, origin: s.pos, vel: s.vel });
        self.projectiles.push(p);
        Some(id)
    }

    pub(crate) fn step_projectiles(&mut self, _dt: f32) {
        for i in 0..self.projectiles.len() {
            let mut p = self.projectiles[i];
            if p.dead {
                continue;
            }
            let rewind = (p.lag > 0.0).then_some(self.tick as f64 - p.lag as f64);
            self.advance_projectile(&mut p, rewind);
            self.projectiles[i] = p;
        }
        self.projectiles.retain(|p| !p.dead);
    }

    fn end_projectile(&mut self, p: &mut Projectile, pos: Vec3, normal: Vec3, hit_player: Option<PlayerId>) {
        p.dead = true;
        self.events.push(GameEvent::ProjectileEnded { id: p.id, pos, normal, hit_player });
    }

    /// One tick of flight. `rewind`: test players at this (fractional) history tick instead of now.
    fn advance_projectile(&mut self, p: &mut Projectile, rewind: Option<f64>) {
        let dt = DT;
        p.age += dt;
        p.portal_cd = (p.portal_cd - dt).max(0.0);
        if p.age > p.spec.life || !self.in_play(p.pos, 20.0) {
            let pos = p.pos;
            self.end_projectile(p, pos, Vec3::ZERO, None);
            return;
        }
        // Fields bend shots (not the owner's own field).
        let mut bent = false;
        for f in &self.fields {
            if f.owner != p.spec.owner {
                let a = f.accel_at(p.pos);
                if a != Vec3::ZERO {
                    p.vel += a * dt;
                    bent = true;
                }
            }
        }
        p.vel.y -= p.spec.kind.gravity() * dt;
        let p0 = p.pos;
        let p1 = p0 + p.vel * dt;
        let r = p.spec.radius;
        // Earliest contact.
        let mut best_t = f32::INFINITY;
        let mut contact: Option<Contact> = None;
        let base = &self.cfg.move_params;
        for pl in &self.players {
            if pl.id == p.spec.owner || !pl.alive {
                continue;
            }
            let hull = match rewind {
                Some(v) => self.history.hull_at(pl.id, v).unwrap_or_else(|| pl.hull(base)),
                None => pl.hull(base),
            };
            if let Some((t, _)) = geom::segment_aabb(p0, p1, &geom::inflate(&hull, r)) {
                if t < best_t {
                    best_t = t;
                    contact = Some(Contact::Player(pl.id, p0 + (p1 - p0) * t));
                }
            }
        }
        if let Some((cid, t, n)) = self.constructs.segment_query(p0, p1, r, Some(p.spec.owner)) {
            if t < best_t {
                best_t = t;
                contact = Some(Contact::Construct(cid, p0 + (p1 - p0) * t, n));
            }
        }
        let seg = p1 - p0;
        let len = seg.length();
        if len > 1e-6 {
            if let Some(h) = raycast(&self.voxels, p0, seg, len) {
                let t = h.t / len;
                if t < best_t {
                    best_t = t;
                    let n = if h.normal == bd_core::IVec3::ZERO { -seg / len } else { h.normal.as_vec3() };
                    contact = Some(Contact::Terrain(h.point, n));
                }
            }
        }
        // Portal crossing before anything else on the segment?
        if p.portal_cd <= 0.0 {
            let mut transit = None;
            for pp in &self.portals {
                if let Some(t) = pp.crossing(p0, p1) {
                    transit = Some((pp.id, t, *pp.exit(&t)));
                    break;
                }
            }
            if let Some((pid, t, exit)) = transit {
                // Only if the portal comes before any contact on this segment.
                let entry = t.entry;
                let plane_t = {
                    let pair = self.portals.iter().find(|q| q.id == pid);
                    pair.map(|q| {
                        let e = &q.ends[entry];
                        let a = (p0 - e.pos).dot(e.normal);
                        let b = (p1 - e.pos).dot(e.normal);
                        a / (a - b).max(1e-6)
                    })
                };
                if plane_t.is_some_and(|pt| pt <= best_t) {
                    p.pos = t.point + exit.normal * (r + 0.05);
                    p.vel = t.rot * p.vel;
                    p.portal_cd = PORTAL_COOLDOWN;
                    self.restart_segment(p);
                    self.events.push(GameEvent::Teleported { who: Traveller::Projectile(p.id), portal: pid, rot: t.rot, pos: p.pos });
                    return;
                }
            }
        }
        match contact {
            None => {
                p.pos = p1;
                p.travelled += (p1 - p0).length();
                // Bent paths are re-published at 20 Hz so clients stay close.
                if bent && self.tick.saturating_sub(p.segment_tick) >= 6 {
                    self.restart_segment(p);
                }
            }
            Some(Contact::Player(pid, point)) => {
                // HELD ENGINEER HOOK: a raised Guard (shield / sword parry / Guard suit) facing the
                // shot blocks it; a perfect parry reflects it at its shooter (COMBAT.md §4, §8).
                if self.guard_projectile(p, pid, p0, point) {
                    return;
                }
                if p.spec.effects & effects::PORTAL != 0 {
                    // Portal rounds are utility: they only open portals on surfaces.
                    self.end_projectile(p, point, -p.vel.normalize_or_zero(), Some(pid));
                    return;
                }
                let s = p.spec;
                p.travelled += (point - p0).length();
                if let Some((radius, carve)) = s.explode {
                    self.explode(Explosion {
                        pos: point,
                        radius,
                        damage: s.damage,
                        knockback: s.knockback,
                        source: Some(s.owner),
                        construct: s.construct,
                        element: s.element,
                        carve_radius: carve,
                        self_damage: false,
                    });
                } else {
                    // Pellet falloff beyond `falloff_start` (to 30 % at +18 m).
                    let fall = if s.falloff_start > 0.0 { (1.0 - (p.travelled - s.falloff_start).max(0.0) / 18.0).max(0.3) } else { 1.0 };
                    // Momentum bonus: projectile-vs-target closing speed beyond the muzzle speed.
                    let tv = self.player(pid).map_or(Vec3::ZERO, |t| t.mv.vel);
                    let dirn = p.vel.normalize_or_zero();
                    let closing = ((p.vel - tv).dot(dirn) - s.muzzle_speed).max(0.0);
                    let mut hit = HitInfo::new(pid, Some(s.owner), s.hit_kind, s.damage * fall, s.knockback * fall, p.vel, point)
                        .with_element(s.element)
                        .with_closing(closing);
                    hit.construct = s.construct;
                    hit.impact = if s.kind == ProjectileKind::RingBlast { 0.9 } else { 1.0 + s.damage / 40.0 };
                    self.apply_hit(hit);
                }
                self.projectile_effects(&s, pid, point);
                self.end_projectile(p, point, -p.vel.normalize_or_zero(), Some(pid));
            }
            Some(Contact::Construct(cid, point, n)) => {
                if self.deflect_projectile(p, cid, point, n) {
                    return;
                }
                let s = p.spec;
                if s.effects & effects::PORTAL != 0 {
                    let fixed = self.constructs.get(cid).is_some_and(|c| c.category == bd_core::mechanics::FormCategory::Structure && !c.body.loose);
                    if fixed {
                        self.place_portal_end(s.owner, point, n, p.vel);
                    }
                    self.end_projectile(p, point, n, None);
                    return;
                }
                // The firing construct's traits (a Pierce rifle finds weak points…).
                let (blade, pierce, blunt) = s.construct.and_then(|c| self.constructs.get(c)).map_or((false, false, false), crate::constructs::components::hit_traits);
                self.apply_construct_hit(ConstructHit { pierce, blunt, target: cid, source: Some(s.owner), damage: s.damage, impulse: p.vel.normalize_or_zero() * s.knockback, blade, point });
                if let Some((radius, carve)) = s.explode {
                    self.explode(Explosion { pos: point, radius, damage: s.damage, knockback: s.knockback, source: Some(s.owner), construct: s.construct, element: s.element, carve_radius: carve, self_damage: false });
                }
                self.end_projectile(p, point, n, None);
            }
            Some(Contact::Terrain(point, n)) => {
                let s = p.spec;
                if s.effects & effects::PORTAL != 0 {
                    self.place_portal_end(s.owner, point, n, p.vel);
                    self.end_projectile(p, point, n, None);
                    return;
                }
                if s.ricochet > 0 {
                    p.spec.ricochet -= 1;
                    p.vel -= n * (2.0 * p.vel.dot(n));
                    p.pos = point + n * (r + 0.02);
                    self.restart_segment(p);
                    return;
                }
                if let Some((radius, carve)) = s.explode {
                    self.explode(Explosion { pos: point, radius, damage: s.damage, knockback: s.knockback, source: Some(s.owner), construct: s.construct, element: s.element, carve_radius: carve, self_damage: false });
                } else if s.chip > 0.0 {
                    self.request_carve(CarveRequest { center: point, radius: s.chip, power: 0.4 });
                } else if self.cfg.blast_chips_terrain && s.kind == ProjectileKind::RingBlast {
                    self.request_carve(CarveRequest { center: point, radius: 0.35, power: 0.3 });
                }
                self.end_projectile(p, point, n, None);
            }
        }
    }

    /// HELD ENGINEER HOOK (see `constructs::held::guard_check`): returns true when `target`'s guard
    /// consumed the projectile — parried (reflected back at its shooter, now owned by the parrier)
    /// or blocked (absorbed; the guard takes integrity damage; explosive rounds still burst).
    fn guard_projectile(&mut self, p: &mut Projectile, target: PlayerId, from: Vec3, point: Vec3) -> bool {
        use crate::constructs::held::{Guarded, guard_check};
        use crate::events::HeldFx;
        let base = &self.cfg.move_params;
        let Some((eye, look)) = self.player(target).map(|t| (t.eye(base), t.look())) else { return false };
        let Some((verdict, gid)) = guard_check(&self.constructs, target, eye, look, from, false) else { return false };
        let s = p.spec;
        match verdict {
            Guarded::Parry => {
                let speed = p.vel.length().max(s.muzzle_speed).max(10.0);
                let back = self.player(s.owner).filter(|o| o.alive).map(|o| o.hull(base).center());
                let mut dir = back.map_or(Vec3::ZERO, |b| (b - point).normalize_or_zero());
                if dir == Vec3::ZERO {
                    dir = -p.vel.normalize_or_zero();
                }
                self.redirect_projectile(p, target, None, point + dir * (s.radius + 0.4), dir * speed);
                self.events.push(GameEvent::HeldFx { id: gid, owner: target, fx: HeldFx::Parry, pos: point, dir, strength: 1.5 });
            }
            Guarded::Block => {
                self.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: gid, source: Some(s.owner), damage: s.damage * 0.5, impulse: Vec3::ZERO, blade: false, point });
                if let Some((radius, carve)) = s.explode {
                    self.explode(Explosion { pos: point, radius, damage: s.damage * 0.5, knockback: s.knockback, source: Some(s.owner), construct: s.construct, element: s.element, carve_radius: carve, self_damage: false });
                }
                self.events.push(GameEvent::HeldFx { id: gid, owner: target, fx: HeldFx::Block, pos: point, dir: p.vel.normalize_or_zero(), strength: s.damage / 10.0 });
                self.end_projectile(p, point, -p.vel.normalize_or_zero(), None);
            }
        }
        true
    }

    /// Turn a projectile around: new owner (and construct), new position / velocity, fresh flight
    /// (no lag compensation — it's the host's shot now). Shared by parries and deflecting constructs.
    fn redirect_projectile(&mut self, p: &mut Projectile, owner: PlayerId, construct: Option<bd_core::ConstructId>, pos: Vec3, vel: Vec3) {
        p.vel = vel;
        p.pos = pos;
        p.spec.owner = owner;
        p.spec.construct = construct;
        p.spec.muzzle_speed = vel.length();
        p.lag = 0.0;
        p.age = 0.0;
        p.travelled = 0.0;
        self.restart_segment(p);
    }

    /// Guard (front face) / Bounce (any face) constructs reflect projectiles along the surface
    /// normal; the round now belongs to the construct's owner. A Guard takes a little integrity
    /// damage; Bounce is free. Returns true if deflected.
    fn deflect_projectile(&mut self, p: &mut Projectile, cid: bd_core::ConstructId, point: Vec3, n: Vec3) -> bool {
        use crate::constructs::components::eff_has;
        use bd_core::mechanics::{Form, Mechanic};
        let Some(c) = self.constructs.get(cid) else { return false };
        if !c.active() || !n.is_finite() {
            return false;
        }
        let bounce = eff_has(c, Mechanic::Bounce) || c.recipe.form == Form::Trampoline;
        let guard = (eff_has(c, Mechanic::Guard) || c.recipe.form == Form::Shield) && n.dot(c.rot * Vec3::NEG_Z) > 0.5;
        if !(bounce || guard) {
            return false;
        }
        let owner = c.owner;
        let n = n.normalize_or(Vec3::Y);
        let speed = p.vel.length().max(10.0);
        let mut dir = (p.vel - n * (2.0 * p.vel.dot(n))).normalize_or(n);
        if dir.dot(n) <= 0.05 {
            dir = n;
        }
        let r = p.spec.radius;
        let (id, s) = (p.id, p.spec);
        self.redirect_projectile(p, owner, Some(cid), point + n * (r + 0.05), dir * speed);
        if guard && !bounce {
            self.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: cid, source: Some(s.owner), damage: s.damage * 0.25, impulse: Vec3::ZERO, blade: false, point });
        }
        self.events.push(GameEvent::Deflected { construct: cid, projectile: id, pos: point, dir, guard: guard && !bounce });
        true
    }

    fn restart_segment(&mut self, p: &mut Projectile) {
        p.origin = p.pos;
        p.vel0 = p.vel;
        p.segment_tick = self.tick;
        self.events.push(GameEvent::ProjectileFired { id: p.id, owner: p.spec.owner, kind: p.spec.kind, origin: p.pos, vel: p.vel });
    }

    fn projectile_effects(&mut self, s: &ProjectileSpawn, target: PlayerId, point: Vec3) {
        if s.effects & effects::SHRINK != 0 {
            self.resize_player(target, SHRINK_SCALE, RESIZE_SECS);
        }
        if s.effects & effects::GROW != 0 {
            self.resize_player(target, GROW_SCALE, RESIZE_SECS);
        }
        if s.effects & effects::LINK != 0 {
            let ground = self.ground_point_below(point).unwrap_or(point);
            self.create_link(Some(s.owner), LinkEnd::Player(target), LinkEnd::Point(ground), LINK_MAX_LEN, LINK_LIFETIME);
        }
        if s.effects & effects::PULL != 0 {
            let to = self.player(s.owner).map(|o| o.mv.pos);
            if let (Some(to), Some(i)) = (to, self.pidx(target)) {
                let d = (to - self.players[i].mv.pos).normalize_or_zero();
                self.players[i].mv.apply_impulse(d * 12.0 + Vec3::Y * 3.0, 0.0);
            }
        }
    }

    /// Recoil: an impulse opposite the aim (×0.35 on the ground, ×1.0 in the air), never a
    /// tumble — shotgun-jumps, rocket-jumps, minigun hovering (COMBAT.md §4b).
    pub fn recoil(&mut self, player: PlayerId, aim: Vec3, strength: f32) {
        let Some(i) = self.pidx(player) else { return };
        let p = &mut self.players[i];
        if !p.alive || !strength.is_finite() || strength <= 0.0 || p.seat.is_some_and(|s| !s.suit) {
            return;
        }
        let k = if p.mv.grounded { crate::combat::GROUND_RECOIL } else { 1.0 };
        let dv = -aim.normalize_or_zero() * (strength.min(40.0) * k);
        p.mv.apply_impulse(dv, 0.0);
    }

    /// Lag-compensated hitscan (railgun): passes through up to `pierce` players and `wall` metres
    /// of terrain, damages the first enemy construct it meets. Returns players hit.
    pub fn hitscan(&mut self, s: crate::weapons::HitscanShot) -> u8 {
        let dir = s.dir.normalize_or_zero();
        if dir == Vec3::ZERO || !s.origin.is_finite() || !s.damage.is_finite() {
            return 0;
        }
        let range = s.range.clamp(1.0, 400.0);
        // Terrain, punching through thin walls.
        let mut end = range;
        let mut from = 0.0f32;
        let mut wall_left = s.wall.clamp(0.0, 4.0);
        for _ in 0..8 {
            let Some(h) = raycast(&self.voxels, s.origin + dir * from, dir, range - from) else { break };
            let t_hit = from + h.t;
            let mut t = t_hit + 0.01;
            let mut thick = 0.0;
            while t < range && self.voxels.is_solid(bd_core::voxel::world_to_voxel(s.origin + dir * t)) {
                t += 0.1;
                thick += 0.1;
                if thick > wall_left {
                    break;
                }
            }
            if thick > wall_left {
                end = t_hit;
                break;
            }
            wall_left -= thick;
            from = t;
        }
        // First enemy construct on the beam takes the hit and stops it unless it is thin.
        if let Some((cid, t, _)) = self.constructs.segment_query(s.origin, s.origin + dir * end, 0.05, Some(s.owner)) {
            let at = s.origin + dir * (t * end);
            let thin = self.constructs.get(cid).is_some_and(|c| c.half.min_element() * 2.0 <= 1.0);
            self.apply_construct_hit(ConstructHit { pierce: s.pierce > 0, blunt: false, target: cid, source: Some(s.owner), damage: s.damage, impulse: dir * s.knockback, blade: false, point: at });
            if !thin {
                end *= t;
            }
        }
        // Players, rewound to what the shooter saw.
        let view = self.player(s.owner).and_then(|p| p.view_tick).and_then(|v| self.history.clamp_view(v));
        let base = &self.cfg.move_params;
        let mut hits: [(f32, PlayerId); bd_core::MAX_PLAYERS] = [(0.0, PlayerId(0)); bd_core::MAX_PLAYERS];
        let mut n = 0;
        let p1 = s.origin + dir * end;
        for pl in &self.players {
            if pl.id == s.owner || !pl.alive || n >= hits.len() {
                continue;
            }
            let hull = match view {
                Some(v) => self.history.hull_at(pl.id, v).unwrap_or_else(|| pl.hull(base)),
                None => pl.hull(base),
            };
            if let Some((t, _)) = geom::segment_aabb(s.origin, p1, &hull) {
                hits[n] = (t, pl.id);
                n += 1;
            }
        }
        hits[..n].sort_by(|a, b| a.0.total_cmp(&b.0));
        let take = n.min(s.pierce as usize + 1);
        let shooter_vel = self.player(s.owner).map_or(Vec3::ZERO, |p| p.mv.vel);
        for &(t, pid) in &hits[..take] {
            let point = s.origin + (p1 - s.origin) * t;
            let tv = self.player(pid).map_or(Vec3::ZERO, |p| p.mv.vel);
            let mut hit = HitInfo::new(pid, Some(s.owner), HitKind::Shot, s.damage, s.knockback, dir, point)
                .with_element(s.element)
                .with_closing((shooter_vel - tv).dot(dir).max(0.0))
                .with_impact(1.6);
            hit.construct = s.construct;
            self.apply_hit(hit);
        }
        if take > 0 && take == s.pierce as usize + 1 {
            end *= hits[take - 1].0.max(0.0);
        }
        self.events.push(GameEvent::Hitscan { owner: s.owner, origin: s.origin, end: s.origin + dir * end, hits: take as u8 });
        take as u8
    }
}
