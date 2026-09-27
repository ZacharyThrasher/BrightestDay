//! MOVIES & COMICS (assets/powers/movies_comics.ron, docs/toolbox/OPS.md): one behaviour assertion
//! (at least) per op this domain built — BlinkCarry, Afflict, DelayedHit, BounceBeam, Barrage,
//! Disarm — and golden phrase tests (phrase → which kit move / borrowed move fires, mishearings and
//! chains included).

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::GameEvent;
use bd_sim::powers::ops::OpEvent;
use bd_sim::powers::toolbox::{split_chain, MoveIndex, Reading};
use bd_sim::powers::{norm_words, MoveOp, PowerEvent, PowerTable};
use bd_sim::status::StatusKind;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);
const GROUND: f32 = 8.0;

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
    stand(&mut w, A, 0.0, 0.0);
    for k in 2..=n {
        stand(&mut w, PlayerId(k), 0.0, -30.0 - k as f32 * 4.0);
    }
    w
}

/// Stand `id` on the ground at (x, z), looking down -Z.
fn stand(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, GROUND, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
    p.damage_pct = 0.0;
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

fn pos(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().mv.pos
}

fn pct(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().damage_pct
}

// ------------------------------------------------------------------------------ ops

#[test]
fn blink_carry_takes_the_touched_enemy_up_and_drops_them() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -1.6);
    w.fire_op(A, &MoveOp::BlinkCarry { dist: 5.0, up: 16.0, grab: 2.6 }).unwrap();
    let (a, b) = (pos(&w, A), pos(&w, B));
    assert!(a.y > GROUND + 14.0, "you went up with them: {a}");
    assert!(b.y > GROUND + 14.0, "they came along: {b}");
    assert!((a - b).length() < 3.5, "set down beside you: {a} {b}");
    assert!(w.player(A).unwrap().status.active(StatusKind::Float), "you hang a beat");
    assert!(w.player(B).unwrap().mv.tumble > 0.0, "they fall helpless");
    // They fall; you hang (Float) — the gap opens.
    run(&mut w, 30);
    let (a2, b2) = (pos(&w, A), pos(&w, B));
    assert!(b2.y < b.y - 1.0, "they're dropping: {b2}");
    assert!(a2.y > b2.y, "you fall slower than they do");
}

#[test]
fn blink_carry_with_nobody_in_reach_is_a_plain_blink() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -12.0);
    w.fire_op(A, &MoveOp::BlinkCarry { dist: 6.0, up: 0.0, grab: 2.6 }).unwrap();
    assert!(pos(&w, A).z < -4.0, "you moved along the look: {}", pos(&w, A));
    assert!((pos(&w, B).z + 12.0).abs() < 0.01, "they stayed put");
}

#[test]
fn afflict_stuns_the_aimed_enemy_briefly_and_caps_it() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -10.0);
    let evs = {
        w.fire_op(A, &MoveOp::Afflict { range: 30.0, cone: 8.0, status: Some(StatusKind::Stun), secs: 9.0, damage: 6.0, knockback: 0.0, up: 0.0, element: Element::Light }).unwrap();
        run(&mut w, 1)
    };
    let _ = evs;
    assert!(w.player(B).unwrap().status.stunned(), "stunned");
    assert!(pct(&w, B) > 5.0, "and hurt");
    // Capped far below the 9 s asked for: never a freeze.
    run(&mut w, 190);
    assert!(!w.player(B).unwrap().status.stunned(), "the stun is micro (≤ 1.2 s)");
}

#[test]
fn afflict_lift_floats_them_off_their_feet() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -10.0);
    w.fire_op(A, &MoveOp::Afflict { range: 30.0, cone: 8.0, status: Some(StatusKind::Float), secs: 2.0, damage: 0.0, knockback: 0.0, up: 10.0, element: Element::Psychic }).unwrap();
    run(&mut w, 40);
    assert!(pos(&w, B).y > GROUND + 1.5, "levitated: {}", pos(&w, B));
    assert!(w.player(B).unwrap().status.active(StatusKind::Float));
}

#[test]
fn afflict_needs_a_target() {
    let mut w = arena(2);
    stand(&mut w, B, 20.0, 0.0);
    assert!(w.fire_op(A, &MoveOp::Afflict { range: 30.0, cone: 5.0, status: Some(StatusKind::Stun), secs: 1.0, damage: 5.0, knockback: 0.0, up: 0.0, element: Element::Light }).is_err());
}

