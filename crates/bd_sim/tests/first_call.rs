//! FIRST CALL + BURN ON DEATH (`bd_sim::first_call`): claim keys (a character and all their moves
//! share one; generic things never claim), claims / rival versions (never refused, never pricier),
//! same-tick ties, claims across KOs, leaving, burns on KO (not before), echoes (never refused,
//! cheaper), the scope knob, the data files, and the wire round-trip.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, ModelSource, Recipe};
use bd_core::{PlayerId, TICK_HZ, Vec3};
use bd_sim::MatchRules;
use bd_sim::events::GameEvent;
use bd_sim::first_call::keys::{CallKind, Identities, composed_key, thing_key};
use bd_sim::first_call::{BurnScope, CallData, CallEvent, CallSnap, ClaimRow, mark};
use bd_sim::powers::chain::{self, ToolStep};
use bd_sim::powers::ops::OpEvent;
use bd_sim::powers::toolbox::{MoveIndex, MoveRef};
use bd_sim::powers::twist::ModTable;
use bd_sim::powers::{PowerEvent, PowerTable, norm_words};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn world(first_call: bool, burn: BurnScope) -> World {
    let mut rules = MatchRules::default();
    rules.first_call = first_call;
    rules.burn = burn;
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, rules, ..SimConfig::default() };
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
    stand(&mut w, B, 20.0, 0.0);
    w
}

fn stand(w: &mut World, id: PlayerId, x: f32, z: f32) {
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

fn secs(s: f32) -> u32 {
    (s * TICK_HZ as f32).ceil() as u32
}

fn shout(s: &str) -> MoveRef {
    MoveIndex::shipped().shout(&norm_words(s)).unwrap_or_else(|| panic!("no shout {s:?}"))
}

fn kit(s: &str) -> MoveRef {
    MoveIndex::shipped().kit_move(PowerTable::get(), &norm_words(s)).unwrap_or_else(|| panic!("no kit move {s:?}")).0
}

fn power(id: &str) -> u16 {
    PowerTable::get().index_of(id).unwrap_or_else(|| panic!("no power {id}"))
}

fn full(w: &mut World, id: PlayerId) {
    let max = w.cfg.rules.willpower_max();
    w.player_mut(id).unwrap().willpower = max;
}

fn wp(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().willpower
}

fn calls(evs: &[GameEvent]) -> Vec<CallEvent> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::FirstCall(c) => Some(*c),
            _ => None,
        })
        .collect()
}

fn twisted_mods(evs: &[GameEvent], who: PlayerId) -> Option<Vec<u8>> {
    let mut fired_by = None;
    for e in evs {
        match e {
            GameEvent::Power(PowerEvent::Op(OpEvent::Twisted { player, mods, .. })) if *player == who => {
                fired_by = Some(mods.iter().copied().filter(|m| *m != 0).collect());
            }
            _ => {}
        }
    }
    fired_by
}

fn byte(id: &str) -> u8 {
    ModTable::shipped().id_of(id).unwrap_or_else(|| panic!("modifier {id}"))
}

/// Fire a borrowed / kit move for `who` along +x / -x (toward the other player) and return the
/// willpower it cost and the events of the next tick.
fn fire(w: &mut World, who: PlayerId, mv: MoveRef, is_kit: bool) -> (f32, Vec<GameEvent>) {
    full(w, who);
    let before = wp(w, who);
    w.toolbox(who, vec![ToolStep::Move { mv, kit: is_kit }], None).unwrap_or_else(|e| panic!("{who:?} {mv:?}: {e:?}"));
    let after = wp(w, who);
    (before - after, run(w, 1))
}

fn icon(id: &str, form: Form) -> Recipe {
    let mut r = Recipe::unknown(id);
    r.form = form;
    r.scale = 2.0;
    r.model = ModelSource::Icon(id.to_string());
    r.normalized()
}

fn aim(w: &World, who: PlayerId) -> Aim {
    let p = w.player(who).unwrap();
    Aim::new(p.mv.pos + Vec3::Y * 1.6, Vec3::new(0.0, -0.2, -1.0).normalize())
}

fn void_ko(w: &mut World, who: PlayerId) -> Vec<GameEvent> {
    w.player_mut(who).unwrap().mv.pos.y = -500.0;
    run(w, 3)
}

// ------------------------------------------------------------------------------ data + keys

