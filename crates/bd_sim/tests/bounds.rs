//! Blast zones vs tethers, construct recall and visible summon placement (`bd_sim::world::bounds`,
//! COMBAT.md §1a / §3):
//! * an attached tether (anchored in the arena) holds a player past the side / bottom zones; the
//!   star KO (top) still applies (Flat, Archipelago, Orbit); a broken or released rope KOs at once;
//! * a construct that leaves the map blinks back next to its summoner in a free spot (or is gone
//!   when the summoner is KO'd, or after the anti-loop valve);
//! * fresh summons land where the caster can see them;
//! * host and client prediction agree while hanging out of bounds.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::voxel::{Aabb, AIR, VoxelQuery, world_to_voxel};
use bd_core::{ConstructId, IVec3, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::{GameEvent, GoneReason};
use bd_sim::movement::{AnchorTarget, ColliderId, Hook, HookPhase};
use bd_sim::rules::WillpowerMode;
use bd_sim::smash::BlastZone;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
/// Flat lab (96 m): ground top at 8 m, voxel world x/z ∈ [-48, 48], y ∈ [0, 64], void at -4.
const G: f32 = 8.0;
const EDGE: f32 = 48.0;

fn cfg(arena: ArenaSource) -> SimConfig {
    SimConfig { arena, check_invariants: true, ..SimConfig::default() }
}

fn setup(mut w: World) -> World {
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w
}

fn flat() -> World {
    setup(World::new(cfg(ArenaSource::Flat { size_m: 96.0 })))
}

fn place(w: &mut World, id: PlayerId, pos: Vec3, vel: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = vel;
    p.mv.grounded = false;
}

fn hold_left() -> PlayerInput {
    PlayerInput { buttons: buttons::TETHER_L, ..PlayerInput::default() }
}

/// Anchor `id`'s left hook on the (solid) voxel at `v`, taut.
fn anchor_voxel(w: &mut World, id: PlayerId, v: IVec3) {
    assert!(w.voxels.is_solid(v), "anchor voxel {v} must be solid");
    let point = (v.as_vec3() + Vec3::new(0.5, 1.0, 0.5)) * 0.5;
    let params = w.params().clone();
    let p = w.player_mut(id).unwrap();
    let chest = p.mv.chest(&params);
    p.mv.hooks[0] = Hook {
        phase: HookPhase::Attached,
        pos: point,
        vel: Vec3::ZERO,
        travelled: 0.0,
        target: Some(AnchorTarget::Voxel(v)),
        local: Vec3::ZERO,
        rope_len: (point - chest).length(),
    };
    p.input.buttons |= buttons::TETHER_L;
    p.prev_buttons |= buttons::TETHER_L;
    p.mv.prev_buttons |= buttons::TETHER_L;
}

/// Tick with `inputs` for B; returns B's KO (zone), if any.
fn run_b(w: &mut World, ticks: u32, input: PlayerInput) -> (Option<Option<BlastZone>>, Vec<GameEvent>) {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[(B, input)]));
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
    }
    let ko = all.iter().find_map(|e| match e {
        GameEvent::Ko { victim, zone, .. } if *victim == B => Some(*zone),
        _ => None,
    });
    (ko, all)
}

/// The top-of-ground voxel at the flat lab's east edge (x = 47.5 m), z = 0.
fn edge_voxel() -> IVec3 {
    world_to_voxel(Vec3::new(EDGE - 0.25, G - 0.25, 0.25))
}

// ================================================================================ tether save

