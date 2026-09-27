//! Generic power effects (COMBAT.md §5h) — small pieces ANY power, tier or transform may list in
//! `assets/powers.ron` (`effects: [...]`). They stack across every power a player holds.
//!
//! * [`PowerEffect::AutoDodge`] — an incoming dodgeable hit may make the body move on its own: the
//!   ordinary dodge (its i-frames + burst, `movement/dodge.rs`) sideways off the hit's line, then a
//!   cooldown. The roll is a hash of (tick, attacker, target): deterministic, no RNG stream touched.
//! * [`PowerEffect::ProjectileSlowField`] — enemy projectiles (world/projectiles.rs POWERS HOOK) and
//!   thrown / launched objects inside the radius lose speed every tick down to a crawl
//!   ([`SLOW_FIELD_CRAWL`]); hitscan and melee are untouched, and a slowed round still arrives.
//! * [`PowerEffect::SelfDrain`] + [`PowerEffect::PowerMult`] — the Kaio-ken trade: your own % rises
//!   and / or willpower drains while you deal more damage / knockback, run faster and cool down
//!   power moves faster. `speed` is read by [`super::PlayerPower::modify_movement`] (client
//!   prediction replays it identically).
//! * [`PowerEffect::CritChance`] — melee connects may crit (Black Flash): × damage and knockback,
//!   `HitInfo::crit`, a [`PowerEvent::Crit`] for the black-red spark.
//! * [`PowerEffect::ReadSlow`] — presentation only (bd_game reads it from the table).
//! * [`PowerEffect::AirJumps`] — more air jumps per airtime (`MoveModifiers::extra_air_jumps`, read
//!   by [`super::PlayerPower::modify_movement`]: host and prediction alike). Stacks, ≤ [`MAX_EXTRA_AIR_JUMPS`].
//! * Gravity cores ([`Core`], `MoveOp::Singularity`): Planetary Devastation. One construct (the
//!   core) + a per-tick pull on enemies, loose constructs and a capped number of carved terrain
//!   chunks; captured enemies are held in the mass until it breaks or they dodge / tether out.

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, ModelSource, Recipe};
use bd_core::{ConstructId, DT, MAX_PLAYERS, PlayerId, Quat, Vec3};

use super::*;
use crate::combat::{CarveRequest, Explosion, HitInfo, HitKind};
use crate::constructs::{AnimState, Loco, SpawnParams};
use crate::events::{GameEvent, GoneReason};
use crate::willpower::validate_recipe;
use crate::world::World;

/// Slowed projectiles / thrown things never drop under this speed (m/s): they crawl in.
pub const SLOW_FIELD_CRAWL: f32 = 1.6;
/// `ProjectileSlowField::factor` is the speed kept per this many seconds.
pub const SLOW_FIELD_STEP: f32 = 0.1;
/// Most gravity cores alive at once (the oldest collapses early beyond it).
pub const MAX_CORES: usize = 2;
/// Most constructs one core pulls per tick.
const CORE_MAX_CONSTRUCTS: usize = 24;
/// Seconds a player who broke out of a core can't be recaptured.
const CORE_ESCAPE_SECS: f32 = 1.4;
/// Seconds a core takes to rise.
const CORE_RISE_SECS: f32 = 0.8;
/// Most extra air jumps all held powers give together.
pub const MAX_EXTRA_AIR_JUMPS: u8 = 4;

/// Clamp authored numbers (hostile / typo'd data never explodes the sim).
pub(crate) fn sanitize_effects(v: &mut Vec<PowerEffect>) {
    v.truncate(8);
    let f = |x: &mut f32, lo: f32, hi: f32, d: f32| *x = if x.is_finite() { x.clamp(lo, hi) } else { d };
    for e in v.iter_mut() {
        match e {
            PowerEffect::AutoDodge { chance, cooldown } => {
                f(chance, 0.0, 1.0, 0.0);
                f(cooldown, 0.1, 30.0, 2.0);
            }
            PowerEffect::ProjectileSlowField { radius, factor } => {
                f(radius, 0.5, 12.0, 3.0);
                f(factor, 0.01, 1.0, 1.0);
            }
            PowerEffect::SelfDrain { damage_pct_per_s, willpower_per_s } => {
                f(damage_pct_per_s, 0.0, 20.0, 0.0);
                f(willpower_per_s, 0.0, 40.0, 0.0);
            }
            PowerEffect::PowerMult { damage, speed, knockback, cooldown } => {
                f(damage, 0.25, 4.0, 1.0);
                f(speed, 0.5, 2.0, 1.0);
                f(knockback, 0.25, 4.0, 1.0);
                f(cooldown, 0.2, 2.0, 1.0);
            }
            PowerEffect::CritChance { chance, mult, .. } => {
                f(chance, 0.0, 1.0, 0.0);
                f(mult, 1.0, 5.0, 1.0);
            }
            PowerEffect::ReadSlow { radius } => f(radius, 0.5, 30.0, 8.0),
            PowerEffect::AirJumps { extra } => *extra = (*extra).min(MAX_EXTRA_AIR_JUMPS),
        }
    }
}

