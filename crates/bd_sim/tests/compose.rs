//! VERB GRAMMAR (docs/toolbox/COMPOSED_MOVE.md): the describe-a-power grammar (golden phrases,
//! typed + spoken), the spec (sanitising, cooldown keys, the wire), the expansion (every kind
//! builds a sane move), and the sim (determinism, validation, pricing + staleness, targets,
//! chains).

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::{self, FireHow, ToolStep};
use bd_sim::powers::compose::{self, ComposedMove, Names, Place, Target, VerbKind, VerbTable};
use bd_sim::powers::{MoveOp, PowerError, PowerEvent};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

const ROSTER: [(PlayerId, &str); 6] = [(PlayerId(1), "Ethan"), (PlayerId(2), "Mia"), (PlayerId(3), "Sam"), (PlayerId(4), "Nova"), (PlayerId(5), "Big Tony"), (PlayerId(6), "Alex_99")];

fn names() -> Names<'static> {
    Names::new(&ROSTER)
}

fn read(s: &str) -> Option<compose::Reading> {
    compose::read(s, &names())
}

fn spec(s: &str) -> ComposedMove {
    read(s).unwrap_or_else(|| panic!("{s:?} should read as a composed move")).spec
}

fn verb_of(s: &ComposedMove) -> String {
    VerbTable::get().verb(s.verb).map(|v| v.id.clone()).unwrap_or_default()
}

// ------------------------------------------------------------------------------------ table

#[test]
fn table_loads_clean() {
    let t = VerbTable::get();
    assert!(t.verbs.len() >= 40, "{} verbs", t.verbs.len());
    assert!(t.warnings.is_empty(), "{:?}", t.warnings);
    // every noun word is one of its verb's words
    for v in &t.verbs {
        for n in &v.noun_words {
            assert!(v.words.contains(n), "verb {:?}: noun word {n:?} is not one of its words", v.id);
        }
    }
    // style fx keys exist in the presentation table (assets/tuning/move_fx.ron)
    let fx = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/move_fx.ron")).unwrap_or_default();
    if !fx.is_empty() {
        for s in &t.styles {
            assert!(fx.contains(&format!("\"{}\":", s.fx)), "style {:?}: fx key {:?} not in move_fx.ron", s.id, s.fx);
        }
        for v in t.verbs.iter().filter(|v| !v.fx.is_empty()) {
            assert!(fx.contains(&format!("\"{}\":", v.fx)), "verb {:?}: fx key {:?} not in move_fx.ron", v.id, v.fx);
        }
    }
}

/// Every verb × a spread of places / targets / counts / durations / styles / modifiers expands to
/// a non-empty move with finite, bounded numbers, and a spark price.
#[test]
fn every_kind_builds_sane_moves() {
    let t = VerbTable::get();
    let places = [Place::None, Place::Behind, Place::Front, Place::Above, Place::Through, Place::Toward, Place::Away, Place::Onto, Place::Up, Place::Down, Place::Beside, Place::Into];
    let targets = [Target::Aim, Target::Me, Target::Pronoun, Target::It, Target::Same, Target::Nearest, Target::Everyone, Target::Player(PlayerId(2)), Target::Thing(77), Target::Ground];
    let tuning = bd_sim::willpower::SparkTuning::default();
    let mut n = 0;
    for v in &t.verbs {
        for (k, p) in places.iter().enumerate() {
            for (j, tg) in targets.iter().enumerate() {
                let style = t.styles.get((k * 7 + j * 3) % t.styles.len()).map_or(0, |s| compose::id16(&s.id));
                let mut s = ComposedMove { verb: compose::id16(&v.id), style, place: *p, target: *tg, count: 1 + ((k + j) % 3) as u8, secs: if j % 4 == 0 { 30 } else { 0 }, mods: [0; 4] };
                if (k + j) % 5 == 0 {
                    s.mods[0] = 1;
                }
                let mv = s.expand().unwrap_or_else(|| panic!("{} {:?} {:?} doesn't build", v.id, p, tg));
                assert!(!mv.ops.is_empty());
                assert!(mv.cooldown >= 0.2 && mv.cooldown <= 20.0, "{} cd {}", v.id, mv.cooldown);
                let dbg = format!("{:?}", mv.ops);
                assert!(!dbg.contains("NaN") && !dbg.contains("inf"), "{} {:?}: {dbg}", v.id, p);
                for op in &mv.ops {
                    assert!(!matches!(op, MoveOp::Transform | MoveOp::Tier(_)), "a composed move never transforms");
                }
                let sparks = chain::move_sparks(&tuning, &mv, 0, 8);
                let cast_only = mv.ops.iter().all(|o| matches!(o, MoveOp::Cast { .. }));
                assert!(cast_only || (1..=6).contains(&sparks), "{} costs {sparks}", v.id);
                n += 1;
            }
        }
    }
    eprintln!("{n} composed moves built");
}

