//! Analytic GPU particle pool: emission API, ring allocation, batch visibility.
//!
//! Layout of the particle buffer: `[0, IMM_CAP)` = immediate region (rewritten each frame: arcs,
//! lightning), `[IMM_CAP, IMM_CAP + RING_CAP)` = ring of analytic particles written once at spawn.
//! Batches of [`BATCH`] cubes are separate entities (same mesh + material → one multi-draw); a ring
//! batch is visible only while one of its particles can still be alive.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology};
use bevy::prelude::*;

use crate::gpu::{BATCH, FxUpload, IMM_CAP, ParticleGpu, RING_CAP};
use crate::material::FxMaterials;
use crate::util::FxRng;
use crate::{FxSet, FxSettings, FxStats};

/// Motion model (see `particles.wgsl`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PKind {
    /// p0 + v0·t (drag) + ½gt² (gravity scale), optional floor. Optional anchor frame.
    #[default]
    Ballistic = 0,
    /// Ring (anchor.ring) → anchor-local voxel slot `vel` (or back with `REVERSE`).
    Stream = 1,
    /// Orbit around anchor + `pos`: vel = (radius, angular speed, radial speed).
    Orbit = 2,
    /// Rising (gravity = rise speed m/s) with wobble (spin = wobble amplitude).
    Rise = 3,
    /// Stationary expanding flash.
    Flash = 4,
    /// Immediate-region cube: vel = segment vector (or quaternion with `QUAT`).
    Static = 5,
    /// Timed segment (beam): pos = centre, vel = segment vector; thick core thinning to a trail.
    Beam = 6,
}

/// Particle flags.
pub struct PFlags;
impl PFlags {
    /// Sun-lit, non-emissive (smoke).
    pub const SMOKE: u32 = 1;
    /// Align to velocity and stretch by `spin` (≥ 1).
    pub const STRETCH: u32 = 2;
    /// Settle on `aux` = floor height.
    pub const FLOOR: u32 = 4;
    /// Stream from the voxel back to the ring.
    pub const REVERSE: u32 = 8;
    /// Brief white-hot flash at birth.
    pub const WHITE_BIRTH: u32 = 16;
    /// Flat colour (no edge lines).
    pub const PLAIN: u32 = 32;
    /// Static: `vel` holds a quaternion; `scale` holds per-axis size.
    pub const QUAT: u32 = 64;
    /// Size grows then fades (smoke puffs).
    pub const GROW: u32 = 128;
    /// Size grows then pops (bubbles).
    pub const POP: u32 = 256;
    /// Ballistic: bounce on `aux` = floor height (restitution ~0.45, friction), then rest.
    pub const BOUNCE: u32 = 512;
}

/// One particle spawn request.
#[derive(Clone, Copy, Debug)]
pub struct Emit {
    pub kind: PKind,
    pub flags: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub size: f32,
    /// Linear rgb.
    pub color: [f32; 3],
    /// Emission multiplier (≥ 1 blooms with the game's threshold-1 bloom).
    pub emissive: f32,
    pub life: f32,
    pub gravity: f32,
    pub drag: f32,
    pub spin: f32,
    pub anchor: u32,
    pub seed: u32,
    pub aux: f32,
    /// Start this many seconds in the future (the particle is invisible until then).
    pub delay: f32,
}

impl Default for Emit {
    fn default() -> Self {
        Emit {
            kind: PKind::Ballistic,
            flags: 0,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            size: 0.1,
            color: [1.0; 3],
            emissive: 2.0,
            life: 1.0,
            gravity: 1.0,
            drag: 0.0,
            spin: 0.0,
            anchor: 0,
            seed: 0,
            aux: 0.0,
            delay: 0.0,
        }
    }
}

