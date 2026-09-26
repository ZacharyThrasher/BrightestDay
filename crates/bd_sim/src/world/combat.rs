//! The one place hits are applied, plus explosions, wall splats, damage over time, construct
//! intents and the terrain carve queue.

use bd_core::mechanics::{Element, Mechanic};
use bd_core::{DT, PlayerId, Tick, VOXEL_SIZE, Vec3};
use bd_voxel::{CarveOp, CarveShape, CollapseLimits, VoxelWorld};

use super::World;
use crate::combat::*;
use crate::events::{GameEvent, KoCause};
use crate::geom;
use crate::space::{GROW_SCALE, LINK_MAX_LEN, LinkEnd, RESIZE_SECS, SHRINK_SCALE};
use crate::status::{StatusKind, element_statuses};
use crate::willpower::{OathState, oath_partial};

/// Carves bigger than this are split into sub-carves spread over [`PROGRESSIVE_TICKS`].
pub const PROGRESSIVE_RADIUS: f32 = 3.0;
pub const PROGRESSIVE_TICKS: u64 = 60;
/// Largest single carve request honoured (m).
pub const MAX_CARVE_RADIUS: f32 = 30.0;
/// Electric chain reach (doubled when wet).
pub const CHAIN_RADIUS: f32 = 6.0;

/// Collapse search bounds used for every replicated carve (host and clients must agree).
/// Pieces bigger than `max_component` count as supported (they float, Cube-World style).
pub const SIM_COLLAPSE_LIMITS: CollapseLimits = CollapseLimits { max_component: 6_000, max_total: 12_000 };

/// Apply a replicated carve exactly as the host did (use this on clients too).
pub fn apply_carve_op(voxels: &mut VoxelWorld, op: &CarveOp) -> bd_voxel::CarveOutcome {
    let removed = voxels.carve(op);
    let falling = voxels.detect_collapse(&removed, SIM_COLLAPSE_LIMITS);
    bd_voxel::CarveOutcome { removed, falling }
}

