//! One-shot effects (shatter, clash, bursts) and attached emitters (element trails, smoke).

use std::collections::HashMap;
use std::sync::Arc;

use bd_core::mechanics::Element;
use bd_shapes::VoxelModel;
use bevy::camera::visibility::VisibilitySystems;
use bevy::math::Affine3A;
use bevy::prelude::*;
use glam::IVec3;

use crate::construct::{ConstructRuntime, ConstructVisual, random_voxel};
use crate::palette::{Palette, element_color};
use crate::particles::{Emit, EmitSystems, FxParticles, PFlags, PKind};
use crate::util::{aff, fin3, v3};
use crate::FxSettings;

// ------------------------------------------------------------------------------------ messages

/// Two constructs (or a construct and a weapon) collide: white flash on both, a ring of shards in
/// their colours around the contact, both constructs flash.
#[derive(Message, Clone, Debug)]
pub struct ClashFx {
    pub point: Vec3,
    /// Contact normal (the shard ring spreads in the plane perpendicular to it).
    pub normal: Vec3,
    /// Linear RGB of each side.
    pub colors: [[f32; 3]; 2],
    /// ~0.2 (tap) … 1 (heavy) … 2+ (kaiju).
    pub strength: f32,
    /// Construct entities to flash (entities with [`ConstructVisual`]).
    pub constructs: [Option<Entity>; 2],
}

/// Shatter a model that is not (or no longer) a [`ConstructVisual`] entity — e.g. a chunk of a
/// power suit, a construct the game already despawned.
#[derive(Message, Clone)]
pub struct ShatterModelFx {
    pub model: Arc<VoxelModel>,
    /// World transform of the model root.
    pub transform: Affine3A,
    /// Optional posed part transforms (bd_shapes::pose output); rest pose if `None`.
    pub pose: Option<Arc<Vec<glam::Affine3A>>>,
    pub corps_color: [f32; 3],
    pub accent: Option<[f32; 3]>,
    pub impulse: Vec3,
    pub point: Option<Vec3>,
    pub floor: Option<f32>,
}

