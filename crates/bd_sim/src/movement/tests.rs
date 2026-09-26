//! Scripted-input scenario tests on hand-built voxel worlds. Nobody can playtest yet, so these pin
//! down the feel contract: speeds, tiers, no tunnelling, no speed loss on steps/landings, determinism.

use std::f32::consts::{FRAC_PI_2, PI};

use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_core::{ConstructId, DT, IVec3, Quat, Vec3};

use super::testkit::*;
use super::*;

fn p() -> MoveParams {
    MoveParams::default()
}

fn floor() -> FnWorld<impl Fn(IVec3) -> bool> {
    FnWorld(|v: IVec3| v.y < 0)
}

/// Yaw that looks along +X.
const YAW_PX: f32 = -FRAC_PI_2;

fn settle<V: VoxelQuery + ?Sized>(s: &mut PlayerState, w: &CollisionWorld<V>, p: &MoveParams) {
    for _ in 0..10 {
        step_player(s, &input(0.0, 0.0, 0, 0, 0), w, p, DT);
    }
}

fn run<V: VoxelQuery + ?Sized>(
    s: &mut PlayerState,
    w: &CollisionWorld<V>,
    p: &MoveParams,
    ticks: usize,
    mut f: impl FnMut(usize, &PlayerState) -> PlayerInput,
    mut each: impl FnMut(usize, &PlayerState, &MoveEvents),
) {
    for t in 0..ticks {
        let i = f(t, s);
        let e = step_player(s, &i, w, p, DT);
        assert!(s.pos.is_finite() && s.vel.is_finite(), "non-finite state at tick {t}: {s:?}");
        each(t, s, &e);
    }
}

// --------------------------------------------------------------------------------------------- ground

#[test]
fn run_speed_and_stop() {
    let p = p();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    assert!(s.grounded);
    let mut t90 = None;
    run(&mut s, &cw, &p, 240, |_, _| input(YAW_PX, 0.0, 0, 1, 0), |t, s, _| {
        if t90.is_none() && s.horizontal_speed() >= 0.9 * 10.0 {
            t90 = Some(t);
        }
    });
    assert!((s.horizontal_speed() - p.run_speed).abs() < 0.05, "run speed {}", s.horizontal_speed());
    assert!(s.vel.x > 9.9 && s.pos.y.abs() < 0.01 && s.grounded);
    let t90 = t90.unwrap() as f32 * DT;
    assert!(t90 < 0.3, "too sluggish: {t90}s to 90% run speed");
    // Stop: snappy, not icy.
    let mut stop_t = None;
    run(&mut s, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |t, s, _| {
        if stop_t.is_none() && s.horizontal_speed() < 0.1 {
            stop_t = Some(t);
        }
    });
    let stop_t = stop_t.expect("never stopped") as f32 * DT;
    assert!(stop_t < 0.6, "stop took {stop_t}s");
}

#[test]
fn landing_never_zeroes_speed() {
    let p = p();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 10.0, 0.0), &p);
    s.vel = Vec3::new(12.0, 0.0, 0.0);
    let mut landed = None;
    run(&mut s, &cw, &p, 200, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |_, s, e| {
        if let Some(l) = e.landed {
            if landed.is_none() {
                landed = Some((l.impact_speed, s.horizontal_speed()));
            }
        }
    });
    let (impact, hs) = landed.expect("never landed");
    assert!(impact > 20.0, "impact speed {impact}");
    assert!(hs > 11.99, "landing ate speed: {hs}");
}

#[test]
fn step_up_single_voxel_ledge_keeps_speed() {
    let p = p();
    // Floor, then a 0.5 m ledge from x = 5 m on.
    let w = FnWorld(|v: IVec3| v.y < 0 || (v.y == 0 && v.x >= 10));
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    s.vel = Vec3::new(p.run_speed, 0.0, 0.0);
    let mut min_hs = f32::MAX;
    let mut stepped = 0.0;
    let mut ungrounded = 0;
    run(&mut s, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 1, 0), |_, s, e| {
        min_hs = min_hs.min(s.horizontal_speed());
        stepped += e.stepped;
        ungrounded += (!s.grounded) as i32;
        assert!(e.wall_hit.is_none(), "step reported as wall hit");
    });
    assert!(min_hs > p.run_speed - 0.02, "speed dipped to {min_hs} on the step");
    assert!((s.pos.y - 0.5).abs() < 0.01, "didn't end on the ledge: {}", s.pos.y);
    assert!((stepped - 0.5f32).abs() < 0.02, "stepped event sum {stepped}");
    assert_eq!(ungrounded, 0, "left the ground while stepping");

    // And back down: run off the ledge (step-down snap keeps you grounded, no speed loss).
    let w2 = FnWorld(|v: IVec3| v.y < 0 || (v.y == 0 && v.x < 10));
    let cw2 = CollisionWorld::new(&w2, &[]);
    let mut s = PlayerState::new(Vec3::new(2.0, 0.51, 0.0), &p);
    settle(&mut s, &cw2, &p);
    s.vel = Vec3::new(p.run_speed, 0.0, 0.0);
    let mut min_hs = f32::MAX;
    run(&mut s, &cw2, &p, 90, |_, _| input(YAW_PX, 0.0, 0, 1, 0), |_, s, _| {
        min_hs = min_hs.min(s.horizontal_speed());
        assert!(s.grounded, "hopped off a single step");
    });
    assert!(min_hs > p.run_speed - 0.02 && s.pos.y.abs() < 0.01);
}

#[test]
fn airborne_lip_assist_keeps_hop_speed() {
    // Falling into a 1-voxel lip mid-hop steps onto it instead of dead-stopping.
    let p = p();
    let w = FnWorld(|v: IVec3| v.y < 0 || (v.y <= 1 && v.x >= 10));
    let cw = CollisionWorld::new(&w, &[]);
    // Hull front 5 cm from the lip face, feet 8 cm below the lip top, falling.
    let mut s = PlayerState::new(Vec3::new(4.55, 0.92, 0.0), &p);
    s.vel = Vec3::new(16.0, -1.0, 0.0);
    let mut stepped = 0.0;
    let mut min_hs = f32::MAX;
    run(&mut s, &cw, &p, 40, |_, _| input(YAW_PX, 0.0, 0, 0, JUMP), |_, s, e| {
        stepped += e.stepped;
        min_hs = min_hs.min(s.horizontal_speed());
    });
    assert!(s.pos.x > 6.0 && s.pos.y > 0.99, "stuck at lip: {:?}", s.pos);
    assert!(stepped > 0.05, "no lip assist");
    // Only the auto-hop friction tax is paid, never a wall stop.
    assert!(min_hs > 15.5, "lip ate speed: {min_hs}");
}

