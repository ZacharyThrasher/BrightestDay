//! Domain expansions (`signature::domain`, COMBAT.md §5j): the expansion you can dash out of, the
//! barrier (both ways, HP), sure hit, a pulse per element, burnout, clash, simple domain, determinism
//! and prediction parity.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{ConstructId, DT, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::ConstructHit;
use bd_sim::events::GameEvent;
use bd_sim::signature::domain::{DomainEvent, DomainLook};
use bd_sim::signature::{SignatureCast, SignatureKind};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const PI: f32 = std::f32::consts::PI;

/// A and B on flat ground, B `dist` m in front of A (-Z).
fn duel(dist: f32) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 200.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    for _ in 0..60 {
        w.tick(&[]);
    }
    let a = w.player(A).unwrap().mv.pos;
    w.player_mut(B).unwrap().mv.pos = a + Vec3::new(0.0, 0.0, -dist);
    for _ in 0..30 {
        w.tick(&[]);
    }
    for p in &mut w.players {
        p.willpower = 100.0;
        p.spawn_shield = 0.0;
    }
    w
}

fn domain_recipe(element: Element, mechs: &[Mechanic]) -> Recipe {
    recipe("domain", Form::Dome, mechs, element, 4.0)
}

fn cast_domain(w: &mut World, who: PlayerId, r: Recipe) -> ConstructId {
    let p = w.player(who).unwrap();
    let aim = Aim::new(p.mv.pos + Vec3::Y * 1.6, Vec3::NEG_Z);
    w.player_mut(who).unwrap().willpower = 100.0;
    w.cast_signature(who, r, None, aim, SignatureCast::new(SignatureKind::Domain, 0.0)).expect("domain")
}

fn domain_events(evs: &[GameEvent]) -> Vec<DomainEvent> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::Domain(d) => Some(*d),
            _ => None,
        })
        .collect()
}

fn run(w: &mut World, secs: f32, mut f: impl FnMut(&World) -> Vec<(PlayerId, PlayerInput)>) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..(secs / DT) as usize {
        let i = f(w);
        all.extend(w.tick(&i).to_vec());
    }
    all
}

fn radius(w: &World, id: ConstructId) -> (Vec3, f32) {
    let c = w.constructs.get(id).expect("domain");
    bd_sim::signature::domain::sphere(c)
}

fn walk(yaw: f32) -> PlayerInput {
    PlayerInput { yaw, move_z: 1, ..Default::default() }
}

#[test]
fn you_can_dash_out_during_the_expansion_but_not_after_it_seals() {
    // B starts just inside the edge and runs out while it closes: free.
    let mut w = duel(14.0);
    let id = cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    let (c, r) = radius(&w, id);
    assert!(r >= 16.0);
    run(&mut w, 1.3, |_| vec![(B, walk(0.0))]);
    let b = w.player(B).unwrap().mv.pos;
    assert!(b.distance(c) > r, "B got out before it sealed ({:.1} m, radius {r})", b.distance(c));

    // Sealed: from inside B can't leave …
    let mut w = duel(10.0);
    let id = cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    let evs = run(&mut w, 1.7, |_| vec![]);
    assert!(domain_events(&evs).iter().any(|e| matches!(e, DomainEvent::Sealed { .. })), "sealed");
    let (c, r) = radius(&w, id);
    run(&mut w, 2.5, |_| vec![(B, walk(0.0))]);
    let b = w.player(B).unwrap().mv.pos;
    assert!(b.distance(c) < r, "held inside: {:.1} m vs radius {r}", b.distance(c));

    // … and from outside B can't get in.
    let mut w = duel(22.0);
    let id = cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    run(&mut w, 1.7, |_| vec![]);
    let (c, r) = radius(&w, id);
    run(&mut w, 2.5, |_| vec![(B, walk(PI))]);
    let b = w.player(B).unwrap().mv.pos;
    assert!(b.distance(c) > r, "kept out: {:.1} m vs radius {r}", b.distance(c));
}

