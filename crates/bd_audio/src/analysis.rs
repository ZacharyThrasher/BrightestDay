//! Offline measurement (reports, tests): BS.1770-4 integrated loudness, 4× oversampled true peak,
//! band energies and spectrograms via a small radix-2 FFT. Not used on the audio thread.

use crate::bank::kweight_filters;

/// Integrated loudness (LUFS, BS.1770-4 gating) of interleaved audio with `ch` channels at 48 kHz.
pub fn integrated_lufs(x: &[f32], ch: usize, sr: u32) -> f32 {
    let ch = ch.max(1);
    let frames = x.len() / ch;
    let block = (0.4 * sr as f32) as usize;
    let hop = block / 4;
    if frames < block {
        return -70.0;
    }
    // K-weighted squares per channel, summed across channels.
    let mut ksq = vec![0.0f64; frames];
    for c in 0..ch {
        let (mut a, mut b) = kweight_filters();
        for f in 0..frames {
            let y = b.process(a.process(x[f * ch + c])) as f64;
            ksq[f] += y * y;
        }
    }
    let mut prefix = vec![0.0f64; frames + 1];
    for f in 0..frames {
        prefix[f + 1] = prefix[f] + ksq[f];
    }
    let mut blocks = Vec::new();
    let mut s = 0;
    while s + block <= frames {
        let ms = (prefix[s + block] - prefix[s]) / block as f64;
        blocks.push(ms);
        s += hop;
    }
    let lk = |ms: f64| -0.691 + 10.0 * (ms + 1e-20).log10();
    let abs: Vec<f64> = blocks.iter().copied().filter(|&m| lk(m) > -70.0).collect();
    if abs.is_empty() {
        return -70.0;
    }
    let mean_abs = abs.iter().sum::<f64>() / abs.len() as f64;
    let rel_gate = lk(mean_abs) - 10.0;
    let rel: Vec<f64> = abs.iter().copied().filter(|&m| lk(m) > rel_gate).collect();
    if rel.is_empty() {
        return -70.0;
    }
    lk(rel.iter().sum::<f64>() / rel.len() as f64) as f32
}

/// Short-term (3 s) loudness series, one value per `step_s`.
pub fn short_term_series(x: &[f32], ch: usize, sr: u32, step_s: f32) -> Vec<f32> {
    let ch = ch.max(1);
    let frames = x.len() / ch;
    let win = (3.0 * sr as f32) as usize;
    let step = ((step_s * sr as f32) as usize).max(1);
    let mut ksq = vec![0.0f64; frames + 1];
    for c in 0..ch {
        let (mut a, mut b) = kweight_filters();
        for f in 0..frames {
            let y = b.process(a.process(x[f * ch + c])) as f64;
            ksq[f + 1] += y * y;
        }
    }
    for f in 0..frames {
        ksq[f + 1] += ksq[f];
    }
    let mut out = Vec::new();
    let mut e = step;
    while e <= frames {
        let s = e.saturating_sub(win);
        let ms = (ksq[e] - ksq[s]) / (e - s).max(1) as f64;
        out.push((-0.691 + 10.0 * (ms + 1e-20).log10()) as f32);
        e += step;
    }
    out
}

/// True peak (dBTP) with 4× windowed-sinc oversampling.
pub fn true_peak_db(x: &[f32], ch: usize) -> f32 {
    let ch = ch.max(1);
    let frames = x.len() / ch;
    const TAPS: i32 = 12; // per side
    let os = 4;
    // Precompute polyphase kernels.
    let mut kern = vec![[0.0f32; 2 * TAPS as usize]; os];
    for (p, k) in kern.iter_mut().enumerate() {
        let frac = p as f32 / os as f32;
        for (j, kj) in k.iter_mut().enumerate() {
            let t = (j as i32 - TAPS + 1) as f32 - frac;
            let sinc = if t.abs() < 1e-6 { 1.0 } else { (std::f32::consts::PI * t).sin() / (std::f32::consts::PI * t) };
            let w = 0.5 + 0.5 * (std::f32::consts::PI * t / TAPS as f32).cos();
            *kj = sinc * if t.abs() < TAPS as f32 { w } else { 0.0 };
        }
    }
    let mut peak = 0.0f32;
    for c in 0..ch {
        for f in 0..frames {
            peak = peak.max(x[f * ch + c].abs());
            for k in kern.iter().skip(1) {
                let mut acc = 0.0;
                for (j, kj) in k.iter().enumerate() {
                    let idx = f as i32 + j as i32 - TAPS + 1;
                    if idx >= 0 && (idx as usize) < frames {
                        acc += x[idx as usize * ch + c] * kj;
                    }
                }
                peak = peak.max(acc.abs());
            }
        }
    }
    20.0 * (peak.max(1e-9)).log10()
}

