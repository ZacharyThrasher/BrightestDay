//! Per-match balance telemetry (COMBAT.md §5i.6). **Local and hidden** — never shown to players
//! (mystery tenet); written to `logs/matches/<timestamp>.json` by the client or tools.
//!
//! ```text
//! let mut tel = Telemetry::new(seed);
//! loop {
//!     world.tick(&inputs);
//!     tel.observe(&world);             // reads world.events() + samples player speed
//! }
//! let report = tel.finish(&world);     // MatchReport (merge many for tournaments)
//! report.write_to_dir("logs/matches")?;
//! ```
//!
//! What it records, keyed by *source* (who / what dealt the damage):
//! * damage dealt / taken, hits and KOs by **signature** (form + sorted mechanics + element), by
//!   mechanic, form, element, gun archetype, held form, creature attack and non-construct source
//!   (ring blast, shove, wall splat, …);
//! * willpower spent per signature (cast + graft cost, and upkeep × lifetime), construct lifetimes
//!   and how they ended;
//! * KO causes, damage taken since the victim's last spawn at the KO (the Smash model's KO %),
//!   fight time (first hit after spawning → KO), launches / recoveries, KO streak lengths, a player
//!   speed histogram, and each player's most-cast mechanics vs. who won.
//!
//! Cost: `observe` walks this tick's events (a handful) and the players; buckets are fixed arrays
//! or maps keyed by a `u64` signature, so steady-state recording does not allocate except when a
//! new signature / construct / KO sample appears (amortised). Deterministic (reads the sim only).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::{ConstructId, DT, PlayerId, Tick};

use crate::combat::HitKind;
use crate::events::GameEvent;
use crate::fuzz::{ELEMENTS, FORMS, MECHANICS};
use crate::rules::MatchPhase;
use crate::weapons::archetype_for;
use crate::world::World;

/// Number of gun archetypes (`GunArchetype as u8` 0..=9).
pub const ARCHETYPES: usize = 10;
pub const ARCHETYPE_NAMES: [&str; ARCHETYPES] = ["Ring", "HandCannon", "SuperShotgun", "Minigun", "Rocket", "Railgun", "Bow", "Cannon", "Staff", "Disc"];
/// Creature attack kinds (`constructs::ai::AttackKind as u8`).
pub const ATTACKS: usize = 11;
pub const ATTACK_NAMES: [&str; ATTACKS] = ["Bite", "Swipe", "Charge", "Stomp", "TailSweep", "Breath", "Lash", "Swoop", "Grab", "Constrict", "Spit"];
/// Damage that no construct delivered.
pub const SOURCES: usize = 7;
pub const SOURCE_NAMES: [&str; SOURCES] = ["RingBlast", "RingShove", "WallSplat", "Chain", "Explosion", "Crush", "Other"];
/// Player speed histogram: 2 m/s bins, the last one open-ended.
pub const SPEED_BINS: usize = 40;
pub const SPEED_BIN_WIDTH: f32 = 2.0;
/// A KO is credited to the last hit within this many seconds (the sim's KO credit window).
pub const KO_ATTRIB_SECS: f32 = 6.0;

fn form_ix(f: Form) -> usize {
    FORMS.iter().position(|x| *x == f).unwrap_or(0)
}
fn mech_ix(m: Mechanic) -> usize {
    MECHANICS.iter().position(|x| *x == m).unwrap_or(0)
}
fn elem_ix(e: Element) -> usize {
    ELEMENTS.iter().position(|x| *x == e).unwrap_or(0)
}

// ------------------------------------------------------------------------------------ signature

/// Form + sorted mechanic set + element, packed: form index (6 bits) | element (4 bits) | mechanic
/// bitmask over [`MECHANICS`] (40 bits). Word-independent: "sword" and "katana" with the same
/// recipe share a signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Signature(pub u64);

