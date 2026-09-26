//! Signature combos from COMBAT.md §0 / §5b / §8 / §9 / §5d-bis: each checks that a combination
//! makes something *different happen* (behaviour, not numbers). Flat lab arena (ground y = 8).

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::{ConstructHit, HitKind};
use bd_sim::constructs::components::XMode;
use bd_sim::events::{GameEvent, KoCause, Traveller};
use bd_sim::rules::{ConstructLimits, WillpowerMode};
use bd_sim::space::{LinkEnd, PortalEnd};
use bd_sim::world::{Aim, ArenaSource, SimConfig, TargetHint, World};

/// Smash model: tests written against health read `100 − damage %` (same comparisons).
#[allow(dead_code)]
trait HealthShim {
    fn health(&self) -> f32;
}
impl HealthShim for bd_sim::Player {
    fn health(&self) -> f32 {
        100.0 - self.damage_pct
    }
}

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);
const G: f32 = 8.0;

fn duel(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    w.cfg.rules.limits = ConstructLimits::chaos();
    for k in 1..=n {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w
}

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
    p.spawn_shield = 0.0;
}

/// Point `id`'s view along `dir`.
fn face(w: &mut World, id: PlayerId, dir: Vec3) {
    let d = dir.normalize();
    let p = w.player_mut(id).unwrap();
    p.input.yaw = (-d.x).atan2(-d.z);
    p.input.pitch = d.y.clamp(-1.0, 1.0).asin();
}

fn eye(w: &World, id: PlayerId) -> Vec3 {
    let p = w.player(id).unwrap();
    p.eye(&w.cfg.move_params)
}

/// Cast `r` from `who` aimed at world point `at` (looking there too).
fn cast_at(w: &mut World, who: PlayerId, r: Recipe, at: Vec3, hint: Option<TargetHint>) -> ConstructId {
    let e = eye(w, who);
    face(w, who, at - e);
    let aim = Aim { target_hint: hint, ..Aim::new(e, (at - e).normalize()) };
    w.cast(who, r, None, aim).unwrap()
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
        assert!(w.last_invariant_error.is_none(), "invariant: {:?}", w.last_invariant_error);
    }
    all
}

/// Run until `f` holds (or `max` ticks). Returns (ticks run, all events).
fn run_until(w: &mut World, max: u32, mut f: impl FnMut(&World, &[GameEvent]) -> bool) -> (u32, Vec<GameEvent>) {
    let mut all = Vec::new();
    for t in 0..max {
        let ev = w.tick(&[]).to_vec();
        assert!(w.last_invariant_error.is_none(), "invariant: {:?}", w.last_invariant_error);
        let done = f(w, &ev);
        all.extend(ev);
        if done {
            return (t + 1, all);
        }
    }
    (max, all)
}

fn hits_from(ev: &[GameEvent], cid: ConstructId, target: PlayerId) -> usize {
    ev.iter().filter(|e| matches!(e, GameEvent::Hit { construct: Some(c), target: t, .. } if *c == cid && *t == target)).count()
}

fn health(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().health()
}

fn pos(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().mv.pos
}

fn cpos(w: &World, id: ConstructId) -> Option<Vec3> {
    w.constructs.get(id).map(|c| c.pos)
}

fn r(name: &str, form: Form, m: &[Mechanic], e: Element, scale: f32) -> Recipe {
    recipe(name, form, m, e, scale)
}

// ------------------------------------------------------------------------------------ §0 combos

#[test]
fn flying_hammer_leaves_the_hand_hunts_and_bonks() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -12.0));
    let id = cast_at(&mut w, A, r("flying hammer", Form::Hammer, &[Mechanic::Flies], Element::Light, 1.2), Vec3::new(0.0, G + 1.0, -12.0), Some(TargetHint::Player(B)));
    let (_, ev) = run_until(&mut w, 600, |w, ev| hits_from(ev, id, B) > 0 && w.constructs.get(id).is_some());
    assert!(hits_from(&ev, id, B) >= 1, "the flying hammer never bonked B");
    let c = w.constructs.get(id).unwrap();
    assert!(!c.attached(), "a flying hammer is a free body");
    // Bonk: it rebounds and comes again — at least two hits in a few seconds.
    let ev2 = run(&mut w, 480);
    assert!(hits_from(&ev2, id, B) >= 1, "it should dive in again after the bonk");
}

