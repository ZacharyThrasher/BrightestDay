//! TRANSFORMERS V2 (COMBAT.md §5d-quinquies): hero scale for piloted combat transformers, the
//! FLASH WINDOW (Transform at a string's end / after a launcher = the vehicle attack; anywhere else
//! it just transforms), guard- and armor-breaking vehicle attacks, the BRANCH window after one lands
//! (rush / air-launch / drift-turn back into melee), followers using the window, and a scripted
//! duel that MEASURES presses per hit landed + the vehicle-attack connect rate (`v2_measure`).

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_core::input::buttons;
use bd_sim::constructs::brawl::{Branch, XPhase, hero_scale};
use bd_sim::events::{GameEvent, HeldFx};
use bd_sim::powers::ops::OpEvent;
use bd_sim::powers::{MoveOp, PowerEvent};
use bd_sim::tuning::CombatTuning;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn world() -> World {
    let mut w = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() });
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w
}

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
    p.mv.tumble = 0.0;
}

fn feats(mut r: Recipe, f: &[Feature], mass: f32) -> Recipe {
    r.features = f.to_vec();
    r.mass = mass;
    r.normalized()
}

fn optimus() -> Recipe {
    feats(recipe("Optimus Prime", Form::Vehicle, &[Mechanic::Blade, Mechanic::Shoots, Mechanic::Charge, Mechanic::Transforms], Element::Light, 6.0), &[Feature::Wheels, Feature::Smokestack, Feature::Cab, Feature::Headlight], 9000.0)
}
fn car() -> Recipe {
    feats(recipe("transforming car", Form::Vehicle, &[Mechanic::Charge, Mechanic::Fast, Mechanic::Transforms], Element::Light, 4.5), &[Feature::Wheels, Feature::Windows, Feature::Headlight], 1200.0)
}

/// Cast `r` piloted by A (robot mode), B `gap` m in front. Returns the construct.
fn duel(r: Recipe, gap: f32) -> (World, ConstructId) {
    let mut w = world();
    place(&mut w, A, Vec3::new(-20.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(40.0, GROUND, -40.0));
    w.tick(&[]);
    let mut r = r;
    r.mechanics.push(Mechanic::Piloted);
    let r = r.normalized();
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, r, None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0).normalize())).unwrap();
    w.constructs.get_mut(id).unwrap().life = 1.0e6; // long duels
    for _ in 0..60 {
        w.tick(&[(A, PlayerInput::default())]);
    }
    if w.constructs.get(id).unwrap().held.vehicle {
        assert!(w.toggle_transform(A));
        for _ in 0..200 {
            w.tick(&[(A, PlayerInput::default())]);
        }
    }
    assert!(!w.constructs.get(id).unwrap().held.vehicle, "robot mode");
    front(&mut w, id, gap);
    (w, id)
}

fn front(w: &mut World, id: ConstructId, gap: f32) {
    let c = w.constructs.get(id).unwrap();
    let f = c.forward();
    let f = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::NEG_Z);
    let p = c.center() + f * (c.half.x.max(c.half.z) + gap);
    place(w, B, Vec3::new(p.x, GROUND, p.z));
    w.player_mut(B).unwrap().damage_pct = 0.0;
    w.player_mut(B).unwrap().spawn_shield = 0.0;
}

/// A's input: looking at B, walking in when out of reach.
fn toward_b(w: &World, id: ConstructId, buttons: u16) -> PlayerInput {
    let c = w.constructs.get(id).unwrap();
    let b = w.player(B).unwrap().mv.pos + Vec3::Y * 0.9;
    let d = b - c.center();
    let flat = Vec3::new(d.x, 0.0, d.z);
    let dist = flat.length() - c.half.x.max(c.half.z);
    let yaw = (-d.x).atan2(-d.z);
    let pitch = (d.y / flat.length().max(0.1)).atan().clamp(-1.2, 1.2);
    PlayerInput { buttons, yaw, pitch, move_z: if dist > 2.5 { 1 } else { 0 }, ..PlayerInput::default() }
}

#[derive(Debug, Default)]
struct Metrics {
    presses: u32,
    transform_presses: u32,
    hits: u32,
    xattacks: u32,
    xattacks_landed: u32,
    plain_transforms: u32,
    flashes: u16,
    flashes_taken: u16,
}

