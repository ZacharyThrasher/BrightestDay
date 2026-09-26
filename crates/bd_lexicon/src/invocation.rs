//! Signature tier (COMBAT.md §5j): invocation grammar + famous ultimates — vocabulary only.
//!
//! * `domain expansion <rest>`, `bankai <rest>`, `final <rest>`, `ultimate <rest>`, `<rest> domain`…
//!   (the keyword lists are data: `assets/signatures.ron`). `<rest>` resolves exactly like any phrase
//!   and supplies the element, mechanics, features and icon; the keyword supplies the [`SignatureKind`]
//!   (or it is inferred from what `<rest>` became).
//! * Famous ultimates carry a `names` tag in the same file ("kamehameha" → Beam, "unlimited void" →
//!   Domain): the whole phrase or the construct it resolves to ("jogo domain" → the Coffin of the Iron
//!   Mountain icon) makes it a signature.
//! * `bang_beams`: a Beam construct yelled with enough "!" ("HYPER BEAM!!!") is a Beam signature.
//!
//! The lexicon only *labels* the resolution ([`crate::Resolution::signature`]); what a signature does
//! (the amplifier over existing mechanics, its cost, windup and the cut-in alert) is the game's and
//! bd_sim's business. Nothing here is ever shown or explained to players (mystery tenet).

use std::collections::HashMap;

use bd_core::mechanics::{Form, FormCategory, Mechanic, ModelSource};
use serde::Deserialize;

use crate::norm::{Tok, normalize_key};
use crate::parse::{Gram, gram_of};
use crate::{Lexicon, Resolution, ResolutionKind};

/// What fantasy a signature cast expresses (the sim decides what that means mechanically).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub enum SignatureKind {
    /// A sealed arena erupting around the caster.
    Domain,
    /// A charged ultimate beam.
    Beam,
    /// The caster's construct in its ultimate form (bankai, super saiyan).
    Transformation,
    /// The named thing itself, at ultimate scale (spirit bomb, susanoo).
    Summon,
}

/// A resolution that is a signature cast.
#[derive(Clone, Debug, PartialEq)]
pub struct SignatureCall {
    pub kind: SignatureKind,
    /// Display title for the cut-in / title letters: "Domain Expansion: Coffin of the Iron Mountain"
    /// (≤ [`MAX_TITLE`] chars).
    pub title: String,
    /// The invocation words used ("domain expansion"), `None` for a tagged name / "!!!".
    pub invocation: Option<String>,
}

/// Longest title (the sim's recipe-name bound).
pub const MAX_TITLE: usize = 48;

// ------------------------------------------------------------------------------------ data

