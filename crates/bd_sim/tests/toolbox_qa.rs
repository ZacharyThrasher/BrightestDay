//! TOOLBOX QA: shout hygiene (a borrowed shout must be DISTINCTIVE), chain splitting that keeps a
//! known "and" phrase whole, and the OPS_REVIEW control-safety blockers (Contagion never stun-locks,
//! BlinkCarry is dodgeable and never drops anyone into the void, Transpose never rings anyone out,
//! non-finite op data never reaches the world).

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::{PlayerId, Vec3, TICK_HZ};
use bd_sim::powers::toolbox::{split_chain, split_chain_with, LexJudge, MoveIndex, Reading};
use bd_sim::powers::{MoveOp, PowerTable};
use bd_sim::status::StatusKind;
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);

// ------------------------------------------------------------------------------ shout hygiene

const TABLE: &str = r#"[
    (id: "kit_hammerer", name: "Hammerer", kind: Kit, names: ["hammerer", "zelda"],
     shouts: [("flaming hammer", 0), ("Spirit Bomb", 1), ("zapzap", 2), ("dash", 3), ("hammerer hammer", 0),
              ("mjolnir", 0), ("Rasengan", 1), ("huff and puff", 2)],
     moves: [
        (name: "hammer", tags: [Primary], ops: [Shove(range: 3.0, angle: 60.0, damage: 5.0, knockback: 5.0)]),
        (name: "bomb", ops: [Explode(radius: 3.0, damage: 5.0, knockback: 5.0)]),
        (name: "zap", ops: [Blink(dist: 5.0)]),
        (name: "dash", tags: [Movement], ops: [Impulse(fwd: 10.0, up: 0.0)]),
        (name: "parry", tags: [Defense], ops: [SelfStatus(status: Float, secs: 0.5)]),
     ]),
]"#;

/// A pretend lexicon: what it reads as an ordinary thing / a curated icon / an everyday word.
fn with_judge<R>(f: impl FnOnce(&LexJudge) -> R) -> R {
    let thing = |s: &str| matches!(s, "flaming hammer" | "spirit bomb" | "huff and puff");
    let icon = |s: &str| matches!(s, "zelda shield" | "mjolnir" | "rasengan");
    let everyday = |w: &str| matches!(w, "flaming" | "hammer" | "spirit" | "bomb" | "dash" | "zelda" | "shield" | "huff" | "and" | "puff" | "parry");
    f(&LexJudge { thing: &thing, icon: &icon, everyday: &everyday })
}

#[test]
fn a_shout_must_be_distinctive_to_be_global() {
    let t = PowerTable::parse(TABLE).expect("table");
    let ix = MoveIndex::build(&t);
    with_judge(|j| {
        let read = |s: &str| ix.read_judged(&t, None, s, j);
        // an ordinary thing the lexicon reads: the construct wins
        assert_eq!(read("flaming hammer"), Reading::Plain, "flaming hammer builds a flaming hammer");
        // a curated icon's exact name: the icon wins over an unmarked shout ("mjolnir" is a thing)
        assert_eq!(read("mjolnir"), Reading::Plain);
        // ONE everyday word is never a global shout ("dash" is the described-move grammar's)
        assert_eq!(read("dash"), Reading::Plain);
        // a LOOSE kit reading ("zelda" + slot word "shield") yields to an icon's exact name ...
        assert_eq!(read("zelda shield"), Reading::Plain);
        // ... but the character's own move word still fires it
        assert!(matches!(read("zelda parry"), Reading::Kit(_)));
        // Title Case in the data = a famous PROPER name: stays a move (even as an icon / a thing)
        assert!(matches!(read("spirit bomb"), Reading::Borrow(m) if m.slot == 1));
        assert!(matches!(read("rasengan"), Reading::Borrow(m) if m.slot == 1));
        // a coined word: proper, stays a move
        assert!(matches!(read("zapzap"), Reading::Borrow(m) if m.slot == 2));
        // carrying the character's name: distinctive
        assert!(matches!(read("hammerer hammer"), Reading::Kit(_) | Reading::Borrow(_)));
        // the hygiene predicates the validator uses
        assert!(ix.shout_yields("flaming hammer", j) && !ix.shout_yields("zapzap", j) && !ix.shout_yields("spirit bomb", j));
        assert!(ix.is_marked_shout("spirit bomb", j.everyday) && ix.is_marked_shout("hammerer hammer", j.everyday));
        assert!(!ix.is_marked_shout("flaming hammer", j.everyday) && ix.is_proper_shout("zapzap", j.everyday));
    });
    // the raw reading (no lexicon) is unchanged: every shout still reads
    assert!(matches!(ix.read(&t, None, "flaming hammer"), Reading::Borrow(_)));
}

