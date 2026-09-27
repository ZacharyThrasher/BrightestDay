//! RE:ZERO (`assets/powers/re_zero.ron`, docs/toolbox/BRIEF.md): the spell family (bare / El / Ul / Al
//! escalating in presence, cost and drama), golden phrases (spells with their tier prefixes and
//! Whisper mishearings, the cast's kits, chains), and behaviour of the headliners fired for real
//! through the toolbox: Return by Death (an escape — the witch's miasma bursts where you stood, you're
//! back three seconds, never more hurt), Invisible Providence (a hand seizes the aimed enemy, then
//! flings them; nothing aimed = nothing spent), the Dragon Sword Reid (only drawn for the worthy),
//! Al Goa (the gather, then the sun lands), Al Shamak, Rem's morning star pull. No new ops.

use bd_core::corps::Corps;
use bd_core::{PlayerId, TICK_HZ, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::{move_tier, ToolStep};
use bd_sim::powers::ops::OpEvent;
use bd_sim::powers::toolbox::{split_chain, MoveIndex, MoveRef};
use bd_sim::powers::{MoveOp, MoveTag, PowerEvent, PowerKind, PowerTable, norm_words};
use bd_sim::presence_tier::Tier;
use bd_sim::status::StatusKind;
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

fn pos(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().mv.pos
}

fn pct(w: &World, id: PlayerId) -> f32 {
    w.player(id).unwrap().damage_pct
}

fn flat_dist(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
}

/// The move a phrase reads as for a powerless player (shout or kit form).
fn move_ref(s: &str) -> MoveRef {
    MoveIndex::shipped().read(PowerTable::get(), None, s).move_ref().unwrap_or_else(|| panic!("{s:?} is not a move"))
}

/// Fire a phrase's move for real (a borrowed / kit one-shot) with a full willpower bar.
fn say(w: &mut World, who: PlayerId, s: &str) -> Result<(), bd_sim::powers::PowerError> {
    w.player_mut(who).unwrap().willpower = 100.0;
    w.borrow_move(who, move_ref(s), true, None)
}

/// (power id, move name) a phrase fires for a powerless player, or `None` for a plain phrase.
fn reads(phrase: &str) -> Option<(String, String)> {
    let t = PowerTable::get();
    let r = MoveIndex::shipped().read(t, None, phrase);
    r.move_ref().map(|m| {
        let (p, mv) = m.resolve(t).expect("resolves");
        (p.id.clone(), mv.name.clone())
    })
}

fn fires(phrase: &str, power: &str, mv: &str) {
    assert_eq!(reads(phrase), Some((power.to_string(), mv.to_string())), "{phrase:?}");
}

// ------------------------------------------------------------------------------------ the table

#[test]
fn every_re_zero_entry_is_well_formed() {
    let t = PowerTable::get();
    let rz: Vec<_> = t.powers.iter().filter(|p| p.id.starts_with("rz_")).collect();
    assert!(rz.len() >= 30, "{} Re:Zero entries", rz.len());
    let moves: usize = rz.iter().map(|p| p.moves.len()).sum();
    assert!(moves >= 150, "{moves} moves");
    for p in &rz {
        assert!(!p.moves.is_empty() && p.moves.len() <= 8, "{}: {} moves", p.id, p.moves.len());
        assert!(p.shouts.iter().all(|(_, s)| (*s as usize) < p.moves.len()), "{}: shout slot", p.id);
        if p.kind == PowerKind::Kit {
            assert!(p.moves.iter().any(|m| m.tags.contains(&MoveTag::Ultimate)), "{}: tag an Ultimate", p.id);
            for m in &p.moves {
                assert!(!m.ops.iter().any(|o| matches!(o, MoveOp::Transform | MoveOp::Tier(_))), "{} / {}: no transforms in a kit", p.id, m.name);
            }
        }
        for m in &p.moves {
            assert!(!m.ops.is_empty(), "{} / {}", p.id, m.name);
        }
    }
    // The Oni form is a technique you BECOME (a grant), not a one-shot.
    let oni = t.powers.iter().find(|p| p.id == "rz_oni").expect("rz_oni");
    assert_eq!(oni.kind, PowerKind::Technique);
}

#[test]
fn the_spell_family_escalates_bare_el_ul_al() {
    let t = PowerTable::get();
    for (id, base) in [("rz_goa", "goa"), ("rz_huma", "huma"), ("rz_fura", "fura"), ("rz_dona", "dona"), ("rz_shamak", "shamak"), ("rz_minya", "minya")] {
        let p = t.powers.iter().find(|p| p.id == id).unwrap_or_else(|| panic!("{id}"));
        let names: Vec<&str> = p.moves.iter().take(4).map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec![base.to_string(), format!("el {base}"), format!("ul {base}"), format!("al {base}")], "{id}");
        let tier = |k: usize| move_tier(&p.moves[k]).unwrap_or(Tier::Tiny);
        for k in 1..4 {
            let (lo, hi) = (&p.moves[k - 1], &p.moves[k]);
            assert!(hi.cost > lo.cost, "{id}: {} costs more than {}", hi.name, lo.name);
            assert!(hi.cooldown > lo.cooldown, "{id}: {} cools longer than {}", hi.name, lo.name);
            assert!(tier(k) >= tier(k - 1), "{id}: {} presence ≥ {}", hi.name, lo.name);
        }
        assert!(tier(1) >= Tier::Normal, "{id}: El is a real spell");
        assert!(tier(2) >= Tier::Big, "{id}: Ul is Big");
        assert_eq!(tier(3), Tier::Titan, "{id}: Al is Titan");
        // Presentation climbs too: the cantrip has no title, El has one, Ul booms, Al has an impact frame.
        let juice = |k: usize| p.moves[k].juice.clone().unwrap_or_default();
        assert!(juice(0).title.is_none(), "{id}: the bare spell is a cantrip");
        assert!(juice(1).title.is_some(), "{id}: El has a title");
        assert!(juice(2).boom && juice(2).title.is_some(), "{id}: Ul booms");
        let al = juice(3);
        assert!(al.boom && al.title.is_some() && al.impact_frame.is_some() && al.shake >= 0.2, "{id}: Al is the showpiece");
        assert!(p.moves[3].tags.contains(&MoveTag::Ultimate) && p.moves[0].tags.contains(&MoveTag::Primary), "{id}: tags");
    }
}

