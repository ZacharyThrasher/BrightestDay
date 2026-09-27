//! POWERS CHECK — validate power files WITHOUT recompiling the game (TOOLBOX, docs/toolbox/).
//!
//!     cargo run --profile fastrel -p bd_sim --example powers_check -- <file.ron>... [--coverage phrases.txt]
//!         [--hold <power_id>[,<power_id>...]] [--dir assets/powers] [--all]
//!
//! Reads every `assets/powers/*.ron` AT RUNTIME (a given file replaces the directory's file of the
//! same name, or joins the table in filename order), then:
//! 1. parses each given file with bd_sim's real schema (`PowerTable`), reporting line / column;
//! 2. runs the lexicon's own tolerant scan (`bd_lexicon::power_scan`) — the warnings the game's
//!    load would print;
//! 3. reports collisions against the WHOLE directory: ids, power / tier names, shouts, kit names
//!    (only those touching the given files, unless `--all`);
//! 4. lints the entries (shout slots that name no move, > 8 moves, empty moves, kits without moves,
//!    a kit name that is also a grant name…);
//! 5. `--coverage phrases.txt` (one phrase per line, `#` comments): prints what each phrase does —
//!    HELD move (with `--hold`), KIT move, BORROW (a shout of a power you don't hold), CHAIN of
//!    steps, GRANT (a power / tier name), or FALLTHROUGH to a generic construct. The fallthrough
//!    list is how content workers find gaps.
//!
//! 6. SHOUT HYGIENE (needs assets/lexicon.bin): a shout that isn't PROPER (Title Case in the data,
//!    or containing a non-dictionary word) and that the lexicon reads as a clean ordinary thing is
//!    DEAD — saying it builds the thing ("flaming hammer"), never the move. `--hygiene` prints the
//!    whole sweep (dead shouts + shouts made only of everyday words, for review); `--raw` makes
//!    `--coverage` read without the hygiene (the old reading, for before / after counts).
//!
//! Exit code 1 if any given file fails to parse or has errors (warnings don't fail).

use std::path::{Path, PathBuf};

use bd_sim::powers::compose;
use bd_sim::powers::toolbox::{split_chain_with, KitMatch, MoveIndex, MoveRef, Reading};
use bd_sim::powers::{move_sel, norm_words, PlayerPower, PowerDef, PowerPhrase, PowerTable};

struct Args {
    files: Vec<PathBuf>,
    coverage: Option<PathBuf>,
    hold: Vec<String>,
    dir: PathBuf,
    all: bool,
    /// --raw: coverage WITHOUT shout hygiene (the old reading; before / after comparisons)
    raw: bool,
    /// --hygiene: print the full shout-hygiene sweep (dead shouts, common-word shouts)
    hygiene: bool,
}

fn parse_args() -> Args {
    let mut a = Args { files: vec![], coverage: None, hold: vec![], dir: PathBuf::new(), all: false, raw: false, hygiene: false };
    let mut it = std::env::args().skip(1);
    while let Some(s) = it.next() {
        match s.as_str() {
            "--coverage" => a.coverage = it.next().map(PathBuf::from),
            "--hold" => a.hold = it.next().map(|h| h.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default(),
            "--dir" => a.dir = it.next().map(PathBuf::from).unwrap_or_default(),
            "--all" => a.all = true,
            "--raw" => a.raw = true,
            "--hygiene" => a.hygiene = true,
            "-h" | "--help" => {
                println!("{}", include_str!("powers_check.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!")).collect::<Vec<_>>().join("\n"));
                std::process::exit(0);
            }
            _ => a.files.push(PathBuf::from(s)),
        }
    }
    if a.dir.as_os_str().is_empty() {
        a.dir = root().join("assets/powers");
    }
    a
}

