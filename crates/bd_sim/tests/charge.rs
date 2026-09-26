//! Charging (hold to power up, `bd_sim::charge`): refill rate + ramp, hit interrupts, can't act,
//! feet planted (and predicted identically), determinism, bots, replication of the flag.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form};
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::bots::{self, recipe};
use bd_sim::charge::{ChargeEvent, ChargeTuning};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::{CastError, GameEvent};
use bd_sim::replicate::{DeltaDecoder, DeltaEncoder, pflags};
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
    // Settle on the ground.
    for _ in 0..60 {
        w.tick(&[]);
    }
    w
}

/// Passive regen off so only the charge refills.
fn no_regen(w: &mut World) {
    let mut t = w.tuning().clone();
    t.willpower.regen_per_sec = 0.0;
    w.set_tuning(t);
}

fn held(buttons: u16) -> PlayerInput {
    PlayerInput { buttons, ..Default::default() }
}

fn charge_events(evs: &[GameEvent], who: PlayerId) -> Vec<ChargeEvent> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::Charge { player, kind, .. } if *player == who => Some(*kind),
            _ => None,
        })
        .collect()
}

#[test]
fn refills_at_the_configured_rate_with_a_ramp() {
    let mut w = duel();
    no_regen(&mut w);
    w.player_mut(A).unwrap().willpower = 10.0;
    let t = ChargeTuning::default();
    let mut expected = 10.0;
    let mut hold = 0.0;
    let mut evs = Vec::new();
    for _ in 0..240 {
        evs.extend_from_slice(w.tick(&[(A, held(buttons::CHARGE))]));
        hold += DT;
        expected += t.rate_at(hold) * DT;
    }
    let wp = w.player(A).unwrap().willpower;
    assert!((wp - expected).abs() < 1e-3, "{wp} vs {expected}");
    // 2 s: 0.8 s ramp (≈ rate·0.8/3) + 1.2 s at full rate.
    let analytic = 10.0 + t.rate_per_sec * (t.ramp_secs / 3.0 + (2.0 - t.ramp_secs));
    assert!((wp - analytic).abs() < 0.05, "{wp} vs {analytic}");
    assert_eq!(charge_events(&evs, A), vec![ChargeEvent::Start]);
    assert!(w.player(A).unwrap().charge.active);
    assert!(w.snapshot_for(None).players.iter().any(|p| p.id == A.0 && p.has(pflags::CHARGING) && p.charge >= 39));
    // Slower than a good oath (≈ 7/s on target), faster than nothing.
    assert!(t.rate_per_sec < 7.0 && t.rate_per_sec > 2.0);

    // Tapping is inefficient: 10 taps of 0.2 s earn far less than one 2 s hold.
    let mut w2 = duel();
    no_regen(&mut w2);
    w2.player_mut(A).unwrap().willpower = 10.0;
    for _ in 0..10 {
        for _ in 0..24 {
            w2.tick(&[(A, held(buttons::CHARGE))]);
        }
        w2.tick(&[(A, held(0))]);
    }
    let tapped = w2.player(A).unwrap().willpower - 10.0;
    assert!(tapped < (wp - 10.0) * 0.25, "taps {tapped} vs hold {}", wp - 10.0);

    // Release ends it; the pool filling announces Full once.
    let evs = w.tick(&[(A, held(0))]).to_vec();
    assert_eq!(charge_events(&evs, A), vec![ChargeEvent::Released]);
    w.player_mut(A).unwrap().willpower = 99.9;
    let mut evs = Vec::new();
    for _ in 0..240 {
        evs.extend_from_slice(w.tick(&[(A, held(buttons::CHARGE))]));
    }
    assert_eq!(charge_events(&evs, A), vec![ChargeEvent::Start, ChargeEvent::Full]);
    assert_eq!(w.player(A).unwrap().willpower, w.rules().willpower_max());
}

