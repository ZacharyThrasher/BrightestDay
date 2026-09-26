//! Machine kit + costumes: generation safety, caps, linkage kinematics, determinism.

use bd_core::mechanics::{Element, Feature, Form, Mechanic, ModelSource, Recipe};
use bd_shapes::features::{CrankMember, MAX_ANIMATED_PARTS};
use bd_shapes::*;
use glam::{Affine3A, Vec3};
use proptest::prelude::*;

fn recipe(form: Form, scale: f32, mech: &[Mechanic], feats: &[Feature], seed: u32) -> Recipe {
    let mut r = demo_recipe(form, seed, mech);
    r.scale = scale;
    r.features = feats.to_vec();
    r.normalized()
}

fn check(m: &VoxelModel, cap: usize) {
    assert!(m.voxel_count <= cap, "{} > {}", m.voxel_count, cap);
    assert!(m.parts.len() <= MAX_ANIMATED_PARTS + 2, "{} parts", m.parts.len());
    assert_eq!(m.machine.motions.len(), m.parts.len());
    assert_eq!(m.instances.len(), m.parts.len());
    for (i, inst) in m.instances.iter().enumerate() {
        assert!(inst.source <= i && inst.offset.is_finite());
        assert_eq!(m.parts[inst.source].voxels.len(), m.parts[i].voxels.len());
    }
    for e in &m.machine.puffs {
        assert!(e.part_index < m.parts.len() && e.pos.is_finite() && e.dir.is_finite());
    }
}

fn drive_poses(m: &VoxelModel, t: f32, speed: f32, dist: f32) {
    let mut out = vec![Affine3A::IDENTITY; m.parts.len()];
    for layers in [
        vec![(AnimState::Locomotion { speed, distance: dist, airborne: false }, t)],
        vec![(AnimState::Idle, t), (AnimState::Aim { yaw: t, pitch: speed }, t)],
        vec![(AnimState::Locomotion { speed, distance: dist, airborne: true }, t), (AnimState::Attack(Attack::Charge), t)],
    ] {
        pose_layered(m, &layers, &mut out);
        assert!(out.iter().all(|a| a.is_finite()));
    }
    let mut puffs = Vec::new();
    let prev = FeatureDrive { time: t, speed, distance: dist };
    let now = FeatureDrive { time: t + 0.1, speed, distance: dist + speed * 0.1 };
    machine_puffs(m, &out, &prev, &now, &mut puffs);
    assert!(puffs.iter().all(|p| p.pos.is_finite() && p.dir.is_finite() && p.size.is_finite()));
}

#[test]
fn every_feature_on_every_form() {
    for f in ALL_FORMS {
        for feat in ALL_FEATURES {
            let (_, m) = build_model(&recipe(f, suggested_scale(f), &[], &[feat], 2));
            check(&m, voxel_cap(f));
            drive_poses(&m, 0.7, 4.0, 3.3);
        }
        let (_, m) = build_model(&recipe(f, suggested_scale(f), &[Mechanic::Piloted, Mechanic::Flies], &ALL_FEATURES, 5));
        check(&m, voxel_cap(f));
        drive_poses(&m, 1.1, 7.0, 12.0);
    }
}

