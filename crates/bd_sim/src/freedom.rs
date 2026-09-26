//! FREEDOM bundle (owner-approved) — the sim half. Every piece is small and hooks into the existing
//! cast path (`world/actions.rs`, marked `FREEDOM HOOK`):
//!
//! * **"it" = your last construct** — the client resolves "make it bigger" / "set it on fire" to its
//!   newest live construct and casts the rest as a graft with `TargetHint::Construct(id)`; the host's
//!   existing ownership check decides (not yours / gone → a normal cast). A scale word travels as
//!   [`CastMeta::resize`] (host-clamped to `tuning.freedom.resize_min..=resize_max` and the rules'
//!   scale range; the growth is paid like buying the bigger construct).
//! * **Name targeting** — "missile at Jake": the client strips the name and sends
//!   `TargetHint::Player(jake)`; the host already validates the player exists (homing does the rest).
//! * **Never refuse for willpower** ([`afford`]): an unaffordable cast becomes a scaled-down
//!   *sputtering* version (count first, then scale, down to `sputter_floor` of the asked size or the
//!   form minimum) that **drains everything left**; at ~0 willpower it is a tiny fizzle — never
//!   nothing. Grafts / animated terrain that can't be paid drain what's left and apply weaker.
//!   Replicated as `cflags::SPUTTER` (the client reuses the low-integrity flicker).
//! * **Yelling matters, a little** ([`energy_bonus`]): the client sends shout energy as a `u8`
//!   ([`energy_byte`]: typed CAPS / "!!!" and a real yell both saturate at 255 — voice == typing);
//!   the host maps it to at most `energy_max_bonus` (+15 %) scale / intensity. Paid as usual.
//! * **Legendary rolls** ([`crate::legendary`]): hidden, novelty-driven, deterministic. Golden
//!   (`cflags::LEGENDARY`), +20 % scale / intensity, a bit more integrity.
//! * **Kill feed** ([`FreedomEvent::Feed`]): the victim, the verb (by mechanic / element / hit kind,
//!   [`VERBS`]) and the killer construct's phrase *as cast* ("Sam was flattened by KING KONG ON A
//!   SKATEBOARD"). Clients slur-filter it on display ([`feed_text`]).
//! * **Highlights** ([`FreedomEvent::Highlights`], at match end): wildest phrases (hidden novelty
//!   ranking, no numbers shown), the top-3 KO lines, the biggest summon, the most creative player.
//!
//! Memory is fixed-size (novelty ring, tag ring, ≤ 3-entry books); nothing runs per frame beyond a
//! scan of the tick's events.

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Tick};
use serde::{Deserialize, Serialize};

use crate::World;
use crate::combat::HitKind;
use crate::events::{GameEvent, KoCause};
use crate::legendary::{self, LegendaryTuning, NoveltySet, PhraseKey};

/// Longest phrase kept for the feed / highlights (the shout words cap, `bd_fx::SHOUT_MAX_CHARS`).
pub const PHRASE_MAX: usize = 48;
/// A cast meta is used by that player's next cast within this many ticks (0.5 s), else dropped.
pub const META_TTL: Tick = 60;
/// Constructs whose phrase is remembered for the feed (ring; old ones are forgotten).
pub const TAG_CAP: usize = 192;
/// Shout energy (`bd_fx::typed_energy` / `voice_energy`) at or below this is calm (no bonus).
pub const ENERGY_CALM: f32 = 0.45;
/// …and at or above this a full shout (255). A typed "HAMMER!!!" (≈ 1.43) and a real yell (≈ 2.2)
/// both saturate: voice == typing (owner rule).
pub const ENERGY_FULL: f32 = 1.4;

// ------------------------------------------------------------------------------------ cast meta

/// How a cast was said — sent by the client right before its `Cast` / `Signature` command
/// ([`World::set_cast_meta`]). Untrusted: the host sanitises and clamps everything.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CastMeta {
    /// The phrase as typed / spoken (sanitised, ≤ [`PHRASE_MAX`] chars) — the kill feed's words.
    pub phrase: String,
    /// Shout energy 0 (calm) … 255 (full shout / yell).
    pub energy: u8,
    /// "it" grafts: resize the target by this factor (1 = none).
    pub resize: f32,
}

impl Default for CastMeta {
    fn default() -> Self {
        Self { phrase: String::new(), energy: 0, resize: 1.0 }
    }
}

impl CastMeta {
    pub fn new(phrase: &str, energy: u8) -> Self {
        Self { phrase: sanitize_phrase(phrase), energy, resize: 1.0 }
    }

    /// Untrusted → safe.
    pub fn sanitized(self) -> Self {
        let resize = if self.resize.is_finite() && self.resize > 0.0 { self.resize.clamp(0.25, 4.0) } else { 1.0 };
        Self { phrase: sanitize_phrase(&self.phrase), energy: self.energy, resize }
    }
}

