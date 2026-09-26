//! End-to-end resolution tests against the real assets (skipped if assets/lexicon.bin is missing).

mod common;

use Mechanic::*;
use bd_core::mechanics::{Element, Feature, Form, FormCategory, Mechanic, ModelSource, Temperament};
use bd_lexicon::{GraftAction, Resolution, ResolutionKind};

fn r(text: &str) -> Option<Resolution> {
    common::lex().map(|l| l.resolve(text))
}

fn has_all(res: &Resolution, ms: &[Mechanic]) -> bool {
    ms.iter().all(|m| res.recipe.has(*m))
}

macro_rules! check {
    ($text:expr, |$res:ident| $body:block) => {{
        let Some($res) = r($text) else { return };
        let ok: bool = $body;
        assert!(ok, "{:?} -> {} {:?}\n  explanation: {:?}", $text, $res.signature(), $res.recipe, $res.explanation);
    }};
}

// ---------------------------------------------------------------------------------------- vocabulary
#[test]
fn maul_is_a_hammer() {
    check!("maul", |x| { x.recipe.form == Form::Hammer && x.explanation.iter().any(|e| e.contains("hammer")) });
}
#[test]
fn parachute_slows_falls() {
    check!("parachute", |x| { x.recipe.has(SlowFall) });
}
#[test]
fn claw_grabs() {
    check!("claw", |x| { x.recipe.has(Grab) || x.graft.mechanics.contains(&Grab) });
}
#[test]
fn scorpion_has_a_tail() {
    check!("scorpion", |x| { x.recipe.has(ArticulatedTail) && x.recipe.form.category() == FormCategory::Creature });
}
#[test]
fn godzilla_is_an_icon_kaiju() {
    check!("godzilla", |x| { x.recipe.form == Form::Kaiju && matches!(x.recipe.model, ModelSource::Icon(_)) && x.shape.is_some() });
}
#[test]
fn pikachu_is_an_electric_creature() {
    check!("pikachu", |x| { x.recipe.element == Element::Electric && x.recipe.form.category() == FormCategory::Creature });
}
#[test]
fn charizard_breathes_fire_and_flies() {
    check!("charizard", |x| { x.recipe.element == Element::Fire && has_all(&x, &[Flies, Breath]) });
}
#[test]
fn captain_americas_shield() {
    check!("captain america's shield", |x| { x.recipe.form == Form::Shield && has_all(&x, &[Returning, Ricochet]) && x.shape.is_some() });
}
#[test]
fn three_flaming_swords() {
    check!("three flaming swords", |x| { x.recipe.form == Form::Sword && x.recipe.count == 3 && x.recipe.element == Element::Fire });
}
#[test]
fn typos() {
    check!("hamer", |x| { x.recipe.form == Form::Hammer && x.explanation.iter().any(|e| e.contains("typo")) });
    check!("godzila", |x| { x.recipe.form == Form::Kaiju });
    check!("captian america sheild", |x| { x.recipe.form == Form::Shield });
    check!("gianttt hammmer", |x| { x.recipe.form == Form::Hammer && x.recipe.scale > 1.5 });
    check!("flamin swrod", |x| { x.recipe.form == Form::Sword && x.recipe.element == Element::Fire });
}
#[test]
fn unknown_word_is_a_blob_with_zero_confidence() {
    check!("qwxzvbnm", |x| { x.recipe.form == Form::Blob0 && x.confidence == 0.0 && !x.ignored.is_empty() });
}

