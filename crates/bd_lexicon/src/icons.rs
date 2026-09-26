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
    #[serde(default)]
    pub source: String,
}

/// A loaded icon.
#[derive(Clone, Debug)]
pub struct IconDef {
    /// Canonical id (first name, or the recipe's `Icon(id)`).
    pub id: String,
    pub names: Vec<String>,
    pub recipe: Recipe,
    pub shape: Option<ShapeSpec>,
    pub source: String,
    /// Mechanics it contributes as a modifier/graft ("mjolnir hammer", aiming + "mjolnir").
    pub graft: Vec<Mechanic>,
    /// File it came from (for diagnostics).
    pub file: String,
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
    /// phonetic key of alias -> icon indices
    pub phon: HashMap<String, Vec<usize>>,
    /// single-symbol deletions of alias keys -> (icon index, full key): symmetric-delete sound matching
    pub phon_del: HashMap<String, Vec<(usize, String)>>,
    pub warnings: Vec<String>,
    pretty_seen: std::collections::HashSet<String>,
}

/// Split the top-level RON list into entry substrings (with their 1-based start line), honouring strings
/// and comments. Tolerates a missing `[`/`]` and trailing commas.
fn split_entries(src: &str) -> Vec<(usize, &str)> {
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
        // RON parsing is the expensive part: parse files on parallel threads, then add them in
        // priority order (so "earlier names win" stays deterministic).
        type Parsed = Vec<Result<IconEntry, String>>;
        let parse_one = |(fname, src): &(String, String)| -> Parsed {
            let entries = split_entries(src);
            if entries.is_empty() && !src.trim().is_empty() {
                return vec![Err(format!("{fname}: no icon list found"))];
            }
            entries
                .into_iter()
                .map(|(line, text)| {
                    ron::from_str::<IconEntry>(text)
                        .map_err(|err| format!("{fname}:{line}: skipped malformed icon: {err}"))
                })
                .collect()
        };
        let parsed: Vec<Parsed> = std::thread::scope(|scope| {
            let handles: Vec<_> = files
                .iter()
                .map(|f| scope.spawn(move || parse_one(f)))
                .collect();
            handles
                .into_iter()
                .zip(files)
                .map(|(h, f)| {
                    h.join()
                        .unwrap_or_else(|_| vec![Err(format!("{}: parser thread failed", f.0))])
                })
                .collect()
        });
        let mut icons = Icons::default();
        for ((fname, _), entries) in files.iter().zip(parsed) {
            for e in entries {
                match e {
                    Ok(e) => icons.add(e, fname),
                    Err(w) => icons.warnings.push(w),
                }
            }
        }
        icons
    }

    #[allow(dead_code)]
    pub fn parse(src: &str) -> Result<Icons, String> {
        Ok(Self::parse_files(&[(
            "icons.ron".to_string(),
            src.to_string(),
        )]))
    }

    fn add(&mut self, e: IconEntry, file: &str) {
        let id = match &e.recipe.model {
            ModelSource::Icon(id) if !id.is_empty() => id.clone(),
            _ => e
                .names
                .first()
                .map(|n| normalize_key(n).replace(' ', "_"))
                .unwrap_or_default(),
        };
        if id.is_empty() || e.names.is_empty() {
            self.warnings
                .push(format!("{file}: icon without names skipped"));
            return;
        }
        let mut recipe = e.recipe.clone();
        if e.shape.is_some() {
            recipe.model = ModelSource::Icon(id.clone());
        }
        if recipe.name.trim().is_empty() {
            recipe.name = e.names.first().cloned().unwrap_or_default();
        }
        let recipe = recipe.normalized();
        let intrinsic = intrinsic(recipe.form);
        let mut graft: Vec<Mechanic> = recipe
            .mechanics
            .iter()
            .copied()
            .filter(|m| !intrinsic.contains(m))
            .collect();
        if graft.is_empty() {
            graft = recipe.mechanics.clone();
        }
        let idx = self.defs.len();
        let mut names = e.names.clone();
        names.push(recipe.name.clone());
        let mut added = 0;
        for n in &names {
            let k = normalize_key(n);
            if k.is_empty() {
                continue;
            }
            match self.alias.get(&k) {
                Some(&other) if other != idx => {
                    if e.names.contains(n) {
                        let owner = self
                            .defs
                            .get(other)
                            .map(|d| format!("{} ({})", d.id, d.file))
                            .unwrap_or_default();
                        self.warnings.push(format!("{file}: name \"{n}\" of [{id}] already used by {owner}; kept the earlier"));
                    }
                    continue;
                }
                Some(_) => continue,
                None => {}
            }
            self.alias.insert(k.clone(), idx);
            added += 1;
            let pretty = n.to_lowercase();
            if self.pretty_seen.insert(pretty.clone()) {
                self.pretty.push((pretty, k.clone(), idx));
            }
            for t in k.split(' ') {
                self.tokens.entry(t.to_string()).or_insert(idx);
            }
            let pk = crate::phonetic::phonetic_key(&k);
            if pk.len() >= 4 {
                for d in crate::fuzzy::deletes1(&pk) {
                    let v = self.phon_del.entry(d).or_default();
                    if !v.iter().any(|(i, k)| *i == idx && *k == pk) {
                        v.push((idx, pk.clone()));
                    }
                }
            }
            if pk.len() >= 2 {
                let v = self.phon.entry(pk).or_default();
                if !v.contains(&idx) {
                    v.push(idx);
                }
            }
        }
        if added == 0 {
            self.warnings
                .push(format!("{file}: icon [{id}] has no unique names; skipped"));
            return;
        }
        self.defs.push(IconDef {
            id,
            names: e.names,
            recipe,
            shape: e.shape,
            source: e.source,
            graft,
            file: file.to_string(),
        });
    }
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
        IconEntry( names: ["broken"], recipe: (name: "x", form: NotAForm), source: "bad" ),
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
        assert_eq!(icons.defs.len(), 2, "{:?}", icons.warnings);
        assert!(icons.warnings.iter().any(|w| w.contains("malformed")));
        assert!(icons.warnings.iter().any(|w| w.contains("already used")));
        assert_eq!(icons.alias.get("tester"), Some(&0));
        assert_eq!(icons.defs[0].graft, vec![Mechanic::Returning]);
        assert_eq!(
            icons.defs[0].recipe.model,
            ModelSource::Icon("test_hammer".into())
        );
        // garbage never panics
        let junk = Icons::parse_files(&[
            ("j.ron".into(), "[[[(((\"".into()),
            ("k.ron".into(), "💥".into()),
        ]);
        assert!(junk.defs.is_empty());
    }
}
