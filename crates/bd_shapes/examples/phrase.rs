//! PHRASE PREVIEW — the owner's dev tool: type a phrase, see what the ring would make of it, in plain
//! English plus a picture. Not shipped; no in-game UI.
//!
//!     phrase_preview.cmd                                   (repo root: builds once, then runs this)
//!     cargo run --release -p bd_shapes --example phrase -- [--spoken] [--no-open] [--out-dir DIR] [PHRASE ...]
//!
//! With no PHRASE it is an interactive loop (`phrase>`); `:voice` / `:typed` switch between the
//! spoken path (sound-alike names, Whisper-style repair) and typed input, `:open` toggles opening the
//! picture, `:help`, `:q`. With PHRASEs it previews each one and exits (`--out-dir` also keeps a
//! `<phrase>.png` per phrase there).
//!
//! It mirrors the game's own pipeline, in the game's order, with the shipped data:
//! `Lexicon::intent` (refer → target → delivery verb → resolver, the same call `lobby/feed.rs`
//! makes, with a sample roster of other players) → the client's costume summon hook
//! (`bd_shapes::costume::summon::apply`, console.rs) → the sim's pure rules: named delivery
//! (`bd_sim::freedom::deliver`), recipe validation / clamps (`willpower::validate_recipe`, default
//! match rules), the signature amplifier (`signature::amplify`, mid shout power), cast cost
//! (`assets/tuning/combat.ron`), the DANGER alert (`signature::spectacle`) and the power-ups gates
//! (`pickups`) and the presence tier (`bd_sim::presence_tier`, knobs from `assets/tuning/presence.ron`).
//! Power moves / incantations are shown for "if you held that power". TOOLBOX lines (a kit move —
//! "reinhardt charge" —, a borrowed shout of any power, or a CHAIN "a -> b", "x then y", "a, b")
//! print each step's reading (`bd_sim::powers::toolbox`, the same function the console uses), the
//! move's presence tier + sparks (`powers::chain::move_sparks`) and its active phase; construct
//! steps of a chain are previewed like single phrases. Costs are SPARKS by presence tier
//! (`presence_tier::cast_tier` + `willpower::cast_sparks`, the sim's own price).
//!
//! The picture: the construct built like the client builds it (curated icon shape, costume summon or
//! procedural model), 3/4 view, painted costume colours for character summons and corps green
//! otherwise → `target/phrase_preview/last.png`, opened with the default viewer (`--no-open` skips).

use std::io::{BufRead, Write};
use std::path::PathBuf;

use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe, Temperament};
use bd_core::shape::ShapeSpec;
use bd_lexicon::{Delivery, Intent, Lexicon, Refer, ResolutionKind, SignatureKind};
use bd_shapes::costume;
use bd_sim::MatchRules;
use bd_sim::freedom::deliver;
use bd_sim::pickups::{self, GateKind};
use bd_sim::presence_tier::{self, Tier, TierKnobs};
use bd_sim::signature::{self as sig, SignatureCast};
use bd_sim::tuning::CombatTuning;

#[path = "raster/mod.rs"]
mod raster;
use raster::{Canvas, Dressed, text, text_w};

/// The other players in the preview match (targets are matched against these names).
const ROSTER: [&str; 4] = ["Ethan", "Mia", "Sam", "Nova"];
/// Shout power assumed for a signature (0 = whispered … 1 = screamed; typed CAPS / "!!!" raise it).
const SHOUT_POWER: f32 = 0.5;
/// `bd_fx::SHOUT_MAX_CHARS` (the title letters' length; bd_fx is a Bevy crate).
const SHOUT_MAX_CHARS: usize = 48;

struct Tool {
    lex: Lexicon,
    tuning: CombatTuning,
    /// `assets/tuning/presence.ron` → `tier` (defaults if missing).
    tier_knobs: TierKnobs,
    rules: MatchRules,
    root: PathBuf,
    spoken: bool,
    open: bool,
    out_dir: Option<PathBuf>,
    /// The last phrase previewed ("again!" repeats it, like the game's cast memory).
    last: Option<String>,
}

