//! Air jump + dodge (movement/air_jump.rs, movement/dodge.rs, crate::dodge): the rules, bhop
//! safety, no-speed-gain properties, restores, intangibility (hitscan / melee, lag compensation),
//! hook-and-strike escape, and host/prediction parity.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::{DT, IVec3, PlayerId, Vec3};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::movement::testkit::{FnWorld, JUMP, TETHER_L, aim_at, input, optimal_strafe_yaw, scenarios};
use scenarios::zigzag_side;
use bd_sim::movement::{CollisionWorld, MoveEvents, MoveParams, PlayerState, air_jump_redirect, step_player};
use bd_sim::rules::WillpowerMode;
use bd_sim::weapons::HitscanShot;
use bd_sim::world::{ArenaSource, SimConfig, World};

const DODGE: u16 = buttons::DODGE;
const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn floor() -> FnWorld<impl Fn(IVec3) -> bool> {
    FnWorld(|v: IVec3| v.y < 0)
}

/// Airborne high above an infinite floor, falling slowly, long past the take-off.
fn airborne(p: &MoveParams, vel: Vec3) -> PlayerState {
    let mut s = PlayerState::new(Vec3::new(0.0, 40.0, 0.0), p);
    s.vel = vel;
    s.air_time = 0.5;
    s
}

fn grounded(p: &MoveParams, w: &CollisionWorld<impl bd_core::voxel::VoxelQuery + ?Sized>) -> PlayerState {
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 0.0), p);
    for _ in 0..10 {
        step_player(&mut s, &input(0.0, 0.0, 0, 0, 0), w, p, DT);
    }
    assert!(s.grounded);
    s
}

// ============================================================================== air jump rules

#[test]
fn air_jump_fires_once_and_comes_back_on_landing() {
    let p = MoveParams::default();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = grounded(&p, &cw);
    for round in 0..2 {
        // Ground jump (press), release.
        let e = step_player(&mut s, &input(0.0, 0.0, 0, 0, JUMP), &cw, &p, DT);
        assert!(e.jumped && !e.air_jumped, "round {round}: the ground jump");
        for _ in 0..30 {
            assert!(!step_player(&mut s, &input(0.0, 0.0, 0, 0, 0), &cw, &p, DT).air_jumped);
        }
        assert!(!s.grounded);
        // Fresh press in the air: the air jump.
        let vy0 = s.vel.y;
        let e = step_player(&mut s, &input(0.0, 0.0, 0, 0, JUMP), &cw, &p, DT);
        assert!(e.air_jumped && !e.jumped, "round {round}: air jump");
        assert!(s.vel.y > vy0 && (s.vel.y - (p.air_jump_speed - p.gravity * DT)).abs() < 1e-3, "vy {}", s.vel.y);
        assert!(p.air_jump_speed < p.jump_speed, "a bit weaker than the ground jump");
        assert!(s.extra.air_jump_used);
        // Only one.
        let mut extra = 0;
        for k in 0..400 {
            let b = if k % 20 == 0 { JUMP } else { 0 };
            let e = step_player(&mut s, &input(0.0, 0.0, 0, 0, b), &cw, &p, DT);
            extra += e.air_jumped as u32;
            if s.grounded {
                break;
            }
        }
        assert_eq!(extra, 0, "one air jump per airtime");
        assert!(s.grounded && !s.extra.air_jump_used, "restored on the landing tick");
        // Settle (a buffered press may hop once more).
        for _ in 0..200 {
            step_player(&mut s, &input(0.0, 0.0, 0, 0, 0), &cw, &p, DT);
        }
        assert!(s.grounded && !s.extra.air_jump_used, "restored on landing");
    }
}

