# Worker coordination protocol (many workers, one working tree)

All workers edit and build the SAME working tree. One broken edit blocks everyone, so:

1. **Build status is published — don't build to find out.** The lead runs a watcher that runs
   `cargo check --workspace --tests` every ~90 s and writes `target\build_status.txt`:
   `GREEN hh:mm:ss` or `RED hh:mm:ss` + the first errors tagged with the owning worker.
   - RED in *someone else's* files → don't start a build. Do non-build work (read, write code in your
     own new files, design tests) and re-read the status file in ~2 min.
   - RED in *your* files → fixing it is your top priority, before anything else.
2. **Stay green.** Create a module before referencing it. After every edit to a shared crate, run
   `cargo check -p <crate> --tests` and fix before moving on. Never leave a file half-edited while
   you go think / wait for a long build.
3. **Claim hot shared files before editing them.** Hot files: `crates/bd_core/src/*`,
   `bd_game/src/{main.rs,menu.rs,input.rs,sim_bridge.rs,lobby/prefs.rs,present/mod.rs,net/proto.rs}`,
   `bd_sim/src/{lib.rs,bots.rs,hook.rs,world/mod.rs,world/actions.rs,world/player.rs,constructs/held.rs,movement/*}`,
   `assets/tuning/*.ron`, any `Cargo.toml`, and any file another worker owns.
   - Claim: `New-Item -ItemType File "target\locks\<path with \ and / replaced by _>.lock" -Value "<worker> <time>"`
     (fails if it exists → someone holds it: wait 30 s, retry).
   - Re-read the file after claiming. Edit surgically (Edit tool, never rewrite/reformat), run
     `cargo check`, then delete your lock. Hold a lock ≤ 5 minutes.
4. **Builds:** `CARGO_TARGET_DIR=target\play`, `CARGO_BUILD_JOBS=8`, default features. Release builds
   only when you need an exe (screenshots/perf), at most ~1 per 15 min; copy the exe out before running.
5. **Runtime smoke after client/render changes.** Compile-green isn't enough (shader and render
   validation errors only show up at startup). After changing cameras, materials, shaders or anything
   that runs during Loading, run the copied release exe with
   `--screenshot target\smoke\<you>.png --quit-after 10` and confirm it gets past Loading.
6. **Never commit.** The lead commits green checkpoints often, so `git diff` stays small and readable.
6. The owner is at the machine playtesting: prefer headless/offscreen/record modes.
