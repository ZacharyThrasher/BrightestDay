//! Reference grammar — "it" means your last construct, "again!" re-casts, and a player's name after a
//! preposition aims at that player ("missile at Jake", "drop an anvil on Alex").
//!
//! The resolver cannot see game state, so everything here is a pure text step that runs **before**
//! [`crate::Lexicon::resolve`]: it reports the intent and hands back the rest of the phrase (cut from
//! the original text, so CAPS / "!!!" survive); the game decides what "it" is and which player a name
//! is, and falls back to casting the whole phrase when the intent doesn't apply ("never says no").
//!
//! * [`crate::Lexicon::refer`]: "make it bigger", "set it on fire", "give it wings", "put rockets on
//!   it", "ignite it", "it explodes", bare comparatives ("bigger!", "even faster") → [`Refer::It`];
//!   "again!", "another one", "one more", "two more", "3 more" → [`Refer::Again`].
//! * [`crate::Lexicon::target_player`]: "<phrase> at|on|onto|toward|after|against|on top of <name>"
//!   where `<name>` is one of the given display names (case-insensitive; typos and, for voice
//!   transcripts, sound-alikes accepted). The name words are removed from the phrase, so they can
//!   never resolve as nouns ("sam" never becomes a random entity).

use crate::fuzzy::{dl_distance, max_typo};
use crate::norm::normalize_key;
use crate::phonetic::phonetic_key;

/// What a phrase refers to (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refer {
    /// Apply `rest` to the caster's most recent live construct (graft path). `rest` is a slice of the
    /// original text (casing kept), possibly with a comparative rewritten ("faster" → "fast").
    It { rest: String },
    /// Re-cast the caster's last phrase. `count` = how many ("two more" → 2); `None` = as before.
    Again { count: Option<u8> },
}

/// A phrase aimed at a named player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Targeted {
    /// The phrase without the preposition and the name (original casing kept).
    pub rest: String,
    /// Index into the `names` slice given to [`crate::Lexicon::target_player`].
    pub player: usize,
}

/// One word of the input: byte span in the original text + normalised form.
#[derive(Clone, Debug)]
struct Word {
    start: usize,
    end: usize,
    key: String,
}

/// Split into words (alphanumeric runs, inner apostrophes kept), each with its normalised key.
/// Words that normalise to nothing are dropped. At most 24 words (the lexicon reads 16 tokens).
fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().take(256).collect();
    let limit = chars.last().map_or(0, |(b, c)| b + c.len_utf8());
    let mut start: Option<usize> = None;
    for (k, &(b, c)) in chars.iter().enumerate() {
        let inner_apos = matches!(c, '\'' | '\u{2019}') && start.is_some() && chars.get(k + 1).is_some_and(|(_, n)| n.is_alphanumeric());
        if c.is_alphanumeric() || inner_apos {
            if start.is_none() {
                start = Some(b);
            }
        } else if let Some(s) = start.take() {
            push_word(text, s, b, &mut out);
            if out.len() >= 24 {
                return out;
            }
        }
    }
    if let Some(s) = start {
        push_word(text, s, limit, &mut out);
    }
    out
}

fn push_word(text: &str, start: usize, end: usize, out: &mut Vec<Word>) {
    let key = normalize_key(&text[start..end]);
    if !key.is_empty() && !key.contains(' ') {
        out.push(Word { start, end, key });
    } else if !key.is_empty() {
        // "it's" normalises to "it"; anything that splits in two keeps its first half's span.
        let first = key.split(' ').next().unwrap_or_default().to_string();
        out.push(Word { start, end, key: first });
    }
}

/// Words that open a phrase without meaning anything ("ok now make it bigger").
const FILLER: &[&str] =
    &["ok", "okay", "now", "please", "and", "then", "oh", "hey", "yo", "quick", "quickly", "alright", "right", "so", "just", "go"];
/// Light verbs before a pronoun ("make it …", "give it …") or a graft ("put wings on it").
const LIGHT: &[&str] = &[
    "make", "set", "give", "let", "have", "turn", "get", "put", "add", "attach", "stick", "strap", "slap", "light", "paint", "coat",
    "cover",
];
const PRONOUN: &[&str] = &["it", "that", "them", "this", "those"];
/// Joiners between a light verb's pronoun and the rest that the resolver would only ignore.
const JOINERS: &[&str] = &["into", "to", "a", "an", "some", "be", "become", "more", "even", "much", "way", "lot", "super"];

