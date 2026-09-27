//! Nobody lives under the floor.
//!
//! Regression: with `AimTuning::neutral()` the alloc scenario (8 bots, flat arena, no casts) left
//! bots alive at y ≈ -1.8 under the 8 m slab. A bot knocked off the edge fired its rescue rope at
//! the high beam; ropes ignored terrain once attached, so the swing carried it in under the slab and
//! the rope then pinned it against the slab's underside (head at y = 0, above the void at -4) for the
//! rest of the match. Now (1) a rope that pins its player against a ceiling it runs up through is
//! cut (`movement::tether::rope_under_terrain`, host and prediction alike), and (2) bots don't pick
//! ceiling anchors for their rescue rope and let go after 1.5 s hanging with terrain over their head
//! (`bots.rs`), so they drop into the void instead of living under the map.

use bd_core::input::PlayerInput;
use bd_core::voxel::world_to_voxel;
use bd_core::{IVec3, Vec3};
use bd_sim::bots;
use bd_sim::movement::testkit::FnWorld;
use bd_sim::movement::{AnchorTarget, CollisionWorld, Hand, Hook, HookPhase, MoveParams, PlayerState, step_player};
use bd_sim::world::{ArenaSource, SimConfig, World};

/// A 40 × 8 × 40 m slab (top at y = 8) with a pillar in the middle rising to y = 20.
fn slab_world() -> FnWorld<impl Fn(IVec3) -> bool> {
    FnWorld(|v: IVec3| {
        let slab = (0..16).contains(&v.y) && (-40..40).contains(&v.x) && (-40..40).contains(&v.z);
        let pillar = (16..40).contains(&v.y) && v.x == 0 && v.z == 0;
        slab || pillar
    })
}

/// One movement step for a player at `feet` holding a left rope attached at `anchor`.
fn step_on_rope(feet: Vec3, anchor: Vec3) -> (PlayerState, bool) {
    let world = slab_world();
    let cw = CollisionWorld::new(&world, &[]);
    let p = MoveParams::default();
    let mut s = PlayerState::new(feet, &p);
    let voxel = world_to_voxel(anchor - Vec3::new(0.0, 0.01, 0.0));
    let voxel = if bd_core::voxel::VoxelQuery::is_solid(&world, voxel) { voxel } else { world_to_voxel(anchor + Vec3::new(0.0, 0.01, 0.0)) };
    assert!(bd_core::voxel::VoxelQuery::is_solid(&world, voxel), "test anchor must be on a solid voxel");
    let len = (anchor - s.chest(&p)).length();
    s.hooks[0] = Hook { phase: HookPhase::Attached, pos: anchor, target: Some(AnchorTarget::Voxel(voxel)), rope_len: len, ..Hook::default() };
    let input = PlayerInput { buttons: Hand::Left.button(), ..PlayerInput::default() };
    let ev = step_player(&mut s, &input, &cw, &p, bd_core::DT);
    (s, ev.tether_broken[0])
}

#[test]
fn rope_through_the_underside_of_terrain_is_cut() {
    // Under the slab, rope running up through it to the pillar top: cut.
    let (s, broken) = step_on_rope(Vec3::new(3.0, -1.85, 0.0), Vec3::new(0.25, 20.0, 0.25));
    assert!(broken && s.hooks[0].phase == HookPhase::Idle, "a rope through the floor above must snap");
}

#[test]
fn ropes_over_ledges_and_on_ceilings_hold() {
    // Dangling beside the slab, the rope over its edge (it crosses the slab's side face): holds.
    let (s, broken) = step_on_rope(Vec3::new(26.0, -2.5, 0.0), Vec3::new(0.25, 20.0, 0.25));
    assert!(!broken && s.hooks[0].attached(), "a rope over a ledge must hold");
    // Pressed against the underside just inside the edge, rope to the cliff lip: holds (reeling
    // slides them out round the corner and back up).
    let (s, broken) = step_on_rope(Vec3::new(19.0, -1.85, 0.0), Vec3::new(19.75, 8.0, 0.25));
    assert!(!broken && s.hooks[0].attached(), "a rope to the lip right above must hold");
    // Hanging from the slab's underside itself (a bridge): holds.
    let (s, broken) = step_on_rope(Vec3::new(1.0, -5.0, 0.0), Vec3::new(0.25, 0.0, 0.25));
    assert!(!broken && s.hooks[0].attached(), "a rope anchored on a ceiling must hold");
    // Above the slab, rope to the pillar top: holds.
    let (s, broken) = step_on_rope(Vec3::new(6.0, 8.0, 0.0), Vec3::new(0.25, 20.0, 0.25));
    assert!(!broken && s.hooks[0].attached(), "an ordinary swing rope must hold");
}

/// The alloc scenario with neutral aim assist: no living bot spends more than 4 s under the flat
/// arena's slab (below its underside at y = 0, inside its footprint). Before the fix: forever.
#[test]
fn bots_never_stay_under_the_flat_arena() {
    for seed in [5u64, 6, 7] {
        let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed, check_invariants: false, ..SimConfig::default() };
        cfg.timings.warmup_secs = 0.1;
        let mut w = World::new(cfg);
        w.cfg.tuning.assist.aim = bd_sim::aim_assist::AimTuning::neutral();
        let floor = w.voxels.bounds();
        let mut brains = bots::add_bots(&mut w, 0, 8, 0.9, seed);
        for b in &mut brains {
            b.casts_enabled = false;
        }
        let mut inputs = Vec::with_capacity(8);
        let mut under = [0u32; 8];
        for t in 0..3600 {
            inputs.clear();
            bots::drive(&mut w, &mut brains, &mut inputs);
            w.tick(&inputs);
            for (i, p) in w.players.iter().enumerate() {
                let q = p.mv.pos;
                let beneath = p.alive && q.y < floor.min.y && q.x > floor.min.x && q.x < floor.max.x && q.z > floor.min.z && q.z < floor.max.z;
                under[i] = if beneath { under[i] + 1 } else { 0 };
                assert!(under[i] < 480, "seed {seed}: player {i} stuck under the floor at t{t}: pos {q:?} vel {:?} hooks {:?}", p.mv.vel, p.mv.hooks);
            }
        }
    }
}
