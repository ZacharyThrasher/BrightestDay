//! Generic power effects (bd_sim `powers::effects`), techniques, tiers and power MIXING
//! (COMBAT.md §5h): AutoDodge, ProjectileSlowField, SelfDrain + PowerMult, CritChance, tiered
//! transforms + the awakening Ascend, several powers held at once (lend-grafts, per-power
//! cooldowns, cross-power shouts), Universal Pull, Planetary Devastation, host / prediction parity.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::movement::MoveModifiers;
use bd_sim::powers::{self, PlayerPower, PowerEvent, PowerPhrase, PowerTable, move_sel};
use bd_sim::projectile::ProjectileSpawn;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn duel(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
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

fn stand_at(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, 8.0, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn pid(key: &str) -> u16 {
    PowerTable::get().index_of(key).unwrap_or_else(|| panic!("no power {key}"))
}

fn power_events(evs: &[GameEvent]) -> Vec<PowerEvent> {
    evs.iter().filter_map(|e| if let GameEvent::Power(p) = e { Some(p.clone()) } else { None }).collect()
}

fn grants(name: &str, key: &str) -> Recipe {
    let mut r = recipe(name, Form::Ball, &[], Element::Light, 0.35);
    r.grants = Some(key.to_string());
    r
}

fn eye_aim(w: &World, id: PlayerId, dir: Vec3) -> Aim {
    let p = w.player(id).unwrap();
    Aim::new(p.eye(w.params()), dir.normalize())
}

fn pct(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().damage_pct
}

/// A hit on `target` from `source` (fresh %, so knockback doesn't carry players away).
fn hit(w: &mut World, target: PlayerId, source: PlayerId, kind: HitKind, damage: f32) -> f32 {
    // Fresh victim: no juggle decay, no momentum bonus.
    for id in [target, source] {
        let p = w.player_mut(id).unwrap();
        p.juggle = Default::default();
        p.mv.vel = Vec3::ZERO;
    }
    w.player_mut(target).unwrap().damage_pct = 0.0;
    let before = pct(w, target);
    let at = w.player(target).unwrap().mv.pos + Vec3::Y;
    w.apply_hit(HitInfo::new(target, Some(source), kind, damage, 4.0, Vec3::X, at));
    pct(w, target) - before
}

// ------------------------------------------------------------------------------------ AutoDodge

#[test]
fn ultra_instinct_auto_dodges_with_a_cooldown_and_never_dodges_explosions() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, 6.0);
    assert!(w.grant_power(A, "ultra_instinct"));
    run(&mut w, 2);
    let (mut dodged, mut landed) = (0, 0);
    let mut dodged_at: Vec<u64> = Vec::new();
    for _ in 0..240 {
        stand_at(&mut w, A, 0.0, 0.0);
        let evs0 = w.events().len();
        let d = hit(&mut w, A, B, HitKind::RingBlast, 10.0);
        let dodge_ev = w.events()[evs0..].iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::AutoDodged { player: A, .. })));
        if dodge_ev {
            dodged += 1;
            assert_eq!(d, 0.0, "a dodged hit deals nothing");
            let p = w.player(A).unwrap();
            assert!(p.mv.extra.dodging() && p.mv.extra.since_dodge == 0.0, "the ordinary dodge (i-frames + burst) started");
            dodged_at.push(w.tick);
        } else if d > 0.0 {
            landed += 1;
        }
        run(&mut w, 30); // 0.25 s between hits
    }
    assert!(dodged >= 10 && landed >= 10, "some dodged ({dodged}), some landed ({landed})");
    // Cooldown 1.6 s: two auto-dodges are never closer than that.
    for pair in dodged_at.windows(2) {
        assert!(pair[1] - pair[0] >= (1.6 * 120.0) as u64 - 1, "cooldown respected: {pair:?}");
    }
    // Explosions are never auto-dodged (area damage: move out of it).
    let mut w2 = duel(2);
    stand_at(&mut w2, A, 0.0, 0.0);
    assert!(w2.grant_power(A, "ultra_instinct"));
    for _ in 0..40 {
        let evs0 = w2.events().len();
        hit(&mut w2, A, B, HitKind::Explosion, 5.0);
        assert!(!w2.events()[evs0..].iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::AutoDodged { .. }))));
        run(&mut w2, 200);
    }
}