impl Signature {
    pub fn of(r: &Recipe) -> Self {
        let mut mask = 0u64;
        for m in &r.mechanics {
            mask |= 1 << mech_ix(*m);
        }
        Self(form_ix(r.form) as u64 | (elem_ix(r.element) as u64) << 6 | mask << 10)
    }
    pub fn form(self) -> Form {
        FORMS[(self.0 & 63) as usize % FORMS.len()]
    }
    pub fn element(self) -> Element {
        ELEMENTS[((self.0 >> 6) & 15) as usize % ELEMENTS.len()]
    }
    pub fn mech_mask(self) -> u64 {
        self.0 >> 10
    }
    /// Mechanic indices (into [`MECHANICS`]) in the set, ascending.
    pub fn mechs(self) -> impl Iterator<Item = usize> {
        let m = self.mech_mask();
        (0..MECHANICS.len()).filter(move |i| m & (1 << i) != 0)
    }
    /// `"Hammer+Blunt+Returning/Electric"`.
    pub fn describe(self) -> String {
        let mut s = format!("{:?}", self.form());
        for i in self.mechs() {
            let _ = write!(s, "+{:?}", MECHANICS[i]);
        }
        let _ = write!(s, "/{:?}", self.element());
        s
    }
}

// --------------------------------------------------------------------------------------- bucket

/// Everything accumulated for one source key.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bucket {
    /// Hits on *other* players.
    pub hits: u32,
    pub damage: f64,
    /// KOs where this was the last hit (within [`KO_ATTRIB_SECS`]).
    pub kos: u32,
    /// Casts (+ grafts) that produced this.
    pub casts: u32,
    /// Willpower paid to cast / graft.
    pub wp_cast: f64,
    /// Willpower drained by upkeep (upkeep/s × lifetime).
    pub wp_upkeep: f64,
    /// Construct lifetimes (s) and how many ended.
    pub life_secs: f64,
    pub lives: u32,
    /// Of those, shattered (integrity 0) rather than expired / dismissed / replaced.
    pub shattered: u32,
}

impl Bucket {
    pub fn merge(&mut self, o: &Bucket) {
        self.hits += o.hits;
        self.damage += o.damage;
        self.kos += o.kos;
        self.casts += o.casts;
        self.wp_cast += o.wp_cast;
        self.wp_upkeep += o.wp_upkeep;
        self.life_secs += o.life_secs;
        self.lives += o.lives;
        self.shattered += o.shattered;
    }
    pub fn willpower(&self) -> f64 {
        self.wp_cast + self.wp_upkeep
    }
    /// Damage per willpower (None if nothing was spent).
    pub fn dpw(&self) -> Option<f64> {
        let w = self.willpower();
        (w > 1e-6).then(|| self.damage / w)
    }
    fn json(&self, s: &mut String) {
        let _ = write!(
            s,
            "{{\"hits\":{},\"damage\":{:.2},\"kos\":{},\"casts\":{},\"wp_cast\":{:.2},\"wp_upkeep\":{:.2},\"life_secs\":{:.2},\"lives\":{},\"shattered\":{}}}",
            self.hits, self.damage, self.kos, self.casts, self.wp_cast, self.wp_upkeep, self.life_secs, self.lives, self.shattered
        );
    }
}

/// Who a hit is attributed to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Attrib {
    sig: Option<Signature>,
    arch: Option<u8>,
    held: Option<u8>,
    attack: Option<u8>,
    source: Option<u8>,
}

// ------------------------------------------------------------------------------------- per player

/// Per-player summary in a report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerReport {
    pub id: u8,
    pub kos: u32,
    pub deaths: u32,
    pub damage_dealt: f64,
    pub damage_taken: f64,
    pub casts: u32,
    pub willpower_spent: f64,
    pub max_streak: u16,
    pub launches: u32,
    pub recoveries: u32,
    /// Mechanic indices this player cast most (up to 3).
    pub top_mechanics: Vec<u8>,
    pub winner: bool,
}

#[derive(Clone, Copy, Debug)]
struct Track {
    present: bool,
    kos: u32,
    deaths: u32,
    dealt: f64,
    taken: f64,
    casts: u32,
    wp: f64,
    streak: u16,
    max_streak: u16,
    launches: u32,
    recoveries: u32,
    /// Damage (%) taken since the last spawn.
    since_spawn: f32,
    first_hit: Option<Tick>,
    last_hit: Option<(Attrib, Tick)>,
    launched: bool,
    mech_casts: [u16; MECHANICS.len()],
}

