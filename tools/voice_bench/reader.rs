//! VOICE BENCH — corpus + scoring stages (dev tool, never shipped; compiled only into bd_game's
//! test binary via `voice_fluency.rs`). See tools/voice_bench/README.md for the whole pipeline.
//!
//!   corpus: `cargo test -p bd_game --bin brightest_day voice_bench_corpus -- --ignored --nocapture`
//!           → target/voice_bench/{corpus.tsv, tts_jobs.tsv, clips.tsv, prompt_old.txt, prompt_new.txt}
//!   score:  `BENCH_HEARD=heard_new.tsv BENCH_TAG=new [BENCH_RAW=1] [BENCH_NORM=old] cargo test … voice_bench_score …`
//!           → target/voice_bench/report_<tag>.md + misses_<tag>.tsv
//!
//! TRUTH is the TYPED reading of the line (voice == typing): a spoken clip hits when the transcript,
//! through the game's voice path (`voice_fluency::heard_line` → `toolbox::command_for(spoken)` /
//! `Lexicon::resolve_spoken`), reads EXACTLY like the typed line (same moves in the same order, same
//! described spec, same twist, same construct).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::PathBuf;

use bd_core::PlayerId;
use bd_lexicon::Lexicon;
use bd_sim::powers::chain::ToolStep;
use bd_sim::powers::toolbox::MoveIndex;
use bd_sim::powers::{norm_words, PowerTable};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
/// `BENCH_DIR` overrides (a snapshot tree scoring into the main tree's bench folder).
fn out_dir() -> PathBuf {
    std::env::var_os("BENCH_DIR").map(PathBuf::from).unwrap_or_else(|| root().join("target/voice_bench"))
}
fn lex() -> Lexicon {
    Lexicon::load(root().join("assets/lexicon.bin")).expect("assets/lexicon.bin (python tools/lexicon_build/build.py)")
}

const ROSTER: [(PlayerId, &str); 4] = [(PlayerId(2), "Ethan"), (PlayerId(3), "Mia"), (PlayerId(4), "Sam"), (PlayerId(5), "Nova")];

/// How a line reads (the console's order: a toolbox command, else the lexicon's construct).
pub fn reading(lex: &Lexicon, line: &str, spoken: bool) -> String {
    let t = PowerTable::get();
    match crate::toolbox::command_for(lex, None, true, line, spoken, &ROSTER) {
        Some(steps) => steps
            .iter()
            .map(|s| match s {
                ToolStep::Move { mv, .. } => match mv.resolve(t) {
                    Some((d, m)) => format!("move:{}/{}/{}:{}", d.id, mv.tier, mv.slot, m.name),
                    None => format!("move:{mv:?}"),
                },
                ToolStep::Held(sel) => format!("held:{sel}"),
                ToolStep::Cast { recipe, signature, .. } => format!("cast:{}{}", recipe.name.to_lowercase(), if signature.is_some() { "+sig" } else { "" }),
                ToolStep::Composed(c) => format!("desc:{c:?}"),
                ToolStep::Twist(tw) => format!("twist:{:?}", **tw),
            })
            .collect::<Vec<_>>()
            .join(" > "),
        None => {
            let r = if spoken { lex.resolve_spoken(line) } else { lex.resolve(line) };
            let g = r.recipe.grants.as_ref().map(|g| format!("+grant:{g:?}")).unwrap_or_default();
            format!("thing:{}{g}", r.recipe.name.to_lowercase())
        }
    }
}

/// Is a reading a power move (not a plain construct)?
fn is_move_reading(sig: &str) -> bool {
    !sig.starts_with("thing:") || sig.contains("+grant")
}

// ------------------------------------------------------------------------------------------------
// Corpus
// ------------------------------------------------------------------------------------------------