fn find_root() -> PathBuf {
    if let Ok(r) = std::env::var("BD_ROOT") {
        return PathBuf::from(r);
    }
    // the launcher runs from the repo root; `cargo run` from anywhere falls back to the source tree
    let cwd = std::env::current_dir().unwrap_or_default();
    if cwd.join("assets/lexicon.bin").exists() {
        return cwd;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The presence tier knobs from `presence.ron` (`tier: (...)`; the rest of the file is bd_game's).
fn load_tier_knobs(path: &std::path::Path) -> TierKnobs {
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct File {
        tier: TierKnobs,
    }
    std::fs::read_to_string(path).ok().and_then(|s| ron::from_str::<File>(&s).ok()).map(|f| f.tier).unwrap_or_default()
}

fn tier_words(t: Tier) -> &'static str {
    match t {
        Tier::Tiny => "a crisp flick (guns, gauntlets, discs, bugs)",
        Tier::Normal => "a clean gather + \"shing\" (swords, shields, walls, balls)",
        Tier::Big => "the world notices (dragons, anvils from the sky, big hammers, vehicles)",
        Tier::Titan => "near-Hollow-Purple treatment (kaiju, giant mechs, meteors, legendaries)",
        Tier::Signature => "the top of the scale (domains, erasing orbs, the priciest spectacles)",
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut spoken = false;
    let mut open = true;
    let mut out_dir = None;
    let mut phrases = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--spoken" | "--voice" => spoken = true,
            "--no-open" => open = false,
            "--out-dir" => out_dir = it.next().map(PathBuf::from),
            "--help" | "-h" => {
                help();
                return;
            }
            _ => phrases.push(a),
        }
    }
    let root = find_root();
    let bin = root.join("assets/lexicon.bin");
    let lex = match Lexicon::load(&bin) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot load the lexicon at {}: {e}", bin.display());
            std::process::exit(1);
        }
    };
    let tuning = CombatTuning::load(&root.join("assets/tuning/combat.ron")).unwrap_or_default();
    let tier_knobs = load_tier_knobs(&root.join("assets/tuning/presence.ron"));
    let mut tool = Tool { lex, tuning, tier_knobs, rules: MatchRules::default(), root, spoken, open, out_dir, last: None };
    if !phrases.is_empty() {
        for p in phrases {
            tool.preview(&p);
        }
        return;
    }
    println!("PHRASE PREVIEW: type a phrase to see what the ring would make of it (:help for commands).");
    println!("Other players in this preview match: {}.", ROSTER.join(", "));
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        print!("{}phrase> ", if tool.spoken { "[voice] " } else { "" });
        let _ = std::io::stdout().flush();
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let l = line.trim();
        match l {
            "" => continue,
            ":q" | ":quit" | ":exit" | "quit" | "exit" => break,
            ":help" | ":h" | "help" | "?" => help(),
            ":voice" | ":spoken" => {
                tool.spoken = true;
                println!("  voice mode: phrases go through the spoken path (sound-alike names, misheard words).");
            }
            ":typed" | ":type" => {
                tool.spoken = false;
                println!("  typed mode.");
            }
            ":open" => {
                tool.open = !tool.open;
                println!("  open the picture after each phrase: {}", if tool.open { "on" } else { "off" });
            }
            _ if l.starts_with(':') => println!("  unknown command {l} (:help)"),
            _ => tool.preview(l),
        }
    }
}

