//! Balance tournaments (COMBAT.md §5i.6): N headless 8-bot FFA matches (seeded, parallel across
//! threads) where bots cast random recipes from the fuzz generator plus a pool of realistic player
//! phrases; telemetry (`bd_sim::telemetry`) is aggregated into `target/balance/report.json` and a
//! readable `docs/BALANCE_REPORT.md` (damage-per-willpower outliers, KO causes, KO %, fight time,
//! recovery rate, win-rate by top mechanics, never-used mechanics, streaks).
//!
//!     cargo run -p bd_sim --release --example tournament -- --matches 500
//!         [--threads 18] [--secs 240] [--ko 10] [--seed 1] [--random 50]
//!         [--before <combat.ron | default>] [--after assets/tuning/combat.ron]
//!         [--json target/balance/report.json] [--md docs/BALANCE_REPORT.md]
//!
//! LAUNCH FEEL: `--classic` runs everything under the CLASSIC LAUNCH rule; `--launch-ab` runs the
//! same seeds classic (before) vs modern (after) with one tuning; `--bot-di` makes bots DI under
//! the classic rule too (isolates the rule from the bot skill). Prints time out of control.
//!
//! With `--after`, the same seeds run twice (before = `--before`, default = built-in defaults;
//! after = the file) and the report shows both side by side. Deterministic: same arguments ⇒ same
//! report (matches run in parallel but are merged in seed order).

use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use bd_core::input::PlayerInput;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe, Temperament};
use bd_core::{DT, PlayerId};
use bd_sim::bots::{self, recipe};
use bd_sim::fuzz::{FORMS, MECHANICS, random_recipe};
use bd_sim::rng::SimRng;
use bd_sim::rules::MatchPhase;
use bd_sim::telemetry::{ARCHETYPE_NAMES, ATTACK_NAMES, Bucket, MatchReport, SOURCE_NAMES, Signature, Telemetry};
use bd_sim::tuning::CombatTuning;
use bd_sim::world::{Aim, ArenaSource, SimConfig, TargetHint, World};

// ------------------------------------------------------------------------------------ options

#[derive(Clone)]
struct Opts {
    matches: usize,
    threads: usize,
    secs: f32,
    ko: u16,
    seed: u64,
    /// % of casts drawn from the fuzz generator (the rest from the phrase pool).
    random_pct: u32,
    before: String,
    after: Option<String>,
    json: String,
    md: String,
    /// LAUNCH FEEL A/B.
    classic: bool,
    launch_ab: bool,
}

fn opts() -> Opts {
    let args: Vec<String> = std::env::args().collect();
    let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let num = |n: &str, d: u64| get(n).and_then(|v| v.parse::<u64>().ok()).unwrap_or(d);
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    Opts {
        matches: num("--matches", 500) as usize,
        threads: num("--threads", cores.saturating_sub(2).max(1) as u64) as usize,
        secs: num("--secs", 240) as f32,
        ko: num("--ko", 10) as u16,
        seed: num("--seed", 1),
        random_pct: num("--random", 50).min(100) as u32,
        before: get("--before").unwrap_or_else(|| "default".into()),
        after: get("--after"),
        json: get("--json").unwrap_or_else(|| "target/balance/report.json".into()),
        md: get("--md").unwrap_or_else(|| "docs/BALANCE_REPORT.md".into()),
        classic: args.iter().any(|a| a == "--classic"),
        launch_ab: args.iter().any(|a| a == "--launch-ab"),
    }
}

fn load_tuning(src: &str) -> CombatTuning {
    if src == "default" {
        return CombatTuning::default();
    }
    match std::fs::read_to_string(src).map_err(|e| e.to_string()).and_then(|s| CombatTuning::from_ron(&s)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tournament: can't load tuning {src}: {e}");
            std::process::exit(2);
        }
    }
}

// ------------------------------------------------------------------------------------ phrases