/// Font characters only (A–Z a–z 0–9 `! ? - ' .`), whitespace collapsed, runs of one letter capped
/// at 6, cut at a word boundary to [`PHRASE_MAX`] — the same rule as `bd_fx::sanitize_shout`.
pub fn sanitize_phrase(raw: &str) -> String {
    let mut out = String::with_capacity(PHRASE_MAX + 8);
    let mut gap = false;
    let mut run = (' ', 0usize);
    for c in raw.chars().take(512) {
        if c.is_ascii_alphanumeric() || matches!(c, '!' | '?' | '-' | '\'' | '.') {
            if gap && !out.is_empty() {
                out.push(' ');
                run = (' ', 0);
            }
            gap = false;
            let k = c.to_ascii_lowercase();
            run = if run.0 == k { (k, run.1 + 1) } else { (k, 1) };
            if run.1 > 6 {
                continue;
            }
            out.push(c);
            if out.len() > PHRASE_MAX + 1 {
                break;
            }
        } else if c.is_whitespace() || matches!(c, ',' | '_' | ';' | ':' | '/' | '~' | '*') {
            gap = true;
        }
    }
    if out.len() > PHRASE_MAX {
        let cut = out[..=PHRASE_MAX].rfind(' ').filter(|&i| i >= PHRASE_MAX / 2).unwrap_or(PHRASE_MAX);
        out.truncate(cut);
    }
    out.truncate(out.trim_end().len());
    out
}

/// Client: shout energy (`bd_fx::typed_energy` or `voice_energy`) → the byte sent with the cast.
/// Calm (≤ [`ENERGY_CALM`]) = 0, a full shout (≥ [`ENERGY_FULL`]) = 255, linear between. Typing and
/// voice go through the same function, and both reach 255.
pub fn energy_byte(shout_energy: f32) -> u8 {
    if !shout_energy.is_finite() {
        return 0;
    }
    (((shout_energy - ENERGY_CALM) / (ENERGY_FULL - ENERGY_CALM)).clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Host: energy byte → scale / intensity bonus fraction (0 … `energy_max_bonus`).
pub fn energy_bonus(energy: u8, t: &FreedomTuning) -> f32 {
    energy as f32 / 255.0 * t.energy_max_bonus
}

// ------------------------------------------------------------------------------------ tuning

/// `tuning.freedom` (assets/tuning/combat.ron). Hidden.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FreedomTuning {
    /// Scale / intensity bonus of a full shout (voice or typed).
    pub energy_max_bonus: f32,
    /// "it" resize factor range.
    pub resize_min: f32,
    pub resize_max: f32,
    /// A sputtering cast keeps at least this fraction of the asked scale (or the form minimum).
    pub sputter_floor: f32,
    pub legendary: LegendaryTuning,
}

impl Default for FreedomTuning {
    fn default() -> Self {
        Self { energy_max_bonus: 0.15, resize_min: 0.5, resize_max: 2.0, sputter_floor: 0.35, legendary: LegendaryTuning::default() }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl FreedomTuning {
    pub fn validated(self) -> Self {
        let d = Self::default();
        let resize_min = fin(self.resize_min, d.resize_min, 0.25, 1.0);
        Self {
            energy_max_bonus: fin(self.energy_max_bonus, d.energy_max_bonus, 0.0, 0.5),
            resize_min,
            resize_max: fin(self.resize_max, d.resize_max, 1.0, 4.0),
            sputter_floor: fin(self.sputter_floor, d.sputter_floor, 0.05, 1.0),
            legendary: self.legendary.validated(),
        }
    }
}

// ------------------------------------------------------------------------------------ verbs

/// Kill-feed verbs (data table; index = the `verb` byte of [`FeedLine`]).
pub const VERBS: &[&str] = &[
    "bonked",       // 0 default
    "flattened",    // 1 heavy / dropped / stomped / crushed / earth
    "zapped",       // 2 electric
    "torched",      // 3 fire / atomic
    "frozen",       // 4 ice
    "launched",     // 5 launch / bounce
    "skewered",     // 6 pierce
    "sliced",       // 7 blade
    "blown up",     // 8 explodes / explosion
    "chomped",      // 9 creature
    "swarmed",      // 10 swarm
    "squeezed",     // 11 grab / constrict
    "lasered",      // 12 beam
    "washed away",  // 13 water
    "blown away",   // 14 wind
    "poisoned",     // 15 poison
    "run over",     // 16 charge / fast / vehicle
    "shot",         // 17 shoots / guns
    "blasted",      // 18 ring blast / no construct
    "mind-blasted", // 19 psychic
    "swallowed",    // 20 shadow
    "sent flying",  // 21 blast zone with no construct
    "erased",       // 22 an erasing orb (signature::orb: Hollow Purple)
];
/// `verb` byte for a KO with no killer ("Sam fell").
pub const VERB_FELL: u8 = 255;

/// Verb text for a byte (unknown → "bonked"; [`VERB_FELL`] → "fell").
pub fn verb_text(v: u8) -> &'static str {
    if v == VERB_FELL {
        return "fell";
    }
    VERBS.get(v as usize).copied().unwrap_or(VERBS[0])
}

/// The verb for a construct's KO, from what it is (mechanics / element / form).
pub fn verb_for(r: &Recipe) -> u8 {
    let has = |m| r.has(m);
    if crate::signature::orb::is_erasing_orb(r) {
        return 22; // SIGNATURE HOOK
    }
    if has(Mechanic::Explodes) {
        return 8;
    }
    if has(Mechanic::DropFromSky) || has(Mechanic::Heavy) || has(Mechanic::Stomp) || r.form == Form::Anvil {
        return 1;
    }
    if has(Mechanic::Beam) {
        return 12;
    }
    match r.element {
        Element::Electric => return 2,
        Element::Fire | Element::Atomic => return 3,
        Element::Ice => return 4,
        Element::Water => return 13,
        Element::Wind => return 14,
        Element::Poison => return 15,
        Element::Psychic => return 19,
        Element::Shadow => return 20,
        Element::Earth => return 1,
        Element::Light => {}
    }
    if has(Mechanic::Swarm) {
        return 10;
    }
    if has(Mechanic::Grab) || has(Mechanic::Constrict) {
        return 11;
    }
    if has(Mechanic::Pierce) {
        return 6;
    }
    if has(Mechanic::Blade) {
        return 7;
    }
    if has(Mechanic::Launch) || has(Mechanic::Bounce) {
        return 5;
    }
    if has(Mechanic::Charge) || has(Mechanic::Fast) || r.form == Form::Vehicle {
        return 16;
    }
    if r.form.category() == FormCategory::Creature {
        return 9;
    }
    if has(Mechanic::Shoots) {
        return 17;
    }
    0
}

/// A hit kind can override the construct's verb (it exploded / landed on them).
fn verb_with_kind(tag_verb: u8, kind: Option<HitKind>) -> u8 {
    match kind {
        Some(HitKind::Explosion) => 8,
        Some(HitKind::Crush) => 1,
        _ => tag_verb,
    }
}

// ------------------------------------------------------------------------------------ events

/// One kill-feed line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeedLine {
    pub victim: PlayerId,
    pub killer: Option<PlayerId>,
    /// The killer construct's phrase as cast (empty = no construct: ring, splat, void).
    pub phrase: String,
    /// [`VERBS`] index / [`VERB_FELL`].
    pub verb: u8,
    /// The construct was legendary (the feed shows it in gold).
    pub legendary: bool,
}

/// A highlighted phrase.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pick {
    pub player: PlayerId,
    pub phrase: String,
    /// Largest extent (m) — only meaningful for [`Highlights::biggest`].
    pub size_m: f32,
    pub legendary: bool,
}

