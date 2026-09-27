//! Curated pop-culture icons: `assets/icons.ron` + `assets/icons/*.ron` (schema: docs/ICONS_SCHEMA.md).
//!
//! Each file is a RON list of `IconEntry { names, recipe: bd_core Recipe, shape: Option<ShapeSpec>, source }`.
//! Loading is fault tolerant: entries are split out of the list and parsed one by one, so a malformed
//! entry is skipped with a warning; later files never override names defined earlier (warning).

use std::collections::HashMap;

use bd_core::mechanics::{Mechanic, ModelSource, Recipe};
use bd_core::shape::ShapeSpec;
use serde::Deserialize;

use crate::norm::normalize_key;

#[derive(Clone, Debug, Deserialize)]
pub struct IconEntry {
    pub names: Vec<String>,
    pub recipe: Recipe,
    #[serde(default)]
    pub shape: Option<ShapeSpec>,
    /// Second hand-authored mode for `Transforms` icons (robot mode when `shape` is the alt/vehicle
    /// mode). Panels that move between modes share `part` ids with `shape`.
    #[serde(default)]
    pub alt_shape: Option<ShapeSpec>,
    #[serde(default)]
    pub source: String,
}

/// An icon recipe kept as RON text and deserialised on first use (with its derived graft list).
#[derive(Clone, Debug, Default)]
pub struct LazyRecipe {
    src: String,
    id: String,
    icon_model: bool,
    fallback_name: String,
    cell: std::sync::OnceLock<(Recipe, Vec<Mechanic>)>,
}

impl LazyRecipe {
    fn from_src(src: String, id: &str, icon_model: bool, fallback_name: &str) -> Self {
        LazyRecipe { src, id: id.to_string(), icon_model, fallback_name: fallback_name.to_string(), cell: Default::default() }
    }
    fn ready(r: Recipe, id: &str, icon_model: bool, fallback_name: &str) -> Self {
        let l = LazyRecipe {
            src: String::new(),
            id: id.to_string(),
            icon_model,
            fallback_name: fallback_name.to_string(),
            cell: Default::default(),
        };
        let _ = l.cell.set(finish(r, id, icon_model, fallback_name));
        l
    }
    fn get(&self) -> &(Recipe, Vec<Mechanic>) {
        self.cell.get_or_init(|| {
            let r = ron::from_str::<Recipe>(&self.src).unwrap_or_else(|_| Recipe::unknown(&self.fallback_name));
            finish(r, &self.id, self.icon_model, &self.fallback_name)
        })
    }
    pub fn parse_error(&self) -> Option<String> {
        if self.src.is_empty() {
            return None;
        }
        ron::from_str::<Recipe>(&self.src).err().map(|e| e.to_string())
    }
}

/// Post-process an icon recipe: model id, default name, normalisation, graft mechanics.
fn finish(mut recipe: Recipe, id: &str, icon_model: bool, fallback_name: &str) -> (Recipe, Vec<Mechanic>) {
    if icon_model {
        recipe.model = ModelSource::Icon(id.to_string());
    }
    if recipe.name.trim().is_empty() {
        recipe.name = fallback_name.to_string();
    }
    let recipe = recipe.normalized();
    let intrinsic = intrinsic(recipe.form);
    let mut graft: Vec<Mechanic> = recipe.mechanics.iter().copied().filter(|m| !intrinsic.contains(m)).collect();
    if graft.is_empty() {
        graft = recipe.mechanics.clone();
    }
    (recipe, graft)
}

/// A voxel model kept as RON text and parsed on first use (icon files are ~1 MB of shapes; parsing
/// them all at startup cost more than the rest of the lexicon load).
#[derive(Clone, Debug, Default)]
pub struct LazyShape {
    src: Option<String>,
    cell: std::sync::OnceLock<Option<ShapeSpec>>,
}

