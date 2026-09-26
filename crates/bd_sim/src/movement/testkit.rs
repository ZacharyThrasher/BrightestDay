//! Tooling for tests, examples and benchmarks: a tiny dense voxel grid and scripted-input helpers
//! (including a perfect air-strafe "bot"). Not used by the game at runtime.

use bd_core::input::{PlayerInput, buttons};
use bd_core::voxel::{AIR, Material, VoxelQuery, world_to_voxel};
use bd_core::{IVec3, Vec3};

use super::params::MoveParams;

/// Dense voxel grid over `[min, min + size)`. Everything outside is air.
#[derive(Clone, Debug)]
pub struct TestGrid {
    min: IVec3,
    size: IVec3,
    cells: Vec<Material>,
}

impl TestGrid {
    pub fn new(min: IVec3, max_exclusive: IVec3) -> Self {
        let size = (max_exclusive - min).max(IVec3::ZERO);
        Self { min, size, cells: vec![AIR; (size.x * size.y * size.z) as usize] }
    }

    #[inline]
    fn index(&self, v: IVec3) -> Option<usize> {
        let l = v - self.min;
        if l.cmplt(IVec3::ZERO).any() || l.cmpge(self.size).any() {
            return None;
        }
        Some(((l.y * self.size.z + l.z) * self.size.x + l.x) as usize)
    }

    pub fn set(&mut self, v: IVec3, m: Material) {
        if let Some(i) = self.index(v) {
            self.cells[i] = m;
        }
    }

    /// Fill voxel coordinates `lo..=hi`.
    pub fn fill(&mut self, lo: IVec3, hi: IVec3, m: Material) {
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    self.set(IVec3::new(x, y, z), m);
                }
            }
        }
    }

    /// Fill every voxel whose centre lies in the world-space box `[lo, hi]`.
    pub fn fill_world(&mut self, lo: Vec3, hi: Vec3, m: Material) {
        let a = world_to_voxel(lo + Vec3::splat(0.25));
        let b = world_to_voxel(hi - Vec3::splat(0.25));
        self.fill(a, b, m);
    }

    /// A flat floor whose top surface is at y = 0 covering `[-half, half]` in x and z.
    pub fn flat(half: i32) -> Self {
        let mut g = Self::new(IVec3::new(-half, -4, -half), IVec3::new(half, 24, half));
        g.fill(IVec3::new(-half, -2, -half), IVec3::new(half - 1, -1, half - 1), 1);
        g
    }
}

impl VoxelQuery for TestGrid {
    #[inline]
    fn material(&self, v: IVec3) -> Material {
        match self.index(v) {
            Some(i) => self.cells[i],
            None => AIR,
        }
    }
}

/// Yaw whose camera-relative `(move_x, move_z)` wish direction points along horizontal `dir`.
pub fn yaw_for_wish(dir: Vec3, move_x: i8, move_z: i8) -> f32 {
    // wish = fwd*mz + right*mx with fwd = (-sin y, 0, -cos y), right = (cos y, 0, -sin y).
    // The wish is the camera forward rotated by atan2(mx, mz) clockwise (seen from above).
    let fwd_yaw = (-dir.x).atan2(-dir.z);
    let offset = (move_x as f32).atan2(move_z as f32);
    fwd_yaw + offset
}

/// Yaw/pitch that look from `eye` at `target`.
pub fn aim_at(eye: Vec3, target: Vec3) -> (f32, f32) {
    let d = (target - eye).normalize_or_zero();
    let yaw = (-d.x).atan2(-d.z);
    let pitch = d.y.clamp(-1.0, 1.0).asin();
    (yaw, pitch)
}

/// A voxel world defined by a predicate (e.g. an infinite floor: `FnWorld(|v| v.y < 0)`).
pub struct FnWorld<F: Fn(IVec3) -> bool>(pub F);

