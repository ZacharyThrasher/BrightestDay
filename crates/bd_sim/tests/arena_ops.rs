//! Generic move ops of the arena-games wave (docs/toolbox/OPS.md, `bd_sim::powers::ops`): one
//! behaviour assertion (at least) per op — Rewind, Ram, Hook, Grab, Deflect, Counter, LockOn,
//! Lockout, Zone.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::Element;
use bd_core::{PlayerId, Vec3};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::powers::ops::{GrabPhase, HookPhase, LockPhase, OpEvent, RamPhase, StanceKind};
use bd_sim::powers::{At, MoveOp, PowerError, PowerEvent};
use bd_sim::projectile::ProjectileSpawn;
use bd_sim::status::StatusKind;
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn duel() -> World {
    arena(2)
}

fn arena(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=n {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    stand(&mut w, A, 0.0, 0.0);
    stand(&mut w, B, 0.0, -30.0);
    w
}

/// Stand `id` on the ground at (x, z), looking down -Z.
fn stand(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, GROUND, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
    p.damage_pct = 0.0;
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn ops(evs: &[GameEvent]) -> Vec<OpEvent> {
    evs.iter()
        .filter_map(|e| if let GameEvent::Power(PowerEvent::Op(o)) = e { Some(o.clone()) } else { None })
        .collect()
}

fn pct(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().damage_pct
}

fn pos(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().mv.pos
}

fn wall(w: &mut World, a: Vec3, b: Vec3) {
    let lo = bd_core::voxel::world_to_voxel(a.min(b));
    let hi = bd_core::voxel::world_to_voxel(a.max(b));
    w.voxels.fill_box(lo, hi, bd_voxel::mat::STONE);
}

fn ram(pin: bool, through: bool) -> MoveOp {
    MoveOp::Ram { speed: 18.0, secs: 1.6, damage: 8.0, knockback: 22.0, up: 0.3, pin, through, wall: 14.0, pitch: 0.0, steer: 0.0, keep: 0.0, element: Element::Light }
}

// ------------------------------------------------------------------------------------ Rewind

#[test]
fn rewind_takes_you_back_along_your_path_and_heals_back_only() {
    let mut w = duel();
    run(&mut w, 150);
    let home = pos(&w, A);
    // Somewhere else, hurt.
    {
        let p = w.player_mut(A).unwrap();
        p.mv.pos = Vec3::new(12.0, GROUND, 4.0);
        p.damage_pct = 55.0;
    }
    run(&mut w, 30);
    let evs = {
        w.fire_op(A, &MoveOp::Rewind { secs: 1.0 }).unwrap();
        w.events().to_vec()
    };
    let back = pos(&w, A);
    assert!((back - home).length() < 0.6, "rewound to {back:?}, was at {home:?}");
    assert_eq!(pct(&w, A), 0.0, "rewind restores the lower damage % of then");
    assert!(ops(&evs).iter().any(|e| matches!(e, OpEvent::Rewound { player, .. } if *player == A)));
    // Rewinding never ADDS damage: hurt at the old spot, healthy now → stays healthy.
    let mut w = duel();
    w.player_mut(A).unwrap().damage_pct = 80.0;
    run(&mut w, 150);
    w.player_mut(A).unwrap().damage_pct = 10.0;
    run(&mut w, 24);
    w.fire_op(A, &MoveOp::Rewind { secs: 1.2 }).unwrap();
    assert_eq!(pct(&w, A), 10.0);
}

// -------------------------------------------------------------------------------------- Ram

#[test]
fn ram_pin_carries_them_into_the_wall_and_slams() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -3.0);
    wall(&mut w, Vec3::new(-4.0, GROUND, -12.5), Vec3::new(4.0, GROUND + 4.0, -13.5));
    w.fire_op(A, &ram(true, false)).unwrap();
    let evs = run(&mut w, 150);
    let o = ops(&evs);
    let pinned = o.iter().position(|e| matches!(e, OpEvent::Ram { phase: RamPhase::Pinned, target: Some(t), .. } if *t == B));
    let slam = o.iter().position(|e| matches!(e, OpEvent::Ram { phase: RamPhase::Slam, target: Some(t), .. } if *t == B));
    assert!(pinned.is_some(), "B was scooped up: {o:?}");
    assert!(slam.is_some() && slam > pinned, "then slammed into the wall: {o:?}");
    // 25 % of the touch + damage + wall bonus (juggle decay aside) — far more than a plain punch.
    assert!(pct(&w, B) >= 18.0, "slam damage {}", pct(&w, B));
    assert!(pos(&w, A).z < -6.0, "A charged across the gap: {:?}", pos(&w, A));
}