impl Default for Track {
    fn default() -> Self {
        Self {
            present: false,
            kos: 0,
            deaths: 0,
            dealt: 0.0,
            taken: 0.0,
            casts: 0,
            wp: 0.0,
            streak: 0,
            max_streak: 0,
            launches: 0,
            recoveries: 0,
            since_spawn: 0.0,
            first_hit: None,
            last_hit: None,
            launched: false,
            mech_casts: [0; MECHANICS.len()],
        }
    }
}

/// A live construct's attribution.
#[derive(Clone, Copy, Debug)]
struct Live {
    sig: Signature,
    owner: PlayerId,
    born: Tick,
    upkeep: f32,
    arch: Option<u8>,
    held: Option<u8>,
    /// Last creature attack cue (`AttackKind as u8`).
    attack: Option<u8>,
    /// Spawned for free by another construct / effect (credited to its root).
    child: bool,
}

// --------------------------------------------------------------------------------------- report

/// One match (or, merged, a whole tournament).
#[derive(Clone, Debug, PartialEq)]
pub struct MatchReport {
    pub matches: u32,
    pub seed: u64,
    /// Live-phase seconds observed.
    pub secs: f64,
    pub by_signature: HashMap<Signature, Bucket>,
    pub by_mechanic: [Bucket; MECHANICS.len()],
    pub by_form: [Bucket; FORMS.len()],
    pub by_element: [Bucket; ELEMENTS.len()],
    pub by_archetype: [Bucket; ARCHETYPES],
    /// Non-gun held items by form.
    pub by_held: [Bucket; FORMS.len()],
    pub by_attack: [Bucket; ATTACKS],
    pub by_source: [Bucket; SOURCES],
    /// KO cause (`"Void"`, `"Damage"`, `"BlastZone/Top"`, …) → count.
    pub ko_causes: BTreeMap<String, u32>,
    /// Damage (%) the victim had taken since spawning, at each KO.
    pub ko_pct: Vec<f32>,
    /// Credited KOs only (someone else got the KO): the victim's % at the KO…
    pub ko_pct_credited: Vec<f32>,
    /// …and seconds from their first hit after spawning to the KO (the "fight" length).
    pub fight_secs: Vec<f32>,
    /// KOs credited to another player (the rest are self-destructs: walked off, own blast).
    pub ko_credited: u32,
    /// Uncredited KOs split: the victim was never hit since spawning (walked / fell off alone)…
    pub ko_self_never_hit: u32,
    /// …or was hit, but the last hit is older than the KO credit window (credit expired).
    pub ko_self_stale: u32,
    /// Seconds since the victim's last hit, for uncredited KOs that had one.
    pub ko_self_stale_secs: Vec<f32>,
    /// Tech (splat cancelled) and punch-through moments.
    pub techs: u32,
    pub punches: u32,
    /// Streak lengths (KOs in a row before dying / the match ending), one sample per ended streak.
    pub streaks: Vec<u16>,
    pub shutdowns: u32,
    pub launches: u32,
    pub recoveries: u32,
    pub speed_hist: [u64; SPEED_BINS],
    /// Per mechanic: (players whose top-3 mechanics included it, of those the winners).
    pub mech_wins: [(u32, u32); MECHANICS.len()],
    pub players: Vec<PlayerReport>,
    pub winner: Option<u8>,
}

impl Default for MatchReport {
    fn default() -> Self {
        Self {
            matches: 0,
            seed: 0,
            secs: 0.0,
            by_signature: HashMap::new(),
            by_mechanic: [Bucket::default(); MECHANICS.len()],
            by_form: [Bucket::default(); FORMS.len()],
            by_element: [Bucket::default(); ELEMENTS.len()],
            by_archetype: [Bucket::default(); ARCHETYPES],
            by_held: [Bucket::default(); FORMS.len()],
            by_attack: [Bucket::default(); ATTACKS],
            by_source: [Bucket::default(); SOURCES],
            ko_causes: BTreeMap::new(),
            ko_pct: Vec::new(),
            ko_pct_credited: Vec::new(),
            fight_secs: Vec::new(),
            ko_credited: 0,
            ko_self_never_hit: 0,
            ko_self_stale: 0,
            ko_self_stale_secs: Vec::new(),
            techs: 0,
            punches: 0,
            streaks: Vec::new(),
            shutdowns: 0,
            launches: 0,
            recoveries: 0,
            speed_hist: [0; SPEED_BINS],
            mech_wins: [(0, 0); MECHANICS.len()],
            players: Vec::new(),
            winner: None,
        }
    }
}

