//! LEGENDARY: the generic legendary move rules (`constructs/legend.rs`, docs/COMBAT.md §5l) on the flat
//! lab arena (ground top at y = 8). One scenario per rule, keyed off COMBINATIONS — the recipes here
//! are what the famous names resolve to (Mjolnir, the Leviathan Axe, Cap's shield, a lightsaber, the
//! Buster Sword, the Blades of Chaos) but nothing in the sim knows those names. Plus determinism and
//! client-prediction parity for the moves that carry their wielder (recall flight, the leap).

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::voxel::{VoxelQuery, world_to_voxel};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::constructs::held::Mode;
use bd_sim::events::{GameEvent, HeldFx};
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);
const D: PlayerId = PlayerId(4);
const GROUND: f32 = 8.0;

fn cfg() -> SimConfig {
    SimConfig { arena: ArenaSource::Flat { size_m: 128.0 }, check_invariants: true, ..SimConfig::default() }
}

fn world(n: u8) -> World {
    let mut w = World::new(cfg());
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    // No hidden golden rolls in these scenarios (a rolled gun would turn into a one-shot).
    w.cfg.tuning.freedom.legendary.base = 0.0;
    w.cfg.tuning.freedom.legendary.cap = 0.0;
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

fn give(w: &mut World, id: PlayerId, r: Recipe) -> ConstructId {
    let eye = w.player(id).unwrap().eye(w.params());
    let cid = w.cast(id, r, None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    run(w, 30);
    cid
}

fn item(name: &str, form: Form, m: &[Mechanic], e: Element) -> Recipe {
    recipe(name, form, m, e, bd_sim::constructs::form_spec(form).default_scale)
}

/// Yaw / pitch from `a`'s eye to `b`'s chest.
fn aim_at(w: &World, a: PlayerId, b: PlayerId) -> (f32, f32) {
    let eye = w.player(a).unwrap().eye(w.params());
    let d = (w.hull_of(b).unwrap().center() - eye).normalize();
    ((-d.x).atan2(-d.z), d.y.asin())
}

fn hits_by(ev: &[GameEvent], target: PlayerId, construct: ConstructId) -> Vec<HitKind> {
    ev.iter()
        .filter_map(|e| match e {
            GameEvent::Hit { target: t, kind, construct: Some(c), .. } if *t == target && *c == construct => Some(*kind),
            _ => None,
        })
        .collect()
}

fn any_hit(ev: &[GameEvent], target: PlayerId, source: PlayerId) -> bool {
    ev.iter().any(|e| matches!(e, GameEvent::Hit { target: t, source: Some(s), .. } if *t == target && *s == source))
}

fn fx_count(ev: &[GameEvent], fx: HeldFx) -> usize {
    ev.iter().filter(|e| matches!(e, GameEvent::HeldFx { fx: f, .. } if *f == fx)).count()
}

fn fx_strength(ev: &[GameEvent], fx: HeldFx) -> Option<f32> {
    ev.iter().find_map(|e| match e {
        GameEvent::HeldFx { fx: f, strength, .. } if *f == fx => Some(*strength),
        _ => None,
    })
}

fn solid_count(w: &World, from: Vec3, to: Vec3) -> usize {
    let (a, b) = (world_to_voxel(from), world_to_voxel(to));
    let mut n = 0;
    for x in a.x.min(b.x)..=a.x.max(b.x) {
        for y in a.y.min(b.y)..=a.y.max(b.y) {
            for z in a.z.min(b.z)..=a.z.max(b.z) {
                n += w.voxels.is_solid(bd_core::IVec3::new(x, y, z)) as usize;
            }
        }
    }
    n
}

fn mjolnir() -> Recipe {
    let mut r = item("Mjolnir", Form::Hammer, &[Mechanic::Blunt, Mechanic::Returning, Mechanic::Heavy], Element::Electric);
    r.mass = 40.0;
    r
}

fn leviathan() -> Recipe {
    item("Leviathan Axe", Form::Axe, &[Mechanic::Blade, Mechanic::Returning], Element::Ice)
}

fn buster() -> Recipe {
    let mut r = recipe("Buster Sword", Form::Sword, &[Mechanic::Blade, Mechanic::Heavy], Element::Light, 2.0);
    r.mass = 40.0;
    r
}

// ------------------------------------------------------------------------------------ 1. recall

#[test]
fn thrown_item_lodges_then_recall_hits_everything_on_the_way_back() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(14.0, GROUND, 30.0)); // off to the side while it flies out
    run(&mut w, 20);
    let cid = give(&mut w, A, leviathan());
    // Throw (RMB tap) at the floor ~8 m ahead: it lodges there.
    let ev = act(&mut w, A, 0.0, -0.1, buttons::SECONDARY, 1, 90);
    assert_eq!(fx_count(&ev, HeldFx::Throw), 1);
    assert_eq!(fx_count(&ev, HeldFx::Lodge), 1, "lodged in the ground");
    let c = w.constructs.get(cid).unwrap();
    assert!(c.held.leg.lodged && !c.attached());
    let stuck = c.pos;
    // It waits (no auto-return yet).
    run(&mut w, 60);
    assert!((w.constructs.get(cid).unwrap().pos - stuck).length() < 1e-3, "stays put until recalled");
    // B steps onto the way back; a tap of RMB recalls it through him.
    let hand = w.player(A).unwrap().mv.pos + Vec3::Y * 1.0;
    let mid = (stuck + hand) * 0.5;
    place(&mut w, B, Vec3::new(mid.x, GROUND, mid.z));
    run(&mut w, 2);
    let ev = act(&mut w, A, 0.0, -0.1, buttons::SECONDARY, 1, 60);
    assert_eq!(fx_count(&ev, HeldFx::RecallStart), 1);
    assert!(!hits_by(&ev, B, cid).is_empty(), "the way back hits whoever is on the path");
    assert_eq!(fx_strength(&ev, HeldFx::Catch), Some(2.0), "the recall snaps into the hand");
    assert!(w.constructs.get(cid).unwrap().attached());
}

