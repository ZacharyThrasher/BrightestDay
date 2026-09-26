//! **Map interactables** (High Roller casino; generic: any map can declare them in
//! [`bd_voxel::ArenaInfo::interactables`]). Optional flavour that *enhances* what players already do —
//! never forced, never a trap (owner map rule). Host-authoritative and deterministic: everything
//! random comes from this module's own [`SimRng`] (seeded from the world seed), so no other random
//! stream shifts on casino maps and maps without interactables pay nothing (one length check).
//!
//! * **Slot machines.** A player pulls a big machine's lever by *hitting* it — a melee swing / lunge /
//!   slam or ring shove within reach, a ring blast / bolt / bullet / explosion landing on it — or by
//!   *hooking it with a tether*. A pull costs a small willpower stake (`tuning.casino.stake`; can't
//!   pay ⇒ the lever just clunks, nothing is lost) and puts that machine on a short cooldown. The reels
//!   spin for `spin_secs`, then pay out:
//!   - usually a random **party effect** (a common one whose behaviour is its standing graft) grafted
//!     onto every cast of the puller for `effect_secs` — the same graft machinery as party mode,
//!     working in any mode, priced as typed (like party grafts);
//!   - rarely the **jackpot**: willpower refilled + the puller's next fresh cast is legendary (the
//!     hidden legendary path, `freedom::spawn_prelude`);
//!   - sometimes a **bust**: harmless comedy (confetti / a pie / a little hop) — never damage, never a
//!     KO, never more than the stake.
//!
//!   Odds live in `assets/tuning/combat.ron` (`casino`) and are **never shown** (mystery tenet): the
//!   result is what happens.
//! * **Roulette "house spin".** Every `house_every` s the wheel's lights sweep (`house_sweep` s,
//!   telegraphed to everyone) and settle on a random pocket region; for `house_lit` s, players
//!   standing in it regain willpower faster. A lure to fight over, never a trap.
//!
//! Hooks (each a commented one-liner): [`step`] (end of the world tick, before the party hooks),
//! [`modify_cast`] (`cast_inner`, after the party graft), [`take_lucky`] (`freedom::spawn_prelude`),
//! [`World::interact_reset`] (`reset_round`). Clients learn everything from [`CasinoEvent`]s
//! (`GameEvent::Casino`); prediction needs nothing (payouts are host events; the bust hop reconciles
//! like any knock).

use bd_core::mechanics::Recipe;
use bd_core::{PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use super::World;
use crate::events::{GameEvent, HeldFx};
use crate::party::{EffectId, EffectTable, Rarity, apply_graft};
use crate::projectile::ProjectileKind;
use crate::rng::SimRng;
use crate::willpower::validate_recipe;

/// Every casino number (`assets/tuning/combat.ron` → `casino`). Hidden from players.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CasinoTuning {
    /// Willpower a pull costs (can't pay ⇒ the lever clunks and nothing happens).
    pub stake: f32,
    /// Per-machine cooldown from the pull (s; includes the spin).
    pub cooldown: f32,
    /// How long the reels spin before the payout (s).
    pub spin_secs: f32,
    /// How long a won party effect lasts (s).
    pub effect_secs: f32,
    /// Chance of the jackpot / a bust per pull (the rest pays a party effect).
    pub p_jackpot: f32,
    pub p_bust: f32,
    /// Upward speed of the "little hop" bust (m/s; harmless, no tumble).
    pub bust_hop: f32,
    /// Melee reach from the swinging hand to the lever knob (m).
    pub melee_reach: f32,
    /// First house spin after the round starts, then one every `house_every` s.
    pub house_first: f32,
    pub house_every: f32,
    /// Telegraph: the lights sweep for this long before settling (s).
    pub house_sweep: f32,
    /// The winning region stays lit this long (s).
    pub house_lit: f32,
    /// Pockets either side of the winning pocket that count as the lit region.
    pub house_half_width: u8,
    /// Extra willpower per second while standing in the lit region.
    pub house_bonus_per_sec: f32,
}

