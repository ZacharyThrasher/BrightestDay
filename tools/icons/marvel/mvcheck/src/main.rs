//! mvcheck — validate the Marvel pending powers + probe the lexicon.
//!
//!     cargo run --release --manifest-path tools/icons/marvel/mvcheck/Cargo.toml -- [phrase ...]
//!
//! Exit 1 on any schema/never-list problem. Run from the game/ directory.

use std::collections::HashMap;

use bd_sim::powers::{MoveOp, PowerTable};

const FLIGHT_CAP: f32 = 17.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pending = std::fs::read_to_string("assets/powers_pending/marvel.ron").expect("pending file");
    let base = std::fs::read_to_string("assets/powers.ron").expect("powers.ron");
    let mut bad = 0;

    let t = match PowerTable::parse(&pending) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("PARSE pending: {e}");
            std::process::exit(1);
        }
    };
    println!("pending: {} powers", t.powers.len());
    for p in &t.powers {
        let mut flights = vec![p.body.moves.thrust_max];
        let mut has_tf_op = false;
        if p.moves.len() > 2 {
            eprintln!("[{}] more than 2 moves", p.id);
            bad += 1;
        }
        for m in &p.moves {
            for op in &m.ops {
                match op {
                    MoveOp::TimeStop { .. } => {
                        eprintln!("[{}] TimeStop (never-list)", p.id);
                        bad += 1;
                    }
                    MoveOp::Transform => has_tf_op = true,
                    _ => {}
                }
            }
        }
        if has_tf_op != p.transform.is_some() {
            eprintln!("[{}] Transform op / transform def mismatch", p.id);
            bad += 1;
        }
        if let Some(tf) = &p.transform {
            flights.push(tf.body.moves.thrust_max);
            if p.body.moves.thrust > 0.0 && p.body.moves.thrust_max <= 0.0 {
                flights.push(30.0);
            }
            if tf.body.moves.thrust > 0.0 && tf.body.moves.thrust_max <= 0.0 {
                eprintln!("[{}] transform thrust without thrust_max", p.id);
                bad += 1;
            }
        }
        if p.body.moves.thrust > 0.0 {
            eprintln!("[{}] note: always-on body flight", p.id);
        }
        for f in flights {
            if f > FLIGHT_CAP {
                eprintln!("[{}] flight cap {f} > {FLIGHT_CAP}", p.id);
                bad += 1;
            }
        }
        for (s, slot) in &p.shouts {
            if *slot as usize >= p.moves.len() {
                eprintln!("[{}] shout {s:?} -> missing slot {slot}", p.id);
                bad += 1;
            }
        }
    }

    // Merge exactly like the lead will: append pending entries to the base list.
    let b = base.trim_end().trim_end_matches(']');
    let body = pending.find('[').map(|i| &pending[i + 1..]).unwrap_or("");
    let merged = format!("{b}\n{body}");
    let mt = match PowerTable::parse(&merged) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("PARSE merged: {e}");
            std::process::exit(1);
        }
    };
    println!("merged: {} powers", mt.powers.len());
    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut names: HashMap<&str, &str> = HashMap::new();
    for p in &mt.powers {
        *ids.entry(p.id.as_str()).or_default() += 1;
        for n in &p.names {
            if let Some(o) = names.insert(n.as_str(), p.id.as_str()).filter(|o| *o != p.id.as_str()) {
                eprintln!("dup power name {n:?}: {o} / {}", p.id);
                bad += 1;
            }
        }
    }
    for (id, n) in ids {
        if n > 1 {
            eprintln!("dup power id {id}");
            bad += 1;
        }
    }

    // Lexicon probe with the merged table.
    let bytes = std::fs::read("assets/lexicon.bin").expect("lexicon.bin");
    let files = bd_lexicon::Lexicon::icon_files(std::path::Path::new("assets"));
    let lex = bd_lexicon::Lexicon::from_parts(bytes, &files, Some(&merged)).expect("lexicon");
    for w in lex.load_warnings() {
        if w.contains("marvel") {
            eprintln!("warning: {w}");
            bad += 1;
        }
    }
    let pending_ids: Vec<&str> = t.powers.iter().map(|p| p.id.as_str()).collect();
    for n in lex.load_notes() {
        if n.contains("marvel") || n.contains("taken over") || pending_ids.iter().any(|id| n.contains(&format!("[{id}]")) || n.contains(&format!("{id} ("))) {
            println!("note: {n}");
        }
    }
    let lex_ids = lex.power_ids();
    for id in &pending_ids {
        if !lex_ids.contains(id) {
            eprintln!("lexicon did not load power {id}");
            bad += 1;
        }
    }
    // names_gen helper: which single words does the lexicon already know (so a names-file entry
    // should be qualified, e.g. "marvel mantis")? MV_KNOWN_IN = one word per line.
    if let (Ok(inp), Ok(outp)) = (std::env::var("MV_KNOWN_IN"), std::env::var("MV_KNOWN_OUT")) {
        let words = std::fs::read_to_string(inp).unwrap_or_default();
        let mut o = String::new();
        for w in words.lines().map(str::trim).filter(|w| !w.is_empty()) {
            let r = lex.resolve(w);
            let known = !bd_lexicon::Lexicon::is_fallback(&r);
            o.push_str(&format!("{w}\t{}\t{}\n", known as u8, r.recipe.name));
        }
        std::fs::write(outp, o).expect("write known");
    }
    for ph in &args {
        let r = lex.resolve(ph);
        println!(
            "{ph:40} -> {:?} {:<24} form {:?} grants {:?} inc {:?} conf {:.2}",
            r.kind,
            r.recipe.name,
            r.recipe.form,
            r.recipe.grants,
            r.incantation,
            r.confidence
        );
    }
    if bad > 0 {
        eprintln!("{bad} problems");
        std::process::exit(1);
    }
    println!("OK");
}
