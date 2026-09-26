//! Signature casts (COMBAT.md §5j, `bd_sim::signature`): an amplifier over existing mechanics.
//! Domain = a huge Dome (Barrier + Trap) around the caster that traps an enemy inside and
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
    for m in [Mechanic::Barrier, Mechanic::Trap] {
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
    w.apply_construct_hit(ConstructHit {
        target: id,
        source: Some(B),
        damage: integrity + 1.0,
        impulse: Vec3::ZERO,
        blade: false,
        pierce: false,
        blunt: false,
        point: a,
    });
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
fn ultimate_beam_is_a_huge_palm_beam_that_expires() {
    let mut w = duel();
    let t = SignatureTuning::default();
    w.player_mut(A).unwrap().willpower = 100.0;
    let k = recipe("Kamehameha", Form::Gun, &[Mechanic::Beam], Element::Light, 1.0);
    let (id, rec, forming, cost) = cast_sig(&mut w, A, k, SignatureKind::Beam, 1.0);
    assert_eq!(rec.form, Form::Hands, "energy from the palms, never a gun");
    assert_eq!(bd_sim::weapons::archetype_for(&rec), Some(bd_sim::weapons::GunArchetype::HandBeam));
    let spec = w.tuning().spec_for(&rec, bd_sim::weapons::GunArchetype::HandBeam);
    let plain =
        w.tuning().spec_for(&recipe("gun", Form::Hands, &[Mechanic::Beam], Element::Light, 0.6), bd_sim::weapons::GunArchetype::HandBeam);
    assert!(spec.damage > plain.damage * 2.0 && spec.knockback > plain.knockback * 2.0, "{} vs {}", spec.damage, plain.damage);
    assert!(forming >= t.windup - 1e-4 && cost > 40.0);
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.category, FormCategory::Held);
    assert!(c.life <= t.held_life + 1e-3, "an ultimate, not a new permanent weapon: {}", c.life);
    let s = spectacle(&rec, cost, forming, &t).expect("announced");
    assert_eq!(s.kind, SignatureKind::Beam);
}

/// Palm beams fire on RELEASE (a partial charge fires a weaker beam) and by themselves at full
/// charge; a tap below the minimum fires nothing.
#[test]
fn palm_beam_fires_on_release_or_at_full_charge() {
    use bd_sim::weapons::{GunArchetype, WeaponState, gun_spec};
    let spec = gun_spec(GunArchetype::HandBeam);
    // Hold 60 % of the charge, then let go: one shot, at that charge.
    let mut s = WeaponState::new(GunArchetype::HandBeam, &spec);
    let hold = (spec.charge * 0.6 / DT) as usize;
    for _ in 0..hold {
        assert!(s.step(&spec, true, false, DT).is_none(), "no auto-fire mid charge");
    }
    let f = s.step(&spec, false, true, DT).expect("fires on release");
    assert!(f.charge > 0.5 && f.charge < 0.7, "partial charge {}", f.charge);
    // A tap fires nothing.
    let mut s = WeaponState::new(GunArchetype::HandBeam, &spec);
    s.step(&spec, true, false, DT);
    assert!(s.step(&spec, false, true, DT).is_none(), "a tap is not a beam");
    // Keep holding: it fires by itself when full.
    let mut s = WeaponState::new(GunArchetype::HandBeam, &spec);
    let mut shot = None;
    for i in 0..((spec.charge * 1.2 / DT) as usize) {
        if let Some(f) = s.step(&spec, true, false, DT) {
            shot = Some((i, f));
            break;
        }
    }
    let (i, f) = shot.expect("auto-fires at full charge");
    assert!((f.charge - 1.0).abs() < 1e-4 && (i as f32 * DT) >= spec.charge - 2.0 * DT);
}

