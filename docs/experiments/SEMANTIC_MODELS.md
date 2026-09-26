# Semantic helpers: what is obtainable offline, and how fast

Status: research experiment, 2026-09-26. Everything below was run in the dev container (4 vCPU Intel
Xeon Sapphire Rapids VM, no GPU, Node 22.22, Python 3.11). Nothing here touches `src/sim`,
`src/lang`, `src/render` or `eval/`.

**TL;DR**

1. **System-1 fallback (unknown noun phrase -> nearest known concept): solved and shipped as a prototype.**
   ConceptNet **Numberbatch 19.08** static vectors are clearly the best candidate: **78% top-1 / 87% top-3**
   on a 120-phrase labelled set, versus 61/75% for the best MIT-licensed option (spaCy `en_core_web_lg`)
   and 49-57% top-1 for the small sentence encoders (bge-small, MiniLM, USE-lite). The pruned artifact is
   **27.2 MB** (127.5k terms incl. ~20k multi-word object names like `pogo_stick`, 200-d int8), loads in
   **~90 ms**, and a query against 100 concepts / 150 aliases takes **~0.1-0.2 ms** in plain JS
   (`src/semantic/embed.mjs`, zero deps, Node + browser). Caveat: Numberbatch is **CC BY-SA 4.0**
   (attribution + share-alike on the vector file); a 15.5 MB MIT fallback artifact (spaCy-lg) is also built.
2. **System-2 small LLM: obtainable, but the 0.5-1.5B models are not good enough zero-shot.**
   huggingface.co is blocked, but llama.cpp release binaries (GitHub release assets) and
   **Qwen2.5-0.5B/1.5B-Instruct Q4_K_M GGUFs** (third-party GitHub release mirrors, byte-identical across
   4 independent mirrors, Apache-2.0) were obtained. On 108 typed decisions (12 utterances x 9 questions)
   option-logit scoring gave **57% (1.5B)** and **34% (0.5B)** accuracy -- *below the 65% majority-class
   baseline*; 5-shot prompting did not help (0.565 / 0.444). Constrained JSON was always valid but semantically weak.
   Recommendation: keep the deterministic parser as System-2's backbone; treat an LLM as an optional
   component served by a player-installed 3-8B model (Ollama / LM Studio / llama-server, OpenAI-compatible)
   or revisit once a >=3B model can be fetched (needs huggingface.co allowed) or fine-tuned.

---

## 1. System-1: unknown phrase -> nearest concept

### 1.1 Method

* **Concept list** (`tools/semantic/concepts.json`): 100 stand-in concepts for a hard-light construct game
  (ramp, wall, shield, fist, glove, hammer, sword, bat, spear, cage, net, bubble, spring, trampoline,
  catapult, cannon, launcher, rope, chain, hook, wheel, car, board, wing, glider, parachute, rocket,
  jetpack, fan, magnet, vacuum, saw, drill, gear, bridge, stairs, platform, pillar, anvil, boulder, ball,
  dome, umbrella, ladder, slide, tube, funnel, conveyor, carousel, pendulum, wrecking_ball, ... plus
  distractors like table, bell, mirror, key). Each concept has 1-3 generic aliases (150 alias strings).
  This is *not* the lead's lexicon -- swap in the real one.
* **Test set** (`tools/semantic/testset.json`): 120 unknown-object phrases, each with the 1-4 concept ids a
  designer would accept (first = best), e.g. `hoverboard -> board|platform`, `pogo stick -> spring|pole`,
  `baggage carousel -> conveyor|carousel`, `flyswatter -> paddle|racket|net`, `battering ram ->
  pillar|hammer|piston`. No test phrase is a concept alias (checked). Single annotator; treat the numbers
  as relative, +-4 points (1 item = 0.8 pt).
* **Scoring**: concept score = max cosine over its aliases; top-1 = best concept is acceptable; top-3 = any
  of the best three is acceptable. Static vectors embed a phrase as the mean of (unit-normalised) word
  vectors, with whole-phrase lookup first where the table has multi-word entries.
* Scripts (raw outputs in `tools/semantic/results/`): `tools/semantic/bench_embed.py` (all methods, Python), `tools/semantic/bench_use.mjs` (USE-lite
  in TF.js), `tools/semantic/eval_artifact.mjs` (the shipped artifact through the shipped JS module).

### 1.2 Results -- full source tables

