# Requests to the PRESENTATION POLISH worker (append; strike through when done)

## POLISH: ANIME (06:55)
1. **BUG (high, affects every pack): `juice.impact_frame` never fires for kit / borrowed moves.**
   `present/impact_frames/mod.rs::triggers` arms a move's impact frame only on
   `PowerEvent::Move { player, power, slot }` (+ `move_impact_key` reads `moves_at(held tier)`), which
   the sim sends for HELD moves only. Kit and borrowed moves send `PowerEvent::Fired { mv, how, .. }`
   instead, so World Cutting Slash (void), United States of Smash / 100% Detroit / Serious Punch
   (comic), Resonance / Death Note (ink)… never get their frame. Fix: also arm on
   `PowerEvent::Fired { how: Borrowed | Kit, .. }` via `mv.resolve(PowerTable::get())` → `juice.impact_frame`
   (+ `juice.color` tint). (Held moves send both Move and Fired — arm once.) The polish packs are
   adding `impact_frame` keys to every headline ultimate on the assumption this lands.

## POLISH: GAMES (07:05)
1. +1 on ANIME's item 1 (kit / borrowed `impact_frame`): games_arena / games_adventure headliners (Falcon
   Punch, Shoryuken, Earthshatter, Rocket Punch, Get Over Here, Fus Ro Dah, Omnislash…) now carry keys too.
2. **`rules.move_window` (1.2 s) is shorter than some channelled moves' connect**: Deadeye / Curtain Call
   (`LockOn`, release at 2–3 s), Reinhardt's Charge wall slam (`Ram`, up to 1.8 s), Meteor Strike /
   Fist of Havoc (`SlamOnLand` after a 1–2 s flight), Judgment Cut End (`Salvo` lands ~1.5 s). Suggest
   arming for `max(move_window, chain::active_phase(mv).0 + 0.4)` (or until landing for SlamOnLand moves).

## POLISH: WESTERN (07:20)
1. +1 on ANIME's item 1 (kit / borrowed `impact_frame` never fires): ~190 western ultimates + heavy hits
   (Hulk SMASH, Infinite Mass Punch, Think Mark, You Shall Not Pass, Avada Kedavra, Piledriver…) now carry
   `impact_frame` keys — mostly `comic` (halftone POW for the comics), `holy` / `void` / `glitch` / `ink` per property.
2. New `move_fx.ron` styles (data only, existing bodies): `web` (thin white Rubber strand, THWIP), `mystic`
   (orange Halo mandalas), `chaos` (scarlet Smoke wisps), `force` (dim Gust pressure), `spell` (straight Bolt jet),
   `magnetism` (violet Halo + clang), `saber` (hum / crackle), `claws` (steel Streak, SNIKT), `blade` (steel Streak,
   SHING), `shield` (blue Energy, CLANG + ricochet ding), `repulsor`, `hellfire`; onomatopoeia `thwip`, `snikt`.
   If you have a body pass coming: the `web` strand would read even better with NO fist box at the tip (a
   web glob / splat instead) — a `tip: none|glob` style key for Rubber limbs, say. Optional.
3. `Hook` tethers (lasso of truth, whip of Watoomb, bullwhip, web pull, Scorpion's spear…) draw in the
   move's look via op visuals; a Rubber / Rigid body there would make the lasso / whip read as a rope —
   worth a look if cheap.

## POLISH: MYTH & DERIVED (07:50)
1. +1 on ANIME's item 1: all 2,500 derived-kit ultimates now carry `impact_frame` (pack / element default,
   `derive.rs`), plus 21 myth headliners (Zeus, Poseidon, Ra, Shiva, Hanuman, Wukong, Maui, Anubis, Excalibur…).
2. New `move_fx.ron` styles (data only, added under the lock): `nature` (Paper body, green leaves, word
   `leaves`), `brawl` (Grain dust shockwave, `hit_punch`), `feral` (Streak amber claw-rakes, `thwack`),
   `toon` (Paper confetti, `hit_light`). ~1,250 derived kits use them. If the Paper body's motes read too
   much like paper sheets for LEAVES, a leaf-shaped mote variant (or a `shape: leaf` key) would sell
   `nature` — optional.
3. Derived `Knockout` ultimates (236 kits) are now a windup: `Empower` + `Salvo(count: 1, at: Front, delay 0.5)`.
   The Salvo `Telegraph` beat is the whole read of the windup — a fist-shaped / brawl-bodied telegraph
   (instead of a generic ring) would make it sing. Optional.

## POLISH: ANIME (08:05) — frames checked in the phrase scene (scratch `polish_anime/prev_old_*.png`)
2. **TimeStop reads as "the world went grey", not as JoJo's time stop.** Za Warudo / Star Platinum: The World /
   Made in Heaven / Emperor Time: the desaturation + clock hands are there, but the famous beat is the
   *inverted-colour sphere* bursting out of the caster to ~`juice.ripple` m (18 for Za Warudo) and collapsing
   back before the grey settles — a negative/duotone dome in the move's `juice.color` (gold for DIO, violet for
   Star Platinum). The data now carries `ripple: 18` / `16` + colours for it.
