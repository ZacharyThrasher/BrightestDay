# HARDLIGHT — imagination-to-construct sandbox (research prototype)

Speak (or type) an idea — *"a giant boxing glove with rockets on the back, punch that drone"* — and a
ring turns it into a physical construct of solid light that you can use, edit ("now make the fingers grab
him", "spin him around", "throw him into that wall"), and combine with fast momentum movement.

This is an autonomy/R&D experiment, not a product. It is built to be **runnable and shareable** on an
ordinary enthusiast Windows PC (reference: RTX 3080-class GPU), and to produce honest evidence about what
works. See [`docs/EXPERIMENTS.md`](docs/EXPERIMENTS.md) for all measurements (including failures) and
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for how it fits together.

## Play (Windows)

1. Download `HARDLIGHT-win32-x64.zip`, unzip anywhere, run `HARDLIGHT.exe`. No install, no internet needed.
2. Optional: copy `settings.example.json` to `settings.json` next to the exe to change mouse sensitivity,
   FOV, graphics quality, voice, or to enable a stronger "System 2" interpreter (your own Anthropic API
   key, or a local Ollama / LM Studio server).

## Controls

| | |
|---|---|
| mouse | look / aim (the crosshair is what "that", "there", "him" refer to) |
| W A S D | move — velocity is momentum: strafe in the air to gain speed, jump as you land to keep it |
| Space | jump (hold while flying = up) |
| Shift | hard-light thruster boost (uses thrust meter) |
| Ctrl / C | slide at speed (flying: down) |
| Q / E (or mouse buttons) | left / right tether — hold to stay attached, release to keep your swing speed |
| R / mouse wheel | reel in while tethered |
| F | toggle flight (easy, but slower than momentum tricks) |
| **V (hold)** | **speak to the ring** (push-to-talk) |
| **Enter** | type to the ring (same interpreter; live ghost preview while you type) |
| X / Z | dismiss focused construct / dismiss all |
| F1 / F3 | help / performance overlay |

Things to try: *"ramp"* while running · *"launch me up there"* at a rooftop · *"a zipline to that
building"* · *"wings"* then dive · *"grab that drone, spin him around, throw him into that wall"* ·
*"a mirror that reflects the shots back"* at the turret · *"a trampoline under that guy"* ·
*"three hammers orbiting me"* · *"bigger"* · *"make five of them"* · *"save this as thumper"* then
*"thumper!"* · *"ten thousand planet-sized anvils"* (watch the clamp).

## Build from source

```
npm install
npm test                 # movement + construct scenario tests
npm run eval -- eval/dev.jsonl             # interpreter on the dev set
npx tsx eval/diversity.ts                  # world-state-judged solution diversity
npm run start            # Electron (dev)
npm run web              # same game in a browser (text input only)
node tools/package-win.mjs                 # Windows x64 zip (from Linux or Windows)
```

Models (speech, word vectors) are not in git; `tools/asr-bench/download-models.mjs` and
`tools/semantic/download.mjs` fetch them by exact URL + sha256.

## Licenses

Code: see repository license. Bundled third-party components and data: [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)
(note: the word-vector file and English word list are CC BY-SA 4.0).
