//! Willpower economy (COMBAT.md §2–3): recipe validation/clamping, cast cost, staleness, upkeep,
//! integrity, dismiss refund, and the oath recital.
//!
//! All formulas are plain functions of the recipe and [`MatchRules`] so the client can preview a
//! cost ("the ring flickers") with exactly the host's numbers.

use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use bd_core::{DT, Tick};
use serde::{Deserialize, Serialize};

use crate::constructs::forms::{MAX_COUNT, MAX_MASS, MAX_SWARM_COUNT, MIN_MASS, form_spec};
use crate::rng::word_hash;
use crate::rules::{COLOSSAL_SCALE, MatchRules};

/// Passive regen (per second), paused after casting.
pub const REGEN_PER_SEC: f32 = 3.0;
pub const REGEN_PAUSE_AFTER_CAST: f32 = 1.5;
/// Fraction of the cast cost refunded on dismiss (X).
pub const DISMISS_REFUND: f32 = 0.25;
/// Staleness: each use of the exact same phrase within the window adds this to the multiplier.
pub const STALE_WINDOW_SECS: f32 = 30.0;
pub const STALE_PER_USE: f32 = 0.3;
/// At 0 willpower a player's constructs lose this fraction of their max integrity per second.
pub const STARVED_DECAY_PER_SEC: f32 = 0.35;
/// Grafting costs this fraction of what the grafted phrase would cost as a fresh cast.
pub const GRAFT_COST_FRACTION: f32 = 0.5;
/// Bounds that keep a (possibly hostile) recipe small on the wire and in the sim.
pub const MAX_NAME_LEN: usize = 48;
pub const MAX_MECHANICS: usize = 10;
pub const MAX_FEATURES: usize = 12;

/// Economy category base cost.
pub fn category_base(form: Form) -> f32 {
    if form == Form::Kaiju {
        return 60.0;
    }
    match form.category() {
        FormCategory::Held => 12.0,
        FormCategory::Worn => 15.0,
        FormCategory::Structure => 10.0,
        FormCategory::Object => 14.0,
        FormCategory::Creature => 25.0,
    }
}

/// Autonomous non-creatures (a held / structure / object recipe that `Flies` or `Walks`: "flying
/// hammer", "wall with legs") hunt like creatures, so they are priced like creatures: this cast
/// base and [`AUTONOMOUS_UPKEEP`] (balance pass 2, docs/BALANCE_REPORT.md). Behaviour is unchanged.
pub const AUTONOMOUS_BASE: f32 = 25.0;
pub const AUTONOMOUS_UPKEEP: f32 = 1.5;

/// Is this recipe priced as an autonomous non-creature? Decide it on the recipe the player
/// **typed** (before party / power grafts): standing grafts never raise a cast's price.
pub fn is_autonomous(recipe: &Recipe) -> bool {
    matches!(recipe.form.category(), FormCategory::Held | FormCategory::Structure | FormCategory::Object)
        && (recipe.has(Mechanic::Flies) || recipe.has(Mechanic::Walks))
}

/// Willpower drained per second while the construct lives (autonomy decided from the recipe).
pub fn upkeep_per_sec(recipe: &Recipe, piloted: bool) -> f32 {
    upkeep_per_sec_as(recipe, piloted, is_autonomous(recipe))
}

/// [`upkeep_per_sec`] with the autonomy pricing decided by the caller (the typed recipe's).
pub fn upkeep_per_sec_as(recipe: &Recipe, piloted: bool, autonomous: bool) -> f32 {
    let base: f32 = if autonomous && recipe.form != Form::Kaiju && recipe.form.category() != FormCategory::Creature {
        AUTONOMOUS_UPKEEP.max(upkeep_category(recipe))
    } else {
        upkeep_category(recipe)
    };
    if piloted {
        // Piloting drains like a creature; colossal things drain like a kaiju (COMBAT.md §5d).
        let p = if is_colossal(recipe) { 4.0 } else { 1.5 };
        base.max(p)
    } else {
        base
    }
}

fn upkeep_category(recipe: &Recipe) -> f32 {
    if recipe.form == Form::Kaiju {
        4.0
    } else {
        match recipe.form.category() {
            FormCategory::Held | FormCategory::Worn => 0.6,
            FormCategory::Creature => 1.5,
            FormCategory::Structure => 0.3,
            FormCategory::Object => 0.0,
        }
    }
}

pub fn is_colossal(recipe: &Recipe) -> bool {
    recipe.scale >= COLOSSAL_SCALE
}