#[test]
fn data_parses_and_every_substitution_modifier_exists() {
    let d = CallData::parse(bd_sim::first_call::DATA_RON).expect("first_call.ron parses");
    let t = ModTable::shipped();
    for m in d.rival.mods.iter().chain(d.echo.mods.iter()) {
        let n = t.id_of(m).unwrap_or_else(|| panic!("modifier {m} missing from move_modifiers.ron"));
        assert!(t.get(n).unwrap().words.is_empty(), "{m} must be wordless (never read from a phrase)");
    }
    for e in [
        Element::Light,
        Element::Fire,
        Element::Electric,
        Element::Ice,
        Element::Water,
        Element::Wind,
        Element::Earth,
        Element::Poison,
        Element::Psychic,
        Element::Shadow,
        Element::Atomic,
    ] {
        let (_, m) = d.rival.opposite.iter().find(|(x, _)| *x == e).unwrap_or_else(|| panic!("no opposite for {e:?}"));
        assert!(t.id_of(m).is_some(), "opposite modifier {m}");
        assert_ne!(d.rival_element(e), e, "{e:?} must shift");
    }
    // the rival modifier never changes the price tier; the echo is a tier cheaper
    let rival = t.get(byte("rival")).unwrap();
    assert_eq!((rival.tier, rival.sparks), (0, 0));
    assert!((rival.cooldown - 1.0).abs() < 1e-6);
    let echo = t.get(byte("echo")).unwrap();
    assert!(echo.tier < 0 && echo.damage < 1.0 && (echo.cooldown - 1.0).abs() < 1e-6);
}

/// The generated named-icon file matches the icon files (tools/first_call/first_call_icons.py).
#[test]
fn named_icon_file_is_in_sync_with_the_icon_files() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut files = vec![root.join("icons.ron")];
    let mut more: Vec<_> = std::fs::read_dir(root.join("icons"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ron"))
        .collect();
    more.sort();
    files.extend(more);
    let quoted = |s: &str, key: &str| -> Option<String> {
        let at = s.find(key)?;
        let rest = &s[at + key.len()..];
        let open = rest.find('"')?;
        let rest = &rest[open + 1..];
        Some(rest[..rest.find('"')?].to_string())
    };
    let mut names = 0usize;
    let rows: Vec<(String, String, String, u32)> = ron::from_str(bd_sim::first_call::ICONS_RON).expect("first_call_icons.ron parses");
    let listed: std::collections::HashSet<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap();
        for part in src.split("IconEntry(").skip(1) {
            let Some(n) = quoted(part, "names:") else { continue };
            let n = n.trim().to_lowercase();
            if listed.contains(n.as_str()) {
                names += 1;
            }
        }
    }
    assert!(rows.len() > 1500, "{} named icons", rows.len());
    assert!(names >= rows.len(), "every listed icon exists in the icon files ({names} found of {})", rows.len());
    for g in ["giant fist", "dragon", "slime", "fireball", "power battery"] {
        assert!(!listed.contains(g), "{g} is generic vocabulary, never claimable");
    }
    for n in ["mjolnir", "optimus prime", "godzilla", "falcon punch", "excalibur"] {
        assert!(listed.contains(n), "{n} is a named icon");
    }
}

