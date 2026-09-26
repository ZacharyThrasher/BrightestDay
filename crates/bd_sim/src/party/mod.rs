//! **Party mode** (COMBAT.md §5g): rounds → pick an effect → rounds … → podium. Deterministic and
//! host-authoritative; everything random comes from the party's own seeded RNG.
//!
//! ```text
//!  Lobby (warmup) ─► Round (match Live) ─► RoundEnd ─► Pick ─► Reveal ─► Round … ─► PartyEnd ─► Lobby
//! ```
//!
//! * **Round** = one FFA match: ends at the round KO target or round time (the world's normal
//!   `check_match_end` with party knobs). Points: `kos · points_per_ko`, + `points_round_win` for the
//!   round winner(s) (most KOs, then fewest deaths; exact ties share it).
//! * **Pick**: every player is offered 3 distinct effects they don't own (seeded). First place (party
//!   points) gets 2; last place gets 3 + **Boom or Bust**. Ties: every tied player gets that rule; if
//!   everybody is tied nobody is first or last. `World::party_choose(player, index)` locks a card
//!   (`PartyLocked`); timeout ⇒ random. The phase ends early once everyone locked.
//! * **Reveal**: picks resolve (Boom or Bust = 50/50 random Legendary / Cursed, `PartyBoomOrBust`),
//!   `PartyReveal` shows everyone's pick, then the next round starts (arena regenerated — see
//!   [`World::party_arena_request`]).
//! * **PartyEnd**: `PartyEnd` podium; after `party_end_secs` (if `timings.auto_rematch`) a new party.
//!
//! Effects: `effects.rs` (data), `graft.rs` (standing graft), `hooks.rs` (event hooks).
//! Replication: [`PartySnap`] in every snapshot (standings, owned effects, phase clock, and the
//! recipient's own offer) + the `Party*` events.

pub mod effects;
pub mod graft;
pub mod hooks;

use std::sync::Arc;

use bd_core::mechanics::Recipe;
use bd_core::{ConstructId, DT, PlayerId, Tick, Vec3};
use bd_voxel::{ArenaInfo, VoxelWorld};
use serde::{Deserialize, Serialize};

use crate::combat::Explosion;
use crate::events::GameEvent;
use crate::rng::{Fnv64, SimRng};
use crate::rules::MatchPhase;
use crate::settings::PartySettings;
use crate::world::{Aim, ArenaSource, World};

pub use effects::{EffectId, EffectSpec, EffectTable, Hook, Rarity};
pub use graft::{StandingGraft, apply_graft};
pub use hooks::{PartyFx, modify_cast};

/// Effects one player can own in a party (15 rounds ⇒ at most 14 picks).
pub const MAX_OWNED: usize = 16;
/// Card value meaning "Boom or Bust" in [`PartyOffer::cards`].
pub const BOOM_CARD: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PartyPhase {
    /// Warmup before the first round (the match's Warmup).
    #[default]
    Lobby = 0,
    Round = 1,
    RoundEnd = 2,
    Pick = 3,
    Reveal = 4,
    PartyEnd = 5,
}

impl PartyPhase {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Round,
            2 => Self::RoundEnd,
            3 => Self::Pick,
            4 => Self::Reveal,
            5 => Self::PartyEnd,
            _ => Self::Lobby,
        }
    }
}

/// A card on the pick screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PartyCard {
    Effect(EffectId),
    BoomOrBust,
}

/// One player's offer (fixed size, no heap). `cards[..n]`; [`BOOM_CARD`] = Boom or Bust.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartyOffer {
    pub cards: [u16; 4],
    pub n: u8,
}

impl PartyOffer {
    pub fn card(&self, i: u8) -> Option<PartyCard> {
        (i < self.n.min(4)).then(|| match self.cards[i as usize] {
            BOOM_CARD => PartyCard::BoomOrBust,
            e => PartyCard::Effect(e),
        })
    }