fn root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    if cwd.join("assets/powers").exists() {
        return cwd;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fname(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn describe(t: &PowerTable, m: MoveRef) -> String {
    match m.resolve(t) {
        Some((def, mv)) => {
            let tier = if m.tier > 0 { def.tier(m.tier).map(|td| format!(" [{}]", td.name)).unwrap_or_default() } else { String::new() };
            format!("{}{} / \"{}\" (slot {})", def.id, tier, mv.name, m.slot)
        }
        None => format!("{m:?} (dangling)"),
    }
}

fn main() {
    let args = parse_args();
    // ---- the table: the directory, with the given files swapped in / added (filename order)
    let mut files: Vec<(String, String)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&args.dir) {
        let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "ron")).collect();
        paths.sort_by_key(|p| fname(p));
        for p in paths {
            files.push((fname(&p), std::fs::read_to_string(&p).unwrap_or_default()));
        }
    } else {
        eprintln!("warning: no directory {}", args.dir.display());
    }
    let mut mine: Vec<String> = Vec::new();
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for f in &args.files {
        let Ok(src) = std::fs::read_to_string(f) else {
            eprintln!("error: cannot read {}", f.display());
            errors += 1;
            continue;
        };
        let name = fname(f);
        match files.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = src,
            None => files.push((name.clone(), src)),
        }
        mine.push(name);
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let check_all = args.all || mine.is_empty();
    let is_mine = |file: &str| check_all || mine.iter().any(|m| file.ends_with(m.as_str()));

    // ---- 1. strict parse, per file (line / column)
    let mut owner: Vec<String> = Vec::new(); // table index → file
    for (name, src) in &files {
        match ron::from_str::<Vec<PowerDef>>(src) {
            Ok(list) => owner.extend(std::iter::repeat_n(name.clone(), list.len())),
            Err(e) => {
                if is_mine(name) {
                    println!("ERROR {name}: {e}");
                    errors += 1;
                } else {
                    println!("warning: {name} (not yours) fails to parse: {e}");
                    warnings += 1;
                }
            }
        }
    }
    let t0 = std::time::Instant::now();
    let (table, table_warn) = PowerTable::parse_files_lenient(&files);
    let t_parse = t0.elapsed();
    for w in table_warn.iter().filter(|w| w.contains("MAX_POWERS")) {
        // never silent: entries past the cap would be dropped
        println!("ERROR {w}");
        errors += 1;
    }
    let ix = MoveIndex::build(&table);
    let file_of = |id: u16| owner.get(id as usize).cloned().unwrap_or_default();

    // ---- 2. the lexicon's scan (the game's load warnings)
    let mut scanned = Vec::new();
    for (name, src) in &files {
        let (p, w) = bd_lexicon::power_scan::parse_powers_named(&format!("powers/{name}"), src);
        scanned.extend(p);
        for w in w {
            if is_mine(name) {
                println!("WARN lexicon: {w}");
                warnings += 1;
            }
        }
    }
    for w in bd_lexicon::power_scan::cross_file_collisions(&scanned) {
        if check_all || mine.iter().any(|m| w.contains(m.as_str())) {
            println!("WARN names: {w}");
            warnings += 1;
        }
    }

    // ---- 3. shout collisions (first claim wins in the table; later ones are dead)
    for (shout, kept, dropped) in &ix.clashes {
        let (fa, fb) = (file_of(kept.power), file_of(dropped.power));
        if is_mine(&fa) || is_mine(&fb) {
            let same_power = kept.power == dropped.power;
            if !same_power {
                println!("WARN shout \"{shout}\": {} ({fa}) keeps it; {} ({fb}) loses it", describe(&table, *kept), describe(&table, *dropped));
                warnings += 1;
            }
        }
    }
    // a shout that is also a power / tier NAME never borrows (saying the name grants)
    for (shout, m) in ix.shouts() {
        if ix.is_grant_name(shout) && is_mine(&file_of(m.power)) {
            let def = table.def(m.power);
            let own = def.is_some_and(|d| d.is_named(shout) || d.tier_named(shout).is_some());
            if !own {
                println!("WARN shout \"{shout}\" of {} is another power's NAME: saying it grants that power, never this move", describe(&table, m));
                warnings += 1;
            }
        }
    }
    // kit names that are grant names (the grant wins in the lexicon; the kit keeps "<name> <move>")
    for (k, pid) in ix.kit_names() {
        if ix.is_grant_name(k) && is_mine(&file_of(pid)) {
            println!("WARN kit name \"{k}\" ({}) is also a power's name: the bare name grants that power, not the costume", table.def(pid).map_or("?", |d| d.id.as_str()));
            warnings += 1;
        }
    }

    // ---- 4. lints
    let mut counts = (0usize, 0usize, 0usize, 0usize); // powers, kits, moves, shouts
    for (i, p) in table.powers.iter().enumerate() {
        let f = file_of(i as u16);
        if !is_mine(&f) {
            continue;
        }
        counts.0 += 1;
        counts.1 += p.kind.is_kit() as usize;
        counts.2 += p.moves.len() + p.tiers.iter().map(|t| t.moves.len()).sum::<usize>();
        counts.3 += p.shouts.len() + p.tiers.iter().map(|t| t.shouts.len()).sum::<usize>();
        let mut err = |m: String| {
            println!("ERROR {f} [{}]: {m}", p.id);
            errors += 1;
        };
        if p.kind.is_kit() && p.moves.is_empty() {
            err("a Kit needs moves".into());
        }
        for (s, slot) in &p.shouts {
            if *slot as usize >= p.moves.len() {
                err(format!("shout \"{s}\" names slot {slot} but there are {} moves", p.moves.len()));
            }
        }
        for td in &p.tiers {
            let n = if td.moves.is_empty() { p.moves.len() } else { td.moves.len() };
            for (s, slot) in &td.shouts {
                if *slot as usize >= n {
                    err(format!("tier {} shout \"{s}\" names slot {slot} but it has {n} moves", td.name));
                }
            }
        }
        for m in p.moves.iter().chain(p.tiers.iter().flat_map(|t| t.moves.iter())) {
            if m.ops.is_empty() {
                err(format!("move \"{}\" has no ops", m.name));
            }
            if !bd_sim::powers::toolbox::move_is_finite(m) {
                err(format!("move \"{}\" has a NaN / inf number in its ops", m.name));
            }
        }
        let mut warn = |m: String| {
            println!("WARN {f} [{}]: {m}", p.id);
            warnings += 1;
        };
        if p.kind.is_kit() && (p.transform.is_some() || p.companion.is_some() || !p.incantations.is_empty()) {
            warn("a Kit is never held: transform / companion / incantations do nothing".into());
        }
        // (AUTO-KITS' generated `dk_*` kits add nothing borrowable by design)
        if p.kind.is_kit() && p.shouts.is_empty() && !p.id.starts_with("dk_") {
            warn("no shouts: only \"<kit name> <move>\" reaches its moves (add each move's famous name as a shout to make it borrowable)".into());
        }
    }

    // ---- 6. shout hygiene (the lexicon's view: a dead shout builds a thing, never the move)
    let lex = load_lex(&files);
    if let Some(l) = &lex {
        let mut dead: Vec<(String, MoveRef, String)> = Vec::new();
        let mut plainish: Vec<(String, MoveRef, String)> = Vec::new();
        let mut coined: Vec<(String, MoveRef, String)> = Vec::new();
        let mut shouts: Vec<(&str, MoveRef)> = ix.shouts().collect();
        shouts.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(b.0)));
        let everyday = |w: &str| l.is_everyday_word(w);
        let thing = |s: &str| plain_thing(l, s);
        let icon = |s: &str| l.is_icon_name(s);
        let judge = bd_sim::powers::toolbox::LexJudge { thing: &thing, icon: &icon, everyday: &everyday };
        for (s, m) in shouts {
            if !is_mine(&file_of(m.power)) || ix.is_grant_name(s) {
                continue;
            }
            let res = l.resolve(s);
            if ix.shout_yields(s, &judge) {
                let why = if l.is_icon_name(s) { format!("icon {}", res.recipe.name) } else { res.recipe.name.clone() };
                dead.push((s.to_string(), m, why));
            } else if !s.contains(' ') && !ix.is_marked_shout(s) && plain_thing(l, s) {
                // one rare / coined word the lexicon reads as a THING ("surfboard", "minigun"):
                // it still borrows (coined words count as proper) — review it
                coined.push((s.to_string(), m, res.recipe.name.clone()));
            } else if !ix.is_proper_shout(s, &everyday) {
                plainish.push((s.to_string(), m, if bd_lexicon::Lexicon::is_fallback(&res) { "improvised".into() } else { format!("{:?} {:.2}", res.kind, res.confidence) }));
            }
        }
        // A KIT's dead shout is only a lint (the kit form "<kit> <shout>" still reads it): prefix it.
        // A held power's shout still works for its HOLDER (held priority): a note, not a warning.
        let kit_of = |m: &MoveRef| table.def(m.power).is_some_and(|d| d.kind.is_kit());
        let (dead_kit, dead_held): (Vec<_>, Vec<_>) = dead.iter().partition(|(_, m, _)| kit_of(m));
        for (s, m, thing) in &dead_kit {
            if args.hygiene || dead_kit.len() <= 60 {
                println!(
                    "WARN hygiene: shout \"{s}\" of {} ({}) reads as a plain thing (\"{thing}\"): saying it BUILDS that, never the move — prefix the character, or write it Title Case if it is the move's famous proper name",
                    describe(&table, *m),
                    file_of(m.power)
                );
            }
            warnings += 1;
        }
        if !args.hygiene && dead_kit.len() > 60 {
            println!("WARN hygiene: {} dead kit shouts (the first 60 above were not printed: run with --hygiene)", dead_kit.len());
        }
        for (s, m, thing) in &coined {
            println!(
                "WARN hygiene: one-word shout \"{s}\" of {} ({}) is also a THING the lexicon builds (\"{thing}\"): the move takes the word from everyone — if it names an object / creature, prefix the character; if it is the move's famous name, write it Title Case",
                describe(&table, *m),
                file_of(m.power)
            );
            warnings += 1;
        }
        if args.hygiene {
            for (s, m, thing) in &coined {
                let (id, mv) = m.resolve(&table).map(|(d, mv)| (d.id.clone(), mv.name.clone())).unwrap_or_default();
                println!("COINED\t{s}\t{id}\t{}\t{mv}\t{}\t{thing}\t1", kit_of(m) as u8, file_of(m.power));
            }
            for (s, m, thing) in &dead_held {
                println!("note hygiene: shout \"{s}\" of {} ({}) reads as a plain thing (\"{thing}\"): only its HOLDER fires it by saying it", describe(&table, *m), file_of(m.power));
            }
            for (s, m, thing) in &dead {
                let (id, mv) = m.resolve(&table).map(|(d, mv)| (d.id.clone(), mv.name.clone())).unwrap_or_default();
                println!("DEAD\t{s}\t{id}\t{}\t{mv}\t{}\t{thing}\t{}", kit_of(m) as u8, file_of(m.power), ix.is_proper_shout(s, &everyday) as u8);
            }
            println!("\n== shouts made only of everyday words (borrowable because the lexicon doesn't read them as one thing; review: generic phrases are global):");
            for (s, m, why) in &plainish {
                println!("  PLAINWORDS \"{s}\" → {} ({}) [{why}]", describe(&table, *m), file_of(m.power));
            }
            println!("hygiene: {} dead shout(s), {} everyday-word shout(s)", dead.len(), plainish.len());
        }
    }
    // the raw file may list more than 8 moves (sanitize truncated them): check the raw parse
    for (name, src) in &files {
        if !is_mine(name) {
            continue;
        }
        if let Ok(list) = ron::from_str::<Vec<PowerDef>>(src) {
            for p in list {
                if p.moves.len() > move_sel::MAX_MOVES {
                    println!("ERROR {name} [{}]: {} moves (max {})", p.id, p.moves.len(), move_sel::MAX_MOVES);
                    errors += 1;
                }
            }
        }
    }
    println!("(table: {} powers parsed in {:.0} ms)", table.powers.len(), t_parse.as_secs_f64() * 1e3);
    println!(
        "{} file(s) checked: {} powers ({} kits), {} moves, {} shouts; table {} powers, {} shouts indexed — {errors} error(s), {warnings} warning(s)",
        if check_all { files.len() } else { mine.len() },
        counts.0,
        counts.1,
        counts.2,
        counts.3,
        table.powers.len(),
        ix.shouts().count()
    );

    // ---- 5. coverage
    if let Some(cov) = &args.coverage {
        coverage(&table, &ix, lex.as_ref(), cov, &args.hold, args.raw);
    }
    std::process::exit(if errors > 0 { 1 } else { 0 });
}