impl Default for CasinoTuning {
    fn default() -> Self {
        Self {
            stake: 12.0,
            cooldown: 3.0,
            spin_secs: 1.5,
            effect_secs: 20.0,
            p_jackpot: 0.06,
            p_bust: 0.24,
            bust_hop: 7.5,
            melee_reach: 2.6,
            house_first: 20.0,
            house_every: 45.0,
            house_sweep: 3.0,
            house_lit: 10.0,
            house_half_width: 2,
            house_bonus_per_sec: 6.0,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl CasinoTuning {
    /// Clamp into sane ranges (untrusted / hand-edited data; NaN → default).
    pub fn validated(self) -> Self {
        let d = Self::default();
        let p_jackpot = fin(self.p_jackpot, d.p_jackpot, 0.0, 1.0);
        let p_bust = fin(self.p_bust, d.p_bust, 0.0, 1.0 - p_jackpot);
        let spin_secs = fin(self.spin_secs, d.spin_secs, 0.1, 10.0);
        let house_sweep = fin(self.house_sweep, d.house_sweep, 0.5, 20.0);
        let house_lit = fin(self.house_lit, d.house_lit, 1.0, 120.0);
        Self {
            stake: fin(self.stake, d.stake, 0.0, 200.0),
            cooldown: fin(self.cooldown, d.cooldown, spin_secs, 120.0),
            spin_secs,
            effect_secs: fin(self.effect_secs, d.effect_secs, 1.0, 600.0),
            p_jackpot,
            p_bust,
            bust_hop: fin(self.bust_hop, d.bust_hop, 0.0, 12.0),
            melee_reach: fin(self.melee_reach, d.melee_reach, 0.5, 6.0),
            house_first: fin(self.house_first, d.house_first, 0.0, 600.0),
            house_every: fin(self.house_every, d.house_every, house_sweep + house_lit + 1.0, 3600.0),
            house_sweep,
            house_lit,
            house_half_width: self.house_half_width.min(18),
            house_bonus_per_sec: fin(self.house_bonus_per_sec, d.house_bonus_per_sec, 0.0, 100.0),
        }
    }
}

/// What pulled a lever.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PullBy {
    /// A melee swing / lunge / slam or a ring shove.
    Melee = 0,
    /// A ring blast or an explosion.
    Blast = 1,
    /// A bolt, arrow, bullet or hitscan shot.
    Shot = 2,
    /// A tether hooked the lever.
    Tether = 3,
}

/// A harmless bust (the client plays the comedy; only `Hop` touches the sim).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum BustKind {
    /// Confetti bursts in your face.
    Confetti = 0,
    /// A cream pie splats on your view for a moment.
    Pie = 1,
    /// A little spring pops you upward.
    Hop = 2,
}

/// What a machine paid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlotOutcome {
    /// A party effect grafted onto the puller's casts for `tuning.casino.effect_secs`.
    Effect(EffectId),
    /// Willpower refilled + the next fresh cast is legendary.
    Jackpot,
    Bust(BustKind),
}

/// Casino happenings (`GameEvent::Casino`), all host-authoritative.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CasinoEvent {
    /// A lever was pulled: the reels of `machine` spin for `secs`.
    Pull { machine: u8, player: PlayerId, by: PullBy, secs: f32 },
    /// A lever was hit without the stake: it clunks, nothing happens.
    Clunk { machine: u8, player: PlayerId },
    /// The reels stopped.
    Payout { machine: u8, player: PlayerId, outcome: SlotOutcome },
    /// A slot-won effect wore off.
    EffectEnded { player: PlayerId, effect: EffectId },
    /// House spin: the lights sweep for `sweep` s, then pockets `pocket ± half_width` stay lit for
    /// `lit` s.
    HouseSpin { pocket: u8, half_width: u8, sweep: f32, lit: f32 },
    /// `player` stepped into the lit region (once per spin).
    HouseBonus { player: PlayerId, pocket: u8 },
}

/// Runtime of one machine.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MachineRt {
    /// Seconds until the lever can be pulled again.
    pub cooldown: f32,
    /// Seconds of spin left (0 = idle).
    pub spin: f32,
    pub puller: Option<PlayerId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HousePhase {
    #[default]
    Idle,
    Sweep,
    Lit,
}