#[test]
fn inside_corner_is_stable() {
    let p = p();
    let w = FnWorld(|v: IVec3| v.y < 0 || v.x >= 10 || v.z <= -10);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    let yaw = yaw_for_wish(Vec3::new(1.0, 0.0, -1.0).normalize(), 0, 1);
    let mut last = Vec::new();
    run(&mut s, &cw, &p, 240, |_, _| input(yaw, 0.0, 0, 1, 0), |t, s, _| {
        if t >= 220 {
            last.push(s.pos);
        }
    });
    for w2 in last.windows(2) {
        assert!((w2[1] - w2[0]).length() < 1e-4, "jitter in corner: {:?} -> {:?}", w2[0], w2[1]);
    }
    assert!(s.pos.x <= 4.6 + 1e-4 && s.pos.z >= -4.6 - 1e-4);
    assert!(s.grounded);
}

#[test]
fn construct_wedge_corner_is_stable() {
    // Two rotated construct walls forming a 60° V; run into the crease.
    let p = p();
    let w = floor();
    let a = Collider::new(Vec3::new(6.0, 1.5, -1.5), Vec3::new(0.2, 1.5, 3.0), Quat::from_rotation_y(0.52), ColliderId::Other(1));
    let b = Collider::new(Vec3::new(6.0, 1.5, 1.5), Vec3::new(0.2, 1.5, 3.0), Quat::from_rotation_y(-0.52), ColliderId::Other(2));
    let cols = [a, b];
    let cw = CollisionWorld::new(&w, &cols);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    let mut last = Vec::new();
    run(&mut s, &cw, &p, 240, |_, _| input(YAW_PX, 0.0, 0, 1, 0), |t, s, _| {
        for c in &cols {
            let l = c.to_local(s.hull(&p).center());
            assert!(l.x.abs() > c.half.x - 1e-3 || l.z.abs() > c.half.z || l.y.abs() > c.half.y, "inside wedge wall");
        }
        if t >= 200 {
            last.push(s.pos);
        }
    });
    let spread = last.iter().fold(0.0f32, |m, p0| m.max((*p0 - last[0]).length()));
    assert!(spread < 0.05, "wedge jitter {spread}");
    assert!(s.grounded);
}

#[test]
fn non_anchorable_collider_blocks_hooks() {
    let p = p();
    let w = FnWorld(|v: IVec3| v.z < -80);
    let mut shield = Collider::new(Vec3::new(0.0, 1.6, -10.0), Vec3::new(3.0, 3.0, 0.1), Quat::IDENTITY, ColliderId::Other(4));
    shield.anchorable = false;
    let cols = [shield];
    let cw = CollisionWorld::new(&w, &cols);
    let eye = Vec3::new(0.0, 1.6, 0.0);
    assert!(query_anchor(&cw, eye, Vec3::NEG_Z, &p).is_none(), "anchor through a non-anchorable wall");
    let mut s = PlayerState::new(Vec3::ZERO, &p);
    s.mods.gravity_scale = 0.0;
    let mut missed = false;
    run(&mut s, &cw, &p, 30, |_, _| input(0.0, 0.0, 0, 0, TETHER_L), |_, _, e| missed |= e.tether_missed[0]);
    assert!(missed && !s.tethered());
}

#[test]
fn no_tunnelling_at_hard_cap() {
    let p = p();
    // One-voxel-thick wall at x ∈ [10, 10.5].
    let w = FnWorld(|v: IVec3| v.x == 20 && (-4..20).contains(&v.y) && (-10..10).contains(&v.z));
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 2.0, 0.3), &p);
    s.mods.gravity_scale = 0.0;
    s.vel = Vec3::new(70.0, 0.0, 0.0);
    let mut hit = None;
    run(&mut s, &cw, &p, 60, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |_, s, e| {
        assert!(s.pos.x + 0.4 <= 10.0 + 1e-3, "tunnelled: {:?}", s.pos);
        if e.wall_hit.is_some() {
            hit = e.wall_hit;
        }
    });
    let hit = hit.expect("no wall_hit event");
    // (The soft cap has bled a little of the 70 m/s by the time we arrive.)
    assert!(hit.speed > 60.0 && hit.normal.x < -0.99, "{hit:?}");

    // Thin rotated construct wall (5 cm) at 70 m/s diagonally.
    let wall = Collider::new(Vec3::new(8.0, 2.0, 0.0), Vec3::new(0.025, 5.0, 10.0), Quat::from_rotation_y(0.5), ColliderId::Other(1));
    let empty = FnWorld(|_| false);
    let cols = [wall];
    let cw = CollisionWorld::new(&empty, &cols);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.0, 0.0), &p);
    s.mods.gravity_scale = 0.0;
    s.vel = Vec3::new(60.0, 0.0, 30.0).normalize() * 70.0;
    run(&mut s, &cw, &p, 60, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |_, s, _| {
        let local = wall.to_local(s.hull(&p).center());
        assert!(local.x < 0.0, "passed through thin construct wall: {local:?}");
    });

    // Vertical: 70 m/s fall onto a single-voxel floor with nothing under it.
    let w = FnWorld(|v: IVec3| v.y == -1 && (-10..10).contains(&v.x) && (-10..10).contains(&v.z));
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 30.0, 0.0), &p);
    s.vel = Vec3::new(0.0, -70.0, 0.0);
    run(&mut s, &cw, &p, 120, |_, _| input(0.0, 0.0, 0, 0, 0), |_, s, _| assert!(s.pos.y > -1e-3, "fell through: {:?}", s.pos));
    assert!(s.grounded);
}

#[test]
fn crouch_jump_reaches_higher_ledge() {
    let p = p();
    // 2.0 m ledge starting at x = 1.5 m.
    let w = FnWorld(|v: IVec3| v.y < 0 || (v.y < 4 && v.x >= 3));
    let cw = CollisionWorld::new(&w, &[]);
    let attempt = |crouch: bool| {
        let mut s = PlayerState::new(Vec3::new(-8.0, 0.01, 0.0), &p);
        settle(&mut s, &cw, &p);
        let jump_t = std::cell::Cell::new(None);
        run(&mut s, &cw, &p, 240, |t, s| {
            if jump_t.get().is_none() && s.pos.x >= -1.4 {
                jump_t.set(Some(t));
            }
            let mut b = 0;
            if let Some(j) = jump_t.get() {
                if t < j + 2 {
                    b |= JUMP;
                }
                if crouch && (j + 4..j + 60).contains(&t) {
                    b |= CROUCH;
                }
            }
            input(YAW_PX, 0.0, 0, 1, b)
        }, |_, _, _| {});
        s.pos.y
    };
    assert!(attempt(false) < 0.1, "standing jump should not reach a 2 m ledge");
    assert!((attempt(true) - 2.0).abs() < 0.02, "crouch-jump should reach the 2 m ledge");
}

// ----------------------------------------------------------------------------------------- air & hop

