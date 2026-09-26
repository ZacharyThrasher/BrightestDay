# bd_music — the Brightest Day soundtrack

Our own chiptune arrangements of public-domain classical pieces. The music is written as text in
`assets/music/*.ron`, synthesised live by a deterministic, band-limited chip synth, and played by an
adaptive player that bd_audio drives on the audio thread. There are no audio files, no samples, and no
recordings.

```
assets/music/instruments.ron   shared instrument bank (pulse / triangle / saw / sine / "hard light" FM)
assets/music/stingers.ron      KO, streak, match point, last standing, victory, defeat
assets/music/<track>.ron       one track per file (the track name is the file stem)
crates/bd_music/src/notation.rs  the MML dialect parser (bar-checked)
crates/bd_music/src/song.rs      RON schema -> compiled songs, MusicLibrary
crates/bd_music/src/synth.rs     oscillators, envelopes, arps, vibrato, glides, NES noise drums, filters
crates/bd_music/src/player.rs    MusicPlayer: sequencer, decks, stems, stingers, adaptive API
crates/bd_music/src/analysis.rs  LUFS / true peak / spectra / WAV (tools and tests only)
crates/bd_music/examples/render.rs  offline renders, the report, the medley, calibration, lint
```

## Soundtrack

| track | piece (public domain) | role | tempo, key |
|---|---|---|---|
| `menu` | Mozart, *Eine kleine Nachtmusik* K. 525, I (1787) | main menu | 132, G |
| `lobby` | Satie, *Gymnopédie No. 1* (1888) | lobby / waiting | 76, D (3/4) |
| `tutorial` | Petzold (attr. Bach), *Minuet in G* BWV Anh. 114 (1725) | movement tutorial | 120, G (3/4) |
| `archipelago_explore` | Vivaldi, *Spring* RV 269, I (1725) | Archipelago, exploring | 104, E |
| `archipelago_combat` | Vivaldi, *Summer* RV 315, III Presto (1725) | Archipelago, combat | 160, G minor (3/4) |
| `miami` | Mozart, *Rondo alla Turca* K. 331 (1783), as synthwave | Miami | 124, A minor (2/4) |
| `orbit_float` | J. Strauss II, *The Blue Danube* Op. 314 (1867) | Orbit, floating | 156, D (3/4) |
| `orbit_combat` | Wagner, *Ride of the Valkyries* (1856/1870) | Orbit, combat | 144, B minor (9/8) |
| `sudden_death` | Grieg, *In the Hall of the Mountain King* (1875) | sudden death / overtime; the intro accelerates from 112 to 164, the loop runs at 168 | 168, B minor |
| `final_stock` | Rimsky-Korsakov, *Flight of the Bumblebee* (1899–1900) | final stock / last minute | 160, A minor |
| `party` | Offenbach, *Galop infernal* ("Can-can") (1858) | party mode | 140, C (2/4) |
| `party_results` | Rossini, *William Tell* Overture finale (1829) | party results | 144, E (2/4) |
| `results` | Beethoven, Symphony No. 9, *Ode to Joy* (1824) | results / podium | 120, D |

Stingers are the Beethoven 5 "fate" motif for a KO, an original rising flourish for a streak,
the Gregorian *Dies irae* for match point, the opening of Bach's Toccata in D minor BWV 565 for the
last player standing, and original fanfares for victory and defeat. Victory plays the current
track's own `ending` section (each track has one), falling back to the fanfare.

Every track has an intro, a loop body of 80–129 s that loops sample-exactly (1.6–2.3 min with the intro), and an ending. Each
`source` field says which bars come from the score. The famous themes are transcribed from the
public-domain scores, written from memory. Episodes, counter-melodies, harmony and every
arrangement are original to Brightest Day. No modern arrangement, recording or third-party score
file was used.

## Song files

