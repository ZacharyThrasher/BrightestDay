# sfxgen — procedural sound effects for Brightest Day

Every sound in `assets/sfx/` is synthesised from scratch by this tool: modal resonator banks (material
families), pitch-dropping sine thumps, 2-op FM, tuned resonators excited by short impulses, granular clusters of
tiny modal ticks, PolyBLEP oscillators, shaped pink/brown noise where the source really is noise (gas, wind,
surf, tyres, thunder), tanh saturation and role-based EQ carving. No samples are downloaded or used.

## Design rules (why it sounds like a world, not a wall of noise)

1. **Dry.** No baked room reverb or long noise tails on anything. The game's mixer supplies one shared space
   (reverb bus + early reflections), so every sound sits in the same room. Natural physical decay stays: a metal
   plate ringing, a rumble, a debris cascade are the source, not the room.
2. **No broadband white noise.** Noise only as a <= 15 ms contact transient (`materials.click`), as a
   band-limited, clearly subordinate texture, or where the physical source *is* noise — and then pink/brown,
   resonant, amplitude-grained. Everything else is partials: modal synthesis, FM, tuned resonators, thumps.
3. **Frequency carving by role** (`dsp.carve_*`):
   - heavy (explosions, heavy hits, kaiju, collapse, wall splat): own the sub/low (30-90 Hz saturated punch) +
     a transient; `carve_heavy` dips 2-5 kHz by ~6 dB and trims > 6.5 kHz so confirms and footsteps read on top;
   - guns: snap (3-8 kHz, < 10 ms) + mechanism (modal clanks, 0.5-3 kHz) + 40-90 Hz punch, no tail;
   - foley (footsteps, jump/land, tethers, reel, casings, reload, debris patter): `carve_foley` — 24 dB/oct
     high-pass at 150-250 Hz, energy in 0.8-3 kHz;
   - UI + feedback (confirms, crit, KO, streak, UI): `carve_ui` — ~36 dB/oct high-pass at 1-2 kHz, energy
     5-10 kHz, short. `dsp.balance_hi` mixes a bright in-key layer in until >= 42 % of the energy is above
     2 kHz (the KO-streak notes keep their fundamental for the melody).
4. **Material modal families** (`materials.py`): `material_hit(r, material, size, energy)` and
   `shatter(r, material, count, spread, size, energy)` for stone, wood, metal, glass, crystal, dirt, leaves
   and hardlight. Every break, debris patter, bullet impact and construct/KO shatter comes from here.
   Bigger = lower and longer; harder = brighter (upper modes excited more).
