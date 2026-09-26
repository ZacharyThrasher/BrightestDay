//! Other players (and the local body in third person): bd_shapes costumes on the fixed humanoid
//! rig, drawn as hard light in their corps colour through bd_fx `ConstructVisual`.
//!
//! * Costume source: [`PlayerCostumes`] (id → phrase + seed + tweaks). NET/lobby fill it from the
//!   lobby profiles (`net::NetStatus::players`), the local player's comes from
//!   `lobby::prefs::UiPrefs`, bots get a deterministic pick. Phrases resolve through the UI's shared
//!   `console::RingLexicon` when it has loaded (else a plain biped); models build off-thread.
//! * Anim from the replicated flags: tumble / slide / tether swing / run / air, + overlays for
//!   tether fire (`Move` events), cast thrust (own `ConstructSpawned`) and hit flinches.
//! * Diegetic health: the body's integrity (subtle cracks + flicker as health drops).
//! * Tether ropes for everyone but the local first-person player (the shell draws those).
//! * KO: [`ShatterPlayer`] shatters the body into its own voxels instantly (no ragdoll).
//! * Remote movement sounds (jumps, hard landings, tethers, boosts) are positional.

use super::camera3p::ViewMode;
use super::constructs::body_frames;
use super::impact::HitFreeze;
use super::sound::Sfx3d;
use super::{bv, corps_rgb, PresentSet};
use crate::debris::{SpawnDebris, VfxRng};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
use bd_core::mechanics::{Element, Form, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::PlayerId;
use bd_fx::{ConstructVisual, ShatterModelFx};
use bd_shapes::{AnimState, CostumeTweaks, VoxelModel};
use bd_sim::events::move_flags;
use bd_sim::replicate::pflags;
use bd_sim::GameEvent;
use bevy::light::NotShadowCaster;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use std::sync::Arc;

/// One player's costume choice (cosmetic; sent once at join).
#[derive(Clone, Debug, PartialEq)]
pub struct CostumeChoice {
    pub phrase: String,
    pub seed: u32,
    pub accent: Option<[f32; 3]>,
    pub head_scale: f32,
    pub hide: u32,
}

impl CostumeChoice {
    pub fn phrase(p: &str, seed: u32) -> Self {
        Self { phrase: p.into(), seed, accent: None, head_scale: 1.0, hide: 0 }
    }
}

/// Costumes by player. NET / lobby write entries; missing players get a default.
#[derive(Resource, Default)]
pub struct PlayerCostumes(pub HashMap<PlayerId, CostumeChoice>);

/// KO: shatter this player's body now.
#[derive(Message, Clone, Copy, Debug)]
pub struct ShatterPlayer {
    pub id: PlayerId,
    pub pos: Vec3,
    pub vel: Vec3,
    pub void: bool,
}

#[derive(Component)]
pub struct PlayerBody {
    pub id: PlayerId,
    choice: CostumeChoice,
    task: Option<Task<Arc<VoxelModel>>>,
    distance: f32,
    time: f32,
    last_pos: Vec3,
    cast_t: Option<f32>,
    tether_t: Option<(i8, f32)>,
    react: Option<(Vec3, f32, f32)>,
    spin: f32,
    ropes: [Entity; 2],
    shattered: bool,
}

#[derive(Resource, Default)]
struct BodyEntities(HashMap<PlayerId, Entity>);

#[derive(Resource)]
struct RopeAssets {
    mesh: Handle<Mesh>,
    mats: HashMap<u8, Handle<StandardMaterial>>,
}

#[derive(Component)]
struct Rope;

const BOT_COSTUMES: [&str; 12] = ["knight", "shark", "robot", "fox", "wizard", "dragon", "pirate", "bear", "samurai", "frog", "astronaut", "owl"];

pub struct PlayersPlugin;

impl Plugin for PlayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerCostumes>()
            .init_resource::<BodyEntities>()
            .add_message::<ShatterPlayer>()
            .add_systems(Startup, setup_ropes)
            .add_systems(OnExit(AppState::InGame), clear_bodies)
            .add_systems(Update, (gather_costumes, body_events, sync_bodies).chain().in_set(PresentSet::Sync));
    }
}

