//! MODIFIERS & FUSION (powers/twist): golden phrase readings (typed + spoken), the generic
//! transforms, fusion rules, pricing / caps, the host executor, determinism and the wire.
//!
//! Golden file: `tests/data/twist_golden.txt` — `phrase => reading`, where a reading is
//! `core{mod,mod}` (core = the move's name, or `cast:<phrase>` for a famous cast the lexicon knows),
//! `A{..} + B{..}` for a fusion, `! {..}` for modifiers on the whole, and `-` for "not a twist".
//! `@hold <power id> | phrase => …` reads as a holder of that power. `~` before a phrase marks a
//! spoken (Whisper-style) line. Re-bless after a deliberate change: `TWIST_BLESS=1 cargo test -p
//! bd_sim --test twist golden` (then READ the diff).

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Mechanic};
use bd_core::{PlayerId, Vec3, TICK_HZ};
use bd_sim::events::GameEvent;
use bd_sim::powers::chain::{self, ToolStep};
use bd_sim::powers::ops::OpEvent;
use bd_sim::powers::toolbox::{MoveIndex, MoveRef, Reading};
use bd_sim::powers::twist::fuse::{fuse, Profile};
use bd_sim::powers::twist::ops::{gather, rescale, twist, Scales, Twisted};
use bd_sim::powers::twist::read::{judge_of, Core, Lex, Reader, SideReading, TwistReading};
use bd_sim::powers::twist::{exec, ModTable, TwistBase, TwistSide, TwistStep, SHIPPED_RON};
use bd_sim::powers::{norm_words, At, MoveDef, MoveOp, PlayerPower, PowerError, PowerEvent, PowerPhrase, PowerTable};
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn t() -> &'static ModTable {
    ModTable::shipped()
}

fn lex() -> &'static Option<bd_lexicon::Lexicon> {
    static L: std::sync::OnceLock<Option<bd_lexicon::Lexicon>> = std::sync::OnceLock::new();
    L.get_or_init(|| {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/lexicon.bin");
        bd_lexicon::Lexicon::load(p).ok()
    })
}

/// The lexicon judge the game uses.
fn judge(s: &str) -> Lex {
    let Some(l) = lex().as_ref() else { return Lex::Other };
    let r = l.resolve(s);
    judge_of(r.signature.is_some(), r.kind == bd_lexicon::ResolutionKind::Construct, r.shape.is_some(), r.confidence, &r.recipe.name, s)
}

fn reader<'a>(held: Option<&'a PlayerPower>, judge: &'a dyn Fn(&str) -> Lex) -> Reader<'a> {
    Reader { t: t(), powers: PowerTable::get(), ix: MoveIndex::shipped(), held, judge, compose: Some(&described) }
}

/// The described-move reader the console uses (VERB GRAMMAR + the lexicon's curated filter), with
/// the preview's sample roster.
fn described(s: &str) -> Option<bd_sim::powers::compose::ComposedMove> {
    use bd_sim::powers::compose;
    let roster = [(PlayerId(2), "Ethan"), (PlayerId(3), "Mia")];
    let names = compose::Names { players: &roster, fuzzy: None };
    let r = compose::read(s, &names)?;
    let Some(l) = lex().as_ref() else { return Some(r.spec) };
    let res = l.resolve(s);
    (!compose::curated_by_lexicon(s, res.signature.is_some(), res.recipe.grants.is_some() || res.stand_rush.is_some(), &res.recipe.model)).then_some(r.spec)
}

fn mod_name(n: u8) -> String {
    t().get(n).map_or(format!("?{n}"), |m| m.id.clone())
}

fn mods_str(m: &[u8]) -> String {
    if m.is_empty() {
        return String::new();
    }
    let mut v: Vec<String> = m.iter().map(|n| mod_name(*n)).collect();
    v.sort();
    format!("{{{}}}", v.join(","))
}

fn core_name(c: &Core, held: Option<&PlayerPower>) -> String {
    let table = PowerTable::get();
    match c {
        Core::Cast(s) => format!("cast:{s}"),
        Core::Composed(spec) => format!("described:{}", bd_sim::powers::twist::split_composed(*spec).0.name()),
        Core::Move(Reading::Kit(m) | Reading::Borrow(m)) => m.resolve(table).map_or("?".into(), |(_, d)| d.name.clone()),
        Core::Move(Reading::Held(PowerPhrase::Move(sel))) => held
            .and_then(|h| exec::held_move(h, *sel))
            .map_or("?held".into(), |(_, _, d)| d.name.clone()),
        Core::Move(r) => format!("{r:?}"),
    }
}

fn side_str(s: &SideReading, held: Option<&PlayerPower>) -> String {
    // a described core's own bytes are applied by the twist too (moved out of the spec)
    let mut mods = match &s.core {
        Core::Composed(spec) => bd_sim::powers::twist::split_composed(*spec).1,
        _ => vec![],
    };
    mods.extend(s.mods.iter().copied());
    format!("{}{}", core_name(&s.core, held), mods_str(&mods))
}

fn reading_str(r: &Option<TwistReading>, held: Option<&PlayerPower>) -> String {
    match r {
        None => "-".into(),
        Some(r) => {
            let mut s = side_str(&r.a, held);
            if let Some(b) = &r.b {
                s.push_str(" + ");
                s.push_str(&side_str(b, held));
            }
            if !r.mods.is_empty() {
                s.push_str(" ! ");
                s.push_str(&mods_str(&r.mods));
            }
            s
        }
    }
}

fn holder(id: &str) -> PlayerPower {
    let k = PowerTable::get().index_of(id).unwrap_or_else(|| panic!("no power {id}"));
    PlayerPower { id: Some(k), ..Default::default() }
}

// ------------------------------------------------------------------------------ golden

