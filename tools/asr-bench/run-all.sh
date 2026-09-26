#!/usr/bin/env bash
# Re-runs the full benchmark matrix sequentially (timing runs must not overlap).
# Prereqs: npm install; node download-models.mjs; node gen-audio.mjs; node word-times.mjs
set -u
cd "$(dirname "$0")"
PRESETS="zipformer-20M zipformer-0621 kroko-en nemo-fc-80ms nemo-fc-480ms moonshine-tiny moonshine-base moonshine2-tiny moonshine2-base whisper-tiny.en whisper-base.en parakeet-tdt-v2 two-pass"
LOG=data/results/run-all.log
mkdir -p data/results
run() { echo "== $(date -u +%T) node bench.mjs $*" | tee -a "$LOG"; node bench.mjs "$@" 2>&1 | grep -v '^/shared' | tee -a "$LOG"; }

# 1) accuracy: all voices, clean + noisy, 2 threads
for p in $PRESETS; do run --preset "$p" --mode accuracy --threads 2; done
# 2) contextual biasing (hotwords need modified_beam_search; compare MBS without hotwords too)
for p in zipformer-20M zipformer-0621 kroko-en parakeet-tdt-v2; do
  run --preset "$p" --mode accuracy --threads 2 --decoding modified_beam_search
  run --preset "$p" --mode accuracy --threads 2 --hotwords
done
# 3) latency simulation (kok-adam + libritts voices, clean), 1 and 2 threads
for t in 1 2; do for p in $PRESETS; do run --preset "$p" --mode latency --threads "$t"; done; done
# 4) repeatability: second latency pass at 2 threads + noisy latency
for p in $PRESETS; do run --preset "$p" --mode latency --threads 2 --tag rep2; done
for p in zipformer-20M nemo-fc-80ms moonshine2-base whisper-base.en; do run --preset "$p" --mode latency --threads 2 --cond noisy --tag noisy; done
echo "== done $(date -u +%T)" | tee -a "$LOG"
