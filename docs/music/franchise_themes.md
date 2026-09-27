# Franchise themes

When you use a franchise's powers, its theme swells in over the map music. Examples are mutant-turtle
kits, devil fruits, transforming robots, golden-aura martial arts, ninja kits, stands, superhero
origins, space knights, pocket creatures, the elf hero, bending and the old gods. It stays up while
you keep using that franchise. It fades back to the map track, which kept its place, about 12 s after
your last use.

## Originality (read this before adding a theme)

**Every theme here was composed from scratch for Brightest Day.** No melody, hook, bassline, chord
riff or lyric of any existing theme song, opening, film score or game track was transcribed, quoted or
paraphrased. The themes evoke each franchise only through generic, non-protectable elements:

- tempo and meter
- key and mode
- instrumentation and chip timbres
- groove and rhythm feel
- production idiom (surf-punk, shanty jig, 80s synth-rock, J-rock, disco-funk, orchestral fanfare,
  march, 8-bit adventure, world-music, epic choir)

Several themes steer clear of the best-known gestures on purpose:

- the space march has no triplet pickups or imperial march figure;
- the creature tune has no descending battle-intro run;
- the overworld tune has no famous overworld melody and no "item" fanfare.

Track names are generic genre names. None is a song title or a trademarked slogan. Each file's
`composer` is "Brightest Day (original)", and its `source` begins with "Original composition". The
tests check both fields.

Adding a theme: write original material only, use a generic name, and keep the same provenance
fields.

## Track list

Every theme is an ordinary bd_music song in `assets/music/themes/<name>.ron`. Its 2-bar `sting`
(the intro section) is the short signature the theme **enters with** when it swells in. The loop
body is the full-length battle theme, and every theme also has an `ending`. All themes are
calibrated to −23 LUFS at intensity 1, the same level as the map tracks. The table was measured with
`render -- --themes`.

| theme (file) | franchise | genre / feel | key / mode | tempo, meter | structure (bars) | loop | true peak |
|---|---|---|---|---|---|---|---|
| **Sewer Surf Stomp** (`sewer_surf_stomp`) | mutant ninja turtles | 90s-cartoon surf-punk: skate-punk beat, duty-swept pulse lead with slides, surf-picked 16ths, power-chord chip arps | E minor (pentatonic + blues colour) | 172, 4/4 | sting 2 · a 8 · b 8 · a2 8 · c 8 (half-time bridge) · b2 8 (+5 semitones) · a3 8 · end 2 | 67.0 s | −8.9 dBTP |
| **Open Sea Jig** (`open_sea_jig`) | pirate adventure (devil fruits, the straw-hat crew, haki) | swashbuckling shanty jig: 12/8 lilt, brassy square lead, hard-light brass, organ "accordion", oom-pah-pah arps, deck stomp | D major; B-minor storm | dotted quarter 104 (quarter 156), 12/8 | sting 2 (horn call) · a 8 (jig verse) · b 8 (anthem) · c 8 (storm) · b2 8 (full crew) · end 2 | 73.8 s | −11.3 |
| **Chrome Titans** (`chrome_titans`) | transforming robots | heroic 80s synth-rock: octave saw bass, gated snare, dual-saw lead, metallic FM "robot voice" stabs, riser transformation sting | D minor → F major chorus; Eb colour in the bridge | 132, 4/4 | sting 2 · a 8 · b 8 · c 8 (breakdown) · a2 8 · b2 8 · end 2 | 72.7 s | −8.0 |
| **Sky Burst Overdrive** (`sky_burst_overdrive`) | power-up martial arts | shouty J-rock chip: fast rock beat, chip power chords, gang-shout claps, screaming 12.5 % lead, key-change power-up | A major (F♯ minor verse); last chorus up a tone to B | 176, 4/4 | sting 2 · a 8 · b 8 (pre) · c 8 (chorus) · a2 8 · b2 8 · c2 8 (+2) · end 2 | 65.5 s | −8.0 |
| **Shadow Step Shinobi** (`shadow_step_shinobi`) | ninja villages and chakra techniques | pentatonic bamboo flute over a syncopated breakbeat, taiko toms, koto plucks in a Japanese "in"-scale colour, 16th ninja runs | D minor pentatonic (E♭ / B♭ colour) | 144, 4/4 | sting 2 · a 8 · b 8 · c 8 (koto break) · a2 8 · b2 8 · end 2 | 66.7 s | −10.2 |
| **Menace Disco Strut** (`menace_disco_strut`) | stand users | funky slap-bass disco chip: four-on-the-floor, clap backbeat, open-hat offbeats, octave-popping slap bass, brass stabs, menacing low break | A dorian (Am7–D7 vamp) | 116, 4/4 | sting 2 · a 8 · b 8 · c 8 (break) · a2 8 · b2 8 · end 2 | 82.8 s | −9.2 |
| **Skyline Vanguard** (`skyline_vanguard`) | comic-book superheroes (both big publishers) | orchestral hero fanfare chip: hard-light brass, 16th string ostinato, timpani toms and snare, soaring second theme | E♭ major; C-minor rally | 138, 4/4 | sting 2 (fanfare) · a 8 · b 8 · c 8 · a2 8 · b2 8 · end 2 | 69.6 s | −12.3 |
| **Distant Stars March** (`distant_stars_march`) | space opera (laser-sword knights, armoured hunters) | space-opera march: dotted brass rhythms, military snare, timpani, lydian-tinged lyrical horn theme | D major; D-minor menace | 112, 4/4 | sting 2 · a 8 (march) · b 8 (lyrical) · c 8 (menace) · a2 8 · b2 8 · end 2 | 85.7 s | −11.4 |
| **Tall Grass Trek** (`tall_grass_trek`) | pocket creatures and their trainers | bright 8-bit handheld adventure: two square leads, bouncing triangle bass, 60 Hz chord arps, light noise kit | G major | 152, 4/4 | sting 2 · a 8 · b 8 · c 8 (cave) · a2 8 · b2 8 · end 2 | 63.2 s | −11.1 |
| **Wildwood Valor** (`wildwood_valor`) | elf-hero overworld adventure | heroic overworld chip: gallop bass and snare, harp arps, bold pulse melody with a lydian lift | F major; D-minor forest | 132, 4/4 | sting 2 · a 8 · b 8 · c 8 (forest) · a2 8 · b2 8 · end 2 | 72.7 s | −12.3 |
| **Four Winds Accord** (`four_winds_accord`) | elemental bending | four-elements world-music chip: erhu-like bending hard-light lead, dizi flute, zither plucks, frame drums, gong | A minor pentatonic | 120, 4/4 | sting 2 · a 8 (water) · b 8 (earth, D minor) · c 8 (fire) · a2 8 (air) · end 2 | 64.0 s | −11.6 |
| **Thunder of the Old Gods** (`old_gods_thunder`) | world mythology and folklore | epic choir-chip: sine choir, supersaw choir pad, war-drum toms, string ostinato, hard-light horns, thunder impact | D minor; F-major hall of heroes | 100, 4/4 | sting 2 (thunder + choir) · a 8 · b 8 · c 8 · b2 8 · end 2 | 76.8 s | −13.0 |