#[test]
fn a_known_phrase_with_and_inside_is_not_cut_apart() {
    let t = PowerTable::parse(TABLE).expect("table");
    let ix = MoveIndex::build(&t);
    let is_move = |s: &str| ix.read(&t, None, s).is_move();
    // (the lead's case: "fish up the islands, huff and puff" was three steps)
    let understood = |s: &str| s == "summon a dragon";
    assert_eq!(split_chain_with("summon a dragon, huff and puff", &is_move, Some(&understood)), vec!["summon a dragon", "huff and puff"]);
    assert_eq!(split_chain("zapzap and huff and puff", &is_move), vec!["zapzap", "huff and puff"]);
    // a spoken comma still separates (only WORD separators are protected)
    assert_eq!(split_chain("zapzap, zapzap", &is_move), vec!["zapzap", "zapzap"]);
}

// -------------------------------------------------------------------------- control safety

fn arena(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=n {
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

/// Stand `id` on the flat lab's ground (top at y = 8) at (x, z), looking down -Z.
fn stand_at(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, 8.0, z);
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = true;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
}

/// Dig a hole through the whole floor (a patch of VOID) over x0..x1, z0..z1.
fn pit(w: &mut World, x0: f32, z0: f32, x1: f32, z1: f32) {
    let lo = bd_core::voxel::world_to_voxel(Vec3::new(x0, -8.0, z0));
    let hi = bd_core::voxel::world_to_voxel(Vec3::new(x1, 12.0, z1));
    w.voxels.fill_box(lo, hi, 0);
}

fn ticks(secs: f32) -> u32 {
    (secs * TICK_HZ as f32).ceil() as u32
}

fn pos(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().mv.pos
}

#[test]
fn a_stun_brand_never_holds_anyone_past_the_cap() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 12.0);
    stand_at(&mut w, B, 0.0, 0.0);
    w.fire_op(A, &MoveOp::Contagion { range: 30.0, cone: 6.0, status: StatusKind::Stun, secs: 10.0, spread: 0.0 }).expect("brand");
    let mut stunned = 0u32;
    for _ in 0..ticks(10.5) {
        w.tick(&[]);
        stunned += w.player(B).unwrap().status.stunned() as u32;
    }
    let secs = stunned as f32 / TICK_HZ as f32;
    // (contagion::CONTROL_CAP = 1.2 s)
    assert!(secs <= 1.25, "stunned {secs} s by one brand");
    assert!(secs > 0.3, "the stun did land: {secs} s");
}

#[test]
fn a_brand_never_passes_back_and_forth_forever() {
    let mut w = arena(3);
    stand_at(&mut w, A, 0.0, 12.0);
    stand_at(&mut w, B, 0.0, 0.0);
    stand_at(&mut w, C, 1.2, 0.0);
    w.fire_op(A, &MoveOp::Contagion { range: 30.0, cone: 6.0, status: StatusKind::Burn, secs: 3.0, spread: 3.0 }).expect("brand");
    for _ in 0..ticks(0.6) {
        w.tick(&[]);
    }
    assert!(w.player(C).unwrap().power.live.brand.is_some(), "it spreads to C standing close");
    // B and C keep standing together: it must burn out and stay out
    for _ in 0..ticks(4.0) {
        w.tick(&[]);
    }
    for _ in 0..ticks(6.0) {
        w.tick(&[]);
        assert!(w.player(B).unwrap().power.live.brand.is_none() && w.player(C).unwrap().power.live.brand.is_none(), "it came back");
    }
}

