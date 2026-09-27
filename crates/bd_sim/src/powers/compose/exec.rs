//! Firing a composed move on the host (`World::fire_composed`), the same rules as a borrowed move
//! (docs/toolbox/COMPOSED_MOVE.md §Transport):
//!
//! validate (alive, powers on, not stunned / reciting / charging / locked out) → re-sanitize +
//! expand the spec → per-spec cooldown ([`ComposedMove::cd_key`] in the toolbox's cooldown table)
//! → price (presence tier of the expanded move → sparks, + staleness per repeat, `chain::
//! move_sparks`) → resolve the TARGET and aim the ops at it → run the ops (the move only counts —
//! and only costs — if its first op did something) → pay, cooldown, events
//! (`PowerEvent::Composed` + `PowerEvent::Fired { how: Composed }`), active phase for chains.
//!
//! Deterministic; nothing here runs (or allocates per tick) unless someone fires a composed move.

use bd_core::{ConstructId, MAX_PLAYERS, PlayerId, Vec3};

use super::{ComposedMove, Target, composed_power};
use crate::events::GameEvent;
use crate::powers::chain::{FireHow, active_phase, move_element, move_sparks, op_kind};
use crate::powers::toolbox::MoveRef;
use crate::powers::{MoveOp, PowerError, PowerEvent};
use crate::world::World;

/// How long "him" keeps meaning the last player / thing you went for (s).
pub const PRONOUN_SECS: f32 = 4.0;
/// How long a chain's `Same` target is remembered (s).
pub const SAME_SECS: f32 = 8.0;
/// Farthest a named / remembered target may be for the aim to swing to it (m).
pub const TARGET_RANGE: f32 = 90.0;

/// The `MoveRef` a composed move's `Fired` event carries (power 0xFFFF never resolves: read the
/// spec from the `Composed` event that precedes it).
pub const COMPOSED_REF: MoveRef = MoveRef { power: u16::MAX, tier: 0, slot: 0 };

/// A resolved target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Forced {
    Player(PlayerId),
    Construct(ConstructId),
    /// Yourself ("me", "myself").
    Me,
}

/// Per-player memory (in `OpState::compose`): the last thing a composed move went for, and the
/// target of the move being fired right now (read by `Relocate`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ComposeMemo {
    pub owner: Option<PlayerId>,
    pub last: Option<Forced>,
    pub last_tick: u64,
    pub now: Option<Forced>,
}

impl World {
    /// Seconds since tick `t`.
    fn compose_age(&self, t: u64) -> f32 {
        self.tick.saturating_sub(t) as f32 * bd_core::DT
    }

    fn compose_memo(&self, i: usize) -> Option<ComposeMemo> {
        let m = *self.powers.ops.compose.get(i)?;
        (m.owner == Some(self.players[i].id)).then_some(m)
    }

    fn forced_valid(&self, i: usize, f: Forced) -> bool {
        match f {
            Forced::Me => true,
            Forced::Player(id) => self.player(id).is_some_and(|q| q.alive && q.id != self.players[i].id),
            Forced::Construct(c) => self.constructs.get(c).is_some_and(|c| c.gone.is_none()),
        }
    }

    /// Where a resolved target is (its middle).
    pub(crate) fn forced_point(&self, f: Forced) -> Option<Vec3> {
        match f {
            Forced::Me => None,
            Forced::Player(id) => self.player(id).filter(|q| q.alive).map(|q| q.hull(&self.cfg.move_params).center()),
            Forced::Construct(c) => self.constructs.get(c).filter(|c| c.gone.is_none()).map(|c| c.pos),
        }
    }

