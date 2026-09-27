//! Summoned character constructs ("goku!" → a hard-light Goku). When a cast's text names a
//! character (exact alias, optionally with scale words and mix-and-match modifiers: "GIANT GOKU",
//! "goku with a sword", "goku with naruto's headband"), the construct is built from that character's
//! costume instead of the generic shape: the recipe's model becomes
//! `ModelSource::Icon("costume:<normalised text>")` and the cast carries the costume's geometry as
//! its curated shape. Every peer re-derives the same costume (and its colours) from that id string,
//! so nothing new goes over the network.
//!
//! * Not a costume: power phrases and signatures (the caller only asks for plain casts), anything
//!   whose head is another object ("goku's power pole"), unknown words (no fuzzy near-misses).
//! * Look: the costume's palette per voxel, which the construct shader blends toward the caster's
//!   lantern colour (rim, lines and outline unchanged) — still the caster's hard light.

use std::collections::HashMap;

use bd_core::CONSTRUCT_VOXEL_SIZE;
use bd_core::mechanics::{Form, Mechanic, ModelSource, Recipe};
use bd_core::shape::ShapeSpec;
use glam::IVec3;

use super::paint::{Paint, Rgb, bake};
use super::{COSTUME_UNIT, CostumeTweaks, grammar, look};
use crate::voxelize::VoxelModel;

/// `ModelSource::Icon` id prefix of summoned costumes.
pub const PREFIX: &str = "costume:";

fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Which costume a summon's text asks for (the normalised text used as its id), if any.
/// `recipe_name` = what the lexicon made of the whole phrase ("giant goku riding a shark" → Goku).
pub fn summon_costume_text(text: &str, recipe_name: &str) -> Option<String> {
    let t = normalise(text);
    if let Some(p) = grammar::parse(&t)
        && p.base.is_some()
        && p.unknown.is_empty()
    {
        return Some(t);
    }
    // The lexicon's head noun is itself a character ("goku riding a shark" → Goku + a mount).
    let n = normalise(recipe_name);
    look::find(&n).filter(|f| f.base_variant == 0).map(|_| n)
}

/// The phrase a costume summon's MECHANICS should be read from, when the costume grammar took words
/// the lexicon would otherwise also read as grafts: "gojo but bald" (the lexicon stems "bald" to a
/// Thrust + Homing graft), "gojo with pink hair", "red goku", "gojo sunglasses", "goku x naruto".
/// Those words dress the costume only; the construct keeps the base character's own mechanics.
/// Returns the size / count words plus the base character's canonical name ("giant gojo but bald" →
/// "giant gojo"), for the caller to re-resolve with the lexicon.
///
/// `None` = read the phrase as said: not a costume-grammar summon (the lexicon's head noun path,
/// "goku riding a shark", keeps its mount), nothing beyond the name and size words ("GIANT GOKU",
/// "satoru gojo"), or the phrase grants a power (the lexicon's reading carries the grant).
pub fn mechanics_phrase(text: &str, recipe: &Recipe) -> Option<String> {
    if recipe.grants.is_some() {
        return None;
    }
    let t = normalise(text);
    let p = grammar::parse(&t)?;
    let (ci, _) = p.base?;
    if !p.unknown.is_empty() {
        return None;
    }
    let table = look::table();
    let rest: String = look::tokens(&t).into_iter().filter(|w| !grammar::plain_word(w)).collect();
    if table.index.get(&rest) == Some(&ci) {
        return None; // just the name (+ size / filler words): the lexicon already reads it right
    }
    let name = table.chars.get(ci)?.names.first()?;
    Some(p.scale.iter().chain(std::iter::once(name)).cloned().collect::<Vec<_>>().join(" "))
}

