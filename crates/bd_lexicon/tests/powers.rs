//! Powers vocabulary (COMBAT.md §5h) against a fixture powers.ron (the real file is loaded the same way).

use bd_core::mechanics::{Form, Mechanic};
use bd_lexicon::{Lexicon, ResolutionKind};
use std::sync::OnceLock;

const POWERS: &str = r#"[
    Power(id: "gomu_gomu", names: ["gomu gomu no mi", "gum-gum fruit", "rubber fruit"],
          incantation: Some("gomu gomu no"), body: (traits: [BounceBody])),
    Power(id: "mera_mera", names: ["mera mera no mi", "flame-flame fruit", "fire fruit"],
          incantation: Some("mera mera no")),
    Power(id: "star_platinum", kind: Stand, names: ["star platinum"], companion: Some((form: Biped))),
    Power(id: "the_world", kind: Stand, names: ["the world", "za warudo"], companion: Some((form: Biped))),
    Power(id: "killer_queen", kind: Stand, names: ["killer queen"], companion: Some((form: Biped))),
]"#;

fn lex() -> Option<&'static Lexicon> {
    static L: OnceLock<Option<Lexicon>> = OnceLock::new();
    L.get_or_init(|| {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bytes = std::fs::read(dir.join("lexicon.bin")).ok()?;
        Lexicon::from_parts(bytes, &Lexicon::icon_files(&dir), Some(POWERS)).ok()
    })
    .as_ref()
}

#[test]
fn fruits_and_stands_grant_powers() {
    let Some(l) = lex() else { return };
    for (p, id, form) in [
        ("gum-gum fruit", "gomu_gomu", Form::Ball),
        ("gomu gomu no mi", "gomu_gomu", Form::Ball),
        ("rubber fruit", "gomu_gomu", Form::Ball),
        ("flame flame fruit", "mera_mera", Form::Ball),
        ("star platinum", "star_platinum", Form::Biped),
        ("ZA WARUDO", "the_world", Form::Biped),
        ("the world", "the_world", Form::Biped),
        ("killer queen", "killer_queen", Form::Biped),
        ("devil fruit", "random:devil_fruit", Form::Ball),
        ("stand arrow", "random:stand", Form::Spear),
    ] {
        let r = l.resolve(p);
        assert_eq!(r.recipe.grants.as_deref(), Some(id), "{p:?} -> {:?}", r.explanation);
        assert_eq!(r.recipe.form, form, "{p:?}");
    }
    assert!(l.resolve("giant gum-gum fruit").recipe.scale > 0.5, "modifiers still apply");
    assert!(l.resolve("hammer").recipe.grants.is_none());
}

#[test]
fn incantations_prefix_any_phrase() {
    let Some(l) = lex() else { return };
    let r = l.resolve("gomu gomu no pistol");
    assert_eq!(r.incantation.as_deref(), Some("gomu_gomu"));
    // a curated "gomu gomu no pistol" icon (anime pack) may win over the generic pistol
    assert!(r.recipe.form == Form::Gun || r.shape.is_some() || r.recipe.form == Form::Gauntlet, "{:?}", r.explanation);
    let r = l.resolve("gomu gomu no anvil");
    assert_eq!(r.incantation.as_deref(), Some("gomu_gomu"));
    assert_eq!(r.recipe.form, Form::Anvil);
    let r = l.resolve("Mera Mera no giant fist");
    assert_eq!(r.incantation.as_deref(), Some("mera_mera"));
    assert_eq!(r.recipe.form, Form::Gauntlet);
    assert!(l.resolve("gomu gomu no").incantation.is_some());
    assert!(l.resolve("giant hammer").incantation.is_none());
}

