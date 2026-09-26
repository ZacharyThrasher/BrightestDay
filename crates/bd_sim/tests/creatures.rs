//! Creature scenarios (constructs/creature.rs, ai.rs, temperament.rs, swarm.rs): every gait reaches
//! its target over the voxel world, every attack lands inside the animation's hit window,
//! temperaments behave measurably differently, swarms split, kaiju stomps crater, cages root kaiju,
//! and walkers never fall off islands. Flat lab arena: ground top at y = 8, void edge at |x|,|z| = 48.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe, Temperament};
use bd_core::voxel::{VoxelQuery, world_to_voxel};
use bd_core::{ConstructId, DT, IVec3, PlayerId, Quat, Vec3};
use bd_sim::bots::recipe;
use bd_sim::constructs::ai::{self, AttackKind, Clip, phase};
use bd_sim::constructs::{SpawnParams, creature};
use bd_sim::events::GameEvent;
use bd_sim::rng::Fnv64;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn duel_on(arena: ArenaSource) -> World {
    let cfg = SimConfig { arena, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=2u8 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
        p.willpower = 100.0;
    }
    w
}

fn duel() -> World {
    duel_on(ArenaSource::Flat { size_m: 96.0 })
}

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
}

/// Keep a player fixed (tests measure creatures, not knockback).
fn pin(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.health = 100.0;
    p.alive = true;
}

fn creature_recipe(form: Form, mechs: &[Mechanic], scale: f32) -> Recipe {
    recipe(&format!("{form:?}"), form, mechs, Element::Light, scale)
}

fn with_temper(mut r: Recipe, t: Temperament, i: f32) -> Recipe {
    r.temperament = t;
    r.intensity = i;
    r
}

fn spawn(w: &mut World, owner: PlayerId, r: Recipe, pos: Vec3) -> ConstructId {
    let colossal = false;
    let tick = w.tick;
    w.constructs.spawn(SpawnParams {
        owner,
        recipe: r,
        shape: None,
        pos,
        rot: Quat::IDENTITY,
        vel: Vec3::ZERO,
        target: None,
        cost: 20.0,
        integrity: 500.0,
        forming: 0.05,
        life: 1000.0,
        tick,
        colossal,
        model: None,
    })
}

fn hits_from(evs: &[GameEvent], id: ConstructId, target: PlayerId) -> bool {
    evs.iter().any(|e| matches!(e, GameEvent::Hit { target: t, construct: Some(c), .. } if *c == id && *t == target))
}

fn hflat(v: Vec3) -> f32 {
    Vec3::new(v.x, 0.0, v.z).length()
}

// ------------------------------------------------------------------------------------ gaits

/// Seconds until the creature first hits B standing still `dist` metres away (None = never in `secs`).
fn time_to_hit(r: Recipe, start: Vec3, b: Vec3, secs: f32) -> Option<f32> {
    let mut w = duel();
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    place(&mut w, B, b);
    let id = spawn(&mut w, A, r, start);
    for i in 0..(secs / DT) as u32 {
        pin(&mut w, B, b);
        let evs = w.tick(&[]).to_vec();
        if hits_from(&evs, id, B) {
            return Some(i as f32 * DT);
        }
        w.constructs.get(id)?;
    }
    None
}

#[test]
fn every_gait_reaches_and_hits_its_target() {
    let b = Vec3::new(0.0, GROUND, 12.0);
    let cases: &[(Form, &[Mechanic], f32, f32)] = &[
        (Form::Biped, &[], 1.8, 0.0),
        (Form::Quadruped, &[], 1.2, 0.0),
        (Form::Arachnid, &[], 1.0, 0.0),
        (Form::Insect, &[], 0.6, 1.5),
        (Form::Serpent, &[], 1.5, 0.0),
        (Form::Blob, &[], 1.0, 0.0),
        (Form::Bird, &[], 1.0, 3.0),
        (Form::Fish, &[], 1.2, 0.0),
        (Form::Kaiju, &[], 12.0, 0.0),
    ];
    for &(form, mechs, scale, lift) in cases {
        let start = Vec3::new(0.0, GROUND + lift, -12.0);
        let t = time_to_hit(creature_recipe(form, mechs, scale), start, b, 12.0);
        assert!(t.is_some(), "{form:?} never reached/hit its target 24 m away");
    }
}

