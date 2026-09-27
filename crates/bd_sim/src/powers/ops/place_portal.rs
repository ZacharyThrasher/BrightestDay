//! `PlacePortal` — the portal gun (docs/toolbox/OPS.md). Each use shoots a portal end onto the aimed
//! surface; ends alternate blue / orange and the newest two are one linked pair (the world's portal
//! rules do the rest: momentum kept, see-through, one pair per owner). A lone first end shows as a
//! mark until its partner lands. Floors and ceilings work (fling!): the ring's up follows your look.

use bd_core::{MAX_PLAYERS, PlayerId, Vec3};

use super::{OpEvent, flat};
use crate::movement::{CollisionWorld, ColliderId, raycast_world};
use crate::powers::PowerError;
use crate::space::{PORTAL_RADIUS, PortalEnd, fit_portal};
use crate::world::World;

pub const MAX_RANGE: f32 = 80.0;
pub const DEFAULT_SECS: f32 = 20.0;

/// A player's portal gun: the last end of each colour (`OpState::portal_guns`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PortalGun {
    pub owner: PlayerId,
    pub ends: [Option<PortalEnd>; 2],
    /// Colour of the next shot (0 = blue, 1 = orange).
    pub next: u8,
}

impl World {
    pub(crate) fn op_place_portal(&mut self, i: usize, range: f32, secs: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let (eye, look) = self.op_eye_look(i);
        let range = if range.is_finite() && range > 0.0 { range.clamp(4.0, MAX_RANGE) } else { 60.0 };
        let secs = if secs.is_finite() && secs > 0.0 { secs.clamp(2.0, 60.0) } else { DEFAULT_SECS };
        let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(ColliderId::Player(id));
        let Some(hit) = raycast_world(&cw, eye, look, range, false) else { return Err(PowerError::NoTarget) };
        // Only terrain takes a portal (the surface must be solid voxels right behind the hit).
        let n = hit.normal.normalize_or(-look);
        if self.voxels.get_at(hit.point - n * 0.3) == 0 {
            return Err(PowerError::NoTarget);
        }
        let up = if n.y.abs() > 0.7 { flat(look, Vec3::NEG_Z) * n.y.signum() } else { Vec3::Y };
        let end = fit_portal(&self.voxels, PortalEnd::new(hit.point + n * 0.05, n, up, PORTAL_RADIUS));
        let mut gun = match self.powers.ops.portal_guns[i] {
            Some(g) if g.owner == id => g,
            _ => PortalGun { owner: id, ends: [None, None], next: 0 },
        };
        let colour = gun.next & 1;
        gun.ends[colour as usize] = Some(end);
        gun.next = colour ^ 1;
        self.powers.ops.portal_guns[i] = Some(gun);
        self.op_event(OpEvent::PortalShot { player: id, pos: end.pos, normal: end.normal, orange: colour == 1 });
        if let [Some(a), Some(b)] = gun.ends {
            self.create_portal(id, a, b, secs, None).ok_or(PowerError::Busy)?;
        }
        Ok(())
    }

    /// A KO (or a player leaving) forgets their ends.
    pub(crate) fn step_portal_guns(&mut self) {
        // Every slot: a departed player's leftover ends are forgotten too (OPS_REVIEW low).
        for i in 0..MAX_PLAYERS {
            if let Some(g) = self.powers.ops.portal_guns[i]
                && !self.players.get(i).is_some_and(|p| p.id == g.owner && p.alive)
            {
                self.powers.ops.portal_guns[i] = None;
            }
        }
    }
}
