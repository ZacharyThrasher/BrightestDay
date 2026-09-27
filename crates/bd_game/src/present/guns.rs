//! Guns and projectiles, world side (the viewmodel kick is UI's): muzzle flash + light, casings,
//! tracers (bd_fx `ProjectileFx` on every live projectile), hitscan beams, impacts, fire sounds.
//! Hitscan gun rounds (`ShotTrace`: hand cannon, minigun, each shotgun pellet) are drawn as a
//! chunky voxel slug that streaks muzzle → impact in a few frames (it must still read as a physical
//! round), then pops the impact FX. Bounded: at most [`MAX_TRACERS`] live, oldest retired first.
//! In client role the local player's own shots were already played from local input, so their
//! `ShotFired` is skipped here.
//! PORTALS: a hitscan round's legs (one `ShotTrace` per portal pass) chain — the exit leg's slug
//! starts when the entry leg's slug reaches the disc; projectiles are drawn where they are after a
//! pass (never extrapolated into the wall behind the entry), and their tracer tail splits across
//! both ends (bd_fx `ProjectileFx::teleport`) instead of smearing across the map.

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

/// Live hitscan tracers (bounded; oldest first).
#[derive(Resource, Default)]
struct Tracers(std::collections::VecDeque<Entity>);

/// Most hitscan tracers alive at once (a shotgun volley is 9; 8 miniguns ≈ 112/s).
pub const MAX_TRACERS: usize = 64;
/// Visual speed of a hitscan tracer slug (m/s): 60 m in ~8 frames at 60 fps — fast, but seen.
pub const TRACER_SPEED: f32 = 450.0;
/// Shortest tracer life (s), so even a point-blank round shows for a couple of frames.
const TRACER_MIN_SECS: f32 = 0.035;

/// One hitscan round in flight on screen.
#[derive(Component, Clone, Copy)]
struct Tracer {
    from: Vec3,
    to: Vec3,
    t: f32,
    dur: f32,
    /// Impact normal (zero = flew to max range: no impact).
    normal: Vec3,
    hit_player: bool,
    impact_scale: f32,
    color: [f32; 3],
    /// Play the impact sound (one per volley; every other minigun round).
    voice: bool,
    /// PORTALS: wait this long before flying (the leg before the portal is still in flight); the
    /// slug is only attached when it starts.
    delay: f32,
    look: Option<ProjectileFx>,
}

pub struct GunsPlugin;

impl Plugin for GunsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Projectiles>()
            .init_resource::<Tracers>()
            .add_systems(OnExit(AppState::InGame), clear)
            .add_systems(Update, (gun_events, sync_projectiles, fly_tracers, super::impact::fade_flash_lights).chain().in_set(PresentSet::Sync));
    }
}

fn clear(mut commands: Commands, mut p: ResMut<Projectiles>, mut tr: ResMut<Tracers>) {
    for (_, e) in p.0.drain() {
        commands.entity(e).try_despawn();
    }
    for e in tr.0.drain(..) {
        commands.entity(e).try_despawn();
    }
}

/// Tracer look per archetype: (bd_fx kind, slug size, impact scale).
fn tracer_look(a: GunArchetype) -> (FxKind, f32, f32) {
    match a {
        GunArchetype::HandCannon => (FxKind::Slug, 1.5, 1.0),
        GunArchetype::SuperShotgun => (FxKind::Pellet, 1.4, 0.45),
        GunArchetype::Minigun => (FxKind::Slug, 0.9, 0.6),
        _ => (FxKind::Slug, 1.0, 0.8),
    }
}

/// Surface colour under an impact (terrain chips), `None` off-terrain.
fn surface_at(arena: Option<&ArenaWorld>, p: Vec3, n: Vec3) -> Option<[f32; 3]> {
    arena.and_then(|a| {
        let w = a.read();
        let v = bd_core::voxel::world_to_voxel(crate::util::core_v3(p - n * 0.2));
        let m = bd_core::voxel::VoxelQuery::material(&*w, v);
        (m != 0).then(|| bd_voxel::material::linear_color(m))
    })
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
        // Palm energy: a deep blast, no gunshot echo (the signature cue adds the roar).
        GunArchetype::HandBeam => ("ring_blast", 0.7, None),
    }
}

