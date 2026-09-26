//! Client prediction (`World::predict_player` on a snapshot-fed replica) must step the local player
//! exactly like the host — including held / worn constructs that move their wielder (wings, jetpack,
//! minigun hover, cape dash, parachute) and Smash DI.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn cfg() -> SimConfig {
    SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() }
}

struct Pair {
    host: World,
    client: World,
}

impl Pair {
    /// Host with A holding / wearing `items`, and a client replica fed the manifest + a snapshot.
    fn new(items: &[Recipe], at: Vec3) -> Self {
        let mut host = World::new(cfg());
        host.cfg.rules.willpower = WillpowerMode::Infinite;
        for k in 1..=2 {
            host.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
        }
        host.start_live();
        host.tick(&[]);
        let far = Vec3::new(-40.0, 8.0, 40.0);
        host.player_mut(B).unwrap().mv.pos = far;
        for r in items {
            let eye = host.player(A).unwrap().eye(host.params());
            host.cast(A, r.clone(), None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
            for _ in 0..30 {
                host.tick(&[]);
            }
        }
        let p = host.player_mut(A).unwrap();
        p.mv.pos = at;
        p.mv.vel = Vec3::ZERO;
        p.mv.grounded = false;
        host.tick(&[]);
        let mut client = World::new(cfg());
        for e in host.construct_manifest() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        Pair { host, client }
    }

    /// One tick: predict from the replica, run the host, feed the client. Returns the error (m, m/s).
    fn step(&mut self, inp: PlayerInput) -> (f32, f32) {
        let mut s = self.client.player(A).unwrap().mv;
        self.client.predict_player(A, &mut s, &inp);
        let evs = self.host.tick(&[(A, inp)]).to_vec();
        for e in &evs {
            self.client.apply_event(e);
        }
        self.client.apply_snapshot(&self.host.snapshot_for(Some(A)), Some(A));
        let h = self.host.player(A).unwrap().mv;
        ((h.pos - s.pos).length(), (h.vel - s.vel).length())
    }

    fn run(&mut self, n: u32, mut inp: impl FnMut(u32) -> PlayerInput) -> u32 {
        let mut bad = 0;
        for k in 0..n {
            let (dp, dv) = self.step(inp(k));
            if dp > 1e-4 || dv > 1e-3 {
                bad += 1;
                if bad < 4 {
                    eprintln!("tick {k}: prediction off by {dp} m, {dv} m/s");
                }
            }
        }
        bad
    }
}

fn item(name: &str, form: Form, m: &[Mechanic]) -> Recipe {
    recipe(name, form, m, Element::Light, bd_sim::constructs::form_spec(form).default_scale)
}

#[test]
fn predicts_wing_flaps_and_glide() {
    let mut p = Pair::new(&[item("wings", Form::Wings, &[])], Vec3::new(0.0, 60.0, 0.0));
    let mut flaps = 0;
    let bad = p.run(300, |k| {
        // Tap Jump every 25 ticks (flaps), then hold it (glide) with forward.
        let tap = k < 150 && k % 25 < 3;
        let hold = k >= 150;
        PlayerInput { move_z: 1, buttons: if tap || hold { buttons::JUMP } else { 0 }, ..Default::default() }
    });
    flaps += p.host.player(A).unwrap().mv.air_flaps;
    assert!(flaps > 0, "flapped");
    assert_eq!(bad, 0, "mispredicted ticks");
}

#[test]
fn predicts_jetpack_thrust() {
    let mut p = Pair::new(&[item("jetpack", Form::Jetpack, &[])], Vec3::new(0.0, 8.0, 0.0));
    let bad = p.run(240, |k| PlayerInput { pitch: 0.6, move_z: 1, buttons: if k % 80 < 60 { buttons::JUMP } else { 0 }, ..Default::default() });
    assert!(p.host.player(A).unwrap().mv.pos.y > 12.0, "flew");
    assert_eq!(bad, 0);
}

#[test]
fn predicts_minigun_hover() {
    let mut r = recipe("minigun", Form::Gun, &[Mechanic::Spin], Element::Light, 0.6);
    r.count = 1;
    let mut p = Pair::new(&[r], Vec3::new(0.0, 90.0, 0.0));
    // The hover (slow-fall modifier) is predicted exactly; per-shot recoil (applied by the gun's
    // fire cadence after movement, host-side) is a small velocity nudge the next snapshot carries.
    let inp = PlayerInput { pitch: -1.5, buttons: buttons::PRIMARY, ..Default::default() };
    for k in 0..150 {
        let (dp, dv) = p.step(inp);
        assert!(dp < 1e-4 && dv < 0.1, "tick {k}: {dp} m, {dv} m/s");
    }
    assert!(-p.host.player(A).unwrap().mv.vel.y <= bd_sim::constructs::gun::MINIGUN_SINK + 0.3, "hovering");
}

#[test]
fn predicts_cape_dash_and_parachute() {
    let mut p = Pair::new(&[item("cape", Form::Cape, &[])], Vec3::new(0.0, 40.0, 0.0));
    let bad = p.run(120, |k| PlayerInput { buttons: if (20..23).contains(&k) || k > 60 { buttons::JUMP } else { 0 }, ..Default::default() });
    assert!(p.host.player(A).unwrap().mv.air_dashed);
    assert_eq!(bad, 0);
    let mut p = Pair::new(&[item("parachute", Form::Parachute, &[])], Vec3::new(0.0, 60.0, 0.0));
    assert_eq!(p.run(200, |_| PlayerInput { buttons: buttons::JUMP, ..Default::default() }), 0);
}

#[test]
fn predicts_smash_di() {
    let mut p = Pair::new(&[], Vec3::new(0.0, 20.0, 0.0));
    p.host.launch_player(A, Vec3::new(0.0, 10.0, -40.0), Some(B));
    p.client.apply_snapshot(&p.host.snapshot_for(Some(A)), Some(A));
    let bad = p.run(60, |_| PlayerInput { move_x: 1, ..Default::default() });
    assert!(p.host.player(A).unwrap().mv.di_left < 1e-3, "DI spent");
    assert_eq!(bad, 0);
}
