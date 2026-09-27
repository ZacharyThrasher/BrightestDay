//! SHONEN CLASSICS (docs/toolbox/BRIEF.md; assets/powers/anime_classics.ron + the Naruto / Dragon
//! Ball / Bleach techniques, the devil fruits and the stands): behaviour of the generic ops this pack
//! added — `Drill` (Rasengan, Chidori), `MarkRecall` (Flying Thunder God), `WarpTo` (Instant
//! Transmission), `CopyLast` (the Sharingan's copy), `GazeStun` (genjutsu, Solar Flare), `Phase`
//! (Kamui, King Crimson), `Banish` (Kamui's swirl), `Rush` (ORA / MUDA), `Contagion` (Amaterasu),
//! `Wave` (Getsuga Tensho, Destructo Disc) — fired directly with `World::fire_op`, plus golden
//! phrase readings (kits, borrowed shouts and their mishearings, grants, chains) through the
//! TOOLBOX move index.

use bd_core::corps::Corps;
use bd_core::mechanics::Element;
use bd_core::{PlayerId, Vec3, TICK_HZ};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::powers::ops::OpEvent;
use bd_sim::powers::ops::live::ToolboxFx;
use bd_sim::powers::toolbox::{split_chain, MoveIndex, MoveRef, Reading};
use bd_sim::powers::{norm_words, MoveOp, PowerEvent, PowerTable};
use bd_sim::status::StatusKind;
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);

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

/// Face +Z (toward a player standing behind you on the lab's line).
fn face_back(w: &mut World, id: PlayerId) {
    w.player_mut(id).unwrap().input.yaw = std::f32::consts::PI;
}

fn pos(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().mv.pos
}

fn pct(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().damage_pct
}

fn ticks(secs: f32) -> u32 {
    (secs * TICK_HZ as f32).ceil() as u32
}

fn run(w: &mut World, n: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..n {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn toolbox_fx(evs: &[GameEvent]) -> Vec<ToolboxFx> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::Power(PowerEvent::Op(OpEvent::Toolbox { fx, .. })) => Some(*fx),
            _ => None,
        })
        .collect()
}

fn fx_now(w: &World) -> Vec<ToolboxFx> {
    toolbox_fx(w.events())
}

// ------------------------------------------------------------------------------------ Drill

#[test]
fn drill_latches_grinds_then_spins_them_away() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 4.0);
    stand_at(&mut w, B, 0.0, 0.0);
    let rasengan = MoveOp::Drill { speed: 17.0, secs: 0.45, grind: 0.5, damage: 16.0, knockback: 30.0, element: Element::Wind, up: 0.35 };
    w.fire_op(A, &rasengan).expect("rasengan");
    let mut fx = fx_now(&w);
    let mut latched_at = None;
    let mut grind_pct = 0.0;
    for k in 0..ticks(1.5) {
        let evs = run(&mut w, 1);
        let f = toolbox_fx(&evs);
        if latched_at.is_none() && f.contains(&ToolboxFx::DrillLatch) {
            latched_at = Some(k);
        }
        // while grinding, B takes ticking damage but is held near A's palm (not launched yet)
        if latched_at.is_some() && !fx.contains(&ToolboxFx::DrillRelease) {
            grind_pct = pct(&w, B);
            assert!((pos(&w, B) - pos(&w, A)).length() < 3.5, "held against the palm while grinding");
        }
        fx.extend(f);
    }
    assert!(latched_at.is_some(), "the palm met B and latched: {fx:?}");
    assert!(fx.contains(&ToolboxFx::DrillRelease), "the grind ended in the release: {fx:?}");
    assert!(grind_pct > 2.0, "ticking grind damage ({grind_pct})");
    assert!(pct(&w, B) >= 14.0, "full damage landed: {}", pct(&w, B));
    assert!(pos(&w, B).z < -3.0, "spun away along A's look (-Z): {:?}", pos(&w, B));
    assert!(w.player(A).unwrap().power.live.drill.is_none(), "the drill is over");
}

