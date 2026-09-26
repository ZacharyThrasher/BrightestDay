use bd_core::mechanics::{Form, FormCategory, Mechanic, ModelSource};
use bd_core::shape::{Part, PartRole, Prim, PrimKind, ShapeSpec, Tint};
use bd_shapes::*;
use glam::{Affine3A, Vec3};
use proptest::prelude::*;

const ALL_MECHANICS: [Mechanic; 39] = {
    use Mechanic::*;
    [
        Blunt, Blade, Pierce, Guard, Returning, Ricochet, Shoots, Beam, Explodes, Grab, Pull, SlowFall, Glide, Thrust, Bounce, Launch,
        Barrier, Trap, Homing, ArticulatedTail, Flies, Charge, Stomp, Breath, Swarm, Spin, DropFromSky, Sticky, Fast, Heavy,
        Constrict, Walks, Piloted, Portal, Attract, Repel, Shrink, Grow, Link,
    ]
};

fn check_model(m: &VoxelModel, cap: usize) {
    assert!(m.voxel_count <= cap, "{} > cap {}", m.voxel_count, cap);
    assert_eq!(m.voxel_count, m.parts.iter().map(|p| p.voxels.len()).sum::<usize>());
    assert!(!m.parts.is_empty() && m.parts[0].id == 0 && m.parts[0].role.is_none());
    let mut ids = std::collections::BTreeSet::new();
    for (i, p) in m.parts.iter().enumerate() {
        assert!(ids.insert(p.id), "duplicate part id {}", p.id);
        if i > 0 {
            assert!(p.parent_index < i, "parent after child");
            assert_eq!(m.parts[p.parent_index].id, p.parent);
        }
        assert!(p.pivot.is_finite());
    }
    assert!(m.bounds.min.is_finite() && m.bounds.max.is_finite());
    assert!(m.voxel_size >= 0.25 && m.voxel_size.is_finite());
}

fn check_pose(m: &VoxelModel, a: AnimState, t: f32) {
    let mut out = vec![Affine3A::IDENTITY; m.parts.len()];
    pose(m, a, t, &mut out);
    assert!(out.iter().all(|x| x.is_finite()), "non-finite pose {a:?} t={t}");
}

fn anims() -> Vec<AnimState> {
    vec![
        AnimState::Idle,
        AnimState::Locomotion { speed: 4.0, distance: 7.3, airborne: false },
        AnimState::Locomotion { speed: 12.0, distance: 55.0, airborne: true },
        AnimState::Attack(Attack::Bite),
        AnimState::Attack(Attack::Swipe),
        AnimState::Attack(Attack::Charge),
        AnimState::Attack(Attack::Stomp),
        AnimState::Attack(Attack::TailSweep),
        AnimState::Attack(Attack::Breath { aim: Vec3::new(0.3, -0.2, -1.0) }),
        AnimState::Attack(Attack::Lash),
        AnimState::HitReact { dir: Vec3::X, strength: 1.0 },
        AnimState::Forming { duration: 0.6 },
        AnimState::Dismiss { duration: 0.4 },
    ]
}

#[test]
fn every_form_generates_nonempty_within_caps() {
    for f in ALL_FORMS {
        for seed in 0..6 {
            let (spec, m) = build_model(&demo_recipe(f, seed, &[]));
            assert!(!spec.prims.is_empty(), "{f:?}");
            assert!(m.voxel_count > 0, "{f:?} seed {seed} empty");
            check_model(&m, voxel_cap(f));
            for a in anims() {
                check_pose(&m, a, 0.37);
            }
            let meshes = mesh_model(&m);
            for (p, mesh) in m.parts.iter().zip(&meshes) {
                assert_eq!(p.voxels.is_empty(), mesh.is_empty());
                assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.positions.len()));
            }
        }
    }
}

#[test]
fn every_mechanic_attaches_to_every_form() {
    for f in ALL_FORMS {
        for m in ALL_MECHANICS {
            let (_, model) = build_model(&demo_recipe(f, 3, &[m]));
            check_model(&model, voxel_cap(f));
        }
        let (_, model) = build_model(&demo_recipe(f, 9, &ALL_MECHANICS));
        check_model(&model, voxel_cap(f));
    }
}

#[test]
fn deterministic() {
    for f in ALL_FORMS {
        let r = demo_recipe(f, 42, &[Mechanic::Flies, Mechanic::Charge]);
        let (s1, m1) = build_model(&r);
        let (s2, m2) = build_model(&r);
        assert_eq!(s1, s2);
        assert_eq!(m1, m2);
        let mut a = vec![Affine3A::IDENTITY; m1.parts.len()];
        let mut b = a.clone();
        pose(&m1, AnimState::Locomotion { speed: 5.0, distance: 3.3, airborne: false }, 1.2, &mut a);
        pose(&m2, AnimState::Locomotion { speed: 5.0, distance: 3.3, airborne: false }, 1.2, &mut b);
        assert_eq!(a, b);
        assert_eq!(collision_proxies(&m1), collision_proxies(&m2));
        assert_eq!(ShapeKey::of(&r), ShapeKey::of(&r.clone()));
    }
}

