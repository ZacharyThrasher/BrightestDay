//! The Doorman's door (Deadlock, Valve — name only, private use): a doorway portal pair.

mod common;

use bd_core::mechanics::{Form, Mechanic, ModelSource};

fn doorway(text: &str) {
    let Some(l) = common::lex() else { return };
    let r = l.resolve(text);
    assert!(
        r.recipe.form == Form::Wall && r.recipe.has(Mechanic::Portal) && r.recipe.model == ModelSource::Icon("doorman door".into()),
        "{text:?} -> {} {:?}\n  explanation: {:?}",
        r.signature(),
        r.recipe,
        r.explanation
    );
}

#[test]
fn doorman_door_names() {
    for t in ["doorman door", "doorman's door", "the doorman's door", "doorway", "a doorway"] {
        doorway(t);
    }
}

#[test]
fn a_door_to_somewhere() {
    for t in ["door to the roof", "a door to the tower", "a door that goes to the arena", "door that leads to the other side", "door to ethan"] {
        doorway(t);
    }
}

#[test]
fn plain_door_words_stay_themselves() {
    let Some(l) = common::lex() else { return };
    // "door" alone and "doormat" are not the portal.
    for t in ["door", "doormat", "trapdoor"] {
        let r = l.resolve(t);
        assert!(!r.recipe.has(Mechanic::Portal) || r.recipe.model != ModelSource::Icon("doorman door".into()), "{t:?} -> {}", r.signature());
    }
}