#[test]
fn projectiles_stop_at_the_barrier_and_it_has_hp() {
    let mut w = duel(26.0);
    let id = cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    run(&mut w, 1.7, |_| vec![]);
    let hp0 = w.constructs.get(id).unwrap().integrity;
    let pct0 = w.player(A).unwrap().damage_pct;
    // B blasts at A from outside (empty-hand ring blasts).
    run(&mut w, 2.0, |_| vec![(B, PlayerInput { yaw: PI, buttons: buttons::PRIMARY, ..Default::default() })]);
    assert!(w.player(A).unwrap().damage_pct <= pct0 + 1e-3, "nothing got through to A");
    let hp1 = w.constructs.get(id).map_or(0.0, |c| c.integrity);
    assert!(hp1 < hp0, "the barrier took them: {hp0} -> {hp1}");
    // Enough damage shatters it; its caster burns out.
    let (c, _) = radius(&w, id);
    w.apply_construct_hit(ConstructHit {
        target: id,
        source: Some(B),
        damage: 1.0e6,
        impulse: Vec3::ZERO,
        blade: false,
        pierce: false,
        blunt: false,
        point: c,
    });
    let evs = run(&mut w, 0.2, |_| vec![]);
    let d = domain_events(&evs);
    assert!(d.iter().any(|e| matches!(e, DomainEvent::Shatter { .. })), "{d:?}");
    assert!(d.iter().any(|e| matches!(e, DomainEvent::Burnout { player, .. } if *player == A)));
}

#[test]
fn inside_the_caster_cant_miss_and_casts_cheaper() {
    let mut w = duel(10.0);
    let base = w.tuning().assist.aim.proj_cone_deg;
    assert_eq!(bd_sim::signature::domain::aim(&w, A).proj_cone_deg, base);
    cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    run(&mut w, 1.7, |_| vec![]);
    let sure = bd_sim::signature::domain::aim(&w, A);
    assert!(sure.proj_cone_deg > base * 4.0 && sure.hitscan_beam_deg > 5.0, "maxed assist");
    assert_eq!(bd_sim::signature::domain::aim(&w, B).proj_cone_deg, base, "not for anyone else");
    // A cast inside costs fewer sparks.
    // (Pricey enough that halving it shows through the whole-spark rounding.)
    let r = recipe("bear", Form::Quadruped, &[Mechanic::Charge, Mechanic::Heavy], Element::Light, 3.0);
    let aim = Aim::new(w.player(A).unwrap().mv.pos + Vec3::Y * 1.6, Vec3::NEG_Z);
    w.player_mut(A).unwrap().willpower = 100.0;
    w.cast(A, r.clone(), None, aim).unwrap();
    let inside = 100.0 - w.player(A).unwrap().willpower;
    let mut plain = duel(10.0);
    plain.player_mut(A).unwrap().willpower = 100.0;
    plain.cast(A, r, None, aim).unwrap();
    let outside = 100.0 - plain.player(A).unwrap().willpower;
    assert!(inside < outside, "cheaper inside: {inside} vs {outside}");
    // A ring blast from inside homes onto B even aimed well off.
    let pct0 = w.player(B).unwrap().damage_pct;
    run(&mut w, 1.2, |_| vec![(A, PlayerInput { yaw: 0.9, buttons: buttons::PRIMARY, ..Default::default() })]);
    assert!(w.player(B).unwrap().damage_pct > pct0, "sure hit");
}

