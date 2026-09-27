//! THE TOOLBOX (docs/toolbox/BRIEF.md): every move of every power, tier and kit is callable by
//! phrase, right now, by anyone.
//!
//! * **Held** powers keep priority ([`super::PlayerPower::parse_phrase`]: their shouts, tier
//!   versions included, tier names, incantations).
//! * **Kits** (`kind: Kit`): "<kit name> <move word>" ("reinhardt charge", "doomfist's punch",
//!   "nightcrawler teleport", "reinhardt ult") fires that kit move ([`MoveIndex::kit_move`]). The
//!   bare kit name is NOT a reading (it stays the costume summon), and a kit name never grants.
//! * **Borrowed**: the exact shout of ANY power / tier / kit the player doesn't hold fires that move
//!   as a one-shot ([`MoveIndex::shout`]). A power's NAME is never a borrow (it grants, as ever).
//! * **Chains**: [`split_chain`] cuts one line into steps ("a -> b -> c", "amaterasu then chidori",
//!   "summon a dragon, rasengan").
//!
//! Everything here is pure and table-parameterised (the `powers_check` validator runs it against
//! files read at runtime); the game uses [`MoveIndex::shipped`] over [`PowerTable::get`].

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{norm_words, MoveDef, MoveOp, MoveTag, PlayerPower, PowerDef, PowerPhrase, PowerTable};

/// A global move id: any move of any power / tier / kit. Packs into 24 bits ([`Self::pack`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MoveRef {
    /// Table index.
    pub power: u16,
    /// 0 = base, 1.. = that tier's moves (a tier without its own moves has none: its shouts name
    /// base slots and are indexed as tier 0).
    pub tier: u8,
    /// Index into `moves_at(tier)` (0..8).
    pub slot: u8,
}

impl MoveRef {
    pub const fn new(power: u16, tier: u8, slot: u8) -> Self {
        Self { power, tier, slot }
    }
    /// `power << 8 | tier << 4 | slot` (tier ≤ 15, slot ≤ 15).
    pub fn pack(self) -> u32 {
        (self.power as u32) << 8 | ((self.tier as u32 & 0x0F) << 4) | (self.slot as u32 & 0x0F)
    }
    pub fn unpack(v: u32) -> Self {
        Self { power: (v >> 8) as u16, tier: ((v >> 4) & 0x0F) as u8, slot: (v & 0x0F) as u8 }
    }
    /// The power and the move this names in `t` (`None` if it names nothing).
    pub fn resolve(self, t: &PowerTable) -> Option<(&PowerDef, &MoveDef)> {
        let def = t.def(self.power)?;
        let mv = def.moves_at(self.tier).get(self.slot as usize)?;
        // a tier without its own moves has none: `moves_at` fell back to the base list
        if self.tier > 0 && def.tier(self.tier).is_none_or(|td| td.moves.is_empty()) {
            return None;
        }
        Some((def, mv))
    }
}

/// How a phrase reads for one player (the order is the priority order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    /// A phrase of a power the player holds ([`PowerPhrase::Move`] / [`PowerPhrase::Incantation`]).
    Held(PowerPhrase),
    /// "<kit> <move word>".
    Kit(MoveRef),
    /// A shout of a power / tier / kit the player doesn't hold (one-shot).
    Borrow(MoveRef),
    /// Not a move: the lexicon's business (a construct, a power NAME → grant, a costume summon…).
    Plain,
}

impl Reading {
    /// The move a Kit / Borrow reading fires.
    pub fn move_ref(&self) -> Option<MoveRef> {
        match self {
            Reading::Kit(m) | Reading::Borrow(m) => Some(*m),
            _ => None,
        }
    }
    /// Any power-move reading (held move, incantation, kit, borrow).
    pub fn is_move(&self) -> bool {
        !matches!(self, Reading::Plain)
    }
}

