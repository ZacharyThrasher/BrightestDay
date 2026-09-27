//! VOICE FLUENCY: what the ring hears when you SPEAK — the Whisper transcript → the line the
//! console reads (voice == typing: after this step a spoken line goes through exactly the typed
//! path, `console::Caster::cast_heard` → `toolbox::command_for` / the lexicon).
//!
//! * [`bias_prompt`]: Whisper is primed with the names players shout most (the moves / characters
//!   the data spells the most ways — every mishearing a content worker added is a vote), so
//!   "Rasengan" comes out as *Rasengan*, not "raising gun".
//! * [`heard_line`]: what Whisper still gets wrong is repaired PIECE BY PIECE (a chain's steps each
//!   on their own), and only where the piece would otherwise mean nothing — a piece that already
//!   reads as a move, a described move, a twist or a clean ordinary thing is never touched, so
//!   plain construct phrases are never hijacked (SHOUT HYGIENE holds):
//!   - drawn-out delivery ("rasengaaaan", "kaaamehaaameha") is squeezed back;
//!   - a chant Whisper's loop-guard cut short ("gum pistol", "ora") gets its repeat back when the
//!     repeated form is a move ("gum gum pistol", "ora ora ora");
//!   - a piece spelled by sound ("raising gun", "rhinehart ultimate", "chi dory") becomes the move
//!     it sounds like, by the lexicon's phonetic key, only when the match is unambiguous;
//!   - a chain said without a separator Whisper could hear ("nightcrawler teleport reinhardt
//!     ultimate") is cut into its moves when EVERY piece is a named move.
//! * [`Beats`]: STREAMING — each beat of a spoken chain fires as soon as it is heard.
//!
//! Measured by the voice bench (tools/voice_bench/README.md: TTS corpus → the real Whisper path →
//! this → the real toolbox reader).

use std::collections::HashMap;
use std::sync::OnceLock;

use bd_lexicon::{phonetic_key, Lexicon};
use bd_sim::powers::toolbox::{split_chain_with, MoveIndex, MoveRef};
use bd_sim::powers::{norm_words, PowerTable};

use crate::toolbox;

/// Whisper's biasing prompt: the ring's curated names (a few), the player's `oath` words, then the
/// most-shouted character and move names (last = weighed most; voice_ui appends player names after).
/// Whisper keeps at most 224 prompt tokens: the list stays well under that.
pub fn bias_prompt(lex: &Lexicon, oath: &str) -> String {
    let (kits, moves) = shouted_names(lex);
    let mut out: Vec<String> = Vec::new();
    let icons = lex.bias_prompt(16);
    if !icons.is_empty() {
        out.push(icons);
    }
    if !oath.trim().is_empty() {
        out.push(oath.trim().to_string());
    }
    out.push(kits.join(", "));
    out.push(moves.join(", "));
    out.join(", ")
}

