//! Golden intent corpus: tests/data/intent_golden.txt lists ~1,200 things players type or SAY with the
//! intent a human means (form / key mechanics / element / scale band / target / delivery / signature).
//! Every phrase goes through the same staged entry point the game uses (`Lexicon::intent`: refer →
//! target → delivery verb → resolver) with a fixed roster of other players. The test reports accuracy
//! per category and fails below the floors in [`FLOORS`] (set from measured accuracy; raise them as
//! recognition improves, never lower them to make a change pass).
//!
//! `BD_INTENT_VERBOSE=1` prints every miss (default: the first few per category).
//! `BD_INTENT_ONLY=<category>` runs one category.

mod common;

use std::collections::BTreeMap;
use std::time::Instant;

use bd_core::mechanics::FormCategory;
use bd_lexicon::{Delivery, Intent, Refer, ResolutionKind};

const ROSTER: [&str; 9] = ["Ethan", "Jake", "Sam", "Alex_99", "Mia", "Big Tony", "xX_Nova_Xx", "DragonSlayer", "Zoe"];

/// Minimum accuracy per category (fraction of lines whose every expectation holds).
/// Measured 2026-09-26 (pre-intent client pipeline → now): target 10.5% → 96.9%, voice_target 9.7% →
/// 99.0%, verbs 74% → 82%, voice 79% → 94%, combos 83% → 89%, TOTAL 72.2% → 95.2%. Floors sit one or two
/// lines under the measured value.
const FLOORS: &[(&str, f64)] = &[
    ("owner", 0.95),
    ("target", 0.95),
    ("voice_target", 0.97),
    ("popculture", 0.97),
    ("everyday", 0.93),
    ("verbs", 0.80),
    ("combos", 0.87),
    ("signature", 0.98),
    ("pokemon", 0.97),
    ("refer", 0.88),
    ("voice", 0.93),
    ("collisions", 0.95),
];
/// Floor for the whole corpus.
const TOTAL_FLOOR: f64 = 0.94;

struct Line {
    no: usize,
    category: String,
    phrase: String,
    spoken: bool,
    want: Vec<String>,
}

