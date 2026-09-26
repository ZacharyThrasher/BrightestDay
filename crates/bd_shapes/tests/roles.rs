//! Construct animation by role: tagging at generation, conservative inference on untagged icons,
//! behaviours stable/bounded under any input, hatch / turret / assembly semantics, grip points.

use bd_core::mechanics::{Feature, Form, Mechanic, Recipe};
use bd_core::shape::{Part, PartRole, Prim, PrimKind, ShapeSpec, Tint};
use bd_shapes::behaviours::{AnimDrive, AnimEvent, ConstructAnimator, CueKind};
use bd_shapes::roles::{Role, analyze};
use bd_shapes::*;
use glam::{Affine3A, Vec3};

fn bx(c: [f32; 3], s: [f32; 3], part: u8) -> Prim {
    Prim {
        kind: PrimKind::Box,
        center: Vec3::from_array(c),
        size: Vec3::from_array(s),
        rot_deg: Vec3::ZERO,
        tint: Tint::Primary,
        part,
        subtract: false,
    }
}

fn cyl_x(c: [f32; 3], d: f32, w: f32, part: u8) -> Prim {
    Prim {
        kind: PrimKind::Cylinder,
        center: Vec3::from_array(c),
        size: Vec3::new(d, w, d),
        rot_deg: Vec3::new(0.0, 0.0, 90.0),
        tint: Tint::Dark,
        part,
        subtract: false,
    }
}

fn part(id: u8, parent: u8, pivot: [f32; 3]) -> Part {
    Part { id, parent, pivot: Vec3::from_array(pivot), role: PartRole::Other }
}

fn machine(form: Form, scale: f32, mech: &[Mechanic], feats: &[Feature]) -> VoxelModel {
    let mut r = demo_recipe(form, 1, mech);
    r.scale = scale;
    r.features = feats.to_vec();
    let r = r.normalized();
    let mut m = build_model(&r).1;
    m.weight = weight_of(&r);
    m
}

fn count(m: &VoxelModel, f: impl Fn(&Role) -> bool) -> usize {
    analyze(m).parts.iter().filter(|p| f(&p.role)).count()
}

/// An untagged hand-authored "tank" icon: hull, turret, gun, hatch lid, 4 road wheels (all `Other`).
fn icon_tank() -> ShapeSpec {
    ShapeSpec {
        prims: vec![
            bx([0.0, 4.0, 0.0], [10.0, 4.0, 16.0], 0),
            bx([0.0, 7.5, 1.0], [6.0, 3.0, 6.0], 1),
            bx([0.0, 7.5, -5.0], [1.0, 1.0, 9.0], 2),
            bx([1.0, 10.0, 2.0], [3.0, 1.0, 3.0], 3),
            cyl_x([-6.0, 2.0, -5.0], 4.0, 1.0, 4),
            cyl_x([6.0, 2.0, -5.0], 4.0, 1.0, 5),
            cyl_x([-6.0, 2.0, 5.0], 4.0, 1.0, 6),
            cyl_x([6.0, 2.0, 5.0], 4.0, 1.0, 7),
        ],
        parts: vec![
            part(1, 0, [0.0, 6.0, 1.0]),
            part(2, 1, [0.0, 7.5, -0.5]),
            part(3, 1, [1.0, 10.0, 3.5]),
            part(4, 0, [-6.0, 2.0, -5.0]),
            part(5, 0, [6.0, 2.0, -5.0]),
            part(6, 0, [-6.0, 2.0, 5.0]),
            part(7, 0, [6.0, 2.0, 5.0]),
        ],
    }
}

#[test]
fn inference_finds_the_tank_parts() {
    let m = voxelize(&icon_tank());
    let r = analyze(&m);
    assert_eq!(r.role(1), Role::Turret, "turret");
    assert!(matches!(r.role(2), Role::Barrel { .. }), "barrel {:?}", r.role(2));
    assert!(matches!(r.role(3), Role::Hatch { .. }), "hatch {:?}", r.role(3));
    for i in 4..8 {
        assert!(matches!(r.role(i), Role::Wheel { .. }), "wheel {i} {:?}", r.role(i));
    }
    // Front wheels of a free-wheeled vehicle steer.
    assert!(matches!(r.role(4), Role::Wheel { steer: true, .. }) && matches!(r.role(6), Role::Wheel { steer: false, .. }));
    assert!(r.parts[1].inferred && r.parts[2].inferred && r.parts[3].inferred);
}

