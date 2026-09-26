//! Guns and projectiles, world side (the viewmodel kick is UI's): muzzle flash + light, casings,
//! tracers (bd_fx `ProjectileFx` on every live projectile), hitscan beams, impacts, fire sounds.
//! In client role the local player's own shots were already played from local input, so their
//! `ShotFired` is skipped here.

use super::impact::FlashLight;
use super::sound::Sfx3d;
use super::{bv, corps_rgb, PresentSet};
use crate::arena::ArenaWorld;
use crate::sim_bridge::{ActiveWorld, GameEvents};
use crate::state::AppState;
use crate::tick::TickClock;
use bd_core::DT;
use bd_fx::{BeamFx, CasingFx, ImpactFx, MuzzleFx, ProjectileFx, ProjectileKind as FxKind, SpawnOnomatopoeia};
use bd_sim::projectile::ProjectileKind;
use bd_sim::weapons::GunArchetype;
use bd_sim::GameEvent;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

#[derive(Resource, Default)]
struct Projectiles(HashMap<u16, Entity>);

pub struct GunsPlugin;

impl Plugin for GunsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Projectiles>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (gun_events, sync_projectiles, super::impact::fade_flash_lights).chain().in_set(PresentSet::Sync));
    }
}

fn clear(mut commands: Commands, mut p: ResMut<Projectiles>) {
    for (_, e) in p.0.drain() {
        commands.entity(e).try_despawn();
    }
}

fn fire_sound(a: GunArchetype) -> (&'static str, f32, Option<&'static str>) {
    match a {
        GunArchetype::Ring => ("ring_blast", 1.0, None),
        GunArchetype::HandCannon => ("gun_handcannon", 1.0, Some("gun")),
        GunArchetype::SuperShotgun => ("gun_shotgun", 1.0, Some("shotgun")),
        GunArchetype::Minigun => ("gun_handcannon", 0.9, None),
        GunArchetype::Rocket => ("gun_rocket_fire", 1.0, Some("rocket")),
        GunArchetype::Railgun => ("gun_railgun_fire", 1.0, Some("railgun")),
        GunArchetype::Bow => ("tether_fire", 1.3, None),
        GunArchetype::Cannon => ("explosion_small", 0.8, Some("gun")),
        GunArchetype::Staff => ("ring_blast", 0.8, None),
        GunArchetype::Disc => ("tether_fire", 0.9, None),
    }
}