```ron
(
    title: "Nachtmusik", composer: "...", work: "...", source: "...", role: "Main menu",
    key: "G",               // key signature for notes without accidentals (G, Bb, F#m, Dm, ...)
    time: (4, 4),           // any n/1,2,4,8,16; 6/8, 9/8, 12/8 count dotted-quarter beats
    bpm: 132.0,             // quarter notes per minute
    gain_db: -4.0,          // calibrated by `render -- --calibrate` to -23 LUFS at intensity 1
    layers: (lead: 0.15, drive: 0.55, base_floor: 0.8),   // intensity thresholds (see below)
    instruments: { "trill": (wave: Pulse, arp: [0, 2], arp_rate: 16.0) },  // optional, song-local
    channels: [             // up to 8; each is a monophonic chip voice routed to one stem
        (name: "mel", inst: "lead25", stem: Lead, pan: -0.12),
        (name: "hl", inst: "hardlight", stem: Lead, echo: (ticks: 72, vol: 0.28)),
        (name: "bass", inst: "bass", stem: Base, vol: 0.9),
        (name: "drums", drums: true, stem: Drive),
    ],
    patterns: { "rocket": "o4 g4 r8 d8 g4 r8 d8 | ..." },       // reusable snippets ($rocket)
    sections: [
        (name: "a", bars: 8, parts: { "mel": "$rocket $answer", "bass": "..." }),
        (name: "b", bars: 8, key: "D", transpose: 0, bpm: 132.0, accel_to: 140.0, parts: { ... }),
    ],
    order: ["intro", "a", "a2", "b"], loop_to: "a", ending: "end",
)
```

Every part is checked against the section length and every `|` bar check. A wrong length or a
misplaced bar line fails loudly at load and names the section, channel and bar.

### Notation (one string per channel per section)

| token | meaning |
|---|---|
| `c d e f g a b` | notes; `+`/`#` sharp, `-` flat, `=` natural (explicit accidentals are absolute, otherwise the key signature applies) |
| `c4 c8. c16 c12` | lengths 1 2 4 8 16 32 64; triplets 3 6 12 24 48; dots |
| `c4&c16`, `c4^16`, `r4^8` | tie to the same pitch, or extend a note or rest |
| `r` | rest |
| `o4` `>` `<` | octave (o4 c = middle C), up, down |
| `l8` `v12` `q6` | default length, volume 0–15, gate in eighths (articulation) |
| `{ceg}4` | chord, played as a chip arpeggio (notes ascend automatically, up to 4) |
| `~e4` | glide (portamento) from the previous note |
| `@name` | instrument change; `k-12` transposes the part (semitones, absolute) |
| `[ ... ]3` | repeat |
| `$name` | insert a named pattern |
| `\|` | bar check |
| `;` | comment to end of line |
| drums | `k s h o c t m x n`: kick, snare, closed hat, open hat, crash, low tom, mid tom, clap, metallic blip |

Tip: start every bar with an explicit `o` octave. `render -- --lint` lists each channel's range per
section and flags minor-2nd / minor-9th clashes on beats, which catches wrong accidentals or octaves.

### Instruments (`instruments.ron`)

`wave` is one of `Pulse` (duty 0.125/0.25/0.5, with `duty_end`/`duty_time` sweeps), `Triangle`,
`Saw`, `Sine`, or `Fm`. `Fm` is the **hard light** voice: two detuned 2-operator FM pairs with an index
envelope, feedback and a Carson-rule band limit. All waves share an ADSR (`attack decay sustain
release`), `vol`, `gate`, delayed vibrato (`vib_depth` in cents, `vib_rate`, `vib_delay`), `arp`
macros (`arp: [0, 12]`, `arp_rate` Hz), `pitch_drop`/`pitch_time`, `glide`, and a one-pole low-pass
with an envelope (`lp_hz`, `lp_env`, `lp_time`).

## The synth

* Pulse and saw use a **4-point BLEP** (integrated cubic B-spline), not naive steps. The worst
  inharmonic (aliased) component is -50 dB for a 12.5 % pulse at A5, -54 dB for a 50 % pulse at
  E6, -84 dB for the triangle bass, and -104 dB for the hard-light FM (`tests/bandlimit.rs`).
  Pulses are DC-free, and retriggers keep oscillator phase and envelope level, so notes never click.
* The noise channel is the NES 15-bit LFSR in long and short mode. It is clocked at NES timer rates
  and box-filtered per sample. Each hit has a 0.8 ms onset ramp and drum tones sweep. A retrigger
  crossfades the previous hit over 12 ms.
* **Echo by channel delay**: any channel can have a tempo-synced delayed copy, panned opposite.
* Output: each stem is high-passed at 60 Hz (12 dB/oct, so no sub) and low-passed at 11.5 kHz
  (4th-order Butterworth).

## Adaptive player and integration contract (for bd_audio)

