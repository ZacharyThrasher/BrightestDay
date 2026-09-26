//! Render-side terrain mirror: every replicated terrain edit is applied to `ArenaWorld` (the
//! rendered voxels) exactly as the sim applied it to its own copy — `Carve` via
//! `bd_sim::world::apply_carve_op` (same collapse limits ⇒ same falling pieces), `Extracted` via
//! `bd_sim::extract::apply_extract`, `Power(Restored)` via `bd_sim::powers::apply_restore` — and
//! `ArenaReset { seed }` regenerates the map on a worker, holding every later terrain event until
//! the swap so the mirror rebuilds identically (NET's late-join resync relies on this).
//!
//! Debris: removed voxels become physical cubes that **inherit the velocity of what caused the
//! carve** (the nearest recent hit / explosion / wall splat / punch-through, else a radial pop);
//! unsupported pieces fall as clumps. Budgeted per frame; nearest-first sampling.

use super::sound::Sfx3d;
use super::{bv, PresentSet};
use crate::arena::{generate_map, ArenaWorld, MapKind};
use crate::camera::PlayerCamera;
use crate::debris::{DebrisImpulse, SpawnDebris, VfxRng};
use crate::sim_bridge::GameEvents;
use crate::util::{hash3, v3};
use bd_core::voxel::voxel_center;
use bd_core::VOXEL_SIZE;
use bd_sim::GameEvent;
use bd_voxel::{material_def, CarveShape, VoxelWorld};
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use std::collections::VecDeque;

/// Something that recently pushed on the world (debris inherits its velocity).
#[derive(Clone, Copy, Debug)]
pub struct Push {
    pub pos: Vec3,
    /// Velocity to inherit (m/s); zero = radial only.
    pub vel: Vec3,
    pub radius: f32,
    pub age: f32,
}

/// Recent pushes (hits, explosions, splats) other modules record for debris inheritance.
#[derive(Resource, Default)]
pub struct RecentPushes(pub VecDeque<Push>);

impl RecentPushes {
    pub fn add(&mut self, pos: Vec3, vel: Vec3, radius: f32) {
        if self.0.len() >= 32 {
            self.0.pop_front();
        }
        self.0.push_back(Push { pos, vel, radius, age: 0.0 });
    }
    fn nearest(&self, p: Vec3, r: f32) -> Option<Push> {
        self.0
            .iter()
            .filter(|q| q.age < 0.5 && q.pos.distance(p) < q.radius + r + 2.0)
            .min_by(|a, b| a.pos.distance_squared(p).total_cmp(&b.pos.distance_squared(p)))
            .copied()
    }
}

/// Terrain regeneration in flight (arena reset) and the events waiting for it.
#[derive(Resource, Default)]
struct PendingReset {
    task: Option<Task<VoxelWorld>>,
    held: Vec<GameEvent>,
}

/// Debris cubes spawned by terrain edits this frame (budget).
const FRAME_DEBRIS_BUDGET: usize = 3500;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RecentPushes>()
            .init_resource::<PendingReset>()
            .add_systems(Update, (age_pushes, apply_terrain_events).chain().in_set(PresentSet::Events));
    }
}