#[test]
fn air_jump_comes_back_on_tether_attach_and_not_while_attached() {
    let p = MoveParams::default();
    let w = scenarios::arena();
    let cw = CollisionWorld::new(&w, &[]);
    // Under the lattice anchor cube at x, z ∈ [0, 2], y ∈ [24, 26].
    let mut s = PlayerState::new(Vec3::new(1.0, 14.0, 7.0), &p);
    s.air_time = 0.5;
    let e = step_player(&mut s, &input(0.0, 0.0, 0, 0, JUMP), &cw, &p, DT);
    assert!(e.air_jumped && s.extra.air_jump_used);
    let anchor = Vec3::new(1.0, 24.0, 1.0);
    let mut attached = false;
    for _ in 0..30 {
        let (yaw, pitch) = aim_at(s.eye(&p), anchor);
        let e = step_player(&mut s, &input(yaw, pitch, 0, 0, TETHER_L), &cw, &p, DT);
        if e.tether_attached[0] {
            attached = true;
            break;
        }
    }
    assert!(attached, "hooked the anchor");
    assert!(!s.extra.air_jump_used, "restored on tether attach");
    // While attached a press does nothing (the rope owns you)…
    let (yaw, pitch) = aim_at(s.eye(&p), anchor);
    let e = step_player(&mut s, &input(yaw, pitch, 0, 0, TETHER_L | JUMP), &cw, &p, DT);
    assert!(!e.air_jumped && s.tethered());
    // …release (momentum kept), then jump.
    step_player(&mut s, &input(yaw, pitch, 0, 0, 0), &cw, &p, DT);
    assert!(!s.tethered());
    let e = step_player(&mut s, &input(yaw, pitch, 0, 0, JUMP), &cw, &p, DT);
    assert!(e.air_jumped, "release + air jump");
}

#[test]
fn no_air_jump_while_tumbling_charging_or_just_after_takeoff() {
    let p = MoveParams::default();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = airborne(&p, Vec3::ZERO);
    s.tumble = 0.5;
    assert!(!step_player(&mut s, &input(0.0, 0.0, 0, 0, JUMP), &cw, &p, DT).air_jumped, "hitstun: the press is the tech input");
    let mut s = airborne(&p, Vec3::ZERO);
    assert!(!step_player(&mut s, &input(0.0, 0.0, 0, 0, JUMP | buttons::CHARGE), &cw, &p, DT).air_jumped, "charging");
    // A second press 0.05 s after the ground jump (wheel / double tap tail) is not an air jump.
    let mut s = grounded(&p, &cw);
    step_player(&mut s, &input(0.0, 0.0, 0, 0, JUMP), &cw, &p, DT);
    for k in 0..6 {
        let e = step_player(&mut s, &input(0.0, 0.0, 0, 0, if k % 2 == 1 { JUMP } else { 0 }), &cw, &p, DT);
        assert!(!e.air_jumped, "tick {k}");
    }
}

// ============================================================================== bhop is sacred

/// Bunny-hop for `secs` with a jump pattern; returns (air jumps fired, hops, final speed).
fn bhop_with(pattern: impl Fn(&PlayerState, usize) -> bool, secs: f32) -> (u32, u32, f32) {
    let p = MoveParams::default();
    let w = scenarios::arena();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 0.01, 1.0), &p);
    for _ in 0..5 {
        step_player(&mut s, &input(0.0, 0.0, 0, 0, 0), &cw, &p, DT);
    }
    s.vel = Vec3::new(p.run_speed, 0.0, 0.0);
    let (mut air, mut hops) = (0, 0);
    let mut side = 1.0f32;
    for t in 0..(secs / DT) as usize {
        side = zigzag_side(s.vel, Vec3::X, side);
        let mx: i8 = if side > 0.0 { 1 } else { -1 };
        let yaw = optimal_strafe_yaw(s.vel, &p, DT, mx, 0, side);
        let b = if pattern(&s, t) { JUMP } else { 0 };
        let e: MoveEvents = step_player(&mut s, &input(yaw, 0.0, mx, 0, b), &cw, &p, DT);
        air += e.air_jumped as u32;
        hops += e.jumped as u32;
    }
    (air, hops, s.horizontal_speed())
}

#[test]
fn holding_jump_through_a_bhop_chain_never_air_jumps() {
    let (air, hops, speed) = bhop_with(|_, _| true, 10.0);
    assert_eq!(air, 0, "held jump fired an air jump");
    assert!(hops > 8 && speed > 14.0, "hops {hops}, speed {speed}");
}