// ------------------------------------------------------------------------------------ grammar

#[test]
fn brief_examples_read_right() {
    let s = spec("shadow teleport behind ethan");
    assert_eq!(verb_of(&s), "teleport");
    assert_eq!(s.place, Place::Behind);
    assert_eq!(s.target, Target::Player(PlayerId(1)));
    assert_eq!(VerbTable::get().style(s.style).map(|x| x.id.as_str()), Some("shadow"));

    let s = spec("lightning dash through him");
    assert_eq!((verb_of(&s).as_str(), s.place, s.target), ("dash", Place::Through, Target::Pronoun));
    assert!(matches!(s.expand().unwrap().ops[0], MoveOp::Ram { through: true, element: bd_core::mechanics::Element::Electric, .. }));

    let s = spec("gravity punch");
    assert_eq!(s.kind(), Some(VerbKind::Punch));
    assert!(s.expand().unwrap().ops.iter().any(|o| matches!(o, MoveOp::Weigh { .. })), "a gravity punch pins them down");

    assert_eq!(spec("ice clone").kind(), Some(VerbKind::Clone));
    let s = spec("swap with the dragon");
    assert_eq!((s.kind(), s.target), (Some(VerbKind::Swap), Target::Thing(compose::id16("dragon"))));
    let s = spec("rewind 3 seconds");
    assert_eq!((s.kind(), s.secs), (Some(VerbKind::Rewind), 30));
    assert!(matches!(s.expand().unwrap().ops[0], MoveOp::Rewind { secs } if (secs - 3.0).abs() < 1e-4));
    assert_eq!(spec("fire uppercut").kind(), Some(VerbKind::Uppercut));
    let s = spec("phase through the wall");
    assert!(matches!(s.expand().unwrap().ops[0], MoveOp::PassThrough { .. }));
    let s = spec("pull everyone to me");
    assert_eq!((s.kind(), s.target), (Some(VerbKind::Pull), Target::Everyone));
    assert!(matches!(s.expand().unwrap().ops[0], MoveOp::Field { strength, .. } if strength > 0.0));
    let s = spec("leap onto the tower");
    assert_eq!((s.kind(), s.place, s.target), (Some(VerbKind::Leap), Place::Onto, Target::Ground));
    let s = spec("blink above him");
    assert!(matches!(s.expand().unwrap().ops[0], MoveOp::Relocate { up, .. } if up > 3.0));
    assert_eq!(spec("turn invisible").kind(), Some(VerbKind::Vanish));
    assert_eq!(spec("absorb his attack").kind(), Some(VerbKind::Absorb));
}

#[test]
fn constructs_and_delivery_phrases_are_not_composed() {
    for s in [
        "a dragon",
        "giant sword",
        "hammer",
        "a flaming hammer",
        "summon a dragon",
        "drop a meteor on ethan",
        "throw a fireball",
        "missile at jake",
        "cage ethan",
        "hit mia with a meteor",
        "meteor",
        "hook",
        "a hook",
        "slide",
        "rocket",
        "ice",
        "flame",
        "shield",
        "fire",
        "shadow",
        "lightning bolt",
        "king kong on a skateboard",
        "teleport gun",
        "punching bag",
        "clone trooper",
        "ghost",
        "duck",
        "turtle",
        "tank",
        "",
        "   ",
        "the the the",
    ] {
        assert_eq!(read(s).map(|r| r.describe()), None, "{s:?} must stay a construct / delivery phrase");
    }
}

