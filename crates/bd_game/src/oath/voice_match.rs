//! Spoken oath recital: how closely a Whisper transcript matches an oath section (pure functions,
//! no audio — built and unit-tested with or without the `voice` feature).
//!
//! Word-level, forgiving of what Whisper does to a shouted verse:
//! * case, punctuation, apostrophes ("Sinestro's" = "sinestros"), contractions ("we'll" = "we will"),
//!   digits and common homophones ("4" = "four" = "for", "knight" = "night", "site" = "sight");
//! * words split or merged ("tornek" ↔ "torn neck");
//! * near misses — above all the Compassion oath's invented words, which Whisper can only spell by
//!   ear — earn partial credit by letter / rough-phonetic similarity;
//! * words before and after the section are free (a yelled full oath still matches a section), an
//!   extra word inside costs half a missed one.
//!
//! Accuracy = `1 − alignment cost / section words`, 0..1.

/// A push-to-talk utterance outside the overlay counts as an oath recital at this similarity.
pub const YELL_MATCH: f32 = 0.7;
/// Cost of an extra transcript word inside the aligned span (a missed section word costs 1).
const EXTRA_WORD: f32 = 0.5;
/// Partial credit below this similarity is no credit.
const MIN_CREDIT: f32 = 0.5;

/// Normalised word tokens of `s` (see the module docs).
pub fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let flush = |w: &mut String, out: &mut Vec<String>| {
        let t = w.trim_matches('\'');
        if !t.is_empty() {
            push_word(t, out);
        }
        w.clear();
    };
    for c in s.chars() {
        let c = if matches!(c, '\u{2019}' | '\u{2018}' | '`') { '\'' } else { c };
        if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        } else if c == '\'' && !word.is_empty() {
            word.push('\'');
        } else {
            flush(&mut word, &mut out);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn push_word(w: &str, out: &mut Vec<String>) {
    let special: Option<&[&str]> = match w {
        "won't" => Some(&["will", "not"]),
        "can't" => Some(&["can", "not"]),
        "cannot" => Some(&["can", "not"]),
        "shan't" => Some(&["shall", "not"]),
        "i'm" => Some(&["i", "am"]),
        "let's" => Some(&["let", "us"]),
        "that's" => Some(&["that", "is"]),
        "it's" => Some(&["it", "is"]),
        "what's" => Some(&["what", "is"]),
        "there's" => Some(&["there", "is"]),
        _ => None,
    };
    if let Some(ws) = special {
        out.extend(ws.iter().map(|w| canon(w)));
        return;
    }
    for (suffix, full) in [("'ll", "will"), ("'re", "are"), ("'ve", "have"), ("'d", "would"), ("n't", "not")] {
        if let Some(base) = w.strip_suffix(suffix) {
            if !base.is_empty() {
                out.push(canon(&base.replace('\'', "")));
                out.push(full.to_string());
                return;
            }
        }
    }
    // Possessive / anything else: drop the apostrophe ("sinestro's" → "sinestros").
    let w = w.replace('\'', "");
    // A number Whisper wrote as digits: spell single digits (they are almost always homophones here).
    if w.chars().all(|c| c.is_ascii_digit()) {
        for d in w.chars() {
            out.push(canon(DIGITS[d as usize - '0' as usize]));
        }
        return;
    }
    out.push(canon(&w));
}

const DIGITS: [&str; 10] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];

/// Homophones folded to one spelling (Whisper picks whichever it likes).
fn canon(w: &str) -> String {
    let c = match w {
        "four" | "fore" => "for",
        "two" | "too" => "to",
        "won" => "one",
        "eight" => "ate",
        "knight" | "nite" => "night",
        "site" | "cite" => "sight",
        "mite" => "might",
        "lite" => "light",
        "write" | "rite" | "wright" => "right",
        "hour" => "our",
        "hole" => "whole",
        "buy" | "bye" => "by",
        "u" => "you",
        "r" => "are",
        "sea" => "see",
        "eye" => "i",
        "harts" => "hearts",
        "dye" => "die",
        "dyes" => "dies",
        "heal" => "hell",
        "sir" | "sure" | "sore" => "sur",
        w => w,
    };
    c.to_string()
}

