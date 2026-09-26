//! World-layer hooks for the construct system: portal gun, attacker traits on construct hits,
//! generic Guard / Bounce deflection, transformer replication, fire-voxel events, own blasts vs own
//! structures, slice halves, authored robot shapes, off-tick arena regeneration.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::shape::{Prim, PrimKind, ShapeSpec};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::{ConstructHit, Explosion};
use bd_sim::events::GameEvent;
use bd_sim::rules::{MatchPhase, WillpowerMode};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const G: f32 = 8.0;

fn duel() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    for k in 1..=2u8 {
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
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
    }
    all
}

fn eye(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().eye(w.params())
}

fn cast_at(w: &mut World, who: PlayerId, r: Recipe, at: Vec3) -> ConstructId {
    let e = eye(w, who);
    w.cast(who, r, None, Aim::new(e, (at - e).normalize())).unwrap()
}

fn look_at(w: &World, who: PlayerId, at: Vec3) -> (f32, f32) {
    let d = (at - eye(w, who)).normalize();
    ((-d.x).atan2(-d.z), d.y.asin())
}

/// Tap PRIMARY once aimed at `at`, then let the shot fly.
fn shoot(w: &mut World, who: PlayerId, at: Vec3) -> Vec<GameEvent> {
    let (yaw, pitch) = look_at(w, who, at);
    let mut ev = Vec::new();
    ev.extend_from_slice(w.tick(&[(who, PlayerInput { yaw, pitch, buttons: buttons::PRIMARY, ..Default::default() })]));
    for _ in 0..60 {
        ev.extend_from_slice(w.tick(&[(who, PlayerInput { yaw, pitch, ..Default::default() })]));
    }
    ev
}

#[test]
fn portal_gun_places_a_then_b_then_moves_ends() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(10.0, G, 8.0));
    place(&mut w, B, Vec3::new(-30.0, G, 30.0));
    run(&mut w, 30);
    let gun = recipe("portal gun", Form::Gun, &[Mechanic::Shoots, Mechanic::Portal], Element::Light, 0.6);
    w.cast(A, gun, None, Aim::new(eye(&w, A), Vec3::NEG_Z)).unwrap();
    run(&mut w, 30);
    assert!(w.portals.is_empty(), "a portal gun opens nothing at cast time");
    let ev = shoot(&mut w, A, Vec3::new(10.0, G, 2.0));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::PortalPlaced { slot: 0, .. })), "ring A on the floor");
    assert!(w.portals.is_empty());
    let ev = shoot(&mut w, A, Vec3::new(13.0, G, -2.0));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::PortalPlaced { slot: 1, .. })));
    let id = ev.iter().find_map(|e| match e {
        GameEvent::PortalOpened { id, owner, .. } if *owner == A => Some(*id),
        _ => None,
    });
    let id = id.expect("pair opens");
    let a0 = w.portals[0].ends[0].pos;
    let ev = shoot(&mut w, A, Vec3::new(6.0, G, 1.0));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::PortalOpened { id: i, .. } if *i == id)), "same pair, end moved");
    assert_eq!(w.portals.len(), 1);
    assert!((w.portals[0].ends[0].pos - a0).length() > 1.0, "third shot moved end A");
}