#[test]
fn drill_without_grind_is_one_piercing_strike_and_a_whiff_just_ends() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 4.0);
    stand_at(&mut w, B, 0.0, 0.0);
    let chidori = MoveOp::Drill { speed: 32.0, secs: 0.35, grind: 0.0, damage: 18.0, knockback: 20.0, element: Element::Electric, up: 0.1 };
    w.fire_op(A, &chidori).expect("chidori");
    let mut fx = fx_now(&w);
    let mut stunned = false;
    for _ in 0..ticks(0.5) {
        let evs = run(&mut w, 1);
        fx.extend(toolbox_fx(&evs));
        stunned |= w.player(B).unwrap().status.active(StatusKind::Stun);
    }
    assert!(fx.contains(&ToolboxFx::DrillRelease) && !fx.contains(&ToolboxFx::DrillLatch), "one strike, no grind: {fx:?}");
    assert!(pct(&w, B) >= 17.0, "the whole damage at once: {}", pct(&w, B));
    assert!(stunned, "lightning stuns");
    // A whiff: nobody in front.
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 4.0);
    stand_at(&mut w, B, 30.0, 0.0);
    w.fire_op(A, &chidori).expect("chidori whiffs but fires");
    let mut fx = fx_now(&w);
    for _ in 0..ticks(0.6) {
        let evs = run(&mut w, 1);
        fx.extend(toolbox_fx(&evs));
    }
    assert!(fx.contains(&ToolboxFx::DrillEnd), "{fx:?}");
    assert_eq!(pct(&w, B), 0.0);
}

// ------------------------------------------------------------------------------------ MarkRecall

#[test]
fn flying_thunder_god_marks_an_enemy_then_teleports_to_them_anywhere() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 12.0);
    stand_at(&mut w, B, 0.0, 0.0);
    let ftg = MoveOp::MarkRecall { range: 45.0, secs: 14.0, strike: 12.0 };
    w.fire_op(A, &ftg).expect("throw the marked kunai");
    assert!(fx_now(&w).contains(&ToolboxFx::Mark));
    run(&mut w, 2);
    // Both move apart; the mark rides on B.
    stand_at(&mut w, A, 30.0, 30.0);
    stand_at(&mut w, B, -20.0, -10.0);
    run(&mut w, 2);
    let before = pct(&w, B);
    w.fire_op(A, &ftg).expect("recall to the mark");
    assert!(fx_now(&w).contains(&ToolboxFx::Recall));
    let d = (pos(&w, A) - pos(&w, B)).length();
    assert!(d < 3.0, "arrived next to the marked enemy ({d} m)");
    assert!(pct(&w, B) > before, "arrival slash");
    // The mark stays for another flash; it expires after its time.
    run(&mut w, ticks(15.0));
    assert!(w.player(A).unwrap().power.live.mark.is_none(), "expired");
}

#[test]
fn flying_thunder_god_on_terrain_is_a_waypoint() {
    let mut w = arena(1);
    stand_at(&mut w, A, 0.0, 0.0);
    // Look down at the ground a few metres ahead: the kunai sticks there.
    w.player_mut(A).unwrap().input.pitch = -0.6;
    w.fire_op(A, &MoveOp::MarkRecall { range: 45.0, secs: 10.0, strike: 0.0 }).expect("mark the ground");
    let mark = pos(&w, A);
    run(&mut w, 1);
    stand_at(&mut w, A, 25.0, 25.0);
    w.fire_op(A, &MoveOp::MarkRecall { range: 45.0, secs: 10.0, strike: 0.0 }).expect("recall");
    let p = pos(&w, A);
    assert!(Vec3::new(p.x, 0.0, p.z).distance(Vec3::new(mark.x, 0.0, mark.z)) < 8.0, "back at the waypoint: {p:?}");
    assert!((p.x - 25.0).abs() > 10.0 || (p.z - 25.0).abs() > 10.0, "not where it was");
}

// ------------------------------------------------------------------------------------ WarpTo

