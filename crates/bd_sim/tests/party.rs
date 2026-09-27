//! Party mode + GameSettings (COMBAT.md §5g): settings, the round / pick / podium flow, offers
//! (first / last place, ties, Boom or Bust), every effect's graft, the hooks, replication,
//! determinism, and "defaults leave FFA bit-identical".

use std::sync::Arc;

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::bots::{self, recipe};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::party::effects::{EffectTable, Hook, Rarity};
use bd_sim::party::graft::fit_mechanic;
use bd_sim::party::{BOOM_CARD, EffectId, PartyCard, PartyFx, PartyPhase, PhraseSource, PickError};
use bd_sim::replicate::{DeltaDecoder, DeltaEncoder};
use bd_sim::rng::Fnv64;
use bd_sim::rules::{MatchPhase, ScaleLimit, WillpowerMode};
use bd_sim::settings::{GameMode, GameSettings, PartySettings, Preset};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

/// Smash model: tests written against health read `100 − damage %` (same comparisons).
#[allow(dead_code)]
trait HealthShim {
    fn health(&self) -> f32;
}
impl HealthShim for bd_sim::Player {
    fn health(&self) -> f32 {
        100.0 - self.damage_pct
    }
}

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);
const D: PlayerId = PlayerId(4);

fn hash_events(h: &mut Fnv64, evs: &[GameEvent]) {
    for e in evs {
        let bytes = bincode::serde::encode_to_vec(e, bincode::config::standard()).unwrap();
        h.write(&bytes);
    }
}

// ================================================================================ FFA unchanged

/// A fixed FFA run (6 bots). `casts = false`: movement, ring blasts, tethers, void KOs only.
fn ffa_run(cfg: SimConfig, casts: bool) -> (u64, u64) {
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 6, 0.8, 21);
    for b in &mut brains {
        b.casts_enabled = casts;
    }
    let mut inputs = Vec::new();
    let mut h = Fnv64::default();
    for _ in 0..(40 * 120) {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        let evs = w.tick(&inputs);
        for e in evs {
            h.write(format!("{e:?}").as_bytes());
        }
    }
    (w.state_hash(), h.0)
}

fn ffa_cfg() -> SimConfig {
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed: 21 }, seed: 21, check_invariants: true, ..SimConfig::default() };
    cfg.timings.warmup_secs = 1.0;
    cfg.rules.ko_target = 50;
    cfg
}

/// Recorded before the party / settings change on the same code (and re-verified by building the
/// tree with every party hunk reverted: identical). Only movement / combat changes by other
/// engineers should ever move it — if they do, re-record it with `print_ffa_hashes`.
// Re-recorded after the Smash damage model (COMBAT.md §1a) deliberately changed combat; the
// party-reverted build matched the original record (0xba74_0f28_1dcd_25fa).
// It also includes the §5i anti-snowball defaults (`tuning.snowball`: respawn with full willpower,
// shutdown +1), which move it on their own; `CombatTuning::legacy()` restores the old snowball
// behaviour. The tuning plumbing itself was verified bit-identical by A/B build (tests/tuning.rs).
// Re-recorded again when bots stopped deliberate tether swings across gaps (a third of all bot KOs
// were such self-falls under the Smash model; bots.rs only — the sim itself is unchanged).
// Re-recorded for hitscan guns + human-like bot aim (reaction delay, tracking lag, jitter,
// overshoot) and the faster, fatter ring blast (180 m/s, r 0.35) — deliberate combat / bot changes.
// Re-recorded (was 0x2af1_c836_0da9_ac04) for the tether-save bot fix (bots.rs only): a bot's rescue
// rope no longer counts a cliff-edge anchor right above it as "reached" and lets go into the void, and
// it holds on while hanging past a blast zone. A/B verified: the same tree with just that bot line
// reverted reproduces the old hash (the tether save / recall / placement sim changes alone don't move it).
// Re-recorded (was 0x89ad_f394_3982_9573) for the air jump + dodge (movement/air_jump.rs, dodge.rs):
// bots now air-jump to recover / chase, dodge telegraphed attacks and no longer re-press Jump in the
// air by accident. A/B verified: a copy of this tree with the air jump short-circuited and
// `dodge::bot_moves` skipped reproduces 0x89ad_f394_3982_9573 (the new state, timers, restores and
// event flags alone don't move it).
// Re-recorded (was 0x1fab_383f_88df_c29f) for the wall refresh (owner: touching a wall / surf ramp
// in the air gives the air jump back): bots don't seek it, but 5 incidental wall touches in this
// run hand air jumps back that `dodge::bot_moves` then uses. A/B verified: the same tree with
// `move_params.wall_refresh = false` reproduces 0x1fab_383f_88df_c29f.
// Re-recorded (was 0xc678_ca58_2062_e205) for juggle protection (bd_sim::juggle, owner: "a cooldown
// after a player has been hit so they can't just be knocked around endlessly"): later hits of a chain
// decay and a victim is briefly knockback-guarded, so bots juggle less. A/B verified:
// tests/juggle.rs `juggle_off_is_the_old_sim_bit_for_bit` runs this exact FFA with
// `tuning.juggle = JuggleTuning::off()` on the same tree (casino interactables included) and
// reproduces 0xc678_ca58_2062_e205.
// Re-recorded (was 0xd1a3_7fca_9b34_c73d) when wall splats joined the juggle chain (`juggle.splats`:
// a splat's knockback grows with % too, so a tumbling body wedged between walls ping-ponged), on a
// tree that also carries the in-flight CONNECT projectile bend (world/projectiles.rs, another worker),
// which moves the hash on its own. A/B on that tree (tests/juggle.rs `print_ffa_juggle_ab`):
// juggle off = 0xee46_5d42_cd65_ae0e, on without splats = 0x9b63_3e8c_b1d0_11af, on = this.
// Re-recorded (was 0x0279_2967_07d1_b7e0) for the under-the-map fix (tests/under_terrain.rs): a
// bot's rescue rope no longer picks an anchor on the underside of an overhang (bots.rs), plus the
// rope cut when a rope pins its player against a ceiling it runs through (movement/tether.rs) and
// bots letting go after 1.5 s hanging under terrain. A/B verified on the same tree: all three off =
// 0x0279_2967_07d1_b7e0; the anchor filter alone moves it (the cut / let-go don't fire in this run
// once bots stop hooking undersides); without the filter the rope cut fires here too (bots hooked
// under Archipelago islands) = 0x1692_0cf5_2e05_5120.
const FFA_NOCAST_STATE_HASH: u64 = 0x4d16_0c05_399e_513e;