#[test]
fn steam_train_is_a_locomotive() {
    use Feature::*;
    let r = recipe(Form::Vehicle, 12.0, &[Mechanic::Charge, Mechanic::Fast, Mechanic::Heavy], &[Boiler, Cab, Pistons, Plow, Smokestack, Wheels], 7);
    let (_, m) = build_model(&r);
    check(&m, MAX_VOXELS);
    let cranks: Vec<(usize, CrankMember, usize)> = m
        .machine
        .motions
        .iter()
        .enumerate()
        .filter_map(|(i, x)| if let Motion::Crank { wheel, member, .. } = x { Some((i, *member, *wheel)) } else { None })
        .collect();
    // Two sides × (coupling rod, connecting rod, piston).
    assert_eq!(cranks.len(), 6, "{cranks:?}");
    let drivers: std::collections::BTreeSet<usize> = cranks.iter().map(|c| c.2).collect();
    assert_eq!(drivers.len(), 2);
    let main = *drivers.iter().next().unwrap();
    let r_main = if let Motion::Spin { roll_k, .. } = m.machine.motions[main] { 1.0 / roll_k } else { 0.0 };
    let same = m.machine.motions.iter().filter(|x| matches!(x, Motion::Spin { roll_k, .. } if *roll_k > 0.0 && (1.0 / roll_k - r_main).abs() < 1e-3)).count();
    assert_eq!(same, 6, "six coupled drivers");
    assert!(!m.machine.puffs.is_empty());
    // Crank-slider: the connecting rod keeps its length and the crosshead stays on its line.
    let mut out = vec![Affine3A::IDENTITY; m.parts.len()];
    for k in 0..40 {
        let d = k as f32 * 0.137;
        pose(&m, AnimState::Locomotion { speed: 6.0, distance: d, airborne: false }, d / 6.0, &mut out);
        for (i, member, _) in &cranks {
            if let Motion::Crank { pin, cross, dir, len, .. } = m.machine.motions[*i] {
                match member {
                    CrankMember::Rod => {
                        let a = out[*i].transform_point3(pin);
                        let b = out[*i].transform_point3(cross);
                        assert!(((b - a).length() - len).abs() < 1e-3);
                        let off = b - cross;
                        assert!((off - dir * off.dot(dir)).length() < 1e-3, "crosshead left its line");
                    }
                    CrankMember::Piston => {
                        let off = out[*i].transform_point3(cross) - cross;
                        assert!((off - dir * off.dot(dir)).length() < 1e-3);
                    }
                    CrankMember::Coupling => {}
                }
            }
        }
    }
    // Chuffs: 4 per driver revolution.
    let rev = std::f32::consts::TAU * r_main;
    let mut puffs = Vec::new();
    rest_pose(&m, &mut out);
    let a = FeatureDrive { time: 0.0, speed: 6.0, distance: 0.0 };
    let b = FeatureDrive { time: 0.0, speed: 6.0, distance: rev * 0.99 };
    machine_puffs(&m, &out, &a, &b, &mut puffs);
    assert!(puffs.iter().filter(|p| p.emitter == 0).count() >= 3);
}

#[test]
fn gears_counter_rotate_and_wheels_share_meshes() {
    let (_, m) = build_model(&recipe(Form::Quadruped, 4.0, &[], &[Feature::Gears], 1));
    let signs: Vec<f32> = m.machine.motions.iter().filter_map(|x| if let Motion::Spin { sign, surf_k, .. } = x { (*surf_k != 0.0).then_some(*sign) } else { None }).collect();
    assert!(signs.contains(&1.0) && signs.contains(&-1.0), "{signs:?}");
    let (_, t) = build_model(&recipe(Form::Vehicle, 12.0, &[], &[Feature::Boiler, Feature::Wheels], 7));
    assert!(t.instances.iter().enumerate().filter(|(i, s)| s.source != *i).count() >= 4);
}

#[test]
fn archetypes_move() {
    use Feature::*;
    for (feats, want) in [(&[Rotor][..], "rotor"), (&[Treads, Turret, Barrels][..], "tank"), (&[Sails, Mast][..], "ship"), (&[Jet, Fins][..], "plane"), (&[Propeller, Fins][..], "sub")] {
        let (_, m) = build_model(&recipe(Form::Vehicle, 6.0, &[], feats, 3));
        let moving = m.machine.motions.iter().filter(|x| **x != Motion::None).count();
        assert!(moving >= 1, "{want}: nothing moves");
        if want == "tank" {
            assert!(m.machine.motions.iter().any(|x| matches!(x, Motion::Aim { pitch: false })));
            assert!(m.machine.motions.iter().any(|x| matches!(x, Motion::Scroll { .. })));
        }
    }
}

// ------------------------------------------------------------------------------------ costumes

const NAMES: [&str; 16] = [
    "knight", "fox", "robot pirate", "toaster", "eeyore", "goku", "wizard", "astronaut", "cactus", "pikachu", "bee", "octopus", "tv head", "skeleton", "mushroom", "zorblax",
];

