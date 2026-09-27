//! Stage-order audit (see `src/intent.rs` module docs): refer → target → delivery verb → resolver
//! (invocation/signature → pokémon → incantation → relations → compounds → grammar → leftovers →
//! signature tags). Each case is a known collision between two stages; the earlier stage must win (or
//! correctly step aside) for the phrase to mean what the player meant.

mod common;

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic};
use bd_lexicon::{Delivery, Intent, Refer, ResolutionKind, SignatureKind};

const ROSTER: [&str; 6] = ["Ethan", "Jake", "Sam", "Mia", "DragonSlayer", "Fire Lord 9000"];

fn it(s: &str) -> Option<Intent> {
    common::lex().map(|l| l.intent(s, &ROSTER, false))
}

fn target(i: &Intent) -> Option<&'static str> {
    i.target.map(|t| ROSTER[t])
}

fn has(i: &Intent, m: Mechanic) -> bool {
    i.resolution.recipe.has(m) || i.resolution.graft.mechanics.contains(&m)
}

#[test]
fn target_runs_before_relations() {
    let Some(i) = it("drop a meteor on ethan") else { return };
    assert_eq!(target(&i), Some("Ethan"));
    assert!(i.resolution.recipe.mount.is_none(), "\"on ethan\" is a target, never a mount: {:?}", i.resolution.explanation);
    assert_eq!(i.resolution.recipe.form, Form::Boulder);
    assert!(has(&i, Mechanic::DropFromSky));
    assert_eq!(i.delivery, Some(Delivery::Onto));
    let i = it("meteor on ethan").unwrap();
    assert_eq!(target(&i), Some("Ethan"));
    assert!(i.resolution.recipe.mount.is_none());
    // …and a real mount is still a mount (no player named "skateboard")
    let i = it("king kong on a skateboard").unwrap();
    assert_eq!(i.target, None);
    assert!(i.resolution.recipe.mount.is_some(), "{:?}", i.resolution.explanation);
    // "rain arrows on mia": target first, then the verb re-reads the rest as a relations volley
    let i = it("rain arrows on mia").unwrap();
    assert_eq!(target(&i), Some("Mia"));
    assert!(i.resolution.recipe.count >= 6 && has(&i, Mechanic::DropFromSky), "{:?}", i.resolution.explanation);
    assert!(i.resolution.recipe.mount.is_none());
}

#[test]
fn idioms_and_names_are_not_targets_or_mounts() {
    let Some(i) = it("sword on fire") else { return };
    assert_eq!(i.target, None, "\"on fire\" is an idiom even with a player called Fire Lord 9000");
    assert_eq!(i.resolution.recipe.form, Form::Sword);
    assert_eq!(i.resolution.recipe.element, Element::Fire);
    let i = it("on fire").unwrap();
    assert_eq!(i.resolution.kind, ResolutionKind::Graft);
    assert!(i.resolution.recipe.mount.is_none());
    let i = it("set it on fire").unwrap();
    assert!(matches!(i.refer, Some(Refer::It { .. })), "refer runs first");
    assert_eq!(i.target, None);
    assert_eq!(i.resolution.graft.element, Some(Element::Fire));
    // a player whose name contains a common word is only targeted by (enough of) the name
    let i = it("dragon on sam").unwrap();
    assert_eq!(target(&i), Some("Sam"));
    assert_eq!(i.resolution.recipe.form.category(), FormCategory::Creature);
    let i = it("sword at dragonslayer").unwrap();
    assert_eq!(target(&i), Some("DragonSlayer"));
}

#[test]
fn exact_names_beat_composition() {
    let Some(i) = it("iron man") else { return };
    assert!(i.resolution.recipe.name.to_lowercase().contains("iron man"), "{:?}", i.resolution.recipe.name);
    let i = it("man of steel").unwrap();
    assert!(i.resolution.recipe.name.to_lowercase().contains("superman"), "{:?}", i.resolution.recipe.name);
    let i = it("hot dog").unwrap();
    assert_ne!(i.resolution.recipe.element, Element::Fire, "hot dog is food");
}

#[test]
fn signatures_and_guns() {
    let Some(i) = it("final flash") else { return };
    let sig = i.resolution.signature.as_ref().expect("final flash is a signature");
    assert_eq!(sig.kind, SignatureKind::Beam);
    assert_ne!(i.resolution.recipe.form, Form::Gun, "an energy attack is a hand beam, not a gun");
    let i = it("laser gun").unwrap();
    assert_eq!(i.resolution.recipe.form, Form::Gun, "a laser gun is a gun");
    assert!(i.resolution.signature.is_none());
    // a signature aimed at someone keeps both
    let i = it("final flash at jake").unwrap();
    assert_eq!(target(&i), Some("Jake"));
    assert!(i.resolution.signature.is_some());
    let i = it("domain expansion unlimited void on sam").unwrap();
    assert_eq!(target(&i), Some("Sam"));
    assert_eq!(i.resolution.signature.as_ref().map(|s| s.kind), Some(SignatureKind::Domain));
}