#[test]
fn spoken_forms_match_typed() {
    for (typed, spoken) in [
        ("shadow teleport behind ethan", "Shadow teleport behind Ethan."),
        ("teleport behind him", "Tele port behind him!"),
        ("uppercut", "Up her cut!"),
        ("phase through the wall", "Faze through the wall."),
        ("rewind 2 seconds", "Rewind two seconds!"),
        ("turn invisible", "Turn in visible."),
        ("flash step behind him", "Flash-step behind him."),
        ("self destruct", "Self-destruct!"),
        ("punch ethan", "Punch Ethan's face"),
        ("teleport behind ethan", "teleporting behind ethan"),
        ("teleport behind ethan", "Teleported behind Ethan."),
    ] {
        let a = read(typed).map(|r| r.spec);
        let b = read(spoken).map(|r| r.spec);
        assert!(a.is_some(), "{typed:?}");
        assert_eq!(a, b, "{typed:?} vs {spoken:?}");
    }
}

#[test]
fn names_and_pronouns() {
    assert_eq!(spec("teleport behind big tony").target, Target::Player(PlayerId(5)));
    assert_eq!(spec("teleport behind tony").target, Target::Player(PlayerId(5)));
    assert_eq!(spec("teleport behind alex99").target, Target::Player(PlayerId(6)));
    assert_eq!(spec("punch ehtan").target, Target::Player(PlayerId(1)), "one typo");
    assert_eq!(spec("punch that guy").target, Target::Pronoun);
    assert_eq!(spec("punch the nearest enemy").target, Target::Nearest);
    assert_eq!(spec("pull everyone").target, Target::Everyone);
    assert_eq!(spec("heal myself").target, Target::Me);
    assert_eq!(spec("pull myself to the wall").target, Target::Me);
    assert!(matches!(spec("pull myself to the wall").expand().unwrap().ops[0], MoveOp::Reel { .. }));
    // a sound-alike matcher from the game (voice): "eaton" → Ethan
    let fuzzy = |ws: &[&str]| (ws.first() == Some(&"eaton")).then_some((PlayerId(1), 1));
    let n = Names { players: &ROSTER, fuzzy: Some(&fuzzy) };
    assert_eq!(compose::read("punch eaton", &n).map(|r| r.spec.target), Some(Target::Player(PlayerId(1))));
    // an unknown name after a place reads as a thing (the host falls back to the aimed enemy)
    assert!(matches!(spec("teleport behind jake").target, Target::Thing(_)));
}

#[test]
fn counts_durations_modifiers() {
    assert_eq!(spec("punch him twice").count, 2);
    assert_eq!(spec("punch him 3 times").count, 3);
    assert_eq!(spec("triple punch").count, 3);
    assert_eq!(spec("dash 3 times").count, 3);
    assert_eq!(spec("hover for 3 seconds").secs, 30);
    assert_eq!(spec("fly for 2 seconds").secs, 20);
    assert_eq!(spec("rewind 1.5 seconds").secs, 15);
    assert_eq!(spec("rewind 2s").secs, 20);
    assert_eq!(spec("punch him 50 times").count, compose::MAX_COUNT, "clamped");
    assert_eq!(spec("rewind 99 seconds").expand().map(|m| matches!(m.ops[0], MoveOp::Rewind { secs } if secs <= 3.0)), Some(true), "rewind is capped");
    let big = spec("giant punch");
    assert_ne!(big.mods, [0; 4]);
    let plain = spec("punch");
    let (b, p) = (big.expand().unwrap(), plain.expand().unwrap());
    let dmg = |m: &bd_sim::powers::MoveDef| match m.ops[0] {
        MoveOp::Shove { damage, .. } => damage,
        _ => 0.0,
    };
    assert!(dmg(&b) > dmg(&p), "giant hits harder");
    assert!(b.cooldown > p.cooldown, "…and recovers slower");
}

#[test]
fn chain_links_share_the_target() {
    let mut steps = vec![None, Some(spec("shadow teleport behind it")), Some(spec("fire uppercut"))];
    compose::grammar::link_targets(&mut steps);
    assert_eq!(steps[2].unwrap().target, Target::Same);
    let mut steps = vec![Some(spec("teleport behind ethan")), Some(spec("dash forward"))];
    compose::grammar::link_targets(&mut steps);
    assert_eq!(steps[1].unwrap().target, Target::Aim, "a movement link keeps the crosshair");
}

