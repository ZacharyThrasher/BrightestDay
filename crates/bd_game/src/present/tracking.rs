//! Diegetic tracking + melee / hook presentation (owner: "could we get some form of diegetic player
//! tracker? how could we easily enable players to seek each other?"). No minimap, no markers, no
//! text — hard light in the world only (mystery tenet):
//!
//! * **Ring compass** — a thin filament drifts from your ring toward the nearest enemy (or whoever
//!   hit you in the last few seconds): longer and brighter the farther they are, gone while they
//!   are on screen and in sight. Behind you it swings to the screen side to turn toward.
//! * **Lantern beacons** — players beyond ~60 m wear a faint, tall pillar in their corps colour
//!   that pokes over terrain and fades out as you close in (`bd_fx::beacon::beacon_pillar`).
//! * **Loud actions reveal** — firing a gun, summoning something big, charging and taking a heavy
//!   hit ping a brief ring at the player that shows **through walls** (x-ray strokes).
//! * **Speed trails** — anyone above ~12 m/s leaves a short fading ribbon in their colour (total
//!   segments capped; comfort = shorter and softer).
//! * **Melee lunge glow** — your held melee weapon brightens while a swing would lunge-connect
//!   (the sim's own `melee_assist::swing_target` on what you see). No reticle, no text.
//! * **Hook-and-strike** — a bright hard-light rope binds a hooked player (wraps on their body); the
//!   victim gets a subtle tug arc at the screen edge toward the hooker; bursts on "thwip" / snap /
//!   slam. Audio: [`TrackingCue`] messages + [`HookReels`] for the audio worker (placeholder one-shots
//!   through `Sfx3d` meanwhile).
//!
//! All numbers: `assets/tuning/tracking.ron` ([`TrackingTuning`], hot-reloaded; written with the
//! defaults if missing). Everything is drawn with the particle pool's immediate cubes (no new
//! pipeline, nothing to warm up, no per-frame allocation after the first frames).
//! `--scenario tracking --tracking-shot filament|beacon|reveal|trails|hook|victim` stages each one
//! for screenshots (see [`scenario`]).

use std::path::PathBuf;
use std::time::SystemTime;

use bd_core::PlayerId;
use bd_core::voxel::Aabb;
use bd_fx::beacon::{beacon_pillar, tapered_stroke, view_ring, xray_polyline};
use bd_fx::{BurstFx, BurstKind, ConstructVisual, FxParticles, PFlags};
use bd_sim::GameEvent;
use bd_sim::charge::ChargeEvent;
use bd_sim::hook::AssistEvent;
use bd_sim::replicate::pflags;
use bd_sim::weapons::GunArchetype;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use serde::{Deserialize, Serialize};

use super::PresentSet;
use super::camera3p::ViewMode;
use super::constructs::{ConstructEntities, body_frames};
use super::sound::Sfx3d;
use crate::arena::ArenaWorld;
use crate::camera::{CameraUpdateSet, PlayerCamera, ViewAnchor};
use crate::input::LookAngles;
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, PlayerView, WorldViews};
use crate::state::AppState;
use crate::tick::FrameSet;
use crate::viewmodel::{HandsPublishSet, ViewmodelHands};

// ------------------------------------------------------------------------------------ tuning

/// Every tracking / assist presentation number (`assets/tuning/tracking.ron`). Never shown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackingTuning {
    // ---- ring compass
    pub compass: bool,
    /// Whoever hit you within this many seconds is tracked instead of the nearest enemy.
    pub compass_attacker_secs: f32,
    /// Distance where the filament starts growing / is at full length + brightness (m).
    pub compass_near: f32,
    pub compass_far: f32,
    /// Filament length (m) near / far.
    pub compass_len_min: f32,
    pub compass_len_max: f32,
    /// Thickness at the ring / at the tip (m).
    pub compass_thick: f32,
    pub compass_tip_thick: f32,
    /// Emissive near / far.
    pub compass_emissive_min: f32,
    pub compass_emissive_max: f32,
    /// On screen = inside this fraction of the view (and in sight): the filament fades out.
    pub compass_screen_margin: f32,
    /// Fade speed (1/s).
    pub compass_fade_rate: f32,
    /// A target behind you bends the filament to the screen side (least forward component).
    pub compass_min_forward: f32,
    pub compass_segments: u32,
    /// Drift wobble amplitude (m) and frequency (Hz).
    pub compass_wobble: f32,
    pub compass_wobble_hz: f32,
    // ---- lantern beacons
    pub beacon: bool,
    /// Beacons appear beyond `beacon_min_dist`, fully in by `+ beacon_fade_dist` (m)…
    pub beacon_min_dist: f32,
    pub beacon_fade_dist: f32,
    /// …and fade out again toward `beacon_max_dist`.
    pub beacon_max_dist: f32,
    pub beacon_height: f32,
    /// Pillar width = distance × this (≈ a couple of pixels at any range).
    pub beacon_px: f32,
    pub beacon_emissive: f32,
    pub beacon_pieces: u32,
    // ---- loud-action reveal pulses
    pub reveal: bool,
    pub reveal_secs: f32,
    /// Ring radius from / to (m).
    pub reveal_r0: f32,
    pub reveal_r1: f32,
    /// Stroke width = max(reveal_thick, distance × reveal_px).
    pub reveal_thick: f32,
    pub reveal_px: f32,
    pub reveal_emissive: f32,
    /// X-ray strokes are pulled to this distance in front of the camera (m).
    pub reveal_near: f32,
    pub reveal_segments: u32,
    /// One pulse per player per this many seconds; none closer than min / beyond max distance.
    pub reveal_cooldown: f32,
    pub reveal_min_dist: f32,
    pub reveal_max_dist: f32,
    /// Summons at least this big (scale, m) reveal their caster.
    pub reveal_summon_scale: f32,
    /// Hits with at least this knockback (or flagged heavy) reveal the victim.
    pub reveal_heavy_kb: f32,
    /// Charging pings this often (s).
    pub reveal_charge_every: f32,
    // ---- speed trails
    pub trails: bool,
    /// Trails start at / are full at this speed (m/s).
    pub trail_speed: f32,
    pub trail_full_speed: f32,
    pub trail_secs: f32,
    pub trail_thick: f32,
    pub trail_emissive: f32,
    /// Comfort preset: length × / width × emissive ×.
    pub trail_comfort_secs: f32,
    pub trail_comfort_scale: f32,
    /// Total segments drawn for all trails per frame.
    pub trail_max_segments: u32,
    pub trail_samples_per_sec: f32,
    // ---- melee lunge glow
    pub glow: bool,
    /// Extra white on the held weapon at full glow (ConstructVisual::flash).
    pub glow_flash: f32,
    /// Ease rate (1/s) and shimmer (fraction, Hz).
    pub glow_rate: f32,
    pub glow_shimmer: f32,
    pub glow_shimmer_hz: f32,
    // ---- hook-and-strike
    pub hook_rope_thick: f32,
    pub hook_rope_emissive: f32,
    pub hook_wrap_radius: f32,
    pub hook_wrap_emissive: f32,
    pub hook_wrap_spin: f32,
    /// Victim's screen-edge tug arc: distance in front of the eye (m), length, thickness,
    /// emissive, how far out toward the screen edge (1 = edge), pulse rate (Hz).
    pub tug: bool,
    pub tug_dist: f32,
    pub tug_len: f32,
    pub tug_thick: f32,
    pub tug_emissive: f32,
    pub tug_edge: f32,
    pub tug_hz: f32,
    /// Forget a rope nobody reported breaking after this long (s).
    pub link_timeout: f32,
}