/// Realistic player phrases, as the lexicon would resolve them.
fn phrase_pool() -> Vec<Recipe> {
    use Element::*;
    use Form::*;
    use Mechanic::*;
    let wl = bots::word_list();
    let mut v: Vec<Recipe> = Vec::new();
    for g in [wl.melee, wl.ranged, wl.summon, wl.thrown, wl.defence, wl.mobility] {
        v.extend(g);
    }
    let mut extra = vec![
        recipe("flaming hammer with rockets that returns", Hammer, &[Thrust, Returning], Fire, 1.2),
        recipe("lightsaber", Sword, &[Blade], Electric, 1.1),
        recipe("captain america's shield", Shield, &[Returning, Ricochet], Light, 1.0),
        recipe("batarang", Disc, &[Returning, Blade], Shadow, 0.4),
        recipe("whip", Whip, &[], Light, 3.0),
        recipe("chain whip", Whip, &[Pull], Light, 4.0),
        recipe("giant axe", Axe, &[Blade, Heavy], Light, 2.2),
        recipe("ice spear", Spear, &[Pierce], Ice, 2.2),
        recipe("boxing glove", Gauntlet, &[Blunt], Light, 0.5),
        recipe("grappling hook", Gauntlet, &[Grab, Pull], Light, 0.5),
        recipe("lightning staff", Staff, &[Shoots, Beam], Electric, 1.6),
        recipe("sniper rifle", Gun, &[Shoots, Pierce], Light, 1.0),
        recipe("bazooka", Gun, &[Shoots, Explodes], Fire, 1.2),
        recipe("shark", Fish, &[Walks], Water, 1.6),
        recipe("shark with wings", Fish, &[Flies], Water, 1.6),
        recipe("pikachu", Quadruped, &[Walks], Electric, 0.8),
        recipe("charizard", Biped, &[Flies, Breath], Fire, 1.7),
        recipe("t-rex", Biped, &[Walks, ArticulatedTail], Light, 4.0),
        recipe("scorpion", Arachnid, &[ArticulatedTail, Grab], Poison, 1.0),
        recipe("snake", Serpent, &[Constrict], Poison, 1.5),
        recipe("eagle", Bird, &[Flies], Wind, 0.9),
        recipe("tank", Vehicle, &[Charge, Shoots], Light, 3.0),
        recipe("car", Vehicle, &[Charge], Light, 3.0),
        recipe("wrecking ball", Ball, &[Heavy, Blunt], Light, 2.0),
        recipe("meteor", Boulder, &[DropFromSky, Explodes], Fire, 2.0),
        recipe("homing missile", Rocket, &[Homing, Explodes], Fire, 1.2),
        recipe("cage", Cage, &[Trap], Light, 2.5),
        recipe("ramp", Ramp, &[Launch], Light, 6.0),
        recipe("bouncy wall", Wall, &[Bounce], Light, 6.0),
        recipe("charging wall", Wall, &[Charge], Light, 6.0),
        recipe("flying hammer", Hammer, &[Flies], Light, 1.2),
        recipe("spinning pillar", Pillar, &[Spin], Light, 8.0),
        recipe("portal", Ball, &[Portal], Light, 1.0),
        recipe("magnet", Ball, &[Attract], Electric, 1.0),
        recipe("leaf blower", Staff, &[Repel], Wind, 1.2),
        recipe("chain", Whip, &[Link], Light, 4.0),
        recipe("grow ray", Staff, &[Shoots, Grow], Psychic, 1.2),
        recipe("cape", Cape, &[Glide], Light, 1.2),
        recipe("umbrella", Staff, &[SlowFall], Light, 1.0),
    ];
    let mut hornets = recipe("very angry hornets", Insect, &[Swarm, Flies, Pierce], Poison, 0.5);
    hornets.count = 8;
    hornets.temperament = Temperament::Aggressive;
    hornets.intensity = 1.0;
    extra.push(hornets);
    let mut dual = recipe("dual pistols", Gun, &[Shoots], Light, 0.6);
    dual.count = 2;
    extra.push(dual);
    v.extend(extra);
    v
}

/// A fuzz recipe made castable: finite sane scale, ≤ 3 distinct mechanics, small counts (the
/// sim clamps the rest; the point is to sample the *combination* space, not hostile numbers).
fn random_cast(rng: &mut SimRng) -> Recipe {
    let mut r = random_recipe(rng, 1);
    let d = bd_sim::constructs::form_spec(r.form).default_scale;
    if !r.scale.is_finite() || r.scale <= 0.0 || r.scale > d * 4.0 {
        r.scale = d * rng.range(-0.4, 0.6).exp();
    }
    if !r.mass.is_finite() || r.mass <= 0.0 {
        r.mass = 20.0;
    }
    let mut seen = Vec::with_capacity(3);
    for m in r.mechanics.drain(..) {
        if !seen.contains(&m) && seen.len() < 3 {
            seen.push(m);
        }
    }
    r.mechanics = seen;
    if !r.has(Mechanic::Swarm) {
        r.count = r.count.clamp(1, 3);
    }
    if !r.intensity.is_finite() {
        r.intensity = 0.5;
    }
    r.normalized()
}

