//! COMBO TARGET (bd_sim powers::combo_target): the pick rules (cone, line of sight, nearest to the
//! aim, a named player wins), step steering (a teleport-behind lands behind the MOVING target, a
//! strike / projectile step aims at them), the drop rules (chain over, looked away, lost sight), the
//! hold-to-focus key, the wire byte.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::voxel::Aabb;
use bd_core::{PlayerId, TICK_HZ, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::ToolStep;
use bd_sim::powers::combo_target::{self, pick};
use bd_sim::powers::compose::{self, Names, Target};
use bd_sim::powers::toolbox::{MoveIndex, MoveRef};
use bd_sim::powers::{PowerEvent, PowerTable};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);
const GROUND: f32 = 8.0;

fn ticks(secs: f32) -> u32 {
    (secs * TICK_HZ as f32).ceil() as u32
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
        p.willpower = 100.0;
    }
    stand(&mut w, A, 0.0, 0.0);
    stand(&mut w, B, 0.0, -12.0);
    if n >= 3 {
        stand(&mut w, C, 30.0, -12.0);
    }
    w
}

fn stand(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, GROUND, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
}

/// Tick with A's input (look + buttons); everyone else keeps theirs.
fn run_a(w: &mut World, n: u32, yaw: f32, b: u16) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..n {
        let input = PlayerInput { yaw, buttons: b, ..Default::default() };
        all.extend_from_slice(w.tick(&[(A, input)]));
    }
    all
}

fn kit(phrase: &str) -> ToolStep {
    let m: MoveRef = MoveIndex::shipped().read(PowerTable::get(), None, phrase).move_ref().unwrap_or_else(|| panic!("{phrase:?} is not a move"));
    ToolStep::Move { mv: m, kit: true }
}

fn composed(phrase: &str) -> ToolStep {
    let roster = [(B, "Bob"), (C, "Cid")];
    ToolStep::Composed(compose::read(phrase, &Names::new(&roster)).unwrap_or_else(|| panic!("{phrase:?}")).spec)
}

fn hull_at(p: Vec3) -> Aabb {
    Aabb { min: p - Vec3::new(0.4, 0.0, 0.4), max: p + Vec3::new(0.4, 1.8, 0.4) }
}

fn center(w: &World, id: PlayerId) -> Vec3 {
    let p = w.player(id).unwrap();
    p.hull(&w.cfg.move_params).center()
}

/// Fired events (step index, dir) of `player`.
fn fired_dirs(evs: &[GameEvent], player: PlayerId) -> Vec<(u8, Vec3)> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::Power(PowerEvent::Fired { player: p, step, dir, .. }) if *p == player => Some((*step, *dir)),
            _ => None,
        })
        .collect()
}

// ------------------------------------------------------------------------------ pick rules

#[test]
fn pick_cone_line_of_sight_and_nearest_to_aim() {
    let eye = Vec3::new(0.0, 1.6, 0.0);
    let fwd = Vec3::NEG_Z;
    let open = |_: Vec3, _: Vec3| true;
    let at_deg = |deg: f32, d: f32| {
        let r = deg.to_radians();
        Vec3::new(r.sin() * d, 0.7, -r.cos() * d)
    };
    // inside the small cone → picked; outside → nothing
    assert_eq!(pick(eye, fwd, [(B, hull_at(at_deg(6.0, 20.0)))], open), Some(B));
    assert_eq!(pick(eye, fwd, [(B, hull_at(at_deg(25.0, 20.0)))], open), None, "outside the cone");
    // out of range
    assert_eq!(pick(eye, fwd, [(B, hull_at(at_deg(0.0, combo_target::PICK_RANGE + 5.0)))], open), None);
    // no line of sight → nothing
    assert_eq!(pick(eye, fwd, [(B, hull_at(at_deg(3.0, 20.0)))], |_, _| false), None, "a wall hides them");
    // nearest to the aim wins over nearer in distance
    assert_eq!(pick(eye, fwd, [(B, hull_at(at_deg(8.0, 8.0))), (C, hull_at(at_deg(2.0, 30.0)))], open), Some(C));
    // the body under the crosshair wins even when another is closer to the aim line by centre
    assert_eq!(pick(eye, fwd, [(B, hull_at(at_deg(0.5, 40.0))), (C, hull_at(at_deg(0.0, 10.0)))], open), Some(C));
    // deterministic tie: same angle and distance → lower id
    assert_eq!(pick(eye, fwd, [(C, hull_at(at_deg(4.0, 20.0))), (B, hull_at(at_deg(-4.0, 20.0)))], open), Some(B));
}

#[test]
fn a_command_picks_the_enemy_on_the_crosshair() {
    let mut w = arena(3);
    w.toolbox(A, vec![kit("doomfist punch")], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B), "B is dead ahead");
    // off the cone: nothing
    let mut w = arena(3);
    stand(&mut w, B, 9.0, -12.0); // ~37° off
    w.toolbox(A, vec![kit("doomfist punch")], None).unwrap();
    assert_eq!(w.combo_target(A), None);
}

