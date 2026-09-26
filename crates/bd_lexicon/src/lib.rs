//! bd_lexicon — Brightest Day's runtime resolver: typed or spoken text -> `bd_core::mechanics::Recipe`.
//!
//! **Vocabulary vs. mechanics** (docs/DESIGN.md): everything here is *vocabulary knowledge* — which of the
//! game's closed-set forms/mechanics/elements a word means, derived offline from WordNet, ConceptNet,
//! dariusk/corpora, PokeAPI and Wikidata (`tools/lexicon_build` -> `assets/lexicon.bin`) plus curated
//! pop-culture icons (`assets/icons.ron`). What those tags *do* is decided by bd_sim alone.
//!
//! ```no_run
//! let lex = bd_lexicon::Lexicon::load("assets/lexicon.bin").expect("lexicon");
//! let r = lex.resolve("three flaming swords");
//! assert_eq!(r.recipe.count, 3);
//! println!("{:?} {}", r.signature(), r.explanation.join("; "));
//! ```
//!
//! Guarantees: `resolve`, `resolve_spoken`, `complete` never panic for any input, always return a
//! normalised `Recipe`, and run well under 1 ms (input is truncated to 120 chars / 16 tokens).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod data;
mod fuzzy;
mod icons;
mod norm;
mod parse;
mod phonetic;
pub mod tables;

#[cfg(feature = "builder")]
pub mod builder;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

use bd_core::mechanics::{Element, Feature, Form, Mechanic, ModelSource, Recipe, Temperament};
use bd_core::shape::ShapeSpec;

pub use data::LoadError;
pub use icons::IconDef;
pub use norm::{MAX_INPUT_CHARS, MAX_TOKENS, normalize_key};
pub use phonetic::phonetic_key;

/// Is the phrase a whole construct, or only modifiers to graft onto something you aim at?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolutionKind {
    /// A thing to build ("hammer", "giant flaming mjolnir", "eeyore shooting laser beams").
    Construct,
    /// Only modifiers ("wings", "legs", "make it explode", "bouncy", "that returns"). The sim grafts
    /// [`Resolution::graft`] onto the aimed construct/terrain; if nothing is aimed it may build
    /// `recipe` (a small blob carrying those modifiers) instead.
    Graft,
}

/// The phrase read as a graft: what it adds to whatever it is cast onto. Always populated, even for
/// constructs ("wings" is a Wings construct *and* a Flies graft), so aiming decides.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Graft {
    /// Sorted, unique.
    pub mechanics: Vec<Mechanic>,
    pub element: Option<Element>,
    /// Multiplier on the target's size (1.0 = unchanged).
    pub scale_mul: f32,
    pub count: Option<u8>,
    /// Sorted, unique.
    pub features: Vec<Feature>,
    pub temperament: Option<Temperament>,
    pub intensity: Option<f32>,
    pub payload: Option<Box<Recipe>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolution {
    pub kind: ResolutionKind,
    /// Always normalised and valid (unknown words give a glowing `Form::Blob0`, never nothing).
    pub recipe: Recipe,
    pub graft: Graft,
    /// Curated voxel model for icons (Mjolnir, Godzilla, Pikachu...), else None (procedural).
    pub shape: Option<ShapeSpec>,
    /// 0..1. 0 = nothing recognised.
    pub confidence: f32,
    /// Human-readable reasoning, e.g. "maul → hammer (WordNet hypernym)", "hamer → hammer (typo)".
    pub explanation: Vec<String>,
    /// Words that were not understood or were blocked (slurs); reported, never fatal.
    pub ignored: Vec<String>,
}

/// Stable identity of *what a construct does* (form + mechanics + element + payload form), for the
/// sim's staleness / novelty bookkeeping. Independent of wording, count and size.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Signature {
    pub form: Option<Form>,
    pub mechanics: Vec<Mechanic>,
    pub element: Element,
    pub payload_form: Option<Form>,
}

impl Signature {
    /// Stable 64-bit key (FNV-1a over variant names — identical across runs, platforms and builds).
    pub fn key(&self) -> u64 {
        data::fnv64(self.to_string().as_bytes())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.form {
            Some(x) => write!(f, "{}", tables::form_name(x))?,
            None => write!(f, "graft")?,
        }
        for m in &self.mechanics {
            write!(f, "+{}", tables::mechanic_name(*m))?;
        }
        write!(f, "/{}", tables::element_name(self.element))?;
        if let Some(p) = self.payload_form {
            write!(f, ">{}", tables::form_name(p))?;
        }
        Ok(())
    }
}