#[allow(clippy::too_many_arguments)]
fn gun_events(
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
    mut commands: Commands,
    mut tracers: ResMut<Tracers>,
    mut volley: Local<(u64, u8, u32)>,
    mut chain: Local<Option<(u64, u8, Vec3, f32)>>,
    ads: Option<Res<super::ads::AdsState>>,
    cam: Query<&GlobalTransform, With<crate::camera::PlayerCamera>>,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let ads_blend = ads.map_or(0.0, |a| a.blend.clamp(0.0, 1.0));
    let sight = cam.single().ok().map(|g| (g.translation(), g.forward().as_vec3()));
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
                // (bd_fx's MuzzleFx lights the muzzle from its pooled lights: no per-shot light entity.)
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
                    // Own shots while aiming down sights: the word pops off to the side of the
                    // muzzle (right, a little low), never over the sight line.
                    let mut at = p + d * 1.2;
                    let mut size = 0.5 + 0.2 * *pellets as f32;
                    if active.local == Some(*owner) && ads_blend > 0.0 {
                        let right = d.cross(Vec3::Y).normalize_or(Vec3::X);
                        at += (right * 1.1 - Vec3::Y * 0.35) * ads_blend;
                        size *= 1.0 - 0.35 * ads_blend;
                    } else if let Some((c, f)) = sight {
                        // Someone else's shot word never lands on our sight line while aiming.
                        (at, size) = super::ads::off_sight_line(at, size, c, f, ads_blend);
                    }
                    let mut o = SpawnOnomatopoeia::key(w, at, size, col);
                    o.dir = d;
                    o.event_id = ev.tick ^ owner.0 as u64;
                    // Your own shot words are small and off to the side, and a stream of shots
                    // says every Nth word (bd_fx text budget).
                    o.local_player = active.local == Some(*owner);
                    o.source = owner.0 as u64 + 1;
                    ono.write(o);
                }
            }
            GameEvent::ShotTrace { owner, archetype, from, to, normal, hit_player } => {
                let (a, b) = (bv(*from), bv(*to));
                if !a.is_finite() || !b.is_finite() {
                    continue;
                }
                // One impact voice per volley (shotgun pellets share a tick); every other minigun round.
                let key = (ev.tick, owner.0);
                let first = (volley.0, volley.1) != key;
                if first {
                    *volley = (key.0, key.1, volley.2.wrapping_add(1));
                }
                let voice = first && (*archetype != GunArchetype::Minigun || volley.2 % 2 == 0);
                let (kind, size, impact_scale) = tracer_look(*archetype);
                let color = corps_rgb(active.corps_of(*owner));
                // LEGENDARY: the precision one-shot (golden gun) fires one fat GOLD round.
                let golden = active.world.constructs.held_by(*owner).is_some_and(|c| bd_sim::constructs::legend::one_shot_recipe(&c.recipe, c.flair & bd_sim::replicate::cflags::LEGENDARY != 0));
                let (color, size) = if golden { ([1.0, 0.72, 0.16], size * 2.2) } else { (color, size) };
                if tracers.0.len() >= MAX_TRACERS {
                    if let Some(old) = tracers.0.pop_front() {
                        commands.entity(old).try_despawn();
                    }
                }
                let dir = (b - a).normalize_or(Vec3::NEG_Z);
                let mut fx = ProjectileFx::new(kind, color);
                fx.size = size;
                let dur = ((b - a).length() / TRACER_SPEED).max(TRACER_MIN_SECS);
                // PORTALS: this leg starts at the exit of the portal the previous leg (same shot)
                // ended in: fly once that slug arrives.
                let delay = match *chain {
                    Some((tick, o, end, at)) if tick == ev.tick && o == owner.0 && leg_continues(&active, end, a) => at,
                    _ => 0.0,
                };
                *chain = (*normal == bd_core::Vec3::ZERO && hit_player.is_none()).then_some((ev.tick, owner.0, b, delay + dur));
                let tracer = Tracer {
                    from: a,
                    to: b,
                    t: 0.0,
                    dur,
                    normal: bv(*normal),
                    hit_player: hit_player.is_some(),
                    impact_scale,
                    color,
                    voice,
                    delay,
                    look: (delay > 0.0).then_some(fx),
                };
                let tf = Transform::from_translation(a).looking_to(dir, Vec3::Y);
                let e = if delay > 0.0 { commands.spawn((tracer, tf, Visibility::default())).id() } else { commands.spawn((fx, tracer, tf, Visibility::default())).id() };
                tracers.0.push_back(e);
            }
            GameEvent::Hitscan { owner, origin, end, .. } => {
                // Palm energy draws its own sustained beam from the hands (present::palm_beam).
                if active.world.constructs.held_by(*owner).is_some_and(|c| c.recipe.form == bd_core::mechanics::Form::Hands) {
                    continue;
                }
                // LEGENDARY: the precision one-shot's round is GOLD.
                let golden = active.world.constructs.held_by(*owner).is_some_and(|c| bd_sim::constructs::legend::one_shot_recipe(&c.recipe, c.flair & bd_sim::replicate::cflags::LEGENDARY != 0));
                let col = if golden { [1.0, 0.72, 0.16] } else { corps_rgb(active.corps_of(*owner)) };
                // Caliber: a giant (ultimate) railgun draws a thick beam (width x sqrt(scale / default)).
                let caliber = active.world.constructs.held_by(*owner).filter(|c| c.recipe.form == bd_core::mechanics::Form::Gun).map_or(1.0, |c| {
                    let d = bd_sim::constructs::forms::form_spec(c.recipe.form).default_scale.max(0.1);
                    (c.recipe.scale / d).sqrt().clamp(1.0, 2.5)
                });
                beam.write(BeamFx { from: bv(*origin), to: bv(*end), color: col, width: caliber });
            }
            GameEvent::ProjectileEnded { pos, normal, hit_player, id } => {
                let _ = id;
                let p = bv(*pos);
                let n = bv(*normal).normalize_or(Vec3::Y);
                let surface = if hit_player.is_none() { surface_at(arena.as_deref(), p, n) } else { None };
                impact.write(ImpactFx { pos: p, normal: n, surface, color: [1.0, 0.95, 0.85], scale: 0.8 });
                let name = if hit_player.is_some() { "ring_blast_hit" } else { "bullet_impact_stone" };
                sfx.write(Sfx3d::at(name, p).gain(0.6));
            }
            _ => {}
        }
    }
}