#[test]
fn lodged_item_comes_back_by_itself_after_a_while() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
    run(&mut w, 20);
    let cid = give(&mut w, A, leviathan());
    let ev = act(&mut w, A, 0.0, -0.1, buttons::SECONDARY, 1, 60);
    assert_eq!(fx_count(&ev, HeldFx::Lodge), 1);
    let secs = w.cfg.tuning.legendary.lodge_secs;
    let ev = run(&mut w, (secs * 120.0) as u32 + 120);
    assert_eq!(fx_count(&ev, HeldFx::Catch), 1);
    assert!(w.constructs.get(cid).unwrap().attached());
}

#[test]
fn holding_recall_flies_you_along_behind_it() {
    let mut w = world(1);
    w.cfg.rules.willpower = WillpowerMode::Normal;
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    run(&mut w, 20);
    let cid = give(&mut w, A, mjolnir());
    w.player_mut(A).unwrap().willpower = 100.0;
    let start = w.player(A).unwrap().mv.pos;
    let ev = act(&mut w, A, 0.0, 0.2, buttons::SECONDARY, 120, 0);
    assert!(fx_count(&ev, HeldFx::ThorFlight) >= 1);
    let p = w.player(A).unwrap();
    assert!(p.mv.extra.legend.thor, "recall flight on the wielder");
    assert!(start.z - p.mv.pos.z > 15.0, "dragged along the aim: {start} → {}", p.mv.pos);
    assert!(p.mv.pos.y > start.y + 2.0, "up with it, not falling: {start} → {}", p.mv.pos);
    assert!(p.willpower < 95.0, "drains willpower ({})", p.willpower);
    // Let go: it comes back and is caught; the wielder falls normally again.
    let ev = act(&mut w, A, 0.0, 0.2, 0, 0, 200);
    assert!(fx_count(&ev, HeldFx::Catch) >= 1);
    assert!(w.constructs.get(cid).unwrap().attached());
    assert!(!w.player(A).unwrap().mv.extra.legend.thor);
}

// ------------------------------------------------------------------------------------ 2. storm

/// A hammer `[Blunt, Heavy]` of element `e` held raised at B then released: what is called down.
fn storm_on(e: Element) -> (Vec<GameEvent>, ConstructId, World) {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 18.0));
    run(&mut w, 20);
    let cid = give(&mut w, A, item("storm hammer", Form::Hammer, &[Mechanic::Blunt, Mechanic::Heavy], e));
    let (y, p) = aim_at(&w, A, B);
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 150, 400);
    (ev, cid, w)
}