```rust
// Load (any thread; allocates):
let lib = Arc::new(bd_music::MusicLibrary::load_dir(Path::new("assets/music"))?);
let menu = lib.track_id("menu").unwrap();            // resolve names off the audio thread
let mut player = bd_music::MusicPlayer::new(Arc::clone(&lib), device_rate);  // ~5.5 MB, all up front

// Audio thread, every callback. No allocation, locks or syscalls.
while let Some(cmd) = music_queue.pop() { player.command(cmd); }   // MusicCommand: Copy
player.render(&mut music_bus);          // interleaved stereo f32, any length
// or, to process layers separately (all interleaved stereo, equal lengths):
player.render_stems(&mut base, &mut drive, &mut lead, &mut stinger);
let status = player.status();           // Copy: track, bars, beat, bpm, intensity, stinger... for visuals
```

* `MusicCommand::Play { track, skip_intro }` starts at once if silent. Otherwise it switches **on the
  next bar line** of the current track: the old deck stops sequencing and releases over 250 ms, and
  the new track starts on that same sample. `Stop` also waits for the bar line.
* `SetIntensity(0..1)` applies **on bar lines only**. A stem fades in over 30 ms, so the downbeat
  lands, and fades out over one bar. Stem gains: base = `base_floor → 1`; lead comes in at `layers.lead`
  (default 0.15); drive comes in at `layers.drive` (default 0.55) at 0.8, rising to 1.0. Exploration is
  about 0.3 (base and lead), combat is 1.0 (everything).
* `Stinger(kind)` behaves differently per kind:
  * `Ko` and `Streak` land on the **next beat**, overlay the track at its tempo, and duck the
    track's lead by 6 dB and drive by 1.4 dB while playing.
  * `MatchPoint` and `LastStanding` land on the **next bar**, hold the track, play, then resume the
    track on the held bar line.
  * `Victory` lands on the next bar and plays the track's `ending` section, then stops.
  * `Defeat` lands on the next bar, fades the track over a beat and plays the jingle.

  Stingers are transposed into the current track's relative key. A new stinger replaces a playing one.
* `SetTempo { multiplier, ramp_secs }` ramps tempo (0.25–4×) without changing pitch. Use it for
  final stock or sudden death, e.g. `1.0 → 1.25` over the last 30 s. bd_audio's snapshot fields
  `music_intensity` / `music_tempo` map straight onto `SetIntensity` / `SetTempo`. Send them only
  when they change; they are quantised musically anyway.
* Output is identical whatever the callback size, because rendering runs in internal 32-sample
  control blocks. A command takes effect at the next block (≤ 0.7 ms) plus its musical quantisation.
* `render` = soft safety limiter(sum of stems). The limiter is transparent below -2 dBFS; the
  calibrated tracks peak around -7 dBTP, so it never engages in practice. `render_stems` output is
  pre-limiter.
* `MusicPlayer` is `Send`, not `Sync`: the audio thread owns it. To hot-reload, build a new library
  and player off the audio thread and swap them in. Hand the old one back so it is dropped (freed)
  off the audio thread.
* Suggested game mapping:

  | game state | music action |
  |---|---|
  | title / menus | `menu` |
  | lobby | `lobby` |
  | tutorial | `tutorial` |
  | Archipelago | `archipelago_explore` at 0.3; switch to `archipelago_combat` and 1.0 when fighting |
  | Miami | `miami`, intensity from danger |
  | Orbit | `orbit_float` / `orbit_combat` |
  | a player on their last stock / last minute | `final_stock` |
  | overtime | `sudden_death` plus a tempo ramp |
  | party card pick | `party` |
  | match end | `Victory` (local winner) / `Defeat` (others), then `results` or `party_results` |

  Throttle `Ko` stingers, e.g. only for the local player's KOs and at most one per 2 s. The per-corps
  streak sounds in SFX carry the rest.

### Performance (release, 48 kHz, this dev box)

Rendering costs 5–10 ms of CPU per second of audio (varies with machine load): 0.5–1 % of one core,
about 130–160× real time, and
~0.14 µs per stereo frame. The cost is all on the audio thread and zero on the game thread. Pre-rendering
was rejected: 13 tracks × 3 stems at ~2 min would be about 1 GB of PCM, or a 100+ ms render on every
track change. `tests/alloc.rs` proves that `render`, `render_stems`, `command` and `status` never
allocate: a counting global allocator watches every code path, including switches and all six stingers.

## Mixing guidance for the Music bus