#[test]
fn a_character_and_all_their_moves_share_one_key() {
    let ids = Identities::shipped();
    let optimus = ids.of_power(power("kit_optimus_prime")).expect("the kit has an identity");
    assert_eq!(optimus.kind, CallKind::Character);
    // the bare-name summon: a transformer icon and the costume summon
    let bot = icon("optimus_prime", Form::Biped);
    assert_eq!(ids.of_recipe(&bot, false).map(|i| i.key), Some(optimus.key), "the transformer icon");
    let mut costume = Recipe::unknown("optimus prime");
    costume.form = Form::Biped;
    costume.model = ModelSource::Icon("costume:optimus prime".into());
    assert_eq!(ids.of_recipe(&costume, false).map(|i| i.key), Some(optimus.key), "the costume summon");
    let mut giant = costume.clone();
    giant.model = ModelSource::Icon("costume:giant optimus prime".into());
    assert_eq!(ids.of_recipe(&giant, false).map(|i| i.key), Some(optimus.key), "a scaled summon");
    // every move of the kit: "optimus prime combo", the borrowed "ion blaster"
    assert_eq!(ids.of_move(kit("optimus prime combo")).map(|i| i.key), Some(optimus.key));
    assert_eq!(ids.of_move(shout("ion blaster")).map(|i| i.key), Some(optimus.key));
    for slot in 0..8u8 {
        if MoveRef::new(power("kit_optimus_prime"), 0, slot).resolve(PowerTable::get()).is_some() {
            assert_eq!(ids.of_move(MoveRef::new(power("kit_optimus_prime"), 0, slot)).map(|i| i.key), Some(optimus.key));
        }
    }
    // other characters are other keys
    let primal = ids.of_power(power("kit_optimus_primal")).unwrap();
    assert_ne!(primal.key, optimus.key);
    let rein = ids.of_power(power("ow_reinhardt")).unwrap();
    assert_eq!(ids.of_move(shout("earthshatter")).map(|i| i.key), Some(rein.key));
    let mut rc = Recipe::unknown("reinhardt");
    rc.form = Form::Biped;
    rc.model = ModelSource::Icon("costume:reinhardt".into());
    assert_eq!(ids.of_recipe(&rc, false).map(|i| i.key), Some(rein.key), "Reinhardt's summon is Reinhardt");
    // a power's grant (a fruit construct / a technique phrase) is that power
    let mut fruit = Recipe::unknown("gum gum fruit");
    fruit.grants = Some(PowerTable::get().powers[power("gomu_gomu") as usize].id.clone());
    assert_eq!(ids.of_recipe(&fruit, false), ids.of_power(power("gomu_gomu")));
    // a famous move's icon / signature cast is its character's (if their kit shouts it)
    for (mv, name) in [("kamehameha", "kamehameha"), ("rasengan", "rasengan")] {
        if let Some(m) = MoveIndex::shipped().shout(mv) {
            assert_eq!(ids.of_name(name).map(|i| i.key), ids.of_move(m).map(|i| i.key), "{name}");
        }
    }
    let (p, c, i, s, n) = ids.counts();
    eprintln!("identities: {p} powers, {c} characters, {i} icon ids, {s} icon seeds, {n} named casts");
    assert!(p > 500 && c > 100 && i > 500 && s > 500);
}

#[test]
fn generic_things_never_claim() {
    let ids = Identities::shipped();
    let mut dragon = Recipe::unknown("fire dragon");
    dragon.form = Form::Quadruped;
    dragon.model = ModelSource::Procedural { seed: 0xDEAD_BEEF };
    assert_eq!(ids.of_recipe(&dragon, false), None, "a described dragon");
    assert_eq!(ids.of_recipe(&icon("dragon", Form::Quadruped), false), None, "the dragon icon is still a dragon");
    assert_eq!(ids.of_recipe(&icon("giant_fist", Form::Gauntlet), false), None, "the lantern's own fist");
    assert_eq!(ids.of_recipe(&Recipe::unknown("sword"), false), None);
    // named icons do claim
    assert!(ids.of_recipe(&icon("mjolnir", Form::Hammer), false).is_some());
    // BURN everything: lenient keys — "a dragon" and "fire dragon" are one dragon; composed by verb + style
    let mut plain = dragon.clone();
    plain.name = "dragon".into();
    plain.element = Element::Light;
    assert_eq!(thing_key(&plain), thing_key(&dragon));
    let mut other = dragon.clone();
    other.model = ModelSource::Procedural { seed: 7 };
    assert_ne!(thing_key(&other), thing_key(&dragon));
    let a = bd_sim::powers::compose::ComposedMove { verb: 3, style: 2, ..Default::default() };
    let b = bd_sim::powers::compose::ComposedMove { verb: 3, style: 2, count: 3, mods: [1, 0, 0, 0], ..Default::default() };
    assert_eq!(composed_key(&a), composed_key(&b));
}

// ------------------------------------------------------------------------------ FIRST CALL

#[test]
fn off_by_default_and_nothing_changes_while_off() {
    assert!(!MatchRules::default().first_call);
    assert_eq!(MatchRules::default().burn, BurnScope::Off);
    let mut w = world(false, BurnScope::Off);
    let m = shout("earthshatter");
    let (_, ev) = fire(&mut w, A, m, false);
    assert!(calls(&ev).is_empty());
    assert!(w.first_call.is_empty());
    assert!(w.snapshot_for(Some(A)).first_call.is_none(), "no replicated state while off");
    let (_, ev) = fire(&mut w, B, m, false);
    assert!(twisted_mods(&ev, B).is_none(), "no rival while off");
}

