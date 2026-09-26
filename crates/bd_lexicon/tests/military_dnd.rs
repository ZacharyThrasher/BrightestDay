//! Military + tabletop (D&D) vocabulary: tests/data/military_dnd.txt (same line format as the intent
//! golden corpus). Every `gold_*` line must hold all its expectations and never be a fallback; the wider
//! audit lines are measured for fallback rate and expectation accuracy against floors.
//!
//! `BD_MILDND_VERBOSE=1` prints every line (reading + pass/fail + fallback); otherwise only misses.

mod common;

use std::collections::BTreeMap;

use bd_core::mechanics::FormCategory;
use bd_lexicon::{Delivery, Intent, Lexicon, ResolutionKind};

const ROSTER: [&str; 9] = ["Ethan", "Jake", "Sam", "Alex_99", "Mia", "Big Tony", "xX_Nova_Xx", "DragonSlayer", "Zoe"];

/// Audit floors: max fallback fraction, min expectation accuracy (over all non-golden lines).
const AUDIT_MAX_FALLBACK: f64 = 0.02;
const AUDIT_MIN_ACCURACY: f64 = 0.95;

struct Line {
    no: usize,
    category: String,
    phrase: String,
    want: Vec<String>,
}

fn corpus() -> Vec<Line> {
    include_str!("data/military_dnd.txt")
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|(i, l)| {
            let mut it = l.splitn(3, '|').map(str::trim);
            let category = it.next()?.to_string();
            let phrase = it.next()?.to_string();
            let want = it.next().unwrap_or("").split_whitespace().map(String::from).collect();
            Some(Line { no: i + 1, category, phrase, want })
        })
        .collect()
}

fn same_name(a: &str, b: &str) -> bool {
    let f = |s: &str| s.to_lowercase().replace(['_', ' '], "");
    f(a) == f(b)
}

fn category(name: &str) -> Option<FormCategory> {
    Some(match name {
        "Held" => FormCategory::Held,
        "Worn" => FormCategory::Worn,
        "Creature" => FormCategory::Creature,
        "Structure" => FormCategory::Structure,
        "Object" => FormCategory::Object,
        _ => return None,
    })
}

/// Why `want` fails for this intent (None = holds). `+A/B` = has any of the mechanics.
fn check(it: &Intent, want: &str) -> Option<String> {
    let r = &it.resolution;
    let rc = &r.recipe;
    let has = |m: &str| bd_lexicon::tables::mechanic_from_name(m).map(|m| rc.has(m) || r.graft.mechanics.contains(&m));
    let any = |list: &str, f: &dyn Fn(&str) -> bool| list.split('/').any(f);
    let ok = if let Some(v) = want.strip_prefix("form=") {
        any(v, &|f| bd_lexicon::tables::form_from_name(f) == Some(rc.form))
    } else if let Some(v) = want.strip_prefix("cat=") {
        any(v, &|c| category(c) == Some(rc.form.category()))
    } else if let Some(v) = want.strip_prefix("kind=") {
        (v == "graft") == (r.kind == ResolutionKind::Graft)
    } else if let Some(v) = want.strip_prefix("el=") {
        any(v, &|e| {
            let name = |x| bd_lexicon::tables::element_name(x).eq_ignore_ascii_case(e);
            name(rc.element) || r.graft.element.is_some_and(name)
        })
    } else if let Some(v) = want.strip_prefix("scale>=") {
        v.parse::<f32>().map(|x| rc.scale >= x).unwrap_or(false)
    } else if let Some(v) = want.strip_prefix("scale<=") {
        v.parse::<f32>().map(|x| rc.scale <= x).unwrap_or(false)
    } else if let Some(v) = want.strip_prefix("n>=") {
        v.parse::<u8>().map(|x| rc.count >= x).unwrap_or(false)
    } else if let Some(v) = want.strip_prefix("target=") {
        match (v, it.target) {
            ("none", t) => t.is_none(),
            (name, Some(t)) => same_name(ROSTER[t], name),
            (_, None) => false,
        }
    } else if let Some(v) = want.strip_prefix("via=") {
        it.delivery == Delivery::from_name(v)
    } else if let Some(v) = want.strip_prefix("name~") {
        rc.name.to_lowercase().contains(&v.to_lowercase())
    } else if let Some(m) = want.strip_prefix('+') {
        any(m, &|m| has(m).unwrap_or(false))
    } else if let Some(m) = want.strip_prefix('-') {
        has(m).map(|h| !h).unwrap_or(false)
    } else {
        return Some(format!("{want}: unknown expectation"));
    };
    (!ok).then(|| want.to_string())
}

