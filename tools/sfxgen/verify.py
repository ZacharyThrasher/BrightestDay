#!/usr/bin/env python3
"""Objective checks for assets/sfx (we can't listen, so we measure).

    python tools/sfxgen/verify.py                 # table + checks + target/audio/sfx_spectrograms.png
    python tools/sfxgen/verify.py --png out.png   # montage elsewhere
    python tools/sfxgen/verify.py --quiet         # summaries only

Per file: duration, channels, peak dBFS, max momentary loudness (LUFS, BS.1770 K-weighted; loops also
integrated), DC, band shares (< 90 Hz, < 150 Hz, 800 Hz-3 kHz, > 2 kHz, 5-12 kHz), "noisiness" and
full-band spectral flatness (see dsp.noisiness / dsp.spectral_flatness), edge levels.
Loops: seam discontinuity vs. typical sample step, and HF energy burst at the wrap point (per channel).
Carve rules (frequency roles) are asserted on every variation. The median noisiness per category is
compared against the pre-redesign backup in target/audio/sfx_before (if present).
Exit code 1 if any hard check fails.
"""
from __future__ import annotations

import argparse
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
BEFORE = ROOT / 'target' / 'audio' / 'sfx_before'
PNG = ROOT / 'target' / 'audio' / 'sfx_spectrograms.png'
MAX_MB = 25.0


def parse_manifest(path):
    txt = path.read_text(encoding='utf-8')
    out = {}
    for m in re.finditer(r'"(\w+)": \(\s*files: \[([^\]]*)\],\s*category: "(\w+)",.*?looping: (\w+),'
                         r'.*?lufs: ([-\d.]+),\s*sequence: (\w+),(\s*stereo: (\w+),)?', txt, re.S):
        files = re.findall(r'"([^"]+)"', m.group(2))
        out[m.group(1)] = dict(files=files, category=m.group(3), loop=m.group(4) == 'true', lufs=float(m.group(5)),
                               sequence=m.group(6) == 'true', stereo=m.group(8) == 'true')
    return out


def band_share(x, lo, hi):
    X = np.abs(np.fft.rfft(dsp.monosum(x))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / dsp.SR)
    return X[(f >= lo) & (f < hi)].sum() / (X.sum() + 1e-20)


def centroid(x):
    X = np.abs(np.fft.rfft(dsp.monosum(x)))
    f = np.fft.rfftfreq(len(x), 1 / dsp.SR)
    return (f * X).sum() / (X.sum() + 1e-20)


seam_metrics = dsp.seam_metrics


FIRE_SOUNDS = ['gun_handcannon', 'gun_shotgun', 'gun_minigun_loop', 'gun_rocket_fire', 'gun_railgun_fire', 'ring_blast',
               'ring_blast_hit']
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


def pitch_report(man, quiet):
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
            cells.append(f'{nm:3s}{cents:+4.0f}c')
            if nm != expect[step - 1] or abs(cents) > 25:
                bad.append(f'{rel}: detected {nm} {cents:+.0f}c, expected {expect[step - 1]}')
        print(f'  {name[10:]:11s} ' + ' | '.join(cells))
    return bad


BAND_CHECKS = {  # name: [(lo, hi, min_share)] - design intents checked on every variation
    'hit_crit': [(3000, 8000, 0.25), (60, 120, 0.04)],       # HF snap over a SMALL thud
    'ko_shatter': [(0, 150, 0.4), (3000, 12000, 0.1)],
    'construct_shatter': [(0, 150, 0.3), (3000, 12000, 0.1)],
    'transform_combine': [(0, 150, 0.4)],
    'robot_footstep_heavy': [(0, 150, 0.5)],
}
# pitch-trend intents: slope (octaves/s) of the energy-weighted 150-12000 Hz centroid over the cascade
TREND_CHECKS = {'transform_to_robot': ('up', 0.03, 0.66), 'transform_to_vehicle': ('down', 0.03, 0.66)}

