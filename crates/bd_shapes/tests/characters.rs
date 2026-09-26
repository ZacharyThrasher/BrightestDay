//! Character costume looks (assets/costumes/characters.ron): parsing, alias resolution, variants,
//! budget, determinism, the derived (icon) tier and the lobby tweaks.

use bd_core::shape::PartRole;
use bd_shapes::costume::{self, Tier, look};
use bd_shapes::{COSTUME_UNIT, CostumeTweaks, acc, spec_bounds};
use glam::Vec3;

fn tw() -> CostumeTweaks {
    CostumeTweaks::default()
}

#[test]
fn characters_parse_and_there_are_plenty() {
    let t = look::table();
    assert!(t.error.is_none(), "characters.ron: {:?}", t.error);
    assert!(t.chars.len() >= 150, "only {} characters", t.chars.len());
    for c in &t.chars {
        assert!(!c.names.is_empty());
        assert!(c.variants.len() <= 4, "{}: {} variants", c.names[0], c.variants.len());
    }
}

#[test]
fn every_alias_resolves_to_its_entry() {
    let t = look::table();
    let mut clash = Vec::new();
    for (i, c) in t.chars.iter().enumerate() {
        for n in &c.names {
            match look::find(n) {
                Some(f) if f.index == i => {}
                other => clash.push(format!("{n:?} -> {:?} (want {})", other.map(|f| &t.chars[f.index].names[0]), c.names[0])),
            }
        }
    }
    assert!(clash.is_empty(), "aliases that don't resolve to their own entry:\n{}", clash.join("\n"));
}

#[test]
fn case_punctuation_and_filler_are_ignored() {
    let idx = |p: &str| look::find(p).map(|f| look::table().chars[f.index].names[0].clone());
    assert_eq!(idx("Spider-Man").as_deref(), Some("spider-man"));
    assert_eq!(idx("spiderman").as_deref(), Some("spider-man"));
    assert_eq!(idx("  GOJO  ").as_deref(), Some("gojo"));
    assert_eq!(idx("the batman").as_deref(), Some("batman"));
    assert_eq!(idx("darth vader costume").as_deref(), Some("darth vader"));
    assert_eq!(idx("toaster"), None);
}

#[test]
fn variant_words_pick_variants() {
    let t = look::table();
    for (i, c) in t.chars.iter().enumerate() {
        for (vi, v) in c.variants.iter().enumerate() {
            let phrase = format!("{} {}", c.names[0], v.name);
            let f = look::find(&phrase).unwrap_or_else(|| panic!("{phrase:?} not found"));
            assert_eq!(f.index, i, "{phrase:?}");
            assert_eq!(f.base_variant, vi, "{phrase:?}");
        }
    }
    let g = look::find("gojo sunglasses").expect("gojo sunglasses");
    let dressed = costume::character_costume(g.index, g.base_variant, tw());
    assert_eq!(dressed.variant.map(|v| v.0), Some(g.base_variant));
}

#[test]
fn every_look_builds_on_the_rig_within_budget() {
    let t = look::table();
    let limit = costume::guardrail_volume_units() * costume::CHARACTER_GUARDRAIL_K / costume::GUARDRAIL_K;
    for (i, c) in t.chars.iter().enumerate() {
        for v in 0..c.variant_count() {
            let d = costume::character_costume(i, v, tw());
            let name = format!("{} #{v}", c.names[0]);
            assert_eq!(d.tier, Tier::Character, "{name}");
            assert_eq!(d.paint.len(), d.spec.prims.len(), "{name}: paint aligned");
            let pm = costume::costume_model_painted(&d);
            assert!(pm.model.voxel_count <= bd_shapes::costume::MAX_COSTUME_VOXELS, "{name}: {} voxels", pm.model.voxel_count);
            assert_eq!(pm.colors.len(), pm.model.parts.len());
            let painted: usize = pm.colors.iter().map(|m| m.len()).sum();
            assert!(painted * 10 >= pm.model.voxel_count * 9, "{name}: only {painted}/{} voxels painted", pm.model.voxel_count);
            // Same rig as the vocabulary costume (animations, hand lookup, first-person hands).
            for (id, role) in [
                (1u8, PartRole::Head),
                (2, PartRole::ArmLeft),
                (3, PartRole::ArmRight),
                (4, PartRole::LegBackLeft),
                (5, PartRole::LegBackRight),
            ] {
                assert!(d.spec.parts.iter().any(|p| p.id == id && p.role == role), "{name}: part {id} {role:?}");
            }
            // Never shrunk as a whole by the guardrail: feet on the ground, full height.
            let b = spec_bounds(&d.spec);
            assert!(b.min.y > -0.6 && b.min.y < 0.6, "{name}: feet at {}", b.min.y);
            let h = (b.max.y - b.min.y + 1.0) * COSTUME_UNIT;
            assert!((1.7..2.6).contains(&h), "{name}: height {h} m");
            let s = b.max - b.min + Vec3::ONE;
            assert!(s.x * s.y * s.z <= limit * 1.001, "{name}: bounding volume {}", s.x * s.y * s.z);
        }
    }
}

