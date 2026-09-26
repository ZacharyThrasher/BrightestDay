//! Player behaviour: determinism, stems, seamless loops, bar-line intensity changes, stinger
//! quantisation, tempo, and the shipped soundtrack's integrity.

use std::sync::Arc;

use bd_music::{AdaptPhase, MusicCommand, MusicLibrary, MusicPlayer, RemixStyle, StingerKind, Style};

const SR: u32 = 48_000;

fn lib() -> Arc<MusicLibrary> {
    Arc::new(MusicLibrary::load_dir(&bd_music::default_music_dir()).expect("assets/music loads"))
}

fn render(p: &mut MusicPlayer, secs: f32, block: usize) -> Vec<f32> {
    let frames = (secs * SR as f32) as usize;
    let mut out = vec![0.0f32; frames * 2];
    for chunk in out.chunks_mut(block * 2) {
        p.render(chunk);
    }
    out
}

fn player(lib: &Arc<MusicLibrary>, track: &str, intensity: f32) -> MusicPlayer {
    let mut p = MusicPlayer::new(Arc::clone(lib), SR);
    p.command(MusicCommand::SetIntensity(intensity));
    p.command(MusicCommand::Play { track: lib.track_id(track).expect(track), skip_intro: false });
    p
}

const TRACKS: [&str; 13] = [
    "menu",
    "lobby",
    "tutorial",
    "archipelago_explore",
    "archipelago_combat",
    "miami",
    "orbit_float",
    "orbit_combat",
    "sudden_death",
    "final_stock",
    "party",
    "party_results",
    "results",
];

#[test]
fn soundtrack_is_complete() {
    let lib = lib();
    for t in TRACKS {
        let s = lib.track(lib.track_id(t).unwrap_or_else(|| panic!("missing track {t}"))).expect("track");
        let (_, body) = s.structure_ticks();
        let secs = body as f64 / f64::from(bd_music::notation::PPQ) * 60.0 / f64::from(s.bpm);
        assert!(secs > 60.0, "{t}: loop body only {secs:.1} s");
        assert!(s.ending.is_some(), "{t}: no ending section");
        assert!(!s.info.composer.is_empty() && !s.info.work.is_empty(), "{t}: missing provenance");
    }
    for k in StingerKind::ALL {
        assert!(lib.stingers.sections[k.index()].is_some(), "missing stinger {}", k.name());
    }
}

#[test]
fn deterministic_and_independent_of_callback_size() {
    let lib = lib();
    let a = render(&mut player(&lib, "menu", 1.0), 6.0, 512);
    let b = render(&mut player(&lib, "menu", 1.0), 6.0, 512);
    assert_eq!(a, b, "same input, different output");
    // Irregular callback sizes (1..=701 frames) must give bit-identical audio.
    let mut p = player(&lib, "menu", 1.0);
    let mut c = vec![0.0f32; a.len()];
    let mut pos = 0;
    let mut n = 1usize;
    while pos < c.len() {
        let end = (pos + n * 2).min(c.len());
        p.render(&mut c[pos..end]);
        pos = end;
        n = (n * 7 + 3) % 701 + 1;
    }
    assert_eq!(a, c, "output depends on callback size");
}

#[test]
fn stems_sum_to_the_mix_and_stay_sample_aligned() {
    let lib = lib();
    let mut a = player(&lib, "menu", 1.0);
    let mut b = player(&lib, "menu", 1.0);
    let frames = SR as usize * 8;
    let mix = render(&mut a, 8.0, 480);
    let mut stems = [vec![0.0f32; frames * 2], vec![0.0f32; frames * 2], vec![0.0f32; frames * 2], vec![0.0f32; frames * 2]];
    for i in (0..frames * 2).step_by(960) {
        let e = (i + 960).min(frames * 2);
        let [s0, s1, s2, s3] = &mut stems;
        b.render_stems(&mut s0[i..e], &mut s1[i..e], &mut s2[i..e], &mut s3[i..e]);
    }
    let mut checked = 0;
    for i in 0..frames * 2 {
        let sum: f32 = stems.iter().map(|s| s[i]).sum();
        if sum.abs() < 0.75 {
            assert!((sum - mix[i]).abs() < 1e-6, "sample {i}: stems {sum} vs mix {}", mix[i]);
            checked += 1;
        }
    }
    assert!(checked > frames, "limiter engaged too often");
    for (k, s) in stems.iter().enumerate().take(3) {
        assert!(s.iter().any(|x| x.abs() > 1e-3), "stem {k} silent at intensity 1");
    }
}

