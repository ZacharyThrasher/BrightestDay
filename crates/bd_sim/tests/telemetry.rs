//! Match telemetry (COMBAT.md §5i.6): aggregation is consistent, merging adds up, JSON is sane, and
//! recording never changes the sim.

use bd_sim::bots;
use bd_sim::rng::Fnv64;
use bd_sim::telemetry::{MatchReport, Signature, Telemetry};
use bd_sim::world::{ArenaSource, SimConfig, World};

/// A short 6-bot FFA with casts; returns (report, state hash).
fn run(seed: u64, record: bool) -> (Option<MatchReport>, u64) {
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed }, seed, check_invariants: true, ..SimConfig::default() };
    cfg.timings.warmup_secs = 1.0;
    cfg.rules.ko_target = 50;
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 6, 0.8, seed);
    let mut tel = Telemetry::new(seed);
    let mut inputs = Vec::new();
    let mut h = Fnv64::default();
    for _ in 0..(60 * 120) {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        for e in w.tick(&inputs) {
            h.write(format!("{e:?}").as_bytes());
        }
        if record {
            tel.observe(&w);
        }
    }
    h.write(&w.state_hash().to_le_bytes());
    (record.then(|| tel.finish(&w)), h.0)
}

#[test]
fn telemetry_aggregation_is_consistent() {
    let (r, _) = run(21, true);
    let r = r.unwrap();
    assert_eq!(r.matches, 1);
    assert!(r.secs > 50.0, "{}", r.secs);
    // Every hit on another player is attributed exactly once: to a construct signature or to a
    // non-construct source. Per-player dealt damage adds up to the same total.
    let by_sig: f64 = r.by_signature.values().map(|b| b.damage).sum();
    let by_src: f64 = r.by_source.iter().map(|b| b.damage).sum();
    let dealt: f64 = r.players.iter().map(|p| p.damage_dealt).sum();
    assert!(dealt > 0.0, "bots dealt damage");
    assert!(((by_sig + by_src) - dealt).abs() < 1e-3 * dealt.max(1.0), "sig {by_sig} + src {by_src} vs dealt {dealt}");
    // Forms partition signatures.
    let by_form: f64 = r.by_form.iter().map(|b| b.damage).sum();
    assert!((by_form - by_sig).abs() < 1e-6 * by_sig.max(1.0));
    let casts_form: u32 = r.by_form.iter().map(|b| b.casts).sum();
    let casts_sig: u32 = r.by_signature.values().map(|b| b.casts).sum();
    assert_eq!(casts_form, casts_sig);
    assert!(casts_sig > 0, "bots cast");
    // Willpower: per-player spend equals the signature buckets' cast + upkeep.
    let wp_players: f64 = r.players.iter().map(|p| p.willpower_spent).sum();
    let wp_sigs: f64 = r.by_signature.values().map(|b| b.willpower()).sum();
    assert!((wp_players - wp_sigs).abs() < 1e-3 * wp_sigs.max(1.0), "{wp_players} vs {wp_sigs}");
    // KOs: one cause per KO, one sample per KO, credited KOs ≤ KOs.
    let kos = r.total_kos();
    assert_eq!(r.ko_pct.len() as u32, kos);
    assert!(r.fight_secs.len() as u32 <= r.ko_credited && r.ko_credited <= kos);
    let credited: u32 = r.by_signature.values().map(|b| b.kos).sum::<u32>() + r.by_source.iter().map(|b| b.kos).sum::<u32>();
    assert!(credited <= kos, "{credited} > {kos}");
    let deaths: u32 = r.players.iter().map(|p| p.deaths).sum();
    assert_eq!(deaths, kos);
    assert!(r.recoveries <= r.launches);
    assert!(r.speed_hist.iter().sum::<u64>() > 0);
    // Mechanic buckets see each mechanic of a signature.
    for (sig, b) in &r.by_signature {
        for m in sig.mechs() {
            assert!(r.by_mechanic[m].casts >= b.casts);
        }
    }
    // JSON is well-formed enough to round the braces.
    let j = r.to_json();
    assert_eq!(j.matches('{').count(), j.matches('}').count());
    assert!(j.starts_with('{') && j.ends_with('}'));
}

#[test]
fn reports_merge_additively() {
    let (a, _) = run(21, true);
    let (b, _) = run(22, true);
    let (a, b) = (a.unwrap(), b.unwrap());
    let mut m = MatchReport::default();
    m.merge(&a);
    m.merge(&b);
    assert_eq!(m.matches, 2);
    assert_eq!(m.total_kos(), a.total_kos() + b.total_kos());
    assert_eq!(m.ko_pct.len(), a.ko_pct.len() + b.ko_pct.len());
    let d = |r: &MatchReport| r.by_mechanic.iter().map(|x| x.damage).sum::<f64>();
    assert!((d(&m) - d(&a) - d(&b)).abs() < 1e-6);
    for (k, x) in &a.by_signature {
        assert!(m.by_signature[k].casts >= x.casts);
    }
}

#[test]
fn recording_is_passive_and_deterministic() {
    let (a, ha) = run(23, true);
    let (_, hb) = run(23, false);
    assert_eq!(ha, hb, "telemetry must not change the sim");
    let (a2, _) = run(23, true);
    assert_eq!(a.unwrap().to_json(), a2.unwrap().to_json());
}

#[test]
fn signature_is_word_independent_and_sorted() {
    use bd_core::mechanics::{Element, Form, Mechanic};
    let a = bots::recipe("sword", Form::Sword, &[Mechanic::Blade, Mechanic::Fast], Element::Fire, 1.1);
    let b = bots::recipe("katana", Form::Sword, &[Mechanic::Fast, Mechanic::Blade], Element::Fire, 1.4);
    assert_eq!(Signature::of(&a), Signature::of(&b));
    let s = Signature::of(&a);
    assert_eq!(s.form(), Form::Sword);
    assert_eq!(s.element(), Element::Fire);
    assert_eq!(s.describe(), "Sword+Blade+Fast/Fire");
}