#[test]
fn ram_punch_ends_on_the_first_hit_and_launches() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -4.0);
    w.fire_op(A, &ram(false, false)).unwrap();
    let evs = run(&mut w, 60);
    let o = ops(&evs);
    assert!(o.iter().any(|e| matches!(e, OpEvent::Ram { phase: RamPhase::Hit, target: Some(t), .. } if *t == B)), "{o:?}");
    assert!(pct(&w, B) >= 7.0);
    assert!(w.player(B).unwrap().mv.vel.z < -5.0 || pos(&w, B).z < -6.0, "B was knocked along the charge");
    assert!(!w.op_busy(A), "the punch ended on contact");
}

#[test]
fn a_pinned_player_escapes_with_a_dodge() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -3.0);
    w.fire_op(A, &ram(true, false)).unwrap();
    let mut pinned = false;
    for _ in 0..40 {
        let evs = w.tick(&[]).to_vec();
        if ops(&evs).iter().any(|e| matches!(e, OpEvent::Ram { phase: RamPhase::Pinned, .. })) {
            pinned = true;
            break;
        }
    }
    assert!(pinned);
    let dodge = PlayerInput { buttons: buttons::DODGE, ..w.player(B).unwrap().input };
    let mut escaped = false;
    for _ in 0..6 {
        let evs = w.tick(&[(B, dodge)]).to_vec();
        escaped |= ops(&evs).iter().any(|e| matches!(e, OpEvent::Ram { phase: RamPhase::Escaped, target: Some(t), .. } if *t == B));
    }
    assert!(escaped, "a dodge breaks the carry");
}

#[test]
fn ram_through_strikes_everyone_on_the_path() {
    let mut w = arena(3);
    stand(&mut w, B, 0.3, -3.0);
    stand(&mut w, PlayerId(3), -0.3, -7.0);
    w.fire_op(A, &ram(false, true)).unwrap();
    run(&mut w, 90);
    assert!(pct(&w, B) > 0.0 && pct(&w, PlayerId(3)) > 0.0, "both struck: {} {}", pct(&w, B), pct(&w, PlayerId(3)));
}

// ------------------------------------------------------------------------------------- Hook

#[test]
fn hook_travels_catches_and_tows_them_to_your_feet() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -16.0);
    w.fire_op(A, &MoveOp::Hook { range: 30.0, speed: 40.0, pull: 30.0, damage: 4.0, status: None, secs: 0.0, anchor: false }).unwrap();
    // A skillshot: nothing caught in the first few ticks (it has 16 m to fly).
    let early = run(&mut w, 10);
    assert!(!ops(&early).iter().any(|e| matches!(e, OpEvent::Hook { phase: HookPhase::Caught, .. })));
    let evs = run(&mut w, 140);
    assert!(ops(&evs).iter().any(|e| matches!(e, OpEvent::Hook { phase: HookPhase::Caught, target: Some(t), .. } if *t == B)));
    let d = (pos(&w, B) - pos(&w, A)).length();
    assert!(d < 4.5, "B towed to A's feet ({d} m)");
    assert!(pct(&w, B) >= 3.9);
}

#[test]
fn a_dart_hook_applies_its_status_and_does_not_pull() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -10.0);
    w.fire_op(A, &MoveOp::Hook { range: 30.0, speed: 60.0, pull: 0.0, damage: 2.0, status: Some(StatusKind::Stun), secs: 0.3, anchor: false }).unwrap();
    let mut stunned = false;
    for _ in 0..40 {
        w.tick(&[]);
        stunned |= w.player(B).unwrap().status.stunned();
    }
    assert!(stunned, "the dart puts them to sleep (briefly)");
    assert!(pos(&w, B).z < -9.0, "no pull");
}

#[test]
fn an_anchor_hook_reels_you_to_the_wall() {
    let mut w = duel();
    wall(&mut w, Vec3::new(-4.0, GROUND, -14.5), Vec3::new(4.0, GROUND + 6.0, -15.5));
    stand(&mut w, B, 20.0, 20.0);
    w.fire_op(A, &MoveOp::Hook { range: 30.0, speed: 60.0, pull: 30.0, damage: 0.0, status: None, secs: 0.0, anchor: true }).unwrap();
    let evs = run(&mut w, 120);
    assert!(ops(&evs).iter().any(|e| matches!(e, OpEvent::Hook { phase: HookPhase::Anchored, .. })));
    assert!(pos(&w, A).z < -9.0, "A was reeled to the wall: {:?}", pos(&w, A));
}

// ------------------------------------------------------------------------------------- Grab

