//! ANIMATION PASS — player body motion + body effects (third-person bodies: remote players, and the
//! local one in third person). Everything is derived from what is already replicated (`PlayerView`
//! flags / velocity / yaw, `GameEvent::Move` flags, `GameEvent::Respawn`) — no new network traffic —
//! and handed to `players::sync_bodies` as extra `bd_shapes` layers ([`BodyLayers`]):
//!
//! | layer | from | reads as |
//! |---|---|---|
//! | `Motion` | velocity in body space, yaw rate, `GROUNDED` / `CROUCHED` / `BOOSTING` | air pose by vertical speed (tuck rising, reach falling, flail plunging), bank into turns (air-strafe), strafe / backpedal / sprint lean, crouch (the silhouette now matches the 1.1 m hull), gas-boost flight lean |
//! | `Action` | touchdown (grounded edge + last air vy), `AIR_JUMP`, `DODGE`, `TETHER_RELEASE` | landing squash by fall height (big drops: the lead-foot-forward crouch), air-jump flip in the travel direction, dodge dive, let-go fling (+ a flip when fast) |
//! | `Channel` | `TYPING` (+ push-to-talk for your own body), `RECITING`, `CHARGING` | typing at the ring (fist up at the face, head bowed), the oath (ring fist raised), the charge stance |
//! | `Forming` | `GameEvent::Respawn` | the ring re-forms you part by part |
//!
//! Effects (pooled `bd_fx` particles, rate-limited, [`FxSettings::particle_budget`]-scaled): landing
//! dust + a corps chip ring on hard landings, a pale gas plume while boosting, heel sparks while
//! sliding, wind streaks off the shoulders at high speed, ring motes while typing, and the respawn
//! column (light raining in, a ring shock when the body locks). Nothing is drawn over the crosshair:
//! every effect of your own body is skipped in first person except the landing ring on the ground.
//!
//! Tuning: `assets/tuning/anim.ron` (hot-reloaded; defaults if missing). Frame sheets:
//! `cargo run -p bd_shapes --example body_sheet --release -- [--before|--seq]` → docs/animation/.

use super::camera3p::ViewMode;
use super::{PresentSet, bv, corps_rgb, gv};
use crate::player::{FrameMoveEvents, LocalPlayer, PlayerSim};
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_core::PlayerId;
use bd_fx::{BurstFx, BurstKind, Emit, FxParticles, FxSettings, PFlags, PKind};
use bd_shapes::{AnimState, BodyAction, BodyMotion, Channel};
use bd_sim::GameEvent;
use bd_sim::events::move_flags;
use bd_sim::replicate::pflags;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

// ------------------------------------------------------------------------------------------ tuning

/// `assets/tuning/anim.ron`. Pose amplitudes live in `bd_shapes::anim_body` (shared with the frame
/// sheets); this file sets *when* and *how hard* each layer plays, and every body effect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyAnimTuning {
    /// Master switch for the new layers (false = the pre-pass body).
    pub enabled: bool,
    /// Ease rates (1/s) of the continuous weights: ground ⇄ air, crouch, boost, channel in / out.
    pub air_rate: f32,
    pub crouch_rate: f32,
    pub boost_rate: f32,
    pub channel_in: f32,
    pub channel_out: f32,
    /// Yaw-rate smoothing (1/s) for the bank.
    pub turn_rate_smooth: f32,
    /// Landing: impact speed (m/s, downward) for strength 0 → 1 (strength keeps rising past it, ≤ 1.5).
    pub land_min_speed: f32,
    pub land_full_speed: f32,
    /// Air jump: flip (true) or tuck only.
    pub air_jump_flip: bool,
    /// Tether let-go flips forward above this speed (m/s); below it the arms just fling open.
    pub release_flip_speed: f32,
    /// Respawn re-form time (s).
    pub respawn_form: f32,

    // ---- effects ----
    /// Landing dust above this impact speed (m/s); the corps chip ring above `land_ring_speed`.
    pub land_dust_speed: f32,
    pub land_ring_speed: f32,
    pub land_ring_chips: u32,
    /// Gas plume puffs per second while boosting.
    pub boost_puffs: f32,
    /// Heel sparks per second while sliding (at 12 m/s; scales with speed).
    pub slide_sparks: f32,
    /// Wind streaks per second above `streak_speed` (m/s), full at `streak_full`.
    pub streak_speed: f32,
    pub streak_full: f32,
    pub streak_rate: f32,
    /// Ring motes per second while typing / reciting.
    pub channel_motes: f32,
    /// Respawn: light chips raining in.
    pub respawn_chips: u32,
    /// Ignore a player's effects beyond this distance from the camera (m).
    pub fx_range: f32,
}