#[test]
fn a_wall_blocks_the_pick() {
    let mut w = arena(2);
    let lo = bd_core::voxel::world_to_voxel(Vec3::new(-3.0, GROUND, -6.5));
    let hi = bd_core::voxel::world_to_voxel(Vec3::new(3.0, GROUND + 5.0, -5.5));
    w.voxels.fill_box(lo, hi, bd_voxel::mat::STONE);
    w.toolbox(A, vec![kit("tracer blink")], None).unwrap_or(());
    assert_eq!(w.combo_target(A), None, "no line of sight, no target");
}

#[test]
fn a_named_player_wins_over_the_crosshair() {
    let mut w = arena(3);
    // crosshair on B; the words name Cid (far off to the side)
    w.toolbox(A, vec![composed("fire punch cid"), kit("doomfist punch")], None).ok();
    let spec_target = match composed("fire punch cid") {
        ToolStep::Composed(s) => s.target,
        _ => unreachable!(),
    };
    assert_eq!(spec_target, Target::Player(C));
    assert_eq!(w.combo_target(A), Some(C), "named wins");
}

// ------------------------------------------------------------------------------ steering

#[test]
fn teleport_behind_lands_behind_the_moving_target() {
    let mut w = arena(2);
    let tele = composed("teleport behind");
    if let ToolStep::Composed(s) = &tele {
        assert_eq!(s.target, Target::Aim, "an untargeted described move");
    }
    w.toolbox(A, vec![kit("tracer blink"), tele], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
    // B strafes hard to the side before the second step fires (out of any aim cone)
    stand(&mut w, B, 8.0, -12.0);
    w.player_mut(B).unwrap().input.yaw = 0.0;
    let mut behind = false;
    for _ in 0..ticks(0.6) {
        run_a(&mut w, 1, 0.0, 0);
        let (a, b) = (w.player(A).unwrap().mv.pos, w.player(B).unwrap().mv.pos);
        let face = {
            let q = w.player(B).unwrap().look();
            Vec3::new(q.x, 0.0, q.z).normalize()
        };
        let rel = a - b;
        if rel.dot(face) < -1.0 && Vec3::new(rel.x, 0.0, rel.z).length() < 4.5 {
            behind = true;
            break;
        }
    }
    assert!(behind, "A ended behind B's new spot: A {:?} B {:?}", w.player(A).unwrap().mv.pos, w.player(B).unwrap().mv.pos);
}

#[test]
fn a_blink_at_the_target_stops_short_of_them() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -5.0);
    w.toolbox(A, vec![kit("tracer blink")], None).ok();
    let (a, b) = (w.player(A).unwrap().mv.pos, w.player(B).unwrap().mv.pos);
    assert!(a.z > b.z, "didn't blink through them: A {a:?} B {b:?}");
}

#[test]
fn later_steps_aim_at_the_target_where_it_is_now() {
    let mut w = arena(2);
    w.toolbox(A, vec![kit("tracer blink"), kit("tracer ult")], None).unwrap();
    // B moves off the look line (A keeps looking straight ahead)
    stand(&mut w, B, 6.0, -14.0);
    let evs = run_a(&mut w, ticks(0.5), 0.0, 0);
    let dirs = fired_dirs(&evs, A);
    let (_, d) = *dirs.iter().find(|f| f.0 >= 1).expect("the second step fired");
    let eye = w.player(A).unwrap().eye(&w.cfg.move_params);
    let want = (center(&w, B) - eye).normalize();
    assert!(d.dot(want) > 0.99, "aimed at B: {d:?} vs {want:?}");
    assert!(d.dot(Vec3::NEG_Z) < 0.97, "not the bare look");
}

// ------------------------------------------------------------------------------ drop rules

#[test]
fn the_target_ends_with_the_chain() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -18.0);
    w.toolbox(A, vec![kit("tracer blink")], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
    run_a(&mut w, ticks(0.6), 0.0, 0);
    assert_eq!(w.combo_target(A), None, "the one-step chain is over");
}

#[test]
fn looking_well_away_drops_it_and_the_rest_fire_on_live_aim() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -18.0);
    w.toolbox(A, vec![kit("tracer blink"), kit("tracer ult")], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
    // flick 90° away (yaw + = turn left: look -X)
    let yaw = std::f32::consts::FRAC_PI_2;
    let evs = run_a(&mut w, ticks(0.5), yaw, 0);
    assert_eq!(w.combo_target(A), None, "looked away");
    let dirs = fired_dirs(&evs, A);
    let (_, d) = *dirs.iter().find(|f| f.0 >= 1).expect("the second step still fired");
    let look = w.player(A).unwrap().look();
    assert!(d.dot(look) > 0.95, "live aim after the drop: {d:?} vs look {look:?}");
}

