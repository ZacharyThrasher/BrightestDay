# Local speech recognition — measured, then dropped

**Status: dropped by design decision.** HARDLIGHT is played by typing. Typing speed and accuracy are
part of the skill ("play as fast as you can type"), the player swaps between movement and rapid typing,
and the world slows while they type (focus time, see `src/app/typing.ts`). Voice input, the ASR
pipeline (`src/voice/`, the Electron utility-process host, push-to-talk on V, microphone permission)
and the benchmark harness (`tools/asr-bench/`) were removed. They remain in git history up to commit
`c644bb9` if anyone wants to revisit.

The interpreter's error repair (`src/lang/fuzzy.ts`: edit distance + a crude phonetic key against the
game's own vocabulary) was built for ASR noise and is kept on: it repairs typing mistakes just as well
(“a tnak with wigns” → “a tank with wings”).

## What we measured before dropping it

Bench: 64 hand-written game commands (create / follow-up / action / odd / disfluent / numeric / short),
synthesised with 6 TTS voices (Kokoro ×3, Piper libritts_r, Piper alan, Piper amy-low) at speeds
0.9–1.25, 16 kHz; a noisy copy with light reverb + pink/white noise at 10–15 dB SNR. All 384 clips per
condition, push-to-talk final result, 2 threads, greedy decoding, `sherpa-onnx-node` 1.13.8 on Node 22,
Intel Xeon @ 2.10 GHz ×4 (no GPU). **Synthetic speech only — never tested with a real microphone or a
real player.**

| model | files MB | WER clean | WER noisy | keyword acc clean / noisy | exact clean / noisy | game-term recall clean / noisy | RTF (2 threads) | load ms | RSS loaded / peak MiB |
|---|---|---|---|---|---|---|---|---|---|
| zipformer-20M | 44 | 16.0% | 24.0% | 81.6% / 72.4% | 52% / 40% | 72% / 58% | 0.04 | 678 | 147 / 198 |
| zipformer-0621 | 189 | 5.8% | 10.2% | 93.9% / 89.8% | 77% / 67% | 94% / 91% | 0.13 | 1399 | 300 / 388 |
| kroko-en | 71 | 38.0% | 39.5% | 59.4% / 58.0% | 18% / 18% | 60% / 58% | 0.04 | 2717 | 187 / 353 |
| nemo-fc-80ms | 137 | 8.2% | 13.2% | 92.0% / 85.7% | 70% / 59% | 89% / 81% | 0.37 | 1393 | 273 / 359 |
| nemo-fc-480ms | 137 | 9.7% | 12.7% | 88.8% / 85.9% | 68% / 62% | 84% / 80% | 0.14 | 1473 | 273 / 368 |
| moonshine-tiny | 125 | 7.9% | 13.8% | 91.4% / 85.1% | 73% / 61% | 88% / 80% | 0.04 | 841 | 236 / 257 |
| moonshine-base | 288 | 7.1% | 14.3% | 92.1% / 87.4% | 74% / 59% | 92% / 86% | 0.08 | 1908 | 409 / 425 |
| moonshine2-tiny | 45 | 9.0% | 16.2% | 90.0% / 83.4% | 71% / 58% | 88% / 78% | 0.03 | 552 | 169 / 188 |
| moonshine2-base | 142 | 6.9% | 19.4% | 92.3% / 87.6% | 74% / 57% | 92% / 86% | 0.05 | 1162 | 299 / 306 |
| whisper-tiny.en | 104 | 7.6% | 13.9% | 93.3% / 87.8% | 76% / 62% | 90% / 83% | 0.11 | 715 | 275 / 337 |
| whisper-base.en | 161 | 7.9% | 10.5% | 93.2% / 90.7% | 78% / 71% | 89% / 88% | 0.20 | 1262 | 383 / 476 |
| parakeet-tdt-v2 | 662 | 3.1% | 5.8% | 97.0% / 94.4% | 87% / 80% | 95% / 92% | 0.16 | 2816 | 846 / 892 |

WER by category (clean + noisy) was worst for **disfluent** (12–49%) and **short** commands (29–75%):
exactly the rapid, clipped commands a fast player issues mid-movement. Contextual biasing with 40 game
hotwords (score 1.5) changed nothing measurable for the transducer models tried. The latency tables
(first partial, word→partial, release→final, hands-free endpointing) were never completed on this
machine.

## Why typing instead

- Best-case accuracy (Parakeet, 662 MB, 850 MiB RSS) still got 1 command in 8 (clean) to 1 in 5
  (noisy) not exactly right on synthetic speech; the small models got 1 in 4 to 1 in 2 wrong. Typed
  text is exact, and the typo repair covers what isn't.
- Speech has a floor on how fast a command can be issued; typing speed is a trainable skill that
  rewards the player (focus time, cast timer, combo) and composes with movement skill.
- Removes a ~50–650 MB model download, a native addon, a utility process and a microphone permission
  from the shipped build.
