//! Player powers (COMBAT.md §5h): devil fruits + stands through the one power framework.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::{ConstructHit, HitInfo, HitKind};
use bd_sim::events::{GameEvent, KoCause};
use bd_sim::fuzz::{ChaosConfig, ChaosWorld, random_recipe, random_rules};
use bd_sim::powers::{self, MoveOp, PowerEvent, PowerPhrase, PowerTable, power_flags};
use bd_sim::rng::{Fnv64, SimRng};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn flat() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    World::new(cfg)
}

fn duel(n: u8) -> World {
    let mut w = flat();
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

/// Stand `id` on the ground (flat lab: ground top at y = 8) at (x, z), looking down -Z.
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

fn power_of(w: &World, id: PlayerId) -> Option<u16> {
    w.player(id).unwrap().power.id
}

fn eye_aim(w: &World, id: PlayerId, dir: Vec3) -> Aim {
    let p = w.player(id).unwrap();
    Aim::new(p.eye(w.params()), dir.normalize())
}

fn grants(name: &str, key: &str) -> Recipe {
    let mut r = recipe(name, Form::Ball, &[], Element::Light, 0.35);
    r.grants = Some(key.to_string());
    r
}

fn power_events(evs: &[GameEvent]) -> Vec<PowerEvent> {
    evs.iter().filter_map(|e| if let GameEvent::Power(p) = e { Some(p.clone()) } else { None }).collect()
}

// ------------------------------------------------------------------------------------ data

#[test]
fn shipped_table_parses_with_the_full_roster() {
    let t = PowerTable::parse(powers::SHIPPED_RON).expect("assets/powers.ron parses");
    let fruits = t.powers.iter().filter(|p| p.kind.is_fruit()).count();
    let stands = t.powers.len() - fruits;
    assert!(fruits >= 20, "{fruits} fruits");
    assert!(stands >= 12, "{stands} stands");
    assert_eq!(t.powers.len(), PowerTable::get().powers.len(), "the shared table is the shipped one");
    let rules = bd_sim::rules::MatchRules::default();
    for (k, p) in t.powers.iter().enumerate() {
        assert!(t.powers.iter().skip(k + 1).all(|q| q.id != p.id), "duplicate id {}", p.id);
        assert!(!p.names.is_empty(), "{} has names for the lexicon", p.id);
        assert!(!p.moves.is_empty() && p.moves.len() <= 2, "{}: 1–2 moves", p.id);
        assert!(p.shouts.iter().all(|(_, s)| (*s as usize) < p.moves.len()), "{}: shout slot", p.id);
        assert_eq!(p.kind.is_fruit(), p.companion.is_none(), "{}: stands (only) have a companion", p.id);
        if p.kind.is_fruit() {
            assert!(!p.incantations.is_empty(), "{}: fruits have an incantation", p.id);
        }
        for m in &p.moves {
            for op in &m.ops {
                if let MoveOp::Cast { recipe, .. } = op {
                    let r = bd_sim::willpower::validate_recipe(recipe.to_recipe(), &rules);
                    assert!(r.scale.is_finite() && !r.name.is_empty());
                }
                if matches!(op, MoveOp::Transform) {
                    assert!(p.transform.is_some(), "{}: Transform move needs a transform", p.id);
                }
            }
        }
    }
    // Canon: Yami Yami can't scatter; the other Logias can.
    assert!(!t.def(t.index_of("yami_yami").unwrap()).unwrap().body.scatter);
    assert!(t.def(t.index_of("mera_mera").unwrap()).unwrap().body.scatter);
    assert!(t.is_rush_token("ORA") && t.is_rush_token("muda!") && t.is_rush_token("WRYYYY") && !t.is_rush_token("hammer"));
}

#[test]
fn phrase_parsing_contract() {
    let gomu = Some(pid("gomu_gomu"));
    assert_eq!(powers::parse_phrase(gomu, "Gomu Gomu no ANVIL!"), PowerPhrase::Incantation { rest: "anvil".into() });
    assert_eq!(powers::parse_phrase(gomu, "gum-gum giant hammer"), PowerPhrase::Incantation { rest: "giant hammer".into() });
    assert_eq!(powers::parse_phrase(gomu, "gomu gomu no pistol"), PowerPhrase::Move(0));
    assert_eq!(powers::parse_phrase(gomu, "anvil"), PowerPhrase::Plain);
    // Someone else's incantation is just a phrase.
    assert_eq!(powers::parse_phrase(gomu, "mera mera no hammer"), PowerPhrase::Plain);
    assert_eq!(powers::parse_phrase(None, "gomu gomu no anvil"), PowerPhrase::Plain);
    assert_eq!(powers::parse_phrase(Some(pid("the_world")), "ZA WARUDO!!"), PowerPhrase::Move(1));
    assert_eq!(PowerTable::get().id_for_name("Gum-Gum Fruit"), Some("gomu_gomu"));
}

// ------------------------------------------------------------------------------------ fruits

#[test]
fn eating_a_cast_fruit_grants_the_power() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 20.0, 0.0);
    let aim = eye_aim(&w, A, Vec3::new(0.0, -0.5, -1.0));
    let cid = w.cast(A, grants("gum gum fruit", "gomu_gomu"), None, aim).expect("fruit cast");
    let c = w.constructs.get(cid).unwrap();
    assert_eq!(c.recipe.grants.as_deref(), Some("gomu_gomu"));
    assert_eq!(w.powers.fruits.len(), 1);
    assert_eq!(power_of(&w, A), None, "not eaten yet");
    run(&mut w, 60);
    // Walk onto it.
    let at = w.constructs.get(cid).map(|c| c.pos).expect("fruit still there");
    stand_at(&mut w, A, at.x, at.z);
    let evs = run(&mut w, 3);
    assert_eq!(power_of(&w, A), Some(pid("gomu_gomu")));
    assert!(power_events(&evs).iter().any(|e| matches!(e, PowerEvent::Gained { player: A, .. })));
    assert!(w.powers.fruits.is_empty() && w.constructs.get(cid).is_none(), "fruit consumed");
    // Rubber: electric does nothing.
    let hp = w.player(A).unwrap().health;
    let h = HitInfo::new(A, Some(B), HitKind::RingBlast, 20.0, 5.0, Vec3::X, Vec3::ZERO).with_element(Element::Electric);
    w.apply_hit(h);
    assert_eq!(w.player(A).unwrap().health, hp);
    assert_eq!(w.player(A).unwrap().power.snap().power(), Some(pid("gomu_gomu")));
}

