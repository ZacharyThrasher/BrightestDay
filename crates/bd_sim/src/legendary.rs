//! Hidden legendary rolls tied to novelty (FREEDOM bundle). **Never shown** — no UI, text or
//! player-visible log ever mentions a chance, a novelty score or that a roll happened; a legendary
//! construct simply turns up golden (`replicate::cflags::LEGENDARY`).
//!
//! Host-side and deterministic: the roll is a pure function of (world seed, tick, caster, phrase key,
//! chance), drawn from its own [`SimRng`] so it never shifts any other random stream.
//!
//! **Chance** = `fresh · personal · (base + (cap − base) · rich)`, clamped to `cap`:
//! * `fresh` (match-wide novelty): 0 when anyone already cast this normalised phrase this match
//!   (so repeats and spam can **never** roll); otherwise `fresh_floor + (1 − fresh_floor) · f` with
//!   `f` = the share of its key concept tokens nobody used yet this match.
//! * `rich` (composition richness): distinct mechanics / features / payload / mount / element /
//!   temperament / count / concept words, weighted, over `rich_full`, capped at 1.
//! * `personal` (the caster's own variety, from [`crate::willpower::CastHistory`]):
//!   `1 / (1 + uses)²` for uses of the same resolved thing in the last `personal_window` s.
//!
//! Tunables: `tuning.freedom` (`assets/tuning/combat.ron`). Memory is fixed: a
//! [`NOVELTY_CAP`]-entry ring of phrase / token hashes per match.

use bd_core::mechanics::{Element, Recipe, Temperament};
use bd_core::{PlayerId, Tick};

use crate::rng::SimRng;

/// Phrase + token hashes remembered per match (oldest forgotten first). Fixed size.
pub const NOVELTY_CAP: usize = 512;
/// Concept tokens kept per phrase.
pub const MAX_TOKENS: usize = 12;

/// Words that carry no concept ("a", "the", "with", …) — dropped before hashing.
const STOP: &[&str] = &[
    "a", "an", "the", "of", "with", "and", "on", "in", "at", "to", "for", "that", "this", "it", "is", "my", "your", "some", "into", "onto",
    "from", "by", "its", "are", "be", "who", "which", "very", "so", "too", "just", "now", "please", "big", "giant", "huge", "small",
    "tiny", "little", "mega", "super",
];

/// FNV-1a (stable across platforms/runs).
fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A phrase reduced to what novelty compares: its concept tokens (lower-case, stop words and
/// size words dropped, crude singular, unique, at most [`MAX_TOKENS`]) and an order-free key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhraseKey {
    /// Hash of the sorted concept tokens ("flaming sword" == "SWORD, FLAMING!!!").
    pub key: u64,
    /// One hash per concept token (sorted, unique).
    pub tokens: Vec<u64>,
}