#[test]
fn buffered_and_wheel_spam_presses_near_the_ground_never_air_jump() {
    // Perfect timing: tap every other tick while falling close to the floor (the testkit bot).
    let (air, hops, _) = bhop_with(|s, t| (s.grounded || (s.vel.y < 0.0 && s.pos.y < 0.5)) && t % 2 == 0, 10.0);
    assert_eq!(air, 0);
    assert!(hops > 8);
    // Wheel spam: a notch every 2 ticks from 0.2 s before touchdown (y < 1.3 m falling) until 0.08 s
    // after take-off.
    let (air, hops, _) = bhop_with(|s, t| t % 2 == 0 && (s.grounded || (s.vel.y < 0.0 && s.pos.y < 1.3) || (s.vel.y > 0.0 && s.air_time < 0.08)), 10.0);
    assert_eq!(air, 0, "wheel spam fired an air jump");
    assert!(hops > 8);
}

// ============================================================================== no speed gain

#[test]
fn air_jump_never_adds_horizontal_speed_at_any_speed_or_angle() {
    // Isolate the air jump: no air acceleration / W-assist, so the tick itself adds nothing.
    let mut p = MoveParams::default();
    p.air_accel = 0.0;
    p.wstrafe_assist = 0.0;
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut n = 0;
    for si in 0..=24 {
        let speed = si as f32 * 2.5;
        for hi in 0..24 {
            let heading = hi as f32 * 15f32.to_radians();
            let vel = Vec3::new(heading.sin() * speed, -3.0, heading.cos() * speed);
            for (mx, mz) in [(0, 1), (1, 0), (1, 1), (-1, 1), (0, -1), (-1, -1), (0, 0)] {
                for yaw in [0.0f32, 1.1, 2.9, -2.2] {
                    let mut s = airborne(&p, vel);
                    let e = step_player(&mut s, &input(yaw, 0.0, mx, mz, JUMP), &cw, &p, DT);
                    assert!(e.air_jumped);
                    let after = s.horizontal_speed();
                    assert!(after <= speed * (1.0 + 1e-5) + 1e-5, "speed {speed} heading {hi} keys ({mx},{mz}) yaw {yaw}: {after}");
                    if speed < p.soft_cap_speed {
                        assert!(after >= speed * (1.0 - 1e-4) - 1e-4, "speed kept: {speed} → {after}");
                    }
                    n += 1;
                }
            }
        }
    }
    assert!(n > 4000);
}

#[test]
fn air_jump_tick_never_beats_the_best_plain_strafe_tick() {
    // Default tuning, across bhop speeds: the air-jump tick never gains more horizontal speed than
    // the best plain air-strafe tick with the same keys (any view angle): the jump adds nothing,
    // it only turns the velocity, and air acceleration depends only on the angle.
    let p = MoveParams::default();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    for si in 0..=20 {
        let speed = si as f32 * 3.0;
        for hi in 0..36 {
            let heading = hi as f32 * 10f32.to_radians();
            let vel = Vec3::new(heading.sin() * speed, -3.0, heading.cos() * speed);
            for (mx, mz) in [(1, 0), (-1, 0), (0, 1), (1, 1)] {
                let yaw = optimal_strafe_yaw(vel, &p, DT, mx, mz, 1.0);
                for dy in [0.0f32, 0.4, -0.9, 2.5] {
                    let mut a = airborne(&p, vel);
                    let ea = step_player(&mut a, &input(yaw + dy, 0.0, mx, mz, JUMP), &cw, &p, DT);
                    assert!(ea.air_jumped);
                    let best = (0..720)
                        .map(|k| {
                            let mut b = airborne(&p, vel);
                            step_player(&mut b, &input(k as f32 * 0.5f32.to_radians(), 0.0, mx, mz, 0), &cw, &p, DT);
                            b.horizontal_speed()
                        })
                        .fold(0.0f32, f32::max);
                    assert!(a.horizontal_speed() <= best + 2e-3, "speed {speed} heading {hi}: {} > best strafe {best}", a.horizontal_speed());
                }
            }
        }
    }
}