#[test]
fn golden_phrases_read_as_expected() {
    if lex().is_none() {
        eprintln!("no lexicon.bin: skipped");
        return;
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/twist_golden.txt");
    let src = std::fs::read_to_string(&path).expect("golden file");
    let bless = std::env::var("TWIST_BLESS").is_ok();
    let mut out = String::new();
    let mut bad = Vec::new();
    let (mut n, mut spoken, mut fused, mut twisted) = (0, 0, 0, 0);
    let t0 = std::time::Instant::now();
    for line in src.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let (lhs, want) = l.split_once("=>").map(|(a, b)| (a.trim(), b.trim())).unwrap_or((l, "?"));
        let (hold, phrase) = match lhs.strip_prefix("@hold ") {
            Some(rest) => {
                let (id, p) = rest.split_once('|').expect("@hold id | phrase");
                (Some(holder(id.trim())), p.trim())
            }
            None => (None, lhs),
        };
        let said = phrase.trim_start_matches('~').trim();
        if phrase.starts_with('~') {
            spoken += 1;
        }
        let r = reader(hold.as_ref(), &judge).read(said);
        let got = reading_str(&r, hold.as_ref());
        n += 1;
        if r.as_ref().is_some_and(|r| r.fused()) {
            fused += 1;
        } else if r.is_some() {
            twisted += 1;
        }
        if got != want {
            bad.push(format!("{lhs:<52} => {got:<40} (want {want})"));
        }
        out.push_str(&format!("{lhs} => {got}\n"));
    }
    let per = t0.elapsed().as_secs_f64() * 1000.0 / n.max(1) as f64;
    eprintln!("twist golden: {n} phrases ({spoken} spoken; {twisted} twisted, {fused} fused), {per:.2} ms/phrase");
    if bless {
        std::fs::write(&path, out).expect("bless");
        return;
    }
    assert!(n >= 250, "at least 250 golden phrases ({n})");
    assert!(bad.is_empty(), "{} golden mismatches:\n{}", bad.len(), bad.join("\n"));
}

