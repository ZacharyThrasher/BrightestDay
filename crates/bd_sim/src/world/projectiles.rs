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
        let id = self.spawn_projectile(s, view_tick)?;
        // CONNECT: a player's shot bends lightly toward the enemy nearest their crosshair.
        if let Some(shooter) = inherit {
            // SIGNATURE HOOK: inside their own domain the caster's shots can't miss (maxed assist).
            let turn = crate::signature::domain::aim(self, shooter).proj_turn(s.kind, s.explode.is_some());
            if turn > 0.0
                && let Some(target) = self.home_target(shooter, view_tick)
                && let Some(p) = self.projectiles.last_mut().filter(|p| p.id == id)
            {
                p.home = Some(target);
                p.home_turn = turn;
            }
        }
        Some(id)
    }

    /// CONNECT: the enemy nearest `shooter`'s crosshair inside the homing cone / range, in clear
    /// sight (terrain and enemy constructs block), seen as the shooter saw them (`view_tick`).
    fn home_target(&self, shooter: PlayerId, view_tick: Option<f64>) -> Option<PlayerId> {
        let aim = crate::signature::domain::aim(self, shooter); // SIGNATURE HOOK: domain sure hit
        let base = &self.cfg.move_params;
        let p = self.player(shooter)?;
        let (eye, look) = (p.eye(base), p.look());
        let view = view_tick.and_then(|v| self.history.clamp_view(v));
        let cands = self.players.iter().filter(|q| q.id != shooter && q.alive && !q.shielded()).map(|q| {
            let hull = view.and_then(|v| self.history.hull_at(q.id, v)).unwrap_or_else(|| q.hull(base));
            (q.id, hull)
        });
        let clear = |a: Vec3, b: Vec3| {
            let d = b - a;
            let len = d.length();
            len < 1e-3 || (raycast(&self.voxels, a, d / len, len).is_none() && self.constructs.segment_query(a, b, 0.05, Some(shooter)).is_none())
        };
        crate::aim_assist::pick_target(eye, look, aim.proj_cone_deg.to_radians().cos(), aim.proj_range, cands, clear).map(|t| t.0)
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
            home: None,
            home_turn: 0.0,
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
        // POWERS HOOK: an enemy's slow field (Infinity) — the round crawls near them (powers::effects).
        bent |= self.powers_slow_projectile(p.spec.owner, p.pos, &mut p.vel);
        if p.spec.effects & (effects::NEEDLE | effects::TENDRILS) != 0 {
            self.legend_projectile(p); // LEGENDARY: needles seek; BFG tendrils zap (world/legend.rs)
        }
        // CONNECT: light homing, strongest at release, only with a clear line (`aim_assist`).
        if let Some(id) = p.home {
            let aim = crate::signature::domain::aim(self, p.spec.owner); // SIGNATURE HOOK: domain sure hit
            // LEGENDARY: needles never stop homing.
            let turn = if p.spec.effects & effects::NEEDLE != 0 { p.home_turn } else { crate::aim_assist::decayed(p.home_turn, p.age, aim.proj_decay_secs) };
            let base = &self.cfg.move_params;
            let at = self.player(id).filter(|t| t.alive && !t.shielded()).map(|t| match rewind {
                Some(v) => self.history.hull_at(id, v).unwrap_or_else(|| t.hull(base)).center(),
                None => t.hull(base).center(),
            });
            match at.filter(|_| turn > 0.0) {
                None => p.home = None,
                Some(at) => {
                    let d = at - p.pos;
                    let len = d.length();
                    if len > 1e-3 && raycast(&self.voxels, p.pos, d / len, len).is_none() {
                        match crate::aim_assist::steer(p.vel, p.pos, at, turn * dt, aim.proj_release_deg.to_radians()) {
                            Some(v) => {
                                p.vel = v;
                                bent = true;
                            }
                            None => p.home = None,
                        }
                    }
                }
            }
        }
        let g = p.spec.kind.gravity();
        if g > 0.0 {
            // MAP HOOK: gravity by altitude. Off the plain ballistic curve => re-published like a bent shot.
            let k = self.arena.gravity_scale_at(p.pos.y);
            if k != 1.0 {
                bent = true;
            }
            p.vel.y -= g * k * dt;
        }
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
                if s.effects & effects::NEEDLE != 0 {
                    self.needle_stick(&s, pid, point, p.vel.normalize_or_zero()); // LEGENDARY: stick + stack
                }
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
                // LEGENDARY: a parried round flies back along the parrier's aim, bending onto the
                // enemy nearest their crosshair (CONNECT homing) — skill, not an auto-return.
                let to_aim = self.cfg.tuning.legendary.reflect_to_aim;
                if to_aim {
                    dir = look.normalize_or_zero();
                }
                if dir == Vec3::ZERO {
                    dir = -p.vel.normalize_or_zero();
                }
                self.redirect_projectile(p, target, None, point + dir * (s.radius + 0.4), dir * speed);
                if to_aim {
                    let turn = self.cfg.tuning.assist.aim.proj_turn(s.kind, s.explode.is_some()).max(1.5);
                    if let Some(t) = self.home_target(target, None) {
                        p.home = Some(t);
                        p.home_turn = turn;
                    }
                }
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
        p.home = None;
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
        self.ray_effects(s.owner, s.effects, target, point);
    }

    /// On-hit side effects a round carries (`projectile::effects` bits); shared by projectiles and
    /// hitscan rounds.
    fn ray_effects(&mut self, owner: PlayerId, fx: u8, target: PlayerId, point: Vec3) {
        if fx & effects::SHRINK != 0 {
            self.resize_player(target, SHRINK_SCALE, RESIZE_SECS);
        }
        if fx & effects::GROW != 0 {
            self.resize_player(target, GROW_SCALE, RESIZE_SECS);
        }
        if fx & effects::LINK != 0 {
            let ground = self.ground_point_below(point).unwrap_or(point);
            self.create_link(Some(owner), LinkEnd::Player(target), LinkEnd::Point(ground), LINK_MAX_LEN, LINK_LIFETIME);
        }
        if fx & effects::PULL != 0 {
            let to = self.player(owner).map(|o| o.mv.pos);
            if let (Some(to), Some(i)) = (to, self.pidx(target)) {
                let d = (to - self.players[i].mv.pos).normalize_or_zero();
                self.players[i].mv.apply_impulse(d * 12.0 + Vec3::Y * 3.0, 0.0);
            }
        }
    }

    /// Recoil: an impulse opposite the aim (×0.35 on the ground, ×1.0 in the air), never a
    /// tumble — shotgun-jumps, rocket-jumps, minigun hovering (COMBAT.md §4b).
    pub fn recoil(&mut self, player: PlayerId, aim: Vec3, strength: f32) {
        let ground_recoil = self.cfg.tuning.ring.ground_recoil;
        let Some(i) = self.pidx(player) else { return };
        let p = &mut self.players[i];
        if !p.alive || !strength.is_finite() || strength <= 0.0 || p.seat.is_some_and(|s| !s.suit) {
            return;
        }
        let k = if p.mv.grounded { ground_recoil } else { 1.0 };
        let dv = -aim.normalize_or_zero() * (strength.min(40.0) * k);
        p.mv.apply_impulse(dv, 0.0);
    }

    /// Lag-compensated hitscan: railgun beams and every hitscan gun round (hand cannon, minigun,
    /// each super-shotgun pellet; COMBAT.md §4b). Other players are tested where the shooter's
    /// screen showed them (hulls rewound to the shooter's `view_tick`, ≤ 250 ms). Passes through
    /// up to `pierce` players and `wall` metres of terrain; the first enemy construct on the ray
    /// takes the hit (only a piercing ray continues through a thin one); a raised guard blocks
    /// (a perfect parry sends the round straight back at its shooter). Hits keep the gun's feel:
    /// distance falloff, the bounded momentum bonus (the shooter's own speed along the aim — what
    /// their movement added), head-zone crits, knockback, terrain chips. Returns players hit.
    pub fn hitscan(&mut self, s: crate::weapons::HitscanShot) -> u8 {
        let s = self.magnetize(s);
        self.hitscan_ray(s, false)
    }

    /// CONNECT: bullet magnetism. A round that misses every body (as the shooter saw them) by at most
    /// the gun's angle (`assist.aim.hitscan`, beams `hitscan_beam_deg`) is bent onto the nearest one,
    /// if the bent ray is clear of terrain and enemy constructs. Shotgun pellets: none.
    fn magnetize(&self, mut s: crate::weapons::HitscanShot) -> crate::weapons::HitscanShot {
        let aim = crate::signature::domain::aim(self, s.owner); // SIGNATURE HOOK: domain sure hit
        let max = aim.hitscan_rad(s.trace);
        let dir = s.dir.normalize_or_zero();
        if !(max > 0.0) || dir == Vec3::ZERO || !s.origin.is_finite() {
            return s;
        }
        let range = if s.range.is_finite() { s.range.clamp(1.0, 400.0) } else { 250.0 }.min(aim.hitscan_range);
        let end = raycast(&self.voxels, s.origin, dir, range).map_or(range, |h| h.t);
        let view = self.player(s.owner).and_then(|p| p.view_tick).and_then(|v| self.history.clamp_view(v));
        let base = &self.cfg.move_params;
        let mut best: Option<(Vec3, f32)> = None;
        for pl in &self.players {
            if pl.id == s.owner || !pl.alive || pl.shielded() {
                continue;
            }
            let hull = view.and_then(|v| self.history.hull_at(pl.id, v)).unwrap_or_else(|| pl.hull(base));
            if geom::segment_aabb(s.origin, s.origin + dir * end, &hull).is_some() {
                return s; // a direct hit: nothing to help
            }
            let Some((at, ang)) = crate::aim_assist::magnet_point(s.origin, dir, &hull, max, range) else { continue };
            if best.is_some_and(|(_, b)| ang >= b) {
                continue;
            }
            let d = at - s.origin;
            let len = d.length();
            let clear = raycast(&self.voxels, s.origin, d / len, len).is_none() && self.constructs.segment_query(s.origin, at, 0.05, Some(s.owner)).is_none();
            if clear {
                best = Some((at, ang));
            }
        }
        if let Some((at, _)) = best {
            s.dir = (at - s.origin).normalize_or(dir);
        }
        s
    }

    fn hitscan_ray(&mut self, s: crate::weapons::HitscanShot, reflected: bool) -> u8 {
        self.hitscan_ray_hop(s, reflected, RayHop::default())
    }

    /// One straight leg of a hitscan ray. A leg that reaches a portal disc (front side, before any
    /// terrain / construct / stopping player) continues from the exit with the rest of the range
    /// (`PORTAL_HOPS` legs at most; players stay rewound to the shooter's view, the portals are where
    /// they are now). Falloff and the momentum bonus carry over from the first leg.
    fn hitscan_ray_hop(&mut self, s: crate::weapons::HitscanShot, reflected: bool, hop: RayHop) -> u8 {
        use crate::constructs::held::{Guarded, guard_check};
        use crate::events::HeldFx;
        let dir = s.dir.normalize_or_zero();
        if dir == Vec3::ZERO || !s.origin.is_finite() || !s.damage.is_finite() || !s.knockback.is_finite() {
            return 0;
        }
        let range = if s.range.is_finite() { s.range.clamp(1.0, 400.0) } else { 250.0 };
        // Terrain, punching through thin walls.
        let mut end = range;
        let mut normal = Vec3::ZERO;
        let mut on_terrain = false;
        let mut from = 0.0f32;
        let mut wall_left = if s.wall.is_finite() { s.wall.clamp(0.0, 4.0) } else { 0.0 };
        for _ in 0..8 {
            let Some(h) = raycast(&self.voxels, s.origin + dir * from, dir, range - from) else { break };
            let t_hit = from + h.t;
            let n = if h.normal == bd_core::IVec3::ZERO { -dir } else { h.normal.as_vec3() };
            let mut t = t_hit + 0.01;
            let mut thick = 0.0;
            if wall_left > 0.0 {
                while t < range && self.voxels.is_solid(bd_core::voxel::world_to_voxel(s.origin + dir * t)) {
                    t += 0.1;
                    thick += 0.1;
                    if thick > wall_left {
                        break;
                    }
                }
            }
            if wall_left <= 0.0 || thick > wall_left {
                end = t_hit;
                normal = n;
                on_terrain = true;
                break;
            }
            wall_left -= thick;
            from = t;
        }
        // PORTALS: a disc on the ray before the terrain stop ends this leg there (continued below).
        let mut through: Option<(usize, usize)> = None;
        if hop.legs < crate::space::PORTAL_HOPS && !self.portals.is_empty() {
            let mut best = f32::INFINITY;
            for (k, pp) in self.portals.iter().enumerate() {
                if let Some((t, entry)) = pp.ray_hit(s.origin, dir, end) {
                    if t < best {
                        best = t;
                        through = Some((k, entry));
                    }
                }
            }
            if through.is_some() {
                end = best;
                normal = Vec3::ZERO;
                on_terrain = false;
            }
        }
        // First enemy construct on the ray takes the hit; only a piercing ray goes on through a thin one.
        if let Some((cid, t, n)) = self.constructs.segment_query(s.origin, s.origin + dir * end, 0.05, Some(s.owner)) {
            let at = s.origin + dir * (t * end);
            let thin = self.constructs.get(cid).is_some_and(|c| c.half.min_element() * 2.0 <= 1.0);
            self.apply_construct_hit(ConstructHit { pierce: s.pierce > 0, blunt: false, target: cid, source: Some(s.owner), damage: s.damage, impulse: dir * s.knockback, blade: false, point: at });
            if !(thin && s.pierce > 0) {
                end *= t;
                normal = if n.is_finite() && n != Vec3::ZERO { n } else { -dir };
                on_terrain = false;
                through = None;
            }
        }
        // Players, rewound to what the shooter saw (a parried round flies in the host's present).
        let view = if reflected { None } else { self.player(s.owner).and_then(|p| p.view_tick).and_then(|v| self.history.clamp_view(v)) };
        let base = &self.cfg.move_params;
        let mut hits: [(f32, PlayerId, Vec3); bd_core::MAX_PLAYERS] = [(0.0, PlayerId(0), Vec3::ZERO); bd_core::MAX_PLAYERS];
        let mut n = 0;
        let p1 = s.origin + dir * end;
        for pl in &self.players {
            if pl.id == s.owner || !pl.alive || n >= hits.len() {
                continue;
            }
            let now = pl.hull(base);
            let hull = match view {
                Some(v) => self.history.hull_at(pl.id, v).unwrap_or(now),
                None => now,
            };
            if let Some((t, _)) = geom::segment_aabb(s.origin, p1, &hull) {
                // Body-relative shift (rewound hull → current hull) so the head zone reads the part
                // of the body the shooter actually hit.
                hits[n] = (t, pl.id, now.min - hull.min);
                n += 1;
            }
        }
        hits[..n].sort_by(|a, b| a.0.total_cmp(&b.0));
        let limit = s.pierce as usize + 1;
        // Bounded momentum bonus: the shooter's own velocity along the aim (what their movement added).
        let closing = hop.closing.unwrap_or_else(|| self.player(s.owner).map_or(0.0, |p| p.mv.vel.dot(dir)).max(0.0));
        let impact = if s.trace.is_some() { 1.0 + s.damage / 40.0 } else { 1.6 };
        let mut taken = 0usize;
        let mut last: Option<(f32, PlayerId)> = None;
        let mut stopped = false;
        for &(t, pid, shift) in &hits[..n] {
            if taken >= limit {
                break;
            }
            let point = s.origin + (p1 - s.origin) * t;
            // HELD ENGINEER HOOK: a raised guard facing the shot stops the round (COMBAT.md §4, §8).
            let view_of = self.player(pid).map(|tp| (tp.eye(&self.cfg.move_params), tp.look()));
            let guard = view_of.and_then(|(eye, look)| guard_check(&self.constructs, pid, eye, look, s.origin, s.pierce > 0).map(|g| (g, eye)));
            if let Some(((verdict, gid), eye)) = guard {
                match verdict {
                    Guarded::Parry => {
                        self.events.push(GameEvent::HeldFx { id: gid, owner: pid, fx: HeldFx::Parry, pos: point, dir: -dir, strength: 1.5 });
                        let back = self.player(s.owner).filter(|o| o.alive).map(|o| o.hull(&self.cfg.move_params).center());
                        if let (false, Some(b)) = (reflected, back) {
                            let mut r = s;
                            r.owner = pid;
                            r.construct = None;
                            r.origin = eye;
                            r.muzzle = point;
                            r.dir = (b - eye).normalize_or_zero();
                            r.pierce = 0;
                            r.wall = 0.0;
                            // LEGENDARY: a deflected hitscan round goes back at the shooter, capped.
                            let cap = self.cfg.tuning.legendary.reflect_hitscan_cap;
                            r.damage = r.damage.min(cap);
                            r.knockback = r.knockback.min(cap);
                            self.hitscan_ray(r, true);
                        }
                    }
                    Guarded::Block => {
                        self.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: gid, source: Some(s.owner), damage: s.damage * 0.5, impulse: Vec3::ZERO, blade: false, point });
                        self.events.push(GameEvent::HeldFx { id: gid, owner: pid, fx: HeldFx::Block, pos: point, dir, strength: s.damage / 10.0 });
                    }
                }
                last = Some((t, pid));
                stopped = true;
                break;
            }
            let fall = crate::weapons::falloff(s.falloff_start, hop.dist + (point - s.origin).length());
            let at = point + shift;
            let mut hit = HitInfo::new(pid, Some(s.owner), HitKind::Shot, s.damage * fall, s.knockback * fall, dir, at)
                .with_element(s.element)
                .with_closing(closing)
                .with_impact(impact);
            hit.construct = s.construct;
            self.apply_hit(hit);
            if s.effects & effects::PULL != 0 {
                // (Shrink / Grow / Link ride the delivering construct in `apply_hit`.)
                self.ray_effects(s.owner, effects::PULL, pid, at);
            }
            taken += 1;
            last = Some((t, pid));
        }
        if let Some((t, _)) = last.filter(|_| stopped || taken == limit) {
            end *= t.max(0.0);
            normal = -dir;
            on_terrain = false;
            through = None;
        }
        let stop = s.origin + dir * end;
        if on_terrain && s.chip > 0.0 {
            self.request_carve(CarveRequest { center: stop, radius: s.chip, power: 0.4 });
        }
        match s.trace {
            Some(archetype) => {
                let from = if s.muzzle == Vec3::ZERO || !s.muzzle.is_finite() { s.origin } else { s.muzzle };
                let hit_player = if taken > 0 { last.map(|l| l.1) } else { None };
                self.events.push(GameEvent::ShotTrace { owner: s.owner, archetype, from, to: stop, normal, hit_player });
            }
            None => self.events.push(GameEvent::Hitscan { owner: s.owner, origin: s.origin, end: stop, hits: taken as u8 }),
        }
        // Through the portal: the next leg leaves the exit disc with the rest of the range.
        if let Some((k, entry)) = through {
            let pp = self.portals[k];
            let exit = pp.ends[1 - entry];
            let out = pp.map_point(entry, stop) + exit.normal * 0.02;
            let left = range - end;
            let pierce_left = (s.pierce as usize + 1).saturating_sub(taken);
            if left > 0.5 && pierce_left > 0 && out.is_finite() {
                let mut n = s;
                n.origin = out;
                n.muzzle = out;
                n.dir = pp.rotation(entry) * dir;
                n.range = left;
                n.pierce = (pierce_left - 1).min(255) as u8;
                let next = RayHop { legs: hop.legs + 1, dist: hop.dist + end, closing: Some(closing) };
                return taken as u8 + self.hitscan_ray_hop(n, reflected, next);
            }
        }
        taken as u8
    }
}

/// State carried from one hitscan leg to the next (through a portal).
#[derive(Clone, Copy, Debug, Default)]
struct RayHop {
    /// Portal passes so far.
    legs: u8,
    /// Distance travelled before this leg (falloff reads the whole path).
    dist: f32,
    /// The first leg's momentum bonus.
    closing: Option<f32>,
}
