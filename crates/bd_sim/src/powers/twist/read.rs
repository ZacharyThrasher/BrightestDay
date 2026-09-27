//! Reading a twisted / fused move out of a typed or spoken line (pure; voice == typing).
//!
//! * A **core** is whatever already reads as a move: a held power's shout, "<kit> <move>", a
//!   borrowed shout ([`MoveIndex::read`]), a power NAME that is also a shout ("black flash but fire"
//!   is the move, not the technique), or a famous CAST the lexicon knows (the caller's `cast`
//!   predicate: signature names like "spirit bomb", "hadouken").
//! * **Modifiers** around it: prefixes ("giant homing"), suffixes ("x10", "times three", "on fire",
//!   "violently", "in reverse"), a tail after "but" / "with" ("but fire", "but make it giant", "with
//!   extra spin"), numbers ("three rasengans", "a hundred rasengans"), "!!!", and — for kits —
//!   between the name and the move word ("doomfist giant punch").
//! * The FEWEST stripped words win: a phrase that is already a move is never twisted ("giant
//!   rasengan" is Naruto's authored Oodama Rasengan, "big bang attack" a signature), and a move
//!   whose name starts with a modifier word ("black flash", "fire strike") keeps it.
//! * **Fusion**: "A + B", "A & B", "A plus B", "A with B" (B a move), "A fused / combined / merged /
//!   mixed / infused with B", "fuse / combine / merge / a fusion of A and B", "A and B combined /
//!   fused / together", "A x B" (both moves; "goku x luffy" stays a costume mix). Plain "A and B" /
//!   "A then B" stay CHAINS (the toolbox's).

use std::collections::{HashMap, HashSet};

use super::ModTable;
use crate::powers::toolbox::{MoveIndex, Reading};
use crate::powers::{norm_words, PlayerPower, PowerPhrase, PowerTable};

/// What a side's core is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Core {
    /// A held / kit / borrowed move (the toolbox reading).
    Move(Reading),
    /// A famous cast: resolve this phrase with the lexicon (the caller's predicate said yes).
    Cast(String),
}

/// One side: its core, the words of the core, its modifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideReading {
    pub core: Core,
    pub core_text: String,
    pub mods: Vec<u8>,
}

/// A twisted (and maybe fused) reading of a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwistReading {
    pub a: SideReading,
    pub b: Option<SideReading>,
    /// Modifiers on the whole ("!!!").
    pub mods: Vec<u8>,
}

impl TwistReading {
    pub fn fused(&self) -> bool {
        self.b.is_some()
    }
    pub fn all_mods(&self) -> impl Iterator<Item = u8> + '_ {
        self.a.mods.iter().chain(self.b.iter().flat_map(|b| b.mods.iter())).chain(self.mods.iter()).copied()
    }
}

/// The modifier table's word index (built once per table).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WordIndex {
    phrases: HashMap<String, u8>,
    max_len: usize,
    fillers: HashSet<String>,
    articles: HashSet<String>,
    tails: HashSet<String>,
    numbers: HashMap<String, u32>,
    infix: Vec<Vec<String>>,
    prefix: Vec<Vec<String>>,
    joiners: Vec<Vec<String>>,
    suffix: Vec<Vec<String>>,
    cross: HashSet<String>,
}

fn split_words(s: &str) -> Vec<String> {
    s.split(' ').filter(|w| !w.is_empty()).map(String::from).collect()
}

impl WordIndex {
    pub fn build(t: &ModTable) -> Self {
        let mut ix = WordIndex::default();
        for m in &t.modifiers {
            for w in &m.words {
                // first claim wins (data order); the value is the wire byte
                ix.phrases.entry(w.clone()).or_insert(m.n);
                ix.max_len = ix.max_len.max(w.split(' ').count());
            }
        }
        ix.fillers = t.fillers.iter().cloned().collect();
        ix.articles = t.articles.iter().cloned().collect();
        ix.tails = t.tails.iter().cloned().collect();
        ix.numbers = t.numbers.iter().cloned().collect();
        let longest_first = |v: &[String]| {
            let mut out: Vec<Vec<String>> = v.iter().map(|s| split_words(s)).filter(|w| !w.is_empty()).collect();
            out.sort_by(|a, b| b.len().cmp(&a.len()));
            out
        };
        ix.infix = longest_first(&t.fusion.infix);
        ix.prefix = longest_first(&t.fusion.prefix);
        ix.joiners = longest_first(&t.fusion.joiners);
        ix.suffix = longest_first(&t.fusion.suffix);
        ix.cross = t.fusion.cross.iter().cloned().collect();
        ix
    }
}