// ---------------------------------------------------------------------------------------- composition
#[test]
fn compositional_phrases() {
    check!("a giant flaming hammer with rockets that returns", |x| {
        x.recipe.form == Form::Hammer && x.recipe.element == Element::Fire && has_all(&x, &[Thrust, Returning]) && x.recipe.scale > 1.5
    });
    check!("electric shark swarm", |x| {
        x.recipe.form == Form::Fish && x.recipe.element == Element::Electric && x.recipe.has(Swarm) && x.recipe.count >= 6
    });
    check!("bouncy anvil", |x| { x.recipe.form == Form::Anvil && x.recipe.has(Bounce) });
    check!("returning axe of ice", |x| { x.recipe.form == Form::Axe && x.recipe.has(Returning) && x.recipe.element == Element::Ice });
    check!("exploding boomerang", |x| { has_all(&x, &[Explodes, Returning]) });
    check!("a sticky bomb", |x| { x.recipe.form == Form::Bomb && x.recipe.has(Sticky) });
    check!("a wall of fire", |x| { x.recipe.form == Form::Wall && x.recipe.element == Element::Fire });
    check!("flying snake", |x| { x.recipe.form == Form::Serpent && x.recipe.has(Flies) });
    check!("a spiked shield", |x| {
        x.recipe.form == Form::Shield && x.recipe.has(Pierce) && x.recipe.features.contains(&Feature::Spikes)
    });
    check!("flaming mjolnir", |x| { x.recipe.form == Form::Hammer && x.recipe.element == Element::Fire && x.shape.is_some() });
    check!("three pikachus", |x| { x.recipe.count == 3 && x.recipe.element == Element::Electric });
    check!("shark with wings", |x| { x.recipe.form == Form::Fish && x.recipe.has(Flies) });
    check!("a hammer that explodes", |x| { x.recipe.form == Form::Hammer && x.recipe.has(Explodes) });
    check!("sword that shoots fire", |x| { x.recipe.form == Form::Sword && x.recipe.has(Shoots) && x.recipe.element == Element::Fire });
    check!("swarm of bees", |x| { x.recipe.form == Form::Insect && x.recipe.has(Swarm) && x.recipe.count >= 6 });
    check!("army of angry skeletons", |x| { x.recipe.count >= 6 && x.recipe.temperament == Temperament::Aggressive });
    check!("hammer made of ice", |x| { x.recipe.form == Form::Hammer && x.recipe.element == Element::Ice });
    check!("rocket powered sledgehammer", |x| { x.recipe.form == Form::Hammer && x.recipe.has(Thrust) });
    check!("a bouncy wall", |x| { x.recipe.form == Form::Wall && x.recipe.has(Bounce) });
    check!("homing trampoline", |x| { x.recipe.form == Form::Trampoline && x.recipe.has(Homing) });
    check!("flying cage", |x| { x.recipe.form == Form::Cage && x.recipe.has(Flies) });
    check!("spinning pillar", |x| { x.recipe.form == Form::Pillar && x.recipe.has(Spin) });
    check!("wall with legs", |x| { x.recipe.form == Form::Wall && x.recipe.has(Walks) });
    check!("sticky rocket", |x| { x.recipe.form == Form::Rocket && x.recipe.has(Sticky) });
    check!("tiny godzilla", |x| { x.recipe.form == Form::Kaiju && x.recipe.scale < 25.0 });
    check!("firesword", |x| { x.recipe.form == Form::Sword && x.recipe.element == Element::Fire });
    check!("sharknado", |x| { x.recipe.has(Spin) });
    check!("catnado", |x| { x.recipe.has(Spin) && x.explanation.iter().any(|e| e.contains("tornado")) });
}

#[test]
fn order_insensitive_signatures() {
    let (Some(a), Some(b)) = (r("giant flaming hammer"), r("flaming giant hammer")) else {
        return;
    };
    assert_eq!(a.signature(), b.signature());
    assert_eq!(a.signature().key(), b.signature().key());
    let (Some(a), Some(b)) = (r("frozen flaming sword"), r("flaming frozen sword")) else {
        return;
    };
    assert_eq!(a.signature(), b.signature());
}