#[test]
fn walkers_hop_one_metre_ledges() {
    let mut w = duel();
    // A 1 m block (2 voxels) the target stands in the middle of: the creature must hop onto it.
    let (lo, hi) = (world_to_voxel(Vec3::new(-3.0, GROUND, 6.0)), world_to_voxel(Vec3::new(3.0, GROUND + 1.0, 14.0)));
    w.voxels.fill_box(lo, hi - IVec3::new(1, 1, 1), 3);
    let b = Vec3::new(0.0, GROUND + 1.0, 12.0);
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    place(&mut w, B, b);
    let id = spawn(&mut w, A, creature_recipe(Form::Quadruped, &[], 1.2), Vec3::new(0.0, GROUND, -8.0));
    let mut top = f32::MIN;
    let mut hit = false;
    for _ in 0..(10.0 / DT) as u32 {
        pin(&mut w, B, b);
        let evs = w.tick(&[]).to_vec();
        hit |= hits_from(&evs, id, B);
        top = top.max(w.constructs.get(id).unwrap().pos.y);
    }
    assert!(top >= GROUND + 0.95, "never got onto the ledge (max y {top})");
    assert!(hit, "never bit the target on the ledge");
}

#[test]
fn walkers_stop_at_the_void_edge_and_fliers_do_not_need_to() {
    // Target hovering beyond the island edge (x > 48): walkers pace at the edge, never fall.
    let mut w = duel();
    w.cfg.check_invariants = false; // 5 creatures for one player (slot limits are the world's job)
    // A bottomless chasm x ∈ [4, 14) m across the whole lab; the target waits on the far side.
    let (lo, hi) = (world_to_voxel(Vec3::new(4.0, 0.0, -48.0)), world_to_voxel(Vec3::new(14.0, GROUND, 48.0)));
    w.voxels.fill_box(lo, hi - IVec3::ONE, 0);
    let b = Vec3::new(20.0, GROUND, 0.0);
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    place(&mut w, B, b);
    let ids: Vec<ConstructId> =
        [(Form::Quadruped, 1.2), (Form::Biped, 1.8), (Form::Serpent, 1.5), (Form::Arachnid, 1.0), (Form::Blob, 1.0)]
            .iter()
            .enumerate()
            .map(|(i, &(f, s))| spawn(&mut w, A, creature_recipe(f, &[], s), Vec3::new(-6.0, GROUND, -6.0 + 3.0 * i as f32)))
            .collect();
    let flier = spawn(&mut w, A, creature_recipe(Form::Bird, &[], 1.0), Vec3::new(-6.0, GROUND + 3.0, 8.0));
    let mut crossed = false;
    for _ in 0..(8.0 / DT) as u32 {
        pin(&mut w, B, b);
        w.tick(&[]);
        crossed |= w.constructs.get(flier).is_some_and(|c| c.pos.x > 14.0);
    }
    for id in ids {
        let c = w.constructs.get(id).expect("walker fell into the chasm");
        assert!(c.pos.y > GROUND - 0.6 && c.pos.x < 4.2, "walker went over the edge: {:?} {:?}", c.recipe.form, c.pos);
        assert!(c.pos.x > 1.0, "walker should have come to the edge: {:?} {:?}", c.recipe.form, c.pos);
    }
    assert!(crossed, "the flier simply flies over");
}

// ------------------------------------------------------------------------------------ attacks

