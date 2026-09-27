//! TAUNT & VOICE TELL (client) — the wordless taunt key and the voice-cast tell, presented.
//!
//! * **Keys → input bits**: the `TAUNT` action (default **Y**; tap = next taunt of the rotation, hold =
//!   dance) sets `buttons::TAUNT`; push-to-talk listening for a cast sets `buttons::VOICE`. Both ride
//!   the normal per-tick input to the host (`LocalInputState::tell_held`), which owns the rules
//!   (`bd_sim::taunt`: rate limit, any input cancels, free body only) and replicates them
//!   (`PlayerSnap::tell` + a `Move` event with `move_flags::TAUNT`).
//! * **Local prediction**: the same `TauntState` runs on your own inputs every tick, so your body (3P)
//!   and hands (1P) taunt on the frame you let go of the key — and stop on the frame you move.
//! * [`TellViews`]: per player this frame — the taunt playing (+ seconds into it) and whether they are
//!   speaking a cast. `present::body_motion` turns it into the `Channel` layer (taunt poses; voice ==
//!   the typing pose: ring fist to the face, head bowed, ring motes), and this module adds the
//!   voice-specific cue: sound-wave rings in the corps colour rolling off the mouth (never on your own
//!   first-person view), the taunt's tiny sound, and a little flair (flex glints, dance notes).
//! * 1P: [`taunt_hands`] adds a matching hand gesture to `viewmodel::HandPose`.
//! * `--scenario taunt` (`--practice --bots 4`): the taunts, a voice caster and a typist side by side
//!   in front of the camera (recordings / the runtime check).

use super::camera3p::ViewMode;
use super::sound::Sfx3d;
use super::{PresentSet, corps_rgb};
use crate::input::{CursorCapture, LocalInputState, UiFocus};
use crate::lobby::prefs::{Action, KeyBinds};
use crate::player::{LocalPlayer, PlayerSim};
use crate::sim_bridge::{ActiveWorld, CmdSource, GameEvents, SimCommand, WorldViews};
use crate::state::{AppState, InGameMenu};
use crate::tick::{FrameSet, SimSet, SimTick};
use bd_core::PlayerId;
use bd_core::input::buttons;
use bd_fx::{Emit, FxParticles, FxSettings, PFlags, PKind};
use bd_shapes::Channel;
use bd_sim::GameEvent;
use bd_sim::events::move_flags;
use bd_sim::replicate::pflags;
use bd_sim::taunt::{TauntKind, TauntState, tell};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// Voice rings: one every this many seconds while speaking.
const RING_EVERY: f32 = 0.26;
/// Particles per ring.
const RING_DOTS: u32 = 14;
/// Skip effects of players farther than this (m).
const FX_RANGE: f32 = 90.0;
/// Taunt sound gain (tiny: a taunt is a wink, not a callout).
const TAUNT_GAIN: f32 = 0.7;

/// The `Channel` pose of a taunt.
pub fn channel(k: TauntKind) -> Option<Channel> {
    Some(match k {
        TauntKind::None => return None,
        TauntKind::Flex => Channel::Flex,
        TauntKind::Beckon => Channel::Beckon,
        TauntKind::Shrug => Channel::Shrug,
        TauntKind::Bow => Channel::Bow,
        TauntKind::Dance => Channel::Dance,
    })
}

/// One player's tells this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TellView {
    pub taunt: TauntKind,
    /// Seconds into the taunt.
    pub t: f32,
    /// Speaking a cast (push-to-talk in voice cast mode).
    pub voice: bool,
}

/// Every player's tells this frame (filled before `body_motion`).
#[derive(Resource, Default)]
pub struct TellViews(pub HashMap<PlayerId, TellView>);

/// Your own taunt, predicted on your own inputs (the host runs the same rules).
#[derive(Resource, Default)]
pub struct LocalTaunt {
    pub state: TauntState,
    /// Started this tick (sound + flair next frame).
    started: Option<TauntKind>,
}

/// A remote player's taunt as seen here: the event starts it at once; the snapshot confirms it
/// (and ends it: a cancel, a late join).
#[derive(Clone, Copy, Debug, Default)]
struct Remote {
    kind: TauntKind,
    t: f32,
    /// The snapshot has shown `kind` since it started (before that a missing kind means "not yet").
    seen: bool,
    ring: f32,
    flair: f32,
}

#[derive(Resource, Default)]
struct Remotes(HashMap<PlayerId, Remote>);

