# HARDLIGHT renderer / HUD / input / Electron shell — notes

Owner: renderer agent. Code: `src/render/`, `src/app/`, `src/electron/`, `tools/build.mjs`,
`tools/serve.mjs`, `tools/render-harness.mjs`, `tools/screenshot.mjs`, `tools/perf.mjs`.
Screenshots: `docs/screenshots/*.png` (regenerate with `npm run screenshots`).
Perf: `docs/experiments/RENDER_PERF.md` (regenerate with `npm run perf`).

## Running it

| command | what |
|---|---|
| `npm run build` | esbuild → `dist/` (Electron: `main.cjs`, `preload.cjs`, `renderer.js`, `index.html`, `styles.css`) and `dist/web/` (same renderer, plain browser) |
| `npm start` | build + Electron window (1600×900, F11 fullscreen) |
| `npm run web` | build + static server on http://127.0.0.1:5173 (any Chromium/Firefox with WebGL2) |
| `npm run screenshots` | scripted, deterministic visual QA in headless Chromium/SwiftShader → `docs/screenshots/` |
| `npm run perf` | perf harness → `docs/experiments/RENDER_PERF.md` + `render_perf.json` |
| `npm run smoke:electron` | `xvfb-run electron . --no-sandbox --smoke-test=3000` (exits 0 once the renderer reports ready) |

URL / CLI options: `?scene=gap`, `?graphics=low`, `?fov=90`, `?sens=1.5`, `?color=ff6bd5`, `?test=1`
(scripted mode, sim only advances via `window.__hl.step`). Electron: `--scene=gap`, `--graphics=low`,
`--software-gl` (forces SwiftShader for VMs / blocklisted GPUs), `--smoke-test[=ms]`.

`settings.json` (Electron) is merged from, lowest precedence first: `<userData>/settings.json`,
`settings.json` next to the executable, and (unpackaged dev runs) `./settings.json`:
```json
{ "mouseSensitivity": 1.0, "invertY": false, "fov": 78, "constructColor": "#3dffa8", "graphics": "high" }
```
The in-game menu (shown whenever the pointer is not locked) edits the same settings and saves them
to `<userData>/settings.json` (web build: localStorage).

## Architecture

```
src/app/main.ts      App: boot, fixed 60 Hz accumulator loop (≤6 catch-up ticks, then drop time),
                     console / ring flow, voice hook, menu, window.__hl test hooks
src/app/input.ts     keyboard/mouse → PlayerInput per tick (sub-tick presses latched), pointer-lock look
src/app/hud.ts       DOM HUD (only writes the DOM when a value changes)
src/app/settings.ts  settings + the `window.hardlight` bridge type
src/render/renderer.ts   GameRenderer: owns scene/camera; tick() after every sim step, render(alpha,…)
src/render/post.ts       custom post chain (HDR MSAA target, dual-filter bloom, composite)
src/render/environment.ts  sky (full-screen triangle), sun + hemisphere light, player-following shadow frustum
src/render/statics.ts    merges ALL static geometry into one mesh with barycentric edge attributes
src/render/toon.ts       toon ramp, rim-lit toon materials, the statics material (grid / windows / ink edges)
src/render/hardlight.ts  construct geometry (via partTransform) + hard-light / ghost / edge shaders
src/render/constructs.ts construct views: assembly, interpolation, reform flash, dissolve, ghost previews
src/render/playerRig.ts  procedural hero (3 instanced meshes) + pose blending + arm IK-lite
src/render/entities.ts   drones, turret, civilian, boulder, crate, dummy
src/render/markers.ts    race rings + goal beacons
src/render/fx.ts         particles (1 draw), beams/streaks (1 draw), projectiles (1 draw)
src/electron/main.ts, preload.ts   Electron shell
```

The renderer is a pure consumer of `World` state. It never mutates the sim, with one exception:
to evaluate the canonical `Player.camera()` at the *interpolated* pose and *live* mouse look it
temporarily swaps `player.pos/vel/yaw/pitch`, calls `camera()`, and restores them (see requests).

### Frame timing & interpolation
- Sim: fixed 60 Hz in an accumulator loop. `GameRenderer.tick()` runs after every `World.step()` and
  snapshots prev/curr poses (player, tether tips, entities, construct instances, projectiles) and
  drains `player.events` (which the sim clears at the start of each update).