/// Every modifier word × a few cores reads back as exactly that modifier on that core (typed and
/// in a "but …" tail): the vocabulary itself is covered, not just the golden samples.
#[test]
fn every_modifier_word_twists_a_move() {
    let cores = ["chidori", "almighty push", "earthshatter"];
    let other = |_: &str| Lex::Other;
    let r = reader(None, &other);
    let mut bad = Vec::new();
    let mut n = 0;
    for m in &t().modifiers {
        for w in &m.words {
            for core in cores {
                // prefix and tail forms
                for phrase in [format!("{w} {core}"), format!("{core} but {w}")] {
                    n += 1;
                    let got = r.read(&phrase);
                    let ok = got.as_ref().is_some_and(|g| g.b.is_none() && g.a.mods.contains(&m.n) && norm_words(&g.a.core_text) == norm_words(core));
                    if !ok {
                        bad.push(format!("{phrase:?} → {}", reading_str(&got, None)));
                    }
                }
            }
        }
    }
    eprintln!("{n} modifier-word readings, {} bad", bad.len());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

// ------------------------------------------------------------------------------ data

#[test]
fn the_table_parses_with_unique_bytes_and_words() {
    let parsed = ModTable::parse(SHIPPED_RON).expect("move_modifiers.ron parses");
    let raw: ron::Value = ron::from_str(SHIPPED_RON).expect("raw");
    let _ = raw;
    assert!(parsed.modifiers.len() >= 60, "{} modifiers", parsed.modifiers.len());
    // no modifier was dropped for a bad / reused byte (sanitize drops them)
    let mut bytes: Vec<u8> = parsed.modifiers.iter().map(|m| m.n).collect();
    let n = bytes.len();
    bytes.sort();
    bytes.dedup();
    assert_eq!(bytes.len(), n);
    assert!(!bytes.contains(&0));
    // VERB GRAMMAR's starter ids keep their meaning
    assert_eq!(parsed.id_of("giant"), Some(1));
    assert_eq!(parsed.id_of("quick"), Some(2));
    assert_eq!(parsed.id_of("long"), Some(3));
    // a phrase belongs to one modifier only
    let mut seen = std::collections::HashMap::new();
    let mut dup = Vec::new();
    for m in &parsed.modifiers {
        for w in &m.words {
            if let Some(o) = seen.insert(w.clone(), m.id.clone())
                && o != m.id
            {
                dup.push(format!("{w:?}: {o} and {}", m.id));
            }
        }
    }
    assert!(dup.is_empty(), "words claimed twice:\n{}", dup.join("\n"));
    // counts name real modifiers; the bang modifier exists
    for (_, id) in &parsed.counts {
        assert!(parsed.id_of(id).is_some(), "count {id}");
    }
    assert!(parsed.bang().is_some());
    // every element look is a real style key (assets/tuning/move_fx.ron)
    let fx = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tuning/move_fx.ron")).unwrap_or_default();
    if !fx.is_empty() {
        for k in parsed.elements.iter().map(|e| e.fx.clone()).chain(parsed.modifiers.iter().map(|m| m.fx.clone())).filter(|k| !k.is_empty()) {
            assert!(fx.contains(&format!("\"{k}\"")), "style key {k:?} is not in move_fx.ron");
        }
    }
}

// ------------------------------------------------------------------------------ transforms

fn mv(ops: Vec<MoveOp>) -> MoveDef {
    MoveDef { name: "test move".into(), cooldown: 4.0, cost: 6.0, ops, ..Default::default() }
}

fn byte(id: &str) -> u8 {
    t().id_of(id).unwrap_or_else(|| panic!("modifier {id}"))
}

fn explode(radius: f32, damage: f32) -> MoveOp {
    MoveOp::Explode { radius, damage, knockback: 20.0, carve: 1.0, element: Element::Light, at: At::Front, dist: 3.0 }
}

#[test]
fn size_scales_every_size_role_and_caps_hold() {
    let d = mv(vec![explode(4.0, 10.0), MoveOp::Shove { range: 3.0, angle: 40.0, damage: 8.0, knockback: 10.0, element: Element::Light, up: 0.0 }]);
    let tw = twist(t(), &d, &[byte("giant")]);
    let MoveOp::Explode { radius, damage, dist, .. } = tw.def.ops[0] else { panic!() };
    assert!((radius - 4.0 * 1.8).abs() < 1e-3, "{radius}");
    assert!((damage - 13.0).abs() < 1e-3, "{damage}");
    assert!((dist - 3.0).abs() < 1e-6, "placement is Fixed");
    let MoveOp::Shove { range, angle, .. } = tw.def.ops[1] else { panic!() };
    assert!((range - 3.9).abs() < 1e-3 && (angle - 40.0).abs() < 1e-6, "reach grows, the arc doesn't");
    assert!(tw.def.cooldown > d.cooldown, "bigger = longer cooldown");
    assert_eq!(tw.title, "GIANT TEST MOVE");
    assert!(tw.def.juice.as_ref().is_some_and(|j| j.boom));
    // stacking is capped: giant × colossal × giant × giant never passes size_max
    let tw = twist(t(), &d, &[byte("giant"), byte("colossal"), byte("giant"), byte("giant")]);
    let MoveOp::Explode { radius, damage, .. } = tw.def.ops[0] else { panic!() };
    assert!(radius <= 4.0 * t().caps.size_max + 1e-3, "{radius}");
    assert!(damage <= 10.0 * t().caps.damage_max + 1e-3, "{damage}");
    // mini shrinks
    let tw = twist(t(), &d, &[byte("mini")]);
    let MoveOp::Explode { radius, .. } = tw.def.ops[0] else { panic!() };
    assert!(radius < 4.0);
}

#[test]
fn crowd_control_never_lasts_longer() {
    let d = mv(vec![
        MoveOp::GazeStun { range: 12.0, cone: 30.0, secs: 1.0, status: bd_sim::status::StatusKind::Stun, aimed: false },
        MoveOp::TimeStop { secs: 2.0, speed: 2.0 },
        MoveOp::Zone { radius: 4.0, secs: 3.0, status: None, slow: 0.0, dps: 2.0, lockout: false, at: At::Me, dist: 0.0, follow: false },
    ]);
    let tw = twist(t(), &d, &[byte("slow"), byte("charged"), byte("lingering")]);
    let MoveOp::GazeStun { secs, .. } = tw.def.ops[0] else { panic!() };
    assert!(secs <= 1.0 + 1e-6, "stun {secs}");
    let MoveOp::TimeStop { secs, speed } = tw.def.ops[1] else { panic!() };
    assert!(secs <= 2.0 + 1e-6 && (speed - 2.0).abs() < 1e-6, "time stop {secs} {speed}");
    let MoveOp::Zone { secs, .. } = tw.def.ops[2] else { panic!() };
    assert!(secs > 3.0, "an ordinary area may last longer ({secs})");
}

#[test]
fn element_swap_reskins_and_reflavours() {
    let d = mv(vec![
        MoveOp::Shove { range: 3.0, angle: 40.0, damage: 8.0, knockback: 10.0, element: Element::Shadow, up: 0.0 },
        MoveOp::Contagion { range: 30.0, cone: 6.0, status: bd_sim::status::StatusKind::Burn, secs: 7.0, spread: 3.0 },
        MoveOp::Cast {
            recipe: bd_sim::powers::RecipeDef { name: "orb".into(), element: Element::Water, ..Default::default() },
            modifier: false,
            signature: Some(bd_sim::signature::SignatureKind::Beam),
            merge: Some((Element::Water, Element::Fire)),
        },
    ]);
    let tw = twist(t(), &d, &[byte("ice")]);
    assert!(matches!(tw.def.ops[0], MoveOp::Shove { element: Element::Ice, .. }));
    assert!(matches!(tw.def.ops[1], MoveOp::Contagion { status: bd_sim::status::StatusKind::Chill, .. }), "amaterasu, but ice: it chills");
    let MoveOp::Cast { recipe, merge, .. } = &tw.def.ops[2] else { panic!() };
    assert_eq!(recipe.element, Element::Ice);
    assert_eq!(*merge, Some((Element::Ice, Element::Ice)), "the windup's orbs are ice too");
    assert_eq!(tw.def.fx, "ice");
    assert_eq!(tw.element, Some(Element::Ice));
    // a stun is never swapped (crowd control stays what it was)
    let d = mv(vec![MoveOp::GazeStun { range: 12.0, cone: 30.0, secs: 1.0, status: bd_sim::status::StatusKind::Stun, aimed: false }]);
    let tw = twist(t(), &d, &[byte("fire")]);
    assert!(matches!(tw.def.ops[0], MoveOp::GazeStun { status: bd_sim::status::StatusKind::Stun, .. }));
}

#[test]
fn counts_fan_instant_ops_and_stream_stateful_ones() {
    // instant blast: three at once, fanned, each a little weaker
    let d = mv(vec![explode(4.0, 20.0)]);
    let tw = twist(t(), &d, &[byte("triple")]);
    assert_eq!(tw.fan.len(), 3);
    assert_eq!(tw.shots.len(), 1);
    assert!(tw.fan.iter().any(|y| *y > 0.0) && tw.fan.iter().any(|y| *y < 0.0));
    let MoveOp::Explode { damage, .. } = tw.def.ops[0] else { panic!() };
    assert!(damage < 20.0, "a triple isn't three full moves ({damage})");
    // a dash: three quick shots in a row
    let d = mv(vec![MoveOp::Ram { speed: 30.0, secs: 0.4, damage: 12.0, knockback: 20.0, up: 0.0, pin: false, through: false, wall: 0.0, pitch: 0.0, steer: 0.0, keep: 0.0, element: Element::Light }]);
    let tw = twist(t(), &d, &[byte("triple")]);
    assert_eq!((tw.fan.len(), tw.shots.len()), (1, 3));
    // an intrinsic count multiplies instead
    let d = mv(vec![MoveOp::Salvo { count: 3, delay: 0.5, interval: 0.1, radius: 3.0, damage: 10.0, knockback: 8.0, element: Element::Light, carve: 0.0, up: 0.0, at: At::Aim, dist: 0.0, spread: 4.0, seek: false, track: false, cut: false }]);
    let tw = twist(t(), &d, &[byte("triple")]);
    assert_eq!((tw.fan.len(), tw.shots.len()), (1, 1));
    assert!(matches!(tw.def.ops[0], MoveOp::Salvo { count: 9, .. }), "{:?}", tw.def.ops[0]);
    // a palm beam fires more pairs of hands
    let d = mv(vec![MoveOp::Cast {
        recipe: bd_sim::powers::RecipeDef { name: "kamehameha".into(), form: bd_core::mechanics::Form::Hands, mech: vec![Mechanic::Beam], ..Default::default() },
        modifier: false,
        signature: Some(bd_sim::signature::SignatureKind::Beam),
        merge: None,
    }]);
    let tw = twist(t(), &d, &[byte("triple")]);
    assert_eq!(tw.hands, 3);
    assert!(matches!(&tw.def.ops[0], MoveOp::Cast { recipe, .. } if recipe.count == 3));
    // x10 = a capped stream
    let d = mv(vec![explode(3.0, 10.0)]);
    let tw = twist(t(), &d, &[byte("x10")]);
    assert_eq!(tw.shots.len(), t().caps.repeat_max as usize);
    let MoveOp::Explode { damage, .. } = tw.def.ops[0] else { panic!() };
    assert!(damage * tw.shots.len() as f32 <= 10.0 * 4.0, "a stream's total stays bounded ({damage} × {})", tw.shots.len());
}

#[test]
fn behaviours_become_the_right_verbs() {
    // homing chidori: dash + stab → a homing lunge that strikes on contact
    let chidori = mv(vec![MoveOp::Impulse { fwd: 26.0, up: 1.0 }, MoveOp::Shove { range: 3.0, angle: 40.0, damage: 14.0, knockback: 16.0, element: Element::Electric, up: 0.0 }]);
    let tw = twist(t(), &chidori, &[byte("homing")]);
    assert!(matches!(tw.def.ops[..], [MoveOp::Pounce { damage, element: Element::Electric, .. }] if damage > 13.0), "{:?}", tw.def.ops);
    // delayed blast → a planted strike
    let tw = twist(t(), &mv(vec![explode(5.0, 12.0)]), &[byte("delayed")]);
    assert!(matches!(tw.def.ops[0], MoveOp::Salvo { count: 1, delay, .. } if delay > 1.0), "{:?}", tw.def.ops);
    // delayed strike → a mark that bursts later
    let tw = twist(t(), &mv(vec![MoveOp::Shove { range: 3.0, angle: 40.0, damage: 14.0, knockback: 16.0, element: Element::Light, up: 0.0 }]), &[byte("delayed")]);
    assert!(matches!(tw.def.ops[0], MoveOp::DelayedHit { .. }));
    // reverse: a blast implodes (a pull first), a field flips, a push yanks
    let tw = twist(t(), &mv(vec![explode(6.0, 10.0), MoveOp::Field { strength: -60.0, radius: 8.0, secs: 0.4, at: At::Me, dist: 0.0 }]), &[byte("reverse")]);
    assert!(matches!(tw.def.ops[0], MoveOp::Field { strength, .. } if strength > 0.0));
    assert!(matches!(tw.def.ops[1], MoveOp::Explode { knockback, .. } if knockback < 20.0));
    assert!(matches!(tw.def.ops[2], MoveOp::Field { strength, .. } if strength > 0.0), "{:?}", tw.def.ops);
    // exploding strike leaves a blast; spinning strike goes all round; piercing ram goes through
    let strike = mv(vec![MoveOp::Shove { range: 3.0, angle: 40.0, damage: 14.0, knockback: 16.0, element: Element::Light, up: 0.0 }]);
    assert!(twist(t(), &strike, &[byte("exploding")]).def.ops.iter().any(|o| matches!(o, MoveOp::Explode { .. })));
    assert!(matches!(twist(t(), &strike, &[byte("spinning")]).def.ops[0], MoveOp::Shove { angle, .. } if angle >= 179.0));
    let ram = mv(vec![MoveOp::Ram { speed: 30.0, secs: 0.4, damage: 12.0, knockback: 20.0, up: 0.0, pin: false, through: false, wall: 0.0, pitch: 0.0, steer: 0.0, keep: 0.0, element: Element::Light }]);
    assert!(matches!(twist(t(), &ram, &[byte("piercing")]).def.ops[0], MoveOp::Ram { through: true, .. }));
    // chain: the hit arcs to everyone near you; lingering: a zone where it lands
    assert!(twist(t(), &strike, &[byte("chain_lightning")]).def.ops.iter().any(|o| matches!(o, MoveOp::DelayedHit { radius, element: Element::Electric, .. } if *radius > 0.0)));
    let tw = twist(t(), &mv(vec![explode(5.0, 12.0)]), &[byte("fire"), byte("lingering")]);
    assert!(tw.def.ops.iter().any(|o| matches!(o, MoveOp::Zone { status: Some(bd_sim::status::StatusKind::Burn), at: At::Front, .. })), "{:?}", tw.def.ops);
    // casts carry the mechanics
    let orb = mv(vec![MoveOp::Cast { recipe: bd_sim::powers::RecipeDef { name: "orb".into(), ..Default::default() }, modifier: false, signature: None, merge: None }]);
    let tw = twist(t(), &orb, &[byte("homing"), byte("bouncing"), byte("sticky")]);
    let MoveOp::Cast { recipe, .. } = &tw.def.ops[0] else { panic!() };
    for m in [Mechanic::Homing, Mechanic::Ricochet, Mechanic::Sticky] {
        assert!(recipe.mech.contains(&m), "{m:?}");
    }
    // silent / invisible lose the fanfare
    let loud = mv(vec![explode(5.0, 12.0)]);
    let tw = twist(t(), &loud, &[byte("silent")]);
    assert!(tw.def.juice.as_ref().is_some_and(|j| j.title.is_none() && !j.boom));
}

#[test]
fn roles_rescale_ops_nobody_taught_it_about() {
    // the rescaler reads ANY op's serialised form: here a hand-written future op
    let src = "Frobnicate(radius:2.0,damage:10.0,secs:1.0,element:Fire,status:Some(Burn),count:3,name:\"x: 1.0\")";
    let s = Scales { size: 2.0, reach: 1.0, damage: 1.5, knock: 1.0, speed: 1.0, secs: 1.2, cone: 1.0, arc: 1.0, count: 1, element: Some(Element::Ice), status: Some(bd_sim::status::StatusKind::Chill) };
    let out = rescale(t(), src, &s);
    assert_eq!(out, "Frobnicate(radius:4.0,damage:15.0,secs:1.2,element:Ice,status:Some(Chill),count:3,name:\"x: 1.0\")");
    // and the real ops round-trip unchanged under identity
    for (_, def) in PowerTable::get().powers.iter().flat_map(|p| p.moves.iter().map(move |m| (p, m))).take(600) {
        let tw = twist(t(), def, &[]);
        assert_eq!(tw.def.ops, def.ops);
    }
}

#[test]
fn twisting_every_shipped_move_is_safe_and_deterministic() {
    let all = [byte("giant"), byte("homing"), byte("ice"), byte("triple"), byte("reverse"), byte("delayed"), byte("chain"), byte("lingering"), byte("exploding")];
    let mut n = 0;
    for p in &PowerTable::get().powers {
        for m in &p.moves {
            for k in 0..all.len() {
                let mods = [all[k], all[(k + 3) % all.len()]];
                let a = twist(t(), m, &mods);
                let b = twist(t(), m, &mods);
                assert_eq!(a, b, "deterministic: {}", m.name);
                assert!(!a.def.ops.is_empty() || m.ops.is_empty(), "{}", m.name);
                assert!(a.def.ops.len() <= 12);
                assert!(a.total() >= 1 && a.total() <= 3 * t().caps.repeat_max as u32);
                n += 1;
            }
        }
    }
    eprintln!("{n} twists of shipped moves");
}

// ------------------------------------------------------------------------------ fusion

fn shipped_move(shout: &str) -> (MoveRef, &'static MoveDef) {
    let m = MoveIndex::shipped().shout(&norm_words(shout)).unwrap_or_else(|| panic!("no shout {shout:?}"));
    (m, m.resolve(PowerTable::get()).unwrap().1)
}

fn plain(d: &MoveDef) -> Twisted {
    Twisted::plain(d)
}

#[test]
fn fusion_rules_by_what_the_moves_are() {
    let orb = |name: &str, el: Element, mech: Vec<Mechanic>| {
        mv(vec![MoveOp::Cast {
            recipe: bd_sim::powers::RecipeDef { name: name.into(), element: el, mech, scale: 1.0, ..Default::default() },
            modifier: false,
            signature: None,
            merge: None,
        }])
    };
    // two projectiles → one orb, both behaviours, a Merge windup (the Hollow Purple grammar)
    let red = orb("red", Element::Fire, vec![Mechanic::Repel]);
    let blue = orb("blue", Element::Water, vec![Mechanic::Attract, Mechanic::Pierce]);
    let f = fuse(t(), &plain(&red), &plain(&blue));
    let casts: Vec<_> = f.def.ops.iter().filter(|o| matches!(o, MoveOp::Cast { .. })).collect();
    assert_eq!(casts.len(), 1, "{:?}", f.def.ops);
    let MoveOp::Cast { recipe, merge, signature, .. } = casts[0] else { panic!() };
    assert_eq!(*merge, Some((Element::Fire, Element::Water)));
    assert!(recipe.mech.contains(&Mechanic::Repel) && recipe.mech.contains(&Mechanic::Attract) && recipe.mech.contains(&Mechanic::Pierce));
    assert!(recipe.scale > 1.0);
    assert_eq!(*signature, Some(bd_sim::signature::SignatureKind::Beam), "a piercing merged orb is an erasing orb");
    assert_eq!(f.title, "TEST MOVE + TEST MOVE");
    assert!(f.tier >= 1);
    // a blink + a blast: the blast arrives with you
    let blink = mv(vec![MoveOp::Blink { dist: 12.0, afterimage: true }]);
    let f = fuse(t(), &plain(&blink), &plain(&mv(vec![explode(5.0, 12.0)])));
    assert!(matches!(f.def.ops[..], [MoveOp::Blink { .. }, MoveOp::Explode { .. }]), "{:?}", f.def.ops);
    // a leap + a strike: delivered as a landing slam
    let leap = mv(vec![MoveOp::Leap { range: 20.0, apex: 6.0 }]);
    let f = fuse(t(), &plain(&leap), &plain(&mv(vec![MoveOp::Shove { range: 3.0, angle: 40.0, damage: 14.0, knockback: 16.0, element: Element::Light, up: 0.0 }])));
    assert!(f.def.ops.iter().any(|o| matches!(o, MoveOp::SlamOnLand { damage, .. } if *damage >= 14.0)), "{:?}", f.def.ops);
    // a dash + a strike: a homing lunge-strike
    let dash = mv(vec![MoveOp::Impulse { fwd: 24.0, up: 0.5 }]);
    let f = fuse(t(), &plain(&dash), &plain(&mv(vec![MoveOp::Shove { range: 3.0, angle: 40.0, damage: 14.0, knockback: 16.0, element: Element::Electric, up: 0.0 }])));
    assert!(matches!(f.def.ops[0], MoveOp::Pounce { element: Element::Electric, .. }), "{:?}", f.def.ops);
    // a dash + a projectile: the projectile fires when the dash ends
    let f = fuse(t(), &plain(&dash), &plain(&red));
    assert!(f.after.as_ref().is_some_and(|(w, ops)| *w > 0.0 && matches!(ops[0], MoveOp::Cast { .. })), "{:?}", f.after);
    // a field + anything: the thing leaves the field where it lands
    let field = mv(vec![MoveOp::Field { strength: 40.0, radius: 6.0, secs: 2.0, at: At::Me, dist: 0.0 }]);
    let f = fuse(t(), &plain(&red), &plain(&field));
    assert!(f.def.ops.iter().any(|o| matches!(o, MoveOp::Field { at: At::Aim, .. })), "{:?}", f.def.ops);
    // a projectile + a strike: an imbued strike (element + behaviours)
    let bomb = orb("bomb", Element::Fire, vec![Mechanic::Explodes]);
    let punch = mv(vec![MoveOp::Shove { range: 3.0, angle: 40.0, damage: 10.0, knockback: 16.0, element: Element::Light, up: 0.0 }]);
    let f = fuse(t(), &plain(&punch), &plain(&bomb));
    assert!(matches!(f.def.ops[0], MoveOp::Shove { element: Element::Fire, damage, .. } if damage > 10.0), "{:?}", f.def.ops);
    assert!(f.def.ops.iter().any(|o| matches!(o, MoveOp::Explode { .. })));
    assert!(!f.def.ops.iter().any(|o| matches!(o, MoveOp::Cast { .. })), "the projectile became the strike");
    // the shipped owner examples: rasengan + amaterasu is a burning rasengan
    let (_, ras) = shipped_move("rasengan");
    let (_, ama) = shipped_move("amaterasu");
    let f = fuse(t(), &plain(ras), &plain(ama));
    let p = Profile::of(t(), &f.def);
    assert!(p.melee, "{:?}", f.def.ops);
    assert!(f.def.ops.iter().any(|o| matches!(o, MoveOp::Drill { element: Element::Fire, .. })), "the rasengan takes the black flames' element: {:?}", f.def.ops);
    assert!(f.def.ops.iter().any(|o| matches!(o, MoveOp::Contagion { .. })), "and brands whoever it hits");
    assert!(f.def.cooldown >= ras.cooldown.max(ama.cooldown));
    // a pull + a push (no casts): the opposites collide into an erasing orb — blue + red IS hollow purple
    let (_, blue) = shipped_move("lapse blue");
    let (_, red) = shipped_move("reversal red");
    let f = fuse(t(), &plain(blue), &plain(red));
    assert!(
        matches!(&f.def.ops[..], [MoveOp::Cast { recipe, signature: Some(bd_sim::signature::SignatureKind::Beam), merge: Some(_), .. }] if recipe.mech.contains(&Mechanic::Pierce) && recipe.mech.contains(&Mechanic::Attract)),
        "{:?}",
        f.def.ops
    );
    assert_eq!(f.how, "opposites collide: an erasing orb");
    assert!(f.def.cooldown >= t().fusion.orb_cooldown, "an erasing orb is an ultimate, not a spam combo ({})", f.def.cooldown);
    // a leap with its own slam + a blast: one bigger landing slam
    let (_, slam) = shipped_move("seismic slam");
    let (_, push) = shipped_move("almighty push");
    let f = fuse(t(), &plain(slam), &plain(push));
    let slams: Vec<_> = f.def.ops.iter().filter(|o| matches!(o, MoveOp::SlamOnLand { .. })).collect();
    assert_eq!(slams.len(), 1, "{:?}", f.def.ops);
    assert!(matches!(slams[0], MoveOp::SlamOnLand { knockback, .. } if *knockback >= 34.0), "the push's force lands with you");
    assert!(!f.def.ops.iter().any(|o| matches!(o, MoveOp::Explode { .. })), "no blast at take-off");
    // two lunges: one straight into the other
    let lunge = mv(vec![MoveOp::Ram { speed: 30.0, secs: 0.4, damage: 12.0, knockback: 20.0, up: 0.0, pin: false, through: false, wall: 0.0, pitch: 0.0, steer: 0.0, keep: 0.0, element: Element::Light }]);
    let f = fuse(t(), &plain(&lunge), &plain(&mv(vec![MoveOp::Drill { speed: 24.0, secs: 0.4, grind: 0.4, damage: 15.0, knockback: 28.0, element: Element::Wind, up: 0.3 }])));
    assert!(matches!(f.def.ops[..], [MoveOp::Ram { .. }]) && f.after.as_ref().is_some_and(|(_, o)| matches!(o[..], [MoveOp::Drill { .. }])), "{:?} / {:?}", f.def.ops, f.after);
}

#[test]
fn a_delayed_exploding_cast_bursts_on_them_later() {
    let bomb = mv(vec![MoveOp::Cast {
        recipe: bd_sim::powers::RecipeDef { name: "pulse bomb".into(), mech: vec![Mechanic::Sticky, Mechanic::Explodes, Mechanic::Fast], element: Element::Electric, scale: 0.45, ..Default::default() },
        modifier: false,
        signature: None,
        merge: None,
    }]);
    let tw = twist(t(), &bomb, &[byte("delayed")]);
    let MoveOp::Cast { recipe, .. } = &tw.def.ops[0] else { panic!() };
    assert!(recipe.mech.contains(&Mechanic::Sticky) && !recipe.mech.contains(&Mechanic::Explodes), "the bomb is the sticky marker now");
    assert!(tw.def.ops.iter().any(|o| matches!(o, MoveOp::DelayedHit { secs, element: Element::Electric, .. } if *secs > 1.0)), "{:?}", tw.def.ops);
}

// ------------------------------------------------------------------------------ pricing

#[test]
fn modifiers_push_the_price_up_the_tiers_and_caps_hold() {
    let sp = bd_sim::willpower::SparkTuning::default();
    let bar = 8;
    let d = mv(vec![explode(4.0, 10.0)]);
    let base = exec::price(&sp, &twist(t(), &d, &[]), 0, bar).0;
    let giant = exec::price(&sp, &twist(t(), &d, &[byte("giant")]), 0, bar).0;
    let colossal = exec::price(&sp, &twist(t(), &d, &[byte("colossal")]), 0, bar).0;
    let triple = exec::price(&sp, &twist(t(), &d, &[byte("triple")]), 0, bar).0;
    let mini = exec::price(&sp, &twist(t(), &d, &[byte("mini")]), 0, bar).0;
    assert!(giant > base, "giant {giant} > {base}");
    assert!(colossal > giant, "colossal {colossal} > {giant}");
    assert!(triple > base, "triple = +count rule ({triple} vs {base})");
    assert!(mini <= base, "mini {mini} ≤ {base}");
    // never Signature, never more than the bar
    let all = [byte("colossal"), byte("giant"), byte("triple"), byte("charged"), byte("homing"), byte("chain"), byte("lingering"), byte("exploding")];
    let max = exec::price(&sp, &twist(t(), &d, &all), 0, bar).0;
    assert!(max <= bar);
    let tw = twist(t(), &d, &all);
    let tier = chain::move_tier(&tw.def).unwrap();
    assert!(tier.index() + (tw.tier.max(0) as usize) >= 3);
    // staleness still counts
    assert_eq!(exec::price(&sp, &twist(t(), &d, &[byte("giant")]), 2, bar).0, (giant + 2).min(bar));
    // a cast-only move: its casts pay the pipeline; the twist adds a surcharge
    let orb = mv(vec![MoveOp::Cast { recipe: Default::default(), modifier: false, signature: None, merge: None }]);
    let (ops, extra) = exec::price(&sp, &twist(t(), &orb, &[byte("giant"), byte("homing")]), 0, bar);
    assert_eq!(ops, 0);
    assert!(extra >= 2, "{extra}");
    // fusion costs at least the pricier half plus the fusion bump
    let (_, push) = shipped_move("almighty push");
    let (_, bamf) = shipped_move("bamf");
    let f = fuse(t(), &plain(bamf), &plain(push));
    let pf = exec::price(&sp, &f, 0, bar).0;
    let pp = exec::price(&sp, &plain(push), 0, bar).0;
    assert!(pf >= pp, "fusion {pf} ≥ push {pp}");
}

// ------------------------------------------------------------------------------ the host

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
    stand(&mut w, B, 0.0, -5.0);
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

fn ticks(secs: f32) -> u32 {
    (secs * TICK_HZ as f32).ceil() as u32
}

fn step_of(shout: &str, mods: &[&str]) -> TwistStep {
    let (m, _) = shipped_move(shout);
    TwistStep { a: TwistSide { base: TwistBase::Move { mv: m, kit: false }, mods: mods.iter().map(|s| byte(s)).collect() }, b: None, mods: vec![], shot: 0 }
}

fn explosions(evs: &[GameEvent]) -> Vec<f32> {
    evs.iter()
        .filter_map(|e| match e {
            GameEvent::Explosion { radius, source: Some(A), .. } => Some(*radius),
            _ => None,
        })
        .collect()
}

#[test]
fn a_giant_twist_fires_bigger_and_cools_the_base_move() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let (push, def) = shipped_move("almighty push");
    let plain_r = def.ops.iter().find_map(|o| if let MoveOp::Explode { radius, .. } = o { Some(*radius) } else { None }).unwrap();
    w.toolbox(A, vec![ToolStep::Twist(Box::new(step_of("almighty push", &["giant"])))], None).expect("giant push fires");
    let evs = run(&mut w, 2);
    let r = explosions(&evs);
    assert!(r.iter().any(|x| *x > plain_r * 1.5), "{r:?} vs {plain_r}");
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Fired { mv, .. }) if *mv == push)));
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Op(OpEvent::Twisted { title, size, .. })) if title.contains("GIANT") && *size > 1.5)));
    // the BASE move is cooling now: plain "almighty push" is refused too (once the active phase is over)
    run(&mut w, ticks(1.0));
    assert_eq!(w.borrow_move(A, push, false, None), Err(PowerError::Cooldown));
    assert!(w.player(A).unwrap().power.id.is_none(), "a twist never grants");
}

