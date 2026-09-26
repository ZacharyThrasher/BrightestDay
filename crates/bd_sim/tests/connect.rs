//! CONNECT (owner: "I feel like I'm slapping noodles around and it is hard to hit anyone"): scripted
//! duels against a strafing target, hit rate per weapon with the aim-assist knobs at their shipped
//! values vs neutral (`bd_sim::aim_assist::AimTuning::neutral` + the pre-pass lunge numbers).
//!
//! The attacker is a human-ish aimer: it tracks where the target *was* `REACTION` ago (no lead) plus
//! a per-trial angular error; the target ADAD-strafes at ground speed. Deterministic (fixed seeds).
//! `cargo test -p bd_sim --test connect -- --nocapture` prints the table.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::GameEvent;
use bd_sim::rng::SimRng;
use bd_sim::rules::WillpowerMode;
use bd_sim::tuning::CombatTuning;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;
/// Perception delay of the scripted aimer (ticks at 120 Hz): 100 ms.
const REACTION: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Weapon {
    Sword,
    Hammer,
    Pistol,
    ThrownAxe,
}

impl Weapon {
    /// (distance band m, aim error sigma deg, button, ticks to wait for the hit).
    fn setup(self) -> ((f32, f32), f32, u16, usize) {
        match self {
            Weapon::Sword => ((2.2, 5.0), 4.0, buttons::PRIMARY, 70),
            Weapon::Hammer => ((2.2, 4.5), 4.0, buttons::PRIMARY, 80),
            Weapon::Pistol => ((12.0, 30.0), 1.4, buttons::PRIMARY, 4),
            Weapon::ThrownAxe => ((7.0, 16.0), 2.0, buttons::SECONDARY, 110),
        }
    }

    fn give(self, w: &mut World) {
        let (form, name, m): (Form, &str, &[Mechanic]) = match self {
            Weapon::Sword => (Form::Sword, "sword", &[]),
            Weapon::Hammer => (Form::Hammer, "hammer", &[]),
            Weapon::Pistol => (Form::Gun, "hand cannon", &[]),
            Weapon::ThrownAxe => (Form::Axe, "axe", &[]),
        };
        let scale = if form == Form::Gun { 0.6 } else { bd_sim::constructs::form_spec(form).default_scale };
        let eye = w.player(A).unwrap().eye(w.params());
        w.cast(A, recipe(name, form, m, Element::Light, scale), None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    }
}

fn world(t: &CombatTuning) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 128.0 }, check_invariants: false, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.set_tuning(t.clone());
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w
}

fn place(w: &mut World, id: PlayerId, pos: Vec3, vel: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = vel;
    p.mv.grounded = true;
    p.mv.tumble = 0.0;
    p.mv.lunge = Default::default();
    p.damage_pct = 0.0;
}

fn yaw_pitch(d: Vec3) -> (f32, f32) {
    let d = d.normalize();
    ((-d.x).atan2(-d.z), d.y.clamp(-1.0, 1.0).asin())
}

/// Approximately N(0, 1) (Irwin-Hall, 3 uniforms).
fn normal(r: &mut SimRng) -> f32 {
    (r.f32() + r.f32() + r.f32() - 1.5) * 2.0
}