/// Deterministic 0..1 roll from a seed (splitmix64 finaliser) — no RNG stream is consumed, so an
/// effect never shifts anything else the world rolls.
pub fn roll(seed: u64) -> f32 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

fn seed(tick: u64, a: PlayerId, b: PlayerId, salt: u64) -> u64 {
    tick.wrapping_mul(0x100_0000_01B3) ^ ((a.0 as u64) << 40) ^ ((b.0 as u64) << 48) ^ salt
}

/// Product of every held `PowerMult::speed`.
pub fn speed_mult(p: &PlayerPower) -> f32 {
    p.effects().fold(1.0, |k, e| if let PowerEffect::PowerMult { speed, .. } = e { k * speed } else { k }).clamp(0.5, 2.5)
}

/// Extra air jumps of every held `AirJumps` (none while drained).
pub fn extra_air_jumps(p: &PlayerPower) -> u8 {
    p.effects().fold(0u8, |n, e| if let PowerEffect::AirJumps { extra } = e { n.saturating_add(*extra) } else { n }).min(MAX_EXTRA_AIR_JUMPS)
}

/// [`extra_air_jumps`] from a replicated [`PowerSnap`] (presentation: the springy boing of a
/// remote player's air jump).
pub fn extra_air_jumps_snap(s: &PowerSnap) -> u8 {
    if s.has(power_flags::DRAINED) || s.has(power_flags::SEASTONE) {
        return 0;
    }
    let mut p = PlayerPower::default();
    p.apply_snap(*s);
    extra_air_jumps(&p)
}

/// (damage, knockback) multipliers of every held `PowerMult`.
pub fn hit_mults(p: &PlayerPower) -> (f32, f32) {
    let (d, k) = p.effects().fold((1.0f32, 1.0f32), |(d, k), e| if let PowerEffect::PowerMult { damage, knockback, .. } = e { (d * damage, k * knockback) } else { (d, k) });
    (d.clamp(0.25, 5.0), k.clamp(0.25, 5.0))
}

/// Power-move cooldown multiplier of every held `PowerMult` (< 1 = faster).
pub fn cooldown_mult(p: &PlayerPower) -> f32 {
    p.effects().fold(1.0, |k, e| if let PowerEffect::PowerMult { cooldown, .. } = e { k * cooldown } else { k }).clamp(0.2, 2.0)
}

/// The strongest slow field a player holds: (radius, factor).
pub fn slow_field(p: &PlayerPower) -> Option<(f32, f32)> {
    p.effects()
        .filter_map(|e| if let PowerEffect::ProjectileSlowField { radius, factor } = e { Some((*radius, *factor)) } else { None })
        .fold(None, |best: Option<(f32, f32)>, (r, f)| Some(best.map_or((r, f), |(br, bf)| (br.max(r), bf.min(f)))))
}

/// Largest ReadSlow radius a player holds (bd_game: telegraph glow on incoming attacks).
pub fn read_radius(p: &PlayerPower) -> Option<f32> {
    p.effects().filter_map(|e| if let PowerEffect::ReadSlow { radius } = e { Some(*radius) } else { None }).reduce(f32::max)
}

/// Speed of a thing moving at `speed` after one tick in a slow field keeping `factor` per
/// [`SLOW_FIELD_STEP`] (never under the crawl; already-slow things are untouched).
pub fn slowed_speed(speed: f32, factor: f32, dt: f32) -> f32 {
    if !(speed > SLOW_FIELD_CRAWL) {
        return speed;
    }
    let k = factor.clamp(0.01, 1.0).powf(dt / SLOW_FIELD_STEP);
    (speed * k).max(SLOW_FIELD_CRAWL)
}