/// Why a kit move was picked (validator / preview wording).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KitMatch {
    /// The move's name or one of the kit's shouts.
    Exact,
    /// A slot word ("ultimate", "dodge") → a move tagged with it.
    Tag(MoveTag),
    /// A slot word with no tagged move: the op-shape fallback (Ultimate = the priciest, Movement =
    /// the first dash / blink, Primary = slot 0…).
    TagFallback(MoveTag),
    /// Word overlap with the move's name / shouts.
    Fuzzy,
}

/// The phrase index of a table: every shout, every grant name, every kit prefix.
#[derive(Clone, Debug, Default)]
pub struct MoveIndex {
    shouts: HashMap<String, MoveRef>,
    /// Shouts claimed by more than one move: (shout, kept, dropped) — for the validator.
    pub clashes: Vec<(String, MoveRef, MoveRef)>,
    /// Names / tier names of non-kit powers (saying one grants: never a borrow).
    names: HashSet<String>,
    /// (normalised kit name, power), longest first.
    kits: Vec<(String, u16)>,
}

/// Filler words skipped between a kit name and its move word ("reinhardt use his charge").
const FILLER: &[&str] = &["use", "uses", "do", "does", "the", "his", "her", "their", "its", "a", "an", "my", "s"];

/// Slot words → tags (docs/toolbox/KIT_DONE).
pub const SLOT_WORDS: &[(&str, MoveTag)] = &[
    ("ultimate", MoveTag::Ultimate),
    ("ult", MoveTag::Ultimate),
    ("ulti", MoveTag::Ultimate),
    ("ultimates", MoveTag::Ultimate),
    ("super", MoveTag::Ultimate),
    ("special", MoveTag::Ultimate),
    ("final", MoveTag::Ultimate),
    ("finisher", MoveTag::Ultimate),
    ("finishing", MoveTag::Ultimate),
    ("fatality", MoveTag::Ultimate),
    ("combo", MoveTag::Combo),
    ("combos", MoveTag::Combo),
    ("string", MoveTag::Combo),
    ("flurry", MoveTag::Combo),
    ("rush", MoveTag::Combo),
    ("barrage", MoveTag::Combo),
    ("attack", MoveTag::Primary),
    ("punch", MoveTag::Primary),
    ("hit", MoveTag::Primary),
    ("strike", MoveTag::Primary),
    ("basic", MoveTag::Primary),
    ("primary", MoveTag::Primary),
    ("light", MoveTag::Primary),
    ("dash", MoveTag::Movement),
    ("teleport", MoveTag::Movement),
    ("tp", MoveTag::Movement),
    ("port", MoveTag::Movement),
    ("jump", MoveTag::Movement),
    ("leap", MoveTag::Movement),
    ("blink", MoveTag::Movement),
    ("dodge", MoveTag::Movement),
    ("mobility", MoveTag::Movement),
    ("movement", MoveTag::Movement),
    ("block", MoveTag::Defense),
    ("shield", MoveTag::Defense),
    ("guard", MoveTag::Defense),
    ("parry", MoveTag::Defense),
    ("defense", MoveTag::Defense),
    ("defence", MoveTag::Defense),
    ("barrier", MoveTag::Defense),
    ("counter", MoveTag::Counter),
    ("reversal", MoveTag::Counter),
    ("counterattack", MoveTag::Counter),
    ("riposte", MoveTag::Counter),
];

fn slot_word(w: &str) -> Option<MoveTag> {
    SLOT_WORDS.iter().find(|(s, _)| *s == w).map(|(_, t)| *t)
}

/// "reinhardt's" → "reinhardt" (possessives, spoken or typed).
fn strip_possessive(w: &str) -> &str {
    w.strip_suffix("'s").or_else(|| w.strip_suffix("s'")).or_else(|| w.strip_suffix('\'')).unwrap_or(w)
}

