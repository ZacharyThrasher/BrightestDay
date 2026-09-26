//! SURF: skatepark / surf words resolve to the steep gliding ramp (Ramp + Glide → a 60° surf face in
//! the sim, never a launch pad). Skipped without assets/lexicon.bin.

mod common;

use bd_core::mechanics::{Form, Mechanic};

#[test]
fn surf_and_skate_ramps_resolve_to_the_gliding_ramp() {
    let Some(lex) = common::lex() else { return };
    for text in ["surf ramp", "half pipe", "halfpipe", "quarter pipe", "skate ramp"] {
        let r = lex.resolve(text);
        assert!(r.recipe.form == Form::Ramp && r.recipe.has(Mechanic::Glide), "{text:?} -> {} {:?}", r.signature(), r.recipe);
    }
    // A plain ramp stays the 30° launch ramp.
    let r = lex.resolve("ramp");
    assert!(r.recipe.form == Form::Ramp && !r.recipe.has(Mechanic::Glide), "ramp -> {:?}", r.recipe);
}
