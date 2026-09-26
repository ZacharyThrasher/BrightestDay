//! FREEDOM bundle (`bd_sim::freedom`, `bd_sim::legendary`): "it" grafts onto your last construct (and
//! only yours), casts never refuse for willpower (sputter, never nothing, never over budget), the
//! shout bonus is clamped (typing == voice), hidden legendary rolls (repeats never roll, rich novel
//! phrases land in their band, deterministic, replicas agree), kill-feed lines with the phrase, and
//! the end-of-match highlights.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe, Temperament};
use bd_core::{ConstructId, DT, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::freedom::{self, CastMeta, FeedLine, FreedomEvent};
use bd_sim::legendary::{self, LegendaryTuning, NoveltySet, PhraseKey};
use bd_sim::replicate::cflags;
use bd_sim::world::{Aim, ArenaSource, SimConfig, TargetHint, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn duel_with(legendary: Option<f32>) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    if let Some(p) = legendary {
        let mut t = w.tuning().clone();
        t.freedom.legendary.base = p;
        t.freedom.legendary.cap = p;
        w.set_tuning(t);
    }
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    for _ in 0..60 {
        w.tick(&[]);
    }
    let a = w.player(A).unwrap().mv.pos;
    w.player_mut(B).unwrap().mv.pos = a + Vec3::new(0.0, 0.0, -10.0);
    for _ in 0..30 {
        w.tick(&[]);
    }
    w
}

/// No legendary rolls (isolates the other rules).
fn duel() -> World {
    duel_with(Some(0.0))
}

fn aim_of(w: &World, who: PlayerId) -> Aim {
    let p = w.player(who).unwrap();
    Aim::new(p.mv.pos + Vec3::Y * 1.6, Vec3::new(0.0, -0.2, -1.0).normalize())
}

/// Aim straight up (nothing under the crosshair).
fn sky(w: &World, who: PlayerId) -> Aim {
    let p = w.player(who).unwrap();
    Aim::new(p.mv.pos + Vec3::Y * 1.6, Vec3::Y)
}

fn cast_said(w: &mut World, who: PlayerId, r: Recipe, phrase: &str, energy: u8, aim: Aim) -> ConstructId {
    w.set_cast_meta(who, CastMeta::new(phrase, energy));
    w.cast(who, r, None, aim).expect("casts never refuse for willpower")
}

fn hammer() -> Recipe {
    recipe("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light, 1.2)
}

// ------------------------------------------------------------------------------------ "it"

#[test]
fn it_grafts_onto_the_last_construct_and_only_your_own() {
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = aim_of(&w, A);
    let id = cast_said(&mut w, A, hammer(), "hammer", 0, aim);
    for _ in 0..30 {
        w.tick(&[]);
    }
    // "set it on fire and make it explode" → a graft (modifier) aimed at the sky, hinted at the hammer.
    let mut add = Recipe::unknown("set on fire");
    add.element = Element::Fire;
    add.mechanics = vec![Mechanic::Explodes];
    let add = add.normalized();
    let mut it = sky(&w, A);
    it.modifier = true;
    it.target_hint = Some(TargetHint::Construct(id));
    let wp0 = w.player(A).unwrap().willpower;
    let got = cast_said(&mut w, A, add.clone(), "set it on fire", 0, it);
    assert_eq!(got, id, "grafted onto the last construct");
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.recipe.element, Element::Fire);
    assert!(c.recipe.has(Mechanic::Explodes) && c.recipe.has(Mechanic::Blunt));
    assert!(w.player(A).unwrap().willpower < wp0, "graft pricing applies");
    // "make it bigger": resize ×1.5 (graft path), paid.
    let s0 = w.constructs.get(id).unwrap().size_scale;
    let wp1 = w.player(A).unwrap().willpower;
    w.set_cast_meta(A, CastMeta { resize: 1.5, ..CastMeta::new("make it bigger", 0) });
    let got = w.cast(A, Recipe::unknown("bigger").normalized(), None, it).unwrap();
    assert_eq!(got, id);
    let s1 = w.constructs.get(id).unwrap().size_scale;
    assert!((s1 / s0 - 1.5).abs() < 1e-3, "resized {s0} → {s1}");
    assert!(w.player(A).unwrap().willpower < wp1);
    // A hostile resize is clamped (×2 max, and never past the rules' scale range for the form).
    w.player_mut(A).unwrap().willpower = 100.0;
    w.set_cast_meta(A, CastMeta { resize: 50.0, ..CastMeta::new("make it enormous", 0) });
    let _ = w.cast(A, Recipe::unknown("enormous").normalized(), None, it).unwrap();
    let c = w.constructs.get(id).unwrap();
    let spec = bd_sim::constructs::forms::form_spec(Form::Hammer);
    assert!(c.recipe.scale * c.size_scale <= spec.max_scale + 1e-3, "{} × {}", c.recipe.scale, c.size_scale);

    // Ownership: B's "set it on fire" hinted at A's hammer builds B's own thing instead.
    w.player_mut(B).unwrap().willpower = 100.0;
    let mut steal = sky(&w, B);
    steal.modifier = true;
    steal.target_hint = Some(TargetHint::Construct(id));
    let mut ice = Recipe::unknown("freeze");
    ice.element = Element::Ice;
    let got = cast_said(&mut w, B, ice.normalized(), "freeze it", 0, steal);
    assert_ne!(got, id, "never grafts onto someone else's construct");
    assert_eq!(w.constructs.get(id).unwrap().recipe.element, Element::Fire);
    assert_eq!(w.constructs.get(got).unwrap().owner, B);
}

// ------------------------------------------------------------------------------------ sputter

#[test]
fn sputter_never_produces_nothing_and_never_overspends() {
    let kaiju = || recipe("godzilla", Form::Kaiju, &[Mechanic::Stomp, Mechanic::Breath], Element::Atomic, 20.0);
    let swarm = || {
        let mut r = recipe("bees", Form::Insect, &[Mechanic::Swarm, Mechanic::Flies], Element::Poison, 0.3);
        r.count = 10;
        r
    };
    for (k, have) in [0.0f32, 0.01, 0.5, 3.0, 12.0, 30.0].into_iter().enumerate() {
        for (j, make) in [&kaiju as &dyn Fn() -> Recipe, &swarm].into_iter().enumerate() {
            let mut w = duel();
            let r = make();
            let (v, full) = w.preview_cost(A, r.clone());
            assert!(full > have, "test premise: {full} > {have}");
            w.player_mut(A).unwrap().willpower = have;
            let aim = aim_of(&w, A);
            let n0 = w.events().len();
            let id = cast_said(&mut w, A, r.clone(), &format!("phrase {k} {j}"), 0, aim);
            let c = w.constructs.get(id).expect("never nothing");
            assert!(freedom::is_sputtering(&w, id), "flagged");
            assert!(w.construct_flair(id) & cflags::SPUTTER != 0);
            let left = w.player(A).unwrap().willpower;
            assert!(left.abs() < 1e-4, "drains what's left: {left}");
            let paid = w.events()[n0..]
                .iter()
                .find_map(|e| match e {
                    GameEvent::ConstructSpawned { id: s, cost, .. } if *s == id => Some(*cost),
                    _ => None,
                })
                .unwrap();
            assert!(paid <= have + 1e-4, "never exceeds what's left: {paid} > {have}");
            assert!(
                c.recipe.scale <= v.scale + 1e-4 && c.recipe.count <= v.count,
                "scaled down: {} ×{} vs {} ×{}",
                c.recipe.scale,
                c.recipe.count,
                v.scale,
                v.count
            );
            assert!(c.recipe.scale < v.scale - 1e-4 || c.recipe.count < v.count || have >= full * 0.9, "smaller than asked");
            let spec = bd_sim::constructs::forms::form_spec(r.form);
            assert!(c.recipe.scale >= spec.min_scale - 1e-4, "floor");
            assert!(c.integrity > 0.0);
        }
    }
    // Affordable casts are untouched.
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = aim_of(&w, A);
    let id = cast_said(&mut w, A, hammer(), "hammer", 0, aim);
    assert!(!freedom::is_sputtering(&w, id));
    assert!(w.player(A).unwrap().willpower > 50.0);
    // An unaffordable graft still applies, drains what's left, sputters.
    for _ in 0..30 {
        w.tick(&[]);
    }
    w.player_mut(A).unwrap().willpower = 0.0;
    let mut it = sky(&w, A);
    it.modifier = true;
    it.target_hint = Some(TargetHint::Construct(id));
    let wings = recipe("wings", Form::Blob0, &[Mechanic::Flies], Element::Light, 1.0);
    assert_eq!(cast_said(&mut w, A, wings, "give it wings", 0, it), id);
    assert!(w.constructs.get(id).unwrap().recipe.has(Mechanic::Flies));
    assert!(freedom::is_sputtering(&w, id));
    assert_eq!(w.player(A).unwrap().willpower, 0.0);
}

// ------------------------------------------------------------------------------------ shout energy

#[test]
fn shout_energy_bonus_is_clamped_and_voice_equals_typing() {
    let scale_with = |energy: u8, r: Recipe| {
        let mut w = duel();
        w.player_mut(A).unwrap().willpower = 100.0;
        let aim = aim_of(&w, A);
        let id = cast_said(&mut w, A, r, "sword", energy, aim);
        w.constructs.get(id).unwrap().recipe.scale
    };
    let sword = || recipe("sword", Form::Sword, &[Mechanic::Blade], Element::Light, 1.0);
    let calm = scale_with(0, sword());
    let full = scale_with(255, sword());
    assert!((full / calm - 1.15).abs() < 1e-4, "full shout = +15 %: {calm} → {full}");
    let half = scale_with(128, sword());
    assert!(half > calm && half < full);
    // Typed max and voice max map to the same byte (and so the same bonus).
    assert_eq!(freedom::energy_byte(2.3), freedom::energy_byte(2.6));
    assert_eq!(freedom::energy_byte(1.43), 255, "typed HAMMER!!!");
    assert_eq!(freedom::energy_byte(2.2), 255, "a full yell");
    // Never past the rules' clamp.
    let spec = bd_sim::constructs::forms::form_spec(Form::Sword);
    let big = scale_with(255, recipe("sword", Form::Sword, &[], Element::Light, spec.max_scale));
    assert!(big <= spec.max_scale + 1e-4);
    // Intensity too (temperament constructs).
    let mut wolf = recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2);
    wolf.temperament = Temperament::Aggressive;
    wolf.intensity = 0.5;
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = aim_of(&w, A);
    let id = cast_said(&mut w, A, wolf, "angry wolf", 255, aim);
    assert!((w.constructs.get(id).unwrap().recipe.intensity - 0.65).abs() < 1e-4);
}

