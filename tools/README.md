# tools/

Dev-side scripts and generators (none of this ships: `package.ps1` copies only the game exe, `assets/`
and the notices).

- `check.ps1` — the gate (fmt / clippy / tests / soak / bench / smoke).
- `package.ps1` — release build + `dist/BrightestDay-win64.zip`.
- `net_smoke.ps1` — multiplayer smoke test (headless host + bot clients over real sockets).
- `lexicon_build/` — builds `assets/lexicon.bin` (`python tools/lexicon_build/build.py`).
- `icons/`, `iconcheck/`, `maps/`, `sfxgen/`, `voice/`, `balance/` — content generators, checks and
  helpers (voice model fetch, bot tournaments).

## Phrase preview (owner tool)

Double-click **`phrase_preview.cmd`** in the repo root (or run it from a terminal). It builds once in
release into its own target dir (`target\tools`, so it never fights the shared build), then asks
`phrase>`: type anything a player might type or say and it prints what the ring would make of it, in
plain English, through the game's own pipeline (`Lexicon::intent` → the costume summon hook → the sim's
delivery / clamps / signature amplifier / cost / DANGER alert / power-ups gates), and writes a 3/4 picture
of the construct to `target\phrase_preview\last.png` (opened in the default viewer).

- `:voice` / `:typed` switch to the spoken path (sound-alike names: "eaton" → Ethan) and back;
  `:open` toggles the picture opening; `:help`; `:q` quits.
- Flags: `--spoken`, `--no-open`, `--out-dir DIR` (also keeps `DIR\<phrase>.png`), and phrases as
  arguments preview them and exit: `phrase_preview.cmd --no-open "reinhardt charge" "final flash"`.
- The sample roster of other players is Ethan, Mia, Sam and Nova.
- Source: `crates/bd_shapes/examples/phrase.rs` (the software rasteriser is shared with
  `costume_gallery` in `examples/raster/`). Samples: `docs/phrase_preview/`.
