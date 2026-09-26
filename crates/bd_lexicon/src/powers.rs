//! Player powers vocabulary (`assets/powers.ron`, COMBAT.md §5h): devil fruits, JoJo stands, ...
//!
//! The lexicon only maps *names* to power ids — the power template itself (body, graft, delivery,
//! moves, companion) belongs to bd_sim. powers.ron is authored by another engineer and may grow new
//! fields, so it is read with a tolerant field scanner rather than a fixed struct. Per top-level entry
//! it reads (at the entry's own nesting level only):
//!   `id: "gomu_gomu"`                      (required)
//!   `names: ["gomu gomu no mi", "gum-gum fruit", "rubber fruit"]`
//!   `incantation: Some("gomu gomu no")` | `incantation: "gomu gomu no"` | `incantations: [..]`
//!   `kind: DevilFruit | Stand | ...` (also `family:` / `category:` / `type:`), `companion: Some(..)`
//! Everything else is ignored. Kind is inferred when absent (a companion or "stand" → Stand; names
//! with "fruit"/"no mi" or an incantation → Fruit).

use bd_core::mechanics::{Element, Form, Mechanic, ModelSource, Recipe, Temperament};

use crate::icons::IconEntry;
use crate::norm::{normalize_key, squeeze2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerKind {
    Fruit,
    Stand,
    Other,
}

#[derive(Clone, Debug)]
pub struct PowerDef {
    pub id: String,
    pub names: Vec<String>,
    /// Normalised incantation prefixes ("gomu gomu no").
    pub incantations: Vec<String>,
    pub kind: PowerKind,
}

/// Sentinel `grants` ids for the "random" items; the game picks the actual power (seeded, host-side).
pub const RANDOM_FRUIT: &str = "random:devil_fruit";
pub const RANDOM_STAND: &str = "random:stand";

/// Stand-rush cries: each typed cry is one punch of the user's stand.
const CRIES: &[&str] = &["ora", "muda", "dora", "ari", "wry", "wryy", "oraora", "mudamuda"];

/// Split a token into rush cries ("oraoraora" -> 3, "ORAAAA" -> 1, "wryyyy" -> 1); None if it isn't one.
pub fn rush_count(tok: &str) -> Option<u16> {
    let t = squeeze2(tok);
    if t.is_empty() {
        return None;
    }
    // greedy segmentation into cry syllables
    let mut n = 0u16;
    let mut rest = t.as_str();
    let mut last = "";
    'outer: while !rest.is_empty() {
        // "DORARARA": trailing "ra" syllables extend the previous ora/dora cry
        if (last == "ora" || last == "dora") && rest.starts_with("ra") {
            rest = &rest[2..];
            continue;
        }
        for c in ["muda", "dora", "ora", "ari", "wry"] {
            if let Some(r) = rest.strip_prefix(c) {
                n = n.saturating_add(1);
                rest = r;
                last = c;
                continue 'outer;
            }
        }
        return None;
    }
    (n > 0 && CRIES.iter().any(|c| t.starts_with(c))).then_some(n)
}

// ------------------------------------------------------------------------------------------ scanning
/// Top-level fields of one RON struct entry: (name, raw value text).
pub(crate) fn fields(entry: &str) -> Vec<(String, String)> {
    let b = entry.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut i = 0;
    let mut val_start: Option<(String, usize)> = None;
    let mut ident_start: Option<usize> = None;
    while i < n {
        let c = b[i];
        match c {
            b'"' => {
                i += 1;
                while i < n && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                ident_start = None;
            }
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                ident_start = None;
            }
            b')' | b']' | b'}' => {
                if depth == 1
                    && let Some((k, s)) = val_start.take()
                {
                    out.push((k, entry.get(s..i).unwrap_or("").to_string()));
                }
                depth -= 1;
                ident_start = None;
            }
            b',' if depth == 1 => {
                if let Some((k, s)) = val_start.take() {
                    out.push((k, entry.get(s..i).unwrap_or("").to_string()));
                }
                ident_start = None;
            }
            b':' if depth == 1 && val_start.is_none() => {
                if let Some(s) = ident_start.take() {
                    let key = entry.get(s..i).unwrap_or("").trim().to_string();
                    val_start = Some((key, i + 1));
                }
            }
            c if depth == 1 && val_start.is_none() && (c.is_ascii_alphanumeric() || c == b'_') => {
                if ident_start.is_none() {
                    ident_start = Some(i);
                }
            }
            c if depth == 1 && val_start.is_none() && !c.is_ascii_whitespace() => ident_start = None,
            _ => {}
        }
        i += 1;
    }
    out
}

