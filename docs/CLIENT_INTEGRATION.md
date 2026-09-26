# Client integration plan (bd_game) — three parallel engineers

The client shell is done (see `crates/bd_game/src/*`: cli, settings, window, state, tick, input, camera,
player, render/{sky, voxel_render, tethers, debris.wgsl}, debris, juice, arena, tutorial, audio, menu, hud,
warmup, automation). All sim crates are done or finishing: bd_sim `World` (FFA, party, powers, constructs,
bots, snapshots/events, `predict_player`, `apply_snapshot`, `apply_event`), bd_lexicon, bd_shapes, bd_fx
(`BdFxPlugin`), bd_net, bd_voice. Now we make the game.

## Ownership (strict — edit only your files; each engineer registers its own plugin in `main.rs` with a
## one-line edit; keep the crate compiling: add stubs first, then fill)

| engineer | owns (new files unless noted) |
|---|---|
| **SIM** (world bridge + presentation) | `src/sim_bridge.rs`, `src/practice.rs`, `src/present/**` (events → juice/sfx/debris/onomatopoeia/KO streak/crosshair flare; constructs via bd_fx; other players' costumes + animation; fruits, stands, portals, links, fields, time-stop grey; third-person camera rig in `src/present/camera3p.rs`), modifications to `player.rs`/`arena.rs`/`camera.rs` needed to run the local player through `World` |
| **UI** (player-facing input & screens) | `src/console.rs` (ring console: typing, ghost completion, lexicon resolve, powers `parse_power_phrase`, stand-rush tokens, `≈` note, `logs/unresolved.txt`), `src/voice_ui.rs` (bd_voice PTT → same path), `src/oath.rs`, `src/viewmodel.rs` (first-person hand + ring: diegetic health/willpower glow, held-construct viewmodel with swing curves, fire kick), `src/lobby/**` (host/join screens, lobby settings panel for every `GameSettings` knob + presets, map picker, costume creator, party pick screen + boom-or-bust coin, scoreboard on Tab, podium) — may restyle `menu.rs`/`hud.rs` |
| **NET** (multiplayer) | `src/net/**`: host (World authority, bd_net HostSession, cloudflared tunnel + lobby code display), client (prediction via `predict_player` with reconciliation + visual error smoothing, remote interpolation via SnapshotBuffer, `apply_snapshot`/`apply_event`, input packets with view_tick), lobby sync (settings, costumes, ready), disconnect/rejoin, LAN direct `ip:port`, a `--host`/`--join <code>` CLI and a 2-process loopback test harness |

## The contract between them (SIM writes it FIRST, within ~20 min, then everyone codes against it)
`src/sim_bridge.rs` exposes an `ActiveWorld` resource abstraction used identically in practice, host and
client modes:
- the authoritative or replica `bd_sim::World` + which `PlayerId` is local;
- `SimCommands` event/queue the UI pushes into: `Cast { recipe, shape, aim, incantation }`,
  `PowerMove(slot)`, `StandRush(n)`, `Oath{...}`, `Dismiss`, `ToggleTransform`, `PartyPick(i)`, `Graft...`;
  in host/practice mode they apply to the World directly; in client mode NET forwards them to the host;
- a `GameEvents` Bevy event re-broadcasting `bd_sim::GameEvent`s after each tick (presentation + UI read it);
- per-frame interpolated views for rendering (players, constructs) — SIM owns rendering them.

## Rules for all three
- Owner's north star + snappiness gates (docs/DESIGN.md): p99 frame < 7 ms on a quiet machine, raw mouse per
  frame, no mid-play loading/compiles (register warm-ups), no per-frame allocation in hot paths.
- Casting must *feel* instant: forming VFX/sound start on the key press locally; the host confirms.
- Minimal text on screen; world-space feedback preferred.
- CARGO_TARGET_DIR: SIM `target\csim`, UI `target\cui`, NET `target\cnet`. If the crate doesn't compile
  because of another engineer's mid-edit, wait and retry; never "fix" their files — message them.
- Verify by running the game (`--screenshot`, `--bench`, automation scripts) and LOOK at screenshots.
- Do not git commit. Never put the user's personal info in any network request.
