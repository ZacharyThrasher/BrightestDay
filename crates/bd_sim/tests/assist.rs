//! Melee lunge (`bd_sim::melee_assist`) and hook-and-strike (`bd_sim::hook`): scenario tests on the
//! flat lab arena (ground top at y = 8). A looks down −Z at B unless noted.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::voxel::world_to_voxel;
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::GameEvent;
use bd_sim::hook::{AssistEvent, HookBreak};
use bd_sim::movement::{AnchorTarget, ColliderId, HookPhase};
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
    p.mv.lunge = Default::default();
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..ticks {
        ev.extend_from_slice(w.tick(&[]));
    }
    ev
}

fn give(w: &mut World, id: PlayerId, form: Form, m: &[Mechanic]) {
    let scale = bd_sim::constructs::form_spec(form).default_scale;
    let eye = w.player(id).unwrap().eye(w.params());
    w.cast(id, recipe("weapon", form, m, Element::Light, scale), None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    run(w, 30);
}

fn aim_at(w: &World, a: PlayerId, b: PlayerId) -> (f32, f32) {
    let eye = w.player(a).unwrap().eye(w.params());
    let d = (w.hull_of(b).unwrap().center() - eye).normalize();
    ((-d.x).atan2(-d.z), d.y.asin())
}

fn input(yaw: f32, pitch: f32, b: u16) -> PlayerInput {
    PlayerInput { yaw, pitch, buttons: b, ..Default::default() }
}

/// Hold `b` for `hold` ticks then release for `after` ticks (A's input only).
fn act(w: &mut World, id: PlayerId, yaw: f32, pitch: f32, b: u16, hold: u32, after: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..hold {
        ev.extend_from_slice(w.tick(&[(id, input(yaw, pitch, b))]));
    }
    for _ in 0..after {
        ev.extend_from_slice(w.tick(&[(id, input(yaw, pitch, 0))]));
    }
    ev
}

fn assists(ev: &[GameEvent], who: PlayerId) -> Vec<AssistEvent> {
    ev.iter()
        .filter_map(|e| match e {
            GameEvent::Assist { player, kind, .. } if *player == who => Some(*kind),
            _ => None,
        })
        .collect()
}

fn lunged(ev: &[GameEvent], who: PlayerId) -> bool {
    assists(ev, who).contains(&AssistEvent::Lunge)
}

fn hit_kb(ev: &[GameEvent], target: PlayerId) -> Option<f32> {
    ev.iter().find_map(|e| match e {
        GameEvent::Hit { target: t, knockback, .. } if *t == target => Some(*knockback),
        _ => None,
    })
}

/// A (sword) and B on the ground, B `dist` m in front of A.
fn sword_duel(dist: f32) -> World {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 6.0 - dist));
    run(&mut w, 20);
    give(&mut w, A, Form::Sword, &[]);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 6.0 - dist));
    run(&mut w, 10);
    w
}

// ------------------------------------------------------------------------------------ lunge

#[test]
fn lunge_dashes_in_range_and_connects() {
    let mut w = sword_duel(4.2);
    let z0 = w.player(A).unwrap().mv.pos.z;
    let (yaw, pitch) = aim_at(&w, A, B);
    let mut peak = 0.0f32;
    let mut ev = Vec::new();
    for k in 0..60 {
        ev.extend_from_slice(w.tick(&[(A, input(yaw, pitch, if k == 0 { buttons::PRIMARY } else { 0 }))]));
        peak = peak.max(w.player(A).unwrap().mv.vel.length());
    }
    assert!(lunged(&ev, A), "sword lunge at 4.2 m");
    assert!(peak > 15.0, "a committed dash, not a nudge: peak {peak} m/s");
    let moved = z0 - w.player(A).unwrap().mv.pos.z;
    assert!(moved > 1.5, "dashed toward B: {moved} m");
    assert!(hit_kb(&ev, B).is_some(), "the lunge swing connects");
}