#[test]
fn inference_stays_quiet_when_unsure() {
    // A creature: head + a long thin *tail* (not a barrel), a plate on its back (not a hatch: not a
    // vehicle), one side plate without a mirror (not a fin), a pole standing up (not a barrel).
    let spec = ShapeSpec {
        prims: vec![
            bx([0.0, 6.0, 0.0], [6.0, 4.0, 10.0], 0),
            bx([0.0, 9.0, -6.0], [3.0, 3.0, 3.0], 1),
            bx([0.0, 6.0, 9.0], [1.0, 1.0, 8.0], 2),
            bx([0.0, 8.5, 1.0], [3.0, 1.0, 3.0], 3),
            bx([4.5, 6.0, 0.0], [3.0, 1.0, 4.0], 4),
            bx([0.0, 12.0, 3.0], [1.0, 8.0, 1.0], 5),
            bx([-2.0, 2.0, -3.0], [1.0, 4.0, 1.0], 6),
            bx([2.0, 2.0, -3.0], [1.0, 4.0, 1.0], 7),
            bx([-2.0, 2.0, 3.0], [1.0, 4.0, 1.0], 8),
            bx([2.0, 2.0, 3.0], [1.0, 4.0, 1.0], 9),
        ],
        parts: vec![
            Part { id: 1, parent: 0, pivot: Vec3::new(0.0, 8.0, -4.0), role: PartRole::Head },
            Part { id: 2, parent: 0, pivot: Vec3::new(0.0, 6.0, 5.0), role: PartRole::Tail },
            part(3, 0, [0.0, 8.5, 1.0]),
            part(4, 0, [3.0, 6.0, 0.0]),
            part(5, 0, [0.0, 8.0, 3.0]),
            Part { id: 6, parent: 0, pivot: Vec3::new(-2.0, 4.0, -3.0), role: PartRole::LegFrontLeft },
            Part { id: 7, parent: 0, pivot: Vec3::new(2.0, 4.0, -3.0), role: PartRole::LegFrontRight },
            Part { id: 8, parent: 0, pivot: Vec3::new(-2.0, 4.0, 3.0), role: PartRole::LegBackLeft },
            Part { id: 9, parent: 0, pivot: Vec3::new(2.0, 4.0, 3.0), role: PartRole::LegBackRight },
        ],
    };
    let m = voxelize(&spec);
    let r = analyze(&m);
    assert_eq!(r.role(1), Role::Head);
    for i in [3usize, 4, 5] {
        assert_eq!(r.role(i), Role::None, "part {i} must stay unanimated: {:?}", r.role(i));
    }
    assert!(!r.parts.iter().any(|p| matches!(p.role, Role::Barrel { .. } | Role::Hatch { .. } | Role::Turret | Role::Fin { .. })));
    // A long beam lying on the ground is not a barrel either.
    let beam = ShapeSpec {
        prims: vec![bx([0.0, 2.0, 0.0], [8.0, 4.0, 8.0], 0), bx([0.0, 0.0, -6.0], [1.0, 1.0, 10.0], 1)],
        parts: vec![part(1, 0, [0.0, 0.0, -1.0])],
    };
    assert_eq!(analyze(&voxelize(&beam)).role(1), Role::None);
}

#[test]
fn generated_constructs_are_tagged() {
    let tank = machine(Form::Vehicle, 7.0, &[Mechanic::Piloted], &[Feature::Treads, Feature::Turret]);
    assert_eq!(count(&tank, |r| *r == Role::Turret), 1);
    assert!(count(&tank, |r| matches!(r, Role::Barrel { .. })) >= 1);
    assert!(count(&tank, |r| matches!(r, Role::Hatch { .. })) >= 1, "turret hatch");
    assert!(count(&tank, |r| matches!(r, Role::Wheel { .. })) >= 4);
    assert_eq!(count(&tank, |r| matches!(r, Role::Wheel { steer: true, .. })), 0, "tracked: no steering");
    assert!(!analyze(&tank).parts.iter().any(|p| p.inferred), "tagged models never infer");
    let heli = machine(Form::Vehicle, 9.0, &[Mechanic::Flies], &[Feature::Rotor]);
    assert!(count(&heli, |r| matches!(r, Role::Rotor { machine: true, .. })) >= 2);
    assert!(count(&heli, |r| matches!(r, Role::Hatch { .. })) >= 1, "canopy");
    let car = machine(Form::Vehicle, 5.0, &[], &[Feature::Wheels, Feature::Cab]);
    assert_eq!(count(&car, |r| matches!(r, Role::Wheel { steer: true, .. })), 2, "front wheels steer");
    let mut mech_r = demo_recipe(Form::Biped, 1, &[Mechanic::Piloted]);
    mech_r.scale = 12.0;
    let mech = build_model(&mech_r).1;
    assert!(count(&mech, |r| matches!(r, Role::Hatch { .. })) >= 1, "chest canopy");
    assert!(mech.seat.is_some());
    assert!(count(&mech, |r| *r == Role::Eye) >= 1 && count(&mech, |r| *r == Role::Head) >= 1);
    let sword = build_model(&demo_recipe(Form::Sword, 1, &[])).1;
    assert_eq!(count(&sword, |r| matches!(r, Role::Blade { .. })), 1);
    let shooter = build_model(&demo_recipe(Form::Dome, 1, &[Mechanic::Shoots])).1;
    assert!(count(&shooter, |r| matches!(r, Role::Barrel { .. })) >= 1);
}