impl ModTable {
    /// The word index (built on first use).
    pub fn words(&self) -> &WordIndex {
        self.index.get_or_init(|| WordIndex::build(self))
    }
}

/// Lower-case tokens: words (apostrophes kept, hyphens split), digits, and the fusion symbols
/// "+", "&", "×" as their own tokens. `!` are counted.
pub fn tokenize(text: &str) -> (Vec<String>, usize) {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut bangs = 0usize;
    let mut run = 0usize;
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        let w = cur.trim_matches('\'').to_string();
        if !w.is_empty() {
            out.push(w);
        }
        cur.clear();
    };
    for c in text.chars().take(MAX_CHARS) {
        if c == '!' {
            run += 1;
            bangs = bangs.max(run);
        } else {
            run = 0;
        }
        match c {
            '+' | '&' | '×' => {
                flush(&mut cur, &mut out);
                out.push(if c == '×' { "x".to_string() } else { c.to_string() });
            }
            c if c.is_alphanumeric() || c == '\'' => cur.extend(c.to_lowercase()),
            _ => flush(&mut cur, &mut out),
        }
    }
    flush(&mut cur, &mut out);
    out.truncate(MAX_TOKENS);
    (out, bangs)
}

/// Input bounds (like the lexicon's).
pub const MAX_CHARS: usize = 160;
pub const MAX_TOKENS: usize = 24;

/// The reader: tables + who is reading + the famous-cast predicate.
pub struct Reader<'a> {
    pub t: &'a ModTable,
    pub powers: &'a PowerTable,
    pub ix: &'a MoveIndex,
    pub held: Option<&'a PlayerPower>,
    /// Is this phrase a famous cast the lexicon knows (a signature name)? Called last, rarely.
    pub cast: &'a dyn Fn(&str) -> bool,
}

impl<'a> Reader<'a> {
    fn w(&self) -> &WordIndex {
        self.t.words()
    }

    /// The whole reading of a line, or `None` (not a twisted / fused move: the toolbox or the
    /// lexicon reads it as ever).
    pub fn read(&self, text: &str) -> Option<TwistReading> {
        let (toks, bangs) = tokenize(text);
        if toks.is_empty() {
            return None;
        }
        let mut whole = Vec::new();
        if bangs >= self.t.caps.bang as usize && self.t.caps.bang > 0 {
            whole.extend(self.t.bang());
        }
        // Already a move as said: the toolbox's (the authored version wins).
        let plain = toks.join(" ");
        if self.move_core(&plain).is_some() {
            if whole.is_empty() {
                return None;
            }
            let core = self.core_of(&toks)?;
            return Some(TwistReading { a: SideReading { core, core_text: plain, mods: vec![] }, b: None, mods: whole });
        }
        if let Some((a, b)) = self.fusion(&toks) {
            return Some(TwistReading { a, b: Some(b), mods: whole });
        }
        let a = self.side(&toks)?;
        if a.mods.is_empty() && whole.is_empty() {
            return None;
        }
        Some(TwistReading { a, b: None, mods: whole })
    }

    /// A single twisted side (a core + at least one modifier), or `None`.
    pub fn read_side(&self, text: &str) -> Option<SideReading> {
        let (toks, _) = tokenize(text);
        self.side(&toks).filter(|s| !s.mods.is_empty())
    }

    // ------------------------------------------------------------------ cores

