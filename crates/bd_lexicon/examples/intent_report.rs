//! Summarise the LOCAL playtest intent log (`logs/intent.log`, written by the game for every cast; never
//! sent anywhere) so recognition can be improved after a playtest:
//!
//!     cargo run --release -p bd_lexicon --example intent_report                 # logs/intent.log
//!     cargo run --release -p bd_lexicon --example intent_report -- path/to/intent.log --now
//!
//! Sections: totals (typed / voice, targeted, deliveries, stages), voice lines Whisper was unsure of,
//! low-confidence / improvised resolutions, words the ring ignored (most frequent first — candidates
//! for `everyday.py` anchors, aliases or `data/relations.ron` / `data/intent.ron` entries), loose name
//! matches, and `logs/unresolved.txt` totals. `--now` re-reads each odd phrase with the current
//! lexicon (did a data change fix it?).
//!
//! Log line (tab-separated): `time  typed|voice  heard-confidence|-  phrase  stages  target  delivery
//! summary` — the target is `P<n>` (n-th other player; `~d` = a typo / sound-alike match), never a name.

use std::collections::BTreeMap;
use std::path::PathBuf;

struct Line {
    time: String,
    voice: bool,
    heard: Option<f32>,
    phrase: String,
    stages: String,
    target: String,
    delivery: String,
    summary: String,
}

impl Line {
    fn conf(&self) -> f32 {
        self.summary
            .split_whitespace()
            .find_map(|w| w.strip_prefix("conf=").and_then(|c| c.parse().ok()))
            .unwrap_or(1.0)
    }
    fn ignored(&self) -> Vec<String> {
        let Some(i) = self.summary.find("ignored=[") else { return Vec::new() };
        let rest = &self.summary[i + 9..];
        let body = rest.split(']').next().unwrap_or("");
        body.split(',').map(|w| w.trim().trim_matches('"').to_string()).filter(|w| !w.is_empty()).collect()
    }
    fn improvised(&self) -> bool {
        self.summary.starts_with("Blob0")
    }
}

fn parse(text: &str) -> Vec<Line> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 8 {
                return None;
            }
            Some(Line {
                time: f[0].to_string(),
                voice: f[1] == "voice",
                heard: f[2].parse().ok(),
                phrase: f[3].to_string(),
                stages: f[4].to_string(),
                target: f[5].to_string(),
                delivery: f[6].to_string(),
                summary: f[7..].join(" "),
            })
        })
        .collect()
}

fn pct(a: usize, b: usize) -> String {
    format!("{:.0}%", if b == 0 { 0.0 } else { a as f64 * 100.0 / b as f64 })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let now = args.iter().any(|a| a == "--now");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from).unwrap_or_else(|| root.join("logs/intent.log"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("no intent log at {} (the game writes it on every cast)", path.display());
        std::process::exit(1);
    };
    let lines = parse(&text);
    let n = lines.len();
    let voice: Vec<&Line> = lines.iter().filter(|l| l.voice).collect();
    println!("== intent log: {} ({} casts, {} .. {})", path.display(), n, lines.first().map_or("", |l| &l.time), lines.last().map_or("", |l| &l.time));
    println!("typed {}  voice {}  targeted {} ({})", n - voice.len(), voice.len(), lines.iter().filter(|l| l.target != "-").count(), pct(lines.iter().filter(|l| l.target != "-").count(), n));
    let mut by_delivery: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_stage: BTreeMap<String, usize> = BTreeMap::new();
    for l in &lines {
        *by_delivery.entry(l.delivery.as_str()).or_default() += 1;
        for s in l.stages.split(',').filter(|s| !s.is_empty() && *s != "-") {
            *by_stage.entry(s.split(':').next().unwrap_or(s).to_string()).or_default() += 1;
        }
    }
    println!("deliveries: {by_delivery:?}");
    println!("stages:     {by_stage:?}");
    if !voice.is_empty() {
        let mean = voice.iter().filter_map(|l| l.heard).sum::<f32>() / voice.iter().filter(|l| l.heard.is_some()).count().max(1) as f32;
        println!("voice: mean Whisper confidence {mean:.2}");
    }

    let lex = if now {
        match bd_lexicon::Lexicon::load(root.join("assets/lexicon.bin")) {
            Ok(l) => Some(l),
            Err(e) => {
                eprintln!("--now: cannot load the lexicon: {e}");
                None
            }
        }
    } else {
        None
    };
    let show = |title: &str, list: &[&Line]| {
        if list.is_empty() {
            return;
        }
        println!("\n-- {title} ({})", list.len());
        for l in list.iter().take(40) {
            let heard = l.heard.map_or(String::new(), |h| format!(" heard {h:.2}"));
            println!("  [{}{heard}] {:40} -> {}", if l.voice { "voice" } else { "typed" }, l.phrase, l.summary);
            if let Some(lex) = &lex {
                let it = lex.intent(&l.phrase, &[], l.voice);
                println!("  {:>48} now: {}", "", it.summary(&[]));
            }
        }
    };
    let mut unsure: Vec<&Line> = voice.iter().copied().filter(|l| l.heard.is_some_and(|h| h < 0.6)).collect();
    unsure.sort_by(|a, b| a.heard.partial_cmp(&b.heard).unwrap_or(std::cmp::Ordering::Equal));
    show("voice lines Whisper was unsure of (< 0.60)", &unsure);
    let mut weak: Vec<&Line> = lines.iter().filter(|l| l.conf() < 0.5 || l.improvised()).collect();
    weak.sort_by(|a, b| a.conf().partial_cmp(&b.conf()).unwrap_or(std::cmp::Ordering::Equal));
    show("low-confidence / improvised resolutions", &weak);
    let loose: Vec<&Line> = lines.iter().filter(|l| l.target.contains('~')).collect();
    show("names matched loosely (typo / sound-alike)", &loose);
    let mut words: BTreeMap<String, usize> = BTreeMap::new();
    for l in &lines {
        for w in l.ignored() {
            *words.entry(w).or_default() += 1;
        }
    }
    let mut words: Vec<(String, usize)> = words.into_iter().collect();
    words.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    if !words.is_empty() {
        println!("\n-- ignored words (most frequent first; add anchors / aliases / relations entries)");
        for (w, c) in words.iter().take(40) {
            println!("  {c:4}  {w}");
        }
    }
    if let Ok(u) = std::fs::read_to_string(path.with_file_name("unresolved.txt")) {
        println!("\n-- logs/unresolved.txt: {} lines (improvised casts; see also examples/review.rs)", u.lines().count());
    }
}
