//! MYTHOLOGY & FOLKLORE content (assets/powers/myth_folklore.ron): golden phrases — what a player
//! says (typed, spoken, misheard, in a chain) → which kit move fires — plus behaviour checks of the
//! headline moves through the real toolbox path (a kit move fired as a one-shot).

use bd_core::corps::Corps;
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::ToolStep;
use bd_sim::powers::ops::{HaulPhase, OpEvent, SeekPhase};
use bd_sim::powers::toolbox::{MoveIndex, MoveRef, Reading, split_chain};
use bd_sim::powers::{PowerEvent, PowerTable};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

/// (kit / power id, move name) a phrase fires for a powerless player, or `None` (not a move).
fn fires(phrase: &str) -> Option<(String, String)> {
    let t = PowerTable::get();
    let r = MoveIndex::shipped().read(t, None, phrase);
    let m = r.move_ref()?;
    let (def, mv) = m.resolve(t)?;
    Some((def.id.clone(), mv.name.clone()))
}

fn want(phrase: &str, id: &str, mv: &str) {
    assert_eq!(fires(phrase), Some((id.to_string(), mv.to_string())), "{phrase:?}");
}

fn kit_ref(phrase: &str) -> MoveRef {
    match MoveIndex::shipped().read(PowerTable::get(), None, phrase) {
        Reading::Kit(m) | Reading::Borrow(m) => m,
        r => panic!("{phrase:?} is not a move: {r:?}"),
    }
}

#[test]
fn the_file_parses_and_every_kit_is_reachable() {
    let t = PowerTable::get();
    let mine: Vec<_> = t.powers.iter().filter(|p| p.id.starts_with("myth_")).collect();
    assert!(mine.len() >= 100, "{} myth kits", mine.len());
    let moves: usize = mine.iter().map(|p| p.moves.len()).sum();
    assert!(moves >= 450, "{moves} myth kit moves");
    for p in &mine {
        assert!(p.kind.is_kit(), "{} is a kit", p.id);
        for m in &p.moves {
            assert!(!m.ops.is_empty(), "{}: {} has ops", p.id, m.name);
        }
    }
}

#[test]
fn god_and_move_names_fire_the_famous_move() {
    // Greek
    want("zeus thunderbolt", "myth_zeus", "thunderbolt");
    want("zeus ultimate", "myth_zeus", "wrath of zeus");
    want("wrath of zeus", "myth_zeus", "wrath of zeus");
    want("keraunos", "myth_zeus", "thunderbolt");
    want("poseidon's earthquake", "myth_poseidon", "earthquake");
    want("tsunami", "myth_poseidon", "tidal wave");
    want("poseidon tidal wave", "myth_poseidon", "tidal wave");
    want("hades helm of darkness", "myth_hades", "helm of darkness");
    want("medusa gaze", "myth_medusa", "petrifying gaze");
    want("petrifying gaze", "myth_medusa", "petrifying gaze");
    want("turn to stone", "myth_medusa", "petrifying gaze");
    want("hermes dash", "myth_hermes", "winged sandals");
    want("too close to the sun", "myth_icarus", "too close to the sun");
    want("siren song", "myth_sirens", "siren song");
    want("open pandora's box", "myth_pandora", "open the box");
    want("weight of the heavens", "myth_atlas", "weight of the heavens");
    want("hercules leap", "myth_heracles", "leap of heracles");
    // Norse
    want("gungnir", "myth_odin", "gungnir");
    want("odin's gungnir", "myth_odin", "gungnir");
    want("allfather gungnir", "myth_odin", "gungnir");
    want("odin ultimate", "myth_odin", "gungnir");
    want("huginn and muninn", "myth_odin", "huginn and muninn");
    want("ragnarok", "myth_surtr", "ragnarok");
    want("choose the slain", "myth_valkyrie", "choose the slain");
    want("hagalaz", "myth_runes", "hagalaz");
    want("rune tiwaz", "myth_runes_futhark", "tiwaz");
    // Egyptian / Hindu
    want("weighing of the heart", "myth_anubis", "weighing of the heart");
    want("apep devours the sun", "myth_apep", "devour the sun");
    want("sudarshan chakra", "myth_vishnu", "sudarshan chakra");
    want("vishnu ult", "myth_vishnu", "sudarshan chakra");
    want("hanuman leap", "myth_hanuman", "leap to lanka");
    want("leap to lanka", "myth_hanuman", "leap to lanka");
    want("hanuman grow", "myth_hanuman", "grow giant");
    want("lift the mountain", "myth_hanuman", "lift the mountain");
    want("shiva third eye", "myth_shiva", "third eye");
    want("tandava", "myth_shiva", "tandava");
    want("brahmastra", "myth_arjuna", "brahmastra");
    // Japanese / Chinese
    want("fujin wind bag", "myth_fujin", "wind bag");
    want("raijin drums", "myth_raijin", "thunder drums");
    want("the storm god susanoo kusanagi", "myth_susanoo", "kusanagi");
    want("foxfire", "myth_kitsune", "foxfire");
    want("ding", "myth_wukong", "ding");
    want("wukong somersault", "myth_wukong", "somersault");
    want("chang'e elixir of immortality", "myth_change", "elixir of immortality");
    want("nezha wind fire wheels", "myth_nezha", "wind fire wheels");
    want("shoot down the suns", "myth_hou_yi", "shoot down the suns");
    want("wave of the iron fan", "myth_iron_fan", "banana leaf fan");
    // Celtic / Arthurian / elsewhere
    want("gae bolg", "myth_cu_chulainn", "gae bolg");
    want("salmon leap", "myth_cu_chulainn", "salmon leap");
    want("fish up the islands", "myth_maui", "fish up the islands");
    want("lasso the sun", "myth_maui", "lasso the sun");
    want("sing into the swamp", "myth_vainamoinen", "sing into the swamp");
    want("three wishes", "myth_djinn", "three wishes");
    want("huff and puff", "myth_big_bad_wolf", "huff and puff");
    want("drink me", "myth_alice", "drink me");
    want("eat me", "myth_alice", "eat me");
    want("the piper's tune", "myth_pied_piper", "piper's tune");
    want("magic beans", "myth_beanstalk", "magic beans");
    want("deathless", "myth_koschei", "deathless");
}