#[test]
fn bouncy_wall_rebounds_a_running_player() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    let id = cast_at(&mut w, A, r("bouncy wall", Form::Wall, &[Mechanic::Bounce], Element::Light, 6.0), Vec3::new(0.0, G, -8.0), None);
    run(&mut w, 40);
    let wall_z = w.constructs.get(id).unwrap().center().z;
    place(&mut w, B, Vec3::new(0.0, G + 0.3, wall_z - 2.5));
    w.player_mut(B).unwrap().mv.vel = Vec3::new(0.0, 0.0, 16.0);
    let (_, _) = run_until(&mut w, 90, |w, _| w.player(B).unwrap().mv.vel.z < -6.0);
    let v = w.player(B).unwrap().mv.vel;
    assert!(v.z < -6.0, "bouncy wall should send B back, vel {v}");
    // A plain wall just stops you.
    let mut w2 = duel(2);
    place(&mut w2, A, Vec3::new(0.0, G, 0.0));
    let id2 = cast_at(&mut w2, A, r("wall", Form::Wall, &[], Element::Light, 6.0), Vec3::new(0.0, G, -8.0), None);
    run(&mut w2, 40);
    let wz = w2.constructs.get(id2).unwrap().center().z;
    place(&mut w2, B, Vec3::new(0.0, G + 0.3, wz - 2.5));
    w2.player_mut(B).unwrap().mv.vel = Vec3::new(0.0, 0.0, 16.0);
    run(&mut w2, 60);
    assert!(w2.player(B).unwrap().mv.vel.z > -3.0, "a plain wall must not rebound");
}

#[test]
fn homing_trampoline_slides_under_the_enemy_and_launches_them() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -14.0));
    run(&mut w, 30);
    let id = cast_at(&mut w, A, r("homing trampoline", Form::Trampoline, &[Mechanic::Homing], Element::Light, 3.0), Vec3::new(0.0, G, -4.0), Some(TargetHint::Player(B)));
    let start = cpos(&w, id).unwrap();
    let mut max_vy = 0.0f32;
    run_until(&mut w, 600, |w, _| {
        max_vy = max_vy.max(w.player(B).unwrap().mv.vel.y);
        max_vy > 12.0
    });
    let moved = (cpos(&w, id).unwrap() - start).length();
    assert!(moved > 5.0, "the trampoline should slide toward B (moved {moved})");
    assert!(max_vy > 12.0, "B should be launched, max vy {max_vy}");
}

#[test]
fn charging_wall_is_a_bulldozer() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -16.0));
    run(&mut w, 30);
    let id = cast_at(&mut w, A, r("charging wall", Form::Wall, &[Mechanic::Charge], Element::Light, 4.0), Vec3::new(0.0, G, -6.0), Some(TargetHint::Player(B)));
    let start = cpos(&w, id).unwrap();
    let (_, ev) = run_until(&mut w, 480, |_, ev| hits_from(ev, id, B) > 0);
    assert!(hits_from(&ev, id, B) >= 1, "the charging wall never rammed B");
    assert!((cpos(&w, id).unwrap() - start).length() > 4.0, "a charging wall moves");
}

