//! Projectile data: ring blasts and shots from held ranged constructs. Stepping (swept tests vs
//! players / constructs / terrain, lag-compensated catch-up, portals, fields) lives in
//! `world/projectiles.rs`.
//!
//! Projectiles are ballistic (`pos(t) = origin + v·t + ½·g·t²` until they bounce or pass a portal),
//! so clients simulate them from the `ProjectileFired` event / snapshot spawn record and only need
//! the `ProjectileEnded` event.

use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId, Tick, Vec3};
use serde::{Deserialize, Serialize};

use crate::combat::HitKind;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ProjectileKind {
    /// Empty-hand ring blast.
    #[default]
    RingBlast = 0,
    /// Staff / gun bolt.
    Bolt = 1,
    Arrow = 2,
    CannonBall = 3,
    Disc = 4,
}

impl ProjectileKind {
    /// Downward acceleration (m/s²).
    pub fn gravity(self) -> f32 {
        match self {
            ProjectileKind::Arrow => 9.0,
            ProjectileKind::CannonBall => 12.0,
            _ => 0.0,
        }
    }

    pub fn from_u8(v: u8) -> ProjectileKind {
        match v {
            1 => ProjectileKind::Bolt,
            2 => ProjectileKind::Arrow,
            3 => ProjectileKind::CannonBall,
            4 => ProjectileKind::Disc,
            _ => ProjectileKind::RingBlast,
        }
    }
}

/// On-hit side effects a projectile carries from its construct's mechanics (rays).
pub mod effects {
    pub const SHRINK: u8 = 1 << 0;
    pub const GROW: u8 = 1 << 1;
    /// Chain the target to the ground where it was hit.
    pub const LINK: u8 = 1 << 2;
    /// Yank the target toward the shooter.
    pub const PULL: u8 = 1 << 3;
    /// Portal gun round: on a surface it places the owner's next portal end (A, then B).
    pub const PORTAL: u8 = 1 << 4;
}

/// Request to fire a projectile (from the ring or a construct).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectileSpawn {
    pub owner: PlayerId,
    pub construct: Option<ConstructId>,
    pub kind: ProjectileKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub damage: f32,
    pub knockback: f32,
    pub element: Element,
    pub radius: f32,
    /// Seconds before it fizzles.
    pub life: f32,
    /// Explodes on impact: (blast radius, crater radius).
    pub explode: Option<(f32, f32)>,
    pub hit_kind: HitKind,
    /// Remaining terrain bounces.
    pub ricochet: u8,
    /// [`effects`] bits.
    pub effects: u8,
    /// Damage falls off beyond this distance (m); 0 = never (shotgun pellets: 12 m).
    pub falloff_start: f32,
    /// Terrain chip radius on impact (m); 0 = none.
    pub chip: f32,
    /// Speed of the round relative to the gun (the momentum bonus counts only closing speed
    /// beyond it). 0 = derive from `vel` at spawn.
    pub muzzle_speed: f32,
}

impl ProjectileSpawn {
    /// The empty-hand ring blast (COMBAT.md §4): 90 m/s, 7 dmg, 5 kb.
    pub fn ring_blast(owner: PlayerId, pos: Vec3, dir: Vec3) -> Self {
        use crate::combat::*;
        Self {
            owner,
            construct: None,
            kind: ProjectileKind::RingBlast,
            pos,
            vel: dir.normalize_or_zero() * BLAST_SPEED,
            damage: BLAST_DAMAGE,
            knockback: BLAST_KNOCKBACK,
            element: Element::Light,
            radius: BLAST_RADIUS,
            life: BLAST_LIFETIME,
            explode: None,
            hit_kind: HitKind::RingBlast,
            ricochet: 0,
            effects: 0,
            falloff_start: 0.0,
            chip: 0.0,
            muzzle_speed: 0.0,
        }
    }
}

/// A projectile in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projectile {
    /// Wire id (wraps; unique among live projectiles).
    pub id: u16,
    pub spec: ProjectileSpawn,
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f32,
    /// Start of the current ballistic segment (reset on bounce / portal) — replicated.
    pub origin: Vec3,
    pub vel0: Vec3,
    pub segment_tick: Tick,
    /// Lag compensation: the shooter saw other players this many ticks in the past; hit tests
    /// use their hulls from `tick − lag` for the whole flight (0 = no rewind).
    pub lag: f32,
    pub portal_cd: f32,
    /// Distance flown (m), for falloff.
    pub travelled: f32,
    /// Remove at the end of this tick.
    pub dead: bool,
}