impl Default for BodyAnimTuning {
    fn default() -> Self {
        BodyAnimTuning {
            enabled: true,
            air_rate: 14.0,
            crouch_rate: 16.0,
            boost_rate: 10.0,
            channel_in: 12.0,
            channel_out: 8.0,
            turn_rate_smooth: 12.0,
            land_min_speed: 4.0,
            land_full_speed: 20.0,
            air_jump_flip: true,
            release_flip_speed: 20.0,
            respawn_form: 0.45,
            land_dust_speed: 9.0,
            land_ring_speed: 16.0,
            land_ring_chips: 16,
            boost_puffs: 38.0,
            slide_sparks: 34.0,
            streak_speed: 17.0,
            streak_full: 30.0,
            streak_rate: 22.0,
            channel_motes: 10.0,
            respawn_chips: 36,
            fx_range: 90.0,
        }
    }
}

fn tuning_path() -> PathBuf {
    crate::assets_dir().join("tuning").join("anim.ron")
}

fn load_tuning() -> BodyAnimTuning {
    match std::fs::read_to_string(tuning_path()) {
        Ok(text) => ron::from_str::<BodyAnimTuning>(&text).unwrap_or_else(|e| {
            eprintln!("anim tuning: failed to parse {}: {e}; using defaults", tuning_path().display());
            BodyAnimTuning::default()
        }),
        Err(_) => BodyAnimTuning::default(),
    }
}

#[derive(Resource)]
pub struct BodyAnim(pub BodyAnimTuning);

fn hot_reload(time: Res<Time<Real>>, mut tuning: ResMut<BodyAnim>, mut last: Local<(f32, Option<SystemTime>)>) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let Ok(modified) = std::fs::metadata(tuning_path()).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(modified),
        Some(prev) if prev != modified => {
            last.1 = Some(modified);
            if let Ok(text) = std::fs::read_to_string(tuning_path()) {
                match ron::from_str::<BodyAnimTuning>(&text) {
                    Ok(t) => {
                        info!("anim tuning reloaded");
                        tuning.0 = t;
                    }
                    Err(e) => warn!("anim tuning: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------------------------------ state

/// Most extra layers one body gets from here (motion + action + channel/forming).
pub const MAX_BODY_LAYERS: usize = 3;

/// This frame's extra layers for one body, read by `players::sync_bodies`.
#[derive(Clone, Copy, Debug)]
pub struct BodyExtra {
    pub layers: [(AnimState, f32); MAX_BODY_LAYERS],
    pub n: usize,
    /// Scale for the base `Locomotion` speed (the gait fades out as the body leaves the ground; the
    /// air pose is the `Motion` layer's). 1 = unchanged.
    pub gait: f32,
    /// Draw the base as grounded (the `Motion` layer owns the air pose).
    pub grounded_base: bool,
}

impl BodyExtra {
    pub fn layers(&self) -> &[(AnimState, f32)] {
        &self.layers[..self.n]
    }
}

/// Per-player extra layers for this frame (filled before `players::BodySync`).
#[derive(Resource, Default)]
pub struct BodyLayers(pub HashMap<PlayerId, BodyExtra>);

#[derive(Clone, Copy, Debug, Default)]
struct Track {
    time: f32,
    air: f32,
    crouch: f32,
    boost: f32,
    turn: f32,
    last_yaw: Option<f32>,
    /// Lowest vertical speed of the current airborne stretch (for the touchdown strength).
    air_vy: f32,
    was_grounded: bool,
    was_alive: bool,
    action: Option<(BodyAction, bd_core::Vec3, f32, f32)>,
    channel: Option<(Channel, f32)>,
    channel_w: f32,
    respawn_t: Option<f32>,
    // Emission credits (fractional particles carried between frames).
    puff: f32,
    spark: f32,
    streak: f32,
    mote: f32,
    seed: u32,
}

#[derive(Resource, Default)]
struct Tracks(HashMap<PlayerId, Track>);

pub struct BodyMotionPlugin;

impl Plugin for BodyMotionPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BodyAnim(load_tuning()))
            .init_resource::<BodyLayers>()
            .init_resource::<Tracks>()
            .add_systems(Update, hot_reload)
            .add_systems(Update, body_motion.in_set(PresentSet::Sync).before(super::players::BodySync));
        scenario::build(app);
    }
}

/// Tiny deterministic jitter.
fn jit(s: &mut u32) -> f32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    (*s >> 8) as f32 / (1u32 << 24) as f32
}