/// Apply the summon costume to a resolved cast: returns true when the recipe/shape were replaced.
/// Non-humanoid resolutions of a character name ("darth vader" → a force grip) become a humanoid.
pub fn apply(text: &str, recipe: &mut Recipe, shape: &mut Option<ShapeSpec>) -> bool {
    // TRANSFORMERS: a transformer keeps its own two-mode body (a curated icon's truck / jet / tank /
    // T-rex + robot, or a procedural alt mode): the costume biped would erase the alt mode.
    if transforms_own_body(recipe) {
        return false;
    }
    let Some(id) = summon_costume_text(text, &recipe.name) else { return false };
    let Some((geo, _, _)) = summon_spec(&id) else { return false };
    if recipe.form != Form::Biped {
        recipe.form = Form::Biped;
        recipe.scale = recipe.scale.max(1.9);
    }
    recipe.model = ModelSource::Icon(format!("{PREFIX}{id}"));
    *shape = Some(geo);
    true
}

/// Does `recipe` carry its own transforming body (the costume must not replace it)? A Transforms
/// resolution that is a curated icon, or whose form is not a plain biped (its alt mode).
pub fn transforms_own_body(recipe: &Recipe) -> bool {
    recipe.has(Mechanic::Transforms) && (matches!(recipe.model, ModelSource::Icon(_)) || recipe.form != Form::Biped)
}

/// The costume id of a recipe (`None` = not a summoned costume).
pub fn costume_id(recipe: &Recipe) -> Option<&str> {
    match &recipe.model {
        ModelSource::Icon(id) => id.strip_prefix(PREFIX),
        _ => None,
    }
}

/// `(geometry, full spec with decals, paint)` in construct units for a costume id.
pub fn summon_spec(id: &str) -> Option<(ShapeSpec, ShapeSpec, Vec<Paint>)> {
    let p = grammar::parse(id)?;
    p.base?;
    let c = super::phrase_costume(&p, 0, CostumeTweaks::default());
    let mut full = c.spec;
    crate::voxelize::scale_spec(&mut full, COSTUME_UNIT / CONSTRUCT_VOXEL_SIZE);
    let geo = ShapeSpec {
        prims: full.prims.iter().zip(&c.paint).filter(|(_, pt)| !pt.decal).map(|(p, _)| p.clone()).collect(),
        parts: full.parts.clone(),
    };
    Some((geo, full, c.paint))
}

fn height(s: &ShapeSpec) -> f32 {
    let b = crate::attach::spec_bounds(s);
    (b.max.y - b.min.y).max(1e-3)
}

