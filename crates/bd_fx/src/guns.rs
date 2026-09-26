//! Gun VFX (COMBAT.md §4b "DOOM feel"): muzzle burst + 1-frame world light, chunky glowing voxel
//! rounds with short tracers, ejected hard-light casings (bounce, then shatter), railgun beam,
//! impact chips + hard-light impact star, minigun barrel spin/heat hooks.
//! All particles go through the pooled GPU ring; lights come from a fixed pool of 4.

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use crate::construct::ConstructRuntime;
use crate::effects::smoke_puff;
use crate::particles::{Emit, EmitSystems, FxParticles, PFlags, PKind};
use crate::util::fin3;
use crate::{FxSet, FxSettings};

/// Fire: voxel muzzle burst + a 1-frame point light that lights the world.
#[derive(Message, Clone, Copy, Debug)]
pub struct MuzzleFx {
    pub pos: Vec3,
    /// Aim direction (unit).
    pub dir: Vec3,
    /// Shooter corps colour (linear RGB).
    pub color: [f32; 3],
    /// 1 = hand cannon; shotgun ~1.6, minigun ~0.6, rocket ~1.4, railgun ~1.8.
    pub scale: f32,
}

/// Ejected hard-light shell casing: tumbles, tinkle-bounces on `floor`, then shatters.
#[derive(Message, Clone, Copy, Debug)]
pub struct CasingFx {
    pub pos: Vec3,
    /// Initial velocity (usually shooter velocity + sideways/up kick).
    pub vel: Vec3,
    pub color: [f32; 3],
    /// Ground height below (from the game's debris ground query). `None` = falls and fades.
    pub floor: Option<f32>,
    /// Casing length multiplier (shotgun shells ~1.4).
    pub scale: f32,
}

/// Railgun beam: thick core fading to a lingering thin trail (~0.4 s) with a spiral of cubes.
#[derive(Message, Clone, Copy, Debug)]
pub struct BeamFx {
    pub from: Vec3,
    pub to: Vec3,
    pub color: [f32; 3],
    /// Core thickness (m), default ~0.12.
    pub width: f32,
}