| Method | top-1 | top-3 | query latency* | download | license |
|---|---:|---:|---:|---:|---|
| **ConceptNet Numberbatch 19.08 (en), mean of words** | **0.792** | **0.892** | 0.1 ms | 325 MB gz (516k terms x 300d) | CC BY-SA 4.0 |
| Numberbatch 19.08, whole-phrase lookup then mean | 0.783 | 0.892 | 0.1 ms | same | CC BY-SA 4.0 |
| Numberbatch 19.08, phrase + head-noun x2 | 0.783 | 0.892 | 0.1 ms | same | CC BY-SA 4.0 |
| spaCy `en_core_web_lg` 3.8.0 vectors, mean + head x2 | 0.642 | 0.758 | 0.2 ms | 401 MB whl (514k x 300d) | MIT |
| spaCy `en_core_web_lg` 3.8.0 vectors, mean | 0.625 | 0.775 | 0.2 ms | same | MIT |
| WordNet 3.0 Wu-Palmer (max over synsets, head-noun fallback) | 0.608 | 0.733 | 107 ms (NLTK) | 10.8 MB | WordNet (BSD-like) |
| all-MiniLM-L6-v2 (ONNX, fastembed export) | 0.567 | 0.717 | 1.4-4.9 ms (onnxruntime) | 83 MB tgz | Apache-2.0 |
| bge-small-en-v1.5 (ONNX, "a X" prompt) | 0.542 | 0.667 | 2.3-9 ms | 77 MB tgz | MIT |
| GloVe 6B 300d (Wikipedia+Gigaword) | 0.517 | 0.683 | 0.1 ms | 394 MB gz (400k x 300d) | PDDL 1.0 |
| bge-small-en-v1.5 (ONNX, raw) | 0.492 | 0.683 | 1.9-18 ms | 77 MB tgz | MIT |
| Universal Sentence Encoder lite (TF.js, 8k SentencePiece) | 0.350 | 0.483 | 58 ms (tfjs pure-JS CPU) | 28 MB | Apache-2.0 |
| spaCy `en_core_web_md` 3.8.0 vectors (20k unique vectors) | 0.208 | 0.375 | 0.1 ms | 33 MB whl | MIT |

\* Python/numpy or onnxruntime per query incl. scoring ~150 alias vectors, this VM. Encoder latencies are
per phrase (alias embeddings computed once); ranges are two runs on this noisy VM (4 onnxruntime threads
suffer from the vCPU oversubscription described in 3.1). Raw per-item results: `tools/semantic/results/`.

Observations
* Numberbatch wins because it is retrofitted onto ConceptNet's IsA / UsedFor / PartOf graph, which is
  exactly the relation "an X is a kind of / works like a Y" the game needs, and it contains 100k+ multi-word
  terms (`pogo_stick`, `baggage_carousel`, `shopping_cart`, `wrecking_ball`, `hoverboard` are all present).
* Distributional vectors (GloVe, spaCy) capture topical relatedness (`chainsaw ~ lumber`) more than
  functional similarity. spaCy-md is poor because its 685k keys share only 20k vectors.
* Small sentence encoders are tuned for sentence-level paraphrase/retrieval; on 1-3 word object names they
  underperform good static vectors and cost ~10x more latency plus a 30-90 MB ONNX runtime + model.
* WordNet hypernym similarity is competitive on top-1 but has 8/120 OOV items, is slow in NLTK, and is
  noisy with polysemy (`bat` the animal); useful as a *second opinion* or to build lexicon entries offline.
* Remaining Numberbatch errors are mostly defensible (`kayak -> paddle`, `wingsuit -> parachute`,
  `chute -> parachute`, `helicopter -> glider`) or reveal missing concepts (`punching bag -> fist`).

### 1.3 Shipped artifact and module

Built by `python3 tools/semantic/build_artifact.py` (defaults: Numberbatch, 100k words, SVD -> 200d):

* Vocabulary (independent of the test set): the 100k most frequent alphabetic words (GloVe 6B frequency
  order) present in Numberbatch + 7.6k extra single-word WordNet nouns under `object.n.01` (rare object
  names) + 19.9k multi-word WordNet object nouns (`pogo_stick`, `grappling_hook`, ...) + concept aliases +
  `tools/semantic/extra_terms.txt` (game words; 10 found) = **127,517 terms**.