/// Generic one-shot particle burst.
#[derive(Message, Clone, Copy, Debug)]
pub struct BurstFx {
    pub kind: BurstKind,
    pub pos: Vec3,
    /// Main direction (hit normal / velocity); zero = omni.
    pub dir: Vec3,
    /// Multiplier on the kind's default count/size (1 = default).
    pub scale: f32,
    /// Colour override (linear RGB); defaults per kind.
    pub color: Option<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurstKind {
    /// Element-flavoured burst (hit with an element).
    Element(Element),
    /// Grey cube smoke puff.
    Smoke,
    /// Glowing shards (small construct chip / ring-blast impact).
    Shards,
    /// Dust / terrain chips (brown, lit).
    Dust,
    /// White spark flash.
    Flash,
}

// ------------------------------------------------------------------------------------ components

/// Continuous element trail. On a [`ConstructVisual`] entity particles leave random voxels of the
/// posed model; on anything else they leave the entity's position (± `radius`).
#[derive(Component, Clone, Copy, Debug)]
pub struct TrailFx {
    pub element: Element,
    /// Particles per second (× `FxSettings::particle_budget`).
    pub rate: f32,
    pub radius: f32,
    /// Particle size multiplier.
    pub size: f32,
    credit: f32,
}

impl TrailFx {
    pub fn new(element: Element) -> Self {
        TrailFx { element, rate: 40.0, radius: 0.3, size: 1.0, credit: 0.0 }
    }
    pub fn rate(mut self, r: f32) -> Self {
        self.rate = r;
        self
    }
}

/// Cube smoke puffs rising from the entity (smokestacks, exhausts, snuffed fire).
#[derive(Component, Clone, Copy, Debug)]
pub struct SmokeFx {
    /// Puffs per second.
    pub rate: f32,
    /// Linear RGB albedo.
    pub color: [f32; 3],
    /// Puff size (m).
    pub size: f32,
    /// Rise speed (m/s).
    pub rise: f32,
    credit: f32,
}

impl Default for SmokeFx {
    fn default() -> Self {
        SmokeFx { rate: 6.0, color: [0.55, 0.56, 0.6], size: 0.35, rise: 1.4, credit: 0.0 }
    }
}

pub fn build(app: &mut App) {
    app.add_message::<ClashFx>()
        .add_message::<ShatterModelFx>()
        .add_message::<BurstFx>()
        .add_systems(
            PostUpdate,
            (clash_system, shatter_messages, burst_system, trails, smoke, machine_smoke).in_set(EmitSystems).before(VisibilitySystems::CheckVisibility),
        );
}

// ------------------------------------------------------------------------------------ shatter

#[derive(Default)]
pub struct ShatterScratch {
    map: HashMap<(u32, IVec3), (glam::Vec3, u32, u8)>,
}

fn tint_slot(t: bd_core::shape::Tint) -> u8 {
    use bd_core::shape::Tint::*;
    match t {
        Primary => 0,
        Accent => 1,
        Dark => 2,
        Core => 3,
    }
}

/// Emit a model's (posed) voxels as glowing cubes flying apart. Models above
/// `settings.shatter_max_cubes` are clustered 2×2×2 (then 4×4×4 …) first.
pub fn shatter_model(
    fx: &mut FxParticles,
    settings: &FxSettings,
    model: &VoxelModel,
    root: &Affine3A,
    pose: &[glam::Affine3A],
    pal: &Palette,
    impulse: Vec3,
    point: Option<Vec3>,
    floor: Option<f32>,
    life_scale: f32,
    cube_scale: f32,
    keep: f32,
    scratch: &mut ShatterScratch,
) {
    let count = model.voxel_count.max(1);
    let max = settings.shatter_max_cubes.max(64);
    // Hollow shells: clusters of c³ hold ~c² surface voxels.
    let mut shift = 0u32;
    while shift < 4 && count / (1usize << (2 * shift)) > max {
        shift += 1;
    }
    let scale = root.matrix3.x_axis.length().max(1e-3);
    let vs = model.voxel_size * scale;
    let centre = {
        let b = &model.bounds;
        root.transform_point3(v3((b.min + b.max) * 0.5))
    };
    let burst_from = point.unwrap_or(centre);
    let extent = v3(model.bounds.max - model.bounds.min).length() * scale * 0.5 + 0.5;
    let impulse = fin3(impulse);
    let emit_one = |fx: &mut FxParticles, wp: Vec3, size: f32, tint: u8| {
        let r = &mut fx.rng;
        if keep < 1.0 && r.f() > keep {
            return;
        }
        let size = size * cube_scale;
        let away = wp - burst_from;
        let dist = away.length();
        let dir = if dist > 1e-3 { away / dist } else { r.dir() };
        let fall = (1.0 - dist / (extent * 2.0)).clamp(0.25, 1.0);
        let vel = impulse * r.range(0.55, 1.15) + dir * r.range(1.8, 4.2) * fall + r.vec() * 1.2 + Vec3::Y * r.range(0.6, 2.2);
        let color = pal.slot(tint as usize);
        let emissive = if tint == 3 { 3.2 } else if tint == 2 { 1.2 } else { 1.9 };
        let seed = r.next_u32();
        let spin = r.range(3.0, 12.0);
        let life = r.range(1.1, 1.8) * life_scale;
        let mut flags = PFlags::WHITE_BIRTH;
        let mut aux = 0.0;
        if let Some(f) = floor {
            flags |= PFlags::FLOOR;
            aux = f;
        }
        fx.emit(Emit {
            kind: PKind::Ballistic,
            flags,
            pos: wp,
            vel,
            size: size * 0.8,
            color,
            emissive,
            life,
            gravity: 0.6,
            drag: 0.7,
            spin,
            seed,
            aux,
            ..default()
        });
    };
    let part_tf = |pi: usize| -> Affine3A {
        match pose.get(pi) {
            Some(p) => *root * aff(p),
            None => *root,
        }
    };
    if shift == 0 {
        for (pi, part) in model.parts.iter().enumerate() {
            let tf = part_tf(pi);
            for (v, t) in &part.voxels {
                if fx.remaining() == 0 {
                    return;
                }
                let wp = tf.transform_point3(v3(model.voxel_center(*v)));
                emit_one(fx, wp, vs, tint_slot(*t));
            }
        }
        return;
    }
    scratch.map.clear();
    for (pi, part) in model.parts.iter().enumerate() {
        for (v, t) in &part.voxels {
            let k = (pi as u32, *v >> shift as i32);
            let c = model.voxel_center(*v);
            let e = scratch.map.entry(k).or_insert((glam::Vec3::ZERO, 0, tint_slot(*t)));
            e.0 += c;
            e.1 += 1;
        }
    }
    let size = vs * (1u32 << shift) as f32 * 0.8;
    for (&(pi, _), &(sum, n, tint)) in scratch.map.iter() {
        if fx.remaining() == 0 {
            return;
        }
        let tf = part_tf(pi as usize);
        let wp = tf.transform_point3(v3(sum / n as f32));
        // Sparse clusters get smaller cubes (keeps the silhouette honest).
        let fill = ((n as f32) / (1u32 << (2 * shift)) as f32).clamp(0.45, 1.0);
        emit_one(fx, wp, size * fill.sqrt(), tint);
    }
}

fn shatter_messages(mut fx: ResMut<FxParticles>, settings: Res<FxSettings>, mut msgs: MessageReader<ShatterModelFx>, mut scratch: Local<ShatterScratch>) {
    for m in msgs.read() {
        let pal = Palette::new(m.corps_color, m.accent);
        let pose: &[glam::Affine3A] = m.pose.as_ref().map(|p| p.as_slice()).unwrap_or(&[]);
        shatter_model(&mut fx, &settings, &m.model, &m.transform, pose, &pal, m.impulse, m.point, m.floor, 1.0, 1.0, 1.0, &mut scratch);
    }
}

// ------------------------------------------------------------------------------------ clash

fn clash_system(mut fx: ResMut<FxParticles>, mut msgs: MessageReader<ClashFx>, mut rts: Query<&mut ConstructRuntime>) {
    for m in msgs.read() {
        let s = if m.strength.is_finite() { m.strength.clamp(0.05, 4.0) } else { 1.0 };
        for e in m.constructs.iter().flatten() {
            if let Ok(mut rt) = rts.get_mut(*e) {
                rt.auto_flash = 1.0;
            }
        }
        let p = fin3(m.point);
        let n = if m.normal.length_squared() > 1e-6 { m.normal.normalize() } else { Vec3::Y };
        // White flash cube + a smaller hot core.
        fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: p, size: 0.9 * s.sqrt() + 0.3, color: [1.0, 1.0, 1.0], emissive: 9.0, life: 0.13, ..default() });
        fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: p, size: 1.6 * s.sqrt() + 0.4, color: m.colors[0], emissive: 3.0, life: 0.2, ..default() });
        // Ring of shards in the contact plane, alternating colours.
        let b = Mat3::from_quat(Quat::from_rotation_arc(Vec3::Z, n));
        let count = (18.0 + 26.0 * s) as usize;
        for i in 0..count {
            let r = &mut fx.rng;
            let a = i as f32 / count as f32 * std::f32::consts::TAU + r.range(-0.1, 0.1);
            let dir = b * Vec3::new(a.cos(), a.sin(), r.range(-0.25, 0.25));
            let speed = r.range(5.0, 11.0) * (0.6 + 0.4 * s);
            let color = m.colors[i % 2];
            let seed = r.next_u32();
            let size = r.range(0.06, 0.14) * (0.7 + 0.3 * s);
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::STRETCH | PFlags::WHITE_BIRTH,
                pos: p,
                vel: dir * speed,
                size,
                color,
                emissive: 3.0,
                life: 0.35 + 0.25 * s.min(1.5),
                gravity: 0.35,
                drag: 3.0,
                spin: 3.5,
                seed,
                ..default()
            });
        }
        // A few chunky shards that tumble and fall.
        for i in 0..(6.0 + 6.0 * s) as usize {
            let r = &mut fx.rng;
            let dir = (r.dir() + n * 0.8).normalize_or_zero();
            let seed = r.next_u32();
            let spin = r.range(6.0, 14.0);
            let vel = dir * r.range(3.0, 7.0);
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::WHITE_BIRTH,
                pos: p,
                vel,
                size: 0.12 + 0.06 * s,
                color: m.colors[i % 2],
                emissive: 2.2,
                life: 0.9,
                gravity: 1.0,
                drag: 0.8,
                spin,
                seed,
                ..default()
            });
        }
    }
}