/// Bullet/beam impact: terrain-coloured chips + a brief hard-light impact star.
#[derive(Message, Clone, Copy, Debug)]
pub struct ImpactFx {
    pub pos: Vec3,
    pub normal: Vec3,
    /// Colour of the hit surface (terrain voxel colour); `None` = no chips (hit a construct/player).
    pub surface: Option<[f32; 3]>,
    /// Shooter colour for the star.
    pub color: [f32; 3],
    /// 1 = hand cannon slug; pellets ~0.4; rocket 2.5.
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileKind {
    /// Hand cannon slug / minigun round: chunky glowing voxel + tracer.
    Slug,
    /// Shotgun pellet: small voxel + short tracer.
    Pellet,
    /// Rocket: body + fire embers + smoke trail.
    Rocket,
    /// Ring blast: fat fist of light.
    Bolt,
}

/// Put on the game's projectile entity (with `Transform`); bd_fx draws the round + tracer each
/// frame from its motion (velocity from the position delta).
#[derive(Component, Clone, Copy, Debug)]
pub struct ProjectileFx {
    pub kind: ProjectileKind,
    pub color: [f32; 3],
    /// Size multiplier (caliber / payload scale).
    pub size: f32,
    prev: Option<Vec3>,
    credit: f32,
}

impl ProjectileFx {
    pub fn new(kind: ProjectileKind, color: [f32; 3]) -> Self {
        ProjectileFx { kind, color, size: 1.0, prev: None, credit: 0.0 }
    }
}

/// Minigun hook: spins a part about its local −Z (barrel axis) around its pivot.
/// Heat glow: set `ConstructVisual::heat`.
#[derive(Component, Clone, Copy, Debug)]
pub struct BarrelSpin {
    pub part: usize,
    /// Current angle (radians); the game integrates spin speed.
    pub angle: f32,
}

#[derive(Resource)]
struct MuzzleLights {
    lights: Vec<(Entity, f32)>,
    next: usize,
}

pub fn build(app: &mut App) {
    app.add_message::<MuzzleFx>()
        .add_message::<CasingFx>()
        .add_message::<BeamFx>()
        .add_message::<ImpactFx>()
        .add_systems(Startup, spawn_lights)
        .add_systems(PostUpdate, barrel_spin.in_set(FxSet::Pose).after(crate::construct::PoseSystems))
        .add_systems(
            PostUpdate,
            (muzzle, casings, beams, impacts, projectiles, light_decay).in_set(EmitSystems).before(VisibilitySystems::CheckVisibility),
        );
}

fn spawn_lights(mut commands: Commands) {
    let mut lights = Vec::new();
    for i in 0..4 {
        let e = commands
            .spawn((
                Name::new(format!("bd_fx muzzle light {i}")),
                PointLight { intensity: 0.0, range: 12.0, radius: 0.1, shadow_maps_enabled: false, ..default() },
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        lights.push((e, 0.0));
    }
    commands.insert_resource(MuzzleLights { lights, next: 0 });
}

fn light_decay(time: Res<Time>, mut ml: ResMut<MuzzleLights>, mut q: Query<(&mut Visibility, &mut PointLight)>) {
    let dt = time.delta_secs();
    for (e, t) in ml.lights.iter_mut() {
        if *t <= 0.0 {
            continue;
        }
        *t -= dt;
        if *t <= 0.0
            && let Ok((mut v, mut l)) = q.get_mut(*e)
        {
            *v = Visibility::Hidden;
            l.intensity = 0.0;
        }
    }
}

fn muzzle(
    mut fx: ResMut<FxParticles>,
    mut msgs: MessageReader<MuzzleFx>,
    mut ml: ResMut<MuzzleLights>,
    mut lights: Query<(&mut Visibility, &mut PointLight, &mut Transform)>,
) {
    for m in msgs.read() {
        let s = if m.scale.is_finite() { m.scale.clamp(0.1, 5.0) } else { 1.0 };
        let p = fin3(m.pos);
        let d = if m.dir.length_squared() > 1e-6 { m.dir.normalize() } else { Vec3::NEG_Z };
        let hot = [0.6 + m.color[0] * 0.4, 0.6 + m.color[1] * 0.4, 0.6 + m.color[2] * 0.4];
        // Core flash + colour bloom.
        fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: p + d * 0.1 * s, size: 0.35 * s, color: [1.0, 1.0, 1.0], emissive: 10.0, life: 0.06, ..default() });
        fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: p + d * 0.25 * s, size: 0.55 * s, color: m.color, emissive: 4.0, life: 0.09, ..default() });
        // Forward cone of stretched voxel sparks + a few side petals (the "star").
        let (s1, s2) = d.any_orthonormal_pair();
        for i in 0..(10.0 * s) as usize {
            let r = &mut fx.rng;
            let cone = (d + (s1 * r.range(-1.0, 1.0) + s2 * r.range(-1.0, 1.0)) * 0.35).normalize();
            let seed = r.next_u32();
            let speed = r.range(8.0, 18.0) * s.sqrt();
            let life = 0.07 + 0.05 * r.f();
            fx.emit(Emit {
                flags: PFlags::STRETCH | PFlags::PLAIN,
                pos: p,
                vel: cone * speed,
                size: 0.04 * s.sqrt(),
                color: if i % 3 == 0 { m.color } else { hot },
                emissive: 6.0,
                life,
                gravity: 0.0,
                drag: 6.0,
                spin: 5.0,
                seed,
                ..default()
            });
        }
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2 + 0.4;
            let side = s1 * a.cos() + s2 * a.sin();
            fx.emit(Emit {
                kind: PKind::Beam,
                flags: PFlags::PLAIN,
                pos: p + d * 0.12 * s + side * 0.14 * s,
                vel: side * 0.28 * s + d * 0.08,
                size: 0.05 * s,
                color: hot,
                emissive: 6.0,
                life: 0.05,
                ..default()
            });
        }
        smoke_puff(&mut fx, p + d * 0.3, d * 1.2, [0.6, 0.6, 0.62], 0.12 * s, 0.6);
        // 1-frame world light.
        let n = ml.lights.len();
        let i = ml.next % n.max(1);
        ml.next = ml.next.wrapping_add(1);
        if let Some((e, t)) = ml.lights.get_mut(i)
            && let Ok((mut v, mut l, mut tf)) = lights.get_mut(*e)
        {
            *t = 0.035;
            *v = Visibility::Visible;
            l.intensity = 180_000.0 * s;
            l.color = Color::linear_rgb(0.5 + m.color[0] * 0.5, 0.5 + m.color[1] * 0.5, 0.5 + m.color[2] * 0.5);
            tf.translation = p + d * 0.3;
        }
    }
}