* Rows L2-normalised, uncentred truncated SVD 300 -> 200 dims (84% energy), int8 with per-row float32
  scale (max abs error 0.003). File format "BDSV" v1 is documented in `build_artifact.py`.

| Artifact | size | terms x dims | top-1 | top-3 | no-vector items | load | first query (index build) | query mean |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `bd-semantic-nb19-int8.bin` (default) | 27.2 MB | 127,517 x 200 | **0.783** | **0.867** | 2 | ~90 ms | ~10 ms | 0.12-0.17 ms |
| `bd-semantic-spacylg-int8.bin` (MIT fallback) | 15.5 MB | 49,543 x 300 | 0.608 | 0.750 | 7 | ~40 ms | ~15 ms | 0.12-0.17 ms |

(Measured by `node tools/semantic/eval_artifact.mjs` through `src/semantic/embed.mjs`, Node 22, this VM.)
Pruning ablation on Numberbatch: 74.7k terms x 300d (23.5 MB) 0.750/0.842; 127.5k x 300d (40 MB)
0.783/0.875; 127.5k x 200d (27.2 MB) 0.783/0.867; 156k x 160d (27.1 MB) 0.783/0.858. Head-noun weighting
x2 did not help on the artifact (0.758/0.867).

Artifact sha256 (rebuilds are deterministic for a given numpy/LAPACK; SVD signs may differ on other
platforms, which changes the bytes but not the similarities):

| file | size (bytes) | sha256 |
|---|---:|---|
| `models/semantic/bd-semantic-nb19-int8.bin` | 27,232,380 | `aac0896909f8ac6cc2aa3f193b0b1be7914d43790429cc0587e0b8396f20cee5` |
| `models/semantic/bd-semantic-spacylg-int8.bin` | 15,471,332 | `d2e97d0435c4668b7d77858d49764b79b6f9764770f286aceb5e3e63aee37405` |

API (`src/semantic/embed.mjs`, plain ESM, no dependencies, no native code):

```js
import { loadEmbedder } from '../semantic/embed.mjs';
const emb = await loadEmbedder('models/semantic/bd-semantic-nb19-int8.bin'); // path | file:/http(s): URL | ArrayBuffer | Uint8Array
emb.nearest('baggage carousel', concepts /* [{id, text: string|string[]}] */, 3); // -> [{id:'cart',score:.51},{id:'conveyor',score:.49},...]
emb.embedPhrase('pogo stick');   // Float32Array(200), unit length, or null if no word is known
emb.has('wrecking ball');        // true (multi-word entry)
emb.segments('giant boxing glove with rockets'); // [{term:'giant'},{term:'boxing_glove'},{term:'rockets'}]
emb.similarity('mallet', 'hammer');
```

* Phrase handling: lowercase, strip punctuation, drop function words, whole-phrase lookup, then greedy
  longest-match of up to 4-word vocabulary terms, then naive singular/plural fallback; mean of segments.
* `nearest` pre-embeds the candidate array once (cached per array object via WeakMap), then is a dot
  product per alias. Returns `[]` for fully unknown input, so callers can fall through to "ask the player".
* In Node it reads paths / `file:` URLs with `fs`; everywhere else it uses `fetch`. Bundles cleanly with
  esbuild `--platform=browser` (6 kB). Test: `node --test src/semantic/embed.test.mjs` (synthetic artifact
  always; real artifact when built).

Suggested integration: use the score as a confidence; e.g. accept top-1 when `score >= ~0.35` and
`top1 - top2 >= ~0.05`, otherwise offer the top-3 to the player or fall back to a generic construct.
(Thresholds not tuned -- derive them from the real lexicon.) Add game words (hoverboard, lightsaber...)
to `extra_terms.txt` if they fall outside the frequency cut.

## 2. What was obtainable (network probe results)

Reachable and used: GitHub release **assets** (github.com/.../releases/download/...),
raw.githubusercontent.com, storage.googleapis.com, conceptnet.s3.amazonaws.com, registry.npmjs.org,
pypi.org / files.pythonhosted.org. The GitHub MCP code search (not api.github.com for arbitrary repos) was
used to discover exact asset URLs.