/// Does a hitscan leg starting at `from` continue one that ended at `end` through a portal?
fn leg_continues(active: &ActiveWorld, end: Vec3, from: Vec3) -> bool {
    active.world.portals.iter().any(|p| {
        let pp = super::portal_view::PairGeo::from_sim(p);
        (0..2).any(|i| {
            let (e, f) = (&pp.ends[i], &pp.ends[1 - i]);
            e.side(end).abs() < 0.2 && e.inside(end - e.pos - e.n * e.side(end), 0.3) && (pp.map_point(i, end) - from).length() < 0.3 && f.side(from) > -0.1
        })
    })
}

/// A drawn projectile position that ran past a portal face (extrapolated between ticks, or the
/// sim already passed it) → the exit-side position and pass (entry end geometry + rotation).
fn through_portal(pairs: &[super::portal_view::PairGeo], a: Vec3, b: Vec3) -> Option<(Vec3, bd_fx::PortalSplit)> {
    for pp in pairs {
        for i in 0..2 {
            if pp.ends[i].crossed(a, b, 0.0) {
                let (e, f) = (&pp.ends[i], &pp.ends[1 - i]);
                return Some((pp.map_point(i, b), bd_fx::PortalSplit { entry_pos: e.pos, exit_pos: f.pos, exit_normal: f.n, rot: pp.rot(i) }));
            }
        }
    }
    None
}