/// All string literals in a value ("Some(\"x\")", "[\"a\", \"b\"]", "\"x\"").
pub(crate) fn strings(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = v.char_indices().peekable();
    while let Some((_, c)) = it.next() {
        if c == '"' {
            let mut s = String::new();
            while let Some((_, d)) = it.next() {
                match d {
                    '\\' => {
                        if let Some((_, e)) = it.next() {
                            s.push(e);
                        }
                    }
                    '"' => break,
                    _ => s.push(d),
                }
            }
            out.push(s);
        }
    }
    out
}

/// Parse powers.ron text. Never fails: returns (powers, warnings).
pub fn parse_powers(src: &str) -> (Vec<PowerDef>, Vec<String>) {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for (line, entry) in crate::icons::split_entries(src) {
        let entry = entry.trim();
        // strip an optional struct name: `Power(` -> `(`
        let body = match entry.find('(') {
            Some(p) if entry[..p].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => &entry[p..],
            _ => entry,
        };
        let f = fields(body);
        let get = |k: &str| f.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        let Some(id) = get("id").and_then(|v| strings(v).into_iter().next()).filter(|s| !s.is_empty()) else {
            warnings.push(format!("powers.ron:{line}: entry without id: skipped"));
            continue;
        };
        let mut names: Vec<String> = get("names").map(strings).unwrap_or_default();
        if let Some(n) = get("name").and_then(|v| strings(v).into_iter().next()) {
            names.insert(0, n);
        }
        let mut incantations: Vec<String> = get("incantation")
            .into_iter()
            .chain(get("incantations"))
            .flat_map(strings)
            .map(|s| normalize_key(&s))
            .filter(|s| !s.is_empty())
            .collect();
        let kind_text = ["kind", "family", "category", "type"].iter().filter_map(|k| get(k)).collect::<Vec<_>>().join(" ").to_lowercase();
        let has_companion = get("companion").map(|v| !v.trim().starts_with("None")).unwrap_or(false);
        let lower_names: Vec<String> = names.iter().map(|n| normalize_key(n)).collect();
        let fruity = lower_names.iter().any(|n| n.ends_with(" fruit") || n.ends_with(" no mi"));
        let kind = if kind_text.contains("stand") || has_companion {
            PowerKind::Stand
        } else if kind_text.contains("fruit") || fruity || !incantations.is_empty() {
            PowerKind::Fruit
        } else {
            PowerKind::Other
        };
        // derived names/prefixes: "gomu gomu no mi" <-> incantation "gomu gomu no"
        for n in &lower_names {
            if let Some(p) = n.strip_suffix(" mi")
                && p.ends_with(" no")
                && !incantations.contains(&p.to_string())
            {
                incantations.push(p.to_string());
            }
        }
        for inc in incantations.clone() {
            let with_mi = format!("{inc} mi");
            // only invent "<prefix> mi" for fruits with no explicit names (Nika shares Luffy's prefix)
            if kind == PowerKind::Fruit && lower_names.is_empty() && inc.ends_with(" no") {
                names.push(with_mi);
            }
            // "gomu gomu no pistol" is also typed as "gomu gomu pistol"
            if let Some(short) = inc.strip_suffix(" no")
                && !short.is_empty()
                && !incantations.contains(&short.to_string())
            {
                incantations.push(short.to_string());
            }
        }
        if names.is_empty() {
            names.push(id.replace('_', " "));
        }
        out.push(PowerDef { id, names, incantations, kind });
    }
    (out, warnings)
}

/// The construct a power's name summons: an edible fruit, a stand, or a small token orb.
pub fn power_entry(p: &PowerDef) -> IconEntry {
    let display =
        p.names.iter().find(|n| n.to_lowercase().contains("fruit")).or_else(|| p.names.first()).cloned().unwrap_or_else(|| p.id.clone());
    let display = title_case(&display);
    IconEntry {
        names: p.names.iter().map(|n| n.to_lowercase()).collect(),
        recipe: grant_recipe(&display, &p.id, p.kind),
        shape: None,
        alt_shape: None,
        source: "powers.ron".to_string(),
    }
}