5. **Hard-light timbre family**, tuned to **E minor** (pitch classes E G A B D, consonant with the KO-streak
   E-minor-pentatonic melody): harmonic partials 1, 2, 3, 4, 6 (never the 5th = G#), a detuned twin per partial
   for a slow shimmer, a harmonic-ratio FM glint on the attack (`hl_ping`, `hl_tone`, `hl_chord`,
   `hl_shards` = falling pentatonic arpeggio runs, `hl_drone` = strictly periodic drone). Every ring/construct
   sound uses it; the low thumps under hard light glide onto E (E3 -> E1).
6. **Variations** differ in modal sets, timing and structure (not just seed noise): footstep x8, type_key x8,
   hits x5-6, all materials x6 ...
7. **The loudness hierarchy is baked into the files** (table below); manifest `volume` is always 1.0 and
   `pitch` ranges are micro variation only (+-10 cents for tonal sounds, +-26..51 cents otherwise).

## Regenerate

```sh
pip install -r tools/sfxgen/requirements.txt       # numpy, scipy, soundfile (+ matplotlib for verify)
python tools/sfxgen/sfxgen.py                       # all sounds -> assets/sfx/*.ogg + assets/sfx/manifest.ron
python tools/sfxgen/sfxgen.py --only hit_heavy clash   # iterate on a few sounds
python tools/sfxgen/sfxgen.py --list                # ids, categories, variations, LUFS, loop/stereo
python tools/sfxgen/sfxgen.py --format wav          # 16-bit PCM WAV instead (needs Bevy's "wav" feature)
python tools/sfxgen/verify.py                       # objective checks + target/audio/sfx_spectrograms.png
```

A full run takes about 2 minutes. Output is deterministic: variation `v` of sound `name` is seeded with
`crc32("brightest-day/<name>/<v>")`. A full run deletes old `*.ogg`/`*.wav` in `assets/sfx/` first, so renamed
sounds don't linger.

## Files

| file | purpose |
|---|---|
| `dsp.py` | DSP toolkit: oscillators, noise, filters, modal synthesis, carving EQ (`carve_heavy/foley/ui`, `peq`, `hshelf`), `glue` soft clip, `balance_hi`, loop helpers, stereo-aware loudness/limiter/trim, analysis (`noisiness`, `spectral_flatness`, seam metrics) |
| `materials.py` | material modal families (`material_hit`, `shatter`, `grit`, `micro`, `click`) and the E-minor hard-light family (`hl_ping`, `hl_tone`, `hl_chord`, `hl_shards`, `note`, `pent`) |
| `sounds.py` | registry (`@sound`) + movement, tethers, hard-light constructs, hits, explosions, destruction, UI |
| `sounds_weapons.py` | guns (snap + mechanism + punch, dry), casings, hit confirms / crit (feedback), bullet impacts per material, ring blast, ko_shatter |
| `sounds_transform.py` | robot/vehicle transformation set (original design: staggered click/servo/clunk cascade with a pitch contour -> lock + hiss), panel clicks, combine, diesel engine loop, air horn, heavy robot footsteps. Transform takes are auto-selected until the verified pitch contour (>= 0.6 oct/s up or down) holds |
| `sounds_streak.py` | KO streak sets: 8 corps x 5 steps (E minor pentatonic E5 G5 A5 B5, D6->E6), one timbre per corps + a shared bright in-key "sheen", plus accents |
| `sounds_ambience.py` | stereo ambience beds, positional ambience loops/one-shots (mono), danger/mix cues (heartbeat, tinnitus, final_blow_suck, mix whooshes) |
| `sfxgen.py` | the CLI: runs the recipes, masters each file, writes OGG files and `manifest.ron` |
| `verify.py` | decodes every file: loudness, peaks, band shares, noisiness, loop seams, KO-streak melody, carve rules, before/after noisiness, spectrogram montage |

## Format and mastering

- 48 kHz; **mono** for everything spatialised, **stereo** (`stereo: true` in the manifest) only for the
  ambience beds (decorrelated L/R above ~150 Hz, shared mono lows; play non-spatialised).
  Mastered as 16-bit PCM, then Ogg Vorbis at about q7 (written in blocks: libsndfile's Vorbis encoder
  overflows the stack on long single writes).
- One-shot chain: 18 Hz zero-phase DC-block, trim at -54 dB below the peak (3 ms leading silence, 1 ms fade-in,
  20 ms fade-out — `final_blow_suck` uses 8 ms so it cuts dead), net-DC removal, loudness normalisation, then a
  look-ahead limiter (at most 7.5 dB of gain reduction; beyond that, gain is lowered instead). Short impacts use
  `dsp.glue` (normalised tanh) in the recipe to lower their crest factor rather than relying on the limiter.
- Peaks are at or below -1 dBFS after decoding (the writer re-encodes slightly quieter if Vorbis overshoots).

### Loudness hierarchy (BS.1770 K-weighted, "LUFS-ish")

One-shots are normalised on their **maximum momentary (400 ms) loudness**; loops on **integrated loudness**
(measured circularly). A few very transient heavies (hit_light/medium, wall_splat, kaiju_stomp, explosion_big,
shotgun) land up to ~1.5 dB short without over-limiting; `sfxgen.py` marks them `(peak-capped)` and `verify.py`
prints the reached level.

| group | sound: target LUFS |
|---|---|
| explosions / heavy | explosion_big -9, kaiju_stomp -10, gun_shotgun -11, gun_railgun_fire -11, explosion_small -12, gun_rocket_fire -12, hit_heavy -12, wall_splat -12, kaiju_roar -12, transform_combine -12, gun_handcannon -13, ko_shatter -13, collapse_rumble -14, transform_to_robot/vehicle -14, vehicle_horn -14 |
| combat / hard light | clash -14, hit_medium -15, construct_shatter -15, robot_footstep_heavy -15, shield_block -16, ring_blast_hit -17, hit_light -18, ring_blast -18, construct_form -19, gun_minigun_loop -16 (loop), beam_loop -22 (loop), construct_dismiss -23 |
| destruction | break_stone/wood/metal/glass/crystal/hardlight -18, break_dirt/leaves -22, bullet_impact_stone/dirt/wood/metal -20, bullet_impact_glass -21, debris_patter -28 |
| weapon mechanics | gun_railgun_charge -20, gun_minigun_spinup/spindown -20, rocket_flyby_loop -20 (loop), gun_shotgun_reload -24, shell_casing -32 |
| movement / foley | land_heavy -20, boost_burst -21, tether_attach -22, tether_fire -23, transform_panel_click -24, tether_release -25, jump -26, land_light -27, footstep -30; loops: boost -26, tether_reel -28, slide -29, wind -30, vehicle_engine -22 |
| feedback / UI (bright) | ko -15, ko_streak_end -15, ko_accent_* -16, match_start/end -16, ko_streak steps -17 rising 0.5 dB/step, hit_crit -18, hit_confirm_heavy -19, oath_complete -19, respawn -20, final_blow_suck -20, hit_confirm -21, cast_confirm -22, heartbeat -22, willpower_low -24, ui_select -24, ui_back -25, type_error -26, mix_whoosh_in -26, mix_whoosh_out -28, ui_move -29, tinnitus -30, type_key -32 |
| ambience beds (stereo loops) | amb_wind_high -30, amb_ocean_loop -30, amb_planet_hum -30, amb_city_bed -31, amb_archipelago_bed -32, amb_tutorial_bed -36, amb_interior_tone -38, amb_space_bed -40 |
| positional ambience (mono) | loops: amb_neon_hum -30, amb_ac_hum -30, amb_surf_loop -30, amb_insects_loop -32; one-shots: amb_distant_boom -26, amb_gull -28, amb_surf -28, amb_bird -30, amb_traffic_swish -30, amb_station_creak -30, amb_palm_rustle -32 |

`transform_panel_click` (-24) is not in the brief's list: it is foley triggered per animated panel, so it sits
with tether/foley levels.

## Loops

All loops are sample-exact and seamless:

- **Tonal layers are strictly periodic.** Every oscillator/LFO/beat frequency x loop length is an integer
  (`dsp.qf` quantises a frequency to the loop), and filtered layers are rendered for one warm-up period that is
  then discarded (`dsp.periodic`) or filtered circularly (`dsp.circ`).
- **Noise layers** are rendered one crossfade longer than the loop and equal-power crossfaded into the start
  (`dsp.loop_xfade`).
- **Seam placement:** rolling a loop is free, and the Vorbis encoder's edge error depends on where the file
  starts, so `write_loop` tries up to six quiet candidate seams (`dsp.seam_candidates`: low HF, low level, small
  step) and keeps the encode whose *decoded* wrap is cleanest.
- `verify.py` checks `|y[0]-y[-1]|` against the 99th-percentile sample step (<= 1) and the >6 kHz energy at the
  wrap against the loop's median (<= 3x, floored at -45 dB re the loop's power — tonal loops have almost no HF).