#[derive(Clone, Debug, Deserialize)]
struct InvocationRow {
    words: String,
    #[serde(default)]
    kind: Option<SignatureKind>,
    #[serde(default)]
    title: String,
    /// What a bare invocation builds (None = the words themselves, resolved normally).
    #[serde(default)]
    alone: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct NameRow {
    name: String,
    kind: SignatureKind,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct SignatureFile {
    bang_beams: usize,
    invocations: Vec<InvocationRow>,
    suffixes: Vec<InvocationRow>,
    names: Vec<NameRow>,
}

/// One invocation, normalised.
#[derive(Clone, Debug)]
pub(crate) struct Invocation {
    pub words: Vec<String>,
    pub kind: Option<SignatureKind>,
    pub title: String,
    pub alone: Option<String>,
}

/// The parsed `signatures.ron`.
#[derive(Clone, Debug, Default)]
pub(crate) struct SignatureTable {
    /// Longest first.
    pub prefixes: Vec<Invocation>,
    /// Longest first.
    pub suffixes: Vec<Invocation>,
    /// normalised name → (kind, optional display title).
    pub names: HashMap<String, (SignatureKind, Option<String>)>,
    pub bang_beams: usize,
}

/// The shipped table (compiled in; `Lexicon::load` prefers a copy next to lexicon.bin).
pub(crate) const DEFAULT_TABLE: &str = include_str!("../../../assets/signatures.ron");

impl SignatureTable {
    /// Parse; a malformed file yields an empty table plus a warning (never fails a load).
    pub fn parse(src: &str) -> (SignatureTable, Option<String>) {
        match ron::from_str::<SignatureFile>(src) {
            Ok(f) => (Self::from_file(f), None),
            Err(e) => (SignatureTable::default(), Some(format!("signatures.ron: {e}"))),
        }
    }

    fn from_file(f: SignatureFile) -> SignatureTable {
        let conv = |rows: Vec<InvocationRow>| {
            let mut v: Vec<Invocation> = rows
                .into_iter()
                .filter_map(|r| {
                    let k = normalize_key(&r.words);
                    if k.is_empty() {
                        return None;
                    }
                    let title = if r.title.trim().is_empty() { title_case(&k) } else { r.title.trim().to_string() };
                    Some(Invocation { words: k.split(' ').map(String::from).collect(), kind: r.kind, title, alone: r.alone.filter(|a| !a.trim().is_empty()) })
                })
                .collect();
            v.sort_by(|a, b| b.words.len().cmp(&a.words.len()).then(a.words.cmp(&b.words)));
            v
        };
        let mut names = HashMap::new();
        for n in f.names {
            let k = normalize_key(&n.name);
            if !k.is_empty() {
                names.entry(k).or_insert((n.kind, n.title));
            }
        }
        SignatureTable { prefixes: conv(f.invocations), suffixes: conv(f.suffixes), names, bang_beams: f.bang_beams }
    }

    pub fn default_table() -> SignatureTable {
        Self::parse(DEFAULT_TABLE).0
    }
}

/// "coffin of the iron mountain" → "Coffin of the Iron Mountain" (small words stay lower-case).
pub fn title_case(s: &str) -> String {
    const SMALL: &[&str] = &["of", "the", "no", "a", "an", "and", "in", "on", "to", "de", "du", "la", "le", "for"];
    let mut out = String::with_capacity(s.len());
    for (i, w) in s.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if i > 0 && SMALL.contains(&w) {
            out.push_str(w);
            continue;
        }
        let mut cs = w.chars();
        if let Some(c) = cs.next() {
            out.extend(c.to_uppercase());
            out.push_str(cs.as_str());
        }
    }
    out
}

/// Cut to [`MAX_TITLE`] chars at a word boundary.
fn clip_title(s: String) -> String {
    if s.chars().count() <= MAX_TITLE {
        return s;
    }
    let cut: String = s.chars().take(MAX_TITLE).collect();
    match cut.rfind(' ') {
        Some(i) if i > MAX_TITLE / 2 => cut[..i].trim_end_matches([':', ' ']).to_string(),
        _ => cut,
    }
}

/// What kind a keyword without its own kind means, from what the phrase built.
pub(crate) fn infer_kind(res: &Resolution) -> SignatureKind {
    let r = &res.recipe;
    if matches!(r.form, Form::Dome | Form::Cage) {
        return SignatureKind::Domain;
    }
    if r.has(Mechanic::Beam) || (r.has(Mechanic::Breath) && r.form.category() != FormCategory::Creature) {
        return SignatureKind::Beam;
    }
    match r.form.category() {
        FormCategory::Creature | FormCategory::Object if r.form != Form::Blob0 => SignatureKind::Summon,
        FormCategory::Held | FormCategory::Worn => SignatureKind::Transformation,
        FormCategory::Structure => SignatureKind::Summon,
        // A bare element / modifier ("ultimate fire"): an elemental blast.
        _ => SignatureKind::Beam,
    }
}

impl Lexicon {
    /// Tagged famous ultimate for a normalised name.
    fn tagged(&self, key: &str) -> Option<(SignatureKind, Option<String>)> {
        self.sig.names.get(key).cloned()
    }

    /// Tag of the construct a phrase resolved to: its display name, else its curated icon id
    /// ("giant kamehameha" is still the Kamehameha icon).
    fn tag_of_recipe(&self, res: &Resolution) -> Option<(SignatureKind, Option<String>)> {
        self.tagged(&normalize_key(&res.recipe.name)).or_else(|| match &res.recipe.model {
            ModelSource::Icon(id) => self.tagged(&normalize_key(&id.replace('_', " "))),
            ModelSource::Procedural { .. } => None,
        })
    }

    /// A signature always builds: a graft-only reading becomes a construct.
    fn as_construct(res: &mut Resolution) {
        if res.kind == ResolutionKind::Graft && res.stand_rush.is_none() && res.graft.action.is_none() {
            res.kind = ResolutionKind::Construct;
        }
    }

