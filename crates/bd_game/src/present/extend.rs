//! WUKONG: telescoping poles (`Mechanic::Extends`, bd_sim `constructs/extend.rs`) — the Ruyi Jingu
//! Bang, the power pole, extending swords and stretchy fists — drawn as one continuous hard-light
//! pole (never voxel pops): an iron-dark shaft with a gold band at each end and a faint corps-colour
//! glow shell, from the grip along the item's axis.
//!
//! * **Source of truth**: the construct's replicated held state (`HeldState::ext / ext_girth / xmove
//!   / xt / xdir`, `ConstructSnap::ext`) in `ActiveWorld::world` and the construct entity's transform
//!   (`present::constructs`: the owner's hand + the swing curve in third person, the viewmodel grip in
//!   first person). The Extends construct's own voxel visual is hidden once it has formed.
//! * **Swings** grow the pole a little at the peak (the sim's `swing_ext`); **idle** it twirls in the
//!   hand every few seconds (the hand slides to its middle and it spins a double turn). **Thrust /
//!   jab / vault** point it along the move's direction, the far part at world scale even in first
//!   person; the **falling pillar** stands up thick and crashes down along its landing line
//!   (`extend::move_dir`, the same function the sim poses it with).
//! * **Client prediction for the local holder** (client role): releasing a charge plays the thrust /
//!   pillar at once from local input (same tuning, same pure functions) instead of waiting a round
//!   trip for the host; remote wielders follow the snapshots (smoothed, clocks advanced locally).
//! * **Dismiss**: the pole shrinks to a needle into the hand with a tink.
//! * **Moments**: extend whoosh, pillar telegraph, the slam (sub-heavy stomp + rumble + camera shake by
//!   distance), the vault plant; creatures striking with an extending weapon draw a transient pole.
//!
//! Cost: 4 entities per live pole (+4 per transient creature strike / dismiss needle).

use super::constructs::SimConstruct;
use super::sound::Sfx3d;
use super::{bq, bv, corps_rgb, replayed_locally};
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::sim_bridge::{ActiveWorld, GameEvents, SimRole};
use crate::state::AppState;
use bd_core::input::buttons;
use bd_core::mechanics::{FormCategory, Mechanic};
use bd_core::{ConstructId, PlayerId};
use bd_shapes::{SwingCurve, SwingKind};
use bd_sim::constructs::AnimState;
use bd_sim::constructs::extend::{self as ext, ExtMove, ExtendTuning};
use bd_sim::events::{GoneReason, HeldFx};
use bd_sim::GameEvent;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// Iron body (linear), gold bands (linear), band length as a fraction of the rest pole.
const IRON: [f32; 3] = [0.075, 0.07, 0.08];
const GOLD: [f32; 3] = [1.0, 0.72, 0.18];
const BAND_FRAC: f32 = 0.11;
/// Idle twirl: every this many seconds of idle, a flourish this long, this many turns.
const TWIRL_EVERY: f32 = 2.8;
const TWIRL_SECS: f32 = 0.6;
const TWIRL_TURNS: f32 = 2.0;
/// Dismiss needle: seconds to shrink into the hand.
const NEEDLE_SECS: f32 = 0.22;

/// The first-person / third-person swing curve for a held item: telescoping items twirl
/// (`bd_shapes::swing_curve_extends`; the combo step rides the `SwingStart` strength 1 + 0.25·k).
pub fn swing_curve_for(extends: bool, form: bd_core::mechanics::Form, kind: SwingKind, fx: HeldFx, strength: f32, weight: f32) -> SwingCurve {
    if !extends {
        return bd_shapes::swing_curve_weighted(form, kind, weight);
    }
    let kind = match (fx, kind) {
        (HeldFx::SwingStart, SwingKind::Primary | SwingKind::Combo(_)) if strength < 1.9 => SwingKind::Combo(((strength - 1.0) / 0.25).round().clamp(0.0, 2.0) as u8),
        (_, k) => k,
    };
    bd_shapes::swing_curve_extends_weighted(kind, weight)
}