// ------------------------------------------------------------------------------------ bursts

/// One element particle leaving `pos` (world). `k` = size multiplier.
pub fn element_particle(fx: &mut FxParticles, e: Element, pos: Vec3, drift: Vec3, k: f32) {
    let r = &mut fx.rng;
    let seed = r.next_u32();
    let base = element_color(e);
    let jitter = |r: &mut crate::util::FxRng, c: [f32; 3], a: f32| {
        let j = r.range(1.0 - a, 1.0 + a);
        [c[0] * j, c[1] * j, c[2] * j]
    };
    let em = match e {
        Element::Fire => Emit {
            kind: PKind::Ballistic,
            flags: PFlags::PLAIN,
            pos,
            vel: drift + r.vec() * 0.6 + Vec3::Y * r.range(0.5, 1.5),
            size: r.range(0.05, 0.1) * k,
            color: if r.f() < 0.35 { [1.0, 0.75, 0.2] } else { jitter(r, base, 0.15) },
            emissive: r.range(3.0, 5.0),
            life: r.range(0.5, 0.95),
            gravity: -0.25,
            drag: 1.4,
            spin: 6.0,
            seed,
            ..default()
        },
        Element::Atomic => Emit {
            kind: PKind::Ballistic,
            flags: PFlags::PLAIN,
            pos,
            vel: drift + r.vec() * 0.5 + Vec3::Y * r.range(0.4, 1.2),
            size: r.range(0.05, 0.1) * k,
            color: jitter(r, base, 0.2),
            emissive: r.range(3.0, 6.0),
            life: r.range(0.5, 0.9),
            gravity: -0.2,
            drag: 1.2,
            spin: 5.0,
            seed,
            ..default()
        },
        Element::Electric => Emit {
            kind: PKind::Ballistic,
            flags: PFlags::STRETCH | PFlags::PLAIN,
            pos,
            vel: drift + r.dir() * r.range(4.0, 9.0),
            size: r.range(0.025, 0.045) * k,
            color: if r.f() < 0.5 { [0.9, 0.95, 1.0] } else { base },
            emissive: 6.0,
            life: r.range(0.08, 0.2),
            gravity: 0.5,
            drag: 4.0,
            spin: 5.0,
            seed,
            ..default()
        },
        Element::Ice => Emit {
            kind: PKind::Ballistic,
            pos,
            vel: drift * 0.5 + r.vec() * 0.4,
            size: r.range(0.05, 0.1) * k,
            color: jitter(r, base, 0.1),
            emissive: 1.4,
            life: r.range(0.8, 1.3),
            gravity: 0.25,
            drag: 1.5,
            spin: r.range(1.0, 4.0),
            seed,
            ..default()
        },
        Element::Water => Emit {
            kind: PKind::Ballistic,
            flags: PFlags::STRETCH,
            pos,
            vel: drift + r.vec() * 1.2 + Vec3::Y * r.range(0.0, 1.0),
            size: r.range(0.04, 0.07) * k,
            color: jitter(r, base, 0.1),
            emissive: 1.5,
            life: r.range(0.45, 0.7),
            gravity: 1.0,
            drag: 0.4,
            spin: 1.8,
            seed,
            ..default()
        },
        Element::Poison => Emit {
            kind: PKind::Rise,
            flags: PFlags::POP,
            pos,
            vel: drift * 0.3 + r.vec() * 0.2,
            size: r.range(0.07, 0.14) * k,
            color: if r.f() < 0.3 { [0.6, 0.2, 0.8] } else { jitter(r, base, 0.1) },
            emissive: 1.6,
            life: r.range(0.7, 1.2),
            gravity: r.range(0.4, 0.8),
            drag: 1.0,
            spin: 0.12,
            seed,
            ..default()
        },
        Element::Shadow => Emit {
            kind: PKind::Rise,
            flags: PFlags::GROW | PFlags::PLAIN,
            pos,
            vel: drift * 0.3 + r.vec() * 0.3,
            size: r.range(0.1, 0.2) * k,
            color: [0.05, 0.0, 0.09],
            emissive: 1.0,
            life: r.range(0.8, 1.3),
            gravity: r.range(0.5, 0.9),
            drag: 1.2,
            spin: 0.35,
            seed,
            ..default()
        },
        Element::Psychic => Emit {
            kind: PKind::Orbit,
            flags: PFlags::STRETCH | PFlags::PLAIN,
            pos: Vec3::ZERO,
            vel: Vec3::new(r.range(0.3, 0.8), r.range(3.0, 6.0), -0.2),
            size: r.range(0.03, 0.05) * k,
            color: jitter(r, base, 0.1),
            emissive: 3.5,
            life: r.range(0.5, 0.9),
            spin: 2.5,
            seed,
            anchor: 0,
            ..default()
        },
        Element::Wind => Emit {
            kind: PKind::Ballistic,
            flags: PFlags::STRETCH | PFlags::PLAIN,
            pos,
            vel: drift + r.dir() * r.range(1.5, 3.0),
            size: r.range(0.02, 0.035) * k,
            color: base,
            emissive: 1.6,
            life: r.range(0.3, 0.5),
            gravity: 0.0,
            drag: 1.0,
            spin: 6.0,
            seed,
            ..default()
        },
        Element::Earth => Emit {
            kind: PKind::Ballistic,
            flags: PFlags::SMOKE,
            pos,
            vel: drift + r.vec() * 1.0 + Vec3::Y,
            size: r.range(0.06, 0.12) * k,
            color: jitter(r, base, 0.15),
            emissive: 1.0,
            life: r.range(0.6, 1.0),
            gravity: 1.0,
            drag: 0.3,
            spin: 5.0,
            seed,
            ..default()
        },
        Element::Light => Emit {
            kind: PKind::Ballistic,
            flags: PFlags::PLAIN,
            pos,
            vel: drift + r.vec() * 0.5,
            size: r.range(0.03, 0.06) * k,
            color: base,
            emissive: 3.0,
            life: r.range(0.3, 0.6),
            gravity: 0.0,
            drag: 2.0,
            spin: 4.0,
            seed,
            ..default()
        },
    };
    let mut em = em;
    if em.kind == PKind::Orbit {
        em.pos = pos; // orbit centre (world, anchor 0)
    }
    fx.emit(em);
}