- Render: every rAF with `alpha = acc / dt`; positions lerp, rotations slerp. Mouse look is applied to
  the camera at display rate (not tick rate) so 120/144 Hz feels smooth; the crosshair stays exact
  because the next tick uses the same yaw/pitch and the camera comes from `Player.camera()`.

### Look
- **Toon world**: `MeshToonMaterial` with a 3-band ramp (35% / 70% / 100%) + hemisphere ambient;
  side-lit sun from the east for readable building faces; PCF shadow map (2048², ±70 m, texel-snapped
  to the player) on *high*.
- **Ink lines, two sources**: (1) baked edges on every static/prop via per-vertex barycentrics with
  internal diagonals disabled (crisp, screen-space-constant width via `fwidth`, zero extra passes);
  (2) silhouettes in the composite pass from the Laplacian of `1/z` (zero on any plane regardless of
  angle, large at depth jumps; only the nearer side is inked, so lines are 1 px, scaled with
  resolution). Energy pixels are excluded via the alpha-mask trick below.
- **Statics**: every box/cylinder/wedge of a scene → one non-indexed triangle soup, one draw call
  (+1 in the shadow pass). World-space patterns in the fragment shader: 4 m / 32 m speed grid on
  the ground (distance-faded), ribbon windows on building walls (LOD-faded), courses on walls.
- **Sky + fog**: sky is one full-screen triangle (gradient, HDR sun disc that blooms, two-tone cel
  clouds on a virtual plane). Fog is applied in post from depth, using the *same* sky gradient in the
  view direction, so distant buildings melt into the horizon colour; height-attenuated.
- **Hard-light constructs**: per construct one merged `InstancedBufferGeometry` (all parts, built with
  `partTransform()` so visuals = colliders) drawn 3× — back faces (inner volume), front faces
  (fresnel rim, scrolling object-space energy lines, shimmer), and `EdgesGeometry` lines (HDR, bloom).
  `count > 1` constructs are GPU-instanced (instance pos + quat attributes), so a construct costs 3
  draw calls regardless of count. Blending is premultiplied-over for colour with the destination
  **alpha forced to 0** — the composite reads scene alpha as a mask (1 = world → fog + ink,
  0 = energy → no fog, no ink) so constructs glow crisply even far away.
  - Assembly (0.34 s): parts grow from their own centres in order of distance from the ring hand
    (`aPart.w` rank), with an ease-out-back overshoot, extra emissive while forming, and a tapering
    beam from the ring hand; instances of an arranged construct stagger.
  - Reform (on `version` bump): rebuild, re-grow from 45%, white flash.
  - Dissolve (`drainRemoved()`): noise-threshold burn with bright edges, parts drift outward, sparks
    and glows shed from each part centre; 0.5 s.
  - Ghost preview: `director.preview(text)` specs are cloned, placed each frame with
    `world.constructs.place(clone)` (span placements set `stretch`, so the geometry key includes the
    rounded stretch), drawn as a pulsing hologram with dashed animated edges.
  - Colour: single constant `HARDLIGHT_COLOR` (`#3dffa8`) in `src/render/config.ts`, overridable by
    `settings.constructColor`; `spec.color` tint names map through `TINTS`.
- **Bloom**: custom dual-filter (Kawase-style) chain, 5 mips on high / 4 on low, threshold 1.12 with
  soft knee + Karis weighting on the prefilter — the pastel world stays < 1.0 so only energy, sun,
  projectiles, rings and eyes bloom.
- **Composite**: ink → fog → bloom → speed lines (radial dashes streaming outward above ~24 m/s) →
  vignette (+ red flash on `player_hit`, faint mint edge while boosting) → soft highlight shoulder
  (keeps pastels intact, lets HDR energy roll to white-mint) → sRGB → dither.
- **Speed feedback**: FOV widens up to +20° with speed (+ short kicks on launch/explosions/fly toggle),
  speed lines, world-space wind streaks, thruster plume, dust on jump/land, hard-landing ring.
- **Player**: original hero from ~40 primitive pieces rendered as 3 `InstancedMesh`es (capsule,
  sphere, box) + ring/visor/emblem emitters: deep navy suit, pale panels, mint emitters, a glowing
  ring on the right hand, a rhombus chest emblem (no lanterns, no DC iconography). Procedural poses
  (idle, run cycle scaled by speed, air/tuck by vertical velocity, slide, superman-style flight,
  boost dive) blended by damped weights, body lean from acceleration, facing = velocity when moving
  fast, else look. Arms aim at tether anchors / at a construct while it forms. A mint drop-marker ring
  shows the landing point when > 2 m up; on *low* (no shadow map) a soft contact blob is added.
