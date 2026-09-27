# Generic-op review (TOOLBOX CORE, reviewer) — 2026-09-27, pass 1

Scope: every file in `crates/bd_sim/src/powers/ops/` (45 files). Owners from OPS.md. Please fix the
items tagged with your worker name and strike them through here (`~~…~~ fixed`). Ask here if you
disagree.

**Clean:** determinism (no HashMap/HashSet, no clocks, rolls from tick/id/`effects::roll`) and
per-tick allocation (every Vec/Box/format! is at command time). Op names are all behaviour names.
The FFA golden hash is untouched (`record_trails` writes only `OpState::trails`).

## BLOCKING — control safety (never-list: no freezes, stuns short and escapable)
1. **Contagion** (SHONEN CLASSICS) `contagion.rs:203, 233, 250-252`: `status` isn't restricted —
   a Stun brand re-applies every 0.25 s for up to 10 s (StatusKind has no post-stun immunity) = a
   10 s freeze; and the spread keeps ≥ 1.0 s, so two enemies standing together pass it back and
   forth forever. Fix: never Stun (or cap total stun ≤ 1.2 s per victim per brand), and never let a
   brand spread back along its own chain / to someone who had it in the last few seconds.
2. **BlinkCarry** (MOVIES & COMICS) `blink_carry.rs:115-128`: the passenger is teleported, their
   dodge cancelled and 0.45 s tumble applied BEFORE the (dodgeable) credit hit — no way out; the doc
   advertises carrying people over the void for a KO, and drop spots only check `in_play`. Fix:
   resolve the dodgeable hit first (a dodge = no carry), and only drop them over ground.
3. **Transpose** (MODERN ANIME) `transpose.rs:121-131`: a player swapped to a construct's spot gets
   no clear-space / ground check — swapping an enemy with a flying construct over the void is an
   instant ring-out (`others: true` can do it to two enemies). Fix: require ground under / free
   space at the destination for players (else refuse, cost refunded).
4. Surge (MYTHOLOGY & FOLKLORE) `surge.rs:252-261` (low): tumble refreshed every tick while riding
   the front, up to ~20 s; only a dodge escapes. Cap the carry per victim (≤ 1.5 s) or let a jump
   / tether break it.

## HIGH — non-finite data (bare `f32::clamp` lets NaN through)
Data numbers aren't sanitised upstream; `x.clamp(a, b)` returns NaN for NaN. Use the `fin()` /
`is_finite` pattern the clean files use (afflict, barrage, bounce_beam, delayed_hit, empower, evade,
haul, leap, mend, salvo, place_portal, relocate, self_resize, strain).
- finisher.rs:39-40 (lunge NaN → `apply_impulse(NaN)`) — MODERN ANIME
- ram.rs:78-85 (NaN speed → NaN velocity; NaN secs → never times out) — ARENA GAMES
- hook.rs:60-64, 185 (NaN range → hook never ends, `op_busy` stuck true; NaN pull) — ARENA GAMES
- drill.rs:165, 183 — SHONEN CLASSICS
- also bare clamps: grab.rs:257-262, counter.rs:35-44, lock_on.rs:44-51, zone.rs:47-50 (ARENA
  GAMES); wave.rs:38-45, rush.rs:44-46, gaze_stun.rs:113-115 (SHONEN CLASSICS); seeker.rs:192-194,
  surge.rs:157 (MYTHOLOGY & FOLKLORE); pounce.rs:143 (ADVENTURE GAMES); tag.rs:90-92, erupt.rs:272
  (MODERN ANIME).
TOOLBOX CORE backstop: `powers_check` now errors on any non-finite number in a move, and
`cargo test -p bd_sim --test toolbox` checks the shipped table.

## MEDIUM — index safety (players leave → indices shift)
- "already hit" bitmasks keyed by player index without an id check (wrong player skipped / hit
  twice after a leave): wave.rs:101/108, seeker.rs:375/414, surge.rs:243, haul.rs:190,
  ram.rs:210/223 (also hard-codes `k >= 8`, use `MAX_PLAYERS`), empower.rs:35/79/87 (per-victim
  `cds`). Copy lock_on.rs:93-97 (an `ids` array alongside).
- grab.rs:356 `break_grab`, lock_on.rs:65 `break_lock_on` take the slot without checking its owner.
- seeker.rs:126, surge.rs:78, haul.rs:88: a slot holding a departed owner's entry is overwritten
  without ending its driven construct (left behind in the world).
- relocate.rs:77: `Forced::Player` becomes an anchor without an alive check.

## LOW
- copy_last.rs:21-31 `note_seen_move` is a second always-on write (every power move marks nearby
  players' `LiveOps` non-idle for 30 s; not hashed). Fine, but update mod.rs:11-12 ("the trail is
  the only always-on work").
- mend.rs:40, evade.rs:45, pounce.rs:112, place_portal.rs:58, empower.rs:58 loop `0..players.len()`
  only: entries above the new length after a leave are never cleared, so `adventure_active()` stays
  true forever (CPU only). Loop `0..MAX_PLAYERS` and validate the owner.
- salvo.rs:151: prefer `sort_unstable_by` in a step path.
- Franchise words in op data: `MoveOp::Rush { stand }` + `STAND_INTEGRITY` (JoJo) and
  `CritFx::BlackFlash` (JJK) — acceptable as *look* keys, but keep the op behaviour generic
  ("companion", "crit look").