/// What players actually yell: the headline moves of the biggest properties (validated against the
/// data — a line whose TYPED reading isn't a move is reported as a data gap).
const FAMOUS: &[&str] = &[
    "Kamehameha", "Rasengan", "Chidori", "Rasenshuriken", "Amaterasu", "Tsukuyomi", "Susanoo", "Kamui", "Almighty Push",
    "Universal Pull", "Planetary Devastation", "Shadow Clone Jutsu", "Fireball Jutsu", "Flying Thunder God", "Eight Gates",
    "Spirit Bomb", "Final Flash", "Galick Gun", "Big Bang Attack", "Special Beam Cannon", "Destructo Disc", "Solar Flare",
    "Instant Transmission", "Masenko", "Hollow Purple", "Domain Expansion", "Black Flash", "Cursed Technique Reversal Red",
    "Getsuga Tensho", "Bankai", "Gum Gum Pistol", "Gum Gum Gatling", "Gear Fifth", "Conqueror's Haki", "Za Warudo",
    "Ora Ora Ora", "Muda Muda Muda", "Road Roller", "Star Platinum The World", "Killer Queen", "Bites the Dust",
    "Hadouken", "Shoryuken", "Tatsumaki Senpukyaku", "Sonic Boom", "Get Over Here", "Fatality", "Falcon Punch",
    "Falcon Kick", "PK Thunder", "Fus Ro Dah", "Yol Toor Shul", "Expelliarmus", "Avada Kedavra", "Expecto Patronum",
    "Wingardium Leviosa", "Stupefy", "Force Lightning", "Force Push", "Force Choke", "Hulk Smash", "Repulsor Blast",
    "Unibeam", "Web Swing", "Adamantium Claws", "Optic Blast", "Bamf", "Earthshatter", "Barrier Field",
    "Rocket Punch", "Pulse Bomb", "Recall", "Blink", "Dragonblade", "Deadeye", "High Noon", "Nano Boost", "Graviton Surge",
    "Death Blossom", "Rip Tire", "Self Destruct", "Meteor Strike", "Thunderbolt", "Hydro Pump", "Flamethrower", "Solar Beam",
    "Hyper Beam", "Shadow Ball", "Hadoken", "Chaos Control", "Spin Dash", "Hylian Shield", "Triforce", "Hookshot",
    "Ground Pound", "Final Smash", "Avatar State", "Airbending", "Lightning Bending", "Blood Bending", "Metal Bending",
    "Omnislash", "Limit Break", "Leap of Faith", "Spartan Rage", "Leviathan Axe", "Portal Gun", "Mjolnir Throw",
    "Bifrost", "Web Shooter", "Shield Throw", "Iron Man Suit Up", "Infinity Gauntlet Snap", "Snap", "Mirror Dimension",
    "Heat Vision", "Freeze Breath", "Batarang", "Grappling Hook", "Speed Force", "Lasso of Truth", "Green Lantern Construct",
    "Kaioken", "Super Saiyan", "Ultra Instinct", "Sharingan", "Rinnegan", "Byakugan", "Sage Mode", "Nine Tails Chakra Mode",
    "Wood Style", "Water Dragon Jutsu", "Summoning Jutsu", "Detroit Smash", "Delaware Smash", "One For All", "Explosion",
    "Serious Punch", "Consecutive Normal Punches", "Titan Transformation", "Thunder Spear", "Water Breathing",
    "Hinokami Kagura", "Thunder Breathing First Form", "Nen", "Jajanken", "Ice Age", "Hiken", "Room Shambles",
    "Gomu Gomu no Red Hawk", "King Kong Gun", "Kamehameha Times Ten",
];

const DESCRIBED: &[&str] = &[
    "shadow teleport behind ethan", "teleport behind him", "gravity punch", "fire uppercut", "lightning dash through him",
    "rewind 3 seconds", "pull everyone to me", "leap onto the tower", "swap with the dragon", "blink above him and slam",
    "fire and ice punch", "punch him", "ice him", "freeze everyone", "hook him", "grab him", "cage ethan", "hex him",
    "dash through him", "phase through the wall", "ground pound", "rapid fire punch at ethan", "quick dash", "long blink",
    "absorb his attack", "deflect", "punch with fire and ice", "teleport behind mia", "fire teleport behind him",
    "giant punch", "electric uppercut", "shadow dash", "ice slam", "wind kick", "poison punch", "lightning punch at nova",
    "teleport above him", "pull him to me", "push everyone away", "flame dash",
];