#[test]
fn a_tethered_player_hangs_below_the_void_and_past_the_side_zone() {
    let mut w = flat();
    let z = w.blast_zones();
    assert_eq!(z.bottom, -4.0);
    assert!((z.max_x - (EDGE + 25.0)).abs() < 1e-3, "side zone = voxel bounds + 25 m ({})", z.max_x);
    // Bottom: dangling under the east cliff, 4 m below the void, then knocked further down / out.
    place(&mut w, B, Vec3::new(EDGE + 2.0, -8.0, 0.0), Vec3::ZERO);
    anchor_voxel(&mut w, B, edge_voxel());
    w.launch_player(B, Vec3::new(18.0, -20.0, 0.0), Some(A));
    let (ko, _) = run_b(&mut w, 360, hold_left());
    assert_eq!(ko, None, "the rope holds below the void");
    let b = w.player(B).unwrap();
    assert!(b.alive && b.mv.tethered() && b.mv.pos.is_finite());
    assert!(b.mv.pos.y < z.bottom, "still hanging below the void ({})", b.mv.pos);
    assert!(z.depth(b.mv.pos) <= w.cfg.tuning.bounds.tether_hold_depth);
    // Side: 30 m out from the cliff, past the side zone; swinging back under the edge.
    place(&mut w, B, Vec3::new(z.max_x + 5.0, G, 0.0), Vec3::new(10.0, 0.0, 0.0));
    anchor_voxel(&mut w, B, edge_voxel());
    let (ko, _) = run_b(&mut w, 240, hold_left());
    assert_eq!(ko, None, "the rope holds past the side zone");
    assert!(w.player(B).unwrap().alive);
}

#[test]
fn without_a_rope_the_same_spots_are_kos() {
    for (at, zone) in [(Vec3::new(EDGE + 2.0, -8.0, 0.0), BlastZone::Bottom), (Vec3::new(EDGE + 30.0, G, 0.0), BlastZone::Side)] {
        let mut w = flat();
        place(&mut w, B, at, Vec3::ZERO);
        let (ko, _) = run_b(&mut w, 2, PlayerInput::default());
        assert_eq!(ko, Some(Some(zone)), "{at}");
    }
}

#[test]
fn an_anchor_outside_the_arena_does_not_save() {
    // A rope tied to something that is itself below the void (a seabed, a falling boulder) is no
    // lifeline.
    let mut w = flat();
    w.arena.void_y = 4.0;
    let v = world_to_voxel(Vec3::new(EDGE - 0.25, 1.75, 0.25));
    place(&mut w, B, Vec3::new(EDGE + 2.0, 0.0, 0.0), Vec3::ZERO);
    anchor_voxel(&mut w, B, v);
    let (ko, _) = run_b(&mut w, 2, hold_left());
    assert_eq!(ko, Some(Some(BlastZone::Bottom)));
}

#[test]
fn too_deep_is_still_a_ko() {
    let mut w = flat();
    let depth = w.cfg.tuning.bounds.tether_hold_depth;
    place(&mut w, B, Vec3::new(EDGE + 2.0, -4.0 - depth - 2.0, 0.0), Vec3::ZERO);
    anchor_voxel(&mut w, B, edge_voxel());
    let (ko, _) = run_b(&mut w, 2, hold_left());
    assert_eq!(ko, Some(Some(BlastZone::Bottom)));
}

#[test]
fn a_broken_anchor_ko_s_at_once() {
    let mut w = flat();
    place(&mut w, B, Vec3::new(EDGE + 2.0, -8.0, 0.0), Vec3::ZERO);
    let v = edge_voxel();
    anchor_voxel(&mut w, B, v);
    let (ko, _) = run_b(&mut w, 60, hold_left());
    assert_eq!(ko, None);
    // The ledge is shot away: the hook loses its grip on the next step → normal KO rules.
    w.voxels.set(v, AIR);
    let (ko, evs) = run_b(&mut w, 1, hold_left());
    assert_eq!(ko, Some(Some(BlastZone::Bottom)), "{evs:?}");
}

#[test]
fn letting_go_out_of_bounds_ko_s_at_once() {
    let mut w = flat();
    place(&mut w, B, Vec3::new(EDGE + 2.0, -8.0, 0.0), Vec3::ZERO);
    anchor_voxel(&mut w, B, edge_voxel());
    let (ko, _) = run_b(&mut w, 60, hold_left());
    assert_eq!(ko, None);
    let (ko, _) = run_b(&mut w, 1, PlayerInput::default());
    assert_eq!(ko, Some(Some(BlastZone::Bottom)), "released below the void: KO on the same tick");
}

