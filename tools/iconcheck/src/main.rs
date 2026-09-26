//! Validates icon RON files against bd_core v1: `cargo run --manifest-path tools/iconcheck/Cargo.toml -- <file.ron>...`
use serde::Deserialize;

#[derive(Deserialize)]
#[allow(dead_code)]
struct IconEntry {
    names: Vec<String>,
    recipe: bd_core::mechanics::Recipe,
    shape: Option<bd_core::shape::ShapeSpec>,
    source: String,
}

fn main() {
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => { eprintln!("{path}: {e}"); failed = true; continue; }
        };
        match ron::from_str::<Vec<IconEntry>>(&text) {
            Err(e) => { eprintln!("{path}: PARSE ERROR {e}"); failed = true; }
            Ok(entries) => {
                let mut warn = 0;
                for e in &entries {
                    let id = e.names.first().cloned().unwrap_or_default();
                    if e.names.is_empty() || e.names.iter().any(|n| n.trim().is_empty() || *n != n.to_lowercase()) {
                        eprintln!("{path}: [{id}] names must be non-empty lowercase"); warn += 1;
                    }
                    if let Some(s) = &e.shape {
                        if s.prims.is_empty() || s.prims.len() > 120 { eprintln!("{path}: [{id}] prim count {}", s.prims.len()); warn += 1; }
                        if s.prims.iter().any(|p| !(p.center.is_finite() && p.size.is_finite() && p.rot_deg.is_finite()) || p.size.min_element() <= 0.0) {
                            eprintln!("{path}: [{id}] non-finite or non-positive prim"); warn += 1;
                        }
                    }
                    if !(e.recipe.scale.is_finite() && e.recipe.scale > 0.0) { eprintln!("{path}: [{id}] bad scale"); warn += 1; }
                }
                println!("{path}: {} entries, {} with shapes, {warn} warnings", entries.len(), entries.iter().filter(|e| e.shape.is_some()).count());
                if warn > 0 { failed = true; }
            }
        }
    }
    std::process::exit(if failed { 1 } else { 0 });
}