#[test]
fn payloads() {
    check!("shark cannon", |x| { x.recipe.form == Form::Cannon && x.recipe.payload.as_ref().map(|p| p.form) == Some(Form::Fish) });
    // "bee gun" is a curated game item (a gun of homing swarms); either reading is fine
    check!("bee gun", |x| { x.recipe.payload.as_ref().map(|p| p.form) == Some(Form::Insect) || x.recipe.has(Swarm) });
    check!("anvil launcher", |x| { x.recipe.payload.as_ref().map(|p| p.form) == Some(Form::Anvil) });
    check!("cannon that shoots anvils", |x| { x.recipe.payload.as_ref().map(|p| p.form) == Some(Form::Anvil) });
    check!("cloud raining anvils", |x| { x.recipe.payload.as_ref().map(|p| p.form == Form::Anvil && p.has(DropFromSky)).unwrap_or(false) });
    check!("giant eeyore shootin laser beams", |x| {
        x.recipe.form == Form::Quadruped && x.recipe.has(Beam) && x.recipe.scale >= 2.5 && x.recipe.payload.is_none()
    });
    check!("GIANT EEYORE SHOOTIN LASER BEAMS", |x| { x.recipe.form == Form::Quadruped && x.recipe.has(Beam) });
    check!("dragon that breathes ice", |x| { x.recipe.has(Breath) && x.recipe.element == Element::Ice });
}

#[test]
fn wild_phrases_do_something() {
    for p in [
        "giant eeyore shootin laser beams",
        "a toaster that shoots bees",
        "electric bees with jetpacks",
        "gigantic angry pikachu breathing fire",
        "tiny sleepy godzilla",
        "a moon made of cheese",
        "shark tornado",
        "catnado",
        "ninja pirate robot",
        "angry flying piano",
        "exploding chicken swarm",
        "a sword made of lightning",
        "fire breathing hamster",
        "a planet sized hammer",
        "the biggest meanest dragon ever",
        "hot dog cannon",
        "bouncy castle full of bees",
        "a very very big spoon",
        "steampunk octopus",
        "rocket skates",
        "chainsaw with legs",
    ] {
        let Some(x) = r(p) else { return };
        assert!(x.recipe.form != Form::Blob0 && x.confidence > 0.2, "{p:?} -> {} {:?}", x.signature(), x.explanation);
    }
}

// ---------------------------------------------------------------------------------------- features / mood
#[test]
fn features_from_knowledge_and_modifiers() {
    check!("steam powered train", |x| {
        x.recipe.form == Form::Vehicle
            && has_all(&x, &[Charge, Heavy])
            && x.recipe.features
                == vec![Feature::Wheels, Feature::Pistons, Feature::Boiler, Feature::Smokestack, Feature::Cab, Feature::Plow]
    });
    check!("clockwork dragon", |x| { x.recipe.has(Flies) && x.recipe.features.contains(&Feature::Gears) });
    check!("armored shark with a drill", |x| {
        x.recipe.form == Form::Fish
            && x.recipe.features.contains(&Feature::Armor)
            && x.recipe.features.contains(&Feature::Drill)
            && x.recipe.has(Guard)
    });
    check!("helicopter", |x| { x.recipe.features.contains(&Feature::Rotor) });
    check!("tank", |x| { x.recipe.features.contains(&Feature::Treads) && x.recipe.features.contains(&Feature::Turret) });
}

#[test]
fn temperament_and_intensity() {
    check!("very angry bees", |x| {
        x.recipe.form == Form::Insect
            && has_all(&x, &[Swarm, Flies, Pierce])
            && x.recipe.temperament == Temperament::Aggressive
            && (0.8..=0.9).contains(&x.recipe.intensity)
    });
    check!("slightly angry bees", |x| { x.recipe.temperament == Temperament::Aggressive && x.recipe.intensity < 0.45 });
    check!("ANGRY bees", |x| { x.recipe.intensity >= 0.99 });
    check!("sleepy bear", |x| { x.recipe.temperament == Temperament::Lazy });
    check!("rage", |x| {
        x.recipe.form == Form::Blob && x.recipe.element == Element::Fire && x.recipe.temperament == Temperament::Aggressive
    });
    check!("sadness", |x| { x.recipe.element == Element::Water && x.recipe.temperament == Temperament::Lazy });
    check!("love", |x| { x.recipe.temperament == Temperament::Protective });
    check!("pissed off shark", |x| { x.recipe.temperament == Temperament::Aggressive });
}

