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

/// PLAYTEST BUG: "optimus prime" came out as the costume biped (the COSTUME SUMMON HOOK replaced
/// the Transformers icon), so transforming gave a blocky humanoid, not a truck. A character whose
/// resolution transforms keeps its own two-mode body: the vehicle / beast alt mode, the curated
/// robot shape, its archetype. Plain characters still wear their costume.
#[test]
fn transformer_characters_keep_their_alt_mode_through_the_costume_hook() {
    use bd_core::mechanics::{Form, Mechanic};
    let Some(lex) = lex() else {
        eprintln!("SKIP: no assets/lexicon.bin");
        return;
    };
    let t = bd_sim::tuning::CombatTuning::default().transformer;
    for (phrase, form, want) in [
        ("optimus prime", Form::Vehicle, "truck"),
        ("optimus", Form::Vehicle, "truck"),
        ("bumblebee", Form::Vehicle, "car"),
        ("megatron", Form::Vehicle, "tank"),
        ("starscream", Form::Vehicle, "jet"),
        ("grimlock", Form::Kaiju, "dino"),
        ("soundwave", Form::Biped, "car"),
        ("arcee", Form::Vehicle, "bike"),
        ("optimus primal", Form::Quadruped, "beast"),
        ("jazz transformer", Form::Vehicle, "car"),
    ] {
        let res = lex.intent(phrase, &[], false).resolution;
        let (mut r, mut shape) = (res.recipe, res.shape);
        // The console's costume hook (console.rs / toolbox.rs), exactly as a summon runs it.
        let costumed = bd_shapes::costume::summon::apply(phrase, &mut r, &mut shape);
        assert!(!costumed, "{phrase}: the costume must not replace a transformer's body");
        assert!(r.has(Mechanic::Transforms), "{phrase}");
        assert_eq!(r.form, form, "{phrase}: the alt mode");
        assert!(res.alt_shape.is_some(), "{phrase}: the curated robot mode");
        let r = bd_sim::willpower::validate_recipe(r, &bd_sim::rules::MatchRules::default());
        assert_eq!(t.archetype_of(&r).name, want, "{phrase}");
    }
    // Plain characters still wear their costume.
    for phrase in ["goku", "reinhardt"] {
        let res = lex.intent(phrase, &[], false).resolution;
        let (mut r, mut shape) = (res.recipe, res.shape);
        assert!(bd_shapes::costume::summon::apply(phrase, &mut r, &mut shape), "{phrase} wears its costume");
    }
}