#[test]
fn auto_dodge_is_deterministic() {
    let seq = || {
        let mut w = duel(2);
        stand_at(&mut w, A, 0.0, 0.0);
        w.grant_power(A, "ultra_instinct");
        let mut out = Vec::new();
        for _ in 0..80 {
            stand_at(&mut w, A, 0.0, 0.0);
            out.push(hit(&mut w, A, B, HitKind::Melee, 8.0) == 0.0);
            run(&mut w, 25);
        }
        out
    };
    assert_eq!(seq(), seq());
}

// ------------------------------------------------------------------------------------ Infinity

/// Fire a ring blast from B at A and return (min speed seen within `r` m of A, did it arrive).
fn blast_at_a(w: &mut World, r: f32) -> (f32, bool) {
    let from = w.player(B).unwrap().mv.pos + Vec3::Y * 1.2;
    let to = w.player(A).unwrap().mv.pos + Vec3::Y * 1.0;
    let mut s = ProjectileSpawn::ring_blast(B, from, (to - from).normalize());
    s.vel *= 40.0 / s.vel.length(); // slow enough to watch
    s.life = 6.0;
    let id = w.spawn_projectile(s, None).expect("projectile");
    let mut min_speed = f32::INFINITY;
    let mut arrived = false;
    for _ in 0..720 {
        stand_at(w, A, 0.0, 0.0);
        let evs = w.tick(&[]).to_vec();
        if let Some(p) = w.projectiles.iter().find(|p| p.id == id) {
            if (p.pos - to).length() < r {
                min_speed = min_speed.min(p.vel.length());
            }
        }
        if evs.iter().any(|e| matches!(e, GameEvent::ProjectileEnded { id: x, hit_player: Some(A), .. } if *x == id)) {
            arrived = true;
            break;
        }
        if !w.projectiles.iter().any(|p| p.id == id) {
            break;
        }
    }
    (min_speed, arrived)
}

#[test]
fn infinity_slows_projectiles_to_a_crawl_but_they_still_arrive() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, 20.0);
    let (plain, _) = blast_at_a(&mut w, 3.0);
    assert!(plain > 30.0, "no field: full speed ({plain})");
    assert!(w.grant_power(A, "limitless"));
    run(&mut w, 2);
    let (slowed, arrived) = blast_at_a(&mut w, 3.0);
    assert!(slowed <= powers::effects::SLOW_FIELD_CRAWL + 0.5, "crawls near the holder ({slowed})");
    assert!(arrived, "a slowed round still arrives (not invulnerable)");
    // Melee is untouched.
    let d = hit(&mut w, A, B, HitKind::Melee, 10.0);
    assert!(d >= 9.9, "melee lands in full ({d})");
    // The holder's own shots are never slowed.
    let from = w.player(A).unwrap().mv.pos + Vec3::Y * 1.2;
    let id = w.spawn_projectile(ProjectileSpawn::ring_blast(A, from, Vec3::Z), None).unwrap();
    run(&mut w, 2);
    if let Some(p) = w.projectiles.iter().find(|p| p.id == id) {
        assert!(p.vel.length() > 100.0, "own shot at full speed");
    }
}

#[test]
fn infinity_slows_thrown_things() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, 14.0);
    assert!(w.grant_power(A, "limitless"));
    w.player_mut(B).unwrap().willpower = 100.0;
    let aim = eye_aim(&w, B, Vec3::new(0.0, 0.05, -1.0));
    let cid = w.cast(B, recipe("rock", Form::Boulder, &[], Element::Light, 0.6), None, aim).expect("thrown");
    let mut min_speed = f32::INFINITY;
    for _ in 0..240 {
        stand_at(&mut w, A, 0.0, 0.0);
        w.tick(&[]);
        let Some(c) = w.constructs.get(cid) else { break };
        if c.comp.launched && (c.center() - Vec3::new(0.0, 9.0, 0.0)).length() < 3.0 {
            min_speed = min_speed.min(c.vel.length());
        }
    }
    assert!(min_speed < 6.0, "thrown thing crawls in the field ({min_speed})");
}