/// CPU mirror of the shader's bounce motion: rest position and time to rest.
fn casing_rest(p: Vec3, v: Vec3, grav: f32, floor: f32, size: f32) -> (Vec3, f32) {
    let g = (9.81 * grav).max(0.1);
    let fh = floor + size * 0.5;
    let (mut y0, mut vy, mut h, mut b) = (p.y, v.y, Vec2::new(v.x, v.z), Vec2::new(p.x, p.z));
    let mut total = 0.0;
    for _ in 0..4 {
        let disc = vy * vy + 2.0 * g * (y0 - fh).max(0.0);
        let th = (vy + disc.sqrt()) / g;
        b += h * th;
        total += th;
        y0 = fh;
        vy = (g * th - vy) * 0.42;
        h *= 0.55;
    }
    (Vec3::new(b.x, fh, b.y), total)
}

fn casings(mut fx: ResMut<FxParticles>, mut msgs: MessageReader<CasingFx>) {
    for m in msgs.read() {
        let s = if m.scale.is_finite() { m.scale.clamp(0.3, 4.0) } else { 1.0 };
        let p = fin3(m.pos);
        let v = fin3(m.vel);
        let size = 0.05 * s;
        let seed = fx.rng.next_u32();
        let spin = fx.rng.range(10.0, 22.0);
        match m.floor {
            Some(floor) if floor.is_finite() && floor < p.y + 0.01 => {
                let (rest, t_rest) = casing_rest(p, v, 1.0, floor, size);
                let life = (t_rest + 0.25).clamp(0.3, 2.5);
                fx.emit(Emit {
                    flags: PFlags::BOUNCE | PFlags::WHITE_BIRTH,
                    pos: p,
                    vel: v,
                    size,
                    color: m.color,
                    emissive: 2.4,
                    life,
                    gravity: 1.0,
                    spin,
                    seed,
                    aux: floor,
                    ..default()
                });
                // ...then it shatters where it came to rest.
                for _ in 0..6 {
                    let r = &mut fx.rng;
                    let seed = r.next_u32();
                    let vel = r.dir() * r.range(0.6, 1.6) + Vec3::Y * 1.2;
                    fx.emit(Emit {
                        flags: PFlags::WHITE_BIRTH,
                        pos: rest,
                        vel,
                        size: size * 0.45,
                        color: m.color,
                        emissive: 3.0,
                        life: 0.35,
                        gravity: 0.8,
                        drag: 1.0,
                        spin: 12.0,
                        seed,
                        delay: life - 0.02,
                        ..default()
                    });
                }
            }
            _ => {
                fx.emit(Emit { pos: p, vel: v, size, color: m.color, emissive: 2.4, life: 0.9, gravity: 1.0, spin, seed, flags: PFlags::WHITE_BIRTH, ..default() });
            }
        }
    }
}