All themes measure −23.0 ± 0.1 LUFS integrated at intensity 1.0 and −23.6 … −25.8 at 0.3. The loudest
sample peak of any theme is −8.0 dBFS, well under the −2 dBFS knee of the safety limiter. CPU is
3.7–5.9 ms per second of audio (fastrel), in line with the map tracks.

## Trigger mapping

`assets/music/themes/franchises.ron` maps powers to themes (`bd_music::franchise`). A power resolves
in this order, and the first match wins:

1. **Per power:** the power's own `music: Some("…")` key in `assets/powers/*.ron`. This is a new
   optional field on `PowerDef` (bd_sim, presentation only; the sim never reads it). It names a
   franchise or a theme, and `"none"` opts the power out.
2. **Per id:** a franchise's `ids`. These are exact ids, or globs such as `mutagen_*`, `*_pokemon`
   and `*contains*`.
3. **Per file:** a franchise's `files`, which covers every power in `assets/powers/<file>.ron`.

An id or file can belong to only one franchise, and this is checked at load.

| franchise | theme | files | ids (summary) |
|---|---|---|---|
| turtles | Sewer Surf Stomp | — | `mutagen_*`, the four brothers, Splinter, Shredder, Casey Jones |
| pirate | Open Sea Jig | `base_one_piece` (every devil fruit) | the crew and rival kits, observation / armament haki, Jack Sparrow |
| robots | Chrome Titans | — | the 17 robot kits in `tv_cartoons` |
| saiyan | Sky Burst Overdrive | — | the Dragon Ball kits, `ultra_instinct`, `super_saiyan`, `kaioken` |
| shinobi | Shadow Step Shinobi | — | the Naruto / Boruto kits, eight gates, byakugan, ninjutsu, sage / Kurama mode, sharingan, rinnegan, shadow clone |
| bizarre | Menace Disco Strut | `base_jojo` (every stand) | the JoJo kits |
| heroes | Skyline Vanguard | `base_marvel`, `base_dc` | the Marvel / X-Men / DC kits in `movies_comics` |
| space | Distant Stars March | — | `kit_jedi`, `kit_sith`, `kit_mandalorian` |
| critters | Tall Grass Trek | — | `ssb_pikachu`, `*_pokemon`, and the derived Pokémon kits (`dk_*`) |
| overworld | Wildwood Valor | — | `adv_link`, `adv_sheikah_slate`, `adv_triforce`, `ssb_ganondorf` |
| elements | Four Winds Accord | — | the bending arts (air, water, earth, fire, metal, lava, combustion, the Avatar state) and the Avatar / Korra kits |
| myth | Thunder of the Old Gods | `myth_folklore`, `base_folklore` | `myth_*` |

Everything else stays on the map music. That includes Bleach, the anime_modern shows, and Mario and
the other game kits. To add one, write an original theme and add a row.

## How it plays

