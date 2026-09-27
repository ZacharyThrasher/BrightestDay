//! MODERN ANIME (docs/toolbox/BRIEF.md): behaviour of the generic ops this pack added —
//! `Transpose` (Boogie Woogie), `Finisher` (Black Flash), `Erupt` (glaciers, alchemy), `Tag` /
//! `TagStrike` (Resonance, Bungee Gum) — fired directly with `World::fire_op`, plus golden phrase
//! readings of the pack's kits / techniques / shouts through the TOOLBOX move index.

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::powers::{MoveOp, PowerEvent, PowerTable};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);

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

/// Stand `id` on the flat lab's ground (top at y = 8) at (x, z), looking down -Z.
fn stand_at(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, 8.0, z);
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = true;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
}

fn pos(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().mv.pos
}

fn pct(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().damage_pct
}

fn power_events(w: &World) -> Vec<PowerEvent> {
    w.events().iter().filter_map(|e| if let GameEvent::Power(p) = e { Some(p.clone()) } else { None }).collect()
}

// ------------------------------------------------------------------------------------ Transpose

#[test]
fn transpose_swaps_you_with_the_aimed_player() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    let (a0, b0) = (pos(&w, A), pos(&w, B));
    w.fire_op(A, &MoveOp::Transpose { range: 30.0, cone: 8.0, others: false }).expect("clap");
    assert!((pos(&w, A) - b0).length() < 0.01, "A is where B stood");
    assert!((pos(&w, B) - a0).length() < 0.01, "B is where A stood");
    assert!(power_events(&w).iter().any(|e| matches!(e, PowerEvent::Swapped { a, b_player: Some(b), .. } if *a == A && *b == B)));
    // The clap is a connect: B's last attacker is A (opens a Finisher window).
    assert_eq!(w.player(B).unwrap().last_attacker.map(|x| x.0), Some(A));
}

#[test]
fn transpose_with_nothing_aimed_fails_and_others_swaps_two_enemies() {
    let mut w = duel(3);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    stand_at(&mut w, C, 20.0, 10.0);
    // Look away from everyone: nothing to swap with.
    w.player_mut(A).unwrap().input.yaw = std::f32::consts::PI;
    assert!(w.fire_op(A, &MoveOp::Transpose { range: 30.0, cone: 0.0, others: false }).is_err());
    // Boogie Woogie shuffle: the enemy you last hit (C) trades places with the aimed one (B).
    w.player_mut(A).unwrap().input.yaw = 0.0;
    w.player_mut(A).unwrap().last_target = Some(C);
    let (a0, b0, c0) = (pos(&w, A), pos(&w, B), pos(&w, C));
    w.fire_op(A, &MoveOp::Transpose { range: 30.0, cone: 8.0, others: true }).expect("shuffle");
    assert!((pos(&w, A) - a0).length() < 0.01, "you stay put");
    assert!((pos(&w, B) - c0).length() < 0.01 && (pos(&w, C) - b0).length() < 0.01, "B and C traded places");
}

// ------------------------------------------------------------------------------------ Finisher

#[test]
fn finisher_crits_only_inside_the_combo_window() {
    let black_flash = MoveOp::Finisher {
        lunge: 0.0,
        range: 4.0,
        angle: 50.0,
        damage: 10.0,
        knockback: 4.0,
        element: Default::default(),
        up: 0.0,
        window: 0.8,
        mult: 2.5,
        fx: bd_sim::powers::CritFx::BlackFlash,
    };
    // Cold: a plain hit, no crit.
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 2.5);
    stand_at(&mut w, B, 0.0, 0.0);
    let before = pct(&w, B);
    w.fire_op(A, &black_flash).unwrap();
    let cold = pct(&w, B) - before;
    assert!(cold > 0.0, "it connects");
    assert!(!power_events(&w).iter().any(|e| matches!(e, PowerEvent::Crit { .. })), "cold open never crits");
    // Right after a connect (inside the window): the Black Flash.
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 2.5);
    stand_at(&mut w, B, 0.0, 0.0);
    let t = w.tick;
    w.player_mut(B).unwrap().last_attacker = Some((A, t));
    let before = pct(&w, B);
    w.fire_op(A, &black_flash).unwrap();
    let hot = pct(&w, B) - before;
    assert!(power_events(&w).iter().any(|e| matches!(e, PowerEvent::Crit { attacker, target, .. } if *attacker == A && *target == B)));
    assert!(hot > cold * 2.0, "the crit hits much harder ({hot} vs {cold})");
    // Stale connect (outside the window): plain again.
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 2.5);
    stand_at(&mut w, B, 0.0, 0.0);
    let t = w.tick;
    w.player_mut(B).unwrap().last_attacker = Some((A, t.saturating_sub(bd_core::TICK_HZ as u64 * 2)));
    w.fire_op(A, &black_flash).unwrap();
    assert!(!power_events(&w).iter().any(|e| matches!(e, PowerEvent::Crit { .. })));
}

// ------------------------------------------------------------------------------------ Erupt

