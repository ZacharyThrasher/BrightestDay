//! THE gather (shared by every construct's GATHER beat and every hold-to-grow energy object): motes
//! stream INTO a point from a shell whose radius grows with size / tier — a few metres for a sword,
//! across the map for a Signature — sampled from the SURROUNDINGS in the element's style where it
//! can (leaf motes from vegetation, droplets from water, embers from lava, grit from stone, light
//! motes from the open sky), else plain element motes. Far motes are bigger and brighter streaks so
//! everyone can read a huge one coming. Bounded: samples are refreshed by a global ray budget, motes
//! by the caller's per-frame budget.
//!
//! Continuous API: write [`GatherFx`] every frame while something is being charged (a rasengan in the
//! palms, a spirit bomb overhead): the stream swells with `size` × `charge` (`presence.ron → hold`).

use super::mote;
use bd_core::PlayerId;
use bd_core::mechanics::Element;
use bd_fx::{Emit, FxParticles, PFlags, PKind};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A continuous gather request for THIS frame (write it every frame while charging; stop writing
/// and the stream stops). One stream per `(owner, key)`.
#[derive(Message, Clone, Copy, Debug)]
pub struct GatherFx {
    /// Where the energy collects (world).
    pub at: Vec3,
    pub element: Element,
    pub owner: Option<PlayerId>,
    /// Distinguishes several streams of one owner (e.g. the construct id bits); 0 is fine.
    pub key: u32,
    /// Current size of the held object (m, diameter).
    pub size: f32,
    /// 0..1 charge (the stream swells toward 1).
    pub charge: f32,
    /// Mote colour override (linear rgb); `None` = the element's colour.
    pub color: Option<[f32; 3]>,
}

/// `presence.ron → hold`: how a continuous gather scales with the held object's size / charge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoldGather {
    /// Shell radius = clamp(radius_per_m · size, radius_min, radius_max) · (0.5 + 0.5 · charge).
    pub radius_per_m: f32,
    pub radius_min: f32,
    pub radius_max: f32,
    /// Motes / s = min(rate_base + rate_per_m · size, rate_max) · (0.3 + 0.7 · charge).
    pub rate_base: f32,
    pub rate_per_m: f32,
    pub rate_max: f32,
    /// Streams tracked at once (the oldest idle one is recycled).
    pub max_streams: usize,
}

impl Default for HoldGather {
    fn default() -> Self {
        HoldGather { radius_per_m: 6.0, radius_min: 2.0, radius_max: 60.0, rate_base: 40.0, rate_per_m: 45.0, rate_max: 360.0, max_streams: 8 }
    }
}

/// What a surface sample looks like when it streams in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Style {
    Leaf,
    Droplet,
    Ember,
    Grit,
    Sparkle,
    Sky,
}

/// Cached surroundings of one gather point.
#[derive(Default)]
pub struct Surround {
    /// (world point, colour, style).
    pub samples: Vec<(Vec3, [f32; 3], Style)>,
    next: usize,
    /// Radius the cache was sampled for.
    radius: f32,
}

/// Classify a voxel material for the gather.
pub fn style_of(m: bd_core::voxel::Material) -> Option<([f32; 3], Style)> {
    use bd_voxel::material::{DebrisClass, linear_color, material_def};
    if m == bd_core::voxel::AIR {
        return None;
    }
    let d = material_def(m);
    let name = d.name.to_ascii_lowercase();
    let c = linear_color(m);
    let lift = |k: f32| [c[0] * k + 0.05, c[1] * k + 0.05, c[2] * k + 0.05];
    if name.contains("lava") || name.contains("fire") || name.contains("ember") {
        return Some(([1.0, 0.35, 0.06], Style::Ember));
    }
    if name.contains("water") || name.contains("waterfall") {
        return Some(([0.25, 0.6, 1.0], Style::Droplet));
    }
    if d.emissive > 0.2 {
        return Some((lift(1.6), Style::Sparkle));
    }
    Some(match d.debris {
        DebrisClass::Leaves | DebrisClass::Grass => (lift(1.4), Style::Leaf),
        DebrisClass::Crystal => (lift(1.6), Style::Sparkle),
        _ => (lift(1.2), Style::Grit),
    })
}

