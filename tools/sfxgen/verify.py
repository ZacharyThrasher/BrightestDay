#!/usr/bin/env python3
"""Objective checks for assets/sfx (we can't listen, so we measure).

    python tools/sfxgen/verify.py            # table + checks + docs/sfx_spectrograms.png

Per file: duration, peak dBFS, RMS dBFS, loudness (LUFS, BS.1770 K-weighted), DC offset,
low-end share (energy < 150 Hz), high share (> 4 kHz), spectral centroid, edge levels.
Loops: seam discontinuity vs. typical sample step, and HF energy burst at the wrap point.
Exit code 1 if any hard check fails.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

import numpy as np
import soundfile as sf

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import dsp  # noqa: E402

ROOT = HERE.parent.parent
SFX = ROOT / 'assets' / 'sfx'
PNG = ROOT / 'docs' / 'sfx_spectrograms.png'


def parse_manifest():
    txt = (SFX / 'manifest.ron').read_text(encoding='utf-8')
    out = {}
    for m in re.finditer(r'"(\w+)": \(\s*files: \[([^\]]*)\],\s*category: "(\w+)",.*?looping: (\w+),'
                         r'.*?sequence: (\w+)', txt, re.S):
        files = re.findall(r'"([^"]+)"', m.group(2))
        out[m.group(1)] = dict(files=files, category=m.group(3), loop=m.group(4) == 'true',
                               sequence=m.group(5) == 'true')
    return out


def band_share(x, lo, hi):
    X = np.abs(np.fft.rfft(x)) ** 2
    f = np.fft.rfftfreq(len(x), 1 / dsp.SR)
    return X[(f >= lo) & (f < hi)].sum() / (X.sum() + 1e-20)


def centroid(x):
    X = np.abs(np.fft.rfft(x))
    f = np.fft.rfftfreq(len(x), 1 / dsp.SR)
    return (f * X).sum() / (X.sum() + 1e-20)


def seam_metrics(x):
    """Return (jump_ratio, hf_ratio). jump: |x[0]-x[-1]| / p99(|diff|).
    hf: HF (>6 kHz) energy in 4 ms around the wrap vs. median 4 ms window energy of the loop."""
    d = np.abs(np.diff(x))
    jump = abs(x[0] - x[-1]) / (np.percentile(d, 99) + 1e-12)
    h = dsp.hp(np.tile(x, 3), 6000, 4)[len(x):2 * len(x)]  # circular-ish: settled filter
    h = np.roll(h, len(x) // 2)  # wrap point now in the middle
    w = dsp.ns(0.004)
    c = len(x) // 2
    e_seam = np.mean(h[c - w // 2:c + w // 2] ** 2)
    frames = h[:len(h) // w * w].reshape(-1, w)
    e_med = np.median(np.mean(frames ** 2, axis=1))
    return jump, e_seam / (e_med + 1e-20)


FIRE_SOUNDS = ['gun_handcannon', 'gun_shotgun', 'gun_minigun_loop', 'gun_rocket_fire', 'gun_railgun_fire',
               'ring_blast', 'ring_blast_hit']
NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']


def note_name(f):
    m = 69 + 12 * np.log2(f / 440.0)
    k = int(round(m))
    return f'{NOTE_NAMES[k % 12]}{k // 12 - 1}', (m - k) * 100


def melody_pitch(x, step):
    """Strongest spectral peak (450-1500 Hz) in the note's body, parabolic-interpolated."""
    on = np.argmax(np.abs(x) > 0.05 * np.max(np.abs(x)))
    t0 = on + dsp.ns(0.17 if step == 5 else 0.06)
    seg = x[t0:t0 + dsp.ns(0.2)] * np.hanning(dsp.ns(0.2))
    N = 1 << 17
    X = np.abs(np.fft.rfft(seg, N))
    f = np.fft.rfftfreq(N, 1 / dsp.SR)
    m = (f > 450) & (f < 1500)
    i = np.argmax(np.where(m, X, 0))
    a, b, c = np.log(X[i - 1:i + 2] + 1e-12)
    p = 0.5 * (a - c) / (a - 2 * b + c)
    return (i + p) * dsp.SR / N


def pitch_report(man):
    expect = ['E5', 'G5', 'A5', 'B5', 'E6']
    bad = []
    print('\nKO streak melody (detected strongest partial per step, note / cents):')
    for name, e in man.items():
        if not name.startswith('ko_streak_') or not e.get('sequence'):
            continue
        cells = []
        for step, rel in enumerate(e['files'], 1):
            x, _ = sf.read(ROOT / 'assets' / rel)
            f = melody_pitch(x, step)
            nm, cents = note_name(f)
            cells.append(f'{f:7.1f}Hz {nm:3s}{cents:+4.0f}c')
            if nm != expect[step - 1] or abs(cents) > 25:
                bad.append(f'{rel}: detected {nm} {cents:+.0f}c, expected {expect[step - 1]}')
        print(f'  {name[10:]:11s} ' + ' | '.join(cells))
    return bad


BAND_CHECKS = {  # name: [(lo, hi, min_share)] — design intents checked on every variation
    'hit_crit': [(3000, 8000, 0.25), (60, 120, 0.25)],
    'ko_shatter': [(0, 150, 0.4), (3000, 12000, 0.1)],
    'construct_shatter': [(0, 150, 0.3), (3000, 12000, 0.1)],
}


