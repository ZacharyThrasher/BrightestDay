//! The one output stream: the default output device, driven by the [`Mixer`] on cpal's audio
//! thread. Requests a small fixed buffer (≈ 5 ms) when the device advertises it, else the device
//! default. Coexists with other streams (bd_voice's microphone input is a separate cpal input
//! stream). The callback records its own cost in lock-free counters ([`OutputStats`]).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::mixer::Mixer;

/// Callback timing, updated from the audio thread with relaxed atomics.
#[derive(Default, Debug)]
pub struct OutputStats {
    pub callbacks: AtomicU64,
    pub frames: AtomicU64,
    /// Total and worst callback time (ns).
    pub busy_ns: AtomicU64,
    pub worst_ns: AtomicU64,
    /// Largest buffer the device asked for (frames).
    pub max_frames: AtomicU64,
}

impl OutputStats {
    /// Fraction of real time spent rendering (0..1).
    pub fn load(&self, sample_rate: u32) -> f64 {
        let frames = self.frames.load(Ordering::Relaxed).max(1) as f64;
        self.busy_ns.load(Ordering::Relaxed) as f64 / (frames / sample_rate as f64 * 1e9)
    }
}

/// Keeps the stream alive; drop to stop audio.
pub struct AudioOutput {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub channels: u16,
    /// Requested device buffer (frames), if a fixed size was accepted.
    pub buffer_frames: Option<u32>,
    pub stats: Arc<OutputStats>,
}

/// Output format the device will run at (build the engine with this sample rate first).
pub fn device_rate() -> Result<(u32, u16), String> {
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("no output device")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    Ok((cfg.sample_rate(), cfg.channels()))
}

/// Start the stream. `mixer` must have been built for [`device_rate`]'s sample rate.
/// `buffer_frames`: requested device buffer (e.g. 256 ≈ 5.3 ms at 48 kHz).
pub fn start(mut mixer: Mixer, buffer_frames: u32) -> Result<AudioOutput, String> {
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("no output device")?;
    let def = dev.default_output_config().map_err(|e| e.to_string())?;
    if def.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("unsupported output sample format {:?}", def.sample_format()));
    }
    let channels = def.channels();
    let sample_rate = def.sample_rate();
    if sample_rate != mixer.sample_rate() {
        return Err(format!("mixer built for {} Hz, device runs {} Hz", mixer.sample_rate(), sample_rate));
    }
    // cpal consumes the callback (and the mixer with it) even when building fails, so the fixed
    // size is only requested when the device advertises it.
    let fixed_ok = match def.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => (*min..=*max).contains(&buffer_frames),
        cpal::SupportedBufferSize::Unknown => false,
    };
    let mut config: cpal::StreamConfig = def.config();
    if fixed_ok {
        config.buffer_size = cpal::BufferSize::Fixed(buffer_frames);
    }
    let ch = channels as usize;
    let stats = Arc::new(OutputStats::default());
    let st = stats.clone();
    let callback = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let t0 = Instant::now();
        mixer.process(data, ch);
        let ns = t0.elapsed().as_nanos() as u64;
        let frames = (data.len() / ch.max(1)) as u64;
        st.callbacks.fetch_add(1, Ordering::Relaxed);
        st.frames.fetch_add(frames, Ordering::Relaxed);
        st.busy_ns.fetch_add(ns, Ordering::Relaxed);
        st.worst_ns.fetch_max(ns, Ordering::Relaxed);
        st.max_frames.fetch_max(frames, Ordering::Relaxed);
    };
    let err_fn = |e| eprintln!("bd_audio: output stream error: {e}");
    let stream = dev.build_output_stream(&config, callback, err_fn, None).map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(AudioOutput { _stream: stream, sample_rate, channels, buffer_frames: fixed_ok.then_some(buffer_frames), stats })
}
