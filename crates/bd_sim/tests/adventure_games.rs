//! ADVENTURE GAMES (docs/toolbox/BRIEF.md, `assets/powers/games_adventure.ron`): a behaviour
//! assertion per generic op this wave added (Pounce, Salvo, Mend, Evade, PlacePortal, Empower) and
//! the golden phrases — kit phrases, borrowed shouts and their mishearings, chains, forms that
//! grant, and icon names that must stay summons.

use bd_core::corps::Corps;
use bd_core::mechanics::Element;
use bd_core::{PlayerId, TICK_HZ, Vec3};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::ToolStep;
use bd_sim::powers::ops::{OpEvent, PouncePhase};
use bd_sim::powers::toolbox::{split_chain, MoveIndex, MoveRef, Reading};
use bd_sim::powers::{At, MoveOp, PowerError, PowerEvent, PowerTable, norm_words};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn duel() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=2u8 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    stand(&mut w, A, 0.0, 0.0);
    stand(&mut w, B, 0.0, -30.0);
    w
}

/// Stand `id` on the ground at (x, z), looking down -Z.
fn stand(w: &mut World, id: PlayerId, x: f32, z: f32) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = Vec3::new(x, GROUND, z);
    p.mv.vel = Vec3::ZERO;
    p.input.yaw = 0.0;
    p.input.pitch = 0.0;
    p.damage_pct = 0.0;
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

fn ops(evs: &[GameEvent]) -> Vec<OpEvent> {
    evs.iter().filter_map(|e| if let GameEvent::Power(PowerEvent::Op(o)) = e { Some(o.clone()) } else { None }).collect()
}

fn pct(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().damage_pct
}

fn centre(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().hull(&w.cfg.move_params).center()
}

fn pounce() -> MoveOp {
    MoveOp::Pounce { range: 18.0, speed: 42.0, cone: 22.0, damage: 8.0, knockback: 10.0, element: Element::Wind, up: 0.3, bounce: 12.0, secs: 0.0 }
}

// ------------------------------------------------------------------------------------ Pounce

#[test]
fn pounce_homes_onto_the_aimed_enemy_strikes_and_bounces_up() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -10.0);
    let mut evs = {
        w.fire_op(A, &pounce()).expect("pounce starts");
        w.events().to_vec()
    };
    assert!(ops(&evs).iter().any(|e| matches!(e, OpEvent::Pounce { phase: PouncePhase::Start, target: Some(t), .. } if *t == B)), "locked on B");
    let mut hit_at = None;
    for t in 0..ticks(0.8) {
        let e = w.tick(&[]).to_vec();
        if hit_at.is_none() && ops(&e).iter().any(|o| matches!(o, OpEvent::Pounce { phase: PouncePhase::Hit, target: Some(t), .. } if *t == B)) {
            hit_at = Some(t);
        }
        evs.extend(e);
    }
    let at = hit_at.expect("the lunge reached B");
    assert!(at <= ticks(0.45), "10 m at 42 m/s arrives fast (tick {at})");
    assert!(pct(&w, B) > 0.0, "B was struck");
    // The bounce: you come off them UP (airborne, ready for the next homing attack).
    let mut w2 = duel();
    stand(&mut w2, B, 0.0, -10.0);
    w2.fire_op(A, &pounce()).unwrap();
    run(&mut w2, at + 1);
    let vy = w2.player(A).unwrap().mv.vel.y;
    assert!(vy > 6.0, "bounced up off them (vy {vy})");
    run(&mut w2, ticks(0.15));
    assert!(w2.player(A).unwrap().mv.pos.y > GROUND + 0.5, "still airborne after the bounce");
}

#[test]
fn pounce_with_nobody_aimed_dashes_along_the_look_and_whiffs() {
    let mut w = duel();
    stand(&mut w, B, 25.0, 25.0); // behind and to the side: not in the cone
    w.fire_op(A, &pounce()).unwrap();
    let evs = run(&mut w, ticks(1.0));
    assert!(w.player(A).unwrap().mv.pos.z < -4.0, "dashed forward along -Z");
    assert!(ops(&evs).iter().any(|e| matches!(e, OpEvent::Pounce { phase: PouncePhase::Whiff, .. })));
    assert_eq!(pct(&w, B), 0.0);
}

