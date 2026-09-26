//! Held / worn / piloted constructs (COMBAT.md §4 Held + Worn, §4b guns, §3d suits, §5d piloting,
//! §5d-bis transformers, §8 Guard): scenario tests on the flat lab arena (ground top at y = 8).

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{ConstructId, IVec3, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::HitKind;
use bd_sim::events::{GameEvent, HeldFx};
use bd_sim::rules::WillpowerMode;
use bd_sim::status::StatusKind;
use bd_sim::weapons::GunArchetype;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

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
const GROUND: f32 = 8.0;

fn world(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
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
    p.mv.tumble = 0.0;
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..ticks {
        ev.extend_from_slice(w.tick(&[]));
    }
    ev
}

/// Hold `b` for `hold` ticks, then release and run `after` more ticks with the same aim.
fn act(w: &mut World, id: PlayerId, yaw: f32, pitch: f32, b: u16, hold: u32, after: u32) -> Vec<GameEvent> {
    let on = PlayerInput { yaw, pitch, buttons: b, ..Default::default() };
    let off = PlayerInput { yaw, pitch, ..Default::default() };
    let mut ev = Vec::new();
    for _ in 0..hold {
        ev.extend_from_slice(w.tick(&[(id, on)]));
    }
    for _ in 0..after {
        ev.extend_from_slice(w.tick(&[(id, off)]));
    }
    ev
}

fn give_r(w: &mut World, id: PlayerId, r: Recipe) -> ConstructId {
    let eye = w.player(id).unwrap().eye(w.params());
    let cid = w.cast(id, r, None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    run(w, 30);
    cid
}

fn give(w: &mut World, id: PlayerId, name: &str, form: Form, m: &[Mechanic]) -> ConstructId {
    let scale = bd_sim::constructs::form_spec(form).default_scale;
    give_r(w, id, recipe(name, form, m, Element::Light, scale))
}

/// Yaw / pitch from `a`'s eye to `b`'s chest.
fn aim_at(w: &World, a: PlayerId, b: PlayerId) -> (f32, f32) {
    let eye = w.player(a).unwrap().eye(w.params());
    let d = (w.hull_of(b).unwrap().center() - eye).normalize();
    ((-d.x).atan2(-d.z), d.y.asin())
}

fn hits_on(ev: &[GameEvent], target: PlayerId) -> Vec<(HitKind, f32)> {
    ev.iter()
        .filter_map(|e| match e {
            GameEvent::Hit { target: t, kind, damage, .. } if *t == target => Some((*kind, *damage)),
            _ => None,
        })
        .collect()
}

fn has_fx(ev: &[GameEvent], fx: HeldFx) -> bool {
    ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: f, .. } if *f == fx))
}

/// A and B on the ground, B `dist` metres in front of A (A looks down −Z).
fn duel(dist: f32) -> World {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 6.0 - dist));
    run(&mut w, 20);
    w
}

// ------------------------------------------------------------------------------------ melee forms

#[test]
fn every_melee_form_hits_with_a_swept_swing() {
    for (form, dist) in [
        (Form::Hammer, 1.8),
        (Form::Sword, 1.8),
        (Form::Axe, 1.7),
        (Form::Spear, 2.5),
        (Form::Shield, 1.2),
        (Form::Gauntlet, 1.2),
        (Form::Whip, 7.0),
    ] {
        let mut w = duel(dist);
        let cid = give(&mut w, A, "weapon", form, &[]);
        let (yaw, pitch) = aim_at(&w, A, B);
        let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 60);
        assert!(has_fx(&ev, HeldFx::SwingStart) && has_fx(&ev, HeldFx::SwingActive), "{form:?}: swing juice");
        let hit = ev.iter().find_map(|e| match e {
            GameEvent::Hit { target, kind: HitKind::Melee, construct, damage, .. } if *target == B => Some((*construct, *damage)),
            _ => None,
        });
        let (by, dmg) = hit.unwrap_or_else(|| panic!("{form:?} swing missed B at {dist} m"));
        assert_eq!(by, Some(cid));
        assert!((dmg - bd_sim::constructs::form_spec(form).damage).abs() < 1e-3, "{form:?} base damage {dmg}");
        assert_eq!(hits_on(&ev, B).len(), 1, "{form:?}: one hit per swing");
    }
}

#[test]
fn sword_three_hit_combo_finisher_is_stronger() {
    let mut w = duel(1.8);
    give(&mut w, A, "sword", Form::Sword, &[]);
    let mut dmgs = Vec::new();
    for _ in 0..3 {
        place(&mut w, B, Vec3::new(0.0, GROUND, 4.2));
        let (yaw, pitch) = aim_at(&w, A, B);
        let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 36);
        dmgs.extend(hits_on(&ev, B).into_iter().map(|h| h.1));
    }
    assert_eq!(dmgs.len(), 3, "{dmgs:?}");
    assert!((dmgs[2] / dmgs[0] - 1.5).abs() < 1e-3, "third hit ×1.5: {dmgs:?}");
}