// ------------------------------------------------------------------------------------ one match

fn run_match(seed: u64, tuning: &CombatTuning, o: &Opts, pool: &[Recipe], classic: bool) -> MatchReport {
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed: seed % 16 + 1 }, seed, check_invariants: false, tuning: tuning.clone(), ..SimConfig::default() };
    cfg.timings.warmup_secs = 1.0;
    cfg.timings.auto_rematch = false;
    cfg.rules.time_limit = o.secs;
    cfg.rules.ko_target = o.ko;
    cfg.rules.classic_launch = classic;
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 8, 0.7, seed);
    for (k, b) in brains.iter_mut().enumerate() {
        b.casts_enabled = false;
        b.difficulty = 0.45 + 0.07 * k as f32;
    }
    let mut rng = SimRng::new(seed ^ 0xBA1A_4CE5);
    let mut cast_cd: Vec<f32> = (0..brains.len()).map(|_| rng.range(1.0, 4.0)).collect();
    let mut seated = vec![0.0f32; brains.len()];
    let mut tel = Telemetry::new(seed);
    let mut inputs: Vec<(PlayerId, PlayerInput)> = Vec::with_capacity(8);
    let max_ticks = ((o.secs + 10.0) / DT) as u64;
    for _ in 0..max_ticks {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        if w.match_state.phase == MatchPhase::Live {
            for k in 0..brains.len() {
                let id = brains[k].id;
                let Some(p) = w.player(id) else { continue };
                if !p.alive || p.oath.reciting() {
                    continue;
                }
                // Bots can't really drive a mech: hop out after a while.
                if p.seat.is_some() {
                    seated[k] += DT;
                    if seated[k] > 12.0 {
                        w.eject(id);
                        seated[k] = 0.0;
                    }
                    continue;
                }
                cast_cd[k] -= DT;
                if cast_cd[k] > 0.0 {
                    continue;
                }
                cast_cd[k] = rng.range(2.5, 6.0);
                let r = if rng.below(100) < o.random_pct { random_cast(&mut rng) } else { pool[rng.below(pool.len() as u32) as usize].clone() };
                let (_, cost) = w.preview_cost(id, r.clone());
                let Some(p) = w.player(id) else { continue };
                if cost > p.willpower {
                    continue;
                }
                let eye = p.eye(w.params());
                let pos = p.mv.pos;
                let target = w
                    .players
                    .iter()
                    .filter(|q| q.alive && q.id != id)
                    .min_by(|a, b| (a.mv.pos - pos).length_squared().total_cmp(&(b.mv.pos - pos).length_squared()))
                    .map(|q| (q.id, q.hull(w.params()).center()));
                let look = match target {
                    Some((_, at)) => (at - eye).normalize_or(p.look()),
                    None => p.look(),
                };
                let mut aim = Aim::new(eye, look);
                aim.target_hint = target.map(|t| TargetHint::Player(t.0));
                let _ = w.cast(id, r, None, aim);
            }
        }
        w.tick(&inputs);
        tel.observe(&w);
        if w.match_state.phase == MatchPhase::Ended {
            break;
        }
    }
    tel.finish(&w)
}

/// All matches for one tuning, in parallel; merged in seed order (deterministic).
fn run_all(label: &str, tuning: &CombatTuning, o: &Opts, pool: &[Recipe], classic: bool) -> MatchReport {
    let next = AtomicUsize::new(0);
    let slots: Mutex<Vec<Option<MatchReport>>> = Mutex::new(vec![None; o.matches]);
    let t0 = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..o.threads.max(1) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= o.matches {
                        break;
                    }
                    let r = run_match(o.seed.wrapping_add(i as u64), tuning, o, pool, classic);
                    if let Ok(mut v) = slots.lock() {
                        v[i] = Some(r);
                    }
                    if (i + 1) % 25 == 0 {
                        eprintln!("[{label}] {}/{} matches ({:.0} s)", i + 1, o.matches, t0.elapsed().as_secs_f32());
                    }
                }
            });
        }
    });
    let mut all = MatchReport::default();
    for r in slots.into_inner().unwrap_or_default().into_iter().flatten() {
        all.merge(&r);
    }
    eprintln!("[{label}] done: {} matches in {:.0} s", all.matches, t0.elapsed().as_secs_f32());
    all
}

// ------------------------------------------------------------------------------------ stats

fn pct(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s[((s.len() - 1) as f64 * q).round() as usize]
}