#[allow(clippy::too_many_arguments)]
fn gun_events(
    mut commands: Commands,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    arena: Option<Res<ArenaWorld>>,
    mut muzzle: MessageWriter<MuzzleFx>,
    mut casing: MessageWriter<CasingFx>,
    mut beam: MessageWriter<BeamFx>,
    mut impact: MessageWriter<ImpactFx>,
    mut sfx: MessageWriter<Sfx3d>,
    mut ono: MessageWriter<SpawnOnomatopoeia>,
    mut minigun_gate: Local<u32>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    for ev in events.read() {
        match &ev.event {
            GameEvent::ShotFired { owner, archetype, muzzle: m, dir, pellets, .. } => {
                if super::replayed_locally(&active, *owner) {
                    continue;
                }
                let col = corps_rgb(active.corps_of(*owner));
                let p = bv(*m);
                let d = bv(*dir).normalize_or(Vec3::NEG_Z);
                let big = matches!(archetype, GunArchetype::SuperShotgun | GunArchetype::Rocket | GunArchetype::Railgun | GunArchetype::Cannon);
                muzzle.write(MuzzleFx { pos: p, dir: d, color: col, scale: if big { 1.6 } else if *archetype == GunArchetype::Ring { 0.6 } else { 1.0 } });
                commands.spawn((
                    FlashLight { t: 0.0, life: 0.06, peak: if big { 4.0e5 } else { 1.5e5 } },
                    PointLight { color: Color::linear_rgb(0.6 + col[0] * 0.4, 0.6 + col[1] * 0.4, 0.6 + col[2] * 0.4), intensity: 0.0, range: 8.0, shadow_maps_enabled: false, ..default() },
                    Transform::from_translation(p + d * 0.3),
                ));
                if matches!(archetype, GunArchetype::HandCannon | GunArchetype::SuperShotgun | GunArchetype::Minigun) {
                    let right = d.cross(Vec3::Y).normalize_or(Vec3::X);
                    casing.write(CasingFx { pos: p - d * 0.25, vel: right * 3.2 + Vec3::Y * 2.2, color: col, floor: None, scale: if *archetype == GunArchetype::SuperShotgun { 1.4 } else { 1.0 } });
                }
                let (name, pitch, word) = fire_sound(*archetype);
                // The minigun fires ~20/s: voice every third shot (the spin loop carries the rest).
                let voice = if *archetype == GunArchetype::Minigun {
                    *minigun_gate = minigun_gate.wrapping_add(1);
                    *minigun_gate % 3 == 0
                } else {
                    true
                };
                if voice {
                    sfx.write(Sfx3d::at(name, p).pitch(pitch));
                }
                if let Some(w) = word {
                    let mut o = SpawnOnomatopoeia::key(w, p + d * 1.2, 0.5 + 0.2 * *pellets as f32, col);
                    o.dir = d;
                    o.event_id = ev.tick ^ owner.0 as u64;
                    ono.write(o);
                }
            }
            GameEvent::Hitscan { owner, origin, end, .. } => {
                let col = corps_rgb(active.corps_of(*owner));
                beam.write(BeamFx { from: bv(*origin), to: bv(*end), color: col, width: 1.0 });
            }
            GameEvent::ProjectileEnded { pos, normal, hit_player, id } => {
                let _ = id;
                let p = bv(*pos);
                let n = bv(*normal).normalize_or(Vec3::Y);
                let surface = if hit_player.is_none() {
                    arena.as_ref().and_then(|a| {
                        let w = a.read();
                        let v = bd_core::voxel::world_to_voxel(crate::util::core_v3(p - n * 0.2));
                        let m = bd_core::voxel::VoxelQuery::material(&*w, v);
                        (m != 0).then(|| bd_voxel::material::linear_color(m))
                    })
                } else {
                    None
                };
                impact.write(ImpactFx { pos: p, normal: n, surface, color: [1.0, 0.95, 0.85], scale: 0.8 });
                let name = if hit_player.is_some() { "ring_blast_hit" } else { "bullet_impact_stone" };
                sfx.write(Sfx3d::at(name, p).gain(0.6));
            }
            _ => {}
        }
    }
}

fn fx_kind(k: ProjectileKind) -> (FxKind, f32) {
    match k {
        ProjectileKind::RingBlast => (FxKind::Bolt, 1.0),
        ProjectileKind::Bolt => (FxKind::Bolt, 0.8),
        ProjectileKind::Arrow => (FxKind::Slug, 0.8),
        ProjectileKind::CannonBall => (FxKind::Rocket, 1.4),
        ProjectileKind::Disc => (FxKind::Pellet, 1.6),
    }
}

fn sync_projectiles(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    clock: Res<TickClock>,
    mut map: ResMut<Projectiles>,
    mut q: Query<&mut Transform, With<ProjectileFx>>,
    mut seen: Local<Vec<u16>>,
) {
    let Some(active) = active else { return };
    seen.clear();
    let t = clock.alpha * DT;
    for pr in &active.world.projectiles {
        if pr.dead {
            continue;
        }
        seen.push(pr.id);
        let pos = bv(pr.pos) + bv(pr.vel) * t;
        let v = bv(pr.vel);
        let tf = Transform::from_translation(pos).looking_to(v.normalize_or(Vec3::NEG_Z), Vec3::Y);
        match map.0.get(&pr.id) {
            Some(e) => {
                if let Ok(mut t) = q.get_mut(*e) {
                    *t = tf;
                }
            }
            None => {
                let (k, size) = fx_kind(pr.spec.kind);
                let mut fx = ProjectileFx::new(k, corps_rgb(active.corps_of(pr.spec.owner)));
                fx.size = size;
                let e = commands.spawn((fx, tf, Visibility::default())).id();
                map.0.insert(pr.id, e);
            }
        }
    }
    seen.sort_unstable();
    map.0.retain(|id, e| {
        let keep = seen.binary_search(id).is_ok();
        if !keep {
            commands.entity(*e).try_despawn();
        }
        keep
    });
}