#[test]
fn a_triple_fans_three_blasts_and_rapid_fire_streams_shots() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    w.toolbox(A, vec![ToolStep::Twist(Box::new(step_of("almighty push", &["triple"])))], None).unwrap();
    let evs = run(&mut w, 2);
    assert!(explosions(&evs).len() >= 3, "{:?}", explosions(&evs));
    // rapid fire: the shots come one after another (queued follow-ups), paid once
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let before = w.player(A).unwrap().willpower;
    w.toolbox(A, vec![ToolStep::Twist(Box::new(step_of("almighty push", &["rapid"])))], None).unwrap();
    let first = explosions(&run(&mut w, 1)).len();
    let after_first = w.player(A).unwrap().willpower;
    assert!(after_first < before, "the command paid");
    let later = explosions(&run(&mut w, ticks(1.5))).len();
    assert!(first >= 1 && later >= 4, "stream: {first} then {later}");
    assert!(w.player(A).unwrap().willpower >= after_first - 1e-3, "follow-ups are free");
    assert!(w.player(A).unwrap().power.toolbox.queue.is_empty());
}

#[test]
fn a_client_cannot_send_free_follow_up_shots() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let mut s = step_of("almighty push", &["rapid"]);
    s.shot = 4;
    let before = w.player(A).unwrap().willpower;
    w.toolbox(A, vec![ToolStep::Twist(Box::new(s))], None).unwrap();
    run(&mut w, 1);
    assert!(before - w.player(A).unwrap().willpower > 0.5, "it paid: the shot index was reset");
}