Blocked / unreachable (HTTP 403 from the egress proxy or connection refused): huggingface.co, hf-mirror,
modelscope.cn, registry.ollama.ai, ollama.com, cdn.jsdelivr.net, unpkg.com, download.pytorch.org,
gpt4all.io, kaggle.com, archive.org, zenodo.org, gitlab.com, nlp.stanford.edu (GloVe original),
dl.fbaipublicfiles.com (fastText), sourceforge (403). api.github.com for ggml-org/llama.cpp is not
configured for this session. HF CDN hostnames were deliberately **not** tried (that would route around the
huggingface.co policy).

### 2.1 Sources used (all fetched by `node tools/semantic/download.mjs`, hashes in `tools/semantic/sources.lock.json`)

| id | URL | bytes | sha256 | license |
|---|---|---:|---|---|
| spacy-md | https://github.com/explosion/spacy-models/releases/download/en_core_web_md-3.8.0/en_core_web_md-3.8.0-py3-none-any.whl | 33,480,380 | `5e6329fe3fecedb1d1a02c3ea2172ee0fede6cea6e4aefb6a02d832dba78a310` | MIT |
| spacy-lg | https://github.com/explosion/spacy-models/releases/download/en_core_web_lg-3.8.0/en_core_web_lg-3.8.0-py3-none-any.whl | 400,658,291 | `293e9547a655b25499198ab15a525b05b9407a75f10255e405e8c3854329ab63` | MIT |
| glove-300 | https://github.com/RaRe-Technologies/gensim-data/releases/download/glove-wiki-gigaword-300/glove-wiki-gigaword-300.gz | 394,362,229 | `0a7aebbe49097dc6e5ffff7a25e9aa20181a6e862050ca68bd2f36e056739e00` | PDDL 1.0 |
| numberbatch-en-19.08 | https://conceptnet.s3.amazonaws.com/downloads/2019/numberbatch/numberbatch-en-19.08.txt.gz | 325,403,502 | `90e57611eb71077ada9fd0b011fd0206de8ec13d035aced40b81b1a4d549c6a2` | CC BY-SA 4.0 |
| wordnet | https://raw.githubusercontent.com/nltk/nltk_data/gh-pages/packages/corpora/wordnet.zip | 10,775,600 | `cbda5ea6eef7f36a97a43d4a75f85e07fccbb4f23657d27b4ccbc93e2646ab59` | WordNet 3.0 license |
| use-lite (model.json + vocab.json + 7 shards) | https://storage.googleapis.com/tfjs-models/savedmodel/universal_sentence_encoder/ | 27.8 MB | per file in lock | Apache-2.0 |
| bge-small-en-v1.5 ONNX | https://storage.googleapis.com/qdrant-fastembed/fast-bge-small-en-v1.5.tar.gz | 76,692,531 | `3858004b3822f64f940280874b8f2d2dc25b34a4f3eb3cdf617bdceeb21ed9ed` | MIT |
| all-MiniLM-L6-v2 ONNX | https://storage.googleapis.com/qdrant-fastembed/sentence-transformers-all-MiniLM-L6-v2.tar.gz | 83,180,180 | `2735afe656e156af64ed603dbb1c96f3cae7f937286a8feb27fff7fa979f6a77` | Apache-2.0 |
| llama.cpp b10951 (linux x64 CPU) | https://github.com/ggml-org/llama.cpp/releases/download/b10951/llama-b10951-bin-ubuntu-x64.tar.gz | 16,821,155 | `cb0e453d8d88b62a8962c5b13e46c117315f7740017f2636bcf0de20c6253f7a` | MIT |
| Qwen2.5-0.5B-Instruct Q4_K_M | https://github.com/Hatk0/offline-models/releases/download/models-v1/qwen2.5-0.5b-instruct-q4_k_m.gguf | 491,400,032 | `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db` | Apache-2.0 |
| Qwen2.5-1.5B-Instruct Q4_K_M | https://github.com/Hatk0/offline-models/releases/download/models-v1/qwen2.5-1.5b-instruct-q4_k_m.gguf | 1,117,320,736 | `6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e` | Apache-2.0 |

Also confirmed present (not all benchmarked):
* llama.cpp b10951 Windows builds: `llama-b10951-bin-win-cpu-x64.zip` (18.4 MB),
  `llama-b10951-bin-win-cuda-12.4-x64.zip` (254 MB) -- same release, asset URLs return 200.
