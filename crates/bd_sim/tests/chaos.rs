//! Short chaos soak inside the test suite (the long one is `examples/soak.rs`): random recipes from
//! the whole combination space, portals, links, fields, resizes, joins/leaves — no panic, invariants
//! every tick, deterministic.

use bd_core::mechanics::{Form, Mechanic};
use bd_sim::fuzz::{ChaosConfig, ChaosWorld, FORMS, MECHANICS, describe, random_recipe, random_rules};
use bd_sim::rng::SimRng;
use bd_sim::world::ArenaSource;

fn chaos(seed: u64, ticks: u32) -> (u64, u64, usize, u64) {
    let rules = random_rules(&mut SimRng::new(seed));
    let mut cfg = ChaosConfig::new(seed, rules);
    cfg.arena = ArenaSource::Flat { size_m: 96.0 };
    cfg.cast_rate = 2.0;
    let mut cw = ChaosWorld::with_config(cfg);
    for _ in 0..ticks {
        let r = cw.step();
        assert!(r.invariant_error.is_none(), "seed {seed} tick {}: {:?}", r.tick, r.invariant_error);
    }
    (cw.world.state_hash(), cw.world.voxels.content_hash(), cw.stats.failures.len(), cw.stats.casts_ok)
}

#[test]
fn chaos_worlds_hold_invariants_and_are_deterministic() {
    for seed in [11u64, 12, 13] {
        let a = chaos(seed, 2400);
        let b = chaos(seed, 2400);
        assert_eq!(a.2, 0);
        assert!(a.3 > 5, "some casts succeeded");
        assert_eq!(a, b, "seed {seed}");
    }
}

/// The fuzz lists must cover every variant (compile-time exhaustive matches keep them honest).
#[test]
fn fuzz_lists_cover_every_form_and_mechanic() {
    fn form_ok(f: Form) -> bool {
        use Form::*;
        match f {
            Hammer | Sword | Axe | Spear | Shield | Gauntlet | Whip | Gun | Bow | Cannon | Staff | Disc | Wings | Parachute | Jetpack | Cape
            | Biped | Kaiju | Quadruped | Serpent | Bird | Fish | Arachnid | Insect | Blob | Wall | Ramp | Pillar | Dome | Cage | Platform
            | Trampoline | Ball | Anvil | Bomb | Boulder | Rocket | Vehicle | Blob0 => FORMS.contains(&f),
        }
    }
    fn mech_ok(m: Mechanic) -> bool {
        use Mechanic::*;
        match m {
            Blunt | Blade | Pierce | Guard | Returning | Ricochet | Shoots | Beam | Explodes | Grab | Pull | SlowFall | Glide | Thrust
            | Bounce | Launch | Barrier | Trap | Homing | ArticulatedTail | Flies | Charge | Stomp | Breath | Swarm | Spin | DropFromSky
            | Sticky | Fast | Heavy | Constrict | Walks | Piloted | Portal | Attract | Repel | Shrink | Grow | Link => MECHANICS.contains(&m),
        }
    }
    assert!(FORMS.iter().all(|f| form_ok(*f)));
    assert!(MECHANICS.iter().all(|m| mech_ok(*m)));
    let mut rng = SimRng::new(3);
    for _ in 0..2000 {
        let r = random_recipe(&mut rng, 2);
        let v = bd_sim::willpower::validate_recipe(r, &bd_sim::rules::MatchRules::default());
        assert!(v.scale.is_finite() && v.mass.is_finite() && v.count >= 1);
        assert!(v.payload.as_ref().is_none_or(|p| p.payload.is_none()));
        assert!(!describe(&v).is_empty());
    }
}