/// A live gravity core (`MoveOp::Singularity`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Core {
    pub owner: PlayerId,
    pub construct: ConstructId,
    pub t: f32,
    pub secs: f32,
    pub from: Vec3,
    pub to: Vec3,
    pub radius: f32,
    pub strength: f32,
    pub capture: f32,
    pub chunks_left: u8,
    pub chunk_every: f32,
    pub chunk_cd: f32,
    pub damage: f32,
    pub knockback: f32,
    /// Players who broke out: (player, seconds of immunity left).
    pub escaped: Vec<(PlayerId, f32)>,
}

impl World {
    /// POWERS HOOK (`hooks::powers_on_hit`, attacker side): the source's `PowerMult` damage /
    /// knockback, and a `CritChance` roll on melee connects.
    pub(crate) fn powers_attacker_mods(&mut self, mut hit: HitInfo) -> HitInfo {
        let Some(si) = hit.source.and_then(|s| self.pidx(s)) else { return hit };
        if hit.source == Some(hit.target) || hit.kind == HitKind::WallSplat {
            return hit;
        }
        let pw = &self.players[si].power;
        if pw.id.is_none() || !pw.active() || !self.players[si].power.def().is_some_and(|d| self.power_allowed(d)) {
            return hit;
        }
        let (dm, km) = hit_mults(pw);
        hit.damage *= dm;
        hit.knockback *= km;
        let melee = matches!(hit.kind, HitKind::Melee | HitKind::HeavyMelee | HitKind::RingShove);
        if melee && hit.damage > 0.0 {
            let crit = pw.effects().find_map(|e| if let PowerEffect::CritChance { chance, mult, fx } = e { Some((*chance, *mult, *fx)) } else { None });
            if let Some((chance, mult, fx)) = crit {
                let attacker = self.players[si].id;
                if roll(seed(self.tick, attacker, hit.target, 0xB1AC_F1A5)) < chance {
                    hit.damage *= mult;
                    hit.knockback *= 1.0 + (mult - 1.0) * 0.5;
                    hit.impact *= 1.0 + mult * 0.5;
                    hit.crit = true;
                    let (target, point) = (hit.target, hit.point);
                    self.inject_event(GameEvent::Power(PowerEvent::Crit { attacker, target, point, fx, mult }));
                }
            }
        }
        hit
    }

    /// POWERS HOOK (target side): Ultra Instinct. `true` = the body dodged on its own (the hit is
    /// cancelled and the ordinary dodge — i-frames + burst — starts sideways off the hit's line).
    pub(crate) fn powers_auto_dodge(&mut self, ti: usize, hit: &HitInfo) -> bool {
        let p = &self.players[ti];
        if p.power.auto_dodge_cd > 0.0 || !crate::dodge::ignores(hit.kind) || hit.source == Some(hit.target) || (hit.damage <= 0.0 && hit.knockback <= 0.0) {
            return false;
        }
        let Some((chance, cooldown)) = p.power.effects().find_map(|e| if let PowerEffect::AutoDodge { chance, cooldown } = e { Some((*chance, *cooldown)) } else { None }) else {
            return false;
        };
        let attacker = hit.source.unwrap_or(PlayerId(255));
        let r = roll(seed(self.tick, attacker, hit.target, 0x0171_D0D6));
        if r >= chance || self.cfg.move_params.dodge_time.is_nan() || self.cfg.move_params.dodge_time <= 0.0 {
            return false;
        }
        let incoming = Vec3::new(hit.impulse_dir.x, 0.0, hit.impulse_dir.z).normalize_or(-p.look());
        let side = Vec3::Y.cross(incoming).normalize_or(Vec3::X) * if roll(seed(self.tick, attacker, hit.target, 0x51DE)) < 0.5 { 1.0 } else { -1.0 };
        // A little back from the attack too: out of the line, not into it.
        let dir = (side * 0.85 + incoming * 0.35).normalize_or(side);
        let from = p.mv.pos;
        let dodge_time = self.cfg.move_params.dodge_time;
        let p = &mut self.players[ti];
        let x = &mut p.mv.extra;
        x.dodge_dir = dir;
        x.dodge_base = Vec3::new(p.mv.vel.x, 0.0, p.mv.vel.z);
        x.dodge_left = dodge_time;
        x.since_dodge = 0.0;
        p.power.auto_dodge_cd = cooldown;
        let id = p.id;
        self.inject_event(GameEvent::Power(PowerEvent::AutoDodged { player: id, from, dir }));
        true
    }

