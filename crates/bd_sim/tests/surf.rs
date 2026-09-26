//! CS-style surfing (movement/surf.rs): descending lines gain speed when surfed, letting go slides
//! off, off-surf movement is bit-identical with surfing on or off, host/prediction parity, and the
//! voxel-staircase finding (a stepped voxel slope is ground, not surf).

use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_core::{DT, IVec3, Vec3};
use bd_sim::movement::testkit::{FnWorld, JUMP, input, optimal_strafe_yaw, scenarios};
use bd_sim::movement::{Collider, CollisionWorld, MoveParams, PlayerState, step_player, surf_line};

/// A 60 m surf line descending `drop` metres toward −Z, 60° faces, 8 m of slope each side.
fn line(drop: f32) -> [Collider; 2] {
    surf_line(Vec3::new(0.0, 40.0, 0.0), Vec3::new(0.0, 40.0 - drop, -60.0), 60.0, 8.0, 1.0, 100)
}

#[derive(Debug, Default)]
struct Ride {
    start: f32,
    end: f32,
    peak: f32,
    /// Ticks in surf contact.
    on: u32,
    /// Speed on the last surf tick (leaving the line).
    exit: f32,
    /// Metres travelled along the line before leaving it.
    along: f32,
    fell_off: bool,
}

/// Drop onto the right face near the top of the line at `v0` m/s along it and ride: `hold` =
/// strafe into the face (A: the face is on the right, its downhill side points away) with the view
/// along the line, else no keys.
fn ride(p: &MoveParams, drop: f32, v0: f32, hold: bool) -> Ride {
    let cols = line(drop);
    let w = FnWorld(|_: IVec3| false);
    let cw = CollisionWorld::new(&w, &cols);
    let d = Vec3::new(0.0, -drop, -60.0).normalize();
    // Right face (+X side): a point 2.5 m down the face from the ridge, 3 m along, just off it.
    let f = cols[0].axes()[0];
    let n = cols[0].axes()[1];
    let on_face = Vec3::new(0.0, 40.0, 0.0) + d * 3.0 + f * 2.5 + n * 0.05;
    let mut s = PlayerState::new(on_face - Vec3::new(-0.4, 0.0, 0.0), p);
    s.air_time = 0.5;
    s.vel = d * v0;
    let mut r = Ride { start: s.speed(), peak: s.speed(), ..Default::default() };
    let yaw = (-d.x).atan2(-d.z);
    for _ in 0..(4.0 / DT) as usize {
        // Strafe toward the face: the face is to the right of the line direction, so its surface
        // (normal +X-ish) is on our left when looking down the line → hold A (move_x −1)… the face's
        // outward normal points +X, i.e. we ride on its outside: push toward −X = left.
        let i = if hold { input(yaw, 0.0, -1, 0, 0) } else { input(yaw, 0.0, 0, 0, 0) };
        step_player(&mut s, &i, &cw, p, DT);
        if bd_sim::movement::surfing(&s, p) {
            r.on += 1;
            r.along = (s.pos - on_face).dot(d);
            r.exit = s.speed();
        } else if r.on > 10 && !r.fell_off && s.pos.dot(d) < 58.0 {
            r.fell_off = true;
        }
        r.peak = r.peak.max(s.speed());
        if s.pos.y < 40.0 - drop - 20.0 {
            break;
        }
    }
    r.end = s.speed();
    r
}

#[test]
fn a_descending_line_gains_speed_when_surfed() {
    let p = MoveParams::default();
    for drop in [10.0f32, 20.0, 30.0] {
        let r = ride(&p, drop, 12.0, true);
        eprintln!("60 m line, drop {drop} m, entry 12 m/s, holding: {r:?}");
        assert!(r.along > 50.0, "rode the whole line (drop {drop}): {r:?}");
        assert!(r.peak > 20.0 && r.peak > r.start + 8.0, "gained speed (drop {drop}): {r:?}");
    }
}

#[test]
fn releasing_the_strafe_slides_off() {
    let p = MoveParams::default();
    let r = ride(&p, 20.0, 12.0, false);
    eprintln!("no keys: {r:?}");
    assert!(r.along < 40.0, "should slide off the face before the end: {r:?}");
}

#[test]
fn surf_speed_is_not_soft_capped_but_hard_capped() {
    let p = MoveParams::default();
    let r = ride(&p, 30.0, 38.0, true);
    eprintln!("entry 38 m/s, drop 30: {r:?}");
    assert!(r.peak > p.soft_cap_speed + 3.0, "no soft-cap bleed on surf: {r:?}");
    assert!(r.peak <= p.hard_cap_speed + 1e-3);
}