fn check_costume(spec: &bd_core::shape::ShapeSpec) {
    let b = spec_bounds(spec);
    let s = (b.max - b.min + Vec3::ONE) * COSTUME_UNIT;
    assert!(s.x * s.y * s.z <= 0.8 * 1.8 * 0.8 * 1.2 * 1.001, "costume volume {s:?}");
    let m = costume_model(spec);
    assert!(m.voxel_count > 200 && m.voxel_count <= bd_shapes::costume::MAX_COSTUME_VOXELS);
    assert_eq!(m.rig.gait, Gait::Biped);
    assert!(m.rig.has_arms && m.rig.has_head);
    let mut out = vec![Affine3A::IDENTITY; m.parts.len()];
    for (a, t) in [
        (AnimState::Idle, 0.4),
        (AnimState::Locomotion { speed: 8.0, distance: 3.0, airborne: false }, 0.4),
        (AnimState::Locomotion { speed: 8.0, distance: 3.0, airborne: true }, 0.4),
        (AnimState::Slide { speed: 9.0 }, 0.2),
        (AnimState::TetherFire { side: 1 }, 0.1),
        (AnimState::Swing { side: -1, lean: 0.3 }, 1.0),
        (AnimState::Cast, 0.15),
        (AnimState::HitReact { dir: Vec3::X, strength: 1.0 }, 0.05),
        (AnimState::Tumble { spin: 9.0 }, 0.3),
        (AnimState::Aim { yaw: 0.5, pitch: 0.3 }, 0.0),
    ] {
        pose(&m, a, t, &mut out);
        assert!(out.iter().all(|x| x.is_finite()));
    }
    let hands = costume_first_person_hands(spec);
    assert_eq!(hands.parts.len(), 2);
    assert!(!hands.prims.is_empty());
}