#[test]
fn instant_transmission_lands_behind_the_enemy_you_look_at() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 30.0);
    stand_at(&mut w, B, 0.0, 0.0); // B looks -Z: "behind" B is +Z
    w.fire_op(A, &MoveOp::WarpTo { range: 60.0, cone: 15.0, behind: 1.6, line: false }).expect("instant transmission");
    let (a, b) = (pos(&w, A), pos(&w, B));
    assert!((a - b).length() < 3.0, "on them: {a:?} vs {b:?}");
    assert!(a.z > b.z + 0.8, "behind their back");
    assert!(fx_now(&w).contains(&ToolboxFx::Warp));
    // `line`: measured along YOUR line to them: you arrive point-blank in front of them, facing them
    // (the chainable Instant Transmission → Kamehameha).
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 30.0);
    stand_at(&mut w, B, 0.0, 0.0);
    face_back(&mut w, B); // B faces A: their back is the far side, yet you still land facing them
    w.fire_op(A, &MoveOp::WarpTo { range: 60.0, cone: 15.0, behind: 1.6, line: true }).expect("instant transmission");
    let (a, b) = (pos(&w, A), pos(&w, B));
    assert!(a.z > b.z + 0.8 && a.z < b.z + 3.0, "between you and them, point-blank: {a:?} vs {b:?}");
    // Nobody under the crosshair or near it: it fails and costs nothing.
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 30.0);
    stand_at(&mut w, B, 0.0, 0.0);
    face_back(&mut w, A);
    assert!(w.fire_op(A, &MoveOp::WarpTo { range: 60.0, cone: 15.0, behind: 1.6, line: false }).is_err());
}

// ------------------------------------------------------------------------------------ CopyLast

#[test]
fn copy_replays_the_last_move_an_enemy_used_nearby() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    face_back(&mut w, B);
    let copy = MoveOp::CopyLast { range: 40.0, secs: 8.0 };
    assert!(w.fire_op(A, &copy).is_err(), "nothing seen yet");
    // B fires a borrowed move (the phoenix flower jutsu: a spray of fireballs) facing away.
    let flower = MoveIndex::shipped().shout(&norm_words("phoenix flower jutsu")).expect("shout");
    w.borrow_move(B, flower, false, None).expect("B's jutsu");
    run(&mut w, 2);
    let mine = |w: &World| w.constructs.iter().filter(|c| c.owner == A && c.recipe.name == "phoenix sage fire").count();
    assert_eq!(mine(&w), 0);
    w.fire_op(A, &copy).expect("copied");
    assert!(fx_now(&w).contains(&ToolboxFx::Copy));
    assert!(mine(&w) > 0, "A threw the same fireballs");
    assert!(w.fire_op(A, &copy).is_err(), "a copy is used up");
}

// ------------------------------------------------------------------------------------ GazeStun

#[test]
fn genjutsu_only_catches_someone_looking_at_you() {
    let gaze = MoveOp::GazeStun { range: 22.0, cone: 40.0, secs: 0.9, status: StatusKind::Stun, aimed: true };
    // B looks straight at A: caught.
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    face_back(&mut w, B);
    w.fire_op(A, &gaze).expect("genjutsu");
    assert!(w.player(B).unwrap().status.stunned(), "stunned");
    run(&mut w, ticks(1.3));
    assert!(!w.player(B).unwrap().status.stunned(), "short: never a lock");
    // B looks away: nothing (and nothing spent).
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    assert!(w.fire_op(A, &gaze).is_err());
    assert!(!w.player(B).unwrap().status.stunned());
    // A radial flash (Solar Flare) blinds everyone facing you, spares the rest.
    let mut w = arena(3);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    stand_at(&mut w, C, 4.0, 2.0);
    face_back(&mut w, B);
    w.fire_op(A, &MoveOp::GazeStun { range: 24.0, cone: 70.0, secs: 3.0, status: StatusKind::Shadow, aimed: false }).expect("flare");
    assert!(w.player(B).unwrap().status.active(StatusKind::Shadow), "B looked");
    assert!(!w.player(C).unwrap().status.active(StatusKind::Shadow), "C was looking away");
}

// ------------------------------------------------------------------------------------ Phase