fn help() {
    println!(
        "  Type anything a player might type or say: \"reinhardt charge\", \"drop a meteor on ethan\", \"gojo but bald\".
  You get: what it becomes, form, behaviours, element, size, delivery / target, signature, DANGER,
  power-ups gating, confidence and why, plus a picture (target\\phrase_preview\\last.png).
  Players you can target: {}.
  :voice   read phrases as SPOKEN (sound-alike names: \"eaton\" -> Ethan, misheard words)
  :typed   read phrases as typed (default)
  :open    toggle opening the picture after each phrase
  :help    this text
  :q       quit
  Flags: --spoken  --no-open  --out-dir DIR (also keep DIR\\<phrase>.png)",
        ROSTER.join(", ")
    );
}

// ------------------------------------------------------------------------------------ the pipeline

fn sim_delivery(d: Delivery) -> deliver::Delivery {
    match d {
        Delivery::Onto => deliver::Delivery::Onto,
        Delivery::At => deliver::Delivery::At,
        Delivery::Near => deliver::Delivery::Near,
        Delivery::Around => deliver::Delivery::Around,
        Delivery::Hunt => deliver::Delivery::Hunt,
    }
}

fn sim_kind(k: SignatureKind) -> sig::SignatureKind {
    match k {
        SignatureKind::Domain => sig::SignatureKind::Domain,
        SignatureKind::Beam => sig::SignatureKind::Beam,
        SignatureKind::Transformation => sig::SignatureKind::Transformation,
        SignatureKind::Summon => sig::SignatureKind::Summon,
    }
}

/// Everything the preview found out about one phrase.
struct Reading {
    it: Intent,
    /// The phrase actually read ("again!" → the repeated one).
    phrase: String,
    repeated: bool,
    costume: bool,
    /// The bare character the costume summon's mechanics were read from ("gojo but bald" → "gojo").
    bare: Option<String>,
    /// What the sim would spawn (after the costume hook, delivery, validation and the amplifier).
    recipe: Recipe,
    shape: Option<ShapeSpec>,
    cost: f32,
    /// SPARKS price (`willpower::cast_sparks` at `presence_tier::cast_tier`, no repeats).
    sparks: u32,
    danger: bool,
    /// Presence tier (how much gravitas the client gives it).
    tier: Tier,
    gates: Vec<String>,
    powers: Vec<String>,
}

impl Tool {
    fn read(&self, text: &str) -> Reading {
        let mut phrase = text.to_string();
        let mut it = self.lex.intent(text, &ROSTER, self.spoken);
        let mut repeated = false;
        let mut count = None;
        if let Some(Refer::Again { count: c }) = it.refer
            && let Some(l) = &self.last
        {
            // the game re-runs the stages on the last phrase ("missile at jake" … "again!" → Jake again)
            phrase = l.clone();
            it = self.lex.intent(l, &ROSTER, self.spoken);
            repeated = true;
            count = c;
        }
        let res = &it.resolution;
        let modifier = res.kind == ResolutionKind::Graft;
        let summon = res.signature.is_none() && !modifier && res.stand_rush.is_none();
        // COSTUME HOOK (console.rs): costume words ("gojo but bald") dress the summon only; its
        // mechanics are read from the bare character ("gojo")
        let bare = if summon { costume::summon::mechanics_phrase(&it.text, &res.recipe) } else { None };
        let (mut recipe, mut shape) = match &bare {
            Some(b) => {
                let b = self.lex.resolve(b);
                (b.recipe, b.shape)
            }
            None => (res.recipe.clone(), res.shape.clone()),
        };
        if let Some(n) = count {
            recipe.count = n.max(1);
        }
        // COSTUME HOOK (console.rs): a plain summon naming a character wears that character's costume
        let costume = summon && costume::summon::apply(&it.text, &mut recipe, &mut shape);
        // FREEDOM HOOK: a named player is delivered to (an object "onto" someone falls from the sky)
        if it.target.is_some() {
            deliver::prepare(&mut recipe, sim_delivery(it.delivery.unwrap_or(Delivery::Hunt)));
        }
        let t = &self.tuning;
        let mut r = bd_sim::willpower::validate_recipe(recipe, &self.rules);
        let signature = res.signature.as_ref().map(|s| sim_kind(s.kind));
        if let Some(k) = signature {
            r = sig::amplify(r, SignatureCast::new(k, SHOUT_POWER), &t.signature, &self.rules, false);
            if k == sig::SignatureKind::Domain || sig::orb::is_erasing_orb(&r) {
                shape = None;
            }
        }
        let pool = self.rules.willpower_max();
        let mut cost = t.willpower.base_cost(&r);
        if sig::is_orbital(&r) {
            cost = (cost * t.signature.orbital.cost_mult).min(pool);
        }
        if signature.is_some() {
            cost = sig::signature_cost(cost, &t.signature, pool);
        }
        let spec = sig::spectacle(&r, cost, 0.0, &t.signature);
        let danger = res.stand_rush.is_none() && !modifier && spec.is_some();
        // PRESENCE TIER + SPARKS: the sim's own model (`World::cast_price`): the tier of the cast's
        // presence weight (its plain cost × the orbital / signature multipliers) prices it.
        let mut weight = t.willpower.base_cost_unadjusted(&r);
        if sig::is_orbital(&r) {
            weight = (weight * t.signature.orbital.cost_mult).min(pool);
        }
        if signature.is_some() {
            weight = sig::signature_cost(weight, &t.signature, pool);
        }
        let tier = presence_tier::cast_tier(&r, weight, false, signature.is_some(), &self.tier_knobs);
        let famous = bd_sim::constructs::legend::famous(&r);
        let sparks = bd_sim::willpower::cast_sparks(&t.willpower.sparks, tier.index(), &r, famous, t.willpower.is_autonomous(&r), 0, self.rules.pool_segments(), 1.0);
        let cost = sparks as f32 * self.rules.segment_size();
        let gates = self.gates(&r, signature, res.recipe.grants.as_deref());
        let powers = self.powers(&phrase, res);
        Reading { phrase, repeated, costume, bare, recipe: r, shape, cost, sparks, danger, tier, gates, powers, it }
    }

    /// TOOLBOX: a kit move, a borrowed shout or a chain → (per step: its phrase and, for a move, a
    /// description). `None` = not a toolbox line (a plain phrase / a grant / an incantation).
    fn toolbox(&self, text: &str) -> Option<Vec<(String, Option<String>)>> {
        use bd_sim::powers::toolbox::{split_chain_with, MoveIndex, Reading as R};
        let t = bd_sim::powers::PowerTable::get();
        let ix = MoveIndex::shipped();
        // the console's grant filter: a phrase the lexicon resolves to a grant never borrows
        let read = |s: &str| match ix.read(t, None, s) {
            R::Borrow(_) if self.lex.resolve(s).recipe.grants.is_some() => R::Plain,
            r => r,
        };
        let understood = |s: &str| !Lexicon::is_fallback(&self.lex.resolve(s));
        let parts = split_chain_with(text, &|s| read(s).is_move(), Some(&understood));
        if parts.len() == 1 && !read(&parts[0]).is_move() {
            return None;
        }
        let sparks = &self.tuning.willpower.sparks;
        let bar = self.rules.pool_segments();
        Some(
            parts
                .into_iter()
                .map(|p| {
                    let d = match read(&p) {
                        R::Kit(m) | R::Borrow(m) => m.resolve(t).map(|(def, mv)| {
                            let how = if def.kind.is_kit() { format!("KIT move of {}", def.name) } else { format!("BORROWED from {} (you don't hold it: a one-shot, never the power)", def.name) };
                            let tier = if m.tier > 0 { def.tier(m.tier).map(|td| format!(" [{}]", td.name)).unwrap_or_default() } else { String::new() };
                            let price = match bd_sim::powers::chain::move_tier(mv) {
                                Some(tr) => format!("{tr:?} tier, {} spark(s) (+1 per repeat inside the staleness window)", bd_sim::powers::chain::move_sparks(sparks, mv, 0, bar)),
                                None => "priced by its cast(s)".into(),
                            };
                            let (secs, land) = bd_sim::powers::chain::active_phase(mv);
                            let tags = if mv.tags.is_empty() { String::new() } else { format!(" tags {:?}", mv.tags) };
                            format!(
                                "{how}{tier}: \"{}\"{tags} - {} - active {:.2} s{} - ops {}",
                                mv.name,
                                price,
                                secs,
                                if land { " (until you land)" } else { "" },
                                mv.ops.iter().map(|o| format!("{:?}", bd_sim::powers::chain::op_kind(o))).collect::<Vec<_>>().join(" + ")
                            )
                        }),
                        R::Held(_) => Some("a move of a power you hold (held priority)".into()),
                        R::Plain => None,
                    };
                    (p, d)
                })
                .collect(),
        )
    }

    /// POWER-UPS mode (`MatchRules::powerups`): which pickup this needs, and what happens without it.
    fn gates(&self, r: &Recipe, signature: Option<sig::SignatureKind>, grants: Option<&str>) -> Vec<String> {
        let p = &self.tuning.pickups;
        let mut out = Vec::new();
        if p.gates(GateKind::Giant) && pickups::is_giant_recipe(r, p) {
            out.push(format!("GIANT pickup (heart / egg) - without it the giant comes out person-sized (~{:.1} m)", p.giant_weak_scale));
        }
        match signature {
            Some(k) if p.gates(GateKind::Ultimate) && p.ultimates.contains(&k) => {
                out.push("ULTIMATE pickup (crackling star) - without it the plain phrase is cast instead of the ultimate".into());
            }
            None if p.gates(GateKind::Ultimate) && pickups::is_ultimate_recipe(r, p) => {
                out.push("ULTIMATE pickup (crackling star) - without it the sky drop / strike comes out fist-sized".into());
            }
            _ => {}
        }
        if let Some(key) = grants
            && p.gates(GateKind::Awakening)
        {
            let (id, tier) = bd_sim::powers::split_tier(key);
            let table = bd_sim::powers::PowerTable::get();
            let def = table.index_of(id).and_then(|i| table.def(i));
            if tier.is_some_and(|t| t >= p.awakening_min_tier) {
                out.push(format!("AWAKENING pickup (ascend orb) for tier {} - without it the plain \"power up\" flare (a holder: the highest free tier)", tier.unwrap_or(0)));
            } else if def.is_some_and(|d| d.transform.is_some()) {
                out.push("AWAKENING pickup (ascend orb) to transform - without it just the \"power up\" flare".into());
            }
        }
        out
    }

    /// Power phrases: what it grants, an incantation, and "if you held <power>" moves.
    fn powers(&self, phrase: &str, res: &bd_lexicon::Resolution) -> Vec<String> {
        let table = bd_sim::powers::PowerTable::get();
        let mut out = Vec::new();
        if let Some(key) = res.recipe.grants.as_deref() {
            let (id, tier) = bd_sim::powers::split_tier(key);
            match table.index_of(id).and_then(|i| table.def(i)) {
                Some(d) => {
                    let how = if tier.is_some() {
                        "a FORM: saying it makes YOU this form on the spot (the power comes with it), with the Ascend moment - the awakening the first time per round"
                    } else if d.kind.is_technique() {
                        "a TECHNIQUE: saying it makes the power yours on the spot"
                    } else if d.kind.is_stand() {
                        "a STAND: called to your side"
                    } else if d.kind.is_pickup() {
                        "a POWER PICKUP: grab / eat the construct to gain the power"
                    } else {
                        "a power"
                    };
                    let tier = tier.map(|t| format!(" (tier {t})")).unwrap_or_default();
                    out.push(format!("grants {}{tier} - {how}", d.name));
                }
                None => out.push(format!("grants {key}")),
            }
        }
        if let Some(p) = &res.incantation {
            let names: Vec<String> = res.incantation_powers.iter().map(|id| power_name(table, id)).collect();
            out.push(format!("starts with {}'s incantation: if you hold it, what follows is cast with that power's delivery", if names.is_empty() { power_name(table, p) } else { names.join(" / ") }));
        }
        for (i, d) in table.powers.iter().enumerate() {
            if let bd_sim::powers::PowerPhrase::Move(slot) = bd_sim::powers::parse_phrase(Some(i as u16), phrase) {
                let mv = d.moves.get(slot as usize).map(|m| format!(" \"{}\"", m.name)).unwrap_or_default();
                out.push(format!("if you hold {}: this is its move{mv} (slot {slot}), not a construct", d.name));
            }
        }
        out
    }

    fn preview(&mut self, text: &str) {
        // TOOLBOX: kit / borrowed moves and chains print their steps; construct steps preview below.
        if let Some(steps) = self.toolbox(text) {
            let chain = steps.len() > 1;
            println!("  {:<11} {}", if chain { "CHAIN" } else { "TOOLBOX" }, if chain { format!("{} steps, each fires when the previous move's active phase ends, re-aimed at the crosshair, paid as it fires", steps.len()) } else { text.to_string() });
            let mut casts = Vec::new();
            for (k, (p, d)) in steps.iter().enumerate() {
                match d {
                    Some(d) => println!("  {:<11} {}. \"{p}\" → {d}", "", k + 1),
                    None => {
                        println!("  {:<11} {}. \"{p}\" → a construct (previewed below)", "", k + 1);
                        casts.push(p.clone());
                    }
                }
            }
            println!();
            for c in casts {
                self.preview(&c);
            }
            return;
        }
        let rd = self.read(text);
        report(&rd, self.spoken);
        match self.picture(&rd) {
            Ok(paths) => {
                println!("  PICTURE     {}", paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("  +  "));
                if self.open
                    && let Some(p) = paths.first()
                {
                    let _ = std::process::Command::new("cmd").args(["/c", "start", ""]).arg(p).spawn();
                }
            }
            Err(e) => println!("  PICTURE     (not written: {e})"),
        }
        println!();
        if !matches!(rd.it.refer, Some(Refer::Again { .. })) || rd.repeated {
            self.last = Some(rd.phrase.clone());
        }
    }

    // -------------------------------------------------------------------------------- the picture

    fn picture(&self, rd: &Reading) -> Result<Vec<PathBuf>, String> {
        let r = &rd.recipe;
        let res = &rd.it.resolution;
        let model = || match &rd.shape {
            Some(s) => bd_shapes::build_icon_model(r, s).1,
            None => bd_shapes::build_model(r).1,
        };
        let d = if rd.costume {
            rd.shape.as_ref().and_then(|s| raster::summon_dressed(String::new(), String::new(), r, s))
        } else {
            None
        };
        let d = match d {
            Some(d) => d,
            None => {
                let mut m = model();
                if r.has(Mechanic::Transforms) {
                    // the client shows a transformer in its alt mode (present/constructs.rs)
                    m = match (&rd.shape, &res.alt_shape) {
                        (Some(s), Some(a)) => bd_shapes::transform_rig_icon(r, s, a).alt,
                        _ => bd_shapes::transform_rig(&m, bd_sim::rng::word_hash(&r.name) as u32).alt,
                    };
                }
                Dressed::new(String::new(), String::new(), m, None, 0.0)
            }
        };
        let (w, h) = (640i32, 720i32);
        let mut c = Canvas::new(w as usize, h as usize);
        let head = 64;
        let foot = 120;
        c.rect(0, head, w, h - head - foot, [24, 27, 35]);
        let eye = if rd.costume { raster::THREE_Q } else { raster::eye_for(r.form) };
        raster::draw_fit(&mut c, &d, eye, 0, head, w, h - head - foot, 28.0);
        // header: the phrase, then what it became
        let said = clip(&format!("\"{}\"", rd.phrase), 50);
        text(&mut c, (w - text_w(&said, 2)) / 2, 10, &said, [235, 238, 244], 2);
        let title = clip(&becomes_short(rd), 50);
        text(&mut c, (w - text_w(&title, 2)) / 2, 36, &title, [120, 230, 150], 2);
        // footer: the facts
        let mut y = h - foot + 10;
        let mut line = |c: &mut Canvas, s: &str, col: [u8; 3]| {
            let s = clip(s, 100);
            text(c, 12, y, &s, col, 1);
            y += 13;
        };
        let cat = format!("{:?}", r.form.category()).to_uppercase();
        line(&mut c, &format!("{} ({cat})  {}  {}  X{}  {}", form_label(r.form), metres(r.scale), kg(r.mass), r.count, element(r.element)), [220, 225, 232]);
        let mechs: Vec<&str> = r.mechanics.iter().map(|m| bd_lexicon::tables::mechanic_name(*m)).collect();
        line(&mut c, &format!("DOES: {}", if mechs.is_empty() { "NOTHING SPECIAL".to_string() } else { mechs.join(" ") }), [200, 210, 220]);
        line(&mut c, &delivery_line(rd), [200, 210, 220]);
        if let Some(s) = &res.signature {
            line(&mut c, &format!("SIGNATURE {:?}: {}", s.kind, shout(&s.title)), [255, 214, 90]);
        }
        if rd.danger {
            line(&mut c, "DANGER ALERT FOR EVERYONE NEARBY", [255, 90, 80]);
        }
        if !rd.gates.is_empty() {
            line(&mut c, &format!("POWER-UPS MODE: NEEDS {}", rd.gates.iter().map(|g| g.split(" pickup").next().unwrap_or("")).collect::<Vec<_>>().join(" + ")), [200, 160, 255]);
        }
        line(&mut c, &format!("CONFIDENCE {:.0}%  COST {} SPARKS  TIER {:?}", res.confidence * 100.0, rd.sparks, rd.tier).to_uppercase(), [130, 140, 155]);
        let mut paths = vec![self.root.join("target").join("phrase_preview").join("last.png")];
        if let Some(dir) = &self.out_dir {
            paths.push(dir.join(format!("{}.png", slug(&rd.phrase))));
        }
        for p in &paths {
            c.write_png(p)?;
        }
        Ok(paths)
    }
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_').to_string();
    if out.is_empty() { "phrase".into() } else { out.chars().take(60).collect() }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}...", s.chars().take(n - 3).collect::<String>()) }
}

