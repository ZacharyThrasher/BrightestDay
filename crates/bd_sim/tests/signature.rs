//! Signature casts (COMBAT.md §5j, `bd_sim::signature`): an amplifier over existing mechanics.
//! Domain = a huge Dome (Barrier + Trap + Homing) around the caster that traps an enemy inside and
//! can be broken; ultimate beams; existing pricing × one multiplier (≤ a full bar) with a plain-cast
//! fallback; a readable windup; plain casts untouched; deterministic; bots use them; replicas agree.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::{ConstructId, DT, PlayerId, Vec3};
use bd_sim::bots::{self, recipe};
use bd_sim::combat::ConstructHit;
use bd_sim::events::GameEvent;
use bd_sim::signature::{SignatureCast, SignatureKind, SignatureTuning, spectacle};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn duel() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
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
    // B stands 10 m in front of A.
    let a = w.player(A).unwrap().mv.pos;
    w.player_mut(B).unwrap().mv.pos = a + Vec3::new(0.0, 0.0, -10.0);
    for _ in 0..30 {
        w.tick(&[]);
    }
    w
}

fn aim_of(w: &World, who: PlayerId) -> Aim {
    let p = w.player(who).unwrap();
    Aim::new(p.mv.pos + Vec3::Y * 1.6, Vec3::new(0.0, -0.2, -1.0).normalize())
}

fn spawned(evs: &[GameEvent]) -> Option<(ConstructId, Recipe, f32, f32)> {
    evs.iter().find_map(|e| match e {
        GameEvent::ConstructSpawned { id, recipe, forming, cost, .. } => Some((*id, (**recipe).clone(), *forming, *cost)),
        _ => None,
    })
}

fn cast_sig(w: &mut World, who: PlayerId, r: Recipe, kind: SignatureKind, power: f32) -> (ConstructId, Recipe, f32, f32) {
    let aim = aim_of(w, who);
    let n0 = w.events().len();
    let id = w.cast_signature(who, r, None, aim, SignatureCast::new(kind, power)).expect("signature cast");
    let evs = w.events()[n0..].to_vec();
    let (sid, rec, forming, cost) = spawned(&evs).expect("spawn event");
    assert_eq!(sid, id);
    (id, rec, forming, cost)
}

#[test]
fn domain_erupts_traps_an_enemy_and_can_be_broken() {
    let mut w = duel();
    let t = SignatureTuning::default();
    w.player_mut(A).unwrap().willpower = 100.0;
    let fire = recipe("coffin of the iron mountain", Form::Dome, &[Mechanic::Barrier], Element::Fire, 4.0);
    let (id, rec, forming, cost) = cast_sig(&mut w, A, fire, SignatureKind::Domain, 0.5);
    // Existing mechanics only, at domain scale, with a readable windup.
    assert_eq!(rec.form, Form::Dome);
    assert_eq!(rec.element, Element::Fire);
    for m in [Mechanic::Barrier, Mechanic::Trap, Mechanic::Homing] {
        assert!(rec.has(m), "{m:?}");
    }
    assert!((rec.scale - (t.domain_radius + 0.5 * t.domain_radius_shout)).abs() < 1e-3, "radius {}", rec.scale);
    assert!(forming >= t.windup - 1e-4, "windup {forming}");
    // Most of the bar: the existing cost × cost_mult (never more than the bar).
    assert!(cost > 50.0 && cost <= 100.0, "cost {cost}");
    let wp = w.player(A).unwrap().willpower;
    assert!((wp - (100.0 - cost)).abs() < 0.5, "paid {wp}");
    // Windup: not live yet, then live.
    for _ in 0..((forming * 0.5 / DT) as usize) {
        w.tick(&[]);
    }
    assert!(!w.constructs.get(id).unwrap().active(), "still forming");
    let b0 = w.player(B).unwrap().mv.pos;
    let mut lifted = false;
    for _ in 0..((forming * 0.5 + 1.2) / DT) as usize {
        w.tick(&[]);
        let b = w.player(B).unwrap().mv.pos;
        lifted |= b.y > b0.y + 1.0;
    }
    assert!(w.constructs.get(id).unwrap().active());
    // Trap snapped shut on B (inside the radius): yanked off their feet toward the caster.
    let a = w.player(A).unwrap().mv.pos;
    let b = w.player(B).unwrap().mv.pos;
    assert!(lifted, "B was trapped and lifted (b0={b0:?} b={b:?})");
    let d0 = Vec3::new(b0.x - a.x, 0.0, b0.z - a.z).length();
    let d1 = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length();
    assert!(d1 < d0 - 2.0, "pulled in: {d0:.1} -> {d1:.1}");
    // The dome stays wrapped around its caster.
    let c = w.constructs.get(id).unwrap();
    assert!(Vec3::new(c.pos.x - a.x, 0.0, c.pos.z - a.z).length() < 1.0, "dome at {:?}, caster at {a:?}", c.pos);
    // Enough damage breaks it (from inside or outside: integrity is all that matters).
    let integrity = c.integrity;
    assert!(integrity > 150.0, "a domain is sturdy: {integrity}");
    w.apply_construct_hit(ConstructHit { target: id, source: Some(B), damage: integrity + 1.0, impulse: Vec3::ZERO, blade: false, pierce: false, blunt: false, point: a });
    for _ in 0..10 {
        w.tick(&[]);
    }
    assert!(w.constructs.get(id).is_none_or(|c| c.gone.is_some()), "shattered");
}

