//! Cast-cost survey over the golden intent corpus (`crates/bd_lexicon/tests/data/intent_golden.txt`):
//! every construct phrase is CAST through the real sim (lexicon → `World::cast` /
//! `World::cast_signature`, like the console) from a full bar, and the sparks it took are bucketed
//! by the presence tier players see and hear (`presence_tier::cast_tier` of the spawn event, like
//! the client). SPARKS prices are driven by that tier (`willpower::cast_sparks`), so the table shows
//! whether price follows presence. Run:
//! `cargo test -p bd_game cost_survey -- --ignored --nocapture`.

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3};
use bd_lexicon::{Lexicon, ResolutionKind};
use bd_sim::GameEvent;
use bd_sim::presence_tier::{Tier, TierKnobs, cast_tier};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);

fn lex() -> Option<Lexicon> {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Lexicon::load(assets.join("lexicon.bin")).ok()
}

fn phrases() -> Vec<(String, bool)> {
    let text = include_str!("../../../bd_lexicon/tests/data/intent_golden.txt");
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let raw = l.splitn(3, '|').nth(1)?.trim();
            Some(match raw.strip_prefix('~') {
                Some(p) => (p.trim().to_string(), true),
                None => (raw.to_string(), false),
            })
        })
        .collect()
}

fn world() -> World {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 160.0 }, ..SimConfig::default() };
    // No hidden legendary rolls (never priced; they would blur the table).
    cfg.tuning.freedom.legendary.base = 0.0;
    cfg.tuning.freedom.legendary.cap = 0.0;
    let mut w = World::new(cfg);
    w.add_player(A, "a".into(), Corps::Willpower).unwrap();
    w.add_player(PlayerId(2), "b".into(), Corps::Rage).unwrap();
    w.start_live();
    for _ in 0..60 {
        w.tick(&[]);
    }
    w
}

/// One cast from a full bar: (sparks paid, tier seen, recipe summary).
fn cast(w: &mut World, lex: &Lexicon, phrase: &str, spoken: bool) -> Option<(f32, Tier, String)> {
    let it = lex.intent(phrase, &[], spoken);
    let r = &it.resolution;
    if r.kind != ResolutionKind::Construct || r.incantation.is_some() || r.stand_rush.is_some() {
        return None;
    }
    w.dismiss(A);
    let pool = w.rules().willpower_max();
    let spark = w.rules().segment_size();
    if let Some(p) = w.player_mut(A) {
        p.willpower = pool;
        p.casts.clear();
        p.alive = true;
        // A power phrase earlier in the corpus (a fruit, a stand) would graft onto every later cast.
        p.power = Default::default();
    }
    let p = w.player(A)?;
    let aim = Aim::new(p.eye(w.params()), Vec3::new(0.0, -0.3, -1.0).normalize());
    let n0 = w.events().len();
    let mut recipe = r.recipe.clone();
    let res = match &r.signature {
        Some(s) => {
            use bd_lexicon::SignatureKind as L;
            use bd_sim::signature::{SignatureCast, SignatureKind as S};
            let kind = match s.kind {
                L::Domain => S::Domain,
                L::Beam => S::Beam,
                L::Transformation => S::Transformation,
                L::Summon => S::Summon,
            };
            if !s.title.is_empty() {
                recipe.name = s.title.clone();
            }
            w.cast_signature(A, recipe, r.shape.clone(), aim, SignatureCast::new(kind, 0.3))
        }
        None => w.cast(A, recipe, r.shape.clone(), aim),
    };
    res.ok()?;
    let paid = pool - w.player(A)?.willpower;
    let (id, rec, weight) = w.events()[n0..].iter().find_map(|e| match e {
        GameEvent::ConstructSpawned { id, owner, recipe, cost, .. } if *owner == A => Some((*id, (**recipe).clone(), *cost)),
        _ => None,
    })?;
    // Exactly what a client derives (the spawn event + the replicated signature flag).
    let signature = w.constructs.get(id).is_some_and(|c| c.flair & bd_sim::replicate::cflags::SIGNATURE != 0);
    let tier = cast_tier(&rec, weight, false, signature, &TierKnobs::default());
    Some((paid / spark, tier, format!("{:?}/{:?} {:.1}m x{} weight {weight:.1} {:?}", rec.form, rec.form.category(), rec.scale, rec.count, rec.mechanics)))
}

#[test]
#[ignore = "prints the cast-cost distribution (SPARKS by presence tier)"]
fn cost_survey() {
    let Some(lex) = lex() else {
        eprintln!("SKIP: no lexicon.bin");
        return;
    };
    let mut w = world();
    let bar = w.rules().pool_segments();
    let mut hist = [[0usize; 13]; 5];
    let mut over = 0;
    let mut n = 0;
    for (p, spoken) in phrases() {
        if let Some((sparks, t, _)) = cast(&mut w, &lex, &p, spoken) {
            let k = sparks.round() as usize;
            hist[t.index()][k.min(12)] += 1;
            over += (sparks > bar as f32 + 1e-3) as usize;
            n += 1;
        }
    }
    println!("{n} casts, bar {bar} sparks, {over} over a full bar");
    println!("tier       n   sparks histogram");
    for t in Tier::ALL {
        let h = &hist[t.index()];
        let tot: usize = h.iter().sum();
        let med = {
            let mut acc = 0;
            (0..13).find(|k| {
                acc += h[*k];
                acc * 2 >= tot
            })
        };
        print!("{:<9} {tot:>4}  median {:?}  [", format!("{t:?}"), med.unwrap_or(0));
        for (k, c) in h.iter().enumerate() {
            if *c > 0 {
                print!("{k}:{c} ");
            }
        }
        println!("]");
    }
    println!("--- examples");
    for p in [
        "sword",
        "gun",
        "wall",
        "fire dragon",
        "meteor",
        "anvil from the sky",
        "godzilla",
        "spirit bomb",
        "domain expansion infinite void",
        "portal",
        "a swarm of bees",
        "hammer",
        "flying hammer",
        "giant robot",
        "mjolnir",
    ] {
        match cast(&mut w, &lex, p, false) {
            Some((s, t, d)) => println!("{p:<32} {s:>4.1} sparks  {t:?}  {d}"),
            None => println!("{p:<32} (not a construct cast)"),
        }
    }
}
