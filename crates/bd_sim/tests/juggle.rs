//! Juggle protection (`bd_sim::juggle`, COMBAT.md §1a): later hits of a chain decay, the victim is
//! guarded after `max_hits`, the chain resets on timeout / ground / own tether; single big hits and
//! star KOs are untouched; host and prediction agree; `enabled: false` is the old sim bit for bit.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots;
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::juggle::{GUARD_STATUS_BIT, JuggleTuning};
use bd_sim::rng::Fnv64;
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const G: f32 = 8.0;

fn world_with(n: u8, f: impl FnOnce(&mut SimConfig)) -> World {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    f(&mut cfg);
    let mut w = World::new(cfg);
    for k in 1..=n {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w
}

fn world(juggle: bool) -> World {
    world_with(2, |c| {
        if !juggle {
            c.tuning.juggle = JuggleTuning::off();
        }
    })
}

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
    p.mv.tumble = 0.0;
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
    }
    all
}

/// A heavy blow on B (≈ 25 m/s at 50 %: a chain hit that launches).
fn heavy(w: &mut World) -> (f32, f32) {
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::HeavyMelee, 12.0, 20.0, Vec3::X, Vec3::ZERO));
    assert!(r.applied);
    (r.damage, r.knockback)
}

/// B high in the air at `pct`, hit every `gap` ticks, re-centred before each hit so it never lands.
fn chain(juggle: bool, hits: usize, gap: u32) -> (World, Vec<(f32, f32)>) {
    let mut w = world(juggle);
    place(&mut w, B, Vec3::new(0.0, 60.0, 0.0));
    w.player_mut(B).unwrap().damage_pct = 50.0;
    let mut out = Vec::new();
    for k in 0..hits {
        if k > 0 {
            run(&mut w, gap);
            let p = w.player_mut(B).unwrap();
            p.mv.pos = Vec3::new(0.0, 60.0, 0.0);
            p.mv.vel = Vec3::ZERO;
        }
        out.push(heavy(&mut w));
    }
    (w, out)
}

#[test]
fn repeated_hits_decay_then_the_victim_is_guarded() {
    let (w, on) = chain(true, 5, 12);
    let (_, off) = chain(false, 5, 12);
    let t = JuggleTuning::default();
    assert_eq!(on[0], off[0], "the first hit is untouched");
    assert!((on[1].0 - off[0].0 * t.dmg_decay).abs() < 1e-3, "2nd hit damage decays: {:?}", on[1]);
    assert!(on[1].1 < off[1].1 * 0.6 + 1e-3 && on[1].1 > 0.0, "2nd knockback ×0.6: {:?} vs {:?}", on[1], off[1]);
    assert!(on[2].1 < off[2].1 * 0.35 + 1e-3 && on[2].1 > 0.0, "3rd knockback ×0.35: {:?} vs {:?}", on[2], off[2]);
    for h in &on[3..] {
        assert_eq!(h.1, 0.0, "guarded: no knockback");
        assert!((h.0 - off[0].0 * t.immune_dmg).abs() < 1e-3, "guarded hits still deal a little: {h:?}");
    }
    assert!(w.player(B).unwrap().juggle.guarded());
    // Replicated as the status byte's guard bit (the client's shimmer).
    let snap = w.snapshot_for(None);
    let b = snap.players.iter().find(|p| p.id == B.0).unwrap();
    assert!(b.status & GUARD_STATUS_BIT != 0);
    // Off: nothing is ever guarded, every hit launches.
    assert!(off.iter().all(|h| h.1 > 20.0), "{off:?}");
}

#[test]
fn a_guarded_hit_does_not_launch_but_registers() {
    let (mut w, _) = chain(true, 3, 12);
    run(&mut w, 1);
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::HeavyMelee, 12.0, 20.0, Vec3::X, Vec3::ZERO));
    assert!(r.applied && r.knockback == 0.0 && r.damage > 0.0);
    let ev = w.events();
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Hit { target, knockback, .. } if *target == B && *knockback == 0.0)), "the Hit event still fires");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Launched { player, .. } if *player == B)), "no launch while guarded");
}