/// Run until `kind` telegraphs; return (ticks from telegraph to first hit on B, timing).
fn attack_window(r: Recipe, start: Vec3, b: Vec3, kind: AttackKind, block: &[AttackKind]) -> (u32, ai::Timing) {
    let mut w = duel();
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    place(&mut w, B, b);
    let id = spawn(&mut w, A, r, start);
    let mut tele: Option<u64> = None;
    for _ in 0..(12.0 / DT) as u32 {
        pin(&mut w, B, b);
        if let Some(c) = w.constructs.get_mut(id) {
            for k in block {
                c.ai.cd[*k as usize] = 100.0;
            }
        }
        let tick = w.tick + 1;
        let evs = w.tick(&[]).to_vec();
        for e in &evs {
            if let GameEvent::CreatureAttack { id: cid, kind: k, phase: ph, .. } = e
                && *cid == id
                && *k == kind as u8
                && *ph == phase::TELEGRAPH
                && tele.is_none()
            {
                tele = Some(tick);
            }
        }
        if let Some(t0) = tele
            && hits_from(&evs, id, B)
        {
            let c = w.constructs.get(id).unwrap();
            return ((tick - t0) as u32, creature::timing(c, kind));
        }
    }
    panic!("{kind:?} never hit (telegraphed: {tele:?})");
}

#[test]
#[allow(clippy::type_complexity)]
fn every_attack_hits_inside_its_animation_window() {
    let z = |d: f32| Vec3::new(0.0, GROUND, d);
    let cases: Vec<(&str, Recipe, Vec3, Vec3, AttackKind, Vec<AttackKind>)> = vec![
        ("bite", creature_recipe(Form::Quadruped, &[], 1.2), z(0.0), z(-2.2), AttackKind::Bite, vec![]),
        ("swipe", creature_recipe(Form::Biped, &[], 1.8), z(0.0), z(-1.6), AttackKind::Swipe, vec![]),
        (
            "charge",
            creature_recipe(Form::Quadruped, &[Mechanic::Charge], 1.2),
            z(0.0),
            z(-10.0),
            AttackKind::Charge,
            vec![AttackKind::Bite],
        ),
        ("stomp", creature_recipe(Form::Biped, &[Mechanic::Stomp], 2.0), z(0.0), z(-2.5), AttackKind::Stomp, vec![AttackKind::Swipe]),
        (
            "tail",
            creature_recipe(Form::Quadruped, &[Mechanic::ArticulatedTail], 2.0),
            z(0.0),
            z(2.8),
            AttackKind::TailSweep,
            vec![AttackKind::Bite],
        ),
        ("breath", creature_recipe(Form::Quadruped, &[Mechanic::Breath], 1.5), z(0.0), z(-5.0), AttackKind::Breath, vec![AttackKind::Bite]),
        (
            "swoop",
            creature_recipe(Form::Bird, &[], 1.0),
            Vec3::new(0.0, GROUND + 4.0, 0.0),
            z(-8.0),
            AttackKind::Swoop,
            vec![AttackKind::Bite],
        ),
        ("grab", creature_recipe(Form::Quadruped, &[Mechanic::Grab], 1.2), z(0.0), z(-2.2), AttackKind::Grab, vec![]),
        ("constrict", creature_recipe(Form::Serpent, &[Mechanic::Constrict], 1.5), z(0.0), z(-2.5), AttackKind::Constrict, vec![]),
    ];
    for (name, r, start, b, kind, block) in cases {
        let (n, tm) = attack_window(r, start, b, kind, &block);
        let t = n as f32 * DT;
        assert!(
            t >= tm.anticipation - 1e-4 && t < tm.anticipation + tm.active + DT + 1e-4,
            "{name}: hit at clip time {t:.3}s outside the window [{:.3}, {:.3}]",
            tm.anticipation,
            tm.anticipation + tm.active
        );
    }
}