fn burst_system(mut fx: ResMut<FxParticles>, mut msgs: MessageReader<BurstFx>) {
    for m in msgs.read() {
        let s = if m.scale.is_finite() { m.scale.clamp(0.05, 8.0) } else { 1.0 };
        let p = fin3(m.pos);
        let d = fin3(m.dir);
        match m.kind {
            BurstKind::Element(e) => {
                let n = (18.0 * s) as usize;
                for _ in 0..n {
                    let drift = d * 2.5 + fx.rng.vec() * 1.5;
                    let off = fx.rng.vec();
                    element_particle(&mut fx, e, p + off * 0.15 * s, drift, s.sqrt());
                }
            }
            BurstKind::Smoke => {
                let c = m.color.unwrap_or([0.55, 0.56, 0.6]);
                for _ in 0..(8.0 * s) as usize {
                    let off = fx.rng.vec();
                    smoke_puff(&mut fx, p + off * 0.2 * s, d * 0.8, c, 0.35 * s.sqrt(), 1.2);
                }
            }
            BurstKind::Shards => {
                let c = m.color.unwrap_or([0.3, 1.0, 0.4]);
                for _ in 0..(14.0 * s) as usize {
                    let r = &mut fx.rng;
                    let vel = d * 3.0 + r.dir() * r.range(2.0, 5.0);
                    let seed = r.next_u32();
                    fx.emit(Emit { pos: p, vel, size: 0.07 * s.sqrt(), color: c, emissive: 2.5, life: 0.6, gravity: 0.8, drag: 1.0, spin: 9.0, seed, flags: PFlags::WHITE_BIRTH, ..default() });
                }
            }
            BurstKind::Dust => {
                let c = m.color.unwrap_or([0.45, 0.36, 0.25]);
                for _ in 0..(16.0 * s) as usize {
                    let r = &mut fx.rng;
                    let vel = d * 2.0 + r.dir() * r.range(1.0, 3.0) + Vec3::Y * 1.5;
                    let seed = r.next_u32();
                    let size = r.range(0.05, 0.12) * s.sqrt();
                    fx.emit(Emit { pos: p, vel, size, color: c, emissive: 1.0, life: 0.8, gravity: 1.0, drag: 0.8, spin: 7.0, seed, flags: PFlags::SMOKE, ..default() });
                }
            }
            BurstKind::Flash => {
                let c = m.color.unwrap_or([1.0, 1.0, 1.0]);
                fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: p, size: 0.6 * s, color: c, emissive: 8.0, life: 0.12, ..default() });
            }
        }
    }
}

