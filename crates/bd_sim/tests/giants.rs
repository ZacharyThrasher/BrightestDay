//! GIANTS (`constructs/giant.rs`): kaiju you fight, not death balls — weak points (crit, stagger,
//! felled), tethers on a moving giant, riding it (host = prediction), limbs cut off (their attack is
//! gone), topples that expose the nape, stomp shockwaves a jump clears, swipes a dodge goes through,
//! command focus, hijack, determinism. Flat lab arena: ground top at y = 8.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{ConstructId, DT, PlayerId, Quat, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::ConstructHit;
use bd_sim::constructs::ai::{AttackKind, Mode, Target, phase};
use bd_sim::constructs::giant::{self, GiantEvent, GiantTuning, Limb, Spot};
use bd_sim::constructs::{Construct, SpawnParams};
use bd_sim::events::GameEvent;
use bd_sim::movement::{AnchorTarget, ColliderId};
use bd_sim::rng::Fnv64;
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);
const GROUND: f32 = 8.0;

fn world(players: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=players {
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

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
}

fn pin(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.damage_pct = 0.0;
    p.alive = true;
}

fn kaiju_recipe(mechs: &[Mechanic]) -> Recipe {
    recipe("godzilla", Form::Kaiju, mechs, Element::Atomic, 12.0)
}

fn spawn(w: &mut World, owner: PlayerId, r: Recipe, pos: Vec3, yaw: f32) -> ConstructId {
    let tick = w.tick;
    w.constructs.spawn(SpawnParams {
        owner,
        recipe: r,
        shape: None,
        pos,
        rot: Quat::from_rotation_y(yaw),
        vel: Vec3::ZERO,
        target: None,
        cost: 60.0,
        integrity: 400.0,
        forming: 0.05,
        life: 1000.0,
        tick,
        colossal: false,
        model: None,
    })
}

/// A kaiju that has become a giant (a few ticks in).
fn kaiju(w: &mut World, owner: PlayerId, mechs: &[Mechanic], pos: Vec3) -> ConstructId {
    let id = spawn(w, owner, kaiju_recipe(mechs), pos, 0.0);
    for _ in 0..12 {
        w.tick(&[]);
    }
    assert!(w.constructs.get(id).unwrap().ai.giant.on, "a kaiju is a giant");
    id
}

fn con(w: &World, id: ConstructId) -> &Construct {
    w.constructs.get(id).unwrap()
}

fn weak_pos(w: &World, id: ConstructId, spot: Spot) -> Vec3 {
    let c = con(w, id);
    let wp = c.ai.giant.body.weak_points().iter().find(|p| p.spot == spot).copied().unwrap();
    giant::weak_world(c, &wp)
}

fn limb_pos(w: &World, id: ConstructId, l: Limb) -> Vec3 {
    let c = con(w, id);
    let (lc, _) = c.ai.giant.body.limb_box(l).unwrap();
    giant::to_world(c, lc)
}

fn hit(id: ConstructId, point: Vec3, damage: f32, blade: bool, by: PlayerId) -> ConstructHit {
    ConstructHit { pierce: false, blunt: false, target: id, source: Some(by), damage, impulse: Vec3::ZERO, blade, point }
}

fn giant_events(evs: &[GameEvent]) -> Vec<GiantEvent> {
    evs.iter().filter_map(|e| if let GameEvent::Giant(g) = e { Some(g.clone()) } else { None }).collect()
}

fn hits_on(evs: &[GameEvent], id: ConstructId, target: PlayerId) -> usize {
    evs.iter().filter(|e| matches!(e, GameEvent::Hit { target: t, construct: Some(c), .. } if *c == id && *t == target)).count()
}

fn yaw_pitch(d: Vec3) -> (f32, f32) {
    let d = d.normalize();
    ((-d.x).atan2(-d.z), d.y.clamp(-1.0, 1.0).asin())
}

// ------------------------------------------------------------------------------------ anatomy

#[test]
fn a_kaiju_is_a_giant_with_parts_weak_points_and_solid_boxes() {
    let mut w = world(2);
    place(&mut w, B, Vec3::new(0.0, GROUND, -60.0));
    let id = spawn(&mut w, A, kaiju_recipe(&[Mechanic::ArticulatedTail]), Vec3::new(0.0, GROUND, 0.0), 0.0);
    let mut born = false;
    for _ in 0..12 {
        let evs = w.tick(&[]).to_vec();
        born |= giant_events(&evs).iter().any(|g| matches!(g, GiantEvent::Born { id: c, .. } if *c == id));
    }
    assert!(born, "Born (anatomy) is announced");
    let c = con(&w, id);
    let b = &c.ai.giant.body;
    for l in [Limb::Torso, Limb::Head, Limb::ArmL, Limb::ArmR, Limb::LegBL, Limb::LegBR, Limb::Tail] {
        assert!(b.has(l), "kaiju has {l:?}");
    }
    let tail_segments = b.parts().iter().filter(|p| p.limb == Limb::Tail).count();
    assert!(tail_segments >= 3, "the tail is a staircase of segments ({tail_segments})");
    let spots: Vec<Spot> = b.weak_points().iter().map(|w| w.spot).collect();
    assert_eq!(spots, vec![Spot::Nape, Spot::Core, Spot::Eye]);
    // The nape is behind the head's front, high up; the core is on the chest front.
    let nape = b.weak_points()[0];
    assert!(nape.pos.y > b.height * 0.6, "nape high: {:?} / h {}", nape.pos, b.height);
    // One solid, anchorable collider per part.
    let cols: Vec<_> = w.colliders().iter().filter(|k| matches!(k.id, ColliderId::Construct { id: cid, .. } if cid == id)).collect();
    assert_eq!(cols.len(), b.parts().len());
    assert!(cols.iter().all(|k| k.solid && k.anchorable));
    // Small creatures are not giants.
    let dog = spawn(&mut w, A, recipe("dog", Form::Quadruped, &[], Element::Light, 1.2), Vec3::new(10.0, GROUND, 10.0), 0.0);
    for _ in 0..12 {
        w.tick(&[]);
    }
    assert!(!con(&w, dog).ai.giant.on);
}

// ------------------------------------------------------------------------------------ weak points

#[test]
fn weak_points_crit_stagger_and_fell_the_giant() {
    let t = GiantTuning::default();
    let mut w = world(2);
    // B in reach so the kaiju is mid-attack when the nape is hit.
    let b = Vec3::new(0.0, GROUND, -6.0);
    place(&mut w, B, b);
    let id = kaiju(&mut w, B, &[], Vec3::new(0.0, GROUND, 0.0));
    // Owner B, target A (in front, close).
    place(&mut w, A, Vec3::new(0.0, GROUND, -7.0));
    let i0 = con(&w, id).integrity;
    // A body hit on the torso's back (not a weak point): plain damage (Blade ×2 vs constructs).
    let back = {
        let c = con(&w, id);
        let (tc, th) = c.ai.giant.body.limb_box(Limb::Torso).unwrap();
        giant::to_world(c, tc + Vec3::new(th.x * 0.9, 0.0, th.z * 0.3))
    };
    w.apply_construct_hit(hit(id, back, 5.0, true, A));
    let i1 = con(&w, id).integrity;
    assert!((i0 - i1 - 10.0).abs() < 1e-3, "body: 5 × blade 2 = 10, got {}", i0 - i1);
    // Nape, blade: × weak_nape, and it staggers.
    let nape = weak_pos(&w, id, Spot::Nape);
    let n0 = w.events().len();
    w.apply_construct_hit(hit(id, nape, 5.0, true, A));
    let i2 = con(&w, id).integrity;
    assert!((i1 - i2 - 10.0 * t.weak_nape).abs() < 1e-3, "nape ×{}: {}", t.weak_nape, i1 - i2);
    assert!(giant_events(&w.events()[n0..]).iter().any(|g| matches!(g, GiantEvent::WeakHit { stagger: true, .. })));
    assert!(con(&w, id).ai.giant.stagger > 0.0);
    let evs = w.tick(&[]).to_vec();
    assert!(evs.iter().any(|e| matches!(e, GameEvent::CreatureMode { id: c, mode } if *c == id && *mode == Mode::Staggered as u8)));
    assert!(!con(&w, id).ai.attack.running, "the stagger cancels its attack");
    // Same attacker again at once: the weak-point bonus is on cooldown (a body hit).
    w.apply_construct_hit(hit(id, nape, 5.0, true, A));
    let i3 = con(&w, id).integrity;
    assert!((i2 - i3 - 10.0).abs() < 1e-3, "cooldown: plain {}", i2 - i3);
    // A blunt hit at the nape gets part of the bonus.
    for _ in 0..((t.weak_cd / DT) as u32 + 2) {
        w.tick(&[]);
    }
    let i4 = con(&w, id).integrity;
    w.apply_construct_hit(hit(id, nape, 5.0, false, A));
    let i5 = con(&w, id).integrity;
    let blunt = 1.0 + (t.weak_nape - 1.0) * t.blunt_frac;
    assert!((i4 - i5 - 5.0 * blunt).abs() < 1e-3, "blunt nape ×{blunt}: {}", i4 - i5);
    // Keep slashing the nape (respecting the cooldown): felled through the weak pool long before
    // its integrity would run out from body hits.
    let mut felled = false;
    let mut slashes = 0;
    for _ in 0..40 {
        if w.constructs.get(id).is_none() {
            break;
        }
        let nape = weak_pos(&w, id, Spot::Nape);
        let n0 = w.events().len();
        w.apply_construct_hit(hit(id, nape, 5.0, true, A));
        slashes += 1;
        felled |= giant_events(&w.events()[n0..]).iter().any(|g| matches!(g, GiantEvent::Felled { .. }));
        for _ in 0..((t.weak_cd / DT) as u32 + 2) {
            w.tick(&[]);
        }
    }
    assert!(felled, "felled through the nape");
    assert!(w.constructs.get(id).is_none(), "gone");
    // 400 integrity, weak pool 200 at 30 a slash: ~7 slashes (not 40 body hits).
    assert!(slashes <= 9, "slashes to fell: {slashes}");
}

// ------------------------------------------------------------------------------------ tethers

#[test]
fn a_tether_on_a_walking_giant_follows_the_part_it_hit() {
    let mut w = world(2);
    place(&mut w, B, Vec3::new(0.0, GROUND, -44.0));
    // A's kaiju walks toward B; A stands beside it and hooks its leg.
    let id = kaiju(&mut w, A, &[], Vec3::new(0.0, GROUND, 0.0));
    place(&mut w, A, Vec3::new(14.0, GROUND, 0.0));
    w.tick(&[]);
    let target = limb_pos(&w, id, Limb::LegBR);
    let eye = w.player(A).unwrap().eye(w.params());
    let (yaw, pitch) = yaw_pitch(target - eye);
    let inp = |b: u16| PlayerInput { yaw, pitch, buttons: b, ..Default::default() };
    let mut attached = None;
    for _ in 0..20 {
        w.tick(&[(A, inp(buttons::TETHER_L))]);
        let h = w.player(A).unwrap().mv.hooks[0];
        if h.attached() {
            attached = h.target;
            break;
        }
    }
    let Some(AnchorTarget::Collider(ColliderId::Construct { id: cid, part })) = attached else { panic!("hooked the giant: {attached:?}") };
    assert_eq!(cid, id);
    assert_eq!(giant::limb_of_collider(con(&w, id), part), Some(Limb::LegBR), "anchored on the leg it hit");
    let p0 = con(&w, id).pos;
    let h0 = w.player(A).unwrap().mv.hooks[0].pos;
    let l0 = w.colliders().iter().find(|k| k.id == ColliderId::Construct { id, part }).unwrap().to_local(h0);
    for _ in 0..(3.0 / DT) as u32 {
        w.tick(&[(A, PlayerInput { yaw, pitch, buttons: buttons::TETHER_L, ..Default::default() })]);
        let h = w.player(A).unwrap().mv.hooks[0];
        assert!(h.attached(), "the rope holds");
        // The anchor sits exactly on the part's box as the giant moves.
        let col = w.colliders().iter().find(|k| k.id == ColliderId::Construct { id, part }).copied().unwrap();
        let l = col.to_local(h.pos);
        assert!((l.abs() - col.half).max_element() < 0.05, "anchor on the part box: local {l:?} half {:?}", col.half);
        assert!((l - l0).length() < 1e-3, "the same spot on the part: {l:?} vs {l0:?}");
    }
    let moved = con(&w, id).pos - p0;
    let hmoved = w.player(A).unwrap().mv.hooks[0].pos - h0;
    assert!(moved.length() > 4.0, "the giant walked ({moved:?})");
    assert!((hmoved - moved).length() < 1.0, "the anchor moved with it: {hmoved:?} vs {moved:?}");
}

// ------------------------------------------------------------------------------------ riding

struct Pair {
    host: World,
    client: World,
}

impl Pair {
    fn step(&mut self, inp: PlayerInput) -> (f32, f32) {
        let mut s = self.client.player(A).unwrap().mv;
        self.client.predict_player(A, &mut s, &inp);
        let evs = self.host.tick(&[(A, inp)]).to_vec();
        for e in &evs {
            self.client.apply_event(e);
        }
        self.client.apply_snapshot(&self.host.snapshot_for(Some(A)), Some(A));
        let h = self.host.player(A).unwrap().mv;
        ((h.pos - s.pos).length(), (h.vel - s.vel).length())
    }
}

#[test]
fn a_rider_is_carried_by_a_walking_giant_and_prediction_agrees() {
    let mut host = world(2);
    place(&mut host, B, Vec3::new(0.0, GROUND, -44.0));
    let id = kaiju(&mut host, A, &[], Vec3::new(0.0, GROUND, 0.0));
    // A stands on the kaiju's back (top of the torso box).
    let (top, tc) = {
        let c = con(&host, id);
        let (tc, th) = c.ai.giant.body.limb_box(Limb::Torso).unwrap();
        (giant::to_world(c, tc + Vec3::Y * th.y), tc)
    };
    let _ = tc;
    place(&mut host, A, top + Vec3::new(0.0, 0.3, 0.5));
    for _ in 0..30 {
        host.tick(&[]);
    }
    assert!(host.player(A).unwrap().mv.grounded, "standing on the giant");
    let mut client = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() });
    for e in host.construct_manifest() {
        client.apply_event(&e);
    }
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    assert!(con(&client, id).ai.giant.on, "the replica knows the anatomy");
    let mut p = Pair { host, client };
    let rel = |w: &World| {
        let c = con(w, id);
        c.rot.inverse() * (w.player(A).unwrap().mv.pos - c.pos)
    };
    let r0 = rel(&p.host);
    let k0 = con(&p.host, id).pos;
    let (mut worst_dp, mut worst_dv) = (0.0f32, 0.0f32);
    let mut grounded = 0;
    let n = (5.0 / DT) as u32;
    for _ in 0..n {
        let (dp, dv) = p.step(PlayerInput::default());
        worst_dp = worst_dp.max(dp);
        worst_dv = worst_dv.max(dv);
        grounded += p.host.player(A).unwrap().mv.grounded as u32;
        if std::env::var("GIANT_DEBUG").is_ok() && grounded % 30 == 0 {
            let c = con(&p.host, id);
            let pl = p.host.player(A).unwrap().mv;
            eprintln!("k pos {:?} vel {:?} fwd {:?} | rel {:?} pvel {:?} gv {:?}", c.pos, c.vel, c.forward(), rel(&p.host), pl.vel, pl.ground_vel);
        }
    }
    let travelled = (con(&p.host, id).pos - k0).length();
    let drift = (rel(&p.host) - r0).length();
    eprintln!("ride: giant travelled {travelled:.2} m, rider drift {drift:.3} m, grounded {grounded}/{n}, prediction worst {worst_dp:.4} m {worst_dv:.4} m/s");
    assert!(travelled > 10.0, "the giant walked ({travelled} m)");
    assert!(drift < 0.6, "carried: drift on its back {drift} m");
    assert!(grounded as f32 > n as f32 * 0.97, "stays standing on it ({grounded}/{n})");
    assert!(worst_dp < 0.02 && worst_dv < 0.05, "host = prediction while riding: {worst_dp} m, {worst_dv} m/s");
}

