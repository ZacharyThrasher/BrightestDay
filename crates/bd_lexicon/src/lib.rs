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
mod intent;
mod invocation;
mod norm;
mod parse;
mod phonetic;
mod pokemon;
mod powers;
mod refer;
mod relations;
pub mod tables;
mod transformer;

#[cfg(feature = "builder")]
pub mod builder;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

use bd_core::mechanics::{Element, Feature, Form, Mechanic, ModelSource, Recipe, Temperament};
use bd_core::shape::ShapeSpec;

pub use data::LoadError;
pub use icons::IconDef;
pub use intent::{Delivery, Intent, Piloting};
pub use invocation::{MAX_TITLE as SIGNATURE_MAX_TITLE, SignatureCall, SignatureKind};
pub use norm::{MAX_INPUT_CHARS, MAX_TOKENS, normalize_key};
pub use phonetic::phonetic_key;
pub use refer::{Refer, Targeted};

/// The lexicon's tolerant scan of `assets/powers/*.ron` (names / tiers / kind only) — for tools
/// (bd_sim's `powers_check` validator) that want the exact warnings the game's load would give.
pub mod power_scan {
    pub use crate::powers::{PowerDef as ScannedPower, PowerKind as ScannedKind, cross_file_collisions, parse_powers, parse_powers_named};
}

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

/// A command-like phrase the game should treat as an *action* on constructs the caster already owns,
/// rather than (only) as mechanics to add. The resolver cannot see game state, so it reports the
/// intent and the game decides (and falls back to the ordinary graft/construct when it doesn't apply).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GraftAction {
    /// "transform", "make it transform", "robot mode", "switch modes": toggle the aimed construct's
    /// mode if it already has `Transforms`, otherwise graft `Transforms` onto it.
    Transform,
    /// "roll out", "transform and roll out", "autobots roll out": toggle *every* owned Transforms
    /// construct (no aim needed).
    RollOut,
    /// "combine", "form devastator", "merge": merge the caster's owned Transforms constructs into one
    /// combiner (the game requires ≥ 3). For a bare combiner name ("devastator") the resolution is a
    /// normal Construct and this is only a hint the game may honour when the caster owns ≥ 3.
    Combine,
    /// "get in", "hop in", "ride it", "let me drive", "take the wheel", "mount up": take the wheel of an
    /// own construct already summoned (the aimed one, else the nearest boardable one): graft `Piloted`
    /// and seat the caster (host-validated). The recipe is named [`BOARD_COMMAND`] so the sim knows it.
    Board,
}

/// Recipe name of a [`GraftAction::Board`] command phrase (the sim's `graft::command_of` reads it).
pub const BOARD_COMMAND: &str = "board";

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
    /// Set for command phrases ("roll out", "combine"); see [`GraftAction`].
    pub action: Option<GraftAction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolution {
    pub kind: ResolutionKind,
    /// Always normalised and valid (unknown words give a glowing `Form::Blob0`, never nothing).
    pub recipe: Recipe,
    pub graft: Graft,
    /// Curated voxel model for icons (Mjolnir, Godzilla, Pikachu...), else None (procedural).
    pub shape: Option<ShapeSpec>,
    /// Second curated mode for two-mode (`Transforms`) icons — robot mode when `shape` is the
    /// vehicle/beast mode; panels share `part` ids. None for single-mode icons and procedural constructs.
    pub alt_shape: Option<ShapeSpec>,
    /// Power id whose incantation prefixed the phrase ("gomu gomu no pistol" -> "gomu_gomu"; the rest of
    /// the phrase resolved normally). The game applies that power's delivery if the caster holds it.
    pub incantation: Option<String>,
    /// Every power whose incantation matched (several powers may share one: Luffy's Gomu Gomu and
    /// Nika both use "gomu gomu no"); the game uses the one the caster holds. `incantation` = first.
    pub incantation_powers: Vec<String>,
    /// Stand-rush punches typed ("ORA ORA ORA" / "MUDAMUDAMUDA" -> 3). When set with nothing else in
    /// the phrase, `kind` is Graft and the game should drive the caster's stand, not build anything.
    pub stand_rush: Option<u16>,
    /// Signature tier (COMBAT.md §5j): "domain expansion <rest>", "bankai <rest>", a tagged famous
    /// ultimate ("kamehameha"), a yelled beam. The recipe is still the ordinary resolution (of the
    /// rest); the game sends it as a signature cast and bd_sim amplifies it. `assets/signatures.ron`.
    pub signature: Option<SignatureCall>,
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
    pub(crate) powers: Vec<powers::PowerDef>,
    /// (incantation tokens, power index), longest first
    pub(crate) incant: Vec<(Vec<String>, usize)>,
    /// Signature tier vocabulary (`assets/signatures.ron`, invocation.rs).
    pub(crate) sig: invocation::SignatureTable,
}

