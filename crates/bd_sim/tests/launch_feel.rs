//! LAUNCH FEEL (COMBAT.md §1a "Launch feel"): the modern launch curve (small hits barely move you at
//! low %, strong hits at high % still send you far), a short hitstun after which a launched player
//! can act mid-flight (air jump / air dodge / tether / cast, still carrying the launch), DI bounds
//! and prediction parity through the actionable tail, the out-of-control telemetry, and the
//! CLASSIC LAUNCH host rule that restores the old behaviour.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::{PlayerId, Vec3};
use bd_sim::movement::PlayerState;
use bd_sim::replicate::OwnState;
use bd_sim::smash::{SmashTuning, base_scale, hitstun_secs, knockback, launch_knockback, tumble_secs};
use bd_sim::telemetry::Telemetry;
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn cfg(classic: bool) -> SimConfig {
    let mut c = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    c.rules.classic_launch = classic;
    c
}

/// Two players, live; A floating high above the flat arena, B far away.
fn world(classic: bool) -> World {
    let mut w = World::new(cfg(classic));
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    w.player_mut(B).unwrap().mv.pos = Vec3::new(-40.0, 8.0, 40.0);
    let p = w.player_mut(A).unwrap();
    p.mv.pos = Vec3::new(0.0, 60.0, 0.0);
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
    w.tick(&[]);
    w
}

fn press(b: u16) -> PlayerInput {
    PlayerInput { buttons: b, ..Default::default() }
}

// ------------------------------------------------------------------------------------ the curve

#[test]
fn curve_is_monotonic_in_percent_damage_and_strength() {
    let t = SmashTuning::default();
    for &dmg in &[0.0f32, 4.0, 9.0, 14.0, 30.0] {
        for base in 0..=40 {
            let base = base as f32;
            let mut prev = -1.0;
            for pct in (0..=400).step_by(5) {
                let k = launch_knockback(&t, false, base, dmg, pct as f32, 1.0, 1.0);
                assert!(k >= prev - 1e-4, "kb must not drop with %: base {base} dmg {dmg} pct {pct}: {prev} → {k}");
                prev = k;
                // Stronger move (base) and more damage never launch less.
                assert!(launch_knockback(&t, false, base + 1.0, dmg, pct as f32, 1.0, 1.0) >= k - 1e-4);
                assert!(launch_knockback(&t, false, base, dmg + 1.0, pct as f32, 1.0, 1.0) >= k - 1e-4);
                let b = base_scale(&t, base, pct as f32);
                assert!((t.curve_base_low..=1.0).contains(&b));
            }
        }
    }
}

#[test]
fn early_life_is_grounded_and_high_percent_still_launches() {
    let t = SmashTuning::default();
    // Classic = the old formula exactly.
    for &(b, d, p) in &[(8.0, 9.0, 0.0), (16.0, 14.0, 120.0), (30.0, 20.0, 300.0)] {
        assert_eq!(launch_knockback(&t, true, b, d, p, 1.0, 1.0), knockback(&t, b, d, p, 1.0, 1.0));
    }
    // A sword poke (8 kb, 9 %) at 0 %: under 60 % of its old shove (base term ×0.45), no tumble (14 m/s).
    let (old, new) = (knockback(&t, 8.0, 9.0, 9.0, 1.0, 1.0), launch_knockback(&t, false, 8.0, 9.0, 9.0, 1.0, 1.0));
    assert!(new < 0.6 * old && new < 14.0, "small early hit: {old} → {new}");
    // A hammer (16 kb, 14 %) at 14 %: a strong hit keeps most of its shove, but never launches early.
    let hammer0 = launch_knockback(&t, false, 16.0, 14.0, 14.0, 1.0, 1.0);
    assert!(hammer0 < t.kb_min_launch && hammer0 < 0.9 * knockback(&t, 16.0, 14.0, 14.0, 1.0, 1.0), "early hammer: {hammer0}");
    // At KO % a strong hit launches at least as hard as before (KOs keep happening).
    for pct in [120.0, 150.0, 200.0] {
        let (old, new) = (knockback(&t, 16.0, 14.0, pct, 1.0, 1.0), launch_knockback(&t, false, 16.0, 14.0, pct, 1.0, 1.0));
        assert!(new >= old, "hammer at {pct}%: {old} → {new}");
        assert!(new >= t.kb_min_launch, "hammer at {pct}% must launch: {new}");
    }
    // …while a weak poke at the same % launches far less than the hammer.
    assert!(launch_knockback(&t, false, 4.0, 3.0, 150.0, 1.0, 1.0) < 0.6 * launch_knockback(&t, false, 16.0, 14.0, 150.0, 1.0, 1.0));
}