/// A scripted duel: `timed` = a player in rhythm (LMB when the robot is ready, Transform ~0.1 s after
/// the string's ender); else a masher (LMB at ~7.5 Hz, Transform every ~0.4 s). B is a still human
/// (put back in front whenever knocked > 14 m away). 20 s.
fn run_duel(r: Recipe, timed: bool) -> Metrics {
    let (mut w, id) = duel(r, 1.5);
    let mut m = Metrics::default();
    let mut ender_at: Option<u32> = None;
    let mut last_x = XPhase::None;
    let mut was_vehicle = false;
    for k in 0..(20 * 120) {
        let c = w.constructs.get(id).unwrap();
        let (bpos, cpos) = (w.player(B).unwrap().mv.pos, c.center());
        if Vec3::new(bpos.x - cpos.x, 0.0, bpos.z - cpos.z).length() > 14.0 + c.half.z.max(c.half.x) || bpos.y < GROUND - 4.0 {
            // Toward the arena's middle, so the duel never drifts off the edge.
            let c = w.constructs.get(id).unwrap();
            let d = Vec3::new(-c.pos.x, 0.0, -c.pos.z).normalize_or(Vec3::NEG_Z);
            let p = c.center() + d * (c.half.x.max(c.half.z) + 2.0);
            place(&mut w, B, Vec3::new(p.x, GROUND, p.z));
            w.player_mut(B).unwrap().damage_pct = 0.0;
        }
        let c = w.constructs.get(id).unwrap();
        let mut b = 0u16;
        let mut transform = false;
        if timed {
            if c.brawl.xatk == XPhase::None && !c.held.vehicle && c.brawl.cd <= 0.0 && k % 2 == 0 {
                b |= buttons::PRIMARY;
            }
            if c.brawl.string_done && c.brawl.open > 0.0 && ender_at.is_none() {
                ender_at = Some(k);
            }
            if ender_at.is_some_and(|e| k == e + 12) {
                transform = true;
            }
            if !c.brawl.string_done {
                ender_at = None;
            }
        } else {
            if k % 16 == 0 {
                b |= buttons::PRIMARY;
            }
            if k % 144 == 60 {
                transform = true;
            }
        }
        if b & buttons::PRIMARY != 0 {
            m.presses += 1;
        }
        if transform {
            m.presses += 1;
            m.transform_presses += 1;
            w.toggle_transform(A);
            if timed {
                ender_at = Some(u32::MAX - 100);
            }
        }
        let inp = toward_b(&w, id, b);
        let ev = w.tick(&[(A, inp)]).to_vec();
        for e in &ev {
            match e {
                GameEvent::Hit { target, source, .. } if *target == B && *source == Some(A) => m.hits += 1,
                _ => {}
            }
        }
        let Some(c) = w.constructs.get(id) else {
            eprintln!("  construct gone at tick {k}: {:?}", ev.iter().filter(|e| matches!(e, GameEvent::ConstructGone { .. })).collect::<Vec<_>>());
            break;
        };
        if last_x == XPhase::None && c.brawl.xatk == XPhase::Burst {
            m.xattacks += 1;
        }
        if last_x != XPhase::None && c.brawl.xatk == XPhase::None && c.brawl.last_xattack_hits > 0 {
            m.xattacks_landed += 1;
        }
        if c.held.vehicle && !was_vehicle && c.brawl.xatk == XPhase::None && last_x == XPhase::None {
            m.plain_transforms += 1;
        }
        was_vehicle = c.held.vehicle;
        (m.flashes, m.flashes_taken) = (c.brawl.flashes, c.brawl.flashes_taken);
        last_x = c.brawl.xatk;
        // A masher that transformed out by accident: transform back (counts as a press).
        if !timed && c.held.vehicle && c.brawl.xatk == XPhase::None && c.held.transform_t <= 0.0 && k % 144 == 100 {
            m.presses += 1;
            w.toggle_transform(A);
        }
    }
    m
}

#[test]
#[ignore = "prints the V2 duel metrics (before / after numbers for docs/piloting/V2_NOTES.md)"]
fn v2_measure() {
    for (name, r) in [("optimus", optimus()), ("car", car())] {
        let (w, id) = duel(r.clone(), 1.5);
        let Some(c) = w.constructs.get(id) else {
            eprintln!("{name}: gone? {:?}", w.constructs.iter().map(|c| (c.id, c.recipe.name.clone())).collect::<Vec<_>>());
            continue;
        };
        eprintln!("{name}: recipe scale {:.2}, robot height {:.2} m, half {:?}", c.recipe.scale, c.half.y * 2.0, c.half);
        for timed in [false, true] {
            let m = run_duel(r.clone(), timed);
            let ppl = m.presses as f32 / m.hits.max(1) as f32;
            let rate = m.xattacks_landed as f32 / m.xattacks.max(1) as f32;
            eprintln!("  {}: {m:?} → presses/hit {ppl:.2}, vehicle attacks landed {:.0}% ({}/{}), per transform press {:.0}%", if timed { "timed " } else { "masher" }, rate * 100.0, m.xattacks_landed, m.xattacks, 100.0 * m.xattacks_landed as f32 / m.transform_presses.max(1) as f32);
        }
    }
}