fn age_pushes(time: Res<Time<Real>>, mut pushes: ResMut<RecentPushes>) {
    let dt = time.delta_secs();
    for p in pushes.0.iter_mut() {
        p.age += dt;
    }
    while pushes.0.front().is_some_and(|p| p.age > 1.0) {
        pushes.0.pop_front();
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_terrain_events(
    mut events: MessageReader<GameEvents>,
    arena: Option<Res<ArenaWorld>>,
    mut pending: ResMut<PendingReset>,
    pushes: Res<RecentPushes>,
    mut rng: ResMut<VfxRng>,
    mut debris: MessageWriter<SpawnDebris>,
    mut impulses: MessageWriter<DebrisImpulse>,
    mut sfx: MessageWriter<Sfx3d>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut queue: ResMut<crate::render::voxel_render::RemeshQueue>,
    sinks: Option<Res<super::signature_fx::EraseSinks>>,
) {
    let Some(arena) = arena else {
        events.clear();
        return;
    };
    // A reset finished: swap the world, then replay what was held back.
    if let Some(task) = pending.task.as_mut()
        && let Some(world) = check_ready(task)
    {
        pending.task = None;
        swap_world(&arena, world, &mut queue);
        let held = std::mem::take(&mut pending.held);
        for e in &held {
            apply_quiet(&arena, e);
        }
    }
    let eye = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    let mut budget = FRAME_DEBRIS_BUDGET;
    let mut sounds = 0;
    for ev in events.read() {
        let e = &ev.event;
        let terrain = matches!(
            e,
            GameEvent::Carve { .. } | GameEvent::Extracted { .. } | GameEvent::ArenaReset { .. } | GameEvent::Power(bd_sim::powers::PowerEvent::Restored { .. })
        );
        if !terrain {
            continue;
        }
        if let GameEvent::ArenaReset { seed } = e {
            let kind = arena.info.kind;
            if kind == MapKind::Tutorial {
                continue;
            }
            let seed = *seed;
            pending.held.clear();
            pending.task = Some(AsyncComputeTaskPool::get().spawn(async move { generate_map(kind, seed).0 }));
            continue;
        }
        if pending.task.is_some() {
            pending.held.push(e.clone());
            continue;
        }
        match e {
            GameEvent::Carve { op } => {
                let outcome = {
                    let mut w = arena.write();
                    bd_sim::world::apply_carve_op(&mut w, op)
                };
                let (center, radius) = match op.shape {
                    CarveShape::Sphere { center, radius } => (bv(center), radius),
                    CarveShape::Box { min, max } => ((bv(min) + bv(max)) * 0.5, (bv(max) - bv(min)).length() * 0.5),
                    CarveShape::Capsule { a, b, radius } => ((bv(a) + bv(b)) * 0.5, radius + (bv(b) - bv(a)).length() * 0.5),
                };
                if outcome.removed.is_empty() && outcome.falling.is_empty() {
                    continue;
                }
                let d = eye.distance(center);
                // Far away carves get fewer cubes.
                let dist_k = (1.0 - (d - 30.0) / 150.0).clamp(0.15, 1.0);
                let push = pushes.nearest(center, radius);
                // SIGNATURE: an erasing orb's tunnel — the removed voxels are sucked INTO the orb.
                let sink = sinks.as_deref().and_then(|s| s.sink(center));
                let r = &mut rng.0;
                // Removed voxels → debris.
                let n = outcome.removed.len();
                let want = ((n as f32 * dist_k) as usize).min(budget);
                let subdivide = if n * 8 <= want.min(600) { 2u32 } else { 1 };
                let keep = (want as f32 / (n * (subdivide * subdivide * subdivide) as usize).max(1) as f32).min(1.0);
                let mut counts = [0usize; 7];
                for rv in &outcome.removed {
                    counts[material_def(rv.material).debris as usize] += 1;
                    if keep < 1.0 && r.f32() > keep {
                        continue;
                    }
                    let def = material_def(rv.material);
                    let c = bd_voxel::material::linear_color(rv.material);
                    let j = 1.0 + (hash3(rv.pos.x, rv.pos.y, rv.pos.z) - 0.5) * 2.0 * def.jitter;
                    let color = [c[0] * j, c[1] * j, c[2] * j];
                    let p = v3(voxel_center(rv.pos));
                    let away = (p - center).normalize_or(Vec3::Y);
                    let base = match push {
                        Some(q) if q.vel.length_squared() > 1.0 => q.vel * r.range(0.45, 0.9) + away * r.range(1.0, 4.0),
                        _ => away * r.range(4.0, 10.0),
                    };
                    let sub = subdivide as f32;
                    let size = VOXEL_SIZE / sub;
                    for k in 0..(subdivide * subdivide * subdivide) {
                        if budget == 0 {
                            break;
                        }
                        budget -= 1;
                        let o = if subdivide == 1 {
                            Vec3::ZERO
                        } else {
                            Vec3::new((k & 1) as f32, ((k >> 1) & 1) as f32, ((k >> 2) & 1) as f32) * size - Vec3::splat(size * 0.5)
                        };
                        let (vel, lifetime) = match sink {
                            Some((sp, sv)) => ((sp + sv * 0.22 - (p + o)) / 0.22, 0.22),
                            None => (base + Vec3::Y * r.range(1.5, 5.0) + r.unit_vec3() * 1.2, r.range(3.0, 6.5)),
                        };
                        debris.write(SpawnDebris { pos: p + o, vel, color, size: size * 0.96, emissive: def.emissive * 3.0, lifetime });
                    }
                }
                // Unsupported pieces fall as clumps sharing one velocity.
                for piece in &outcome.falling {
                    let com = v3(piece.center_of_mass());
                    let away = (com - center).normalize_or_zero();
                    let inherit = push.map(|q| q.vel * 0.35).unwrap_or(Vec3::ZERO);
                    let common = inherit + away * r.range(1.0, 3.5) + Vec3::Y * r.range(0.3, 1.6);
                    let fkeep = (budget as f32 / piece.voxels.len().max(1) as f32).min(1.0);
                    for (v, m) in &piece.voxels {
                        if budget == 0 {
                            break;
                        }
                        if fkeep < 1.0 && r.f32() > fkeep {
                            continue;
                        }
                        budget -= 1;
                        let def = material_def(*m);
                        debris.write(SpawnDebris {
                            pos: v3(voxel_center(*v)),
                            vel: common + r.unit_vec3() * 0.5,
                            color: bd_voxel::material::linear_color(*m),
                            size: VOXEL_SIZE * 0.96,
                            emissive: def.emissive * 3.0,
                            lifetime: r.range(6.0, 9.0),
                        });
                    }
                }
                if radius > 1.2 && sink.is_none() {
                    impulses.write(DebrisImpulse { center, radius: radius * 2.5, strength: 4.0 + radius * 1.5 });
                }
                // One material break sound per carve (rate-limited per frame).
                if sounds < 4 {
                    sounds += 1;
                    if let Some((class, _)) = counts.iter().enumerate().max_by_key(|(_, c)| **c).filter(|(_, c)| **c > 0) {
                        let name = match class {
                            0 => "break_stone",
                            1 | 5 | 6 => "break_dirt",
                            2 => "break_wood",
                            3 => "break_leaves",
                            _ => "break_crystal",
                        };
                        sfx.write(Sfx3d::at(name, center).gain((0.4 + radius * 0.2).min(1.0)));
                    }
                    if !outcome.falling.is_empty() {
                        sfx.write(Sfx3d::at("collapse_rumble", center));
                    }
                }
            }
            other => apply_quiet(&arena, other),
        }
    }
}

/// Apply a terrain event with no debris (replays after a reset, extracts, restores).
fn apply_quiet(arena: &ArenaWorld, e: &GameEvent) {
    let mut w = arena.write();
    match e {
        GameEvent::Carve { op } => {
            let _ = bd_sim::world::apply_carve_op(&mut w, op);
        }
        GameEvent::Extracted { op, .. } => {
            let _ = bd_sim::extract::apply_extract(&mut w, op);
        }
        GameEvent::Power(bd_sim::powers::PowerEvent::Restored { voxels, .. }) => bd_sim::powers::apply_restore(&mut w, voxels),
        _ => {}
    }
}

/// Replace the rendered voxels with a freshly generated world and remesh everything.
fn swap_world(arena: &ArenaWorld, mut world: VoxelWorld, queue: &mut crate::render::voxel_render::RemeshQueue) {
    world.drain_dirty();
    // Every chunk dirty: the arena's remesh pass resubmits them (empty ones clear their meshes).
    world.mark_all_dirty();
    let old: Vec<IVec3> = queue.entities.keys().copied().collect();
    *arena.write() = world;
    for k in old {
        queue.remove(k);
    }
}
