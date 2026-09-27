//! VOICE BENCH stage 2 (dev tool, never shipped): every TTS clip → the game's real Whisper path.
//!
//! Built as a bd_voice example (so it links the exact engine the game uses):
//!   cargo run -p bd_voice --features whisper --profile fastrel --example voice_bench -- \
//!       --clips target/voice_bench/clips.tsv --prompt-file target/voice_bench/prompt_new.txt \
//!       --out target/voice_bench/heard_new.tsv
//!
//! `clips.tsv` (written by the corpus stage): `<clip id> \t <wav file> \t <condition>`, condition =
//! `clean` | `noise` (pink-ish noise, ~16 dB SNR) | `room` (small-room reverb + noise, ~22 dB SNR) |
//! `quiet` (-26 dB mic gain + noise). Every clip also gets the PTT framing the game's capture thread
//! gives it: 150 ms of pre-roll room tone in front, 200 ms of tail behind.
//!
//! Each clip goes through `VoiceInput::submit_clip` — resample → VAD trim → normalise → Whisper
//! (the game's `VoiceConfig`, the prompt from `--prompt-file`) → `normalize_transcript` — one at a
//! time, so `ms` is honest latency (submit → Final). The text is Whisper's RAW transcript (the
//! scorer applies `bd_voice::text`'s clean-up itself, so clean-ups compare offline). Output TSV:
//!   `<clip id> \t <text> \t <ms> \t <confidence> \t <outcome>`   (outcome: ok | discard | error)
//! With `--stream` the clip is also cut at pauses by `bd_voice::dsp::split_at_pauses` and every
//! segment transcribed on its own (the streaming path): `<clip id> \t <seg texts joined by " | ">
//! \t <ms of the FIRST segment> \t …`.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bd_voice::{VoiceConfig, VoiceEvent, VoiceInput};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut clips = root.join("target/voice_bench/clips.tsv");
    let mut wav_dir = root.join("target/voice_bench/wav");
    let mut out = root.join("target/voice_bench/heard.tsv");
    let mut prompt: Option<String> = None;
    let mut limit = usize::MAX;
    let mut stream = false;
    let mut cfg = VoiceConfig {
        model_path: root.join("assets/models/ggml-base.en-q5_1.bin"),
        capture_mic: false,
        raw_transcript: true, // the scorer applies bd_voice::text's clean-ups (old / new) itself
        ..VoiceConfig::default()
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| panic!("{a} needs a value"));
        match a.as_str() {
            "--clips" => clips = val().into(),
            "--wav-dir" => wav_dir = val().into(),
            "--out" => out = val().into(),
            "--prompt-file" => prompt = Some(std::fs::read_to_string(val()).expect("prompt file")),
            "--prompt" => prompt = Some(val()),
            "--threads" => cfg.threads = val().parse().expect("number"),
            "--ctx" => cfg.audio_ctx = val().parse().expect("number"),
            "--limit" => limit = val().parse().expect("number"),
            "--stream" => stream = true,
            other => panic!("unknown argument {other}"),
        }
    }
    cfg.initial_prompt = prompt.unwrap_or_default().trim().to_string();
    eprintln!("voice_bench: threads {} | audio_ctx {} | prompt {} chars", cfg.resolved_threads(), cfg.audio_ctx, cfg.initial_prompt.len());

    let voice = VoiceInput::new(cfg).unwrap_or_else(|e| panic!("voice: {e}"));
    loop {
        match voice.poll() {
            Some(VoiceEvent::Ready) => break,
            Some(VoiceEvent::Error(e)) => panic!("{e}"),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }

    let list = std::fs::read_to_string(&clips).unwrap_or_else(|e| panic!("{}: {e}", clips.display()));
    let mut wav_cache: HashMap<String, (Vec<f32>, u32)> = HashMap::new();
    let mut w = std::io::BufWriter::new(std::fs::File::create(&out).expect("out file"));
    let t0 = Instant::now();
    let mut n = 0usize;
    for line in list.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).take(limit) {
        let f: Vec<&str> = line.split('\t').collect();
        let (id, wav, cond) = (f[0], f[1], f.get(2).copied().unwrap_or("clean"));
        let (pcm, rate) = wav_cache.entry(wav.to_string()).or_insert_with(|| read_wav(&wav_dir.join(wav))).clone();
        let seed = fnv(id.as_bytes());
        let clip = condition(&pcm, rate, cond, seed);
        let (text, ms, conf, outcome) = if stream {
            let segs = bd_voice::dsp::split_at_pauses(&clip, rate, &bd_voice::dsp::PauseParams::default());
            let mut texts = Vec::new();
            let (mut first_ms, mut conf_min, mut outcome) = (0u32, 1.0f32, "ok");
            for (k, (a, b)) in segs.iter().enumerate() {
                let (t, ms, c, o) = run(&voice, clip[*a..*b].to_vec(), rate);
                if k == 0 {
                    first_ms = ms;
                }
                conf_min = conf_min.min(c);
                if o == "ok" {
                    texts.push(t);
                } else if outcome == "ok" && texts.is_empty() {
                    outcome = o;
                }
            }
            if !texts.is_empty() {
                outcome = "ok";
            }
            (texts.join(" | "), first_ms, conf_min, outcome)
        } else {
            run(&voice, clip, rate)
        };
        let text = text.replace(['\t', '\n', '\r'], " ");
        writeln!(w, "{id}\t{text}\t{ms}\t{conf:.3}\t{outcome}").unwrap();
        n += 1;
        if n % 100 == 0 {
            eprintln!("  {n} clips, {:.0} s", t0.elapsed().as_secs_f32());
            w.flush().ok();
        }
    }
    w.flush().unwrap();
    eprintln!("voice_bench: {n} clips in {:.1} s -> {}", t0.elapsed().as_secs_f32(), out.display());
}