* npm `node-llama-cpp@3.21.1` (MIT) with prebuilt binaries as optional deps: `@node-llama-cpp/win-x64`
  (48.6 MB unpacked), `win-x64-cuda` (175.5 MB), `win-x64-vulkan` (103 MB), `linux-x64` (29.6 MB) --
  an in-process Electron-main runtime option (not benchmarked; the llama-server binary was used instead).
* PyPI `llama-cpp-python` 0.3.35 is sdist-only (74.9 MB, needs a C++ build).
* Model weights bundled in registries: npm `slaunt-model-tinyllama-1b-part1/2` (TinyLlama-1.1B-Chat
  **IQ2_XXS**, 2 x 161 MB, Apache-2.0 -- 2-bit, too degraded to be useful); PyPI
  `gemma3-270m-q4-k-m-gguf-part1..4` (Gemma 3 270M Q4_K_M, ~230 MB, Gemma terms); PyPI `llm-smollm2`
  (SmolLM2-135M-Instruct, 93 MB). All below the 0.5B floor or unsuitable.
* storage.googleapis.com/mediapipe-models and /mediapipe-assets (publicly listable): **EmbeddingGemma-300M**
  `text_embedder/embedding_gemma/int4int8/1/embedding_gemma.task` (184 MB, Gemma terms, MediaPipe
  TextEmbedder runtime), `gecko.task` (113 MB), `universal_sentence_encoder.tflite` (6 MB),
  `bert_embedder.tflite` (26 MB), task-specific Gemma-200M `.litertlm` proofreader/summariser (118 MB), and
  `deepseek_q8_ekv1280.task` (1.86 GB, DeepSeek-R1-Distill-Qwen-1.5B for MediaPipe LLM Inference,
  WebGPU-only in the browser/Electron renderer). None were benchmarked here (EmbeddingGemma is the most
  promising neural embedder if a neural fallback is ever wanted; the R1 distill is a reasoning model and
  cannot expose logits through the MediaPipe API).
* Other third-party GitHub release mirrors found by code search (not downloaded):
  `granite-3.1-1b-a400m-instruct-Q8_0.gguf` (leok7v/gyptix, 1.42 GB), `qwen2.5-0.5b-instruct-q4_0.gguf`
  (random-unknown-username/xdna.cpp, 429 MB).

Provenance of the Qwen GGUFs: huggingface.co is blocked, so the official SHA-256 could not be checked
directly. The 1.5B file has the same size (1,117,320,736 B) at four unrelated GitHub mirrors
(Hatk0/offline-models, ImSurajx/rythem-app, Erncm/kisisel-asistan-android, razvan-eduard/VoxApps) and three
200 kB windows (offsets 0, 500 MB, 1.117 GB) hash identically against a second mirror; the sha256 matches
the one pinned in the Hatk0 manifest; GGUF metadata loads as `qwen2 1.5B Q4_K - Medium`. This is strong
but not cryptographic evidence it equals `Qwen/Qwen2.5-1.5B-Instruct-GGUF/qwen2.5-1.5b-instruct-q4_k_m.gguf`.
Re-verify against huggingface.co before shipping anything.

## 3. System-2: small local LLM

### 3.1 Method (`tools/semantic/bench_llm.mjs`)

Runtime: llama.cpp b10951 `llama-server` (CPU build), Qwen chat template, temperature 0, KV prompt cache.

* **(a) Option-logit scoring** (the SemIf/OpenJev idea): 12 utterances x 9 typed questions = 108 decisions
  (intent create/modify/act/control; target self/aimed/previous/none; compound yes/no;
  size small/normal/large; attach yes/no; count one/several/zero; negation yes/no;
  motion none/spin/launch/swing/punch; surface normal/bouncy/slippery/sticky), hand-labelled gold (some
  items accept two answers). The shared prefix (system prompt + command) is cached; each question costs one
  forward pass over ~20 new tokens + 1 decode step, reading `n_probs=100` of the raw next-token
  distribution; an option's score is the summed probability of its first-token variants
  (`x`, ` x`, `X`, ` X`; no first-token collisions). No text is generated.
  Variants: *calibrated* (divide by probs for a content-free "N/A" command -- contextual calibration) and
  *few-shot* (5 worked examples, disjoint from the test utterances, in the cached system prompt).
* **(b) Constrained JSON**: `{intent, object, size, target}` via llama-server `json_schema` (-> GBNF).

