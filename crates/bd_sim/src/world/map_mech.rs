//! MAP MECH HOOK: the map's transforming panels and permanent space bridge
//! (`bd_voxel::ArenaInfo::mechanics`; Cybertron only — every other map has none and skips all of this).
//!
//! **Movers** (flavour that *adds* routes; owner rule: never a gimmick players must play around):
//! each runs `Folded → WarnExtend → Extending → Extended → WarnRetract → Retracting → Folded`.
//! - `Warn*` last [`WARN_SECS`] (client: pulsing amber light + panel clicks) before anything moves.
//! - Pieces move as **non-solid** hard light; they only become solid (`Extended`) once the extended
//!   volume is clear of players, and only start retracting once nobody is on or in them — so a
//!   transform never crushes, shoves, drops or KOs anyone. A blocked piece simply waits.
//! - Solid only in `Extended` / `WarnRetract`: box colliders (not anchorable, zero velocity) pushed
//!   next to the surf lines, by the host every tick and by client prediction from the replicated
//!   state (`GameEvent::MapMover` on every phase change and round reset → [`World::apply_map_mover`]).
//!
//! **Space bridge:** each `MapPortal` becomes a `PortalPair` owned by [`MAP_OWNER`] that never
//! expires and can't be broken (infinite hp); re-installed on every round reset.

use bd_core::{PlayerId, Quat, Tick, Vec3};
use bd_voxel::mechanics::{MOVE_SECS, WARN_SECS};
use serde::{Deserialize, Serialize};

use super::World;
use crate::events::GameEvent;
use crate::movement::{Collider, ColliderId};
use crate::space::{PortalEnd, PortalPair};

/// Owner id of map-owned portals (never a real player).
/// (252 % 8 = Hope: the corps colour fallback draws the space-bridge discs energon blue.)
pub const MAP_OWNER: PlayerId = PlayerId(252);
/// Collider ids of mover panels: `MOVER_ID_BASE + mover * 16 + part` (`ColliderId::Other`).
pub const MOVER_ID_BASE: u32 = 0x4d56_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoverPhase {
    Folded,
    WarnExtend,
    Extending,
    Extended,
    WarnRetract,
    Retracting,
}

impl MoverPhase {
    /// Panels block movement in this phase.
    pub fn solid(self) -> bool {
        matches!(self, MoverPhase::Extended | MoverPhase::WarnRetract)
    }
    /// A telegraph phase (client light + sound cue).
    pub fn warning(self) -> bool {
        matches!(self, MoverPhase::WarnExtend | MoverPhase::WarnRetract)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoverState {
    pub phase: MoverPhase,
    /// Tick the phase began (may be negative for the staggered first hold).
    pub since: i64,
}

/// Replicated state of every mover on the map.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapMechState {
    pub movers: Vec<MoverState>,
}

fn ticks(secs: f32) -> i64 {
    (secs.max(0.0) * bd_core::TICK_HZ as f32).round() as i64
}

/// Snappy ease with no overshoot (panels read as mechanical, never pass their stops).
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}

impl MapMechState {
    /// Every mover folded, staggered by its `offset_secs`, as of `tick`.
    pub fn new(arena: &bd_voxel::ArenaInfo, tick: Tick) -> Self {
        let movers = arena.mechanics.movers.iter().map(|m| MoverState { phase: MoverPhase::Folded, since: tick as i64 - ticks(m.offset_secs) }).collect();
        Self { movers }
    }

    /// Openness 0 (folded) … 1 (extended) of mover `i` at `tick` (presentation; eased).
    pub fn openness(&self, i: usize, tick: Tick) -> f32 {
        let Some(s) = self.movers.get(i) else { return 0.0 };
        let el = (tick as i64 - s.since).max(0) as f32 / ticks(MOVE_SECS).max(1) as f32;
        match s.phase {
            MoverPhase::Folded | MoverPhase::WarnExtend => 0.0,
            MoverPhase::Extending => ease(el),
            MoverPhase::Extended | MoverPhase::WarnRetract => 1.0,
            MoverPhase::Retracting => 1.0 - ease(el),
        }
    }
}

/// Append the solid mover panels (phase `Extended` / `WarnRetract`) as colliders. Pure function
/// of the map data + replicated state: host and prediction build identical lists.
pub fn push_mover_colliders(arena: &bd_voxel::ArenaInfo, state: &MapMechState, out: &mut Vec<Collider>) {
    for (i, (m, s)) in arena.mechanics.movers.iter().zip(&state.movers).enumerate() {
        if !s.phase.solid() {
            continue;
        }
        for (k, p) in m.parts.iter().enumerate() {
            let (c, q) = p.pose(1.0);
            let mut col = Collider::new(c, p.half, q, ColliderId::Other(MOVER_ID_BASE + i as u32 * 16 + k as u32));
            col.anchorable = false;
            out.push(col);
        }
    }
}

/// Is `id` a mover panel?
#[inline]
pub fn is_mover(id: ColliderId) -> bool {
    matches!(id, ColliderId::Other(i) if (MOVER_ID_BASE..MOVER_ID_BASE + 0x1_0000).contains(&i))
}