/// A high anchor voxel (`h` metres below the voxel world's top at x = z = 0) and a player `r`
/// metres east of it, launched straight up: the swing carries them over the top zone.
fn star_ko_while_tethered(mut w: World, r: f32) {
    let top = w.voxels.bounds().max.y;
    let v = world_to_voxel(Vec3::new(0.25, top - 1.25, 0.25));
    w.voxels.set(v, 3);
    let anchor = (v.as_vec3() + Vec3::new(0.5, 1.0, 0.5)) * 0.5;
    place(&mut w, B, anchor + Vec3::new(r, -1.2, 0.0), Vec3::ZERO);
    anchor_voxel(&mut w, B, v);
    assert!(w.player(B).unwrap().mv.tethered());
    w.launch_player(B, Vec3::new(0.0, 80.0, 0.0), Some(A));
    let mut tethered_before_ko = false;
    for _ in 0..600 {
        let was = w.player(B).unwrap().mv.tethered();
        let (ko, _) = run_b(&mut w, 1, hold_left());
        if let Some(zone) = ko {
            assert_eq!(zone, Some(BlastZone::Top), "the star KO ignores tethers");
            tethered_before_ko = was;
            break;
        }
    }
    assert!(tethered_before_ko, "still on the rope when the star KO landed");
}

#[test]
fn tethered_star_ko_on_archipelago() {
    let w = setup(World::new(cfg(ArenaSource::Archipelago { seed: 3 })));
    let r = w.cfg.tuning.smash.blast_margin_top + 8.0;
    star_ko_while_tethered(w, r);
}

#[test]
fn tethered_star_ko_on_orbit() {
    let (voxels, info) = bd_voxel::generate_orbit(1);
    let w = setup(World::with_arena(cfg(ArenaSource::Flat { size_m: 96.0 }), voxels, info.arena.clone(), 1));
    let r = w.cfg.tuning.smash.blast_margin_top + 8.0;
    star_ko_while_tethered(w, r);
}

#[test]
fn tethered_star_ko_on_flat() {
    let w = flat();
    let r = w.cfg.tuning.smash.blast_margin_top + 8.0;
    star_ko_while_tethered(w, r);
}

// ================================================================================ recall

/// A's construct, formed, near A. A stands at (20, 8, 0) facing +X.
fn with_boulder(w: &mut World, form: Form) -> ConstructId {
    place(w, A, Vec3::new(20.0, G, 0.0), Vec3::ZERO);
    w.player_mut(A).unwrap().input.yaw = -std::f32::consts::FRAC_PI_2;
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, recipe("boulder", form, &[], Element::Light, 1.0), None, Aim::new(eye, Vec3::new(1.0, -0.2, 0.0))).unwrap();
    for _ in 0..90 {
        w.tick(&[]);
    }
    assert!(w.constructs.get(id).is_some_and(|c| c.active()), "formed");
    id
}

fn throw_off(w: &mut World, id: ConstructId) {
    let c = w.constructs.get_mut(id).unwrap();
    // Structures are knocked loose first (a static wall doesn't fly).
    bd_sim::constructs::components::loosen(c, Vec3::ZERO);
    c.pos = Vec3::new(EDGE + 8.0, G + 6.0, 0.0);
    c.prev_pos = c.pos;
    c.vel = Vec3::new(30.0, 4.0, 0.0);
    c.held.fly_vel = c.vel;
}

/// Tick until `id` is recalled or gone (≤ 6 s). Returns the event.
fn until_recalled_or_gone(w: &mut World, id: ConstructId) -> GameEvent {
    for _ in 0..720 {
        let evs = w.tick(&[]).to_vec();
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
        if let Some(e) = evs.into_iter().find(|e| match e {
            GameEvent::ConstructRecalled { id: c, .. } | GameEvent::ConstructGone { id: c, .. } | GameEvent::ConstructShattered { id: c, .. } => *c == id,
            _ => false,
        }) {
            return e;
        }
    }
    panic!("construct {id:?} neither recalled nor gone");
}

fn solid_in(w: &World, b: &Aabb) -> bool {
    let lo = world_to_voxel(b.min + Vec3::splat(0.07));
    let hi = world_to_voxel(b.max - Vec3::splat(0.07));
    for x in lo.x..=hi.x {
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                if w.voxels.is_solid(IVec3::new(x, y, z)) {
                    return true;
                }
            }
        }
    }
    false
}