impl Lexicon {
    /// Load `lexicon.bin` plus the icon files next to it (`icons.ron`, then `icons/*.ron` by name).
    /// A missing/corrupt lexicon.bin is an error (use [`Lexicon::icons_only`] as the fallback); icon
    /// problems never fail the load — see [`Lexicon::load_warnings`].
    pub fn load(path: impl AsRef<Path>) -> Result<Lexicon, LoadError> {
        let path = path.as_ref();
        let data = data::LexData::map_file(path)?;
        let files = path.parent().map(Self::icon_files).unwrap_or_default();
        let powers = path.parent().map(Self::power_files).unwrap_or_default();
        let mut lex = Self::assemble(Some(data), icons::Icons::parse_files(&files), Some(&powers));
        // A signatures.ron next to lexicon.bin overrides the compiled-in table (data authoring).
        if let Some(src) = path.parent().and_then(|d| std::fs::read_to_string(d.join("signatures.ron")).ok()) {
            let (t, warn) = invocation::SignatureTable::parse(&src);
            match warn {
                Some(w) => lex.icons.warnings.push(w),
                None => lex.sig = t,
            }
        }
        Ok(lex)
    }

    /// Build from raw parts: lexicon.bin bytes, icon files `(name, ron)` in priority order, power
    /// files `(name, ron)` in load order ([`Lexicon::power_files`]; names appear in warnings).
    pub fn from_parts(bytes: Vec<u8>, icon_files: &[(String, String)], power_files: &[(String, String)]) -> Result<Lexicon, LoadError> {
        let data = data::LexData::parse(bytes)?;
        Ok(Self::assemble(Some(data), icons::Icons::parse_files(icon_files), Some(power_files)))
    }

