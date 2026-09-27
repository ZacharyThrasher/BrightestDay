//! POLISH: WESTERN (docs/toolbox/polish/western_audit.md): every western Ultimate lands with an impact
//! frame and a boom, the headline identities wear their own bodies (eye beams from the eyes, web
//! strands, the Force, spell jets), the reworked recipes keep their shape, and the new famous lines fire.

use bd_sim::powers::toolbox::MoveIndex;
use bd_sim::powers::{MoveOp, MoveTag, PowerTable};

/// TRANSFORMERS FEEL owns these kits (tv_cartoons.ron); their drama is theirs.
const TRANSFORMERS: [&str; 17] = ["kit_optimus_prime", "kit_megatron", "kit_bumblebee", "kit_starscream", "kit_soundwave", "kit_grimlock",
    "kit_jazz", "kit_ironhide", "kit_ratchet", "kit_shockwave", "kit_devastator", "kit_unicron", "kit_arcee", "kit_ultra_magnus",
    "kit_bulkhead", "kit_wheeljack", "kit_optimus_primal"];
const FILES: [&str; 5] = ["movies_comics.ron", "tv_cartoons.ron", "base_marvel.ron", "base_dc.ron", "base_cartoons.ron"];

fn reads(phrase: &str) -> Option<(String, String)> {
    let t = PowerTable::get();
    MoveIndex::shipped().read(t, None, phrase).move_ref().map(|m| {
        let (p, mv) = m.resolve(t).expect("resolves");
        (p.id.clone(), mv.name.clone())
    })
}

#[track_caller]
fn fires(phrase: &str, power: &str, mv: &str) {
    assert_eq!(reads(phrase), Some((power.to_string(), mv.to_string())), "{phrase:?}");
}

fn mv(power: &str, name: &str) -> (String, bd_sim::powers::MoveDef) {
    let t = PowerTable::get();
    let p = t.powers.iter().find(|p| p.id == power).unwrap_or_else(|| panic!("{power}"));
    let m = p.moves.iter().find(|m| m.name == name).unwrap_or_else(|| panic!("{power}/{name}")).clone();
    (p.fx.clone(), m)
}

/// The style a move draws in (its own key, else its power's).
fn style(power: &str, name: &str) -> String {
    let (pfx, m) = mv(power, name);
    if m.fx.is_empty() { pfx } else { m.fx }
}

#[test]
fn every_western_ultimate_lands_with_an_impact_frame_and_a_boom() {
    let files: Vec<(&str, &str)> = bd_sim::powers::SHIPPED_FILES.iter().copied().filter(|(n, _)| FILES.contains(n)).collect();
    assert_eq!(files.len(), FILES.len());
    let t = PowerTable::parse_files(&files).expect("western packs parse");
    let mut bare = Vec::new();
    for p in t.powers.iter().filter(|p| !TRANSFORMERS.contains(&p.id.as_str())) {
        for m in p.moves.iter().filter(|m| m.tags.contains(&MoveTag::Ultimate)) {
            let j = m.juice.as_ref();
            if j.is_none_or(|j| j.impact_frame.is_none() || !j.boom || j.title.is_none()) {
                bare.push(format!("{}/{}", p.id, m.name));
            }
        }
    }
    assert!(bare.is_empty(), "ultimates with no frame / boom / title: {bare:?}");
}

#[test]
fn headliners_wear_their_own_bodies() {
    // eye beams come out of the eyes
    for (p, m) in [("kit_superman", "heat vision"), ("kit_cyclops", "optic blast"), ("kit_homelander", "laser eyes"), ("kryptonian", "heat vision")] {
        assert_eq!(style(p, m), "gaze", "{p}/{m}");
    }
    assert_eq!(style("kit_superman", "freeze breath"), "ice", "not painted gold by the kit's holy");
    assert_eq!(style("kit_spider_man", "web zip"), "web");
    assert_eq!(style("kit_jedi", "force push"), "force");
    assert_eq!(style("kit_jedi", "lightsaber"), "saber");
    assert_eq!(style("kit_harry_potter", "stupefy"), "spell");
    assert_eq!(style("kit_doctor_strange", "sling ring"), "mystic");
    assert_eq!(style("kit_scarlet_witch", "hex bolt"), "chaos");
    assert_eq!(style("kit_magneto", "metal storm"), "magnetism", "never the Ruyi pole");
    assert_eq!(style("kit_wolverine", "claws"), "claws");
    assert_eq!(style("kit_captain_america", "shield throw"), "shield");
    assert_eq!(style("kit_batman", "grapnel"), "cable");
    for m in ["lightning generation", "lightning redirection"] {
        assert_eq!(style("firebending", m), "lightning", "firebending/{m} is lightning, not fire");
    }
    assert_eq!(style("kit_azula", "lightning"), "lightning");
    assert_eq!(style("kit_geralt", "igni"), "fire");
    assert_eq!(style("kit_geralt", "aard"), "force");
}

#[test]
fn reworked_recipes_keep_their_shape() {
    // Thor is towed by the hammer (a steerable ram line); Superman flies straight and coasts — not the same verb.
    let (_, thor) = mv("kit_thor", "hammer flight");
    let (_, sup) = mv("kit_superman", "flight");
    let ram = |m: &bd_sim::powers::MoveDef| match m.ops.as_slice() {
        [MoveOp::Ram { steer, keep, through, .. }] => (*steer, *keep, *through),
        other => panic!("{other:?}"),
    };
    let (ts, _, tt) = ram(&thor);
    let (ss, sk, st) = ram(&sup);
    assert!(tt && st, "both fly through whoever is in the way");
    assert!(ts > ss * 2.0, "Thor swerves, Superman goes straight: {ts} vs {ss}");
    assert!(sk > 0.8, "Superman coasts on");
    // a Jedi's jump lands where they look
    let (_, fj) = mv("kit_jedi", "force jump");
    assert!(matches!(fj.ops.as_slice(), [MoveOp::Leap { .. }]), "{:?}", fj.ops);
}

#[test]
fn the_famous_lines_fire_the_famous_moves() {
    fires("crucio", "kit_dark_arts", "crucio");
    fires("imperio", "kit_dark_arts", "imperio");
    fires("riddikulus", "kit_hogwarts_charms", "riddikulus");
    fires("alohomora", "kit_hogwarts_charms", "alohomora");
    fires("execute order 66", "kit_clone_trooper", "order 66");
    fires("order 66", "kit_clone_trooper", "order 66");
    fires("on your left", "kit_captain_america", "on your left");
    fires("geralt sword", "kit_geralt", "silver sword");
    fires("the flash run", "kit_flash", "flash step");
    fires("wonder woman lasso", "kit_wonder_woman", "lasso of truth");
    fires("jean grey phoenix", "kit_jean_grey", "phoenix rising");
    fires("doctor strange portal", "kit_doctor_strange", "sling ring");
    fires("time stone", "kit_doctor_strange", "eye of agamotto");
    fires("in brightest day", "kit_green_lantern", "lantern's light");
    fires("lightning bending", "firebending", "lightning generation");
    fires("eleven telekinesis", "kit_eleven", "throw");
    // the owner's chain still reads
    fires("nightcrawler teleport", "kit_nightcrawler", "teleport");
}