#[test]
fn first_caller_owns_it_and_the_rival_gets_the_rival_version_at_the_same_price() {
    let mut w = world(true, BurnScope::Off);
    let m = shout("earthshatter");
    let key = Identities::shipped().of_move(m).unwrap().key;
    let (paid_a, ev) = fire(&mut w, A, m, false);
    assert!(calls(&ev).contains(&CallEvent::Claimed { player: A, key, kind: CallKind::Character }), "{:?}", calls(&ev));
    assert_eq!(w.first_call.snap.owner_of(key), Some(A));
    assert!(twisted_mods(&ev, A).is_none(), "the owner gets the original");
    // B calls it: never refused, the rival version, the same price
    let (paid_b, ev) = fire(&mut w, B, m, false);
    let c = calls(&ev);
    assert!(
        c.iter().any(|e| matches!(e, CallEvent::Rival { player, owner, key: k, .. } if *player == B && *owner == A && *k == key)),
        "{c:?}"
    );
    let mods = twisted_mods(&ev, B).expect("B's move is twisted");
    assert!(mods.contains(&byte("rival")), "{mods:?}");
    assert!(mods.len() >= 2, "the opposite element rides with it: {mods:?}");
    assert!(paid_b > 0.0 && (paid_b - paid_a).abs() < 1e-3, "rival price {paid_b} vs original {paid_a}");
    assert_eq!(w.first_call.snap.owner_of(key), Some(A), "still A's");
    // a kit move of the same character is the same claim: B's "reinhardt charge" is a rival too
    run(&mut w, secs(1.0));
    let (_, ev) = fire(&mut w, B, kit("reinhardt charge"), true);
    assert!(twisted_mods(&ev, B).is_some_and(|m| m.contains(&byte("rival"))));
    // the replicated state says so
    let snap = w.snapshot_for(Some(B)).first_call.expect("replicated while on");
    assert_eq!(snap.owner_of(key), Some(A));
    assert_eq!(snap.verdict(B, key, true, false).rival, Some(A));
    assert!(snap.verdict(A, key, true, false).plain());
}

#[test]
fn a_character_summon_is_claimed_and_the_rival_gets_the_evil_twin() {
    let mut w = world(true, BurnScope::Off);
    let mut costume = Recipe::unknown("optimus prime");
    costume.form = Form::Biped;
    costume.scale = 2.0;
    costume.element = Element::Fire;
    costume.model = ModelSource::Icon("costume:optimus prime".into());
    full(&mut w, A);
    let a = aim(&w, A);
    w.cast(A, costume.clone(), None, a).expect("A summons Optimus");
    let ev = run(&mut w, 1);
    let key = Identities::shipped().of_power(power("kit_optimus_prime")).unwrap().key;
    assert_eq!(w.first_call.snap.owner_of(key), Some(A), "{:?}", calls(&ev));
    // B: never refused, the evil twin: dressed dark, the opposite element, a rival mark, same price
    full(&mut w, B);
    let before = wp(&w, B);
    let a = aim(&w, B);
    let cid = w.cast(B, costume.clone(), None, a).expect("B is never refused");
    let paid_b = before - wp(&w, B);
    let ev = run(&mut w, 1);
    let c = w.constructs.get(cid).expect("B's rival Optimus");
    assert_eq!(c.recipe.element, CallData::shipped().rival_element(Element::Fire));
    assert!(matches!(&c.recipe.model, ModelSource::Icon(id) if id == "costume:black optimus prime"), "{:?}", c.recipe.model);
    assert!(calls(&ev).iter().any(|e| matches!(e, CallEvent::Rival { player, owner, .. } if *player == B && *owner == A)));
    assert_eq!(w.first_call.snap.mark_of(cid) & mark::RIVAL, mark::RIVAL);
    // the same cast by a fresh third party in an OFF world costs the same
    let mut off = world(false, BurnScope::Off);
    full(&mut off, B);
    let before = wp(&off, B);
    let a = aim(&off, B);
    off.cast(B, costume, None, a).unwrap();
    assert!((paid_b - (before - wp(&off, B))).abs() < 1e-3, "the rival never costs more");
    // and it is still the same kit: B's "optimus prime combo" is a rival move
    let (_, ev) = fire(&mut w, B, kit("optimus prime combo"), true);
    assert!(twisted_mods(&ev, B).is_some_and(|m| m.contains(&byte("rival"))));
}