const MODIFIED: &[(&str, &str)] = &[
    ("homing chidori", "homing chidori"), ("triple kamehameha", "triple kamehameha"), ("ice hollow purple", "ice hollow purple"),
    ("black flash but fire", "black flash but fire"), ("reverse earthshatter", "reverse earthshatter"),
    ("rapid fire hadouken", "rapid fire hadouken"), ("delayed pulse bomb", "delayed pulse bomb"),
    ("chain lightning rocket punch", "chain lightning rocket punch"), ("mini spirit bomb", "mini spirit bomb"),
    ("doomfist giant punch", "doomfist giant punch"), ("giant rasengan", "giant rasengan"), ("homing rasengan", "homing rasengan"),
    ("giant homing rasengan", "giant homing rasengan"), ("fire rasengan", "fire rasengan"), ("ice kamehameha", "ice kamehameha"),
    ("double hadouken", "double hadouken"), ("mega falcon punch", "mega falcon punch"), ("tiny spirit bomb", "tiny spirit bomb"),
    ("lightning amaterasu", "lightning amaterasu"), ("rasengan plus amaterasu", "rasengan + amaterasu"),
    ("chidori plus rasengan", "chidori + rasengan"), ("fuse rasengan and chidori", "fuse rasengan and chidori"),
    ("rasengan and chidori combined", "rasengan and chidori combined"), ("bamf with almighty push", "bamf with almighty push"),
    ("kamehameha plus final flash", "kamehameha + final flash"), ("giant chidori", "giant chidori"),
    ("homing hadouken", "homing hadouken"), ("explosive rasengan", "explosive rasengan"), ("gravity kamehameha", "gravity kamehameha"),
    ("poison chidori", "poison chidori"), ("quick amaterasu", "quick amaterasu"), ("huge getsuga tensho", "huge getsuga tensho"),
    ("triple rasengan", "triple rasengan"), ("rapid fire shoryuken", "rapid fire shoryuken"), ("delayed hollow purple", "delayed hollow purple"),
    ("fire and ice rasengan", "fire and ice rasengan"), ("homing fire punch", "homing fire punch"), ("rapid fire uppercut", "rapid fire uppercut"),
    ("mini rasenshuriken", "mini rasenshuriken"), ("mega spirit bomb", "mega spirit bomb"),
];

const CONSTRUCTS: &[&str] = &[
    "flaming hammer", "giant ice sword", "bouncy wall with legs", "a dragon", "three flaming swords", "rock wall", "fire portal",
    "a big flaming hammer", "sand shield", "ice sword", "a giant robot", "a flying car", "drop a meteor on ethan", "throw a fireball",
    "a wall of fire", "lightning spear", "a pirate ship", "a tank", "a shark with a laser", "giant fist", "a cage", "a bridge",
    "a staircase", "ten bees", "a giant boulder", "a tornado", "a black hole", "an ice wall", "a flaming sword", "a golden shield",
    "a trampoline", "a cannon", "a helicopter", "a spiked ball", "a giant spider", "a train", "a castle", "a lava pool",
    "goku", "optimus prime", "doomfist", "naruto", "pikachu", "charizard", "godzilla", "batman", "mjolnir", "captain america's shield",
    "hammer", "a sword", "a big wall", "hot dog", "a rubber duck", "the moon",
];

/// Chain separators as SAID (text) and as TYPED; "…" = a spoken beat (a pause, no word).
const SEPS: &[(&str, &str, u32)] = &[(" then ", " then ", 30), (", ", ", ", 15), (" into ", " into ", 10), (" and then ", " and then ", 15), ("…", ", ", 30)];

const VOICES: &[&str] = &["sapi:David", "sapi:Zira", "rt:Mark", "rt:David", "rt:Zira"];
const CONDS: &[&str] = &["clean", "noise", "room", "quiet"];
const RATES: &[&str] = &["-10%", "0%", "+15%"];

struct Line {
    id: String,
    cat: &'static str,
    /// What the TTS says (SSML body).
    ssml: String,
    /// What a player would type for the same thing (the truth's input).
    typed: String,
    /// Plain text of what is said (for the report).
    said: String,
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, c| (h ^ c as u64).wrapping_mul(0x100000001b3))
}

fn esc(s: &str) -> String {
    s.replace('&', "and").replace('<', " ").replace('>', " ").replace('\'', "&apos;").replace('"', " ")
}

fn prosody(body: &str, rate: &str) -> String {
    format!("<prosody rate='{rate}'>{body}</prosody>")
}

