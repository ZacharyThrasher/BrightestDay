//! Per-form data: validation clamps, default proportions, base damage/knockback, economy
//! categories. One table so the cast pipeline, the economy and the behaviours agree.
//!
//! Scale convention (matches `Recipe::scale`): the *characteristic size* in metres — length for held
//! items, height for creatures, width for walls, length for ramps, height for pillars, radius for
//! domes, diameter for objects.

use bd_core::Vec3;
use bd_core::mechanics::{Form, FormCategory};

/// Validation + default behaviour numbers for one form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FormSpec {
    pub default_scale: f32,
    pub min_scale: f32,
    pub max_scale: f32,
    /// Collision box *full* size at scale 1.0 (multiply by the clamped scale).
    pub dims: Vec3,
    /// Base damage / knockback (m/s) of the form's default attack or impact.
    pub damage: f32,
    pub knockback: f32,
    /// Default speed (m/s): throw speed for objects, walk speed at scale 2 m for creatures,
    /// projectile speed for ranged held items.
    pub speed: f32,
}

const fn fs(default_scale: f32, min_scale: f32, max_scale: f32, dims: Vec3, damage: f32, knockback: f32, speed: f32) -> FormSpec {
    FormSpec { default_scale, min_scale, max_scale, dims, damage, knockback, speed }
}

const HELD: Vec3 = Vec3::new(0.25, 0.25, 1.0);
const WORN: Vec3 = Vec3::new(0.6, 0.6, 0.3);
const LUMP: Vec3 = Vec3::new(1.0, 1.0, 1.0);

/// The table.
pub fn form_spec(f: Form) -> FormSpec {
    use Form::*;
    match f {
        // Held: scale = length. Ranged held items use `speed` for their shots.
        Hammer => fs(1.2, 0.5, 3.0, Vec3::new(0.35, 0.35, 1.0), 14.0, 16.0, 0.0),
        Sword => fs(1.1, 0.4, 2.5, HELD, 9.0, 8.0, 0.0),
        Axe => fs(1.0, 0.4, 2.5, HELD, 12.0, 11.0, 0.0),
        Spear => fs(2.2, 1.0, 4.0, Vec3::new(0.12, 0.12, 1.0), 10.0, 10.0, 0.0),
        Shield => fs(1.0, 0.5, 2.0, Vec3::new(0.9, 1.1, 0.12), 6.0, 12.0, 0.0),
        Gauntlet => fs(0.4, 0.25, 1.2, Vec3::new(0.6, 0.6, 1.0), 11.0, 13.0, 0.0),
        Whip => fs(3.0, 1.5, 12.0, Vec3::new(0.05, 0.05, 1.0), 7.0, 6.0, 0.0),
        Gun => fs(0.6, 0.3, 1.5, HELD, 5.0, 3.0, 120.0),
        Bow => fs(1.0, 0.5, 2.0, HELD, 14.0, 8.0, 80.0),
        Cannon => fs(1.2, 0.6, 3.0, Vec3::new(0.45, 0.45, 1.0), 20.0, 22.0, 45.0),
        Staff => fs(1.6, 0.8, 3.0, Vec3::new(0.1, 0.1, 1.0), 8.0, 8.0, 70.0),
        Disc => fs(0.5, 0.3, 1.2, Vec3::new(1.0, 0.1, 1.0), 8.0, 8.0, 40.0),
        // Worn.
        Wings => fs(2.0, 1.0, 4.0, WORN, 0.0, 0.0, 0.0),
        Parachute => fs(1.5, 1.0, 4.0, WORN, 0.0, 0.0, 0.0),
        Jetpack => fs(0.8, 0.5, 2.0, WORN, 0.0, 0.0, 0.0),
        Cape => fs(1.2, 0.8, 2.5, WORN, 0.0, 0.0, 0.0),
        // Creatures: scale = height.
        Biped => fs(1.8, 0.5, 4.0, Vec3::new(0.45, 1.0, 0.35), 8.0, 9.0, 9.0),
        Kaiju => fs(14.0, 10.0, 22.0, Vec3::new(0.5, 1.0, 0.6), 18.0, 22.0, 9.0),
        Quadruped => fs(1.2, 0.5, 4.0, Vec3::new(0.5, 0.8, 1.3), 8.0, 9.0, 9.0),
        Serpent => fs(0.8, 0.5, 4.0, Vec3::new(0.4, 0.4, 2.5), 8.0, 9.0, 9.0),
        Bird => fs(0.8, 0.5, 4.0, Vec3::new(1.2, 0.6, 0.8), 6.0, 7.0, 9.0),
        Fish => fs(0.8, 0.5, 4.0, Vec3::new(0.4, 0.6, 1.4), 6.0, 7.0, 9.0),
        Arachnid => fs(0.8, 0.5, 4.0, Vec3::new(1.4, 0.6, 1.4), 8.0, 8.0, 9.0),
        Insect => fs(0.5, 0.5, 2.0, Vec3::new(1.0, 0.6, 1.2), 3.0, 4.0, 9.0),
        Blob => fs(1.0, 0.5, 4.0, LUMP, 7.0, 10.0, 9.0),
        // Structures.
        Wall => fs(6.0, 2.0, 12.0, Vec3::new(1.0, 0.667, 0.085), 0.0, 0.0, 0.0),
        Ramp => fs(6.0, 3.0, 12.0, Vec3::new(0.5, 0.08, 1.0), 0.0, 0.0, 0.0),
        Pillar => fs(8.0, 3.0, 16.0, Vec3::new(0.14, 1.0, 0.14), 0.0, 0.0, 0.0),
        Dome => fs(3.0, 2.0, 5.0, Vec3::new(2.0, 1.0, 2.0), 0.0, 0.0, 0.0),
        Cage => fs(2.5, 1.5, 5.0, LUMP, 0.0, 0.0, 0.0),
        Platform => fs(4.0, 2.0, 8.0, Vec3::new(1.0, 0.12, 1.0), 0.0, 0.0, 0.0),
        Trampoline => fs(3.0, 2.0, 6.0, Vec3::new(1.0, 0.15, 1.0), 0.0, 0.0, 0.0),
        // Objects: scale = diameter / length. `speed` = throw speed.
        Ball => fs(0.8, 0.3, 2.5, LUMP, 8.0, 10.0, 30.0),
        Anvil => fs(1.0, 0.5, 3.0, Vec3::new(1.0, 0.7, 0.6), 30.0, 18.0, 0.0),
        Bomb => fs(0.7, 0.3, 2.5, LUMP, 30.0, 20.0, 24.0),
        Boulder => fs(1.6, 0.8, 4.0, LUMP, 14.0, 16.0, 20.0),
        Rocket => fs(1.2, 0.5, 3.0, Vec3::new(0.35, 0.35, 1.0), 20.0, 18.0, 32.0),
        Vehicle => fs(3.0, 1.5, 6.0, Vec3::new(0.6, 0.5, 1.0), 16.0, 18.0, 26.0),
        Blob0 => fs(0.8, 0.3, 2.0, LUMP, 6.0, 8.0, 26.0),
        // Palms: scale = the energy's caliber (like a gun's length).
        Hands => fs(0.6, 0.3, 1.5, Vec3::new(0.3, 0.3, 0.3), 5.0, 3.0, 120.0),
    }
}

