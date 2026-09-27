//! Intent layer — the ONE place a phrase's stages run, in one fixed order, for typed and spoken input
//! alike (the game calls [`crate::Lexicon::intent`]; `docs/LEXICON_STAGES.md`-style notes live in the
//! module docs below and in `tests/stage_order.rs`).
//!
//! **Stage order** (earlier stages cut words so later stages never misread them):
//! 0. *(game, voice only)* oath claim — the raw transcript of your own corps oath is a recital, not a
//!    cast (`bd_game::oath`); matched on the untouched utterance so no later surgery can cut an oath.
//! 1. **refer** — "again!" / "two more" (re-cast; the game substitutes the last phrase and runs the
//!    stages again) and "make it bigger" / "set it on fire" (the rest grafts onto your last construct).
//! 2. **target** — "<thing> at|on|onto|on top of|around|after … <player>" and "<verb> <player> [with
//!    <thing>]" ("cage ethan", "smite jake", "hit mia with a meteor"). The name words are removed, so
//!    "meteor on ethan" is never read as a meteor *riding* Ethan (relations' mount) and "sam" never
//!    becomes a noun. The preposition gives the delivery.
//! 3. **delivery verb** — "drop / rain / throw / summon / cage …" at the front comes off and gives (or
//!    overrides) the delivery; some verbs add a mechanic (drop → DropFromSky) or re-read the rest through
//!    an existing relation ("rain arrows" → "rain of arrows", a volley). Data: `data/intent.ron`.
//! 4. the resolver ([`crate::Lexicon::resolve`] / `resolve_spoken`), whose own order is:
//!    **invocation/signature** ("domain expansion …", "bankai …") → **pokémon grammar** ("pikachu use
//!    thunderbolt") → incantation prefix ("gomu gomu no …") → **relations** (sizes, mounts, emitters,
//!    volleys) → **compounds** / exact names → **resolver** grammar → **leftover words** (light grafts)
//!    → signature tags (famous ultimates by name, yelled beams).
//!
//! Everything here is vocabulary: which player, which delivery. bd_sim decides what a delivery does per
//! form category and validates the target (exists, alive, in range); anything that doesn't apply falls
//! back to an ordinary cast ("never says no").

use std::sync::OnceLock;

use bd_core::mechanics::{Feature, Form, FormCategory, Mechanic, Recipe};
use serde::Deserialize;

use crate::fuzzy::{dl_distance, max_typo};
use crate::norm::normalize_key;
use crate::refer::{Refer, Targeted, Word, words};
use crate::{Lexicon, Resolution};

/// How a phrase reaches its target (or aim point). See `data/intent.ron`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub enum Delivery {
    /// From the sky onto them (drop, rain, smite, "on top of").
    Onto,
    /// Thrown / fired / driven toward them (throw, launch, "at").
    At,
    /// Appears next to them (summon, spawn, "near").
    Near,
    /// Placed around them (cage, trap, "around").
    Around,
    /// Sent after them from here ("after", sic, chase).
    Hunt,
}

impl Delivery {
    pub fn name(self) -> &'static str {
        match self {
            Delivery::Onto => "onto",
            Delivery::At => "at",
            Delivery::Near => "near",
            Delivery::Around => "around",
            Delivery::Hunt => "hunt",
        }
    }
    pub fn from_name(s: &str) -> Option<Delivery> {
        Some(match s {
            "onto" => Delivery::Onto,
            "at" => Delivery::At,
            "near" => Delivery::Near,
            "around" => Delivery::Around,
            "hunt" => Delivery::Hunt,
            _ => return None,
        })
    }
}

#[derive(Deserialize)]
struct Verb {
    words: String,
    via: Delivery,
    #[serde(default)]
    mech: Vec<Mechanic>,
    #[serde(default)]
    rewrite: Option<String>,
    #[serde(default)]
    alone: Option<String>,
    #[serde(default)]
    strict: bool,
    /// Generic placement verb ("send", "put", "summon"): a preposition's delivery wins over it.
    #[serde(default)]
    generic: bool,
}

#[derive(Deserialize)]
struct Prep {
    words: String,
    via: Delivery,
}

#[derive(Deserialize)]
struct IntentData {
    verbs: Vec<Verb>,
    target_verbs: Vec<String>,
    /// Words that introduce the thing in "<verb> <name> with|in <thing>".
    with_words: Vec<String>,
    preps: Vec<Prep>,
    weak_preps: Vec<String>,
    after_name: Vec<String>,
    name_noise: Vec<String>,
    around_forms: Vec<(String, String)>,
    not_verbs: Vec<String>,
    location_preps: Vec<String>,
    // ---- PILOTING (see `data/intent.ron`, [`Piloting`])
    #[serde(default)]
    pilot_verbs: Vec<String>,
    #[serde(default)]
    pilot_trailing: Vec<String>,
    #[serde(default)]
    board_rest: Vec<String>,
    #[serde(default)]
    board_commands: Vec<String>,
    #[serde(default)]
    follower_words: Vec<String>,
    #[serde(default)]
    ride_default: Vec<String>,
    #[serde(default)]
    ride_min_scale: f32,
}

fn data() -> Option<&'static IntentData> {
    static D: OnceLock<Option<IntentData>> = OnceLock::new();
    D.get_or_init(|| {
        let mut d = ron::from_str::<IntentData>(include_str!("../data/intent.ron")).ok()?;
        // longest first
        d.verbs.sort_by_key(|v| std::cmp::Reverse(v.words.split(' ').count()));
        d.preps.sort_by_key(|p| std::cmp::Reverse(p.words.split(' ').count()));
        d.target_verbs.sort_by_key(|v| std::cmp::Reverse(v.split(' ').count()));
        d.pilot_verbs.sort_by_key(|v| std::cmp::Reverse(v.split(' ').count()));
        d.pilot_trailing.sort_by_key(|v| std::cmp::Reverse(v.split(' ').count()));
        d.follower_words.sort_by_key(|v| std::cmp::Reverse(v.split(' ').count()));
        Some(d)
    })
    .as_ref()
}

