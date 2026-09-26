//! Mass-scaled motion: weighted swings/attacks stay sane and hit-matchable; footfalls; follow-through.

use bd_core::mechanics::{Form, FormCategory, Mechanic};
use bd_shapes::*;
use glam::{Affine3A, Vec3};
use proptest::prelude::*;

const KINDS: [SwingKind; 7] = [SwingKind::Primary, SwingKind::Heavy, SwingKind::Combo(0), SwingKind::Combo(1), SwingKind::Combo(2), SwingKind::Secondary, SwingKind::Throw];

fn check_curve(c: &SwingCurve) {
    let (a0, a1) = c.active_window();
    assert!(c.anticipation >= 0.0 && c.active >= 0.0 && c.recovery > 0.0, "{}", c.name);
    assert!(a1 <= c.duration() + 1e-5 && c.duration() < 1.6, "{} {}", c.name, c.duration());
    assert!(c.hold_at <= a1 + 1e-4);
    assert!(c.keys.windows(2).all(|w| w[0].t < w[1].t), "{} keys unsorted", c.name);
    assert!((c.keys.last().unwrap().t - c.duration()).abs() < 1e-3);
    for i in 0..=40 {
        assert!(c.sample(c.duration() * i as f32 / 40.0).is_finite());
    }
    assert!((c.sample(0.0).translation - c.sample(c.duration()).translation).length() < 1e-3, "{} doesn't return", c.name);
    assert!(c.sweep(a0, a1, 0.15).count() >= 1);
}

#[test]
fn weighted_swings() {
    for f in ALL_FORMS.iter().copied().filter(|f| f.category() == FormCategory::Held) {
        for k in KINDS {
            assert_eq!(swing_curve_weighted(f, k, 1.0), swing_curve(f, k), "weight 1 must be the authored curve");
            let light = swing_curve_weighted(f, k, 0.6);
            let heavy = swing_curve_weighted(f, k, 2.2);
            check_curve(&light);
            check_curve(&heavy);
            assert!(heavy.anticipation >= light.anticipation && heavy.recovery > light.recovery && heavy.duration() > light.duration(), "{}", heavy.name);
            assert!(heavy.active <= swing_curve(f, k).active * 1.001, "heavy strikes must stay snappy");
        }
    }
    // Weight from recipes.
    let mut anvil = demo_recipe(Form::Hammer, 1, &[Mechanic::Heavy, Mechanic::Blunt]);
    anvil.mass = 60.0;
    let mut rapier = demo_recipe(Form::Sword, 1, &[Mechanic::Fast]);
    rapier.mass = 1.5;
    let (wa, wr) = (weight_of(&anvil), weight_of(&rapier));
    assert!(wa > 1.5 && wr < 0.9, "{wa} {wr}");
    assert!(weight_of(&demo_recipe(Form::Kaiju, 1, &[])) >= 1.8);
}

#[test]
fn weighted_attacks_and_footfalls() {
    let (_, mut m) = build_model(&demo_recipe(Form::Kaiju, 1, &[]));
    for a in [Attack::Bite, Attack::Swipe, Attack::Stomp, Attack::TailSweep, Attack::Charge, Attack::Breath { aim: Vec3::NEG_Z }, Attack::Lash] {
        assert_eq!(attack_timing_hw(m.rig.height, a, 1.0), attack_timing(&m, a));
        let h = attack_timing_hw(m.rig.height, a, 2.2);
        let l = attack_timing_hw(m.rig.height, a, 0.6);
        assert!(h.total() >= l.total());
    }
    assert_eq!(attack_timing_hw(9.0, Attack::Charge, 2.2).anticipation, 0.4);
    m.weight = 2.2;
    assert_eq!(attack_timing(&m, Attack::Stomp), attack_timing_hw(m.rig.height, Attack::Stomp, 2.2));
    // A kaiju walking 10 m plants several heavy feet; a person's steps are light.
    let mut ev = Vec::new();
    let mut d = 0.0f32;
    for _ in 0..200 {
        footfalls(&m, 4.0, d, d + 0.05, &mut ev);
        d += 0.05;
    }
    assert!(ev.len() >= 4, "{} footfalls", ev.len());
    assert!(ev.iter().all(|f| f.impact > 2.0 && f.pos.is_finite()));
    let (_, p) = build_model(&demo_recipe(Form::Biped, 1, &[]));
    let mut ev2 = Vec::new();
    footfalls(&p, 6.0, 0.0, 10.0 / 40.0, &mut ev2);
    assert!(ev2.iter().all(|f| f.impact < 2.0));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]
    #[test]
    fn weight_and_secondary_safe(form in 0usize..39, w in prop_oneof![0.0f32..4.0, Just(f32::NAN), Just(f32::INFINITY)], seed in any::<u32>(), path in prop::collection::vec((-50.0f32..50.0, -50.0f32..50.0, -50.0f32..50.0, 0.0f32..0.2), 1..40), s in -1.0f32..3.0, t in -1.0f32..3.0) {
        let f = ALL_FORMS[form];
        for k in KINDS {
            let c = swing_curve_weighted(f, k, w);
            prop_assert!(c.duration().is_finite() && c.duration() < 1.6);
            prop_assert!(c.sample(c.duration() * 0.5).is_finite());
        }
        let (_, mut m) = build_model(&demo_recipe(f, seed, &[Mechanic::Flies, Mechanic::ArticulatedTail]));
        m.weight = if w.is_finite() { w } else { 1.0 };
        let _ = attack_timing_hw(m.rig.height, Attack::Stomp, w);
        let mut out = vec![Affine3A::IDENTITY; m.parts.len()];
        for a in [AnimState::HitReact { dir: Vec3::new(s, 0.0, 1.0), strength: s }, AnimState::Launched { dir: Vec3::new(1.0, s, 0.0), spin: s * 9.0 }, AnimState::Attack(Attack::Stomp)] {
            pose(&m, a, t, &mut out);
            prop_assert!(out.iter().all(|x| x.is_finite()));
        }
        let mut sec = SecondaryMotion::new(&m);
        for (x, y, z, dt) in path {
            sec.update(Affine3A::from_translation(Vec3::new(x, y, z)), dt);
        }
        pose(&m, AnimState::Idle, 0.3, &mut out);
        sec.apply(&m, &mut out);
        prop_assert!(out.iter().all(|x| x.is_finite()));
        let mut ev = Vec::new();
        footfalls(&m, s.abs() * 5.0, t, t + 1.0, &mut ev);
        prop_assert!(ev.iter().all(|e| e.impact.is_finite() && e.pos.is_finite()));
    }
}