// ------------------------------------------------------------------------------------ SelfDrain + PowerMult

#[test]
fn kaioken_trades_self_damage_for_power_and_scales_with_the_multiplier() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, 30.0);
    let base = hit(&mut w, B, A, HitKind::Melee, 10.0);
    assert!(w.grant_power(A, "kaioken"));
    let x1 = hit(&mut w, B, A, HitKind::Melee, 10.0);
    assert!((x1 / base - 1.15).abs() < 0.02, "x1 ×1.15 ({base} → {x1})");
    // Self-drain: your own % climbs while it lasts.
    w.player_mut(A).unwrap().damage_pct = 0.0;
    run(&mut w, 120);
    let drained = pct(&w, A);
    assert!((drained - 1.0).abs() < 0.1, "1 %/s at x1 ({drained})");
    // "kaioken times ten": tier 4 (the ×10 row).
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = eye_aim(&w, A, Vec3::NEG_Z);
    w.cast(A, grants("kaioken x10", "kaioken@4"), None, aim).ok();
    assert_eq!(w.player(A).unwrap().power.tier, 4);
    let x10 = hit(&mut w, B, A, HitKind::Melee, 10.0);
    assert!((x10 / base - 1.9).abs() < 0.03, "x10 ×1.9 ({x10})");
    w.player_mut(A).unwrap().damage_pct = 0.0;
    run(&mut w, 120);
    assert!((pct(&w, A) - 8.0).abs() < 0.3, "8 %/s at x10 ({})", pct(&w, A));
    // Speed: host movement modifiers carry the ×1.35.
    let mut m = MoveModifiers::default();
    w.player(A).unwrap().power.modify_movement(true, &mut m);
    assert!((m.speed_mult - 1.35).abs() < 0.01, "speed ×1.35 ({})", m.speed_mult);
}

#[test]
fn self_drain_takes_willpower_for_forms() {
    let mut w = duel(1);
    stand_at(&mut w, A, 0.0, 0.0);
    assert!(w.grant_power(A, "super_saiyan"));
    w.player_mut(A).unwrap().willpower = 80.0;
    w.player_mut(A).unwrap().regen_pause = 100.0;
    run(&mut w, 120);
    let after = w.player(A).unwrap().willpower;
    assert!(after < 80.0 && after > 78.0, "SSJ drains ~0.6/s ({after})");
}

// ------------------------------------------------------------------------------------ CritChance

#[test]
fn black_flash_crit_rate_and_determinism() {
    let seq = |n: usize| {
        let mut w = duel(2);
        stand_at(&mut w, A, 0.0, 0.0);
        stand_at(&mut w, B, 0.0, 40.0);
        assert!(w.grant_power(A, "black_flash_technique"));
        let mut crits = Vec::with_capacity(n);
        for _ in 0..n {
            let evs0 = w.events().len();
            let d = hit(&mut w, B, A, HitKind::Melee, 10.0);
            let ev = w.events()[evs0..].iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Crit { attacker: A, target: B, .. })));
            assert_eq!(ev, d > 20.0, "a crit is ×2.5 and announced ({d})");
            crits.push(ev);
            w.tick(&[]);
        }
        crits
    };
    let a = seq(1500);
    let rate = a.iter().filter(|c| **c).count() as f32 / a.len() as f32;
    assert!((rate - 0.22).abs() < 0.04, "22 % crit rate ({rate})");
    assert_eq!(a, seq(1500), "deterministic");
    // Ranged hits never crit.
    let mut w = duel(2);
    stand_at(&mut w, B, 0.0, 40.0);
    w.grant_power(A, "black_flash_technique");
    for _ in 0..200 {
        let d = hit(&mut w, B, A, HitKind::Shot, 10.0);
        assert!(d < 11.0);
        w.tick(&[]);
    }
}