fn truncate_name(s: &mut String) {
    if s.len() > MAX_NAME_LEN {
        let mut cut = MAX_NAME_LEN;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
}

/// Validate and clamp a recipe from the lexicon (or a client) into something the sim will run:
/// scale per form (and lobby [`crate::rules::ScaleLimit`]), mass, count (≤ 6, swarms ≤ 12), payload
/// nesting depth 1, bounded name/mechanics/features.
pub fn validate_recipe(recipe: Recipe, rules: &MatchRules) -> Recipe {
    let raw_scale = recipe.scale;
    let mut r = validate_inner(recipe, rules);
    // Readability: a creature under ~0.9 m is an unreadable speck (Pikachu at its canon 0.4 m is 8
    // voxels) — raise it, unless the phrase asked for small (tiny/mini/small…: the lexicon then puts
    // the scale well below the form default / under 0.3 m for icons), which keeps a ~0.5 m floor.
    // Top-level only (payload projectiles and swarm units stay small); idempotent on re-validation.
    if r.form.category() == FormCategory::Creature && !r.has(Mechanic::Swarm) {
        let explicit_small = if !raw_scale.is_finite() {
            false
        } else if (raw_scale - CREATURE_SMALL_FLOOR).abs() < 1e-3 {
            true // our own floor, re-validated
        } else if matches!(r.model, bd_core::mechanics::ModelSource::Icon(_)) {
            raw_scale < 0.3
        } else {
            raw_scale < form_spec(r.form).default_scale * 0.75
        };
        let floor = if explicit_small { CREATURE_SMALL_FLOOR } else { CREATURE_READABLE_FLOOR };
        let (_, hi) = rules.scale_range(0.0, form_spec(r.form).max_scale);
        r.scale = r.scale.max(floor.min(hi));
    }
    r
}

/// Minimum creature height (m) for readability (see [`validate_recipe`]).
pub const CREATURE_READABLE_FLOOR: f32 = 0.9;
/// Minimum for creatures the phrase explicitly asked to be small (not exactly any form default:
/// re-validation recognises it).
pub const CREATURE_SMALL_FLOOR: f32 = 0.55;

fn validate_inner(recipe: Recipe, rules: &MatchRules) -> Recipe {
    let mut r = recipe.normalized();
    truncate_name(&mut r.name);
    r.mechanics.truncate(MAX_MECHANICS);
    r.features.truncate(MAX_FEATURES);
    let spec = form_spec(r.form);
    let (lo, hi) = rules.scale_range(spec.min_scale, spec.max_scale);
    r.scale = r.scale.clamp(lo, hi);
    r.mass = r.mass.clamp(MIN_MASS, MAX_MASS);
    let max_count = if r.has(Mechanic::Swarm) { MAX_SWARM_COUNT } else { MAX_COUNT };
    r.count = r.count.clamp(1, max_count);
    if let Some(p) = r.payload.take() {
        let mut p = validate_inner(*p, rules);
        p.payload = None;
        r.payload = Some(Box::new(p));
    }
    // v1.6 mount ("king kong on a skateboard"): clamped like any recipe (per-form scale range of these
    // rules), one level only, and it neither shoots nor rides anything itself.
    if let Some(m) = r.mount.take() {
        let mut m = validate_inner(*m, rules);
        m.payload = None;
        m.mount = None;
        r.mount = Some(Box::new(m));
    }
    if let Some(a) = r.accent.as_mut() {
        for c in a.iter_mut() {
            *c = if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.0 };
        }
    }
    r
}

/// `clamp((scale/default)^1.3, 0.5, 3)`.
pub fn size_factor(recipe: &Recipe) -> f32 {
    let d = form_spec(recipe.form).default_scale;
    (recipe.scale / d).powf(1.3).clamp(0.5, 3.0)
}

/// Copies are cheaper than separate casts but not free: `count^0.6` (swarm units are small: `^0.4`).
pub fn count_factor(recipe: &Recipe) -> f32 {
    let c = recipe.count.max(1) as f32;
    if recipe.has(Mechanic::Swarm) { c.powf(0.4) } else { c.powf(0.6) }
}

/// Cast cost before staleness: `base · size · (1 + 0.15·#mechanics) · count` (a payload counts as
/// one mechanic).
pub fn base_cost(recipe: &Recipe) -> f32 {
    base_cost_as(recipe, is_autonomous(recipe))
}