#[test]
fn default_settings_leave_ffa_bit_identical() {
    let base = ffa_cfg();
    // The lobby's settings carry the rules; everything else at its default.
    let settings = GameSettings { rules: base.rules.clone(), ..GameSettings::preset(Preset::Classic) };
    let via = settings.sim_config(base.clone());
    assert_eq!(via, base, "default settings must not change the config");
    assert_eq!(GameSettings::default().sim_config(SimConfig::default()), SimConfig::default());
    let a = ffa_run(base.clone(), false);
    let b = ffa_run(via, false);
    assert_eq!(a, b);
    assert_eq!(a.0, FFA_NOCAST_STATE_HASH, "FFA (no casts) state hash moved: got {:#x}", a.0);
}

#[test]
#[ignore = "prints the FFA hashes (re-record FFA_NOCAST_STATE_HASH)"]
fn print_ffa_hashes() {
    let a = ffa_run(ffa_cfg(), false);
    let b = ffa_run(ffa_cfg(), true);
    println!("FFA nocast state={:#x} events={:#x} | casts state={:#x} events={:#x}", a.0, a.1, b.0, b.1);
}

// ================================================================================ settings

#[test]
fn settings_round_trip_presets_and_clamping() {
    for p in Preset::ALL {
        let s = GameSettings::preset(p);
        let ron = s.to_ron().unwrap();
        let back = GameSettings::from_ron(&ron).unwrap();
        assert_eq!(back, s.clone().validated(), "{}", p.name());
        let bin = bincode::serde::encode_to_vec(&s, bincode::config::standard()).unwrap();
        let (b2, _): (GameSettings, usize) = bincode::serde::decode_from_slice(&bin, bincode::config::standard()).unwrap();
        assert_eq!(b2, s);
    }
    let chaos = GameSettings::preset(Preset::Chaos);
    assert_eq!(chaos.mode, GameMode::Party);
    assert_eq!(chaos.rules.scale, ScaleLimit::NoLimits);
    assert_eq!(chaos.rules.willpower, WillpowerMode::Infinite);
    let gods = GameSettings::preset(Preset::MovementGods);
    assert_eq!(gods.rules.knockback_scale, 1.5);
    assert_eq!(gods.gas_regen_scale, 1.5);

    // Partial files fill defaults.
    let partial = GameSettings::from_ron("(mode: Party, party: (rounds: 3))").unwrap();
    assert_eq!(partial.mode, GameMode::Party);
    assert_eq!(partial.party.rounds, 3);
    assert_eq!(partial.party.pick_secs, PartySettings::default().pick_secs);

    // Clamping (hostile / broken values).
    let mut bad = GameSettings::default();
    bad.gravity_scale = f32::NAN;
    bad.gas_regen_scale = 99.0;
    bad.rules.knockback_scale = -3.0;
    bad.rules.respawn_secs = f32::INFINITY;
    bad.rules.ko_target = 0;
    bad.party.rounds = 200;
    bad.party.pick_secs = 0.0;
    bad.party.round_time = f32::NAN;
    let v = bad.validated();
    assert_eq!(v.gravity_scale, 1.0);
    assert_eq!(v.gas_regen_scale, 4.0);
    assert_eq!(v.rules.knockback_scale, 0.0);
    assert_eq!(v.rules.respawn_secs, 3.0);
    assert_eq!(v.rules.ko_target, 1);
    assert_eq!(v.party.rounds, 15);
    assert_eq!(v.party.pick_secs, 2.0);
    assert_eq!(v.party.round_time, PartySettings::default().round_time);

    // Wiring into the sim config.
    let mut s = GameSettings::preset(Preset::MovementGods);
    s.gravity_scale = 2.0;
    s.mode = GameMode::Party;
    let cfg = s.sim_config(SimConfig::default());
    let d = SimConfig::default();
    assert_eq!(cfg.move_params.gravity, d.move_params.gravity * 2.0);
    assert_eq!(cfg.move_params.gas_regen_ground, d.move_params.gas_regen_ground * 1.5);
    assert_eq!(cfg.rules.knockback_scale, 1.5);
    assert!(cfg.party.is_some());
    let w = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 64.0 }, ..cfg });
    assert_eq!(w.cfg.rules.ko_target, PartySettings::default().round_ko_target, "party rounds use the round KO target");
}

