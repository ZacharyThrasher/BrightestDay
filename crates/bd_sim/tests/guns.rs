//! Guns (COMBAT.md §4b): recoil as movement, velocity inheritance, closing-speed momentum bonus,
//! pellets + falloff, rockets (self-knockback, no self-damage), lag-compensated piercing hitscan,
//! fire/hit juice events.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::HitKind;
use bd_sim::events::GameEvent;
use bd_sim::rules::WillpowerMode;
use bd_sim::weapons::{GunArchetype, HitscanShot};
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

fn settle(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

fn give(w: &mut World, id: PlayerId, name: &str, m: &[Mechanic], count: u8) {
    let eye = w.player(id).unwrap().eye(w.params());
    let mut r = recipe(name, Form::Gun, m, Element::Light, 0.6);
    r.count = count;
    w.cast(id, r, None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    settle(w, 30);
}

/// Hold PRIMARY for `ticks` with the given aim; collect events.
fn fire(w: &mut World, id: PlayerId, yaw: f32, pitch: f32, ticks: u32) -> Vec<GameEvent> {
    let inp = PlayerInput { yaw, pitch, buttons: buttons::PRIMARY, ..Default::default() };
    let mut ev = Vec::new();
    for _ in 0..ticks {
        ev.extend_from_slice(w.tick(&[(id, inp)]));
    }
    ev.extend_from_slice(w.tick(&[(id, PlayerInput { yaw, pitch, ..Default::default() })]));
    ev
}

#[test]
fn ring_blast_is_meaty_and_emits_fire_juice() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(10.0, 8.0, 8.0));
    place(&mut w, B, Vec3::new(10.0, 8.0, -8.0));
    settle(&mut w, 30);
    let ev = fire(&mut w, A, 0.0, -0.03, 40);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ShotFired { archetype: GunArchetype::Ring, owner, .. } if *owner == A)));
    let dmg = ev.iter().find_map(|e| match e {
        GameEvent::Hit { target, kind: HitKind::RingBlast, damage, .. } if *target == B => Some(*damage),
        _ => None,
    });
    assert_eq!(dmg, Some(12.0));
}

#[test]
fn recoil_pushes_opposite_the_aim_more_in_the_air() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(10.0, 8.0, 8.0));
    settle(&mut w, 30);
    give(&mut w, A, "shotgun", &[Mechanic::Shoots], 3);
    // Ground: aim straight down → small hop (×0.35).
    let ev = fire(&mut w, A, 0.0, -1.5, 1);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ShotFired { archetype: GunArchetype::SuperShotgun, pellets: 9, .. })));
    let v_ground = w.player(A).unwrap().mv.vel.y;
    // Air: the second barrel from mid-air kicks ×1.0 — a shotgun-jump.
    settle(&mut w, 40);
    place(&mut w, A, Vec3::new(10.0, 30.0, 8.0));
    let vy0 = w.player(A).unwrap().mv.vel.y;
    let _ = fire(&mut w, A, 0.0, -1.5, 1);
    let v_air = w.player(A).unwrap().mv.vel.y - vy0;
    assert!(v_air > 8.0, "air recoil ≈ 9 m/s: {v_air}");
    assert!(v_ground < v_air, "ground ×0.35 ({v_ground}) < air ({v_air})");
    assert_eq!(w.player(A).unwrap().mv.tumble, 0.0, "recoil never tumbles");
}

#[test]
fn projectiles_inherit_shooter_velocity() {
    // Rockets (and every other projectile) still inherit 100 % of the shooter's velocity.
    let fired_vel = |moving: bool| {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(10.0, 12.0, 20.0));
        place(&mut w, B, Vec3::new(10.0, 8.0, -12.0));
        give(&mut w, A, "rocket launcher", &[Mechanic::Explodes], 1);
        place(&mut w, A, Vec3::new(10.0, 30.0, 20.0));
        if moving {
            w.player_mut(A).unwrap().mv.vel = Vec3::new(0.0, 0.0, -30.0);
        }
        let ev = fire(&mut w, A, 0.0, 0.0, 1);
        ev.iter()
            .find_map(|e| match e {
                GameEvent::ProjectileFired { owner, vel, .. } if *owner == A => Some(*vel),
                _ => None,
            })
            .expect("rocket")
    };
    let (still, moving) = (fired_vel(false), fired_vel(true));
    assert!(moving.z - still.z < -25.0, "30 m/s inherited: {still} → {moving}");
}