/// ≥ 300 phrases (typed + spoken), each with its expected reading in composed_phrases.golden.
#[test]
fn golden() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/");
    let src = std::fs::read_to_string(format!("{dir}composed_phrases.txt")).unwrap();
    let mut out = String::new();
    let mut n = 0;
    let t0 = std::time::Instant::now();
    for line in src.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let phrase = l.trim_start_matches('~');
        let r = read(phrase).map_or("NONE".to_string(), |r| r.describe());
        out.push_str(&format!("{l} => {r}\n"));
        n += 1;
    }
    let per = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
    eprintln!("{n} phrases, {per:.3} ms per phrase");
    assert!(n >= 300, "{n} golden phrases");
    assert!(per < 5.0, "{per} ms per phrase");
    let path = format!("{dir}composed_phrases.golden");
    if std::env::var("BD_GOLDEN_WRITE").is_ok() {
        std::fs::write(&path, &out).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    let mut bad = Vec::new();
    for (a, b) in out.lines().zip(want.lines()) {
        if a != b {
            bad.push(format!("  got  {a}\n  want {b}"));
        }
    }
    assert!(bad.is_empty() && out.lines().count() == want.lines().count(), "{} golden mismatches:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn never_panics_on_odd_input() {
    for s in ["", " ", "'''", "3", "3 3 3 3", "for for for", "teleport teleport", "behind behind", "x".repeat(3000).as_str(), "é ü ñ teleport", "teleport behind \u{2019}s", &"punch ".repeat(40), "1e9 seconds rewind", "rewind -3 seconds", "rewind nan seconds"] {
        let _ = read(s);
    }
}

// ------------------------------------------------------------------------------------ spec / wire

#[test]
fn spec_sanitizing_and_keys() {
    assert_eq!(ComposedMove { verb: 0xdead, ..Default::default() }.sanitized(), None, "unknown verb");
    let s = ComposedMove { verb: compose::id16("punch"), style: 0xbeef, count: 99, secs: 255, mods: [200, 1, 1, 0], ..Default::default() }.sanitized().unwrap();
    assert_eq!(s.style, 0, "unknown style dropped");
    assert_eq!(s.count, compose::MAX_COUNT);
    assert_eq!(s.secs, compose::MAX_TENTHS);
    assert_eq!(s.mods, [1, 0, 0, 0], "unknown / repeated modifiers dropped");
    assert_eq!(s.sanitized(), Some(s), "idempotent");
    // re-wording never dodges a cooldown: style / target don't change the key
    let a = spec("fire teleport behind him");
    let b = spec("shadow teleport behind ethan");
    assert_eq!(a.cd_key(), b.cd_key());
    assert_ne!(a.cd_key(), spec("teleport").cd_key());
    assert!(ComposedMove::is_key(a.cd_key()));
    // a composed key never decodes to a real move
    let r = bd_sim::powers::toolbox::MoveRef::unpack(a.cd_key() - 1);
    assert_eq!(r.power, u16::MAX);
    // wire: bincode round-trip, compact
    let step = ToolStep::Composed(b);
    let cfg = bincode::config::standard();
    let bytes = bincode::serde::encode_to_vec(&step, cfg).unwrap();
    assert!(bytes.len() <= 32, "{} bytes", bytes.len());
    let (back, _): (ToolStep, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    assert_eq!(back, step);
}

// ------------------------------------------------------------------------------------ sim

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
    stand_at(&mut w, B, 0.0, -8.0);
    run(&mut w, 30);
    w
}

fn stand_at(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, p.mv.pos.y.max(1.0), z);
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

fn fire(w: &mut World, s: &str) -> Result<(), PowerError> {
    w.toolbox(A, vec![ToolStep::Composed(compose::read(s, &Names::new(&[(B, "Bob")])).unwrap_or_else(|| panic!("{s}")).spec)], None)
}

#[test]
fn teleport_behind_a_named_player_lands_at_their_back() {
    let mut w = duel();
    // B faces +Z? whatever their look: "behind" = opposite their look
    let face = {
        let q = w.player(B).unwrap();
        Vec3::new(q.look().x, 0.0, q.look().z).normalize()
    };
    let bpos = w.player(B).unwrap().mv.pos;
    fire(&mut w, "shadow teleport behind bob").unwrap();
    let a = w.player(A).unwrap().mv.pos;
    let rel = a - bpos;
    assert!(rel.dot(face) < -1.0, "behind: rel {rel:?}, face {face:?}");
    assert!(Vec3::new(rel.x, 0.0, rel.z).length() < 4.5, "close to them: {rel:?}");
    let evs = run(&mut w, 1);
    let _ = evs;
}

#[test]
fn composed_moves_price_cool_down_and_go_stale() {
    let mut w = duel();
    let seg = w.cfg.rules.segment_size();
    let wp0 = w.player(A).unwrap().willpower;
    fire(&mut w, "punch").unwrap();
    let evs = run(&mut w, 1);
    let paid = wp0 - w.player(A).unwrap().willpower;
    assert!(paid >= seg * 0.99, "a composed punch costs sparks ({paid})");
    let (spec_ev, fired_ev) = (
        evs.iter().chain(w.events().iter()).any(|e| matches!(e, GameEvent::Power(PowerEvent::Composed { player: A, .. }))),
        evs.iter().chain(w.events().iter()).any(|e| matches!(e, GameEvent::Power(PowerEvent::Fired { player: A, how: FireHow::Composed, .. }))),
    );
    let _ = (spec_ev, fired_ev);
    // wait out the cooldown (not the staleness window): the repeat costs more
    run(&mut w, 90);
    let wp1 = w.player(A).unwrap().willpower;
    fire(&mut w, "punch").unwrap();
    run(&mut w, 1);
    let paid2 = wp1 - w.player(A).unwrap().willpower;
    assert!(paid2 > paid + seg * 0.5, "a repeat inside the staleness window costs more ({paid} → {paid2})");
}

#[test]
fn composed_events_carry_the_spec() {
    let mut w = duel();
    let s = compose::read("fire uppercut", &Names::default()).unwrap().spec;
    w.toolbox(A, vec![ToolStep::Composed(s)], None).unwrap();
    let mut evs: Vec<GameEvent> = w.events().to_vec();
    evs.extend(run(&mut w, 2));
    let got = evs.iter().find_map(|e| match e {
        GameEvent::Power(PowerEvent::Composed { player: A, spec, .. }) => Some(*spec),
        _ => None,
    });
    assert_eq!(got, Some(s.sanitized().unwrap()));
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Fired { player: A, how: FireHow::Composed, .. }))));
    // the presentation can intern it
    assert!(compose::interned(&s).is_some());
}

