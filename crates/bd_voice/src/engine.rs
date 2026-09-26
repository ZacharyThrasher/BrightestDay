//! Real implementation (feature `whisper`): cpal capture thread + whisper.cpp worker thread.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded, unbounded};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use crate::dsp::{self, WHISPER_RATE};
use crate::text::normalize_transcript;
use crate::{DiscardReason, VoiceConfig, VoiceError, VoiceEvent};

const STATUS_LOADING: u8 = 0;
const STATUS_READY: u8 = 1;
const STATUS_FAILED: u8 = 2;

/// Messages into the capture thread. Audio and PTT edges share one channel so ordering is exact.
enum CapMsg {
    Audio(Vec<f32>),
    Ptt(bool, Instant),
    Shutdown,
}

/// A finished clip for the inference thread.
struct Job {
    samples: Vec<f32>,
    rate: u32,
    released: Instant,
}

/// Push-to-talk voice input. Cheap to poll every frame; `Send + Sync` (usable as a Bevy
/// `Resource` behind `Res`, all methods take `&self`).
pub struct VoiceInput {
    held: AtomicBool,
    cap_tx: Option<Sender<CapMsg>>,
    job_tx: Sender<Job>,
    events: Receiver<VoiceEvent>,
    status: Arc<AtomicU8>,
    mic: Option<(String, u32)>,
}

impl VoiceInput {
    /// Opens the default microphone (unless `config.capture_mic == false`) and starts loading
    /// the model on a background thread. Returns quickly; watch for [`VoiceEvent::Ready`].
    pub fn new(config: VoiceConfig) -> Result<Self, VoiceError> {
        if !config.model_path.is_file() {
            return Err(VoiceError::Model(format!(
                "model not found at {} (run tools/voice/fetch_model.ps1)",
                config.model_path.display()
            )));
        }
        let (ev_tx, events) = unbounded::<VoiceEvent>();
        let (job_tx, job_rx) = unbounded::<Job>();
        let status = Arc::new(AtomicU8::new(STATUS_LOADING));

        let (cap_tx, mic) = if config.capture_mic {
            let (tx, info) = spawn_capture(&config, job_tx.clone(), ev_tx.clone(), status.clone())?;
            (Some(tx), Some(info))
        } else {
            (None, None)
        };

        {
            let status = status.clone();
            let config = config.clone();
            thread::Builder::new()
                .name("bd_voice-whisper".into())
                .spawn(move || inference_thread(config, job_rx, ev_tx, status))
                .map_err(|e| VoiceError::Model(format!("spawn failed: {e}")))?;
        }

        Ok(Self { held: AtomicBool::new(false), cap_tx, job_tx, events, status, mic })
    }

    /// Call every frame with whether the voice key is held. Only edges do any work.
    pub fn set_push_to_talk(&self, held: bool) {
        if self.held.swap(held, Ordering::Relaxed) != held {
            if let Some(tx) = &self.cap_tx {
                let _ = tx.send(CapMsg::Ptt(held, Instant::now()));
            }
        }
    }

    /// Next pending event, if any. Drain in a loop each frame.
    pub fn poll(&self) -> Option<VoiceEvent> {
        self.events.try_recv().ok()
    }

    /// Transcribe an already-recorded mono clip (any sample rate) through the same
    /// resample → VAD → Whisper pipeline. Latency in `Final::ms` counts from this call.
    pub fn submit_clip(&self, samples: Vec<f32>, sample_rate: u32) {
        let _ = self.job_tx.send(Job { samples, rate: sample_rate, released: Instant::now() });
    }

    /// Model loaded and warmed up.
    pub fn is_ready(&self) -> bool {
        self.status.load(Ordering::Relaxed) == STATUS_READY
    }

    /// Name and native sample rate of the opened input device (`None` without mic capture).
    pub fn mic_info(&self) -> Option<(&str, u32)> {
        self.mic.as_ref().map(|(n, r)| (n.as_str(), *r))
    }
}