/// Does a player hull (feet `pos`, 0.8 × 1.8 m) come within `margin` of the box, or stand on it
/// (a margin of 0.6 m above the top face counts)?
fn hull_near_box(pos: Vec3, c: Vec3, half: Vec3, q: Quat, margin: f32) -> bool {
    let inv = q.inverse();
    // Sample the hull's axis (feet, middle, head) as spheres of the hull's half diagonal.
    for dy in [0.4f32, 0.9, 1.4] {
        let l = inv * (pos + Vec3::Y * dy - c);
        let r = 0.6 + margin;
        let d = (l.abs() - half).max(Vec3::ZERO);
        if d.length_squared() <= r * r {
            return true;
        }
    }
    false
}

impl World {
    /// Reset the movers (all folded, staggered from now) + (re)install the map's portal pairs
    /// (world creation, round reset, arena swap). `announce` replicates the reset to clients.
    pub(crate) fn map_mech_init(&mut self, announce: bool) {
        self.map_mech = MapMechState::new(&self.arena, self.tick);
        self.install_map_portals();
        if announce {
            for (i, s) in self.map_mech.movers.iter().enumerate() {
                self.events.push(GameEvent::MapMover { mover: i as u8, phase: s.phase, since: s.since });
            }
        }
    }

    /// (Re-)install the permanent space-bridge portal pairs.
    pub(crate) fn install_map_portals(&mut self) {
        self.portals.retain(|p| p.owner != MAP_OWNER);
        let specs = self.arena.mechanics.portals.clone();
        for sp in specs {
            // The map sets the size (only raised to the placement floor if it is too small).
            let r = sp.radius.max(crate::space::PORTAL_MIN_RADIUS);
            let mut a = PortalEnd::new(sp.a, sp.a_normal, Vec3::Y, r);
            let mut b = PortalEnd::new(sp.b, sp.b_normal, Vec3::Y, r);
            a.hp = f32::INFINITY;
            b.hp = f32::INFINITY;
            let id = self.next_space_id();
            self.portals.push(PortalPair { id, owner: MAP_OWNER, ends: [a, b], expires: Tick::MAX, construct: None });
        }
    }

    /// Host: advance every mover's phase for this tick and emit `MapMover` on changes.
    pub(crate) fn step_map_mech(&mut self) {
        if self.arena.mechanics.movers.is_empty() {
            return;
        }
        let now = self.tick as i64;
        for i in 0..self.map_mech.movers.len() {
            let Some(m) = self.arena.mechanics.movers.get(i) else { break };
            let s = self.map_mech.movers[i];
            let el = now - s.since;
            let clear = |margin: f32| {
                !self.players.iter().any(|p| p.alive && m.parts.iter().any(|part| hull_near_box(p.mv.pos, part.center, part.half, part.rot, margin)))
            };
            let next = match s.phase {
                MoverPhase::Folded if el >= ticks(m.folded_secs) => Some(MoverPhase::WarnExtend),
                MoverPhase::WarnExtend if el >= ticks(WARN_SECS) => Some(MoverPhase::Extending),
                // Solidify only into an empty volume (no pop, no shove).
                MoverPhase::Extending if el >= ticks(MOVE_SECS) && clear(0.05) => Some(MoverPhase::Extended),
                MoverPhase::Extended if el >= ticks(m.extended_secs) => Some(MoverPhase::WarnRetract),
                // Retract only when nobody is on it or hanging around its edges.
                MoverPhase::WarnRetract if el >= ticks(WARN_SECS) && clear(0.9) => Some(MoverPhase::Retracting),
                MoverPhase::Retracting if el >= ticks(MOVE_SECS) => Some(MoverPhase::Folded),
                _ => None,
            };
            if let Some(phase) = next {
                self.map_mech.movers[i] = MoverState { phase, since: now };
                self.events.push(GameEvent::MapMover { mover: i as u8, phase, since: now });
            }
        }
    }

    /// Client mirror: a replicated mover phase change.
    pub fn apply_map_mover(&mut self, mover: u8, phase: MoverPhase, since: i64) {
        if let Some(s) = self.map_mech.movers.get_mut(mover as usize) {
            *s = MoverState { phase, since };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_is_monotonic_and_bounded() {
        let mut prev = 0.0;
        for k in 0..=20 {
            let e = ease(k as f32 / 20.0);
            assert!((0.0..=1.0).contains(&e) && e >= prev);
            prev = e;
        }
        assert_eq!(ease(1.0), 1.0);
    }

    #[test]
    fn standing_on_a_box_counts_as_near() {
        let c = Vec3::new(0.0, -0.4, 0.0);
        let h = Vec3::new(4.0, 0.4, 2.0);
        assert!(hull_near_box(Vec3::ZERO, c, h, Quat::IDENTITY, 0.05));
        assert!(!hull_near_box(Vec3::new(0.0, 4.0, 0.0), c, h, Quat::IDENTITY, 0.05));
        assert!(!hull_near_box(Vec3::new(8.0, 0.0, 0.0), c, h, Quat::IDENTITY, 0.9));
    }
}