/// Edit distance ≤ 1 (substitution / insertion / deletion), ASCII-ish words.
fn within_one(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a == b {
        return true;
    }
    let (s, l) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if l.len() - s.len() > 1 {
        return false;
    }
    let mut i = 0;
    while i < s.len() && s[i] == l[i] {
        i += 1;
    }
    if s.len() == l.len() {
        s[i + 1..] == l[i + 1..]
    } else {
        s[i..] == l[i + 1..]
    }
}

/// Does spoken word `w` match move word `m` (exact, a ≥ 4-letter prefix, or one typo for ≥ 5)?
fn word_hits(w: &str, m: &str) -> bool {
    w == m || (w.len() >= 4 && m.starts_with(w)) || (m.len() >= 4 && w.starts_with(m)) || (w.len() >= 5 && m.len() >= 5 && within_one(w, m))
}

impl MoveIndex {
    /// Index a table (first claim of a shout wins, in table order: base shouts, then tiers).
    pub fn build(t: &PowerTable) -> MoveIndex {
        let mut ix = MoveIndex::default();
        for (pi, p) in t.powers.iter().enumerate() {
            let pid = pi as u16;
            if p.kind.is_kit() {
                let mut names: Vec<String> = p.names.clone();
                names.push(norm_words(&p.name));
                names.push(p.id.replace('_', " "));
                for n in names {
                    if !n.is_empty() && !ix.kits.iter().any(|(k, q)| *k == n && *q == pid) {
                        ix.kits.push((n, pid));
                    }
                }
            } else {
                ix.names.insert(norm_words(&p.name));
                ix.names.insert(p.id.replace('_', " "));
                ix.names.extend(p.names.iter().cloned());
                for td in &p.tiers {
                    ix.names.extend(td.names.iter().cloned());
                }
            }
            for (s, slot) in &p.shouts {
                if p.moves.get(*slot as usize).is_some_and(borrowable) {
                    ix.claim(s, MoveRef::new(pid, 0, *slot));
                }
            }
            for (k, td) in p.tiers.iter().enumerate() {
                let tier = (k + 1).min(15) as u8;
                for (s, slot) in &td.shouts {
                    // a tier with its own moves: its slot; otherwise the shout names a base move
                    let r = if !td.moves.is_empty() { MoveRef::new(pid, tier, *slot) } else { MoveRef::new(pid, 0, *slot) };
                    if r.resolve(t).is_some_and(|(_, m)| borrowable(m)) {
                        ix.claim(s, r);
                    }
                }
            }
        }
        ix.names.remove("");
        ix.kits.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
        ix
    }

    fn claim(&mut self, shout: &str, r: MoveRef) {
        if shout.is_empty() {
            return;
        }
        match self.shouts.get(shout) {
            Some(old) if *old != r => self.clashes.push((shout.to_string(), *old, r)),
            Some(_) => {}
            None => {
                self.shouts.insert(shout.to_string(), r);
            }
        }
    }