#[test]
#[ignore]
fn voice_bench_corpus() {
    let lex = lex();
    let t = PowerTable::get();
    let ix = MoveIndex::shipped();
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let mut lines: Vec<Line> = Vec::new();
    let push = |cat: &'static str, said: String, ssml: String, typed: String, lines: &mut Vec<Line>| {
        let id = format!("{cat}{:03}", lines.iter().filter(|l| l.cat == cat).count());
        lines.push(Line { id, cat, ssml, typed, said });
    };
    let rate_of = |s: &str| RATES[(fnv(s) % RATES.len() as u64) as usize];

    // famous moves
    let mut famous_moves: Vec<&str> = Vec::new();
    let mut gaps: Vec<String> = Vec::new();
    for &f in FAMOUS {
        let sig = reading(&lex, f, false);
        if is_move_reading(&sig) {
            famous_moves.push(f);
        } else {
            gaps.push(format!("{f} → {sig}"));
        }
        push("famous", f.into(), prosody(&esc(f), rate_of(f)), f.into(), &mut lines);
    }

    // character kits: "<name> <move>" / "<name> ultimate" / "<name> <slot word>"
    let hand_kits: Vec<(usize, &bd_sim::powers::PowerDef)> = t.powers.iter().enumerate().filter(|(_, p)| p.kind.is_kit() && p.derived.is_none() && !p.names.is_empty()).collect();
    let derived_kits: Vec<(usize, &bd_sim::powers::PowerDef)> = t.powers.iter().enumerate().filter(|(_, p)| p.kind.is_kit() && p.derived.is_some() && !p.names.is_empty()).collect();
    let pick = |n: usize, want: usize| -> Vec<usize> { (0..want.min(n)).map(|k| k * n / want.min(n).max(1)).collect() };
    let mut kit_phrases: Vec<String> = Vec::new();
    for (k, i) in pick(hand_kits.len(), 170).into_iter().enumerate() {
        let p = hand_kits[i].1;
        let name = p.names[0].clone();
        let phrase = match k % 3 {
            0 => format!("{name} ultimate"),
            1 => format!("{name} {}", p.moves.first().map(|m| m.name.clone()).unwrap_or_default()),
            _ => format!("{name} {}", p.moves.get(1 + k % p.moves.len().max(1)).or(p.moves.first()).map(|m| m.name.clone()).unwrap_or_default()),
        };
        kit_phrases.push(phrase.clone());
        push("kit", phrase.clone(), prosody(&esc(&phrase), rate_of(&phrase)), phrase, &mut lines);
    }
    for (k, i) in pick(derived_kits.len(), 50).into_iter().enumerate() {
        let p = derived_kits[i].1;
        let name = p.names[0].clone();
        let phrase = if k % 2 == 0 { format!("{name} ultimate") } else { format!("{name} {}", p.moves.first().map(|m| m.name.clone()).unwrap_or_default()) };
        push("kit_gen", phrase.clone(), prosody(&esc(&phrase), rate_of(&phrase)), phrase, &mut lines);
    }

    // obscure names: each hand power's canonical (first) shout per slot, not in the famous list
    let famous_norm: Vec<String> = FAMOUS.iter().map(|f| norm_words(f)).collect();
    let mut canon: Vec<String> = Vec::new();
    for p in t.powers.iter().filter(|p| p.derived.is_none()) {
        let mut seen = Vec::new();
        for (s, slot) in &p.shouts {
            if seen.contains(slot) {
                continue;
            }
            seen.push(*slot);
            let n = norm_words(s);
            if n.len() >= 4 && !famous_norm.contains(&n) && ix.shout(&n).is_some() {
                canon.push(s.clone());
            }
        }
    }
    canon.sort();
    canon.dedup();
    for i in pick(canon.len(), 170) {
        let s = canon[i].clone();
        push("obscure", s.clone(), prosody(&esc(&s), rate_of(&s)), s, &mut lines);
    }

    // chains of famous moves / kit moves
    let pool: Vec<String> = famous_moves.iter().map(|s| s.to_string()).chain(kit_phrases.iter().filter(|k| is_move_reading(&reading(&lex, k, false))).cloned()).collect();
    for c in 0..130usize {
        let h = fnv(&format!("chain{c}"));
        let n = match h % 10 {
            0..=5 => 2,
            6..=8 => 3,
            _ => 4,
        };
        let steps: Vec<&String> = (0..n).map(|k| &pool[((h >> (8 + 7 * k)) as usize + c * 13 + k * 31) % pool.len()]).collect();
        // weighted round robin over the separators (SEPS weights, out of 100)
        let mut roll = ((c * 37) % 100) as u32;
        let sep = SEPS.iter().find(|s| {
            if roll < s.2 {
                true
            } else {
                roll -= s.2;
                false
            }
        }).unwrap_or(&SEPS[0]);
        let typed = steps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(sep.1);
        let (said, body) = if sep.0 == "…" {
            (steps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" … "), steps.iter().map(|s| esc(s)).collect::<Vec<_>>().join("<break time='550ms'/>"))
        } else {
            let s = steps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(sep.0);
            (s.clone(), esc(&s))
        };
        let cat = if sep.0 == "…" { "chain_pause" } else { "chain" };
        push(cat, said, prosody(&body, rate_of(&typed)), typed, &mut lines);
    }

    for &d in DESCRIBED {
        push("described", d.into(), prosody(&esc(d), rate_of(d)), d.into(), &mut lines);
    }
    for &(said, typed) in MODIFIED {
        push("modifier", said.into(), prosody(&esc(said), rate_of(said)), typed.into(), &mut lines);
    }

    // shouted / elongated delivery of famous moves
    for (k, f) in famous_moves.iter().enumerate().filter(|(k, _)| k % 3 == 0) {
        let body = if k % 2 == 0 {
            format!("<prosody volume='x-loud' pitch='+30%' rate='+10%'>{}!</prosody>", esc(&f.to_uppercase()))
        } else {
            // the last word dragged out ("kamehameeeehaaaa")
            let w: Vec<&str> = f.split(' ').collect();
            let (head, last) = w.split_at(w.len() - 1);
            format!("{} <prosody rate='x-slow' pitch='+25%' volume='loud'>{}</prosody>", esc(&head.join(" ")), esc(last[0]))
        };
        push("shouted", format!("{f} (shouted)"), body, f.to_string(), &mut lines);
    }
    for (spelled, typed) in [
        ("Kamehamehaaaaa", "Kamehameha"), ("Rasengaaaan", "Rasengan"), ("Hadoukeeeen", "Hadouken"), ("Fus Ro Daaaah", "Fus Ro Dah"),
        ("Chidoriiii", "Chidori"), ("Shoryukeeeen", "Shoryuken"), ("Falcon Paaaawnch", "Falcon Punch"), ("Getsugaaa Tenshooo", "Getsuga Tensho"),
        ("Kaaamehaaameha", "Kamehameha"), ("Amaterasuuuu", "Amaterasu"), ("Spirit Booooomb", "Spirit Bomb"), ("Hollow Purrrple", "Hollow Purple"),
        ("Bankaaaai", "Bankai"), ("Ora ora ora ora ora ora", "Ora Ora Ora"), ("Muda muda muda muda muda", "Muda Muda Muda"),
        ("Za Waaarudo", "Za Warudo"), ("Get over heeeere", "Get Over Here"), ("Detroit Smaaaash", "Detroit Smash"),
    ] {
        push("elongated", spelled.into(), format!("<prosody volume='x-loud'>{}</prosody>", esc(spelled)), typed.into(), &mut lines);
    }

    for &c in CONSTRUCTS {
        push("construct", c.into(), prosody(&esc(c), rate_of(c)), c.into(), &mut lines);
    }

    // write: corpus (with the typed truth), TTS jobs (3 voices a line), clips (one condition each)
    let mut corpus = String::from("# id\tcat\tsaid\ttyped\ttruth\n");
    let mut jobs = String::new();
    let mut clips = String::new();
    for l in &lines {
        let truth = reading(&lex, &l.typed, false);
        writeln!(corpus, "{}\t{}\t{}\t{}\t{}", l.id, l.cat, l.said, l.typed, truth).unwrap();
        let h = fnv(&l.id) as usize;
        for k in 0..3 {
            let v = VOICES[(h + k * 2) % VOICES.len()];
            let wav = format!("{}_{}.wav", l.id, v.replace(':', "_"));
            writeln!(jobs, "{wav}\t{v}\t{}", l.ssml).unwrap();
            let cond = CONDS[(h / 7 + k) % CONDS.len()];
            writeln!(clips, "{}.{k}\t{wav}\t{cond}", l.id).unwrap();
        }
    }
    std::fs::write(dir.join("corpus.tsv"), corpus).unwrap();
    std::fs::write(dir.join("tts_jobs.tsv"), jobs).unwrap();
    std::fs::write(dir.join("clips.tsv"), clips).unwrap();

    // Whisper prompts: the game's current one (lexicon bias + the default corps' oath words) and the
    // voice-fluency one
    let oaths = crate::oath::OathTexts::from_ron(include_str!("../../assets/oaths.ron")).unwrap();
    let corps = bd_core::corps::Corps::Willpower;
    std::fs::write(dir.join("prompt_old.txt"), format!("{}, {}", lex.bias_prompt(60), oaths.bias_words(corps, 24))).unwrap();
    std::fs::write(dir.join("prompt_new.txt"), super::bias_prompt(&lex, &oaths.bias_words(corps, 24))).unwrap();

    let mut by_cat: BTreeMap<&str, usize> = BTreeMap::new();
    for l in &lines {
        *by_cat.entry(l.cat).or_default() += 1;
    }
    eprintln!("voice bench corpus: {} lines ({by_cat:?}), {} clips → {}", lines.len(), lines.len() * 3, dir.display());
    eprintln!("famous names whose TYPED reading is not a move ({}):", gaps.len());
    for g in &gaps {
        eprintln!("  {g}");
    }
}

