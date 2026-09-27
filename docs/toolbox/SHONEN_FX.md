# SHONEN CLASSICS ops → presentation beats (for the PRESENTATION worker)

Ten generic ops (docs/toolbox/OPS.md; code `crates/bd_sim/src/powers/ops/{drill,mark_recall,warp_to,
copy_last,gaze_stun,phase,banish,rush,contagion,wave}.rs`, per-player state `ops/live.rs`) send one
event per beat:

```rust
GameEvent::Power(PowerEvent::Op(OpEvent::Toolbox { player, fx: ToolboxFx, from: Vec3, to: Vec3, secs: f32, size: f32 }))
```

| `fx` | op · beat | `from` → `to` | `secs` / `size` | what it should look like (the headliners' bar) |
|---|---|---|---|---|
| `DrillStart` | Drill: lunge starts | hand → lunge end | lunge s / grind s | the orb (Rasengan: blue grinding sphere) / blade (Chidori: crackling hand) held OUT in the palm, trail behind |
| `DrillLatch` | Drill: the palm met them | driller chest → contact | grind s | the sphere grinding INTO them: spiral shreds, both bodies shake, rising whine |
| `DrillRelease` | Drill: spin-away / the pierce | driller → contact | — | burst + spiral shockwave along the look; the victim spins away |
| `DrillEnd` | Drill: whiff / hit a wall | driller → end point | — | the orb bursts into wind (terrain version: a bored crater) |
| `Mark` | MarkRecall: marker thrown | hand → where it stuck | mark s / 1 = on a player | a kunai with a seal tag streaks and sticks; on a player a glowing seal rides on them |
| `Recall` | MarkRecall: teleport to it | old feet → new feet | — | yellow flash: vanish in a streak, reappear at the seal (NOT a bamf, NOT a blink) |
| `Warp` | WarpTo | old feet → new feet | — | Instant Transmission: two fingers to the forehead, a flicker, gone; reappear with a soft "shunk" |
| `Copy` | CopyLast | eyes → ahead | — | the red eye flares, tomoe spin, then the copied move plays |
| `Gaze` | GazeStun caught someone | caster eyes → victim eyes | status s | a thin red line eye-to-eye; the victim's screen swims (they're in the genjutsu) |
| `Flash` | GazeStun radial (Solar Flare) | eyes | status s / range | a white nova flash from the face; only people who looked get whited out |
| `PhaseOn` / `PhaseOff` | Phase | chest | phase s | the body swirls into a vortex (Kamui) / red flicker (King Crimson); hits pass through |
| `Banish` | Banish | where it was → where it went | 1 = a construct swallowed | a spiral vortex sucks the thing in (and spits a player out above) |
| `Rush` | Rush: the flurry starts | chest | rush s / hits | the stand steps out (a real construct when summoned); `StandRush` events per punch as for typed ORA |
| `Brand` | Contagion took hold | victim | brand s / 1 = spread | BLACK flames that cling to the victim (Amaterasu); a spread jumps like fire catching |
| `BrandEnd` | Contagion burned out | victim | — | the flames gutter out |
| `Wave` | Wave: a crescent leaves | start → far end | flight s / width | a crescent `size` m wide flying `from → to` over `secs` (Getsuga Tensho black-red, Destructo Disc a flat spinning yellow disc, flying sword slashes pale green); the host steps it exactly this way: it passes through players and cuts terrain |

A wave's flight is fully determined by its start event (straight line, constant speed). Until these
are drawn, `movefx::alias_op` maps Drill → Impulse, Wave → Carve, Rush → Shove, MarkRecall /
GazeStun / Contagion / Banish → Link, so every move has a beat today.

Chain note: WarpTo(`line: true`) lands you point-blank in FRONT of the target, facing them (the
chainable Instant Transmission → Kamehameha). The plain version lands behind their back relative to
THEIR facing, which leaves the warper facing away when the target was looking at them — a client
"snap the camera to face the target on `Warp` for the local player" would make that version great too.
