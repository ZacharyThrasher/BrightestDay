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
fn shots_inherit_shooter_velocity_and_earn_closing_speed_bonus() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(10.0, 12.0, 20.0));
    place(&mut w, B, Vec3::new(10.0, 8.0, -12.0));
    give(&mut w, A, "hand cannon", &[], 1);
    // A flies toward B at 30 m/s and fires.
    w.player_mut(A).unwrap().mv.vel = Vec3::new(0.0, 0.0, -30.0);
    let eye = w.player(A).unwrap().eye(w.params());
    let chest = w.hull_of(B).unwrap().center();
    let d = (chest - eye).normalize();
    let yaw = (-d.x).atan2(-d.z);
    let pitch = d.y.asin();
    let ev = fire(&mut w, A, yaw, pitch, 1);
    let fired = ev.iter().find_map(|e| match e {
        GameEvent::ProjectileFired { owner, vel, .. } if *owner == A => Some(*vel),
        _ => None,
    });
    let v = fired.expect("shot");
    assert!(v.length() > 160.0, "140 m/s slug + 30 m/s inherited: {}", v.length());
    let mut ev = Vec::new();
    for _ in 0..40 {
        ev.extend_from_slice(w.tick(&[]));
    }
    let bonus = ev.iter().find_map(|e| match e {
        GameEvent::Hit { target, kind: HitKind::Shot, bonus, .. } if *target == B => Some(*bonus),
        _ => None,
    });
    let b = bonus.expect("hit");
    assert!(b > 0.4 && b <= 0.6, "closing 30 m/s beyond muzzle → bonus {b}");
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
    assert_eq!(p.health, 100.0, "no self damage");
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
        owner: A,
        construct: None,
        origin: eye,
        dir: (w.hull_of(C).unwrap().center() - eye).normalize(),
        range: 100.0,
        damage: 45.0,
        knockback: 20.0,
        element: Element::Electric,
        pierce: 2,
        wall: 1.0,
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
    let shot = HitscanShot { owner: A, construct: None, origin: eye, dir: (seen - eye).normalize(), range: 100.0, damage: 45.0, knockback: 20.0, element: Element::Light, pierce: 0, wall: 1.0 };
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
    assert_eq!(hit, Some((45.0, true)));
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
    w.apply_hit(HitInfo::new(B, Some(A), HitKind::Shot, 500.0, 0.0, Vec3::NEG_Z, body));
    w.apply_hit(HitInfo::new(C, Some(A), HitKind::Shot, 500.0, 0.0, Vec3::NEG_Z, body));
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