    pub fn len(&self) -> usize {
        self.n.min(4) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    fn push(&mut self, c: u16) {
        if (self.n as usize) < 4 {
            self.cards[self.n as usize] = c;
            self.n += 1;
        }
    }
}

/// Why `party_choose` refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PickError {
    NotParty,
    NotPicking,
    UnknownPlayer,
    AlreadyPicked,
    BadIndex,
}

/// Per-player party state.
#[derive(Clone, Debug, PartialEq)]
pub struct PartyPlayer {
    pub id: PlayerId,
    pub points: u16,
    pub round_wins: u8,
    pub total_kos: u16,
    /// Owned effects in pick order: `effects[..n_effects]`.
    pub effects: [EffectId; MAX_OWNED],
    pub n_effects: u8,
    /// Union of the owned effects' hook bits (`effects::hook_bits`).
    pub hooks: u32,
    /// This pick's offer and locked card.
    pub offer: PartyOffer,
    pub picked: Option<u8>,
    // ---- hook runtime (host) ----
    pub(crate) t_butter: f32,
    pub(crate) t_hiccup: f32,
    pub(crate) t_sidekick: f32,
    pub(crate) t_loud: f32,
    pub(crate) cd_copycat: f32,
    pub(crate) cd_pinata: f32,
    pub(crate) cd_popcorn: f32,
    pub(crate) sidekick: Option<ConstructId>,
}

impl PartyPlayer {
    fn new(id: PlayerId) -> Self {
        Self {
            id,
            points: 0,
            round_wins: 0,
            total_kos: 0,
            effects: [0; MAX_OWNED],
            n_effects: 0,
            hooks: 0,
            offer: PartyOffer::default(),
            picked: None,
            t_butter: 0.0,
            t_hiccup: 0.0,
            t_sidekick: 0.0,
            t_loud: 0.0,
            cd_copycat: 0.0,
            cd_pinata: 0.0,
            cd_popcorn: 0.0,
            sidekick: None,
        }
    }

    pub fn owned(&self) -> &[EffectId] {
        &self.effects[..(self.n_effects as usize).min(MAX_OWNED)]
    }

    pub fn owns(&self, e: EffectId) -> bool {
        self.owned().contains(&e)
    }

    pub fn has_hook(&self, bit: u32) -> bool {
        self.hooks & bit != 0
    }
}

/// Where a random phrase comes from for Wild Magic. The sim has no lexicon: the game layer injects
/// one (e.g. a random lexicon entry resolved to a recipe). Must be deterministic in `seed` (host
/// only). Without one the sim uses [`crate::fuzz::random_sane_recipe`].
pub trait PhraseSource: Send + Sync {
    fn random_recipe(&self, seed: u64) -> Option<Recipe>;
}

/// Something a hook does later (at the end of tick `due`).
#[derive(Clone, Debug)]
pub(crate) enum Scheduled {
    /// A free cast (Echo). `parent` = the construct it echoes.
    Cast { due: Tick, player: PlayerId, recipe: Box<Recipe>, aim: Aim, parent: Option<ConstructId> },
    Explosion { due: Tick, e: Explosion },
    Portal { due: Tick, owner: PlayerId, pos: Vec3 },
    Field { due: Tick, owner: PlayerId, pos: Vec3, radius: f32, strength: f32, secs: f32 },
}

impl Scheduled {
    fn due(&self) -> Tick {
        match self {
            Scheduled::Cast { due, .. } | Scheduled::Explosion { due, .. } | Scheduled::Portal { due, .. } | Scheduled::Field { due, .. } => *due,
        }
    }
}