/// Bare comparatives that mean "your last construct, but …" (canonical rest for the resolver).
const COMPARATIVE: &[(&str, &str)] = &[
    ("bigger", "bigger"),
    ("larger", "bigger"),
    ("huger", "huge"),
    ("biggerer", "huge"),
    ("taller", "bigger"),
    ("smaller", "smaller"),
    ("tinier", "smaller"),
    ("littler", "smaller"),
    ("faster", "fast"),
    ("quicker", "fast"),
    ("speedier", "fast"),
    ("heavier", "heavy"),
    ("stronger", "heavy"),
    ("angrier", "angry"),
    ("meaner", "angry"),
    ("hotter", "fire"),
    ("colder", "ice"),
    ("bouncier", "bouncy"),
    ("stickier", "sticky"),
];
/// Words allowed around a bare comparative ("even bigger", "way bigger still").
const INTENSIFIER: &[&str] = &["even", "much", "way", "lot", "a", "bit", "little", "more", "super", "still", "again", "and", "far", "lots"];

/// Number words for "two more" / "another three".
fn number(w: &str) -> Option<u8> {
    Some(match w {
        "one" | "a" | "an" => 1,
        "two" | "couple" | "pair" => 2,
        "three" | "few" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" | "dozen" => 12,
        _ => return w.parse::<u8>().ok().filter(|n| (1..=12).contains(n)),
    })
}

fn is(w: &Word, set: &[&str]) -> bool {
    set.contains(&w.key.as_str())
}

/// Parse a reference ("make it bigger", "again!", "two more"). `None` = an ordinary phrase.
pub(crate) fn refer(text: &str) -> Option<Refer> {
    let all = words(text);
    let mut ws: &[Word] = &all;
    while ws.len() > 1 && is(&ws[0], FILLER) {
        ws = &ws[1..];
    }
    while ws.len() > 1 && ws.last().is_some_and(|w| matches!(w.key.as_str(), "please" | "now" | "too" | "plz")) {
        ws = &ws[..ws.len() - 1];
    }
    if ws.is_empty() {
        return None;
    }
    if let Some(r) = again(ws) {
        return Some(r);
    }
    let keys: Vec<&str> = ws.iter().map(|w| w.key.as_str()).collect();
    let cut = |from: usize, to: usize| -> String {
        if from >= to || to > ws.len() {
            return String::new();
        }
        text[ws[from].start..ws[to - 1].end].trim().to_string()
    };
    // Bare comparative: "bigger!", "even bigger", "way faster".
    if ws.len() <= 4 {
        let comps: Vec<&(&str, &str)> = keys.iter().filter_map(|k| COMPARATIVE.iter().find(|(w, _)| w == k)).collect();
        if !comps.is_empty() && keys.iter().all(|k| INTENSIFIER.contains(k) || COMPARATIVE.iter().any(|(w, _)| w == k)) {
            let strong = keys.iter().any(|k| matches!(*k, "even" | "much" | "way" | "lot" | "lots" | "far" | "super"));
            let mut rest = comps.iter().map(|(_, canon)| *canon).collect::<Vec<_>>();
            rest.dedup();
            let rest = rest.join(" ");
            let rest = if strong && rest == "bigger" { "huge".to_string() } else { rest };
            return Some(Refer::It { rest: shout_case(text, &rest) });
        }
    }
    let n = ws.len();
    // "make it bigger", "set it on fire", "give it wings", "turn it into a dragon".
    if n >= 3 && is(&ws[0], LIGHT) && is(&ws[1], PRONOUN) {
        let mut from = 2;
        while from < n - 1 && is(&ws[from], JOINERS) {
            from += 1;
        }
        return it_rest(text, cut(from, n), &keys[from..]);
    }
    // "put wings on it", "add lasers to it", "slap rockets onto that".
    if n >= 3 && matches!(keys[n - 2], "on" | "onto" | "to" | "in" | "into") && is(&ws[n - 1], PRONOUN) {
        let from = if is(&ws[0], LIGHT) { 1 } else { 0 };
        return it_rest(text, cut(from, n - 2), &keys[from..n - 2]);
    }
    // "ignite it", "shrink it", "electrify it".
    if (2..=4).contains(&n) && keys[n - 1] == "it" {
        let from = if is(&ws[0], LIGHT) { 1 } else { 0 };
        return it_rest(text, cut(from, n - 1), &keys[from..n - 1]);
    }
    // "it explodes", "it's on fire", "it flies".
    if n >= 2 && keys[0] == "it" {
        let mut from = 1;
        while from < n - 1 && matches!(keys[from], "is" | "can" | "should" | "will" | "now" | "gets" | "get" | "becomes") {
            from += 1;
        }
        return it_rest(text, cut(from, n), &keys[from..]);
    }
    None
}