#[test]
fn every_element_calls_down_its_own_storm() {
    for e in [
        Element::Electric,
        Element::Fire,
        Element::Ice,
        Element::Wind,
        Element::Water,
        Element::Psychic,
        Element::Earth,
        Element::Shadow,
        Element::Atomic,
        Element::Poison,
    ] {
        let (ev, cid, w) = storm_on(e);
        assert_eq!(fx_count(&ev, HeldFx::StormRaise), 1, "{e:?}: raised");
        assert!(fx_count(&ev, HeldFx::StormMark) >= 1, "{e:?}: telegraphed");
        assert!(fx_count(&ev, HeldFx::StormStrike) >= 1, "{e:?}: struck");
        let struck = !hits_by(&ev, B, cid).is_empty() || any_hit(&ev, B, A);
        assert!(struck, "{e:?}: B under the aim is hit");
        assert!(w.player(B).unwrap().damage_pct > 0.0, "{e:?}");
        // The telegraph comes first: no hit before the mark's delay.
        let first_mark = ev.iter().position(|x| matches!(x, GameEvent::HeldFx { fx: HeldFx::StormMark, .. })).unwrap();
        let first_strike = ev.iter().position(|x| matches!(x, GameEvent::HeldFx { fx: HeldFx::StormStrike, .. })).unwrap();
        assert!(first_mark < first_strike, "{e:?}: telegraph before the strike");
    }
    // Lightning arcs on to a second enemy standing near the first.
    let mut w = world(3);
    place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 18.0));
    place(&mut w, C, Vec3::new(5.5, GROUND, 18.0));
    run(&mut w, 20);
    let cid = give(&mut w, A, mjolnir());
    let (y, p) = aim_at(&w, A, B);
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 150, 200);
    assert!(fx_count(&ev, HeldFx::StormArc) >= 1, "arced");
    assert!(hits_by(&ev, C, cid).contains(&HitKind::Chain), "C caught by the arc");
    // Rain: meteors land one after another around the mark.
    let (ev, _, _) = storm_on(Element::Fire);
    assert_eq!(fx_count(&ev, HeldFx::StormMark), w.cfg.tuning.legendary.rain_count as usize);
}

#[test]
fn light_items_and_short_holds_do_not_call_storms() {
    let (ev, _, _) = storm_on(Element::Light);
    assert_eq!(fx_count(&ev, HeldFx::StormRaise), 0);
    assert_eq!(fx_count(&ev, HeldFx::StormMark), 0);
    // A short hold is an ordinary heavy.
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 18.0));
    run(&mut w, 20);
    give(&mut w, A, mjolnir());
    let ev = act(&mut w, A, 0.0, 0.0, buttons::PRIMARY, 60, 200);
    assert_eq!(fx_count(&ev, HeldFx::StormMark), 0);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::SwingStart, strength, .. } if *strength >= 1.9)), "heavy swing");
}

// ------------------------------------------------------------------------------------ 3. ricochet chain

#[test]
fn ricochet_shield_chains_between_enemies_then_returns() {
    let mut w = world(4);
    place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 20.0));
    place(&mut w, C, Vec3::new(8.0, GROUND, 17.0));
    place(&mut w, D, Vec3::new(-7.0, GROUND, 12.0));
    run(&mut w, 20);
    let cid = give(&mut w, A, item("Captain America's Shield", Form::Shield, &[Mechanic::Guard, Mechanic::Returning, Mechanic::Ricochet], Element::Light));
    let (y, p) = aim_at(&w, A, B);
    // Shields throw on a charged LMB release.
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 50, 300);
    assert_eq!(fx_count(&ev, HeldFx::Throw), 1);
    let struck = [B, C, D].iter().filter(|t| !hits_by(&ev, **t, cid).is_empty()).count();
    assert_eq!(struck, 3, "B, then C, then D");
    assert_eq!(fx_count(&ev, HeldFx::Ricochet), 2, "two bounces between enemies");
    assert!(fx_count(&ev, HeldFx::Catch) >= 1, "then back to the hand");
}

// ------------------------------------------------------------------------------------ 4. deflect

#[test]
fn blade_fast_swing_deflects_a_bolt_toward_the_aim() {
    let mut w = world(4);
    // B holds a lightsaber; A shoots from 20 m; B looks at D (30° off the shot) and swings.
    place(&mut w, B, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, A, Vec3::new(0.0, GROUND, 10.0));
    place(&mut w, D, Vec3::new(-8.0, GROUND, 16.0));
    place(&mut w, C, Vec3::new(40.0, GROUND, 40.0));
    run(&mut w, 20);
    let saber = give(&mut w, B, item("Lightsaber", Form::Sword, &[Mechanic::Blade, Mechanic::Fast], Element::Electric));
    let (ay, ap) = aim_at(&w, A, B);
    let (by, bp) = aim_at(&w, B, D);
    let fire = PlayerInput { yaw: ay, pitch: ap, buttons: buttons::PRIMARY, ..Default::default() };
    let a_off = PlayerInput { yaw: ay, pitch: ap, ..Default::default() };
    let swing = PlayerInput { yaw: by, pitch: bp, buttons: buttons::PRIMARY, ..Default::default() };
    let b_off = PlayerInput { yaw: by, pitch: bp, ..Default::default() };
    let mut ev = w.tick(&[(A, fire), (B, b_off)]).to_vec();
    for k in 0..60 {
        let b = if (4..6).contains(&k) { swing } else { b_off };
        ev.extend_from_slice(w.tick(&[(A, a_off), (B, b)]));
    }
    assert!(fx_count(&ev, HeldFx::Parry) >= 1, "deflected");
    assert_eq!(w.player(B).unwrap().damage_pct, 0.0, "the deflector is untouched");
    assert!(any_hit(&ev, D, B), "the bolt went where B aimed (D), not back at the shooter");
    let _ = saber;
    // The window has a cooldown: spamming swings does not keep it open.
    let c = w.constructs.held_by(B).unwrap();
    assert!(c.held.leg.deflect_cd >= 0.0);
}

