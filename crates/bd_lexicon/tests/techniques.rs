//! Techniques (powers.ron `kind: Technique`): saying the name resolves to the POWER (a grant), never
//! to a construct — including tier names ("super saiyan blue" → `super_saiyan@5`) and voice-misheard
//! variants. Runs against the real lexicon + the shipped powers.ron.

mod common;

fn grants(p: &str) -> Option<String> {
    common::lex().map(|l| l.resolve(p).recipe.grants.unwrap_or_default())
}

#[test]
fn technique_names_grant_the_power() {
    if common::lex().is_none() {
        return;
    }
    for (p, want) in [
        ("ultra instinct", "ultra_instinct"),
        ("Ultra Instinct!", "ultra_instinct"),
        ("mastered ultra instinct", "ultra_instinct@1"),
        ("super saiyan", "super_saiyan"),
        ("ssj", "super_saiyan"),
        ("super saiyan 2", "super_saiyan@1"),
        ("super saiyan 3", "super_saiyan@2"),
        ("super saiyan god", "super_saiyan@3"),
        ("super saiyan blue", "super_saiyan@4"),
        ("kaioken", "kaioken"),
        ("kaio-ken", "kaioken"),
        ("kaio-ken times ten", "kaioken@4"),
        ("kaioken times 10", "kaioken@4"),
        ("kaioken x10", "kaioken@4"),
        ("kaioken x20", "kaioken@5"),
        ("sage mode", "sage_mode"),
        ("kurama mode", "kurama_mode"),
        ("nine tails chakra mode", "kurama_mode"),
        ("sharingan", "sharingan"),
        ("mangekyo sharingan", "sharingan@1"),
        ("rinnegan", "rinnegan"),
        ("shadow clone jutsu", "shadow_clone"),
        ("limitless", "limitless"),
        ("infinity", "limitless"),
        ("black flash technique", "black_flash_technique"),
        ("kokusen", "black_flash_technique"),
        ("sukuna's shrine", "shrine"), // the bare "sukuna" stays the costume summon ("sukuna <move>" = the kit)
        ("ten shadows", "ten_shadows"),
        ("bankai", "bankai"),
        ("nen", "nen"),
        ("nen jajanken", "nen@1"),
        ("nen specialist", "nen@6"),
        ("transmuter", "nen@3"),
        ("armament haki", "armament_haki"),
        ("busoshoku haki", "armament_haki"),
    ] {
        assert_eq!(grants(p).as_deref(), Some(want), "{p:?}");
    }
}

#[test]
fn voice_misheard_variants_still_grant() {
    if common::lex().is_none() {
        return;
    }
    for (p, want) in [
        ("ultra instincts", "ultra_instinct"),
        ("super sayan", "super_saiyan"),
        ("super saiyen blue", "super_saiyan@4"),
        ("kayo ken", "kaioken"),
        ("kai o ken", "kaioken"),
        ("sharing an", "sharingan"),
        ("rinne gan", "rinnegan"),
        ("rinegan", "rinnegan"),
        ("ban kai", "bankai"),
        ("bon kai", "bankai"),
        ("rock paper scissors", "nen@1"),
        ("sage mood", "sage_mode"),
        ("kyuubi mode", "kurama_mode"),
    ] {
        assert_eq!(grants(p).as_deref(), Some(want), "{p:?}");
    }
}

#[test]
fn power_up_phrases_are_the_cosmetic_ascend() {
    if common::lex().is_none() {
        return;
    }
    for p in ["power up", "powering up", "full power", "maximum power"] {
        assert_eq!(grants(p).as_deref(), Some("ascend:power_up"), "{p:?}");
    }
}

#[test]
fn everyday_words_stay_everyday() {
    if common::lex().is_none() {
        return;
    }
    // Everyday words never become power grants (form names do: see below).
    for p in ["hammer", "red", "blue", "rock", "paper", "scissors", "shrine", "gear", "second", "fifth"] {
        let g = grants(p).unwrap_or_default();
        assert!(!g.starts_with("gomu_gomu") && !g.starts_with("limitless") && !g.starts_with("nen"), "{p:?} -> {g}");
    }
}

/// A FORM name of any power (not only techniques) makes you that form: "gear 5" is Luffy's Gear
/// Fifth (the fruit comes with it), never the gauntlet icon or a signature ultimate — typed or voiced.
#[test]
fn form_names_of_any_power_grant_that_form() {
    let Some(l) = common::lex() else { return };
    for (p, want) in [
        ("gear 5", "gomu_gomu@4"),
        ("gear five", "gomu_gomu@4"),
        ("gear fifth", "gomu_gomu@4"),
        ("GEAR 5!!!", "gomu_gomu@4"),
        ("gear second", "gomu_gomu@1"),
        ("gear 3", "gomu_gomu@2"),
        ("gear fourth", "gomu_gomu@3"),
    ] {
        let r = l.resolve(p);
        assert_eq!(r.recipe.grants.as_deref(), Some(want), "{p:?}: {:?}", r.explanation);
        assert!(r.signature.is_none(), "{p:?} is the form, not a signature ultimate");
    }
    for p in ["gear five", "gear 5", "gear fifth", "gear 5!"] {
        assert_eq!(l.resolve_spoken(p).recipe.grants.as_deref(), Some("gomu_gomu@4"), "spoken {p:?}");
    }
    // The gauntlet only by object phrasing.
    for p in ["gear 5 gauntlet", "gear 5 fist"] {
        let r = l.resolve(p);
        assert!(r.recipe.grants.is_none(), "{p:?}");
        assert_eq!(r.recipe.form, bd_core::mechanics::Form::Gauntlet, "{p:?}");
    }
}