#[test]
fn whisper_mishearings_and_native_names_still_fire() {
    want("gungner", "myth_odin", "gungnir");
    want("gun near", "myth_odin", "gungnir");
    want("gay bolg", "myth_cu_chulainn", "gae bolg");
    want("coo hullin salmon leap", "myth_cu_chulainn", "salmon leap");
    want("sudershan chakra", "myth_vishnu", "sudarshan chakra");
    want("bramastra", "myth_arjuna", "brahmastra");
    want("bajrangbali leap", "myth_hanuman", "leap to lanka");
    want("wotan gungnir", "myth_odin", "gungnir");
    want("neptune earthquake", "myth_poseidon", "earthquake");
    want("jupiter thunderbolt", "myth_zeus", "thunderbolt");
    want("your mungander venom", "myth_jormungandr", "venom");
    want("kaze bukuro", "myth_fujin", "wind bag");
    want("manaiakalani", "myth_maui", "fish up the islands");
}

#[test]
fn bare_god_names_stay_the_costume_summon() {
    for n in ["zeus", "odin", "hanuman", "anubis", "medusa", "fenrir", "quetzalcoatl"] {
        assert_eq!(MoveIndex::shipped().read(PowerTable::get(), None, n), Reading::Plain, "{n} alone is not a move");
    }
}

#[test]
fn myth_chains_split_into_their_moves() {
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let is_move = |s: &str| ix.read(t, None, s).is_move();
    let steps = split_chain("gungnir then leap to lanka then hanuman grow", &is_move);
    assert_eq!(steps, vec!["gungnir", "leap to lanka", "hanuman grow"]);
    let steps = split_chain("zeus gather the clouds -> zeus thunderbolt", &is_move);
    assert_eq!(steps.len(), 2);
    assert_eq!(fires(&steps[1]), Some(("myth_zeus".into(), "thunderbolt".into())));
    let steps = split_chain("fish up the islands then huff and puff", &is_move);
    assert_eq!(steps, vec!["fish up the islands", "huff and puff"]);
}

// ------------------------------------------------------------------------------ behaviour

fn duel() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=2u8 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
        p.willpower = 100.0;
    }
    stand(&mut w, A, 0.0, 0.0);
    stand(&mut w, B, 0.0, -14.0);
    w
}

fn stand(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, GROUND, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
    p.damage_pct = 0.0;
}

