# asr-bench — local streaming speech recognition bench

Measures candidate local ASR paths for short spoken game commands as a
*control loop* (partials while speaking, finalization after the player stops),
not just WER. Results and the recommendation live in
[`docs/experiments/ASR.md`](../../docs/experiments/ASR.md).

Everything runs through the same module the game will use,
[`src/voice/asr-engine.mjs`](../../src/voice/asr-engine.mjs), with presets in
[`src/voice/asr-presets.mjs`](../../src/voice/asr-presets.mjs).

## Re-running

Requires Node ≥ 20 (tested with 22.22), `tar` with bzip2 support, ~4 GB disk.

```bash
cd tools/asr-bench
npm install                      # sherpa-onnx-node (pinned) + platform binary package
node download-models.mjs         # all models into <repo>/models (≈2.9 GB, sha256 recorded in models.registry.lock.json)
node gen-audio.mjs               # TTS test set -> data/audio/{clean,noisy}/<voice>/<id>.wav (~4 min on 4 cores)
node word-times.mjs              # reference word end times via Parakeet (for per-word partial latency)
./run-all.sh                     # full matrix, sequential (~1-2 h on 4 cores); results in data/results/*.json
node report.mjs                  # -> data/results/summary.md
```

Single runs:

```bash
node bench.mjs --preset nemo-fc-80ms --mode accuracy --threads 2
node bench.mjs --preset zipformer-20M --mode accuracy --hotwords          # contextual biasing
node bench.mjs --preset moonshine2-base --mode latency --threads 1 --voices kok-adam,libritts
node ../../src/voice/test-asr-engine.mjs data/audio/clean/kok-adam/c01.wav nemo-fc-80ms --realtime
```

Options: `--cond clean|noisy|clean,noisy`, `--voices a,b`, `--limit N`,
`--decoding modified_beam_search`, `--partial-ms 250` (offline re-decode
period), `--endpoint-ms 500` (hands-free trailing silence), `--tag name`.
Only run one bench process at a time: timing runs share the CPU.

## Files

| file | purpose |
|---|---|
| `models.registry.json` | every model/TTS voice used (GitHub release asset paths of k2-fsa/sherpa-onnx) |
| `models.registry.lock.json` | sha256 of each downloaded archive (written by the downloader, verified on re-download) |
| `download-models.mjs` | downloads + verifies + extracts; `--group asr|vad|tts` or ids |
| `utterances.json` | 64 hand-written commands (create / follow-up / action / odd / disfluent / numeric / short) + game hotword list |
| `gen-audio.mjs` | sherpa-onnx TTS (Kokoro ×3 voices, Piper libritts_r multi-speaker, Piper alan, Piper amy-low), speeds 0.9–1.25, 16 kHz; noisy copy = light reverb + pink/white noise at 10–15 dB SNR |
| `word-times.mjs` | word end-time estimates (Parakeet-TDT token timestamps aligned to the reference) |
| `bench.mjs` | accuracy mode and virtual-clock latency simulation for one preset |
| `lib/text.mjs` | normalization (lowercase, digits→words, fillers removed, compound spellings), WER alignment, keyword accuracy |
| `lib/audio.mjs` | resampler, noise, reverb, SNR mixing, speech bounds |
| `report.mjs` | aggregates `data/results/*.json` into tables |
| `wasm-check.mjs` | same streaming model through the WASM build (`sherpa-onnx` npm) for comparison |

## How latency is simulated

The engine is synchronous (all compute happens inside `acceptPcm()` /
`endUtterance()`), which is how it runs in a dedicated utility process. The
bench feeds 100 ms chunks on a *virtual clock*: chunk *i* arrives at its real
time end, processing starts at `max(arrival, previous chunk done)` and lasts the
measured wall time of the call. Partial/final events are stamped with virtual
time. This reproduces queueing/backlog exactly as a single worker would see it
without having to sleep in real time. Push-to-talk release is placed 100 ms
after the energy-based end of speech; hands-free runs feed the trailing 1.5 s of
silence and wait for the engine's own endpoint (VAD min-silence 500 ms for
offline kinds, sherpa rule2 trailing silence 500 ms for streaming kinds).
