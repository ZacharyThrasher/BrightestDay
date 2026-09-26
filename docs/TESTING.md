# Testing & the quality gate

Brightest Day is built by several engineers in parallel. What keeps the pieces from turning into a
fragile mess is **machine verification at every boundary**. No reviewer can hold the whole game in
their head, so each crate proves its own contract with tests, and one gate runs all of them.
This file covers how to run the gate, what each stage guarantees, and the rules every engineer
follows. See also DESIGN.md, "Robustness".

## Running the gate

```powershell
pwsh tools/check.ps1                          # manifest + fmt + clippy + tests, every crate
pwsh tools/check.ps1 -Crates bd_voxel,bd_net  # just these crates (names with or without "bd_")
pwsh tools/check.ps1 -Soak 120 -Bench -Smoke  # full integration gate (before merging to main)
pwsh tools/check.ps1 -Deep -Crates bd_sim     # property tests with 10x the cases (or -PropScale N)
pwsh tools/check.ps1 -Skip fmt,clippy         # tests only
```

Each stage prints `PASS` / `FAIL` / `WARN` / `SKIP`. The script exits 1 if anything FAILs.
Builds use `CARGO_TARGET_DIR=target\check`, so the gate never fights other engineers' target
directories. Full logs are in `target\check\logs\<stage>-<crate>.log`. On a failure the gate prints
the relevant lines (panic message, minimal failing input, compiler errors).

**Nothing merges unless the gate is green for the crates it touches.** Run `-Soak -Bench -Smoke` before
merging anything that touches the sim, the net layer, or the client.

### Options

| option | effect |
|---|---|
| `-Crates a,b` | only these workspace members |
| `-Skip fmt,clippy,test` | skip stages |
| `-CargoProfile gate\|release\|dev` | test profile (default `gate`, see below) |
| `-Deep` / `-PropScale N` | multiply every property test's case count (env `BD_PROP_SCALE`) |
| `-Soak <secs>` `-SoakSeed <n>` | run the chaos soak |
| `-Bench` `-BenchSecs <n>` | client stress benchmark vs `tools/perf_thresholds.ron` |
| `-Smoke` | launch the client, take a screenshot, check it isn't blank |
| `-FmtReportOnly` / `-StrictFmt` | formatting policy (see fmt below) |
| `-ClippyStrict` | any clippy warning fails |
| `-UpdateLintBudget` | rewrite `tools/lint_budget.json` from the current counts |
| `-Isolated` | force the isolated mirror workspace |
| `-TimeoutMin <n>` | per-cargo-invocation timeout (default 60) |

## Stages and what they guarantee

