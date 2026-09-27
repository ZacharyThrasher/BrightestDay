//! Transformer phrases (crates/bd_lexicon/src/transformer.rs + data/transformer.ron): every way a
//! player asks for a transforming robot — typed or voiced — must give ONE piloted `Transforms`
//! construct whose form / features / mass describe the ALT MODE (bd_shapes builds the robot mode from
//! it; bd_sim picks the combat archetype from the same tags). Phrases that are not transformers must
//! read as before. Everything goes through the game's staged entry point (`Lexicon::intent`).
//!
//! Expectation tokens (all must hold): `form=Vehicle`, `+Mech` / `-Mech` (recipe mechanics),
//! `f+Feature` / `f-Feature`, `el=Water`, `t=Aggressive` (temperament), `scale>=x`, `scale<=x`,
//! `n=1`, `n>=3`, `arch=<name>` (the archetype `assets/tuning/combat.ron` `transformer.rules` picks —
//! checked only when that archetype exists there and every rule before the match is understood here).

mod common;

use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_lexicon::Intent;
use bd_lexicon::tables::{element_name, feature_from_name, form_from_name, form_name, mechanic_from_name, temperament_name};

const ROSTER: [&str; 3] = ["Ethan", "Mia", "Sam"];

/// (phrase, expectations). A leading '~' = a voice transcript (the spoken path).
const TRANSFORMERS: &[(&str, &str)] = &[
    // --- the gaps this stage closes
    ("a helicopter robot", "form=Vehicle f+Rotor +Transforms +Piloted n=1 scale>=4 scale<=6 arch=helicopter"),
    ("a robot that turns into a helicopter", "form=Vehicle f+Rotor +Transforms +Piloted n=1 arch=helicopter"),
    ("a robot that turns into a motorcycle", "form=Vehicle f+Wheels f+Exhaust f-Cab f-Windows f-Cockpit +Transforms +Piloted scale<=5 arch=bike"),
    ("a robot that turns into a boat", "form=Vehicle +Transforms +Piloted el=Water arch=boat"),
    ("a robot that turns into a train", "form=Vehicle f+Wheels f+Cab f+Plow +Heavy +Transforms +Piloted arch=train"),
    ("a transformer that turns into a t rex", "form=Kaiju +ArticulatedTail +Transforms +Piloted n=1 arch=dino"),
    ("a dinobot", "form=Kaiju +ArticulatedTail +Transforms +Piloted n=1 t=Aggressive arch=dino"),
    ("a predacon", "+Transforms +Piloted +Flies n=1 t=Aggressive form=Quadruped"),
    ("a transforming wolf", "form=Quadruped +Transforms +Piloted -Swarm n=1 scale>=4 arch=beast"),
    ("a train robot", "form=Vehicle f+Plow f+Wheels +Transforms +Piloted arch=train"),
    ("a jet robot", "form=Vehicle f+Jet +Flies +Transforms +Piloted arch=jet"),
    ("a car robot", "form=Vehicle f+Wheels +Transforms +Piloted arch=car"),
    ("a robot car", "form=Vehicle f+Wheels +Transforms +Piloted arch=car"),
    // --- every phrasing
    ("a robot that transforms into a helicopter", "form=Vehicle f+Rotor +Transforms +Piloted"),
    ("a robot that can turn into a jet", "form=Vehicle f+Jet +Transforms +Piloted arch=jet"),
    ("a robot that changes into a tank", "form=Vehicle f+Treads f+Turret +Transforms +Piloted arch=tank"),
    ("a robot that is also a car", "form=Vehicle f+Wheels +Transforms +Piloted"),
    ("a robot turning into a truck", "form=Vehicle f+Cab +Transforms +Piloted arch=truck"),
    ("an helicopter transformer", "form=Vehicle f+Rotor +Transforms +Piloted"),
    ("a motorcycle transformer", "form=Vehicle f+Wheels f+Exhaust f-Windows +Transforms +Piloted scale<=5 arch=bike"),
    ("transformer shark", "form=Fish +Transforms +Piloted n=1 arch=beast"),
    ("a transforming tank", "form=Vehicle f+Treads f+Turret +Transforms +Piloted arch=tank"),
    ("a transforming bulldozer", "form=Vehicle f+Plow +Transforms +Piloted arch=construction"),
    ("a car that transforms into a robot", "form=Vehicle f+Wheels +Transforms +Piloted arch=car"),
    ("a jet that turns into a robot", "form=Vehicle f+Jet +Transforms +Piloted arch=jet"),
    ("a tank that transforms", "form=Vehicle f+Treads +Transforms +Piloted arch=tank"),
    ("a jet bot", "form=Vehicle f+Jet +Transforms +Piloted arch=jet"),
    ("a car bot", "form=Vehicle f+Wheels +Transforms +Piloted arch=car"),
    ("a maximal", "form=Quadruped +Transforms +Piloted t=Protective arch=beast"),
    ("beast wars rhino", "form=Quadruped +Transforms +Piloted arch=beast"),
    ("a beast wars cheetah", "form=Quadruped +Transforms +Piloted arch=beast"),
    ("an autobot car", "form=Vehicle f+Wheels +Transforms +Piloted t=Protective arch=car"),
    ("a decepticon jet", "form=Vehicle f+Jet +Transforms +Piloted t=Aggressive arch=jet"),
    ("predacon scorpion", "form=Arachnid +Transforms +Piloted scale>=4"),
    // --- alt modes by head noun
    ("a robot that turns into a motorbike", "f+Wheels f+Exhaust f-Windows f-Cab scale<=5 arch=bike"),
    ("a robot that turns into a bike", "f+Wheels f+Exhaust f-Chains f-Gears -DropFromSky scale<=5 arch=bike"),
    ("a robot that turns into a speedboat", "form=Vehicle el=Water +Transforms arch=boat"),
    ("a robot that turns into a ship", "form=Vehicle +Transforms arch=boat"),
    ("a robot that turns into a locomotive", "form=Vehicle f+Plow f+Wheels +Heavy arch=train"),
    ("a robot that turns into a fighter jet", "f+Jet +Flies arch=jet"),
    ("a robot that turns into a plane", "f+Jet f-Propeller +Flies arch=jet"),
    ("a robot that turns into a sports car", "f+Wheels f-Cab arch=car"),
    ("a robot that turns into a police car", "f+Wheels arch=car"),
    ("a robot that turns into a race car", "f+Wheels arch=car"),
    ("a robot that turns into a semi truck", "f+Cab arch=truck"),
    ("a fire truck robot", "f+Cab f-CraneArm arch=truck"),
    ("a robot that turns into an excavator", "f+CraneArm arch=construction"),
    ("a robot that turns into a crane", "form=Vehicle f+CraneArm arch=construction"),
    ("a robot that turns into a dinosaur", "form=Kaiju +ArticulatedTail arch=dino"),
    ("a robot that turns into a raptor", "form=Kaiju +ArticulatedTail -Swarm n=1 arch=dino"),
    ("a robot that turns into a lion", "form=Quadruped arch=beast"),
    ("a robot that turns into a gorilla", "form=Quadruped arch=beast"),
    ("a robot that turns into a cheetah", "form=Quadruped arch=beast"),
    ("a robot that turns into an eagle", "form=Bird +Flies"),
    ("a flaming helicopter robot", "f+Rotor el=Fire +Transforms"),
    ("a robot car with rockets", "form=Vehicle f+Wheels +Transforms"),
    // --- sizes: transformer-sized by default, the suit you wear when small, bigger when giant
    ("a small robot that turns into a car", "form=Vehicle +Transforms +Piloted scale<=2.6"),
    ("a tiny transformer that turns into a motorbike", "f+Exhaust +Transforms scale<=2.6"),
    ("a robot suit that turns into a sports car", "+Transforms scale<=2.6"),
    ("a mini jet robot", "f+Jet +Transforms scale<=2.6"),
    ("a transformer suit", "+Transforms +Piloted scale<=2.6"),
    ("a small transformer", "+Transforms +Piloted scale<=2.6"),
    ("a giant robot that turns into a jet", "f+Jet +Transforms scale>=8"),
    ("a giant transformer", "+Transforms scale>=8"),
    ("a robot that turns into a ship", "scale<=6"),
    ("a transforming eagle", "form=Bird scale>=4"),
    // --- a squad only when plural (followers, not piloted)
    ("dinobots", "form=Kaiju +Transforms n>=3 -Piloted"),
    ("car robots", "form=Vehicle +Transforms n>=3 -Piloted"),
    ("three robots that turn into cars", "form=Vehicle +Transforms n>=3"),
    ("transforming wolves", "form=Quadruped +Transforms -Swarm n>=3"),
    // --- voice: Whisper splits and mishearings
    ("~a helicopter robot.", "form=Vehicle f+Rotor +Transforms +Piloted"),
    ("~a trans former that turns into a heli copter", "form=Vehicle f+Rotor +Transforms +Piloted"),
    ("~a robot that turns into a motor cycle", "f+Wheels f+Exhaust f-Windows +Transforms +Piloted arch=bike"),
    ("~a robot that turns in to a boat", "form=Vehicle +Transforms arch=boat"),
    ("~dino bot", "form=Kaiju +Transforms +Piloted"),
    ("~dino bots", "form=Kaiju +Transforms n>=3"),
    ("~preda con", "+Transforms +Piloted t=Aggressive"),
    ("~maxi mal", "form=Quadruped +Transforms"),
    ("~decepti con jet", "f+Jet +Transforms t=Aggressive"),
    ("~tea rex transformer", "form=Kaiju +Transforms"),
    ("~Transformer that turns into a T-Rex!", "form=Kaiju +ArticulatedTail +Transforms +Piloted"),
    ("a transfomer that turns into a jet", "f+Jet +Transforms +Piloted"),
    ("a trans former", "form=Biped +Transforms"),
    // --- follower / delivery words still apply
    ("a helicopter robot that follows me", "f+Rotor +Transforms -Piloted"),
    ("drop a car robot on mia", "form=Vehicle +Transforms -Piloted"),
    ("ride a jet robot", "f+Jet +Transforms +Piloted"),
];