#[test]
fn pierce_finds_weak_points() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(0.0, G, -40.0));
    run(&mut w, 30);
    let kaiju = cast_at(&mut w, B, recipe("godzilla", Form::Kaiju, &[], Element::Light, 12.0), Vec3::new(0.0, G, -20.0));
    run(&mut w, 160);
    let k = w.constructs.get(kaiju).unwrap();
    let (center, rot, half, i0) = (k.center(), k.rot, k.half, k.integrity);
    let hit = |point, pierce| ConstructHit { pierce, blunt: false, target: kaiju, source: Some(A), damage: 5.0, impulse: Vec3::ZERO, blade: false, point };
    w.apply_construct_hit(hit(center, true));
    let i1 = w.constructs.get(kaiju).unwrap().integrity;
    let nape = center + rot * Vec3::new(0.0, half.y * 0.9, half.z * 0.5);
    w.apply_construct_hit(hit(nape, true));
    let i2 = w.constructs.get(kaiju).unwrap().integrity;
    assert!(((i1 - i2) - 3.0 * (i0 - i1)).abs() < 1e-3, "pierce at the nape ×3: body {} nape {}", i0 - i1, i1 - i2);
    w.apply_construct_hit(hit(nape, false));
    let i3 = w.constructs.get(kaiju).unwrap().integrity;
    assert!(((i2 - i3) - (i0 - i1)).abs() < 1e-3, "no trait → no weak point");
}

#[test]
fn bounce_and_guard_constructs_deflect_projectiles() {
    for (name, form, mech) in [("trampoline", Form::Trampoline, &[Mechanic::Bounce][..]), ("mirror wall", Form::Wall, &[Mechanic::Guard][..])] {
        let mut w = duel();
        place(&mut w, A, Vec3::new(10.0, G, 8.0));
        place(&mut w, B, Vec3::new(10.0, G, -10.0));
        run(&mut w, 30);
        // B builds it between them, facing A (structures face their caster's look).
        let (yaw, pitch) = look_at(&w, B, Vec3::new(10.0, G, 8.0));
        w.tick(&[(B, PlayerInput { yaw, pitch, ..Default::default() })]);
        let e = eye(&w, B);
        let at = Vec3::new(10.0, G, -1.0);
        let mut r = recipe(name, form, mech, Element::Light, if form == Form::Wall { 6.0 } else { 3.0 });
        r.scale = r.scale.max(1.0);
        w.cast(B, r, None, Aim::new(e, (at - e).normalize())).unwrap();
        run(&mut w, 60);
        let target = if form == Form::Trampoline { Vec3::new(10.0, G + 0.2, -1.0) } else { Vec3::new(10.0, G + 1.5, -1.0) };
        let ev = shoot(&mut w, A, target);
        let d = ev.iter().find_map(|e| match e {
            GameEvent::Deflected { guard, .. } => Some(*guard),
            _ => None,
        });
        assert!(d.is_some(), "{name} deflects");
        assert_eq!(d, Some(form == Form::Wall), "{name}: guard flag");
        assert!(ev.iter().any(|e| matches!(e, GameEvent::ProjectileFired { owner, .. } if *owner == B)), "{name}: round now belongs to B");
    }
}

#[test]
fn transformer_state_replicates_with_an_event() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(-40.0, G, 40.0)); // far: the autonomous machine stays in alt mode
    run(&mut w, 30);
    let id = cast_at(&mut w, A, recipe("car", Form::Vehicle, &[Mechanic::Transforms], Element::Light, 3.0), Vec3::new(0.0, G, -6.0));
    run(&mut w, 120);
    let snap = |w: &World| w.snapshot_for(Some(A)).constructs.iter().find(|c| c.id == id.0).map(|c| c.xform).unwrap();
    let before = snap(&w);
    assert_eq!(before & 128, 0, "alt mode");
    assert!(bd_sim::constructs::components::request_transform(w.constructs.get_mut(id).unwrap()));
    let ev = run(&mut w, 30);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Transformed { id: i, robot: true, .. } if *i == id)));
    let mid = snap(&w);
    assert!(mid & 128 != 0 && mid & 32 != 0, "target robot, busy: {mid:#x}");
    run(&mut w, 150);
    assert_eq!(snap(&w) & 31, 31, "fully robot");
    // A client mirror applying the snapshot sees the same mode / progress.
    let mut client = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() });
    for e in w.construct_manifest() {
        client.apply_event(&e);
    }
    client.apply_snapshot(&w.snapshot_for(Some(A)), Some(A));
    let x = client.constructs.get(id).unwrap().comp.xform;
    assert!(x.progress > 0.99 && x.mode == bd_sim::constructs::components::XMode::Robot);
}