#[test]
fn phase_lets_every_hit_through_until_you_strike() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    w.fire_op(A, &MoveOp::Phase { secs: 1.2 }).expect("kamui");
    let c = w.player(A).unwrap().hull(w.params()).center();
    let hit = HitInfo::new(A, Some(B), HitKind::Explosion, 20.0, 20.0, Vec3::Z, c);
    assert!(!w.apply_hit(hit).applied, "it passes through");
    assert_eq!(pct(&w, A), 0.0);
    // Striking makes you solid again.
    let push = MoveIndex::shipped().shout(&norm_words("frog kumite")).expect("shout");
    w.borrow_move(A, push, false, None).expect("strike");
    assert!(w.apply_hit(hit).applied, "solid again");
    // And it runs out on its own.
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 10.0);
    w.fire_op(A, &MoveOp::Phase { secs: 0.5 }).expect("phase");
    run(&mut w, ticks(0.7));
    let c = w.player(A).unwrap().hull(w.params()).center();
    assert!(w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 5.0, 5.0, Vec3::Z, c)).applied);
}

// ------------------------------------------------------------------------------------ Banish

#[test]
fn banish_swallows_a_construct_or_throws_the_enemy_up_unhurt() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    let y0 = pos(&w, B).y;
    w.fire_op(A, &MoveOp::Banish { range: 30.0, dist: 9.0 }).expect("kamui");
    assert!(pos(&w, B).y > y0 + 5.0, "warped up: {:?}", pos(&w, B));
    assert_eq!(pct(&w, B), 0.0, "no damage");
    assert!(!w.player(B).unwrap().status.stunned(), "keeps control");
    assert!(fx_now(&w).contains(&ToolboxFx::Banish));
}

// ------------------------------------------------------------------------------------ Rush

#[test]
fn rush_brings_out_a_stand_that_punches_then_fades() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 2.6);
    stand_at(&mut w, B, 0.0, 0.0);
    let ora = MoveOp::Rush {
        hits: 10,
        rate: 20.0,
        damage: 2.0,
        knockback: 0.0,
        range: 4.0,
        stand: Some(bd_sim::powers::RecipeDef { name: "Star Platinum".into(), form: bd_core::mechanics::Form::Biped, scale: 2.0, ..Default::default() }),
    };
    w.fire_op(A, &ora).expect("ORA");
    let stand = w.constructs.iter().find(|c| c.owner == A && c.recipe.name == "Star Platinum").map(|c| c.id).expect("the stand appears");
    let mut punches = 0;
    let evs = run(&mut w, ticks(0.8));
    for e in &evs {
        if let GameEvent::Power(PowerEvent::StandRush { player, stand: s, .. }) = e {
            assert_eq!((*player, *s), (A, stand));
            punches += 1;
        }
    }
    assert_eq!(punches, 10, "every punch thrown");
    assert!(pct(&w, B) > 12.0, "they landed: {}", pct(&w, B));
    run(&mut w, ticks(1.0));
    assert!(w.constructs.get(stand).is_none_or(|c| c.gone.is_some()), "the stand fades after the rush");
    assert!(w.player(A).unwrap().power.live.rush.is_none());
}

// ------------------------------------------------------------------------------------ Contagion

#[test]
fn amaterasu_sticks_spreads_and_burns_out() {
    let mut w = arena(3);
    stand_at(&mut w, A, 0.0, 12.0);
    stand_at(&mut w, B, 0.0, 0.0);
    stand_at(&mut w, C, 1.6, -0.5);
    w.fire_op(A, &MoveOp::Contagion { range: 30.0, cone: 6.0, status: StatusKind::Burn, secs: 4.0, spread: 2.5 }).expect("amaterasu");
    run(&mut w, ticks(0.6));
    assert!(w.player(B).unwrap().status.active(StatusKind::Burn), "B burns");
    assert!(w.player(C).unwrap().power.live.brand.is_some(), "it jumped to C standing close");
    // Water doesn't put it out: clear the status, it comes straight back.
    w.player_mut(B).unwrap().status.clear();
    run(&mut w, ticks(0.3));
    assert!(w.player(B).unwrap().status.active(StatusKind::Burn), "can't be put out");
    let hurt = pct(&w, B);
    run(&mut w, ticks(1.0));
    assert!(pct(&w, B) > hurt, "it keeps burning");
    run(&mut w, ticks(6.0));
    assert!(w.player(B).unwrap().power.live.brand.is_none() && w.player(C).unwrap().power.live.brand.is_none(), "it burns out");
    // Nobody aimed: fails.
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 12.0);
    stand_at(&mut w, B, 0.0, 0.0);
    face_back(&mut w, A);
    assert!(w.fire_op(A, &MoveOp::Contagion { range: 30.0, cone: 6.0, status: StatusKind::Burn, secs: 4.0, spread: 2.5 }).is_err());
}

