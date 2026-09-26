//! Euler convention for `Prim::rot_deg`: glam XYZ order (R = Rx·Ry·Rz), degrees.
//!
//! Construct shape description: a list of simple primitives in construct-voxel units
//! (1 unit = [`crate::CONSTRUCT_VOXEL_SIZE`] m), origin at the construct's pivot (grip for held items,
//! feet-centre for creatures and structures), +Y up, -Z forward (the direction a held item points /
//! a creature faces).
//!
//! Both the procedural form generators (bd_sim) and curated icon models (assets/icons.ron via bd_lexicon)
//! produce a `ShapeSpec`; bd_sim voxelises it for collision/damage/shatter and the client meshes it.

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimKind {
    Box,
    Ellipsoid,
    /// Cylinder along the primitive's local Y axis.
    Cylinder,
    /// Cone along local Y, base at -Y, tip at +Y.
    Cone,
}

/// Palette slot. The final colour is decided at render time from the owner's corps colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tint {
    /// Corps colour.
    #[default]
    Primary,
    /// Recipe accent (falls back to a lighter corps colour).
    Accent,
    /// Darker corps colour (edges, grips, joints).
    Dark,
    /// Near-white hot core (eyes, blades' edges, energy).
    Core,
}

/// Which animated part of a construct a primitive belongs to. Creatures/items animate by rigidly
/// transforming each part around its pivot; the sim/client share the part list.
pub type PartId = u8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prim {
    pub kind: PrimKind,
    /// Centre in construct-voxel units.
    pub center: Vec3,
    /// Full extents in construct-voxel units (before rotation).
    pub size: Vec3,
    /// Euler rotation XYZ in degrees (authoring-friendly).
    #[serde(default)]
    pub rot_deg: Vec3,
    #[serde(default)]
    pub tint: Tint,
    /// Animated part this primitive belongs to (0 = root/body).
    #[serde(default)]
    pub part: PartId,
    /// Subtract instead of add (carve holes, hollow shapes).
    #[serde(default)]
    pub subtract: bool,
}

/// A named animated part: pivot point + parent part. Part 0 is the root and needs no entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: PartId,
    pub parent: PartId,
    /// Pivot in construct-voxel units (in the model's rest pose).
    pub pivot: Vec3,
    /// Semantic role so generic animations can drive it (e.g. every `Tail` part sways).
    pub role: PartRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PartRole {
    Head,
    Jaw,
    Tail,
    LegFrontLeft,
    LegFrontRight,
    LegBackLeft,
    LegBackRight,
    ArmLeft,
    ArmRight,
    WingLeft,
    WingRight,
    Weapon,
    /// Pilot seat / saddle / cockpit (Mechanic::Piloted). (v1.1)
    Seat,
    /// Spins with locomotion speed. (v1.1)
    Wheel,
    /// Flame/exhaust flicker. (v1.1)
    Exhaust,
    /// Sways (capes, sails, flags). (v1.1)
    Cloth,
    Other,
    // ---- v1.5: construct-animation roles (appended so serde/bincode indices stay stable). The
    // client animates parts by role x semantic event (aim, fire, move, pilot, summon…), never by
    // word, so any combination of parts animates. Hinged parts put their pivot on the hinge.
    /// Hinged panel — hatch, canopy, door, lid, visor: opens for the pilot, shuts on entry. (v1.5)
    Hatch,
    /// Yaws toward the aim (tank turret, gun-mount base). (v1.5)
    Turret,
    /// Pitches toward the aim, recoils along its length on fire (points −Z). (v1.5)
    Barrel,
    /// Spins with throttle: rotor, propeller, fan, turbine (axis = its thinnest extent). (v1.5)
    Rotor,
    /// Pumps along its longest extent on fire / movement. (v1.5)
    Piston,
    /// Fin / small wing / flap: flexes with speed and turns. (v1.5)
    Fin,
    /// Engine nozzle / thruster flame: flares with speed and throttle. (v1.5)
    Thruster,
    /// Eye(s): blink, light up at the end of the summon. (v1.5)
    Eye,
    /// Blade that extends from its base (pivot) on summon and attack. (v1.5)
    Blade,
    /// Where the hand holds a held item: the handle geometry (hilt, pistol grip, shaft wrap, strap),
    /// pivot = palm centre. See `bd_shapes::grip_point`. (v1.5)
    Grip,
    /// Second-hand point of a two-handed item (fore-grip, shaft, launcher handle): a marker part
    /// (no prims needed), pivot = palm centre. See `bd_shapes::support_point`. (v1.5)
    Support,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShapeSpec {
    pub prims: Vec<Prim>,
    #[serde(default)]
    pub parts: Vec<Part>,
}