impl PhraseKey {
    pub fn of(phrase: &str) -> Self {
        let mut toks: Vec<String> = Vec::with_capacity(8);
        for w in phrase.split(|c: char| !c.is_ascii_alphanumeric()).take(32) {
            if w.is_empty() {
                continue;
            }
            let mut t = w.to_ascii_lowercase();
            // Stretched shouting ("HAMMERRRR") and plurals collapse to one concept.
            t = squeeze(&t);
            if t.len() > 3 && t.ends_with('s') && !t.ends_with("ss") {
                t.pop();
            }
            if t.len() < 2 || STOP.contains(&t.as_str()) {
                continue;
            }
            if !toks.contains(&t) {
                toks.push(t);
            }
            if toks.len() >= MAX_TOKENS {
                break;
            }
        }
        toks.sort();
        let tokens: Vec<u64> = {
            let mut v: Vec<u64> = toks.iter().map(|t| fnv(t.as_bytes())).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let key =
            if toks.is_empty() { fnv(phrase.trim().to_ascii_lowercase().as_bytes()) ^ 0x5bd1_e995 } else { fnv(toks.join(" ").as_bytes()) };
        PhraseKey { key, tokens }
    }
}

/// Runs of 3+ identical letters → 2 ("hammerrrr" → "hammerr" → then plural/stop rules).
fn squeeze(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = (' ', 0usize);
    for c in s.chars() {
        run = if run.0 == c { (c, run.1 + 1) } else { (c, 1) };
        if run.1 <= 2 {
            out.push(c);
        }
    }
    // "hammerr" → "hammer" when the doubled letter ends the word after a stretch.
    if out.len() > 4 && out.as_bytes()[out.len() - 1] == out.as_bytes()[out.len() - 2] && s.len() > out.len() {
        out.pop();
    }
    out
}

/// Match-wide memory of what was cast: (hash, bitmask of players who cast it, last-use stamp). Fixed
/// capacity; when full the **least recently cast** entry is forgotten, so a phrase that keeps being
/// cast (spam) is never forgotten and can never become "fresh" again.
#[derive(Clone, Debug)]
pub struct NoveltySet {
    entries: Vec<(u64, u16, u32)>,
    stamp: u32,
}

impl Default for NoveltySet {
    fn default() -> Self {
        Self { entries: Vec::with_capacity(NOVELTY_CAP), stamp: 0 }
    }
}

fn bit(p: PlayerId) -> u16 {
    1u16 << (p.0 % 16)
}

impl NoveltySet {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.stamp = 0;
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    fn find(&self, h: u64) -> Option<usize> {
        self.entries.iter().position(|e| e.0 == h)
    }
    pub fn contains(&self, h: u64) -> bool {
        self.find(h).is_some()
    }
    /// Has `p` cast `h` this match?
    pub fn cast_by(&self, h: u64, p: PlayerId) -> bool {
        self.find(h).is_some_and(|i| self.entries[i].1 & bit(p) != 0)
    }
    /// Remember `h` for `p` (refreshing it). Returns true when `p` had not cast it before this match.
    pub fn insert(&mut self, h: u64, p: PlayerId) -> bool {
        self.stamp = self.stamp.wrapping_add(1);
        let stamp = self.stamp;
        if let Some(i) = self.find(h) {
            let e = &mut self.entries[i];
            let fresh = e.1 & bit(p) == 0;
            e.1 |= bit(p);
            e.2 = stamp;
            return fresh;
        }
        if self.entries.len() < NOVELTY_CAP {
            self.entries.push((h, bit(p), stamp));
        } else if let Some(oldest) = self.entries.iter().enumerate().min_by_key(|(_, e)| e.2).map(|(i, _)| i) {
            self.entries[oldest] = (h, bit(p), stamp);
        }
        true
    }
}

/// The three novelty components (all 0..1). Hidden.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Novelty {
    /// Match-wide: 0 = someone already cast this phrase this match.
    pub fresh: f32,
    /// Composition richness.
    pub rich: f32,
    /// Caster's own variety.
    pub personal: f32,
}

impl Novelty {
    /// Freshness × personal variety × (0.3 + 0.7 · richness): the "wildest phrase" ranking.
    pub fn score(&self) -> f32 {
        self.fresh * self.personal * (0.3 + 0.7 * self.rich)
    }
}

/// Legendary tunables (`tuning.freedom.legendary`).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LegendaryTuning {
    /// Chance for a brand-new but plain phrase (before richness).
    pub base: f32,
    /// Chance ceiling (a brand-new, rich composition).
    pub cap: f32,
    /// Freshness of a new phrase made only of already-used words.
    pub fresh_floor: f32,
    /// Richness points for `rich = 1`.
    pub rich_full: f32,
    /// Window (s) of the caster's own history for personal variety.
    pub personal_window: f32,
    /// Scale multiplier of a legendary construct (may exceed the form clamp by this much).
    pub scale: f32,
    /// Temperament intensity added.
    pub intensity: f32,
    /// Integrity multiplier (a bit more durable).
    pub integrity: f32,
}

