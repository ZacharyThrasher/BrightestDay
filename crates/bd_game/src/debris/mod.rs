//! Debris: pooled physical cubes (impact pillar).
//!
//! * Send [`SpawnDebris`] messages (or call [`shatter`]) — up to [`DEBRIS_CAPACITY`] cubes live;
//!   when full, the oldest slots are recycled.
//! * [`DebrisImpulse`] shoves/wakes loose debris (shockwaves).
//! * Collision is pluggable: put a [`DebrisCollider`] in [`DebrisGround`] (e.g. [`VoxelCollider`]
//!   around anything implementing `bd_core::voxel::VoxelQuery`). Without one, debris falls forever
//!   (killed below `kill_y`).
//! * Physics: gravity, drag, spin, per-axis bounce against the collider, ground friction, settle
//!   onto a face, sleep. Cubes shrink (and emissive fades) over the last part of their life.
//! * Simulated once per rendered frame on the compute pool (cosmetic, client-only), rendered with a
//!   single instanced material (see `render`).

mod render;

use crate::tick::FrameSet;
use crate::util::{core_v3, Rng};
use bd_core::voxel::{voxel_min, world_to_voxel, VoxelQuery};
use bd_core::VOXEL_SIZE;
use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;
use std::sync::{Arc, RwLock};

pub use render::DebrisInstance;

/// Total cube budget.
pub const DEBRIS_CAPACITY: usize = 20_480;

#[derive(Message, Clone, Copy, Debug)]
pub struct SpawnDebris {
    pub pos: Vec3,
    pub vel: Vec3,
    /// Linear RGB.
    pub color: [f32; 3],
    /// Edge length in metres.
    pub size: f32,
    /// Emissive strength (0 = plain; 2–8 = glowing hard light).
    pub emissive: f32,
    /// Seconds.
    pub lifetime: f32,
}

/// Radial push on existing debris (explosion shockwave). Wakes sleeping cubes.
#[derive(Message, Clone, Copy, Debug)]
pub struct DebrisImpulse {
    pub center: Vec3,
    pub radius: f32,
    /// Velocity change (m/s) at the centre, falling off linearly to 0 at `radius`.
    pub strength: f32,
}

/// Point queries debris needs. Implemented for every `bd_core::voxel::VoxelQuery`.
pub trait SolidQuery {
    fn is_solid(&self, p: Vec3) -> bool;
    /// Top surface height of the solid cell containing `p`, used to rest cubes exactly on it.
    fn surface_top(&self, _p: Vec3) -> Option<f32> {
        None
    }
}

impl<T: VoxelQuery + ?Sized> SolidQuery for T {
    #[inline]
    fn is_solid(&self, p: Vec3) -> bool {
        VoxelQuery::is_solid(self, world_to_voxel(core_v3(p)))
    }
    #[inline]
    fn surface_top(&self, p: Vec3) -> Option<f32> {
        Some(voxel_min(world_to_voxel(core_v3(p))).y + VOXEL_SIZE)
    }
}

/// Collision world for debris, shared with worker threads. `with_query` is called once per batch
/// of ~1k cubes, so implementations can take a lock once instead of per query.
pub trait DebrisCollider: Send + Sync + 'static {
    fn with_query(&self, f: &mut dyn FnMut(&dyn SolidQuery));
}

/// Immutable voxel data (tests, static maps).
pub struct VoxelCollider<T>(pub Arc<T>);

impl<T: VoxelQuery + Send + Sync + 'static> DebrisCollider for VoxelCollider<T> {
    fn with_query(&self, f: &mut dyn FnMut(&dyn SolidQuery)) {
        f(&*self.0)
    }
}

/// Shared mutable voxel world (the arena: carved on the main thread, read by debris/meshing).
pub struct LockedVoxelCollider<T>(pub Arc<RwLock<T>>);