/// Is construct `id` a telescoping held item?
pub fn is_pole(active: &ActiveWorld, id: ConstructId) -> bool {
    active.world.constructs.get(id).is_some_and(|c| c.category == FormCategory::Held && c.has(Mechanic::Extends) && !bd_sim::constructs::gun::is_gun(c))
}

#[derive(Clone, Copy)]
struct Pred {
    kind: ExtMove,
    clock: f32,
    xrange: f32,
    xdir: bd_core::Vec3,
}

struct Pole {
    id: ConstructId,
    owner: PlayerId,
    parts: [Entity; 4],
    /// Smoothed length beyond the rest pole and thickness factor.
    ext: f32,
    girth: f32,
    /// Local move clock (advanced between snapshots), and the replicated (kind, xt) it follows.
    clock: f32,
    seen_move: (ExtMove, f32),
    idle: f32,
    twirl: Option<f32>,
    /// Local prediction (client role, own pole): charge timer and the predicted move.
    charge: f32,
    was_primary: bool,
    pred: Option<Pred>,
    /// Last drawn segment (world): grip end, direction, length, radius — for the dismiss needle.
    last: (Vec3, Vec3, f32, f32),
    alive: bool,
}

/// A pole with no construct behind it: a creature's extending strike, or a dismissed pole
/// shrinking into the hand.
struct Ghost {
    parts: [Entity; 4],
    from: Vec3,
    dir: Vec3,
    len: f32,
    radius: f32,
    t: f32,
    /// Strike (out, hold, back) or needle (shrink).
    strike: bool,
}

#[derive(Resource, Default)]
struct Poles {
    live: Vec<Pole>,
    ghosts: Vec<Ghost>,
    /// Ids of every Extends construct seen (their voxel visuals stay hidden, even while dissolving).
    known: Vec<ConstructId>,
}

#[derive(Resource)]
struct PoleAssets {
    cylinder: Handle<Mesh>,
    iron: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
}

#[derive(Component)]
struct PolePart;

pub struct ExtendPlugin;

impl Plugin for ExtendPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<GameEvents>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<Sfx3d>()
            .init_resource::<Poles>()
            .add_systems(Startup, assets)
            .add_systems(Update, (pole_events.in_set(super::PresentSet::Events), track_poles.after(super::PresentSet::Sync)))
            .add_systems(PostUpdate, place_poles.before(TransformSystems::Propagate))
            .add_systems(OnExit(AppState::InGame), clear);
    }
}

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let iron = mats.add(StandardMaterial {
        base_color: Color::linear_rgb(IRON[0], IRON[1], IRON[2]),
        emissive: LinearRgba::rgb(0.05, 0.045, 0.06),
        metallic: 0.85,
        perceptual_roughness: 0.32,
        ..default()
    });
    let gold = mats.add(StandardMaterial {
        base_color: Color::linear_rgb(GOLD[0], GOLD[1], GOLD[2]),
        emissive: LinearRgba::rgb(GOLD[0] * 2.4, GOLD[1] * 2.4, GOLD[2] * 2.4),
        metallic: 1.0,
        perceptual_roughness: 0.25,
        ..default()
    });
    commands.insert_resource(PoleAssets { cylinder: meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(20)), iron, gold });
}

fn glow_mat(col: [f32; 3]) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::linear_rgba(col[0], col[1], col[2], 0.22),
        emissive: LinearRgba::rgb(col[0] * 2.0, col[1] * 2.0, col[2] * 2.0),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    }
}

fn clear(mut commands: Commands, mut p: ResMut<Poles>, parts: Query<Entity, With<PolePart>>) {
    p.live.clear();
    p.ghosts.clear();
    p.known.clear();
    for e in &parts {
        commands.entity(e).despawn();
    }
}

fn spawn_parts(commands: &mut Commands, a: &PoleAssets, glow: Handle<StandardMaterial>) -> [Entity; 4] {
    let part = |commands: &mut Commands, name: &'static str, mat: Handle<StandardMaterial>, shadow: bool| {
        let mut e = commands.spawn((Name::new(name), PolePart, Mesh3d(a.cylinder.clone()), MeshMaterial3d(mat), Transform::from_scale(Vec3::splat(0.001)), Visibility::Hidden));
        if !shadow {
            e.insert(NotShadowCaster);
        }
        e.id()
    };
    [
        part(commands, "PoleBody", a.iron.clone(), true),
        part(commands, "PoleBandGrip", a.gold.clone(), true),
        part(commands, "PoleBandTip", a.gold.clone(), true),
        part(commands, "PoleGlow", glow, false),
    ]
}