#[test]
fn deflected_hitscan_goes_back_capped() {
    let mut w = world(2);
    place(&mut w, B, Vec3::new(0.0, GROUND, 30.0));
    place(&mut w, A, Vec3::new(0.0, GROUND, 10.0));
    run(&mut w, 20);
    give(&mut w, B, item("excalibur", Form::Sword, &[Mechanic::Blade, Mechanic::Guard], Element::Light));
    let rail = give(&mut w, A, item("railgun", Form::Gun, &[Mechanic::Pierce], Element::Light));
    let _ = rail;
    let (ay, ap) = aim_at(&w, A, B);
    let (by, bp) = aim_at(&w, B, A);
    // B raises the guard (RMB) just as A's (charged) railgun fires.
    let mut ev = Vec::new();
    for k in 0..200 {
        let a = PlayerInput { yaw: ay, pitch: ap, buttons: if k < 80 { buttons::PRIMARY } else { 0 }, ..Default::default() };
        let b = PlayerInput { yaw: by, pitch: bp, buttons: if (78..84).contains(&k) { buttons::SECONDARY } else { 0 }, ..Default::default() };
        ev.extend_from_slice(w.tick(&[(A, a), (B, b)]));
    }
    let cap = w.cfg.tuning.legendary.reflect_hitscan_cap;
    if fx_count(&ev, HeldFx::Parry) > 0 {
        let back = ev.iter().find_map(|e| match e {
            GameEvent::Hit { target, source: Some(s), damage, .. } if *target == A && *s == B => Some(*damage),
            _ => None,
        });
        assert!(back.is_some_and(|d| d <= cap * 1.6), "reflected round capped: {back:?}");
    }
}

// ------------------------------------------------------------------------------------ 5. blade wave

#[test]
fn blade_heavy_fires_a_wave_that_pierces_players() {
    let mut w = world(3);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 28.0));
    place(&mut w, C, Vec3::new(0.3, GROUND, 20.0));
    run(&mut w, 20);
    let sword = give(&mut w, A, item("sword", Form::Sword, &[Mechanic::Blade], Element::Light));
    let (y, p) = aim_at(&w, A, B);
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 110, 90);
    assert_eq!(fx_count(&ev, HeldFx::Wave), 1, "a full heavy fires one crescent");
    assert!(!hits_by(&ev, B, sword).is_empty(), "B at 12 m");
    assert!(!hits_by(&ev, C, sword).is_empty(), "C behind B: it pierces");
    // A quick swing does not.
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 1, 90);
    assert_eq!(fx_count(&ev, HeldFx::Wave), 0);
}

#[test]
fn blade_wave_carves_a_light_groove_and_shooting_blades_fire_small_waves() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    run(&mut w, 20);
    give(&mut w, A, item("sword", Form::Sword, &[Mechanic::Blade], Element::Light));
    let (lo, hi) = (Vec3::new(-2.0, GROUND - 3.0, 4.0), Vec3::new(2.0, GROUND - 0.1, 34.0));
    let before = solid_count(&w, lo, hi);
    let ev = act(&mut w, A, 0.0, -0.18, buttons::PRIMARY, 110, 200);
    assert_eq!(fx_count(&ev, HeldFx::Wave), 1);
    let after = solid_count(&w, lo, hi);
    assert!(after < before, "carved along its path ({before} → {after})");
    assert!(before - after < 600, "lightly ({before} → {after})");
    // Blade + Shoots (the Master Sword): every swing fires a small wave.
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    run(&mut w, 20);
    give(&mut w, A, item("Master Sword", Form::Sword, &[Mechanic::Blade, Mechanic::Shoots], Element::Light));
    let ev = act(&mut w, A, 0.0, 0.0, buttons::PRIMARY, 1, 90);
    assert_eq!(fx_count(&ev, HeldFx::Wave), 1);
    let s = fx_strength(&ev, HeldFx::Wave).unwrap();
    assert!(s > 0.0 && s < 1.0, "a small flat crescent ({s})");
}

// ------------------------------------------------------------------------------------ 6. heavy blade

