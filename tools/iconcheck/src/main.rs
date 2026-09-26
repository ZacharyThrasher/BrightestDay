//! Validates icon RON files against bd_core v1.2: `cargo run --manifest-path tools/iconcheck/Cargo.toml -- <file.ron>...`
use bd_core::mechanics::Mechanic;
use bd_core::shape::ShapeSpec;
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[allow(dead_code)]
struct IconEntry {
    names: Vec<String>,
    recipe: bd_core::mechanics::Recipe,
    shape: Option<ShapeSpec>,
    /// Second mode for `Transforms` icons (robot mode); panels share `part` ids with `shape`.
    #[serde(default)]
    alt_shape: Option<ShapeSpec>,
    source: String,
}

/// Returns the number of warnings for one shape.
fn check_shape(path: &str, id: &str, what: &str, s: &ShapeSpec) -> usize {
    let mut warn = 0;
    if s.prims.is_empty() || s.prims.len() > 120 {
        eprintln!("{path}: [{id}] {what} prim count {}", s.prims.len());
        warn += 1;
    }
    if s.prims.iter().any(|p| {
        !(p.center.is_finite() && p.size.is_finite() && p.rot_deg.is_finite())
            || p.size.min_element() <= 0.0
    }) {
        eprintln!("{path}: [{id}] {what}: non-finite or non-positive prim");
        warn += 1;
    }
    warn
}

fn part_ids(s: &ShapeSpec) -> BTreeSet<u8> {
    s.prims.iter().map(|p| p.part).filter(|&p| p != 0).collect()
}

fn main() {
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{path}: {e}");
                failed = true;
                continue;
            }
        };
        match ron::from_str::<Vec<IconEntry>>(&text) {
            Err(e) => {
                eprintln!("{path}: PARSE ERROR {e}");
                failed = true;
            }
            Ok(entries) => {
                let mut warn = 0;
                for e in &entries {
                    let id = e.names.first().cloned().unwrap_or_default();
                    if e.names.is_empty()
                        || e.names
                            .iter()
                            .any(|n| n.trim().is_empty() || *n != n.to_lowercase())
                    {
                        eprintln!("{path}: [{id}] names must be non-empty lowercase");
                        warn += 1;
                    }
                    if let Some(s) = &e.shape {
                        warn += check_shape(&path, &id, "shape", s);
                    }
                    if let Some(a) = &e.alt_shape {
                        warn += check_shape(&path, &id, "alt_shape", a);
                        if !e.recipe.mechanics.contains(&Mechanic::Transforms) {
                            eprintln!("{path}: [{id}] has alt_shape but recipe lacks Transforms");
                            warn += 1;
                        }
                        match &e.shape {
                            None => {
                                eprintln!("{path}: [{id}] alt_shape without shape");
                                warn += 1;
                            }
                            Some(s) => {
                                // panels are matched by part id: the modes should share at least some
                                let (a_ids, s_ids) = (part_ids(a), part_ids(s));
                                if !a_ids.is_empty()
                                    && !s_ids.is_empty()
                                    && a_ids.is_disjoint(&s_ids)
                                {
                                    eprintln!(
                                        "{path}: [{id}] shape and alt_shape share no part ids (panels can't be matched)"
                                    );
                                    warn += 1;
                                }
                            }
                        }
                    }
                    if !(e.recipe.scale.is_finite() && e.recipe.scale > 0.0) {
                        eprintln!("{path}: [{id}] bad scale");
                        warn += 1;
                    }
                }
                println!(
                    "{path}: {} entries, {} with shapes, {} two-mode, {warn} warnings",
                    entries.len(),
                    entries.iter().filter(|e| e.shape.is_some()).count(),
                    entries.iter().filter(|e| e.alt_shape.is_some()).count()
                );
                if warn > 0 {
                    failed = true;
                }
            }
        }
    }
    std::process::exit(if failed { 1 } else { 0 });
}