fn flat_with(settings: GameSettings, n: u8) -> World {
    let base = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(settings.sim_config(base));
    for k in 1..=n {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w
}

#[test]
fn knobs_reach_the_world() {
    // Knockback scale.
    let kb = |scale: f32| {
        let mut s = GameSettings::default();
        s.rules.knockback_scale = scale;
        let mut w = flat_with(s, 2);
        w.start_live();
        w.tick(&[]);
        for p in &mut w.players {
            p.spawn_shield = 0.0;
        }
        w.apply_hit(HitInfo::new(B, Some(A), HitKind::RingBlast, 5.0, 10.0, Vec3::X, Vec3::ZERO)).knockback
    };
    assert!((kb(2.0) - 2.0 * kb(1.0)).abs() < 1e-3);
    // Respawn time + spawn shield.
    let mut s = GameSettings::default();
    s.rules.respawn_secs = 1.0;
    s.rules.spawn_shield_secs = 0.25;
    let mut w = flat_with(s, 2);
    w.start_live();
    w.tick(&[]);
    assert_eq!(w.player(A).unwrap().spawn_shield, 0.25 - DT);
    w.player_mut(A).unwrap().mv.pos.y = -500.0;
    w.tick(&[]);
    assert!(!w.player(A).unwrap().alive);
    for _ in 0..121 {
        w.tick(&[]);
    }
    assert!(w.player(A).unwrap().alive, "respawned after 1 s");
    // Void off: rescued, no KO.
    let mut s = GameSettings::default();
    s.rules.void_ko = false;
    let mut w = flat_with(s, 2);
    w.start_live();
    w.tick(&[]);
    w.player_mut(A).unwrap().mv.pos.y = -500.0;
    let evs = w.tick(&[]).to_vec();
    assert!(evs.iter().any(|e| matches!(e, GameEvent::VoidRescue { player, .. } if *player == A)));
    assert!(!evs.iter().any(|e| matches!(e, GameEvent::Ko { .. })));
    assert!(w.player(A).unwrap().alive && w.player(A).unwrap().mv.pos.y > 0.0);
}

// ================================================================================ party flow

fn party_settings() -> GameSettings {
    let mut s = GameSettings::preset(Preset::Party);
    s.party.round_time = 18.0;
    s.party.round_ko_target = 3;
    s.party.pick_secs = 4.0;
    s.party.round_end_secs = 1.0;
    s.party.reveal_secs = 1.0;
    s.party.party_end_secs = 5.0;
    s
}

fn run_party(seed: u64) -> (u64, u64, Vec<GameEvent>, World) {
    let base = SimConfig { arena: ArenaSource::Archipelago { seed: 7 }, seed, check_invariants: true, ..SimConfig::default() };
    let mut cfg = party_settings().sim_config(base);
    cfg.timings.warmup_secs = 1.0;
    cfg.timings.auto_rematch = false; // stay on the podium
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 8, 0.7, seed);
    let mut inputs = Vec::new();
    let mut h = Fnv64::default();
    let mut keep = Vec::new();
    for _ in 0..(400 * 120) {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        let evs = w.tick(&inputs);
        hash_events(&mut h, evs);
        keep.extend(evs.iter().filter(|e| {
            matches!(
                e,
                GameEvent::PartyPhase { .. }
                    | GameEvent::PartyOffer { .. }
                    | GameEvent::PartyRoundEnd { .. }
                    | GameEvent::PartyReveal { .. }
                    | GameEvent::PartyBoomOrBust { .. }
                    | GameEvent::PartyEnd { .. }
                    | GameEvent::PartyLocked { .. }
            )
        }).cloned());
        if w.party().unwrap().phase == PartyPhase::PartyEnd {
            break;
        }
    }
    (h.0, w.state_hash(), keep, w)
}

#[test]
fn eight_bot_party_runs_to_podium_deterministically() {
    let (h1, s1, evs, w) = run_party(7);
    let party = w.party().unwrap();
    assert_eq!(party.phase, PartyPhase::PartyEnd, "party did not finish");
    assert_eq!(party.round, 7);
    assert_eq!(w.match_state.phase, MatchPhase::Ended);
    let round_ends = evs.iter().filter(|e| matches!(e, GameEvent::PartyRoundEnd { .. })).count();
    let reveals: Vec<_> = evs.iter().filter_map(|e| if let GameEvent::PartyReveal { picks } = e { Some(picks.clone()) } else { None }).collect();
    assert_eq!(round_ends, 7);
    assert_eq!(reveals.len(), 6, "a pick between every pair of rounds");
    assert!(reveals.iter().all(|p| p.len() == 8), "everyone picked every time");
    // Bots locked picks (not all timeouts).
    assert!(evs.iter().any(|e| matches!(e, GameEvent::PartyLocked { .. })));
    assert!(reveals.iter().flatten().any(|p| !p.random));
    // Effects stacked: 6 picks each, no duplicates.
    for p in &party.players {
        assert_eq!(p.n_effects, 6, "{:?}", p.id);
        let mut v = p.owned().to_vec();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), 6);
    }
    // Podium consistent with standings; points = KOs + round-win bonuses.
    let podium = evs.iter().find_map(|e| if let GameEvent::PartyEnd { podium } = e { Some(podium.clone()) } else { None }).unwrap();
    assert_eq!(podium.len(), 8);
    assert!(podium.windows(2).all(|w| w[0].points >= w[1].points));
    assert_eq!(w.match_state.winner, Some(podium[0].player));
    for s in &podium {
        let p = party.player(s.player).unwrap();
        assert_eq!(s.points, p.total_kos + 3 * p.round_wins as u16);
    }
    // Every offer respected the place rules at the time.
    let mut points = std::collections::BTreeMap::<PlayerId, u16>::new();
    for e in &evs {
        match e {
            GameEvent::PartyRoundEnd { results, .. } => {
                for r in results {
                    points.insert(r.player, r.points);
                }
            }
            GameEvent::PartyOffer { player, offer } => {
                let mine = points[player];
                let max = *points.values().max().unwrap();
                let min = *points.values().min().unwrap();
                let boom = offer.cards[..offer.len()].contains(&BOOM_CARD);
                if max == min {
                    assert_eq!(offer.n, 3);
                } else if mine == max {
                    assert_eq!(offer.n, 2, "first place gets 2");
                } else if mine == min {
                    assert!(boom && offer.n == 4, "last place gets 3 + Boom or Bust");
                } else {
                    assert!(offer.n == 3 && !boom);
                }
            }
            _ => {}
        }
    }
    // Same seed ⇒ same everything.
    let (h2, s2, _, _) = run_party(7);
    assert_eq!((h1, s1), (h2, s2));
}