/// [`base_cost`] with the autonomy pricing decided by the caller (the typed recipe's): an
/// autonomous non-creature uses [`AUTONOMOUS_BASE`] instead of its category base.
pub fn base_cost_as(recipe: &Recipe, autonomous: bool) -> f32 {
    let n = recipe.mechanics.len() + recipe.payload.is_some() as usize;
    let base = if autonomous && recipe.form.category() != FormCategory::Creature { AUTONOMOUS_BASE } else { category_base(recipe.form) };
    base * size_factor(recipe) * (1.0 + 0.15 * n as f32) * count_factor(recipe)
}

/// SPARKS (READABILITY; working name, see `MatchRules::willpower_segments`): every cast cost is a
/// whole number of sparks, so the crosshair gauge's preview is exact and a cost is learnable by feel
/// ("that dragon cost me three"). One spark = [`SEGMENT_BASE_POOL`] / segments (the Normal pool:
/// Double willpower shows twice the sparks, it does not make them bigger).
pub const SEGMENT_BASE_POOL: f32 = 100.0;
/// Rounding of a raw willpower cost to sparks (grafts' "it" resize; casts are priced by presence
/// tier instead: [`cast_sparks`]): `sparks = max(1, floor(cost / spark + QUANTIZE_BIAS))` — nearest,
/// leaning down a little (a remainder of 0.65 spark or more rounds up).
pub const QUANTIZE_BIAS: f32 = 0.35;

/// Whole sparks a raw cost rounds to (0 for a free / invalid cost; at least 1 otherwise).
pub fn cost_segments(cost: f32, segment: f32) -> u32 {
    if !(cost.is_finite() && cost > 0.0 && segment.is_finite() && segment > 0.0) {
        return 0;
    }
    ((cost / segment + QUANTIZE_BIAS).floor().max(1.0)).min(1.0e6) as u32
}

/// A raw cast cost rounded to whole sparks of `segment` willpower ([`cost_segments`] × `segment`).
/// A free cast stays free.
pub fn quantize_cost(cost: f32, segment: f32) -> f32 {
    if !(segment.is_finite() && segment > 0.0) {
        return if cost.is_finite() { cost.max(0.0) } else { 0.0 };
    }
    cost_segments(cost, segment) as f32 * segment
}

/// SPARKS price of a CAST, driven by the presence tier players SEE and HEAR
/// (`presence_tier::cast_tier`): size and gravitas teach the cost by feel. The raw willpower cost
/// stays the cast's *weight* (the tier's input, integrity, the spawn event, grafts, upkeep).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SparkTuning {
    /// Base sparks per presence tier: [Tiny, Normal, Big, Titan, Signature].
    pub tier: [u8; 5],
    /// A famous held weapon (an icon: Mjolnir, the Buster Sword — presence treats it as legendary).
    pub legendary: u8,
    /// +sparks for many copies (`count ≥ crowd_count`) or a swarm.
    pub crowd: u8,
    pub crowd_count: u8,
    /// +sparks for a piloted construct, an autonomous non-creature (a flying hammer hunts on its own)
    /// and strong utility (portals).
    pub piloted: u8,
    pub autonomous: u8,
    pub utility: u8,
    /// Nothing below the Signature tier costs more than this (and nothing more than the whole bar:
    /// every cast is castable from a full bar).
    pub cap: u8,
    /// Anti-spam: +sparks per repeat of the exact phrase inside the staleness window.
    pub stale: u8,
}

impl Default for SparkTuning {
    fn default() -> Self {
        Self { tier: [1, 2, 3, 5, 6], legendary: 6, crowd: 1, crowd_count: 3, piloted: 1, autonomous: 1, utility: 1, cap: 6, stale: 1 }
    }
}