/// Title Case of a normalised phrase ("getsuga tensho" → "Getsuga Tensho").
fn title(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The character names and move names players shout most, most-shouted LAST: coined words only
/// (Whisper spells everyday words fine), ranked by how many ways the hand-written data spells them.
fn shouted_names(lex: &Lexicon) -> (Vec<String>, Vec<String>) {
    const KITS: usize = 22;
    const MOVES: usize = 44;
    let t = PowerTable::get();
    let coined = |s: &str| s.split(' ').any(|w| w.len() > 2 && !lex.is_everyday_word(w));
    let mut moves: HashMap<String, usize> = HashMap::new();
    let mut kits: HashMap<String, usize> = HashMap::new();
    for p in t.powers.iter().filter(|p| p.derived.is_none()) {
        let mut per_slot: HashMap<u8, (usize, Option<&str>)> = HashMap::new();
        for (s, slot) in &p.shouts {
            let e = per_slot.entry(*slot).or_default();
            e.0 += 1;
            if e.1.is_none() && t.proper_shouts.contains(s) {
                e.1 = Some(s.as_str());
            }
        }
        for (n, s) in per_slot.into_values() {
            if let Some(s) = s.filter(|s| coined(s) && s.split(' ').count() <= 3) {
                let e = moves.entry(s.to_string()).or_default();
                *e = (*e).max(n);
            }
        }
        if p.kind.is_kit() {
            let name = norm_words(&p.name);
            if coined(&name) && name.split(' ').count() <= 2 {
                kits.insert(name, p.names.len());
            }
        }
    }
    let top = |m: HashMap<String, usize>, n: usize| -> Vec<String> {
        let mut v: Vec<(String, usize)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut out: Vec<String> = v.into_iter().take(n).map(|(s, _)| title(&s)).collect();
        out.reverse(); // most-shouted last
        out
    };
    (top(kits, KITS), top(moves, MOVES))
}

// ------------------------------------------------------------------------------------------------
// The heard line
// ------------------------------------------------------------------------------------------------

/// A spoken transcript → the line to read (see the module doc). Pure; ~1 ms for a line that
/// already reads, a few ms for one that needs repair.
pub fn heard_line(lex: &Lexicon, text: &str) -> String {
    let said = text.trim().trim_matches(',').trim();
    if said.is_empty() {
        return String::new();
    }
    MEMO.with(|m| m.borrow_mut().clear());
    let line = squeeze_elongation(lex, said);
    // Whisper's commas inside ONE name ("Amador, Ray, Suge" for Amaterasu, "Nightcrawler, teleport"):
    // a line that is one named move without them is that move
    if line.contains(',') && !split_is_all_moves(lex, &line) {
        let plain = line.replace(',', " ");
        if named_move(lex, &plain) {
            return norm_words(&plain);
        }
        if let Some(f) = fix_piece(lex, &plain).filter(|f| named_move(lex, f)) {
            return f;
        }
    }
    let pieces = split_chain_with(&line, &|s: &str| piece_is_move(lex, s) || fix_piece(lex, s).is_some(), Some(&|s: &str| toolbox::understood(lex, s, true)));
    if pieces.len() == 1 && toolbox::command(lex, None, true, &line, true).is_some_and(|s| s.len() > 1) {
        return line; // already a chain as heard
    }
    if pieces.len() > 1 {
        let fixed: Vec<String> = pieces.iter().map(|p| fix_piece(lex, p).unwrap_or_else(|| p.clone())).collect();
        if fixed == pieces {
            return line;
        }
        return fixed.join(" then ");
    }
    if let Some(f) = fix_piece(lex, &line) {
        return f;
    }
    if piece_ok(lex, &line) {
        return line;
    }
    // no separator Whisper could hear: cut into named moves, if EVERY piece is one
    if let Some(parts) = segment(lex, &line) {
        return parts.join(" then ");
    }
    line
}

/// The line's comma pieces are each a power move already (a real spoken chain).
fn split_is_all_moves(lex: &Lexicon, line: &str) -> bool {
    let parts: Vec<&str> = line.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    parts.len() > 1 && parts.iter().all(|p| piece_is_move(lex, p))
}

/// "rasengaaaan" → "rasengan", "kaaamehaaameha" → "kamehameha", "booom" → "boom": a letter held
/// for 3+ is squeezed to one or two, whichever reads better (a move, else a clean thing).
fn squeeze_elongation(lex: &Lexicon, s: &str) -> String {
    let squeeze = |max: usize| -> String {
        let mut out = String::with_capacity(s.len());
        let (mut last, mut run) = ('\0', 0usize);
        for c in s.chars() {
            run = if c == last { run + 1 } else { 1 };
            last = c;
            if run <= max || !c.is_alphabetic() {
                out.push(c);
            }
        }
        out
    };
    let two = squeeze(2);
    if two == s {
        return two; // nothing held
    }
    let one = squeeze(1);
    let good = |l: &str| line_reads(lex, l);
    if good(&one) && !good(&two) { one } else { two }
}

/// Does a line read as something (a move, a chain, a described move, a clean thing)?
fn line_reads(lex: &Lexicon, l: &str) -> bool {
    toolbox::command(lex, None, true, l, true).is_some() || toolbox::understood(lex, l, true)
}

/// A piece already means something on its own.
fn piece_ok(lex: &Lexicon, p: &str) -> bool {
    piece_is_move(lex, p) || toolbox::understood(lex, p, true)
}

/// A piece reads as a power move (named, kit, held, twisted, described).
fn piece_is_move(lex: &Lexicon, p: &str) -> bool {
    toolbox::command(lex, None, true, p, true).is_some_and(|s| s.len() == 1 && !matches!(s[0], bd_sim::powers::chain::ToolStep::Cast { .. }))
}

/// A NAMED move as TYPED (a shout / kit move / held move / twist — what a repair must land on).
fn named_move(lex: &Lexicon, p: &str) -> bool {
    toolbox::read_step(lex, None, true, p, false).is_move() || toolbox::twist_step(lex, None, true, p, false, None).is_some()
}

/// A power move as TYPED, described moves included (what a spelling variant may land on).
fn typed_move(lex: &Lexicon, p: &str) -> bool {
    toolbox::command(lex, None, true, p, false).is_some_and(|s| s.len() == 1 && !matches!(s[0], bd_sim::powers::chain::ToolStep::Cast { .. }))
}

/// Repair of one piece → the typed phrase it most likely was, or `None` (keep it as heard).
///
/// * Whisper's loop guard (bd_voice) cut a chant short: "gum pistol" / "captain falcon punch" /
///   "ora" are exactly the collapse of "gum gum pistol" / "captain falcon falcon punch" / "ora
///   ora ora" → the chant (even when the short form reads as something: it is textual evidence).
/// * The piece means nothing as heard: Whisper's spelling habits (digits "8 gates", dotted
///   acronyms "p k thunder", "mr" / "dr", a leading "and" / "the", a trailing "a a a"), then the
///   sound-alike index (shouts, "<kit> <move>" phrases, a kit name + the rest).
/// * The piece reads as an ordinary thing but sounds EXACTLY like a coined famous name ("raising
///   gun" → Rasengan): the name. Only coined names (not everyday words) and only exact keys, so
///   "flaming hammer" can never become a move.
fn fix_piece(lex: &Lexicon, p: &str) -> Option<String> {
    // (memoised per heard line: the splitter, the pieces and the cutter ask about the same spans)
    let key = p.trim().to_string();
    if let Some(hit) = MEMO.with(|m| m.borrow().get(&key).cloned()) {
        return hit;
    }
    let r = fix_piece_uncached(lex, p);
    MEMO.with(|m| m.borrow_mut().insert(key, r.clone()));
    r
}

thread_local! {
    static MEMO: std::cell::RefCell<HashMap<String, Option<String>>> = std::cell::RefCell::new(HashMap::new());
}

fn fix_piece_uncached(lex: &Lexicon, p: &str) -> Option<String> {
    let n = norm_words(p);
    let words: Vec<&str> = n.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() || words.len() > 8 {
        return None;
    }
    if piece_is_move(lex, p) {
        return None;
    }
    let ix = sounds(lex);
    let chant = |s: &str| ix.chants.get(s).filter(|c| named_move(lex, c)).cloned();
    // a phrase the ring reads plainly and confidently (typed, nothing ignored) is an ordinary
    // thing: only an exact coined name may replace it, never a fuzzy match ("shadow ball" is not
    // "shoot the apple")
    let clean = toolbox::understood(lex, p, true);
    // (a one-word short form that reads as a clean thing stays the thing: "katamari" is the summon)
    if (words.len() >= 2 || !clean)
        && let Some(c) = chant(&n)
    {
        return Some(c);
    }
    let understood = clean || {
        let r = lex.resolve(p);
        !Lexicon::is_fallback(&r) && r.ignored.is_empty() && r.confidence >= 0.85
    };
    if understood {
        return ix.coined_like(&n).filter(|s| named_move(lex, s));
    }
    let variants = spelling_variants(&words);
    for v in &variants {
        if typed_move(lex, v) {
            return Some(v.clone());
        }
        if let Some(c) = chant(v) {
            return Some(c);
        }
    }
    let mut tries: Vec<&str> = vec![n.as_str()];
    tries.extend(variants.iter().map(String::as_str));
    if let Some(r) = tries.iter().find_map(|t| respell(lex, t)) {
        return Some(r);
    }
    repair_spans(lex, &words)
}

/// Part of a piece misheard ("homing shadori", "giant raise nong", "reverse earth shutter",
/// "chidori plus raiz nahn"): a span of 1–3 words replaced by the shout it sounds like, the rest
/// kept, accepted only when the whole then reads as a move typed. Up to two spans.
fn repair_spans(lex: &Lexicon, words: &[&str]) -> Option<String> {
    if words.len() < 2 || words.len() > 7 {
        return None;
    }
    let ix = sounds(lex);
    let mut best: Option<Vec<String>> = None;
    'outer: for len in (1..=3.min(words.len() - 1)).rev() {
        for a in 0..=words.len() - len {
            let span = words[a..a + len].join(" ");
            let Some(fix) = ix.chants.get(&span).cloned().or_else(|| ix.phrase_like(&span)).filter(|f| *f != span) else { continue };
            let mut w: Vec<String> = words[..a].iter().map(|s| s.to_string()).collect();
            w.push(fix);
            w.extend(words[a + len..].iter().map(|s| s.to_string()));
            let line = w.join(" ");
            if typed_move(lex, &line) {
                return Some(line);
            }
            if best.is_none() {
                best = Some(w);
                break 'outer;
            }
        }
    }
    // a second span (two names misheard: "raised non plus amateur a soot")
    let w = best?;
    let ws: Vec<&str> = w.iter().map(String::as_str).collect();
    let flat = ws.join(" ");
    let flat_words: Vec<&str> = flat.split(' ').collect();
    for len in (1..=3.min(flat_words.len() - 1)).rev() {
        for a in 0..=flat_words.len() - len {
            let span = flat_words[a..a + len].join(" ");
            let Some(fix) = ix.chants.get(&span).cloned().or_else(|| ix.phrase_like(&span)).filter(|f| *f != span) else { continue };
            let line = [flat_words[..a].join(" "), fix, flat_words[a + len..].join(" ")].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ");
            if typed_move(lex, &line) {
                return Some(line);
            }
        }
    }
    None
}