#[test]
fn every_held_form_has_a_grip() {
    for f in [Form::Hammer, Form::Sword, Form::Axe, Form::Spear, Form::Shield, Form::Whip, Form::Gun, Form::Bow, Form::Cannon, Form::Staff]
    {
        for seed in 1..4 {
            let m = build_model(&demo_recipe(f, seed, &[])).1;
            let (pos, axis) = grip_point(&m).unwrap_or_else(|| panic!("{f:?} grip"));
            assert!(pos.length() < 0.3, "{f:?} grip at the held origin: {pos:?}");
            assert!((axis.length() - 1.0).abs() < 1e-3, "{f:?} axis {axis:?}");
            assert!(grip_part(&m).is_some(), "{f:?} tags its grip");
        }
    }
    // Shafts run along Z; pistol grips / shield straps / bow grips stand up.
    let sword = build_model(&demo_recipe(Form::Sword, 1, &[])).1;
    assert!(grip_point(&sword).unwrap().1.z.abs() > 0.9);
    let shield = build_model(&demo_recipe(Form::Shield, 1, &[])).1;
    assert!(grip_point(&shield).unwrap().1.y.abs() > 0.9);
    // Two-handers get a support point up the item; one-handers don't.
    assert!(support_point(&build_model(&demo_recipe(Form::Spear, 1, &[])).1).is_some_and(|p| p.z < -0.2));
    assert!(support_point(&build_model(&demo_recipe(Form::Staff, 1, &[])).1).is_some());
    assert!(support_point(&build_model(&demo_recipe(Form::Cannon, 1, &[])).1).is_some());
    assert!(support_point(&sword).is_none());
    // Untagged icon (origin = grip): inferred along its handle.
    let icon = ShapeSpec { prims: vec![bx([0.0, 0.0, -1.0], [1.0, 1.0, 4.0], 0), bx([0.0, 0.0, -5.0], [4.0, 3.0, 4.0], 0)], parts: vec![] };
    let (p, a) = grip_point(&voxelize(&icon)).expect("icon grip");
    assert!(p.length() < 1e-3 && a.z < -0.9, "{p:?} {a:?}");
    let far = ShapeSpec { prims: vec![bx([0.0, 0.0, -20.0], [2.0, 2.0, 2.0], 0)], parts: vec![] };
    assert!(grip_point(&voxelize(&far)).is_none(), "nothing near the origin: not a held item");
}

fn angle_of(l: &Local) -> f32 {
    l.rot.to_axis_angle().1.abs()
}

#[test]
fn hatch_opens_for_the_pilot_and_shuts_with_a_clunk() {
    let tank = machine(Form::Vehicle, 7.0, &[Mechanic::Piloted], &[Feature::Treads, Feature::Turret]);
    let r = analyze(&tank);
    let h = r.parts.iter().position(|p| matches!(p.role, Role::Hatch { .. })).expect("hatch");
    let mut a = ConstructAnimator::new(&tank, 1);
    let dt = 1.0 / 144.0;
    let mut t = 0.0;
    let mut step = |a: &mut ConstructAnimator, secs: f32, near: bool, seated: bool| {
        let mut cues = Vec::new();
        let n = (secs / dt) as usize;
        for _ in 0..n {
            t += dt;
            a.update(&AnimDrive { time: t, pilot_near: near, piloted: seated, ..Default::default() }, dt);
            cues.extend(a.cues().iter().map(|c| c.kind));
        }
        cues
    };
    step(&mut a, 0.5, false, false);
    assert!(angle_of(&a.locals()[h]) < 0.05, "closed at rest");
    let cues = step(&mut a, 1.0, true, false);
    assert!(angle_of(&a.locals()[h]) > 1.2, "open for the approaching pilot: {}", angle_of(&a.locals()[h]));
    assert!(cues.contains(&CueKind::HatchOpen));
    a.event(AnimEvent::PilotEnter);
    let cues = step(&mut a, 0.25, false, true);
    assert!(angle_of(&a.locals()[h]) > 0.5, "stays open while they climb in");
    let more = step(&mut a, 1.0, false, true);
    assert!(angle_of(&a.locals()[h]) < 0.08, "shut once seated");
    assert!(cues.iter().chain(&more).any(|c| *c == CueKind::HatchClose), "clunk");
    a.event(AnimEvent::PilotExit);
    step(&mut a, 0.6, false, false);
    assert!(angle_of(&a.locals()[h]) > 0.8, "reopens on exit");
}