#[test]
fn sim_timing_is_the_animation_timing() {
    use bd_shapes::anim::{Attack, attack_timing};
    let clips = [
        (Clip::Bite, Attack::Bite),
        (Clip::Swipe, Attack::Swipe),
        (Clip::Charge, Attack::Charge),
        (Clip::Stomp, Attack::Stomp),
        (Clip::TailSweep, Attack::TailSweep),
        (Clip::Breath, Attack::Breath { aim: Vec3::NEG_Z }),
        (Clip::Lash, Attack::Lash),
    ];
    for (form, scale) in [(Form::Quadruped, 1.2), (Form::Biped, 1.8), (Form::Serpent, 2.0), (Form::Bird, 1.0), (Form::Kaiju, 14.0)] {
        let r = creature_recipe(form, &[], scale);
        let (spec, model) = bd_shapes::build_model(&r);
        for (clip, atk) in clips {
            let a = ai::attack_timing(model.rig.height, clip);
            let b = attack_timing(&model, atk);
            assert!(
                (a.anticipation - b.anticipation).abs() < 1e-5
                    && (a.active - b.active).abs() < 1e-5
                    && (a.recovery - b.recovery).abs() < 1e-5,
                "{form:?} {clip:?}"
            );
        }
        // With a shape (the game casts with one) the collider height the sim times from is the
        // model's rig height to within a voxel or two: < 1 tick of timing difference.
        let mut w = duel();
        let id = spawn(&mut w, A, r.clone(), Vec3::new(0.0, GROUND, 0.0));
        let c = w.constructs.get(id).unwrap();
        let h = creature::timing_height(c);
        let _ = spec;
        for (clip, _) in clips {
            let ours = ai::attack_timing(h, clip);
            let theirs = ai::attack_timing(model.rig.height, clip);
            assert!(
                (ours.anticipation - theirs.anticipation).abs() <= DT && (ours.impact() - theirs.impact()).abs() <= DT,
                "{form:?} {clip:?}: sim h {h} vs rig h {}",
                model.rig.height
            );
        }
    }
}

// ------------------------------------------------------------------------------------ temperaments

/// Final horizontal distance to B after `secs`, and fraction of ticks the creature stood still.
fn temper_run(t: Temperament, i: f32, start: Vec3, b: Vec3, secs: f32) -> (f32, f32) {
    let mut w = duel();
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    place(&mut w, B, b);
    let id = spawn(&mut w, A, with_temper(creature_recipe(Form::Quadruped, &[], 1.2), t, i), start);
    let n = (secs / DT) as u32;
    let mut still = 0;
    for _ in 0..n {
        pin(&mut w, B, b);
        w.tick(&[]);
        if hflat(w.constructs.get(id).unwrap().vel) < 0.3 {
            still += 1;
        }
    }
    (hflat(w.constructs.get(id).unwrap().pos - b), still as f32 / n as f32)
}

#[test]
fn fearful_keeps_its_distance() {
    let b = Vec3::new(0.0, GROUND, 4.0);
    let (calm, _) = temper_run(Temperament::Calm, 0.6, Vec3::new(0.0, GROUND, 0.0), b, 4.0);
    let (fear, _) = temper_run(Temperament::Fearful, 0.8, Vec3::new(0.0, GROUND, 0.0), b, 4.0);
    assert!(calm < 3.0, "calm closes in ({calm})");
    assert!(fear > 8.0, "fearful kites away ({fear})");
}

#[test]
fn lazy_stops_and_sits() {
    // 40 m apart, 3 s: a calm dog runs the whole time; a very lazy one mostly sits in the road.
    let b = Vec3::new(0.0, GROUND, 30.0);
    let (d_calm, still_calm) = temper_run(Temperament::Calm, 0.6, Vec3::new(0.0, GROUND, -10.0), b, 3.0);
    let (d_lazy, still_lazy) = temper_run(Temperament::Lazy, 1.0, Vec3::new(0.0, GROUND, -10.0), b, 3.0);
    assert!(still_lazy > 0.4 && still_calm < 0.2, "lazy still {still_lazy}, calm still {still_calm}");
    assert!(d_lazy > d_calm + 5.0, "lazy is a roadblock, not a hunter ({d_lazy} vs {d_calm})");
}

