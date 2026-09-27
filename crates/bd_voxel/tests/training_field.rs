//! TRAINING FIELD (`generate_training_field`, `--map training`): deterministic, sane arena info,
//! and a raycast-checkable flat grass floor with a 5 m grid.

use bd_core::voxel::{raycast, AIR};
use bd_voxel::*;
use glam::Vec3;

#[test]
fn deterministic() {
    let (a, _) = generate_training_field(140.0);
    let (b, _) = generate_training_field(140.0);
    assert_eq!(a.content_hash(), b.content_hash(), "same size_m must generate identical voxels");
}

#[test]
fn arena_info_is_sane() {
    let (_, info) = generate_training_field(140.0);
    assert!(info.spawns.len() >= 4, "needs room for a real match, not just the dummy");
    for s in &info.spawns {
        assert!(s.is_finite());
    }
    assert!(info.bounds.min.is_finite() && info.bounds.max.is_finite());
    assert!(info.void_y < info.center.y, "void must be below the ground");
    assert!(info.sun_dir.is_finite() && info.sun_dir.length() > 0.9);
    // PRACTICE TOOL: no water / gravity gimmicks / slipstream / map mechanics — cheap and plain.
    assert!(info.water_sheets.is_empty());
    assert!(info.gravity.is_none());
    assert!(info.slipstream.is_none());
}

#[test]
fn flat_grass_floor_with_grid() {
    let (w, info) = generate_training_field(140.0);
    // Straight down near the centre hits the grass top at `center.y`.
    let hit = raycast(&w, Vec3::new(0.3, info.center.y + 20.0, 0.3), Vec3::NEG_Y, 60.0).expect("ground under the centre");
    assert!((hit.point.y - info.center.y).abs() < 1e-3);
    assert!(w.get(hit.voxel) == mat::GRASS || w.get(hit.voxel) == mat::PAINT_WHITE);
    // A wide open field: straight up from the centre is clear for a long way (cheap to render).
    assert!(raycast(&w, Vec3::new(0.3, info.center.y + 1.0, 0.3), Vec3::Y, 50.0).is_none());
    // Void well beyond the platform: no ground to hit near the edge, straight down from high up.
    let far = w.bounds().max.x + 5.0;
    let below = raycast(&w, Vec3::new(far, info.center.y + 20.0, 0.3), Vec3::NEG_Y, 60.0);
    assert!(below.is_none() || w.get(below.unwrap().voxel) == AIR);
}