/// Bhop with jump held (auto-hop) and perfect strafing, circling right.
fn bhop_session(p: &MoveParams, secs: f32, perfect_timing: bool) -> (f32, f32) {
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), p);
    settle(&mut s, &cw, p);
    s.vel = Vec3::new(0.0, 0.0, -p.run_speed);
    let start = s.horizontal_speed();
    let mut airborne_ticks = 0;
    run(&mut s, &cw, p, (secs / DT) as usize, |t, s| {
        let yaw = optimal_strafe_yaw(s.vel, p, DT, 1, 0, 1.0);
        let jump = if perfect_timing {
            // Tap jump a few ticks before each landing (scroll-wheel style).
            if s.grounded || (s.vel.y < 0.0 && s.pos.y < 0.4) || t < 2 { JUMP } else { 0 }
        } else {
            JUMP
        };
        let jump = if perfect_timing && t % 2 == 1 { 0 } else { jump };
        input(yaw, 0.0, 1, 0, jump)
    }, |_, s, _| airborne_ticks += (!s.grounded) as usize);
    assert!(airborne_ticks as f32 * DT > secs * 0.9, "spent too long on the ground");
    (start, s.horizontal_speed())
}

#[test]
fn bhop_held_jump_with_strafe_gains_speed() {
    let p = p();
    let (start, end) = bhop_session(&p, 6.0, false);
    assert!(end > start + 5.0, "held-jump bhop should gain: {start} -> {end}");
}

#[test]
fn perfect_hops_beat_held_hops() {
    let p = p();
    let (_, held) = bhop_session(&p, 6.0, false);
    let (_, timed) = bhop_session(&p, 6.0, true);
    assert!(timed > held, "timed {timed} vs held {held}");
}

#[test]
fn human_air_strafe_circle_gains_speed() {
    // A constant 150°/s turn holding D + jump (no per-tick optimisation) must still gain speed.
    let p = p();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    s.vel = Vec3::new(0.0, 0.0, -12.0);
    let rate = 150f32.to_radians();
    // Start with the strafe direction ~80° clockwise of the velocity.
    let a = 80f32.to_radians();
    let yaw0 = yaw_for_wish(Vec3::new(a.sin(), 0.0, -a.cos()), 1, 0);
    run(&mut s, &cw, &p, 5 * 120, |t, _| input(yaw0 - rate * t as f32 * DT, 0.0, 1, 0, JUMP), |_, _, _| {});
    assert!(s.horizontal_speed() > 14.0, "human circle strafe: {}", s.horizontal_speed());
}

#[test]
fn pure_air_strafe_without_ground_gains_speed() {
    // Zero-g air strafing, no ground contact at all: the air-accel math alone gains speed.
    let p = p();
    let w = FnWorld(|_| false);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::ZERO, &p);
    s.mods.gravity_scale = 0.0;
    s.vel = Vec3::new(0.0, 0.0, -10.0);
    run(&mut s, &cw, &p, 4 * 120, |_, s| input(optimal_strafe_yaw(s.vel, &p, DT, 1, 0, 1.0), 0.0, 1, 0, 0), |_, _, _| {});
    let v = s.horizontal_speed();
    assert!(v > 17.0, "4 s of perfect strafing only reached {v}");
    // W-only in the air adds nothing beyond the cap (no free forward air speed).
    let mut s2 = PlayerState::new(Vec3::ZERO, &p);
    s2.mods.gravity_scale = 0.0;
    s2.vel = Vec3::new(0.0, 0.0, -10.0);
    run(&mut s2, &cw, &p, 240, |_, _| input(0.0, 0.0, 0, 1, 0), |_, _, _| {});
    assert!((s2.horizontal_speed() - 10.0).abs() < 1e-3);
}

// -------------------------------------------------------------------------------------------- slide

#[test]
fn slide_burst_and_cooldown() {
    let p = p();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    s.vel = Vec3::new(p.run_speed, 0.0, 0.0);
    let e = step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, CROUCH), &cw, &p, DT);
    assert!(e.slide_start && e.slide_burst && s.sliding);
    assert!(s.horizontal_speed() > p.run_speed + p.slide_burst - 0.2, "burst: {}", s.horizontal_speed());
    // Slide decays slowly (much slower than friction).
    run(&mut s, &cw, &p, 60, |_, _| input(YAW_PX, 0.0, 0, 1, CROUCH), |_, _, _| {});
    assert!(s.sliding && s.horizontal_speed() > 12.5, "slide decayed too fast: {}", s.horizontal_speed());
    // Stand up, crouch again right away: slide but no burst (cooldown).
    step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, 0), &cw, &p, DT);
    let before = s.horizontal_speed();
    let e = step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, CROUCH), &cw, &p, DT);
    assert!(e.slide_start && !e.slide_burst && s.horizontal_speed() <= before);
}

fn descending_ramp(angle_deg: f32) -> Collider {
    // Ascends toward −X (yaw +π/2), low edge at x = 40, y = 0.
    Collider::ramp(Vec3::new(40.0, 0.0, 0.0), FRAC_PI_2, angle_deg.to_radians(), 40.0, 8.0, 1.0, ColliderId::Other(7))
}

#[test]
fn slide_down_ramp_gains_speed() {
    let p = p();
    let ramp = descending_ramp(25.0);
    // 2 m down-slope from the top edge, resting on the surface (the AABB's uphill bottom edge touches),
    // already moving down-slope at run speed, crouched.
    let a = 25f32.to_radians();
    let down = Vec3::new(a.cos(), -a.sin(), 0.0);
    let start = ramp.to_world(Vec3::new(0.0, ramp.half.y, -ramp.half.z + 2.0)) + Vec3::new(0.0, 0.4 * a.tan() + 0.01, 0.0);
    let w = FnWorld(|v: IVec3| v.y < -40);
    let cols = [ramp];
    let cw = CollisionWorld::new(&w, &cols);
    let mut s = PlayerState::new(start, &p);
    s.crouched = true;
    s.vel = down * 10.0;
    let mut sliding_ticks = 0;
    run(&mut s, &cw, &p, 180, |_, _| input(YAW_PX, 0.0, 0, 1, CROUCH), |_, s, _| sliding_ticks += s.sliding as i32);
    assert!(sliding_ticks > 160, "didn't slide: {sliding_ticks}");
    let v = s.speed();
    assert!(v > 22.0, "ramp slide only reached {v}");
    // And running (no crouch) down the same ramp stays at run speed (walkable ramps don't accelerate you).
    let mut s = PlayerState::new(start, &p);
    s.vel = down * 10.0;
    run(&mut s, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 1, 0), |_, _, _| {});
    assert!(s.grounded && (s.speed() - p.run_speed).abs() < 0.3, "run down ramp {}", s.speed());
}

#[test]
fn slide_down_voxel_stairs_gains_speed() {
    let p = p();
    // Staircase descending toward +X: each 1 m horizontally drops 0.5 m.
    let w = FnWorld(|v: IVec3| v.y < -(v.x.max(0) / 2) || v.y < -40);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(-2.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    s.vel = Vec3::new(p.run_speed, 0.0, 0.0);
    run(&mut s, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 1, CROUCH), |_, _, _| {});
    assert!(s.sliding, "slide ended on stairs");
    assert!(s.horizontal_speed() > 16.0, "stair slide: {}", s.horizontal_speed());
}

// --------------------------------------------------------------------------------------- ramps/surf