#[test]
fn costumes_are_deterministic() {
    for name in ["gojo", "darth vader", "goku", "naruto", "spider-man", "master chief"] {
        let f = look::find(name).expect(name);
        for seed in 0..3 {
            let a = costume::dress(name, &bd_core::mechanics::Recipe::unknown(name), None, None, seed, tw());
            let b = costume::dress(name, &bd_core::mechanics::Recipe::unknown(name), None, None, seed, tw());
            assert_eq!(a.spec, b.spec, "{name}");
            assert_eq!(a.paint, b.paint, "{name}");
            assert_eq!(a.variant.map(|v| v.0), Some((f.base_variant + seed as usize) % look::table().chars[f.index].variant_count()));
            let (ma, mb) = (costume::costume_model_painted(&a), costume::costume_model_painted(&b));
            assert_eq!(ma.colors, mb.colors, "{name}");
        }
    }
}

#[test]
fn tweaks_apply_on_top_of_characters() {
    let f = look::find("goku").expect("goku");
    let base = costume::character_costume(f.index, 0, tw());
    // Accent recolours the outfit zone.
    let red = costume::character_costume(f.index, 0, CostumeTweaks { accent: Some([1.0, 0.0, 0.0]), ..tw() });
    assert!(red.paint.iter().any(|p| p.rgb == Some([255, 0, 0])));
    assert!(!base.paint.iter().any(|p| p.rgb == Some([255, 0, 0])));
    // Head size scales the head.
    let m = look::find("mario").expect("mario");
    let small = costume::character_costume(m.index, 0, tw());
    let big = costume::character_costume(m.index, 0, CostumeTweaks { head_scale: 1.3, ..tw() });
    assert!(spec_bounds(&big.spec).max.y > spec_bounds(&small.spec).max.y);
    // Every accessory slot can be hidden without panicking; hiding the helmet shows hair.
    let t = look::table();
    for i in 0..t.chars.len() {
        for bit in 0..10 {
            let d = costume::character_costume(i, 0, CostumeTweaks { hide_accessories: 1 << bit, ..tw() });
            assert!(!d.spec.prims.is_empty());
        }
        let all = costume::character_costume(i, 0, CostumeTweaks { hide_accessories: acc::ALL, ..tw() });
        assert!(costume::costume_model_painted(&all).model.voxel_count > 500);
    }
    let v = look::find("darth vader").expect("vader");
    let helm = costume::character_costume(v.index, 0, tw());
    let bare = costume::character_costume(v.index, 0, CostumeTweaks { hide_accessories: acc::HEAD, ..tw() });
    assert_ne!(helm.spec, bare.spec);
}

/// Humanoid icons without a hand-authored look go through the derived tier.
#[test]
fn derived_costumes_for_character_icons() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let Ok(lex) = bd_lexicon::Lexicon::load(root.join("assets/lexicon.bin")) else {
        eprintln!("lexicon.bin missing (gitignored build output): skipping");
        return;
    };
    let mut names = Vec::new();
    for f in [
        "icons/marvel.ron",
        "icons/dc.ron",
        "icons/shows.ron",
        "icons/games.ron",
        "icons/anime.ron",
        "icons/film_tv.ron",
        "icons/tmnt.ron",
        "icons.ron",
    ] {
        let Ok(src) = std::fs::read_to_string(root.join("assets").join(f)) else { continue };
        for l in src.lines() {
            if let Some(rest) = l.trim().strip_prefix("names: [\"")
                && let Some(n) = rest.split('"').next()
                && costume::grammar::parse(n).is_none()
            {
                names.push(n.to_string());
            }
        }
    }
    // Deterministic "random" pick: hash order.
    names.sort_by_key(|n| n.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3)));
    let limit = costume::guardrail_volume_units() * costume::CHARACTER_GUARDRAIL_K / costume::GUARDRAIL_K;
    let mut done = 0;
    for n in &names {
        let r = lex.resolve(n);
        let Some(shape) = r.shape.as_ref() else { continue };
        if !costume::is_character_icon(&r.recipe, shape) {
            continue;
        }
        let d = costume::dress(n, &r.recipe, Some(shape), None, 0, tw());
        assert_eq!(d.tier, Tier::Derived, "{n}");
        let d2 = costume::dress(n, &r.recipe, Some(shape), None, 0, tw());
        assert_eq!(d.spec, d2.spec, "{n}: deterministic");
        let pm = costume::costume_model_painted(&d);
        assert!(
            pm.model.voxel_count > 300 && pm.model.voxel_count <= bd_shapes::costume::MAX_COSTUME_VOXELS,
            "{n}: {}",
            pm.model.voxel_count
        );
        let b = spec_bounds(&d.spec);
        let s = b.max - b.min + Vec3::ONE;
        assert!(s.x * s.y * s.z <= limit * 1.001, "{n}: volume");
        done += 1;
        if done == 20 {
            break;
        }
    }
    assert_eq!(done, 20, "found only {done} humanoid icons");
}