| stage | guarantee |
|---|---|
| **manifest** | Every workspace manifest parses and its targets exist. If an in-progress crate breaks its manifest (a common way to block *everyone*: e.g. a declared example whose file doesn't exist yet), the gate names that crate. It then checks every other selected crate in an isolated mirror workspace (`target\check\ws`, which holds only the selected crates and their path deps). One broken crate never hides the state of the others. |
| **fmt** | `cargo fmt --check` per crate, using the house style in `rustfmt.toml` (`max_width = 140`, `use_small_heuristics = "Max"`). **Enforced** (FAIL) for finished crates (those listed in `tools/lint_budget.json`: bd_core, bd_voxel, bd_net, bd_voice). Crates still being written are report-only (WARN), so nobody's uncommitted work gets reformatted under them. When a crate is finished, run `cargo fmt -p <crate>` once and add it to the budget file. `-StrictFmt` enforces everything; `-FmtReportOnly` enforces nothing. |
| **clippy** | Pass 1: `cargo clippy -p <crate> --all-targets`. Compile errors and *denied* lints (crate-level `#![deny(...)]`) FAIL. Other warnings are counted. Pass 2: runtime code only (lib + bins, no tests), with the panicking-construct lints `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `unreachable`, `todo`, `unimplemented` counted per crate. A crate listed in `tools/lint_budget.json` FAILs if any count goes *above* its budget: a ratchet, so counts only go down. Crates not listed yet are report-only (WARN). |
| **test** | `cargo test -p <crate> --no-fail-fast` for every crate: unit, integration, property, and doc tests. The profile is `gate`: optimised like release, so property tests run hundreds of thousands of cases in seconds, but with **debug assertions and overflow checks on**. Integer overflow on hostile input (which panics in the game's dev builds) fails the gate. It is not allowed to wrap silently. |
| **soak** | `-Soak <secs>`: runs `cargo run -p bd_sim --profile gate --example soak -- --seed <n> --ticks <secs×120>`. `<secs>` is simulated time. The seed is random each run unless you pass `-SoakSeed`, and it is printed. This is the chaos world (`bd_sim::fuzz`): bots cast random recipes from the full combination space, with portals, links, resizes, piloting, joins and leaves. The soak asserts no panic, `check_invariants()` every tick, the tick budget, and determinism. It is SKIPped if `crates/bd_sim/examples/soak.rs` doesn't exist. |
| **bench** | `-Bench`: builds `brightest_day` (release) and runs `--bench 20`, which is a practice scene with continuous explosion stress. It reads the JSON the run writes and compares `p99_ms`, `max_ms`, `avg_ms` and `hitches_16ms` against `tools/perf_thresholds.ron`. The design targets are frame p99 < 7 ms and no hitch > 16 ms. |
| **smoke** | `-Smoke`: builds `brightest_day` (release) and runs it with `--screenshot <png> --quit-after 8 --windowed --size 1280x720`. The run must exit cleanly with no panic, and the PNG must exist and not be blank or black. The check samples 32×24 pixels and needs mean luminance ≥ 4, std-dev ≥ 2, and at least 8 distinct colours. |

## The property tests

Property tests use `proptest`. They generate thousands of random (and deliberately hostile) inputs,
check invariants, and on failure **shrink** to a minimal failing case and print it.

| file | what it proves |
|---|---|
| `crates/bd_core/tests/props_core.rs` | `Recipe::normalized` is idempotent and always yields finite, positive, in-range numbers, recursively through payload chains (NaN/inf/0/huge inputs). `raycast` terminates and never panics for **any** f32 input (NaN, ±inf, `max_dist = 1e30` on an infinite empty world), and agrees with a brute-force ray march on random worlds. Geometry helpers never panic. |
| `crates/bd_voxel/tests/props_voxel.rs` | Random worlds (random min/size, boxes, spheres, specks, unknown material ids, indestructible cores) under random carve sequences, including NaN/inf/1e9/negative/inverted shapes and powers. Nothing panics. Chunk solid counters stay exact. Indestructible voxels are never removed. Removed and falling voxels are in bounds and accounted for exactly. A falling chunk is a true island that carries no anchor. With unlimited limits, collapse leaves nothing unanchored. Dirty chunks cover every edit. The content hash tracks edits. Everything is bit-deterministic across two "clients". The mesher's output is well-formed: indices in range, finite, inside the chunk, CCW winding, and face count equal to a brute-force count of exposed faces. Arena generation is deterministic. |
| `crates/bd_net/tests/props_codec.rs` | Decoding random, bit-flipped, truncated, padded, or hostile-length byte blobs as every wire type never panics. Roundtrips are exact. A 100 000-deep recipe payload chain is rejected instead of overflowing the stack. Oversized messages are rejected by the receiver and never crash the sender. |
| `crates/bd_net/tests/props_netcode.rs` | Hostile `InputPacket`s (out-of-order, duplicate, `u32::MAX`, huge gaps, NaN view ticks) can't crash the host's `InputQueue`, and acks never go backwards. An honest client's inputs are consumed in order, at most once. `InputBuffer` invariants hold. `Reconciler` is checked differentially against a model under random acks, capacity and frame times. `SnapshotBuffer` gives finite alphas within the extrapolation limit. `LagCompHistory::rewind` and `clamp_view_tick` stay inside the window for any f64. `ClockSync`, `TickPacer` and `LatestLane` accounting hold. Address parsing never panics. |
| `crates/bd_net/tests/fuzz_session.rs` | Real loopback sockets. A client that stops reading is dropped once its send queue overflows, and an oversized client message drops only that client. While two good clients play, attackers send raw TCP junk, WebSocket garbage before the handshake, bad magic/protocol, a 30 000-character control-char name, malformed/NaN/inf pings, unknown frame kinds, random data after a valid handshake, and 10 idle connections. The host must reject or kick every attacker, sanitise the name and RTT, and keep the good clients connected and talking. |
| `crates/bd_sim/tests/movement_props.rs` | Random small voxel worlds + random oriented colliders + garbage input sequences (NaN/inf angles, any i8 axes, random buttons and reel notches) + extreme impulses and modifiers. Every tick the state is finite, gas is in range, speed stays under the hard cap, and the hull never penetrates a voxel or collider (no tunnelling). Hooks never fly past `tether_range` and only anchor to real solids. Two runs are bit-identical, and a bincode snapshot/restore mid-run changes nothing. A separate test shows an infinite 1-voxel wall can't be crossed by any inputs or launches. |

### Reproducing a failure

1. The failure output shows `minimal failing input: ...`. That is the shrunk case.
2. proptest writes the seed to `proptest-regressions/<test file>.txt` next to the test.
   **Commit that file.** From then on the case runs first on every run, forever.
3. Re-run a single test with `cargo test -p bd_voxel --profile gate --test props_voxel carve_sequences`.
   Add `-- --nocapture` for prints.
4. To dig deeper, paste the minimal input into a plain `#[test]` and step through it.

### Reproducing a soak failure

The gate prints the seed before the run (`soak: seed <n>, <ticks> ticks`), and the soak prints the seed
and tick of any failure. Re-run exactly that world:

```powershell
cargo run -p bd_sim --profile gate --example soak -- --seed <n> --ticks <ticks>
# or through the gate:
pwsh tools/check.ps1 -Crates bd_sim -Skip fmt,clippy,test -Soak <secs> -SoakSeed <n>
```

The chaos world is deterministic for a seed, so the failure replays tick-for-tick. The gate prints
the exact reproduce command when the soak fails. **Contract for `examples/soak.rs`:** accept
`--seed S` and `--ticks N`, print the seed and tick on failure, and exit non-zero on any
invariant violation, determinism mismatch, or tick-budget breach.

## Rules every engineer follows

1. **Untrusted data never panics.** Untrusted means network bytes, anything a client reports (sequence
   numbers, view ticks, RTTs, names), typed or spoken text, recipes, carve ops, and modifier values
   written by other systems. Decode defensively. Validate or clamp at the boundary. Malformed input is
   kicked, ignored, or replaced by a safe default, and never crashes. In these paths, don't use
   `unwrap`/`expect`/`[]` on anything derived from untrusted data (use `get`, `?`, `let else`). Use
   `checked_`/`saturating_`/`wrapping_` arithmetic on external integers: the gate and dev builds have
   overflow checks on.
2. **Floats from outside are hostile.** Every public function that takes `f32`/`f64` either rejects
   non-finite values or handles them. Outputs are always finite. "Finite" is not enough on its own:
   `1e30` squared is `inf`. Clamp to a plausible range before doing arithmetic (speeds, RTTs, ticks).
3. **Every loop driven by data has a bound.** Examples: ray steps (`MAX_RAY_STEPS`), collapse search
   (`CollapseLimits`), sweep splitting, payload nesting depth (`MAX_PAYLOAD_DEPTH`), and input queue
   depth. Recursion over data is bounded too.
4. **Invariants are written down and machine-checked.** Each module states its invariants in docs and
   asserts them in property tests. The sim checks them every tick in debug (`check_invariants()`).
   A misbehaving construct is removed, never the game.
5. **Determinism.** Anything replicated or predicted (movement, carves, arena generation, the sim) is a
   pure function of its inputs. That means no `HashMap` iteration order in results, no wall-clock time,
   seeded RNGs, and fixed visit orders. Identical inputs give bit-identical outputs. State that
   reconciliation restores must round-trip through serde unchanged. Both properties are tested.
6. **Contracts are frozen.** `bd_core` is v1 and changes only through the lead. Wire types change only
   with a `PROTOCOL_VERSION` bump.
7. **Every bug gets a test.** Fix the bug, then add a property or regression test that would have
   caught it. Commit the `proptest-regressions` file if proptest found it.
8. **A crate is "finished" when** it has property tests for its contract, it is in
   `tools/lint_budget.json`, and the gate is green for it.

## Bugs the gate found (2026-09, first run)

| where | bug | fix |
|---|---|---|
| `bd_core::voxel::raycast` | `max_dist = 1e30`/inf on an empty world looped ~forever. A NaN `max_dist` looped forever. An origin near the i32 voxel limit overflowed. | step cap `MAX_RAY_STEPS`, NaN/non-finite early-out, checked steps |
| `bd_core::Recipe` | `normalized()` left NaN accents, so it was not idempotent, and it ignored payloads. Deserialising a deeply nested payload chain **overflowed the stack and aborted the process** (a hostile peer could kill the host). | accent sanitised, payload normalised and chain capped at `MAX_PAYLOAD_DEPTH`; depth-limited payload deserializer |
| `bd_voxel` carve | A carve op with non-finite geometry still removed voxels (e.g. a NaN centre carved the voxel at the origin, and an inf box carved everything). | non-finite shapes are a no-op |
| `bd_net::InputQueue::receive` | An out-of-order packet from a client caused `u32` subtract overflow, which **panics the host** in debug builds. | `saturating_sub` |
| `bd_net::clamp_view_tick` / `LagCompHistory::rewind` | A NaN view tick from a client produced a NaN rewind (NaN hit positions). A NaN or negative `max_rewind` panicked in `f64::clamp`. | NaN means "no rewind"; bounds sanitised |
| `bd_net::ClockSync` / `TickPacer` | One NaN/inf/absurd clock sample poisoned the offset forever. An inf `dt` turned the accumulator into NaN. A huge target overflowed the `u64` tick. | implausible samples ignored; dt/target sanitised; saturating tick |
| `bd_net::Reconciler::update_smoothing` | A NaN or negative frame time turned the visual offset into NaN, or made it blow up. | dt sanitised |
| `bd_net` transport | A client-reported RTT of `inf` was stored and exposed by `HostSession::rtt`. | non-finite/absurd RTTs ignored |
| `bd_voxel::VoxelWorld::content_hash` | Word-wise FNV only propagated differences upwards: edits to every 8th voxel changed only the top 8 bits, so desync detection missed about 1 in 256 such edits. | full-avalanche mix per word |
| `bd_sim::movement` ground probe | Sliding along a sloped construct can leave the hull a few µm inside the floor. The ground probe then ignored the floor, and the step-down snap **dropped the player 0.5 m into the terrain** (a 1-voxel floor would be fallen through). | lifted re-probe when the normal probe misses (bit-identical otherwise) |
| `bd_sim::movement` swept SAT | When the hull *touched* a box (overlap below `OVERLAP_EPS`), the sweep reported no hit even when the hull was moving into the box. A hull resting 0.02 mm from a construct face then moved 3 cm inside it in one tick. | shallow contacts block inward motion too (`sweep_aabb` and `sweep_obb`) |
| `bd_net` transport | A client that stopped reading made the host queue its messages forever, with memory growing without bound. Clients could send messages of up to 8 MB. | per-connection send queue capped at `MAX_QUEUED_BYTES` (16 MiB), then the peer is dropped ("send queue overflow"). Client → host messages are capped at `MAX_CLIENT_MESSAGE_BYTES` (64 KiB) by the host's WebSocket layer; host → client keyframes keep the 8 MB limit |
| `bd_sim::movement` tethers | A rope could attach longer than `tether_range` if the player retreated while the hook flew (design call). | the rope attaches taut at `tether_range` and winds in at `rope_retract_speed` |
| `bd_sim::movement` broadphase | A 1e30 knockback caused `i32` overflow computing the query region, which **panics** in debug. | i64 region maths |
| `bd_sim::movement::step_player` | A huge velocity (`|v|²` overflows to inf) or garbage modifiers (NaN, `f32::MAX` speed multiplier) turned the player into NaN for good. An inf tumble lasted forever. | speed pre-clamp (4× hard cap), modifier sanitising, finite tumble only (bit-identical for legitimate play) |

`crates/bd_net/tests/lag.rs` runs the prediction/reconciliation/interpolation checks on a **virtual
clock with a simulated, order-preserving lag channel** (8 seeds, deterministic, no wall-clock sockets),
so it can't fail because the machine is busy. A small real-socket `LagProxy` smoke test with
generous tolerances keeps the transport path covered.