    /// A toolbox move (held / kit / borrow, or a grant name that is also a shout).
    fn move_core(&self, text: &str) -> Option<Reading> {
        let n = norm_words(text);
        if n.is_empty() {
            return None;
        }
        match self.ix.read(self.powers, self.held.filter(|h| h.id.is_some()), &n) {
            Reading::Held(PowerPhrase::Move(sel)) => Some(Reading::Held(PowerPhrase::Move(sel))),
            Reading::Held(_) => None,
            r @ (Reading::Kit(_) | Reading::Borrow(_)) => Some(r),
            Reading::Plain => {
                if self.ix.is_grant_name(&n) {
                    self.ix.shout(&n).map(Reading::Borrow)
                } else {
                    None
                }
            }
        }
    }

    fn core_of(&self, toks: &[String]) -> Option<Core> {
        if toks.is_empty() {
            return None;
        }
        let text = toks.join(" ");
        if let Some(r) = self.move_core(&text) {
            return Some(Core::Move(r));
        }
        if (self.cast)(&text) {
            return Some(Core::Cast(text));
        }
        None
    }

    /// `core_of`, trying the singular of a plural last word too ("rasengans", "fireballs").
    fn core_plural(&self, toks: &[String], cheap: bool) -> Option<(Core, Vec<String>)> {
        let mut variants: Vec<Vec<String>> = vec![toks.to_vec()];
        if let Some(last) = toks.last() {
            for s in singulars(last) {
                let mut v = toks.to_vec();
                if let Some(l) = v.last_mut() {
                    *l = s;
                }
                variants.push(v);
            }
        }
        for v in &variants {
            if let Some(r) = self.move_core(&v.join(" ")) {
                return Some((Core::Move(r), v.clone()));
            }
        }
        if cheap {
            return None;
        }
        for v in &variants {
            let text = v.join(" ");
            if (self.cast)(&text) {
                return Some((Core::Cast(text), v.clone()));
            }
        }
        None
    }

    // ------------------------------------------------------------------ modifiers

    /// A number word / digits / "x3" / "3x" ("a hundred" reads through the article).
    fn number(&self, s: &str) -> Option<u32> {
        if let Some(n) = self.w().numbers.get(s) {
            return Some(*n);
        }
        let d = s.strip_prefix('x').or_else(|| s.strip_suffix('x')).unwrap_or(s);
        if !d.is_empty() && d.len() <= 7 && d.chars().all(|c| c.is_ascii_digit()) {
            return d.parse().ok();
        }
        None
    }

    /// Exactly these tokens are one modifier: a data phrase, or a count ("three", "x3", "3x",
    /// "times three", "three times", "x 3").
    fn mod_exact(&self, toks: &[String]) -> Option<u8> {
        if toks.is_empty() {
            return None;
        }
        if let Some(id) = self.w().phrases.get(&toks.join(" ")) {
            return Some(*id);
        }
        let n = match toks {
            [a] => self.number(a),
            [t, a] if t == "times" || t == "x" => self.number(a),
            [a, t] if t == "times" || t == "x" => self.number(a),
            _ => None,
        }?;
        self.t.count_mod(n)
    }

    /// Longest modifier starting at `i`: (id, tokens used).
    fn mod_at(&self, toks: &[String], i: usize) -> Option<(u8, usize)> {
        let max = self.w().max_len.max(2).min(toks.len().saturating_sub(i));
        (1..=max).rev().find_map(|len| self.mod_exact(&toks[i..i + len]).map(|id| (id, len)))
    }

    /// Longest modifier ending just before `end`: (id, start).
    fn mod_ending(&self, toks: &[String], end: usize) -> Option<(u8, usize)> {
        let max = self.w().max_len.max(2);
        (end.saturating_sub(max)..end).find_map(|start| self.mod_exact(&toks[start..end]).map(|id| (id, start)))
    }