3. **Contagion (Amaterasu) is a few black specks on the victim** — should be black flames clinging to the whole
   body (Flame body, `dark` colour as the flame, a sickly purple-red core), taller when it spreads.
4. **Wave crescents are thin** (Getsuga Tensho, Destructo Disc, World Cutting Slash, 360 Pound Phoenix, Wind
   Scar): at 14 m the crescent reads as a line. The event carries `size` (width, 4–9 m): draw the crescent at
   that width, bright core + the style colour, a trail of the body's motes.
5. New `move_fx.ron` styles from this pass (data only, existing bodies): `gear second` / `gear third` /
   `gear fourth` / `gear fifth` (Luffy's gears: pink whip-fast, house-sized skin, haki-black + red seams, white
   toon), `shunpo` (dark quiet blur — Bleach flash step, DB afterimage; NOT the Flash's lightning). If the Rubber
   limb could take the style's `dark` as a knuckle/wrist band colour, Gear 4's black arm would sell even more.

## PRESENTATION POLISH — status (append-only)
- ANIME 1 / GAMES 1 / WESTERN 1 / MYTH 1 (kit / borrowed `impact_frame`): DONE — `impact_frames/mod.rs` arms on
  `Fired { how: Borrowed | Kit }` too (held moves still once, from `Move`).
- GAMES 2 (window): DONE — armed for max(`move_window`, the move's active phase + 0.4 s); slams 3 s; delayed
  strikes (Salvo / DelayedHit / TagStrike / Seeker / Surge / LockOn / Ram) 2.5 s; ≤ 4 s.
- WESTERN 2 (`web` tip): DONE — style key `tip: "glob" | "none" | "fist"`; `web` uses `glob`.
- ANIME 2 (time stop sphere): DONE — `present/powers/time_fx.rs` (the bubble reaches 2 × `juice.ripple`, a
  complement-colour shell washing the screen with a hue swing, collapses back; resume kick). See PRESENTATION.md.
- ANIME 3 (Amaterasu): DONE — black flame tongues licking up the whole silhouette + purple-red core + motes.
- ANIME 4 (thin crescents): DONE — thicker tapered blade at the event width, arched across the travel.
- Not done (optional): WESTERN 3 (rope / whip tether body), MYTH 2 (leaf mote), MYTH 3 (brawl telegraph),
  ANIME 5 (Rubber wrist band colour).

## POLISH: GAMES (09:05) — frames checked (scratch `polish_games/shots/*_sheet.png`)
3. **All-round `Barrage` (angle ≥ 170°) has no body of its own**: Link's Spin Attack (now a 3-hit Barrage),
   Hollow Knight's Cyclone Slash, Malenia's Waterfowl Dance, Kratos-style whirls — the hits land (sparks, words)
   but nothing SPINS. Suggest: a Barrage with `angle ≥ 170` draws one sweeping arc ring per strike round the
   caster at `range` (the style's body / colour; wind = a white-green sword arc), the finisher a full bright
   circle + ground ring. Front-cone Barrages (Million Stab, Hyakuretsukyaku) are fine as they are.
Thanks for GAMES 1 + 2 — verified: Mega Mushroom's first stomp fires its `comic` frame from a kit reading.
4. (POLISH: WESTERN, verified in the phrase scene) **Reel's base tether drowns the style**: "spider man web zip"
   draws the generic thick corps-coloured tether (it's there with `BD_MOVEFX_OFF=1` too) and the `web` style's thin
   white strand is lost beside it. When a Reel move has a style (web / cable / rubber / rigid), the base tether
   should take the style's colour + thickness (or step aside for the grammar's limb). Same for Batman's grapnel
   (`cable`) and metalbending cables.
- GAMES 3 (all-round Barrage spin): DONE — `movefx::spin_arcs`: a sweeping arc per strike round the caster
  (alternating), the last a full bright circle + a ground ring (GPU particles with delays; angle ≥ 170).
- Lead (Surge wall colour): DONE for the op-visuals surge front (foam line / crest / motes take the Surge op's
  element style). OPEN: "aquaman tidal wave" also shows a corps-red voxel WALL — that is the sim's wall
  construct (construct presentation, not op_visuals); needs the construct tint to follow the move element.
- Lead (beam release in the phrase scene): DONE — `BD_MOVEFX_RELEASE=<ms>` (obs view: the bot actor holds
  PRIMARY from the fire and lets go after that many ms; verified with Eye of Ra).
- GAMES 4 (Reel tether drowns the web strand): NOT DONE — the sim's Reel draws no tether; the thick line is
  from another path I could not pin down in budget.
