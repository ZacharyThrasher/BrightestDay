//! **Franchise themes**: which powers wake which theme (`assets/music/themes/franchises.ron`).
//!
//! Every theme in `assets/music/themes/` is an ORIGINAL chip piece written for Brightest Day that
//! evokes a franchise's genre and energy (tempo, mode, instrumentation, groove) — never its actual
//! music. When the local player uses a power / kit / costume of a franchise, the game plays that
//! franchise's theme on the player's theme layer ([`crate::MusicCommand::Theme`]).
//!
//! The mapping is data, resolved per power in this order (the first match wins):
//! 1. the power's own `music` key in `assets/powers/*.ron` (`music: Some("pirate")` — a franchise
//!    name or a theme track name; `"none"` opts a power out);
//! 2. a franchise's `ids` (exact power ids, or globs with a leading / trailing `*`);
//! 3. a franchise's `files` (every power of `assets/powers/<file>.ron`).
//!
//! ```ron
//! [
//!     (name: "pirate", title: "Pirate adventure", theme: "open_sea_jig",
//!      files: ["base_one_piece"], ids: ["kit_luffy", "kit_zoro"]),
//! ]
//! ```

use serde::Deserialize;

use crate::song::TrackId;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FranchiseDef {
    name: String,
    #[serde(default)]
    title: String,
    theme: String,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    ids: Vec<String>,
}

/// One franchise → theme entry (compiled).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Franchise {
    /// Key (lower case), e.g. `"pirate"`.
    pub name: String,
    pub title: String,
    /// The theme track (a `themes/` song).
    pub theme: TrackId,
    pub theme_name: String,
    /// Power file stems (`base_one_piece`), lower case, no extension.
    pub files: Vec<String>,
    /// Power ids or `*` globs, lower case.
    pub ids: Vec<String>,
}

fn stem(file: &str) -> String {
    let f = file.trim().to_ascii_lowercase();
    let f = f.rsplit(['/', '\\']).next().unwrap_or(&f).to_string();
    f.strip_suffix(".ron").map(str::to_string).unwrap_or(f)
}

fn glob(pat: &str, id: &str) -> bool {
    match (pat.strip_prefix('*'), pat.strip_suffix('*')) {
        (Some(rest), _) if rest.ends_with('*') => id.contains(rest.trim_end_matches('*')),
        (Some(suffix), _) => id.ends_with(suffix),
        (None, Some(prefix)) => id.starts_with(prefix),
        (None, None) => id == pat,
    }
}

/// Parse and validate `franchises.ron` (`""` = no mapping). `theme_id` resolves a theme track.
pub(crate) fn compile(src: &str, theme_id: &dyn Fn(&str) -> Option<TrackId>) -> Result<Vec<Franchise>, String> {
    if src.trim().is_empty() {
        return Ok(Vec::new());
    }
    let defs: Vec<FranchiseDef> = ron::from_str(src).map_err(|e| e.to_string())?;
    let mut out: Vec<Franchise> = Vec::new();
    for d in defs {
        let name = d.name.trim().to_ascii_lowercase();
        if name.is_empty() || name == "none" {
            return Err(format!("bad franchise name '{}'", d.name));
        }
        if out.iter().any(|f| f.name == name) {
            return Err(format!("duplicate franchise '{name}'"));
        }
        let theme = theme_id(d.theme.trim()).ok_or_else(|| format!("{name}: no theme track '{}' in themes/", d.theme))?;
        out.push(Franchise {
            name,
            title: d.title,
            theme,
            theme_name: d.theme.trim().to_string(),
            files: d.files.iter().map(|f| stem(f)).filter(|f| !f.is_empty()).collect(),
            ids: d.ids.iter().map(|i| i.trim().to_ascii_lowercase()).filter(|i| !i.is_empty()).collect(),
        });
    }
    // A power id / file may belong to one franchise only (ambiguity is an authoring error).
    for (i, a) in out.iter().enumerate() {
        for b in &out[i + 1..] {
            if let Some(x) = a.ids.iter().find(|x| !x.contains('*') && b.ids.contains(x)) {
                return Err(format!("id '{x}' is in both '{}' and '{}'", a.name, b.name));
            }
            if let Some(x) = a.files.iter().find(|x| b.files.contains(x)) {
                return Err(format!("file '{x}' is in both '{}' and '{}'", a.name, b.name));
            }
        }
    }
    Ok(out)
}