/// A 4-player party world parked at the end of round 1 with the given points.
fn at_round_end(points: &[u16]) -> World {
    let mut w = flat_with(party_settings(), points.len() as u8);
    w.start_live();
    w.tick(&[]);
    let p = w.party.as_mut().unwrap();
    for (k, pts) in points.iter().enumerate() {
        p.players[k].points = *pts;
    }
    p.phase = PartyPhase::RoundEnd;
    p.phase_time = 100.0;
    w.match_state.phase = MatchPhase::Ended;
    w.tick(&[]);
    assert_eq!(w.party().unwrap().phase, PartyPhase::Pick);
    w
}

fn offer_of(w: &World, id: PlayerId) -> bd_sim::party::PartyOffer {
    w.party().unwrap().player(id).unwrap().offer
}

#[test]
fn first_gets_two_last_gets_boom_or_bust_and_ties_share() {
    let w = at_round_end(&[5, 3, 1]);
    assert_eq!(offer_of(&w, A).n, 2);
    assert_eq!(offer_of(&w, B).n, 3);
    let c = offer_of(&w, C);
    assert_eq!(c.n, 4);
    assert_eq!(c.card(3), Some(PartyCard::BoomOrBust));
    // Offers are distinct commons.
    let t = EffectTable::shipped();
    for id in [A, B, C] {
        let o = offer_of(&w, id);
        let mut v: Vec<u16> = o.cards[..o.len()].iter().copied().filter(|c| *c != BOOM_CARD).collect();
        assert!(v.iter().all(|e| t.get(*e).unwrap().rarity == Rarity::Common));
        let n = v.len();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), n);
    }
    // Ties: both leaders get 2, both trailers get the 4th card.
    let w = at_round_end(&[5, 5, 1, 1]);
    assert_eq!((offer_of(&w, A).n, offer_of(&w, B).n), (2, 2));
    assert_eq!((offer_of(&w, C).n, offer_of(&w, D).n), (4, 4));
    // Everyone tied: nobody is first or last.
    let w = at_round_end(&[2, 2, 2]);
    assert!([A, B, C].iter().all(|id| offer_of(&w, *id).n == 3));
    // Toggles.
    let mut s = party_settings();
    s.party.first_place_two_choices = false;
    s.party.boom_or_bust = false;
    let mut w = flat_with(s, 2);
    w.start_live();
    w.tick(&[]);
    let p = w.party.as_mut().unwrap();
    p.players[0].points = 9;
    p.phase = PartyPhase::RoundEnd;
    p.phase_time = 100.0;
    w.tick(&[]);
    assert_eq!((offer_of(&w, A).n, offer_of(&w, B).n), (3, 3));
}

#[test]
fn choose_timeout_and_boom_or_bust_resolve_with_reveal() {
    let mut w = at_round_end(&[5, 3, 1]);
    assert_eq!(w.party_choose(A, 7), Err(PickError::BadIndex));
    assert_eq!(w.party_choose(A, 1), Ok(()));
    assert_eq!(w.party_choose(A, 0), Err(PickError::AlreadyPicked));
    w.party_choose(C, 3).unwrap(); // Boom or Bust
    let a_card = offer_of(&w, A).cards[1];
    // B never picks: timeout ⇒ random.
    let mut evs = Vec::new();
    for _ in 0..(5 * 120) {
        evs.extend_from_slice(w.tick(&[]));
    }
    let party = w.party().unwrap();
    assert!(matches!(party.phase, PartyPhase::Reveal | PartyPhase::Round));
    assert!(party.player(A).unwrap().owns(a_card));
    let reveal = evs.iter().find_map(|e| if let GameEvent::PartyReveal { picks } = e { Some(picks.clone()) } else { None }).unwrap();
    assert_eq!(reveal.len(), 3);
    assert!(reveal.iter().find(|p| p.player == B).unwrap().random);
    let (boom, eff) = evs.iter().find_map(|e| if let GameEvent::PartyBoomOrBust { player, boom, effect } = e { (*player == C).then_some((*boom, *effect)) } else { None }).unwrap();
    let eff = eff.unwrap();
    let spec = party.table.get(eff).unwrap();
    assert_eq!(spec.rarity, if boom { Rarity::Legendary } else { Rarity::Cursed });
    assert!(party.player(C).unwrap().owns(eff));
    assert_eq!(w.party_choose(A, 0), Err(PickError::NotPicking));
    // Coin is ~50/50 over many seeds.
    let mut booms = 0;
    for seed in 0..40u64 {
        let base = SimConfig { arena: ArenaSource::Flat { size_m: 64.0 }, seed, check_invariants: false, ..SimConfig::default() };
        let mut w = World::new(party_settings().sim_config(base));
        for k in 1..=2 {
            w.add_player(PlayerId(k), "p".into(), Corps::from_index(k)).unwrap();
        }
        w.start_live();
        w.tick(&[]);
        let p = w.party.as_mut().unwrap();
        p.players[0].points = 3;
        p.phase = PartyPhase::RoundEnd;
        p.phase_time = 100.0;
        w.tick(&[]);
        w.party_choose(B, 3).unwrap();
        w.party_choose(A, 0).unwrap();
        let evs = w.tick(&[]).to_vec();
        booms += evs.iter().filter(|e| matches!(e, GameEvent::PartyBoomOrBust { boom: true, .. })).count();
    }
    assert!((8..=32).contains(&booms), "{booms}/40");
}