#[test]
fn crit_roll_is_uniform() {
    let n = 20_000u64;
    let mut buckets = [0u32; 10];
    for k in 0..n {
        let r = powers::effects::roll(k.wrapping_mul(0x9E37));
        assert!((0.0..1.0).contains(&r));
        buckets[(r * 10.0) as usize] += 1;
    }
    for b in buckets {
        assert!((b as f32 / n as f32 - 0.1).abs() < 0.015, "{buckets:?}");
    }
}

// ------------------------------------------------------------------------------------ tiers

#[test]
fn super_saiyan_tiers_escalate_and_the_first_ascend_is_an_awakening() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, 30.0);
    let base = hit(&mut w, B, A, HitKind::Melee, 10.0);
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = eye_aim(&w, A, Vec3::NEG_Z);
    w.cast(A, grants("super saiyan", "super_saiyan"), None, aim).ok();
    let evs = power_events(&run(&mut w, 1));
    assert!(evs.iter().any(|e| matches!(e, PowerEvent::Ascend { player: A, tier: 0, awakening: true, .. })), "{evs:?}");
    let mut mults = vec![hit(&mut w, B, A, HitKind::Melee, 10.0) / base];
    for tier in 1..=4u8 {
        w.power_move(A, move_sel::select(0, tier)).expect("tier select");
        assert_eq!(w.player(A).unwrap().power.tier, tier);
        mults.push(hit(&mut w, B, A, HitKind::Melee, 10.0) / base);
    }
    // SSJ < SSJ2 < SSJ3; God and Blue are their own line (Blue the strongest of all).
    assert!(mults[0] < mults[1] && mults[1] < mults[2], "{mults:?}");
    assert!(mults[4] > mults[2] && mults[4] > mults[3], "{mults:?}");
    // Willpower drain escalates too.
    let drain = |w: &mut World| {
        w.player_mut(A).unwrap().willpower = 90.0;
        w.player_mut(A).unwrap().regen_pause = 100.0;
        run(w, 60);
        90.0 - w.player(A).unwrap().willpower
    };
    w.power_move(A, move_sel::select(0, 1)).unwrap();
    let d2 = drain(&mut w);
    w.power_move(A, move_sel::select(0, 4)).unwrap();
    let d_blue = drain(&mut w);
    assert!(d_blue > d2 * 1.5, "blue drains faster ({d2} → {d_blue})");
    // The second time a tier is taken this round, the Ascend is the short one.
    let evs = power_events(&{
        w.power_move(A, move_sel::select(0, 1)).unwrap();
        run(&mut w, 1)
    });
    assert!(evs.iter().any(|e| matches!(e, PowerEvent::Ascend { player: A, tier: 1, awakening: false, .. })), "{evs:?}");
    // Saying the power again drops back to base.
    assert_eq!(w.parse_power_phrase(A, "super saiyan"), PowerPhrase::Move(move_sel::select(0, 0)));
    w.power_move(A, move_sel::select(0, 0)).unwrap();
    assert_eq!(w.player(A).unwrap().power.tier, 0);
}

#[test]
fn gomu_gears_are_tiers_picked_by_phrase() {
    let mut w = duel(1);
    stand_at(&mut w, A, 0.0, 0.0);
    assert!(w.grant_power(A, "gomu_gomu"));
    for (phrase, tier) in [("gear second", 1u8), ("gear 3", 2), ("gear fourth", 3), ("gear fifth", 4), ("gum gum fruit", 0)] {
        let PowerPhrase::Move(slot) = w.parse_power_phrase(A, phrase) else { panic!("{phrase}: not a power phrase") };
        w.power_move(A, slot).unwrap_or_else(|e| panic!("{phrase}: {e:?}"));
        assert_eq!(w.player(A).unwrap().power.tier, tier, "{phrase}");
    }
    // Gear Second: faster and quicker moves; "gomu gomu no jet <x>" is an incantation.
    w.power_move(A, move_sel::select(0, 1)).unwrap();
    let mut m = MoveModifiers::default();
    w.player(A).unwrap().power.modify_movement(true, &mut m);
    assert!(m.speed_mult > 1.25);
    assert!(matches!(w.parse_power_phrase(A, "gomu gomu no jet hammer"), PowerPhrase::Incantation { rest } if rest == "hammer"));
    assert!(matches!(w.parse_power_phrase(A, "jet pistol"), PowerPhrase::Move(0)));
    // Gear Fifth plays the drum layer (data) and awakens once.
    w.power_move(A, move_sel::select(0, 4)).unwrap();
    let d = PowerTable::get().def(pid("gomu_gomu")).unwrap();
    assert_eq!(d.ascend_at(4).and_then(|a| a.music.as_deref()), Some("drums"));
    assert!(d.ascend_at(4).is_some_and(|a| a.toon));
}