* **Loudness.** Tracks are calibrated to **-23 LUFS integrated at intensity 1.0**: -23.0 ± 0.1 at
  1.0 and -23.2 … -24.3 at 0.3, with true peak ≤ -6.8 dBTP. With the Music fader at 0 dB, the music sits
  8–10 LU under combat SFX (target SFX momentary around -14 … -16 LUFS). Stingers reach about -19 …
  -21 LUFS momentary. Don't let the HDR loudness window push the music up in quiet moments; treat
  the Music bus as a fixed-level bed.
* **Frequency pockets.** The music leaves **< 60 Hz** to impacts and explosions (already high-passed).
  Add a gentle **-2 … -3 dB bell at ~2.5 kHz (Q ≈ 0.7)** only while voice chat, announcer or other
  dialogue-like cues play. Keep a **-3 dB dip across 5–12 kHz** permanently: that is the UI and
  feedback band (hit ticks, crosshair, ring UI). The music has nothing above 11.5 kHz, so the band
  above 12 kHz belongs entirely to SFX and UI.
  On every track the lead stem is the loudest layer (1.3–14 dB above the base), and the drums are
  modest (drive stem -29 … -39 LUFS), so SFX transients stay on top.
* **Ducking.**
  * Heavy hits and KO confirms duck the music **-3 … -4 dB**: attack 10 ms, hold 80 ms, release
    250–400 ms. Heavy and KO tiers only; never duck on every light hit, or the music pumps.
  * A KO you dealt ducks the music ~250 ms (DESIGN.md "negative space") so the streak note rings.
  * Push-to-talk voice ducks the music -3 dB plus the 2.5 kHz bell.
  * The player already ducks its own lead and drive under overlay stingers.
* **Snapshots.**

  | snapshot | music bus |
  |---|---|
  | `RingOpen` (typing a construct) | -4 dB and LPF ~4 kHz; musical but out of the way; intensity unchanged |
  | `Space` | "muffled": LPF ~1.5 kHz, -3 dB, a little more reverb send |
  | `Danger` | `SetIntensity(1.0)` |
  | pause menu | -6 dB, LPF ~2 kHz |
  | loading or alt-tab | Stop, or bus mute |

* **Reverb and space.** The music is dry chip. Keep the Music send to the shared world reverb
  ≤ -18 dB (orbit_float can take -12 dB). Don't run it through occlusion or air absorption, which
  are for diegetic sources only.
* **Stereo.** Channels are panned within ±0.35 and echoes are mirrored. Everything is
  mono-compatible.

## Verification

```powershell
$env:CARGO_TARGET_DIR = "target\music"
cargo run --release -p bd_music --example render -- --track menu --intensity 0.3 --out target/music/menu_0.3.wav
cargo run --release -p bd_music --example render -- --all          # every track at 0.3 and 1.0 + target/music/report.md
cargo run --release -p bd_music --example render -- --medley target/music/medley.wav
cargo run --release -p bd_music --example render -- --stingers target/music/stingers.wav
cargo run --release -p bd_music --example render -- --track miami --spectrogram target/music/miami.png --start 10 --seconds 15
cargo run --release -p bd_music --example render -- --lint          # ranges + clashes per section
cargo run --release -p bd_music --example render -- --calibrate     # rewrite gain_db to hit -23 LUFS
cargo test -p bd_music ; cargo clippy -p bd_music --all-targets -- -D warnings ; cargo fmt -p bd_music --check
```

* `--all` writes the per-track WAVs to `target/music/<track>_0.3.wav` and `<track>_1.0.wav`, plus
  `report.md`. The report gives the length, loop start and loop length as samples (predicted by the
  integer tempo math and observed in the real player; they are equal), LUFS at 0.3 and 1.0, true
  and sample peak, band energies, stem loudness, and CPU cost.
* The medley plays each track for ~20 s through the real adaptive API: exploration then combat,
  bar-line switches, stingers and a tempo ramp. It is lifted to -16 LUFS for listening; in game the
  music sits at -23.
* The tests cover:
  * seamless loops: no step at the seam larger than any in the body, identical note-ons per bar
    after the wrap, and matching stem levels;
  * sample-aligned stems, and stems that sum exactly to the mix;
  * bit-identical output for any callback size;
  * intensity changes only on bar lines, and stingers on the beat or bar grid;
  * track switches on bar lines, victory endings, tempo multiplier;
  * no NaN and no clipping on any track;
  * no allocation, and band-limited oscillators.