// ------------------------------------------------------------------------------------ golden phrases

#[test]
fn golden_spells_with_their_tier_prefixes_and_mishearings() {
    for (base, id) in [("goa", "rz_goa"), ("huma", "rz_huma"), ("fura", "rz_fura"), ("dona", "rz_dona"), ("shamak", "rz_shamak"), ("minya", "rz_minya")] {
        fires(base, id, base);
        for pre in ["el", "ul", "al"] {
            let spell = format!("{pre} {base}");
            fires(&spell, id, &spell);
        }
        // "al" heard as "all", joined words, capitals and punctuation from Whisper.
        fires(&format!("all {base}"), id, &format!("al {base}"));
        fires(&format!("Al {}!", base.to_uppercase()), id, &format!("al {base}"));
        // "<attribute> magic ultimate" finds the Al spell by its tag.
        fires(&format!("{base} magic ultimate"), id, &format!("al {base}"));
    }
    fires("el hooma", "rz_huma", "el huma");
    fires("ool huma", "rz_huma", "ul huma");
    fires("algoa", "rz_goa", "al goa");
    fires("shamac", "rz_shamak", "shamak");
    fires("all shamac", "rz_shamak", "al shamak");
    fires("minia", "rz_minya", "minya");
    fires("doona", "rz_dona", "dona");
    fires("murak", "rz_shamak", "murak");
    fires("el murak", "rz_shamak", "el murak");
    fires("jiwald", "rz_goa", "jiwald");
    fires("el jiwald", "rz_goa", "el jiwald");
    fires("E.M.M.", "rz_beatrice_kit", "e m m");
    fires("emm", "rz_beatrice_kit", "e m m");
}