    /// The index of the shipped table ([`PowerTable::get`]).
    pub fn shipped() -> &'static MoveIndex {
        static IX: OnceLock<MoveIndex> = OnceLock::new();
        IX.get_or_init(|| MoveIndex::build(PowerTable::get()))
    }

    /// Every indexed shout (validator).
    pub fn shouts(&self) -> impl Iterator<Item = (&str, MoveRef)> {
        self.shouts.iter().map(|(k, v)| (k.as_str(), *v))
    }

    /// The move a whole normalised phrase shouts (any power / tier / kit).
    pub fn shout(&self, n: &str) -> Option<MoveRef> {
        self.shouts.get(n).copied()
    }

    /// Is `n` (normalised) a grant name of some non-kit power / tier?
    pub fn is_grant_name(&self, n: &str) -> bool {
        self.names.contains(n)
    }

    /// Kit prefixes (validator).
    pub fn kit_names(&self) -> impl Iterator<Item = (&str, u16)> {
        self.kits.iter().map(|(k, p)| (k.as_str(), *p))
    }

    /// "<kit name> <move word>" (normalised phrase) → that kit's move. The bare kit name → `None`.
    pub fn kit_move(&self, t: &PowerTable, n: &str) -> Option<(MoveRef, KitMatch)> {
        if self.kits.is_empty() {
            return None;
        }
        // Possessives: "reinhardt's charge" → "reinhardt charge" (also "reinhardts charge").
        let mut words: Vec<&str> = n.split(' ').filter(|w| !w.is_empty()).map(strip_possessive).collect();
        if words.len() < 2 {
            return None;
        }
        for (kit, pid) in &self.kits {
            let kw: Vec<&str> = kit.split(' ').collect();
            if kw.len() >= words.len() {
                continue;
            }
            let mut fits = true;
            for (k, w) in kw.iter().enumerate() {
                let last = k + 1 == kw.len();
                // "reinhardts" (a possessive without the apostrophe) on the name's last word
                if *w != words[k] && !(last && words[k].len() == w.len() + 1 && words[k].ends_with('s') && words[k].starts_with(w)) {
                    fits = false;
                    break;
                }
            }
            if !fits {
                continue;
            }
            let rest: Vec<&str> = words.split_off(kw.len());
            let def = t.def(*pid)?;
            if let Some(hit) = pick_kit_move(def, kit, &rest) {
                return Some((MoveRef::new(*pid, 0, hit.0), hit.1));
            }
            // A kit matched but no move did: don't try shorter kit names inside a longer one.
            return None;
        }
        None
    }

    /// How `text` reads for a player holding `held` (see [`Reading`]; `held` = `None` for a
    /// powerless reading, e.g. the validator).
    pub fn read(&self, t: &PowerTable, held: Option<&PlayerPower>, text: &str) -> Reading {
        let n = norm_words(text);
        if n.is_empty() {
            return Reading::Plain;
        }
        if let Some(pw) = held {
            match pw.parse_phrase(text) {
                PowerPhrase::Plain => {}
                other => return Reading::Held(other),
            }
        }
        if let Some((m, _)) = self.kit_move(t, &n) {
            return Reading::Kit(m);
        }
        // A power's NAME grants (the lexicon): never borrowed, even if some move shouts it.
        if self.is_grant_name(&n) {
            return Reading::Plain;
        }
        if let Some(m) = self.shout(&n) {
            // A shout of a power you hold always went the held way above (tier versions included);
            // a held power's shout of another tier is still a borrow of that tier's move.
            return Reading::Borrow(m);
        }
        Reading::Plain
    }
}

/// Can a move be fired by someone who doesn't hold its power? Not one that only transforms or
/// switches tiers (those are the holder's; saying the power's NAME is how you get it).
pub fn borrowable(mv: &MoveDef) -> bool {
    mv.ops.iter().any(|o| !matches!(o, MoveOp::Transform | MoveOp::Tier(_)))
}

/// The kit move `rest` (words after the kit name) names: (slot, why).
fn pick_kit_move(def: &PowerDef, kit: &str, rest: &[&str]) -> Option<(u8, KitMatch)> {
    let moves = def.moves_at(0);
    if moves.is_empty() {
        return None;
    }
    let rest: Vec<&str> = rest.iter().copied().filter(|w| !FILLER.contains(w)).collect();
    if rest.is_empty() {
        return None;
    }
    // Loose readings (words, tags) need most of the words to mean something: "reinhardt charge
    // doomfist punch" (a chain said without a separator) is not one move.
    let enough = |hits: usize| hits * 2 >= rest.len();
    let joined = rest.join(" ");
    // 1. exact: a move name, or a kit shout (with or without the kit name in front)
    for (k, m) in moves.iter().enumerate() {
        if norm_words(&m.name) == joined {
            return Some((k as u8, KitMatch::Exact));
        }
    }
    let with_kit = format!("{kit} {joined}");
    for (s, slot) in &def.shouts {
        if (*slot as usize) < moves.len() && (*s == joined || *s == with_kit) {
            return Some((*slot, KitMatch::Exact));
        }
    }
    // 2. a whole word of a move's name / shouts ("doomfist punch" → "rocket punch", even if
    //    another move is tagged Primary: the character's own word wins over the slot word)
    if let Some((k, hits)) = best_overlap(def, moves, &rest, true)
        && enough(hits)
    {
        return Some((k, KitMatch::Fuzzy));
    }
    // 3. slot words → tags (the first slot word decides)
    let tag = rest.iter().find_map(|w| slot_word(w));
    let tag_ok = enough(rest.iter().filter(|w| slot_word(w).is_some()).count());
    if let Some(tag) = tag.filter(|_| tag_ok) {
        if let Some(k) = moves.iter().position(|m| m.tags.contains(&tag)) {
            return Some((k as u8, KitMatch::Tag(tag)));
        }
    }
    // 4. prefix / one-typo overlap ("reinhardt chrge", "nightcrawler tele")
    if let Some((k, hits)) = best_overlap(def, moves, &rest, false)
        && enough(hits)
    {
        return Some((k, KitMatch::Fuzzy));
    }
    // 5. a slot word with no tagged move: the op-shape guess
    tag.filter(|_| tag_ok).and_then(|tag| tag_fallback(moves, tag).map(|k| (k, KitMatch::TagFallback(tag))))
}