/// End-of-match highlights (ranked host-side; no scores travel).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Highlights {
    /// Wildest phrases, best first (≤ 3).
    pub wildest: Vec<Pick>,
    /// Funniest KO lines, best first (≤ 3).
    pub ko_lines: Vec<FeedLine>,
    pub biggest: Option<Pick>,
    /// Player with the most unique phrases, and how many.
    pub creative: Option<(PlayerId, u16)>,
}

impl Highlights {
    pub fn is_empty(&self) -> bool {
        self.wildest.is_empty() && self.ko_lines.is_empty() && self.biggest.is_none() && self.creative.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FreedomEvent {
    /// A KO for the kill feed.
    Feed(FeedLine),
    /// A legendary construct just appeared (golden sting + sparkle). Never says why.
    Legendary { id: ConstructId, owner: PlayerId },
    /// Match over: the highlights panel.
    Highlights(Box<Highlights>),
}

/// Kill-feed text. `blocked(word)` = a slur / hate term (the client passes the lexicon's filter):
/// blocked words are dropped; a phrase with nothing allowed left falls back to the killer's name.
/// "Sam was flattened by KING KONG ON A SKATEBOARD" / "Sam was blasted by Jake" / "Sam fell".
pub fn feed_text(line: &FeedLine, victim: &str, killer: Option<&str>, blocked: impl Fn(&str) -> bool) -> String {
    let phrase = filter_phrase(&line.phrase, &blocked);
    let verb = verb_text(line.verb);
    match (phrase, killer) {
        (Some(p), _) => format!("{victim} was {verb} by {}", p.to_uppercase()),
        (None, Some(k)) if line.verb != VERB_FELL => format!("{victim} was {verb} by {k}"),
        _ => format!("{victim} fell"),
    }
}

/// Drop blocked words; `None` if nothing (allowed) remains or the whole phrase is blocked.
pub fn filter_phrase(phrase: &str, blocked: impl Fn(&str) -> bool) -> Option<String> {
    let clean = sanitize_phrase(phrase);
    let mut out = String::with_capacity(clean.len());
    for w in clean.split(' ').filter(|w| !w.is_empty()) {
        let core: String = w.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if !core.is_empty() && blocked(&core) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    let letters: String = out.chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ').collect();
    if letters.trim().is_empty() || blocked(&letters) {
        return None;
    }
    Some(out)
}

// ------------------------------------------------------------------------------------ state

/// A cast in progress.
#[derive(Clone, Debug)]
pub(crate) struct Current {
    player: PlayerId,
    meta: CastMeta,
    key: PhraseKey,
}

/// What the feed remembers about a construct.
#[derive(Clone, Debug)]
struct Tag {
    id: ConstructId,
    owner: PlayerId,
    phrase: String,
    legendary: bool,
    verb: u8,
    /// Hidden novelty score (highlights ranking).
    score: f32,
}

/// Returned by [`spawn_prelude`] for [`after_spawn`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Flavor {
    pub legendary: bool,
    pub score: f32,
}

/// Per-match bookkeeping for the highlights (fixed-size).
#[derive(Clone, Debug, Default)]
struct Book {
    wildest: Vec<(f32, Pick, u64)>,
    lines: Vec<(f32, FeedLine)>,
    biggest: Option<Pick>,
    unique: Vec<(PlayerId, u16)>,
}

impl Book {
    fn offer_wild(&mut self, score: f32, pick: Pick, key: u64) {
        if score <= 0.0 || pick.phrase.is_empty() || self.wildest.iter().any(|(_, _, k)| *k == key) {
            return;
        }
        self.wildest.push((score, pick, key));
        self.wildest.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.wildest.truncate(3);
    }
    fn offer_line(&mut self, score: f32, line: FeedLine) {
        if line.phrase.is_empty() || self.lines.iter().any(|(_, l)| l.phrase.eq_ignore_ascii_case(&line.phrase) && l.victim == line.victim)
        {
            return;
        }
        self.lines.push((score, line));
        self.lines.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.lines.truncate(3);
    }
    fn highlights(&self) -> Highlights {
        let creative = self.unique.iter().copied().filter(|(_, n)| *n > 0).max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)));
        Highlights {
            wildest: self.wildest.iter().map(|(_, p, _)| p.clone()).collect(),
            ko_lines: self.lines.iter().map(|(_, l)| l.clone()).collect(),
            biggest: self.biggest.clone(),
            creative,
        }
    }
}

