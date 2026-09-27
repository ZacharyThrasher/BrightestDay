# TOOLBOX → PRESENTATION API (TOOLBOX CORE; for the PRESENTATION worker)

What the sim exposes so HUD / move visuals / audio can present every move — held, borrowed, kit —
and chains. Everything lives in `bd_sim::powers::{toolbox, chain}`; all of it is host-authoritative
and replicated (events to everyone, the own-toolbox view to its owner only).

## 1. `PowerEvent::Fired` — ANY move fired (held, borrowed, kit)

```rust
GameEvent::Power(PowerEvent::Fired {
    player: PlayerId,
    mv: MoveRef,          // global move id: { power: u16, tier: u8, slot: u8 }
    how: FireHow,         // Held | Borrowed | Kit
    op: OpKind,           // the op the move leads with (Cast, Blink, Impulse, Shove, Explode, …, Other = a generic ops/ op)
    element: Element,     // first op with an element (a Cast's recipe element, Shove / Explode element), else Light
    from: Vec3,           // mover's feet BEFORE the move
    to: Vec3,             // mover's feet right AFTER the ops ran (a blink's arrival; dashes start moving next tick)
    dir: Vec3,            // the aim it fired along (host-validated crosshair ray / look)
    step: u8,             // steps of the current chain fired before this one (0 = first / not a chain)
    sparks: u8,           // what a borrowed / kit move cost (0 for held moves)
})
```

* Everything else about the move is DATA: `mv.resolve(PowerTable::get()) -> Option<(&PowerDef, &MoveDef)>`
  gives `MoveDef { name, ops, juice, fx, tags, cost, cooldown }` and the power (`fx`, `kind`,
  `companion`, `ascend`…). For `OpKind::Other`, match on `MoveDef::ops` yourself (generic ops from
  `powers/ops/` also send their own `PowerEvent::Op(OpEvent::…)` beats).
* Held moves still ALSO send the old `PowerEvent::Move { player, power, slot }` (the HUD uses it).
* A held move of the ACTIVE power: `mv.tier` is its tier if that tier has its own move list, else 0.
* **Held context for borrowed moves** (decided): a borrowed / kit move of a power that normally
  needs its holder's context is run WITHOUT that context in the sim — ops are world ops. When the
  power has a `companion` (a Stand) and `how != Held`, present a GHOST of the companion for the
  beat (the stand steps out, throws the move, fades ~0.6 s) — presentation only; nothing is spawned
  in the sim. `Transform` / `Tier` ops never run on a borrow (a borrow never transforms, never
  grants); the ascend moment is not triggered by a borrow — only the move's own `juice`.

## 2. `PowerEvent::ChainStopped { player, why: PowerError }`

A pending chain was cut short: `NotEnoughWillpower` (out of sparks), `Dead` (KO), `Busy` (stunned,
or a new command replaced it). Fizzled steps (cooldown, nothing to hit) are skipped silently.

## 3. Own toolbox view — `ToolboxSnap` (recent borrowed cooldowns + the pending chain)

`OwnState::toolbox: Option<ToolboxSnap>` (owner-only in snapshots; `None` while idle, so nothing on
the wire changes for players who never borrow). In practice / host also `World::toolbox_view(player)`.

```rust
pub struct ToolboxSnap {
    pub cds: [(u32, u8, u8); 8],   // (packed MoveRef + 1, cooldown left 0..=255 of full, repeats in the staleness window); 0 = empty
    pub queue: [u32; 8],           // pending steps, front first: MoveRef::pack()+1 | QUEUE_HELD|sel | QUEUE_CAST; 0 = none
    pub pending: u8,               // steps pending in total
    pub active: u8,                // current move's active phase left, 1/100 s
    pub chain: u16,                // current chain id (bumps per command)
    pub fired: u8,                 // steps of it fired so far
}
```

`MoveRef::unpack(v - 1)` decodes a packed entry. A repeat count ≥ 1 means the next use of that move
costs +1 spark per repeat (the staleness nudge — worth a subtle "stale" tint on its icon).

## 4. Pricing (what a borrowed / kit move costs)

`chain::move_tier(&MoveDef) -> Option<Tier>` (the same five presence tiers as casts, by what the
move does on screen) and `chain::move_sparks(&SparkTuning, &MoveDef, uses, bar)` → whole sparks.
A cast-only move returns `None` / 0: its Cast ops pay the cast pipeline's own presence-tier price.

## 5. Phrase readings (for the console echo / preview)

`toolbox::MoveIndex::shipped().read(PowerTable::get(), Some(&player.power), text) -> Reading`
(`Held(PowerPhrase) | Kit(MoveRef) | Borrow(MoveRef) | Plain`) and `toolbox::split_chain(text, is_move)`.