// ------------------------------------------------------------------------------------------------
// Scoring
// ------------------------------------------------------------------------------------------------

struct Heard {
    text: String,
    ms: u32,
    conf: f32,
    ok: bool,
}

fn pct(a: usize, b: usize) -> f32 {
    if b == 0 { 0.0 } else { 100.0 * a as f32 / b as f32 }
}

fn quant(v: &mut [f32], q: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f32 * q).round() as usize]
}

#[test]
#[ignore]
fn voice_bench_score() {
    let lex = lex();
    let dir = out_dir();
    let heard_file = std::env::var("BENCH_HEARD").unwrap_or_else(|_| "heard.tsv".into());
    let tag = std::env::var("BENCH_TAG").unwrap_or_else(|_| "run".into());
    let raw = std::env::var("BENCH_RAW").is_ok_and(|v| v == "1");
    let old_norm = std::env::var("BENCH_NORM").is_ok_and(|v| v == "old");
    let corpus = std::fs::read_to_string(dir.join("corpus.tsv")).expect("corpus.tsv (run voice_bench_corpus)");
    let heard_src = std::fs::read_to_string(dir.join(&heard_file)).expect("heard file");
    struct C {
        cat: String,
        said: String,
        typed: String,
        truth: String,
    }
    let mut lines: HashMap<String, C> = HashMap::new();
    for l in corpus.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split('\t').collect();
        // the truth is re-read now (the reader may have changed since the corpus was written)
        let truth = reading(&lex, f[3], false);
        lines.insert(f[0].into(), C { cat: f[1].into(), said: f[2].into(), typed: f[3].into(), truth });
    }
    let clip_meta: HashMap<String, (String, String)> = std::fs::read_to_string(dir.join("clips.tsv"))
        .unwrap()
        .lines()
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let voice = f[1].rsplitn(3, '_').take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(":").trim_end_matches(".wav").to_string();
            (f[0].to_string(), (voice, f[2].to_string()))
        })
        .collect();

    #[derive(Default)]
    struct Agg {
        n: usize,
        hit: usize,
        heard_ok: usize,
        hijack: usize,
        whisper: Vec<f32>,
        reader: Vec<f32>,
    }
    let mut by_cat: BTreeMap<String, Agg> = BTreeMap::new();
    let mut by_voice: BTreeMap<String, Agg> = BTreeMap::new();
    let mut by_cond: BTreeMap<String, Agg> = BTreeMap::new();
    let mut misses = String::from("# clip\tcat\tvoice\tcond\tsaid\ttyped\theard\tline\ttruth\tgot\n");
    let mut miss_by_phrase: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
    let mut total = Agg::default();
    // warm the indices
    let _ = reading(&lex, "rasengan then chidori", true);
    for l in heard_src.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = l.split('\t').collect();
        let clip = f[0];
        let Some(c) = lines.get(clip.rsplit_once('.').map(|x| x.0).unwrap_or(clip)) else { continue };
        let h = Heard { text: f.get(1).unwrap_or(&"").to_string(), ms: f.get(2).and_then(|s| s.parse().ok()).unwrap_or(0), conf: f.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0), ok: f.get(4) == Some(&"ok") };
        // Whisper's raw text → bd_voice's clean-up (old: no beats; new: pause punctuation kept);
        // a streamed clip is several beats (" | ")
        let segs: Vec<String> = h.text.split(" | ").map(|s| if old_norm { bd_voice::text::normalize_transcript(s) } else { bd_voice::text::normalize_line(s) }).filter(|s| !s.is_empty()).collect();
        let t0 = std::time::Instant::now();
        let (line, got) = if !h.ok || segs.is_empty() {
            (String::new(), "(nothing)".to_string())
        } else if raw {
            let l = segs.join(", ");
            let g = reading(&lex, &l, true);
            (l, g)
        } else if segs.len() > 1 {
            // STREAMING, as voice_ui runs it: each beat that is a move fires as it is heard, the
            // rest waits and is read with what follows
            let mut beats = super::Beats::default();
            let mut fired: Vec<String> = Vec::new();
            for s in &segs[..segs.len() - 1] {
                if let Some(l) = beats.beat(&lex, s) {
                    fired.push(l);
                }
            }
            let rest = beats.finish(&segs[segs.len() - 1]);
            if !rest.is_empty() {
                fired.push(super::heard_line(&lex, &rest));
            }
            let g = fired.iter().map(|l| reading(&lex, l, true)).collect::<Vec<_>>().join(" > ");
            (fired.join(" | "), g)
        } else {
            let l = super::heard_line(&lex, &segs[0]);
            let g = reading(&lex, &l, true);
            (l, g)
        };
        let reader_ms = t0.elapsed().as_secs_f32() * 1000.0;
        let hit = got == c.truth;
        let hijack = c.cat == "construct" && !is_move_reading(&c.truth) && is_move_reading(&got);
        let (voice, cond) = clip_meta.get(clip).cloned().unwrap_or_default();
        for a in [by_cat.entry(c.cat.clone()).or_default(), by_voice.entry(voice.clone()).or_default(), by_cond.entry(cond.clone()).or_default(), &mut total] {
            a.n += 1;
            a.hit += hit as usize;
            a.heard_ok += h.ok as usize;
            a.hijack += hijack as usize;
            a.whisper.push(h.ms as f32);
            a.reader.push(reader_ms);
        }
        let e = miss_by_phrase.entry(format!("{}\t{}", c.cat, c.said)).or_default();
        e.1 += 1;
        if !hit {
            e.0 += 1;
            e.2.push(h.text.clone());
            writeln!(misses, "{clip}\t{}\t{voice}\t{cond}\t{}\t{}\t{}\t{line}\t{}\t{got}", c.cat, c.said, c.typed, h.text, c.truth).unwrap();
        }
        let _ = h.conf;
    }

    let mut r = String::new();
    writeln!(
        r,
        "# Voice bench — {tag}\n\nheard file `{heard_file}`, clean-up `{}`, voice path {}\n",
        if old_norm { "normalize_transcript (old)" } else { "normalize_line" },
        if raw { "RAW (the clean transcript read as-is)" } else { "voice_fluency::heard_line (+ streaming beats)" }
    )
    .unwrap();
    let table = |r: &mut String, title: &str, m: &mut BTreeMap<String, Agg>| {
        writeln!(r, "## by {title}\n\n| {title} | clips | hit % | heard % | whisper ms p50 / p90 | reader ms p50 / p90 | hijacked |\n|---|---|---|---|---|---|---|").unwrap();
        for (k, a) in m.iter_mut() {
            writeln!(
                r,
                "| {k} | {} | {:.1} | {:.1} | {:.0} / {:.0} | {:.2} / {:.2} | {} |",
                a.n,
                pct(a.hit, a.n),
                pct(a.heard_ok, a.n),
                quant(&mut a.whisper, 0.5),
                quant(&mut a.whisper, 0.9),
                quant(&mut a.reader, 0.5),
                quant(&mut a.reader, 0.9),
                a.hijack
            )
            .unwrap();
        }
        writeln!(r).unwrap();
    };
    writeln!(
        r,
        "**TOTAL: {} clips, hit {:.1}%**, whisper p50 {:.0} ms / p90 {:.0} ms, reader p50 {:.2} ms / p90 {:.2} ms\n",
        total.n,
        pct(total.hit, total.n),
        quant(&mut total.whisper, 0.5),
        quant(&mut total.whisper, 0.9),
        quant(&mut total.reader, 0.5),
        quant(&mut total.reader, 0.9)
    )
    .unwrap();
    table(&mut r, "category", &mut by_cat);
    table(&mut r, "voice", &mut by_voice);
    table(&mut r, "condition", &mut by_cond);
    let mut worst: Vec<(&String, &(usize, usize, Vec<String>))> = miss_by_phrase.iter().filter(|(_, v)| v.0 > 0).collect();
    worst.sort_by(|a, b| (b.1.0 * 100 / b.1.1).cmp(&(a.1.0 * 100 / a.1.1)).then(b.1.0.cmp(&a.1.0)).then(a.0.cmp(b.0)));
    writeln!(r, "## phrases missed ({} of {})\n\n| cat | said | missed | heard |\n|---|---|---|---|", worst.len(), miss_by_phrase.len()).unwrap();
    for (k, v) in worst.iter().take(200) {
        let (cat, said) = k.split_once('\t').unwrap();
        writeln!(r, "| {cat} | {said} | {}/{} | {} |", v.0, v.1, v.2.join(" · ")).unwrap();
    }
    std::fs::write(dir.join(format!("report_{tag}.md")), &r).unwrap();
    std::fs::write(dir.join(format!("misses_{tag}.tsv")), &misses).unwrap();
    eprintln!("{}", r.lines().take(40).collect::<Vec<_>>().join("\n"));
}