#[test]
fn a_fusion_fires_both_and_cools_both() {
    let mut w = duel();
    stand(&mut w, A, 0.0, 20.0);
    w.player_mut(A).unwrap().willpower = 100.0;
    let (bamf, _) = shipped_move("bamf");
    let (push, _) = shipped_move("almighty push");
    let s = TwistStep {
        a: TwistSide { base: TwistBase::Move { mv: bamf, kit: false }, mods: vec![] },
        b: Some(TwistSide { base: TwistBase::Move { mv: push, kit: false }, mods: vec![] }),
        mods: vec![],
        shot: 0,
    };
    let from = w.player(A).unwrap().mv.pos;
    w.toolbox(A, vec![ToolStep::Twist(Box::new(s))], None).expect("bamf + push");
    let evs = run(&mut w, 2);
    assert!(w.player(A).unwrap().mv.pos.distance(from) > 3.0, "blinked");
    assert!(!explosions(&evs).is_empty(), "and the push arrived with you");
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Op(OpEvent::Twisted { b: bd_sim::powers::twist::BaseRef::Move(_), merge: Some(_), .. })))));
    run(&mut w, ticks(1.0));
    assert_eq!(w.borrow_move(A, push, false, None), Err(PowerError::Cooldown), "both base moves cool");
}

