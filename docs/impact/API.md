# Impact frames: API (IMPACT FRAMES worker)

An **impact frame** is the anime beat where a huge hit freezes the picture for 2–5 frames. The scene
goes high-contrast monochrome or duotone in the move's colours (inverted for Black Flash), ink speed
lines rush in to the impact point, the attacker's and victim's silhouettes are burned in, the colour
channels split, and then it snaps back with a zoom punch. It's render-side only and never touches
the sim.

Code: `crates/bd_game/src/present/impact_frames/` (plugin `ImpactFramesPlugin`). Styles and rules:
`assets/tuning/impact_frames.ron` (hot-reloaded; a new style is data only). Shader:
`impact_frames/impact_frame.wgsl` (one `FullscreenMaterial` pass on the player camera, in
`PostProcess` before tonemapping). The render-world component only exists while a frame is playing, so the pass
costs nothing when idle.

## Triggering one (other workers)

Write the message. The rules decide the rest: who sees it, how long it holds, the frequency cap, and
the photosensitivity pref.

```rust
use crate::present::impact_frames::{ImpactFrameRequest, ImpactTrigger};

fn my_system(mut frames: MessageWriter<ImpactFrameRequest>) {
    frames.write(
        ImpactFrameRequest::new(ImpactTrigger::HeavyWeapon, point) // world-space impact point
            .between(Some(attacker), Some(victim))   // PlayerIds (either may be None)
            .weight(0.8)                             // 0..1+: hit weight → hold 2..5 frames + intensity
            .dir(hit_dir)                            // world hit direction (chromatic split / skew), may be ZERO
            .style("ink")                            // optional: a style key from impact_frames.ron
            .tint([1.0, 0.6, 0.1]),                  // optional: sRGB accent override (the move's colour)
    );
}
```

Also add `app.add_message::<ImpactFrameRequest>()` in your plugin. It's idempotent, and it keeps
your plugin's startup safe even if you run before this one.

* **`ImpactTrigger`** is WHY the frame fires. It sets priority, which cap class applies, and the
  default style when you don't name one: `Move`, `Crit`, `Ko`, `Signature`, `Counter`,
  `HeavyWeapon`, `BeamClash`, `DomainSureHit`, `Custom`. A higher-priority request can pre-empt a
  frame that's still playing, but only inside the cap.
* **Style keys** live in the `styles:` map of `impact_frames.ron`. Built in: `black_flash`, `holy`,
  `void`, `ink`, `comic`, `glitch`. An unknown key falls back to the trigger's default and logs once.
* **Weight** comes from the hit. Use `ImpactFx::strength` (≈ 0.1 tap … 1 big … 2 kaiju) or your own
  0..1 charge fraction. It's clamped to 0..1.5.

### WEAPON FEEL: fully charged heavy weapons

Two routes:

1. Automatic: every `HitKind::HeavyMelee` hit with `impact.strength >= rules.heavy_melee_min_strength`
   fires `HeavyWeapon` with the style `rules.trigger_styles.heavy_weapon`.
2. Explicit, for a class-specific look: write the `ImpactFrameRequest` yourself with
   `ImpactTrigger::HeavyWeapon` and your class's style key (add the style to the RON). The
   automatic route skips a hit you've already announced (same attacker + victim within 0.1 s).

## Triggers wired in by this worker (all data-driven)

| trigger | source | default style |
|---|---|---|
| a move's juice | `MoveJuice::impact_frame: Some("key")` in powers data. The move's next hit that connects within `rules.move_window` s fires it | the key |
| power crit | `PowerEvent::Crit { fx }` | `crit_styles` (`BlackFlash` → `black_flash`) |
| KO blow | `Hit { lethal: true }` from a player | `trigger_styles.ko` |
| Signature-tier hit | `Hit` whose construct is `presence::Tier::Signature` (e.g. Hollow Purple landing) | `element_styles[element]`, else `trigger_styles.signature` |
| perfect counter / parry | `OpEvent::Parried { kind: Counter }` | `trigger_styles.counter` |
| heavy weapon | see above | `trigger_styles.heavy_weapon` |
| beam-clash detonation | `PalmBeam(phase: Detonate)` | `trigger_styles.beam_clash` |
| domain sure-hit | `DomainEvent::Strike { victim: Some(_) }` | `trigger_styles.domain` |

### Move data keys (bd_sim `powers/data.rs`; presentation plus one generic sim flag)

```ron
(name: "black flash", crit: Some((2.5, BlackFlash)),          // every hit this move lands crits (× mult, the fx)
 ops: [...],
 juice: Some((impact_frame: Some("black_flash"), ...))),     // this move's connect plays an impact frame
```

* `MoveDef::crit: Option<(f32 /*mult*/, CritFx)>` is a GENERIC guaranteed crit for every hit the
  move's ops land while it runs. It announces `PowerEvent::Crit` with that fx, the same as a
  `CritChance` roll. It's deterministic and does nothing when unset.
* `MoveJuice::impact_frame: Option<String>` is a style key. It's never read by the sim.

## Who sees what

* **Attacker and victim**: the full-screen frame.
* **Bystanders** within `rules.bubble_range` m who can see the point: a *bubble* of the same
  treatment, anchored in the world around the impact (`bubble_radius` m, projected). A shockwave ring
  bursts out of it. A fast FFA stays readable because the rest of the screen stays normal.
* **Everyone else**: nothing.

## Frequency cap and photosensitivity (WCAG 2.3.1)

* At most **3 frames in any 1 s window**, for everyone and every pref (`rules.max_per_sec`, clamped
  to ≤ 3). Frames are also at least `rules.min_gap` s apart, and the same attacker gets at most one
  per `rules.same_attacker_gap` s. Rapid hits never strobe. A frame never alternates polarity
  mid-hold (one change in, one change out).
* **Settings → Reduce flashes** (`ComfortSettings::reduce_flashes`):
  * `Off`: the full treatment.
  * `Reduced`: no luminance inversion, capped contrast and strength, a ~0.1 s fade in, a slow
    fade out, no chromatic split. Max 1 per second.
  * `None`: no full-screen or bubble treatment. Hit-stop, lightning and sounds stay.
  * `SCREEN FLASHES` at 0 % (or the Comfort preset) counts as `None`.

## Frame timeline

`hold` = lerp(`hold_frames.0`, `hold_frames.1`, weight) × `frame_secs` × style `hold_mult`. That's
2–5 frames at 30 fps by default. Attacker and victim visuals are held (`HitFreeze`) and burn in as
the hot silhouettes. Then the `release` s snap back: the treatment decays, the zoom punch eases out,
and the style's `sfx_out` plays (Black Flash: the suck during the hold, the crack on release).