// ------------------------------------------------------------------------------------- Salvo

fn seeking_bolt(delay: f32) -> MoveOp {
    MoveOp::Salvo {
        count: 1,
        delay,
        interval: 0.0,
        radius: 2.5,
        damage: 10.0,
        knockback: 10.0,
        element: Element::Electric,
        carve: 0.0,
        up: 0.0,
        at: At::Me,
        dist: 0.0,
        spread: 12.0,
        seek: true,
        track: false,
        cut: false,
    }
}

#[test]
fn salvo_marks_first_and_lands_after_the_delay() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -8.0);
    let evs = {
        w.fire_op(A, &seeking_bolt(0.5)).expect("salvo starts");
        w.events().to_vec()
    };
    let mark = ops(&evs).iter().find_map(|e| if let OpEvent::Telegraph { pos, secs, .. } = e { Some((*pos, *secs)) } else { None });
    let (pos, secs) = mark.expect("the strike is marked the moment it's cast");
    assert!((pos - centre(&w, B)).length() < 1.0, "seek put the mark on B ({pos:?})");
    assert!((secs - 0.5).abs() < 0.05);
    run(&mut w, ticks(0.3));
    assert_eq!(pct(&w, B), 0.0, "nothing lands before the delay");
    run(&mut w, ticks(0.35));
    assert!(pct(&w, B) > 0.0, "the strike landed");
    assert_eq!(pct(&w, A), 0.0, "never hurts its caster");
}

#[test]
fn salvo_marks_are_dodgeable_they_do_not_follow() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -8.0);
    w.fire_op(A, &seeking_bolt(0.6)).unwrap();
    run(&mut w, ticks(0.2));
    stand(&mut w, B, 14.0, -20.0); // B leaves the mark
    run(&mut w, ticks(0.6));
    assert_eq!(pct(&w, B), 0.0, "the mark stays where it was placed");
}

#[test]
fn salvo_staggers_a_count_one_every_interval() {
    let mut w = duel();
    let op = MoveOp::Salvo {
        count: 4,
        delay: 0.3,
        interval: 0.2,
        radius: 1.5,
        damage: 1.0,
        knockback: 0.0,
        element: Element::Light,
        carve: 0.0,
        up: 0.0,
        at: At::Front,
        dist: 6.0,
        spread: 5.0,
        seek: false,
        track: false,
        cut: false,
    };
    w.fire_op(A, &op).unwrap();
    let mut blasts = Vec::new();
    for t in 0..ticks(1.4) {
        for e in w.tick(&[]) {
            if let GameEvent::Explosion { source: Some(s), .. } = e {
                if *s == A {
                    blasts.push(t);
                }
            }
        }
    }
    assert_eq!(blasts.len(), 4, "{blasts:?}");
    for pair in blasts.windows(2) {
        let gap = pair[1] - pair[0];
        assert!((ticks(0.2) as i64 - gap as i64).abs() <= 1, "one every interval ({blasts:?})");
    }
}

// -------------------------------------------------------------------------------------- Mend

#[test]
fn mend_heals_now_then_over_time_and_a_hit_ends_the_trickle() {
    let mut w = duel();
    w.player_mut(A).unwrap().damage_pct = 50.0;
    w.fire_op(A, &MoveOp::Mend { pct: 20.0, per_s: 10.0, secs: 2.0 }).unwrap();
    assert!((pct(&w, A) - 30.0).abs() < 0.01);
    run(&mut w, ticks(1.0));
    let after = pct(&w, A);
    assert!((after - 20.0).abs() < 1.0, "~10 %/s for a second ({after})");
    let c = centre(&w, A);
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 5.0, 0.0, Vec3::Z, c));
    let hurt = pct(&w, A);
    run(&mut w, ticks(0.8));
    assert!((pct(&w, A) - hurt).abs() < 0.01, "the hit stopped the healing");
    // Nothing to heal and no trickle: the move does nothing (and costs nothing).
    let mut w = duel();
    assert_eq!(w.fire_op(A, &MoveOp::Mend { pct: 30.0, per_s: 0.0, secs: 0.0 }), Err(PowerError::NoTarget));
}

