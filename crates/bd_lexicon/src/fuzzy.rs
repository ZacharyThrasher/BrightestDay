//! Edit-distance helpers for typo correction (restricted Damerau-Levenshtein on bytes).

/// Restricted Damerau-Levenshtein distance (adjacent transpositions), or None if it exceeds `max`.
pub fn dl_distance(a: &[u8], b: &[u8], max: usize) -> Option<usize> {
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > max {
        return None;
    }
    if n > 64 || m > 64 {
        return None;
    }
    let w = m + 1;
    let mut d = vec![0usize; (n + 1) * w];
    for i in 0..=n {
        d[i * w] = i;
    }
    for (j, v) in d.iter_mut().enumerate().take(m + 1) {
        *v = j;
    }
    for i in 1..=n {
        let mut row_min = usize::MAX;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[(i - 1) * w + j] + 1).min(d[i * w + j - 1] + 1).min(d[(i - 1) * w + j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[(i - 2) * w + j - 2] + 1);
            }
            d[i * w + j] = v;
            row_min = row_min.min(v);
        }
        if row_min > max {
            return None;
        }
    }
    let r = d[n * w + m];
    (r <= max).then_some(r)
}

/// Every string at edit distance 1 (deletion, transposition, substitution, insertion over a-z).
pub fn edits1(w: &str) -> Vec<String> {
    let b = w.as_bytes();
    let n = b.len();
    let mut out = Vec::with_capacity(54 * n + 25);
    for i in 0..n {
        let mut v = b.to_vec();
        v.remove(i);
        out.push(v);
    }
    for i in 0..n.saturating_sub(1) {
        let mut v = b.to_vec();
        v.swap(i, i + 1);
        out.push(v);
    }
    for i in 0..n {
        for c in b'a'..=b'z' {
            if c != b[i] {
                let mut v = b.to_vec();
                v[i] = c;
                out.push(v);
            }
        }
    }
    for i in 0..=n {
        for c in b'a'..=b'z' {
            let mut v = b.to_vec();
            v.insert(i, c);
            out.push(v);
        }
    }
    out.into_iter().filter_map(|v| String::from_utf8(v).ok()).collect()
}

/// All single-character deletions.
pub fn deletes1(w: &str) -> Vec<String> {
    let b = w.as_bytes();
    (0..b.len())
        .filter_map(|i| {
            let mut v = b.to_vec();
            v.remove(i);
            String::from_utf8(v).ok()
        })
        .collect()
}

/// Max allowed typo distance for a token of this length (typed input); spoken input gets +1 for long words.
pub fn max_typo(len: usize, spoken: bool) -> usize {
    match len {
        0..=3 => 0,
        4..=7 => 1,
        _ => {
            if spoken {
                3
            } else {
                2
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distances() {
        assert_eq!(dl_distance(b"hamer", b"hammer", 2), Some(1));
        assert_eq!(dl_distance(b"godzila", b"godzilla", 2), Some(1));
        assert_eq!(dl_distance(b"captian", b"captain", 2), Some(1));
        assert_eq!(dl_distance(b"sheild", b"shield", 2), Some(1));
        assert_eq!(dl_distance(b"abc", b"xyz", 2), None);
        assert!(edits1("hamer").contains(&"hammer".to_string()));
    }
}