#[test]
fn erupt_raises_terrain_along_the_aim_and_launches_whoever_stands_on_it() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 3.0);
    let h0 = w.voxels.content_hash();
    let glacier = MoveOp::Erupt { length: 14.0, width: 2.0, height: 3.0, count: 0, element: bd_core::mechanics::Element::Ice, damage: 6.0, launch: 14.0, across: false, dist: 0.0 };
    w.fire_op(A, &glacier).expect("erupts");
    let voxels = power_events(&w).iter().find_map(|e| if let PowerEvent::Restored { voxels, .. } = e { Some(voxels.clone()) } else { None }).expect("replicated");
    assert!(voxels.len() > 40, "a ridge of ice rose ({} voxels)", voxels.len());
    assert!(voxels.iter().all(|(_, m)| *m == bd_voxel::mat::ICE), "ice is ice");
    assert_ne!(w.voxels.content_hash(), h0);
    // Never inside a player: no new voxel overlaps B.
    let hull = w.player(B).unwrap().hull(w.params());
    assert!(voxels.iter().all(|(v, _)| {
        let c = bd_core::voxel::voxel_center(*v);
        !hull.intersects(&bd_core::voxel::Aabb { min: c - Vec3::splat(0.25), max: c + Vec3::splat(0.25) })
    }));
    // B, standing on the line, was hit and launched.
    assert!(pct(&w, B) > 0.0);
    // A replica applying the event list gets the same terrain.
    let mut r = duel(2);
    bd_sim::powers::apply_restore(&mut r.voxels, &voxels);
    assert_eq!(r.voxels.content_hash(), w.voxels.content_hash(), "replicas match");
}

// ------------------------------------------------------------------------------------ Tag / TagStrike

#[test]
fn tag_then_strike_hits_every_tagged_enemy_from_anywhere_and_consumes_the_tags() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    let resonance = MoveOp::TagStrike { damage: 12.0, knockback: 6.0, up: 0.2, element: Default::default(), pull: 0.0 };
    assert!(w.fire_op(A, &resonance).is_err(), "no tag, nothing to resonate");
    w.fire_op(A, &MoveOp::Tag { range: 30.0, cone: 8.0, secs: 8.0 }).expect("nail");
    // Turn around and walk away: the tag doesn't care.
    w.player_mut(A).unwrap().input.yaw = std::f32::consts::PI;
    stand_at(&mut w, A, 30.0, 30.0);
    w.player_mut(A).unwrap().input.yaw = std::f32::consts::PI;
    let before = pct(&w, B);
    w.fire_op(A, &resonance).expect("resonance");
    assert!(pct(&w, B) > before + 5.0, "the doll hurts them from across the map");
    assert!(w.fire_op(A, &resonance).is_err(), "the strike consumed the tag");
}

#[test]
fn bungee_gum_snaps_the_tagged_enemy_back_to_you() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 20.0);
    stand_at(&mut w, B, 0.0, 0.0);
    w.fire_op(A, &MoveOp::Tag { range: 40.0, cone: 8.0, secs: 8.0 }).expect("stick");
    w.fire_op(A, &MoveOp::TagStrike { damage: 0.0, knockback: 0.0, up: 0.0, element: Default::default(), pull: 26.0 }).expect("snap");
    let v = w.player(B).unwrap().mv.vel;
    assert!(v.z > 15.0, "B flies toward A (+Z), got {v:?}");
    // Tags expire.
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 20.0);
    stand_at(&mut w, B, 0.0, 0.0);
    w.fire_op(A, &MoveOp::Tag { range: 40.0, cone: 8.0, secs: 0.5 }).unwrap();
    for _ in 0..(bd_core::TICK_HZ) {
        w.tick(&[]);
    }
    assert!(w.fire_op(A, &MoveOp::TagStrike { damage: 5.0, knockback: 0.0, up: 0.0, element: Default::default(), pull: 0.0 }).is_err());
}

// ------------------------------------------------------------------------------------ data

#[test]
fn the_anime_pack_parses_and_every_move_is_well_formed() {
    let files: Vec<(&str, &str)> = bd_sim::powers::SHIPPED_FILES.iter().copied().filter(|(n, _)| *n == "anime_modern.ron").collect();
    assert_eq!(files.len(), 1, "assets/powers/anime_modern.ron is shipped");
    let t = PowerTable::parse_files(&files).expect("anime_modern.ron parses");
    assert!(t.powers.len() >= 40, "{} entries", t.powers.len());
    for p in &t.powers {
        assert!(!p.moves.is_empty(), "{} has moves", p.id);
        assert!(p.shouts.iter().all(|(_, s)| (*s as usize) < p.moves.len()), "{}: shout slot", p.id);
        for td in &p.tiers {
            let n = if td.moves.is_empty() { p.moves.len() } else { td.moves.len() };
            assert!(td.shouts.iter().all(|(_, s)| (*s as usize) < n), "{} / {}: tier shout slot", p.id, td.name);
        }
    }
}