#[test]
fn delayed_hit_lands_after_its_countdown_wherever_they_are() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -2.5);
    w.fire_op(A, &MoveOp::DelayedHit { range: 3.0, cone: 20.0, radius: 0.0, secs: 1.0, damage: 20.0, knockback: 10.0, up: 0.3, element: Element::Shadow }).unwrap();
    // They walk away (five steps…).
    stand(&mut w, B, 15.0, -10.0);
    let early = run(&mut w, 60);
    assert!(pct(&w, B) < 1.0, "nothing yet at 0.5 s");
    assert!(!ops(&early).iter().any(|o| matches!(o, OpEvent::Marked { burst: true, .. })));
    let late = run(&mut w, 90);
    assert!(pct(&w, B) >= 15.0, "it burst on them: {}", pct(&w, B));
    assert!(ops(&late).iter().any(|o| matches!(o, OpEvent::Marked { target, burst: true, .. } if *target == B)));
}

#[test]
fn delayed_hit_wave_marks_everyone_in_the_radius() {
    let mut w = arena(3);
    stand(&mut w, B, 6.0, 0.0);
    stand(&mut w, C, 0.0, -40.0);
    w.fire_op(A, &MoveOp::DelayedHit { range: 0.0, cone: 0.0, radius: 12.0, secs: 0.5, damage: 12.0, knockback: 6.0, up: 0.3, element: Element::Shadow }).unwrap();
    run(&mut w, 90);
    assert!(pct(&w, B) >= 10.0, "inside the wave");
    assert!(pct(&w, C) < 1.0, "outside it");
    assert!(pct(&w, A) < 1.0, "never yourself");
}

#[test]
fn bounce_beam_banks_off_the_floor_into_them() {
    // Look down at the floor a few metres ahead: the first leg never reaches them, the bounce does.
    let setup = |bounces: u8| {
        let mut w = arena(2);
        stand(&mut w, B, 0.0, -8.0);
        w.player_mut(A).unwrap().input.pitch = -0.35;
        assert!(w.player(A).unwrap().look().y < 0.0);
        w.fire_op(A, &MoveOp::BounceBeam { length: 30.0, bounces, radius: 0.5, damage: 10.0, knockback: 6.0, element: Element::Light }).unwrap();
        let evs = run(&mut w, 1);
        (w, evs)
    };
    let (w0, _) = setup(0);
    assert!(pct(&w0, B) < 1.0, "a straight beam into the floor misses");
    let (w1, evs) = setup(1);
    assert!(pct(&w1, B) >= 10.0, "the bank shot lands (and hits harder): {}", pct(&w1, B));
    let legs = ops(&evs).iter().filter(|o| matches!(o, OpEvent::Zap { .. })).count();
    assert!(legs >= 2, "one zap per leg: {legs}");
}

#[test]
fn barrage_strikes_over_time_and_ends() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -2.4);
    w.fire_op(A, &MoveOp::Barrage { hits: 5, interval: 0.1, range: 3.0, angle: 80.0, damage: 3.0, knockback: 1.0, finish: 0.0, advance: 0.0, up: 0.0, blink: false, element: Element::Light }).unwrap();
    let first = pct(&w, B);
    assert!(first > 2.0 && first < 6.0, "one strike lands at once: {first}");
    run(&mut w, 70);
    assert!(pct(&w, B) > 12.0, "the rest follow: {}", pct(&w, B));
    assert!(!w.op_busy(A), "and it ends");
}

#[test]
fn blink_barrage_teleports_around_the_target() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -4.0);
    w.fire_op(A, &MoveOp::Barrage { hits: 5, interval: 0.12, range: 3.0, angle: 70.0, damage: 3.0, knockback: 1.0, finish: 0.0, advance: 0.0, up: 0.0, blink: true, element: Element::Shadow }).unwrap();
    assert!(w.op_busy(A), "a flurry is an active phase (chains wait on it)");
    let evs = run(&mut w, 80);
    let hops = evs.iter().filter(|e| matches!(e, GameEvent::Power(PowerEvent::Blinked { player, .. }) if *player == A)).count();
    assert!(hops >= 3, "vanish-reappear between strikes: {hops}");
    assert!(pct(&w, B) > 8.0, "and they land: {}", pct(&w, B));
}

