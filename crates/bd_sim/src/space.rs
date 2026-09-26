//! World-level spatial effects from the second-wave mechanics (COMBAT.md §5f). Constructs create
//! them; the world owns, steps and replicates them so they affect players, projectiles and
//! constructs uniformly and deterministically:
//!
//! * [`PortalPair`] — two linked discs. Anything whose centre crosses a disc front-to-back comes
//!   out of the other one with position/velocity/view rotated from the entry frame to the exit
//!   frame (speed kept). Short per-body cooldown prevents ping-pong.
//! * [`ForceField`] — Attract (+) / Repel (−) acceleration with linear falloff; Heavy resists ×0.3,
//!   tethered players ×0.3, the owner is immune.
//! * [`Link`] — an inextensible chain with slack between two things (player / construct / fixed
//!   point), mass-weighted pull, cut by Blade or 25 damage.
//! * [`SizeState`] — Shrink / Grow on a player: smooth 0.25 s transition, 6 s duration.

use bd_core::{ConstructId, PlayerId, Quat, Tick, Vec3};
use glam::Mat3;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------------------------ portals

pub const PORTAL_LIFETIME: f32 = 12.0;
pub const PORTAL_RADIUS: f32 = 1.4;
pub const PORTAL_HP: f32 = 30.0;
/// After passing a portal a body ignores portals for this long.
pub const PORTAL_COOLDOWN: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortalEnd {
    pub pos: Vec3,
    /// Unit normal pointing out of the portal's face (the side you enter from / exit to).
    pub normal: Vec3,
    /// Unit "up" of the ring (orthogonal to `normal`).
    pub up: Vec3,
    pub radius: f32,
    pub hp: f32,
}

impl PortalEnd {
    /// Build with an orthonormalised frame. `up_hint` may be anything not parallel to `normal`.
    pub fn new(pos: Vec3, normal: Vec3, up_hint: Vec3, radius: f32) -> Self {
        let n = normal.normalize_or_zero();
        let n = if n == Vec3::ZERO { Vec3::Y } else { n };
        let mut up = (up_hint - n * up_hint.dot(n)).normalize_or_zero();
        if up == Vec3::ZERO {
            up = n.any_orthonormal_vector();
        }
        Self { pos, normal: n, up, radius: radius.clamp(0.3, 8.0), hp: PORTAL_HP }
    }

    fn frame(&self) -> Mat3 {
        Mat3::from_cols(self.up.cross(self.normal), self.up, self.normal)
    }
}

/// Rotation mapping directions entering `from` to directions leaving `to` (into `from` = −n_from
/// becomes +n_to; up stays up).
pub fn portal_rotation(from: &PortalEnd, to: &PortalEnd) -> Quat {
    let f = to.frame();
    let exit = Mat3::from_cols(-f.x_axis, f.y_axis, -f.z_axis);
    let m = exit * from.frame().transpose();
    Quat::from_mat3(&m).normalize()
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortalPair {
    pub id: u16,
    pub owner: PlayerId,
    pub ends: [PortalEnd; 2],
    pub expires: Tick,
    /// The construct that made it (portal dies with it if set).
    pub construct: Option<ConstructId>,
}

/// Result of passing through a portal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalTransit {
    /// Index of the entry end (0/1).
    pub entry: usize,
    pub rot: Quat,
    /// Mapped point (not yet pushed clear of the exit face).
    pub point: Vec3,
}

impl PortalPair {
    /// Does the segment `p0 → p1` cross either disc front-to-back? Returns the transit through the
    /// end crossed first.
    pub fn crossing(&self, p0: Vec3, p1: Vec3) -> Option<PortalTransit> {
        self.crossing_body(p0, p1, Vec3::ZERO)
    }