#[test]
fn blink_carry_never_drops_anyone_into_the_void() {
    let op = MoveOp::BlinkCarry { dist: 6.0, up: 0.0, grab: 2.0 };
    // over ground: the passenger comes along
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 1.0, 0.0);
    w.fire_op(A, &op).expect("carry");
    assert!(pos(&w, B).distance(pos(&w, A)) < 3.0, "B was carried beside A");
    // the landing is a void: A blinks alone, B stays where they were
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 1.0, 0.0);
    pit(&mut w, -5.0, -10.0, 5.0, -2.5);
    let before = pos(&w, B);
    let _ = w.fire_op(A, &op);
    assert!(pos(&w, B).distance(before) < 0.01, "never dropped over the void: {:?}", pos(&w, B));
}

#[test]
fn blink_carry_is_dodged_first() {
    let op = MoveOp::BlinkCarry { dist: 6.0, up: 0.0, grab: 2.0 };
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 1.0, 0.0);
    let dodge = PlayerInput { buttons: buttons::DODGE, ..w.player(B).unwrap().input };
    w.tick(&[(B, dodge)]);
    assert!(w.player(B).unwrap().mv.extra.dodging(), "B is mid-dodge");
    let before = pos(&w, B);
    let _ = w.fire_op(A, &op);
    assert!(pos(&w, B).distance(before) < 0.01, "a dodge means no carry");
}

#[test]
fn transpose_never_swaps_anyone_over_the_void() {
    let op = MoveOp::Transpose { range: 30.0, cone: 6.0, others: false };
    // over ground: a swap
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 12.0);
    stand_at(&mut w, B, 0.0, 0.0);
    w.fire_op(A, &op).expect("swap");
    assert!(pos(&w, B).distance(Vec3::new(0.0, 8.0, 12.0)) < 0.5, "B took A's spot");
    // A hangs over a pit: B would be put over the void — refused, nothing moves
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 12.0);
    stand_at(&mut w, B, 0.0, 0.0);
    pit(&mut w, -3.0, 9.0, 3.0, 15.0);
    let (a0, b0) = (pos(&w, A), pos(&w, B));
    assert!(w.fire_op(A, &op).is_err(), "refused");
    assert!(pos(&w, A).distance(a0) < 0.01 && pos(&w, B).distance(b0) < 0.01);
}

// ------------------------------------------------------------------ chains re-aim in 3P / price

fn fired_dirs(evs: &[bd_sim::events::GameEvent]) -> Vec<Vec3> {
    evs.iter()
        .filter_map(|e| match e {
            bd_sim::events::GameEvent::Power(bd_sim::powers::PowerEvent::Fired { dir, .. }) => Some(*dir),
            _ => None,
        })
        .collect()
}

#[test]
fn later_chain_steps_keep_the_third_person_crosshair() {
    use bd_sim::powers::chain::ToolStep;
    use bd_sim::powers::norm_words;
    let ix = MoveIndex::shipped();
    let step = |s: &str| ToolStep::Move { mv: ix.shout(&norm_words(s)).unwrap_or_else(|| panic!("{s}")), kit: false };
    for (skew, want) in [(0.0f32, 0.0f32), (8.0, 8.0)] {
        let mut w = arena(1);
        w.cfg.rules.allow_third_person = true;
        stand_at(&mut w, A, 0.0, 0.0);
        let p = w.player(A).unwrap();
        let (eye, look) = (p.eye(&w.cfg.move_params), p.look());
        let right = look.cross(Vec3::Y).normalize();
        // an over-the-shoulder camera ray, aimed `skew`° right of the look
        let dir = (look * skew.to_radians().cos() + right * skew.to_radians().sin()).normalize();
        let aim = bd_sim::world::Aim::new(eye + right * 0.8 + Vec3::Y * 0.3, dir);
        w.toolbox(A, vec![step("bamf"), step("almighty push")], Some(aim)).expect("chain");
        let mut dirs = Vec::new();
        for _ in 0..ticks(2.0) {
            dirs.extend(fired_dirs(w.tick(&[])));
        }
        assert_eq!(dirs.len(), 2, "both steps fired");
        let look_now = w.player(A).unwrap().look();
        for (k, d) in dirs.iter().enumerate() {
            let got = d.angle_between(look_now).to_degrees();
            assert!((got - want).abs() < 1.0, "skew {skew}°: step {k} fired {got}° off the look");
        }
    }
}

