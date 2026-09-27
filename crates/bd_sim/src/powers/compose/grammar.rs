//! The describe-a-power GRAMMAR: phrase → [`ComposedMove`] (docs/toolbox/COMPOSED_MOVE.md §Grammar).
//!
//! A phrase is read only when (a) it has EXACTLY ONE self-action verb and (b) EVERY word is
//! understood as one of: verb, style / element, modifier, place ("behind", "through", "onto"…),
//! target ("him", a player name, "it", "the dragon", "everyone", "the wall"), count ("twice",
//! "3 times"), duration ("for 3 seconds"), or filler ("please", "the", "i'll"). Anything else (an
//! unknown noun: "a dragon", "giant sword", "throw a fireball") is NOT a composed move — it stays
//! the lexicon's construct. Word order is free ("behind ethan shadow teleport" reads too): speech
//! and typing both come out in every order.
//!
//! Typed and spoken input take the same path (Whisper output is plain words); `respell` repairs
//! common mishearings, inflections are folded ("teleporting", "slammed", "punches"), one typo is
//! forgiven on long words, and player names are matched exactly, by possessive, joined ("alex99"),
//! by one word of a multi-word name, within one typo — plus whatever sound-alike matcher the game
//! passes ([`Names::fuzzy`], the lexicon's).
//!
//! Pure, deterministic, allocation-light; a parse is a small search over word segmentations with
//! a node budget (never slow, never panics).

use std::collections::HashMap;
use std::sync::OnceLock;

use bd_core::PlayerId;

use super::{ComposedMove, MAX_COUNT, MAX_MODS, MAX_TENTHS, Place, Target, VerbKind, VerbTable, id16};
use crate::powers::norm_words;

/// Other players' display names (+ an optional sound-alike matcher from the game).
#[derive(Clone, Copy, Default)]
pub struct Names<'a> {
    pub players: &'a [(PlayerId, &'a str)],
    /// Extra matcher (voice: sound-alikes): given the words from here on, `(player, words used)`.
    pub fuzzy: Option<&'a dyn Fn(&[&str]) -> Option<(PlayerId, usize)>>,
}

impl<'a> Names<'a> {
    pub fn new(players: &'a [(PlayerId, &'a str)]) -> Self {
        Self { players, fuzzy: None }
    }

    /// A player name starting at `ws[0]`: (player, words consumed).
    pub fn match_at(&self, ws: &[&str]) -> Option<(PlayerId, usize)> {
        if ws.is_empty() {
            return None;
        }
        let w0 = strip_possessive(ws[0]);
        let mut best: Option<(PlayerId, usize, u8)> = None; // (id, used, quality: 0 best)
        let mut offer = |id: PlayerId, used: usize, q: u8| {
            if best.is_none_or(|b| q < b.2 || (q == b.2 && used > b.1)) {
                best = Some((id, used, q));
            }
        };
        for (id, name) in self.players {
            let nw: Vec<String> = norm_words(&name.replace(['_', '.'], " ")).split(' ').filter(|s| !s.is_empty()).map(str::to_string).collect();
            if nw.is_empty() {
                continue;
            }
            let joined: String = nw.concat();
            // the whole name, word by word
            if nw.len() <= ws.len() && nw.iter().enumerate().all(|(k, n)| strip_possessive(ws[k]) == n) {
                offer(*id, nw.len(), 0);
            }
            // joined ("alex99" for "Alex_99", "bigtony")
            if w0 == joined {
                offer(*id, 1, 0);
            }
            // one distinctive word of a multi-word name ("tony" for "Big Tony")
            if nw.len() > 1 && nw.iter().any(|n| n.len() >= 3 && *n == w0) {
                offer(*id, 1, 1);
            }
            // a typo (≥ 4 letters)
            if w0.len() >= 4 && joined.len() >= 4 && within_one(w0, &joined) {
                offer(*id, 1, 2);
            }
        }
        if let Some((id, used, _)) = best {
            return Some((id, used));
        }
        self.fuzzy.and_then(|f| f(ws)).filter(|(_, n)| *n >= 1 && *n <= ws.len())
    }
}

fn strip_possessive(w: &str) -> &str {
    w.strip_suffix("'s").or_else(|| w.strip_suffix("s'")).or_else(|| w.strip_suffix('\'')).unwrap_or(w)
}

/// A successful reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub spec: ComposedMove,
    /// How each word was read ("teleport=verb", "ethan=target"…) — preview / validator wording.
    pub roles: Vec<(String, &'static str)>,
}