/// Rows with enough samples: (name, bucket, dpw).
fn rows<'a>(names: impl Iterator<Item = String>, b: &'a [Bucket], min_casts: u32) -> Vec<(String, &'a Bucket, f64)> {
    names.zip(b.iter()).filter_map(|(n, b)| b.dpw().filter(|_| b.casts >= min_casts).map(|d| (n, b, d))).collect()
}

/// Median over damage-dealing rows only (≥ 10 hits): utility sources (wings, portals, walls…)
/// deal ~no damage by design and would drag the reference down.
fn dpw_median(r: &[(String, &Bucket, f64)]) -> f64 {
    pct(&r.iter().filter(|x| x.1.hits >= 10).map(|x| x.2).collect::<Vec<_>>(), 0.5)
}

struct Summary {
    matches: u32,
    kos: u32,
    ko_share: Vec<(String, u32, f64)>,
    credited_share: f64,
    self_never_hit: f64,
    self_stale: f64,
    stale_secs: f64,
    ko_pct: (f64, f64, f64),
    ko_pct_all: f64,
    fight: (f64, f64, f64),
    recovery: f64,
    launches: u32,
    techs: u32,
    punches: u32,
    kos_per_min: f64,
    streak_max: u16,
    streak_mean: f64,
    shutdowns: u32,
}

fn summarize(r: &MatchReport, min_casts: u32) -> Summary {
    let kos = r.total_kos();
    let ko_share = r.ko_causes.iter().map(|(k, n)| (k.clone(), *n, *n as f64 / kos.max(1) as f64)).collect::<Vec<_>>();
    let _ = min_casts;
    let kp: Vec<f64> = r.ko_pct_credited.iter().map(|x| f64::from(*x)).collect();
    let all: Vec<f64> = r.ko_pct.iter().map(|x| f64::from(*x)).collect();
    let fs: Vec<f64> = r.fight_secs.iter().map(|x| f64::from(*x)).collect();
    Summary {
        matches: r.matches,
        kos,
        ko_share,
        credited_share: r.ko_credited as f64 / kos.max(1) as f64,
        self_never_hit: r.ko_self_never_hit as f64 / kos.max(1) as f64,
        self_stale: r.ko_self_stale as f64 / kos.max(1) as f64,
        stale_secs: pct(&r.ko_self_stale_secs.iter().map(|x| f64::from(*x)).collect::<Vec<_>>(), 0.5),
        ko_pct: (pct(&kp, 0.25), pct(&kp, 0.5), pct(&kp, 0.75)),
        ko_pct_all: pct(&all, 0.5),
        fight: (pct(&fs, 0.25), pct(&fs, 0.5), pct(&fs, 0.75)),
        recovery: r.recoveries as f64 / r.launches.max(1) as f64,
        launches: r.launches,
        techs: r.techs,
        punches: r.punches,
        kos_per_min: kos as f64 / (r.secs / 60.0).max(1e-6),
        streak_max: r.streaks.iter().copied().max().unwrap_or(0),
        streak_mean: r.streaks.iter().map(|s| f64::from(*s)).sum::<f64>() / r.streaks.len().max(1) as f64,
        shutdowns: r.shutdowns,
    }
}

// ------------------------------------------------------------------------------------ report

fn flag(ratio: f64) -> &'static str {
    if ratio > 2.0 {
        "**HIGH**"
    } else if ratio < 0.3 {
        "low"
    } else {
        ""
    }
}

fn table(md: &mut String, title: &str, before: Vec<(String, &Bucket, f64)>, after: Option<Vec<(String, &Bucket, f64)>>) {
    let mb = dpw_median(&before);
    let _ = writeln!(md, "\n### {title}\n");
    match &after {
        None => {
            let _ = writeln!(md, "median dmg/wp = {mb:.2}\n");
            let _ = writeln!(md, "| source | casts | hits | damage | KOs | willpower | dmg/wp | ×median | flag |");
            let _ = writeln!(md, "|---|---:|---:|---:|---:|---:|---:|---:|---|");
            let mut v = before;
            v.sort_by(|a, b| b.2.total_cmp(&a.2));
            for (n, b, d) in v {
                let _ = writeln!(md, "| {n} | {} | {} | {:.0} | {} | {:.0} | {d:.2} | {:.2} | {} |", b.casts, b.hits, b.damage, b.kos, b.willpower(), d / mb, flag(d / mb));
            }
        }
        Some(a) => {
            let ma = dpw_median(a);
            let _ = writeln!(md, "median dmg/wp: before {mb:.2} → after {ma:.2}\n");
            let _ = writeln!(md, "| source | casts b/a | KOs b/a | dmg/wp before | ×med | dmg/wp after | ×med | flag after |");
            let _ = writeln!(md, "|---|---:|---:|---:|---:|---:|---:|---|");
            let mut v = before;
            v.sort_by(|x, y| y.2.total_cmp(&x.2));
            for (n, b, d) in v {
                let (ac, ak, ad) = a.iter().find(|x| x.0 == n).map_or((0, 0, f64::NAN), |x| (x.1.casts, x.1.kos, x.2));
                let _ = writeln!(md, "| {n} | {}/{ac} | {}/{ak} | {d:.2} | {:.2} | {ad:.2} | {:.2} | {} |", b.casts, b.kos, d / mb, ad / ma, flag(ad / ma));
            }
        }
    }
}