/// Rough English phonetic key: silent letters dropped, look-alike consonants merged, every vowel
/// run → `a`, doubled letters collapsed. Only used for partial credit.
fn phonetic(w: &str) -> Vec<u8> {
    let b: Vec<u8> = w.bytes().filter(|c| c.is_ascii_alphabetic()).collect();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let vowel = |c: u8| matches!(c, b'a' | b'e' | b'i' | b'o' | b'u' | b'y');
    let n = b.len();
    let mut i = 0;
    while i < n {
        let c = b[i];
        let next = b.get(i + 1).copied();
        let mapped: &[u8] = match c {
            // Silent-ish starts: kn, wr, wh, ps.
            b'k' if i == 0 && next == Some(b'n') => b"",
            b'w' if i == 0 && next == Some(b'r') => b"",
            b'p' if i == 0 && next == Some(b's') => b"",
            b'g' if next == Some(b'h') => {
                i += 2;
                continue;
            }
            b'p' if next == Some(b'h') => {
                i += 2;
                out.push(b'f');
                continue;
            }
            b'c' if next == Some(b'k') => b"",
            b'c' if matches!(next, Some(b'e' | b'i' | b'y')) => b"s",
            b'c' | b'q' => b"k",
            b'x' => b"ks",
            b'z' => b"s",
            b'v' => b"f",
            b'h' if i > 0 => b"",
            b'w' if i > 0 && vowel(b[i - 1]) => b"",
            // Silent final e.
            b'e' if i == n - 1 && n > 3 => b"",
            c if vowel(c) => b"a",
            _ => std::slice::from_ref(&b[i]),
        };
        for &m in mapped {
            if out.last() != Some(&m) {
                out.push(m);
            }
        }
        i += 1;
    }
    out
}

fn lev<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, x) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + (x != y) as usize).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn norm_sim<T: PartialEq>(a: &[T], b: &[T]) -> f32 {
    let m = a.len().max(b.len());
    if m == 0 { 1.0 } else { 1.0 - lev(a, b) as f32 / m as f32 }
}

/// How much credit transcript word `t` earns for section word `r` (0..1).
pub fn word_credit(r: &str, t: &str) -> f32 {
    if r == t {
        return 1.0;
    }
    let letters = norm_sim(r.as_bytes(), t.as_bytes());
    let (pr, pt) = (phonetic(r), phonetic(t));
    let sound = if pr == pt && !pr.is_empty() { 0.9 } else { norm_sim(&pr, &pt) * 0.9 };
    let s = letters.max(sound);
    if s < MIN_CREDIT { 0.0 } else { s }
}

/// Similarity 0..1 of a spoken transcript to one section's text.
pub fn similarity(transcript: &str, section: &str) -> f32 {
    align(&tokens(section), &tokens(transcript))
}

/// Semi-global word alignment: every section word must be accounted for; transcript words before
/// and after the matched span are free.
fn align(r: &[String], t: &[String]) -> f32 {
    let (n, m) = (r.len(), t.len());
    if n == 0 {
        return 0.0;
    }
    // dp[i][j]: cost of section words ..i against transcript words ..j (leading words free).
    let mut dp = vec![vec![f32::INFINITY; m + 1]; n + 1];
    for j in 0..=m {
        dp[0][j] = 0.0;
    }
    for i in 1..=n {
        for j in 0..=m {
            let mut best = dp[i - 1][j] + 1.0; // section word missed
            if j >= 1 {
                best = best.min(dp[i][j - 1] + EXTRA_WORD);
                best = best.min(dp[i - 1][j - 1] + 1.0 - word_credit(&r[i - 1], &t[j - 1]));
            }
            if j >= 2 {
                // Two spoken words for one section word ("torn neck" for "tornek").
                let joined = format!("{}{}", t[j - 2], t[j - 1]);
                best = best.min(dp[i - 1][j - 2] + 1.0 - word_credit(&r[i - 1], &joined));
            }
            if i >= 2 && j >= 1 {
                // One spoken word for two section words ("sinestros" / "abinsur").
                let joined = format!("{}{}", r[i - 2], r[i - 1]);
                best = best.min(dp[i - 2][j - 1] + 2.0 * (1.0 - word_credit(&joined, &t[j - 1])));
            }
            dp[i][j] = best;
        }
    }
    let cost = dp[n].iter().copied().fold(f32::INFINITY, f32::min);
    (1.0 - cost / n as f32).clamp(0.0, 1.0)
}

/// The section a transcript matches best: `(index, similarity)` (ties → the earlier section).
pub fn best_section<S: AsRef<str>>(transcript: &str, sections: &[S]) -> Option<(usize, f32)> {
    let t = tokens(transcript);
    let mut best: Option<(usize, f32)> = None;
    for (i, s) in sections.iter().enumerate() {
        let a = align(&tokens(s.as_ref()), &t);
        if best.is_none_or(|(_, b)| a > b) {
            best = Some((i, a));
        }
    }
    best
}