// ------------------------------------------------------------------------------------ Wave

#[test]
fn getsuga_crescent_travels_passes_through_and_hits_each_once() {
    let mut w = arena(3);
    stand_at(&mut w, A, 0.0, 20.0);
    stand_at(&mut w, B, 0.0, 0.0);
    stand_at(&mut w, C, 1.0, -8.0);
    let getsuga = MoveOp::Wave { length: 36.0, speed: 30.0, width: 4.5, carve: 0.9, damage: 16.0, knockback: 0.0, up: 0.0, element: Element::Shadow, thin: true };
    w.fire_op(A, &getsuga).expect("getsuga tensho");
    assert!(fx_now(&w).contains(&ToolboxFx::Wave));
    run(&mut w, 2);
    assert_eq!(pct(&w, B), 0.0, "it travels: not instant");
    run(&mut w, ticks(1.4));
    assert!(pct(&w, B) >= 15.0 && pct(&w, B) < 20.0, "B hit once: {}", pct(&w, B));
    assert!(pct(&w, C) >= 15.0, "passed through B and hit C: {}", pct(&w, C));
    // Cut a trench on the way (carve > 0): the ground under its path is lower now.
    let evs = run(&mut w, 1);
    let _ = evs;
    assert!(w.player(A).unwrap().power.live.waves.iter().all(|x| x.is_none()), "flight over");
}

#[test]
fn a_crescent_without_carve_breaks_on_the_first_wall() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 20.0);
    stand_at(&mut w, B, 0.0, 0.0);
    // Aim into the ground: it breaks at once.
    w.player_mut(A).unwrap().input.pitch = -1.2;
    let wave = MoveOp::Wave { length: 36.0, speed: 30.0, width: 2.0, carve: 0.0, damage: 16.0, knockback: 0.0, up: 0.0, element: Element::Light, thin: false };
    w.fire_op(A, &wave).expect("fires");
    run(&mut w, ticks(1.5));
    assert_eq!(pct(&w, B), 0.0, "broke on the ground");
}

// ------------------------------------------------------------------------------------ nothing live

#[test]
fn nobody_using_the_ops_carries_no_live_state() {
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 10.0);
    stand_at(&mut w, B, 0.0, 0.0);
    run(&mut w, ticks(2.0));
    for p in &w.players {
        assert!(p.power.live.idle(), "idle");
    }
}

// ------------------------------------------------------------------------------ golden phrases

fn read(s: &str) -> Reading {
    MoveIndex::shipped().read(PowerTable::get(), None, s)
}

/// (power id, tier, move name) a phrase fires for a powerless player.
fn fires(s: &str) -> Option<(String, u8, String)> {
    let m: MoveRef = read(s).move_ref()?;
    let (p, mv) = m.resolve(PowerTable::get())?;
    Some((p.id.clone(), m.tier, mv.name.clone()))
}

fn is(s: &str, power: &str, mv: &str) {
    let got = fires(s);
    assert_eq!(got.as_ref().map(|g| (g.0.as_str(), g.2.as_str())), Some((power, mv)), "{s:?} → {got:?}");
}

fn grants(s: &str) {
    assert_eq!(read(s), Reading::Plain, "{s:?} is a power / form NAME (grants)");
    assert!(MoveIndex::shipped().is_grant_name(&norm_words(s)), "{s:?}");
}