#[test]
fn every_look_pulses_its_element_telegraphed_never_at_the_caster() {
    let looks: [(Element, &[Mechanic], DomainLook); 12] = [
        (Element::Shadow, &[Mechanic::Blade], DomainLook::Blade),
        (Element::Psychic, &[], DomainLook::Void),
        (Element::Fire, &[], DomainLook::Fire),
        (Element::Water, &[], DomainLook::Water),
        (Element::Electric, &[], DomainLook::Electric),
        (Element::Shadow, &[], DomainLook::Shadow),
        (Element::Ice, &[], DomainLook::Ice),
        (Element::Light, &[], DomainLook::Light),
        (Element::Earth, &[], DomainLook::Earth),
        (Element::Wind, &[], DomainLook::Wind),
        (Element::Poison, &[], DomainLook::Poison),
        (Element::Atomic, &[], DomainLook::Atomic),
    ];
    for (element, mechs, look) in looks {
        let mut w = duel(8.0);
        cast_domain(&mut w, A, domain_recipe(element, mechs));
        let (pa, pb) = (w.player(A).unwrap().damage_pct, w.player(B).unwrap().damage_pct);
        let evs = run(&mut w, 1.7 + 2.0, |_| vec![]);
        let d = domain_events(&evs);
        let tele = d.iter().position(|e| matches!(e, DomainEvent::Telegraph { .. })).unwrap_or_else(|| panic!("{look:?}: telegraphed"));
        let strike = d
            .iter()
            .position(|e| matches!(e, DomainEvent::Strike { victim: Some(v), .. } if *v == B))
            .unwrap_or_else(|| panic!("{look:?}: strikes B"));
        assert!(tele < strike, "{look:?}: the marker comes first");
        assert!(d.iter().all(|e| !matches!(e, DomainEvent::Strike { victim: Some(v), .. } if *v == A)), "{look:?}: never the caster");
        assert!(d.iter().any(|e| matches!(e, DomainEvent::Sealed { look: l, .. } if *l == look)), "{look:?}");
        let b = w.player(B).unwrap();
        assert!(b.damage_pct > pb || b.status.active(bd_sim::status::StatusKind::Chill), "{look:?}: B felt it");
        assert!((w.player(A).unwrap().damage_pct - pa).abs() < 1e-3, "{look:?}: A untouched");
    }
}

#[test]
fn stepping_off_the_marker_dodges_a_pulse() {
    let mut w = duel(8.0);
    cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    run(&mut w, 1.7, |_| vec![]);
    let pb = w.player(B).unwrap().damage_pct;
    // B sprints sideways the moment a marker appears.
    let mut dodging = false;
    for _ in 0..(3.0 / DT) as usize {
        let inp = if dodging {
            vec![(B, PlayerInput { yaw: PI * 0.5, move_z: 1, buttons: buttons::BOOST, ..Default::default() })]
        } else {
            vec![]
        };
        let evs = w.tick(&inp).to_vec();
        dodging |= domain_events(&evs).iter().any(|e| matches!(e, DomainEvent::Telegraph { .. }));
    }
    assert!(dodging);
    assert!(w.player(B).unwrap().damage_pct - pb < 7.0, "most pulses dodged: {}", w.player(B).unwrap().damage_pct - pb);
}

#[test]
fn burnout_after_the_domain_ends() {
    let mut w = duel(30.0);
    let id = cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    let life = w.tuning().signature.domain_life;
    run(&mut w, life + 0.3, |_| vec![]);
    assert!(w.constructs.get(id).is_none_or(|c| c.gone.is_some()), "it ends");
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = Aim::new(w.player(A).unwrap().mv.pos + Vec3::Y * 1.6, Vec3::NEG_Z);
    assert!(w.cast(A, recipe("sword", Form::Sword, &[], Element::Light, 1.0), None, aim).is_err(), "burnt out");
    let burnout = w.tuning().signature.domain.burnout;
    run(&mut w, burnout + 0.2, |_| vec![]);
    assert!(w.cast(A, recipe("sword", Form::Sword, &[], Element::Light, 1.0), None, aim).is_ok(), "back");
}

#[test]
fn domains_clash_the_pouring_side_wins_the_loser_shatters() {
    let mut w = duel(26.0);
    let da = cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    let db = cast_domain(&mut w, B, domain_recipe(Element::Psychic, &[]));
    run(&mut w, 1.7, |_| vec![]);
    let r_b0 = radius(&w, db).1;
    let mut clashed = false;
    let mut shattered = None;
    for _ in 0..(6.0 / DT) as usize {
        for p in &mut w.players {
            p.willpower = 100.0;
        }
        let evs = w.tick(&[(A, PlayerInput { buttons: buttons::PRIMARY, ..Default::default() })]).to_vec();
        for e in domain_events(&evs) {
            match e {
                DomainEvent::Clash { losing, .. } => clashed |= losing == db,
                DomainEvent::Shatter { id, .. } => shattered = shattered.or(Some(id)),
                _ => {}
            }
        }
        if shattered.is_some() {
            break;
        }
    }
    assert!(clashed, "they clash, B's side gives way");
    assert!(
        shattered == Some(db) || w.constructs.get(db).is_some_and(|c| bd_sim::signature::domain::sphere(c).1 < r_b0 - 3.0),
        "B's domain loses"
    );
    assert!(w.constructs.get(da).is_some_and(|c| c.gone.is_none()), "A's stands");
}