impl Drop for VoiceInput {
    fn drop(&mut self) {
        // The capture thread drops the cpal stream and exits; the inference thread exits when
        // `job_tx` (and the capture thread's clone) are gone. Neither is joined, so dropping
        // never blocks the game on an in-flight inference.
        if let Some(tx) = &self.cap_tx {
            let _ = tx.send(CapMsg::Shutdown);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------------------------

/// Spawns the mic thread; it owns the cpal stream (which is `!Send` on some hosts) and runs
/// the push-to-talk state machine. Waits for the stream to open so errors surface from `new`.
fn spawn_capture(
    config: &VoiceConfig,
    job_tx: Sender<Job>,
    ev_tx: Sender<VoiceEvent>,
    status: Arc<AtomicU8>,
) -> Result<(Sender<CapMsg>, (String, u32)), VoiceError> {
    let (cap_tx, cap_rx) = unbounded::<CapMsg>();
    let (ready_tx, ready_rx) = bounded::<Result<(String, u32), String>>(1);
    let cfg = config.clone();
    let audio_tx = cap_tx.clone();
    thread::Builder::new()
        .name("bd_voice-mic".into())
        .spawn(move || {
            let stream = match open_mic(audio_tx, ev_tx.clone()) {
                Ok((stream, name, rate)) => {
                    let _ = ready_tx.send(Ok((name, rate)));
                    (stream, rate)
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            capture_loop(&cfg, stream.1, cap_rx, job_tx, ev_tx, status);
            drop(stream);
        })
        .map_err(|e| VoiceError::Audio(format!("spawn failed: {e}")))?;

    match ready_rx.recv() {
        Ok(Ok(info)) => Ok((cap_tx, info)),
        Ok(Err(e)) => Err(VoiceError::Audio(e)),
        Err(_) => Err(VoiceError::Audio("capture thread died".into())),
    }
}

fn open_mic(tx: Sender<CapMsg>, ev_tx: Sender<VoiceEvent>) -> Result<(cpal::Stream, String, u32), String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("no default input device")?;
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "unknown".into());
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let rate = config.sample_rate;

    let err_fn = move |e: cpal::StreamError| {
        let _ = ev_tx.send(VoiceEvent::Error(format!("microphone stream error: {e}")));
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, channels, tx, err_fn),
        SampleFormat::I16 => build::<i16>(&device, &config, channels, tx, err_fn),
        SampleFormat::U16 => build::<u16>(&device, &config, channels, tx, err_fn),
        SampleFormat::I32 => build::<i32>(&device, &config, channels, tx, err_fn),
        SampleFormat::I8 => build::<i8>(&device, &config, channels, tx, err_fn),
        SampleFormat::U8 => build::<u8>(&device, &config, channels, tx, err_fn),
        SampleFormat::F64 => build::<f64>(&device, &config, channels, tx, err_fn),
        other => return Err(format!("unsupported mic sample format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, name, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    tx: Sender<CapMsg>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| {
            let mut mono = Vec::with_capacity(data.len() / channels.max(1));
            if channels <= 1 {
                mono.extend(data.iter().map(|&s| f32::from_sample_(s)));
            } else {
                let inv = 1.0 / channels as f32;
                mono.extend(
                    data.chunks_exact(channels)
                        .map(|f| f.iter().map(|&s| f32::from_sample_(s)).sum::<f32>() * inv),
                );
            }
            let _ = tx.send(CapMsg::Audio(mono));
        },
        err_fn,
        None,
    )
}

enum CapState {
    Idle,
    Recording { clip: Vec<f32> },
    Tail { clip: Vec<f32>, released: Instant, remaining: usize },
}

fn capture_loop(
    cfg: &VoiceConfig,
    rate: u32,
    rx: Receiver<CapMsg>,
    job_tx: Sender<Job>,
    ev_tx: Sender<VoiceEvent>,
    status: Arc<AtomicU8>,
) {
    let pre_cap = dsp::ms_to_samples(cfg.pre_roll_ms, rate);
    let tail_len = dsp::ms_to_samples(cfg.tail_ms, rate);
    let max_len = dsp::ms_to_samples(cfg.max_clip_ms, rate);
    let level_block = (rate / 30).max(1) as usize;
    let tail_timeout = Duration::from_millis(cfg.tail_ms as u64 + 250);

    let mut ring: VecDeque<f32> = VecDeque::with_capacity(pre_cap + 4096);
    let mut state = CapState::Idle;
    let (mut lvl_acc, mut lvl_n, mut lvl_smooth) = (0.0f32, 0usize, 0.0f32);

    let finish = |clip: Vec<f32>, released: Instant| {
        let _ = job_tx.send(Job { samples: clip, rate, released });
    };

    loop {
        let msg = match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(m) => Some(m),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        state = match (msg, state) {
            (Some(CapMsg::Shutdown), _) => break,

            (Some(CapMsg::Ptt(true, _)), CapState::Idle) => {
                if status.load(Ordering::Relaxed) == STATUS_FAILED {
                    let _ = ev_tx.send(VoiceEvent::Error("speech model failed to load".into()));
                    CapState::Idle
                } else {
                    let _ = ev_tx.send(VoiceEvent::Listening);
                    lvl_smooth = 0.0;
                    let mut clip = Vec::with_capacity(rate as usize * 3);
                    clip.extend(ring.drain(..));
                    CapState::Recording { clip }
                }
            }
            // Re-pressed during the tail: keep extending the same clip.
            (Some(CapMsg::Ptt(true, _)), CapState::Tail { clip, .. }) => {
                let _ = ev_tx.send(VoiceEvent::Listening);
                CapState::Recording { clip }
            }
            (Some(CapMsg::Ptt(false, t)), CapState::Recording { clip }) => {
                CapState::Tail { clip, released: t, remaining: tail_len }
            }
            (Some(CapMsg::Ptt(_, _)), s) => s,

            (Some(CapMsg::Audio(chunk)), CapState::Idle) => {
                ring.extend(chunk);
                let excess = ring.len().saturating_sub(pre_cap);
                ring.drain(..excess);
                CapState::Idle
            }
            (Some(CapMsg::Audio(chunk)), CapState::Recording { mut clip }) => {
                for &s in &chunk {
                    lvl_acc += s * s;
                    lvl_n += 1;
                    if lvl_n >= level_block {
                        let l = dsp::level_from_rms((lvl_acc / lvl_n as f32).sqrt());
                        // Fast attack, slower release.
                        let k = if l > lvl_smooth { 0.6 } else { 0.25 };
                        lvl_smooth += (l - lvl_smooth) * k;
                        let _ = ev_tx.send(VoiceEvent::Level(lvl_smooth));
                        lvl_acc = 0.0;
                        lvl_n = 0;
                    }
                }
                clip.extend_from_slice(&chunk);
                if clip.len() >= max_len {
                    // Auto-release: stop here, no tail.
                    finish(clip, Instant::now());
                    CapState::Idle
                } else {
                    CapState::Recording { clip }
                }
            }
            (Some(CapMsg::Audio(chunk)), CapState::Tail { mut clip, released, remaining }) => {
                let n = remaining.min(chunk.len());
                clip.extend_from_slice(&chunk[..n]);
                if n == remaining {
                    finish(clip, released);
                    ring.extend(chunk[n..].iter().copied());
                    CapState::Idle
                } else {
                    CapState::Tail { clip, released, remaining: remaining - n }
                }
            }

            // No audio arriving (device stalled): don't hang in the tail forever.
            (None, CapState::Tail { clip, released, .. }) if released.elapsed() > tail_timeout => {
                finish(clip, released);
                CapState::Idle
            }
            (None, s) => s,
        };
    }
}

// ---------------------------------------------------------------------------------------------
// Inference
// ---------------------------------------------------------------------------------------------

struct Engine {
    state: WhisperState,
    _ctx: WhisperContext,
    params: FullParams<'static, 'static>,
    audio_ctx: i32,
}

impl Engine {
    fn load(cfg: &VoiceConfig) -> Result<Self, String> {
        if !cfg.whisper_logs {
            // With no log backend feature enabled this routes whisper/ggml logs to nowhere.
            whisper_rs::install_logging_hooks();
        }
        let mut cp = WhisperContextParameters::default();
        cp.use_gpu(false); // the game owns the GPU
        cp.flash_attn(cfg.flash_attn);
        let ctx = WhisperContext::new_with_params(&cfg.model_path, cp)
            .map_err(|e| format!("loading {}: {e}", cfg.model_path.display()))?;
        let state = ctx.create_state().map_err(|e| format!("whisper state: {e}"))?;

        // Built once and cloned per clip: whisper-rs leaks the CStrings passed to
        // set_language / set_initial_prompt, so never call those per inference.
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_n_threads(cfg.resolved_threads() as i32);
        p.set_language(Some("en"));
        p.set_detect_language(false);
        p.set_translate(false);
        p.set_no_context(true);
        p.set_no_timestamps(true);
        p.set_single_segment(true);
        p.set_token_timestamps(false);
        p.set_print_special(false);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        p.set_suppress_blank(true);
        p.set_suppress_nst(true);
        p.set_temperature(0.0);
        p.set_temperature_inc(0.0); // no temperature fallback: bounded latency
        p.set_max_tokens(32); // commands are short; stops runaway repetition
        let prompt = cfg.initial_prompt.replace('\0', " ");
        if !prompt.trim().is_empty() {
            p.set_initial_prompt(prompt.trim());
        }

        Ok(Self { state, _ctx: ctx, params: p, audio_ctx: if cfg.audio_ctx == 0 { 1500 } else { cfg.audio_ctx.min(1500) as i32 } })
    }

    /// `pcm` is 16 kHz mono, already trimmed and padded.
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String, String> {
        let mut p = self.params.clone();
        if self.audio_ctx < 1500 {
            // Encoder runs 50 frames per second of audio; 1500 = the full 30 s window.
            let secs = pcm.len() as f32 / WHISPER_RATE as f32;
            let fit = (secs * 50.0).ceil() as i32 + 32;
            p.set_audio_ctx(self.audio_ctx.max(fit).min(1500));
        }
        self.state.full(p, pcm).map_err(|e| format!("whisper: {e}"))?;
        let mut text = String::new();
        for seg in self.state.as_iter() {
            if let Ok(s) = seg.to_str_lossy() {
                text.push_str(&s);
                text.push(' ');
            }
        }
        Ok(text)
    }
}

fn inference_thread(cfg: VoiceConfig, jobs: Receiver<Job>, ev_tx: Sender<VoiceEvent>, status: Arc<AtomicU8>) {
    let mut engine = match Engine::load(&cfg) {
        Ok(e) => e,
        Err(e) => {
            status.store(STATUS_FAILED, Ordering::Relaxed);
            let _ = ev_tx.send(VoiceEvent::Error(e));
            // Drain so submitted clips get an answer instead of silence.
            for _ in jobs {
                let _ = ev_tx.send(VoiceEvent::Error("speech model failed to load".into()));
            }
            return;
        }
    };
    if cfg.warm_up {
        // 1 s of faint noise: primes allocations / caches so the first real cast is fast.
        let mut seed = 0x2545F491u32;
        let noise: Vec<f32> = (0..WHISPER_RATE as usize)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed as f32 / u32::MAX as f32 - 0.5) * 0.002
            })
            .collect();
        let _ = engine.transcribe(&noise);
    }
    status.store(STATUS_READY, Ordering::Relaxed);
    let _ = ev_tx.send(VoiceEvent::Ready);

