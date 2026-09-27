//! THE TOOLBOX (docs/toolbox/): kits, borrowed moves, chains, the input buffer, cooldowns, spark
//! pricing, staleness, determinism and the wire round-trip.

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3, TICK_HZ};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::{self, FireHow, ToolStep, ToolboxSnap};
use bd_sim::powers::toolbox::{split_chain, KitMatch, MoveIndex, MoveRef, Reading};
use bd_sim::powers::{norm_words, MoveTag, PlayerPower, PowerError, PowerEvent, PowerPhrase, PowerTable};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

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
    }
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, -4.0);
    w
}

fn stand_at(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, 8.0, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn fired(evs: &[GameEvent]) -> Vec<(PlayerId, MoveRef, FireHow, u8)> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::Power(PowerEvent::Fired { player, mv, how, step, .. }) => Some((*player, *mv, *how, *step)),
            _ => None,
        })
        .collect()
}

/// The shipped move a shout names.
fn shout(s: &str) -> MoveRef {
    MoveIndex::shipped().shout(&norm_words(s)).unwrap_or_else(|| panic!("no shout {s:?}"))
}

fn willpower(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().willpower
}

fn ticks(secs: f32) -> u32 {
    (secs * TICK_HZ as f32).ceil() as u32
}

/// Whole sparks a borrowed move costs (no repeats).
fn price(w: &World, m: MoveRef) -> f32 {
    let (_, mv) = m.resolve(PowerTable::get()).unwrap();
    chain::move_sparks(&w.cfg.tuning.willpower.sparks, mv, 0, w.cfg.rules.pool_segments()) as f32
}

/// Fired events of chain steps after the first (the first fires at issue time).
fn later(evs: &[GameEvent]) -> Vec<(PlayerId, MoveRef, FireHow, u8)> {
    fired(evs).into_iter().filter(|f| f.3 > 0).collect()
}

// ------------------------------------------------------------------------------ readings

/// A fixture table with two kits (the shipped data grows its own; these pin the grammar).
const KITS: &str = r#"[
    (id: "reinhardt", name: "Reinhardt", kind: Kit, names: ["reinhardt", "rein", "rein hart"],
     shouts: [("earthshatter", 3), ("hammer down", 3), ("charge", 1)],
     moves: [
        (name: "rocket hammer", tags: [Primary], ops: [Shove(range: 3.5, angle: 60.0, damage: 8.0, knockback: 12.0)]),
        (name: "charge", tags: [Movement], ops: [Impulse(fwd: 30.0, up: 1.0)]),
        (name: "barrier field", tags: [Defense], ops: [Field(strength: -30.0, radius: 4.0, secs: 2.0)]),
        (name: "earthshatter", tags: [Ultimate], cost: 30.0, ops: [Explode(radius: 8.0, damage: 14.0, knockback: 20.0)]),
     ]),
    (id: "doomfist", name: "Doomfist", kind: Kit, names: ["doomfist", "doom fist"],
     moves: [
        (name: "hand cannon", tags: [Primary], ops: [Shove(range: 6.0, angle: 20.0, damage: 6.0, knockback: 4.0)]),
        (name: "rocket punch", ops: [Impulse(fwd: 34.0, up: 0.5)]),
        (name: "seismic slam", ops: [Impulse(fwd: 8.0, up: 10.0), SlamOnLand(radius: 5.0, damage: 10.0, knockback: 14.0, carve: 1.0)]),
        (name: "meteor strike", cost: 40.0, ops: [Impulse(fwd: 0.0, up: 16.0), SlamOnLand(radius: 7.0, damage: 20.0, knockback: 24.0, carve: 2.0)]),
     ]),
    (id: "zap_fruit", name: "Zap Fruit", kind: Paramecia, names: ["zap fruit"],
     shouts: [("zap", 0), ("big zap", 1), ("zap fruit", 0)],
     moves: [
        (name: "zap", ops: [Shove(range: 3.0, angle: 30.0, damage: 4.0, knockback: 4.0, element: Electric)]),
        (name: "zap form", ops: [Transform]),
     ]),
]"#;

fn kit_table() -> (PowerTable, MoveIndex) {
    let t = PowerTable::parse(KITS).expect("fixture parses");
    let ix = MoveIndex::build(&t);
    (t, ix)
}