#[test]
fn incantation_delivers_the_phrase_on_a_stretching_arm() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    assert!(w.grant_power(A, "gomu_gomu"));
    w.player_mut(A).unwrap().willpower = 100.0;
    let anvil = recipe("anvil", Form::Anvil, &[Mechanic::Heavy], Element::Light, 1.0);
    let aim = eye_aim(&w, A, Vec3::new(0.0, 0.0, -1.0));
    let eye = aim.origin;
    let cid = w.cast_incantation(A, anvil.clone(), None, aim).expect("incanted cast");
    let c = w.constructs.get(cid).unwrap();
    assert!(c.recipe.has(Mechanic::Returning) && c.recipe.has(Mechanic::Bounce), "{:?}", c.recipe.mechanics);
    assert!(c.pos.y < eye.y + 2.0, "from the fist, not 12 m up the sky: {}", c.pos);
    let evs = run(&mut w, 1);
    assert!(power_events(&evs).iter().any(|e| matches!(e, PowerEvent::Delivered { player: A, construct, arm: true, from_fist: true } if *construct == cid)));
    // It is thrown along the look (and not dropped from above).
    let mut min_z = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for _ in 0..60 {
        w.tick(&[]);
        if let Some(c) = w.constructs.get(cid) {
            min_z = min_z.min(c.pos.z);
            max_y = max_y.max(c.pos.y);
        }
    }
    assert!(min_z < -4.0, "launched forward (min z {min_z})");
    assert!(max_y < eye.y + 4.0, "never up in the sky (max y {max_y})");
    // Without the power the same call is just a cast.
    let plain = w.cast_incantation(B, anvil, None, eye_aim(&w, B, Vec3::NEG_Z)).expect("plain");
    assert!(!w.constructs.get(plain).unwrap().recipe.has(Mechanic::Returning));
}

#[test]
fn standing_graft_and_moves() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    assert!(w.grant_power(A, "mera_mera"));
    w.player_mut(A).unwrap().willpower = 100.0;
    let hammer = recipe("hammer", Form::Hammer, &[], Element::Light, 1.2);
    let cid = w.cast(A, hammer, None, eye_aim(&w, A, Vec3::NEG_Z)).unwrap();
    assert_eq!(w.constructs.get(cid).unwrap().recipe.element, Element::Fire, "Mera: everything burns");
    // Gomu rocket: reel toward an anchor.
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    w.grant_power(A, "gomu_gomu");
    w.player_mut(A).unwrap().input.pitch = -0.6;
    w.power_move(A, 1).expect("rocket to the ground ahead");
    assert!(w.player(A).unwrap().mv.vel.length() > 20.0);
    assert_eq!(w.power_move(A, 1), Err(powers::PowerError::Cooldown));
}