/// A phrase after the intent stages (see the module docs for the order).
#[derive(Clone, Debug, PartialEq)]
pub struct Intent {
    /// What the resolver read (names, "make it", delivery verbs removed; original casing kept).
    pub text: String,
    /// "it" / "again" (for `Again` the game re-runs [`crate::Lexicon::intent`] on its last phrase).
    pub refer: Option<Refer>,
    /// Index into the `names` given to [`crate::Lexicon::intent`].
    pub target: Option<usize>,
    /// The words that were read as the name ("eaton", "jay k").
    pub heard_name: Option<String>,
    /// 0 = the name exactly, else how loosely it matched (typo / sound-alike).
    pub name_dist: u8,
    /// How it is delivered (verb beats preposition). `None` = an ordinary cast at the crosshair.
    pub delivery: Option<Delivery>,
    pub resolution: Resolution,
    /// The intent stages that fired, in order ("again", "it", "target", "verb:drop", "signature", …).
    pub stages: Vec<String>,
    /// Who drives what this builds (None = not something you drive; see [`Piloting`]).
    pub pilot: Option<Piloting>,
}

/// How a phrase decided who drives what it builds (`data/intent.ron` PILOTING; COMBAT.md §5d-ter).
/// Whatever it says, the recipe carries it: `Piloted` = the host seats the caster (if they are alive and
/// not already seated — else it runs as a follower); no `Piloted` = it fights on its own AI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Piloting {
    /// "ride / drive / pilot / get in / hop in / board … <thing>", "<thing> i can pilot": seated.
    Explicit,
    /// Nothing said, but a single summon whose fantasy is being driven or ridden (a vehicle, anything
    /// that Transforms, a cockpit mech / titan, a classic mount): seated (owner rule: pilot by default,
    /// hop out with E to make it a follower).
    Default,
    /// "<thing> that follows me", "on autopilot", "on its own", "driverless": no seat, its own AI.
    Follower,
    /// "get in", "hop in", "ride it", "let me drive", "take the wheel", "mount up": a command — board a
    /// construct you already own ([`crate::GraftAction::Board`]).
    Board,
}

impl Intent {
    /// One-line summary for logs (`logs/intent.log`): signature, kind, target, delivery, confidence.
    pub fn summary(&self, names: &[&str]) -> String {
        let r = &self.resolution;
        let mut s = format!("{} [{:?}] conf={:.2}", r.signature(), r.kind, r.confidence);
        if r.recipe.count > 1 {
            s.push_str(&format!(" n={}", r.recipe.count));
        }
        s.push_str(&format!(" scale={:.1}", r.recipe.scale));
        if let Some(sig) = &r.signature {
            s.push_str(&format!(" sig={:?}", sig.kind));
        }
        if let Some(t) = self.target {
            let n = names.get(t).copied().unwrap_or("?");
            s.push_str(&format!(" target={n}"));
            if self.name_dist > 0 {
                s.push_str(&format!("~{}", self.name_dist));
            }
        }
        if let Some(d) = self.delivery {
            s.push_str(&format!(" via={}", d.name()));
        }
        if !r.ignored.is_empty() {
            s.push_str(&format!(" ignored={:?}", r.ignored));
        }
        s
    }
}

// ------------------------------------------------------------------------------------------ names

/// Split a display name into lowercase word tokens: separators, camelCase and letter/digit boundaries
/// ("DragonSlayer" → dragon slayer, "Alex_99" → alex 99, "xX_Nova_Xx" → xx nova xx).
pub(crate) fn name_tokens(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    for c in raw.chars().take(64) {
        let c = if c.is_ascii() { c } else { normalize_key(&c.to_string()).chars().next().unwrap_or(' ') };
        if !c.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev = None;
            continue;
        }
        let boundary = match prev {
            Some(p) => (p.is_ascii_lowercase() && c.is_ascii_uppercase()) || (p.is_ascii_digit() != c.is_ascii_digit()),
            None => false,
        };
        if boundary && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c.to_ascii_lowercase());
        prev = Some(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.truncate(6);
    out
}

/// A display name, prepared once per call.
struct NameForm {
    /// Word tokens (see [`name_tokens`]).
    toks: Vec<String>,
    /// Everything squashed ("dragonslayer", "alex99").
    squashed: String,
    /// Letters only ("alex").
    letters: String,
    /// Tokens that identify the player on their own (≥ 3 letters, not noise like "xx").
    distinctive: Vec<String>,
}

impl NameForm {
    fn new(raw: &str, noise: &[String]) -> NameForm {
        let toks = name_tokens(raw);
        let squashed: String = toks.concat();
        let letters: String = squashed.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        let distinctive = toks
            .iter()
            .filter(|t| t.len() >= 3 && t.chars().all(|c| c.is_ascii_alphabetic()) && !noise.iter().any(|n| n == *t))
            .cloned()
            .collect();
        NameForm { toks, squashed, letters, distinctive }
    }
}