#[test]
fn stems_start_on_the_same_sample() {
    // A tiny song with one note per stem on the downbeat: the first non-silent sample of every
    // stem must be identical.
    let song = r#"(
        title: "align", key: "C", bpm: 120.0,
        channels: [
            (name: "a", inst: "lead25", stem: Base),
            (name: "b", inst: "lead25", stem: Drive),
            (name: "c", inst: "lead25", stem: Lead),
        ],
        sections: [(name: "s", bars: 1, parts: { "a": "r4 c2.|", "b": "r4 e2.|", "c": "r4 g2.|" })],
        order: ["s"], loop_to: "s", ending: "s",
    )"#;
    let dir = bd_music::default_music_dir();
    let inst = std::fs::read_to_string(dir.join("instruments.ron")).expect("instruments");
    let st = std::fs::read_to_string(dir.join("stingers.ron")).expect("stingers");
    let lib = Arc::new(MusicLibrary::from_sources(&inst, &st, &[("align", song)]).expect("song"));
    let mut p = MusicPlayer::new(Arc::clone(&lib), SR);
    p.command(MusicCommand::SetIntensity(1.0));
    p.command(MusicCommand::Play { track: lib.track_id("align").expect("id"), skip_intro: false });
    let frames = SR as usize;
    let mut stems = [vec![0.0f32; frames * 2], vec![0.0f32; frames * 2], vec![0.0f32; frames * 2], vec![0.0f32; frames * 2]];
    {
        let [s0, s1, s2, s3] = &mut stems;
        p.render_stems(s0, s1, s2, s3);
    }
    let onset = |s: &Vec<f32>| s.iter().position(|x| x.abs() > 1e-4).map(|i| i / 2);
    let o: Vec<Option<usize>> = stems.iter().take(3).map(onset).collect();
    let expect = SR as usize / 2; // one beat at 120 bpm
    for (k, x) in o.iter().enumerate() {
        let x = x.unwrap_or_else(|| panic!("stem {k} silent"));
        // HP filter + 4 ms attack: the first audible sample is within a few samples of the beat.
        assert!(x >= expect && x < expect + 8, "stem {k} starts at {x}, beat at {expect}");
    }
    assert_eq!(o[0], o[1]);
    assert_eq!(o[1], o[2]);
}

fn rms_env(x: &[f32], win: usize) -> Vec<f32> {
    x.chunks(win * 2).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect()
}

fn corr(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let ma = a.iter().sum::<f32>() / n as f32;
    let mb = b.iter().sum::<f32>() / n as f32;
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for i in 0..n {
        sab += (a[i] - ma) * (b[i] - mb);
        saa += (a[i] - ma) * (a[i] - ma);
        sbb += (b[i] - mb) * (b[i] - mb);
    }
    sab / (saa * sbb).sqrt().max(1e-12)
}