- The minigun loop is exactly 14 rounds in 1.0 s (14 rounds/s), dry.
- Ambience beds are 10-16 s so repetition isn't obvious.

## Sequences (KO streak)

`sequence=True` sounds (`ko_streak_<corps>`) have ordered files, where `_N` is the streak step and not a random
variation. The manifest marks them `sequence: true` and adds a `ko_streak` table (key, steps, corps -> id,
accents). `verify.py` detects the melody note of every step (the strongest partial in 450-1500 Hz in the note
body) and fails if a step is off by more than 25 cents.

## Checks in verify.py

- decoded peak <= -1 dBFS, DC, faded edges, channel count matches `stereo`, loudness within 2.5 dB of target,
  no stale files, total size < 25 MB;
- **carve rules** on every variation: feedback/UI (categories ui, feedback, ko_streak) >= 35 % of energy above
  2 kHz and < 10 % below 200 Hz (hit_crit exempt from the low rule, heartbeat from both); foley < 10 % below
  150 Hz; heavy (explosions, kaiju, hit_heavy, wall_splat, collapse, ko_shatter, land_heavy) >= 30 % below 150 Hz;
- design-intent band checks (hit_crit, ko_shatter, construct_shatter, combine, robot footstep), gun low end,
  transform pitch contours, KO-streak melody;
- **noisiness** = energy-weighted octave-band spectral flatness, median over the loud frames (0 = partials /
  tones, ~0.56 = any noise, including band-passed hiss — full-band flatness misses filtered hiss). The per-category
  median is compared with the pre-redesign backup in `target/audio/sfx_before/` when present.

## Adding or changing a sound

```python
@sound('my_sound', 'combat', variations=6, pitch=P_IMPACT, lufs=-15, notes='...')
def my_sound(r, v):              # r: seeded numpy Generator, v: variation index
    out = buf(0.8)               # padded scratch buffer; trailing silence is trimmed later
    add(out, click(r, 2500, 0.003), 0, 0.6)                       # contact transient (<= 3 ms)
    add(out, material_hit(r, 'stone', 1.2, 1.3), 0, 1.0)          # the struck object (modal)
    add(out, thump(0.5, 140, 45, 0.04, 0.15, drive=2.5), 0, 1.0)  # low-end weight
    return carve_heavy(out)                                       # carve by role; no reverb
```

For stereo beds pass `stereo=True, loop=True` and return an `(n, 2)` array (`dsp.st(l, r)`). Then run
`verify.py` and look at `target/audio/sfx_spectrograms.png`.