CPU note: this VM's 4 vCPUs are oversubscribed on the host. llama-bench for the 1.5B model gave pp64 =
122 t/s / tg32 = 10.9 t/s with 2 threads but collapsed to 16-38 t/s (tg 2.5 t/s) with 3-4 threads
(spin-barrier threads being descheduled). All numbers below use **2 threads**; they are pessimistic for a
real 8-core desktop.

### 3.2 Results

| Model (Q4_K_M) | prompt | option acc. (raw) | calibrated | per-question latency p50 / p95 | JSON valid | JSON latency p50 | gen speed |
|---|---|---:|---:|---:|---:|---:|---:|
| Qwen2.5-1.5B-Instruct | zero-shot | **0.574** | 0.352 | 366 / 862 ms | 12/12 | 4.2 s (32 tok) | 8.4 tok/s |
| Qwen2.5-0.5B-Instruct | zero-shot | 0.343 | 0.380 | 159 / 459 ms | 12/12 | 1.3 s (26 tok) | 27 tok/s |
| Qwen2.5-1.5B-Instruct | 5-shot | 0.565 | 0.537 | 337 / 747 ms | 12/12 | 3.9 s (33 tok) | ~8 tok/s |
| Qwen2.5-0.5B-Instruct | 5-shot | 0.444 | 0.380 | 217 / 503 ms | 12/12 | 1.4 s (26 tok) | ~27 tok/s |
| majority-class baseline | -- | 0.648 | | | | | |

Few-shot helped `intent` (7 -> 10/12) and `surface` (5 -> 9/12) for the 1.5B model but hurt `negation`
(12 -> 6/12) and `attach` (9 -> 4/12); net accuracy unchanged (0.565). Per-question (1.5B zero-shot, raw): negation 12/12, attach 9/12, compound 8/12, intent 7/12, count 7/12,
target 5/12, motion 5/12, surface 5/12, size 4/12. Failure modes: strong option-prior bias (`surface ->
sticky` for almost everything, `target -> aimed`, `count -> several`), `act` vs `create` confusion when a
command both creates and uses an object. JSON outputs were schema-valid but the `object` field often
leaked the system prompt ("hard-light") and `target` was almost always `aimed`. Contextual calibration with
an "N/A" anchor over-corrected and hurt the 1.5B model -- a negative result.

### 3.3 Extrapolation to the reference PC (RTX 3080, 8-core CPU) -- NOT MEASURED

Assumptions (public llama.cpp CUDA numbers of this class, not verified here): a 1.5B Q4_K_M model on an
RTX 3080 does prompt processing at several thousand tokens/s and generation at roughly 200-350 tokens/s
(memory-bandwidth bound: 760 GB/s over a 1.04 GiB model gives a ~700 t/s ceiling); a 7-8B Q4_K_M model
~80-120 t/s generation.

* Option scoring, 1.5B: ~20 new tokens + 1 step per question, dominated by kernel launch/HTTP overhead:
  **~5-15 ms per question**, ~50-130 ms for 9 sequential questions; ~15-30 ms if the 9 questions are
  submitted as parallel sequences sharing the cached prefix (`-np 9`, or batch in node-llama-cpp).
* Constrained JSON, 1.5B, ~30 tokens: **~100-200 ms**; 7-8B: ~0.3-0.5 s.
* 8-core CPU only (no GPU offload): roughly 2-4x this VM's 2-thread numbers, i.e. ~100-200 ms per option
  question and 1-2 s per JSON for 1.5B -- too slow for per-utterance use while the game also needs the CPU.

### 3.4 How the shipped game could use an LLM anyway

* **Bring-your-own local server** (recommended if an LLM is wanted): the game talks to any
  OpenAI-compatible endpoint on the player's PC -- Ollama (`http://127.0.0.1:11434/v1`), LM Studio
  (`http://127.0.0.1:1234/v1`) or `llama-server` (`/v1/chat/completions`, plus the native `/completion`
  with `n_probs` for option scoring and `json_schema`/`grammar` for constrained output). The player
  chooses the model (e.g. Qwen2.5-7B / Llama-3.1-8B on a 3080), the game ships no weights and no licence
  burden, and the feature degrades gracefully when no server answers. Option scoring needs logprobs:
  llama-server `/completion` (`n_probs`, `post_sampling_probs:false`) and the OpenAI-compatible
  `logprobs/top_logprobs` fields in Ollama/LM Studio/llama-server all expose them.