#[test]
fn golden_kit_phrases_fire_the_characters_own_move() {
    fires("subaru invisible providence", "rz_subaru_kit", "invisible providence");
    fires("barusu invisible providence", "rz_subaru_kit", "invisible providence");
    fires("subaru natsuki ultimate", "rz_subaru_kit", "return by death");
    fires("subaru return by death", "rz_subaru_kit", "return by death");
    fires("subaru shamak", "rz_subaru_kit", "shamak");
    fires("subaru block", "rz_subaru_kit", "cor leonis");
    fires("subaru attack", "rz_subaru_kit", "invisible providence");
    fires("emilia ultimate", "rz_emilia_kit", "al huma");
    fires("emilia ice wall", "rz_emilia_kit", "ice wall");
    fires("emilia-tan ice brand arts", "rz_emilia_kit", "ice brand arts");
    fires("emilia puck", "rz_emilia_kit", "puck");
    fires("puck ultimate", "rz_puck_kit", "eternal winter");
    fires("rem morning star", "rz_rem_kit", "morning star");
    fires("rem's morning star", "rz_rem_kit", "morning star");
    fires("rems morning star", "rz_rem_kit", "morning star");
    fires("rem ultimate", "rz_rem_kit", "oni form");
    fires("rem el huma", "rz_rem_kit", "el huma");
    fires("rem combo", "rz_rem_kit", "morning star spin");
    fires("ram el fura", "rz_ram_kit", "el fura");
    fires("ram clairvoyance", "rz_ram_kit", "clairvoyance");
    fires("ram ultimate", "rz_ram_kit", "ul fura");
    fires("beako al shamak", "rz_beatrice_kit", "al shamak");
    fires("betty el minya", "rz_beatrice_kit", "el minya");
    fires("beatrice door crossing", "rz_beatrice_kit", "door crossing");
    fires("reinhard van astrea ultimate", "rz_reinhard_kit", "dragon sword reid");
    fires("sword saint counter", "rz_reinhard_kit", "divine protection");
    fires("reinhard divine protection", "rz_reinhard_kit", "divine protection");
    fires("wilhelm combo", "rz_wilhelm_kit", "sword demon flurry");
    fires("julius ultimate", "rz_julius_kit", "al clauzeria");
    fires("crusch ultimate", "rz_crusch_kit", "hundred man slash");
    fires("garfiel ultimate", "rz_garfiel_kit", "tiger form");
    fires("roswaal ultimate", "rz_roswaal_kit", "al goa");
    fires("elsa kukri flurry", "rz_elsa_kit", "kukri flurry");
    fires("petelgeuse unseen hands", "rz_petelgeuse_kit", "unseen hands");
    fires("betelgeuse unseen hands", "rz_petelgeuse_kit", "unseen hands");
    fires("regulus lion's heart", "rz_regulus_kit", "lion's heart");
    fires("satella ultimate", "rz_satella_kit", "shadow of envy");
    fires("priscilla ultimate", "rz_priscilla_kit", "yang sword volakia");
    // Rem's morning star is hers: the bare "morning star" shout stays the myth pack's (Venus).
    assert_ne!(reads("morning star").map(|r| r.0), Some("rz_rem_kit".to_string()));
}

#[test]
fn golden_borrowed_shouts_and_names() {
    fires("invisible providence", "rz_subaru_kit", "invisible providence");
    fires("return by death", "rz_subaru_kit", "return by death");
    fires("Return By Death!", "rz_subaru_kit", "return by death");
    // Shouting the show's own title starts you over from zero (Whisper: "Re:Zero", "rezero", "ree zero").
    for title in ["re zero", "Re:Zero!", "rezero", "ree zero"] {
        fires(title, "rz_subaru_kit", "return by death");
    }
    fires("cor leonis", "rz_subaru_kit", "cor leonis");
    fires("dragon sword reid", "rz_reinhard_kit", "dragon sword reid");
    fires("al clauzeria", "rz_julius_kit", "al clauzeria");
    fires("hundred man slash", "rz_crusch_kit", "hundred man slash");
    fires("hundred-man slash", "rz_crusch_kit", "hundred man slash");
    fires("unseen hands", "rz_petelgeuse_kit", "unseen hands");
    fires("brain trembling", "rz_petelgeuse_kit", "brain trembling");
    fires("lion's heart", "rz_regulus_kit", "lion's heart");
    fires("little king", "rz_regulus_kit", "little king");
    fires("bowel hunter", "rz_elsa_kit", "bowel hunter");
    fires("door crossing", "rz_beatrice_kit", "door crossing");
    fires("beast of the end", "rz_puck_kit", "beast of the end");
    fires("shadow of envy", "rz_satella_kit", "shadow of envy");
    fires("yang sword volakia", "rz_priscilla_kit", "yang sword volakia");
    fires("oni el huma", "rz_oni", "oni el huma");
    // Names never fire anything by themselves: the bare cast stays the costume summon, the Oni form
    // is a grant, and the composed grammar still owns "ram <someone>".
    for plain in ["subaru", "emilia", "rem", "beatrice", "reinhard van astrea", "oni form", "oni mode", "ram him", "ram through him"] {
        assert_eq!(reads(plain), None, "{plain:?} is not a move reading");
    }
}