#[test]
fn ramp_launch() {
    let p = p();
    // Ramp ascending toward +X, ~18°, 8 m of slope, cresting at 2.5 m onto a voxel plateau.
    let a = (2.5f32 / 8.0).asin();
    let ramp = Collider::ramp(Vec3::new(5.0, 0.0, 0.0), -FRAC_PI_2, a, 8.0, 6.0, 1.0, ColliderId::Other(3));
    let top_y = 2.5;
    let crest_x = 5.0 + 8.0 * a.cos();
    let w = FnWorld(|v: IVec3| v.y < 0 || (v.y < 5 && (25..80).contains(&v.x) && (-6..6).contains(&v.z)));
    let cols = [ramp];
    let cw = CollisionWorld::new(&w, &cols);
    let mut s = PlayerState::new(Vec3::new(2.0, 0.3, 0.0), &p);
    s.vel = Vec3::new(26.0, 0.0, 0.0);
    let mut max_y = 0.0f32;
    let mut crest_vy = 0.0;
    run(&mut s, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |_, s, _| {
        max_y = max_y.max(s.pos.y);
        if crest_vy == 0.0 && s.pos.x > crest_x - 0.5 {
            crest_vy = s.vel.y;
        }
        if s.pos.x < crest_x {
            assert!(!s.grounded || s.pos.y < 0.1, "grounded (friction!) while ramp-sliding at speed: {:?} {:?}", s.pos, s.vel);
        }
    });
    // Hitting the ramp clips the into-ramp part (Source-style), the rest carries you up and off.
    assert!(crest_vy > 4.5, "vy at the crest {crest_vy}");
    assert!(max_y > top_y + 0.6, "ramp launch apex {max_y} (ramp top {top_y})");

    // Same ramp at run speed: you walk up and over it, no launch.
    let mut s = PlayerState::new(Vec3::new(2.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    let mut max_air = 0;
    let mut air = 0;
    run(&mut s, &cw, &p, 240, |_, _| input(YAW_PX, 0.0, 0, 1, 0), |_, s, _| {
        air = if s.grounded { 0 } else { air + 1 };
        max_air = max_air.max(air);
    });
    assert!(max_air <= 2, "walking over a ramp crest went airborne for {max_air} ticks");
}

#[test]
fn surf_steep_ramp_holds_player() {
    let p = p();
    // A 60° surf ramp running along X; its face looks toward −Z.
    let ramp = Collider::ramp(Vec3::new(0.0, 0.0, 0.0), PI, 60f32.to_radians(), 12.0, 400.0, 1.0, ColliderId::Other(9));
    let n = ramp.axes()[1];
    assert!((n.y - 0.5).abs() < 1e-3 && n.z < -0.8);
    let w = FnWorld(|_| false);
    let cols = [ramp];
    let cw = CollisionWorld::new(&w, &cols);
    let p0 = ramp.to_world(Vec3::new(0.0, ramp.half.y, 0.0));
    // Put the hull's (+z, −y) bottom edge 1 cm off the face.
    let mut s = PlayerState::new(Vec3::ZERO, &p);
    let corner = Vec3::new(0.0, 0.0, 0.4);
    s.pos = p0 - corner + n * 0.01 + Vec3::new(-50.0, 0.0, 0.0);
    s.vel = Vec3::new(20.0, 0.0, 0.0);
    let y0 = s.pos.y;
    let mut contact_ticks = 0;
    run(&mut s, &cw, &p, 240, |_, _| input(YAW_PX, 0.0, 1, 0, 0), |_, s, _| {
        let c = s.pos + corner;
        let d = (c - p0).dot(n);
        if d < 0.05 {
            contact_ticks += 1;
        }
        assert!(d > -1e-3, "inside the ramp");
        assert!(!s.grounded, "grounded on a surf ramp");
    });
    assert!(contact_ticks > 230, "fell off the surf ramp ({contact_ticks} contact ticks)");
    assert!(s.vel.x > 19.9, "surf lost speed: {}", s.vel.x);
    assert!((s.pos.y - y0).abs() < 4.0, "surf height drift {}", s.pos.y - y0);

    // Without pressing into the ramp you slide down it (and off the bottom).
    let mut s2 = PlayerState::new(s.pos, &p);
    s2.pos = p0 - corner + n * 0.01 + Vec3::new(-50.0, 0.0, 0.0);
    s2.vel = Vec3::new(20.0, 0.0, 0.0);
    run(&mut s2, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |_, _, _| {});
    assert!(s2.pos.y < y0 - 5.0, "no-input surf should slide down: {}", s2.pos.y - y0);
}

// ------------------------------------------------------------------------------------------ tethers

/// Floor at y = 0 plus a big anchor block with its −X face at x = 20, y ∈ [30, 32], z ∈ [-20, 20].
fn swing_world() -> FnWorld<impl Fn(IVec3) -> bool> {
    FnWorld(|v: IVec3| v.y < 0 || ((40..44).contains(&v.x) && (60..64).contains(&v.y) && (-40..40).contains(&v.z)))
}

#[test]
fn tether_swing_and_release_keeps_momentum() {
    let p = p();
    let w = swing_world();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 15.0, 0.0), &p);
    s.vel = Vec3::new(12.0, 0.0, 0.0);
    let target = Vec3::new(20.0, 30.5, 0.0);
    let (yaw, pitch) = aim_at(s.eye(&p), target);
    let attached_at = std::cell::Cell::new(None);
    let mut peak = 0.0f32;
    let released = std::cell::Cell::new(None);
    let mut release_speed = 0.0;
    let mut after = Vec::new();
    run(&mut s, &cw, &p, 360, |t, s| {
        let hold = released.get().is_none() && !(attached_at.get().is_some() && s.vel.y > 4.0 && s.pos.y > 8.0);
        let _ = t;
        input(yaw, pitch, 0, 0, if hold { TETHER_L } else { 0 })
    }, |t, s, e| {
        if e.tether_attached[0] {
            attached_at.set(Some(t));
        }
        if s.hooks[0].attached() {
            peak = peak.max(s.speed());
            let d = (s.hooks[0].pos - s.chest(&p)).length();
            assert!(d <= s.hooks[0].rope_len + 0.05, "rope stretched: {d} > {}", s.hooks[0].rope_len);
        }
        if e.tether_released[0] {
            released.set(Some(t));
            release_speed = s.speed();
        }
        if let Some(r) = released.get() {
            if t == r + 48 {
                after.push(s.speed());
            }
        }
    });
    let at = attached_at.get().expect("hook never attached") as f32 * DT;
    assert!(at < 0.2, "hook travel took {at}s");
    // A plain pendulum from 15 m at 12 m/s, no reel: ~7 m of drop plus the gentle pull.
    assert!(peak > 22.0, "swing peak only {peak}");
    assert!(released.get().is_some() && release_speed > 18.0, "release speed {release_speed}");
    // 0.4 s after release: no drag on release — only gravity acts (speed can only change by g·t·sinθ).
    let a = after[0];
    assert!(a > release_speed * 0.8, "post-release {a} vs release {release_speed}");
}