#[test]
fn disarm_knocks_the_weapon_out_of_their_hand() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -10.0);
    let eye = w.player(B).unwrap().eye(w.params());
    let cid = w.cast(B, recipe("sword", Form::Sword, &[], Element::Light, 1.0), None, Aim::new(eye, Vec3::Z)).unwrap();
    run(&mut w, 30);
    assert!(w.constructs.held_by(B).is_some(), "armed");
    w.fire_op(A, &MoveOp::Disarm { range: 30.0, cone: 8.0, pull: true, knockback: 8.0 }).unwrap();
    run(&mut w, 30);
    assert!(w.constructs.get(cid).is_none_or(|c| c.gone.is_some()) || w.constructs.held_by(B).is_none(), "disarmed");
}

// ------------------------------------------------------------------------------ phrases

/// (kit id, move name) a phrase fires for a powerless player.
fn reads(text: &str) -> (String, String, &'static str) {
    let t = PowerTable::get();
    let r = MoveIndex::shipped().read(t, None, text);
    let how = match r {
        Reading::Kit(_) => "kit",
        Reading::Borrow(_) => "borrow",
        Reading::Held(_) => "held",
        Reading::Plain => "plain",
    };
    let Some(m) = r.move_ref() else { return (String::new(), String::new(), how) };
    let (def, mv) = m.resolve(t).expect("resolves");
    (def.id.clone(), norm_words(&mv.name), how)
}

#[track_caller]
fn fires(text: &str, id: &str, mv: &str) {
    let (k, m, how) = reads(text);
    assert_eq!((k.as_str(), m.as_str()), (id, mv), "{text:?} read as {how}");
}

#[test]
fn nightcrawler_is_perfect() {
    fires("nightcrawler teleport", "kit_nightcrawler", "teleport");
    fires("Nightcrawler's teleport", "kit_nightcrawler", "teleport");
    fires("night crawler teleport", "kit_nightcrawler", "teleport");
    fires("bamf", "kit_nightcrawler", "teleport");
    fires("BAMF!", "kit_nightcrawler", "teleport");
    fires("bamf drop", "kit_nightcrawler", "bamf drop");
    fires("nightcrawler bamf drop", "kit_nightcrawler", "bamf drop");
    fires("nightcrawler ultimate", "kit_nightcrawler", "sky drop");
    fires("nightcrawler combo", "kit_nightcrawler", "bamf drop");
    fires("bamf bamf bamf", "kit_nightcrawler", "bamf flurry");
    fires("nightcrawler tail whip", "kit_nightcrawler", "tail whip");
    // The bare name is never a move (it stays the costume summon).
    assert_eq!(reads("nightcrawler").2, "plain");
}

#[test]
fn owners_example_chain_splits_and_reads() {
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let is_move = |s: &str| ix.read(t, None, s).is_move();
    let steps = split_chain("nightcrawler teleport -> hulk smash -> thor bifrost -> expelliarmus", &is_move);
    assert_eq!(steps.len(), 4, "{steps:?}");
    assert!(steps.iter().all(|s| is_move(s)), "{steps:?}");
    let steps = split_chain("bamf, bamf drop then force push", &is_move);
    assert_eq!(steps.len(), 3, "{steps:?}");
}