def main():
    man = parse_manifest()
    fails = []
    rows = []
    print(f'{"file":26s} {"dur":>5s} {"peak":>6s} {"rms":>6s} {"LUFS":>6s} {"dc":>8s} {"<150Hz":>6s} {"40-90":>6s} {">4k":>5s} {"cent":>6s}  loop-seam')
    for name, e in man.items():
        for i, rel in enumerate(e['files']):
            p = ROOT / 'assets' / rel
            if not p.exists():
                fails.append(f'missing {rel}')
                continue
            x, sr = sf.read(p)
            if sr != dsp.SR or x.ndim != 1:
                fails.append(f'{rel}: sr={sr} ndim={x.ndim}')
            peak = 20 * np.log10(np.max(np.abs(x)) + 1e-12)
            rms = 10 * np.log10(np.mean(x ** 2) + 1e-20)
            lufs = dsp.lufs_integrated(x, loop=True) if e['loop'] else dsp.lufs_momentary_max(x)
            dc = np.mean(x)
            lo = band_share(x, 0, 150)
            hi = band_share(x, 4000, 24000)
            cen = centroid(x)
            seam = ''
            if e['loop']:
                jump, hf = seam_metrics(x)
                seam = f'jump={jump:.2f} hf={hf:.2f}'
                if jump > 1.0 or hf > 3.0:
                    fails.append(f'{rel}: loop seam jump={jump:.2f} hf={hf:.2f}')
            else:
                if abs(x[0]) > 0.02 or abs(x[-1]) > 1e-3:
                    fails.append(f'{rel}: edge not faded (first={x[0]:.4f}, last={x[-1]:.4f})')
            if peak > -0.9:
                fails.append(f'{rel}: peak {peak:.2f} dBFS > -1')
            if abs(dc) > 1e-3:
                fails.append(f'{rel}: DC offset {dc:.5f}')
            for lo_, hi_, mn in BAND_CHECKS.get(name, []):
                sh = band_share(x, lo_, hi_)
                seam += f' {lo_}-{hi_}Hz={sh * 100:.0f}%'
                if sh < mn:
                    fails.append(f'{rel}: {lo_}-{hi_} Hz share {sh * 100:.0f}% < {mn * 100:.0f}%')
            sub = band_share(x, 40, 90)
            if name in FIRE_SOUNDS and (sub < 0.2 or lo < 0.4):
                fails.append(f'{rel}: weak low end (40-90 Hz {sub * 100:.0f}%, <150 Hz {lo * 100:.0f}%)')
            print(f'{Path(rel).name:26s} {len(x) / sr:5.2f} {peak:6.1f} {rms:6.1f} {lufs:6.1f} {dc:8.1e} '
                  f'{lo * 100:5.1f}% {sub * 100:5.1f}% {hi * 100:4.1f}% {cen:6.0f}  {seam}')
            if i == 0:
                rows.append((name, e['category'], x, lo, hi, cen, lufs))
    total = sum(p.stat().st_size for p in SFX.glob('*.*') if p.suffix in ('.ogg', '.wav'))
    nfiles = sum(len(e['files']) for e in man.values())
    print(f'\n{len(man)} sounds, {nfiles} files, {total / 1e6:.2f} MB')
    fails += pitch_report(man)
    montage(rows)
    if fails:
        print('\nFAILED CHECKS:')
        for f in fails:
            print('  ' + f)
        sys.exit(1)
    print('all checks passed')


def montage(rows):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from scipy import signal

    cols = 6
    nrows = (len(rows) + cols - 1) // cols
    fig, axes = plt.subplots(nrows, cols, figsize=(cols * 3.6, nrows * 2.5), facecolor='#111')
    for ax in axes.flat:
        ax.set_facecolor('#111')
        ax.axis('off')
    for ax, (name, cat, x, lo, hi, cen, lufs) in zip(axes.flat, rows):
        ax.axis('on')
        xx = np.concatenate([x, np.zeros(4096)])
        f, t, S = signal.spectrogram(xx, dsp.SR, nperseg=2048, noverlap=2048 - 256, window='hann')
        S = 10 * np.log10(S + 1e-14)
        S -= S.max()
        m = f >= 20
        ax.pcolormesh(t, f[m], S[m], vmin=-80, vmax=0, cmap='magma', shading='auto')
        ax.set_yscale('log')
        ax.set_ylim(20, 24000)
        ax.set_yticks([50, 150, 500, 2000, 8000])
        ax.set_yticklabels(['50', '150', '500', '2k', '8k'], fontsize=6, color='#aaa')
        ax.axhline(150, color='#3af', lw=0.5, alpha=0.6)
        ax.tick_params(axis='x', labelsize=6, colors='#aaa')
        ax.set_title(f'{name}  [{cat}]\n<150Hz {lo * 100:.0f}%  >4k {hi * 100:.0f}%  cent {cen:.0f}Hz',
                     fontsize=7.5, color='#eee')
    fig.suptitle('Brightest Day SFX — first variation of each sound (log freq; blue line = 150 Hz)',
                 color='#fff', fontsize=12)
    fig.tight_layout(rect=(0, 0, 1, 0.985))
    PNG.parent.mkdir(exist_ok=True)
    fig.savefig(PNG, dpi=80, facecolor=fig.get_facecolor())
    print(f'wrote {PNG}')


if __name__ == '__main__':
    main()
