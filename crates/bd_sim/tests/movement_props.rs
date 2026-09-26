//! Property tests for the movement controller (see docs/TESTING.md).
//!
//! Random small voxel worlds + random oriented-box colliders + random (including garbage) input
//! sequences + random impulses/modifiers. Every tick must satisfy:
//! - all state finite; gas within [0, gas_max]; speed ≤ hard cap (+ tolerance for moving surfaces);
//! - the hull never overlaps a solid voxel or collider beyond a small epsilon (no tunnelling);
//! - hooks never fly past `tether_range`, anchored ropes have sane lengths, and anchors are real
//!   (solid voxel / existing anchorable collider);
//! - determinism: two runs are bit-identical, and a bincode snapshot/restore mid-run changes nothing
//!   (what client reconciliation relies on).

use bd_core::input::PlayerInput;
use bd_core::voxel::{Aabb, VoxelQuery, voxel_min};
use bd_core::{ConstructId, DT, IVec3, Quat, VOXEL_SIZE, Vec3};
use bd_sim::movement::testkit::{FnWorld, TestGrid};
use bd_sim::movement::{
    AnchorTarget, Collider, ColliderId, CollisionWorld, HookPhase, MoveEvents, MoveModifiers, MoveParams, PlayerState, step_player,
};
use proptest::prelude::*;