// ------------------------------------------------------------------------------------ legendary

fn rich() -> Recipe {
    let mut r = recipe(
        "King Kong",
        Form::Kaiju,
        &[Mechanic::Blunt, Mechanic::Grab, Mechanic::Charge, Mechanic::Fast, Mechanic::Heavy],
        Element::Light,
        22.0,
    );
    r.temperament = Temperament::Aggressive;
    r.mount = Some(Box::new(recipe("skateboard", Form::Vehicle, &[Mechanic::Fast], Element::Light, 20.0)));
    r.normalized()
}

#[test]
fn repeated_phrase_never_rolls_in_10k_trials() {
    let t = LegendaryTuning::default();
    let mut set = NoveltySet::default();
    let k = PhraseKey::of("KING KONG ON A SKATEBOARD");
    legendary::record(&mut set, &k, A);
    for trial in 0..10_000u64 {
        let who = PlayerId((trial % 8) as u8);
        let uses = (trial % 3) as u32;
        let c = legendary::chance(&legendary::novelty(&set, &PhraseKey::of("king kong on a skateboard!!!"), &rich(), uses, &t), &t);
        assert_eq!(c, 0.0);
        assert!(!legendary::roll(trial * 7919, trial, who, k.key, c));
    }
    // In a world, with the chance forced to 100 %: the first cast is golden, repeats never.
    let mut w = duel_with(Some(1.0));
    for n in 0..40 {
        w.player_mut(A).unwrap().willpower = 100.0;
        let aim = aim_of(&w, A);
        let id = cast_said(&mut w, A, hammer(), "HAMMER!!!", 255, aim);
        assert_eq!(freedom::is_legendary(&w, id), n == 0, "cast {n}");
        w.tick(&[]);
    }
}