// ------------------------------------------------------------------------------------ the readout

fn power_name(table: &bd_sim::powers::PowerTable, id: &str) -> String {
    table.index_of(id).and_then(|i| table.def(i)).map_or_else(|| id.to_string(), |d| d.name.clone())
}

/// "Final Flash" → "FINAL FLASH!!" (console.rs `signature_shout`).
fn shout(title: &str) -> String {
    let t: String = title.trim().to_uppercase().chars().take(SHOUT_MAX_CHARS - 2).collect();
    format!("{}!!", t.trim_end())
}

fn metres(m: f32) -> String {
    if m < 1.0 { format!("{:.0} CM", m * 100.0) } else { format!("{m:.1} M") }
}

fn kg(m: f32) -> String {
    if m >= 1000.0 { format!("{:.1} T", m / 1000.0) } else { format!("{m:.0} KG") }
}

fn element(e: bd_core::mechanics::Element) -> String {
    bd_lexicon::tables::element_name(e).to_uppercase()
}

fn form_label(f: Form) -> String {
    bd_lexicon::tables::form_name(f).to_uppercase()
}

fn category_words(c: FormCategory) -> &'static str {
    match c {
        FormCategory::Held => "held (it appears in your hand)",
        FormCategory::Worn => "worn (it goes on you)",
        FormCategory::Creature => "creature (it acts on its own)",
        FormCategory::Structure => "structure (it stands where you aim)",
        FormCategory::Object => "object (it is thrown / dropped / placed)",
    }
}

