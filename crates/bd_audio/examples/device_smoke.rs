//! Opens the real output device through bd_audio (one cpal stream), plays a short sequence of
//! game sounds for ~4 s, and prints the callback budget. Proves the device path works and is
//! real-time safe on this machine (the offline renderer covers what it sounds like).
//!
//!     cargo run --release -p bd_audio --example device_smoke

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use bd_audio::*;
use glam::Vec3;

fn main() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let (rate, channels) = match output::device_rate() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("no output device: {e}");
            return;
        }
    };
    let a = AudioAssets::load(&assets).expect("assets");
    let (mut engine, mixer) = AudioEngine::new(a.bank.clone(), &a.cues, a.mix.clone(), rate, 1);
    let latency = mixer.latency_frames();
    let out = match output::start(mixer, 256) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("could not start output: {e}");
            return;
        }
    };
    println!("device: {rate} Hz, {channels} ch, requested buffer {:?}, mixer latency {latency} frames", out.buffer_frames);
    engine.set_listener(Vec3::new(0.0, 1.6, 0.0), Vec3::NEG_Z, Vec3::Y);
    engine.set_settings(AudioSettings { master: 0.2, ..Default::default() });
    let script: [(f32, &str, Emit); 8] = [
        (0.2, "construct_form", Emit::Own),
        (0.8, "gun_handcannon", Emit::At(Vec3::new(-4.0, 1.5, -8.0))),
        (1.0, "hit_confirm", Emit::Ui),
        (1.4, "hit_heavy", Emit::At(Vec3::new(3.0, 1.0, -5.0))),
        (1.9, "explosion_big", Emit::At(Vec3::new(10.0, 0.0, -30.0))),
        (2.6, "footstep", Emit::Own),
        (2.9, "footstep", Emit::Own),
        (3.2, "ko", Emit::Ui),
    ];
    let t0 = Instant::now();
    let mut next = 0;
    while t0.elapsed() < Duration::from_secs_f32(4.0) {
        let t = t0.elapsed().as_secs_f32();
        while next < script.len() && script[next].0 <= t {
            let (_, name, emit) = script[next];
            engine.play(name, PlayOpts { emit, ..Default::default() });
            next += 1;
        }
        engine.advance(0.016);
        std::thread::sleep(Duration::from_millis(16));
    }
    let s = &out.stats;
    let cb = s.callbacks.load(Ordering::Relaxed).max(1);
    println!(
        "callbacks {cb}, max buffer {} frames ({:.1} ms), mean callback {:.0} us, worst {:.0} us, load {:.2}%",
        s.max_frames.load(Ordering::Relaxed),
        s.max_frames.load(Ordering::Relaxed) as f64 / rate as f64 * 1000.0,
        s.busy_ns.load(Ordering::Relaxed) as f64 / cb as f64 / 1000.0,
        s.worst_ns.load(Ordering::Relaxed) as f64 / 1000.0,
        s.load(rate) * 100.0
    );
    let t = engine.telemetry();
    println!("telemetry: real {} virtual {} hdr {:.1} dB peak {:.2}", t.real, t.virt, t.hdr_gain_db, t.peak);
}