/// Runtime of the roulette house spin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HouseRt {
    pub phase: HousePhase,
    /// Seconds in the current phase.
    pub t: f32,
    /// Seconds until the next spin starts (Idle).
    pub next: f32,
    pub pocket: u8,
    /// Players who already got this spin's `HouseBonus` event.
    pub paid: Vec<PlayerId>,
}

/// A slot-won effect on a player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timed {
    pub player: PlayerId,
    pub effect: EffectId,
    /// Seconds left.
    pub left: f32,
}

/// Recent projectile owners (id, owner): `ProjectileEnded` doesn't carry one.
const PROJ_RING: usize = 128;

/// All interactable state (lives in `World::interact`).
#[derive(Clone, Debug)]
pub struct InteractState {
    pub machines: Vec<MachineRt>,
    pub house: HouseRt,
    /// Slot-won effects (≤ one per player).
    pub timed: Vec<Timed>,
    /// Players whose next fresh cast is legendary (jackpot).
    pub lucky: Vec<PlayerId>,
    /// Which effects a machine can pay (common, graft-only; see [`effect_pool`]).
    pub pool: Vec<EffectId>,
    rng: SimRng,
    proj: [(u16, PlayerId, bool); PROJ_RING],
    proj_head: usize,
    /// The arena shape this state was built for (machines, has roulette).
    built: (usize, bool),
}

impl InteractState {
    pub fn new(seed: u64) -> Self {
        Self {
            machines: Vec::new(),
            house: HouseRt::default(),
            timed: Vec::with_capacity(bd_core::MAX_PLAYERS),
            lucky: Vec::with_capacity(bd_core::MAX_PLAYERS),
            pool: effect_pool(&EffectTable::shipped()),
            rng: SimRng::new(seed ^ 0x0CA5_14E0_7777_0001),
            proj: [(0, PlayerId(0), false); PROJ_RING],
            proj_head: 0,
            built: (0, false),
        }
    }

    /// The slot-won effect `player` is under, if any.
    pub fn effect_of(&self, player: PlayerId) -> Option<EffectId> {
        self.timed.iter().find(|t| t.player == player && t.left > 0.0).map(|t| t.effect)
    }

    pub fn is_lucky(&self, player: PlayerId) -> bool {
        self.lucky.contains(&player)
    }

    /// Fresh round: machines idle, effects and jackpot luck gone, first house spin rescheduled.
    fn reset(&mut self, n_machines: usize, roulette: bool, t: &CasinoTuning) {
        self.machines.clear();
        self.machines.resize(n_machines, MachineRt::default());
        self.house = HouseRt { next: t.house_first, ..Default::default() };
        self.timed.clear();
        self.lucky.clear();
        self.built = (n_machines, roulette);
    }

    fn proj_owner(&self, id: u16) -> Option<(PlayerId, bool)> {
        self.proj.iter().find(|(pid, _, _)| *pid == id && id != 0).map(|(_, o, blast)| (*o, *blast))
    }
}

/// Effects a slot machine can pay: common party effects whose whole behaviour is their standing
/// graft (hook effects need party-mode machinery; cursed / legendary ones are for Boom or Bust).
pub fn effect_pool(table: &EffectTable) -> Vec<EffectId> {
    table
        .effects
        .iter()
        .enumerate()
        .filter(|(_, e)| e.rarity == Rarity::Common && e.hooks.is_empty() && !e.graft.is_empty())
        .map(|(i, _)| i as EffectId)
        .collect()
}

impl World {
    /// INTERACT HOOK (`reset_round`): a new round starts every machine / the house spin fresh.
    pub(crate) fn interact_reset(&mut self) {
        let (n, r) = (self.arena.interactables.slots.len(), self.arena.interactables.roulette.is_some());
        let t = self.cfg.tuning.casino.clone();
        self.interact.reset(n, r, &t);
    }
}