impl<F: Fn(IVec3) -> bool> VoxelQuery for FnWorld<F> {
    #[inline]
    fn material(&self, v: IVec3) -> Material {
        if (self.0)(v) { 1 } else { AIR }
    }
}

/// Perfect Quake air-strafe: the yaw that makes `(move_x, move_z)` point at the optimal angle to the
/// current horizontal velocity, turning toward `turn_sign` (+1 = clockwise/right, −1 = left).
pub fn optimal_strafe_yaw(vel: Vec3, p: &MoveParams, dt: f32, move_x: i8, move_z: i8, turn_sign: f32) -> f32 {
    let h = Vec3::new(vel.x, 0.0, vel.z);
    let sp = h.length();
    if sp < 0.1 {
        return 0.0;
    }
    let vdir = h / sp;
    let accel = p.air_accel * p.run_speed * dt;
    let cap = p.air_wish_cap.min(p.run_speed);
    // Optimal: v·w = cap − accel (clamped so the angle exists); at high accel just go perpendicular.
    let cos_t = ((cap - accel) / sp).clamp(-1.0, 1.0).max(0.0);
    let theta = cos_t.acos();
    // Rotate vdir about +Y by −turn_sign·theta (clockwise from above for turn_sign = +1).
    let a = -turn_sign * theta;
    let (s, c) = a.sin_cos();
    let wish = Vec3::new(vdir.x * c + vdir.z * s, 0.0, -vdir.x * s + vdir.z * c);
    yaw_for_wish(wish, move_x, move_z)
}

/// Convenience input builder.
pub fn input(yaw: f32, pitch: f32, move_x: i8, move_z: i8, buttons_held: u16) -> PlayerInput {
    PlayerInput { seq: 0, yaw, pitch, move_x, move_z, buttons: buttons_held, reel_notches: 0 }
}

pub use buttons::{BOOST, CROUCH, JUMP, REEL, TETHER_L, TETHER_R};

/// Reference scenarios measuring each speed tier with scripted "skilled" inputs. Shared by the tier
/// test and `examples/trajectory.rs`.
pub mod scenarios {
    use super::*;
    use crate::movement::{Collider, ColliderId, CollisionWorld, PlayerState, step_player};
    use bd_core::DT;
    use std::f32::consts::FRAC_PI_2;

    const YAW_PX: f32 = -FRAC_PI_2;

    /// Floor at y = 0 plus 2 m anchor cubes floating at y ∈ [24, 26] every 30 m (x, z ∈ 30k + [0, 2]).
    pub fn arena() -> FnWorld<impl Fn(IVec3) -> bool> {
        FnWorld(|v: IVec3| v.y < 0 || ((48..52).contains(&v.y) && v.x.rem_euclid(60) < 4 && v.z.rem_euclid(60) < 4))
    }

    /// Centre of the underside of the lattice anchor cube nearest to `p` (horizontally).
    pub fn lattice_anchor(p: Vec3) -> Vec3 {
        let snap = |c: f32| (c / 30.0).round() * 30.0 + 1.0;
        Vec3::new(snap(p.x), 24.0, snap(p.z))
    }

    /// Zig-zag strafing along `route`: keep strafing to one side until the heading is 20° off the route,
    /// then switch (returns the turn side for [`optimal_strafe_yaw`], +1 = clockwise).
    pub fn zigzag_side(vel: Vec3, route: Vec3, side: f32) -> f32 {
        let h = Vec3::new(vel.x, 0.0, vel.z);
        if h.length_squared() < 1e-4 {
            return side;
        }
        // Signed CCW angle (seen from above) from route to heading.
        let e = route.cross(h).y.atan2(route.dot(h));
        let lim = 20f32.to_radians();
        if e > lim {
            1.0
        } else if e < -lim {
            -1.0
        } else {
            side
        }
    }

