//! Behavioural tests of the mixer through the public API (synthetic sounds, offline blocks).

use std::sync::Arc;

use bd_audio::analysis::band_energies;
use bd_audio::cue::{CueSpec, CueTable};
use bd_audio::mix::{MixConfig, Snapshot};
use bd_audio::*;
use glam::Vec3;

const SR: u32 = 48000;

fn tone(freq: f32, secs: f32, amp: f32) -> Vec<f32> {
    (0..(secs * SR as f32) as usize).map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin()).collect()
}

fn noise(secs: f32, amp: f32, seed: u64) -> Vec<f32> {
    let mut r = bd_audio::dsp::Rng::new(seed);
    (0..(secs * SR as f32) as usize).map(|_| amp * r.range(-1.0, 1.0)).collect()
}

fn cue(bus: Bus) -> CueSpec {
    CueSpec { bus: Some(bus), pitch_cents: Some(0.0), gain_jitter_db: Some(0.0), ..Default::default() }
}

fn render(m: &mut Mixer, blocks: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(blocks * BLOCK * 2);
    let mut b = [0.0f32; BLOCK * 2];
    for _ in 0..blocks {
        m.render_block_into(&mut b);
        out.extend_from_slice(&b);
    }
    out
}

fn left(x: &[f32]) -> Vec<f32> {
    x.iter().step_by(2).copied().collect()
}

struct Setup {
    engine: AudioEngine,
    mixer: Mixer,
}