#[test]
fn anvil_swarm_is_a_hailstorm() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -10.0));
    run(&mut w, 30);
    let mut rec = r("anvil swarm", Form::Anvil, &[Mechanic::Swarm], Element::Light, 1.0);
    rec.count = 6;
    let id = cast_at(&mut w, A, rec, Vec3::new(0.0, G, -10.0), Some(TargetHint::Player(B)));
    let mut max_live = 0usize;
    let (_, ev) = run_until(&mut w, 360, |w, _| {
        max_live = max_live.max(w.constructs.owned_by(A).count());
        false
    });
    assert!(max_live >= 5, "the swarm should split into many anvils (max live {max_live})");
    let children = ev.iter().filter(|e| matches!(e, GameEvent::ConstructSpawned { owner, .. } if *owner == A)).count();
    assert!(children >= 5, "children spawned: {children}");
    let crushes = ev.iter().filter(|e| matches!(e, GameEvent::Hit { target, kind: HitKind::Crush, .. } if *target == B)).count();
    let craters = ev.iter().filter(|e| matches!(e, GameEvent::Carve { .. })).count();
    assert!(crushes >= 1, "hail should crush B");
    assert!(craters >= 3, "hail should crater the ground ({craters})");
    let _ = id;
}

#[test]
fn flying_cage_scoops_a_player_and_carries_them_over_the_void() {
    let mut w = duel(2);
    // Near the east edge of the 96 m flat island (void beyond x = 48).
    // North edge (void beyond z = 48); nothing between A and B.
    place(&mut w, A, Vec3::new(12.0, G, 30.0));
    place(&mut w, B, Vec3::new(12.0, G, 40.0));
    run(&mut w, 30);
    let id = cast_at(&mut w, A, r("flying cage", Form::Cage, &[Mechanic::Flies], Element::Light, 2.5), Vec3::new(12.0, G, 40.0), Some(TargetHint::Player(B)));
    let (_, _) = run_until(&mut w, 300, |w, _| w.constructs.get(id).is_some_and(|c| c.comp.hold != bd_sim::constructs::components::HoldTarget::None));
    assert!(matches!(w.constructs.get(id).unwrap().comp.hold, bd_sim::constructs::components::HoldTarget::Player(p) if p == B), "cage should snap shut around B");
    let edge = |p: Vec3| p.z > 48.5 || p.x.abs() > 48.5;
    let (_, ev) = run_until(&mut w, 600, |w, ev| edge(pos(w, B)) || ev.iter().any(|e| matches!(e, GameEvent::Ko { .. })));
    let over_void = edge(pos(&w, B)) || ev.iter().any(|e| matches!(e, GameEvent::Ko { victim, cause: KoCause::Void, .. } if *victim == B));
    assert!(over_void, "B should be carried out over the void, at {}", pos(&w, B));
}

#[test]
fn sticky_rocket_rides_its_target_then_blows() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -12.0));
    run(&mut w, 30);
    let id = cast_at(&mut w, A, r("sticky rocket", Form::Rocket, &[Mechanic::Sticky], Element::Light, 1.2), Vec3::new(0.0, G + 1.0, -12.0), Some(TargetHint::Player(B)));
    let (_, _) = run_until(&mut w, 360, |w, _| w.constructs.get(id).is_some_and(|c| c.stuck_to == Some(B)));
    assert_eq!(w.constructs.get(id).and_then(|c| c.stuck_to), Some(B), "the rocket should stick to B");
    let b0 = pos(&w, B);
    let (_, ev) = run_until(&mut w, 240, |w, _| w.constructs.get(id).is_none());
    let exploded = ev.iter().any(|e| matches!(e, GameEvent::Explosion { source: Some(s), .. } if *s == A));
    assert!(exploded, "sticky rocket must explode on its rider");
    assert!((pos(&w, B) - b0).length() > 2.0, "riding rocket drags B around");
    assert!(health(&w, B) < 100.0);
}

