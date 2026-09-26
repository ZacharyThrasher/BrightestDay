//! Validates pending power files against bd_sim's real PowerDef schema + the never-list / flight rules:
//! `cargo run --manifest-path tools/icons/dc/powercheck/Cargo.toml -- assets/powers_pending/dc.ron [assets/powers.ron]`
//! The first file is linted; any further files only contribute ids/names for uniqueness checks.
use bd_sim::powers::{MoveOp, PowerDef, PowerTable, norm_words};
use std::collections::HashMap;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: powercheck <pending.ron> [other.ron...]");
        std::process::exit(2);
    };
    let src = std::fs::read_to_string(path).expect("read");
    let raw: Vec<PowerDef> = match ron::from_str(&src) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{path}: PARSE ERROR {e}");
            std::process::exit(1);
        }
    };
    let table = PowerTable::parse(&src).expect("PowerTable::parse");
    let mut bad = 0;
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for other in &args[1..] {
        let o = std::fs::read_to_string(other).expect("read other");
        let v: Vec<PowerDef> = ron::from_str(&o).unwrap_or_else(|e| panic!("{other}: {e}"));
        for p in v {
            ids.insert(p.id.clone(), other.clone());
            for n in &p.names {
                names.insert(norm_words(n), format!("{other}:{}", p.id));
            }
        }
    }
    let mut warn = |msg: String| {
        eprintln!("{path}: {msg}");
        bad += 1;
    };
    for p in &raw {
        if let Some(f) = ids.insert(p.id.clone(), path.clone()) {
            warn(format!("[{}] duplicate id (also in {f})", p.id));
        }
        for n in &p.names {
            if let Some(o) = names.insert(norm_words(n), p.id.clone()) {
                warn(format!("[{}] duplicate name {n:?} (also {o})", p.id));
            }
        }
        if p.moves.len() > 2 {
            warn(format!("[{}] {} moves (max 2)", p.id, p.moves.len()));
        }
        for (s, slot) in &p.shouts {
            if *slot as usize >= p.moves.len() {
                warn(format!("[{}] shout {s:?} -> missing slot {slot}", p.id));
            }
        }
        let mut bodies = vec![("body", &p.body)];
        if let Some(t) = &p.transform {
            bodies.push(("transform.body", &t.body));
            if t.pilot.is_none() && t.costume.is_empty() {
                warn(format!("[{}] transform without costume or pilot", p.id));
            }
        }
        for (what, b) in bodies {
            if b.moves.thrust > 0.0 && !(b.moves.thrust_max > 0.0 && b.moves.thrust_max <= 17.0) {
                warn(format!("[{}] {what}: flight cap {} (must be 0 < cap <= 17 m/s)", p.id, b.moves.thrust_max));
            }
            if b.moves.speed > 1.35 {
                warn(format!("[{}] {what}: speed {} too high (skilled movement must win)", p.id, b.moves.speed));
            }
        }
        for m in &p.moves {
            for op in &m.ops {
                match op {
                    MoveOp::TimeStop { .. } => warn(format!("[{}] {}: TimeStop is never-list for new kits", p.id, m.name)),
                    MoveOp::Blink { dist, .. } if *dist > 25.0 => warn(format!("[{}] {}: blink {dist} > 25", p.id, m.name)),
                    MoveOp::Cast { recipe, .. } if recipe.count > 12 => warn(format!("[{}] {}: count {}", p.id, m.name, recipe.count)),
                    _ => {}
                }
            }
        }
    }
    println!(
        "{path}: {} powers parsed by bd_sim ({} kept after sanitize), {} with transform, {} with companion, {bad} problems",
        raw.len(),
        table.powers.len(),
        raw.iter().filter(|p| p.transform.is_some()).count(),
        raw.iter().filter(|p| p.companion.is_some()).count()
    );
    for p in &raw {
        println!("  {:22} {:?} names={} shouts={} moves={}", p.id, p.kind, p.names.len(), p.shouts.len(), p.moves.len());
    }
    std::process::exit(if bad > 0 { 1 } else { 0 });
}