/// Plain-English behaviour of each mechanic (bd_core `Mechanic` docs, shortened).
fn mechanic_words(m: Mechanic) -> &'static str {
    use Mechanic::*;
    match m {
        Blunt => "hits hard: big knockback, craters the ground",
        Blade => "cuts: fast, slices through constructs",
        Pierce => "pierces: lunges, long reach, goes through shields",
        Guard => "blocks hits from the front",
        Returning => "comes back when thrown",
        Ricochet => "bounces between surfaces / targets when thrown",
        Shoots => "fires projectiles",
        Beam => "fires a continuous beam",
        Explodes => "explodes: crater + knockback all round",
        Grab => "grabs someone and can throw them",
        Pull => "latches on and yanks toward its owner",
        SlowFall => "slows falls right down",
        Glide => "turns falling into gliding",
        Thrust => "gives thrust (jetpack / rocket)",
        Bounce => "bounces anything that lands on it",
        Launch => "launches whoever steps on it",
        Barrier => "blocks movement and shots, takes hits for you",
        Trap => "encloses and holds a target",
        Homing => "seeks its target",
        ArticulatedTail => "sweeps its tail",
        Flies => "flies",
        Charge => "charges in a line and rams",
        Stomp => "stomps: a ground shockwave",
        Breath => "breathes its element in a cone",
        Swarm => "comes as a swarm of small copies",
        Spin => "spins: continuous contact damage",
        DropFromSky => "drops from the sky onto the target",
        Sticky => "sticks to what it hits",
        Fast => "very fast",
        Heavy => "very heavy: more impact, slower",
        Constrict => "wraps around and squeezes",
        Walks => "grows legs and walks after its target",
        Piloted => "you ride / pilot it",
        Portal => "makes a linked portal pair",
        Attract => "pulls players and things toward it",
        Repel => "pushes everything away",
        Shrink => "shrinks what it hits",
        Grow => "grows what it hits",
        Link => "chains what it hits to the next thing",
        Transforms => "transforms between two modes",
        Extends => "telescopes: extends and snaps back",
        #[allow(unreachable_patterns)]
        _ => "",
    }
}