// ------------------------------------------------------------------------------------ limbs

#[test]
fn a_severed_arm_takes_its_swipe_and_a_severed_tail_its_sweep() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(40.0, GROUND, 40.0));
    let id = kaiju(&mut w, B, &[Mechanic::ArticulatedTail], Vec3::new(0.0, GROUND, 0.0));
    let arm_parts = |w: &World| {
        let c = con(w, id);
        w.colliders()
            .iter()
            .filter(|k| matches!(k.id, ColliderId::Construct { id: cid, part } if cid == id && giant::limb_of_collider(c, part).is_some_and(|l| l.is_arm())))
            .count()
    };
    assert_eq!(arm_parts(&w), 2);
    let mut severed = Vec::new();
    for l in [Limb::ArmL, Limb::ArmR, Limb::Tail] {
        for _ in 0..30 {
            if con(&w, id).ai.giant.is_severed(l) {
                break;
            }
            let at = limb_pos(&w, id, l);
            let n0 = w.events().len();
            w.apply_construct_hit(hit(id, at, 6.0, true, A));
            for g in giant_events(&w.events()[n0..]) {
                if let GiantEvent::Limb { limb, severed: true, .. } = g {
                    severed.push(Limb::from_u8(limb));
                }
            }
        }
    }
    assert_eq!(severed, vec![Limb::ArmL, Limb::ArmR, Limb::Tail]);
    w.tick(&[]);
    assert_eq!(arm_parts(&w), 0, "a severed arm's collider is gone");
    let c = con(&w, id);
    assert!(!giant::can(c, AttackKind::Swipe) && !giant::can(c, AttackKind::TailSweep));
    // Now fight A up close: it bites / stomps, never swipes or tail-sweeps.
    let a = Vec3::new(0.0, GROUND, -7.0);
    let mut kinds = Vec::new();
    for _ in 0..(8.0 / DT) as u32 {
        pin(&mut w, A, a);
        for e in w.tick(&[]).to_vec() {
            if let GameEvent::CreatureAttack { id: cid, kind, phase: phase::TELEGRAPH, .. } = e {
                if cid == id {
                    kinds.push(AttackKind::from_u8(kind));
                }
            }
        }
    }
    assert!(!kinds.is_empty(), "it still fights");
    assert!(!kinds.contains(&AttackKind::Swipe) && !kinds.contains(&AttackKind::TailSweep), "attacks after the cuts: {kinds:?}");
}

