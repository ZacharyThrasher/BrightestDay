//! CONSTRUCTS rules (`MatchRules::limits`): per-kind caps from the lobby, replace-oldest, 0 = off
//! with the kind fallback (kaiju → big creature → fizzle), the all-kinds total, the map-wide giants
//! cap, presets and the rules round-trip (wire + prefs file).

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::constructs::Slot;
use bd_sim::events::{CastError, GameEvent, GoneReason};
use bd_sim::rules::{COLOSSAL_SCALE, ConstructLimits, MatchRules, ScaleLimit, UNLIMITED, WillpowerMode};
use bd_sim::settings::{GameSettings, Preset};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn duel(limits: ConstructLimits) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    w.cfg.rules.limits = limits;
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

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn aim(w: &World, id: PlayerId) -> Aim {
    Aim::new(w.player(id).unwrap().eye(w.params()), Vec3::new(0.0, -0.4, -1.0))
}

fn wall() -> bd_core::mechanics::Recipe {
    recipe("wall", Form::Wall, &[], Element::Light, 4.0)
}

fn kaiju() -> bd_core::mechanics::Recipe {
    recipe("kaiju", Form::Kaiju, &[], Element::Fire, 14.0)
}

#[test]
fn defaults_are_the_shipped_caps() {
    let l = ConstructLimits::default();
    let r = MatchRules::default();
    for s in [Slot::Held, Slot::Worn, Slot::Structure, Slot::Object, Slot::Creature] {
        assert_eq!(r.slot_limit(s), s.limit(), "{s:?}");
    }
    assert_eq!(r.kaiju_limit(), bd_sim::constructs::KAIJU_LIMIT);
    assert_eq!(r.colossal_limit(), 1);
    assert_eq!((l.total, l.giants_on_map), (UNLIMITED, UNLIMITED));
    assert_eq!((r.total_limit(), r.giants_on_map_limit()), (usize::MAX, usize::MAX));
    // Chaos = the old CapMode::Chaos (×3 structures / objects / creatures / kaiju).
    let c = ConstructLimits::chaos();
    assert_eq!((c.held, c.worn, c.structures, c.objects, c.creatures, c.kaiju, c.colossal), (1, 1, 9, 6, 6, 3, 1));
}

#[test]
fn per_category_caps_come_from_the_rules() {
    let mut w = duel(ConstructLimits { structures: 5, creatures: 1, ..ConstructLimits::default() });
    let a = aim(&w, A);
    let first = w.cast(A, wall(), None, a).unwrap();
    for _ in 0..6 {
        w.cast(A, wall(), None, a).unwrap();
    }
    let ev = run(&mut w, 1);
    assert_eq!(w.constructs.in_slot(A, Slot::Structure).count(), 5);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ConstructGone { id, reason: GoneReason::Replaced, .. } if *id == first)));
    // Creatures: 1 → the second replaces the first.
    let wolf = w.cast(A, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2), None, a).unwrap();
    let bear = w.cast(A, recipe("bear", Form::Quadruped, &[], Element::Light, 1.5), None, a).unwrap();
    run(&mut w, 1);
    let live: Vec<_> = w.constructs.in_slot(A, Slot::Creature).map(|c| c.id).collect();
    assert_eq!(live, vec![bear]);
    assert!(w.constructs.get(wolf).is_none_or(|c| c.gone.is_some()));
    // Unlimited: no replacement.
    w.cfg.rules.limits.structures = UNLIMITED;
    for _ in 0..8 {
        w.cast(A, wall(), None, a).unwrap();
    }
    run(&mut w, 1);
    assert_eq!(w.constructs.in_slot(A, Slot::Structure).count(), 13);
}

#[test]
fn zero_disables_a_kind_and_fizzles_unpaid() {
    let mut w = duel(ConstructLimits { structures: 0, held: 0, ..ConstructLimits::default() });
    w.cfg.rules.willpower = WillpowerMode::Normal;
    w.player_mut(A).unwrap().willpower = 80.0;
    let a = aim(&w, A);
    assert_eq!(w.cast(A, wall(), None, a).unwrap_err(), CastError::Fizzled);
    assert_eq!(w.cast(A, recipe("sword", Form::Sword, &[], Element::Light, 1.0), None, a).unwrap_err(), CastError::Fizzled);
    assert_eq!(w.player(A).unwrap().willpower, 80.0, "a fizzle costs nothing");
    // Through the input path it is a CastFailed event (the client fizzles the shout).
    assert!(w.constructs.owned_by(A).next().is_none());
    // Other kinds still work.
    w.cast(A, recipe("ball", Form::Ball, &[], Element::Light, 0.8), None, a).unwrap();
}

#[test]
fn kaiju_off_becomes_a_big_creature_or_fizzles() {
    let mut w = duel(ConstructLimits { kaiju: 0, ..ConstructLimits::default() });
    let a = aim(&w, A);
    let id = w.cast(A, kaiju(), None, a).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.recipe.form, Form::Biped);
    let spec = bd_sim::constructs::form_spec(Form::Biped);
    assert_eq!(c.recipe.scale, spec.max_scale, "as big as a creature may be");
    // Creatures off too: nothing near it is allowed → fizzle.
    w.cfg.rules.limits.creatures = 0;
    assert_eq!(w.cast(A, kaiju(), None, a).unwrap_err(), CastError::Fizzled);
    // Total 0 = nothing at all.
    w.cfg.rules.limits = ConstructLimits { total: 0, ..ConstructLimits::default() };
    assert_eq!(w.cast(A, wall(), None, a).unwrap_err(), CastError::Fizzled);
}