#[test]
fn held_twists_and_hash_determinism() {
    let run_once = || {
        let mut w = duel();
        w.player_mut(A).unwrap().willpower = 100.0;
        w.toolbox(A, vec![ToolStep::Twist(Box::new(step_of("almighty push", &["homing", "ice", "triple"])))], None).unwrap();
        let evs = run(&mut w, ticks(2.0));
        (format!("{:?}", evs.len()), w.player(B).unwrap().mv.pos, w.player(B).unwrap().damage_pct)
    };
    assert_eq!(run_once(), run_once());
}

#[test]
fn the_wire_round_trips() {
    let s = ToolStep::Twist(Box::new(step_of("almighty push", &["giant", "ice"])));
    let cfg = bincode::config::standard();
    let bytes = bincode::serde::encode_to_vec(&s, cfg).unwrap();
    let (back, _): (ToolStep, usize) = bincode::serde::decode_from_slice(&bytes, cfg).unwrap();
    assert_eq!(back, s);
    assert!(bytes.len() < 48, "{} bytes", bytes.len());
    // sanitising drops unknown bytes and caps the list
    let bad = TwistStep { a: TwistSide { base: TwistBase::Held(0), mods: vec![0, 255, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1] }, b: None, mods: vec![], shot: 9 }.sanitized(t());
    assert!(bad.a.mods.len() <= bd_sim::powers::twist::MAX_MODS && !bad.a.mods.contains(&0) && bad.shot == 0);
}