impl LazyShape {
    fn from_src(src: Option<String>) -> Self {
        let src = src.map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != "None");
        LazyShape { src, cell: std::sync::OnceLock::new() }
    }
    fn ready(shape: Option<ShapeSpec>) -> Self {
        let cell = std::sync::OnceLock::new();
        let src = shape.as_ref().map(|_| String::new());
        let _ = cell.set(shape);
        LazyShape { src, cell }
    }
    pub fn is_some(&self) -> bool {
        self.src.is_some()
    }
    /// Parse (once) and return the shape; a malformed shape yields None (see `parse_error`).
    pub fn get(&self) -> Option<&ShapeSpec> {
        self.cell
            .get_or_init(|| {
                let src = self.src.as_deref()?;
                ron::Options::default()
                    .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
                    .from_str::<Option<ShapeSpec>>(src)
                    .ok()
                    .flatten()
            })
            .as_ref()
    }
    pub fn parse_error(&self) -> Option<String> {
        let src = self.src.as_deref().filter(|s| !s.is_empty())?;
        ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
            .from_str::<Option<ShapeSpec>>(src)
            .err()
            .map(|e| e.to_string())
    }
}

/// A loaded icon.
#[derive(Clone, Debug)]
pub struct IconDef {
    /// Canonical id (first name, or the recipe's `Icon(id)`).
    pub id: String,
    pub names: Vec<String>,
    /// Display name (the recipe's `name`), available without parsing the recipe.
    pub name: String,
    recipe: LazyRecipe,
    /// Curated model (alt/vehicle mode for two-mode icons); parsed lazily.
    pub shape: LazyShape,
    /// Robot mode of a two-mode (`Transforms`) icon; parsed lazily.
    pub alt_shape: LazyShape,
    pub source: String,
    /// File it came from (for diagnostics).
    pub file: String,
}

impl IconDef {
    /// The icon's recipe (parsed on first use; a malformed recipe becomes a named glowing blob and is
    /// reported by `Lexicon::validate_icons`).
    pub fn recipe(&self) -> &Recipe {
        &self.recipe.get().0
    }
    /// Mechanics it contributes as a modifier/graft ("mjolnir hammer", aiming + "mjolnir").
    pub fn graft(&self) -> &[Mechanic] {
        &self.recipe.get().1
    }
    pub fn recipe_error(&self) -> Option<String> {
        self.recipe.parse_error()
    }
}

#[derive(Default)]
pub struct Icons {
    pub defs: Vec<IconDef>,
    /// normalised alias -> icon index
    pub alias: HashMap<String, usize>,
    /// (lowercase alias as authored, its normalised key, icon index) for completion ghost text
    pub pretty: Vec<(String, String, usize)>,
    /// every token appearing in an alias
    pub tokens: HashMap<String, usize>,
    /// Sound-alike indexes, built on first use from `alias` (see [`Icons::phon`]).
    phon_cell: std::sync::OnceLock<PhonIndex>,
    pub warnings: Vec<String>,
    /// Intentional policy outcomes (an alias yielding to a common word, a power claiming a name) —
    /// informational, not authoring errors.
    pub notes: Vec<String>,
    pretty_seen: std::collections::HashSet<String>,
    /// Aliases that came only from a recipe's display `name` (not the explicit `names` list): a later
    /// file's explicit name takes them over silently (dc's Megamind "Minion" vs the Despicable Me minions).
    weak: std::collections::HashSet<String>,
}

/// phonetic key -> icon indices, and single-symbol deletions of keys -> (icon, full key).
#[derive(Default)]
pub struct PhonIndex {
    pub exact: HashMap<String, Vec<usize>>,
    pub dels: HashMap<String, Vec<(usize, String)>>,
}

