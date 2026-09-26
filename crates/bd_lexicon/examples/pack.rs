//! Pack the offline pipeline's JSON into the runtime binary.
//!
//!     cargo run --release -p bd_lexicon --features builder --example pack -- \
//!         tools/lexicon_build/cache/lexicon.json assets/lexicon.bin
//!
//! Normally invoked by `python tools/lexicon_build/build.py`. After packing it reloads the file and
//! prints load time + a few sample resolutions as a smoke test.

use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (src, dst) = match (args.get(1), args.get(2)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => {
            eprintln!("usage: pack <lexicon.json> <lexicon.bin>");
            std::process::exit(2);
        }
    };
    let t = Instant::now();
    let json = match std::fs::read_to_string(&src) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {src}: {e}");
            std::process::exit(1);
        }
    };
    let (bytes, summary) = match bd_lexicon::builder::pack(&json) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("pack failed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = std::fs::write(&dst, &bytes) {
        eprintln!("write {dst}: {e}");
        std::process::exit(1);
    }
    println!(
        "packed {} -> {} ({:.1} MB) in {:.1}s\n{summary}",
        src,
        dst,
        bytes.len() as f64 / 1e6,
        t.elapsed().as_secs_f64()
    );
    let t = Instant::now();
    match bd_lexicon::Lexicon::load(&dst) {
        Ok(lex) => {
            println!(
                "reload: {} entries, {} icons in {:.1} ms",
                lex.len(),
                lex.icon_count(),
                t.elapsed().as_secs_f64() * 1e3
            );
            for p in [
                "maul",
                "parachute",
                "three flaming swords",
                "giant eeyore shootin laser beams",
            ] {
                let r = lex.resolve(p);
                println!("  {p:40} -> {} [{:.2}]", r.signature(), r.confidence);
            }
        }
        Err(e) => {
            eprintln!("reload failed: {e}");
            std::process::exit(1);
        }
    }
}
