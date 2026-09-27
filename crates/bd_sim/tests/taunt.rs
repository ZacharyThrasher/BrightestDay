//! TAUNT & VOICE TELL (`bd_sim::taunt`): the wordless taunt and the voice-cast tell are host state
//! that replicates (event + `PlayerSnap::tell`), is rate-limited, ends on any input, and has no
//! gameplay effect (same inputs minus the taunt / voice bits ⇒ the same state hash).

use bd_core::input::{PlayerInput, buttons};
use bd_core::{DT, PlayerId};
use bd_sim::events::move_flags;
use bd_sim::replicate::{PlayerSnap, WorldSnapshot};
use bd_sim::taunt::{self, BUCKET, MIN_GAP, REFILL_SECS, TauntKind, tell};
use bd_sim::world::{ArenaSource, SimConfig, World};
use bd_sim::{GameEvent, SnapshotDelta};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn secs(s: f32) -> usize {
    (s / DT).round() as usize
}

/// Flat lab, two players standing, match live.
fn world() -> World {
    let mut w = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 64.0 }, seed: 7, check_invariants: true, ..SimConfig::default() });
    w.add_player(A, "a".into(), bd_core::corps::Corps::Willpower).unwrap();
    w.add_player(B, "b".into(), bd_core::corps::Corps::Rage).unwrap();
    w.start_live();
    for _ in 0..secs(1.5) {
        w.tick(&[]);
    }
    assert!(w.player(A).unwrap().alive && w.player(A).unwrap().mv.grounded);
    w
}

fn input(b: u16) -> PlayerInput {
    PlayerInput { buttons: b, ..Default::default() }
}

/// Tick with A's input; returns the taunts A started (from the replicated `Move` events).
fn tick(w: &mut World, a: PlayerInput, n: usize) -> Vec<TauntKind> {
    let mut out = Vec::new();
    for _ in 0..n {
        for e in w.tick(&[(A, a)]) {
            if let GameEvent::Move { player, flags, .. } = e
                && *player == A
                && flags & move_flags::TAUNT != 0
            {
                out.push(move_flags::taunt_kind(*flags));
            }
        }
    }
    out
}

fn snap_of(w: &World, id: PlayerId) -> PlayerSnap {
    *w.snapshot_for(Some(B)).players.iter().find(|p| p.id == id.0).unwrap()
}

/// A tap (key down a few ticks, then up).
fn tap(w: &mut World) -> Vec<TauntKind> {
    let mut s = tick(w, input(buttons::TAUNT), 3);
    s.extend(tick(w, input(0), 1));
    s
}

