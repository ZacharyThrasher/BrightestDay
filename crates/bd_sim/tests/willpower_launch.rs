//! WILLPOWER = LAUNCH (host match rule `MatchRules::willpower_launch`, `bd_sim::launch`): the launch
//! curve read at hit time (players, construct-delivered hits, construct impulses), casts / moves never
//! refused, slow regen + fast charge, the anti-camp leak, wire round-trip, FFA unchanged when off, and
//! a bots FFA with the rule on (invariants checked every tick).

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form};
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::bots::{self, recipe};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::launch::{LaunchTuning, curve};
use bd_sim::tuning::CombatTuning;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn duel(launch: bool) -> World {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    cfg.rules.willpower_launch = launch;
    let mut w = World::new(cfg);
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w.player_mut(A).unwrap().mv.pos = Vec3::new(-3.0, 9.0, 0.0);
    w.player_mut(B).unwrap().mv.pos = Vec3::new(3.0, 9.0, 0.0);
    for _ in 0..60 {
        w.tick(&[]);
    }
    w
}

/// Knockback B takes from one hit by A (A's bar at `wp`, B at 80 %).
fn kb_at(launch: bool, wp: f32, construct: bool) -> (f32, f32) {
    let mut w = duel(launch);
    let cid = if construct {
        let eye = w.player(A).unwrap().mv.pos + Vec3::Y * 1.6;
        Some(w.cast(A, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.0), None, Aim::new(eye, Vec3::X)).expect("summon"))
    } else {
        None
    };
    w.player_mut(A).unwrap().willpower = wp;
    w.player_mut(B).unwrap().damage_pct = 80.0;
    let pos = w.player(B).unwrap().mv.pos;
    let kind = if construct { HitKind::Creature } else { HitKind::Melee };
    let mut hit = HitInfo::new(B, Some(A), kind, 10.0, 8.0, Vec3::X, pos);
    hit.construct = cid;
    let r = w.apply_hit(hit);
    assert!(r.applied);
    (r.knockback, r.damage)
}

#[test]
fn launch_scales_with_the_attackers_bar_at_hit_time() {
    let t = LaunchTuning::default();
    for construct in [false, true] {
        let (base, base_dmg) = kb_at(false, 0.0, construct);
        // Rule off: the bar doesn't matter.
        assert!((kb_at(false, 100.0, construct).0 - base).abs() < 1e-4);
        for (wp, want) in [(0.0, t.empty_mult), (50.0, t.mid_mult), (100.0, t.full_mult), (75.0, curve(&t, 0.75))] {
            let (kb, dmg) = kb_at(true, wp, construct);
            assert!((kb / base - want).abs() < 1e-3, "construct={construct} wp={wp}: {} vs {want}", kb / base);
            assert!((dmg - base_dmg).abs() < 1e-4, "damage % is untouched");
        }
    }
    // The curve: ≈ 0.55 empty → 1.0 half → 1.35 full.
    assert!((curve(&t, 0.0) - 0.55).abs() < 1e-4 && (curve(&t, 0.5) - 1.0).abs() < 1e-4 && (curve(&t, 1.0) - 1.35).abs() < 1e-4);
}

#[test]
fn construct_impulses_scale_too() {
    let run = |wp: f32| {
        let mut w = duel(true);
        let eye = w.player(B).unwrap().mv.pos + Vec3::Y * 1.6;
        let cid = w.cast(B, recipe("ball", Form::Ball, &[], Element::Light, 1.0), None, Aim::new(eye, Vec3::NEG_X)).expect("ball");
        w.player_mut(A).unwrap().willpower = wp;
        let v0 = w.constructs.get(cid).unwrap().vel;
        let point = w.constructs.get(cid).unwrap().center();
        w.apply_construct_hit(bd_sim::combat::ConstructHit { target: cid, source: Some(A), damage: 0.1, impulse: Vec3::X * 10.0, blade: false, pierce: false, blunt: false, point });
        (w.constructs.get(cid).unwrap().vel - v0).length()
    };
    let (lo, hi) = (run(0.0), run(100.0));
    assert!(hi > 0.0 && lo > 0.0, "the impulse moved it ({lo}, {hi})");
    assert!((hi / lo - 1.35 / 0.55).abs() < 0.05, "{}", hi / lo);
}