impl Reading {
    /// One-line description for tools ("teleport[Teleport] style=shadow place=Behind target=Player(1)").
    pub fn describe(&self) -> String {
        let t = VerbTable::get();
        let s = &self.spec;
        let mut out = format!("{}", t.verb(s.verb).map_or("?".to_string(), |v| format!("{}[{:?}]", v.id, v.kind)));
        if let Some(st) = t.style(s.style) {
            out.push_str(&format!(" style={}", st.id));
        }
        if s.place != Place::None {
            out.push_str(&format!(" place={:?}", s.place));
        }
        if s.target != Target::Aim {
            out.push_str(&format!(" target={:?}", s.target));
        }
        if s.count > 1 {
            out.push_str(&format!(" x{}", s.count));
        }
        if s.secs > 0 {
            out.push_str(&format!(" {:.1}s", s.secs as f32 / 10.0));
        }
        for m in s.mods.iter().filter_map(|id| t.modifier(*id)) {
            out.push_str(&format!(" +{}", m.name));
        }
        out
    }
}

/// Read a phrase with the shipped table.
pub fn read(text: &str, names: &Names) -> Option<Reading> {
    read_with(VerbTable::get(), text, names)
}

// ------------------------------------------------------------------------------------ index

#[derive(Clone, Copy, Debug, PartialEq)]
enum What {
    Filler,
    Place(Place),
    Me,
    Pronoun,
    It,
    Everyone,
    Nearest,
    Terrain,
    /// (verb id, the matched words are a noun too)
    Verb(u16, bool),
    Style(u16),
    Mod(u8),
    Count(u8),
    Number(f32),
    Seconds,
    Times,
    DurLead,
}

struct Entry {
    words: Vec<String>,
    what: What,
}

struct Index {
    /// first word → entries starting with it (longest first)
    by_first: HashMap<String, Vec<usize>>,
    entries: Vec<Entry>,
    /// Single-word verbs / styles ≥ 5 letters, for one-typo matching.
    long_words: Vec<(String, usize)>,
    /// Every word any table knows (a `the <word>` Thing must not be one).
    known: std::collections::HashSet<String>,
    respell: Vec<(Vec<String>, Vec<String>)>,
}

fn index_of(t: &VerbTable) -> Index {
    let mut entries: Vec<Entry> = Vec::new();
    let mut push = |w: &str, what: What| {
        let words: Vec<String> = w.split(' ').filter(|s| !s.is_empty()).map(str::to_string).collect();
        if !words.is_empty() {
            entries.push(Entry { words, what });
        }
    };
    for v in &t.verbs {
        let id = id16(&v.id);
        for w in &v.words {
            push(w, What::Verb(id, v.noun_words.contains(w)));
        }
    }
    for s in &t.styles {
        for w in &s.words {
            push(w, What::Style(id16(&s.id)));
        }
    }
    for p in &t.places {
        for w in &p.words {
            push(w, What::Place(p.place));
        }
    }
    for m in &t.modifiers {
        for w in &m.words {
            push(w, What::Mod(m.id));
        }
    }
    for w in &t.me {
        push(w, What::Me);
    }
    for w in &t.pronouns {
        push(w, What::Pronoun);
    }
    for w in &t.it_words {
        push(w, What::It);
    }
    for w in &t.everyone {
        push(w, What::Everyone);
    }
    for w in &t.nearest {
        push(w, What::Nearest);
    }
    for w in &t.terrain {
        push(w, What::Terrain);
    }
    for w in &t.filler {
        push(w, What::Filler);
    }
    for (w, n) in &t.counts {
        push(w, What::Count(*n));
    }
    for (w, n) in &t.numbers {
        push(w, What::Number(*n));
    }
    for w in &t.seconds {
        push(w, What::Seconds);
    }
    for w in &t.times {
        push(w, What::Times);
    }
    for w in &t.duration_lead {
        push(w, What::DurLead);
    }
    let mut by_first: HashMap<String, Vec<usize>> = HashMap::new();
    let mut known = std::collections::HashSet::new();
    let mut long_words = Vec::new();
    for (k, e) in entries.iter().enumerate() {
        by_first.entry(e.words[0].clone()).or_default().push(k);
        for w in &e.words {
            known.insert(w.clone());
        }
        if e.words.len() == 1 && e.words[0].len() >= 5 && matches!(e.what, What::Verb(..) | What::Style(_)) {
            long_words.push((e.words[0].clone(), k));
        }
    }
    for v in by_first.values_mut() {
        v.sort_by(|a, b| entries[*b].words.len().cmp(&entries[*a].words.len()));
    }
    let mut respell: Vec<(Vec<String>, Vec<String>)> = t
        .respell
        .iter()
        .map(|(a, b)| (a.split(' ').filter(|s| !s.is_empty()).map(str::to_string).collect(), b.split(' ').filter(|s| !s.is_empty()).map(str::to_string).collect()))
        .filter(|(a, _): &(Vec<String>, Vec<String>)| !a.is_empty())
        .collect();
    respell.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    Index { by_first, entries, long_words, known, respell }
}