#[test]
fn arena_regen_request_between_rounds() {
    let base = SimConfig { arena: ArenaSource::Flat { size_m: 64.0 }, check_invariants: true, ..SimConfig::default() };
    let mut cfg = party_settings().sim_config(base);
    cfg.timings.auto_rematch = false;
    let mut w = World::new(cfg);
    for k in 1..=2 {
        w.add_player(PlayerId(k), "p".into(), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    assert!(w.party_arena_request().is_none(), "no request while a round is live");
    w.request_carve(bd_sim::combat::CarveRequest { center: w.arena.center, radius: 2.0, power: 1.0 });
    for _ in 0..3 {
        w.tick(&[]);
    }
    let p = w.party.as_mut().unwrap();
    p.phase = PartyPhase::RoundEnd;
    p.phase_time = 100.0;
    w.match_state.phase = MatchPhase::Ended;
    w.tick(&[]);
    let src = w.party_arena_request().expect("carved terrain ⇒ regen request during the pick");
    let (v, a, s) = World::build_arena(src); // (a worker thread in the host)
    w.provide_party_arena(v, a, s);
    assert!(w.party_arena_request().is_none());
    w.party_choose(A, 0).unwrap();
    w.party_choose(B, 0).unwrap();
    let mut evs = Vec::new();
    for _ in 0..(3 * 120) {
        evs.extend_from_slice(w.tick(&[]));
    }
    assert!(evs.iter().any(|e| matches!(e, GameEvent::ArenaReset { .. })));
    assert_eq!(w.party().unwrap().phase, PartyPhase::Round);
    assert_eq!(w.party().unwrap().round, 2);
    // Without a host-provided arena the world's own worker-thread prefetch regenerates it.
    w.request_carve(bd_sim::combat::CarveRequest { center: w.arena.center, radius: 2.0, power: 1.0 });
    for _ in 0..3 {
        w.tick(&[]);
    }
    let p = w.party.as_mut().unwrap();
    p.phase = PartyPhase::Round;
    w.match_state.phase = MatchPhase::Ended; // round over
    let mut evs = Vec::new();
    for _ in 0..(8 * 120) {
        evs.extend_from_slice(w.tick(&[]));
        if w.party().unwrap().phase == PartyPhase::Pick {
            w.party_choose(A, 0).ok();
            w.party_choose(B, 0).ok();
        }
        if w.party().unwrap().round == 3 {
            break;
        }
    }
    assert_eq!(w.party().unwrap().round, 3);
    assert!(evs.iter().any(|e| matches!(e, GameEvent::ArenaReset { .. })), "prefetched terrain swapped in");
}

#[test]
fn party_snapshot_replicates_standings_and_own_offer() {
    let w = at_round_end(&[5, 3, 1]);
    let sa = w.snapshot_for(Some(A));
    let sc = w.snapshot_for(Some(C));
    let pa = sa.party.as_ref().unwrap();
    assert_eq!(pa.phase, PartyPhase::Pick as u8);
    assert_eq!(pa.offer.unwrap().n, 2);
    assert_eq!(sc.party.as_ref().unwrap().offer.unwrap().n, 4);
    assert_eq!(pa.players.iter().find(|p| p.id == A.0).unwrap().place, 1);
    assert!(pa.time_left > 0);
    // Deltas carry it.
    let mut enc = DeltaEncoder::new();
    let mut dec = DeltaDecoder::new();
    let d = enc.encode(&sa);
    let got = dec.decode(&d).unwrap();
    assert_eq!(got.party, sa.party);
    // FFA snapshots carry none.
    let mut ffa = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 64.0 }, ..SimConfig::default() });
    ffa.add_player(A, "a".into(), Corps::Hope).unwrap();
    assert!(ffa.snapshot_for(Some(A)).party.is_none());
    // A client mirror adopts owned effects (prediction needs Slippery).
    let mut host = at_round_end(&[1, 0]);
    let slippery = host.party().unwrap().table.find("slippery").unwrap();
    host.grant_party_effect(A, slippery);
    let snap = host.snapshot_for(Some(A));
    let mut client = World::new(host.cfg.clone());
    client.apply_snapshot(&snap, Some(A));
    assert!(client.party().unwrap().player(A).unwrap().owns(slippery));
}

// ================================================================================ effects

/// A live 2-player party world (A casts, B is the target 10 m in front), A owns `effects`.
fn live_with(effects: &[&str]) -> World {
    let mut w = flat_with(party_settings(), 2);
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    let t = w.party().unwrap().table.clone();
    for e in effects {
        let id = t.find(e).unwrap_or_else(|| panic!("no effect {e}"));
        assert!(w.grant_party_effect(A, id));
    }
    let c = w.arena.center + Vec3::Y * 0.1;
    w.player_mut(A).unwrap().mv.pos = c;
    w.player_mut(B).unwrap().mv.pos = c + Vec3::new(0.0, 0.0, -10.0);
    w.player_mut(A).unwrap().input.yaw = 0.0;
    w
}

fn aim_of(w: &World, id: PlayerId) -> Aim {
    let p = w.player(id).unwrap();
    Aim::new(p.eye(w.params()), (p.look() + Vec3::new(0.0, -0.3, 0.0)).normalize())
}

fn cast(w: &mut World, r: Recipe) -> bd_core::ConstructId {
    let aim = aim_of(w, A);
    w.player_mut(A).unwrap().willpower = 100.0;
    w.cast(A, r, None, aim).unwrap()
}

fn sample_for(cat: FormCategory) -> Recipe {
    match cat {
        FormCategory::Held => recipe("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light, 1.2),
        FormCategory::Worn => recipe("wings", Form::Wings, &[Mechanic::Glide], Element::Light, 2.0),
        FormCategory::Creature => recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2),
        FormCategory::Structure => recipe("wall", Form::Wall, &[], Element::Light, 4.0),
        FormCategory::Object => recipe("ball", Form::Ball, &[], Element::Light, 0.8),
    }
}

#[test]
fn shipped_table_is_complete() {
    let t = EffectTable::shipped();
    let n = |r: Rarity| t.effects.iter().filter(|e| e.rarity == r).count();
    assert!(n(Rarity::Common) >= 30, "{}", n(Rarity::Common));
    assert!(n(Rarity::Legendary) >= 6);
    assert!(n(Rarity::Cursed) >= 6);
    for key in [
        "bouncy_castle", "clingy", "heat_seekers", "spicy", "shocking", "party_popper", "mini_me", "clown_car", "growth_spurt", "chain_gang",
        "portal_pals", "magnet_brain", "rebound", "legs_day", "wings_day", "boomerang", "road_rage", "friendly_fire_ant", "worthy",
        "kaiju_heart", "echo", "roll_out", "main_character", "butterfingers", "pinata", "wild_magic", "reverse_card", "loud", "slippery",
    ] {
        assert!(t.find(key).is_some(), "COMBAT.md §5g effect {key} missing");
    }
    for e in &t.effects {
        assert!(!e.graft.is_empty() || !e.hooks.is_empty(), "{} does nothing", e.id);
        assert!(e.desc.split_whitespace().count() <= 6);
    }
}