    for job in jobs {
        let _ = ev_tx.send(VoiceEvent::Transcribing);
        let ev = process(&mut engine, &cfg, job);
        let _ = ev_tx.send(ev);
    }
}

fn process(engine: &mut Engine, cfg: &VoiceConfig, job: Job) -> VoiceEvent {
    let pcm = dsp::resample(&job.samples, job.rate, WHISPER_RATE);
    let Some(span) = dsp::find_speech(&pcm, WHISPER_RATE, &cfg.vad) else {
        return VoiceEvent::Discarded(DiscardReason::NoSpeech);
    };
    if span.speech_ms < cfg.min_speech_ms {
        return VoiceEvent::Discarded(DiscardReason::TooShort);
    }
    let mut speech = pcm[span.start..span.end].to_vec();
    dsp::normalize_peak(&mut speech, 0.8, 20.0);
    let padded = dsp::pad_silence(&speech, WHISPER_RATE, 100, 300, 1_000);
    match engine.transcribe(&padded) {
        Ok(raw) => {
            let text = normalize_transcript(&raw);
            if text.is_empty() {
                VoiceEvent::Discarded(DiscardReason::NoSpeech)
            } else {
                VoiceEvent::Final { text, ms: job.released.elapsed().as_millis() as u32 }
            }
        }
        Err(e) => VoiceEvent::Error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drives the push-to-talk state machine with synthetic 10 ms chunks (no device needed)
    /// and checks pre-roll + held audio + tail end up in the clip.
    #[test]
    fn capture_preroll_and_tail() {
        let rate = 48_000u32;
        let chunk = 480usize; // 10 ms
        let cfg = VoiceConfig::default(); // 150 ms pre-roll, 200 ms tail
        let (cap_tx, cap_rx) = unbounded();
        let (job_tx, job_rx) = unbounded();
        let (ev_tx, ev_rx) = unbounded();
        let status = Arc::new(AtomicU8::new(STATUS_READY));
        let h = thread::spawn(move || capture_loop(&cfg, rate, cap_rx, job_tx, ev_tx, status));

        let mut n = 0.0f32;
        let mut audio = |count: usize, amp: f32| {
            for _ in 0..count {
                let c: Vec<f32> = (0..chunk)
                    .map(|_| {
                        n += 1.0;
                        amp * (n * 0.05).sin()
                    })
                    .collect();
                cap_tx.send(CapMsg::Audio(c)).unwrap();
            }
        };
        audio(50, 0.01); // idle: only the last 150 ms is kept
        cap_tx.send(CapMsg::Ptt(true, Instant::now())).unwrap();
        audio(100, 0.3); // held 1 s
        cap_tx.send(CapMsg::Ptt(false, Instant::now())).unwrap();
        audio(30, 0.01); // 300 ms after release; only 200 ms is taken
        let job = job_rx.recv_timeout(Duration::from_secs(2)).expect("clip");
        cap_tx.send(CapMsg::Shutdown).unwrap();
        h.join().unwrap();

        assert_eq!(job.rate, rate);
        assert_eq!(job.samples.len(), 7_200 + 48_000 + 9_600);
        let evs: Vec<_> = ev_rx.try_iter().collect();
        assert_eq!(evs.first(), Some(&VoiceEvent::Listening));
        let levels = evs.iter().filter(|e| matches!(e, VoiceEvent::Level(l) if *l > 0.5)).count();
        assert!(levels >= 20, "expected ~30 loud level events, got {levels}");
    }
}