#[test]
fn no_lunge_outside_range_cone_or_sight() {
    // Out of range.
    let mut w = sword_duel(8.5);
    let (yaw, pitch) = aim_at(&w, A, B);
    assert!(!lunged(&act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 40), A), "8.5 m is out of range");
    // Outside the cone (aim 40° off).
    let mut w = sword_duel(4.0);
    let (yaw, pitch) = aim_at(&w, A, B);
    assert!(!lunged(&act(&mut w, A, yaw + 40f32.to_radians(), pitch, buttons::PRIMARY, 1, 40), A), "40° off the aim");
    // Behind a wall.
    let mut w = sword_duel(4.2);
    let lo = world_to_voxel(Vec3::new(-3.0, GROUND, 3.8));
    let hi = world_to_voxel(Vec3::new(3.0, GROUND + 4.0, 4.1));
    w.voxels.fill_box(lo, hi, bd_voxel::mat::STONE);
    let (yaw, pitch) = aim_at(&w, A, B);
    assert!(!lunged(&act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 40), A), "never through walls");
    // Inside natural reach: no dash needed.
    let mut w = sword_duel(1.6);
    let (yaw, pitch) = aim_at(&w, A, B);
    assert!(!lunged(&act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 40), A), "already in reach");
}

#[test]
fn lunge_has_a_cooldown() {
    let t = World::new(SimConfig::default()).tuning().assist.lunge.cooldown;
    let mut w = sword_duel(4.2);
    // (A connecting lunge clears the cooldown — tests/connect.rs; here the plain cooldown.)
    let mut tt = w.tuning().clone();
    tt.assist.lunge.hit_resets_cooldown = false;
    w.set_tuning(tt);
    let (yaw, pitch) = aim_at(&w, A, B);
    let first = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 1);
    assert!(lunged(&first, A));
    // Let the swing finish, put B back at 4.2 m and swing again (still in the cooldown).
    run(&mut w, ((t * 120.0) as u32).saturating_sub(24));
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 1.8));
    let soon = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 3);
    assert!(soon.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: bd_sim::events::HeldFx::SwingStart, .. })), "second swing started");
    assert!(!lunged(&soon, A), "within {t} s: no second lunge");
    assert!(w.player(A).unwrap().assist.lunge_cd > 0.0, "still cooling down");
    // After the cooldown (and the swing) it is back.
    run(&mut w, (t * 120.0) as u32 + 30);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 1.8));
    run(&mut w, 5);
    let (yaw, pitch) = aim_at(&w, A, B);
    assert!(lunged(&act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 1), A), "ready again");
}

#[test]
fn air_lunge_works() {
    let mut w = sword_duel(4.2);
    place(&mut w, A, Vec3::new(0.0, GROUND + 6.0, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND + 6.0, 1.8));
    w.tick(&[]);
    let (yaw, pitch) = aim_at(&w, A, B);
    let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 40);
    assert!(lunged(&ev, A), "lunges in the air");
    assert!(hit_kb(&ev, B).is_some(), "and connects");
}

#[test]
fn lunge_never_stacks_on_a_faster_approach() {
    let mut w = sword_duel(5.0);
    let (yaw, pitch) = aim_at(&w, A, B);
    w.player_mut(A).unwrap().mv.vel = Vec3::new(0.0, 0.0, -30.0);
    w.player_mut(A).unwrap().mv.grounded = false;
    let mut peak = 0.0f32;
    let mut ev = Vec::new();
    for k in 0..12 {
        ev.extend_from_slice(w.tick(&[(A, input(yaw, pitch, if k == 0 { buttons::PRIMARY } else { 0 }))]));
        peak = peak.max(w.player(A).unwrap().mv.vel.length());
    }
    assert!(peak <= 30.0 + 0.5, "the lunge adds nothing on top of 30 m/s: {peak}");
}

#[test]
fn lunge_is_lag_compensated() {
    // B stood in front for a while, then (as far as the host is concerned) stepped 45° aside; A
    // swings while still seeing the old position (view tick): the lunge chases what A saw.
    let run_case = |view: bool| {
        let mut w = sword_duel(4.2);
        run(&mut w, 10);
        let seen = w.tick as f64 - 2.0;
        let (yaw, pitch) = aim_at(&w, A, B);
        place(&mut w, B, Vec3::new(4.0, GROUND, 1.8));
        w.tick(&[(A, input(yaw, pitch, 0))]);
        w.set_view_tick(A, view.then_some(seen));
        lunged(&act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 2), A)
    };
    assert!(run_case(true), "rewound to the view tick: B was in the cone");
    assert!(!run_case(false), "without rewind B is outside the cone");
}