- **Entities**: vertex-coloured merged meshes (1–3 draw calls each): drones with spinning-rotor discs
  (shader streaks, no per-rotor transforms), glowing eye that flares when firing and dies when
  disabled + smoke; turret head tracks the player; civilians flail while falling, tip over when hurt,
  hop when safe; crates/turret base/dummy base use the ink-edged statics material.
- **FX**: one CPU particle pool (points: glow, ring, dot; sparks are routed to the beam batch as
  velocity-aligned streaks), one camera-facing ribbon batch (tethers, forming beams, sparks, wind),
  one instanced projectile batch (stretched along velocity; hostile = orange, reflected = construct
  colour). Effects for impact, smash, explosion, launch, blocked (projectile death puff, mint when a
  construct stopped it), player_hit, entity_damage/disabled, fire (muzzle flash), construct_fire,
  construct_formed/modified/rejected (red fizzle at the ring hand), grabbed, civilian_hurt/safe,
  ring_passed, goal_reached, tether fire/attach, jump/land, fly toggle.

### HUD
Crosshair (turns red and tightens on hostiles; ring shows when a tether would reach), speed readout
+ bar, thrust (gas) and willpower bars, ring status pill (ready / listening / interpreting / formed /
formed·clamped / not understood / rejected), "the ring understood" chip (summary · source ·
confidence · latency), toast stack (clamps amber, approximations blue, abstain/rejections red — never
silent), voice transcript line, the console, construct list (focused ◆ highlighted, clamped marked,
lifetime bar), objective panel (scene goal text, ring-run counter/timer, goal checklist), world
markers for the next ring and open goals (clamped to screen edges when off-screen), banners for goal
reached / area clear / civilian saved / ring run complete, F1 help, F3 perf overlay, pause/menu with
scene buttons and settings.

### Input map
WASD move · Space jump/up · Ctrl or C slide/down · Shift boost · Q / LMB left tether · E / RMB right
tether (hold) · R, wheel or MMB reel · F flight · Enter console (typing shows ghosts; Enter commits;
Esc cancels; ↑/↓ history) · V hold push-to-talk (`window.hardlight.voice.start/stop`) · X dismiss focused ·
Z dismiss all · 1–9 scenes (playground, arena, gap, tower, barrage, falling, boulder, drones, flat) ·
Backspace/F5 reset · F1 help · F3 perf · F11 fullscreen.

Voice integration surface: `window.hardlightApp.setTranscript(partialText, isFinal)` (partials drive
the ghost preview + transcript line, finals commit), also `commit(text)`, `preview(text)`.
The preload exposes `window.hardlight.voice.{start, stop, onPartial(cb), onFinal(cb)}`; main.ts has
placeholder `hl:voice-start/stop` handlers and the renderer listens for `hl:voice-partial/final`.