#[test]
fn charged_heavy_hits_harder() {
    let mut w = duel(1.8);
    give(&mut w, A, "sword", Form::Sword, &[]);
    let (yaw, pitch) = aim_at(&w, A, B);
    let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 110, 80);
    assert!(has_fx(&ev, HeldFx::ChargeStart));
    let h = hits_on(&ev, B);
    assert_eq!(h.len(), 1, "{h:?}");
    assert_eq!(h[0].0, HitKind::HeavyMelee);
    assert!((h[0].1 - 9.0 * 2.2).abs() < 1e-3, "×2.2 at full charge: {}", h[0].1);
}

#[test]
fn spear_lunge_is_a_movement_tool() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    run(&mut w, 20);
    give(&mut w, A, "spear", Form::Spear, &[]);
    act(&mut w, A, 0.0, 0.0, buttons::SECONDARY, 1, 0);
    let v = w.player(A).unwrap().mv.vel;
    assert!(-v.z >= 17.0, "18 m/s dash along look: {v}");
    // Cooldown: an immediate second lunge does nothing extra.
    run(&mut w, 30);
    act(&mut w, A, 0.0, 0.0, buttons::SECONDARY, 1, 0);
    assert!(w.player(A).unwrap().mv.vel.length() < 17.0);
}

#[test]
fn whip_lash_yanks_players_toward_you() {
    // 8 m: between the old 3 m-spaced samples (6 m / 9 m) — the whip now samples its length every
    // ≤ 0.75 m (`held::WHIP_SAMPLE_SPACING`), so anything along the lash is hit.
    let mut w = duel(8.0);
    give(&mut w, A, "whip", Form::Whip, &[]);
    let (yaw, pitch) = aim_at(&w, A, B);
    let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 30);
    assert!(has_fx(&ev, HeldFx::WhipYank));
    assert!(!hits_on(&ev, B).is_empty());
    // B was pulled toward A (+Z) at some point.
    let mut w2 = duel(8.0);
    give(&mut w2, A, "whip", Form::Whip, &[]);
    let z0 = w2.player(B).unwrap().mv.pos.z;
    act(&mut w2, A, yaw, pitch, buttons::PRIMARY, 1, 40);
    assert!(w2.player(B).unwrap().mv.pos.z > z0 + 0.3, "yanked toward the wielder");
}

/// The lash hits anywhere along its length (dense samples), not only at the sample points.
#[test]
fn whip_lash_hits_all_along_its_length() {
    let mut missed = Vec::new();
    for d in [3.0f32, 4.5, 6.0, 7.0, 7.5, 8.0, 8.5, 10.0, 11.0] {
        let mut w = duel(d);
        give(&mut w, A, "whip", Form::Whip, &[]);
        let (yaw, pitch) = aim_at(&w, A, B);
        let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 30);
        if hits_on(&ev, B).is_empty() {
            missed.push(d);
        }
    }
    assert!(missed.is_empty(), "whip missed at {missed:?} m");
}

#[test]
fn gauntlet_grab_seizes_and_throws_into_a_wall_splat() {
    let mut w = duel(1.2);
    // A wall 5 m to B's side (+X): the throw goes that way.
    w.voxels.fill_box(IVec3::new(14, 16, -2), IVec3::new(16, 30, 18), 1);
    give(&mut w, A, "grabbing gauntlet", Form::Gauntlet, &[Mechanic::Grab]);
    let (yaw, pitch) = aim_at(&w, A, B);
    let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 12);
    assert!(has_fx(&ev, HeldFx::Grab), "seized");
    // Throw toward +X (yaw −90°).
    let ev = act(&mut w, A, -std::f32::consts::FRAC_PI_2, 0.1, buttons::SECONDARY, 1, 90);
    assert!(has_fx(&ev, HeldFx::GrabThrow));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::WallSplat { player, .. } if *player == B)), "thrown players splat");
}

#[test]
fn gauntlet_uppercut_launches() {
    let mut w = duel(1.2);
    give(&mut w, A, "gauntlet", Form::Gauntlet, &[]);
    let (yaw, pitch) = aim_at(&w, A, B);
    // (Weight pass: a default gauntlet weighs a little over nominal, so its uppercut winds up a
    // few ticks longer than the authored curve — give it the room.)
    let ev = act(&mut w, A, yaw, pitch, buttons::SECONDARY, 1, 32);
    assert!(has_fx(&ev, HeldFx::Uppercut));
    assert!(!hits_on(&ev, B).is_empty());
    // (Smash model: 0 % takes base knockback; give the rise a few more ticks.)
    run(&mut w, 8);
    assert!(w.player(B).unwrap().mv.pos.y > GROUND + 1.0, "launched up");
}

