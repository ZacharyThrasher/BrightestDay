//! UNSTICK (world/unstick.rs): the embedded-player pop-out on the host, and the player's unstick
//! request — safest spawn, no damage / score change, 10 s cooldown with queueing, no escape from
//! a combo (3 s damage lockout) unless actually stuck.

use bd_core::input::PlayerInput;
use bd_core::{DT, IVec3, PlayerId, Vec3};
use bd_sim::GameEvent;
use bd_sim::world::unstick::{NO_PROGRESS_SECS, UNSTICK_COOLDOWN, UNSTICK_DAMAGE_LOCKOUT};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
/// Flat lab ground top (m).
const GROUND: f32 = 8.0;

fn secs(s: f32) -> usize {
    (s / DT).round() as usize
}

/// Flat lab, two players spawned and standing, match live.
fn world() -> World {
    world_with(true)
}

fn world_with(check_invariants: bool) -> World {
    let mut w = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 64.0 }, seed: 5, check_invariants, ..SimConfig::default() });
    w.add_player(A, "a".into(), bd_core::corps::Corps::Willpower).unwrap();
    w.add_player(B, "b".into(), bd_core::corps::Corps::Rage).unwrap();
    w.start_live();
    for _ in 0..secs(1.0) {
        w.tick(&[]);
    }
    assert!(w.player(A).unwrap().alive && w.player(B).unwrap().alive);
    w
}

/// Tick `n` times with `input` for A; returns A's unstick events (auto?, to).
fn tick(w: &mut World, n: usize, input: Option<PlayerInput>) -> Vec<(bool, Vec3)> {
    let mut out = Vec::new();
    for _ in 0..n {
        let inputs: Vec<(PlayerId, PlayerInput)> = input.map(|i| (A, i)).into_iter().collect();
        for e in w.tick(&inputs) {
            if let GameEvent::Unstuck { player, to, auto, .. } = e {
                if *player == A {
                    out.push((*auto, *to));
                }
            }
        }
    }
    out
}

/// Wedge `id` — blown up to 4× (past the Grow power's 1.6×, so invariant checks must be off) — inside a tall 3D checkerboard of voxels: too many
/// separate boxes around that big a hull for the controller's per-tick depenetration cache, so it
/// stays embedded: the case the safety net exists for. (Plain burial the controller resolves on
/// its own within a tick or two — `nobody_is_popped_in_normal_play` / the FFA golden hash.)
/// A's % goes up like a hit landed (its spawn shield over).
fn hurt(w: &mut World, pct: f32) {
    let p = w.player_mut(A).unwrap();
    p.spawn_shield = 0.0;
    p.damage_pct += pct;
}

fn bury(w: &mut World, id: PlayerId) {
    let feet = Vec3::new(3.25, GROUND + 0.01, 3.25);
    let c = bd_core::voxel::world_to_voxel(feet);
    for x in c.x - 11..=c.x + 11 {
        for y in c.y..=c.y + 40 {
            for z in c.z - 11..=c.z + 11 {
                if (x + y + z).rem_euclid(2) == 0 {
                    w.voxels.set(IVec3::new(x, y, z), 3);
                }
            }
        }
    }
    let p = w.player_mut(id).unwrap();
    p.size.target = 4.0;
    p.size.scale = 4.0;
    p.size.timer = 60.0;
    p.mv.pos = feet;
    p.mv.vel = Vec3::ZERO;
}

#[test]
fn a_buried_player_is_popped_out_by_the_host() {
    let mut w = world_with(false);
    bury(&mut w, A);
    assert!(w.player_embedded(A));
    tick(&mut w, 12, None);
    assert!(w.player_embedded(A), "the controller alone can't free this body");
    let ev = tick(&mut w, secs(0.5), None);
    assert!(!w.player_embedded(A), "out of the ground at {:?}", w.player(A).unwrap().mv.pos);
    assert!(ev.iter().any(|(auto, _)| *auto), "Unstuck {{ auto }} for the shimmer: {ev:?}");
    let p = w.player(A).unwrap();
    assert!(p.mv.pos.y >= GROUND - 0.01 && p.alive, "free, alive: {:?}", p.mv.pos);
}

#[test]
fn nobody_is_popped_in_normal_play() {
    let mut w = world();
    let walk = PlayerInput { move_z: -1, buttons: bd_core::input::buttons::JUMP, ..PlayerInput::default() };
    let ev = tick(&mut w, secs(8.0), Some(walk));
    assert!(ev.is_empty(), "{ev:?}");
}

#[test]
fn unstick_goes_to_the_safest_spawn_and_keeps_damage_and_score() {
    let mut w = world();
    {
        let p = w.player_mut(A).unwrap();
        p.kos = 3;
        p.deaths = 1;
    }
    let pct = w.player(A).unwrap().damage_pct;
    let want = w.safest_spawn(A);
    let b = w.player(B).unwrap().mv.pos;
    assert!(w.arena.spawns.iter().all(|s| (*s - b).length() <= (want - b).length() + 1e-3), "the spawn farthest from the enemy");
    assert!(w.request_unstick(A));
    let ev = tick(&mut w, 1, None);
    assert_eq!(ev.len(), 1, "fired at once (no cooldown, no damage): {ev:?}");
    assert!(!ev[0].0 && (ev[0].1 - want).length() < 1e-4);
    let p = w.player(A).unwrap();
    assert!((p.mv.pos - want).length() < 0.2, "moved to {want:?}, at {:?}", p.mv.pos);
    assert_eq!((p.kos, p.deaths, p.damage_pct), (3, 1, pct), "no score / % change");
    assert!(w.spawn_clear(want));
}