// ------------------------------------------------------------------------------------ mixing

#[test]
fn powers_mix_lend_grafts_and_keep_their_own_cooldowns() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, -12.0);
    assert!(w.grant_power(A, "rinnegan"));
    assert!(w.grant_power(A, "armament_haki"));
    assert!(w.grant_power(A, "gomu_gomu"));
    let p = &w.player(A).unwrap().power;
    assert_eq!(p.count(), 3);
    assert_eq!(p.id, Some(pid("gomu_gomu")), "the newest is active");
    // Every held power lends its grafts: a plain hammer gains gravity pull (Rinnegan) and haki (Heavy).
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = eye_aim(&w, A, Vec3::NEG_Z);
    let cid = w.cast(A, recipe("hammer", Form::Hammer, &[], Element::Light, 1.0), None, aim).expect("hammer");
    let c = w.constructs.get(cid).unwrap();
    assert!(c.recipe.has(Mechanic::Attract) && c.recipe.has(Mechanic::Heavy), "{:?}", c.recipe.mechanics);
    assert_eq!(c.recipe.element, Element::Shadow, "hardened black");
    // A shout of a power that isn't active runs ITS move (cross-power chains)...
    w.player_mut(A).unwrap().willpower = 100.0;
    let PowerPhrase::Move(slot) = w.parse_power_phrase(A, "almighty push") else { panic!() };
    assert!(slot & move_sel::MOVE_OF != 0);
    w.power_move(A, slot).expect("almighty push from the rinnegan while gomu is active");
    assert_eq!(w.player(A).unwrap().power.id, Some(pid("gomu_gomu")), "still gomu active");
    // ...its cooldown stays with the Rinnegan; gomu's own moves are free to chain right after.
    let rin = w.player(A).unwrap().power.others.iter().flatten().find(|o| o.id == pid("rinnegan")).copied().unwrap();
    assert!(rin.move_cd[0] > 6.0, "rinnegan cooldown ticking");
    w.power_move(A, 0).expect("gum-gum pistol chains");
    assert!(w.power_move(A, slot).is_err(), "almighty push is on cooldown");
    // Say the Rinnegan again: it becomes active (no cost, no new grant).
    let wp = w.player(A).unwrap().willpower;
    let aim = eye_aim(&w, A, Vec3::NEG_Z);
    w.cast(A, grants("rinnegan", "rinnegan"), None, aim).ok();
    assert_eq!(w.player(A).unwrap().power.id, Some(pid("rinnegan")));
    assert_eq!(w.player(A).unwrap().willpower, wp, "re-selecting is free");
    assert_eq!(w.player(A).unwrap().power.count(), 3);
    // G double-tap cycles.
    let before = w.player(A).unwrap().power.id;
    w.power_move(A, move_sel::CYCLE).unwrap();
    assert_ne!(w.player(A).unwrap().power.id, before);
    // KO drops everything; the fruit reappears.
    w.player_mut(A).unwrap().mv.pos.y = -100.0;
    w.tick(&[]);
    assert_eq!(w.player(A).unwrap().power.count(), 0);
    assert!(w.powers.fruits.iter().any(|f| f.power == pid("gomu_gomu")));
}