fn merge_arr<const N: usize>(a: &mut [Bucket; N], b: &[Bucket; N]) {
    for (x, y) in a.iter_mut().zip(b.iter()) {
        x.merge(y);
    }
}

impl MatchReport {
    /// Fold another report in (tournament aggregation). Per-player rows are dropped (they only
    /// mean something within one match).
    pub fn merge(&mut self, o: &MatchReport) {
        self.matches += o.matches;
        self.secs += o.secs;
        for (k, b) in &o.by_signature {
            self.by_signature.entry(*k).or_default().merge(b);
        }
        merge_arr(&mut self.by_mechanic, &o.by_mechanic);
        merge_arr(&mut self.by_form, &o.by_form);
        merge_arr(&mut self.by_element, &o.by_element);
        merge_arr(&mut self.by_archetype, &o.by_archetype);
        merge_arr(&mut self.by_held, &o.by_held);
        merge_arr(&mut self.by_attack, &o.by_attack);
        merge_arr(&mut self.by_source, &o.by_source);
        for (k, n) in &o.ko_causes {
            *self.ko_causes.entry(k.clone()).or_default() += n;
        }
        self.ko_pct.extend_from_slice(&o.ko_pct);
        self.fight_secs.extend_from_slice(&o.fight_secs);
        self.ko_pct_credited.extend_from_slice(&o.ko_pct_credited);
        self.ko_credited += o.ko_credited;
        self.ko_self_never_hit += o.ko_self_never_hit;
        self.ko_self_stale += o.ko_self_stale;
        self.ko_self_stale_secs.extend_from_slice(&o.ko_self_stale_secs);
        self.techs += o.techs;
        self.punches += o.punches;
        self.streaks.extend_from_slice(&o.streaks);
        self.shutdowns += o.shutdowns;
        self.launches += o.launches;
        self.recoveries += o.recoveries;
        for (a, b) in self.speed_hist.iter_mut().zip(o.speed_hist.iter()) {
            *a += b;
        }
        for (a, b) in self.mech_wins.iter_mut().zip(o.mech_wins.iter()) {
            a.0 += b.0;
            a.1 += b.1;
        }
    }

    pub fn total_kos(&self) -> u32 {
        self.ko_causes.values().sum()
    }