#[test]
fn guard_times_out_then_hits_launch_again() {
    let t = JuggleTuning::default();
    let (mut w, _) = chain(true, 3, 12);
    assert!(w.player(B).unwrap().juggle.guarded());
    let ticks = (t.immune_secs * 120.0).ceil() as u32 + 1;
    for _ in 0..ticks {
        let p = w.player_mut(B).unwrap();
        p.mv.pos = Vec3::new(0.0, 60.0, 0.0);
        p.mv.vel = Vec3::ZERO;
        w.tick(&[]);
    }
    let j = w.player(B).unwrap().juggle;
    assert!(!j.guarded() && j.hits == 0, "guard over, chain reset: {j:?}");
    let (_, kb) = heavy(&mut w);
    assert!(kb > 22.0, "a fresh chain launches at full strength: {kb}");
}

#[test]
fn window_times_out_between_slow_hits() {
    // One hit every 1.1 s never chains (the window is 1 s).
    let (_, on) = chain(true, 4, 132);
    let (_, off) = chain(false, 4, 132);
    assert_eq!(on, off);
}

#[test]
fn landing_resets_the_chain() {
    let (mut w, _) = chain(true, 2, 12);
    assert_eq!(w.player(B).unwrap().juggle.hits, 2);
    place(&mut w, B, Vec3::new(0.0, G + 0.2, 0.0));
    run(&mut w, 30);
    assert!(w.player(B).unwrap().mv.grounded);
    assert_eq!(w.player(B).unwrap().juggle.hits, 0, "standing again: a clean slate");
    let (_, kb) = heavy(&mut w);
    assert!(kb > 22.0, "{kb}");
}

#[test]
fn own_tether_resets_the_chain() {
    let mut w = world(true);
    // A stone ceiling to hook.
    w.voxels.fill_box(bd_core::IVec3::new(-20, 70, -20), bd_core::IVec3::new(20, 72, 20), bd_voxel::mat::STONE);
    place(&mut w, B, Vec3::new(0.0, 28.0, 0.0));
    w.player_mut(B).unwrap().damage_pct = 50.0;
    heavy(&mut w);
    run(&mut w, 12);
    heavy(&mut w);
    assert_eq!(w.player(B).unwrap().juggle.hits, 2);
    let up = PlayerInput { pitch: 1.5, buttons: buttons::TETHER_L, ..Default::default() };
    let mut attached = false;
    for _ in 0..60 {
        w.tick(&[(B, up)]);
        if w.player(B).unwrap().mv.tethered() {
            attached = true;
            break;
        }
    }
    assert!(attached, "B hooked the ceiling");
    w.tick(&[(B, up)]);
    assert_eq!(w.player(B).unwrap().juggle.hits, 0, "hanging on your own tether: a clean slate");
}

#[test]
fn single_big_hit_still_kos_after_small_pokes() {
    let mut w = world(true);
    place(&mut w, B, Vec3::new(12.0, G, 0.0));
    w.player_mut(B).unwrap().damage_pct = 300.0;
    // Two pokes (not chain hits) right before the blow change nothing.
    for _ in 0..2 {
        w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, 0.5, 1.0, Vec3::X, Vec3::ZERO));
        run(&mut w, 6);
    }
    assert_eq!(w.player(B).unwrap().juggle.hits, 0);
    place(&mut w, B, Vec3::new(12.0, G, 0.0));
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::HeavyMelee, 30.0, 28.0, Vec3::X, Vec3::ZERO));
    assert!(r.knockback > 80.0, "{}", r.knockback);
    let ev = run(&mut w, 600);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Ko { victim, killer: Some(k), .. } if *victim == B && *k == A)), "launched out");
}

#[test]
fn a_star_ko_is_one_hit() {
    let mut w = world(true);
    place(&mut w, B, Vec3::new(0.0, G, 0.0));
    w.player_mut(B).unwrap().damage_pct = 400.0;
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::HeavyMelee, 30.0, 40.0, Vec3::Y, Vec3::ZERO));
    assert!(r.knockback > 80.0);
    let ev = run(&mut w, 900);
    let zone = ev.iter().find_map(|e| match e {
        GameEvent::Ko { victim, zone, .. } if *victim == B => Some(*zone),
        _ => None,
    });
    assert_eq!(zone, Some(Some(bd_sim::smash::BlastZone::Top)), "star KO");
}