#[test]
fn golden_chains_split_into_the_right_moves() {
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let is_move = |s: &str| ix.read(t, None, s).is_move();
    let steps = |s: &str| -> Vec<Option<(String, String)>> { split_chain(s, &is_move).iter().map(|p| reads(p)).collect() };
    let m = |p: &str, n: &str| Some((p.to_string(), n.to_string()));
    assert_eq!(
        steps("shamak -> invisible providence -> el huma"),
        vec![m("rz_shamak", "shamak"), m("rz_subaru_kit", "invisible providence"), m("rz_huma", "el huma")]
    );
    assert_eq!(
        steps("subaru shamak then subaru invisible providence then al huma"),
        vec![m("rz_subaru_kit", "shamak"), m("rz_subaru_kit", "invisible providence"), m("rz_huma", "al huma")]
    );
    assert_eq!(
        steps("rem morning star pull -> rem morning star -> rem oni form"),
        vec![m("rz_rem_kit", "morning star pull"), m("rz_rem_kit", "morning star"), m("rz_rem_kit", "oni form")]
    );
    assert_eq!(
        steps("al goa then al huma then al fura then al dona"),
        vec![m("rz_goa", "al goa"), m("rz_huma", "al huma"), m("rz_fura", "al fura"), m("rz_dona", "al dona")]
    );
    assert_eq!(steps("el minya, al shamak"), vec![m("rz_minya", "el minya"), m("rz_shamak", "al shamak")]);
    assert_eq!(steps("ram clairvoyance, ram wind cutter"), vec![m("rz_ram_kit", "clairvoyance"), m("rz_ram_kit", "wind cutter")]);
    assert_eq!(steps("elsa kukri flurry -> elsa bowel hunter"), vec![m("rz_elsa_kit", "kukri flurry"), m("rz_elsa_kit", "bowel hunter")]);
    assert_eq!(steps("reinhard divine protection -> dragon sword reid"), vec![m("rz_reinhard_kit", "divine protection"), m("rz_reinhard_kit", "dragon sword reid")]);
}

// ------------------------------------------------------------------------------------ headliners, for real

#[test]
fn return_by_death_bursts_where_you_stood_and_takes_you_back() {
    let mut w = duel();
    // Three and a half quiet seconds at the origin (the trail), then hurt and 12 m away.
    run(&mut w, ticks(3.5));
    {
        let p = w.player_mut(A).unwrap();
        p.mv.pos = Vec3::new(12.0, GROUND, 0.0);
        p.damage_pct = 60.0;
    }
    stand(&mut w, B, 14.0, 0.0);
    run(&mut w, ticks(0.2));
    let left = pos(&w, A);
    say(&mut w, A, "return by death").expect("return by death");
    run(&mut w, 2);
    assert!(w.player(B).unwrap().status.active(StatusKind::Shadow), "B is inside the witch's shroud");
    run(&mut w, ticks(0.3));
    // Back where you were three seconds ago (the origin), never more hurt than you are.
    assert!(flat_dist(pos(&w, A), Vec3::new(0.0, GROUND, 0.0)) < 3.0, "rewound to the origin: {:?}", pos(&w, A));
    assert!(pct(&w, A) < 60.0, "the damage you had then: {}", pct(&w, A));
    // The miasma burst where you stood: B (2 m from it) was thrown and is in the dark.
    assert!(flat_dist(pos(&w, B), left) > 3.0, "B thrown off the spot: {:?}", pos(&w, B));
}

#[test]
fn invisible_providence_seizes_then_flings_and_needs_a_target() {
    // Nothing under the crosshair: nothing happens and nothing is spent.
    let mut w = duel();
    w.player_mut(A).unwrap().input.yaw = std::f32::consts::PI; // look away from B
    w.player_mut(A).unwrap().willpower = 100.0;
    assert!(w.borrow_move(A, move_ref("invisible providence"), true, None).is_err());
    assert!((w.player(A).unwrap().willpower - 100.0).abs() < 1e-3, "no target, no cost");
    // B 12 m ahead: lifted (weightless) and hurt now, flung away a beat later; Subaru pays a little.
    let mut w = duel();
    stand(&mut w, B, 0.0, -12.0);
    say(&mut w, A, "invisible providence").expect("fires");
    run(&mut w, 2);
    assert!(w.player(B).unwrap().status.active(StatusKind::Float), "seized");
    let seized = pct(&w, B);
    assert!(seized > 0.0);
    assert!(pct(&w, A) > 0.0, "the hand hurts Subaru too (Strain)");
    run(&mut w, ticks(0.6));
    assert!(pct(&w, B) > seized, "the crush lands a beat later");
    assert!(pos(&w, B).z < -13.0, "flung along the look: {:?}", pos(&w, B));
}