#[test]
fn kaiju_cap_replaces_the_oldest_kaiju() {
    let mut w = duel(ConstructLimits { kaiju: 2, creatures: 5, ..ConstructLimits::default() });
    let a = aim(&w, A);
    let k1 = w.cast(A, kaiju(), None, a).unwrap();
    let k2 = w.cast(A, kaiju(), None, a).unwrap();
    let k3 = w.cast(A, kaiju(), None, a).unwrap();
    run(&mut w, 1);
    let live: Vec<_> = w.constructs.owned_by(A).filter(|c| c.recipe.form == Form::Kaiju).map(|c| c.id).collect();
    assert_eq!(live, vec![k2, k3]);
    assert!(w.constructs.get(k1).is_none_or(|c| c.gone.is_some()));
}

#[test]
fn total_cap_replaces_the_oldest_of_any_kind() {
    let mut w = duel(ConstructLimits { total: 3, ..ConstructLimits::default() });
    let a = aim(&w, A);
    let w1 = w.cast(A, wall(), None, a).unwrap();
    let b1 = w.cast(A, recipe("ball", Form::Ball, &[], Element::Light, 0.8), None, a).unwrap();
    let s1 = w.cast(A, recipe("sword", Form::Sword, &[], Element::Light, 1.0), None, a).unwrap();
    let c1 = w.cast(A, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2), None, a).unwrap();
    run(&mut w, 1);
    let live: Vec<_> = w.constructs.owned_by(A).filter(|c| c.parent.is_none()).map(|c| c.id).collect();
    assert_eq!(live.len(), 3);
    assert!(!live.contains(&w1), "the oldest (the wall) made room");
    for id in [b1, s1, c1] {
        assert!(live.contains(&id) || w.constructs.get(id).is_none(), "{id:?}");
    }
}

#[test]
fn giants_on_map_cap_is_match_wide() {
    let mut w = duel(ConstructLimits { giants_on_map: 1, kaiju: 3, creatures: 6, ..ConstructLimits::default() });
    let (a, b) = (aim(&w, A), aim(&w, B));
    let ka = w.cast(A, kaiju(), None, a).unwrap();
    run(&mut w, 1);
    // B has no giant to swap out → B's kaiju comes out as a big creature.
    let kb = w.cast(B, kaiju(), None, b).unwrap();
    assert_eq!(w.constructs.get(kb).unwrap().recipe.form, Form::Biped);
    // A already has one → A's new kaiju replaces A's old one (never B's).
    let ka2 = w.cast(A, kaiju(), None, a).unwrap();
    run(&mut w, 1);
    assert_eq!(w.constructs.get(ka2).unwrap().recipe.form, Form::Kaiju);
    assert!(w.constructs.get(ka).is_none_or(|c| c.gone.is_some()));
    let giants = w.constructs.iter().filter(|c| c.gone.is_none() && c.recipe.form == Form::Kaiju).count();
    assert_eq!(giants, 1);
    // 0 giants on the map: every kaiju is a big creature.
    w.cfg.rules.limits.giants_on_map = 0;
    let k = w.cast(A, kaiju(), None, a).unwrap();
    assert_eq!(w.constructs.get(k).unwrap().recipe.form, Form::Biped);
}

#[test]
fn colossal_off_casts_just_big() {
    let mut w = duel(ConstructLimits { colossal: 0, ..ConstructLimits::default() });
    w.cfg.rules.scale = ScaleLimit::NoLimits;
    let a = aim(&w, A);
    let id = w.cast(A, recipe("moon", Form::Ball, &[], Element::Light, 500.0), None, a).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!(!c.colossal && c.recipe.scale < COLOSSAL_SCALE && c.recipe.scale > COLOSSAL_SCALE - 1.0);
    // Colossal cap 2: two moons, the third is refused as shipped.
    w.cfg.rules.limits.colossal = 2;
    w.cast(A, recipe("moon 1", Form::Ball, &[], Element::Light, 500.0), None, a).unwrap();
    w.cast(A, recipe("moon 2", Form::Boulder, &[], Element::Light, 500.0), None, a).unwrap();
    assert_eq!(w.cast(A, recipe("moon 3", Form::Boulder, &[], Element::Light, 500.0), None, a).unwrap_err(), CastError::ColossalLimit);
}

#[test]
fn presets_set_their_caps() {
    assert_eq!(GameSettings::preset(Preset::Classic).rules.limits, ConstructLimits::default());
    assert_eq!(GameSettings::preset(Preset::Party).rules.limits, ConstructLimits::default());
    assert_eq!(GameSettings::preset(Preset::MovementGods).rules.limits, ConstructLimits::default());
    assert_eq!(GameSettings::preset(Preset::Chaos).rules.limits, ConstructLimits::chaos());
}

#[test]
fn rules_round_trip_on_the_wire_and_in_the_prefs_file() {
    let mut s = GameSettings::default();
    s.rules.limits = ConstructLimits { held: 0, worn: 1, structures: 7, objects: UNLIMITED, creatures: 4, kaiju: 0, colossal: 2, total: 12, giants_on_map: 3 };
    // Wire: bincode (bd_net's codec) inside WorldInit.cfg.
    let bytes = bincode::serde::encode_to_vec(&s.rules, bincode::config::standard()).unwrap();
    let (back, _): (MatchRules, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(back, s.rules);
    // Prefs file (RON): round-trips, survives validation, and an old file without `limits` (or
    // with the retired `caps` switch) loads with the shipped caps.
    let text = s.to_ron().unwrap();
    assert_eq!(GameSettings::from_ron(&text).unwrap().validated().rules.limits, s.rules.limits);
    let old = "(rules: (caps: Chaos, ko_target: 12))";
    let g = GameSettings::from_ron(old).unwrap();
    assert_eq!((g.rules.limits, g.rules.ko_target), (ConstructLimits::default(), 12));
}