    /// JSON (hand-written; no serde_json dependency). Deterministic key order.
    pub fn to_json(&self) -> String {
        let mut s = String::with_capacity(16 * 1024);
        let _ = write!(s, "{{\"matches\":{},\"seed\":{},\"secs\":{:.1},\"winner\":{},", self.matches, self.seed, self.secs, self.winner.map_or("null".to_string(), |w| w.to_string()));
        let arr = |s: &mut String, name: &str, names: &mut dyn Iterator<Item = String>, b: &[Bucket]| {
            let _ = write!(s, "\"{name}\":{{");
            let mut first = true;
            for (n, x) in names.zip(b.iter()) {
                if *x == Bucket::default() {
                    continue;
                }
                if !first {
                    s.push(',');
                }
                first = false;
                let _ = write!(s, "\"{n}\":");
                x.json(s);
            }
            s.push_str("},");
        };
        arr(&mut s, "by_mechanic", &mut MECHANICS.iter().map(|m| format!("{m:?}")), &self.by_mechanic);
        arr(&mut s, "by_form", &mut FORMS.iter().map(|m| format!("{m:?}")), &self.by_form);
        arr(&mut s, "by_element", &mut ELEMENTS.iter().map(|m| format!("{m:?}")), &self.by_element);
        arr(&mut s, "by_archetype", &mut ARCHETYPE_NAMES.iter().map(|m| m.to_string()), &self.by_archetype);
        arr(&mut s, "by_held", &mut FORMS.iter().map(|m| format!("{m:?}")), &self.by_held);
        arr(&mut s, "by_creature_attack", &mut ATTACK_NAMES.iter().map(|m| m.to_string()), &self.by_attack);
        arr(&mut s, "by_source", &mut SOURCE_NAMES.iter().map(|m| m.to_string()), &self.by_source);
        s.push_str("\"by_signature\":{");
        let mut sigs: Vec<(&Signature, &Bucket)> = self.by_signature.iter().collect();
        sigs.sort_by(|a, b| a.0.cmp(b.0));
        for (i, (k, b)) in sigs.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(s, "\"{}\":", k.describe());
            b.json(&mut s);
        }
        s.push_str("},\"ko_causes\":{");
        for (i, (k, n)) in self.ko_causes.iter().enumerate() {
            let _ = write!(s, "{}\"{k}\":{n}", if i > 0 { "," } else { "" });
        }
        let list_f = |s: &mut String, name: &str, v: &[f32]| {
            let _ = write!(s, "\"{name}\":[");
            for (i, x) in v.iter().enumerate() {
                let _ = write!(s, "{}{:.2}", if i > 0 { "," } else { "" }, x);
            }
            s.push_str("],");
        };
        s.push_str("},");
        list_f(&mut s, "ko_pct", &self.ko_pct);
        list_f(&mut s, "fight_secs", &self.fight_secs);
        list_f(&mut s, "ko_pct_credited", &self.ko_pct_credited);
        let _ = write!(
            s,
            "\"ko_credited\":{},\"ko_self_never_hit\":{},\"ko_self_stale\":{},\"techs\":{},\"punch_throughs\":{},",
            self.ko_credited, self.ko_self_never_hit, self.ko_self_stale, self.techs, self.punches
        );
        list_f(&mut s, "ko_self_stale_secs", &self.ko_self_stale_secs);
        s.push_str("\"streaks\":[");
        for (i, x) in self.streaks.iter().enumerate() {
            let _ = write!(s, "{}{x}", if i > 0 { "," } else { "" });
        }
        let _ = write!(s, "],\"shutdowns\":{},\"launches\":{},\"recoveries\":{},\"speed_hist_bin_mps\":{},\"speed_hist\":[", self.shutdowns, self.launches, self.recoveries, SPEED_BIN_WIDTH);
        for (i, x) in self.speed_hist.iter().enumerate() {
            let _ = write!(s, "{}{x}", if i > 0 { "," } else { "" });
        }
        s.push_str("],\"mech_top3_wins\":{");
        let mut first = true;
        for (i, (n, w)) in self.mech_wins.iter().enumerate() {
            if *n == 0 {
                continue;
            }
            let _ = write!(s, "{}\"{:?}\":[{n},{w}]", if first { "" } else { "," }, MECHANICS[i]);
            first = false;
        }
        s.push_str("},\"players\":[");
        for (i, p) in self.players.iter().enumerate() {
            let tops: Vec<String> = p.top_mechanics.iter().map(|m| format!("\"{:?}\"", MECHANICS[*m as usize % MECHANICS.len()])).collect();
            let _ = write!(
                s,
                "{}{{\"id\":{},\"kos\":{},\"deaths\":{},\"damage_dealt\":{:.1},\"damage_taken\":{:.1},\"casts\":{},\"willpower_spent\":{:.1},\"max_streak\":{},\"launches\":{},\"recoveries\":{},\"top_mechanics\":[{}],\"winner\":{}}}",
                if i > 0 { "," } else { "" },
                p.id,
                p.kos,
                p.deaths,
                p.damage_dealt,
                p.damage_taken,
                p.casts,
                p.willpower_spent,
                p.max_streak,
                p.launches,
                p.recoveries,
                tops.join(","),
                p.winner
            );
        }
        s.push_str("]}");
        s
    }

    /// Write `<dir>/<UTC timestamp>-<seed>.json` (creates `dir`). Returns the path.
    pub fn write_to_dir(&self, dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}-{:x}.json", utc_stamp(), self.seed));
        std::fs::write(&path, self.to_json())?;
        Ok(path)
    }
}

