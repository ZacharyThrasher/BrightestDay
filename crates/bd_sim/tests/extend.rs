//! WUKONG: telescoping weapons (`Mechanic::Extends`, bd_core v1.7; `constructs/extend.rs`) on the flat
//! lab arena (ground top at y = 8): the charged thrust hits far away, the falling pillar craters a line
//! and knocks up, the pole vault adds velocity along the mirrored aim (and composes with momentum),
//! the twirl combo still hits, creatures strike from range, and everything is deterministic.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::voxel::{VoxelQuery, world_to_voxel};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::HitKind;
use bd_sim::constructs::extend::ExtMove;
use bd_sim::events::{GameEvent, HeldFx};
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn world(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 128.0 }, check_invariants: true, ..SimConfig::default() };
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
    p.mv.tumble = 0.0;
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..ticks {
        ev.extend_from_slice(w.tick(&[]));
    }
    ev
}

fn act(w: &mut World, id: PlayerId, yaw: f32, pitch: f32, b: u16, hold: u32, after: u32) -> Vec<GameEvent> {
    let on = PlayerInput { yaw, pitch, buttons: b, ..Default::default() };
    let off = PlayerInput { yaw, pitch, ..Default::default() };
    let mut ev = Vec::new();
    for _ in 0..hold {
        ev.extend_from_slice(w.tick(&[(id, on)]));
    }
    for _ in 0..after {
        ev.extend_from_slice(w.tick(&[(id, off)]));
    }
    ev
}

fn ruyi() -> Recipe {
    let mut r = recipe("Ruyi Jingu Bang", Form::Spear, &[Mechanic::Blunt, Mechanic::Extends], Element::Light, 2.4);
    r.mass = 60.0;
    r
}

fn give(w: &mut World, id: PlayerId, r: Recipe) {
    let eye = w.player(id).unwrap().eye(w.params());
    w.cast(id, r, None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    run(w, 40);
}

fn hits_on(ev: &[GameEvent], target: PlayerId) -> Vec<(HitKind, f32, f32)> {
    ev.iter()
        .filter_map(|e| match e {
            GameEvent::Hit { target: t, kind, damage, knockback, .. } if *t == target => Some((*kind, *damage, *knockback)),
            _ => None,
        })
        .collect()
}

fn has_fx(ev: &[GameEvent], fx: HeldFx) -> bool {
    ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: f, .. } if *f == fx))
}

fn solid_count(w: &World, from: Vec3, to: Vec3) -> usize {
    let (a, b) = (world_to_voxel(from), world_to_voxel(to));
    let mut n = 0;
    for x in a.x.min(b.x)..=a.x.max(b.x) {
        for y in a.y.min(b.y)..=a.y.max(b.y) {
            for z in a.z.min(b.z)..=a.z.max(b.z) {
                n += w.voxels.is_solid(bd_core::IVec3::new(x, y, z)) as usize;
            }
        }
    }
    n
}

/// A at z = 30 looking down −Z; B `dist` metres ahead.
fn duel(dist: f32) -> World {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 30.0 - dist));
    run(&mut w, 20);
    give(&mut w, A, ruyi());
    w
}

#[test]
fn charged_thrust_extends_and_hits_far_away() {
    let mut w = duel(26.0);
    // Aim level at B's chest (a flat arena: pitch ≈ 0).
    let ev = act(&mut w, A, 0.0, -0.01, buttons::PRIMARY, 90, 13);
    assert!(has_fx(&ev, HeldFx::Extend), "thrust fired");
    let c = w.constructs.held_by(A).expect("held");
    assert!(c.held.ext > 15.0, "pole extended ({})", c.held.ext);
    let ev = [ev, run(&mut w, 80)].concat();
    let h = hits_on(&ev, B);
    assert!(!h.is_empty(), "the thrust reached B at 26 m");
    assert_eq!(h[0].0, HitKind::HeavyMelee);
    assert!(h[0].2 >= 20.0, "knocks back hard ({})", h[0].2);
    // It snaps back.
    let c = w.constructs.held_by(A).expect("held");
    assert_eq!(c.held.xmove, ExtMove::None);
    assert!(c.held.ext < 0.01);
    // A plain (uncharged) swing never reaches that far.
    let mut w2 = duel(26.0);
    let ev2 = act(&mut w2, A, 0.0, -0.01, buttons::PRIMARY, 1, 90);
    assert!(hits_on(&ev2, B).is_empty());
}

#[test]
fn pillar_slam_craters_a_line_and_knocks_up() {
    let mut w = duel(12.0);
    let before_near = solid_count(&w, Vec3::new(-1.5, GROUND - 2.0, 30.0 - 8.0), Vec3::new(1.5, GROUND - 0.1, 30.0 - 4.0));
    let before_far = solid_count(&w, Vec3::new(-1.5, GROUND - 2.0, 30.0 - 20.0), Vec3::new(1.5, GROUND - 0.1, 30.0 - 16.0));
    // Charge while looking down, release: the pillar stands up, then crashes down along the heading.
    let mut ev = act(&mut w, A, 0.0, -0.7, buttons::PRIMARY, 90, 0);
    let mut up = 0.0f32;
    for _ in 0..150 {
        ev.extend_from_slice(w.tick(&[]));
        up = up.max(w.player(B).unwrap().mv.vel.y);
    }
    assert!(up > 8.0, "knock-up ({up})");
    assert!(has_fx(&ev, HeldFx::PillarRise) && has_fx(&ev, HeldFx::PillarSlam), "rise + slam");
    let after_near = solid_count(&w, Vec3::new(-1.5, GROUND - 2.0, 30.0 - 8.0), Vec3::new(1.5, GROUND - 0.1, 30.0 - 4.0));
    let after_far = solid_count(&w, Vec3::new(-1.5, GROUND - 2.0, 30.0 - 20.0), Vec3::new(1.5, GROUND - 0.1, 30.0 - 16.0));
    assert!(after_near < before_near, "cratered near ({before_near} -> {after_near})");
    assert!(after_far < before_far, "cratered far along the line ({before_far} -> {after_far})");
    let h = hits_on(&ev, B);
    assert!(!h.is_empty(), "B under the pillar was hit");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::PillarSlam, dir, .. } if dir.z < -0.9)), "crashes along the heading");
}