// ------------------------------------------------------------------------------------ helpers

fn tick_n(w: &mut World, id: ConstructId, b: u16, n: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..n {
        let inp = toward_b(w, id, b);
        ev.extend_from_slice(w.tick(&[(A, inp)]));
    }
    ev
}

fn hits_on_b(ev: &[GameEvent]) -> usize {
    ev.iter().filter(|e| matches!(e, GameEvent::Hit { target, .. } if *target == B)).count()
}

fn flash_fx(ev: &[GameEvent], branch: bool) -> bool {
    ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::Flash, strength, .. } if (*strength < 0.0) == branch))
}

/// Land the string (B put back in front before every hit) until its ender connects and opens the
/// flash window; returns right as it opens.
fn string_to_flash(w: &mut World, id: ConstructId) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    // A launched B may have been KO'd off this small arena: wait for the respawn.
    for _ in 0..2400 {
        if w.player(B).unwrap().alive {
            break;
        }
        tick_n(w, id, 0, 1);
    }
    for _ in 0..12 {
        front(w, id, 1.0);
        ev.extend(tick_n(w, id, buttons::PRIMARY, 1));
        for _ in 0..60 {
            let c = w.constructs.get(id).unwrap();
            if c.brawl.flash > 0.0 {
                return ev;
            }
            if c.brawl.cd <= 0.0 {
                break;
            }
            ev.extend(tick_n(w, id, 0, 1));
        }
    }
    panic!("the ender never opened the flash window");
}

/// Back to robot mode after a plain transform.
fn back_to_robot(w: &mut World, id: ConstructId) {
    for _ in 0..200 {
        tick_n(w, id, 0, 1);
    }
    if w.constructs.get(id).unwrap().held.vehicle {
        assert!(w.toggle_transform(A));
        for _ in 0..200 {
            tick_n(w, id, 0, 1);
        }
    }
    assert!(!w.constructs.get(id).unwrap().held.vehicle);
}

// ------------------------------------------------------------------------------------ STAGE 1

#[test]
fn a_piloted_transformer_stands_at_hero_scale_unless_asked_big() {
    let t = CombatTuning::default().transformer;
    // The default: a piloted Optimus (a 6 m recipe) fights at hero scale.
    let (w, id) = duel(optimus(), 1.5);
    let c = w.constructs.get(id).unwrap();
    let h = c.half.y * 2.0;
    assert!((3.4..=5.0).contains(&h), "hero-scale robot height: {h}");
    assert_eq!(c.recipe.scale, t.hero_max_scale);
    // Said big ("giant optimus" = x2.5), a combiner-class row, a follower, a suit, a squad: kept.
    let piloted = |mut r: Recipe| {
        r.mechanics.push(Mechanic::Piloted);
        r.normalized()
    };
    let mut giant = piloted(optimus());
    giant.scale = 15.0;
    assert!(!hero_scale(&t, &mut giant, 1.0) && giant.scale == 15.0, "said giant keeps its size");
    let mut combiner = piloted(feats(recipe("combining robot", Form::Kaiju, &[Mechanic::Stomp, Mechanic::Heavy, Mechanic::Transforms], Element::Light, 6.5), &[Feature::CraneArm, Feature::Plow], 60000.0));
    assert!(t.archetype_of(&combiner).giant);
    assert!(!hero_scale(&t, &mut combiner, 1.0) && combiner.scale == 6.5, "a colossus row stays big");
    let mut follower = optimus();
    assert!(!hero_scale(&t, &mut follower, 1.0) && follower.scale == 6.0, "followers keep their size");
    let mut suit = piloted(car());
    suit.scale = 2.4;
    assert!(!hero_scale(&t, &mut suit, 1.0) && suit.scale == 2.4);
    let mut squad = piloted(car());
    squad.count = 3;
    assert!(!hero_scale(&t, &mut squad, 1.0));
    let mut plain = piloted(car());
    plain.scale = 5.5;
    assert!(hero_scale(&t, &mut plain, 1.0) && plain.scale == t.hero_max_scale);
    // A two-mode icon whose robot stands 1.45x its alt length: the ROBOT is capped.
    let mut icon = piloted(optimus());
    assert!(hero_scale(&t, &mut icon, 1.45) && (icon.scale * 1.45 - t.hero_max_scale).abs() < 1e-3);
}