/// `YYYYMMDD-HHMMSS` (UTC) from the system clock (file names only; never feeds the sim).
pub fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

// ------------------------------------------------------------------------------------ recorder

/// The recorder. Feed it once per tick with [`Telemetry::observe`].
pub struct Telemetry {
    pub report: MatchReport,
    live: HashMap<ConstructId, Live>,
    players: Vec<Track>,
    last_tick: Tick,
    finished: bool,
}

impl Telemetry {
    pub fn new(seed: u64) -> Self {
        Self {
            report: MatchReport { matches: 1, seed, ..Default::default() },
            live: HashMap::with_capacity(128),
            players: vec![Track::default(); 256],
            last_tick: 0,
            finished: false,
        }
    }

    fn player(&mut self, id: PlayerId) -> &mut Track {
        let t = &mut self.players[id.0 as usize];
        t.present = true;
        t
    }

    /// Credit `f` to every bucket the attribution touches.
    fn credit(&mut self, a: &Attrib, f: impl Fn(&mut Bucket)) {
        let r = &mut self.report;
        if let Some(sig) = a.sig {
            f(r.by_signature.entry(sig).or_default());
            for m in sig.mechs() {
                f(&mut r.by_mechanic[m]);
            }
            f(&mut r.by_form[form_ix(sig.form())]);
            f(&mut r.by_element[elem_ix(sig.element())]);
        }
        if let Some(x) = a.arch {
            f(&mut r.by_archetype[x as usize % ARCHETYPES]);
        }
        if let Some(x) = a.held {
            f(&mut r.by_held[x as usize % FORMS.len()]);
        }
        if let Some(x) = a.attack {
            f(&mut r.by_attack[x as usize % ATTACKS]);
        }
        if let Some(x) = a.source {
            f(&mut r.by_source[x as usize % SOURCES]);
        }
    }

    fn attrib_of(&self, construct: Option<ConstructId>, kind: HitKind) -> Attrib {
        if let Some(l) = construct.and_then(|c| self.live.get(&c)) {
            return Attrib { sig: Some(l.sig), arch: l.arch, held: l.held, attack: if kind == HitKind::Creature { l.attack } else { None }, source: None };
        }
        let src = match kind {
            HitKind::RingBlast => 0,
            HitKind::RingShove => 1,
            HitKind::WallSplat => 2,
            HitKind::Chain => 3,
            HitKind::Explosion => 4,
            HitKind::Crush => 5,
            _ => 6,
        };
        Attrib { source: Some(src), arch: (kind == HitKind::RingBlast).then_some(0), ..Default::default() }
    }

    fn spawn(&mut self, w: &World, id: ConstructId, owner: PlayerId, recipe: &Recipe, cost: f32) {
        // Free children (swarm units, payload rounds, party copies) inherit their root's
        // attribution so their damage lands where the willpower was paid.
        let parent = w.constructs.get(id).and_then(|c| c.parent).filter(|p| *p != id);
        if cost <= 0.0 {
            if let Some(root) = parent.and_then(|p| self.live.get(&p)).copied() {
                self.live.insert(id, Live { born: w.tick, upkeep: 0.0, child: true, ..root });
                return;
            }
        }
        let sig = Signature::of(recipe);
        let arch = archetype_for(recipe).filter(|_| recipe.form.category() == FormCategory::Held).map(|a| a as u8);
        let held = (recipe.form.category() == FormCategory::Held && arch.is_none()).then(|| form_ix(recipe.form) as u8);
        let upkeep = w.tuning().willpower.upkeep_per_sec(recipe, false);
        self.live.insert(id, Live { sig, owner, born: w.tick, upkeep, arch, held, attack: None, child: false });
        let a = Attrib { sig: Some(sig), arch, held, ..Default::default() };
        let c = f64::from(cost.max(0.0));
        self.credit(&a, |b| {
            b.casts += 1;
            b.wp_cast += c;
        });
        let t = self.player(owner);
        t.casts += 1;
        t.wp += c;
        for m in &recipe.mechanics {
            let k = mech_ix(*m);
            t.mech_casts[k] = t.mech_casts[k].saturating_add(1);
        }
    }