/// Aim yaw / pitch from `id`'s eye at `p`.
fn aim_at_point(w: &World, id: PlayerId, p: Vec3) -> (f32, f32) {
    let eye = w.player(id).unwrap().eye(w.params());
    let d = (p - eye).normalize();
    ((-d.x).atan2(-d.z), d.y.asin())
}

#[test]
fn hitscan_hand_cannon_is_instant_and_earns_the_momentum_bonus() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(10.0, 12.0, 20.0));
    place(&mut w, B, Vec3::new(10.0, 8.0, -12.0));
    settle(&mut w, 30);
    give(&mut w, A, "hand cannon", &[], 1);
    // A flies toward B at 30 m/s and fires: the shooter's own speed along the aim is the bonus.
    place(&mut w, A, Vec3::new(10.0, 12.0, 20.0));
    w.player_mut(A).unwrap().mv.vel = Vec3::new(0.0, 0.0, -30.0);
    let chest = w.hull_of(B).unwrap().center();
    let (yaw, pitch) = aim_at_point(&w, A, chest);
    let inp = PlayerInput { yaw, pitch, buttons: buttons::PRIMARY, ..Default::default() };
    let ev = w.tick(&[(A, inp)]).to_vec();
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::ProjectileFired { owner, .. } if *owner == A)), "no slug");
    let bonus = ev.iter().find_map(|e| match e {
        GameEvent::Hit { target, kind: HitKind::Shot, bonus, .. } if *target == B => Some(*bonus),
        _ => None,
    });
    let b = bonus.expect("hit on the trigger tick");
    assert!(b > 0.4 && b <= 0.6, "30 m/s along the aim → bonus {b}");
    let trace = ev.iter().find_map(|e| match e {
        GameEvent::ShotTrace { archetype: GunArchetype::HandCannon, from, to, hit_player, .. } => Some((*from, *to, *hit_player)),
        _ => None,
    });
    let (from, to, who) = trace.expect("tracer event");
    assert_eq!(who, Some(B));
    assert!((to - chest).length() < 1.0 && (from - to).length() > 20.0, "tracer muzzle → impact: {from} → {to}");
}

#[test]
fn hitscan_hits_a_fast_mover_that_a_slug_misses() {
    // B streaks sideways at 40 m/s, 60 m away (tether-swing speed).
    let setup = || {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(10.0, 8.0, 30.0));
        place(&mut w, B, Vec3::new(0.0, 30.0, -30.0));
        settle(&mut w, 30);
        give(&mut w, A, "hand cannon", &[], 1);
        place(&mut w, B, Vec3::new(0.0, 30.0, -30.0));
        w.player_mut(B).unwrap().mv.vel = Vec3::new(40.0, 0.0, 0.0);
        w
    };
    let hits_b = |ev: &[GameEvent]| ev.iter().filter(|e| matches!(e, GameEvent::Hit { target, .. } if *target == B)).count();
    // Hitscan: aim where the crosshair shows B (one tick of movement ahead) and it lands.
    let mut w = setup();
    let b = w.player(B).unwrap();
    let seen = w.hull_of(B).unwrap().center() + b.mv.vel * bd_core::DT;
    let (yaw, pitch) = aim_at_point(&w, A, seen);
    let ev = fire(&mut w, A, yaw, pitch, 1);
    assert_eq!(hits_b(&ev), 1, "hitscan lands on the 40 m/s target");
    // The old 140 m/s slug, aimed the same way, arrives ~0.4 s later — 17 m behind.
    let mut w = setup();
    let eye = w.player(A).unwrap().eye(w.params());
    let dir = (w.hull_of(B).unwrap().center() - eye).normalize();
    let mut slug = bd_sim::projectile::ProjectileSpawn::ring_blast(A, eye + dir * 0.6, dir);
    slug.kind = bd_sim::projectile::ProjectileKind::Bolt;
    slug.hit_kind = HitKind::Shot;
    slug.vel = dir * 140.0;
    slug.radius = 0.2;
    slug.life = 2.0;
    w.fire_projectile(slug, None, Some(A));
    let mut ev = Vec::new();
    for _ in 0..120 {
        ev.extend_from_slice(w.tick(&[]));
    }
    assert_eq!(hits_b(&ev), 0, "the slug misses the same shot");
}