#[test]
fn string_hits_connect_on_a_human_sized_target_a_step_away() {
    // Magnetism: B a few metres past the reach — the first press still lands (the slide closes it).
    let (mut w, id) = duel(optimus(), 3.0);
    let ev = tick_n(&mut w, id, buttons::PRIMARY, 1);
    let ev = [ev, tick_n(&mut w, id, 0, 20)].concat();
    assert_eq!(hits_on_b(&ev), 1, "the first press connects on a 1.8 m target");
}

// ------------------------------------------------------------------------------------ STAGE 2

#[test]
fn the_flash_window_opens_at_the_strings_end_and_only_there() {
    let t = CombatTuning::default().transformer;
    let (mut w, id) = duel(optimus(), 1.0);
    // Mid-string: no window — Transform just transforms.
    let ev = tick_n(&mut w, id, buttons::PRIMARY, 1);
    tick_n(&mut w, id, 0, 4);
    assert_eq!(hits_on_b(&ev), 1);
    assert_eq!(w.constructs.get(id).unwrap().brawl.flash, 0.0);
    assert!(w.toggle_transform(A));
    tick_n(&mut w, id, 0, 3);
    assert_eq!(w.constructs.get(id).unwrap().brawl.xatk, XPhase::None, "outside the window: a plain transform");
    back_to_robot(&mut w, id);
    // A whiffed ender opens nothing (B far behind; A stands and swings at the air).
    let behind = w.constructs.get(id).unwrap().pos - w.constructs.get(id).unwrap().forward() * 30.0;
    place(&mut w, B, Vec3::new(behind.x, GROUND, behind.z));
    let f = w.constructs.get(id).unwrap().forward();
    let look = |b: u16| PlayerInput { buttons: b, yaw: (-f.x).atan2(-f.z), ..PlayerInput::default() };
    for _ in 0..8 {
        w.tick(&[(A, look(buttons::PRIMARY))]);
        for _ in 0..50 {
            w.tick(&[(A, look(0))]);
        }
    }
    assert_eq!(w.constructs.get(id).unwrap().brawl.flashes, 0, "no connect, no flash");
    // The ender that connects opens it, `flash_secs` long, with the cue.
    let ev = string_to_flash(&mut w, id);
    assert!(flash_fx(&ev, false), "the gold burst / chime cue");
    let f = w.constructs.get(id).unwrap().brawl.flash;
    assert!(f > t.flash_secs - 0.02 && f <= t.flash_secs, "{f}");
    // It closes: then Transform just transforms again.
    tick_n(&mut w, id, 0, (t.flash_secs * 120.0) as u32 + 2);
    assert_eq!(w.constructs.get(id).unwrap().brawl.flash, 0.0);
    assert!(w.toggle_transform(A));
    tick_n(&mut w, id, 0, 3);
    assert_eq!(w.constructs.get(id).unwrap().brawl.xatk, XPhase::None, "after the window: a plain transform");
    back_to_robot(&mut w, id);
    // Inside it (a human beat late): the vehicle attack, cancelling the ender's recovery.
    string_to_flash(&mut w, id);
    tick_n(&mut w, id, 0, 12);
    assert!(w.constructs.get(id).unwrap().brawl.cd > 0.0, "still recovering from the ender");
    assert!(w.toggle_transform(A));
    tick_n(&mut w, id, 0, 2);
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.xatk, XPhase::Burst, "inside the window: the vehicle attack");
    assert_eq!(c.brawl.flashes_taken, 1);
}

#[test]
fn vehicle_attacks_break_guard_and_counter_stances() {
    let (mut w, id) = duel(optimus(), 1.0);
    // Control: B's counter stance negates a string hit.
    w.fire_op(B, &MoveOp::Counter { secs: 3.0, damage: 0.0, knockback: 0.0, up: 0.0, element: Element::Light }).unwrap();
    let ev = tick_n(&mut w, id, buttons::PRIMARY, 1);
    let ev = [ev, tick_n(&mut w, id, 0, 10)].concat();
    assert_eq!(hits_on_b(&ev), 0, "the stance stops a string hit");
    tick_n(&mut w, id, 0, 400);
    // The vehicle attack goes through a fresh guard / counter stance.
    string_to_flash(&mut w, id);
    front(&mut w, id, 3.0);
    w.fire_op(B, &MoveOp::Counter { secs: 3.0, damage: 10.0, knockback: 20.0, up: 0.3, element: Element::Light }).unwrap();
    assert!(w.toggle_transform(A));
    let mut ev = Vec::new();
    for _ in 0..60 {
        ev.extend(tick_n(&mut w, id, 0, 1));
    }
    let broke = ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: bd_sim::combat::HitKind::HeavyMelee, .. } if *target == B));
    assert!(broke, "the vehicle attack breaks the stance and lands (heavy: the impact frame)");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Op(OpEvent::Parried { .. })))), "nothing parried it");
}