pub fn smoke_puff(fx: &mut FxParticles, pos: Vec3, drift: Vec3, color: [f32; 3], size: f32, rise: f32) {
    let r = &mut fx.rng;
    let seed = r.next_u32();
    let g = r.range(0.85, 1.1);
    let vel = drift + r.vec() * 0.25;
    let size = size * r.range(0.7, 1.2);
    let life = r.range(1.4, 2.2);
    let gravity = rise * r.range(0.8, 1.2);
    fx.emit(Emit {
        kind: PKind::Rise,
        flags: PFlags::SMOKE | PFlags::GROW,
        pos,
        vel,
        size,
        color: [color[0] * g, color[1] * g, color[2] * g],
        emissive: 1.0,
        life,
        gravity,
        drag: 1.5,
        spin: 0.25,
        seed,
        ..default()
    });
}

// ------------------------------------------------------------------------------------ emitters

fn trails(
    mut fx: ResMut<FxParticles>,
    settings: Res<FxSettings>,
    time: Res<Time>,
    mut q: Query<(&mut TrailFx, &GlobalTransform, Option<&ConstructVisual>, Option<&ConstructRuntime>)>,
) {
    let dt = time.delta_secs();
    let budget = settings.particle_budget.clamp(0.0, 1.0);
    for (mut tr, gt, cv, rt) in &mut q {
        tr.credit += tr.rate.max(0.0) * budget * dt;
        let root = gt.affine();
        let drift = rt.map(|r| -r.vel * 0.15).unwrap_or(Vec3::ZERO);
        let mut guard = 0;
        while tr.credit >= 1.0 && guard < 256 {
            guard += 1;
            tr.credit -= 1.0;
            let pos = match (cv, rt) {
                (Some(cv), Some(rt)) if rt.reveal > 0.9 => {
                    let r = fx.rng.next_u32();
                    let cum_total = cv.model.voxel_count.max(1);
                    let _ = cum_total;
                    match random_voxel(&cv.model, rt_cum(rt), r) {
                        Some((pi, c)) => {
                            let local = rt.pose.get(pi).map(|p| p.transform_point3(c)).unwrap_or(c);
                            root.transform_point3(v3(local))
                        }
                        None => root.translation.into(),
                    }
                }
                (Some(_), Some(_)) => continue,
                _ => Vec3::from(root.translation) + fx.rng.vec() * tr.radius,
            };
            let k = tr.size;
            element_particle(&mut fx, tr.element, pos, drift, k);
        }
    }
}

