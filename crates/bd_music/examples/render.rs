//! Offline renderer + report for the soundtrack (the lead cannot listen live).
//!
//! ```text
//! cargo run --release -p bd_music --example render -- --track menu --intensity 0.3 --out target/music/menu_0.3.wav
//! cargo run --release -p bd_music --example render -- --all            # every track at 0.3 and 1.0 + report
//! cargo run --release -p bd_music --example render -- --medley target/music/medley.wav
//! cargo run --release -p bd_music --example render -- --stingers target/music/stingers.wav
//! cargo run --release -p bd_music --example render -- --lint [--track name]
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use bd_music::analysis::{BANDS, band_energies_db, integrated_lufs, sample_peak_db, true_peak_db, write_wav16};
use bd_music::notation::PPQ;
use bd_music::{MusicCommand, MusicLibrary, MusicPlayer, StingerKind};

const SR: u32 = 48_000;

struct Args {
    track: Option<String>,
    intensity: f32,
    out: Option<PathBuf>,
    out_dir: PathBuf,
    seconds: Option<f32>,
    all: bool,
    medley: Option<PathBuf>,
    stingers: Option<PathBuf>,
    lint: bool,
    list: bool,
    medley_secs: f32,
    spectrogram: Option<PathBuf>,
    start: f32,
    calibrate: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        track: None,
        intensity: 1.0,
        out: None,
        out_dir: PathBuf::from("target/music"),
        seconds: None,
        all: false,
        medley: None,
        stingers: None,
        lint: false,
        list: false,
        medley_secs: 20.0,
        spectrogram: None,
        start: 0.0,
        calibrate: false,
    };
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let next = |i: usize| v.get(i + 1).cloned().unwrap_or_default();
        match v[i].as_str() {
            "--track" => {
                a.track = Some(next(i));
                i += 1;
            }
            "--intensity" => {
                a.intensity = next(i).parse().unwrap_or(1.0);
                i += 1;
            }
            "--out" => {
                a.out = Some(PathBuf::from(next(i)));
                i += 1;
            }
            "--out-dir" => {
                a.out_dir = PathBuf::from(next(i));
                i += 1;
            }
            "--seconds" => {
                a.seconds = next(i).parse().ok();
                i += 1;
            }
            "--medley" => {
                a.medley = Some(PathBuf::from(next(i)));
                i += 1;
            }
            "--medley-secs" => {
                a.medley_secs = next(i).parse().unwrap_or(20.0);
                i += 1;
            }
            "--stingers" => {
                a.stingers = Some(PathBuf::from(next(i)));
                i += 1;
            }
            "--spectrogram" => {
                a.spectrogram = Some(PathBuf::from(next(i)));
                i += 1;
            }
            "--start" => {
                a.start = next(i).parse().unwrap_or(0.0);
                i += 1;
            }
            "--calibrate" => a.calibrate = true,
            "--all" | "--report" => a.all = true,
            "--lint" => a.lint = true,
            "--list" => a.list = true,
            other => eprintln!("ignoring unknown argument {other}"),
        }
        i += 1;
    }
    a
}

struct Rendered {
    audio: Vec<f32>,
    loop_start: Option<u64>,
    wrap: Option<u64>,
    secs_cpu: f64,
}