fn beams(mut fx: ResMut<FxParticles>, mut msgs: MessageReader<BeamFx>) {
    for m in msgs.read() {
        let a = fin3(m.from);
        let b = fin3(m.to);
        let d = b - a;
        let len = d.length();
        if len < 0.05 {
            continue;
        }
        let w = if m.width.is_finite() && m.width > 0.0 { m.width.min(1.0) } else { 0.12 };
        let hot = [0.7 + m.color[0] * 0.3, 0.7 + m.color[1] * 0.3, 0.7 + m.color[2] * 0.3];
        // Core (split into ≤ 8 m segments so fog/culling stay sane).
        let segs = (len / 8.0).ceil().max(1.0) as usize;
        for i in 0..segs {
            let t0 = i as f32 / segs as f32;
            let t1 = (i + 1) as f32 / segs as f32;
            let c = a + d * ((t0 + t1) * 0.5);
            fx.emit(Emit { kind: PKind::Beam, flags: PFlags::PLAIN, pos: c, vel: d / segs as f32, size: w, color: hot, emissive: 7.0, life: 0.45, ..default() });
            fx.emit(Emit { kind: PKind::Beam, flags: PFlags::PLAIN, pos: c, vel: d / segs as f32, size: w * 2.2, color: m.color, emissive: 1.4, life: 0.18, ..default() });
        }
        // Quake-style spiral of cubes that drift outward and fade.
        let dir = d / len;
        let (s1, s2) = dir.any_orthonormal_pair();
        let n = ((len / 0.18) as usize).min(260);
        for i in 0..n {
            let t = i as f32 / n as f32;
            let ang = t * len * 2.2;
            let off = s1 * ang.cos() + s2 * ang.sin();
            let r = &mut fx.rng;
            let seed = r.next_u32();
            let vel = off * r.range(0.3, 0.8);
            let life = 0.55 + 0.25 * r.f();
            fx.emit(Emit {
                pos: a + d * t + off * w * 1.3,
                vel,
                size: w * 0.45,
                color: m.color,
                emissive: 2.8,
                life,
                gravity: 0.0,
                drag: 2.0,
                spin: 4.0,
                seed,
                ..default()
            });
        }
    }
}

fn impacts(mut fx: ResMut<FxParticles>, mut msgs: MessageReader<ImpactFx>) {
    for m in msgs.read() {
        let s = if m.scale.is_finite() { m.scale.clamp(0.1, 6.0) } else { 1.0 };
        let p = fin3(m.pos);
        let n = if m.normal.length_squared() > 1e-6 { m.normal.normalize() } else { Vec3::Y };
        let (s1, s2) = n.any_orthonormal_pair();
        // Hard-light impact star: short radial streaks + flash.
        fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: p + n * 0.05, size: 0.3 * s.sqrt(), color: [1.0, 1.0, 1.0], emissive: 8.0, life: 0.07, ..default() });
        let rays = 6 + (s * 2.0) as usize;
        for k in 0..rays {
            let a = k as f32 / rays as f32 * std::f32::consts::TAU + fx.rng.range(-0.2, 0.2);
            let dir = (s1 * a.cos() + s2 * a.sin()) * 0.9 + n * 0.35;
            let l = 0.35 * s.sqrt() * fx.rng.range(0.7, 1.3);
            fx.emit(Emit {
                kind: PKind::Beam,
                flags: PFlags::PLAIN,
                pos: p + dir * l * 0.5,
                vel: dir * l,
                size: 0.035 * s.sqrt(),
                color: m.color,
                emissive: 5.0,
                life: 0.09,
                ..default()
            });
        }
        // Terrain chips.
        if let Some(c) = m.surface {
            let count = (10.0 * s) as usize + 3;
            for _ in 0..count {
                let r = &mut fx.rng;
                let dir = (n * r.range(0.6, 1.4) + r.vec() * 0.8).normalize_or_zero();
                let seed = r.next_u32();
                let j = r.range(0.8, 1.15);
                let vel = dir * r.range(2.0, 6.0) * s.sqrt();
                let size = r.range(0.04, 0.1) * s.sqrt();
                let life = r.range(0.5, 0.9);
                fx.emit(Emit {
                    flags: PFlags::SMOKE,
                    pos: p + n * 0.05,
                    vel,
                    size,
                    color: [c[0] * j, c[1] * j, c[2] * j],
                    emissive: 1.0,
                    life,
                    gravity: 1.2,
                    drag: 0.6,
                    spin: 10.0,
                    seed,
                    ..default()
                });
            }
            smoke_puff(&mut fx, p + n * 0.15, n * 0.6, [c[0] * 0.5 + 0.3, c[1] * 0.5 + 0.3, c[2] * 0.5 + 0.3], 0.18 * s.sqrt(), 0.5);
        }
    }
}