#[test]
fn redirect_turns_by_the_fraction_and_keeps_the_speed() {
    let signed = |a: Vec3, b: Vec3| a.cross(b).y.atan2(a.dot(b));
    for frac in [0.0f32, 0.25, 0.3, 0.5, 1.0] {
        for sp in [0.5f32, 9.0, 31.0] {
            for k in -17..=17 {
                let ang = (k as f32 * 10.0).to_radians();
                let h = Vec3::new(0.3, 0.0, -1.0).normalize() * sp;
                let wish = Vec3::new(h.x * ang.cos() + h.z * ang.sin(), 0.0, h.z * ang.cos() - h.x * ang.sin()).normalize();
                let before = signed(h, wish);
                assert!((before - ang).abs() < 1e-4, "setup");
                let r = air_jump_redirect(h, wish, frac);
                assert!(r.length() <= sp * (1.0 + 1e-6), "never longer");
                assert!((r.length() - sp).abs() < 1e-3 * sp.max(1.0));
                let after = signed(r, wish);
                assert!((after - before * (1.0 - frac)).abs() < 1e-3, "frac {frac} angle {k}0°: {after} vs {}", before * (1.0 - frac));
            }
        }
    }
    // Default: about a third of the way.
    let p = MoveParams::default();
    assert!((0.25..=0.35).contains(&p.air_jump_redirect));
    // No wish / no speed: untouched.
    assert_eq!(air_jump_redirect(Vec3::new(3.0, 0.0, 4.0), Vec3::ZERO, 0.3), Vec3::new(3.0, 0.0, 4.0));
    assert_eq!(air_jump_redirect(Vec3::ZERO, Vec3::X, 0.3), Vec3::ZERO);
}

// ============================================================================== dodge

/// Dodge from `vel` with keys, then let go; returns (max horizontal speed after the burst,
/// horizontal displacement during the burst, ticks of burst, the dodge state at the start).
fn dodge_from(p: &MoveParams, vel: Vec3, ground: bool, yaw: f32, mx: i8, mz: i8) -> (f32, Vec3, u32, bd_sim::movement::ExtraMoves) {
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = if ground { grounded(p, &cw) } else { airborne(p, vel) };
    s.vel = Vec3::new(vel.x, if ground { 0.0 } else { vel.y }, vel.z);
    let p0 = s.pos;
    let e = step_player(&mut s, &input(yaw, 0.0, mx, mz, DODGE), &cw, p, DT);
    assert!(e.dodged);
    let x0 = s.extra;
    let mut ticks = 1;
    let mut disp = s.pos - p0;
    let mut max_after = 0.0f32;
    for _ in 0..60 {
        let was = s.extra.dodging();
        step_player(&mut s, &input(yaw, 0.0, 0, 0, 0), &cw, p, DT);
        if was && s.extra.dodging() {
            ticks += 1;
            disp = s.pos - p0;
        } else {
            max_after = max_after.max(s.horizontal_speed());
        }
    }
    (max_after, Vec3::new(disp.x, 0.0, disp.z), ticks, x0)
}

#[test]
fn dodge_never_raises_sustained_speed_at_any_speed_or_angle() {
    let p = MoveParams::default();
    for ground in [false, true] {
        for si in 0..=16 {
            let speed = si as f32 * 3.0;
            for hi in 0..12 {
                let heading = hi as f32 * 30f32.to_radians();
                let vel = Vec3::new(heading.sin() * speed, 0.0, heading.cos() * speed);
                for (mx, mz) in [(0, 1), (1, 0), (-1, 1), (0, -1), (0, 0)] {
                    // The ridden velocity is this tick's ordinary ground / air movement (Source accel quirks included).
                    let (after, _, _, x) = dodge_from(&p, vel, ground, 0.7, mx, mz);
                    let cap = x.dodge_base.length().max(p.run_speed);
                    assert!(after <= cap + 1e-3, "ground {ground} speed {speed} heading {hi} keys ({mx},{mz}): {after} > {cap}");
                }
            }
        }
    }
}

#[test]
fn dodge_covers_its_distance_in_its_time() {
    let p = MoveParams::default();
    let (_, disp, ticks, x) = dodge_from(&p, Vec3::ZERO, false, 0.0, 1, 0);
    assert_eq!(ticks as f32, (p.dodge_time / DT).round(), "burst ticks");
    // The burst on top of the ridden velocity (here one tick of air acceleration along D).
    let want = x.dodge_dir * p.dodge_distance + x.dodge_base * (ticks as f32 * DT);
    assert!((disp - want).length() < 0.02, "distance {disp} vs {want}");
    // Along D at yaw 0 = +X; no keys = backward (+Z at yaw 0).
    assert!(disp.x > 3.9);
    let (_, back, _, _) = dodge_from(&p, Vec3::ZERO, true, 0.0, 0, 0);
    assert!(back.z > 3.5 && back.x.abs() < 0.1, "backward with no keys: {back}");
}