/// Rewrite a lone comparative the resolver doesn't know ("faster" → "fast").
fn it_rest(text: &str, rest: String, keys: &[&str]) -> Option<Refer> {
    if rest.is_empty() {
        return None;
    }
    if keys.len() == 1
        && let Some((_, canon)) = COMPARATIVE.iter().find(|(w, _)| *w == keys[0])
    {
        return Some(Refer::It { rest: shout_case(text, canon) });
    }
    Some(Refer::It { rest })
}

/// Keep the shout: an all-caps phrase keeps its canonical rest in caps.
fn shout_case(text: &str, canon: &str) -> String {
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let upper = text.chars().filter(|c| c.is_uppercase()).count();
    if letters >= 2 && upper * 2 > letters { canon.to_uppercase() } else { canon.to_string() }
}

/// "again", "another one", "one more", "two more", "3 more times", "do it again", "encore".
fn again(ws: &[Word]) -> Option<Refer> {
    let keys: Vec<&str> = ws.iter().map(|w| w.key.as_str()).collect();
    let k: Vec<&str> = keys.iter().copied().filter(|k| !matches!(*k, "times" | "time" | "please" | "again")).collect();
    let had_again = keys.contains(&"again");
    match k.as_slice() {
        [] if had_again => Some(Refer::Again { count: None }),
        ["encore"] | ["repeat"] | ["more"] | ["another"] | ["same"] | ["same", "thing"] | ["same", "again"] => {
            Some(Refer::Again { count: None })
        }
        ["do", p] | ["do", p, "over"] if PRONOUN.contains(p) && had_again => Some(Refer::Again { count: None }),
        ["once", "more"] => Some(Refer::Again { count: None }),
        ["one", "more"] | ["another", "one"] => Some(Refer::Again { count: Some(1) }),
        [n, "more"] | ["another", n] | [n, "more", "of", "those" | "them" | "these"] => number(n).map(|c| Refer::Again { count: Some(c) }),
        ["a", n, "more"] => number(n).filter(|c| *c > 1).map(|c| Refer::Again { count: Some(c) }),
        _ => None,
    }
}

/// Prepositions that aim a phrase at someone ("missile at Jake", "bees on Sam").
const AIM_PREP: &[&str] =
    &["at", "on", "onto", "toward", "towards", "after", "against", "vs", "versus", "upon", "over", "for", "to", "into"];
/// Words allowed after the name ("at Jake's head now!").
const AFTER_NAME: &[&str] = &["now", "please", "head", "face", "feet", "butt", "back", "again", "plz"];

/// Match a span of words against the display names. Returns (name index, edit distance).
fn match_name(span: &[Word], names: &[Vec<String>], spoken: bool, common_word: impl Fn(&str) -> bool) -> Option<(usize, usize)> {
    let text: String = span.iter().map(|w| w.key.as_str()).collect::<Vec<_>>().join(" ");
    let squashed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut best: Option<(usize, usize)> = None;
    for (i, toks) in names.iter().enumerate() {
        if toks.is_empty() {
            continue;
        }
        let full = toks.join(" ");
        let full_sq: String = toks.concat();
        let letters_only: String = full_sq.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        // Exact: the full name, the name without spaces / digits, or its first word (≥ 3 letters).
        // One word of a multi-word name ("tony" for "Big Tony"), 3+ letters.
        let word_of = |w: &str| toks.len() > 1 && toks.iter().any(|t| t.len() >= 3 && t.chars().all(|c| c.is_ascii_alphabetic()) && t == w);
        let exact = text == full
            || squashed == full_sq
            || (letters_only.len() >= 3 && squashed == letters_only)
            || (span.len() == 1 && word_of(&text));
        let d = if exact {
            Some(0)
        } else if common_word(&text) {
            None // "fire" stays fire unless a player is literally called Fire
        } else {
            let mut cands: Vec<&str> = vec![full_sq.as_str(), letters_only.as_str()];
            if toks.len() > 1 {
                cands.extend(toks.iter().map(String::as_str).filter(|t| t.len() >= 4));
            }
            let mut d_best: Option<usize> = None;
            for c in cands.iter().filter(|c| c.len() >= 3) {
                let limit = max_typo(c.len().max(squashed.len()), spoken);
                if limit > 0
                    && let Some(d) = dl_distance(squashed.as_bytes(), c.as_bytes(), limit)
                {
                    d_best = Some(d_best.map_or(d, |b: usize| b.min(d)));
                }
                // Voice: "jake" heard as "jay k" / "shake" — same consonant skeleton.
                if spoken && squashed.len() >= 3 && d_best.is_none() {
                    let (a, b) = (phonetic_key(&squashed), phonetic_key(c));
                    if a.len() >= 2 && a == b {
                        d_best = Some(1);
                    }
                }
            }
            d_best
        };
        if let Some(d) = d
            && best.is_none_or(|(_, bd)| d < bd)
        {
            best = Some((i, d));
        }
    }
    best
}