/// Hit rate of `weapon` over `trials` duels (fraction of attacks that landed at least one hit).
pub fn hit_rate(t: &CombatTuning, weapon: Weapon, trials: usize, seed: u64) -> f32 {
    let mut w = world(t);
    let mut rng = SimRng::new(seed);
    let ((d0, d1), sigma, button, wait) = weapon.setup();
    let mut hits = 0usize;
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0), Vec3::ZERO);
    place(&mut w, B, Vec3::new(0.0, GROUND, -30.0), Vec3::ZERO);
    for _ in 0..10 {
        w.tick(&[]);
    }
    if weapon != Weapon::ThrownAxe {
        weapon.give(&mut w);
        for _ in 0..30 {
            w.tick(&[]);
        }
    }
    for _ in 0..trials {
        if weapon == Weapon::ThrownAxe {
            weapon.give(&mut w);
            for _ in 0..30 {
                w.tick(&[]);
            }
        }
        let dist = rng.range(d0, d1);
        let side = if rng.f32() < 0.5 { -1.0 } else { 1.0 };
        let err_yaw = normal(&mut rng) * sigma.to_radians();
        let err_pitch = normal(&mut rng) * sigma.to_radians() * 0.5;
        place(&mut w, A, Vec3::new(0.0, GROUND, 0.0), Vec3::ZERO);
        place(&mut w, B, Vec3::new(rng.range(-1.0, 1.0), GROUND, -dist), Vec3::X * side * 9.0);
        let mut strafe = side;
        let mut flip = rng.range(0.2, 0.6);
        let mut seen: Vec<Vec3> = Vec::new();
        let warm = 45;
        let mut landed = false;
        for k in 0..(warm + wait) {
            // The target ADAD-strafes (faces A so move_x is lateral).
            flip -= bd_core::DT;
            if flip <= 0.0 {
                strafe = -strafe;
                flip = rng.range(0.25, 0.6);
            }
            let bpos = w.hull_of(B).unwrap().center();
            seen.push(bpos);
            let aeye = w.player(A).unwrap().eye(w.params());
            let (byaw, _) = yaw_pitch(aeye - bpos);
            let target = seen[seen.len().saturating_sub(REACTION + 1)];
            let (yaw, pitch) = yaw_pitch(target - aeye);
            let press = k == warm;
            let a_in = PlayerInput { yaw: yaw + err_yaw, pitch: pitch + err_pitch, buttons: if press { button } else { 0 }, ..Default::default() };
            let b_in = PlayerInput { yaw: byaw, pitch: 0.0, move_x: strafe as i8, ..Default::default() };
            let ev = w.tick(&[(A, a_in), (B, b_in)]);
            if k >= warm {
                landed |= ev.iter().any(|e| matches!(e, GameEvent::Hit { target, source: Some(s), .. } if *target == B && *s == A));
            }
        }
        hits += usize::from(landed);
    }
    hits as f32 / trials as f32
}

/// Prints the before/after table (`--nocapture`); asserts every weapon connects at least as often
/// with the shipped assist as with it neutralised.
#[test]
fn assist_raises_hit_rate_vs_strafing_target() {
    let shipped = CombatTuning::default();
    let neutral = legacy_tuning();
    let n = 120;
    let mut rows = Vec::new();
    for (k, wpn) in [Weapon::Sword, Weapon::Hammer, Weapon::Pistol, Weapon::ThrownAxe].into_iter().enumerate() {
        let before = hit_rate(&neutral, wpn, n, 11 + k as u64);
        let after = hit_rate(&shipped, wpn, n, 11 + k as u64);
        println!("CONNECT {wpn:?}: before {:.1}% -> after {:.1}% ({n} trials)", before * 100.0, after * 100.0);
        rows.push((wpn, before, after));
    }
    for (wpn, before, after) in rows {
        assert!(after >= before, "{wpn:?}: assist must not lower the hit rate ({before} -> {after})");
    }
}

/// The tuning before the connect pass: aim assist neutral, the old lunge numbers.
fn legacy_tuning() -> CombatTuning {
    let mut t = CombatTuning::default();
    t.assist.aim = bd_sim::aim_assist::AimTuning::neutral();
    let l = &mut t.assist.lunge;
    l.extra = 2.6;
    l.range_max = 5.5;
    l.cone_deg = 20.0;
    l.dash_speed = 26.0;
    l.dash_max_secs = 0.22;
    l.cooldown = 0.7;
    l.hit_resets_cooldown = false;
    l.aim_max_deg = 12.0;
    t
}