#[test]
fn turret_tracks_the_aim_and_the_barrel_recoils() {
    let tank = machine(Form::Vehicle, 7.0, &[], &[Feature::Treads, Feature::Turret]);
    let r = analyze(&tank);
    let tu = r.parts.iter().position(|p| p.role == Role::Turret).expect("turret");
    let ba = r.parts.iter().position(|p| matches!(p.role, Role::Barrel { .. })).expect("barrel");
    let mut a = ConstructAnimator::new(&tank, 1);
    let aim = Vec3::new(-1.0, 0.0, 0.0); // hard left: +90° yaw
    let dt = 1.0 / 120.0;
    for k in 0..360 {
        a.update(&AnimDrive { time: k as f32 * dt, aim: Some(aim), ..Default::default() }, dt);
    }
    let (axis, ang) = a.locals()[tu].rot.to_axis_angle();
    assert!((ang * axis.y - std::f32::consts::FRAC_PI_2).abs() < 0.1, "turret yaw {} about {axis:?}", ang);
    let rest = a.locals()[ba].pos.length();
    a.event(AnimEvent::Fire { strength: 1.5 });
    let mut peak: f32 = 0.0;
    for k in 0..30 {
        a.update(&AnimDrive { time: 3.0 + k as f32 * dt, aim: Some(aim), ..Default::default() }, dt);
        peak = peak.max(a.locals()[ba].pos.z);
    }
    assert!(peak > rest + 0.05, "recoils back along +Z: {peak}");
    for k in 0..240 {
        a.update(&AnimDrive { time: 3.5 + k as f32 * dt, aim: Some(aim), ..Default::default() }, dt);
    }
    assert!(a.locals()[ba].pos.length() < 0.02, "and returns");
}

#[test]
fn summon_assembles_in_role_order_then_rests() {
    let tank = machine(Form::Vehicle, 7.0, &[Mechanic::Piloted], &[Feature::Treads, Feature::Turret]);
    let mut a = ConstructAnimator::new(&tank, 1);
    let total = a.assembly_seconds();
    assert!((0.3..=0.6).contains(&total), "{total}");
    let r = a.roles.clone();
    let turret = r.parts.iter().position(|p| p.role == Role::Turret).expect("turret");
    let wheel = r.parts.iter().position(|p| matches!(p.role, Role::Wheel { .. })).expect("wheel");
    assert!(r.parts[turret].stage > r.parts[wheel].stage, "wheels before the turret");
    a.event(AnimEvent::Summon);
    let dt = 1.0 / 120.0;
    let mut clicks = 0;
    let mut locks = 0;
    let mut t = 0.0;
    let mut hidden_turret_early = false;
    while t < total + 0.5 {
        t += dt;
        a.update(&AnimDrive { time: t, ring: Some(Vec3::new(3.0, 2.0, 4.0)), ..Default::default() }, dt);
        for c in a.cues() {
            match c.kind {
                CueKind::AssembleClick => clicks += 1,
                CueKind::AssembleLock => locks += 1,
                _ => {}
            }
        }
        if t < 0.02 && a.locals()[turret].scale.x < 0.01 {
            hidden_turret_early = true;
        }
        if t < total {
            assert!(a.assembly().active || t > total - 0.02);
        }
    }
    assert!(hidden_turret_early, "later stages wait their turn");
    assert_eq!(clicks + locks, r.stages as usize, "one cue per stage");
    assert_eq!(locks, 1, "the last stage locks");
    assert!(!a.assembly().active);
    for (i, l) in a.locals().iter().enumerate() {
        assert!((l.scale - Vec3::ONE).abs().max_element() < 0.02, "part {i} settled: {l:?}");
    }
}