/// See [`crate::Lexicon::target_player`]. `common_word(key)` = the span is itself a known word (then
/// only an exact name match counts).
pub(crate) fn target_player(text: &str, names: &[&str], spoken: bool, common_word: impl Fn(&str) -> bool) -> Option<Targeted> {
    if names.is_empty() {
        return None;
    }
    let norm: Vec<Vec<String>> =
        names.iter().map(|n| normalize_key(n).split(' ').filter(|t| !t.is_empty()).take(4).map(String::from).collect()).collect();
    let ws = words(text);
    let n = ws.len();
    if n < 3 {
        return None;
    }
    // Scan preposition positions from the end (the aim is the phrase's tail): "<rest> <prep> <name> [tail]".
    for p in (1..n - 1).rev() {
        let name_from =
            if ws[p].key == "on" && ws.get(p + 1).is_some_and(|w| w.key == "top") && ws.get(p + 2).is_some_and(|w| w.key == "of") {
                p + 3
            } else if is(&ws[p], AIM_PREP) {
                p + 1
            } else {
                continue;
            };
        if name_from >= n {
            continue;
        }
        // The name is 1–3 words, followed only by harmless tail words.
        for len in (1..=3.min(n - name_from)).rev() {
            let tail = &ws[name_from + len..];
            if !tail.iter().all(|w| AFTER_NAME.contains(&w.key.as_str())) {
                continue;
            }
            if let Some((i, _)) = match_name(&ws[name_from..name_from + len], &norm, spoken, &common_word) {
                let rest = text[..ws[p].start].trim().trim_end_matches([',', '-', ':', ';']).trim().to_string();
                if rest.is_empty() {
                    return None;
                }
                return Some(Targeted { rest, player: i });
            }
        }
    }
    None
}

impl crate::Lexicon {
    /// "it" / "again" grammar — see [`Refer`]. Pure text; `None` for an ordinary phrase.
    pub fn refer(&self, text: &str) -> Option<Refer> {
        refer(text)
    }