#[test]
fn a_hit_interrupts_and_the_charge_restarts_cold_after_a_stagger() {
    let mut w = duel();
    no_regen(&mut w);
    w.player_mut(A).unwrap().willpower = 10.0;
    for _ in 0..120 {
        w.tick(&[(A, held(buttons::CHARGE))]);
    }
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 4.0, 0.0, Vec3::X, Vec3::ZERO));
    let evs = w.tick(&[(A, held(buttons::CHARGE))]).to_vec();
    assert!(charge_events(&evs, A).contains(&ChargeEvent::Broken), "{evs:?}");
    assert!(!w.player(A).unwrap().charge.active);
    let wp = w.player(A).unwrap().willpower;
    // Still holding: nothing during the stagger…
    let stagger = ChargeTuning::default().stagger_secs;
    let mut evs = Vec::new();
    for _ in 0..((stagger / DT) as u32 - 4) {
        evs.extend_from_slice(w.tick(&[(A, held(buttons::CHARGE))]));
    }
    assert!(charge_events(&evs, A).is_empty());
    assert_eq!(w.player(A).unwrap().willpower, wp);
    // …then it starts again from a cold ramp.
    for _ in 0..30 {
        evs.extend_from_slice(w.tick(&[(A, held(buttons::CHARGE))]));
    }
    assert_eq!(charge_events(&evs, A), vec![ChargeEvent::Start]);
    assert!(w.player(A).unwrap().charge.held < 0.3);
    // Damage over time interrupts too.
    for _ in 0..120 {
        w.tick(&[(A, held(buttons::CHARGE))]);
    }
    w.player_mut(A).unwrap().status.apply(bd_sim::status::StatusKind::Burn, 2.0, Some(B));
    let mut evs = Vec::new();
    for _ in 0..130 {
        evs.extend_from_slice(w.tick(&[(A, held(buttons::CHARGE))]));
    }
    assert!(charge_events(&evs, A).contains(&ChargeEvent::Broken));
}

#[test]
fn cannot_cast_attack_or_use_powers_while_charging() {
    let mut w = duel();
    for _ in 0..10 {
        w.tick(&[(A, held(buttons::CHARGE))]);
    }
    assert!(w.player(A).unwrap().busy());
    let eye = w.player(A).unwrap().eye(w.params());
    let hammer = recipe("hammer", Form::Hammer, &[], Element::Light, 1.2);
    let err = w.cast(A, hammer.clone(), None, Aim::new(eye, Vec3::NEG_Z)).unwrap_err();
    assert_eq!(err, CastError::Busy);
    // Ring blast held with the charge: no shot.
    let mut evs = Vec::new();
    for _ in 0..60 {
        evs.extend_from_slice(w.tick(&[(A, held(buttons::CHARGE | buttons::PRIMARY))]));
    }
    assert!(!evs.iter().any(|e| matches!(e, GameEvent::ShotFired { owner, .. } if *owner == A)));
    // Powers refuse too (whatever the reason, never a successful move while charging).
    assert!(w.power_move(A, 0).is_err());
    // Let go: blasting and casting work again.
    let mut evs = Vec::new();
    for _ in 0..30 {
        evs.extend_from_slice(w.tick(&[(A, held(buttons::PRIMARY))]));
    }
    assert!(evs.iter().any(|e| matches!(e, GameEvent::ShotFired { owner, .. } if *owner == A)));
    let eye = w.player(A).unwrap().eye(w.params());
    assert!(w.cast(A, hammer, None, Aim::new(eye, Vec3::NEG_Z)).is_ok());
}

