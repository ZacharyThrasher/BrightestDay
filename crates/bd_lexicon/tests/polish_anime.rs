//! POLISH: ANIME — the lexicon side of "the famous line does the famous thing": attack names that used
//! to resolve to a power GRANT (so the console never borrowed the move) no longer do; the powers'
//! own names still grant.

mod common;

fn grants(p: &str) -> Option<String> {
    common::lex().and_then(|l| l.resolve(p).recipe.grants)
}

#[test]
fn attack_names_are_not_power_grants() {
    if common::lex().is_none() {
        return;
    }
    // "mugetsu" was compound-repaired into "mug + senketsu" and granted Kamui Senketsu; "za warudo"
    // was a name of The World; "godspeed" a Nen tier name (so "thunderclap and flash godspeed" split).
    for p in ["mugetsu", "za warudo", "godspeed", "thunderclap and flash godspeed"] {
        assert_eq!(grants(p), None, "{p:?} must stay a move, not a grant");
    }
}

#[test]
fn the_powers_own_names_still_grant() {
    if common::lex().is_none() {
        return;
    }
    for (p, want) in [("kamui senketsu", "life_fibers"), ("life fiber synchronize", "life_fibers"), ("the world stand", "the_world"),
                      ("transmuter", "nen@3"), ("kanmuru", "nen@3")] {
        assert_eq!(grants(p).as_deref(), Some(want), "{p:?}");
    }
}