/// World-side state (`World::freedom`).
#[derive(Clone, Debug, Default)]
pub struct FreedomState {
    /// Cast metas waiting for their cast: (player, meta, tick received).
    pending: Vec<(PlayerId, CastMeta, Tick)>,
    cur: Option<Current>,
    novelty: NoveltySet,
    tags: Vec<Tag>,
    tag_next: usize,
    /// victim → (construct, attacker, hit kind, tick) of the last construct hit.
    last_hit: Vec<(PlayerId, ConstructId, PlayerId, HitKind, Tick)>,
    book: Book,
}

impl FreedomState {
    fn tag(&self, id: ConstructId) -> Option<&Tag> {
        self.tags.iter().find(|t| t.id == id)
    }
    fn push_tag(&mut self, t: Tag) {
        if let Some(old) = self.tags.iter_mut().find(|o| o.id == t.id) {
            *old = t;
        } else if self.tags.len() < TAG_CAP {
            self.tags.push(t);
        } else {
            self.tags[self.tag_next] = t;
            self.tag_next = (self.tag_next + 1) % TAG_CAP;
        }
    }
    fn new_match(&mut self) {
        self.novelty.clear();
        self.book = Book::default();
        self.last_hit.clear();
    }
}

// ------------------------------------------------------------------------------------ world API

impl World {
    /// The client's words / energy for `player`'s next cast (send it right before the cast). Used by
    /// the next `cast*` of that player within [`META_TTL`] ticks; a newer meta replaces an unused one.
    pub fn set_cast_meta(&mut self, player: PlayerId, meta: CastMeta) {
        let meta = meta.sanitized();
        let f = &mut self.freedom;
        f.pending.retain(|(p, _, _)| *p != player);
        if f.pending.len() < 16 {
            f.pending.push((player, meta, self.tick));
        }
    }

    /// Construct presentation bits (`replicate::cflags::LEGENDARY | SPUTTER`), 0 if unknown.
    pub fn construct_flair(&self, id: ConstructId) -> u8 {
        self.constructs.get(id).map_or(0, |c| c.flair)
    }
}