#[test]
fn second_fruit_explodes_without_credit() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 3.0, 0.0);
    w.tick(&[]);
    assert!(w.grant_power(A, "gomu_gomu"));
    let hb = w.player(B).unwrap().health;
    assert!(w.grant_power(A, "mera_mera"));
    let evs = run(&mut w, 1);
    assert!(!w.player(A).unwrap().alive, "overdose = self-KO");
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Ko { victim: A, killer: None, cause: KoCause::Damage, .. })));
    let pe = power_events(&evs);
    assert!(pe.iter().any(|e| matches!(e, PowerEvent::Overdose { player: A, .. })));
    assert!(w.player(B).unwrap().health < hb, "big blast");
    assert_eq!(w.player(A).unwrap().kos + w.player(B).unwrap().kos, 0, "no credit");
    // The first fruit comes back where A died.
    assert!(pe.iter().any(|e| matches!(e, PowerEvent::FruitSpawned { respawn: true, power, .. } if *power == pid("gomu_gomu"))));
}

#[test]
fn fruit_respawns_at_the_ko_spot() {
    let mut w = duel(2);
    stand_at(&mut w, A, 10.0, -6.0);
    stand_at(&mut w, B, -20.0, 0.0);
    run(&mut w, 5);
    assert!(w.grant_power(A, "hie_hie"));
    w.player_mut(A).unwrap().power.scatter_cd = 10.0; // no dodge for this test
    let spot = w.player(A).unwrap().mv.pos;
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 500.0, 0.0, Vec3::X, spot));
    assert!(!w.player(A).unwrap().alive);
    assert_eq!(power_of(&w, A), None, "power lost at KO");
    assert_eq!(w.powers.fruits.len(), 1);
    let f = w.powers.fruits[0];
    assert_eq!(f.power, pid("hie_hie"));
    assert!((f.last_pos - spot).length() < 1.5, "respawned where A died: {} vs {spot}", f.last_pos);
    // B walks over and takes it.
    run(&mut w, 60);
    let at = w.constructs.get(w.powers.fruits[0].construct).unwrap().pos;
    stand_at(&mut w, B, at.x, at.z);
    run(&mut w, 3);
    assert_eq!(power_of(&w, B), Some(pid("hie_hie")));
}

#[test]
fn water_drains_a_fruit_for_three_seconds() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    assert!(w.grant_power(A, "mera_mera"));
    let splash = HitInfo::new(A, Some(B), HitKind::Thrown, 5.0, 2.0, Vec3::X, Vec3::ZERO).with_element(Element::Water);
    w.apply_hit(splash);
    let p = w.player(A).unwrap();
    assert!(p.power.drained > 0.0 && !p.power.active());
    assert!(p.health < 100.0, "water is never scattered");
    assert!(p.power.snap().has(power_flags::DRAINED));
    // Drained: fire now burns the fire man and he can't scatter.
    let hp = w.player(A).unwrap().health;
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 10.0, 2.0, Vec3::X, Vec3::ZERO).with_element(Element::Fire));
    assert!(w.player(A).unwrap().health < hp);
    assert_eq!(w.power_move(A, 0), Err(powers::PowerError::Drained));
    run(&mut w, (3.1 * 120.0) as u32);
    assert!(w.player(A).unwrap().power.active(), "back after 3 s");
}

#[test]
fn seastone_link_disables_while_linked() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, 3.0);
    assert!(w.grant_power(A, "gomu_gomu"));
    w.player_mut(B).unwrap().willpower = 100.0;
    let chain = recipe("seastone chain", Form::Whip, &[Mechanic::Link], Element::Light, 1.5);
    let cid = w.cast(B, chain, None, eye_aim(&w, B, Vec3::NEG_Z)).unwrap();
    let pa = w.player(A).unwrap().hull(w.params()).center();
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::Melee, 2.0, 1.0, Vec3::NEG_Z, pa).with_construct(cid));
    assert!(!w.links.is_empty(), "the chain linked A");
    let evs = run(&mut w, 2);
    let p = w.player(A).unwrap();
    assert!(p.power.seastone && !p.power.active() && p.power.drained == 0.0);
    assert!(power_events(&evs).iter().any(|e| matches!(e, PowerEvent::Seastone { player: A, on: true })));
    // An ordinary (non-seastone) link does nothing to powers.
    w.links.clear();
    run(&mut w, 2);
    assert!(w.player(A).unwrap().power.active(), "free again once unlinked");
}

