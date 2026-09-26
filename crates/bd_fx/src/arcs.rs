//! Jagged, animated energy arcs (ring → construct while forming/controlling, electric zaps),
//! drawn as immediate-region voxel segments (a few dozen cubes per arc, rebuilt every frame).

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use crate::construct::{ConstructRuntime, ConstructVisual};
use crate::palette::Palette;
use crate::particles::{EmitSystems, FxParticles, PFlags};
use crate::util::{FxRng, fin3, v3};

/// A standalone arc between two world points (the game updates `from`/`to`). With `lifetime` set
/// bd_fx despawns the entity when it runs out.
#[derive(Component, Clone, Copy, Debug)]
pub struct ArcFx {
    pub from: Vec3,
    pub to: Vec3,
    /// Linear RGB.
    pub color: [f32; 3],
    /// Thickness multiplier / jaggedness (1 = default).
    pub intensity: f32,
    pub lifetime: Option<f32>,
    pub seed: u32,
}

impl ArcFx {
    pub fn new(from: Vec3, to: Vec3, color: [f32; 3]) -> Self {
        ArcFx { from, to, color, intensity: 1.0, lifetime: None, seed: 0 }
    }
}

/// One-shot lightning zap (electric chain, ring blast): spawns a short-lived [`ArcFx`].
#[derive(Message, Clone, Copy, Debug)]
pub struct ZapFx {
    pub from: Vec3,
    pub to: Vec3,
    pub color: [f32; 3],
    pub duration: f32,
}

pub fn build(app: &mut App) {
    app.add_message::<ZapFx>()
        .add_systems(Update, spawn_zaps)
        .add_systems(PostUpdate, draw_arcs.in_set(EmitSystems).before(VisibilitySystems::CheckVisibility));
}

fn spawn_zaps(mut commands: Commands, mut msgs: MessageReader<ZapFx>) {
    for (i, z) in msgs.read().enumerate() {
        commands.spawn(ArcFx {
            from: z.from,
            to: z.to,
            color: z.color,
            intensity: 1.2,
            lifetime: Some(z.duration.clamp(0.03, 5.0)),
            seed: i as u32 * 7919 + 17,
        });
    }
}

/// Draw one jagged arc. `phase` quantises the jitter (≈ 30 Hz flicker).
pub fn draw_arc(fx: &mut FxParticles, a: Vec3, b: Vec3, color: [f32; 3], intensity: f32, seed: u32, phase: u32) {
    let a = fin3(a);
    let b = fin3(b);
    let d = b - a;
    let len = d.length();
    if len < 0.05 {
        return;
    }
    let dir = d / len;
    let up = if dir.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let s1 = dir.cross(up).normalize();
    let s2 = dir.cross(s1);
    let segs = ((len / 0.22) as usize).clamp(6, 48);
    let amp = (len * 0.03 + 0.06).min(0.6) * intensity.clamp(0.3, 3.0);
    let hot = [color[0] * 0.35 + 0.65, color[1] * 0.35 + 0.65, color[2] * 0.35 + 0.65];
    let th = 0.028 * intensity.clamp(0.3, 3.0);
    for strand in 0..2u32 {
        let mut rng = FxRng::new(((seed as u64) << 32) ^ (phase as u64 * 2654435761) ^ (strand as u64 * 97));
        let mut prev = a;
        let amp_s = if strand == 0 { amp } else { amp * 1.4 };
        for i in 1..=segs {
            let t = i as f32 / segs as f32;
            let env = (t * std::f32::consts::PI).sin();
            let p = if i == segs { b } else { a + d * t + (s1 * rng.range(-1.0, 1.0) + s2 * rng.range(-1.0, 1.0)) * amp_s * env };
            if strand == 0 {
                fx.immediate_segment(prev, p, th, hot, 6.0, PFlags::PLAIN);
            } else {
                fx.immediate_segment(prev, p, th * 0.6, color, 3.5, PFlags::PLAIN);
            }
            // Occasional fork.
            if strand == 0 && rng.f() < 0.12 && i < segs {
                let fork_dir = (dir * 0.6 + (s1 * rng.range(-1.0, 1.0) + s2 * rng.range(-1.0, 1.0))).normalize_or_zero();
                let q = p + fork_dir * rng.range(0.15, 0.45) * (0.5 + len * 0.05).min(2.0);
                fx.immediate_segment(p, q, th * 0.5, color, 3.0, PFlags::PLAIN);
            }
            prev = p;
        }
    }
}

fn draw_arcs(
    mut commands: Commands,
    time: Res<Time>,
    mut fx: ResMut<FxParticles>,
    constructs: Query<(&ConstructVisual, &ConstructRuntime, &GlobalTransform)>,
    mut arcs: Query<(Entity, &mut ArcFx)>,
) {
    let phase = (time.elapsed_secs_wrapped() * 30.0) as u32;
    let dt = time.delta_secs();
    for (i, (v, rt, gt)) in constructs.iter().enumerate() {
        let (Some(ring), true) = (v.ring, v.arc) else { continue };
        if rt.dissolve.is_some() {
            continue;
        }
        let b = &v.model.bounds;
        let target = gt.affine().transform_point3(v3((b.min + b.max) * 0.5));
        let pal = Palette::new(v.corps_color, v.accent);
        let k = if v.forming < 1.0 { 1.2 } else { 0.8 };
        draw_arc(&mut fx, ring, target, pal.slot(0), k, v.seed ^ (i as u32 * 31), phase);
    }
    for (e, mut arc) in &mut arcs {
        let a = *arc;
        draw_arc(&mut fx, a.from, a.to, a.color, a.intensity, a.seed ^ (e.to_bits() as u32), phase);
        if let Some(l) = arc.lifetime.as_mut() {
            *l -= dt;
            if *l <= 0.0 {
                commands.entity(e).despawn();
            }
        }
    }
}