#[test]
fn novel_rich_phrases_land_in_their_band() {
    let t = LegendaryTuning::default();
    let set = NoveltySet::default();
    let key = PhraseKey::of("king kong on a skateboard");
    let n = legendary::novelty(&set, &key, &rich(), 0, &t);
    let c = legendary::chance(&n, &t);
    assert!((0.10..=0.12).contains(&c), "rich novel chance {c}");
    let plain = legendary::chance(&legendary::novelty(&set, &PhraseKey::of("hammer"), &hammer(), 0, &t), &t);
    assert!((0.01..0.03).contains(&plain), "plain novel chance {plain}");
    // Personal repetition of the same thing (other words) cuts it.
    let again = legendary::chance(&legendary::novelty(&set, &key, &rich(), 1, &t), &t);
    assert!(again < c * 0.3, "{again}");
    // Empirical rate over 10k rolls.
    let hits = (0..10_000u64).filter(|i| legendary::roll(42, 1000 + i * 3, A, key.key, c)).count() as f32 / 10_000.0;
    assert!((c - 0.02..=c + 0.02).contains(&hits), "rate {hits} vs chance {c}");
    // Determinism.
    for i in 0..100u64 {
        assert_eq!(legendary::roll(9, i, B, key.key, c), legendary::roll(9, i, B, key.key, c));
    }
}

