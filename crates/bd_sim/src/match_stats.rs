//! Competitive match stats for the Tab board / end screen: an observer over the sim's events
//! (like [`crate::telemetry`], but small, per player and shown to everyone).
//!
//! ```text
//! let mut stats = MatchStats::default();
//! loop {
//!     world.tick(&inputs);
//!     stats.observe(&world);          // reads world.events()
//! }
//! let lines = stats.lines();          // host → clients (bd_game LobbyInfo)
//! ```
//!
//! It never writes to the [`World`], so the sim (and its golden hashes) cannot change. KOs, falls,
//! stocks and party points are *not* here: the board reads those from the world itself (they are
//! already replicated in every snapshot). Counted only while the match is live (warmup fights do
//! not count); FFA resets when a match goes live, party mode when a new party starts.

use bd_core::{DT, PlayerId, Tick};
use serde::{Deserialize, Serialize};

use crate::combat::HitKind;
use crate::events::{GameEvent, HeldFx};
use crate::rules::MatchPhase;
use crate::world::World;

/// Hits by the same attacker on the same victim no further apart than this chain into one combo.
pub const COMBO_GAP_SECS: f32 = 1.5;

/// One player's numbers (compact: travels host → clients about once a second).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatLine {
    pub player: PlayerId,
    /// KOs with no one to credit (void / blast zone on your own).
    pub self_kos: u16,
    /// Damage % dealt to other players / taken from anything (rounded).
    pub dealt: u32,
    pub taken: u32,
    /// Longest KO streak (KOs without dying).
    pub best_streak: u16,
    /// Longest combo (hits on one victim, each within [`COMBO_GAP_SECS`], before they recovered).
    pub best_combo: u16,
    /// Weapon hits on players (ring blasts, shots, melee) / weapon attacks (shots fired — each
    /// pellet — and melee swings).
    pub hits: u32,
    pub attacks: u32,
    /// Constructs summoned (swarm units / payload children don't count).
    pub constructs: u16,
}

impl Default for StatLine {
    fn default() -> Self {
        Self { player: PlayerId(0), self_kos: 0, dealt: 0, taken: 0, best_streak: 0, best_combo: 0, hits: 0, attacks: 0, constructs: 0 }
    }
}

impl StatLine {
    /// Hits / attacks in %, `None` before the first attack.
    pub fn accuracy(&self) -> Option<u8> {
        (self.attacks > 0).then(|| ((self.hits.min(self.attacks) as u64 * 100 + self.attacks as u64 / 2) / self.attacks as u64) as u8)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Acc {
    seen: bool,
    self_kos: u16,
    dealt: f64,
    taken: f64,
    streak: u16,
    best_streak: u16,
    best_combo: u16,
    hits: u32,
    attacks: u32,
    constructs: u16,
}

/// The chain currently landing on one victim.
#[derive(Clone, Copy, Debug, Default)]
struct Combo {
    source: Option<PlayerId>,
    last: Tick,
    count: u16,
}

/// Per-player counters (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct MatchStats {
    acc: Vec<Acc>,
    combos: Vec<Combo>,
    live: Option<bool>,
    last_tick: Option<Tick>,
}

fn slot<T: Default + Clone>(v: &mut Vec<T>, id: PlayerId) -> &mut T {
    let i = id.0 as usize;
    if v.len() <= i {
        v.resize(i + 1, T::default());
    }
    &mut v[i]
}

/// Weapon hits (the accuracy numerator): what a trigger pull / swing lands.
fn weapon_hit(k: HitKind) -> bool {
    matches!(k, HitKind::RingBlast | HitKind::Shot | HitKind::Melee | HitKind::HeavyMelee)
}

impl MatchStats {
    /// Forget everything (a new match).
    pub fn reset(&mut self) {
        self.acc.clear();
        self.combos.clear();
    }

    /// Record the world's latest events (call once after each `World::tick`; a repeated call for
    /// the same tick is ignored).
    pub fn observe(&mut self, w: &World) {
        if self.last_tick == Some(w.tick) {
            return;
        }
        self.last_tick = Some(w.tick);
        self.observe_events(w.events(), w);
    }

