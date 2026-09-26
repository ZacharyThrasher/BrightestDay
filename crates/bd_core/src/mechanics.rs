//! The closed set of things Brightest Day knows how to *do*.
//!
//! This is the boundary between vocabulary and mechanics. The lexicon (built from ConceptNet, WordNet,
//! corpora, PokeAPI…) may only answer "which of these does word X mean, and with what parameters?".
//! What each variant means physically is decided exclusively by `bd_sim`. Adding a variant here is a game
//! design decision, never a data-import decision.

use serde::{Deserialize, Serialize};

/// The shape family a construct is built from. Each form has a procedural voxel generator (bd_sim) and a
/// default way of being used. Forms are grouped by how the player relates to the construct.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Form {
    // --- Held: wielded in the ring hand, LMB/RMB use it ---
    Hammer,
    Sword,
    Axe,
    Spear,
    Shield,
    Gauntlet,
    Whip,
    Gun,
    Bow,
    Cannon,
    Staff,
    Disc,
    // --- Worn: attach to the body, change movement ---
    Wings,
    Parachute,
    Jetpack,
    Cape,
    // --- Creatures: autonomous summons that hunt the owner's enemies ---
    Biped,
    Kaiju,
    Quadruped,
    Serpent,
    Bird,
    Fish,
    Arachnid,
    Insect,
    Blob,
    // --- Structures: placed in the world ---
    Wall,
    Ramp,
    Pillar,
    Dome,
    Cage,
    Platform,
    Trampoline,
    // --- Objects: thrown, dropped or launched ---
    Ball,
    Anvil,
    Bomb,
    Boulder,
    Rocket,
    Vehicle,
    /// Unknown word: a generic glowing lump. Still does something (a thrown blob).
    Blob0,
}

impl Form {
    pub fn category(self) -> FormCategory {
        use Form::*;
        match self {
            Hammer | Sword | Axe | Spear | Shield | Gauntlet | Whip | Gun | Bow | Cannon | Staff
            | Disc => FormCategory::Held,
            Wings | Parachute | Jetpack | Cape => FormCategory::Worn,
            Biped | Kaiju | Quadruped | Serpent | Bird | Fish | Arachnid | Insect | Blob => {
                FormCategory::Creature
            }
            Wall | Ramp | Pillar | Dome | Cage | Platform | Trampoline => FormCategory::Structure,
            Ball | Anvil | Bomb | Boulder | Rocket | Vehicle | Blob0 => FormCategory::Object,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormCategory {
    Held,
    Worn,
    Creature,
    Structure,
    Object,
}

/// Mechanic tags: composable behaviours layered onto a form. A hammer that `Returns` and is `Electric`
/// is Mjolnir; a `Shield` that `Returns` and `Ricochets` is Captain America's shield.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Mechanic {
    /// Blunt, heavy: big knockback, craters on ground slam, slow wind-up.
    Blunt,
    /// Edged: fast, cuts through constructs, low knockback.
    Blade,
    /// Pointed: lunges, long reach, pierces shields.
    Pierce,
    /// Blocks hits from the front; bash on attack.
    Guard,
    /// Can be thrown and comes back to the owner.
    Returning,
    /// Bounces between surfaces/targets when thrown.
    Ricochet,
    /// Fires projectiles.
    Shoots,
    /// Fires a continuous beam.
    Beam,
    /// Detonates: carves a crater, radial knockback.
    Explodes,
    /// Seizes a target (player or construct) and can throw it.
    Grab,
    /// Latches and yanks toward the owner (whip, lasso, web).
    Pull,
    /// Reduces fall speed dramatically (parachute, umbrella, feather).
    SlowFall,
    /// Converts falling into forward speed (wings, glider, cape).
    Glide,
    /// Provides thrust (jetpack, rocket).
    Thrust,
    /// Bounces anything that lands on it.
    Bounce,
    /// Launches whoever steps on it along its facing.
    Launch,
    /// Blocks movement and projectiles; takes hits for you.
    Barrier,
    /// Encloses and holds a target briefly.
    Trap,
    /// Seeks its target.
    Homing,
    /// Has a tail that sweeps (scorpion, dragon, kaiju).
    ArticulatedTail,
    /// Creature flies.
    Flies,
    /// Charges in a line and rams (horns, rhino, bull).
    Charge,
    /// Stomps: ground shockwave (kaiju, elephant).
    Stomp,
    /// Breathes its element in a cone (dragon, Godzilla, Charizard).
    Breath,
    /// Many small copies (bees, bats, piranhas). Count comes from `Recipe::count`.
    Swarm,
    /// Spins (drill, tornado, saw) — continuous contact damage.
    Spin,
    /// Dropped from above onto the target (anvil, piano, meteor).
    DropFromSky,
    /// Sticks to what it hits (web, glue, slime).
    Sticky,
    /// Very fast (cheetah, bullet train, the Flash).
    Fast,
    /// Very heavy (anvil, elephant, tank): more impact, slower.
    Heavy,
    /// Constricts/wraps (python, tentacle).
    Constrict,
}

/// Elemental flavour. Changes effects/particles and adds a status on hit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Element {
    /// Pure hard light (default).
    #[default]
    Light,
    Fire,
    Electric,
    Ice,
    Water,
    Wind,
    Earth,
    Poison,
    Psychic,
    Shadow,
    /// Godzilla-style radiation / atomic.
    Atomic,
}

/// Where a construct's voxel model comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelSource {
    /// Procedurally generated from form + tags + scale. `seed` varies proportions deterministically so
    /// "sword" and "katana" are recognisably different swords.
    Procedural { seed: u32 },
    /// Curated voxel model id in `assets/icons.ron` (Mjolnir, Cap's shield, Godzilla, Pikachu…).
    Icon(String),
}

/// A fully resolved construct description. Produced by the lexicon, validated/clamped and executed by the
/// sim. Contains **no behaviour** — only which closed-set mechanics apply and their parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Display name, e.g. "Mjolnir", "Charizard", "sledgehammer".
    pub name: String,
    pub form: Form,
    /// Sorted, unique.
    pub mechanics: Vec<Mechanic>,
    pub element: Element,
    /// Characteristic size in metres (length for items, height for creatures). The sim clamps it.
    pub scale: f32,
    /// Weight class in kg (drives impact). The sim clamps it.
    pub mass: f32,
    /// Number of copies (swarms, "three hammers"). The sim clamps it.
    pub count: u8,
    /// Optional secondary tint for icons (e.g. Cap's shield stripes), sRGB 0..1; blended with corps colour.
    pub accent: Option<[f32; 3]>,
    pub model: ModelSource,
}

impl Recipe {
    pub fn has(&self, m: Mechanic) -> bool {
        self.mechanics.binary_search(&m).is_ok()
    }

    /// Normalise: sort + dedup mechanics, sanitise NaNs. Always call after building a recipe by hand.
    pub fn normalized(mut self) -> Self {
        self.mechanics.sort();
        self.mechanics.dedup();
        if !self.scale.is_finite() || self.scale <= 0.0 {
            self.scale = 1.0;
        }
        if !self.mass.is_finite() || self.mass <= 0.0 {
            self.mass = 10.0;
        }
        self.count = self.count.max(1);
        self
    }

    /// The fallback for words we genuinely cannot place.
    pub fn unknown(word: &str) -> Self {
        Recipe {
            name: word.to_string(),
            form: Form::Blob0,
            mechanics: vec![],
            element: Element::Light,
            scale: 1.0,
            mass: 10.0,
            count: 1,
            accent: None,
            model: ModelSource::Procedural { seed: 0 },
        }
    }
}