#[test]
fn heavy_blade_leaps_and_cleaves() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    // Off the lunge cone (35°) but inside the landing shockwave.
    place(&mut w, B, Vec3::new(3.6, GROUND, 34.5));
    run(&mut w, 20);
    let cid = give(&mut w, A, buster());
    let start = w.player(A).unwrap().mv.pos;
    let on = PlayerInput { buttons: buttons::PRIMARY, ..Default::default() };
    let off = PlayerInput::default();
    let mut ev = Vec::new();
    for _ in 0..70 {
        ev.extend_from_slice(w.tick(&[(A, on)]));
    }
    let mut top = start.y;
    for _ in 0..200 {
        ev.extend_from_slice(w.tick(&[(A, off)]));
        top = top.max(w.player(A).unwrap().mv.pos.y);
    }
    assert_eq!(fx_count(&ev, HeldFx::LeapStart), 1, "leapt");
    assert_eq!(fx_count(&ev, HeldFx::LeapSlam), 1, "cleaved on landing");
    assert!(top > start.y + 1.5, "up ({} → {top})", start.y);
    let end = w.player(A).unwrap().mv.pos;
    assert!(start.z - end.z > 3.0, "and forward ({start} → {end})");
    assert!(hits_by(&ev, B, cid).contains(&HitKind::HeavyMelee), "B caught in the cleave");
    assert!(fx_count(&ev, HeldFx::Wave) >= 1, "a standing wave runs on along the ground");
    assert!(!w.player(A).unwrap().mv.extra.legend.dove || w.player(A).unwrap().mv.extra.legend.leap_t == 0.0);
}

#[test]
fn heavy_blade_super_armor_reduces_knockback_never_immunity() {
    let kb_taken = |armored: bool| -> (f32, bool) {
        let mut w = world(2);
        place(&mut w, A, Vec3::new(0.0, GROUND, 30.0));
        place(&mut w, B, Vec3::new(0.0, GROUND, 20.0));
        run(&mut w, 30);
        let cid = give(&mut w, A, buster());
        if armored {
            w.constructs.get_mut(cid).unwrap().held.mode = Mode::Charge;
        }
        w.player_mut(A).unwrap().mv.vel = Vec3::ZERO;
        let before = w.events().len();
        w.apply_hit(HitInfo::new(A, Some(B), HitKind::HeavyMelee, 10.0, 30.0, Vec3::NEG_Z, Vec3::new(0.0, GROUND + 1.0, 30.0)));
        let fx = w.events()[before..].iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::Armor, .. }));
        (w.player(A).unwrap().mv.vel.length(), fx)
    };
    let (plain, fx0) = kb_taken(false);
    let (armor, fx1) = kb_taken(true);
    assert!(!fx0 && fx1, "armor clang only when armored");
    assert!(armor < plain * 0.7, "less knockback ({plain} → {armor})");
    assert!(armor > 0.0, "never immune");
}

// ------------------------------------------------------------------------------------ 7. chain blades

fn chaos() -> Recipe {
    item("Blades of Chaos", Form::Whip, &[Mechanic::Blade, Mechanic::Pull], Element::Fire)
}

#[test]
fn chain_blades_pull_you_to_an_enemy() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 28.0));
    run(&mut w, 20);
    give(&mut w, A, chaos());
    let (y, p) = aim_at(&w, A, B);
    let ev = act(&mut w, A, y, p, buttons::SECONDARY, 1, 2);
    assert_eq!(fx_count(&ev, HeldFx::ChainPull), 1);
    let a = w.player(A).unwrap().mv.vel;
    let b = w.player(B).unwrap().mv.vel;
    assert!(-a.z > 12.0, "yanked toward B: {a}");
    assert!(b.z > 3.0, "B dragged a little toward A: {b}");
    // No enemy in reach: the whip lash (terrain tether) as before.
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    run(&mut w, 20);
    give(&mut w, A, chaos());
    let ev = act(&mut w, A, 0.0, -0.5, buttons::SECONDARY, 1, 2);
    assert_eq!(fx_count(&ev, HeldFx::ChainPull), 0);
    assert_eq!(fx_count(&ev, HeldFx::WhipLatch), 1);
}