    /// Record `events` (one tick's, in sim order). `w` = the world after that tick.
    pub fn observe_events(&mut self, events: &[GameEvent], w: &World) {
        let party = w.party().is_some();
        let mut live = *self.live.get_or_insert(w.match_state.phase == MatchPhase::Live);
        let gap = (COMBO_GAP_SECS / DT).round() as Tick;
        let tick = w.tick;
        for e in events {
            match e {
                GameEvent::MatchPhase { phase: MatchPhase::Live, .. } => {
                    if !party {
                        self.reset();
                    }
                    live = true;
                }
                GameEvent::MatchPhase { phase, .. } => {
                    if party && *phase == MatchPhase::Warmup {
                        self.reset(); // a new party
                    }
                    live = false;
                }
                _ if !live => {}
                GameEvent::Hit { target, source, kind, damage, .. } => {
                    let d = if damage.is_finite() { f64::from(damage.max(0.0)) } else { 0.0 };
                    slot(&mut self.acc, *target).taken += d;
                    slot(&mut self.acc, *target).seen = true;
                    let Some(s) = source.filter(|s| s != target) else { continue };
                    let a = slot(&mut self.acc, s);
                    a.seen = true;
                    a.dealt += d;
                    if weapon_hit(*kind) {
                        a.hits += 1;
                    }
                    let c = slot(&mut self.combos, *target);
                    if c.source == Some(s) && tick.saturating_sub(c.last) <= gap {
                        c.count = c.count.saturating_add(1);
                    } else {
                        *c = Combo { source: Some(s), last: tick, count: 1 };
                    }
                    c.last = tick;
                    let n = c.count;
                    let a = slot(&mut self.acc, s);
                    a.best_combo = a.best_combo.max(n);
                }
                GameEvent::Ko { victim, killer, streak, .. } => {
                    match killer.filter(|k| k != victim) {
                        Some(k) => {
                            let a = slot(&mut self.acc, k);
                            a.seen = true;
                            a.streak = (*streak).max(a.streak.saturating_add(1));
                            a.best_streak = a.best_streak.max(a.streak);
                        }
                        None => slot(&mut self.acc, *victim).self_kos += 1,
                    }
                    let v = slot(&mut self.acc, *victim);
                    v.seen = true;
                    v.streak = 0;
                    *slot(&mut self.combos, *victim) = Combo::default();
                }
                GameEvent::Respawn { player, .. } | GameEvent::Recovered { player, .. } => {
                    *slot(&mut self.combos, *player) = Combo::default();
                }
                GameEvent::ShotFired { owner, pellets, .. } => {
                    let a = slot(&mut self.acc, *owner);
                    a.seen = true;
                    a.attacks += u32::from((*pellets).max(1));
                }
                GameEvent::HeldFx { owner, fx: HeldFx::SwingActive, .. } => {
                    let a = slot(&mut self.acc, *owner);
                    a.seen = true;
                    a.attacks += 1;
                }
                GameEvent::ConstructSpawned { id, owner, .. } => {
                    let child = w.constructs.get(*id).is_some_and(|c| c.parent.is_some_and(|p| p != *id));
                    if !child {
                        let a = slot(&mut self.acc, *owner);
                        a.seen = true;
                        a.constructs = a.constructs.saturating_add(1);
                    }
                }
                _ => {}
            }
        }
        self.live = Some(live);
    }

    /// One line per player in `w` (in world order).
    pub fn lines_for(&self, w: &World) -> Vec<StatLine> {
        w.players.iter().map(|p| self.line(p.id)).collect()
    }

    /// Every player seen so far.
    pub fn lines(&self) -> Vec<StatLine> {
        (0..self.acc.len()).filter(|&i| self.acc[i].seen || self.acc[i].self_kos > 0).map(|i| self.line(PlayerId(i as u8))).collect()
    }

