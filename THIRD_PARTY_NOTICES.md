# Third-party notices

HARDLIGHT (research prototype) bundles or downloads the following. Licenses as stated by upstream at the
time of the build; verify before any redistribution beyond research/demo use.

| component | use | license |
|---|---|---|
| [Electron](https://www.electronjs.org/) | desktop shell | MIT (+ Chromium licenses, see `LICENSES.chromium.html` in the build) |
| [three.js](https://threejs.org/) | rendering | MIT |
| [Rapier](https://rapier.rs/) (`@dimforge/rapier3d-compat`) | physics | Apache-2.0 |
| [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (`sherpa-onnx-node`) | local speech recognition runtime | Apache-2.0 |
| speech model (see `docs/experiments/ASR.md` / models manifest) | local speech recognition | per model: see manifest (zipformer: Apache-2.0; NeMo models: CC-BY-4.0; Moonshine: MIT) |
| [Silero VAD](https://github.com/snakers4/silero-vad) | voice activity detection | MIT |
| [ConceptNet Numberbatch 19.08](https://github.com/commonsense/conceptnet-numberbatch) (pruned, int8) | unknown-word → concept fallback | **CC BY-SA 4.0** — © ConceptNet / Luminoso; this derived vector file is shared under the same license |
| [wordfreq](https://github.com/rspeer/wordfreq) (top-40k English word list, `src/lang/english40k.json`) | ASR-repair vocabulary | code Apache-2.0 / MIT; data **CC BY-SA 4.0** (derived from multiple sources, see wordfreq README) |

Not bundled (optional, player-configured): Anthropic API or any OpenAI-compatible local server
(Ollama, LM Studio, llama.cpp server) used as "System 2".

The game's own code, art and identity ("HARDLIGHT") are original; it is not affiliated with any
existing superhero franchise.