#[test]
fn literal_senses_and_profanity() {
    check!("horny goku", |x| {
        x.recipe.form == Form::Biped && x.recipe.has(Charge) && x.explanation.iter().any(|e| e.contains("horned"))
    });
    check!("fucking huge sword", |x| { x.recipe.form == Form::Sword && x.recipe.scale > 2.5 });
    check!("dick rocket ship", |x| { x.recipe.has(Thrust) });
}

#[test]
fn piloting_and_scale() {
    check!("a dragon i can ride", |x| { x.recipe.has(Piloted) && x.recipe.has(Flies) });
    check!("ride a horse", |x| { x.recipe.has(Piloted) });
    check!("mech suit", |x| { x.recipe.has(Piloted) && x.recipe.scale < 5.0 });
    check!("throw a moon at him", |x| { x.recipe.scale >= 30.0 && x.recipe.form.category() == FormCategory::Object });
    check!("planet sized hammer", |x| { x.recipe.form == Form::Hammer && x.recipe.scale >= 50.0 });
}

// ---------------------------------------------------------------------------------------- grafts
#[test]
fn graft_phrases() {
    let g = |p: &str, m: Mechanic| {
        let Some(x) = r(p) else { return };
        assert!(x.graft.mechanics.contains(&m), "{p:?} graft {:?}", x.graft);
    };
    check!("legs", |x| { x.kind == ResolutionKind::Graft && x.graft.mechanics.contains(&Walks) });
    check!("make it explode", |x| { x.kind == ResolutionKind::Graft && x.graft.mechanics == vec![Explodes] });
    check!("bouncy", |x| { x.kind == ResolutionKind::Graft && x.graft.mechanics.contains(&Bounce) });
    check!("that returns", |x| { x.kind == ResolutionKind::Graft && x.graft.mechanics.contains(&Returning) });
    check!("spikes", |x| { x.kind == ResolutionKind::Graft && x.graft.features.contains(&Feature::Spikes) });
    check!("fire", |x| { x.graft.element == Some(Element::Fire) });
    check!("angry", |x| { x.kind == ResolutionKind::Graft && x.graft.temperament == Some(Temperament::Aggressive) });
    g("wings", Flies);
    g("fly", Flies);
    g("rockets", Thrust);
    g("walk", Walks);
    g("come alive", Walks);
    g("spin", Spin);
    g("magnet", Attract);
    g("explode", Explodes);
    g("give it wings", Flies);
}

#[test]
fn new_mechanics() {
    check!("portal gun", |x| { x.recipe.form == Form::Gun && x.recipe.has(Portal) });
    check!("black hole", |x| { has_all(&x, &[Attract, Explodes]) });
    check!("tractor beam", |x| { has_all(&x, &[Beam, Attract]) });
    check!("shrink ray", |x| { x.recipe.has(Shrink) });
    check!("super mushroom", |x| { x.recipe.has(Grow) });
    check!("ball and chain", |x| { x.recipe.has(Link) });
    check!("leaf blower", |x| { x.recipe.has(Repel) });
}

// ---------------------------------------------------------------------------------------- voice
#[test]
fn spoken_sound_alikes() {
    let Some(l) = common::lex() else { return };
    for (heard, want) in [
        ("picked you", "pikachu"),
        ("pick a chew", "pikachu"),
        ("mule near", "mjolnir"),
        ("mjolner", "mjolnir"),
        ("god zilla", "godzilla"),
        ("char is ard", "charizard"),
    ] {
        let x = l.resolve_spoken(heard);
        assert!(x.recipe.name.to_lowercase().contains(want), "{heard:?} -> {:?} {:?}", x.recipe.name, x.explanation);
    }
    assert!(l.bias_prompt(30).to_lowercase().contains("mjolnir"));
}