fn rt_cum(rt: &ConstructRuntime) -> &[usize] {
    rt.cum_slice()
}

fn smoke(mut fx: ResMut<FxParticles>, settings: Res<FxSettings>, time: Res<Time>, mut q: Query<(&mut SmokeFx, &GlobalTransform)>) {
    let dt = time.delta_secs();
    let budget = settings.particle_budget.clamp(0.0, 1.0);
    for (mut s, gt) in &mut q {
        s.credit += s.rate.max(0.0) * budget * dt;
        let p = gt.translation();
        let up = gt.up().as_vec3();
        let mut guard = 0;
        while s.credit >= 1.0 && guard < 64 {
            guard += 1;
            s.credit -= 1.0;
            let (c, size, rise) = (s.color, s.size, s.rise);
            smoke_puff(&mut fx, p, up * 0.6, c, size, rise);
        }
    }
}


/// Machine-kit smokestacks / exhausts (bd_shapes `machine_puffs`): puffs synced to wheel revolutions.
fn machine_smoke(
    mut fx: ResMut<FxParticles>,
    settings: Res<FxSettings>,
    mut q: Query<(&ConstructVisual, &mut ConstructRuntime, &GlobalTransform)>,
    mut scratch: Local<Vec<bd_shapes::Puff>>,
) {
    let budget = settings.particle_budget.clamp(0.0, 1.0);
    for (v, mut rt, gt) in &mut q {
        if v.model.machine.puffs.is_empty() || rt.reveal < 0.95 {
            continue;
        }
        let mut now = bd_shapes::FeatureDrive::default();
        for (s, t) in v.layers.as_slice() {
            now.time = now.time.max(*t);
            if let bd_shapes::AnimState::Locomotion { speed, distance, .. } = s {
                now.speed = *speed;
                now.distance = *distance;
            }
        }
        if let Some(prev) = rt.prev_drive {
            scratch.clear();
            bd_shapes::machine_puffs(&v.model, &rt.pose, &prev, &now, &mut scratch);
            let root = gt.affine();
            let scale = root.matrix3.x_axis.length();
            for p in scratch.iter() {
                if fx.rng.f() > budget {
                    continue;
                }
                let pos = root.transform_point3(v3(p.pos));
                let dir = root.transform_vector3(v3(p.dir)).normalize_or(Vec3::Y);
                let s = p.size * scale * (0.7 + 0.5 * p.strength);
                smoke_puff(&mut fx, pos, dir * (1.2 + 2.0 * p.strength), [0.6, 0.6, 0.64], s.max(0.1), 0.9);
                smoke_puff(&mut fx, pos + dir * s * 0.5, dir * (0.8 + 1.5 * p.strength), [0.72, 0.72, 0.75], (s * 0.7).max(0.08), 1.1);
            }
        }
        rt.prev_drive = Some(now);
    }
}