#[test]
fn a_landed_vehicle_attack_branches_into_rush_launch_or_drift() {
    for want in [Branch::Rush, Branch::Launch, Branch::Drift] {
        let (mut w, id) = duel(optimus(), 1.0);
        string_to_flash(&mut w, id);
        front(&mut w, id, 3.0);
        assert!(w.toggle_transform(A));
        let mut ev = Vec::new();
        for _ in 0..120 {
            ev.extend(tick_n(&mut w, id, 0, 1));
            if w.constructs.get(id).unwrap().brawl.branch > 0.0 {
                break;
            }
        }
        assert!(w.constructs.get(id).unwrap().brawl.branch > 0.0, "{want:?}: the connect opens the branch window");
        assert!(flash_fx(&ev, true), "the branch cue");
        let picked = match want {
            Branch::Rush => [tick_n(&mut w, id, buttons::PRIMARY, 1), tick_n(&mut w, id, 0, 1)].concat(),
            Branch::Launch => tick_n(&mut w, id, buttons::SECONDARY, 1),
            _ => {
                assert!(w.toggle_transform(A));
                tick_n(&mut w, id, 0, 1)
            }
        };
        let c = w.constructs.get(id).unwrap();
        assert_eq!(c.brawl.last_branch, want);
        match want {
            Branch::Rush => assert!(c.brawl.xatk == XPhase::Burst && c.held.vehicle && c.brawl.xatk_t < 0.05, "a second pass"),
            Branch::Launch => {
                assert!(c.brawl.xatk == XPhase::Finish && !c.held.vehicle && c.vel.y > 5.0, "the launcher, going up after them");
                assert_eq!(hits_on_b(&picked), 1, "the juggle launcher connects");
            }
            _ => assert!(c.brawl.xatk == XPhase::None && !c.held.vehicle && c.brawl.open > 0.0 && c.brawl.cd <= 0.0, "back in robot melee, the string open"),
        }
        let before = hits_on_b(&ev);
        let mut more = Vec::new();
        for _ in 0..150 {
            more.extend(tick_n(&mut w, id, if want == Branch::Rush { 0 } else { buttons::PRIMARY }, 1));
            more.extend(tick_n(&mut w, id, 0, 1));
        }
        assert!(hits_on_b(&more) >= 1, "{want:?}: the follow-up connects ({before} before)");
        let c = w.constructs.get(id).unwrap();
        assert_eq!(c.brawl.xatk, XPhase::None);
        assert!(!c.held.vehicle, "{want:?}: ends in robot mode");
    }
}

#[test]
fn followers_take_the_flash_window() {
    let mut w = world();
    place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(-30.0, GROUND, 10.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, optimus(), None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0).normalize())).unwrap();
    w.constructs.get_mut(id).unwrap().life = 1.0e6;
    assert!(w.constructs.get(id).unwrap().pilot.is_none());
    assert_eq!(w.constructs.get(id).unwrap().recipe.scale, 6.0, "a follower keeps its size");
    for k in 0..3600 {
        if k % 60 == 0 {
            place(&mut w, B, Vec3::new(-30.0, GROUND, 10.0));
            w.player_mut(B).unwrap().damage_pct = 0.0;
        }
        w.tick(&[]);
        let c = w.constructs.get(id).unwrap();
        if c.brawl.ai_xattacks >= 2 && c.brawl.xatk == XPhase::None {
            break;
        }
    }
    let c = w.constructs.get(id).unwrap();
    eprintln!("follower: {} flashes, {} taken, {} transform attacks, {} branches", c.brawl.flashes, c.brawl.flashes_taken, c.brawl.ai_xattacks, c.brawl.branches);
    assert!(c.brawl.ai_xattacks >= 1, "the follower extends its strings");
    assert_eq!(c.brawl.ai_xattacks, c.brawl.flashes_taken, "every follower transform attack came from a flash window");
}