#[test]
fn a_teleport_that_leaves_them_behind_you_keeps_the_target() {
    let mut w = arena(2);
    // behind them → they're behind your (unchanged) look: not "looking away"
    w.toolbox(A, vec![composed("teleport behind"), kit("doomfist punch"), kit("reinhardt ultimate")], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
    run_a(&mut w, 3, 0.0, 0);
    assert_eq!(w.combo_target(A), Some(B), "still hunting after the teleport");
}

#[test]
fn losing_sight_drops_it_after_the_grace() {
    let mut w = arena(2);
    stand(&mut w, B, 0.0, -20.0);
    w.toolbox(A, vec![kit("tracer blink")], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
    // a wall appears between them; focus held so the chain's end doesn't drop it first
    let lo = bd_core::voxel::world_to_voxel(Vec3::new(-4.0, GROUND - 1.0, -12.5));
    let hi = bd_core::voxel::world_to_voxel(Vec3::new(4.0, GROUND + 8.0, -11.5));
    w.voxels.fill_box(lo, hi, bd_voxel::mat::STONE);
    run_a(&mut w, ticks(0.3), 0.0, buttons::FOCUS);
    assert_eq!(w.combo_target(A), Some(B), "inside the grace");
    run_a(&mut w, ticks(0.35), 0.0, buttons::FOCUS);
    assert_eq!(w.combo_target(A), None, "lost sight > 0.5 s");
}

#[test]
fn a_ko_drops_it() {
    let mut w = arena(2);
    w.toolbox(A, vec![kit("tracer blink"), kit("reinhardt ultimate")], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
    {
        let b = w.player_mut(B).unwrap();
        b.alive = false;
        b.respawn_timer = 5.0;
    }
    run_a(&mut w, 1, 0.0, 0);
    assert_eq!(w.combo_target(A), None);
}

// ------------------------------------------------------------------------------ focus

#[test]
fn focus_keeps_the_target_across_separate_commands() {
    let mut w = arena(3);
    run_a(&mut w, 1, 0.0, buttons::FOCUS);
    stand(&mut w, B, 0.0, -18.0);
    w.toolbox(A, vec![kit("tracer blink")], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
    run_a(&mut w, ticks(0.6), 0.0, buttons::FOCUS);
    assert_eq!(w.combo_target(A), Some(B), "held across the chain's end");
    // a new command with the crosshair on C (turned toward C): the target stays B
    stand(&mut w, C, -20.0, -20.0);
    let to_c = (w.player(C).unwrap().mv.pos - w.player(A).unwrap().mv.pos).normalize();
    let yaw = (-to_c.x).atan2(-to_c.z);
    run_a(&mut w, 1, yaw, buttons::FOCUS);
    w.toolbox(A, vec![kit("tracer ult")], None).ok();
    assert_eq!(w.combo_target(A), Some(B), "focus persists across commands");
    // released: the next command re-picks (C under the crosshair)
    run_a(&mut w, ticks(0.8), yaw, 0);
    assert_eq!(w.combo_target(A), None, "released + chain over");
    w.toolbox(A, vec![kit("doomfist punch")], None).ok();
    assert_eq!(w.combo_target(A), Some(C));
}

#[test]
fn another_command_ends_it_without_focus() {
    let mut w = arena(2);
    w.toolbox(A, vec![kit("tracer blink"), kit("reinhardt ultimate")], None).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
    w.toolbox_interrupt(A);
    assert_eq!(w.combo_target(A), None);
}

// ------------------------------------------------------------------------------ wire

#[test]
fn replicated_and_round_trips() {
    let mut w = arena(2);
    w.toolbox(A, vec![kit("tracer blink"), kit("reinhardt ultimate")], None).unwrap();
    let snap = w.snapshot_for(Some(B));
    let a = snap.players.iter().find(|p| p.id == A.0).unwrap();
    assert_eq!(combo_target::unwire(a.hunt), Some(B));
    let b = snap.players.iter().find(|p| p.id == B.0).unwrap();
    assert_eq!(b.hunt, 0);
    let bytes = bincode::serde::encode_to_vec(a, bincode::config::standard()).unwrap();
    let (back, _): (bd_sim::replicate::PlayerSnap, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(&back, a);
    assert_eq!(combo_target::wire(None), 0);
}

#[test]
fn deterministic() {
    let run = || {
        let mut w = arena(3);
        w.toolbox(A, vec![kit("tracer blink"), composed("teleport behind"), kit("doomfist punch"), kit("reinhardt ultimate")], None).unwrap();
        for k in 0..ticks(3.0) {
            let bi = PlayerInput { move_x: if (k / 60) % 2 == 0 { 1 } else { -1 }, ..Default::default() };
            w.tick(&[(A, PlayerInput::default()), (B, bi)]);
        }
        w.state_hash()
    };
    assert_eq!(run(), run());
}


#[test]
fn a_crosshair_aim_picks_too() {
    let mut w = arena(2);
    let eye = w.player(A).unwrap().eye(&w.cfg.move_params);
    let dir = bd_core::input::look_dir(0.0, -0.05);
    w.toolbox(A, vec![kit("tracer blink"), kit("doomfist punch")], Some(bd_sim::world::Aim::new(eye, dir))).unwrap();
    assert_eq!(w.combo_target(A), Some(B));
}
