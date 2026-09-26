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
        ("black flash", "black_flash"),
        ("sukuna", "shrine"),
        ("ten shadows", "ten_shadows"),
        ("bankai", "bankai"),
        ("nen", "nen"),
        ("jajanken", "nen@1"),
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
        ("ja jan ken", "nen@1"),
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
    // Non-technique tier names never become lexicon entries (a held gomu fruit parses them itself).
    for p in ["gear second", "hammer", "red", "blue", "rock", "paper", "scissors", "shrine"] {
        let g = grants(p).unwrap_or_default();
        assert!(!g.starts_with("gomu_gomu") && !g.starts_with("limitless") && !g.starts_with("nen"), "{p:?} -> {g}");
    }
}
