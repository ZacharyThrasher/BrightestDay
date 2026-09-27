//! Map interactables (High Roller, `world/interactables.rs`): lever pulls by hit (ring blast,
//! shove) and by tether, stake + cooldown + "can't pay = clunk", the payout distribution over many
//! seeded pulls, effect payouts graft casts then wear off, the jackpot (refill + next cast
//! legendary, once), busts never hurt, the roulette house spin pays only inside the lit pocket
//! region, determinism, and maps without interactables unchanged.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::GameEvent;
use bd_sim::movement::testkit::aim_at;
use bd_sim::rng::Fnv64;
use bd_sim::world::interactables::{BustKind, CasinoEvent, PullBy, SlotOutcome, pull};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};
use bd_voxel::{Interactables, RouletteWheel, SlotMachine, mat};
use glam::IVec3;

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

/// Where the test machine's lever knob is, relative to the floor.
const LEVER: Vec3 = Vec3::new(0.0, 1.5, -10.0);

fn floor_y(w: &World) -> f32 {
    w.arena.spawns[0].y
}

/// A flat world with one slot machine (a stone post with the lever knob in front of its face) and
/// a roulette wheel of 38 pockets around (20, floor, 20).
fn world(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 9, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    let fy = floor_y(&w);
    let lever = LEVER + Vec3::Y * fy;
    // The cabinet: a 2 × 4 × 1 m post just behind the knob (the "mount plate" a tether hooks).
    for y in 0..8 {
        for x in -2..2 {
            for z in 0..2 {
                w.voxels.set(IVec3::new(x, (fy / 0.5) as i32 + y, (lever.z / 0.5) as i32 - 3 - z), mat::STONE);
            }
        }
    }
    let wheel = RouletteWheel {
        center: Vec3::new(20.0, fy, 20.0),
        r_inner: 3.0,
        r_outer: 9.0,
        y_min: fy - 0.5,
        y_max: fy + 1.0,
        zero_angle: 0.0,
        labels: bd_voxel::interact::AMERICAN_WHEEL.iter().map(|p| p.0).collect(),
        colors: bd_voxel::interact::AMERICAN_WHEEL.iter().map(|p| p.1).collect(),
    };
    w.arena.interactables = Interactables {
        slots: vec![SlotMachine { lever, radius: 1.3, reels: lever + Vec3::new(-2.0, 0.0, 0.0), facing: Vec3::Z, window: [1.0, 0.5] }],
        roulette: Some(wheel),
    };
    for k in 1..=n {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    // Keep the house spin out of the slot tests unless asked for.
    w.interact.house.next = 1.0e6;
    w
}

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
}

fn casino_events(evs: &[GameEvent]) -> Vec<CasinoEvent> {
    evs.iter().filter_map(|e| if let GameEvent::Casino(c) = e { Some(*c) } else { None }).collect()
}

/// Tick until something matches (max `n` ticks); returns every casino event seen.
fn run_until(w: &mut World, n: usize, stop: impl Fn(&CasinoEvent) -> bool) -> Vec<CasinoEvent> {
    let mut seen = Vec::new();
    for _ in 0..n {
        let evs = casino_events(w.tick(&[]));
        let done = evs.iter().any(&stop);
        seen.extend(evs);
        if done {
            break;
        }
    }
    seen
}

fn pulled(evs: &[CasinoEvent]) -> Option<(PlayerId, PullBy)> {
    evs.iter().find_map(|e| if let CasinoEvent::Pull { player, by, .. } = e { Some((*player, *by)) } else { None })
}