#[test]
fn prediction_mirror_matches_the_host_movement_mods() {
    let mut w = duel(1);
    stand_at(&mut w, A, 0.0, 0.0);
    for key in ["kurama_mode", "gomu_gomu", "ultra_instinct"] {
        assert!(w.grant_power(A, key));
    }
    w.power_move(A, move_sel::select(0, 1)).unwrap(); // mastered ultra instinct
    let gomu = w.player(A).unwrap().power.slot_of(pid("gomu_gomu")).unwrap();
    w.power_move(A, move_sel::select(gomu, 4)).unwrap(); // gear fifth, now active
    let host = w.player(A).unwrap().power.clone();
    let mut mirror = PlayerPower::default();
    mirror.apply_snap(host.snap());
    for grounded in [true, false] {
        let (mut a, mut b) = (MoveModifiers::default(), MoveModifiers::default());
        host.modify_movement(grounded, &mut a);
        mirror.modify_movement(grounded, &mut b);
        assert_eq!(a, b, "grounded {grounded}");
    }
    assert_eq!(mirror.count(), 3);
    assert_eq!(mirror.snap(), host.snap());
}

// ------------------------------------------------------------------------------------ Rinnegan

#[test]
fn almighty_push_flings_everyone_out() {
    let mut w = duel(3);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 3.0, 0.0);
    stand_at(&mut w, PlayerId(3), -2.0, -3.0);
    assert!(w.grant_power(A, "rinnegan"));
    w.player_mut(A).unwrap().willpower = 100.0;
    let y0 = w.player(A).unwrap().mv.pos.y;
    w.power_move(A, 0).expect("almighty push");
    assert!(w.player(A).unwrap().mv.vel.y < 1.0, "the caster is never rocket-jumped by their own push");
    run(&mut w, 30);
    assert!(w.player(A).unwrap().mv.pos.y < y0 + 0.5, "the caster stays put (no launch into the canopy)");
    for id in [B, PlayerId(3)] {
        let d = (w.player(id).unwrap().mv.pos - w.player(A).unwrap().mv.pos).length();
        assert!(d > 7.0, "{id:?} flung away ({d})");
    }
    assert!(w.player(A).unwrap().damage_pct == 0.0, "never hurts the caster");
}

#[test]
fn universal_pull_yanks_the_aimed_enemy() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, -18.0);
    assert!(w.grant_power(A, "rinnegan"));
    w.player_mut(A).unwrap().willpower = 100.0;
    let PowerPhrase::Move(slot) = w.parse_power_phrase(A, "universal pull") else { panic!() };
    assert_eq!(slot, 2, "slot 2 by its shout");
    let from = w.player(B).unwrap().mv.pos;
    let evs = power_events(&{
        w.power_move(A, slot).expect("pull");
        run(&mut w, 1)
    });
    assert!(evs.iter().any(|e| matches!(e, PowerEvent::Yanked { target: Some(B), .. })));
    run(&mut w, 36);
    let to = w.player(B).unwrap().mv.pos;
    assert!((to - Vec3::new(0.0, 8.0, 0.0)).length() < (from - Vec3::new(0.0, 8.0, 0.0)).length() - 6.0, "pulled in: {from} → {to}");
}

#[test]
fn planetary_devastation_captures_and_dodging_breaks_free() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 20.0);
    stand_at(&mut w, B, 0.0, -2.0);
    assert!(w.grant_power(A, "rinnegan"));
    w.player_mut(A).unwrap().willpower = 100.0;
    let evs = {
        w.power_move(A, 1).expect("planetary devastation");
        run(&mut w, 1)
    };
    assert!(evs.iter().any(|e| matches!(e, GameEvent::ConstructSpawned { cost, .. } if *cost >= 45.0)), "announced like a signature");
    assert!(power_events(&evs).iter().any(|e| matches!(e, PowerEvent::Singularity { end: false, .. })));
    // B is dragged up into the mass and held.
    let mut held = false;
    for _ in 0..360 {
        w.tick(&[]);
        let core = w.constructs.iter().find(|c| c.recipe.name == "planetary devastation").map(|c| c.pos);
        let (Some(core), Some(b)) = (core, w.player(B)) else { continue };
        if (b.hull(w.params()).center() - core).length() < 3.5 {
            held = true;
            break;
        }
    }
    assert!(held, "the core drags B in");
    let y = w.player(B).unwrap().mv.pos.y;
    run(&mut w, 30);
    assert!((w.player(B).unwrap().mv.pos.y - y).abs() < 1.0, "held in the mass");
    // A dodge breaks free.
    let p = w.player_mut(B).unwrap();
    p.mv.extra.dodge_left = 0.15;
    p.mv.extra.dodge_dir = Vec3::X;
    run(&mut w, 60);
    assert!(w.player(B).unwrap().mv.pos.y < y - 1.0 || (w.player(B).unwrap().mv.pos - Vec3::new(0.0, y, -2.0)).length() > 3.0, "escaped");
    // It ends (a blast) and cleans up.
    run(&mut w, 800);
    assert!(!w.constructs.iter().any(|c| c.recipe.name == "planetary devastation"));
}

