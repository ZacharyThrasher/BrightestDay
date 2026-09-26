//! Load-time check + icon diagnostics: `cargo run --release -p bd_lexicon --example loadtime [-- --warnings]`
use std::time::Instant;
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let show = std::env::args().any(|a| a == "--warnings");
    for run in 0..3 {
        let t = Instant::now();
        let lex = match bd_lexicon::Lexicon::load(dir.join("lexicon.bin")) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("load failed: {e}");
                return;
            }
        };
        println!(
            "load {:.1} ms ({} entries, {} icons, {} powers, {} load warnings)",
            t.elapsed().as_secs_f64() * 1e3,
            lex.len(),
            lex.icon_count(),
            lex.power_ids().len(),
            lex.load_warnings().len()
        );
        if run == 0 && show {
            for w in lex.load_warnings() {
                println!("  warn: {w}");
            }
            for w in lex.load_notes() {
                println!("  note: {w}");
            }
            for w in lex.validate_icon_shapes() {
                println!("  invalid: {w}");
            }
        }
    }
}