#[test]
fn ring_blast_pulls_the_lever() {
    let mut w = world(1);
    let fy = floor_y(&w);
    place(&mut w, A, Vec3::new(0.0, fy, 0.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let (yaw, pitch) = aim_at(eye, LEVER + Vec3::Y * fy + Vec3::new(0.0, 0.0, -1.2));
    let before = w.player(A).unwrap().willpower;
    let mut evs = casino_events(w.tick(&[(A, PlayerInput { yaw, pitch, buttons: buttons::PRIMARY, ..Default::default() })]));
    evs.extend(run_until(&mut w, 120, |e| matches!(e, CasinoEvent::Pull { .. })));
    assert_eq!(pulled(&evs), Some((A, PullBy::Blast)), "events: {evs:?}");
    let stake = w.cfg.tuning.casino.stake;
    let wp = w.player(A).unwrap().willpower;
    assert!(wp <= before - stake + 1.0, "stake not paid: {before} → {wp}");
}

#[test]
fn shove_pulls_the_lever_in_reach() {
    let mut w = world(1);
    let fy = floor_y(&w);
    place(&mut w, A, Vec3::new(0.0, fy, -7.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let (yaw, pitch) = aim_at(eye, LEVER + Vec3::Y * fy);
    let evs = casino_events(w.tick(&[(A, PlayerInput { yaw, pitch, buttons: buttons::SECONDARY, ..Default::default() })]));
    assert_eq!(pulled(&evs), Some((A, PullBy::Melee)), "events: {evs:?}");
}

#[test]
fn tether_yank_pulls_the_lever() {
    let mut w = world(1);
    let fy = floor_y(&w);
    place(&mut w, A, Vec3::new(0.5, fy, 2.0));
    w.tick(&[]);
    let mut evs = Vec::new();
    for t in 0..240 {
        let eye = w.player(A).unwrap().eye(w.params());
        // Aim at the mount plate right behind the knob.
        let (yaw, pitch) = aim_at(eye, LEVER + Vec3::Y * fy + Vec3::new(0.0, 0.0, -1.1));
        let b = if t % 60 < 50 { buttons::TETHER_L } else { 0 };
        evs.extend(casino_events(w.tick(&[(A, PlayerInput { yaw, pitch, buttons: b, ..Default::default() })])));
        if pulled(&evs).is_some() {
            break;
        }
    }
    assert_eq!(pulled(&evs), Some((A, PullBy::Tether)), "events: {evs:?}");
}

#[test]
fn stake_cooldown_and_clunk() {
    let mut w = world(1);
    let t = w.cfg.tuning.casino.clone();
    w.player_mut(A).unwrap().willpower = 50.0;
    assert!(pull(&mut w, 0, A, PullBy::Melee));
    assert!((w.player(A).unwrap().willpower - (50.0 - t.stake)).abs() < 1e-3);
    // Cooling down: a second hit does nothing (no charge).
    assert!(!pull(&mut w, 0, A, PullBy::Melee));
    assert!((w.player(A).unwrap().willpower - (50.0 - t.stake)).abs() < 1e-3);
    // The spin pays out after `spin_secs`, the lever frees after `cooldown`.
    let evs = run_until(&mut w, (t.cooldown * 120.0) as usize + 4, |_| false);
    assert!(evs.iter().any(|e| matches!(e, CasinoEvent::Payout { player: A, .. })));
    // Can't pay: a clunk, nothing lost, no spin.
    w.player_mut(A).unwrap().willpower = t.stake * 0.5;
    w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Normal;
    let before = w.player(A).unwrap().willpower;
    let evs0 = w.events().len();
    assert!(!pull(&mut w, 0, A, PullBy::Shot));
    assert!(w.events()[evs0..].iter().any(|e| matches!(e, GameEvent::Casino(CasinoEvent::Clunk { player: A, .. }))));
    assert_eq!(w.player(A).unwrap().willpower, before);
    assert_eq!(w.interact.machines[0].spin, 0.0);
}

/// Pull `n` times as fast as the (shortened) machine allows; returns the outcomes.
fn pull_many(w: &mut World, n: usize) -> Vec<SlotOutcome> {
    w.cfg.tuning.casino.spin_secs = 0.05;
    w.cfg.tuning.casino.cooldown = 0.05;
    w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        pull(w, 0, A, PullBy::Melee);
        for e in casino_events(w.tick(&[])) {
            if let CasinoEvent::Payout { outcome, .. } = e {
                out.push(outcome);
            }
        }
    }
    out
}

#[test]
fn payout_distribution_matches_the_tuning() {
    let mut w = world(1);
    let n = 3000;
    let outs = pull_many(&mut w, n);
    let t = &w.cfg.tuning.casino;
    let jackpots = outs.iter().filter(|o| matches!(o, SlotOutcome::Jackpot)).count() as f32 / n as f32;
    let busts = outs.iter().filter(|o| matches!(o, SlotOutcome::Bust(_))).count() as f32 / n as f32;
    let effects = outs.iter().filter(|o| matches!(o, SlotOutcome::Effect(_))).count() as f32 / n as f32;
    // Binomial 3σ at n = 3000 is ~1.3 % for p ≈ 0.06 and ~2.3 % for p ≈ 0.24.
    assert!((jackpots - t.p_jackpot).abs() < 0.015, "jackpot rate {jackpots} vs {}", t.p_jackpot);
    assert!((busts - t.p_bust).abs() < 0.025, "bust rate {busts} vs {}", t.p_bust);
    assert!((effects - (1.0 - t.p_jackpot - t.p_bust)).abs() < 0.03, "effect rate {effects}");
    // Every bust kind and many different effects turn up; effects are only graft-only commons.
    for k in [BustKind::Confetti, BustKind::Pie, BustKind::Hop] {
        assert!(outs.contains(&SlotOutcome::Bust(k)), "never saw {k:?}");
    }
    let table = bd_sim::party::EffectTable::shipped();
    let mut distinct: Vec<u16> = outs.iter().filter_map(|o| if let SlotOutcome::Effect(e) = o { Some(*e) } else { None }).collect();
    distinct.sort();
    distinct.dedup();
    assert!(distinct.len() >= 20, "only {} distinct effects", distinct.len());
    for e in &distinct {
        let s = table.get(*e).unwrap();
        assert!(s.rarity == bd_sim::party::Rarity::Common && s.hooks.is_empty() && !s.graft.is_empty(), "{} can't be paid by a slot", s.id);
    }
}

#[test]
fn effect_payout_grafts_casts_then_wears_off() {
    let mut w = world(1);
    w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
    w.cfg.tuning.casino.p_jackpot = 0.0;
    w.cfg.tuning.casino.p_bust = 0.0;
    // "Spicy" style effects force an element: find a pull that pays one we can see on a cast.
    let table = bd_sim::party::EffectTable::shipped();
    let mut effect = None;
    for _ in 0..40 {
        pull(&mut w, 0, A, PullBy::Melee);
        let evs = run_until(&mut w, 400, |e| matches!(e, CasinoEvent::Payout { .. }));
        if let Some(CasinoEvent::Payout { outcome: SlotOutcome::Effect(e), .. }) = evs.iter().find(|e| matches!(e, CasinoEvent::Payout { .. })) {
            let g = &table.get(*e).unwrap().graft;
            if g.element.is_some() || !g.mechanics.is_empty() {
                effect = Some(*e);
                break;
            }
        }
        w.interact.machines[0].cooldown = 0.0;
    }
    let e = effect.expect("an element / mechanic effect");
    assert_eq!(w.interact.effect_of(A), Some(e));
    let g = table.get(e).unwrap().graft.clone();
    let eye = w.player(A).unwrap().eye(w.params());
    let r = recipe("wall", Form::Wall, &[], Element::Light, 4.0);
    let id = w.cast(A, r, None, Aim::new(eye, Vec3::new(0.0, -0.3, 1.0).normalize())).unwrap();
    let got = w.constructs.get(id).unwrap().recipe.clone();
    let grafted = g.element.is_some_and(|el| got.element == el) || g.mechanics.iter().any(|m| got.mechanics.contains(m));
    assert!(grafted, "cast {:?} missing the graft {:?}", got, g);
    // It wears off.
    let secs = w.cfg.tuning.casino.effect_secs;
    let evs = run_until(&mut w, (secs * 120.0) as usize + 10, |ev| matches!(ev, CasinoEvent::EffectEnded { .. }));
    assert!(evs.iter().any(|ev| matches!(ev, CasinoEvent::EffectEnded { player: A, effect } if *effect == e)));
    assert_eq!(w.interact.effect_of(A), None);
}

#[test]
fn jackpot_refills_and_makes_the_next_cast_legendary_once() {
    let mut w = world(1);
    w.cfg.tuning.casino.p_jackpot = 1.0;
    w.player_mut(A).unwrap().willpower = 40.0;
    assert!(pull(&mut w, 0, A, PullBy::Tether));
    let evs = run_until(&mut w, 400, |e| matches!(e, CasinoEvent::Payout { .. }));
    assert!(evs.iter().any(|e| matches!(e, CasinoEvent::Payout { outcome: SlotOutcome::Jackpot, .. })));
    let max = w.cfg.rules.willpower_max();
    assert!(w.player(A).unwrap().willpower >= max - 1.0, "not refilled");
    assert!(w.interact.is_lucky(A));
    let eye = w.player(A).unwrap().eye(w.params());
    let aim = Aim::new(eye, Vec3::new(0.0, -0.2, 1.0).normalize());
    let a = w.cast(A, recipe("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light, 1.0), None, aim).unwrap();
    assert!(bd_sim::freedom::is_legendary(&w, a), "the jackpot cast isn't legendary");
    assert!(!w.interact.is_lucky(A), "luck not consumed");
    w.dismiss_one(A, a);
    for _ in 0..10 {
        w.tick(&[]);
    }
    let b = w.cast(A, recipe("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light, 1.0), None, aim).unwrap();
    assert!(!bd_sim::freedom::is_legendary(&w, b), "luck used twice");
}

#[test]
fn busts_never_hurt() {
    let mut w = world(2);
    let fy = floor_y(&w);
    w.cfg.tuning.casino.p_jackpot = 0.0;
    w.cfg.tuning.casino.p_bust = 1.0;
    w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
    place(&mut w, A, Vec3::new(3.0, fy, 3.0));
    let mut hops = 0;
    for _ in 0..60 {
        let pct = w.player(A).unwrap().damage_pct;
        pull(&mut w, 0, A, PullBy::Melee);
        let evs = run_until(&mut w, 400, |e| matches!(e, CasinoEvent::Payout { .. }));
        if evs.iter().any(|e| matches!(e, CasinoEvent::Payout { outcome: SlotOutcome::Bust(BustKind::Hop), .. })) {
            hops += 1;
            assert!(w.player(A).unwrap().mv.vel.y > 0.0 || !w.player(A).unwrap().mv.grounded);
        }
        for _ in 0..240 {
            let evs = w.tick(&[]).to_vec();
            assert!(!evs.iter().any(|e| matches!(e, GameEvent::Ko { .. } | GameEvent::Hit { target: A, .. } | GameEvent::Launched { .. })), "a bust hurt: {evs:?}");
        }
        let p = w.player(A).unwrap();
        assert!(p.alive && p.damage_pct == pct);
        w.interact.machines[0].cooldown = 0.0;
    }
    assert!(hops > 5, "only {hops} hops");
}

#[test]
fn house_spin_pays_only_in_the_lit_pockets() {
    let mut w = world(2);
    w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Normal;
    w.cfg.tuning.willpower.regen_per_sec = 0.0;
    w.interact.house.next = 0.5;
    let evs = run_until(&mut w, 200, |e| matches!(e, CasinoEvent::HouseSpin { .. }));
    let Some(CasinoEvent::HouseSpin { pocket, half_width, sweep, lit }) = evs.iter().find(|e| matches!(e, CasinoEvent::HouseSpin { .. })).copied() else {
        panic!("no house spin: {evs:?}")
    };
    let wheel = w.arena.interactables.roulette.clone().unwrap();
    let fy = floor_y(&w);
    let inside = |k: usize| {
        let a = wheel.pocket_angle(k);
        Vec3::new(wheel.center.x + a.cos() * 6.0, fy, wheel.center.z + a.sin() * 6.0)
    };
    let far = (pocket as usize + 19) % 38;
    for p in &mut w.players {
        p.willpower = 10.0;
    }
    place(&mut w, A, inside(pocket as usize));
    place(&mut w, B, inside(far));
    let mut bonus = Vec::new();
    for _ in 0..((sweep + lit) * 120.0) as usize + 20 {
        // Hold them in place (no drift).
        place(&mut w, A, inside(pocket as usize));
        place(&mut w, B, inside(far));
        for e in casino_events(w.tick(&[])) {
            if let CasinoEvent::HouseBonus { player, .. } = e {
                bonus.push(player);
            }
        }
    }
    let (a, b) = (w.player(A).unwrap().willpower, w.player(B).unwrap().willpower);
    let want = 10.0 + w.cfg.tuning.casino.house_bonus_per_sec * lit;
    assert!((a - want).abs() < 1.5, "in the lit pocket: {a} vs ~{want}");
    assert!((b - 10.0).abs() < 1e-3, "outside: {b}");
    assert_eq!(bonus, vec![A], "bonus events");
    assert!(half_width as usize == w.cfg.tuning.casino.house_half_width as usize);
}

#[test]
fn deterministic_and_idle_without_interactables() {
    let run = |seed: u64| {
        let mut w = world(1);
        w.cfg.seed = seed;
        let outs = pull_many(&mut w, 200);
        let mut h = Fnv64::default();
        h.write(format!("{outs:?}").as_bytes());
        h.0
    };
    assert_eq!(run(1), run(1));
    // A world without interactables never emits a casino event and grafts nothing.
    let mut w = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() });
    w.add_player(A, "a".into(), Corps::from_index(1)).unwrap();
    w.start_live();
    for _ in 0..600 {
        assert!(casino_events(w.tick(&[])).is_empty());
    }
    assert!(!pull(&mut w, 0, A, PullBy::Melee));
    assert!(w.interact.timed.is_empty() && w.interact.lucky.is_empty());
}

/// Sim cost on the real map (`--ignored --nocapture`, release): 8 bots, 60 s, tick p50 / p99 on
/// High Roller vs Archipelago (the halo's 96 surf colliders + the interactables scan).
#[test]
#[ignore = "timing report"]
fn sim_tick_cost_vs_archipelago() {
    let run = |casino: bool| {
        let cfg = SimConfig { seed: 3, check_invariants: false, ..SimConfig::default() };
        let mut w = if casino {
            let (v, m) = bd_voxel::generate_casino(1);
            World::with_arena(cfg, v, m.arena, 1)
        } else {
            World::new(cfg)
        };
        let mut brains = bd_sim::bots::add_bots(&mut w, 0, 8, 0.8, 7);
        w.start_live();
        let mut inputs = Vec::new();
        let mut times = Vec::with_capacity(7200);
        for _ in 0..7200 {
            inputs.clear();
            bd_sim::bots::drive(&mut w, &mut brains, &mut inputs);
            let t0 = std::time::Instant::now();
            w.tick(&inputs);
            times.push(t0.elapsed().as_secs_f64() * 1e3);
        }
        times.sort_by(|a, b| a.total_cmp(b));
        let alive = w.players.iter().filter(|p| p.alive).count();
        let casts = w.constructs.len();
        println!("  {}: alive {alive}, constructs {casts}, kos {}", if casino { "casino" } else { "archipelago" }, w.players.iter().map(|p| p.kos as u32).sum::<u32>());
        (times[times.len() / 2], times[times.len() * 99 / 100], times[times.len() - 1])
    };
    let (a50, a99, amax) = run(false);
    let (c50, c99, cmax) = run(true);
    println!("sim tick (8 bots, 60 s): archipelago p50 {a50:.3} ms p99 {a99:.3} ms max {amax:.3} ms | high roller p50 {c50:.3} ms p99 {c99:.3} ms max {cmax:.3} ms");
}