fn shipped_index() -> &'static Index {
    static IX: OnceLock<Index> = OnceLock::new();
    IX.get_or_init(|| index_of(VerbTable::get()))
}

/// Inflection-folded forms of a word ("teleporting" → teleport, "slammed" → slam, "punches" →
/// punch, "phased" → phase). The word itself first.
fn forms(w: &str) -> [Option<String>; 5] {
    let mut out: [Option<String>; 5] = [Some(w.to_string()), None, None, None, None];
    let mut k = 1;
    let mut add = |s: String, out: &mut [Option<String>; 5]| {
        if k < 5 && !s.is_empty() && out.iter().flatten().all(|o| *o != s) {
            out[k] = Some(s);
            k += 1;
        }
    };
    let undouble = |s: &str| -> Option<String> {
        let b = s.as_bytes();
        (b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2] && !b"aeiouls".contains(&b[b.len() - 1])).then(|| s[..s.len() - 1].to_string())
    };
    if let Some(s) = w.strip_suffix("ing").filter(|s| s.len() >= 2) {
        add(s.to_string(), &mut out);
        add(format!("{s}e"), &mut out);
        if let Some(u) = undouble(s) {
            add(u, &mut out);
        }
    } else if let Some(s) = w.strip_suffix("ed").filter(|s| s.len() >= 2) {
        add(s.to_string(), &mut out);
        add(format!("{s}e"), &mut out);
        if let Some(u) = undouble(s) {
            add(u, &mut out);
        }
    } else if let Some(s) = w.strip_suffix("es").filter(|s| s.len() >= 3 && (s.ends_with("sh") || s.ends_with("ch") || s.ends_with('x') || s.ends_with('s') || s.ends_with('z'))) {
        add(s.to_string(), &mut out);
    } else if let Some(s) = w.strip_suffix("'s").or_else(|| w.strip_suffix('s')).filter(|s| s.len() >= 3 && !s.ends_with('s')) {
        add(s.to_string(), &mut out);
    }
    out
}

fn word_is(input: &str, entry: &str) -> bool {
    input == entry || forms(input).iter().flatten().any(|f| f == entry)
}

/// Split "3s" / "2.5sec" / "3x" into number + unit; keep words otherwise.
fn tokens(text: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().take(400).collect();
    for (k, &c) in chars.iter().enumerate() {
        let digit_dot = c == '.' && k > 0 && chars[k - 1].is_ascii_digit() && chars.get(k + 1).is_some_and(|n| n.is_ascii_digit());
        if c == '\u{2019}' {
            cur.push('\'');
        } else if c.is_alphanumeric() || c == '\'' || digit_dot {
            cur.extend(c.to_lowercase());
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    let mut out = Vec::with_capacity(words.len());
    for w in words {
        let w = w.trim_matches('\'').to_string();
        if w.is_empty() {
            continue;
        }
        let digits = w.chars().take_while(|c| c.is_ascii_digit() || *c == '.').count();
        if digits > 0 && digits < w.len() {
            let (a, b) = w.split_at(digits);
            if matches!(b, "s" | "sec" | "secs" | "second" | "seconds") {
                out.push(a.to_string());
                out.push("seconds".to_string());
                continue;
            }
        }
        out.push(w);
    }
    out
}

fn respell(ix: &Index, words: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut k = 0;
    'w: while k < words.len() {
        for (a, b) in &ix.respell {
            if k + a.len() <= words.len() && a.iter().enumerate().all(|(j, x)| words[k + j] == *x) {
                out.extend(b.iter().cloned());
                k += a.len();
                continue 'w;
            }
        }
        out.push(words[k].clone());
        k += 1;
    }
    out
}

fn number_of(w: &str) -> Option<f32> {
    let v: f32 = w.parse().ok()?;
    (v.is_finite() && v >= 0.0 && v <= 1000.0).then_some(v)
}

/// Within one edit (substitution / insertion / deletion / adjacent swap), ASCII.
pub(crate) fn within_one(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a == b {
        return true;
    }
    if a.len() == b.len() {
        let diff: Vec<usize> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
        return diff.len() == 1 || (diff.len() == 2 && diff[1] == diff[0] + 1 && a[diff[0]] == b[diff[1]] && a[diff[1]] == b[diff[0]]);
    }
    let (s, l) = if a.len() < b.len() { (a, b) } else { (b, a) };
    if l.len() - s.len() != 1 {
        return false;
    }
    let mut i = 0;
    while i < s.len() && s[i] == l[i] {
        i += 1;
    }
    s[i..] == l[i + 1..]
}

// ------------------------------------------------------------------------------------ search

#[derive(Clone, Copy, Debug, PartialEq)]
enum Seg {
    Filler,
    Place(Place),
    Target(Target),
    Verb(u16, bool),
    Style(u16),
    Mod(u8),
    Count(u8),
    Secs(f32),
}

impl Seg {
    fn role(&self) -> &'static str {
        match self {
            Seg::Filler => "filler",
            Seg::Place(_) => "place",
            Seg::Target(_) => "target",
            Seg::Verb(..) => "verb",
            Seg::Style(_) => "style",
            Seg::Mod(_) => "modifier",
            Seg::Count(_) => "count",
            Seg::Secs(_) => "duration",
        }
    }
}

