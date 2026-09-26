//! Hold-to-charge actions fire on RELEASE (or at max charge where that is the design) — never on
//! a second press. Exact tick-by-tick input: press on tick 0, hold 0.5 s (60 ticks), release, then
//! idle; every assertion names the tick the attack started relative to the release tick.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::HitKind;
use bd_sim::events::{GameEvent, HeldFx};
use bd_sim::hook::AssistEvent;
use bd_sim::rules::WillpowerMode;
use bd_sim::weapons::GunArchetype;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;
/// 0.5 s at 120 Hz.
const HOLD: usize = 60;

fn duel(dist: f32) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 6.0 - dist));
    for _ in 0..20 {
        w.tick(&[]);
    }
    w
}

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
    p.mv.tumble = 0.0;
    p.mv.lunge = Default::default();
}

fn give(w: &mut World, form: Form, m: &[Mechanic]) {
    let scale = bd_sim::constructs::form_spec(form).default_scale;
    let eye = w.player(A).unwrap().eye(w.params());
    w.cast(A, recipe("weapon", form, m, Element::Light, scale), None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    for _ in 0..30 {
        w.tick(&[]);
    }
}

fn aim_at(w: &World) -> (f32, f32) {
    let eye = w.player(A).unwrap().eye(w.params());
    let d = (w.hull_of(B).unwrap().center() - eye).normalize();
    ((-d.x).atan2(-d.z), d.y.asin())
}

/// Press on tick 0, hold for `hold` ticks, release (tick `hold` is the first without the button),
/// run `after` more ticks. Returns the events of every tick.
fn press_hold_release(w: &mut World, b: u16, hold: usize, after: usize) -> Vec<Vec<GameEvent>> {
    let (yaw, pitch) = aim_at(w);
    (0..hold + after)
        .map(|k| {
            let inp = PlayerInput { yaw, pitch, buttons: if k < hold { b } else { 0 }, ..Default::default() };
            w.tick(&[(A, inp)]).to_vec()
        })
        .collect()
}

fn first_tick(ticks: &[Vec<GameEvent>], f: impl Fn(&GameEvent) -> bool) -> Option<usize> {
    ticks.iter().position(|ev| ev.iter().any(&f))
}

fn fx(ticks: &[Vec<GameEvent>], want: HeldFx, min_strength: f32) -> Option<usize> {
    first_tick(ticks, |e| matches!(e, GameEvent::HeldFx { owner, fx, strength, .. } if *owner == A && *fx == want && *strength >= min_strength))
}

fn heavy_hit(ticks: &[Vec<GameEvent>]) -> Option<usize> {
    first_tick(ticks, |e| matches!(e, GameEvent::Hit { target, kind: HitKind::HeavyMelee, .. } if *target == B))
}

/// Melee forms: tap starts the quick swing, holding past the grace coils a heavy charge, and the
/// RELEASE tick starts the heavy (SwingStart strength 2 — the swing presentation keys off it), which
/// then connects — with no second press.
#[test]
fn melee_heavy_starts_on_the_release_tick() {
    for (form, dist) in [(Form::Sword, 1.8), (Form::Hammer, 1.8), (Form::Gauntlet, 1.2), (Form::Axe, 1.7), (Form::Spear, 2.5)] {
        let mut w = duel(dist);
        give(&mut w, form, &[]);
        let t = press_hold_release(&mut w, buttons::PRIMARY, HOLD, 90);
        let coil = fx(&t, HeldFx::ChargeStart, 0.0).unwrap_or_else(|| panic!("{form:?}: no charge"));
        assert!(coil < HOLD, "{form:?}: coiled while held ({coil})");
        let start = fx(&t[HOLD..], HeldFx::SwingStart, 1.9).map(|k| k + HOLD);
        assert_eq!(start, Some(HOLD), "{form:?}: the heavy swing starts on the release tick");
        let active = fx(&t, HeldFx::SwingActive, 1.9).unwrap_or_else(|| panic!("{form:?}: heavy never went live"));
        assert!(active >= HOLD && active <= HOLD + 30, "{form:?}: heavy live {} ticks after release", active as i64 - HOLD as i64);
        let hit = heavy_hit(&t).unwrap_or_else(|| panic!("{form:?}: the released heavy never hit"));
        assert!(hit >= HOLD, "{form:?}: heavy hit before release ({hit})");
    }
}

/// Holding to full charge still fires by itself (the design), also announcing the heavy swing.
#[test]
fn melee_heavy_fires_at_full_charge_while_held() {
    let mut w = duel(1.8);
    give(&mut w, Form::Sword, &[]);
    let t = press_hold_release(&mut w, buttons::PRIMARY, 240, 60);
    let start = fx(&t, HeldFx::SwingStart, 1.9).expect("heavy swing at max charge");
    assert!(start < 240, "fires at max charge without releasing ({start})");
    assert!(heavy_hit(&t).is_some_and(|k| k < 240));
}

/// The melee lunge rides the swing start: the press lunges in (tick 0), the coil holds, and the
/// released heavy connects — no second press anywhere in the chain.
#[test]
fn lunge_then_released_heavy_connects() {
    let mut w = duel(4.2);
    give(&mut w, Form::Sword, &[]);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 1.8));
    for _ in 0..10 {
        w.tick(&[]);
    }
    let t = press_hold_release(&mut w, buttons::PRIMARY, HOLD, 60);
    let lunges: Vec<usize> = t
        .iter()
        .enumerate()
        .filter(|(_, ev)| ev.iter().any(|e| matches!(e, GameEvent::Assist { player, kind: AssistEvent::Lunge, .. } if *player == A)))
        .map(|(k, _)| k)
        .collect();
    assert_eq!(lunges.first(), Some(&0), "lunge on the press: {lunges:?}");
    assert_eq!(fx(&t, HeldFx::SwingStart, 1.9), Some(HOLD), "heavy on the release tick");
    assert!(heavy_hit(&t).is_some_and(|k| k >= HOLD), "the released heavy connects after the lunge");
}