#[test]
#[ignore]
fn print_prim_counts() {
    let t = look::table();
    let mut worst = Vec::new();
    for i in 0..t.chars.len() {
        for v in 0..t.chars[i].variant_count() {
            let d = costume::character_costume(i, v, tw());
            let geo = d.paint.iter().filter(|p| !p.decal).count();
            worst.push((geo, d.spec.prims.len(), t.chars[i].names[0].clone()));
        }
    }
    worst.sort();
    for w in worst.iter().rev().take(15) {
        println!("{w:?}");
    }
}

// ------------------------------------------------------------------------------ mix and match

fn composed(phrase: &str) -> look::Look {
    let p = costume::grammar::parse(phrase).unwrap_or_else(|| panic!("{phrase:?} did not parse"));
    costume::grammar::compose(&p, 0)
}

fn base_of(phrase: &str) -> Option<String> {
    costume::grammar::parse(phrase).and_then(|p| p.base).map(|(c, _)| look::table().chars[c].names[0].clone())
}

#[test]
fn mix_and_match_phrases() {
    use look::{Cut, HairStyle, Hat, Mask};
    // "gojo but bald"
    let l = composed("gojo but bald");
    assert_eq!(l.hair.map(|h| h.style), Some(HairStyle::Bald));
    assert_eq!(l.top.map(|t| t.cut), Some(Cut::HighCollar), "still Gojo's uniform");
    // "donatello with a straw hat": his bandana mask stays, the hat is added.
    let l = composed("donatello with a straw hat");
    assert!(matches!(l.hat, Some(Hat::Straw(..))));
    assert!(matches!(l.mask, Some(Mask::Bandana(..))));
    // "goku with naruto's headband": Naruto's exact headband (with the plate).
    let naruto = look::table().chars[look::find("naruto").expect("naruto").index].look_of(0);
    let l = composed("goku with naruto's headband");
    assert_eq!(l.hat, naruto.hat);
    assert_eq!(l.hair.map(|h| h.style), Some(HairStyle::Saiyan));
    // "red goku": the gi turns red.
    let l = composed("red goku");
    assert_eq!(l.top.map(|t| t.color.rgb()), Some([210, 30, 36]));
    // "goku x luffy": Goku's head on Luffy's outfit.
    let l = composed("goku x luffy");
    assert_eq!(l.hair.map(|h| h.style), Some(HairStyle::Saiyan));
    assert_eq!(l.top.map(|t| t.cut), Some(Cut::Vest));
    assert_eq!(composed("goku naruto").top.map(|t| t.cut), Some(Cut::Jumpsuit));
    // "gojo but flibbertigibbet": unknown words are ignored — base Gojo, no panic.
    assert_eq!(base_of("gojo but flibbertigibbet").as_deref(), Some("gojo"));
    assert_eq!(base_of("zombie gojo").as_deref(), Some("gojo"));
    // More grammar.
    assert_eq!(composed("gojo with pink hair").hair.map(|h| h.color.rgb()), Some([245, 140, 180]));
    assert!(matches!(composed("naruto without the headband").hat, Some(Hat::No)));
    assert!(matches!(composed("darth vader with a straw hat").hat, Some(Hat::Straw(..))));
    assert!(matches!(composed("darth vader with a straw hat").helmet, Some(look::Helmet::No)), "a hat replaces the helmet");
    assert!(composed("batman no cape").acc.unwrap_or_default().iter().all(|a| !matches!(a, look::Acc::Cape(..))));
    // A part alone dresses a default body; nonsense alone is not a costume phrase.
    assert!(matches!(composed("straw hat").hat, Some(Hat::Straw(..))));
    assert_eq!(base_of("straw hat"), None);
    assert!(costume::grammar::parse("flibbertigibbet").is_none());
    assert!(costume::grammar::parse("knight").is_none(), "vocabulary phrases stay vocabulary");
    // All of them build (and deterministically).
    for ph in [
        "gojo but bald", "donatello with a straw hat", "goku with naruto's headband", "red goku", "goku x luffy", "gojo but flibbertigibbet", "straw hat",
        "spider-man in black", "batman with a gold cape", "mario with sunglasses and a cape", "master chief but no helmet",
    ] {
        let r = bd_core::mechanics::Recipe::unknown(ph);
        let a = costume::dress(ph, &r, None, None, 0, tw());
        let b = costume::dress(ph, &r, None, None, 0, tw());
        assert_eq!(a.spec, b.spec, "{ph}");
        assert_eq!(a.tier, Tier::Character, "{ph}");
        assert!(costume::costume_model_painted(&a).model.voxel_count > 800, "{ph}");
    }
}