    /// Read `<dir>/powers/*.ron` sorted by file name (the same order bd_sim compiles them in) as
    /// (`"powers/<file>"`, contents).
    pub fn power_files(dir: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir.join("powers")) {
            let mut paths: Vec<_> =
                rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().map(|x| x == "ron").unwrap_or(false)).collect();
            paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            for p in paths {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    let name = p.file_name().map(|n| format!("powers/{}", n.to_string_lossy())).unwrap_or_default();
                    out.push((name, s));
                }
            }
        }
        out
    }

    /// Ids of the powers loaded from assets/powers/ (plus nothing else).
    pub fn power_ids(&self) -> Vec<&str> {
        self.powers.iter().map(|p| p.id.as_str()).collect()
    }

    /// TOOLBOX chains: the phrase reads as one clean THING on its own — a construct (or a grant) the
    /// ring fully understood, not a bare modifier ("a big" as a size graft), not a guess ("a big" as
    /// a Wikidata alias at 0.74), not an improvisation. A comma / "and" / "into" only splits a line
    /// into chain steps when every piece is a move or such a thing.
    pub fn is_clean_thing(res: &Resolution) -> bool {
        res.recipe.grants.is_some() || (res.kind == ResolutionKind::Construct && !Self::is_fallback(res) && res.confidence >= 0.9)
    }

    /// True when the ring had to improvise: nothing (or only part) of the phrase was understood. The
    /// game logs these to `logs/unresolved.txt` so playtests grow the vocabulary. Never means "no".
    pub fn is_fallback(res: &Resolution) -> bool {
        if res.stand_rush.is_some() || res.graft.action.is_some() {
            return false;
        }
        let unknown_words = res.ignored.iter().any(|w| !w.ends_with("(blocked)"));
        let nothing =
            res.recipe.form == Form::Blob0 && res.graft.mechanics.is_empty() && res.graft.element.is_none() && res.incantation.is_none();
        nothing || unknown_words || res.confidence < 0.35
    }

    /// TOOLBOX shout hygiene: `text` (articles aside) is exactly the name / alias of a curated icon
    /// ("zelda shield" → the Hylian Shield) — not a power's grant name.
    pub fn is_icon_name(&self, text: &str) -> bool {
        let k = normalize_key(text);
        let k = k.split(' ').filter(|w| !matches!(*w, "a" | "an" | "the")).collect::<Vec<_>>().join(" ");
        self.icons.alias.get(&k).and_then(|&i| self.icons.defs.get(i)).is_some_and(|d| d.file != "powers.ron")
    }

    /// TOOLBOX shout hygiene: an everyday dictionary word (wordfreq-common and not a person's name,
    /// a grammar word, or a modifier / verb / element word the ring knows) — as opposed to a coined
    /// or proper word ("rasengan", "kamehameha", "fus", "zelda", "kirby").
    pub fn is_everyday_word(&self, w: &str) -> bool {
        let w = normalize_key(w);
        if w.is_empty() || w.contains(' ') {
            return false;
        }
        if w.bytes().all(|b| b.is_ascii_digit()) || parse::gram_of(&w).is_some() {
            return true;
        }
        if self.t.adj.contains_key(&w) || self.t.verb.contains_key(&w) || self.t.intens.contains_key(&w) || self.t.pure.contains_key(&w) || self.t.coll.contains_key(&w) {
            return true;
        }
        // wordfreq-common (zipf >= 3.3) and not a person's name. (Rarer dictionary words —
        // "surfboard", "flamethrower" — count as coined here; the validator's hygiene lint flags
        // such a shout when the lexicon reads it as a thing.)
        let common = |k: &str| self.entry(k).is_some_and(|(_, r)| r.flags & data::F_PERSON == 0 && r.flags & data::F_COMMON != 0);
        common(&w) || self.t.infl.get(&w).is_some_and(|b| common(b))
    }

    /// Read `<dir>/icons.ron` and `<dir>/icons/*.ron` (sorted) as (file name, contents).
    pub fn icon_files(dir: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let base = dir.join("icons.ron");
        if let Ok(s) = std::fs::read_to_string(&base) {
            out.push(("icons.ron".to_string(), s));
        }
        if let Ok(rd) = std::fs::read_dir(dir.join("icons")) {
            let mut paths: Vec<_> =
                rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().map(|x| x == "ron").unwrap_or(false)).collect();
            paths.sort();
            for p in paths {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    let name = p.file_name().map(|n| format!("icons/{}", n.to_string_lossy())).unwrap_or_default();
                    out.push((name, s));
                }
            }
        }
        out
    }

    /// Build from raw bytes plus icon file contents `(name, ron)` in priority order.
    pub fn from_bytes(bytes: Vec<u8>, icon_files: &[(String, String)]) -> Result<Lexicon, LoadError> {
        let data = data::LexData::parse(bytes)?;
        Ok(Self::assemble(Some(data), icons::Icons::parse_files(icon_files), None))
    }

    /// Icons-only fallback (no lexicon.bin): curated names still resolve, everything else is a blob.
    pub fn icons_only(icon_files: &[(String, String)]) -> Lexicon {
        Self::assemble(None, icons::Icons::parse_files(icon_files), None)
    }

    /// Parse every icon recipe and shape now (they are otherwise parsed lazily on first use) and report
    /// the ones that are malformed — for tools/tests; the game never needs to call this.
    pub fn validate_icon_shapes(&self) -> Vec<String> {
        let mut out = Vec::new();
        for d in &self.icons.defs {
            if let Some(e) = d.recipe_error() {
                out.push(format!("{}: [{}] recipe: {e}", d.file, d.id));
            }
            for (what, s) in [("shape", &d.shape), ("alt_shape", &d.alt_shape)] {
                if let Some(e) = s.parse_error() {
                    out.push(format!("{}: [{}] {what}: {e}", d.file, d.id));
                }
            }
        }
        out
    }

    /// Informational load notes: aliases that yield to common words, power names that took over an
    /// icon's name. Not problems; useful when authoring packs.
    pub fn load_notes(&self) -> &[String] {
        &self.icons.notes
    }

    /// Non-fatal problems found while loading icon files (malformed entries, duplicate names).
    pub fn load_warnings(&self) -> &[String] {
        &self.icons.warnings
    }

    fn assemble(data: Option<data::LexData>, icons: icons::Icons, power_files: Option<&[(String, String)]>) -> Lexicon {
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
        let mut powers = Vec::new();
        for (name, src) in power_files.unwrap_or_default() {
            let (p, warn) = powers::parse_powers_named(name, src);
            powers.extend(p);
            icons.warnings.extend(warn);
        }
        if power_files.is_some() {
            icons.warnings.extend(powers::cross_file_collisions(&powers));
            for p in &powers {
                // A kit's names never grant: they stay the costume summon's ("doomfist"), and
                // "<kit> <move>" is resolved by the game against bd_sim's table (TOOLBOX).
                if p.kind == powers::PowerKind::Kit {
                    continue;
                }
                icons.add_power(powers::power_entry(p));
                for e in powers::tier_entries(p) {
                    icons.add_power(e);
                }
            }
            for e in powers::random_entries() {
                icons.add_power(e);
            }
        }
        let mut incant: Vec<(Vec<String>, usize)> = powers
            .iter()
            .enumerate()
            .flat_map(|(i, p)| p.incantations.iter().map(move |s| (s.split(' ').map(String::from).collect(), i)))
            .collect();
        incant.sort_by(|a: &(Vec<String>, usize), b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
        if let Some(d) = &data {
            let claimed: Vec<String> = powers.iter().flat_map(|p| p.claim.iter().cloned()).collect();
            shadow_domain_aliases(&mut icons, d, &claimed);
        }
        Lexicon { data, icons, t, powers, incant, sig: invocation::SignatureTable::default_table() }
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
        // VOICE == TYPING: words the typed reading understands plainly (nothing ignored, confident)
        // read exactly as typed; the sound-alike rescue is for what typing would NOT understand
        // ("picked you" -> Pikachu, "mule near" -> Mjolnir), never a re-reading of right words
        // ("shadow ball" is not Cthulhu, "limit break" is not a porg). Voice fluency bench.
        let typed = self.resolve_mode(text, false);
        if Self::heard_exactly(&typed) {
            return typed;
        }
        self.resolve_mode(text, true)
    }

    /// A typed resolution that needs no sound-alike help: nothing improvised, no word ignored,
    /// confidence >= 0.85 ("mule near" 0.76 and "spirit bum" 0.73 still get rescued).
    fn heard_exactly(res: &Resolution) -> bool {
        !Self::is_fallback(res) && res.ignored.is_empty() && res.confidence >= 0.85
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
        let icon_names = self.icons.defs.iter().map(|d| d.name.clone());
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
                if self.t.blocked.contains(t) || t.strip_suffix('s').map(|s| self.t.blocked.contains(s)).unwrap_or(false) {
                    return true;
                }
            }
            let squashed: String = k.chars().filter(|c| !c.is_whitespace()).collect();
            let squeezed = norm::squeeze2(&squashed);
            if self.t.blocked_sub.iter().any(|b| squashed.contains(b.as_str()) || squeezed.contains(b.as_str())) {
                return true;
            }
        }
        false
    }

    pub(crate) fn rec_recipe(&self, key: &str, rec: &data::Rec) -> Recipe {
        let d = self.data.as_ref();
        let display = d.map(|d| d.pool_str(rec.display)).filter(|s| !s.is_empty()).unwrap_or(key);
        let temper = tables::ALL_TEMPERAMENT.get(rec.temper as usize).copied();
        Recipe {
            name: display.to_string(),
            form: tables::ALL_FORM.get(rec.form as usize).copied().unwrap_or(Form::Blob0),
            mechanics: parse::bits_to_mechs(rec.mech),
            element: tables::ALL_ELEMENT.get(rec.element as usize).copied().unwrap_or_default(),
            scale: rec.scale,
            mass: rec.mass,
            count: rec.count.max(1),
            accent: rec.accent.map(|a| a.map(|x| x as f32 / 255.0)),
            model: ModelSource::Procedural { seed: data::fnv64(key.as_bytes()) as u32 },
            payload: None,
            grants: None,
            features: parse::bits_to_feats(rec.feat),
            temperament: temper.unwrap_or_default(),
            intensity: if temper.is_some() { 0.6 } else { 0.0 },
            mount: None,
            opening: None,
        }
        .normalized()
    }

    pub(crate) fn icon_recipe(&self, d: &IconDef) -> Recipe {
        let mut r = d.recipe().clone();
        if r.temperament != Temperament::Calm && r.intensity <= 0.0 {
            r.intensity = 0.6;
        }
        r.normalized()
    }
}

