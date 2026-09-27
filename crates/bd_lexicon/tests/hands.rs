//! Energy attacks come from the caster's PALMS (`Form::Hands`), never a gun model (owner: "final
//! flash gave me a super powered gun, not the iconic beam"). Data: `delivery` / `energy` in
//! assets/signatures.ron.

mod common;

use bd_core::mechanics::{Form, Mechanic, ModelSource};
use bd_lexicon::{Lexicon, ResolutionKind, SignatureKind};

fn palms(l: &Lexicon, p: &str) {
    let r = l.resolve(p);
    assert_eq!(r.recipe.form, Form::Hands, "{p:?} should come from the palms: {:?} {:?}", r.recipe.form, r.explanation);
    assert_eq!(r.kind, ResolutionKind::Construct, "{p:?}");
    assert!(r.recipe.has(Mechanic::Beam), "{p:?}: {:?}", r.recipe.mechanics);
    assert!(r.shape.is_none() && matches!(r.recipe.model, ModelSource::Procedural { .. }), "{p:?}: no weapon model");
}

#[test]
fn no_energy_name_resolves_to_a_gun() {
    let Some(l) = common::lex() else { return };
    // Signature beams (typed, yelled, spoken-style lower case).
    for p in [
        "kamehameha",
        "KAMEHAMEHA!!!",
        "final kamehameha",
        "big bang kamehameha",
        "final flash",
        "FINAL FLASH!!",
        "galick gun",
        "special beam cannon",
        "masenko",
        "big bang attack",
        "spirit gun",
        "hadouken",
        "hyper beam",
        "solar beam",
        "fus ro dah",
        "ultimate beam",
        "LASER BEAM!!!",
    ] {
        palms(l, p);
        let r = l.resolve(p);
        assert_eq!(r.signature.as_ref().map(|s| s.kind), Some(SignatureKind::Beam), "{p:?}");
    }
    // Plain energy casts (not ultimates) come from the palms too.
    for p in ["repulsor", "repulsor blast", "unibeam", "ki blast", "energy blast", "energy beam", "laser beam", "photon beam"] {
        palms(l, p);
        assert!(l.resolve(p).signature.is_none(), "{p:?} is not an ultimate");
    }
}

#[test]
fn guns_stay_guns_and_other_deliveries_keep_their_body() {
    let Some(l) = common::lex() else { return };
    for p in ["gun", "laser gun", "railgun", "RAILGUN!!!", "blaster", "ray gun", "death ray", "shrink ray", "sword beam"] {
        let r = l.resolve(p);
        assert_ne!(r.recipe.form, Form::Hands, "{p:?} names a gun");
        assert!(r.signature.is_none(), "{p:?}: a yelled gun is not an ultimate");
    }
    // Rasengan is a thrown orb, the Spirit Bomb a summoned ball, Hollow Purple the erasing orb,
    // Getsuga Tensho keeps the sword, Avada Kedavra the wand.
    for (p, form, kind) in [
        ("rasengan", Form::Ball, SignatureKind::Summon),
        ("spirit bomb", Form::Ball, SignatureKind::Summon),
        ("hollow purple", Form::Cannon, SignatureKind::Beam),
        ("getsuga tensho", Form::Sword, SignatureKind::Transformation),
        ("avada kedavra", Form::Staff, SignatureKind::Beam),
    ] {
        let r = l.resolve(p);
        assert_eq!(r.recipe.form, form, "{p:?}: {:?}", r.explanation);
        assert_eq!(r.signature.as_ref().map(|s| s.kind), Some(kind), "{p:?}");
    }
}

/// Guard techniques against a domain resolve to a small Guard dome — never a domain themselves; and
/// a named domain brings its technique (Malevolent Shrine slashes).
#[test]
fn simple_domain_is_a_guard_technique() {
    let Some(l) = common::lex() else { return };
    for p in ["simple domain", "counter domain", "domain amplification", "shield domain", "SIMPLE DOMAIN!!"] {
        let r = l.resolve(p);
        assert!(r.signature.is_none(), "{p:?} is not a domain: {:?}", r.signature);
        assert_eq!(r.recipe.form, Form::Dome, "{p:?}");
        assert!(r.recipe.has(Mechanic::Guard) && r.recipe.scale < 8.0, "{p:?}: {:?}", r.recipe);
    }
    for p in ["fire domain", "domain expansion unlimited void"] {
        assert_eq!(l.resolve(p).signature.map(|s| s.kind), Some(SignatureKind::Domain), "{p:?} is still a domain");
    }
    let shrine = l.resolve("malevolent shrine");
    assert!(shrine.recipe.has(Mechanic::Blade), "the shrine slashes: {:?}", shrine.recipe.mechanics);
}

/// Canonical titles for the title letters ("did it hear me right?").
#[test]
fn signature_titles_are_canonical() {
    let Some(l) = common::lex() else { return };
    for (p, title) in [
        ("final flash", "Final Flash"),
        ("FINAL FLASH!!", "Final Flash"),
        ("kamehameha", "Kamehameha"),
        ("galick gun", "Galick Gun"),
        ("galic gun", "Galick Gun"),
        ("special beam cannon", "Special Beam Cannon"),
    ] {
        let r = l.resolve(p);
        assert_eq!(r.signature.map(|s| s.title).as_deref(), Some(title), "{p:?}");
    }
}

#[test]
#[ignore]
fn probe_energy_names() {
    let Some(l) = common::lex() else { return };
    for p in [
        "kamehameha",
        "final flash",
        "galick gun",
        "special beam cannon",
        "masenko",
        "big bang attack",
        "spirit gun",
        "hadouken",
        "rasengan",
        "rasenshuriken",
        "getsuga tensho",
        "repulsor",
        "unibeam",
        "hyper beam",
        "avada kedavra",
        "ki blast",
        "energy beam",
        "laser beam",
        "ultimate beam",
        "final fire",
        "ultimate laser",
        "hollow purple",
        "railgun",
        "RAILGUN!!!",
        "laser gun",
        "light beam",
    ] {
        let r = l.resolve(p);
        println!(
            "{p:24} -> {:?} {:?} scale {:.2} mech {:?} el {:?} shape {} sig {:?}",
            r.kind,
            r.recipe.form,
            r.recipe.scale,
            r.recipe.mechanics,
            r.recipe.element,
            r.shape.is_some(),
            r.signature.as_ref().map(|s| (s.kind, s.title.clone())),
        );
    }
}