#[test]
fn seeds_vary_proportions() {
    for f in [Form::Sword, Form::Quadruped, Form::Kaiju, Form::Hammer, Form::Biped] {
        let a = generate(&demo_recipe(f, 1, &[]));
        let distinct = (2..8).filter(|s| generate(&demo_recipe(f, *s, &[])) != a).count();
        assert!(distinct >= 5, "{f:?} seeds barely vary");
    }
}

#[test]
fn creature_rigs() {
    let gait = |f: Form, seed: u32| build_model(&demo_recipe(f, seed, &[])).1.rig.gait;
    assert_eq!(gait(Form::Biped, 1), Gait::Biped);
    assert_eq!(gait(Form::Kaiju, 1), Gait::Biped);
    assert_eq!(gait(Form::Quadruped, 1), Gait::Quadruped);
    assert_eq!(gait(Form::Serpent, 1), Gait::Serpent);
    assert_eq!(gait(Form::Fish, 1), Gait::Fish);
    assert_eq!(gait(Form::Arachnid, 1), Gait::Octopod);
    assert_eq!(gait(Form::Insect, 1), Gait::Hexapod);
    assert_eq!(gait(Form::Blob, 1), Gait::Hopper);
    assert_eq!(gait(Form::Hammer, 1), Gait::Static);
    let k = build_model(&demo_recipe(Form::Kaiju, 1, &[])).1;
    assert!(k.rig.tails.iter().any(|c| c.len() >= 6), "kaiju tail must be multi-segment");
    assert!(k.rig.has_jaw && k.rig.has_head && k.rig.has_arms);
    assert!(k.weak_points.iter().any(|w| w.kind == WeakKind::Nape));
    let wall = build_model(&demo_recipe(Form::Wall, 1, &[Mechanic::Walks])).1;
    assert!(matches!(wall.rig.gait, Gait::Biped | Gait::Quadruped));
    let hammer = build_model(&demo_recipe(Form::Hammer, 1, &[Mechanic::Flies])).1;
    assert!(hammer.rig.has_wings);
}

#[test]
fn piloted_has_seat_and_cockpit_weak_point() {
    for f in [Form::Biped, Form::Quadruped, Form::Vehicle, Form::Kaiju, Form::Wall] {
        let m = build_model(&demo_recipe(f, 1, &[Mechanic::Piloted])).1;
        let seat = m.seat.expect("seat");
        assert!(seat.pos.is_finite());
        assert!(m.weak_points.iter().any(|w| w.kind == WeakKind::Cockpit));
        let mut p = vec![Affine3A::IDENTITY; m.parts.len()];
        pose(&m, AnimState::Locomotion { speed: 3.0, distance: 2.0, airborne: false }, 0.5, &mut p);
        assert!(m.seat_transform(&p).is_some_and(|t| t.is_finite()));
    }
}

#[test]
fn colossal_constructs_coarsen_voxels() {
    let mut r = demo_recipe(Form::Kaiju, 1, &[]);
    r.scale = 60.0;
    let m = build_model(&r).1;
    assert!(m.voxel_size > 0.25, "voxel size {}", m.voxel_size);
    check_model(&m, MAX_VOXELS_KAIJU);
    let mut ball = demo_recipe(Form::Ball, 1, &[]);
    ball.scale = 50.0;
    let m = build_model(&ball).1;
    assert!(m.voxel_size >= 1.0);
    check_model(&m, MAX_VOXELS);
    assert_ne!(ShapeKey::of(&ball), ShapeKey::of(&demo_recipe(Form::Ball, 1, &[])));
}

#[test]
fn collision_proxies_bounded() {
    for f in ALL_FORMS {
        let m = build_model(&demo_recipe(f, 2, &[])).1;
        let px = collision_proxies(&m);
        let budget = if f == Form::Kaiju { 24 } else { 12 };
        let solid_parts = m.parts.iter().filter(|p| !p.voxels.is_empty()).count();
        assert!(px.len() <= budget.max(solid_parts), "{f:?}: {} proxies", px.len());
        assert!(!px.is_empty());
        for p in px {
            match p.shape {
                Proxy::Obb { center, half, rot } => assert!(center.is_finite() && half.is_finite() && rot.is_finite()),
                Proxy::Capsule { a, b, radius } => assert!(a.is_finite() && b.is_finite() && radius.is_finite()),
            }
        }
    }
}