#[test]
fn hitscan_gun_is_lag_compensated() {
    let shoot = |rewind: bool| {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(8.0, 8.0, 8.0));
        place(&mut w, B, Vec3::new(8.0, 8.0, -4.0));
        settle(&mut w, 30);
        give(&mut w, A, "hand cannon", &[], 1);
        while w.tick < 120 {
            w.tick(&[]);
        }
        // B dodges; A's client (20 ticks ≈ 167 ms behind) still shows B at the old spot.
        place(&mut w, B, Vec3::new(15.0, 8.0, -4.0));
        settle(&mut w, 2);
        let view = (w.tick - 20) as f64;
        let seen = w.history.hull_at(B, view).unwrap().center();
        let (yaw, pitch) = aim_at_point(&w, A, seen);
        if rewind {
            w.set_view_tick(A, Some(view));
        }
        let ev = fire(&mut w, A, yaw, pitch, 1);
        ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: HitKind::Shot, .. } if *target == B))
    };
    assert!(!shoot(false), "unrewound: miss");
    assert!(shoot(true), "rewound to the shooter's view: hit");
}

#[test]
fn shotgun_pellets_are_rays_with_a_fixed_spread() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(10.0, 8.0, 8.0));
    settle(&mut w, 30);
    give(&mut w, A, "shotgun", &[Mechanic::Shoots], 3);
    let eye = w.player(A).unwrap().eye(w.params());
    let pitch = -0.12f32;
    let aim = bd_core::input::look_dir(0.0, pitch);
    let ev = fire(&mut w, A, 0.0, pitch, 1);
    let ends: Vec<Vec3> = ev
        .iter()
        .filter_map(|e| match e {
            GameEvent::ShotTrace { archetype: GunArchetype::SuperShotgun, to, normal, .. } => {
                assert!(*normal != Vec3::ZERO, "pellet ended on the floor");
                Some(*to)
            }
            _ => None,
        })
        .collect();
    assert_eq!(ends.len(), 9, "one ray per pellet");
    let max_angle = ends.iter().map(|p| (*p - eye).normalize().dot(aim).clamp(-1.0, 1.0).acos().to_degrees()).fold(0.0f32, f32::max);
    assert!(max_angle <= 4.0 + 0.3 && max_angle > 3.0, "4° cone: {max_angle}");
    // Deterministic, learnable pattern: the same shot twice lands the same.
    let mut w2 = world(1);
    place(&mut w2, A, Vec3::new(10.0, 8.0, 8.0));
    settle(&mut w2, 30);
    give(&mut w2, A, "shotgun", &[Mechanic::Shoots], 3);
    let ev2 = fire(&mut w2, A, 0.0, pitch, 1);
    let ends2: Vec<Vec3> = ev2.iter().filter_map(|e| if let GameEvent::ShotTrace { to, .. } = e { Some(*to) } else { None }).collect();
    assert_eq!(ends, ends2);
    // Falloff curve: full to 12 m, 30 % floor.
    use bd_sim::weapons::falloff;
    assert_eq!(falloff(12.0, 5.0), 1.0);
    assert!((falloff(12.0, 21.0) - 0.5).abs() < 1e-5);
    assert_eq!(falloff(12.0, 100.0), 0.3);
    assert_eq!(falloff(0.0, 100.0), 1.0);
}

#[test]
fn ring_blast_is_a_fast_fat_bolt() {
    let t = bd_sim::tuning::CombatTuning::default();
    assert_eq!((t.ring.blast_speed, t.ring.blast_radius), (180.0, 0.35));
    // A blast passing 0.3 m beside B's hull still connects (swept 0.35 m sphere); 0.5 m misses.
    let graze = |side: f32| {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(10.0, 8.0, 8.0));
        place(&mut w, B, Vec3::new(10.0, 8.0, -10.0));
        settle(&mut w, 40);
        let hull = w.hull_of(B).unwrap();
        let p = Vec3::new(hull.max.x + side, hull.center().y, hull.center().z);
        let (yaw, pitch) = aim_at_point(&w, A, p);
        let ev = fire(&mut w, A, yaw, pitch, 1);
        let v = ev.iter().find_map(|e| match e {
            GameEvent::ProjectileFired { owner, vel, .. } if *owner == A => Some(vel.length()),
            _ => None,
        });
        assert!(v.is_some_and(|v| (v - 180.0).abs() < 2.0), "180 m/s: {v:?}");
        let mut ev = ev;
        for _ in 0..30 {
            ev.extend_from_slice(w.tick(&[]));
        }
        ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: HitKind::RingBlast, .. } if *target == B))
    };
    assert!(graze(0.25), "grazing blast connects");
    assert!(!graze(0.5), "a clear miss stays a miss");
}