fn kit(t: &PowerTable, ix: &MoveIndex, phrase: &str) -> Option<(String, KitMatch)> {
    ix.kit_move(t, &norm_words(phrase)).map(|(m, why)| (m.resolve(t).unwrap().1.name.clone(), why))
}

#[test]
fn kit_phrases_find_the_characters_move() {
    let (t, ix) = kit_table();
    let name = |p: &str| kit(&t, &ix, p).map(|x| x.0);
    assert_eq!(name("reinhardt charge").as_deref(), Some("charge"));
    assert_eq!(name("Reinhardt's charge!").as_deref(), Some("charge"), "possessive");
    assert_eq!(name("reinhardts charge").as_deref(), Some("charge"), "possessive without the apostrophe");
    assert_eq!(name("rein hart ult").as_deref(), Some("earthshatter"), "misheard name + slot word");
    assert_eq!(name("reinhardt ultimate").as_deref(), Some("earthshatter"));
    assert_eq!(name("reinhardt earthshatter").as_deref(), Some("earthshatter"), "kit + its shout");
    assert_eq!(name("reinhardt block").as_deref(), Some("barrier field"), "tag Defense");
    assert_eq!(name("reinhardt attack").as_deref(), Some("rocket hammer"), "tag Primary");
    assert_eq!(name("reinhardt use his hammer").as_deref(), Some("rocket hammer"), "filler + a name word");
    // the character's own word beats the slot word: "punch" is Doomfist's ROCKET punch
    assert_eq!(name("doomfist punch").as_deref(), Some("rocket punch"));
    assert_eq!(name("doom fist slam").as_deref(), Some("seismic slam"));
    assert_eq!(name("doomfist ult").as_deref(), Some("meteor strike"), "untagged: the priciest move");
    assert_eq!(name("doomfist dash").as_deref(), Some("rocket punch"), "untagged Movement: the first dash");
    assert_eq!(name("doomfist meteor strke").as_deref(), Some("meteor strike"), "one typo");
    assert_eq!(kit(&t, &ix, "reinhardt ult").map(|x| x.1), Some(KitMatch::Tag(MoveTag::Ultimate)));
    // the bare name is the costume summon, and nonsense after a kit name is not a move
    assert_eq!(name("reinhardt"), None);
    assert_eq!(name("doomfist hello there"), None);
    // a kit name is never a grant name
    assert!(!ix.is_grant_name("reinhardt"));
    assert!(ix.is_grant_name("zap fruit"));
}

#[test]
fn readings_priority_held_then_kit_then_borrow_and_names_grant() {
    let (t, ix) = kit_table();
    // nobody holds anything: kit, borrow, plain
    assert!(matches!(ix.read(&t, None, "reinhardt charge"), Reading::Kit(_)));
    assert!(matches!(ix.read(&t, None, "big zap"), Reading::Borrow(_)) == false, "a Transform-only move is never borrowable");
    assert!(matches!(ix.read(&t, None, "zap"), Reading::Borrow(m) if m.slot == 0));
    // "zap fruit" is the power's NAME (and a shout): the name wins → the lexicon grants it
    assert_eq!(ix.read(&t, None, "zap fruit"), Reading::Plain);
    assert_eq!(ix.read(&t, None, "a big flaming hammer"), Reading::Plain);
    // a bare kit shout borrows the kit move
    assert!(matches!(ix.read(&t, None, "earthshatter"), Reading::Borrow(m) if m.slot == 3));
    // HELD priority (shipped table: a holder's shout is its own move, a non-holder borrows it)
    let gomu = PowerTable::get().index_of("gomu_gomu").unwrap();
    let mut holder = PlayerPower::default();
    holder.id = Some(gomu);
    let sx = MoveIndex::shipped();
    assert_eq!(sx.read(PowerTable::get(), Some(&holder), "gum gum pistol"), Reading::Held(PowerPhrase::Move(0)));
    assert!(matches!(sx.read(PowerTable::get(), None, "gum gum pistol"), Reading::Borrow(m) if m.power == gomu));
    // a tier shout of a power you hold but aren't in: borrowed at that tier
    assert!(matches!(sx.read(PowerTable::get(), Some(&holder), "jet pistol"), Reading::Borrow(m) if m.power == gomu && m.tier == 1));
    // names grant, even when nobody holds them
    assert_eq!(sx.read(PowerTable::get(), None, "star platinum"), Reading::Plain);
    assert_eq!(sx.read(PowerTable::get(), None, "super saiyan blue"), Reading::Plain);
}