#[test]
fn a_broken_leg_limps_then_topples_it_and_exposes_the_nape() {
    let t = GiantTuning::default();
    let mut w = world(2);
    place(&mut w, A, Vec3::new(40.0, GROUND, 40.0));
    let id = kaiju(&mut w, B, &[], Vec3::new(0.0, GROUND, 0.0));
    let tn = weak_pos(&w, id, Spot::Nape).y;
    // Half a leg: it limps.
    let leg = limb_pos(&w, id, Limb::LegBL);
    let max = con(&w, id).ai.giant.limb_max[Limb::LegBL as usize];
    w.apply_construct_hit(hit(id, leg, max * 0.25, false, A));
    let slow = giant::speed_mult(con(&w, id), &t);
    assert!((slow - (1.0 - t.limp_slow * 0.25)).abs() < 1e-3, "limps: ×{slow}");
    // Break it: it topples onto that (left) side.
    let mut toppled = false;
    w.apply_construct_hit(hit(id, leg, max, false, A));
    for _ in 0..3 {
        for e in w.tick(&[]).to_vec() {
            toppled |= matches!(e, GameEvent::Giant(GiantEvent::Topple { id: c, phase: 0, side, .. }) if c == id && side < 0.0);
        }
    }
    assert!(toppled, "topple announced (left side)");
    for _ in 0..((t.topple_fall + 0.3) / DT) as u32 {
        w.tick(&[]);
    }
    let c = con(&w, id);
    assert!(c.ai.giant.toppled());
    let nape = weak_pos(&w, id, Spot::Nape);
    assert!(nape.y < GROUND + c.ai.giant.body.height * 0.45 && nape.y < tn - 3.0, "the nape comes down within reach: {} (was {tn})", nape.y);
    assert!((c.rot * Vec3::Y).y < 0.5, "lying on its side");
    // Exposed: the nape takes the topple bonus too.
    let i0 = c.integrity;
    w.apply_construct_hit(hit(id, nape, 5.0, true, A));
    let d = i0 - con(&w, id).integrity;
    assert!((d - 10.0 * t.weak_nape * t.topple_weak).abs() < 1e-3, "exposed nape: {d}");
    // Toppled: no attacks, no walking.
    let p0 = con(&w, id).pos;
    let mut attacked = false;
    pin(&mut w, A, Vec3::new(0.0, GROUND, -6.0));
    for _ in 0..((t.topple_down - 0.5) / DT) as u32 {
        pin(&mut w, A, Vec3::new(0.0, GROUND, -6.0));
        attacked |= w.tick(&[]).iter().any(|e| matches!(e, GameEvent::CreatureAttack { id: c, .. } if *c == id));
    }
    assert!(!attacked, "helpless while down");
    assert!((con(&w, id).pos - p0).length() < 0.5);
    // It gets up (the leg partly recovered) and stands upright.
    for _ in 0..((t.topple_rise + 0.6) / DT) as u32 {
        pin(&mut w, A, Vec3::new(40.0, GROUND, 40.0));
        w.tick(&[]);
    }
    let c = con(&w, id);
    assert!(!c.ai.giant.toppled());
    assert!((c.rot * Vec3::Y).y > 0.99, "upright again");
    assert!((c.ai.giant.limb_hp[Limb::LegBL as usize] - max * t.leg_recover).abs() < 1e-3);
}