// ------------------------------------------------------------------------------------ guard

#[test]
fn perfect_shield_parry_reflects_a_projectile_to_its_shooter() {
    let mut w = duel(7.0);
    give(&mut w, A, "shield", Form::Shield, &[]);
    let (ay, ap) = aim_at(&w, A, B);
    let (by, bp) = aim_at(&w, B, A);
    let mut ev = Vec::new();
    // Same tick: B fires a ring blast, A raises the shield (the blast arrives inside 150 ms).
    let a_in = PlayerInput { yaw: ay, pitch: ap, buttons: buttons::SECONDARY, ..Default::default() };
    let b_in = PlayerInput { yaw: by, pitch: bp, buttons: buttons::PRIMARY, ..Default::default() };
    ev.extend_from_slice(w.tick(&[(A, a_in), (B, b_in)]));
    let b_off = PlayerInput { yaw: by, pitch: bp, ..Default::default() };
    for _ in 0..40 {
        ev.extend_from_slice(w.tick(&[(A, a_in), (B, b_off)]));
    }
    assert!(has_fx(&ev, HeldFx::Parry), "perfect parry");
    assert_eq!(w.player(A).unwrap().health(), 100.0, "parrier untouched");
    assert!(
        ev.iter().any(|e| matches!(e, GameEvent::Hit { target, source: Some(s), kind: HitKind::RingBlast, .. } if *target == B && *s == A)),
        "B eats its own reflected blast"
    );
}

#[test]
fn late_shield_block_absorbs_a_projectile() {
    let mut w = duel(7.0);
    give(&mut w, A, "shield", Form::Shield, &[]);
    let (ay, ap) = aim_at(&w, A, B);
    let (by, bp) = aim_at(&w, B, A);
    let a_in = PlayerInput { yaw: ay, pitch: ap, buttons: buttons::SECONDARY, ..Default::default() };
    for _ in 0..60 {
        w.tick(&[(A, a_in)]);
    }
    let b_in = PlayerInput { yaw: by, pitch: bp, buttons: buttons::PRIMARY, ..Default::default() };
    let b_off = PlayerInput { yaw: by, pitch: bp, ..Default::default() };
    let mut ev = w.tick(&[(A, a_in), (B, b_in)]).to_vec();
    for _ in 0..30 {
        ev.extend_from_slice(w.tick(&[(A, a_in), (B, b_off)]));
    }
    assert!(has_fx(&ev, HeldFx::Block) && !has_fx(&ev, HeldFx::Parry));
    assert_eq!(w.player(A).unwrap().health(), 100.0);
    assert!(hits_on(&ev, B).is_empty());
}

#[test]
fn shield_parry_staggers_a_melee_attacker() {
    let mut w = duel(1.6);
    give(&mut w, A, "shield", Form::Shield, &[]);
    give(&mut w, B, "sword", Form::Sword, &[]);
    let (ay, ap) = aim_at(&w, A, B);
    let (by, bp) = aim_at(&w, B, A);
    let mut ev = Vec::new();
    let b_in = PlayerInput { yaw: by, pitch: bp, buttons: buttons::PRIMARY, ..Default::default() };
    let b_off = PlayerInput { yaw: by, pitch: bp, ..Default::default() };
    let a_off = PlayerInput { yaw: ay, pitch: ap, ..Default::default() };
    let a_in = PlayerInput { yaw: ay, pitch: ap, buttons: buttons::SECONDARY, ..Default::default() };
    // B winds up; A raises the shield 3 ticks later — inside the window when B's blade arrives.
    ev.extend_from_slice(w.tick(&[(A, a_off), (B, b_in)]));
    for _ in 0..2 {
        ev.extend_from_slice(w.tick(&[(A, a_off), (B, b_off)]));
    }
    for _ in 0..30 {
        ev.extend_from_slice(w.tick(&[(A, a_in), (B, b_off)]));
    }
    assert!(has_fx(&ev, HeldFx::Parry), "parried");
    assert_eq!(w.player(A).unwrap().health(), 100.0);
    assert!(
        ev.iter().any(|e| matches!(e, GameEvent::StatusApplied { player, status: StatusKind::Stun, .. } if *player == B)),
        "attacker staggered"
    );
}