/// Best parse so far: (fillers, segments) — lower is better.
struct Search<'a> {
    ix: &'a Index,
    words: Vec<String>,
    names: &'a Names<'a>,
    budget: u32,
    cur: Vec<(Seg, usize, usize)>,
    best: Option<(u32, u32, Vec<(Seg, usize, usize)>)>,
}

impl Search<'_> {
    /// Entries of the index matching at `i`: (what, words used).
    fn entries_at(&self, i: usize, out: &mut Vec<(What, usize)>) {
        let w = &self.words[i];
        let mut seen: Vec<usize> = Vec::new();
        for f in forms(w).iter().flatten() {
            if let Some(list) = self.ix.by_first.get(f.as_str()) {
                for &k in list {
                    if seen.contains(&k) {
                        continue;
                    }
                    seen.push(k);
                    let e = &self.ix.entries[k];
                    let n = e.words.len();
                    if i + n <= self.words.len() && e.words.iter().enumerate().all(|(j, ew)| if j == 0 { true } else { word_is(&self.words[i + j], ew) }) {
                        out.push((e.what, n));
                    }
                }
            }
        }
        // one typo on a long verb / style word
        if out.iter().all(|(wh, _)| !matches!(wh, What::Verb(..) | What::Style(_))) && w.len() >= 5 {
            for (lw, k) in &self.ix.long_words {
                if within_one(w, lw) {
                    out.push((self.ix.entries[*k].what, 1));
                    break;
                }
            }
        }
        if let Some(v) = number_of(w) {
            out.push((What::Number(v), 1));
        }
    }

    fn run(&mut self, i: usize) {
        if self.budget == 0 {
            return;
        }
        self.budget -= 1;
        if i >= self.words.len() {
            self.finish();
            return;
        }
        let mut here: Vec<(What, usize)> = Vec::with_capacity(6);
        self.entries_at(i, &mut here);
        let prev_place = matches!(self.cur.last(), Some((Seg::Place(_), _, _)));
        let prev_article = i > 0 && matches!(self.words[i - 1].as_str(), "the" | "that" | "this" | "my" | "his" | "her" | "their" | "your" | "those" | "these");
        // player names
        let ws: Vec<&str> = self.words[i..].iter().map(String::as_str).collect();
        if let Some((id, n)) = self.names.match_at(&ws) {
            self.push(Seg::Target(Target::Player(id)), i, n);
        }
        for (what, n) in here.iter().copied() {
            match what {
                What::Filler => self.push(Seg::Filler, i, n),
                What::Place(p) => self.push(Seg::Place(p), i, n),
                What::Me => self.push(Seg::Target(Target::Me), i, n),
                What::Pronoun => self.push(Seg::Target(Target::Pronoun), i, n),
                What::It => self.push(Seg::Target(Target::It), i, n),
                What::Everyone => self.push(Seg::Target(Target::Everyone), i, n),
                What::Nearest => self.push(Seg::Target(Target::Nearest), i, n),
                What::Terrain => self.push(Seg::Target(Target::Ground), i, n),
                What::Verb(id, noun) => self.push(Seg::Verb(id, noun), i, n),
                What::Style(id) => self.push(Seg::Style(id), i, n),
                What::Mod(id) => self.push(Seg::Mod(id), i, n),
                What::Count(c) => self.push(Seg::Count(c), i, n),
                What::Number(v) => {
                    // "3 times" / "three times"
                    let j = i + n;
                    if j < self.words.len() && self.is(j, |w| w == What::Times) && v >= 1.0 && v.fract() == 0.0 {
                        self.push(Seg::Count((v as u32).min(MAX_COUNT as u32) as u8), i, n + 1);
                    }
                    // "3 seconds" / "half a second"
                    if let Some(u) = self.unit_after(j) {
                        self.push(Seg::Secs(v), i, n + u);
                    }
                    // "3 punches": a small integer right before a verb
                    if j < self.words.len() && (2.0..=5.0).contains(&v) && v.fract() == 0.0 && self.is(j, |w| matches!(w, What::Verb(..))) {
                        self.push(Seg::Count(v as u8), i, n);
                    }
                }
                What::DurLead => {
                    // "for 3 seconds", "for a second"
                    let j = i + n;
                    if j < self.words.len() {
                        let mut nums: Vec<(What, usize)> = Vec::new();
                        self.entries_at(j, &mut nums);
                        for (w2, n2) in nums {
                            if let What::Number(v) = w2 {
                                if let Some(u) = self.unit_after(j + n2) {
                                    self.push(Seg::Secs(v), i, n + n2 + u);
                                }
                            }
                        }
                    }
                }
                What::Seconds | What::Times => {}
            }
        }
        // "the dragon", "that wall of ice", "behind dragon": an unknown word names a thing
        let w = &self.words[i];
        if (prev_article || prev_place) && !self.ix.known.contains(w.as_str()) && w.len() >= 2 && number_of(w).is_none() {
            // a filler article before it is folded in by the search (fewer fillers win)
            self.push(Seg::Target(Target::Thing(id16(w))), i, 1);
        }
    }

    fn is(&self, j: usize, f: impl Fn(What) -> bool) -> bool {
        let mut v = Vec::new();
        self.entries_at(j, &mut v);
        v.into_iter().any(|(w, _)| f(w))
    }

    fn unit_after(&self, j: usize) -> Option<usize> {
        if j >= self.words.len() {
            return None;
        }
        let mut v = Vec::new();
        self.entries_at(j, &mut v);
        v.into_iter().find(|(w, _)| *w == What::Seconds).map(|(_, n)| n)
    }

    fn push(&mut self, s: Seg, i: usize, n: usize) {
        if n == 0 || i + n > self.words.len() {
            return;
        }
        // prune: at most one verb
        if matches!(s, Seg::Verb(..)) && self.cur.iter().any(|(c, _, _)| matches!(c, Seg::Verb(..))) {
            return;
        }
        self.cur.push((s, i, n));
        self.run(i + n);
        self.cur.pop();
    }

    fn finish(&mut self) {
        let verbs = self.cur.iter().filter(|(s, _, _)| matches!(s, Seg::Verb(..))).count();
        if verbs != 1 {
            return;
        }
        let fillers: u32 = self.cur.iter().filter(|(s, _, _)| *s == Seg::Filler).map(|(_, _, n)| *n as u32).sum();
        // Things are guesses: they cost like a filler word, so a known reading always wins.
        let things = self.cur.iter().filter(|(s, _, _)| matches!(s, Seg::Target(Target::Thing(_)))).count() as u32;
        let score = (fillers + things, self.cur.len() as u32);
        if self.best.as_ref().is_none_or(|b| (score.0, score.1) < (b.0, b.1)) {
            self.best = Some((score.0, score.1, self.cur.clone()));
        }
    }
}