impl World {
    /// Apply a hit to a player: spawn shields, momentum bonus, damage (suits absorb first),
    /// knockback scaled by missing health / element / size, tumble, statuses, electric chain,
    /// oath interruption, KO credit, and a `Hit` event with the juice numbers.
    pub fn apply_hit(&mut self, hit: HitInfo) -> HitResult {
        let mut none = HitResult::default();
        if self.match_state.phase == crate::rules::MatchPhase::Ended {
            return none;
        }
        let Some(ti) = self.pidx(hit.target) else { return none };
        if !self.players[ti].alive {
            return none;
        }
        // POWERS HOOK: water drain / seastone marks, Logia scatter, immunities, rubber, Bara Bara.
        let Some(hit) = self.powers_on_hit(ti, hit) else { return none };
        let finite = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let base_damage = finite(hit.damage).min(500.0);
        let base_kb = finite(hit.knockback).min(120.0);
        let dir_in = if hit.impulse_dir.is_finite() { hit.impulse_dir } else { Vec3::Y };
        let src_i = hit.source.and_then(|s| self.pidx(s));
        let self_hit = hit.source == Some(hit.target);
        if self.players[ti].shielded() {
            self.events.push(GameEvent::Blocked { target: hit.target, source: hit.source, point: hit.point });
            return none;
        }
        if let Some(si) = src_i {
            if !self_hit && self.players[si].shielded() && hit.kind != HitKind::WallSplat {
                return none;
            }
        }
        // Momentum bonus (melee only): closing speed along attacker → victim.
        let mut bonus = 0.0;
        let mut v_close = 0.0;
        if let Some(v) = hit.closing_speed.filter(|v| v.is_finite()) {
            v_close = v.max(0.0);
            bonus = momentum_bonus(v_close);
        } else if hit.kind.momentum_bonus() {
            if let Some(si) = src_i {
                let (a, b) = (&self.players[si], &self.players[ti]);
                let to = (b.mv.pos - a.mv.pos).normalize_or_zero();
                v_close = (a.mv.vel - b.mv.vel).dot(to).max(0.0);
                bonus = momentum_bonus(v_close);
            }
        }
        let dealt_mult = match src_i {
            Some(si) if !self_hit => self.players[si].size.damage_dealt(hit.kind.momentum_bonus()),
            _ => 1.0,
        };
        // Head zone: top 0.35 m of the hull (scaled with body size) → ×1.5 and a crit.
        let head = {
            let p = &self.players[ti];
            let hull = p.hull(&self.cfg.move_params);
            hit.kind.can_headshot() && hit.point.is_finite() && hit.point.y >= hull.max.y - HEAD_ZONE * p.size.scale
        };
        let crit = hit.crit || head;
        let mut damage = base_damage * (1.0 + bonus) * dealt_mult * if head { HEAD_MULT } else { 1.0 };
        // Power suit absorbs first.
        if let Some(seat) = self.players[ti].seat.filter(|s| s.suit) {
            let mut shattered = false;
            if let Some(c) = self.constructs.get_mut(seat.construct) {
                let absorbed = damage.min(c.integrity.max(0.0));
                c.integrity -= absorbed;
                damage -= absorbed;
                self.events.push(GameEvent::ConstructHit { id: c.id, source: hit.source, damage: absorbed, point: hit.point });
                if c.integrity <= 0.0 && c.gone.is_none() {
                    c.integrity = 0.0;
                    c.gone = Some(crate::events::GoneReason::Spent);
                    c.shatter = Some(dir_in.normalize_or_zero() * base_kb);
                    shattered = true;
                }
            }
            if shattered {
                self.players[ti].seat = None;
                if let Some(c) = self.constructs.get_mut(seat.construct) {
                    c.pilot = None;
                }
            }
        }
        let cockpit = self.players[ti].seat.is_some_and(|s| !s.suit);
        let health = {
            let p = &mut self.players[ti];
            p.health = (p.health - damage).max(0.0);
            p.health
        };
        // Lobby knob `knockback_scale` (1.0 = shipped, bit-identical).
        let kb_mult = knock_mult(health) * element_knockback(hit.element) * self.players[ti].size.knockback_taken() * self.cfg.rules.knockback_scale;
        let kb = if cockpit { 0.0 } else { base_kb * (1.0 + bonus) * kb_mult };
        let dir = knockback_dir(dir_in, hit.element);
        let tumble = kb > TUMBLE_IMPULSE;
        if kb > 0.0 {
            let mv = &mut self.players[ti].mv;
            let vd = mv.vel.dot(dir);
            if vd < 0.0 {
                mv.vel -= dir * vd.max(-kb);
            }
            mv.apply_impulse(dir * kb, if tumble { TUMBLE_SECS } else { 0.0 });
        }
        // Credit.
        if let (Some(s), false) = (hit.source, self_hit) {
            self.players[ti].last_attacker = Some((s, self.tick));
            if let Some(si) = src_i {
                self.players[si].last_target = Some(hit.target);
            }
        }
        // Statuses from the element.
        for &(k, secs) in element_statuses(hit.element) {
            self.apply_status(ti, k, secs, hit.source);
        }
        // Oath interrupted by damage.
        if damage > 0.0 {
            self.interrupt_oath(ti);
        }
        let lethal = health <= 0.0;
        let weight = hit.impact * if hit.kind == HitKind::HeavyMelee { 1.3 } else { 1.0 };
        let impact = impact_fx(damage, kb, weight, dir, v_close);
        self.events.push(GameEvent::Hit {
            target: hit.target,
            source: hit.source,
            construct: hit.construct,
            kind: hit.kind,
            element: hit.element,
            damage,
            knockback: kb,
            point: hit.point,
            bonus,
            tumble,
            lethal,
            crit,
            impact,
        });
        // Mechanic side effects of the delivering construct (rays / chains).
        if let Some(cid) = hit.construct {
            let effects = self.constructs.get(cid).map(|c| (c.has(Mechanic::Shrink), c.has(Mechanic::Grow), c.has(Mechanic::Link), c.attached()));
            if let Some((shrink, grow, link, attached)) = effects {
                if shrink {
                    self.resize_player(hit.target, SHRINK_SCALE, RESIZE_SECS);
                }
                if grow {
                    self.resize_player(hit.target, GROW_SCALE, RESIZE_SECS);
                }
                if link && !lethal {
                    let other = if attached { self.ground_point_below(hit.point) } else { None };
                    let end_b = match other {
                        Some(p) => LinkEnd::Point(p),
                        None if !attached => LinkEnd::Construct(cid),
                        None => LinkEnd::Point(hit.point),
                    };
                    self.create_link(hit.source, LinkEnd::Player(hit.target), end_b, LINK_MAX_LEN, crate::space::LINK_LIFETIME);
                }
            }
        }
        // Electric chain: one extra target at 50 % (wet targets arc further — and back to a wet caster).
        if hit.element == Element::Electric && hit.kind != HitKind::Chain && !lethal {
            self.electric_chain(ti, &hit, damage);
        }
        if lethal {
            let killer = if self_hit { self.credit(ti) } else { hit.source.or_else(|| self.credit(ti)) };
            let cause = if hit.kind == HitKind::WallSplat { KoCause::WallSplat } else { KoCause::Damage };
            self.ko(ti, killer, cause);
        }
        none.applied = true;
        none.damage = damage;
        none.knockback = kb;
        none.lethal = lethal;
        none
    }