/// Refresh up to `rays` samples of `s` around `at` within `radius` (downward rays from random points
/// on the upper shell: the surfaces around the gather point; a miss = open sky).
pub fn refresh(s: &mut Surround, fx: &mut FxParticles, at: Vec3, radius: f32, rays: &mut u32, vox: Option<&bd_voxel::VoxelWorld>, max: usize) {
    if (s.radius - radius).abs() > radius * 0.5 {
        s.samples.clear();
        s.next = 0;
    }
    s.radius = radius;
    let Some(v) = vox else { return };
    let want = if s.samples.len() < max { 3 } else { 1 };
    for _ in 0..want {
        if *rays == 0 {
            return;
        }
        *rays -= 1;
        let a = fx.rng.f() * std::f32::consts::TAU;
        let r = radius * fx.rng.range(0.35, 1.0);
        let top = at + Vec3::new(a.cos() * r, radius * 0.35 + 2.0, a.sin() * r);
        let hit = bd_core::voxel::raycast(v, super::super::gv(top), bd_core::Vec3::NEG_Y, radius * 1.2 + 6.0);
        let sample = match hit {
            Some(h) => match style_of(bd_core::voxel::VoxelQuery::material(v, h.voxel)) {
                Some((c, st)) => (super::super::bv(h.point) + Vec3::Y * 0.2, c, st),
                None => continue,
            },
            None => (top + Vec3::Y * radius * 0.2, [0.85, 0.92, 1.0], Style::Sky),
        };
        if s.samples.len() < max {
            s.samples.push(sample);
        } else {
            let i = s.next % max;
            s.samples[i] = sample;
            s.next = s.next.wrapping_add(1);
        }
    }
}

/// Emit `n` gather motes into `target` from a shell of `radius` (surroundings where sampled, else
/// element motes). `life` = travel time for the farthest ones (scaled down for near ones).
/// `face`: the caster's view (first person): keep motes in front of the camera.
#[allow(clippy::too_many_arguments)]
pub fn emit(
    fx: &mut FxParticles,
    s: Option<&Surround>,
    target: Vec3,
    radius: f32,
    n: u32,
    element: Element,
    color: [f32; 3],
    size: f32,
    life: f32,
    face: Option<Vec3>,
) {
    let far = (radius / 15.0).clamp(0.0, 3.0);
    for i in 0..n {
        // Every other mote from the surroundings (when sampled): the world gives its energy.
        let sampled = s.filter(|s| !s.samples.is_empty() && face.is_none() && i % 2 == 0).map(|s| s.samples[fx.rng.below(s.samples.len())]);
        match sampled {
            Some((p, c, st)) => {
                let p = p + fx.rng.vec() * 0.6;
                let d = p.distance(target);
                let l = (life * (0.4 + 0.6 * d / radius.max(0.1))).clamp(0.08, 2.0) * fx.rng.range(0.85, 1.15);
                let (flags, emissive, sz, col) = match st {
                    Style::Leaf => (PFlags::PLAIN, 2.2, size * 1.2, c),
                    Style::Droplet => (PFlags::STRETCH, 3.0, size, c),
                    Style::Ember => (PFlags::STRETCH | PFlags::WHITE_BIRTH, 5.0, size, c),
                    Style::Grit => (PFlags::PLAIN, 1.6, size * 0.9, c),
                    Style::Sparkle => (PFlags::PLAIN | PFlags::WHITE_BIRTH, 5.0, size * 0.8, c),
                    Style::Sky => (PFlags::STRETCH, 4.0, size, [0.5 * c[0] + 0.5 * color[0], 0.5 * c[1] + 0.5 * color[1], 0.5 * c[2] + 0.5 * color[2]]),
                };
                // Lifting off the ground first, then pulled in: an arc that still lands on the
                // target at t = l (up = ½·g·l ⇒ gravity scale = 2·up / (9.81·l)).
                let up = (d * 0.15 / l).min(8.0);
                fx.emit(Emit {
                    kind: PKind::Ballistic,
                    flags,
                    pos: p,
                    vel: (target - p) / l + Vec3::Y * up,
                    size: sz * (1.0 + far * 0.6),
                    color: col,
                    emissive: emissive * (1.0 + far * 0.4),
                    life: l,
                    gravity: 2.0 * up / (9.81 * l),
                    spin: 3.0,
                    ..default()
                });
            }
            None => {
                let mut dir = fx.rng.dir();
                if let Some(f) = face {
                    dir = (dir + f * 1.3).normalize_or(f);
                }
                let r = fx.rng.range(radius * 0.5, radius);
                let l = (life * (0.4 + 0.6 * r / radius.max(0.1))).clamp(0.08, 2.0) * fx.rng.range(0.85, 1.15);
                if matches!(element, Element::Psychic | Element::Wind) && face.is_none() {
                    // A spiral: an orbit whose radius collapses into the centre.
                    let seed = fx.rng.next_u32();
                    let w = if element == Element::Wind { 14.0 } else { 7.0 };
                    fx.emit(Emit {
                        kind: PKind::Orbit,
                        flags: PFlags::STRETCH,
                        pos: target,
                        vel: Vec3::new(r, w / (1.0 + far), -r / l),
                        size: size * (1.0 + far * 0.6),
                        color,
                        emissive: 4.0,
                        life: l,
                        gravity: 0.0,
                        seed,
                        ..default()
                    });
                } else {
                    let p = target + dir * r;
                    mote(fx, element, p, (target - p) / l, color, size * (1.0 + far * 0.6), l);
                }
            }
        }
    }
}