#[test]
fn dodge_cooldown_and_one_air_dodge_per_airtime() {
    let p = MoveParams::default();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = grounded(&p, &cw);
    let press = |s: &mut PlayerState| step_player(s, &input(0.0, 0.0, 1, 0, DODGE), &cw, &p, DT).dodged;
    let idle = |s: &mut PlayerState, n: usize| {
        for _ in 0..n {
            step_player(s, &input(0.0, 0.0, 0, 0, 0), &cw, &p, DT);
        }
    };
    assert!(press(&mut s), "ground dodge");
    idle(&mut s, 60);
    assert!(!press(&mut s), "cooling down (0.5 s)");
    idle(&mut s, 80);
    assert!(press(&mut s), "ready again after the cooldown");
    // Air: once per airtime, even after the cooldown.
    let mut s = airborne(&p, Vec3::ZERO);
    s.pos.y = 80.0;
    assert!(press(&mut s), "air dodge");
    idle(&mut s, 150);
    assert!(!s.grounded);
    assert!(!press(&mut s), "one air dodge per airtime");
    // Being hit gives it back (Smash recovery).
    bd_sim::dodge::on_hit(&mut s);
    idle(&mut s, 1);
    assert!(press(&mut s), "restored by a hit");
    // Landing gives it back.
    idle(&mut s, 1200);
    assert!(s.grounded && !s.extra.air_dodge_used);
    // No dodge while charging or tumbling.
    idle(&mut s, 150);
    assert!(!step_player(&mut s, &input(0.0, 0.0, 1, 0, DODGE | buttons::CHARGE), &cw, &p, DT).dodged);
    idle(&mut s, 1);
    s.tumble = 0.4;
    assert!(!press(&mut s));
}

#[test]
fn a_push_ends_the_dodge_without_keeping_the_burst() {
    let p = MoveParams::default();
    let w = floor();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = airborne(&p, Vec3::new(5.0, 0.0, 0.0));
    step_player(&mut s, &input(0.0, 0.0, 0, -1, DODGE), &cw, &p, DT);
    assert!(s.extra.dodging() && s.horizontal_speed() > 20.0);
    let base = s.extra.dodge_base;
    assert!((base.length() - 5.0).abs() < 0.1);
    s.apply_impulse(Vec3::new(0.0, 3.0, 0.0), 0.0);
    assert!(!s.extra.dodging());
    assert!((Vec3::new(s.vel.x, 0.0, s.vel.z) - base).length() < 1e-5, "back to the ridden velocity: {}", s.vel);
}

// ============================================================================== world

fn world(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
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

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
}

fn settle(w: &mut World, n: u32) {
    for _ in 0..n {
        w.tick(&[]);
    }
}

#[test]
fn being_hit_restores_the_air_jump_and_air_dodge() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, 20.0, 0.0));
    let p = w.player_mut(A).unwrap();
    p.mv.extra.air_jump_used = true;
    p.mv.extra.air_dodge_used = true;
    let pos = p.mv.pos;
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 5.0, 6.0, Vec3::X, pos));
    let x = w.player(A).unwrap().mv.extra;
    assert!(!x.air_jump_used && !x.air_dodge_used);
    // A launch too.
    w.player_mut(A).unwrap().mv.extra.air_jump_used = true;
    w.launch_player(A, Vec3::new(0.0, 10.0, 30.0), Some(B));
    assert!(!w.player(A).unwrap().mv.extra.air_jump_used);
}

/// B dodges this tick; returns B's damage % before.
fn b_dodges(w: &mut World) -> f32 {
    let before = w.player(B).unwrap().damage_pct;
    let evs = w.tick(&[(B, PlayerInput { move_x: 1, buttons: DODGE, ..Default::default() })]).to_vec();
    assert!(w.player(B).unwrap().move_events.dodged);
    assert!(evs.iter().any(|e| matches!(e, bd_sim::GameEvent::Move { player, flags, .. } if *player == B && flags & bd_sim::events::move_flags::DODGE != 0)));
    before
}

