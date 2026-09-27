# bd_music — the Brightest Day soundtrack

Our own chiptune arrangements of public-domain classical pieces. The music is written as text in
`assets/music/*.ron`, synthesised live by a deterministic, band-limited chip synth, and played by an
adaptive player that bd_audio drives on the audio thread. There are no audio files, no samples, and no
recordings.

```
assets/music/instruments.ron   shared instrument bank (pulse / triangle / saw / sine / "hard light" FM)
assets/music/stingers.ron      KO, streak, match point, last standing, victory, defeat
assets/music/<track>.ron       one track per file (the track name is the file stem)
assets/music/remix/kit.ron     remix instruments (808, reese, supersaw, ...) and genre templates
assets/music/remix/<track>_<style>.ron   remix variants (31), each a data variant of a base track
assets/music/themes/<theme>.ron   franchise themes (12 original pieces) + themes/franchises.ron (power -> theme)
crates/bd_music/src/notation.rs  the MML dialect parser (bar-checked)
crates/bd_music/src/song.rs      RON schema -> compiled songs, MusicLibrary
crates/bd_music/src/synth.rs     oscillators, envelopes, arps, vibrato, glides, NES noise drums, filters
crates/bd_music/src/player.rs    MusicPlayer: sequencer, decks, stems, stingers, adaptive API
crates/bd_music/src/franchise.rs  franchise -> theme mapping (per power key, per id, per power file)
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

## Remixes

Every piece also exists as 2-3 head-nod remixes. The classical theme stays in front, and genre
conventions supply the groove: 808s, supersaws, filter builds and drops. They are data variants of
their base track: they inherit its melody patterns, and chord progressions (`ch_*` patterns in the
base file) drive generated bass, chord and arp parts.

| base | remixes (bpm) |
|---|---|
| menu (Nachtmusik) | house 124, lofi 86 |
| lobby (Gymnopédie) | lofi 82, synthwave 100 |
| tutorial (Minuet in G) | chipedm 130, house 124 |
| archipelago_explore (Spring) | house 122, lofi 90 |
| archipelago_combat (Summer) | dnb 172, trap 140 (half-time), chipedm 150 |
| miami (Rondo alla Turca) | house 126, synthwave 108, phonk 132 |
| orbit_float (Blue Danube) | synthwave 104, lofi 84 |
| orbit_combat (Valkyries) | synthwave 110, trap 140, dnb 174 |
| sudden_death (Mountain King) | trap 110→140, phonk 118→138, dnb 150→174 (accelerating intros) |
| final_stock (Bumblebee) | dnb 174, chipedm 150, trap 145 |
| party (Can-can) | chipedm 140, house 128 |
| party_results (William Tell) | dnb 170, synthwave 108 |
| results (Ode to Joy) | chipedm 128, trap 140 |

Styles:
- **Trap:** half-time sliding 808s, pitched and rolled hats, clap on three.
- **Synthwave:** dual-saw leads, gated snare, octave saw bass, 16th plucks.
- **House:** four-on-the-floor, offbeat bass and open hats, pumping stabs, filter-sweep breaks.
- **DnB:** original two-step breaks with ghost snares, reese bass, half-time breakdowns.
- **Lo-fi:** swung 8ths, electric-piano sevenths, crackle and tape wow, a mild crush.
- **Phonk:** a pitched FM cowbell plays the theme over distorted 808s.
- **Chip EDM:** our chip voices plus 60 Hz arps, pumping supersaw chords and a supersaw drop.

Waltzes (3/4) are straightened into 4/4 by `remeter`, which holds each bar's first beat. Valkyries
has hand-written 4/4 versions of its motif.

**Adaptive behaviour.** Intensity does more than add layers here.

| state | what plays |
|---|---|
| exploring | base and drive stems are low-passed (`explore_cutoff`, ~700–1300 Hz) and the drums are thinned (`explore_drive`) |
| entering combat | on the bar line where intensity crosses `combat_at`, a **build** starts: noise riser, snare roll (8ths → 16ths → 32nds), resonant filter sweep up |
| the drop | after `build_bars` bars the **DROP** lands on a bar line: the filter snaps open in 4 ms, an impact hits, and every layer comes in |
| leaving combat | the filter closes over one bar |

Lo-fi variants just open up (no riser). Stingers and `SetTempo` work unchanged on every variant.

**Choosing versions.** `MusicCommand::SetStyle(Style)` sets the game option:

| option | behaviour |
|---|---|
| `Classic` | the chip arrangements |
| `Remix(Some(RemixStyle::House))` | that genre when the track has one, otherwise its remixes in rotation |
| `Remix(None)` | rotate through all of a track's remixes |
| `Shuffle` | each `Play` picks between the classic and its remixes, deterministically |

`Play { track }` with a classic id is resolved through the style. A remix id, e.g.
`lib.track_id("menu_house")`, plays that exact variant. Changing the style while a track plays
switches versions on the next bar line.

## Franchise themes

`assets/music/themes/` holds 12 ORIGINAL chip pieces composed from scratch for Brightest Day. Each
one evokes a franchise's genre and energy through tempo, mode, instrumentation and groove only; no
existing theme's melody, hook, bassline or lyric is used. The genres are surf-punk, a shanty jig,
80s synth-rock, J-rock, a shinobi breakbeat, disco-funk, a hero fanfare, a space march, an 8-bit
adventure, a heroic overworld, a four-elements world piece and an epic choir. They are ordinary
songs (the remix kit's instruments and templates are available to them) with a 2-bar `sting`
intro. `MusicCommand::Theme { track }` plays one on the **theme layer**:

- It swells in on the map track's next beat, starting with its sting.
- The map track ducks out but keeps sequencing, so it comes back in place when the theme fades out.
- A theme → theme change happens on a bar line.
- KO and streak stingers follow the theme's grid and key; the big stingers cut it.

`themes/franchises.ron` maps powers to themes. bd_audio's `ThemeDirector` decides when a theme plays:
local player only, 12 s keep-alive, 6 s minimum hold, 8 s re-entry cooldown, and the Settings toggle.
Full details, the track list and the mapping are in `docs/music/franchise_themes.md`.
`render -- --themes` renders every theme and a swell demo, and `--calibrate --themes` levels them.

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

Section options for remixes:

| option | meaning |
|---|---|
| `chords: "G \| Em C \| %"` | chord progression, 1, 2 or 4 chords per bar; `%` repeats the previous bar; `$pattern` and `[ ]n` work |
| channel `gen: "$trap_808"` | a template tiled over any section that has chords and doesn't write the channel |
| `mute: [..]` | silence channels in this section |
| `remeter: [..]` | channels written in the base's meter |
| `swing` / `swing_unit` | swing amount and grid |
| `auto: [(target: "all" \| "base" \| "drive" \| "lead" \| <channel>, param: Cutoff \| Res, from, to, start, bars)]` | filter automation lanes |

Song-level remix fields: `base`, `style`, `sub_bass`, `hpf_hz`, `pump: (depth_db, release, stems)`,
`adaptive: (explore_cutoff, explore_res, explore_drive, build_bars, build_cutoff, build_res, riser,
roll, impact, combat_at, filter_lead)`, `lofi: (bits, downsample, crackle, wow, wow_rate)`.

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
| drums | chip kit `k s h o c t m x n` (kick, snare, closed hat, open hat, crash, low tom, mid tom, clap, metallic blip) and modern kit `K S X G H O C b W R I`. Modern: punchy synth kick, noise+band-pass snare and clap, gated snare, closed/open hats (a hit on the same channel chokes the previous one), crash, cowbell, white-noise riser and reverse cymbal (both swell over the written length), impact. `p4` sets the pitch of the following hits (trap-hat pitch ramps). |
| `!c` `?c` | accent (+3 volume) / ghost note (40 %) |
| `R T F S O N` | with a section `chords`: root, third, fifth, seventh (or octave), octave, ninth of the chord sounding now; `X`/`Y` = triad / seventh chord (arpeggiated); `u4` keeps roots in one register (`o1 u4` = E1..D#2 for 808s) |

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
* Remix additions:
  * The 808 is a sine with a pitch-drop envelope, long decay, glides and soft saturation (`drive`).
  * Supersaw uses up to 7 band-limited saws (the CPU cap) with detune and stereo spread; `unison: 2`
    is the dual saw.
  * Every voice filter can be a resonant state-variable filter (`res`). Each deck also runs resonant
    stem filters for the adaptive sweeps and `auto` lanes.
  * A sidechain pump: kicks duck the listed stems (4 ms attack, `release`).
  * The modern drum kit, risers and impacts.
  * Groove: swing, ghost notes, accents.
  * Lo-fi colour: bit-crush, sample-hold, vinyl crackle, tape wow.
* Output: each stem is high-passed (12 dB/oct) and low-passed at 11.5 kHz (4th-order Butterworth).
  The high-pass is at 60 Hz for chip tracks (no sub). It is the variant's `hpf_hz` for remixes:
  32 Hz for `sub_bass` variants, 45 Hz for lo-fi.

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
* `SetStyle(Style)` chooses Classic / Remix / Shuffle (see Remixes). `status().phase` gives the remix
  state (Calm / Build / Combat). `status().sub_bass` and `status().hpf_hz` say whether the playing
  variant needs the sub band, and where to put the music high-pass.
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
* **Sub-bass remixes (808s, sub kicks).** Every remix except the lo-fi ones is flagged
  `sub_bass: true`, with a suggested high-pass of 32 Hz.
  * While `status().sub_bass` is true, bd_audio must lower its Music-bus high-pass (MusicCarve
    currently cuts at 60 Hz) to `status().hpf_hz`. Restore 60 Hz for chip tracks.
  * **Sidechain the music's sub band under impacts and explosions.** Duck the music below ~120 Hz by
    6–9 dB (attack 5 ms, release 150–250 ms) on heavy hits, KOs and explosions, so an 808 never masks
    a hit. A low-shelf or split-band duck on the Music bus keyed from the Impacts bus is enough.
  * Remixes are calibrated to the same -23 LUFS, so switching versions keeps the level.
* **Frequency pockets.** Chip tracks leave **< 60 Hz** to impacts and explosions (already high-passed).
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
cargo run --release -p bd_music --example render -- --remix-medley target/music/remix_medley.wav --medley-secs 16
cargo run --release -p bd_music --example render -- --track menu_house --intensity 0.3 --drop-at 10 --seconds 30 --out target/music/x.wav
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
* `--all` also writes `<remix>_drop.wav` (exploring → intensity 1.0 at 8 s → build → drop) and a
  remix table in the report: loop length, LUFS, sub/HPF, build and drop times, CPU, and the
  heaviest variant.
* The tests cover:
  * seamless loops on every track **including all 31 remixes**;
  * the drop landing on a bar line exactly `build_bars` bars after the build;
  * style selection and switching;
  * remix determinism and callback-size independence;
  * every piece having ≥ 2 remixes (≥ 3 for the combat tracks);
  * zero allocation through the remix path;
  * seamless loops: no step at the seam larger than any in the body, identical note-ons per bar
    after the wrap, and matching stem levels;
  * sample-aligned stems, and stems that sum exactly to the mix;
  * bit-identical output for any callback size;
  * intensity changes only on bar lines, and stingers on the beat or bar grid;
  * track switches on bar lines, victory endings, tempo multiplier;
  * no NaN and no clipping on any track;
  * no allocation, and band-limited oscillators.