#[test]
fn legendary_is_golden_modest_deterministic_and_replicas_agree() {
    let run = || {
        let mut w = duel_with(Some(1.0));
        w.player_mut(A).unwrap().willpower = 100.0;
        let aim = aim_of(&w, A);
        let n0 = w.events().len();
        let id = cast_said(
            &mut w,
            A,
            recipe("sword", Form::Sword, &[Mechanic::Blade], Element::Light, 1.0),
            "sword of a thousand truths",
            0,
            aim,
        );
        let evs = w.events()[n0..].to_vec();
        (w, id, evs)
    };
    let (w, id, evs) = run();
    assert!(freedom::is_legendary(&w, id));
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Freedom(FreedomEvent::Legendary { id: g, owner }) if *g == id && *owner == A)));
    let c = w.constructs.get(id).unwrap();
    assert!((c.recipe.scale - 1.2).abs() < 1e-4, "+20 % scale: {}", c.recipe.scale);
    // Modest durability bump vs the same cast without the roll.
    let mut plain = duel();
    plain.player_mut(A).unwrap().willpower = 100.0;
    let aim = aim_of(&plain, A);
    let pid = cast_said(
        &mut plain,
        A,
        recipe("sword", Form::Sword, &[Mechanic::Blade], Element::Light, 1.0),
        "sword of a thousand truths",
        0,
        aim,
    );
    let pc = plain.constructs.get(pid).unwrap();
    assert!(!freedom::is_legendary(&plain, pid));
    let ratio = c.max_integrity / pc.max_integrity;
    assert!(ratio > 1.0 && ratio < 1.7, "a bit more durable: ×{ratio}");
    // Deterministic.
    let (w2, id2, _) = run();
    assert_eq!(id, id2);
    assert_eq!(w.state_hash(), w2.state_hash());
    // A replica (client prediction world) fed the host's events + snapshot sees the gold.
    let mut replica = World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() });
    for e in &evs {
        replica.apply_event(e);
    }
    replica.apply_snapshot(&w.snapshot_for(Some(B)), Some(B));
    assert!(freedom::is_legendary(&replica, id), "replicated flag");
}

#[test]
fn default_rolls_are_deterministic_across_hosts() {
    let run = || {
        let mut w = duel_with(None);
        let mut gold = Vec::new();
        let words = ["dragon", "hammer", "shark", "cannon", "tornado", "anvil", "piano", "bees", "robot", "wall"];
        let els = [Element::Fire, Element::Ice, Element::Electric, Element::Water, Element::Light];
        for n in 0..120 {
            w.player_mut(A).unwrap().willpower = 100.0;
            let aim = aim_of(&w, A);
            let phrase = format!("{} {} {}", words[n % 10], words[(n / 10) % 10], n);
            let r = recipe(&phrase, Form::Quadruped, &[Mechanic::Charge, Mechanic::Flies, Mechanic::Breath], els[n % 5], 1.5);
            let id = cast_said(&mut w, A, r, &phrase, (n * 37 % 256) as u8, aim);
            if freedom::is_legendary(&w, id) {
                gold.push(n);
            }
            for _ in 0..3 {
                w.tick(&[]);
            }
        }
        (gold, w.state_hash())
    };
    let (g1, h1) = run();
    let (g2, h2) = run();
    assert_eq!(g1, g2);
    assert_eq!(h1, h2);
    // Rich, all-new phrases: a handful of the 120 are golden (never shown why).
    assert!(!g1.is_empty() && g1.len() < 40, "{} golden", g1.len());
}

