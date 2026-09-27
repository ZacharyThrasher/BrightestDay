//! Portals (owner: "make our portals allow full vision through and projectiles to travel right
//! through … and make them bigger"): hitscan legs through portals (lag-compensated, hop-capped,
//! deterministic), bigger construct portals with sane placement, doorways, and client prediction
//! parity for a player stepping through.

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::voxel::VoxelQuery;
use bd_core::{IVec3, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::GameEvent;
use bd_sim::rules::WillpowerMode;
use bd_sim::space::{DOOR_ASPECT, PORTAL_MIN_RADIUS, PORTAL_RADIUS, PORTAL_STYLE_DOOR, PortalEnd, fit_portal};
use bd_sim::weapons::HitscanShot;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn cfg() -> SimConfig {
    SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() }
}

fn world(n: u8) -> World {
    let mut w = World::new(cfg());
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

/// Legs (trace / beam events) a shot by `owner` produced this tick.
fn legs(w: &World, owner: PlayerId) -> Vec<(Vec3, Vec3)> {
    w.events()
        .iter()
        .filter_map(|e| match e {
            GameEvent::Hitscan { owner: o, origin, end, .. } if *o == owner => Some((*origin, *end)),
            GameEvent::ShotTrace { owner: o, from, to, .. } if *o == owner => Some((*from, *to)),
            _ => None,
        })
        .collect()
}

/// A at (0, 8, 20) looking −Z into a free-standing gate at z = 10 (eye height); the exit stands at
/// (30, eye, −20) facing +X. Returns (world, eye, exit centre).
fn gate_world() -> (World, Vec3, Vec3) {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, 8.0, 20.0));
    place(&mut w, B, Vec3::new(-30.0, 8.0, 30.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let entry = PortalEnd::new(Vec3::new(0.0, eye.y, 10.0), Vec3::Z, Vec3::Y, PORTAL_RADIUS);
    let exit = PortalEnd::new(Vec3::new(30.0, eye.y, -20.0), Vec3::X, Vec3::Y, PORTAL_RADIUS);
    w.create_portal(A, entry, exit, 60.0, None).unwrap();
    (w, eye, exit.pos)
}

#[test]
fn hitscan_through_a_portal_hits_a_target_behind_the_exit_as_the_shooter_saw_it() {
    let (mut w, eye, exit) = gate_world();
    // B stands 8 m out of the exit (the ray leaves it along +X at eye height).
    let target = Vec3::new(exit.x + 8.0, 8.0, exit.z);
    place(&mut w, B, target);
    for _ in 0..40 {
        w.tick(&[]);
    }
    let shot = HitscanShot { range: 150.0, ..HitscanShot::new(A, eye, Vec3::NEG_Z, 20.0, 5.0) };
    let before = w.player(B).unwrap().damage_pct;
    assert_eq!(w.hitscan(shot), 1, "hit through the portal");
    assert!(w.player(B).unwrap().damage_pct > before, "took the damage");
    let l = legs(&w, A);
    assert_eq!(l.len(), 2, "one leg to the entry, one out of the exit: {l:?}");
    assert!((l[0].1 - Vec3::new(0.0, eye.y, 10.0)).length() < 0.1, "first leg ends on the entry disc: {:?}", l[0]);
    assert!((l[1].0 - exit).length() < 0.2, "second leg starts at the exit: {:?}", l[1]);
    assert!(l[1].1.x > exit.x + 7.0, "and runs out of it along +X to B: {:?}", l[1]);

    // Lag compensation: B stepped aside since; A's view (20 ticks behind) still shows B in the line.
    let (mut w, eye, exit) = gate_world();
    place(&mut w, B, Vec3::new(exit.x + 8.0, 8.0, exit.z));
    for _ in 0..40 {
        w.tick(&[]);
    }
    let seen_tick = w.tick;
    place(&mut w, B, Vec3::new(exit.x + 8.0, 8.0, exit.z + 6.0));
    for _ in 0..3 {
        w.tick(&[]);
    }
    let shot = HitscanShot { range: 150.0, ..HitscanShot::new(A, eye, Vec3::NEG_Z, 20.0, 5.0) };
    assert_eq!(w.hitscan(shot), 0, "unrewound: B is no longer behind the exit");
    w.set_view_tick(A, Some(seen_tick as f64));
    assert_eq!(w.hitscan(shot), 1, "rewound to the shooter's view: hit through the portal");
}

#[test]
fn hitscan_portal_legs_are_capped() {
    // Two ends facing each other 10 m apart: a ray between them would loop forever.
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, 8.0, 5.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let a = PortalEnd::new(Vec3::new(0.0, eye.y, 0.0), Vec3::Z, Vec3::Y, PORTAL_RADIUS);
    let b = PortalEnd::new(Vec3::new(0.0, eye.y, 10.0), Vec3::NEG_Z, Vec3::Y, PORTAL_RADIUS);
    w.create_portal(A, a, b, 60.0, None).unwrap();
    w.tick(&[]);
    w.hitscan(HitscanShot { range: 300.0, ..HitscanShot::new(A, eye, Vec3::NEG_Z, 20.0, 5.0) });
    let l = legs(&w, A);
    assert_eq!(l.len(), bd_sim::space::PORTAL_HOPS as usize + 1, "{l:?}");
    // The legs chain: each one starts where the previous one entered, mapped to the exit.
    for k in 1..l.len() {
        assert!((l[k].0.z - 10.0).abs() < 0.1, "leg {k} leaves the exit: {:?}", l[k]);
    }
}

#[test]
fn hitscan_through_portals_is_deterministic() {
    let run = || {
        let (mut w, eye, exit) = gate_world();
        place(&mut w, B, Vec3::new(exit.x + 8.0, 8.0, exit.z));
        let mut h = Vec::new();
        for k in 0..120u32 {
            if k % 10 == 0 {
                let dir = (Vec3::NEG_Z + Vec3::X * ((k as f32 * 0.37).sin() * 0.05)).normalize();
                w.hitscan(HitscanShot { range: 150.0, pierce: 1, ..HitscanShot::new(A, eye, dir, 5.0, 3.0) });
            }
            let evs = w.tick(&[]);
            h.push(format!("{evs:?}"));
        }
        (w.state_hash(), h)
    };
    let (a, ea) = run();
    let (b, eb) = run();
    assert_eq!(a, b);
    assert_eq!(ea, eb);
}

// ------------------------------------------------------------------------------------ placement

/// Tiny voxel grid (0.5 m voxels): solid below y = 0, plus a wall of `wall_h` m at z ∈ [0, 1).
struct Grid {
    wall_h: f32,
}

impl VoxelQuery for Grid {
    fn material(&self, v: IVec3) -> u8 {
        let p = (v.as_vec3() + Vec3::splat(0.5)) * bd_core::VOXEL_SIZE;
        let floor = p.y < 0.0;
        let wall = p.z >= 0.0 && p.z < 1.0 && p.x.abs() < 12.0 && p.y < self.wall_h;
        (floor || wall) as u8
    }
}

fn samples_ok(g: &Grid, e: &PortalEnd, mounted: bool) -> bool {
    let solid = |p: Vec3| g.is_solid(bd_core::voxel::world_to_voxel(p));
    let (hx, hy) = if e.style == PORTAL_STYLE_DOOR { (e.radius * DOOR_ASPECT, e.radius) } else { (e.radius, e.radius) };
    (0..16).all(|k| {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        let q = e.pos + e.right() * (a.cos() * hx * 0.9) + e.up * (a.sin() * hy * 0.9);
        if mounted { !solid(q + e.normal * 0.3) && solid(q - e.normal * 0.35) } else { !solid(q) }
    })
}

#[test]
fn bigger_portals_fit_the_surface_and_clamp_on_small_walls() {
    assert!(PORTAL_RADIUS >= 1.4 * 1.5, "owner: ~1.5–2× the old 1.4 m radius");
    assert!(bd_sim::space::portal_radius_for(0.5) == PORTAL_RADIUS, "bigger floor");
    assert!(bd_sim::space::portal_radius_for(4.0) > PORTAL_RADIUS, "scales with the cast");
    // A big wall: the full-size gate fits where aimed (0.9 m up: slides up off the floor).
    let big = Grid { wall_h: 12.0 };
    let e = fit_portal(&big, PortalEnd::new(Vec3::new(0.0, 0.9, 1.05), Vec3::Z, Vec3::Y, PORTAL_RADIUS));
    assert_eq!(e.radius, PORTAL_RADIUS);
    assert!(e.pos.y - e.radius >= -0.3, "slid up off the floor: {}", e.pos);
    assert!((e.pos - Vec3::new(0.0, 0.9, 1.05)).length() <= PORTAL_RADIUS + 1e-3, "stays near the aim");
    assert!(samples_ok(&big, &e, true));
    // A 3 m wall: shrinks until the whole opening is on the wall.
    let mid = Grid { wall_h: 3.0 };
    let e = fit_portal(&mid, PortalEnd::new(Vec3::new(0.0, 1.5, 1.05), Vec3::Z, Vec3::Y, PORTAL_RADIUS));
    assert!(e.radius < PORTAL_RADIUS && e.radius >= PORTAL_MIN_RADIUS, "{}", e.radius);
    assert!(samples_ok(&mid, &e, true), "{e:?}");
    // A 1.5 m wall can't hold even the smallest: clamped to the minimum at the aim point.
    let small = Grid { wall_h: 1.5 };
    let e = fit_portal(&small, PortalEnd::new(Vec3::new(0.0, 0.75, 1.05), Vec3::Z, Vec3::Y, PORTAL_RADIUS));
    assert_eq!(e.radius, PORTAL_MIN_RADIUS);
    assert_eq!(e.pos, Vec3::new(0.0, 0.75, 1.05));
    // Free-standing (the end in front of the caster, 1.2 m above the feet): lifted out of the floor.
    let e = fit_portal(&big, PortalEnd::new(Vec3::new(0.0, 1.2, 8.0), Vec3::NEG_Z, Vec3::Y, PORTAL_RADIUS));
    assert_eq!(e.radius, PORTAL_RADIUS);
    assert!(e.pos.y - e.radius > -0.3 && e.pos.y - e.radius < 0.8, "stands on the floor: {}", e.pos);
    assert!(samples_ok(&big, &e, false));
    // A doorway is a tall rectangle.
    let d = fit_portal(&big, PortalEnd::new(Vec3::new(0.0, 1.7, 1.05), Vec3::Z, Vec3::Y, 1.7).door());
    assert_eq!(d.style, PORTAL_STYLE_DOOR);
    assert!(samples_ok(&big, &d, true));
    assert!(d.contains_lateral(Vec3::new(0.0, 1.5, 0.0), 0.0) && !d.contains_lateral(Vec3::new(1.2, 0.0, 0.0), 0.0));
}

#[test]
fn construct_portals_are_bigger_and_wall_portals_are_doorways() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let gate = recipe("portal", Form::Platform, &[Mechanic::Portal], Element::Light, 2.0);
    w.cast(A, gate, None, Aim::new(eye, Vec3::new(0.0, -0.4, -1.0).normalize())).unwrap();
    w.tick(&[]);
    let p = w.portals.iter().find(|p| p.owner == A).expect("portal opened");
    assert!(p.ends.iter().all(|e| e.radius >= PORTAL_MIN_RADIUS), "{:?}", p.ends);
    assert!(p.ends.iter().any(|e| e.radius >= PORTAL_RADIUS - 1e-3), "full size where it fits: {:?}", p.ends);
    let door = recipe("doorman door", Form::Wall, &[Mechanic::Portal], Element::Fire, 3.4);
    w.cast(A, door, None, Aim::new(eye, Vec3::new(0.0, -0.4, -1.0).normalize())).unwrap();
    w.tick(&[]);
    let p = w.portals.iter().find(|p| p.owner == A).expect("door opened");
    assert!(p.ends.iter().all(|e| e.style == PORTAL_STYLE_DOOR), "{:?}", p.ends);
    // Upright (a door aimed at the floor stands on it) and replicated with its style.
    assert!(p.ends.iter().all(|e| e.normal.y.abs() < 0.2), "{:?}", p.ends);
    let snap = w.snapshot_for(Some(A));
    assert!(snap.portals.iter().any(|s| s.ends.iter().all(|e| e.style == PORTAL_STYLE_DOOR)));
}