/// How players TYPE what Whisper writes otherwise: number words, joined acronyms, "mister" /
/// "doctor", no leading "and" / "the", no drawn-out "a a a" at the end. The all-at-once variant
/// first, then each on its own.
fn spelling_variants(words: &[&str]) -> Vec<String> {
    fn number(w: &str) -> Option<&'static str> {
        Some(match w {
            "0" => "zero",
            "1" | "1st" => if w == "1" { "one" } else { "first" },
            "2" => "two",
            "2nd" => "second",
            "3" => "three",
            "3rd" => "third",
            "4" => "four",
            "4th" => "fourth",
            "5" => "five",
            "5th" => "fifth",
            "6" => "six",
            "6th" => "sixth",
            "7" => "seven",
            "7th" => "seventh",
            "8" => "eight",
            "8th" => "eighth",
            "9" => "nine",
            "9th" => "ninth",
            "10" => "ten",
            "10th" => "tenth",
            "11" => "eleven",
            "12" => "twelve",
            "20" => "twenty",
            "50" => "fifty",
            "100" => "hundred",
            "1000" => "thousand",
            _ => return None,
        })
    }
    fn abbrev(w: &str) -> Option<&'static str> {
        Some(match w {
            "mr" => "mister",
            "dr" => "doctor",
            "st" => "saint",
            "jr" => "junior",
            "sr" => "senior",
            "vs" => "versus",
            "lt" => "lieutenant",
            "sgt" => "sergeant",
            "capt" => "captain",
            _ => return None,
        })
    }
    let filler_head = |w: &str| matches!(w, "and" | "the" | "a" | "an" | "uh" | "um" | "so" | "oh" | "then");
    let drawn_tail = |w: &str| w.len() <= 4 && w.chars().all(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'h' | 'y'));
    let nums: Vec<String> = words.iter().map(|w| number(w).map(str::to_string).unwrap_or_else(|| w.to_string())).collect();
    let abbr: Vec<String> = words.iter().map(|w| abbrev(w).map(str::to_string).unwrap_or_else(|| w.to_string())).collect();
    // "p k thunder" → "pk thunder" (runs of single letters, not the article "a")
    let join_letters = |ws: &[&str]| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut run = false;
        for w in ws {
            let single = w.len() == 1 && w.chars().all(|c| c.is_ascii_alphabetic()) && *w != "a";
            match out.last_mut() {
                Some(last) if single && run => last.push_str(w),
                _ => out.push(w.to_string()),
            }
            run = single;
        }
        out
    };
    let acro = join_letters(words);
    let head = words.iter().position(|w| !filler_head(w)).unwrap_or(words.len());
    let tail = words.iter().rposition(|w| !drawn_tail(w)).map_or(0, |i| i + 1);
    let trimmed: Vec<String> = if head < tail { words[head..tail].iter().map(|w| w.to_string()).collect() } else { Vec::new() };
    let all: Vec<String> = {
        let t: Vec<&str> = if head < tail { words[head..tail].iter().map(|w| number(w).or_else(|| abbrev(w)).unwrap_or(w)).collect() } else { Vec::new() };
        join_letters(&t)
    };
    let orig = words.join(" ");
    let mut v: Vec<String> = Vec::new();
    for cand in [all, nums, abbr, acro, trimmed] {
        let s = cand.join(" ");
        if !s.is_empty() && s != orig && !v.contains(&s) {
            v.push(s);
        }
    }
    v
}

