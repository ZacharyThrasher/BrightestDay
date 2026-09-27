//! Text normalisation shared by the builder and the runtime. `normalize_key` MUST match
//! `norm()` in tools/lexicon_build/build.py for ASCII/Latin input.

/// Hard input limits: anything longer is truncated before parsing (robustness + the 1 ms budget).
pub const MAX_INPUT_CHARS: usize = 120;
pub const MAX_TOKENS: usize = 16;

fn fold_char(c: char, out: &mut String) {
    let s: &str = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' | 'ŉ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ŏ' | 'ő' | 'ø' | 'ǫ' => "o",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' => "s",
        'ţ' | 'ť' | 'ŧ' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'þ' => "th",
        '♀' => " f ",
        '♂' => " m ",
        _ => {
            out.push(c);
            return;
        }
    };
    out.push_str(s);
}

/// Lowercase + fold diacritics (no punctuation handling).
fn lower_fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
        } else {
            for lc in c.to_lowercase() {
                fold_char(lc, &mut out);
            }
        }
    }
    out
}

fn is_apos(c: char) -> bool {
    c == '\'' || c == '\u{2019}' || c == '\u{2018}' || c == '`'
}

/// Normalise a key: lowercase, fold diacritics, drop possessive "'s", drop apostrophes, every other
/// non-[a-z0-9] run becomes one space, trimmed. `"Captain America's Shield!"` -> `"captain america shield"`.
pub fn normalize_key(s: &str) -> String {
    let f = lower_fold(s);
    let chars: Vec<char> = f.chars().collect();
    let mut out = String::with_capacity(f.len());
    let mut pending_space = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_apos(c) {
            // possessive: <alnum>'s<non-alnum|end>
            let prev_alnum = i > 0 && chars[i - 1].is_ascii_alphanumeric();
            let next_s = chars.get(i + 1) == Some(&'s');
            let after = chars.get(i + 2).map(|c| c.is_ascii_alphanumeric()).unwrap_or(false);
            if prev_alnum && next_s && !after {
                i += 2;
                continue;
            }
            // Pinyin syllable mark before a lone final "e" ("Chang'e", "Heng'e"): a word break, not a
            // contraction — dropping it would make "change". (English never ends a word in "'e".)
            let letters_before = chars[..i].iter().rev().take_while(|c| c.is_ascii_alphabetic()).count();
            let lone_e = chars.get(i + 1) == Some(&'e') && !chars.get(i + 2).is_some_and(|c| c.is_ascii_alphanumeric());
            if letters_before >= 3 && lone_e {
                pending_space = true;
            }
            i += 1;
            continue;
        }
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(c);
        } else {
            pending_space = true;
        }
        i += 1;
    }
    out
}

/// A normalised input token plus shouting info from the raw text.
#[derive(Clone, Debug, PartialEq)]
pub struct Tok {
    pub text: String,
    /// The raw word was ALL CAPS (≥ 2 letters): "SO angry", "GIANT".
    pub caps: bool,
}

/// Truncate to the input limits, split into normalised tokens (at most `MAX_TOKENS`).
/// Returns (tokens, exclamation count).
pub fn tokenize(raw: &str) -> (Vec<Tok>, usize) {
    let mut end = raw.len();
    for (n, (i, _)) in raw.char_indices().enumerate() {
        if n == MAX_INPUT_CHARS {
            end = i;
            break;
        }
    }
    let raw = &raw[..end];
    let bangs = raw.chars().filter(|&c| c == '!').count();
    let all_caps_letters = raw.chars().filter(|c| c.is_alphabetic()).count();
    let mut toks = Vec::new();
    for w in raw.split_whitespace() {
        let letters: Vec<char> = w.chars().filter(|c| c.is_alphabetic()).collect();
        let caps = letters.len() >= 2 && letters.iter().all(|c| c.is_uppercase());
        let n = normalize_key(w);
        for t in n.split(' ') {
            if t.is_empty() {
                continue;
            }
            toks.push(Tok { text: t.to_string(), caps });
            if toks.len() >= MAX_TOKENS {
                return (toks, bangs);
            }
        }
    }
    let _ = all_caps_letters;
    (toks, bangs)
}

/// Collapse runs of ≥ 3 identical letters to 2 ("gianttt" -> "giantt").
pub fn squeeze3(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = '\0';
    let mut run = 0;
    for c in s.chars() {
        if c == last {
            run += 1;
        } else {
            last = c;
            run = 1;
        }
        if run <= 2 {
            out.push(c);
        }
    }
    out
}

/// Collapse every run of identical letters to 1 ("giantt" -> "giant", "baloon" stays).
pub fn squeeze2(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = '\0';
    for c in s.chars() {
        if c != last {
            out.push(c);
        }
        last = c;
    }
    out
}

/// Candidate singular forms of a plural/inflected token (most likely first).
pub fn singulars(t: &str) -> Vec<String> {
    let mut v = Vec::new();
    let n = t.len();
    if n < 3 || !t.is_ascii() {
        return v;
    }
    if let Some(stem) = t.strip_suffix("ies")
        && stem.len() >= 2
    {
        v.push(format!("{stem}y"));
    }
    if let Some(stem) = t.strip_suffix("ves")
        && stem.len() >= 2
    {
        v.push(format!("{stem}f"));
        v.push(format!("{stem}fe"));
    }
    if let Some(stem) = t.strip_suffix("es")
        && stem.len() >= 2
    {
        v.push(stem.to_string());
    }
    if let Some(stem) = t.strip_suffix('s')
        && !t.ends_with("ss")
        && stem.len() >= 2
    {
        v.push(stem.to_string());
    }
    if let Some(stem) = t.strip_suffix("i") {
        v.push(format!("{stem}us")); // cacti, octopi
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_examples() {
        assert_eq!(normalize_key("Captain America's Shield!"), "captain america shield");
        assert_eq!(normalize_key("Farfetch'd"), "farfetchd");
        assert_eq!(normalize_key("Flabébé"), "flabebe");
        assert_eq!(normalize_key("Nidoran♀"), "nidoran f");
        assert_eq!(normalize_key("  T-Rex  "), "t rex");
        assert_eq!(normalize_key("Mjölnir"), "mjolnir");
        assert_eq!(normalize_key("Pokémon"), "pokemon");
        assert_eq!(normalize_key("shootin'"), "shootin");
        assert_eq!(normalize_key("Chang'e"), "chang e");
        assert_eq!(normalize_key("moon goddess chang’e!"), "moon goddess chang e");
        assert_eq!(normalize_key("don't"), "dont");
        assert_eq!(normalize_key("o'er"), "oer");
        assert_eq!(normalize_key("🔥🔥"), "");
    }

    #[test]
    fn tokenize_caps_and_limits() {
        let (t, _) = tokenize("GIANT eeyore");
        assert!(t[0].caps && !t[1].caps);
        let long = "a ".repeat(500);
        assert_eq!(tokenize(&long).0.len(), MAX_TOKENS);
        assert_eq!(squeeze3("gianttttt"), "giantt");
        assert_eq!(squeeze2("giantt"), "giant");
    }
}