fn arr_rows<'a>(names: &[String], b: &'a [Bucket], min: u32) -> Vec<(String, &'a Bucket, f64)> {
    rows(names.iter().cloned(), b, min)
}

/// Cost proposals for dmg/wp outliers among mechanics and forms (never behaviour).
fn proposals(r: &MatchReport, min_casts: u32) -> Vec<String> {
    let mut out = Vec::new();
    let mech = rows(MECHANICS.iter().map(|m| format!("{m:?}")), &r.by_mechanic, min_casts);
    let mm = dpw_median(&mech);
    for (n, b, d) in &mech {
        let k = d / mm;
        if k > 2.0 {
            // Mean mechanic factor of casts carrying it ≈ 1 + 0.15·2; raise its weight so the cast
            // portion of the cost grows by ≈ k/1.5 (capped ×2).
            let grow = (k / 1.5).min(2.0);
            let extra = 1.3 * (grow - 1.0);
            out.push(format!("mechanic {n}: dmg/wp {d:.2} = {k:.2}× median ({} casts) → willpower.mechanic_cost (mechanic: {n}, extra: {extra:.2})", b.casts));
        } else if k < 0.3 {
            out.push(format!("mechanic {n}: dmg/wp {d:.2} = {k:.2}× median ({} casts) → cheaper? (utility mechanics often deal no damage by design — check before cutting)", b.casts));
        }
    }
    let names: Vec<String> = FORMS.iter().map(|f| format!("{f:?}")).collect();
    let forms = arr_rows(&names, &r.by_form, min_casts);
    let fm = dpw_median(&forms);
    for (n, b, d) in &forms {
        let k = d / fm;
        if k > 2.0 {
            out.push(format!("form {n}: dmg/wp {d:.2} = {k:.2}× median ({} casts) → willpower.form_cost (form: {n}, mult: {:.2})", b.casts, (k / 1.5).min(2.0)));
        } else if k < 0.3 {
            out.push(format!("form {n}: dmg/wp {d:.2} = {k:.2}× median ({} casts) → low", b.casts));
        }
    }
    out
}

fn summary_md(md: &mut String, s: &Summary) {
    let _ = writeln!(md, "- matches: {}, KOs: {} ({:.2} KOs/min across the lobby)", s.matches, s.kos, s.kos_per_min);
    let _ = writeln!(md, "- KO causes (cause/blast zone): {}", s.ko_share.iter().map(|(k, n, f)| format!("{k} {n} ({:.0}%)", f * 100.0)).collect::<Vec<_>>().join(", "));
    let _ = writeln!(
        md,
        "- credited to an opponent: **{:.0}%**; uncredited: never hit since spawning {:.0}%, hit but credit expired {:.0}% (median {:.1} s after the last hit)",
        s.credited_share * 100.0,
        s.self_never_hit * 100.0,
        s.self_stale * 100.0,
        s.stale_secs
    );
    let _ = writeln!(md, "- KO % (credited KOs): p25 {:.0} / **median {:.0}** / p75 {:.0} (all KOs: median {:.0})", s.ko_pct.0, s.ko_pct.1, s.ko_pct.2, s.ko_pct_all);
    let _ = writeln!(md, "- fight time (victim's first hit after spawning → credited KO): p25 {:.1} s / **median {:.1} s** / p75 {:.1} s", s.fight.0, s.fight.1, s.fight.2);
    let _ = writeln!(md, "- launches: {}, recovered: **{:.0}%**; techs {}, punch-throughs {}", s.launches, s.recovery * 100.0, s.techs, s.punches);
    let _ = writeln!(md, "- streaks: mean {:.2}, longest {}, shutdowns {}", s.streak_mean, s.streak_max, s.shutdowns);
}