#[test]
fn naruto_phrases() {
    for s in ["rasengan", "RASENGAN!", "rasen gan", "rasengun", "spiraling sphere"] {
        is(s, "kit_naruto", "rasengan");
    }
    is("naruto rasengan", "kit_naruto", "rasengan");
    is("naruto ultimate", "kit_naruto", "rasenshuriken");
    is("rasenshuriken", "kit_naruto", "rasenshuriken");
    is("wind style rasenshuriken", "kit_naruto", "rasenshuriken");
    is("talk no jutsu", "kit_naruto", "talk no jutsu");
    is("sexy jutsu", "kit_naruto", "sexy jutsu");
    is("multi shadow clone jutsu", "shadow_clone", "multi shadow clone jutsu");
    is("sage art rasengan", "sage_mode", "sage art rasengan");
    // The Sharingan reads, copies and casts genjutsu; the fireball is only a shout now.
    is("copy jutsu", "sharingan", "copy");
    is("genjutsu", "sharingan", "genjutsu");
    is("chidori", "sharingan", "chidori");
    is("one thousand birds", "sharingan", "chidori");
    is("fireball jutsu", "sharingan", "fireball jutsu");
    is("amaterasu", "sharingan", "amaterasu");
    is("tsukuyomi", "sharingan", "tsukuyomi");
    is("susanoo", "sharingan", "susanoo");
    is("sasuke chidori", "kit_sasuke", "chidori");
    is("sasuke kirin", "kit_sasuke", "kirin");
    is("itachi tsukuyomi", "kit_itachi", "tsukuyomi");
    is("itachi amaterasu", "kit_itachi", "amaterasu");
    is("raikiri", "kit_kakashi", "raikiri");
    is("kakashi kamui", "kit_kakashi", "kamui");
    is("obito kamui", "kit_obito", "kamui");
    is("kamui jutsu", "kit_obito", "kamui");
    for s in ["flying thunder god", "hiraishin", "flying raijin"] {
        is(s, "kit_minato", "flying thunder god");
    }
    is("minato rasengan", "kit_minato", "rasengan");
    is("shinra tensei", "rinnegan", "almighty push");
    is("pain shinra tensei", "kit_pain", "shinra tensei");
    is("amenotejikara", "rinnegan", "amenotejikara");
    is("preta path", "rinnegan", "preta path");
    is("sand coffin", "kit_gaara", "sand coffin");
    is("sabaku soso", "kit_gaara", "sand burial");
    is("infinite tsukuyomi", "kit_madara", "infinite tsukuyomi");
    is("dynamic entry", "kit_might_guy", "dynamic entry");
    is("night guy", "kit_might_guy", "night guy");
    is("hidden lotus", "kit_rock_lee", "hidden lotus");
    is("eight trigrams sixty four palms", "kit_hinata", "sixty four palms");
    is("kaiten", "kit_neji", "rotation");
    is("body flicker", "ninjutsu", "body flicker");
    is("kawarimi", "ninjutsu", "substitution");
    is("shadow possession jutsu", "kit_shikamaru", "shadow possession");
    grants("eight gates");
    grants("gate of death");
    grants("sharingan");
    grants("mangekyo sharingan");
    grants("byakugan");
}

#[test]
fn dragon_ball_phrases() {
    for s in ["kamehameha", "kamehamaha", "kame hame ha", "KAMEHAMEHA!!!"] {
        is(s, "kit_goku", "kamehameha");
    }
    is("instant transmission", "super_saiyan", "instant transmission");
    is("goku instant transmission", "kit_goku", "instant transmission");
    is("goku ultimate", "kit_goku", "spirit bomb");
    is("genki dama", "kit_goku", "spirit bomb");
    is("instant kamehameha", "kit_goku", "instant kamehameha");
    is("garlic gun", "kit_vegeta", "galick gun");
    is("final flash", "kit_vegeta", "final flash");
    is("final flesh", "kit_vegeta", "final flash");
    is("special beam cannon", "kit_piccolo", "special beam cannon");
    is("makankosappo", "kit_piccolo", "special beam cannon");
    is("destructo disc", "kit_krillin", "destructo disc");
    is("kienzan", "kit_krillin", "destructo disc");
    is("solar flare", "kit_krillin", "solar flare");
    is("taiyoken", "kit_krillin", "solar flare");
    is("death beam", "kit_frieza", "death beam");
    is("hellzone grenade", "kit_piccolo", "hellzone grenade");
    is("masenko", "kit_gohan", "masenko");
    is("hakai", "kit_beerus", "hakai");
    is("kaioken kamehameha", "kaioken", "kaioken kamehameha");
    grants("super saiyan blue");
    grants("ultra instinct");
}