fn setup_ropes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(RopeAssets { mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)), mats: HashMap::default() });
}

fn clear_bodies(mut commands: Commands, mut map: ResMut<BodyEntities>, q: Query<&PlayerBody>) {
    for (_, e) in map.0.drain() {
        if let Ok(b) = q.get(e) {
            for r in b.ropes {
                commands.entity(r).try_despawn();
            }
        }
        commands.entity(e).try_despawn();
    }
}

/// Keep [`PlayerCostumes`] filled: lobby profiles, the local prefs, bots.
fn gather_costumes(
    active: Option<Res<ActiveWorld>>,
    net: Option<Res<crate::net::NetStatus>>,
    prefs: Option<Res<crate::lobby::prefs::UiPrefs>>,
    mut costumes: ResMut<PlayerCostumes>,
) {
    let Some(active) = active else { return };
    if let Some(net) = net.as_ref().filter(|n| n.is_changed()) {
        for p in &net.players {
            let c = &p.profile.costume;
            let choice = CostumeChoice { phrase: c.phrase.clone(), seed: c.seed, accent: c.accent, head_scale: c.head_scale, hide: c.hide };
            if costumes.0.get(&p.id) != Some(&choice) {
                costumes.0.insert(p.id, choice);
            }
        }
    }
    if let (Some(local), Some(prefs)) = (active.local, prefs.as_ref()) {
        let c = &prefs.costume;
        let choice = CostumeChoice { phrase: c.phrase.clone(), seed: c.seed, accent: c.accent, head_scale: c.head_scale, hide: c.hide };
        if costumes.0.get(&local) != Some(&choice) {
            costumes.0.insert(local, choice);
        }
    }
    for p in &active.world.players {
        if !costumes.0.contains_key(&p.id) {
            let i = (p.id.0 as usize * 7 + active.world.cfg.seed as usize) % BOT_COSTUMES.len();
            costumes.0.insert(p.id, CostumeChoice::phrase(BOT_COSTUMES[i], p.id.0 as u32));
        }
    }
}

fn build_costume(choice: CostumeChoice, lex: Option<Arc<bd_lexicon::Lexicon>>) -> Task<Arc<VoxelModel>> {
    AsyncComputeTaskPool::get().spawn(async move {
        let (recipe, icon): (Recipe, Option<ShapeSpec>) = match &lex {
            Some(l) => {
                let r = l.resolve(&choice.phrase);
                (r.recipe, r.shape)
            }
            None => (bd_sim::bots::recipe(&choice.phrase, Form::Biped, &[], Element::Light, 1.8), None),
        };
        let tweaks = CostumeTweaks { accent: choice.accent, head_scale: choice.head_scale, hide_accessories: choice.hide };
        let spec = bd_shapes::costume_with(&recipe, icon.as_ref(), None, choice.seed, tweaks);
        Arc::new(bd_shapes::costume_model(&spec))
    })
}