/// Pull `machine` for `player` (host). Public for tools / tests; the tick finds pulls itself.
pub fn pull(w: &mut World, machine: usize, player: PlayerId, by: PullBy) -> bool {
    let t = &w.cfg.tuning.casino;
    let (stake, cooldown, spin) = (t.stake, t.cooldown, t.spin_secs);
    let Some(m) = w.interact.machines.get(machine).copied() else { return false };
    if m.cooldown > 0.0 {
        return false;
    }
    let Some(i) = w.pidx(player) else { return false };
    if !w.players[i].alive {
        return false;
    }
    let free = w.cfg.rules.infinite_willpower();
    if !free && w.players[i].willpower + 1e-4 < stake {
        // The lever clunks: nothing lost (a short lockout so a spray doesn't spam the sound).
        w.interact.machines[machine].cooldown = 0.5;
        w.events.push(GameEvent::Casino(CasinoEvent::Clunk { machine: machine as u8, player }));
        return false;
    }
    if !free {
        w.players[i].willpower = (w.players[i].willpower - stake).max(0.0);
    }
    w.interact.machines[machine] = MachineRt { cooldown, spin, puller: Some(player) };
    w.events.push(GameEvent::Casino(CasinoEvent::Pull { machine: machine as u8, player, by, secs: spin }));
    true
}

/// Roll a payout (host RNG).
fn roll(st: &mut InteractState, t: &CasinoTuning) -> SlotOutcome {
    let u = st.rng.f32();
    if u < t.p_jackpot {
        SlotOutcome::Jackpot
    } else if u < t.p_jackpot + t.p_bust {
        SlotOutcome::Bust(match st.rng.below(3) {
            0 => BustKind::Confetti,
            1 => BustKind::Pie,
            _ => BustKind::Hop,
        })
    } else if st.pool.is_empty() {
        SlotOutcome::Bust(BustKind::Confetti)
    } else {
        SlotOutcome::Effect(st.pool[st.rng.below(st.pool.len() as u32) as usize])
    }
}

fn pay(w: &mut World, machine: usize, player: PlayerId) {
    let t = w.cfg.tuning.casino.clone();
    let outcome = roll(&mut w.interact, &t);
    let Some(i) = w.pidx(player) else { return };
    match outcome {
        SlotOutcome::Effect(e) => {
            let st = &mut w.interact;
            match st.timed.iter_mut().find(|x| x.player == player) {
                Some(x) => *x = Timed { player, effect: e, left: t.effect_secs },
                None => st.timed.push(Timed { player, effect: e, left: t.effect_secs }),
            }
        }
        SlotOutcome::Jackpot => {
            w.players[i].willpower = w.cfg.rules.willpower_max();
            if !w.interact.lucky.contains(&player) {
                w.interact.lucky.push(player);
            }
        }
        SlotOutcome::Bust(BustKind::Hop) => {
            // A little spring: up only, no tumble, no damage — can't carry anyone out of bounds.
            let p = &mut w.players[i];
            if p.alive && p.seat.is_none() {
                p.mv.vel.y = p.mv.vel.y.max(t.bust_hop);
                p.mv.grounded = false;
            }
        }
        SlotOutcome::Bust(_) => {}
    }
    w.events.push(GameEvent::Casino(CasinoEvent::Payout { machine: machine as u8, player, outcome }));
}

/// INTERACT HOOK (world tick, after projectiles / constructs, before the party hooks): find pulls
/// in this tick's events + tether hooks, run spins / payouts, effect timers and the house spin.
pub fn step(w: &mut World, dt: f32) {
    let (n_slots, roulette) = (w.arena.interactables.slots.len(), w.arena.interactables.roulette.is_some());
    if n_slots == 0 && !roulette {
        if !w.interact.timed.is_empty() || !w.interact.lucky.is_empty() || w.interact.built != (0, false) {
            w.interact_reset();
        }
        return;
    }
    if w.interact.built != (n_slots, roulette) {
        w.interact_reset();
    }
    if n_slots > 0 {
        find_pulls(w);
        for k in 0..n_slots {
            let m = &mut w.interact.machines[k];
            m.cooldown = (m.cooldown - dt).max(0.0);
            if m.spin > 0.0 {
                m.spin -= dt;
                if m.spin <= 0.0 {
                    m.spin = 0.0;
                    if let Some(p) = m.puller.take() {
                        pay(w, k, p);
                    }
                }
            }
        }
    }
    // Slot-won effects wear off.
    let mut k = 0;
    while k < w.interact.timed.len() {
        w.interact.timed[k].left -= dt;
        let x = w.interact.timed[k];
        if x.left <= 0.0 || w.pidx(x.player).is_none() {
            let x = w.interact.timed.swap_remove(k);
            w.events.push(GameEvent::Casino(CasinoEvent::EffectEnded { player: x.player, effect: x.effect }));
        } else {
            k += 1;
        }
    }
    if w.interact.lucky.iter().any(|p| w.pidx(*p).is_none()) {
        let players = &w.players;
        w.interact.lucky.retain(|p| players.binary_search_by_key(p, |q| q.id).is_ok());
    }
    if roulette && w.match_state.phase != crate::rules::MatchPhase::Ended {
        step_house(w, dt);
    }
}