    /// Every token of `toks` is a filler or inside a modifier phrase: the ids, else `None`.
    fn all_mods(&self, toks: &[String]) -> Option<Vec<u8>> {
        let w = self.w();
        let mut out = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            if let Some((id, len)) = self.mod_at(toks, i) {
                out.push(id);
                i += len;
            } else if w.fillers.contains(&toks[i]) || w.articles.contains(&toks[i]) {
                i += 1;
            } else {
                return None;
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// One side: `[articles] [prefix mods] CORE [suffix mods] [but|with <mods>]`.
    fn side(&self, toks: &[String]) -> Option<SideReading> {
        let w = self.w();
        let mut toks: &[String] = toks;
        while toks.len() > 1 && w.articles.contains(&toks[0]) {
            toks = &toks[1..];
        }
        if toks.is_empty() {
            return None;
        }
        // tail: the last "but" / "with" whose remainder is only modifiers
        let mut tail: Vec<u8> = Vec::new();
        let mut end = toks.len();
        for p in (1..toks.len()).rev() {
            if w.tails.contains(&toks[p])
                && let Some(ids) = self.all_mods(&toks[p + 1..])
            {
                tail = ids;
                end = p;
                break;
            }
        }
        // suffix spans (from the end; fillers between them skipped: "x10 and violently"):
        // (id, start) — suffix j strips spans[..j], the core ends at the last one's start
        let mut sfx: Vec<(u8, usize)> = Vec::new();
        {
            let mut e = end;
            loop {
                let mut e2 = e;
                if !sfx.is_empty() {
                    while e2 > 1 && w.fillers.contains(&toks[e2 - 1]) {
                        e2 -= 1;
                    }
                }
                match self.mod_ending(toks, e2) {
                    Some((id, start)) if start >= 1 => {
                        sfx.push((id, start));
                        e = start;
                    }
                    _ => break,
                }
            }
        }
        // prefix spans (greedy, longest phrase first; fillers like "very" skipped)
        let mut spans: Vec<(u8, usize, usize)> = Vec::new(); // (id, start, len)
        let mut i = 0;
        while i < end {
            if let Some((id, len)) = self.mod_at(toks, i) {
                if i + len > end {
                    break;
                }
                spans.push((id, i, len));
                i += len;
            } else if w.fillers.contains(&toks[i]) && !spans.is_empty() {
                i += 1;
            } else {
                break;
            }
        }
        // every (prefix k, suffix j) split, fewest stripped first (suffix words kept first: they
        // are more often part of a name — "hollow purple", "spirit bomb", "black flash")
        let mut combos: Vec<(usize, usize)> = Vec::new();
        for k in 0..=spans.len() {
            for j in 0..=sfx.len() {
                combos.push((k, j));
            }
        }
        combos.sort_by_key(|(k, j)| (k + j, *j, *k));
        // the cheap (toolbox) cores before the lexicon predicate
        for cheap in [true, false] {
            for &(k, j) in &combos {
                let start = if k == 0 { 0 } else { spans[k - 1].1 + spans[k - 1].2 };
                let stop = if j == 0 { end } else { sfx[j - 1].1 };
                if start >= stop {
                    continue;
                }
                let core = &toks[start..stop];
                if let Some((c, words)) = self.core_plural(core, cheap) {
                    let mut mods: Vec<u8> = spans[..k].iter().map(|s| s.0).collect();
                    mods.extend(sfx[..j].iter().rev().map(|s| s.0));
                    mods.extend(tail.iter().copied());
                    return Some(SideReading { core: c, core_text: words.join(" "), mods });
                }
            }
            let post: Vec<u8> = sfx.iter().rev().map(|s| s.0).chain(tail.iter().copied()).collect();
            let end = sfx.last().map_or(end, |s| s.1);
            // kit phrases with the modifier inside: "doomfist giant punch"
            let start = spans.last().map_or(0, |s| s.1 + s.2);
            if end > start + 2 {
                let core = &toks[start..end];
                let mut kept: Vec<String> = Vec::new();
                let mut inner: Vec<u8> = Vec::new();
                let mut j = 1;
                kept.push(core[0].clone());
                while j < core.len() {
                    match self.mod_at(core, j) {
                        Some((id, len)) if j + len < core.len() => {
                            inner.push(id);
                            j += len;
                        }
                        _ => {
                            kept.push(core[j].clone());
                            j += 1;
                        }
                    }
                }
                if !inner.is_empty()
                    && let Some((c, words)) = self.core_plural(&kept, cheap)
                {
                    let mut mods: Vec<u8> = spans.iter().map(|s| s.0).collect();
                    mods.extend(inner);
                    mods.extend(post.iter().copied());
                    return Some(SideReading { core: c, core_text: words.join(" "), mods });
                }
            }
        }
        None
    }

    // ------------------------------------------------------------------ fusion

    fn both(&self, a: &[String], b: &[String], moves_only: bool) -> Option<(SideReading, SideReading)> {
        if a.is_empty() || b.is_empty() {
            return None;
        }
        let sa = self.side_any(a)?;
        let sb = self.side_any(b)?;
        if moves_only && !(matches!(sa.core, Core::Move(_)) && matches!(sb.core, Core::Move(_))) {
            return None;
        }
        Some((sa, sb))
    }

    /// A side with or without modifiers.
    fn side_any(&self, toks: &[String]) -> Option<SideReading> {
        let w = self.w();
        let mut t: &[String] = toks;
        while t.len() > 1 && w.articles.contains(&t[0]) {
            t = &t[1..];
        }
        if let Some((c, words)) = self.core_plural(t, false) {
            return Some(SideReading { core: c, core_text: words.join(" "), mods: vec![] });
        }
        self.side(t)
    }

    fn find(toks: &[String], pat: &[String], from: usize) -> Option<usize> {
        if pat.is_empty() || toks.len() < pat.len() {
            return None;
        }
        (from..=toks.len() - pat.len()).find(|&i| toks[i..i + pat.len()] == *pat)
    }

    /// "A + B", "fuse A and B", "A and B combined", "A x B" …
    fn fusion(&self, toks: &[String]) -> Option<(SideReading, SideReading)> {
        let w = self.w();
        // "<prefix> A <joiner> B"
        for pre in &w.prefix {
            if toks.len() > pre.len() + 2 && toks[..pre.len()] == pre[..] {
                let rest = &toks[pre.len()..];
                for j in &w.joiners {
                    let mut from = 1;
                    while let Some(p) = Self::find(rest, j, from) {
                        if let Some(r) = self.both(&rest[..p], &rest[p + j.len()..], false) {
                            return Some(r);
                        }
                        from = p + 1;
                    }
                }
            }
        }
        // "A <joiner> B <suffix>"
        for suf in &w.suffix {
            if toks.len() > suf.len() + 2 && toks[toks.len() - suf.len()..] == suf[..] {
                let rest = &toks[..toks.len() - suf.len()];
                for j in &w.joiners {
                    let mut from = 1;
                    while let Some(p) = Self::find(rest, j, from) {
                        if let Some(r) = self.both(&rest[..p], &rest[p + j.len()..], false) {
                            return Some(r);
                        }
                        from = p + 1;
                    }
                }
            }
        }
        // "A <infix> B"
        for inf in &w.infix {
            let mut from = 1;
            while let Some(p) = Self::find(toks, inf, from) {
                if let Some(r) = self.both(&toks[..p], &toks[p + inf.len()..], false) {
                    return Some(r);
                }
                from = p + 1;
            }
        }
        // "A x B": both must be moves
        for (p, tok) in toks.iter().enumerate().skip(1) {
            if w.cross.contains(tok)
                && let Some(r) = self.both(&toks[..p], &toks[p + 1..], true)
            {
                return Some(r);
            }
        }
        None
    }
}

/// Singular candidates of a plural word ("rasengans" → "rasengan", "punches" → "punch").
fn singulars(w: &str) -> Vec<String> {
    let mut out = Vec::new();
    if w.len() > 4 && (w.ends_with("ches") || w.ends_with("shes") || w.ends_with("xes") || w.ends_with("sses")) {
        out.push(w[..w.len() - 2].to_string());
    }
    if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
        out.push(w[..w.len() - 1].to_string());
    }
    out
}
