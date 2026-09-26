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
- **Implemented** in `src/net/game.ts` (HostSession / ClientSession; wired up in `src/app/main.ts`):
  - A peer joins → both load the duel (or city) scene; the host adds `p2` and starts the match, which waits (paused) until
    the client reports it has loaded. Casts from the client run on the host through `p2`'s own Director (`commitSync`).
  - The host owns the client's lantern while it is dead, held or riding; otherwise the client's reported pose drives the
    proxy (`world.externallyDriven`). Knockback shows up as a velocity correction, a construct's tether as a tether attach.
  - Snapshots are delta-encoded against what was already sent on the ordered channel (constructs and entities are only
    resent when they move). Construct specs travel once per version (`cadd`). Every snapshot carries
    `world.stateHash('sync')`, a hash of the discrete state (score, health, alive, willpower, which constructs and entities
    exist). The client compares it after applying that snapshot and asks for a full resync if they differ.
  - Lost connection → the match pauses with a banner and resumes after the reconnect with a full resync.
  - Not mirrored yet: breakable-block debris (cosmetic), articulated colliders of machines on the client (they are drawn
    with their joint poses but collide as their rest shape), and System 2 (LLM) casts for the remote player (System 1 only).
- cloudflared.exe (Apache-2.0) is bundled in the Windows zip (pinned version + sha256).
  Quick tunnels are Cloudflare's free testing service: no uptime guarantee; fine for friend duels.

## AI rival lanterns
- Bot players on the host with the same body, movement and ring. They **type** commands through the same
  interpreter at a configurable speed (e.g. 4–9 chars/s + think time), choosing situational ideas
  (shield when shot at, launch/tether when the enemy is high, siege machines when far, grab/throw when close)
  and varying them to stay inspired. Difficulty = typing speed, aim error, reaction time, idea repertoire.
- Bots are also the automated test opponents for PvP (bot vs bot matches must finish and be won by the
  better settings more often than not).

## Speed vs. hitting (movement is defence, constructs are offence)
Owner feedback: the movement's weight/momentum feels good — keep it. So speed is only trimmed at the extreme
and hitting is solved on the construct side.

**Speed tiers (measured, `test/movement.test.ts`)**

| technique | before | now | note |
|---|---|---|---|
| run / bhop | 11 | 11 | unchanged |
| air-strafe | 24.6 | 24.6 | unchanged |
| tether swing peak | 33.6 | 31.9 | barely touched |
| boost peak | 63.7 | 43.7 → 28.7 after 2 s | stacked speed bleeds back |
| flight cruise | 17 | 17 | unchanged |
| ramp launch (30 m/s in) | 11.3 m | 10.9 m | |

Mechanism: a soft ceiling (`MOVE.softCap` 28 m/s). Below it nothing changes; above it the excess is dragged
(linear + quadratic), construct launches are exempt for 1.6 s. Hard cap 70 m/s (was 160).

**Hitting a fast target: lock-on + honest tracking.** You aim with a crosshair, but you *type*, so pixel aim
isn't the skill:
1. **Lock-on at cast time:** "him/her/the enemy/them" always resolves to the opponent; the crosshair also locks
   the opponent if they're within a generous cone (~15°) or within 6 m of the aim ray. Locked casts show a
   lock marker.
2. **Predictive intercept:** projectiles, strikes and chasers lead the target (aim at where they will be,
   from their velocity), re-planned every tick for homing things.
3. **Speed tiers, so escaping costs something:** fast strikes (hammer/fist lunge, ~45–60 m/s) commit their
   direction at launch — a juke dodges them; chasers (hands, dogs, missiles, cars) track continuously with
   a max speed just above the sustainable player speed (~30–34 m/s) but a limited turn rate — outrunning
   needs boost/tether bursts, and hard direction changes, walls and line of sight shake them; area
   constructs (cages, domes, fields, explosions, walls across the path, floors that launch) don't need to
   hit a point at all, they deny space — the answer to a very mobile opponent.
4. **Counters are constructs too:** shields/walls block, mirrors reflect, decoys pull locks, cutting a
   chaser's tether/destroying it. Nothing is unavoidable; everything costs willpower and goes stale if spammed.