#[test]
fn behaviours_are_finite_and_bounded_for_any_input() {
    let mut models = Vec::new();
    for f in ALL_FORMS {
        models.push(build_model(&demo_recipe(f, 1, &[Mechanic::Piloted, Mechanic::Shoots, Mechanic::Blade])).1);
    }
    models.push(machine(Form::Vehicle, 7.0, &[Mechanic::Piloted], &[Feature::Treads, Feature::Turret]));
    models.push(machine(Form::Vehicle, 9.0, &[Mechanic::Flies], &[Feature::Rotor, Feature::Exhaust]));
    models.push(voxelize(&icon_tank()));
    let weird = [0.0, 1.0, -1.0, 1e9, -1e9, f32::NAN, f32::INFINITY, 1e-9];
    let mut s = 12345u32;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    for (mi, m) in models.iter_mut().enumerate() {
        m.weight = [0.6, 1.0, 2.2, f32::NAN][mi % 4];
        let mut a = ConstructAnimator::new(m, mi as u32);
        let mut pose_out = vec![Affine3A::IDENTITY; m.parts.len()];
        for k in 0..400 {
            let w = |i: u32| weird[(i as usize) % weird.len()];
            let x = rnd();
            match x % 9 {
                0 => a.event(AnimEvent::Summon),
                1 => a.event(AnimEvent::Fire { strength: w(x >> 4) }),
                2 => a.event(AnimEvent::Hit { dir: Vec3::new(w(x >> 3), w(x >> 5), w(x >> 7)), strength: w(x >> 9) }),
                3 => a.event(AnimEvent::PilotEnter),
                4 => a.event(AnimEvent::PilotExit),
                5 => a.event(AnimEvent::Attack { roar: x & 1 == 0 }),
                6 if k > 300 => a.event(AnimEvent::Dismiss { duration: w(x >> 2) }),
                _ => {}
            }
            let d = AnimDrive {
                time: if k % 50 == 7 { w(x >> 11) } else { k as f32 * 0.01 },
                speed: w(x >> 1),
                turn: w(x >> 2),
                airborne: x & 4 == 0,
                aim: Some(Vec3::new(w(x >> 6), w(x >> 8), w(x >> 10))),
                look: Some(Vec3::new(w(x >> 12), 0.5, w(x >> 13))),
                pilot_near: x & 8 == 0,
                piloted: x & 16 == 0,
                charge: w(x >> 14),
                ring: Some(Vec3::new(w(x >> 15), 1.0, 2.0)),
            };
            let dt = [1.0 / 144.0, 0.0, 0.1, f32::NAN, -1.0, 1e6][(x >> 20) as usize % 6];
            a.update(&d, dt);
            for l in a.locals() {
                assert!(l.rot.is_finite() && l.pos.is_finite() && l.scale.is_finite(), "{mi} {l:?}");
                assert!(l.pos.abs().max_element() <= 100.0 && l.scale.min_element() > 0.0 && l.scale.max_element() <= 4.0);
            }
            pose_layered_with(
                m,
                &[(AnimState::Locomotion { speed: 3.0, distance: k as f32 * 0.03, airborne: false }, k as f32 * 0.01)],
                Some(a.locals()),
                &mut pose_out,
            );
            assert!(pose_out.iter().all(|p| p.is_finite()));
        }
    }
}

#[test]
fn nominal_models_pose_exactly_as_before_without_an_overlay() {
    // The sim never passes an overlay: `pose_layered_with(.., None, ..)` is `pose_layered`.
    let m = build_model(&demo_recipe(Form::Kaiju, 1, &[])).1;
    let layers = [(AnimState::Locomotion { speed: 4.0, distance: 3.0, airborne: false }, 1.3), (AnimState::Attack(Attack::Stomp), 0.4)];
    let mut a = vec![Affine3A::IDENTITY; m.parts.len()];
    let mut b = a.clone();
    pose_layered(&m, &layers, &mut a);
    pose_layered_with(&m, &layers, None, &mut b);
    assert_eq!(a, b);
    let identity = vec![Local::IDENTITY; m.parts.len()];
    pose_layered_with(&m, &layers, Some(&identity), &mut b);
    for (x, y) in a.iter().zip(&b) {
        assert!(x.abs_diff_eq(*y, 1e-5));
    }
    let _ = Recipe::unknown("x");
}
