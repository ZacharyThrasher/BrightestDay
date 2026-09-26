//! Console demo for bd_voice.
//!
//! WAV mode (no mic; used for the latency numbers in README.md):
//!   cargo run -p bd_voice --features whisper --release --example ptt -- tools/voice/testwav/*.wav
//! Mic mode (press Enter to start talking, Enter again to stop; Ctrl+C / "q" to quit):
//!   cargo run -p bd_voice --features whisper --release --example ptt
//!
//! Options: --model PATH  --threads N  --prompt "..."  --no-prompt  --ctx N  --flash  --repeat N

use std::io::BufRead;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bd_voice::{VoiceConfig, VoiceEvent, VoiceInput};

const DEFAULT_PROMPT: &str =
    "Mjolnir, Pikachu, Godzilla, Charizard, Batman, hammer, shield, sword, wall, flaming, ice, giant.";

fn main() {
    let mut cfg = VoiceConfig {
        model_path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/ggml-base.en-q5_1.bin"),
        initial_prompt: DEFAULT_PROMPT.to_string(),
        ..VoiceConfig::default()
    };
    let mut wavs = Vec::new();
    let mut repeat = 1usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--model" => cfg.model_path = args.next().expect("--model PATH").into(),
            "--threads" => cfg.threads = args.next().expect("--threads N").parse().expect("number"),
            "--prompt" => cfg.initial_prompt = args.next().expect("--prompt TEXT"),
            "--no-prompt" => cfg.initial_prompt.clear(),
            "--ctx" => cfg.audio_ctx = args.next().expect("--ctx N").parse().expect("number"),
            "--flash" => cfg.flash_attn = true,
            "--repeat" => repeat = args.next().expect("--repeat N").parse().expect("number"),
            "--logs" => cfg.whisper_logs = true,
            _ => wavs.push(PathBuf::from(a)),
        }
    }
    cfg.capture_mic = wavs.is_empty();
    println!(
        "model {} | threads {} | audio_ctx {} | flash_attn {} | prompt {:?}",
        cfg.model_path.display(),
        cfg.resolved_threads(),
        cfg.audio_ctx,
        cfg.flash_attn,
        cfg.initial_prompt
    );

    let t0 = Instant::now();
    let voice = match VoiceInput::new(cfg) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    // Wait for the model.
    loop {
        match voice.poll() {
            Some(VoiceEvent::Ready) => break,
            Some(VoiceEvent::Error(e)) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    println!("model loaded + warmed up in {} ms", t0.elapsed().as_millis());
    if let Some((name, rate)) = voice.mic_info() {
        println!("mic: {name} @ {rate} Hz");
    }

    if wavs.is_empty() {
        mic_mode(&voice);
    } else {
        wav_mode(&voice, &wavs, repeat);
    }
}

fn wav_mode(voice: &VoiceInput, wavs: &[PathBuf], repeat: usize) {
    for path in wavs {
        let (samples, rate) = read_wav(path);
        let mut times = Vec::new();
        let mut last = String::new();
        for _ in 0..repeat {
            voice.submit_clip(samples.clone(), rate);
            match wait_result(voice) {
                VoiceEvent::Final { text, ms } => {
                    times.push(ms);
                    last = format!("{text:?}");
                }
                other => last = format!("{other:?}"),
            }
        }
        times.sort();
        let name = path.file_name().unwrap().to_string_lossy();
        let dur = samples.len() as f32 / rate as f32;
        if times.is_empty() {
            println!("{name:<32} {dur:4.2}s -> {last}");
        } else {
            println!(
                "{name:<32} {dur:4.2}s -> {last:<30} median {} ms (min {} / max {}, n={})",
                times[times.len() / 2],
                times[0],
                times[times.len() - 1],
                times.len()
            );
        }
    }
}

fn wait_result(voice: &VoiceInput) -> VoiceEvent {
    loop {
        match voice.poll() {
            Some(ev @ (VoiceEvent::Final { .. } | VoiceEvent::Discarded(_) | VoiceEvent::Error(_))) => return ev,
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(1)),
        }
    }
}

fn mic_mode(voice: &VoiceInput) {
    println!("Press Enter to start talking, Enter again to stop. Type q + Enter to quit.");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut held = false;
    let mut released_at: Option<Instant> = None;
    loop {
        match rx.try_recv() {
            Ok(line) if line.trim() == "q" => return,
            Ok(_) => {
                held = !held;
                if !held {
                    released_at = Some(Instant::now());
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            Err(_) => {}
        }
        voice.set_push_to_talk(held);
        while let Some(ev) = voice.poll() {
            match ev {
                VoiceEvent::Listening => println!("[listening… press Enter to stop]"),
                VoiceEvent::Level(l) => {
                    let n = (l * 30.0) as usize;
                    print!("\r{:<30}|", "#".repeat(n));
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
                VoiceEvent::Transcribing => println!("\n[transcribing…]"),
                VoiceEvent::Final { text, ms } => {
                    let wall = released_at.map(|t| t.elapsed().as_millis()).unwrap_or(0);
                    println!("=> {text:?}  ({ms} ms release->final, {wall} ms wall)");
                }
                other => println!("{other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read_wav(path: &PathBuf) -> (Vec<f32>, u32) {
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