#[test]
fn hitscan_bots_are_deterministic() {
    let run = || {
        let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 9, check_invariants: true, ..SimConfig::default() };
        let mut w = World::new(cfg);
        w.cfg.rules.willpower = WillpowerMode::Infinite;
        let mut brains = bd_sim::bots::add_bots(&mut w, 1, 4, 0.7, 9);
        for b in &mut brains {
            b.casts_enabled = false;
        }
        w.start_live();
        let mut inputs = Vec::new();
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut shots = 0;
        let mut hits = 0;
        for k in 0..(20 * 120) {
            if k == 240 {
                for (i, b) in brains.iter().enumerate() {
                    let (name, m, count): (&str, &[Mechanic], u8) = match i % 3 {
                        0 => ("hand cannon", &[], 1),
                        1 => ("shotgun", &[Mechanic::Shoots], 3),
                        _ => ("minigun", &[Mechanic::Spin], 1),
                    };
                    let mut r = recipe(name, Form::Gun, m, Element::Light, 0.6);
                    r.count = count;
                    let eye = w.player(b.id).unwrap().eye(w.params());
                    let _ = w.cast(b.id, r, None, Aim::new(eye, Vec3::NEG_Z));
                }
            }
            inputs.clear();
            bd_sim::bots::drive(&mut w, &mut brains, &mut inputs);
            for e in w.tick(&inputs) {
                shots += matches!(e, GameEvent::ShotTrace { .. }) as u32;
                hits += matches!(e, GameEvent::Hit { kind: HitKind::Shot, .. }) as u32;
                for byte in format!("{e:?}").bytes() {
                    h = (h ^ byte as u64).wrapping_mul(0x0100_0000_01b3);
                }
            }
        }
        (h, w.state_hash(), shots, hits)
    };
    let a = run();
    assert!(a.2 > 50 && a.3 > 0, "bots fired hitscan guns and hit something: {a:?}");
    assert_eq!(a, run(), "same seed → identical events and state");
}

#[test]
fn shotgun_pellets_fall_off_with_distance() {
    let dmg_at = |dist: f32| {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(10.0, 8.0, 8.0));
        place(&mut w, B, Vec3::new(10.0, 8.0, 8.0 - dist));
        settle(&mut w, 30);
        give(&mut w, A, "shotgun", &[Mechanic::Shoots], 3);
        let ev = fire(&mut w, A, 0.0, -0.05, 1);
        let mut total = 0.0;
        let mut evs = ev;
        for _ in 0..40 {
            evs.extend_from_slice(w.tick(&[]));
        }
        for e in &evs {
            if let GameEvent::Hit { target, damage, .. } = e {
                if *target == B {
                    total += damage;
                }
            }
        }
        total
    };
    let near = dmg_at(4.0);
    let far = dmg_at(26.0);
    assert!(near > 40.0, "point blank ≈ 9 × 8: {near}");
    assert!(far < near * 0.6, "falloff beyond 12 m: {far} vs {near}");
}

#[test]
fn rocket_self_knockback_without_self_damage() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(10.0, 8.0, 8.0));
    settle(&mut w, 30);
    give(&mut w, A, "rocket launcher", &[Mechanic::Explodes], 1);
    let ev = fire(&mut w, A, 0.0, -1.5, 1);
    let mut evs = ev;
    for _ in 0..10 {
        evs.extend_from_slice(w.tick(&[]));
    }
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Explosion { .. })));
    let p = w.player(A).unwrap();
    assert_eq!(p.health(), 100.0, "no self damage");
    assert!(p.mv.vel.y > 5.0 || p.mv.pos.y > 9.0, "rocket jump: {} {}", p.mv.vel, p.mv.pos);
}