/// Sparks a cast of `recipe` at presence tier `tier` (index 0 = Tiny … 4 = Signature) costs.
/// `famous` = a legendary held icon, `autonomous` = priced as an autonomous non-creature (decided on
/// the typed phrase), `uses` = repeats of the phrase inside the staleness window, `bar` = sparks in a
/// full pool. `mult` scales the result (party-free mechanics, your own domain), never below 1.
#[allow(clippy::too_many_arguments)]
pub fn cast_sparks(t: &SparkTuning, tier: usize, recipe: &Recipe, famous: bool, autonomous: bool, uses: u32, bar: u32, mult: f32) -> u32 {
    let bar = bar.max(1);
    let ultimate = tier >= 4;
    let mut n = t.tier.get(tier.min(4)).copied().unwrap_or(2) as u32;
    if famous {
        n = n.max(t.legendary as u32);
    }
    if recipe.count.max(1) >= t.crowd_count.max(2) || recipe.has(Mechanic::Swarm) {
        n += t.crowd as u32;
    }
    if recipe.has(Mechanic::Piloted) {
        n += t.piloted as u32;
    }
    if autonomous {
        n += t.autonomous as u32;
    }
    if recipe.has(Mechanic::Portal) {
        n += t.utility as u32;
    }
    let cap = if ultimate { bar } else { (t.cap as u32).max(t.legendary as u32 * famous as u32).max(1).min(bar) };
    n = n.clamp(1, cap);
    if mult.is_finite() && (mult - 1.0).abs() > 1e-4 {
        n = ((n as f32 * mult.max(0.0)).round() as u32).max(1);
    }
    (n + uses.min(64) * t.stale as u32).clamp(1, bar)
}

/// `1 + 0.3·uses` of this exact phrase in the last 30 s.
pub fn staleness(uses: u32) -> f32 {
    1.0 + STALE_PER_USE * uses as f32
}

/// Integrity of a freshly cast construct: `20 + 4·cost`.
pub fn integrity_for_cost(cost: f32) -> f32 {
    20.0 + 4.0 * cost
}

/// Recent casts of one player (for staleness). Fixed-size ring: no allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CastHistory {
    entries: [(u64, Tick); 16],
    next: u8,
}

impl CastHistory {
    pub fn record(&mut self, word: &str, tick: Tick) {
        self.entries[self.next as usize] = (word_hash(word), tick.max(1));
        self.next = (self.next + 1) % 16;
    }

    /// Uses of `word` within the staleness window ending at `now`.
    pub fn uses(&self, word: &str, now: Tick) -> u32 {
        self.uses_in(word, now, STALE_WINDOW_SECS)
    }

    /// [`Self::uses`] with a tuned window (TUNING HOOK: `tuning.willpower.stale_window`).
    pub fn uses_in(&self, word: &str, now: Tick, window_secs: f32) -> u32 {
        let h = word_hash(word);
        let window = (window_secs / DT) as Tick;
        self.entries.iter().filter(|(eh, t)| *t > 0 && *eh == h && now.saturating_sub(*t) <= window).count() as u32
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Tick of the newest recorded cast / move (0 = none) — WILLPOWER = LAUNCH's anti-camp leak.
    pub fn last_tick(&self) -> Tick {
        self.entries.iter().map(|e| e.1).max().unwrap_or(0)
    }
}

// ----------------------------------------------------------------------------------------- oath

/// Characters per second that earns the full "on target" refill.
pub const OATH_TARGET_CPS: f32 = 6.5;
/// Faster than this is not humanly plausible (≈ 240 wpm): the report is rejected.
pub const OATH_MAX_CPS: f32 = 20.0;
/// Network slack when comparing a client's claimed time with host-measured time (s).
pub const OATH_LATENCY_SLACK: f32 = 0.6;
/// Slack for a **spoken** recital (`tuning.willpower.oath_voice_slack`): the host's clock also runs
/// while Whisper transcribes (not the player's time), so voice gets more before the host-measured
/// time floors the claimed time. The plausibility check (`OATH_MAX_CPS`) is unchanged.
pub const OATH_VOICE_SLACK: f32 = 2.0;
/// Fraction of progress refunded when damage interrupts the recital.
pub const OATH_INTERRUPT_FRACTION: f32 = 0.5;

/// Most sections one oath is cut into (two authored lines, each split once if long).
pub const MAX_OATH_SECTIONS: usize = 4;
/// An authored line longer than this (characters) is split at the clause boundary nearest its middle.
pub const OATH_SECTION_MAX_CHARS: usize = 95;
/// Section length used for a corps with no oath in the file.
pub const OATH_FALLBACK_LEN: u16 = 110;

/// Parse `assets/oaths.ron`: a map of corps **name** → oath text (raw: authored line breaks kept).
/// String keys, so an oath for a corps the game doesn't have yet (e.g. `"Death"`) is simply
/// ignored instead of failing the whole file.
pub fn parse_oath_ron(src: &str) -> Result<std::collections::HashMap<String, String>, String> {
    ron::from_str(src).map_err(|e| e.to_string())
}

/// The recital sections of one oath — ONE definition shared by the sim ([`OathTable`], lengths) and
/// the client (texts), so the two always agree. Each authored line is a section (whitespace inside
/// collapsed to single spaces); a line over [`OATH_SECTION_MAX_CHARS`] is split at the clause
/// boundary (`, . ; : ! ? -` then a space) nearest its middle. At most [`MAX_OATH_SECTIONS`].
pub fn oath_sections(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let s = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !s.is_empty() {
            split_oath_line(&s, &mut out);
        }
    }
    out.truncate(MAX_OATH_SECTIONS);
    out
}