/// Read a phrase with table `t` (tests / the validator can pass their own).
pub fn read_with(t: &VerbTable, text: &str, names: &Names) -> Option<Reading> {
    let owned;
    let ix: &Index = if std::ptr::eq(t, VerbTable::get()) {
        shipped_index()
    } else {
        owned = index_of(t);
        &owned
    };
    let words = respell(ix, tokens(text));
    if words.is_empty() || words.len() > 20 {
        return None;
    }
    let mut s = Search { ix, words, names, budget: 20_000, cur: Vec::with_capacity(12), best: None };
    s.run(0);
    let (_, _, segs) = s.best?;
    assemble(t, &s.words, &segs)
}

fn assemble(t: &VerbTable, words: &[String], segs: &[(Seg, usize, usize)]) -> Option<Reading> {
    let mut spec = ComposedMove { count: 1, ..Default::default() };
    let mut places: Vec<Place> = Vec::new();
    let mut targets: Vec<Target> = Vec::new();
    let mut mods: Vec<u8> = Vec::new();
    let mut noun = false;
    let mut extras = 0; // anything besides the verb and fillers
    let mut roles = Vec::with_capacity(segs.len());
    for (s, i, n) in segs {
        roles.push((words[*i..*i + *n].join(" "), s.role()));
        match *s {
            Seg::Verb(id, nw) => {
                spec.verb = id;
                noun = nw;
            }
            Seg::Style(id) => {
                extras += 1;
                if spec.style == 0 {
                    spec.style = id;
                }
            }
            Seg::Place(p) => {
                extras += 1;
                places.push(p);
            }
            Seg::Target(tg) => {
                extras += 1;
                targets.push(tg);
            }
            Seg::Mod(m) => {
                extras += 1;
                mods.push(m);
            }
            Seg::Count(c) => {
                extras += 1;
                spec.count = c.clamp(1, MAX_COUNT);
            }
            Seg::Secs(v) => {
                extras += 1;
                spec.secs = (v * 10.0).round().clamp(1.0, MAX_TENTHS as f32) as u8;
            }
            Seg::Filler => {}
        }
    }
    let v = t.verb(spec.verb)?;
    // A noun-verb said bare ("hook", "meteor", "slide") is the THING; "a hook" too.
    if noun && (extras == 0 || words.first().is_some_and(|w| w == "a" || w == "an")) {
        return None;
    }
    // "turn invisible"-style verbs are phrases already; a lone article + noun-ish verb is handled.
    spec.place = pick_place(&places);
    spec.target = pick_target(&targets);
    // "time" as the object of freeze / stop / slow reads as the verb phrase itself (verbs.ron).
    mods.sort_unstable();
    mods.dedup();
    for (k, m) in mods.into_iter().take(MAX_MODS).enumerate() {
        spec.mods[k] = m;
    }
    let _ = v;
    let spec = spec.sanitized_in(t)?;
    Some(Reading { spec, roles })
}

