# PvP design (1v1 duels, online + AI rivals)

**Focus (owner's words): "really polished 1v1 combat — don't go so wide in the features (racing/enemies/etc.),
just let their constructs go wild and give them absolute freedom in that regard."**
So: one mode (the duel), one excellent arena, no PvE side content in the menu. Breadth goes into what a
construct can be and do, depth goes into combat feel (hits, knockback, readability, counters, balance).

Decided by the project owner: PvP focus; **online matches + AI "rival lanterns"**; connection via a
**Cloudflare quick tunnel** (no account, no port forwarding); first mode: **1v1 duels**.

## Match rules (duel v1)
- Two lanterns, one arena, first to **3 kills** wins (rounds of max 3 min; sudden death after).
- Players have **health** (100) and a short **respawn** (3 s, at the spawn farthest from the opponent, 2 s spawn shield).
- Everything hurts the opponent: construct weapons, strikes, thrown objects, crushes, explosions, falling from
  a launch into a wall… (damage via one `damagePlayer(target, amount, cause, by)` path with attribution).
  Own constructs never hurt their owner (except self-launch fall damage? no: no fall damage).
- Willpower (energy) and Inspiration (creativity) are per player. Creativity is the power curve: novel ideas
  hit harder and cost less; spamming the same idea goes stale. Typing speed is the skill ceiling.
- Constructs have an **owner**. Opponent constructs can be targeted and destroyed ("smash his wall") —
  construct HP scales with the energy spent on it.
- Arenas: vertical, lots of tether anchors, cover to build on, 60–90 m across (movement matters).

## Netcode (listen server, host-authoritative except own movement)
- The **host** runs the authoritative simulation (World, constructs, entities, interpreter for every player).
- Each **client simulates its own lantern's movement locally** (instant, keeps bhop/tether feel) and sends
  pose + velocity + input bits every tick; the host applies them to that player's proxy.
  Knockback/launches/grabs of a remote player are sent back as velocity/position corrections the client applies.
  (Trade-off: a modified client could cheat on its own movement — acceptable for friend duels.)
- Typed commands: the client sends `{text, aim: {origin, dir, entity/player/construct id}}`; the host runs that
  player's Director and broadcasts the result (summary, clamps, messages) + construct spawns.
  Ghost previews are computed on the client (the client keeps a mirror world: same static scene + construct
  colliders from snapshots) — previews never change state.
- Snapshots host→client at 30 Hz: players, construct instances (+ joint poses), entities, projectiles; fx/world
  events; construct create/modify/dismiss carry the validated spec so the client builds meshes/colliders.
  Clients render remote things ~100 ms in the past (interpolation).
- Transport: WebSocket (JSON first; binary later if needed). Host = `ws` server on localhost in the Electron
  main process + `cloudflared tunnel --url http://localhost:PORT` → share the `https://<words>.trycloudflare.com`
  address (the words are the "room code"). Joiner connects to `wss://<words>.trycloudflare.com`.
  LAN/localhost direct join also supported (`ws://ip:port`) for testing.
- cloudflared.exe (Apache-2.0) is bundled in the Windows zip (pinned version + sha256).
  Quick tunnels are Cloudflare's free testing service: no uptime guarantee; fine for friend duels.

## AI rival lanterns
- Bot players on the host with the same body, movement and ring. They **type** commands through the same
  interpreter at a configurable speed (e.g. 4–9 chars/s + think time), choosing situational ideas
  (shield when shot at, launch/tether when the enemy is high, siege machines when far, grab/throw when close)
  and varying them to stay inspired. Difficulty = typing speed, aim error, reaction time, idea repertoire.
- Bots are also the automated test opponents for PvP (bot vs bot matches must finish and be won by the
  better settings more often than not).
