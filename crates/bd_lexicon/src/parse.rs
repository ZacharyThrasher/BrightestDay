//! Phrase grammar: repair -> chunk -> segment -> noun phrases -> compose.
//!
//! Tokens are repaired first (repeated letters, dropped g, plurals, compounds/portmanteaus, typos,
//! sound-alikes, adjacent merges), then chunked by longest match against icons, lexicon keys and the
//! modifier tables. Chunks split into segments at connectors ("with", "of", "made of", "that", a verb
//! after the head noun...). Each noun phrase is head-final: the last noun is the head, earlier nouns are
//! adjuncts that graft their mechanics ("scorpion hammer"), adjectives modify ("giant flaming").
//! Composition is order-insensitive: mechanics/features are unions, element conflicts resolve by source
//! priority then enum order, sizes multiply.

use bd_core::mechanics::{Element, Feature, Form, Mechanic, ModelSource, Recipe, Temperament};

use crate::data::{F_ENTITY, F_MOVE, F_PART, F_PURE, F_SPIRIT, NONE8, Rec, fnv32, fnv64};
use crate::fuzzy::{deletes1, dl_distance, edits1, max_typo};
use crate::norm::{Tok, normalize_key, singulars, squeeze2, squeeze3, tokenize};
use crate::phonetic::phonetic_key;
use crate::tables::*;
use crate::{Graft, GraftAction, Lexicon, Resolution, ResolutionKind};

/// Combiner names that "form X" / a bare "X" refer to (GraftAction::Combine).
const COMBINERS: &[&str] =
    &["devastator", "voltron", "bruticus", "superion", "menasor", "predaking", "computron", "defensor", "megazord", "combiner", "gestalt"];

/// Command phrases (already normalised; articles/fillers/pronouns/"make it" removed).
fn action_of(phrase: &str) -> Option<GraftAction> {
    let p = phrase.trim();
    let p =
        p.strip_prefix("autobots ").or_else(|| p.strip_prefix("decepticons ")).or_else(|| p.strip_prefix("constructicons ")).unwrap_or(p);
    match p {
        "roll out"
        | "transform and roll out"
        | "transform roll out"
        | "rollout"
        | "lets roll"
        | "transform all"
        | "everyone transform"
        | "all transform" => Some(GraftAction::RollOut),
        "transform"
        | "transform now"
        | "transform back"
        | "transforms"
        | "transforming"
        | "switch mode"
        | "switch modes"
        | "change mode"
        | "change modes"
        | "robot mode"
        | "vehicle mode"
        | "car mode"
        | "alt mode"
        | "beast mode"
        | "transform into robot"
        | "transform into a robot"
        | "mode switch" => Some(GraftAction::Transform),
        "combine" | "combine them" | "merge" | "merge them" | "unite" | "combine into one" | "combiner" | "combine and form"
        | "gestalt" => Some(GraftAction::Combine),
        _ => {
            let rest = p
                .strip_prefix("form ")
                .or_else(|| p.strip_prefix("combine into "))
                .or_else(|| p.strip_prefix("combine to form "))
                .or_else(|| p.strip_prefix("combine and form "))?;
            COMBINERS.contains(&rest.trim()).then_some(GraftAction::Combine)
        }
    }
}

pub(crate) fn bits_to_mechs(b: u64) -> Vec<Mechanic> {
    ALL_MECHANIC.iter().copied().filter(|m| b & (1u64 << mechanic_index(*m)) != 0).collect()
}
pub(crate) fn bits_to_feats(b: u64) -> Vec<Feature> {
    ALL_FEATURE.iter().copied().filter(|f| b & (1u64 << feature_index(*f)) != 0).collect()
}
fn mbit(m: Mechanic) -> u64 {
    1u64 << mechanic_index(m)
}
fn elem_at(i: u8) -> Option<Element> {
    ALL_ELEMENT.get(i as usize).copied()
}
fn mech_at(i: u8) -> Option<Mechanic> {
    ALL_MECHANIC.get(i as usize).copied()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Gram {
    Article,
    Filler,
    Cast,
    With,
    Of,
    MadeOf,
    Clause,
    And,
    Stop,
    Pronoun,
    MakeIt,
    Copula,
    Not,
}

/// Grammar words (also added to the token vocabulary so they are never "typo-corrected").
#[cfg_attr(not(feature = "builder"), allow(dead_code))]
pub(crate) const GRAMMAR_WORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "some",
    "my",
    "your",
    "his",
    "her",
    "their",
    "our",
    "its",
    "this",
    "these",
    "those",
    "any",
    "another",
    "ur",
    "please",
    "pls",
    "plz",
    "like",
    "just",
    "um",
    "uh",
    "er",
    "erm",
    "yo",
    "hey",
    "okay",
    "ok",
    "now",
    "lol",
    "lmao",
    "literally",
    "basically",
    "actually",
    "also",
    "then",
    "yeah",
    "bro",
    "dude",
    "summon",
    "cast",
    "create",
    "build",
    "make",
    "construct",
    "conjure",
    "spawn",
    "give",
    "gimme",
    "get",
    "generate",
    "manifest",
    "craft",
    "forge",
    "produce",
    "bring",
    "want",
    "need",
    "wanna",
    "gonna",
    "let",
    "lets",
    "with",
    "w",
    "having",
    "has",
    "have",
    "wielding",
    "holding",
    "wearing",
    "carrying",
    "sporting",
    "plus",
    "armed",
    "equipped",
    "covered",
    "filled",
    "full",
    "fitted",
    "loaded",
    "of",
    "made",
    "out",
    "from",
    "built",
    "carved",
    "forged",
    "formed",
    "composed",
    "consisting",
    "that",
    "which",
    "who",
    "whose",
    "can",
    "could",
    "will",
    "would",
    "to",
    "able",
    "and",
    "n",
    "as",
    "well",
    "at",
    "on",
    "onto",
    "into",
    "towards",
    "toward",
    "against",
    "for",
    "it",
    "them",
    "him",
    "me",
    "you",
    "us",
    "i",
    "they",
    "he",
    "she",
    "everyone",
    "everybody",
    "enemy",
    "enemies",
    "opponent",
    "target",
    "is",
    "are",
    "be",
    "being",
    "was",
    "were",
    "gets",
    "becomes",
    "become",
    "goes",
    "moves",
    "looks",
    "seems",
    "feels",
    "acts",
    "not",
    "no",
    "without",
    "never",
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
    "hundred",
    "thousand",
    "million",
    "few",
    "several",
    "many",
    "multiple",
    "lots",
    "tons",
    "loads",
    "double",
    "triple",
    "twin",
    "twins",
    "quad",
    "dual",
    "alive",
    "animate",
];

pub(crate) fn gram_of(s: &str) -> Option<Gram> {
    use Gram::*;
    Some(match s {
        "a" | "an" | "the" | "some" | "my" | "your" | "his" | "her" | "their" | "our" | "its" | "this" | "these" | "those" | "any"
        | "another" | "ur" => Article,
        "please" | "pls" | "plz" | "like" | "just" | "um" | "uh" | "er" | "erm" | "yo" | "hey" | "okay" | "ok" | "now" | "lol" | "lmao"
        | "literally" | "basically" | "actually" | "also" | "then" | "yeah" | "bro" | "dude" | "thing" | "things" | "stuff"
        | "something" | "kind of thing" => Filler,
        "summon" | "cast" | "create" | "build" | "make" | "construct" | "conjure" | "spawn" | "give" | "gimme" | "get" | "generate"
        | "manifest" | "craft" | "forge" | "produce" | "bring" | "want" | "need" | "wanna" | "gonna" | "let" | "lets" | "i want"
        | "i need" | "give me" | "make me" | "i wanna" | "can i have" | "can i get" | "let me have" | "build me" => Cast,
        "with" | "w" | "having" | "has" | "have" | "wielding" | "holding" | "wearing" | "carrying" | "sporting" | "plus" | "armed with"
        | "equipped with" | "covered in" | "covered with" | "filled with" | "full of" | "fitted with" | "loaded with" | "with a"
        | "that has" | "which has" | "who has" | "that have" => With,
        "of" => Of,
        "made of" | "made out of" | "made from" | "built from" | "built of" | "carved from" | "carved out of" | "forged from"
        | "formed of" | "composed of" | "consisting of" | "out of" => MadeOf,
        "that" | "which" | "who" | "whose" | "can" | "could" | "will" | "would" | "to" | "i can" | "you can" | "that can" | "it can"
        | "which can" | "who can" | "that will" | "able to" | "that i can" | "which i can" | "i could" | "we can" | "that could" => Clause,
        "and" | "n" | "as well as" => And,
        "at" | "on" | "onto" | "into" | "towards" | "toward" | "against" | "for" => Stop,
        "it" | "them" | "him" | "me" | "you" | "us" | "i" | "they" | "he" | "she" | "everyone" | "everybody" | "enemy" | "enemies"
        | "opponent" | "target" => Pronoun,
        "make it" | "make them" | "make that" | "give it" | "give them" | "let it" | "let them" | "now make it" | "make this"
        | "make my" | "give that" => MakeIt,
        "is" | "are" | "be" | "being" | "was" | "were" | "gets" | "becomes" | "become" | "goes" | "moves" | "looks" | "seems" | "feels"
        | "acts" | "can be" | "that is" | "which is" | "who is" => Copula,
        "not" | "no" | "without" | "never" => Not,
        _ => return None,
    })
}

fn number_of(s: &str) -> Option<u8> {
    if let Ok(n) = s.parse::<u32>() {
        return Some(n.clamp(1, 255) as u8);
    }
    Some(match s {
        "zero" | "one" | "single" | "lone" => 1,
        "two" | "double" | "twin" | "twins" | "dual" | "both" => 2,
        "three" | "triple" | "few" | "multiple" => 3,
        "four" | "quad" => 4,
        "five" => 5,
        "six" | "many" => 6,
        "seven" => 7,
        "eight" | "lots" | "loads" => 8,
        "nine" => 9,
        "ten" | "tons" => 10,
        "several" => 4,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "hundred" => 100,
        "thousand" | "million" | "billion" | "infinite" => 255,
        _ => return None,
    })
}

/// Idioms that read as adjectives.
fn adj_alias(s: &str) -> Option<&'static str> {
    Some(match s {
        "on fire" | "in flames" | "ablaze" | "alight" | "set on fire" => "flaming",
        "on ice" | "iced over" => "frozen",
        "come alive" | "come to life" | "brought to life" | "alive" => "living",
        "animate" | "animated" => "animated",
        _ => return None,
    })
}