/// Words of `rest` hitting move `slot`'s name / shouts (`whole`: exact words only).
fn overlap(m: &MoveDef, def: &PowerDef, slot: u8, rest: &[&str], whole: bool) -> usize {
    let name = norm_words(&m.name);
    let mut words: Vec<&str> = name.split(' ').collect();
    for (s, sl) in &def.shouts {
        if *sl == slot {
            words.extend(s.split(' '));
        }
    }
    rest.iter()
        .filter(|w| w.len() >= 3 && words.iter().any(|m| m.len() >= 3 && if whole { *w == m } else { word_hits(w, m) }))
        .count()
}

/// The move with the most word hits: (slot, hits).
fn best_overlap(def: &PowerDef, moves: &[MoveDef], rest: &[&str], whole: bool) -> Option<(u8, usize)> {
    let mut best: Option<(usize, u8)> = None;
    for (k, m) in moves.iter().enumerate() {
        let s = overlap(m, def, k as u8, rest, whole);
        if s > 0 && best.is_none_or(|(b, _)| s > b) {
            best = Some((s, k as u8));
        }
    }
    best.map(|(s, k)| (k, s))
}

/// A slot word with no tagged move: guess from the move's shape.
fn tag_fallback(moves: &[MoveDef], tag: MoveTag) -> Option<u8> {
    let find = |f: &dyn Fn(&MoveOp) -> bool| moves.iter().position(|m| m.ops.iter().any(f)).map(|k| k as u8);
    match tag {
        MoveTag::Primary => Some(0),
        MoveTag::Ultimate => {
            // the priciest (ties: the later one — kits list their ultimate last)
            let mut best = 0usize;
            for (k, m) in moves.iter().enumerate() {
                if m.cost >= moves[best].cost {
                    best = k;
                }
            }
            (moves.len() > 1 || moves[0].cost > 0.0).then_some(best as u8)
        }
        MoveTag::Movement => find(&|o| matches!(o, MoveOp::Blink { .. } | MoveOp::Impulse { .. } | MoveOp::Reel { .. })),
        MoveTag::Defense => find(&|o| matches!(o, MoveOp::Field { strength, .. } if *strength < 0.0) || matches!(o, MoveOp::SelfStatus { .. })),
        MoveTag::Combo | MoveTag::Counter => None,
    }
}

// ------------------------------------------------------------------------------------ chains

/// Most steps one chain line runs.
pub const MAX_CHAIN: usize = 8;

/// Strong separators always split; weak ones ("," / "into" / "and") only when some step is a move.
const STRONG_SEPS: &[&str] = &["->", "=>", ">", "→", "then", "and then", "followed by", "after that", "and after that"];
const WEAK_SEPS: &[&str] = &[",", ";", "into", "and"];