/// Render a track from the top: intro + one full loop + `tail` seconds past the wrap (or `seconds`).
fn render_track(lib: &Arc<MusicLibrary>, name: &str, intensity: f32, seconds: Option<f32>, tail: f32) -> Rendered {
    let id = lib.track_id(name).unwrap_or_else(|| panic!("no track {name}"));
    let mut p = MusicPlayer::new(Arc::clone(lib), SR);
    p.command(MusicCommand::SetIntensity(intensity));
    p.command(MusicCommand::Play { track: id, skip_intro: false });
    let mut audio = Vec::new();
    let block = 512;
    let mut buf = vec![0.0f32; block * 2];
    let limit = seconds.map_or(8 * 60 * SR as usize, |s| (s * SR as f32) as usize);
    let t0 = Instant::now();
    let mut stop_at: Option<usize> = None;
    while audio.len() / 2 < limit {
        p.render(&mut buf);
        audio.extend_from_slice(&buf);
        if seconds.is_none() {
            if stop_at.is_none()
                && let Some(w) = p.debug_log().last_wrap
            {
                stop_at = Some(w as usize + (tail * SR as f32) as usize);
            }
            if let Some(s) = stop_at
                && audio.len() / 2 >= s
            {
                audio.truncate(s * 2);
                break;
            }
        }
    }
    let secs_cpu = t0.elapsed().as_secs_f64();
    let log = p.debug_log();
    Rendered { audio, loop_start: log.loop_start, wrap: log.last_wrap, secs_cpu }
}

/// Integer-exact prediction of the intro / loop length in samples (tempo multiplier 1).
fn predict(lib: &MusicLibrary, name: &str) -> (u64, u64) {
    let Some(song) = lib.track_id(name).and_then(|id| lib.track(id)) else { return (0, 0) };
    let thresh = 60 * u64::from(SR) * 1000;
    let rate = |bpm: f32| ((f64::from(bpm) * 1000.0).round() as u64).max(1) * u64::from(PPQ);
    let sec_samples = |i: usize, next: usize| -> u64 {
        let s = &song.sections[i];
        let mut acc = 0u64;
        let mut total = 0u64;
        for t in 0..s.len {
            let bpm = if t + 1 < s.len { s.bpm_at(t + 1) } else { song.sections[next].bpm_at(0) };
            let r = rate(bpm);
            let n = (thresh - acc.min(thresh)).div_ceil(r);
            total += n;
            acc = acc + n * r - thresh;
            if t + 1 == s.len {
                acc = 0;
            }
        }
        total
    };
    let order = &song.order;
    let mut intro = 0;
    let mut body = 0;
    for (k, &sec) in order.iter().enumerate() {
        let next = if k + 1 < order.len() { order[k + 1] } else { order[song.loop_idx] };
        let n = sec_samples(sec, next);
        if k < song.loop_idx {
            intro += n;
        } else {
            body += n;
        }
    }
    (intro, body)
}

/// Integrated loudness of each stem over the first 40 s of the loop body at intensity 1.
fn stem_lufs(lib: &Arc<MusicLibrary>, name: &str) -> [f32; 4] {
    let id = lib.track_id(name).expect("track");
    let mut p = MusicPlayer::new(Arc::clone(lib), SR);
    p.command(MusicCommand::SetIntensity(1.0));
    p.command(MusicCommand::Play { track: id, skip_intro: true });
    let frames = 40 * SR as usize;
    let mut s: [Vec<f32>; 4] = [vec![0.0; frames * 2], vec![0.0; frames * 2], vec![0.0; frames * 2], vec![0.0; frames * 2]];
    {
        let [a, b, c, d] = &mut s;
        for i in (0..frames * 2).step_by(1024) {
            let e = (i + 1024).min(frames * 2);
            p.render_stems(&mut a[i..e], &mut b[i..e], &mut c[i..e], &mut d[i..e]);
        }
    }
    [integrated_lufs(&s[0], SR), integrated_lufs(&s[1], SR), integrated_lufs(&s[2], SR), integrated_lufs(&s[3], SR)]
}

fn fmt_bands(b: &[f32; 8]) -> String {
    b.iter().map(|x| format!("{x:6.1}")).collect::<Vec<_>>().join(" ")
}