/// Moments: sounds, shakes, creature strikes, dismiss needles.
#[allow(clippy::too_many_arguments)]
fn pole_events(
    mut commands: Commands,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    assets: Option<Res<PoleAssets>>,
    mut poles: ResMut<Poles>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut sfx: MessageWriter<Sfx3d>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut fov: MessageWriter<FovPunch>,
    cam: Query<&GlobalTransform, With<crate::camera::PlayerCamera>>,
) {
    let (Some(active), Some(assets)) = (active, assets) else {
        events.clear();
        return;
    };
    let eye = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    for ev in events.read() {
        match &ev.event {
            GameEvent::HeldFx { id, owner, fx, pos, dir, strength } => {
                let p = bv(*pos);
                let d = bv(*dir).normalize_or(Vec3::NEG_Z);
                let mine = active.local == Some(*owner);
                let near = |r: f32| (1.0 - eye.distance(p) / r).clamp(0.0, 1.0);
                match fx {
                    HeldFx::Extend => {
                        // Own thrusts were already heard when released locally (client prediction).
                        if !replayed_locally(&active, *owner) {
                            sfx.write(Sfx3d::at("hook_fire", p).pitch(0.55).gain(1.1));
                            sfx.write(Sfx3d::at("charge_release", p).pitch(0.75).gain(0.7));
                        }
                        if mine {
                            shake.write(CameraShakeEvent { trauma: 0.14, dir: d });
                        }
                        // A creature's extending strike: no held pole to stretch, so draw one.
                        let held = active.world.constructs.get(*id).is_some_and(|c| c.category == FormCategory::Held);
                        if !held && poles.ghosts.len() < 12 {
                            let col = corps_rgb(active.corps_of(*owner));
                            let parts = spawn_parts(&mut commands, &assets, mats.add(glow_mat(col)));
                            poles.ghosts.push(Ghost { parts, from: p, dir: d, len: strength.clamp(0.5, 80.0), radius: 0.09, t: 0.0, strike: true });
                        }
                    }
                    HeldFx::PillarRise => {
                        if !replayed_locally(&active, *owner) {
                            sfx.write(Sfx3d::at("strike_telegraph", p).pitch(0.7).gain(0.9));
                        }
                        sfx.write(Sfx3d::at("charge_full", p).pitch(0.6).gain(0.6));
                    }
                    HeldFx::PillarSlam => {
                        let mid = p + d * (strength * 0.5);
                        sfx.write(Sfx3d::at("kaiju_stomp", mid).gain(1.4).pitch(0.8));
                        sfx.write(Sfx3d::at("explosion_big", mid).gain(0.8).pitch(0.55));
                        sfx.write(Sfx3d::at("collapse_rumble", mid).gain(0.9));
                        let k = if mine { 1.0 } else { (1.0 - eye.distance(mid) / 70.0).clamp(0.0, 1.0) };
                        if k > 0.0 {
                            shake.write(CameraShakeEvent { trauma: 0.65 * k, dir: Vec3::NEG_Y });
                        }
                        if mine {
                            fov.write(FovPunch { degrees: 4.0 });
                        }
                    }
                    HeldFx::Vault => {
                        sfx.write(Sfx3d::at("hook_slam", p).pitch(1.25).gain(0.9));
                        sfx.write(Sfx3d::at("boost_burst", p).gain(0.6 + 0.4 * near(40.0)));
                        if mine {
                            fov.write(FovPunch { degrees: 5.0 });
                            shake.write(CameraShakeEvent { trauma: 0.1, dir: -d });
                        }
                    }
                    _ => {}
                }
            }
            GameEvent::ConstructGone { id, reason, .. } => {
                let Some(i) = poles.live.iter().position(|p| p.id == *id) else { continue };
                let pole = poles.live.swap_remove(i);
                let (from, dir, len, radius) = pole.last;
                if matches!(reason, GoneReason::Dismissed | GoneReason::Replaced | GoneReason::Expired) && len > 0.0 {
                    // Shrinks to a needle and vanishes into the hand — tink.
                    sfx.write(Sfx3d::at("shield_block", from).pitch(2.3).gain(0.55));
                    poles.ghosts.push(Ghost { parts: pole.parts, from, dir, len, radius, t: 0.0, strike: false });
                } else {
                    for e in pole.parts {
                        commands.entity(e).try_despawn();
                    }
                }
            }
            _ => {}
        }
    }
}