// ------------------------------------------------------------------------------------ attacks

/// Hits from `id` on B over `secs` while B is held at `b` (airborne or on the ground), the kaiju held
/// in place; whether a stomp shockwave went out.
fn stomp_run(airborne: bool) -> (usize, bool) {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(40.0, GROUND, 40.0));
    let id = kaiju(&mut w, A, &[], Vec3::new(0.0, GROUND, 0.0));
    let at = con(&w, id).pos;
    let b = Vec3::new(0.0, GROUND + if airborne { 2.2 } else { 0.0 }, -8.3);
    let (mut hits, mut shock) = (0, false);
    for _ in 0..(6.0 / DT) as u32 {
        pin(&mut w, B, b);
        if let Some(c) = w.constructs.get_mut(id) {
            c.pos = at;
            c.vel = Vec3::ZERO;
        }
        let evs = w.tick(&[]).to_vec();
        hits += hits_on(&evs, id, B);
        shock |= giant_events(&evs).iter().any(|g| matches!(g, GiantEvent::Shock { .. }));
    }
    (hits, shock)
}

#[test]
fn a_stomp_shockwave_hits_the_ground_and_misses_the_airborne() {
    let (grounded_hits, s1) = stomp_run(false);
    let (air_hits, s2) = stomp_run(true);
    assert!(s1 && s2, "it stomped both times");
    assert!(grounded_hits > 0, "the shockwave hits a player on the ground");
    assert_eq!(air_hits, 0, "a jump clears it");
}

