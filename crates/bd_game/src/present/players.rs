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
//! * **Held items ride the animated hand** (stabilisation): the costume's right-arm chain end, posed
//!   with the same layers the renderer uses, is published in [`BodyHands`] for
//!   `constructs::place`. Holding a gun raises the arm (low-ready; fully along the aim while
//!   firing / ADS; ADS brings the support hand in) through a `bd_fx::PartOverlay`.
//! * **Third-person near clip**: the local body fades (and hides) when the shoulder camera is
//!   pushed within ~1.6 m of the head (a wall behind you), so the helmet never fills the view.
//! * The tutorial has no sim world: `camera3p` adds a synthetic local view in third person, so the
//!   body is drawn there too.

use super::camera3p::ViewMode;
use super::constructs::body_frames;
use super::impact::HitFreeze;
use super::sound::Sfx3d;
use super::{bv, corps_rgb, PresentSet};
use crate::debris::{SpawnDebris, VfxRng};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use crate::state::AppState;
#[cfg(test)]
use bd_core::mechanics::{Element, Form};
use bd_core::PlayerId;
use bd_fx::{ConstructVisual, PartOverlay, ShatterModelFx};
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

/// Right-hand world positions of the bodies drawn this frame (held items attach here; see
/// `constructs::place`). Written in [`BodySync`].
#[derive(Resource, Default)]
pub struct BodyHands(pub HashMap<PlayerId, Vec3>);

/// Player bodies are posed (runs before construct placement, which reads [`BodyHands`]).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct BodySync;

/// Most costume parts the hand lookup poses (stack scratch; bigger models skip it).
const MAX_POSE_PARTS: usize = 96;

/// Where a costume's hands are: per side (0 = left, 1 = right) the top arm part and the chain end
/// (part index, rest-space point, bd_core types).
#[derive(Clone, Copy, Debug, Default)]
struct HandRig {
    root: [Option<usize>; 2],
    end: [Option<(usize, bd_core::Vec3)>; 2],
}

impl HandRig {
    fn of(model: &VoxelModel) -> Self {
        use bd_shapes::PartKind;
        let mut r = HandRig::default();
        let kinds = &model.rig.kinds;
        for (k, side) in [(0usize, -1i8), (1, 1)] {
            let is_arm = |i: usize| kinds.get(i) == Some(&PartKind::Arm(side));
            let Some(root) = (1..model.parts.len()).find(|&i| is_arm(i) && !is_arm(model.parts[i].parent_index)) else { continue };
            r.root[k] = Some(root);
            // The chain: the root and every part whose ancestry reaches it. The hand = the voxel
            // farthest from the shoulder pivot.
            let pivot = model.parts[root].pivot;
            let mut best: Option<(f32, usize, bd_core::Vec3)> = None;
            for i in root..model.parts.len() {
                let mut j = i;
                let mut hops = 0;
                while j != root && j != 0 && hops < 16 {
                    j = model.parts[j].parent_index;
                    hops += 1;
                }
                if j != root {
                    continue;
                }
                let p = &model.parts[i];
                for (v, _) in &p.voxels {
                    let c = p.voxel_center(*v);
                    let d = c.distance_squared(pivot);
                    if best.is_none_or(|b| d > b.0) {
                        best = Some((d, i, c));
                    }
                }
            }
            r.end[k] = best.map(|(_, i, c)| (i, c));
        }
        r
    }
}

/// Eased gun-raise state of a body.
#[derive(Clone, Copy, Debug, Default)]
struct Raise {
    /// Right arm 0 (down) … 1 (along the aim).
    arm: f32,
    /// Support hand in (ADS).
    ads: f32,
    /// Seconds since the last shot (holds the arm up briefly).
    since_shot: f32,
}

