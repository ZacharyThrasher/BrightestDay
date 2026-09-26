//! Resolve phrases from the command line (or stdin, one per line) and print what the lexicon decided.
//!
//!     cargo run -p bd_lexicon --example probe -- "giant flaming hammer with rockets" "maul"
//!     cargo run -p bd_lexicon --example probe -- --spoken "picked you"
//!     cargo run -p bd_lexicon --example probe -- --complete "captain am"

use std::io::BufRead;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // BD_LEXICON=<path/to/lexicon.bin> probes another build (icons/powers are read next to it)
    let root = std::env::var_os("BD_LEXICON")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/lexicon.bin"));
    let t = Instant::now();
    let lex = match bd_lexicon::Lexicon::load(&root) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("load {}: {e}", root.display());
            std::process::exit(1);
        }
    };
    eprintln!("loaded {} entries + {} icons in {:.1} ms", lex.len(), lex.icon_count(), t.elapsed().as_secs_f64() * 1e3);
    for w in lex.load_warnings().iter().take(10) {
        eprintln!("  icon warning: {w}");
    }
    let mut spoken = false;
    let mut complete = false;
    let mut phrases: Vec<String> = Vec::new();
    for a in args {
        match a.as_str() {
            "--spoken" => spoken = true,
            "--complete" => complete = true,
            _ => phrases.push(a),
        }
    }
    if phrases.is_empty() {
        phrases = std::io::stdin().lock().lines().map_while(Result::ok).collect();
    }
    for p in phrases {
        if complete {
            let t = Instant::now();
            let c = lex.complete(&p);
            println!("{p:?} -> {c:?}  ({:.0} µs)", t.elapsed().as_secs_f64() * 1e6);
            continue;
        }
        let t = Instant::now();
        let r = if spoken { lex.resolve_spoken(&p) } else { lex.resolve(&p) };
        let us = t.elapsed().as_secs_f64() * 1e6;
        let rc = &r.recipe;
        println!(
            "{p:?}\n  {:?} {} | {:?} el={:?} scale={:.2} mass={:.1} n={} feat={:?} temp={:?}/{:.2} conf={:.2} {:.0}µs{}",
            r.kind,
            r.signature(),
            rc.name,
            rc.element,
            rc.scale,
            rc.mass,
            rc.count,
            rc.features,
            rc.temperament,
            rc.intensity,
            r.confidence,
            us,
            if r.shape.is_some() { " [icon shape]" } else { "" }
        );
        if let Some(pl) = &rc.payload {
            println!("  payload: {} {:?} {:?} el={:?} n={}", pl.name, pl.form, pl.mechanics, pl.element, pl.count);
        }
        println!("  graft: {:?} el={:?} x{:.2} feat={:?}", r.graft.mechanics, r.graft.element, r.graft.scale_mul, r.graft.features);
        if let Some(s) = &r.signature {
            println!("  SIGNATURE {:?} \"{}\" via {:?}", s.kind, s.title, s.invocation);
        }
        for e in &r.explanation {
            println!("    - {e}");
        }
        if !r.ignored.is_empty() {
            println!("    ignored: {:?}", r.ignored);
        }
    }
}