#[test]
fn tether_rope_ratchets_and_never_lengthens() {
    let p = p();
    let w = swing_world();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(10.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    let (yaw, pitch) = aim_at(s.eye(&p), Vec3::new(20.0, 30.5, 0.0));
    let mut lens = Vec::new();
    run(&mut s, &cw, &p, 240, |t, _| {
        let mx = if t < 120 { 0 } else { 0 };
        input(yaw, pitch, mx, if t > 20 { 1 } else { 0 }, TETHER_L | if t > 20 && t < 60 { JUMP } else { 0 })
    }, |_, s, _| {
        if s.hooks[0].attached() {
            lens.push(s.hooks[0].rope_len);
        }
    });
    assert!(!lens.is_empty());
    for w2 in lens.windows(2) {
        assert!(w2[1] <= w2[0] + 1e-5, "rope lengthened {} -> {}", w2[0], w2[1]);
    }
}

#[test]
fn reel_pulls_in_fast() {
    let p = p();
    let w = swing_world();
    let cw = CollisionWorld::new(&w, &[]);
    let dist_after = |reel: bool, notches: i8| {
        let mut s = PlayerState::new(Vec3::new(-10.0, 0.01, 0.0), &p);
        settle(&mut s, &cw, &p);
        let (yaw, pitch) = aim_at(s.eye(&p), Vec3::new(20.0, 30.5, 0.0));
        run(&mut s, &cw, &p, 120, |t, _| {
            let mut i = input(yaw, pitch, 0, 0, TETHER_L | if reel { REEL } else { 0 });
            if t % 12 == 0 && t > 0 {
                i.reel_notches = notches;
            }
            i
        }, |_, _, _| {});
        (s.hooks[0].pos - s.chest(&p)).length()
    };
    let start = (Vec3::new(20.0, 30.5, 0.0) - Vec3::new(-10.0, 1.08, 0.0)).length();
    let hold = dist_after(false, 0);
    let reel = dist_after(true, 0);
    let wheel = dist_after(false, 1);
    let msg = format!("start {start} hold {hold} reel {reel} wheel {wheel}");
    assert!(reel < start - 10.0, "{msg}");
    assert!(reel < hold - 6.0, "{msg}");
    assert!(wheel < hold - 3.0, "{msg}");
}

#[test]
fn dual_tether_blends_and_holds() {
    let p = p();
    // Two anchor pillars ahead-left and ahead-right.
    let w = FnWorld(|v: IVec3| {
        v.y < 0 || ((60..62).contains(&v.x) && (0..80).contains(&v.y) && ((-42..-40).contains(&v.z) || (40..42).contains(&v.z)))
    });
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 12.0, 0.0), &p);
    s.vel = Vec3::new(14.0, 0.0, 0.0);
    let al = Vec3::new(30.0, 34.0, -20.0);
    let ar = Vec3::new(30.0, 34.0, 20.0);
    let mut both = 0;
    let mut peak = 0.0f32;
    run(&mut s, &cw, &p, 180, |t, s| {
        let target = if t < 10 { al } else { ar };
        let (yaw, pitch) = aim_at(s.eye(&p), target);
        let b = TETHER_L | if t >= 10 { TETHER_R } else { 0 };
        input(yaw, pitch, 0, 0, b)
    }, |_, s, _| {
        if s.hooks[0].attached() && s.hooks[1].attached() {
            both += 1;
            peak = peak.max(s.speed());
            for h in &s.hooks {
                let d = (h.pos - s.chest(&p)).length();
                assert!(d <= h.rope_len + 0.05, "dual rope stretched {d} > {}", h.rope_len);
            }
        }
    });
    assert!(both > 100, "both hooks attached for only {both} ticks");
    assert!(peak > 20.0, "dual swing peak {peak}");
    // Symmetric anchors: lateral drift stays small (blend, not a tug-of-war).
    assert!(s.pos.z.abs() < 3.0, "dual tether pulled sideways: z = {}", s.pos.z);
}

#[test]
fn hook_misses_out_of_range_and_breaks_on_carved_anchor() {
    let p = p();
    let w = FnWorld(|v: IVec3| v.y < 0);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    let mut missed = false;
    run(&mut s, &cw, &p, 90, |_, _| input(0.0, 0.3, 0, 0, TETHER_R), |_, _, e| missed |= e.tether_missed[1]);
    assert!(missed && s.hooks[1].phase == HookPhase::Idle);

    // Anchor on a moving construct part that then disappears.
    let c = Collider::new(Vec3::new(0.0, 20.0, -20.0), Vec3::splat(1.0), Quat::IDENTITY, ColliderId::Construct { id: ConstructId(5), part: 0 })
        .with_vel(Vec3::new(3.0, 0.0, 0.0));
    let mut cols = vec![c];
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &CollisionWorld::new(&w, &cols), &p);
    let (yaw, pitch) = aim_at(s.eye(&p), c.center);
    let mut ev = MoveEvents::default();
    for t in 0..60 {
        let v = cols[0].vel;
        cols[0].center += v * DT;
        let cw = CollisionWorld::new(&w, &cols);
        ev = step_player(&mut s, &input(yaw, pitch, 0, 0, TETHER_L), &cw, &p, DT);
        if t > 20 {
            assert!(s.hooks[0].attached());
            let expect = cols[0].to_world(s.hooks[0].local);
            assert!((s.hooks[0].pos - expect).length() < 1e-4, "anchor didn't follow the construct");
        }
    }
    assert!(!ev.tether_broken[0]);
    cols.clear();
    let ev = step_player(&mut s, &input(yaw, pitch, 0, 0, TETHER_L), &CollisionWorld::new(&w, &cols), &p, DT);
    assert!(ev.tether_broken[0] && ev.tether_released[0] && s.hooks[0].phase == HookPhase::Idle);
}

#[test]
fn anchor_query_and_aim_assist() {
    let p = p();
    // A 1 m cube 20 m ahead (−Z), centred at y = 10.
    let w = FnWorld(|v: IVec3| (-1..1).contains(&v.x) && (19..21).contains(&v.y) && (-41..-39).contains(&v.z));
    let cw = CollisionWorld::new(&w, &[]);
    let eye = Vec3::new(0.0, 10.0, 0.0);
    let q = query_anchor(&cw, eye, Vec3::new(0.0, 0.0, -1.0), &p).expect("direct hit");
    assert!(!q.assisted && (q.point.z + 19.5).abs() < 1e-3);
    // Aim 2° past the right edge (edge at 0.5 m / 19.5 m ≈ 1.47°; aim at 3.5°): center misses, assist finds it.
    let a = 3.5f32.to_radians();
    let look = Vec3::new(a.sin(), 0.0, -a.cos());
    let q = query_anchor(&cw, eye, look, &p).expect("assist should catch the edge");
    assert!(q.assisted);
    // Way off: nothing.
    let a = 10f32.to_radians();
    assert!(query_anchor(&cw, eye, Vec3::new(a.sin(), 0.0, -a.cos()), &p).is_none());
    // Out of range: nothing.
    let far = FnWorld(|v: IVec3| v.z < -400);
    assert!(query_anchor(&CollisionWorld::new(&far, &[]), eye, Vec3::NEG_Z, &p).is_none());
}

// -------------------------------------------------------------------------------------------- boost