/// A push-to-talk utterance outside the overlay that recites a section of the player's own oath:
/// `Some((section, accuracy))` at ≥ [`YELL_MATCH`], else `None` (it's a cast).
pub fn yelled_section<S: AsRef<str>>(transcript: &str, sections: &[S]) -> Option<(usize, f32)> {
    best_section(transcript, sections).filter(|(_, a)| *a >= YELL_MATCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::corps::Corps;
    use bd_sim::willpower::{oath_sections, parse_oath_ron};

    fn sections(c: Corps) -> Vec<String> {
        let map = parse_oath_ron(include_str!("../../../../assets/oaths.ron")).unwrap();
        oath_sections(&map[c.name()])
    }

    #[test]
    fn tokens_are_forgiving() {
        assert_eq!(tokens("Burn like my power... Sinestro's might!"), ["burn", "like", "my", "power", "sinestros", "might"]);
        assert_eq!(tokens("We'll burn you all--That is your fate!"), ["we", "will", "burn", "you", "all", "that", "is", "your", "fate"]);
        assert_eq!(tokens("4 hearts long lost"), ["for", "hearts", "long", "lost"]);
        assert_eq!(tokens("THAT'S MY RIGHT"), ["that", "is", "my", "right"]);
        assert_eq!(tokens("no evil shall escape my site"), ["no", "evil", "shall", "escape", "my", "sight"]);
    }

    #[test]
    fn exact_and_whisper_style_transcripts_match() {
        // (corps, section, what Whisper typically returns: lowercase, punctuation stripped, its own
        // spellings / homophones / contractions, looping repeats collapsed).
        let cases: &[(Corps, usize, &str)] = &[
            (Corps::Willpower, 0, "in brightest day in blackest night no evil shall escape my sight"),
            (Corps::Willpower, 1, "let those who worship evils might beware my power green lanterns light"),
            (Corps::Willpower, 0, "in brightest day in blackest knight no evil shall escape my site"),
            (Corps::Fear, 1, "let those who try to stop what's right burn like my power sinestro's might"),
            (Corps::Fear, 0, "in blackest day in brightest night beware your fears made into light"),
            (Corps::Rage, 1, "together with our hellish hate we will burn you all that is your fate"),
            (Corps::Rage, 0, "with blood and rage of crimson red ripped from a corpse so freshly dead"),
            (Corps::Greed, 1, "i lay claim to all within my sight to take what i want that's my right"),
            (Corps::Hope, 1, "when all seems lost in the war of light look to the stars for hope burns bright"),
            (Corps::Love, 0, "4 hearts long lost and full of fright for those alone in blackest night"),
            (Corps::Life, 1, "let darkness cower at the sight of life behold the power of white lanterns light"),
            // The Compassion oath is invented words: Whisper spells them by ear.
            (Corps::Compassion, 0, "tor lorek san bor naka mur natromo fan tornek wot ur"),
            (Corps::Compassion, 1, "ter lantern ker lo abin sur tan lek nok for morrow sir"),
            (Corps::Compassion, 0, "tore lore x-an bore knacka moor natrono farn torn neck what or"),
        ];
        for &(c, s, spoken) in cases {
            let secs = sections(c);
            let a = similarity(spoken, &secs[s]);
            println!("{c:?} §{s}: {a:.3}  {spoken:?}");
            assert!(a >= 0.85, "{c:?} §{s}: {a} for {spoken:?} vs {:?}", secs[s]);
            // And it is recognised as that section when yelled outside the overlay.
            assert_eq!(yelled_section(spoken, &secs).map(|(i, _)| i), Some(s), "{c:?}: {spoken:?}");
        }
        // The exact text scores 1.
        for c in Corps::ALL {
            for s in sections(c) {
                assert!((similarity(&s, &s) - 1.0).abs() < 1e-6, "{s:?}");
            }
        }
    }

    #[test]
    fn construct_phrases_and_sloppy_recitals_do_not() {
        for c in Corps::ALL {
            let secs = sections(c);
            for phrase in ["giant hammer", "flying shark with a laser", "a big green wall", "summon a dragon made of fire", "hammer hammer hammer", ""] {
                let (_, a) = best_section(phrase, &secs).unwrap();
                assert!(a < 0.5, "{c:?}: {phrase:?} scored {a}");
                assert_eq!(yelled_section(phrase, &secs), None);
            }
        }
        // Half an oath line is not a recital (but still earns partial accuracy in the overlay).
        let w = sections(Corps::Willpower);
        let half = similarity("in brightest day in blackest night", &w[0]);
        assert!(half > 0.3 && half < YELL_MATCH, "{half}");
        // A near-miss mumble: accuracy drops but stays meaningful.
        let sloppy = similarity("in bright day in black night no evil escape my side", &w[0]);
        assert!(sloppy > 0.55 && sloppy < 0.9, "{sloppy}");
    }

    #[test]
    fn a_yelled_full_oath_matches_a_section() {
        let secs = sections(Corps::Willpower);
        let full = "in brightest day in blackest night no evil shall escape my sight let those who worship evils might beware my power green lanterns light";
        let (i, a) = yelled_section(full, &secs).unwrap();
        assert!(a > 0.95, "{a}");
        assert_eq!(i, 0);
        // With words around it (e.g. "okay" before, "yeah" after).
        let (i, a) = yelled_section(&format!("okay {} yeah", secs[1]), &secs).unwrap();
        assert!(i == 1 && a > 0.99, "{i} {a}");
    }
}