/// Domain packs (`assets/icons/*.ron`) may not hijack common words: an alias like "scorpion" (a game
/// tank) that is a hand-anchored or common lexicon word is dropped (with a load warning). The base
/// `icons.ron` is curated alongside the lexicon and keeps all its aliases.
fn shadow_domain_aliases(icons: &mut icons::Icons, d: &data::LexData, claimed: &[String]) {
    let mut drop: Vec<String> = Vec::new();
    for (alias, &idx) in icons.alias.iter() {
        let Some(def) = icons.defs.get(idx) else {
            continue;
        };
        if def.file == "icons.ron" {
            continue;
        }
        if def.file == "powers.ron" && claimed.iter().any(|c| c == alias) {
            // a power's explicit `claim` ("infinity" is Limitless, not a spirit)
            continue;
        }
        if def.file == "powers.ron" {
            // power names keep everything except single very common words ("cream" stays food)
            // ...and hand-anchored everyday phrases ("storm cloud" isn't the weather power's pickup)
            let rec = d.entry_id(alias).and_then(|id| d.rec(id));
            let common_single = !alias.contains(' ') && rec.map(|r| r.rank_f() >= 4.5).unwrap_or(false);
            let anchored_phrase = alias.contains(' ') && rec.map(|r| r.flags & data::F_ANCHOR != 0).unwrap_or(false);
            if !common_single && !anchored_phrase {
                continue;
            }
        }
        let Some(rec) = d.entry_id(alias).and_then(|id| d.rec(id)) else {
            continue;
        };
        // a hand-shaped Pokemon icon beats the procedural PokeAPI entry for the same Pokemon ("ditto")
        if rec.flags & data::F_POKEMON != 0 && (def.file.ends_with("pokemon.ron") || def.source.to_lowercase().contains("pokemon")) {
            continue;
        }
        let anchored = rec.flags & data::F_ANCHOR != 0;
        // Only single common words and hand-anchored phrases are protected: an exact multi-word name
        // ("man of steel", "eye of sauron") always beats a WordNet/ConceptNet phrase of the same text.
        // "Everyday" = frequency, not source: "punch" stays a punch even though a curated names file
        // also lists it, so a zoo-monkey icon called Punch can't hijack "sucker punch" / "punch".
        let common = !alias.contains(' ') && rec.flags & data::F_COMMON != 0;
        if anchored || common {
            drop.push(alias.clone());
        }
    }
    drop.sort();
    for a in drop {
        if let Some(idx) = icons.alias.remove(&a) {
            let (id, file) = icons.defs.get(idx).map(|d| (d.id.clone(), d.file.clone())).unwrap_or_default();
            icons.notes.push(format!("{file}: alias \"{a}\" of [{id}] yields to the common word (use a more specific name)"));
            icons.pretty.retain(|(_, k, _)| *k != a);
            let still_named = icons.alias.values().any(|v| *v == idx);
            let _ = still_named; // phonetic indexes are built lazily from the final alias set
        }
    }
}