fn body_events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    map: Res<BodyEntities>,
    mut bodies: Query<(&mut PlayerBody, &ConstructVisual, &GlobalTransform)>,
    mut shatter: MessageReader<ShatterPlayer>,
    mut shatter_fx: MessageWriter<ShatterModelFx>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else {
        events.clear();
        shatter.clear();
        return;
    };
    for ev in events.read() {
        match &ev.event {
            GameEvent::ConstructSpawned { owner, .. } => {
                if let Some(&e) = map.0.get(owner)
                    && let Ok((mut b, _, _)) = bodies.get_mut(e)
                {
                    b.cast_t = Some(0.0);
                }
            }
            GameEvent::Hit { target, impact, .. } => {
                if let Some(&e) = map.0.get(target)
                    && let Ok((mut b, _, gt)) = bodies.get_mut(e)
                {
                    let local_dir = gt.compute_transform().rotation.inverse() * bv(impact.dir);
                    b.react = Some((local_dir, impact.strength.min(1.0), 0.0));
                }
            }
            GameEvent::Move { player, flags, speed } => {
                let remote = active.local != Some(*player);
                let Some(&e) = map.0.get(player) else { continue };
                let Ok((mut b, _, gt)) = bodies.get_mut(e) else { continue };
                let p = gt.translation();
                if flags & move_flags::TETHER_FIRE != 0 {
                    b.tether_t = Some((1, 0.0));
                }
                if !remote {
                    continue;
                }
                if flags & move_flags::JUMP != 0 {
                    sfx.write(Sfx3d::at("jump", p).gain(0.5).pitch(if flags & move_flags::PERFECT_HOP != 0 { 1.06 } else { 1.0 }));
                }
                if flags & move_flags::LAND_HARD != 0 {
                    sfx.write(Sfx3d::at("land_heavy", p).gain((speed / 25.0).clamp(0.4, 1.0)));
                }
                if flags & move_flags::TETHER_FIRE != 0 {
                    sfx.write(Sfx3d::at("tether_fire", p).gain(0.6));
                }
                if flags & move_flags::TETHER_ATTACH != 0 {
                    sfx.write(Sfx3d::at("tether_attach", p).gain(0.6));
                }
                if flags & move_flags::BOOST != 0 {
                    sfx.write(Sfx3d::at("boost_burst", p).gain(0.6));
                }
                if flags & move_flags::SLIDE != 0 {
                    sfx.write(Sfx3d::at("land_light", p).gain(0.4).pitch(0.8));
                }
            }
            _ => {}
        }
    }
    for s in shatter.read() {
        let Some(&e) = map.0.get(&s.id) else { continue };
        let Ok((mut b, vis, gt)) = bodies.get_mut(e) else { continue };
        if b.shattered {
            continue;
        }
        b.shattered = true;
        shatter_fx.write(ShatterModelFx {
            model: vis.model.clone(),
            transform: gt.affine(),
            pose: None,
            corps_color: vis.corps_color,
            accent: vis.accent,
            impulse: s.vel * 0.6 + Vec3::Y * 2.0,
            point: Some(s.pos + Vec3::Y),
            floor: None,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_bodies(
    mut commands: Commands,
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    costumes: Res<PlayerCostumes>,
    freeze: Res<HitFreeze>,
    mode: Res<ViewMode>,
    lex: Option<Res<crate::console::RingLexicon>>,
    mut map: ResMut<BodyEntities>,
    mut ropes: ResMut<RopeAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bodies: Query<(Entity, &mut PlayerBody, Option<&mut ConstructVisual>, &mut Transform, &mut Visibility), Without<Rope>>,
    mut rope_q: Query<(&mut Transform, &mut Visibility), With<Rope>>,
    mut trail: MessageWriter<SpawnDebris>,
    mut rng: ResMut<VfxRng>,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.1);
    let third = mode.third_person();
    for v in &views.players {
        let Some(choice) = costumes.0.get(&v.id) else { continue };
        let corps = corps_rgb(v.corps);
        let e = match map.0.get(&v.id).copied() {
            Some(e) => e,
            None => {
                let mat = ropes
                    .mats
                    .entry(v.corps as u8)
                    .or_insert_with(|| {
                        materials.add(StandardMaterial {
                            base_color: Color::linear_rgb(corps[0], corps[1], corps[2]),
                            emissive: LinearRgba::rgb(corps[0] * 4.0, corps[1] * 4.0, corps[2] * 4.0),
                            unlit: true,
                            ..default()
                        })
                    })
                    .clone();
                let mesh = ropes.mesh.clone();
                let r = [0, 1].map(|_| commands.spawn((Rope, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster)).id());
                let e = commands
                    .spawn((
                        Name::new(format!("PlayerBody{}", v.id.0)),
                        PlayerBody {
                            id: v.id,
                            choice: choice.clone(),
                            task: Some(build_costume(choice.clone(), lex.as_ref().map(|l| l.0.clone()))),
                            distance: 0.0,
                            time: 0.0,
                            last_pos: v.pos,
                            cast_t: None,
                            tether_t: None,
                            react: None,
                            spin: 0.0,
                            ropes: r,
                            shattered: false,
                        },
                        Transform::from_translation(v.pos),
                        Visibility::Hidden,
                    ))
                    .id();
                map.0.insert(v.id, e);
                continue;
            }
        };
        let Ok((e, mut b, vis, mut tf, mut visibility)) = bodies.get_mut(e) else { continue };
        // Costume changed (lobby edit / lexicon arrived): rebuild.
        if &b.choice != choice {
            b.choice = choice.clone();
            b.task = Some(build_costume(choice.clone(), lex.as_ref().map(|l| l.0.clone())));
        }
        if let Some(t) = b.task.as_mut()
            && let Some(model) = check_ready(t)
        {
            b.task = None;
            let mut cv = ConstructVisual::new(model, corps);
            cv.opacity = 0.92;
            cv.seed = v.id.0 as u32 * 7919;
            if let Some(a) = choice.accent {
                cv.accent = Some(Color::srgb(a[0], a[1], a[2]).to_linear().to_f32_array_no_alpha());
            }
            commands.entity(e).insert(cv);
            continue;
        }
        let Some(mut cv) = vis else { continue };
        let alive = v.alive();
        if alive {
            b.shattered = false;
        }
        let hide_self = v.is_local && !third;
        let show = alive && !hide_self && v.seat.is_none_or(|_| v.has(pflags::SUIT));
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != want {
            *visibility = want;
        }
        b.time += dt;
        if !freeze.player(v.id) {
            let (_, _, body) = body_frames(v);
            *tf = Transform::from_translation(v.pos).with_rotation(body).with_scale(Vec3::splat(v.size));
        }
        let moved = Vec2::new(v.pos.x - b.last_pos.x, v.pos.z - b.last_pos.z).length();
        if moved < 5.0 {
            b.distance += moved;
        }
        b.last_pos = v.pos;
        // Anim.
        let hspeed = Vec2::new(v.vel.x, v.vel.z).length();
        let grounded = v.has(pflags::GROUNDED);
        let attached = [v.has(pflags::HOOK_L_ATTACHED), v.has(pflags::HOOK_R_ATTACHED)];
        let base = if v.has(pflags::TUMBLING) && v.has(pflags::LAUNCHED) && v.vel.length() > 4.0 {
            // Weight pass: a big launch spins end over end along the flight, limbs trailing.
            let body = Quat::from_rotation_y(v.yaw);
            let dir = body.inverse() * v.vel.normalize_or(Vec3::Y);
            AnimState::Launched { dir: super::gv(dir), spin: (6.0 + v.vel.length() * 0.35).min(22.0) }
        } else if v.has(pflags::TUMBLING) {
            b.spin += dt * (6.0 + v.vel.length() * 0.3);
            AnimState::Tumble { spin: 6.0 + v.vel.length() * 0.3 }
        } else if v.has(pflags::SLIDING) {
            AnimState::Slide { speed: hspeed }
        } else if attached[0] || attached[1] {
            let side = match attached {
                [true, true] => 0,
                [true, false] => -1,
                _ => 1,
            };
            let fwd = Quat::from_rotation_y(v.yaw) * Vec3::NEG_Z;
            let lean = (v.vel.dot(fwd) / 25.0).clamp(-1.0, 1.0);
            AnimState::Swing { side, lean }
        } else {
            AnimState::Locomotion { speed: hspeed, distance: b.distance, airborne: !grounded }
        };
        let t = b.time;
        let mut layers: [(AnimState, f32); 4] = [(base, t); 4];
        let mut n = 1;
        if let Some(ct) = b.cast_t.as_mut() {
            *ct += dt;
            if *ct < 0.5 {
                layers[n] = (AnimState::Cast, *ct);
                n += 1;
            } else {
                b.cast_t = None;
            }
        }
        if let Some((side, tt)) = b.tether_t.as_mut() {
            *tt += dt;
            if *tt < 0.35 && n < 4 {
                layers[n] = (AnimState::TetherFire { side: *side }, *tt);
                n += 1;
            } else if *tt >= 0.35 {
                b.tether_t = None;
            }
        }
        if let Some((dir, s, rt)) = b.react.as_mut() {
            *rt += dt;
            if *rt < 0.4 && n < 4 {
                layers[n] = (AnimState::HitReact { dir: super::gv(*dir), strength: *s }, *rt);
                n += 1;
            } else if *rt >= 0.4 {
                b.react = None;
            }
        }
        if n < 4 {
            layers[n] = (AnimState::Aim { yaw: 0.0, pitch: v.pitch * 0.6 }, t);
            n += 1;
        }
        cv.layers.set(&layers[..n]);
        // Diegetic damage (Smash %): the hard-light body shifts white -> gold -> orange -> red
        // and cracks creep in as % rises (readable at 40 m); never fully "broken".
        let (dmg_col, k) = damage_glow(v.damage_pct);
        cv.corps_color = [corps[0] + (dmg_col[0] - corps[0]) * k, corps[1] + (dmg_col[1] - corps[1]) * k, corps[2] + (dmg_col[2] - corps[2]) * k];
        cv.accent = Some(dmg_col);
        cv.integrity = 1.0 - (v.damage_pct / 180.0).clamp(0.0, 0.72);
        // Launch trail: glowing shards stream off a launched body, hotter the faster it flies.
        if v.launch > 0.02 && show {
            let n = (v.launch * 5.0).ceil() as usize;
            for _ in 0..n {
                let r = &mut rng.0;
                trail.write(SpawnDebris {
                    pos: v.pos + Vec3::Y * 0.9 * v.size + r.unit_vec3() * 0.35,
                    vel: -v.vel * 0.08 + r.unit_vec3() * 0.8,
                    color: dmg_col,
                    size: r.range(0.06, 0.16) * (0.6 + v.launch),
                    emissive: 3.0 + 6.0 * v.launch,
                    lifetime: r.range(0.25, 0.5),
                });
            }
        }
        cv.flash = if v.has(pflags::SHIELD) { 0.25 + 0.15 * (t * 12.0).sin().abs() } else { 0.0 };
        // Ropes (not the local first-person player: the shell draws those).
        let (eye, look, _) = body_frames(v);
        for i in 0..2 {
            let Ok((mut rt, mut rv)) = rope_q.get_mut(b.ropes[i]) else { continue };
            let draw = show && !(v.is_local && !third) && v.hooks[i].is_some();
            let want = if draw { Visibility::Inherited } else { Visibility::Hidden };
            if *rv != want {
                *rv = want;
            }
            if let (true, Some(h)) = (draw, v.hooks[i]) {
                let hand = eye + look * Vec3::new(if i == 0 { -0.35 } else { 0.35 } * v.size, -0.3 * v.size, -0.2);
                let d = h - hand;
                let len = d.length().max(0.01);
                *rt = Transform::from_translation(hand + d * 0.5).looking_to(d / len, Vec3::Y).with_scale(Vec3::new(0.035, 0.035, len));
            }
        }
    }
    // Players that left.
    let gone: Vec<PlayerId> = map.0.keys().copied().filter(|id| views.player(*id).is_none() && active.world.player(*id).is_none()).collect();
    for id in gone {
        if let Some(e) = map.0.remove(&id) {
            if let Ok((_, b, ..)) = bodies.get(e) {
                for r in b.ropes {
                    commands.entity(r).try_despawn();
                }
            }
            commands.entity(e).try_despawn();
        }
    }
}

/// Damage % → glow colour (linear) and how far the body leans into it (0..1).
pub fn damage_glow(pct: f32) -> ([f32; 3], f32) {
    const STOPS: [(f32, [f32; 3]); 4] = [(0.0, [1.0, 1.0, 1.0]), (50.0, [1.0, 0.78, 0.25]), (100.0, [1.0, 0.42, 0.08]), (150.0, [1.0, 0.08, 0.04])];
    let p = pct.max(0.0);
    let mut c = STOPS[3].1;
    for w in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if p <= b {
            let t = ((p - a) / (b - a)).clamp(0.0, 1.0);
            c = [ca[0] + (cb[0] - ca[0]) * t, ca[1] + (cb[1] - ca[1]) * t, ca[2] + (cb[2] - ca[2]) * t];
            break;
        }
    }
    (c, (p / 120.0).clamp(0.0, 0.7))
}