/// The lexicon over THESE power files (assets/lexicon.bin + icons), if there is one.
fn load_lex(files: &[(String, String)]) -> Option<bd_lexicon::Lexicon> {
    let assets = root().join("assets");
    std::fs::read(assets.join("lexicon.bin")).ok().and_then(|bytes| {
        let pf: Vec<(String, String)> = files.iter().map(|(n, s)| (format!("powers/{n}"), s.clone())).collect();
        bd_lexicon::Lexicon::from_parts(bytes, &bd_lexicon::Lexicon::icon_files(&assets), &pf).ok()
    })
}

/// SHOUT HYGIENE — the same predicate as bd_game `toolbox::plain_thing`: a construct the lexicon
/// composed and fully understood (lower-case name: not a named entity / icon), not a grant, not a
/// signature.
fn plain_thing(l: &bd_lexicon::Lexicon, s: &str) -> bool {
    let r = l.resolve(s);
    let named = r.recipe.name.chars().any(|c| c.is_uppercase());
    !named && r.signature.is_none() && r.recipe.grants.is_none() && r.stand_rush.is_none() && bd_lexicon::Lexicon::is_clean_thing(&r)
}

/// The pretend match roster for --coverage (described moves may name a player).
const ROSTER: [(bd_core::PlayerId, &str); 4] = [(bd_core::PlayerId(1), "Ethan"), (bd_core::PlayerId(2), "Mia"), (bd_core::PlayerId(3), "Sam"), (bd_core::PlayerId(4), "Nova")];