fn split_oath_line(s: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= OATH_SECTION_MAX_CHARS {
        out.push(s.to_string());
        return;
    }
    let mid = chars.len() / 2;
    let is_clause = |i: usize| i > 0 && matches!(chars[i - 1], ',' | '.' | ';' | ':' | '!' | '?' | '-');
    let spaces = || (1..chars.len() - 1).filter(|&i| chars[i] == ' ');
    let cut = spaces().filter(|&i| is_clause(i)).min_by_key(|&i| i.abs_diff(mid)).or_else(|| spaces().min_by_key(|&i| i.abs_diff(mid)));
    match cut {
        Some(i) => {
            let a: String = chars[..i].iter().collect();
            let b: String = chars[i + 1..].iter().collect();
            split_oath_line(a.trim(), out);
            split_oath_line(b.trim(), out);
        }
        None => out.push(s.to_string()),
    }
}

/// Oath section lengths per corps (the sim only needs lengths; the client shows the text).
/// Fixed-size: looking a section up in the tick never allocates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OathTable {
    /// Characters per section, indexed by `Corps::index()` then section (unused slots 0).
    pub sections: [[u16; MAX_OATH_SECTIONS]; 8],
    /// Sections per corps (≥ 1).
    pub counts: [u8; 8],
}

impl Default for OathTable {
    /// Parsed from the shipped `assets/oaths.ron` at compile time (a corps missing from the file —
    /// or an unparsable file — gets one [`OATH_FALLBACK_LEN`]-char section).
    fn default() -> Self {
        Self::from_ron(include_str!("../../../assets/oaths.ron")).unwrap_or_else(|_| Self::fallback())
    }
}

impl OathTable {
    /// One [`OATH_FALLBACK_LEN`]-char section for every corps.
    pub fn fallback() -> Self {
        let mut sections = [[0u16; MAX_OATH_SECTIONS]; 8];
        for s in &mut sections {
            s[0] = OATH_FALLBACK_LEN;
        }
        Self { sections, counts: [1; 8] }
    }

    /// Parse an `oaths.ron` map `{ "Willpower": "…", "Rage": "…", … }` (unknown names ignored).
    pub fn from_ron(src: &str) -> Result<Self, String> {
        let map = parse_oath_ron(src)?;
        let mut t = Self::fallback();
        for c in bd_core::corps::Corps::ALL {
            let Some(text) = map.get(c.name()) else { continue };
            let secs = oath_sections(text);
            if secs.is_empty() {
                continue;
            }
            let i = c.index() as usize;
            t.sections[i] = [0; MAX_OATH_SECTIONS];
            for (k, s) in secs.iter().enumerate() {
                t.sections[i][k] = s.chars().count().clamp(1, u16::MAX as usize) as u16;
            }
            t.counts[i] = secs.len() as u8;
        }
        Ok(t)
    }

    /// Number of sections of `corps`'s oath (≥ 1).
    pub fn section_count(&self, corps: bd_core::corps::Corps) -> u8 {
        self.counts[corps.index() as usize].max(1)
    }

    /// `section` if it exists for `corps`, else 0 (untrusted input from the client).
    pub fn valid_section(&self, corps: bd_core::corps::Corps, section: u8) -> u8 {
        if section < self.section_count(corps) { section } else { 0 }
    }

    /// Characters of one section (an out-of-range section reads as section 0).
    pub fn section_len(&self, corps: bd_core::corps::Corps, section: u8) -> u16 {
        let s = self.valid_section(corps, section) as usize;
        self.sections[corps.index() as usize][s].max(1)
    }
}

/// Seconds that earn the full refill for an oath of `len` characters.
pub fn oath_target_secs(len: u16) -> f32 {
    len as f32 / OATH_TARGET_CPS
}

/// Fastest plausible time for `len` characters.
pub fn oath_min_secs(len: u16) -> f32 {
    len as f32 / OATH_MAX_CPS
}