impl Emit {
    #[inline]
    fn to_gpu(self, now: f32) -> ParticleGpu {
        let delay = if self.delay.is_finite() { self.delay.clamp(0.0, 30.0) } else { 0.0 };
        let t0 = (now + delay) % 3600.0;
        let life = if self.life.is_finite() { self.life.clamp(0.01, 60.0) } else { 1.0 };
        let size = if self.size.is_finite() { self.size.clamp(0.0, 50.0) } else { 0.0 };
        let p = if self.pos.is_finite() { self.pos } else { Vec3::ZERO };
        let v = if self.vel.is_finite() { self.vel } else { Vec3::ZERO };
        ParticleGpu {
            p0: [p.x, p.y, p.z, size],
            v0: [v.x, v.y, v.z, t0],
            color: [self.color[0], self.color[1], self.color[2], self.emissive],
            a: [life, self.gravity, self.drag.max(0.0), self.spin],
            b: [(self.kind as u32) | (self.flags << 8), self.anchor, self.seed, self.aux.to_bits()],
        }
    }
}

/// Particle pool front-end. Use [`FxParticles::emit`] from any system in [`FxSet::Emit`] (or
/// earlier in the frame).
#[derive(Resource)]
pub struct FxParticles {
    cursor: u32,
    frame_start: u32,
    pending: Vec<ParticleGpu>,
    batch_death: Vec<f64>,
    imm: Vec<ParticleGpu>,
    imm_prev: usize,
    /// GPU time (Time::elapsed_secs_wrapped) and absolute time.
    now: f32,
    now_abs: f64,
    budget: usize,
    dropped: usize,
    pub rng: FxRng,
    /// Immediate scope ([`FxParticles::scope`]): a module's cap on the shared immediate region and
    /// a distance floor for line / box thickness (camera, metres per metre of distance).
    imm_limit: usize,
    floor: Option<(Vec3, f32)>,
    scope_dropped: usize,
}

impl Default for FxParticles {
    fn default() -> Self {
        FxParticles {
            cursor: 0,
            frame_start: 0,
            pending: Vec::with_capacity(16_384),
            batch_death: vec![0.0; RING_CAP / BATCH],
            imm: Vec::with_capacity(IMM_CAP),
            imm_prev: 0,
            now: 0.0,
            now_abs: 0.0,
            budget: 12_000,
            dropped: 0,
            rng: FxRng::new(0xB0D1),
            imm_limit: IMM_CAP,
            floor: None,
            scope_dropped: 0,
        }
    }
}

impl FxParticles {
    /// Spawn one ring particle. Returns false if this frame's budget is exhausted.
    #[inline]
    pub fn emit(&mut self, e: Emit) -> bool {
        if self.pending.len() >= self.budget {
            self.dropped += 1;
            return false;
        }
        let idx = (self.frame_start as usize + self.pending.len()) % RING_CAP;
        let b = idx / BATCH;
        let death = self.now_abs + (e.life.clamp(0.0, 60.0) + e.delay.clamp(0.0, 30.0)) as f64 + 0.05;
        if self.batch_death[b] < death {
            self.batch_death[b] = death;
        }
        self.pending.push(e.to_gpu(self.now));
        true
    }

    /// Remaining spawn budget this frame.
    pub fn remaining(&self) -> usize {
        self.budget.saturating_sub(self.pending.len())
    }

    /// Place an immediate (this-frame-only) cube. `kind` must be [`PKind::Static`].
    #[inline]
    pub fn immediate(&mut self, mut e: Emit) -> bool {
        if self.imm.len() >= self.imm_limit {
            self.scope_dropped += 1;
            return false;
        }
        if let Some((cam, k)) = self.floor {
            e.size = e.size.max(e.pos.distance(cam) * k);
            (e.size, e.emissive) = fade_thin(e.size, e.color, e.emissive);
        }
        self.imm.push(e.to_gpu(self.now));
        true
    }

    /// Open an immediate SCOPE for one presentation module: at most `budget` more immediate cubes
    /// this scope (so one module's burst of markers can't starve every other module's lines), and
    /// every immediate line / box at least `px` × its distance from `cam` thick (≈ a pixel floor: a
    /// 3 cm line at 40 m is sub-pixel). Close it with [`FxParticles::end_scope`].
    pub fn scope(&mut self, cam: Vec3, px: f32, budget: usize) {
        self.imm_limit = (self.imm.len() + budget).min(IMM_CAP);
        self.floor = (px > 0.0 && cam.is_finite()).then_some((cam, px));
        self.scope_dropped = 0;
    }

