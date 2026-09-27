//! AUTO-KITS (docs/toolbox/AUTO_KITS.md): the GENERATED character kits — `assets/powers/zz_derived_kits.ron`,
//! written by `python tools/derive_kits/derive_kits.py`, compact template ids expanded by `powers::derive` —
//! give the most-named characters no hand-made kit covers a `kind: Kit` ("<character> ultimate" does
//! something in character).
//!
//! Covered here: the file sorts last and fits the table, coverage counts, derived kits never grant / shout,
//! every derived kit answers the slot words with the right move, HAND KITS ALWAYS WIN, golden phrases for
//! obscure characters, and one move of every op shape the generator emits fires cleanly and
//! deterministically. Staleness (a hand kit landed for a character that still has a derived kit) is only
//! a printed warning: re-run the generator (`--check` exits 1 when the file is stale).

use std::collections::{BTreeMap, HashSet};

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3, TICK_HZ};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::{FireHow, ToolStep};
use bd_sim::powers::toolbox::{MoveIndex, MoveRef, Reading};
use bd_sim::powers::{norm_words, MoveOp, MoveTag, PowerDef, PowerEvent, PowerTable, MAX_POWERS, SHIPPED_FILES};
use bd_sim::world::{ArenaSource, SimConfig, World};

const DERIVED_FILE: &str = "zz_derived_kits.ron";
const PREFIX: &str = "dk_";
const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn table() -> &'static PowerTable {
    PowerTable::get()
}

fn is_derived(p: &PowerDef) -> bool {
    p.id.starts_with(PREFIX)
}

fn derived() -> impl Iterator<Item = (u16, &'static PowerDef)> {
    table().powers.iter().enumerate().filter(|(_, p)| is_derived(p)).map(|(i, p)| (i as u16, p))
}

fn has_derived() -> bool {
    SHIPPED_FILES.iter().any(|(n, _)| *n == DERIVED_FILE)
}

/// "<name> <word>" the way the game reads it (toolbox.rs; possessives stripped like a spoken phrase).
fn kit_of(ix: &MoveIndex, phrase: &str) -> Option<MoveRef> {
    ix.kit_move(table(), &norm_words(phrase)).map(|(m, _)| m)
}

/// A kit's name as players say it (the first alias; `names` are already normalised).
fn said(p: &PowerDef) -> &str {
    p.names.first().map(String::as_str).unwrap_or(&p.name)
}

#[test]
fn derived_file_sorts_last_and_fits_the_table() {
    if !has_derived() {
        eprintln!("no {DERIVED_FILE}: run python tools/derive_kits/derive_kits.py");
        return;
    }
    assert_eq!(SHIPPED_FILES.last().map(|f| f.0), Some(DERIVED_FILE), "the derived file must load LAST (ties go to hand content)");
    let t = table();
    assert!(t.powers.len() < MAX_POWERS, "{} powers: raise MAX_POWERS (PowerSnap ids are u16)", t.powers.len());
    // nothing truncated: the table holds every entry of every file
    let total: usize = SHIPPED_FILES.iter().map(|(n, src)| PowerTable::parse(src).unwrap_or_else(|e| panic!("{n}: {e}")).powers.len()).sum();
    assert_eq!(t.powers.len(), total, "the power table was truncated");
    // every derived entry comes after every hand entry (ids of hand content never move)
    let first = t.powers.iter().position(is_derived).expect("derived kits present");
    assert!(t.powers[first..].iter().all(is_derived), "hand content after the derived kits");
    let ids: HashSet<&str> = t.powers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), t.powers.len(), "duplicate power ids");
}

#[test]
fn coverage_every_character_has_a_kit() {
    if !has_derived() {
        return;
    }
    let t = table();
    let hand = t.powers.iter().filter(|p| p.kind.is_kit() && !is_derived(p)).count();
    let dk = derived().count();
    let moves: usize = derived().map(|(_, p)| p.moves.len()).sum();
    let names: usize = derived().map(|(_, p)| p.names.len()).sum();
    eprintln!("characters with a kit: {hand} hand-made -> {} with derived kits ({dk} derived kits, {moves} moves, {names} names)", hand + dk);
    assert!(dk >= 1000, "only {dk} derived kits: did the generator lose its sources?");
    assert!(t.powers.len() < MAX_POWERS, "the derived budget must leave the table room");
}