// ------------------------------------------------------------------------------------- Evade

#[test]
fn evade_slips_the_first_hit_and_grants_time_slow() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -2.0);
    w.fire_op(A, &MoveOp::Evade { secs: 0.4, slow: 2.0, speed: 2.0, blink: 0.0 }).unwrap();
    run(&mut w, 2);
    let c = centre(&w, A);
    let r = w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 10.0, 14.0, Vec3::Z, c));
    assert!(!r.applied, "the perfect dodge slipped it");
    assert_eq!(pct(&w, A), 0.0);
    let a = w.player(A).unwrap();
    assert!(a.power.time_stop > 1.5, "witch time ({})", a.power.time_stop);
    assert!(a.mv.extra.dodging(), "the dodge (i-frames + sidestep) started");
    // The window is spent: the next blow lands.
    run(&mut w, ticks(0.5));
    let c = centre(&w, A);
    assert!(w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 10.0, 0.0, Vec3::Z, c)).applied);
    // A borrowed evade by a powerless player: the slow runs out on its own.
    run(&mut w, ticks(2.5));
    assert_eq!(w.player(A).unwrap().power.time_stop, 0.0);
}

#[test]
fn evade_whiffed_window_just_closes() {
    let mut w = duel();
    w.fire_op(A, &MoveOp::Evade { secs: 0.3, slow: 2.0, speed: 2.0, blink: 0.0 }).unwrap();
    run(&mut w, ticks(0.5));
    let c = centre(&w, A);
    assert!(w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 10.0, 0.0, Vec3::Z, c)).applied, "too late: it lands");
    assert_eq!(w.player(A).unwrap().power.time_stop, 0.0, "no reward");
}

// ------------------------------------------------------------------------------- PlacePortal

#[test]
fn place_portal_links_the_two_newest_ends() {
    let mut w = duel();
    stand(&mut w, B, 30.0, 30.0);
    let lo = bd_core::voxel::world_to_voxel(Vec3::new(-6.0, GROUND, -12.0));
    let hi = bd_core::voxel::world_to_voxel(Vec3::new(6.0, GROUND + 8.0, -11.0));
    w.voxels.fill_box(lo, hi, bd_voxel::mat::STONE);
    let lo = bd_core::voxel::world_to_voxel(Vec3::new(11.0, GROUND, -6.0));
    let hi = bd_core::voxel::world_to_voxel(Vec3::new(12.0, GROUND + 8.0, 6.0));
    w.voxels.fill_box(lo, hi, bd_voxel::mat::STONE);
    let gun = MoveOp::PlacePortal { range: 60.0, secs: 20.0 };
    let mine = |w: &World| w.portals.iter().filter(|p| p.owner == A).count();
    w.fire_op(A, &gun).expect("blue lands on the wall ahead");
    assert_eq!(mine(&w), 0, "one end alone is not a portal yet");
    w.player_mut(A).unwrap().input.yaw = -std::f32::consts::FRAC_PI_2; // face +X
    w.fire_op(A, &gun).expect("orange lands on the side wall");
    assert_eq!(mine(&w), 1, "blue + orange = a pair");
    let pair = w.portals.iter().find(|p| p.owner == A).unwrap();
    assert!(pair.ends[0].pos.z < -10.0 && pair.ends[1].pos.x > 10.0, "{:?}", pair.ends.map(|e| e.pos));
    // A third shot moves the blue end; still one pair.
    w.player_mut(A).unwrap().input.yaw = 0.0;
    w.player_mut(A).unwrap().mv.pos.x = 3.0;
    w.fire_op(A, &gun).unwrap();
    assert_eq!(mine(&w), 1);
    // Open sky takes no portal.
    w.player_mut(A).unwrap().input.pitch = 1.3;
    assert_eq!(w.fire_op(A, &gun), Err(PowerError::NoTarget));
}

// ----------------------------------------------------------------------------------- Empower