#[test]
fn surf_off_changes_nothing_off_surf() {
    // Bhop + strafe over voxel terrain and among axis-aligned boxes: identical with surf on / off.
    let on = MoveParams::default();
    let off = MoveParams { surf: false, surf_air_accel: 9999.0, ..MoveParams::default() };
    let w = scenarios::arena();
    let cw = CollisionWorld::new(&w, &[]);
    let (mut a, mut b) = (PlayerState::new(Vec3::new(0.0, 0.01, 1.0), &on), PlayerState::new(Vec3::new(0.0, 0.01, 1.0), &off));
    a.vel = Vec3::new(10.0, 0.0, 0.0);
    b.vel = a.vel;
    for t in 0..2400 {
        let mx: i8 = if (t / 90) % 2 == 0 { 1 } else { -1 };
        let yaw = optimal_strafe_yaw(a.vel, &on, DT, mx, 0, mx as f32);
        let i = input(yaw, 0.0, mx, 0, JUMP);
        step_player(&mut a, &i, &cw, &on, DT);
        step_player(&mut b, &i, &cw, &off, DT);
        assert_eq!((a.pos, a.vel), (b.pos, b.vel), "tick {t}");
        assert_eq!(a.extra.surf_n, Vec3::ZERO);
    }
}

#[test]
fn a_voxel_staircase_is_ground_not_surf() {
    // 60°-ish staircase of 0.5 m voxels (two up per one across) descending toward −Z: its step tops
    // are walkable, so a rider lands on them (friction) instead of surfing.
    let p = MoveParams::default();
    let w = FnWorld(|v: IVec3| v.x >= 0 && v.x < 40 && v.y >= 0 && v.y < 80 - 2 * v.z.clamp(0, 40) && v.z >= 0);
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(10.0, 25.0, 5.0), &p);
    s.vel = Vec3::new(0.0, 0.0, 15.0);
    let (mut grounded, mut surf) = (0, 0);
    for _ in 0..240 {
        step_player(&mut s, &input(0.0, 0.0, 0, 0, 0), &cw, &p, DT);
        grounded += s.grounded as u32;
        surf += bd_sim::movement::surfing(&s, &p) as u32;
    }
    eprintln!("voxel staircase: grounded {grounded} ticks, surfing {surf} ticks, end speed {:.1}", s.horizontal_speed());
    assert_eq!(surf, 0, "voxel steps are never surf faces");
    assert!(grounded > 0, "the step tops are ground");
}

#[test]
fn surf_air_accel_measurements() {
    // Report numbers (not a tuning target): exit speed off the 60 m line vs surf_air_accel.
    for aa in [12.0f32, 60.0, 120.0, 240.0] {
        let p = MoveParams { surf_air_accel: aa, ..MoveParams::default() };
        for drop in [10.0f32, 20.0, 30.0] {
            let r = ride(&p, drop, 12.0, true);
            eprintln!("surf_air_accel {aa:>5}: drop {drop} m → exit {:.1} m/s after {:.1} m ({} surf ticks)", r.exit, r.along, r.on);
        }
    }
    let _ = PlayerInput::default();
}

// ============================================================================== map lines

use bd_sim::world::{ArenaSource, SimConfig, World};

fn archipelago() -> World {
    World::new(SimConfig { arena: ArenaSource::Archipelago { seed: 21 }, check_invariants: true, ..SimConfig::default() })
}

/// Hold the strafe key that pushes into `face` with the view along the line.
fn surf_input(face: &Collider, d: Vec3) -> PlayerInput {
    let n = face.axes()[1];
    let yaw = (-d.x).atan2(-d.z);
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let into = Vec3::new(-n.x, 0.0, -n.z);
    input(yaw, 0.0, if into.dot(right) > 0.0 { 1 } else { -1 }, 0, 0)
}

/// Feet position that puts the hull's face-side bottom corner `gap` m off `face` at `point`.
fn on_face(face: &Collider, point: Vec3, gap: f32) -> Vec3 {
    let n = face.axes()[1];
    let corner = Vec3::new(-0.4 * n.x.signum(), 0.0, -0.4 * n.z.signum());
    point - corner + n * gap
}