#[test]
fn casts_and_moves_are_never_refused() {
    let mut w = duel(true);
    let eye = w.player(A).unwrap().mv.pos + Vec3::Y * 1.6;
    w.player_mut(A).unwrap().willpower = 0.0;
    // A big creature at 0 fires at full size and drains nothing more (floor 0).
    let big = recipe("dragon", Form::Serpent, &[], Element::Fire, 4.0);
    let id = w.cast(A, big.clone(), None, Aim::new(eye, Vec3::X)).expect("never refused");
    let c = w.constructs.get(id).unwrap();
    assert!((c.recipe.scale - 4.0).abs() < 1e-3 || c.recipe.scale >= 3.9, "not sputtered: {}", c.recipe.scale);
    assert_eq!(c.flair & bd_sim::replicate::cflags::SPUTTER, 0);
    assert_eq!(w.player(A).unwrap().willpower, 0.0);
    // Its constructs don't starve at 0.
    let i0 = w.constructs.get(id).unwrap().integrity;
    for _ in 0..120 {
        w.tick(&[]);
    }
    if let Some(c) = w.constructs.get(id) {
        assert!(c.integrity >= i0 - 1e-3, "starved: {i0} → {}", c.integrity);
    }
    // Partial: a 2-spark sword from 5 willpower drains to 0.
    let mut w = duel(true);
    w.player_mut(A).unwrap().willpower = 5.0;
    w.cast(A, recipe("sword", Form::Sword, &[], Element::Light, 1.0), None, Aim::new(eye, Vec3::X)).expect("fires");
    assert_eq!(w.player(A).unwrap().willpower, 0.0);
    // Rule off, the big serpent from 5: a sputter (shipped behaviour) — still fires, flagged.
    let mut w = duel(false);
    w.player_mut(A).unwrap().willpower = 5.0;
    let id = w.cast(A, recipe("dragon", Form::Serpent, &[], Element::Fire, 4.0), None, Aim::new(eye, Vec3::X)).unwrap();
    assert_ne!(w.constructs.get(id).unwrap().flair & bd_sim::replicate::cflags::SPUTTER, 0);
}

#[test]
fn slow_regen_fast_charge() {
    let refill = |launch: bool, charge: bool| {
        let mut w = duel(launch);
        w.player_mut(A).unwrap().willpower = 10.0;
        let inp = PlayerInput { buttons: if charge { buttons::CHARGE } else { 0 }, ..Default::default() };
        for _ in 0..(2.0 / DT) as u32 {
            w.tick(&[(A, inp)]);
        }
        w.player(A).unwrap().willpower - 10.0
    };
    let t = LaunchTuning::default();
    let (regen_off, regen_on) = (refill(false, false), refill(true, false));
    assert!((regen_on / regen_off - t.regen_mult).abs() < 0.02, "{regen_on} / {regen_off}");
    let (ch_off, ch_on) = (refill(false, true) - regen_off, refill(true, true) - regen_on);
    assert!((ch_on / ch_off - t.charge_mult).abs() < 0.05, "{ch_on} / {ch_off}");
}

#[test]
fn a_full_bar_leaks_to_the_line_when_idle_not_when_active() {
    let t = LaunchTuning::default();
    let mut w = duel(true);
    let mut tn = w.tuning().clone();
    tn.willpower.regen_per_sec = 0.0;
    w.set_tuning(tn);
    // Idle (never cast): past leak_after_secs of match time it leaks at leak_per_sec…
    for _ in 0..((t.leak_after_secs + 0.5) / DT) as u32 {
        w.player_mut(A).unwrap().willpower = 100.0;
        w.tick(&[]);
    }
    w.player_mut(A).unwrap().willpower = 100.0;
    for _ in 0..(1.0 / DT) as u32 {
        w.tick(&[]);
    }
    let wp = w.player(A).unwrap().willpower;
    assert!((100.0 - wp - t.leak_per_sec).abs() < 0.1, "{wp}");
    for _ in 0..(20.0 / DT) as u32 {
        w.tick(&[]);
    }
    assert!((w.player(A).unwrap().willpower - t.leak_above * 100.0).abs() < 1e-3);
    // Just cast: no leak for leak_after_secs.
    let eye = w.player(A).unwrap().mv.pos + Vec3::Y * 1.6;
    w.cast(A, recipe("sword", Form::Sword, &[], Element::Light, 1.0), None, Aim::new(eye, Vec3::X)).unwrap();
    w.player_mut(A).unwrap().willpower = 100.0;
    let up = w.tuning().willpower.upkeep_held_worn;
    for _ in 0..((t.leak_after_secs - 0.5) / DT) as u32 {
        w.tick(&[]);
    }
    let wp = w.player(A).unwrap().willpower;
    assert!(wp > 100.0 - up * t.leak_after_secs - 0.1, "leaked while active: {wp}");
    // Charging never leaks.
    let mut w = duel(true);
    w.player_mut(B).unwrap().willpower = 95.0;
    for _ in 0..(8.0 / DT) as u32 {
        w.tick(&[(B, PlayerInput { buttons: buttons::CHARGE, ..Default::default() })]);
    }
    assert!(w.player(B).unwrap().willpower >= 99.9);
    // Rule off: no leak.
    let mut w = duel(false);
    w.player_mut(A).unwrap().willpower = 100.0;
    for _ in 0..(10.0 / DT) as u32 {
        w.tick(&[]);
    }
    assert!(w.player(A).unwrap().willpower >= 99.9);
}