#[test]
fn exploding_boomerang_craters_on_the_way_out_and_back() {
    let mut w = duel(3);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -10.0));
    // C stands beside the flight line, close to A: missed on the way out, passed on the way back.
    place(&mut w, C, Vec3::new(1.6, G, -4.0));
    run(&mut w, 30);
    let id = cast_at(&mut w, A, r("exploding boomerang", Form::Blob0, &[Mechanic::Returning, Mechanic::Explodes], Element::Light, 0.6), Vec3::new(0.0, G + 1.0, -10.0), Some(TargetHint::Player(B)));
    let (_, ev) = run_until(&mut w, 480, |w, _| w.constructs.get(id).is_none());
    let bursts = ev.iter().filter(|e| matches!(e, GameEvent::Explosion { source: Some(s), .. } if *s == A)).count();
    assert!(bursts >= 2, "out and back: {bursts} bursts");
    assert!(health(&w, B) < 100.0 && health(&w, C) < 100.0, "both B (out) and C (back) caught a burst");
}

#[test]
fn tree_with_legs_walks_out_of_the_island_at_the_enemy() {
    let mut w = duel(2);
    // The flat arena's tree stands at (-32, 32).
    place(&mut w, A, Vec3::new(-26.0, G, 26.0));
    place(&mut w, B, Vec3::new(-16.0, G, 20.0));
    run(&mut w, 30);
    let e = eye(&w, A);
    let at = Vec3::new(-32.0 + 0.25, G + 2.0, 32.0 + 0.25);
    face(&mut w, A, at - e);
    let aim = Aim { modifier: true, target_hint: None, ..Aim::new(e, (at - e).normalize()) };
    let id = w.cast(A, r("legs", Form::Blob0, &[Mechanic::Walks], Element::Light, 1.0), None, aim).expect("animate the tree");
    let c = w.constructs.get(id).unwrap();
    assert!(c.model.is_some(), "the construct is the extracted tree");
    let start = c.pos;
    let d0 = (start - pos(&w, B)).length();
    run(&mut w, 600);
    let now = cpos(&w, id).expect("tree still walking");
    let d1 = (now - pos(&w, B)).length();
    assert!((now - start).length() > 3.0 && d1 < d0 - 2.0, "the tree should walk at B: {d0} -> {d1}");
}

#[test]
fn blade_slices_a_wall_into_two_falling_halves() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -14.0));
    let wall = cast_at(&mut w, A, r("wall", Form::Wall, &[], Element::Light, 6.0), Vec3::new(0.0, G, -7.0), None);
    run(&mut w, 40);
    let c = w.constructs.get(wall).unwrap();
    let (rot0, center) = (c.rot, c.center());
    // Any Blade hit (a sword swing, a thrown saw…) goes through the same counter.
    w.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: wall, source: Some(B), damage: 6.0, impulse: Vec3::new(0.0, 0.0, 4.0), blade: true, point: center + Vec3::new(0.5, 0.0, 0.0) });
    let ev = run(&mut w, 2);
    assert!(w.constructs.get(wall).is_none(), "the sliced wall is replaced by its halves");
    let halves: Vec<ConstructId> = w.constructs.iter().filter(|c| c.parent == Some(wall)).map(|c| c.id).collect();
    assert_eq!(halves.len(), 2, "two halves");
    assert!(ev.iter().filter(|e| matches!(e, GameEvent::ConstructSpawned { .. })).count() >= 2);
    run(&mut w, 90);
    for h in &halves {
        let c = w.constructs.get(*h).expect("half alive");
        assert!(c.body.loose, "halves fall physically");
        assert!(c.rot.angle_between(rot0) > 0.3, "each half topples away from the cut");
    }
    // A plain (non-blade) hit of the same size doesn't slice.
    let mut w2 = duel(2);
    place(&mut w2, A, Vec3::new(0.0, G, 0.0));
    let wall2 = cast_at(&mut w2, A, r("wall", Form::Wall, &[], Element::Light, 6.0), Vec3::new(0.0, G, -7.0), None);
    run(&mut w2, 40);
    let c2 = w2.constructs.get(wall2).unwrap().center();
    w2.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: wall2, source: Some(B), damage: 6.0, impulse: Vec3::ZERO, blade: false, point: c2 });
    run(&mut w2, 2);
    assert!(w2.constructs.get(wall2).is_some());
}