/// Per-player construct slots (COMBAT.md §3). Casting over a limit replaces the oldest in the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Held,
    Worn,
    Structure,
    Object,
    Creature,
}

impl Slot {
    pub fn of(form: Form) -> Slot {
        match form.category() {
            FormCategory::Held => Slot::Held,
            FormCategory::Worn => Slot::Worn,
            FormCategory::Structure => Slot::Structure,
            FormCategory::Object => Slot::Object,
            FormCategory::Creature => Slot::Creature,
        }
    }

    /// Shipped per-player cap of this slot (the live cap is `MatchRules::slot_limit`, lobby-set).
    pub fn limit(self) -> usize {
        match self {
            Slot::Held => 1,
            Slot::Worn => 1,
            Slot::Structure => 3,
            Slot::Object => 2,
            Slot::Creature => 2,
        }
    }
}

/// Shipped max simultaneous kaiju per player (they also count as creatures; live cap:
/// `MatchRules::kaiju_limit`).
pub const KAIJU_LIMIT: usize = 1;

/// Max `count` for a non-swarm recipe / a swarm.
pub const MAX_COUNT: u8 = 6;
pub const MAX_SWARM_COUNT: u8 = 12;

/// Mass clamp (kg).
pub const MIN_MASS: f32 = 0.5;
pub const MAX_MASS: f32 = 60_000.0;

/// Forming time (s): voxels stream from the ring. Covers the network round trip.
pub fn forming_secs(form: Form) -> f32 {
    if form == Form::Kaiju {
        return 1.2;
    }
    match form.category() {
        FormCategory::Held => 0.15,
        FormCategory::Worn => 0.2,
        FormCategory::Structure => 0.25,
        FormCategory::Object => 0.15,
        FormCategory::Creature => 0.6,
    }
}

/// Lifetime (s) before a construct expires on its own. `f32::INFINITY` = until dismissed/shattered.
pub fn lifetime_secs(form: Form) -> f32 {
    if form == Form::Kaiju {
        return 18.0;
    }
    match form.category() {
        FormCategory::Held | FormCategory::Worn => f32::INFINITY,
        FormCategory::Structure => 30.0,
        FormCategory::Object => 6.0,
        FormCategory::Creature => 14.0,
    }
}