// ---------------------------------------------------------------------------------------- completion / safety
#[test]
fn completion() {
    let Some(l) = common::lex() else { return };
    let c = l.complete("captain am");
    assert!(c.iter().any(|s| s.contains("captain america")), "{c:?}");
    let c = l.complete("ham");
    assert!(c.iter().any(|s| s == "hammer"), "{c:?}");
    let c = l.complete("three flaming sw");
    assert!(c.iter().any(|s| s.starts_with("three flaming sword")), "{c:?}");
    assert!(l.complete("").is_empty());
}

#[test]
fn names_and_slurs() {
    let Some(l) = common::lex() else { return };
    assert!(l.is_blocked_name("H1tl3r_fan"));
    assert!(l.is_blocked_name("KKK Lord"));
    assert!(!l.is_blocked_name("RaccoonKing"));
    assert!(!l.is_blocked_name("Pakistan Pete"));
    assert!(!l.is_blocked_name("fire retardant"));
    assert!(!l.is_blocked_name("xX_fuckboy_Xx"), "profanity is allowed");
    // a blocked token is ignored, the rest still resolves
    let x = l.resolve("nazi hammer");
    assert_eq!(x.recipe.form, Form::Hammer);
    assert!(!x.ignored.is_empty());
}

#[test]
fn random_phrases_are_deterministic_and_resolve() {
    let Some(l) = common::lex() else { return };
    for seed in 0..200u64 {
        let p = l.random_phrase(seed);
        assert_eq!(p, l.random_phrase(seed));
        let x = l.resolve(&p);
        assert!(x.recipe.scale.is_finite() && x.recipe.count >= 1, "{p:?}");
    }
}

#[test]
fn speed_budget() {
    let Some(l) = common::lex() else { return };
    let phrases: Vec<String> = (0..300u64).map(|s| l.random_phrase(s)).collect();
    let t = std::time::Instant::now();
    for p in &phrases {
        std::hint::black_box(l.resolve(p));
    }
    let avg_us = t.elapsed().as_secs_f64() * 1e6 / phrases.len() as f64;
    eprintln!("average resolve: {avg_us:.0} µs");
    // tests build at opt-level 1; release is ~3x faster. Budget: < 1 ms in release.
    assert!(avg_us < 2500.0, "average {avg_us} µs");
}

// ---------------------------------------------------------------------------------------- transformers
#[test]
fn transformers_vocabulary() {
    check!("transformer shark", |x| { x.recipe.form == Form::Fish && x.recipe.has(Transforms) });
    check!("a tank that transforms", |x| { x.recipe.form == Form::Vehicle && x.recipe.has(Transforms) });
    check!("transforming car", |x| { x.recipe.form == Form::Vehicle && x.recipe.has(Transforms) });
    check!("robot in disguise", |x| { x.recipe.has(Transforms) });
    check!("robots in disguise", |x| { x.recipe.has(Transforms) && x.recipe.count >= 3 });
    check!("transformer", |x| { x.recipe.has(Transforms) });
    check!("autobots", |x| {
        x.recipe.form == Form::Biped && x.recipe.count == 3 && x.recipe.has(Transforms) && x.recipe.temperament == Temperament::Protective
    });
    check!("decepticons", |x| { x.recipe.count == 3 && x.recipe.has(Transforms) && x.recipe.temperament == Temperament::Aggressive });
    check!("optimus prime", |x| { x.recipe.has(Transforms) });
}

