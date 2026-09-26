//! Timing over realistic and hostile inputs: `cargo run --release -p bd_lexicon --example bench`
use std::time::Instant;
fn main() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/lexicon.bin");
    let Ok(lex) = bd_lexicon::Lexicon::load(&p) else {
        eprintln!("no lexicon");
        return;
    };
    let mut phrases: Vec<String> = (0..2000u64).map(|s| lex.random_phrase(s)).collect();
    phrases.extend(
        [
            "hamer",
            "godzila",
            "captian america sheild",
            "giant eeyore shootin laser beams",
            "sharknado",
            "qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz qwxz",
        ]
        .map(String::from),
    );
    let mut garbage = Vec::new();
    let mut x: u64 = 88172645463325252;
    for _ in 0..300 {
        let mut s = String::new();
        for _ in 0..(x % 120) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            s.push(char::from_u32((x % 0x3000) as u32 + 32).unwrap_or('x'));
        }
        garbage.push(s);
    }
    for (label, set, spoken) in [
        ("typed phrases", &phrases, false),
        ("spoken phrases", &phrases, true),
        ("garbage typed", &garbage, false),
        ("garbage spoken", &garbage, true),
    ] {
        let mut worst = 0f64;
        let t0 = Instant::now();
        for p in set.iter() {
            let t = Instant::now();
            std::hint::black_box(if spoken {
                lex.resolve_spoken(p)
            } else {
                lex.resolve(p)
            });
            let e = t.elapsed().as_secs_f64();
            if e > 0.001 && !spoken {
                println!("  slow {:.0} µs: {p:?}", e * 1e6);
            }
            worst = worst.max(e);
        }
        println!(
            "{label:16} n={:5} avg {:7.1} µs  worst {:7.1} µs",
            set.len(),
            t0.elapsed().as_secs_f64() * 1e6 / set.len() as f64,
            worst * 1e6
        );
    }
    let t0 = Instant::now();
    for p in phrases.iter().take(1000) {
        std::hint::black_box(lex.complete(&p[..p.len().min(6)]));
    }
    println!(
        "complete         n= 1000 avg {:7.1} µs",
        t0.elapsed().as_secs_f64() * 1e6 / 1000.0
    );
}