fn secs(s: f32) -> u32 {
    (s / DT).round() as u32
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn ops(evs: &[GameEvent]) -> Vec<OpEvent> {
    evs.iter().filter_map(|e| if let GameEvent::Power(PowerEvent::Op(o)) = e { Some(o.clone()) } else { None }).collect()
}

fn say(w: &mut World, phrase: &str) -> Vec<GameEvent> {
    let m = kit_ref(phrase);
    w.toolbox(A, vec![ToolStep::Move { mv: m, kit: true }], None).unwrap_or_else(|e| panic!("{phrase:?}: {e:?}"));
    let mut evs = w.tick(&[]).to_vec();
    evs.extend(run(w, 1));
    evs
}

#[test]
fn gungnir_is_thrown_strikes_and_flies_home() {
    let mut w = duel();
    let mut evs = say(&mut w, "odin gungnir");
    evs.extend(run(&mut w, secs(6.0)));
    let o = ops(&evs);
    assert!(o.iter().any(|e| matches!(e, OpEvent::Seek { phase: SeekPhase::Strike, target: Some(t), .. } if *t == B)), "{o:?}");
    assert!(o.iter().any(|e| matches!(e, OpEvent::Seek { phase: SeekPhase::Catch, .. })));
    assert!(w.player(B).unwrap().damage_pct > 10.0);
}

#[test]
fn maui_hauls_a_piece_of_the_land_to_his_feet() {
    let mut w = duel();
    stand(&mut w, B, 30.0, 30.0);
    w.player_mut(A).unwrap().input.pitch = -0.12;
    let mut evs = say(&mut w, "fish up the islands");
    evs.extend(run(&mut w, secs(2.5)));
    let o = ops(&evs);
    assert!(o.iter().any(|e| matches!(e, OpEvent::Haul { phase: HaulPhase::Tear, .. })), "{o:?}");
    assert!(o.iter().any(|e| matches!(e, OpEvent::Haul { phase: HaulPhase::Land, .. })));
}

#[test]
fn hanuman_leaps_across_the_map_and_lands_where_he_looks() {
    let mut w = duel();
    stand(&mut w, B, 30.0, 30.0);
    // Face +X (the flat lab's structures sit south) and aim at the ground ~22 m out.
    let eye = w.player(A).unwrap().eye(w.params());
    let d = (Vec3::new(22.0, GROUND, 0.0) - eye).normalize();
    let p = w.player_mut(A).unwrap();
    p.input.yaw = -std::f32::consts::FRAC_PI_2;
    p.input.pitch = d.y.asin();
    let evs = say(&mut w, "leap to lanka");
    let o = ops(&evs);
    let arc = o.iter().find_map(|e| if let OpEvent::Leap(a) = e { Some(*a) } else { None }).expect("a leap arc");
    assert!(arc.to.x > 18.0 && arc.to.z.abs() < 1.0, "a long leap where you look: {arc:?}");
    let mut landed = None;
    for _ in 0..secs(5.0) {
        w.tick(&[]);
        let p = w.player(A).unwrap();
        if p.mv.grounded && (p.mv.pos - arc.from).length() > 5.0 {
            landed = Some(p.mv.pos);
            break;
        }
    }
    let at = landed.expect("landed");
    assert!((Vec3::new(at.x - arc.to.x, 0.0, at.z - arc.to.z)).length() < 3.0, "landed at {at}, aimed {}", arc.to);
}

#[test]
fn pandoras_box_lets_one_evil_out() {
    let mut w = duel();
    let evs = say(&mut w, "open pandora's box");
    let o = ops(&evs);
    let picks: Vec<_> = o.iter().filter_map(|e| if let OpEvent::Roulette { pick, of, .. } = e { Some((*pick, *of)) } else { None }).collect();
    assert_eq!(picks.len(), 1, "exactly one evil: {o:?}");
    assert!(picks[0].0 < picks[0].1 && picks[0].1 == 6);
}

#[test]
fn the_tidal_wave_carries_them_off() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -7.0);
    let before = w.player(B).unwrap().mv.pos;
    say(&mut w, "tsunami");
    run(&mut w, secs(0.8));
    let after = w.player(B).unwrap().mv.pos;
    assert!(before.z - after.z > 5.0, "swept {before} -> {after}");
    assert!(w.player(B).unwrap().damage_pct > 3.0);
}

#[test]
fn medusa_stops_only_those_who_look_at_her() {
    // B looks at Medusa: stone for a moment.
    let mut w = duel();
    w.player_mut(B).unwrap().input.yaw = std::f32::consts::PI;
    say(&mut w, "medusa gaze");
    assert!(w.player(B).unwrap().status.stunned(), "met her eyes: stunned");
    // B looks away: nothing.
    let mut w = duel();
    w.player_mut(B).unwrap().input.yaw = 0.0;
    say(&mut w, "look into my eyes");
    assert!(!w.player(B).unwrap().status.stunned(), "looked away: free");
}