#[test]
fn lunge_fight_is_deterministic() {
    let fight = || {
        let mut w = sword_duel(4.4);
        let mut log = String::new();
        for k in 0..240u32 {
            let (yaw, pitch) = aim_at(&w, A, B);
            let b = if k % 50 == 0 { buttons::PRIMARY } else { 0 };
            for e in w.tick(&[(A, input(yaw, pitch, b))]) {
                log.push_str(&format!("{e:?}"));
            }
        }
        (w.state_hash(), log)
    };
    assert_eq!(fight(), fight());
}

// ------------------------------------------------------------------------------------ hook

/// A and B on the ground `dist` apart; A fires the right tether at B and holds it (plus `extra`).
fn hook_setup(dist: f32, sword: bool) -> World {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 6.0 - dist));
    run(&mut w, 20);
    if sword {
        give(&mut w, A, Form::Sword, &[]);
        place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
        place(&mut w, B, Vec3::new(0.0, GROUND, 6.0 - dist));
        run(&mut w, 10);
    }
    w
}

/// Fire + hold A's right tether at B until it hooks (≤ 30 ticks). Returns the events.
fn hook_b(w: &mut World) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..30 {
        let (yaw, pitch) = aim_at(w, A, B);
        ev.extend_from_slice(w.tick(&[(A, input(yaw, pitch, buttons::TETHER_R))]));
        if w.player(A).unwrap().mv.hooks[1].attached() {
            break;
        }
    }
    ev
}

fn hooked_on_b(w: &World) -> bool {
    let h = w.player(A).unwrap().mv.hooks[1];
    h.phase == HookPhase::Attached && h.target == Some(AnchorTarget::Collider(ColliderId::Player(B)))
}

fn broke(ev: &[GameEvent]) -> Option<HookBreak> {
    assists(ev, A).into_iter().find_map(|k| match k {
        AssistEvent::HookBroken { why, .. } => Some(why),
        _ => None,
    })
}

#[test]
fn tether_hooks_a_player_and_reels_you_in() {
    let mut w = hook_setup(16.0, false);
    let ev = hook_b(&mut w);
    assert!(hooked_on_b(&w), "the tether hooked B");
    assert!(assists(&ev, A).iter().any(|k| matches!(k, AssistEvent::Hooked { hand: 1 })), "Hooked event");
    let b0 = w.player(B).unwrap().mv.pos;
    let d0 = (w.player(A).unwrap().mv.pos - b0).length();
    for _ in 0..60 {
        let (yaw, pitch) = aim_at(&w, A, B);
        w.tick(&[(A, input(yaw, pitch, buttons::TETHER_R | buttons::REEL))]);
    }
    let d1 = (w.player(A).unwrap().mv.pos - w.player(B).unwrap().mv.pos).length();
    assert!(d1 < d0 - 6.0, "reeled toward B: {d0} → {d1}");
    assert!((w.player(B).unwrap().mv.pos - b0).length() < 0.3, "the victim is never towed");
}

#[test]
fn rule_toggle_turns_player_hooks_off() {
    let mut w = hook_setup(16.0, false);
    w.cfg.rules.tether_hooks_players = false;
    hook_b(&mut w);
    run(&mut w, 5);
    assert!(!hooked_on_b(&w));
}

#[test]
fn victim_escapes_by_boost_or_own_tether() {
    for (btn, why) in [(buttons::BOOST, HookBreak::Dodged), (buttons::TETHER_L, HookBreak::Countered)] {
        let mut w = hook_setup(14.0, false);
        hook_b(&mut w);
        assert!(hooked_on_b(&w));
        let (yaw, pitch) = aim_at(&w, A, B);
        let (by, bp) = aim_at(&w, B, A);
        let ev = w.tick(&[(A, input(yaw, pitch, buttons::TETHER_R)), (B, input(by + 1.2, bp + 0.4, btn))]).to_vec();
        assert_eq!(broke(&ev), Some(why), "{why:?}");
        assert!(!hooked_on_b(&w));
    }
}

