//! **Surfing** (owner: "make CS-style surfing real"): steep, unwalkable slopes you ride in the air.
//!
//! * **Surf state** = airborne and touching (last tick's slide-move contact) a surface whose angle
//!   from horizontal is within [`MoveParams::surf_min_angle`, `surf_max_angle`] (45.6°–80°: past
//!   walkable, not a wall, never a ceiling). Stored as [`super::ExtraMoves::surf_n`] (replicated, so
//!   prediction agrees).
//! * While surfing: air acceleration uses `surf_air_accel` (the Quake/CS wish cap is unchanged) and
//!   the horizontal soft cap doesn't bleed speed (only the global hard cap applies). The slide move
//!   already clips velocity onto the face, so gravity along a descending line becomes speed and
//!   strafing into the face holds the line; let go and you slide off.
//! * Off-surf everything is bit-identical ([`air_accel`] returns `air_accel` exactly).
//! * Geometry: [`surf_line`] builds a CS-style triangular-prism line (two faces, Λ) from a ridge
//!   segment — map lines (`bd_voxel::SurfLine`) and tests use it. Voxel staircases can't be surfed
//!   (their step tops are walkable ground), so map lines are smooth hard-light colliders.

use bd_core::{Quat, Vec3};
use glam::Mat3;

use super::params::MoveParams;
use super::state::{PlayerState, WallHit};
use super::world::{Collider, ColliderId};

/// Is `n` (a contact normal) a surfable face?
#[inline]
pub fn surfable(n: Vec3, p: &MoveParams) -> bool {
    // Angle from horizontal of a face = angle of its normal from +Y.
    let lo = p.surf_max_angle.to_radians().cos();
    let hi = p.surf_min_angle.to_radians().cos();
    n.is_finite() && n.y >= lo && n.y <= hi
}

/// Surfing this tick (touching a surf face since last tick, in the air)?
#[inline]
pub fn surfing(s: &PlayerState, p: &MoveParams) -> bool {
    p.surf && !s.grounded && s.extra.surf_n != Vec3::ZERO
}

/// Air acceleration for this tick.
#[inline]
pub(crate) fn air_accel(s: &PlayerState, p: &MoveParams) -> f32 {
    if surfing(s, p) { p.surf_air_accel } else { p.air_accel }
}

/// End of the tick: remember a surf contact (the slide move's steepest non-walkable hit).
#[inline]
pub(crate) fn update_contact(s: &mut PlayerState, wall: Option<WallHit>, p: &MoveParams) {
    s.extra.surf_n = match wall {
        Some(w) if p.surf && !s.grounded && surfable(w.normal, p) => w.normal,
        _ => Vec3::ZERO,
    };
}

/// A CS-style surf line: a triangular prism whose ridge runs from `a` to `b` (descend by making
/// `b` lower), each side a face at `face_deg` from horizontal and `width` metres of slope, `thick`
/// metres deep. Two colliders (right face, left face), ids `id` and `id + 1` (`ColliderId::Other`).
pub fn surf_line(a: Vec3, b: Vec3, face_deg: f32, width: f32, thick: f32, id: u32) -> [Collider; 2] {
    let d = (b - a).normalize_or(Vec3::NEG_Z);
    let r = d.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = r.cross(d).normalize_or(Vec3::Y);
    let (st, ct) = face_deg.to_radians().sin_cos();
    let mid = (a + b) * 0.5;
    let len = (b - a).length();
    let face = |s: f32, k: u32| {
        let n = r * (s * st) + up * ct;
        let f = r * (s * ct) - up * st;
        let z = f.cross(n);
        let rot = Quat::from_mat3(&Mat3::from_cols(f, n, z));
        let center = mid + f * (width * 0.5) - n * (thick * 0.5);
        Collider::new(center, Vec3::new(width * 0.5, thick * 0.5, len * 0.5), rot, ColliderId::Other(id + k))
    };
    [face(1.0, 0), face(-1.0, 1)]
}

/// Collider ids of map surf lines start here (`ColliderId::Other`).
pub const SURF_ID_BASE: u32 = 0x5355_0000;

/// Append a map's surf lines (`bd_voxel::ArenaInfo::surf_lines`) as colliders: two faces per line.
/// Host and clients call it with the same generated map data, so prediction collides identically.
pub fn push_surf_colliders(lines: &[bd_voxel::SurfLine], out: &mut Vec<Collider>) {
    for (i, l) in lines.iter().enumerate() {
        out.extend(surf_line(l.a, l.b, l.face_deg, l.width, 1.5, SURF_ID_BASE + 2 * i as u32));
    }
}