#[derive(Clone, Debug, Default)]
struct Chunk {
    text: String,
    caps: bool,
    icon: Option<usize>,
    entry: Option<u32>,
    plural: bool,
    adj: Option<usize>,
    verb: Option<usize>,
    intens: Option<f32>,
    number: Option<u8>,
    coll: Option<(u8, bool)>,
    gram: Option<Gram>,
    blocked: bool,
}

impl Chunk {
    fn is_noun(&self) -> bool {
        self.icon.is_some() || self.entry.is_some()
    }
    fn is_modifier(&self) -> bool {
        self.adj.is_some() || self.intens.is_some() || self.number.is_some()
    }
}

/// Everything the modifiers of a phrase contribute (order-insensitive accumulation).
#[derive(Clone, Debug)]
struct Mods {
    mech: u64,
    neg_mech: u64,
    feat: u64,
    /// (priority, element index): 3 explicit (adjective/verb/with/of), 2 noun adjunct, 0 weak (mood)
    elems: Vec<(u8, u8)>,
    scale_mul: f32,
    mass_mul: f32,
    abs_size: Option<f32>,
    tempers: Vec<(u8, f32)>,
    count: Option<u8>,
    payload: Option<Box<Recipe>>,
    n: usize,
}

impl Default for Mods {
    fn default() -> Self {
        Mods {
            mech: 0,
            neg_mech: 0,
            feat: 0,
            elems: Vec::new(),
            scale_mul: 1.0,
            mass_mul: 1.0,
            abs_size: None,
            tempers: Vec::new(),
            count: None,
            payload: None,
            n: 0,
        }
    }
}

impl Mods {
    fn element(&self) -> Option<(u8, u8)> {
        // highest priority wins; ties -> lowest enum index (deterministic, order-insensitive)
        self.elems.iter().copied().max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
    }
    fn temper(&self) -> Option<(u8, f32)> {
        self.tempers.iter().copied().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(b.0.cmp(&a.0)))
    }
    fn merge(&mut self, o: &Mods) {
        self.mech |= o.mech;
        self.neg_mech |= o.neg_mech;
        self.feat |= o.feat;
        self.elems.extend(o.elems.iter().copied());
        self.scale_mul *= o.scale_mul;
        self.mass_mul *= o.mass_mul;
        if let Some(a) = o.abs_size {
            self.abs_size = Some(self.abs_size.map_or(a, |b| b.max(a)));
        }
        self.tempers.extend(o.tempers.iter().copied());
        if o.count.is_some() {
            self.count = o.count;
        }
        if o.payload.is_some() {
            self.payload = o.payload.clone();
        }
        self.n += o.n;
    }
}

#[derive(Clone, Copy, Debug)]
enum Head {
    Icon(usize),
    Entry(u32),
}

#[derive(Default)]
struct Np {
    head: Option<(Head, String, bool)>, // (head, text, plural)
    head_caps: bool,
    mods: Mods,
    /// Collective used as the head ("swarm" in "swarm of bees").
    collective: Option<(u8, bool, String)>,
}

pub(crate) struct Cx {
    spoken: bool,
    phrase: String,
    why: Vec<String>,
    ignored: Vec<String>,
    conf_mul: f32,
    unknown: usize,
}

impl Cx {
    fn note(&mut self, s: impl Into<String>) {
        let s = s.into();
        if !s.is_empty() && self.why.len() < 24 && !self.why.contains(&s) {
            self.why.push(s);
        }
    }
}

impl Lexicon {
    // ------------------------------------------------------------------------------------------ lookups
    pub(crate) fn entry(&self, key: &str) -> Option<(u32, Rec)> {
        let d = self.data.as_ref()?;
        let id = d.entry_id(key)?;
        Some((id, d.rec(id)?))
    }

    fn is_blocked(&self, t: &str) -> bool {
        self.t.blocked.contains(t)
    }

    pub(crate) fn known_token(&self, t: &str) -> bool {
        if t.is_empty() {
            return false;
        }
        if self.icons.tokens.contains_key(t) || gram_of(t).is_some() || number_of(t).is_some() {
            return true;
        }
        self.data.as_ref().map(|d| d.token_id(t).is_some()).unwrap_or(false)
    }

    pub(crate) fn token_rank(&self, t: &str) -> u8 {
        let Some(d) = self.data.as_ref() else {
            return 0;
        };
        d.token_id(t).map(|id| d.token(id).1).unwrap_or(0)
    }

    /// Popular exact target for merges / phonetic: icon alias or a pop-culture/popular entry.
    fn popular_key(&self, k: &str) -> bool {
        if self.icons.alias.contains_key(k) {
            return true;
        }
        match self.entry(k) {
            Some((_, r)) => r.rank_f() >= 3.0 || r.flags & (crate::data::F_POP | crate::data::F_POKEMON) != 0,
            None => false,
        }
    }

    // ------------------------------------------------------------------------------------------ repair
    /// Best known token within edit distance of `t` (typed: 1, or 2 for long words).
    fn typo(&self, t: &str, spoken: bool) -> Option<(String, usize, u8)> {
        let d = self.data.as_ref();
        let maxd = max_typo(t.len(), spoken);
        if maxd == 0 || t.len() > 30 {
            return None;
        }
        let mut best: Option<(usize, u8, String)> = None;
        let consider = |cand: &str, dist: usize, rank: u8, best: &mut Option<(usize, u8, String)>| {
            if self.is_blocked(cand) || rank < 20 {
                return;
            }
            let better = match best {
                None => true,
                Some((bd, br, bs)) => (dist, std::cmp::Reverse(rank), cand) < (*bd, std::cmp::Reverse(*br), bs.as_str()),
            };
            if better {
                *best = Some((dist, rank, cand.to_string()));
            }
        };
        for e in edits1(t) {
            if let Some(dd) = d
                && let Some(id) = dd.token_id(&e)
            {
                let (s, r) = dd.token(id);
                consider(s, 1, r, &mut best);
            }
            if self.icons.tokens.contains_key(&e) {
                consider(&e, 1, 200, &mut best);
            }
        }
        if best.is_none() && maxd >= 2 {
            if let Some(dd) = d {
                let mut probes = vec![t.to_string()];
                for d1 in deletes1(t) {
                    probes.extend(deletes1(&d1));
                    probes.push(d1);
                }
                probes.sort();
                probes.dedup();
                let mut ids = Vec::new();
                for p in probes.iter().take(300) {
                    dd.dels_lookup(fnv32(p.as_bytes()), &mut ids);
                    if let Some(id) = dd.token_id(p) {
                        ids.push(id);
                    }
                    if ids.len() > 400 {
                        break;
                    }
                }
                ids.sort_unstable();
                ids.dedup();
                for id in ids.into_iter().take(300) {
                    let (s, r) = dd.token(id);
                    if let Some(dist) = dl_distance(t.as_bytes(), s.as_bytes(), maxd) {
                        consider(s, dist, r, &mut best);
                    }
                }
            }
            let first = t.as_bytes().first().copied();
            for tok in self.icons.tokens.keys() {
                if tok.as_bytes().first().copied() == first
                    && tok.len().abs_diff(t.len()) <= maxd
                    && let Some(dist) = dl_distance(t.as_bytes(), tok.as_bytes(), maxd)
                {
                    consider(tok, dist, 200, &mut best);
                }
            }
        }
        best.map(|(dd, r, s)| (s, dd, r))
    }

    /// Split an unknown token into two known words: "firesword" -> fire sword, "sharknado" -> shark tornado.
    fn compound(&self, t: &str) -> Option<(String, String)> {
        if t.len() < 6 || !t.is_ascii() {
            return None;
        }
        let mut best: Option<(u32, String, String)> = None;
        for i in 3..=t.len().saturating_sub(3) {
            let (l, r) = t.split_at(i);
            if !self.known_token(l) || self.is_blocked(l) {
                continue;
            }
            let lr = self.token_rank(l) as u32;
            if lr < 50 {
                continue; // the left part must be a real, reasonably common word
            }
            let rr = self.token_rank(r) as u32;
            // an inflection is not the second word of a compound ("glimmering" is not "glimmer ing")
            let suffix = matches!(r, "ing" | "ings" | "ed" | "er" | "ers" | "est" | "ly" | "ness" | "ish");
            if self.known_token(r) && !self.is_blocked(r) && rr >= 60 && !suffix {
                let score = 40 + lr + rr;
                if best.as_ref().is_none_or(|b| score > b.0) {
                    best = Some((score, l.to_string(), r.to_string()));
                }
            }
            if r.len() >= 4 {
                // portmanteau: r is the tail of a known word ("nado" <- "tornado")
                if let Some(d) = self.data.as_ref() {
                    let rev: String = r.chars().rev().collect();
                    let mut upper = rev.clone();
                    upper.push('\u{7f}');
                    use fst::{IntoStreamer, Streamer};
                    let mut st = d.rtok.range().ge(rev.as_str()).lt(upper.as_str()).into_stream();
                    let mut n = 0;
                    while let Some((_, id)) = st.next() {
                        n += 1;
                        if n > 200 {
                            break;
                        }
                        let (w, rank) = d.token(id as u32);
                        if w.len() <= r.len() + 1 || rank < 75 {
                            continue;
                        }
                        // shorter overlap = less certain blend
                        let score = lr + rank as u32 - (w.len() - r.len()) as u32 * 8;
                        if best.as_ref().is_none_or(|b| score > b.0) {
                            best = Some((score, l.to_string(), w.to_string()));
                        }
                    }
                }
            }
        }
        best.map(|(_, a, b)| (a, b))
    }

    /// Sound-alike lookup of a (merged) string: -> (key, is_icon, distance, rank)
    fn phonetic_match(&self, s: &str, allow_dist1: bool) -> Option<(String, bool, usize, u32)> {
        self.phonetic_match_opts(s, allow_dist1, false)
    }