/// Every graft effect shows up in the recipe of a matching cast (per-form sanity applied), and the
/// cast still costs what was typed.
#[test]
fn every_graft_effect_transforms_casts() {
    let t = EffectTable::shipped();
    for (i, spec) in t.effects.iter().enumerate() {
        if spec.graft.is_empty() {
            continue;
        }
        let cats = if spec.graft.only.is_empty() { vec![FormCategory::Object, FormCategory::Creature, FormCategory::Structure, FormCategory::Held] } else { spec.graft.only.clone() };
        for cat in cats {
            let mut w = live_with(&[]);
            assert!(w.grant_party_effect(A, i as EffectId));
            let typed = sample_for(cat);
            let (_, typed_cost) = w.preview_cost(A, typed.clone());
            w.player_mut(A).unwrap().willpower = 100.0;
            let aim = aim_of(&w, A);
            let id = w.cast(A, typed.clone(), None, aim).unwrap_or_else(|e| panic!("{} on {cat:?}: {e:?}", spec.id));
            let spent = 100.0 - w.player(A).unwrap().willpower;
            assert!((spent - typed_cost).abs() < 1e-3, "{} on {cat:?}: party grafts must be free ({spent} vs {typed_cost})", spec.id);
            let r = &w.constructs.get(id).unwrap().recipe;
            for m in &spec.graft.mechanics {
                if let Some(m2) = fit_mechanic(*m, typed.form) {
                    assert!(r.has(m2), "{} on {cat:?}: missing {m2:?} in {:?}", spec.id, r.mechanics);
                }
            }
            if let Some(e) = spec.graft.element {
                assert_eq!(r.element, e, "{}", spec.id);
            }
            if let Some(tm) = spec.graft.temperament {
                assert_eq!(r.temperament, tm, "{}", spec.id);
            }
            for f in &spec.graft.features {
                assert!(r.features.contains(f), "{}", spec.id);
            }
            if spec.graft.count.is_some() && fit_mechanic(Mechanic::Swarm, typed.form).is_some() {
                assert!(r.count >= 3 && r.scale < typed.scale, "{}", spec.id);
            }
        }
    }
    // Held-only effects leave other forms alone; Walks never lands on a held item.
    let mut w = live_with(&["worthy", "legs_day"]);
    let id = cast(&mut w, sample_for(FormCategory::Object));
    let r = w.constructs.get(id).unwrap().recipe.clone();
    assert!(!r.has(Mechanic::Returning) && r.element == Element::Light && r.has(Mechanic::Walks));
    let id = cast(&mut w, sample_for(FormCategory::Held));
    let r = w.constructs.get(id).unwrap().recipe.clone();
    assert!(r.has(Mechanic::Returning) && r.element == Element::Electric && !r.has(Mechanic::Walks));
}

fn fx_count(evs: &[GameEvent], want: PartyFx) -> usize {
    evs.iter().filter(|e| matches!(e, GameEvent::PartyFx { fx, .. } if *fx == want)).count()
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn shatter(w: &mut World, id: bd_core::ConstructId) {
    let c = w.constructs.get_mut(id).unwrap();
    c.integrity = 0.0;
    c.gone = Some(bd_sim::events::GoneReason::Spent);
    c.shatter = Some(Vec3::Y);
}

#[test]
fn echo_and_mini_me() {
    let mut w = live_with(&["echo"]);
    let id = cast(&mut w, recipe("wall", Form::Wall, &[], Element::Light, 4.0));
    let evs = run(&mut w, 125);
    assert_eq!(fx_count(&evs, PartyFx::Echo), 1);
    let echo = w.constructs.iter().find(|c| c.parent == Some(id)).expect("echo copy");
    assert_eq!(echo.recipe.form, Form::Wall);
    assert_eq!(echo.cost, 0.0);
    // Held items are not echoed (the copy would replace the original).
    let mut w = live_with(&["echo"]);
    cast(&mut w, sample_for(FormCategory::Held));
    assert_eq!(fx_count(&run(&mut w, 125), PartyFx::Echo), 0);

    let mut w = live_with(&["mini_me"]);
    let id = cast(&mut w, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.5));
    let mini = w.constructs.iter().find(|c| c.parent == Some(id)).expect("mini copy");
    assert!(mini.recipe.name.starts_with("mini") && mini.recipe.scale < 1.5);
    let mini_id = mini.id;
    run(&mut w, 240);
    let (o, m) = (w.constructs.get(id), w.constructs.get(mini_id));
    if let (Some(o), Some(m)) = (o, m) {
        assert!((o.center() - m.center()).length() < 12.0, "the mini follows the original");
    }
}

#[test]
fn shatter_hooks_confetti_portal_black_hole() {
    let mut w = live_with(&["party_popper", "portal_pals", "black_hole_sun"]);
    let id = cast(&mut w, recipe("wall", Form::Wall, &[], Element::Light, 4.0));
    run(&mut w, 40);
    shatter(&mut w, id);
    let evs = run(&mut w, 2);
    assert_eq!(fx_count(&evs, PartyFx::Confetti), 1);
    assert_eq!(fx_count(&evs, PartyFx::BlackHole), 1);
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Explosion { .. })), "confetti blast");
    assert!(evs.iter().any(|e| matches!(e, GameEvent::PortalOpened { owner, .. } if *owner == A)));
    assert!(evs.iter().any(|e| matches!(e, GameEvent::FieldCreated { .. })));
    let later = run(&mut w, 200);
    assert!(later.iter().any(|e| matches!(e, GameEvent::Explosion { element: Element::Shadow, .. })), "the black hole pops");
}