#[test]
fn crossing_swings_sword_lock() {
    let mut w = duel(1.5);
    give(&mut w, A, "sword", Form::Sword, &[]);
    give(&mut w, B, "sword", Form::Sword, &[]);
    let (ay, ap) = aim_at(&w, A, B);
    let (by, bp) = aim_at(&w, B, A);
    let a_in = PlayerInput { yaw: ay, pitch: ap, buttons: buttons::PRIMARY, ..Default::default() };
    let b_in = PlayerInput { yaw: by, pitch: bp, buttons: buttons::PRIMARY, ..Default::default() };
    let mut ev = w.tick(&[(A, a_in), (B, b_in)]).to_vec();
    let a_off = PlayerInput { buttons: 0, ..a_in };
    let b_off = PlayerInput { buttons: 0, ..b_in };
    for _ in 0..30 {
        ev.extend_from_slice(w.tick(&[(A, a_off), (B, b_off)]));
    }
    assert!(has_fx(&ev, HeldFx::SwordLock), "blades met");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Clash { .. })));
}

// ------------------------------------------------------------------------------------ hammer

#[test]
fn hammer_dive_slam_craters_and_knocks_back() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND + 8.0, 0.0));
    place(&mut w, B, Vec3::new(2.5, GROUND, 0.0));
    give(&mut w, A, "hammer", Form::Hammer, &[]);
    place(&mut w, A, Vec3::new(0.0, GROUND + 8.0, 0.0));
    let ev = act(&mut w, A, 0.0, -1.3, buttons::PRIMARY, 1, 90);
    assert!(has_fx(&ev, HeldFx::DiveStart) && has_fx(&ev, HeldFx::DiveSlam));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Carve { .. })), "crater");
    let h = hits_on(&ev, B);
    assert!(h.iter().any(|(k, _)| *k == HitKind::HeavyMelee), "radial shockwave: {h:?}");
    // Terrain under the landing spot is gone.
    use bd_core::voxel::VoxelQuery;
    assert!(!w.voxels.is_solid(bd_core::voxel::world_to_voxel(Vec3::new(0.0, GROUND - 0.6, 0.0))));
}

#[test]
fn returning_hammer_thor_flight_then_catch() {
    let mut w = world(1);
    w.cfg.rules.willpower = WillpowerMode::Normal;
    place(&mut w, A, Vec3::new(0.0, GROUND + 26.0, 40.0));
    let cid = give(&mut w, A, "mjolnir", Form::Hammer, &[Mechanic::Returning]);
    place(&mut w, A, Vec3::new(0.0, GROUND + 26.0, 40.0));
    w.player_mut(A).unwrap().willpower = 100.0;
    let start = w.player(A).unwrap().mv.pos;
    let ev = act(&mut w, A, 0.0, 0.08, buttons::SECONDARY, 150, 0);
    // (Released on the next tick: ct(.., 0, 1) below.)
    assert!(has_fx(&ev, HeldFx::Throw) && has_fx(&ev, HeldFx::ThorFlight));
    let p = w.player(A).unwrap();
    assert!(start.z - p.mv.pos.z > 20.0, "dragged along the look: {} → {}", start, p.mv.pos);
    assert!(p.mv.pos.y > start.y, "flying, not falling");
    assert!(p.willpower < 90.0, "Thor flight drains willpower: {}", p.willpower);
    assert!(!w.constructs.get(cid).unwrap().attached(), "out of the hand");
    let ev = act(&mut w, A, 0.0, 0.08, 0, 0, 150);
    assert!(has_fx(&ev, HeldFx::Catch), "returns and is caught");
    assert!(w.constructs.get(cid).unwrap().attached());
}

#[test]
fn thrust_hammer_lunges_the_wielder() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    run(&mut w, 20);
    give(&mut w, A, "rocket hammer", Form::Hammer, &[Mechanic::Thrust]);
    act(&mut w, A, 0.0, 0.0, buttons::PRIMARY, 1, 0);
    assert!(-w.player(A).unwrap().mv.vel.z > 10.0);
}

// ------------------------------------------------------------------------------------ whip tether

#[test]
fn whip_on_terrain_is_a_third_tether_you_can_swing_on() {
    let mut w = world(1);
    // An overhang slab at y = 20 m.
    w.voxels.fill_box(IVec3::new(-8, 40, -8), IVec3::new(8, 42, 8), 1);
    place(&mut w, A, Vec3::new(8.0, GROUND + 1.0, 0.0));
    give(&mut w, A, "whip", Form::Whip, &[]);
    place(&mut w, A, Vec3::new(7.0, 14.0, 0.0));
    let eye = w.player(A).unwrap().eye(w.params());
    let d = (Vec3::new(1.0, 19.9, 0.0) - eye).normalize();
    let (yaw, pitch) = ((-d.x).atan2(-d.z), d.y.asin());
    let inp = PlayerInput { yaw, pitch, buttons: buttons::SECONDARY, ..Default::default() };
    let ev = w.tick(&[(A, inp)]).to_vec();
    assert!(has_fx(&ev, HeldFx::WhipLatch), "latched");
    let mut min_y = f32::MAX;
    let mut max_speed: f32 = 0.0;
    let mut min_x = f32::MAX;
    for _ in 0..150 {
        w.tick(&[(A, inp)]);
        let p = w.player(A).unwrap();
        min_y = min_y.min(p.mv.pos.y);
        min_x = min_x.min(p.mv.pos.x);
        max_speed = max_speed.max(p.mv.horizontal_speed());
    }
    assert!(min_y > GROUND + 1.0, "the rope held: lowest y {min_y}");
    assert!(max_speed > 6.0, "pendulum swing speed {max_speed}");
    assert!(min_x < 2.5, "swung under the anchor: {min_x}");
}