#[test]
fn one_piece_phrases() {
    for s in ["gum gum pistol", "gomu gomu no pistol", "gum-gum pistol"] {
        is(s, "gomu_gomu", "pistol");
    }
    is("gum gum gatling", "gomu_gomu", "gatling");
    is("gomu gomu no bazooka", "gomu_gomu", "bazooka");
    is("red hawk", "gomu_gomu", "red hawk");
    is("bajrang gun", "gomu_gomu", "bajrang gun");
    is("luffy gatling", "kit_luffy", "gatling");
    is("zoro asura", "kit_zoro", "asura");
    is("asura", "kit_zoro", "asura");
    for s in ["oni giri", "onigiri", "santoryu oni giri"] {
        is(s, "kit_zoro", "oni giri");
    }
    is("360 pound phoenix", "kit_zoro", "360 pound phoenix");
    is("diable jambe", "kit_sanji", "diable jambe");
    is("sky walk", "kit_sanji", "sky walk");
    is("fire fist", "mera_mera", "fire fist");
    is("hiken", "mera_mera", "fire fist");
    is("room", "ope_ope", "room");
    is("shambles", "ope_ope", "shambles");
    is("gamma knife", "ope_ope", "gamma knife");
    is("quake punch", "gura_gura", "quake punch");
    is("seaquake", "gura_gura", "seaquake");
    is("conqueror's haki", "kit_shanks", "conqueror's haki");
    is("haoshoku haki", "kit_shanks", "conqueror's haki");
    is("black vortex", "yami_yami", "black vortex");
    is("light speed kick", "pika_pika", "light speed kick");
    is("love love beam", "kit_hancock", "love love beam");
    grants("gear fifth");
    grants("gum gum fruit");
}

#[test]
fn bleach_phrases() {
    is("getsuga tensho", "bankai", "getsuga tensho");
    is("getsuga", "bankai", "getsuga tensho");
    is("ichigo getsuga tensho", "kit_ichigo", "getsuga tensho");
    is("getsuga jujisho", "kit_ichigo", "getsuga jujisho");
    is("senbonzakura", "kit_byakuya", "senbonzakura");
    is("scatter senbonzakura", "kit_byakuya", "senbonzakura");
    is("kyoka suigetsu", "kit_aizen", "kyoka suigetsu");
    is("hado 90", "kit_aizen", "black coffin");
    is("hado 31", "kido", "shakkaho");
    is("byakurai", "kido", "byakurai");
    is("shunko", "kit_yoruichi", "shunko");
    is("hyorinmaru", "kit_toshiro", "hyorinmaru");
    is("gran rey cero", "kit_grimmjow", "gran rey cero");
    grants("bankai");
    grants("kido");
}

#[test]
fn jojo_phrases() {
    for s in ["ora ora", "ORA ORA ORA", "oraoraora"] {
        is(s, "star_platinum", "ora rush");
    }
    is("muda muda", "the_world", "muda rush");
    is("dora dora", "crazy_diamond", "dora rush");
    is("jotaro ora rush", "kit_jotaro", "ora rush");
    is("dio za warudo", "kit_dio", "za warudo");
    is("dio road roller", "kit_dio", "road roller");
    is("bites the dust", "killer_queen", "bites the dust");
    is("killer queen touch", "killer_queen", "first bomb");
    is("kira detonate", "kit_kira", "detonate");
    is("return to zero", "gold_experience", "return to zero");
    is("time skip", "king_crimson", "erase time");
    is("epitaph", "king_crimson", "epitaph");
    is("hermit purple", "kit_joseph", "hermit purple");
    is("nigerundayo", "kit_joseph", "run away");
    is("made in heaven", "kit_pucci", "made in heaven");
    grants("gold experience requiem");
    grants("star platinum");
}

#[test]
fn chains_split_into_the_famous_combos() {
    let ix = MoveIndex::shipped();
    let is_move = |s: &str| ix.read(PowerTable::get(), None, s).is_move();
    let split = |s: &str| split_chain(s, &is_move);
    assert_eq!(split("instant transmission then kamehameha"), vec!["instant transmission", "kamehameha"]);
    assert_eq!(split("flying thunder god -> rasengan"), vec!["flying thunder god", "rasengan"]);
    assert_eq!(split("gum gum rocket into gum gum bazooka"), vec!["gum gum rocket", "gum gum bazooka"]);
    assert_eq!(split("room, shambles, gamma knife"), vec!["room", "shambles", "gamma knife"]);
    assert_eq!(split("za warudo > muda muda > road roller"), vec!["za warudo", "muda muda", "road roller"]);
}

