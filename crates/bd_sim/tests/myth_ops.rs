//! Generic move ops of the mythology & folklore wave (docs/toolbox/OPS.md, `bd_sim::powers::ops`):
//! one behaviour assertion (at least) per op — Seeker, Haul, Leap, SelfResize, Surge (Roulette's
//! roll is unit-tested in ops/roulette.rs and exercised through a kit move in `myth_folklore`).

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::powers::ops::{HaulPhase, OpEvent, SeekPhase, SurgePhase};
use bd_sim::powers::{MoveOp, PowerEvent, RecipeDef};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);
const GROUND: f32 = 8.0;

fn world(n: u8) -> World {
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
    evs.iter().filter_map(|e| if let GameEvent::Power(PowerEvent::Op(o)) = e { Some(o.clone()) } else { None }).collect()
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

fn spear() -> RecipeDef {
    RecipeDef { name: "gungnir".into(), form: Form::Spear, mech: vec![Mechanic::Pierce, Mechanic::Returning], element: Element::Electric, scale: 2.2, ..Default::default() }
}

fn seeker(hits: u8) -> MoveOp {
    MoveOp::Seeker { recipe: spear(), range: 50.0, cone: 30.0, speed: 34.0, damage: 14.0, knockback: 16.0, up: 0.2, element: Element::Electric, hits, chain: 30.0 }
}

// ------------------------------------------------------------------------------------ Seeker

#[test]
fn seeker_bends_around_cover_to_strike_and_comes_home() {
    let mut w = world(2);
    // A stone wall squarely between them, taller than both (no straight line exists).
    wall(&mut w, Vec3::new(-5.0, GROUND, -16.0), Vec3::new(5.0, GROUND + 5.0, -14.0));
    let before = w.constructs.len();
    w.fire_op(A, &seeker(1)).expect("seeker launches");
    assert_eq!(w.constructs.len(), before + 1, "the spear is a real construct");
    let evs = run(&mut w, 60 * 4);
    let o = ops(&evs);
    assert!(pct(&w, B) > 5.0, "the spear found B behind the wall ({})", pct(&w, B));
    assert!(o.iter().any(|e| matches!(e, OpEvent::Seek { phase: SeekPhase::Strike, target: Some(t), .. } if *t == B)), "{o:?}");
    // Never through the wall: the wall is still whole.
    assert_ne!(w.voxels.get_at(Vec3::new(0.0, GROUND + 1.0, -15.0)), 0);
    let evs = run(&mut w, 60 * 5);
    assert!(ops(&evs).iter().chain(o.iter()).any(|e| matches!(e, OpEvent::Seek { phase: SeekPhase::Catch, .. })), "it comes back to the hand");
    assert_eq!(w.constructs.len(), before, "caught: the construct is gone");
}

#[test]
fn seeker_chains_through_several_enemies_and_one_throw_at_a_time() {
    let mut w = world(3);
    stand(&mut w, B, -4.0, -20.0);
    stand(&mut w, C, 6.0, -26.0);
    w.fire_op(A, &seeker(3)).unwrap();
    assert!(w.fire_op(A, &seeker(3)).is_err(), "one seeker out at a time");
    run(&mut w, 60 * 4);
    assert!(pct(&w, B) > 5.0 && pct(&w, C) > 5.0, "both struck: {} {}", pct(&w, B), pct(&w, C));
}

#[test]
fn seeker_with_nobody_aimed_flies_straight_and_returns() {
    let mut w = world(2);
    w.player_mut(A).unwrap().input.yaw = std::f32::consts::PI; // look away from B
    w.fire_op(A, &seeker(1)).unwrap();
    let evs = run(&mut w, 60 * 7);
    let o = ops(&evs);
    assert!(o.iter().any(|e| matches!(e, OpEvent::Seek { phase: SeekPhase::Return, .. })));
    assert!(o.iter().any(|e| matches!(e, OpEvent::Seek { phase: SeekPhase::Catch, .. })));
    assert_eq!(pct(&w, B), 0.0);
}

// ------------------------------------------------------------------------------------ Haul

#[test]
fn haul_tears_a_chunk_out_of_the_ground_and_lands_it_at_your_feet() {
    let mut w = world(2);
    stand(&mut w, B, 30.0, 30.0); // out of the way
    // Look down at the ground ~12 m ahead.
    w.player_mut(A).unwrap().input.pitch = -0.14;
    let before = w.constructs.len();
    let op = MoveOp::Haul { range: 40.0, radius: 2.0, speed: 26.0, damage: 10.0 };
    w.fire_op(A, &op).expect("haul bites terrain");
    let evs = run(&mut w, 3);
    let o = ops(&evs);
    let Some(OpEvent::Haul { from, .. }) = o.iter().find(|e| matches!(e, OpEvent::Haul { phase: HaulPhase::Tear, .. })).cloned() else {
        panic!("no tear: {o:?}")
    };
    assert!(from.z < -6.0, "bit the ground ahead: {from}");
    assert_eq!(w.voxels.get_at(from - Vec3::Y * 0.6), 0, "a crater where it bit");
    assert_eq!(w.constructs.len(), before + 1);
    let evs = run(&mut w, 60 * 2);
    assert!(ops(&evs).iter().any(|e| matches!(e, OpEvent::Haul { phase: HaulPhase::Land, .. })));
    let c = w.constructs.iter().last().unwrap();
    assert!(c.gone.is_none() && !c.driven, "handed to physics");
    let d = Vec3::new(c.pos.x, 0.0, c.pos.z).length();
    assert!(d < 7.0, "landed in front of the hauler ({d} m)");
}

#[test]
fn haul_drags_an_aimed_enemy_in() {
    let mut w = world(2);
    stand(&mut w, B, 0.0, -20.0);
    w.fire_op(A, &MoveOp::Haul { range: 40.0, radius: 2.0, speed: 26.0, damage: 6.0 }).unwrap();
    run(&mut w, 30);
    assert!(pos(&w, B).z > -14.0, "B came toward A: {}", pos(&w, B));
}

// ------------------------------------------------------------------------------------ Leap

#[test]
fn leap_lands_on_the_aimed_point() {
    let mut w = world(2);
    stand(&mut w, B, 40.0, 40.0);
    // Aim at the ground ~30 m ahead (eye ≈ 1.6 m up).
    let eye = w.player(A).unwrap().eye(w.params());
    let want = Vec3::new(0.0, GROUND, -30.0);
    let d = (want - eye).normalize();
    w.player_mut(A).unwrap().input.pitch = d.y.asin();
    w.fire_op(A, &MoveOp::Leap { range: 60.0, apex: 6.0 }).expect("leap");
    let evs = run(&mut w, 2);
    let Some(OpEvent::Leap(arc)) = ops(&evs).into_iter().find(|e| matches!(e, OpEvent::Leap(_))) else { panic!("no arc") };
    assert!((arc.to - want).length() < 1.0, "target {}", arc.to);
    // Fly it out; the player lands near the point.
    let mut landed = None;
    for _ in 0..60 * 4 {
        w.tick(&[]);
        let p = w.player(A).unwrap();
        if p.mv.grounded && p.mv.vel.y <= 0.0 && (p.mv.pos - Vec3::new(0.0, GROUND, 0.0)).length() > 3.0 {
            landed = Some(p.mv.pos);
            break;
        }
    }
    let at = landed.expect("landed");
    assert!((Vec3::new(at.x, 0.0, at.z) - Vec3::new(want.x, 0.0, want.z)).length() < 2.5, "landed at {at}");
}

// ------------------------------------------------------------------------------------ SelfResize

#[test]
fn self_resize_grows_you() {
    let mut w = world(2);
    w.fire_op(A, &MoveOp::SelfResize { scale: 1.6, secs: 5.0 }).unwrap();
    run(&mut w, 30);
    assert!(w.player(A).unwrap().size.scale > 1.5);
}

// ------------------------------------------------------------------------------------ Surge

#[test]
fn surge_carries_enemies_along_its_run() {
    let mut w = world(2);
    stand(&mut w, B, 0.0, -10.0);
    let op = MoveOp::Surge { width: 10.0, dist: 30.0, speed: 14.0, push: 16.0, damage: 8.0, element: Element::Water };
    w.fire_op(A, &op).expect("surge");
    let evs = run(&mut w, 60 * 2);
    let o = ops(&evs);
    assert!(o.iter().any(|e| matches!(e, OpEvent::Surge { phase: SurgePhase::Catch, target: Some(t), .. } if *t == B)), "{o:?}");
    assert!(pct(&w, B) > 3.0);
    assert!(pos(&w, B).z < -20.0, "carried along: {}", pos(&w, B));
    let evs = run(&mut w, 60 * 2);
    assert!(ops(&evs).iter().chain(o.iter()).any(|e| matches!(e, OpEvent::Surge { phase: SurgePhase::Break, .. })));
}