    /// Close the scope; returns how many immediate cubes it dropped over its budget.
    pub fn end_scope(&mut self) -> usize {
        self.imm_limit = IMM_CAP;
        self.floor = None;
        std::mem::take(&mut self.scope_dropped)
    }

    /// Immediate cubes left in the current scope (the whole region outside a scope).
    pub fn imm_left(&self) -> usize {
        self.imm_limit.saturating_sub(self.imm.len())
    }

    /// Immediate oriented box: centre, rotation, full size per axis.
    #[inline]
    pub fn immediate_box(&mut self, centre: Vec3, rot: Quat, size: Vec3, color: [f32; 3], emissive: f32, flags: u32) -> bool {
        if self.imm.len() >= self.imm_limit {
            self.scope_dropped += 1;
            return false;
        }
        let q = if rot.is_finite() { rot } else { Quat::IDENTITY };
        let c = if centre.is_finite() { centre } else { Vec3::ZERO };
        let (size, emissive) = match self.floor {
            Some((cam, k)) => {
                let (f, em) = fade_thin(1.0, color, emissive);
                (size.max(Vec3::splat(c.distance(cam) * k)) * f, em)
            }
            None => (size, emissive),
        };
        self.imm.push(ParticleGpu {
            p0: [c.x, c.y, c.z, 1.0],
            v0: [q.x, q.y, q.z, q.w],
            color: [color[0], color[1], color[2], emissive],
            a: [size.x.max(0.0), size.y.max(0.0), size.z.max(0.0), 0.0],
            b: [(PKind::Static as u32) | ((flags | PFlags::QUAT) << 8), 0, 0, 0],
        });
        true
    }

    /// Immediate segment cube from `a` to `b` with square cross-section `thickness`.
    #[inline]
    pub fn immediate_segment(&mut self, a: Vec3, b: Vec3, thickness: f32, color: [f32; 3], emissive: f32, flags: u32) -> bool {
        self.immediate(Emit {
            kind: PKind::Static,
            flags,
            pos: (a + b) * 0.5,
            vel: b - a,
            size: thickness,
            color,
            emissive,
            ..default()
        })
    }

    /// GPU time base used for `v0.w`.
    pub fn now(&self) -> f32 {
        self.now
    }
}

/// Immediate cubes are opaque and their colour is `color × emissive`, so a bright line FADED by its
/// emissive goes black (the dark rings / black blocks at the end of a fade). Inside a scope a bright
/// colour fading below 1 thins instead (area ≈ light), keeping its colour; deliberately dark colours
/// (smoke, ink, a void core) keep their low emissive.
#[inline]
fn fade_thin(size: f32, color: [f32; 3], emissive: f32) -> (f32, f32) {
    let bright = color[0].max(color[1]).max(color[2]);
    if emissive >= 1.0 || bright < 0.3 || !emissive.is_finite() {
        return (size, emissive);
    }
    let k = emissive.max(0.0);
    (size * k.sqrt(), 1.0)
}

#[derive(Component)]
struct ParticleBatch {
    /// Ring batch index, or None for immediate batches.
    ring: Option<usize>,
    imm: usize,
}

pub fn build(app: &mut App) {
    app.init_resource::<FxParticles>()
        .add_systems(Startup, spawn_batches)
        .add_systems(First, begin_frame)
        .add_systems(PostUpdate, finalize.in_set(FxSet::Emit).after(EmitSystems));
    app.configure_sets(PostUpdate, EmitSystems.in_set(FxSet::Emit));
}

/// Every emitting system goes in here; `finalize` runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmitSystems;