/// Case count × `BD_PROP_SCALE` (default 1; `tools/check.ps1 -Deep` sets 10).
fn scaled(n: u32) -> u32 {
    std::env::var("BD_PROP_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).map_or(n, |k| n.saturating_mul(k.max(1)))
}

/// Allowed overlap (m) between the hull and solid geometry at tick end. The controller keeps a 2 mm
/// skin; anything deeper than this is penetration.
const PEN_EPS: f32 = 0.01;

// ------------------------------------------------------------------------------------ strategies

#[derive(Clone, Debug)]
struct WorldSpec {
    boxes: Vec<(IVec3, IVec3)>,
    specks: Vec<IVec3>,
    colliders: Vec<Collider>,
}

fn ivec(lo: IVec3, hi: IVec3) -> impl Strategy<Value = IVec3> {
    (lo.x..hi.x, lo.y..hi.y, lo.z..hi.z).prop_map(|(x, y, z)| IVec3::new(x, y, z))
}

fn collider(i: u32) -> impl Strategy<Value = Collider> {
    (
        (-5.0f32..5.0, -1.0f32..7.0, -5.0f32..5.0),
        (0.05f32..3.0, 0.05f32..2.0, 0.05f32..3.0),
        (-3.2f32..3.2, -3.2f32..3.2, -3.2f32..3.2),
        prop_oneof![3 => Just(Vec3::ZERO), 1 => (-5.0f32..5.0, -2.0f32..2.0, -5.0f32..5.0).prop_map(|(x, y, z)| Vec3::new(x, y, z))],
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(move |(c, h, r, vel, solid, anchorable)| {
            let mut col = Collider::new(
                Vec3::new(c.0, c.1, c.2),
                Vec3::new(h.0, h.1, h.2),
                Quat::from_euler(glam::EulerRot::XYZ, r.0, r.1, r.2),
                ColliderId::Construct { id: ConstructId(i), part: 0 },
            )
            .with_vel(vel);
            col.solid = solid || !anchorable;
            col.anchorable = anchorable;
            col
        })
}

fn world_spec() -> impl Strategy<Value = WorldSpec> {
    let lo = IVec3::new(-12, -2, -12);
    let hi = IVec3::new(12, 20, 12);
    (
        prop::collection::vec((ivec(lo, hi), (1i32..6, 1i32..8, 1i32..6)), 0..12),
        prop::collection::vec(ivec(lo, hi), 0..40),
        prop::collection::vec(0u32..1, 0..4),
    )
        .prop_flat_map(|(boxes, specks, n)| {
            let cols: Vec<_> = (0..n.len() as u32).map(collider).collect();
            (Just(boxes), Just(specks), cols)
        })
        .prop_map(|(boxes, specks, colliders)| WorldSpec {
            boxes: boxes.into_iter().map(|(a, s)| (a, a + IVec3::new(s.0, s.1, s.2) - IVec3::ONE)).collect(),
            specks,
            colliders,
        })
}

const SPAWN: Vec3 = Vec3::new(0.25, 0.0, 0.25);

fn build(spec: &WorldSpec, p: &MoveParams) -> (TestGrid, Vec<Collider>) {
    let mut g = TestGrid::new(IVec3::new(-14, -4, -14), IVec3::new(14, 40, 14));
    g.fill(IVec3::new(-14, -2, -14), IVec3::new(13, -1, 13), 1);
    for &(a, b) in &spec.boxes {
        g.fill(a, b, 3);
    }
    for &v in &spec.specks {
        g.set(v, 2);
    }
    // Clear room around the spawn so the player starts free.
    let spawn_hull = Aabb { min: SPAWN - Vec3::new(p.hull_half_width + 0.3, 0.0, p.hull_half_width + 0.3), max: SPAWN + Vec3::new(p.hull_half_width + 0.3, p.stand_height + 0.3, p.hull_half_width + 0.3) };
    let (lo, hi) = spawn_hull.voxel_range();
    g.fill(lo, hi, 0);
    let cols = spec.colliders.iter().copied().filter(|c| !c.bounds().intersects(&spawn_hull)).collect();
    (g, cols)
}

fn wild_angle() -> impl Strategy<Value = f32> {
    prop_oneof![
        8 => -7.0f32..7.0,
        1 => any::<f32>(),
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => Just(1e30f32),
    ]
}

/// One input held for a number of ticks (holding buttons is what makes tethers attach and swing).
fn segment() -> impl Strategy<Value = (PlayerInput, u8)> {
    (
        wild_angle(),
        wild_angle(),
        prop_oneof![6 => -1i8..=1, 1 => any::<i8>()],
        prop_oneof![6 => -1i8..=1, 1 => any::<i8>()],
        prop_oneof![4 => any::<u16>(), 2 => 0u16..0x40, 1 => Just(0u16)],
        prop_oneof![8 => Just(0i8), 1 => any::<i8>(), 1 => -3i8..=3],
        1u8..40,
    )
        .prop_map(|(yaw, pitch, move_x, move_z, buttons, reel_notches, hold)| {
            (PlayerInput { seq: 0, yaw, pitch, move_x, move_z, buttons, reel_notches }, hold)
        })
}

fn wild_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        6 => -60.0f32..60.0,
        1 => any::<f32>(),
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => Just(f32::NEG_INFINITY),
        1 => Just(1e30f32),
        1 => Just(-1e30f32),
        1 => Just(f32::MAX),
    ]
}

#[derive(Clone, Debug)]
enum Poke {
    Impulse(Vec3, f32),
    Mods(MoveModifiers),
    ReleaseTethers,
}

fn mods() -> impl Strategy<Value = MoveModifiers> {
    (
        (0.0f32..3.0, 0.0f32..1.5, prop_oneof![2 => Just(0.0f32), 1 => 0.0f32..120.0], 0.0f32..40.0),
        (prop_oneof![2 => Just(0.0f32), 1 => 0.0f32..10.0], 0.0f32..3.0, 0.0f32..3.0, 0.0f32..3.0),
        0.0f32..3.0,
    )
        .prop_map(|((gravity_scale, glide, thrust, thrust_max_speed), (slow_fall, speed_mult, air_control, jump_mult), accel_mult)| {
            MoveModifiers {
                gravity_scale,
                glide,
                thrust,
                thrust_max_speed,
                slow_fall,
                speed_mult,
                air_control,
                jump_mult,
                accel_mult,
            }
        })
}