// ------------------------------------------------------------------------------------ ranged held

#[test]
fn ranged_held_forms_fire() {
    for (form, arch, hold) in [(Form::Bow, GunArchetype::Bow, 90), (Form::Cannon, GunArchetype::Cannon, 1), (Form::Staff, GunArchetype::Staff, 1), (Form::Disc, GunArchetype::Disc, 1)] {
        let mut w = duel(12.0);
        give(&mut w, A, "ranged", form, &[]);
        let (yaw, pitch) = aim_at(&w, A, B);
        let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, hold, 60);
        assert!(ev.iter().any(|e| matches!(e, GameEvent::ShotFired { archetype, .. } if *archetype == arch)), "{form:?} fired");
        assert!(!hits_on(&ev, B).is_empty(), "{form:?} hit B");
    }
}

#[test]
fn staff_beam_while_held() {
    let mut w = duel(10.0);
    give(&mut w, A, "beam staff", Form::Staff, &[Mechanic::Beam]);
    let (yaw, pitch) = aim_at(&w, A, B);
    let ev = act(&mut w, A, yaw, pitch, buttons::SECONDARY, 60, 0);
    assert!(ev.iter().filter(|e| matches!(e, GameEvent::Hitscan { .. })).count() >= 4);
    assert!(hits_on(&ev, B).len() >= 4, "continuous beam");
}

// ------------------------------------------------------------------------------------ guns

fn gun(w: &mut World, id: PlayerId, name: &str, m: &[Mechanic], count: u8) -> ConstructId {
    let mut r = recipe(name, Form::Gun, m, Element::Light, 0.6);
    r.count = count;
    give_r(w, id, r)
}

#[test]
fn super_shotgun_double_barrel_jump() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    gun(&mut w, A, "shotgun", &[Mechanic::Shoots], 3);
    place(&mut w, A, Vec3::new(0.0, 30.0, 0.0));
    let y0 = w.player(A).unwrap().mv.pos.y;
    let ev = act(&mut w, A, 0.0, -1.55, buttons::SECONDARY, 1, 0);
    assert_eq!(ev.iter().filter(|e| matches!(e, GameEvent::ShotFired { archetype: GunArchetype::SuperShotgun, .. })).count(), 2, "both barrels");
    assert!(w.player(A).unwrap().mv.vel.y > 16.0, "double recoil kick {}", w.player(A).unwrap().mv.vel.y);
    let mut apex = y0;
    for _ in 0..120 {
        w.tick(&[]);
        apex = apex.max(w.player(A).unwrap().mv.pos.y);
    }
    assert!(apex - y0 > 5.0, "shotgun-jump height {}", apex - y0);
}

#[test]
fn rocket_jump_beats_a_plain_jump() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    gun(&mut w, A, "rocket launcher", &[Mechanic::Explodes], 1);
    run(&mut w, 30);
    let inp = PlayerInput { pitch: -1.55, buttons: buttons::PRIMARY | buttons::JUMP, ..Default::default() };
    let mut apex: f32 = 0.0;
    w.tick(&[(A, inp)]);
    for _ in 0..120 {
        w.tick(&[(A, PlayerInput { pitch: -1.55, ..Default::default() })]);
        apex = apex.max(w.player(A).unwrap().mv.pos.y - GROUND);
    }
    let plain = 9.4f32 * 9.4 / (2.0 * 25.0);
    assert!(apex > plain + 1.0, "rocket jump apex {apex} vs plain {plain}");
    assert_eq!(w.player(A).unwrap().health(), 100.0, "no self damage");
}

#[test]
fn minigun_pointed_down_hovers_at_a_slow_sink() {
    let fall_speed = |fire: bool| {
        let mut w = world(1);
        place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
        gun(&mut w, A, "minigun", &[Mechanic::Spin], 1);
        place(&mut w, A, Vec3::new(0.0, 90.0, 0.0));
        let inp = PlayerInput { pitch: -1.5, buttons: if fire { buttons::PRIMARY } else { 0 }, ..Default::default() };
        for _ in 0..150 {
            w.tick(&[(A, inp)]);
        }
        -w.player(A).unwrap().mv.vel.y
    };
    let hover = fall_speed(true);
    let free = fall_speed(false);
    assert!(hover <= bd_sim::constructs::gun::MINIGUN_SINK + 0.3, "hover sink rate {hover}");
    assert!(free > 20.0, "free fall {free}");
}

