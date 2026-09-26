# Icon files (curated pop culture)

Files: `assets/icons.ron` (lexicon engineer's base set) and `assets/icons/*.ron` (domain packs). Each file
is a RON list of `IconEntry`:

```
IconEntry(
    names: ["mjolnir", "thor's hammer", "mjölnir"],   // lowercase lookup keys; first = canonical id
    recipe: <bd_core::mechanics::Recipe>,              // serde RON of the real type (fields below)
    shape: Some(<bd_core::shape::ShapeSpec>),          // optional hand-authored model; None = procedural
    alt_shape: None,                                   // optional second mode (Transforms icons), see below
    source: "Marvel (Thor)",                            // franchise/credit, for the explanation note
)
```

**Two-mode icons (`Mechanic::Transforms`, bd_core v1.2, COMBAT.md §5d-bis):** `shape` is the alt mode
(vehicle / beast / boombox…) and `alt_shape: Some(<ShapeSpec>)` the robot mode. The recipe must list
`Transforms`. Panels that move between modes use the **same `part` ids** in both shapes (cab → chest,
doors → back-wings, wheels → shoulders…); parts only present in one mode (e.g. the robot head that
unfolds from inside) get their own ids. `alt_shape` defaults to `None` and may be omitted.
`bd_lexicon` exposes it as `Resolution::alt_shape`.

`Recipe` fields (defaults may be omitted: `payload`, `features`, `temperament`, `intensity`):
`name`, `form` (Form variant), `mechanics` ([Mechanic]), `element` (Element), `scale` (m), `mass`
(kg), `count` (u8), `accent` (Some((r,g,b)) sRGB 0..1 or None), `model` (`Icon("<canonical id>")` when a
shape is given, else `Procedural(seed: N)`), `payload` (Some(Recipe) / None), `features` ([Feature]),
`temperament` (Temperament), `intensity` (0..1).

`ShapeSpec`: `prims: [Prim(kind: Box|Ellipsoid|Cylinder|Cone, center: (x,y,z), size: (x,y,z),
rot_deg: (x,y,z), tint: Primary|Accent|Dark|Core, part: u8, subtract: bool)]`, `parts: [Part(id, parent,
pivot: (x,y,z), role: Head|Jaw|Tail|LegFrontLeft|...|Weapon|Other)]`. Units: construct voxels
(0.25 m) — a 1 m hammer is ~4 units long. Origin = grip for held items, feet-centre for creatures and
structures; +Y up, −Z forward. Keep silhouettes bold and readable at 30 m; 5–40 prims is typical.

Example (verified to parse against bd_core v1):
```ron
[
    IconEntry(
        names: ["mjolnir", "thor's hammer", "mjölnir"],
        recipe: (
            name: "Mjolnir",
            form: Hammer,
            mechanics: [Blunt, Returning],
            element: Electric,
            scale: 0.9,
            mass: 40.0,
            count: 1,
            accent: Some((0.75, 0.75, 0.8)),
            model: Icon("mjolnir"),
        ),
        shape: Some((
            prims: [
                (kind: Cylinder, center: (0.0, 0.0, -1.5), size: (0.8, 3.0, 0.8), rot_deg: (90.0, 0.0, 0.0), tint: Dark),
                (kind: Box, center: (0.0, 0.0, -4.0), size: (3.2, 2.2, 2.2), tint: Primary),
                (kind: Box, center: (0.0, 0.0, -4.0), size: (3.4, 1.0, 2.4), tint: Accent),
                (kind: Box, center: (0.0, 0.0, 0.6), size: (0.3, 0.9, 0.3), tint: Accent),
            ],
        )),
        source: "Marvel (Thor)",
    ),
]
```

Validate: `cargo run --manifest-path tools/iconcheck/Cargo.toml -- assets/icons/<file>.ron` (exit 0 = OK; also
checks two-mode entries: `alt_shape` needs `shape`, `Transforms`, and shared part ids).