#[test]
fn nobody_twisting_leaves_the_world_untouched() {
    // no twist steps: the toolbox state stays idle (the FFA golden hash test guards the rest)
    let mut w = duel();
    run(&mut w, ticks(1.0));
    assert!(w.toolbox_view(A).is_none());
    let _ = gather(t(), &[]);
}

// ------------------------------------------------------------------------------ palm beams

/// A pair of hands with count 3 (the lexicon's "triple kamehameha", or the twisted borrowed one)
/// fires THREE beams at once, fanned, each a little weaker; a plain pair fires one.
#[test]
fn a_triple_kamehameha_fires_three_palm_beams() {
    use bd_core::input::{buttons, PlayerInput};
    use bd_core::mechanics::Form;
    use bd_sim::signature::{SignatureCast, SignatureKind};
    let fire = |count: u8| -> Vec<bd_sim::signature::palm::BeamView> {
        let mut w = duel();
        stand(&mut w, B, 0.0, -30.0);
        w.player_mut(A).unwrap().willpower = 100.0;
        let mut r = bd_sim::bots::recipe("Kamehameha", Form::Hands, &[Mechanic::Beam], Element::Light, 0.6);
        r.count = count;
        let p = w.player(A).unwrap();
        let aim = bd_sim::Aim::new(p.mv.pos + Vec3::Y * 1.6, Vec3::NEG_Z);
        w.cast_signature(A, r, None, aim, SignatureCast::new(SignatureKind::Beam, 1.0)).expect("hands");
        run(&mut w, ticks(0.9));
        w.player_mut(A).unwrap().willpower = 100.0;
        let hold = PlayerInput { buttons: buttons::PRIMARY, pitch: -0.02, ..Default::default() };
        let mut out = Vec::new();
        for _ in 0..ticks(2.5) {
            let evs = w.tick(&[(A, hold)]).to_vec();
            let started: Vec<_> = evs.iter().filter_map(|e| if let GameEvent::PalmBeam(v) = e { Some(*v) } else { None }).collect();
            if !started.is_empty() {
                out = started;
                break;
            }
        }
        out
    };
    let one = fire(1);
    assert_eq!(one.len(), 1, "{one:?}");
    let three = fire(3);
    assert_eq!(three.len(), 3, "three beams at once: {three:?}");
    let yaws: Vec<f32> = three.iter().map(|b| b.dir.x.atan2(-b.dir.z).to_degrees()).collect();
    assert!(yaws.iter().any(|y| *y > 4.0) && yaws.iter().any(|y| *y < -4.0), "fanned: {yaws:?}");
}