/// `100 · accuracy² · clamp(target / taken, 0.5, 1.2)` (scaled to the pool size).
pub fn oath_refill(accuracy: f32, seconds: f32, len: u16, pool: f32) -> f32 {
    let acc = if accuracy.is_finite() { accuracy.clamp(0.0, 1.0) } else { 0.0 };
    let t = seconds.max(1e-3);
    pool * acc * acc * (oath_target_secs(len) / t).clamp(0.5, 1.2)
}

/// Partial refill when damage interrupts: progress so far × 0.5.
pub fn oath_partial(progress_chars: u16, accuracy: f32, len: u16, pool: f32) -> f32 {
    let frac = (progress_chars as f32 / len.max(1) as f32).clamp(0.0, 1.0);
    let acc = accuracy.clamp(0.0, 1.0);
    pool * acc * acc * frac * OATH_INTERRUPT_FRACTION
}

/// Per-player oath recital state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum OathState {
    #[default]
    Idle,
    Reciting {
        started: Tick,
        len: u16,
        /// Last reported correct characters (optional client progress reports).
        progress: u16,
        /// Last reported accuracy (defaults to 0.9 until reported).
        accuracy: f32,
    },
}

impl OathState {
    pub fn reciting(&self) -> bool {
        matches!(self, OathState::Reciting { .. })
    }
}