/// One pole per live Extends construct; hide their voxel visuals.
fn track_poles(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    assets: Option<Res<PoleAssets>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut poles: ResMut<Poles>,
    mut q: Query<(&SimConstruct, &mut Visibility), Without<PolePart>>,
) {
    let (Some(active), Some(assets)) = (active, assets) else { return };
    let poles = &mut *poles;
    for p in poles.live.iter_mut() {
        p.alive = false;
    }
    for (sc, mut vis) in &mut q {
        let known = poles.known.contains(&sc.id);
        let Some(c) = active.world.constructs.get(sc.id) else {
            if known && *vis != Visibility::Hidden {
                *vis = Visibility::Hidden; // a dismissed pole's dissolve: the needle shows instead
            }
            continue;
        };
        if !(c.category == FormCategory::Held && c.has(Mechanic::Extends)) || bd_sim::constructs::gun::is_gun(c) || !c.active() {
            continue;
        }
        if !known {
            if poles.known.len() > 256 {
                poles.known.drain(..128);
            }
            poles.known.push(sc.id);
        }
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        if let Some(p) = poles.live.iter_mut().find(|p| p.id == sc.id) {
            p.alive = true;
            continue;
        }
        let col = corps_rgb(active.corps_of(sc.owner));
        let parts = spawn_parts(&mut commands, &assets, mats.add(glow_mat(col)));
        poles.live.push(Pole {
            id: sc.id,
            owner: sc.owner,
            parts,
            ext: 0.0,
            girth: 1.0,
            clock: 0.0,
            seen_move: (ExtMove::None, 0.0),
            idle: 0.0,
            twirl: None,
            charge: 0.0,
            was_primary: false,
            pred: None,
            last: (Vec3::ZERO, Vec3::NEG_Z, 0.0, 0.0),
            alive: true,
        });
    }
    let mut i = 0;
    while i < poles.live.len() {
        if poles.live[i].alive {
            i += 1;
            continue;
        }
        let p = poles.live.swap_remove(i);
        for e in p.parts {
            commands.entity(e).try_despawn();
        }
    }
}

/// Place a cylinder from `a` along `d` for `len` metres, radius `r`.
fn seg(tf: &mut Transform, a: Vec3, d: Vec3, len: f32, r: f32) {
    let len = len.max(0.001);
    tf.translation = a + d * (len * 0.5);
    tf.rotation = Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::NEG_Z));
    tf.scale = Vec3::new(r.max(0.001), len, r.max(0.001));
}

type PartQ<'w, 's> = Query<'w, 's, (&'static mut Transform, &'static mut Visibility), (With<PolePart>, Without<SimConstruct>)>;

fn draw(parts: &[Entity; 4], q: &mut PartQ, a: Vec3, d: Vec3, len: f32, r: f32, band: f32, show: bool) {
    let band = band.min(len * 0.45);
    let pieces = [
        (a, len, r),
        (a, band, r * 1.3),
        (a + d * (len - band), band, r * 1.3),
        (a, len, r * 1.9),
    ];
    for (e, (from, l, rad)) in parts.iter().zip(pieces) {
        if let Ok((mut tf, mut vis)) = q.get_mut(*e) {
            let want = if show { Visibility::Visible } else { Visibility::Hidden };
            if *vis != want {
                *vis = want;
            }
            if show {
                seg(&mut tf, from, d, l, rad);
            }
        }
    }
}