#[test]
fn derived_kits_are_complete_kits_that_never_grant_or_shout() {
    let ix = MoveIndex::shipped();
    for (_, p) in derived() {
        assert!(p.kind.is_kit(), "{}: not a kit", p.id);
        assert!(p.shouts.is_empty(), "{}: derived kits add no borrowable shouts", p.id);
        assert!(p.tiers.is_empty() && p.transform.is_none() && p.companion.is_none(), "{}", p.id);
        assert!((5..=8).contains(&p.moves.len()), "{}: {} moves", p.id, p.moves.len());
        for tag in [MoveTag::Primary, MoveTag::Movement, MoveTag::Defense, MoveTag::Counter, MoveTag::Combo, MoveTag::Ultimate] {
            let n = p.moves.iter().filter(|m| m.tags.contains(&tag)).count();
            assert_eq!(n, 1, "{}: {n} moves tagged {tag:?}", p.id);
        }
        let mut seen = HashSet::new();
        for m in &p.moves {
            assert!(seen.insert(m.name.as_str()), "{}: two moves named {:?}", p.id, m.name);
            assert!(!m.ops.is_empty(), "{} {}: no ops", p.id, m.name);
            assert!(!m.ops.iter().any(|o| matches!(o, MoveOp::Transform | MoveOp::Tier(_))), "{} {}", p.id, m.name);
        }
        for n in &p.names {
            assert!(!ix.is_grant_name(n), "{}: kit name {n:?} is a grant name (a power's NAME) — re-run the generator", p.id);
        }
        // the bare name stays the costume summon: never a derived move reading (a HAND kit may claim
        // "<its name> <word>" = a derived bare name — that is the hand file's phrase, and hand wins)
        if let Reading::Kit(m) | Reading::Borrow(m) = ix.read(table(), None, said(p)) {
            assert!(!is_derived(&table().powers[m.power as usize]), "{}: bare name {:?} reads as a derived move", p.id, said(p));
        }
    }
}

