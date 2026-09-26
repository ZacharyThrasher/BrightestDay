//! Signature tier (COMBAT.md §5j): invocation grammar + tagged famous ultimates (assets/signatures.ron).

mod common;

use bd_core::mechanics::{Element, Mechanic};
use bd_lexicon::{Lexicon, ResolutionKind, SignatureKind};

fn sig(l: &Lexicon, p: &str) -> Option<(SignatureKind, String)> {
    l.resolve(p).signature.map(|s| (s.kind, s.title))
}

#[test]
fn owner_phrases_are_signatures_with_sensible_elements() {
    let Some(l) = common::lex() else { return };
    use SignatureKind::*;
    for (p, kind, element) in [
        ("coffin of the iron mountain", Domain, Some(Element::Fire)),
        ("domain expansion coffin of the iron mountain", Domain, Some(Element::Fire)),
        ("DOMAIN EXPANSION: BOUNCY CASTLE", Domain, None),
        ("unlimited void", Domain, Some(Element::Psychic)),
        ("malevolent shrine", Domain, Some(Element::Shadow)),
        ("kamehameha", Beam, None),
        ("KAMEHAMEHA!!!", Beam, None),
        ("final flash", Beam, Some(Element::Electric)),
        ("bankai", Transformation, None),
        ("bankai senbonzakura kageyoshi", Transformation, None),
        ("domain expansion", Domain, None),
        ("fire domain", Domain, Some(Element::Fire)),
        ("hollow purple", Beam, Some(Element::Psychic)),
        ("spirit bomb", Summon, None),
    ] {
        let r = l.resolve(p);
        let s = r.signature.as_ref().unwrap_or_else(|| panic!("{p:?} is not a signature: {:?}", r.explanation));
        assert_eq!(s.kind, kind, "{p:?}: {:?}", r.explanation);
        assert_eq!(r.kind, ResolutionKind::Construct, "{p:?} builds");
        assert!(!s.title.is_empty() && s.title.chars().count() <= bd_lexicon::SIGNATURE_MAX_TITLE, "{p:?}: {:?}", s.title);
        if let Some(e) = element {
            assert_eq!(r.recipe.element, e, "{p:?}");
        }
    }
    // The rest of the phrase resolves normally: the bouncy castle keeps its Bounce.
    assert!(l.resolve("DOMAIN EXPANSION: BOUNCY CASTLE").recipe.has(Mechanic::Bounce));
    assert_eq!(
        sig(l, "domain expansion coffin of the iron mountain").map(|s| s.1).as_deref(),
        Some("Domain Expansion: Coffin of the Iron Mountain")
    );
    assert_eq!(sig(l, "fire domain").map(|s| s.1).as_deref(), Some("Fire Domain"));
}

#[test]
fn domain_and_expansion_no_longer_graft_flies() {
    let Some(l) = common::lex() else { return };
    for p in
        ["domain expansion coffin of the iron mountain", "DOMAIN EXPANSION: BOUNCY CASTLE", "domain expansion unlimited void", "domain"]
    {
        let r = l.resolve(p);
        assert!(!r.recipe.has(Mechanic::Flies) && !r.graft.mechanics.contains(&Mechanic::Flies), "{p:?}: {:?}", r.explanation);
    }
}

#[test]
fn ordinary_phrases_stay_ordinary() {
    let Some(l) = common::lex() else { return };
    for p in [
        "dome",
        "tent",
        "igloo",
        "hammer",
        "giant hammer",
        "laser beam",
        "shield",
        "cage",
        "a dome with spikes",
        "fire",
        "coffin",
        "HAMMER!!!",
    ] {
        let r = l.resolve(p);
        assert!(r.signature.is_none(), "{p:?} must not be a signature: {:?}", r.signature);
    }
}

#[test]
fn yelled_beams_and_icon_variants() {
    let Some(l) = common::lex() else { return };
    assert_eq!(sig(l, "HYPER BEAM!!!").map(|s| s.0), Some(SignatureKind::Beam));
    assert_eq!(sig(l, "laser beam!!!").map(|s| s.0), Some(SignatureKind::Beam));
    assert!(sig(l, "laser beam!").is_none(), "one bang is not a yell");
    // A modified famous ultimate is still that ultimate.
    assert_eq!(sig(l, "giant kamehameha").map(|s| s.0), Some(SignatureKind::Beam));
    // Keywords before a tagged name keep the keyword's title.
    assert_eq!(sig(l, "ultimate kamehameha").map(|s| s.0), Some(SignatureKind::Beam));
}

#[test]
fn spoken_invocations_work_too() {
    let Some(l) = common::lex() else { return };
    let r = l.resolve_spoken("domain expansion coffin of the iron mountain");
    assert_eq!(r.signature.map(|s| s.kind), Some(SignatureKind::Domain));
}

#[test]
fn every_tagged_name_resolves_to_something() {
    let Some(l) = common::lex() else { return };
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/signatures.ron")).expect("signatures.ron");
    let mut n = 0;
    for line in src.lines() {
        let Some(rest) = line.trim().strip_prefix("(name: \"") else { continue };
        let Some(name) = rest.split('"').next() else { continue };
        n += 1;
        let r = l.resolve(name);
        assert!(r.signature.is_some(), "tagged {name:?} is not a signature: {:?}", r.explanation);
        assert_eq!(r.kind, ResolutionKind::Construct, "{name:?}");
    }
    assert!(n >= 40, "only {n} tagged names");
}

#[test]
fn signatures_resolve_fast_and_never_panic() {
    let Some(l) = common::lex() else { return };
    for p in [
        "domain expansion",
        "domain expansion domain expansion domain expansion",
        "bankai bankai",
        "ultimate",
        "final",
        "domain",
        "   DOMAIN   ",
        "domain expansion !!!!!!!!",
        "final final final final final final final final final final final final final final final final final",
    ] {
        let _ = l.resolve(p);
        let _ = l.resolve_spoken(p);
    }
    let t = std::time::Instant::now();
    for _ in 0..50 {
        let _ = l.resolve("domain expansion coffin of the iron mountain");
    }
    let per = t.elapsed().as_secs_f64() * 1e3 / 50.0;
    assert!(per < 5.0, "{per:.2} ms per signature resolve");
}