impl<T: VoxelQuery + Send + Sync + 'static> DebrisCollider for LockedVoxelCollider<T> {
    fn with_query(&self, f: &mut dyn FnMut(&dyn SolidQuery)) {
        let guard = self.0.read().unwrap_or_else(|e| e.into_inner());
        f(&*guard)
    }
}

#[derive(Resource, Default, Clone)]
pub struct DebrisGround(pub Option<Arc<dyn DebrisCollider>>);

#[derive(Resource, Debug, Clone)]
pub struct DebrisSettings {
    pub gravity: f32,
    /// Linear drag (1/s).
    pub drag: f32,
    /// Vertical restitution on landing.
    pub restitution: f32,
    /// Horizontal speed kept per landing impact.
    pub impact_friction: f32,
    /// Continuous ground friction (1/s).
    pub ground_friction: f32,
    /// Portion of life (max seconds) spent shrinking out.
    pub fade_secs: f32,
    pub kill_y: f32,
    /// Multiplies dt (hit-stop hooks in here if the world should freeze too).
    pub time_scale: f32,
    /// The map's gravity by altitude (Orbit: debris drifts in the zero-g band). Set by
    /// `present::space`; `None` = plain `gravity` everywhere.
    pub gravity_profile: Option<bd_voxel::GravityProfile>,
}

