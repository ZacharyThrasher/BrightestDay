//! "it" / "again" grammar and player-name targeting against the real lexicon (skips without
//! assets/lexicon.bin): the rest of an "it" phrase resolves to the graft it promises, and a targeted
//! player's name never reaches the resolver as a noun.

mod common;

use bd_core::mechanics::{Element, Mechanic};
use bd_lexicon::{Refer, ResolutionKind};

fn rest_of(s: &str) -> String {
    let Some(lex) = common::lex() else { return String::new() };
    match lex.refer(s) {
        Some(Refer::It { rest }) => rest,
        other => panic!("{s:?} → {other:?}"),
    }
}

#[test]
fn it_phrases_resolve_to_grafts() {
    let Some(lex) = common::lex() else { return };
    let r = lex.resolve(&rest_of("make it bigger"));
    assert_eq!(r.kind, ResolutionKind::Graft);
    assert!(r.graft.scale_mul > 1.2, "bigger → scale ×{}", r.graft.scale_mul);
    let r = lex.resolve(&rest_of("make it smaller"));
    assert!(r.graft.scale_mul < 0.9, "smaller → scale ×{}", r.graft.scale_mul);
    let r = lex.resolve(&rest_of("set it on fire"));
    assert_eq!(r.graft.element, Some(Element::Fire));
    let r = lex.resolve(&rest_of("give it wings"));
    assert!(r.graft.mechanics.contains(&Mechanic::Flies), "{:?}", r.graft);
    let r = lex.resolve(&rest_of("give it a jetpack"));
    assert!(r.graft.mechanics.contains(&Mechanic::Thrust), "{:?}", r.graft);
    let r = lex.resolve(&rest_of("give it lasers"));
    assert!(r.graft.mechanics.contains(&Mechanic::Beam), "{:?}", r.graft);
    let r = lex.resolve(&rest_of("make it explode"));
    assert!(r.graft.mechanics.contains(&Mechanic::Explodes), "{:?}", r.graft);
    let r = lex.resolve(&rest_of("make it bouncy"));
    assert!(r.graft.mechanics.contains(&Mechanic::Bounce), "{:?}", r.graft);
    let r = lex.resolve(&rest_of("make it fly"));
    assert!(r.graft.mechanics.contains(&Mechanic::Flies), "{:?}", r.graft);
    let r = lex.resolve(&rest_of("bigger!"));
    assert!(r.graft.scale_mul > 1.2);
    assert_eq!(lex.refer("two more"), Some(Refer::Again { count: Some(2) }));
    assert_eq!(lex.refer("again!"), Some(Refer::Again { count: None }));
    assert_eq!(lex.refer("another one"), Some(Refer::Again { count: Some(1) }));
}

#[test]
fn player_names_never_leak_as_nouns() {
    let Some(lex) = common::lex() else { return };
    let names = ["Jake", "Sam", "Alex", "Rex", "Link", "Bolt"];
    for (phrase, who, form_word) in [
        ("missile at Jake", "Jake", "missile"),
        ("bees on Sam", "Sam", "bees"),
        ("drop an anvil on Alex", "Alex", "anvil"),
        ("MISSILE AT REX!!!", "Rex", "missile"),
        ("hammer at link", "Link", "hammer"),
        ("sharks on bolt", "Bolt", "shark"),
    ] {
        let t = lex.target_player(phrase, &names, false).unwrap_or_else(|| panic!("{phrase}: no target"));
        assert_eq!(names[t.player], who, "{phrase}");
        let r = lex.resolve(&t.rest);
        let who_l = who.to_lowercase();
        let blob = format!("{} {} {:?}", r.recipe.name.to_lowercase(), r.explanation.join(" ").to_lowercase(), r.ignored).to_lowercase();
        assert!(!blob.contains(&who_l), "{phrase}: the name leaked into the resolution: {blob}");
        assert!(r.recipe.name.to_lowercase().contains(form_word) || blob.contains(form_word), "{phrase} → {:?}", r.recipe.name);
        // Voice transcripts too.
        let s = lex.target_player(&phrase.to_lowercase(), &names, true).unwrap_or_else(|| panic!("{phrase}: no spoken target"));
        assert_eq!(names[s.player], who, "{phrase} (spoken)");
    }
    // A known word after "on" stays a word unless a player is literally called that.
    assert_eq!(lex.target_player("sword on fire", &["Fyre", "Sam"], false), None);
    assert!(lex.target_player("sword on fire", &["Fire"], false).is_some());
    assert_eq!(lex.target_player("king kong on a skateboard", &names, false), None);
}