/// Did a drawn projectile jump from `prev` (entry side) to `now` (exit side) through a pair?
fn passed(pairs: &[super::portal_view::PairGeo], prev: Vec3, now: Vec3) -> Option<bd_fx::PortalSplit> {
    for pp in pairs {
        for j in 0..2 {
            let back = pp.map_point(j, now);
            let entry = 1 - j;
            let (e, f) = (&pp.ends[entry], &pp.ends[j]);
            if (back - prev).length() < 30.0 && e.crossed(prev, back, 0.3) {
                return Some(bd_fx::PortalSplit { entry_pos: e.pos, exit_pos: f.pos, exit_normal: f.n, rot: pp.rot(entry) });
            }
        }
    }
    None
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
    mut q: Query<(&mut Transform, &mut ProjectileFx), Without<Tracer>>,
    mut seen: Local<Vec<u16>>,
    mut pairs: Local<Vec<super::portal_view::PairGeo>>,
) {
    let Some(active) = active else { return };
    seen.clear();
    pairs.clear();
    pairs.extend(active.world.portals.iter().map(super::portal_view::PairGeo::from_sim));
    let t = clock.alpha * DT;
    for pr in &active.world.projectiles {
        if pr.dead {
            continue;
        }
        seen.push(pr.id);
        let mut pos = bv(pr.pos) + bv(pr.vel) * t;
        let mut v = bv(pr.vel);
        // Extrapolated past a portal face: draw it already out of the exit (the sim passes it on
        // the next tick), never inside the wall behind the entry.
        if !pairs.is_empty()
            && let Some((p, split)) = through_portal(&pairs, bv(pr.pos), pos)
        {
            pos = p;
            v = split.rot * v;
        }
        let tf = Transform::from_translation(pos).looking_to(v.normalize_or(Vec3::NEG_Z), Vec3::Y);
        match map.0.get(&pr.id) {
            Some(e) => {
                if let Ok((mut t, mut fx)) = q.get_mut(*e) {
                    if !pairs.is_empty()
                        && let Some(split) = passed(&pairs, t.translation, pos)
                    {
                        fx.teleport(split);
                    }
                    *t = tf;
                }
            }
            None => {
                let (k, size) = fx_kind(pr.spec.kind);
                // LEGENDARY: the BFG orb is big and green; needles are pink crystal.
                let gun = pr.spec.construct.and_then(|id| active.world.constructs.get(id));
                let (col, size) = match gun {
                    Some(g) if bd_sim::constructs::legend::bfg(g) => ([0.25, 1.0, 0.3], size * 3.0),
                    Some(g) if bd_sim::constructs::legend::needler(g) => ([1.0, 0.32, 0.78], size * 0.7),
                    _ => (corps_rgb(active.corps_of(pr.spec.owner)), size),
                };
                let mut fx = ProjectileFx::new(k, col);
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

/// Advance hitscan tracer slugs muzzle → impact; on arrival retire them and pop the impact FX.
fn fly_tracers(
    mut commands: Commands,
    time: Res<Time>,
    arena: Option<Res<ArenaWorld>>,
    mut tracers: ResMut<Tracers>,
    mut q: Query<(Entity, &mut Tracer, &mut Transform)>,
    mut impact: MessageWriter<ImpactFx>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let dt = time.delta_secs().min(0.1);
    for (e, mut tr, mut tf) in &mut q {
        if tr.delay > 0.0 {
            // PORTALS: the leg before the portal is still flying.
            tr.delay -= dt;
            if tr.delay > 0.0 {
                continue;
            }
            if let Some(fx) = tr.look.take() {
                commands.entity(e).try_insert(fx);
            }
        }
        tr.t += dt;
        let k = (tr.t / tr.dur).min(1.0);
        tf.translation = tr.from.lerp(tr.to, k);
        if k < 1.0 {
            continue;
        }
        commands.entity(e).try_despawn();
        if let Some(i) = tracers.0.iter().position(|x| *x == e) {
            tracers.0.remove(i);
        }
        if tr.normal == Vec3::ZERO {
            continue; // flew to max range
        }
        let n = tr.normal.normalize_or(Vec3::Y);
        let surface = if tr.hit_player { None } else { surface_at(arena.as_deref(), tr.to, n) };
        let c = tr.color;
        impact.write(ImpactFx { pos: tr.to, normal: n, surface, color: [0.55 + c[0] * 0.45, 0.55 + c[1] * 0.45, 0.55 + c[2] * 0.45], scale: tr.impact_scale });
        if tr.voice {
            let name = if tr.hit_player { "ring_blast_hit" } else { "bullet_impact_stone" };
            sfx.write(Sfx3d::at(name, tr.to).gain(0.55));
        }
    }
}