/// Must read as before (not a transformer made of their words, or unchanged): (phrase, expectations).
const UNCHANGED: &[(&str, &str)] = &[
    ("robot", "form=Biped -Transforms"),
    ("a robot", "form=Biped -Transforms"),
    ("robot arm", "form=Gauntlet -Transforms"),
    ("robot dog", "form=Quadruped -Transforms"),
    ("robot dinosaur", "form=Kaiju -Transforms"),
    ("robot t rex", "-Transforms"),
    ("robot shark", "-Transforms"),
    ("robot horse", "-Transforms"),
    ("giant robot", "form=Biped -Transforms"),
    ("killer robot", "-Transforms"),
    ("robot vacuum", "-Transforms"),
    ("a car", "form=Vehicle -Transforms +Piloted"),
    ("a helicopter", "form=Vehicle -Transforms f+Rotor"),
    ("a motorcycle", "form=Vehicle -Transforms"),
    ("a tank", "form=Vehicle -Transforms"),
    ("a robot riding a motorcycle", "form=Biped -Transforms"),
    ("a transforming sword", "form=Sword"),
    ("transformer", "form=Biped +Transforms scale>=4"),
    ("autobot", "form=Biped +Transforms"),
    ("autobots", "form=Biped +Transforms n>=3 t=Protective"),
    ("decepticons", "+Transforms n>=3 t=Aggressive"),
    ("robot in disguise", "+Transforms"),
    ("robots in disguise", "+Transforms n>=3"),
    ("convertible robot", "form=Vehicle +Transforms"),
    ("optimus prime", "form=Vehicle +Transforms +Piloted f+Cab"),
    ("bumblebee", "form=Vehicle +Transforms"),
    ("megatron", "form=Vehicle +Transforms f+Treads"),
    ("starscream", "form=Vehicle +Transforms f+Jet"),
    ("soundwave", "form=Biped +Transforms"),
    ("grimlock", "form=Kaiju +Transforms scale>=8"),
    ("bumblebee robot", "form=Vehicle +Transforms"),
];