#[test]
fn chains_split_on_every_separator_but_keep_plain_phrases_whole() {
    let (t, ix) = kit_table();
    let is_move = |s: &str| ix.read(&t, None, s).is_move();
    let split = |s: &str| split_chain(s, &is_move);
    assert_eq!(split("reinhardt charge -> doomfist punch"), vec!["reinhardt charge", "doomfist punch"]);
    assert_eq!(split("reinhardt charge > doomfist punch => zap"), vec!["reinhardt charge", "doomfist punch", "zap"]);
    assert_eq!(split("reinhardt charge then doomfist punch and then earthshatter"), vec!["reinhardt charge", "doomfist punch", "earthshatter"]);
    assert_eq!(split("reinhardt charge, doomfist punch"), vec!["reinhardt charge", "doomfist punch"], "comma (Whisper)");
    assert_eq!(split("Reinhardt charge. Then, doomfist punch."), vec!["Reinhardt charge", "doomfist punch"], "spoken punctuation");
    assert_eq!(split("reinhardt charge into earthshatter"), vec!["reinhardt charge", "earthshatter"]);
    assert_eq!(split("zap followed by doomfist ult"), vec!["zap", "doomfist ult"]);
    assert_eq!(split("summon a dragon, zap"), vec!["summon a dragon", "zap"], "a construct then a move");
    // weak separators never split plain phrases
    assert_eq!(split("a big, flaming hammer"), vec!["a big, flaming hammer"]);
    assert_eq!(split("turn it into a dragon"), vec!["turn it into a dragon"]);
    assert_eq!(split("rock and roll"), vec!["rock and roll"]);
    // strong separators always split
    assert_eq!(split("a hammer then a shield"), vec!["a hammer", "a shield"]);
    assert_eq!(split("a -> b -> c -> d -> e -> f -> g -> h -> i -> j").len(), 8, "capped");
}

#[test]
fn move_refs_pack_and_every_shipped_shout_resolves() {
    for m in [MoveRef::new(0, 0, 0), MoveRef::new(3999, 15, 7), MoveRef::new(257, 3, 5)] {
        assert_eq!(MoveRef::unpack(m.pack()), m);
    }
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let mut n = 0;
    for (s, m) in ix.shouts() {
        let (_, mv) = m.resolve(t).unwrap_or_else(|| panic!("{s:?} → {m:?} resolves"));
        assert!(bd_sim::powers::toolbox::borrowable(mv), "{s:?}");
        n += 1;
    }
    assert!(n > 300, "{n} borrowable shouts");
}

// ------------------------------------------------------------------------------ borrowing

#[test]
fn a_borrowed_move_fires_without_granting_and_costs_sparks_by_tier() {
    let mut w = duel();
    let push = shout("almighty push");
    let before = willpower(&w, A);
    w.borrow_move(A, push, false, None).expect("borrowed almighty push fires");
    let evs = run(&mut w, 1);
    let f = fired(&evs);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].2, FireHow::Borrowed);
    assert_eq!(w.player(A).unwrap().power.id, None, "borrowing never grants the power");
    // Explode r 9: a Titan-tier move → 5 sparks (default SparkTuning)
    let (_, mv) = push.resolve(PowerTable::get()).unwrap();
    let spark = w.cfg.rules.segment_size();
    let want = chain::move_sparks(&w.cfg.tuning.willpower.sparks, mv, 0, w.cfg.rules.pool_segments()) as f32 * spark;
    assert!((before - willpower(&w, A) - want).abs() < spark * 0.51 + 1.0, "paid {} of {want}", before - willpower(&w, A));
    // B got pushed
    assert!(w.player(B).unwrap().mv.vel.length() > 2.0 || w.player(B).unwrap().damage_pct > 0.0);
    // the per-move cooldown: again once the active phase is over (7 s cooldown) is refused
    run(&mut w, ticks(1.0));
    assert_eq!(w.borrow_move(A, push, false, None), Err(PowerError::Cooldown));
}

