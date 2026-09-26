//! Offline measurement for the render report and tests: ITU-R BS.1770-4 integrated loudness,
//! 4× oversampled true peak, averaged power spectrum by band, and a minimal WAV writer.
//! (Not used on the audio thread.)

use std::io::Write;
use std::path::Path;

#[derive(Clone, Copy)]
struct Bq {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Bq {
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

fn k_weighting(sr: f64) -> (Bq, Bq) {
    use std::f64::consts::PI;
    // Stage 1: high shelf (+4 dB above ~1.7 kHz).
    let f0 = 1_681.974_450_955_533;
    let g = 3.999_843_853_973_347;
    let q = 0.707_175_236_955_419_6;
    let k = (PI * f0 / sr).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let s1 = Bq {
        b: [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        z: [0.0; 2],
    };
    // Stage 2: RLB high-pass.
    let f0 = 38.135_470_876_024_44;
    let q = 0.500_327_037_323_877_3;
    let k = (PI * f0 / sr).tan();
    let d = 1.0 + k / q + k * k;
    let s2 = Bq { b: [1.0, -2.0, 1.0], a: [2.0 * (k * k - 1.0) / d, (1.0 - k / q + k * k) / d], z: [0.0; 2] };
    (s1, s2)
}

/// Integrated loudness (LUFS) of interleaved stereo.
pub fn integrated_lufs(stereo: &[f32], sr: u32) -> f32 {
    let frames = stereo.len() / 2;
    let srf = f64::from(sr);
    let (mut l1, mut l2) = k_weighting(srf);
    let (mut r1, mut r2) = k_weighting(srf);
    // Mean square per 100 ms hop.
    let hop = (srf * 0.1) as usize;
    let mut hops = Vec::new();
    let mut acc = 0.0f64;
    let mut n = 0usize;
    for f in 0..frames {
        let l = l2.run(l1.run(f64::from(stereo[2 * f])));
        let r = r2.run(r1.run(f64::from(stereo[2 * f + 1])));
        acc += l * l + r * r;
        n += 1;
        if n == hop {
            hops.push(acc / hop as f64);
            acc = 0.0;
            n = 0;
        }
    }
    if hops.len() < 4 {
        return -70.0;
    }
    let blocks: Vec<f64> = hops.windows(4).map(|w| w.iter().sum::<f64>() / 4.0).collect();
    let lufs = |ms: f64| -0.691 + 10.0 * ms.max(1e-12).log10();
    let abs: Vec<f64> = blocks.iter().copied().filter(|&m| lufs(m) > -70.0).collect();
    if abs.is_empty() {
        return -70.0;
    }
    let mean_abs = abs.iter().sum::<f64>() / abs.len() as f64;
    let rel_gate = lufs(mean_abs) - 10.0;
    let rel: Vec<f64> = abs.iter().copied().filter(|&m| lufs(m) > rel_gate).collect();
    if rel.is_empty() {
        return -70.0;
    }
    lufs(rel.iter().sum::<f64>() / rel.len() as f64) as f32
}

/// Momentary loudness (LUFS, 400 ms window) every 100 ms.
pub fn momentary_lufs(stereo: &[f32], sr: u32) -> Vec<f32> {
    let frames = stereo.len() / 2;
    let srf = f64::from(sr);
    let (mut l1, mut l2) = k_weighting(srf);
    let (mut r1, mut r2) = k_weighting(srf);
    let hop = (srf * 0.1) as usize;
    let mut hops = Vec::new();
    let mut acc = 0.0f64;
    for f in 0..frames {
        let l = l2.run(l1.run(f64::from(stereo[2 * f])));
        let r = r2.run(r1.run(f64::from(stereo[2 * f + 1])));
        acc += l * l + r * r;
        if (f + 1) % hop == 0 {
            hops.push(acc / hop as f64);
            acc = 0.0;
        }
    }
    hops.windows(4).map(|w| (-0.691 + 10.0 * (w.iter().sum::<f64>() / 4.0).max(1e-12).log10()) as f32).collect()
}

/// True peak (dBTP) via 4× windowed-sinc oversampling of each channel.
pub fn true_peak_db(stereo: &[f32]) -> f32 {
    const TAPS: usize = 12; // per phase (48 total)
    let mut kernel = [[0.0f32; TAPS]; 4];
    for (p, row) in kernel.iter_mut().enumerate() {
        let frac = p as f64 / 4.0;
        for (t, k) in row.iter_mut().enumerate() {
            let x = t as f64 - (TAPS / 2) as f64 + 1.0 - frac;
            let sinc = if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
            let w = 0.5 + 0.5 * (std::f64::consts::PI * x / (TAPS as f64 / 2.0 + 1.0)).cos();
            *k = (sinc * w) as f32;
        }
    }
    let mut peak = 0.0f32;
    for ch in 0..2 {
        let x: Vec<f32> = stereo.iter().skip(ch).step_by(2).copied().collect();
        for i in TAPS..x.len() {
            peak = peak.max(x[i].abs());
            for row in &kernel[1..] {
                let mut s = 0.0;
                for (t, k) in row.iter().enumerate() {
                    s += x[i - TAPS + 1 + t] * k;
                }
                peak = peak.max(s.abs());
            }
        }
    }
    20.0 * peak.max(1e-9).log10()
}

/// Sample peak (dBFS).
pub fn sample_peak_db(stereo: &[f32]) -> f32 {
    let p = stereo.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    20.0 * p.max(1e-9).log10()
}

fn fft(re: &mut [f64], im: &mut [f64]) {
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
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Magnitude spectrogram (dB, mid channel): one column of `n / 2` bins every `hop` frames.
pub fn spectrogram_db(stereo: &[f32], n: usize, hop: usize) -> Vec<Vec<f32>> {
    let n = n.next_power_of_two().max(64);
    let mono: Vec<f64> = stereo.as_chunks::<2>().0.iter().map(|c| f64::from(c[0] + c[1]) * 0.5).collect();
    let win: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos()).collect();
    let mut cols = Vec::new();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    let mut start = 0;
    while start + n <= mono.len() {
        for i in 0..n {
            re[i] = mono[start + i] * win[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        let norm = 2.0 / (n as f64 * 0.5);
        cols.push((0..n / 2).map(|k| (20.0 * ((re[k].hypot(im[k]) * norm).max(1e-9)).log10()) as f32).collect());
        start += hop.max(1);
    }
    cols
}

/// Single-frame magnitude spectrum (dB) of a mono signal with a 4-term Blackman-Harris window
/// (-92 dB sidelobes: a clean floor for measuring aliasing). Uses the largest power-of-two prefix.
pub fn spectrum_db_blackman_harris(mono: &[f32]) -> Vec<f32> {
    let mut n = 1;
    while n * 2 <= mono.len() {
        n *= 2;
    }
    let (a0, a1, a2, a3) = (0.358_75, 0.488_29, 0.141_28, 0.011_68);
    let mut re: Vec<f64> = (0..n)
        .map(|i| {
            let x = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            f64::from(mono[i]) * (a0 - a1 * x.cos() + a2 * (2.0 * x).cos() - a3 * (3.0 * x).cos())
        })
        .collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    (0..n / 2).map(|k| (20.0 * re[k].hypot(im[k]).max(1e-12).log10()) as f32).collect()
}

/// Band edges (Hz) used by the report.
pub const BANDS: [(f32, f32); 8] = [
    (0.0, 60.0),
    (60.0, 250.0),
    (250.0, 1000.0),
    (1000.0, 4000.0),
    (4000.0, 8000.0),
    (8000.0, 12_000.0),
    (12_000.0, 16_000.0),
    (16_000.0, 24_000.0),
];

/// Energy per [`BANDS`] entry in dB relative to the total (averaged Hann-windowed 4096-pt spectra of
/// the mid channel).
pub fn band_energies_db(stereo: &[f32], sr: u32) -> [f32; 8] {
    const N: usize = 4096;
    let mono: Vec<f64> = stereo.as_chunks::<2>().0.iter().map(|c| f64::from(c[0] + c[1]) * 0.5).collect();
    let mut power = vec![0.0f64; N / 2];
    let win: Vec<f64> = (0..N).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / N as f64).cos()).collect();
    let mut start = 0;
    let mut re = vec![0.0; N];
    let mut im = vec![0.0; N];
    while start + N <= mono.len() {
        for i in 0..N {
            re[i] = mono[start + i] * win[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for k in 0..N / 2 {
            power[k] += re[k] * re[k] + im[k] * im[k];
        }
        start += N / 2;
    }
    let total: f64 = power.iter().sum::<f64>().max(1e-30);
    let hz_per_bin = f64::from(sr) / N as f64;
    let mut out = [0.0f32; 8];
    for (b, (lo, hi)) in BANDS.iter().enumerate() {
        let e: f64 = power
            .iter()
            .enumerate()
            .filter(|(k, _)| {
                let f = *k as f64 * hz_per_bin;
                f >= f64::from(*lo) && f < f64::from(*hi)
            })
            .map(|(_, p)| p)
            .sum();
        out[b] = (10.0 * (e / total).max(1e-15).log10()) as f32;
    }
    out
}

/// Write 16-bit PCM stereo WAV (TPDF-dithered).
pub fn write_wav16(path: &Path, sr: u32, stereo: &[f32]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let data_len = (stereo.len() * 2) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&(sr * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    let mut rng = 0x1234_5678u32;
    let mut rnd = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng as f32 / u32::MAX as f32
    };
    for &x in stereo {
        let d = rnd() - rnd();
        let v = (x.clamp(-1.0, 1.0) * 32767.0 + d).round().clamp(-32768.0, 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lufs_of_reference_sine() {
        // A 997 Hz sine at -20 dBFS in both channels reads about -20 LUFS (+3 for two channels,
        // ≈ -3 for the RMS of a sine): the standard sanity value is -20.0 ± 0.3.
        let sr = 48_000;
        let amp = 10f32.powf(-20.0 / 20.0);
        let s: Vec<f32> = (0..sr * 5)
            .flat_map(|i| {
                let x = amp * (2.0 * std::f32::consts::PI * 997.0 * i as f32 / sr as f32).sin();
                [x, x]
            })
            .collect();
        let l = integrated_lufs(&s, sr);
        assert!((l + 20.0).abs() < 0.3, "{l}");
        let tp = true_peak_db(&s);
        assert!((tp + 20.0).abs() < 0.2, "{tp}");
    }
}