#[test]
fn thrown_saw_slices_a_wall() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -20.0));
    let wall = cast_at(&mut w, A, r("wall", Form::Wall, &[], Element::Light, 6.0), Vec3::new(0.0, G, -8.0), None);
    run(&mut w, 40);
    // B throws a saw blade at A's wall.
    let at = w.constructs.get(wall).unwrap().center();
    cast_at(&mut w, B, r("saw blade", Form::Blob0, &[Mechanic::Blade], Element::Light, 0.6), at, None);
    run_until(&mut w, 240, |w, _| w.constructs.get(wall).is_none());
    assert!(w.constructs.get(wall).is_none(), "the saw should slice the wall");
    assert!(w.constructs.iter().filter(|c| c.parent == Some(wall)).count() == 2);
}

#[test]
fn ice_freezes_a_fire_creature_solid_and_the_next_hit_shatters_it() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -20.0));
    run(&mut w, 30);
    let wolf = cast_at(&mut w, B, r("fire wolf", Form::Quadruped, &[], Element::Fire, 1.2), Vec3::new(0.0, G, -10.0), None);
    let ice = cast_at(&mut w, A, r("ice wall", Form::Wall, &[], Element::Ice, 4.0), Vec3::new(0.0, G, -9.0), None);
    run(&mut w, 80);
    // Put the fire creature against the ice.
    let ice_c = w.constructs.get(ice).unwrap().center();
    if let Some(c) = w.constructs.get_mut(wolf) {
        c.pos = Vec3::new(ice_c.x, G, ice_c.z + 0.6);
    }
    run_until(&mut w, 40, |w, _| w.constructs.get(wolf).is_some_and(|c| c.comp.frozen > 0.0));
    let c = w.constructs.get(wolf).expect("wolf");
    assert!(c.comp.frozen > 0.0 && c.comp.snuffed, "ice snuffs the fire and freezes it");
    let p0 = c.pos;
    run(&mut w, 30);
    assert!((w.constructs.get(wolf).unwrap().pos - p0).length() < 0.6, "frozen solid: it can't move");
    let pt = w.constructs.get(wolf).unwrap().center();
    w.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: wolf, source: Some(A), damage: 1.0, impulse: Vec3::X, blade: false, point: pt });
    let ev = run(&mut w, 2);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ConstructShattered { id, .. } if *id == wolf)), "brittle: the next hit shatters it");
}

#[test]
fn a_cage_roots_a_kaiju() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -40.0));
    run(&mut w, 30);
    let kaiju = cast_at(&mut w, B, r("godzilla", Form::Kaiju, &[], Element::Light, 12.0), Vec3::new(0.0, G, -20.0), Some(TargetHint::Player(A)));
    run(&mut w, 160);
    let kp = cpos(&w, kaiju).unwrap();
    let cage = cast_at(&mut w, A, r("cage", Form::Cage, &[], Element::Light, 4.0), kp, None);
    if let Some(c) = w.constructs.get_mut(cage) {
        c.pos = kp;
    }
    run_until(&mut w, 60, |w, _| w.constructs.get(kaiju).is_some_and(|c| c.comp.rooted > 0.0));
    let k = w.constructs.get(kaiju).unwrap();
    assert!(k.comp.rooted > 0.0, "caged kaiju is rooted");
    assert!(k.comp.pinned > 0.0, "a kaiju cage topples it");
    let p0 = k.pos;
    run(&mut w, 90);
    let moved = Vec3::new(cpos(&w, kaiju).unwrap().x - p0.x, 0.0, cpos(&w, kaiju).unwrap().z - p0.z).length();
    assert!(moved < 1.5, "rooted in place (moved {moved})");
}

