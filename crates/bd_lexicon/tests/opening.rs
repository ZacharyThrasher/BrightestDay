//! OPENING MOVE: "<name> <move>" ("reinhardt charge", "godzilla's breath", "t rex stomp") tells the
//! sim which move to perform once right after spawning (`Recipe::opening`); a bare name has none.

mod common;

use bd_core::mechanics::Mechanic;

#[test]
fn named_moves_set_the_opening_move() {
    let Some(lex) = common::lex() else { return };
    let cases: &[(&str, Option<Mechanic>)] = &[
        ("reinhardt charge", Some(Mechanic::Charge)),
        ("godzilla's breath", Some(Mechanic::Breath)),
        ("t rex stomp", Some(Mechanic::Stomp)),
        ("reinhardt", None),
        ("godzilla", None),
        ("charge", None),
    ];
    for (phrase, want) in cases {
        let it = lex.intent(phrase, &[], false);
        assert_eq!(it.resolution.recipe.opening, *want, "{phrase}: {:?}", it.resolution.explanation);
        if let Some(m) = want {
            assert!(it.resolution.recipe.has(*m), "{phrase} has its move's mechanic");
        }
    }
}