fn temperament_words(t: Temperament) -> &'static str {
    match t {
        Temperament::Calm => "calm",
        Temperament::Aggressive => "aggressive (attacks readily)",
        Temperament::Fearful => "fearful (keeps its distance)",
        Temperament::Protective => "protective (guards you)",
        Temperament::Playful => "playful",
        #[allow(unreachable_patterns)]
        _ => "",
    }
}

/// The short title for the picture ("REINHARDT - CREATURE, NAMED MOVE").
fn becomes_short(rd: &Reading) -> String {
    let (kind, _) = becomes(rd);
    let name = display_name(rd);
    format!("{} - {}", name.to_uppercase(), kind.to_uppercase())
}

fn display_name(rd: &Reading) -> String {
    match &rd.it.resolution.signature {
        Some(s) if !s.title.trim().is_empty() => s.title.clone(),
        _ => rd.recipe.name.clone(),
    }
}

/// (what kind of thing it becomes, a sentence about it).
fn becomes(rd: &Reading) -> (String, String) {
    let res = &rd.it.resolution;
    let named_move = res.explanation.iter().any(|e| e.contains("doing its move"));
    let mv = if named_move { ", doing a named move" } else { "" };
    if let Some(n) = res.stand_rush {
        return ("stand rush".into(), format!("a stand rush of {n} punch{} (drives your stand if you have one)", if n == 1 { "" } else { "es" }));
    }
    if let Some(s) = &res.signature {
        return (
            "signature move".into(),
            format!("a SIGNATURE {:?} (an amplified ultimate of what the words build; sent as a signature cast)", s.kind),
        );
    }
    if let Some(g) = &res.recipe.grants {
        let table = bd_sim::powers::PowerTable::get();
        let (id, _) = bd_sim::powers::split_tier(g);
        let tech = table.index_of(id).and_then(|i| table.def(i)).is_some_and(|d| d.kind.is_technique());
        return (if tech { "technique" } else { "power" }.into(), "a power (see POWERS)".into());
    }
    if res.kind == ResolutionKind::Graft {
        return ("modifier".into(), "a MODIFIER (graft): it adds its behaviours to the construct / terrain you aim at; aimed at nothing it makes a small blob that carries them".into());
    }
    if rd.costume {
        return (format!("costume character{mv}"), format!("a COSTUME CHARACTER{mv}: the character's hand-authored look, built from hard light"));
    }
    if res.shape.is_some() {
        return (format!("curated icon{mv}"), format!("a CURATED ICON{mv}: a hand-made voxel model"));
    }
    if Lexicon::is_fallback(res) {
        return ("improvised".into(), "IMPROVISED FROM WORDS: the ring didn't fully understand it and made its best guess".into());
    }
    (format!("known word{mv}"), format!("a KNOWN WORD / NAME{mv} from the lexicon, given a procedural shape"))
}