impl Resolution {
    pub fn signature(&self) -> Signature {
        match self.kind {
            ResolutionKind::Construct => Signature {
                form: Some(self.recipe.form),
                mechanics: self.recipe.mechanics.clone(),
                element: self.recipe.element,
                payload_form: self.recipe.payload.as_ref().map(|p| p.form),
            },
            ResolutionKind::Graft => Signature {
                form: None,
                mechanics: self.graft.mechanics.clone(),
                element: self.graft.element.unwrap_or_default(),
                payload_form: self.graft.payload.as_ref().map(|p| p.form),
            },
        }
    }
}

pub(crate) struct Tables {
    pub adj: HashMap<String, usize>,
    pub verb: HashMap<String, usize>,
    pub intens: HashMap<String, f32>,
    pub coll: HashMap<String, (u8, bool)>,
    pub infl: HashMap<String, String>,
    pub pure: HashMap<String, (u8, u8)>,
    pub hosts: HashSet<String>,
    pub blocked: HashSet<String>,
    pub blocked_sub: Vec<String>,
}

/// The loaded vocabulary. Cheap to share (`&Lexicon` is all resolution needs; it is `Send + Sync`).
pub struct Lexicon {
    pub(crate) data: Option<data::LexData>,
    pub(crate) icons: icons::Icons,
    pub(crate) t: Tables,
}