fn poke() -> impl Strategy<Value = Poke> {
    prop_oneof![
        4 => ((wild_f32(), wild_f32(), wild_f32()), wild_f32()).prop_map(|((x, y, z), t)| Poke::Impulse(Vec3::new(x, y, z), t)),
        3 => mods().prop_map(Poke::Mods),
        1 => Just(Poke::ReleaseTethers),
    ]
}

/// Expand segments into a per-tick input list (bounded length).
fn expand(segs: &[(PlayerInput, u8)], max_ticks: usize) -> Vec<PlayerInput> {
    let mut out = Vec::new();
    for (i, h) in segs {
        for k in 0..*h {
            let mut i = *i;
            if k > 0 {
                i.reel_notches = 0; // notches are per-tick deltas
            }
            out.push(i);
            if out.len() >= max_ticks {
                return out;
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------ invariants

fn finite_state(s: &PlayerState) -> bool {
    let f = |v: f32| v.is_finite();
    s.pos.is_finite()
        && s.vel.is_finite()
        && s.ground_normal.is_finite()
        && s.ground_vel.is_finite()
        && [s.slide_cooldown, s.jump_buffer, s.air_time, s.gas, s.gas_regen_delay, s.burst_cooldown, s.tumble].into_iter().all(f)
        && s.hooks.iter().all(|h| h.pos.is_finite() && h.vel.is_finite() && h.local.is_finite() && f(h.travelled) && f(h.rope_len))
}

/// Penetration depth of `a` into the oriented box (SAT, 15 axes); ≤ 0 means separated.
fn obb_penetration(a: &Aabb, c: &Collider) -> f32 {
    let (ca, ha) = (a.center(), a.half());
    let u = c.axes();
    let w = [Vec3::X, Vec3::Y, Vec3::Z];
    let mut axes = vec![w[0], w[1], w[2], u[0], u[1], u[2]];
    for wi in w {
        for ui in u {
            axes.push(wi.cross(ui));
        }
    }
    let mut min_pen = f32::INFINITY;
    for l in axes {
        if l.length_squared() < 1e-6 {
            continue;
        }
        let l = l.normalize();
        let ra = ha.x * l.x.abs() + ha.y * l.y.abs() + ha.z * l.z.abs();
        let rb = c.half.x * u[0].dot(l).abs() + c.half.y * u[1].dot(l).abs() + c.half.z * u[2].dot(l).abs();
        let pen = ra + rb - (c.center - ca).dot(l).abs();
        min_pen = min_pen.min(pen);
    }
    min_pen
}

fn check_tick<V: VoxelQuery>(
    s: &PlayerState,
    ev: &MoveEvents,
    vox: &V,
    cols: &[Collider],
    p: &MoveParams,
    tick: usize,
) -> Result<(), TestCaseError> {
    prop_assert!(finite_state(s), "tick {}: non-finite state {:?}", tick, s);
    prop_assert!(s.gas >= 0.0 && s.gas <= p.gas_max + 1e-3, "tick {}: gas {}", tick, s.gas);
    let max_surface = cols.iter().map(|c| c.vel.length()).fold(0.0, f32::max);
    prop_assert!(s.speed() <= p.hard_cap_speed * 1.02 + 2.0 * max_surface + 0.5, "tick {}: speed {} > hard cap", tick, s.speed());

    // No penetration into voxels.
    let hull = s.hull(p);
    let inner = Aabb { min: hull.min + Vec3::splat(PEN_EPS), max: hull.max - Vec3::splat(PEN_EPS) };
    let (lo, hi) = inner.voxel_range();
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                let v = IVec3::new(x, y, z);
                if vox.is_solid(v) {
                    let vb = Aabb { min: voxel_min(v), max: voxel_min(v) + Vec3::splat(VOXEL_SIZE) };
                    prop_assert!(!inner.intersects(&vb), "tick {}: hull {:?} penetrates voxel {:?}", tick, hull, v);
                }
            }
        }
    }
    // No penetration into solid colliders.
    for c in cols.iter().filter(|c| c.solid) {
        let pen = obb_penetration(&hull, c);
        prop_assert!(pen <= PEN_EPS * 2.0, "tick {}: hull penetrates collider {:?} by {}", tick, c.id, pen);
    }

    // Tethers.
    for (i, h) in s.hooks.iter().enumerate() {
        prop_assert!(h.travelled <= p.tether_range + 1e-3, "tick {}: hook {} flew {}", tick, i, h.travelled);
        if h.phase == HookPhase::Attached {
            // A rope is never longer than `tether_range` (it attaches taut at the range if the
            // player moved away while the hook flew).
            prop_assert!(h.rope_len >= p.tether_min_len - 1e-4 && h.rope_len <= p.tether_range + 1e-3, "tick {}: rope_len {}", tick, h.rope_len);
            // The anchor itself can be further away than the range for a moment (the rope winds
            // in at `rope_retract_speed`), but never by more than the player could travel while
            // the hook flew.
            let max_rope = p.tether_range + p.hard_cap_speed * (p.tether_range / p.hook_speed) + 3.0;
            match h.target {
                Some(AnchorTarget::Voxel(v)) => prop_assert!(vox.is_solid(v), "tick {}: anchored to air {:?}", tick, v),
                Some(AnchorTarget::Collider(id)) => {
                    prop_assert!(cols.iter().any(|c| c.id == id && c.anchorable), "tick {}: anchored to missing collider", tick)
                }
                None => prop_assert!(false, "tick {}: attached hook without target", tick),
            }
            if ev.tether_attached[i] {
                let d = (h.pos - s.eye(p)).length();
                prop_assert!(d <= max_rope, "tick {}: anchored {} m away (range {})", tick, d, p.tether_range);
            }
        } else {
            prop_assert!(h.target.is_none() || h.phase == HookPhase::Flying || h.phase == HookPhase::Idle);
        }
    }
    Ok(())
}

/// Run a full sequence; returns every state (for determinism comparisons).
fn run<V: VoxelQuery>(
    vox: &V,
    cols: &[Collider],
    p: &MoveParams,
    inputs: &[PlayerInput],
    pokes: &[(usize, Poke)],
    restore_at: Option<usize>,
) -> Result<Vec<(PlayerState, MoveEvents)>, TestCaseError> {
    let w = CollisionWorld::new(vox, cols);
    let mut s = PlayerState::new(SPAWN, p);
    let mut out = Vec::with_capacity(inputs.len());
    for (t, input) in inputs.iter().enumerate() {
        for (at, pk) in pokes {
            if *at == t {
                match pk {
                    Poke::Impulse(dv, tumble) => s.apply_impulse(*dv, *tumble),
                    Poke::Mods(m) => s.mods = *m,
                    Poke::ReleaseTethers => s.release_tethers(),
                }
            }
        }
        if restore_at == Some(t) {
            // Snapshot → bytes → state, exactly like a reconciliation reset.
            let bytes = bincode::serde::encode_to_vec(s, bincode::config::standard()).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let (back, _): (PlayerState, usize) =
                bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(back, s);
            s = back;
        }
        let before = s;
        let ev = step_player(&mut s, input, &w, p, DT);
        check_tick(&s, &ev, vox, cols, p, t).map_err(|e| {
            // Context for debugging: the state before the bad tick, the input and the events.
            TestCaseError::fail(format!("{e}
  before: {before:?}
  input: {input:?}
  events: {ev:?}"))
        })?;
        out.push((s, ev));
    }
    Ok(out)
}

// ----------------------------------------------------------------------------------------- tests

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(256), ..ProptestConfig::default() })]

    #[test]
    fn random_play_keeps_invariants_and_is_deterministic(
        spec in world_spec(),
        segs in prop::collection::vec(segment(), 1..40),
        pokes in prop::collection::vec((0usize..600, poke()), 0..8),
        restore in 0usize..600,
    ) {
        let p = MoveParams::default();
        let (grid, cols) = build(&spec, &p);
        let inputs = expand(&segs, 600);
        let a = run(&grid, &cols, &p, &inputs, &pokes, None)?;
        let b = run(&grid, &cols, &p, &inputs, &pokes, Some(restore))?;
        prop_assert_eq!(a.len(), b.len());
        for (t, (x, y)) in a.iter().zip(&b).enumerate() {
            prop_assert!(x == y, "tick {}: runs diverged\n{:?}\n{:?}", t, x, y);
        }
    }

    /// An infinite 1-voxel wall at x ∈ [0, 0.5): whatever the inputs, launches and tethers, the
    /// player never ends a tick inside it or on the other side.
    #[test]
    fn never_tunnels_through_a_thin_wall(
        segs in prop::collection::vec(segment(), 1..40),
        launches in prop::collection::vec((0usize..400, (0.0f32..200.0, -30.0f32..60.0, -40.0f32..40.0)), 0..10),
        start_x in -8.0f32..-1.0,
    ) {
        let p = MoveParams::default();
        let world = FnWorld(|v: IVec3| v.y < 0 || v.x == 0);
        let w = CollisionWorld::new(&world, &[]);
        let mut s = PlayerState::new(Vec3::new(start_x, 0.0, 0.3), &p);
        for (t, input) in expand(&segs, 400).iter().enumerate() {
            for (at, (vx, vy, vz)) in &launches {
                if *at == t {
                    s.apply_impulse(Vec3::new(*vx, *vy, *vz), 0.5);
                }
            }
            let ev = step_player(&mut s, input, &w, &p, DT);
            check_tick(&s, &ev, &world, &[], &p, t)?;
            prop_assert!(s.hull(&p).max.x <= PEN_EPS, "tick {}: tunnelled to x={}", t, s.pos.x);
        }
    }

    /// Extreme impulses (NaN, ±inf, 1e30, f32::MAX) and tumble values are absorbed safely.
    #[test]
    fn extreme_impulses_are_safe(dv in (wild_f32(), wild_f32(), wild_f32()), tumble in wild_f32(), ticks in 1usize..30) {
        let p = MoveParams::default();
        let world = FnWorld(|v: IVec3| v.y < 0);
        let w = CollisionWorld::new(&world, &[]);
        let mut s = PlayerState::new(Vec3::ZERO, &p);
        s.apply_impulse(Vec3::new(dv.0, dv.1, dv.2), tumble);
        s.apply_impulse(Vec3::new(dv.0, dv.1, dv.2), tumble);
        for t in 0..ticks {
            let ev = step_player(&mut s, &PlayerInput::default(), &w, &p, DT);
            check_tick(&s, &ev, &world, &[], &p, t)?;
        }
    }

    /// Garbage modifiers written by buggy construct code (NaN/inf/negative) must not poison the
    /// player: the state stays finite.
    #[test]
    fn garbage_modifiers_are_contained(vals in prop::collection::vec(wild_f32(), 9), ticks in 1usize..60, buttons in any::<u16>()) {
        let p = MoveParams::default();
        let world = FnWorld(|v: IVec3| v.y < 0);
        let w = CollisionWorld::new(&world, &[]);
        let mut s = PlayerState::new(Vec3::ZERO, &p);
        s.mods = MoveModifiers {
            gravity_scale: vals[0],
            glide: vals[1],
            thrust: vals[2],
            thrust_max_speed: vals[3],
            slow_fall: vals[4],
            speed_mult: vals[5],
            air_control: vals[6],
            jump_mult: vals[7],
            accel_mult: vals[8],
        };
        let input = PlayerInput { buttons, move_z: 1, pitch: 0.3, ..Default::default() };
        for t in 0..ticks {
            let ev = step_player(&mut s, &input, &w, &p, DT);
            check_tick(&s, &ev, &world, &[], &p, t)?;
        }
    }
}