fn report_md(o: &Opts, before: &MatchReport, after: Option<&MatchReport>, props: &[String], min_casts: u32) -> String {
    let sb = summarize(before, min_casts);
    let sa = after.map(|a| summarize(a, min_casts));
    let mut md = String::new();
    let _ = writeln!(md, "# Balance report\n");
    let _ = writeln!(
        md,
        "_Generated by `cargo run -p bd_sim --release --example tournament` — {} matches × 8 bots, {} s cap / first to {} KOs, seeds {}..{}, {}% fuzz recipes + {}% phrase pool. Before = `{}`{}. Hidden data (mystery tenet): never shown to players._\n",
        o.matches,
        o.secs,
        o.ko,
        o.seed,
        o.seed + o.matches as u64 - 1,
        o.random_pct,
        100 - o.random_pct,
        o.before,
        o.after.as_ref().map_or(String::new(), |a| format!(", after = `{a}`"))
    );
    let _ = writeln!(md, "## Targets (COMBAT.md §5i)\n");
    let _ = writeln!(md, "Smash model: typical KO % ≈ 110–170; a focused fight from 0 % to KO ≈ 8–15 s; recoveries (tethers / constructs) save ~40 % of launches. Every KO is a ring-out now; the split is bottom / side / top.\n");
    let _ = writeln!(md, "**Before**\n");
    summary_md(&mut md, &sb);
    if let Some(sa) = &sa {
        let _ = writeln!(md, "\n**After**\n");
        summary_md(&mut md, sa);
    }
    let _ = writeln!(md, "\n## Damage per willpower\n");
    let _ = writeln!(md, "damage = % (health model: HP) dealt to *other* players; willpower = cast + graft cost + upkeep × lifetime. Rows need ≥ {min_casts} casts. Outliers: > 2× or < 0.3× the median. Utility mechanics (Portal, SlowFall, Glide, Guard, Barrier…) are *supposed* to deal little damage — a low ratio there is not a bug.");
    let a = after;
    let mech_names: Vec<String> = MECHANICS.iter().map(|m| format!("{m:?}")).collect();
    let form_names: Vec<String> = FORMS.iter().map(|m| format!("{m:?}")).collect();
    let arch_names: Vec<String> = ARCHETYPE_NAMES.iter().map(|m| m.to_string()).collect();
    let att_names: Vec<String> = ATTACK_NAMES.iter().map(|m| m.to_string()).collect();
    table(&mut md, "By mechanic", arr_rows(&mech_names, &before.by_mechanic, min_casts), a.map(|a| arr_rows(&mech_names, &a.by_mechanic, min_casts)));
    table(&mut md, "By form", arr_rows(&form_names, &before.by_form, min_casts), a.map(|a| arr_rows(&form_names, &a.by_form, min_casts)));
    table(&mut md, "By gun archetype (held guns)", arr_rows(&arch_names, &before.by_archetype, min_casts / 4), a.map(|a| arr_rows(&arch_names, &a.by_archetype, min_casts / 4)));
    table(&mut md, "By held melee form", arr_rows(&form_names, &before.by_held, min_casts / 4), a.map(|a| arr_rows(&form_names, &a.by_held, min_casts / 4)));
    // Creature attacks have no casts of their own: damage share instead.
    let _ = writeln!(md, "\n### Creature attacks (damage share of all creature damage)\n");
    let tot: f64 = before.by_attack.iter().map(|b| b.damage).sum();
    let _ = writeln!(md, "| attack | hits | damage | share | KOs |\n|---|---:|---:|---:|---:|");
    for (n, b) in att_names.iter().zip(before.by_attack.iter()) {
        if b.hits > 0 {
            let _ = writeln!(md, "| {n} | {} | {:.0} | {:.0}% | {} |", b.hits, b.damage, 100.0 * b.damage / tot.max(1e-9), b.kos);
        }
    }
    let _ = writeln!(md, "\n### Non-construct sources\n\n| source | hits | damage | KOs |\n|---|---:|---:|---:|");
    for (n, b) in SOURCE_NAMES.iter().zip(before.by_source.iter()) {
        if b.hits > 0 || b.kos > 0 {
            let _ = writeln!(md, "| {n} | {} | {:.0} | {} |", b.hits, b.damage, b.kos);
        }
    }
    // Signatures.
    let mut sigs: Vec<(&Signature, &Bucket, f64)> = before.by_signature.iter().filter_map(|(k, b)| b.dpw().filter(|_| b.casts >= min_casts / 2).map(|d| (k, b, d))).collect();
    sigs.sort_by(|x, y| y.2.total_cmp(&x.2));
    let sm = pct(&sigs.iter().map(|x| x.2).collect::<Vec<_>>(), 0.5);
    let _ = writeln!(md, "\n### Top / bottom signatures (form + mechanics + element, ≥ {} casts; median dmg/wp {sm:.2})\n", min_casts / 2);
    let _ = writeln!(md, "| signature | casts | damage | KOs | dmg/wp | ×median |\n|---|---:|---:|---:|---:|---:|");
    let n = sigs.len();
    for (i, (k, b, d)) in sigs.iter().enumerate() {
        if i < 12 || i + 8 >= n {
            let _ = writeln!(md, "| {} | {} | {:.0} | {} | {d:.2} | {:.2} |", k.describe(), b.casts, b.damage, b.kos, d / sm);
        } else if i == 12 {
            let _ = writeln!(md, "| … | | | | | |");
        }
    }
    // Win rate by top mechanics.
    let _ = writeln!(md, "\n## Win rate by a player's top-3 mechanics\n\nBaseline 1/8 = 12.5 %. Needs ≥ 40 appearances.\n");
    let _ = writeln!(md, "| mechanic | appearances | wins | win rate{} |\n|---|---:|---:|---:{}", if a.is_some() { " | after" } else { "" }, if a.is_some() { "|---:" } else { "" });
    let mut wr: Vec<(usize, f64)> = before.mech_wins.iter().enumerate().filter(|(_, (n, _))| *n >= 40).map(|(i, (n, w))| (i, *w as f64 / *n as f64)).collect();
    wr.sort_by(|x, y| y.1.total_cmp(&x.1));
    for (i, r) in wr {
        let (n, w) = before.mech_wins[i];
        let after_col = a.map_or(String::new(), |a| {
            let (n2, w2) = a.mech_wins[i];
            format!(" | {:.1}%", 100.0 * w2 as f64 / n2.max(1) as f64)
        });
        let _ = writeln!(md, "| {:?} | {n} | {w} | {:.1}%{after_col} |", MECHANICS[i], 100.0 * r);
    }
    // Never used.
    let never_cast: Vec<String> = MECHANICS.iter().zip(before.by_mechanic.iter()).filter(|(_, b)| b.casts == 0).map(|(m, _)| format!("{m:?}")).collect();
    let never_dmg: Vec<String> = MECHANICS.iter().zip(before.by_mechanic.iter()).filter(|(_, b)| b.casts > 0 && b.hits == 0).map(|(m, _)| format!("{m:?}")).collect();
    let never_ko: Vec<String> = MECHANICS.iter().zip(before.by_mechanic.iter()).filter(|(_, b)| b.casts > 0 && b.kos == 0).map(|(m, _)| format!("{m:?}")).collect();
    let _ = writeln!(md, "\n## Never-used mechanics\n");
    let _ = writeln!(md, "- never cast: {}", if never_cast.is_empty() { "none".into() } else { never_cast.join(", ") });
    let _ = writeln!(md, "- cast but never hit a player: {}", if never_dmg.is_empty() { "none".into() } else { never_dmg.join(", ") });
    let _ = writeln!(md, "- cast but never scored a KO: {}", if never_ko.is_empty() { "none".into() } else { never_ko.join(", ") });
    // Streaks.
    let _ = writeln!(md, "\n## Streak lengths\n");
    let mut hist = [0u32; 8];
    for s in &before.streaks {
        hist[(*s as usize).min(7)] += 1;
    }
    let _ = writeln!(md, "| streak | 1 | 2 | 3 | 4 | 5 | 6 | 7+ |\n|---|---:|---:|---:|---:|---:|---:|---:|");
    let _ = writeln!(md, "| count | {} | {} | {} | {} | {} | {} | {} |", hist[1], hist[2], hist[3], hist[4], hist[5], hist[6], hist[7]);
    // Speed.
    let tot: u64 = before.speed_hist.iter().sum();
    let mut acc = 0u64;
    let mut p50 = 0;
    let mut p90 = 0;
    for (i, h) in before.speed_hist.iter().enumerate() {
        acc += h;
        if p50 == 0 && acc * 2 >= tot {
            p50 = i;
        }
        if p90 == 0 && acc * 10 >= tot * 9 {
            p90 = i;
        }
    }
    let _ = writeln!(md, "\n## Player speed\n\nmedian ≈ {}–{} m/s, p90 ≈ {}–{} m/s (living players, every tick).", p50 * 2, p50 * 2 + 2, p90 * 2, p90 * 2 + 2);
    // Proposals.
    let _ = writeln!(md, "\n## Cost proposals (from the *before* run)\n");
    let _ = writeln!(md, "Balance with willpower cost, never behaviour (§5i.1). Only clear-cut ones are applied in `assets/tuning/combat.ron`; see “Applied” below.\n");
    for p in props {
        let _ = writeln!(md, "- {p}");
    }
    md
}