#[test]
fn a_portal_passes_a_falling_anvil() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(20.0, G, 20.0));
    // Floor portal under the drop point, exit ring on a wall facing +x.
    let a = PortalEnd::new(Vec3::new(0.0, G + 3.0, -8.0), Vec3::Y, Vec3::Z, 2.0);
    let b = PortalEnd::new(Vec3::new(-10.0, G + 4.0, 10.0), Vec3::X, Vec3::Y, 2.0);
    w.create_portal(A, a, b, 12.0, None).unwrap();
    let anvil = cast_at(&mut w, A, r("anvil", Form::Anvil, &[], Element::Light, 1.0), Vec3::new(0.0, G, -8.0), None);
    let (_, ev) = run_until(&mut w, 240, |_, ev| ev.iter().any(|e| matches!(e, GameEvent::Teleported { who: Traveller::Construct(c), .. } if *c == anvil)));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Teleported { who: Traveller::Construct(c), .. } if *c == anvil)), "the anvil should fall through the floor portal");
    let v = w.constructs.get(anvil).unwrap().vel;
    assert!(v.x > 3.0, "and fly out of the wall portal sideways (vel {v})");
}

#[test]
fn link_chains_the_player_to_the_anvil() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -8.0));
    run(&mut w, 30);
    let anvil = cast_at(&mut w, A, r("chained anvil", Form::Anvil, &[Mechanic::Link], Element::Light, 1.0), Vec3::new(0.0, G, -8.0), Some(TargetHint::Player(B)));
    let (_, ev) = run_until(&mut w, 240, |_, ev| ev.iter().any(|e| matches!(e, GameEvent::LinkCreated { .. })));
    let linked = ev.iter().any(|e| match e {
        GameEvent::LinkCreated { link } => {
            (link.a == LinkEnd::Player(B) && link.b == LinkEnd::Construct(anvil)) || (link.b == LinkEnd::Player(B) && link.a == LinkEnd::Construct(anvil))
        }
        _ => false,
    });
    assert!(linked, "B should be chained to the anvil");
}

#[test]
fn black_hole_pulls_then_collapses_at_end_of_life() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(4.5, G, -9.0));
    run(&mut w, 30);
    let id = cast_at(&mut w, A, r("black hole", Form::Blob0, &[Mechanic::Attract, Mechanic::Explodes], Element::Light, 1.0), Vec3::new(0.0, G, -6.0), None);
    assert!(w.fields.iter().any(|f| f.construct == Some(id) && f.strength > 0.0), "Attract makes a field");
    let life = w.constructs.get(id).unwrap().life;
    let mut early = run(&mut w, 60);
    let d0 = (pos(&w, B) - cpos(&w, id).unwrap()).length();
    early.extend(run(&mut w, ((life - 2.0) * 120.0) as u32));
    assert!(!early.iter().any(|e| matches!(e, GameEvent::Explosion { source: Some(s), .. } if *s == A)), "a black hole doesn't pop on contact");
    let hole = cpos(&w, id).unwrap();
    let d1 = (pos(&w, B) - hole).length();
    assert!(d1 < d0 - 1.0, "B is pulled in: {d0} -> {d1}");
    let late = run(&mut w, 300);
    assert!(late.iter().any(|e| matches!(e, GameEvent::Explosion { source: Some(s), radius, .. } if *s == A && *radius > 3.0)), "collapses at end of life");
}

// ------------------------------------------------------------------------------------ physics

#[test]
fn an_anvil_lands_and_stacks_on_a_platform() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, A, Vec3::new(10.0, G, 6.0));
    let plat = cast_at(&mut w, A, r("platform", Form::Platform, &[], Element::Light, 4.0), Vec3::new(10.0, G + 3.0, 14.0), None);
    run(&mut w, 40);
    let top = w.constructs.get(plat).unwrap().bounds().max.y;
    let pc = w.constructs.get(plat).unwrap().center();
    let anvil = cast_at(&mut w, A, r("anvil", Form::Anvil, &[], Element::Light, 1.0), Vec3::new(pc.x, G, pc.z), None);
    run(&mut w, 150);
    let c = w.constructs.get(anvil).expect("anvil resting");
    let bottom = c.bounds().min.y;
    assert!((bottom - top).abs() < 0.3, "anvil should rest on the platform: bottom {bottom} top {top}");
}

