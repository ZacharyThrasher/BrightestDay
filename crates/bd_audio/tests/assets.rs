//! The shipped data files parse, and every sound they name exists in the manifest.

use std::collections::BTreeSet;
use std::path::PathBuf;

use bd_audio::{CueTable, MapAudioTable, MixConfig};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn manifest_keys() -> BTreeSet<String> {
    #[derive(serde::Deserialize)]
    struct M {
        sounds: std::collections::BTreeMap<String, ron::Value>,
    }
    let t = std::fs::read_to_string(assets().join("sfx/manifest.ron")).unwrap();
    let m: M = ron::from_str(&t).unwrap();
    m.sounds.into_keys().collect()
}

#[test]
fn audio_tables_parse_and_reference_real_sounds() {
    let dir = assets().join("audio");
    let cues = CueTable::load(&dir.join("cues.ron")).unwrap();
    let mix = MixConfig::load(&dir.join("mix.ron")).unwrap();
    let maps = MapAudioTable::load(&dir.join("maps.ron")).unwrap();
    let keys = manifest_keys();
    let mut missing = Vec::new();
    for k in cues.sounds.keys() {
        if !keys.contains(k) {
            missing.push(format!("cues.ron: {k}"));
        }
    }
    for (map, a) in &maps.0 {
        let names = a.beds.iter().map(|b| &b.sound).chain(a.loops.iter().map(|l| &l.sound)).chain(a.sparkles.iter().map(|s| &s.sound));
        for n in names {
            if !keys.contains(n) {
                missing.push(format!("maps.ron {map}: {n}"));
            }
        }
    }
    for s in ["heartbeat", "final_blow_suck"] {
        if !keys.contains(s) {
            missing.push(format!("director: {s}"));
        }
    }
    assert!(missing.is_empty(), "unknown sounds: {missing:#?}");
    for s in ["Exploration", "Combat", "Danger", "KO", "RingOpen", "Space", "Menu", "Lobby"] {
        assert!(mix.snapshots.contains_key(s), "snapshot {s}");
    }
    for d in ["heavy_hit", "ko_confirm", "final_blow"] {
        assert!(mix.ducks.contains_key(d), "duck {d}");
    }
    for m in ["archipelago", "miami", "tutorial", "orbit", "cybertron"] {
        assert!(maps.0.contains_key(m), "map {m}");
    }
}
