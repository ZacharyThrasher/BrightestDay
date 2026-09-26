//! **Swarms** (COMBAT.md §4 Creatures: Swarm) — OWNED BY THE CREATURE ENGINEER.
//!
//! The framework splits a `Swarm` construct into `count` independent child constructs on its first
//! active tick (`components::swarm_split` queues [`super::Interaction::Split`]). For creature
//! forms it asks [`split_plan`] how (how many, at what size); non-creature swarms ("anvil swarm"
//! = a hailstorm) use the generic plan in `components.rs`. Children have `c.parent = Some(..)`
//! and do not count toward slot limits. The framework strips `Swarm` from every copy (the original
//! included) and marks them `comp.split`; each runs its normal controller (creature.rs), which
//! treats them as **swarm units**: small, fragile (integrity capped at [`UNIT_INTEGRITY`]), 3 dmg
//! nips ([`UNIT_DAMAGE`]), steered with cheap boids ([`boids`]): cohesion + separation against
//! siblings found in the per-tick proxy snapshot (same owner, same form, same size, within
//! [`NEIGHBOUR_RADIUS`]) — O(constructs) per unit, no allocation.

use bd_core::Vec3;
use bd_core::mechanics::{FormCategory, Mechanic};

use super::{Construct, Env};

/// Most copies a creature swarm splits into (COMBAT.md §3: swarm ≤ 12).
pub const MAX_UNITS: u8 = 12;
/// Copies when the phrase gave no count ("bees").
pub const DEFAULT_UNITS: u8 = 6;
/// A unit's body size (m) — small enough to read as a swarm.
pub const UNIT_SIZE: f32 = 0.45;
/// Per-nip damage of a swarm unit (COMBAT.md §4: 3 dmg each).
pub const UNIT_DAMAGE: f32 = 3.0;
/// Units are fragile: one ring blast pops one.
pub const UNIT_INTEGRITY: f32 = 8.0;
/// Sibling search radius for boids (m).
pub const NEIGHBOUR_RADIUS: f32 = 8.0;

/// (copies, size factor) for a creature swarm, or `None` for the generic plan (non-creatures).
pub fn split_plan(c: &Construct) -> Option<(u8, f32)> {
    if c.category != FormCategory::Creature || !c.has(Mechanic::Swarm) {
        return None;
    }
    let n = if c.recipe.count >= 2 { c.recipe.count.min(MAX_UNITS) } else { DEFAULT_UNITS };
    let scale = c.recipe.scale.max(0.1);
    // Each unit is ~UNIT_SIZE, never bigger than the original.
    let factor = (UNIT_SIZE / scale).clamp(0.08, 1.0);
    Some((n, factor))
}

/// Is `c` one of the copies a creature swarm split into (the original included)?
#[inline]
pub fn is_unit(c: &Construct) -> bool {
    c.comp.split && c.category == FormCategory::Creature
}

/// Boids-ish steering offset for a swarm unit (add to its desired velocity): cohesion toward the
/// siblings' centre and separation from any sibling closer than `spacing`. Returns (offset
/// velocity, sibling count).
pub fn boids(c: &Construct, env: &Env<'_>, speed: f32) -> (Vec3, u32) {
    let me = c.center();
    let spacing = (c.half.max_element() * 3.0).max(0.8);
    let mut sum = Vec3::ZERO;
    let mut sep = Vec3::ZERO;
    let mut n = 0u32;
    for p in env.others {
        if p.id == c.id
            || p.owner != c.owner
            || p.form != c.recipe.form
            || !p.active
            || (p.half - c.half).abs().max_element() > c.half.max_element() * 0.15
        {
            continue;
        }
        let d = p.center - me;
        let dl = d.length();
        if dl > NEIGHBOUR_RADIUS {
            continue;
        }
        n += 1;
        sum += p.center;
        if dl < spacing {
            let away = if dl > 1e-4 { -d / dl } else { Vec3::new(((c.id.0 % 7) as f32 - 3.0) * 0.3, 0.2, 0.5).normalize_or(Vec3::X) };
            sep += away * (1.0 - dl / spacing);
        }
    }
    if n == 0 {
        return (Vec3::ZERO, 0);
    }
    let centre = sum / n as f32;
    let coh = (centre - me).clamp_length_max(4.0) * 0.35;
    ((coh + sep * 1.4) * speed * 0.5, n)
}

/// Squad coordination ("autobots" / "decepticons": 3 mini transformers, any walking swarm): each
/// unit takes a different side of the target (slots at −120°, 0°, +120° around the line from the
/// target to the unit) until it is close, so a squad arrives as a pincer instead of a queue.
/// Used by creature units and available to the generic transformer locomotion.
pub fn squad_goal(c: &Construct, target: Vec3) -> Vec3 {
    let here = c.pos;
    let d = Vec3::new(here.x - target.x, 0.0, here.z - target.z);
    if d.length() < 5.0 {
        return target;
    }
    let dir = d.normalize_or(Vec3::Z);
    let slot = (c.id.0 % 3) as f32 - 1.0;
    let (s, co) = (slot * 2.1).sin_cos();
    let side = Vec3::new(dir.x * co + dir.z * s, 0.0, -dir.x * s + dir.z * co);
    target + side * 4.0
}