#[test]
fn logia_scatter_dodges_then_cools_down() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    assert!(w.grant_power(A, "goro_goro"));
    let before = w.player(A).unwrap().mv.pos;
    let r = w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 20.0, 9.0, Vec3::X, before));
    assert!(!r.applied);
    let p = w.player(A).unwrap();
    assert_eq!(p.health, 100.0);
    let moved = (p.mv.pos - before).length();
    assert!(moved > 0.5 && moved < 1.6, "reformed ~1 m away ({moved})");
    let evs = run(&mut w, 1);
    assert!(power_events(&evs).iter().any(|e| matches!(e, PowerEvent::Scattered { player: A, element: Element::Electric, .. })));
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 20.0, 9.0, Vec3::X, before));
    assert!(w.player(A).unwrap().health < 100.0, "3 s cooldown");
    // Yami Yami can't.
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    w.grant_power(A, "yami_yami");
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 20.0, 9.0, Vec3::X, before));
    assert!(w.player(A).unwrap().health < 100.0);
}

// ------------------------------------------------------------------------------------ stands

#[test]
fn stand_follows_and_mirrors_damage_to_its_user() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 20.0, 20.0);
    assert!(w.grant_power(A, "star_platinum"));
    let sid = w.player(A).unwrap().power.companion.expect("stand out");
    run(&mut w, 60);
    stand_at(&mut w, A, 10.0, -10.0);
    run(&mut w, 60);
    let s = w.constructs.get(sid).expect("stand persists");
    let a = w.player(A).unwrap().mv.pos;
    assert!((s.pos - a).length() < 2.0, "floats behind its user: {} vs {a}", s.pos);
    assert!(s.pos.z > a.z, "behind (user looks down -Z)");
    assert!(w.constructs.in_slot(A, bd_sim::constructs::Slot::Creature).count() == 0, "a stand takes no creature slot");
    let hp = w.player(A).unwrap().health;
    w.apply_construct_hit(ConstructHit { target: sid, source: Some(B), damage: 20.0, impulse: Vec3::X, blade: false, point: s.pos });
    run(&mut w, 1);
    let p = w.player(A).unwrap();
    assert!((hp - p.health - 20.0).abs() < 1.0, "stand damage hurts the user ({} → {})", hp, p.health);
    assert_eq!(p.last_attacker.map(|a| a.0), Some(B));
    let s = w.constructs.get(sid).expect("the stand itself never breaks");
    assert_eq!(s.integrity, s.max_integrity);
    // A second stand replaces the first.
    assert!(w.grant_power(A, "the_world"));
    run(&mut w, 1);
    assert!(w.constructs.get(sid).is_none());
    assert_eq!(power_of(&w, A), Some(pid("the_world")));
}

#[test]
fn ora_tokens_are_rush_punches() {
    let mut w = duel(2);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, -1.8);
    assert!(w.grant_power(A, "star_platinum"));
    run(&mut w, 80);
    stand_at(&mut w, A, 0.0, 0.0);
    stand_at(&mut w, B, 0.0, -1.8);
    // Console closed: nothing.
    assert_eq!(w.stand_rush(A, 3), 0);
    let typing = PlayerInput { buttons: buttons::TYPING, ..Default::default() };
    w.tick(&[(A, typing)]);
    let hp = w.player(B).unwrap().health;
    assert!(w.stand_rush_token(A));
    assert_eq!(w.stand_rush(A, 4), 4, "typed faster than the rush rate: queued");
    let mut rushes = 0;
    let mut hits = 0;
    for _ in 0..60 {
        for e in w.tick(&[(A, typing)]) {
            if let GameEvent::Power(PowerEvent::StandRush { player: A, hits: h, .. }) = e {
                rushes += 1;
                hits += *h as u32;
            }
        }
    }
    assert_eq!(rushes, 5, "one punch per token");
    assert!(hits >= 2, "the stand hit B ({hits})");
    assert!(w.player(B).unwrap().health < hp);
}