    /// Like [`Self::crossing`] for a box of half extents `half` centred on the segment: it passes
    /// when its leading face reaches the disc (so a player *falling onto* a floor portal goes
    /// through even though the floor stops the body).
    pub fn crossing_body(&self, p0: Vec3, p1: Vec3, half: Vec3) -> Option<PortalTransit> {
        let mut best: Option<(f32, usize)> = None;
        for (i, e) in self.ends.iter().enumerate() {
            let n = e.normal;
            let ext = n.x.abs() * half.x + n.y.abs() * half.y + n.z.abs() * half.z;
            let slack = if half == Vec3::ZERO { 0.0 } else { 0.06 };
            let d0 = (p0 - e.pos).dot(n) - ext;
            let d1 = (p1 - e.pos).dot(n) - ext;
            if !(d0 > slack && d1 <= slack) {
                continue;
            }
            let t = ((d0 - slack) / (d0 - d1).max(1e-6)).clamp(0.0, 1.0);
            let hit = p0 + (p1 - p0) * t;
            let lateral = hit - n * (hit - e.pos).dot(n) - e.pos;
            if lateral.length_squared() > e.radius * e.radius {
                continue;
            }
            if best.is_none_or(|b| t < b.0) {
                best = Some((t, i));
            }
        }
        let (_, i) = best?;
        let (from, to) = (&self.ends[i], &self.ends[1 - i]);
        let rot = portal_rotation(from, to);
        Some(PortalTransit { entry: i, rot, point: to.pos + rot * (p1 - from.pos) })
    }

    pub fn exit(&self, t: &PortalTransit) -> &PortalEnd {
        &self.ends[1 - t.entry]
    }
}

/// Push a body (half extents `half` around `center`) clear of an exit face.
pub fn clear_of_face(center: Vec3, half: Vec3, end: &PortalEnd) -> Vec3 {
    let n = end.normal;
    let ext = n.x.abs() * half.x + n.y.abs() * half.y + n.z.abs() * half.z + 0.05;
    let d = (center - end.pos).dot(n);
    if d < ext { center + n * (ext - d) } else { center }
}

// ------------------------------------------------------------------------------------- fields

/// Tethered players feel fields ×0.3; Heavy constructs ×0.3.
pub const FIELD_RESIST: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForceField {
    pub id: u16,
    pub owner: PlayerId,
    pub pos: Vec3,
    pub radius: f32,
    /// Acceleration at the centre (m/s²): > 0 attracts, < 0 repels. Falls off linearly to 0.
    pub strength: f32,
    pub expires: Tick,
    /// Follows this construct (and dies with it).
    pub construct: Option<ConstructId>,
}

impl ForceField {
    /// Acceleration at `p` (zero outside / at the exact centre).
    pub fn accel_at(&self, p: Vec3) -> Vec3 {
        let d = self.pos - p;
        let dist = d.length();
        if dist >= self.radius || dist < 1e-3 {
            return Vec3::ZERO;
        }
        let fall = 1.0 - dist / self.radius;
        d / dist * (self.strength * fall)
    }
}

// -------------------------------------------------------------------------------------- links

pub const LINK_MAX_LEN: f32 = 8.0;
pub const LINK_LIFETIME: f32 = 6.0;
pub const LINK_HP: f32 = 25.0;
/// Mass used for players in links (kg, × size³).
pub const PLAYER_MASS: f32 = 80.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum LinkEnd {
    Player(PlayerId),
    Construct(ConstructId),
    /// Immovable.
    Point(Vec3),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub id: u16,
    pub owner: Option<PlayerId>,
    pub a: LinkEnd,
    pub b: LinkEnd,
    /// Chain length (slack below it, inextensible at it).
    pub max_len: f32,
    pub expires: Tick,
    pub hp: f32,
}

// -------------------------------------------------------------------------------------- resize

pub const SHRINK_SCALE: f32 = 0.5;
pub const GROW_SCALE: f32 = 1.6;
pub const RESIZE_SECS: f32 = 6.0;
pub const RESIZE_TRANSITION: f32 = 0.25;

/// A player's Shrink / Grow state.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SizeState {
    /// Where the scale is heading (1 when nothing is active).
    pub target: f32,
    /// Current body scale.
    pub scale: f32,
    /// Seconds left before returning to 1.
    pub timer: f32,
}

impl Default for SizeState {
    fn default() -> Self {
        Self { target: 1.0, scale: 1.0, timer: 0.0 }
    }
}

impl SizeState {
    pub fn apply(&mut self, target: f32, secs: f32) {
        self.target = target.clamp(SHRINK_SCALE, GROW_SCALE);
        self.timer = secs.clamp(0.0, 30.0);
    }

    pub fn step(&mut self, dt: f32) {
        if self.timer > 0.0 {
            self.timer -= dt;
            if self.timer <= 0.0 {
                self.timer = 0.0;
                self.target = 1.0;
            }
        }
        // Full 0.5 → 1.6 swing in RESIZE_TRANSITION seconds.
        let rate = (GROW_SCALE - SHRINK_SCALE) / RESIZE_TRANSITION * dt;
        let d = self.target - self.scale;
        self.scale = if d.abs() <= rate { self.target } else { self.scale + rate * d.signum() };
    }