impl Default for TrackingTuning {
    fn default() -> Self {
        Self {
            compass: true,
            compass_attacker_secs: 5.0,
            compass_near: 8.0,
            compass_far: 60.0,
            compass_len_min: 0.08,
            compass_len_max: 0.32,
            compass_thick: 0.0032,
            compass_tip_thick: 0.0011,
            compass_emissive_min: 0.6,
            compass_emissive_max: 2.6,
            compass_screen_margin: 0.8,
            compass_fade_rate: 6.0,
            compass_min_forward: 0.35,
            compass_segments: 10,
            compass_wobble: 0.012,
            compass_wobble_hz: 0.7,
            beacon: true,
            beacon_min_dist: 60.0,
            beacon_fade_dist: 20.0,
            beacon_max_dist: 450.0,
            beacon_height: 28.0,
            beacon_px: 0.0026,
            beacon_emissive: 1.4,
            beacon_pieces: 4,
            reveal: true,
            reveal_secs: 0.6,
            reveal_r0: 0.5,
            reveal_r1: 2.6,
            reveal_thick: 0.05,
            reveal_px: 0.0022,
            reveal_emissive: 1.3,
            reveal_near: 1.2,
            reveal_segments: 24,
            reveal_cooldown: 0.5,
            reveal_min_dist: 4.0,
            reveal_max_dist: 160.0,
            reveal_summon_scale: 3.0,
            reveal_heavy_kb: 18.0,
            reveal_charge_every: 1.2,
            trails: true,
            trail_speed: 12.0,
            trail_full_speed: 28.0,
            trail_secs: 0.35,
            trail_thick: 0.09,
            trail_emissive: 2.2,
            trail_comfort_secs: 0.18,
            trail_comfort_scale: 0.6,
            trail_max_segments: 160,
            trail_samples_per_sec: 60.0,
            glow: true,
            glow_flash: 0.22,
            glow_rate: 14.0,
            glow_shimmer: 0.15,
            glow_shimmer_hz: 5.0,
            hook_rope_thick: 0.03,
            hook_rope_emissive: 3.2,
            hook_wrap_radius: 0.55,
            hook_wrap_emissive: 2.6,
            hook_wrap_spin: 2.5,
            tug: true,
            tug_dist: 0.6,
            tug_len: 0.08,
            tug_thick: 0.0045,
            tug_emissive: 2.2,
            tug_edge: 0.85,
            tug_hz: 2.5,
            link_timeout: 12.0,
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct TrackingTuningRes(pub TrackingTuning);

fn tuning_path() -> PathBuf {
    crate::assets_dir().join("tuning").join("tracking.ron")
}

fn load_tuning() -> TrackingTuning {
    let path = tuning_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => ron::from_str::<TrackingTuning>(&text).unwrap_or_else(|e| {
            eprintln!("tracking tuning: failed to parse {}: {e}; using defaults", path.display());
            TrackingTuning::default()
        }),
        Err(_) => {
            let t = TrackingTuning::default();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(text) = ron::ser::to_string_pretty(&t, ron::ser::PrettyConfig::default()) {
                let _ = std::fs::write(
                    &path,
                    format!(
                        "// Tracking + melee/hook presentation (bd_game present/tracking.rs). Hot-reloaded. Missing fields use defaults.\n{text}\n"
                    ),
                );
            }
            t
        }
    }
}

fn hot_reload_tuning(time: Res<Time<Real>>, mut tuning: ResMut<TrackingTuningRes>, mut last: Local<(f32, Option<SystemTime>)>) {
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
                match ron::from_str::<TrackingTuning>(&text) {
                    Ok(t) => {
                        info!("tracking tuning reloaded");
                        tuning.0 = t;
                    }
                    Err(e) => warn!("tracking tuning: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------------------------ audio hooks

/// What happened, for the audio mixer (the audio worker wires these to cues; see the report's
/// wishlist: `hook_thwip`, `hook_snap`, `slam_ready`, `slam_impact`, `lunge_whoosh`, `reveal_ping`,
/// `lunge_glow_on`). Placeholder one-shots play through `Sfx3d` meanwhile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackingCueKind {
    /// A tether hooked a player.
    HookThwip,
    /// A rope on a player broke (dodge, counter, cut, too far, out of sight).
    HookSnap,
    /// Reeled in with a melee weapon: the next swing slams.
    SlamReady,
    /// The slam connected.
    Slam,
    /// Melee lunge dash started.
    Lunge,
    /// A loud-action reveal ping (through walls).
    RevealPing,
    /// The local melee weapon started glowing (a swing would lunge-connect).
    GlowOn,
}

/// Audio hook message (nothing in bd_audio reads it yet).
#[allow(dead_code)]
#[derive(Message, Clone, Copy, Debug)]
pub struct TrackingCue {
    pub kind: TrackingCueKind,
    pub player: PlayerId,
    pub other: Option<PlayerId>,
    pub pos: Vec3,
    /// The local player is the actor or the target.
    pub local: bool,
}

/// One rope from `hooker` to `victim` right now (for a reel loop: pitch by `closing`).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct HookReel {
    pub hooker: PlayerId,
    pub victim: PlayerId,
    /// Midpoint of the rope.
    pub pos: Vec3,
    /// Hooker's closing speed toward the victim (m/s).
    pub closing: f32,
    pub local: bool,
}

/// Every live rope on a player this frame (rebuilt each frame; no allocation after warm-up).
#[derive(Resource, Default, Debug)]
pub struct HookReels(pub Vec<HookReel>);

// ------------------------------------------------------------------------------------ state

const TRAIL_N: usize = 40;
const MAX_TRAILS: usize = 16;

#[derive(Clone, Copy)]
struct Trail {
    id: PlayerId,
    /// (chest, time, intensity) ring; newest at `head`.
    pts: [(Vec3, f32, f32); TRAIL_N],
    head: usize,
    len: usize,
    last: f32,
    /// Last frame this player was seen (stale trails are recycled).
    seen: f32,
}

impl Trail {
    fn new(id: PlayerId) -> Self {
        Self { id, pts: [(Vec3::ZERO, 0.0, 0.0); TRAIL_N], head: 0, len: 0, last: -1.0, seen: 0.0 }
    }
    fn push(&mut self, p: Vec3, t: f32, k: f32) {
        self.head = (self.head + 1) % TRAIL_N;
        self.pts[self.head] = (p, t, k);
        self.len = (self.len + 1).min(TRAIL_N);
        self.last = t;
    }
    /// The i-th newest sample.
    fn get(&self, i: usize) -> (Vec3, f32, f32) {
        self.pts[(self.head + TRAIL_N - i) % TRAIL_N]
    }
}

#[derive(Clone, Copy, Debug)]
struct Pulse {
    player: PlayerId,
    pos: Vec3,
    t: f32,
    color: [f32; 3],
    scale: f32,
}

#[derive(Clone, Copy, Debug)]
struct HookLink {
    hooker: PlayerId,
    hand: u8,
    victim: PlayerId,
    t: f32,
}

#[derive(Resource)]
pub struct TrackingState {
    now: f32,
    attacker: Option<(PlayerId, f32)>,
    compass_fade: f32,
    pulses: Vec<Pulse>,
    last_pulse: Vec<(PlayerId, f32)>,
    trails: Vec<Trail>,
    links: Vec<HookLink>,
    glow: f32,
    scratch: Vec<Vec3>,
}

impl Default for TrackingState {
    fn default() -> Self {
        Self {
            now: 0.0,
            attacker: None,
            compass_fade: 0.0,
            pulses: Vec::with_capacity(32),
            last_pulse: Vec::with_capacity(16),
            trails: Vec::with_capacity(MAX_TRAILS),
            links: Vec::with_capacity(8),
            glow: 0.0,
            scratch: Vec::with_capacity(64),
        }
    }
}

impl TrackingState {
    /// Queue a reveal ping at `player` (rate-limited per player; never the local player).
    fn pulse(&mut self, player: PlayerId, local: Option<PlayerId>, views: &WorldViews, t: &TrackingTuning, scale: f32) {
        if !t.reveal || Some(player) == local {
            return;
        }
        let Some(v) = views.player(player).filter(|v| v.alive()) else { return };
        let now = self.now;
        match self.last_pulse.iter_mut().find(|(id, _)| *id == player) {
            Some((_, at)) if now - *at < t.reveal_cooldown => return,
            Some((_, at)) => *at = now,
            None => self.last_pulse.push((player, now)),
        }
        if self.pulses.len() >= 32 {
            self.pulses.remove(0);
        }
        self.pulses.push(Pulse { player, pos: chest(v), t: 0.0, color: super::corps_rgb(v.corps), scale });
    }
}

fn chest(v: &PlayerView) -> Vec3 {
    v.pos + Vec3::Y * (1.0 * v.size)
}

/// A remote player's rope hand (same place the body renderer starts its ropes).
fn hand_of(v: &PlayerView, hand: u8) -> Vec3 {
    let (eye, look, _) = body_frames(v);
    eye + look * Vec3::new(if hand == 0 { -0.35 } else { 0.35 } * v.size, -0.3 * v.size, -0.2)
}

/// The sim-sized hull of a rendered player (bd_core types).
fn view_hull(v: &PlayerView) -> Aabb {
    let s = v.size;
    let h = if v.has(pflags::CROUCHED) { 1.1 } else { 1.8 } * s;
    let p = super::gv(v.pos);
    Aabb { min: p - bd_core::Vec3::new(0.4 * s, 0.0, 0.4 * s), max: p + bd_core::Vec3::new(0.4 * s, h, 0.4 * s) }
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let k = ((x - e0) / (e1 - e0).max(1e-4)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

// ------------------------------------------------------------------------------------ plugin

pub struct TrackingPlugin;

impl Plugin for TrackingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TrackingTuningRes(load_tuning()))
            .init_resource::<TrackingState>()
            .init_resource::<HookReels>()
            .add_message::<TrackingCue>()
            .add_systems(OnExit(AppState::InGame), reset)
            .add_systems(Update, (hot_reload_tuning, tracking_events.in_set(PresentSet::Events)))
            .add_systems(Update, weapon_glow.in_set(FrameSet::Present).after(PresentSet::Sync))
            .add_systems(PostUpdate, draw_tracking.after(CameraUpdateSet).after(HandsPublishSet).before(TransformSystems::Propagate));
        scenario::build(app);
    }
}

fn reset(mut state: ResMut<TrackingState>, mut reels: ResMut<HookReels>) {
    let s = &mut *state;
    s.attacker = None;
    s.compass_fade = 0.0;
    s.pulses.clear();
    s.last_pulse.clear();
    s.trails.clear();
    s.links.clear();
    s.glow = 0.0;
    reels.0.clear();
}

// ------------------------------------------------------------------------------------ events

#[allow(clippy::too_many_arguments)]
fn tracking_events(
    time: Res<Time<Real>>,
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    tuning: Res<TrackingTuningRes>,
    mut state: ResMut<TrackingState>,
    mut sfx: MessageWriter<Sfx3d>,
    mut cues: MessageWriter<TrackingCue>,
    mut burst: MessageWriter<BurstFx>,
) {
    state.now += time.delta_secs();
    let Some(active) = active else {
        events.clear();
        return;
    };
    let t = &tuning.0;
    let local = active.local;
    let now = state.now;
    for ev in events.read() {
        match &ev.event {
            GameEvent::Hit { target, source, knockback, impact, .. } => {
                if Some(*target) == local
                    && let Some(s) = source.filter(|s| Some(*s) != local)
                {
                    state.attacker = Some((s, now));
                }
                if impact.heavy || *knockback >= t.reveal_heavy_kb {
                    state.pulse(*target, local, &views, t, 1.0);
                }
            }
            GameEvent::ShotFired { owner, archetype, .. } if *archetype != GunArchetype::Ring => {
                state.pulse(*owner, local, &views, t, 0.9);
            }
            GameEvent::ConstructSpawned { owner, recipe, .. } if recipe.scale >= t.reveal_summon_scale => {
                state.pulse(*owner, local, &views, t, 1.3);
            }
            GameEvent::Charge { player, kind: ChargeEvent::Start, .. } => {
                state.pulse(*player, local, &views, t, 1.15);
            }
            GameEvent::Ko { victim, .. } | GameEvent::Respawn { player: victim, .. } => {
                state.links.retain(|l| l.hooker != *victim && l.victim != *victim);
            }
            GameEvent::Assist { player, other, kind, pos } => {
                let p = super::bv(*pos);
                let involved = Some(*player) == local || (other.is_some() && *other == local);
                let color = views.player(*player).map_or([1.0; 3], |v| super::corps_rgb(v.corps));
                let cue = |kind| TrackingCue { kind, player: *player, other: *other, pos: p, local: involved };
                match kind {
                    AssistEvent::Hooked { hand } => {
                        if let Some(v) = other {
                            state.links.retain(|l| !(l.hooker == *player && l.hand == *hand));
                            state.links.push(HookLink { hooker: *player, hand: *hand, victim: *v, t: 0.0 });
                        }
                        cues.write(cue(TrackingCueKind::HookThwip));
                        sfx.write(Sfx3d::at("tether_attach", p).gain(0.95).pitch(1.3));
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::ZERO, scale: 0.6, color: Some(color) });
                    }
                    AssistEvent::HookBroken { hand, .. } => {
                        state.links.retain(|l| !(l.hooker == *player && l.hand == *hand));
                        cues.write(cue(TrackingCueKind::HookSnap));
                        sfx.write(Sfx3d::at("break_hardlight", p).gain(0.55).pitch(1.45));
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::ZERO, scale: 0.45, color: Some(color) });
                    }
                    AssistEvent::SlamReady => {
                        state.links.retain(|l| l.hooker != *player);
                        cues.write(cue(TrackingCueKind::SlamReady));
                        sfx.write(Sfx3d::at("tether_release", p).gain(0.6).pitch(1.5));
                    }
                    AssistEvent::Slam => {
                        cues.write(cue(TrackingCueKind::Slam));
                        sfx.write(Sfx3d::at("break_hardlight", p).gain(1.0).pitch(0.55));
                        burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::ZERO, scale: 1.5, color: Some(color) });
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::Y, scale: 1.5, color: Some(color) });
                    }
                    AssistEvent::Lunge => {
                        cues.write(cue(TrackingCueKind::Lunge));
                        sfx.write(Sfx3d::at("mix_whoosh_out", p).gain(0.5).pitch(1.4));
                    }
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------------------------ weapon glow