### Electron shell
One `BrowserWindow`, `contextIsolation: true`, `sandbox: true`, `nodeIntegration: false`, loads
`dist/index.html` from disk. No application menu; `setWindowOpenHandler` denies (http(s) links go to the
OS browser); `will-navigate` blocked; every non-`file:`/`data:`/`blob:`/`devtools:` request is
cancelled in `webRequest.onBeforeRequest`; permissions denied except `media` (microphone); CSP meta in
`index.html` (`'wasm-unsafe-eval'` is needed for Rapier's embedded WASM). The renderer bundle is an
IIFE classic script so `file://` module-CORS rules never apply.

## Budgets (see RENDER_PERF.md for measured numbers)
Measured at 1280×720 in headless Chromium/SwiftShader, 240 frames, one sim tick per frame:

| scenario (high / low) | CPU per frame p50 / p95 | sim tick | scene draw calls (+post) | triangles |
|---|---|---|---|---|
| idle playground | 2.0 / 5.0 ms · 2.0 / 5.9 ms | 0.6 ms | 30 (+10) · 21 (+8) | 23k · 15k |
| high-speed traversal | 2.5 / 5.4 ms · 2.5 / 5.4 ms | 0.5 ms | 24–32 (+10) · 19–23 (+8) | 20k · 12k |
| stress: 20 constructs + ~170 projectiles | 4.1 / 7.6 ms · 3.8 / 8.0 ms | 2.2 ms | 81 (+10) · 72 (+8) | 75k · 68k |

Scene draw calls include the shadow-map pass. Normal play is ~10× under the 300-call budget and
~2–2.5 ms CPU (target < 4 ms) on these shared, contended vCPUs; the stress case is ~4 ms p50.
SwiftShader rAF frame times (0.4–1 s) are software rasterisation and irrelevant to a GPU.

## Tried and rejected
- **EffectComposer + UnrealBloomPass + OutlinePass**: more passes, per-object outline needs extra
  scene renders; replaced by one custom chain (bloom + composite) and baked/depth ink.
- **Inverted-hull outlines**: doubles draw calls and triangles, cracks on hard-edged boxes (split
  normals) and fights with the translucent constructs. Baked barycentric edges + depth silhouettes are
  crisper and free.
- **Depth-difference edge detection** (instead of the 1/z Laplacian): false lines on the ground at
  grazing angles.
- **Built-in `scene.fog`**: fog colour can't follow the sky gradient and it fogs the energy constructs;
  post fog with the sky function + alpha mask solves both.
- **Additive construct blending**: washes out to white against the bright pastel sky; premultiplied
  over + HDR rims reads better.
- **Per-part meshes for constructs / entities**: 100+ draw calls in the drones scene; merged +
  instanced geometry brought it to ~28.
- **PCFSoftShadowMap**: removed in three r18x (falls back to PCF with a warning) — use PCFShadowMap.
- **Noisy "smear" clouds** (first attempt): replaced by large, hard-edged two-tone cel clouds.

## Known issues / limitations
- Only validated on SwiftShader (no GPU here). Colour/brightness were tuned from SwiftShader
  screenshots; a quick pass on a real GPU/monitor is advisable. MSAA + HalfFloat targets need WebGL2
  with `EXT_color_buffer_float` (all desktop GPUs).
- The real-time loop under SwiftShader runs ~1 fps at 1600×900, so all visual QA is done in scripted
  mode (`?test=1`), where the sim advances only via `__hl.step`; animation timers (assembly, dissolve,
  pose damping) use render time and are clamped to 50 ms per frame.
- In a browser, Esc exits pointer lock *and* cancels the console (browser behaviour). In the web build
  Ctrl+W closes the tab — use C to crouch there; Electron has no menu so Ctrl is safe.
- Transparent construct passes are ordered by `renderOrder` (all back faces, then fronts, then edges)
  and by distance within each; heavy overlaps of many constructs can show minor ordering artefacts.
- Constructs don't cast shadows (deliberate: they are light). No real dynamic lights from constructs
  (would recompile every toon shader); glow is bloom only.
- Sphere/cylinder/cone parts are drawn with their declared (possibly elliptic) size; the physics uses
  a ball/cylinder of the *mean* radius, so a strongly non-uniform part (e.g. a flattened ellipsoid)
  looks slightly different from its collider.
- Particle/beam pools are CPU-updated (4096 particles, 2048 beams); plenty for this game, but a GPU
  particle system would be the next step if effects get much denser.
- The HUD's crosshair state does one extra `player.aim()` raycast per frame.

## Requests to the lead (sim / lang)
1. **Make `ConstructRuntime.arrangementOffsets` public** (or add `placeAll(spec)` returning every
   instance pose). Ghost previews of `count > 1` specs call it through `as any` today.
2. **Pure camera function**: `Player.cameraAt(pos, vel, yaw, pitch)` (or equivalent) so the renderer
   does not have to temporarily mutate `player.pos/vel/yaw/pitch` to evaluate the canonical camera at
   the interpolated pose.
3. **spec.ts comment is wrong**: it says wedges rise toward −z; the collider (and the renderer) rise
   toward **+z**. Please fix the comment (or the hull) so forms authors aren't misled.
4. **Event payloads**: add `pos` to `blocked` (projectile position) and `fire` (muzzle), and a
   `player.boosting` flag (renderer currently infers it from gas draining).
5. `drainRemoved()` has a single consumer (the renderer). If audio also needs removals, switch it to
   the `construct_dismissed` event (the renderer could use that too) — please don't call
   `drainRemoved()` elsewhere.
6. Consider a `World.dispose()` that frees the Rapier world + event queue; the app does
   `eq.free(); pw.free()` on scene switch itself (skipped while a commit is in flight).