#[test]
fn bogus_specs_are_rejected_by_the_host() {
    let mut w = duel();
    let wp = w.player(A).unwrap().willpower;
    let r = w.toolbox(A, vec![ToolStep::Composed(ComposedMove { verb: 0x1234, ..Default::default() })], None);
    assert_eq!(r, Err(PowerError::NoSuchMove));
    assert_eq!(w.player(A).unwrap().willpower, wp, "nothing spent");
}

#[test]
fn pull_everyone_and_push_everyone() {
    let mut w = duel();
    stand_at(&mut w, B, 0.0, -4.0);
    run(&mut w, 2);
    let d0 = (w.player(B).unwrap().mv.pos - w.player(A).unwrap().mv.pos).length();
    fire(&mut w, "push everyone away").unwrap();
    run(&mut w, 20);
    let d1 = (w.player(B).unwrap().mv.pos - w.player(A).unwrap().mv.pos).length();
    assert!(d1 > d0 + 1.0, "pushed away {d0} → {d1}");
}

#[test]
fn phase_through_a_wall_crosses_it() {
    let mut w = duel();
    // a 3-thick wall right in front of A (A looks along -Z)
    let a = w.player(A).unwrap().mv.pos;
    let m = 1u8;
    for dx in -3..=3 {
        for dy in 0..5 {
            for dz in 0..3 {
                let v = bd_core::voxel::world_to_voxel(a + Vec3::new(dx as f32 * 0.5, dy as f32 * 0.5, -1.5 - dz as f32 * 0.5));
                w.voxels.set(v, m);
            }
        }
    }
    let r = fire(&mut w, "phase through the wall");
    assert_eq!(r, Ok(()));
    let a2 = w.player(A).unwrap().mv.pos;
    assert!(a2.z < a.z - 2.5, "came out the far side: {a:?} → {a2:?}");
}

