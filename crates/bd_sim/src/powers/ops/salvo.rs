//! `Salvo` — telegraphed strikes placed in space (docs/toolbox/OPS.md). Each strike is MARKED where
//! it will land (everyone sees the mark for `delay` s: fair, readable), then lands as a blast. The
//! marks go at the crosshair (`at: Aim`: the enemy under it, else the surface), around you (`at: Me`) or in front (`at: Front`), scattered
//! by `spread`; `seek` snaps each onto the nearest enemies inside `spread` of the centre; `track`
//! places each mark at the LIVE crosshair when its turn comes (you paint the strikes).
//!
//! Judgment Cut = 3 quick marks at the crosshair; Judgment Cut End = a dozen marks all around you
//! after a long draw; Urbosa's Fury = seeking bolts on everyone near; Knights of the Round = 13
//! tracked strikes; Megaflare = one huge mark with a long telegraph.
//!
//! Timeline of strike `k`: untracked marks all appear at once and land at `delay + k·interval`;
//! tracked / seeking marks appear at `k·interval` and land `delay` later. Deterministic scatter
//! (golden-angle spiral + a seeded jitter); fixed pool, no allocation.

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, PlayerId, Vec3};

use super::OpEvent;
use crate::combat::Explosion;
use crate::powers::{At, PowerError};
use crate::world::World;

/// Pending strikes in the whole world (a cast that doesn't fit keeps what fits).
pub const MAX_STRIKES: usize = 32;
/// Most strikes one cast may ask for.
pub const MAX_COUNT: u8 = 16;
pub const MAX_DELAY: f32 = 3.0;
pub const MAX_INTERVAL: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SalvoSpec {
    pub count: u8,
    pub delay: f32,
    pub interval: f32,
    pub radius: f32,
    pub damage: f32,
    pub knockback: f32,
    pub element: Element,
    pub carve: f32,
    pub up: f32,
    pub at: At,
    pub dist: f32,
    pub spread: f32,
    pub seek: bool,
    pub track: bool,
    pub cut: bool,
}

/// One pending strike (`OpState::strikes`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Strike {
    pub owner: PlayerId,
    /// Index of the strike within its salvo (scatter / seek order).
    pub k: u8,
    /// Seconds until the mark is placed (0 = placed).
    pub mark_in: f32,
    /// Seconds from the mark until it lands.
    pub land_in: f32,
    pub placed: bool,
    pub centre: Vec3,
    pub pos: Vec3,
    pub spec: SalvoSpec,
}

/// How long the salvo's active phase lasts (the last strike lands).
pub fn active_secs(count: u8, delay: f32, interval: f32) -> f32 {
    let n = count.clamp(1, MAX_COUNT) as f32;
    delay.clamp(0.0, MAX_DELAY) + (n - 1.0) * interval.clamp(0.0, MAX_INTERVAL)
}

