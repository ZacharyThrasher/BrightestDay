//! AUTO-KITS cost check (docs/toolbox/AUTO_KITS.md): what `assets/powers/zz_derived_kits.ron` adds to startup.
//!
//!     cargo run --profile fastrel -p bd_sim --example derived_kits_bench
//!
//! Measures, WITH and WITHOUT the generated file: the power table parse (+ the derived kits' expansion),
//! the phrase index build, a kit-phrase reading, and the lexicon's load (its tolerant power-file scan runs
//! inside `Lexicon::from_parts`; `assets/lexicon.bin` is read once and reused).

use std::time::Instant;

use bd_sim::powers::toolbox::MoveIndex;
use bd_sim::powers::{PowerTable, SHIPPED_FILES};

const DERIVED: &str = "zz_derived_kits.ron";

fn best_of<T>(n: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut best = f64::MAX;
    let mut last = None;
    for _ in 0..n {
        let t = Instant::now();
        let v = f();
        best = best.min(t.elapsed().as_secs_f64() * 1e3);
        last = Some(v);
    }
    (best, last.unwrap())
}

fn main() {
    let with: Vec<(&str, &str)> = SHIPPED_FILES.to_vec();
    let without: Vec<(&str, &str)> = SHIPPED_FILES.iter().copied().filter(|(n, _)| *n != DERIVED).collect();
    let bytes: usize = with.iter().filter(|(n, _)| *n == DERIVED).map(|(_, s)| s.len()).sum();
    println!("{DERIVED}: {} KiB compiled in", bytes / 1024);
    for (label, files) in [("without", &without), ("with", &with)] {
        let (parse, t) = best_of(5, || PowerTable::parse_files(files).expect("parses"));
        let (index, ix) = best_of(5, || MoveIndex::build(&t));
        let phrases: Vec<String> = t.powers.iter().filter(|p| p.kind.is_kit()).take(400).flat_map(|p| p.names.first().map(|n| format!("{n} ultimate"))).collect();
        let (read, _) = best_of(3, || {
            for p in &phrases {
                std::hint::black_box(ix.read(&t, None, p));
            }
        });
        let moves: usize = t.powers.iter().map(|p| p.moves.len()).sum();
        println!(
            "{label:>7}: {} powers / {moves} moves | table parse+expand {parse:.1} ms | index {index:.1} ms | kit reading {:.2} µs/phrase",
            t.powers.len(),
            read * 1e3 / phrases.len().max(1) as f64
        );
    }
    // the lexicon: its load scans every power file (kits are skipped for icons, but still parsed)
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let Ok(lex) = std::fs::read(root.join("lexicon.bin")) else {
        println!("(no assets/lexicon.bin: lexicon timing skipped)");
        return;
    };
    let icons = bd_lexicon::Lexicon::icon_files(&root);
    let powers_all = bd_lexicon::Lexicon::power_files(&root);
    let powers_hand: Vec<(String, String)> = powers_all.iter().filter(|(n, _)| !n.ends_with(DERIVED)).cloned().collect();
    for (label, pf) in [("without", &powers_hand), ("with", &powers_all)] {
        let (ms, l) = best_of(3, || bd_lexicon::Lexicon::from_parts(lex.clone(), &icons, pf).expect("lexicon loads"));
        println!("{label:>7}: lexicon load {ms:.1} ms ({} power files, {} power ids)", pf.len(), l.power_ids().len());
    }
}
