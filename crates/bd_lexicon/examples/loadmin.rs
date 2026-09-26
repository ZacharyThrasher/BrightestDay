//! Min-of-N load phase timings (robust to machine contention).
use std::time::Instant;
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let files = bd_lexicon::Lexicon::icon_files(&dir);
    let (mut read, mut icons, mut data, mut full) = (f64::MAX, f64::MAX, f64::MAX, f64::MAX);
    for _ in 0..10 {
        let t = Instant::now();
        let bytes = std::fs::read(dir.join("lexicon.bin")).unwrap_or_default();
        read = read.min(t.elapsed().as_secs_f64());
        let t = Instant::now();
        let _ = bd_lexicon::Lexicon::icons_only(&files);
        icons = icons.min(t.elapsed().as_secs_f64());
        let t = Instant::now();
        let _ = bd_lexicon::Lexicon::from_bytes(bytes, &[]);
        data = data.min(t.elapsed().as_secs_f64());
        let t = Instant::now();
        let _ = bd_lexicon::Lexicon::load(dir.join("lexicon.bin"));
        full = full.min(t.elapsed().as_secs_f64());
    }
    println!(
        "min of 10: read {:.1} ms, icons {:.1} ms, data parse {:.1} ms, full load {:.1} ms",
        read * 1e3,
        icons * 1e3,
        data * 1e3,
        full * 1e3
    );
}