#[test]
fn hitstun_is_short_capped_and_inside_the_tumble() {
    let t = SmashTuning::default();
    for kb in (14..=90).map(|k| k as f32) {
        let (h, tu) = (hitstun_secs(&t, false, kb), tumble_secs(&t, kb));
        assert!(h <= tu + 1e-6 && h <= t.hitstun_secs_max + 1e-6 && h >= t.hitstun_secs_min.min(tu) - 1e-6, "kb {kb}: {h} / {tu}");
        assert_eq!(hitstun_secs(&t, true, kb), tu, "classic: the whole tumble is hitstun");
    }
    // Scaled by hit strength (until the cap).
    assert!(hitstun_secs(&t, false, 50.0) > hitstun_secs(&t, false, 25.0));
    assert!(hitstun_secs(&t, false, 60.0) < 0.6 * tumble_secs(&t, 60.0), "KO launches are mostly actionable flight");
}

// ------------------------------------------------------------------------- actionable mid-flight

/// Launch A, then press `button` at `at_secs` (released the tick before). Returns (hitstun left,
/// tumble left, air jump used, air dodge used, horizontal speed before → after the press).
fn press_during_launch(classic: bool, button: u16, at_secs: f32) -> (f32, f32, bool, bool, f32, f32) {
    let mut w = world(classic);
    w.launch_player(A, Vec3::new(30.0, 20.0, 0.0), Some(B));
    let n = (at_secs / bd_core::DT).round() as u32;
    for _ in 0..n {
        w.tick(&[(A, press(0))]);
    }
    let mv = w.player(A).unwrap().mv;
    let before = Vec3::new(mv.vel.x, 0.0, mv.vel.z).length();
    let (stun, tumble) = (mv.hitstun_left(), mv.tumble);
    w.tick(&[(A, press(button))]);
    let mv = w.player(A).unwrap().mv;
    (stun, tumble, mv.extra.air_jump_used, mv.extra.air_dodge_used, before, Vec3::new(mv.vel.x, 0.0, mv.vel.z).length())
}

#[test]
fn actionable_after_hitstun_but_not_during() {
    let t = SmashTuning::default();
    let kb = Vec3::new(30.0, 20.0, 0.0).length();
    let stun = hitstun_secs(&t, false, kb);
    // During hitstun: no air jump, no air dodge.
    let (left, _, jumped, _, _, _) = press_during_launch(false, buttons::JUMP, 0.5 * stun);
    assert!(left > 0.0 && !jumped, "air jump refused in hitstun");
    let (_, _, _, dodged, _, _) = press_during_launch(false, buttons::DODGE, 0.5 * stun);
    assert!(!dodged, "air dodge refused in hitstun");
    // After it, still tumbling (carrying the launch): both work, and the launch is carried.
    let at = stun + 0.1;
    let (left, tumble, jumped, _, before, after) = press_during_launch(false, buttons::JUMP, at);
    assert!(left <= 0.0 && tumble > 0.0, "out of hitstun, still tumbling: {left} / {tumble}");
    assert!(jumped, "air jump out of the tumble after hitstun");
    assert!(after >= 0.95 * before, "the launch is carried through the air jump: {before} → {after}");
    let (_, _, _, dodged, _, _) = press_during_launch(false, buttons::DODGE, at);
    assert!(dodged, "air dodge out of the tumble after hitstun");
    // CLASSIC LAUNCH: the whole tumble is hitstun.
    let (left, tumble, jumped, _, _, _) = press_during_launch(true, buttons::JUMP, at);
    assert!(left > 0.0 && tumble > 0.0 && !jumped, "classic: no air jump while tumbling");
    let (_, _, _, dodged, _, _) = press_during_launch(true, buttons::DODGE, at);
    assert!(!dodged, "classic: no air dodge while tumbling");
}

