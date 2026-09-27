//! Cast-cost survey over the golden intent corpus (`crates/bd_lexicon/tests/data/intent_golden.txt`):
//! every construct phrase through the real lexicon + `bd_sim` validation + the shipped willpower
//! tuning, bucketed by the presence tier players SEE and HEAR. The spark size / rounding in
//! `bd_sim::willpower::quantize_cost` was chosen from this. Run:
//! `cargo test -p bd_game --profile fastrel cost_survey -- --ignored --nocapture`.

use bd_core::mechanics::FormCategory;
use bd_lexicon::{Lexicon, ResolutionKind};
use bd_sim::rules::MatchRules;
use bd_sim::tuning::WillpowerTuning;
use bd_sim::willpower::validate_recipe;

use crate::present::presence::tier::{Tier, TierInput, TierKnobs, tier};

fn lex() -> Option<Lexicon> {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Lexicon::load(assets.join("lexicon.bin")).ok()
}

fn phrases() -> Vec<(String, bool)> {
    let text = include_str!("../../../bd_lexicon/tests/data/intent_golden.txt");
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let raw = l.splitn(3, '|').nth(1)?.trim();
            Some(match raw.strip_prefix('~') {
                Some(p) => (p.trim().to_string(), true),
                None => (raw.to_string(), false),
            })
        })
        .collect()
}

/// (plain cost, tier) of one phrase, or None for grafts / power incantations.
fn price(lex: &Lexicon, phrase: &str, spoken: bool) -> Option<(f32, Tier, String)> {
    let it = lex.intent(phrase, &[], spoken);
    let r = &it.resolution;
    if r.kind != ResolutionKind::Construct || r.incantation.is_some() || r.stand_rush.is_some() {
        return None;
    }
    let rules = MatchRules::default();
    let recipe = validate_recipe(r.recipe.clone(), &rules);
    let wt = WillpowerTuning::default();
    let cost = wt.base_cost(&recipe);
    let t = tier(&TierInput::of(&recipe, cost, false, false, false), &TierKnobs::default());
    Some((cost, t, format!("{:?}/{:?} {:.1}m", recipe.form, recipe.form.category(), recipe.scale)))
}

fn pct(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v[((v.len() - 1) as f32 * p).round() as usize]
}

#[test]
#[ignore = "prints the cast-cost distribution (chooses the spark size)"]
fn cost_survey() {
    let Some(lex) = lex() else {
        eprintln!("SKIP: no lexicon.bin");
        return;
    };
    let mut by_tier: [Vec<f32>; 5] = Default::default();
    let mut cats: std::collections::BTreeMap<String, Vec<f32>> = Default::default();
    for (p, spoken) in phrases() {
        if let Some((c, t, desc)) = price(&lex, &p, spoken) {
            by_tier[t.index()].push(c);
            let cat = desc.split('/').nth(1).unwrap_or("").split(' ').next().unwrap_or("").to_string();
            cats.entry(cat).or_default().push(c);
        }
    }
    let sizes: [f32; 5] = [100.0 / 6.0, 100.0 / 7.0, 12.5, 100.0 / 9.0, 10.0];
    for v in by_tier.iter_mut() {
        v.sort_by(|a, b| a.total_cmp(b));
    }
    println!("tier      n    p10    p25    p50    p75    p90");
    for t in Tier::ALL {
        let v = &by_tier[t.index()];
        println!("{:<8} {:>4} {:>6.1} {:>6.1} {:>6.1} {:>6.1} {:>6.1}", format!("{t:?}"), v.len(), pct(v, 0.1), pct(v, 0.25), pct(v, 0.5), pct(v, 0.75), pct(v, 0.9));
    }
    for (c, mut v) in cats {
        v.sort_by(|a, b| a.total_cmp(b));
        println!("cat {c:<10} n={:>4} p50={:.1} p90={:.1}", v.len(), pct(&v, 0.5), pct(&v, 0.9));
    }
    for size in sizes {
        for bias in [0.5f32, 0.35, 0.25] {
            print!("size {size:>5.2} (segments {:>2}) bias {bias:.2}:", (100.0 / size).round());
            for t in Tier::ALL {
                let v = &by_tier[t.index()];
                let mut hist = [0usize; 13];
                for c in v {
                    let n = ((c / size + bias).floor().max(1.0) as usize).min(12);
                    hist[n] += 1;
                }
                let med = pct(v, 0.5);
                print!(" {t:?} med={} [", ((med / size + bias).floor().max(1.0)) as usize);
                for (k, h) in hist.iter().enumerate().skip(1) {
                    if *h > 0 {
                        print!("{k}:{h} ");
                    }
                }
                print!("]");
            }
            println!();
        }
    }
    println!("--- examples");
    for p in ["sword", "gun", "wall", "fire dragon", "meteor", "kaiju", "spirit bomb", "hammer", "shield", "wings", "dragon", "godzilla", "giant robot", "car", "anvil", "flying hammer", "bee swarm"] {
        match price(&lex, p, false) {
            Some((c, t, d)) => println!("{p:<14} cost {c:>6.1}  {t:?}  {d}"),
            None => println!("{p:<14} (not a construct)"),
        }
    }
    let _ = FormCategory::Held;
}