/// [`TellViews`] is written in this set (`body_motion` runs after it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TellSet;

pub struct TauntPlugin;

impl Plugin for TauntPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TellViews>()
            .init_resource::<LocalTaunt>()
            .init_resource::<Remotes>()
            .add_message::<SimCommand>()
            .add_message::<GameEvents>()
            .add_systems(Update, taunt_keys.in_set(FrameSet::Input))
            .add_systems(SimTick, predict_local.in_set(SimSet::Post))
            .add_systems(Update, update_tells.in_set(PresentSet::Sync).in_set(TellSet))
            .add_systems(Update, tell_fx.in_set(PresentSet::Sync).after(update_tells));
        scenario::build(app);
    }
}

/// The taunt key and push-to-talk → this tick's input bits.
#[allow(clippy::too_many_arguments)]
fn taunt_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Option<Res<KeyBinds>>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    focus: Res<UiFocus>,
    capture: Res<CursorCapture>,
    voice: Option<Res<crate::voice_ui::VoiceUi>>,
    mut input: ResMut<LocalInputState>,
) {
    let in_play = *state.get() == AppState::InGame && !menu.0 && capture.grabbed;
    // A letter bound to the taunt must not fire while typing in the ring console.
    let live = in_play && !focus.typing;
    let (held, just) = binds.as_ref().filter(|_| live).map_or((false, false), |b| (b.pressed(Action::Taunt, &keys, &mouse), b.just_pressed(Action::Taunt, &keys, &mouse)));
    let speaking = in_play && voice.as_ref().is_some_and(|v| v.listening);
    let bits = if held { buttons::TAUNT } else { 0 } | if speaking { buttons::VOICE } else { 0 };
    if input.tell_held != bits {
        input.tell_held = bits;
    }
    if just {
        input.tell_latched |= buttons::TAUNT;
    }
}

/// Your own taunt on your own inputs, once per tick (same rules as the host; the free-body check
/// from the predicted movement state + your replicated flags).
fn predict_local(
    input: Res<LocalInputState>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    local_q: Query<&PlayerSim, With<LocalPlayer>>,
    mut local: ResMut<LocalTaunt>,
) {
    let Some(active) = active else {
        *local = LocalTaunt::default();
        return;
    };
    let Some(id) = active.local else { return };
    let view = views.player(id);
    let alive = view.is_some_and(|v| v.alive());
    let mv = local_q.single().ok().map(|s| s.state);
    let p = active.world.player(id);
    let free = alive
        && mv.is_some_and(|s| s.grounded && s.tumble <= 0.0)
        && view.is_some_and(|v| v.seat.is_none() && !v.has(pflags::TUMBLING | pflags::LAUNCHED | pflags::RECITING | pflags::CHARGING | pflags::SEATED))
        && p.is_none_or(|p| !p.status.stunned());
    if !alive {
        local.state.stop();
        return;
    }
    if let Some(k) = local.state.step(&input.current, free, bd_core::DT) {
        local.started = Some(k);
    }
}

