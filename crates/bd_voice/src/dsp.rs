//! Pure-Rust audio helpers: downmix, resampling, level metering, energy VAD, padding.
//! Always compiled (no `whisper` feature needed) so it is unit-testable without clang/cmake.

/// Whisper's native sample rate.
pub const WHISPER_RATE: u32 = 16_000;

/// Samples for `ms` milliseconds at `rate`.
#[inline]
pub fn ms_to_samples(ms: u32, rate: u32) -> usize {
    (ms as u64 * rate as u64 / 1000) as usize
}

/// Milliseconds represented by `n` samples at `rate`.
#[inline]
pub fn samples_to_ms(n: usize, rate: u32) -> u32 {
    (n as u64 * 1000 / rate.max(1) as u64) as u32
}

/// Average interleaved frames down to mono, appending to `out`.
pub fn downmix_into(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    let inv = 1.0 / channels as f32;
    out.extend(interleaved.chunks_exact(channels).map(|f| f.iter().sum::<f32>() * inv));
}

/// Root-mean-square of a block.
pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// Map an RMS value to a perceptual 0..1 "mic level" (-60 dBFS -> 0, -6 dBFS -> 1).
pub fn level_from_rms(rms: f32) -> f32 {
    let db = 20.0 * rms.max(1e-9).log10();
    ((db + 60.0) / 54.0).clamp(0.0, 1.0)
}

/// Band-limited (Blackman-windowed sinc) resampler. Good enough for speech; O(taps) per
/// output sample (~50 taps for 48 kHz -> 16 kHz, a few ms for a multi-second clip).
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let n_out = (input.len() as f64 * ratio).floor() as usize;
    // Cutoff in cycles per input sample; leave a little transition band below Nyquist.
    let fc = 0.5 * ratio.min(1.0) * 0.92;
    // Half width = 8 zero crossings of the low-pass kernel.
    let half = (8.0 / (2.0 * fc)).ceil();
    let len = input.len() as isize;
    let mut out = Vec::with_capacity(n_out);
    for i in 0..n_out {
        let center = i as f64 / ratio;
        let lo = (center - half).ceil() as isize;
        let hi = (center + half).floor() as isize;
        let (mut acc, mut wsum) = (0.0f64, 0.0f64);
        for j in lo.max(0)..=hi.min(len - 1) {
            let t = j as f64 - center;
            let h = sinc(2.0 * fc * t) * blackman(t / half);
            acc += input[j as usize] as f64 * h;
            wsum += h;
        }
        out.push(if wsum.abs() > 1e-9 { (acc / wsum) as f32 } else { 0.0 });
    }
    out
}

#[inline]
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Blackman window over u in [-1, 1].
#[inline]
fn blackman(u: f64) -> f64 {
    if u.abs() >= 1.0 {
        return 0.0;
    }
    let a = std::f64::consts::PI * (u + 1.0); // 0..2π across the window
    0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos()
}

/// Energy-VAD tuning.
#[derive(Clone, Copy, Debug)]
pub struct VadParams {
    /// Analysis frame length.
    pub frame_ms: u32,
    /// Frames quieter than this are never speech (dBFS).
    pub abs_threshold_db: f32,
    /// Speech must be this many dB above the clip's noise floor (10th percentile frame).
    pub rel_threshold_db: f32,
    /// Padding kept around the detected speech span.
    pub pad_ms: u32,
}

impl Default for VadParams {
    fn default() -> Self {
        Self { frame_ms: 20, abs_threshold_db: -50.0, rel_threshold_db: 10.0, pad_ms: 120 }
    }
}

/// Result of [`find_speech`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeechSpan {
    /// Sample range to keep (speech span plus padding, clamped to the clip).
    pub start: usize,
    pub end: usize,
    /// Duration from the first to the last voiced frame (no padding).
    pub speech_ms: u32,
}

/// Simple energy VAD: finds the first..last frame whose energy clears an adaptive threshold.
/// Returns `None` if nothing in the clip looks like speech.
pub fn find_speech(x: &[f32], rate: u32, p: &VadParams) -> Option<SpeechSpan> {
    let frame = ms_to_samples(p.frame_ms, rate).max(1);
    if x.len() < frame {
        return None;
    }
    let db: Vec<f32> = x.chunks(frame).map(|f| 20.0 * rms(f).max(1e-7).log10()).collect();
    let mut sorted = db.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() / 10];
    let peak = *sorted.last().unwrap();
    // Threshold is relative to the noise floor, but never above (peak - 15 dB) so a clip that
    // is wall-to-wall speech (floor ~ speech level) still gets its words detected.
    let thr = p.abs_threshold_db.max((floor + p.rel_threshold_db).min(peak - 15.0));
    let first = db.iter().position(|&d| d > thr)?;
    let last = db.iter().rposition(|&d| d > thr)?;
    let pad = ms_to_samples(p.pad_ms, rate);
    let start = (first * frame).saturating_sub(pad);
    let end = ((last + 1) * frame + pad).min(x.len());
    let speech_ms = (last + 1 - first) as u32 * p.frame_ms;
    Some(SpeechSpan { start, end, speech_ms })
}

