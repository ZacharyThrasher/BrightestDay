//! Saying a power's FORM name makes YOU that form (owner: "if I say gear 5 … I don't get an
//! animation or anything"): a grants key with a tier (`"gomu_gomu@4"`, what the lexicon makes of
//! "gear 5" / "gear fifth" / "gear five") grants the power straight in that form — for ANY kind (a
//! fruit's form comes with the fruit, no pickup to eat), with the Ascend moment (the first time per
//! round: the awakening). Power-ups mode gates the awakening tiers; unaffordable = the plain flare.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Recipe};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::GameEvent;
use bd_sim::pickups::{GateKind, Pickup, PickupPhase};
use bd_sim::powers::{GainHow, PowerEvent, PowerTable};
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);

fn world(powerups: bool) -> World {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    cfg.rules.powerups = powerups;
    let mut w = World::new(cfg);
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

fn say(w: &mut World, name: &str, key: &str) -> Vec<GameEvent> {
    let mut r: Recipe = recipe(name, Form::Ball, &[], Element::Light, 0.4);
    r.grants = Some(key.to_string());
    let p = w.player(A).unwrap();
    let aim = Aim::new(p.eye(w.params()), Vec3::NEG_Z);
    let _ = w.cast(A, r, None, aim);
    let mut ev = Vec::new();
    for _ in 0..3 {
        ev.extend_from_slice(w.tick(&[]));
    }
    ev
}

fn gomu() -> u16 {
    PowerTable::get().index_of("gomu_gomu").unwrap()
}

fn ascends(ev: &[GameEvent]) -> Vec<(Option<u16>, u8, bool)> {
    ev.iter()
        .filter_map(|e| match e {
            GameEvent::Power(PowerEvent::Ascend { player, power, tier, awakening, .. }) if *player == A => Some((*power, *tier, *awakening)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_fresh_player_saying_gear_5_becomes_gear_5_with_the_awakening() {
    let mut w = world(false);
    w.player_mut(A).unwrap().willpower = 100.0;
    assert_eq!(w.player(A).unwrap().power.count(), 0, "fresh");
    let ev = say(&mut w, "Gear Fifth", "gomu_gomu@4");
    let p = &w.player(A).unwrap().power;
    assert_eq!(p.id, Some(gomu()), "the rubber fruit comes with the form");
    assert_eq!(p.tier, 4, "Gear Fifth");
    assert!(w.powers.fruits.is_empty(), "no pickup to go and eat");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Gained { player, power, how: GainHow::Learned }) if *player == A && *power == gomu())));
    assert_eq!(ascends(&ev), vec![(Some(gomu()), 4, true)], "the AWAKENING (the first time this round)");
    assert!(w.player(A).unwrap().willpower < 100.0 - 1.0, "priced like the power");
    // The form is live: Gear Fifth's toon / drums data + its body (the sim's movement mods).
    let d = PowerTable::get().def(gomu()).unwrap();
    assert!(d.ascend_at(4).is_some_and(|a| a.toon && a.music.as_deref() == Some("drums")));
    // Saying it again while holding it: just the form (already there), no new grant.
    let ev = say(&mut w, "Gear Second", "gomu_gomu@1");
    assert_eq!(w.player(A).unwrap().power.tier, 1);
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Gained { .. }))));
    let ev = say(&mut w, "Gear Fifth", "gomu_gomu@4");
    assert_eq!(w.player(A).unwrap().power.tier, 4);
    assert_eq!(ascends(&ev), vec![(Some(gomu()), 4, false)], "the second time is the ordinary Ascend");
}

#[test]
fn technique_forms_still_learn_and_awaken() {
    let mut w = world(false);
    w.player_mut(A).unwrap().willpower = 100.0;
    let ev = say(&mut w, "Super Saiyan Blue", "super_saiyan@4");
    let ssj = PowerTable::get().index_of("super_saiyan").unwrap();
    let p = &w.player(A).unwrap().power;
    assert_eq!((p.id, p.tier), (Some(ssj), 4));
    assert!(ascends(&ev).iter().any(|a| *a == (Some(ssj), 4, true)));
}

#[test]
fn a_fruit_form_sets_another_fruit_down_instead_of_an_overdose() {
    let mut w = world(false);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    assert!(w.grant_power(A, "mera_mera"));
    let ev = say(&mut w, "Gear Fourth", "gomu_gomu@3");
    assert!(w.player(A).unwrap().alive, "no overdose for words");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Overdose { .. }))));
    let p = &w.player(A).unwrap().power;
    assert_eq!((p.id, p.tier), (Some(gomu()), 3));
    let mera = PowerTable::get().index_of("mera_mera").unwrap();
    assert!(p.slot_of(mera).is_none(), "one fruit per body");
    assert!(w.powers.fruits.iter().any(|f| f.power == mera), "the flame fruit lies at your feet");
}

#[test]
fn power_ups_mode_gates_the_awakening_tiers() {
    let mut w = world(true);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    // Gear Second is below the awakening tiers: free.
    let ev = say(&mut w, "Gear Second", "gomu_gomu@1");
    assert_eq!(w.player(A).unwrap().power.tier, 1);
    assert!(!ascends(&ev).is_empty());
    // A fresh player without the Awakening orb: Gear 5 is the plain power-up flare, no form.
    let mut w = world(true);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    let ev = say(&mut w, "Gear Fifth", "gomu_gomu@4");
    assert_eq!(w.player(A).unwrap().power.count(), 0, "no form without the orb");
    assert_eq!(ascends(&ev), vec![(None, 0, false)], "the plain power-up flare");
    // With it: the full form, and the orb is spent.
    let until = w.tick + 10_000;
    let pos = w.player(A).unwrap().mv.pos;
    w.pickups.items.push(Pickup { id: 901, kind: GateKind::Awakening, phase: PickupPhase::Held, pos, holder: Some(A), until });
    w.player_mut(A).unwrap().power.ascend_cd = 0.0;
    let ev = say(&mut w, "Gear Fifth", "gomu_gomu@4");
    let p = &w.player(A).unwrap().power;
    assert_eq!((p.id, p.tier), (Some(gomu()), 4));
    assert!(ascends(&ev).iter().any(|a| a.0 == Some(gomu()) && a.1 == 4));
    assert!(w.pickups.held_by(A).is_none(), "spent");
}

#[test]
fn a_form_you_cannot_afford_is_the_plain_flare() {
    let mut w = world(false);
    w.player_mut(A).unwrap().willpower = 2.0;
    let ev = say(&mut w, "Gear Fifth", "gomu_gomu@4");
    assert_eq!(w.player(A).unwrap().power.count(), 0);
    assert_eq!(ascends(&ev), vec![(None, 0, false)]);
    assert!(w.constructs.iter().all(|c| c.owner != A), "no stray orb construct");
}