/// The party (lives in `World::party`; `None` = FFA).
pub struct PartyState {
    pub settings: PartySettings,
    pub table: Arc<EffectTable>,
    pub phase: PartyPhase,
    /// Seconds in the current party phase (Round: see the match clock).
    pub phase_time: f32,
    /// 1-based once the first round started.
    pub round: u8,
    /// Sorted by id.
    pub players: Vec<PartyPlayer>,
    /// Union of every player's hooks (fast skip).
    pub any_hooks: u32,
    pub(crate) rng: SimRng,
    pub(crate) scheduled: Vec<Scheduled>,
    /// (mini-me, original).
    pub(crate) companions: Vec<(ConstructId, ConstructId)>,
    /// Main Character constructs that already split once.
    pub(crate) staged: Vec<ConstructId>,
    /// Recent projectile id → owner (Popcorn needs the owner of an ended projectile).
    pub(crate) proj_owner: [(u16, u8); 256],
    pub(crate) proj_head: usize,
    /// Scratch for the pre-reap pass: (construct, owner hooks).
    pub(crate) shatter_scratch: Vec<(ConstructId, u32)>,
    pub(crate) pending_arena: Option<Box<(VoxelWorld, ArenaInfo, u64)>>,
    pub(crate) phrase_source: Option<Arc<dyn PhraseSource>>,
    pub(crate) pinata_pool: Vec<Recipe>,
}

impl PartyState {
    pub fn new(settings: PartySettings, seed: u64) -> Self {
        Self {
            settings: settings.validated(),
            table: EffectTable::shipped(),
            phase: PartyPhase::Lobby,
            phase_time: 0.0,
            round: 0,
            players: Vec::with_capacity(bd_core::MAX_PLAYERS),
            any_hooks: 0,
            rng: SimRng::new(seed ^ 0x9A27_7E55_EFFE_C75u64),
            scheduled: Vec::with_capacity(32),
            companions: Vec::with_capacity(16),
            staged: Vec::with_capacity(16),
            proj_owner: [(0, 0); 256],
            proj_head: 0,
            shatter_scratch: Vec::with_capacity(16),
            pending_arena: None,
            phrase_source: None,
            pinata_pool: hooks::pinata_pool(),
        }
    }

    pub fn player(&self, id: PlayerId) -> Option<&PartyPlayer> {
        self.players.binary_search_by_key(&id, |p| p.id).ok().map(|i| &self.players[i])
    }

    pub(crate) fn player_mut(&mut self, id: PlayerId) -> Option<&mut PartyPlayer> {
        self.players.binary_search_by_key(&id, |p| p.id).ok().map(move |i| &mut self.players[i])
    }

    pub fn hooks_of(&self, id: PlayerId) -> u32 {
        self.player(id).map_or(0, |p| p.hooks)
    }

    /// Owned effect specs of `id` in pick order.
    pub fn effects_of(&self, id: PlayerId) -> impl Iterator<Item = &EffectSpec> {
        self.player(id).into_iter().flat_map(|p| p.owned().iter()).filter_map(|e| self.table.get(*e))
    }

    fn recompute_hooks(&mut self) {
        let table = self.table.clone();
        let mut any = 0;
        for p in &mut self.players {
            p.hooks = p.owned().iter().filter_map(|e| table.get(*e)).fold(0, |b, s| b | s.hook_bits());
            any |= p.hooks;
        }
        self.any_hooks = any;
    }

    /// Give `id` effect `e` (no duplicates, ≤ [`MAX_OWNED`]). Returns false if not added.
    pub fn grant(&mut self, id: PlayerId, e: EffectId) -> bool {
        if self.table.get(e).is_none() {
            return false;
        }
        let Some(p) = self.player_mut(id) else { return false };
        if p.owns(e) || p.n_effects as usize >= MAX_OWNED {
            return false;
        }
        p.effects[p.n_effects as usize] = e;
        p.n_effects += 1;
        self.recompute_hooks();
        true
    }

    /// Standings best-first: points desc, round wins desc, total KOs desc, id asc.
    pub fn standings(&self) -> Vec<PartyStanding> {
        let mut v: Vec<PartyStanding> = self
            .players
            .iter()
            .map(|p| PartyStanding { player: p.id, points: p.points, round_wins: p.round_wins, total_kos: p.total_kos, place: 0 })
            .collect();
        v.sort_by(|a, b| b.points.cmp(&a.points).then(b.round_wins.cmp(&a.round_wins)).then(b.total_kos.cmp(&a.total_kos)).then(a.player.cmp(&b.player)));
        for i in 0..v.len() {
            // Ties on points share a place.
            let better = v.iter().filter(|o| o.points > v[i].points).count();
            v[i].place = (better + 1).min(255) as u8;
        }
        v
    }