/// `BATCH` unit cubes in one mesh: uv = face corner, uv_b.x = cube index in the batch.
pub fn batch_mesh() -> Mesh {
    const FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let mut positions = Vec::with_capacity(BATCH * 24);
    let mut normals = Vec::with_capacity(BATCH * 24);
    let mut uvs = Vec::with_capacity(BATCH * 24);
    let mut uvb = Vec::with_capacity(BATCH * 24);
    let mut indices = Vec::with_capacity(BATCH * 36);
    for cube in 0..BATCH {
        for (n, u, v) in FACES {
            let (n, u, v) = (Vec3::from(n), Vec3::from(u), Vec3::from(v));
            let base = positions.len() as u32;
            for (su, sv) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let p = (n + u * su + v * sv) * 0.5;
                positions.push(p.to_array());
                normals.push(n.to_array());
                uvs.push([(su > 0.0) as u8 as f32, (sv > 0.0) as u8 as f32]);
                uvb.push([cube as f32, 0.0]);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uvb)
        .with_inserted_indices(Indices::U32(indices))
}

/// The shared batch-of-cubes mesh (handle kept alive for the lifetime of the app).
#[derive(Resource)]
pub struct ParticleBatchMesh(#[allow(dead_code)] pub Handle<Mesh>);

fn spawn_batches(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mats: Res<FxMaterials>) {
    let mesh = meshes.add(batch_mesh());
    commands.insert_resource(ParticleBatchMesh(mesh.clone()));
    // Immediate batches (batch 0 always visible: keeps the pipeline compiled from the first frame).
    for i in 0..IMM_CAP / BATCH {
        commands.spawn((
            Name::new(format!("bd_fx imm batch {i}")),
            ParticleBatch { ring: None, imm: i },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mats.particles.clone()),
            MeshTag((i * BATCH) as u32),
            Transform::IDENTITY,
            NoFrustumCulling,
            NotShadowCaster,
            if i == 0 { Visibility::Visible } else { Visibility::Hidden },
        ));
    }
    for b in 0..RING_CAP / BATCH {
        commands.spawn((
            Name::new(format!("bd_fx ring batch {b}")),
            ParticleBatch { ring: Some(b), imm: 0 },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mats.particles.clone()),
            MeshTag((IMM_CAP + b * BATCH) as u32),
            Transform::IDENTITY,
            NoFrustumCulling,
            NotShadowCaster,
            Visibility::Hidden,
        ));
    }
}

fn begin_frame(mut fx: ResMut<FxParticles>, time: Res<Time>, settings: Res<FxSettings>) {
    fx.now = time.elapsed_secs_wrapped();
    fx.now_abs = time.elapsed_secs_f64();
    fx.budget = settings.max_spawn_per_frame.min(RING_CAP / 2);
    fx.frame_start = fx.cursor;
    fx.pending.clear();
    fx.imm.clear();
    fx.dropped = 0;
}

fn finalize(
    mut fx: ResMut<FxParticles>,
    mut up: ResMut<FxUpload>,
    mut stats: ResMut<FxStats>,
    mut batches: Query<(&ParticleBatch, &mut Visibility)>,
) {
    let fx = &mut *fx;
    let n = fx.pending.len();
    stats.spawned = n;
    stats.dropped = fx.dropped;
    // Hand the spawn list to the upload (swap: no copy, both Vecs keep their capacity).
    up.ring.clear();
    std::mem::swap(&mut up.ring, &mut fx.pending);
    up.ring_start = fx.frame_start;
    fx.cursor = ((fx.frame_start as usize + n) % RING_CAP) as u32;

    // Immediate region: this frame's cubes + dead fill over last frame's extra entries.
    let used = fx.imm.len();
    up.immediate.clear();
    up.immediate.extend_from_slice(&fx.imm);
    if fx.imm_prev > used {
        up.immediate.resize(fx.imm_prev, ParticleGpu::default());
    }
    fx.imm_prev = used;
    let imm_batches = used.div_ceil(BATCH).max(1);

    let now = fx.now_abs;
    let mut visible = 0;
    for (b, mut vis) in &mut batches {
        let want = match b.ring {
            Some(i) => fx.batch_death[i] > now,
            None => b.imm < imm_batches,
        };
        visible += want as usize;
        let w = if want { Visibility::Visible } else { Visibility::Hidden };
        if *vis != w {
            *vis = w;
        }
    }
    stats.visible_batches = visible;
}
