//! Everyday-phrase regression: tests/data/everyday_phrases.txt lists ~400 things players type with the
//! expected reading (Form, category, required mechanic, or graft). Compounds must stay units
//! ("hot dog" is food, not a burning dog) and common words must not be hijacked by pop-culture icons.

mod common;

use bd_core::mechanics::{FormCategory, Mechanic};
use bd_lexicon::{Lexicon, Resolution, ResolutionKind};

pub struct Expect {
    pub phrase: String,
    pub want: String,
    pub mech: Option<String>,
}

pub fn phrases() -> Vec<Expect> {
    include_str!("data/everyday_phrases.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut it = l.split('|').map(str::trim);
            let phrase = it.next()?.to_string();
            let want = it.next()?.to_string();
            Some(Expect { phrase, want, mech: it.next().map(String::from) })
        })
        .collect()
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

pub fn check(r: &Resolution, e: &Expect) -> bool {
    let form_ok = match e.want.as_str() {
        "@graft" => r.kind == ResolutionKind::Graft,
        w if w.starts_with('@') => category(&w[1..]).map(|c| r.recipe.form.category() == c).unwrap_or(false),
        w => bd_lexicon::tables::form_from_name(w).map(|f| r.recipe.form == f).unwrap_or(false),
    };
    let mech_ok = match &e.mech {
        None => true,
        Some(m) => {
            bd_lexicon::tables::mechanic_from_name(m).map(|m: Mechanic| r.recipe.has(m) || r.graft.mechanics.contains(&m)).unwrap_or(false)
        }
    };
    form_ok && mech_ok
}

#[test]
fn everyday_phrases_resolve_as_expected() {
    let Some(l) = common::lex() else { return };
    let list = phrases();
    assert!(list.len() >= 400, "phrase list has {}", list.len());
    let mut bad = Vec::new();
    for e in &list {
        let r = l.resolve(&e.phrase);
        let fell_back = Lexicon::is_fallback(&r) && e.want != "@graft";
        if !check(&r, e) || fell_back {
            bad.push(format!(
                "{:28} want {:10} {:10} got {:?}/{} conf {:.2}{} [{}]",
                e.phrase,
                e.want,
                e.mech.as_deref().unwrap_or(""),
                r.kind,
                r.signature(),
                r.confidence,
                if fell_back { " FALLBACK" } else { "" },
                r.explanation.first().cloned().unwrap_or_default()
            ));
        }
    }
    let rate = 1.0 - bad.len() as f64 / list.len() as f64;
    eprintln!("everyday phrases: {}/{} as expected ({:.1}%)", list.len() - bad.len(), list.len(), rate * 100.0);
    assert!(bad.is_empty(), "{} unexpected:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn compounds_are_units_and_common_words_are_not_hijacked() {
    let Some(l) = common::lex() else { return };
    // the modifier reading must NOT win for established compounds
    for p in ["hot dog", "big mac", "hot potato", "cold shoulder", "hot rod", "cold war", "big bang"] {
        let r = l.resolve(p);
        assert!(
            !r.explanation.iter().any(|x| x.starts_with("hot \u{2192}") || x.starts_with("big \u{2192}")),
            "{p:?} was split: {:?}",
            r.explanation
        );
    }
    // but free combinations still compose
    let r = l.resolve("hot hammer");
    assert!(r.recipe.form == bd_core::mechanics::Form::Hammer && r.recipe.element == bd_core::mechanics::Element::Fire);
    // "punch" is an everyday word: a curated icon named Punch can't take it
    let r = l.resolve("punch");
    assert!(r.recipe.form.category() != FormCategory::Creature, "punch -> {:?}", r.explanation);
    let r = l.resolve("sucker punch");
    assert_eq!(r.recipe.form, bd_core::mechanics::Form::Gauntlet);
}