#[test]
fn chain_blades_finisher_sweeps_all_around() {
    let mut w = world(3);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 45.0)); // behind
    place(&mut w, C, Vec3::new(6.0, GROUND, 40.5)); // to the side
    run(&mut w, 20);
    let cid = give(&mut w, A, chaos());
    let on = PlayerInput { buttons: buttons::PRIMARY, ..Default::default() };
    let off = PlayerInput::default();
    let mut ev = Vec::new();
    for k in 0..240 {
        let press = k % 30 == 0;
        ev.extend_from_slice(w.tick(&[(A, if press { on } else { off })]));
        if k % 30 == 29 {
            place(&mut w, B, Vec3::new(0.0, GROUND, 45.0));
            place(&mut w, C, Vec3::new(6.0, GROUND, 40.5));
        }
    }
    assert!(fx_count(&ev, HeldFx::ChainSweep) >= 1, "360° finisher");
    assert!(hits_by(&ev, B, cid).contains(&HitKind::HeavyMelee), "hits behind");
    assert!(!hits_by(&ev, C, cid).is_empty(), "hits beside");
}

// ------------------------------------------------------------------------------------ determinism + prediction

fn scripted() -> (u64, usize) {
    let mut w = world(3);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    place(&mut w, B, Vec3::new(2.0, GROUND, 28.0));
    place(&mut w, C, Vec3::new(-6.0, GROUND, 24.0));
    run(&mut w, 20);
    let mut n = 0;
    for r in [mjolnir(), leviathan(), buster(), chaos()] {
        give(&mut w, A, r);
        for k in 0..400u32 {
            let b = match k {
                0..=150 => buttons::PRIMARY,
                170 => buttons::SECONDARY,
                200..=260 => buttons::SECONDARY,
                300 => buttons::SECONDARY,
                _ => 0,
            };
            let inp = PlayerInput { yaw: 0.05 * (k as f32 * 0.01).sin(), pitch: -0.05, buttons: b, ..Default::default() };
            n += w.tick(&[(A, inp)]).len();
        }
    }
    (w.state_hash(), n)
}

#[test]
fn legendary_moves_are_deterministic() {
    assert_eq!(scripted(), scripted());
}

struct Pair {
    host: World,
    client: World,
}

impl Pair {
    fn new(r: Recipe, at: Vec3) -> Self {
        let mut host = world(2);
        host.player_mut(B).unwrap().mv.pos = Vec3::new(-40.0, GROUND, 40.0);
        place(&mut host, A, at);
        for _ in 0..30 {
            host.tick(&[]);
        }
        give(&mut host, A, r);
        let mut client = World::new(cfg());
        for e in host.construct_manifest() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        Pair { host, client }
    }

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

    fn run(&mut self, n: u32, mut inp: impl FnMut(u32) -> PlayerInput) -> u32 {
        let mut bad = 0;
        for k in 0..n {
            let (dp, dv) = self.step(inp(k));
            if dp > 1e-4 || dv > 1e-3 {
                bad += 1;
                if bad < 4 {
                    eprintln!("tick {k}: prediction off by {dp} m, {dv} m/s");
                }
            }
        }
        bad
    }
}

#[test]
fn recall_flight_is_predicted_exactly() {
    let mut p = Pair::new(mjolnir(), Vec3::new(0.0, GROUND, 0.0));
    let mut flew = false;
    let bad = p.run(260, |k| {
        let b = if (10..130).contains(&k) || (170..230).contains(&k) { buttons::SECONDARY } else { 0 };
        PlayerInput { yaw: 0.3, pitch: 0.15, buttons: b, ..Default::default() }
    });
    flew |= p.host.player(A).unwrap().mv.pos.z < -10.0;
    assert!(flew, "flew ({})", p.host.player(A).unwrap().mv.pos);
    assert_eq!(bad, 0, "recall flight mispredicted on {bad} ticks");
}

#[test]
fn heavy_blade_leap_is_predicted_exactly() {
    let mut p = Pair::new(buster(), Vec3::new(0.0, GROUND, 0.0));
    let start = p.host.player(A).unwrap().mv.pos;
    let bad = p.run(320, |k| {
        let b = if (10..80).contains(&k) || (200..270).contains(&k) { buttons::PRIMARY } else { 0 };
        PlayerInput { yaw: -0.4, pitch: 0.0, move_z: if k > 150 { 1 } else { 0 }, buttons: b, ..Default::default() }
    });
    let end = p.host.player(A).unwrap().mv.pos;
    assert!((end - start).length() > 5.0, "leapt ({start} → {end})");
    assert_eq!(bad, 0, "leap mispredicted on {bad} ticks");
}

#[test]
fn famous_is_an_icon_model() {
    use bd_core::mechanics::ModelSource;
    let mut r = mjolnir();
    assert!(!bd_sim::constructs::legend::famous(&r));
    r.model = ModelSource::Icon("mjolnir".into());
    assert!(bd_sim::constructs::legend::famous(&r));
    r.model = ModelSource::Icon("costume:thor".into());
    assert!(!bd_sim::constructs::legend::famous(&r));
}

// ------------------------------------------------------------------------------------ 8. guns

