# Feel reference (from the lazy TS prototype — a *taste* reference, not a spec)

> Owner: "don't copy the prototype — use it as an idea of the direction; assume you can do better."
> Nothing below is a requirement. Use it only to know roughly what "the movement felt weighty" meant, then
> design and tune our own. No code, structure, or art from the prototype is to be ported.

The owner liked the prototype's movement weight/momentum. These numbers are a starting point to beat,
not to copy. We run at 120 Hz (prototype: 60 Hz). Units m, s.

## Movement (Quake/Source)
- Gravity 22. Jump: `vy = max(vy,0) + 10` (apex ≈ 2.3 m). Friction skipped on the jump tick (bhop).
- Jump buffer 0.12 s; coyote 0.1 s (only if vy ≤ 0.5). Holding jump = auto-rehop on land (buffer).
- Ground accel (Source `Accelerate`): runSpeed 11, accel 10. Friction 5, stopSpeed 3.
- Air accel (Source `AirAccelerate`): air wish cap 1.4 m/s, airAccel 14 → air-strafing 11 → ~25 m/s in 8 s.
- Ground = normal.y > 0.7 and relative vy ≤ 3.5 (so ramps launch you).
- Slide: crouch at > 7 m/s; friction 0.25; gravity along slope; weak steering. (Improve: a small entry
  boost with cooldown, crouch hull.)
- Step-up 0.55 m (Quake step-slide); collide-and-slide 4–6 bumps, crease handling, overbounce 1.001.
- Soft cap 28 m/s: excess `e` decays `e -= (0.5e + 0.08e²)dt` (grace 1.6 s after launches, 0.35 s after
  knockback). Hard cap 70.
- Gas boost: 34 m/s² along look(+0.6·wish); 100 gas, drain 32/s, regen 40/s ground / 10/s air.
- Hard landing at vy < −18 → camera dip/shake.

## Tethers (AoTTG)
- Q/E independent, hold to stay attached; release keeps velocity. Range ~140 m, hook travel 320 m/s.
- Aim: camera ray through crosshair (to 210 m) then ray from hand to that point. (Improve: slight aim
  assist toward edges/voxel corners, visual preview of valid anchor on crosshair.)
- Rope = inextensible ratcheting constraint (never lengthens) + constant pull 16 m/s² toward anchor;
  reel adds 58 m/s² and shortens rope 26 m/s (min 1.5 m). Remove outward radial velocity when taut;
  keep tangential → swings/orbits. Leave ground when anchor dir y > 0.2.
- Measured: tether swing peak ~32–34 m/s; post-release keeps > 80% speed after 0.4 s.

## Flight (should be slower than skilled movement)
- Cruise 17 m/s, hover brake, slow overspeed bleed. Glide: lerp velocity toward look, small lift.

## Camera & input
- Prototype was third-person boom (5.2 m back + speed, 0.9 right, 0.35 up, wall pull-in).
- FOV base 78 + min(20, (speed−12)·0.3); kick springs. Source sensitivity units (0.022°/count).
- Raw mouse, no smoothing, ignore first 60 ms after cursor grab, spike filter on absurd deltas.
- Juice: trauma² shake decaying 1.4/s (short, never steals aim); hitstop 40–120 ms render-only with a
  budget (≤ 300 ms/s); FOV punch on tether attach (+2.2) and speed thresholds.

## Combat / impact / art
Deliberately NOT carried over. The owner considers the prototype's impact and art decisions poor. Design
impact from first principles (see DESIGN.md "Impact") and playtest it.

## Lessons
- Velocity is state; input only redirects it. Landing never zeroes speed.
- Flight cruise < momentum tech, or skill flattens.
- Constructs (ramps, launch pads, trampolines, wings) feed the same velocity/collision model as terrain.
- Nobody playtested the prototype: correct equations can still feel bad → we playtest and tune.
- Perf: batch draws, instance debris, precompile shaders at startup (first-use hitches are fatal to feel).