#[test]
fn every_track_loops_seamlessly_without_nan_or_clipping() {
    let lib = lib();
    let mut bad = Vec::new();
    let names: Vec<String> = lib.track_names().map(str::to_string).collect();
    for t in names.iter().map(String::as_str) {
        let mut p = player(&lib, t, 1.0);
        let mut stems: [Vec<f32>; 4] = Default::default();
        let mut mix: Vec<f32> = Vec::new();
        let mut b = [vec![0.0f32; 1024], vec![0.0f32; 1024], vec![0.0f32; 1024], vec![0.0f32; 1024]];
        let mut wrap = None;
        let mut stop = usize::MAX;
        while mix.len() / 2 < stop && mix.len() < (SR as usize) * 2 * 60 * 8 {
            let [b0, b1, b2, b3] = &mut b;
            p.render_stems(b0, b1, b2, b3);
            for i in 0..1024 {
                mix.push(b.iter().map(|s| s[i]).sum());
            }
            for (k, s) in stems.iter_mut().enumerate() {
                s.extend_from_slice(&b[k]);
            }
            if wrap.is_none() && p.debug_log().last_wrap.is_some() {
                wrap = p.debug_log().last_wrap;
                stop = wrap.unwrap_or(0) as usize + SR as usize * 6;
            }
        }
        let log = *p.debug_log();
        let (ls, w) = (log.loop_start.expect("loop start") as usize, wrap.expect("track never wrapped") as usize);
        assert!(mix.iter().all(|x| x.is_finite()), "{t}: non-finite sample");
        let peak = mix.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak < 0.79, "{t}: too hot ({peak}), would reach the safety limiter");
        // (1) No click at the seam: the largest sample-to-sample step within ±10 ms of the wrap is
        //     no larger than the largest step anywhere in the loop body.
        let step = |i: usize| (mix[2 * i] - mix[2 * i - 2]).abs().max((mix[2 * i + 1] - mix[2 * i - 1]).abs());
        let body_max = (ls + 1..w).map(step).fold(0.0f32, f32::max);
        let seam_max = (w - 480..w + 480).map(step).fold(0.0f32, f32::max);
        if seam_max > body_max {
            bad.push(format!("{t}: seam step {seam_max} > body max {body_max}"));
        }
        // (2) The sequencer repeats: note-ons per bar after the wrap match the first pass.
        let n = log.bar_n.min(256);
        let i0 = log.bars[..n].iter().position(|&s| s == ls as u64).expect("loop start is a bar line");
        let i1 = log.bars[..n].iter().position(|&s| s == w as u64).expect("wrap is a bar line");
        let k = (n - 1 - i1).min(4);
        assert!(k >= 2, "{t}: not enough bars after the wrap");
        if log.bar_notes[i0 + 1..=i0 + k] != log.bar_notes[i1 + 1..=i1 + k] {
            bad.push(format!("{t}: note counts differ {:?} vs {:?}", &log.bar_notes[i0 + 1..=i0 + k], &log.bar_notes[i1 + 1..=i1 + k]));
        }
        // (3) ...and sounds the same: per-stem loudness over 5 s agrees, and the drums/arps stem's 50 ms
        //     envelope correlates (sustained pads and leads beat/chorus freely, so only their level is compared).
        let mut report = String::new();
        for (si, s) in stems.iter().take(3).enumerate() {
            let a = rms_env(&s[2 * ls..2 * (ls + SR as usize * 5)], 2400);
            let bb = rms_env(&s[2 * w..2 * (w + SR as usize * 5)], 2400);
            let (ma, mb) = (a.iter().sum::<f32>() / a.len() as f32, bb.iter().sum::<f32>() / bb.len() as f32);
            if ma < 1e-3 {
                continue;
            }
            let rel = (ma - mb).abs() / ma;
            let c = corr(&a, &bb);
            report.push_str(&format!(" stem{si} level {rel:.3} corr {c:.3};"));
            if rel > 0.1 || (si == 1 && c < 0.75) {
                bad.push(format!("{t}: stem {si} second pass differs (level {rel:.3}, corr {c:.3})"));
            }
        }
        println!("{t}: seam {seam_max:.4} / body {body_max:.4}, peak {peak:.3};{report}");
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn intensity_changes_only_on_bar_lines() {
    let lib = lib();
    let mut p = player(&lib, "archipelago_combat", 0.0);
    let mut buf = vec![0.0f32; 2 * 333];
    let levels = [1.0, 0.2, 0.7, 0.0, 1.0, 0.4];
    for (i, lvl) in levels.iter().enumerate() {
        // Change at odd, non-aligned moments.
        for _ in 0..(200 + i * 37) {
            p.render(&mut buf);
        }
        p.command(MusicCommand::SetIntensity(*lvl));
    }
    for _ in 0..600 {
        p.render(&mut buf);
    }
    let log = *p.debug_log();
    assert!(log.change_n >= 4, "only {} stem changes", log.change_n);
    for &c in log.target_changes.iter().take(log.change_n.min(64)) {
        assert!(log.is_bar(c), "stem target changed at sample {c}, not a bar line");
    }
}

#[test]
fn stingers_land_on_the_grid() {
    let lib = lib();
    let mut p = player(&lib, "menu", 1.0);
    let mut buf = vec![0.0f32; 2 * 173];
    for _ in 0..500 {
        p.render(&mut buf);
    }
    p.command(MusicCommand::Stinger(StingerKind::Ko));
    for _ in 0..300 {
        p.render(&mut buf);
    }
    let log = *p.debug_log();
    let s = log.stinger_start.expect("ko started");
    assert!(log.beats.iter().take(log.beat_n.min(64)).any(|&b| b == s), "KO at {s} is not on a beat");
    p.command(MusicCommand::Stinger(StingerKind::MatchPoint));
    for _ in 0..300 {
        p.render(&mut buf);
    }
    let log = *p.debug_log();
    let s2 = log.stinger_start.expect("match point started");
    assert!(s2 != s && log.is_bar(s2), "match point at {s2} is not on a bar line");
    // The held track resumes after the stinger.
    for _ in 0..3000 {
        p.render(&mut buf);
    }
    assert!(p.status().playing && p.status().stinger.is_none());
    let bars_before = p.status().bars;
    for _ in 0..1000 {
        p.render(&mut buf);
    }
    assert!(p.status().bars > bars_before, "track did not resume after match point");
}

#[test]
fn victory_plays_the_ending_and_stops() {
    let lib = lib();
    let mut p = player(&lib, "menu", 1.0);
    let _ = render(&mut p, 3.0, 256);
    p.command(MusicCommand::Stinger(StingerKind::Victory));
    let _ = render(&mut p, 12.0, 256);
    assert!(!p.status().playing, "still playing after victory");
    let tail = render(&mut p, 1.0, 256);
    assert!(tail.iter().all(|x| x.abs() < 1e-3), "not silent after the ending");
}

#[test]
fn track_switch_happens_on_a_bar_line() {
    let lib = lib();
    let mut p = player(&lib, "menu", 0.5);
    let _ = render(&mut p, 2.3, 100);
    let lobby = lib.track_id("lobby").expect("lobby");
    p.command(MusicCommand::Play { track: lobby, skip_intro: true });
    let bars_before = p.debug_log().bar_n;
    let _ = render(&mut p, 4.0, 100);
    assert_eq!(p.status().track, Some(lobby));
    let log = p.debug_log();
    assert!(log.bar_n > bars_before);
}

#[test]
fn tempo_multiplier_shortens_bars() {
    let lib = lib();
    let bar_len = |mult: f32| {
        let mut p = player(&lib, "menu", 1.0);
        p.command(MusicCommand::SetTempo { multiplier: mult, ramp_secs: 0.0 });
        let _ = render(&mut p, 10.0, 256);
        let log = p.debug_log();
        let n = log.bar_n.min(256);
        (log.bars[n - 1] - log.bars[n - 2]) as f64
    };
    let a = bar_len(1.0);
    let b = bar_len(1.5);
    let ratio = a / b;
    assert!((ratio - 1.5).abs() < 0.05, "bar ratio {ratio}");
}

#[test]
fn player_can_move_to_the_audio_thread() {
    fn assert_send<T: Send + 'static>() {}
    assert_send::<MusicPlayer>();
    assert_send::<MusicCommand>();
    assert_send::<Arc<MusicLibrary>>();
}

#[test]
fn every_piece_has_remixes() {
    let lib = lib();
    let combat = ["archipelago_combat", "orbit_combat", "sudden_death", "final_stock"];
    for t in TRACKS {
        let id = lib.track_id(t).expect(t);
        let v = lib.variants_of(id);
        let need = if combat.contains(&t) { 3 } else { 2 };
        assert!(v.len() >= need, "{t}: only {} remixes", v.len());
        for r in v {
            let s = lib.track(*r).expect("remix");
            assert!(s.meta.style.is_some() && s.meta.adaptive.is_some(), "{}: no style / adaptive", s.info.name);
            if s.meta.sub_bass {
                assert!(s.meta.hpf_hz <= 35.0, "{}: sub variant with a {} Hz high-pass", s.info.name, s.meta.hpf_hz);
            }
            let (_, body) = s.structure_ticks();
            let secs = body as f64 / f64::from(bd_music::notation::PPQ) * 60.0 / f64::from(s.bpm);
            assert!(secs > 40.0, "{}: loop only {secs:.1} s", s.info.name);
        }
    }
}

#[test]
fn remix_drop_lands_on_a_bar_line_after_the_build() {
    let lib = lib();
    for t in ["menu_house", "sudden_death_trap", "party_chipedm", "lobby_lofi", "final_stock_dnb"] {
        let s = lib.track(lib.track_id(t).expect(t)).expect("song");
        let build_bars = s.meta.adaptive.expect("adaptive").build_bars as usize;
        let mut p = player(&lib, t, 0.2);
        let mut buf = vec![0.0f32; 2 * 377];
        let _ = render(&mut p, 6.3, 377);
        assert_eq!(p.status().phase, AdaptPhase::Calm, "{t}");
        p.command(MusicCommand::SetIntensity(1.0));
        for _ in 0..(48_000 * 14 / 377) {
            p.render(&mut buf);
        }
        let log = *p.debug_log();
        assert!(log.drop_n >= 1, "{t}: no drop");
        let drop = log.drops[(log.drop_n - 1) % 16];
        assert!(log.is_bar(drop), "{t}: drop at {drop} is not a bar line");
        if build_bars > 0 {
            let build = log.builds[(log.build_n - 1) % 16];
            assert!(log.is_bar(build), "{t}: build not on a bar line");
            let n = log.bar_n.min(256);
            let bi = log.bars[..n].iter().position(|&b| b == build).expect("build bar");
            let di = log.bars[..n].iter().position(|&b| b == drop).expect("drop bar");
            assert_eq!(di - bi, build_bars, "{t}: drop {} bars after the build", di - bi);
        }
        assert_eq!(p.status().phase, AdaptPhase::Combat, "{t}");
        // Back to exploring on the next bar line.
        p.command(MusicCommand::SetIntensity(0.2));
        let _ = render(&mut p, 5.0, 377);
        assert_eq!(p.status().phase, AdaptPhase::Calm, "{t}");
    }
}

#[test]
fn style_setting_picks_variants_and_switches_on_bar_lines() {
    let lib = lib();
    let menu = lib.track_id("menu").expect("menu");
    let house = lib.track_id("menu_house").expect("menu_house");
    let mut p = MusicPlayer::new(Arc::clone(&lib), SR);
    p.command(MusicCommand::SetStyle(Style::Remix(Some(RemixStyle::House))));
    p.command(MusicCommand::Play { track: menu, skip_intro: false });
    let _ = render(&mut p, 1.0, 256);
    assert_eq!(p.status().track, Some(house));
    assert!(p.status().sub_bass && p.status().hpf_hz < 40.0);
    // Switching the setting mid-track changes version on the next bar line.
    p.command(MusicCommand::SetStyle(Style::Classic));
    let bars_before = p.debug_log().bar_n;
    let _ = render(&mut p, 4.0, 256);
    assert_eq!(p.status().track, Some(menu));
    assert!(p.debug_log().bar_n > bars_before);
    assert!(!p.status().sub_bass);
    // Replaying the same request keeps the current version.
    p.command(MusicCommand::Play { track: menu, skip_intro: false });
    let _ = render(&mut p, 1.0, 256);
    assert_eq!(p.status().track, Some(menu));
    // Shuffle picks among the classic and its remixes, deterministically.
    let pick = || {
        let mut q = MusicPlayer::new(Arc::clone(&lib), SR);
        q.command(MusicCommand::SetStyle(Style::Shuffle));
        let mut seen = Vec::new();
        for _ in 0..6 {
            q.command(MusicCommand::Stop);
            let _ = render(&mut q, 3.0, 256);
            q.command(MusicCommand::Play { track: menu, skip_intro: true });
            let _ = render(&mut q, 0.2, 256);
            seen.push(q.status().track);
        }
        seen
    };
    let a = pick();
    assert_eq!(a, pick());
    assert!(a.iter().any(|x| *x != Some(menu)) && a.iter().all(|x| x.is_some_and(|t| lib.classic_of(t) == menu)));
}

#[test]
fn remixes_are_deterministic_for_any_callback_size() {
    let lib = lib();
    for t in ["lobby_lofi", "archipelago_combat_trap", "menu_house"] {
        // Intensity change at a fixed sample, fixed-size callbacks: run twice, compare.
        let run = || {
            let mut a = player(&lib, t, 0.3);
            let mut x = vec![0.0f32; 48_000 * 2 * 10];
            let mut pa = 0;
            while pa < x.len() {
                if pa == 1024 * 280 {
                    a.command(MusicCommand::SetIntensity(1.0));
                }
                let e = (pa + 1024).min(x.len());
                a.render(&mut x[pa..e]);
                pa = e;
            }
            x
        };
        let x = run();
        assert_eq!(x, run(), "{t}: not deterministic");
        assert!(x.iter().all(|v| v.is_finite()));
        let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 0.79, "{t}: peak {peak}");
        // Irregular callback sizes give bit-identical audio.
        let fixed = render(&mut player(&lib, t, 1.0), 6.0, 512);
        let mut b = player(&lib, t, 1.0);
        let mut y = vec![0.0f32; fixed.len()];
        let (mut pos, mut n) = (0, 1usize);
        while pos < y.len() {
            let e = (pos + 2 * n).min(y.len());
            b.render(&mut y[pos..e]);
            pos = e;
            n = (n * 7 + 3) % 509 + 1;
        }
        assert_eq!(fixed, y, "{t}: output depends on callback size");
    }
}