    fn electric_chain(&mut self, ti: usize, hit: &HitInfo, damage: f32) {
        let base = self.cfg.move_params.clone();
        let from = self.players[ti].hull(&base).center();
        let target_wet = self.players[ti].status.active(StatusKind::Wet);
        let mut best: Option<(usize, f32)> = None;
        for (k, p) in self.players.iter().enumerate() {
            if k == ti || !p.alive || p.shielded() {
                continue;
            }
            let wet = p.status.active(StatusKind::Wet);
            // The attacker is only a valid arc target when soaked (water + electric backfires).
            if Some(p.id) == hit.source && !wet {
                continue;
            }
            let reach = if wet || target_wet { CHAIN_RADIUS * 2.0 } else { CHAIN_RADIUS };
            let d = (p.hull(&base).center() - from).length();
            if d <= reach && best.is_none_or(|b| d < b.1) {
                best = Some((k, d));
            }
        }
        if let Some((k, _)) = best {
            let to = self.players[k].hull(&base).center();
            let chain = HitInfo {
                target: self.players[k].id,
                source: hit.source,
                construct: hit.construct,
                damage: damage * 0.5,
                impulse_dir: to - from,
                knockback: hit.knockback * 0.5,
                element: Element::Electric,
                impact: 0.6,
                kind: HitKind::Chain,
                point: to,
                closing_speed: None,
                crit: false,
            };
            self.apply_hit(chain);
        }
    }

    pub(crate) fn apply_status(&mut self, i: usize, k: StatusKind, secs: f32, source: Option<PlayerId>) {
        let p = &mut self.players[i];
        if !p.alive || !secs.is_finite() || secs <= 0.0 {
            return;
        }
        let secs = secs.min(10.0);
        if p.status.apply(k, secs, source) {
            let id = p.id;
            self.events.push(GameEvent::StatusApplied { player: id, status: k, secs, source });
        }
    }

    fn interrupt_oath(&mut self, i: usize) {
        let pool = self.cfg.rules.willpower_max();
        let p = &mut self.players[i];
        if let OathState::Reciting { started, len, progress, accuracy } = p.oath {
            let elapsed = self.tick.saturating_sub(started) as f32 * DT;
            let est = (elapsed * crate::willpower::OATH_TARGET_CPS).min(len as f32) as u16;
            let prog = if progress > 0 { progress } else { est };
            let refill = oath_partial(prog, accuracy, len, pool);
            p.willpower = (p.willpower + refill).min(pool);
            p.oath = OathState::Idle;
            let id = p.id;
            self.events.push(GameEvent::OathInterrupted { player: id, refill });
        }
    }

    /// Burn / poison ticks: damage without knockback, credited to whoever applied the status.
    pub(crate) fn damage_over_time(&mut self, i: usize, dmg: f32, src: Option<PlayerId>) {
        if self.match_state.phase == crate::rules::MatchPhase::Ended || self.players[i].shielded() {
            return;
        }
        let p = &mut self.players[i];
        p.health = (p.health - dmg).max(0.0);
        if let Some(s) = src.filter(|s| *s != p.id) {
            p.last_attacker = Some((s, self.tick));
        }
        if p.health <= 0.0 {
            let killer = src.filter(|s| *s != self.players[i].id).or_else(|| self.credit(i));
            self.ko(i, killer, KoCause::Status);
        } else {
            self.interrupt_oath(i);
        }
    }

    /// Tumbling impacts recorded by movement: bonus damage, a crater, a bounce.
    pub(crate) fn resolve_wall_splats(&mut self) {
        let base = self.cfg.move_params.clone();
        for i in 0..self.players.len() {
            let Some(w) = self.players[i].pending_splat.take() else { continue };
            if !self.players[i].alive {
                continue;
            }
            let dmg = splat_damage(w.speed);
            let hull = self.players[i].hull(&base);
            let point = hull.center() - w.normal * (hull.half().x + 0.25);
            // Bounce off at 30 % of the impact speed.
            self.players[i].mv.vel += w.normal * (w.speed * SPLAT_BOUNCE);
            let id = self.players[i].id;
            self.events.push(GameEvent::WallSplat { player: id, point, normal: w.normal, speed: w.speed, damage: dmg });
            self.request_carve(CarveRequest { center: point, radius: SPLAT_CRATER_RADIUS, power: 0.8 });
            let source = self.credit(i);
            let mut hit = HitInfo::new(id, source, HitKind::WallSplat, dmg, 0.0, w.normal, point);
            hit.impact = 1.2;
            self.apply_hit(hit);
        }
    }