#[test]
fn every_headline_uses_its_own_mechanic() {
    // The owner's rule: a famous power does the thing it is famous for (not "a projectile comes out").
    let op_of = |s: &str| {
        let m = read(s).move_ref().unwrap_or_else(|| panic!("{s:?}"));
        let (_, mv) = m.resolve(PowerTable::get()).unwrap();
        mv.ops.clone()
    };
    let has = |s: &str, f: &dyn Fn(&MoveOp) -> bool| assert!(op_of(s).iter().any(f), "{s:?}: {:?}", op_of(s));
    has("rasengan", &|o| matches!(o, MoveOp::Drill { grind, .. } if *grind > 0.0));
    has("chidori", &|o| matches!(o, MoveOp::Drill { grind, element: Element::Electric, .. } if *grind == 0.0));
    has("flying thunder god", &|o| matches!(o, MoveOp::MarkRecall { .. }));
    has("instant transmission", &|o| matches!(o, MoveOp::WarpTo { .. }));
    has("obito kamui", &|o| matches!(o, MoveOp::Phase { .. }));
    has("obito kamui", &|o| matches!(o, MoveOp::Banish { .. }));
    has("amaterasu", &|o| matches!(o, MoveOp::Contagion { .. }));
    has("copy jutsu", &|o| matches!(o, MoveOp::CopyLast { .. }));
    has("genjutsu", &|o| matches!(o, MoveOp::GazeStun { aimed: true, .. }));
    has("solar flare", &|o| matches!(o, MoveOp::GazeStun { aimed: false, .. }));
    has("getsuga tensho", &|o| matches!(o, MoveOp::Wave { .. }));
    has("destructo disc", &|o| matches!(o, MoveOp::Wave { thin: true, .. }));
    has("special beam cannon", &|o| matches!(o, MoveOp::Carve { .. }));
    has("ora ora", &|o| matches!(o, MoveOp::Rush { .. }));
    has("bites the dust", &|o| matches!(o, MoveOp::Rewind { .. }));
    has("time skip", &|o| matches!(o, MoveOp::Phase { .. }));
    has("return to zero", &|o| matches!(o, MoveOp::Counter { .. }));
    has("return to zero", &|o| matches!(o, MoveOp::Deflect { .. }));
    has("gum gum pistol", &|o| matches!(o, MoveOp::Shove { range, .. } if *range >= 6.0));
    has("dio za warudo", &|o| matches!(o, MoveOp::TimeStop { .. }));
    has("star platinum time stop", &|o| matches!(o, MoveOp::TimeStop { .. }));
    has("conqueror's haki", &|o| matches!(o, MoveOp::Zone { status: Some(StatusKind::Stun), .. }));
}

// ------------------------------------------------------------------------------ chains that land

#[test]
fn instant_transmission_chains_into_a_point_blank_rasengan() {
    use bd_sim::powers::chain::ToolStep;
    let mut w = arena(2);
    stand_at(&mut w, A, 0.0, 17.0);
    stand_at(&mut w, B, 1.5, 0.0);
    face_back(&mut w, B); // B watches A come
    let ix = MoveIndex::shipped();
    let step = |s: &str| {
        let r = ix.read(PowerTable::get(), None, s);
        ToolStep::Move { mv: r.move_ref().unwrap_or_else(|| panic!("{s:?}")), kit: matches!(r, Reading::Kit(_)) }
    };
    w.toolbox(A, vec![step("instant transmission"), step("rasengan")], None).expect("chain");
    let mut fx = fx_now(&w);
    for _ in 0..ticks(2.5) {
        let evs = run(&mut w, 1);
        fx.extend(toolbox_fx(&evs));
    }
    assert!(fx.contains(&ToolboxFx::Warp), "warped: {fx:?}");
    assert!(fx.contains(&ToolboxFx::DrillLatch), "the rasengan landed on them: {fx:?}");
    assert!(pct(&w, B) >= 14.0, "{}", pct(&w, B));
}