#[test]
fn railgun_construct_pierces_two_players() {
    let mut w = world(3);
    place(&mut w, A, Vec3::new(0.0, GROUND, 10.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 2.0));
    place(&mut w, C, Vec3::new(0.0, GROUND, -6.0));
    run(&mut w, 20);
    gun(&mut w, A, "railgun", &[Mechanic::Pierce], 1);
    let (yaw, pitch) = aim_at(&w, A, B);
    let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 70, 2);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Hitscan { hits: 2, .. })), "pierced");
    assert!(!hits_on(&ev, B).is_empty() && !hits_on(&ev, C).is_empty());
}

#[test]
fn hand_cannon_fan_burst_and_payload_rounds() {
    let mut w = duel(10.0);
    gun(&mut w, A, "revolver", &[], 1);
    let (yaw, pitch) = aim_at(&w, A, B);
    let ev = act(&mut w, A, yaw, pitch, buttons::SECONDARY, 1, 40);
    assert_eq!(ev.iter().filter(|e| matches!(e, GameEvent::ShotFired { archetype: GunArchetype::HandCannon, .. })).count(), 3, "fan the hammer");
    // A cannon that shoots anvils: slow heavy arcing rounds.
    let mut w = duel(10.0);
    let mut r = recipe("anvil cannon", Form::Cannon, &[], Element::Light, 1.2);
    r.payload = Some(Box::new(recipe("anvil", Form::Anvil, &[], Element::Light, 1.0)));
    give_r(&mut w, A, r);
    let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 5);
    let v = ev.iter().find_map(|e| match e {
        GameEvent::ProjectileFired { owner, vel, .. } if *owner == A => Some(vel.length()),
        _ => None,
    });
    assert!(v.is_some_and(|v| v < 30.0), "novelty round is slow: {v:?}");
}

#[test]
fn dual_wield_alternates_barrels() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    gun(&mut w, A, "pistols", &[], 2);
    let ev = act(&mut w, A, 0.0, 0.0, buttons::PRIMARY, 60, 0);
    let muzzles: Vec<Vec3> = ev.iter().filter_map(|e| match e { GameEvent::ShotFired { muzzle, .. } => Some(*muzzle), _ => None }).collect();
    assert!(muzzles.len() >= 3);
    assert!((muzzles[0].x - muzzles[1].x).abs() > 0.3, "left / right barrels");
}

// ------------------------------------------------------------------------------------ worn

#[test]
fn parachute_saves_a_launch() {
    let launched = |chute: bool| {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
        if chute {
            give(&mut w, A, "parachute", Form::Parachute, &[]);
        }
        place(&mut w, A, Vec3::new(0.0, GROUND + 2.0, 0.0));
        w.apply_hit(bd_sim::combat::HitInfo::new(A, Some(B), HitKind::Explosion, 5.0, 30.0, Vec3::new(1.0, 1.5, 0.0), Vec3::ZERO));
        assert!(w.player(A).unwrap().mv.tumble > 0.0);
        let hold = PlayerInput { buttons: buttons::JUMP, ..Default::default() };
        for _ in 0..12 {
            w.tick(&[(A, hold)]);
        }
        let tumble = w.player(A).unwrap().mv.tumble;
        let mut max_fall: f32 = 0.0;
        for _ in 0..400 {
            w.tick(&[(A, hold)]);
            let p = w.player(A).unwrap();
            if p.mv.grounded {
                break;
            }
            max_fall = max_fall.max(-p.mv.vel.y);
        }
        (tumble, max_fall)
    };
    let (t_chute, fall_chute) = launched(true);
    let (t_none, fall_none) = launched(false);
    assert_eq!(t_chute, 0.0, "canopy cancels tumble");
    assert!(t_none > 0.0);
    assert!(fall_chute <= 3.3, "slow fall {fall_chute}");
    assert!(fall_none > 10.0, "no chute: {fall_none}");
}

#[test]
fn wings_flap_three_times_then_need_ground() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    give(&mut w, A, "wings", Form::Wings, &[]);
    place(&mut w, A, Vec3::new(0.0, 60.0, 0.0));
    run(&mut w, 30);
    let mut flaps = 0;
    for _ in 0..5 {
        let ev = act(&mut w, A, 0.0, 0.0, buttons::JUMP, 1, 20);
        flaps += ev.iter().filter(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::Flap, .. })).count();
    }
    assert_eq!(flaps, 3);
    // Glide: holding jump turns the fall into forward speed, capped.
    let ev = act(&mut w, A, 0.0, 0.0, buttons::JUMP, 240, 0);
    let _ = ev;
    let p = w.player(A).unwrap();
    assert!(-p.mv.vel.z > 5.0 && p.mv.horizontal_speed() < 24.0, "glide {}", p.mv.vel);
}