#[test]
fn the_world_greys_and_speeds_only_its_user() {
    let mut w = duel(2);
    stand_at(&mut w, A, -10.0, 20.0);
    stand_at(&mut w, B, 10.0, 20.0);
    assert!(w.grant_power(A, "the_world"));
    w.player_mut(A).unwrap().willpower = 100.0;
    run(&mut w, 30);
    w.power_move(A, 1).expect("za warudo");
    let evs = run(&mut w, 1);
    assert!(power_events(&evs).iter().any(|e| matches!(e, PowerEvent::TimeStop { player: A, .. })));
    assert!(w.player(A).unwrap().power.snap().has(power_flags::TIME_STOP));
    assert!(!w.player(B).unwrap().power.snap().has(power_flags::TIME_STOP));
    let (a0, b0) = (w.player(A).unwrap().mv.pos, w.player(B).unwrap().mv.pos);
    let fwd = PlayerInput { move_z: 1, ..Default::default() };
    for _ in 0..90 {
        w.tick(&[(A, fwd), (B, fwd)]);
    }
    let da = (w.player(A).unwrap().mv.pos - a0).length();
    let db = (w.player(B).unwrap().mv.pos - b0).length();
    assert!(db > 2.0, "B still moves (never frozen): {db}");
    assert!(da > db * 1.6, "user ×2: {da} vs {db}");
    run(&mut w, 120);
    assert!(!w.player(A).unwrap().power.snap().has(power_flags::TIME_STOP), "1.5 s");
}

fn crater_and_restore(seed: u64) -> (u64, u64, u64, usize) {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.add_player(A, "a".into(), Corps::Willpower).unwrap();
    w.start_live();
    w.tick(&[]);
    stand_at(&mut w, A, 0.0, 0.0);
    assert!(w.grant_power(A, "crazy_diamond"));
    w.player_mut(A).unwrap().willpower = 100.0;
    run(&mut w, 10);
    let before = w.voxels.content_hash();
    w.request_carve(bd_sim::combat::CarveRequest { center: Vec3::new(4.0, 8.0, 0.0), radius: 2.0, power: 1.0 });
    run(&mut w, 5);
    let carved = w.voxels.content_hash();
    w.power_move(A, 0).expect("restore");
    let evs = run(&mut w, 1);
    let restored = power_events(&evs)
        .iter()
        .find_map(|e| if let PowerEvent::Restored { voxels, .. } = e { Some(voxels.len()) } else { None })
        .expect("Restored event");
    (before, carved, w.voxels.content_hash(), restored)
}

#[test]
fn crazy_diamond_restores_terrain_deterministically() {
    let a = crater_and_restore(3);
    let b = crater_and_restore(3);
    assert_ne!(a.0, a.1, "the carve removed terrain");
    assert!(a.3 > 10, "{} voxels rebuilt", a.3);
    assert_eq!(a.2, a.0, "exactly the original terrain is back");
    assert_eq!(a, b, "deterministic");
    // A client mirror applying the event gets the same terrain.
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 3, ..SimConfig::default() };
    let mut host = World::new(cfg.clone());
    let mut client = World::new(cfg);
    host.add_player(A, "a".into(), Corps::Willpower).unwrap();
    host.start_live();
    host.tick(&[]);
    stand_at(&mut host, A, 0.0, 0.0);
    host.grant_power(A, "crazy_diamond");
    host.request_carve(bd_sim::combat::CarveRequest { center: Vec3::new(4.0, 8.0, 0.0), radius: 2.0, power: 1.0 });
    let mut evs = run(&mut host, 5);
    host.power_move(A, 0).unwrap();
    evs.extend(run(&mut host, 1));
    for e in &evs {
        client.apply_event(e);
    }
    assert_eq!(client.voxels.content_hash(), host.voxels.content_hash());
}

// ------------------------------------------------------------------------------------ toggles

#[test]
fn powers_off_means_fruits_are_just_constructs() {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    cfg.rules.powers_enabled = false;
    let mut w = World::new(cfg);
    w.add_player(A, "a".into(), Corps::Willpower).unwrap();
    w.start_live();
    w.tick(&[]);
    assert!(!w.grant_power(A, "gomu_gomu"));
    let cid = w.cast(A, grants("gum gum fruit", "gomu_gomu"), None, eye_aim(&w, A, Vec3::NEG_Z)).expect("never says no");
    assert!(w.constructs.get(cid).is_some());
    assert!(w.powers.fruits.is_empty());
    // Stands off only.
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    cfg.rules.stands = false;
    let mut w = World::new(cfg);
    w.add_player(A, "a".into(), Corps::Willpower).unwrap();
    w.start_live();
    w.tick(&[]);
    assert!(!w.grant_power(A, "star_platinum"));
    assert!(w.grant_power(A, "gomu_gomu"));
}