/// Number words → digits inside a heard name ("alex ninety nine" → alex 99).
fn fold_numbers(keys: &[&str]) -> String {
    fn unit(w: &str) -> Option<u32> {
        Some(match w {
            "zero" | "oh" => 0,
            "one" => 1,
            "two" => 2,
            "three" => 3,
            "four" => 4,
            "five" => 5,
            "six" => 6,
            "seven" => 7,
            "eight" => 8,
            "nine" => 9,
            "ten" => 10,
            "eleven" => 11,
            "twelve" => 12,
            "thirteen" => 13,
            "fourteen" => 14,
            "fifteen" => 15,
            "sixteen" => 16,
            "seventeen" => 17,
            "eighteen" => 18,
            "nineteen" => 19,
            _ => return None,
        })
    }
    fn tens(w: &str) -> Option<u32> {
        Some(match w {
            "twenty" => 20,
            "thirty" => 30,
            "forty" => 40,
            "fifty" => 50,
            "sixty" => 60,
            "seventy" => 70,
            "eighty" => 80,
            "ninety" => 90,
            _ => return None,
        })
    }
    let mut out = String::new();
    let mut i = 0;
    while i < keys.len() {
        if let Some(t) = tens(keys[i]) {
            let u = keys.get(i + 1).and_then(|w| unit(w)).filter(|u| *u < 10);
            out.push_str(&(t + u.unwrap_or(0)).to_string());
            i += if u.is_some() { 2 } else { 1 };
        } else if let Some(u) = unit(keys[i]) {
            out.push_str(&u.to_string());
            i += 1;
        } else {
            out.push_str(keys[i]);
            i += 1;
        }
    }
    out
}

/// A sound key for names: how it is *said*, not spelled ("ethan", "eathan", "e than", "eaton" → etan /
/// etn). Letters only.
pub(crate) fn sound_key(s: &str) -> String {
    let w: Vec<u8> = s.bytes().filter(|b| b.is_ascii_alphabetic()).map(|b| b.to_ascii_lowercase()).collect();
    let mut out: Vec<u8> = Vec::with_capacity(w.len());
    let mut i = 0;
    while i < w.len() {
        let c = w[i];
        let nx = w.get(i + 1).copied().unwrap_or(0);
        let (sym, step): (&[u8], usize) = match (c, nx) {
            (b't', b'h') => (b"t", 2),
            (b'p', b'h') => (b"f", 2),
            (b'c', b'k') => (b"k", 2),
            (b'g', b'h') => (b"g", 2),
            (b'k', b'n') if i == 0 => (b"n", 2),
            (b'w', b'r') if i == 0 => (b"r", 2),
            (b's', b'h') => (b"s", 2),
            (b'c', b'h') => (b"c", 2),
            (b'c', b'e' | b'i' | b'y') => (b"s", 1),
            (b'c', _) => (b"k", 1),
            (b'q', _) => (b"k", 1),
            (b'x', _) => (b"ks", 1),
            (b'z', _) => (b"s", 1),
            (b'y', _) if i == 0 => (b"y", 1),
            (b'y', _) => (b"a", 1),
            // silent final e ("jake" sounds like "jayk")
            (b'e', 0) if w.len() > 3 && i > 0 && !matches!(w[i - 1], b'a' | b'e' | b'i' | b'o' | b'u' | b'y') => (b"", 1),
            (b'a' | b'e' | b'i' | b'o' | b'u', _) => (b"a", 1),
            (b'h', _) if i > 0 => (b"", 1),
            _ => (std::slice::from_ref(&w[i]), 1),
        };
        for &b in sym {
            if out.last() != Some(&b) {
                out.push(b);
            }
        }
        i += step;
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Consonant skeleton of a sound key (first letter kept): "atan" → "atn".
fn skeleton(sound: &str) -> String {
    sound.char_indices().filter(|(i, c)| *i == 0 || *c != 'a').map(|(_, c)| c).collect()
}

/// Match a span of words against the names. Returns (name index, distance: 0 = exact).
fn match_name(span: &[Word], names: &[NameForm], spoken: bool, weak_prep: bool, common: &dyn Fn(&str) -> bool) -> Option<(usize, u8)> {
    let keys: Vec<&str> = span.iter().map(|w| w.key.as_str()).collect();
    if keys.first().is_some_and(|k| matches!(*k, "a" | "an" | "the" | "some" | "my" | "your" | "his" | "her" | "their" | "our")) {
        // "missile at the moon" — names don't take articles, except a spoken "the dragon slayer"
        // for DragonSlayer: then only the exact name counts
        if keys[0] == "the" && span.len() >= 2 {
            return match_name(&span[1..], names, spoken, weak_prep, common).filter(|(_, d)| *d == 0);
        }
        return None;
    }
    let spaced = keys.join(" ");
    let squashed = fold_numbers(&keys);
    let letters: String = squashed.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let is_common = common(&spaced);
    let mut best: Option<(usize, u8)> = None;
    let mut offer = |i: usize, d: u8| {
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    };
    for (i, n) in names.iter().enumerate() {
        if n.toks.is_empty() {
            continue;
        }
        // Exact: the whole name (any spacing), its letters ("alex" for Alex_99), or one distinctive word
        // of a multi-word name ("tony" for Big Tony, "nova" for xX_Nova_Xx) — a lone *common* word only
        // after a strong aim preposition ("missile at nova"), never after "on" ("sword on fire").
        let whole = squashed == n.squashed || (n.letters.len() >= 3 && squashed == n.letters) || spaced == n.toks.join(" ");
        let part = span.len() == 1 && n.toks.len() > 1 && n.distinctive.iter().any(|t| *t == spaced) && (!is_common || !weak_prep);
        if whole || part {
            offer(i, 0);
            continue;
        }
        if is_common {
            continue; // "fire" stays fire unless a player is literally called Fire
        }
        // Typos: the whole name, its letters, a distinctive word (≥ 4).
        let mut cands: Vec<&str> = vec![n.squashed.as_str(), n.letters.as_str()];
        if n.toks.len() > 1 {
            cands.extend(n.distinctive.iter().map(String::as_str).filter(|t| t.len() >= 4));
        }
        for c in cands.iter().filter(|c| c.len() >= 3) {
            // 3-letter names forgive one swap / slip of a later letter ("sma", "zeo"), first letter kept
            let short = c.len() == 3 && squashed.len() == 3 && c.as_bytes()[0] == squashed.as_bytes()[0];
            let limit = if short { 1 } else { max_typo(c.len().max(squashed.len()), spoken) };
            if limit > 0
                && let Some(d) = dl_distance(squashed.as_bytes(), c.as_bytes(), limit)
            {
                offer(i, d as u8);
            }
            // Voice (and phonetic typing): same sound ("eaton" / "e than" / "ethen" for Ethan, "jay k"
            // for Jake, "zoey" for Zoe). Short names need the whole sound; longer ones the skeleton.
            if letters.len() >= 3 {
                let (a, b) = (sound_key(&letters), sound_key(c));
                if a == b && a.len() >= 2 {
                    offer(i, 1);
                } else if spoken {
                    let (sa, sb) = (skeleton(&a), skeleton(&b));
                    if sa == sb && sa.len() >= 3 {
                        offer(i, 1);
                    } else if a.len() >= 4 && b.len() >= 4 && dl_distance(a.as_bytes(), b.as_bytes(), 1).is_some() && sa.len() >= 2 && sa == sb {
                        offer(i, 2);
                    }
                }
            }
        }
    }
    best
}

fn keys_at<'a>(ws: &'a [Word], at: usize, n: usize) -> Option<Vec<&'a str>> {
    (at + n <= ws.len()).then(|| ws[at..at + n].iter().map(|w| w.key.as_str()).collect())
}

