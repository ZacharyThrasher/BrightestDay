//! TV & CARTOONS content (assets/powers/tv_cartoons.ron + base_cartoons.ron): golden phrases (what
//! a spoken / typed line fires), chains, mishearings, grant names, the `Strain` op, and a smoke run
//! of EVERY move in the file through the real toolbox path (no panic, invariants hold).

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3};
use bd_sim::powers::toolbox::{split_chain, MoveIndex, MoveRef, Reading};
use bd_sim::powers::{norm_words, MoveOp, PowerKind, PowerTable};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);

const FILE: &str = include_str!("../../../assets/powers/tv_cartoons.ron");

fn t() -> &'static PowerTable {
    PowerTable::get()
}

fn ix() -> &'static MoveIndex {
    MoveIndex::shipped()
}

/// (power id, move name) a phrase fires for a powerless player.
fn reads(phrase: &str) -> Option<(String, String, bool)> {
    let r = ix().read(t(), None, phrase);
    let kit = matches!(r, Reading::Kit(_));
    let m = r.move_ref()?;
    let (def, mv) = m.resolve(t())?;
    Some((def.id.clone(), mv.name.clone(), kit))
}

#[track_caller]
fn kit(phrase: &str, id: &str, mv: &str) {
    let got = reads(phrase);
    assert_eq!(got, Some((id.to_string(), mv.to_string(), true)), "{phrase:?} should fire kit move {id}/{mv}");
}

/// A shout said without the power: a borrowed one-shot (or, when the shout starts with a kit's
/// name, the same move as a kit phrase — either way THAT move fires).
#[track_caller]
fn borrow(phrase: &str, id: &str, mv: &str) {
    let got = reads(phrase);
    let kit_named = ix().kit_names().any(|(k, _)| norm_words(phrase).starts_with(&format!("{k} ")));
    assert_eq!(got, Some((id.to_string(), mv.to_string(), kit_named && got.as_ref().is_some_and(|g| g.2))), "{phrase:?} should borrow {id}/{mv}");
}

/// Fires this move either way (a kit phrase or a borrowed shout).
#[track_caller]
fn fires(phrase: &str, id: &str, mv: &str) {
    let got = reads(phrase).map(|(i, m, _)| (i, m));
    assert_eq!(got, Some((id.to_string(), mv.to_string())), "{phrase:?} should fire {id}/{mv}");
}

#[track_caller]
fn plain(phrase: &str) {
    assert_eq!(ix().read(t(), None, phrase), Reading::Plain, "{phrase:?} must stay the lexicon's (a grant / a summon)");
}

#[test]
fn file_parses_and_is_in_the_shipped_table() {
    let mine = PowerTable::parse(FILE).expect("tv_cartoons.ron parses");
    assert!(mine.powers.len() >= 60, "{} entries", mine.powers.len());
    for p in &mine.powers {
        assert!(t().index_of(&p.id).is_some(), "{} is shipped", p.id);
        assert!(!p.moves.is_empty(), "{} has moves", p.id);
        assert!(p.moves.len() <= 8, "{} has ≤ 8 moves", p.id);
        for m in &p.moves {
            assert!(!m.ops.is_empty(), "{}/{} has ops", p.id, m.name);
        }
    }
    // the lexicon reads it without complaint
    let (lex, warnings) = bd_lexicon::power_scan::parse_powers_named("powers/tv_cartoons.ron", FILE);
    assert_eq!(lex.len(), mine.powers.len());
    assert!(warnings.is_empty(), "lexicon warnings: {warnings:#?}");
}

#[test]
fn owner_examples_fire_the_characters_move() {
    // The owner's own example ends "... -> optimus prime combo".
    kit("optimus prime combo", "kit_optimus_prime", "combo");
    kit("Optimus Prime's combo!", "kit_optimus_prime", "combo");
    kit("optimist prime combo", "kit_optimus_prime", "combo");
    kit("optimus prime ultimate", "kit_optimus_prime", "matrix of leadership");
    kit("optimus prime axe slam", "kit_optimus_prime", "axe slam");
    kit("optimus prime heavy", "kit_optimus_prime", "heavy");
    borrow("energon axe", "kit_optimus_prime", "combo");
    borrow("matrix of leadership", "kit_optimus_prime", "matrix of leadership");
    kit("optimus prime dash", "kit_optimus_prime", "charge");
    kit("optimus prime block", "kit_optimus_prime", "shield");
    kit("megatron fusion cannon", "kit_megatron", "fusion cannon");
    kit("starscream null ray", "kit_starscream", "null ray");
    kit("starscream missiles", "kit_starscream", "missiles");
    kit("bumblebee stinger", "kit_bumblebee", "stinger");
    kit("grimlock t rex", "kit_grimlock", "t rex");
    borrow("laserbeak eject", "kit_soundwave", "laserbeak");
    borrow("ravage eject", "kit_soundwave", "ravage");
}