fn delivery_words(d: deliver::Delivery) -> &'static str {
    match d {
        deliver::Delivery::Onto => "dropped onto them from the sky",
        deliver::Delivery::At => "thrown / driven at them",
        deliver::Delivery::Near => "appears next to them",
        deliver::Delivery::Around => "placed around them",
        deliver::Delivery::Hunt => "forms here and hunts them",
    }
}

fn delivery_line(rd: &Reading) -> String {
    let it = &rd.it;
    match it.target {
        Some(t) => {
            let via = sim_delivery(it.delivery.unwrap_or(Delivery::Hunt));
            let eff = deliver::effective(via, &rd.recipe);
            format!("TARGET {}: {}", ROSTER[t].to_uppercase(), delivery_words(eff).to_uppercase())
        }
        None => match it.delivery {
            Some(d) => format!("NO PLAYER NAMED: DELIVERED {} THE CROSSHAIR", d.name().to_uppercase()),
            None => "NO PLAYER NAMED: CAST AT THE CROSSHAIR".into(),
        },
    }
}

fn confidence_words(c: f32) -> &'static str {
    if c >= 0.85 {
        "sure"
    } else if c >= 0.6 {
        "fairly sure"
    } else if c >= 0.35 {
        "guessing"
    } else {
        "improvising"
    }
}