#[test]
fn protective_body_blocks_a_projectile() {
    let mut w = duel();
    let a = Vec3::new(0.0, GROUND, 0.0);
    let b = Vec3::new(0.0, GROUND, -16.0);
    place(&mut w, A, a);
    place(&mut w, B, b);
    let id =
        spawn(&mut w, A, with_temper(creature_recipe(Form::Biped, &[], 1.8), Temperament::Protective, 0.6), Vec3::new(3.0, GROUND, 1.0));
    // Let it take up its guard post between A and B.
    for _ in 0..(2.5 / DT) as u32 {
        pin(&mut w, A, a);
        pin(&mut w, B, b);
        w.tick(&[]);
    }
    let c = w.constructs.get(id).unwrap();
    assert!(c.pos.z < -1.0 && c.pos.x.abs() < 0.6, "guards on the threat line: {:?}", c.pos);
    // B fires a ring blast straight at A's chest.
    let eye_b = b + Vec3::Y * 1.6;
    let chest_a = a + Vec3::Y * 0.9;
    let d = (chest_a - eye_b).normalize();
    let mut inp = bd_core::input::PlayerInput { buttons: bd_core::input::buttons::PRIMARY, ..Default::default() };
    inp.yaw = (-d.x).atan2(-d.z);
    inp.pitch = d.y.asin();
    let mut blocked = false;
    let mut a_hit = false;
    for k in 0..60 {
        pin(&mut w, A, a);
        pin(&mut w, B, b);
        let i = if k < 1 { inp } else { bd_core::input::PlayerInput { yaw: inp.yaw, pitch: inp.pitch, ..Default::default() } };
        let evs = w.tick(&[(B, i)]).to_vec();
        for e in &evs {
            match e {
                GameEvent::ConstructHit { id: cid, source: Some(s), .. } if *cid == id && *s == B => blocked = true,
                GameEvent::Hit { target, source: Some(s), .. } if *target == A && *s == B => a_hit = true,
                _ => {}
            }
        }
    }
    assert!(blocked && !a_hit, "protective creature must eat the shot (blocked {blocked}, owner hit {a_hit})");
}

#[test]
fn chaotic_attacks_other_peoples_constructs() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    // B is far away; only B's wall is near the chaotic creature.
    let b = Vec3::new(40.0, GROUND, 40.0);
    place(&mut w, B, b);
    let wall = spawn(&mut w, B, recipe("wall", Form::Wall, &[], Element::Light, 6.0), Vec3::new(0.0, GROUND, 6.0));
    let id =
        spawn(&mut w, A, with_temper(creature_recipe(Form::Quadruped, &[], 1.2), Temperament::Chaotic, 0.8), Vec3::new(0.0, GROUND, 0.0));
    let calm = spawn(&mut w, A, creature_recipe(Form::Quadruped, &[], 1.2), Vec3::new(-6.0, GROUND, 0.0));
    let mut chaotic_hit = false;
    for _ in 0..(8.0 / DT) as u32 {
        pin(&mut w, B, b);
        let evs = w.tick(&[]).to_vec();
        // The chaotic creature's own attack cue followed by a hit on the wall.
        let attacked = evs.iter().any(|e| matches!(e, GameEvent::CreatureAttack { id: c, .. } if *c == id));
        let wall_hit = evs.iter().any(|e| matches!(e, GameEvent::ConstructHit { id: c, source: Some(s), .. } if *c == wall && *s == A));
        chaotic_hit |= attacked && wall_hit;
        if w.constructs.get(wall).is_none() {
            break;
        }
    }
    assert!(chaotic_hit, "chaotic creature never attacked the enemy wall");
    let _ = calm;
}