fn assert_free_near_summoner(w: &World, id: ConstructId) {
    let c = w.constructs.get(id).expect("still exists");
    let a = w.player(A).unwrap();
    assert_eq!(c.vel, Vec3::ZERO, "velocity zeroed");
    assert!((c.center() - a.mv.pos).length() < 10.0, "next to the summoner ({} vs {})", c.center(), a.mv.pos);
    assert!(w.blast_zones().contains(c.center()));
    assert!(!solid_in(w, &c.bounds()), "not inside terrain ({:?})", c.bounds());
    assert!(!c.bounds().intersects(&a.hull(w.params())), "not inside the summoner");
}

#[test]
fn a_construct_thrown_off_the_map_recalls_next_to_its_summoner() {
    for form in [Form::Boulder, Form::Quadruped, Form::Wall] {
        let mut w = flat();
        let id = with_boulder(&mut w, form);
        throw_off(&mut w, id);
        let e = until_recalled_or_gone(&mut w, id);
        let GameEvent::ConstructRecalled { owner, from, to, .. } = e else { panic!("{form:?}: expected a recall, got {e:?}") };
        assert_eq!(owner, A);
        assert!(!w.blast_zones().contains(from) || from.y < -4.0, "{form:?} was lost at {from}");
        assert_eq!(w.constructs.get(id).unwrap().pos, to);
        assert_free_near_summoner(&w, id);
        // It keeps living afterwards (not despawned right after).
        for _ in 0..120 {
            w.tick(&[]);
        }
        assert!(w.constructs.get(id).is_some(), "{form:?} survives after the recall");
    }
}

#[test]
fn a_thrown_held_item_comes_back_to_the_hand() {
    let mut w = flat();
    place(&mut w, A, Vec3::new(30.0, G, 0.0), Vec3::ZERO);
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, recipe("axe", Form::Axe, &[], Element::Light, 1.0), None, Aim::new(eye, Vec3::X)).unwrap();
    for _ in 0..60 {
        w.tick(&[]);
    }
    {
        let c = w.constructs.get_mut(id).unwrap();
        c.held.thrown = true;
        c.held.mode = bd_sim::constructs::held::Mode::Thrown;
        c.held.fly_t = 0.5;
    }
    assert!(!w.constructs.get(id).unwrap().attached());
    throw_off(&mut w, id);
    let e = until_recalled_or_gone(&mut w, id);
    assert!(matches!(e, GameEvent::ConstructRecalled { .. }), "{e:?}");
    assert!(w.constructs.get(id).unwrap().attached(), "back in the hand");
}

#[test]
fn a_ko_d_summoner_s_construct_is_gone() {
    let mut w = flat();
    let id = with_boulder(&mut w, Form::Boulder);
    place(&mut w, A, Vec3::new(EDGE + 30.0, G, 0.0), Vec3::ZERO);
    w.tick(&[]);
    assert!(!w.player(A).unwrap().alive);
    throw_off(&mut w, id);
    let e = until_recalled_or_gone(&mut w, id);
    assert!(matches!(e, GameEvent::ConstructGone { reason: GoneReason::Expired, .. }), "{e:?}");
}

#[test]
fn the_anti_loop_valve_lets_the_fourth_exit_despawn() {
    let mut w = flat();
    let id = with_boulder(&mut w, Form::Boulder);
    let max = w.cfg.tuning.bounds.recall_max;
    for k in 0..max {
        throw_off(&mut w, id);
        let e = until_recalled_or_gone(&mut w, id);
        assert!(matches!(e, GameEvent::ConstructRecalled { .. }), "recall {k}: {e:?}");
    }
    throw_off(&mut w, id);
    let e = until_recalled_or_gone(&mut w, id);
    assert!(matches!(e, GameEvent::ConstructGone { .. }), "exit {} within the window despawns: {e:?}", max + 1);
}

#[test]
fn ropes_tied_to_a_recalled_construct_snap() {
    let mut w = flat();
    let id = with_boulder(&mut w, Form::Wall);
    let center = w.constructs.get(id).unwrap().center();
    place(&mut w, B, center + Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO);
    {
        let p = w.player_mut(B).unwrap();
        p.mv.hooks[0] = Hook {
            phase: HookPhase::Attached,
            pos: center,
            target: Some(AnchorTarget::Collider(ColliderId::Construct { id, part: 0 })),
            rope_len: 6.0,
            ..Hook::default()
        };
    }
    throw_off(&mut w, id);
    for _ in 0..720 {
        let evs = w.tick(&[(B, hold_left())]).to_vec();
        if evs.iter().any(|e| matches!(e, GameEvent::ConstructRecalled { id: c, .. } if *c == id)) {
            assert!(!w.player(B).unwrap().mv.hooks[0].attached(), "the rope to the recalled wall snapped");
            return;
        }
    }
    panic!("never recalled");
}