fn setup(sounds: Vec<(&str, Vec<f32>, bool, CueSpec)>, cfg: MixConfig) -> Setup {
    let mut bank = SoundBank::empty(SR);
    let mut cues = CueTable::default();
    for (name, pcm, looping, spec) in sounds {
        bank.push_mono(name, "test", pcm, looping);
        cues.sounds.insert(name.to_string(), spec);
    }
    let (mut engine, mixer) = AudioEngine::new(Arc::new(bank), &cues, cfg, SR, 7);
    engine.set_listener(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
    Setup { engine, mixer }
}

#[test]
fn abuse_never_produces_nan_or_denormals() {
    let mut s = setup(
        vec![
            ("a", tone(440.0, 0.5, 0.9), false, cue(Bus::Impacts)),
            ("b", noise(0.3, 1.0, 1), true, cue(Bus::Foley)),
            ("tiny", vec![0.5], false, cue(Bus::Feedback)),
        ],
        MixConfig::default(),
    );
    let e = &mut s.engine;
    e.play("a", PlayOpts::at(Vec3::new(f32::NAN, 0.0, 0.0)));
    e.play("a", PlayOpts::at(Vec3::new(1e30, 0.0, 0.0)).gain_db(f32::INFINITY));
    e.play("a", PlayOpts::at(Vec3::new(0.0, 0.0, -0.0)).pitch(1e9));
    e.play("b", PlayOpts::own().pitch(f32::NAN).gain_db(f32::NAN));
    e.play("tiny", PlayOpts::ui().occlusion(f32::NAN));
    e.set_listener(Vec3::splat(f32::NAN), Vec3::ZERO, Vec3::ZERO);
    let mut bad = MixConfig::default().base_params();
    bad.world_lpf_hz = f32::NAN;
    bad.bus[0].gain_db = f32::INFINITY;
    e.set_mix(bad);
    e.set_reverb(ReverbParams { rt60: f32::NAN, size: -5.0, ..Default::default() });
    e.duck(DuckSpec { depth_db: f32::NEG_INFINITY, release: f32::NAN, ..Default::default() });
    let out = render(&mut s.mixer, 3000);
    for x in &out {
        assert!(x.is_finite(), "non-finite output");
        assert!(!x.is_subnormal(), "denormal output {x:e}");
    }
    // Silence afterwards decays to exact zeros (no denormal tails).
    s.engine.stop_all(0.0);
    let tail = render(&mut s.mixer, 48000 / BLOCK * 3);
    assert!(tail.iter().all(|x| x.is_finite() && !x.is_subnormal()));
}

#[test]
fn limiter_holds_under_48_full_scale_voices() {
    let mut s = setup(vec![("loud", noise(1.0, 1.0, 3), false, CueSpec { gain_db: Some(12.0), ..cue(Bus::Impacts) })], {
        let mut c = MixConfig::default();
        c.hdr.enabled = false; // worst case: no window attenuation
        c
    });
    for i in 0..60 {
        s.engine.play("loud", PlayOpts::at(Vec3::new(i as f32 * 0.1, 0.0, -1.0)));
    }
    let out = render(&mut s.mixer, 700);
    let peak = out.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
    let ceiling = 10f32.powf(MixConfig::default().limiter.ceiling_db / 20.0);
    assert!(peak <= ceiling + 1e-6, "peak {peak} > ceiling {ceiling}");
    assert!(peak > 0.5, "limiter should be working hard, peak {peak}");
}

#[test]
fn deterministic_output() {
    let run = || {
        let mut s = setup(
            vec![("a", noise(0.5, 0.5, 9), false, CueSpec { pitch_cents: Some(40.0), gain_jitter_db: Some(1.5), ..cue(Bus::Impacts) })],
            MixConfig::default(),
        );
        for i in 0..20 {
            s.engine.play("a", PlayOpts::at(Vec3::new(i as f32, 0.0, -3.0)));
        }
        render(&mut s.mixer, 400)
    };
    let a = run();
    let b = run();
    assert_eq!(a.len(), b.len());
    assert!(a.iter().zip(b.iter()).all(|(x, y)| x.to_bits() == y.to_bits()));
}

#[test]
fn voice_budget_keeps_the_most_important() {
    let mut cfg = MixConfig::default();
    cfg.voices.max_real = 8;
    cfg.hdr.enabled = false;
    let mut s = setup(vec![("v", tone(300.0, 2.0, 0.2), false, CueSpec { max_instances: Some(64), ..cue(Bus::Impacts) })], cfg);
    let mut prio = std::collections::HashMap::new();
    for i in 0..20 {
        let h = s.engine.play("v", PlayOpts::at(Vec3::new(0.0, 0.0, -2.0)).priority(i as f32 * 2.0)).unwrap();
        prio.insert(h.0, i as f32 * 2.0);
    }
    render(&mut s.mixer, 20);
    let mut v = Vec::new();
    s.mixer.debug_voices(&mut v);
    assert_eq!(v.len(), 20, "all voices tracked (real + virtual)");
    let real: Vec<f32> = v.iter().filter(|x| x.1).map(|x| prio[&x.0]).collect();
    let virt: Vec<f32> = v.iter().filter(|x| !x.1).map(|x| prio[&x.0]).collect();
    assert_eq!(real.len(), 8);
    let min_real = real.iter().copied().fold(f32::INFINITY, f32::min);
    let max_virt = virt.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(min_real > max_virt, "real {real:?} virtual {virt:?}");
    let t = s.mixer.telemetry();
    assert_eq!((t.real, t.virt), (8, 12));
}

#[test]
fn only_the_closest_footsteps_play() {
    let mut s = setup(
        vec![("footstep", noise(1.0, 0.3, 5), false, CueSpec { max_instances: Some(3), priority: Some(20.0), ..cue(Bus::Foley) })],
        MixConfig::default(),
    );
    let mut dist = std::collections::HashMap::new();
    for i in 0..8 {
        let d = 4.0 + i as f32 * 5.0;
        let h = s.engine.play("footstep", PlayOpts::at(Vec3::new(0.0, 0.0, -d))).unwrap();
        dist.insert(h.0, d);
    }
    render(&mut s.mixer, 10);
    let mut v = Vec::new();
    s.mixer.debug_voices(&mut v);
    let mut real: Vec<f32> = v.iter().filter(|x| x.1).map(|x| dist[&x.0]).collect();
    real.sort_by(f32::total_cmp);
    assert_eq!(real, vec![4.0, 9.0, 14.0]);
}

#[test]
fn hdr_window_hides_quiet_sounds_under_a_blast_then_restores_them() {
    // Quiet bird loop at about -44 LUFS, a 0.6 s blast at about -6 LUFS.
    let mut s = setup(
        vec![
            ("bird", tone(3000.0, 1.0, 0.01), true, CueSpec { priority: Some(10.0), ..cue(Bus::Ambience) }),
            ("blast", noise(0.6, 0.9, 11), false, CueSpec { priority: Some(90.0), ..cue(Bus::Impacts) }),
        ],
        MixConfig::default(),
    );
    let bird = s.engine.play("bird", PlayOpts::at(Vec3::new(0.0, 0.0, -3.0))).unwrap();
    render(&mut s.mixer, 100);
    let is_real = |m: &Mixer| {
        let mut v = Vec::new();
        m.debug_voices(&mut v);
        v.iter().find(|x| x.0 == bird.0).map(|x| x.1)
    };
    assert_eq!(is_real(&s.mixer), Some(true), "audible in quiet");
    s.engine.play("blast", PlayOpts::at(Vec3::new(0.0, 0.0, -3.0)));
    render(&mut s.mixer, 60);
    assert_eq!(is_real(&s.mixer), Some(false), "culled below the window floor during the blast");
    let t = s.mixer.telemetry();
    assert!(t.hdr_gain_db < -3.0, "window pushed the world down: {t:?}");
    // Blast ends + window release → the bird comes back (virtual voice resumes).
    render(&mut s.mixer, 48000 * 4 / BLOCK);
    assert_eq!(is_real(&s.mixer), Some(true), "restored after the window released");
}

#[test]
fn feedback_sidechain_carves_the_world_high_band() {
    let mut s = setup(
        vec![
            ("bed", noise(2.0, 0.3, 21), true, CueSpec { hdr: Some(false), ..cue(Bus::Impacts) }),
            ("confirm", tone(8000.0, 0.4, 0.5), false, CueSpec { hdr: Some(false), ..cue(Bus::Feedback) }),
        ],
        MixConfig::default(),
    );
    s.mixer.enable_stems();
    s.engine.play("bed", PlayOpts::at(Vec3::new(0.0, 0.0, -2.0)));
    let mut world = Vec::new();
    let mut b = [0.0f32; BLOCK * 2];
    for i in 0..600 {
        if i == 300 {
            s.engine.play("confirm", PlayOpts::ui());
        }
        s.mixer.render_block_into(&mut b);
        world.extend(s.mixer.stems().unwrap().world_out.iter().step_by(2).copied());
    }
    let before = &world[200 * BLOCK..280 * BLOCK];
    let during = &world[310 * BLOCK..390 * BLOCK];
    let band = [(5000.0, 12000.0), (200.0, 1000.0)];
    let eb = band_energies(before, SR, &band);
    let ed = band_energies(during, SR, &band);
    let hi_drop = 10.0 * (ed[0] / eb[0]).log10();
    let mid_drop = 10.0 * (ed[1] / eb[1]).log10();
    assert!(hi_drop < -6.0, "5-12 kHz carved: {hi_drop:.1} dB");
    assert!(mid_drop < -2.0 && mid_drop > hi_drop, "broadband duck smaller than the HF carve: mid {mid_drop:.1} hi {hi_drop:.1}");
}

#[test]
fn occlusion_darkens_and_attenuates() {
    let measure = |occ: f32| {
        let mut s = setup(vec![("n", noise(1.0, 0.3, 4), true, CueSpec { hdr: Some(false), ..cue(Bus::Impacts) })], MixConfig::default());
        s.engine.play("n", PlayOpts::at(Vec3::new(0.0, 0.0, -5.0)).occlusion(occ));
        let out = render(&mut s.mixer, 600);
        band_energies(&left(&out[200 * BLOCK * 2..]), SR, &[(4000.0, 16000.0), (100.0, 400.0)])
    };
    let clear = measure(0.0);
    let walled = measure(1.0);
    let hi = 10.0 * (walled[0] / clear[0]).log10();
    let lo = 10.0 * (walled[1] / clear[1]).log10();
    assert!(hi < lo - 10.0, "highs lose far more than lows: hi {hi:.1} lo {lo:.1}");
    assert!(lo < -3.0, "also quieter overall: {lo:.1}");
}

#[test]
fn distance_attenuates_and_rolls_off_highs() {
    let measure = |d: f32| {
        let mut s =
            setup(vec![("n", noise(1.0, 0.3, 4), true, CueSpec { hdr: Some(false), max_dist: Some(400.0), ..cue(Bus::Impacts) })], {
                let mut c = MixConfig::default();
                c.reverb.wet_db = -90.0;
                c.reverb.er_db = -90.0;
                c
            });
        s.engine.play("n", PlayOpts::at(Vec3::new(0.0, 0.0, -d)));
        let out = render(&mut s.mixer, 600);
        band_energies(&left(&out[200 * BLOCK * 2..]), SR, &[(6000.0, 16000.0), (100.0, 400.0)])
    };
    let near = measure(3.0);
    let far = measure(120.0);
    let lo = 10.0 * (far[1] / near[1]).log10();
    let hi = 10.0 * (far[0] / near[0]).log10();
    assert!(lo < -20.0, "{lo}");
    assert!(hi < lo - 6.0, "air absorption: hi {hi:.1} vs lo {lo:.1}");
}

struct NoiseMusic(bd_audio::dsp::Rng);
impl MusicSource for NoiseMusic {
    fn render(&mut self, out: &mut [f32]) {
        for x in out.iter_mut() {
            *x = self.0.range(-0.2, 0.2);
        }
    }
    fn command(&mut self, _c: MusicCommand) {}
}

#[test]
fn music_bus_is_carved_out_of_sub_and_feedback_band() {
    let mut s = setup(vec![], MixConfig::default());
    s.mixer.enable_stems();
    s.engine.set_music(Box::new(NoiseMusic(bd_audio::dsp::Rng::new(5))));
    let mut music = Vec::new();
    let mut b = [0.0f32; BLOCK * 2];
    for _ in 0..800 {
        s.mixer.render_block_into(&mut b);
        music.extend(s.mixer.stems().unwrap().bus[Bus::Music.index()].iter().step_by(2).copied());
    }
    let e = band_energies(&music[100 * BLOCK..], SR, &[(20.0, 45.0), (6000.0, 10000.0), (800.0, 2000.0)]);
    // White noise has equal energy per Hz: normalise by bandwidth.
    let per_hz = |i: usize, w: f64| e[i] / w;
    let sub = 10.0 * (per_hz(0, 25.0) / per_hz(2, 1200.0)).log10();
    let hf = 10.0 * (per_hz(1, 4000.0) / per_hz(2, 1200.0)).log10();
    assert!(sub < -6.0, "sub carved: {sub:.1} dB");
    assert!(hf < -5.0, "5-12 kHz carved: {hf:.1} dB");
}

#[test]
fn director_crossfades_danger_snapshot() {
    let mut cfg = MixConfig::default();
    cfg.snapshots.insert("Danger".into(), Snapshot { fade_in: 1.0, fade_out: 1.0, world_lpf_hz: Some(800.0), ..Default::default() });
    let mut s = setup(vec![], cfg.clone());
    let mut d = MixDirector::new();
    let mut inp = MixInputs { scene: Scene::InGame, damage_pct: 200.0, ..Default::default() };
    let mut prev = f32::INFINITY;
    let mut w = Vec::new();
    for _ in 0..90 {
        d.update(1.0 / 60.0, &inp, &cfg, &mut s.engine);
        assert!(d.current.world_lpf_hz <= prev + 1e-3, "monotonic");
        prev = d.current.world_lpf_hz;
        w.push(d.weights().danger);
    }
    // Fades over ~1 s, not a jump.
    assert!(w[10] > 0.1 && w[10] < 0.3, "{}", w[10]);
    assert!((w[80] - 1.0).abs() < 1e-3);
    assert!((d.current.world_lpf_hz - 800.0).abs() < 1.0);
    inp.damage_pct = 0.0;
    for _ in 0..90 {
        d.update(1.0 / 60.0, &inp, &cfg, &mut s.engine);
    }
    assert!(d.weights().danger < 1e-3);
}