#[test]
fn borrowed_moves_never_transform_and_ignore_tier_ops() {
    let mut w = duel();
    // a Transform-only move of a held-only power: never in the index, and refused by the sim
    let t = PowerTable::get();
    let (pid, def) = t.powers.iter().enumerate().find(|(_, p)| p.moves.iter().any(|m| !bd_sim::powers::toolbox::borrowable(m))).unwrap();
    let slot = def.moves.iter().position(|m| !bd_sim::powers::toolbox::borrowable(m)).unwrap() as u8;
    assert_eq!(w.borrow_move(A, MoveRef::new(pid as u16, 0, slot), false, None), Err(PowerError::NoSuchMove));
    let evs = run(&mut w, 2);
    assert!(!evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Transformed { .. } | PowerEvent::Gained { .. } | PowerEvent::Ascend { .. }))));
}

#[test]
fn repeats_cost_more_so_cycling_is_cheaper() {
    let mut w = duel();
    let m = shout("bamf");
    let spark = w.cfg.rules.segment_size();
    let mut paid = Vec::new();
    for _ in 0..3 {
        w.player_mut(A).unwrap().willpower = 100.0;
        stand_at(&mut w, A, 0.0, 20.0);
        let before = willpower(&w, A);
        w.borrow_move(A, m, false, None).expect("bamf");
        paid.push(((before - willpower(&w, A)) / spark).round() as i32);
        // wait out the cooldown (but stay inside the staleness window)
        let cd = m.resolve(PowerTable::get()).unwrap().1.cooldown;
        run(&mut w, ticks(cd) + 4);
    }
    assert!(paid[1] == paid[0] + 1 && paid[2] == paid[0] + 2, "+1 spark per repeat: {paid:?}");
    // a different move is back at its base price (cycling is rewarded)
    let other = shout("almighty push");
    let (_, mv) = other.resolve(PowerTable::get()).unwrap();
    let base = chain::move_sparks(&w.cfg.tuning.willpower.sparks, mv, 0, w.cfg.rules.pool_segments()) as i32;
    w.player_mut(A).unwrap().willpower = 100.0;
    let before = willpower(&w, A);
    w.borrow_move(A, other, false, None).unwrap();
    assert_eq!(((before - willpower(&w, A)) / spark).round() as i32, base);
    // the view shows both (cooldown + repeats)
    let v = w.toolbox_view(A).expect("toolbox view");
    assert!(v.cds.iter().any(|c| c.0 == m.pack() + 1 && c.2 >= 3), "{v:?}");
}

#[test]
fn out_of_sparks_refuses_and_a_chain_stops_cleanly() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 0.0;
    assert_eq!(w.borrow_move(A, shout("almighty push"), false, None), Err(PowerError::NotEnoughWillpower));
    // chain: a cheap blink, then a pricey push we can't afford → ChainStopped(NotEnoughWillpower)
    let spark = w.cfg.rules.segment_size();
    stand_at(&mut w, A, 0.0, 20.0);
    w.player_mut(A).unwrap().willpower = spark * (price(&w, shout("bamf")) + 0.6);
    w.toolbox(A, vec![ToolStep::Move { mv: shout("bamf"), kit: false }, ToolStep::Move { mv: shout("almighty push"), kit: false }], None).unwrap();
    let evs = run(&mut w, ticks(0.5));
    assert_eq!(later(&evs).len(), 0, "only the (affordable) first step fired");
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::ChainStopped { player: A, why: PowerError::NotEnoughWillpower }))), "stopped");
    assert!(w.player(A).unwrap().power.toolbox.queue.is_empty());
}

// ------------------------------------------------------------------------------ chains

#[test]
fn a_chain_fires_each_step_when_the_previous_active_phase_ends() {
    let mut w = duel();
    stand_at(&mut w, A, 0.0, 20.0);
    let bamf = shout("bamf");
    let push = shout("almighty push");
    w.player_mut(A).unwrap().willpower = 100.0;
    w.toolbox(A, vec![ToolStep::Move { mv: bamf, kit: false }, ToolStep::Move { mv: push, kit: false }], None).unwrap();
    // step 0 fired at issue (between ticks); step 1 waits for the blink's active phase
    let (phase, _) = chain::active_phase(bamf.resolve(PowerTable::get()).unwrap().1);
    let mut at = None;
    for t in 1..=ticks(1.0) {
        let evs = w.tick(&[]).to_vec();
        if let Some(f) = later(&evs).first() {
            assert_eq!(f.1, push);
            assert_eq!(f.3, 1, "second step of the chain");
            at = Some(t);
            break;
        }
    }
    let at = at.expect("the second step fired");
    let want = ticks(phase);
    assert!(at >= want && at <= want + 2, "fired {at} ticks after the blink (active phase {phase} s = {want} ticks)");
}