#[test]
fn costumes_fit_and_animate() {
    let forms = [Form::Biped, Form::Quadruped, Form::Fish, Form::Insect, Form::Blob, Form::Kaiju, Form::Cannon, Form::Pillar, Form::Hammer, Form::Vehicle, Form::Blob0, Form::Bird];
    let ms = [Mechanic::Flies, Mechanic::Blade, Mechanic::Guard, Mechanic::Charge];
    for (i, n) in NAMES.iter().enumerate() {
        for f in forms {
            let mut r = recipe(f, 1.8, &ms[..(i % 5)], &ALL_FEATURES[..(i * 2) % 32], i as u32);
            r.name = n.to_string();
            let spec = costume(&r, 3, CostumeTweaks::default());
            check_costume(&spec);
            assert_eq!(spec, costume(&r, 3, CostumeTweaks::default()));
        }
    }
    // Head donor + everything hidden + big head.
    let body = recipe(Form::Biped, 1.8, &[Mechanic::Blade], &[Feature::Armor], 1);
    let mut fox = recipe(Form::Quadruped, 1.0, &[], &[], 9);
    fox.name = "fox".into();
    let spec = costume_with(&body, None, Some(CostumeHead { recipe: &fox, shape: None }), 0, CostumeTweaks { accent: Some([1.0, 0.5, 0.0]), head_scale: 1.35, hide_accessories: acc::ALL });
    check_costume(&spec);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, .. ProptestConfig::default() })]

    #[test]
    fn any_machine_is_safe(form in 0usize..39, feats in any::<u32>(), mech in any::<u64>(), scale in prop_oneof![0.1f32..60.0, Just(f32::NAN), Just(1e9)], seed in any::<u32>(), t in -10.0f32..1e4, speed in -5.0f32..80.0, dist in -1e4f32..1e5) {
        let f = ALL_FORMS[form];
        let fs: Vec<Feature> = ALL_FEATURES.iter().copied().enumerate().filter(|(i, _)| feats & (1 << i) != 0).map(|(_, x)| x).collect();
        let ms: Vec<Mechanic> = [Mechanic::Flies, Mechanic::Piloted, Mechanic::Charge, Mechanic::Shoots, Mechanic::Walks, Mechanic::Thrust, Mechanic::Heavy].into_iter().enumerate().filter(|(i, _)| mech & (1 << i) != 0).map(|(_, x)| x).collect();
        let mut r = recipe(f, 1.0, &ms, &fs, seed);
        r.scale = scale;
        let (s1, m) = build_model(&r);
        check(&m, voxel_cap(f));
        prop_assert_eq!(&s1, &generate(&r));
        prop_assert_eq!(&m, &build_model(&r).1);
        drive_poses(&m, t, speed, dist);
        let _ = collision_proxies(&m);
    }

    #[test]
    fn any_costume_is_safe(form in 0usize..39, name in 0usize..16, feats in any::<u32>(), mech in any::<u64>(), seed in any::<u32>(), elem in 0usize..11, icon in any::<bool>(), hs in prop_oneof![0.0f32..3.0, Just(f32::NAN)], hide in any::<u32>()) {
        let f = ALL_FORMS[form];
        let fs: Vec<Feature> = ALL_FEATURES.iter().copied().enumerate().filter(|(i, _)| feats & (1 << i) != 0).map(|(_, x)| x).collect();
        let all_m = [Mechanic::Flies, Mechanic::Glide, Mechanic::Blade, Mechanic::Guard, Mechanic::Shoots, Mechanic::Pull, Mechanic::Grab, Mechanic::Charge, Mechanic::Fast, Mechanic::Heavy, Mechanic::Thrust, Mechanic::Explodes, Mechanic::Bounce, Mechanic::Pierce];
        let ms: Vec<Mechanic> = all_m.into_iter().enumerate().filter(|(i, _)| mech & (1 << i) != 0).map(|(_, x)| x).collect();
        let mut r = recipe(f, 2.0, &ms, &fs, seed);
        r.name = NAMES[name].to_string();
        r.element = [Element::Light, Element::Fire, Element::Electric, Element::Ice, Element::Water, Element::Wind, Element::Earth, Element::Poison, Element::Psychic, Element::Shadow, Element::Atomic][elem];
        let icon_spec = if icon { r.model = ModelSource::Icon(format!("i{seed}")); Some(generate(&demo_recipe(Form::Kaiju, seed, &[]))) } else { None };
        let tw = CostumeTweaks { accent: None, head_scale: hs, hide_accessories: hide };
        let spec = costume_with(&r, icon_spec.as_ref(), None, seed, tw);
        check_costume(&spec);
        prop_assert_eq!(&spec, &costume_with(&r, icon_spec.as_ref(), None, seed, tw));
    }
}

#[test]
fn icons_normalise_to_real_size() {
    // The same icon authored at 1× and 3.7× units must build identically (same density, same size).
    for (form, scale) in [(Form::Biped, 1.8f32), (Form::Hammer, 1.0), (Form::Vehicle, 6.0), (Form::Kaiju, 30.0), (Form::Dome, 3.0)] {
        let base = generate(&demo_recipe(form, 3, &[]));
        let mut big = base.clone();
        scale_spec(&mut big, 3.7);
        let mut r = demo_recipe(form, 3, &[]);
        r.scale = scale;
        r.model = ModelSource::Icon("x".into());
        let (s1, m1) = build_icon_model(&r, &base);
        let (_, m2) = build_icon_model(&r, &big);
        assert!((m1.voxel_size - m2.voxel_size).abs() < 1e-6);
        let want = quantize_scale(scale) * if form == Form::Dome { 2.0 } else { 1.0 };
        let e = icon_extent(form, &s1).unwrap() * bd_core::CONSTRUCT_VOXEL_SIZE;
        assert!((e - want).abs() < 0.02 * want, "{form:?} extent {e} vs {want}");
        let ratio = m1.voxel_count as f32 / m2.voxel_count.max(1) as f32;
        assert!((0.85..1.18).contains(&ratio), "{form:?} {} vs {}", m1.voxel_count, m2.voxel_count);
        assert!(m1.voxel_count <= voxel_cap(form));
        if want <= FINE_ICON_MAX_SCALE {
            assert_eq!(m1.voxel_size, FINE_ICON_VOXEL);
        }
    }
}