fn phrase_at(ws: &[Word], at: usize, phrase: &str) -> bool {
    let want: Vec<&str> = phrase.split(' ').collect();
    keys_at(ws, at, want.len()).is_some_and(|k| k == want)
}

/// Words that open a phrase without meaning anything ("um drop a meteor on ethan").
const LEAD_FILLER: &[&str] =
    &["ok", "okay", "now", "please", "and", "then", "oh", "hey", "yo", "alright", "so", "just", "um", "uh", "er", "erm", "yeah", "bro", "dude"];

/// Targeting (stage 2). `common(key)` = the words are a known everyday word (then only an exact name
/// counts). `None` = no player named.
pub(crate) fn find_target(
    text: &str,
    names: &[&str],
    spoken: bool,
    common: &dyn Fn(&str) -> bool,
    phrase: &dyn Fn(&str) -> bool,
) -> Option<Targeted> {
    let d = data()?;
    if names.is_empty() {
        return None;
    }
    let forms: Vec<NameForm> = names.iter().map(|n| NameForm::new(n, &d.name_noise)).collect();
    let ws = words(text);
    let n = ws.len();
    if n < 2 {
        return None;
    }
    let tail_ok = |from: usize| ws[from..].iter().all(|w| d.after_name.iter().any(|a| *a == w.key));
    let heard = |from: usize, len: usize| ws[from..from + len].iter().map(|w| w.key.as_str()).collect::<Vec<_>>().join(" ");
    // A: "<rest> <prep> <name> [tail]" — scan preposition positions from the end (the aim is the tail).
    // (longest preposition ending right before the name: "next to alex" is "next to", not "to")
    for name_from in (2..n).rev() {
        let Some((prep, p)) = d.preps.iter().find_map(|pr| {
            let k = pr.words.split(' ').count();
            (name_from > k && phrase_at(&ws, name_from - k, &pr.words)).then(|| (pr, name_from - k))
        }) else {
            continue;
        };
        let weak_prep = d.weak_preps.iter().any(|w| *w == prep.words);
        for len in (1..=3.min(n - name_from)).rev() {
            if !tail_ok(name_from + len) {
                continue;
            }
            // After a weak preposition a common word only names a player when "<prep> <word>" isn't a
            // phrase of its own ("sword on fire", "shark on ice" stay; "bomb on nova" finds xX_Nova_Xx).
            let weak = weak_prep && phrase(&format!("{} {}", prep.words, heard(name_from, len)));
            if let Some((i, dist)) = match_name(&ws[name_from..name_from + len], &forms, spoken, weak, common) {
                let rest = text[..ws[p].start].trim().trim_end_matches([',', '-', ':', ';']).trim().to_string();
                if rest.is_empty() {
                    break;
                }
                return Some(Targeted { rest, player: i, via: prep.via, heard: heard(name_from, len), dist, verb: None });
            }
        }
    }
    let mut s = 0;
    while s < n && LEAD_FILLER.contains(&ws[s].key.as_str()) {
        s += 1;
    }
    // B: "<verb> <name> [with|in <thing>]" ("cage ethan", "smite jake", "hit mia with a meteor",
    // "trap nova in a bubble").
    let is_with = |k: &str| d.with_words.iter().any(|w| w == k);
    if let Some(verb) = d.target_verbs.iter().find(|v| phrase_at(&ws, s, v)) {
        let name_from = s + verb.split(' ').count();
        for len in (1..=3.min(n.saturating_sub(name_from))).rev() {
            let after = name_from + len;
            let with_at = (after < n && is_with(&ws[after].key)).then_some(after);
            if with_at.is_none() && !tail_ok(after) {
                continue;
            }
            // the verb's object slot is strong: a distinctive word of a name counts ("hit nova with …")
            if let Some((i, dist)) = match_name(&ws[name_from..after], &forms, spoken, false, common) {
                let via = d.verbs.iter().find(|x| x.words == *verb).map(|x| x.via).unwrap_or(Delivery::Hunt);
                let verb_text = text[ws[s].start..ws[name_from - 1].end].to_string();
                let rest = match with_at {
                    Some(w) if w + 1 < n => format!("{verb_text} {}", text[ws[w + 1].start..].trim()),
                    _ => verb_text.clone(),
                };
                return Some(Targeted { rest, player: i, via, heard: heard(name_from, len), dist, verb: Some(verb_text) });
            }
        }
    }
    // C: "<thing> <verb>s <name>" ("wolves chase sam", "bees attack mia", "a dragon hunts jake").
    for p in (s + 1..n.saturating_sub(1)).rev() {
        let k = ws[p].key.as_str();
        let stems = [k, k.strip_suffix("es").unwrap_or("-"), k.strip_suffix('s').unwrap_or("-")];
        let Some(v) = d.verbs.iter().find(|v| stems.contains(&v.words.as_str()) && d.target_verbs.iter().any(|t| *t == v.words)) else {
            continue;
        };
        let from = p + 1;
        for len in (1..=3.min(n - from)).rev() {
            if !tail_ok(from + len) {
                continue;
            }
            if let Some((i, dist)) = match_name(&ws[from..from + len], &forms, spoken, false, common) {
                let rest = text[..ws[p].start].trim().to_string();
                if rest.is_empty() {
                    break;
                }
                return Some(Targeted { rest, player: i, via: v.via, heard: heard(from, len), dist, verb: None });
            }
        }
    }
    None
}

