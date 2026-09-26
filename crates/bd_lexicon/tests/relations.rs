//! Relations layer (src/relations.rs + data/relations.ron): size references, magnitude words, mounts,
//! emitters / sky delivery, volleys, guided words, and leftover words becoming light grafts.

mod common;

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Temperament};
use bd_lexicon::{Resolution, ResolutionKind};

fn r(p: &str) -> Option<Resolution> {
    common::lex().map(|l| l.resolve(p))
}

fn has(res: &Resolution, m: Mechanic) -> bool {
    res.recipe.mechanics.contains(&m)
}

#[test]
fn size_relations() {
    let Some(d) = r("T-REX SIZED DUCK") else { return };
    assert_eq!(d.recipe.form, Form::Bird, "{d:?}");
    assert!((9.0..=15.0).contains(&d.recipe.scale), "t-rex sized duck ~12 m, got {}", d.recipe.scale);
    assert!(!has(&d, Mechanic::Stomp) && !has(&d, Mechanic::ArticulatedTail), "X only lends its size: {:?}", d.recipe.mechanics);
    assert!(d.ignored.is_empty(), "{:?}", d.ignored);

    let h = r("duck the size of a house").unwrap();
    assert_eq!(h.recipe.form, Form::Bird);
    assert!((4.0..=10.0).contains(&h.recipe.scale), "house-sized duck, got {}", h.recipe.scale);
    assert!(!has(&h, Mechanic::DropFromSky), "house mechanics must not graft: {:?}", h.recipe.mechanics);

    let b = r("cat bigger than a horse").unwrap();
    let horse = r("horse").unwrap();
    assert_eq!(b.recipe.form, Form::Quadruped);
    assert!((b.recipe.scale - horse.recipe.scale * 1.5).abs() < 0.2, "{} vs {}", b.recipe.scale, horse.recipe.scale);
}

#[test]
fn magnitude_words() {
    let Some(m) = r("PLANETARY DESTROYING METEOR") else { return };
    assert_eq!(m.recipe.form, Form::Boulder);
    assert!(m.recipe.scale >= 40.0 && has(&m, Mechanic::DropFromSky), "{:?}", m.recipe);
    assert!(m.ignored.is_empty(), "{:?}", m.ignored);

    let g = r("GRAVITY NUCLEAR BOMB").unwrap();
    let plain = r("bomb").unwrap();
    assert!(has(&g, Mechanic::Explodes) && has(&g, Mechanic::Attract), "{:?}", g.recipe.mechanics);
    assert_eq!(g.recipe.element, Element::Atomic);
    assert!(g.recipe.scale >= plain.recipe.scale * 2.5, "nuclear = bigger blast: {} vs {}", g.recipe.scale, plain.recipe.scale);

    let micro = r("micro shark").unwrap();
    let shark = r("shark").unwrap();
    assert!(micro.recipe.scale < shark.recipe.scale * 0.5);

    let homing = r("heat seeking missile").unwrap();
    assert!(has(&homing, Mechanic::Homing));
    let lg = r("laser guided bomb").unwrap();
    assert!(has(&lg, Mechanic::Homing) && has(&lg, Mechanic::Explodes), "{:?}", lg.recipe.mechanics);
}

#[test]
fn mounts() {
    let Some(k) = r("KING KONG ON A SKATEBOARD") else { return };
    assert_eq!(k.recipe.form, Form::Kaiju, "king kong stays primary");
    let m = k.recipe.mount.as_ref().expect("skateboard mount");
    assert_eq!(m.name, "skateboard");
    assert!(m.scale > k.recipe.scale * 0.5, "mount scaled to the rider");
    assert!(has(&k, Mechanic::Fast) && !has(&k, Mechanic::Stomp), "rolls instead of stomping: {:?}", k.recipe.mechanics);
    assert!(k.shape.is_some(), "the icon model is kept");

    let g = r("skateboarding gorilla").unwrap();
    assert!(g.recipe.name.contains("gorilla"));
    assert!(g.recipe.mount.as_ref().is_some_and(|m| m.name == "skateboard"), "{:?}", g.recipe.mount);
    assert!(g.ignored.is_empty(), "{:?}", g.ignored);

    for (p, mount) in [
        ("knight riding a horse", "horse"),
        ("gorilla in a tank", "tank"),
        ("witch on a broom", "broom"),
        ("shark riding a jet ski", "jet ski"),
        ("skateboard riding gorilla", "skateboard"),
    ] {
        let x = r(p).unwrap();
        assert!(x.recipe.mount.as_ref().is_some_and(|m| m.name == mount), "{p}: {:?}", x.recipe.mount);
    }
    // mounts only when rideable; idioms and names are left alone
    for p in
        ["cat on a hot tin roof", "iron man", "man of steel", "on fire", "ride the lightning", "boy with a dog", "dragon riding a knight"]
    {
        let x = r(p).unwrap();
        assert!(x.recipe.mount.is_none(), "{p}: unexpected mount {:?}", x.recipe.mount);
    }
    assert_eq!(r("iron man").unwrap().recipe.name, "Iron Man Suit");
    assert_eq!(r("man of steel").unwrap().recipe.name, "Superman");
    let fire = r("on fire").unwrap();
    assert_eq!(fire.kind, ResolutionKind::Graft);
    assert_eq!(fire.recipe.element, Element::Fire);
}