fn shoot_b(w: &mut World) -> f32 {
    let eye = w.player(A).unwrap().eye(w.params());
    // Aim where the shooter's screen shows B (rewound when A has a view tick).
    let view = w.player(A).unwrap().view_tick;
    let chest = view.and_then(|v| w.history.hull_at(B, v)).unwrap_or_else(|| w.hull_of(B).unwrap()).center();
    let before = w.player(B).unwrap().damage_pct;
    w.hitscan(HitscanShot::new(A, eye, chest - eye, 10.0, 2.0));
    w.player(B).unwrap().damage_pct - before
}

#[test]
fn dodge_is_intangible_to_hitscan_and_melee_then_not() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(8.0, 0.2, 8.0));
    place(&mut w, B, Vec3::new(8.0, 0.2, -4.0));
    settle(&mut w, 60);
    assert!(shoot_b(&mut w) > 0.0, "control: the shot lands");
    settle(&mut w, 150);
    b_dodges(&mut w);
    assert_eq!(shoot_b(&mut w), 0.0, "hitscan passes through the dodge");
    let pos = w.player(B).unwrap().mv.pos;
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, 8.0, 6.0, Vec3::Z, pos));
    assert_eq!(r.damage, 0.0, "melee passes through the dodge");
    assert!(bd_sim::hook::hook_breaks_on_dodge(w.player(B).unwrap()), "hook-and-strike escape flag");
    // Explosions are not dodged by intangibility (get out of them).
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::Explosion, 5.0, 3.0, Vec3::Z, pos));
    assert!(r.damage > 0.0);
    // After the window.
    let n = (w.cfg.move_params.dodge_iframes / DT).ceil() as u32 + 1;
    settle(&mut w, n);
    assert!(shoot_b(&mut w) > 0.0, "tangible again");
    let pos = w.player(B).unwrap().mv.pos;
    assert!(w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, 8.0, 6.0, Vec3::Z, pos)).damage > 0.0);
}

#[test]
fn intangibility_is_judged_at_the_shooters_view_tick() {
    let setup = || {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(8.0, 0.2, 8.0));
        place(&mut w, B, Vec3::new(8.0, 0.2, -4.0));
        settle(&mut w, 60);
        w
    };
    // The shooter's screen (6 ticks behind) hadn't shown the dodge yet: the shot counts.
    let mut w = setup();
    b_dodges(&mut w);
    settle(&mut w, 2);
    let t = w.tick;
    w.set_view_tick(A, Some((t - 6) as f64));
    assert!(shoot_b(&mut w) > 0.0, "attacker saw B before the dodge");
    // Their screen already shows the dodge: it passes through.
    let mut w = setup();
    b_dodges(&mut w);
    settle(&mut w, 6);
    let t = w.tick;
    w.set_view_tick(A, Some((t - 3) as f64));
    assert_eq!(shoot_b(&mut w), 0.0, "attacker saw the dodge");
}

// ============================================================================== prediction

struct Pair {
    host: World,
    client: World,
}

impl Pair {
    fn new(at: Vec3) -> Self {
        let mut host = world(2);
        host.player_mut(B).unwrap().mv.pos = Vec3::new(-40.0, 8.0, 40.0);
        let p = host.player_mut(A).unwrap();
        p.mv.pos = at;
        p.mv.vel = Vec3::ZERO;
        p.mv.grounded = false;
        host.tick(&[]);
        let mut client = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() });
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        Pair { host, client }
    }

    fn run(&mut self, n: u32, mut inp: impl FnMut(u32) -> PlayerInput) -> (u32, u32, u32) {
        let (mut bad, mut air, mut dodges) = (0, 0, 0);
        for k in 0..n {
            let i = inp(k);
            let mut s = self.client.player(A).unwrap().mv;
            let ev = self.client.predict_player(A, &mut s, &i);
            air += ev.air_jumped as u32;
            dodges += ev.dodged as u32;
            let evs = self.host.tick(&[(A, i)]).to_vec();
            for e in &evs {
                self.client.apply_event(e);
            }
            self.client.apply_snapshot(&self.host.snapshot_for(Some(A)), Some(A));
            let h = self.host.player(A).unwrap();
            assert_eq!(h.move_events.air_jumped, ev.air_jumped, "tick {k}: air jump event");
            assert_eq!(h.move_events.dodged, ev.dodged, "tick {k}: dodge event");
            let (dp, dv) = ((h.mv.pos - s.pos).length(), (h.mv.vel - s.vel).length());
            if dp > 1e-4 || dv > 1e-3 || h.mv.extra != s.extra {
                bad += 1;
                if bad < 4 {
                    eprintln!("tick {k}: prediction off by {dp} m, {dv} m/s");
                }
            }
        }
        (bad, air, dodges)
    }
}