/// "<head> <place-prep> the|a <place>" → the head ("meteor shower over the lake" → "meteor shower").
fn location_head(d: &IntentData, text: &str) -> Option<String> {
    let ws = words(text);
    let n = ws.len();
    for p in (1..n.saturating_sub(1)).rev() {
        let Some(prep) = d.location_preps.iter().find(|pr| phrase_at(&ws, p, pr)) else { continue };
        let at = p + prep.split(' ').count();
        if at < n && matches!(ws[at].key.as_str(), "the" | "a" | "an" | "my" | "your" | "his" | "her" | "their" | "that" | "this") && n - at <= 4 {
            let head = text[..ws[p].start].trim().to_string();
            return (!head.is_empty()).then_some(head);
        }
    }
    None
}

// ------------------------------------------------------------------------------------------ verbs

/// A delivery verb found at the front of a phrase (stage 3).
struct VerbHit<'a> {
    verb: &'a Verb,
    /// The phrase for the resolver (the rest, rewritten, or the verb's `alone`).
    rest: String,
}

const DETERMINERS: &[&str] = &[
    "a", "an", "the", "some", "my", "your", "his", "her", "their", "our", "this", "that", "these", "those", "lots", "lot", "loads", "tons",
    "bunch", "few", "several", "many", "couple", "pair", "dozen", "all", "every", "one", "two", "three", "four", "five", "six", "seven",
    "eight", "nine", "ten", "twelve", "twenty", "fifty", "hundred", "thousand", "million", "giant", "huge", "big", "massive", "gigantic",
];

// ------------------------------------------------------------------------------------------ piloting

/// First word after lead filler ("ok", "um", "now").
fn lead_start(ws: &[Word]) -> usize {
    let mut s = 0;
    while s + 1 < ws.len() && LEAD_FILLER.contains(&ws[s].key.as_str()) {
        s += 1;
    }
    s
}

/// End (exclusive) without trailing "please" / "now".
fn trail_end(ws: &[Word]) -> usize {
    let mut e = ws.len();
    while e > 1 && matches!(ws[e - 1].key.as_str(), "please" | "plz" | "pls" | "now") {
        e -= 1;
    }
    e
}

fn keys_join(ws: &[Word], a: usize, b: usize) -> String {
    ws[a.min(b)..b.min(ws.len())].iter().map(|w| w.key.as_str()).collect::<Vec<_>>().join(" ")
}

/// Words that may dangle before a cut-off trailing clause ("optimus prime *that* follows me").
fn dangling(k: &str) -> bool {
    matches!(k, "that" | "which" | "who" | "so" | "and" | "to")
}

/// "<thing> that follows me", "autopilot <thing>", "<thing> on its own": the phrase without the
/// follower words (None = no follower words, or nothing else left).
fn follower_cut(d: &IntentData, text: &str) -> Option<String> {
    let ws = words(text);
    let (s, e) = (lead_start(&ws), trail_end(&ws));
    for f in &d.follower_words {
        let k = f.split(' ').count();
        if e < s + k + 1 {
            continue;
        }
        if phrase_at(&ws, e - k, f) {
            let mut j = e - k;
            while j > s + 1 && dangling(&ws[j - 1].key) {
                j -= 1;
            }
            return Some(text[..ws[j - 1].end].trim().to_string());
        }
        // "autopilot tank", "a driverless car" (after an article the article stays)
        let p = if matches!(ws[s].key.as_str(), "a" | "an" | "the" | "my") { s + 1 } else { s };
        if p + k < e && phrase_at(&ws, p, f) {
            return Some(format!("{}{}", &text[..ws[p].start], &text[ws[p + k].start..]).trim().to_string());
        }
    }
    None
}

/// "<thing> i can pilot", "a car i can sit in", "a horse to ride": the phrase without the clause.
fn pilot_trailing_cut(d: &IntentData, text: &str) -> Option<String> {
    let ws = words(text);
    let (s, e) = (lead_start(&ws), trail_end(&ws));
    for t in &d.pilot_trailing {
        let k = t.split(' ').count();
        if e < s + k + 1 || !phrase_at(&ws, e - k, t) {
            continue;
        }
        let mut j = e - k;
        while j > s + 1 && dangling(&ws[j - 1].key) {
            j -= 1;
        }
        return Some(text[..ws[j - 1].end].trim().to_string());
    }
    None
}

/// Add / remove `Piloted` on both readings (construct and graft).
fn set_piloted(res: &mut Resolution, on: bool) {
    let rc = &mut res.recipe;
    if on {
        if !rc.has(Mechanic::Piloted) {
            rc.mechanics.push(Mechanic::Piloted);
            rc.mechanics.sort();
            rc.mechanics.dedup();
        }
        if !res.graft.mechanics.contains(&Mechanic::Piloted) {
            res.graft.mechanics.push(Mechanic::Piloted);
            res.graft.mechanics.sort();
        }
    } else {
        rc.mechanics.retain(|m| *m != Mechanic::Piloted);
        res.graft.mechanics.retain(|m| *m != Mechanic::Piloted);
    }
}