/// PARITY (no audio): every corpus line typed vs the same words as a clean transcript would carry
/// them (lower case, no punctuation) through the voice path. A gap here is the reader's, not
/// Whisper's. `BENCH_RAW=1` skips `heard_line`.
#[test]
#[ignore]
fn voice_bench_parity() {
    let lex = lex();
    let dir = out_dir();
    let raw = std::env::var("BENCH_RAW").is_ok_and(|v| v == "1");
    let corpus = std::fs::read_to_string(dir.join("corpus.tsv")).expect("corpus.tsv (run voice_bench_corpus)");
    let (mut n, mut same) = (0usize, 0usize);
    let mut gaps = String::from("# cat\ttyped\tspoken line\ttyped reading\tspoken reading\n");
    let mut by_cat: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for l in corpus.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = l.split('\t').collect();
        let typed = f[3];
        // (how it is SAID: "+" is "plus", an arrow is "then")
        let said = bd_voice::text::normalize_line(&typed.replace(" + ", " plus ").replace("->", " then "));
        let line = if raw { said.clone() } else { super::heard_line(&lex, &said) };
        let (a, b) = (reading(&lex, typed, false), reading(&lex, &line, true));
        n += 1;
        let e = by_cat.entry(f[1].to_string()).or_default();
        e.1 += 1;
        if a == b {
            same += 1;
            e.0 += 1;
        } else {
            writeln!(gaps, "{}\t{typed}\t{line}\t{a}\t{b}", f[1]).unwrap();
        }
    }
    let tag = if raw { "raw" } else { "fluent" };
    std::fs::write(dir.join(format!("parity_{tag}.tsv")), &gaps).unwrap();
    eprintln!("PARITY ({tag}): {same}/{n} lines read the same typed and spoken ({:.1}%)", pct(same, n));
    for (c, (s, t)) in &by_cat {
        eprintln!("  {c:<12} {s}/{t}");
    }
}