#[test]
fn stand_rush() {
    let Some(l) = lex() else { return };
    for (p, n) in [("ORA ORA ORA", 3), ("MUDAMUDAMUDA", 3), ("ora", 1), ("DORARARA dora", 2), ("WRYYYYY", 1)] {
        let r = l.resolve(p);
        assert_eq!(r.stand_rush, Some(n), "{p:?}");
        assert_eq!(r.kind, ResolutionKind::Graft);
        assert!(r.recipe.has(Mechanic::Blunt));
    }
    assert_eq!(l.resolve("orange").stand_rush, None);
    assert_eq!(l.resolve("dora the explorer").stand_rush, None);
}

#[test]
fn never_says_no_and_fallback_logging() {
    let Some(l) = lex() else { return };
    for p in ["", "   ", "???", "qwxzvbnm", "🔥", "hammer"] {
        let r = l.resolve(p);
        assert!(!r.recipe.name.is_empty() && r.recipe.scale > 0.0, "{p:?}");
    }
    assert!(Lexicon::is_fallback(&l.resolve("qwxzvbnm")));
    assert!(Lexicon::is_fallback(&l.resolve("")));
    assert!(Lexicon::is_fallback(&l.resolve("hammer qwxzvbnm")));
    assert!(!Lexicon::is_fallback(&l.resolve("hammer")));
    assert!(!Lexicon::is_fallback(&l.resolve("giant flaming hammer with rockets")));
    assert!(!Lexicon::is_fallback(&l.resolve("roll out")));
    assert!(!Lexicon::is_fallback(&l.resolve("ORA ORA")));
}

#[test]
fn real_powers_file_loads_cleanly() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let Ok(l) = Lexicon::load(dir.join("lexicon.bin")) else { return };
    if !dir.join("powers.ron").exists() {
        return;
    }
    assert!(!l.power_ids().is_empty());
    let w: Vec<&String> = l.load_warnings().iter().filter(|w| w.starts_with("powers.ron")).collect();
    assert!(w.is_empty(), "{w:#?}");
    // shared incantations report every candidate power; own fruit names are never stolen
    let r = l.resolve("gomu gomu no mi");
    assert_eq!(r.recipe.grants.as_deref(), Some("gomu_gomu"));
    let r = l.resolve("gomu gomu no anvil");
    assert!(!r.incantation_powers.is_empty() && r.recipe.form == Form::Anvil, "{:?}", r.incantation_powers);
}

#[test]
fn every_icon_pack_parses_without_problems() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let Ok(l) = Lexicon::load(dir.join("lexicon.bin")) else { return };
    let bad = l.validate_icon_shapes();
    assert!(bad.is_empty(), "{bad:#?}");
    let malformed: Vec<&String> = l.load_warnings().iter().filter(|w| w.contains("malformed") || w.contains("no icon list")).collect();
    assert!(malformed.is_empty(), "{malformed:#?}");
    // packs landing later (e.g. icons/tmnt.ron) are covered automatically: 0 warnings of their own
    for pack in ["icons/tmnt.ron", "icons/transformers.ron"] {
        let own: Vec<&String> = l.load_warnings().iter().filter(|w| w.starts_with(pack)).collect();
        assert!(own.is_empty(), "{pack}: {own:#?}");
    }
}

#[test]
fn mutagen_generic_is_random_specific_is_specific() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let Ok(l) = Lexicon::load(dir.join("lexicon.bin")) else { return };
    for p in ["mutagen", "the ooze", "ooze", "mutagen ooze", "mutagen canister", "tcri canister", "ooze of tcri", "tmnt ooze", "MUTAGEN"] {
        let r = l.resolve(p);
        assert_eq!(r.recipe.grants.as_deref(), Some("random:mutagen"), "{p:?} -> {:?}", r.explanation);
        assert_eq!(r.recipe.form, Form::Ball, "{p:?}");
    }
    if l.power_ids().contains(&"mutagen_rat") {
        assert_eq!(l.resolve("rat mutagen").recipe.grants.as_deref(), Some("mutagen_rat"));
        assert_eq!(l.resolve("turtle mutagen").recipe.grants.as_deref(), Some("mutagen_turtle"));
        assert_eq!(l.resolve("secret of the ooze").recipe.grants.as_deref(), Some("mutagen_turtle"));
    }
}