/// The local held melee weapon brightens while a swing would lunge-connect (same query as the sim).
#[allow(clippy::too_many_arguments)]
fn weapon_glow(
    time: Res<Time<Real>>,
    tuning: Res<TrackingTuningRes>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    anchor: Res<ViewAnchor>,
    look: Res<LookAngles>,
    arena: Option<Res<ArenaWorld>>,
    ents: Res<ConstructEntities>,
    mut state: ResMut<TrackingState>,
    mut q: Query<&mut ConstructVisual>,
    mut cues: MessageWriter<TrackingCue>,
) {
    let t = &tuning.0;
    let dt = time.delta_secs().min(0.1);
    let Some(active) = active else {
        state.glow = 0.0;
        return;
    };
    let Some(local) = active.local else { return };
    let held = active.world.constructs.held_by(local);
    let mut want = 0.0;
    if t.glow
        && let Some(c) = held
        && let Some(me) = views.player(local).filter(|v| v.alive())
        && let Some(reach) = bd_sim::melee_assist::reach_of(&active.world.cfg.tuning.assist.lunge, c, me.size)
    {
        let eye = super::gv(anchor.eye);
        let fwd = super::gv(look.forward());
        let world = arena.as_ref().map(|a| a.read());
        let cands = views.players.iter().filter(|v| !v.is_local && v.alive() && !v.has(pflags::SHIELD)).map(|v| (v.id, view_hull(v)));
        let clear = |a: bd_core::Vec3, b: bd_core::Vec3| {
            world.as_ref().is_none_or(|w| {
                let d = b - a;
                let len = d.length();
                len < 1e-3 || bd_core::voxel::raycast(&**w, a, d / len, (len - 0.3).max(0.0)).is_none()
            })
        };
        if bd_sim::melee_assist::swing_target(eye, fwd, &reach, cands, clear).is_some() {
            want = 1.0;
        }
    }
    let was = state.glow;
    state.glow += (want - state.glow) * (1.0 - (-t.glow_rate * dt).exp());
    if was < 0.5 && state.glow >= 0.5 {
        cues.write(TrackingCue { kind: TrackingCueKind::GlowOn, player: local, other: None, pos: anchor.eye, local: true });
    }
    if state.glow < 0.01 {
        return;
    }
    if let Some(c) = held
        && let Some(e) = ents.0.get(&c.id)
        && let Ok(mut cv) = q.get_mut(*e)
    {
        let shimmer = 1.0 - t.glow_shimmer * 0.5 * (1.0 + (state.now * std::f32::consts::TAU * t.glow_shimmer_hz).sin());
        cv.flash = cv.flash.max(state.glow * t.glow_flash * shimmer);
    }
}

