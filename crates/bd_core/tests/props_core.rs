//! Property tests for the bd_core contract (see docs/TESTING.md).
//!
//! - `Recipe::normalized` is idempotent and always yields sane numbers (finite, positive, in range),
//!   recursively through payloads.
//! - `raycast` never panics and always terminates, for *any* f32 input (NaN, ±inf, 1e30 max
//!   distance on an infinite empty world), and agrees with a brute-force march on small worlds.

use bd_core::corps::Corps;
use bd_core::input::look_dir;
use bd_core::mechanics::{Element, Feature, Form, Mechanic, ModelSource, Recipe, Temperament};
use bd_core::voxel::{AIR, Aabb, Material, VoxelQuery, raycast, voxel_center, world_to_voxel};
use bd_core::{IVec3, VOXEL_SIZE, Vec3};
use proptest::prelude::*;
use std::cell::Cell;

/// Case count × `BD_PROP_SCALE` (default 1; `tools/check.ps1 -Deep` sets 10).
fn scaled(n: u32) -> u32 {
    std::env::var("BD_PROP_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).map_or(n, |k| n.saturating_mul(k.max(1)))
}

// ------------------------------------------------------------------------------------ strategies

/// Any f32 with the nasty values heavily over-represented.
fn wild_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        3 => any::<f32>(),
        2 => -1e4f32..1e4,
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => Just(f32::NEG_INFINITY),
        1 => Just(0.0f32),
        1 => Just(-0.0f32),
        1 => Just(f32::MAX),
        1 => Just(f32::MIN),
        1 => Just(f32::MIN_POSITIVE),
        1 => Just(1e30f32),
        1 => Just(-1e30f32),
    ]
}

