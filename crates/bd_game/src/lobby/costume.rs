//! Costume building shared by the costume creator (turntable), other players' bodies
//! (`present::players`) and the first-person hands.
//!
//! Resolution (`bd_shapes::costume::dress`): a hand-authored **character** look ("gojo", "darth
//! vader"; assets/costumes/characters.ron, `seed` cycles its variants) → a look **derived** from a
//! curated humanoid icon → the **vocabulary** costume. `"knight with a fox head"` → body phrase
//! "knight" dressed on the rig, head donated by "fox" (vocabulary tier).
//!
//! Everyone builds the same costume from the replicated (phrase, seed, tweaks): the character table
//! is compiled into bd_shapes, so peers agree without the lexicon.

use crate::lobby::prefs::CostumePref;
use bd_core::mechanics::{Form, Recipe};
use bd_core::shape::ShapeSpec;
use bd_lexicon::Lexicon;
use bd_shapes::costume::{Costume, PaintedModel};
use bd_shapes::{CostumeHead, CostumeTweaks};

/// Split "X with a Y head" / "X with Y's head" / "Y-headed X" into (body, Some(head)).
pub fn split_head(phrase: &str) -> (String, Option<String>) {
    let p = phrase.trim().to_lowercase();
    if let Some(body) = p.strip_suffix(" head").or_else(|| p.strip_suffix(" face"))
        && let Some(i) = body.rfind(" with ")
    {
        let (b, h) = (&body[..i], &body[i + 6..]);
        let h = h.trim_start_matches("a ").trim_start_matches("an ").trim_start_matches("the ");
        let h = h.strip_suffix("'s").unwrap_or(h);
        if !b.trim().is_empty() && !h.trim().is_empty() {
            return (b.trim().to_string(), Some(h.trim().to_string()));
        }
    }
    // "fox-headed knight" / "fox headed knight"
    for pat in ["-headed ", " headed "] {
        if let Some(i) = p.find(pat) {
            let (h, b) = (&p[..i], &p[i + pat.len()..]);
            if !b.trim().is_empty() && !h.trim().is_empty() {
                return (b.trim().to_string(), Some(h.trim().to_string()));
            }
        }
    }
    (p, None)
}

pub fn tweaks(c: &CostumePref) -> CostumeTweaks {
    CostumeTweaks { accent: c.accent, head_scale: c.head_scale, hide_accessories: c.hide }
}

/// Full costume spec for a pref (pure; a few ms).
pub fn costume_spec(lex: &Lexicon, c: &CostumePref) -> ShapeSpec {
    dress(Some(lex), &c.phrase, c.seed, tweaks(c)).spec
}

/// Dress a phrase (all tiers, see the module docs). Without a lexicon, non-character phrases get a
/// plain biped recipe named after the phrase.
pub fn dress(lex: Option<&Lexicon>, phrase: &str, seed: u32, tw: CostumeTweaks) -> Costume {
    // Character / mix-and-match phrases ("gojo but bald", "donatello with a straw hat").
    let (body, head) = split_head(phrase);
    let whole = phrase.trim().to_lowercase();
    if (head.is_none() || bd_shapes::costume::is_character_phrase(&body)) && bd_shapes::costume::is_character_phrase(&whole) {
        return bd_shapes::costume::dress(&whole, &Recipe::unknown(&whole), None, None, seed, tw);
    }
    let body = if body.is_empty() { "person".to_string() } else { body };
    match lex {
        Some(lex) => {
            let rb = lex.resolve(&body);
            let rh = head.map(|h| lex.resolve(&h));
            let head = rh.as_ref().map(|r| CostumeHead { recipe: &r.recipe, shape: r.shape.as_ref() });
            bd_shapes::costume::dress(&body, &rb.recipe, rb.shape.as_ref(), head, seed, tw)
        }
        None => {
            let mut r = Recipe::unknown(&body);
            r.form = Form::Biped;
            bd_shapes::costume::dress(&body, &r, None, None, seed, tw)
        }
    }
}

/// Voxel model + per-voxel colours for a costume phrase (runs on a worker thread; ~ms).
pub fn painted_model(lex: Option<&Lexicon>, phrase: &str, seed: u32, tw: CostumeTweaks) -> PaintedModel {
    bd_shapes::costume::costume_model_painted(&dress(lex, phrase, seed, tw))
}

/// Number of variants the phrase's character look has (`None` = not a character phrase).
pub fn variant_count(phrase: &str) -> Option<usize> {
    let (body, head) = split_head(phrase);
    if head.is_some() && !bd_shapes::costume::is_character_phrase(&body) {
        return None;
    }
    bd_shapes::costume::variant_count(phrase)
}

/// Accent colour the costume renders with (palette slot of unpainted voxels).
pub fn costume_accent(lex: &Lexicon, c: &CostumePref) -> Option<[f32; 3]> {
    let (body, head) = split_head(&c.phrase);
    if (head.is_none() || bd_shapes::costume::is_character_phrase(&body)) && bd_shapes::costume::is_character_phrase(&c.phrase) {
        return tweaks(c).accent;
    }
    let rb = lex.resolve(&body);
    bd_shapes::costume::costume_accent(&rb.recipe, &tweaks(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_heads() {
        assert_eq!(split_head("knight with a fox head"), ("knight".into(), Some("fox".into())));
        assert_eq!(split_head("Robot with an Eagle Head"), ("robot".into(), Some("eagle".into())));
        assert_eq!(split_head("pirate with godzilla's head"), ("pirate".into(), Some("godzilla".into())));
        assert_eq!(split_head("fox-headed knight"), ("knight".into(), Some("fox".into())));
        assert_eq!(split_head("shark"), ("shark".into(), None));
        assert_eq!(split_head("with a head"), ("with a head".into(), None));
    }

    #[test]
    fn character_phrases_dress_without_a_lexicon() {
        let c = dress(None, "Gojo", 0, CostumeTweaks::default());
        assert_eq!(c.tier, bd_shapes::costume::Tier::Character);
        assert!(c.is_painted());
        assert_eq!(variant_count("darth vader"), Some(1));
        assert!(variant_count("gojo").is_some_and(|n| n >= 2));
        assert_eq!(variant_count("knight with a gojo head"), None);
        let a = painted_model(None, "gojo", 1, CostumeTweaks::default());
        let b = painted_model(None, "gojo", 1, CostumeTweaks::default());
        assert_eq!(a.model.voxel_count, b.model.voxel_count);
        assert_eq!(a.colors, b.colors);
    }
}