fn main() {
    let args = parse_args();
    let dir = bd_music::default_music_dir();
    let lib = match MusicLibrary::load_dir(&dir) {
        Ok(l) => Arc::new(l),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    if args.list {
        for s in &lib.tracks {
            let (i, b) = s.structure_ticks();
            println!(
                "{:22} {:28} {:>4} bpm  {:>3}+{:>3} bars  {}",
                s.info.name,
                s.info.title,
                s.bpm,
                i / u64::from(s.bar_ticks),
                b / u64::from(s.bar_ticks),
                s.info.role
            );
        }
        return;
    }
    if args.lint {
        for s in &lib.tracks {
            if args.track.as_deref().is_none_or(|t| t == s.info.name) {
                lint(s);
            }
        }
        return;
    }
    if args.calibrate {
        calibrate(&lib, &dir);
        return;
    }
    if let Some(path) = &args.stingers {
        render_stingers(&lib, path);
    }
    if let Some(path) = &args.medley {
        render_medley(&lib, path, args.medley_secs);
    }
    if let (Some(png), Some(t)) = (&args.spectrogram, &args.track) {
        let secs = args.seconds.unwrap_or(20.0);
        let r = render_track(&lib, t, args.intensity, Some(args.start + secs), 0.0);
        let skip = (args.start * SR as f32) as usize * 2;
        spectrogram_png(png, r.audio.get(skip..).unwrap_or(&[]));
        println!("spectrogram {t} {:.1}-{:.1} s -> {}", args.start, args.start + secs, png.display());
        return;
    }
    if let (Some(t), false) = (&args.track, args.all) {
        let r = render_track(&lib, t, args.intensity, args.seconds, 4.0);
        let out = args.out.clone().unwrap_or_else(|| args.out_dir.join(format!("{t}_{}.wav", args.intensity)));
        write_wav16(&out, SR, &r.audio).expect("write wav");
        let secs = r.audio.len() as f64 / 2.0 / f64::from(SR);
        println!(
            "{t}: {:.1} s, loop start {:?}, wrap {:?}, {:.1} LUFS, {:.1} dBTP, cpu {:.1} ms per s -> {}",
            secs,
            r.loop_start,
            r.wrap,
            integrated_lufs(&r.audio, SR),
            true_peak_db(&r.audio),
            r.secs_cpu * 1000.0 / secs,
            out.display()
        );
    }
    if args.all {
        report(&lib, &args.out_dir);
    }
}

fn report(lib: &Arc<MusicLibrary>, out_dir: &Path) {
    let mut md = String::new();
    md.push_str("# bd_music render report\n\n");
    md.push_str(&format!(
        "48 kHz stereo. Each track rendered from the top: intro + one full loop + 4 s past the loop point, at intensity 0.3 \
         (exploration) and 1.0 (combat). Loop points are sample positions; \"predicted\" is the integer tempo math, \
         \"observed\" is where the real-time player wrapped. Bands = energy relative to total (dB): {}.\n\n",
        BANDS.iter().map(|(a, b)| format!("{a:.0}-{b:.0}")).collect::<Vec<_>>().join(", ")
    ));
    md.push_str(
        "| track | piece | length s | loop start (obs / pred) | loop len samples (obs / pred) | LUFS 0.3 | LUFS 1.0 | dBTP 1.0 | peak 1.0 | >12 kHz dB | CPU ms per s (1.0) |\n|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    let mut bands_md = String::from(
        "\n## Spectral bands at intensity 1.0 (dB rel. total)\n\n```\ntrack                   <60    60-250 250-1k  1k-4k  4k-8k  8k-12k 12k-16k 16k-24k\n",
    );
    let mut stems_md = String::from(
        "\n## Stem loudness (LUFS, first 40 s of the loop at intensity 1.0) and tonal-only HF check\n\n| track | base | drive | lead | >12 kHz at 0.3 (no drums), dB rel. total |\n|---|---|---|---|---|\n",
    );
    let mut total_cpu = 0.0;
    let mut total_secs = 0.0;
    for s in &lib.tracks {
        let name = &s.info.name;
        let lo = render_track(lib, name, 0.3, None, 4.0);
        let hi = render_track(lib, name, 1.0, None, 4.0);
        write_wav16(&out_dir.join(format!("{name}_0.3.wav")), SR, &lo.audio).expect("wav");
        write_wav16(&out_dir.join(format!("{name}_1.0.wav")), SR, &hi.audio).expect("wav");
        let (pi, pb) = predict(lib, name);
        let secs = hi.audio.len() as f64 / 2.0 / f64::from(SR);
        let l_lo = integrated_lufs(&lo.audio, SR);
        let l_hi = integrated_lufs(&hi.audio, SR);
        let tp = true_peak_db(&hi.audio);
        let pk = sample_peak_db(&hi.audio);
        let bands = band_energies_db(&hi.audio, SR);
        let above12 = 10.0 * (10f32.powf(bands[6] / 10.0) + 10f32.powf(bands[7] / 10.0)).log10();
        let cpu = hi.secs_cpu * 1000.0 / secs;
        total_cpu += hi.secs_cpu;
        total_secs += secs;
        let obs_len = match (hi.loop_start, hi.wrap) {
            (Some(a), Some(b)) => (b - a).to_string(),
            _ => "?".into(),
        };
        let ok = if hi.loop_start == Some(pi) && hi.wrap.map(|w| w - pi) == Some(pb) { "exact" } else { "MISMATCH" };
        md.push_str(&format!(
            "| {name} | {} ({}) | {secs:.1} | {} / {pi} | {obs_len} / {pb} ({ok}, {:.2} s) | {l_lo:.1} | {l_hi:.1} | {tp:.1} | {pk:.1} | {above12:.1} | {cpu:.2} |\n",
            s.info.title,
            s.info.composer.split(" (").next().unwrap_or(""),
            hi.loop_start.map_or("?".into(), |x| x.to_string()),
            pb as f64 / f64::from(SR),
        ));
        bands_md.push_str(&format!("{name:22} {}\n", fmt_bands(&bands)));
        let st = stem_lufs(lib, name);
        let b3 = band_energies_db(&lo.audio, SR);
        let tonal12 = 10.0 * (10f32.powf(b3[6] / 10.0) + 10f32.powf(b3[7] / 10.0)).log10();
        stems_md.push_str(&format!("| {name} | {:.1} | {:.1} | {:.1} | {tonal12:.1} |\n", st[0], st[1], st[2]));
        println!(
            "{name:22} {secs:6.1}s  loop {obs_len:>9} ({ok})  LUFS {l_lo:6.1} / {l_hi:6.1}  TP {tp:5.1}  >12k {above12:6.1} dB  cpu {cpu:.2} ms/s"
        );
    }
    bands_md.push_str("```\n");
    md.push_str(&bands_md);
    md.push_str(&stems_md);
    md.push_str(&format!(
        "\nTotal: {:.0} s of audio rendered (intensity 1.0 pass) in {:.2} s CPU = {:.2} ms per second of audio \
         ({:.3} % of one core, {:.0}x real time). The player renders on the audio thread with zero allocation.\n",
        total_secs,
        total_cpu,
        total_cpu * 1000.0 / total_secs,
        total_cpu * 100.0 / total_secs,
        total_secs / total_cpu
    ));
    let path = out_dir.join("report.md");
    std::fs::create_dir_all(out_dir).ok();
    std::fs::write(&path, md).expect("report");
    println!("report -> {}", path.display());
}

/// ~`secs` per track: exploration (0.3) for the first half, combat (1.0) for the second, switching
/// tracks on bar lines through the real adaptive API, with a few stingers and a tempo ramp.
fn render_medley(lib: &Arc<MusicLibrary>, path: &Path, secs: f32) {
    let order = [
        "menu",
        "lobby",
        "tutorial",
        "archipelago_explore",
        "archipelago_combat",
        "miami",
        "orbit_float",
        "orbit_combat",
        "final_stock",
        "sudden_death",
        "party",
        "party_results",
        "results",
    ];
    let names: Vec<&str> = order.iter().copied().filter(|n| lib.track_id(n).is_some()).collect();
    let mut p = MusicPlayer::new(Arc::clone(lib), SR);
    let mut audio: Vec<f32> = Vec::new();
    let mut buf = vec![0.0f32; 256 * 2];
    let per = (secs * SR as f32) as usize;
    let mut run = |p: &mut MusicPlayer, audio: &mut Vec<f32>, frames: usize| {
        let target = audio.len() / 2 + frames;
        while audio.len() / 2 < target {
            p.render(&mut buf);
            audio.extend_from_slice(&buf);
        }
    };
    for (i, n) in names.iter().enumerate() {
        let id = lib.track_id(n).expect("track");
        p.command(MusicCommand::SetIntensity(0.3));
        p.command(MusicCommand::Play { track: id, skip_intro: i > 0 });
        if *n == "sudden_death" {
            p.command(MusicCommand::SetTempo { multiplier: 1.0, ramp_secs: 0.0 });
        }
        run(&mut p, &mut audio, per / 2);
        p.command(MusicCommand::SetIntensity(1.0));
        if *n == "sudden_death" {
            p.command(MusicCommand::SetTempo { multiplier: 1.3, ramp_secs: 6.0 });
        }
        run(&mut p, &mut audio, per / 4);
        let sting = match *n {
            "archipelago_combat" => Some(StingerKind::Ko),
            "orbit_combat" => Some(StingerKind::Streak),
            "final_stock" => Some(StingerKind::MatchPoint),
            "miami" => Some(StingerKind::Ko),
            _ => None,
        };
        if let Some(k) = sting {
            p.command(MusicCommand::Stinger(k));
        }
        run(&mut p, &mut audio, per / 4);
        if *n == "sudden_death" {
            p.command(MusicCommand::SetTempo { multiplier: 1.0, ramp_secs: 0.5 });
        }
    }
    // Finish with a victory on the last track, then let it ring.
    p.command(MusicCommand::Stinger(StingerKind::Victory));
    run(&mut p, &mut audio, (6.0 * SR as f32) as usize);
    // In game the music sits at -23 LUFS under the SFX; the preview file is lifted to a normal
    // listening level (-16 LUFS, peaks kept under -1 dBTP) so it can be judged on its own.
    let game_lufs = integrated_lufs(&audio, SR);
    let game_tp = true_peak_db(&audio);
    let lift_db = (-16.0 - game_lufs).min(-1.0 - game_tp);
    let m = bd_music::analysis::momentary_lufs(&audio, SR);
    let per_track = (secs * 10.0) as usize;
    for (i, n) in names.iter().enumerate() {
        let seg = m.get(i * per_track..((i + 1) * per_track).min(m.len())).unwrap_or(&[]);
        let lo = seg.iter().copied().fold(0.0f32, f32::min);
        let hi = seg.iter().copied().fold(-99.0f32, f32::max);
        let half = seg.len() / 2;
        let avg = |s: &[f32]| s.iter().sum::<f32>() / s.len().max(1) as f32;
        println!("  {n:20} momentary min {lo:6.1} max {hi:6.1}  mean explore {:6.1} combat {:6.1}", avg(&seg[..half]), avg(&seg[half..]));
    }
    let g = 10f32.powf(lift_db / 20.0);
    for x in &mut audio {
        *x *= g;
    }
    write_wav16(path, SR, &audio).expect("medley wav");
    println!(
        "medley: {} tracks, {:.1} s; in-game level {game_lufs:.1} LUFS / {game_tp:.1} dBTP; preview file lifted {lift_db:+.1} dB -> {:.1} LUFS, {:.1} dBTP -> {}",
        names.len(),
        audio.len() as f32 / 2.0 / SR as f32,
        integrated_lufs(&audio, SR),
        true_peak_db(&audio),
        path.display()
    );
}

/// Every stinger over a bar of the menu track (and in silence).
fn render_stingers(lib: &Arc<MusicLibrary>, path: &Path) {
    let mut p = MusicPlayer::new(Arc::clone(lib), SR);
    let mut audio: Vec<f32> = Vec::new();
    let mut buf = vec![0.0f32; 256 * 2];
    let mut run = |p: &mut MusicPlayer, audio: &mut Vec<f32>, secs: f32| {
        let target = audio.len() / 2 + (secs * SR as f32) as usize;
        while audio.len() / 2 < target {
            p.render(&mut buf);
            audio.extend_from_slice(&buf);
        }
    };
    let track = lib.track_id("archipelago_combat").or_else(|| lib.track_id("menu")).expect("a track");
    for k in StingerKind::ALL {
        p.command(MusicCommand::SetIntensity(1.0));
        p.command(MusicCommand::Play { track, skip_intro: true });
        run(&mut p, &mut audio, 4.0);
        let before = audio.len();
        p.command(MusicCommand::Stinger(k));
        run(&mut p, &mut audio, if k.ends_track() { 6.0 } else { 9.0 });
        let m = bd_music::analysis::momentary_lufs(&audio[before - 2 * 3 * SR as usize..before], SR);
        let s = bd_music::analysis::momentary_lufs(&audio[before..before + 2 * 3 * SR as usize], SR);
        let mx = |v: &[f32]| v.iter().copied().fold(-99.0f32, f32::max);
        println!("  {:14} music max momentary {:6.1} LUFS, with stinger {:6.1} LUFS", k.name(), mx(&m), mx(&s));
        p.command(MusicCommand::Stop);
        run(&mut p, &mut audio, 1.5);
    }
    write_wav16(path, SR, &audio).expect("stingers wav");
    println!("stingers: {:.1} s -> {}", audio.len() as f32 / 2.0 / SR as f32, path.display());
}

// ------------------------------------------------------------------------------------------------
// Lint: vertical clashes on strong beats (catches wrong accidentals / octaves while authoring).
// ------------------------------------------------------------------------------------------------

fn lint(song: &bd_music::Song) {
    const NAMES: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
    let nm = |k: u8| format!("{}{}", NAMES[usize::from(k % 12)], i32::from(k) / 12 - 1);
    println!("== {} ({})", song.info.name, song.info.title);
    let mut total = 0;
    for sec in &song.sections {
        // Range per channel.
        let mut ranges = String::new();
        for (ci, ch) in song.channels.iter().enumerate() {
            if ch.drums {
                continue;
            }
            let notes = &sec.parts[ci];
            if notes.is_empty() {
                continue;
            }
            let lo = notes.iter().map(|n| n.key).min().unwrap_or(0);
            let hi = notes.iter().map(|n| n.keys().0.iter().take(n.keys().1).copied().max().unwrap_or(0)).max().unwrap_or(0);
            ranges.push_str(&format!(" {}:{}-{}", ch.name, nm(lo), nm(hi)));
        }
        println!("  [{}]{}", sec.name, ranges);
        let beat = song.beat_ticks;
        let mut t = 0;
        while t < sec.len {
            // Notes sounding at t that last at least a beat from their start and started at t or are held.
            let mut sounding: Vec<(usize, u8, bool)> = Vec::new();
            for (ci, ch) in song.channels.iter().enumerate() {
                if ch.drums {
                    continue;
                }
                for n in &sec.parts[ci] {
                    if n.tick <= t && t < n.tick + n.len && n.len >= beat {
                        let (k, cnt) = n.keys();
                        for &key in k.iter().take(cnt) {
                            sounding.push((ci, key, n.tick == t));
                        }
                    }
                }
            }
            for i in 0..sounding.len() {
                for j in i + 1..sounding.len() {
                    let (ca, ka, sa) = sounding[i];
                    let (cb, kb, sb) = sounding[j];
                    if ca == cb || !(sa || sb) {
                        continue;
                    }
                    // Minor 2nds / minor 9ths only (major 7ths are idiomatic in maj7 chords).
                    let d = (i32::from(ka) - i32::from(kb)).abs();
                    if d % 12 == 1 && d < 25 {
                        total += 1;
                        println!(
                            "    bar {:>3} beat {}: {} {} vs {} {}",
                            t / song.bar_ticks + 1,
                            (t % song.bar_ticks) / beat + 1,
                            song.channels[ca].name,
                            nm(ka),
                            song.channels[cb].name,
                            nm(kb)
                        );
                    }
                }
            }
            t += beat;
        }
    }
    println!("  {total} semitone clash(es) on beats");
}

// ------------------------------------------------------------------------------------------------
// Spectrogram PNG (log frequency 40 Hz - 22 kHz, 1 column per 10 ms) — for eyeballing notes and
// aliasing without listening. Minimal PNG encoder (stored deflate), no dependencies.
// ------------------------------------------------------------------------------------------------

fn spectrogram_png(path: &Path, audio: &[f32]) {
    let n = 4096;
    let cols = bd_music::analysis::spectrogram_db(audio, n, (SR / 100) as usize);
    let w = cols.len().max(1);
    let h = 480usize;
    let (f_lo, f_hi) = (40.0f32, 22_000.0f32);
    let bin_hz = SR as f32 / n as f32;
    let mut img = vec![0u8; w * h * 3];
    for (x, col) in cols.iter().enumerate() {
        for y in 0..h {
            let fy = 1.0 - y as f32 / (h - 1) as f32;
            let f = f_lo * (f_hi / f_lo).powf(fy);
            let bin = ((f / bin_hz) as usize).min(col.len() - 1);
            let db = col[bin];
            let v = ((db + 95.0) / 85.0).clamp(0.0, 1.0);
            let (r, g, b) = heat(v);
            let i = (y * w + x) * 3;
            img[i] = r;
            img[i + 1] = g;
            img[i + 2] = b;
        }
    }
    // Octave guide lines (C notes) on the left edge + a 12 kHz line.
    for f in [65.4f32, 130.8, 261.6, 523.3, 1046.5, 2093.0, 4186.0, 8372.0, 12000.0] {
        let y = ((1.0 - (f / f_lo).ln() / (f_hi / f_lo).ln()) * (h - 1) as f32) as usize;
        for x in 0..w.min(if f == 12000.0 { w } else { 12 }) {
            let i = (y.min(h - 1) * w + x) * 3;
            img[i] = 255;
            img[i + 1] = 255;
            img[i + 2] = 255;
        }
    }
    write_png(path, w as u32, h as u32, &img).expect("png");
}

fn heat(v: f32) -> (u8, u8, u8) {
    let r = (v * 3.0).clamp(0.0, 1.0);
    let g = (v * 3.0 - 1.0).clamp(0.0, 1.0);
    let b = if v < 0.33 { v * 1.5 } else { (v * 3.0 - 2.0).clamp(0.0, 1.0) };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn write_png(path: &Path, w: u32, h: u32, rgb: &[u8]) -> std::io::Result<()> {
    let mut raw = Vec::with_capacity((w as usize * 3 + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w as usize * 3..(y + 1) * w as usize * 3]);
    }
    let mut z = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, c) in chunks.iter().enumerate() {
        z.push(u8::from(i + 1 == chunks.len()));
        let len = c.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |ty: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = ty.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, out)
}
/// Measure every track at intensity 1 and rewrite its gain_db so it lands on -23 LUFS integrated.
fn calibrate(lib: &Arc<MusicLibrary>, dir: &Path) {
    const TARGET: f32 = -23.0;
    for s in &lib.tracks {
        let name = &s.info.name;
        let r = render_track(lib, name, 1.0, None, 0.0);
        let lufs = integrated_lufs(&r.audio, SR);
        let path = dir.join(format!("{name}.ron"));
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Some(pos) = text.find("gain_db:") else { continue };
        let end = text[pos..].find(',').map_or(pos, |e| pos + e);
        let old: f32 = text[pos + 8..end].trim().parse().unwrap_or(0.0);
        let new = ((old + TARGET - lufs) * 10.0).round() / 10.0;
        let out = format!("{}gain_db: {new:.1}{}", &text[..pos], &text[end..]);
        std::fs::write(&path, out).expect("write");
        println!("{name:22} {lufs:6.1} LUFS  gain_db {old:5.1} -> {new:5.1}");
    }
}