    /// (first-place rule applies, last-place rule applies) for `points` — nobody when all tied or
    /// fewer than two players.
    pub fn place_rules(&self, points: u16) -> (bool, bool) {
        if self.players.len() < 2 {
            return (false, false);
        }
        let max = self.players.iter().map(|p| p.points).max().unwrap_or(0);
        let min = self.players.iter().map(|p| p.points).min().unwrap_or(0);
        if max == min {
            return (false, false);
        }
        (points == max, points == min)
    }

    /// Seconds left in the current phase (Round: the match clock decides).
    pub fn phase_secs(&self) -> f32 {
        let s = &self.settings;
        match self.phase {
            PartyPhase::Lobby | PartyPhase::Round => 0.0,
            PartyPhase::RoundEnd => s.round_end_secs,
            PartyPhase::Pick => s.pick_secs,
            PartyPhase::Reveal => s.reveal_secs,
            PartyPhase::PartyEnd => s.party_end_secs,
        }
    }

    /// Deterministic digest (tests / desync checks).
    pub fn hash_into(&self, h: &mut Fnv64) {
        h.write(&[self.phase as u8, self.round]);
        h.write_f32(self.phase_time);
        for p in &self.players {
            h.write(&[p.id.0, p.n_effects, p.round_wins, p.picked.unwrap_or(255)]);
            h.write(&p.points.to_le_bytes());
            for e in p.owned() {
                h.write(&e.to_le_bytes());
            }
            for c in &p.offer.cards[..p.offer.len()] {
                h.write(&c.to_le_bytes());
            }
        }
        h.write_u64(self.scheduled.len() as u64);
    }

    /// Draw `n` distinct unowned effects of the given rarities.
    fn draw(&mut self, owner: usize, rarities: &[Rarity], n: usize, out: &mut PartyOffer) {
        let mut cands: Vec<EffectId> = (0..self.table.len() as EffectId)
            .filter(|e| self.table.get(*e).is_some_and(|s| rarities.contains(&s.rarity)) && !self.players[owner].owns(*e))
            .collect();
        for _ in 0..n {
            if cands.is_empty() {
                break;
            }
            let k = self.rng.below(cands.len() as u32) as usize;
            out.push(cands.remove(k));
        }
    }

    fn offer_rarities(&self) -> Vec<Rarity> {
        let s = &self.settings;
        if s.pool_common {
            vec![Rarity::Common]
        } else {
            let mut v = Vec::new();
            if s.pool_legendary {
                v.push(Rarity::Legendary);
            }
            if s.pool_cursed {
                v.push(Rarity::Cursed);
            }
            v
        }
    }

    fn any_unowned(&self, owner: usize, r: Rarity) -> bool {
        self.table.effects.iter().enumerate().any(|(i, s)| s.rarity == r && !self.players[owner].owns(i as EffectId))
    }

    /// Build every player's offer for this pick.
    fn make_offers(&mut self) {
        let rar = self.offer_rarities();
        for k in 0..self.players.len() {
            let (first, last) = self.place_rules(self.players[k].points);
            let n = if first && self.settings.first_place_two_choices { 2 } else { 3 };
            let mut offer = PartyOffer::default();
            self.draw(k, &rar, n, &mut offer);
            let boom_pool = (self.settings.pool_legendary && self.any_unowned(k, Rarity::Legendary))
                || (self.settings.pool_cursed && self.any_unowned(k, Rarity::Cursed));
            if last && self.settings.boom_or_bust && boom_pool {
                offer.push(BOOM_CARD);
            }
            let p = &mut self.players[k];
            p.offer = offer;
            p.picked = None;
        }
    }

