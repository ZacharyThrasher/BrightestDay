//! Pokémon moves + trainer grammar against the real assets (skipped if assets/lexicon.bin is missing or
//! predates the MOVS section). `BD_LEXICON=<path>` points the tests at another build.

use std::sync::OnceLock;

use Mechanic::*;
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, ModelSource, Temperament};
use bd_lexicon::{Lexicon, Resolution, ResolutionKind};

fn lex() -> Option<&'static Lexicon> {
    static LEX: OnceLock<Option<Lexicon>> = OnceLock::new();
    LEX.get_or_init(|| {
        let p = std::env::var_os("BD_LEXICON")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/lexicon.bin"));
        match Lexicon::load(&p) {
            // a lexicon without the move table can't answer these (rebuild with tools/lexicon_build)
            Ok(l) if l.resolve("use thunderbolt").graft.mechanics.contains(&Beam) => Some(l),
            Ok(_) => {
                eprintln!("SKIP: {} has no Pokémon move table (rebuild the lexicon)", p.display());
                None
            }
            Err(e) => {
                eprintln!("SKIP: cannot load {}: {e}", p.display());
                None
            }
        }
    })
    .as_ref()
}

fn r(text: &str) -> Option<Resolution> {
    lex().map(|l| l.resolve(text))
}

macro_rules! check {
    ($text:expr, |$res:ident| $body:block) => {{
        let Some($res) = r($text) else { return };
        let ok: bool = $body;
        assert!(
            ok,
            "{:?} -> {:?} {} {:?}\n  graft: {:?}\n  explanation: {:?}",
            $text,
            $res.kind,
            $res.signature(),
            $res.recipe,
            $res.graft,
            $res.explanation
        );
    }};
}

fn creature(x: &Resolution) -> bool {
    x.kind == ResolutionKind::Construct && x.recipe.form.category() == FormCategory::Creature
}

// ------------------------------------------------------------------------------------ bare moves = grafts
#[test]
fn thunderbolt_is_an_electric_beam_graft() {
    check!("thunderbolt", |x| {
        x.kind == ResolutionKind::Graft
            && x.graft.mechanics.contains(&Beam)
            && x.graft.element == Some(Element::Electric)
            // cast alone: an electric beam emitter
            && x.recipe.has(Beam)
            && x.recipe.element == Element::Electric
            && x.recipe.form.category() == FormCategory::Held
    });
}
#[test]
fn quick_attack_is_fast_charge() {
    check!("quick attack", |x| { x.kind == ResolutionKind::Graft && x.graft.mechanics == vec![Charge, Fast] });
}
#[test]
fn vine_whip_is_a_pulling_whip() {
    check!("vine whip", |x| {
        x.kind == ResolutionKind::Graft
            && x.graft.mechanics.contains(&Pull)
            && x.recipe.form == Form::Whip
            && x.graft.element == Some(Element::Earth)
    });
}
#[test]
fn shadow_ball_is_a_shadow_graft() {
    check!("shadow ball", |x| {
        x.kind == ResolutionKind::Graft && x.graft.element == Some(Element::Shadow) && x.graft.mechanics.contains(&Shoots)
    });
}
#[test]
fn body_slam_drops_heavy() {
    check!("body slam", |x| {
        x.kind == ResolutionKind::Graft && x.graft.mechanics.contains(&Heavy) && x.graft.mechanics.contains(&DropFromSky)
    });
}
#[test]
fn move_constructs_carry_only_their_graft() {
    // the sim grafts a recipe by merging its mechanics: a move's construct must add nothing extra
    for p in ["thunderbolt", "vine whip", "body slam", "quick attack", "shadow ball", "self destruct"] {
        check!(p, |x| { x.recipe.mechanics == x.graft.mechanics });
    }
}
#[test]
fn use_everyday_word_moves() {
    // "earthquake"/"surf" alone keep their everyday reading; "use X" is the move
    check!("use earthquake", |x| {
        x.kind == ResolutionKind::Graft && x.graft.mechanics.contains(&Stomp) && x.graft.element == Some(Element::Earth)
    });
    check!("use surf", |x| {
        x.kind == ResolutionKind::Graft && x.graft.mechanics.contains(&Repel) && x.graft.element == Some(Element::Water)
    });
    check!("use explosion", |x| { x.kind == ResolutionKind::Graft && x.graft.mechanics.contains(&Explodes) });
}
#[test]
fn splash_does_nothing() {
    check!("use splash", |x| {
        x.kind == ResolutionKind::Graft && x.graft.mechanics.is_empty() && x.graft.temperament == Some(Temperament::Playful)
    });
    let Some(base) = r("magikarp") else { return };
    check!("magikarp use splash", |x| {
        creature(&x) && x.recipe.mechanics == base.recipe.mechanics && x.recipe.temperament == Temperament::Playful
    });
}

