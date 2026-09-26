//! Costume building shared by the costume creator (turntable) and the first-person hands.
//!
//! `"knight with a fox head"` → body phrase "knight" dressed on the rig, head donated by "fox"
//! (`bd_shapes::costume_with` with a [`CostumeHead`]). Curated icon shapes are passed through so
//! "goku" / "pikachu" costumes use their authored heads.

use crate::lobby::prefs::CostumePref;
use bd_core::shape::ShapeSpec;
use bd_lexicon::Lexicon;
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
    let (body, head) = split_head(&c.phrase);
    let body = if body.is_empty() { "person".to_string() } else { body };
    let rb = lex.resolve(&body);
    let rh = head.map(|h| lex.resolve(&h));
    let head = rh.as_ref().map(|r| CostumeHead { recipe: &r.recipe, shape: r.shape.as_ref() });
    bd_shapes::costume_with(&rb.recipe, rb.shape.as_ref(), head, c.seed, tweaks(c))
}

/// Accent colour the costume renders with.
pub fn costume_accent(lex: &Lexicon, c: &CostumePref) -> Option<[f32; 3]> {
    let (body, _) = split_head(&c.phrase);
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
}