    fn grounded_at(x: f32, p: &MoveParams, w: &CollisionWorld<impl VoxelQuery + ?Sized>) -> PlayerState {
        let mut s = PlayerState::new(Vec3::new(x, 0.01, 1.0), p);
        for _ in 0..5 {
            step_player(&mut s, &input(YAW_PX, 0.0, 0, 0, 0), w, p, DT);
        }
        s
    }

    /// Steady-state running speed.
    pub fn run(p: &MoveParams) -> f32 {
        let w = arena();
        let cw = CollisionWorld::new(&w, &[]);
        let mut s = grounded_at(0.0, p, &cw);
        for _ in 0..240 {
            step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, 0), &cw, p, DT);
        }
        s.horizontal_speed()
    }

    /// Bunny-hop from run speed with perfect strafing for `secs`. `perfect_timing` taps jump inside the
    /// buffer window (friction-free hops) instead of holding it. Zig-zags (alternates strafe side each
    /// hop) like a player going somewhere. Returns (average, final) horizontal speed over the last second.
    pub fn bhop(p: &MoveParams, secs: f32, perfect_timing: bool) -> (f32, f32) {
        let w = arena();
        let cw = CollisionWorld::new(&w, &[]);
        let mut s = grounded_at(0.0, p, &cw);
        s.vel = Vec3::new(p.run_speed, 0.0, 0.0);
        let n = (secs / DT) as usize;
        let mut side = 1.0f32;
        let mut sum = 0.0;
        let mut cnt = 0;
        for t in 0..n {
            side = zigzag_side(s.vel, Vec3::X, side);
            let mx: i8 = if side > 0.0 { 1 } else { -1 };
            let yaw = optimal_strafe_yaw(s.vel, p, DT, mx, 0, side);
            let jump = if perfect_timing {
                let near = s.vel.y < 0.0 && s.pos.y < 0.5;
                (near || s.grounded) && t % 2 == 0
            } else {
                true
            };
            step_player(&mut s, &input(yaw, 0.0, mx, 0, if jump { JUMP } else { 0 }), &cw, p, DT);
            if t + 120 >= n {
                sum += s.horizontal_speed();
                cnt += 1;
            }
        }
        (sum / cnt as f32, s.horizontal_speed())
    }

    /// Run, crouch into a slide (entry burst): speed right after the burst and after 0.5 s.
    pub fn slide_flat(p: &MoveParams) -> (f32, f32) {
        let w = arena();
        let cw = CollisionWorld::new(&w, &[]);
        let mut s = grounded_at(0.0, p, &cw);
        for _ in 0..120 {
            step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, 0), &cw, p, DT);
        }
        step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, CROUCH), &cw, p, DT);
        let peak = s.horizontal_speed();
        for _ in 0..60 {
            step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, CROUCH), &cw, p, DT);
        }
        (peak, s.horizontal_speed())
    }

    /// Bhop-speed slide down a 20° construct ramp 20 m long: speed at the bottom.
    pub fn slide_ramp(p: &MoveParams) -> f32 {
        let len = 20.0;
        let a = 20f32.to_radians();
        // Ascends toward −X; low edge at x = 0 on the floor.
        let ramp = Collider::ramp(Vec3::new(0.0, 0.0, 1.0), FRAC_PI_2, a, len, 6.0, 1.0, ColliderId::Other(1));
        let w = FnWorld(|v: IVec3| v.y < 0);
        let cols = [ramp];
        let cw = CollisionWorld::new(&w, &cols);
        let top = ramp.to_world(Vec3::new(0.0, ramp.half.y, -ramp.half.z + 0.5));
        let mut s = PlayerState::new(top + Vec3::new(0.0, 0.3, 0.0), p);
        s.vel = Vec3::new(14.0, 0.0, 0.0);
        let mut best = 0.0f32;
        for _ in 0..360 {
            step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, CROUCH), &cw, p, DT);
            if s.pos.x > 0.5 {
                break;
            }
            best = best.max(s.speed());
        }
        best
    }

    /// What a skilled player does while swinging on `hand`: look along the swing, reel in while the
    /// arc would scrape the ground, optionally gas-boost along the downswing.
    pub fn swing_input(s: &PlayerState, p: &MoveParams, hand: u16, boost: bool) -> PlayerInput {
        let h = s.hooks.iter().find(|h| h.attached());
        let Some(h) = h else {
            return input(0.0, 0.0, 0, 0, hand);
        };
        let (yaw, pitch) = aim_at(Vec3::ZERO, s.vel);
        let mut b = hand;
        // Lowest point of the arc (feet) must clear the ground by a few metres.
        let feet_low = h.pos.y - h.rope_len - p.stand_height * 0.6;
        if feet_low < 3.0 {
            b |= REEL;
        }
        let boosting = boost && s.vel.y < 0.0;
        if boosting {
            b |= BOOST;
        }
        input(yaw, pitch, 0, boosting as i8, b)
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct SwingResult {
        pub peak: f32,
        pub release: f32,
        /// Speed 0.5 s after release.
        pub after: f32,
        /// Horizontal distance covered from the jump to 0.5 s after release.
        pub distance: f32,
    }

    /// Hop at bhop speed (14 m/s), hook the lattice anchor ahead at the apex, swing (reeling in only as
    /// much as needed to clear the ground, optionally gas-boosting along the downswing), release on the
    /// upswing.
    pub fn swing(p: &MoveParams, boost: bool) -> SwingResult {
        let w = arena();
        let cw = CollisionWorld::new(&w, &[]);
        let mut s = grounded_at(-22.0, p, &cw);
        s.vel = Vec3::new(14.0, 0.0, 0.0);
        let x0 = s.pos.x;
        let anchor = lattice_anchor(Vec3::new(0.0, 0.0, 0.0));
        let mut r = SwingResult::default();
        let mut phase = 0; // 0 hop, 1 swing, 2 released
        let mut t_rel = 0usize;
        for t in 0..1200 {
            let i = match phase {
                0 => {
                    if t > 5 && s.vel.y <= 0.0 {
                        let (yaw, pitch) = aim_at(s.eye(p), anchor);
                        phase = 1;
                        input(yaw, pitch, 0, 0, TETHER_L)
                    } else {
                        input(YAW_PX, 0.0, 0, 0, if t < 2 { JUMP } else { 0 })
                    }
                }
                1 => {
                    let past = s.hooks[0].attached() && s.pos.x > anchor.x && s.vel.y > 0.4 * s.speed();
                    if past {
                        phase = 2;
                        t_rel = t;
                        input(YAW_PX, 0.0, 0, 0, 0)
                    } else if s.hooks[0].attached() {
                        swing_input(&s, p, TETHER_L, boost)
                    } else {
                        let (yaw, pitch) = aim_at(s.eye(p), anchor);
                        input(yaw, pitch, 0, 0, TETHER_L)
                    }
                }
                _ => input(YAW_PX, 0.0, 0, 0, 0),
            };
            let e = step_player(&mut s, &i, &cw, p, DT);
            if s.hooks[0].attached() {
                r.peak = r.peak.max(s.speed());
            }
            if e.tether_released[0] {
                r.release = s.speed();
            }
            if phase == 2 && (t == t_rel + 60 || s.grounded) {
                r.after = s.speed();
                r.distance = s.pos.x - x0;
                break;
            }
        }
        r
    }

    /// Flight-construct cruise reference (thrust modifier, gravity compensated).
    pub fn flight(p: &MoveParams) -> f32 {
        let w = arena();
        let cw = CollisionWorld::new(&w, &[]);
        let mut s = PlayerState::new(Vec3::new(0.0, 10.0, 1.0), p);
        s.mods.thrust = 30.0;
        s.mods.gravity_scale = 0.0;
        for _ in 0..480 {
            step_player(&mut s, &input(YAW_PX, 0.0, 0, 1, 0), &cw, p, DT);
        }
        s.speed()
    }
}
