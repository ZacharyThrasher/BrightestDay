//! Never panic, always a valid recipe, always fast — for ANY input (typed, spoken, completion).

mod common;

use proptest::prelude::*;

fn valid(lex: &bd_lexicon::Lexicon, s: &str) {
    for r in [lex.resolve(s), lex.resolve_spoken(s)] {
        let rc = &r.recipe;
        assert!(rc.scale.is_finite() && rc.scale > 0.0, "{s:?}");
        assert!(rc.mass.is_finite() && rc.mass > 0.0, "{s:?}");
        assert!(rc.count >= 1);
        assert!(rc.intensity.is_finite() && (0.0..=1.0).contains(&rc.intensity));
        assert!(r.confidence.is_finite() && (0.0..=1.0).contains(&r.confidence));
        assert!(
            rc.mechanics.windows(2).all(|w| w[0] < w[1]),
            "mechanics sorted/unique"
        );
        let _ = r.signature().key();
    }
    let _ = lex.complete(s);
    let _ = lex.is_blocked_name(s);
}

#[test]
fn hostile_inputs() {
    let Some(lex) = common::lex() else { return };
    let long = "hammer ".repeat(15_000);
    let zalgo = "h\u{0336}\u{0337}a\u{0338}\u{0489}m\u{0335}m\u{0334}e\u{0336}r\u{0337}".repeat(50);
    let cases: Vec<String> = vec![
        String::new(),
        " ".into(),
        "\t\n\r\0".into(),
        "!!!???...,,,".into(),
        "🔥🔨🐝💥".into(),
        "مطرقة نار".into(),
        "\u{202E}remmah\u{202C}".into(),
        "日本語のハンマー".into(),
        zalgo,
        long,
        "a".repeat(100_000),
        "'s's's's".into(),
        "of of of with with that that".into(),
        "make it make it make it".into(),
        "shooting shooting shooting".into(),
        "999999999999999999999 hammers".into(),
        "\u{0000}\u{FFFF}\u{10FFFF}".into(),
        "the the the the the the the the the the the the the the the the the the".into(),
        "x".into(),
        "-".into(),
        "'".into(),
        "   GIANT   ".into(),
    ];
    for c in &cases {
        let t = std::time::Instant::now();
        valid(lex, c);
        assert!(
            t.elapsed().as_millis() < 250,
            "{:?} took {:?}",
            c.chars().take(40).collect::<String>(),
            t.elapsed()
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, failure_persistence: None, .. ProptestConfig::default() })]

    #[test]
    fn any_unicode_string(s in "\\PC{0,200}") {
        if let Some(lex) = common::lex() { valid(lex, &s); }
    }

    #[test]
    fn wordy_strings(words in proptest::collection::vec("[a-zA-Z']{1,12}", 0..24)) {
        if let Some(lex) = common::lex() { valid(lex, &words.join(" ")); }
    }
}