#[test]
fn a_simple_domain_is_immune_to_the_pulses() {
    let mut w = duel(8.0);
    cast_domain(&mut w, A, domain_recipe(Element::Fire, &[]));
    run(&mut w, 1.6, |_| vec![]);
    let b = w.player(B).unwrap().mv.pos;
    let guard = recipe("simple domain", Form::Dome, &[Mechanic::Guard], Element::Light, 2.0);
    w.cast(B, guard, None, Aim::new(b + Vec3::Y * 1.6, Vec3::Z)).unwrap();
    let pb = w.player(B).unwrap().damage_pct;
    let evs = run(&mut w, 2.5, |_| vec![]);
    let d = domain_events(&evs);
    assert!(d.iter().any(|e| matches!(e, DomainEvent::Guard { player, .. } if *player == B)));
    assert!(!d.iter().any(|e| matches!(e, DomainEvent::Strike { victim: Some(v), .. } if *v == B)), "no sure hit on B");
    assert!(w.player(B).unwrap().damage_pct - pb < 1e-3);
}

#[test]
fn domains_are_deterministic_and_the_barrier_is_predicted() {
    let once = || {
        let mut w = duel(10.0);
        cast_domain(&mut w, A, domain_recipe(Element::Electric, &[]));
        run(&mut w, 4.0, |_| vec![(B, walk(0.0))]);
        (w.state_hash(), w.voxels.content_hash())
    };
    assert_eq!(once(), once());
    // B (a client) runs into the barrier from inside; the replica predicts them exactly.
    let mut host = duel(10.0);
    // (Pulses off: a hit is an outside force no prediction knows about — this is the wall alone.)
    let mut t = host.tuning().clone();
    t.signature.domain.pulse_every = 20.0;
    host.set_tuning(t);
    cast_domain(&mut host, A, domain_recipe(Element::Fire, &[]));
    for _ in 0..(1.7 / DT) as usize {
        host.tick(&[]);
    }
    let mut client = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 200.0 }, check_invariants: true, ..SimConfig::default() });
    for e in host.construct_manifest() {
        client.apply_event(&e);
    }
    client.apply_snapshot(&host.snapshot_for(Some(B)), Some(B));
    let mut bad = 0;
    for k in 0..300u32 {
        let i = PlayerInput { yaw: if k < 150 { 0.0 } else { 0.4 }, move_z: 1, ..Default::default() };
        let mut s = client.player(B).unwrap().mv;
        client.predict_player(B, &mut s, &i);
        for e in host.tick(&[(B, i)]).to_vec() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(B)), Some(B));
        let h = host.player(B).unwrap().mv;
        if (h.pos - s.pos).length() > 1e-4 || (h.vel - s.vel).length() > 1e-3 {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "prediction diverged on {bad} ticks");
}

/// Tick cost (`--release -- --ignored --nocapture`): the same duel idle vs. two clashing domains with
/// pulses, the barrier and a player running into it.
#[test]
#[ignore = "perf probe (release build)"]
fn domain_tick_cost() {
    let run = |domains: bool| {
        let mut w = duel(26.0);
        if domains {
            cast_domain(&mut w, A, domain_recipe(Element::Electric, &[]));
            cast_domain(&mut w, B, domain_recipe(Element::Psychic, &[]));
        }
        let mut times = Vec::new();
        for _ in 0..1080u32 {
            let t0 = std::time::Instant::now();
            w.tick(&[(B, walk(0.0))]);
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
            for p in &mut w.players {
                p.willpower = 100.0;
            }
        }
        times.sort_by(|a, b| a.total_cmp(b));
        (times[times.len() / 2], times[times.len() * 99 / 100], times[times.len() - 1])
    };
    let off = run(false);
    let on = run(true);
    eprintln!("DOMAIN tick ms (p50, p99, max): idle {off:?}, two domains clashing + pulses {on:?}");
}