/// Kaiju swiping at B (stomps disabled); B presses Dodge at `dodge_at` (tick index). Returns the
/// tick of the first hit on B and whether a swipe was active then.
fn swipe_run(dodge_at: Option<u32>) -> (Option<u32>, Vec<u32>) {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(40.0, GROUND, 40.0));
    let id = kaiju(&mut w, A, &[], Vec3::new(0.0, GROUND, 0.0));
    let at = con(&w, id).pos;
    let b = Vec3::new(1.0, GROUND, -5.5);
    let mut first = None;
    let mut actives = Vec::new();
    for k in 0..(5.0 / DT) as u32 {
        {
            let p = w.player_mut(B).unwrap();
            p.mv.pos = b;
            p.mv.vel = Vec3::ZERO;
            p.damage_pct = 0.0;
        }
        if let Some(c) = w.constructs.get_mut(id) {
            c.pos = at;
            c.vel = Vec3::ZERO;
            c.ai.cd[AttackKind::Stomp as usize] = 99.0;
        }
        let inp = PlayerInput { buttons: if dodge_at == Some(k) { buttons::DODGE } else { 0 }, move_x: 1, ..Default::default() };
        let evs = w.tick(&[(B, inp)]).to_vec();
        for e in &evs {
            if let GameEvent::CreatureAttack { id: c, kind, phase: phase::ACTIVE, .. } = e {
                if *c == id && *kind == AttackKind::Swipe as u8 {
                    actives.push(k);
                }
            }
        }
        if first.is_none() && hits_on(&evs, id, B) > 0 {
            first = Some(k);
        }
    }
    (first, actives)
}