    /// Invocation grammar: `<keyword> <rest>` / `<rest> <suffix>`. `None` = not an invocation.
    pub(crate) fn invocation(&self, toks: &[Tok], bangs: usize, spoken: bool) -> Option<Resolution> {
        let skip = toks.iter().take_while(|t| matches!(gram_of(&t.text), Some(Gram::Article | Gram::Filler | Gram::Cast))).count();
        let body = toks.get(skip..).unwrap_or(&[]);
        if body.is_empty() {
            return None;
        }
        // A whole phrase that is itself a tagged name ("final flash") is not "final" + "flash".
        let whole = body.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
        if self.tagged(&whole).is_some() {
            return None;
        }
        // An exact curated / lexicon name wins over the grammar ("the ultimate nullifier", "final
        // fantasy" are things, not "ultimate" + "nullifier").
        let full = toks.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
        let exact = |k: &str| k.contains(' ') && (self.icons.alias.contains_key(k) || self.entry(k).is_some());
        if exact(&whole) || exact(&full) {
            return None;
        }
        let matches_at = |words: &[String], at: usize| body.len() >= at + words.len() && body[at..at + words.len()].iter().zip(words).all(|(t, w)| t.text == *w);
        // (invocation, rest, is a suffix)
        let mut hit: Option<(&Invocation, Vec<Tok>, bool)> = None;
        for inv in &self.sig.prefixes {
            if matches_at(&inv.words, 0) {
                hit = Some((inv, body[inv.words.len()..].to_vec(), false));
                break;
            }
        }
        if hit.is_none() {
            for inv in &self.sig.suffixes {
                let n = inv.words.len();
                if body.len() >= n && matches_at(&inv.words, body.len() - n) {
                    hit = Some((inv, body[..body.len() - n].to_vec(), true));
                    break;
                }
            }
        }
        let (inv, rest, suffix) = hit?;
        // Leading articles of the rest ("domain expansion: the iron coffin").
        let rest: Vec<Tok> = rest.into_iter().skip_while(|t| matches!(gram_of(&t.text), Some(Gram::Article | Gram::Filler))).collect();
        let rest_key = rest.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
        let bare = rest.is_empty();
        let mut res = if bare {
            let alone = inv.alone.clone().unwrap_or_else(|| inv.words.join(" "));
            let (t, _) = crate::norm::tokenize(&alone);
            self.resolve_mode_inner(t, bangs, spoken)
        } else {
            self.resolve_mode_inner(rest, bangs, spoken)
        };
        // A bare element / modifier ("ultimate fire") is an elemental blast.
        let modifier_only = res.kind == ResolutionKind::Graft || res.recipe.form == Form::Blob0;
        Self::as_construct(&mut res);
        let tag = self.tagged(&rest_key).or_else(|| self.tag_of_recipe(&res));
        let inferred = if modifier_only { SignatureKind::Beam } else { infer_kind(&res) };
        let kind = inv.kind.or(tag.as_ref().map(|t| t.0)).unwrap_or(inferred);
        let title = match (bare, suffix) {
            (true, _) => inv.title.clone(),
            (false, false) => format!("{}: {}", inv.title, title_case(&rest_key)),
            (false, true) => format!("{} {}", title_case(&rest_key), inv.title),
        };
        res.explanation.insert(0, format!("{} \u{2192} signature {kind:?}", inv.words.join(" ")));
        res.confidence = res.confidence.max(0.9);
        res.signature = Some(SignatureCall { kind, title: clip_title(title), invocation: Some(inv.words.join(" ")) });
        Some(res)
    }

    /// Tag a finished resolution: a famous ultimate by name, or a Beam yelled with enough "!".
    pub(crate) fn tag_signature(&self, res: &mut Resolution, whole: &str, bangs: usize) {
        if res.signature.is_some() || res.stand_rush.is_some() || res.graft.action.is_some() || res.incantation.is_some() {
            return;
        }
        let name_key = normalize_key(&res.recipe.name);
        let tag = self.tagged(whole).map(|t| (t, whole.to_string())).or_else(|| self.tag_of_recipe(res).map(|t| (t, name_key.clone())));
        if let Some(((kind, title), key)) = tag {
            Self::as_construct(res);
            let title = title.unwrap_or_else(|| {
                // Curated icons carry proper capitalisation ("Kamehameha"); lexicon names are lower-case.
                if res.recipe.name.chars().any(|c| c.is_uppercase()) { res.recipe.name.clone() } else { title_case(&key) }
            });
            res.explanation.insert(0, format!("{key} \u{2192} signature {kind:?}"));
            res.signature = Some(SignatureCall { kind, title: clip_title(title), invocation: None });
            return;
        }
        let beam = res.kind == ResolutionKind::Construct && res.recipe.has(Mechanic::Beam);
        if self.sig.bang_beams > 0 && bangs >= self.sig.bang_beams && beam {
            let title = if res.recipe.name.is_empty() { title_case(whole) } else { title_case(&res.recipe.name.to_lowercase()) };
            res.explanation.insert(0, format!("{bangs}\u{00d7}! beam \u{2192} signature Beam"));
            res.signature = Some(SignatureCall { kind: SignatureKind::Beam, title: clip_title(title), invocation: None });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_table_parses() {
        let (t, warn) = SignatureTable::parse(DEFAULT_TABLE);
        assert!(warn.is_none(), "{warn:?}");
        assert!(t.prefixes.iter().any(|i| i.words == ["domain", "expansion"] && i.kind == Some(SignatureKind::Domain)));
        assert!(t.suffixes.iter().any(|i| i.words == ["domain"]));
        assert_eq!(t.names.get("kamehameha").map(|n| n.0), Some(SignatureKind::Beam));
        // Longest first, so "domain expansion" wins over a shorter overlapping keyword.
        assert!(t.prefixes.windows(2).all(|w| w[0].words.len() >= w[1].words.len()));
    }

    #[test]
    fn hostile_table_is_empty_not_a_panic() {
        let (t, warn) = SignatureTable::parse("(invocations: [(words: 7)])");
        assert!(warn.is_some());
        assert!(t.prefixes.is_empty() && t.names.is_empty());
        let (t, warn) = SignatureTable::parse("()");
        assert!(warn.is_none() && t.prefixes.is_empty());
    }

    #[test]
    fn titles() {
        assert_eq!(title_case("coffin of the iron mountain"), "Coffin of the Iron Mountain");
        assert_eq!(title_case("of the mice"), "Of the Mice");
        let long = clip_title("Domain Expansion: ".to_string() + &"Very Long Words ".repeat(8));
        assert!(long.chars().count() <= MAX_TITLE, "{long}");
        assert!(!long.ends_with(' '));
    }
}