#[test]
fn dragon_sword_reid_stays_sheathed_without_a_worthy_target() {
    let mut w = duel();
    w.player_mut(A).unwrap().input.yaw = std::f32::consts::PI;
    w.player_mut(A).unwrap().willpower = 100.0;
    assert!(w.borrow_move(A, move_ref("dragon sword reid"), true, None).is_err(), "no one worthy: sheathed");
    assert!((w.player(A).unwrap().willpower - 100.0).abs() < 1e-3);
    // Someone 20 m down the crosshair (just off it): a sure hit.
    let mut w = duel();
    stand(&mut w, B, 1.0, -20.0);
    say(&mut w, A, "dragon sword reid").expect("drawn");
    run(&mut w, 2);
    assert!(pct(&w, B) >= 26.0, "sure hit: {}", pct(&w, B));
}

#[test]
fn al_goa_gathers_then_lands() {
    let mut w = duel();
    stand(&mut w, B, 3.0, -12.0);
    w.player_mut(A).unwrap().input.pitch = -0.08; // the crosshair on the ground ~12 m ahead
    say(&mut w, A, "al goa").expect("al goa");
    // The landing point is marked (the warning ring) before anything lands.
    let evs = run(&mut w, 1);
    let sun = evs
        .iter()
        .find_map(|e| if let GameEvent::Power(PowerEvent::Op(OpEvent::Telegraph { pos, .. })) = e { Some(*pos) } else { None })
        .or_else(|| w.events().iter().find_map(|e| if let GameEvent::Power(PowerEvent::Op(OpEvent::Telegraph { pos, .. })) = e { Some(*pos) } else { None }))
        .expect("Al Goa is telegraphed");
    let d0 = flat_dist(pos(&w, B), sun);
    run(&mut w, ticks(0.6));
    assert!(pct(&w, B) < 1.0, "the gather doesn't hurt yet: {}", pct(&w, B));
    let d1 = flat_dist(pos(&w, B), sun);
    assert!(d1 < d0, "B slid toward the second sun ({d0} -> {d1}; sun at {sun:?})");
    run(&mut w, ticks(1.0));
    assert!(pct(&w, B) >= 22.0, "the sun landed: {} (B at {:?}, sun at {sun:?})", pct(&w, B), pos(&w, B));
}

#[test]
fn al_shamak_sends_them_away_unhurt() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -15.0);
    let b0 = pos(&w, B);
    say(&mut w, A, "al shamak").expect("al shamak");
    run(&mut w, 2);
    assert!((pos(&w, B) - b0).length() > 2.0, "displaced: {:?}", pos(&w, B));
    assert!(pct(&w, B) < 1.0, "unhurt: {}", pct(&w, B));
}

#[test]
fn rems_morning_star_drags_them_in() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -15.0);
    say(&mut w, A, "rem morning star pull").expect("thrown");
    run(&mut w, ticks(1.2));
    assert!(pct(&w, B) > 0.0, "the ball hit");
    assert!(flat_dist(pos(&w, A), pos(&w, B)) < 9.0, "hauled in: {:?}", pos(&w, B));
}

#[test]
fn the_spells_fire_through_the_toolbox_in_a_chain() {
    let mut w = duel();
    stand(&mut w, B, 0.0, -14.0);
    w.player_mut(A).unwrap().willpower = 100.0;
    let steps = vec![
        ToolStep::Move { mv: MoveIndex::shipped().shout(&norm_words("el minya")).expect("el minya"), kit: false },
        ToolStep::Move { mv: MoveIndex::shipped().shout(&norm_words("el fura")).expect("el fura"), kit: false },
    ];
    w.toolbox(A, steps, None).unwrap();
    let evs = run(&mut w, ticks(2.5));
    let fired: Vec<String> = evs
        .iter()
        .filter_map(|e| match e {
            GameEvent::Power(PowerEvent::Fired { mv, .. }) => mv.resolve(PowerTable::get()).map(|(_, m)| m.name.clone()),
            _ => None,
        })
        .collect();
    assert!(fired.iter().any(|n| n == "el fura"), "the chain reached El Fura: {fired:?}");
    assert!(pct(&w, B) > 0.0);
}