fn projectiles(mut fx: ResMut<FxParticles>, settings: Res<FxSettings>, time: Res<Time>, mut q: Query<(&mut ProjectileFx, &GlobalTransform)>) {
    let dt = time.delta_secs().max(1e-4);
    let budget = settings.particle_budget.clamp(0.0, 1.0);
    for (mut pr, gt) in &mut q {
        let pos = gt.translation();
        let prev = pr.prev.unwrap_or(pos);
        pr.prev = Some(pos);
        let delta = pos - prev;
        let speed = delta.length() / dt;
        let dir = if delta.length_squared() > 1e-8 { delta.normalize() } else { gt.forward().as_vec3() };
        let k = pr.size.clamp(0.1, 10.0);
        let rot = Quat::from_rotation_arc(Vec3::Z, dir);
        let hot = [0.55 + pr.color[0] * 0.45, 0.55 + pr.color[1] * 0.45, 0.55 + pr.color[2] * 0.45];
        let (body, tracer_len, tracer_w) = match pr.kind {
            ProjectileKind::Slug => (Vec3::new(0.12, 0.12, 0.3) * k, 2.5, 0.05),
            ProjectileKind::Pellet => (Vec3::new(0.07, 0.07, 0.14) * k, 1.2, 0.03),
            ProjectileKind::Rocket => (Vec3::new(0.2, 0.2, 0.5) * k, 0.0, 0.0),
            ProjectileKind::Bolt => (Vec3::splat(0.3) * k, 3.0, 0.12),
        };
        fx.immediate_box(pos, rot, body, hot, 5.0, 0);
        if pr.kind == ProjectileKind::Bolt {
            fx.immediate_box(pos, rot * Quat::from_rotation_z(0.785), body * 1.35, pr.color, 2.5, PFlags::PLAIN);
        }
        if tracer_len > 0.0 && speed > 1.0 {
            let tl = (speed * 0.025).min(tracer_len * k);
            fx.immediate_segment(pos - dir * (tl + body.z * 0.5), pos - dir * body.z * 0.5, tracer_w * k, pr.color, 3.5, PFlags::PLAIN);
        }
        if pr.kind == ProjectileKind::Rocket {
            pr.credit += 90.0 * budget * dt;
            let mut guard = 0;
            while pr.credit >= 1.0 && guard < 16 {
                guard += 1;
                pr.credit -= 1.0;
                let back = pos - dir * body.z * 0.6;
                let r = &mut fx.rng;
                let seed = r.next_u32();
                let vel = -dir * r.range(2.0, 5.0) + r.vec() * 1.0;
                fx.emit(Emit { flags: PFlags::PLAIN, pos: back, vel, size: 0.07 * k, color: [1.0, 0.55, 0.12], emissive: 5.0, life: 0.25, gravity: -0.2, drag: 3.0, spin: 8.0, seed, ..default() });
                if guard % 3 == 0 {
                    smoke_puff(&mut fx, back, -dir * 0.5, [0.5, 0.5, 0.52], 0.16 * k, 0.4);
                }
            }
        }
    }
}

fn barrel_spin(q: Query<(&BarrelSpin, &ConstructRuntime)>, mut tfs: Query<&mut Transform>) {
    for (spin, rt) in &q {
        if let Some(e) = rt.part_entity(spin.part)
            && let Ok(mut tf) = tfs.get_mut(e)
        {
            // Spin about the barrel axis (model −Z) through the part's own centre.
            tf.rotation *= Quat::from_rotation_z(spin.angle);
        }
    }
}