/// Client prediction replays the host's knockback (decayed and guarded alike) exactly.
#[test]
fn prediction_matches_the_host_through_a_juggle() {
    assert_eq!(predict_through_hits(false), (0, false), "harness: prediction without protection");
    assert_eq!(predict_through_hits(true), (0, true), "prediction with protection (reaches the guard)");
}

/// B hits A (airborne, steering) every 1/8 s for 1 s; returns (diverged ticks, guard reached).
fn predict_through_hits(juggle: bool) -> (u32, bool) {
    let mut host = world(juggle);
    place(&mut host, A, Vec3::new(0.0, 40.0, 0.0));
    host.player_mut(A).unwrap().damage_pct = 60.0;
    host.tick(&[]);
    let mut client = World::new(host.cfg.clone());
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let inp = PlayerInput { move_x: 1, move_z: 1, ..Default::default() };
    let mut bad = 0;
    let mut guarded = false;
    for k in 0..240u32 {
        if k % 15 == 0 && k < 120 {
            let r = host.apply_hit(HitInfo::new(A, Some(B), HitKind::HeavyMelee, 12.0, 20.0, Vec3::new(1.0, 0.0, 0.3), Vec3::ZERO));
            assert!(r.applied);
            client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        }
        guarded |= host.player(A).unwrap().juggle.guarded();
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &inp);
        let evs = host.tick(&[(A, inp)]).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        if !host.player(A).unwrap().alive {
            break; // launched out (no protection): nothing left to predict
        }
        let h = host.player(A).unwrap().mv;
        if (h.pos - s.pos).length() > 1e-4 || (h.vel - s.vel).length() > 1e-3 {
            bad += 1;
            if bad < 4 {
                eprintln!("juggle={juggle} tick {k}: {} m, {} m/s", (h.pos - s.pos).length(), (h.vel - s.vel).length());
            }
        }
    }
    (bad, guarded)
}

// ------------------------------------------------------------------------ whole-sim runs

fn ffa_cfg(juggle: JuggleTuning, seed: u64) -> SimConfig {
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed: 21 }, seed, check_invariants: true, ..SimConfig::default() };
    cfg.tuning.juggle = juggle;
    cfg.timings.warmup_secs = 1.0;
    cfg.rules.ko_target = 50;
    cfg
}

/// Bot brawl → (state hash, chain stats).
fn brawl(juggle: JuggleTuning, seed: u64, bots_n: u8, secs: u32, casts: bool) -> (u64, ChainStats) {
    let mut w = World::new(ffa_cfg(juggle, seed));
    let mut brains = bots::add_bots(&mut w, 0, bots_n, 0.8, seed);
    for b in &mut brains {
        b.casts_enabled = casts;
    }
    let mut inputs = Vec::new();
    let mut stats = ChainStats::default();
    for _ in 0..(secs * 120) {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        let evs = w.tick(&inputs).to_vec();
        stats.observe(&w, &evs);
    }
    stats.finish();
    (w.state_hash(), stats)
}

/// Juggle chains as a victim experiences them, measured from events + state only (independent of
/// the protection code): consecutive hits that knock you around (knockback ≥ the tumble threshold,
/// 14 m/s; all hits in one tick = one step), each within 1 s of the previous, without being back in
/// control (standing out of tumble / on your own tether) or dead in between. Duration = first to
/// last knocking hit.
#[derive(Default, Debug)]
struct ChainStats {
    /// Per player: (hits, first tick, last tick).
    open: Vec<Option<(u32, u64, u64)>>,
    chains: Vec<(u32, u64)>,
    guarded_ticks: u64,
    kos: u32,
}