#[test]
fn garbage_dt_is_a_no_op() {
    let p = MoveParams::default();
    let world = FnWorld(|v: IVec3| v.y < 0);
    let w = CollisionWorld::new(&world, &[]);
    for dt in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut s = PlayerState::new(Vec3::new(0.0, 3.0, 0.0), &p);
        let before = s;
        step_player(&mut s, &PlayerInput::default(), &w, &p, dt);
        assert_eq!(s, before, "dt={dt}");
    }
}

/// A floor made only of many small construct boxes (a swarm / rubble field): more colliders around
/// the player than the broadphase caches at once must never let the player fall through.
#[test]
fn dense_collider_floor_holds() {
    let p = MoveParams::default();
    let air = FnWorld(|_: IVec3| false);
    let mut cols = Vec::new();
    let tile = 0.3f32;
    for i in 0..24 {
        for k in 0..24 {
            let c = Vec3::new((i as f32 - 12.0) * tile, -0.15, (k as f32 - 12.0) * tile);
            cols.push(Collider::new(c, Vec3::new(tile * 0.5, 0.15, tile * 0.5), Quat::IDENTITY, ColliderId::Other(i * 100 + k)));
        }
    }
    let w = CollisionWorld::new(&air, &cols);
    for (yaw, buttons) in [(0.3f32, 0u16), (2.0, bd_core::input::buttons::JUMP), (-1.2, bd_core::input::buttons::CROUCH)] {
        let mut s = PlayerState::new(Vec3::new(0.05, 0.0, 0.05), &p);
        for t in 0..240 {
            let input = PlayerInput { yaw, move_z: 1, buttons, ..Default::default() };
            let ev = step_player(&mut s, &input, &w, &p, DT);
            if s.pos.x.abs() < 3.0 && s.pos.z.abs() < 3.0 {
                assert!(s.pos.y >= -0.05, "tick {t}: fell through a dense collider floor (y = {})", s.pos.y);
                check_tick(&s, &ev, &air, &cols, &p, t).map_err(|e| format!("{e}")).unwrap();
            }
        }
    }
}