/// Spam-clicking duel (owner: "I don't even feel the lunge / I just end up spam clicking"): the
/// attacker walks at a strafing target starting 3-10 m away and clicks ~7 times a second for
/// 1.2 s. Returns (verdict counts over every melee swing start, trials that landed a hit).
fn spam_duel(t: &CombatTuning, weapon: Weapon, trials: usize, seed: u64) -> (std::collections::BTreeMap<String, usize>, usize) {
    let mut w = world(t);
    let mut rng = SimRng::new(seed);
    let mut verdicts = std::collections::BTreeMap::new();
    let mut landed_trials = 0;
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0), Vec3::ZERO);
    place(&mut w, B, Vec3::new(0.0, GROUND, -30.0), Vec3::ZERO);
    weapon.give(&mut w);
    for _ in 0..30 {
        w.tick(&[]);
    }
    for _ in 0..trials {
        let dist = rng.range(3.0, 10.0);
        let side = if rng.f32() < 0.5 { -1.0 } else { 1.0 };
        let err_yaw = normal(&mut rng) * 5f32.to_radians();
        place(&mut w, A, Vec3::new(0.0, GROUND, 0.0), Vec3::ZERO);
        place(&mut w, B, Vec3::new(rng.range(-1.0, 1.0), GROUND, -dist), Vec3::X * side * 9.0);
        let mut strafe = side;
        let mut flip = rng.range(0.2, 0.6);
        let mut seen: Vec<Vec3> = Vec::new();
        let mut next_click = 20 + rng.below(10) as usize;
        let mut landed = false;
        for k in 0..(20 + 144) {
            flip -= bd_core::DT;
            if flip <= 0.0 {
                strafe = -strafe;
                flip = rng.range(0.25, 0.6);
            }
            let bpos = w.hull_of(B).unwrap().center();
            seen.push(bpos);
            let aeye = w.player(A).unwrap().eye(w.params());
            let (byaw, _) = yaw_pitch(aeye - bpos);
            let target = seen[seen.len().saturating_sub(REACTION + 1)];
            let (yaw, pitch) = yaw_pitch(target - aeye);
            let press = k == next_click;
            if press {
                next_click = k + 14 + rng.below(8) as usize;
            }
            let a_in = PlayerInput { yaw: yaw + err_yaw, pitch, move_z: 1, buttons: if press { buttons::PRIMARY } else { 0 }, ..Default::default() };
            let b_in = PlayerInput { yaw: byaw, pitch: 0.0, move_x: strafe as i8, ..Default::default() };
            let ev = w.tick(&[(A, a_in), (B, b_in)]);
            landed |= ev.iter().any(|e| matches!(e, GameEvent::Hit { target, source: Some(s), .. } if *target == B && *s == A));
            let a = w.player(A).unwrap().assist;
            if a.verdict_tick == w.tick {
                *verdicts.entry(format!("{:?}", a.verdict)).or_insert(0) += 1;
            }
        }
        landed_trials += usize::from(landed);
    }
    (verdicts, landed_trials)
}

/// Lunge trigger rate when spam-clicking at a strafing target, before vs after; prints why the
/// lunge did not fire. Asserts the pass made it trigger more often.
#[test]
fn lunge_triggers_when_spam_clicking() {
    let n = 80;
    for (k, wpn) in [Weapon::Sword, Weapon::Hammer].into_iter().enumerate() {
        let mut rates = [0.0f32; 2];
        for (j, (label, t)) in [("before", legacy_tuning()), ("after", CombatTuning::default())].into_iter().enumerate() {
            let (v, landed) = spam_duel(&t, wpn, n, 101 + k as u64);
            let total: usize = v.values().sum();
            let lunged = v.get("Lunged").copied().unwrap_or(0);
            rates[j] = lunged as f32 / total.max(1) as f32;
            println!(
                "LUNGE {wpn:?} {label}: {lunged}/{total} swing starts lunged ({:.1}%), trials with a hit {landed}/{n} ({:.1}%); verdicts {v:?}",
                rates[j] * 100.0,
                landed as f32 / n as f32 * 100.0
            );
        }
        assert!(rates[1] > rates[0], "{wpn:?}: the lunge must trigger more often ({} -> {})", rates[0], rates[1]);
    }
}