    /// Resolve a Boom-or-Bust card: (boom?, effect).
    fn boom_or_bust(&mut self, owner: usize) -> (bool, Option<EffectId>) {
        let boom = self.rng.chance(0.5);
        let pick = |me: &mut Self, r: Rarity, enabled: bool| -> Option<EffectId> {
            if !enabled {
                return None;
            }
            let cands: Vec<EffectId> =
                (0..me.table.len() as EffectId).filter(|e| me.table.get(*e).is_some_and(|s| s.rarity == r) && !me.players[owner].owns(*e)).collect();
            if cands.is_empty() { None } else { Some(cands[me.rng.below(cands.len() as u32) as usize]) }
        };
        let (pl, pc) = (self.settings.pool_legendary, self.settings.pool_cursed);
        if boom {
            match pick(self, Rarity::Legendary, pl) {
                Some(e) => (true, Some(e)),
                None => (false, pick(self, Rarity::Cursed, pc)),
            }
        } else {
            match pick(self, Rarity::Cursed, pc) {
                Some(e) => (false, Some(e)),
                None => (true, pick(self, Rarity::Legendary, pl)),
            }
        }
    }

    /// Sync the player list with the world's (joins get a fresh row; leavers are dropped).
    fn sync_players(&mut self, world_players: &[crate::world::Player]) {
        let before = self.players.len();
        self.players.retain(|p| world_players.binary_search_by_key(&p.id, |q| q.id).is_ok());
        let mut changed = before != self.players.len();
        for q in world_players {
            if let Err(at) = self.players.binary_search_by_key(&q.id, |p| p.id) {
                self.players.insert(at, PartyPlayer::new(q.id));
                changed = true;
            }
        }
        if changed {
            self.recompute_hooks();
        }
    }

    fn reset(&mut self) {
        self.phase = PartyPhase::Lobby;
        self.phase_time = 0.0;
        self.round = 0;
        for p in &mut self.players {
            *p = PartyPlayer::new(p.id);
        }
        self.any_hooks = 0;
        self.clear_round_runtime();
    }

    fn clear_round_runtime(&mut self) {
        self.scheduled.clear();
        self.companions.clear();
        self.staged.clear();
        for p in &mut self.players {
            p.sidekick = None;
            p.t_sidekick = 0.0;
        }
    }

    // -------------------------------------------------------------------------- replication

    /// Fill `out` for `recipient` (their own offer is included; others' offers are private).
    pub fn write_snap(&self, recipient: Option<PlayerId>, time_left: f32, out: &mut PartySnap) {
        out.phase = self.phase as u8;
        out.round = self.round;
        out.rounds = self.settings.rounds;
        out.time_left = time_left.ceil().clamp(0.0, u16::MAX as f32) as u16;
        out.players.clear();
        for p in &self.players {
            let mut effects = [0u16; MAX_OWNED];
            effects[..p.owned().len()].copy_from_slice(p.owned());
            let place = 1 + self.players.iter().filter(|o| o.points > p.points).count() as u8;
            out.players.push(PartyPlayerSnap {
                id: p.id.0,
                points: p.points,
                round_wins: p.round_wins,
                place,
                effects,
                n_effects: p.n_effects,
                locked: p.picked.is_some(),
            });
        }
        out.offer = recipient.and_then(|r| self.player(r)).filter(|_| self.phase == PartyPhase::Pick).map(|p| p.offer);
    }

    /// Client mirror: adopt a snapshot (phase, standings, owned effects ⇒ hooks for prediction).
    pub fn apply_snap(&mut self, s: &PartySnap) {
        self.phase = PartyPhase::from_u8(s.phase);
        self.round = s.round;
        self.players.clear();
        for q in &s.players {
            let mut p = PartyPlayer::new(PlayerId(q.id));
            p.points = q.points;
            p.round_wins = q.round_wins;
            p.n_effects = q.n_effects.min(MAX_OWNED as u8);
            p.effects = q.effects;
            p.picked = q.locked.then_some(0);
            self.players.push(p);
        }
        self.players.sort_by_key(|p| p.id);
        self.recompute_hooks();
    }
}

/// One standings row (best first in [`PartyState::standings`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartyStanding {
    pub player: PlayerId,
    pub points: u16,
    pub round_wins: u8,
    pub total_kos: u16,
    /// 1 = first; tied points share a place.
    pub place: u8,
}