#[test]
fn burning_terrain_emits_fire_voxel_changes() {
    let mut w = duel();
    // The lab's tree (north-west): find a leaf voxel.
    let mut leaf = None;
    'find: for y in 20..60 {
        for z in 60..80 {
            for x in -80..-60 {
                let v = bd_core::IVec3::new(x, y, z);
                let m = w.voxels.get(v);
                if m != 0 && bd_voxel::material_def(m).debris == bd_voxel::DebrisClass::Leaves {
                    leaf = Some(v);
                    break 'find;
                }
            }
        }
    }
    let leaf = leaf.expect("the lab has a tree");
    w.ignite_terrain((leaf.as_vec3() + Vec3::splat(0.5)) * 0.5, 1.5, Some(A));
    let ev = run(&mut w, 600);
    let (lit, out): (usize, usize) = ev.iter().fold((0, 0), |acc, e| match e {
        GameEvent::FireVoxels { lit, out } => (acc.0 + lit.len(), acc.1 + out.len()),
        _ => acc,
    });
    assert!(lit > 0, "lit voxels announced");
    assert!(out > 0, "burnt-out voxels announced");
}

#[test]
fn own_blast_damages_own_structure_but_not_own_creature() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    run(&mut w, 30);
    let wall = cast_at(&mut w, A, recipe("wall", Form::Wall, &[], Element::Light, 6.0), Vec3::new(0.0, G, -8.0));
    run(&mut w, 60);
    let i0 = w.constructs.get(wall).unwrap().integrity;
    let at = w.constructs.get(wall).unwrap().center();
    w.explode(Explosion { pos: at + Vec3::new(0.0, 0.0, 1.0), radius: 4.0, damage: 20.0, knockback: 30.0, source: Some(A), construct: None, element: Element::Fire, carve_radius: 0.0, self_damage: false });
    assert!(w.constructs.get(wall).is_none_or(|c| c.integrity < i0), "own wall hurt / knocked loose");
}

#[test]
fn slice_halves_carry_their_cut() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    run(&mut w, 30);
    let wall = cast_at(&mut w, B, recipe("wall", Form::Wall, &[], Element::Light, 6.0), Vec3::new(0.0, G, -8.0));
    run(&mut w, 60);
    let c = w.constructs.get(wall).unwrap();
    let (at, half) = (c.center(), c.half);
    w.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: wall, source: Some(A), damage: 6.0, impulse: Vec3::ZERO, blade: true, point: at + Vec3::new(0.5, 0.0, 0.0) });
    let sliced: Vec<_> = w
        .events()
        .iter()
        .filter_map(|e| match e {
            GameEvent::ConstructSliced { id, parent, cut } if *parent == wall => Some((*id, *cut)),
            _ => None,
        })
        .collect();
    assert_eq!(sliced.len(), 2);
    assert_eq!(sliced[0].1.side, -1);
    assert_eq!(sliced[1].1.side, 1);
    assert_eq!(sliced[0].1.parent_half, half);
    for (id, cut) in &sliced {
        assert_eq!(w.constructs.get(*id).unwrap().slice, Some(*cut));
    }
    assert!(w.construct_manifest().iter().any(|e| matches!(e, GameEvent::ConstructSliced { .. })), "late joiners get cuts");
}