#[test]
fn grab_beats_a_guard_and_throws() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -1.6);
    w.fire_op(B, &MoveOp::Counter { secs: 1.0, damage: 0.0, knockback: 0.0, up: 0.0, element: Element::Light }).unwrap();
    w.fire_op(A, &MoveOp::Grab { range: 2.5, hold: 0.3, damage: 12.0, knockback: 20.0, up: 0.4, lift: 0.0, back: false }).unwrap();
    let evs = run(&mut w, 60);
    let o = ops(&evs);
    assert!(o.iter().any(|e| matches!(e, OpEvent::Grab { phase: GrabPhase::Throw, target: Some(t), .. } if *t == B)), "{o:?}");
    assert!(pct(&w, B) >= 11.0, "the throw lands through the guard: {}", pct(&w, B));
}

#[test]
fn a_grab_with_nobody_in_reach_whiffs_and_is_spent() {
    let mut w = duel();
    let r = w.fire_op(A, &MoveOp::Grab { range: 2.5, hold: 0.3, damage: 12.0, knockback: 20.0, up: 0.4, lift: 0.0, back: false });
    assert_eq!(r, Ok(()));
    assert!(ops(w.events()).iter().any(|e| matches!(e, OpEvent::Grab { phase: GrabPhase::Whiff, .. })));
}

// ------------------------------------------------------------------------ Counter / Guard

#[test]
fn counter_negates_the_hit_and_answers_the_attacker() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -2.0);
    w.fire_op(A, &MoveOp::Counter { secs: 0.6, damage: 10.0, knockback: 20.0, up: 0.3, element: Element::Light }).unwrap();
    let a_c = pos(&w, A) + Vec3::Y;
    let r = w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 9.0, 12.0, Vec3::Z, a_c));
    assert!(!r.applied);
    assert_eq!(pct(&w, A), 0.0);
    assert!(pct(&w, B) >= 9.0, "B ate the counter: {}", pct(&w, B));
    assert!(ops(w.events()).iter().any(|e| matches!(e, OpEvent::Parried { kind: StanceKind::Counter, .. })));
    // Used up: the next hit lands.
    let r = w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 5.0, 0.0, Vec3::Z, a_c));
    assert!(r.applied);
}

#[test]
fn a_guard_shrugs_even_explosions_for_its_window_only() {
    let mut w = duel();
    w.fire_op(A, &MoveOp::Counter { secs: 0.4, damage: 0.0, knockback: 0.0, up: 0.0, element: Element::Light }).unwrap();
    let c = pos(&w, A) + Vec3::Y;
    for _ in 0..3 {
        assert!(!w.apply_hit(HitInfo::new(A, Some(B), HitKind::Explosion, 10.0, 10.0, Vec3::Y, c)).applied);
    }
    run(&mut w, 60);
    assert!(w.apply_hit(HitInfo::new(A, Some(B), HitKind::Explosion, 10.0, 10.0, Vec3::Y, c)).applied);
}

// ------------------------------------------------------------------------------------ Deflect

#[test]
fn deflect_sends_a_round_back_along_your_aim() {
    let mut w = duel();
    // (a ring blast fizzles after ~19 m: keep the shooter well inside that)
    stand(&mut w, B, 0.0, -12.0);
    w.fire_op(A, &MoveOp::Deflect { secs: 1.5, angle: 0.0 }).unwrap();
    // B fires a ring blast at A (A looks -Z, straight at B).
    let from = pos(&w, B) + Vec3::Y * 1.5 + Vec3::Z * 1.0;
    w.spawn_projectile(ProjectileSpawn::ring_blast(B, from, Vec3::Z), None).unwrap();
    let evs = run(&mut w, 60);
    assert!(ops(&evs).iter().any(|e| matches!(e, OpEvent::Parried { kind: StanceKind::Deflect, .. })));
    assert_eq!(pct(&w, A), 0.0, "A never got hit");
    assert!(pct(&w, B) > 0.0, "the round came back and hit its shooter");
}

// ------------------------------------------------------------------------------------- LockOn

#[test]
fn lock_on_fires_on_everyone_it_marked_and_scales_with_the_lock() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -12.0);
    w.fire_op(A, &MoveOp::LockOn { secs: 1.5, range: 40.0, damage: 40.0, knockback: 0.0, angle: 0.0 }).unwrap();
    let evs = run(&mut w, 200);
    let o = ops(&evs);
    assert!(o.iter().any(|e| matches!(e, OpEvent::LockOn { phase: LockPhase::Mark, frac, .. } if *frac >= 1.0)), "{o:?}");
    assert!(o.iter().any(|e| matches!(e, OpEvent::LockOn { phase: LockPhase::Fire, hits: 1, .. })));
    assert!(pct(&w, B) >= 35.0, "a full lock is worth everything: {}", pct(&w, B));
    // An early trigger is worth much less.
    let mut w = duel();
    stand(&mut w, B, 0.0, -12.0);
    w.fire_op(A, &MoveOp::LockOn { secs: 1.5, range: 40.0, damage: 40.0, knockback: 0.0, angle: 0.0 }).unwrap();
    run(&mut w, 50);
    let pull = PlayerInput { buttons: buttons::PRIMARY, ..w.player(A).unwrap().input };
    w.tick(&[(A, pull)]);
    run(&mut w, 5);
    assert!(pct(&w, B) > 0.0 && pct(&w, B) < 20.0, "early shot: {}", pct(&w, B));
}

