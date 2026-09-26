# bd_voice — experimental push-to-talk voice casting

Hold the voice key, say a phrase ("flaming hammer", "pikachu"), release. The clip is transcribed
locally by Whisper (whisper.cpp via `whisper-rs`, **CPU only**) and the cleaned lowercase text goes
through the same `bd_lexicon` resolver as typed input. No cloud, no LLM. For fun; not benchmarked
into the core loop.

## API

```rust
use bd_voice::{VoiceConfig, VoiceEvent, VoiceInput};

let voice = VoiceInput::new(VoiceConfig {
    initial_prompt: "Mjolnir, Pikachu, Godzilla, Charizard, hammer, shield.".into(),
    ..Default::default()                 // model: assets/models/ggml-base.en-q5_1.bin
})?;                                     // opens default mic now; model loads on a background thread

// every frame (all methods take &self; VoiceInput is Send + Sync => fine as a Bevy Resource):
voice.set_push_to_talk(keys.pressed(KeyCode::KeyV));
while let Some(ev) = voice.poll() {
    match ev {
        VoiceEvent::Ready => {}                          // model loaded + warmed up
        VoiceEvent::Listening => {}                      // key down, capturing
        VoiceEvent::Level(l) => ring_pulse = l,          // 0..1, ~30 Hz while held
        VoiceEvent::Transcribing => {}                   // released, Whisper running
        VoiceEvent::Final { text, ms } => cast(&text),   // "captain america's shield", ms = release -> text
        VoiceEvent::Discarded(_reason) => {}             // too short (<250 ms speech) / no speech
        VoiceEvent::Error(e) => warn!("{e}"),
    }
}
```

Also: `submit_clip(samples, rate)` (transcribe a recorded mono clip through the same pipeline),
`is_ready()`, `mic_info()`, `bd_voice::ENABLED`. Pure helpers live in `bd_voice::dsp` / `bd_voice::text`.

Pipeline: cpal default input (any format/rate, downmixed on the audio thread) → 150 ms pre-roll
ring buffer → capture while held → 200 ms tail after release (8 s max) → windowed-sinc resample to
16 kHz → energy VAD trim (reject < 250 ms of speech) → peak normalise + pad → Whisper
(`base.en` q5_1, greedy, English, no timestamps, single segment, no temperature fallback,
suppress blank + non-speech tokens, `initial_prompt` bias, `audio_ctx` 256, max 32 tokens)
→ lowercase, strip punctuation (in-word apostrophes kept), fold accents, collapse Whisper's
repetition loops ("charizard charizard" → "charizard").

Threads: `bd_voice-mic` (owns the cpal stream + PTT state machine) and `bd_voice-whisper`
(owns the model + one reused `WhisperState`; FullParams built once). Nothing blocks the game
thread; dropping `VoiceInput` does not join.

## Cargo feature

`whisper` (off by default). Off = zero-dependency stub: `VoiceInput::new` returns
`Err(VoiceError::Unavailable)`, so the game builds on machines without the toolchain below.
Suggested wiring in bd_game: `voice = ["bd_voice/whisper"]`.

## Build requirements (Windows, feature `whisper`)

1. **MSVC Build Tools + CMake** (whisper.cpp is built from source by `whisper-rs-sys`).
2. **LLVM / libclang** for bindgen — *required on Windows*: the bindings bundled in
   `whisper-rs-sys` are Linux-generated and fail layout asserts on MSVC
   (`WHISPER_DONT_GENERATE_BINDINGS=1` does **not** work here).
   `winget install --id LLVM.LLVM -e`. `clang-sys` finds `C:\Program Files\LLVM\bin\libclang.dll`
   automatically (verified with no `LIBCLANG_PATH`); if LLVM is elsewhere set
   `LIBCLANG_PATH=<llvm>\bin`.
3. **Optimisation flags — important.** `cmake` crate 0.1.58 + the Visual Studio generator
   overrides `CMAKE_<LANG>_FLAGS_RELEASE` *without `/O2`*, so ggml is compiled unoptimised and
   inference is ~7x slower (≈750 ms instead of ≈110 ms per command). `whisper-rs-sys` forwards
   any `CMAKE_*` env var as a CMake define, which fixes it. Put this in the workspace
   `.cargo/config.toml` (Windows/MSVC-only values; `/MD` matches Rust's default dynamic CRT):

   ```toml
   [env]
   CMAKE_C_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG"
   CMAKE_CXX_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG /EHsc"
   CMAKE_C_FLAGS_RELWITHDEBINFO = "/MD /Zi /O2 /Ob1 /DNDEBUG"
   CMAKE_CXX_FLAGS_RELWITHDEBINFO = "/MD /Zi /O2 /Ob1 /DNDEBUG /EHsc"
   ```

   Changing env vars does not re-run the build script: `cargo clean -p whisper-rs-sys` after
   adding them. Check: the build output for whisper-rs-sys should show `/O2 /arch:AVX2`.
4. First build of whisper-rs-sys takes ~1–2 min.

## Model

`pwsh tools/voice/fetch_model.ps1` → `assets/models/ggml-base.en-q5_1.bin` (57 MiB, pinned HF
commit + SHA256; `assets/models/` has its own `.gitignore`). `-Full` also fetches
`ggml-base.en.bin` (141 MiB f16): same transcripts on the test set, same speed, so q5_1 is the
default. Memory: ~150 MB resident (59 MB weights + ~230 MB whisper.cpp buffers, partly
untouched); stable over repeated casts.

## Measured (i7-12700K, 8 threads, release, `examples/ptt.rs`)

Test clips from Windows TTS (`powershell -File tools/voice/make_test_wavs.ps1 [-Voice 'Microsoft Zira Desktop']`),
44.1 kHz. Times are submit→Final (inference + resample/VAD) with an idle machine; with the
mic add the 200 ms tail for release→Final (≈ 310–340 ms).

| phrase | David | Zira | ms (median) |
|---|---|---|---|
| flaming hammer | flaming hammer | flaming hammer | 115 / 122 |
| pikachu | p k chip | pikachu | 116 / 112 |
| mjolnir | mjolnir | joelnir | 117 / 112 |
| captain america's shield | captain america's shield | captain america shield | 131 / 123 |
| bouncy wall with legs | bouncy wall with legs | bouncy wall with legs | 132 / 139 |
| giant ice sword | giant eye sword | giant ice sword | 121 / 118 |
| charizard | charizard | charizard | 111 / 127 |

Model load + warm-up: 170–450 ms. With other builds saturating the CPU: 150–650 ms, outliers
~1–2 s. `audio_ctx`: 1500 (Whisper default) gives identical text at ~750 ms; "fit the clip"
(~130) truncates and loops; 256 is the sweet spot. Flash attention and 12 threads: no gain;
4 threads: ~160 ms. Without the prompt "mjolnir" → "john ear".

## Known issues

- Live-mic transcription is untested by me: the default mic here ("Microphone", 48 kHz) opened
  and the Listening → Level → Transcribing → Discarded path ran, but it only recorded silence.
  The PTT/pre-roll/tail state machine has a unit test with synthetic audio.
- The mic stream stays open while `VoiceInput` exists (needed for pre-roll), so Windows shows the
  microphone-in-use indicator. Drop `VoiceInput` when voice mode is off.
- Inference briefly saturates `threads` cores (~0.1 s per cast); set `threads: 4` if it causes
  frame hitches alongside Bevy's task pools.
- Hallucination on noise is limited by the VAD + non-speech suppression but not eliminated;
  rely on the lexicon rejecting junk.
- Adding bd_voice's deps changed the shared `Cargo.lock`.