#[test]
fn a_boulder_rolls_down_a_slope() {
    let mut w = duel(2);
    // The flat arena's 45° stair ramp rises along +x from x = -6 at z ∈ [-24, -20].
    place(&mut w, A, Vec3::new(4.0, G + 8.0, -22.0));
    let e = eye(&w, A);
    face(&mut w, A, Vec3::new(-1.0, -0.2, 0.0));
    let id = w.cast(A, r("boulder", Form::Boulder, &[], Element::Light, 1.6), None, Aim::new(e, Vec3::new(-1.0, -0.2, 0.0).normalize())).unwrap();
    run(&mut w, 60);
    let p0 = cpos(&w, id).unwrap();
    run(&mut w, 60);
    let p1 = cpos(&w, id).unwrap_or(p0);
    assert!(p1.x < p0.x - 0.5 || p1.y < p0.y - 0.5, "boulder keeps going downhill: {p0} -> {p1}");
}

#[test]
fn fire_spreads_through_the_tree_and_burns_it_away() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(-26.0, G, 26.0));
    run(&mut w, 30);
    // A fire bomb into the canopy.
    let at = Vec3::new(-32.0, G + 11.0, 32.0);
    cast_at(&mut w, A, r("fire bomb", Form::Bomb, &[], Element::Fire, 0.7), at, None);
    let mut max_burning = 0;
    let ev = {
        let mut all = Vec::new();
        for _ in 0..(8 * 120) {
            all.extend_from_slice(w.tick(&[]));
            max_burning = max_burning.max(w.constructs.chemistry().burning().len());
        }
        all
    };
    assert!(max_burning >= 10, "fire should spread through leaves (max burning {max_burning})");
    let near_tree = ev.iter().filter(|e| matches!(e, GameEvent::Carve { op } if matches!(op.shape, bd_voxel::CarveShape::Sphere { center, .. } if (center - at).length() < 12.0))).count();
    assert!(near_tree >= 10, "burnt voxels carve away ({near_tree})");
}

// ------------------------------------------------------------------------------------ Transforms

#[test]
fn transformer_drives_in_transforms_close_and_fights() {
    let mut w = duel(2);
    // Open ground (the lab's stair ramp is at x ∈ [-6, 2], z ∈ [-24, -20]).
    place(&mut w, A, Vec3::new(12.0, G, -8.0));
    place(&mut w, B, Vec3::new(12.0, G, 20.0));
    run(&mut w, 30);
    let id = cast_at(&mut w, A, r("robot in disguise", Form::Vehicle, &[Mechanic::Transforms], Element::Light, 3.0), Vec3::new(12.0, G, -4.0), Some(TargetHint::Player(B)));
    let mut max_speed = 0.0f32;
    let (t, _) = run_until(&mut w, 900, |w, _| {
        let c = w.constructs.get(id).unwrap();
        if c.comp.xform.mode == XMode::Alt {
            max_speed = max_speed.max(Vec3::new(c.vel.x, 0.0, c.vel.z).length());
        }
        c.comp.xform.mode == XMode::Robot
    });
    assert!(max_speed > 12.0, "alt mode drives fast ({max_speed})");
    assert!(t < 900, "it should transform near the target");
    let d = (cpos(&w, id).unwrap() - pos(&w, B)).length();
    assert!(d < 16.0, "transformed ~12 m out (at {d})");
    let (_, ev) = run_until(&mut w, 600, |_, ev| hits_from(ev, id, B) > 0);
    assert!(hits_from(&ev, id, B) >= 1, "robot mode fights");
}