#[test]
fn empower_boosts_damage_armors_you_and_touching_hurts() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -1.0);
    let op = MoveOp::Empower { secs: 3.0, damage: 2.0, armor: 0.5, contact: 6.0, contact_kb: 8.0, contact_cd: 1.0, element: Element::Light };
    w.fire_op(A, &op).unwrap();
    run(&mut w, 3);
    assert!(pct(&w, B) > 0.0, "touching you hurts");
    let touched = pct(&w, B);
    run(&mut w, ticks(0.3));
    assert!((pct(&w, B) - touched).abs() < 0.01 || w.player(B).unwrap().mv.pos.z < -2.5, "once per contact_cd (or flung away)");
    // Your hits deal double.
    stand(&mut w, B, 0.0, -30.0);
    let c = centre(&w, B);
    w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, 5.0, 0.0, Vec3::Z, c));
    assert!((pct(&w, B) - 10.0).abs() < 0.6, "×2 damage ({})", pct(&w, B));
    // Expires.
    run(&mut w, ticks(3.2));
    let before = pct(&w, B);
    let c = centre(&w, B);
    w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, 5.0, 0.0, Vec3::Z, c));
    assert!((pct(&w, B) - before - 5.0).abs() < 0.6, "back to ×1");
}

// ------------------------------------------------------------------------------ golden phrases

fn shout(s: &str) -> MoveRef {
    MoveIndex::shipped().shout(&norm_words(s)).unwrap_or_else(|| panic!("no shout {s:?}"))
}

fn read(s: &str) -> Reading {
    MoveIndex::shipped().read(PowerTable::get(), None, s)
}

/// (power id, move name) a phrase fires for a powerless player.
fn fires(s: &str) -> Option<(String, String)> {
    let m = read(s).move_ref()?;
    let (p, mv) = m.resolve(PowerTable::get())?;
    Some((p.id.clone(), mv.name.clone()))
}

fn is(s: &str, power: &str, mv: &str) {
    assert_eq!(fires(s), Some((power.to_string(), mv.to_string())), "{s:?}");
}

fn shove_kb(m: MoveRef) -> f32 {
    let (_, mv) = m.resolve(PowerTable::get()).unwrap();
    mv.ops.iter().find_map(|o| if let MoveOp::Shove { knockback, .. } = o { Some(*knockback) } else { None }).unwrap()
}

#[test]
fn the_thuum_gets_stronger_with_every_word() {
    let (fus, fus_ro, dah) = (shout("fus"), shout("fus ro"), shout("fus ro dah"));
    assert!(shove_kb(fus) < shove_kb(fus_ro) && shove_kb(fus_ro) < shove_kb(dah), "escalates");
    for heard in ["FUS RO DAH!", "fus roh dah", "foos row da", "fusrodah", "fus-ro-dah", "fus ro da", "unrelenting force"] {
        assert_eq!(shout(heard), dah, "{heard:?}");
    }
    is("dragonborn fus ro dah", "adv_dragonborn", "fus ro dah");
    is("dovahkiin ultimate", "adv_dragonborn", "fus ro dah");
    is("yol toor shul", "adv_dragonborn", "yol toor shul");
    is("tiid klo ul", "adv_dragonborn", "tiid klo ul");
    is("od ah viing", "adv_greybeards", "od ah viing");
}

#[test]
fn kit_phrases_fire_the_characters_move() {
    is("link spin attack", "adv_link", "spin attack");
    is("link hookshot", "adv_link", "hookshot");
    is("link's revali's gale", "adv_link", "revali's gale");
    is("mario ground pound", "adv_mario", "ground pound");
    is("sonic homing attack", "adv_sonic", "homing attack");
    is("sonic spin dash", "adv_sonic", "spin dash");
    is("cloud ultimate", "adv_cloud", "omnislash");
    is("cloud braver", "adv_cloud", "braver");
    is("vergil judgment cut", "adv_vergil", "judgment cut");
    is("dante stinger", "adv_dante", "stinger");
    is("kratos ult", "adv_kratos", "spartan rage");
    is("kratos axe throw", "adv_kratos", "axe throw");
    is("master chief sword lunge", "adv_master_chief", "energy sword lunge");
    is("samus screw attack", "adv_samus", "screw attack");
    is("bayonetta witch time", "adv_bayonetta", "witch time");
    is("sora strike raid", "adv_sora", "strike raid");
    is("sephiroth supernova", "adv_sephiroth", "supernova");
    is("doom slayer glory kill", "adv_doom_slayer", "glory kill");
    is("corvo blink", "adv_corvo", "blink");
    is("chell portal", "adv_portal", "portal");
}