#[test]
fn aggressive_attacks_faster_than_calm() {
    let count = |t: Temperament| {
        let mut w = duel();
        place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
        let b = Vec3::new(0.0, GROUND, -2.2);
        place(&mut w, B, b);
        let id = spawn(&mut w, A, with_temper(creature_recipe(Form::Quadruped, &[], 1.2), t, 1.0), Vec3::new(0.0, GROUND, 0.0));
        let mut n = 0;
        for _ in 0..(6.0 / DT) as u32 {
            pin(&mut w, B, b);
            n += w.tick(&[]).iter().filter(|e| matches!(e, GameEvent::CreatureAttack { id: c, phase: 0, .. } if *c == id)).count();
        }
        n
    };
    let (calm, angry) = (count(Temperament::Calm), count(Temperament::Aggressive));
    assert!(angry > calm, "aggressive {angry} vs calm {calm} attacks in 6 s");
}

// ------------------------------------------------------------------------------------ swarm

#[test]
fn swarm_splits_into_fragile_independent_units() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    let b = Vec3::new(0.0, GROUND, -20.0);
    place(&mut w, B, b);
    let mut r = creature_recipe(Form::Insect, &[Mechanic::Swarm], 0.5);
    r.count = 6;
    let id = spawn(&mut w, A, r, Vec3::new(0.0, GROUND + 2.0, -3.0));
    for _ in 0..(1.0 / DT) as u32 {
        pin(&mut w, B, b);
        w.tick(&[]);
    }
    let units: Vec<ConstructId> = w.constructs.iter().filter(|c| c.parent == Some(id) && c.gone.is_none()).map(|c| c.id).collect();
    assert!(units.len() >= 5, "split into {} units", units.len());
    let mut stung = 0;
    for _ in 0..(6.0 / DT) as u32 {
        pin(&mut w, B, b);
        let evs = w.tick(&[]).to_vec();
        for e in &evs {
            if let GameEvent::Hit { target, construct: Some(c), damage, .. } = e
                && *target == B
                && units.contains(c)
            {
                stung += 1;
                assert!(*damage <= 3.5, "swarm nips are small ({damage})");
            }
        }
    }
    assert!(stung >= 3, "the swarm stung {stung} times");
    for u in &units {
        if let Some(c) = w.constructs.get(*u) {
            assert!(c.max_integrity <= bd_sim::constructs::swarm::UNIT_INTEGRITY + 1e-3, "units are fragile");
        }
    }
}

// ------------------------------------------------------------------------------------ kaiju

fn solid_count(w: &World, center: Vec3, r: f32) -> usize {
    let (lo, hi) = (world_to_voxel(center - Vec3::splat(r)), world_to_voxel(center + Vec3::splat(r)));
    let mut n = 0;
    for x in lo.x..=hi.x {
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                n += w.voxels.is_solid(IVec3::new(x, y, z)) as usize;
            }
        }
    }
    n
}

#[test]
fn kaiju_stomps_and_footsteps_crater_the_ground() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    let b = Vec3::new(0.0, GROUND, -4.0);
    place(&mut w, B, b);
    let id = spawn(&mut w, A, creature_recipe(Form::Kaiju, &[], 12.0), Vec3::new(0.0, GROUND, 0.0));
    let before = solid_count(&w, Vec3::new(0.0, GROUND, -1.0), 8.0);
    let mut stomped = false;
    let mut carves = 0;
    for _ in 0..(6.0 / DT) as u32 {
        pin(&mut w, B, b);
        let evs = w.tick(&[]).to_vec();
        stomped |= evs.iter().any(|e| matches!(e, GameEvent::CreatureAttack { id: c, kind, phase: 2, impact, .. } if *c == id && *kind == AttackKind::Stomp as u8 && *impact > 1.0));
        carves += evs.iter().filter(|e| matches!(e, GameEvent::Carve { .. })).count();
    }
    assert!(stomped, "kaiju never stomped");
    assert!(carves > 0, "no carve ops");
    let after = solid_count(&w, Vec3::new(0.0, GROUND, -1.0), 8.0);
    assert!(after < before, "stomp crater: {before} → {after} solid voxels");

    // Walking: every footstep cracks the ground.
    let mut w = duel();
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    let b = Vec3::new(0.0, GROUND, 40.0);
    place(&mut w, B, b);
    spawn(&mut w, A, creature_recipe(Form::Kaiju, &[], 12.0), Vec3::new(0.0, GROUND, -20.0));
    let mut steps = 0;
    for _ in 0..(5.0 / DT) as u32 {
        pin(&mut w, B, b);
        steps += w.tick(&[]).iter().filter(|e| matches!(e, GameEvent::Carve { .. })).count();
    }
    assert!(steps >= 3, "footstep carves: {steps}");
}