fn coverage(table: &PowerTable, ix: &MoveIndex, lex: Option<&bd_lexicon::Lexicon>, path: &Path, hold: &[String], raw: bool) {
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("error: cannot read {}", path.display());
        return;
    };
    if lex.is_none() {
        println!("(no assets/lexicon.bin: non-move phrases show as PLAIN)");
    }
    // a pretend holder for --hold (held priority)
    let mut pw = PlayerPower::default();
    let ids: Vec<u16> = hold.iter().filter_map(|h| table.index_of(h)).collect();
    // `PlayerPower::held()` reads the shipped table: only meaningful when the ids agree
    let holds = !ids.is_empty();
    if holds {
        pw.id = Some(ids[0]);
        for (k, id) in ids.iter().skip(1).take(pw.others.len()).enumerate() {
            pw.others[k] = Some(bd_sim::powers::HeldPower { id: *id, ..Default::default() });
        }
    }
    let held = holds.then_some(&pw);
    let mut fall = Vec::new();
    let mut n = 0;
    let (mut moves, mut things) = (0usize, 0usize);
    // SHOUT HYGIENE, the console's judge (bd_game toolbox::read_step)
    let thing = |s: &str| lex.is_some_and(|l| plain_thing(l, s));
    let icon = |s: &str| lex.is_some_and(|l| l.is_icon_name(s));
    let everyday = |w: &str| lex.is_none_or(|l| l.is_everyday_word(w));
    let judge = bd_sim::powers::toolbox::LexJudge { thing: &thing, icon: &icon, everyday: &everyday };
    println!("\n== coverage: {}", path.display());
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        n += 1;
        // the console's reading: a phrase the lexicon resolves to a GRANT never borrows; weak
        // separators split only what the lexicon doesn't read as one thing
        let read = |s: &str| match if raw { ix.read(table, held, s) } else { ix.read_judged(table, held, s, &judge) } {
            Reading::Borrow(_) if lex.is_some_and(|l| l.resolve(s).recipe.grants.is_some()) => Reading::Plain,
            r => r,
        };
        let understood = |s: &str| lex.is_some_and(|l| bd_lexicon::Lexicon::is_clean_thing(&l.resolve(s)));
        // VERB GRAMMAR (docs/toolbox/COMPOSED_MOVE.md): a described move, unless the lexicon knows
        // the exact phrase as something curated (the console's rule, bd_game toolbox::composed)
        let composed = |s: &str| {
            let r = compose::read(s, &compose::Names::new(&ROSTER))?;
            let curated = lex.as_ref().is_some_and(|l| {
                let res = l.resolve(s);
                compose::curated_by_lexicon(s, res.signature.is_some(), res.recipe.grants.is_some() || res.stand_rush.is_some(), &res.recipe.model)
            });
            (!curated).then_some(r)
        };
        let steps = split_chain_with(line, &|s| read(s).is_move() || composed(s).is_some(), lex.as_ref().map(|_| &understood as &dyn Fn(&str) -> bool));
        let mut out = Vec::new();
        for s in &steps {
            let mut fell = false;
            let r = read(s);
            out.push(match r {
                Reading::Held(PowerPhrase::Move(sel)) => format!("HELD move sel {sel:#x}"),
                Reading::Held(PowerPhrase::Incantation { rest }) => format!("HELD incantation + \"{rest}\""),
                Reading::Held(PowerPhrase::Plain) | Reading::Plain if composed(s).is_some() => {
                    let r = composed(s).expect("checked");
                    let ops: Vec<String> = r.spec.expand().map(|m| m.ops.iter().map(|o| format!("{o:?}").split([' ', '{', '(']).next().unwrap_or("").to_string()).collect()).unwrap_or_default();
                    format!("COMPOSED {} → [{}]", r.describe(), ops.join(", "))
                }
                Reading::Held(PowerPhrase::Plain) | Reading::Plain => match &lex {
                    Some(l) => {
                        let res = l.resolve(s);
                        match &res.recipe.grants {
                            Some(g) => format!("GRANT {g}"),
                            None => {
                                fell = true;
                                let fb = if bd_lexicon::Lexicon::is_fallback(&res) { " (improvised)" } else { "" };
                                let op = res.recipe.opening.map(|m| format!(" opening {m:?}")).unwrap_or_default();
                                format!("FALLTHROUGH construct \"{}\" ({:?}{op}){fb}", res.recipe.name, res.recipe.form)
                            }
                        }
                    }
                    None => {
                        fell = true;
                        "PLAIN".to_string()
                    }
                },
                Reading::Kit(m) => {
                    let why = match ix.kit_move(table, &norm_words(s)).map(|x| x.1) {
                        Some(KitMatch::Exact) => "exact".to_string(),
                        Some(KitMatch::Tag(t)) => format!("tag {t:?}"),
                        Some(KitMatch::TagFallback(t)) => format!("{t:?} by shape"),
                        Some(KitMatch::Fuzzy) => "words".to_string(),
                        None => String::new(),
                    };
                    format!("KIT {} ({why})", describe(table, m))
                }
                Reading::Borrow(m) => format!("BORROW {}", describe(table, m)),
            });
            if fell {
                fall.push(if steps.len() > 1 { format!("{s}    (step of: {line})") } else { s.clone() });
            }
        }
        if steps.len() == 1 && read(&steps[0]).is_move() {
            moves += 1;
        } else if steps.len() == 1 && composed(&steps[0]).is_none() {
            things += 1;
        }
        if steps.len() > 1 {
            println!("{line:<48} → CHAIN {}", steps.iter().zip(&out).map(|(s, o)| format!("[{s}: {o}]")).collect::<Vec<_>>().join(" → "));
        } else {
            println!("{line:<48} → {}", out.first().cloned().unwrap_or_default());
        }
    }
    println!(
        "\nSUMMARY{}: {n} phrase(s): {moves} read as ONE power move, {things} as a thing (construct / grant / summon), {} other (chains, described moves)",
        if raw { " (raw, no shout hygiene)" } else { "" },
        n - moves - things
    );
    println!("\n{} phrase(s); {} step(s) fall through to a generic construct (no power move):", n, fall.len());
    for f in &fall {
        println!("  {f}");
    }
}