#[derive(Component)]
pub struct PlayerBody {
    pub id: PlayerId,
    choice: CostumeChoice,
    task: Option<Task<(Arc<VoxelModel>, Arc<bd_fx::VoxelColors>)>>,
    distance: f32,
    time: f32,
    last_pos: Vec3,
    cast_t: Option<f32>,
    tether_t: Option<(i8, f32)>,
    react: Option<(Vec3, f32, f32)>,
    spin: f32,
    ropes: [Entity; 2],
    shattered: bool,
    hands: HandRig,
    raise: Raise,
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
            .init_resource::<BodyHands>()
            .add_message::<ShatterPlayer>()
            .add_systems(Startup, setup_ropes)
            .add_systems(OnExit(AppState::InGame), clear_bodies)
            .add_systems(Update, (gather_costumes, body_events, sync_bodies).chain().in_set(PresentSet::Sync).in_set(BodySync));
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
    views: Res<WorldViews>,
    net: Option<Res<crate::net::NetStatus>>,
    prefs: Option<Res<crate::lobby::prefs::UiPrefs>>,
    mut costumes: ResMut<PlayerCostumes>,
) {
    let Some(active) = active else {
        // Tutorial: the synthetic local body (`camera3p::tutorial_view`) wears your costume.
        if let (Some(v), Some(prefs)) = (views.players.iter().find(|p| p.is_local), prefs.as_ref()) {
            let c = &prefs.costume;
            let choice = CostumeChoice { phrase: c.phrase.clone(), seed: c.seed, accent: c.accent, head_scale: c.head_scale, hide: c.hide };
            if costumes.0.get(&v.id) != Some(&choice) {
                costumes.0.insert(v.id, choice);
            }
        }
        return;
    };
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

/// Costume model + per-voxel colours (character looks are painted; see `lobby::costume::dress`).
fn build_costume(choice: CostumeChoice, lex: Option<Arc<bd_lexicon::Lexicon>>) -> Task<(Arc<VoxelModel>, Arc<bd_fx::VoxelColors>)> {
    AsyncComputeTaskPool::get().spawn(async move {
        let tweaks = CostumeTweaks { accent: choice.accent, head_scale: choice.head_scale, hide_accessories: choice.hide };
        let pm = crate::lobby::costume::painted_model(lex.as_deref(), &choice.phrase, choice.seed, tweaks);
        (Arc::new(pm.model), Arc::new(bd_fx::VoxelColors { parts: pm.colors }))
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
            GameEvent::ShotFired { owner, construct: Some(_), .. } => {
                if let Some(&e) = map.0.get(owner)
                    && let Ok((mut b, _, _)) = bodies.get_mut(e)
                {
                    b.raise.since_shot = 0.0;
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
    (aim, ads, fx): (Res<super::camera3p::AimRay>, Res<super::ads::AdsState>, Option<Res<bd_fx::FxSettings>>),
    lex: Option<Res<crate::console::RingLexicon>>,
    mut map: ResMut<BodyEntities>,
    mut hands_out: ResMut<BodyHands>,
    (mut ropes, mut materials): (ResMut<RopeAssets>, ResMut<Assets<StandardMaterial>>),
    mut bodies: Query<(Entity, &mut PlayerBody, Option<&mut ConstructVisual>, &mut Transform, &mut Visibility, Option<&mut PartOverlay>), Without<Rope>>,
    mut rope_q: Query<(&mut Transform, &mut Visibility), With<Rope>>,
    mut trail: MessageWriter<SpawnDebris>,
    mut rng: ResMut<VfxRng>,
) {
    hands_out.0.clear();
    // No sim world = the tutorial (a synthetic local view in third person, see `camera3p`).
    let active = active.as_deref();
    if active.is_none() && views.players.is_empty() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let third = mode.third_person();
    let translucent = fx.as_ref().is_none_or(|f| f.translucent);
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
                            hands: HandRig::default(),
                            raise: Raise::default(),
                        },
                        Transform::from_translation(v.pos),
                        Visibility::Hidden,
                    ))
                    .id();
                map.0.insert(v.id, e);
                continue;
            }
        };
        let Ok((e, mut b, vis, mut tf, mut visibility, overlay)) = bodies.get_mut(e) else { continue };
        // Costume changed (lobby edit / lexicon arrived): rebuild.
        if &b.choice != choice {
            b.choice = choice.clone();
            b.task = Some(build_costume(choice.clone(), lex.as_ref().map(|l| l.0.clone())));
        }
        if let Some(t) = b.task.as_mut()
            && let Some((model, colors)) = check_ready(t)
        {
            b.task = None;
            b.hands = HandRig::of(&model);
            commands.entity(e).insert(PartOverlay { locals: vec![bd_shapes::Local::IDENTITY; model.parts.len()] });
            let mut cv = ConstructVisual::new(model, corps);
            if colors.parts.iter().any(|p| !p.is_empty()) {
                cv = cv.with_colors(colors);
            }
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
        // Third-person near clip: the shoulder camera pushed into the head (a wall behind you) —
        // fade the local body out (hide it when the fade can't: opaque style / too close).
        let near = if v.is_local && third { near_clip_fade(aim.camera.distance(v.pos + Vec3::Y * 1.6 * v.size) / v.size.max(0.2)) } else { 1.0 };
        let clipped = near < if translucent { 0.3 } else { 0.6 };
        let show = alive && !hide_self && !clipped && v.seat.is_none_or(|_| v.has(pflags::SUIT));
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
        // Held item: gun raise / ADS pose, and the animated hand for `constructs::place`.
        let held = active.and_then(|a| a.world.constructs.held_by(v.id)).filter(|c| c.category == bd_core::mechanics::FormCategory::Held && c.attached());
        let gun = held.is_some_and(bd_sim::constructs::gun::is_gun);
        let firing = held.and_then(|c| views.construct(c.id)).is_some_and(|c| {
            use bd_sim::weapons::WeaponPhase::*;
            matches!(bd_sim::weapons::WeaponPhase::from_u8(c.weapon & 7), Firing | Charging | SpinUp)
        });
        b.raise.since_shot += dt;
        let ads_k = if v.is_local && gun { ads.blend } else { 0.0 };
        let (arm_t, ads_t) = match gun {
            true if firing || b.raise.since_shot < 0.9 || ads_k > 0.05 => (1.0, ads_k),
            true => (0.45, 0.0),
            false => (0.0, 0.0),
        };
        let k = 1.0 - (-14.0 * dt).exp();
        b.raise.arm += (arm_t - b.raise.arm) * k;
        b.raise.ads += (ads_t - b.raise.ads) * k;
        if let Some(mut ov) = overlay {
            let raise = b.raise;
            gun_pose(&b.hands, raise, v.pitch, &mut ov.locals);
            if show
                && held.is_some()
                && let Some(h) = hand_world(&cv.model, &layers[..n], &ov.locals, &b.hands, &tf)
            {
                hands_out.0.insert(v.id, h);
            }
        }
        if v.is_local {
            // Near-clip fade (only ever lowers the base 0.92; charge brightens it after this).
            let o = if near < 1.0 { 0.05 + 0.87 * near } else { 0.92 };
            if (cv.opacity - o).abs() > 1e-3 && cv.opacity <= 0.92 + 1e-3 {
                cv.opacity = o;
            }
        }
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
            // The local player's ropes are always the shell's (render/tethers.rs; from the body's
            // hands in third person via `camera3p::ThirdPersonHands`).
            let draw = show && !v.is_local && v.hooks[i].is_some();
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
    let gone: Vec<PlayerId> = map.0.keys().copied().filter(|id| views.player(*id).is_none() && active.is_none_or(|a| a.world.player(*id).is_none())).collect();
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

/// Third-person near clip: 1 = fully drawn … 0 = gone, from the camera's distance to the head (m,
/// body-size normalised). Starts fading inside ~1.6 m (a big costume helmet fills the view from
/// there); gone by ~0.8 m. The normal shoulder camera sits ~3 m out (ADS ~2 m): untouched.
pub fn near_clip_fade(dist: f32) -> f32 {
    let t = ((dist - 0.8) / 0.8).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Gun raise (right arm up along the aim, `raise.arm`) + ADS support hand (`raise.ads`) as
/// part overlays on the arm roots (applied outside the clip layers: walk swing stays on top).
fn gun_pose(h: &HandRig, raise: Raise, pitch: f32, locals: &mut [bd_shapes::Local]) {
    use bd_core::Quat as Q;
    for l in locals.iter_mut() {
        *l = bd_shapes::Local::IDENTITY;
    }
    // Arms hang down (-Y); rx(90° + pitch) points them forward along the aim (model -Z).
    let up = std::f32::consts::FRAC_PI_2 + pitch.clamp(-1.1, 1.1);
    if let Some(r) = h.root[1].filter(|&r| r < locals.len())
        && raise.arm > 1e-3
    {
        // A little outward (the gun reads past the shoulder over the third-person camera), less so
        // aimed in.
        let out = -0.22 * raise.arm * (1.0 - 0.7 * raise.ads);
        locals[r] = bd_shapes::Local::rot(Q::from_rotation_y(out) * Q::from_rotation_x((up + 0.12) * raise.arm));
    }
    if let Some(l) = h.root[0].filter(|&l| l < locals.len())
        && raise.ads > 1e-3
    {
        locals[l] = bd_shapes::Local::rot(Q::from_rotation_y(-0.55 * raise.ads) * Q::from_rotation_x(up * raise.ads));
    }
}

/// World position of the right hand (the arm chain's end) for this frame's layers + overlay.
fn hand_world(model: &VoxelModel, layers: &[(AnimState, f32)], locals: &[bd_shapes::Local], h: &HandRig, tf: &Transform) -> Option<Vec3> {
    let (i, p) = h.end[1]?;
    let n = model.parts.len();
    if n > MAX_POSE_PARTS || i >= n {
        return None;
    }
    let mut pose = [Default::default(); MAX_POSE_PARTS];
    bd_shapes::pose_layered_with(model, layers, Some(locals).filter(|l| !l.is_empty()), &mut pose[..n]);
    let m = super::bv(pose[i].transform_point3(p));
    m.is_finite().then(|| tf.transform_point(m))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn costume(name: &str) -> VoxelModel {
        let r = bd_sim::bots::recipe(name, Form::Biped, &[], Element::Light, 1.8);
        bd_shapes::costume_model(&bd_shapes::costume_with(&r, None, None, 3, CostumeTweaks::default()))
    }

    fn hand(m: &VoxelModel, h: &HandRig, raise: Raise, pitch: f32) -> Vec3 {
        let mut locals = vec![bd_shapes::Local::IDENTITY; m.parts.len()];
        gun_pose(h, raise, pitch, &mut locals);
        hand_world(m, &[(AnimState::Idle, 0.0)], &locals, h, &Transform::IDENTITY).expect("a hand")
    }

    /// Held items ride the costume's animated hand: the right arm chain end is found on the
    /// shipped bot costumes, and the gun raise brings it up in front of the chest along the aim.
    #[test]
    fn costume_hands_are_found_and_the_gun_raise_points_along_the_aim() {
        for name in ["knight", "robot", "wizard", "astronaut", "samurai"] {
            let m = costume(name);
            let h = HandRig::of(&m);
            assert!(h.root[1].is_some() && h.end[1].is_some(), "{name}: right arm");
            let rest = hand(&m, &h, Raise::default(), 0.0);
            let up = hand(&m, &h, Raise { arm: 1.0, ..Default::default() }, 0.0);
            assert!(rest.x > 0.05, "{name}: the right hand is on the right ({rest})");
            assert!(up.z < rest.z - 0.3, "{name}: raised forward ({rest} -> {up})");
            assert!(up.y > rest.y + 0.3, "{name}: raised up ({rest} -> {up})");
            let high = hand(&m, &h, Raise { arm: 1.0, ..Default::default() }, 0.6);
            assert!(high.y > up.y + 0.1, "{name}: follows the aim pitch");
            let ads = hand(&m, &h, Raise { arm: 1.0, ads: 1.0, ..Default::default() }, 0.0);
            assert!(ads.x < up.x, "{name}: ADS brings the gun in toward the sight line");
        }
    }

    #[test]
    fn near_clip_fade_ramps() {
        assert_eq!(near_clip_fade(3.0), 1.0);
        assert_eq!(near_clip_fade(1.7), 1.0, "ADS boom (~2 m) never fades");
        assert_eq!(near_clip_fade(0.7), 0.0);
        let m = near_clip_fade(1.2);
        assert!(m > 0.1 && m < 0.9);
    }
}