/// Pause detection for spoken chains ([`split_at_pauses`], the streaming path and the auto-cut).
#[derive(Clone, Copy, Debug)]
pub struct PauseParams {
    /// A silent run at least this long between two stretches of speech separates them (a spoken
    /// chain's beat: "teleport … rasengan"). Word gaps inside a phrase are ~50–200 ms.
    pub min_pause_ms: u32,
    /// A stretch of speech shorter than this is merged into its neighbour (a breath, a click).
    pub min_segment_ms: u32,
    /// Silence kept on each side of a cut segment (Whisper wants a little room).
    pub pad_ms: u32,
}

impl Default for PauseParams {
    fn default() -> Self {
        Self { min_pause_ms: 380, min_segment_ms: 220, pad_ms: 100 }
    }
}

/// Per-frame speech flags (the [`find_speech`] threshold) and the frame length in samples.
pub fn voiced_frames(x: &[f32], rate: u32, p: &VadParams) -> (Vec<bool>, usize) {
    let frame = ms_to_samples(p.frame_ms, rate).max(1);
    if x.len() < frame {
        return (Vec::new(), frame);
    }
    let db: Vec<f32> = x.chunks(frame).map(|f| 20.0 * rms(f).max(1e-7).log10()).collect();
    let mut sorted = db.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() / 10];
    let peak = *sorted.last().unwrap();
    let thr = p.abs_threshold_db.max((floor + p.rel_threshold_db).min(peak - 15.0));
    (db.iter().map(|&d| d > thr).collect(), frame)
}

/// Cuts a clip at its pauses: sample ranges of each stretch of speech separated by at least
/// `min_pause_ms` of silence (padded, in order). One range when there is no such pause; empty when
/// there is no speech. Chains spoken with beats ("nightcrawler teleport … rasengan … chidori") come
/// out one move per range.
pub fn split_at_pauses(x: &[f32], rate: u32, p: &PauseParams) -> Vec<(usize, usize)> {
    let vad = VadParams::default();
    let (voiced, frame) = voiced_frames(x, rate, &vad);
    let fms = vad.frame_ms.max(1);
    let min_gap = (p.min_pause_ms / fms).max(1) as usize;
    let min_seg = (p.min_segment_ms / fms).max(1) as usize;
    // runs of speech (first frame, one past the last), bridging gaps shorter than a pause
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, &v) in voiced.iter().enumerate() {
        if !v {
            continue;
        }
        match runs.last_mut() {
            Some(r) if i - r.1 < min_gap => r.1 = i + 1,
            _ => runs.push((i, i + 1)),
        }
    }
    // a blip too short to be a phrase joins its nearer neighbour
    let mut k = 0;
    while runs.len() > 1 && k < runs.len() {
        if runs[k].1 - runs[k].0 >= min_seg {
            k += 1;
            continue;
        }
        let gap_prev = if k > 0 { runs[k].0 - runs[k - 1].1 } else { usize::MAX };
        let gap_next = if k + 1 < runs.len() { runs[k + 1].0 - runs[k].1 } else { usize::MAX };
        if gap_prev <= gap_next {
            runs[k - 1].1 = runs[k].1;
        } else {
            runs[k + 1].0 = runs[k].0;
        }
        runs.remove(k);
    }
    let pad = ms_to_samples(p.pad_ms, rate);
    runs.iter().map(|&(a, b)| ((a * frame).saturating_sub(pad), (b * frame + pad).min(x.len()))).collect()
}

/// Scale so the peak sits at `target` (gain capped at `max_gain`; never attenuates below 1x
/// unless the clip clips).
pub fn normalize_peak(x: &mut [f32], target: f32, max_gain: f32) {
    let peak = x.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak < 1e-6 {
        return;
    }
    let g = (target / peak).min(max_gain);
    if (g - 1.0).abs() > 1e-3 {
        x.iter_mut().for_each(|s| *s *= g);
    }
}