#[test]
fn domain_expires_after_its_lifetime() {
    let mut w = duel();
    let t = SignatureTuning::default();
    w.player_mut(A).unwrap().willpower = 100.0;
    let (id, ..) = cast_sig(&mut w, A, recipe("dome", Form::Dome, &[Mechanic::Barrier], Element::Psychic, 4.0), SignatureKind::Domain, 0.0);
    for _ in 0..((t.domain_life - 0.3) / DT) as usize {
        w.tick(&[]);
    }
    assert!(w.constructs.get(id).is_some_and(|c| c.gone.is_none()), "alive until the end");
    for _ in 0..(0.6 / DT) as usize {
        w.tick(&[]);
    }
    assert!(w.constructs.get(id).is_none_or(|c| c.gone.is_some()), "expired");
}

#[test]
fn unaffordable_signature_casts_the_plain_phrase() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 30.0;
    let aim = aim_of(&w, A);
    let n0 = w.events().len();
    let r = recipe("dome", Form::Dome, &[Mechanic::Barrier], Element::Fire, 4.0);
    w.cast_signature(A, r, None, aim, SignatureCast::new(SignatureKind::Domain, 1.0)).expect("the ring never says no");
    let (_, rec, forming, cost) = spawned(&w.events()[n0..]).unwrap();
    assert!(rec.scale <= 5.0 + 1e-4 && !rec.has(Mechanic::Trap), "plain dome: {rec:?}");
    assert!(cost < 30.0 && forming < 0.5);
    assert!(!w.events()[n0..].iter().any(|e| matches!(e, GameEvent::CastFailed { .. })));
}

#[test]
fn plain_casts_are_untouched() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = aim_of(&w, A);
    let n0 = w.events().len();
    w.cast(A, recipe("dome", Form::Dome, &[Mechanic::Barrier], Element::Fire, 40.0), None, aim).unwrap();
    let (_, rec, forming, cost) = spawned(&w.events()[n0..]).unwrap();
    assert!((rec.scale - 5.0).abs() < 1e-4, "form clamp still applies: {}", rec.scale);
    assert!((forming - 0.25).abs() < 1e-4);
    let wt = &w.tuning().willpower;
    assert!((cost - wt.base_cost_as(&rec, false)).abs() < 1e-3);
}