/// Cut one typed / spoken line into chain steps. `is_move(step)` says whether a step reads as a
/// power move (held / kit / borrow); weak separators only split when at least one step does
/// ("a big, flaming hammer" stays one phrase, "summon a dragon, rasengan" is two). The whole line
/// reading as a move is never split. Returns 1 step for a plain phrase.
pub fn split_chain(text: &str, is_move: &dyn Fn(&str) -> bool) -> Vec<String> {
    split_chain_with(text, is_move, None)
}

/// [`split_chain`] with the lexicon's opinion (the console, the preview tool, the validator):
/// `understood(s)` = the lexicon reads `s` as one clean thing (not improvised). Weak separators
/// then split only when every piece is a move or understood on its own:
/// "a big, flaming hammer" stays one hammer even though "flaming hammer" is somebody's shout;
/// "summon a dragon, rasengan" is a dragon then a Rasengan.
pub fn split_chain_with(text: &str, is_move: &dyn Fn(&str) -> bool, understood: Option<&dyn Fn(&str) -> bool>) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }
    // A whole line that is itself one move is never split ("and then there were none" as a shout)
    // — unless it has an arrow: arrows only ever mean a chain.
    let arrows = ["->", "=>", ">", "→"].iter().any(|a| text.contains(a));
    if !arrows && is_move(text) {
        return vec![text.to_string()];
    }
    let strong = split_on(text, STRONG_SEPS);
    let single = strong.len() == 1;
    let mut out: Vec<String> = Vec::new();
    for part in strong {
        // one part = the line as typed (keeps its own spacing / punctuation for the lexicon)
        let part = if single { clean_step(text) } else { clean_step(&part) };
        let weak: Vec<String> = split_on(&part, WEAK_SEPS).iter().map(|w| clean_step(w)).filter(|w| !norm_words(w).is_empty()).collect();
        let lexicon_ok = match understood {
            None => true,
            Some(u) => weak.iter().all(|w| is_move(w) || u(w)),
        };
        if weak.len() > 1 && weak.iter().any(|w| is_move(w)) && lexicon_ok {
            out.extend(weak);
        } else {
            out.push(part);
        }
    }
    out.retain(|s| !norm_words(s).is_empty());
    out.truncate(MAX_CHAIN);
    out
}

/// A step without the spoken punctuation around it ("Then, doomfist punch." → "doomfist punch").
fn clean_step(s: &str) -> String {
    s.trim().trim_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '-')).trim().to_string()
}

/// Split on whole-word separators (symbols anywhere; words at word boundaries).
fn split_on(text: &str, seps: &[&str]) -> Vec<String> {
    // Tokenise keeping symbol separators as their own tokens.
    let mut toks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        if two == "->" || two == "=>" {
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
            toks.push(two);
            i += 2;
            continue;
        }
        if c == '>' || c == '→' || c == ',' || c == ';' {
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
            toks.push(c.to_string());
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
            i += 1;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if !cur.is_empty() {
        toks.push(cur);
    }
    let lower: Vec<String> = toks.iter().map(|t| t.trim_matches(|c: char| c == '.' || c == '!' || c == '?').to_lowercase()).collect();
    // longest separators first (multi-word)
    let mut seps: Vec<Vec<&str>> = seps.iter().map(|s| s.split(' ').collect()).collect();
    seps.sort_by(|a, b| b.len().cmp(&a.len()));
    let mut out: Vec<String> = Vec::new();
    let mut part: Vec<&str> = Vec::new();
    let mut k = 0;
    'tok: while k < toks.len() {
        for s in &seps {
            if k + s.len() <= lower.len() && s.iter().enumerate().all(|(j, w)| lower[k + j] == *w) {
                if !part.is_empty() {
                    out.push(part.join(" "));
                    part.clear();
                }
                k += s.len();
                continue 'tok;
            }
        }
        part.push(&toks[k]);
        k += 1;
    }
    if !part.is_empty() {
        out.push(part.join(" "));
    }
    out
}