/// Add `lead_ms` of silence in front, and trailing silence so the result is at least
/// `min_total_ms` long and has at least `tail_ms` of trailing silence.
pub fn pad_silence(x: &[f32], rate: u32, lead_ms: u32, tail_ms: u32, min_total_ms: u32) -> Vec<f32> {
    let lead = ms_to_samples(lead_ms, rate);
    let tail = ms_to_samples(tail_ms, rate);
    let min_total = ms_to_samples(min_total_ms, rate);
    let total = (lead + x.len() + tail).max(min_total);
    let mut out = Vec::with_capacity(total);
    out.resize(lead, 0.0);
    out.extend_from_slice(x);
    out.resize(total, 0.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, rate: u32, ms: u32, amp: f32) -> Vec<f32> {
        (0..ms_to_samples(ms, rate)).map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin()).collect()
    }

    #[test]
    fn resample_length_and_passband() {
        let x = tone(440.0, 48_000, 1000, 0.5);
        let y = resample(&x, 48_000, 16_000);
        assert_eq!(y.len(), 16_000);
        // 440 Hz passes with ~unity gain (skip edges).
        let r = rms(&y[1000..15_000]);
        assert!((r - 0.5 / 2f32.sqrt()).abs() < 0.01, "rms {r}");
    }

    #[test]
    fn resample_rejects_aliases() {
        // 12 kHz is above the 8 kHz output Nyquist: must be strongly attenuated, not folded.
        let x = tone(12_000.0, 48_000, 500, 0.5);
        let y = resample(&x, 48_000, 16_000);
        assert!(rms(&y[500..7500]) < 0.01);
        // Non-integer ratio path.
        let x = tone(300.0, 44_100, 500, 0.5);
        let y = resample(&x, 44_100, 16_000);
        assert_eq!(y.len(), 8000);
        assert!((rms(&y[500..7500]) - 0.354).abs() < 0.01);
    }

    #[test]
    fn downmix_stereo() {
        let mut out = Vec::new();
        downmix_into(&[1.0, 0.0, 0.5, 0.5], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn vad_finds_speech_between_silence() {
        let rate = 16_000;
        let mut x = vec![0.0005f32; ms_to_samples(400, rate)];
        x.extend(tone(200.0, rate, 600, 0.3));
        x.extend(vec![0.0005f32; ms_to_samples(300, rate)]);
        let s = find_speech(&x, rate, &VadParams::default()).unwrap();
        assert!((580..=640).contains(&s.speech_ms), "{s:?}");
        assert!(s.start >= ms_to_samples(250, rate) && s.start <= ms_to_samples(400, rate));
    }

    #[test]
    fn vad_rejects_silence_and_clicks() {
        let rate = 16_000;
        let x = vec![0.0f32; ms_to_samples(1000, rate)];
        assert!(find_speech(&x, rate, &VadParams::default()).is_none());
        let mut y = vec![0.0f32; ms_to_samples(1000, rate)];
        y[8000..8100].iter_mut().for_each(|s| *s = 0.8);
        let s = find_speech(&y, rate, &VadParams::default()).unwrap();
        assert!(s.speech_ms < 250);
    }

    #[test]
    fn pauses_cut_a_spoken_chain() {
        let rate = 16_000;
        let hush = |ms| vec![0.0005f32; ms_to_samples(ms, rate)];
        // "teleport" · 120 ms word gap · "behind" · 500 ms beat · "rasengan" · 450 ms · "chidori"
        let mut x = hush(200);
        x.extend(tone(200.0, rate, 400, 0.3));
        x.extend(hush(120));
        x.extend(tone(220.0, rate, 300, 0.3));
        x.extend(hush(500));
        x.extend(tone(180.0, rate, 500, 0.3));
        x.extend(hush(450));
        x.extend(tone(240.0, rate, 450, 0.3));
        x.extend(hush(250));
        let segs = split_at_pauses(&x, rate, &PauseParams::default());
        assert_eq!(segs.len(), 3, "{segs:?}");
        assert!(segs.windows(2).all(|w| w[0].1 <= w[1].0), "ordered, disjoint: {segs:?}");
        // a clip with only word gaps is one segment; silence is none
        let mut y = hush(200);
        y.extend(tone(200.0, rate, 400, 0.3));
        y.extend(hush(150));
        y.extend(tone(200.0, rate, 400, 0.3));
        y.extend(hush(200));
        assert_eq!(split_at_pauses(&y, rate, &PauseParams::default()).len(), 1);
        assert!(split_at_pauses(&hush(1000), rate, &PauseParams::default()).len() <= 1);
    }

    #[test]
    fn padding() {
        let y = pad_silence(&[1.0; 1600], 16_000, 100, 200, 1000);
        assert_eq!(y.len(), 16_000);
        assert_eq!(y[1599], 0.0);
        assert_eq!(y[1600], 1.0);
    }

    #[test]
    fn level_mapping() {
        assert_eq!(level_from_rms(0.0), 0.0);
        assert!(level_from_rms(0.5) > 0.99);
        assert!(level_from_rms(0.01) > 0.2 && level_from_rms(0.01) < 0.5);
    }
}