fn step_house(w: &mut World, dt: f32) {
    let t = w.cfg.tuning.casino.clone();
    let Some(wheel) = w.arena.interactables.roulette.as_ref() else { return };
    let n = wheel.pockets();
    if n == 0 {
        return;
    }
    let h = &mut w.interact.house;
    h.t += dt;
    match h.phase {
        HousePhase::Idle => {
            h.next -= dt;
            if h.next <= 0.0 {
                let pocket = w.interact.rng.below(n as u32) as u8;
                let h = &mut w.interact.house;
                *h = HouseRt { phase: HousePhase::Sweep, t: 0.0, next: t.house_every, pocket, paid: std::mem::take(&mut h.paid) };
                h.paid.clear();
                w.events.push(GameEvent::Casino(CasinoEvent::HouseSpin { pocket, half_width: t.house_half_width, sweep: t.house_sweep, lit: t.house_lit }));
            }
        }
        HousePhase::Sweep => {
            h.next -= dt;
            if h.t >= t.house_sweep {
                h.phase = HousePhase::Lit;
                h.t = 0.0;
            }
        }
        HousePhase::Lit => {
            h.next -= dt;
            let pocket = h.pocket as usize;
            let max = w.cfg.rules.willpower_max();
            for i in 0..w.players.len() {
                let p = &w.players[i];
                if !p.alive || !wheel.in_region(p.mv.pos, pocket, t.house_half_width as usize) {
                    continue;
                }
                let id = p.id;
                let p = &mut w.players[i];
                p.willpower = (p.willpower + t.house_bonus_per_sec * dt).min(max);
                if !w.interact.house.paid.contains(&id) {
                    w.interact.house.paid.push(id);
                    w.events.push(GameEvent::Casino(CasinoEvent::HouseBonus { player: id, pocket: pocket as u8 }));
                }
            }
            let h = &mut w.interact.house;
            if h.t >= t.house_lit {
                h.phase = HousePhase::Idle;
                h.t = 0.0;
            }
        }
    }
}

/// The machine whose lever knob is within `extra` m of its trigger sphere around `p`.
fn machine_at(w: &World, p: Vec3, extra: f32) -> Option<usize> {
    if !p.is_finite() {
        return None;
    }
    w.arena.interactables.slots.iter().position(|m| m.lever.distance_squared(p) <= (m.radius + extra) * (m.radius + extra))
}