#[test]
fn a_second_hit_never_shortens_running_hitstun() {
    let mut s = PlayerState::new(Vec3::ZERO, &Default::default());
    s.apply_impulse(Vec3::new(0.0, 5.0, 30.0), 1.5);
    s.limit_hitstun(0.0, 0.6);
    assert!((s.hitstun_left() - 0.6).abs() < 1e-5 && s.in_hitstun());
    // A weaker follow-up (0.2 s of hitstun) keeps the 0.6 s running.
    let before = s.hitstun_left();
    s.apply_impulse(Vec3::new(0.0, 1.0, 5.0), 0.8);
    s.limit_hitstun(before, 0.2);
    assert!((s.hitstun_left() - 0.6).abs() < 1e-5);
    // Tumble over → no hitstun, tail cleared by the controller.
    s.tumble = 0.0;
    assert!(!s.in_hitstun());
}

// ------------------------------------------------------------------------ DI + prediction parity

#[test]
fn di_is_bounded_and_prediction_matches_through_the_actionable_tail() {
    let mut host = world(false);
    let mut client = World::new(cfg(false));
    for e in host.construct_manifest() {
        client.apply_event(&e);
    }
    host.launch_player(A, Vec3::new(0.0, 15.0, -40.0), Some(B));
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let t = host.tuning().smash.clone();
    let stun_ticks = (hitstun_secs(&t, false, Vec3::new(0.0, 15.0, -40.0).length()) / bd_core::DT) as u32;
    let mut bad = 0;
    let mut jumped = false;
    for k in 0..90u32 {
        // Hold right (DI), then tap Jump and Dodge once out of hitstun.
        let b = if k == stun_ticks + 6 { buttons::JUMP } else if k == stun_ticks + 20 { buttons::DODGE } else { 0 };
        let inp = PlayerInput { move_x: 1, buttons: b, ..Default::default() };
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &inp);
        let evs = host.tick(&[(A, inp)]).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let h = host.player(A).unwrap().mv;
        jumped |= h.extra.air_jump_used;
        if (h.pos - s.pos).length() > 1e-4 || (h.vel - s.vel).length() > 1e-3 || h.tumble_free != s.tumble_free {
            bad += 1;
            eprintln!("tick {k}: prediction off by {} m / {} m/s", (h.pos - s.pos).length(), (h.vel - s.vel).length());
        }
    }
    assert!(jumped, "the air jump fired after hitstun");
    assert_eq!(bad, 0, "prediction diverged");
    if let Some(l) = host.player(A).unwrap().launch {
        assert!(l.di <= t.di_max_deg.to_radians() + 1e-4, "DI bounded: {}", l.di.to_degrees());
    }
    assert!(host.player(A).unwrap().mv.di_left < 1e-3, "DI spent");
}

#[test]
fn own_state_round_trips_the_actionable_tail() {
    let mut s = PlayerState::new(Vec3::new(1.0, 2.0, 3.0), &Default::default());
    s.tumble = 1.2;
    s.tumble_free = 0.7;
    assert_eq!(OwnState::from_state(&s, 1.0, 0.0).to_state(), s);
}

// ------------------------------------------------------------------------------------ telemetry

fn flight_telemetry(classic: bool) -> (f64, f64, f64, f64) {
    let mut w = world(classic);
    let mut tel = Telemetry::new(1);
    w.launch_player(A, Vec3::new(20.0, 25.0, 0.0), Some(B));
    for _ in 0..240 {
        w.tick(&[]);
        tel.observe(&w);
    }
    let c = tel.finish(&w).control;
    (c.alive, c.hitstun, c.tumble, c.launched_air)
}

#[test]
fn telemetry_counts_hitstun_tumble_and_flight() {
    let t = SmashTuning::default();
    let kb = Vec3::new(20.0, 25.0, 0.0).length();
    let (alive, hitstun, tumble, air) = flight_telemetry(false);
    let dt = f64::from(bd_core::DT);
    assert!((alive - 2.0 * 240.0 * dt).abs() < 2.0 * dt, "both players alive for 4 s: {alive}");
    assert!((hitstun - f64::from(hitstun_secs(&t, false, kb))).abs() < 2.5 * dt, "hitstun {hitstun}");
    assert!(tumble > hitstun + 0.2, "tumble {tumble} > hitstun {hitstun}");
    assert!(air > 0.0 && air <= tumble + dt, "launched in the air {air} (≤ the tumble)");
    let (_, c_hitstun, c_tumble, _) = flight_telemetry(true);
    assert!((c_hitstun - c_tumble).abs() < 1e-9, "classic: the whole tumble is hitstun");
    assert!(c_hitstun > hitstun + 0.2, "modern cuts time out of control: {c_hitstun} → {hitstun}");
}