#[test]
fn summon_and_transformation_grow_past_the_form_clamp() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let (_, rec, ..) = cast_sig(
        &mut w,
        A,
        recipe("Spirit Bomb", Form::Ball, &[Mechanic::Explodes, Mechanic::Heavy], Element::Light, 8.0),
        SignatureKind::Summon,
        0.5,
    );
    assert!(rec.scale > 2.5 + 1e-3, "past the ball clamp: {}", rec.scale);
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let (_, rec, ..) = cast_sig(
        &mut w,
        A,
        recipe("Tensa Zangetsu", Form::Sword, &[Mechanic::Blade, Mechanic::Fast], Element::Shadow, 2.2),
        SignatureKind::Transformation,
        0.0,
    );
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
        let _ = w.cast_signature(
            A,
            recipe("dome", Form::Dome, &[], Element::Fire, 4.0),
            None,
            aim,
            SignatureCast::new(SignatureKind::Domain, 1.0),
        );
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

/// Perf: an 8-bot brawl where every bot cast is a signature (domains + ultimate beams everywhere)
/// vs the same brawl with signatures off. Prints tick p50 / p99 (run with `--release --ignored`).
#[test]
#[ignore = "perf report: cargo test -p bd_sim --release --test signature -- --ignored --nocapture"]
fn perf_brawl_with_signatures() {
    let run = |sig: bool, orbitals: bool| {
        let cfg = SimConfig { arena: ArenaSource::Archipelago { seed: 3 }, ..SimConfig::default() };
        let mut w = World::new(cfg);
        let mut t = w.tuning().clone();
        t.signature.bot_willpower = if sig { 0.0 } else { 1000.0 };
        t.signature.bot_chance = if sig { 1.0 } else { 0.0 };
        w.set_tuning(t);
        let mut brains = bots::add_bots(&mut w, 0, 8, 0.9, 11);
        w.start_live();
        let mut inputs = Vec::new();
        let mut ms = Vec::new();
        let mut domains = 0;
        for k in 0..(60.0 / DT) as usize {
            inputs.clear();
            let t0 = std::time::Instant::now();
            // …and an orbital satellite strike from bot 0 every 6 s.
            if orbitals && k % 720 == 360 {
                let b = brains[0].id;
                if let Some(p) = w.player(b).filter(|p| p.alive) {
                    let eye = p.mv.pos + Vec3::Y * 1.6;
                    let dir = (w.arena.center - eye).normalize_or(Vec3::NEG_Z);
                    let mut r = satellite(&[Mechanic::Homing]);
                    r.scale = 3.0;
                    w.player_mut(b).unwrap().willpower = 100.0;
                    let _ = w.cast(b, r, None, Aim::new(eye, dir));
                }
            }
            bots::drive(&mut w, &mut brains, &mut inputs);
            let evs = w.tick(&inputs);
            ms.push(t0.elapsed().as_secs_f64() * 1e3);
            domains += evs
                .iter()
                .filter(|e| matches!(e, GameEvent::ConstructSpawned { recipe, .. } if recipe.form == Form::Dome && recipe.scale > 10.0))
                .count();
        }
        ms.sort_by(|a, b| a.total_cmp(b));
        (ms[ms.len() / 2], ms[ms.len() * 99 / 100], ms[ms.len() - 1], domains)
    };
    let off = run(false, false);
    let on = run(true, true);
    for (name, a, b) in [("off", false, false), ("sigs", true, false), ("orbitals", false, true), ("both", true, true)] {
        let r = run(a, b);
        println!("  {name}: p50 {:.3} p99 {:.3} max {:.3} ({} domains)", r.0, r.1, r.2, r.3);
    }
    println!(
        "tick ms  signatures off: p50 {:.3} p99 {:.3} max {:.3} | on: p50 {:.3} p99 {:.3} max {:.3} ({} domains)",
        off.0, off.1, off.2, on.0, on.1, on.2, on.3
    );
}

// ================================================================================ orbital strikes

use bd_sim::signature::StrikePhase;

fn satellite(extra: &[Mechanic]) -> Recipe {
    let mut m = vec![Mechanic::Flies, Mechanic::DropFromSky, Mechanic::Beam];
    m.extend_from_slice(extra);
    recipe("satellite death beam", Form::Vehicle, &m, Element::Light, 3.0)
}