/// The "board" command reading (`GraftAction::Board`): nothing to build — the sim seats the caster in
/// the aimed (else nearest) construct they own (`bd_sim` `graft::Command::Board`).
fn board_resolution(res: &mut Resolution) {
    let mut r = Recipe::unknown(crate::BOARD_COMMAND);
    r.mechanics = vec![Mechanic::Piloted];
    res.recipe = r.normalized();
    res.kind = crate::ResolutionKind::Graft;
    res.graft = crate::Graft { mechanics: vec![Mechanic::Piloted], scale_mul: 1.0, action: Some(crate::GraftAction::Board), ..Default::default() };
    res.shape = None;
    res.alt_shape = None;
    res.signature = None;
    res.incantation = None;
    res.incantation_powers.clear();
    res.stand_rush = None;
    res.ignored.clear();
    res.confidence = 0.95;
    res.explanation = vec!["board \u{2192} take the wheel of your construct (the aimed one, else the nearest)".into()];
}

/// DEFAULT-PILOTED (owner rule, `data/intent.ron` `ride_default`): a single, undelivered summon whose
/// fantasy is being driven or ridden — a vehicle big enough to sit in, anything that Transforms, a
/// cockpit (mechs, titans, jets), a classic mount by name, or data that already says Piloted. Monsters
/// and animals you sic on people (wolves, kaiju, T-rex, a plain dragon), swarms, counts > 1, riders
/// on a mount, dropped things and held / worn things never are.
fn naturally_piloted(d: &IntentData, res: &Resolution) -> bool {
    let r = &res.recipe;
    if res.kind != crate::ResolutionKind::Construct
        || res.signature.is_some()
        || res.stand_rush.is_some()
        || r.count > 1
        || r.has(Mechanic::Swarm)
        || r.mount.is_some()
        || r.has(Mechanic::DropFromSky)
        || r.form == Form::Blob0
    {
        return false;
    }
    let cat = r.form.category();
    if matches!(cat, FormCategory::Held | FormCategory::Worn) {
        return false;
    }
    if r.has(Mechanic::Piloted) || r.has(Mechanic::Transforms) {
        return true;
    }
    let name = normalize_key(&r.name);
    if d.ride_default.iter().any(|m| name == *m || name.strip_suffix(m.as_str()).is_some_and(|p| p.ends_with(' '))) {
        return true;
    }
    let sits = r.scale >= d.ride_min_scale;
    (r.form == Form::Vehicle && sits) || (r.features.contains(&Feature::Cockpit) && cat != FormCategory::Structure && sits)
}

impl Lexicon {
    /// "get in", "hop in", "ride it", "let me drive", "take the wheel", "mount up", a bare "pilot" /
    /// "ride" / "drive": a command to board a construct you already own.
    fn board_command(&self, d: &IntentData, text: &str) -> bool {
        let ws = words(text);
        let (s, e) = (lead_start(&ws), trail_end(&ws));
        if s >= e {
            return false;
        }
        if d.board_commands.iter().any(|c| *c == keys_join(&ws, s, e)) {
            return true;
        }
        let Some(v) = d.pilot_verbs.iter().find(|v| phrase_at(&ws, s, v)) else { return false };
        let k = s + v.split(' ').count();
        if k >= e {
            // a bare "board" / "mount" / "steer" is more likely the noun
            return k == e && !matches!(v.as_str(), "board" | "mount" | "steer" | "sit in");
        }
        let rest = keys_join(&ws, k, e);
        d.board_rest.iter().any(|r| *r == rest)
    }

    /// "ride optimus prime", "drive a tank", "get in a mech": the rest after a leading pilot verb (None =
    /// no pilot verb, or the verb is part of a name: "pilot whale", "board game", "mount everest").
    fn pilot_verb_rest(&self, d: &IntentData, text: &str) -> Option<String> {
        let ws = words(text);
        let n = ws.len();
        let s = lead_start(&ws);
        let v = d.pilot_verbs.iter().find(|v| phrase_at(&ws, s, v))?;
        let mut from = s + v.split(' ').count();
        if from >= n {
            return None;
        }
        for extra in 1..=2usize.min(n - from) {
            if self.known_phrase(&keys_join(&ws, s, from + extra)) {
                return None;
            }
        }
        while from + 1 < n && matches!(ws[from].key.as_str(), "a" | "an" | "the" | "my" | "your" | "in" | "into" | "inside" | "on" | "onto" | "aboard") {
            from += 1;
        }
        Some(text[ws[from].start..].trim().to_string())
    }
}

impl Lexicon {
    /// Is this (normalised) text a known multi-word thing on its own ("fire truck", "rain boots",
    /// "drop kick")? Then a delivery verb inside it is not a verb.
    fn known_phrase(&self, k: &str) -> bool {
        if self.icons.alias.contains_key(k) {
            return true;
        }
        match self.entry(k) {
            Some((_, r)) => {
                use crate::data::{F_ANCHOR, F_COMMON, F_COMPOUND, F_ENTITY, F_POKEMON, F_POP};
                r.flags & (F_ANCHOR | F_COMPOUND | F_POP | F_ENTITY | F_POKEMON | F_COMMON) != 0 || r.rank_f() >= 3.0
            }
            None => false,
        }
    }

    /// Everyday word (for name matching): only an exact name may claim it.
    pub(crate) fn common_word(&self, k: &str) -> bool {
        if k.is_empty() {
            return false;
        }
        // (a curated icon name like "zoey" or "sam" is still a fine player name: icons don't count)
        if crate::parse::gram_of(k).is_some() {
            return true;
        }
        match self.entry(k) {
            Some((_, r)) => {
                use crate::data::{F_ANCHOR, F_COMMON, F_PERSON};
                // everyday words only: a first name ("ethan", "zoey", "maya") or a famous entity is not one
                r.flags & F_PERSON == 0 && r.flags & (F_COMMON | F_ANCHOR) != 0
            }
            None => false,
        }
    }