/// Per-voxel colours for a summoned costume construct's model. `shape` = the construct's curated
/// shape as the client has it (normalised by the sim), `model` = `build_icon_model(recipe, shape).1`.
pub fn summon_colors(recipe: &Recipe, shape: &ShapeSpec, model: &VoxelModel) -> Option<Vec<HashMap<IVec3, Rgb>>> {
    let id = costume_id(recipe)?;
    let (geo, mut full, paint) = summon_spec(id)?;
    // shape = geo × f_sim; the model = normalise(shape) (≈ ×1) voxelised at voxel_size / unit_m.
    let f = height(shape) / height(&geo) * crate::forms::icon_scale_factor(recipe, shape) * (model.unit_m / model.voxel_size.max(1e-4));
    crate::voxelize::scale_spec(&mut full, f);
    let mut cols = bake(&full, &paint, model);
    super::paint::fill_gaps(model, &mut cols, 3);
    Some(cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str, name: &str) -> Option<String> {
        summon_costume_text(text, name)
    }

    #[test]
    fn character_summons_resolve_and_objects_do_not() {
        assert_eq!(id("goku", "Goku").as_deref(), Some("goku"));
        assert_eq!(id("Naruto", "Naruto").as_deref(), Some("naruto"));
        assert_eq!(id("gojo", "Gojo").as_deref(), Some("gojo"));
        assert_eq!(id("luffy", "Luffy").as_deref(), Some("luffy"));
        assert_eq!(id("GIANT GOKU", "Goku").as_deref(), Some("giant goku"));
        assert_eq!(id("goku with a sword", "Goku").as_deref(), Some("goku with a sword"));
        assert_eq!(id("goku's power pole", "Power Pole"), None);
        assert_eq!(id("kamehameha", "Kamehameha"), None);
        assert_eq!(id("ultra instinct", "Ultra Instinct"), None);
        assert_eq!(id("gear 5", "Gear 5"), None);
        assert_eq!(id("goku riding a shark", "Goku").as_deref(), Some("goku"));
        assert_eq!(id("shark", "Shark"), None);
        assert_eq!(id("gokuu", "Gokuu"), None, "no fuzzy near-misses");
        // fighters: by name, and by the lexicon's reading of a famous move ("falcon punch" → Captain Falcon)
        assert_eq!(id("reinhardt", "Reinhardt").as_deref(), Some("reinhardt"));
        assert_eq!(id("reinhardt charge", "Reinhardt").as_deref(), Some("reinhardt"));
        assert_eq!(id("reinhardt with a rocket hammer", "Reinhardt").as_deref(), Some("reinhardt with a rocket hammer"));
        assert_eq!(id("falcon punch", "Captain Falcon").as_deref(), Some("captain falcon"));
        assert_eq!(id("falcon", "falcon"), None, "the bird stays a bird");
        for name in ["captain falcon", "akuma", "m bison", "kazuya", "jin kazama", "liu kang", "raiden mk", "zarya", "doomfist"] {
            assert_eq!(id(name, name).as_deref(), Some(name));
        }
    }

    #[test]
    fn costume_words_never_become_mechanics() {
        let m = |text: &str| mechanics_phrase(text, &Recipe::unknown(text));
        // the costume grammar's words: read the mechanics from the bare character
        assert_eq!(m("gojo but bald").as_deref(), Some("gojo"));
        assert_eq!(m("Gojo shirtless").as_deref(), Some("gojo"));
        assert_eq!(m("gojo unmasked").as_deref(), Some("gojo"));
        assert_eq!(m("gojo with pink hair").as_deref(), Some("gojo"));
        assert_eq!(m("red goku").as_deref(), Some("goku"));
        assert_eq!(m("goku with a sword").as_deref(), Some("goku"));
        assert_eq!(m("batman no cape").as_deref(), Some("batman"));
        assert_eq!(m("naruto without the headband").as_deref(), Some("naruto"));
        assert_eq!(m("gojo with luffy's hat").as_deref(), Some("gojo"));
        assert_eq!(m("goku x naruto").as_deref(), Some("goku"));
        assert_eq!(m("gojo sunglasses").as_deref(), Some("gojo"), "variant words dress the costume only");
        assert_eq!(m("gojo blindfold").as_deref(), Some("gojo"));
        assert_eq!(m("satoru gojo but bald").as_deref(), Some("gojo"));
        // size words stay (the lexicon scales the construct)
        assert_eq!(m("giant gojo but bald").as_deref(), Some("giant gojo"));
        assert_eq!(m("GIANT GOKU with a cape").as_deref(), Some("giant goku"));
        // read as said: just the name (+ size / filler), not a costume phrase, or a power grant
        assert_eq!(m("gojo"), None);
        assert_eq!(m("satoru gojo"), None);
        assert_eq!(m("GIANT GOKU"), None);
        assert_eq!(m("the batman"), None);
        assert_eq!(m("goku riding a shark"), None);
        assert_eq!(m("kamehameha"), None);
        assert_eq!(m("shark"), None);
        let mut granting = Recipe::unknown("goku");
        granting.grants = Some("ssj".into());
        assert_eq!(mechanics_phrase("goku super saiyan", &granting), None);
    }

    #[test]
    fn summoned_costume_colours_its_model() {
        let mut r = Recipe::unknown("goku");
        r.form = Form::Biped;
        r.scale = 1.9;
        let mut shape = None;
        assert!(apply("goku", &mut r, &mut shape));
        let s = shape.expect("shape");
        for scale in [1.9f32, 14.0] {
            let mut r = r.clone();
            r.scale = scale;
            let (norm, model) = crate::forms::build_icon_model(&r, &s);
            let cols = summon_colors(&r, &norm, &model).expect("colours");
            let painted: usize = cols.iter().map(|m| m.len()).sum();
            assert!(painted * 10 >= model.voxel_count * 8, "scale {scale}: {painted}/{}", model.voxel_count);
            // Goku's gi orange is in there.
            assert!(cols.iter().flat_map(|m| m.values()).any(|c| *c == [0xf4, 0x7a, 0x1f]), "scale {scale}");
        }
    }
}
