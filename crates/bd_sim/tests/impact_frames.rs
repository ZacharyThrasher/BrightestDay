//! IMPACT FRAMES (docs/impact/API.md): `MoveDef::crit` — a move whose every hit is a guaranteed
//! crit (Black Flash's shout always sparks black), deterministic, and nothing changes for moves
//! without it.

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::powers::{CritFx, PowerEvent, PowerTable};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn duel() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
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

fn stage(w: &mut World) {
    // B stands 2.2 m in front of A (A looks along −Z at yaw 0).
    for (id, z) in [(A, 0.0), (B, -2.2)] {
        let p = w.player_mut(id).unwrap();
        p.mv.pos = Vec3::new(0.0, 8.0, z);
        p.mv.vel = Vec3::ZERO;
        p.input.yaw = 0.0;
        p.input.pitch = 0.0;
        p.juggle = Default::default();
        p.damage_pct = 0.0;
        p.willpower = 100.0;
    }
    for _ in 0..30 {
        w.tick(&[]);
    }
    for (id, z) in [(A, 0.0), (B, -2.2)] {
        let p = w.player_mut(id).unwrap();
        p.mv.pos.x = 0.0;
        p.mv.pos.z = z;
        p.mv.vel = Vec3::ZERO;
        p.power.move_cd = Default::default();
    }
}

fn slot_of(key: &str, name: &str) -> u8 {
    let d = PowerTable::get().def(PowerTable::get().index_of(key).unwrap()).unwrap();
    d.moves.iter().position(|m| m.name == name).unwrap_or_else(|| panic!("{key}: no move {name}")) as u8
}

/// Fire a move at B; returns (crit events, damage dealt).
fn fire(w: &mut World, slot: u8) -> (Vec<(CritFx, f32)>, f32) {
    stage(w);
    let before = w.player(B).unwrap().damage_pct;
    let n0 = w.events().len();
    w.power_move(A, slot).expect("the move fires");
    let crits = w.events()[n0..]
        .iter()
        .filter_map(|e| match e {
            GameEvent::Power(PowerEvent::Crit { attacker: A, target: B, fx, mult, .. }) => Some((*fx, *mult)),
            _ => None,
        })
        .collect();
    (crits, w.player(B).unwrap().damage_pct - before)
}

#[test]
fn the_black_flash_shout_always_sparks_black() {
    let table = PowerTable::get();
    let def = table.def(table.index_of("black_flash_technique").unwrap()).unwrap();
    let shout = def.moves.iter().find(|m| m.name == "black flash").unwrap();
    assert_eq!(shout.crit, Some((2.5, CritFx::BlackFlash)), "a data flag, not a name check");
    assert_eq!(shout.juice.as_ref().and_then(|j| j.impact_frame.as_deref()), Some("black_flash"));
    // The bare shout fires this move for anyone (a one-off borrow; the technique grants from "kokusen" /
    // "black flash technique").
    let ix = bd_sim::powers::toolbox::MoveIndex::shipped();
    let m = ix.read(table, None, "black flash").move_ref().expect("\"black flash\" is a move");
    assert_eq!(m.resolve(table).map(|(p, mv)| (p.id.as_str(), mv.name.as_str())), Some(("black_flash_technique", "black flash")));
    let slot = slot_of("black_flash_technique", "black flash");
    let mut w = duel();
    assert!(w.grant_power(A, "black_flash_technique"));
    for k in 0..12 {
        let (crits, dealt) = fire(&mut w, slot);
        assert_eq!(crits.len(), 1, "shout {k}: exactly one Black Flash crit ({crits:?})");
        assert_eq!(crits[0].0, CritFx::BlackFlash);
        assert!(dealt > 7.5 * 2.0, "shout {k}: ×2.5 damage ({dealt})");
    }
}

#[test]
fn moves_without_the_flag_still_roll() {
    let slot = slot_of("black_flash_technique", "divergent fist");
    let mut w = duel();
    assert!(w.grant_power(A, "black_flash_technique"));
    let mut crits = 0;
    let n = 60;
    for _ in 0..n {
        let (c, _) = fire(&mut w, slot);
        crits += c.len();
    }
    assert!(crits > 0 && crits < n, "the 22 % CritChance roll still decides divergent fist ({crits}/{n})");
}

#[test]
fn guaranteed_crits_are_deterministic() {
    let slot = slot_of("black_flash_technique", "black flash");
    let run = || {
        let mut w = duel();
        w.grant_power(A, "black_flash_technique");
        let mut out = Vec::new();
        for _ in 0..4 {
            let (_, d) = fire(&mut w, slot);
            out.push((d * 1000.0).round() as i64);
        }
        out
    };
    assert_eq!(run(), run());
}