#[test]
fn transformer_commands() {
    let act = |p: &str, a: GraftAction| {
        let Some(x) = r(p) else { return };
        assert_eq!(x.graft.action, Some(a), "{p:?} -> {:?}", x.explanation);
        assert_eq!(x.kind, ResolutionKind::Graft, "{p:?}");
        assert!(x.graft.mechanics.contains(&Transforms), "{p:?}");
    };
    act("roll out", GraftAction::RollOut);
    act("Autobots, roll out!", GraftAction::RollOut);
    act("transform and roll out", GraftAction::RollOut);
    act("transform", GraftAction::Transform);
    act("make it transform", GraftAction::Transform);
    act("robot mode", GraftAction::Transform);
    act("combine", GraftAction::Combine);
    act("form devastator", GraftAction::Combine);
    check!("form devastator", |x| { x.recipe.form == Form::Kaiju || x.recipe.scale >= 10.0 });
    // ordinary phrases carry no action
    check!("hammer", |x| { x.graft.action.is_none() });
}

#[test]
fn all_icon_shapes_parse() {
    let Some(l) = common::lex() else { return };
    let bad = l.validate_icon_shapes();
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(l.icon_count() >= 97);
}

#[test]
fn seastone_is_a_water_link() {
    for p in ["seastone chain", "sea stone handcuffs", "seastone cuffs", "sea-prism stone net", "kairoseki", "seastone", "seastone bullet"]
    {
        check!(p, |x| {
            x.recipe.element == Element::Water
                && x.recipe.has(Link)
                && (x.recipe.name.to_lowercase().contains("seastone")
                    || x.recipe.name.to_lowercase().contains("sea stone")
                    || x.recipe.name.to_lowercase().contains("sea prism")
                    || x.recipe.name.to_lowercase().contains("kairoseki"))
        });
    }
    check!("seastone chain", |x| { x.recipe.name.contains("seastone") });
}

/// Exact curated names beat composition grammar: every icon / power alias that exactly names something
/// ("man of steel", "eye of sauron", "ball and chain", "sword of omens") resolves to that thing, not to
/// an "X of Y" material/graft reading.
#[test]
fn exact_names_beat_phrase_grammar() {
    let Some(l) = common::lex() else { return };
    let grammar = [" of ", " with ", " and ", " that ", " made ", " on ", " in ", " the "];
    let mut checked = 0;
    let mut bad = Vec::new();
    for d in l.icons() {
        for n in d.names.iter().chain(std::iter::once(&d.name)) {
            let padded = format!(" {} ", n.to_lowercase());
            if !grammar.iter().any(|g| padded.contains(g)) {
                continue;
            }
            // only aliases this icon actually owns (earlier packs / common words may own the rest)
            let Some(owner) = l.lookup(n) else { continue };
            if owner.name != d.recipe().name {
                continue;
            }
            checked += 1;
            let r = l.resolve(n);
            if r.recipe.name != owner.name || r.recipe.form != owner.form || r.recipe.grants != owner.grants {
                bad.push(format!("{n:?} -> {} ({:?}) expected {} ({:?})", r.recipe.name, r.recipe.form, owner.name, owner.form));
            }
        }
    }
    eprintln!("exact-name audit: {checked} grammar-looking names checked, {} failed", bad.len());
    assert!(checked > 50, "only {checked} names checked");
    assert!(bad.is_empty(), "{} failures:\n{}", bad.len(), bad.join("\n"));
    for (p, want) in [
        ("man of steel", "superman"),
        ("eye of sauron", "sauron"),
        ("hand of doom", "hand of doom"),
        ("sword of omens", "omens"),
        ("ball and chain", "ball and chain"),
        ("one ring", "ring"),
        ("pym particles", "pym"),
    ] {
        let r = l.resolve(p);
        assert!(r.recipe.name.to_lowercase().contains(want), "{p:?} -> {:?} {:?}", r.recipe.name, r.explanation);
    }
    check!("tree of life", |x| { x.recipe.form == Form::Pillar });
}

#[test]
fn random_origin_phrases() {
    let Some(l) = common::lex() else { return };
    for p in ["mystery origin", "random superpower", "superpower", "super serum", "origin story", "compound v"] {
        let r = l.resolve(p);
        assert_eq!(r.recipe.grants.as_deref(), Some("random:origin"), "{p:?} -> {:?}", r.explanation);
    }
}
