//! Next-map vote rules — the host lobby's `GameSettings::vote` ([`MapVoteRules`], Match Rules
//! page "NEXT MAP"): which maps are offered after a match, the RANDOM card, hidden picks and
//! locked votes. Pure logic; [`HostCore`](super::host::HostCore) owns the open [`VoteState`].
//!
//! * **Offers** — `choices` maps (default 2; 0 = the whole pool) drawn by the host from the pool
//!   (`pool_off` bits): never the map just played (nor the one before while the pool allows),
//!   least-offered first so every map comes round over a session, ties by a seeded hash.
//!   Deterministic for the same history + seed.
//! * **RANDOM** — a pool map that isn't on offer (and isn't the map just played; falls back when the
//!   pool is small), rolled only when RANDOM wins and revealed with a short roulette
//!   ([`ROULETTE_SECS`]) on the end screen.
//! * **Hidden** (default on) — a player who hasn't voted sees who has voted, not what
//!   ([`VoteState::view_for`]; the host sends each client its own lobby view). The host plays by the
//!   same rule. Once the vote is decided everyone sees everything.
//! * **Locked** (default on) — the first pick stands; the host ignores re-votes.
//! * **Tally** — most votes wins; ties by a fair seeded roll; non-voters don't count; nobody voted
//!   = a roll among the offers (showing the whole pool: the same map again, as before).

use bd_core::PlayerId;
use bd_sim::settings::MapVoteRules;

use super::proto::{NetMapKind, NextVote, VotePick};

const N: usize = NetMapKind::ALL.len();
/// Seconds the RANDOM roulette spins on the end screen before the rolled map loads.
pub const ROULETTE_SECS: f64 = 2.4;

/// splitmix64: any seed spreads evenly.
pub fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Maps the pool allows, in menu order (every map if the pool leaves them all out).
pub fn pool(rules: &MapVoteRules) -> Vec<NetMapKind> {
    let v: Vec<NetMapKind> = NetMapKind::ALL.into_iter().filter(|k| rules.pool_off & (1 << k.index()) == 0).collect();
    if v.is_empty() { NetMapKind::ALL.to_vec() } else { v }
}

/// The host's session history, for fair offers.
#[derive(Clone, Debug, Default)]
pub struct Rotation {
    /// Maps played, newest first (the last two).
    pub recent: Vec<NetMapKind>,
    /// Times each map has been offered this session.
    pub offered: [u32; N],
    /// Votes opened this session (mixed into the tie-break seed).
    pub round: u64,
    pub seed: u64,
}

impl Rotation {
    pub fn new(seed: u64) -> Self {
        Self { seed, ..Default::default() }
    }

    /// A match on `k` started.
    pub fn played(&mut self, k: NetMapKind) {
        self.recent.retain(|r| *r != k);
        self.recent.insert(0, k);
        self.recent.truncate(2);
    }

    /// This vote's offers (counted towards the rotation).
    pub fn draw(&mut self, rules: &MapVoteRules) -> Vec<NetMapKind> {
        let offers = offer_maps(&pool(rules), &self.recent, &self.offered, rules.choices as usize, self.seed ^ mix(self.round));
        self.round += 1;
        if rules.choices != 0 {
            for k in &offers {
                self.offered[k.index()] += 1;
            }
        }
        offers
    }
}

/// `n` maps from `pool` (0 = all of it), in menu order: the recent maps (`recent`, newest first)
/// are left out while enough others remain (the older one comes back first), then the
/// least-offered win, ties by `seed`.
pub fn offer_maps(pool: &[NetMapKind], recent: &[NetMapKind], offered: &[u32; N], n: usize, seed: u64) -> Vec<NetMapKind> {
    if n == 0 {
        return pool.to_vec();
    }
    let n = n.min(pool.len());
    let mut skip: Vec<NetMapKind> = recent.iter().copied().filter(|k| pool.contains(k)).collect();
    while !skip.is_empty() && pool.iter().filter(|k| !skip.contains(k)).count() < n {
        skip.pop();
    }
    let mut cand: Vec<NetMapKind> = pool.iter().copied().filter(|k| !skip.contains(k)).collect();
    cand.sort_by_key(|k| (offered[k.index()], mix(seed ^ (k.index() as u64 + 1).wrapping_mul(0xA24B_AED4_963E_E407))));
    cand.truncate(n);
    cand.sort_by_key(|k| k.index());
    cand
}