    /// Integrity damage to a construct.
    pub fn apply_construct_hit(&mut self, hit: ConstructHit) -> bool {
        if !hit.damage.is_finite() {
            return false;
        }
        self.constructs.damage(&hit, &mut self.events)
    }

    /// Radial blast: players (falloff to 30 % at the edge), constructs, portals, links, crater.
    pub fn explode(&mut self, e: Explosion) {
        if !e.pos.is_finite() || !e.radius.is_finite() {
            return;
        }
        let r = e.radius.clamp(0.1, 40.0);
        self.events.push(GameEvent::Explosion { pos: e.pos, radius: r, element: e.element, source: e.source });
        let base = self.cfg.move_params.clone();
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive {
                continue;
            }
            let hull = p.hull(&base);
            let d = geom::dist_sq_point_aabb(&hull, e.pos).sqrt();
            if d > r {
                continue;
            }
            let fall = 1.0 - 0.7 * (d / r);
            let dir = hull.center() - e.pos + Vec3::Y * 0.5;
            if Some(p.id) == e.source && !e.self_damage {
                // Rocket-jump: your own blast pushes you (no damage, no tumble).
                let d = dir.normalize_or_zero();
                if !p.shielded() && p.seat.is_none_or(|s| s.suit) {
                    self.players[i].mv.apply_impulse(d * (e.knockback * fall), 0.0);
                }
                continue;
            }
            let mut hit = HitInfo::new(p.id, e.source, HitKind::Explosion, e.damage * fall, e.knockback * fall, dir, geom::closest_point(&hull, e.pos))
                .with_element(e.element)
                .with_impact(1.4);
            hit.construct = e.construct;
            self.apply_hit(hit);
        }
        let mut targets: Vec<(bd_core::ConstructId, f32, Vec3)> = Vec::new();
        for c in self.constructs.iter() {
            if !c.active() || Some(c.owner) == e.source || Some(c.id) == e.construct {
                continue;
            }
            let d = geom::dist_sq_point_aabb(&c.bounds(), e.pos).sqrt();
            if d <= r {
                targets.push((c.id, 1.0 - 0.7 * (d / r), (c.center() - e.pos).normalize_or_zero()));
            }
        }
        for (id, fall, dir) in targets {
            self.apply_construct_hit(ConstructHit { target: id, source: e.source, damage: e.damage * fall, impulse: dir * e.knockback * fall, blade: false, point: e.pos });
        }
        self.damage_space_in_radius(e.pos, r, e.damage);
        if e.carve_radius > 0.0 {
            let k = if e.element == Element::Earth { 1.26 } else { 1.0 };
            self.request_carve(CarveRequest { center: e.pos, radius: e.carve_radius * k, power: 1.0 });
        }
    }

    /// Apply everything constructs asked for this tick.
    pub(crate) fn apply_construct_out(&mut self) {
        // Take buffers out so we can call &mut self methods; put them back (capacity reused).
        let mut out = std::mem::take(&mut self.cout);
        for e in out.events.drain(..) {
            self.events.push(e);
        }
        for h in out.player_hits.drain(..) {
            self.apply_hit(h);
        }
        for h in out.construct_hits.drain(..) {
            self.apply_construct_hit(h);
        }
        for e in out.explosions.drain(..) {
            self.explode(e);
        }
        for c in out.carves.drain(..) {
            self.request_carve(c);
        }
        for s in out.projectiles.drain(..) {
            let view = self.player(s.owner).and_then(|p| p.view_tick);
            // Shots from a held gun inherit 100 % of the shooter's velocity (COMBAT.md §4b).
            let held = s.construct.and_then(|c| self.constructs.get(c)).is_some_and(|c| c.attached());
            self.fire_projectile(s, view, held.then_some(s.owner));
        }
        for (pid, aim, strength) in out.recoils.drain(..) {
            self.recoil(pid, aim, strength);
        }
        for h in out.hitscans.drain(..) {
            self.hitscan(h);
        }
        for (pid, dv, tumble) in out.impulses.drain(..) {
            if let Some(i) = self.pidx(pid) {
                if self.players[i].alive && dv.is_finite() {
                    self.players[i].mv.apply_impulse(dv, tumble.clamp(0.0, TUMBLE_SECS));
                }
            }
        }
        for (pid, k, secs, src) in out.statuses.drain(..) {
            if let Some(i) = self.pidx(pid) {
                self.apply_status(i, k, secs, src);
            }
        }
        for (pid, amount) in out.drain.drain(..) {
            if let Some(p) = self.player_mut(pid) {
                if amount.is_finite() {
                    p.willpower = (p.willpower - amount.max(0.0)).max(0.0);
                }
            }
        }
        for (pid, cid, vel) in out.ejects.drain(..) {
            self.eject_with(pid, cid, vel, false);
        }
        self.cout = out;
    }

    // ------------------------------------------------------------------------------ carves

    /// Queue terrain destruction. Big requests become progressive: sub-carves spread over ~0.5 s
    /// (inner first), each replicated as its own `CarveOp`.
    pub fn request_carve(&mut self, req: CarveRequest) {
        if !req.center.is_finite() || !req.radius.is_finite() || !req.power.is_finite() {
            return;
        }
        let radius = req.radius.clamp(0.25, MAX_CARVE_RADIUS);
        let power = req.power.clamp(0.0, 4.0);
        if radius <= PROGRESSIVE_RADIUS {
            let op = self.make_carve(req.center, radius, power);
            self.enqueue_carve(self.tick, op);
            return;
        }
        // Small sub-carves (≤ ~500 voxels each) so the per-tick voxel budget can pace them.
        let sub = (radius / 4.0).clamp(1.5, 2.5);
        let spacing = sub * 1.2;
        let n = (radius / spacing).ceil() as i32;
        let mut pts: Vec<(f32, Vec3)> = Vec::new();
        for y in -n..=n {
            for z in -n..=n {
                for x in -n..=n {
                    let o = Vec3::new(x as f32, y as f32, z as f32) * spacing;
                    let d = o.length();
                    if d + sub * 0.5 <= radius + sub * 0.5 && d <= radius {
                        pts.push((d, o));
                    }
                }
            }
        }
        // Stable sort: inner first, ties keep the fixed loop order.
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let count = pts.len().max(1) as u64;
        for (k, (_, o)) in pts.into_iter().enumerate() {
            let due = self.tick + (k as u64 * PROGRESSIVE_TICKS) / count;
            let op = self.make_carve(req.center + o, sub, power);
            self.enqueue_carve(due, op);
        }
    }

    fn make_carve(&mut self, center: Vec3, radius: f32, power: f32) -> CarveOp {
        let seed = self.carve_seed;
        self.carve_seed = self.carve_seed.wrapping_add(1).max(1);
        CarveOp { shape: CarveShape::Sphere { center, radius }, power, seed }
    }

    fn enqueue_carve(&mut self, due: Tick, op: CarveOp) {
        // Hard cap on the backlog: drop the newest beyond it (never stall).
        if self.carve_queue.len() >= 4096 {
            return;
        }
        let at = self.carve_queue.partition_point(|(d, _)| *d <= due);
        self.carve_queue.insert(at, (due, op));
    }

    /// Apply due carves within the per-tick budgets (at least one per tick when any is due).
    pub(crate) fn apply_carves(&mut self) {
        let mut ops = 0usize;
        let mut budget = self.cfg.carve_voxel_budget;
        while let Some(&(due, op)) = self.carve_queue.front() {
            if due > self.tick {
                break;
            }
            let est = match op.shape {
                CarveShape::Sphere { radius, .. } => {
                    let rv = radius / VOXEL_SIZE;
                    4.18879 * rv * rv * rv
                }
                _ => 1000.0,
            };
            if ops > 0 && (ops >= self.cfg.carves_per_tick || est > budget) {
                break;
            }
            self.carve_queue.pop_front();
            ops += 1;
            budget -= est;
            let outcome = apply_carve_op(&mut self.voxels, &op);
            self.powers_record_carve(&outcome); // POWERS HOOK: Crazy Diamond's restore log
            self.events.push(GameEvent::Carve { op });
        }
    }

    /// Pending (not yet applied) carves.
    pub fn pending_carves(&self) -> impl Iterator<Item = &CarveOp> {
        self.carve_queue.iter().map(|(_, op)| op)
    }

    /// Ground point under `p` within 30 m (for "link to the ground").
    pub(crate) fn ground_point_below(&self, p: Vec3) -> Option<Vec3> {
        bd_core::voxel::raycast(&self.voxels, p, Vec3::NEG_Y, 30.0).map(|h| h.point)
    }
}