// ================================================================================ placement

/// A stands at (10, 8, 5) looking along `yaw` / `pitch`; casts `r`. Returns the construct.
fn cast_from(w: &mut World, yaw: f32, pitch: f32, r: Recipe) -> ConstructId {
    place(w, A, Vec3::new(10.0, G, 5.0), Vec3::ZERO);
    place(w, B, Vec3::new(-40.0, G, -40.0), Vec3::ZERO);
    {
        let p = w.player_mut(A).unwrap();
        p.mv.grounded = true;
        p.input.yaw = yaw;
        p.input.pitch = pitch;
    }
    let eye = w.player(A).unwrap().eye(w.params());
    let dir = bd_core::input::look_dir(yaw, pitch);
    w.cast(A, r, None, Aim::new(eye, dir)).unwrap()
}

/// The construct's centre is in front of the eye (within `deg` of the aim), with a clear line of
/// sight, clear of terrain and of the caster.
fn assert_in_view(w: &World, id: ConstructId, yaw: f32, pitch: f32, deg: f32) -> (f32, f32) {
    let c = w.constructs.get(id).unwrap();
    let a = w.player(A).unwrap();
    let eye = a.eye(w.params());
    let to = c.center() - eye;
    let dist = to.length();
    let aim = bd_core::input::look_dir(yaw, pitch);
    let angle = (to.dot(aim) / dist).clamp(-1.0, 1.0).acos().to_degrees();
    assert!(angle <= deg, "centre {angle}° off the aim (pos {}, eye {eye})", c.pos);
    let hit = bd_core::voxel::raycast(&w.voxels, eye, to / dist, dist);
    assert!(hit.is_none_or(|h| h.t >= dist - c.half.length() * 0.5), "line of sight blocked at {:?}", hit.map(|h| h.point));
    assert!(!solid_in(w, &c.bounds()), "inside terrain");
    assert!(!c.bounds().intersects(&a.hull(w.params())), "on top of the caster");
    (dist, angle)
}

#[test]
fn a_summon_facing_a_wall_still_spawns_in_view() {
    let mut w = flat();
    // A 4 m wall 6 m in front of A (A looks toward −X).
    let lo = world_to_voxel(Vec3::new(3.0, G, -2.0));
    let hi = world_to_voxel(Vec3::new(3.9, G + 3.9, 12.0));
    w.voxels.fill_box(lo, hi, 3);
    let yaw = std::f32::consts::FRAC_PI_2;
    for r in [recipe("wolf", Form::Quadruped, &[], Element::Light, 1.0), recipe("wall", Form::Wall, &[], Element::Light, 1.0), recipe("big wolf", Form::Quadruped, &[], Element::Light, 2.5)] {
        let id = cast_from(&mut w, yaw, -0.05, r.clone());
        let (dist, _) = assert_in_view(&w, id, yaw, -0.05, 45.0);
        assert!(dist < 8.0, "{}: between the caster and the wall ({dist})", r.name);
        w.dismiss(A);
        w.tick(&[]);
    }
}

#[test]
fn a_summon_cast_at_the_sky_lands_on_the_ground_ahead() {
    let mut w = flat();
    let yaw = std::f32::consts::FRAC_PI_2;
    let pitch = 1.2;
    let id = cast_from(&mut w, yaw, pitch, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.0));
    let c = w.constructs.get(id).unwrap();
    let a = w.player(A).unwrap().mv.pos;
    let d = Vec3::new(c.pos.x - a.x, 0.0, c.pos.z - a.z);
    assert!((c.pos.y - G).abs() < 0.6, "on the ground ({})", c.pos);
    assert!(d.length() > 3.0 && d.length() < 30.0, "a sane distance ahead ({})", d.length());
    assert!(d.normalize().dot(Vec3::NEG_X) > 0.8, "ahead of the caster");
    // Looking straight down: not under the caster's feet.
    w.dismiss(A);
    w.tick(&[]);
    let id = cast_from(&mut w, yaw, -1.5, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.0));
    let c = w.constructs.get(id).unwrap();
    let a = w.player(A).unwrap();
    assert!(!c.bounds().intersects(&a.hull(w.params())));
    assert!(Vec3::new(c.pos.x - a.mv.pos.x, 0.0, c.pos.z - a.mv.pos.z).length() >= 3.0);
}