/// A player's round result (in `PartyRoundEnd`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartyRoundResult {
    pub player: PlayerId,
    pub kos: u16,
    pub deaths: u16,
    /// Points earned this round (KOs + round-win bonus).
    pub gained: u16,
    /// Party total after the round.
    pub points: u16,
}

/// A resolved pick (in `PartyReveal`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartyPick {
    pub player: PlayerId,
    pub card: PartyCard,
    /// What was gained (a Boom-or-Bust shows the rolled effect). `None` = nothing left to give.
    pub effect: Option<EffectId>,
    /// Picked by timeout.
    pub random: bool,
}

/// Per-player party state in a snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartyPlayerSnap {
    pub id: u8,
    pub points: u16,
    pub round_wins: u8,
    pub place: u8,
    pub effects: [u16; MAX_OWNED],
    pub n_effects: u8,
    /// Locked a card this pick.
    pub locked: bool,
}

/// Party state for one recipient (in `WorldSnapshot::party`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartySnap {
    /// [`PartyPhase`] as u8.
    pub phase: u8,
    pub round: u8,
    pub rounds: u8,
    /// Seconds left in the phase (Round: the round clock).
    pub time_left: u16,
    pub players: Vec<PartyPlayerSnap>,
    /// The recipient's own offer while picking.
    pub offer: Option<PartyOffer>,
}

// ============================================================================== World: flow

impl World {
    /// The party, if this world runs Party mode.
    pub fn party(&self) -> Option<&PartyState> {
        self.party.as_ref()
    }

    /// Replace the effect table (e.g. a hot-reloaded `party_effects.ron`). Host and clients must
    /// use the same table.
    pub fn set_party_effects(&mut self, table: Arc<EffectTable>) {
        if let Some(p) = self.party.as_mut() {
            p.table = table;
            p.recompute_hooks();
        }
    }

    /// Inject the game layer's random-phrase source for Wild Magic.
    pub fn set_party_phrase_source(&mut self, src: Arc<dyn PhraseSource>) {
        if let Some(p) = self.party.as_mut() {
            p.phrase_source = Some(src);
        }
    }

    /// Give a player an effect directly (debug / tests / soak). Returns false if not added.
    pub fn grant_party_effect(&mut self, player: PlayerId, e: EffectId) -> bool {
        let Some(mut party) = self.party.take() else { return false };
        party.sync_players(&self.players);
        let ok = party.grant(player, e);
        self.party = Some(party);
        ok
    }

    /// Lock card `index` of `player`'s offer (pick screen).
    pub fn party_choose(&mut self, player: PlayerId, index: u8) -> Result<(), PickError> {
        let Some(party) = self.party.as_mut() else { return Err(PickError::NotParty) };
        if party.phase != PartyPhase::Pick {
            return Err(PickError::NotPicking);
        }
        let Some(p) = party.player_mut(player) else { return Err(PickError::UnknownPlayer) };
        if p.picked.is_some() {
            return Err(PickError::AlreadyPicked);
        }
        if p.offer.card(index).is_none() {
            return Err(PickError::BadIndex);
        }
        p.picked = Some(index);
        self.events.push(GameEvent::PartyLocked { player });
        Ok(())
    }

    /// Host arena regeneration off the tick thread: while this returns `Some(src)` (from the end of
    /// a round until the next starts) build `World::build_arena(src)` on a worker and hand it to
    /// [`Self::provide_party_arena`]. If nothing was provided when the round starts, the world
    /// builds it synchronously when `timings.auto_rematch` (headless / tests) and otherwise keeps
    /// the current terrain — never a hitch on a live host.
    pub fn party_arena_request(&self) -> Option<ArenaSource> {
        let p = self.party.as_ref()?;
        let between = matches!(p.phase, PartyPhase::RoundEnd | PartyPhase::Pick | PartyPhase::Reveal) && p.round < p.settings.rounds;
        (between && p.pending_arena.is_none() && self.arena_needs_regen()).then_some(self.cfg.arena)
    }