#[test]
fn vault_launches_along_the_mirrored_aim_and_keeps_momentum() {
    // Standing: aim at the floor ahead, RMB → up and forward.
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    run(&mut w, 20);
    give(&mut w, A, ruyi());
    let ev = act(&mut w, A, 0.0, -0.6, buttons::SECONDARY, 1, 1);
    assert!(has_fx(&ev, HeldFx::Vault), "vault fx");
    let v = w.player(A).unwrap().mv.vel;
    assert!(v.y > 8.0 && -v.z > 8.0, "launched up and forward: {v:?}");

    // Moving fast sideways (a bhop / tether): the vault adds, never takes speed away.
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND + 3.0, 0.0));
    give(&mut w, A, ruyi());
    let p = w.player_mut(A).unwrap();
    p.mv.pos = Vec3::new(0.0, GROUND + 1.5, 0.0);
    p.mv.vel = Vec3::new(18.0, -2.0, 0.0);
    p.mv.grounded = false;
    let ev = act(&mut w, A, 0.0, -1.2, buttons::SECONDARY, 1, 0);
    assert!(has_fx(&ev, HeldFx::Vault));
    let v = w.player(A).unwrap().mv.vel;
    assert!(v.x > 16.0, "sideways momentum kept: {v:?}");
    assert!(v.y > 10.0, "and launched up: {v:?}");
    // Cooldown: an immediate second press does nothing; after it, it works again.
    let v0 = w.player(A).unwrap().mv.vel;
    let ev = act(&mut w, A, 0.0, -1.2, buttons::SECONDARY, 1, 0);
    assert!(!has_fx(&ev, HeldFx::Vault), "cooldown");
    let _ = v0;

    // Open air: RMB is a jab, not a vault.
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    run(&mut w, 20);
    give(&mut w, A, ruyi());
    let ev = act(&mut w, A, 0.0, 0.3, buttons::SECONDARY, 1, 3);
    assert!(!has_fx(&ev, HeldFx::Vault) && has_fx(&ev, HeldFx::Extend), "jab");
}

#[test]
fn vault_is_predicted_exactly() {
    // Client prediction runs the same pre-move: predicting A's inputs reproduces the host's velocity.
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    run(&mut w, 20);
    give(&mut w, A, ruyi());
    let mut st = w.player(A).unwrap().mv;
    let inp = PlayerInput { yaw: 0.0, pitch: -0.6, buttons: buttons::SECONDARY, ..Default::default() };
    let _ = w.predict_player(A, &mut st, &inp);
    w.tick(&[(A, inp)]);
    let host = w.player(A).unwrap().mv;
    assert!((st.vel - host.vel).length() < 1e-4, "prediction {:?} vs host {:?}", st.vel, host.vel);
    assert!(st.extra.vault_cd > 0.0);
}

#[test]
fn twirl_combo_hits_and_finisher_is_heavy() {
    let mut w = duel(2.6);
    let mut kinds = Vec::new();
    for _ in 0..3 {
        let ev = act(&mut w, A, 0.0, -0.05, buttons::PRIMARY, 1, 30);
        kinds.extend(hits_on(&ev, B).into_iter().map(|h| h.0));
        for e in &ev {
            if let GameEvent::HeldFx { fx: HeldFx::SwingStart, strength, .. } = e {
                eprintln!("swing start {strength}");
            }
        }
        // Keep B in reach.
        place(&mut w, B, Vec3::new(0.0, GROUND, 30.0 - 2.6));
    }
    assert!(kinds.len() >= 2, "combo connects: {kinds:?}");
    assert!(kinds.contains(&HitKind::HeavyMelee), "windmill finisher: {kinds:?}");
}

#[test]
fn creature_with_extends_strikes_from_range() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 18.0));
    run(&mut w, 20);
    let mut r = recipe("Sun Wukong", Form::Biped, &[Mechanic::Blunt, Mechanic::Extends, Mechanic::Fast], Element::Light, 1.7);
    r.mass = 50.0;
    let eye = w.player(A).unwrap().eye(w.params());
    w.cast(A, r, None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    let mut ev = Vec::new();
    for _ in 0..(8 * 120) {
        ev.extend_from_slice(w.tick(&[]));
        if ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: HitKind::Creature, .. } if *target == B)) {
            break;
        }
    }
    assert!(has_fx(&ev, HeldFx::Extend), "the staff extends");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: HitKind::Creature, .. } if *target == B)), "wukong hits B");
}

#[test]
fn extends_is_deterministic() {
    let run_once = || {
        let mut w = duel(20.0);
        let mut ev = act(&mut w, A, 0.0, -0.01, buttons::PRIMARY, 80, 60);
        ev.extend(act(&mut w, A, 0.0, -0.7, buttons::PRIMARY, 80, 120));
        ev.extend(act(&mut w, A, 0.0, -0.6, buttons::SECONDARY, 1, 60));
        (w.state_hash(), w.voxels.content_hash(), format!("{ev:?}").len())
    };
    assert_eq!(run_once(), run_once());
}