#[test]
fn main_character_splits_before_shattering() {
    let mut w = live_with(&["main_character"]);
    let id = cast(&mut w, recipe("boulder", Form::Boulder, &[], Element::Light, 1.5));
    run(&mut w, 30);
    shatter(&mut w, id);
    let evs = run(&mut w, 1);
    assert_eq!(fx_count(&evs, PartyFx::Split), 1);
    assert!(!evs.iter().any(|e| matches!(e, GameEvent::ConstructShattered { id: x, .. } if *x == id)));
    let c = w.constructs.get(id).expect("survived stage one");
    assert!(c.gone.is_none() && c.integrity > 0.0);
    assert!(w.constructs.iter().any(|c| c.parent == Some(id)), "split half");
    shatter(&mut w, id);
    let evs = run(&mut w, 1);
    assert!(evs.iter().any(|e| matches!(e, GameEvent::ConstructShattered { id: x, .. } if *x == id)), "stage two shatters");
}

#[test]
fn growth_spurt_and_kaiju_heart() {
    let mut w = live_with(&["growth_spurt"]);
    let id = cast(&mut w, recipe("wall", Form::Wall, &[], Element::Light, 4.0));
    let s0 = w.constructs.get(id).unwrap().size_scale;
    let evs = run(&mut w, 5 * 120);
    let s1 = w.constructs.get(id).unwrap().size_scale;
    assert!(s1 > s0 * 1.4, "{s0} → {s1}");
    assert!(fx_count(&evs, PartyFx::Grow) >= 2);

    let mut w = live_with(&["kaiju_heart"]);
    let a = cast(&mut w, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2));
    let base = w.constructs.get(a).unwrap().recipe.scale;
    w.player_mut(A).unwrap().kos = 1;
    let b = cast(&mut w, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2));
    assert!(w.constructs.get(b).unwrap().recipe.scale > base * 1.3);
    w.player_mut(A).unwrap().kos = 3;
    let k = cast(&mut w, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2));
    assert_eq!(w.constructs.get(k).unwrap().recipe.form, Form::Kaiju);
    // A KO grows the live creatures.
    let before = w.constructs.get(b).map(|c| c.size_scale);
    w.apply_hit(HitInfo::new(B, Some(A), HitKind::RingBlast, 50.0, 5.0, Vec3::X, Vec3::ZERO));
    w.player_mut(B).unwrap().mv.pos.y = -100.0; // Smash model: KOs only through a blast zone
    let evs = run(&mut w, 1);
    assert_eq!(fx_count(&evs, PartyFx::KaijuGrow), 1);
    if let (Some(b0), Some(c)) = (before, w.constructs.get(b)) {
        assert!(c.size_scale > b0);
    }
}

struct Fixed;
impl PhraseSource for Fixed {
    fn random_recipe(&self, _seed: u64) -> Option<Recipe> {
        Some(recipe("rubber duck", Form::Ball, &[Mechanic::Bounce], Element::Water, 0.8))
    }
}

#[test]
fn wild_magic_uses_the_injected_phrase_source() {
    // A table where Wild Magic always fires.
    let mut t = (*EffectTable::shipped()).clone();
    let wm = t.find("wild_magic").unwrap() as usize;
    t.effects[wm].hooks = vec![Hook::WildMagic { chance: 1.0 }];
    let mut w = live_with(&[]);
    w.set_party_effects(Arc::new(t));
    w.grant_party_effect(A, wm as EffectId);
    let id = cast(&mut w, sample_for(FormCategory::Held));
    assert_ne!(w.constructs.get(id).unwrap().recipe.name, "hammer", "wild magic (fallback generator)");
    w.set_party_phrase_source(Arc::new(Fixed));
    let id = cast(&mut w, sample_for(FormCategory::Held));
    let r = &w.constructs.get(id).unwrap().recipe;
    assert_eq!((r.name.as_str(), r.form), ("rubber duck", Form::Ball));
    assert_eq!(fx_count(&run(&mut w, 1), PartyFx::WildMagic), 2);
}

#[test]
fn butterfingers_slips_the_held_item() {
    let mut w = live_with(&["butterfingers"]);
    let h = cast(&mut w, sample_for(FormCategory::Held));
    let evs = run(&mut w, 20 * 120);
    assert!(fx_count(&evs, PartyFx::Slip) >= 1);
    assert!(w.constructs.get(h).is_none(), "the hammer left the hand");
    assert!(evs.iter().any(|e| matches!(e, GameEvent::ConstructSpawned { owner, recipe, .. } if *owner == A && recipe.form == Form::Anvil)));
}

#[test]
fn pinata_copycat_last_words() {
    // Piñata: A (owner) gets hit by B → B gets candy.
    let mut w = live_with(&["pinata", "copycat", "last_words"]);
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 5.0, 3.0, Vec3::X, w.player(A).unwrap().mv.pos));
    let evs = run(&mut w, 1);
    assert_eq!(fx_count(&evs, PartyFx::Pinata), 1);
    assert!(w.constructs.iter().any(|c| c.owner == B), "B picked up candy");
    // Copycat: a construct of B hits A → A casts a copy.
    let aim = aim_of(&w, B);
    w.player_mut(B).unwrap().willpower = 100.0;
    let bc = w.cast(B, recipe("boulder", Form::Boulder, &[], Element::Earth, 1.2), None, aim).unwrap();
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::Thrown, 5.0, 3.0, Vec3::X, Vec3::ZERO).with_construct(bc));
    let evs = run(&mut w, 1);
    assert_eq!(fx_count(&evs, PartyFx::Copycat), 1);
    assert!(w.constructs.iter().any(|c| c.owner == A && c.recipe.form == Form::Boulder));
    // Last Words: KO'd ⇒ a bomb goes off where A fell.
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 50.0, 3.0, Vec3::X, Vec3::ZERO));
    w.player_mut(A).unwrap().mv.pos.y = -100.0; // Smash model: KOs only through a blast zone
    let evs = run(&mut w, 130);
    assert_eq!(fx_count(&evs, PartyFx::LastWords), 1);
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Explosion { source: Some(s), element: Element::Fire, .. } if *s == A)));
}