    /// "<prep> <word>" is a phrase of its own ("on fire", "on ice", "over the moon"): the word after a
    /// weak preposition is then not a player's name.
    pub(crate) fn prep_phrase(&self, k: &str) -> bool {
        let word = k.rsplit(' ').next().unwrap_or(k);
        self.known_phrase(k) || self.t.adj.contains_key(word) || self.t.pure.contains_key(word) || self.t.intens.contains_key(word)
    }

    fn delivery_verb<'d>(&self, d: &'d IntentData, text: &str, force: bool) -> Option<VerbHit<'d>> {
        let ws = words(text);
        let n = ws.len();
        let mut s = 0;
        while s + 1 < n && LEAD_FILLER.contains(&ws[s].key.as_str()) {
            s += 1;
        }
        let verb = d.verbs.iter().find(|v| phrase_at(&ws, s, &v.words))?;
        let k = verb.words.split(' ').count();
        let from = s + k;
        if from >= n {
            // "smite jake" → a lightning bolt; a bare "attack" / "smite" typed alone is left to the resolver
            let alone = verb.alone.as_ref().filter(|_| force)?;
            return Some(VerbHit { verb, rest: alone.clone() });
        }
        // "rain boots", "fire truck", "drop kick" (data: `not_verbs`)
        let two: String = ws[s..(from + 1).min(n)].iter().map(|w| w.key.as_str()).collect::<Vec<_>>().join(" ");
        if d.not_verbs.iter().any(|x| two == *x || two.strip_suffix('s') == Some(x.as_str())) {
            return None;
        }
        // "fire truck", "rain boots", "drop kick": the verb is part of a known name
        for extra in 1..=2usize.min(n - from) {
            let k: Vec<&str> = ws[s..from + extra].iter().map(|w| w.key.as_str()).collect();
            if self.known_phrase(&k.join(" ")) {
                return None;
            }
        }
        let next = ws[from].key.as_str();
        if verb.strict && !force {
            let det = DETERMINERS.contains(&next) || next.chars().all(|c| c.is_ascii_digit());
            let plural_word = |w: &str| w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") && self.known_token(&w[..w.len() - 1]);
            // "rain arrows", "rain bowling balls": the noun phrase ends in a plural
            let plural = plural_word(next) || ws[from..].last().is_some_and(|w| plural_word(&w.key));
            // "rain fire", "rain lightning": a volley verb before an element
            let element = verb.rewrite.is_some() && self.t.pure.contains_key(next);
            if !det && !plural && !element {
                return None;
            }
        }
        // the article goes too ("drop a whale" reads "whale", never the film "A Whale")
        let from = if from + 1 < n && matches!(next, "a" | "an" | "the") { from + 1 } else { from };
        let rest = text[ws[from].start..].trim().to_string();
        let rest = match &verb.rewrite {
            Some(t) => t.replace("{}", &rest),
            None => rest,
        };
        Some(VerbHit { verb, rest })
    }

    /// The staged reading of a phrase (see the module docs): refer → target → delivery verb → resolver.
    /// `names` = the *other* players' display names (targets are indices into it). Never fails.
    pub fn intent(&self, text: &str, names: &[&str], spoken: bool) -> Intent {
        let mut stages: Vec<String> = Vec::new();
        let mut cur = text.trim().to_string();
        let mut refer = None;
        // 1. refer
        match self.refer(&cur) {
            Some(Refer::Again { count }) => {
                stages.push("again".into());
                refer = Some(Refer::Again { count });
            }
            Some(Refer::It { rest }) => {
                stages.push("it".into());
                refer = Some(Refer::It { rest: rest.clone() });
                cur = rest;
            }
            None => {}
        }
        // 2. target
        let mut target = None;
        let mut heard_name = None;
        let mut name_dist = 0u8;
        let mut delivery = None;
        let mut forced_verb = false;
        let mut from_prep = false;
        if !matches!(refer, Some(Refer::Again { .. }))
            && let Some(t) = self.target_player(&cur, names, spoken)
        {
            stages.push("target".into());
            target = Some(t.player);
            heard_name = Some(t.heard);
            name_dist = t.dist;
            delivery = Some(t.via);
            forced_verb = t.verb.is_some();
            from_prep = t.verb.is_none();
            cur = t.rest;
        }
        // 3. delivery verb
        let mut verb_mech: Vec<Mechanic> = Vec::new();
        let mut verb_note = None;
        if let Some(d) = data()
            && !matches!(refer, Some(Refer::Again { .. }))
            && let Some(v) = self.delivery_verb(d, &cur, forced_verb)
        {
            stages.push(format!("verb:{}", v.verb.words));
            // a generic verb ("send bees after jake", "put a dome over zoe") keeps the preposition's delivery
            if !(v.verb.generic && delivery.is_some()) {
                delivery = Some(v.verb.via);
                from_prep = false;
            }
            verb_mech = v.verb.mech.clone();
            verb_note = Some(format!("{} \u{2192} delivered {}", v.verb.words, v.verb.via.name()));
            cur = v.rest;
        }
        // 3b. piloting: "get in" / "ride it" (board what you own), "ride / drive / pilot <thing>", "<thing> i
        // can pilot", "<thing> that follows me". The words come off: the verb's dictionary sense never grafts
        // ("pilot" is no aircraft pilot that Flies, "ride" / "drive" add no Blunt).
        let mut pilot = None;
        if let Some(d) = data()
            && !matches!(refer, Some(Refer::Again { .. }))
        {
            if self.board_command(d, &cur) {
                stages.push("board".into());
                pilot = Some(Piloting::Board);
            } else {
                if let Some(rest) = follower_cut(d, &cur) {
                    stages.push("follower".into());
                    pilot = Some(Piloting::Follower);
                    cur = rest;
                }
                if pilot.is_none() {
                    if let Some(rest) = self.pilot_verb_rest(d, &cur) {
                        stages.push("pilot".into());
                        pilot = Some(Piloting::Explicit);
                        cur = rest;
                    } else if let Some(rest) = pilot_trailing_cut(d, &cur) {
                        stages.push("pilot".into());
                        pilot = Some(Piloting::Explicit);
                        cur = rest;
                    }
                }
            }
        }
        // 4. the resolver (invocation/signature → pokémon → incantation → relations → compounds → grammar → leftovers)
        let rs = |t: &str| if spoken { self.resolve_spoken(t) } else { self.resolve(t) };
        let mut res = rs(&cur);
        // "meteor shower over the lake", "a wall on the bridge": a trailing *place* is where, not what —
        // when the words before it read clearly and the whole doesn't, the place goes (the crosshair aims).
        if target.is_none()
            && let Some(d) = data()
            && let Some(head) = location_head(d, &cur)
        {
            let h = rs(&head);
            let whole_weak = Lexicon::is_fallback(&res) || res.confidence + 0.15 < h.confidence;
            if whole_weak && !Lexicon::is_fallback(&h) {
                stages.push("location".into());
                cur = head;
                res = h;
            }
        }
        if res.signature.is_some() {
            stages.push("signature".into());
        }
        if res.incantation.is_some() {
            stages.push("incantation".into());
        }
        if res.recipe.mount.is_some() {
            stages.push("mount".into());
        }
        // delivery mechanics ("drop" → DropFromSky) on what was built
        for m in verb_mech {
            if !res.recipe.mechanics.contains(&m) {
                res.recipe.mechanics.push(m);
                res.recipe = res.recipe.clone().normalized();
            }
            if !res.graft.mechanics.contains(&m) {
                res.graft.mechanics.push(m);
                res.graft.mechanics.sort();
            }
        }
        // A preposition says where, the thing says how: a creature "on sam" appears next to Sam (it is not
        // dropped on him — "drop a dragon on sam" is); a dome / cage "over" or "on" someone goes around them.
        if from_prep && let Some(dv) = delivery {
            let f = res.recipe.form;
            let creature = f.category() == bd_core::mechanics::FormCategory::Creature;
            delivery = Some(match dv {
                Delivery::Onto if creature && !res.recipe.has(Mechanic::DropFromSky) => Delivery::Near,
                Delivery::Onto | Delivery::Near | Delivery::At if matches!(f, Form::Dome | Form::Cage) => Delivery::Around,
                other => other,
            });
        }
        // "wall around sam" is an enclosure
        if delivery == Some(Delivery::Around)
            && let Some(d) = data()
            && let Some((_, to)) = d.around_forms.iter().find(|(f, _)| crate::tables::form_from_name(f) == Some(res.recipe.form))
            && let Some(to) = crate::tables::form_from_name(to)
        {
            res.explanation.insert(0, format!("{} around \u{2192} {}", crate::tables::form_name(res.recipe.form), crate::tables::form_name(to)));
            res.recipe.form = to;
            if to == Form::Cage && !res.recipe.mechanics.contains(&Mechanic::Trap) {
                res.recipe.mechanics.push(Mechanic::Trap);
            }
            res.recipe = res.recipe.clone().normalized();
        }
        if let Some(n) = verb_note {
            res.explanation.insert(0, n);
        }
        // piloting: who drives it (explicit words, follower words, or the default for things you drive)
        match pilot {
            Some(Piloting::Board) => board_resolution(&mut res),
            Some(Piloting::Explicit) => {
                if !matches!(res.recipe.form.category(), FormCategory::Held | FormCategory::Worn) {
                    set_piloted(&mut res, true);
                    res.explanation.insert(0, "you pilot it (seated)".into());
                } else {
                    pilot = None;
                }
            }
            Some(Piloting::Follower) => {
                set_piloted(&mut res, false);
                res.explanation.insert(0, "follower \u{2192} it fights on its own".into());
            }
            Some(Piloting::Default) => {}
            None => {
                let plain = target.is_none() && delivery.is_none() && refer.is_none();
                if plain
                    && let Some(d) = data()
                    && naturally_piloted(d, &res)
                {
                    set_piloted(&mut res, true);
                    pilot = Some(Piloting::Default);
                    res.explanation.insert(0, "a thing you drive \u{2192} you pilot it (E hops out: it follows and fights)".into());
                } else if res.recipe.has(Mechanic::Piloted) && res.kind == crate::ResolutionKind::Construct {
                    pilot = Some(Piloting::Explicit);
                }
            }
        }
        if let (Some(t), Some(h)) = (target, heard_name.as_ref()) {
            let who = names.get(t).copied().unwrap_or("?");
            res.explanation.insert(0, format!("\"{h}\" \u{2192} player {who}{}", if name_dist > 0 { " (sounds like)" } else { "" }));
        }
        Intent { text: cur, refer, target, heard_name, name_dist, delivery, resolution: res, stages, pilot }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_parses() {
        let d = data().expect("intent.ron parses");
        assert!(d.verbs.len() > 40 && d.preps.len() > 20);
        for v in &d.target_verbs {
            assert!(d.verbs.iter().any(|x| x.words == *v) || v.contains(' '), "target verb {v} not in verbs");
        }
    }

    #[test]
    fn name_tokens_split() {
        assert_eq!(name_tokens("DragonSlayer"), ["dragon", "slayer"]);
        assert_eq!(name_tokens("Alex_99"), ["alex", "99"]);
        assert_eq!(name_tokens("xX_Nova_Xx"), ["x", "x", "nova", "xx"]);
        assert_eq!(name_tokens("Big Tony"), ["big", "tony"]);
        assert_eq!(name_tokens("ETHAN"), ["ethan"]);
    }

    #[test]
    fn sound_keys() {
        assert_eq!(sound_key("ethan"), sound_key("eathan"));
        assert_eq!(skeleton(&sound_key("ethan")), skeleton(&sound_key("eaton")));
        assert_eq!(sound_key("jake"), sound_key("jayk"));
        assert_eq!(sound_key("ethan"), sound_key("eaton"));
        assert_eq!(fold_numbers(&["alex", "ninety", "nine"]), "alex99");
    }
}