fn wild_vec3() -> impl Strategy<Value = Vec3> {
    (wild_f32(), wild_f32(), wild_f32()).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

const FORMS: &[Form] = &[
    Form::Hammer, Form::Sword, Form::Axe, Form::Spear, Form::Shield, Form::Gauntlet, Form::Whip, Form::Gun,
    Form::Bow, Form::Cannon, Form::Staff, Form::Disc, Form::Wings, Form::Parachute, Form::Jetpack, Form::Cape,
    Form::Biped, Form::Kaiju, Form::Quadruped, Form::Serpent, Form::Bird, Form::Fish, Form::Arachnid,
    Form::Insect, Form::Blob, Form::Wall, Form::Ramp, Form::Pillar, Form::Dome, Form::Cage, Form::Platform,
    Form::Trampoline, Form::Ball, Form::Anvil, Form::Bomb, Form::Boulder, Form::Rocket, Form::Vehicle,
    Form::Blob0,
];

const MECHANICS: &[Mechanic] = &[
    Mechanic::Blunt, Mechanic::Blade, Mechanic::Pierce, Mechanic::Guard, Mechanic::Returning,
    Mechanic::Ricochet, Mechanic::Shoots, Mechanic::Beam, Mechanic::Explodes, Mechanic::Grab, Mechanic::Pull,
    Mechanic::SlowFall, Mechanic::Glide, Mechanic::Thrust, Mechanic::Bounce, Mechanic::Launch,
    Mechanic::Barrier, Mechanic::Trap, Mechanic::Homing, Mechanic::ArticulatedTail, Mechanic::Flies,
    Mechanic::Charge, Mechanic::Stomp, Mechanic::Breath, Mechanic::Swarm, Mechanic::Spin,
    Mechanic::DropFromSky, Mechanic::Sticky, Mechanic::Fast, Mechanic::Heavy, Mechanic::Constrict,
    Mechanic::Walks, Mechanic::Piloted, Mechanic::Portal, Mechanic::Attract, Mechanic::Repel, Mechanic::Shrink,
    Mechanic::Grow, Mechanic::Link, Mechanic::Transforms,
];

const FEATURES: &[Feature] = &[
    Feature::Wheels, Feature::Treads, Feature::Pistons, Feature::Gears, Feature::Boiler, Feature::Smokestack,
    Feature::Pipes, Feature::Exhaust, Feature::Propeller, Feature::Rotor, Feature::Jet, Feature::Turbine,
    Feature::Sails, Feature::Mast, Feature::Cockpit, Feature::Cab, Feature::Windows, Feature::Turret,
    Feature::Barrels, Feature::Drill, Feature::Saw, Feature::CraneArm, Feature::Chains, Feature::Antenna,
    Feature::Dish, Feature::Headlight, Feature::Plow, Feature::Fins, Feature::Spikes, Feature::Armor,
    Feature::Bell, Feature::Cage,
];

const ELEMENTS: &[Element] = &[
    Element::Light, Element::Fire, Element::Electric, Element::Ice, Element::Water, Element::Wind,
    Element::Earth, Element::Poison, Element::Psychic, Element::Shadow, Element::Atomic,
];

const TEMPERAMENTS: &[Temperament] = &[
    Temperament::Calm, Temperament::Aggressive, Temperament::Fearful, Temperament::Protective,
    Temperament::Playful, Temperament::Lazy, Temperament::Chaotic, Temperament::Sneaky,
];

fn leaf_recipe() -> impl Strategy<Value = Recipe> {
    (
        (".{0,12}", prop::sample::select(FORMS), prop::collection::vec(prop::sample::select(MECHANICS), 0..12)),
        (prop::sample::select(ELEMENTS), wild_f32(), wild_f32(), any::<u8>()),
        (proptest::option::of((wild_f32(), wild_f32(), wild_f32())), any::<u32>()),
        (prop::collection::vec(prop::sample::select(FEATURES), 0..10), prop::sample::select(TEMPERAMENTS), wild_f32()),
    )
        .prop_map(|((name, form, mechanics), (element, scale, mass, count), (accent, seed), (features, temperament, intensity))| {
            Recipe {
                name,
                form,
                mechanics,
                element,
                scale,
                mass,
                count,
                accent: accent.map(|(r, g, b)| [r, g, b]),
                model: if seed % 3 == 0 { ModelSource::Icon(format!("icon{seed}")) } else { ModelSource::Procedural { seed } },
                payload: None,
                grants: None,
                features,
                temperament,
                intensity,
            }
        })
}

/// Recipes with payload chains up to 4 deep.
fn recipe() -> impl Strategy<Value = Recipe> {
    leaf_recipe().prop_recursive(4, 8, 1, |inner| {
        (leaf_recipe(), inner).prop_map(|(mut outer, inner)| {
            outer.payload = Some(Box::new(inner));
            outer
        })
    })
}

// --------------------------------------------------------------------------------- recipe checks

fn assert_sane(r: &Recipe, depth: usize) {
    assert!(depth < 64, "payload chain unexpectedly deep");
    assert!(r.scale.is_finite() && r.scale > 0.0, "scale {}", r.scale);
    assert!(r.mass.is_finite() && r.mass > 0.0, "mass {}", r.mass);
    assert!(r.count >= 1);
    assert!(r.intensity.is_finite() && (0.0..=1.0).contains(&r.intensity), "intensity {}", r.intensity);
    assert!(r.mechanics.windows(2).all(|w| w[0] < w[1]), "mechanics sorted+unique");
    assert!(r.features.windows(2).all(|w| w[0] < w[1]), "features sorted+unique");
    if let Some(a) = r.accent {
        assert!(a.iter().all(|c| c.is_finite() && (0.0..=1.0).contains(c)), "accent {a:?}");
    }
    for m in &r.mechanics {
        assert!(r.has(*m));
    }
    if let Some(p) = &r.payload {
        assert_sane(p, depth + 1);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(512), ..ProptestConfig::default() })]

    #[test]
    fn normalized_is_sane_and_idempotent(r in recipe()) {
        let once = r.clone().normalized();
        assert_sane(&once, 0);
        let twice = once.clone().normalized();
        // Bitwise idempotence (PartialEq would treat any leftover NaN as unequal, which is also a bug).
        prop_assert_eq!(&once, &twice);
        // Normalisation never changes identity fields.
        prop_assert_eq!(&once.name, &r.name);
        prop_assert_eq!(once.form, r.form);
        prop_assert_eq!(once.element, r.element);
        prop_assert_eq!(&once.model, &r.model);
    }

    #[test]
    fn normalized_keeps_valid_values(scale in 0.001f32..1000.0, mass in 0.001f32..1e6, count in 1u8..=255, intensity in 0.0f32..=1.0) {
        let mut r = Recipe::unknown("x");
        r.scale = scale;
        r.mass = mass;
        r.count = count;
        r.intensity = intensity;
        let n = r.normalized();
        prop_assert_eq!(n.scale, scale);
        prop_assert_eq!(n.mass, mass);
        prop_assert_eq!(n.count, count);
        prop_assert_eq!(n.intensity, intensity);
    }

    #[test]
    fn corps_index_roundtrip(i in any::<u8>()) {
        let c = Corps::from_index(i);
        prop_assert!(c.index() < 8);
        prop_assert_eq!(Corps::from_index(c.index()), c);
        prop_assert!(c.color().iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn look_dir_is_unit_for_finite_angles(yaw in -1e4f32..1e4, pitch in -1e4f32..1e4) {
        let d = look_dir(yaw, pitch);
        prop_assert!((d.length() - 1.0).abs() < 1e-3, "{:?}", d);
    }

    #[test]
    fn geometry_helpers_never_panic(p in wild_vec3(), h in wild_vec3()) {
        let _ = world_to_voxel(p);
        let b = Aabb::from_center_half(p, h);
        let _ = b.voxel_range();
        let _ = b.intersects(&b);
        let _ = b.translate(h).center();
    }
}

// -------------------------------------------------------------------------------------- raycasts

/// A world that counts queries and fails loudly if a raycast runs away.
struct Counting<'a, W: VoxelQuery> {
    inner: &'a W,
    calls: Cell<u64>,
}