#[test]
fn prediction_matches_the_host_for_air_jumps_and_dodges() {
    let mut p = Pair::new(Vec3::new(0.0, 0.2, 0.0));
    // Hop, air jump at the apex steering right, dodge in the air, land, ground dodges on cooldown,
    // hop again with held jump (never an air jump), wheel-ish taps near the ground.
    let (bad, air, dodges) = p.run(900, |k| {
        let k = k as i32;
        let b = match k {
            10 => JUMP,
            50 => JUMP,
            70 => DODGE,
            200 | 230 | 380 => DODGE,
            400..=600 => JUMP,
            650 => JUMP,
            690 | 692 | 694 => JUMP,
            _ => 0,
        };
        PlayerInput { yaw: 0.3 + k as f32 * 0.002, move_x: if (40..80).contains(&k) { 1 } else { 0 }, move_z: 1, buttons: b, ..Default::default() }
    });
    assert!(air >= 1, "air jumped");
    assert!(dodges >= 3, "dodged {dodges}");
    assert_eq!(bad, 0, "mispredicted ticks");
}

// ============================================================================== bots

#[test]
fn a_bot_knocked_off_the_edge_air_jumps_back() {
    use bd_sim::bots::BotBrain;
    // Flat lab: ground top at 8 m, x ∈ [-48, 48].
    let (g, edge) = (8.0, 48.0);
    let mut w = world(2);
    place(&mut w, A, Vec3::new(-40.0, g, -40.0));
    place(&mut w, B, Vec3::new(edge - 1.0, g, 0.0));
    let mut bot = BotBrain::new(B, 1.0, 7);
    bot.casts_enabled = false;
    for _ in 0..60 {
        let act = bot.think(&w);
        w.tick(&[(B, act.input)]);
        place(&mut w, B, Vec3::new(edge - 1.0, g, 0.0));
        w.player_mut(B).unwrap().mv.grounded = true;
    }
    w.launch_player(B, Vec3::new(22.0, -4.0, 0.0), Some(A));
    let mut air_jumps = 0;
    for _ in 0..(6 * 120) {
        let act = bot.think(&w);
        let evs = w.tick(&[(B, act.input)]).to_vec();
        assert!(!evs.iter().any(|e| matches!(e, bd_sim::GameEvent::Ko { victim, .. } if *victim == B)), "the bot fell");
        air_jumps += w.player(B).unwrap().move_events.air_jumped as u32;
    }
    assert!(air_jumps >= 1, "recovered with the air jump");
}

#[test]
fn bots_sometimes_dodge_a_charging_railgun() {
    use bd_core::mechanics::{Element, Form, Mechanic};
    use bd_sim::bots::{BotBrain, recipe};
    use bd_sim::world::Aim;
    let mut dodges = 0;
    for seed in 1..=6u64 {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(0.0, 0.2, 20.0));
        place(&mut w, B, Vec3::new(0.0, 0.2, -10.0));
        settle(&mut w, 30);
        let eye = w.player(A).unwrap().eye(w.params());
        w.cast(A, recipe("railgun", Form::Gun, &[Mechanic::Shoots, Mechanic::Pierce], Element::Electric, 1.0), None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
        settle(&mut w, 30);
        let mut bot = BotBrain::new(B, 1.0, seed);
        bot.casts_enabled = false;
        for _ in 0..(3 * 120) {
            let eye = w.player(A).unwrap().eye(w.params());
            let chest = w.hull_of(B).unwrap().center();
            let (yaw, pitch) = aim_at(eye, chest);
            let act = bot.think(&w);
            w.tick(&[(A, PlayerInput { yaw, pitch, buttons: buttons::PRIMARY, ..Default::default() }), (B, act.input)]);
            dodges += w.player(B).unwrap().move_events.dodged as u32;
        }
    }
    assert!(dodges >= 2, "bots dodged a charging railgun {dodges} times");
}