/// Pose every pole (PostUpdate: after the construct transforms are final for this frame).
#[allow(clippy::too_many_arguments)]
fn place_poles(
    mut commands: Commands,
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    input: Res<crate::input::LocalInputState>,
    mut poles: ResMut<Poles>,
    constructs: Query<(&SimConstruct, &Transform), Without<PolePart>>,
    mut parts: PartQ,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(active) = active else { return };
    let poles = &mut *poles;
    let dt = time.delta_secs().min(0.05);
    let tuning: ExtendTuning = active.world.cfg.tuning.extend;
    let client = active.role == SimRole::Client;
    for pole in poles.live.iter_mut() {
        let Some(c) = active.world.constructs.get(pole.id) else { continue };
        let Some((_, tf)) = constructs.iter().find(|(sc, _)| sc.id == pole.id) else { continue };
        let s = &c.held;
        let base = ext::base_len(c);
        let base_r = ext::base_radius(c).clamp(0.05, 0.35);
        let grip = tf.translation;
        let scale = tf.scale.x.max(tf.scale.y).max(1e-3);
        let sz = tf.scale.z.max(1e-3);
        let axis = (tf.rotation * Vec3::NEG_Z).normalize_or(Vec3::NEG_Z);
        let right = (tf.rotation * Vec3::X).normalize_or(Vec3::X);
        // ---- local prediction (client, own pole): a released charge plays now.
        let own = client && active.local == Some(pole.owner);
        let primary = input.current.held(buttons::PRIMARY);
        if own {
            if c.anim_state == AnimState::Charging && primary {
                pole.charge += dt;
            }
            if pole.was_primary && !primary && c.anim_state == AnimState::Charging && pole.pred.is_none() {
                let look = bv(input.current.look_dir());
                let k = (pole.charge / bd_sim::constructs::held::CHARGE_MAX).clamp(0.0, 1.0);
                let grounded = active.world.player(pole.owner).is_none_or(|p| p.mv.grounded);
                let kind = ext::release_move(super::gv(look), grounded, &tuning);
                let (xrange, xdir) = if kind == ExtMove::Pillar {
                    let len = tuning.pillar_len * (0.7 + 0.3 * k);
                    let heading = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
                    let total = base + len;
                    let far = grip + heading * total;
                    let ground = bd_core::voxel::raycast(&active.world.voxels, super::gv(far + Vec3::Y * 8.0), bd_core::Vec3::NEG_Y, 80.0).map_or(grip.y - 1.4, |h| h.point.y);
                    let drop = (ground + 0.3 - grip.y).min(total * 0.2).max(-total * 0.95);
                    (len, (heading * total + Vec3::Y * drop).normalize_or(heading))
                } else {
                    (tuning.thrust_min + (tuning.thrust_max - tuning.thrust_min) * k, look)
                };
                pole.pred = Some(Pred { kind, clock: 0.0, xrange, xdir: super::gv(xdir) });
                if kind == ExtMove::Pillar {
                    sfx.write(Sfx3d::at("strike_telegraph", grip).pitch(0.7).gain(0.9));
                } else {
                    sfx.write(Sfx3d::at("hook_fire", grip).pitch(0.55).gain(1.1));
                    sfx.write(Sfx3d::at("charge_release", grip).pitch(0.75).gain(0.7));
                }
            }
            if c.anim_state != AnimState::Charging {
                pole.charge = 0.0;
            }
        }
        pole.was_primary = primary;
        // ---- what to draw: (kind, clock, target ext, girth, dir).
        let (kind, clock, want_ext, want_girth, dir) = if let Some(p) = pole.pred.as_mut() {
            p.clock += dt;
            let (e, g, done) = ext::move_shape(p.kind, p.clock, p.xrange, &tuning);
            // Terrain stops a predicted thrust like the host's (the tip never draws through rock).
            let e = if p.kind == ExtMove::Pillar {
                e
            } else {
                bd_core::voxel::raycast(&active.world.voxels, super::gv(grip), p.xdir, base + e).map_or(e, |h| (h.t - base).max(0.0))
            };
            let d = bv(ext::move_dir(p.kind, p.clock, p.xdir, &tuning));
            let r = (p.kind, p.clock, e, g, d);
            if done || p.clock > 1.6 {
                pole.pred = None;
            }
            r
        } else if s.xmove != ExtMove::None {
            // Replicated / authoritative move; a client advances the clock between snapshots.
            if (s.xmove, s.xt) != pole.seen_move {
                pole.seen_move = (s.xmove, s.xt);
                pole.clock = s.xt;
            } else if client {
                pole.clock += dt;
            } else {
                pole.clock = s.xt;
            }
            let d = bv(ext::move_dir(s.xmove, pole.clock, s.xdir, &tuning));
            (s.xmove, pole.clock, s.ext, s.ext_girth.max(1.0), d)
        } else {
            pole.seen_move = (ExtMove::None, 0.0);
            (ExtMove::None, 0.0, s.ext, 1.0, axis)
        };
        let _ = clock;
        // Smooth toward the target (fast: 25 ms), snap when shrinking to rest.
        let k = 1.0 - (-40.0 * dt).exp();
        pole.ext += (want_ext - pole.ext) * if want_ext > pole.ext || kind == ExtMove::None { k } else { k.max(0.6) };
        pole.girth += (want_girth - pole.girth) * k;
        // ---- idle twirl (only at rest, never while moving the pole).
        let resting = kind == ExtMove::None && c.anim_state == AnimState::Idle && pole.ext < 0.05;
        if resting {
            pole.idle += dt;
            if pole.twirl.is_none() && pole.idle > TWIRL_EVERY {
                pole.twirl = Some(0.0);
                pole.idle = 0.0;
            }
        } else {
            pole.idle = 0.0;
            pole.twirl = None;
        }
        let (a, d, len, r) = if kind == ExtMove::None {
            let len = (base + pole.ext) * sz;
            let r = base_r * scale;
            match pole.twirl.as_mut() {
                Some(t) => {
                    *t += dt;
                    let u = (*t / TWIRL_SECS).clamp(0.0, 1.0);
                    let e = u * u * (3.0 - 2.0 * u);
                    let ang = e * TWIRL_TURNS * std::f32::consts::TAU;
                    let d = (Quat::from_axis_angle(right, ang) * axis).normalize_or(axis);
                    let b = (u * std::f32::consts::PI).sin();
                    if u >= 1.0 {
                        pole.twirl = None;
                    }
                    (grip - d * (len * 0.5 * b), d, len, r)
                }
                None => (grip, axis, len, r),
            }
        } else {
            // Out in the world: the extension is world-sized even from a first-person grip.
            let len = base * sz + pole.ext;
            let world_r = base_r * pole.girth;
            let reach = (pole.ext / (pole.ext + 2.0)).clamp(0.0, 1.0);
            let r = base_r * scale * pole.girth + (world_r - base_r * scale * pole.girth) * reach;
            (grip, dir.normalize_or(axis), len, r)
        };
        let band = (base * sz * BAND_FRAC).max(0.03) * pole.girth.sqrt();
        draw(&pole.parts, &mut parts, a, d, len, r, band, true);
        pole.last = (a, d, len, r);
    }
    // Ghosts: creature strikes (out 0.08 s, hold 0.1 s, back 0.17 s) and dismiss needles.
    let mut i = 0;
    while i < poles.ghosts.len() {
        let g = &mut poles.ghosts[i];
        g.t += dt;
        let (len, r, a, done) = if g.strike {
            let f = if g.t < 0.08 {
                g.t / 0.08
            } else if g.t < 0.18 {
                1.0
            } else {
                1.0 - ((g.t - 0.18) / 0.17).clamp(0.0, 1.0)
            };
            (g.len * f, g.radius, g.from, g.t >= 0.35)
        } else {
            let u = (g.t / NEEDLE_SECS).clamp(0.0, 1.0);
            let k = (1.0 - u) * (1.0 - u);
            (g.len * k + 0.02, g.radius * (0.15 + 0.85 * k), g.from, u >= 1.0)
        };
        if done {
            let g = poles.ghosts.swap_remove(i);
            for e in g.parts {
                commands.entity(e).try_despawn();
            }
            continue;
        }
        let band = (len * 0.08).clamp(0.02, 0.3);
        let parts_e = g.parts;
        let dir = g.dir;
        draw(&parts_e, &mut parts, a, dir, len.max(0.01), r, band, len > 0.005);
        i += 1;
    }
    let _ = bq;
}