#[test]
fn roll_out_toggles_every_transformer_and_combine_makes_a_giant() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(40.0, G, 40.0));
    run(&mut w, 30);
    let mut ids = Vec::new();
    for k in 0..3 {
        let x = -4.0 + 4.0 * k as f32;
        let id = cast_at(&mut w, A, r("truck", Form::Vehicle, &[Mechanic::Transforms], Element::Light, 3.0), Vec3::new(x, G, -6.0), None);
        ids.push(id);
        run(&mut w, 30);
    }
    run(&mut w, 60);
    let modes: Vec<XMode> = ids.iter().map(|i| w.constructs.get(*i).unwrap().comp.xform.mode).collect();
    cast_at(&mut w, A, r("roll out", Form::Blob0, &[], Element::Light, 0.5), Vec3::new(0.0, G, -3.0), None);
    run(&mut w, 150);
    for (i, m) in ids.iter().zip(modes) {
        let c = w.constructs.get(*i).unwrap();
        assert_ne!(c.comp.xform.mode, m, "roll out toggles every transformer");
    }
    cast_at(&mut w, A, r("combine", Form::Blob0, &[], Element::Light, 0.5), Vec3::new(0.0, G, -3.0), None);
    run(&mut w, 300);
    let alive: Vec<ConstructId> = ids.iter().copied().filter(|i| w.constructs.get(*i).is_some()).collect();
    assert_eq!(alive.len(), 1, "three transformers merge into one");
    let giant = w.constructs.get(alive[0]).unwrap();
    assert!(giant.comp.xform.combined && giant.comp.xform.mode == XMode::Robot);
    assert!(giant.half.y > 3.0, "Kaiju-scale (half height {})", giant.half.y);
}

#[test]
fn transforming_mid_air_lands_with_a_slam() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(2.0, G, -6.0));
    run(&mut w, 30);
    let id = cast_at(&mut w, A, r("car", Form::Vehicle, &[Mechanic::Transforms], Element::Light, 3.0), Vec3::new(0.0, G, -4.0), None);
    run(&mut w, 30);
    // Launch it into the air in vehicle mode, then transform on the way down.
    if let Some(c) = w.constructs.get_mut(id) {
        c.pos = Vec3::new(0.0, G + 12.0, -6.0);
        c.vel = Vec3::ZERO;
        c.comp.xform.mode = XMode::Alt;
        c.comp.xform.since = 0.0;
        bd_sim::constructs::components::request_transform(c);
    }
    let (_, ev) = run_until(&mut w, 240, |_, ev| ev.iter().any(|e| matches!(e, GameEvent::Explosion { source: Some(s), element: Element::Earth, .. } if *s == A)));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Explosion { source: Some(s), element: Element::Earth, .. } if *s == A)), "landing slam shockwave");
    assert!(health(&w, B) < 100.0, "the slam hits B");
}

#[test]
fn blade_on_a_kaiju_nape_is_a_weak_point() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -40.0));
    run(&mut w, 30);
    let kaiju = cast_at(&mut w, B, r("godzilla", Form::Kaiju, &[], Element::Light, 12.0), Vec3::new(0.0, G, -20.0), None);
    run(&mut w, 160);
    let k = w.constructs.get(kaiju).unwrap();
    let (center, rot, half, i0) = (k.center(), k.rot, k.half, k.integrity);
    // Belly vs nape (top rear), same blade hit.
    w.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: kaiju, source: Some(A), damage: 5.0, impulse: Vec3::ZERO, blade: true, point: center });
    let i1 = w.constructs.get(kaiju).unwrap().integrity;
    let nape = center + rot * Vec3::new(0.0, half.y * 0.9, half.z * 0.5);
    w.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: kaiju, source: Some(A), damage: 5.0, impulse: Vec3::ZERO, blade: true, point: nape });
    let i2 = w.constructs.get(kaiju).unwrap().integrity;
    assert!(((i1 - i2) - 3.0 * (i0 - i1)).abs() < 1e-3, "nape takes ×3: body {} nape {}", i0 - i1, i1 - i2);
}