The theme layer is a player feature in bd_music: `MusicCommand::Theme { track: Option<TrackId> }`.

- **Swell in:** the theme starts on the playing map track's **next beat**, from its sting. It fades
  up over 0.35 s while the map track ducks out over 0.9 s. If nothing is playing, it starts at once.
  If a match-point or last-standing stinger is holding the track, it waits until the stinger has
  finished.
- **The map keeps its place:** the map track keeps sequencing silently underneath. When the theme
  fades out (2.5 s), the map swells back exactly where it would have been.
- **Theme → theme** switches on the old theme's bar line with the normal 250 ms hand-off.
- **Stingers:**
  - KO and streak stingers land on the theme's beat, in the theme's key, while a theme is up.
  - Match point, last standing, victory, defeat and Stop cut the theme in 0.4 s. The big moment wins.
- **Overlay layers** such as Gear Fifth's "drums" lock to the theme's 16ths while it is up.
- **Tempo:** themes keep their own tempo. The sudden-death / final-stock tempo ramp applies to the
  map track only.
- **Mixing:** themes are calibrated like every track and sit on the Music bus, so the existing combat
  ducks put them under the SFX: heavy hit / KO / final blow of −3.5 … −9 dB, and the push-to-talk duck.
  Themes play at intensity ≥ 0.7 because they are battle themes. The stems follow the game's
  intensity above that.

The rules live in `bd_audio::music::ThemeDirector` on the game thread. It is pure logic, is
unit-tested, and sits inside `MusicDirector`:

| rule | value |
|---|---|
| who triggers | **the local player only**: their own `PowerEvent::Move`, `Ascend` (with a power), `Gained`, `Selected` (`bd_game::audio::themes::used_power`) |
| keep-alive | the theme fades out **12 s** after the last use of its franchise |
| most recent wins | another franchise takes over, but only after the current theme has held **6 s**. A use during the hold is queued, and takes over when the hold ends if that use is still fresh (≤ 4 s old) |
| cooldown | **8 s** after a theme ends before any theme can start again (a fresh use then brings it in) |
| no spam | repeat uses re-assert the playing theme to the player at most every 1.5 s, and a repeat is a no-op in the player |
| where | live matches only. Menus, loading, the results screen and leaving the match turn it off |
| setting | **Settings → Sound → MIX → FRANCHISE THEMES** (on by default; `AudioPrefs.franchise_themes`, and older settings files load as on). Turning it off ends a playing theme at once |

Timings are in `ThemeTiming::default()`.

## Files

- `assets/music/themes/*.ron`: the 12 themes.
- `assets/music/themes/franchises.ron`: the mapping.
- `crates/bd_music/src/franchise.rs`: parses and resolves the mapping.
- `crates/bd_music/src/song.rs`:
  - loads `themes/` as tracks, with `TrackInfo::theme`;
  - makes the remix kit's instruments and templates available to themes;
  - `MusicLibrary::{themes, is_theme, franchise}`.
- `crates/bd_music/src/player.rs`: the theme layer.
  - `MusicCommand::Theme`, two theme decks, the crossfade, entry on the beat, switch on the bar line;
  - stinger routing;
  - `MusicStatus::{theme, theme_level}` and `theme_debug_log`.
- `crates/bd_audio/src/mixer.rs`: `MusicCommand::Theme`.
- `crates/bd_audio/src/music.rs`:
  - `ThemeDirector`, `ThemeTiming`, `ThemeCmd`;
  - `MusicDirector::{franchise_theme, theme_used, set_themes_enabled, themes_off}`.
- `crates/bd_game/src/audio.rs`:
  - `AudioPrefs.franchise_themes`;
  - the `themes` module: `used_power`, `power_files`, `theme_of`, and the `franchise_themes` system;
  - settings apply, and turning the layer off on leaving the match.
- `crates/bd_game/src/menu.rs`: the FRANCHISE THEMES toggle.
- `crates/bd_sim/src/powers/data.rs`: the optional per-power `music` key.

## Verification

```powershell
$env:CARGO_TARGET_DIR = "target\play"
cargo run --profile fastrel -p bd_music --example render -- --themes --out-dir target/music/themes   # WAVs + swell demos + themes_report.md
cargo run --profile fastrel -p bd_music --example render -- --calibrate --themes                    # re-level the themes only
cargo run --profile fastrel -p bd_music --example render -- --lint --track sewer_surf_stomp
cargo test -p bd_music          # tests/themes.rs + every theme in the seamless-loop / no-clip test + zero-allocation test
cargo test -p bd_audio --lib theme_tests
cargo test -p bd_game -- franchise_theme_tests every_setting
```

`--themes` writes two WAVs per theme:

- `<theme>.wav`: from the sting through one loop, plus 4 s.
- `<theme>_swell.wav`: `archipelago_combat` for 6.2 s, then the theme swells in on the next map
  beat, plays 16 s and fades out, then 6 s of the map again.

It also writes `themes_report.md`: length, loop, LUFS at 1.0 and 0.3, peaks, swell levels
(map → theme → map), entrance timing and CPU.