    /// POWERS HOOK (world/projectiles.rs, one line): an enemy's slow field bends this round's
    /// speed down. Returns true if it changed (the segment is re-published like a bent shot).
    pub(crate) fn powers_slow_projectile(&self, owner: PlayerId, pos: Vec3, vel: &mut Vec3) -> bool {
        if self.powers.slow_fields.is_empty() {
            return false;
        }
        let mut changed = false;
        for &(pid, center, radius, factor) in &self.powers.slow_fields {
            if pid == owner || (pos - center).length_squared() > radius * radius {
                continue;
            }
            let s = vel.length();
            let n = slowed_speed(s, factor, DT);
            if n < s {
                *vel *= n / s;
                changed = true;
            }
        }
        changed
    }

    /// Per-tick effects (called from `step_powers`): refresh the slow fields, slow thrown things,
    /// self-drains, the auto-dodge / power-up timers, gravity cores. Allocation-free after warm-up.
    pub(crate) fn step_power_effects(&mut self, dt: f32) {
        self.powers.slow_fields.clear();
        let infinite = self.cfg.rules.infinite_willpower();
        let base = self.cfg.move_params.clone();
        for i in 0..self.players.len() {
            {
                let pw = &mut self.players[i].power;
                pw.auto_dodge_cd = (pw.auto_dodge_cd - dt).max(0.0);
                pw.ascend_cd = (pw.ascend_cd - dt).max(0.0);
            }
            let p = &self.players[i];
            if !p.alive || p.power.id.is_none() || !p.power.active() || !p.power.def().is_some_and(|d| self.power_allowed(d)) {
                continue;
            }
            if let Some((r, f)) = slow_field(&p.power) {
                if f < 1.0 && self.powers.slow_fields.len() < MAX_PLAYERS {
                    self.powers.slow_fields.push((p.id, p.hull(&base).center(), r, f));
                }
            }
            let (mut pct, mut wp) = (0.0f32, 0.0f32);
            for e in p.power.effects() {
                if let PowerEffect::SelfDrain { damage_pct_per_s, willpower_per_s } = e {
                    pct += damage_pct_per_s;
                    wp += willpower_per_s;
                }
            }
            if pct > 0.0 || wp > 0.0 {
                let p = &mut self.players[i];
                p.damage_pct = (p.damage_pct + pct * dt).min(999.0);
                if !infinite {
                    p.willpower = (p.willpower - wp * dt).max(0.0);
                }
            }
        }
        // Thrown / launched enemy objects inside a slow field crawl too.
        if !self.powers.slow_fields.is_empty() {
            let fields = std::mem::take(&mut self.powers.slow_fields);
            for c in self.constructs.iter_mut() {
                if c.category != FormCategory::Object || !c.comp.launched || c.attached() || c.gone.is_some() || c.parent == Some(c.id) {
                    continue;
                }
                for &(pid, center, radius, factor) in &fields {
                    if c.owner == pid || (c.center() - center).length_squared() > radius * radius {
                        continue;
                    }
                    let s = c.vel.length();
                    let n = slowed_speed(s, factor, dt);
                    if n < s {
                        c.vel *= n / s;
                    }
                }
            }
            self.powers.slow_fields = fields;
        }
        if !self.powers.cores.is_empty() {
            self.step_cores(dt);
        }
    }

    // ------------------------------------------------------------------------------ cores

