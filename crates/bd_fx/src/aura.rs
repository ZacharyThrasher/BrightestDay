//! Charging aura — the anime power-up (bd_game `present/charge.rs` drives it).
//!
//! Put a [`ChargeAuraFx`] on an entity whose `Transform` sits at the charger's feet and update its
//! fields every frame. Everything is analytic GPU particles + a few immediate cubes (no new
//! pipelines, no per-frame allocation):
//! * **flames** — stretched hard-light streaks rising around the body in a teardrop that narrows
//!   above the head; size / rate / heat scale with `level`;
//! * **sparks** — small white-hot voxels shooting up;
//! * **pebbles** — terrain-coloured voxels lifting off the ground around the feet and hovering;
//! * **pressure ring + cracks** (immediate, standing only) — a glowing ring pressed into the ground
//!   and radial cracks that grow with `level`;
//! * **shockwave** — set [`ChargeAuraFx::pulse`] for one frame: a ring of light dashes races out;
//! * **far pillar** — beyond ~18 m a vertical light pillar keeps the charger readable at any range
//!   (thickness grows with distance so it never drops under a few pixels); flames scale up too.
//!
//! One-shots: [`AuraBurstFx`] (full-charge burst / broken collapse).

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use crate::FxSettings;
use crate::particles::{Emit, EmitSystems, FxParticles, PFlags, PKind};
use crate::util::fin3;

/// Continuous charging aura (see the module docs).
#[derive(Component, Clone, Debug)]
pub struct ChargeAuraFx {
    /// Corps colour, linear RGB.
    pub color: [f32; 3],
    /// 0..1 overall strength (fade in / out). 0 emits nothing.
    pub intensity: f32,
    /// 0..1 power level (hold time): size, rate, heat.
    pub level: f32,
    /// Body height in metres (1.8 × player scale).
    pub height: f32,
    /// Ground height under the feet while standing (pebbles, ring, cracks); `None` = airborne.
    pub ground: Option<f32>,
    /// Pebble colour (the terrain under the feet), linear RGB.
    pub ground_color: [f32; 3],
    /// Distance from the viewing camera (LOD + minimum on-screen size).
    pub view_distance: f32,
    /// Viewing camera position: most flames hug the silhouette as seen from here (an outline
    /// aura that never hides the body).
    pub eye: Vec3,
    /// The local first-person player: keep the flames off the camera (only the outer shell).
    pub first_person: bool,
    /// Comfort: steady ring (no wobble), softer shockwaves.
    pub calm: bool,
    /// Set for one frame to fire a shockwave pulse (bd_fx clears it).
    pub pulse: bool,
    credit: [f32; 3],
    t: f32,
    seed: u32,
}

impl ChargeAuraFx {
    pub fn new(color: [f32; 3], seed: u32) -> Self {
        Self {
            color,
            intensity: 0.0,
            level: 0.0,
            height: 1.8,
            ground: None,
            ground_color: [0.4, 0.34, 0.26],
            view_distance: 10.0,
            eye: Vec3::ZERO,
            first_person: false,
            calm: false,
            pulse: false,
            credit: [0.0; 3],
            t: 0.0,
            seed,
        }
    }
}

/// One-shot: a charge filled up (`broken == false`: radial burst + shockwave, + a flash near the
/// charger when `flash`), or was broken by a hit (`broken`: the aura collapses into falling shards).
#[derive(Message, Clone, Copy, Debug)]
pub struct AuraBurstFx {
    /// Feet position.
    pub pos: Vec3,
    pub color: [f32; 3],
    /// Body height (m).
    pub height: f32,
    /// 0.5 … 2 (charge level / hold time).
    pub scale: f32,
    pub flash: bool,
    pub broken: bool,
    /// Standing: ground height + terrain colour (pebbles blasted outward).
    pub ground: Option<(f32, [f32; 3])>,
}

