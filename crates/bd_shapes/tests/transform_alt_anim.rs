//! TRANSFORMER FEEL: an authored two-mode icon's alt mode keeps its parts' roles / pivots /
//! hierarchy, so a beast alt walks and bites and a car alt's wheels roll once settled — and the
//! transformation sequence itself (panels, voxels, t = 0 identity) is unchanged.

use bd_core::mechanics::Form;
use bd_core::shape::PartRole;
use bd_shapes::*;
use glam::Affine3A;

fn two_mode(form: Form, seed: u32) -> TransformRig {
    let a = generate(&demo_recipe(form, seed, &[]));
    let mut b = a.clone();
    for p in &mut b.prims {
        p.center.y += 2.0;
    }
    transform_rig_authored(&a, &b, MAX_VOXELS)
}

#[test]
fn authored_beast_alt_keeps_its_rig() {
    for (form, seed) in [(Form::Quadruped, 1), (Form::Biped, 2), (Form::Kaiju, 3)] {
        let rig = two_mode(form, seed);
        let legs = rig.alt.parts.iter().filter(|p| matches!(p.role, Some(PartRole::LegBackLeft | PartRole::LegBackRight | PartRole::LegFrontLeft | PartRole::LegFrontRight))).count();
        assert!(legs >= 2, "{form:?}: {legs} legs in the alt model");
        assert_ne!(rig.alt.rig.gait, Gait::Static, "{form:?}");
        // Parents always precede children; no part sways (plates ride rigidly).
        for (i, p) in rig.alt.parts.iter().enumerate().skip(1) {
            assert!(p.parent_index < i, "{form:?}: part {i} parent {}", p.parent_index);
            assert_ne!(p.role, Some(PartRole::Other), "{form:?}: part {i} would sway");
        }
        // A walk moves a leg; the rest pose is identity (continuous with the settled sequence).
        let mut walk = vec![Affine3A::IDENTITY; rig.alt.parts.len()];
        pose(&rig.alt, AnimState::Locomotion { speed: 4.0, distance: 0.7, airborne: false }, 0.3, &mut walk);
        let moved = rig.alt.parts.iter().enumerate().any(|(i, p)| p.role.is_some_and(|r| matches!(r, PartRole::LegBackLeft | PartRole::LegBackRight | PartRole::LegFrontLeft | PartRole::LegFrontRight)) && !walk[i].abs_diff_eq(Affine3A::IDENTITY, 1e-3));
        assert!(moved, "{form:?}: the alt legs don't walk");
        // The sequence is untouched: t = 0 (alt) is the rest pose for every kept panel.
        let mut out = vec![Affine3A::IDENTITY; rig.alt.parts.len()];
        pose_transform(&rig, 0.0, true, &mut out);
        for (i, a) in out.iter().enumerate() {
            if rig.panels[i].scale_mode == 0 {
                assert!(a.abs_diff_eq(Affine3A::IDENTITY, 1e-5), "{form:?}: panel {i}");
            }
        }
    }
}

#[test]
fn procedural_alt_is_unchanged() {
    // Procedural rigs (no authored alt) keep plain plates: nothing to animate in alt mode.
    let mut r = demo_recipe(Form::Vehicle, 3, &[bd_core::mechanics::Mechanic::Transforms]);
    r.scale = 6.0;
    let m = build_model(&r.normalized()).1;
    let rig = transform_rig(&m, 1);
    assert!(rig.alt.parts.iter().skip(1).all(|p| p.role == Some(PartRole::Other)));
}