// ------------------------------------------------------------------------------------ kill feed + highlights

#[test]
fn kill_feed_uses_the_phrase_and_highlights_close_the_match() {
    let mut w = duel();
    w.cfg.rules.ko_target = 1;
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = aim_of(&w, A);
    let kong = cast_said(&mut w, A, rich(), "KING KONG ON A SKATEBOARD", 255, aim);
    w.tick(&[]);
    // The kaiju lands on B, who then falls off the world.
    let bpos = w.player(B).unwrap().mv.pos;
    let mut hit = HitInfo::new(B, Some(A), HitKind::Creature, 30.0, 10.0, Vec3::Y, bpos);
    hit.construct = Some(kong);
    w.apply_hit(hit);
    w.player_mut(B).unwrap().mv.pos = Vec3::new(bpos.x, -500.0, bpos.z);
    let mut evs = Vec::new();
    for _ in 0..(1.0 / DT) as usize {
        evs.extend_from_slice(w.tick(&[]));
    }
    let feed: Vec<FeedLine> = evs
        .iter()
        .filter_map(|e| match e {
            GameEvent::Freedom(FreedomEvent::Feed(l)) => Some(l.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(feed.len(), 1, "{feed:?}");
    let l = &feed[0];
    assert_eq!((l.victim, l.killer), (B, Some(A)));
    assert_eq!(l.phrase, "KING KONG ON A SKATEBOARD");
    let text = freedom::feed_text(l, "Sam", Some("Jake"), |_| false);
    assert!(text.starts_with("Sam was ") && text.ends_with(" by KING KONG ON A SKATEBOARD"), "{text}");
    // ko_target 1 → the match ended with highlights.
    let h = evs
        .iter()
        .find_map(|e| match e {
            GameEvent::Freedom(FreedomEvent::Highlights(h)) => Some(h.clone()),
            _ => None,
        })
        .expect("highlights at match end");
    assert_eq!(h.wildest.first().map(|p| p.phrase.as_str()), Some("KING KONG ON A SKATEBOARD"));
    assert_eq!(h.ko_lines.len(), 1);
    assert_eq!(h.biggest.as_ref().map(|b| b.player), Some(A));
    assert_eq!(h.creative, Some((A, 1)));
    // Nothing in any event carries a chance or a score.
    let dbg = format!("{h:?}");
    assert!(!dbg.contains("score") && !dbg.contains("chance") && !dbg.contains("novel"));
}

#[test]
fn a_bots_kill_without_words_uses_the_construct_name() {
    // No cast meta (bots, tools): the recipe name stands in for the phrase.
    let mut w = duel();
    w.player_mut(A).unwrap().willpower = 100.0;
    let aim = aim_of(&w, A);
    let id = w.cast(A, recipe("flaming sword", Form::Sword, &[Mechanic::Blade], Element::Fire, 1.0), None, aim).unwrap();
    w.tick(&[]);
    let bpos = w.player(B).unwrap().mv.pos;
    let mut hit = HitInfo::new(B, Some(A), HitKind::Melee, 10.0, 5.0, Vec3::Y, bpos);
    hit.construct = Some(id);
    w.apply_hit(hit);
    w.player_mut(B).unwrap().mv.pos = Vec3::new(bpos.x, -500.0, bpos.z);
    let evs = w.tick(&[]).to_vec();
    let l = evs
        .iter()
        .find_map(|e| match e {
            GameEvent::Freedom(FreedomEvent::Feed(l)) => Some(l.clone()),
            _ => None,
        })
        .expect("feed line");
    assert_eq!(l.phrase, "flaming sword");
    assert_eq!(freedom::verb_text(l.verb), "torched");
}
