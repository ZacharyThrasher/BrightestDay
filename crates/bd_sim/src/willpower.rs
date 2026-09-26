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

/// Willpower drained per second while the construct lives.
pub fn upkeep_per_sec(recipe: &Recipe, piloted: bool) -> f32 {
    let base: f32 = if recipe.form == Form::Kaiju {
        4.0
    } else {
        match recipe.form.category() {
            FormCategory::Held | FormCategory::Worn => 0.6,
            FormCategory::Creature => 1.5,
            FormCategory::Structure => 0.3,
            FormCategory::Object => 0.0,
        }
    };
    if piloted {
        // Piloting drains like a creature; colossal things drain like a kaiju (COMBAT.md §5d).
        let p = if is_colossal(recipe) { 4.0 } else { 1.5 };
        base.max(p)
    } else {
        base
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
    let n = recipe.mechanics.len() + recipe.payload.is_some() as usize;
    category_base(recipe.form) * size_factor(recipe) * (1.0 + 0.15 * n as f32) * count_factor(recipe)
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
}

// ----------------------------------------------------------------------------------------- oath

/// Characters per second that earns the full "on target" refill.
pub const OATH_TARGET_CPS: f32 = 6.5;
/// Faster than this is not humanly plausible (≈ 240 wpm): the report is rejected.
pub const OATH_MAX_CPS: f32 = 20.0;
/// Network slack when comparing a client's claimed time with host-measured time (s).
pub const OATH_LATENCY_SLACK: f32 = 0.6;
/// Fraction of progress refunded when damage interrupts the recital.
pub const OATH_INTERRUPT_FRACTION: f32 = 0.5;

/// Oath length per corps (the sim only needs lengths; the client shows the text).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OathTable {
    /// Characters, indexed by `Corps::index()`.
    pub lens: [u16; 8],
}

impl Default for OathTable {
    /// Parsed from the shipped `assets/oaths.ron` at compile time (falls back to 110 chars).
    fn default() -> Self {
        Self::from_ron(include_str!("../../../assets/oaths.ron")).unwrap_or(Self { lens: [110; 8] })
    }
}

impl OathTable {
    /// Parse an `oaths.ron` map `{ Willpower: "…", Rage: "…", … }`.
    pub fn from_ron(src: &str) -> Result<Self, String> {
        let map: std::collections::HashMap<bd_core::corps::Corps, String> =
            ron::from_str(src).map_err(|e| e.to_string())?;
        let mut lens = [110u16; 8];
        for (c, text) in &map {
            lens[c.index() as usize] = text.chars().count().clamp(1, u16::MAX as usize) as u16;
        }
        Ok(Self { lens })
    }

    pub fn len(&self, corps: bd_core::corps::Corps) -> u16 {
        self.lens[corps.index() as usize]
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
        let t = OathTable::default();
        assert!(t.lens.iter().all(|l| *l > 60 && *l < 200), "{:?}", t.lens);
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
