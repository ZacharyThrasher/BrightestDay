# sfxgen — procedural sound effects for Brightest Day

Every sound in `assets/sfx/` is synthesised from scratch by this tool: oscillators (sine, PolyBLEP saw/square,
2-op FM), white/pink/brown noise, static and time-varying filters, modal resonator banks (struck metal, glass,
wood and stone), pitch-dropping sine "thumps", tanh saturation, and convolution with synthetic, frequency-dependent
reverb impulse responses. No samples are downloaded or used.

## Regenerate

```sh
pip install -r tools/sfxgen/requirements.txt       # numpy, scipy, soundfile (+ matplotlib for verify)
python tools/sfxgen/sfxgen.py                       # all sounds -> assets/sfx/*.ogg + assets/sfx/manifest.ron
python tools/sfxgen/sfxgen.py --only hit_heavy clash   # iterate on a few sounds
python tools/sfxgen/sfxgen.py --list                # ids, categories, variation counts
python tools/sfxgen/sfxgen.py --format wav          # 16-bit PCM WAV instead (needs Bevy's "wav" feature)
python tools/sfxgen/verify.py                       # objective checks + docs/sfx_spectrograms.png
```

A full run takes about 20 s. Output is deterministic: variation `v` of sound `name` is seeded with
`crc32("brightest-day/<name>/<v>")`, so a regeneration only changes files whose recipe changed. A full run
deletes old `*.ogg`/`*.wav` in `assets/sfx/` first, so renamed sounds don't linger.

## Files

| file | purpose |
|---|---|
| `dsp.py` | the DSP toolkit: oscillators, noise, filters, modal synthesis, reverb, loop helpers, loudness, limiter, trimming |
| `sounds.py` | one `@sound(...)` function per sound id (movement, tethers, constructs, hits, destruction, UI), plus category loudness targets |
| `sounds_weapons.py` | guns (hand cannon, super shotgun, minigun spin-up/loop/spin-down, rocket, railgun), casings, hit confirms/crit, bullet impacts, ring blast, ko_shatter — every fire sound is transient + 40-90 Hz sub thump + mechanism + slapback/room tail |
| `sounds_streak.py` | KO streak sets: 8 corps × 5 steps (E minor pentatonic E5 G5 A5 B5, D6→E6), one timbre per corps, plus accents |
| `sfxgen.py` | the CLI. Runs the recipes, masters each file and writes the OGG files and `manifest.ron` |
| `verify.py` | decodes every file and reports peak, RMS, LUFS, DC, band shares and loop-seam metrics; renders the spectrogram montage |

## Format and mastering

- 48 kHz mono (for 3D spatialisation), mastered as 16-bit PCM and then encoded as Ogg Vorbis at about q7.
  Bevy decodes Vorbis with its default features.
- The chain for a one-shot is: 18 Hz zero-phase DC-block, then a trim at -54 dB below the peak (plus 3 ms of
  leading silence so the codec's pre-echo never starts on a step, a 1 ms fade-in and a 20 ms fade-out), then net-DC
  removal, then loudness normalisation, then a look-ahead limiter (at most 6 dB of gain reduction; beyond that,
  gain is lowered instead).
- Peaks are at or below -1 dBFS after decoding. The writer decodes each OGG and re-encodes it slightly quieter if
  Vorbis overshoots.
- Every layer gets a cosine tail fade, so nothing ends on a step. `sfxgen.py` prints a warning when a layer is
  still ringing at its end.

### Loudness targets (BS.1770 K-weighted, "LUFS-ish")

One-shots are normalised on their **maximum momentary (400 ms) loudness**. Loops are normalised on
**integrated loudness**, measured circularly.

| category | target | notes / overrides |
|---|---|---|
| combat (hits, clash, shatter, explosions, kaiju, wall_splat) | -12 | hit_light -14, hit_medium -13, kaiju_stomp / explosion_big -11 |
| construct (ring, constructs) | -15 | shield_block -13 |
| destruction | -15 | break_leaves -17, debris_patter -19, collapse_rumble -13 |
| tether | -17 | |
| movement | -18 | land_light -19, land_heavy -15, boost_burst -16, footstep -22 |
| ui / status | -21 | ui_move -24, type_key -26, ko -12, match_start/end -13, oath_complete / respawn -16 |
| weapon | -12 | shotgun / rocket / railgun fire -11, reloads & spin -16/-17, shell_casing -24 |
| ko_streak | -14 | streak steps -15 rising 0.5 dB per step to -13 |
| loops | -21 | wind -22, boost -20, beam -18, minigun -14, rocket flyby -18 |

Very transient sounds (hits, footsteps, breaks) sometimes can't reach their target without more than 6 dB of
limiting. They land up to about 1.7 dB short, and `sfxgen.py` marks them `(peak-capped)`. The loudness actually
reached is in the `verify.py` table. The mix lives in `manifest.ron` (`volume`): the files are consistent, and the
game decides the balance.

## Loops

`slide_loop`, `wind_loop`, `boost_loop`, `tether_reel_loop` and `beam_loop` are sample-exact seamless loops:

- **Tonal layers are strictly periodic.** Every oscillator and LFO frequency × loop length is an integer, and the
  layers are rendered through their filters for one warm-up period that is then discarded (`dsp.periodic`).
- **Noise layers** are rendered one crossfade longer than the loop, and the overhang is equal-power crossfaded into
  the start (`dsp.loop_xfade`). `y[0]` continues `y[-1]`.
- DC-blocking of loops is circular.
- `verify.py` checks `|y[0]-y[-1]|` against the 99th-percentile sample step, and the >6 kHz energy at the wrap point
  against the median. The loops currently score jump ≤ 0.6 (a value below 1 means the wrap is smaller than a normal
  sample step) and HF ≤ 2.3× the median (no click).

The minigun loop is exactly 14 rounds in 1.0 s (14 rounds/s). Its room reverb and slapback are applied
circularly (`dsp.reverb_circ`, `dsp.circ`), so the tail of the last round rings into the first.

## Sequences (KO streak)

`sequence=True` sounds (`ko_streak_<corps>`) have ordered files, where `_N` is the streak step and not a random
variation. The manifest marks them `sequence: true` and adds a `ko_streak` table (key, steps, corps → id, accents).
`verify.py` detects the melody note of every step (the strongest partial in the note body) and fails if a step is
off by more than 25 cents.

## Adding or changing a sound

```python
@sound('my_sound', 'combat', variations=3, volume=0.9, pitch=(0.95, 1.05), lufs=None, notes='...')
def my_sound(r, v):              # r: seeded numpy Generator, v: variation index
    out = buf(0.8)               # padded scratch buffer; trailing silence is trimmed later
    add(out, crack(r, 2000, 0.004), 0, 1.0)                      # transient
    add(out, thump(0.5, 140, 45, 0.04, 0.15, drive=2.5), 0, 1.0)  # low-end weight
    return reverb(out, r, rt60=0.8, wet=0.2)                      # tail
```

The layering recipe for anything that should feel heavy is **transient** (crack, click), then **body**
(filtered noise, modal ring, FM), then **weight** (a pitch-dropping sine thump with saturation, so it still reads
on small speakers), then **tail** (reverb, debris cascade or rumble). Then run `verify.py` and look at
`docs/sfx_spectrograms.png`.