pub fn build(app: &mut App) {
    app.add_message::<AuraBurstFx>()
        .add_systems(PostUpdate, (emit_auras, aura_bursts).in_set(EmitSystems).before(VisibilitySystems::CheckVisibility));
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale3(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// Small stable hash → 0..1 (crack layout per aura, no per-frame flicker).
fn h01(seed: u32, i: u32) -> f32 {
    let mut z = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    z ^= z >> 15;
    z = z.wrapping_mul(0x2C1B_3C6D);
    z ^= z >> 12;
    (z >> 8) as f32 / 16_777_216.0
}

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

fn emit_auras(
    mut fx: ResMut<FxParticles>,
    settings: Res<FxSettings>,
    time: Res<Time>,
    mut q: Query<(&mut ChargeAuraFx, &GlobalTransform)>,
) {
    let dt = time.delta_secs().min(0.1);
    let budget = settings.particle_budget.clamp(0.0, 1.0);
    let fx = &mut *fx;
    let mut rng = std::mem::take(&mut fx.rng);
    for (mut a, gt) in &mut q {
        let pulse = std::mem::take(&mut a.pulse);
        let k = if a.intensity.is_finite() { a.intensity.clamp(0.0, 1.0) } else { 0.0 };
        if k <= 0.001 {
            a.credit = [0.0; 3];
            continue;
        }
        a.t += dt;
        let lv = if a.level.is_finite() { a.level.clamp(0.0, 1.0) } else { 0.0 };
        let feet = gt.translation();
        let h = if a.height.is_finite() { a.height.clamp(0.3, 20.0) } else { 1.8 };
        let hs = h / 1.8;
        let d = if a.view_distance.is_finite() { a.view_distance.max(0.0) } else { 10.0 };
        // Far away: fewer but bigger particles (minimum on-screen size), plus the pillar.
        let lod = (1.0 - (d - 30.0) / 70.0).clamp(0.3, 1.0);
        let grow = (d / 18.0).max(1.0);
        let col = a.color;
        let hot = mix3(col, WHITE, 0.35);
        let fp = a.first_person;

        // ---- flames: thin stretched streaks on a teardrop shell around the body (never inside it:
        // particles are opaque, the charger's body must stay readable through the gaps).
        let rate = (90.0 + 120.0 * lv) * k * budget * lod * if fp { 0.35 } else { 1.0 };
        a.credit[0] += rate * dt;
        // Silhouette angle (perpendicular to the view, in the ground plane).
        let to_eye = Vec3::new(a.eye.x - feet.x, 0.0, a.eye.z - feet.z);
        let rim = if to_eye.length_squared() > 1e-4 { to_eye.z.atan2(to_eye.x) + std::f32::consts::FRAC_PI_2 } else { 0.0 };
        let mut guard = 0;
        while a.credit[0] >= 1.0 && guard < 128 {
            guard += 1;
            a.credit[0] -= 1.0;
            let r = &mut rng;
            // First person: keep the flames low and wide (peripheral, never over the camera).
            let yn = r.f() * if fp { 0.45 } else { 1.0 };
            let ang = if !fp && r.f() < 0.72 {
                rim + if r.f() < 0.5 { 0.0 } else { std::f32::consts::PI } + r.range(-0.45, 0.45)
            } else {
                r.f() * std::f32::consts::TAU
            };
            let bulge = (std::f32::consts::PI * (yn * 1.1).min(1.0)).sin();
            let taper = 1.0 - 0.6 * ((yn - 0.8) / 0.2).clamp(0.0, 1.0);
            let shell = (0.48 + 0.14 * lv) * hs * (0.72 + 0.35 * bulge) * taper;
            let rr = if fp { r.range(1.1, 1.45) * hs } else { shell * r.range(0.95, 1.12) };
            let (s, c) = ang.sin_cos();
            let radial = Vec3::new(c, 0.0, s);
            let pos = feet + radial * rr + Vec3::Y * (yn * h * 1.02);
            // Up, drawn in near the top (flame tip over the head), with a little swirl.
            let inward = if yn > 0.75 && !fp { -radial * (0.6 + 0.4 * lv) } else { radial * 0.15 };
            let swirl = Vec3::new(-s, 0.0, c) * r.range(-0.3, 0.3);
            let vel = Vec3::Y * (1.6 + 1.3 * lv) * hs * r.range(0.8, 1.2) + inward + swirl;
            let whiten = r.range(0.0, 0.08) + 0.05 * lv;
            let seed = r.next_u32();
            let size = (0.045 + 0.025 * lv) * hs * r.range(0.75, 1.25) * grow;
            let life = r.range(0.22, 0.36) * (0.9 + 0.3 * lv) * if fp { 0.6 } else { 1.0 };
            let white_birth = r.f() < 0.1;
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::STRETCH | if white_birth { PFlags::WHITE_BIRTH } else { 0 },
                pos,
                vel,
                size,
                color: mix3(col, WHITE, whiten.min(0.6)),
                emissive: (2.2 + 1.6 * lv) * (0.6 + 0.4 * k),
                life,
                gravity: -(0.12 + 0.15 * lv),
                drag: 0.6,
                spin: 2.2 + 1.8 * lv,
                seed,
                ..default()
            });
        }

        // ---- sparks: white-hot voxels shooting up.
        a.credit[1] += (14.0 + 36.0 * lv) * k * budget * lod * dt;
        guard = 0;
        while a.credit[1] >= 1.0 && guard < 64 {
            guard += 1;
            a.credit[1] -= 1.0;
            let r = &mut rng;
            let off = Vec3::new(r.range(-1.0, 1.0), 0.0, r.range(-1.0, 1.0)) * (0.5 + 0.3 * lv) * hs;
            let off = if fp { off.normalize_or_zero() * r.range(1.3, 2.0) * hs } else { off };
            let pos = feet + off + Vec3::Y * r.range(0.0, h * if fp { 0.3 } else { 0.8 });
            let vel = Vec3::Y * r.range(4.0, 8.0 + 4.0 * lv) * hs + Vec3::new(r.range(-1.0, 1.0), 0.0, r.range(-1.0, 1.0));
            let seed = r.next_u32();
            let size = r.range(0.03, 0.06) * grow.min(3.0);
            let life = r.range(0.55, 1.0);
            fx.emit(Emit {
                pos,
                vel,
                size,
                color: hot,
                emissive: 7.0,
                life,
                gravity: -0.05,
                drag: 0.9,
                spin: 12.0,
                seed,
                flags: PFlags::WHITE_BIRTH,
                ..default()
            });
        }

        // ---- pebbles: terrain voxels lifting off the ground and hovering (standing only).
        if let Some(gy) = a.ground.filter(|g| g.is_finite() && d < 90.0) {
            a.credit[2] += (7.0 + 12.0 * lv) * k * budget * dt;
            guard = 0;
            while a.credit[2] >= 1.0 && guard < 32 {
                guard += 1;
                a.credit[2] -= 1.0;
                let r = &mut rng;
                let ang = r.f() * std::f32::consts::TAU;
                let rad = r.range(0.55, 1.6 + 0.8 * lv) * hs;
                let pos = Vec3::new(feet.x + ang.cos() * rad, gy + 0.06, feet.z + ang.sin() * rad);
                let seed = r.next_u32();
                let tint = r.range(0.75, 1.1);
                let size = r.range(0.07, 0.13 + 0.06 * lv);
                let life = r.range(1.3, 2.3);
                let vel = Vec3::Y * r.range(1.0, 2.4 + 1.2 * lv);
                fx.emit(Emit {
                    kind: PKind::Rise,
                    flags: PFlags::SMOKE,
                    pos,
                    vel,
                    size,
                    color: scale3(a.ground_color, tint),
                    emissive: 1.0,
                    life,
                    gravity: r.range(0.04, 0.16),
                    drag: 2.6,
                    spin: 0.07,
                    seed,
                    ..default()
                });
            }
            // ---- pressure ring + cracks (immediate; redrawn every frame).
            if d < 70.0 {
                let wob = if a.calm { 0.0 } else { 0.05 * (a.t * 9.0).sin() };
                let rr = ((1.0 + 0.55 * lv) * hs + wob) * (0.7 + 0.3 * k);
                let y = gy + 0.035;
                let n = 28;
                let ring_col = mix3(col, WHITE, 0.25);
                let e = (2.2 + 2.5 * lv) * k;
                let th = (0.05 + 0.03 * lv) * grow.min(2.5);
                let mut prev = Vec3::new(feet.x + rr, y, feet.z);
                for i in 1..=n {
                    let an = i as f32 / n as f32 * std::f32::consts::TAU;
                    let p = Vec3::new(feet.x + an.cos() * rr, y, feet.z + an.sin() * rr);
                    fx.immediate_segment(prev, p, th, ring_col, e, PFlags::PLAIN);
                    prev = p;
                }
                // Radial cracks: fixed layout per aura, length grows with the level.
                let cracks = 7;
                let reach = (0.35 + (0.7 + 1.4 * lv) * k) * hs;
                for i in 0..cracks {
                    let a0 = (i as f32 + h01(a.seed, i) * 0.6) / cracks as f32 * std::f32::consts::TAU;
                    let jag = (h01(a.seed, i + 17) - 0.5) * 0.7;
                    let dir0 = Vec3::new(a0.cos(), 0.0, a0.sin());
                    let dir1 = Vec3::new((a0 + jag).cos(), 0.0, (a0 + jag).sin());
                    let p0 = Vec3::new(feet.x, y, feet.z) + dir0 * 0.3 * hs;
                    let mid = p0 + dir0 * reach * 0.5;
                    let end = mid + dir1 * reach * (0.35 + 0.4 * h01(a.seed, i + 31));
                    let cc = mix3(col, WHITE, 0.1);
                    fx.immediate_segment(p0, mid, 0.045 * grow.min(2.0), cc, 1.6 * k, PFlags::PLAIN);
                    fx.immediate_segment(mid, end, 0.03 * grow.min(2.0), cc, 1.2 * k, PFlags::PLAIN);
                }
            }
        }

        // ---- shockwave pulse.
        if pulse {
            let y = a.ground.filter(|g| g.is_finite()).map_or(feet.y + h * 0.45, |g| g + 0.12);
            let n = if a.calm { 20 } else { 34 };
            let speed = (8.0 + 5.0 * lv) * hs;
            for i in 0..n {
                let an = (i as f32 + rng.f() * 0.5) / n as f32 * std::f32::consts::TAU;
                let dir = Vec3::new(an.cos(), 0.0, an.sin());
                let seed = rng.next_u32();
                fx.emit(Emit {
                    pos: Vec3::new(feet.x, y, feet.z) + dir * 0.4 * hs,
                    vel: dir * speed,
                    size: 0.07 * grow.min(3.0),
                    color: mix3(col, WHITE, 0.3),
                    emissive: if a.calm { 2.5 } else { 4.5 } * (0.5 + 0.5 * k),
                    life: 0.45,
                    gravity: 0.0,
                    drag: 2.4,
                    spin: 3.5,
                    seed,
                    flags: PFlags::STRETCH,
                    ..default()
                });
            }
        }

        // ---- far pillar: readable at any range.
        let far = ((d - 18.0) / 14.0).clamp(0.0, 1.0);
        if far > 0.0 && !fp {
            let flick = if a.calm { 1.0 } else { 0.85 + 0.15 * (a.t * 23.0).sin() };
            let top = feet + Vec3::Y * (h + (3.0 + 4.0 * lv) * hs);
            let th = (d * 0.0032).max(0.1);
            fx.immediate_segment(feet, top, th, col, (2.6 + 2.0 * lv) * far * k * flick, PFlags::PLAIN);
            // A hot core line inside it.
            fx.immediate_segment(feet + Vec3::Y * 0.2, top - Vec3::Y * 0.4, th * 0.4, hot, 4.0 * far * k * flick, PFlags::PLAIN);
        }
    }
    fx.rng = rng;
}