impl Default for DebrisSettings {
    fn default() -> Self {
        Self {
            gravity: 22.0,
            drag: 0.35,
            restitution: 0.32,
            impact_friction: 0.72,
            ground_friction: 5.0,
            fade_secs: 0.7,
            kill_y: -120.0,
            time_scale: 1.0,
            gravity_profile: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    rot: Quat,
    ang: Vec3,
    color: [f32; 3],
    size: f32,
    emissive: f32,
    age: f32,
    life: f32,
    sleeping: bool,
    grounded: bool,
    phase: u8,
}

#[derive(Resource)]
pub struct DebrisPool {
    particles: Vec<Particle>,
    instances: Vec<DebrisInstance>,
    recycle_cursor: usize,
    rng: Rng,
    frame: u32,
    prev_written: usize,
    /// Cubes spawned since start (diagnostics).
    pub spawned_total: u64,
}

impl Default for DebrisPool {
    fn default() -> Self {
        Self {
            particles: Vec::with_capacity(DEBRIS_CAPACITY),
            instances: vec![DebrisInstance::HIDDEN; DEBRIS_CAPACITY],
            recycle_cursor: 0,
            rng: Rng::new(0xdeb415),
            frame: 0,
            prev_written: 0,
            spawned_total: 0,
        }
    }
}

impl DebrisPool {
    pub fn live(&self) -> usize {
        self.particles.len()
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    pub(crate) fn instances(&self) -> &[DebrisInstance] {
        &self.instances
    }

    pub fn spawn(&mut self, d: &SpawnDebris) {
        let rng = &mut self.rng;
        let p = Particle {
            pos: d.pos,
            vel: d.vel,
            rot: rng.quat(),
            ang: rng.unit_vec3() * rng.range(4.0, 14.0) * (1.0 + d.vel.length() * 0.03),
            color: d.color,
            size: d.size.max(0.01),
            emissive: d.emissive.max(0.0),
            age: 0.0,
            life: d.lifetime.max(0.05) * rng.range(0.85, 1.15),
            sleeping: false,
            grounded: false,
            phase: (rng.next_u32() & 7) as u8,
        };
        self.spawned_total += 1;
        if self.particles.len() < DEBRIS_CAPACITY {
            self.particles.push(p);
        } else {
            // Recycle: sweep through the pool so the replaced cubes are spread out in age.
            self.recycle_cursor = (self.recycle_cursor + 7919) % DEBRIS_CAPACITY;
            self.particles[self.recycle_cursor] = p;
        }
    }

    pub fn impulse(&mut self, i: &DebrisImpulse) {
        let r2 = i.radius * i.radius;
        for p in &mut self.particles {
            let d = p.pos - i.center;
            let l2 = d.length_squared();
            if l2 < r2 {
                let l = l2.sqrt();
                let falloff = 1.0 - l / i.radius;
                let dir = if l > 1e-3 { d / l } else { Vec3::Y };
                p.vel += (dir + Vec3::Y * 0.5).normalize() * i.strength * falloff;
                p.sleeping = false;
                p.grounded = false;
            }
        }
    }

    /// Advance all cubes by `dt` and rebuild the instance array.
    fn step(&mut self, dt: f32, s: &DebrisSettings, collider: Option<&dyn DebrisCollider>) {
        self.frame = self.frame.wrapping_add(1);
        // Remove the dead (they were drawn at size 0 last frame).
        let mut i = 0;
        while i < self.particles.len() {
            if self.particles[i].age >= self.particles[i].life {
                self.particles.swap_remove(i);
            } else {
                i += 1;
            }
        }
        let live = self.particles.len();
        let frame = self.frame;
        const CHUNK: usize = 1024;
        let particles = &mut self.particles[..];
        let instances = &mut self.instances[..live];
        if live > 2048 {
            ComputeTaskPool::get().scope(|scope| {
                for (pc, ic) in particles.chunks_mut(CHUNK).zip(instances.chunks_mut(CHUNK)) {
                    scope.spawn(async move { step_batch(pc, ic, dt, s, collider, frame) });
                }
            });
        } else {
            step_batch(particles, instances, dt, s, collider, frame);
        }
        // Hide slots that were live last frame but not now.
        if self.prev_written > live {
            self.instances[live..self.prev_written].fill(DebrisInstance::HIDDEN);
        }
        self.prev_written = live;
    }
}

fn step_batch(
    particles: &mut [Particle],
    instances: &mut [DebrisInstance],
    dt: f32,
    s: &DebrisSettings,
    collider: Option<&dyn DebrisCollider>,
    frame: u32,
) {
    let mut run = |col: Option<&dyn SolidQuery>| {
        for (p, inst) in particles.iter_mut().zip(instances.iter_mut()) {
            step_particle(p, dt, s, col, frame);
            *inst = DebrisInstance::from_particle(p, s.fade_secs);
        }
    };
    match collider {
        Some(c) => c.with_query(&mut |q| run(Some(q))),
        None => run(None),
    }
}

#[inline]
fn step_particle(p: &mut Particle, dt: f32, s: &DebrisSettings, col: Option<&dyn SolidQuery>, frame: u32) {
    p.age += dt;
    if p.age >= p.life {
        return;
    }
    let r = p.size * 0.5;
    if p.sleeping {
        // Occasionally re-check support (the ground may have been carved away).
        if (frame & 7) == p.phase as u32
            && let Some(c) = col
            && !c.is_solid(p.pos - Vec3::Y * (r + 0.08))
        {
            p.sleeping = false;
            p.grounded = false;
        }
        return;
    }

    let g = match s.gravity_profile {
        Some(gp) => s.gravity * gp.scale_at(p.pos.y),
        None => s.gravity,
    };
    p.vel.y -= g * dt;
    p.vel *= 1.0 / (1.0 + s.drag * dt);
    let d = p.vel * dt;
    let mut pos = p.pos;
    let was_grounded = p.grounded;
    p.grounded = false;

    if let Some(c) = col {
        // Y first (landing is what matters most visually).
        let ny = pos.y + d.y;
        let probe = Vec3::new(pos.x, ny + r * d.y.signum(), pos.z);
        if d.y != 0.0 && c.is_solid(probe) {
            if d.y < 0.0 {
                pos.y = c.surface_top(probe).map(|t| t + r).unwrap_or(pos.y).max(ny);
                p.grounded = true;
                let impact = -p.vel.y;
                p.vel.y = if impact > 2.5 { impact * s.restitution } else { 0.0 };
                if !was_grounded {
                    p.vel.x *= s.impact_friction;
                    p.vel.z *= s.impact_friction;
                    p.ang *= 0.6;
                }
            } else {
                p.vel.y = -p.vel.y * 0.2;
            }
        } else {
            pos.y = ny;
        }
        // X / Z: bounce off walls.
        let nx = pos.x + d.x;
        if d.x != 0.0 && c.is_solid(Vec3::new(nx + r * d.x.signum(), pos.y, pos.z)) {
            p.vel.x = -p.vel.x * s.restitution;
            p.ang *= 0.7;
        } else {
            pos.x = nx;
        }
        let nz = pos.z + d.z;
        if d.z != 0.0 && c.is_solid(Vec3::new(pos.x, pos.y, nz + r * d.z.signum())) {
            p.vel.z = -p.vel.z * s.restitution;
            p.ang *= 0.7;
        } else {
            pos.z = nz;
        }
    } else {
        pos += d;
    }
    p.pos = pos;

    if p.grounded {
        let f = (-s.ground_friction * dt).exp();
        p.vel.x *= f;
        p.vel.z *= f;
        p.ang *= (-7.0 * dt).exp();
        // Settle: roll toward the nearest face-down orientation.
        let m = Mat3::from_quat(p.rot);
        let axes = [m.x_axis, m.y_axis, m.z_axis];
        let mut best = axes[0];
        for a in &axes[1..] {
            if a.y.abs() > best.y.abs() {
                best = *a;
            }
        }
        let up = best * best.y.signum();
        let target = Quat::from_rotation_arc(up, Vec3::Y) * p.rot;
        p.rot = p.rot.slerp(target, 1.0 - (-10.0 * dt).exp());
        if p.vel.length_squared() < 0.05 && p.ang.length_squared() < 0.5 {
            p.vel = Vec3::ZERO;
            p.ang = Vec3::ZERO;
            p.rot = target;
            p.sleeping = true;
        }
    } else {
        let a = p.ang * dt;
        p.rot = (Quat::from_scaled_axis(a) * p.rot).normalize();
    }
    if p.pos.y < s.kill_y {
        p.age = p.life;
    }
}

/// Parameters for [`shatter`].
#[derive(Clone, Copy, Debug)]
pub struct ShatterParams {
    /// Where the blow came from (voxels fly away from it).
    pub origin: Vec3,
    /// Outward speed (m/s) for a voxel right next to the origin.
    pub strength: f32,
    /// Velocity added to every piece (the hit's / the flying body's velocity).
    pub inherit_vel: Vec3,
    /// Edge length of the source voxels.
    pub voxel_size: f32,
    /// Split each voxel into `subdivide³` smaller cubes (1 = keep whole).
    pub subdivide: u32,
    /// Min/max lifetime, seconds.
    pub lifetime: (f32, f32),
    pub emissive: f32,
    /// Extra upward kick (m/s) so debris arcs instead of skimming.
    pub lift: f32,
    /// Random velocity jitter as a fraction of the outward speed.
    pub jitter: f32,
}

impl Default for ShatterParams {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            strength: 12.0,
            inherit_vel: Vec3::ZERO,
            voxel_size: VOXEL_SIZE,
            subdivide: 1,
            lifetime: (3.0, 6.0),
            emissive: 0.0,
            lift: 4.0,
            jitter: 0.35,
        }
    }
}

/// Turn a set of voxels (world centres + linear colours) into flying debris.
/// Speed falls off with distance from `origin` (1/(1 + d/2)), so the blast core is violent and
/// the rim just crumbles.
pub fn shatter(
    out: &mut MessageWriter<SpawnDebris>,
    rng: &mut Rng,
    voxels: impl IntoIterator<Item = (Vec3, [f32; 3])>,
    p: &ShatterParams,
) {
    let n = p.subdivide.max(1);
    let sub = p.voxel_size / n as f32;
    for (center, color) in voxels {
        for ix in 0..n {
            for iy in 0..n {
                for iz in 0..n {
                    let off = (Vec3::new(ix as f32, iy as f32, iz as f32) + 0.5) * sub - Vec3::splat(p.voxel_size * 0.5);
                    let pos = center + off;
                    let away = pos - p.origin;
                    let dist = away.length();
                    let dir = if dist > 1e-3 { away / dist } else { rng.unit_vec3() };
                    let speed = p.strength / (1.0 + dist * 0.5);
                    let jitter = rng.unit_vec3() * speed * p.jitter;
                    let vel = p.inherit_vel + dir * speed * rng.range(0.7, 1.15) + jitter + Vec3::Y * p.lift * rng.range(0.5, 1.2);
                    // Slight colour variation so a cloud of same-material cubes still reads as many.
                    let shade = rng.range(0.88, 1.08);
                    out.write(SpawnDebris {
                        pos,
                        vel,
                        color: [color[0] * shade, color[1] * shade, color[2] * shade],
                        size: sub * rng.range(0.82, 0.98),
                        emissive: p.emissive,
                        lifetime: rng.range(p.lifetime.0, p.lifetime.1),
                    });
                }
            }
        }
    }
}

/// RNG resource for gameplay-side VFX helpers (e.g. `shatter`).
#[derive(Resource)]
pub struct VfxRng(pub Rng);

impl Default for VfxRng {
    fn default() -> Self {
        Self(Rng::new(0x5eed_f00d))
    }
}

/// The debris simulation step (spawns are consumed here). Send `SpawnDebris` before it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DebrisSimSet;

pub struct DebrisPlugin;

impl Plugin for DebrisPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnDebris>()
            .add_message::<DebrisImpulse>()
            .init_resource::<DebrisPool>()
            .init_resource::<DebrisSettings>()
            .init_resource::<DebrisGround>()
            .init_resource::<VfxRng>()
            .add_plugins(render::DebrisRenderPlugin)
            .add_systems(Update, simulate_debris.in_set(FrameSet::Present).in_set(DebrisSimSet));
    }
}