#[test]
fn deterministic() {
    let script = ["shadow teleport behind bob", "fire uppercut", "gravity punch", "pull everyone to me", "dash through him", "rewind 2 seconds", "freeze him", "clone"];
    let go = || {
        let mut w = duel();
        let mut h = Vec::new();
        for s in script {
            let _ = fire(&mut w, s);
            run(&mut w, 45);
            let p = w.player(A).unwrap();
            let q = w.player(B).unwrap();
            h.push((p.mv.pos, q.mv.pos, p.willpower, q.damage_pct));
        }
        h
    };
    assert_eq!(go(), go());
}

/// Every headline described move DOES something in a real world (no silent no-ops): the caster
/// moves, the target is hit / moved, a construct appears, or a state starts.
#[test]
fn every_headline_move_does_something() {
    let phrases = [
        "shadow teleport behind bob",
        "lightning dash through him",
        "gravity punch",
        "fire uppercut",
        "pull everyone to me",
        "push everyone away",
        "swap with him",
        "blink above him",
        "teleport back",
        "dash",
        "charge him",
        "leap",
        "fly",
        "punch him",
        "kick him twice",
        "slam",
        "stomp",
        "smash",
        "fire slash",
        "flurry",
        "spin attack",
        "throw him",
        "grab him",
        "yank him",
        "force push him",
        "lift him",
        "crush him down",
        "freeze him",
        "burn him",
        "shock him",
        "poison him",
        "blind him",
        "stun him",
        "explode",
        "clone",
        "turn invisible",
        "grow",
        "shrink",
        "heal",
        "harden",
        "stop time",
        "rewind 2 seconds",
        "dodge",
        "go intangible",
        "block",
        "parry",
        "reflect",
    ];
    let mut dead = Vec::new();
    for s in phrases {
        let mut w = duel();
        stand_at(&mut w, B, 0.0, -3.5);
        // a little history for rewind / heal
        w.player_mut(A).unwrap().damage_pct = 30.0;
        run(&mut w, 40);
        let a0 = w.player(A).unwrap().mv.pos;
        let b0 = w.player(B).unwrap().mv.pos;
        let bd0 = w.player(B).unwrap().damage_pct;
        let c0 = w.constructs.iter().count();
        let r = fire(&mut w, s);
        let evs = run(&mut w, 30);
        let a1 = w.player(A).unwrap().mv.pos;
        let b1 = w.player(B).unwrap().mv.pos;
        let moved_a = (a1 - a0).length() > 0.5;
        let moved_b = (b1 - b0).length() > 0.5;
        let hit_b = w.player(B).unwrap().damage_pct > bd0 + 0.1;
        let spawned = w.constructs.iter().count() > c0;
        let op_beat = evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Op(_) | PowerEvent::TimeStop { .. } | PowerEvent::Blinked { .. }) | GameEvent::Resized { .. }));
        let healed = w.player(A).unwrap().damage_pct < 30.0 - 0.1;
        if r.is_err() || !(moved_a || moved_b || hit_b || spawned || op_beat || healed) {
            dead.push(format!("{s:?}: {r:?} moved_a={moved_a} moved_b={moved_b} hit_b={hit_b} spawned={spawned} beat={op_beat}"));
        }
    }
    assert!(dead.is_empty(), "silent / failing described moves:\n{}", dead.join("\n"));
}

/// "teleport behind bob, fire uppercut": the uppercut goes for the same target (a chain `Same`).
#[test]
fn chain_teleport_then_strike_hits_the_target() {
    let mut w = duel();
    let names = [(B, "Bob")];
    let n = Names::new(&names);
    let mut specs = vec![compose::read("teleport behind bob", &n).map(|r| r.spec), compose::read("fire uppercut", &n).map(|r| r.spec)];
    compose::grammar::link_targets(&mut specs);
    let steps: Vec<ToolStep> = specs.into_iter().map(|s| ToolStep::Composed(s.unwrap())).collect();
    let d0 = w.player(B).unwrap().damage_pct;
    w.toolbox(A, steps, None).unwrap();
    run(&mut w, 90);
    assert!(w.player(B).unwrap().damage_pct > d0 + 1.0, "the uppercut after the teleport landed on Bob");
}