fn ease(x: &mut f32, target: f32, rate: f32, dt: f32) {
    *x += (target - *x) * (1.0 - (-rate * dt).exp());
}

fn wrap_pi(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[allow(clippy::too_many_arguments)]
fn body_motion(
    time: Res<Time<Real>>,
    tuning: Res<BodyAnim>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mode: Res<ViewMode>,
    frame: Res<FrameMoveEvents>,
    local_q: Query<&PlayerSim, With<LocalPlayer>>,
    voice: Option<Res<crate::voice_ui::VoiceUi>>,
    fx_settings: Option<Res<FxSettings>>,
    cam: Query<&GlobalTransform, With<crate::camera::PlayerCamera>>,
    mut events: MessageReader<GameEvents>,
    mut tracks: ResMut<Tracks>,
    mut out: ResMut<BodyLayers>,
    mut fx: Option<ResMut<FxParticles>>,
    mut burst: MessageWriter<BurstFx>,
) {
    out.0.clear();
    let tu = &tuning.0;
    let Some(active) = active else {
        events.clear();
        tracks.0.clear();
        return;
    };
    if !tu.enabled {
        events.clear();
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let third = mode.third_person();
    let budget = fx_settings.as_ref().map_or(1.0, |f| f.particle_budget.clamp(0.0, 1.0));
    let eye = cam.single().map(|g| g.translation()).ok();

    // One-shot starts from the sim: remote players from the replicated move flags, the local one
    // from this frame's predicted movement events (no round trip).
    let start = |tr: &mut Track, kind: BodyAction, dir: bd_core::Vec3, strength: f32| {
        tr.action = Some((kind, dir, strength, 0.0));
    };
    for ev in events.read() {
        match &ev.event {
            GameEvent::Move { player, flags, .. } if active.local != Some(*player) => {
                let Some(v) = views.player(*player) else { continue };
                let tr = tracks.0.entry(*player).or_default();
                let inv = Quat::from_rotation_y(v.yaw).inverse();
                let h = Vec3::new(v.vel.x, 0.0, v.vel.z);
                let dir = gv(inv * h.normalize_or(Quat::from_rotation_y(v.yaw) * Vec3::NEG_Z));
                if flags & move_flags::AIR_JUMP != 0 {
                    start(tr, BodyAction::AirJump, dir, if tu.air_jump_flip { 1.0 } else { 0.0 });
                }
                if flags & move_flags::DODGE != 0 {
                    start(tr, BodyAction::Dodge, dir, 1.0);
                }
                if flags & move_flags::TETHER_RELEASE != 0 && !v.has(pflags::GROUNDED) {
                    let fast = v.vel.length() >= tu.release_flip_speed;
                    start(tr, BodyAction::Release, dir, if fast { 1.0 } else { 0.5 });
                }
            }
            GameEvent::Respawn { player, pos, .. } => {
                let tr = tracks.0.entry(*player).or_default();
                tr.respawn_t = Some(0.0);
                tr.action = None;
                let color = views.player(*player).map_or([1.0; 3], |v| corps_rgb(v.corps));
                let size = views.player(*player).map_or(1.0, |v| v.size);
                respawn_fx(fx.as_deref_mut(), &mut burst, bv(*pos), color, size, tu, ev.tick as u32 ^ player.0 as u32);
            }
            _ => {}
        }
    }
    if let (Some(id), Ok(sim)) = (active.local, local_q.single())
        && let Some(v) = views.player(id)
    {
        let tr = tracks.0.entry(id).or_default();
        let inv = Quat::from_rotation_y(v.yaw).inverse();
        for ev in &frame.0 {
            if ev.air_jumped {
                let h = Vec3::new(v.vel.x, 0.0, v.vel.z);
                let dir = gv(inv * h.normalize_or(Quat::from_rotation_y(v.yaw) * Vec3::NEG_Z));
                start(tr, BodyAction::AirJump, dir, if tu.air_jump_flip { 1.0 } else { 0.0 });
            }
            if ev.dodged {
                let d = bv(sim.state.extra.dodge_dir);
                start(tr, BodyAction::Dodge, gv(inv * d.normalize_or(Vec3::NEG_Z)), 1.0);
            }
            if (ev.tether_released[0] || ev.tether_released[1]) && !v.has(pflags::GROUNDED) {
                let h = Vec3::new(v.vel.x, 0.0, v.vel.z);
                let dir = gv(inv * h.normalize_or(Quat::from_rotation_y(v.yaw) * Vec3::NEG_Z));
                start(tr, BodyAction::Release, dir, if v.vel.length() >= tu.release_flip_speed { 1.0 } else { 0.5 });
            }
        }
    }

    let listening = voice.as_ref().is_some_and(|v| v.listening);
    for v in &views.players {
        let tr = tracks.0.entry(v.id).or_default();
        if tr.seed == 0 {
            tr.seed = 0x9E37_79B9 ^ (v.id.0 as u32).wrapping_mul(0x85EB_CA6B) | 1;
        }
        tr.time += dt;
        let alive = v.alive();
        if alive && !tr.was_alive && tr.respawn_t.is_none() && tr.time > dt * 1.5 {
            // Respawned without the event reaching us (resync): re-form anyway.
            tr.respawn_t = Some(0.0);
        }
        tr.was_alive = alive;
        if !alive {
            // Whatever the KO fall was, the respawn doesn't land from it.
            tr.air_vy = 0.0;
            tr.was_grounded = true;
            tr.action = None;
            tr.channel = None;
            tr.channel_w = 0.0;
            continue;
        }
        let grounded = v.has(pflags::GROUNDED);
        let sliding = v.has(pflags::SLIDING);
        let tethered = v.has(pflags::HOOK_L_ATTACHED) || v.has(pflags::HOOK_R_ATTACHED);
        let special = v.has(pflags::TUMBLING) || sliding || tethered || v.seat.is_some();
        let body = Quat::from_rotation_y(v.yaw);
        let local_v = body.inverse() * v.vel;
        // Yaw rate (smoothed) for the bank.
        let raw_turn = tr.last_yaw.map_or(0.0, |y| wrap_pi(v.yaw - y) / dt.max(1e-4));
        tr.last_yaw = Some(v.yaw);
        let raw_turn = if raw_turn.abs() > 40.0 { 0.0 } else { raw_turn };
        ease(&mut tr.turn, raw_turn, tu.turn_rate_smooth, dt);
        ease(&mut tr.air, if grounded || special { 0.0 } else { 1.0 }, tu.air_rate, dt);
        ease(&mut tr.crouch, if v.has(pflags::CROUCHED) && !sliding { 1.0 } else { 0.0 }, tu.crouch_rate, dt);
        let boosting = v.has(pflags::BOOSTING) && !grounded && !tethered;
        ease(&mut tr.boost, if boosting { 1.0 } else { 0.0 }, tu.boost_rate, dt);
        // Touchdown: strength from the fastest fall of the airborne stretch.
        if !grounded {
            tr.air_vy = tr.air_vy.min(v.vel.y);
        }
        let mut landed = None;
        if grounded && !tr.was_grounded {
            let impact = (-tr.air_vy).max(0.0);
            if impact > tu.land_min_speed && !sliding {
                let s = ((impact - tu.land_min_speed) / (tu.land_full_speed - tu.land_min_speed).max(0.1)).clamp(0.0, 1.5);
                // A landing never interrupts a dodge / flip still playing.
                if tr.action.is_none_or(|a| matches!(a.0, BodyAction::Land | BodyAction::Release)) {
                    tr.action = Some((BodyAction::Land, bd_core::Vec3::NEG_Z, s, 0.0));
                }
            }
            landed = Some(impact);
            tr.air_vy = 0.0;
        }
        if grounded {
            tr.air_vy = 0.0;
        }
        tr.was_grounded = grounded;
        // Channel (busy with the ring).
        let want = if v.has(pflags::RECITING) {
            Some(Channel::Oath)
        } else if v.has(pflags::TYPING) || (v.is_local && listening) {
            Some(Channel::Typing)
        } else if v.has(pflags::CHARGING) {
            Some(Channel::Charge)
        } else {
            None
        };
        match (want, tr.channel) {
            (Some(k), Some((c, _))) if k == c => {}
            (Some(k), _) => tr.channel = Some((k, 0.0)),
            (None, _) => {}
        }
        ease(&mut tr.channel_w, if want.is_some() && !special { 1.0 } else { 0.0 }, if want.is_some() { tu.channel_in } else { tu.channel_out }, dt);
        if let Some((_, t)) = tr.channel.as_mut() {
            *t += dt;
        }
        if want.is_none() && tr.channel_w < 0.01 {
            tr.channel = None;
        }

        // Layers.
        let mut e = BodyExtra { layers: [(AnimState::Idle, 0.0); MAX_BODY_LAYERS], n: 0, gait: 1.0 - tr.air, grounded_base: true };
        let mo = BodyMotion {
            fwd: -local_v.z,
            side: local_v.x,
            vy: v.vel.y,
            turn: tr.turn,
            air: tr.air,
            crouch: tr.crouch,
            boost: tr.boost,
        };
        if !special {
            e.layers[e.n] = (AnimState::Motion(mo), tr.time);
            e.n += 1;
        } else {
            e.gait = 1.0;
            e.grounded_base = false;
        }
        if let Some((kind, dir, s, t)) = tr.action.as_mut() {
            *t += dt;
            if *t >= kind.duration(*s) || (special && *kind != BodyAction::Release) {
                tr.action = None;
            } else {
                e.layers[e.n] = (AnimState::Action { kind: *kind, dir: *dir, strength: *s }, *t);
                e.n += 1;
            }
        }
        if let Some(rt) = tr.respawn_t.as_mut() {
            *rt += dt;
            if *rt >= tu.respawn_form {
                tr.respawn_t = None;
            } else {
                e.layers[e.n] = (AnimState::Forming { duration: tu.respawn_form.max(0.05) }, *rt);
                e.n += 1;
            }
        } else if let Some((kind, t)) = tr.channel
            && tr.channel_w > 1e-3
        {
            e.layers[e.n] = (AnimState::Channel { kind, w: tr.channel_w }, t);
            e.n += 1;
        }
        out.0.insert(v.id, e);

        // ---- effects ----
        let own_hidden = v.is_local && !third;
        let near = eye.is_none_or(|p| p.distance(v.pos) < tu.fx_range);
        if !near {
            continue;
        }
        let Some(fx) = fx.as_deref_mut() else { continue };
        let color = corps_rgb(v.corps);
        let s = v.size.clamp(0.3, 3.0);
        let fwd = body * Vec3::NEG_Z;
        let right = body * Vec3::X;
        let seed = &mut tr.seed;
        if let Some(impact) = landed
            && impact > tu.land_dust_speed
        {
            let k = ((impact - tu.land_dust_speed) / 20.0).clamp(0.0, 1.0);
            burst.write(BurstFx { kind: BurstKind::Dust, pos: v.pos + Vec3::Y * 0.1, dir: Vec3::Y, scale: (0.5 + 0.9 * k) * s, color: None });
            if impact > tu.land_ring_speed {
                land_ring(fx, v.pos, color, s, k, tu.land_ring_chips, seed);
            }
        }
        if own_hidden {
            continue;
        }
        // Gas plume: pale puffs from the hips, blown back against the boost.
        if tr.boost > 0.05 {
            tr.puff += tu.boost_puffs * budget * tr.boost * dt;
            let back = -(v.vel.normalize_or(fwd) * 0.8 + fwd * 0.2).normalize_or(-fwd);
            let gas = [0.75 + 0.25 * color[0], 0.75 + 0.25 * color[1], 0.75 + 0.25 * color[2]];
            while tr.puff >= 1.0 {
                tr.puff -= 1.0;
                let side = if jit(seed) < 0.5 { -1.0 } else { 1.0 };
                let p = v.pos + Vec3::Y * 0.95 * s + right * side * 0.16 * s - fwd * 0.22 * s;
                let j = Vec3::new(jit(seed) - 0.5, jit(seed) - 0.5, jit(seed) - 0.5);
                fx.emit(Emit {
                    pos: p,
                    vel: v.vel * 0.55 + back * (7.0 + 3.0 * jit(seed)) + j * 2.0,
                    size: (0.12 + 0.1 * jit(seed)) * s,
                    color: gas,
                    emissive: 0.6,
                    life: 0.35 + 0.15 * jit(seed),
                    gravity: -0.1,
                    drag: 4.5,
                    seed: *seed,
                    flags: PFlags::SMOKE | PFlags::GROW,
                    ..default()
                });
            }
        } else {
            tr.puff = 0.0;
        }
        let hs = Vec2::new(v.vel.x, v.vel.z).length();
        // Slide: sparks kicked off the heels.
        if sliding && hs > 5.0 {
            tr.spark += tu.slide_sparks * budget * (hs / 12.0).min(2.0) * dt;
            let dir = Vec3::new(v.vel.x, 0.0, v.vel.z) / hs.max(0.01);
            while tr.spark >= 1.0 {
                tr.spark -= 1.0;
                let side = (jit(seed) - 0.5) * 0.5;
                fx.emit(Emit {
                    pos: v.pos + dir * 0.45 * s + right * side * s + Vec3::Y * 0.05,
                    vel: -dir * (2.0 + 3.0 * jit(seed)) + Vec3::Y * (1.5 + 2.5 * jit(seed)) + right * (jit(seed) - 0.5) * 3.0,
                    size: 0.035 * s,
                    color: [1.0, 0.85 + 0.15 * color[1], 0.55 + 0.4 * color[2]],
                    emissive: 5.0,
                    life: 0.18 + 0.12 * jit(seed),
                    gravity: 1.4,
                    spin: 3.0,
                    seed: *seed,
                    flags: PFlags::PLAIN | PFlags::STRETCH,
                    ..default()
                });
            }
        } else {
            tr.spark = 0.0;
        }
        // High speed: wind streaks peeling off the shoulders (never for your own first-person body).
        let sp = v.vel.length();
        if sp > tu.streak_speed && !grounded {
            let k = ((sp - tu.streak_speed) / (tu.streak_full - tu.streak_speed).max(1.0)).clamp(0.0, 1.0);
            tr.streak += tu.streak_rate * budget * (0.3 + 0.7 * k) * dt;
            while tr.streak >= 1.0 {
                tr.streak -= 1.0;
                let side = if jit(seed) < 0.5 { -1.0 } else { 1.0 };
                let p = v.pos + Vec3::Y * (1.1 + 0.5 * jit(seed)) * s + right * side * (0.3 + 0.15 * jit(seed)) * s;
                fx.emit(Emit {
                    pos: p,
                    vel: v.vel * 0.85,
                    size: 0.03 * s,
                    color,
                    emissive: 1.6 + 1.4 * k,
                    life: 0.16 + 0.08 * jit(seed),
                    gravity: 0.0,
                    drag: 6.0,
                    spin: 9.0 + 10.0 * k,
                    seed: *seed,
                    flags: PFlags::PLAIN | PFlags::STRETCH,
                    ..default()
                });
            }
        } else {
            tr.streak = 0.0;
        }
        // Busy with the ring: corps motes curl up off the ring fist.
        if matches!(tr.channel, Some((Channel::Typing | Channel::Oath, _))) && tr.channel_w > 0.5 {
            tr.mote += tu.channel_motes * budget * dt;
            let oath = matches!(tr.channel, Some((Channel::Oath, _)));
            let hand = if oath { v.pos + Vec3::Y * 2.05 * s + right * 0.25 * s } else { v.pos + Vec3::Y * 1.35 * s + fwd * 0.42 * s + right * 0.05 * s };
            while tr.mote >= 1.0 {
                tr.mote -= 1.0;
                let j = Vec3::new(jit(seed) - 0.5, jit(seed) * 0.3, jit(seed) - 0.5);
                fx.emit(Emit {
                    kind: PKind::Rise,
                    pos: hand + j * 0.2 * s,
                    size: 0.035 * s,
                    color,
                    emissive: 3.0,
                    life: 0.5 + 0.3 * jit(seed),
                    gravity: 0.6,
                    spin: 0.05,
                    seed: *seed,
                    flags: PFlags::PLAIN,
                    ..default()
                });
            }
        } else {
            tr.mote = 0.0;
        }
    }
    // Players that left.
    if tracks.0.len() > views.players.len() {
        tracks.0.retain(|id, _| views.player(*id).is_some());
    }
}

/// A hard landing: a ring of corps chips racing out along the ground from the feet.
fn land_ring(fx: &mut FxParticles, feet: Vec3, color: [f32; 3], s: f32, k: f32, n: u32, seed: &mut u32) {
    let n = n.max(4);
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU + jit(seed) * 0.2;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        fx.emit(Emit {
            pos: feet + d * 0.3 * s + Vec3::Y * 0.06,
            vel: d * (5.0 + 5.0 * k) * s + Vec3::Y * (0.6 + jit(seed)),
            size: (0.07 + 0.04 * k) * s,
            color,
            emissive: 3.5,
            life: 0.3 + 0.1 * k,
            gravity: 0.5,
            drag: 5.0,
            seed: *seed,
            flags: PFlags::PLAIN | PFlags::WHITE_BIRTH,
            ..default()
        });
    }
}

/// Respawn: light chips rain in from above and converge on the body as it re-forms, then a ring
/// shock at the feet when it locks.
fn respawn_fx(fx: Option<&mut FxParticles>, burst: &mut MessageWriter<BurstFx>, feet: Vec3, color: [f32; 3], size: f32, tu: &BodyAnimTuning, seed: u32) {
    let s = size.clamp(0.3, 3.0);
    burst.write(BurstFx { kind: BurstKind::Shards, pos: feet + Vec3::Y * 0.05, dir: Vec3::Y, scale: 0.7 * s, color: Some(color) });
    let Some(fx) = fx else { return };
    let mut sd = seed | 1;
    let form = tu.respawn_form.max(0.1);
    for _ in 0..tu.respawn_chips {
        let a = jit(&mut sd) * std::f32::consts::TAU;
        let r = (0.6 + 1.4 * jit(&mut sd)) * s;
        let h = (5.0 + 5.0 * jit(&mut sd)) * s;
        let from = feet + Vec3::new(a.cos() * r, h, a.sin() * r);
        let to = feet + Vec3::Y * (0.2 + 1.5 * jit(&mut sd)) * s;
        let delay = jit(&mut sd) * form * 0.5;
        let life = form - delay + 0.05;
        fx.emit(Emit {
            pos: from,
            vel: (to - from) / life,
            size: 0.07 * s,
            color,
            emissive: 4.0,
            life,
            gravity: 0.0,
            spin: 4.0,
            seed: sd,
            delay,
            flags: PFlags::PLAIN | PFlags::STRETCH,
            ..default()
        });
    }
    // The lock: a ring racing out on the ground.
    for i in 0..20u32 {
        let a = i as f32 / 20.0 * std::f32::consts::TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        fx.emit(Emit {
            pos: feet + d * 0.35 * s + Vec3::Y * 0.05,
            vel: d * 7.5 * s,
            size: 0.08 * s,
            color,
            emissive: 4.0,
            life: 0.28,
            gravity: 0.0,
            drag: 7.0,
            seed: sd ^ i,
            delay: form,
            flags: PFlags::PLAIN | PFlags::WHITE_BIRTH,
            ..default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_anim_tuning_parses_and_matches_the_defaults_keys() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/tuning/anim.ron");
        let text = std::fs::read_to_string(&path).expect("assets/tuning/anim.ron");
        let t: BodyAnimTuning = ron::from_str(&text).expect("parse anim.ron");
        assert!(t.enabled);
        assert!(t.land_full_speed > t.land_min_speed);
        assert!(t.respawn_form > 0.1 && t.respawn_form < 1.5);
    }

    #[test]
    fn yaw_wrap_is_shortest() {
        assert!((wrap_pi(0.1 - (std::f32::consts::TAU - 0.1)) - 0.2).abs() < 1e-4);
        assert!((wrap_pi(-0.3) + 0.3).abs() < 1e-6);
    }
}

/// `--scenario bodyanim` (`--practice --bots 4`): four players in front of the camera cycling the
/// body layers — a strafing bunny-hopper with air jumps, a typist who then crouch-walks, a booster
/// who air-dodges, a charger who hops — for `--record` clips / screenshots (docs/animation/).
pub mod scenario {
    use super::*;
    use crate::input::LookAngles;
    use crate::tick::{SimSet, SimTick};
    use bd_core::input::{PlayerInput, buttons};

    #[derive(Resource, Clone, Debug, Default)]
    pub struct BodyAnimScenario {
        placed: bool,
        yaw: f32,
        t: f32,
        start_tick: u64,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "bodyanim") {
            return;
        }
        info!("body anim scenario");
        app.insert_resource(BodyAnimScenario::default())
            .add_systems(Update, place.run_if(resource_exists::<ActiveWorld>))
            .add_systems(SimTick, drive.in_set(SimSet::Input).run_if(resource_exists::<ActiveWorld>));
    }

    fn ground_at(w: &bd_sim::World, x: f32, z: f32, near_y: f32) -> Option<bd_core::Vec3> {
        let from = bd_core::Vec3::new(x, near_y + 20.0, z);
        let hit = bd_core::voxel::raycast(&w.voxels, from, bd_core::Vec3::NEG_Y, 60.0)?;
        ((hit.point.y - near_y).abs() < 4.0).then_some(bd_core::Vec3::new(x, hit.point.y + 0.05, z))
    }

    const SPOTS: [(u8, f32, f32); 4] = [(1, 9.0, -5.0), (2, 7.0, -1.7), (3, 7.0, 1.7), (4, 9.0, 5.0)];

    fn place(mut sc: ResMut<BodyAnimScenario>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
        sc.t += time.delta_secs();
        if sc.placed || sc.t < 0.5 {
            return;
        }
        let Some(me) = active.local_player().filter(|p| p.alive).map(|p| p.mv.pos) else { return };
        let mut chosen = None;
        for step in 0..24 {
            let yaw = step as f32 * std::f32::consts::TAU / 24.0;
            let dir = bd_core::input::look_dir(yaw, 0.0);
            let side = bd_core::Vec3::new(-dir.z, 0.0, dir.x);
            let spots: Vec<_> = SPOTS.iter().filter_map(|&(id, d, lat)| {
                let p = me + dir * d + side * lat;
                ground_at(&active.world, p.x, p.z, me.y).map(|g| (id, g))
            }).collect();
            if spots.len() == SPOTS.len() {
                chosen = Some((yaw, spots));
                break;
            }
        }
        let Some((yaw, spots)) = chosen else {
            if sc.t > 6.0 {
                warn!("body anim scenario: no open ground around the spawn");
                sc.placed = true;
            }
            return;
        };
        sc.yaw = yaw;
        for (id, g) in &spots {
            if let Some(p) = active.world.player_mut(PlayerId(*id)) {
                p.mv.pos = *g;
                p.mv.vel = bd_core::Vec3::ZERO;
                p.spawn_shield = 0.0;
            }
        }
        active.bots.clear();
        look.yaw = yaw;
        look.pitch = -0.1;
        sc.start_tick = active.world.tick;
        sc.placed = true;
        info!("body anim scenario: placed {} players", spots.len());
    }

    fn drive(sc: Res<BodyAnimScenario>, mut active: ResMut<ActiveWorld>) {
        if !sc.placed {
            return;
        }
        let tick = active.world.tick;
        let k = tick.saturating_sub(sc.start_tick) % 360;
        // Everyone faces the camera three-quarters.
        let face = sc.yaw + std::f32::consts::PI;
        let mut push = |id: u8, yaw: f32, mx: i8, mz: i8, b: u16| {
            if active.world.player(PlayerId(id)).is_some() {
                active.remote_inputs.push((PlayerId(id), PlayerInput { seq: tick as u32, yaw, move_x: mx, move_z: mz, buttons: b, ..Default::default() }));
            }
        };
        // 1: strafing bunny-hop, an air jump each way.
        let j1 = if matches!(k, 0 | 45 | 180 | 225) || (k % 30 == 0 && k != 60 && k != 240) { buttons::JUMP } else { 0 };
        push(1, face + 0.5, if k < 180 { 1 } else { -1 }, 0, j1);
        // 2: typing at the ring, then a crouch walk.
        let b2 = if k < 150 { buttons::TYPING } else if k < 330 { buttons::CROUCH } else { 0 };
        push(2, face - 0.3, if (170..250).contains(&k) { 1 } else if (250..330).contains(&k) { -1 } else { 0 }, 0, b2);
        // 3: jump + gas boost, then a dodge.
        let b3 = match k {
            0 => buttons::JUMP,
            12..=70 => buttons::BOOST,
            200 => buttons::DODGE,
            _ => 0,
        };
        push(3, face + 0.3, if k == 200 { -1 } else { 0 }, if (12..=70).contains(&k) { 1 } else { 0 }, b3);
        // 4: charge, then hops.
        let b4 = if k < 200 { buttons::CHARGE } else if matches!(k, 220 | 290) { buttons::JUMP } else { 0 };
        push(4, face - 0.5, 0, 0, b4);
    }
}