#[test]
fn boost_meter_drains_and_regenerates() {
    let p = p();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    s.pos.y += 20.0; // in the air
    s.grounded = false;
    s.mods.gravity_scale = 0.0;
    let mut starts = 0;
    run(&mut s, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 1, BOOST), |_, _, e| starts += e.boost_start as i32);
    assert_eq!(starts, 1);
    let expect = p.gas_max - p.burst_cost - p.gas_drain * 1.0;
    assert!((s.gas - expect).abs() < 0.6, "gas {} expected {expect}", s.gas);
    assert!(s.vel.x > 30.0, "1 s of boost gave only {}", s.vel.x);
    // Delay, then air regen.
    let g0 = s.gas;
    run(&mut s, &cw, &p, 60, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |_, _, _| {});
    assert!((s.gas - g0).abs() < 1e-3, "regen during delay");
    run(&mut s, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |_, _, _| {});
    assert!((s.gas - (g0 + p.gas_regen_air)).abs() < 0.3, "air regen {} -> {}", g0, s.gas);
    // Drain to empty: thrust stops.
    s.gas = 5.0;
    s.vel = Vec3::ZERO;
    run(&mut s, &cw, &p, 120, |_, _| input(YAW_PX, 0.0, 0, 1, BOOST), |_, _, _| {});
    assert_eq!(s.gas, 0.0);
    assert!(s.vel.x < 6.0, "boost without gas: {}", s.vel.x);
}

// --------------------------------------------------------------------------------- modifiers & misc

#[test]
fn impulse_tumble_reduces_air_control_until_landing() {
    let p = p();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let lateral = |tumble: f32| {
        let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
        settle(&mut s, &cw, &p);
        s.apply_impulse(Vec3::new(0.0, 12.0, 0.0), tumble);
        step_player(&mut s, &input(0.0, 0.0, 1, 0, 0), &cw, &p, DT);
        assert!(!s.grounded);
        s.vel.x
    };
    assert!(lateral(2.0) < lateral(0.0) * 0.5);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    s.apply_impulse(Vec3::new(5.0, 10.0, 0.0), 5.0);
    run(&mut s, &cw, &p, 200, |_, _| input(0.0, 0.0, 0, 0, 0), |_, _, _| {});
    assert!(s.grounded && s.tumble == 0.0);
}

#[test]
fn moving_platform_carries_and_momentum_inherits() {
    let p = p();
    let w = FnWorld(|_| false);
    let mut plat = Collider::new(Vec3::new(0.0, -0.5, 0.0), Vec3::new(5.0, 0.5, 5.0), Quat::IDENTITY, ColliderId::Other(2)).with_vel(Vec3::new(4.0, 0.0, 0.0));
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    for t in 0..150 {
        plat.center += plat.vel * DT;
        let cols = [plat];
        let cw = CollisionWorld::new(&w, &cols);
        let jump = if t == 140 { JUMP } else { 0 };
        step_player(&mut s, &input(0.0, 0.0, 0, 0, jump), &cw, &p, DT);
        if t == 60 {
            // Landing on a moving platform: friction catches you up to its speed quickly.
            assert!(s.grounded && (s.vel.x - 4.0).abs() < 0.05, "not carried: {}", s.vel.x);
        }
    }
    assert!((s.vel.x - 4.0).abs() < 0.1, "jumping off a platform should keep its velocity: {}", s.vel.x);
}

#[test]
fn glide_slowfall_and_flight_modifiers() {
    let p = p();
    let w = FnWorld(|_| false);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 100.0, 0.0), &p);
    s.mods.slow_fall = 3.0;
    run(&mut s, &cw, &p, 240, |_, _| input(0.0, 0.0, 0, 0, 0), |_, _, _| {});
    assert!(s.vel.y > -3.3, "parachute fall {}", s.vel.y);
    let mut s = PlayerState::new(Vec3::new(0.0, 100.0, 0.0), &p);
    s.mods.glide = 1.0;
    run(&mut s, &cw, &p, 240, |_, _| input(0.0, 0.0, 0, 0, 0), |_, _, _| {});
    assert!(s.vel.y > -12.0 && s.vel.z < -8.0, "glide {:?}", s.vel);
    // Flight cruise reference.
    let mut s = PlayerState::new(Vec3::new(0.0, 100.0, 0.0), &p);
    s.mods.thrust = 30.0;
    s.mods.gravity_scale = 0.0;
    run(&mut s, &cw, &p, 480, |_, _| input(0.0, 0.0, 0, 1, 0), |_, _, _| {});
    assert!((s.speed() - 17.0).abs() < 0.1, "flight cruise {}", s.speed());
}

#[test]
fn typing_suppresses_jump_and_boost_but_keeps_momentum() {
    let p = p();
    let w = FnWorld(|_| false);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 100.0, 0.0), &p);
    s.vel = Vec3::new(20.0, 0.0, 0.0);
    run(&mut s, &cw, &p, 60, |_, _| input(0.0, 0.0, 0, 0, bd_core::input::buttons::TYPING | JUMP | BOOST), |_, _, _| {});
    assert!((s.vel.x - 20.0).abs() < 1e-4 && s.vel.z.abs() < 1e-6 && s.gas == p.gas_max);
}

#[test]
fn typing_with_latched_forward_keeps_ground_speed() {
    // The client latches the move keys held when the ring opens: running while typing never
    // brakes (voice == typing), and jump stays suppressed.
    let p = p();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    run(&mut s, &cw, &p, 240, |_, _| input(YAW_PX, 0.0, 0, 1, 0), |_, _, _| {});
    let typing = bd_core::input::buttons::TYPING | JUMP | BOOST;
    run(&mut s, &cw, &p, 240, |_, _| input(YAW_PX, 0.0, 0, 1, typing), |_, s, _| {
        assert!(s.grounded, "typing must not jump");
    });
    assert!((s.horizontal_speed() - p.run_speed).abs() < 0.05, "typing run speed {}", s.horizontal_speed());
    assert!(s.vel.x > 9.9 && s.gas == p.gas_max);
    // Aim still steers the latched wish: turn to +Z (yaw π) and the run follows.
    run(&mut s, &cw, &p, 120, |_, _| input(PI, 0.0, 0, 1, typing), |_, _, _| {});
    assert!(s.vel.z > 9.9 && s.vel.x.abs() < 0.1, "typing steer {:?}", s.vel);
    // Nothing latched (keys released before opening the ring) → coast to a stop as before.
    run(&mut s, &cw, &p, 120, |_, _| input(PI, 0.0, 0, 0, typing), |_, _, _| {});
    assert!(s.horizontal_speed() < 0.1, "typing coast {}", s.horizontal_speed());
}

#[test]
fn typing_air_strafe_still_gains_speed() {
    let p = p();
    let w = FnWorld(|_| false);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::ZERO, &p);
    s.mods.gravity_scale = 0.0;
    s.vel = Vec3::new(0.0, 0.0, -10.0);
    let typing = bd_core::input::buttons::TYPING;
    run(&mut s, &cw, &p, 4 * 120, |_, s| input(optimal_strafe_yaw(s.vel, &p, DT, 1, 0, 1.0), 0.0, 1, 0, typing), |_, _, _| {});
    let v = s.horizontal_speed();
    assert!(v > 17.0, "typing air strafe only reached {v}");
}