#[test]
fn avatar_bending_reads() {
    // the arts are techniques: saying them grants (the lexicon's), never a borrow
    for n in ["airbending", "waterbending", "earthbending", "firebending", "avatar state", "metalbending", "lavabending",
              "air bending", "earth bender", "fire bender"] {
        plain(n);
    }
    // their famous moves are borrowable one-shots
    borrow("air scooter", "airbending", "air scooter");
    borrow("rock wall", "earthbending", "earth wall");
    borrow("earthbending wall", "earthbending", "earth wall");
    borrow("rock spikes", "earthbending", "earth spikes");
    borrow("earthquake", "earthbending", "earthquake");
    borrow("water whip", "waterbending", "water whip");
    borrow("octopus form", "waterbending", "octopus form");
    borrow("lightning redirection", "firebending", "lightning redirection");
    borrow("jet propulsion", "firebending", "jet propulsion");
    borrow("metal cable", "metalbending", "metal cable");
    borrow("sparky sparky boom", "combustionbending", "sparky sparky boom");
    // the characters
    kit("toph seismic sense", "kit_toph", "seismic sense");
    kit("the blind bandit earth wall", "kit_toph", "earth wall");
    kit("toph ultimate", "kit_toph", "seismic sense");
    kit("azula lightning", "kit_azula", "lightning");
    kit("azula's blue fire", "kit_azula", "blue fire");
    kit("aang avatar state", "kit_aang", "avatar state");
    kit("aang glider", "kit_aang", "glider");
    kit("katara bloodbending", "kit_katara", "bloodbending");
    kit("zuko agni kai", "kit_zuko", "agni kai");
    kit("iroh lightning redirection", "kit_iroh", "lightning redirection");
    kit("uncle iroh ultimate", "kit_iroh", "dragon's breath");
    kit("korra avatar state", "kit_korra", "avatar state");
    kit("amon take your bending", "kit_amon", "take your bending");
    kit("sokka boomerang", "kit_sokka", "boomerang");
    borrow("yip yip", "kit_aang", "yip yip");
    borrow("bloodbending", "kit_katara", "bloodbending");
    borrow("dragon of the west", "kit_iroh", "dragon's breath");
    kit("optimus prime shoot", "kit_optimus_prime", "ion blaster");
}