    /// Terrain built off-thread for the next round (see [`Self::party_arena_request`]).
    pub fn provide_party_arena(&mut self, voxels: VoxelWorld, arena: ArenaInfo, seed: u64) {
        if let Some(p) = self.party.as_mut() {
            p.pending_arena = Some(Box::new((voxels, arena, seed)));
        }
    }

    fn arena_needs_regen(&self) -> bool {
        self.cfg.rules.regen_islands_between_rounds && self.carve_seed > 1
    }

    fn party_phase_event(&mut self) {
        if let Some(p) = &self.party {
            let (phase, round, rounds, secs) = (p.phase, p.round, p.settings.rounds, p.phase_secs());
            self.events.push(GameEvent::PartyPhase { phase, round, rounds, secs });
        }
    }

    /// Replaces `step_match` in Party mode (called from it).
    pub(crate) fn step_party_match(&mut self, dt: f32) {
        let Some(mut party) = self.party.take() else { return };
        party.sync_players(&self.players);
        let t = self.cfg.timings.clone();
        let phase = party.phase;
        if phase != PartyPhase::Lobby && phase != PartyPhase::Round {
            party.phase_time += dt;
        }
        if self.match_state.phase != MatchPhase::Warmup {
            self.match_state.phase_time += dt;
        }
        let secs = party.phase_secs();
        let all_locked = party.players.iter().all(|p| p.picked.is_some() || p.offer.is_empty());
        let rounds = party.settings.rounds;
        let round = party.round;
        self.party = Some(party);
        match phase {
            PartyPhase::Lobby => {
                if self.players.len() >= t.min_players.max(1) {
                    self.match_state.phase_time += dt;
                    if self.match_state.phase_time >= t.warmup_secs {
                        self.party_start_round();
                    }
                } else {
                    self.match_state.phase_time = 0.0;
                }
            }
            PartyPhase::Round => {
                // Round end is detected by `check_match_end` → `party_round_over`.
                if self.match_state.phase != MatchPhase::Live {
                    self.party_round_over();
                }
            }
            PartyPhase::RoundEnd => {
                if self.party_phase_time() >= secs {
                    if round >= rounds {
                        self.party_end();
                    } else {
                        self.party_enter_pick();
                    }
                }
            }
            PartyPhase::Pick => {
                if self.party_phase_time() >= secs || all_locked {
                    self.party_reveal();
                }
            }
            PartyPhase::Reveal => {
                if self.party_phase_time() >= secs {
                    self.party_start_round();
                }
            }
            PartyPhase::PartyEnd => {
                if t.auto_rematch && self.party_phase_time() >= secs {
                    self.rematch();
                }
            }
        }
    }

    fn party_phase_time(&self) -> f32 {
        self.party.as_ref().map_or(0.0, |p| p.phase_time)
    }

    fn set_party_phase(&mut self, phase: PartyPhase) {
        if let Some(p) = self.party.as_mut() {
            p.phase = phase;
            p.phase_time = 0.0;
        }
        self.party_phase_event();
    }

    /// Start the next round (regenerate the arena between rounds; see `party_arena_request`).
    pub(crate) fn party_start_round(&mut self) {
        let Some(p) = self.party.as_mut() else { return };
        let first = p.round == 0;
        p.round = p.round.saturating_add(1).min(p.settings.rounds);
        p.clear_round_runtime();
        let pending = p.pending_arena.take();
        let round = p.round;
        if !first {
            if let Some(b) = pending {
                let (v, a, s) = *b;
                self.swap_arena(v, a, s);
            } else if self.arena_needs_regen() && self.cfg.timings.auto_rematch {
                let (v, a, s) = World::build_arena(self.cfg.arena);
                self.swap_arena(v, a, s);
            }
        }
        self.match_state.round = round as u32;
        self.start_live_ffa();
        self.set_party_phase(PartyPhase::Round);
    }

