//! bd_core v1.6 mount attach point: a rider stands on its mount; the mount's parts keep their roles.

use bd_core::mechanics::{Feature, Form, Mechanic, Recipe};
use bd_core::shape::PartRole;
use bd_shapes::forms::{demo_recipe, generate};
use bd_shapes::spec_bounds;

fn board(scale: f32) -> Recipe {
    let mut b = demo_recipe(Form::Vehicle, 7, &[Mechanic::Fast]);
    b.name = "skateboard".into();
    b.features = vec![Feature::Wheels];
    b.scale = scale;
    b.normalized()
}

#[test]
fn rider_stands_on_its_mount() {
    let mut gorilla = demo_recipe(Form::Biped, 3, &[Mechanic::Grab]);
    gorilla.scale = 1.8;
    let alone = generate(&gorilla);
    let mount_spec = generate(&board(1.4));
    gorilla.mount = Some(Box::new(board(1.4)));
    let riding = generate(&gorilla);

    // everything the mount had is added, nothing of the rider is lost
    assert_eq!(riding.prims.len(), alone.prims.len() + mount_spec.prims.len());
    // the rider was lifted by the mount's height; the combined model still starts at the ground
    let rb = spec_bounds(&riding);
    let ab = spec_bounds(&alone);
    let mb = spec_bounds(&mount_spec);
    assert!(rb.min.y.abs() < 1.0, "combined model should start at the ground, min y = {}", rb.min.y);
    assert!(rb.max.y > ab.max.y + (mb.max.y - mb.min.y) * 0.5, "rider not lifted: {} vs {}", rb.max.y, ab.max.y);
    // wheel parts survive the merge with unique ids
    if mount_spec.parts.iter().any(|p| p.role == PartRole::Wheel) {
        assert!(riding.parts.iter().any(|p| p.role == PartRole::Wheel));
    }
    let mut ids: Vec<_> = riding.parts.iter().map(|p| p.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "part ids must stay unique");
    // a mount's own mount is ignored (one level)
    let mut nested = board(1.0);
    nested.mount = Some(Box::new(board(0.5)));
    let r = Recipe { mount: Some(Box::new(nested)), ..gorilla.clone() }.normalized();
    assert!(r.mount.as_ref().is_some_and(|m| m.mount.is_none()));
}