#[test]
fn swing_curves_sane() {
    for f in ALL_FORMS.iter().copied().filter(|f| f.category() == FormCategory::Held) {
        for kind in [SwingKind::Primary, SwingKind::Heavy, SwingKind::Combo(0), SwingKind::Combo(1), SwingKind::Combo(2), SwingKind::Secondary, SwingKind::Throw] {
            let c = swing_curve(f, kind);
            let (a0, a1) = c.active_window();
            assert!(c.anticipation >= 0.0 && c.active >= 0.0 && c.recovery > 0.0, "{}", c.name);
            assert!(a0 <= a1 && a1 <= c.duration() && c.duration() < 1.6, "{}", c.name);
            assert!(c.hold_at <= a1 + 1e-6);
            assert!(c.keys.windows(2).all(|w| w[0].t < w[1].t), "{} keys unsorted", c.name);
            assert!((c.keys.last().unwrap().t - c.duration()).abs() < 1e-3, "{} last key != duration", c.name);
            for i in 0..=40 {
                assert!(c.sample(c.duration() * i as f32 / 40.0).is_finite());
            }
            // Ends where it started (ready pose) so chaining swings doesn't pop.
            let d = (c.sample(0.0).translation - c.sample(c.duration()).translation).length();
            assert!(d < 1e-3, "{} doesn't return to ready", c.name);
            assert!(c.sweep(a0, a1, 0.15).count() >= 1);
        }
    }
    let h = swing_curve(Form::Hammer, SwingKind::Primary);
    assert_eq!((h.anticipation, h.active, h.recovery), (0.07, 0.09, 0.16));
    // The strike arc actually moves a lot during the active window.
    let ang = h.rotation(0.07).angle_between(h.rotation(0.16));
    assert!(ang > 2.0, "hammer active arc only {ang} rad");
}

#[test]
fn portal_and_chain_shapes() {
    let m = voxelize(&portal_ring_spec(3.0));
    assert!(m.voxel_count > 20);
    assert!(m.parts[0].voxels.iter().any(|(_, t)| *t == Tint::Core));
    let mesh = chain_link_mesh(4.0);
    assert!(!mesh.is_empty());
    assert!(chain_transform(Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0), 4.0).is_finite());
    assert!(chain_transform(Vec3::ZERO, Vec3::ZERO, 4.0).is_finite());
}

// ------------------------------------------------------------------------------ property tests

fn any_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        -100.0f32..100.0,
        Just(f32::NAN),
        Just(f32::INFINITY),
        Just(f32::NEG_INFINITY),
        Just(0.0),
        Just(1e9),
        Just(-1e9),
        0.0f32..5.0,
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 160, .. ProptestConfig::default() })]

    #[test]
    fn any_recipe_is_safe(form in 0usize..39, mech in any::<u64>(), scale in any_f32(), seed in any::<u32>(), icon in any::<bool>(), t in any_f32(), speed in any_f32()) {
        let f = ALL_FORMS[form];
        let ms: Vec<Mechanic> = ALL_MECHANICS.iter().copied().enumerate().filter(|(i, _)| mech & (1 << i) != 0 && (mech >> 40) & 3 != 0).map(|(_, m)| m).collect();
        let mut r = demo_recipe(f, seed, &ms);
        r.scale = scale;
        r.count = 255;
        if icon { r.model = ModelSource::Icon(format!("icon{seed}")); }
        let (s1, m) = build_model(&r);
        check_model(&m, voxel_cap(f));
        prop_assert_eq!(&s1, &generate(&r));
        for a in anims() { check_pose(&m, a, t); }
        check_pose(&m, AnimState::Locomotion { speed, distance: t, airborne: false }, t);
        check_pose(&m, AnimState::HitReact { dir: Vec3::new(speed, t, 0.0), strength: speed }, t);
        let _ = collision_proxies(&m);
        let _ = mesh_model(&m);
    }

    #[test]
    fn any_spec_is_safe(prims in prop::collection::vec((0u8..4, any_f32(), any_f32(), any_f32(), any_f32(), any_f32(), 0u8..12, any::<bool>()), 0..40),
                        parts in prop::collection::vec((0u8..14, 0u8..14, 0u8..13), 0..16)) {
        let kinds = [PrimKind::Box, PrimKind::Ellipsoid, PrimKind::Cylinder, PrimKind::Cone];
        let roles = [PartRole::Head, PartRole::Jaw, PartRole::Tail, PartRole::LegFrontLeft, PartRole::LegFrontRight, PartRole::LegBackLeft,
                     PartRole::LegBackRight, PartRole::ArmLeft, PartRole::ArmRight, PartRole::WingLeft, PartRole::WingRight, PartRole::Weapon, PartRole::Other];
        let spec = ShapeSpec {
            prims: prims.iter().map(|&(k, cx, cy, sx, sy, rot, part, sub)| Prim {
                kind: kinds[k as usize], center: Vec3::new(cx, cy, cx * 0.5), size: Vec3::new(sx, sy, sx.abs().min(9.0)),
                rot_deg: Vec3::new(rot, 0.0, rot), tint: Tint::Primary, part, subtract: sub && part % 3 == 0,
            }).collect(),
            parts: parts.iter().map(|&(id, parent, role)| Part { id, parent, pivot: Vec3::new(id as f32, parent as f32, 0.0), role: roles[role as usize] }).collect(),
        };
        let mut s2 = spec.clone();
        attach_mechanic_parts(&mut s2, &ALL_MECHANICS, spec_bounds(&spec));
        for sp in [&spec, &s2] {
            let m = voxelize_capped(sp, MAX_VOXELS);
            check_model(&m, MAX_VOXELS);
            for a in anims() { check_pose(&m, a, 0.5); }
            let _ = collision_proxies(&m);
            let _ = mesh_model(&m);
        }
    }
}