#[test]
fn a_swipe_is_an_arc_that_dodge_iframes_go_through() {
    let (hit_at, actives) = swipe_run(None);
    let t = hit_at.expect("the swipe connects when you stand there");
    let active = *actives.iter().filter(|a| **a <= t).last().expect("a swipe was active");
    assert!(t >= active, "the blow lands inside the swipe's window");
    // Same fight, B dodges just before the arm passes (i-frames 0.13 s): no hit from that swipe.
    let (dodged, _) = swipe_run(Some(t.saturating_sub(3)));
    assert!(dodged.is_none_or(|d| d > t + 20), "dodged through the arc (first hit {dodged:?}, undodged {t})");
}

#[test]
fn walking_into_a_giant_never_launches_you() {
    let mut w = world(2);
    // The kaiju walks toward B far away; A stands in its path, then walks into its legs.
    place(&mut w, B, Vec3::new(0.0, GROUND, -44.0));
    let id = kaiju(&mut w, B, &[], Vec3::new(0.0, GROUND, 0.0));
    // Owner B's kaiju targets A: hold its attacks off (cooldowns) and walk A into it.
    let mut hits = 0;
    place(&mut w, A, Vec3::new(0.0, GROUND, -14.0));
    for _ in 0..(4.0 / DT) as u32 {
        if let Some(c) = w.constructs.get_mut(id) {
            for cd in c.ai.cd.iter_mut() {
                *cd = 99.0;
            }
        }
        let evs = w.tick(&[(A, PlayerInput { move_z: 1, yaw: std::f32::consts::PI, ..Default::default() })]).to_vec();
        hits += hits_on(&evs, id, A);
        assert!(w.player(A).unwrap().mv.tumble <= 0.0, "never tumbling");
    }
    assert_eq!(hits, 0, "no ambient hits (footsteps, contact)");
}