    /// Player-name targeting ("missile at Jake"): `names` = the other players' display names. Returns
    /// the phrase without "at Jake" and which name matched. Case-insensitive; typos (and, when
    /// `spoken`, sound-alikes) accepted unless the word is itself a known noun ("sword on fire" never
    /// targets a player called "Fyre"; one literally called "Fire" still matches exactly).
    pub fn target_player(&self, text: &str, names: &[&str], spoken: bool) -> Option<Targeted> {
        target_player(text, names, spoken, |k| self.lookup(k).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(s: &str) -> Option<String> {
        match refer(s) {
            Some(Refer::It { rest }) => Some(rest),
            _ => None,
        }
    }

    #[test]
    fn it_phrases() {
        assert_eq!(it("make it bigger").as_deref(), Some("bigger"));
        assert_eq!(it("set it on fire").as_deref(), Some("on fire"));
        assert_eq!(it("give it wings").as_deref(), Some("wings"));
        assert_eq!(it("give it a jetpack").as_deref(), Some("jetpack"));
        assert_eq!(it("give it lasers!").as_deref(), Some("lasers"));
        assert_eq!(it("make it fly").as_deref(), Some("fly"));
        assert_eq!(it("make it EXPLODE!!!").as_deref(), Some("EXPLODE"));
        assert_eq!(it("make it bouncy").as_deref(), Some("bouncy"));
        assert_eq!(it("make it faster").as_deref(), Some("fast"));
        assert_eq!(it("turn it into a dragon").as_deref(), Some("dragon"));
        assert_eq!(it("put rockets on it").as_deref(), Some("rockets"));
        assert_eq!(it("ignite it").as_deref(), Some("ignite"));
        assert_eq!(it("it explodes").as_deref(), Some("explodes"));
        assert_eq!(it("it's on fire").as_deref(), Some("on fire"));
        assert_eq!(it("ok now make it bigger please").as_deref(), Some("bigger"));
        assert_eq!(it("bigger!").as_deref(), Some("bigger"));
        assert_eq!(it("BIGGER!!!").as_deref(), Some("BIGGER"));
        assert_eq!(it("even bigger").as_deref(), Some("huge"));
        assert_eq!(it("way faster").as_deref(), Some("fast"));
        assert_eq!(it("smaller").as_deref(), Some("smaller"));
    }

    #[test]
    fn ordinary_phrases_are_not_references() {
        for s in [
            "hammer",
            "bigger hammer",
            "a sword that returns",
            "king kong on a skateboard",
            "make",
            "it",
            "flaming sword",
            "three swords",
            "lighter",
            "missile at jake",
            "hit",
        ] {
            assert_eq!(refer(s), None, "{s}");
        }
    }

    #[test]
    fn again_phrases() {
        assert_eq!(refer("again!"), Some(Refer::Again { count: None }));
        assert_eq!(refer("AGAIN AGAIN"), Some(Refer::Again { count: None }));
        assert_eq!(refer("do it again"), Some(Refer::Again { count: None }));
        assert_eq!(refer("another one"), Some(Refer::Again { count: Some(1) }));
        assert_eq!(refer("one more"), Some(Refer::Again { count: Some(1) }));
        assert_eq!(refer("two more"), Some(Refer::Again { count: Some(2) }));
        assert_eq!(refer("3 more!"), Some(Refer::Again { count: Some(3) }));
        assert_eq!(refer("another five"), Some(Refer::Again { count: Some(5) }));
        assert_eq!(refer("a couple more"), Some(Refer::Again { count: Some(2) }));
        assert_eq!(refer("encore"), Some(Refer::Again { count: None }));
        assert_eq!(refer("two more swords"), None, "a new phrase, not a re-cast");
    }

    const NAMES: [&str; 4] = ["Jake", "Sam", "Alex_99", "Big Tony"];

    fn aim(s: &str, spoken: bool) -> Option<(String, &'static str)> {
        target_player(s, &NAMES, spoken, |k| matches!(k, "fire" | "sword" | "hammer")).map(|t| (t.rest, NAMES[t.player]))
    }

    #[test]
    fn name_targeting() {
        assert_eq!(aim("missile at Jake", false), Some(("missile".into(), "Jake")));
        assert_eq!(aim("bees on sam", false), Some(("bees".into(), "Sam")));
        assert_eq!(aim("drop an anvil on Alex", false), Some(("drop an anvil".into(), "Alex_99")));
        assert_eq!(aim("drop an anvil on top of alex99!", false), Some(("drop an anvil".into(), "Alex_99")));
        assert_eq!(aim("GIANT HAMMER AT JAKE!!!", false), Some(("GIANT HAMMER".into(), "Jake")));
        assert_eq!(aim("sharks after big tony", false), Some(("sharks".into(), "Big Tony")));
        assert_eq!(aim("sharks after tony now", false), Some(("sharks".into(), "Big Tony")));
        assert_eq!(aim("rockets at jkae", false), Some(("rockets".into(), "Jake")), "typo");
        assert_eq!(aim("missile at jake's head", false), Some(("missile".into(), "Jake")));
    }

    #[test]
    fn name_targeting_voice() {
        assert_eq!(aim("missile at jay k", true), Some(("missile".into(), "Jake")));
        assert_eq!(aim("bees on sam.", true), Some(("bees".into(), "Sam")));
    }

    #[test]
    fn no_false_targets() {
        assert_eq!(aim("sword on fire", false), None);
        assert_eq!(aim("king kong on a skateboard", false), None);
        assert_eq!(aim("at jake", false), None, "nothing left to cast: an ordinary phrase");
        assert_eq!(aim("missile at the moon", false), None);
        assert_eq!(target_player("missile at jake", &[], false, |_| false), None);
    }

    #[test]
    fn never_panics_on_odd_input() {
        for s in ["", " ", "it", "'''", "at", "on on on on", "é ü ñ at sam", "\u{2019}s it", &"it ".repeat(300), &"x".repeat(2000)] {
            let _ = refer(s);
            let _ = target_player(s, &NAMES, true, |_| false);
        }
    }
}
