//! Transcript clean-up: the lexicon wants plain lowercase words.

/// Lowercase, fold common accents, drop Whisper's bracketed annotations (`[BLANK_AUDIO]`,
/// `(music)`, `*laughs*`), turn punctuation into spaces (keeping in-word apostrophes, e.g.
/// "america's"), and collapse whitespace.
pub fn normalize_transcript(raw: &str) -> String {
    let marked = clean(raw);
    collapse_repeats(tokens(&marked)).into_iter().map(|(w, _)| w).collect::<Vec<_>>().join(" ")
}

/// [`normalize_transcript`] that keeps the BEATS Whisper heard: a comma / full stop / "!" / "?"
/// between two words becomes ", " — the typed chain separator ("Rasengan! Chidori!" →
/// "rasengan, chidori"; the reader splits there only when every piece is a move or a clean thing,
/// so "a big, flaming hammer" stays one hammer). What the game casts from.
pub fn normalize_line(raw: &str) -> String {
    let marked = clean(raw);
    let mut out = String::with_capacity(marked.len());
    for (k, (w, beat)) in collapse_repeats(tokens(&marked)).into_iter().enumerate() {
        if k > 0 {
            out.push_str(if beat { ", " } else { " " });
        }
        out.push_str(w);
    }
    out
}

/// Whisper's pause punctuation, marked in [`clean`]'s output.
const BEAT: char = '\u{1}';

/// The words of [`clean`]'s output, each with "a beat came right before it" (never the first).
fn tokens(marked: &str) -> Vec<(&str, bool)> {
    let mut out: Vec<(&str, bool)> = Vec::new();
    let mut beat = false;
    for t in marked.split_whitespace() {
        if t.chars().all(|c| c == BEAT) {
            beat = !out.is_empty();
        } else {
            out.push((t, beat));
            beat = false;
        }
    }
    out
}

/// Lowercased, accent-folded, annotations dropped, punctuation → spaces (pause punctuation →
/// a [`BEAT`] token), in-word apostrophes kept.
fn clean(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut depth_sq = 0u32;
    let mut depth_paren = 0u32;
    let mut in_star = false;
    let chars: Vec<char> = raw.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '[' => depth_sq += 1,
            ']' => depth_sq = depth_sq.saturating_sub(1),
            '(' => depth_paren += 1,
            ')' => depth_paren = depth_paren.saturating_sub(1),
            '*' => in_star = !in_star,
            _ if depth_sq > 0 || depth_paren > 0 || in_star => {}
            '\'' | '\u{2019}' | '\u{2018}' => {
                let prev = i.checked_sub(1).map(|j| chars[j].is_alphanumeric()).unwrap_or(false);
                let next = chars.get(i + 1).map(|c| c.is_alphanumeric()).unwrap_or(false);
                out.push(if prev && next { '\'' } else { ' ' });
            }
            c if c.is_alphanumeric() => {
                for l in c.to_lowercase() {
                    out.push(fold_accent(l));
                }
            }
            ',' | '.' | '!' | '?' | ';' | ':' | '\u{2026}' => {
                // a beat, unless glued inside a token ("3.5", "u.s.")
                let prev = i.checked_sub(1).map(|j| chars[j].is_alphanumeric()).unwrap_or(false);
                let next = chars.get(i + 1).map(|c| c.is_alphanumeric()).unwrap_or(false);
                if prev && next {
                    out.push(' ');
                } else {
                    out.push(' ');
                    out.push(BEAT);
                    out.push(' ');
                }
            }
            _ => out.push(' '),
        }
    }
    out
}

/// Whisper's classic failure on short clips is looping ("charizard charizard", "p k chip p k
/// chip p k …"). Spoken commands essentially never repeat a phrase back-to-back, so drop any
/// immediately repeated n-gram (n = 1..=16), including a truncated trailing copy.
fn collapse_repeats(mut w: Vec<(&str, bool)>) -> Vec<(&str, bool)> {
    // (up to 16 words: Whisper also loops whole spoken chains, "fireball jutsu hollow purple
    // fireball jutsu hollow purple fire…")
    for n in (1..=16).rev() {
        let mut i = 0;
        while i + n < w.len() {
            // Count how many words after w[i..i+n] continue the cycle (may end mid-cycle).
            let mut k = 0;
            while i + n + k < w.len() && w[i + n + k].0 == w[i + (k % n)].0 {
                k += 1;
            }
            // a last word cut off mid-copy ("… explosive raise nong, explosive raise n") still ends it
            if k >= n && i + n + k + 1 == w.len() && w[i + (k % n)].0.starts_with(w[w.len() - 1].0) {
                k += 1;
            }
            if k >= n {
                // At least one full repeat. Drop whole repeats, plus a truncated trailing copy
                // if the cycle runs to the end of the text.
                let drop = if i + n + k == w.len() { k } else { k / n * n };
                w.drain(i + n..i + n + drop);
            } else {
                i += 1;
            }
        }
    }
    w
}

fn fold_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_line as line;
    use super::normalize_transcript as n;

    #[test]
    fn beats_kept_as_commas() {
        assert_eq!(line("Rasengan! Chidori!"), "rasengan, chidori");
        assert_eq!(line("Nightcrawler teleport. Reinhardt ultimate."), "nightcrawler teleport, reinhardt ultimate");
        assert_eq!(line(" Flaming hammer."), "flaming hammer");
        assert_eq!(line("A big, flaming hammer"), "a big, flaming hammer");
        assert_eq!(line("Charizard. Charizard."), "charizard");
        assert_eq!(line("3.5 swords"), "3 5 swords");
        assert_eq!(line("Captain America's shield, then Mjölnir"), "captain america's shield, then mjolnir");
        assert_eq!(line("[BLANK_AUDIO]"), "");
        assert_eq!(line("... Kamehameha..."), "kamehameha");
        assert_eq!(n("Rasengan! Chidori!"), "rasengan chidori", "the plain clean-up has no beats");
        // Whisper looping a whole spoken chain, the last copy cut off mid-word
        assert_eq!(line("Explosive Raise Nong, Explosive Raise Nong, Explosive Raise Nong, Explosive Raise N"), "explosive raise nong");
        assert_eq!(line("Titan Transformation then Mukman Launcher, Titan Transformation then Mukman Launcher, Titan Transformation then Mukman L"), "titan transformation then mukman launcher");
    }

    #[test]
    fn basics() {
        assert_eq!(n(" Flaming hammer."), "flaming hammer");
        assert_eq!(n("Pikachu!"), "pikachu");
        assert_eq!(n("Captain America's shield."), "captain america's shield");
        assert_eq!(n("Captain America\u{2019}s shield"), "captain america's shield");
        assert_eq!(n("Mjölnir"), "mjolnir");
        assert_eq!(n("Pokémon, go!"), "pokemon go");
        assert_eq!(n("[BLANK_AUDIO]"), "");
        assert_eq!(n(" (music) bouncy wall -- with legs"), "bouncy wall with legs");
        assert_eq!(n("'quoted'"), "quoted");
        assert_eq!(n("ice-sword"), "ice sword");
        assert_eq!(n("Charizard. Charizard."), "charizard");
        assert_eq!(n("P K chip P K chip P K chip P K"), "p k chip");
        assert_eq!(n("big big hammer"), "big hammer");
        assert_eq!(n("flaming hammer flaming hammer"), "flaming hammer");
        assert_eq!(n("bouncy wall with legs"), "bouncy wall with legs");
    }
}