    /// 0..1 how shrunk, 0..1 how grown.
    fn amounts(&self) -> (f32, f32) {
        if self.scale < 1.0 {
            (((1.0 - self.scale) / (1.0 - SHRINK_SCALE)).clamp(0.0, 1.0), 0.0)
        } else {
            (0.0, ((self.scale - 1.0) / (GROW_SCALE - 1.0)).clamp(0.0, 1.0))
        }
    }

    /// Run-speed multiplier (shrink ×1.2, grow ×0.85).
    pub fn speed_mult(&self) -> f32 {
        let (s, g) = self.amounts();
        1.0 + 0.2 * s - 0.15 * g
    }

    /// Knockback-taken multiplier (shrink ×1.6, grow ×0.5).
    pub fn knockback_taken(&self) -> f32 {
        let (s, g) = self.amounts();
        1.0 + 0.6 * s - 0.5 * g
    }

    /// Damage-dealt multiplier (shrink ×0.6 on everything; grow ×1.4 on melee).
    pub fn damage_dealt(&self, melee: bool) -> f32 {
        let (s, g) = self.amounts();
        (1.0 - 0.4 * s) * if melee { 1.0 + 0.4 * g } else { 1.0 }
    }

    /// Melee reach multiplier (grow ×1.4).
    pub fn reach(&self) -> f32 {
        let (_, g) = self.amounts();
        1.0 + 0.4 * g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_to_wall_portal_turns_fall_into_forward_speed() {
        // Enter a floor portal (normal up) falling; exit a wall portal facing +X.
        let a = PortalEnd::new(Vec3::ZERO, Vec3::Y, Vec3::Z, 1.5);
        let b = PortalEnd::new(Vec3::new(20.0, 5.0, 0.0), Vec3::X, Vec3::Y, 1.5);
        let r = portal_rotation(&a, &b);
        let v = r * Vec3::new(0.0, -20.0, 0.0);
        assert!((v - Vec3::new(20.0, 0.0, 0.0)).length() < 1e-3, "{v}");
        let pair = PortalPair { id: 1, owner: PlayerId(0), ends: [a, b], expires: 100, construct: None };
        let t = pair.crossing(Vec3::new(0.2, 0.3, 0.0), Vec3::new(0.2, -0.3, 0.0)).unwrap();
        assert_eq!(t.entry, 0);
        assert!((t.point - b.pos).length() < 1.0);
        // Crossing back-to-front does nothing.
        assert!(pair.crossing(Vec3::new(0.0, -0.3, 0.0), Vec3::new(0.0, 0.3, 0.0)).is_none());
    }

    #[test]
    fn wall_to_wall_keeps_speed() {
        let a = PortalEnd::new(Vec3::ZERO, Vec3::Z, Vec3::Y, 1.5);
        let b = PortalEnd::new(Vec3::new(5.0, 0.0, 0.0), Vec3::NEG_X, Vec3::Y, 1.5);
        let r = portal_rotation(&a, &b);
        let v_in = Vec3::new(1.0, 2.0, -10.0);
        let v_out = r * v_in;
        assert!((v_out.length() - v_in.length()).abs() < 1e-3);
        assert!(v_out.x < -9.0 && (v_out.y - 2.0).abs() < 1e-3);
    }

    #[test]
    fn resize_transitions_smoothly() {
        let mut s = SizeState::default();
        s.apply(GROW_SCALE, RESIZE_SECS);
        s.step(0.1);
        assert!(s.scale > 1.0 && s.scale < GROW_SCALE);
        for _ in 0..30 {
            s.step(0.01);
        }
        assert_eq!(s.scale, GROW_SCALE);
        assert!((s.knockback_taken() - 0.5).abs() < 1e-5);
        for _ in 0..700 {
            s.step(0.01);
        }
        assert_eq!(s.scale, 1.0);
    }

    #[test]
    fn field_falloff() {
        let f = ForceField { id: 0, owner: PlayerId(0), pos: Vec3::ZERO, radius: 10.0, strength: 20.0, expires: 10, construct: None };
        let a = f.accel_at(Vec3::new(5.0, 0.0, 0.0));
        assert!((a - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-4);
        assert_eq!(f.accel_at(Vec3::new(11.0, 0.0, 0.0)), Vec3::ZERO);
    }
}