/// Tether attach while moving away fast: the hook reaches a wall just inside `tether_range`, but by
/// then the player is well beyond the range. The rope must attach taut at `tether_range` and reel the
/// player back within range (at the bounded retract speed) instead of attaching slack and long.
#[test]
fn rope_attaches_taut_at_range_when_player_retreats() {
    let p = MoveParams::default();
    // Wall face at z = -119.5 m (just inside range from the hand); no floor, no gravity.
    let world = FnWorld(|v: IVec3| v.z == -240);
    let w = CollisionWorld::new(&world, &[]);
    let mut s = PlayerState::new(Vec3::new(0.0, 10.0, 0.0), &p);
    s.mods.gravity_scale = 0.0;
    s.vel = Vec3::new(0.0, 0.0, 60.0); // flying away from the wall
    let fire = PlayerInput { yaw: 0.0, pitch: 0.0, buttons: bd_core::input::buttons::TETHER_L, ..Default::default() };
    let mut attached_at = None;
    for t in 0..240 {
        let ev = step_player(&mut s, &fire, &w, &p, DT);
        let h = s.hooks[0];
        if ev.tether_attached[0] {
            let dist = (h.pos - s.chest(&p)).length();
            assert!(dist > p.tether_range, "setup: player should be beyond range at attach ({dist})");
            assert!((h.rope_len - p.tether_range).abs() < 1e-3, "rope attached at {} instead of taut at range", h.rope_len);
            attached_at = Some(t);
        }
        if h.attached() {
            assert!(h.rope_len <= p.tether_range + 1e-3, "tick {t}: rope_len {}", h.rope_len);
        }
    }
    assert!(attached_at.is_some(), "hook never attached");
    let h = s.hooks[0];
    assert!(h.attached(), "rope released");
    let dist = (h.pos - s.chest(&p)).length();
    assert!(dist <= p.tether_range + 0.05, "player still {dist} m from the anchor after 2 s");
}