#[test]
fn same_tick_ties_go_to_the_lower_player() {
    let mut w = world(true, BurnScope::Off);
    let m = shout("earthshatter");
    let key = Identities::shipped().of_move(m).unwrap().key;
    full(&mut w, A);
    full(&mut w, B);
    // B's command is applied first, then A's — the same tick
    w.toolbox(B, vec![ToolStep::Move { mv: m, kit: false }], None).unwrap();
    w.toolbox(A, vec![ToolStep::Move { mv: m, kit: false }], None).unwrap();
    let ev = run(&mut w, 1);
    assert_eq!(w.first_call.snap.owner_of(key), Some(A), "the lower id wins the tie");
    let c = calls(&ev);
    assert!(c.iter().any(|e| matches!(e, CallEvent::Rival { player, owner, .. } if *player == B && *owner == A)), "{c:?}");
    // the next tick it is an ordinary claim: B stays the rival
    run(&mut w, secs(20.0));
    let (_, ev) = fire(&mut w, B, m, false);
    assert!(twisted_mods(&ev, B).is_some_and(|m| m.contains(&byte("rival"))));
}

#[test]
fn claims_survive_kos_and_leaving_frees_them() {
    let mut w = world(true, BurnScope::Off);
    let m = shout("earthshatter");
    let key = Identities::shipped().of_move(m).unwrap().key;
    fire(&mut w, A, m, false);
    void_ko(&mut w, A);
    run(&mut w, secs(4.0));
    assert!(w.player(A).unwrap().alive, "respawned");
    assert_eq!(w.first_call.snap.owner_of(key), Some(A), "a KO never releases a claim");
    // A (still the owner) gets the original after the KO
    run(&mut w, secs(20.0));
    let (_, ev) = fire(&mut w, A, m, false);
    assert!(twisted_mods(&ev, A).is_none());
    // A leaves: it's free; B calls it and owns it
    w.remove_player(A);
    run(&mut w, secs(20.0));
    assert_eq!(w.first_call.snap.owner_of(key), None);
    let (_, ev) = fire(&mut w, B, m, false);
    assert!(twisted_mods(&ev, B).is_none());
    assert_eq!(w.first_call.snap.owner_of(key), Some(B));
}

#[test]
fn holding_a_claimed_power_fires_rival_moves() {
    let mut w = world(true, BurnScope::Off);
    let p = power("gomu_gomu");
    let key = Identities::shipped().of_power(p).unwrap().key;
    assert!(w.grant_power(A, "gomu_gomu"));
    run(&mut w, 1);
    assert_eq!(w.first_call.snap.owner_of(key), Some(A));
    assert!(w.grant_power(B, "gomu_gomu"), "B still gets the fruit");
    let ev = run(&mut w, 1);
    assert!(calls(&ev).iter().any(|e| matches!(e, CallEvent::Rival { player, owner, .. } if *player == B && *owner == A)));
    // B's G tap: the rival version, never refused
    full(&mut w, B);
    let r = w.power_move(B, 0);
    let ev = run(&mut w, 1);
    assert!(r.is_ok(), "{r:?}");
    assert!(twisted_mods(&ev, B).is_some_and(|m| m.contains(&byte("rival"))), "{:?}", twisted_mods(&ev, B));
    // A's is the original
    full(&mut w, A);
    assert!(w.power_move(A, 0).is_ok());
    let ev = run(&mut w, 1);
    assert!(twisted_mods(&ev, A).is_none());
}

// ------------------------------------------------------------------------------ BURN ON DEATH