#[test]
fn cartoon_kits_and_shouts() {
    kit("leonardo parry", "kit_leonardo", "parry");
    kit("raphael sai", "kit_raphael", "sai");
    kit("donatello bo staff", "kit_donatello", "bo staff");
    kit("michelangelo cowabunga", "kit_michelangelo", "cowabunga");
    kit("mikey nunchucks", "kit_michelangelo", "nunchucks");
    kit("four arms clap", "kit_four_arms", "clap");
    kit("heatblast supernova", "kit_heatblast", "supernova");
    kit("xlr8 blur", "kit_xlr8", "blur");
    kit("diamondhead reflect", "kit_diamondhead", "reflect");
    kit("eleven throw", "kit_eleven", "throw");
    kit("homelander laser eyes", "kit_homelander", "laser eyes");
    kit("a train run through", "kit_a_train", "run through");
    kit("raven soul self", "kit_raven", "soul self");
    kit("starfire starbolts", "kit_starfire", "starbolts");
    kit("cyborg sonic cannon", "kit_cyborg", "sonic cannon");
    kit("beast boy t rex", "kit_beast_boy", "t rex");
    kit("rick portal gun", "kit_rick", "portal gun");
    kit("pickle rick rat armor", "kit_pickle_rick", "rat armor");
    kit("the doctor sonic screwdriver", "kit_the_doctor", "sonic screwdriver");
    kit("doctor who tardis", "kit_the_doctor", "tardis");
    kit("jake stretchy punch", "kit_jake", "stretchy punch");
    kit("steven bubble", "kit_steven", "bubble");
    kit("danny phantom ghostly wail", "kit_danny_phantom", "ghostly wail");
    kit("samurai jack iaido", "kit_samurai_jack", "iaido");
    kit("spongebob karate chop", "kit_spongebob", "karate chop");
    kit("jinx fishbones", "kit_jinx", "fishbones");
    kit("ekko rewind", "kit_ekko", "rewind");
    kit("geralt yrden", "kit_geralt", "yrden");
    kit("the witcher dodge", "kit_geralt", "roll");
    kit("voltron blazing sword", "kit_voltron", "blazing sword");
    kit("invincible flying punch", "kit_invincible", "flying punch");
    kit("omni man think mark", "kit_omni_man", "think mark");
    borrow("azarath metrion zinthos", "kit_raven", "zinthos");
    borrow("exterminate", "kit_dalek", "exterminate");
    borrow("aard", "kit_geralt", "aard");
    borrow("igni", "kit_geralt", "igni");
    borrow("quen", "kit_geralt", "quen");
    borrow("axii", "kit_geralt", "axii");
    borrow("lucky charm", "kit_ladybug", "lucky charm");
    borrow("miraculous ladybug", "kit_ladybug", "miraculous ladybug");
    borrow("cataclysm", "kit_cat_noir", "cataclysm");
    borrow("meep meep", "kit_road_runner", "meep meep");
    borrow("spinjitzu", "kit_ninjago", "spinjitzu");
    borrow("by the power of grayskull", "kit_he_man", "power of grayskull");
    fires("thundercats ho", "kit_lion_o", "thundercats ho");
    fires("thunder thunder thundercats ho", "kit_lion_o", "thundercats ho");
    borrow("acme anvil", "kit_acme", "anvil");
    borrow("null ray", "kit_starscream", "null ray");
    borrow("take your bending", "kit_amon", "take your bending");
    borrow("think mark", "kit_omni_man", "think mark");
    borrow("seismic sense slam", "kit_toph", "seismic sense");
    borrow("stretchy punch", "kit_jake", "stretchy punch");
    borrow("ghostly wail", "kit_danny_phantom", "ghostly wail");
    borrow("chronobreak", "kit_ekko", "rewind");
    borrow("portable hole", "kit_acme", "portable hole");
    // base cartoons (deepened): turtle power calls the brothers, the Omnitrix roulette
    borrow("turtle power", "mutagen_turtle", "turtle power");
    borrow("ninja vanish", "mutagen_turtle", "ninja vanish");
    borrow("random alien", "omnitrix", "omnitrix roulette");
    borrow("viltrumite barrage", "viltrumite", "viltrumite barrage");
    // bare names stay the costume summon (never a move, never a grant of a kit)
    for n in ["optimus prime", "toph", "aang", "leonardo", "homelander", "eleven", "raven", "geralt", "spongebob", "megatron"] {
        plain(n);
    }
}

/// Every name of every kit in the file, said bare, stays the costume summon — even when a shorter
/// alias of the same character plus the rest of the words could read as a move ("optimus prime" is
/// never "optimus" + a move called "prime").
#[test]
fn bare_kit_names_never_fire_a_move() {
    let mine = PowerTable::parse(FILE).expect("parses");
    let mut bad = Vec::new();
    for p in mine.powers.iter().filter(|p| p.kind == PowerKind::Kit) {
        for n in p.names.iter().chain(std::iter::once(&p.name)) {
            if ix().read(t(), None, n) != Reading::Plain {
                bad.push(format!("{} {n:?} -> {:?}", p.id, reads(n)));
            }
        }
    }
    assert!(bad.is_empty(), "bare kit names that fire a move: {bad:#?}");
}

#[test]
fn chains_split_into_kit_steps() {
    let is_move = |s: &str| ix().read(t(), None, s).is_move();
    let steps = split_chain("toph earth wall -> azula lightning -> optimus prime combo", &is_move);
    assert_eq!(steps.len(), 3, "{steps:?}");
    let names: Vec<String> = steps.iter().map(|s| reads(s).map(|r| r.1).unwrap_or_default()).collect();
    assert_eq!(names, ["earth wall", "lightning", "combo"]);
    let steps = split_chain("air scooter then air slam, earth spikes", &is_move);
    assert_eq!(steps.len(), 3, "{steps:?}");
    let steps = split_chain("aard into yrden and then geralt whirl", &is_move);
    assert_eq!(steps.len(), 3, "{steps:?}");
}

// ------------------------------------------------------------------------------ the world side