#[test]
fn the_twisted_borrowed_kamehameha_keeps_its_three_hands() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    w.toolbox(A, vec![ToolStep::Twist(Box::new(step_of("kamehameha", &["triple"])))], None).expect("triple kamehameha");
    let evs = run(&mut w, 2);
    let spawned = evs.iter().find_map(|e| match e {
        GameEvent::ConstructSpawned { owner: A, recipe, .. } if recipe.form == bd_core::mechanics::Form::Hands => Some(recipe.count),
        _ => None,
    });
    assert_eq!(spawned, Some(3), "{evs:?}");
}

// ------------------------------------------------------------------------------ described moves

fn described_step(phrase: &str) -> TwistStep {
    let cast = |_: &str| Lex::Other;
    let r = reader(None, &cast).read(phrase).unwrap_or_else(|| panic!("{phrase:?} is a twist"));
    let side = |s: &SideReading| TwistSide {
        base: match &s.core {
            Core::Composed(spec) => TwistBase::Composed(*spec),
            Core::Move(Reading::Borrow(m)) => TwistBase::Move { mv: *m, kit: false },
            Core::Move(Reading::Kit(m)) => TwistBase::Move { mv: *m, kit: true },
            c => panic!("{c:?}"),
        },
        mods: s.mods.clone(),
    };
    TwistStep { a: side(&r.a), b: r.b.as_ref().map(side), mods: r.mods.clone(), shot: 0 }.sanitized(t())
}

#[test]
fn a_twisted_described_move_goes_for_its_target_and_applies_modifiers_once() {
    // "giant punch" is the grammar's own (big) — twisted with "homing", the big byte moves to the
    // twist side and the spec keeps no modifiers (applied once)
    let s = described_step("homing giant punch at ethan");
    let spec = exec::side_spec(&s.a).expect("a described core");
    assert!(spec.mods.iter().all(|m| *m == 0), "the spec carries no modifiers: {spec:?}");
    assert!(s.a.mods.contains(&byte("giant")) && s.a.mods.contains(&byte("homing")), "{:?}", s.a.mods);
    // fired at B (Ethan = PlayerId(2) in the reader's roster), who stands well off the crosshair
    let mut w = duel();
    stand(&mut w, B, 11.0, -4.0);
    run(&mut w, 20);
    w.player_mut(A).unwrap().willpower = 100.0;
    let pct0 = w.player(B).unwrap().damage_pct;
    w.toolbox(A, vec![ToolStep::Twist(Box::new(s))], None).expect("fires");
    let evs = run(&mut w, ticks(1.5));
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Op(OpEvent::Twisted { a: bd_sim::powers::twist::BaseRef::Composed(_), .. })))));
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Fired { how: chain::FireHow::Composed, .. }))));
    assert!(w.player(B).unwrap().damage_pct > pct0, "the homing punch went for Ethan (not the crosshair)");
}

#[test]
fn fire_and_ice_is_one_move_with_both_elements() {
    let s = described_step("fire and ice punch");
    assert!(s.b.is_some(), "a twin fusion");
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let plan = w.twist_plan(0, &s).expect("plan");
    assert_eq!(plan.tw.how, "one move, both elements");
    assert_eq!(plan.tw.merge.map(|(a, b)| (a, b)), Some((Element::Fire, Element::Ice)));
    assert!(plan.tw.def.ops.iter().any(|o| matches!(o, MoveOp::Afflict { status: Some(bd_sim::status::StatusKind::Chill), .. })), "{:?}", plan.tw.def.ops);
    // the named version too
    let (_, ras) = shipped_move("rasengan");
    let f = fuse(t(), &twist(t(), ras, &[byte("fire")]), &twist(t(), ras, &[byte("ice")]));
    assert_eq!(f.how, "one move, both elements");
    assert!(f.def.ops.iter().any(|o| matches!(o, MoveOp::Drill { element: Element::Fire, .. })));
}