#[test]
fn unstick_cooldown_queues_the_request_until_ready() {
    let mut w = world();
    assert!(w.request_unstick(A));
    assert_eq!(tick(&mut w, 1, None).len(), 1);
    // Straight away again: queued, not refused.
    assert!(w.request_unstick(A));
    assert!(w.unstick_wait(A) > UNSTICK_COOLDOWN - 0.1);
    assert!(tick(&mut w, secs(UNSTICK_COOLDOWN - 0.5), None).is_empty(), "not during the cooldown");
    assert!(w.player(A).unwrap().unstick.queued, "still queued");
    assert_eq!(tick(&mut w, secs(1.0), None).len(), 1, "fires once the cooldown is over");
    assert!(!w.player(A).unwrap().unstick.queued);
}

#[test]
fn no_unstick_out_of_a_combo_unless_stuck() {
    let mut w = world();
    hurt(&mut w, 20.0);
    tick(&mut w, 1, None);
    assert!(w.request_unstick(A));
    assert!(tick(&mut w, secs(UNSTICK_DAMAGE_LOCKOUT - 0.5), None).is_empty(), "no free escape while being hit");
    assert!(w.player(A).unwrap().unstick.queued);
    assert_eq!(tick(&mut w, secs(1.0), None).len(), 1, "fires when the lockout ends");

    // Stuck inside geometry: allowed right away, even straight after a hit.
    let mut w = world_with(false);
    hurt(&mut w, 20.0);
    tick(&mut w, 1, None);
    bury(&mut w, A);
    tick(&mut w, 2, None); // embedded (the pop-out would free them at 0.25 s)
    assert!(w.player_embedded(A));
    assert!(w.request_unstick(A));
    let ev = tick(&mut w, 1, None);
    assert!(ev.iter().any(|(auto, _)| !auto), "stuck: the lockout doesn't apply: {ev:?}");
}

#[test]
fn pushing_without_progress_counts_as_stuck() {
    let mut w = world();
    // Box A in: a 1-voxel ring of 3 m walls around where it stands.
    let at = w.player(A).unwrap().mv.pos;
    let c = bd_core::voxel::world_to_voxel(at);
    let gy = (GROUND / bd_core::VOXEL_SIZE) as i32;
    for (min, max) in [
        (IVec3::new(c.x - 2, gy, c.z - 2), IVec3::new(c.x - 2, gy + 6, c.z + 2)),
        (IVec3::new(c.x + 2, gy, c.z - 2), IVec3::new(c.x + 2, gy + 6, c.z + 2)),
        (IVec3::new(c.x - 2, gy, c.z - 2), IVec3::new(c.x + 2, gy + 6, c.z - 2)),
        (IVec3::new(c.x - 2, gy, c.z + 2), IVec3::new(c.x + 2, gy + 6, c.z + 2)),
    ] {
        w.voxels.fill_box(min, max, 3);
    }
    assert!(!w.player_embedded(A), "the box leaves room to stand");
    let walk = PlayerInput { move_z: -1, ..PlayerInput::default() };
    // Keep getting hit (the lockout stays on) while pushing into the walls.
    for _ in 0..4 {
        hurt(&mut w, 5.0);
        tick(&mut w, secs(1.0), Some(walk));
    }
    let p = w.player(A).unwrap();
    assert!((p.mv.pos - at).length() < 1.0, "boxed in: {:?} → {:?}", at, p.mv.pos);
    assert!(p.unstick.push_t >= NO_PROGRESS_SECS, "no progress for {} s", p.unstick.push_t);
    assert!(w.request_unstick(A));
    let ev = tick(&mut w, 1, Some(walk));
    assert_eq!(ev.len(), 1, "stuck (no progress) beats the damage lockout: {ev:?}");
}

#[test]
fn dead_players_cannot_request() {
    let mut w = world();
    w.player_mut(A).unwrap().alive = false;
    assert!(!w.request_unstick(A));
    assert!(!w.request_unstick(PlayerId(9)));
}

/// Client prediction runs the same pop-out on the same tick (the timer rides in the replicated
/// movement state), so the host's pop-out is never a correction.
#[test]
fn prediction_pops_out_on_the_same_tick_as_the_host() {
    let mut w = world_with(false);
    bury(&mut w, A);
    tick(&mut w, 1, None);
    let input = PlayerInput::default();
    let mut popped = false;
    for _ in 0..secs(0.6) {
        let mut st = w.player(A).unwrap().mv;
        w.predict_player(A, &mut st, &input);
        let ev = tick(&mut w, 1, Some(input));
        let host = w.player(A).unwrap().mv;
        assert!((st.pos - host.pos).length() < 1e-4, "prediction {:?} vs host {:?}", st.pos, host.pos);
        assert_eq!(st.extra.stuck, host.extra.stuck);
        popped |= !ev.is_empty();
    }
    assert!(popped && !w.player_embedded(A));
}