// ------------------------------------------------------------------------------------ AirJumps

/// Gear Fourth: two extra air jumps (the generic `AirJumps` effect), read the same way by the
/// host and by client prediction; they come back on landing and are gone with the form.
#[test]
fn gear_fourth_air_jumps_match_on_host_and_prediction() {
    use bd_core::input::{PlayerInput, buttons};
    let mut host = duel(1);
    stand_at(&mut host, A, 0.0, 0.0);
    assert!(host.grant_power(A, "gomu_gomu"));
    host.power_move(A, move_sel::select(0, 3)).unwrap(); // gear fourth
    let mut m = MoveModifiers::default();
    host.player(A).unwrap().power.modify_movement(false, &mut m);
    assert_eq!(m.extra_air_jumps, 2, "gear fourth: +2 air jumps");
    let mut mirror = PlayerPower::default();
    mirror.apply_snap(host.player(A).unwrap().power.snap());
    let mut mm = MoveModifiers::default();
    mirror.modify_movement(false, &mut mm);
    assert_eq!(mm, m, "prediction mirror reads the same modifiers");
    assert_eq!(powers::effects::extra_air_jumps_snap(&host.player(A).unwrap().power.snap()), 2);
    // Settle on the ground, then a client mirror predicts the local player.
    run(&mut host, 60);
    let mut client = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() });
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    // Hop, then mash Jump in the air: 1 + 2 air jumps, the fourth press does nothing.
    let presses = [10u32, 40, 70, 100, 130];
    let (mut air, mut bad) = (0, 0);
    for k in 0..480u32 {
        let i = PlayerInput { buttons: if presses.contains(&k) { buttons::JUMP } else { 0 }, ..Default::default() };
        let mut s = client.player(A).unwrap().mv;
        let ev = client.predict_player(A, &mut s, &i);
        let evs = host.tick(&[(A, i)]).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let h = host.player(A).unwrap();
        assert_eq!(h.move_events.air_jumped, ev.air_jumped, "tick {k}: air jump event");
        air += h.move_events.air_jumped as u32;
        if (h.mv.pos - s.pos).length() > 1e-4 || (h.mv.vel - s.vel).length() > 1e-3 || h.mv.extra != s.extra {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "mispredicted ticks");
    assert_eq!(air, 3, "one air jump + two extra");
    // Landed: restored.
    let x = host.player(A).unwrap().mv.extra;
    assert!(host.player(A).unwrap().mv.grounded && !x.air_jump_used && x.extra_air_jumps_used == 0);
    // Back to base: no extras.
    host.power_move(A, move_sel::select(0, 0)).unwrap();
    let mut m = MoveModifiers::default();
    host.player(A).unwrap().power.modify_movement(false, &mut m);
    assert_eq!(m.extra_air_jumps, 0);
}

/// Worn air moves own Jump in the air: a spent air jump never leaves the extras live on the same press.
#[test]
fn extra_air_jumps_follow_the_air_jump_rules() {
    use bd_sim::movement::ExtraMoves;
    let mut x = ExtraMoves::default();
    assert!(x.air_jump_left(0));
    x.air_jump_used = true;
    assert!(!x.air_jump_left(0));
    assert!(x.air_jump_left(2));
    x.extra_air_jumps_used = 2;
    assert!(!x.air_jump_left(2));
    x.restore();
    assert!(x.air_jump_left(0) && x.extra_air_jumps_used == 0);
    x.spend_air_jumps();
    assert!(!x.air_jump_left(4));
}