#[test]
fn famous_shouts_borrow_for_anyone_misheard_too() {
    is("judgment cut", "adv_vergil", "judgment cut");
    is("judgement cut end", "adv_vergil", "judgment cut end");
    is("chaos control", "adv_shadow_hedgehog", "chaos control");
    is("witch time", "adv_bayonetta", "witch time");
    is("which time", "adv_bayonetta", "witch time");
    is("omnislash", "adv_cloud", "omnislash");
    is("omni slash", "adv_cloud", "omnislash");
    is("megaflare", "adv_ff_summons", "megaflare");
    is("knights of the round table", "adv_ff_summons", "knights of the round");
    is("estus", "adv_dark_souls", "estus flask");
    is("comet azur", "adv_elden_ring", "comet azur");
    is("waterfall dance", "adv_malenia", "waterfowl dance");
    is("bloodhound's step", "adv_elden_ring", "bloodhound's step");
    is("urbosa's fury", "adv_link", "urbosa's fury");
    is("miphas grace", "adv_link", "mipha's grace");
    is("homing attack", "adv_sonic", "homing attack");
    is("strun bah qo", "adv_dragonborn", "strun bah qo");
    is("fist of havoc", "adv_destiny_titan", "fist of havoc");
    is("nova bomb", "adv_destiny_warlock", "nova bomb");
    is("biotic charge", "adv_biotics", "biotic charge");
    is("spartan rage", "adv_kratos", "spartan rage");
    is("super star", "adv_mario", "super star");
    is("power pellet", "adv_pac_man", "power pellet");
    is("pk starstorm", "adv_earthbound", "pk starstorm");
}

#[test]
fn forms_grant_and_item_names_stay_summons() {
    // Forms are techniques: their names GRANT (the lexicon), never borrow.
    for form in ["super sonic", "hyper sonic", "devil trigger", "sin devil trigger", "triforce", "triforce of courage"] {
        assert_eq!(read(form), Reading::Plain, "{form:?} grants");
        assert!(MoveIndex::shipped().is_grant_name(&norm_words(form)), "{form:?}");
    }
    // Items and characters stay the icon / costume summons (no shout takes them).
    for item in ["hookshot", "master sword", "bfg", "leviathan axe", "energy sword", "gravity gun", "portal gun", "keyblade",
                 "link", "mario", "sonic", "kratos", "cloud strife", "halo warthog", "golden gun", "chaos blades"] {
        assert_eq!(read(item), Reading::Plain, "{item:?} stays a summon");
    }
}

#[test]
fn chains_of_adventure_moves_split_into_steps() {
    let ix = MoveIndex::shipped();
    let t = PowerTable::get();
    let is_move = |s: &str| ix.read(t, None, s).is_move();
    assert_eq!(split_chain("homing attack -> homing attack -> chaos control", &is_move), vec!["homing attack", "homing attack", "chaos control"]);
    assert_eq!(split_chain("revali's gale then mario ground pound", &is_move), vec!["revali's gale", "mario ground pound"]);
    assert_eq!(split_chain("fus ro dah, judgment cut", &is_move), vec!["fus ro dah", "judgment cut"]);
    assert_eq!(split_chain("witch time into omnislash", &is_move), vec!["witch time", "omnislash"]);
}

#[test]
fn a_homing_attack_chain_bounces_from_one_to_the_next() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -10.0);
    w.player_mut(A).unwrap().willpower = 100.0;
    let home = shout("homing attack");
    w.toolbox(A, vec![ToolStep::Move { mv: home, kit: false }, ToolStep::Move { mv: shout("chaos control"), kit: false }], None).unwrap();
    let evs = run(&mut w, ticks(1.2));
    let hits = ops(&evs).iter().filter(|e| matches!(e, OpEvent::Pounce { phase: PouncePhase::Hit, .. })).count();
    assert!(hits >= 1, "the homing attack connected");
    let fired: Vec<MoveRef> = evs.iter().filter_map(|e| if let GameEvent::Power(PowerEvent::Fired { mv, .. }) = e { Some(*mv) } else { None }).collect();
    assert!(fired.contains(&shout("chaos control")), "the chain went on after the lunge: {fired:?}");
    assert!(pct(&w, B) > 0.0);
}