#[test]
fn burns_on_ko_not_before_and_the_echo_is_never_refused_and_cheaper() {
    let mut w = world(false, BurnScope::Named);
    let m = shout("earthshatter");
    let key = Identities::shipped().of_move(m).unwrap().key;
    let (paid, ev) = fire(&mut w, A, m, false);
    assert!(calls(&ev).iter().any(|e| matches!(e, CallEvent::Fresh { player, .. } if *player == A)), "the fresh spark");
    assert!(!w.first_call.snap.burned(A, key), "not burned before the KO");
    run(&mut w, secs(20.0));
    let (_, ev) = fire(&mut w, A, m, false);
    assert!(twisted_mods(&ev, A).is_none(), "still fresh this life");
    // KO: burned; the respawn crumbles the ash off the ring
    let ev = void_ko(&mut w, A);
    assert!(calls(&ev).iter().any(|e| matches!(e, CallEvent::Burned { player, n } if *player == A && *n >= 1)));
    assert!(w.first_call.snap.burned(A, key));
    assert!(!w.first_call.snap.burned(B, key), "burns are per player");
    let ev = run(&mut w, secs(4.0));
    assert!(calls(&ev).iter().any(|e| matches!(e, CallEvent::Crumble { player, .. } if *player == A)));
    // the echo: never refused, ashen, cheaper (past the staleness window: no repeat surcharge)
    run(&mut w, secs(35.0));
    let (paid_echo, ev) = fire(&mut w, A, m, false);
    let mods = twisted_mods(&ev, A).expect("the echo");
    assert!(mods.contains(&byte("echo")));
    assert!(calls(&ev).iter().any(|e| matches!(e, CallEvent::Echo { player, .. } if *player == A)));
    assert!(paid_echo > 0.0 && paid_echo < paid, "echo {paid_echo} < original {paid}");
    // fresh things are unaffected
    let (_, ev) = fire(&mut w, A, kit("doomfist punch"), true);
    assert!(twisted_mods(&ev, A).is_none(), "a fresh move is the original");
    // B never used it: B's is the original
    let (_, ev) = fire(&mut w, B, m, false);
    assert!(twisted_mods(&ev, B).is_none());
}

#[test]
fn the_scope_knob_named_vs_everything() {
    let mut dragon = Recipe::unknown("dragon");
    dragon.form = Form::Quadruped;
    dragon.scale = 3.0;
    dragon.model = ModelSource::Procedural { seed: 0x1234_5678 };
    for scope in [BurnScope::Named, BurnScope::Everything] {
        let mut w = world(false, scope);
        full(&mut w, A);
        let a = aim(&w, A);
        w.cast(A, dragon.clone(), None, a).unwrap();
        run(&mut w, 1);
        void_ko(&mut w, A);
        run(&mut w, secs(4.0));
        full(&mut w, A);
        let before = wp(&w, A);
        let mut fire_dragon = dragon.clone();
        fire_dragon.name = "fire dragon".into();
        fire_dragon.element = Element::Fire;
        let a = aim(&w, A);
        let cid = w.cast(A, fire_dragon, None, a).expect("never refused");
        let paid = before - wp(&w, A);
        let c = w.constructs.get(cid).unwrap();
        let echo = w.first_call.snap.mark_of(cid) & mark::ECHO != 0;
        match scope {
            BurnScope::Everything => {
                assert!(echo, "EVERYTHING burns constructs: \"fire dragon\" is the burned dragon");
                assert!(c.recipe.scale < 3.0 * 0.8, "the echo is smaller ({})", c.recipe.scale);
                assert!(paid > 0.0);
            }
            _ => assert!(!echo, "NAMED never burns a generic dragon"),
        }
    }
}

#[test]
fn both_rules_compose_into_an_ashen_rival_priced_as_the_echo() {
    let mut w = world(true, BurnScope::Named);
    let m = shout("earthshatter");
    let (paid_a, _) = fire(&mut w, A, m, false);
    run(&mut w, secs(20.0));
    fire(&mut w, B, m, false); // B: rival
    void_ko(&mut w, B);
    run(&mut w, secs(24.0));
    let (paid, ev) = fire(&mut w, B, m, false);
    let mods = twisted_mods(&ev, B).expect("twisted");
    assert!(mods.contains(&byte("rival")) && mods.contains(&byte("echo")), "{mods:?}");
    assert!(paid < paid_a);
}