fn report(rd: &Reading, spoken: bool) {
    let it = &rd.it;
    let res = &it.resolution;
    let r = &rd.recipe;
    let mode = if spoken { "said (voice path)" } else { "typed" };
    if rd.repeated {
        println!("  \"again\" repeats your last phrase: \"{}\"", rd.phrase);
    }
    println!("  {:<11} {} [{}]", "PHRASE", rd.phrase, mode);
    let (_, sentence) = becomes(rd);
    println!("  {:<11} {} - {}", "BECOMES", display_name(rd), sentence);
    if it.text.trim() != rd.phrase.trim() {
        println!("  {:<11} the resolver read \"{}\"", "", it.text);
    }
    match &it.refer {
        Some(Refer::It { .. }) => println!("  {:<11} \"it\": grafts onto your newest construct (none alive: cast as a new thing)", ""),
        Some(Refer::Again { count }) if !rd.repeated => {
            println!("  {:<11} \"again\": re-casts your last phrase{} (nothing previewed yet)", "", count.map(|n| format!(" x{n}")).unwrap_or_default())
        }
        _ => {}
    }
    if res.stand_rush.is_some() {
        println!();
        return;
    }
    println!("  {:<11} {} - {}", "FORM", form_label(r.form), category_words(r.form.category()));
    if rd.costume {
        println!("  {:<11} the character's costume (assets/costumes/characters.ron), palette pulled toward the lantern colour", "LOOK");
    } else if r.form == Form::Biped && res.signature.is_none() {
        let why = if res.shape.is_some() { "its curated icon model" } else { "a procedural hard-light figure" };
        println!("  {:<11} {why} (no costume look named \"{}\" in assets/costumes/characters.ron)", "LOOK", r.name.to_lowercase());
    }
    let emphasised: Vec<&str> = res.explanation.iter().filter(|e| e.contains("doing its move")).map(String::as_str).collect();
    if r.mechanics.is_empty() {
        println!("  {:<11} nothing special (just the thing itself)", "BEHAVIOURS");
    }
    for (i, m) in r.mechanics.iter().enumerate() {
        let name = bd_lexicon::tables::mechanic_name(*m);
        let star = if emphasised.iter().any(|e| e.contains(&format!("{m:?}"))) { "  <- the named move" } else { "" };
        println!("  {:<11} {name}: {}{star}", if i == 0 { "BEHAVIOURS" } else { "" }, mechanic_words(*m));
    }
    if res.kind == ResolutionKind::Graft {
        let g = &res.graft;
        let mut parts: Vec<String> = g.mechanics.iter().map(|m| bd_lexicon::tables::mechanic_name(*m).to_string()).collect();
        if let Some(e) = g.element {
            parts.push(format!("{} element", bd_lexicon::tables::element_name(e)));
        }
        if (g.scale_mul - 1.0).abs() > 0.01 {
            parts.push(format!("size x{:.2}", g.scale_mul));
        }
        println!("  {:<11} adds to what you aim at: {}", "GRAFT", if parts.is_empty() { "nothing".to_string() } else { parts.join(", ") });
    }
    if !r.features.is_empty() {
        println!("  {:<11} {}", "FEATURES", r.features.iter().map(|f| format!("{f:?}")).collect::<Vec<_>>().join(", "));
    }
    if r.temperament != Temperament::Calm {
        println!("  {:<11} {}", "TEMPER", temperament_words(r.temperament));
    }
    println!("  {:<11} {}", "ELEMENT", bd_lexicon::tables::element_name(r.element));
    println!(
        "  {:<11} {:.2} m, {}, x{}{}",
        "SIZE",
        r.scale,
        if r.mass >= 1000.0 { format!("{:.1} t", r.mass / 1000.0) } else { format!("{:.1} kg", r.mass) },
        r.count,
        if (r.scale - res.recipe.scale).abs() > 0.01 { format!("  (the words said {:.2} m; the sim clamps / amplifies)", res.recipe.scale) } else { String::new() }
    );
    if let Some(p) = &r.payload {
        println!("  {:<11} fires {} ({}, x{})", "PAYLOAD", p.name, bd_lexicon::tables::form_name(p.form), p.count);
    }
    if let Some(m) = &r.mount {
        println!("  {:<11} rides a {} ({}, {:.1} m)", "MOUNT", m.name, bd_lexicon::tables::form_name(m.form), m.scale);
    }
    match it.target {
        Some(t) => {
            let heard = it.heard_name.as_deref().unwrap_or("");
            let loose = if it.name_dist > 0 { " - a sound-alike / typo match" } else { "" };
            let via = sim_delivery(it.delivery.unwrap_or(Delivery::Hunt));
            let eff = deliver::effective(via, r);
            println!("  {:<11} {} (\"{heard}\" matched {}{loose})", "TARGET", ROSTER[t], ROSTER[t]);
            println!(
                "  {:<11} the words say \"{}\"; for this {} that means: {}",
                "DELIVERY",
                it.delivery.map_or("hunt", |d| d.name()),
                format!("{:?}", r.form.category()).to_lowercase(),
                delivery_words(eff)
            );
        }
        None => match it.delivery {
            Some(d) => println!("  {:<11} \"{}\" but no player named: an ordinary cast at the crosshair", "DELIVERY", d.name()),
            None => println!("  {:<11} no player named: an ordinary cast at the crosshair", "DELIVERY"),
        },
    }
    match &res.signature {
        Some(s) => {
            let via = match &s.invocation {
                Some(w) => format!("invoked with \"{w}\""),
                None => "a famous ultimate by name / a yelled beam".to_string(),
            };
            println!("  {:<11} {:?} ({via}) - title letters: {}", "SIGNATURE", s.kind, shout(&s.title));
        }
        None => println!("  {:<11} none - title letters show what was {}: \"{}\"", "SIGNATURE", if spoken { "heard" } else { "typed" }, rd.phrase),
    }
    println!("  {:<11} {:?} - {}", "TIER", rd.tier, tier_words(rd.tier));
    println!("  {:<11} {}", "DANGER", if rd.danger { "YES - the cut-in DANGER alert warns everyone nearby" } else { "no" });
    if rd.gates.is_empty() {
        println!("  {:<11} free (nothing here is gated in the power-ups mode)", "POWER-UPS");
    }
    for (i, g) in rd.gates.iter().enumerate() {
        println!("  {:<11} needs the {g}", if i == 0 { "POWER-UPS" } else { "" });
    }
    for (i, p) in rd.powers.iter().enumerate() {
        println!("  {:<11} {p}", if i == 0 { "POWERS" } else { "" });
    }
    println!("  {:<11} {} spark{} ({:?} tier; {:.0} willpower)", "COST", rd.sparks, if rd.sparks == 1 { "" } else { "s" }, rd.tier, rd.cost);
    let fallback = Lexicon::is_fallback(res);
    println!(
        "  {:<11} {:.2} ({}){}",
        "CONFIDENCE",
        res.confidence,
        confidence_words(res.confidence),
        if fallback { " - logged as unresolved in-game" } else { "" }
    );
    if !it.stages.is_empty() {
        println!("  {:<11} {}", "STAGES", it.stages.join(" > "));
    }
    // PILOTING (COMBAT.md §5d-ter): who drives what this builds.
    if let Some(p) = it.pilot {
        let what = match p {
            bd_lexicon::Piloting::Explicit => "you pilot it (the words said so) - E hops out, it becomes your follower",
            bd_lexicon::Piloting::Default => "you pilot it (a thing you drive, by default) - E hops out, it becomes your follower",
            bd_lexicon::Piloting::Follower => "a follower (fights on its own; \"get in\" or the Transform key boards it)",
            bd_lexicon::Piloting::Board => "board command: you take the wheel of your aimed (else nearest) construct",
        };
        println!("  {:<11} {what}", "PILOT");
    }
    if let Some(m) = res.recipe.opening {
        println!("  {:<11} performs {m:?} once right after it forms", "OPENING");
    }
    for (i, e) in res.explanation.iter().enumerate() {
        println!("  {:<11} - {e}", if i == 0 { "WHY" } else { "" });
    }
    if rd.costume {
        println!("  {:<11} - wears the costume look (costume summon hook)", if res.explanation.is_empty() { "WHY" } else { "" });
        if let Some(b) = &rd.bare {
            println!("  {:<11} - the costume words only dress it: its behaviours are read from \"{b}\"", "");
        }
    }
    if !res.ignored.is_empty() {
        println!("  {:<11} {}", "IGNORED", res.ignored.join(", "));
    }
}