/// Split the top-level RON list into entry substrings (with their 1-based start line), honouring strings
/// and comments. Tolerates a missing `[`/`]` and trailing commas.
pub(crate) fn split_entries(src: &str) -> Vec<(usize, &str)> {
    let b = src.as_bytes();
    let n = b.len();
    let mut i = 0;
    let mut line = 1;
    let mut out = Vec::new();
    // skip to the opening '[' (ignoring #![enable(..)] attributes and comments)
    let mut depth_list = false;
    while i < n {
        match b[i] {
            b'\n' => line += 1,
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                i += 2;
                while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 2;
                continue;
            }
            b'#' => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'[' => {
                depth_list = true;
                i += 1;
                break;
            }
            _ => {}
        }
        i += 1;
    }
    if !depth_list {
        return out;
    }
    let mut start: Option<(usize, usize)> = None;
    let mut depth = 0i32;
    while i < n {
        let c = b[i];
        match c {
            b'\n' => line += 1,
            b'"' => {
                if start.is_none() {
                    start = Some((i, line));
                }
                i += 1;
                while i < n && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    } else if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                i += 2;
                while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'(' | b'[' | b'{' => {
                if start.is_none() {
                    start = Some((i, line));
                }
                depth += 1;
            }
            b')' | b'}' => {
                depth -= 1;
            }
            b']' => {
                if depth == 0 {
                    if let Some((s, l)) = start.take() {
                        out.push((l, src.get(s..i).unwrap_or("")));
                    }
                    break;
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                if let Some((s, l)) = start.take() {
                    out.push((l, src.get(s..i).unwrap_or("")));
                }
            }
            c if !c.is_ascii_whitespace() && start.is_none() => {
                start = Some((i, line));
            }
            _ => {}
        }
        i += 1;
    }
    if let Some((s, l)) = start {
        let tail = src.get(s..).unwrap_or("").trim_end().trim_end_matches(']');
        if !tail.trim().is_empty() {
            out.push((l, tail));
        }
    }
    out
}

impl Icons {
    /// Parse one or more icon files (in priority order). Never fails: problems become `warnings`.
    pub fn parse_files(files: &[(String, String)]) -> Icons {
        // Entries are only scanned here (recipes and shapes stay text until first use), which is a few
        // ms per file — cheaper than spawning threads on a busy machine. Files are added in priority
        // order, so "earlier names win" is deterministic.
        let mut icons = Icons::default();
        for (fname, src) in files {
            let entries = split_entries(src);
            if entries.is_empty() && !src.trim().is_empty() {
                icons.warnings.push(format!("{fname}: no icon list found"));
            }
            for (line, text) in entries {
                match parse_lite(text) {
                    Ok(e) => icons.add(e, fname),
                    Err(err) => icons.warnings.push(format!("{fname}:{line}: skipped malformed icon: {err}")),
                }
            }
        }
        icons
    }

    /// Sound-alike indexes over the final alias set (built once, on first spoken/phonetic lookup).
    pub fn phon(&self) -> &PhonIndex {
        self.phon_cell.get_or_init(|| {
            let mut idx = PhonIndex::default();
            let mut aliases: Vec<(&String, &usize)> = self.alias.iter().collect();
            aliases.sort();
            for (k, &i) in aliases {
                let pk = crate::phonetic::phonetic_key(k);
                if pk.len() >= 4 {
                    for d in crate::fuzzy::deletes1(&pk) {
                        let v = idx.dels.entry(d).or_default();
                        if !v.iter().any(|(j, kk)| *j == i && *kk == pk) {
                            v.push((i, pk.clone()));
                        }
                    }
                }
                if pk.len() >= 2 {
                    let v = idx.exact.entry(pk).or_default();
                    if !v.contains(&i) {
                        v.push(i);
                    }
                }
            }
            idx
        })
    }

    #[allow(dead_code)]
    pub fn parse(src: &str) -> Result<Icons, String> {
        Ok(Self::parse_files(&[("icons.ron".to_string(), src.to_string())]))
    }

    /// Power names (powers.ron) take their names over from earlier icons (a stand beats a character pack
    /// entry of the same name): existing aliases are released first, with a warning.
    pub fn add_power(&mut self, e: IconEntry) {
        let src = if e.source.is_empty() { "powers.ron".to_string() } else { e.source.clone() };
        let e = Lite::from(e);
        for n in &e.names {
            let k = normalize_key(n);
            if let Some(old) = self.alias.remove(&k) {
                let owner = self.defs.get(old).map(|d| format!("{} ({})", d.id, d.file)).unwrap_or_default();
                self.notes.push(format!("{src}: name \"{n}\" taken over from {owner}"));
            }
        }
        self.add(e, "powers.ron");
    }