* **Bundled runtime**: llama.cpp release zips (18 MB CPU, 254 MB CUDA 12.4 for Windows) or
  `node-llama-cpp` prebuilt binaries are obtainable from GitHub/npm today; weights are the problem. Needed
  from the user/IT: allow `huggingface.co` (+ its CDN) to fetch official GGUFs with verifiable hashes (e.g.
  `Qwen/Qwen2.5-3B-Instruct-GGUF`, `Qwen/Qwen3-4B-GGUF` -- official sha256 for Qwen3 files are pinned in
  third-party manifests such as ShaftHQ/SHAFT_ENGINE `tools/local-ai-poc/manifest.json`), or accept the
  GitHub-mirror provenance risk. Check licences: Qwen2.5 0.5/1.5/7B = Apache-2.0, **Qwen2.5-3B = Qwen
  Research licence (non-commercial)**, Qwen3 = Apache-2.0, Gemma = Gemma terms, Llama 3.x = Llama licence.

## 4. Recommendation

**System-1: adopt Numberbatch static vectors via `src/semantic/embed.mjs`.**

* Pros: best accuracy by a wide margin (78/87%), sub-millisecond, 27 MB, zero dependencies, identical in
  Node and renderer, fully offline, deterministic, trivially explainable (`segments()` shows what matched).
* Cons / risks: **CC BY-SA 4.0** -- the game must credit ConceptNet Numberbatch and distribute the
  derived vector file under CC BY-SA 4.0 (it is a separate data file, so code licensing is unaffected, but
  legal should sign off). Vocabulary is frozen at 2019 and at the frequency cut (e.g. `hoverboard` is in
  Numberbatch but not in the pruned artifact -> add to `extra_terms.txt`). Similarity is not identity:
  keep a confidence threshold and a "did you mean" path. The test set is small and single-annotator.
* Fallback if CC BY-SA is unacceptable: the MIT spaCy-lg artifact (61/75%) -- or rebuild with
  `--source glove` (PDDL, ~48/63%). A later upgrade path is a distilled static table from a neural
  embedder (e.g. EmbeddingGemma from the MediaPipe bucket, Gemma terms) -- only worth it if it beats
  Numberbatch on the real lexicon.

**System-2: do not ship a 0.5-1.5B model as a decision-maker.** Zero-shot they are worse than predicting
the most common label, and 5-shot prompting did not change that (1.5B: 0.565; 0.5B: 0.444). The option-logit machinery works and is
cheap (one forward pass per question, prefix cached), so the *infrastructure* is worth keeping for an
optional "smart mode" backed by a player-run OpenAI-compatible server with a 7-8B model, or for a
fine-tuned small model later (fine-tuning a 0.5-1.5B model on a few thousand synthetic labelled commands is
the realistic route to <20 ms, reliable typed decisions on a 3080).

## 5. Reproduce

```sh
node tools/semantic/download.mjs                    # all sources (~3 GB); --group embed | --group llm
node tools/semantic/download.mjs --verify           # re-hash existing files
cd models/semantic/raw && mkdir -p x nltk_data/corpora && tar xzf fast-bge-small-en-v1.5.tar.gz -C x \
  && tar xzf sentence-transformers-all-MiniLM-L6-v2.tar.gz -C x && tar xzf llama-b10951-bin-ubuntu-x64.tar.gz -C x \
  && (cd nltk_data/corpora && unzip -qo ../../wordnet.zip) && find x -name '._*' -delete && cd -
pip install numpy onnxruntime tokenizers nltk spacy models/semantic/raw/en_core_web_*-3.8.0-py3-none-any.whl
(cd tools/semantic && npm install)                  # bench-only: tfjs + USE tokenizer
node tools/semantic/bench_use.mjs                   # USE-lite embeddings -> cache
python3 tools/semantic/bench_embed.py               # table 1.2 (caches parsed tables as .npy)
python3 tools/semantic/build_artifact.py            # -> models/semantic/bd-semantic-nb19-int8.bin + manifest
python3 tools/semantic/build_artifact.py --source spacy-lg
node tools/semantic/eval_artifact.mjs               # table 1.3
node --test src/semantic/embed.test.mjs
node tools/semantic/bench_llm.mjs --model qwen2.5-1.5b-instruct-q4_k_m.gguf [--fewshot] [--threads 2]
```