#[test]
fn map_surf_lines_exist_and_spawns_are_clear() {
    for (mut w, lines) in [(archipelago(), 2), (miami(), 1)] {
        w.tick(&[]);
        assert_eq!(w.arena.surf_lines.len(), lines);
        let surf: Vec<&Collider> = w
            .colliders()
            .iter()
            .filter(|c| matches!(c.id, bd_sim::movement::ColliderId::Other(i) if i >= bd_sim::movement::SURF_ID_BASE))
            .collect();
        assert_eq!(surf.len(), 2 * lines, "two faces per line, pushed every tick");
        for s in &w.arena.spawns {
            for c in &surf {
                let l = c.to_local(*s + Vec3::Y * 0.9);
                let out = (l.abs() - c.half).max(Vec3::ZERO).length();
                assert!(out > 2.0, "spawn {s} is {out:.2} m from a surf face");
            }
        }
        let p = MoveParams::default();
        for c in &surf {
            assert!(bd_sim::movement::surfable(c.axes()[1], &p), "map faces are surf faces");
        }
    }
}

fn miami() -> World {
    let (v, info) = bd_voxel::generate_miami(7);
    World::with_arena(SimConfig { check_invariants: true, ..SimConfig::default() }, v, info.arena.clone(), 7)
}

#[test]
fn scripted_surf_down_the_map_lines() {
    for (map, k, name) in [(0, 0usize, "Spire Slide"), (0, 1, "Crag Chute"), (1, 0, "Miami Causeway Chute")] {
        let mut w = if map == 0 { archipelago() } else { miami() };
        w.add_player(bd_core::PlayerId(1), "surfer".into(), bd_core::corps::Corps::from_index(1)).unwrap();
        w.start_live();
        w.tick(&[]);
        let l = w.arena.surf_lines[k];
        let faces = surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0);
        let d = (l.b - l.a).normalize();
        let f = faces[0].axes()[0];
        // A quarter of the way down (the start is buried in the island you run off).
        let start = on_face(&faces[0], l.a + (l.b - l.a) * 0.25 + f * 2.5, 0.05);
        let p = w.params().clone();
        {
            let pl = w.player_mut(bd_core::PlayerId(1)).unwrap();
            pl.spawn_shield = 0.0;
            pl.mv.pos = start;
            pl.mv.vel = d * 10.0;
            pl.mv.grounded = false;
            pl.mv.air_time = 0.5;
        }
        let i = surf_input(&faces[0], d);
        let (mut on, mut exit, mut along, mut landed) = (0, 0.0f32, 0.0f32, None);
        for t in 0..(6.0 / DT) as usize {
            w.tick(&[(bd_core::PlayerId(1), i)]);
            let pl = w.player(bd_core::PlayerId(1)).unwrap();
            assert!(pl.alive, "{name}: died at tick {t} ({})", pl.mv.pos);
            if bd_sim::movement::surfing(&pl.mv, &p) {
                on += 1;
                exit = pl.mv.speed();
                along = (pl.mv.pos - start).dot(d);
            } else if on > 0 && pl.mv.grounded && landed.is_none() {
                landed = Some((t, pl.mv.pos, pl.mv.horizontal_speed()));
                break;
            }
        }
        let len = (l.b - l.a).length();
        eprintln!(
            "{name}: line {len:.0} m, drop {:.0} m; surfed {along:.0} m ({on} ticks), exit {exit:.1} m/s, landed {landed:?}",
            l.a.y - l.b.y
        );
        assert!(along > len * 0.75 - 6.0, "{name}: rode the line ({along:.1} of {len:.1} m)");
        assert!(exit > 20.0, "{name}: surf built speed ({exit:.1})");
        assert!(landed.is_some(), "{name}: landed on the island below");
    }
}

/// Solid voxels inside `c` (sampled every 0.5 m), skipping the first `skip` fraction of its length
/// (the run-off start buried in the island you leave from).
fn buried(w: &World, c: &Collider, skip: f32) -> Vec<Vec3> {
    let mut hits = Vec::new();
    let steps = |h: f32| ((2.0 * h) / 0.5).ceil() as i32;
    let (nx, ny, nz) = (steps(c.half.x), steps(c.half.y), steps(c.half.z));
    for k in 0..=nz {
        let z = -c.half.z + 2.0 * c.half.z * k as f32 / nz as f32;
        // Local +z runs from b to a or a to b depending on the face; test both ends by position below.
        for i in 0..=nx {
            for j in 0..=ny {
                let l = Vec3::new(-c.half.x + 2.0 * c.half.x * i as f32 / nx as f32, -c.half.y + 2.0 * c.half.y * j as f32 / ny as f32, z);
                let p = c.to_world(l);
                if w.voxels.is_solid(bd_core::voxel::world_to_voxel(p)) {
                    hits.push(p);
                }
            }
        }
    }
    let _ = skip;
    hits
}