/// In-place iterative radix-2 FFT (`re`/`im` length must be a power of two).
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for s in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (a, b) = (s + k, s + k + len / 2);
                let tr = re[b] as f64 * cr - im[b] as f64 * ci;
                let ti = re[b] as f64 * ci + im[b] as f64 * cr;
                re[b] = (re[a] as f64 - tr) as f32;
                im[b] = (im[a] as f64 - ti) as f32;
                re[a] = (re[a] as f64 + tr) as f32;
                im[a] = (im[a] as f64 + ti) as f32;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Power spectrum frames of a mono signal (Hann window), dB, `n_fft/2` bins per frame.
pub fn spectrogram(x: &[f32], n_fft: usize, hop: usize) -> Vec<Vec<f32>> {
    let win: Vec<f32> = (0..n_fft).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n_fft as f32).cos()).collect();
    let mut out = Vec::new();
    let mut s = 0;
    let mut re = vec![0.0f32; n_fft];
    let mut im = vec![0.0f32; n_fft];
    while s + n_fft <= x.len() {
        for i in 0..n_fft {
            re[i] = x[s + i] * win[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        out.push((0..n_fft / 2).map(|k| 10.0 * (re[k] * re[k] + im[k] * im[k] + 1e-12).log10()).collect());
        s += hop;
    }
    out
}

/// Energy per band for a mono signal: `bands` are (lo, hi) Hz. Returns linear energies.
pub fn band_energies(x: &[f32], sr: u32, bands: &[(f32, f32)]) -> Vec<f64> {
    let n_fft = 4096;
    let hop = 2048;
    let bin_hz = sr as f32 / n_fft as f32;
    let mut e = vec![0.0f64; bands.len()];
    let win: Vec<f32> = (0..n_fft).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n_fft as f32).cos()).collect();
    let mut re = vec![0.0f32; n_fft];
    let mut im = vec![0.0f32; n_fft];
    let mut s = 0;
    while s + n_fft <= x.len() {
        for i in 0..n_fft {
            re[i] = x[s + i] * win[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for k in 1..n_fft / 2 {
            let f = k as f32 * bin_hz;
            let p = (re[k] * re[k] + im[k] * im[k]) as f64;
            for (bi, &(lo, hi)) in bands.iter().enumerate() {
                if f >= lo && f < hi {
                    e[bi] += p;
                }
            }
        }
        s += hop;
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lufs_of_reference_sine() {
        // 1 kHz sine at -18 dBFS in both channels ≈ -18 LUFS (+ ~0.7 K-weight + 3 dB stereo sum
        // − 3 dB for the sine's crest): BS.1770 gives ≈ -18.0 for a 0 dBFS-peak-scaled reference
        // at 997 Hz with −20 dBFS RMS… here we just check the relation to a known offset.
        let sr = 48000;
        let a = 10f32.powf(-18.0 / 20.0);
        let x: Vec<f32> = (0..sr * 5)
            .flat_map(|i| {
                let s = a * (2.0 * std::f32::consts::PI * 997.0 * i as f32 / sr as f32).sin();
                [s, s]
            })
            .collect();
        let l = integrated_lufs(&x, 2, sr as u32);
        assert!((l - -18.0).abs() < 1.0, "{l}");
    }

    #[test]
    fn fft_finds_a_tone() {
        let n = 1024;
        let mut re: Vec<f32> = (0..n).map(|i| (2.0 * std::f32::consts::PI * 64.0 * i as f32 / n as f32).sin()).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let mag: Vec<f32> = (0..n / 2).map(|k| re[k].hypot(im[k])).collect();
        let peak = mag.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(peak, 64);
    }

    #[test]
    fn true_peak_catches_intersample_overs() {
        // fs/4 sine sampled at 45° phase: samples at 0.707, true peak 1.0.
        let x: Vec<f32> = (0..4800).map(|i| (std::f32::consts::FRAC_PI_2 * i as f32 + std::f32::consts::FRAC_PI_4).sin()).collect();
        let tp = true_peak_db(&x, 1);
        assert!(tp > -0.5, "{tp}");
    }
}