// ------------------------------------------------------------------------------------ prediction

#[test]
fn prediction_matches_the_host_through_a_portal() {
    let mut host = world(1);
    // Wall-to-wall pair: walk into a gate facing +Z, come out of one facing +X far away.
    let entry = PortalEnd::new(Vec3::new(0.0, 10.0, -6.0), Vec3::Z, Vec3::Y, PORTAL_RADIUS);
    let exit = PortalEnd::new(Vec3::new(-25.0, 10.4, 20.0), Vec3::X, Vec3::Y, PORTAL_RADIUS);
    host.create_portal(A, entry, exit, 60.0, None).unwrap();
    place(&mut host, A, Vec3::new(0.0, 8.0, 0.0));
    host.tick(&[]);
    let mut client = World::new(cfg());
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    assert_eq!(client.portals.len(), 1, "portals replicate");
    let mut crossed = false;
    let mut worst: f32 = 0.0;
    let mut yaw = 0.0f32; // looking −Z
    for k in 0..240u32 {
        let inp = PlayerInput { seq: k, move_z: 1, yaw, ..Default::default() };
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &inp);
        let evs = host.tick(&[(A, inp)]).to_vec();
        for e in &evs {
            if let GameEvent::Teleported { who: bd_sim::events::Traveller::Player(p), rot, .. } = e
                && *p == A
            {
                crossed = true;
                // The client turns its view with the portal (present::portal_cross does this).
                let look = *rot * bd_core::input::look_dir(yaw, 0.0);
                yaw = (-look.x).atan2(-look.z);
            }
            client.apply_event(e);
        }
        let h = host.player(A).unwrap().mv;
        worst = worst.max((h.pos - s.pos).length());
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    }
    assert!(crossed, "walked through");
    assert!(worst < 1e-3, "prediction diverged by {worst} m around the crossing");
    let p = host.player(A).unwrap().mv.pos;
    assert!(p.x > -25.0 && (p.z - 20.0).abs() < 3.0, "came out of the exit and kept walking: {p}");
}