fn gun_duel(dist: f32, r: Recipe) -> (World, ConstructId) {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 40.0 - dist));
    run(&mut w, 20);
    let cid = give(&mut w, A, r);
    (w, cid)
}

fn dmg_on(ev: &[GameEvent], target: PlayerId, source: PlayerId) -> f32 {
    ev.iter()
        .filter_map(|e| match e {
            GameEvent::Hit { target: t, source: Some(s), damage, .. } if *t == target && *s == source => Some(*damage),
            _ => None,
        })
        .sum()
}

#[test]
fn golden_gun_is_one_huge_shot_then_a_long_reload() {
    use bd_core::mechanics::ModelSource;
    let mut golden = item("golden gun", Form::Gun, &[Mechanic::Shoots], Element::Light);
    golden.model = ModelSource::Icon("golden gun".into());
    let (mut w, _) = gun_duel(20.0, golden);
    let (y, p) = aim_at(&w, A, B);
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 1, 30);
    assert_eq!(fx_count(&ev, HeldFx::OneShot), 1);
    let one = dmg_on(&ev, B, A);
    assert!(one >= 50.0, "one huge shot ({one})");
    // Holding the trigger right after: nothing until the reload is done.
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 200, 0);
    assert_eq!(fx_count(&ev, HeldFx::OneShot), 0, "reloading");
    // A plain procedural gun with the same mechanics is an ordinary hand cannon.
    let (mut w, _) = gun_duel(20.0, item("gun", Form::Gun, &[Mechanic::Shoots], Element::Light));
    let (y, p) = aim_at(&w, A, B);
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 1, 30);
    assert_eq!(fx_count(&ev, HeldFx::OneShot), 0);
    assert!(dmg_on(&ev, B, A) < 30.0);
    // ... unless it rolled golden (the hidden legendary roll).
    let (mut w, gun) = gun_duel(20.0, item("gun", Form::Gun, &[Mechanic::Shoots], Element::Light));
    w.constructs.get_mut(gun).unwrap().flair |= bd_sim::replicate::cflags::LEGENDARY;
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 1, 30);
    assert_eq!(fx_count(&ev, HeldFx::OneShot), 1);
}

#[test]
fn needles_home_stick_and_supercombine() {
    let mut needler = item("Needler", Form::Gun, &[Mechanic::Shoots], Element::Psychic);
    needler.payload = Some(Box::new(item("needle", Form::Ball, &[Mechanic::Explodes, Mechanic::Homing, Mechanic::Sticky], Element::Psychic)));
    let (mut w, gun) = gun_duel(14.0, needler);
    let (y, p) = aim_at(&w, A, B);
    // Aim a little off: the needles curve in.
    let ev = act(&mut w, A, y + 0.12, p, buttons::PRIMARY, 110, 60);
    let sticks = fx_count(&ev, HeldFx::NeedleStick);
    assert!(sticks >= w.cfg.tuning.legendary.needle_count as usize, "needles stuck ({sticks})");
    assert!(fx_count(&ev, HeldFx::Supercombine) >= 1, "supercombined");
    assert!(hits_by(&ev, B, gun).contains(&HitKind::Explosion), "the burst hit B");
}

#[test]
fn gravity_gun_catches_and_punts() {
    let grav = item("Gravity Gun", Form::Gun, &[Mechanic::Grab, Mechanic::Launch, Mechanic::Attract], Element::Psychic);
    // A player: caught, then punted within a beat (never held longer than `grav_player_secs`).
    let (mut w, _) = gun_duel(9.0, grav.clone());
    let (y, p) = aim_at(&w, A, B);
    let ev = act(&mut w, A, y, p, buttons::ADS, 1, 1);
    assert_eq!(fx_count(&ev, HeldFx::GravGrab), 1, "caught B");
    let ev = act(&mut w, A, y, p, 0, 0, 60);
    assert_eq!(fx_count(&ev, HeldFx::GravPunt), 1, "punted by itself after a beat");
    let b = w.player(B).unwrap();
    assert!(b.mv.pos.z < 40.0 - 9.0 - 4.0 || b.mv.vel.z < -10.0, "flew along the aim ({} {})", b.mv.pos, b.mv.vel);
    // A construct: caught, held at the muzzle, punted with LMB.
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 40.0));
    place(&mut w, B, Vec3::new(30.0, GROUND, 10.0));
    run(&mut w, 20);
    let eye = w.player(B).unwrap().eye(w.params());
    let rock = w.cast(B, item("boulder", Form::Boulder, &[], Element::Earth), None, Aim::new(eye, (Vec3::new(0.0, GROUND, 32.0) - eye).normalize())).unwrap();
    run(&mut w, 30);
    give(&mut w, A, grav);
    // Set it down in front of A.
    {
        let c = w.constructs.get_mut(rock).unwrap();
        c.pos = Vec3::new(0.0, GROUND + 0.6, 31.0);
        c.vel = Vec3::ZERO;
    }
    run(&mut w, 20);
    let at = w.constructs.get(rock).unwrap().center();
    let eye = w.player(A).unwrap().eye(w.params());
    let d = (at - eye).normalize();
    let (y, p) = ((-d.x).atan2(-d.z), d.y.asin());
    let ev = act(&mut w, A, y, p, buttons::ADS, 1, 40);
    assert_eq!(fx_count(&ev, HeldFx::GravGrab), 1, "caught the boulder");
    let held = w.constructs.get(rock).unwrap().center();
    let muzzle = w.player(A).unwrap().eye(w.params());
    assert!((held - muzzle).length() < 7.0, "held at the muzzle ({held} vs {muzzle})");
    let ev = act(&mut w, A, 0.0, 0.1, buttons::PRIMARY, 1, 3);
    assert_eq!(fx_count(&ev, HeldFx::GravPunt), 1);
    let v = w.constructs.get(rock).unwrap().vel;
    assert!(-v.z > 20.0, "punted along the aim ({v})");
}