/// Sound-alike repair: the whole piece ~ one shout or "<kit> <move>" phrase, or a kit name ~ the
/// first words and the rest a move of that kit.
fn respell(lex: &Lexicon, whole: &str) -> Option<String> {
    let ix = sounds(lex);
    let words: Vec<&str> = whole.split(' ').filter(|w| !w.is_empty()).collect();
    if let Some(s) = ix.phrase_like(whole)
        && named_move(lex, &s)
    {
        return Some(s);
    }
    // "rhinehart ultimate", "night crawler dash", "naruto raising gun"
    for k in (1..=3.min(words.len().saturating_sub(1))).rev() {
        let head = words[..k].join(" ");
        let rest = words[k..].join(" ");
        for (kit, pid) in ix.kits_like(&head) {
            let cand = format!("{kit} {rest}");
            if named_move(lex, &cand) {
                return Some(cand);
            }
            if let Some(mv) = ix.kit_move_like(pid, &rest) {
                let cand = format!("{kit} {mv}");
                if named_move(lex, &cand) {
                    return Some(cand);
                }
            }
        }
    }
    None
}

/// A line with no separator cut into 2–4 NAMED moves (each exact or repaired), fewest pieces first.
fn segment(lex: &Lexicon, line: &str) -> Option<Vec<String>> {
    let n = norm_words(line);
    let w: Vec<&str> = n.split(' ').filter(|w| !w.is_empty()).collect();
    if w.len() < 2 || w.len() > 10 {
        return None;
    }
    let mut memo: HashMap<(usize, usize), Option<String>> = HashMap::new();
    let mut piece = |a: usize, b: usize| -> Option<String> {
        memo.entry((a, b))
            .or_insert_with(|| {
                let s = w[a..b].join(" ");
                if named_move(lex, &s) { Some(s) } else { fix_piece(lex, &s).filter(|f| named_move(lex, f)) }
            })
            .clone()
    };
    // best[k] = fewest pieces covering w[..k]
    let mut best: Vec<Option<Vec<String>>> = vec![None; w.len() + 1];
    best[0] = Some(Vec::new());
    for b in 1..=w.len() {
        // (a move name is at most 5 words)
        for a in b.saturating_sub(5)..b {
            let Some(prev) = best[a].clone() else { continue };
            if prev.len() >= 4 {
                continue;
            }
            if let Some(p) = piece(a, b) {
                let mut v = prev;
                v.push(p);
                if best[b].as_ref().is_none_or(|cur| v.len() < cur.len()) {
                    best[b] = Some(v);
                }
            }
        }
    }
    best.pop().flatten().filter(|v| v.len() >= 2)
}