#[test]
fn reverse_card_curves_shots_back() {
    let mut w = live_with(&["reverse_card"]);
    let fire = PlayerInput { buttons: buttons::PRIMARY, ..Default::default() };
    w.tick(&[(A, fire)]);
    let pid = w.projectiles.iter().find(|p| p.spec.owner == A).map(|p| p.id).unwrap();
    for _ in 0..(1.25 / DT) as usize {
        w.tick(&[(A, PlayerInput::default())]);
    }
    let me = w.player(A).unwrap().mv.pos;
    if let Some(p) = w.projectiles.iter().find(|p| p.id == pid) {
        assert!(p.vel.dot(me - p.pos) > 0.0, "the shot is heading back");
    }
}

#[test]
fn slippery_hiccups_sidekick_loud_magnetic_popcorn() {
    // Slippery: the same ground stop takes much longer.
    let stop_time = |effects: &[&str]| {
        let mut w = live_with(effects);
        w.player_mut(A).unwrap().mv.vel = Vec3::new(8.0, 0.0, 0.0);
        let mut t = 0;
        while w.player(A).unwrap().mv.vel.length() > 0.5 && t < 2000 {
            w.tick(&[(A, PlayerInput::default())]);
            t += 1;
        }
        t
    };
    assert!(stop_time(&["slippery"]) > 3 * stop_time(&[]));

    let mut w = live_with(&["hiccups", "sidekick", "loud", "magnetic_personality", "popcorn"]);
    let wall = cast(&mut w, recipe("wall", Form::Wall, &[], Element::Light, 4.0));
    let r = &w.constructs.get(wall).unwrap().recipe;
    assert!(r.features.contains(&bd_core::mechanics::Feature::Bell));
    // B's ball near A drifts toward A.
    let aim = aim_of(&w, B);
    w.player_mut(B).unwrap().willpower = 100.0;
    let ball = w.cast(B, recipe("ball", Form::Ball, &[], Element::Light, 0.8), None, aim).unwrap();
    let evs = run(&mut w, 16 * 120);
    assert!(fx_count(&evs, PartyFx::Hiccup) >= 1);
    assert!(fx_count(&evs, PartyFx::Sidekick) >= 1);
    assert!(fx_count(&evs, PartyFx::Honk) >= 1);
    assert!(w.constructs.iter().any(|c| c.owner == A && c.recipe.name == "sidekick spirit"));
    let _ = ball;
    // Popcorn: a shot into the ground pops into kernels.
    let mut w = live_with(&["popcorn"]);
    w.player_mut(A).unwrap().input.pitch = -1.4;
    let fire = PlayerInput { buttons: buttons::PRIMARY, pitch: -1.4, ..Default::default() };
    // (The 180 m/s ring blast can reach the floor on the trigger tick: keep that tick's events.)
    let mut evs: Vec<GameEvent> = w.tick(&[(A, fire)]).to_vec();
    evs.extend((0..60).flat_map(|_| w.tick(&[(A, PlayerInput { pitch: -1.4, ..Default::default() })]).to_vec()));
    assert_eq!(fx_count(&evs, PartyFx::Popcorn), 1);
}

#[test]
fn magnetic_personality_pulls_enemy_constructs() {
    let drift = |effects: &[&str]| {
        let mut w = live_with(effects);
        let a = w.player(A).unwrap().mv.pos;
        let aim = Aim::new(w.player(B).unwrap().eye(w.params()), Vec3::new(0.0, -0.2, 1.0).normalize());
        w.player_mut(B).unwrap().willpower = 100.0;
        let ball = w.cast(B, recipe("ball", Form::Ball, &[], Element::Light, 0.8), None, aim).unwrap();
        {
            let c = w.constructs.get_mut(ball).unwrap();
            c.pos = a + Vec3::new(0.0, 0.5, -7.0);
            c.vel = Vec3::ZERO;
        }
        let d0 = (w.constructs.get(ball).unwrap().center() - a).length();
        run(&mut w, 90);
        w.constructs.get(ball).map_or(0.0, |c| d0 - (c.center() - a).length())
    };
    let (m, n) = (drift(&["magnetic_personality"]), drift(&[]));
    assert!(m > n + 0.5, "enemy constructs drift toward the cursed player ({m} vs {n})");
}

// ================================================================================ chaos

#[test]
fn chaos_party_with_stacked_effects_is_deterministic() {
    use bd_sim::fuzz::{ChaosConfig, ChaosWorld, random_rules};
    use bd_sim::rng::SimRng;
    let go = |seed: u64| {
        let mut cfg = ChaosConfig::new(seed, random_rules(&mut SimRng::new(seed)));
        cfg.arena = ArenaSource::Flat { size_m: 96.0 };
        cfg.cast_rate = 2.0;
        cfg.party = Some(6);
        let mut cw = ChaosWorld::with_config(cfg);
        let mut h = Fnv64::default();
        for _ in 0..2400 {
            let r = cw.step();
            assert!(r.invariant_error.is_none(), "seed {seed} tick {}: {:?}", r.tick, r.invariant_error);
            hash_events(&mut h, cw.world.events());
        }
        // (chaos joins / leaves: newcomers start with nothing)
        assert!(cw.world.party().unwrap().players.iter().filter(|p| p.n_effects >= 3).count() >= 4);
        (h.0, cw.world.state_hash())
    };
    for seed in [31u64, 32] {
        assert_eq!(go(seed), go(seed), "seed {seed}");
    }
}