#[test]
fn volleys_are_not_parties() {
    let Some(i) = it("baby shower") else { return };
    assert!(i.resolution.recipe.count <= 1 && !has(&i, Mechanic::DropFromSky), "{:?}", i.resolution.explanation);
    let i = it("meteor shower").unwrap();
    assert!(i.resolution.recipe.count >= 6 && has(&i, Mechanic::DropFromSky));
    let i = it("rain boots").unwrap();
    assert!(!has(&i, Mechanic::DropFromSky) && i.resolution.recipe.count <= 1, "rain boots are boots: {:?}", i.stages);
}

#[test]
fn delivery_verbs_are_not_modifiers() {
    // "fire" the verb is not the element; "fire sword" the modifier is not a verb
    let Some(i) = it("fire a missile at jake") else { return };
    assert_eq!(i.resolution.recipe.form, Form::Rocket);
    assert_eq!(i.delivery, Some(Delivery::At));
    assert_ne!(i.resolution.recipe.element, Element::Fire, "{:?}", i.resolution.explanation);
    let i = it("fire sword").unwrap();
    assert_eq!(i.resolution.recipe.element, Element::Fire);
    assert!(!i.stages.iter().any(|s| s.starts_with("verb")));
    let i = it("launch a rocket").unwrap();
    assert!(!i.resolution.recipe.has(Mechanic::Launch), "\"launch\" the verb adds no Launch mechanic");
    let i = it("throw a rock at jake").unwrap();
    assert_eq!((target(&i), i.delivery), (Some("Jake"), Some(Delivery::At)));
    let i = it("cage ethan").unwrap();
    assert_eq!((target(&i), i.delivery, i.resolution.recipe.form), (Some("Ethan"), Some(Delivery::Around), Form::Cage));
    let i = it("wall around sam").unwrap();
    assert_eq!((i.delivery, i.resolution.recipe.form), (Some(Delivery::Around), Form::Cage), "a wall around someone encloses them");
    let i = it("bees on sam").unwrap();
    assert_eq!(i.delivery, Some(Delivery::Near), "creatures appear next to them, not dropped on them");
}

#[test]
fn trainer_grammar_and_incantations_keep_working_behind_a_target() {
    let Some(i) = it("pikachu use thunderbolt on ethan") else { return };
    assert_eq!(target(&i), Some("Ethan"));
    assert_eq!(i.resolution.recipe.element, Element::Electric);
    let i = it("gomu gomu no pistol at jake").unwrap();
    assert_eq!(target(&i), Some("Jake"));
    assert_eq!(i.resolution.incantation.as_deref(), Some("gomu_gomu"));
}

#[test]
fn again_repeats_and_stages_are_recorded_in_order() {
    let Some(i) = it("again!") else { return };
    assert!(matches!(i.refer, Some(Refer::Again { .. })));
    assert_eq!(i.target, None, "an again never targets by itself (the repeated phrase does)");
    let i = it("make it chase jake").unwrap();
    assert_eq!(i.stages.first().map(String::as_str), Some("it"));
    assert_eq!(target(&i), Some("Jake"));
    let i = it("drop a meteor on ethan").unwrap();
    let pos = |s: &str| i.stages.iter().position(|x| x.starts_with(s));
    assert!(pos("target") < pos("verb"), "{:?}", i.stages);
}

#[test]
fn voice_names() {
    let Some(l) = common::lex() else { return };
    for (heard, who) in [
        ("drop a meteor on eaton", "Ethan"),
        ("drop a meteor on e than", "Ethan"),
        ("drop a meteor on ethan.", "Ethan"),
        ("missile at jay k", "Jake"),
        ("bees on sam.", "Sam"),
        ("anvil on top of mia", "Mia"),
        ("sword at dragon slayer", "DragonSlayer"),
    ] {
        let i = l.intent(heard, &ROSTER, true);
        assert_eq!(target(&i), Some(who), "{heard:?}: {:?}", i.resolution.explanation);
    }
}

#[test]
fn intent_never_panics() {
    let Some(l) = common::lex() else { return };
    let odd = [
        "", " ", "on", "at at at", "on top of", "drop", "drop on", "cage", "the", "on the", "x at y", "é ü ñ at sam", "rain",
        "rain of", "drop a on ethan", "at ethan", "ethan", "the dragon slayer", "missile at the", "hit with", "cage with a",
    ];
    for s in odd.iter().map(|s| s.to_string()).chain([ "on ".repeat(200), "drop a meteor on ethan ".repeat(40), "x".repeat(3000)]) {
        for spoken in [false, true] {
            let _ = l.intent(&s, &ROSTER, spoken);
            let _ = l.intent(&s, &[], spoken);
            let _ = l.intent(&s, &["", " ", "__", "é", &"n".repeat(300)], spoken);
        }
    }
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(200))]
    #[test]
    fn intent_never_panics_on_random_text(s in "[a-zA-Z0-9 _'!.,-]{0,80}", spoken in proptest::bool::ANY) {
        if let Some(l) = common::lex() {
            let _ = l.intent(&s, &ROSTER, spoken);
        }
    }
}