/// What RANDOM rolls: a pool map not on offer and not the map just played; if none, one not on
/// offer; then any but the last; then anything in the pool.
pub fn resolve_random(pool: &[NetMapKind], offers: &[NetMapKind], last: NetMapKind, seed: u64) -> NetMapKind {
    let tiers: [&dyn Fn(&NetMapKind) -> bool; 4] =
        [&|k| !offers.contains(k) && *k != last, &|k| !offers.contains(k), &|k| *k != last, &|_| true];
    for keep in tiers {
        let c: Vec<NetMapKind> = pool.iter().copied().filter(|k| keep(k)).collect();
        if !c.is_empty() {
            return c[(mix(seed ^ 0x5EED_0F_4A4D) % c.len() as u64) as usize];
        }
    }
    last
}

/// The top pick (None = nobody voted); ties by a fair roll over `seed`.
pub fn tally_picks(votes: &[(PlayerId, VotePick)], seed: u64) -> Option<VotePick> {
    let slot = |p: VotePick| match p {
        VotePick::Map(k) => k.index(),
        VotePick::Random => N,
    };
    let mut count = [0u32; N + 1];
    for (_, p) in votes {
        count[slot(*p)] += 1;
    }
    let best = count.iter().copied().max().unwrap_or(0);
    if best == 0 {
        return None;
    }
    let tied: Vec<VotePick> = NetMapKind::ALL.into_iter().map(VotePick::Map).chain([VotePick::Random]).filter(|p| count[slot(*p)] == best).collect();
    Some(tied[(mix(seed) % tied.len() as u64) as usize])
}

/// The vote for the next match (open while the match is over).
#[derive(Clone, Debug)]
pub struct VoteState {
    /// When the next match starts (`now_secs`).
    pub deadline: f64,
    /// One pick per player.
    pub votes: Vec<(PlayerId, VotePick)>,
    /// The maps on the cards.
    pub offers: Vec<NetMapKind>,
    /// A RANDOM card.
    pub random: bool,
    pub hidden: bool,
    pub locked: bool,
    /// Every map is on offer (the old vote): nobody voting keeps the same map.
    pub all: bool,
    /// Maps RANDOM may roll (the pool when the vote opened).
    pub pool: Vec<NetMapKind>,
    /// The map just played.
    pub last: NetMapKind,
    /// Decided: (map, rolled by RANDOM). New votes are ignored from then on.
    pub result: Option<(NetMapKind, bool)>,
}

impl VoteState {
    /// Open a vote after a match on `last` under `rules` (offers drawn from `rot`).
    pub fn open(rules: &MapVoteRules, rot: &mut Rotation, last: NetMapKind, deadline: f64) -> Self {
        let offers = rot.draw(rules);
        let pool = pool(rules);
        // RANDOM only when it can land somewhere the cards don't already show.
        let random = rules.random && rules.choices != 0 && pool.iter().any(|k| !offers.contains(k));
        Self {
            deadline,
            votes: Vec::new(),
            offers,
            random,
            hidden: rules.hidden,
            locked: rules.locked,
            all: rules.choices == 0,
            pool,
            last,
            result: None,
        }
    }

    /// Would `player`'s `pick` count? (on offer, the vote undecided, not a locked re-vote)
    pub fn accepts(&self, player: PlayerId, pick: VotePick) -> bool {
        let on_offer = match pick {
            VotePick::Map(k) => self.offers.contains(&k),
            VotePick::Random => self.random,
        };
        on_offer && self.result.is_none() && !(self.locked && self.votes.iter().any(|(p, _)| *p == player))
    }

    /// Record `player`'s pick (the latest counts unless locked). Returns whether it changed anything.
    pub fn cast(&mut self, player: PlayerId, pick: VotePick) -> bool {
        if !self.accepts(player, pick) || self.votes.contains(&(player, pick)) {
            return false;
        }
        self.votes.retain(|(p, _)| *p != player);
        self.votes.push((player, pick));
        true
    }

    /// The next map for these votes: (map, rolled by RANDOM). Pure (the same `seed` = the same map).
    pub fn decide(&self, seed: u64) -> (NetMapKind, bool) {
        if let Some(r) = self.result {
            return r;
        }
        match tally_picks(&self.votes, seed) {
            Some(VotePick::Map(k)) => (k, false),
            Some(VotePick::Random) => (resolve_random(&self.pool, &self.offers, self.last, seed), true),
            None if self.all || self.offers.is_empty() => (self.last, false),
            None => (self.offers[(mix(seed ^ 0x0FFE_2) % self.offers.len() as u64) as usize], false),
        }
    }