/// Generous bound: a legitimate ray within a 1e6 m world needs ≤ ~6e6 voxel steps.
const QUERY_LIMIT: u64 = 20_000_000;

impl<W: VoxelQuery> VoxelQuery for Counting<'_, W> {
    fn material(&self, v: IVec3) -> Material {
        let n = self.calls.get() + 1;
        self.calls.set(n);
        assert!(n < QUERY_LIMIT, "raycast did not terminate (>{QUERY_LIMIT} voxel queries)");
        self.inner.material(v)
    }
}

struct Empty;
impl VoxelQuery for Empty {
    fn material(&self, _: IVec3) -> Material {
        AIR
    }
}

struct Floor;
impl VoxelQuery for Floor {
    fn material(&self, v: IVec3) -> Material {
        if v.y < 0 { 1 } else { AIR }
    }
}

/// Small dense world for brute-force comparison.
struct Grid {
    cells: Vec<Material>,
}
const G: i32 = 12;
impl Grid {
    fn from_bits(bits: &[bool]) -> Self {
        Self { cells: bits.iter().map(|b| if *b { 1 } else { AIR }).collect() }
    }
}
impl VoxelQuery for Grid {
    fn material(&self, v: IVec3) -> Material {
        if v.cmplt(IVec3::ZERO).any() || v.cmpge(IVec3::splat(G)).any() {
            return AIR;
        }
        self.cells[(v.x + G * (v.z + G * v.y)) as usize]
    }
}

fn check_ray<W: VoxelQuery>(w: &W, o: Vec3, d: Vec3, max: f32) {
    let c = Counting { inner: w, calls: Cell::new(0) };
    if let Some(h) = raycast(&c, o, d, max) {
        assert!(w.is_solid(h.voxel), "hit a non-solid voxel");
        assert!(h.t >= 0.0, "negative t");
        if max.is_finite() {
            assert!(h.t <= max.max(0.0) + 1e-3, "hit beyond max_dist: t={} max={max}", h.t);
        }
        assert!(h.normal.abs().element_sum() <= 1, "normal must be axis aligned or zero");
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(2048), ..ProptestConfig::default() })]

    /// Any input at all: must terminate and never panic.
    #[test]
    fn raycast_terminates_on_empty_world(o in wild_vec3(), d in wild_vec3(), max in wild_f32()) {
        check_ray(&Empty, o, d, max);
    }

    #[test]
    fn raycast_terminates_on_floor(o in wild_vec3(), d in wild_vec3(), max in wild_f32()) {
        check_ray(&Floor, o, d, max);
    }
}

#[test]
fn raycast_huge_max_dist_on_empty_world_is_bounded() {
    for (o, d) in [
        (Vec3::ZERO, Vec3::X),
        (Vec3::new(0.3, 0.7, 0.1), Vec3::new(1.0, 2.0, 3.0)),
        (Vec3::splat(-1e6), Vec3::new(-0.2, 1.0, 0.0)),
        (Vec3::splat(1e9), Vec3::NEG_ONE),
    ] {
        for max in [1e30, f32::MAX, f32::INFINITY, f32::NAN] {
            let c = Counting { inner: &Empty, calls: Cell::new(0) };
            assert!(raycast(&c, o, d, max).is_none());
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(1024), ..ProptestConfig::default() })]

    /// On small random worlds the DDA agrees with a fine brute-force march: the reported voxel is
    /// solid, and no solid voxel lies on the ray noticeably before the hit.
    #[test]
    fn raycast_matches_brute_force(
        bits in prop::collection::vec(prop::bool::weighted(0.08), (G * G * G) as usize),
        o in (0.1f32..5.9, 0.1f32..5.9, 0.1f32..5.9),
        d in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
        max in 0.0f32..12.0,
    ) {
        let w = Grid::from_bits(&bits);
        let o = Vec3::new(o.0, o.1, o.2);
        let d = Vec3::new(d.0, d.1, d.2);
        prop_assume!(d.length() > 1e-3);
        let dir = d.normalize();
        let hit = raycast(&w, o, d, max);
        let end = hit.map_or(max, |h| h.t);
        // March to just before the hit (or to max): nothing solid may be found.
        let step = 0.002;
        let mut t = 0.0;
        while t < end - 0.01 {
            let v = world_to_voxel(o + dir * t);
            prop_assert!(!w.is_solid(v), "missed solid voxel {:?} at t={} (hit {:?})", v, t, hit);
            t += step;
        }
        if let Some(h) = hit {
            prop_assert!(w.is_solid(h.voxel));
            prop_assert!(h.t <= max + 1e-4);
            let p = o + dir * h.t;
            prop_assert!((p - h.point).length() < 1e-3);
            // The hit point lies on the surface of the hit voxel.
            let c = voxel_center(h.voxel);
            prop_assert!(((h.point - c).abs().max_element() - VOXEL_SIZE * 0.5).abs() < 1e-3 || h.t == 0.0);
        }
    }
}