fn corpus() -> Vec<Line> {
    include_str!("data/intent_golden.txt")
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|(i, l)| {
            let mut it = l.splitn(3, '|').map(str::trim);
            let category = it.next()?.to_string();
            let raw = it.next()?;
            let want = it.next().unwrap_or("").split_whitespace().map(String::from).collect();
            let (spoken, phrase) = match raw.strip_prefix('~') {
                Some(p) => (true, p.trim().to_string()),
                None => (false, raw.to_string()),
            };
            Some(Line { no: i + 1, category, phrase, spoken, want })
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

/// Why `want` fails for this intent (None = holds).
fn check(it: &Intent, want: &str) -> Option<String> {
    let r = &it.resolution;
    let rc = &r.recipe;
    let has = |m: &str| bd_lexicon::tables::mechanic_from_name(m).map(|m| rc.has(m) || r.graft.mechanics.contains(&m));
    let any = |list: &str, f: &dyn Fn(&str) -> bool| list.split('/').any(f);
    let ok = if let Some(v) = want.strip_prefix("form!=") {
        !any(v, &|f| bd_lexicon::tables::form_from_name(f) == Some(rc.form))
    } else if let Some(v) = want.strip_prefix("form=") {
        any(v, &|f| bd_lexicon::tables::form_from_name(f) == Some(rc.form))
    } else if let Some(v) = want.strip_prefix("cat=") {
        any(v, &|c| category(c) == Some(rc.form.category()))
    } else if let Some(v) = want.strip_prefix("kind=") {
        match v {
            "graft" => r.kind == ResolutionKind::Graft,
            _ => r.kind == ResolutionKind::Construct,
        }
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
    } else if let Some(v) = want.strip_prefix("n<=") {
        v.parse::<u8>().map(|x| rc.count <= x).unwrap_or(false)
    } else if let Some(v) = want.strip_prefix("target=") {
        match (v, it.target) {
            ("none", t) => t.is_none(),
            (name, Some(t)) => same_name(ROSTER[t], name),
            (_, None) => false,
        }
    } else if let Some(v) = want.strip_prefix("via=") {
        it.delivery == Delivery::from_name(v)
    } else if let Some(v) = want.strip_prefix("sig=") {
        match (v, &r.signature) {
            ("none", s) => s.is_none(),
            (k, Some(s)) => format!("{:?}", s.kind).eq_ignore_ascii_case(k),
            (_, None) => false,
        }
    } else if let Some(v) = want.strip_prefix("refer=") {
        match v {
            "it" => matches!(it.refer, Some(Refer::It { .. })),
            "again" => matches!(it.refer, Some(Refer::Again { .. })),
            _ => false,
        }
    } else if let Some(v) = want.strip_prefix("name~") {
        rc.name.to_lowercase().contains(&v.to_lowercase())
    } else if let Some(v) = want.strip_prefix("mount=") {
        match (&rc.mount, v) {
            (Some(_), "any") => true,
            (Some(m), f) => bd_lexicon::tables::form_from_name(f) == Some(m.form),
            (None, _) => false,
        }
    } else if let Some(v) = want.strip_prefix("incant=") {
        r.incantation.as_deref() == Some(v) || r.incantation_powers.iter().any(|p| p == v)
    } else if want == "rush" {
        r.stand_rush.is_some()
    } else if let Some(m) = want.strip_prefix('+') {
        has(m).unwrap_or(false)
    } else if let Some(m) = want.strip_prefix('-') {
        has(m).map(|h| !h).unwrap_or(false)
    } else {
        return Some(format!("{want}: unknown expectation"));
    };
    (!ok).then(|| want.to_string())
}

fn describe(it: &Intent) -> String {
    let r = &it.resolution;
    let mut s = format!("{} {:?} el={:?} scale={:.1} n={} \"{}\"", r.signature(), r.kind, r.recipe.element, r.recipe.scale, r.recipe.count, r.recipe.name);
    if let Some(t) = it.target {
        s.push_str(&format!(" target={}", ROSTER[t]));
    }
    if let Some(d) = it.delivery {
        s.push_str(&format!(" via={}", d.name()));
    }
    if let Some(sig) = &r.signature {
        s.push_str(&format!(" sig={:?}", sig.kind));
    }
    if let Some(m) = &r.recipe.mount {
        s.push_str(&format!(" mount={:?}", m.form));
    }
    if let Some(rf) = &it.refer {
        s.push_str(&format!(" refer={rf:?}"));
    }
    s.push_str(&format!(" | {}", r.explanation.iter().take(2).cloned().collect::<Vec<_>>().join("; ")));
    s
}

#[test]
fn intent_golden_corpus() {
    let Some(lex) = common::lex() else { return };
    let only = std::env::var("BD_INTENT_ONLY").ok();
    let verbose = std::env::var_os("BD_INTENT_VERBOSE").is_some();
    let lines = corpus();
    assert!(lines.len() >= 1000, "corpus has {} lines", lines.len());
    // category -> (ok, total, misses)
    let mut stats: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
    let mut times: Vec<f64> = Vec::with_capacity(lines.len());
    let mut bad_tokens = Vec::new();
    for l in lines.iter().filter(|l| only.as_ref().is_none_or(|o| *o == l.category)) {
        let t = Instant::now();
        let it = lex.intent(&l.phrase, &ROSTER, l.spoken);
        times.push(t.elapsed().as_secs_f64() * 1e3);
        let fails: Vec<String> = l.want.iter().filter_map(|w| check(&it, w)).collect();
        if fails.iter().any(|f| f.contains("unknown expectation")) {
            bad_tokens.push(format!("line {}: {:?}", l.no, fails));
        }
        let e = stats.entry(l.category.clone()).or_default();
        e.1 += 1;
        if fails.is_empty() {
            e.0 += 1;
        } else {
            e.2.push(format!("  L{:<5}{}{:34} want [{}] got {}", l.no, if l.spoken { "~" } else { " " }, l.phrase, fails.join(" "), describe(&it)));
        }
    }
    assert!(bad_tokens.is_empty(), "malformed expectations:\n{}", bad_tokens.join("\n"));
    let (mut ok, mut total) = (0, 0);
    let mut below = Vec::new();
    eprintln!("intent corpus accuracy:");
    for (cat, (c_ok, c_n, misses)) in &stats {
        ok += c_ok;
        total += c_n;
        let acc = *c_ok as f64 / (*c_n).max(1) as f64;
        let floor = FLOORS.iter().find(|(c, _)| c == cat).map(|f| f.1).unwrap_or(0.0);
        eprintln!("  {cat:13} {c_ok:4}/{c_n:<4} {:5.1}%  (floor {:4.1}%)", acc * 100.0, floor * 100.0);
        let show = if verbose { misses.len() } else { misses.len().min(6) };
        for m in &misses[..show] {
            eprintln!("{m}");
        }
        if acc + 1e-9 < floor {
            below.push(format!("{cat}: {:.1}% < floor {:.1}%", acc * 100.0, floor * 100.0));
        }
    }
    let acc = ok as f64 / total.max(1) as f64;
    times.sort_by(f64::total_cmp);
    let pct = |p: f64| times.get(((times.len() as f64 - 1.0) * p) as usize).copied().unwrap_or(0.0);
    eprintln!("  TOTAL         {ok:4}/{total:<4} {:5.1}%  (floor {:.1}%)", acc * 100.0, TOTAL_FLOOR * 100.0);
    eprintln!("  intent() time: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms", pct(0.5), pct(0.99), pct(1.0));
    if only.is_none() && acc + 1e-9 < TOTAL_FLOOR {
        below.push(format!("TOTAL: {:.1}% < floor {:.1}%", acc * 100.0, TOTAL_FLOOR * 100.0));
    }
    assert!(below.is_empty(), "intent accuracy fell below its floor:\n{}", below.join("\n"));
    #[cfg(not(debug_assertions))]
    assert!(pct(0.99) < 15.0, "intent() p99 {:.2} ms >= 15 ms", pct(0.99));
}