#[cfg(test)]
mod lib_tests {
    use super::*;

    const TWO_MODE: &str = r#"[
        IconEntry(
            names: ["test prime", "testbot"],
            recipe: (name: "Test Prime", form: Vehicle, mechanics: [Charge, Transforms], element: Light,
                     scale: 8.0, mass: 9000.0, count: 1, accent: None, model: Icon("test_prime")),
            shape: Some((prims: [(kind: Box, center: (0.0, 4.0, 0.0), size: (8.0, 8.0, 20.0), part: 1)])),
            alt_shape: Some((prims: [(kind: Box, center: (0.0, 16.0, 0.0), size: (8.0, 12.0, 4.0), part: 1),
                                     (kind: Box, center: (0.0, 25.0, 0.0), size: (3.0, 3.0, 3.0), part: 2)])),
            source: "test",
        ),
    ]"#;

    #[test]
    fn two_mode_icons_expose_alt_shape() {
        let lex = Lexicon::icons_only(&[("t.ron".into(), TWO_MODE.into())]);
        assert!(lex.load_warnings().is_empty(), "{:?}", lex.load_warnings());
        let r = lex.resolve("testbot");
        assert!(r.recipe.has(Mechanic::Transforms));
        assert!(r.shape.is_some() && r.alt_shape.is_some());
        let r = lex.resolve("roll out");
        assert_eq!(r.kind, ResolutionKind::Graft);
        assert_eq!(r.graft.action, Some(GraftAction::RollOut));
    }

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