    fn end_construct(&mut self, id: ConstructId, now: Tick, shattered: bool) {
        let Some(l) = self.live.remove(&id) else { return };
        if l.child {
            return;
        }
        let secs = now.saturating_sub(l.born) as f64 * f64::from(DT);
        let up = f64::from(l.upkeep) * secs;
        let a = Attrib { sig: Some(l.sig), arch: l.arch, held: l.held, ..Default::default() };
        self.credit(&a, |b| {
            b.wp_upkeep += up;
            b.life_secs += secs;
            b.lives += 1;
            b.shattered += u32::from(shattered);
        });
        self.player(l.owner).wp += up;
    }

    fn end_streak(&mut self, id: PlayerId) {
        let t = &mut self.players[id.0 as usize];
        if t.streak > 0 {
            let s = t.streak;
            t.streak = 0;
            self.report.streaks.push(s);
        }
    }

    /// Record this tick: call right after `world.tick(..)` (reads `world.events()`).
    pub fn observe(&mut self, w: &World) {
        if self.finished || w.tick == self.last_tick {
            return;
        }
        self.last_tick = w.tick;
        let live_phase = w.match_state.phase == MatchPhase::Live;
        if live_phase {
            self.report.secs += f64::from(DT);
            for p in &w.players {
                if p.alive {
                    let v = p.mv.vel.length();
                    let bin = if v.is_finite() { ((v / SPEED_BIN_WIDTH) as usize).min(SPEED_BINS - 1) } else { SPEED_BINS - 1 };
                    self.report.speed_hist[bin] += 1;
                }
            }
        }
        let credit_ticks = (KO_ATTRIB_SECS / DT) as Tick;
        for e in w.events() {
            match e {
                GameEvent::ConstructSpawned { id, owner, recipe, cost, .. } => {
                    self.spawn(w, *id, *owner, recipe, *cost);
                }
                GameEvent::Grafted { player, id, recipe, cost } => {
                    let sig = Signature::of(recipe);
                    let c = f64::from(cost.max(0.0));
                    if let Some(l) = self.live.get_mut(id) {
                        l.sig = sig;
                        l.upkeep = w.tuning().willpower.upkeep_per_sec(recipe, false);
                    }
                    let (arch, held) = self.live.get(id).map_or((None, None), |l| (l.arch, l.held));
                    self.credit(&Attrib { sig: Some(sig), arch, held, ..Default::default() }, |b| {
                        b.casts += 1;
                        b.wp_cast += c;
                    });
                    let t = self.player(*player);
                    t.casts += 1;
                    t.wp += c;
                }
                GameEvent::CreatureAttack { id, kind, .. } => {
                    if let Some(l) = self.live.get_mut(id) {
                        l.attack = Some(*kind);
                    }
                }
                GameEvent::ConstructShattered { id, .. } => self.end_construct(*id, w.tick, true),
                GameEvent::ConstructGone { id, .. } => self.end_construct(*id, w.tick, false),
                GameEvent::Hit { target, source, construct, kind, damage, tumble, .. } => {
                    let d = if damage.is_finite() { damage.max(0.0) } else { 0.0 };
                    let a = self.attrib_of(*construct, *kind);
                    let other = source.is_some_and(|s| s != *target);
                    if other {
                        let dd = f64::from(d);
                        self.credit(&a, |b| {
                            b.hits += 1;
                            b.damage += dd;
                        });
                        if let Some(s) = source {
                            self.player(*s).dealt += dd;
                        }
                    }
                    let tick = w.tick;
                    let t = self.player(*target);
                    t.taken += f64::from(d);
                    t.since_spawn += d;
                    if other && d > 0.0 && t.first_hit.is_none() {
                        t.first_hit = Some(tick);
                    }
                    if other {
                        t.last_hit = Some((a, tick));
                    }
                    let _ = tumble;
                }
                GameEvent::Launched { player, .. } => {
                    let t = self.player(*player);
                    t.launched = true;
                    t.launches += 1;
                    self.report.launches += 1;
                }
                GameEvent::Tech { .. } => self.report.techs += 1,
                GameEvent::PunchThrough { .. } => self.report.punches += 1,
                GameEvent::Recovered { player, .. } => {
                    let t = self.player(*player);
                    if t.launched {
                        t.launched = false;
                        t.recoveries += 1;
                        self.report.recoveries += 1;
                    }
                }
                GameEvent::Ko { victim, killer, cause, shutdown, streak, zone, pct, .. } => {
                    let key = match zone {
                        Some(z) => format!("{cause:?}/{z:?}"),
                        None => format!("{cause:?}"),
                    };
                    *self.report.ko_causes.entry(key).or_default() += 1;
                    if *shutdown {
                        self.report.shutdowns += 1;
                    }
                    let tick = w.tick;
                    let vt = self.players[victim.0 as usize];
                    let p = if pct.is_finite() { *pct } else { vt.since_spawn };
                    self.report.ko_pct.push(p);
                    if let Some(k) = killer.filter(|k| k != victim) {
                        self.report.ko_credited += 1;
                        self.report.ko_pct_credited.push(p);
                        if let Some(f) = vt.first_hit {
                            self.report.fight_secs.push(tick.saturating_sub(f) as f32 * DT);
                        }
                        if let Some((a, at)) = vt.last_hit {
                            if tick.saturating_sub(at) <= credit_ticks {
                                self.credit(&a, |b| b.kos += 1);
                            }
                        }
                        let kt = self.player(k);
                        kt.kos += 1;
                        kt.streak = (*streak).max(1);
                        kt.max_streak = kt.max_streak.max(kt.streak);
                    } else if let Some((_, at)) = vt.last_hit {
                        self.report.ko_self_stale += 1;
                        self.report.ko_self_stale_secs.push(tick.saturating_sub(at) as f32 * DT);
                    } else {
                        self.report.ko_self_never_hit += 1;
                    }
                    self.end_streak(*victim);
                    let v = self.player(*victim);
                    v.deaths += 1;
                    v.since_spawn = 0.0;
                    v.first_hit = None;
                    v.last_hit = None;
                    v.launched = false;
                }
                GameEvent::Respawn { player, .. } => {
                    let v = self.player(*player);
                    v.since_spawn = 0.0;
                    v.first_hit = None;
                    v.last_hit = None;
                    v.launched = false;
                }
                GameEvent::MatchPhase { phase: MatchPhase::Ended, winner, .. } => {
                    self.report.winner = winner.map(|p| p.0);
                }
                _ => {}
            }
        }
    }