/// The franchise a power belongs to (see the module docs for the order). `power_id` is the
/// power's id, `file` its source file (`base_one_piece.ron` or a stem), `key` its own `music`
/// key. `None` = no theme.
pub fn resolve<'a>(list: &'a [Franchise], power_id: &str, file: Option<&str>, key: Option<&str>) -> Option<&'a Franchise> {
    if let Some(k) = key.map(|k| k.trim().to_ascii_lowercase()) {
        if k.is_empty() || k == "none" {
            return None;
        }
        if let Some(f) = list.iter().find(|f| f.name == k || f.theme_name.eq_ignore_ascii_case(&k)) {
            return Some(f);
        }
        // An unknown key (e.g. an overlay layer name) falls through to the table.
    }
    let id = power_id.trim().to_ascii_lowercase();
    if let Some(f) = list.iter().find(|f| f.ids.iter().any(|p| !p.contains('*') && *p == id)) {
        return Some(f);
    }
    if let Some(f) = list.iter().find(|f| f.ids.iter().any(|p| p.contains('*') && glob(p, &id))) {
        return Some(f);
    }
    let file = file.map(stem)?;
    list.iter().find(|f| f.files.contains(&file))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Vec<Franchise> {
        let src = r#"[
            (name: "Pirate", theme: "sea", files: ["base_one_piece.ron"], ids: ["kit_luffy", "kit_zoro"]),
            (name: "turtles", theme: "surf", ids: ["mutagen_*", "kit_leonardo"]),
            (name: "myth", theme: "gods", files: ["myth_folklore"], ids: ["*_myth", "*odin*"]),
        ]"#;
        let ids = ["sea", "surf", "gods"];
        compile(src, &|n: &str| ids.iter().position(|x| *x == n).map(|i| TrackId(i as u16))).expect("compiles")
    }

    #[test]
    fn ids_beat_files_and_keys_beat_both() {
        let t = table();
        fn name(f: Option<&Franchise>) -> Option<&str> {
            f.map(|f| f.name.as_str())
        }
        assert_eq!(name(resolve(&t, "gomu_gomu", Some("base_one_piece.ron"), None)), Some("pirate"));
        assert_eq!(name(resolve(&t, "kit_luffy", Some("anime_classics.ron"), None)), Some("pirate"));
        assert_eq!(name(resolve(&t, "KIT_LEONARDO", None, None)), Some("turtles"));
        assert_eq!(name(resolve(&t, "mutagen_rat", Some("base_cartoons"), None)), Some("turtles"));
        assert_eq!(name(resolve(&t, "sea_myth", None, None)), Some("myth"), "suffix glob");
        assert_eq!(name(resolve(&t, "kit_odin_allfather", None, None)), Some("myth"), "contains glob");
        assert_eq!(name(resolve(&t, "kit_goku", Some("anime_classics.ron"), None)), None);
        // The power's own key wins; "none" opts out; an unknown key falls through.
        assert_eq!(name(resolve(&t, "gomu_gomu", Some("base_one_piece.ron"), Some("turtles"))), Some("turtles"));
        assert_eq!(name(resolve(&t, "gomu_gomu", Some("base_one_piece.ron"), Some("surf"))), Some("turtles"), "by theme name");
        assert_eq!(name(resolve(&t, "gomu_gomu", Some("base_one_piece.ron"), Some("none"))), None);
        assert_eq!(name(resolve(&t, "gomu_gomu", Some("base_one_piece.ron"), Some("drums"))), Some("pirate"));
    }

    #[test]
    fn bad_tables_fail_loudly() {
        let ids = |n: &str| (n == "sea").then_some(TrackId(0));
        assert!(compile("", &ids).expect("empty").is_empty());
        assert!(compile(r#"[(name: "a", theme: "nope")]"#, &ids).is_err(), "unknown theme");
        assert!(compile(r#"[(name: "a", theme: "sea"), (name: "A", theme: "sea")]"#, &ids).is_err(), "duplicate");
        assert!(compile(r#"[(name: "a", theme: "sea", ids: ["x"]), (name: "b", theme: "sea", ids: ["x"])]"#, &ids).is_err());
        assert!(compile(r#"[(name: "a", theme: "sea", files: ["f"]), (name: "b", theme: "sea", files: ["f.ron"])]"#, &ids).is_err());
        assert!(compile(r#"[(name: "a", theme: "sea", colour: 3)]"#, &ids).is_err(), "unknown field");
    }
}