#[test]
fn jetpack_thrust_is_capped_and_drains() {
    let mut w = world(1);
    w.cfg.rules.willpower = WillpowerMode::Normal;
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    give(&mut w, A, "jetpack", Form::Jetpack, &[]);
    w.player_mut(A).unwrap().willpower = 100.0;
    act(&mut w, A, 0.0, 0.6, buttons::JUMP, 240, 0);
    let p = w.player(A).unwrap();
    assert!(p.mv.pos.y > GROUND + 10.0, "flew up: {}", p.mv.pos);
    assert!(p.mv.vel.length() < 18.5, "capped below skilled movement: {}", p.mv.vel.length());
    assert!(p.willpower < 95.0);
}

#[test]
fn cape_mid_air_dash_once() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    give(&mut w, A, "cape", Form::Cape, &[]);
    place(&mut w, A, Vec3::new(0.0, 40.0, 0.0));
    run(&mut w, 20);
    let ev = act(&mut w, A, 0.0, 0.0, buttons::JUMP, 1, 3);
    assert!(has_fx(&ev, HeldFx::CapeDash));
    assert!(-w.player(A).unwrap().mv.vel.z > 10.0);
    let ev = act(&mut w, A, 0.0, 0.0, buttons::JUMP, 1, 3);
    assert!(!has_fx(&ev, HeldFx::CapeDash), "one per air time");
}

// ------------------------------------------------------------------------------------ piloting

#[test]
fn mech_piloting_punches_and_ejects() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 10.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, -30.0));
    run(&mut w, 20);
    let eye = w.player(A).unwrap().eye(w.params());
    let mech = w.cast(A, recipe("giant mech", Form::Biped, &[Mechanic::Piloted], Element::Light, 4.0), None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0))).unwrap();
    assert!(!w.player(A).unwrap().seat.unwrap().suit);
    run(&mut w, 90);
    assert!(w.constructs.get(mech).unwrap().held.third_person, "camera pulls back");
    let start = w.constructs.get(mech).unwrap().pos;
    let fwd = PlayerInput { move_z: 1, ..Default::default() };
    for _ in 0..240 {
        w.tick(&[(A, fwd)]);
    }
    let c = w.constructs.get(mech).unwrap();
    assert!(start.z - c.pos.z > 5.0, "WASD drives it: {start} → {}", c.pos);
    // Punch B when in reach.
    let front = c.pos + Vec3::new(0.0, 0.0, -2.5);
    place(&mut w, B, Vec3::new(front.x, GROUND, front.z));
    let ev = act(&mut w, A, 0.0, -0.2, buttons::PRIMARY, 1, 5);
    assert!(hits_on(&ev, B).iter().any(|(k, _)| *k == HitKind::Creature), "mech punch");
    assert!(has_fx(&ev, HeldFx::PilotAttack));
    // E ejects with a launch.
    let ev = act(&mut w, A, 0.0, 0.0, buttons::TETHER_R, 1, 0);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Ejected { .. })));
    let p = w.player(A).unwrap();
    assert!(p.seat.is_none() && p.mv.vel.y > 15.0);
}

#[test]
fn blade_on_a_weak_point_hits_three_times_harder() {
    let mut w = world(2);
    place(&mut w, B, Vec3::new(0.0, GROUND, -8.0));
    run(&mut w, 20);
    let eye = w.player(B).unwrap().eye(w.params());
    let mech = w.cast(B, recipe("mech", Form::Biped, &[Mechanic::Piloted], Element::Light, 4.0), None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0))).unwrap();
    run(&mut w, 90);
    let c = w.constructs.get(mech).unwrap();
    let p = bd_sim::constructs::Proxy::of(c);
    let mut pts = [(Vec3::ZERO, 0.0); 4];
    let n = bd_sim::constructs::pilot::weak_points(&p, &mut pts);
    assert_eq!(n, 4);
    assert_eq!(bd_sim::constructs::pilot::weak_mult(&p, pts[0].0, true), 3.0, "nape vs blade");
    assert_eq!(bd_sim::constructs::pilot::weak_mult(&p, pts[0].0, false), 1.0, "blunt: no bonus");
}