#[test]
fn authored_robot_shape_is_stored_replicated_and_used() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(-40.0, G, 40.0));
    run(&mut w, 30);
    // A tall authored robot (construct voxels: 0.25 m): 1 × 4 × 0.8 m.
    let robot = ShapeSpec {
        prims: vec![Prim { kind: PrimKind::Box, center: Vec3::new(0.0, 8.0, 0.0), size: Vec3::new(4.0, 16.0, 3.2), rot_deg: Vec3::ZERO, tint: Default::default(), part: 0, subtract: false }],
        parts: vec![],
    };
    let e = eye(&w, A);
    let at = Vec3::new(0.0, G, -6.0);
    let id = w
        .cast_shaped(A, recipe("optimus", Form::Vehicle, &[Mechanic::Transforms], Element::Light, 3.0), None, Some(robot.clone()), Aim::new(e, (at - e).normalize()))
        .unwrap();
    assert!(w.events().iter().any(|e| matches!(e, GameEvent::ConstructShapes { id: i, .. } if *i == id)));
    assert!(w.construct_manifest().iter().any(|e| matches!(e, GameEvent::ConstructShapes { .. })));
    run(&mut w, 120);
    assert!(bd_sim::constructs::components::request_transform(w.constructs.get_mut(id).unwrap()));
    run(&mut w, 200);
    let c = w.constructs.get(id).unwrap();
    assert!((c.half - Vec3::new(0.5, 2.0, 0.4)).length() < 1e-3, "robot box from the authored shape: {}", c.half);
    assert_eq!(c.alt_shape.as_deref(), Some(&robot));
}

#[test]
fn rematch_terrain_is_built_off_the_tick() {
    let mut w = duel();
    w.cfg.timings.ended_secs = 1.0;
    w.cfg.rules.ko_target = 1;
    w.request_carve(bd_sim::combat::CarveRequest { center: Vec3::new(0.0, G, 0.0), radius: 2.0, power: 1.0 });
    run(&mut w, 2);
    w.apply_hit(bd_sim::combat::HitInfo::new(B, Some(A), bd_sim::combat::HitKind::Shot, 1.0, 0.0, Vec3::X, Vec3::ZERO));
    w.player_mut(B).unwrap().mv.pos.y = -100.0; // blast zone
    run(&mut w, 1);
    assert_eq!(w.match_state.phase, MatchPhase::Ended);
    // The worker is building now; the rematch swaps it in without building on the tick.
    let ev = run(&mut w, 125);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ArenaReset { .. })));
    assert_eq!(w.match_state.phase, MatchPhase::Warmup);
    assert_eq!(w.pending_carves().count(), 0);
}

#[test]
fn icon_shapes_are_normalised_once_at_cast() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(0.0, G, 0.0));
    place(&mut w, B, Vec3::new(-40.0, G, 40.0));
    run(&mut w, 30);
    let bx = |h: f32, l: f32| ShapeSpec {
        prims: vec![Prim { kind: PrimKind::Box, center: Vec3::new(0.0, h * 0.5, 0.0), size: Vec3::new(l * 0.4, h, l), rot_deg: Vec3::ZERO, tint: Default::default(), part: 0, subtract: false }],
        parts: vec![],
    };
    // Authored in arbitrary units (10× too big): a 120-unit truck and its 160-unit robot.
    let (truck, robot) = (bx(40.0, 120.0), bx(160.0, 40.0));
    let mut r = recipe("optimus", Form::Vehicle, &[Mechanic::Transforms], Element::Light, 3.0);
    r.model = bd_core::mechanics::ModelSource::Icon("optimus".into());
    let e = eye(&w, A);
    let at = Vec3::new(0.0, G, -6.0);
    let id = w.cast_shaped(A, r, Some(truck.clone()), Some(robot.clone()), Aim::new(e, (at - e).normalize())).unwrap();
    let c = w.constructs.get(id).unwrap();
    let shape = c.shape.as_ref().expect("stored shape");
    let k = bd_sim::constructs::shape_scale(&c.recipe, shape);
    assert!((k - 1.0).abs() < 0.02, "stored icon spec is real-size: rescale {k}");
    let f = bd_shapes::icon_scale_factor(&c.recipe, &truck);
    let alt = c.alt_shape.as_deref().expect("robot");
    assert!((alt.prims[0].size.y - robot.prims[0].size.y * f).abs() < 1e-3, "robot uses the vehicle's factor");
}