#[test]
fn a_new_command_ko_or_stun_stops_the_chain() {
    let steps = || vec![ToolStep::Move { mv: shout("bamf"), kit: false }, ToolStep::Move { mv: shout("almighty push"), kit: false }, ToolStep::Move { mv: shout("universal pull"), kit: false }];
    // a new (non-toolbox) command
    let mut w = duel();
    stand_at(&mut w, A, 0.0, 20.0);
    w.toolbox(A, steps(), None).unwrap();
    w.toolbox_interrupt(A);
    let evs = run(&mut w, ticks(1.0));
    assert!(later(&evs).is_empty());
    // a stun
    let mut w = duel();
    stand_at(&mut w, A, 0.0, 20.0);
    w.toolbox(A, steps(), None).unwrap();
    w.player_mut(A).unwrap().status.apply(bd_sim::status::StatusKind::Stun, 1.0, None);
    let evs = run(&mut w, ticks(1.0));
    assert!(later(&evs).is_empty());
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::ChainStopped { player: A, why: PowerError::Busy }))));
}

#[test]
fn a_move_issued_during_an_active_phase_is_buffered_not_dropped() {
    let mut w = duel();
    stand_at(&mut w, A, 0.0, 20.0);
    let bamf = shout("bamf");
    let push = shout("almighty push");
    w.player_mut(A).unwrap().willpower = 100.0;
    w.borrow_move(A, bamf, false, None).unwrap();
    // one tick later (still inside the blink's active phase): queued, fires at the phase end
    run(&mut w, 1);
    assert!(w.borrow_move(A, push, false, None).is_ok());
    assert_eq!(w.player(A).unwrap().power.toolbox.queue.len(), 1, "buffered");
    let evs = run(&mut w, ticks(0.5));
    assert_eq!(fired(&evs).iter().map(|f| f.1).collect::<Vec<_>>(), vec![push]);
}

#[test]
fn held_moves_chain_with_borrowed_ones() {
    let mut w = duel();
    stand_at(&mut w, A, 0.0, 20.0);
    assert!(w.grant_power(A, "gomu_gomu"));
    run(&mut w, 1);
    w.player_mut(A).unwrap().willpower = 100.0;
    w.toolbox(A, vec![ToolStep::Move { mv: shout("bamf"), kit: false }, ToolStep::Held(0)], None).unwrap();
    let evs = run(&mut w, ticks(0.6));
    let f = fired(&evs);
    assert!(f.iter().any(|x| x.2 == FireHow::Held && x.3 == 1), "the held pistol fired as step 2: {f:?}");
}

#[test]
fn chains_are_deterministic() {
    let go = || {
        let mut w = duel();
        stand_at(&mut w, A, 0.0, 20.0);
        w.player_mut(A).unwrap().willpower = 100.0;
        w.toolbox(A, vec![ToolStep::Move { mv: shout("bamf"), kit: false }, ToolStep::Move { mv: shout("almighty push"), kit: false }, ToolStep::Move { mv: shout("universal pull"), kit: false }], None).unwrap();
        run(&mut w, ticks(2.0));
        w.state_hash()
    };
    assert_eq!(go(), go());
}

#[test]
fn toolbox_state_round_trips_on_the_wire() {
    let mut w = duel();
    stand_at(&mut w, A, 0.0, 20.0);
    w.toolbox(A, vec![ToolStep::Move { mv: shout("bamf"), kit: true }, ToolStep::Move { mv: shout("almighty push"), kit: false }], None).unwrap();
    let evs = w.tick(&[]).to_vec();
    let v = w.toolbox_view(A).expect("view");
    let cfg = bincode::config::standard();
    let bytes = bincode::serde::encode_to_vec(v, cfg).unwrap();
    let (back, _): (ToolboxSnap, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    assert_eq!(back, v);
    assert_eq!(v.pending, 1);
    for e in evs.iter().chain(run(&mut w, ticks(0.5)).iter()) {
        if let GameEvent::Power(p @ (PowerEvent::Fired { .. } | PowerEvent::ChainStopped { .. })) = e {
            let bytes = bincode::serde::encode_to_vec(e, cfg).unwrap();
            let (back, _): (GameEvent, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
            assert_eq!(&back, e, "{p:?}");
        }
    }
}