#[test]
fn railgun_hitscan_pierces_and_is_lag_compensated() {
    let mut w = world(3);
    place(&mut w, A, Vec3::new(8.0, 8.0, 8.0));
    place(&mut w, B, Vec3::new(8.0, 8.0, -2.0));
    place(&mut w, C, Vec3::new(8.0, 8.0, -8.0));
    settle(&mut w, 60);
    // Straight line through both.
    let eye = w.player(A).unwrap().eye(w.params());
    let n = w.hitscan(HitscanShot {
        range: 100.0,
        element: Element::Electric,
        pierce: 2,
        wall: 1.0,
        ..HitscanShot::new(A, eye, (w.hull_of(C).unwrap().center() - eye).normalize(), 45.0, 20.0)
    });
    assert_eq!(n, 2, "pierces B, hits C");
    assert!(w.events().iter().any(|e| matches!(e, GameEvent::Hitscan { hits: 2, .. })));
    // Lag compensation: B dodges; A's client (20 ticks behind) still sees B at the old spot.
    let mut w = world(2);
    place(&mut w, A, Vec3::new(8.0, 8.0, 8.0));
    place(&mut w, B, Vec3::new(8.0, 8.0, -2.0));
    while w.tick < 60 {
        w.tick(&[]);
    }
    place(&mut w, B, Vec3::new(15.0, 8.0, -2.0));
    settle(&mut w, 2);
    let view = (w.tick - 20) as f64;
    let seen = w.history.hull_at(B, view).unwrap().center();
    let eye = w.player(A).unwrap().eye(w.params());
    let shot = HitscanShot { range: 100.0, wall: 1.0, ..HitscanShot::new(A, eye, (seen - eye).normalize(), 45.0, 20.0) };
    assert_eq!(w.hitscan(shot), 0, "unrewound: miss");
    w.set_view_tick(A, Some(view));
    assert_eq!(w.hitscan(shot), 1, "rewound: hit");
}

#[test]
fn railgun_construct_charges_then_fires_hitscan() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(8.0, 8.0, 8.0));
    place(&mut w, B, Vec3::new(8.0, 8.0, -8.0));
    settle(&mut w, 30);
    give(&mut w, A, "railgun", &[Mechanic::Pierce], 1);
    let eye = w.player(A).unwrap().eye(w.params());
    let d = (w.hull_of(B).unwrap().center() - eye).normalize();
    let ev = fire(&mut w, A, (-d.x).atan2(-d.z), d.y.asin(), 70);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ShotFired { archetype: GunArchetype::Railgun, .. })));
    let hit = ev.iter().find_map(|e| match e {
        GameEvent::Hit { target, damage, impact, .. } if *target == B => Some((*damage, impact.heavy)),
        _ => None,
    });
    assert_eq!(hit, Some((bd_sim::weapons::gun_spec(GunArchetype::Railgun).damage, true)));
}

#[test]
fn head_zone_crits_and_ko_confirmation() {
    use bd_sim::combat::HitInfo;
    let mut w = world(3);
    place(&mut w, B, Vec3::new(8.0, 8.0, -2.0));
    settle(&mut w, 30);
    let hull = w.hull_of(B).unwrap();
    let head = Vec3::new(hull.center().x, hull.max.y - 0.1, hull.max.z);
    let body = Vec3::new(hull.center().x, hull.min.y + 0.5, hull.max.z);
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::Shot, 10.0, 0.0, Vec3::NEG_Z, head));
    assert!((r.damage - 15.0).abs() < 1e-3, "headshot ×1.5");
    assert!(w.events().iter().any(|e| matches!(e, GameEvent::Hit { crit: true, .. })));
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::Shot, 10.0, 0.0, Vec3::NEG_Z, body));
    assert!((r.damage - 10.0).abs() < 1e-3);
    let r = w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, 10.0, 0.0, Vec3::NEG_Z, head));
    assert!((r.damage - 10.0).abs() < 1e-3, "melee exempt");
    // Streaks, doubles, shutdowns.
    w.player_mut(C).unwrap().streak = 3;
    w.apply_hit(HitInfo::new(B, Some(A), HitKind::Shot, 1.0, 0.0, Vec3::NEG_Z, body));
    w.apply_hit(HitInfo::new(C, Some(A), HitKind::Shot, 1.0, 0.0, Vec3::NEG_Z, body));
    // Smash model: KOs through the blast zone (both this tick).
    w.player_mut(B).unwrap().mv.pos.y = -100.0;
    w.player_mut(C).unwrap().mv.pos.y = -100.0;
    w.tick(&[]);
    let kos: Vec<(u16, bool, bool)> = w
        .events()
        .iter()
        .filter_map(|e| match e {
            GameEvent::Ko { killer: Some(k), streak, double, shutdown, .. } if *k == A => Some((*streak, *double, *shutdown)),
            _ => None,
        })
        .collect();
    assert_eq!(kos, vec![(1, false, false), (2, true, true)]);
}