    /// The round's match just ended: score it.
    pub(crate) fn party_round_over(&mut self) {
        let Some(mut party) = self.party.take() else { return };
        if party.phase != PartyPhase::Round {
            self.party = Some(party);
            return;
        }
        party.sync_players(&self.players);
        let (ppk, win) = (party.settings.points_per_ko, party.settings.points_round_win);
        let best = self.players.iter().map(|p| p.kos).max().unwrap_or(0);
        let fewest = self.players.iter().filter(|p| p.kos == best).map(|p| p.deaths).min().unwrap_or(0);
        let mut results = Vec::with_capacity(self.players.len());
        let mut winners = Vec::new();
        for p in &self.players {
            let won = best > 0 && p.kos == best && p.deaths == fewest;
            let gained = p.kos.saturating_mul(ppk).saturating_add(if won { win } else { 0 });
            if won {
                winners.push(p.id);
            }
            if let Some(pp) = party.player_mut(p.id) {
                pp.points = pp.points.saturating_add(gained);
                pp.total_kos = pp.total_kos.saturating_add(p.kos);
                if won {
                    pp.round_wins = pp.round_wins.saturating_add(1);
                }
                results.push(PartyRoundResult { player: p.id, kos: p.kos, deaths: p.deaths, gained, points: pp.points });
            }
        }
        let round = party.round;
        party.phase = PartyPhase::RoundEnd;
        party.phase_time = 0.0;
        self.party = Some(party);
        self.events.push(GameEvent::PartyRoundEnd { round, results, winners });
        self.party_phase_event();
    }

    fn party_enter_pick(&mut self) {
        let Some(mut party) = self.party.take() else { return };
        party.make_offers();
        for p in &party.players {
            self.events.push(GameEvent::PartyOffer { player: p.id, offer: p.offer });
        }
        self.party = Some(party);
        self.set_party_phase(PartyPhase::Pick);
    }

    fn party_reveal(&mut self) {
        let Some(mut party) = self.party.take() else { return };
        let mut picks = Vec::with_capacity(party.players.len());
        for k in 0..party.players.len() {
            let offer = party.players[k].offer;
            if offer.is_empty() {
                continue;
            }
            let (idx, random) = match party.players[k].picked {
                Some(i) => (i, false),
                None => (party.rng.below(offer.n as u32) as u8, true),
            };
            let Some(card) = offer.card(idx) else { continue };
            let id = party.players[k].id;
            let effect = match card {
                PartyCard::Effect(e) => Some(e),
                PartyCard::BoomOrBust => {
                    let (boom, e) = party.boom_or_bust(k);
                    self.events.push(GameEvent::PartyBoomOrBust { player: id, boom, effect: e });
                    e
                }
            };
            if let Some(e) = effect {
                party.grant(id, e);
            }
            party.players[k].picked = Some(idx);
            picks.push(PartyPick { player: id, card, effect, random });
        }
        self.party = Some(party);
        self.events.push(GameEvent::PartyReveal { picks });
        self.set_party_phase(PartyPhase::Reveal);
    }

    fn party_end(&mut self) {
        let Some(party) = self.party.as_ref() else { return };
        let podium = party.standings();
        self.match_state.winner = podium.first().map(|s| s.player);
        self.events.push(GameEvent::PartyEnd { podium });
        self.set_party_phase(PartyPhase::PartyEnd);
    }

    /// Back to the lobby with a fresh party (called by rematch).
    pub(crate) fn party_reset(&mut self) {
        if let Some(p) = self.party.as_mut() {
            p.reset();
            self.party_phase_event();
        }
    }

    /// Party time left in the current phase (Round: the match clock).
    pub fn party_time_left(&self) -> f32 {
        match self.party.as_ref() {
            Some(p) if matches!(p.phase, PartyPhase::Round | PartyPhase::Lobby) => self.time_left(),
            Some(p) => (p.phase_secs() - p.phase_time).max(0.0),
            None => 0.0,
        }
    }
}

/// Party timings in ticks (hooks).
pub(crate) fn ticks(secs: f32) -> Tick {
    (secs.max(0.0) / DT).round() as Tick
}