    fn phonetic_match_opts(&self, s: &str, allow_dist1: bool, icons_only: bool) -> Option<(String, bool, usize, u32)> {
        let pk = phonetic_key(s);
        if pk.len() < 2 {
            return None;
        }
        let dist1 = allow_dist1 && pk.len() >= 4;
        let q_dels = if dist1 { crate::fuzzy::deletes1(&pk) } else { Vec::new() };
        // entries: exact code, plus (when loose) the query minus one symbol
        let mut keys = vec![(pk.clone(), 0usize)];
        if dist1 && !icons_only {
            keys.extend(q_dels.iter().cloned().map(|k| (k, 1)));
        }
        let mut best: Option<(String, bool, usize, u32)> = None;
        let take = |cand: (String, bool, usize, u32), best: &mut Option<(String, bool, usize, u32)>| {
            let better = match best {
                None => true,
                Some(b) => (cand.2, std::cmp::Reverse(cand.3), &cand.0) < (b.2, std::cmp::Reverse(b.3), &b.0),
            };
            if better {
                *best = Some(cand);
            }
        };
        // icons: symmetric delete (exact, query has one extra symbol, icon has one extra, substitution)
        let mut icon_hits: Vec<(usize, usize)> = Vec::new();
        let ph = self.icons.phon();
        for i in ph.exact.get(&pk).map(|v| v.as_slice()).unwrap_or(&[]) {
            icon_hits.push((*i, 0));
        }
        if dist1 {
            for (i, _) in ph.dels.get(&pk).map(|v| v.as_slice()).unwrap_or(&[]) {
                icon_hits.push((*i, 1));
            }
            for d in &q_dels {
                for i in ph.exact.get(d).map(|v| v.as_slice()).unwrap_or(&[]) {
                    icon_hits.push((*i, 1));
                }
                for (i, full) in ph.dels.get(d).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if full.len() == pk.len() {
                        icon_hits.push((*i, 1));
                    }
                }
            }
        }
        for (i, dist) in icon_hits {
            if let Some(def) = self.icons.defs.get(i) {
                take(
                    (
                        normalize_key(&def.names.first().cloned().unwrap_or_default()),
                        true,
                        dist,
                        if def.file == "icons.ron" { 300 } else { 280 },
                    ),
                    &mut best,
                );
            }
        }
        for (k, dist) in &keys {
            {}
            if let Some(d) = self.data.as_ref().filter(|_| !icons_only) {
                let mut ids = Vec::new();
                d.phon_lookup(fnv32(k.as_bytes()), &mut ids);
                for id in ids {
                    let Some(rec) = d.rec(id) else { continue };
                    // recover the key text (entries are addressed by id; find via display or stream)
                    if let Some(key) = self.key_of(id)
                        && phonetic_key(&key) == *k
                        && !key.split(' ').any(|t| self.is_blocked(t))
                    {
                        let bonus = if rec.flags & (crate::data::F_POKEMON | crate::data::F_ANCHOR) != 0 { 40 } else { 0 };
                        take((key, false, *dist, rec.rank as u32 + bonus), &mut best);
                    }
                }
            }
        }
        best
    }

    /// id -> normalised key (stored in the record).
    fn key_of(&self, id: u32) -> Option<String> {
        let d = self.data.as_ref()?;
        let rec = d.rec(id)?;
        let k = d.pool_str(rec.key);
        (!k.is_empty()).then(|| k.to_string())
    }

    /// Token repair cascade for typed/spoken input.
    fn repair(&self, toks: Vec<Tok>, cx: &mut Cx) -> Vec<Tok> {
        let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
        let mut seen_unknown: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            let s = t.text.as_str();
            if self.is_blocked(s) {
                cx.ignored.push(format!("{s} (blocked)"));
                i += 1;
                continue;
            }
            // adjacent merges into popular names: "god zilla", "pika chu", "iron man" stays (known phrase)
            let mut merged = false;
            for n in [3usize, 2] {
                if i + n > toks.len() || (n == 3 && !cx.spoken) {
                    continue;
                }
                let cat: String = toks[i..i + n].iter().map(|x| x.text.as_str()).collect();
                let spaced: String = toks[i..i + n].iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join(" ");
                if cat.len() < 5 || self.icons.alias.contains_key(&spaced) || self.entry(&spaced).is_some() {
                    continue;
                }
                let any_unknown = toks[i..i + n].iter().any(|x| !self.known_token(&x.text));
                let piece_rank = toks[i..i + n].iter().map(|x| self.token_rank(&x.text)).max().unwrap_or(0);
                let cat_rank = self.entry(&cat).map(|(_, r)| r.rank).unwrap_or(0);
                if self.popular_key(&cat) && (any_unknown || cx.spoken || self.icons.alias.contains_key(&cat) || cat_rank > piece_rank) {
                    cx.note(format!("{spaced} \u{2192} {cat} (merged)"));
                    out.push(Tok { text: cat, caps: toks[i].caps });
                    i += n;
                    merged = true;
                    break;
                }
            }
            if merged {
                continue;
            }
            if self.known_token(s) || s.chars().all(|c| c.is_ascii_digit()) {
                out.push(t.clone());
                i += 1;
                continue;
            }
            // unknown token: cascade (memoised per phrase: "qwxz qwxz qwxz" repairs once)
            if let Some(prev) = seen_unknown.get(s) {
                for w in prev {
                    out.push(Tok { text: w.clone(), caps: t.caps });
                }
                i += 1;
                continue;
            }
            let out_start = out.len();
            let caps = t.caps;
            let push1 = |w: &str, out: &mut Vec<Tok>| out.push(Tok { text: w.to_string(), caps });
            let sq3 = squeeze3(s);
            let sq2 = squeeze2(s);
            let typo = if self.known_token(&sq3) || self.known_token(&sq2) { None } else { self.typo(s, cx.spoken) };
            if sq3 != s && self.known_token(&sq3) {
                push1(&sq3, &mut out);
            } else if sq2 != s && self.known_token(&sq2) {
                push1(&sq2, &mut out);
            } else if s.ends_with("in") && self.known_token(&format!("{s}g")) {
                push1(&format!("{s}g"), &mut out);
            } else if let Some(base) = self.t.infl.get(s).cloned().or_else(|| singulars(s).into_iter().find(|x| self.known_token(x))) {
                // keep the plural surface form; chunking singularises and records plurality
                let _ = base;
                push1(s, &mut out);
            } else if let Some((w, _, _)) = typo.as_ref().filter(|(_, d, r)| *d == 1 && *r >= 60) {
                cx.note(format!("{s} \u{2192} {w} (typo)"));
                cx.conf_mul *= 0.9;
                push1(w, &mut out);
            } else if self.known_stem(s) {
                // an inflected known word ("glimmering", "hammering"): keep it for the leftover pass
                push1(s, &mut out);
            } else if let Some((a, b)) = self.compound(s) {
                cx.note(format!("{s} \u{2192} {a} + {b} (compound)"));
                cx.conf_mul *= 0.85;
                push1(&a, &mut out);
                push1(&b, &mut out);
            } else if let Some((w, dist, _)) = typo.or_else(|| self.typo(&sq2, cx.spoken)) {
                cx.note(format!("{s} \u{2192} {w} (typo)"));
                cx.conf_mul *= if dist == 1 { 0.9 } else { 0.8 };
                push1(&w, &mut out);
            } else if let Some((k, _, dist, _)) = self.phonetic_match(s, cx.spoken).filter(|_| s.len() >= 4) {
                cx.note(format!("{s} \u{2192} {k} (sounds like)"));
                cx.conf_mul *= if dist == 0 { 0.8 } else { 0.7 };
                for w in k.split(' ') {
                    push1(w, &mut out);
                }
            } else {
                push1(s, &mut out); // stays unknown; reported later
            }
            let produced: Vec<String> = out[out_start..].iter().map(|x| x.text.clone()).collect();
            seen_unknown.insert(s.to_string(), produced);
            i += 1;
        }
        out
    }

    // ------------------------------------------------------------------------------------------ chunking
    fn lookup_noun(&self, span: &str) -> (Option<usize>, Option<u32>, bool) {
        if let Some(&i) = self.icons.alias.get(span) {
            return (Some(i), None, false);
        }
        // plural / irregular on the last word
        let (pre, last) = match span.rfind(' ') {
            Some(p) => (&span[..p + 1], &span[p + 1..]),
            None => ("", span),
        };
        let mut cands: Vec<String> = Vec::new();
        if let Some(b) = self.t.infl.get(last) {
            cands.push(b.clone());
        }
        cands.extend(singulars(last));
        if let Some((id, rec)) = self.entry(span) {
            // A plural that is only some obscure name ("sharks" = a gang in West Side Story, "spiders" =
            // a band) reads as the everyday plural when the singular is a hand-anchored / common noun.
            let weak = rec.flags & (F_ENTITY | crate::data::F_POP) != 0
                && rec.flags & (crate::data::F_ANCHOR | crate::data::F_COMPOUND | crate::data::F_POKEMON) == 0;
            if weak {
                for c in &cands {
                    let k = format!("{pre}{c}");
                    if let Some((sid, srec)) = self.entry(&k)
                        && srec.flags & (crate::data::F_ANCHOR | crate::data::F_COMMON) != 0
                    {
                        return (None, Some(sid), true);
                    }
                }
            }
            return (None, Some(id), false);
        }
        for c in cands {
            let k = format!("{pre}{c}");
            if let Some(&i) = self.icons.alias.get(&k) {
                return (Some(i), None, true);
            }
            if let Some((id, _)) = self.entry(&k) {
                return (None, Some(id), true);
            }
        }
        (None, None, false)
    }

    fn chunk(&self, toks: &[Tok]) -> Vec<Chunk> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            let maxn = (toks.len() - i).min(10);
            let mut done = false;
            for n in (1..=maxn).rev() {
                let span: String = toks[i..i + n].iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
                let mut c = Chunk { text: span.clone(), caps: toks[i..i + n].iter().all(|t| t.caps), ..Default::default() };
                if n == 1 && self.is_blocked(&span) {
                    c.blocked = true;
                }
                let alias = adj_alias(&span);
                let akey = alias.unwrap_or(&span);
                c.adj = self.t.adj.get(akey).copied();
                c.verb = self.t.verb.get(&span).copied();
                c.intens = self.t.intens.get(&span).copied();
                c.coll = self.t.coll.get(&span).copied();
                c.gram = gram_of(&span);
                if n == 1 {
                    c.number = number_of(&span);
                }
                if alias.is_none() {
                    let (icon, entry, plural) = self.lookup_noun(&span);
                    c.icon = icon;
                    c.entry = entry;
                    c.plural = plural;
                }
                if n > 1 {
                    // a multi-word *entity* that merely starts with a modifier ("flaming sword" as some
                    // game item) should not swallow compositional meaning: prefer adjective + noun.
                    if c.icon.is_none()
                        && let Some(id) = c.entry
                    {
                        let first = toks[i].text.as_str();
                        let first_mod = self.t.adj.contains_key(first) || self.t.intens.contains_key(first) || number_of(first).is_some();
                        let is_entity = self
                            .data
                            .as_ref()
                            .and_then(|d| d.rec(id))
                            .map(|r| {
                                r.flags & (F_ENTITY | crate::data::F_POP) != 0
                                    && r.flags & (crate::data::F_ANCHOR | crate::data::F_COMPOUND) == 0
                            })
                            .unwrap_or(false);
                        // ...nor one that merely starts with an article ("a whale" the film, "a bomb" = A-bomb)
                        let first_article = matches!(first, "a" | "an" | "the");
                        if (first_mod || first_article) && is_entity {
                            c.entry = None;
                        }
                    }
                    let any = c.icon.is_some()
                        || c.entry.is_some()
                        || c.adj.is_some()
                        || c.verb.is_some()
                        || c.intens.is_some()
                        || c.gram.is_some()
                        || c.coll.is_some();
                    if !any {
                        continue;
                    }
                }
                out.push(c);
                i += n;
                done = true;
                break;
            }
            if !done {
                i += 1;
            }
        }
        out
    }

    // ------------------------------------------------------------------------------------------ modifiers
    fn apply_adj(&self, idx: usize, intensity: Option<f32>, caps: bool, m: &mut Mods, cx: &mut Cx) {
        let Some(a) = self.data.as_ref().and_then(|d| d.meta.adjectives.get(idx)) else {
            return;
        };
        m.mech |= a.mech;
        m.feat |= a.feat;
        if a.element != NONE8 {
            m.elems.push((3, a.element));
        }
        if a.weak_element != NONE8 {
            m.elems.push((0, a.weak_element));
        }
        let k = match intensity {
            Some(i) => (1.0 + (i - 0.6) * 1.2).clamp(0.3, 1.6),
            None => 1.0,
        };
        if (a.scale - 1.0).abs() > 1e-3 {
            m.scale_mul *= a.scale.max(0.01).powf(k);
            m.mass_mul *= a.mass.max(0.0001).powf(k);
        }
        if a.abs > 0.0 {
            m.abs_size = Some(m.abs_size.map_or(a.abs, |b| b.max(a.abs)));
        }
        if a.temper != NONE8 {
            let v = if caps { 1.0 } else { intensity.unwrap_or(0.6) };
            m.tempers.push((a.temper, v));
        }
        m.n += 1;
        cx.conf_mul *= a.conf.clamp(0.5, 1.0).sqrt();
        let mut w = a.why.clone();
        if let Some(i) = intensity {
            w.push_str(&format!(" (intensity {i:.2})"));
        }
        cx.note(w);
    }

    fn apply_verb(&self, idx: usize, m: &mut Mods, cx: &mut Cx) -> Option<(u8, u8)> {
        let v = self.data.as_ref().and_then(|d| d.meta.verbs.get(idx))?;
        m.mech |= v.mech;
        if v.element != NONE8 {
            m.elems.push((3, v.element));
        }
        m.n += 1;
        cx.note(v.why.clone());
        v.payload
    }

    /// A noun used as a modifier: its graft mechanics, graft element and features.
    fn apply_noun_graft(&self, h: Head, prio: u8, m: &mut Mods, cx: &mut Cx) {
        match h {
            Head::Icon(i) => {
                if let Some(d) = self.icons.defs.get(i) {
                    for x in d.graft() {
                        m.mech |= mbit(*x);
                    }
                    for f in &d.recipe().features {
                        m.feat |= 1u64 << feature_index(*f);
                    }
                    if d.recipe().element != Element::Light {
                        m.elems.push((prio, element_index(d.recipe().element)));
                    }
                    cx.note(format!("+{} (graft)", d.recipe().name));
                }
            }
            Head::Entry(id) => {
                let Some(d) = self.data.as_ref() else { return };
                let Some(r) = d.rec(id) else { return };
                m.mech |= r.graft;
                m.feat |= r.feat;
                if r.graft_element != NONE8 {
                    m.elems.push((prio, r.graft_element));
                }
                if r.temper != NONE8 {
                    m.tempers.push((r.temper, 0.6));
                }
                let names: Vec<&str> = bits_to_mechs(r.graft).iter().map(|x| mechanic_name(*x)).collect();
                let k = self.key_of_fast(id, &r);
                cx.note(format!("+{} \u{2192} {} (graft)", k, if names.is_empty() { "look".to_string() } else { names.join("+") }));
            }
        }
        m.n += 1;
    }

    fn key_of_fast(&self, id: u32, _r: &Rec) -> String {
        self.key_of(id).unwrap_or_default()
    }

    fn pure_of(&self, text: &str, head: Option<Head>) -> Option<(u8, u8)> {
        if let Some(p) = self.t.pure.get(text) {
            return Some(*p);
        }
        if let Some(Head::Entry(id)) = head {
            let r = self.data.as_ref()?.rec(id)?;
            if r.flags & F_PURE != 0 {
                return Some((r.element, NONE8));
            }
        }
        None
    }

    // ------------------------------------------------------------------------------------------ noun phrases
    fn np(&self, chunks: &[Chunk], cx: &mut Cx) -> Np {
        self.np_of(chunks, false, cx)
    }

    fn np_of(&self, chunks: &[Chunk], followed_by_of: bool, cx: &mut Cx) -> Np {
        let mut np = Np::default();
        let content: Vec<&Chunk> = chunks
            .iter()
            .filter(|c| {
                !matches!(c.gram, Some(Gram::Article | Gram::Filler | Gram::Pronoun | Gram::Copula | Gram::Cast))
                    || c.is_noun() && c.text.contains(' ')
            })
            .collect();
        // head = last noun; a trailing collective after a noun is a count ("shark swarm")
        let mut head_i: Option<usize> = None;
        let mut coll_after: Option<(u8, bool, String)> = None;
        for (i, c) in content.iter().enumerate().rev() {
            if c.is_noun() && c.number.is_none() {
                if let Some((n, sw)) = c.coll {
                    let earlier_noun = content[..i].iter().any(|x| x.is_noun() && x.coll.is_none() && x.adj.is_none());
                    let last = i + 1 == content.len();
                    if (earlier_noun || (followed_by_of && last)) && coll_after.is_none() {
                        coll_after = Some((n, sw, c.text.clone()));
                        continue;
                    }
                }
                head_i = Some(i);
                break;
            }
            if let Some((n, sw)) = c.coll
                && coll_after.is_none()
                && (followed_by_of || !c.is_noun())
            {
                coll_after = Some((n, sw, c.text.clone()));
                continue;
            }
            if c.adj.is_some() || c.intens.is_some() || c.number.is_some() {
                continue;
            }
        }
        // a lone adjective that is also a noun, last in the phrase, is the head ("fire", "giant")
        let mut pending_int: Option<(f32, bool)> = None;
        for (i, c) in content.iter().enumerate() {
            if Some(i) == head_i {
                let h = if let Some(ic) = c.icon {
                    Head::Icon(ic)
                } else if let Some(id) = c.entry {
                    Head::Entry(id)
                } else {
                    continue;
                };
                if let Some((v, _)) = pending_int.take() {
                    // "very godzilla": intensity with no adjective -> size bump
                    np.mods.scale_mul *= 1.0 + (v - 0.5).max(0.0);
                }
                np.head = Some((h, c.text.clone(), c.plural));
                np.head_caps = c.caps;
                continue;
            }
            if let Some(n) = c.number {
                np.mods.count = Some(n);
                cx.note(format!("count {n} ({})", c.text));
                continue;
            }
            if let Some(v) = c.intens {
                let next_is_mod = content.get(i + 1).map(|x| x.adj.is_some() || x.intens.is_some()).unwrap_or(false);
                if next_is_mod {
                    pending_int = Some((pending_int.map_or(v, |(p, _)| p.max(v)), c.caps));
                    continue;
                }
                if c.adj.is_none() {
                    pending_int = Some((v, c.caps));
                    continue;
                }
            }
            if let Some(a) = c.adj {
                let (inten, caps) = match pending_int.take() {
                    Some((v, pc)) => (Some(if c.caps || pc { 1.0 } else { v }), c.caps || pc),
                    None => (if c.caps { Some(1.0) } else { None }, c.caps),
                };
                self.apply_adj(a, inten, caps, &mut np.mods, cx);
                continue;
            }
            if let Some((n, sw)) = c.coll
                && (head_i.is_none() || Some(i) != head_i)
            {
                np.collective = Some((n, sw, c.text.clone()));
                continue;
            }
            if c.is_noun() {
                let h = if let Some(ic) = c.icon {
                    Head::Icon(ic)
                } else if let Some(id) = c.entry {
                    Head::Entry(id)
                } else {
                    continue;
                };
                if head_i.is_some_and(|hi| i < hi) {
                    // adjunct noun before the head: payload for hosts ("shark cannon"), else a graft
                    let head_text = head_i.and_then(|hi| content.get(hi)).map(|x| x.text.as_str()).unwrap_or("");
                    let head_sing = singulars(head_text).into_iter().next().unwrap_or_default();
                    if self.t.hosts.contains(head_text) || self.t.hosts.contains(head_sing.as_str()) {
                        if let Some((el, pm)) = self.pure_of(&c.text, Some(h)) {
                            if el != NONE8 && el != element_index(Element::Light) {
                                np.mods.elems.push((3, el));
                            }
                            if pm != NONE8 {
                                np.mods.mech |= 1u64 << pm;
                            }
                            cx.note(format!("{} ammo \u{2192} element", c.text));
                        } else if let Some(p) = self.head_recipe(h, &c.text, c.plural) {
                            cx.note(format!("payload: {} ({})", p.name, form_name(p.form)));
                            np.mods.payload = Some(Box::new(p));
                        }
                        continue;
                    }
                    self.apply_noun_graft(h, 2, &mut np.mods, cx);
                    continue;
                }
                // noun after the head (rare): graft
                self.apply_noun_graft(h, 2, &mut np.mods, cx);
                continue;
            }
            if let Some(v) = c.verb {
                let _ = self.apply_verb(v, &mut np.mods, cx);
                continue;
            }
            if c.blocked {
                continue;
            }
            if c.gram.is_none() {
                cx.ignored.push(c.text.clone());
                cx.unknown += 1;
            }
        }
        if let Some((v, caps)) = pending_int {
            // dangling intensifier ("GIANT eeyore SO"): boosts temperament if any
            for t in np.mods.tempers.iter_mut() {
                t.1 = t.1.max(if caps { 1.0 } else { v });
            }
        }
        if let Some((n, sw, txt)) = coll_after {
            if np.head.is_some() {
                np.mods.count = Some(np.mods.count.map_or(n, |c| c.max(n)));
                if sw {
                    np.mods.mech |= mbit(Mechanic::Swarm);
                }
                cx.note(format!("{txt} \u{2192} count {n}"));
            } else {
                np.collective = Some((n, sw, txt));
            }
        }
        np
    }

    /// Recipe for a head noun on its own (no modifiers).
    fn head_recipe(&self, h: Head, text: &str, plural: bool) -> Option<Recipe> {
        let mut r = match h {
            Head::Icon(i) => self.icon_recipe(self.icons.defs.get(i)?),
            Head::Entry(id) => {
                let rec = self.data.as_ref()?.rec(id)?;
                let key = if plural { self.key_of_fast(id, &rec) } else { text.to_string() };
                self.rec_recipe(&key, &rec)
            }
        };
        if plural && r.count <= 1 {
            r.count = 3;
        }
        Some(r)
    }

    // ------------------------------------------------------------------------------------------ top level
    pub(crate) fn resolve_mode(&self, text: &str, spoken: bool) -> Resolution {
        let (toks, bangs) = tokenize(text);
        // stand rush: "ORA ORA ORA", "MUDAMUDAMUDA" — each cry is one punch, not a construct
        let cries: Vec<Option<u16>> = toks.iter().map(|t| crate::powers::rush_count(&t.text)).collect();
        let n_cry_toks = cries.iter().filter(|c| c.is_some()).count();
        let others: Vec<Tok> = toks
            .iter()
            .zip(&cries)
            .filter(|(t, c)| c.is_none() && !matches!(gram_of(&t.text), Some(Gram::Article | Gram::Filler)))
            .map(|(t, _)| t.clone())
            .collect();
        let punches: u16 = cries.iter().flatten().fold(0u16, |a, b| a.saturating_add(*b));
        if n_cry_toks > 0 && (others.is_empty() || n_cry_toks >= 2) {
            let mut res = if others.is_empty() {
                self.rush_resolution(punches)
            } else {
                let mut r = self.resolve_body(others, bangs, spoken);
                r.explanation.insert(0, format!("stand rush \u{00d7}{punches}"));
                r
            };
            res.stand_rush = Some(punches);
            return res;
        }
        self.resolve_body(toks, bangs, spoken)
    }

    fn rush_resolution(&self, punches: u16) -> Resolution {
        let mut recipe = Recipe::unknown(&format!("stand rush \u{00d7}{punches}"));
        recipe.form = Form::Gauntlet;
        recipe.mechanics = vec![Mechanic::Blunt, Mechanic::Fast];
        recipe.scale = 0.6;
        recipe.mass = 5.0;
        recipe.temperament = Temperament::Aggressive;
        recipe.intensity = (0.5 + punches as f32 * 0.05).min(1.0);
        Resolution {
            kind: ResolutionKind::Graft,
            recipe: recipe.normalized(),
            graft: Graft { mechanics: vec![Mechanic::Blunt, Mechanic::Fast], scale_mul: 1.0, ..Default::default() },
            shape: None,
            alt_shape: None,
            incantation: None,
            incantation_powers: Vec::new(),
            stand_rush: Some(punches),
            signature: None,
            confidence: 1.0,
            explanation: vec![format!("stand rush: {punches} punch{}", if punches == 1 { "" } else { "es" })],
            ignored: Vec::new(),
        }
    }

    /// Incantation prefix ("gomu gomu no <phrase>") then the normal (typed/spoken) resolution.
    fn resolve_body(&self, toks: Vec<Tok>, bangs: usize, spoken: bool) -> Resolution {
        // Pokémon trainer grammar: "<pokemon> use <move>", "go <pokemon>", "<pokemon> i choose you"
        if let Some(res) = self.pokemon_command(&toks, bangs, spoken) {
            return res;
        }
        // Signature tier: "domain expansion <rest>", "bankai <rest>", "<rest> domain" (invocation.rs)
        if let Some(res) = self.invocation(&toks, bangs, spoken) {
            return res;
        }
        let skip = toks.iter().take_while(|t| matches!(gram_of(&t.text), Some(Gram::Article | Gram::Filler | Gram::Cast))).count();
        let whole: String = toks[skip.min(toks.len())..].iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
        // the power's own name ("gomu gomu no mi") is not an incantation; a curated icon for the full
        // move ("gomu gomu no pistol") resolves as that icon *and* keeps the incantation tag
        let whole_icon = self.icons.alias.get(&whole).and_then(|&i| self.icons.defs.get(i));
        let whole_is_power_name = whole_icon.map(|d| d.file == "powers.ron").unwrap_or(false);
        let whole_is_icon = whole_icon.is_some();
        for (words, pi) in self.incant.iter().filter(|_| !whole_is_power_name) {
            let n = words.len();
            if toks.len() >= skip + n && toks[skip..skip + n].iter().zip(words).all(|(t, w)| t.text == *w) {
                let Some(p) = self.powers.get(*pi) else {
                    continue;
                };
                let rest: Vec<Tok> = toks[skip + n..].to_vec();
                let mut res = if whole_is_icon {
                    self.resolve_mode_inner(toks.clone(), bangs, spoken)
                } else if rest.is_empty() {
                    let mut r = self.resolve_mode_inner(Vec::new(), bangs, spoken);
                    r.kind = ResolutionKind::Graft;
                    r.confidence = 0.8;
                    r
                } else {
                    self.resolve_mode_inner(rest, bangs, spoken)
                };
                res.incantation = Some(p.id.clone());
                res.incantation_powers =
                    self.incant.iter().filter(|(w, _)| w == words).filter_map(|(_, i)| self.powers.get(*i).map(|p| p.id.clone())).collect();
                res.explanation.insert(0, format!("{} \u{2192} {} incantation", words.join(" "), p.id));
                return res;
            }
        }
        let mut res = self.resolve_mode_inner(toks, bangs, spoken);
        // Signature tier: famous ultimates by name, yelled beams (invocation.rs)
        self.tag_signature(&mut res, &whole, bangs);
        res
    }

    pub(crate) fn resolve_mode_inner(&self, toks: Vec<Tok>, bangs: usize, spoken: bool) -> Resolution {
        let res = self.resolve_tokens(toks.clone(), bangs, spoken, 0);
        if !spoken || toks.is_empty() {
            return res;
        }
        // spoken fallback: a window of words that *sounds like* a popular name ("mule near" -> mjolnir)
        let weak = res.confidence < 0.55 || !res.ignored.is_empty() || res.kind == ResolutionKind::Graft;
        let mut best: Option<(f32, usize, usize, String, bool)> = None;
        if toks.len() > 8 {
            return res; // long utterances are sentences, not misheard names
        }
        for start in 0..toks.len() {
            for len in 1..=4usize.min(toks.len() - start) {
                let cat: String = toks[start..start + len].iter().map(|t| t.text.as_str()).collect();
                if cat.len() < 4 || phonetic_key(&cat).len() < 3 || (!weak && len < 2) {
                    continue;
                }
                // A sound-alike window never starts or ends on a function word ("uh missile", "king kong
                // on a", "at the dragon") and only spans one ("car at zoey", "rain of arrows") when the
                // name it sounds like has that word too (checked below).
                // ("a" may sit inside: whisper splits syllables — "pick a chew" is Pikachu.)
                let edge = |t: &Tok| {
                    matches!(
                        gram_of(&t.text),
                        Some(Gram::Article | Gram::Filler | Gram::Stop | Gram::With | Gram::Of | Gram::And | Gram::Copula | Gram::Cast)
                    )
                };
                let inner = |t: &Tok| matches!(gram_of(&t.text), Some(Gram::Stop | Gram::With | Gram::Of | Gram::And));
                let win = &toks[start..start + len];
                if len > 1 && (edge(&win[0]) || edge(&win[len - 1])) {
                    continue;
                }
                let inner_func: Vec<&str> = win.iter().filter(|t| inner(t)).map(|t| t.text.as_str()).collect();
                let cands =
                    [self.phonetic_match_opts(&cat, true, true), if weak { self.phonetic_match(&cat, cat.len() >= 6) } else { None }];
                for (key, is_icon, dist, rank) in cands.into_iter().flatten() {
                    if !inner_func.iter().all(|f| key.split(' ').any(|k| k == *f)) {
                        continue;
                    }
                    // explaining more of what was heard beats matching a common short word
                    let score = rank as f32 / 25.0 + if is_icon { 3.0 } else { 0.0 } - dist as f32 * 1.5 + len as f32 * 1.5;
                    if (weak || is_icon) && best.as_ref().is_none_or(|b| score > b.0) {
                        best = Some((score, start, len, key, is_icon));
                    }
                }
            }
        }
        if let Some((_, start, len, key, is_icon)) = best {
            let mut t2: Vec<Tok> = toks[..start].to_vec();
            let caps = toks[start].caps;
            t2.extend(key.split(' ').map(|w| Tok { text: w.to_string(), caps }));
            t2.extend(toks[start + len..].iter().cloned());
            let mut alt = self.resolve_tokens(t2, bangs, spoken, 0);
            // a multi-word window that sounds like a curated icon wins over a literal reading
            // ("mule near" -> mjolnir); otherwise only replace weak literal readings
            let icon_window = is_icon && len >= 2 && alt.confidence >= res.confidence * 0.8;
            let better = weak && (alt.confidence >= res.confidence * 0.9 || alt.ignored.len() < res.ignored.len());
            if icon_window || better {
                let heard: Vec<&str> = toks[start..start + len].iter().map(|t| t.text.as_str()).collect();
                alt.explanation.insert(0, format!("\"{}\" sounds like {key}", heard.join(" ")));
                return alt;
            }
        }
        res
    }

    pub(crate) fn resolve_tokens(&self, toks: Vec<Tok>, bangs: usize, spoken: bool, depth: u8) -> Resolution {
        let mut res = self.resolve_tokens_inner(toks, bangs, spoken, depth, true);
        if depth == 0 {
            // leftover content words become light grafts, never vanish silently (relations.rs)
            self.absorb_leftovers(&mut res, bangs, spoken);
        }
        res
    }

    /// `relations`: try the relations layer (relations.rs: sizes, mounts, emitters, volleys) first.
    pub(crate) fn resolve_tokens_inner(&self, toks: Vec<Tok>, bangs: usize, spoken: bool, depth: u8, relations: bool) -> Resolution {
        let mut cx = Cx { spoken, phrase: String::new(), why: Vec::new(), ignored: Vec::new(), conf_mul: 1.0, unknown: 0 };
        // an exact curated/lexicon name is taken as typed (no typo/compound repair of its words)
        let whole = toks.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
        let exact = toks.len() > 1 && (self.icons.alias.contains_key(&whole) || self.entry(&whole).is_some());
        let toks = if exact { toks } else { self.repair(toks, &mut cx) };
        // relations layer (relations.rs); sub-phrases resolve with it off
        if relations
            && depth == 0
            && let Some(mut r) = self.relations(&toks, bangs, spoken)
        {
            r.explanation.splice(0..0, cx.why.drain(..));
            return r;
        }
        cx.phrase = toks
            .iter()
            .filter(|t| !matches!(gram_of(&t.text), Some(Gram::Article | Gram::Filler | Gram::Cast | Gram::MakeIt | Gram::Pronoun)))
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let mut chunks = self.chunk(&toks);
        // a lone word that is both a verb and only a weak noun reads as the verb ("walk", "spin")
        let content: Vec<usize> = (0..chunks.len())
            .filter(|&i| !matches!(chunks[i].gram, Some(Gram::Article | Gram::Filler | Gram::Cast | Gram::Pronoun | Gram::MakeIt)))
            .collect();
        if let [only] = content[..] {
            let c = &mut chunks[only];
            if c.verb.is_some()
                && c.icon.is_none()
                && let Some(rec) = c.entry.and_then(|id| self.data.as_ref().and_then(|d| d.rec(id)))
                && (rec.conf_f() < 0.55 || (rec.flags & F_SPIRIT != 0 && rec.flags & crate::data::F_ANCHOR == 0))
            {
                c.entry = None;
            }
        }
        let phrase = cx.phrase.clone();
        let mut res = self.interpret(&chunks, bangs, &mut cx, depth);
        self.apply_action(&phrase, &mut res);
        res
    }

    /// Transformer command phrases -> graft actions (see `GraftAction`).
    fn apply_action(&self, phrase: &str, res: &mut Resolution) {
        let Some(action) = action_of(phrase) else {
            // a bare combiner name is a construct, with a hint that it can be formed by combining
            if COMBINERS.contains(&phrase) {
                res.graft.action = Some(GraftAction::Combine);
            }
            return;
        };
        res.graft.action = Some(action);
        if !res.graft.mechanics.contains(&Mechanic::Transforms) {
            res.graft.mechanics.push(Mechanic::Transforms);
            res.graft.mechanics.sort();
        }
        res.kind = ResolutionKind::Graft;
        res.explanation.insert(
            0,
            match action {
                GraftAction::Transform => "transform \u{2192} toggle mode / add Transforms (graft)".to_string(),
                GraftAction::RollOut => "roll out \u{2192} every Transforms construct switches mode".to_string(),
                GraftAction::Combine => "combine \u{2192} merge owned transformers into a combiner".to_string(),
            },
        );
        res.confidence = res.confidence.max(0.9);
        if action == GraftAction::Combine {
            // what the combination becomes: a named combiner if given, else Devastator, else a generic giant
            let name = phrase.rsplit(' ').next().filter(|w| COMBINERS.contains(w)).unwrap_or("devastator");
            let giant = self.lookup(name).unwrap_or_else(|| {
                let mut r = Recipe::unknown("combiner");
                r.form = Form::Kaiju;
                r.mechanics = vec![Mechanic::Stomp, Mechanic::Heavy, Mechanic::Transforms];
                r.scale = 20.0;
                r.mass = 50_000.0;
                r.normalized()
            });
            if let Some(&i) = self.icons.alias.get(name)
                && let Some(d) = self.icons.defs.get(i)
            {
                res.shape = d.shape.get().cloned();
                res.alt_shape = d.alt_shape.get().cloned();
            }
            // a combiner is always a Kaiju-scale Transforms giant, whatever the source data says
            let mut giant = giant;
            if giant.form != Form::Kaiju {
                giant.form = Form::Kaiju;
                giant.scale = giant.scale.max(20.0);
                giant.mass = giant.mass.max(50_000.0);
            }
            giant.mechanics.extend([Mechanic::Transforms, Mechanic::Stomp, Mechanic::Heavy]);
            res.recipe = giant.normalized();
        } else if res.recipe.form == Form::Blob0 || res.recipe.name.is_empty() {
            res.recipe.mechanics = vec![Mechanic::Transforms];
        }
    }

    fn interpret(&self, chunks: &[Chunk], bangs: usize, cx: &mut Cx, depth: u8) -> Resolution {
        let mut graft_mode = false;
        let mut lead = Mods::default();
        let mut i = 0;
        // leading articles / cast verbs / graft markers / imperative verbs
        while i < chunks.len() {
            let c = &chunks[i];
            match c.gram {
                Some(Gram::Article | Gram::Filler | Gram::Cast | Gram::Pronoun | Gram::Copula)
                    if !(c.is_noun() && c.text.contains(' ')) =>
                {
                    i += 1;
                    continue;
                }
                Some(Gram::MakeIt) => {
                    graft_mode = true;
                    i += 1;
                    continue;
                }
                Some(Gram::With | Gram::Clause) if i == 0 => {
                    graft_mode = true;
                    break;
                }
                _ => {}
            }
            // imperative verb: "ride a dragon", "drop an anvil", "throw a moon at him"
            if let Some(v) = c.verb {
                let next_is_np =
                    chunks.get(i + 1).map(|n| n.gram == Some(Gram::Article) || n.is_noun() || n.is_modifier()).unwrap_or(false);
                let base_form = self.data.as_ref().and_then(|d| d.meta.verbs.get(v)).map(|vr| vr.k == vr.lemma).unwrap_or(false);
                if next_is_np && base_form && (!c.is_noun() || chunks.get(i + 1).map(|n| n.gram == Some(Gram::Article)).unwrap_or(false)) {
                    let lemma = self.data.as_ref().and_then(|d| d.meta.verbs.get(v)).map(|vr| vr.lemma.clone()).unwrap_or_default();
                    match lemma.as_str() {
                        "throw" | "launch" | "fire" | "shoot" | "hurl" | "toss" | "fling" | "yeet" | "lob" | "chuck" | "summon"
                        | "spawn" | "release" | "unleash" => {
                            cx.note(format!("{} (cast)", c.text));
                        }
                        _ => {
                            if let Some((pm, _)) = self.apply_verb(v, &mut lead, cx) {
                                // "drop X" / "rain X": the construct itself falls
                                if mech_at(pm) == Some(Mechanic::Shoots) && matches!(lemma.as_str(), "drop" | "rain" | "shower") {
                                    lead.mech &= !mbit(Mechanic::Shoots);
                                    lead.mech |= mbit(Mechanic::DropFromSky);
                                }
                            }
                        }
                    }
                    i += 1;
                    continue;
                }
            }
            break;
        }
        // segments
        #[derive(Clone, Copy, PartialEq, Debug)]
        enum Seg {
            Main,
            With,
            Of,
            Made,
            Clause,
        }
        let mut segs: Vec<(Seg, Vec<Chunk>)> = vec![(Seg::Main, Vec::new())];
        let mut seen_noun = false;
        let mut stop = false;
        while i < chunks.len() && !stop {
            let c = &chunks[i];
            let cur = segs.last().map(|s| s.0).unwrap_or(Seg::Main);
            let multi_noun = c.is_noun() && c.text.contains(' ');
            match c.gram {
                Some(Gram::With) if !multi_noun => segs.push((Seg::With, Vec::new())),
                Some(Gram::Of) => segs.push((Seg::Of, Vec::new())),
                Some(Gram::MadeOf) => segs.push((Seg::Made, Vec::new())),
                Some(Gram::Clause) if !multi_noun => segs.push((Seg::Clause, Vec::new())),
                Some(Gram::And) => {
                    if cur == Seg::Main && seen_noun {
                        segs.push((Seg::With, Vec::new()));
                    } else if let Some(s) = segs.last_mut() {
                        s.1.push(c.clone());
                    }
                }
                Some(Gram::Stop) if !multi_noun && c.adj.is_none() => {
                    // "at him", "on the enemy": the rest is targeting, not construct
                    let rest_is_target = chunks[i + 1..].iter().all(|x| {
                        matches!(x.gram, Some(Gram::Pronoun | Gram::Article | Gram::Filler)) || x.text == "guy" || x.text == "player"
                    });
                    if rest_is_target || seen_noun {
                        stop = true;
                    }
                }
                Some(Gram::Not) => {
                    // "without a handle": skip the next noun phrase
                    let mut j = i + 1;
                    while j < chunks.len() && (chunks[j].gram == Some(Gram::Article) || chunks[j].is_modifier()) {
                        j += 1;
                    }
                    if j < chunks.len() {
                        cx.note(format!("ignored: not {}", chunks[j].text));
                    }
                    i = j + 1;
                    continue;
                }
                _ => {
                    // a verb right after the head noun starts a participle clause ("eeyore shooting lasers")
                    let is_verbish = c.verb.is_some() && {
                        let inflected =
                            self.data.as_ref().and_then(|d| d.meta.verbs.get(c.verb.unwrap_or(0))).map(|v| v.k != v.lemma).unwrap_or(false);
                        let payloadish = self
                            .data
                            .as_ref()
                            .and_then(|d| d.meta.verbs.get(c.verb.unwrap_or(0)))
                            .map(|v| v.payload.is_some())
                            .unwrap_or(false);
                        (inflected || (payloadish && !c.is_noun()))
                            && !(c.adj.is_some() && chunks.get(i + 1).map(|n| n.is_noun()).unwrap_or(false))
                    };
                    if cur == Seg::Main && seen_noun && is_verbish && !(c.is_noun() && c.text.contains(' ')) {
                        segs.push((Seg::Clause, vec![c.clone()]));
                    } else if let Some(s) = segs.last_mut() {
                        s.1.push(c.clone());
                    }
                    if c.is_noun() && cur == Seg::Main {
                        seen_noun = true;
                    }
                }
            }
            i += 1;
        }

        // main noun phrase
        let main_chunks = segs.first().map(|s| s.1.clone()).unwrap_or_default();
        let of_next = segs.get(1).map(|s| s.0 == Seg::Of).unwrap_or(false);
        let mut np = self.np_of(&main_chunks, of_next, cx);
        np.mods.merge(&lead);
        let mut of_head_used = false;
        for (kind, cs) in segs.iter().skip(1) {
            match kind {
                Seg::With => {
                    let sub = self.np(cs, cx);
                    let mut m = sub.mods.clone();
                    m.count = None; // "with two heads" doesn't multiply the construct
                    if let Some((h, _, _)) = sub.head {
                        self.apply_noun_graft(h, 3, &mut m, cx);
                        // grafted nouns' elements count as explicit
                    }
                    for e in m.elems.iter_mut() {
                        e.0 = e.0.max(2);
                    }
                    np.mods.merge(&m);
                }
                Seg::Of | Seg::Made => {
                    let sub = self.np(cs, cx);
                    let main_is_collective = np.head.is_none() && (np.collective.is_some() || np.mods.count.is_some());
                    if *kind == Seg::Of && (np.head.is_none() || main_is_collective) && !of_head_used && sub.head.is_some() {
                        // "swarm of bees", "pair of swords", "army of angry skeletons"
                        of_head_used = true;
                        if let Some((n, sw, t)) = np.collective.take() {
                            np.mods.count = Some(np.mods.count.map_or(n, |c| c.max(n)));
                            if sw {
                                np.mods.mech |= mbit(Mechanic::Swarm);
                            }
                            cx.note(format!("{t} of \u{2192} count {n}"));
                        }
                        let mut m = sub.mods.clone();
                        m.merge(&np.mods);
                        np = Np { head: sub.head, head_caps: sub.head_caps, mods: m, collective: None };
                        continue;
                    }
                    let mut m = sub.mods.clone();
                    m.count = None;
                    if let Some((h, text, _)) = sub.head {
                        if let Some((el, pm)) = self.pure_of(&text, Some(h)) {
                            if el != NONE8 {
                                m.elems.push((3, el));
                            }
                            if pm != NONE8 {
                                m.mech |= 1u64 << pm;
                            }
                            cx.note(format!("of {text} \u{2192} element"));
                        } else {
                            // element of the material / owner is explicit here ("axe of ice", "made of stone")
                            if let Head::Entry(id) = h
                                && let Some(r) = self.data.as_ref().and_then(|d| d.rec(id))
                                && r.element != element_index(Element::Light)
                            {
                                m.elems.push((3, r.element));
                            }
                            self.apply_noun_graft(h, 3, &mut m, cx);
                        }
                    }
                    np.mods.merge(&m);
                }
                Seg::Clause => {
                    let m = self.clause(cs, cx, depth);
                    np.mods.merge(&m);
                }
                Seg::Main => {}
            }
        }
        self.compose(np, graft_mode, bangs, cx, depth)
    }

    /// "that returns and explodes", "shooting laser beams", "breathes fire", "i can ride".
    fn clause(&self, cs: &[Chunk], cx: &mut Cx, depth: u8) -> Mods {
        let mut m = Mods::default();
        let mut i = 0;
        let mut pending_int: Option<f32> = None;
        while i < cs.len() {
            let c = &cs[i];
            if matches!(c.gram, Some(Gram::Article | Gram::Filler | Gram::Pronoun | Gram::Copula | Gram::And | Gram::Clause | Gram::Cast))
                && c.verb.is_none()
            {
                i += 1;
                continue;
            }
            if c.gram == Some(Gram::With) {
                // "that has wings"
                let mut j = i + 1;
                while j < cs.len() && cs[j].gram != Some(Gram::And) && !(cs[j].verb.is_some() && !cs[j].is_noun()) {
                    j += 1;
                }
                let sub = self.np(&cs[i + 1..j.min(cs.len())], cx);
                let mut g = sub.mods.clone();
                g.count = None;
                if let Some((h, _, _)) = sub.head {
                    self.apply_noun_graft(h, 3, &mut g, cx);
                }
                m.merge(&g);
                i = j;
                continue;
            }
            if let Some(v) =
                c.verb.filter(|_| !(c.adj.is_some() && c.verb.is_some() && cs.get(i + 1).map(|n| n.is_noun()).unwrap_or(false) && i > 0))
            {
                let payload = self.apply_verb(v, &mut m, cx);
                // object noun phrase up to the next verb / "and"
                let mut j = i + 1;
                while j < cs.len() && cs[j].gram != Some(Gram::And) && !(cs[j].verb.is_some() && !cs[j].is_noun() && cs[j].adj.is_none()) {
                    j += 1;
                }
                let obj = &cs[i + 1..j];
                if !obj.is_empty() {
                    let sub = self.np(obj, cx);
                    if let Some((host_m, pay_m)) = payload {
                        if let Some((h, text, plural)) = sub.head {
                            if let Some((el, pm)) = self.pure_of(&text, Some(h)) {
                                // "shoots fire", "shooting laser beams": the host emits its element
                                if el != NONE8 && el != element_index(Element::Light) {
                                    m.elems.push((3, el));
                                }
                                if pm != NONE8 {
                                    if mech_at(pm) == Some(Mechanic::Beam) && mech_at(host_m) == Some(Mechanic::Shoots) {
                                        m.mech &= !mbit(Mechanic::Shoots);
                                    }
                                    m.mech |= 1u64 << pm;
                                }
                                cx.note(format!("emits {text}"));
                                m.mech |= sub.mods.mech & (mbit(Mechanic::Homing) | mbit(Mechanic::Explodes));
                            } else if depth == 0
                                && let Some(mut p) = self.head_recipe(h, &text, plural)
                            {
                                // payload keeps its own modifiers ("shoots flaming anvils")
                                let pm_mods = sub.mods;
                                self.apply_mods_to(&mut p, &pm_mods, cx, false);
                                if pay_m != NONE8
                                    && let Some(x) = mech_at(pay_m)
                                {
                                    p.mechanics.push(x);
                                }
                                p.payload = None;
                                let p = p.normalized();
                                cx.note(format!("payload: {} ({})", p.name, form_name(p.form)));
                                m.payload = Some(Box::new(p));
                            }
                        } else {
                            // object was only modifiers ("shoots really fast")
                            m.merge(&sub.mods);
                        }
                    } else {
                        // non-payload verb: its object's modifiers still count ("that flies really fast")
                        let mut g = sub.mods.clone();
                        g.count = None;
                        g.payload = None;
                        m.merge(&g);
                    }
                }
                i = j;
                continue;
            }
            if let Some(v) = c.intens {
                pending_int = Some(v);
                i += 1;
                continue;
            }
            if let Some(a) = c.adj {
                self.apply_adj(a, pending_int.take(), c.caps, &mut m, cx);
                i += 1;
                continue;
            }
            if c.is_noun() {
                let h = if let Some(ic) = c.icon {
                    Head::Icon(ic)
                } else if let Some(id) = c.entry {
                    Head::Entry(id)
                } else {
                    i += 1;
                    continue;
                };
                self.apply_noun_graft(h, 3, &mut m, cx);
                i += 1;
                continue;
            }
            if c.gram.is_none() && !c.blocked {
                cx.ignored.push(c.text.clone());
                cx.unknown += 1;
            }
            i += 1;
        }
        m
    }

    fn apply_mods_to(&self, r: &mut Recipe, m: &Mods, _cx: &mut Cx, graft_only: bool) {
        for x in bits_to_mechs(m.mech) {
            r.mechanics.push(x);
        }
        let neg = bits_to_mechs(m.neg_mech);
        r.mechanics.retain(|x| !neg.contains(x));
        for f in bits_to_feats(m.feat) {
            r.features.push(f);
        }
        if let Some((prio, e)) = m.element() {
            // weak (mood) elements only apply when nothing else set one
            if (prio > 0 || r.element == Element::Light)
                && let Some(el) = elem_at(e)
            {
                r.element = el;
            }
        }
        if !graft_only {
            if let Some(a) = m.abs_size {
                let k = (a / r.scale.max(0.01)).max(0.0);
                r.scale = a;
                r.mass *= k.powi(3).clamp(1e-6, 1e9);
            }
            r.scale = (r.scale * m.scale_mul).clamp(0.02, 60.0);
            r.mass = (r.mass * m.mass_mul).clamp(0.001, 1e10);
            if let Some(n) = m.count {
                r.count = n.max(1);
            }
            if m.mech & mbit(Mechanic::Swarm) != 0 && r.count <= 1 {
                r.count = 6;
            }
        }
        if let Some((t, v)) = m.temper()
            && let Some(tp) = ALL_TEMPERAMENT.get(t as usize)
        {
            r.temperament = *tp;
            r.intensity = v.clamp(0.0, 1.0);
        }
        if m.payload.is_some() {
            r.payload = m.payload.clone();
        }
    }

    fn compose(&self, np: Np, graft_mode: bool, bangs: usize, cx: &mut Cx, _depth: u8) -> Resolution {
        let mut mods = np.mods;
        if bangs >= 2 {
            for t in mods.tempers.iter_mut() {
                t.1 = (t.1 + 0.1 * bangs.min(4) as f32).min(1.0);
            }
        }
        let head = np.head.clone();
        let alt_shape = match head {
            Some((Head::Icon(i), _, _)) => self.icons.defs.get(i).and_then(|d| d.alt_shape.get().cloned()),
            _ => None,
        };
        let (mut recipe, shape, head_conf, head_flags, head_graft, head_gelem) = match head.clone() {
            Some((Head::Icon(i), _, plural)) => {
                let d = self.icons.defs.get(i);
                let mut r = d.map(|d| self.icon_recipe(d)).unwrap_or_else(|| Recipe::unknown(""));
                if plural && r.count <= 1 {
                    r.count = 3;
                }
                if let Some(d) = d {
                    cx.why.insert(
                        0,
                        if d.source.is_empty() {
                            format!("{} (icon)", d.recipe().name)
                        } else {
                            format!("{} (icon, {})", d.recipe().name, d.source)
                        },
                    );
                }
                let g = d.map(|d| d.graft().to_vec()).unwrap_or_default();
                let ge = d.map(|d| d.recipe().element).filter(|e| *e != Element::Light);
                (r, d.and_then(|d| d.shape.get().cloned()), 1.0f32, 0u16, g, ge)
            }
            Some((Head::Entry(id), ref text, plural)) => {
                let text = text.clone();
                let rec = self.data.as_ref().and_then(|d| d.rec(id)).unwrap_or_default();
                let mut r = self.head_recipe(Head::Entry(id), &text, plural).unwrap_or_else(|| Recipe::unknown(&text));
                if plural && r.count <= 1 {
                    r.count = 3;
                }
                if let Some(d) = self.data.as_ref() {
                    let key = d.pool_str(rec.key).to_string();
                    let mut lines = vec![d.why_str(rec.why1, &key), d.why_str(rec.why2, &key)];
                    lines.retain(|s| !s.is_empty());
                    for (k, l) in lines.into_iter().enumerate() {
                        cx.why.insert(k, l);
                    }
                }
                (r, None, rec.conf_f(), rec.flags, bits_to_mechs(rec.graft), elem_at(rec.graft_element))
            }
            None => (Recipe::unknown(""), None, 0.0, 0, Vec::new(), None),
        };
        let head_is_part = head_flags & F_PART != 0;
        // a Pokémon move name ("thunderbolt", "vine whip") is a graft: aim it at your own construct
        let head_is_move = head_flags & F_MOVE != 0;
        let move_temp = (head_is_move && recipe.temperament != Temperament::Calm).then_some(recipe.temperament);
        let has_mods = mods.n > 0 || mods.mech != 0 || mods.feat != 0 || !mods.elems.is_empty() || mods.payload.is_some();
        let kind = if graft_mode || head.is_none() || (head_is_part && mods.payload.is_none()) || head_is_move {
            ResolutionKind::Graft
        } else {
            ResolutionKind::Construct
        };
        // graft view (always)
        let mut gm: Vec<Mechanic> = bits_to_mechs(mods.mech);
        if head.is_some() && (kind == ResolutionKind::Graft || head_is_part || true) {
            gm.extend(head_graft.iter().copied());
        }
        gm.sort();
        gm.dedup();
        let mut gfeat = bits_to_feats(mods.feat);
        if head_is_part || kind == ResolutionKind::Graft {
            gfeat.extend(recipe.features.iter().copied());
        }
        gfeat.sort();
        gfeat.dedup();
        let gelem = mods
            .element()
            .filter(|(p, _)| *p > 0)
            .and_then(|(_, e)| elem_at(e))
            .or(head_gelem)
            .or_else(|| mods.element().and_then(|(_, e)| elem_at(e)));
        let gtemp = mods.temper();
        let graft = Graft {
            mechanics: gm,
            element: gelem,
            scale_mul: (mods.scale_mul * mods.abs_size.map(|a| a / recipe.scale.max(0.1)).unwrap_or(1.0)).clamp(0.05, 60.0),
            count: mods.count,
            features: gfeat,
            temperament: gtemp.and_then(|(t, _)| ALL_TEMPERAMENT.get(t as usize).copied()).or(move_temp),
            intensity: gtemp.map(|(_, v)| v).or(move_temp.map(|_| 0.6)),
            payload: mods.payload.clone(),
            action: None,
        };
        // construct recipe
        if head.is_none() {
            recipe.form = Form::Blob0;
        }
        if head_flags & F_SPIRIT != 0 && recipe.temperament == Temperament::Calm && mods.tempers.is_empty() {
            recipe.intensity = 0.0;
        }
        self.apply_mods_to(&mut recipe, &mods, cx, false);
        if np.head_caps && recipe.temperament != Temperament::Calm {
            recipe.intensity = 1.0;
        }
        // name: the head's display name, or the (repaired) phrase when modifiers shape it
        if (has_mods || head.is_none()) && !cx.phrase.is_empty() {
            recipe.name = cx.phrase.clone();
            if recipe.name.len() > 60 {
                let mut e = 60;
                while !recipe.name.is_char_boundary(e) {
                    e -= 1;
                }
                recipe.name.truncate(e);
            }
        }
        if let ModelSource::Procedural { seed } = &mut recipe.model
            && head.is_none()
        {
            *seed = fnv64(format!("{:?}", graft.mechanics).as_bytes()) as u32;
        }
        if recipe.name.trim().is_empty() {
            recipe.name = "light".to_string(); // the ring never says no: an empty cast is a glowing blob
        }
        let recipe = recipe.normalized();
        let mut confidence = match head {
            Some(_) => head_conf.max(0.05) * cx.conf_mul,
            None if has_mods => (0.55 + 0.1 * mods.n.min(4) as f32) * cx.conf_mul,
            None => 0.0,
        };
        confidence *= 0.85f32.powi(cx.unknown.min(8) as i32);
        if head.is_none() && !has_mods {
            confidence = 0.0;
            if !cx.ignored.is_empty() {
                cx.note("unknown word \u{2192} glowing blob");
            }
        }
        Resolution {
            kind,
            recipe,
            graft,
            shape,
            alt_shape,
            incantation: None,
            incantation_powers: Vec::new(),
            stand_rush: None,
            signature: None,
            confidence: confidence.clamp(0.0, 1.0),
            explanation: std::mem::take(&mut cx.why),
            ignored: std::mem::take(&mut cx.ignored),
        }
    }

    // ------------------------------------------------------------------------------------------ completion
    /// Top-8 completions for ghost text. Returned strings are lowercase and extend the (normalised)
    /// prefix; curated icon names keep their punctuation ("captain america's shield").
    pub fn complete(&self, prefix: &str) -> Vec<String> {
        self.complete_n(prefix, 8)
    }

    pub fn complete_n(&self, prefix: &str, k: usize) -> Vec<String> {
        let (toks, _) = tokenize(prefix);
        if toks.is_empty() || prefix.ends_with(char::is_whitespace) || k == 0 {
            return Vec::new();
        }
        let words: Vec<&str> = toks.iter().map(|t| t.text.as_str()).collect();
        let n = words.len();
        let mut cands: Vec<(f32, String)> = Vec::new();
        // the same suggestion can arrive from several spans: keep its best score
        let push = |score: f32, s: String, cands: &mut Vec<(f32, String)>| match cands.iter_mut().find(|c| c.1 == s) {
            Some(c) => c.0 = c.0.max(score),
            None => cands.push((score, s)),
        };
        for j in n.saturating_sub(4)..n {
            let span = words[j..].join(" ");
            let before = words[..j].join(" ");
            let join = |s: &str| {
                if before.is_empty() { s.to_string() } else { format!("{before} {s}") }
            };
            let multi = j + 1 < n;
            for (pretty, pkey, i) in &self.icons.pretty {
                if pkey.starts_with(&span) && pretty.len() > 1 {
                    let bonus = if self
                        .icons
                        .defs
                        .get(*i)
                        .map(|d| d.names.first().map(|n| n.to_lowercase()) == Some(pretty.clone()))
                        .unwrap_or(false)
                    {
                        0.5
                    } else {
                        0.0
                    };
                    // several typed words matching a curated name is a strong signal ("captain am")
                    let multi_word = if span.contains(' ') { 1.5 } else { 0.0 };
                    let base = if self.icons.defs.get(*i).map(|d| d.file == "icons.ron").unwrap_or(false) { 5.5 } else { 4.5 };
                    push(base + bonus + multi_word - pretty.len() as f32 * 0.02, join(pretty), &mut cands);
                }
            }
            if let Some(d) = self.data.as_ref() {
                if !multi && span.len() <= 2 {
                    if let Some((_, ids)) = d.meta.top_prefix.iter().find(|(p, _)| *p == span) {
                        for id in ids.iter().take(16) {
                            if let (Some(key), Some(r)) = (self.key_of(*id), d.rec(*id)) {
                                push(r.rank_f(), join(&key), &mut cands);
                            }
                        }
                    }
                } else if span.len() >= 3 || multi {
                    use fst::{IntoStreamer, Streamer};
                    let mut upper = span.clone();
                    upper.push('\u{7f}');
                    let mut st = d.keys.range().ge(span.as_str()).lt(upper.as_str()).into_stream();
                    let mut scanned = 0;
                    let mut local: Vec<(f32, String)> = Vec::new();
                    while let Some((key, id)) = st.next() {
                        scanned += 1;
                        if scanned > 1500 {
                            break;
                        }
                        let Some(r) = d.rec(id as u32) else { continue };
                        let Ok(ks) = std::str::from_utf8(key) else {
                            continue;
                        };
                        if ks.split(' ').any(|t| self.t.blocked.contains(t)) {
                            continue;
                        }
                        local.push((r.rank_f() - ks.len() as f32 * 0.01, ks.to_string()));
                    }
                    local.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
                    for (s, key) in local.into_iter().take(k * 2) {
                        push(s, join(&key), &mut cands);
                    }
                }
                if !multi {
                    let modifiers = d.meta.adjectives.iter().map(|a| a.k.as_str()).chain(d.meta.intensifiers.iter().map(|x| x.0.as_str()));
                    for w in modifiers {
                        if w.starts_with(span.as_str()) && w.len() > span.len() {
                            push(5.2 - w.len() as f32 * 0.02, join(w), &mut cands);
                        }
                    }
                    for v in d.meta.verbs.iter().filter(|v| v.k == v.lemma) {
                        if v.k.starts_with(span.as_str()) && v.k.len() > span.len() {
                            push(4.0 - v.k.len() as f32 * 0.02, join(&v.k), &mut cands);
                        }
                    }
                }
            }
        }
        cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
        cands.into_iter().map(|c| c.1).filter(|s| s.len() > prefix.trim().len().min(s.len()) || true).take(k).collect()
    }

    /// A deterministic random-but-real phrase for soak tests ("giant flaming hammer with rockets").
    pub fn random_phrase(&self, seed: u64) -> String {
        let mut s = seed ^ 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let pick_noun = |r: u64| -> String {
            if let Some(d) = self.data.as_ref() {
                let n = d.n_entries() as u64;
                if n > 0 {
                    let mut id = (r % n) as u32;
                    for _ in 0..64 {
                        if let Some(rec) = d.rec(id)
                            && rec.rank_f() >= 3.0
                            && let Some(k) = self.key_of_fast_opt(id, &rec)
                        {
                            return k;
                        }
                        id = ((id as u64 * 2_654_435_761 + 97) % n) as u32;
                    }
                }
            }
            self.icons
                .defs
                .get((r % self.icons.defs.len().max(1) as u64) as usize)
                .map(|d| d.recipe().name.to_lowercase())
                .unwrap_or_else(|| "hammer".into())
        };
        let adjs: Vec<&str> = self.data.as_ref().map(|d| d.meta.adjectives.iter().map(|a| a.k.as_str()).collect()).unwrap_or_default();
        let verbs: Vec<&str> = self
            .data
            .as_ref()
            .map(|d| d.meta.verbs.iter().filter(|v| v.k == v.lemma && !v.k.contains(' ')).map(|v| v.k.as_str()).collect())
            .unwrap_or_default();
        let adj = |r: u64| adjs.get((r % adjs.len().max(1) as u64) as usize).copied().unwrap_or("giant");
        let verb = |r: u64| verbs.get((r % verbs.len().max(1) as u64) as usize).copied().unwrap_or("explode");
        match next() % 8 {
            0 => pick_noun(next()),
            1 => format!("{} {}", adj(next()), pick_noun(next())),
            2 => format!("{} {} {}s", 2 + next() % 4, adj(next()), pick_noun(next())),
            3 => format!("{} with {}", pick_noun(next()), pick_noun(next())),
            4 => format!("{} that {}s", pick_noun(next()), verb(next())),
            5 => format!("very {} {} shooting {}", adj(next()), pick_noun(next()), pick_noun(next())),
            6 => format!("make it {}", verb(next())),
            _ => format!("{} {} of {}", adj(next()), pick_noun(next()), pick_noun(next())),
        }
    }

    fn key_of_fast_opt(&self, id: u32, _r: &Rec) -> Option<String> {
        self.key_of(id)
    }
}