#[test]
fn map_surf_faces_are_clear_of_terrain_past_the_run_off() {
    for w in [archipelago(), miami()] {
        for (k, l) in w.arena.surf_lines.iter().enumerate() {
            let len = (l.b - l.a).length();
            let d = (l.b - l.a) / len;
            for c in surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0) {
                let bad: Vec<Vec3> = buried(&w, &c, 0.0).into_iter().filter(|p| (*p - l.a).dot(d) > len * 0.22).collect();
                assert!(bad.is_empty(), "line {k}: {} buried samples, e.g. {:?}", bad.len(), &bad[..bad.len().min(3)]);
            }
        }
    }
}

#[test]
fn prediction_matches_the_host_on_a_surf_line() {
    use bd_core::PlayerId;
    use bd_core::corps::Corps;
    let make = || {
        let (v, mut info) = bd_voxel::generate_flat_test(96.0);
        info.surf_lines.push(bd_voxel::SurfLine {
            a: Vec3::new(0.0, 40.0, 40.0),
            b: Vec3::new(0.0, 22.0, -20.0),
            face_deg: 60.0,
            width: 8.0,
        });
        World::with_arena(SimConfig { check_invariants: true, ..SimConfig::default() }, v, info, 1)
    };
    let a = PlayerId(1);
    let mut host = make();
    host.add_player(a, "a".into(), Corps::from_index(1)).unwrap();
    host.start_live();
    host.tick(&[]);
    let l = host.arena.surf_lines[0];
    let faces = surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0);
    let d = (l.b - l.a).normalize();
    let start = on_face(&faces[0], l.a + (l.b - l.a) * 0.1 + faces[0].axes()[0] * 2.5, 0.05);
    {
        let p = host.player_mut(a).unwrap();
        p.spawn_shield = 0.0;
        p.mv.pos = start;
        p.mv.vel = d * 12.0;
        p.mv.grounded = false;
        p.mv.air_time = 0.5;
    }
    host.tick(&[]);
    let mut client = make();
    client.apply_snapshot(&host.snapshot_for(Some(a)), Some(a));
    let hold = surf_input(&faces[0], d);
    let params = host.params().clone();
    let (mut bad, mut surfed) = (0, 0);
    for k in 0..360u32 {
        // Hold for 2 s (a jump off the face at 1 s), then let go and slide off.
        let mut i = if k < 240 { hold } else { input(hold.yaw, 0.0, 0, 0, 0) };
        if k == 120 {
            i.buttons |= JUMP;
        }
        let mut s = client.player(a).unwrap().mv;
        client.predict_player(a, &mut s, &i);
        let evs = host.tick(&[(a, i)]).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(a)), Some(a));
        let h = host.player(a).unwrap();
        surfed += bd_sim::movement::surfing(&h.mv, &params) as u32;
        if (h.mv.pos - s.pos).length() > 1e-4 || (h.mv.vel - s.vel).length() > 1e-3 || h.mv.extra != s.extra {
            bad += 1;
        }
    }
    assert!(surfed > 30, "surfed {surfed} ticks");
    assert_eq!(bad, 0, "mispredicted ticks");
}

#[test]
fn a_cast_surf_ramp_stands_steep_a_plain_ramp_does_not() {
    use bd_core::PlayerId;
    use bd_core::corps::Corps;
    use bd_core::mechanics::{Element, Form, Mechanic};
    use bd_sim::movement::ColliderId;
    use bd_sim::world::Aim;
    let mut w = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() });
    w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
    let a = PlayerId(1);
    w.add_player(a, "a".into(), Corps::from_index(1)).unwrap();
    w.start_live();
    w.tick(&[]);
    let eye = w.player(a).unwrap().eye(w.params());
    let surf_ramp = bd_sim::bots::recipe("surf ramp", Form::Ramp, &[Mechanic::Glide], Element::Light, 12.0);
    let plain = bd_sim::bots::recipe("ramp", Form::Ramp, &[], Element::Light, 12.0);
    let p = MoveParams::default();
    for (r, steep) in [(surf_ramp, true), (plain, false)] {
        w.cast(a, r, None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0).normalize())).unwrap();
        for _ in 0..90 {
            w.tick(&[]);
        }
        let top = w
            .colliders()
            .iter()
            .filter(|c| matches!(c.id, ColliderId::Construct { .. }))
            .map(|c| c.axes()[1])
            .fold(Vec3::NEG_Y, |best, n| if n.y > best.y { n } else { best });
        eprintln!("steep {steep}: ramp top normal {top:.2} (surfable {})", bd_sim::movement::surfable(top, &p));
        assert_eq!(bd_sim::movement::surfable(top, &p), steep);
        w.dismiss(a);
        for _ in 0..30 {
            w.tick(&[]);
        }
    }
}