#[test]
fn feet_are_planted_and_prediction_matches_the_host() {
    let walk = |charge: bool| {
        let mut w = duel();
        let start = w.player(A).unwrap().mv.pos;
        let b = if charge { buttons::CHARGE } else { 0 };
        for k in 0..120 {
            w.tick(&[(A, PlayerInput { seq: k, move_z: 1, buttons: b, ..Default::default() })]);
        }
        (w.player(A).unwrap().mv.pos - start).length()
    };
    let (free, charging) = (walk(false), walk(true));
    assert!(charging < free * 0.35, "charging moved {charging} m vs {free} m");

    // Client prediction applies the same slow-down (pure function of the input).
    let mut w = duel();
    let mut client = duel();
    client.apply_snapshot(&w.snapshot_for(Some(A)), Some(A));
    let mut predicted = w.player(A).unwrap().mv;
    for k in 0..240 {
        let b = if k % 90 < 60 { buttons::CHARGE } else { 0 } | if k % 70 < 5 { buttons::JUMP } else { 0 };
        let inp = PlayerInput {
            seq: k,
            yaw: k as f32 * 0.01,
            move_z: 1,
            move_x: if k % 60 < 30 { 1 } else { -1 },
            buttons: b,
            ..Default::default()
        };
        client.predict_player(A, &mut predicted, &inp);
        w.tick(&[(A, inp)]);
    }
    let host = w.player(A).unwrap().mv;
    assert!((host.pos - predicted.pos).length() < 1e-3, "{} vs {}", host.pos, predicted.pos);
}

#[test]
fn deterministic_and_bots_charge_when_low_and_safe() {
    let run = || {
        let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 9, check_invariants: true, ..SimConfig::default() };
        let mut w = World::new(cfg);
        let mut brains = bots::add_bots(&mut w, 1, 2, 0.6, 9);
        w.start_live();
        w.tick(&[]);
        // Far apart and low on willpower.
        w.player_mut(PlayerId(1)).unwrap().mv.pos = Vec3::new(-40.0, 9.0, -40.0);
        w.player_mut(PlayerId(2)).unwrap().mv.pos = Vec3::new(40.0, 9.0, 40.0);
        for p in &mut w.players {
            p.willpower = 15.0;
            p.spawn_shield = 0.0;
        }
        let mut inputs = Vec::new();
        let mut starts = 0;
        let mut hashes = Vec::new();
        for t in 0..1800 {
            inputs.clear();
            bots::drive(&mut w, &mut brains, &mut inputs);
            let evs = w.tick(&inputs);
            starts += evs.iter().filter(|e| matches!(e, GameEvent::Charge { kind: ChargeEvent::Start, .. })).count();
            if t % 60 == 0 {
                hashes.push(w.state_hash());
            }
        }
        (starts, hashes)
    };
    let (starts, h1) = run();
    let (_, h2) = run();
    assert_eq!(h1, h2, "same inputs → same world");
    assert!(starts > 0, "bots never charged");
}

#[test]
fn the_charging_flag_and_level_replicate() {
    let mut host = duel();
    let mut client = duel();
    let mut enc = DeltaEncoder::new();
    let mut dec = DeltaDecoder::new();
    let mut seen = false;
    for t in 0..240 {
        let b = if t < 180 { buttons::CHARGE } else { 0 };
        host.tick(&[(A, held(b))]);
        if t % 2 == 0 {
            let snap = host.snapshot_for(Some(B));
            let delta = enc.encode(&snap);
            let bytes = bincode::serde::encode_to_vec(&delta, bincode::config::standard()).unwrap();
            let (back, _): (bd_sim::SnapshotDelta, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
            let full = dec.decode(&back).expect("baseline known");
            assert_eq!(full, snap);
            enc.ack(full.tick);
            client.apply_snapshot(&full, Some(B));
            let (h, c) = (host.player(A).unwrap(), client.player(A).unwrap());
            assert_eq!(h.charge.active, c.charge.active, "tick {t}");
            if h.charge.active {
                seen = true;
                assert!((h.charge.held - c.charge.held).abs() <= 1.0 / 20.0 + 1e-4, "{} vs {}", h.charge.held, c.charge.held);
                // Remote chargers read as busy on the client too.
                assert!(c.busy());
            }
        }
    }
    assert!(seen);
    assert!(!client.player(A).unwrap().charge.active);
}