// ------------------------------------------------------------------ headliners, borrowed for real

fn borrow(w: &mut World, s: &str) {
    w.player_mut(A).unwrap().willpower = 100.0;
    w.borrow_move(A, shout(s), false, None).unwrap_or_else(|e| panic!("{s:?}: {e:?}"));
}

#[test]
fn fus_ro_dah_throws_harder_than_fus() {
    let fly = |s: &str| {
        let mut w = duel();
        stand(&mut w, B, 0.0, -4.0);
        borrow(&mut w, s);
        run(&mut w, 2);
        w.player(B).unwrap().mv.vel.length()
    };
    let (fus, dah) = (fly("fus"), fly("fus ro dah"));
    assert!(dah > fus * 2.0, "fus {fus} vs fus ro dah {dah}");
}

#[test]
fn judgment_cut_opens_where_you_look_a_beat_later() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -12.0);
    w.player_mut(A).unwrap().input.pitch = -0.08; // the crosshair on the ground at B's feet
    borrow(&mut w, "judgment cut");
    let marks: Vec<Vec3> = ops(w.events()).iter().filter_map(|e| if let OpEvent::Telegraph { pos, .. } = e { Some(*pos) } else { None }).collect();
    assert!(!marks.is_empty(), "cuts are marked at the aim first");
    let b = centre(&w, B);
    assert!(marks.iter().any(|m| (*m - b).length() < 3.0), "marked around B: {marks:?} vs {b:?}");
    assert_eq!(pct(&w, B), 0.0);
    run(&mut w, ticks(0.6));
    assert!(pct(&w, B) > 0.0, "the spatial cuts landed on B");
}

#[test]
fn omnislash_is_many_blows_then_the_launch() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -5.0);
    borrow(&mut w, "omnislash");
    let mut hits_over_time = 0;
    let mut last = pct(&w, B);
    for _ in 0..ticks(1.8) {
        w.tick(&[]);
        let now = pct(&w, B);
        if now > last + 0.1 {
            hits_over_time += 1;
        }
        last = now;
    }
    assert!(hits_over_time >= 6, "a flurry, not one hit ({hits_over_time})");
}

#[test]
fn farores_wind_sets_a_point_then_takes_you_back() {
    let mut w = duel();
    let home = w.player(A).unwrap().mv.pos;
    borrow(&mut w, "farore's wind");
    run(&mut w, ticks(8.2)); // its cooldown
    w.player_mut(A).unwrap().mv.pos = Vec3::new(20.0, GROUND, 10.0);
    run(&mut w, ticks(0.2));
    borrow(&mut w, "farore's wind");
    run(&mut w, 2);
    let back = w.player(A).unwrap().mv.pos;
    assert!((back - home).length() < 4.0, "warped back to the point ({back:?} vs {home:?})");
}

#[test]
fn a_cut_salvo_strikes_cleanly_without_a_blast() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -8.0);
    let op = MoveOp::Salvo {
        count: 1,
        delay: 0.2,
        interval: 0.0,
        radius: 2.5,
        damage: 10.0,
        knockback: 10.0,
        element: Element::Psychic,
        carve: 0.0,
        up: 0.0,
        at: At::Me,
        dist: 0.0,
        spread: 12.0,
        seek: true,
        track: false,
        cut: true,
    };
    w.fire_op(A, &op).unwrap();
    let evs = run(&mut w, ticks(0.4));
    assert!(pct(&w, B) > 0.0, "the cut landed on B");
    assert!(!evs.iter().any(|e| matches!(e, GameEvent::Explosion { .. })), "no blast, no boom");
    assert!(ops(&evs).iter().any(|e| matches!(e, OpEvent::Strike { cut: true, .. })));
}