// ------------------------------------------------------------------------------------ trainer grammar
#[test]
fn pikachu_use_thunderbolt() {
    for p in [
        "pikachu use thunderbolt",
        "Pikachu, use Thunderbolt!",
        "pikachu thunderbolt",
        "go pikachu, use thunderbolt!",
        "PIKACHU USE THUNDERBOLT",
    ] {
        check!(p, |x| {
            creature(&x) && x.recipe.has(Beam) && x.recipe.element == Element::Electric && matches!(x.recipe.model, ModelSource::Icon(_))
        });
    }
}
#[test]
fn go_and_i_choose_you_summon() {
    for p in ["go pikachu", "pikachu i choose you", "i choose you, pikachu!", "go go pikachu"] {
        check!(p, |x| { creature(&x) && x.recipe.element == Element::Electric && !x.recipe.has(Beam) });
    }
    check!("charizard, i choose you!", |x| { creature(&x) && x.recipe.element == Element::Fire });
}
#[test]
fn starters_use_their_moves() {
    check!("go charizard, use flamethrower!", |x| { creature(&x) && x.recipe.has(Breath) && x.recipe.element == Element::Fire });
    check!("blastoise use hydro pump", |x| { creature(&x) && x.recipe.has(Beam) && x.recipe.element == Element::Water });
    check!("bulbasaur use vine whip", |x| { creature(&x) && x.recipe.has(Pull) && x.recipe.element == Element::Earth });
    check!("squirtle use water gun", |x| { creature(&x) && x.recipe.has(Shoots) && x.recipe.element == Element::Water });
}
#[test]
fn famous_moves_on_pokemon() {
    check!("lapras use surf", |x| { creature(&x) && x.recipe.has(Repel) && x.recipe.element == Element::Water });
    check!("golem use earthquake", |x| { creature(&x) && x.recipe.has(Stomp) && x.recipe.element == Element::Earth });
    check!("snorlax use body slam", |x| { creature(&x) && x.recipe.has(Heavy) && x.recipe.has(DropFromSky) });
    check!("pikachu use quick attack", |x| { creature(&x) && x.recipe.has(Fast) && x.recipe.has(Charge) });
    check!("electrode use self destruct", |x| { creature(&x) && x.recipe.has(Explodes) });
    check!("slowpoke use rest", |x| { creature(&x) && x.recipe.temperament == Temperament::Lazy });
    check!("machamp use swords dance", |x| { creature(&x) && x.recipe.temperament == Temperament::Aggressive });
}
#[test]
fn movement_moves_keep_the_element() {
    check!("charizard use fly", |x| { creature(&x) && x.recipe.has(Flies) && x.recipe.element == Element::Fire });
}
#[test]
fn any_graft_phrase_works_as_a_move() {
    check!("charizard use wings", |x| { creature(&x) && x.recipe.has(Flies) });
    check!("onix use fire", |x| { creature(&x) && x.recipe.element == Element::Fire });
}
#[test]
fn typos_and_speech() {
    check!("pickachu use thundrbolt", |x| { creature(&x) && x.recipe.has(Beam) });
    let Some(l) = lex() else { return };
    let x = l.resolve_spoken("pikachu use thunder bolt");
    assert!(creature(&x) && x.recipe.has(Beam), "{:?}", x.explanation);
}
#[test]
fn modifiers_still_compose() {
    check!("giant pikachu use thunderbolt", |x| { creature(&x) && x.recipe.has(Beam) && x.recipe.scale > 0.5 });
}
#[test]
fn not_trainer_commands() {
    check!("a sword i can use", |x| { x.recipe.form == Form::Sword });
    check!("go kart", |x| { x.recipe.form == Form::Vehicle });
}