fn aura_bursts(mut fx: ResMut<FxParticles>, mut msgs: MessageReader<AuraBurstFx>) {
    if msgs.is_empty() {
        return;
    }
    let fx = &mut *fx;
    let mut rng = std::mem::take(&mut fx.rng);
    for m in msgs.read() {
        let p = fin3(m.pos);
        let s = if m.scale.is_finite() { m.scale.clamp(0.3, 3.0) } else { 1.0 };
        let h = if m.height.is_finite() { m.height.clamp(0.3, 20.0) } else { 1.8 };
        let col = m.color;
        let chest = p + Vec3::Y * h * 0.55;
        if m.broken {
            // The aura collapses: shards spill outward and fall.
            for _ in 0..(26.0 * s) as usize {
                let r = &mut rng;
                let pos = chest + r.vec() * Vec3::new(0.5, 0.8, 0.5) * (h / 1.8);
                let vel = Vec3::new(r.range(-1.0, 1.0), r.range(0.0, 1.5), r.range(-1.0, 1.0)) * r.range(2.0, 4.5);
                let seed = r.next_u32();
                fx.emit(Emit {
                    pos,
                    vel,
                    size: r.range(0.05, 0.1),
                    color: col,
                    emissive: 3.0,
                    life: r.range(0.4, 0.7),
                    gravity: 1.2,
                    drag: 0.6,
                    spin: 10.0,
                    seed,
                    flags: PFlags::WHITE_BIRTH,
                    ..default()
                });
            }
            continue;
        }
        // Full: radial burst + ground shockwave (+ a flash near the charger).
        for _ in 0..(56.0 * s) as usize {
            let r = &mut rng;
            let mut dir = r.dir();
            dir.y = dir.y.abs() * 0.6 + 0.1;
            let dir = dir.normalize_or_zero();
            let seed = r.next_u32();
            let speed = r.range(9.0, 16.0) * s.sqrt();
            fx.emit(Emit {
                pos: chest,
                vel: dir * speed,
                size: r.range(0.06, 0.11),
                color: mix3(col, WHITE, r.range(0.1, 0.5)),
                emissive: 8.0,
                life: r.range(0.45, 0.75),
                gravity: 0.1,
                drag: 2.0,
                spin: 4.0,
                seed,
                flags: PFlags::STRETCH | PFlags::WHITE_BIRTH,
                ..default()
            });
        }
        let gy = m.ground.map_or(p.y + 0.1, |g| g.0 + 0.12);
        for i in 0..48 {
            let an = i as f32 / 48.0 * std::f32::consts::TAU;
            let dir = Vec3::new(an.cos(), 0.0, an.sin());
            let seed = rng.next_u32();
            fx.emit(Emit {
                pos: Vec3::new(p.x, gy, p.z) + dir * 0.5,
                vel: dir * 16.0 * s.sqrt(),
                size: 0.1,
                color: mix3(col, WHITE, 0.4),
                emissive: 6.0,
                life: 0.55,
                gravity: 0.0,
                drag: 2.2,
                spin: 4.0,
                seed,
                flags: PFlags::STRETCH,
                ..default()
            });
        }
        if let Some((gy, gc)) = m.ground {
            for _ in 0..18 {
                let r = &mut rng;
                let an = r.f() * std::f32::consts::TAU;
                let dir = Vec3::new(an.cos(), 0.0, an.sin());
                let seed = r.next_u32();
                fx.emit(Emit {
                    pos: Vec3::new(p.x, gy + 0.1, p.z) + dir * r.range(0.4, 1.4),
                    vel: dir * r.range(4.0, 8.0) + Vec3::Y * r.range(2.0, 5.0),
                    size: r.range(0.07, 0.15),
                    color: scale3(gc, r.range(0.75, 1.1)),
                    emissive: 1.0,
                    life: r.range(0.8, 1.3),
                    gravity: 1.0,
                    drag: 0.5,
                    spin: 8.0,
                    seed,
                    flags: PFlags::SMOKE | PFlags::BOUNCE,
                    aux: gy,
                    ..default()
                });
            }
        }
        if m.flash {
            fx.emit(Emit {
                kind: PKind::Flash,
                flags: PFlags::PLAIN,
                pos: chest,
                size: 2.6 * s * (h / 1.8),
                color: mix3(col, WHITE, 0.5),
                emissive: 5.0,
                life: 0.18,
                ..default()
            });
        }
    }
    fx.rng = rng;
}
