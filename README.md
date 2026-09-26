# HARDLIGHT — imagination-to-construct sandbox (research prototype)

Type an idea — *"a giant boxing glove with rockets on the back, punch that drone"* — and a ring turns it
into a physical construct of solid light that you can use, edit ("now make the fingers grab him", "spin
him around", "throw him into that wall"), and combine with fast momentum movement. Typing is part of the
skill: you keep flying/swinging and aiming while you type, and you play as fast as you can type. The ring is as strong as your imagination: new ideas and new combinations make it
stronger and cheaper to use; repeating yourself goes stale.

This is an autonomy/R&D experiment, not a product. It is built to be **runnable and shareable** on an
ordinary enthusiast Windows PC (reference: RTX 3080-class GPU), and to produce honest evidence about what
works. See [`docs/EXPERIMENTS.md`](docs/EXPERIMENTS.md) for all measurements (including failures) and
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for how it fits together.

## Play (Windows)

1. Download `HARDLIGHT-win32-x64.zip`, unzip anywhere, run `HARDLIGHT.exe`. No install, no internet needed.
2. Optional: copy `settings.example.json` to `settings.json` next to the exe to change mouse sensitivity,
   FOV, graphics quality, focus time, or to enable a stronger "System 2" interpreter (your own Anthropic
   API key, or a local Ollama / LM Studio server).

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
| **Enter** | open the ring console and type (live ghost preview). You keep your momentum, held tethers and mouse aim; only the movement keys pause |
| Enter / Shift+Enter | form it (and close) / form it and keep typing (chain casts) |
| empty Enter | recast the last command |
| Tab / → | accept the faint word completion (Tab again cycles) |
| ↑ / ↓ | command history |
| 1 – 9 | quick-cast a saved design or a recent command (bar above the key hints) |
| X / Z | dismiss focused construct / dismiss all |
| Alt+1 – 9 | switch scene (or use the menu) |

Focus time (the world slows while you type, until a 5 s meter runs dry) exists as an **offline practice**
option only (menu → Focus time, or `focusTimeScale` in settings.json); it is off by default and never
applies in online duels, where both players share one clock.
| F1 / F3 | help / performance overlay |

**Inspiration.** Every cast is scored for novelty: what it is, its add-on parts, materials, size, count,
behaviours, what it acts on, the verbs you used — and above all new *combinations* of those. Novel casts
fill your INSPIRATION bar (willpower cap and regen, construct speed/force, lifetime, lower cost); repeating
the same idea goes stale (weaker, more expensive, and the HUD says so). Each player has their own meter.
Hurting or taking down your rival, breaking their constructs or blocking their shots with something
unusual for the job (a dropped piano rather than a cannon) pays a bonus. Quick
successive casts build a combo that amplifies inspiration gains. The validator's hard limits always hold.

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

Models (word vectors) are not in git; `tools/semantic/download.mjs` fetches them by exact URL + sha256.
Voice input was measured and dropped by design — see [`docs/experiments/ASR.md`](docs/experiments/ASR.md).

## Licenses

Code: see repository license. Bundled third-party components and data: [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)
(note: the word-vector file and English word list are CC BY-SA 4.0).
