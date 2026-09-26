//! Map mechanics the sim runs from map data (empty on every map but Cybertron): **transforming
//! panels** ([`MapMover`]) and a **permanent space bridge** ([`MapPortal`]).
//!
//! Design rule (owner): flavour that *adds* options, never a gimmick players must play around.
//! - A mover's folded pose is decoration; its extended pose is a solid, walkable piece that adds a
//!   route (bridge, ramp). The base layout is complete without any of them.
//! - The sim (`bd_sim::world::map_mech`) telegraphs every change [`WARN_SECS`] ahead, moves pieces
//!   as non-solid hard light, solidifies only when the target volume is clear of players, and only
//!   retracts once nobody is on the piece: no crushing, no forced movement, no KO from a transform.
//! - Movers are sim colliders (not voxels): smooth, indestructible, and the same on the host and on
//!   predicting clients (their state is replicated as `GameEvent::MapMover`).

use glam::{Quat, Vec3};

/// Telegraph before any move (light cue + panel sounds), seconds.
pub const WARN_SECS: f32 = 2.0;
/// Duration of a move (folding / unfolding), seconds.
pub const MOVE_SECS: f32 = 1.2;

/// One rigid panel of a mover: a box in its **extended** pose (`center`, `half`, `rot`), folded by
/// rotating `fold_angle` radians about `hinge_axis` through `hinge` and then translating by
/// `fold_offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoverPart {
    pub center: Vec3,
    pub half: Vec3,
    pub rot: Quat,
    pub hinge: Vec3,
    pub hinge_axis: Vec3,
    pub fold_angle: f32,
    pub fold_offset: Vec3,
}

impl MoverPart {
    /// Pose at openness `t` (0 = folded, 1 = extended): box centre and rotation. `t = 1` returns the
    /// extended pose exactly (no trig), so solid colliders are bit-identical everywhere.
    pub fn pose(&self, t: f32) -> (Vec3, Quat) {
        if t >= 1.0 {
            return (self.center, self.rot);
        }
        let k = 1.0 - t.max(0.0);
        let q = if self.fold_angle == 0.0 { Quat::IDENTITY } else { Quat::from_axis_angle(self.hinge_axis.normalize_or_zero(), self.fold_angle * k) };
        let c = self.hinge + q * (self.center - self.hinge) + self.fold_offset * k;
        (c, q * self.rot)
    }
}

/// A transforming piece: panels that move together on a schedule.
#[derive(Clone, Debug, PartialEq)]
pub struct MapMover {
    pub name: &'static str,
    pub parts: Vec<MoverPart>,
    /// Seconds folded / extended between moves (the warn and move phases come on top).
    pub folded_secs: f32,
    pub extended_secs: f32,
    /// Seconds into the first folded hold at match start (staggers the pieces).
    pub offset_secs: f32,
}

/// A permanent portal pair (a space bridge): discs at `a` / `b` facing `a_normal` / `b_normal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapPortal {
    pub a: Vec3,
    pub a_normal: Vec3,
    pub b: Vec3,
    pub b_normal: Vec3,
    pub radius: f32,
}

/// All sim-run map mechanics of a map (`ArenaInfo::mechanics`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapMechanics {
    pub movers: Vec<MapMover>,
    pub portals: Vec<MapPortal>,
}

impl MapMechanics {
    pub fn is_empty(&self) -> bool {
        self.movers.is_empty() && self.portals.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_folds_about_the_hinge() {
        let p = MoverPart {
            center: Vec3::new(5.0, 0.0, 0.0),
            half: Vec3::new(5.0, 0.5, 2.0),
            rot: Quat::IDENTITY,
            hinge: Vec3::ZERO,
            hinge_axis: Vec3::Z,
            fold_angle: std::f32::consts::FRAC_PI_2,
            fold_offset: Vec3::ZERO,
        };
        assert_eq!(p.pose(1.0), (p.center, p.rot));
        let (c, _) = p.pose(0.0);
        assert!((c - Vec3::new(0.0, 5.0, 0.0)).length() < 1e-4, "{c}");
    }
}