// ------------------------------------------------------------------------------------ drawing

/// Vertical field of view (rad) and aspect of the gameplay camera.
fn fov_of(p: &Projection) -> (f32, f32) {
    match p {
        Projection::Perspective(pp) => (pp.fov, pp.aspect_ratio.max(0.1)),
        _ => (1.2, 16.0 / 9.0),
    }
}

/// Is world point `p` inside `margin` of the view (in front of the camera)?
fn on_screen(cam: &Transform, fov: (f32, f32), p: Vec3, margin: f32) -> bool {
    let c = cam.rotation.inverse() * (p - cam.translation);
    if c.z > -0.1 {
        return false;
    }
    let ty = (fov.0 * 0.5).tan();
    let tx = ty * fov.1;
    let (x, y) = (c.x / -c.z / tx, c.y / -c.z / ty);
    x.abs() < margin && y.abs() < margin
}

#[allow(clippy::too_many_arguments)]
fn draw_tracking(
    time: Res<Time<Real>>,
    tuning: Res<TrackingTuningRes>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    mode: Res<ViewMode>,
    hands: Res<ViewmodelHands>,
    arena: Option<Res<ArenaWorld>>,
    cams: Query<(&Transform, &Projection), With<PlayerCamera>>,
    mut state: ResMut<TrackingState>,
    mut fx: ResMut<FxParticles>,
    mut reels: ResMut<HookReels>,
    mut cues: MessageWriter<TrackingCue>,
) {
    reels.0.clear();
    let Some(active) = active else { return };
    let Ok((cam, proj)) = cams.single() else { return };
    let t = &tuning.0;
    let dt = time.delta_secs().min(0.1);
    let now = state.now;
    let eye = cam.translation;
    let rot = cam.rotation;
    let fov = fov_of(proj);
    let calm = comfort.flashes <= 0.01 || comfort.vignette > 0.3;
    let third = mode.third_person();
    let local = active.local;
    let me = local.and_then(|id| views.player(id)).filter(|v| v.alive());
    let st = &mut *state;
    let fx = &mut *fx;

    // ---- ring compass (first person).
    let mut want = 0.0;
    let mut target: Option<&PlayerView> = None;
    if t.compass
        && hands.active
        && !third
        && let Some(me) = me
    {
        let attacker =
            st.attacker.filter(|(_, at)| now - at <= t.compass_attacker_secs).and_then(|(id, _)| views.player(id)).filter(|v| v.alive());
        target = attacker.or_else(|| {
            views
                .players
                .iter()
                .filter(|v| !v.is_local && v.alive())
                .min_by(|a, b| a.pos.distance_squared(me.pos).total_cmp(&b.pos.distance_squared(me.pos)))
        });
        if let Some(tv) = target {
            let c = chest(tv);
            let visible = on_screen(cam, fov, c, t.compass_screen_margin)
                && arena.as_ref().is_none_or(|a| {
                    let w = a.read();
                    let d = c - eye;
                    let len = d.length();
                    len < 1e-3 || bd_core::voxel::raycast(&*w, super::gv(eye), super::gv(d / len), (len - 0.5).max(0.0)).is_none()
                });
            want = if visible { 0.0 } else { 1.0 };
        }
    }
    st.compass_fade += (want - st.compass_fade) * (1.0 - (-t.compass_fade_rate * dt).exp());
    if let (Some(tv), Some(me)) = (target, me)
        && st.compass_fade > 0.01
    {
        let ring = hands.world[1];
        let c = chest(tv);
        let dist = c.distance(me.pos);
        let far = smooth(t.compass_near, t.compass_far, dist);
        let len = t.compass_len_min + (t.compass_len_max - t.compass_len_min) * far;
        let d = (rot.inverse() * (c - ring)).normalize_or(Vec3::NEG_Z);
        let dc = if -d.z < t.compass_min_forward {
            let lat = Vec2::new(d.x, d.y).normalize_or(Vec2::X);
            let k = (1.0 - t.compass_min_forward * t.compass_min_forward).max(0.0).sqrt();
            Vec3::new(lat.x * k, lat.y * k, -t.compass_min_forward)
        } else {
            d
        };
        let dir = rot * dc;
        let (side, up) = (rot * Vec3::X, rot * Vec3::Y);
        let wob = t.compass_wobble * if calm { 0.5 } else { 1.0 };
        let ph = now * std::f32::consts::TAU * t.compass_wobble_hz;
        let n = t.compass_segments.clamp(2, 24) as usize;
        st.scratch.clear();
        for i in 0..=n {
            let s = i as f32 / n as f32;
            let base = ring + dir * (len * s) - up * (0.12 * len * s * s);
            let w = (side * (ph + s * 7.0).sin() + up * (ph * 1.3 + s * 5.0).cos() * 0.6) * (wob * s);
            st.scratch.push(base + w);
        }
        let k = st.compass_fade;
        let e = t.compass_emissive_min + (t.compass_emissive_max - t.compass_emissive_min) * far;
        let own = super::corps_rgb(me.corps);
        let theirs = super::corps_rgb(tv.corps);
        tapered_stroke(fx, &st.scratch, t.compass_thick * (0.5 + 0.5 * k), t.compass_tip_thick * k, own, theirs, e * 0.7 * k, e * k, 1.0);
    }

    // ---- lantern beacons.
    if t.beacon {
        for v in views.players.iter().filter(|v| !v.is_local && v.alive()) {
            let d = v.pos.distance(eye);
            let k = smooth(t.beacon_min_dist, t.beacon_min_dist + t.beacon_fade_dist, d)
                * (1.0 - smooth(t.beacon_max_dist - 60.0, t.beacon_max_dist, d));
            if k > 0.0 {
                beacon_pillar(
                    fx,
                    eye,
                    v.pos,
                    t.beacon_height,
                    super::corps_rgb(v.corps),
                    t.beacon_emissive,
                    k,
                    t.beacon_px,
                    t.beacon_pieces,
                );
            }
        }
    }

    // ---- reveal pulses (charging pings keep coming while charging).
    if t.reveal {
        for v in views.players.iter().filter(|v| !v.is_local && v.alive() && v.has(pflags::CHARGING)) {
            let due = st.last_pulse.iter().find(|(id, _)| *id == v.id).is_none_or(|(_, at)| now - at >= t.reveal_charge_every);
            if due {
                st.pulse(v.id, local, &views, t, 1.15);
            }
        }
    }
    let life = t.reveal_secs.max(0.05);
    let n = t.reveal_segments.clamp(6, 48) as usize;
    let mut ring = [Vec3::ZERO; 48];
    for p in st.pulses.iter_mut() {
        if p.t == 0.0 {
            cues.write(TrackingCue { kind: TrackingCueKind::RevealPing, player: p.player, other: None, pos: p.pos, local: false });
        }
        p.t += dt;
        if let Some(v) = views.player(p.player) {
            p.pos = chest(v);
        }
        let u = (p.t / life).clamp(0.0, 1.0);
        let d = p.pos.distance(eye);
        if u >= 1.0 || d < t.reveal_min_dist || d > t.reveal_max_dist {
            continue;
        }
        let ease = 1.0 - (1.0 - u) * (1.0 - u);
        let r = (t.reveal_r0 + (t.reveal_r1 - t.reveal_r0) * ease) * p.scale;
        let em = t.reveal_emissive * (1.0 - u).powf(1.5) * if calm { 0.7 } else { 1.0 };
        view_ring(eye, p.pos, r, &mut ring[..n]);
        xray_polyline(fx, eye, &ring[..n], true, t.reveal_thick.max(d * t.reveal_px), p.color, em, t.reveal_near);
    }
    st.pulses.retain(|p| p.t < life);

    // ---- speed trails.
    if t.trails {
        let secs = if calm { t.trail_comfort_secs } else { t.trail_secs }.max(0.02);
        let scale = if calm { t.trail_comfort_scale } else { 1.0 };
        let step = 1.0 / t.trail_samples_per_sec.max(10.0);
        let fast: usize = views.players.iter().filter(|v| v.alive() && (!v.is_local || third) && v.vel.length() > t.trail_speed).count();
        let per = (t.trail_max_segments as usize / fast.max(1)).min(TRAIL_N - 1);
        let mut budget = t.trail_max_segments as usize;
        for v in views.players.iter().filter(|v| v.alive() && (!v.is_local || third)) {
            let i = match st.trails.iter().position(|tr| tr.id == v.id) {
                Some(i) => i,
                None => {
                    if st.trails.len() >= MAX_TRAILS {
                        // Recycle the stalest.
                        let (k, _) =
                            st.trails.iter().enumerate().min_by(|a, b| a.1.seen.total_cmp(&b.1.seen)).unwrap_or((0, &st.trails[0]));
                        st.trails[k] = Trail::new(v.id);
                        k
                    } else {
                        st.trails.push(Trail::new(v.id));
                        st.trails.len() - 1
                    }
                }
            };
            let tr = &mut st.trails[i];
            tr.seen = now;
            let head = chest(v);
            let speed = v.vel.length();
            let k = smooth(t.trail_speed, t.trail_full_speed, speed);
            if now - tr.last >= step {
                tr.push(head, now, k);
            }
            if k <= 0.0 && tr.len > 0 && now - tr.get(0).1 > secs {
                continue;
            }
            // Live head → samples, newest first, while young enough.
            let color = super::corps_rgb(v.corps);
            let mut prev = (head, now, k);
            let mut drawn = 0;
            for j in 0..tr.len {
                if drawn >= per || budget == 0 {
                    break;
                }
                let s = tr.get(j);
                let age = now - s.1;
                if age > secs {
                    break;
                }
                let fade = 1.0 - age / secs;
                let kk = prev.2.max(s.2);
                if kk > 0.01 && prev.0.distance_squared(s.0) < 36.0 {
                    let th = t.trail_thick * fade * kk * scale * v.size;
                    let em = t.trail_emissive * fade.powf(1.2) * kk * scale;
                    if fx.immediate_segment(prev.0, s.0, th, color, em, PFlags::PLAIN) {
                        drawn += 1;
                        budget = budget.saturating_sub(1);
                    }
                }
                prev = s;
            }
        }
    }

    // ---- hook-and-strike ropes, wraps, the victim's tug.
    st.links.retain_mut(|l| {
        l.t += dt;
        let (Some(h), Some(v)) = (views.player(l.hooker), views.player(l.victim)) else { return false };
        h.alive() && v.alive() && h.hooks[l.hand as usize & 1].is_some() && l.t < t.link_timeout
    });
    let fp = !third && hands.active;
    for l in &st.links {
        let (Some(h), Some(v)) = (views.player(l.hooker), views.player(l.victim)) else { continue };
        let start = if h.is_local && fp { hands.world[l.hand as usize & 1] } else { hand_of(h, l.hand) };
        let victim_fp = v.is_local && !third;
        let end = if victim_fp { eye + rot * Vec3::new(0.0, -0.45, -0.3) } else { chest(v) };
        let color = super::corps_rgb(h.corps);
        let hot = [0.5 + color[0] * 0.5, 0.5 + color[1] * 0.5, 0.5 + color[2] * 0.5];
        // A bright hard-light core rope (the base tether rope renders too).
        st.scratch.clear();
        let segs = 10;
        let len = start.distance(end);
        for i in 0..=segs {
            let s = i as f32 / segs as f32;
            let sag = (len * 0.02).min(0.6) * 4.0 * s * (1.0 - s);
            st.scratch.push(start.lerp(end, s) - Vec3::Y * sag);
        }
        tapered_stroke(
            fx,
            &st.scratch,
            t.hook_rope_thick * 0.5,
            t.hook_rope_thick,
            hot,
            color,
            t.hook_rope_emissive,
            t.hook_rope_emissive * 1.2,
            50.0,
        );
        // Binding wraps on the victim (not from inside their own head).
        if !victim_fp {
            let spin = now * t.hook_wrap_spin;
            let r = t.hook_wrap_radius * v.size;
            for (j, dy) in [-0.2f32, 0.15].iter().enumerate() {
                let c = chest(v) + Vec3::Y * (dy * v.size);
                for k in 0..8 {
                    if k % 2 == 1 {
                        continue;
                    }
                    let a0 = spin * if j == 0 { 1.0 } else { -1.0 } + k as f32 * std::f32::consts::TAU / 8.0;
                    let a1 = a0 + std::f32::consts::TAU / 8.0;
                    let p0 = c + Vec3::new(a0.cos(), 0.0, a0.sin()) * r;
                    let p1 = c + Vec3::new(a1.cos(), 0.0, a1.sin()) * r;
                    fx.immediate_segment(p0, p1, t.hook_rope_thick * 0.8, hot, t.hook_wrap_emissive, PFlags::PLAIN);
                }
            }
        }
        let closing = (h.vel - v.vel).dot((v.pos - h.pos).normalize_or_zero());
        reels.0.push(HookReel { hooker: h.id, victim: v.id, pos: (start + end) * 0.5, closing, local: h.is_local || v.is_local });
        // The victim's tug: a short arc at the screen edge toward the hooker.
        if t.tug && victim_fp {
            let hc = rot.inverse() * (start - eye);
            let v2 = Vec2::new(hc.x, hc.y);
            let front = -hc.z > 0.0 && on_screen(cam, fov, start, 0.6);
            if v2.length() > 1e-3 && !front {
                let d2 = v2.normalize();
                let ty = (fov.0 * 0.5).tan();
                let tx = ty * fov.1;
                let s = t.tug_edge / (d2.x.abs() / tx).max(d2.y.abs() / ty).max(1e-3);
                let p2 = d2 * s;
                let tan2 = Vec2::new(-d2.y, d2.x);
                let pulse = 0.6 + 0.4 * (now * std::f32::consts::TAU * t.tug_hz).sin();
                let dd = t.tug_dist;
                let at = |u: f32| {
                    let q = p2 + tan2 * ((u - 0.5) * t.tug_len / dd) - d2 * (0.3 * t.tug_len / dd * (1.0 - (2.0 * u - 1.0).powi(2)));
                    eye + rot * Vec3::new(q.x * dd, q.y * dd, -dd)
                };
                for k in 0..4 {
                    let (u0, u1) = (k as f32 / 4.0, (k + 1) as f32 / 4.0);
                    fx.immediate_segment(at(u0), at(u1), t.tug_thick, hot, t.tug_emissive * pulse, PFlags::PLAIN);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------ scenario

/// `--practice --scenario tracking --bots 3 --tracking-shot <shot>`: stages one tracking feature in
/// front of the camera for screenshots (bots stand down; the scenario drives them):
/// `filament` (an enemy far behind you), `beacon` (an enemy ~85 m ahead), `reveal` (an enemy
/// behind terrain firing a gun + one charging), `trails` (three enemies circling fast),
/// `hook` (you hook an enemy with a sword, reel in and slam), `victim` (an enemy hooks you).
pub mod scenario {
    use super::*;
    use crate::input::InputOverride;
    use crate::tick::{SimSet, SimTick};
    use bd_core::input::{PlayerInput, buttons, look_dir};
    use bd_core::mechanics::{Element, Form, Mechanic};

    #[derive(Resource, Clone, Debug)]
    pub struct TrackingScenario {
        pub shot: String,
        placed: bool,
        t: f32,
        yaw: f32,
        /// Circle centre for `trails`.
        center: bd_core::Vec3,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "tracking") {
            return;
        }
        let shot = args.windows(2).find(|w| w[0] == "--tracking-shot").map(|w| w[1].clone()).unwrap_or_else(|| "hook".into());
        info!("tracking scenario: {shot}");
        app.insert_resource(TrackingScenario { shot, placed: false, t: 0.0, yaw: 0.0, center: bd_core::Vec3::ZERO })
            .add_systems(Update, (place, script).chain().run_if(resource_exists::<ActiveWorld>))
            .add_systems(SimTick, drive.in_set(SimSet::Input).run_if(resource_exists::<ActiveWorld>));
    }

    fn ground_at(w: &bd_sim::World, x: f32, z: f32, near_y: f32, span: f32) -> Option<bd_core::Vec3> {
        let from = bd_core::Vec3::new(x, near_y + span, z);
        let hit = bd_core::voxel::raycast(&w.voxels, from, bd_core::Vec3::NEG_Y, span * 2.0)?;
        Some(bd_core::Vec3::new(x, hit.point.y + 0.05, z))
    }

    fn put(w: &mut bd_sim::World, id: u8, at: bd_core::Vec3) {
        if let Some(p) = w.player_mut(PlayerId(id)) {
            p.mv.pos = at;
            p.mv.vel = bd_core::Vec3::ZERO;
            p.spawn_shield = 0.0;
            p.damage_pct = 40.0;
        }
    }

    fn give(w: &mut bd_sim::World, id: u8, name: &str, form: Form, m: &[Mechanic]) {
        let Some(p) = w.player(PlayerId(id)) else { return };
        let eye = p.eye(w.params());
        let look = p.look();
        let scale = bd_sim::constructs::form_spec(form).default_scale;
        let _ = w.cast(PlayerId(id), bd_sim::bots::recipe(name, form, m, Element::Light, scale), None, bd_sim::Aim::new(eye, look));
    }

    fn place(mut sc: ResMut<TrackingScenario>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
        sc.t += time.delta_secs();
        if sc.placed || sc.t < 0.6 {
            return;
        }
        let Some(me) = active.local_player().filter(|p| p.alive).map(|p| p.mv.pos) else { return };
        active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Infinite;
        let eye = me + bd_core::Vec3::Y * 1.6;
        // Pick a view direction that works for the shot.
        let mut chosen: Option<(f32, Vec<(u8, bd_core::Vec3)>)> = None;
        for step in 0..48 {
            let yaw = step as f32 * std::f32::consts::TAU / 48.0;
            let dir = look_dir(yaw, 0.0);
            let side = bd_core::Vec3::new(-dir.z, 0.0, dir.x);
            let w = &active.world;
            let spots: Option<Vec<(u8, bd_core::Vec3)>> = match sc.shot.as_str() {
                "filament" => {
                    ground_at(w, me.x - dir.x * 30.0 + side.x * 12.0, me.z - dir.z * 30.0 + side.z * 12.0, me.y, 30.0).map(|g| vec![(1, g)])
                }
                "beacon" => ground_at(w, me.x + dir.x * 85.0, me.z + dir.z * 85.0, me.y, 60.0).map(|g| vec![(1, g)]),
                "reveal" => {
                    // Walk the view until terrain blocks it, then stand the enemy behind it.
                    let mut out = None;
                    for d in (10..60).step_by(2) {
                        let at = me + dir * d as f32;
                        let Some(g) = ground_at(w, at.x, at.z, me.y, 40.0) else { continue };
                        let chest = g + bd_core::Vec3::Y;
                        let to = chest - eye;
                        let len = to.length();
                        let blocked = bd_core::voxel::raycast(&w.voxels, eye, to / len, len - 0.6).is_some_and(|h| h.t > 4.0);
                        if blocked && (g.y - me.y).abs() < 12.0 {
                            let g2 = ground_at(w, at.x + side.x * 5.0, at.z + side.z * 5.0, me.y, 40.0).unwrap_or(g);
                            out = Some(vec![(1, g), (2, g2)]);
                            break;
                        }
                    }
                    out
                }
                "trails" => {
                    let c = me + dir * 24.0;
                    let ok = (0..8).all(|k| {
                        let a = k as f32 * std::f32::consts::TAU / 8.0;
                        ground_at(w, c.x + a.cos() * 9.0, c.z + a.sin() * 9.0, me.y, 8.0).is_some()
                    });
                    ok.then(|| {
                        let c = ground_at(w, c.x, c.z, me.y, 8.0).unwrap_or(c);
                        vec![(1, c + bd_core::Vec3::X * 9.0), (2, c - bd_core::Vec3::X * 9.0), (3, c + bd_core::Vec3::Z * 9.0)]
                    })
                }
                _ => {
                    // hook / victim: flat ground 16 m ahead.
                    let at = me + dir * 16.0;
                    ground_at(w, at.x, at.z, me.y, 3.0).filter(|g| (g.y - me.y).abs() < 1.5).map(|g| vec![(1, g)])
                }
            };
            if let Some(s) = spots {
                chosen = Some((yaw, s));
                break;
            }
        }
        let Some((yaw, spots)) = chosen else {
            if sc.t > 8.0 {
                warn!("tracking scenario: no layout found around the spawn");
                sc.placed = true;
            }
            return;
        };
        sc.yaw = yaw;
        active.bots.clear();
        let w = &mut active.world;
        // Everyone not in the shot goes far away (beacons of extras would clutter).
        let ids: Vec<u8> = w.players.iter().map(|p| p.id.0).filter(|id| *id != 0).collect();
        for id in ids {
            if !spots.iter().any(|(s, _)| *s == id) {
                if let Some(p) = w.player_mut(PlayerId(id)) {
                    p.alive = false;
                    p.respawn_timer = 1e6;
                }
            }
        }
        for (id, g) in &spots {
            put(w, *id, *g);
        }
        if sc.shot == "trails" {
            let c = spots.iter().map(|s| s.1).fold(bd_core::Vec3::ZERO, |a, b| a + b) / spots.len() as f32;
            sc.center = c;
        }
        match sc.shot.as_str() {
            "reveal" => give(w, 1, "gun", Form::Gun, &[Mechanic::Shoots]),
            "hook" => give(w, 0, "sword", Form::Sword, &[]),
            "victim" => give(w, 1, "sword", Form::Sword, &[]),
            _ => {}
        }
        look.yaw = match sc.shot.as_str() {
            "victim" => yaw + 1.3,
            _ => yaw,
        };
        look.pitch = if sc.shot == "beacon" { 0.05 } else { -0.05 };
        sc.placed = true;
        info!("tracking scenario: placed {:?} at yaw {:.0}°", spots, yaw.to_degrees());
    }

    /// Local player's scripted input (hook shot) + the camera following the target.
    fn script(sc: Res<TrackingScenario>, active: Res<ActiveWorld>, mut look: ResMut<LookAngles>, mut ov: ResMut<InputOverride>) {
        if !sc.placed || sc.shot != "hook" {
            return;
        }
        let (Some(me), Some(b)) = (active.local_player(), active.world.player(PlayerId(1))) else { return };
        let eye = me.eye(active.world.params());
        let d = (b.hull(active.world.params()).center() - eye).normalize_or_zero();
        let t = sc.t - 0.6;
        ov.active = true;
        ov.held = 0;
        ov.move_x = 0;
        ov.move_z = 0;
        if t > 0.8 {
            look.yaw = (-d.x).atan2(-d.z);
            look.pitch = d.y.clamp(-1.0, 1.0).asin();
        }
        let hooked = me.mv.hooks[1].phase != bd_sim::movement::HookPhase::Idle;
        if (1.2..3.4).contains(&t) && (hooked || t < 1.3) {
            ov.held |= buttons::TETHER_R;
            if t > 1.35 {
                ov.held |= buttons::REEL;
            }
        }
        if me.assist.slam_t > 0.0 || (3.4..3.5).contains(&t) {
            ov.held |= buttons::PRIMARY;
        }
    }

    fn drive(sc: Res<TrackingScenario>, mut active: ResMut<ActiveWorld>) {
        if !sc.placed {
            return;
        }
        let tick = active.world.tick;
        let dt = bd_core::DT;
        match sc.shot.as_str() {
            "reveal" => {
                // Bot 1 fires its gun at the sky now and then; bot 2 charges.
                let fire = (tick / 40) % 3 == 0;
                let b = if fire { buttons::PRIMARY } else { 0 };
                active
                    .remote_inputs
                    .push((PlayerId(1), PlayerInput { seq: tick as u32, yaw: sc.yaw, pitch: 0.9, buttons: b, ..Default::default() }));
                active
                    .remote_inputs
                    .push((PlayerId(2), PlayerInput { seq: tick as u32, yaw: sc.yaw, buttons: buttons::CHARGE, ..Default::default() }));
            }
            "trails" => {
                // Three enemies circle at ~24 m/s.
                let c = sc.center;
                let ph = tick as f32 * dt * 2.4;
                for (k, id) in [1u8, 2, 3].iter().enumerate() {
                    let a = ph + k as f32 * std::f32::consts::TAU / 3.0;
                    let r = 9.0;
                    if let Some(p) = active.world.player_mut(PlayerId(*id)) {
                        let want = c + bd_core::Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                        let tangent = bd_core::Vec3::new(-a.sin(), 0.0, a.cos()) * (r * 2.4);
                        p.mv.vel = tangent + (want - p.mv.pos) * 4.0;
                        p.mv.vel.y = 0.0;
                    }
                    active
                        .remote_inputs
                        .push((PlayerId(*id), PlayerInput { seq: tick as u32, yaw: a + std::f32::consts::PI, ..Default::default() }));
                }
            }
            "victim" => {
                // Bot 1 hooks the local player and reels in.
                let Some(me) = active.local_player().map(|p| p.hull(active.world.params()).center()) else { return };
                let Some(b) = active.world.player(PlayerId(1)) else { return };
                let eye = b.eye(active.world.params());
                let d = (me - eye).normalize_or_zero();
                let hooked = b.mv.hooks[1].phase != bd_sim::movement::HookPhase::Idle;
                let t = tick % 900;
                let btn = if t > 120 && (hooked || t < 125) { buttons::TETHER_R | buttons::REEL } else { 0 };
                active.remote_inputs.push((
                    PlayerId(1),
                    PlayerInput {
                        seq: tick as u32,
                        yaw: (-d.x).atan2(-d.z),
                        pitch: d.y.clamp(-1.0, 1.0).asin(),
                        buttons: btn,
                        ..Default::default()
                    },
                ));
            }
            _ => {}
        }
    }
}