/// Is `id` legendary / sputtering (flags as replicated)?
pub fn is_legendary(w: &World, id: ConstructId) -> bool {
    w.construct_flair(id) & crate::replicate::cflags::LEGENDARY != 0
}
pub fn is_sputtering(w: &World, id: ConstructId) -> bool {
    w.construct_flair(id) & crate::replicate::cflags::SPUTTER != 0
}

// ------------------------------------------------------------------------------------ hooks

/// FREEDOM HOOK (`cast_plain`): pick up the pending meta for this cast. Returns the outer cast's
/// state (a cast inside a cast — powers, party copies) for [`end_cast`] to restore.
pub(crate) fn begin_cast(w: &mut World, player: PlayerId) -> Option<Current> {
    let tick = w.tick;
    let f = &mut w.freedom;
    let meta = match f.pending.iter().position(|(p, _, _)| *p == player) {
        Some(k) => {
            let (_, m, at) = f.pending.swap_remove(k);
            if tick.saturating_sub(at) <= META_TTL { m } else { CastMeta::default() }
        }
        None => match f.cur.as_ref().filter(|c| c.player == player) {
            Some(outer) => outer.meta.clone(),
            None => CastMeta::default(),
        },
    };
    let key = PhraseKey::of(&meta.phrase);
    f.cur.replace(Current { player, meta, key })
}

/// FREEDOM HOOK (`cast_plain`): the cast is over.
pub(crate) fn end_cast(w: &mut World, outer: Option<Current>) {
    w.freedom.cur = outer;
}

fn cur_for(w: &World, player: PlayerId) -> Option<&Current> {
    w.freedom.cur.as_ref().filter(|c| c.player == player)
}

/// FREEDOM HOOK (top of `spawn_cast`, fresh constructs only): shout bonus, hidden legendary roll,
/// novelty / highlights bookkeeping. `recipe` is final (powers / party / signature applied).
pub(crate) fn spawn_prelude(w: &mut World, i: usize, recipe: &mut Recipe, terrain: bool) -> Flavor {
    if w.free_cast || terrain {
        return Flavor::default();
    }
    let id = w.players[i].id;
    // Bots / tools without a meta: the recipe's name stands in for the phrase.
    let (phrase, key, energy) = match cur_for(w, id) {
        Some(c) if !c.meta.phrase.is_empty() => (c.meta.phrase.clone(), c.key.clone(), c.meta.energy),
        Some(c) => (sanitize_phrase(&recipe.name), PhraseKey::of(&recipe.name), c.meta.energy),
        None => (sanitize_phrase(&recipe.name), PhraseKey::of(&recipe.name), 0),
    };
    let t = w.cfg.tuning.freedom;
    let spec = crate::constructs::forms::form_spec(recipe.form);
    let (_, hi) = w.cfg.rules.scale_range(spec.min_scale, spec.max_scale);
    let colossal_before = crate::willpower::is_colossal(recipe);
    // Yelling: up to +15 % scale / intensity (never past the rules' clamp unless already there).
    let b = energy_bonus(energy, &t);
    if b > 0.0 {
        recipe.scale = (recipe.scale * (1.0 + b)).min(hi.max(recipe.scale));
        if recipe.temperament != bd_core::mechanics::Temperament::Calm {
            recipe.intensity = (recipe.intensity + b).min(1.0);
        }
    }
    // Hidden legendary roll.
    let lt = &t.legendary;
    let uses = w.players[i].casts.uses_in(&recipe.name, w.tick, lt.personal_window);
    let n = legendary::novelty(&w.freedom.novelty, &key, recipe, uses, lt);
    let chance = legendary::chance(&n, lt);
    let legendary = legendary::roll(w.cfg.seed, w.tick, id, key.key, chance);
    if !colossal_before && crate::willpower::is_colossal(recipe) {
        recipe.scale = crate::rules::COLOSSAL_SCALE - 0.01; // a bonus never turns a cast into a colossal one
    }
    // Bookkeeping (fixed-size).
    let score = n.score();
    let f = &mut w.freedom;
    let fresh_for_player = legendary::record(&mut f.novelty, &key, id);
    if fresh_for_player && !phrase.is_empty() {
        match f.book.unique.iter().position(|(p, _)| *p == id) {
            Some(k) => f.book.unique[k].1 = f.book.unique[k].1.saturating_add(1),
            None if f.book.unique.len() < 32 => f.book.unique.push((id, 1)),
            None => {}
        }
    }
    let size_m = recipe.scale;
    let pick = Pick { player: id, phrase: phrase.clone(), size_m, legendary };
    if f.book.biggest.as_ref().is_none_or(|b| size_m > b.size_m) && !phrase.is_empty() {
        f.book.biggest = Some(pick.clone());
    }
    f.book.offer_wild(score, pick, key.key);
    Flavor { legendary, score }
}

