# POWER HOTBAR — keys 1–4 as a dynamic power bar (POWER HOTBAR worker, 2026-09-27)

Code: `crates/bd_game/src/hotbar/` (`mod.rs` model + rules, `fire.rs` key firing + auto-bind intake,
`hud.rs` strip slots + Tab editor, `dismiss.rs` DISMISS BY SLOT, `scenario.rs` proof shots).
The old 1–4 quick-cast loadout IS the hotbar (`UiPrefs::loadout` phrases + `loadout_pins`).

## Data shape (for FIRST CALL / BURN ON DEATH / anyone reading the bar)

```rust
// bd_game::hotbar
pub struct Hotbar { pub slots: [Slot; 4], pub recent: Vec<Entry>, .. }   // a Bevy Resource
pub struct Slot {
    pub phrase: String,            // what the player said ("reinhardt ult")
    pub bind: SlotBind,            // the canonical reading (below)
    pub pinned: bool,              // auto-bind never replaces it
    pub state: SlotState,          // YOUR HOOK
    pub def: Option<&'static MoveDef>, pub power: Option<&'static PowerDef>,   // presentation
    pub live: Vec<ConstructId>,    // constructs this slot called (DISMISS BY SLOT)
    pub granted: Option<u16>,      // a power this slot granted
    ..
}
pub enum SlotBind { Empty, Phrase { signature: bool }, Move { mv: MoveRef, kit: bool },
                    Composed(ComposedMove), Twist(Box<TwistStep>) }
pub enum SlotState { Fresh, Burned, Claimed }
impl SlotBind { pub fn move_ref(&self) -> Option<MoveRef>; pub fn cd_key(&self) -> Option<u32>; }
```

**FIRST CALL / BURN ON DEATH hook:** write `slots[i].state` every frame from your replicated data
(a system in your module, `.before(bd_game::present::PresentSet::Sync)`), keyed by
`slot.bind.move_ref()` (named moves), `compose` spec (described moves) or `hotbar::norm(&slot.phrase)`
(constructs / signatures / characters: "optimus prime"). The HUD then draws:
- `Fresh`: normal.
- `Burned`: greyed / ash (the dim path), still pressable.
- `Claimed`: dimmed, colour pulled toward red.
And a NON-fresh slot fires through the FULL phrase reading (`Caster::cast(phrase)`), not the stored
reading, so whatever rival / echo reading you give that phrase applies to the key too. If your rule
lives entirely on the host (keyed by `MoveRef` / claim table), the stored reading already goes
through it — `Cmd::Toolbox` is exactly what the phrase sends.

## Rules
- A key press sends exactly the command saying the phrase sends (`fire::step_for`): same host path,
  cooldown, spark price, staleness. A held power's move fires as the HELD move while held.
- AUTO-BIND (Settings → Gameplay → HOTBAR → AUTO-BIND POWERS, on by default): a one-move line you
  SAID that the HOST confirmed (`PowerEvent::Fired` / a signature's construct) binds: already there →
  refreshed; else first empty; else the least recently used unpinned slot (ties → rightmost).
  Chains and plain constructs never auto-bind (you can still type a construct phrase into a slot in
  Customize, or drag it).
- Tab: cursor frees (`input::SoftCursor`: mouse look + mouse buttons pause, keyboard play goes on),
  names show under slots, a tray (recent moves, held powers' moves) shows beside the bar.

## TOTAL LIMIT (host match rule) — bd_sim `limit.rs`
`MatchRules::active_limit: u8` (0 = OFF, the default; lobby RING → TOTAL LIMIT: OFF / 2..8;
protocol v47). Caps how many ACTIVE things one player has at once.
- **Counts (one each):** every live construct you own that stands on its own (summons, creatures,
  followers, walls, held weapons, worn things, vehicles, clones — `parent.is_none()`, as the per-kind
  caps count); every power you HOLD (fruit, stand, technique / form, origin — its stand / transform
  body is part of it, not one more). `World::active_count(player)`.
- **Never counts:** one-shot moves once they ran (punch, blink, borrowed / kit / described moves),
  projectiles, timed zones / fields / portals moves leave (they expire), grafts onto something you
  have, fruit / origin pickups lying on the map.
- **Refusal:** at the cap a call that would ADD one more doesn't happen — before anything is paid
  (`CastError::AtLimit`, sent as `CastFailed`): a fresh construct (`spawn_cast`), a said power grant
  (`cast_grant`); a fruit you touch stays on the map. Anything that REPLACES is allowed (the total
  doesn't grow): a full per-kind construct cap (replace-oldest), `max_powers` pushing the oldest
  power out, a second stand replacing the first, re-selecting a held power, grafts.
  Per-kind caps keep working exactly as before; the total limit only refuses additions.
- **Feedback (client, wordless):** a soft "sputter", the limit pips (one small bar per allowed thing
  beside the hotbar, filled = out) throb warm, live slots pulse, the DISMISS key's cap shows beside
  the pips until it fades.
- Host and prediction run the same `World` code on the replicated rules.

## DISMISS BY SLOT — bd_game `hotbar/dismiss.rs`
- X (`Action::Dismiss`, the existing "dismiss all" key): if any slot has something out, opens a 1 s
  window (live slots pulse); a slot key dismisses that slot's things (its constructs via
  `Cmd::DismissOne`; a power the slot granted via `Cmd::DropPower` → `World::drop_power`, the stand /
  body leaves with it, a fruit pops back out); X again = dismiss all; the window running out = dismiss
  all (what X always did). Nothing live on the bar: X dismisses all at once, as before.
- Voice / typing: "dismiss 3", "dismiss slot three", "dismiss star platinum", "dismiss" /
  "dismiss everything" (`console::Caster::cast_heard` sends it to `DismissReq`; never casts).
- Slots track what they called: constructs spawned right after a line whose reading matches the slot
  (`Slot::live`), and the power a phrase slot grants (`Slot::granted`).