#[test]
fn replication_round_trip() {
    let mut w = world();
    let base = w.snapshot_for(Some(B));
    assert_eq!(snap_of(&w, A).tell, 0);
    assert_eq!(tap(&mut w), vec![TauntKind::Flex], "a tap starts the first taunt of the rotation, as an event");
    let now = w.snapshot_for(Some(B));
    let a = now.players.iter().find(|p| p.id == A.0).unwrap();
    assert_eq!(TauntKind::from_bits(a.tell & tell::KIND), TauntKind::Flex, "…and in the snapshot");
    // The wire: a delta against the old baseline, bincode (bd_net's codec is bincode standard), applied.
    let delta = SnapshotDelta::encode(Some(&base), &now);
    let cfg = bincode::config::standard();
    let bytes = bincode::serde::encode_to_vec(&delta, cfg).unwrap();
    let (back, _): (SnapshotDelta, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    let got: WorldSnapshot = back.apply(Some(&base)).unwrap();
    assert_eq!(got, now);
    let ga = got.players.iter().find(|p| p.id == A.0).unwrap();
    assert_eq!(TauntKind::from_bits(ga.tell & tell::KIND), TauntKind::Flex);
    // The event on the wire too.
    let ev = GameEvent::Move { player: A, flags: move_flags::taunt(TauntKind::Bow), speed: 0.0 };
    let bytes = bincode::serde::encode_to_vec(&ev, cfg).unwrap();
    let (back, _): (GameEvent, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    let GameEvent::Move { flags, .. } = back else { panic!() };
    assert_eq!(move_flags::taunt_kind(flags), TauntKind::Bow);
    // Every other movement moment decodes as no taunt.
    for f in [move_flags::JUMP, move_flags::DODGE, move_flags::WALL_REFRESH, move_flags::AIR_JUMP | move_flags::LAND_HARD] {
        assert_eq!(move_flags::taunt_kind(f), TauntKind::None);
    }
    // It ends by itself; the snapshot clears.
    tick(&mut w, input(0), secs(TauntKind::Flex.duration() + 0.1));
    assert_eq!(snap_of(&w, A).tell & tell::KIND, 0);
}

#[test]
fn voice_tell_replicates_and_clears() {
    let mut w = world();
    tick(&mut w, input(buttons::VOICE), 2);
    let s = snap_of(&w, A);
    assert_ne!(s.tell & tell::VOICE, 0, "speaking a cast shows");
    assert_eq!(snap_of(&w, B).tell, 0);
    // Voice while running (voice casting never stops you; the tell stays).
    tick(&mut w, PlayerInput { move_z: 1, buttons: buttons::VOICE, ..Default::default() }, 10);
    assert_ne!(snap_of(&w, A).tell & tell::VOICE, 0);
    tick(&mut w, input(0), 1);
    assert_eq!(snap_of(&w, A).tell & tell::VOICE, 0, "let go: gone");
    // Wire.
    let snap = w.snapshot_for(Some(B));
    let mut s2 = snap.clone();
    s2.players.iter_mut().for_each(|p| p.tell = tell::VOICE | TauntKind::Dance.bits());
    let d = SnapshotDelta::encode(Some(&snap), &s2);
    let bytes = bincode::serde::encode_to_vec(&d, bincode::config::standard()).unwrap();
    let (back, _): (SnapshotDelta, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(back.apply(Some(&snap)).unwrap(), s2);
}

#[test]
fn rate_limited() {
    let mut w = world();
    // Mash the key (a tap every 6 ticks) for 12 s.
    let mut starts: Vec<(usize, TauntKind)> = Vec::new();
    let total = secs(12.0);
    let mut t = 0;
    while t < total {
        for k in tap(&mut w) {
            starts.push((t, k));
        }
        tick(&mut w, input(0), 2);
        t += 6;
    }
    let max = BUCKET + 12.0 / REFILL_SECS;
    assert!(starts.len() as f32 <= max + 0.01, "{} taunts in 12 s (max {max})", starts.len());
    assert!(starts.len() >= 4, "mashing still taunts now and then ({})", starts.len());
    for pair in starts.windows(2) {
        assert!((pair[1].0 - pair[0].0) as f32 * DT >= MIN_GAP - 1e-3, "starts {:?} too close", pair);
    }
    // The rotation advanced only on real starts.
    let kinds: Vec<TauntKind> = starts.iter().map(|s| s.1).collect();
    assert_eq!(&kinds[..4], &TauntKind::TAPS);
}

#[test]
fn any_input_cancels_instantly_mouse_look_does_not() {
    let cancels: [(&str, PlayerInput); 6] = [
        ("move", PlayerInput { move_x: 1, ..Default::default() }),
        ("forward", PlayerInput { move_z: -1, ..Default::default() }),
        ("jump", input(buttons::JUMP)),
        ("primary", input(buttons::PRIMARY)),
        ("typing", input(buttons::TYPING)),
        ("voice", input(buttons::VOICE)),
    ];
    for (what, i) in cancels {
        let mut w = world();
        assert_eq!(tap(&mut w), vec![TauntKind::Flex]);
        tick(&mut w, input(0), 5);
        assert_eq!(w.player(A).unwrap().taunt.kind, TauntKind::Flex);
        tick(&mut w, i, 1);
        assert_eq!(w.player(A).unwrap().taunt.kind, TauntKind::None, "{what} cancels on the same tick");
        assert_eq!(snap_of(&w, A).tell & tell::KIND, 0, "{what}: the snapshot says so");
    }
    // Looking around (yaw / pitch only) keeps it going.
    let mut w = world();
    tap(&mut w);
    for k in 0..30 {
        tick(&mut w, PlayerInput { yaw: k as f32 * 0.05, pitch: 0.2, ..Default::default() }, 1);
    }
    assert_eq!(w.player(A).unwrap().taunt.kind, TauntKind::Flex);
    // The wheel too cancels.
    tick(&mut w, PlayerInput { reel_notches: 1, ..Default::default() }, 1);
    assert_eq!(w.player(A).unwrap().taunt.kind, TauntKind::None);
}

#[test]
fn hold_dances_until_released_and_never_in_the_air() {
    let mut w = world();
    let started = tick(&mut w, input(buttons::TAUNT), secs(1.0));
    assert_eq!(started, vec![TauntKind::Dance]);
    assert_eq!(TauntKind::from_bits(snap_of(&w, A).tell), TauntKind::Dance);
    tick(&mut w, input(0), 1);
    assert_eq!(w.player(A).unwrap().taunt.kind, TauntKind::None);
    // Airborne: refused (a jump also cancels anything playing).
    tick(&mut w, input(0), secs(2.0));
    tick(&mut w, input(buttons::JUMP), 1);
    tick(&mut w, input(0), 3);
    assert!(!w.player(A).unwrap().mv.grounded);
    assert!(tap(&mut w).is_empty(), "no taunts mid-air");
}

/// No gameplay effect: the same match with and without taunt / voice bits hashes identically
/// (the FFA golden hash lives in tests/party.rs and never sees these bits).
#[test]
fn no_gameplay_effect() {
    let run = |tell: bool| {
        let mut w = world();
        for k in 0..secs(8.0) {
            let mut b = 0;
            if tell && (k / 40) % 3 == 0 {
                b |= buttons::TAUNT;
            }
            if tell && (k / 90) % 4 == 1 {
                b |= buttons::VOICE;
            }
            // B moves around; A stands, looks, and sometimes steps.
            let a = PlayerInput { yaw: k as f32 * 0.01, move_x: if k % 400 > 350 { 1 } else { 0 }, buttons: b, ..Default::default() };
            let bi = PlayerInput { move_z: -1, yaw: (k as f32 * 0.02).sin(), buttons: if k % 97 == 0 { buttons::JUMP } else { 0 }, ..Default::default() };
            w.tick(&[(A, a), (B, bi)]);
        }
        w.state_hash()
    };
    assert_eq!(run(false), run(true));
}

#[test]
fn ko_ends_a_taunt() {
    let mut w = world();
    tap(&mut w);
    assert!(w.player(A).unwrap().taunt.kind.is_some());
    let p = w.player_mut(A).unwrap();
    p.alive = false;
    p.respawn_timer = 5.0;
    tick(&mut w, input(0), 1);
    assert_eq!(w.player(A).unwrap().taunt.kind, TauntKind::None);
    assert_eq!(taunt::tell_byte(false, TauntKind::Bow, true), 0);
}
