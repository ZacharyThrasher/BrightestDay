//! Transcript clean-up: the lexicon wants plain lowercase words.

/// Lowercase, fold common accents, drop Whisper's bracketed annotations (`[BLANK_AUDIO]`,
/// `(music)`, `*laughs*`), turn punctuation into spaces (keeping in-word apostrophes, e.g.
/// "america's"), and collapse whitespace.
pub fn normalize_transcript(raw: &str) -> String {
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
            _ => out.push(' '),
        }
    }
    collapse_repeats(out.split_whitespace().collect()).join(" ")
}

/// Whisper's classic failure on short clips is looping ("charizard charizard", "p k chip p k
/// chip p k …"). Spoken commands essentially never repeat a phrase back-to-back, so drop any
/// immediately repeated n-gram (n = 1..=4), including a truncated trailing copy.
fn collapse_repeats(mut w: Vec<&str>) -> Vec<&str> {
    for n in (1..=4).rev() {
        let mut i = 0;
        while i + n < w.len() {
            // Count how many words after w[i..i+n] continue the cycle (may end mid-cycle).
            let mut k = 0;
            while i + n + k < w.len() && w[i + n + k] == w[i + (k % n)] {
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
    use super::normalize_transcript as n;

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