/// A bot with a minigun vs a pinned target (optionally strafing at up to 30 m/s): (shots, hits,
/// first shot tick).
fn bot_duel(strafe: bool, difficulty: f32) -> (u32, u32, Option<u32>, f32) {
    use bd_sim::bots::BotBrain;
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, 8.0, 20.0));
    place(&mut w, B, Vec3::new(0.0, 8.0, -5.0));
    settle(&mut w, 60);
    give(&mut w, A, "minigun", &[Mechanic::Spin], 1);
    let (a0, b0) = (w.player(A).unwrap().mv.pos, w.player(B).unwrap().mv.pos);
    let mut bot = BotBrain::new(A, difficulty, 3);
    bot.casts_enabled = false;
    let (mut shots, mut hits, mut first) = (0, 0, None);
    for k in 0..(6 * 120) {
        let t = k as f32 * bd_core::DT;
        let (bx, bv) = if strafe { (12.0 * (t * 2.5).sin(), 30.0 * (t * 2.5).cos()) } else { (0.0, 0.0) };
        {
            let a = w.player_mut(A).unwrap();
            a.mv.pos = a0;
            a.mv.vel = Vec3::ZERO;
        }
        {
            let b = w.player_mut(B).unwrap();
            b.mv.pos = b0 + Vec3::X * bx;
            b.mv.vel = Vec3::X * bv;
            b.damage_pct = 0.0;
        }
        let act = bot.think(&w);
        for e in w.tick(&[(A, act.input)]) {
            match e {
                GameEvent::ShotFired { owner, .. } if *owner == A => {
                    shots += 1;
                    first.get_or_insert(k);
                }
                GameEvent::Hit { target, source: Some(s), .. } if *target == B && *s == A => hits += 1,
                _ => {}
            }
        }
    }
    (shots, hits, first, bot.aim_profile().reaction)
}

#[test]
fn bot_aim_is_human_not_an_aimbot() {
    use bd_sim::bots::AimProfile;
    let (lo, hi) = (AimProfile::for_difficulty(0.0), AimProfile::for_difficulty(1.0));
    assert!((lo.reaction - 0.35).abs() < 1e-6 && (hi.reaction - 0.15).abs() < 1e-6, "150–350 ms");
    assert!(lo.jitter > hi.jitter && lo.tracking > hi.tracking && lo.overshoot_chance > hi.overshoot_chance && lo.turn_rate < hi.turn_rate);
    let acc = |r: (u32, u32, Option<u32>, f32)| r.1 as f32 / r.0.max(1) as f32;
    let still = bot_duel(false, 0.7);
    let strafer = bot_duel(true, 0.7);
    let strafer_easy = bot_duel(true, 0.2);
    eprintln!("bot minigun (shots, hits, first tick, reaction): still {still:?}, strafer {strafer:?}, easy strafer {strafer_easy:?}");
    // Reaction: no trigger before the target has been seen for the bot's reaction time.
    let first = still.2.expect("fired");
    assert!(first as f32 * bd_core::DT >= still.3 - 0.01, "reaction {} s, first shot at tick {first}", still.3);
    assert!(acc(still) > 0.6, "a still target is easy: {still:?}");
    assert!(acc(strafer) < acc(still) * 0.75, "tracking a 30 m/s strafer is hard: {strafer:?} vs {still:?}");
    assert!(acc(strafer_easy) < acc(strafer), "difficulty matters: easy {strafer_easy:?} vs {strafer:?}");
}