#[test]
fn rope_breaks_when_too_long_or_out_of_sight() {
    // Too far.
    let mut w = hook_setup(14.0, false);
    hook_b(&mut w);
    place(&mut w, B, Vec3::new(0.0, GROUND, -50.0));
    let mut ev = Vec::new();
    for _ in 0..4 {
        let (yaw, pitch) = aim_at(&w, A, B);
        ev.extend_from_slice(w.tick(&[(A, input(yaw, pitch, buttons::TETHER_R))]));
    }
    assert_eq!(broke(&ev), Some(HookBreak::TooFar));
    // Line of sight.
    let mut w = hook_setup(14.0, false);
    hook_b(&mut w);
    let lo = world_to_voxel(Vec3::new(-4.0, GROUND, -1.5));
    let hi = world_to_voxel(Vec3::new(4.0, GROUND + 6.0, -1.0));
    w.voxels.fill_box(lo, hi, bd_voxel::mat::STONE);
    let mut ev = Vec::new();
    for _ in 0..60 {
        let (yaw, pitch) = aim_at(&w, A, B);
        ev.extend_from_slice(w.tick(&[(A, input(yaw, pitch, buttons::TETHER_R))]));
    }
    assert_eq!(broke(&ev), Some(HookBreak::LostSight));
}

#[test]
fn victim_cuts_the_rope_with_a_swing() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, -8.0));
    run(&mut w, 20);
    give(&mut w, B, Form::Sword, &[]);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, -8.0));
    run(&mut w, 10);
    hook_b(&mut w);
    assert!(hooked_on_b(&w));
    let mut ev = Vec::new();
    for k in 0..40 {
        let (yaw, pitch) = aim_at(&w, A, B);
        let (by, bp) = aim_at(&w, B, A);
        ev.extend_from_slice(
            w.tick(&[(A, input(yaw, pitch, buttons::TETHER_R)), (B, input(by, bp, if k == 0 { buttons::PRIMARY } else { 0 }))]),
        );
    }
    assert_eq!(broke(&ev), Some(HookBreak::Cut));
}

/// Hook B from 12 m with a sword, reel in until the rope lets go (arrival), then swing once.
/// Returns (events, the swing's knockback on B).
fn hook_and_strike(slam_knockback: f32) -> (Vec<GameEvent>, Option<f32>) {
    let mut w = hook_setup(12.0, true);
    w.cfg.tuning.assist.hook.slam_knockback = slam_knockback;
    let mut ev = hook_b(&mut w);
    assert!(hooked_on_b(&w));
    for _ in 0..120 {
        let (yaw, pitch) = aim_at(&w, A, B);
        let e = w.tick(&[(A, input(yaw, pitch, buttons::TETHER_R | buttons::REEL))]).to_vec();
        let arrived = assists(&e, A).contains(&AssistEvent::SlamReady);
        ev.extend(e);
        if arrived {
            break;
        }
    }
    assert!(assists(&ev, A).contains(&AssistEvent::SlamReady), "arrived with a melee weapon: slam ready");
    assert!(!w.player(A).unwrap().mv.hooks[1].attached(), "the rope let go on arrival");
    let (yaw, pitch) = aim_at(&w, A, B);
    let swing = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 50);
    let kb = hit_kb(&swing, B);
    ev.extend(swing);
    // A second swing is a normal one (the slam is consumed).
    run(&mut w, 30);
    let (yaw, pitch) = aim_at(&w, A, B);
    ev.extend(act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 50));
    (ev, kb)
}

#[test]
fn hook_and_strike_slams_once() {
    let (ev, kb) = hook_and_strike(bd_sim::hook::HookTuning::default().slam_knockback);
    let slams = assists(&ev, A).iter().filter(|k| **k == AssistEvent::Slam).count();
    assert_eq!(slams, 1, "one slam, consumed");
    let (_, plain) = hook_and_strike(1.0);
    let (kb, plain) = (kb.expect("slam connected"), plain.expect("control connected"));
    assert!(kb > plain * 1.15, "slam knockback {kb} vs plain {plain}");
}

// ------------------------------------------------------------------------------------ prediction

struct Pair {
    host: World,
    client: World,
}

impl Pair {
    fn new(host: World) -> Self {
        let mut client = World::new(host.cfg.clone());
        for e in host.construct_manifest() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        Pair { host, client }
    }