#[test]
fn a_summon_cast_off_a_ledge_stands_on_solid_ground() {
    let mut w = flat();
    // 2 m from the east cliff, looking out over the void.
    place(&mut w, A, Vec3::new(EDGE - 2.0, G, 0.0), Vec3::ZERO);
    place(&mut w, B, Vec3::new(-40.0, G, -40.0), Vec3::ZERO);
    let yaw = -std::f32::consts::FRAC_PI_2;
    w.player_mut(A).unwrap().input.yaw = yaw;
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.0), None, Aim::new(eye, bd_core::input::look_dir(yaw, 0.0))).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!((c.pos.y - G).abs() < 0.6 && c.pos.x < EDGE, "on the island, not over the void ({})", c.pos);
    assert!(!c.bounds().intersects(&w.player(A).unwrap().hull(w.params())));
}

#[test]
fn a_kaiju_fits_in_the_view() {
    let mut w = flat();
    place(&mut w, A, Vec3::new(-30.0, G, 0.0), Vec3::ZERO);
    let yaw = -std::f32::consts::FRAC_PI_2; // looking +X across the open middle
    let id = {
        place(&mut w, B, Vec3::new(-40.0, G, -40.0), Vec3::ZERO);
        let p = w.player_mut(A).unwrap();
        p.input.yaw = yaw;
        p.input.pitch = 0.0;
        let eye = w.player(A).unwrap().eye(w.params());
        w.cast(A, recipe("godzilla", Form::Kaiju, &[], Element::Fire, 1.0), None, Aim::new(eye, bd_core::input::look_dir(yaw, 0.0))).unwrap()
    };
    let (dist, angle) = assert_in_view(&w, id, yaw, 0.0, 45.0);
    let r = w.constructs.get(id).unwrap().half.length();
    let half_angle = (r / dist).clamp(0.0, 1.0).asin().to_degrees();
    assert!(half_angle <= 45.0, "the whole kaiju fits a 90° view (r {r}, dist {dist}, half angle {half_angle})");
    assert!(angle > 3.0, "huge constructs stand a little to the side ({angle}°)");
}

#[test]
fn placement_is_deterministic() {
    let mk = || {
        let mut w = flat();
        let id = cast_from(&mut w, 0.7, 0.1, recipe("bear", Form::Quadruped, &[Mechanic::Heavy], Element::Light, 1.4));
        w.constructs.get(id).unwrap().pos
    };
    assert_eq!(mk().to_array().map(f32::to_bits), mk().to_array().map(f32::to_bits));
}

// ================================================================================ parity

struct Pair {
    host: World,
    client: World,
}

impl Pair {
    fn new(setup_host: impl FnOnce(&mut World)) -> Self {
        let mut host = flat();
        setup_host(&mut host);
        host.tick(&[]);
        let mut client = World::new(cfg(ArenaSource::Flat { size_m: 96.0 }));
        for e in host.construct_manifest() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(B)), Some(B));
        Pair { host, client }
    }

    /// Predict B from the replica, run the host, feed the client; worst (pos, vel) error.
    fn run(&mut self, ticks: u32, inp: PlayerInput) -> (f32, f32) {
        let mut worst = (0.0f32, 0.0f32);
        for _ in 0..ticks {
            let mut s = self.client.player(B).unwrap().mv;
            self.client.predict_player(B, &mut s, &inp);
            let evs = self.host.tick(&[(B, inp)]).to_vec();
            for e in &evs {
                self.client.apply_event(e);
            }
            self.client.apply_snapshot(&self.host.snapshot_for(Some(B)), Some(B));
            let h = self.host.player(B).unwrap().mv;
            worst.0 = worst.0.max((h.pos - s.pos).length());
            worst.1 = worst.1.max((h.vel - s.vel).length());
        }
        worst
    }
}