#[test]
fn cage_roots_a_kaiju() {
    let mut w = duel();
    place(&mut w, A, Vec3::new(-40.0, GROUND, -40.0));
    let b = Vec3::new(0.0, GROUND, 40.0);
    place(&mut w, B, b);
    let id = spawn(&mut w, A, creature_recipe(Form::Kaiju, &[], 12.0), Vec3::new(0.0, GROUND, -20.0));
    for _ in 0..(1.5 / DT) as u32 {
        pin(&mut w, B, b);
        w.tick(&[]);
    }
    let at = w.constructs.get(id).unwrap().pos;
    spawn(&mut w, B, recipe("cage", Form::Cage, &[], Element::Light, 4.0), Vec3::new(at.x, GROUND, at.z + 1.0));
    let mut rooted = false;
    for _ in 0..(0.3 / DT) as u32 {
        pin(&mut w, B, b);
        let evs = w.tick(&[]).to_vec();
        rooted |= evs.iter().any(|e| matches!(e, GameEvent::CreatureMode { id: c, mode } if *c == id && (*mode == ai::Mode::Rooted as u8 || *mode == ai::Mode::Pinned as u8)));
    }
    let p0 = w.constructs.get(id).unwrap().pos;
    for _ in 0..(1.0 / DT) as u32 {
        pin(&mut w, B, b);
        w.tick(&[]);
    }
    let p1 = w.constructs.get(id).unwrap().pos;
    assert!(rooted, "no rooted / toppled mode reported");
    assert!(hflat(p1 - p0) < 0.6, "caged kaiju kept walking: {p0:?} → {p1:?}");
}

#[test]
fn falling_anvil_slams_a_flier_out_of_the_sky() {
    let mut w = duel();
    // No valid target (B is spawn-shielded): the bird idles, circling slowly above its owner.
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    let b = Vec3::new(0.0, GROUND, 30.0);
    place(&mut w, B, b);
    let bird = spawn(&mut w, A, creature_recipe(Form::Bird, &[], 1.0), Vec3::new(3.0, GROUND + 5.0, 0.0));
    for _ in 0..30 {
        pin(&mut w, B, b);
        w.player_mut(B).unwrap().spawn_shield = 5.0;
        w.tick(&[]);
    }
    let at = w.constructs.get(bird).unwrap().center();
    let anvil = spawn(&mut w, B, recipe("anvil", Form::Anvil, &[], Element::Light, 1.0), at + Vec3::Y * 1.5);
    if let Some(c) = w.constructs.get_mut(anvil) {
        c.vel = Vec3::new(0.0, -20.0, 0.0);
    }
    let mut pinned = false;
    for _ in 0..(1.0 / DT) as u32 {
        pin(&mut w, B, b);
        w.player_mut(B).unwrap().spawn_shield = 5.0;
        // The shadow telegraph follows the bird (an aimed drop) until the anvil falls.
        let over = w.constructs.get(bird).map(|c| c.center() + Vec3::Y * 1.5);
        if let (Some(over), Some(a)) = (over, w.constructs.get_mut(anvil))
            && a.vel.y > -1.0
            && a.anim_state != bd_sim::constructs::AnimState::Move
        {
            a.pos = over - a.rot * a.local_center;
        }
        let evs = w.tick(&[]).to_vec();
        pinned |= evs.iter().any(|e| matches!(e, GameEvent::CreatureMode { id: c, mode } if *c == bird && *mode == ai::Mode::Pinned as u8));
    }
    let y = w.constructs.get(bird).map_or(GROUND, |c| c.pos.y);
    assert!(pinned, "flier was not slammed");
    assert!(y < GROUND + 2.0, "slammed flier should be on the ground (y {y})");
}