    /// `MoveOp::Singularity`: throw a gravity core along the aim (announced like a signature: a
    /// hostile ConstructSpawned priced at the move's cost raises everyone's cut-in).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn op_singularity(&mut self, i: usize, eye: Vec3, look: Vec3, name: &str, cost: f32, op: (f32, f32, f32, f32, f32, f32, u8, f32, f32)) -> Result<(), PowerError> {
        let (dist, rise, secs, radius, strength, capture, chunks, damage, knockback) = op;
        if self.constructs.len() >= crate::world::MAX_CONSTRUCTS - 8 {
            return Err(PowerError::Busy);
        }
        let owner = self.players[i].id;
        let cw = crate::movement::CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(owner));
        let want = dist.clamp(2.0, 40.0);
        let d = crate::movement::raycast_world(&cw, eye, look, want, false).map_or(want, |h| (h.t - 1.5).max(1.5));
        let from = eye + look * d;
        let to = from + Vec3::Y * rise.clamp(0.0, 20.0);
        if !self.in_play(to, 0.0) {
            return Err(PowerError::NoTarget);
        }
        // The oldest core collapses early.
        if self.powers.cores.len() >= MAX_CORES {
            let k = self.powers.cores.iter().enumerate().max_by(|a, b| a.1.t.total_cmp(&b.1.t)).map_or(0, |(k, _)| k);
            self.powers.cores[k].secs = self.powers.cores[k].t;
        }
        let recipe = validate_recipe(
            Recipe {
                name: if name.is_empty() { "gravity core".into() } else { name.to_string() },
                form: Form::Ball,
                mechanics: vec![Mechanic::Attract, Mechanic::Explodes, Mechanic::Heavy],
                element: Element::Shadow,
                scale: 2.2,
                mass: 500.0,
                count: 1,
                accent: None,
                model: ModelSource::Procedural { seed: crate::rng::word_hash(name) as u32 },
                payload: None,
                features: vec![],
                grants: None,
                temperament: Default::default(),
                intensity: 0.0,
                mount: None,
            },
            &self.cfg.rules,
        );
        let tick = self.tick;
        let cid = self.constructs.spawn(SpawnParams {
            owner,
            recipe: recipe.clone(),
            shape: None,
            pos: from,
            rot: Quat::IDENTITY,
            vel: Vec3::ZERO,
            target: None,
            cost: 0.0,
            integrity: 400.0,
            forming: 0.0,
            life: f32::INFINITY,
            tick,
            colossal: false,
            model: None,
        });
        if let Some(c) = self.constructs.get_mut(cid) {
            c.parent = Some(cid);
            c.comp.launched = true;
            c.comp.dud = true; // it collapses on OUR schedule (the end blast), never on contact
            c.body.loco = Loco::Ballistic;
            c.body.gravity = 0.0;
            c.set_anim(AnimState::Active, tick);
        }
        let secs = secs.clamp(0.5, 12.0);
        let chunks = chunks.min(24);
        self.powers.cores.push(Core {
            owner,
            construct: cid,
            t: 0.0,
            secs,
            from,
            to,
            radius: radius.clamp(2.0, 30.0),
            strength: strength.clamp(0.0, 120.0),
            capture: capture.clamp(0.5, 8.0),
            chunks_left: chunks,
            chunk_every: if chunks > 0 { (secs - CORE_RISE_SECS).max(0.3) / chunks as f32 } else { f32::INFINITY },
            chunk_cd: CORE_RISE_SECS * 0.5,
            damage: damage.clamp(0.0, 60.0),
            knockback: knockback.clamp(0.0, 60.0),
            escaped: Vec::new(),
        });
        self.inject_event(GameEvent::ConstructSpawned { id: cid, owner, recipe: Box::new(recipe), pos: from, rot: Quat::IDENTITY, forming: 0.0, cost: cost.max(0.0) });
        let r = radius.clamp(2.0, 30.0);
        self.inject_event(GameEvent::Power(PowerEvent::Singularity { player: owner, construct: cid, pos: to, radius: r, secs, end: false }));
        Ok(())
    }

    fn step_cores(&mut self, dt: f32) {
        let base = self.cfg.move_params.clone();
        let mut cores = std::mem::take(&mut self.powers.cores);
        cores.retain_mut(|core| {
            core.t += dt;
            let alive = self.constructs.get(core.construct).is_some_and(|c| c.gone.is_none()) && self.pidx(core.owner).is_some();
            if !alive || core.t >= core.secs {
                self.end_core(core);
                return false;
            }
            // Rise, then hover.
            let u = (core.t / CORE_RISE_SECS).clamp(0.0, 1.0);
            let e = u * u * (3.0 - 2.0 * u);
            let pos = core.from + (core.to - core.from) * e;
            if let Some(c) = self.constructs.get_mut(core.construct) {
                let prev = c.pos;
                c.pos = pos;
                c.vel = (pos - prev) / dt.max(1e-4);
                c.body.gravity = 0.0;
                c.integrity = c.max_integrity;
            }
            for e in &mut core.escaped {
                e.1 -= dt;
            }
            core.escaped.retain(|e| e.1 > 0.0);
            if core.t >= CORE_RISE_SECS * 0.6 {
                self.core_pull_players(core, pos, dt, &base);
                self.core_pull_constructs(core, pos, dt);
                core.chunk_cd -= dt;
                if core.chunk_cd <= 0.0 && core.chunks_left > 0 {
                    core.chunk_cd += core.chunk_every;
                    core.chunks_left -= 1;
                    let n = PlayerId(core.chunks_left);
                    let a = roll(seed(self.tick, core.owner, n, 0xC0DE)) * std::f32::consts::TAU;
                    let r = core.radius * (0.35 + 0.55 * roll(seed(self.tick, core.owner, n, 0xD157)));
                    let top = Vec3::new(pos.x + a.cos() * r, pos.y, pos.z + a.sin() * r);
                    if let Some(h) = bd_core::voxel::raycast(&self.voxels, top, Vec3::NEG_Y, core.radius + 12.0) {
                        self.request_carve(CarveRequest { center: h.point, radius: 1.3, power: 1.5 });
                    }
                }
            }
            true
        });
        // (op_singularity never runs during this step; anything pushed meanwhile is kept.)
        cores.append(&mut self.powers.cores);
        self.powers.cores = cores;
    }

    fn core_pull_players(&mut self, core: &mut Core, pos: Vec3, dt: f32, base: &crate::movement::MoveParams) {
        let r2 = core.radius * core.radius;
        for q in 0..self.players.len() {
            let p = &self.players[q];
            if !p.alive || p.id == core.owner || p.seat.is_some() || p.shielded() {
                continue;
            }
            let c = p.hull(base).center();
            let d = pos - c;
            let len2 = d.length_squared();
            if len2 > r2 || len2 < 1e-6 {
                continue;
            }
            let len = len2.sqrt();
            let dir = d / len;
            let pid = p.id;
            let immune = core.escaped.iter().any(|e| e.0 == pid);
            let breaking = p.mv.extra.dodging() || p.mv.tethered();
            let p = &mut self.players[q];
            if len <= core.capture && !immune {
                if breaking {
                    // Dodge / tether out: a clean escape.
                    core.escaped.push((pid, CORE_ESCAPE_SECS));
                    p.mv.vel += -dir * 9.0 + Vec3::Y * 3.0;
                    continue;
                }
                // Held in the mass: pinned onto its shell.
                let shell = pos - dir * core.capture * 0.75;
                let feet_off = c - p.mv.pos;
                let want = shell - feet_off;
                p.mv.pos += (want - p.mv.pos) * (12.0 * dt).min(1.0);
                p.mv.vel = Vec3::ZERO;
                p.mv.grounded = false;
                p.mv.extra.cancel_dodge(&mut p.mv.vel);
                continue;
            }
            if immune {
                continue;
            }
            let pull = core.strength * (1.0 - len / core.radius).max(0.15);
            p.mv.vel += dir * pull * dt;
            let s = p.mv.vel.length();
            if s > 26.0 {
                p.mv.vel *= 26.0 / s;
            }
            if dir.y > 0.0 {
                p.mv.grounded = false;
            }
        }
    }

    fn core_pull_constructs(&mut self, core: &Core, pos: Vec3, dt: f32) {
        let r2 = core.radius * core.radius;
        let mut n = 0;
        for c in self.constructs.iter_mut() {
            if n >= CORE_MAX_CONSTRUCTS {
                break;
            }
            if c.id == core.construct || c.owner == core.owner || c.attached() || c.gone.is_some() || c.parent == Some(c.id) || c.pilot.is_some() {
                continue;
            }
            let d = pos - c.center();
            let len2 = d.length_squared();
            if len2 > r2 || len2 < 1e-6 {
                continue;
            }
            n += 1;
            let len = len2.sqrt();
            let dir = d / len;
            if len <= core.capture + c.half.max_element() {
                c.vel = Vec3::ZERO;
                c.pos += dir * (len - core.capture * 0.8).max(0.0) * (8.0 * dt).min(1.0);
                continue;
            }
            if matches!(c.body.loco, Loco::Static) {
                continue;
            }
            c.vel += dir * core.strength * 1.2 * dt;
            if c.body.loco == Loco::Kinematic && c.category == FormCategory::Structure {
                continue;
            }
        }
    }

    fn end_core(&mut self, core: &Core) {
        let pos = self.constructs.get(core.construct).map_or(core.to, |c| c.pos);
        if let Some(c) = self.constructs.get_mut(core.construct) {
            if c.gone.is_none() {
                c.gone = Some(GoneReason::Spent);
            }
        }
        if core.damage > 0.0 || core.knockback > 0.0 {
            self.explode(Explosion {
                pos,
                radius: (core.capture * 1.8).max(3.0),
                damage: core.damage,
                knockback: core.knockback,
                source: self.pidx(core.owner).map(|_| core.owner),
                construct: None,
                element: Element::Shadow,
                carve_radius: 0.0,
                self_damage: false,
            });
        }
        self.inject_event(GameEvent::Power(PowerEvent::Singularity { player: core.owner, construct: core.construct, pos, radius: core.radius, secs: core.secs, end: true }));
    }
}