fn fin(v: f32, lo: f32, hi: f32, d: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl World {
    pub(crate) fn op_salvo(&mut self, i: usize, s: SalvoSpec) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let s = SalvoSpec {
            count: s.count.clamp(1, MAX_COUNT),
            delay: fin(s.delay, 0.05, MAX_DELAY, 0.4),
            interval: fin(s.interval, 0.0, MAX_INTERVAL, 0.1),
            radius: fin(s.radius, 0.5, 12.0, 2.0),
            damage: fin(s.damage, 0.0, 60.0, 0.0),
            knockback: fin(s.knockback, 0.0, 60.0, 0.0),
            carve: fin(s.carve, 0.0, 4.0, 0.0),
            up: fin(s.up, 0.0, 30.0, 0.0),
            dist: fin(s.dist, 0.0, 30.0, 0.0),
            spread: fin(s.spread, 0.0, 30.0, 0.0),
            ..s
        };
        let centre = self.salvo_centre(i, s.at, s.dist);
        let free = self.powers.ops.strikes.iter().filter(|x| x.is_none()).count();
        if free == 0 {
            return Err(PowerError::Busy);
        }
        let staggered = s.track || s.seek;
        let mut k = 0u8;
        for slot in self.powers.ops.strikes.iter_mut() {
            if k >= s.count {
                break;
            }
            if slot.is_some() {
                continue;
            }
            let (mark_in, land_in) = if staggered { (k as f32 * s.interval, s.delay) } else { (0.0, s.delay + k as f32 * s.interval) };
            *slot = Some(Strike { owner: id, k, mark_in, land_in, placed: false, centre, pos: centre, spec: s });
            k += 1;
        }
        // Marks due now appear this tick (the step places them, so a cast and a tick agree).
        self.step_strikes(0.0);
        Ok(())
    }

    /// The salvo's centre: `At::Aim` is the first thing under the crosshair — an enemy on the ray
    /// (their centre) before the terrain behind them.
    fn salvo_centre(&self, i: usize, at: At, dist: f32) -> Vec3 {
        if at == At::Aim
            && let Some(k) = self.op_aimed_player(i, 60.0, 0.0)
        {
            return self.players[k].hull(&self.cfg.move_params).center();
        }
        self.op_at_point(i, at, dist)
    }

    /// Where strike `k` goes (called when its mark is placed).
    fn strike_pos(&self, st: &Strike) -> Vec3 {
        let s = &st.spec;
        let base = &self.cfg.move_params;
        let owner = self.pidx(st.owner);
        // Tracked: the owner's live crosshair (their look — no move aim outside the move).
        let centre = match owner {
            Some(i) if s.track && self.players[i].alive => self.salvo_centre(i, s.at, s.dist),
            _ => st.centre,
        };
        if s.seek {
            // The (k mod n)-th nearest enemy to the centre within the spread (+ the radius).
            let reach = s.spread + s.radius;
            let mut near: [(f32, usize); MAX_PLAYERS] = [(f32::INFINITY, usize::MAX); MAX_PLAYERS];
            let mut n = 0;
            for (q, p) in self.players.iter().enumerate() {
                if Some(q) == owner || !p.alive || n >= near.len() {
                    continue;
                }
                let c = p.hull(base).center();
                let d = c.distance(centre);
                if d <= reach {
                    near[n] = (d, q);
                    n += 1;
                }
            }
            if n > 0 {
                near[..n].sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let q = near[st.k as usize % n].1;
                return self.players[q].hull(base).center();
            }
        }
        if s.spread <= 0.0 || (st.k == 0 && !s.seek) {
            return centre;
        }
        // Golden-angle spiral with a seeded jitter, then settled onto the ground below (≤ 4 m).
        let seed = crate::powers::effects::roll(
            (u64::from(st.owner.0) << 40) ^ (u64::from(st.k) << 32) ^ (self.tick as u64).wrapping_mul(0x9E37_79B9),
        );
        let n = s.count.max(2) as f32;
        let a = st.k as f32 * 2.399_963 + seed * 0.9;
        let r = s.spread * ((st.k as f32 + 0.5) / n).sqrt();
        let p = centre + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        if s.at == At::Front {
            return p;
        }
        match bd_core::voxel::raycast(&self.voxels, p + Vec3::Y * 1.5, Vec3::NEG_Y, 5.5) {
            Some(h) => h.point + Vec3::Y * 0.3,
            None => p,
        }
    }

    /// A clean strike: every enemy inside the radius is hit (outward + `up`), terrain carved if
    /// asked; nothing else is pushed.
    fn strike_cut(&mut self, st: &Strike) {
        let s = st.spec;
        let base = self.cfg.move_params.clone();
        let source = self.pidx(st.owner).map(|_| st.owner);
        let mut hits: [Option<(PlayerId, Vec3)>; MAX_PLAYERS] = [None; MAX_PLAYERS];
        for (q, p) in self.players.iter().enumerate().take(MAX_PLAYERS) {
            if !p.alive || Some(p.id) == source {
                continue;
            }
            let c = p.hull(&base).center();
            if crate::geom::dist_sq_point_aabb(&p.hull(&base), st.pos) <= s.radius * s.radius {
                hits[q] = Some((p.id, c));
            }
        }
        for (tid, c) in hits.iter().flatten().copied() {
            let dir = Vec3::new(c.x - st.pos.x, 0.0, c.z - st.pos.z).normalize_or(Vec3::Y) + Vec3::Y * (0.4 + s.up / 20.0);
            let h = crate::combat::HitInfo::new(tid, source, crate::combat::HitKind::Melee, s.damage, s.knockback, dir, c).with_element(s.element).with_impact(1.3);
            self.apply_hit(h);
        }
        if s.carve > 0.0 {
            self.request_carve(crate::combat::CarveRequest { center: st.pos, radius: s.carve, power: 1.5 });
        }
    }

    pub(crate) fn step_strikes(&mut self, dt: f32) {
        for k in 0..MAX_STRIKES {
            let Some(mut st) = self.powers.ops.strikes[k] else { continue };
            if !st.placed {
                st.mark_in -= dt;
                if st.mark_in > 0.0 {
                    self.powers.ops.strikes[k] = Some(st);
                    continue;
                }
                st.placed = true;
                st.pos = self.strike_pos(&st);
                let s = st.spec;
                self.op_event(OpEvent::Telegraph { player: st.owner, pos: st.pos, radius: s.radius, secs: st.land_in, element: s.element });
                // A tick-0 placement doesn't also count this tick's time against the landing.
                self.powers.ops.strikes[k] = Some(st);
                continue;
            }
            st.land_in -= dt;
            if st.land_in > 0.0 {
                self.powers.ops.strikes[k] = Some(st);
                continue;
            }
            self.powers.ops.strikes[k] = None;
            let s = st.spec;
            self.op_event(OpEvent::Strike { player: st.owner, pos: st.pos, radius: s.radius, element: s.element, cut: s.cut });
            if s.cut {
                self.strike_cut(&st);
                continue;
            }
            let source = self.pidx(st.owner).map(|_| st.owner);
            // Your own strikes never rocket-jump you (the ordinary own-blast push is undone).
            let keep = self.pidx(st.owner).map(|q| (q, self.players[q].mv.vel, self.players[q].mv.grounded));
            self.explode(Explosion {
                pos: st.pos,
                radius: s.radius,
                damage: s.damage,
                knockback: s.knockback,
                source,
                construct: None,
                element: s.element,
                carve_radius: s.carve,
                self_damage: false,
            });
            if let Some((q, vel, grounded)) = keep {
                let mv = &mut self.players[q].mv;
                mv.vel = vel;
                mv.grounded = grounded;
            }
            if s.up > 0.0 {
                let base = self.cfg.move_params.clone();
                for p in self.players.iter_mut() {
                    if !p.alive || Some(p.id) == source || p.hull(&base).center().distance(st.pos) > s.radius + 0.4 {
                        continue;
                    }
                    p.mv.vel.y = p.mv.vel.y.max(s.up);
                    p.mv.grounded = false;
                }
            }
        }
    }
}