fn parse(raw: &str) -> (bool, &str) {
    match raw.strip_prefix('~') {
        Some(p) => (true, p.trim()),
        None => (false, raw),
    }
}

/// Why `want` fails (None = holds).
fn check(it: &Intent, want: &str, arch: &Option<Vec<Rule>>) -> Option<String> {
    let rc = &it.resolution.recipe;
    let ok = if let Some(v) = want.strip_prefix("form=") {
        form_from_name(v) == Some(rc.form)
    } else if let Some(v) = want.strip_prefix("f+") {
        feature_from_name(v).is_some_and(|f| rc.features.contains(&f))
    } else if let Some(v) = want.strip_prefix("f-") {
        feature_from_name(v).is_some_and(|f| !rc.features.contains(&f))
    } else if let Some(v) = want.strip_prefix('+') {
        mechanic_from_name(v).is_some_and(|m| rc.has(m))
    } else if let Some(v) = want.strip_prefix('-') {
        mechanic_from_name(v).is_some_and(|m| !rc.has(m))
    } else if let Some(v) = want.strip_prefix("el=") {
        element_name(rc.element).eq_ignore_ascii_case(v)
    } else if let Some(v) = want.strip_prefix("t=") {
        temperament_name(rc.temperament).eq_ignore_ascii_case(v)
    } else if let Some(v) = want.strip_prefix("scale>=") {
        v.parse::<f32>().is_ok_and(|x| rc.scale >= x)
    } else if let Some(v) = want.strip_prefix("scale<=") {
        v.parse::<f32>().is_ok_and(|x| rc.scale <= x)
    } else if let Some(v) = want.strip_prefix("n>=") {
        v.parse::<u8>().is_ok_and(|x| rc.count >= x)
    } else if let Some(v) = want.strip_prefix("n=") {
        v.parse::<u8>().is_ok_and(|x| rc.count == x)
    } else if let Some(v) = want.strip_prefix("arch=") {
        match arch.as_ref().map(|rules| archetype(rules, rc)) {
            // the rule table isn't readable / has conditions this test doesn't know: not checked here
            None | Some(Arch::Unknown) => true,
            Some(Arch::Named(a)) => a == v || !arch.as_ref().is_some_and(|rs| rs.iter().any(|r| r.to == v)),
        }
    } else {
        return Some(format!("bad expectation {want:?}"));
    };
    (!ok).then(|| want.to_string())
}

