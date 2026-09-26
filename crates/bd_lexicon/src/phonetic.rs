//! Sound-alike keys for voice input (local Whisper transcripts) and last-resort typed matching.
//!
//! A Metaphone-style consonant skeleton made deliberately *voicing-insensitive* (b/p, d/t, g/k, v/f, z/s,
//! j/ch/sh collapse) because speech recognisers confuse exactly those pairs. Yod-coalescence ("picked
//! you" ~ "pick-chew") is modelled too. Keys are compared exactly or at edit distance 1 (spoken mode).
//!
//!   pikachu -> PKX,  "pick a chew" -> PKX,  "picked you" -> PKX
//!   mjolnir -> MXLNR, "mule near" -> MLNR (distance 1), charizard/"char is ard" -> XRSRT

fn is_vowel(c: u8) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u' | b'y')
}

/// Phonetic key of a lowercase ASCII word or phrase (spaces and digits ignored). Max 12 symbols.
pub fn phonetic_key(s: &str) -> String {
    let w: Vec<u8> = s.bytes().filter(|b| b.is_ascii_lowercase()).collect();
    let n = w.len();
    let mut out: Vec<u8> = Vec::with_capacity(12);
    let at = |i: usize| -> u8 { if i < n { w[i] } else { 0 } };
    let push = |c: u8, out: &mut Vec<u8>| {
        if out.last() != Some(&c) {
            out.push(c);
        }
    };
    let mut i = 0;
    // initial exceptions
    if n >= 2 {
        match (w[0], w[1]) {
            (b'k', b'n') | (b'g', b'n') | (b'p', b'n') | (b'w', b'r') | (b'p', b's') => i = 1,
            (b'x', _) => {
                out.push(b'S');
                i = 1;
            }
            _ => {}
        }
    }
    if i < n && is_vowel(at(i)) && out.is_empty() {
        out.push(b'A');
        i += 1;
    }
    while i < n && out.len() < 12 {
        let c = w[i];
        let nx = at(i + 1);
        let nx2 = at(i + 2);
        let mut step = 1;
        match c {
            b'a' | b'e' | b'i' | b'o' | b'u' => {}
            b'y' => {
                if is_vowel(nx) && nx != b'y' {
                    push(b'Y', &mut out);
                }
            }
            b'b' | b'p' => {
                if c == b'p' && nx == b'h' {
                    push(b'F', &mut out);
                    step = 2;
                } else if !(c == b'b' && i > 0 && w[i - 1] == b'm' && i + 1 == n) {
                    push(b'P', &mut out);
                }
            }
            b'c' => {
                if nx == b'h' {
                    push(b'X', &mut out);
                    step = 2;
                } else if nx == b'k' || nx == b'q' {
                    push(b'K', &mut out);
                    step = 2;
                } else if matches!(nx, b'e' | b'i' | b'y') {
                    if nx == b'i' && matches!(nx2, b'a' | b'o') {
                        push(b'X', &mut out);
                    } else {
                        push(b'S', &mut out);
                    }
                } else {
                    push(b'K', &mut out);
                }
            }
            b'd' | b't' => {
                if c == b't' && nx == b'h' {
                    push(b'T', &mut out);
                    step = 2;
                } else if c == b't' && nx == b'c' && nx2 == b'h' {
                    push(b'X', &mut out);
                    step = 3;
                } else if nx == b'y' && is_vowel(nx2) {
                    // yod-coalescence: "did you" ~ "di-joo", "got you" ~ "gotcha"
                    push(b'X', &mut out);
                    step = 2;
                } else if c == b't' && nx == b'i' && matches!(nx2, b'a' | b'o') {
                    push(b'X', &mut out);
                } else if c == b'd' && nx == b'g' && matches!(nx2, b'e' | b'i' | b'y') {
                    push(b'X', &mut out);
                    step = 2;
                } else {
                    push(b'T', &mut out);
                }
            }
            b'f' | b'v' => push(b'F', &mut out),
            b'g' => {
                if nx == b'h' {
                    if i == 0 {
                        push(b'K', &mut out);
                    }
                    step = 2;
                } else if nx == b'n' && (i + 2 == n) {
                    // "sign"
                } else if matches!(nx, b'e' | b'i' | b'y') {
                    push(b'X', &mut out);
                } else {
                    push(b'K', &mut out);
                }
            }
            b'h' => {
                if (i == 0 || !is_vowel(w[i - 1])) && is_vowel(nx) {
                    push(b'H', &mut out);
                }
            }
            b'j' => push(b'X', &mut out),
            b'k' | b'q' => {
                push(b'K', &mut out);
                if c == b'q' && nx == b'u' {
                    step = 2;
                }
            }
            b'l' => push(b'L', &mut out),
            b'm' => push(b'M', &mut out),
            b'n' => push(b'N', &mut out),
            b'r' => push(b'R', &mut out),
            b's' | b'z' => {
                if c == b's' && nx == b'h' {
                    push(b'X', &mut out);
                    step = 2;
                } else if c == b's' && nx == b'i' && matches!(nx2, b'a' | b'o') {
                    push(b'X', &mut out);
                } else {
                    push(b'S', &mut out);
                }
            }
            b'w' => {
                if nx == b'h' {
                    push(b'W', &mut out);
                    step = 2;
                } else if is_vowel(nx) {
                    push(b'W', &mut out);
                }
            }
            b'x' => {
                push(b'K', &mut out);
                push(b'S', &mut out);
            }
            _ => {}
        }
        i += step;
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
const SYMBOLS: &[u8] = b"AFHKLMNPRSTWXY";

/// All keys at edit distance exactly 1 from `key` (deletions, substitutions, insertions over the symbol
/// alphabet) — used for loose spoken matching. ~30·len variants.
#[cfg(test)]
pub fn key_variants(key: &str) -> Vec<String> {
    let k = key.as_bytes();
    let mut out = Vec::new();
    for i in 0..k.len() {
        let mut v = k.to_vec();
        v.remove(i);
        out.push(v);
        for &s in SYMBOLS {
            if s != k[i] {
                let mut v = k.to_vec();
                v[i] = s;
                out.push(v);
            }
        }
    }
    for i in 0..=k.len() {
        for &s in SYMBOLS {
            let mut v = k.to_vec();
            v.insert(i, s);
            out.push(v);
        }
    }
    out.into_iter().filter_map(|v| String::from_utf8(v).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_alikes() {
        assert_eq!(phonetic_key("pikachu"), phonetic_key("pickachew"));
        assert_eq!(phonetic_key("pikachu"), phonetic_key("pickedyou"));
        assert_eq!(phonetic_key("mjolnir"), phonetic_key("mjolner"));
        assert_eq!(phonetic_key("charizard"), phonetic_key("charisard"));
        assert_eq!(phonetic_key("godzilla"), phonetic_key("godzila"));
        assert!(key_variants(&phonetic_key("mjolnir")).contains(&phonetic_key("mulenear")));
    }
}
