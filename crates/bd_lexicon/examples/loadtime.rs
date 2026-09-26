//! Measure load phases: `cargo run -p bd_lexicon --example loadtime`
use std::time::Instant;
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    for _ in 0..3 {
        let t = Instant::now();
        let bytes = std::fs::read(dir.join("lexicon.bin")).unwrap_or_default();
        let t_read = t.elapsed();
        let files = bd_lexicon::Lexicon::icon_files(&dir);
        let t_files = t.elapsed();
        let lex = bd_lexicon::Lexicon::from_bytes(bytes, &files);
        let t_all = t.elapsed();
        println!(
            "read {:.1} ms, icon files {:.1} ms, parse+icons {:.1} ms, total {:.1} ms ({} icons, ok={})",
            t_read.as_secs_f64() * 1e3,
            (t_files - t_read).as_secs_f64() * 1e3,
            (t_all - t_files).as_secs_f64() * 1e3,
            t_all.as_secs_f64() * 1e3,
            lex.as_ref().map(|l| l.icon_count()).unwrap_or(0),
            lex.is_ok()
        );
    }
}