/// Why an oath report was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OathError {
    NotReciting,
    /// Faster than humanly possible for this length, or faster than the host saw time pass.
    Implausible,
    Dead,
    UnknownPlayer,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::mechanics::Recipe;

    #[test]
    fn creatures_get_a_readable_minimum_scale() {
        use bd_core::mechanics::ModelSource;
        let rules = MatchRules::default();
        let mk = |form, scale: f32, icon: bool| {
            let mut r = Recipe::unknown("x");
            r.form = form;
            r.scale = scale;
            if icon {
                r.model = ModelSource::Icon("pikachu".into());
            }
            r
        };
        let pika = validate_recipe(mk(Form::Quadruped, 0.4, true), &rules);
        assert!((pika.scale - CREATURE_READABLE_FLOOR).abs() < 1e-4, "{}", pika.scale);
        let tiny_pika = validate_recipe(mk(Form::Quadruped, 0.16, true), &rules);
        assert!((tiny_pika.scale - CREATURE_SMALL_FLOOR).abs() < 1e-4, "{}", tiny_pika.scale);
        let tiny_dog = validate_recipe(mk(Form::Quadruped, 1.2 * 0.4, false), &rules);
        assert!((tiny_dog.scale - CREATURE_SMALL_FLOOR).abs() < 1e-4, "{}", tiny_dog.scale);
        let bug = validate_recipe(mk(Form::Insect, 0.5, false), &rules);
        assert!((bug.scale - CREATURE_READABLE_FLOOR).abs() < 1e-4, "{}", bug.scale);
        // Idempotent (party re-validates), and non-creatures are untouched.
        for r in [pika, tiny_pika, tiny_dog, bug] {
            assert_eq!(validate_recipe(r.clone(), &rules).scale, r.scale);
        }
        assert_eq!(validate_recipe(mk(Form::Gauntlet, 0.4, false), &rules).scale, 0.4);
    }

    #[test]
    fn oaths_parse_from_asset() {
        use bd_core::corps::Corps;
        let src = include_str!("../../../assets/oaths.ron");
        let map = parse_oath_ron(src).expect("assets/oaths.ron must parse");
        // A key the game has no corps for (Death) is ignored, not an error.
        assert!(map.contains_key("Death"));
        let t = OathTable::from_ron(src).unwrap();
        assert_eq!(t, OathTable::default());
        for c in Corps::ALL {
            // No corps may fall back: every one has its own text, and the exact section lengths.
            let text = map.get(c.name()).unwrap_or_else(|| panic!("no oath for {}", c.name()));
            let secs = oath_sections(text);
            assert!((2..=MAX_OATH_SECTIONS).contains(&secs.len()), "{}: {secs:?}", c.name());
            assert_eq!(t.section_count(c) as usize, secs.len(), "{}", c.name());
            for (k, s) in secs.iter().enumerate() {
                assert!(!s.contains('\n') && !s.contains("  ") && s.trim() == s, "{}: {s:?}", c.name());
                assert!(s.chars().count() <= OATH_SECTION_MAX_CHARS, "{}: {s:?}", c.name());
                assert_eq!(t.section_len(c, k as u8) as usize, s.chars().count(), "{} §{k}", c.name());
                assert!(t.section_len(c, k as u8) > 40, "{} §{k} suspiciously short: {s:?}", c.name());
            }
            // The sections are the whole oath, in order.
            let whole = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(secs.join(" "), whole, "{}", c.name());
            // Out of range → section 0.
            assert_eq!(t.section_len(c, 200), t.section_len(c, 0));
            assert_eq!(t.valid_section(c, t.section_count(c)), 0);
        }
    }

    #[test]
    fn long_lines_split_at_the_middle_clause() {
        let line = "Aaaa bbbb cccc dddd eeee, ffff gggg hhhh iiii jjjj kkkk llll, mmmm nnnn oooo pppp qqqq rrrr ssss tttt uuuu vvvv.";
        assert!(line.chars().count() > OATH_SECTION_MAX_CHARS);
        let s = oath_sections(&format!("{line}\n  Short second line."));
        assert_eq!(
            s,
            vec![
                "Aaaa bbbb cccc dddd eeee, ffff gggg hhhh iiii jjjj kkkk llll,".to_string(),
                "mmmm nnnn oooo pppp qqqq rrrr ssss tttt uuuu vvvv.".to_string(),
                "Short second line.".to_string(),
            ]
        );
        // A one-line oath still gets sections when it's long; blank lines are skipped.
        assert_eq!(oath_sections("\n  one   short  line \n\n"), vec!["one short line".to_string()]);
    }

    #[test]
    fn section_refill_keeps_plausibility() {
        // A section is ~half the old whole-oath length: the same full refill on target, in half the
        // time, and the plausibility floor still scales with the section.
        let t = OathTable::default();
        for c in bd_core::corps::Corps::ALL {
            for s in 0..t.section_count(c) {
                let len = t.section_len(c, s);
                assert!((oath_refill(1.0, oath_target_secs(len), len, 100.0) - 100.0).abs() < 1e-3);
                assert!(oath_min_secs(len) >= 2.5, "{c:?} §{s}: {len} chars is too short to be a recital");
                assert!(oath_target_secs(len) < 15.0);
            }
        }
    }

    #[test]
    fn cost_formula_matches_design() {
        let mut r = Recipe::unknown("hammer");
        r.form = Form::Hammer;
        r.scale = form_spec(Form::Hammer).default_scale;
        let r = validate_recipe(r, &MatchRules::default());
        assert!((base_cost(&r) - 12.0).abs() < 1e-4);
        let mut r2 = r.clone();
        r2.mechanics = vec![Mechanic::Returning, Mechanic::Blunt];
        assert!((base_cost(&r2) - 12.0 * 1.3).abs() < 1e-4);
        let mut big = r.clone();
        big.scale = 100.0;
        let big = validate_recipe(big, &MatchRules::default());
        assert_eq!(big.scale, form_spec(Form::Hammer).max_scale);
        assert!(size_factor(&big) <= 3.0);
    }

    #[test]
    fn costs_quantise_to_whole_sparks() {
        let spark = SEGMENT_BASE_POOL / 8.0;
        // Free stays free; anything paid costs at least one spark.
        assert_eq!(quantize_cost(0.0, spark), 0.0);
        assert_eq!(quantize_cost(-3.0, spark), 0.0);
        assert_eq!(quantize_cost(f32::NAN, spark), 0.0);
        assert_eq!(quantize_cost(0.5, spark), spark);
        assert_eq!(cost_segments(6.8, spark), 1, "a wall");
        assert_eq!(cost_segments(13.8, spark), 1, "a sword");
        // Rounding leans down a little: 1.64 sparks → 1, 1.65 → 2.
        assert_eq!(cost_segments(spark * 1.64, spark), 1);
        assert_eq!(cost_segments(spark * 1.66, spark), 2);
        assert_eq!(cost_segments(52.2, spark), 4, "a gun");
        assert_eq!(cost_segments(60.9, spark), 5, "a spirit bomb");
        assert_eq!(cost_segments(108.8, spark), 9, "a fire dragon (more than a full bar: it sputters)");
        // A whole bar is exactly the pool (no float drift past it) for every segment count.
        for n in 3..=12u32 {
            let s = SEGMENT_BASE_POOL / n as f32;
            assert!((quantize_cost(SEGMENT_BASE_POOL, s) - SEGMENT_BASE_POOL).abs() < 1e-3, "{n}");
            for k in 1..=n {
                let c = quantize_cost(s * k as f32, s);
                assert_eq!(cost_segments(c, s), k, "idempotent: {n} sparks, {k}");
            }
        }
        // Monotone.
        let mut last = 0.0;
        for c in 0..400 {
            let q = quantize_cost(c as f32 * 0.5, spark);
            assert!(q >= last);
            last = q;
        }
        // No spark size: unchanged.
        assert_eq!(quantize_cost(7.5, 0.0), 7.5);
    }

    #[test]
    fn cast_sparks_follow_the_tier_and_stay_castable() {
        let t = SparkTuning::default();
        let mut r = Recipe::unknown("x");
        r.form = Form::Sword;
        // Tier bases: Tiny 1, Normal 2, Big 3, Titan 5, Signature 6.
        for (tier, want) in [(0, 1), (1, 2), (2, 3), (3, 5), (4, 6)] {
            assert_eq!(cast_sparks(&t, tier, &r, false, false, 0, 8, 1.0), want, "tier {tier}");
        }
        // A famous held icon is legendary: 6.
        assert_eq!(cast_sparks(&t, 1, &r, true, false, 0, 8, 1.0), 6);
        // Adjustments: a swarm / crowd, a piloted mech, a flying hammer, a portal: +1 each.
        let mut swarm = r.clone();
        swarm.mechanics = vec![Mechanic::Swarm];
        assert_eq!(cast_sparks(&t, 1, &swarm, false, false, 0, 8, 1.0), 3);
        let mut crowd = r.clone();
        crowd.count = 3;
        assert_eq!(cast_sparks(&t, 0, &crowd, false, false, 0, 8, 1.0), 2);
        assert_eq!(cast_sparks(&t, 0, &r, false, true, 0, 8, 1.0), 2, "autonomous");
        let mut mech = r.clone();
        mech.mechanics = vec![Mechanic::Piloted, Mechanic::Swarm, Mechanic::Portal];
        // Everything below Signature caps at 6; a Signature may take the whole bar.
        assert_eq!(cast_sparks(&t, 3, &mech, false, true, 0, 8, 1.0), 6);
        assert_eq!(cast_sparks(&t, 4, &mech, false, true, 0, 8, 1.0), 8);
        // Never more than the bar: a 3-spark bar fits a Titan.
        assert_eq!(cast_sparks(&t, 3, &r, false, false, 0, 3, 1.0), 3);
        // Staleness: +1 per repeat, felt even on a 1-spark cast; still capped by the bar.
        assert_eq!(cast_sparks(&t, 0, &r, false, false, 1, 8, 1.0), 2);
        assert_eq!(cast_sparks(&t, 0, &r, false, false, 2, 8, 1.0), 3);
        assert_eq!(cast_sparks(&t, 3, &r, false, false, 9, 8, 1.0), 8);
        // A cost multiplier (your own domain: ×0.5) rounds, never below 1.
        assert_eq!(cast_sparks(&t, 3, &r, false, false, 0, 8, 0.5), 3);
        assert_eq!(cast_sparks(&t, 0, &r, false, false, 0, 8, 0.5), 1);
    }

    #[test]
    fn staleness_window() {
        let mut h = CastHistory::default();
        h.record("Hammer", 100);
        h.record("hammer", 200);
        assert_eq!(h.uses("HAMMER", 300), 2);
        assert_eq!(h.uses("hammer", 200 + (31.0 / DT) as Tick), 0);
        assert_eq!(h.uses("sword", 300), 0);
    }

    #[test]
    fn payload_depth_is_one_and_counts_clamped() {
        let mut inner = Recipe::unknown("bee");
        inner.payload = Some(Box::new(Recipe::unknown("anvil")));
        let mut r = Recipe::unknown("cannon");
        r.form = Form::Cannon;
        r.count = 50;
        r.payload = Some(Box::new(inner));
        let r = validate_recipe(r, &MatchRules::default());
        assert_eq!(r.count, MAX_COUNT);
        assert!(r.payload.as_ref().unwrap().payload.is_none());
    }

    #[test]
    fn oath_refill_curve() {
        let len = 110;
        let on_target = oath_refill(1.0, oath_target_secs(len), len, 100.0);
        assert!((on_target - 100.0).abs() < 1e-3);
        let fast = oath_refill(1.0, oath_target_secs(len) * 0.5, len, 100.0);
        assert!((fast - 120.0).abs() < 1e-3);
        let sloppy = oath_refill(0.8, oath_target_secs(len), len, 100.0);
        assert!((sloppy - 64.0).abs() < 1e-3);
    }
}