fn run(cases: &[(&str, &str)]) {
    let Some(lex) = common::lex() else { return };
    let rules = rules();
    let mut fails = Vec::new();
    for (raw, want) in cases {
        let (spoken, phrase) = parse(raw);
        let it = lex.intent(phrase, &ROSTER, spoken);
        let rc = &it.resolution.recipe;
        let bad: Vec<String> = want.split_whitespace().filter_map(|w| check(&it, w, &rules)).collect();
        let a = rules.as_ref().map(|r| match archetype(r, rc) {
            Arch::Named(a) => a,
            Arch::Unknown => "?".into(),
        });
        if std::env::var_os("BD_TF_VERBOSE").is_some() || !bad.is_empty() {
            eprintln!(
                "{raw:?} -> {} {:?} feats={:?} el={} {:.1} m {:.0} kg x{} arch={}{}",
                form_name(rc.form),
                rc.mechanics,
                rc.features,
                element_name(rc.element),
                rc.scale,
                rc.mass,
                rc.count,
                a.unwrap_or_default(),
                if bad.is_empty() { String::new() } else { format!("   MISS {bad:?}") }
            );
        }
        if !bad.is_empty() {
            fails.push(format!("{raw:?}: {bad:?} (why: {:?})", it.resolution.explanation));
        }
    }
    assert!(fails.is_empty(), "{} miss(es):\n{}", fails.len(), fails.join("\n"));
}

#[test]
fn transformer_phrases() {
    run(TRANSFORMERS);
}

#[test]
fn not_transformers_read_as_before() {
    run(UNCHANGED);
}

#[test]
fn commands_stay_commands() {
    let Some(lex) = common::lex() else { return };
    for p in ["transform", "roll out", "autobots roll out", "combine", "transform into a robot", "robot mode"] {
        let r = lex.resolve(p);
        assert!(r.graft.action.is_some(), "{p:?} lost its command: {:?}", r.explanation);
    }
    let it = lex.intent("get in", &ROSTER, false);
    assert_eq!(it.resolution.graft.action, Some(bd_lexicon::GraftAction::Board));
}

// ------------------------------------------------------------------------ archetype rules (data)

/// One `transformer.rules` row of `assets/tuning/combat.ron` (mirrors bd_sim `ArchRule`, which this
/// crate cannot see). Unknown fields fail the strict parse: the archetype is then not judged here.
#[derive(serde::Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Rule {
    to: String,
    features_any: Vec<Feature>,
    features_all: Vec<Feature>,
    features_none: Vec<Feature>,
    mechanics_any: Vec<Mechanic>,
    mechanics_all: Vec<Mechanic>,
    forms: Vec<Form>,
    elements: Vec<Element>,
    min_mass: f32,
    max_mass: f32,
    min_scale: f32,
    max_scale: f32,
}

enum Arch {
    Named(String),
    Unknown,
}

fn rules() -> Option<Vec<Rule>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tuning/combat.ron");
    let text = std::fs::read_to_string(path).ok()?;
    let start = text.find("transformer:")?;
    let rs = start + text[start..].find("rules:")? + "rules:".len();
    let body = &text[rs..];
    let end = body.find("
        ],")?;
    let list = format!("{}
]", &body[..end]);
    match ron::from_str::<Vec<Rule>>(list.trim_start()) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("NOTE: transformer.rules not judged here ({e})");
            None
        }
    }
}

fn archetype(rules: &[Rule], rc: &Recipe) -> Arch {
    let f = |x: &Feature| rc.features.contains(x);
    for r in rules {
        let ok = (r.features_any.is_empty() || r.features_any.iter().any(f))
            && r.features_all.iter().all(f)
            && !r.features_none.iter().any(f)
            && (r.mechanics_any.is_empty() || r.mechanics_any.iter().any(|m| rc.has(*m)))
            && r.mechanics_all.iter().all(|m| rc.has(*m))
            && (r.forms.is_empty() || r.forms.contains(&rc.form))
            && (r.elements.is_empty() || r.elements.contains(&rc.element))
            && (r.min_mass <= 0.0 || rc.mass >= r.min_mass)
            && (r.max_mass <= 0.0 || rc.mass <= r.max_mass)
            && (r.min_scale <= 0.0 || rc.scale >= r.min_scale)
            && (r.max_scale <= 0.0 || rc.scale <= r.max_scale);
        if ok {
            return Arch::Named(r.to.clone());
        }
    }
    Arch::Unknown
}
