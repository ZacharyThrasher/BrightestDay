//! Combat tuning (COMBAT.md §5i): RON round trip, "defaults are bit-identical to the constants",
//! hot reload, and the anti-snowball knobs.

use bd_core::mechanics::Form;
use bd_sim::bots;
use bd_sim::fuzz::{ChaosConfig, ChaosWorld, FORMS, random_recipe, random_rules};
use bd_sim::rng::{Fnv64, SimRng};
use bd_sim::rules::MatchRules;
use bd_sim::tuning::{CombatTuning, FormCost, MechanicCost, GUN_ARCHETYPES};
use bd_sim::weapons::{archetype_for, gun_spec, spec_for};
use bd_sim::willpower::{base_cost, integrity_for_cost, staleness, upkeep_per_sec, validate_recipe};
use bd_sim::world::{ArenaSource, SimConfig, World};

// ================================================================================ RON

#[test]
fn tuning_round_trips_through_ron_and_bincode() {
    let t = CombatTuning::default();
    let ron = t.to_ron().unwrap();
    assert_eq!(CombatTuning::from_ron(&ron).unwrap(), t);
    // Partial files fill in defaults.
    let partial = CombatTuning::from_ron("(ring: (blast_damage: 20.0), willpower: (mechanic_cost: [(mechanic: Explodes, extra: 0.2)]))").unwrap();
    assert_eq!(partial.ring.blast_damage, 20.0);
    assert_eq!(partial.ring.blast_speed, t.ring.blast_speed);
    assert_eq!(partial.willpower.mechanic_cost.len(), 1);
    assert_eq!(partial.forms, t.forms);
    let bin = bincode::serde::encode_to_vec(&t, bincode::config::standard()).unwrap();
    let (back, _): (CombatTuning, _) = bincode::serde::decode_from_slice(&bin, bincode::config::standard()).unwrap();
    assert_eq!(back, t);
    // A SimConfig carries it.
    let cfg = SimConfig::default();
    let bin = bincode::serde::encode_to_vec(&cfg, bincode::config::standard()).unwrap();
    let (back, _): (SimConfig, _) = bincode::serde::decode_from_slice(&bin, bincode::config::standard()).unwrap();
    assert_eq!(back.tuning, t);
}

#[test]
#[ignore = "rewrites assets/tuning/combat.ron from the built-in defaults, keeping its cost knobs"]
fn write_default_combat_ron() {
    // Run after changing any default in bd_sim::tuning (e.g. SmashTuning):
    //   cargo test -p bd_sim --test tuning write_default_combat_ron -- --ignored
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/combat.ron");
    let mut t = CombatTuning::default();
    if let Some(old) = std::fs::read_to_string(path).ok().and_then(|s| CombatTuning::from_ron(&s).ok()) {
        t.willpower.mechanic_cost = old.willpower.mechanic_cost;
        t.willpower.form_cost = old.willpower.form_cost;
    }
    std::fs::write(path, t.to_ron().unwrap()).unwrap();
}

/// The shipped file equals the built-in defaults except for the balance cost knobs (balance with
/// cost, not damage — §5i.1). Fails when a default changes without regenerating the file (a stale
/// file would silently override the new default in the hot-reloading client): run
/// `write_default_combat_ron` (above).
#[test]
fn shipped_combat_ron_is_defaults_plus_cost_knobs() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/combat.ron")).unwrap();
    let mut t = CombatTuning::from_ron(&src).unwrap();
    t.willpower.mechanic_cost.clear();
    t.willpower.form_cost.clear();
    assert_eq!(t, CombatTuning::default(), "assets/tuning/combat.ron is stale: regenerate it (see write_default_combat_ron)");
}