#[test]
fn rule_and_tuning_round_trip_the_wire() {
    let mut cfg = SimConfig::default();
    cfg.rules.willpower_launch = true;
    cfg.tuning.launch.full_mult = 1.5;
    let bytes = bincode::serde::encode_to_vec(&cfg.rules, bincode::config::standard()).unwrap();
    let (rules, _): (bd_sim::rules::MatchRules, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(rules, cfg.rules);
    let bytes = bincode::serde::encode_to_vec(&cfg.tuning, bincode::config::standard()).unwrap();
    let (tuning, _): (CombatTuning, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(tuning.launch, cfg.tuning.launch);
    // The shipped combat.ron carries the block and parses to the defaults.
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/combat.ron")).unwrap();
    let t = CombatTuning::from_ron(&src).unwrap();
    assert_eq!(t.launch, LaunchTuning::default());
    assert!(!bd_sim::rules::MatchRules::default().willpower_launch, "off by default");
}

fn ffa(seed: u64, ticks: u32, launch: Option<LaunchTuning>) -> (u64, usize, usize) {
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed }, seed, check_invariants: true, ..SimConfig::default() };
    cfg.timings.warmup_secs = 1.0;
    if let Some(t) = launch {
        cfg.rules.willpower_launch = true;
        cfg.tuning.launch = t;
    }
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 4, 0.8, seed);
    let mut inputs = Vec::new();
    let (mut hits, mut charges) = (0, 0);
    for _ in 0..ticks {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        let evs = w.tick(&inputs);
        hits += evs.iter().filter(|e| matches!(e, GameEvent::Hit { .. })).count();
        charges += evs.iter().filter(|e| matches!(e, GameEvent::Charge { kind: bd_sim::charge::ChargeEvent::Start, .. })).count();
        assert!(w.check_invariants().is_ok(), "{:?}", w.check_invariants());
    }
    (w.state_hash(), hits, charges)
}

/// Rule OFF: every hook is an exact ×1.0 / a short-circuited branch, so the FFA is bit-identical to
/// the pre-rule build (checked by hand against a hash recorded before the feature landed; other
/// features have moved the hash since, so it isn't pinned here) — and the launch tuning is inert
/// while off (a wild curve changes nothing).
#[test]
fn ffa_hash_unchanged_with_the_rule_off() {
    let (h, _, _) = ffa(3, 120 * 60, None);
    let mut cfg_t = LaunchTuning::default();
    cfg_t.empty_mult = 0.1;
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed: 3 }, seed: 3, check_invariants: true, ..SimConfig::default() };
    cfg.tuning.launch = cfg_t;
    cfg.timings.warmup_secs = 1.0;
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 4, 0.8, 3);
    let mut inputs = Vec::new();
    for _ in 0..120 * 60 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        w.tick(&inputs);
    }
    assert_eq!(w.state_hash(), h, "launch tuning must be inert with the rule off");
    println!("FFA off hash={h:#x}");
}

#[test]
fn bots_ffa_with_the_rule_on_runs_clean() {
    let (h1, hits, charges) = ffa(5, 120 * 90, Some(LaunchTuning::default()));
    let (h2, _, _) = ffa(5, 120 * 90, Some(LaunchTuning::default()));
    assert_eq!(h1, h2, "deterministic");
    println!("launch FFA: hits={hits} charges={charges}");
    assert!(hits > 0, "bots fought");
    assert!(charges > 0, "bots charged");
}

#[test]
fn landing_a_combo_refills_a_little() {
    // Owner 2026-09-27: combos refill willpower (2nd+ step of a chain on the same target).
    for launch in [false, true] {
        let mut w = duel(launch);
        w.player_mut(A).unwrap().willpower = 20.0;
        let mut steps = Vec::new();
        for k in 0..3 {
            let pos = w.player(B).unwrap().mv.pos;
            let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, 6.0, 30.0, Vec3::X, pos));
            assert!(r.applied);
            steps.push(w.player(A).unwrap().willpower);
            for _ in 0..(6 + k) {
                w.tick(&[]);
            }
        }
        let gain = steps[2] - 20.0;
        if launch {
            assert!(gain > 1.0, "chain hits refill the attacker with the rule on: {steps:?}");
        } else {
            assert!(gain < 1.0, "no combo refill with the rule off: {steps:?}");
        }
    }
}