#[allow(clippy::too_many_arguments)]
fn update_tells(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    voice: Option<Res<crate::voice_ui::VoiceUi>>,
    mut local: ResMut<LocalTaunt>,
    mut cmds: MessageReader<SimCommand>,
    mut events: MessageReader<GameEvents>,
    mut remotes: ResMut<Remotes>,
    mut out: ResMut<TellViews>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    out.0.clear();
    let Some(active) = active else {
        events.clear();
        cmds.clear();
        remotes.0.clear();
        return;
    };
    let dt = time.delta_secs().min(0.1);
    // Any command of yours (a quick cast, a power, dismiss…) is input: it ends your taunt now (the
    // host does the same when it applies it).
    if cmds.read().any(|c| matches!(c.source, CmdSource::Local)) {
        local.state.stop();
    }
    for ev in events.read() {
        if let GameEvent::Move { player, flags, .. } = &ev.event
            && flags & move_flags::TAUNT != 0
            && active.local != Some(*player)
        {
            let k = move_flags::taunt_kind(*flags);
            let r = remotes.0.entry(*player).or_default();
            *r = Remote { kind: k, t: 0.0, seen: false, ring: r.ring, flair: 0.0 };
            if let (Some(name), Some(v)) = (k.sound(), views.player(*player)) {
                sfx.write(Sfx3d::at(name, v.pos + Vec3::Y * 1.4 * v.size).gain(TAUNT_GAIN));
            }
        }
    }
    let listening = voice.as_ref().is_some_and(|v| v.listening);
    for v in &views.players {
        if v.is_local {
            if let Some(k) = local.started.take()
                && let Some(name) = k.sound()
            {
                sfx.write(Sfx3d::at(name, v.pos + Vec3::Y * 1.4 * v.size).gain(TAUNT_GAIN));
            }
            let st = local.state;
            out.0.insert(v.id, TellView { taunt: st.kind, t: st.t, voice: listening && v.alive() });
            continue;
        }
        let snap = TauntKind::from_bits(v.tell & tell::KIND);
        let r = remotes.0.entry(v.id).or_default();
        if snap == r.kind && snap.is_some() {
            r.seen = true;
            r.t += dt;
        } else if r.kind.is_some() && !r.seen && r.t < 0.35 {
            // The event ran ahead of the interpolated snapshot: keep playing, it will catch up.
            r.t += dt;
        } else if snap.is_some() {
            // Joined mid-taunt / missed the event.
            *r = Remote { kind: snap, t: 0.0, seen: true, ring: r.ring, flair: 0.0 };
        } else {
            r.kind = TauntKind::None;
            r.t = 0.0;
        }
        let voice = v.alive() && v.tell & tell::VOICE != 0;
        out.0.insert(v.id, TellView { taunt: r.kind, t: r.t, voice });
    }
    if remotes.0.len() > views.players.len() {
        remotes.0.retain(|id, _| views.player(*id).is_some());
    }
}

/// Tiny deterministic jitter.
fn jit(s: &mut u32) -> f32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    (*s >> 8) as f32 / (1u32 << 24) as f32
}

/// Voice rings off the mouth + taunt flair.
#[allow(clippy::too_many_arguments)]
fn tell_fx(
    time: Res<Time<Real>>,
    tells: Res<TellViews>,
    views: Res<WorldViews>,
    mode: Res<ViewMode>,
    fx_settings: Option<Res<FxSettings>>,
    cam: Query<&GlobalTransform, With<crate::camera::PlayerCamera>>,
    mut remotes: ResMut<Remotes>,
    mut fx: Option<ResMut<FxParticles>>,
    mut local_fx: Local<(f32, f32, u32)>,
) {
    let Some(fx) = fx.as_deref_mut() else { return };
    let dt = time.delta_secs().min(0.1);
    let budget = fx_settings.as_ref().map_or(1.0, |f| f.particle_budget.clamp(0.0, 1.0));
    let eye = cam.single().map(|g| g.translation()).ok();
    let third = mode.third_person();
    let (lring, lflair, seed) = &mut *local_fx;
    for v in &views.players {
        let Some(tv) = tells.0.get(&v.id).copied() else { continue };
        if !v.alive() || (v.is_local && !third) || eye.is_some_and(|p| p.distance(v.pos) > FX_RANGE) {
            continue;
        }
        let (ring, flair) = if v.is_local {
            (&mut *lring, &mut *lflair)
        } else {
            let r = remotes.0.entry(v.id).or_default();
            (&mut r.ring, &mut r.flair)
        };
        if *seed == 0 {
            *seed = 0x2545_F491;
        }
        let s = v.size.clamp(0.3, 3.0);
        let body = Quat::from_rotation_y(v.yaw);
        let fwd = body * Vec3::NEG_Z;
        let right = body * Vec3::X;
        let color = corps_rgb(v.corps);
        // Voice: a sound-wave ring rolls forward off the mouth every beat, growing as it goes.
        if tv.voice {
            *ring -= dt;
            if *ring <= 0.0 {
                *ring = RING_EVERY;
                let mouth = v.pos + Vec3::Y * 1.52 * s + fwd * 0.26 * s;
                let n = ((RING_DOTS as f32) * (0.5 + 0.5 * budget)).round().max(6.0) as u32;
                let spin = jit(seed) * std::f32::consts::TAU;
                for k in 0..n {
                    let a = spin + k as f32 / n as f32 * std::f32::consts::TAU;
                    let radial = right * a.cos() + Vec3::Y * a.sin();
                    fx.emit(Emit {
                        pos: mouth + radial * 0.1 * s,
                        vel: fwd * 2.6 * s + radial * 1.5 * s + v.vel * 0.9,
                        size: 0.035 * s,
                        color,
                        emissive: 3.2,
                        life: 0.42,
                        gravity: 0.0,
                        drag: 1.5,
                        seed: *seed ^ k,
                        flags: PFlags::PLAIN | PFlags::WHITE_BIRTH,
                        ..default()
                    });
                }
            }
        } else {
            *ring = 0.0;
        }
        // Flair: glints off the fists at each flex pump; notes bobbing up off a dancer.
        match tv.taunt {
            TauntKind::Flex => {
                for c in [0.34, 0.6] {
                    if tv.t >= c && tv.t - dt < c {
                        for side in [-1.0f32, 1.0] {
                            let fist = v.pos + Vec3::Y * 2.0 * s + right * side * 0.62 * s;
                            for _ in 0..5 {
                                let j = Vec3::new(jit(seed) - 0.5, jit(seed), jit(seed) - 0.5);
                                fx.emit(Emit {
                                    pos: fist + j * 0.12 * s,
                                    vel: j * 2.4 * s + Vec3::Y * 1.2,
                                    size: 0.04 * s,
                                    color: [1.0, 0.95, 0.8],
                                    emissive: 5.0,
                                    life: 0.22,
                                    gravity: 0.2,
                                    spin: 6.0,
                                    seed: *seed,
                                    flags: PFlags::PLAIN | PFlags::STRETCH,
                                    ..default()
                                });
                            }
                        }
                    }
                }
            }
            TauntKind::Dance => {
                *flair += dt * 3.0 * budget;
                while *flair >= 1.0 {
                    *flair -= 1.0;
                    let side = if jit(seed) < 0.5 { -1.0 } else { 1.0 };
                    fx.emit(Emit {
                        kind: PKind::Rise,
                        pos: v.pos + Vec3::Y * (1.9 + 0.3 * jit(seed)) * s + right * side * 0.45 * s,
                        size: 0.06 * s,
                        color,
                        emissive: 2.6,
                        life: 0.9,
                        gravity: 0.9,
                        spin: 0.12,
                        seed: *seed,
                        flags: PFlags::PLAIN,
                        ..default()
                    });
                }
            }
            _ => *flair = 0.0,
        }
    }
}