#[test]
fn famous_moves_do_the_famous_thing() {
    // X-Men
    fires("wolverine ult", "kit_wolverine", "berserker barrage");
    fires("berserker barrage", "kit_wolverine", "berserker barrage");
    fires("healing factor", "kit_wolverine", "healing factor");
    fires("trick shot", "kit_cyclops", "trick shot");
    fires("cyclops optic blast", "kit_cyclops", "optic blast");
    fires("fastball special", "kit_colossus", "fastball special");
    fires("the juggernaut charge", "kit_juggernaut", "unstoppable charge");
    fires("unstoppable charge", "kit_juggernaut", "unstoppable charge");
    fires("quicksilver ultimate", "kit_quicksilver", "slow motion");
    fires("maximum effort", "kit_deadpool", "maximum effort");
    // Avengers
    fires("hulk smash", "kit_hulk", "smash");
    fires("thunderclap", "kit_hulk", "thunderclap");
    fires("puny god", "kit_hulk", "puny god");
    fires("thor's bifrost", "kit_thor", "bifrost");
    // SHOUT HYGIENE: bare "mjolnir" summons the curated Mjolnir construct; Thor's move is "thor mjolnir"
    fires("thor mjolnir", "kit_thor", "mjolnir");
    fires("spidey sense", "kit_spider_man", "spider sense");
    fires("web zip", "kit_spider_man", "web zip");
    fires("eye of agamotto", "kit_doctor_strange", "eye of agamotto");
    fires("crimson bands of cyttorak", "kit_doctor_strange", "crimson bands");
    fires("the snap", "kit_thanos", "snap");
    fires("thanos snap", "kit_thanos", "snap");
    fires("i am iron man", "kit_iron_man", "i am iron man");
    fires("unibeam", "kit_iron_man", "unibeam");
    fires("avengers assemble", "kit_captain_america", "avengers assemble");
    fires("venom strike", "kit_miles_morales", "venom strike");
    fires("penance stare", "kit_ghost_rider", "penance stare");
    fires("no more", "kit_scarlet_witch", "no more");
    // DC
    fires("infinite mass punch", "kit_flash", "infinite mass punch");
    fires("heat vision", "kit_superman", "heat vision");
    fires("lasso of truth", "kit_wonder_woman", "lasso of truth");
    fires("omega beams", "kit_darkseid", "omega beams");
    fires("batman counter", "kit_batman", "counter");
    // Star Wars
    fires("force push", "kit_jedi", "force push");
    fires("forced push", "kit_jedi", "force push");
    fires("force choke", "kit_sith", "force choke");
    fires("force lightning", "kit_sith", "force lightning");
    fires("jedi mind trick", "kit_jedi", "jedi mind trick");
    fires("saber throw", "kit_jedi", "saber throw");
    // Harry Potter, with mishearings
    fires("expelliarmus", "kit_harry_potter", "expelliarmus");
    fires("expel yarmus", "kit_harry_potter", "expelliarmus");
    fires("wingardium leviosa", "kit_harry_potter", "wingardium leviosa");
    fires("win guardian leviosa", "kit_harry_potter", "wingardium leviosa");
    fires("petrificus totalus", "kit_harry_potter", "petrificus totalus");
    fires("stupify", "kit_harry_potter", "stupefy");
    fires("accio", "kit_harry_potter", "accio");
    fires("expecto patronum", "kit_harry_potter", "expecto patronum");
    fires("avada kedavra", "kit_voldemort", "avada kedavra");
    fires("avada kadavra", "kit_voldemort", "avada kedavra");
    fires("bombarda maxima", "kit_hogwarts_duel", "bombarda");
    fires("side along apparition", "kit_hogwarts_duel", "side along");
    // Films
    fires("you shall not pass", "kit_gandalf", "you shall not pass");
    fires("eye of sauron", "kit_sauron", "eye of sauron");
    fires("neo bullet time", "kit_neo", "bullet time");
    fires("i know kung fu", "kit_neo", "i know kung fu");
    fires("cross the streams", "kit_ghostbusters", "cross the streams");
    fires("neuralyzer", "kit_men_in_black", "neuralyzer");
    fires("noisy cricket", "kit_men_in_black", "noisy cricket");
    fires("five point palm exploding heart technique", "kit_the_bride", "five point palm");
    fires("wuxi finger hold", "kit_po", "wuxi finger hold");
    fires("skadoosh", "kit_po", "wuxi finger hold");
    fires("crane kick", "kit_karate_kid", "crane kick");
    fires("wax on wax off", "kit_karate_kid", "wax on wax off");
    fires("hasta la vista baby", "kit_terminator", "hasta la vista");
    fires("88 miles per hour", "kit_back_to_the_future", "88 miles per hour");
    fires("atomic breath", "kit_godzilla", "atomic breath");
    fires("ice palace", "kit_elsa", "ice palace");
    fires("moana ocean wave", "kit_moana", "ocean wave");
    fires("bruno's vision", "kit_encanto", "bruno's vision");
    fires("jack-jack attack", "kit_incredibles", "jack jack attack");
}

#[test]
fn borrowed_bamf_drop_really_lifts_the_touched_enemy() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -1.6);
    let m = MoveIndex::shipped().shout("bamf drop").expect("bamf drop is a shout");
    let _ = w.borrow_move(A, m, false, None);
    run(&mut w, 2);
    assert!(pos(&w, B).y > GROUND + 10.0, "taken up: {}", pos(&w, B));
}

#[test]
fn every_kit_move_names_an_op_and_every_kit_has_shouts() {
    let t = PowerTable::get();
    let kits: Vec<_> = t.powers.iter().filter(|p| p.id.starts_with("kit_") && p.kind.is_kit()).collect();
    let mine = t.powers.iter().filter(|p| p.kind.is_kit()).count();
    assert!(mine >= 80, "{mine} kits");
    for k in kits {
        assert!(!k.moves.is_empty() && k.moves.len() <= 8, "{}", k.id);
        assert!(k.moves.iter().all(|m| !m.ops.is_empty()), "{}", k.id);
    }
}