impl Lexicon {
    /// Load `lexicon.bin` plus the icon files next to it (`icons.ron`, then `icons/*.ron` by name).
    /// A missing/corrupt lexicon.bin is an error (use [`Lexicon::icons_only`] as the fallback); icon
    /// problems never fail the load — see [`Lexicon::load_warnings`].
    pub fn load(path: impl AsRef<Path>) -> Result<Lexicon, LoadError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        let data = data::LexData::parse(bytes)?;
        let files = path.parent().map(Self::icon_files).unwrap_or_default();
        Ok(Self::assemble(
            Some(data),
            icons::Icons::parse_files(&files),
        ))
    }

    /// Read `<dir>/icons.ron` and `<dir>/icons/*.ron` (sorted) as (file name, contents).
    pub fn icon_files(dir: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let base = dir.join("icons.ron");
        if let Ok(s) = std::fs::read_to_string(&base) {
            out.push(("icons.ron".to_string(), s));
        }
        if let Ok(rd) = std::fs::read_dir(dir.join("icons")) {
            let mut paths: Vec<_> = rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().map(|x| x == "ron").unwrap_or(false))
                .collect();
            paths.sort();
            for p in paths {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    let name = p
                        .file_name()
                        .map(|n| format!("icons/{}", n.to_string_lossy()))
                        .unwrap_or_default();
                    out.push((name, s));
                }
            }
        }
        out
    }

    /// Build from raw bytes plus icon file contents `(name, ron)` in priority order.
    pub fn from_bytes(
        bytes: Vec<u8>,
        icon_files: &[(String, String)],
    ) -> Result<Lexicon, LoadError> {
        let data = data::LexData::parse(bytes)?;
        Ok(Self::assemble(
            Some(data),
            icons::Icons::parse_files(icon_files),
        ))
    }

    /// Icons-only fallback (no lexicon.bin): curated names still resolve, everything else is a blob.
    pub fn icons_only(icon_files: &[(String, String)]) -> Lexicon {
        Self::assemble(None, icons::Icons::parse_files(icon_files))
    }

    /// Non-fatal problems found while loading icon files (malformed entries, duplicate names).
    pub fn load_warnings(&self) -> &[String] {
        &self.icons.warnings
    }

    fn assemble(data: Option<data::LexData>, icons: icons::Icons) -> Lexicon {
        let mut t = Tables {
            adj: HashMap::new(),
            verb: HashMap::new(),
            intens: HashMap::new(),
            coll: HashMap::new(),
            infl: HashMap::new(),
            pure: HashMap::new(),
            hosts: HashSet::new(),
            blocked: HashSet::new(),
            blocked_sub: Vec::new(),
        };
        if let Some(d) = &data {
            let m = &d.meta;
            for (i, a) in m.adjectives.iter().enumerate() {
                t.adj.insert(a.k.clone(), i);
            }
            for (i, v) in m.verbs.iter().enumerate() {
                t.verb.insert(v.k.clone(), i);
            }
            t.intens.extend(m.intensifiers.iter().cloned());
            for (k, n, s) in &m.collectives {
                t.coll.insert(k.clone(), (*n, *s));
            }
            t.infl.extend(m.inflections.iter().cloned());
            for (k, e, mm) in &m.pure {
                t.pure.insert(k.clone(), (*e, *mm));
            }
            t.hosts.extend(m.payload_hosts.iter().cloned());
            for b in &m.blocked {
                let w = b.trim_start_matches('*').to_string();
                if b.starts_with('*') {
                    t.blocked_sub.push(w.replace(' ', ""));
                }
                t.blocked.insert(w);
            }
        }
        let mut icons = icons;
        if let Some(d) = &data {
            shadow_domain_aliases(&mut icons, d);
        }
        Lexicon { data, icons, t }
    }

    /// Number of lexicon entries (not counting icons).
    pub fn len(&self) -> usize {
        self.data.as_ref().map(|d| d.n_entries()).unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0 && self.icons.defs.is_empty()
    }
    pub fn icon_count(&self) -> usize {
        self.icons.defs.len()
    }
    pub fn icons(&self) -> &[IconDef] {
        &self.icons.defs
    }

    /// Resolve typed text.
    pub fn resolve(&self, text: &str) -> Resolution {
        self.resolve_mode(text, false)
    }

    /// Resolve a speech-to-text transcript: adjacent-token merging, sound-alike keys and looser typo
    /// thresholds ("picked you" -> Pikachu, "mule near" -> Mjolnir, "god zilla" -> Godzilla).
    pub fn resolve_spoken(&self, text: &str) -> Resolution {
        self.resolve_mode(text, true)
    }

    /// Exact lookup of a normalised key (icons first), no phrase parsing.
    pub fn lookup(&self, key: &str) -> Option<Recipe> {
        let k = normalize_key(key);
        if let Some(&i) = self.icons.alias.get(&k) {
            return self.icons.defs.get(i).map(|d| self.icon_recipe(d));
        }
        let d = self.data.as_ref()?;
        let id = d.entry_id(&k)?;
        let rec = d.rec(id)?;
        Some(self.rec_recipe(&k, &rec))
    }

    /// Words for a speech recogniser's initial prompt: the `n` most distinctive curated names.
    pub fn bias_prompt(&self, n: usize) -> String {
        let mut seen = HashSet::new();
        let mut out: Vec<String> = Vec::new();
        let icon_names = self.icons.defs.iter().map(|d| d.recipe.name.clone());
        let bias = self.data.iter().flat_map(|d| d.meta.bias.iter().cloned());
        for s in icon_names.chain(bias) {
            if out.len() >= n {
                break;
            }
            if seen.insert(s.to_lowercase()) {
                out.push(s);
            }
        }
        out.join(", ")
    }

    /// True if a player name contains a slur / hate term (after leetspeak folding). Profanity is allowed.
    pub fn is_blocked_name(&self, name: &str) -> bool {
        let folded: String = name
            .chars()
            .take(256)
            .map(|c| match c {
                '0' => 'o',
                '1' | '!' | '|' => 'i',
                '3' => 'e',
                '4' | '@' => 'a',
                '5' | '$' => 's',
                '7' => 't',
                '8' => 'b',
                '9' => 'g',
                _ => c,
            })
            .collect();
        let raw_digits = normalize_key(name);
        let norm = normalize_key(&folded);
        for k in [&norm, &raw_digits] {
            for t in k.split(' ') {
                if t.is_empty() {
                    continue;
                }
                if self.t.blocked.contains(t)
                    || t.strip_suffix('s')
                        .map(|s| self.t.blocked.contains(s))
                        .unwrap_or(false)
                {
                    return true;
                }
            }
            let squashed: String = k.chars().filter(|c| !c.is_whitespace()).collect();
            let squeezed = norm::squeeze2(&squashed);
            if self
                .t
                .blocked_sub
                .iter()
                .any(|b| squashed.contains(b.as_str()) || squeezed.contains(b.as_str()))
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn rec_recipe(&self, key: &str, rec: &data::Rec) -> Recipe {
        let d = self.data.as_ref();
        let display = d
            .map(|d| d.pool_str(rec.display))
            .filter(|s| !s.is_empty())
            .unwrap_or(key);
        let temper = tables::ALL_TEMPERAMENT.get(rec.temper as usize).copied();
        Recipe {
            name: display.to_string(),
            form: tables::ALL_FORM
                .get(rec.form as usize)
                .copied()
                .unwrap_or(Form::Blob0),
            mechanics: parse::bits_to_mechs(rec.mech),
            element: tables::ALL_ELEMENT
                .get(rec.element as usize)
                .copied()
                .unwrap_or_default(),
            scale: rec.scale,
            mass: rec.mass,
            count: rec.count.max(1),
            accent: rec.accent.map(|a| a.map(|x| x as f32 / 255.0)),
            model: ModelSource::Procedural {
                seed: data::fnv64(key.as_bytes()) as u32,
            },
            payload: None,
            features: parse::bits_to_feats(rec.feat),
            temperament: temper.unwrap_or_default(),
            intensity: if temper.is_some() { 0.6 } else { 0.0 },
        }
        .normalized()
    }

    pub(crate) fn icon_recipe(&self, d: &IconDef) -> Recipe {
        let mut r = d.recipe.clone();
        if r.temperament != Temperament::Calm && r.intensity <= 0.0 {
            r.intensity = 0.6;
        }
        r.normalized()
    }
}

/// Domain packs (`assets/icons/*.ron`) may not hijack common words: an alias like "scorpion" (a game
/// tank) that is a hand-anchored or common lexicon word is dropped (with a load warning). The base
/// `icons.ron` is curated alongside the lexicon and keeps all its aliases.
fn shadow_domain_aliases(icons: &mut icons::Icons, d: &data::LexData) {
    let mut drop: Vec<String> = Vec::new();
    for (alias, &idx) in icons.alias.iter() {
        let Some(def) = icons.defs.get(idx) else {
            continue;
        };
        if def.file == "icons.ron" {
            continue;
        }
        let Some(rec) = d.entry_id(alias).and_then(|id| d.rec(id)) else {
            continue;
        };
        let anchored = rec.flags & data::F_ANCHOR != 0;
        let common = rec.flags & (data::F_ENTITY | data::F_POP | data::F_POKEMON) == 0
            && rec.rank_f() >= 2.5;
        if anchored || common {
            drop.push(alias.clone());
        }
    }
    drop.sort();
    for a in drop {
        if let Some(idx) = icons.alias.remove(&a) {
            let (id, file) = icons
                .defs
                .get(idx)
                .map(|d| (d.id.clone(), d.file.clone()))
                .unwrap_or_default();
            icons.warnings.push(format!("{file}: alias \"{a}\" of [{id}] shadowed by the common word (use a more specific name)"));
            icons.pretty.retain(|(_, k, _)| *k != a);
            let still_named = icons.alias.values().any(|v| *v == idx);
            if !still_named {
                for v in icons.phon.values_mut() {
                    v.retain(|i| *i != idx);
                }
                for v in icons.phon_del.values_mut() {
                    v.retain(|(i, _)| *i != idx);
                }
            }
        }
    }
}

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn signature_is_stable_and_order_free() {
        let s = Signature {
            form: Some(Form::Hammer),
            mechanics: vec![Mechanic::Blunt, Mechanic::Returning],
            element: Element::Electric,
            payload_form: None,
        };
        assert_eq!(s.to_string(), "Hammer+Blunt+Returning/Electric");
        assert_eq!(s.key(), data::fnv64(b"Hammer+Blunt+Returning/Electric"));
    }

    #[test]
    fn corrupt_input_is_an_error_not_a_panic() {
        assert!(Lexicon::from_bytes(vec![], &[]).is_err());
        assert!(Lexicon::from_bytes(b"BDLX\x04\0\0\0\xff\xff\xff\xff".to_vec(), &[]).is_err());
        assert!(Lexicon::from_bytes(vec![7u8; 1000], &[]).is_err());
        let lex = Lexicon::icons_only(&[]);
        let r = lex.resolve("anything at all");
        assert_eq!(r.recipe.form, Form::Blob0);
        assert!(Lexicon::load("definitely/missing/lexicon.bin").is_err());
    }
}