impl ChainStats {
    fn observe(&mut self, w: &World, evs: &[GameEvent]) {
        if self.open.len() < 256 {
            self.open.resize(256, None);
        }
        let now = w.tick;
        for e in evs {
            match e {
                GameEvent::Hit { target, source, knockback, .. } if *source != Some(*target) && *knockback >= 14.0 => {
                    let slot = &mut self.open[target.0 as usize];
                    *slot = match *slot {
                        // Every hit in one tick (pellets, a blast) is one step of the chain.
                        Some((n, first, last)) if now == last => Some((n, first, last)),
                        Some((n, first, last)) if now - last <= 120 => Some((n + 1, first, now)),
                        Some((n, first, last)) => {
                            self.chains.push((n, last - first));
                            Some((1, now, now))
                        }
                        None => Some((1, now, now)),
                    };
                }
                GameEvent::Ko { .. } => self.kos += 1,
                _ => {}
            }
        }
        for p in &w.players {
            if p.juggle.guarded() {
                self.guarded_ticks += 1;
            }
            let back = !p.alive || (p.mv.grounded && p.mv.tumble <= 0.0) || p.mv.tethered();
            let slot = &mut self.open[p.id.0 as usize];
            if let Some((n, first, last)) = *slot {
                if back || now - last > 120 {
                    self.chains.push((n, last - first));
                    *slot = None;
                }
            }
        }
    }

    fn finish(&mut self) {
        for (n, first, last) in self.open.drain(..).flatten() {
            self.chains.push((n, last - first));
        }
    }

    fn longest(&self) -> u32 {
        self.chains.iter().map(|c| c.0).max().unwrap_or(0)
    }

    fn summary(&self) -> String {
        let mut n: Vec<u32> = self.chains.iter().map(|c| c.0).collect();
        n.sort_unstable();
        let multi = n.iter().filter(|&&k| k >= 2).count();
        let ge3 = n.iter().filter(|&&k| k >= 3).count();
        let ge4 = n.iter().filter(|&&k| k >= 4).count();
        let p99 = n.get(((n.len() as f32 * 0.99) as usize).min(n.len().saturating_sub(1))).copied().unwrap_or(0);
        let longest_secs = self.chains.iter().map(|c| c.1).max().unwrap_or(0) as f32 / 120.0;
        format!(
            "chains={} (>=2 steps: {multi}, >=3: {ge3}, >=4: {ge4}) longest={} hits / {longest_secs:.2} s, p99={p99}, guarded={:.1} player-s, KOs={}",
            n.len(),
            self.longest(),
            self.guarded_ticks as f32 / 120.0,
            self.kos
        )
    }
}

#[test]
fn juggle_off_is_the_old_sim_bit_for_bit() {
    // The pinned FFA run (tests/party.rs, 6 bots, no casts, 40 s) with the protection off must hash
    // exactly as it did before juggle protection existed (FFA_NOCAST_STATE_HASH before this change).
    let mut w = World::new(ffa_cfg(JuggleTuning::off(), 21));
    let mut brains = bots::add_bots(&mut w, 0, 6, 0.8, 21);
    for b in &mut brains {
        b.casts_enabled = false;
    }
    let mut inputs = Vec::new();
    let mut h = Fnv64::default();
    for _ in 0..(40 * 120) {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        for e in w.tick(&inputs) {
            h.write(format!("{e:?}").as_bytes());
        }
    }
    assert_eq!(w.state_hash(), 0xc678_ca58_2062_e205, "juggle off moved the FFA hash: {:#x}", w.state_hash());
}

#[test]
fn bot_brawl_is_deterministic_with_juggle_protection() {
    let a = brawl(JuggleTuning::default(), 5, 6, 20, true);
    let b = brawl(JuggleTuning::default(), 5, 6, 20, true);
    assert_eq!(a.0, b.0);
    assert_eq!(a.1.chains, b.1.chains);
}

#[test]
fn tuning_file_matches_the_rust_defaults() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/combat.ron");
    let t = bd_sim::tuning::CombatTuning::from_ron(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(t.juggle, JuggleTuning::default(), "combat.ron `juggle` = Rust defaults");
}

/// `cargo test -p bd_sim --test juggle -- --ignored --nocapture print_juggle_chains`
#[test]
#[ignore = "prints the before / after juggle-chain measurement"]
fn print_juggle_chains() {
    for casts in [false, true] {
        for (name, t) in [("off", JuggleTuning::off()), ("on ", JuggleTuning::default())] {
            let mut all = ChainStats::default();
            for seed in 1..=8u64 {
                let (_, s) = brawl(t.clone(), seed, 8, 120, casts);
                all.chains.extend(s.chains);
                all.guarded_ticks += s.guarded_ticks;
                all.kos += s.kos;
            }
            println!("casts={casts} juggle {name}: {}", all.summary());
        }
    }
}