/// Probe: `BENCH_PROBE="shadow ball|teleport behind him"` → each phrase's typed / spoken / voice-path
/// readings and the lexicon's view (kind, confidence, ignored words).
#[test]
#[ignore]
fn voice_bench_probe() {
    let lex = lex();
    for p in std::env::var("BENCH_PROBE").unwrap_or_default().split('|').filter(|s| !s.trim().is_empty()) {
        let t = lex.resolve(p);
        let s = lex.resolve_spoken(p);
        let line = super::heard_line(&lex, p);
        eprintln!("{p:?}\n  typed  {} | lex {:?} conf {:.2} ignored {:?} fallback {}", reading(&lex, p, false), t.recipe.name, t.confidence, t.ignored, Lexicon::is_fallback(&t));
        eprintln!("  spoken {} | lex {:?} conf {:.2} ignored {:?}", reading(&lex, p, true), s.recipe.name, s.confidence, s.ignored);
        eprintln!("  voice  {line:?} → {}", reading(&lex, &line, true));
        eprintln!(
            "  read_step typed {:?} spoken {:?} | twist spoken {} | understood spoken {}",
            crate::toolbox::read_step(&lex, None, true, p, false),
            crate::toolbox::read_step(&lex, None, true, p, true),
            crate::toolbox::twist_step(&lex, None, true, p, true, None).is_some(),
            crate::toolbox::understood(&lex, p, true)
        );
    }
}