/// Longest phrase key kept apart by length (longer ones share the last bucket).
const MAX_KEY: usize = 40;

/// Phonetic index of every borrowable shout, every "<kit> <move>" phrase and every kit name
/// (built once, off the game thread by [`warm`]).
struct Sounds {
    /// (key, normalised phrase, move) — shouts and kit phrases
    phrases: Vec<(String, String, MoveRef)>,
    by_key: HashMap<String, Vec<usize>>,
    /// key length → indices into `phrases`
    by_len: Vec<Vec<usize>>,
    /// indices of shouts with a coined (non-dictionary) word — the only ones a clean thing may
    /// turn into, by exact key
    coined: HashMap<String, Vec<usize>>,
    /// (key, kit name, power)
    kits: Vec<(String, String, u16)>,
    /// Whisper's loop guard collapse of a chant → the chant ("gum pistol" → "gum gum pistol")
    chants: HashMap<String, String>,
}

/// Build the voice indices now (call from a background thread when voice starts; the first
/// spoken line would otherwise pay ~0.1–0.3 s).
pub fn warm(lex: &Lexicon) {
    let _ = sounds(lex);
}

fn sounds(lex: &Lexicon) -> &'static Sounds {
    static S: OnceLock<Sounds> = OnceLock::new();
    S.get_or_init(|| {
        let t = PowerTable::get();
        let ix = MoveIndex::shipped();
        let mut phrases: Vec<(String, String, MoveRef)> = ix.shouts().map(|(s, m)| (long_key(s), s.to_string(), m)).collect();
        // "<kit name> <move name>" and "<kit name> ultimate"
        let mut kits: Vec<(String, String, u16)> = Vec::new();
        for (name, pid) in ix.kit_names() {
            kits.push((long_key(name), name.to_string(), pid));
            let Some(def) = t.def(pid) else { continue };
            for (slot, m) in def.moves.iter().enumerate() {
                let p = format!("{name} {}", norm_words(&m.name));
                phrases.push((long_key(&p), p, MoveRef::new(pid, 0, slot as u8)));
            }
            let p = format!("{name} ultimate");
            if let Some((m, _)) = ix.kit_move(t, &p) {
                phrases.push((long_key(&p), p, m));
            }
        }
        phrases.retain(|(k, _, _)| k.len() >= 3);
        phrases.sort_by(|a, b| a.2.cmp(&b.2).then(a.1.cmp(&b.1)));
        phrases.dedup_by(|a, b| a.0 == b.0 && a.2 == b.2);
        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_len: Vec<Vec<usize>> = vec![Vec::new(); MAX_KEY + 1];
        let mut chants: HashMap<String, String> = HashMap::new();
        for (i, (k, p, _)) in phrases.iter().enumerate() {
            by_key.entry(k.clone()).or_default().push(i);
            by_len[k.len().min(MAX_KEY)].push(i);
            let c = bd_voice::text::normalize_transcript(p);
            if c != *p && !c.is_empty() {
                chants.entry(c).or_insert_with(|| p.clone());
            }
        }
        let everyday = |w: &str| lex.is_everyday_word(w);
        let mut coined: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, (k, p, _)) in phrases.iter().enumerate() {
            if k.len() >= 5 && ix.shout(p).is_some() && t.proper_shouts.contains(p) && p.split(' ').any(|w| w.len() >= 5 && !everyday(w)) {
                coined.entry(k.clone()).or_default().push(i);
            }
        }
        kits.sort_by(|a, b| a.2.cmp(&b.2).then(a.1.cmp(&b.1)));
        Sounds { phrases, by_key, by_len, coined, kits, chants }
    })
}