/// A casts at the ground `dist` m ahead (B stands there). Returns (construct, ground aim point).
fn cast_orbital(w: &mut World, r: Recipe, dist: f32) -> (ConstructId, Vec3) {
    let a = w.player(A).unwrap().mv.pos;
    let eye = a + Vec3::Y * 1.6;
    let target = a + Vec3::new(0.0, 0.0, -dist);
    w.player_mut(B).unwrap().mv.pos = target;
    w.player_mut(A).unwrap().willpower = 100.0;
    let id = w.cast(A, r, None, Aim::new(eye, (target - eye).normalize())).expect("cast");
    (id, target)
}

fn strikes(evs: &[GameEvent]) -> Vec<(StrikePhase, Vec3)> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::OrbitalStrike { phase, point, .. } => Some((*phase, *point)),
            _ => None,
        })
        .collect()
}

#[test]
fn orbital_hovers_high_and_in_view() {
    let mut w = duel();
    let t = SignatureTuning::default().orbital;
    let (id, target) = cast_orbital(&mut w, satellite(&[]), 25.0);
    let a = w.player(A).unwrap().mv.pos;
    let eye = a + Vec3::Y * 1.6;
    for _ in 0..(2.0 / DT) as usize {
        w.tick(&[]);
    }
    let c = w.constructs.get(id).expect("alive");
    let alt = c.pos.y - target.y;
    let ceiling = w.arena.bounds.max.y - t.ceiling_margin;
    assert!(alt >= t.altitude_min.min(ceiling - target.y) - 0.5 && alt <= t.altitude_max + 0.5, "altitude {alt}");
    assert!(c.pos.y <= ceiling + 0.5, "under the ceiling");
    let flat = Vec3::new(c.pos.x - eye.x, 0.0, c.pos.z - eye.z).length();
    let elev = ((c.pos.y - eye.y) / flat.max(1e-3)).atan().to_degrees();
    assert!(elev <= t.view_elev_deg + 3.0, "in view: {elev:.1}°");
    let lean = Vec3::new(c.pos.x - target.x, 0.0, c.pos.z - target.z).length();
    assert!(lean <= alt + 0.5, "beam within 45° of vertical: {lean:.1} m off at {alt:.1} m");
    let p0 = c.pos;
    for _ in 0..(1.0 / DT) as usize {
        w.tick(&[]);
    }
    assert!((w.constructs.get(id).unwrap().pos - p0).length() < 0.05, "hovers");
}

#[test]
fn telegraph_precedes_damage_then_the_beam_hits() {
    let mut w = duel();
    let t = SignatureTuning::default().orbital;
    let pct0 = w.player(B).unwrap().damage_pct;
    let n0 = w.events().len();
    let _ = cast_orbital(&mut w, satellite(&[Mechanic::Homing]), 20.0);
    let mut evs: Vec<GameEvent> = w.events()[n0..].to_vec();
    let mut first_hit: Option<f32> = None;
    let mut strike_at: Option<f32> = None;
    let mut telegraph_at: Option<f32> = None;
    let mut time = 0.0;
    for _ in 0..(t.telegraph + 1.5) as usize * 120 {
        for (ph, _) in strikes(&evs) {
            match ph {
                StrikePhase::Telegraph => telegraph_at = telegraph_at.or(Some(time)),
                StrikePhase::Strike => strike_at = strike_at.or(Some(time)),
                StrikePhase::End => {}
            }
        }
        if first_hit.is_none() && w.player(B).unwrap().damage_pct > pct0 {
            first_hit = Some(time);
        }
        w.player_mut(B).unwrap().mv.vel = Vec3::ZERO; // stand still in the beam
        evs = w.tick(&[]).to_vec();
        time += DT;
    }
    let (tg, st, hit) = (telegraph_at.expect("marker"), strike_at.expect("strike"), first_hit.expect("B got hit"));
    assert!(tg < 0.02, "the marker shows at once: {tg}");
    assert!(st >= t.telegraph - 0.02 && hit >= st - 1e-3, "telegraph {tg} -> strike {st} -> hit {hit}");
}

