//! KO-range smoke (Smash smoke): a body at or past [`super::HeatTuning::ko_pct`] trails dark smoke
//! puffs and throws hot sparks in its heat colour, denser the deeper in — a finisher opportunity
//! reads from across the map without any number. Your own body only in third person.
//! Rate-limited per body (a fractional accumulator, no allocation), pooled GPU particles.

use bd_fx::{Emit, FxParticles, PFlags, PKind};
use bevy::prelude::*;

use super::{Readability, heat_rgb, ko_depth, ko_range};
use crate::present::camera3p::ViewMode;
use crate::sim_bridge::WorldViews;

pub fn build(app: &mut App) {
    app.add_systems(Update, ko_smoke.in_set(crate::present::PresentSet::Events));
}

/// Emission debt per player slot (smoke, sparks).
struct Debt([(f32, f32); 256]);

/// A little sideways drift per puff (from its seed: no extra RNG draws).
fn r_x(seed: u32) -> f32 {
    ((seed & 0xff) as f32 / 255.0 - 0.5) * 0.5
}
fn r_z(seed: u32) -> f32 {
    (((seed >> 8) & 0xff) as f32 / 255.0 - 0.5) * 0.5
}

impl Default for Debt {
    fn default() -> Self {
        Debt([(0.0, 0.0); 256])
    }
}

fn ko_smoke(
    time: Res<Time>,
    tuning: Res<Readability>,
    views: Option<Res<WorldViews>>,
    mode: Option<Res<ViewMode>>,
    cams: Query<&GlobalTransform, With<crate::camera::PlayerCamera>>,
    fx: Option<ResMut<FxParticles>>,
    mut debt: Local<Option<Box<Debt>>>,
) {
    let (Some(views), Some(mut fx)) = (views, fx) else { return };
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let debt = debt.get_or_insert_with(Box::default);
    let (h, s) = (&tuning.0.heat, &tuning.0.smoke);
    let cam = cams.iter().next().map(|g| g.translation());
    let third = mode.is_some_and(|m| m.third_person());
    for v in &views.players {
        let slot = &mut debt.0[v.id.0 as usize];
        let far = cam.is_some_and(|c| c.distance(v.pos) > s.max_dist);
        if !v.alive() || !ko_range(h, v.damage_pct) || (v.is_local && !third) || far {
            *slot = (0.0, 0.0);
            continue;
        }
        let mult = 1.0 + (s.max_mult - 1.0) * ko_depth(h, v.damage_pct);
        slot.0 += s.smoke_per_sec * mult * dt;
        slot.1 += s.sparks_per_sec * mult * dt;
        let heat = heat_rgb(h, v.damage_pct);
        let size = v.size.max(0.3);
        let r = &mut fx.rng;
        let mut puffs = [(Vec3::ZERO, 0u32, 0.0f32, 0.0f32); 4];
        let mut n = 0;
        while slot.0 >= 1.0 && n < puffs.len() {
            slot.0 -= 1.0;
            let off = Vec3::new(r.range(-0.3, 0.3), r.range(0.5, 1.7), r.range(-0.3, 0.3)) * size;
            puffs[n] = (v.pos + off, r.next_u32(), r.range(0.22, 0.4) * size, r.range(1.1, 1.7));
            n += 1;
        }
        let mut sparks = [(Vec3::ZERO, Vec3::ZERO, 0u32); 3];
        let mut m = 0;
        while slot.1 >= 1.0 && m < sparks.len() {
            slot.1 -= 1.0;
            let off = Vec3::new(r.range(-0.25, 0.25), r.range(0.6, 1.6), r.range(-0.25, 0.25)) * size;
            let vel = Vec3::new(r.range(-2.5, 2.5), r.range(1.5, 4.0), r.range(-2.5, 2.5));
            sparks[m] = (v.pos + off, vel, r.next_u32());
            m += 1;
        }
        for &(pos, seed, sz, life) in &puffs[..n] {
            // Dark, sun-lit smoke with a faint ember tint: reads on bright AND dark maps (it is a
            // silhouette against sky, and the sparks carry it at night).
            fx.emit(Emit {
                kind: PKind::Rise,
                flags: PFlags::SMOKE | PFlags::GROW,
                pos,
                vel: v.vel * 0.25 + Vec3::new(r_x(seed), 0.9, r_z(seed)),
                size: sz,
                color: [0.06 + heat[0] * 0.05, 0.05, 0.05],
                emissive: 1.0,
                life,
                gravity: 1.3,
                drag: 1.4,
                spin: 0.25,
                seed,
                ..default()
            });
        }
        for &(pos, vel, seed) in &sparks[..m] {
            fx.emit(Emit {
                kind: PKind::Ballistic,
                flags: PFlags::STRETCH | PFlags::PLAIN | PFlags::WHITE_BIRTH,
                pos,
                vel: vel + v.vel * 0.5,
                size: 0.05 * size,
                color: [heat[0].max(0.8), heat[1] * 0.6 + 0.1, heat[2] * 0.5],
                emissive: 5.0,
                life: 0.35,
                gravity: 1.2,
                drag: 0.6,
                spin: 3.0,
                seed,
                ..default()
            });
        }
    }
}