#[test]
fn soft_cap_only_bites_far_above_skill_speeds() {
    let p = p();
    let w = FnWorld(|_| false);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::ZERO, &p);
    s.mods.gravity_scale = 0.0;
    s.vel = Vec3::new(40.0, 0.0, 0.0);
    run(&mut s, &cw, &p, 120, |_, _| input(0.0, 0.0, 0, 0, 0), |_, _, _| {});
    assert_eq!(s.vel.x, 40.0);
    s.vel = Vec3::new(65.0, 0.0, 0.0);
    run(&mut s, &cw, &p, 120, |_, _| input(0.0, 0.0, 0, 0, 0), |_, _, _| {});
    assert!(s.vel.x < 55.0 && s.vel.x > 42.0, "soft cap {}", s.vel.x);
}

#[test]
fn determinism_bit_identical() {
    let p = p();
    let w = swing_world();
    let ramp = descending_ramp(20.0);
    let cols = [ramp];
    let cw = CollisionWorld::new(&w, &cols);
    let script = |t: usize, s: &PlayerState| {
        let mut b = 0;
        if t % 50 < 25 {
            b |= JUMP;
        }
        if (200..320).contains(&t) {
            b |= TETHER_L;
        }
        if (250..270).contains(&t) || (400..420).contains(&t) {
            b |= BOOST;
        }
        if (330..380).contains(&t) {
            b |= CROUCH;
        }
        let yaw = if s.horizontal_speed() > 1.0 { optimal_strafe_yaw(s.vel, &p, DT, 1, 0, 1.0) } else { YAW_PX };
        let mut i = input(yaw, 0.4, 1, (t % 3) as i8 - 1, b);
        i.reel_notches = (t % 37 == 0) as i8;
        i
    };
    let trace = || {
        let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
        let mut out = String::new();
        run(&mut s, &cw, &p, 600, script, |_, s, e| {
            use std::fmt::Write;
            let _ = write!(out, "{s:?}{e:?}");
        });
        out
    };
    let a = trace();
    let b = trace();
    assert!(a == b, "non-deterministic");
}

#[test]
fn speed_tiers_are_layered() {
    use super::testkit::scenarios as sc;
    let p = p();
    let run = sc::run(&p);
    let flight = sc::flight(&p);
    let (naive, _) = sc::naive_bhop(&p, 5.0);
    let (bhop_held, _) = sc::bhop(&p, 5.0, false);
    let (bhop_timed, _) = sc::bhop(&p, 5.0, true);
    let (slide_burst, _) = sc::slide_flat(&p);
    let slide_ramp = sc::slide_ramp(&p);
    let swing = sc::swing(&p, false);
    let boost = sc::swing(&p, true);
    let tiers = format!(
        "run {run:.1} flight {flight:.1} naive {naive:.1} bhop {bhop_held:.1}/{bhop_timed:.1} slide {slide_burst:.1}/{slide_ramp:.1} \
         swing {:.1}/{:.1}/{:.1} boost {:.1}/{:.1}/{:.1}",
        swing.peak, swing.release, swing.after, boost.peak, boost.release, boost.after
    );
    assert!((run - p.run_speed).abs() < 0.05, "{tiers}");
    assert!((flight - 17.0).abs() < 0.1, "{tiers}");
    // Competent movement beats flight cruise.
    assert!(bhop_held > flight + 1.0 && bhop_timed > bhop_held, "{tiers}");
    assert!(slide_burst > run + 4.0 && slide_ramp > 20.0, "{tiers}");
    assert!(swing.peak > 26.0 && swing.peak > bhop_timed + 5.0, "{tiers}");
    assert!(swing.release > 24.0 && swing.after > 0.85 * swing.release, "{tiers}");
    assert!(boost.peak > swing.peak + 5.0 && boost.release > swing.release + 5.0, "{tiers}");
    assert!(boost.after > 0.85 * boost.release && boost.distance > swing.distance, "{tiers}");
    assert!(boost.peak < p.hard_cap_speed, "{tiers}");
    // The ladder: a beginner holding W + jump and sweeping the mouse gains a little (feels it) but stays
    // far below a skilled strafer; then slide tech, swing, swing + boost.
    assert!(naive > run + 1.0 && naive < bhop_held - 5.0, "{tiers}");
    assert!(bhop_held < slide_ramp && slide_ramp < swing.peak && swing.peak < boost.peak, "{tiers}");
}

// ------------------------------------------------------------------------------------- generosity

/// Land at `speed` on flat ground (falling from 1.5 m, no input), then press jump `late` ticks after the
/// touchdown tick (`None` = pressed 0.15 s before touchdown, i.e. buffered). Returns (horizontal speed
/// right after the hop, the jump events).
fn hop_after_landing(p: &MoveParams, speed: f32, late: Option<usize>) -> (f32, MoveEvents) {
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 1.5, 0.0), p);
    s.vel = Vec3::new(speed, 0.0, 0.0);
    // Ticks until touchdown (no jump pressed), measured on a copy.
    let mut probe = s;
    let mut land = 0;
    while !probe.grounded {
        step_player(&mut probe, &input(YAW_PX, 0.0, 0, 0, 0), &cw, p, DT);
        land += 1;
    }
    let press = match late {
        None => land - (0.15 / DT) as usize,
        Some(k) => land + k,
    };
    for t in 0..400 {
        let e = step_player(&mut s, &input(YAW_PX, 0.0, 0, 0, if t == press { JUMP } else { 0 }), &cw, p, DT);
        if e.jumped {
            return (s.horizontal_speed(), e);
        }
    }
    panic!("never hopped");
}

#[test]
fn jump_buffer_and_landing_grace_forgive_timing() {
    let p = p();
    let (perfect, e) = hop_after_landing(&p, 15.0, None);
    assert!(e.perfect_hop, "a press 0.15 s before touchdown is buffered into a perfect hop");
    assert!(perfect > 15.0 - 1e-3, "perfect hop lost speed: {perfect}");
    // Late presses inside the landing grace: still hop, keep ≥ 90 % (≈ 4 % at the grace edge), not "perfect".
    let grace_ticks = (p.land_grace / DT).round() as usize;
    let mut prev = perfect;
    for late in 1..grace_ticks {
        let (v, e) = hop_after_landing(&p, 15.0, Some(late));
        assert!(!e.perfect_hop, "late by {late} ticks counted as perfect");
        assert!(v >= 0.9 * perfect && v < prev, "late by {late}: {v} vs perfect {perfect}");
        prev = v;
    }
    assert!(prev > 0.95 * perfect, "grace edge keeps {prev} of {perfect}");
    // Past the grace friction bites fully: 150 ms late costs a lot more.
    let (slow, _) = hop_after_landing(&p, 15.0, Some(18));
    assert!(slow < 0.8 * perfect, "150 ms late should cost real speed: {slow}");
    // Without the grace the same late hop would have cost ~20 %.
    let strict = MoveParams { land_grace: 0.0, ..p.clone() };
    let (v_strict, _) = hop_after_landing(&strict, 15.0, Some(grace_ticks - 1));
    assert!(v_strict < 0.85 * perfect, "{v_strict}");
    // A standing jump is not a perfect hop.
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    let e = step_player(&mut s, &input(YAW_PX, 0.0, 0, 0, JUMP), &cw, &p, DT);
    assert!(e.jumped && !e.perfect_hop);
}