#[test]
fn the_beam_slews_at_a_capped_speed_and_can_be_outrun() {
    let mut w = duel();
    let t = SignatureTuning::default().orbital;
    let _ = cast_orbital(&mut w, satellite(&[Mechanic::Homing]), 20.0);
    let mut ends: Vec<Vec3> = Vec::new();
    let b0 = w.player(B).unwrap().mv.pos;
    for k in 0..((t.telegraph + t.strike_secs) / DT) as usize {
        // B sprints sideways at 14 m/s once the strike is live.
        if k as f32 * DT > t.telegraph {
            let p = w.player_mut(B).unwrap();
            p.mv.pos = b0 + Vec3::new((k as f32 * DT - t.telegraph) * 14.0, 0.0, 0.0);
            p.mv.vel = Vec3::ZERO;
        }
        for e in w.tick(&[]) {
            if let GameEvent::Hitscan { owner, end, .. } = e
                && *owner == A
            {
                ends.push(*end);
            }
        }
    }
    assert!(ends.len() >= 10, "{} pulses", ends.len());
    // The ground point moves at ≤ slew (the drawn end point also follows the crater it digs, so
    // judge the whole strike, with a crater's worth of slack).
    let (first, last) = (ends[0], ends[ends.len() - 1]);
    let travelled = Vec3::new(last.x - first.x, 0.0, last.z - first.z).length();
    let secs = (ends.len() - 1) as f32 * t.pulse;
    assert!(travelled <= t.slew * secs + 2.0, "slew: {travelled:.1} m in {secs:.1} s");
    let b = w.player(B).unwrap().mv.pos;
    let last = *ends.last().unwrap();
    assert!(Vec3::new(b.x - last.x, 0.0, b.z - last.z).length() > 5.0, "outran it");
}

#[test]
fn abduction_beam_lifts_instead_of_blasting() {
    let mut w = duel();
    let t = SignatureTuning::default().orbital;
    let pct0 = w.player(B).unwrap().damage_pct;
    let _ = cast_orbital(&mut w, satellite(&[Mechanic::Attract, Mechanic::Homing]), 18.0);
    let y0 = w.player(B).unwrap().mv.pos.y;
    let mut top = y0;
    for _ in 0..((t.telegraph + 1.0) / DT) as usize {
        w.tick(&[]);
        top = top.max(w.player(B).unwrap().mv.pos.y);
    }
    assert!(top > y0 + 2.0, "lifted {:.1} m", top - y0);
    assert_eq!(w.player(B).unwrap().damage_pct, pct0, "abduction doesn't blast");
}

#[test]
fn orbital_cost_is_existing_pricing_times_one_knob() {
    let mut w = duel();
    let t = SignatureTuning::default().orbital;
    let r = satellite(&[]);
    let n0 = w.events().len();
    let _ = cast_orbital(&mut w, r.clone(), 20.0);
    let (_, rec, _, cost) = spawned(&w.events()[n0..]).unwrap();
    let wt = &w.tuning().willpower;
    let plain = wt.base_cost_as(&rec, wt.is_autonomous(&rec));
    assert!((cost - (plain * t.cost_mult).min(100.0)).abs() < 1e-3, "{cost} vs {plain} x {}", t.cost_mult);
}

#[test]
fn orbital_strikes_are_deterministic_and_replicas_agree() {
    let run = || {
        let mut w = duel();
        let _ = cast_orbital(&mut w, satellite(&[Mechanic::Homing, Mechanic::Heavy]), 22.0);
        let mut evs = w.events().to_vec();
        for _ in 0..(8.0 / DT) as usize {
            evs.extend_from_slice(w.tick(&[]));
        }
        (w, evs)
    };
    let (w1, e1) = run();
    let (w2, _) = run();
    assert_eq!(w1.state_hash(), w2.state_hash());
    assert!(strikes(&e1).iter().filter(|s| s.0 == StrikePhase::Strike).count() >= 2, "strike, cool down, strike again");
    // A replica fed the host's events + snapshot sees the hovering emitter where the host has it.
    let mut replica = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() });
    for e in &e1 {
        replica.apply_event(e);
    }
    let snap = w1.snapshot_for(None);
    replica.apply_snapshot(&snap, None);
    for c in w1.constructs.iter().filter(|c| c.gone.is_none()) {
        let r = replica.constructs.get(c.id).expect("replica has it");
        assert!((r.pos - c.pos).length() < 0.05, "{:?} vs {:?}", r.pos, c.pos);
    }
}