    fn add(&mut self, e: Lite, file: &str) {
        let id = match &e.model_id {
            Some(id) if !id.is_empty() => id.clone(),
            _ => e.names.first().map(|n| normalize_key(n).replace(' ', "_")).unwrap_or_default(),
        };
        if id.is_empty() || e.names.is_empty() {
            self.warnings.push(format!("{file}: icon without names skipped"));
            return;
        }
        let first = e.names.first().cloned().unwrap_or_default();
        let name = if e.name.trim().is_empty() { first.clone() } else { e.name.clone() };
        let icon_model = e.shape.is_some();
        let recipe = match e.recipe {
            RecipeSrc::Ready(r) => LazyRecipe::ready(r, &id, icon_model, &first),
            RecipeSrc::Text(t) => LazyRecipe::from_src(t, &id, icon_model, &first),
        };
        let idx = self.defs.len();
        let mut names = e.names.clone();
        names.push(name.clone());
        let mut added = 0;
        for n in &names {
            let k = normalize_key(n);
            if k.is_empty() {
                continue;
            }
            let explicit = e.names.contains(n);
            if explicit
                && self.weak.contains(&k)
                && let Some(old) = self.alias.remove(&k)
            {
                self.weak.remove(&k);
                self.pretty.retain(|(_, pk, i)| !(pk == &k && *i == old));
            }
            match self.alias.get(&k) {
                Some(&other) if other != idx => {
                    if explicit {
                        let owner = self.defs.get(other).map(|d| format!("{} ({})", d.id, d.file)).unwrap_or_default();
                        self.warnings.push(format!("{file}: name \"{n}\" of [{id}] already used by {owner}; kept the earlier"));
                    }
                    continue;
                }
                Some(_) => continue,
                None => {}
            }
            self.alias.insert(k.clone(), idx);
            if !explicit {
                self.weak.insert(k.clone());
            }
            added += 1;
            let pretty = n.to_lowercase();
            if self.pretty_seen.insert(pretty.clone()) {
                self.pretty.push((pretty, k.clone(), idx));
            }
            for t in k.split(' ') {
                self.tokens.entry(t.to_string()).or_insert(idx);
            }
        }
        if added == 0 {
            self.warnings.push(format!("{file}: icon [{id}] has no unique names; skipped"));
            return;
        }
        self.defs.push(IconDef {
            id,
            names: e.names,
            name,
            recipe,
            shape: e.shape,
            alt_shape: e.alt_shape,
            source: e.source,
            file: file.to_string(),
        });
    }
}

enum RecipeSrc {
    Ready(Recipe),
    Text(String),
}

/// An icon entry with its recipe and shapes still unparsed.
pub(crate) struct Lite {
    names: Vec<String>,
    /// The recipe's `name`, scanned from the text.
    name: String,
    /// `Icon("<id>")` from the recipe's `model`, scanned from the text.
    model_id: Option<String>,
    recipe: RecipeSrc,
    source: String,
    shape: LazyShape,
    alt_shape: LazyShape,
}

impl From<IconEntry> for Lite {
    fn from(e: IconEntry) -> Lite {
        let model_id = match &e.recipe.model {
            ModelSource::Icon(id) => Some(id.clone()),
            _ => None,
        };
        Lite {
            names: e.names,
            name: e.recipe.name.clone(),
            model_id,
            recipe: RecipeSrc::Ready(e.recipe),
            source: e.source,
            shape: LazyShape::ready(e.shape),
            alt_shape: LazyShape::ready(e.alt_shape),
        }
    }
}

fn first_string(v: &str) -> Option<String> {
    crate::powers::strings(v).into_iter().next()
}