    /// Time's up: decide once. Returns the map to build now, or None while RANDOM's roulette
    /// plays (the deadline moves [`ROULETTE_SECS`] on; call again when it's due).
    pub fn settle(&mut self, seed: u64, now: f64) -> Option<NetMapKind> {
        if let Some((k, _)) = self.result {
            return Some(k);
        }
        let (k, rolled) = self.decide(seed);
        self.result = Some((k, rolled));
        if rolled {
            self.deadline = now + ROULETTE_SECS;
            None
        } else {
            Some(k)
        }
    }

    /// What `viewer` may see: every pick once they voted (or hidden votes are off, or it's decided),
    /// else only who has voted.
    pub fn view_for(&self, viewer: PlayerId, now: f64) -> NextVote {
        let hidden = self.hidden && self.result.is_none() && !self.votes.iter().any(|(p, _)| *p == viewer);
        NextVote {
            votes: if hidden { Vec::new() } else { self.votes.clone() },
            voted: self.votes.iter().map(|(p, _)| *p).collect(),
            offers: self.offers.clone(),
            random: self.random,
            hidden,
            locked: self.locked,
            result: self.result.map(|(k, _)| k),
            rolled: self.result.is_some_and(|(_, r)| r),
            secs_left: (self.deadline - now).max(0.0) as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::proto::{C2S, LobbyInfo};

    /// The distinct maps (menu order).
    fn kinds(it: impl IntoIterator<Item = NetMapKind>) -> Vec<NetMapKind> {
        let mut v: Vec<NetMapKind> = it.into_iter().collect();
        v.sort_by_key(|k| k.index());
        v.dedup();
        v
    }

    fn rules(choices: u8) -> MapVoteRules {
        MapVoteRules { choices, ..Default::default() }
    }

    #[test]
    fn defaults_are_two_choices_random_hidden_locked() {
        let r = MapVoteRules::default();
        assert_eq!((r.choices, r.random, r.hidden, r.locked, r.pool_off), (2, true, true, true, 0));
        assert_eq!(bd_sim::settings::GameSettings::default().vote, r);
    }

    /// Offers: the asked count, never the map just played, deterministic, and fair over time.
    #[test]
    fn offered_set_count_excludes_last_and_is_deterministic() {
        for choices in [1u8, 2, 3, 4] {
            for last in NetMapKind::ALL {
                let mut a = Rotation::new(77);
                a.played(last);
                let mut b = a.clone();
                let oa = a.draw(&rules(choices));
                assert_eq!(oa.len(), choices as usize);
                assert!(!oa.contains(&last), "{choices} choices after {last:?}: {oa:?}");
                assert_eq!(oa, b.draw(&rules(choices)), "deterministic");
                assert_eq!(kinds(oa.clone()).len(), oa.len(), "no duplicates");
            }
        }
        // The one before the last is left out too while the pool allows.
        let mut r = Rotation::new(1);
        r.played(NetMapKind::Casino);
        r.played(NetMapKind::Lair);
        for _ in 0..20 {
            let o = r.draw(&rules(4));
            assert!(!o.contains(&NetMapKind::Lair) && !o.contains(&NetMapKind::Casino), "{o:?}");
        }
        // ALL = the whole pool (the old vote), map just played included.
        let mut r = Rotation::new(1);
        r.played(NetMapKind::Casino);
        assert_eq!(r.draw(&rules(0)), NetMapKind::ALL.to_vec());
        // Fair rotation: playing one of the cards each time, every map is offered within 8 votes
        // of 2, and offer counts stay close.
        let mut r = Rotation::new(9);
        r.played(NetMapKind::Archipelago);
        let mut seen = [false; N];
        for _ in 0..8 {
            let o = r.draw(&rules(2));
            for k in &o {
                seen[k.index()] = true;
            }
            r.played(o[0]);
        }
        assert!(seen.iter().all(|s| *s), "every map offered within 8 votes: {seen:?}");
        let (lo, hi) = (r.offered.iter().min().unwrap(), r.offered.iter().max().unwrap());
        assert!(hi - lo <= 2, "fair: {:?}", r.offered);
    }

    #[test]
    fn pool_limits_offers_and_small_pools_fall_back() {
        // Only three maps in the pool; the last one played can come back when needed.
        let keep = [NetMapKind::Miami, NetMapKind::Casino, NetMapKind::Lair];
        let pool_off = NetMapKind::ALL.iter().filter(|k| !keep.contains(k)).fold(0u32, |m, k| m | 1 << k.index());
        let r = MapVoteRules { choices: 2, pool_off, ..Default::default() };
        assert_eq!(pool(&r), keep.to_vec());
        let mut rot = Rotation::new(3);
        rot.played(NetMapKind::Casino);
        let o = rot.draw(&r);
        assert_eq!(o, vec![NetMapKind::Miami, NetMapKind::Lair], "the two that weren't just played");
        let r4 = MapVoteRules { choices: 4, ..r.clone() };
        assert_eq!(rot.draw(&r4).len(), 3, "never more than the pool");
        // An empty pool means every map.
        assert_eq!(pool(&MapVoteRules { pool_off: u32::MAX, ..Default::default() }).len(), N);
    }

    #[test]
    fn random_resolves_off_the_cards() {
        let p = NetMapKind::ALL.to_vec();
        let offers = [NetMapKind::Miami, NetMapKind::Ember];
        for s in 0..200 {
            let k = resolve_random(&p, &offers, NetMapKind::Casino, s);
            assert!(!offers.contains(&k) && k != NetMapKind::Casino, "{k:?}");
            assert_eq!(k, resolve_random(&p, &offers, NetMapKind::Casino, s), "deterministic");
        }
        // Spreads over the candidates.
        let hits = kinds((0..400).map(|s| resolve_random(&p, &offers, NetMapKind::Casino, s)));
        assert_eq!(hits.len(), N - 3);
        // Tiny pool: everything is on offer → any map (but the last, if possible).
        let small = [NetMapKind::Miami, NetMapKind::Ember];
        assert_eq!(resolve_random(&small, &small, NetMapKind::Miami, 5), NetMapKind::Ember);
        assert_eq!(resolve_random(&[NetMapKind::Miami], &[NetMapKind::Miami], NetMapKind::Miami, 5), NetMapKind::Miami);
    }

    fn open_vote(r: &MapVoteRules) -> VoteState {
        let mut rot = Rotation::new(42);
        rot.played(NetMapKind::Archipelago);
        VoteState::open(r, &mut rot, NetMapKind::Archipelago, 100.0)
    }

    #[test]
    fn random_wins_rolls_once_with_a_roulette() {
        let mut v = open_vote(&MapVoteRules::default());
        assert!(v.random && v.offers.len() == 2);
        let (a, b) = (PlayerId(0), PlayerId(1));
        assert!(v.cast(a, VotePick::Random) && v.cast(b, VotePick::Random));
        let (k, rolled) = v.decide(7);
        assert!(rolled && !v.offers.contains(&k) && k != NetMapKind::Archipelago);
        assert_eq!(v.settle(7, 50.0), None, "the roulette plays first");
        assert_eq!(v.deadline, 50.0 + ROULETTE_SECS);
        assert_eq!(v.settle(999, 60.0), Some(k), "the same roll after the roulette");
        assert!(!v.cast(PlayerId(2), VotePick::Map(v.offers[0])), "decided: no more votes");
        let view = v.view_for(PlayerId(5), 50.0);
        assert_eq!((view.result, view.rolled, view.hidden), (Some(k), true, false), "revealed to everyone");
    }

    #[test]
    fn tally_ties_are_fair_and_non_voters_dont_count() {
        let mut v = open_vote(&rules(3));
        let [x, y, _] = [v.offers[0], v.offers[1], v.offers[2]];
        v.cast(PlayerId(0), VotePick::Map(x));
        v.cast(PlayerId(1), VotePick::Map(y));
        let picks = kinds((0..64).map(|s| v.decide(s).0));
        assert_eq!(picks, kinds([x, y]), "a fair roll between the tied maps only");
        v.cast(PlayerId(2), VotePick::Map(y));
        assert_eq!(v.decide(3), (y, false));
        // Nobody voted: a roll among the cards (never the map just played).
        let v = open_vote(&rules(2));
        for s in 0..32 {
            assert!(v.offers.contains(&v.decide(s).0));
        }
        // Whole pool on offer (the old vote): nobody voted = the same map again.
        let v = open_vote(&MapVoteRules::open_all());
        assert_eq!(v.decide(5), (NetMapKind::Archipelago, false));
    }

    #[test]
    fn only_offered_picks_count() {
        let mut v = open_vote(&MapVoteRules { locked: false, ..Default::default() });
        let off = NetMapKind::ALL.into_iter().find(|k| !v.offers.contains(k)).unwrap();
        assert!(!v.cast(PlayerId(0), VotePick::Map(off)), "not on a card");
        let mut no_random = open_vote(&MapVoteRules { random: false, ..Default::default() });
        assert!(!no_random.random && !no_random.cast(PlayerId(0), VotePick::Random));
        assert!(v.cast(PlayerId(0), VotePick::Random));
    }

    #[test]
    fn locked_votes_reject_a_second_pick() {
        let mut v = open_vote(&MapVoteRules::default());
        let (a, b) = (v.offers[0], v.offers[1]);
        assert!(v.cast(PlayerId(3), VotePick::Map(a)));
        assert!(!v.cast(PlayerId(3), VotePick::Map(b)), "locked: the first pick stands");
        assert!(!v.cast(PlayerId(3), VotePick::Random));
        assert_eq!(v.votes, vec![(PlayerId(3), VotePick::Map(a))]);
        // Unlocked: swapping is allowed, one vote per player.
        let mut v = open_vote(&MapVoteRules { locked: false, ..Default::default() });
        let (a, b) = (v.offers[0], v.offers[1]);
        assert!(v.cast(PlayerId(3), VotePick::Map(a)) && v.cast(PlayerId(3), VotePick::Map(b)));
        assert_eq!(v.votes, vec![(PlayerId(3), VotePick::Map(b))]);
    }

    #[test]
    fn hidden_until_you_vote_per_viewer() {
        let mut v = open_vote(&MapVoteRules::default());
        let (me, other, late) = (PlayerId(0), PlayerId(1), PlayerId(2));
        v.cast(other, VotePick::Map(v.offers[1]));
        let mine = v.view_for(me, 100.0);
        assert!(mine.hidden && mine.votes.is_empty(), "not voted: no picks shown");
        assert_eq!(mine.voted, vec![other], "…but who has voted");
        let theirs = v.view_for(other, 100.0);
        assert!(!theirs.hidden && theirs.votes.len() == 1, "voted: tallies shown");
        v.cast(me, VotePick::Random);
        let mine = v.view_for(me, 100.0);
        assert!(!mine.hidden && mine.votes.len() == 2);
        let l = v.view_for(late, 100.0);
        assert!(l.hidden && l.votes.is_empty() && l.voted.len() == 2);
        // Hidden votes off: everyone sees everything.
        let mut v = open_vote(&MapVoteRules { hidden: false, ..Default::default() });
        v.cast(other, VotePick::Map(v.offers[0]));
        let mine = v.view_for(me, 100.0);
        assert!(!mine.hidden && mine.votes.len() == 1);
    }

    /// Settings: RON persistence (old files without the section get the defaults) and validation.
    #[test]
    fn vote_rules_persist_and_validate() {
        let mut g = bd_sim::settings::GameSettings::default();
        g.vote = MapVoteRules { choices: 3, random: false, hidden: false, locked: false, pool_off: 0b101 };
        let back = bd_sim::settings::GameSettings::from_ron(&g.to_ron().unwrap()).unwrap();
        assert_eq!(back.vote, g.vote);
        let old = bd_sim::settings::GameSettings::from_ron("(mode: Ffa)").unwrap();
        assert_eq!(old.vote, MapVoteRules::default());
        let wild = MapVoteRules { choices: 200, ..Default::default() }.validated();
        assert_eq!(wild.choices, bd_sim::settings::MAP_CHOICES_MAX);
        assert_eq!(MapVoteRules { choices: 0, ..Default::default() }.validated().choices, 0, "0 = ALL");
        // The lobby prefs (the host's last lobby settings) carry them too.
        let mut p = crate::lobby::prefs::UiPrefs::default();
        p.game.vote.choices = 4;
        let s = ron::ser::to_string(&p).unwrap();
        let q: crate::lobby::prefs::UiPrefs = ron::from_str(&s).unwrap();
        assert_eq!(q.game.vote.choices, 4);
    }

    /// Over real sockets: each client gets its own view (hidden until it voted), a locked
    /// re-vote is ignored by the host, and the host's own view follows the same rule.
    #[test]
    fn per_client_views_and_locked_revotes_over_the_wire() {
        use crate::net::client::ClientCore;
        use crate::net::host::{HostCore, HostStart};
        use crate::net::proto::NetProfile;
        use bd_core::corps::Corps;
        use bd_net::now_secs;
        let mut host = HostCore::start(HostStart { port: 0, tunnel: false, lan: false, ..Default::default() }).expect("host");
        let addr = format!("127.0.0.1:{}", host.port);
        let mut a = ClientCore::connect(&addr, "alpha", Corps::Rage, NetProfile::default()).expect("a");
        let mut b = ClientCore::connect(&addr, "bravo", Corps::Rage, NetProfile::default()).expect("b");
        let mut cmds = Vec::new();
        let mut pump = |host: &mut HostCore, a: &mut ClientCore, b: &mut ClientCore, f: &dyn Fn(&HostCore, &ClientCore, &ClientCore) -> bool| {
            let t = now_secs();
            while now_secs() - t < 8.0 {
                host.poll(None, &mut cmds);
                a.update(None);
                b.update(None);
                if f(host, a, b) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            false
        };
        assert!(pump(&mut host, &mut a, &mut b, &|_, a, b| a.me.is_some() && b.me.is_some()), "joined");
        let deadline = now_secs() + 60.0;
        host.vote = Some(VoteState::open(&host.settings.vote, &mut host.rotation, NetMapKind::Archipelago, deadline));
        assert!(pump(&mut host, &mut a, &mut b, &|_, a, b| a.lobby.vote.is_some() && b.lobby.vote.is_some()), "vote reached both");
        let offers = a.lobby.vote.as_ref().unwrap().offers.clone();
        assert_eq!(offers.len(), 2, "default: two maps");
        assert!(!offers.contains(&NetMapKind::Archipelago), "not the map just played");
        a.vote(VotePick::Map(offers[0]));
        assert!(pump(&mut host, &mut a, &mut b, &|h, _, _| !h.vote.as_ref().unwrap().votes.is_empty()), "a's vote counted");
        // Locked: a's second pick is ignored.
        a.vote(VotePick::Map(offers[1]));
        let a_id = a.me.unwrap();
        assert!(pump(&mut host, &mut a, &mut b, &|_, a, b| {
            let (va, vb) = (a.lobby.vote.as_ref().unwrap(), b.lobby.vote.as_ref().unwrap());
            va.votes.len() == 1 && vb.voted.len() == 1
        }));
        assert_eq!(host.vote.as_ref().unwrap().votes, vec![(a_id, VotePick::Map(offers[0]))], "the first pick stands");
        let (va, vb) = (a.lobby.vote.clone().unwrap(), b.lobby.vote.clone().unwrap());
        assert!(!va.hidden && va.votes == vec![(a_id, VotePick::Map(offers[0]))], "a voted: sees the picks");
        assert!(vb.hidden && vb.votes.is_empty() && vb.voted == vec![a_id], "b hasn't: sees who, not what");
        let hv = host.lobby().vote.unwrap();
        assert!(hv.hidden && hv.votes.is_empty(), "the host hasn't voted either");
        // b votes RANDOM → b sees everything too.
        b.vote(VotePick::Random);
        assert!(pump(&mut host, &mut a, &mut b, &|_, _, b| b.lobby.vote.as_ref().is_some_and(|v| !v.hidden && v.votes.len() == 2)), "b sees the picks after voting");
    }

    /// Protocol: the pick, the per-viewer vote view and the settings survive the wire.
    #[test]
    fn protocol_round_trip() {
        for pick in [VotePick::Random, VotePick::Map(NetMapKind::Hyperline)] {
            let back: C2S = bd_net::codec::decode(&bd_net::codec::encode(&C2S::MapVote(pick))).unwrap();
            assert!(matches!(back, C2S::MapVote(p) if p == pick));
        }
        let mut v = open_vote(&MapVoteRules::default());
        v.cast(PlayerId(1), VotePick::Map(v.offers[0]));
        let l = LobbyInfo {
            vote: Some(v.view_for(PlayerId(0), 100.0)),
            settings: bd_sim::settings::GameSettings { vote: MapVoteRules { choices: 4, ..Default::default() }, ..Default::default() },
            ..Default::default()
        };
        let back: LobbyInfo = bd_net::codec::decode(&bd_net::codec::encode(&l)).unwrap();
        assert_eq!(back, l);
    }
}