#[test]
fn random_fruit_and_stand_arrow_pick_deterministically() {
    let pick = |seed: u64| {
        let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed, ..SimConfig::default() };
        let mut w = World::new(cfg);
        w.add_player(A, "a".into(), Corps::Willpower).unwrap();
        w.add_player(B, "b".into(), Corps::Rage).unwrap();
        w.start_live();
        w.tick(&[]);
        w.player_mut(A).unwrap().willpower = 100.0;
        w.cast(A, grants("stand arrow", powers::RANDOM_STAND), None, eye_aim(&w, A, Vec3::NEG_Z)).expect("arrow");
        let fruit = w.cast(B, grants("devil fruit", powers::RANDOM_FRUIT), None, eye_aim(&w, B, Vec3::NEG_Z)).expect("fruit");
        let key = w.constructs.get(fruit).unwrap().recipe.grants.clone();
        (power_of(&w, A), key)
    };
    let (a, fruit) = pick(9);
    let def = PowerTable::get().def(a.expect("a stand")).unwrap();
    assert!(!def.kind.is_fruit());
    let fk = fruit.expect("a fruit key");
    assert!(PowerTable::get().def(PowerTable::get().index_of(&fk).unwrap()).unwrap().kind.is_fruit());
    assert_eq!(pick(9), pick(9));
}

// ------------------------------------------------------------------------------------ chaos

fn chaos_powers(seed: u64, ticks: u32) -> (u64, u64, u64, usize) {
    let rules = random_rules(&mut SimRng::new(seed));
    let mut cfg = ChaosConfig::new(seed, rules);
    cfg.arena = ArenaSource::Flat { size_m: 96.0 };
    cfg.cast_rate = 1.5;
    let mut cw = ChaosWorld::with_config(cfg);
    let mut rng = SimRng::new(seed ^ 0x9E37_79B9);
    let t = PowerTable::get();
    let mut h = Fnv64::default();
    let mut power_events_seen = 0usize;
    for _ in 0..ticks {
        let ids: Vec<PlayerId> = cw.world.players.iter().filter(|p| p.alive).map(|p| p.id).collect();
        for id in ids {
            let r = rng.f32();
            if r < 0.004 {
                let k = rng.below(t.powers.len() as u32) as u16;
                cw.world.grant_power(id, &t.def(k).unwrap().id.clone());
            } else if r < 0.006 {
                let key = if rng.chance(0.5) { powers::RANDOM_FRUIT } else { powers::RANDOM_STAND };
                let aim = { let p = cw.world.player(id).unwrap(); Aim::new(p.eye(cw.world.params()), p.look()) };
                let _ = cw.world.cast(id, grants("mystery", key), None, aim);
            } else if r < 0.03 {
                let _ = cw.world.power_move(id, rng.below(2) as u8);
            } else if r < 0.036 {
                let aim = { let p = cw.world.player(id).unwrap(); Aim::new(p.eye(cw.world.params()), p.look()) };
                let rec = random_recipe(&mut rng, 1);
                let _ = cw.world.cast_incantation(id, rec, None, aim);
            } else if r < 0.05 {
                cw.world.stand_rush(id, 1 + rng.below(4) as u16);
            }
        }
        let rep = cw.step();
        assert!(rep.invariant_error.is_none(), "seed {seed} tick {}: {:?}", rep.tick, rep.invariant_error);
        for e in cw.world.events() {
            if let GameEvent::Power(pe) = e {
                power_events_seen += 1;
                h.write(format!("{pe:?}").as_bytes());
            }
        }
    }
    (cw.world.state_hash(), cw.world.voxels.content_hash(), h.0, power_events_seen)
}

#[test]
fn chaos_soak_with_random_powers_holds_invariants_and_is_deterministic() {
    for seed in [21u64, 22] {
        let a = chaos_powers(seed, 2400);
        let b = chaos_powers(seed, 2400);
        assert!(a.3 > 20, "powers were actually used ({} events)", a.3);
        assert_eq!(a, b, "seed {seed}");
    }
}