/// The phonetic key of a whole phrase: `phonetic_key` stops at 12 symbols, so long phrases are keyed
/// in word groups of at most 10 symbols each, joined ("sentinel marvel disciplined stomp" and the
/// same with "… then consecutive normal punches" must not share a key).
fn long_key(s: &str) -> String {
    let mut out = String::new();
    let mut group = String::new();
    for w in s.split(' ').filter(|w| !w.is_empty()) {
        let with = if group.is_empty() { w.to_string() } else { format!("{group} {w}") };
        if !group.is_empty() && phonetic_key(&with).len() > 10 {
            out.push_str(&phonetic_key(&group));
            group = w.to_string();
        } else {
            group = with;
        }
    }
    out.push_str(&phonetic_key(&group));
    out
}

/// Edit distance of two keys, capped at `cap + 1`.
fn key_dist(a: &str, b: &str, cap: usize) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len().abs_diff(b.len()) > cap {
        return cap + 1;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        let mut row_min = cur[0];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
            row_min = row_min.min(cur[j]);
        }
        if row_min > cap {
            return cap + 1;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// How far a key may be from a name's key and still be that name: exact under 4 symbols, one edit
/// from 4, two from 9, three from 16 (Whisper's misspellings of long names drift further).
fn allowed(len: usize) -> usize {
    match len {
        0..=3 => 0,
        4..=8 => 1,
        9..=15 => 2,
        _ => 3,
    }
}

/// Letters of a phrase (spaces aside) — a sound-alike must be about as long as what was heard.
fn letters(s: &str) -> usize {
    s.chars().filter(|c| c.is_alphanumeric()).count()
}

impl Sounds {
    /// Unambiguous pick among candidate phrases: every one fires the same move; the one spelled
    /// most like what was heard.
    fn pick(&self, cands: &[usize], heard: &str) -> Option<String> {
        let mv = self.phrases[*cands.first()?].2;
        if !cands.iter().all(|&i| self.phrases[i].2 == mv) {
            return None;
        }
        cands.iter().map(|&i| &self.phrases[i].1).min_by_key(|c| (key_dist(c, heard, 64), c.len())).cloned()
    }

    /// The shout / "<kit> <move>" phrase `s` sounds like.
    fn phrase_like(&self, s: &str) -> Option<String> {
        let key = long_key(s);
        if key.len() < 4 {
            return None;
        }
        let n = letters(s);
        let near = |i: &usize| {
            let m = letters(&self.phrases[*i].1);
            m * 5 >= n * 3 && m * 3 <= n * 5
        };
        if let Some(c) = self.by_key.get(&key) {
            let c: Vec<usize> = c.iter().copied().filter(near).collect();
            return self.pick(&c, s);
        }
        let cap = allowed(key.len());
        if cap == 0 {
            return None;
        }
        let mut best = cap + 1;
        let mut cands: Vec<usize> = Vec::new();
        let lo = key.len().saturating_sub(cap);
        let hi = (key.len() + cap).min(MAX_KEY);
        for len in lo..=hi {
            if allowed(len) == 0 {
                continue;
            }
            for i in self.by_len[len].iter().copied().filter(near) {
                let d = key_dist(&key, &self.phrases[i].0, best.min(cap));
                if d < best {
                    best = d;
                    cands.clear();
                }
                if d == best && d <= cap {
                    cands.push(i);
                }
            }
        }
        if best > cap { None } else { self.pick(&cands, s) }
    }

    /// A coined famous shout with EXACTLY this sound ("raising gun" → "rasengan").
    fn coined_like(&self, s: &str) -> Option<String> {
        let key = long_key(s);
        if key.len() < 5 || s.split(' ').count() > 3 {
            return None;
        }
        self.coined.get(&key).and_then(|c| self.pick(c, s))
    }

    /// Kit names that sound like `s` (closest first; exact keys, or one edit for 5+ symbols).
    fn kits_like(&self, s: &str) -> Vec<(String, u16)> {
        let key = long_key(s);
        if key.len() < 3 {
            return Vec::new();
        }
        let cap = allowed(key.len()).min(1);
        let mut v: Vec<(usize, &str, u16)> = self
            .kits
            .iter()
            .filter_map(|(k, n, p)| {
                let d = key_dist(&key, k, cap);
                (d <= cap && (d == 0 || allowed(k.len()) > 0)).then_some((d, n.as_str(), *p))
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)));
        v.dedup_by(|a, b| a.2 == b.2);
        v.into_iter().take(4).map(|(_, n, p)| (n.to_string(), p)).collect()
    }

    /// A move name / shout of kit `pid` that sounds like `s`.
    fn kit_move_like(&self, pid: u16, s: &str) -> Option<String> {
        let def = PowerTable::get().def(pid)?;
        let key = long_key(s);
        if key.len() < 3 {
            return None;
        }
        let cap = allowed(key.len());
        let names = def.moves.iter().map(|m| norm_words(&m.name)).chain(def.shouts.iter().map(|(s, _)| s.clone()));
        let mut best: Option<(usize, String)> = None;
        for n in names {
            let d = key_dist(&key, &long_key(&n), cap);
            if d <= cap && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, n));
            }
        }
        best.map(|(_, n)| n)
    }
}