fn simulate_debris(
    time: Res<Time<Real>>,
    settings: Res<DebrisSettings>,
    ground: Res<DebrisGround>,
    mut spawns: MessageReader<SpawnDebris>,
    mut impulses: MessageReader<DebrisImpulse>,
    mut pool: ResMut<DebrisPool>,
) {
    for s in spawns.read() {
        pool.spawn(s);
    }
    for i in impulses.read() {
        pool.impulse(i);
    }
    let dt = (time.delta_secs() * settings.time_scale).min(1.0 / 20.0);
    let collider = ground.0.as_deref();
    pool.step(dt, &settings, collider);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Floor;
    impl VoxelQuery for Floor {
        fn material(&self, v: bd_core::IVec3) -> bd_core::voxel::Material {
            if v.y < 0 { 1 } else { 0 }
        }
    }

    #[test]
    fn cubes_land_and_sleep_on_floor() {
        let mut pool = DebrisPool::default();
        let col = VoxelCollider(Arc::new(Floor));
        pool.spawn(&SpawnDebris {
            pos: Vec3::new(0.3, 5.0, 0.3),
            vel: Vec3::new(3.0, 0.0, 0.0),
            color: [1.0; 3],
            size: 0.4,
            emissive: 0.0,
            lifetime: 100.0,
        });
        let s = DebrisSettings::default();
        for _ in 0..(144 * 6) {
            pool.step(1.0 / 144.0, &s, Some(&col));
        }
        let p = pool.particles[0];
        assert!(p.sleeping, "should settle: {p:?}");
        assert!((p.pos.y - 0.2).abs() < 1e-3, "rests on the floor: {}", p.pos.y);
    }

    #[test]
    fn capacity_recycles() {
        let mut pool = DebrisPool::default();
        let d = SpawnDebris { pos: Vec3::ZERO, vel: Vec3::ZERO, color: [1.0; 3], size: 0.2, emissive: 0.0, lifetime: 5.0 };
        for _ in 0..(DEBRIS_CAPACITY + 500) {
            pool.spawn(&d);
        }
        assert_eq!(pool.live(), DEBRIS_CAPACITY);
    }
}