/// Bow: draws while held, looses on the release tick.
#[test]
fn bow_looses_on_release() {
    let mut w = duel(12.0);
    give(&mut w, Form::Bow, &[]);
    let t = press_hold_release(&mut w, buttons::PRIMARY, HOLD, 30);
    let shot = first_tick(&t, |e| matches!(e, GameEvent::ShotFired { owner, archetype: GunArchetype::Bow, .. } if *owner == A));
    assert_eq!(shot, Some(HOLD), "arrow on the release tick");
}

/// Railgun: charges while held and fires by itself when full (0.5 s) — no release, no second press.
#[test]
fn railgun_fires_at_full_charge_while_held() {
    let mut w = duel(12.0);
    give(&mut w, Form::Gun, &[Mechanic::Pierce]);
    let t = press_hold_release(&mut w, buttons::PRIMARY, HOLD + 12, 30);
    let shot = first_tick(&t, |e| matches!(e, GameEvent::ShotFired { owner, archetype: GunArchetype::Railgun, .. } if *owner == A));
    assert!(shot.is_some_and(|k| (HOLD - 2..=HOLD + 2).contains(&k)), "railgun at full charge: {shot:?}");
}

/// Cap's shield: a charged LMB release throws it (RMB blocks).
#[test]
fn returning_shield_throws_on_release() {
    let mut w = duel(8.0);
    give(&mut w, Form::Shield, &[Mechanic::Returning]);
    let t = press_hold_release(&mut w, buttons::PRIMARY, HOLD, 30);
    assert_eq!(fx(&t, HeldFx::Throw, 0.0), Some(HOLD), "thrown on the release tick");
}

/// Empty hand: the ring blast fires on the press (no charge, no second press).
#[test]
fn empty_hand_blasts_on_press() {
    let mut w = duel(12.0);
    let t = press_hold_release(&mut w, buttons::PRIMARY, HOLD, 10);
    let shot = first_tick(&t, |e| matches!(e, GameEvent::ShotFired { owner, construct: None, .. } if *owner == A));
    assert_eq!(shot, Some(0));
}