// ------------------------------------------------------------------------------------ 1P hands

/// The local taunt's matching first-person hand gesture, on top of the procedural hand pose
/// (between `viewmodel_anim::drive_hand_anim` and `animate_viewmodel`).
pub(crate) fn taunt_hands(
    time: Res<Time>,
    local: Res<LocalTaunt>,
    mut pose: ResMut<crate::viewmodel::HandPose>,
    mut w: Local<(f32, TauntKind, f32)>,
) {
    let dt = time.delta_secs().min(0.05);
    let st = local.state;
    // Ease in quickly; out fast (any input cancels at once).
    let target = if st.kind.is_some() { 1.0 } else { 0.0 };
    let rate = if st.kind.is_some() { 14.0 } else { 26.0 };
    w.0 += (target - w.0) * (1.0 - (-rate * dt).exp());
    if st.kind.is_some() {
        w.1 = st.kind;
        w.2 = st.t;
    }
    if w.0 < 1e-3 {
        return;
    }
    let (g, t) = hand_gesture(w.1, w.2);
    let k = w.0;
    for h in 0..2 {
        pose.hand_t[h] += g.hand_t[h] * k;
        pose.hand_r[h] = Quat::IDENTITY.slerp(g.hand_r[h], k) * pose.hand_r[h];
    }
    pose.root_t += g.root_t * k;
    pose.root_r = Quat::IDENTITY.slerp(g.root_r, k) * pose.root_r;
    let _ = t;
}

/// A hand gesture sample (camera space: x right, y up, z back). Index 0 = left, 1 = right (ring).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandGesture {
    pub hand_t: [Vec3; 2],
    pub hand_r: [Quat; 2],
    pub root_t: Vec3,
    pub root_r: Quat,
}

fn bump(t: f32, c: f32, width: f32) -> f32 {
    let x = ((t - c) / (width * 0.5)).clamp(-1.0, 1.0);
    let y = 1.0 - x * x;
    y * y
}

fn env(t: f32, a: f32, out: f32, dur: f32) -> f32 {
    let i = (t / a.max(1e-3)).clamp(0.0, 1.0);
    let o = ((dur - t) / (dur - out).max(1e-3)).clamp(0.0, 1.0);
    let s = |x: f32| x * x * (3.0 - 2.0 * x);
    s(i).min(s(o))
}