# frequency-role carve rules
FEEDBACK_CATS = ('ui', 'feedback', 'ko_streak')        # >= 35 % above 2 kHz, < 10 % below 200 Hz
LOW_EXEMPT = {'hit_crit', 'heartbeat'}
HIGH_EXEMPT = {'heartbeat'}
FOLEY = {'footstep', 'jump', 'land_light', 'slide_loop', 'boost_burst', 'boost_loop', 'tether_fire', 'tether_attach',
         'tether_release', 'tether_reel_loop', 'shell_casing', 'gun_shotgun_reload', 'debris_patter',
         'transform_panel_click', 'break_leaves', 'hatch_open', 'hatch_close', 'exhaust_puff', 'air_jump', 'dodge',
         'lunge', 'hook_fire', 'hook_attach', 'hook_reel_loop', 'hook_break', 'ads_in', 'ads_out'}  # < 10 % below 150 Hz
HEAVY = {'explosion_small', 'explosion_big', 'kaiju_stomp', 'hit_heavy', 'wall_splat', 'collapse_rumble',
         'ko_shatter', 'land_heavy', 'kaiju_step', 'hook_slam', 'domain_open', 'ult_fire'}  # >= 30 % below 150 Hz


def analyse(x, loop):
    return dict(
        peak=20 * np.log10(np.max(np.abs(x)) + 1e-12),
        mom=dsp.lufs_momentary_max(x),
        integ=dsp.lufs_integrated(x, loop=True) if loop else None,
        b90=band_share(x, 0, 90), b150=band_share(x, 0, 150), b200=band_share(x, 0, 200),
        mid=band_share(x, 800, 3000), hi2k=band_share(x, 2000, 24000), air=band_share(x, 5000, 12000),
        noisy=dsp.noisiness(x), sfm=dsp.spectral_flatness(x), cen=centroid(x),
    )