#[test]
fn host_and_prediction_agree_while_hanging_out_of_bounds() {
    let mut pair = Pair::new(|w| {
        place(w, B, Vec3::new(EDGE + 6.0, -6.0, 0.0), Vec3::new(8.0, -4.0, 0.0));
        anchor_voxel(w, B, edge_voxel());
    });
    let mut inp = hold_left();
    let (dp, dv) = pair.run(240, inp);
    assert!(dp < 1e-4 && dv < 1e-3, "prediction off by {dp} m / {dv} m/s");
    let b = pair.host.player(B).unwrap();
    assert!(b.alive && b.mv.pos.y < -4.0, "hanging below the void ({})", b.mv.pos);
    // Reeling back up in: still bit-for-bit.
    inp.buttons |= buttons::REEL;
    let (dp, dv) = pair.run(240, inp);
    assert!(dp < 1e-4 && dv < 1e-3, "reel: prediction off by {dp} m / {dv} m/s");
    assert!(pair.host.player(B).unwrap().alive);
}

#[test]
fn host_and_prediction_agree_next_to_a_fresh_summon() {
    let mut pair = Pair::new(|w| {
        place(w, B, Vec3::new(10.0, G, 5.0), Vec3::ZERO);
        w.player_mut(B).unwrap().input.yaw = std::f32::consts::FRAC_PI_2;
        let eye = w.player(B).unwrap().eye(w.params());
        w.cast(B, recipe("wall", Form::Wall, &[], Element::Light, 1.0), None, Aim::new(eye, Vec3::NEG_X)).unwrap();
        for _ in 0..60 {
            w.tick(&[]);
        }
    });
    // Strafe along the fresh wall (touching a replica's snapshot-quantised collider is a known,
    // reconciled mm-level mismatch; the placement itself is host-only).
    let inp = PlayerInput { move_x: 1, yaw: std::f32::consts::FRAC_PI_2, ..PlayerInput::default() };
    let (dp, dv) = pair.run(180, inp);
    assert!(dp < 1e-4 && dv < 1e-3, "prediction off by {dp} m / {dv} m/s");
}



// ================================================================================ bots

#[test]
fn a_bot_knocked_off_the_edge_catches_the_cliff_and_holds_on() {
    use bd_sim::bots::BotBrain;
    let mut w = flat();
    place(&mut w, A, Vec3::new(-40.0, G, -40.0), Vec3::ZERO);
    place(&mut w, B, Vec3::new(EDGE - 1.0, G, 0.0), Vec3::ZERO);
    let mut bot = BotBrain::new(B, 1.0, 7);
    bot.casts_enabled = false;
    // Stand on the edge for a moment (the bot remembers solid ground), then get knocked out.
    for _ in 0..60 {
        let act = bot.think(&w);
        w.tick(&[(B, act.input)]);
        place(&mut w, B, Vec3::new(EDGE - 1.0, G, 0.0), Vec3::ZERO);
        w.player_mut(B).unwrap().mv.grounded = true;
    }
    w.launch_player(B, Vec3::new(22.0, -4.0, 0.0), Some(A));
    // AIR JUMP: this is about the rope rescue. With a working air jump the bot hops back onto the
    // edge by itself (tests/air_jump.rs), so make the air jump a no-op here.
    w.cfg.move_params.air_jump_speed = 0.0;
    w.cfg.move_params.air_jump_redirect = 0.0;
    let mut tethered = 0;
    let mut below_void = 0;
    for _ in 0..(10 * 120) {
        let act = bot.think(&w);
        let evs = w.tick(&[(B, act.input)]).to_vec();
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
        if let Some(GameEvent::Ko { zone, .. }) = evs.iter().find(|e| matches!(e, GameEvent::Ko { victim, .. } if *victim == B)) {
            panic!("the bot fell ({zone:?}) after {tethered} tethered ticks ({below_void} below the void)");
        }
        let b = w.player(B).unwrap();
        tethered += usize::from(b.mv.tethered());
        below_void += usize::from(b.mv.tethered() && b.mv.pos.y < -4.0);
    }
    assert!(tethered > 0, "the bot's rescue tether caught something");
    eprintln!("tethered {tethered} ticks, {below_void} of them below the void; ends at {}", w.player(B).unwrap().mv.pos);
}