#[test]
fn a_hard_hit_breaks_the_lock_on() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -12.0);
    w.fire_op(A, &MoveOp::LockOn { secs: 1.5, range: 40.0, damage: 40.0, knockback: 0.0, angle: 0.0 }).unwrap();
    run(&mut w, 60);
    let c = pos(&w, A) + Vec3::Y;
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 5.0, 20.0, Vec3::Z, c));
    assert!(ops(w.events()).iter().any(|e| matches!(e, OpEvent::LockOn { phase: LockPhase::Broken, .. })));
    run(&mut w, 200);
    assert_eq!(pct(&w, B), 0.0, "nothing fired");
}

#[test]
fn lock_on_needs_line_of_sight() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -12.0);
    wall(&mut w, Vec3::new(-3.0, GROUND, -6.5), Vec3::new(3.0, GROUND + 4.0, -7.5));
    w.fire_op(A, &MoveOp::LockOn { secs: 1.0, range: 40.0, damage: 40.0, knockback: 0.0, angle: 0.0 }).unwrap();
    run(&mut w, 160);
    assert_eq!(pct(&w, B), 0.0);
}

// ------------------------------------------------------------------------------------ Lockout

#[test]
fn lockout_takes_power_moves_away_for_a_moment_only() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -8.0);
    assert!(w.grant_power(B, "super_saiyan"));
    w.fire_op(A, &MoveOp::Lockout { range: 20.0, secs: 1.5 }).unwrap();
    assert!(w.power_locked(B));
    assert_eq!(w.power_move(B, 1), Err(PowerError::Busy));
    run(&mut w, 200);
    assert!(!w.power_locked(B));
    assert!(w.power_move(B, 1).is_ok() || w.power_move(B, 0).is_ok());
    // Nobody aimed at: nothing happens (and the move isn't spent).
    let mut w = duel();
    stand(&mut w, B, 20.0, 20.0);
    assert_eq!(w.fire_op(A, &MoveOp::Lockout { range: 20.0, secs: 1.5 }), Err(PowerError::NoTarget));
}

// --------------------------------------------------------------------------------------- Zone

#[test]
fn a_zone_slows_and_chills_enemies_but_never_its_owner() {
    let mut w = duel();
    stand(&mut w, B, 2.0, -2.0);
    w.fire_op(A, &MoveOp::Zone { radius: 6.0, secs: 2.0, status: Some(StatusKind::Chill), slow: 0.6, dps: 0.0, lockout: true, at: At::Me, dist: 0.0, follow: true }).unwrap();
    w.player_mut(B).unwrap().mv.vel = Vec3::new(14.0, 0.0, 0.0);
    w.player_mut(A).unwrap().mv.vel = Vec3::new(0.0, 0.0, 0.0);
    run(&mut w, 30);
    let b = w.player(B).unwrap();
    assert!(b.status.active(StatusKind::Chill));
    assert!(w.power_locked(B), "an EMP zone locks power moves inside");
    assert!(!w.player(A).unwrap().status.active(StatusKind::Chill));
    assert!(!w.power_locked(A));
    // Stun zones are refused (never a freeze).
    let mut w = duel();
    stand(&mut w, B, 1.0, -1.0);
    w.fire_op(A, &MoveOp::Zone { radius: 6.0, secs: 2.0, status: Some(StatusKind::Stun), slow: 0.0, dps: 0.0, lockout: false, at: At::Me, dist: 0.0, follow: false }).unwrap();
    run(&mut w, 20);
    assert!(!w.player(B).unwrap().status.stunned());
}

#[test]
fn a_zone_slows_movement_through_it() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -10.0);
    w.fire_op(A, &MoveOp::Zone { radius: 6.0, secs: 3.0, status: None, slow: 0.7, dps: 0.0, lockout: false, at: At::Front, dist: 10.0, follow: false }).unwrap();
    for _ in 0..60 {
        w.player_mut(B).unwrap().mv.vel.x = 12.0;
        w.tick(&[]);
    }
    let vx = w.player(B).unwrap().mv.vel.x;
    assert!(vx < 10.0, "thick air: {vx}");
}