#[test]
fn hostile_tuning_is_sanitised() {
    let mut t = CombatTuning::default();
    t.willpower.per_mechanic = f32::NAN;
    t.willpower.form_cost = vec![FormCost { form: Form::Kaiju, mult: f32::INFINITY }];
    t.willpower.mechanic_cost = vec![MechanicCost { mechanic: bd_core::mechanics::Mechanic::Heavy, extra: -100.0 }];
    t.hit.momentum_span = 0.0;
    t.guns.retain(|g| g.arch != bd_sim::weapons::GunArchetype::Railgun);
    let v = t.validated();
    assert_eq!(v.willpower.per_mechanic, 0.15);
    assert_eq!(v.willpower.form_cost[0].mult, 1.0);
    assert!(v.willpower.mechanic_cost[0].extra >= -0.15);
    assert!(v.hit.momentum_span >= 1.0);
    assert_eq!(v.guns.len(), GUN_ARCHETYPES.len());
    let mut rng = SimRng::new(9);
    for _ in 0..500 {
        let r = validate_recipe(random_recipe(&mut rng, 1), &MatchRules::default());
        let c = v.willpower.base_cost(&r);
        assert!(c.is_finite() && c >= 0.0, "{c}");
    }
}

// ================================================================================ bit-identical

/// Every tuned formula equals the constant-based original bit for bit over the fuzz space.
#[test]
fn default_tuning_formulas_are_bit_identical() {
    let t = CombatTuning::default();
    let rules = MatchRules::default();
    let mut rng = SimRng::new(77);
    for _ in 0..5000 {
        let r = validate_recipe(random_recipe(&mut rng, 2), &rules);
        assert_eq!(t.willpower.base_cost(&r).to_bits(), base_cost(&r).to_bits(), "{r:?}");
        for piloted in [false, true] {
            assert_eq!(t.willpower.upkeep_per_sec(&r, piloted).to_bits(), upkeep_per_sec(&r, piloted).to_bits());
        }
        let c = base_cost(&r) * staleness(2);
        assert_eq!((t.willpower.base_cost(&r) * t.willpower.staleness(2)).to_bits(), c.to_bits());
        assert_eq!(t.construct.integrity_for_cost(c).to_bits(), integrity_for_cost(c).to_bits());
        if let Some(a) = archetype_for(&r) {
            let (x, y) = (t.spec_for(&r, a), spec_for(&r, a));
            assert_eq!(format!("{x:?}"), format!("{y:?}"));
        }
    }
    for u in 0..10 {
        assert_eq!(t.willpower.staleness(u).to_bits(), staleness(u).to_bits());
    }
    for a in GUN_ARCHETYPES.into_iter().chain([bd_sim::weapons::GunArchetype::Ring]) {
        assert_eq!(format!("{:?}", t.gun_spec(a)), format!("{:?}", gun_spec(a)));
    }
    for f in FORMS {
        let s = bd_sim::constructs::form_spec(f);
        assert_eq!(t.form(f), (s.damage, s.knockback));
        assert_eq!(t.form_spec(f), s);
    }
    for v in [0.0f32, 7.9, 8.0, 12.5, 23.0, 40.0, 1000.0] {
        assert_eq!(t.hit.momentum_bonus(v).to_bits(), bd_sim::combat::momentum_bonus(v).to_bits());
        assert_eq!(t.hit.splat_damage(v).to_bits(), bd_sim::combat::splat_damage(v).to_bits());
    }
}