#[test]
fn chains_and_twists_of_a_claimed_thing_count_as_using_it() {
    let mut w = world(true, BurnScope::Off);
    let m = shout("earthshatter");
    fire(&mut w, A, m, false);
    run(&mut w, secs(20.0));
    // B twists it ("giant earthshatter"): the rival modifiers ride on the whole
    full(&mut w, B);
    let step = bd_sim::powers::twist::TwistStep {
        a: bd_sim::powers::twist::TwistSide {
            base: bd_sim::powers::twist::TwistBase::Move { mv: m, kit: false },
            mods: vec![byte("giant")],
        },
        b: None,
        mods: vec![],
        shot: 0,
    };
    w.toolbox(B, vec![ToolStep::Twist(Box::new(step))], None).unwrap();
    let ev = run(&mut w, 1);
    let mods = twisted_mods(&ev, B).unwrap();
    assert!(mods.contains(&byte("giant")) && mods.contains(&byte("rival")), "{mods:?}");
}

#[test]
fn bots_play_under_both_rules() {
    let mut w = world(true, BurnScope::Everything);
    let mut bots: Vec<bd_sim::bots::BotBrain> = [A, B].iter().map(|id| bd_sim::bots::BotBrain::new(*id, 0.8, id.0 as u64)).collect();
    let mut inputs = Vec::new();
    for _ in 0..secs(30.0) {
        inputs.clear();
        bd_sim::bots::drive(&mut w, &mut bots, &mut inputs);
        w.tick(&inputs);
    }
    assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
}

// ------------------------------------------------------------------------------ wire

#[test]
fn rules_state_and_events_round_trip_the_wire() {
    let cfg = bincode::config::standard();
    let mut r = MatchRules::default();
    r.first_call = true;
    r.burn = BurnScope::Everything;
    let bytes = bincode::serde::encode_to_vec(&r, cfg).unwrap();
    let (back, _): (MatchRules, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    assert_eq!(back, r);
    // the replicated state rides snapshots and deltas
    let mut w = world(true, BurnScope::Named);
    fire(&mut w, A, shout("earthshatter"), false);
    let base = w.snapshot_for(Some(A));
    run(&mut w, secs(20.0));
    fire(&mut w, B, shout("earthshatter"), false);
    let cur = w.snapshot_for(Some(A));
    assert!(cur.first_call.as_ref().is_some_and(|s| !s.claims.is_empty()));
    let d = bd_sim::SnapshotDelta::encode(Some(&base), &cur);
    let bytes = bincode::serde::encode_to_vec(&d, cfg).unwrap();
    let (d2, _): (bd_sim::SnapshotDelta, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    let rebuilt = d2.apply(Some(&base)).unwrap();
    assert_eq!(rebuilt.first_call, cur.first_call);
    // a client replica reads the same owners
    let mut client = world(true, BurnScope::Named);
    client.apply_snapshot(&cur, Some(B));
    let key = Identities::shipped().of_move(shout("earthshatter")).unwrap().key;
    assert_eq!(client.first_call.snap.owner_of(key), Some(A));
    // events
    let e = GameEvent::FirstCall(CallEvent::Rival { player: B, owner: A, key, kind: CallKind::Character, pos: Vec3::ONE });
    let bytes = bincode::serde::encode_to_vec(&e, cfg).unwrap();
    let (e2, _): (GameEvent, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    assert_eq!(e2, e);
    let s = CallSnap { claims: vec![ClaimRow { key, owner: A, kind: CallKind::Character, tick: 5 }], ..Default::default() };
    let bytes = bincode::serde::encode_to_vec(&s, cfg).unwrap();
    let (s2, _): (CallSnap, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    assert_eq!(s2, s);
    let _ = chain::MAX_QUEUE_VIEW;
}

#[test]
#[ignore]
fn dump_identities() {
    let ids = Identities::shipped();
    for n in [
        "goku",
        "son goku",
        "kamehameha",
        "spirit bomb",
        "rasengan",
        "naruto",
        "sharingan",
        "amaterasu",
        "chidori",
        "optimus",
        "optimus prime",
        "megatron",
        "bumblebee",
        "mjolnir",
        "thor",
        "spider-man",
        "spiderman",
        "batman",
        "batarang",
        "hollow purple",
        "gojo",
        "domain expansion",
        "unlimited void",
        "hadouken",
        "ryu",
        "doomfist",
        "rocket punch",
        "gum gum pistol",
        "luffy",
        "gomu gomu",
        "pikachu",
        "godzilla",
        "super saiyan",
        "ultra instinct",
        "dragon",
        "fire",
        "hammer",
        "sword",
    ] {
        eprintln!("{n:>18}: {:?}", ids.of_name(n));
    }
}
