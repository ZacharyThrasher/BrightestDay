//! Transformers: panel split, seamless robot swap, sequence sanity, combiner, authored modes.

use std::collections::HashSet;

use bd_core::mechanics::{Feature, Form, Mechanic};
use bd_shapes::*;
use glam::{Affine3A, IVec3};
use proptest::prelude::*;

fn alt(form: Form, scale: f32, feats: &[Feature], seed: u32) -> VoxelModel {
    let mut r = demo_recipe(form, seed, &[Mechanic::Transforms]);
    r.scale = scale;
    r.features = feats.to_vec();
    build_model(&r.normalized()).1
}

fn check_rig(rig: &TransformRig) {
    assert_eq!(rig.panels.len(), rig.alt.parts.len());
    assert!(rig.panels.len() >= 8, "{} panels", rig.panels.len());
    assert_eq!(rig.robot.rig.gait, Gait::Biped);
    assert!(rig.robot.rig.has_arms && rig.robot.rig.has_head);
    // No floaters: every robot part is one connected piece and meets the torso (joint gap ≤ 1).
    let torso: HashSet<IVec3> = rig.robot.parts[0].voxels.iter().map(|(v, _)| *v).collect();
    for p in &rig.robot.parts {
        let v: Vec<IVec3> = p.voxels.iter().map(|(v, _)| *v).collect();
        assert_eq!(component_count(&v), 1, "robot part {:?} is in {} pieces", p.role, component_count(&v));
        if p.id != 0 {
            let near = v.iter().any(|a| (-2..=2).any(|x| (-2..=2).any(|y| (-2..=2).any(|z| torso.contains(&(*a + IVec3::new(x, y, z)))))));
            assert!(near, "robot part {:?} floats", p.role);
        }
    }
    // Seamless swap: every panel voxel lands exactly on a robot voxel at t = 1.
    let robot: HashSet<IVec3> = rig.robot.parts.iter().flat_map(|p| p.voxels.iter().map(|(v, _)| *v)).collect();
    let mut out = vec![Affine3A::IDENTITY; rig.alt.parts.len()];
    pose_transform(rig, 1.0, true, &mut out);
    let vs = rig.alt.voxel_size;
    for (i, p) in rig.alt.parts.iter().enumerate() {
        if rig.panels[i].scale_mode < 0 {
            continue;
        }
        for (v, _) in &p.voxels {
            let w = out[i].transform_point3(v.as_vec3() * vs) / vs;
            let r = w.round().as_ivec3();
            assert!((w - r.as_vec3()).length() < 1e-3 && robot.contains(&r), "panel {i} voxel off-grid");
        }
    }
    // t = 0 is the alt model exactly (robot-only pieces are scaled to nothing).
    pose_transform(rig, 0.0, true, &mut out);
    for (i, a) in out.iter().enumerate() {
        if rig.panels[i].scale_mode == 0 {
            assert!(a.abs_diff_eq(Affine3A::IDENTITY, 1e-5));
        } else if rig.panels[i].scale_mode == 1 {
            assert!(a.matrix3.x_axis.length() < 0.01);
        }
    }
    for k in 0..=24 {
        pose_transform(rig, k as f32 / 24.0, k % 2 == 0, &mut out);
        assert!(out.iter().all(|a| a.is_finite()));
    }
    let ev = sequence_events(rig);
    assert!(ev.windows(2).all(|w| w[0].time <= w[1].time));
    assert!(ev.iter().all(|e| (0.0..=1.0).contains(&e.time) && e.pos.is_finite()));
    let mut p = vec![Affine3A::IDENTITY; rig.robot.parts.len()];
    pose(&rig.robot, AnimState::Locomotion { speed: 5.0, distance: 2.0, airborne: false }, 0.4, &mut p);
    assert!(p.iter().all(|a| a.is_finite()));
}

#[test]
fn vehicles_and_creatures_transform() {
    use Feature::*;
    for (form, scale, feats) in [
        (Form::Vehicle, 7.0, vec![Wheels, Cab]),
        (Form::Vehicle, 6.0, vec![Treads, Turret, Barrels]),
        (Form::Vehicle, 7.0, vec![Jet, Fins]),
        (Form::Vehicle, 5.0, vec![Rotor]),
        (Form::Fish, 5.0, vec![]),
        (Form::Quadruped, 3.0, vec![]),
        (Form::Hammer, 2.4, vec![]),
        (Form::Ball, 1.5, vec![]),
    ] {
        let rig = transform_rig(&alt(form, scale, &feats, 3), 1);
        check_rig(&rig);
        assert_eq!(rig, transform_rig(&alt(form, scale, &feats, 3), 1), "deterministic");
    }
}

#[test]
fn combiner_and_authored() {
    use Feature::*;
    let rigs: Vec<TransformRig> = [(vec![Wheels, Cab], 2), (vec![Wheels], 4), (vec![Jet], 2), (vec![Treads], 5), (vec![Wheels, Exhaust], 6)]
        .into_iter()
        .map(|(f, s)| transform_rig(&alt(Form::Vehicle, 6.0, &f, s), 1))
        .collect();
    for n in 3..=5 {
        let c = combine_rigs(&rigs[..n]);
        assert_eq!(c.giant.rig.gait, Gait::Biped);
        assert!(c.giant.voxel_count <= MAX_VOXELS_KAIJU);
        let mut out = vec![Affine3A::IDENTITY; n];
        for k in 0..=20 {
            pose_combine(&c, k as f32 / 20.0, &mut out);
            assert!(out.iter().all(|a| a.is_finite()));
        }
        pose_combine(&c, 1.0, &mut out);
        assert_eq!(out, c.placements);
    }
    // Two-mode icon: robot spec = alt spec with parts moved/rotated.
    let a = generate(&demo_recipe(Form::Biped, 1, &[]));
    let mut b = a.clone();
    for p in &mut b.prims {
        p.center.y += 2.0;
    }
    let rig = transform_rig_authored(&a, &b, MAX_VOXELS);
    assert!(rig.panels.len() >= 2);
    let mut out = vec![Affine3A::IDENTITY; rig.alt.parts.len()];
    pose_transform(&rig, 0.5, true, &mut out);
    assert!(out.iter().all(|x| x.is_finite()));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, .. ProptestConfig::default() })]
    #[test]
    fn any_construct_transforms(form in 0usize..39, feats in any::<u32>(), seed in any::<u32>(), scale in 0.5f32..20.0, t in -2.0f32..3.0) {
        let fs: Vec<Feature> = ALL_FEATURES.iter().copied().enumerate().filter(|(i, _)| feats & (1 << i) != 0).map(|(_, x)| x).collect();
        let m = alt(ALL_FORMS[form], scale, &fs, seed);
        let rig = transform_rig(&m, seed);
        prop_assert_eq!(rig.robot.rig.gait, Gait::Biped);
        let mut out = vec![Affine3A::IDENTITY; rig.alt.parts.len()];
        pose_transform(&rig, t, true, &mut out);
        prop_assert!(out.iter().all(|a| a.is_finite()));
        pose_transform(&rig, f32::NAN, false, &mut out);
        prop_assert!(out.iter().all(|a| a.is_finite()));
    }
}