fn arena(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=n {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    place(&mut w, A, 0.0, 0.0);
    place(&mut w, B, 0.0, -5.0);
    if n > 2 {
        place(&mut w, C, 3.0, -8.0);
    }
    w
}

fn place(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, 8.0, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
}

#[test]
fn strain_is_a_toll_on_yourself_only() {
    let mut w = arena(2);
    for _ in 0..30 {
        w.tick(&[]);
    }
    let (a0, b0) = (w.player(A).unwrap().damage_pct, w.player(B).unwrap().damage_pct);
    let vel0 = w.player(A).unwrap().mv.vel;
    w.fire_op(A, &MoveOp::Strain { pct: 7.0 }).expect("strain fires");
    let a = w.player(A).unwrap();
    assert!((a.damage_pct - a0 - 7.0).abs() < 1e-4, "{} -> {}", a0, a.damage_pct);
    assert_eq!(a.mv.vel, vel0, "no knockback");
    assert_eq!(w.player(B).unwrap().damage_pct, b0, "nobody else pays");
    // clamped: a data typo can't one-shot yourself
    w.fire_op(A, &MoveOp::Strain { pct: 5000.0 }).unwrap();
    assert!(w.player(A).unwrap().damage_pct <= a0 + 7.0 + bd_sim::powers::ops::STRAIN_MAX_PCT + 1e-3);
    w.fire_op(A, &MoveOp::Strain { pct: f32::NAN }).unwrap();
}

#[test]
fn eleven_pays_for_her_throw() {
    // Eleven's throw: the aimed enemy is hurled AND her nose bleeds (her own damage % rises).
    let mut w = arena(2);
    for _ in 0..30 {
        w.tick(&[]);
    }
    let r = reads("eleven throw").expect("reads");
    let pid = t().index_of("kit_eleven").unwrap();
    let slot = t().def(pid).unwrap().moves.iter().position(|m| m.name == r.1).unwrap() as u8;
    let a0 = w.player(A).unwrap().damage_pct;
    let b0 = w.player(B).unwrap().damage_pct;
    w.player_mut(A).unwrap().willpower = 100.0;
    w.borrow_move(A, MoveRef::new(pid, 0, slot), true, None).expect("eleven throw fires at the enemy in front");
    for _ in 0..20 {
        w.tick(&[]);
    }
    assert!(w.player(B).unwrap().damage_pct > b0, "the target is hurt");
    assert!(w.player(A).unwrap().damage_pct > a0, "the nosebleed: Eleven pays");
}

/// Every move in the file fires through the real toolbox path in a 3-player arena without a
/// panic (and the world invariants hold for 2 s after): a data smoke test for the whole domain.
#[test]
fn every_tv_cartoon_move_runs() {
    let mine = PowerTable::parse(FILE).expect("parses");
    let mut fired = 0usize;
    let mut total = 0usize;
    for p in &mine.powers {
        let pid = t().index_of(&p.id).unwrap();
        let def = t().def(pid).unwrap();
        let tiers = std::iter::once(0u8).chain((1..=def.tiers.len() as u8).filter(|k| def.tier(*k).is_some_and(|td| !td.moves.is_empty())));
        for tier in tiers {
            for (slot, mv) in def.moves_at(tier).iter().enumerate() {
                if !bd_sim::powers::toolbox::borrowable(mv) {
                    continue;
                }
                total += 1;
                let mut w = arena(3);
                for _ in 0..20 {
                    w.tick(&[]);
                }
                w.player_mut(A).unwrap().willpower = 100.0;
                let r = w.borrow_move(A, MoveRef::new(pid, tier, slot as u8), def.kind == PowerKind::Kit, None);
                if r.is_ok() {
                    fired += 1;
                }
                for _ in 0..(2 * 60) {
                    w.tick(&[]);
                }
                assert!(w.player(A).is_some(), "{}/{} ran", p.id, mv.name);
            }
        }
    }
    // most moves have something to act on in front of them (a few need a wall / an own construct)
    assert!(fired * 10 >= total * 7, "only {fired}/{total} moves fired");
}

#[test]
fn shouts_are_unique_in_the_file() {
    // every shout in the file is indexed to a move of this file (nobody else took it first)
    let mine = PowerTable::parse(FILE).expect("parses");
    let mut lost = Vec::new();
    for p in &mine.powers {
        let pid = t().index_of(&p.id).unwrap();
        for (s, _) in p.shouts.iter().chain(p.tiers.iter().flat_map(|td| td.shouts.iter())) {
            let n = norm_words(s);
            if ix().is_grant_name(&n) {
                continue;
            }
            match ix().shout(&n) {
                Some(m) if m.power == pid => {}
                other => lost.push(format!("{} {s:?} -> {other:?}", p.id)),
            }
        }
    }
    // Another file that sorts earlier may claim a shout later (first claim wins): that's reported by
    // the validator (`powers_check`) as a warning, not a failure here — the headline phrases above are
    // pinned one by one. Within this file every shout must be unique.
    for l in &lost {
        eprintln!("note: shout lost to an earlier file: {l}");
    }
    let mut seen = std::collections::HashSet::new();
    for p in &mine.powers {
        for (s, _) in p.shouts.iter().chain(p.tiers.iter().flat_map(|td| td.shouts.iter())) {
            assert!(seen.insert(norm_words(s)), "shout {s:?} appears twice in tv_cartoons.ron");
        }
    }
}