    pub fn line(&self, id: PlayerId) -> StatLine {
        let a = self.acc.get(id.0 as usize).copied().unwrap_or_default();
        let r = |x: f64| x.round().clamp(0.0, u32::MAX as f64) as u32;
        StatLine {
            player: id,
            self_kos: a.self_kos,
            dealt: r(a.dealt),
            taken: r(a.taken),
            best_streak: a.best_streak,
            best_combo: a.best_combo,
            hits: a.hits,
            attacks: a.attacks,
            constructs: a.constructs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{ArenaSource, SimConfig};
    use bd_core::corps::Corps;
    use bd_core::Vec3;

    fn world() -> World {
        let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() };
        cfg.rules.ko_target = 50;
        cfg.rules.spawn_shield_secs = 0.0;
        let mut w = World::new(cfg);
        for (i, c) in [Corps::Rage, Corps::Hope, Corps::Fear].into_iter().enumerate() {
            w.add_player(PlayerId(i as u8), format!("p{i}"), c).unwrap();
        }
        w
    }

    fn tick(w: &mut World, s: &mut MatchStats, n: usize) {
        for _ in 0..n {
            w.tick(&[]);
            s.observe(w);
        }
    }

    fn hit(w: &mut World, target: u8, source: Option<u8>, kind: HitKind, dmg: f32) {
        if let Some(p) = w.player_mut(PlayerId(target)) {
            p.spawn_shield = 0.0;
        }
        let pos = w.player(PlayerId(target)).map_or(Vec3::ZERO, |p| p.mv.pos);
        let h = crate::HitInfo::new(PlayerId(target), source.map(PlayerId), kind, dmg, 0.5, Vec3::Y, pos);
        w.apply_hit(h);
    }

    fn drop_into_void(w: &mut World, id: u8) {
        if let Some(p) = w.player_mut(PlayerId(id)) {
            p.mv.pos = Vec3::new(p.mv.pos.x, -500.0, p.mv.pos.z);
        }
    }

    /// Scripted: hits, a combo, a credited KO, a self-KO, a streak; the warmup doesn't count.
    #[test]
    fn scripted_counters_are_exact() {
        let mut w = world();
        let mut s = MatchStats::default();
        tick(&mut w, &mut s, 30);
        // Warmup hit: not counted.
        hit(&mut w, 1, Some(0), HitKind::Melee, 10.0);
        tick(&mut w, &mut s, 1);
        w.start_live();
        tick(&mut w, &mut s, 400); // everyone spawned, spawn shields over
        assert_eq!(s.line(PlayerId(0)), StatLine { player: PlayerId(0), ..Default::default() }, "warmup ignored, reset at live");
        // A 3-hit combo from p0 on p1 (each 0.25 s apart), then a lone hit 3 s later.
        for _ in 0..3 {
            hit(&mut w, 1, Some(0), HitKind::Melee, 5.0);
            tick(&mut w, &mut s, 30);
        }
        tick(&mut w, &mut s, 360);
        hit(&mut w, 1, Some(0), HitKind::Shot, 7.0);
        tick(&mut w, &mut s, 1);
        // Environmental damage on p2 (no source): taken only.
        hit(&mut w, 2, None, HitKind::Crush, 4.0);
        tick(&mut w, &mut s, 1);
        let l0 = s.line(PlayerId(0));
        let l1 = s.line(PlayerId(1));
        assert_eq!(l0.best_combo, 3);
        assert_eq!(l0.hits, 4);
        assert!((l0.dealt as i64 - 22).abs() <= 1, "dealt {}", l0.dealt);
        assert!(l1.taken >= 22, "taken {}", l1.taken);
        assert_eq!(s.line(PlayerId(2)).taken, 4);
        assert_eq!(s.line(PlayerId(2)).dealt, 0);
        // p0 KOs p1 (hit, then the void inside the credit window) → streak 1.
        hit(&mut w, 1, Some(0), HitKind::Melee, 5.0);
        drop_into_void(&mut w, 1);
        tick(&mut w, &mut s, 2);
        assert_eq!(w.player(PlayerId(0)).unwrap().kos, 1, "credited");
        // p2 falls with nobody to blame → self-KO.
        drop_into_void(&mut w, 2);
        tick(&mut w, &mut s, 2);
        assert_eq!(s.line(PlayerId(2)).self_kos, 1);
        assert_eq!(s.line(PlayerId(0)).best_streak, 1);
        // Respawn, then a second credited KO without dying → streak 2.
        tick(&mut w, &mut s, 600);
        hit(&mut w, 2, Some(0), HitKind::Melee, 5.0);
        drop_into_void(&mut w, 2);
        tick(&mut w, &mut s, 2);
        let l0 = s.line(PlayerId(0));
        assert_eq!(w.player(PlayerId(0)).unwrap().kos, 2);
        assert_eq!(l0.best_streak, 2);
        assert_eq!(l0.self_kos, 0);
        assert_eq!(s.line(PlayerId(1)).self_kos, 0);
        // p0 dies on its own: streak resets, best stays.
        drop_into_void(&mut w, 0);
        tick(&mut w, &mut s, 2);
        assert_eq!(s.line(PlayerId(0)).best_streak, 2);
        assert_eq!(s.line(PlayerId(0)).self_kos, 1);
        assert_eq!(s.lines_for(&w).len(), 3);
    }

    #[test]
    fn accuracy_rounds_and_caps() {
        let l = StatLine { hits: 1, attacks: 3, ..Default::default() };
        assert_eq!(l.accuracy(), Some(33));
        assert_eq!(StatLine { hits: 9, attacks: 3, ..Default::default() }.accuracy(), Some(100));
        assert_eq!(StatLine::default().accuracy(), None);
    }

    /// Bots: totals agree with the hidden telemetry recorder (same events, other bookkeeping).
    #[test]
    fn bot_match_agrees_with_telemetry() {
        let seed = 9;
        let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed }, seed, ..SimConfig::default() };
        cfg.timings.warmup_secs = 1.0;
        cfg.rules.ko_target = 50;
        let mut w = World::new(cfg);
        let mut brains = crate::bots::add_bots(&mut w, 0, 4, 0.8, seed);
        let mut tel = crate::telemetry::Telemetry::new(seed);
        let mut s = MatchStats::default();
        let mut inputs = Vec::new();
        for _ in 0..(30 * 120) {
            inputs.clear();
            crate::bots::drive(&mut w, &mut brains, &mut inputs);
            w.tick(&inputs);
            tel.observe(&w);
            s.observe(&w);
        }
        let r = tel.finish(&w);
        let dealt: u64 = s.lines_for(&w).iter().map(|l| l.dealt as u64).sum();
        let tdealt: f64 = r.players.iter().map(|p| p.damage_dealt).sum();
        assert!(dealt > 0, "bots dealt damage");
        // Telemetry also counts warmup hits; the board only counts the live match.
        assert!(dealt as f64 <= tdealt + 4.0, "{dealt} vs {tdealt}");
        let casts: u32 = s.lines_for(&w).iter().map(|l| l.constructs as u32).sum();
        assert!(casts > 0, "bots cast");
        for l in s.lines_for(&w) {
            assert!(l.best_combo as u32 <= l.hits.max(l.best_combo as u32));
            assert!(l.accuracy().is_none_or(|a| a <= 100));
        }
    }
}
