//! TRANSFORMER COMBAT archetypes over the REAL lexicon: what players type → the archetype the sim
//! fights with (bd_sim `constructs/brawl.rs`, combat.ron `transformer.rules`). Lives here because
//! bd_shapes' tests already see both bd_lexicon and bd_sim.

use std::sync::OnceLock;

fn lex() -> Option<&'static bd_lexicon::Lexicon> {
    static L: OnceLock<Option<bd_lexicon::Lexicon>> = OnceLock::new();
    L.get_or_init(|| bd_lexicon::Lexicon::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/lexicon.bin")).ok()).as_ref()
}

#[test]
fn phrases_land_in_their_archetype() {
    let Some(lex) = lex() else {
        eprintln!("SKIP: no assets/lexicon.bin");
        return;
    };
    let t = bd_sim::tuning::CombatTuning::default().transformer;
    for (phrase, want) in [
        ("starscream", "jet"),
        ("optimus prime", "truck"),
        ("bumblebee", "car"),
        ("a transforming tank", "tank"),
        ("a helicopter robot", "helicopter"),
        ("a motorcycle transformer", "bike"),
        ("a transforming bulldozer", "construction"),
        ("megatron", "tank"),
        ("arcee", "bike"),
        ("a transforming fighter jet", "jet"),
        ("a transforming helicopter", "helicopter"),
        ("a transforming race car", "car"),
        ("a transforming dump truck", "truck"),
    ] {
        let r = lex.intent(phrase, &[], false).resolution.recipe;
        let r = bd_sim::willpower::validate_recipe(r, &bd_sim::rules::MatchRules::default());
        assert_eq!(t.archetype_of(&r).name, want, "{phrase}: {:?} {:?}", r.form, r.features);
    }
}