#[test]
fn power_suit_absorbs_damage_and_punches() {
    let mut w = duel(1.8);
    let eye = w.player(A).unwrap().eye(w.params());
    let suit = w.cast(A, recipe("iron man suit", Form::Biped, &[Mechanic::Piloted], Element::Light, 2.0), None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    assert!(w.player(A).unwrap().seat.unwrap().suit);
    run(&mut w, 100);
    let integ = w.constructs.get(suit).unwrap().integrity;
    w.apply_hit(bd_sim::combat::HitInfo::new(A, Some(B), HitKind::Shot, 10.0, 0.0, Vec3::X, Vec3::ZERO));
    assert_eq!(w.player(A).unwrap().health(), 100.0, "the suit took it");
    assert!(w.constructs.get(suit).unwrap().integrity < integ);
    place(&mut w, B, Vec3::new(0.0, GROUND, 4.4));
    let (yaw, pitch) = aim_at(&w, A, B);
    let ev = act(&mut w, A, yaw, pitch, buttons::PRIMARY, 1, 5);
    assert!(hits_on(&ev, B).iter().any(|(k, _)| *k == HitKind::Melee), "suit punch replaces the ring blast");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::ShotFired { archetype: GunArchetype::Ring, .. })));
}

#[test]
fn transformer_suit_drives_ramps_transforms_midair_and_slams() {
    let mut w = world(2);
    // x = −40 is a clear lane through the lab arena.
    place(&mut w, A, Vec3::new(-40.0, GROUND, 44.0));
    place(&mut w, B, Vec3::new(30.0, GROUND, 0.0));
    run(&mut w, 20);
    let eye = w.player(A).unwrap().eye(w.params());
    let r = recipe("transforming car", Form::Vehicle, &[Mechanic::Piloted, Mechanic::Transforms], Element::Light, 2.0);
    let suit = w.cast(A, r, None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    assert!(w.player(A).unwrap().seat.is_some_and(|s| s.suit));
    run(&mut w, 30);
    let c = w.constructs.get(suit).unwrap();
    assert!(c.held.vehicle && c.held.third_person, "starts as a car, third-person");
    // Drive.
    let drive = PlayerInput { move_z: 1, ..Default::default() };
    let mut top: f32 = 0.0;
    for _ in 0..150 {
        w.tick(&[(A, drive)]);
        top = top.max(w.player(A).unwrap().mv.horizontal_speed());
    }
    assert!(top > 20.0 && top <= 24.5, "vehicle speed cap 24 m/s: {top}");
    // A ramp ahead launches the car.
    let pos = w.player(A).unwrap().mv.pos;
    let ramp_at = pos + Vec3::new(0.0, 0.0, -14.0);
    let ramp = recipe("ramp", Form::Ramp, &[], Element::Light, 6.0);
    // B (beside the lane, behind the ramp spot) casts it: it rises away from the caster → toward −Z.
    place(&mut w, B, ramp_at + Vec3::new(-3.0, 0.0, 12.0));
    let beye = w.player(B).unwrap().eye(w.params());
    w.cast(B, ramp, None, Aim::new(beye, (ramp_at - beye).normalize())).ok();
    let mut airborne_at = None;
    for k in 0..120 {
        w.tick(&[(A, drive)]);
        let p = w.player(A).unwrap();
        if p.mv.pos.y > GROUND + 1.5 {
            airborne_at = Some(k);
            break;
        }
    }
    assert!(airborne_at.is_some(), "the ramp launched the car");
    // Transform mid-air → robot, land → slam.
    assert!(w.toggle_transform(A));
    let mut ev = Vec::new();
    for _ in 0..240 {
        ev.extend_from_slice(w.tick(&[(A, PlayerInput::default())]));
    }
    assert!(has_fx(&ev, HeldFx::Transform));
    assert!(has_fx(&ev, HeldFx::TransformSlam), "landing slam");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Carve { .. })));
    let c = w.constructs.get(suit).unwrap();
    assert!(!c.held.vehicle && !c.held.third_person, "robot mode");
    assert!(!w.toggle_transform(B), "B isn't seated in a transformer");
}

#[test]
fn held_scenarios_are_deterministic() {
    let run_once = || {
        let mut w = duel(1.8);
        give(&mut w, A, "sword", Form::Sword, &[Mechanic::Explodes]);
        give(&mut w, B, "shield", Form::Shield, &[Mechanic::Returning]);
        let (ay, ap) = aim_at(&w, A, B);
        let (by, bp) = aim_at(&w, B, A);
        let mut log = Vec::new();
        for k in 0..400u32 {
            let a = PlayerInput { yaw: ay, pitch: ap, buttons: if k % 37 < 2 { buttons::PRIMARY } else { 0 }, ..Default::default() };
            let b = PlayerInput { yaw: by, pitch: bp, buttons: if k % 53 < 20 { buttons::SECONDARY } else if k % 41 < 30 { buttons::PRIMARY } else { 0 }, ..Default::default() };
            log.push(format!("{:?}", w.tick(&[(A, a), (B, b)])));
        }
        (log, w.state_hash())
    };
    assert_eq!(run_once(), run_once());
}
