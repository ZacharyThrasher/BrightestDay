//! POLISH: ANIME (docs/toolbox/polish/anime_audit.md): the famous lines now do the famous thing, every
//! move of the anime packs wears a body, the headline recipes that were reworked keep their shape.

use bd_sim::powers::toolbox::MoveIndex;
use bd_sim::powers::{MoveOp, PowerTable};

const FILES: [&str; 5] = ["anime_classics.ron", "anime_modern.ron", "base_techniques.ron", "base_one_piece.ron", "base_jojo.ron"];

/// (power id, move name) a phrase fires through the TOOLBOX index (no held power).
fn reads(phrase: &str) -> Option<(String, String)> {
    let t = PowerTable::get();
    MoveIndex::shipped().read(t, None, phrase).move_ref().map(|m| {
        let (p, mv) = m.resolve(t).expect("resolves");
        (p.id.clone(), mv.name.clone())
    })
}

fn fires(phrase: &str, power: &str, mv: &str) {
    assert_eq!(reads(phrase), Some((power.to_string(), mv.to_string())), "{phrase:?}");
}

fn mv(power: &str, tier: Option<usize>, name: &str) -> bd_sim::powers::MoveDef {
    let t = PowerTable::get();
    let p = t.powers.iter().find(|p| p.id == power).unwrap_or_else(|| panic!("{power}"));
    let list = match tier {
        None => &p.moves,
        Some(k) => &p.tiers[k].moves,
    };
    list.iter().find(|m| m.name == name).unwrap_or_else(|| panic!("{power}/{name}")).clone()
}

#[test]
fn the_famous_lines_fire_the_famous_moves() {
    // "ZA WARUDO" stops time (it used to grant the stand); "the world" is still the stand's name.
    fires("za warudo", "the_world", "za warudo");
    fires("ZA WARUDO!", "the_world", "za warudo");
    fires("toki wo tomare", "the_world", "za warudo");
    assert!(MoveIndex::shipped().is_grant_name("the world"), "the stand's own name still grants it");
    fires("road roller da", "the_world", "road roller");
    fires("za hando", "the_hand", "erase");
    fires("mugetsu", "kit_ichigo", "mugetsu");
    fires("thunderclap and flash godspeed", "breathing", "godspeed thunderclap");
    fires("godspeed", "killua_kit", "godspeed");
    fires("ka me ha me ha", "kit_goku", "kamehameha");
    fires("izanagi", "kit_obito", "izanagi");
    fires("edo tensei", "kit_orochimaru", "edo tensei");
    fires("plus ultra", "deku_kit", "one hundred percent detroit smash");
    fires("set your heart ablaze", "rengoku_kit", "rengoku");
    fires("room", "ope_ope", "room");
    fires("love train", "kit_valentine", "love train");
    fires("dirty deeds done dirt cheap", "kit_valentine", "d4c");
    fires("funny valentine ultimate", "kit_valentine", "love train");
    fires("summoning jutsu", "kit_jiraiya", "summon gamabunta");
    // Owner call: bare attack names fire the one-off move; the techniques grant from their other names.
    fires("black flash", "black_flash_technique", "black flash");
    fires("jajanken", "gon_kit", "jajanken rock");
    fires("ja jan ken", "gon_kit", "jajanken rock");
}

#[test]
fn golden_anime_chains() {
    use bd_sim::powers::toolbox::split_chain;
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let is_move = |s: &str| ix.read(t, None, s).is_move();
    let steps = |s: &str| -> Vec<Option<(String, String)>> { split_chain(s, &is_move).iter().map(|p| reads(p)).collect() };
    let m = |p: &str, n: &str| Some((p.to_string(), n.to_string()));
    // DIO's whole bit: stop time, drop the roller, rush.
    assert_eq!(steps("za warudo -> road roller da -> muda muda"), vec![m("the_world", "za warudo"), m("the_world", "road roller"), m("the_world", "muda rush")]);
    // Obito: phase through it, then undo it.
    assert_eq!(steps("kamui then izanagi"), vec![m("kit_obito", "kamui"), m("kit_obito", "izanagi")]);
    // Zenitsu's godspeed is one step, not two (the "and" is part of the name).
    assert_eq!(steps("thunderclap and flash godspeed"), vec![m("breathing", "godspeed thunderclap")]);
    // ("d4c, love train" is the move's own full name: one step; an arrow always splits.)
    assert_eq!(steps("d4c -> love train"), vec![m("kit_valentine", "d4c"), m("kit_valentine", "love train")]);
}

#[test]
fn every_anime_move_wears_a_body() {
    // A move draws in its own `fx`, else its power's (else an element's, else the default hard light):
    // every anime power / kit now names its body, so nothing falls back to the default.
    let files: Vec<(&str, &str)> = bd_sim::powers::SHIPPED_FILES.iter().copied().filter(|(n, _)| FILES.contains(n)).collect();
    assert_eq!(files.len(), FILES.len());
    let t = PowerTable::parse_files(&files).expect("anime packs parse");
    let mut bare = Vec::new();
    for p in &t.powers {
        let all = p.moves.iter().chain(p.tiers.iter().flat_map(|t| t.moves.iter()));
        for m in all {
            if m.fx.is_empty() && p.fx.is_empty() {
                bare.push(format!("{}/{}", p.id, m.name));
            }
        }
    }
    assert!(bare.is_empty(), "moves with no body: {bare:?}");
}

#[test]
fn reworked_headliners_keep_their_shape() {
    // Road Roller Da: leap to the crosshair, the roller drops where you land.
    for p in ["the_world", "kit_dio"] {
        let m = mv(p, None, "road roller");
        assert!(matches!(m.ops[0], MoveOp::Leap { .. }), "{p}: leaps first");
        assert!(matches!(m.ops[1], MoveOp::Cast { .. }), "{p}: then the roller");
    }
    // Yuji's Black Flash always sparks black, like the technique's shout.
    let bf = mv("itadori_kit", None, "black flash");
    assert!(matches!(bf.crit, Some((_, bd_sim::powers::CritFx::BlackFlash))));
    assert_eq!(bf.juice.as_ref().and_then(|j| j.impact_frame.as_deref()), Some("black_flash"));
    // Divergent Fist is two impacts from one swing.
    let df = mv("black_flash_technique", None, "divergent fist");
    assert!(df.ops.iter().any(|o| matches!(o, MoveOp::DelayedHit { .. })));
    // Kaio-ken rush: a flurry, and the body pays.
    let kr = mv("kaioken", None, "kaioken rush");
    assert!(kr.ops.iter().any(|o| matches!(o, MoveOp::Barrage { .. })) && kr.ops.iter().any(|o| matches!(o, MoveOp::Strain { .. })));
    // Each of Luffy's gears stretches its own body (red hawk / the lightning keep their element).
    for (tier, style) in [(0, "gear second"), (1, "gear third"), (2, "gear fourth"), (3, "gear fifth")] {
        let p = PowerTable::get().powers.iter().find(|p| p.id == "gomu_gomu").unwrap();
        for m in &p.tiers[tier].moves {
            if m.name != "red hawk" && m.name != "lightning grab" {
                assert_eq!(m.fx, style, "gear {} {}", tier + 2, m.name);
            }
        }
    }
    // Hollow Purple is a violet void, never the cursed flame.
    assert_eq!(mv("limitless", None, "hollow purple").fx, "void");
    assert_eq!(mv("gojo_kit", None, "hollow purple").fx, "void");
}