def flatness_compare(man):
    """Median noisiness / full-band flatness per category, before (backup) vs after."""
    if not BEFORE.exists():
        print(f'\n(no {BEFORE} - skipping before/after flatness comparison)')
        return None
    rows = {}
    for name, e in man.items():
        for rel in e['files']:
            fn = Path(rel).name
            a, _ = sf.read(SFX / fn)
            d = rows.setdefault(e['category'], dict(b_n=[], b_s=[], a_n=[], a_s=[], new_n=[]))
            bp_ = BEFORE / fn
            if bp_.exists():
                b, _ = sf.read(bp_)
                d['b_n'].append(dsp.noisiness(b))
                d['b_s'].append(dsp.spectral_flatness(b))
                d['a_n'].append(dsp.noisiness(a))
                d['a_s'].append(dsp.spectral_flatness(a))
            else:
                d['new_n'].append(dsp.noisiness(a))
    print('\nNoisiness before -> after (median per category over files that exist in both; '
          'noisiness = energy-weighted octave-band spectral flatness, 0 tonal .. ~0.56 noise;'
          ' SFM = full-band flatness):')
    print(f'  {"category":12s} {"files":>5s} {"noisy before":>12s} {"after":>7s} {"change":>7s}   '
          f'{"SFM before":>10s} {"after":>7s}   {"new-key files (noisy)":>22s}')
    allb, alla = [], []
    for cat, d in sorted(rows.items()):
        nb = len(d['b_n'])
        newc = f'{len(d["new_n"])} ({np.median(d["new_n"]):.3f})' if d['new_n'] else '-'
        if nb:
            mb, ma = np.median(d['b_n']), np.median(d['a_n'])
            allb += d['b_n']
            alla += d['a_n']
            print(f'  {cat:12s} {nb:5d} {mb:12.3f} {ma:7.3f} {100 * (ma - mb) / (mb + 1e-9):+6.0f}%   '
                  f'{np.median(d["b_s"]):10.4f} {np.median(d["a_s"]):7.4f}   {newc:>22s}')
        else:
            print(f'  {cat:12s} {0:5d} {"-":>12s} {"-":>7s} {"":>7s}   {"-":>10s} {"-":>7s}   {newc:>22s}')
    if allb:
        print(f'  {"ALL":12s} {len(allb):5d} {np.median(allb):12.3f} {np.median(alla):7.3f} '
              f'{100 * (np.median(alla) - np.median(allb)) / np.median(allb):+6.0f}%   '
              f'(mean {np.mean(allb):.3f} -> {np.mean(alla):.3f})')
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--png', type=Path, default=PNG)
    ap.add_argument('--quiet', action='store_true')
    args = ap.parse_args()
    man = parse_manifest(SFX / 'manifest.ron')
    fails = []
    rows = []
    carve = []
    if not args.quiet:
        print(f'{"file":27s} {"dur":>5s} {"ch":>2s} {"peak":>5s} {"momLU":>6s} {"intLU":>6s} {"<90":>4s} {"<150":>4s} '
              f'{"8-3k":>4s} {">2k":>4s} {"5-12k":>5s} {"noisy":>5s} {"sfm":>6s}  notes')
    listed = set()
    for name, e in man.items():
        for i, rel in enumerate(e['files']):
            listed.add(Path(rel).name)
            p = ROOT / 'assets' / rel
            if not p.exists():
                fails.append(f'missing {rel}')
                continue
            x, sr = sf.read(p)
            ch = 1 if x.ndim == 1 else x.shape[1]
            if sr != dsp.SR:
                fails.append(f'{rel}: sr={sr}')
            if (ch == 2) != e['stereo']:
                fails.append(f'{rel}: {ch} channel(s) but manifest stereo={e["stereo"]}')
            a = analyse(x, e['loop'])
            notes = ''
            for c in dsp.chans(x):
                if e['loop']:
                    jump, hf = seam_metrics(c)
                    notes += f'seam j={jump:.2f} hf={hf:.2f} '
                    if jump > 1.0 or hf > 3.0:
                        fails.append(f'{rel}: loop seam jump={jump:.2f} hf={hf:.2f}')
                else:
                    if abs(c[0]) > 0.02 or abs(c[-1]) > 1e-3:
                        fails.append(f'{rel}: edge not faded (first={c[0]:.4f}, last={c[-1]:.4f})')
                if abs(np.mean(c)) > 1e-3:
                    fails.append(f'{rel}: DC offset {np.mean(c):.5f}')
            if a['peak'] > -0.9:
                fails.append(f'{rel}: peak {a["peak"]:.2f} dBFS > -1')
            meas = a['integ'] if e['loop'] else a['mom']
            tgt = e['lufs'] + (0.5 * i if e['sequence'] else 0.0)
            if abs(meas - tgt) > 2.5:
                fails.append(f'{rel}: loudness {meas:.1f} LUFS vs target {tgt:.1f}')
            elif meas < tgt - 0.6:
                notes += f'(short {tgt - meas:.1f} dB) '
            for lo_, hi_, mn in BAND_CHECKS.get(name, []):
                sh = band_share(x, lo_, hi_)
                notes += f'{lo_}-{hi_}={sh * 100:.0f}% '
                if sh < mn:
                    fails.append(f'{rel}: {lo_}-{hi_} Hz share {sh * 100:.0f}% < {mn * 100:.0f}%')
            if name in TREND_CHECKS:
                want, *w = TREND_CHECKS[name]
                tr = dsp.pitch_trend(dsp.monosum(x), *w)
                notes += f'trend {tr:+.2f} oct/s '
                if (want == 'up' and tr <= 0.3) or (want == 'down' and tr >= -0.3):
                    fails.append(f'{rel}: pitch trend {tr:.2f} not {want}')
            sub = band_share(x, 40, 90)
            if name in FIRE_SOUNDS and (sub < 0.2 or a['b150'] < 0.4):
                fails.append(f'{rel}: weak low end (40-90 Hz {sub * 100:.0f}%, <150 Hz {a["b150"] * 100:.0f}%)')
            # carve rules
            if e['category'] in FEEDBACK_CATS:
                if name not in HIGH_EXEMPT and a['hi2k'] < 0.35:
                    carve.append(f'{rel}: feedback/UI >2 kHz share {a["hi2k"] * 100:.0f}% < 35%')
                if name not in LOW_EXEMPT and a['b200'] >= 0.10:
                    carve.append(f'{rel}: feedback/UI <200 Hz share {a["b200"] * 100:.0f}% >= 10%')
            if name in FOLEY and a['b150'] >= 0.10:
                carve.append(f'{rel}: foley <150 Hz share {a["b150"] * 100:.0f}% >= 10%')
            if name in HEAVY and a['b150'] < 0.30:
                carve.append(f'{rel}: heavy <150 Hz share {a["b150"] * 100:.0f}% < 30%')
            if not args.quiet:
                il = f'{a["integ"]:6.1f}' if a['integ'] is not None else '     -'
                print(f'{Path(rel).name:27s} {len(x) / sr:5.2f} {ch:2d} {a["peak"]:5.1f} {a["mom"]:6.1f} {il} '
                      f'{a["b90"] * 100:4.0f} {a["b150"] * 100:4.0f} {a["mid"] * 100:4.0f} {a["hi2k"] * 100:4.0f} '
                      f'{a["air"] * 100:5.0f} {a["noisy"]:5.3f} {a["sfm"]:6.4f}  {notes}')
            if i == 0:
                rows.append((name, e['category'], dsp.monosum(x), a))
    stale = [p.name for p in SFX.glob('*.*') if p.suffix in ('.ogg', '.wav') and p.name not in listed]
    if stale:
        fails.append(f'stale files not in manifest: {stale[:8]}{"..." if len(stale) > 8 else ""}')
    total = sum(p.stat().st_size for p in SFX.glob('*.*') if p.suffix in ('.ogg', '.wav'))
    nfiles = sum(len(e['files']) for e in man.values())
    nst = sum(1 for e in man.values() if e['stereo'])
    print(f'\n{len(man)} sounds ({nst} stereo), {nfiles} files, {total / 1e6:.2f} MB')
    if total / 1e6 > MAX_MB:
        fails.append(f'asset size {total / 1e6:.1f} MB > {MAX_MB} MB')
    fails += pitch_report(man, args.quiet)
    print('\nCarve rules: feedback/UI (>=35% >2 kHz, <10% <200 Hz), foley (<10% <150 Hz), heavy (>=30% <150 Hz):')
    print('  all pass' if not carve else '\n'.join('  FAIL ' + c for c in carve))
    fails += carve
    flatness_compare(man)
    montage(rows, args.png)
    if fails:
        print('\nFAILED CHECKS:')
        for f in fails:
            print('  ' + f)
        sys.exit(1)
    print('all checks passed')