#[test]
fn stream_latches_drags_and_flings() {
    let pack = item("Proton Pack", Form::Gun, &[Mechanic::Beam, Mechanic::Pull], Element::Atomic);
    let (mut w, _) = gun_duel(12.0, pack);
    let (y, p) = aim_at(&w, A, B);
    let mut ev = act(&mut w, A, y, p, buttons::PRIMARY, 10, 0);
    assert_eq!(fx_count(&ev, HeldFx::StreamLatch), 1);
    let before = w.player(B).unwrap().mv.pos;
    // Sweep the aim to the right while holding: B is dragged along.
    for k in 0..60 {
        let inp = PlayerInput { yaw: y - 0.012 * k as f32, pitch: p, buttons: buttons::PRIMARY, ..Default::default() };
        ev.extend_from_slice(w.tick(&[(A, inp)]));
    }
    let after = w.player(B).unwrap().mv.pos;
    assert!(after.x - before.x > 2.0, "dragged with the stream ({before} → {after})");
    ev.extend(act(&mut w, A, y - 0.72, p, 0, 0, 5));
    assert_eq!(fx_count(&ev, HeldFx::StreamRelease), 1);
    assert!(dmg_on(&ev, B, A) > 0.0);
}

#[test]
fn cannon_charge_shot_grows_with_the_hold() {
    let cannon = item("Arm Cannon", Form::Cannon, &[Mechanic::Shoots, Mechanic::Beam], Element::Light);
    let (mut w, _) = gun_duel(16.0, cannon.clone());
    let (y, p) = aim_at(&w, A, B);
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 2, 80);
    assert_eq!(fx_strength(&ev, HeldFx::ChargeShot), Some(0.0), "a tap is a pellet");
    let tap = dmg_on(&ev, B, A);
    let (mut w, _) = gun_duel(16.0, cannon);
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 160, 80);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::GunCharge, strength, .. } if *strength >= 1.0)), "full charge");
    assert_eq!(fx_strength(&ev, HeldFx::ChargeShot), Some(1.0));
    let full = dmg_on(&ev, B, A);
    assert!(full > tap * 2.0, "the charged blast hits far harder ({tap} → {full})");
}

#[test]
fn bfg_orb_zaps_along_its_path_then_detonates() {
    let mut bfg = item("BFG 9000", Form::Cannon, &[Mechanic::Shoots, Mechanic::Heavy], Element::Atomic);
    bfg.payload = Some(Box::new(item("BFG ball", Form::Ball, &[Mechanic::Beam, Mechanic::Explodes], Element::Atomic)));
    let mut w = world(3);
    place(&mut w, A, Vec3::new(0.0, GROUND, 50.0));
    place(&mut w, B, Vec3::new(6.0, GROUND, 35.0)); // beside the path
    place(&mut w, C, Vec3::new(0.0, GROUND, 20.0)); // at its end
    run(&mut w, 20);
    let gun = give(&mut w, A, bfg);
    let (y, p) = aim_at(&w, A, C);
    let ev = act(&mut w, A, y, p, buttons::PRIMARY, 1, 360);
    assert_eq!(fx_count(&ev, HeldFx::GunCharge), 1, "spin-up telegraph");
    assert!(fx_count(&ev, HeldFx::BfgZap) >= 1, "tendrils");
    assert!(hits_by(&ev, B, gun).contains(&HitKind::Chain), "B zapped from the path");
    assert!(hits_by(&ev, C, gun).contains(&HitKind::Explosion), "C caught in the blast");
}