#[test]
fn hop_chain_counts_clean_hops_and_resets() {
    let p = p();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    s.vel = Vec3::new(0.0, 0.0, -p.run_speed);
    let mut chains = Vec::new();
    let mut gain = 0.0;
    let h0 = s.horizontal_speed();
    // Held-jump strafe bhop for 4 s: the first hop comes off a run (friction) and doesn't count.
    run(&mut s, &cw, &p, 480, |_, s| input(optimal_strafe_yaw(s.vel, &p, DT, 1, 0, 1.0), 0.0, 1, 0, JUMP), |_, _, e| {
        gain += e.speed_gain;
        if e.jumped {
            chains.push(e.hop_chain);
        }
    });
    assert!(chains.len() >= 5, "{chains:?}");
    for (i, c) in chains.iter().enumerate() {
        assert_eq!(*c as usize, i, "chain {chains:?}");
    }
    assert_eq!(s.hop_chain(), *chains.last().unwrap());
    assert!((gain - (s.horizontal_speed() - h0)).abs() < 1e-3, "speed_gain sums to the change: {gain}");
    assert!(gain > 3.0);
    // Stop hopping: the chain survives the grace, then resets.
    let mut after = Vec::new();
    run(&mut s, &cw, &p, 200, |_, _| input(YAW_PX, 0.0, 0, 0, 0), |_, s, e| {
        if s.grounded {
            after.push((s.ground_time(), e.hop_chain));
        }
    });
    let (t_first, c_first) = after[0];
    assert!(t_first == 0.0 && c_first > 0, "{:?}", &after[..3]);
    assert!(after.iter().all(|&(t, c)| (t < p.land_grace) == (c > 0)), "{after:?}");
    // A late-but-forgiven hop counts; a slide breaks the chain.
    let (_, e) = hop_after_landing(&p, 15.0, Some(2));
    assert_eq!(e.hop_chain, 1, "the first hop after a fall within the grace counts");
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), &p);
    settle(&mut s, &cw, &p);
    s.hop_chain = 5;
    s.vel = Vec3::new(14.0, 0.0, 0.0);
    let e = step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, CROUCH), &cw, &p, DT);
    assert!(s.sliding && e.hop_chain == 0);
    // Garbage high input bits never leak into the chain.
    s.prev_buttons = JUMP;
    assert_eq!(s.hop_chain(), 0);
    step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, 0xF000), &cw, &p, DT);
    assert_eq!(s.hop_chain(), 0);
}

#[test]
fn wstrafe_assist_gives_beginners_a_fraction_of_strafing() {
    use super::testkit::scenarios as sc;
    let p = p();
    let off = MoveParams { wstrafe_assist: 0.0, ..p.clone() };
    for deg in [90.0f32, 150.0] {
        let r = deg.to_radians();
        let (sync, _) = sc::turn_gain(&p, r, 3.0, 12.0, true);
        let (w, lag) = sc::turn_gain(&p, r, 3.0, 12.0, false);
        let (w_off, lag_off) = sc::turn_gain(&off, r, 3.0, 12.0, false);
        let ratio = w / sync;
        let msg = format!("{deg}°/s: sync {sync} W {w} (lag {lag}) W-off {w_off} (lag {lag_off})");
        assert!((0.38..=0.52).contains(&ratio), "{msg} ratio {ratio}");
        // The velocity follows the view (the beginner looks where they go); without the assist the
        // view runs ~90° ahead before W does anything.
        assert!(lag < 15f32.to_radians() && lag_off > 60f32.to_radians(), "{msg}");
    }
    // Looking straight ahead (no turn) never gains: W alone isn't free speed.
    let w = FnWorld(|_| false);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::ZERO, &p);
    s.mods.gravity_scale = 0.0;
    s.vel = Vec3::new(0.0, 0.0, -14.0);
    run(&mut s, &cw, &p, 240, |_, _| input(0.0, 0.0, 0, 1, 0), |_, _, _| {});
    assert_eq!(s.vel, Vec3::new(0.0, 0.0, -14.0));
    // Mouse-wiggling in place (±5° at 20 Hz) gains far less than real strafing.
    let mut s = PlayerState::new(Vec3::ZERO, &p);
    s.mods.gravity_scale = 0.0;
    s.vel = Vec3::new(0.0, 0.0, -14.0);
    run(&mut s, &cw, &p, 360, |t, _| input(if (t / 3) % 2 == 0 { 0.09 } else { -0.09 }, 0.0, 0, 1, 0), |_, _, _| {});
    let (sync, _) = sc::turn_gain(&p, 150f32.to_radians(), 3.0, 14.0, true);
    assert!(s.horizontal_speed() - 14.0 < 0.5 * sync, "wiggle {} vs sync {sync}", s.horizontal_speed() - 14.0);
}

#[test]
fn strafe_keys_are_untouched_by_wstrafe_assist() {
    // Bit-identical: any input with a strafe key (or without forward) never takes the assist path.
    let p = p();
    let off = MoveParams { wstrafe_assist: 0.0, ..p.clone() };
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    for (mx, mz) in [(1i8, 0i8), (-1, 0), (1, 1), (-1, 1), (0, -1), (0, 0)] {
        let trace = |p: &MoveParams| {
            let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), p);
            settle(&mut s, &cw, p);
            s.vel = Vec3::new(0.0, 0.0, -p.run_speed);
            let mut out = Vec::new();
            run(&mut s, &cw, p, 600, |t, s| {
                let yaw = if s.horizontal_speed() > 1.0 { optimal_strafe_yaw(s.vel, p, DT, mx, mz, 1.0) } else { 0.3 * t as f32 * DT };
                input(yaw, 0.0, mx, mz, if t % 90 < 80 { JUMP } else { 0 })
            }, |_, s, e| out.push((*s, *e)));
            out
        };
        assert!(trace(&p) == trace(&off), "strafe input ({mx}, {mz}) changed by the W assist");
    }
}

#[test]
fn params_round_trip_ron() {
    let p = p();
    let s = ron::ser::to_string_pretty(&p, Default::default()).unwrap();
    let back: MoveParams = ron::from_str(&s).unwrap();
    assert_eq!(p, back);
    // Partial files override only what they name.
    let partial: MoveParams = ron::from_str("(gravity: 30.0)").unwrap();
    assert_eq!(partial.gravity, 30.0);
    assert_eq!(partial.run_speed, p.run_speed);
    // The shipped hot-reload file matches the built-in defaults.
    let shipped: MoveParams = ron::from_str(include_str!("../../../../assets/tuning/move.ron")).unwrap();
    assert_eq!(shipped, p, "assets/tuning/move.ron drifted from MoveParams::default()");
}