#[test]
fn without_homing_the_beam_follows_the_casters_crosshair() {
    let mut w = duel();
    let t = SignatureTuning::default().orbital;
    let (_, target) = cast_orbital(&mut w, satellite(&[]), 20.0);
    // A looks at the ground 12 m to the right of the target.
    let a = w.player(A).unwrap().mv.pos;
    let eye = a + Vec3::Y * 1.6;
    let want = target + Vec3::new(12.0, 0.0, 0.0);
    let d = (want - eye).normalize();
    let input = bd_core::input::PlayerInput { yaw: (-d.x).atan2(-d.z), pitch: d.y.asin(), ..Default::default() };
    let mut last = None;
    for _ in 0..((t.telegraph + t.strike_secs) / DT) as usize {
        for e in w.tick(&[(A, input)]) {
            if let GameEvent::Hitscan { owner, end, .. } = e
                && *owner == A
            {
                last = Some(*end);
            }
        }
    }
    let end = last.expect("beam fired");
    assert!(Vec3::new(end.x - want.x, 0.0, end.z - want.z).length() < 3.0, "tracked the crosshair: {end:?} vs {want:?}");
}

/// Host / prediction parity: the caster moving inside their own domain, holding an ultimate beam,
/// under a hovering orbital emitter, is predicted exactly by a snapshot-fed replica.
#[test]
fn prediction_matches_the_host_around_signatures() {
    use bd_core::input::{PlayerInput, buttons};
    let mut host = duel();
    host.player_mut(A).unwrap().willpower = 100.0;
    let aim = aim_of(&host, A);
    host.cast_signature(A, recipe("dome", Form::Dome, &[], Element::Fire, 4.0), None, aim, SignatureCast::new(SignatureKind::Domain, 1.0))
        .unwrap();
    host.player_mut(A).unwrap().willpower = 100.0;
    host.cast_signature(
        A,
        recipe("Kamehameha", Form::Gun, &[Mechanic::Beam], Element::Light, 1.0),
        None,
        aim,
        SignatureCast::new(SignatureKind::Beam, 1.0),
    )
    .unwrap();
    // An orbital emitter of B's far away (hovering; its strikes land elsewhere).
    host.player_mut(B).unwrap().willpower = 100.0;
    let b_eye = host.player(B).unwrap().mv.pos + Vec3::Y * 1.6;
    host.cast(B, satellite(&[]), None, Aim::new(b_eye, Vec3::new(0.3, -0.4, -1.0).normalize())).unwrap();
    for _ in 0..60 {
        host.tick(&[]);
    }
    let mut client = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() });
    for e in host.construct_manifest() {
        client.apply_event(&e);
    }
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let mut bad = 0;
    for k in 0..240u32 {
        let inp = PlayerInput {
            move_z: -1,
            move_x: if k % 60 < 30 { 1 } else { -1 },
            buttons: if k % 40 == 0 { buttons::JUMP } else { 0 },
            ..Default::default()
        };
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &inp);
        let evs = host.tick(&[(A, inp)]).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let h = host.player(A).unwrap().mv;
        if (h.pos - s.pos).length() > 1e-4 || (h.vel - s.vel).length() > 1e-3 {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "prediction diverged on {bad} ticks");
}