    /// The enemy nearest the crosshair (generous), else the nearest enemy in front within 35 m.
    fn compose_aimed_enemy(&self, i: usize) -> Option<Forced> {
        if let Some(k) = self.op_aimed_player(i, 70.0, 25.0) {
            return Some(Forced::Player(self.players[k].id));
        }
        let (eye, look) = self.op_eye_look(i);
        let mut best: Option<(f32, PlayerId)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive {
                continue;
            }
            let d = q.hull(&self.cfg.move_params).center() - eye;
            let len = d.length();
            if !(0.3..=35.0).contains(&len) || d.dot(look) < 0.0 {
                continue;
            }
            if best.is_none_or(|b| len < b.0) {
                best = Some((len, q.id));
            }
        }
        best.map(|b| Forced::Player(b.1))
    }

    fn compose_nearest_enemy(&self, i: usize) -> Option<Forced> {
        let me = self.players[i].mv.pos;
        let mut best: Option<(f32, PlayerId)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive {
                continue;
            }
            let d = (q.mv.pos - me).length();
            if d <= TARGET_RANGE && best.is_none_or(|b| d < b.0) {
                best = Some((d, q.id));
            }
        }
        best.map(|b| Forced::Player(b.1))
    }

    /// The construct nearest the crosshair (≤ 60 m, 25° cone), any owner.
    fn compose_aimed_construct(&self, i: usize) -> Option<Forced> {
        let (eye, look) = self.op_eye_look(i);
        let cos = 25f32.to_radians().cos();
        let mut best: Option<(f32, ConstructId)> = None;
        for c in self.constructs.iter() {
            if c.gone.is_some() || c.parent.is_some() {
                continue;
            }
            let d = c.pos - eye;
            let len = d.length();
            if !(0.5..=60.0).contains(&len) {
                continue;
            }
            let dot = d.dot(look) / len;
            if dot >= cos && best.is_none_or(|b| dot > b.0) {
                best = Some((dot, c.id));
            }
        }
        best.map(|b| Forced::Construct(b.1))
    }

    /// Resolve a spec's target for player `i` (host; see [`Target`]).
    pub(crate) fn compose_resolve(&self, i: usize, t: Target) -> Option<Forced> {
        let me = self.players[i].id;
        let memo = self.compose_memo(i);
        let remembered = |max: f32| memo.and_then(|m| m.last.filter(|f| self.compose_age(m.last_tick) <= max && self.forced_valid(i, *f)));
        match t {
            Target::Aim | Target::Ground | Target::Everyone => None,
            Target::Me => Some(Forced::Me),
            Target::Player(id) => {
                let ok = id != me && self.player(id).is_some_and(|q| q.alive && (q.mv.pos - self.players[i].mv.pos).length() <= TARGET_RANGE);
                if ok { Some(Forced::Player(id)) } else { self.compose_aimed_enemy(i) }
            }
            Target::Pronoun => remembered(PRONOUN_SECS).filter(|f| matches!(f, Forced::Player(_))).or_else(|| self.compose_aimed_enemy(i)),
            Target::Same => remembered(SAME_SECS).or_else(|| self.compose_aimed_enemy(i)),
            Target::Nearest => self.compose_nearest_enemy(i),
            Target::It => {
                // your newest live construct, else the one under the crosshair
                let own = self.constructs.owned_by(me).filter(|c| c.parent.is_none() && c.gone.is_none()).max_by_key(|c| c.id.0).map(|c| Forced::Construct(c.id));
                own.or_else(|| self.compose_aimed_construct(i))
            }
            Target::Thing(h) => {
                let pos = self.players[i].mv.pos;
                let mut best: Option<(f32, ConstructId)> = None;
                for c in self.constructs.iter() {
                    if c.gone.is_some() || c.parent.is_some() {
                        continue;
                    }
                    let named = crate::powers::norm_words(&c.recipe.name).split(' ').any(|w| super::id16(w) == h || super::id16(w.trim_end_matches('s')) == h);
                    if !named {
                        continue;
                    }
                    let d = (c.pos - pos).length();
                    if d <= TARGET_RANGE && best.is_none_or(|b| d < b.0) {
                        best = Some((d, c.id));
                    }
                }
                // a misheard name / an unnamed thing: the construct under the crosshair, else "him"
                best.map(|b| Forced::Construct(b.1)).or_else(|| self.compose_aimed_construct(i)).or_else(|| self.compose_aimed_enemy(i))
            }
        }
    }

    /// Fire a composed move for player index `i` (from `fire_step`; `move_aim` already holds the
    /// client's validated crosshair ray if one came with the command).
    pub(crate) fn fire_composed(&mut self, i: usize, spec: ComposedMove) -> Result<(), PowerError> {
        let player = self.players[i].id;
        let p = &self.players[i];
        if !p.alive {
            return Err(PowerError::Dead);
        }
        if !self.cfg.rules.powers_enabled {
            return Err(PowerError::Disabled);
        }
        if p.status.stunned() || p.oath.reciting() || p.charge.active || p.seat.is_some_and(|s| !s.suit) || self.locked_at(i) {
            return Err(PowerError::Busy);
        }
        let Some(spec) = spec.sanitized() else { return Err(PowerError::NoSuchMove) };
        let Some(mv) = spec.expand() else { return Err(PowerError::NoSuchMove) };
        let key = spec.cd_key();
        let window = self.cfg.tuning.willpower.stale_window;
        let (uses, cooling) = match p.power.toolbox.cd_of_key(key) {
            Some(c) => (if c.since <= window { c.uses as u32 } else { 0 }, c.left > 0.0),
            None => (0, false),
        };
        if cooling {
            return Err(PowerError::Cooldown);
        }
        let rules = &self.cfg.rules;
        let cost = if rules.infinite_willpower() { 0.0 } else { move_sparks(&self.cfg.tuning.willpower.sparks, &mv, uses, rules.pool_segments()) as f32 * rules.segment_size() };
        if cost > p.willpower + 1e-4 {
            return Err(PowerError::NotEnoughWillpower);
        }
        // Target → aim: every aimed op (a lunge, a spell, a pull, a swap…) now points at it.
        let forced = self.compose_resolve(i, spec.target);
        let saved_aim = self.powers.move_aim;
        let (eye, _) = self.eye_look(i);
        if let Some(pt) = forced.and_then(|f| self.forced_point(f)) {
            let d = pt - eye;
            if d.length_squared() > 0.01 {
                self.powers.move_aim = Some((i, eye, d.normalize()));
            }
        }
        if i < MAX_PLAYERS {
            let m = &mut self.powers.ops.compose[i];
            if m.owner != Some(player) {
                *m = super::ComposeMemo { owner: Some(player), ..Default::default() };
            }
            m.now = forced;
        }
        let from = self.players[i].mv.pos;
        let (_, dir) = self.eye_look(i);
        let def = composed_power();
        let mut first: Option<Result<(), PowerError>> = None;
        for op in mv.ops.iter().filter(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_))) {
            let r = self.run_op(i, def, &mv, op);
            if first.is_none() {
                first = Some(r);
                if r.is_err() {
                    break;
                }
            }
            if !self.players[i].alive {
                break;
            }
        }
        self.powers.move_aim = saved_aim;
        if i < MAX_PLAYERS {
            self.powers.ops.compose[i].now = None;
        }
        if let Some(Err(e)) = first {
            return Err(e);
        }
        // remember who / what this went for ("him" next time, `Same` in a chain)
        if i < MAX_PLAYERS && matches!(forced, Some(Forced::Player(_) | Forced::Construct(_))) {
            let tick = self.tick;
            let m = &mut self.powers.ops.compose[i];
            m.last = forced;
            m.last_tick = tick;
        }
        let cd = mv.cooldown * crate::powers::effects::cooldown_mult(&self.players[i].power);
        let p = &mut self.players[i];
        p.willpower = (p.willpower - cost).max(0.0);
        let slot = p.power.toolbox.slot_for_key(key);
        let repeat = slot.since <= window && slot.left <= 0.0 && slot.full > 0.0;
        slot.uses = if repeat { slot.uses.saturating_add(1) } else { 1 };
        slot.left = cd;
        slot.full = cd.max(0.05);
        slot.since = 0.0;
        let to = p.mv.pos;
        let step = p.power.toolbox.fired;
        let sparks = (cost / self.cfg.rules.segment_size()).round() as u8;
        let op = mv.ops.first().map_or(crate::powers::chain::OpKind::Other, op_kind);
        let element = move_element(&mv);
        self.inject_event(GameEvent::Power(PowerEvent::Composed { player, spec, from, to, dir, step, sparks }));
        self.inject_event(GameEvent::Power(PowerEvent::Fired { player, mv: COMPOSED_REF, how: FireHow::Composed, op, element, from, to, dir, step, sparks }));
        let (secs, land) = active_phase(&mv);
        self.set_active(i, secs, land);
        Ok(())
    }

    /// The target the composed move being fired right now resolved to (for `Relocate`).
    pub(crate) fn compose_now(&self, i: usize) -> Option<Forced> {
        self.compose_memo(i).and_then(|m| m.now)
    }
}