fn find_pulls(w: &mut World) {
    // Scan this tick's events (and the commands before it): every event here is new.
    let mut pulls: [(usize, PlayerId, PullBy); 8] = [(usize::MAX, PlayerId(0), PullBy::Melee); 8];
    let mut n = 0;
    let reach = w.cfg.tuning.casino.melee_reach;
    let shove = (w.cfg.tuning.ring.shove_range, w.cfg.tuning.ring.shove_half_angle.cos());
    let mut push = |m: usize, p: PlayerId, by: PullBy| {
        if n < pulls.len() && !pulls[..n].iter().any(|x| x.0 == m) {
            pulls[n] = (m, p, by);
            n += 1;
        }
    };
    for e in w.events.iter() {
        match e {
            GameEvent::ProjectileFired { id, owner, kind, .. } => {
                let st = &mut w.interact;
                st.proj[st.proj_head] = (*id, *owner, *kind == ProjectileKind::RingBlast);
                st.proj_head = (st.proj_head + 1) % PROJ_RING;
            }
            GameEvent::ProjectileEnded { id, pos, hit_player: None, .. } => {
                if let (Some(m), Some((owner, blast))) = (machine_at(w, *pos, 0.35), w.interact.proj_owner(*id)) {
                    push(m, owner, if blast { PullBy::Blast } else { PullBy::Shot });
                }
            }
            GameEvent::ShotTrace { owner, to, hit_player: None, .. } => {
                if let Some(m) = machine_at(w, *to, 0.2) {
                    push(m, *owner, PullBy::Shot);
                }
            }
            GameEvent::Hitscan { owner, end, .. } => {
                if let Some(m) = machine_at(w, *end, 0.2) {
                    push(m, *owner, PullBy::Shot);
                }
            }
            GameEvent::Explosion { pos, radius, source: Some(owner), .. } => {
                if let Some(m) = machine_at(w, *pos, radius.clamp(0.0, 6.0)) {
                    push(m, *owner, PullBy::Blast);
                }
            }
            GameEvent::HeldFx { owner, fx, pos, dir, .. }
                if matches!(fx, HeldFx::SwingActive | HeldFx::SwingImpact | HeldFx::Lunge | HeldFx::DiveSlam | HeldFx::GroundPound | HeldFx::Uppercut) =>
            {
                for (k, m) in w.arena.interactables.slots.iter().enumerate() {
                    let d = m.lever - *pos;
                    if d.length_squared() <= (reach + m.radius) * (reach + m.radius) && d.dot(*dir) >= -0.25 * d.length() {
                        push(k, *owner, PullBy::Melee);
                    }
                }
            }
            GameEvent::Shove { player, origin, dir } => {
                for (k, m) in w.arena.interactables.slots.iter().enumerate() {
                    let d = m.lever - *origin;
                    let len = d.length();
                    if len <= shove.0 + m.radius && (len < 0.5 || d.dot(*dir) / len >= shove.1 - 0.1) {
                        push(k, *player, PullBy::Melee);
                    }
                }
            }
            _ => {}
        }
    }
    // Tethers that hooked a lever this tick.
    for p in &w.players {
        if !p.alive {
            continue;
        }
        for k in 0..2 {
            if p.move_events.tether_attached[k]
                && p.mv.hooks[k].attached()
                && let Some(m) = machine_at(w, p.mv.hooks[k].pos, 0.5)
            {
                push(m, p.id, PullBy::Tether);
            }
        }
    }
    for &(m, p, by) in &pulls[..n] {
        pull(w, m, p, by);
    }
}

/// INTERACT HOOK (`cast_inner`, after the party graft): a slot-won effect's standing graft rides on
/// every cast of its winner, priced as typed (like party grafts: the discount multiplies into
/// `party_cost_ratio`). No-op without slot effects.
pub(crate) fn modify_cast(w: &mut World, i: usize, r: &mut Recipe, autonomous: bool) {
    if w.interact.timed.is_empty() {
        return;
    }
    let Some(id) = w.players.get(i).map(|p| p.id) else { return };
    let Some(e) = w.interact.effect_of(id) else { return };
    let table = EffectTable::shipped();
    let Some(spec) = table.get(e) else { return };
    let before = w.cfg.tuning.willpower.base_cost_as(r, autonomous);
    if !apply_graft(r, &spec.graft) {
        return;
    }
    let cur = std::mem::replace(r, Recipe::unknown(""));
    *r = validate_recipe(cur, &w.cfg.rules);
    let after = w.cfg.tuning.willpower.base_cost_as(r, autonomous);
    if after > 0.0 && before.is_finite() && (after - before).abs() > 1e-6 {
        let ratio = w.party_cost_ratio.unwrap_or(1.0) * (before / after);
        w.party_cost_ratio = Some(ratio.clamp(0.05, 20.0));
    }
}

/// INTERACT HOOK (`freedom::spawn_prelude`): consume the jackpot luck — true once, for the
/// player's next fresh cast.
pub(crate) fn take_lucky(w: &mut World, player: PlayerId) -> bool {
    match w.interact.lucky.iter().position(|p| *p == player) {
        Some(k) => {
            w.interact.lucky.swap_remove(k);
            true
        }
        None => false,
    }
}