/// The sky-emitter shape the sim's downward strike keys on: a non-Held emitter with
/// Flies + DropFromSky + (Beam | Shoots | Breath) hovers high above the aim point and fires down.
fn sky_emitter(x: &Resolution) -> bool {
    has(x, Mechanic::Flies)
        && has(x, Mechanic::DropFromSky)
        && (has(x, Mechanic::Beam) || has(x, Mechanic::Shoots) || has(x, Mechanic::Breath))
}

#[test]
fn emitters_and_sky_delivery() {
    let Some(s) = r("SATELLITE DEATH BEAM") else { return };
    assert_eq!(s.recipe.name, "satellite", "{:?}", s.recipe);
    assert!(sky_emitter(&s), "{:?}", s.recipe.mechanics);
    assert!(!has(&s, Mechanic::Returning));
    assert_eq!(s.recipe.element, Element::Shadow);
    assert!(s.recipe.features.iter().any(|f| format!("{f:?}") == "Dish"));

    let o = r("orbital laser").unwrap();
    assert!(has(&o, Mechanic::Beam) && has(&o, Mechanic::DropFromSky), "{:?}", o.recipe.mechanics);
    let sl = r("space laser").unwrap();
    assert!(sky_emitter(&sl), "{:?}", sl.recipe);

    let a = r("airstrike").unwrap();
    assert!(has(&a, Mechanic::DropFromSky) && has(&a, Mechanic::Explodes), "{:?}", a.recipe.mechanics);

    let m = r("missile strike").unwrap();
    assert!(m.recipe.count >= 3 && has(&m, Mechanic::DropFromSky) && has(&m, Mechanic::Explodes), "{:?}", m.recipe);

    let e = r("eye laser").unwrap();
    assert!(has(&e, Mechanic::Beam) && e.recipe.form.category() != FormCategory::Held, "{:?}", e.recipe);

    // no spawnable X: the handheld guns stay guns
    for p in ["laser gun", "ray gun", "death ray", "sword beam"] {
        let x = r(p).unwrap();
        assert_eq!(x.recipe.form, Form::Gun, "{p}: {:?}", x.recipe);
    }
    // sky / space / rain words in ordinary compounds are not relations
    for p in ["sky blue", "space bar", "rain boots"] {
        let x = r(p).unwrap();
        assert!(!has(&x, Mechanic::DropFromSky) && x.recipe.count == 1, "{p}: {:?}", x.recipe);
    }
}

#[test]
fn volleys() {
    let Some(a) = r("rain of arrows") else { return };
    assert!(a.recipe.count >= 8 && has(&a, Mechanic::DropFromSky), "{:?}", a.recipe);
    assert!(matches!(a.recipe.form, Form::Spear), "{:?}", a.recipe.form);
    let m = r("meteor shower").unwrap();
    assert_eq!(m.recipe.form, Form::Boulder);
    assert_eq!(m.recipe.element, Element::Fire);
    assert!(m.recipe.count >= 8 && has(&m, Mechanic::DropFromSky), "{:?}", m.recipe);
    let b = r("baby shower").unwrap();
    assert!(!has(&b, Mechanic::DropFromSky), "a party, not falling babies: {:?}", b.recipe);
}

#[test]
fn leftover_words_become_light_grafts() {
    let Some(d) = r("destroying robot") else { return };
    assert!(d.ignored.is_empty(), "{:?}", d.ignored);
    assert!(has(&d, Mechanic::Explodes) || has(&d, Mechanic::Heavy), "{:?}", d.recipe.mechanics);
    assert!(d.explanation.iter().any(|e| e.contains("leftover")));

    let l = r("lumbering bear").unwrap();
    assert!(l.ignored.is_empty() && has(&l, Mechanic::Heavy), "{:?}", l);

    let f = r("furious shark").unwrap();
    assert_eq!(f.recipe.temperament, Temperament::Aggressive, "{:?}", f.recipe);

    let g = r("glimmering sword").unwrap();
    assert_eq!(g.recipe.form, Form::Sword);
    assert!(g.ignored.is_empty(), "{:?}", g.ignored);

    let alone = r("destroying").unwrap();
    assert_eq!(alone.kind, ResolutionKind::Graft);
    assert!(alone.confidence >= 0.5 && has(&alone, Mechanic::Explodes));

    // gibberish still reported, never invented
    let gib = r("zqxv robot").unwrap();
    assert_eq!(gib.recipe.form, Form::Biped);
}

#[test]
fn relation_timings() {
    let Some(lex) = common::lex() else { return };
    let phrases = [
        "T-REX SIZED DUCK",
        "PLANETARY DESTROYING METEOR",
        "GRAVITY NUCLEAR BOMB",
        "KING KONG ON A SKATEBOARD",
        "duck the size of a house",
        "skateboarding gorilla",
        "SATELLITE DEATH BEAM",
        "orbital laser",
        "airstrike",
        "rain of arrows",
        "meteor shower",
        "missile strike",
        "destroying robot",
    ];
    let mut times = Vec::new();
    for _ in 0..3 {
        for p in phrases {
            let t = std::time::Instant::now();
            let _ = lex.resolve(p);
            times.push(t.elapsed().as_secs_f64() * 1e3);
        }
    }
    times.sort_by(|a, b| a.total_cmp(b));
    let p99 = times[(times.len() * 99 / 100).min(times.len() - 1)];
    eprintln!("relations resolve: median {:.2} ms, p99 {:.2} ms", times[times.len() / 2], p99);
    assert!(p99 < 50.0, "relations too slow: p99 {p99:.1} ms");
}