/// A cost knob changes cost and nothing else.
#[test]
fn cost_knobs_only_change_cost() {
    let mut t = CombatTuning::default();
    t.willpower.mechanic_cost.push(MechanicCost { mechanic: bd_core::mechanics::Mechanic::Explodes, extra: 0.3 });
    t.willpower.form_cost.push(FormCost { form: Form::Bomb, mult: 1.5 });
    let t = t.validated();
    let d = CombatTuning::default();
    let bomb = bots::recipe("bomb", Form::Bomb, &[bd_core::mechanics::Mechanic::Explodes], bd_core::mechanics::Element::Fire, 0.7);
    let hammer = bots::recipe("hammer", Form::Hammer, &[bd_core::mechanics::Mechanic::Blunt], bd_core::mechanics::Element::Light, 1.2);
    let expect = d.willpower.base_cost(&bomb) / 1.15 * (1.15 + 0.3) * 1.5;
    assert!((t.willpower.base_cost(&bomb) - expect).abs() < 1e-3, "{} vs {expect}", t.willpower.base_cost(&bomb));
    assert_eq!(t.willpower.base_cost(&hammer), d.willpower.base_cost(&hammer));
    assert_eq!(t.forms, d.forms);
    // Integrity is priced from the unadjusted cost: a pricier bomb is not a tougher bomb.
    let adj = t.willpower.base_cost(&bomb) * t.willpower.adjustment_ratio(&bomb);
    assert!((adj - d.willpower.base_cost(&bomb)).abs() < 1e-4);
    // A form multiplier prices upkeep too (sustained held weapons).
    let mut g = CombatTuning::default();
    g.willpower.form_cost.push(FormCost { form: Form::Gun, mult: 2.0 });
    let gun = bots::recipe("gun", Form::Gun, &[bd_core::mechanics::Mechanic::Shoots], bd_core::mechanics::Element::Light, 0.6);
    assert_eq!(g.willpower.upkeep_per_sec(&gun, false), 2.0 * d.willpower.upkeep_per_sec(&gun, false));
    assert_eq!(g.willpower.base_cost(&gun), 2.0 * d.willpower.base_cost(&gun));
}

// ================================================================================ whole-sim hashes

fn ffa_cfg(tuning: CombatTuning) -> SimConfig {
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed: 21 }, seed: 21, check_invariants: true, tuning, ..SimConfig::default() };
    cfg.timings.warmup_secs = 1.0;
    cfg.rules.ko_target = 50;
    cfg
}

/// The party.rs FFA run with casts (6 bots, 40 s): (state hash, event hash).
fn ffa_casts(tuning: CombatTuning) -> (u64, u64) {
    let mut w = World::new(ffa_cfg(tuning));
    let mut brains = bots::add_bots(&mut w, 0, 6, 0.8, 21);
    let mut inputs = Vec::new();
    let mut h = Fnv64::default();
    for _ in 0..(40 * 120) {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        for e in w.tick(&inputs) {
            h.write(format!("{e:?}").as_bytes());
        }
    }
    (w.state_hash(), h.0)
}

fn chaos(seed: u64, tuning: CombatTuning) -> (u64, u64) {
    let rules = random_rules(&mut SimRng::new(seed));
    let mut cfg = ChaosConfig::new(seed, rules);
    cfg.arena = ArenaSource::Flat { size_m: 96.0 };
    cfg.cast_rate = 2.0;
    let mut cw = ChaosWorld::with_config(cfg);
    cw.world.set_tuning(tuning);
    for _ in 0..2400 {
        cw.step();
    }
    (cw.world.state_hash(), cw.world.voxels.content_hash())
}

#[test]
#[ignore = "prints whole-sim hashes (used to verify tuning plumbing is bit-identical)"]
fn print_tuning_hashes() {
    for (name, t) in [("legacy", CombatTuning::legacy()), ("default", CombatTuning::default())] {
        let f = ffa_casts(t.clone());
        let c: Vec<(u64, u64)> = [11u64, 12, 13].iter().map(|s| chaos(*s, t.clone())).collect();
        println!("{name}: ffa_casts state={:#x} events={:#x} chaos={c:x?}", f.0, f.1);
    }
}

/// Hot reload: a world with changed tuning diverges; setting it back is deterministic.
#[test]
fn set_tuning_changes_the_sim_deterministically() {
    let mut t = CombatTuning::default();
    t.ring.blast_damage = 30.0;
    t.ring.blast_knockback = 20.0;
    let a = ffa_casts(t.clone());
    let b = ffa_casts(t);
    assert_eq!(a, b);
    assert_ne!(a, ffa_casts(CombatTuning::default()));
}