#[test]
fn ultimate_beam_is_a_huge_railgun_that_expires() {
    let mut w = duel();
    let t = SignatureTuning::default();
    w.player_mut(A).unwrap().willpower = 100.0;
    let k = recipe("Kamehameha", Form::Gun, &[Mechanic::Beam], Element::Light, 1.0);
    let (id, rec, forming, cost) = cast_sig(&mut w, A, k, SignatureKind::Beam, 1.0);
    assert_eq!(rec.form, Form::Gun);
    assert_eq!(bd_sim::weapons::archetype_for(&rec), Some(bd_sim::weapons::GunArchetype::Railgun));
    let spec = w.tuning().spec_for(&rec, bd_sim::weapons::GunArchetype::Railgun);
    let plain = w.tuning().spec_for(&recipe("gun", Form::Gun, &[Mechanic::Beam], Element::Light, 0.6), bd_sim::weapons::GunArchetype::Railgun);
    assert!(spec.damage > plain.damage * 2.0 && spec.knockback > plain.knockback * 2.0, "{} vs {}", spec.damage, plain.damage);
    assert!(forming >= t.windup - 1e-4 && cost > 40.0);
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.category, FormCategory::Held);
    assert!(c.life <= t.held_life + 1e-3, "an ultimate, not a new permanent weapon: {}", c.life);
    let s = spectacle(&rec, cost, forming, &t).expect("announced");
    assert_eq!(s.kind, SignatureKind::Beam);
}

#[test]
fn summon_and_transformation_grow_past_the_form_clamp() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let (_, rec, ..) = cast_sig(&mut w, A, recipe("Spirit Bomb", Form::Ball, &[Mechanic::Explodes, Mechanic::Heavy], Element::Light, 8.0), SignatureKind::Summon, 0.5);
    assert!(rec.scale > 2.5 + 1e-3, "past the ball clamp: {}", rec.scale);
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let (_, rec, ..) = cast_sig(&mut w, A, recipe("Tensa Zangetsu", Form::Sword, &[Mechanic::Blade, Mechanic::Fast], Element::Shadow, 2.2), SignatureKind::Transformation, 0.0);
    assert!(rec.scale > 2.5 && rec.has(Mechanic::Heavy), "{rec:?}");
}

/// Two identical runs with signature casts and bots (who shout signatures too) agree bit for bit,
/// and a replica built from the events sees the same (lifted) constructs.
#[test]
fn deterministic_with_bots_and_replicas_agree() {
    let run = || {
        let mut w = duel();
        let mut t = w.tuning().clone();
        t.signature.bot_willpower = 0.0;
        t.signature.bot_chance = 1.0;
        w.set_tuning(t);
        let mut brains = bots::add_bots(&mut w, 3, 4, 0.8, 5);
        w.player_mut(A).unwrap().willpower = 100.0;
        let aim = aim_of(&w, A);
        let _ = w.cast_signature(A, recipe("dome", Form::Dome, &[], Element::Fire, 4.0), None, aim, SignatureCast::new(SignatureKind::Domain, 1.0));
        let mut inputs = Vec::new();
        let mut events: Vec<GameEvent> = w.events().to_vec();
        let mut sigs = 0;
        for _ in 0..(25.0 / DT) as usize {
            inputs.clear();
            bots::drive(&mut w, &mut brains, &mut inputs);
            let evs = w.tick(&inputs).to_vec();
            for e in &evs {
                if let GameEvent::ConstructSpawned { recipe, .. } = e {
                    let spec = bd_sim::constructs::forms::form_spec(recipe.form);
                    if recipe.scale > spec.max_scale + 1e-3 || recipe.name.starts_with("Ultimate") || recipe.name == "Domain Expansion" {
                        sigs += 1;
                    }
                }
            }
            events.extend(evs);
        }
        (w.state_hash(), sigs, events)
    };
    let (h1, s1, e1) = run();
    let (h2, s2, _) = run();
    assert_eq!(h1, h2, "deterministic");
    assert_eq!(s1, s2);
    assert!(s1 >= 2, "bots shouted signatures: {s1}");
    // Replica: spawn events carry the amplified recipe as-is.
    let mut replica = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() });
    for e in e1.iter().filter(|e| matches!(e, GameEvent::ConstructSpawned { .. })) {
        replica.apply_event(e);
        if let GameEvent::ConstructSpawned { id, recipe, .. } = e {
            let c = replica.constructs.get(*id).expect("replica spawned it");
            assert_eq!(c.recipe.scale, recipe.scale);
        }
    }
}
