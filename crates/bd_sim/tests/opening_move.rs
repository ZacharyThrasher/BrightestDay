//! OPENING MOVE (`Recipe::opening`, from the lexicon's "<name> <move>": "reinhardt charge"): the
//! creature performs its named move once right after it forms — along the cast aim when nobody is in
//! range, at the target when someone is — then carries on with its normal AI.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::constructs::ai::AttackKind;
use bd_sim::events::GameEvent;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn world() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
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

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
}

fn reinhardt(opening: Option<Mechanic>) -> Recipe {
    let mut r = recipe("reinhardt", Form::Biped, &[Mechanic::Guard, Mechanic::Charge], Element::Light, 1.8);
    r.opening = opening;
    r
}

/// Cast `r` from A looking along `dir`; run `ticks`; return (id, charge attacks started, events).
fn cast_and_run(w: &mut World, r: Recipe, dir: Vec3, ticks: u32) -> (ConstructId, Vec<Vec3>, Vec<GameEvent>) {
    let yaw = (-dir.x).atan2(-dir.z);
    w.player_mut(A).unwrap().input.yaw = yaw;
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, r, None, Aim::new(eye, (dir + Vec3::new(0.0, -0.3, 0.0)).normalize())).unwrap();
    let inp = bd_core::input::PlayerInput { yaw, ..Default::default() };
    let mut ev = Vec::new();
    for _ in 0..ticks {
        ev.extend_from_slice(w.tick(&[(A, inp)]));
    }
    let charges = ev
        .iter()
        .filter_map(|e| match e {
            GameEvent::CreatureAttack { id: c, kind, phase: 0, aim, .. } if *c == id && *kind == AttackKind::Charge as u8 => Some(*aim),
            _ => None,
        })
        .collect();
    (id, charges, ev)
}

#[test]
fn reinhardt_charge_charges_along_the_aim_once_on_spawn() {
    let mut w = world();
    place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(40.0, GROUND, -40.0)); // far out of range
    w.tick(&[]);
    let dir = Vec3::new(0.0, 0.0, -1.0);
    let (id, charges, _) = cast_and_run(&mut w, reinhardt(Some(Mechanic::Charge)), dir, 240);
    assert_eq!(charges.len(), 1, "exactly one opening charge (no target in range for a normal one)");
    assert!(charges[0].dot(dir) > 0.9, "along the cast aim: {:?}", charges[0]);
    let c = w.constructs.get(id).unwrap();
    // It dashed forward along −Z from where it formed (in front of A).
    assert!(c.pos.z < 30.0 - 8.0, "the dash carried it along the aim: z {}", c.pos.z);

    // The plain name: no opening move, no charge with nobody in range.
    let mut w = world();
    place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(40.0, GROUND, -40.0));
    w.tick(&[]);
    let (_, charges, _) = cast_and_run(&mut w, reinhardt(None), dir, 240);
    assert!(charges.is_empty(), "no opening move without a named move");
}

#[test]
fn opening_charge_aims_at_a_target_in_range() {
    let mut w = world();
    place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
    // B off to the side (+X), within range of where it forms.
    place(&mut w, B, Vec3::new(-18.0, GROUND, 22.0));
    w.tick(&[]);
    let dir = Vec3::new(0.0, 0.0, -1.0);
    let (_, charges, _) = cast_and_run(&mut w, reinhardt(Some(Mechanic::Charge)), dir, 150);
    assert!(!charges.is_empty(), "opening charge");
    assert!(charges[0].x > 0.5, "aimed at B (+X), not along the cast aim: {:?}", charges[0]);
}

#[test]
fn opening_move_is_deterministic() {
    let once = || {
        let mut w = world();
        place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
        place(&mut w, B, Vec3::new(40.0, GROUND, -40.0));
        w.tick(&[]);
        cast_and_run(&mut w, reinhardt(Some(Mechanic::Charge)), Vec3::NEG_Z, 200);
        w.state_hash()
    };
    assert_eq!(once(), once());
}