// ------------------------------------------------------------------------------------ command

#[test]
fn the_summoner_points_and_the_giant_goes() {
    let mut w = world(3);
    let a = Vec3::new(0.0, GROUND, 20.0);
    place(&mut w, A, a);
    // B is near the kaiju, C far: without a command it goes for B.
    place(&mut w, B, Vec3::new(10.0, GROUND, -4.0));
    place(&mut w, C, Vec3::new(-30.0, GROUND, -30.0));
    let id = kaiju(&mut w, A, &[], Vec3::new(0.0, GROUND, 0.0));
    for _ in 0..30 {
        pin(&mut w, B, Vec3::new(10.0, GROUND, -4.0));
        pin(&mut w, C, Vec3::new(-30.0, GROUND, -30.0));
        w.tick(&[]);
    }
    assert_eq!(con(&w, id).ai.target, Target::Player(B), "nearest by default");
    // A holds the crosshair on C.
    let mut focus = false;
    for _ in 0..(0.6 / DT) as u32 {
        pin(&mut w, B, Vec3::new(10.0, GROUND, -4.0));
        pin(&mut w, C, Vec3::new(-30.0, GROUND, -30.0));
        let eye = w.player(A).unwrap().eye(w.params());
        let cc = w.player(C).unwrap().mv.pos + Vec3::Y * 1.0;
        let (yaw, pitch) = yaw_pitch(cc - eye);
        pin(&mut w, A, a);
        let evs = w.tick(&[(A, PlayerInput { yaw, pitch, ..Default::default() })]).to_vec();
        focus |= giant_events(&evs).iter().any(|g| matches!(g, GiantEvent::Focus { target, .. } if *target == C));
    }
    assert!(focus, "focus ping");
    assert_eq!(con(&w, id).ai.target, Target::Player(C), "the commanded target");
    // Looking away keeps the focus (for `focus_secs`).
    for _ in 0..60 {
        pin(&mut w, B, Vec3::new(10.0, GROUND, -4.0));
        w.tick(&[(A, PlayerInput::default())]);
    }
    assert_eq!(con(&w, id).ai.target, Target::Player(C));
}

// ------------------------------------------------------------------------------------ hijack

#[test]
fn charging_at_the_nape_hijacks_the_giant() {
    let t = GiantTuning::default();
    let mut w = world(2);
    place(&mut w, B, Vec3::new(40.0, GROUND, 40.0));
    let id = kaiju(&mut w, A, &[], Vec3::new(0.0, GROUND, 0.0));
    // Freeze the kaiju (a rider test, not a chase).
    let hold = |w: &mut World| {
        if let Some(c) = w.constructs.get_mut(id) {
            c.vel = Vec3::ZERO;
            for cd in c.ai.cd.iter_mut() {
                *cd = 99.0;
            }
        }
    };
    let charge = PlayerInput { buttons: buttons::CHARGE, ..Default::default() };
    // Let go half-way: progress resets.
    for k in 0..((t.hijack_secs * 0.6) / DT) as u32 {
        hold(&mut w);
        let nape = weak_pos(&w, id, Spot::Nape);
        pin(&mut w, B, nape + Vec3::Y * 0.2);
        w.tick(&[(B, if k < 5 { PlayerInput::default() } else { charge })]);
    }
    assert_eq!(con(&w, id).owner, A);
    hold(&mut w);
    pin(&mut w, B, Vec3::new(40.0, GROUND, 40.0));
    w.tick(&[(B, PlayerInput::default())]);
    assert!(con(&w, id).ai.giant.hijack_t < 0.1, "reset when knocked off");
    // The full hold steals it.
    let mut done = false;
    for _ in 0..((t.hijack_secs + 0.3) / DT) as u32 {
        hold(&mut w);
        let nape = weak_pos(&w, id, Spot::Nape);
        pin(&mut w, B, nape + Vec3::Y * 0.2);
        let evs = w.tick(&[(B, charge)]).to_vec();
        done |= giant_events(&evs).iter().any(|g| matches!(g, GiantEvent::Hijack { by, done: true, .. } if *by == B));
    }
    assert!(done, "hijacked");
    assert_eq!(con(&w, id).owner, B, "now B's summon");
    // Off in the tuning: nothing happens.
    let mut w = world(2);
    let mut tt = w.cfg.tuning.clone();
    tt.giants.hijack = false;
    w.set_tuning(tt);
    let id = kaiju(&mut w, A, &[], Vec3::new(0.0, GROUND, 0.0));
    for _ in 0..((t.hijack_secs + 0.5) / DT) as u32 {
        if let Some(c) = w.constructs.get_mut(id) {
            c.vel = Vec3::ZERO;
        }
        let nape = weak_pos(&w, id, Spot::Nape);
        pin(&mut w, B, nape + Vec3::Y * 0.2);
        w.tick(&[(B, charge)]);
    }
    assert_eq!(con(&w, id).owner, A);
}