/// FREEDOM HOOK (`spawn_cast`, after paying): the legendary empowerment — +20 % scale (may pass the
/// form clamp by that much, never into colossal), + intensity. Free: never priced (the roll is hidden,
/// so the cost must not give it away).
pub(crate) fn legendary_boost(w: &World, recipe: &mut Recipe, flavor: Flavor) {
    if !flavor.legendary {
        return;
    }
    let lt = &w.cfg.tuning.freedom.legendary;
    let spec = crate::constructs::forms::form_spec(recipe.form);
    let (_, hi) = w.cfg.rules.scale_range(spec.min_scale, spec.max_scale);
    let colossal_before = crate::willpower::is_colossal(recipe);
    recipe.scale = (recipe.scale * lt.scale).min((hi * lt.scale).max(recipe.scale));
    recipe.intensity = (recipe.intensity + lt.intensity).min(1.0);
    if !colossal_before && crate::willpower::is_colossal(recipe) {
        recipe.scale = crate::rules::COLOSSAL_SCALE - 0.01;
    }
}

/// FREEDOM HOOK (`spawn_cast`, before paying): never refuse for willpower. If `cost` can't be paid,
/// shrink the recipe (count first, then scale, to `sputter_floor` of the asked scale / the form
/// minimum) toward what's left, and charge **everything left**. Returns (cost to pay, sputtering).
pub(crate) fn afford(w: &World, i: usize, recipe: &mut Recipe, cost: f32, price: &dyn Fn(&World, &Recipe) -> f32) -> (f32, bool) {
    if w.cfg.rules.infinite_willpower() || w.free_cast {
        return (cost, false);
    }
    let have = w.players[i].willpower.max(0.0);
    if cost <= have + 1e-4 {
        return (cost, false);
    }
    let spec = crate::constructs::forms::form_spec(recipe.form);
    let (lo, _) = w.cfg.rules.scale_range(spec.min_scale, spec.max_scale);
    let floor = (recipe.scale * w.cfg.tuning.freedom.sputter_floor).max(lo).min(recipe.scale);
    // Count first (fewer copies), then size, a few deterministic steps.
    let mut c = cost;
    for _ in 0..12 {
        if c <= have {
            break;
        }
        if recipe.count > 1 {
            recipe.count = ((recipe.count as f32 * 0.6).floor() as u8).max(1);
        } else if recipe.scale > floor + 1e-4 {
            recipe.scale = (recipe.scale * 0.8).max(floor);
        } else {
            break;
        }
        c = price(w, recipe);
    }
    // Drains what's left (honest: no free spam; at ~0 it is a tiny fizzle).
    (have, true)
}

/// FREEDOM HOOK (`spawn_cast`, after `ConstructSpawned`): flags, legendary durability, feed tag.
pub(crate) fn after_spawn(w: &mut World, i: usize, cid: ConstructId, recipe: &Recipe, flavor: Flavor, sputter: bool) {
    use crate::replicate::cflags;
    let owner = w.players[i].id;
    let lt = w.cfg.tuning.freedom.legendary;
    if let Some(c) = w.constructs.get_mut(cid) {
        if flavor.legendary {
            c.flair |= cflags::LEGENDARY;
            c.integrity *= lt.integrity;
            c.max_integrity = c.max_integrity.max(c.integrity);
        }
        if sputter {
            c.flair |= cflags::SPUTTER;
        }
    }
    if flavor.legendary {
        w.events.push(GameEvent::Freedom(FreedomEvent::Legendary { id: cid, owner }));
    }
    let phrase = match cur_for(w, owner) {
        Some(c) if !c.meta.phrase.is_empty() => c.meta.phrase.clone(),
        _ => sanitize_phrase(&recipe.name),
    };
    w.freedom.push_tag(Tag { id: cid, owner, phrase, legendary: flavor.legendary, verb: verb_for(recipe), score: flavor.score });
}

