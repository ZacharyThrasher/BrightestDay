//! The Lookout in the sim (feature `lookout`): off the rim is a bottom (void) KO and far out is a side
//! KO through the normal blast-zone rules — no map-specific sim code; Korin's roof catches a player
//! dropped onto it; eight bots brawl on the disc with invariants on, nobody ever embedded in terrain
//! and every spawn standing clear.
#![cfg(feature = "lookout")]

use bd_core::corps::Corps;
use bd_core::voxel::VoxelQuery;
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::bots;
use bd_sim::events::GameEvent;
use bd_sim::world::{SimConfig, World};
use bd_voxel::maps::lookout::{self, generate_lookout};

fn lookout_world(seed: u64) -> World {
    let (v, info) = generate_lookout(seed);
    World::with_arena(SimConfig { check_invariants: true, ..SimConfig::default() }, v, info.arena.clone(), seed)
}

/// Drop player `a` at `pos` with `vel` and tick until a KO (or `secs`): `(seconds, void?)`.
fn drop_until_ko(w: &mut World, a: PlayerId, pos: Vec3, vel: Vec3, secs: f32) -> Option<(f32, bool)> {
    {
        let p = w.player_mut(a).unwrap();
        p.spawn_shield = 0.0;
        p.mv.pos = pos;
        p.mv.vel = vel;
        p.mv.grounded = false;
    }
    for t in 0..(secs / DT) as usize {
        let ev = w.tick(&[]).to_vec();
        if let Some(GameEvent::Ko { victim, void, .. }) = ev.iter().find(|e| matches!(e, GameEvent::Ko { .. }))
            && *victim == a
        {
            return Some((t as f32 * DT, *void));
        }
    }
    None
}

#[test]
fn off_the_rim_is_a_void_ko_and_far_out_a_side_ko() {
    let mut w = lookout_world(1);
    let a = PlayerId(1);
    w.add_player(a, "diver".into(), Corps::from_index(1)).unwrap();
    w.start_live();
    w.tick(&[]);
    // Walked off the west rim: falls past the underside to the KO line.
    let (t, void) = drop_until_ko(&mut w, a, Vec3::new(-lookout::DISC_R - 2.0, lookout::FLOOR_Y + 0.5, 0.0), Vec3::new(-3.0, 0.0, 0.0), 6.0)
        .expect("no KO after walking off the rim");
    eprintln!("rim fall KO after {t:.2} s");
    assert!(void, "falling off the disc is a bottom (void) KO");
    assert!(t > 1.2 && t < 3.5, "fall time {t:.2} s (time to recover, not forever)");
    // Launched far past the side of the world: a side KO.
    w.tick(&[]);
    for _ in 0..(5.0 / DT) as usize {
        if w.player(a).is_some_and(|p| p.alive) {
            break;
        }
        w.tick(&[]);
    }
    let bx = w.arena.bounds.max.x;
    let (_, void) = drop_until_ko(&mut w, a, Vec3::new(bx - 1.0, lookout::FLOOR_Y + 10.0, 0.0), Vec3::new(40.0, 0.0, 0.0), 3.0)
        .expect("no KO flying out of the side");
    assert!(!void, "flying out of the side is a side KO, not a void KO");
}

#[test]
fn korin_roof_catches_a_fall() {
    let mut w = lookout_world(1);
    let a = PlayerId(1);
    w.add_player(a, "faller".into(), Corps::from_index(2)).unwrap();
    w.start_live();
    w.tick(&[]);
    {
        let p = w.player_mut(a).unwrap();
        p.spawn_shield = 0.0;
        p.mv.pos = Vec3::new(lookout::KORIN.x + 1.0, lookout::FLOOR_Y + 1.0, lookout::KORIN.y + 2.0);
        p.mv.vel = Vec3::ZERO;
        p.mv.grounded = false;
    }
    for _ in 0..(2.0 / DT) as usize {
        let ev = w.tick(&[]).to_vec();
        assert!(!ev.iter().any(|e| matches!(e, GameEvent::Ko { .. })), "KO'd instead of landing on Korin's roof");
    }
    let p = w.player(a).unwrap();
    assert!(p.mv.grounded, "not standing after the drop ({})", p.mv.pos);
    assert!((p.mv.pos.y - lookout::KORIN_TOP).abs() < 0.6, "landed at {} (roof {})", p.mv.pos, lookout::KORIN_TOP);
}

#[test]
fn eight_bots_brawl_on_the_lookout() {
    let mut w = lookout_world(2);
    let mut brains = bots::add_bots(&mut w, 1, 8, 0.8, 11);
    w.start_live();
    let mut inputs = Vec::with_capacity(8);
    let (mut kos, mut void) = (0, 0);
    let mut embedded = 0usize;
    for t in 0..(40.0 / DT) as usize {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        for e in w.tick(&inputs) {
            if let GameEvent::Ko { void: v, .. } = e {
                kos += 1;
                void += *v as u32;
            }
        }
        if t % 12 == 0 {
            for p in w.players.iter().filter(|p| p.alive) {
                let chest = p.mv.pos + Vec3::Y * 0.9;
                if w.voxels.is_solid(bd_core::voxel::world_to_voxel(chest)) {
                    embedded += 1;
                    eprintln!("player {:?} embedded at {}", p.id, p.mv.pos);
                }
            }
        }
    }
    eprintln!("40 s, 8 bots on the Lookout: {kos} KOs ({void} off the edge)");
    assert_eq!(embedded, 0, "players embedded in terrain");
    let alive = w.players.iter().filter(|p| p.alive).count();
    assert!(alive >= 1, "everyone dead at the end");
    for s in &w.arena.spawns {
        let v = bd_core::voxel::world_to_voxel(*s + Vec3::Y * 0.9);
        assert!(!w.voxels.is_solid(v), "spawn {s} inside terrain");
    }
}