    /// One tick: predict A from the replica, run the host, feed the client. Returns the error.
    fn step(&mut self, a: PlayerInput, b: Option<PlayerInput>) -> (f32, f32) {
        let mut s = self.client.player(A).unwrap().mv;
        self.client.predict_player(A, &mut s, &a);
        let mut inputs = vec![(A, a)];
        if let Some(b) = b {
            inputs.push((B, b));
        }
        let evs = self.host.tick(&inputs).to_vec();
        for e in &evs {
            self.client.apply_event(e);
        }
        self.client.apply_snapshot(&self.host.snapshot_for(Some(A)), Some(A));
        let h = self.host.player(A).unwrap().mv;
        ((h.pos - s.pos).length(), (h.vel - s.vel).length())
    }
}

#[test]
fn prediction_matches_the_host_for_hooks_and_lunges() {
    // Hook B and reel in.
    let mut p = Pair::new(hook_setup(16.0, false));
    let mut bad = 0;
    for _ in 0..90 {
        let (yaw, pitch) = aim_at(&p.host, A, B);
        let (dp, dv) = p.step(input(yaw, pitch, buttons::TETHER_R | buttons::REEL), None);
        if dp > 1e-4 || dv > 1e-3 {
            bad += 1;
        }
    }
    assert!(hooked_on_b(&p.host) || p.host.player(A).unwrap().mv.hooks[1].phase == HookPhase::Idle);
    assert_eq!(bad, 0, "hook + reel mispredicted ticks");
    // A lunge: the dash starts on the host after the constructs step and is replayed exactly.
    let mut p = Pair::new(sword_duel(4.4));
    let mut bad = 0;
    let mut lunge = false;
    for k in 0..60 {
        let (yaw, pitch) = aim_at(&p.host, A, B);
        let (dp, dv) = p.step(input(yaw, pitch, if k == 0 { buttons::PRIMARY } else { 0 }), None);
        lunge |= p.host.player(A).unwrap().mv.lunge.active();
        if dp > 1e-4 || dv > 1e-3 {
            bad += 1;
        }
    }
    assert!(lunge, "dashed");
    assert_eq!(bad, 0, "lunge dash mispredicted ticks");
}

// ------------------------------------------------------------------------------------ tuning

#[test]
fn tuning_files_match_the_rust_defaults() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/");
    let combat = std::fs::read_to_string(format!("{dir}combat.ron")).unwrap();
    let t = bd_sim::tuning::CombatTuning::from_ron(&combat).unwrap();
    assert_eq!(t.assist, bd_sim::melee_assist::AssistTuning::default(), "combat.ron `assist` = Rust defaults");
    let mv: bd_sim::movement::MoveParams = ron::from_str(&std::fs::read_to_string(format!("{dir}move.ron")).unwrap()).unwrap();
    assert_eq!(mv.hook_player_lift, bd_sim::movement::MoveParams::default().hook_player_lift);
}

// ------------------------------------------------------------------------------------ bots

#[test]
fn bots_hook_and_strike_now_and_then() {
    let mut w = world(0);
    let mut brains = bd_sim::bots::add_bots(&mut w, 1, 2, 0.8, 7);
    for b in &mut brains {
        b.casts_enabled = false;
    }
    w.start_live();
    w.tick(&[]);
    for id in [A, B] {
        w.player_mut(id).unwrap().spawn_shield = 0.0;
        give(&mut w, id, Form::Sword, &[]);
    }
    let mut inputs = Vec::new();
    let mut hooked = 0;
    let mut slam_ready = 0;
    for k in 0..(60 * 120) {
        if k % 360 == 0 {
            // Spread them out again (bots close in fast with swords).
            place(&mut w, A, Vec3::new(-10.0, GROUND, 0.0));
            place(&mut w, B, Vec3::new(10.0, GROUND, 0.0));
        }
        inputs.clear();
        bd_sim::bots::drive(&mut w, &mut brains, &mut inputs);
        for e in w.tick(&inputs) {
            if let GameEvent::Assist { kind, .. } = e {
                match kind {
                    AssistEvent::Hooked { .. } => hooked += 1,
                    AssistEvent::SlamReady => slam_ready += 1,
                    _ => {}
                }
            }
        }
    }
    assert!(hooked > 0, "bots hooked someone");
    eprintln!("bots: {hooked} hooks, {slam_ready} slams ready");
}