    /// Close the match: open constructs / streaks are finalised, winners and per-player rows
    /// filled in. Returns the report (the recorder stops observing).
    pub fn finish(&mut self, w: &World) -> MatchReport {
        if !self.finished {
            self.finished = true;
            let ids: Vec<ConstructId> = self.live.keys().copied().collect();
            let mut ids = ids;
            ids.sort();
            for id in ids {
                self.end_construct(id, w.tick, false);
            }
            if self.report.winner.is_none() {
                self.report.winner = w.scoreboard().first().map(|r| r.player.0);
            }
            for i in 0..self.players.len() {
                self.end_streak(PlayerId(i as u8));
            }
            let winner = self.report.winner;
            for (i, t) in self.players.iter().enumerate() {
                if !t.present {
                    continue;
                }
                let mut order: Vec<usize> = (0..MECHANICS.len()).filter(|k| t.mech_casts[*k] > 0).collect();
                order.sort_by(|a, b| t.mech_casts[*b].cmp(&t.mech_casts[*a]).then(a.cmp(b)));
                order.truncate(3);
                let win = winner == Some(i as u8);
                for &m in &order {
                    self.report.mech_wins[m].0 += 1;
                    self.report.mech_wins[m].1 += u32::from(win);
                }
                self.report.players.push(PlayerReport {
                    id: i as u8,
                    kos: t.kos,
                    deaths: t.deaths,
                    damage_dealt: t.dealt,
                    damage_taken: t.taken,
                    casts: t.casts,
                    willpower_spent: t.wp,
                    max_streak: t.max_streak,
                    launches: t.launches,
                    recoveries: t.recoveries,
                    top_mechanics: order.iter().map(|m| *m as u8).collect(),
                    winner: win,
                });
            }
        }
        self.report.clone()
    }
}
