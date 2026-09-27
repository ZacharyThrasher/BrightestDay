# Menu feel audit (MENU FEEL pass)

Owner feedback: *"the mouse input takes priority so when trying to go down the menu it snaps back up to
the mouse position"* and *"sliders > number"*.

Code: `crates/bd_game/src/menu/nav.rs` (arbitration state machine, key repeat, scroll window, slider
curves — unit-tested) and `crates/bd_game/src/menu/widgets.rs` (controls, hit-tested mouse, styling,
focus memory), hooked into `menu.rs`. Before / after: `menu_before_after.png`.

## Root cause of the snap-back

Every ↑/↓ rebuilt the whole menu (despawn + respawn every row). The freshly spawned row under a
*resting* pointer reported `Interaction::Hovered` as a change, and `menu_mouse` moved the cursor to
it — so focus jumped back to wherever the mouse happened to sit.

## Fixed

| Problem | Fix |
|---|---|
| Stationary mouse overrides keyboard focus | **Last-used device owns focus** (`nav::Arbiter`). Hover moves focus only on frames the pointer actually moves; after key / pad navigation it must travel 6 px from where it rested. Menus open keyboard-owned (a pointer resting over a row picks nothing). Clicks always count as mouse use. |
| Hover highlight fights key focus | Hover tints (tabs, segments, picker arrows, slider handle) only show while the mouse owns focus; with keys there is exactly one highlight — the focused row. |
| Wheel moved the *cursor* (and then hover snapped it back) | Wheel scrolls the **view**; focus is dragged along only as far as needed to stay visible (`nav::wheel`). Wheel over a slider adjusts that slider. |
| Bare numbers ("80%", "0.85") that players had to ask how to change | **Sliders** (track + fill + handle + readout; click-to-jump, drag, wheel, ←/→, d-pad) for every continuous / wide numeric setting: volumes, sensitivity (log track: 1.0 mid), ADS sensitivity / zoom speed, FOV, outline thickness, head size, camera shake / FOV kicks / landing dip / screen flashes / vignette, sparks (3–12, tick marks), lives, KOs to win, time limit, respawn, spawn shield, knockback, gravity, gas regen, construct caps (… → UNLIMITED notch), party rounds / KOs / time / points / pick timer. |
| ON / OFF read as text | **Toggle switch** (→ on, ← off, click / Enter flips). |
| 2–4 named options hidden behind arrow-cycling | **Segmented control** — every option visible, chosen one lit, click any (view, shoulder, ADS mode, comic text / style, shout words, reduce flashes, enemy %, music style, mode, preset, scoring, construct size, willpower, caps, held / worn…). Long lists (map, corps, accent) are pickers with clickable ‹ › arrows. |
| Knobs added later wouldn't get a control | Controls are **derived**: known numeric ranges come from `widgets::spec`; anything else is enumerated by stepping a scratch copy, so a new ON/OFF knob becomes a toggle, 2–4 options a segmented control, a long clamped stepper a slider — automatically. All changes still go through `step_knob` (values, clamps, save/load, host sync unchanged; tests pin slider ends to the stepper clamps). |
| Single-press only; long ranges tedious | Held ↑/↓/←/→ (and d-pad / stick) **repeat with acceleration** (~8/s → ~28/s); long holds on a slider take bigger steps. Held ↑/↓ stops at the list ends (a fresh press still wraps). |
| Dead zones between rows | Row hit areas cover the 4 px gaps. Picker arrows have padded hit boxes. |
| Every value change replayed the focused row's slide-in | The focused row keeps its settled offset across rebuilds (no jitter while dragging / holding). |
| Back lost your place | Going back returns focus to the row that led away (Settings → back lands on SETTINGS, not PRACTICE). |
| No controller in menus | D-pad / left stick navigate, A confirms, B backs out (also closes the pause menu), LB / RB switch settings tabs. |
| Weak click feedback | Clicked rows flash; slider moves tick with pitch rising along the track (throttled while dragging); hover focus changes tick softly. |

## Still open (not cheap / not this pass)

- **Map vote cards** (end screen, `lobby/scoreboard.rs`) are mouse-only: no keyboard / pad focus.
- **Party card pick** (`lobby/party.rs`) is keys-only (1–4 / slot binds): cards aren't clickable.
- **Text fields**: no caret movement (←/→ inside the text), no select-all / clear; the caret only
  redraws on the 0.5 s blink after hover-focusing a text row. OS key-repeat of a held S that lands on
  a text row types into it.
- **No confirmation** on QUIT, LEAVE MATCH or RESET KEYS TO DEFAULTS (instant, no undo).
- **Scrolling lists**: only `^` / `v` glyphs, no draggable scrollbar; the glyphs aren't clickable.
- **Footer hints** are keyboard words only (no controller glyphs); pad can't open the in-game pause
  (gameplay has no pad support yet).
- **Esc on Title / Lobby** does nothing (deliberate: no accidental leave) — consider a visible hint.
- The whole menu is still despawned and respawned on every change (fine for cost, but it rules out
  transitions and makes each change re-layout; a retained-mode row list would be the next step).
- Mouse hit-testing is geometric over the menu's own nodes; if a modal is ever drawn *over* the menu
  it must block these hit tests itself.