/// Two place words: the specific one wins over a bare direction ("leap up onto the tower").
fn pick_place(p: &[Place]) -> Place {
    let weak = |x: &Place| matches!(x, Place::Up | Place::Down | Place::Forward | Place::Toward);
    p.iter().find(|x| !weak(x)).or(p.first()).copied().unwrap_or(Place::None)
}

/// Two targets: "myself … the wall" keeps Me (pull myself to the wall); otherwise a named /
/// pointed target beats terrain beats me.
fn pick_target(t: &[Target]) -> Target {
    if t.contains(&Target::Me) && t.contains(&Target::Ground) {
        return Target::Me;
    }
    let rank = |x: &Target| match x {
        Target::Player(_) => 0,
        Target::Pronoun | Target::Nearest | Target::Everyone => 1,
        Target::Thing(_) | Target::It => 2,
        Target::Ground => 3,
        Target::Me => 4,
        Target::Same | Target::Aim => 5,
    };
    t.iter().min_by_key(|x| rank(x)).copied().unwrap_or(Target::Aim)
}

/// Kinds whose untargeted link after a targeted link keeps that target in a chain.
pub fn aims_at_target(kind: VerbKind) -> bool {
    use VerbKind::*;
    matches!(kind, Punch | Kick | Uppercut | Slam | Smash | Flurry | Grab | Pull | Push | Lift | Crush | Freeze | Burn | Shock | Poison | Blind | Stun | Dash | Charge | Swap | Slash)
}

/// Chain continuity: "teleport behind ethan, fire uppercut" — an untargeted aimable link after a
/// targeted one aims at the same target ([`Target::Same`]).
pub fn link_targets(steps: &mut [Option<ComposedMove>]) {
    let mut last_explicit = false;
    for s in steps.iter_mut() {
        match s {
            Some(spec) => {
                if spec.target == Target::Aim && last_explicit && spec.kind().is_some_and(aims_at_target) {
                    spec.target = Target::Same;
                }
                last_explicit = spec.target.is_explicit();
            }
            None => last_explicit = false,
        }
    }
}