#[test]
fn every_derived_kit_answers_the_slot_words() {
    let ix = MoveIndex::shipped();
    let t = table();
    let words = [
        ("attack", MoveTag::Primary),
        ("punch", MoveTag::Primary),
        ("dash", MoveTag::Movement),
        ("dodge", MoveTag::Movement),
        ("block", MoveTag::Defense),
        ("counter", MoveTag::Counter),
        ("combo", MoveTag::Combo),
        ("ultimate", MoveTag::Ultimate),
        ("special", MoveTag::Ultimate),
        ("super", MoveTag::Ultimate),
    ];
    // names a HAND kit now claims too (a stale derived file): the hand kit answers, maybe with nothing
    let hand_names: HashSet<&str> = t.powers.iter().filter(|p| p.kind.is_kit() && !is_derived(p)).flat_map(|p| p.names.iter().map(String::as_str)).collect();
    let mut checked = 0;
    let mut bad = Vec::new();
    let t0 = std::time::Instant::now();
    for (k, (pid, p)) in derived().enumerate() {
        // every kit's ultimate; the other slot words on a spread sample (debug builds are slow)
        let sample = k % 6 == 0;
        if hand_names.contains(said(p)) {
            continue;
        }
        for (w, tag) in words {
            if !sample && tag != MoveTag::Ultimate {
                continue;
            }
            let phrase = format!("{} {w}", said(p));
            checked += 1;
            match kit_of(ix, &phrase) {
                Some(m) if m.power == pid => {
                    let mv = &p.moves[m.slot as usize];
                    if !mv.tags.contains(&tag) {
                        bad.push(format!("{phrase:?} fired {:?} {:?}, wanted {tag:?}", mv.name, mv.tags));
                    }
                }
                Some(m) if !is_derived(&t.powers[m.power as usize]) => {} // a hand kit took the name: hand wins
                other => bad.push(format!("{phrase:?} reads as {:?}", other.map(|m| t.powers[m.power as usize].id.clone()))),
            }
        }
    }
    eprintln!("{checked} kit phrases read in {:?}", t0.elapsed());
    assert!(bad.is_empty(), "{} slot-word misses:\n{}", bad.len(), bad.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn hand_kits_always_win() {
    let ix = MoveIndex::shipped();
    let t = table();
    let derived_names: HashSet<&str> = derived().flat_map(|(_, p)| p.names.iter().map(String::as_str)).collect();
    let mut stale = Vec::new();
    for (pid, p) in t.powers.iter().enumerate().filter(|(_, p)| p.kind.is_kit() && !is_derived(p)) {
        for n in &p.names {
            if derived_names.contains(n.as_str()) {
                stale.push(format!("{} / {n}", p.id));
            }
            // whatever the derived table says, "<hand kit name> <slot word>" is the hand kit's move
            for w in ["ultimate", "attack", "dash", "block", "combo"] {
                if let Some(m) = kit_of(ix, &format!("{n} {w}")) {
                    let got = &t.powers[m.power as usize];
                    assert!(!is_derived(got), "{n:?} {w}: the derived kit {} beat the hand kit {}", got.id, p.id);
                    let _ = pid;
                }
            }
        }
    }
    if !stale.is_empty() {
        eprintln!(
            "STALE derived kits ({}): hand kits now name them — run `python tools/derive_kits/derive_kits.py` (hand still wins meanwhile): {:?}",
            stale.len(),
            &stale[..stale.len().min(20)]
        );
    }
}

/// Obscure characters nobody hand-built: "<name> <slot word>" must fire THEIR derived move (or, once a
/// content worker hand-builds them, the hand kit — hand always wins).
#[test]
fn golden_phrases_obscure_characters() {
    if !has_derived() {
        return;
    }
    let ix = MoveIndex::shipped();
    let t = table();
    let golden: &[(&str, MoveTag, &str)] = &[
        ("slenderman ultimate", MoveTag::Ultimate, "dk_slenderman"),
        ("jon snow combo", MoveTag::Combo, "dk_jon_snow"),
        ("eric cartman ultimate", MoveTag::Ultimate, "dk_eric_cartman"),
        ("garchomp ultimate", MoveTag::Ultimate, "dk_garchomp"),
        ("zapp brannigan dodge", MoveTag::Movement, "dk_zapp_brannigan"),
        ("catbus attack", MoveTag::Primary, "dk_catbus"),
        ("mongolian death worm special", MoveTag::Ultimate, "dk_mongolian_death_worm"),
        ("court of owls block", MoveTag::Defense, "dk_court_of_owls"),
        ("michael scott super", MoveTag::Ultimate, "dk_michael_scott"),
        ("lockheed attack", MoveTag::Primary, "dk_lockheed"),
        ("kenshin counter", MoveTag::Counter, "dk_kenshin"),
        ("martian manhunter's ultimate", MoveTag::Ultimate, "dk_martian_manhunter"),
    ];
    let mut hits = 0;
    for (phrase, tag, id) in golden {
        let Some(m) = kit_of(ix, phrase) else {
            eprintln!("{phrase:?}: no kit move (cut by the budget after a regeneration?)");
            continue;
        };
        let (def, mv) = m.resolve(t).expect("resolves");
        if !is_derived(def) {
            eprintln!("{phrase:?}: now a hand kit ({}) — fine, hand wins", def.id);
            continue;
        }
        if def.id != *id {
            // the character's id may carry a pack suffix after a regeneration; the name must still match
            assert!(def.names.iter().any(|n| phrase.starts_with(n.as_str())), "{phrase:?} fired {} ({})", def.id, mv.name);
        }
        assert!(mv.tags.contains(tag), "{phrase:?} fired {} {:?}, wanted {tag:?}", mv.name, mv.tags);
        hits += 1;
    }
    assert!(hits >= golden.len() / 2, "most golden characters should still be derived ({hits}/{})", golden.len());
    // the Pokémon get their own move names (vocab.py POKEMON): Garchomp's ultimate is Earthquake
    if let Some(m) = kit_of(ix, "garchomp ultimate") {
        let (def, mv) = m.resolve(t).unwrap();
        if is_derived(def) {
            assert_eq!(mv.name, "earthquake");
        }
    }
}

/// POLISH: every presentation style a derived kit or move names is a style key / alias in
/// `assets/tuning/move_fx.ron` (bd_game's `move_fx_every_fx_key…` test only scans RON text, and derived
/// moves get their `fx` when they are expanded).
#[test]
fn derived_fx_keys_exist_in_move_fx() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tuning/move_fx.ron");
    let Ok(src) = std::fs::read_to_string(path) else { return };
    let norm = |s: &str| s.to_lowercase().replace([' ', '_', '-'], "");
    let mut keys = HashSet::new();
    for line in src.lines() {
        let line = line.split("//").next().unwrap_or("");
        let mut rest = line;
        while let Some(a) = rest.find('"') {
            let Some(b) = rest[a + 1..].find('"') else { break };
            let key = &rest[a + 1..a + 1 + b];
            let after = rest[a + 2 + b..].trim_start();
            if after.starts_with(':') {
                keys.insert(norm(key));
            }
            rest = &rest[a + 2 + b..];
        }
    }
    let mut bad = Vec::new();
    for (_, p) in derived() {
        for fx in std::iter::once(&p.fx).chain(p.moves.iter().map(|m| &m.fx)) {
            if !fx.is_empty() && !keys.contains(&norm(fx)) {
                bad.push(format!("{}: fx {fx:?}", p.id));
            }
        }
    }
    bad.dedup();
    assert!(bad.is_empty(), "unknown move_fx keys:\n{}", bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
}

/// POLISH: grass-type Pokémon are PLANTS (the icon data files them under Earth): leaf / vine / root / solar
/// moves in the leaf style, never an earthquake or a rock throw.
#[test]
fn grass_types_are_plants_not_rocks() {
    let t = table();
    let mut seen = 0;
    for id in ["dk_chikorita", "dk_venusaur", "dk_sceptile", "dk_leafeon", "dk_treecko", "dk_torterra"] {
        let Some(p) = t.powers.iter().find(|p| p.id == id) else { continue };
        seen += 1;
        assert_eq!(p.fx, "nature", "{id}: kit style");
        for m in &p.moves {
            assert!(!["earthquake", "rock throw", "stone bolt", "rock slide"].contains(&m.name.as_str()), "{id}: {:?}", m.name);
        }
        let ult = p.moves.iter().find(|m| m.tags.contains(&MoveTag::Ultimate)).expect("ultimate");
        let ok = ult.ops.iter().any(|o| matches!(o, MoveOp::Cast { signature: Some(_), .. } | MoveOp::Erupt { .. }));
        assert!(ok, "{id}: the ultimate should be the solar beam or the overgrowth, got {:?}", ult.name);
    }
    if seen == 0 {
        eprintln!("no grass-type derived kits (hand-built now?)");
    }
}

/// POLISH: every derived ULTIMATE lands with an impact frame (the pack's, else the element's look).
#[test]
fn derived_ultimates_have_impact_frames() {
    for (_, p) in derived() {
        for m in p.moves.iter().filter(|m| m.tags.contains(&MoveTag::Ultimate)) {
            let fr = m.juice.as_ref().and_then(|j| j.impact_frame.as_deref());
            assert!(fr.is_some_and(|f| !f.is_empty()), "{} {:?}: no impact frame", p.id, m.name);
        }
    }
}

// ------------------------------------------------------------------------------------ firing

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
    for (id, z) in [(A, 0.0), (B, -5.0)] {
        let p = w.player_mut(id).unwrap();
        p.mv.pos = Vec3::new(0.0, 8.0, z);
        p.mv.vel = Vec3::ZERO;
        p.input.yaw = 0.0;
        p.input.pitch = 0.0;
    }
    w.player_mut(A).unwrap().willpower = 100.0;
    w
}

/// "Shove+Impulse" etc.: the op shape of a move.
fn shape(ops: &[MoveOp]) -> String {
    ops.iter()
        .map(|o| {
            let s = format!("{o:?}");
            s.split(|c| c == ' ' || c == '(' || c == '{').next().unwrap_or("").to_string()
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[test]
fn every_generated_move_shape_fires_cleanly_and_deterministically() {
    if !has_derived() {
        return;
    }
    // one move per (op shape, tag): the generator's whole template vocabulary
    let mut picks: BTreeMap<String, MoveRef> = BTreeMap::new();
    for (pid, p) in derived() {
        for (slot, m) in p.moves.iter().enumerate() {
            let key = format!("{} {:?}", shape(&m.ops), m.tags.first());
            picks.entry(key).or_insert(MoveRef::new(pid, 0, slot as u8));
        }
    }
    eprintln!("{} distinct generated move shapes", picks.len());
    let fire = |mv: MoveRef| {
        let mut w = duel();
        // a targeted move (warp behind, grab…) may refuse up front when nobody is in reach: fine
        let refused = w.toolbox(A, vec![ToolStep::Move { mv, kit: true }], None).is_err();
        let mut evs = Vec::new();
        for _ in 0..(TICK_HZ * 3) {
            evs.extend_from_slice(w.tick(&[]));
        }
        (evs, w.state_hash(), refused)
    };
    let mut silent = Vec::new();
    let mut refused = Vec::new();
    for (key, mv) in &picks {
        let (evs, h1, no) = fire(*mv);
        if no {
            refused.push(key.clone());
            continue;
        }
        let ok = evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Fired { mv: m, how: FireHow::Kit, .. }) if m == mv));
        if !ok {
            silent.push(format!("{key}: {}", table().powers[mv.power as usize].id));
        }
        let (_, h2, _) = fire(*mv);
        assert_eq!(h1, h2, "{key}: not deterministic");
    }
    if !refused.is_empty() {
        eprintln!("refused up front (no target in the test duel): {refused:?}");
    }
    assert!(refused.len() * 10 <= picks.len(), "too many refusals: {refused:?}");
    // a Cast-only move pays the cast pipeline (it may be refused for willpower in odd setups): allow a few
    assert!(silent.len() * 10 <= picks.len(), "moves that never fired:\n{}", silent.join("\n"));
}