/// The 1P gesture of a taunt at `t` (pure; unit-tested for finiteness and rest at the ends).
pub fn hand_gesture(k: TauntKind, t: f32) -> (HandGesture, f32) {
    let t = if t.is_finite() { t.max(0.0) } else { 0.0 };
    let mut g = HandGesture { hand_t: [Vec3::ZERO; 2], hand_r: [Quat::IDENTITY; 2], root_t: Vec3::ZERO, root_r: Quat::IDENTITY };
    let d = k.duration();
    match k {
        TauntKind::None => {}
        TauntKind::Flex => {
            // Both fists curl up into view (a biceps pump each side), twice.
            let e = env(t, 0.16, 0.86, d);
            let pump = bump(t, 0.34, 0.2) + bump(t, 0.6, 0.2);
            g.hand_t[1] = Vec3::new(0.04, 0.16 + 0.03 * pump, 0.06) * e;
            g.hand_t[0] = Vec3::new(-0.04, 0.14 + 0.03 * pump, 0.06) * e;
            g.hand_r[1] = Quat::from_rotation_z((0.55 + 0.12 * pump) * e) * Quat::from_rotation_x(0.6 * e);
            g.hand_r[0] = Quat::from_rotation_z(-(0.55 + 0.12 * pump) * e) * Quat::from_rotation_x(0.6 * e);
        }
        TauntKind::Beckon => {
            // Ring hand out front, palm up, pulled back toward you twice.
            let e = env(t, 0.14, 0.8, d);
            let pull = bump(t, 0.36, 0.22) + bump(t, 0.62, 0.22);
            g.hand_t[1] = Vec3::new(-0.08, 0.1 + 0.04 * pull, -0.08 + 0.1 * pull) * e;
            g.hand_r[1] = Quat::from_rotation_z(-1.3 * e) * Quat::from_rotation_x((0.2 + 0.7 * pull) * e);
            g.hand_t[0] = Vec3::new(-0.02, -0.06, 0.03) * e;
        }
        TauntKind::Shrug => {
            // Both palms up and out, then the ring hand brushes you off.
            let e = env(t, 0.12, 0.76, d);
            let flick = bump(t, 0.6, 0.26);
            g.hand_t[0] = Vec3::new(-0.07, 0.08, 0.0) * e;
            g.hand_t[1] = Vec3::new(0.07 + 0.1 * flick, 0.08 - 0.03 * flick, -0.04 * flick) * e;
            g.hand_r[0] = Quat::from_rotation_z(0.8 * e);
            g.hand_r[1] = Quat::from_rotation_z(-0.8 * e - 0.6 * flick);
        }
        TauntKind::Bow => {
            // The view doesn't move (never the camera): the hands dip away and the ring hand sweeps across.
            let e = env(t, 0.3, 0.82, d);
            g.root_t = Vec3::new(0.0, -0.1, 0.02) * e;
            g.root_r = Quat::from_rotation_x(-0.18 * e);
            g.hand_t[1] = Vec3::new(-0.14, -0.02, 0.03) * e;
            g.hand_r[1] = Quat::from_rotation_y(0.7 * e);
            g.hand_t[0] = Vec3::new(-0.06, -0.08, 0.06) * e;
        }
        TauntKind::Dance => {
            // Fists pump up alternately on the beat, the hands sway.
            let e = (t / 0.18).clamp(0.0, 1.0);
            let ph = t * std::f32::consts::TAU * 2.2;
            let alt = (ph * 0.5).sin();
            let dip = 0.5 + 0.5 * ph.cos();
            g.hand_t[1] = Vec3::new(0.02 * alt, 0.1 + 0.08 * alt.max(0.0) - 0.03 * (1.0 - dip), 0.04) * e;
            g.hand_t[0] = Vec3::new(0.02 * alt, 0.1 + 0.08 * (-alt).max(0.0) - 0.03 * (1.0 - dip), 0.04) * e;
            g.hand_r[1] = Quat::from_rotation_x(0.5 * e);
            g.hand_r[0] = Quat::from_rotation_x(0.5 * e);
            g.root_r = Quat::from_rotation_z(0.05 * alt * e);
        }
    }
    (g, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_gestures_are_finite_and_rest_at_the_ends() {
        for k in TauntKind::ALL {
            for i in 0..=40 {
                let t = k.duration() * i as f32 / 40.0;
                let (g, _) = hand_gesture(k, t);
                assert!(g.hand_t.iter().all(|v| v.is_finite()) && g.root_t.is_finite(), "{k:?} @ {t}");
                assert!(g.hand_r.iter().all(|q| q.is_finite()) && g.root_r.is_finite());
                assert!(g.hand_t.iter().all(|v| v.length() < 0.35), "{k:?} @ {t}: hands stay near the rest pose");
            }
            if k != TauntKind::Dance {
                let (g, _) = hand_gesture(k, k.duration());
                assert!(g.hand_t.iter().all(|v| v.length() < 1e-3), "{k:?} ends at rest");
            }
            let (g, _) = hand_gesture(k, f32::NAN);
            assert!(g.hand_t.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn every_taunt_has_a_pose() {
        for k in TauntKind::ALL {
            assert!(channel(k).is_some_and(|c| c.is_taunt()), "{k:?}");
        }
        assert!(channel(TauntKind::None).is_none());
    }
}

/// `--scenario taunt` (`--practice --bots 4`): four players facing the camera — one tapping through
/// the taunts, one dancing (held), one voice-casting, one typing (voice vs typing tells side by side).
pub mod scenario {
    use super::*;
    use crate::input::LookAngles;
    use bd_core::input::PlayerInput;

    #[derive(Resource, Clone, Debug, Default)]
    pub struct TauntScenario {
        placed: bool,
        yaw: f32,
        t: f32,
        start_tick: u64,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "taunt") {
            return;
        }
        info!("taunt scenario");
        app.insert_resource(TauntScenario::default())
            .add_systems(Update, place.run_if(resource_exists::<ActiveWorld>))
            .add_systems(SimTick, drive.in_set(SimSet::Input).run_if(resource_exists::<ActiveWorld>));
    }

    fn ground_at(w: &bd_sim::World, x: f32, z: f32, near_y: f32) -> Option<bd_core::Vec3> {
        let from = bd_core::Vec3::new(x, near_y + 20.0, z);
        let hit = bd_core::voxel::raycast(&w.voxels, from, bd_core::Vec3::NEG_Y, 60.0)?;
        ((hit.point.y - near_y).abs() < 1.5).then_some(bd_core::Vec3::new(x, hit.point.y + 0.05, z))
    }

    const SPOTS: [(u8, f32, f32); 4] = [(1, 6.5, -3.3), (2, 5.5, -1.1), (3, 5.5, 1.1), (4, 6.5, 3.3)];

    fn place(mut sc: ResMut<TauntScenario>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
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
            let spots: Vec<_> = SPOTS
                .iter()
                .filter_map(|&(id, d, lat)| {
                    let p = me + dir * d + side * lat;
                    ground_at(&active.world, p.x, p.z, me.y).map(|g| (id, g))
                })
                .collect();
            if spots.len() == SPOTS.len() {
                chosen = Some((yaw, spots));
                break;
            }
        }
        let Some((yaw, spots)) = chosen else {
            if sc.t > 6.0 {
                warn!("taunt scenario: no open ground around the spawn");
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
        info!("taunt scenario: placed {} players", spots.len());
    }

    fn drive(sc: Res<TauntScenario>, mut active: ResMut<ActiveWorld>) {
        if !sc.placed {
            return;
        }
        let tick = active.world.tick;
        let k = tick.saturating_sub(sc.start_tick) % 720;
        let face = sc.yaw + std::f32::consts::PI;
        let mut push = |id: u8, yaw: f32, b: u16| {
            if active.world.player(PlayerId(id)).is_some() {
                active.remote_inputs.push((PlayerId(id), PlayerInput { seq: tick as u32, yaw, buttons: b, ..Default::default() }));
            }
        };
        // 1: a tap every 1.5 s (the rotation: flex, beckon, shrug, bow).
        push(1, face + 0.35, if k % 180 < 6 { buttons::TAUNT } else { 0 });
        // 2: hold to dance for 3 s, rest 3 s.
        push(2, face + 0.1, if k < 360 { buttons::TAUNT } else { 0 });
        // 3: speaking a cast by voice (4 s on, 2 s off).
        push(3, face - 0.1, if k < 480 { buttons::VOICE } else { 0 });
        // 4: typing at the ring (4 s on, 2 s off).
        push(4, face - 0.35, if k < 480 { buttons::TYPING } else { 0 });
    }
}