/// Scan one entry without deserialising anything: its top-level fields are split textually; names,
/// source, the recipe's `name`/`model` are read as string literals; recipe and shapes keep their text
/// for lazy parsing. Structural problems (no names, no recipe) skip the entry with a warning.
fn parse_lite(text: &str) -> Result<Lite, String> {
    let t = text.trim();
    let body = match t.find('(') {
        Some(p) if t[..p].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => &t[p..],
        _ => return Err("expected IconEntry(...)".into()),
    };
    let f = crate::powers::fields(body);
    let get = |k: &str| f.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let names: Vec<String> = get("names").map(crate::powers::strings).unwrap_or_default();
    if names.is_empty() {
        return Err("missing or empty `names`".into());
    }
    let recipe_text = get("recipe").map(str::trim).filter(|r| r.starts_with('(')).ok_or("missing `recipe: (...)`")?;
    let rf = crate::powers::fields(recipe_text);
    let rget = |k: &str| rf.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    if rget("form").is_none() {
        return Err("recipe has no `form`".into());
    }
    let name = rget("name").and_then(first_string).unwrap_or_default();
    let model_id = rget("model").filter(|m| m.trim_start().starts_with("Icon")).and_then(first_string);
    Ok(Lite {
        names,
        name,
        model_id,
        recipe: RecipeSrc::Text(recipe_text.to_string()),
        source: get("source").and_then(first_string).unwrap_or_default(),
        shape: LazyShape::from_src(get("shape").map(String::from)),
        alt_shape: LazyShape::from_src(get("alt_shape").map(String::from)),
    })
}

/// Mechanics a form implies by itself (not interesting as a graft).
fn intrinsic(f: bd_core::mechanics::Form) -> &'static [Mechanic] {
    use Mechanic::*;
    use bd_core::mechanics::Form::*;
    match f {
        Hammer | Staff | Gauntlet => &[Blunt],
        Sword | Axe => &[Blade],
        Spear => &[Pierce],
        Shield => &[Guard],
        Gun | Bow | Cannon => &[Shoots],
        Whip => &[Pull],
        Wings | Cape => &[Glide],
        Parachute => &[SlowFall],
        Jetpack => &[Thrust],
        Bird => &[Flies],
        Wall | Pillar | Dome => &[Barrier],
        Cage => &[Trap],
        Trampoline => &[Bounce],
        Ramp => &[Launch],
        Bomb => &[Explodes],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"[
        IconEntry(
            names: ["test hammer", "tester"],
            recipe: (name: "Test Hammer", form: Hammer, mechanics: [Blunt, Returning], element: Electric,
                     scale: 0.9, mass: 40.0, count: 1, accent: Some((0.7, 0.7, 0.8)), model: Icon("test_hammer")),
            shape: Some((prims: [(kind: Box, center: (0.0, 0.0, -4.0), size: (3.2, 2.2, 2.2), tint: Primary)])),
            source: "test",
        ),
        IconEntry( names: ["broken"], source: "bad" ),
        IconEntry( names: ["lazy broken"], recipe: (name: "Lazy", form: NotAForm), source: "bad" ),
        IconEntry(
            names: ["tester", "second"],   // "tester" duplicates the first icon
            recipe: (name: "Second", form: Ball, mechanics: [], element: Light, scale: 0.4, mass: 1.0, count: 1,
                     accent: None, model: Procedural(seed: 3)),
            shape: None,
            source: "test",
        ),
    ]"#;

    #[test]
    fn tolerant_multi_entry_parse() {
        let icons = Icons::parse_files(&[("a.ron".into(), GOOD.into())]);
        assert_eq!(icons.defs.len(), 3, "{:?}", icons.warnings);
        // a recipe that fails to deserialise is found lazily: named blob + reported by validation
        let lazy = icons.defs.iter().find(|d| d.id == "lazy_broken").map(|d| (d.recipe().form, d.recipe_error()));
        assert!(matches!(lazy, Some((bd_core::mechanics::Form::Blob0, Some(_)))));
        assert!(icons.warnings.iter().any(|w| w.contains("malformed")));
        assert!(icons.warnings.iter().any(|w| w.contains("already used")));
        assert_eq!(icons.alias.get("tester"), Some(&0));
        assert_eq!(icons.defs[0].graft(), &[Mechanic::Returning]);
        assert_eq!(icons.defs[0].recipe().model, ModelSource::Icon("test_hammer".into()));
        // garbage never panics
        let junk = Icons::parse_files(&[("j.ron".into(), "[[[(((\"".into()), ("k.ron".into(), "💥".into())]);
        assert!(junk.defs.is_empty());
    }
}