/// The lexicon's exact shape for "SATELLITE DEATH BEAM": a Disc named "satellite", 4 m, Shadow,
/// Dish + Antenna, Beam + Flies + DropFromSky (a held form that flies free).
#[test]
fn lexicon_satellite_shape_strikes_from_orbit() {
    use bd_core::mechanics::Feature;
    let mut w = duel();
    let t = SignatureTuning::default().orbital;
    let mut r = recipe("satellite", Form::Disc, &[Mechanic::Beam, Mechanic::Flies, Mechanic::DropFromSky], Element::Shadow, 4.0);
    r.features = vec![Feature::Dish, Feature::Antenna];
    let pct0 = w.player(B).unwrap().damage_pct;
    let (id, target) = cast_orbital(&mut w, r, 20.0);
    let mut phases = Vec::new();
    for _ in 0..((t.telegraph + 1.5) / DT) as usize {
        w.player_mut(B).unwrap().mv.vel = Vec3::ZERO;
        let evs = w.tick(&[]).to_vec();
        phases.extend(strikes(&evs).into_iter().map(|s| s.0));
    }
    let c = w.constructs.get(id).expect("alive");
    assert!(!c.attached(), "a free-flying satellite, not a disc in the hand");
    assert!(c.pos.y - target.y > t.altitude_min - 1.0, "hovers high: {}", c.pos.y - target.y);
    assert!(phases.contains(&StrikePhase::Strike), "{phases:?}");
    assert!(w.player(B).unwrap().damage_pct > pct0, "the beam came down on B");
}

// ================================================================================ Hollow Purple

use bd_sim::signature::SigFx;

fn hollow_purple() -> Recipe {
    let mut r = recipe("Hollow Purple", Form::Cannon, &[Mechanic::Pierce, Mechanic::Explodes, Mechanic::Heavy], Element::Psychic, 2.0);
    r.name = "Hollow Purple".into();
    r
}

/// A faces -Z; a stone wall 14–18 m ahead; B's wall construct at 22 m; B at 28 m.
fn purple_range() -> (World, Vec3) {
    let mut w = duel();
    let a = w.player(A).unwrap().mv.pos;
    let vs = bd_core::VOXEL_SIZE;
    let v = |p: Vec3| bd_core::IVec3::new((p.x / vs).floor() as i32, (p.y / vs).floor() as i32, (p.z / vs).floor() as i32);
    w.voxels.fill_box(v(a + Vec3::new(-4.0, 0.0, -18.0)), v(a + Vec3::new(4.0, 5.0, -14.0)), 3);
    w.player_mut(B).unwrap().mv.pos = a + Vec3::new(0.0, 0.0, -28.0);
    w.player_mut(B).unwrap().willpower = 100.0;
    let b_eye = a + Vec3::new(0.0, 1.6, -28.0);
    w.cast(
        B,
        recipe("wall", Form::Wall, &[Mechanic::Barrier], Element::Light, 4.0),
        None,
        Aim::new(b_eye, Vec3::new(0.0, -0.3, 1.0).normalize()),
    )
    .unwrap();
    for _ in 0..30 {
        w.tick(&[]);
    }
    w.player_mut(A).unwrap().willpower = 100.0;
    (w, a)
}

fn cast_purple(w: &mut World) -> ConstructId {
    let aim = aim_of(w, A);
    let aim = Aim::new(aim.origin, Vec3::NEG_Z);
    w.cast_signature(A, hollow_purple(), None, aim, SignatureCast::new(SignatureKind::Beam, 1.0).with_merge(Element::Water, Element::Fire))
        .expect("cast")
}