fn run(voice: &VoiceInput, clip: Vec<f32>, rate: u32) -> (String, u32, f32, &'static str) {
    voice.submit_clip(clip, rate);
    loop {
        match voice.poll() {
            Some(VoiceEvent::Final { text, ms, confidence }) => return (text, ms, confidence, "ok"),
            Some(VoiceEvent::Discarded(_)) => return (String::new(), 0, 0.0, "discard"),
            Some(VoiceEvent::Error(e)) => return (e.replace('\t', " "), 0, 0.0, "error"),
            Some(_) => {}
            None => std::thread::sleep(Duration::from_micros(300)),
        }
    }
}

/// The PTT framing + the recording condition (deterministic per clip id).
fn condition(x: &[f32], rate: u32, cond: &str, seed: u64) -> Vec<f32> {
    let ms = |m: u32| (m as u64 * rate as u64 / 1000) as usize;
    let mut y = Vec::with_capacity(x.len() + ms(400));
    y.resize(ms(150), 0.0);
    y.extend_from_slice(x);
    y.resize(y.len() + ms(200), 0.0);
    let speech_rms = {
        let v: Vec<f32> = x.iter().copied().filter(|s| s.abs() > 1e-3).collect();
        if v.is_empty() { 0.05 } else { (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt() }
    };
    let mut rng = seed | 1;
    let mut noise = |gain: f32, y: &mut [f32]| {
        // Paul Kellet's economy pink filter over white noise: fan / room hum colour.
        let (mut b0, mut b1, mut b2) = (0.0f32, 0.0f32, 0.0f32);
        for s in y.iter_mut() {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let white = (rng >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0;
            b0 = 0.99765 * b0 + white * 0.0990460;
            b1 = 0.96300 * b1 + white * 0.2965164;
            b2 = 0.57000 * b2 + white * 1.0526913;
            *s += (b0 + b1 + b2 + white * 0.1848) * 0.25 * gain;
        }
    };
    match cond {
        "noise" => noise(speech_rms * 10f32.powf(-16.0 / 20.0), &mut y),
        "room" => {
            y = reverb(&y, rate);
            noise(speech_rms * 10f32.powf(-22.0 / 20.0), &mut y);
        }
        "quiet" => {
            y.iter_mut().for_each(|s| *s *= 0.05);
            noise(speech_rms * 0.05 * 10f32.powf(-20.0 / 20.0), &mut y);
        }
        _ => {}
    }
    y
}

/// Schroeder room: 4 parallel combs + 2 series allpasses (RT60 ≈ 0.4 s), 35 % wet.
fn reverb(x: &[f32], rate: u32) -> Vec<f32> {
    let sc = rate as f32 / 44_100.0;
    let combs = [1116usize, 1188, 1277, 1356].map(|d| ((d as f32 * sc) as usize).max(1));
    let tail = (0.45 * rate as f32) as usize;
    let n = x.len() + tail;
    let mut wet = vec![0.0f32; n];
    for &d in &combs {
        let g = 10f32.powf(-3.0 * d as f32 / (0.4 * rate as f32));
        let mut buf = vec![0.0f32; d];
        let mut i = 0;
        for (k, o) in wet.iter_mut().enumerate() {
            let inp = x.get(k).copied().unwrap_or(0.0);
            let out = buf[i];
            buf[i] = inp + out * g;
            *o += out * 0.25;
            i = (i + 1) % d;
        }
    }
    for &d in &[((556.0 * sc) as usize).max(1), ((441.0 * sc) as usize).max(1)] {
        let mut buf = vec![0.0f32; d];
        let mut i = 0;
        for s in wet.iter_mut() {
            let b = buf[i];
            let out = -*s + b;
            buf[i] = *s + b * 0.5;
            *s = out;
            i = (i + 1) % d;
        }
    }
    (0..n).map(|k| x.get(k).copied().unwrap_or(0.0) * 0.8 + wet[k] * 0.35).collect()
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3))
}

fn read_wav(path: &std::path::Path) -> (Vec<f32>, u32) {
    let mut r = hound::WavReader::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let spec = r.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.unwrap() as f32 * scale).collect()
        }
    };
    let mut mono = Vec::new();
    bd_voice::dsp::downmix_into(&interleaved, spec.channels as usize, &mut mono);
    (mono, spec.sample_rate)
}