pub fn grant_recipe(name: &str, id: &str, kind: PowerKind) -> Recipe {
    let mut r = Recipe::unknown(name);
    r.grants = Some(id.to_string());
    r.model = ModelSource::Procedural { seed: crate::data::fnv64(id.as_bytes()) as u32 };
    match kind {
        PowerKind::Fruit => {
            r.form = Form::Ball;
            r.scale = 0.35;
            r.mass = 0.5;
        }
        PowerKind::Stand => {
            r.form = Form::Biped;
            r.mechanics = vec![Mechanic::Flies, Mechanic::Blunt, Mechanic::Fast];
            r.scale = 2.0;
            r.mass = 80.0;
            r.temperament = Temperament::Protective;
            r.intensity = 0.6;
            r.element = Element::Psychic;
        }
        PowerKind::Other => {
            r.form = Form::Ball;
            r.scale = 0.4;
            r.mass = 0.5;
        }
    }
    r.normalized()
}

/// Built-in "random power" items (names are vocabulary; which power you get is the game's roll).
pub fn random_entries() -> Vec<IconEntry> {
    let fruit = IconEntry {
        names: ["devil fruit", "a devil fruit", "random devil fruit", "mystery fruit", "akuma no mi"].map(String::from).to_vec(),
        recipe: grant_recipe("Devil Fruit", RANDOM_FRUIT, PowerKind::Fruit),
        shape: None,
        alt_shape: None,
        source: "powers.ron".to_string(),
    };
    let mut arrow_recipe = grant_recipe("Stand Arrow", RANDOM_STAND, PowerKind::Other);
    arrow_recipe.form = Form::Spear;
    arrow_recipe.mechanics = vec![Mechanic::Pierce];
    arrow_recipe.scale = 0.6;
    let arrow = IconEntry {
        names: ["stand arrow", "the stand arrow", "arrow of the stand", "requiem arrow", "random stand"].map(String::from).to_vec(),
        recipe: arrow_recipe.normalized(),
        shape: None,
        alt_shape: None,
        source: "powers.ron".to_string(),
    };
    vec![fruit, arrow]
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const SAMPLE: &str = r#"
    // future schema may have many more fields; the lexicon only reads names
    [
        Power(
            id: "gomu_gomu",
            names: ["gomu gomu no mi", "gum-gum fruit", "rubber fruit"],
            incantation: Some("gomu gomu no"),
            body: (traits: [BounceBody], immune: [Electric]),
            moves: [(id: "pistol", recipe: (name: "Pistol", form: Gauntlet))],
        ),
        (
            id: "star_platinum",
            names: ["star platinum", "star plat"],
            kind: Stand,
            companion: Some((name: "Star Platinum", form: Biped)),
        ),
        (names: ["no id here"]),
    ]"#;

    #[test]
    fn tolerant_power_scan() {
        let (p, w) = parse_powers(SAMPLE);
        assert_eq!(p.len(), 2, "{w:?}");
        assert_eq!(w.len(), 1);
        assert_eq!(p[0].id, "gomu_gomu");
        assert_eq!(p[0].kind, PowerKind::Fruit);
        assert!(p[0].incantations.contains(&"gomu gomu no".to_string()));
        assert!(p[0].incantations.contains(&"gomu gomu".to_string()));
        assert_eq!(p[1].kind, PowerKind::Stand);
        assert_eq!(power_entry(&p[0]).recipe.grants.as_deref(), Some("gomu_gomu"));
        assert!(parse_powers("💥[[((").0.is_empty());
    }

    #[test]
    fn rush_cries() {
        assert_eq!(rush_count("ora"), Some(1));
        assert_eq!(rush_count("oraoraora"), Some(3));
        assert_eq!(rush_count("oraaaaa"), Some(1));
        assert_eq!(rush_count("mudamudamuda"), Some(3));
        assert_eq!(rush_count("wryyyy"), Some(1));
        assert_eq!(rush_count("orange"), None);
        assert_eq!(rush_count("hammer"), None);
    }
}