/// FREEDOM HOOK (`graft`, before paying): an "it" resize from the cast meta. Returns (factor, extra
/// cost) — the growth is priced like buying the bigger construct; clamped by the rules' scale range.
pub(crate) fn graft_resize(w: &World, i: usize, cid: ConstructId) -> (f32, f32) {
    let id = w.players[i].id;
    let Some(want) = cur_for(w, id).map(|c| c.meta.resize).filter(|r| (*r - 1.0).abs() > 1e-3) else { return (1.0, 0.0) };
    let Some(c) = w.constructs.get(cid).filter(|c| c.owner == id) else { return (1.0, 0.0) };
    let t = &w.cfg.tuning.freedom;
    let spec = crate::constructs::forms::form_spec(c.recipe.form);
    let (lo, hi) = w.cfg.rules.scale_range(spec.min_scale, spec.max_scale);
    let cur = (c.recipe.scale * c.size_scale).max(1e-3);
    let f = want.clamp(t.resize_min, t.resize_max).clamp(lo / cur, (hi / cur).max(lo / cur)).clamp(0.25, 4.0);
    if (f - 1.0).abs() < 1e-3 {
        return (1.0, 0.0);
    }
    let wt = &w.cfg.tuning.willpower;
    let mut now = c.recipe.clone();
    now.scale = cur;
    let mut after = now.clone();
    after.scale = cur * f;
    let extra = (wt.base_cost_as(&after, c.priced_autonomous) - wt.base_cost_as(&now, c.priced_autonomous)).max(0.0);
    (f, extra)
}

/// FREEDOM HOOK (`graft` / `animate_terrain`, before paying): an unaffordable graft still applies —
/// it drains what's left. Returns (cost to pay, paid fraction 0..1).
pub(crate) fn drain_afford(w: &World, i: usize, cost: f32) -> (f32, f32) {
    if w.cfg.rules.infinite_willpower() || w.free_cast {
        return (cost, 1.0);
    }
    let have = w.players[i].willpower.max(0.0);
    if cost <= have + 1e-4 || cost <= 0.0 {
        return (cost, 1.0);
    }
    (have, (have / cost).clamp(0.0, 1.0))
}

/// FREEDOM HOOK (`graft`, after the merge): apply the resize; an underpaid graft sputters.
pub(crate) fn graft_after(w: &mut World, cid: ConstructId, factor: f32, paid: f32) {
    if let Some(c) = w.constructs.get_mut(cid) {
        if (factor - 1.0).abs() > 1e-3 {
            c.resize(factor);
        }
        if paid < 0.999 {
            c.flair |= crate::replicate::cflags::SPUTTER;
        }
    }
}