// ------------------------------------------------------------------------------------------------
// Streaming
// ------------------------------------------------------------------------------------------------

/// Does a line fire on its own as a POWER MOVE (named / kit / held / twisted / described moves,
/// chains of them)? A streamed beat that doesn't waits for the next one (a construct, half a name).
pub fn fires_alone(lex: &Lexicon, line: &str) -> bool {
    use bd_sim::powers::chain::ToolStep;
    crate::toolbox::command(lex, None, true, line, true).is_some_and(|s| !s.is_empty() && s.iter().all(|s| !matches!(s, ToolStep::Cast { .. })))
}

/// STREAMING (bd_voice `VoiceEvent::Partial`): the beats of one push-to-talk press. Each beat that
/// is a move on its own fires at once (a spoken chain starts while you are still talking); one
/// that isn't (a construct, half a long name cut by a breath) waits and is joined to what follows.
#[derive(Default, Debug)]
pub struct Beats {
    pending: String,
    /// Beats already fired this press.
    pub fired: u32,
}

impl Beats {
    fn join(&mut self, text: &str) -> String {
        let t = text.trim();
        let s = if self.pending.is_empty() { t.to_string() } else if t.is_empty() { self.pending.clone() } else { format!("{} {t}", self.pending) };
        self.pending.clear();
        s
    }

    /// A beat heard while the key is still down → the line to cast now, if it fires alone.
    pub fn beat(&mut self, lex: &Lexicon, text: &str) -> Option<String> {
        let said = self.join(text);
        let line = heard_line(lex, &said);
        if fires_alone(lex, &line) {
            self.fired += 1;
            Some(line)
        } else {
            self.pending = said;
            None
        }
    }

    /// Keep a beat without firing it (the oath overlay's recital).
    pub fn hold(&mut self, text: &str) {
        self.pending = self.join(text);
    }

    /// The press ended (`text` = its last stretch, "" when it was silent): everything not yet fired,
    /// as SAID (the oath hook reads it first; then [`heard_line`]). Empty = nothing left.
    pub fn finish(&mut self, text: &str) -> String {
        self.join(text)
    }

    /// Forget the press (the oath overlay took it, an error).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
#[path = "../../../tools/voice_bench/reader.rs"]
mod bench;
