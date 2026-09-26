//! Aim down sights (CAMERA; `buttons::ADS`): accuracy only. The pellet cone tightens
//! deterministically, movement never reads the bit (host / client prediction parity), and the same
//! inputs give the same world on every run.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::constructs::gun::ADS_SPREAD;
use bd_sim::events::GameEvent;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn cfg() -> SimConfig {
    SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() }
}

fn world() -> World {
    let mut w = World::new(cfg());
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    // B far away (out of the pellets' way).
    w.player_mut(B).unwrap().mv.pos = Vec3::new(-40.0, 8.0, 40.0);
    w
}

fn shotgun(w: &mut World) {
    let eye = w.player(A).unwrap().eye(w.params());
    let mut r = recipe("shotgun", Form::Gun, &[Mechanic::Shoots], Element::Light, 0.6);
    r.count = 3;
    w.cast(A, r, None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    for _ in 0..30 {
        w.tick(&[]);
    }
}

/// One super-shotgun blast aimed at the floor; returns the pellets' angles (rad) off the aim.
fn blast(ads: bool) -> (Vec<f32>, Vec<GameEvent>) {
    let mut w = world();
    shotgun(&mut w);
    let (yaw, pitch) = (0.0f32, -0.35f32);
    let b = buttons::PRIMARY | if ads { buttons::ADS } else { 0 };
    let eye = w.player(A).unwrap().eye(w.params());
    let ev = w.tick(&[(A, PlayerInput { yaw, pitch, buttons: b, ..Default::default() })]).to_vec();
    let aim = bd_core::input::look_dir(yaw, pitch);
    let angles: Vec<f32> = ev
        .iter()
        .filter_map(|e| match e {
            GameEvent::ShotTrace { owner, to, .. } if *owner == A => Some((*to - eye).normalize().dot(aim).clamp(-1.0, 1.0).acos()),
            _ => None,
        })
        .collect();
    (angles, ev)
}

#[test]
fn ads_tightens_the_pellet_cone() {
    let (hip, _) = blast(false);
    let (ads, _) = blast(true);
    assert_eq!(hip.len(), 9, "nine pellet traces");
    assert_eq!(ads.len(), 9);
    let max = |v: &[f32]| v.iter().copied().fold(0.0f32, f32::max);
    let (h, a) = (max(&hip), max(&ads));
    assert!(h > 3.0f32.to_radians(), "hip cone ≈ 4°: {}", h.to_degrees());
    assert!((a / h - ADS_SPREAD).abs() < 0.05, "ADS cone = {ADS_SPREAD} × hip: {} vs {}", a.to_degrees(), h.to_degrees());
}

#[test]
fn ads_spread_is_deterministic() {
    let (a1, e1) = blast(true);
    let (a2, e2) = blast(true);
    assert_eq!(a1, a2);
    assert_eq!(format!("{e1:?}"), format!("{e2:?}"));
}

/// Movement never reads ADS: the host with ADS held and a client predicting without it (or with it)
/// agree bit for bit over a strafe-jump run.
#[test]
fn ads_never_changes_movement_host_prediction_parity() {
    let run = |ads: bool| {
        let mut w = world();
        shotgun(&mut w);
        let p = w.player_mut(A).unwrap();
        p.mv.pos = Vec3::new(10.0, 1.0, 10.0);
        p.mv.vel = Vec3::new(9.0, 0.0, 0.0);
        let mut host_states = Vec::new();
        let mut predicted = w.player(A).unwrap().mv;
        let mut pred_states = Vec::new();
        for k in 0..240u32 {
            let yaw = (k as f32 * 0.02).sin() * 0.6;
            let strafe = if (k / 30) % 2 == 0 { 1 } else { -1 };
            let base = buttons::JUMP;
            let inp = PlayerInput {
                seq: k,
                yaw,
                pitch: 0.0,
                move_x: strafe,
                move_z: 1,
                buttons: base | if ads { buttons::ADS } else { 0 },
                ..Default::default()
            };
            // The client predicts with the same input it sends.
            w.predict_player(A, &mut predicted, &inp);
            pred_states.push(predicted);
            w.tick(&[(A, inp)]);
            predicted = w.player(A).unwrap().mv;
            host_states.push(predicted);
        }
        (host_states, pred_states)
    };
    let (host_hip, pred_hip) = run(false);
    let (host_ads, pred_ads) = run(true);
    assert_eq!(host_hip.len(), host_ads.len());
    for k in 0..host_hip.len() {
        assert_eq!(host_hip[k].pos, host_ads[k].pos, "tick {k}: ADS moved the host player");
        assert_eq!(host_hip[k].vel, host_ads[k].vel, "tick {k}");
        assert_eq!(pred_hip[k].pos, pred_ads[k].pos, "tick {k}: ADS changed the prediction");
        assert_eq!(pred_hip[k].vel, pred_ads[k].vel, "tick {k}");
        // Whatever the prediction error is, ADS adds none.
        let e_hip = (pred_hip[k].pos - host_hip[k].pos).length();
        let e_ads = (pred_ads[k].pos - host_ads[k].pos).length();
        assert_eq!(e_hip, e_ads, "tick {k}");
        assert!(e_ads < 1e-3, "tick {k}: prediction off by {e_ads} m");
    }
}
