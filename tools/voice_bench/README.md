# tools/voice_bench — the voice fluency bench (dev tool, never shipped)

Measures what the owner cares about: *say any move / character / chain → the game fires exactly
what typing it would.* Every clip goes through the REAL paths: Windows TTS audio → `bd_voice`
(resample, VAD, the game's Whisper config and prompt, the transcript clean-up) → bd_game
`voice_fluency::heard_line` (+ streaming beats) → bd_game `toolbox::command_for` / the lexicon.

```
pwsh -File tools\voice_bench\run.ps1 -Corpus -Tag new -Prompt new -Stream     # the current path
pwsh -File tools\voice_bench\run.ps1 -Tag before -Prompt old -Norm old -Raw   # the pre-fluency path
```

Stages (each re-usable; outputs in `target\voice_bench\`, ignored by git):

1. **corpus** — `reader.rs::voice_bench_corpus` (an ignored test in bd_game's test binary; the file
   is compiled in only under `cfg(test)` from `crates/bd_game/src/voice_fluency.rs`). Builds
   ~800 lines from the shipped power data: famous moves (a curated list, validated against the data),
   hand-written character kits ("<name> ultimate" / "<name> <move>"), generated kits, obscure names
   (each hand power's canonical shout per slot), chains (2–4 steps; "then", ",", "into",
   "and then", or a spoken beat), described moves, modifiers / fusions, shouted and drawn-out
   deliveries, and ordinary constructs (the control set: they must NOT be hijacked).
   Writes `corpus.tsv` (with the TYPED reading = the truth), `tts_jobs.tsv` (3 voices a line),
   `clips.tsv` (a recording condition per clip), `prompt_old.txt` / `prompt_new.txt`.
2. **tts** — `tts.ps1` (Windows PowerShell 5.1): SAPI David / Zira and OneCore David / Zira / Mark,
   SSML prosody (rate −10 % / 0 / +15 %, shouted = x-loud + high pitch, drawn-out = x-slow last word,
   beats = `<break 550ms>`). WAVs are a cache.
3. **whisper** — `transcribe.rs`, built as the bd_voice example `voice_bench` (own target dir
   `target\voice_fluid`). Adds the PTT framing (150 ms pre-roll, 200 ms tail) and a condition:
   clean / noise (pink, ~16 dB SNR) / room (Schroeder reverb + noise) / quiet (−26 dB gain + noise).
   One clip at a time through `VoiceInput::submit_clip`, so `ms` is real latency. `--stream` cuts
   each clip at its pauses (`bd_voice::dsp::split_at_pauses`, the same rule as the live capture
   thread) and transcribes beat by beat. Writes Whisper's RAW text; the scorer applies the clean-up.
4. **score** — `reader.rs::voice_bench_score`: a clip HITS when its reading equals the typed line's
   (same moves in order, same described spec, same twist, same construct). Report by category /
   voice / condition, Whisper + reader latency, hijacked constructs, and every missed phrase with
   what was heard: `report_<tag>.md`, `misses_<tag>.tsv`.

Caveat: TTS voices mispronounce coined names ("Chidori", "Amaterasu") far worse than a fan does,
and they don't shout or stretch like a person; treat absolute numbers as a pessimistic floor and
the before/after deltas as the signal.