/// FREEDOM HOOK (end of `World::tick`): kill-feed lines, children inherit their parent's phrase,
/// highlights at match end. A scan of this tick's events; allocates only when a KO / end happens.
pub(crate) fn end_of_tick(w: &mut World) {
    let n = w.events.len();
    let mut out: Vec<GameEvent> = Vec::new();
    let credit = (w.cfg.tuning.hit.ko_credit_secs.max(0.0) / bd_core::DT) as Tick;
    for k in 0..n {
        match &w.events[k] {
            GameEvent::MatchPhase { phase: crate::rules::MatchPhase::Live, .. } => w.freedom.new_match(),
            GameEvent::MatchPhase { phase: crate::rules::MatchPhase::Ended, .. } => {
                let h = w.freedom.book.highlights();
                if !h.is_empty() {
                    out.push(GameEvent::Freedom(FreedomEvent::Highlights(Box::new(h))));
                }
            }
            GameEvent::ConstructSpawned { id, .. } => {
                // Swarm copies / payloads / slice halves speak with their parent's words.
                let parent = w.constructs.get(*id).and_then(|c| c.parent);
                if let Some(t) = parent.and_then(|p| w.freedom.tag(p)).filter(|_| w.freedom.tag(*id).is_none()) {
                    let t = Tag { id: *id, ..t.clone() };
                    w.freedom.push_tag(t);
                }
            }
            GameEvent::Hit { target, source: Some(src), construct: Some(cid), kind, .. } => {
                let (target, src, cid, kind, tick) = (*target, *src, *cid, *kind, w.tick);
                let lh = &mut w.freedom.last_hit;
                match lh.iter().position(|e| e.0 == target) {
                    Some(p) => lh[p] = (target, cid, src, kind, tick),
                    None if lh.len() < 32 => lh.push((target, cid, src, kind, tick)),
                    None => {}
                }
            }
            GameEvent::Ko { victim, killer, cause, zone, .. } => {
                let (victim, killer, cause, zone) = (*victim, *killer, *cause, *zone);
                let f = &mut w.freedom;
                let hit = f.last_hit.iter().position(|e| e.0 == victim).map(|p| f.last_hit.swap_remove(p));
                let hit = hit.filter(|h| Some(h.2) == killer && w.tick.saturating_sub(h.4) <= credit);
                let tag = hit.and_then(|h| f.tag(h.1).map(|t| (t.clone(), h.3)));
                let line = match (&tag, killer) {
                    (Some((t, kind)), _) => FeedLine {
                        victim,
                        killer,
                        phrase: t.phrase.clone(),
                        verb: verb_with_kind(t.verb, Some(*kind)),
                        legendary: t.legendary,
                    },
                    (None, Some(_)) => {
                        let verb = if cause == KoCause::BlastZone || zone.is_some() { 21 } else { 18 };
                        FeedLine { victim, killer, phrase: String::new(), verb, legendary: false }
                    }
                    (None, None) => FeedLine { victim, killer: None, phrase: String::new(), verb: VERB_FELL, legendary: false },
                };
                if let Some((t, _)) = &tag {
                    let words = t.phrase.split(' ').filter(|s| !s.is_empty()).count().min(6) as f32;
                    let fun = t.score + 0.4 * t.legendary as u8 as f32 + 0.08 * words + 0.05 * (t.owner != victim) as u8 as f32;
                    f.book.offer_line(fun, line.clone());
                }
                out.push(GameEvent::Freedom(FreedomEvent::Feed(line)));
            }
            _ => {}
        }
    }
    w.events.append(&mut out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_is_clamped_and_voice_equals_typing() {
        let t = FreedomTuning::default();
        assert_eq!(energy_byte(f32::NAN), 0);
        assert_eq!(energy_byte(-5.0), 0);
        assert_eq!(energy_byte(0.35), 0, "calm typing");
        assert_eq!(energy_byte(0.45), 0, "calm talking");
        assert_eq!(energy_byte(1e9), 255);
        // bd_fx::typed_energy("HAMMER!!!") ≈ 0.35 + 0.6 + 0.48 = 1.43; a full yell ≈ 2.2; both clamp maxima.
        assert_eq!(energy_byte(1.43), 255);
        assert_eq!(energy_byte(2.2), 255);
        assert_eq!(energy_byte(2.3), energy_byte(2.6), "typed max == voice max");
        assert!((energy_bonus(255, &t) - 0.15).abs() < 1e-6);
        assert_eq!(energy_bonus(0, &t), 0.0);
        let mut hostile = t;
        hostile.energy_max_bonus = 99.0;
        assert!(hostile.validated().energy_max_bonus <= 0.5);
    }

    #[test]
    fn phrases_are_sanitised() {
        assert_eq!(sanitize_phrase("  KING   KONG\ton a\nskateboard!!!  "), "KING KONG on a skateboard!!!");
        assert_eq!(sanitize_phrase("HAMMERRRRRRRRRRRR"), "HAMMERRRRRR");
        assert!(sanitize_phrase(&"word ".repeat(100)).len() <= PHRASE_MAX);
        assert_eq!(sanitize_phrase("<script>é</script>"), "script script");
        let m = CastMeta { phrase: "x".repeat(900), energy: 7, resize: f32::NAN }.sanitized();
        assert!(m.phrase.len() <= PHRASE_MAX && m.resize == 1.0);
    }

    #[test]
    fn feed_text_and_filter() {
        let line = FeedLine {
            victim: PlayerId(1),
            killer: Some(PlayerId(2)),
            phrase: "King Kong on a skateboard".into(),
            verb: 1,
            legendary: false,
        };
        let none = |_: &str| false;
        assert_eq!(feed_text(&line, "Sam", Some("Jake"), none), "Sam was flattened by KING KONG ON A SKATEBOARD");
        let bad = |w: &str| w.eq_ignore_ascii_case("kong");
        assert_eq!(feed_text(&line, "Sam", Some("Jake"), bad), "Sam was flattened by KING ON A SKATEBOARD");
        let all_bad = |_: &str| true;
        assert_eq!(feed_text(&line, "Sam", Some("Jake"), all_bad), "Sam was flattened by Jake");
        let ring = FeedLine { phrase: String::new(), verb: 18, ..line.clone() };
        assert_eq!(feed_text(&ring, "Sam", Some("Jake"), none), "Sam was blasted by Jake");
        let fell = FeedLine { killer: None, phrase: String::new(), verb: VERB_FELL, ..line };
        assert_eq!(feed_text(&fell, "Sam", None, none), "Sam fell");
        assert_eq!(verb_text(200), "bonked");
    }

    #[test]
    fn verbs_follow_the_construct() {
        let mut r = Recipe::unknown("x");
        r.form = Form::Anvil;
        assert_eq!(verb_text(verb_for(&r)), "flattened");
        r.form = Form::Hammer;
        r.element = Element::Electric;
        assert_eq!(verb_text(verb_for(&r)), "zapped");
        r.element = Element::Light;
        r.mechanics = vec![Mechanic::Pierce];
        assert_eq!(verb_text(verb_for(&r)), "skewered");
        r.mechanics = vec![Mechanic::Pierce, Mechanic::Explodes]; // sorted (Recipe::has binary-searches)
        assert_eq!(verb_text(verb_for(&r)), "blown up");
        r.mechanics = vec![Mechanic::Launch];
        assert_eq!(verb_text(verb_for(&r)), "launched");
        assert_eq!(verb_text(verb_with_kind(5, Some(HitKind::Crush))), "flattened");
    }
}