fn describe(it: &Intent) -> String {
    let r = &it.resolution;
    let mut s = format!("{} el={:?} s={:.1} n={} \"{}\" conf={:.2}", r.signature(), r.recipe.element, r.recipe.scale, r.recipe.count, r.recipe.name, r.confidence);
    if let Some(t) = it.target {
        s.push_str(&format!(" target={}", ROSTER[t]));
    }
    if let Some(d) = it.delivery {
        s.push_str(&format!(" via={}", d.name()));
    }
    if !r.ignored.is_empty() {
        s.push_str(&format!(" ignored={:?}", r.ignored));
    }
    s
}

#[test]
fn military_and_tabletop_vocabulary() {
    let Some(lex) = common::lex() else { return };
    let verbose = std::env::var_os("BD_MILDND_VERBOSE").is_some();
    let lines = corpus();
    assert!(lines.len() >= 250, "corpus has {} lines", lines.len());
    // category -> (ok, fallback, total)
    let mut stats: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut gold_fail = Vec::new();
    let mut bad = Vec::new();
    for l in &lines {
        let it = lex.intent(&l.phrase, &ROSTER, false);
        let fails: Vec<String> = l.want.iter().filter_map(|w| check(&it, w)).collect();
        if fails.iter().any(|f| f.contains("unknown expectation")) {
            bad.push(format!("line {}: {fails:?}", l.no));
        }
        let fb = Lexicon::is_fallback(&it.resolution);
        let e = stats.entry(l.category.clone()).or_default();
        e.2 += 1;
        e.0 += fails.is_empty() as usize;
        e.1 += fb as usize;
        let mark = match (fails.is_empty(), fb) {
            (true, false) => "ok  ",
            (true, true) => "FB  ",
            (false, false) => "MISS",
            (false, true) => "MSFB",
        };
        let row = format!("  {mark} L{:<4}{:36} [{}] {}", l.no, l.phrase, fails.join(" "), describe(&it));
        if verbose || mark != "ok  " {
            eprintln!("{row}");
        }
        if l.category.starts_with("gold_") && (fb || !fails.is_empty()) {
            gold_fail.push(row);
        }
    }
    assert!(bad.is_empty(), "malformed expectations:\n{}", bad.join("\n"));
    eprintln!("military / tabletop vocabulary:");
    let (mut a_ok, mut a_fb, mut a_n) = (0, 0, 0);
    for (cat, (ok, fb, n)) in &stats {
        eprintln!("  {cat:12} ok {ok:3}/{n:<3} fallback {fb:3}/{n:<3} ({:4.1}%)", *fb as f64 * 100.0 / (*n).max(1) as f64);
        if !cat.starts_with("gold_") {
            a_ok += ok;
            a_fb += fb;
            a_n += n;
        }
    }
    let (fb_rate, acc) = (a_fb as f64 / a_n.max(1) as f64, a_ok as f64 / a_n.max(1) as f64);
    eprintln!("  AUDIT        ok {a_ok:3}/{a_n:<3} ({:4.1}%) fallback {a_fb:3}/{a_n:<3} ({:4.1}%)", acc * 100.0, fb_rate * 100.0);
    if std::env::var_os("BD_MILDND_NO_ASSERT").is_some() {
        return;
    }
    assert!(gold_fail.is_empty(), "golden military/tabletop phrases failed:\n{}", gold_fail.join("\n"));
    assert!(fb_rate <= AUDIT_MAX_FALLBACK + 1e-9, "audit fallback {:.1}% > {:.1}%", fb_rate * 100.0, AUDIT_MAX_FALLBACK * 100.0);
    assert!(acc + 1e-9 >= AUDIT_MIN_ACCURACY, "audit accuracy {:.1}% < {:.1}%", acc * 100.0, AUDIT_MIN_ACCURACY * 100.0);
}