// ------------------------------------------------------------------------------------ determinism + tuning

#[test]
fn a_giant_fight_is_deterministic() {
    let run = || {
        let mut w = world(3);
        place(&mut w, A, Vec3::new(-10.0, GROUND, 10.0));
        place(&mut w, B, Vec3::new(10.0, GROUND, -10.0));
        place(&mut w, C, Vec3::new(0.0, GROUND, -20.0));
        let k1 = kaiju(&mut w, A, &[Mechanic::ArticulatedTail], Vec3::new(0.0, GROUND, 0.0));
        let _k2 = spawn(&mut w, B, kaiju_recipe(&[Mechanic::Breath]), Vec3::new(-20.0, GROUND, -20.0), 1.0);
        let mut h = Fnv64::default();
        for k in 0..(8.0 / DT) as u32 {
            if k % 30 == 0 && w.constructs.get(k1).is_some() {
                let at = if k % 60 == 0 { weak_pos(&w, k1, Spot::Nape) } else { limb_pos(&w, k1, Limb::LegBR) };
                w.apply_construct_hit(hit(k1, at, 7.0, true, B));
            }
            let inp = PlayerInput { move_z: 1, yaw: k as f32 * 0.01, buttons: if k % 90 < 30 { buttons::JUMP } else { 0 }, ..Default::default() };
            for e in w.tick(&[(B, inp), (C, inp)]) {
                h.write(format!("{e:?}").as_bytes());
            }
            for c in w.constructs.iter() {
                h.write(format!("{:?}{:?}", c.pos, c.rot).as_bytes());
            }
        }
        h.0
    };
    assert_eq!(run(), run());
}

#[test]
fn shipped_tuning_has_the_giant_defaults() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/combat.ron")).unwrap();
    let t = bd_sim::tuning::CombatTuning::from_ron(&src).unwrap();
    assert_eq!(t.giants, GiantTuning::default());
    assert_eq!(GiantTuning::default().validated(), GiantTuning::default());
}

/// Perf probe (release): tick cost of a brawl with two kaiju vs the same brawl without giants.
/// `cargo test --release -p bd_sim --test giants giant_brawl_tick_cost -- --ignored --nocapture`
#[test]
#[ignore = "perf probe (release build)"]
fn giant_brawl_tick_cost() {
    let run = |giants: bool| {
        let mut w = world(8);
        let mut t = w.cfg.tuning.clone();
        t.giants.enabled = giants;
        w.set_tuning(t);
        w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
        for k in 1..=8u8 {
            let a = k as f32 * 0.785;
            place(&mut w, PlayerId(k), Vec3::new(a.cos() * 16.0, GROUND, a.sin() * 16.0));
        }
        spawn(&mut w, A, kaiju_recipe(&[Mechanic::ArticulatedTail, Mechanic::Stomp]), Vec3::new(-8.0, GROUND, 0.0), 0.0);
        spawn(&mut w, B, kaiju_recipe(&[Mechanic::Breath]), Vec3::new(8.0, GROUND, 0.0), 3.0);
        let mut times = Vec::new();
        for k in 0..2400u32 {
            let inps: Vec<(PlayerId, PlayerInput)> = (1..=8u8)
                .map(|p| (PlayerId(p), PlayerInput { move_z: 1, yaw: (k as f32 * 0.013 + p as f32).sin() * 3.0, buttons: if (k + p as u32 * 7) % 70 < 20 { buttons::JUMP } else { 0 }, ..Default::default() }))
                .collect();
            let t0 = std::time::Instant::now();
            w.tick(&inps);
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(|a, b| a.total_cmp(b));
        (times[times.len() / 2], times[times.len() * 99 / 100], times[times.len() - 1])
    };
    let off = run(false);
    let on = run(true);
    eprintln!("tick ms (p50, p99, max): giants off {off:?}, giants on {on:?}");
}