#[test]
fn hollow_purple_winds_up_then_erases_a_tunnel_constructs_and_players() {
    let (mut w, a) = purple_range();
    let wall_id = w.constructs.iter().find(|c| c.owner == B && c.recipe.form == Form::Wall).map(|c| c.id).expect("B's wall");
    let n0 = w.events().len();
    let orb = cast_purple(&mut w);
    let (_, rec, forming, cost) = spawned(&w.events()[n0..]).unwrap();
    assert_eq!(rec.form, Form::Ball);
    assert!(rec.scale >= 3.5, "a LARGE sphere: {}", rec.scale);
    assert!(rec.has(Mechanic::Attract), "suction wake");
    assert!((0.55..=0.8).contains(&forming), "windup {forming}");
    assert!(cost > 50.0 && cost <= 100.0, "most of the bar: {cost}");
    let mut evs: Vec<GameEvent> = w.events()[n0..].to_vec();
    let mut launch_at = None;
    let mut b_launch: Option<f32> = None;
    let mut time = 0.0;
    for _ in 0..(4.0 / DT) as usize {
        let e = w.tick(&[]).to_vec();
        time += DT;
        for x in &e {
            match x {
                GameEvent::SignatureFx { id, fx: SigFx::Launch { .. }, .. } if *id == orb => launch_at = launch_at.or(Some(time)),
                GameEvent::Launched { player, speed, .. } if *player == B => b_launch = b_launch.or(Some(*speed)),
                _ => {}
            }
        }
        evs.extend(e);
    }
    assert!(
        evs.iter().any(|e| matches!(e, GameEvent::SignatureFx { fx: SigFx::Merge { a: Element::Water, b: Element::Fire, .. }, .. })),
        "blue + red merge"
    );
    let launch_at = launch_at.expect("released");
    assert!(launch_at >= forming - 0.02, "released after the windup: {launch_at}");
    // The tunnel: the wall is bored through along the path.
    let vs = bd_core::VOXEL_SIZE;
    let mut air = 0;
    for dz in 14..=18 {
        let p = a + Vec3::new(0.0, 1.5, -(dz as f32) - 0.25);
        let v = bd_core::IVec3::new((p.x / vs).floor() as i32, (p.y / vs).floor() as i32, (p.z / vs).floor() as i32);
        air += (w.voxels.get(v) == 0) as u32;
    }
    assert_eq!(air, 5, "a tunnel through the wall");
    // Everything in the way is erased; B was launched, hard but survivable at 0 %.
    assert!(w.constructs.get(wall_id).is_none_or(|c| c.gone.is_some()), "B's wall erased");
    let s = b_launch.expect("B launched");
    assert!((25.0..=60.0).contains(&s), "launch speed at low %: {s}");
    assert!(evs.iter().any(|e| matches!(e, GameEvent::SignatureFx { id, fx: SigFx::End, .. } if *id == orb)), "it ends");
    assert!(w.constructs.get(orb).is_none_or(|c| c.gone.is_some()));
}

#[test]
fn hollow_purple_knockback_grows_into_a_ko_at_mid_percent() {
    let speed_at = |pct: f32| {
        let (mut w, _) = purple_range();
        w.player_mut(B).unwrap().damage_pct = pct;
        cast_purple(&mut w);
        for _ in 0..(3.0 / DT) as usize {
            for e in w.tick(&[]).to_vec() {
                if let GameEvent::Launched { player, speed, .. } = e
                    && player == B
                {
                    return speed;
                }
            }
        }
        0.0
    };
    let (low, mid, high) = (speed_at(0.0), speed_at(90.0), speed_at(150.0));
    eprintln!("hollow purple launch speed: 0% {low:.1} m/s, 90% {mid:.1} m/s, 150% {high:.1} m/s");
    assert!(mid > low * 1.25, "grows with damage: {low} -> {mid}");
    assert!(mid >= 50.0, "mid % is KO territory: {mid}");
}

#[test]
fn hollow_purple_is_deterministic_and_predicted() {
    use bd_core::input::PlayerInput;
    let run = || {
        let (mut w, _) = purple_range();
        cast_purple(&mut w);
        for _ in 0..(3.5 / DT) as usize {
            w.tick(&[]);
        }
        (w.state_hash(), w.voxels.content_hash())
    };
    assert_eq!(run(), run());
    // The caster strafes during the windup and flight; a replica predicts them exactly.
    let (mut host, _) = purple_range();
    cast_purple(&mut host);
    let mut client = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() });
    for e in host.construct_manifest() {
        client.apply_event(&e);
    }
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let mut bad = 0;
    for k in 0..300u32 {
        let inp = PlayerInput { move_x: if k % 80 < 40 { 1 } else { -1 }, ..Default::default() };
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &inp);
        for e in host.tick(&[(A, inp)]).to_vec() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let h = host.player(A).unwrap().mv;
        if (h.pos - s.pos).length() > 1e-4 || (h.vel - s.vel).length() > 1e-3 {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "prediction diverged on {bad} ticks");
}