// ------------------------------------------------------------------------------------ islands

#[test]
fn walkers_never_fall_off_islands_in_a_minute() {
    let mut w = duel_on(ArenaSource::Archipelago { seed: 7 });
    w.cfg.check_invariants = false; // 6 creatures for one player (slot limits are the world's job)
    w.cfg.rules.willpower = WillpowerMode::Infinite; // no upkeep starvation
    let spawns = w.arena.spawns.clone();
    let void_y = w.arena.void_y;
    let a = spawns[0];
    // The target is the farthest spawn (likely on another island: the walkers must pace the edge).
    let b = *spawns.iter().max_by(|x, y| (**x - a).length().total_cmp(&(**y - a).length())).unwrap();
    place(&mut w, A, a);
    place(&mut w, B, b);
    let forms =
        [(Form::Biped, 1.8), (Form::Quadruped, 1.2), (Form::Arachnid, 1.0), (Form::Serpent, 1.5), (Form::Blob, 1.0), (Form::Kaiju, 10.0)];
    let ids: Vec<(Form, ConstructId)> = forms
        .iter()
        .enumerate()
        .map(|(i, &(f, s))| {
            let off = Vec3::new((i as f32 - 2.5) * 1.5, 0.5, 2.0);
            (f, spawn(&mut w, A, creature_recipe(f, &[], s), a + off))
        })
        .collect();
    for t in 0..(60.0 / DT) as u32 {
        pin(&mut w, A, a);
        pin(&mut w, B, b);
        w.tick(&[]);
        for (f, id) in &ids {
            let c = w.constructs.get(*id).unwrap_or_else(|| panic!("{f:?} vanished at t={:.1}s", t as f32 * DT));
            assert!(c.pos.y > void_y + 5.0, "{f:?} fell off the island at t={:.1}s: {:?}", t as f32 * DT, c.pos);
        }
    }
}

// ------------------------------------------------------------------------------------ determinism

#[test]
fn creature_brawl_is_deterministic() {
    let run = || {
        let mut w = duel();
        place(&mut w, A, Vec3::new(-6.0, GROUND, 0.0));
        place(&mut w, B, Vec3::new(6.0, GROUND, 0.0));
        let mut r = creature_recipe(Form::Quadruped, &[Mechanic::Breath, Mechanic::Charge], 1.5);
        r.element = Element::Fire;
        spawn(&mut w, A, with_temper(r, Temperament::Playful, 0.7), Vec3::new(-4.0, GROUND, 3.0));
        spawn(
            &mut w,
            B,
            with_temper(creature_recipe(Form::Biped, &[Mechanic::Stomp], 2.0), Temperament::Chaotic, 0.9),
            Vec3::new(4.0, GROUND, -3.0),
        );
        spawn(&mut w, B, with_temper(creature_recipe(Form::Bird, &[], 1.0), Temperament::Sneaky, 0.9), Vec3::new(4.0, GROUND + 4.0, 3.0));
        let mut h = Fnv64::default();
        for _ in 0..(10.0 / DT) as u32 {
            for e in w.tick(&[]) {
                h.write(format!("{e:?}").as_bytes());
            }
            for c in w.constructs.iter() {
                h.write(&c.pos.x.to_bits().to_le_bytes());
                h.write(&c.pos.z.to_bits().to_le_bytes());
            }
        }
        h.0
    };
    assert_eq!(run(), run());
}

#[test]
fn chunk_voxel_query_matches_world() {
    // Sanity for the probe helpers against the real world type.
    let w = duel();
    assert_eq!(ai::ground_at(&w.voxels, Vec3::new(1.0, GROUND + 0.5, 1.0), 1.0, 2.0), Some(GROUND));
    assert!(w.voxels.is_solid(world_to_voxel(Vec3::new(1.0, GROUND - 0.2, 1.0))));
}