#[test]
fn held_moves_cost_what_borrowed_moves_cost() {
    use bd_sim::powers::chain::{move_cost, move_sparks};
    let t = PowerTable::parse(TABLE).expect("table");
    let w = arena(1);
    let sparks = &w.cfg.tuning.willpower.sparks;
    for mv in &t.def(0).unwrap().moves {
        let cost = move_cost(sparks, &w.cfg.rules, mv, 0);
        let want = move_sparks(sparks, mv, 0, w.cfg.rules.pool_segments()) as f32 * w.cfg.rules.segment_size();
        assert_eq!(cost, want, "{}", mv.name);
        assert!(cost > 0.0 && (cost / w.cfg.rules.segment_size()).fract().abs() < 1e-4, "a whole number of sparks");
    }
}

#[test]
fn a_move_cast_never_forms_inside_the_near_camera_zone() {
    use bd_sim::powers::toolbox::MoveRef;
    // any shipped move whose first op is a plain (non-signature) Cast
    let t = PowerTable::get();
    let mut found = None;
    'find: for (pi, d) in t.powers.iter().enumerate() {
        for (s, m) in d.moves.iter().enumerate() {
            if let Some(MoveOp::Cast { signature: None, .. }) = m.ops.first() {
                found = Some(MoveRef::new(pi as u16, 0, s as u8));
                break 'find;
            }
        }
    }
    let Some(mr) = found else { return };
    let mut w = arena(1);
    stand_at(&mut w, A, 0.0, 0.0);
    let before: Vec<_> = w.constructs.iter().map(|c| c.id).collect();
    if w.borrow_move(A, mr, false, None).is_err() {
        return;
    }
    let p = w.player(A).unwrap();
    let (eye, look) = (p.eye(&w.cfg.move_params), p.look());
    for c in w.constructs.iter().filter(|c| !before.contains(&c.id)) {
        let ahead = (c.pos - eye).dot(look);
        assert!(ahead >= c.half.max_element() + 0.85 || (c.pos - eye).length() > c.half.max_element() + 0.85, "{:?} formed {ahead} m ahead of the eye (size {})", c.recipe.name, c.half.max_element());
    }
}

#[test]
fn non_finite_op_data_never_reaches_the_world() {
    let nan = f32::NAN;
    let ops = [
        MoveOp::Transpose { range: nan, cone: nan, others: false },
        MoveOp::BlinkCarry { dist: nan, up: nan, grab: nan },
        MoveOp::Contagion { range: nan, cone: nan, status: StatusKind::Burn, secs: nan, spread: nan },
    ];
    for op in ops {
        let mut w = arena(2);
        stand_at(&mut w, A, 0.0, 12.0);
        stand_at(&mut w, B, 0.0, 0.0);
        let _ = w.fire_op(A, &op);
        for _ in 0..ticks(1.0) {
            w.tick(&[]);
        }
        for id in [A, B] {
            let p = w.player(id).unwrap();
            assert!(p.mv.pos.is_finite() && p.mv.vel.is_finite(), "{op:?} → {id:?} at {:?}", p.mv.pos);
        }
    }
}
