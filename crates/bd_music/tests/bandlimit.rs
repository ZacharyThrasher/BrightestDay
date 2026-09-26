//! The oscillators are band-limited: a sustained high pulse / saw / triangle / FM note has its
//! energy on the harmonic series, with aliased (inharmonic) components far below the loudest
//! harmonic. Measured with a Blackman-Harris window (-92 dB sidelobes), ±8 bins around each harmonic
//! excluded.

use bd_music::analysis::spectrum_db_blackman_harris;
use bd_music::synth::{CONTROL, Instrument, Voice, Wave, midi_hz};

const SR: f32 = 48_000.0;
const N: usize = 16_384;

/// Worst inharmonic peak (dB relative to the strongest harmonic) below 20 kHz for one sustained note.
fn worst_alias_db(inst: Instrument, key: f32) -> f32 {
    let mut v = Voice::new(SR);
    v.note_on(&inst, &[key], 1.0, false);
    let mut buf = [0.0f32; CONTROL];
    let mut mono = Vec::new();
    for _ in 0..(N / CONTROL + 128) {
        v.render(&mut buf);
        mono.extend_from_slice(&buf);
    }
    let col = spectrum_db_blackman_harris(&mono[mono.len() - N..]);
    let f0 = midi_hz(key);
    let bin_hz = SR / N as f32;
    let top = col.iter().copied().fold(-200.0f32, f32::max);
    let mut worst = -200.0f32;
    for (k, &db) in col.iter().enumerate() {
        let f = k as f32 * bin_hz;
        if !(40.0..20_000.0).contains(&f) {
            continue;
        }
        let h = (f / f0).round().max(1.0);
        if (f - h * f0).abs() > 8.0 * bin_hz {
            worst = worst.max(db - top);
        }
    }
    worst
}

#[test]
fn oscillators_do_not_alias() {
    let sustain = Instrument { sustain: 1.0, decay: 0.0, vib_depth: 0.0, lp_hz: 0.0, ..Instrument::default() };
    let cases = [
        // (name, instrument, key, limit dB) — the highest notes any track writes for that voice.
        ("pulse 12.5% A5", Instrument { wave: Wave::Pulse, duty: 0.125, duty_end: 0.125, ..sustain }, 81.0, -48.0),
        ("pulse 25% E6", Instrument { wave: Wave::Pulse, duty: 0.25, duty_end: 0.25, ..sustain }, 88.0, -46.0),
        ("pulse 25% C7", Instrument { wave: Wave::Pulse, duty: 0.25, duty_end: 0.25, ..sustain }, 96.0, -40.0),
        ("pulse 50% E6", Instrument { wave: Wave::Pulse, duty: 0.5, duty_end: 0.5, ..sustain }, 88.0, -48.0),
        ("saw A4", Instrument { wave: Wave::Saw, ..sustain }, 69.0, -55.0),
        ("triangle A3", Instrument { wave: Wave::Triangle, ..sustain }, 57.0, -70.0),
        ("triangle D6", Instrument { wave: Wave::Triangle, ..sustain }, 86.0, -55.0),
        (
            "hard light FM E6",
            Instrument { wave: Wave::Fm, fm_ratio: 1.0, fm_index: 2.4, fm_index_end: 2.4, fm_detune: 0.0, ..sustain },
            88.0,
            -60.0,
        ),
    ];
    let mut failures = Vec::new();
    for (name, inst, key, limit) in cases {
        let db = worst_alias_db(inst, key);
        println!("{name:18} worst inharmonic component {db:6.1} dB (limit {limit})");
        if db > limit {
            failures.push(format!("{name}: {db:.1} dB > {limit}"));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}