impl Default for LegendaryTuning {
    fn default() -> Self {
        Self { base: 0.01, cap: 0.12, fresh_floor: 0.35, rich_full: 5.0, personal_window: 45.0, scale: 1.2, intensity: 0.2, integrity: 1.3 }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl LegendaryTuning {
    pub fn validated(self) -> Self {
        let d = Self::default();
        let base = fin(self.base, d.base, 0.0, 1.0);
        Self {
            base,
            cap: fin(self.cap, d.cap, base, 1.0),
            fresh_floor: fin(self.fresh_floor, d.fresh_floor, 0.0, 1.0),
            rich_full: fin(self.rich_full, d.rich_full, 0.5, 50.0),
            personal_window: fin(self.personal_window, d.personal_window, 1.0, 600.0),
            scale: fin(self.scale, d.scale, 1.0, 2.0),
            intensity: fin(self.intensity, d.intensity, 0.0, 1.0),
            integrity: fin(self.integrity, d.integrity, 1.0, 3.0),
        }
    }
}

/// Richness points of a recipe as cast (+ how many concept words the phrase used).
pub fn richness_points(r: &Recipe, concept_words: usize) -> f32 {
    let mut p = 0.5 * r.mechanics.len() as f32 + 0.25 * r.features.len() as f32;
    if r.payload.is_some() {
        p += 1.0;
    }
    if r.mount.is_some() {
        p += 1.5;
    }
    if r.element != Element::Light {
        p += 1.0;
    }
    if r.temperament != Temperament::Calm {
        p += 0.75;
    }
    if r.count > 1 {
        p += 0.5;
    }
    p + 0.5 * concept_words.saturating_sub(1).min(4) as f32
}

/// Novelty of a cast about to happen (before it is recorded in `set`). `personal_uses` = the caster's
/// uses of the same resolved thing in the last `personal_window` s.
pub fn novelty(set: &NoveltySet, phrase: &PhraseKey, recipe: &Recipe, personal_uses: u32, t: &LegendaryTuning) -> Novelty {
    let fresh = if set.contains(phrase.key) {
        0.0
    } else if phrase.tokens.is_empty() {
        t.fresh_floor
    } else {
        let new = phrase.tokens.iter().filter(|h| !set.contains(**h)).count() as f32 / phrase.tokens.len() as f32;
        t.fresh_floor + (1.0 - t.fresh_floor) * new
    };
    let rich = (richness_points(recipe, phrase.tokens.len()) / t.rich_full.max(0.5)).clamp(0.0, 1.0);
    let personal = 1.0 / ((1 + personal_uses) as f32).powi(2);
    Novelty { fresh, rich, personal }
}

/// Legendary chance for a novelty (0 when the phrase isn't fresh).
pub fn chance(n: &Novelty, t: &LegendaryTuning) -> f32 {
    let f = n.fresh * n.personal;
    if f <= 0.0 {
        return 0.0;
    }
    (f * (t.base + (t.cap - t.base) * n.rich)).clamp(0.0, t.cap)
}

/// The deterministic roll: its own RNG from (seed, tick, caster, phrase) — host and any replica
/// running the same cast agree, and no other random stream moves.
pub fn roll(seed: u64, tick: Tick, player: PlayerId, phrase_key: u64, chance: f32) -> bool {
    if chance.is_nan() || chance <= 0.0 {
        return false;
    }
    let s = seed ^ tick.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((player.0 as u64) << 56) ^ phrase_key.rotate_left(29) ^ 0x1E6E_17DA_u64;
    SimRng::new(s).f32() < chance
}

/// Record a cast phrase for `p` (phrase key + concept tokens). Returns true when the phrase is
/// new for this player this match (the "most creative" count).
pub fn record(set: &mut NoveltySet, phrase: &PhraseKey, p: PlayerId) -> bool {
    let fresh = set.insert(phrase.key, p);
    for h in &phrase.tokens {
        set.insert(*h, p);
    }
    fresh
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::mechanics::{Form, Mechanic};

    fn rec(form: Form, mechs: &[Mechanic]) -> Recipe {
        let mut r = Recipe::unknown("x");
        r.form = form;
        r.mechanics = mechs.to_vec();
        r.normalized()
    }

    #[test]
    fn phrase_keys_ignore_order_case_and_filler() {
        assert_eq!(PhraseKey::of("flaming sword").key, PhraseKey::of("SWORD, FLAMING!!!").key);
        assert_eq!(PhraseKey::of("a hammer").key, PhraseKey::of("HAMMERRRR").key);
        assert_eq!(PhraseKey::of("swords").key, PhraseKey::of("sword").key);
        assert_ne!(PhraseKey::of("sword").key, PhraseKey::of("hammer").key);
        assert_eq!(PhraseKey::of("the big giant hammer").tokens.len(), 1);
        assert!(!PhraseKey::of("!!!").tokens.iter().any(|_| true));
    }

    #[test]
    fn set_is_fixed_size() {
        let mut s = NoveltySet::default();
        for i in 0..(NOVELTY_CAP as u64 * 3) {
            s.insert(i, PlayerId(1));
        }
        assert_eq!(s.len(), NOVELTY_CAP);
        assert!(s.contains(NOVELTY_CAP as u64 * 3 - 1));
        assert!(!s.contains(0), "oldest forgotten");
        // A phrase that keeps being cast is never forgotten, however much else is cast.
        let spam = u64::MAX;
        for i in 0..(NOVELTY_CAP as u64 * 4) {
            if i % 100 == 0 {
                s.insert(spam, PlayerId(2));
            }
            s.insert(10_000 + i, PlayerId(1));
        }
        assert!(s.contains(spam));
        assert_eq!(s.len(), NOVELTY_CAP);
    }

    #[test]
    fn repeats_have_zero_chance() {
        let t = LegendaryTuning::default();
        let mut set = NoveltySet::default();
        let k = PhraseKey::of("king kong on a skateboard");
        let r = rec(Form::Kaiju, &[Mechanic::Blunt, Mechanic::Grab, Mechanic::Charge, Mechanic::Heavy]);
        assert!(chance(&novelty(&set, &k, &r, 0, &t), &t) > 0.0);
        record(&mut set, &k, PlayerId(1));
        // Anyone, any wording of the same concepts, any personal state: 0.
        for p in 0..4 {
            let _ = p;
            let k2 = PhraseKey::of("SKATEBOARD KING KONG!!!");
            assert_eq!(chance(&novelty(&set, &k2, &r, 0, &t), &t), 0.0);
        }
    }
}