fn main() {
    let o = opts();
    let pool = phrase_pool();
    let before_t = load_tuning(&o.before);
    let min_casts = ((o.matches as u32) / 5).max(20);
    if std::env::args().any(|a| a == "--bot-di") {
        bd_sim::dodge::BOT_DI_IN_CLASSIC.store(true, Ordering::Relaxed);
    }
    let before = run_all("before", &before_t, &o, &pool, o.classic || o.launch_ab);
    let after = if o.launch_ab {
        Some(run_all("after", &before_t, &o, &pool, false))
    } else {
        o.after.as_ref().map(|p| {
            let t = load_tuning(p);
            run_all("after", &t, &o, &pool, o.classic)
        })
    };
    let props = proposals(&before, min_casts);
    for p in &props {
        println!("PROPOSAL {p}");
    }
    let mut json = String::new();
    let _ = write!(json, "{{\"matches\":{},\"secs\":{},\"seed\":{},\"random_pct\":{},\"before_tuning\":\"{}\",\"before\":{}", o.matches, o.secs, o.seed, o.random_pct, o.before, before.to_json());
    if let (Some(a), Some(p)) = (&after, &o.after) {
        let _ = write!(json, ",\"after_tuning\":\"{p}\",\"after\":{}", a.to_json());
    }
    json.push_str(",\"proposals\":[");
    for (i, p) in props.iter().enumerate() {
        let _ = write!(json, "{}\"{}\"", if i > 0 { "," } else { "" }, p.replace('"', "'"));
    }
    json.push_str("]}");
    if let Some(d) = std::path::Path::new(&o.json).parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Err(e) = std::fs::write(&o.json, json) {
        eprintln!("can't write {}: {e}", o.json);
    }
    let md = report_md(&o, &before, after.as_ref(), &props, min_casts);
    if let Err(e) = std::fs::write(&o.md, md) {
        eprintln!("can't write {}: {e}", o.md);
    }
    let s = summarize(&before, min_casts);
    let line = |tag: &str, s: &Summary| {
        println!(
            "{tag}: {} KOs ({:.0}% credited, {:.0}% never hit, {:.0}% stale credit), KO% median {:.0} (all {:.0}), fight median {:.1}s, recovery {:.0}%, causes {:?}",
            s.kos,
            s.credited_share * 100.0,
            s.self_never_hit * 100.0,
            s.self_stale * 100.0,
            s.ko_pct.1,
            s.ko_pct_all,
            s.fight.1,
            s.recovery * 100.0,
            s.ko_share.iter().map(|(k, n, _)| format!("{k}={n}")).collect::<Vec<_>>()
        );
    };
    line("before", &s);
    if let Some(a) = &after {
        line("after", &summarize(a, min_casts));
    }
    // LAUNCH FEEL: time out of control (share of alive time) + KO %.
    let control = |tag: &str, r: &MatchReport| {
        let c = &r.control;
        let kp: Vec<f64> = r.ko_pct_credited.iter().map(|x| f64::from(*x)).collect();
        let mean = if kp.is_empty() { f64::NAN } else { kp.iter().sum::<f64>() / kp.len() as f64 };
        println!(
            "CONTROL {tag}: hitstun {:.2}% | tumble {:.2}% | launched-air {:.2}% of alive time ({:.0} player-s) | launches {} ({:.2}/player-min) | KOs {} ({:.2}/player-min) | credited KO% mean {:.0} median {:.0}",
            c.hitstun_frac() * 100.0,
            c.tumble_frac() * 100.0,
            c.launched_air_frac() * 100.0,
            c.alive,
            r.launches,
            f64::from(r.launches) / (c.alive / 60.0).max(1e-9),
            r.total_kos(),
            f64::from(r.total_kos()) / (c.alive / 60.0).max(1e-9),
            mean,
            pct(&kp, 0.5)
        );
    };
    control("before", &before);
    if let Some(a) = &after {
        control("after", a);
    }
    println!("wrote {} and {}", o.json, o.md);
}