def montage(rows, png):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from scipy import signal

    cols = 8
    nrows = (len(rows) + cols - 1) // cols
    fig, axes = plt.subplots(nrows, cols, figsize=(cols * 3.4, nrows * 2.4), facecolor='#111')
    for ax in axes.flat:
        ax.set_facecolor('#111')
        ax.axis('off')
    for ax, (name, cat, x, a) in zip(axes.flat, rows):
        ax.axis('on')
        x = x[:dsp.ns(6.0)]
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
        ax.axhline(2000, color='#fa3', lw=0.5, alpha=0.5)
        ax.tick_params(axis='x', labelsize=6, colors='#aaa')
        ax.set_title(f'{name} [{cat}]\n<150 {a["b150"] * 100:.0f}%  >2k {a["hi2k"] * 100:.0f}%  noisy {a["noisy"]:.2f}',
                     fontsize=7, color='#eee')
    fig.suptitle('Brightest Day SFX - first variation of each sound (log freq; blue = 150 Hz, orange = 2 kHz)',
                 color='#fff', fontsize=12)
    fig.tight_layout(rect=(0, 0, 1, 0.99))
    png.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(png, dpi=72, facecolor=fig.get_facecolor())
    print(f'wrote {png}')


if __name__ == '__main__':
    main()
