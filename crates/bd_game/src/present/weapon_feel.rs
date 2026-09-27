//! **WEAPON FEEL** (presentation): what each weapon class (bd_sim `constructs::weapon_class`) sounds,
//! looks and feels like — per class, from data (`assets/tuning/weapons.ron` → `feel`, hot-reloaded;
//! never read by the sim).
//!
//! | class | swing | connect | trail | camera (yours) | body (3P) |
//! |---|---|---|---|---|---|
//! | light | high thin swish | crit chime on the string's finisher | one thin, bright, short streak | tiny | a flick, no lean |
//! | medium | mid whoosh | — | a medium streak | small | a little lean |
//! | heavy | deep heaving whoosh | bone-deep crunch; charged slam: shockwave ring + ground crack | a thick, dim, long smear (4 lines) | wind-up zoom-in, strike lurch + downward shake; whiffs crack the floor | both hands on the handle, the whole body leans into it |
//! | reach | air-dart thrust | tip sweet-spot ding | a thin streak | tiny | two-handed, a lunge lean |
//! | flexible | light whoosh (higher) | tip crack | a long thin lash | tiny | — |
//! | shield | low whoosh | bash slam | — | bash lurch | shoulder in |
//! | guns | — | — | — | light guns kick snappier, heavy guns shove harder | heavy guns two-handed |
//!
//! * The swing itself (both views) is the class curve the sim sweeps
//!   (`bd_sim::constructs::held::event_curve` / `swing_rate`) — the viewmodel and the third-person
//!   item both play it at the sim's rate, so what you see is the hitbox.
//! * Telescoping poles (the Ruyi staff) and energy blades keep their own sound signatures
//!   (`present::extend`, `present::legend_sound`); this module stays out of their way.
//! * Charged heavy slams are HeavyMelee hits with a big impact: the impact-frames worker's automatic
//!   route (docs/impact/API.md) fires the anime frame for them — nothing to duplicate here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic};
use bd_core::{ConstructId, PlayerId};
use bd_fx::{ConstructVisual, FxParticles, PFlags};
use bd_sim::constructs::weapon_class::{WeaponClass, class_of};
use bd_sim::constructs::{Construct, held};
use bd_sim::events::HeldFx;
use bd_sim::GameEvent;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use serde::{Deserialize, Serialize};

use super::constructs::SimConstruct;
use super::sound::Sfx3d;
use super::{PresentSet, bv};
use crate::debris::SpawnDebris;
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::sim_bridge::{ActiveWorld, GameEvents};
use crate::state::AppState;

// ------------------------------------------------------------------------------------ tuning

/// One class's presentation (`assets/tuning/weapons.ron` → `feel.<class>`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClassFeel {
    /// Swing sound at the strike (the hitbox opening); "" = none.
    pub swish: String,
    pub swish_gain: f32,
    pub swish_pitch: f32,
    /// Extra layer on a connecting hit (under the regular hit sound); "" = none.
    pub hit: String,
    pub hit_gain: f32,
    /// Played on a crit (finisher / sweet spot); "" = none.
    pub crit: String,
    /// A strike buried in terrain; "" = none.
    pub whiff: String,
    /// Swing trail: width (m, scaled with the drawn item), how long a point lives (s), glow, and how
    /// many streaks along the blade (1 = the tip only … 4 = a solid smear).
    pub trail_width: f32,
    pub trail_secs: f32,
    pub trail_glow: f32,
    pub trail_lines: u8,
    /// Your camera (render-only; the aim never moves): FOV zoom-in while winding up (deg), FOV lurch
    /// at the strike (deg), shake at the strike / on a connect / on a whiff into the ground (trauma).
    pub windup_zoom: f32,
    pub strike_punch: f32,
    pub strike_shake: f32,
    pub hit_shake: f32,
    pub whiff_shake: f32,
    /// Both hands on it (first-person support hand, third-person left arm).
    pub two_hand: bool,
    /// Third-person body lean into the strike (rad).
    pub lean: f32,
    /// Guns: viewmodel kick multiplier.
    pub kick: f32,
}

impl Default for ClassFeel {
    fn default() -> Self {
        Self {
            swish: String::new(),
            swish_gain: 1.0,
            swish_pitch: 1.0,
            hit: String::new(),
            hit_gain: 1.0,
            crit: String::new(),
            whiff: String::new(),
            trail_width: 0.0,
            trail_secs: 0.0,
            trail_glow: 0.0,
            trail_lines: 1,
            windup_zoom: 0.0,
            strike_punch: 0.0,
            strike_shake: 0.0,
            hit_shake: 0.0,
            whiff_shake: 0.0,
            two_hand: false,
            lean: 0.0,
            kick: 1.0,
        }
    }
}

/// Every class's feel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeelTable {
    pub light: ClassFeel,
    pub medium: ClassFeel,
    pub heavy: ClassFeel,
    pub reach: ClassFeel,
    pub flexible: ClassFeel,
    pub shield: ClassFeel,
    pub light_gun: ClassFeel,
    pub heavy_gun: ClassFeel,
}

impl FeelTable {
    pub fn of(&self, c: WeaponClass) -> &ClassFeel {
        match c {
            WeaponClass::Light => &self.light,
            WeaponClass::Medium => &self.medium,
            WeaponClass::Heavy => &self.heavy,
            WeaponClass::Reach => &self.reach,
            WeaponClass::Flexible => &self.flexible,
            WeaponClass::Shield => &self.shield,
            WeaponClass::LightGun => &self.light_gun,
            WeaponClass::HeavyGun => &self.heavy_gun,
        }
    }

    /// Parse the `feel` section of weapons.ron (its sim `classes` are the combat tuning's).
    pub fn from_ron(src: &str) -> Result<Self, String> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct File {
            feel: FeelTable,
        }
        ron::from_str::<File>(src).map(|f| f.feel).map_err(|e| e.to_string())
    }
}

/// The live feel table (hot-reloaded from `assets/tuning/weapons.ron`).
#[derive(Resource, Clone, Debug, Default)]
pub struct WeaponFeel(pub FeelTable);

impl WeaponFeel {
    /// The feel of a held construct's class (`None`: not a held weapon).
    pub fn of_construct(&self, c: &Construct) -> Option<&ClassFeel> {
        (c.category == FormCategory::Held).then(|| class_of(&c.recipe)).flatten().map(|k| self.0.of(k))
    }
}

fn path() -> PathBuf {
    crate::assets_dir().join("tuning").join("weapons.ron")
}

fn load() -> FeelTable {
    match std::fs::read_to_string(path()).map_err(|e| e.to_string()).and_then(|s| FeelTable::from_ron(&s)) {
        Ok(t) => t,
        Err(e) => {
            warn!("weapon feel: {e}; no class presentation");
            FeelTable::default()
        }
    }
}

fn hot_reload(time: Res<Time<Real>>, mut feel: ResMut<WeaponFeel>, mut last: Local<(f32, Option<SystemTime>)>) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let Ok(m) = std::fs::metadata(path()).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(m),
        Some(prev) if prev != m => {
            last.1 = Some(m);
            match std::fs::read_to_string(path()).map_err(|e| e.to_string()).and_then(|s| FeelTable::from_ron(&s)) {
                Ok(t) => {
                    feel.0 = t;
                    info!("weapon feel reloaded");
                }
                Err(e) => warn!("weapon feel: {e}; keeping the previous table"),
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------------------------ state

/// A swing the presentation is following (trail window + third-person body pose).
#[derive(Clone, Debug)]
struct LiveSwing {
    id: ConstructId,
    owner: PlayerId,
    class: WeaponClass,
    curve: &'static bd_shapes::SwingCurve,
    /// Curve time (s of curve) and playback rate.
    t: f32,
    rate: f32,
    /// Trail points (age s, [tip, 3/4, 1/2, 1/4 along the item]), newest last.
    pts: Vec<(f32, [Vec3; 4])>,
}

/// Third-person body pose of a player's class swing right now (see [`body_overlay`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodySwing {
    /// Right-arm pitch (rad, 0 = hanging; π/2 = along the aim) and yaw.
    pub arm_pitch: f32,
    pub arm_yaw: f32,
    /// 0..1 left hand on the handle.
    pub two_hand: f32,
    /// Torso lean forward (rad; negative leans back during the wind-up).
    pub lean: f32,
}

#[derive(Resource, Default)]
pub struct WeaponFeelState {
    swings: Vec<LiveSwing>,
    bodies: HashMap<PlayerId, BodySwing>,
    rings: Vec<GroundRing>,
}

impl WeaponFeelState {
    /// The third-person class swing pose of `player`, if one is playing.
    pub fn body(&self, player: PlayerId) -> Option<BodySwing> {
        self.bodies.get(&player).copied()
    }
}

// ------------------------------------------------------------------------------------ plugin

pub struct WeaponFeelPlugin;

impl Plugin for WeaponFeelPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Sfx3d>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<SpawnDebris>()
            .insert_resource(WeaponFeel(load()))
            .init_resource::<WeaponFeelState>()
            .add_systems(Update, hot_reload)
            .add_systems(Update, (feel_events, advance).chain().in_set(PresentSet::Events))
            .add_systems(
                PostUpdate,
                draw_trails.after(crate::viewmodel::HandsPublishSet).before(TransformSystems::Propagate).before(bd_fx::FxSet::Emit),
            )
            .add_systems(OnExit(AppState::InGame), |mut st: ResMut<WeaponFeelState>| {
                st.swings.clear();
                st.bodies.clear();
                st.rings.clear();
            });
    }
}

/// Items with their own sound signature (telescoping poles, energy blades) keep it.
fn own_signature(c: &Construct) -> bool {
    c.has(Mechanic::Extends) || super::legend_sound::energy_blade(c)
}

fn near(eye: Vec3, p: Vec3, range: f32) -> f32 {
    (1.0 - eye.distance(p) / range).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn feel_events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    feel: Res<WeaponFeel>,
    mut st: ResMut<WeaponFeelState>,
    mut out: (MessageWriter<Sfx3d>, MessageWriter<CameraShakeEvent>, MessageWriter<FovPunch>, MessageWriter<SpawnDebris>),
    cam: Query<&GlobalTransform, With<crate::camera::PlayerCamera>>,
    mut seed: Local<u32>,
) {
    let (sfx, shake, fov, debris) = (&mut out.0, &mut out.1, &mut out.2, &mut out.3);
    let Some(active) = active else {
        events.clear();
        return;
    };
    let eye = cam.single().map(|g| g.translation()).unwrap_or(Vec3::splat(1e6));
    let tuning = &active.world.cfg.tuning;
    for ev in events.read() {
        match &ev.event {
            GameEvent::HeldFx { id, owner, fx, pos, dir, strength } => {
                let Some(c) = active.world.constructs.get(*id) else { continue };
                let Some(class) = (c.category == FormCategory::Held).then(|| class_of(&c.recipe)).flatten() else { continue };
                let f = feel.0.of(class);
                let p = bv(*pos);
                let mine = active.local == Some(*owner);
                match fx {
                    HeldFx::SwingStart | HeldFx::ChargeStart | HeldFx::Lunge | HeldFx::Uppercut => {
                        // Follow the swing (trail window, third-person body).
                        if let Some((curve, _, _)) = held::event_curve(c, *fx, *strength, tuning) {
                            let heavy_release = *fx == HeldFx::SwingStart && *strength >= 1.9;
                            let t0 = if heavy_release { curve.hold_at } else { 0.0 };
                            let rate = held::swing_rate(c, tuning);
                            st.swings.retain(|s| s.id != *id);
                            st.swings.push(LiveSwing { id: *id, owner: *owner, class, curve, t: t0, rate, pts: Vec::with_capacity(48) });
                        }
                        if mine && *fx != HeldFx::Lunge && f.windup_zoom > 0.0 && !class.is_gun() {
                            fov.write(FovPunch { degrees: -f.windup_zoom * if *strength >= 1.9 || *fx == HeldFx::ChargeStart { 1.4 } else { 1.0 } });
                        }
                    }
                    HeldFx::SwingActive => {
                        if !f.swish.is_empty() && !own_signature(c) {
                            let heavy = *strength >= 1.9;
                            sfx.write(Sfx3d::at(f.swish.clone(), p).gain(f.swish_gain * if heavy { 1.25 } else { 1.0 }).pitch(f.swish_pitch * if heavy { 0.92 } else { 1.0 }));
                        }
                        if mine {
                            if f.strike_punch > 0.0 {
                                fov.write(FovPunch { degrees: f.strike_punch });
                            }
                            if f.strike_shake > 0.0 {
                                shake.write(CameraShakeEvent { trauma: f.strike_shake, dir: Vec3::NEG_Y });
                            }
                        }
                    }
                    HeldFx::SwingImpact if *strength >= 1.9 => {
                        // A heavy strike buried in the floor: crack, grit, dust.
                        if !f.whiff.is_empty() {
                            sfx.write(Sfx3d::at(f.whiff.clone(), p));
                        }
                        let k = if mine { 1.0 } else { near(eye, p, 45.0) };
                        if k > 0.0 && f.whiff_shake > 0.0 {
                            shake.write(CameraShakeEvent { trauma: f.whiff_shake * k, dir: Vec3::NEG_Y });
                        }
                        burst(debris, &mut seed, p, bv(*dir), 14, 5.5, [0.42, 0.38, 0.34]);
                    }
                    HeldFx::DiveSlam if class == WeaponClass::Heavy => {
                        // The charged slam breaking the ground (or a dive slam): ring of rubble.
                        if !f.whiff.is_empty() {
                            sfx.write(Sfx3d::at(f.whiff.clone(), p).gain(1.2).pitch(0.85));
                        }
                        let k = if mine { 1.0 } else { near(eye, p, 70.0) };
                        if k > 0.0 {
                            shake.write(CameraShakeEvent { trauma: (0.45 + 0.1 * *strength).min(0.9) * k, dir: Vec3::NEG_Y });
                        }
                        if mine {
                            fov.write(FovPunch { degrees: f.strike_punch * 1.5 });
                        }
                        ring_burst(debris, &mut seed, p, 3.0 + *strength, 26);
                        st.rings.push(GroundRing { at: p, r: 1.2 * *strength + 2.5, t: 0.0 });
                    }
                    _ => {}
                }
            }
            GameEvent::Hit { source: Some(src), construct: Some(cid), point, crit, .. } => {
                let Some(c) = active.world.constructs.get(*cid).filter(|c| c.category == FormCategory::Held && c.owner == *src) else { continue };
                if bd_sim::constructs::gun::is_gun(c) || own_signature(c) {
                    continue;
                }
                let Some(class) = class_of(&c.recipe) else { continue };
                let f = feel.0.of(class);
                let p = bv(*point);
                if !f.hit.is_empty() {
                    sfx.write(Sfx3d::at(f.hit.clone(), p).gain(f.hit_gain));
                }
                if *crit && !f.crit.is_empty() {
                    sfx.write(Sfx3d::at(f.crit.clone(), p));
                }
                if active.local == Some(*src) && f.hit_shake > 0.0 {
                    shake.write(CameraShakeEvent { trauma: f.hit_shake, dir: (p - eye).normalize_or_zero() });
                }
            }
            _ => {}
        }
    }
}

fn rng_next(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

fn rdir(seed: &mut u32) -> Vec3 {
    let v = Vec3::new(rng_next(seed) * 2.0 - 1.0, rng_next(seed) * 2.0 - 1.0, rng_next(seed) * 2.0 - 1.0);
    v.normalize_or(Vec3::Y)
}

/// Grit and rock chips thrown up from a strike into the ground.
fn burst(out: &mut MessageWriter<SpawnDebris>, seed: &mut u32, p: Vec3, dir: Vec3, n: usize, speed: f32, col: [f32; 3]) {
    let up = (Vec3::Y * 1.2 - dir * 0.3).normalize_or(Vec3::Y);
    for _ in 0..n {
        let v = (up + rdir(seed) * 0.7).normalize_or(Vec3::Y) * speed * (0.5 + rng_next(seed));
        let shade = 0.8 + 0.4 * rng_next(seed);
        out.write(SpawnDebris {
            pos: p + rdir(seed) * 0.2,
            vel: v,
            color: [col[0] * shade, col[1] * shade, col[2] * shade],
            size: 0.06 + 0.12 * rng_next(seed),
            emissive: 0.0,
            lifetime: 0.6 + 0.6 * rng_next(seed),
        });
    }
}

/// A ring of rubble thrown outward (the charged slam).
fn ring_burst(out: &mut MessageWriter<SpawnDebris>, seed: &mut u32, p: Vec3, r: f32, n: usize) {
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU + rng_next(seed) * 0.3;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let shade = 0.75 + 0.4 * rng_next(seed);
        out.write(SpawnDebris {
            pos: p + d * (0.4 + r * 0.25 * rng_next(seed)) + Vec3::Y * 0.1,
            vel: d * (4.0 + 5.0 * rng_next(seed)) + Vec3::Y * (4.0 + 5.0 * rng_next(seed)),
            color: [0.45 * shade, 0.41 * shade, 0.36 * shade],
            size: 0.1 + 0.2 * rng_next(seed),
            emissive: 0.0,
            lifetime: 0.8 + 0.7 * rng_next(seed),
        });
    }
}

/// Expanding crack rings drawn on the ground.
#[derive(Clone, Copy, Debug)]
struct GroundRing {
    at: Vec3,
    r: f32,
    t: f32,
}


/// Advance the followed swings; compute the third-person body poses.
fn advance(time: Res<Time>, active: Option<Res<ActiveWorld>>, feel: Res<WeaponFeel>, mut st: ResMut<WeaponFeelState>) {
    let dt = time.delta_secs().min(0.05);
    let st = &mut *st;
    st.bodies.clear();
    let Some(active) = active else {
        st.swings.clear();
        return;
    };
    st.swings.retain_mut(|s| {
        let alive = active.world.constructs.get(s.id).is_some_and(|c| c.active() && c.attached());
        s.t += dt * s.rate;
        for p in s.pts.iter_mut() {
            p.0 += dt;
        }
        let f = feel.0.of(s.class);
        s.pts.retain(|p| p.0 <= f.trail_secs.max(0.0));
        let playing = s.t <= s.curve.duration();
        if alive && playing {
            if let Some(b) = body_swing(s.curve, s.t, f) {
                st.bodies.insert(s.owner, b);
            }
        }
        alive && (playing || !s.pts.is_empty())
    });
}

/// The third-person pose for curve time `t` (None when the class has no body language).
fn body_swing(cv: &bd_shapes::SwingCurve, t: f32, f: &ClassFeel) -> Option<BodySwing> {
    if f.lean <= 0.0 && !f.two_hand {
        return None;
    }
    // The authored (unwrapped) angles: an overhead goes past vertical, which Euler extraction of the
    // rotation would fold back.
    let rest = euler_at(cv, 0.0);
    let e = euler_at(cv, t);
    // Arms follow the item: raised with it in the wind-up, driven down / across through the strike.
    let dp = (e.x - rest.x).to_radians();
    let arm_pitch = (std::f32::consts::FRAC_PI_2 + dp * 0.75).clamp(0.2, 3.1);
    let arm_yaw = (e.y * 0.5).to_radians().clamp(-1.2, 1.2);
    // Lean: back a little while coiling, forward through the strike, settling in the recovery.
    let (a, b) = cv.active_window();
    let lean = if t < a {
        -0.3 * f.lean * (t / a.max(1e-3))
    } else if t < b {
        let u = (t - a) / (b - a).max(1e-3);
        f.lean * (-0.3 + 1.3 * u)
    } else {
        let u = ((t - b) / cv.recovery.max(1e-3)).clamp(0.0, 1.0);
        f.lean * (1.0 - u) * (1.0 - u)
    };
    // Blend the whole pose in and out at the ends of the swing.
    let edge = (t / 0.06).min((cv.duration() - t) / 0.12).clamp(0.0, 1.0);
    Some(BodySwing { arm_pitch: arm_pitch * edge, arm_yaw: arm_yaw * edge, two_hand: if f.two_hand { edge } else { 0.0 }, lean: lean * edge })
}

/// A curve's Euler angles (deg: pitch, yaw, roll) at `t`, interpolated like `SwingCurve::sample`.
fn euler_at(cv: &bd_shapes::SwingCurve, t: f32) -> Vec3 {
    let k = &cv.keys;
    let Some(first) = k.first() else { return Vec3::ZERO };
    if t <= first.t {
        return Vec3::from_array(first.euler_deg.to_array());
    }
    for w in k.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if t <= b.t {
            let u = b.ease.apply((t - a.t) / (b.t - a.t).max(1e-6));
            return Vec3::from_array(a.euler_deg.lerp(b.euler_deg, u).to_array());
        }
    }
    Vec3::from_array(k[k.len() - 1].euler_deg.to_array())
}

/// Third-person overlay (called by `present::players` after the gun / palm pose): the arms follow a
/// class swing, the left hand joins a two-handed one, the torso leans into the strike. `arm_roots` =
/// [left, right] arm chain roots; `root` = the body's root part.
pub fn body_overlay(b: BodySwing, arm_roots: [Option<usize>; 2], locals: &mut [bd_shapes::Local]) {
    use bd_core::Quat as Q;
    let len = locals.len();
    if let Some(r) = arm_roots[1].filter(|&r| r < len) {
        locals[r] = bd_shapes::Local::rot(Q::from_rotation_y(b.arm_yaw) * Q::from_rotation_x(b.arm_pitch));
    }
    if let Some(l) = arm_roots[0].filter(|&l| l < len)
        && b.two_hand > 1e-3
    {
        // The left hand meets the right on the handle (a little inward).
        locals[l] = bd_shapes::Local::rot(Q::from_rotation_y(b.arm_yaw - 0.45 * b.two_hand) * Q::from_rotation_x(b.arm_pitch * b.two_hand));
    }
    if len > 0 && b.lean.abs() > 1e-4 {
        locals[0] = bd_shapes::Local::rot(Q::from_rotation_x(-b.lean));
    }
}

// ------------------------------------------------------------------------------------ trails

fn item_points(tf: &Transform, vis: &ConstructVisual) -> [Vec3; 4] {
    let b = &vis.model.bounds;
    let (cx, cy) = ((b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5);
    let grip = b.max.z.min(0.0);
    let tip = b.min.z;
    let at = |f: f32| tf.transform_point(Vec3::new(cx, cy, grip + (tip - grip) * f));
    [at(1.0), at(0.75), at(0.5), at(0.25)]
}

#[allow(clippy::too_many_arguments)]
fn draw_trails(
    active: Option<Res<ActiveWorld>>,
    feel: Res<WeaponFeel>,
    mut st: ResMut<WeaponFeelState>,
    mut particles: ResMut<FxParticles>,
    ents: Option<Res<super::constructs::ConstructEntities>>,
    items: Query<(&SimConstruct, &Transform, &ConstructVisual)>,
    time: Res<Time>,
) {
    let fx = &mut *particles;
    let dt = time.delta_secs().min(0.05);
    // Ground rings from charged slams.
    {
        let v = &mut st.rings;
        for g in v.iter_mut() {
            g.t += dt;
            let u = (g.t / 0.45).clamp(0.0, 1.0);
            let r = g.r * (0.25 + 0.75 * (1.0 - (1.0 - u) * (1.0 - u)));
            let k = 1.0 - u;
            let segs = 28;
            let mut prev = g.at + Vec3::new(r, 0.06, 0.0);
            for i in 1..=segs {
                let a = i as f32 / segs as f32 * std::f32::consts::TAU;
                let wob = 1.0 + 0.08 * ((i * 7 % 5) as f32 - 2.0) / 2.0;
                let p = g.at + Vec3::new(a.cos() * r * wob, 0.06, a.sin() * r * wob);
                fx.immediate_segment(prev, p, 0.07 + 0.08 * k, [1.0, 0.82, 0.55], 5.0 * k, PFlags::PLAIN);
                prev = p;
            }
        }
        v.retain(|g| g.t < 0.45);
    }
    let (Some(active), Some(ents)) = (active, ents) else { return };
    for s in st.swings.iter_mut() {
        let f = feel.0.of(s.class);
        if f.trail_width <= 0.0 || f.trail_secs <= 0.0 {
            continue;
        }
        let Some((_, tf, vis)) = ents.0.get(&s.id).and_then(|e| items.get(*e).ok()) else { continue };
        // Record while the blade is live (a hair before and after the hitbox window).
        let (a, b) = s.curve.active_window();
        if s.t >= a - 0.03 && s.t <= b + 0.05 {
            let p = item_points(tf, vis);
            if p.iter().all(|v| v.is_finite()) {
                s.pts.push((0.0, p));
            }
        }
        if s.pts.len() < 2 {
            continue;
        }
        let c = active.world.constructs.get(s.id);
        let col = trail_color(vis, c.map(|c| c.recipe.element));
        let scale = tf.scale.max_element().max(0.05);
        let lines = f.trail_lines.clamp(1, 4) as usize;
        let life = f.trail_secs.max(1e-3);
        for w in s.pts.windows(2) {
            let (a0, pa) = w[0];
            let (_, pb) = w[1];
            let k = (1.0 - a0 / life).clamp(0.0, 1.0);
            if k <= 0.0 {
                continue;
            }
            for (li, (x, y)) in pa.iter().zip(pb.iter()).take(lines).enumerate() {
                let lk = 1.0 - li as f32 / (lines as f32 + 0.5);
                let th = f.trail_width * scale.min(1.5) * (0.35 + 0.65 * k) * lk;
                // Hot white core at the tip for light blades; a warmer, dimmer smear for heavy ones.
                let c = if li == 0 { mix(col, [1.0, 1.0, 1.0], 0.45 * k) } else { col };
                fx.immediate_segment(*x, *y, th.max(0.004), c, f.trail_glow * k * lk, PFlags::PLAIN);
            }
        }
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn trail_color(vis: &ConstructVisual, element: Option<Element>) -> [f32; 3] {
    let e = element.and_then(|e| match e {
        Element::Fire => Some([1.0, 0.45, 0.12]),
        Element::Ice => Some([0.55, 0.85, 1.0]),
        Element::Electric => Some([0.6, 0.75, 1.0]),
        Element::Poison => Some([0.45, 1.0, 0.35]),
        Element::Shadow => Some([0.45, 0.2, 0.7]),
        Element::Water => Some([0.3, 0.6, 1.0]),
        Element::Earth => Some([0.7, 0.55, 0.35]),
        Element::Wind => Some([0.8, 1.0, 0.9]),
        Element::Psychic => Some([1.0, 0.4, 0.9]),
        Element::Atomic => Some([0.7, 1.0, 0.3]),
        _ => None,
    });
    e.unwrap_or_else(|| mix(vis.corps_color, [1.0, 1.0, 1.0], 0.35))
}

/// First-person helpers for the viewmodel: is the held construct two-handed by class, and its gun
/// kick multiplier.
pub fn two_handed(feel: &WeaponFeel, c: &Construct) -> bool {
    !c.has(Mechanic::Extends) && c.recipe.form != Form::Hands && feel.of_construct(c).is_some_and(|f| f.two_hand)
}

pub fn gun_kick(feel: &WeaponFeel, c: &Construct) -> f32 {
    feel.of_construct(c).map_or(1.0, |f| f.kick.clamp(0.2, 3.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_feel_table_parses_and_every_sound_exists() {
        let src = std::fs::read_to_string(crate::assets_dir().join("tuning").join("weapons.ron")).expect("weapons.ron");
        let t = FeelTable::from_ron(&src).expect("feel parses");
        let manifest = std::fs::read_to_string(crate::assets_dir().join("sfx").join("manifest.ron")).expect("manifest");
        for k in WeaponClass::ALL {
            let f = t.of(k);
            for name in [&f.swish, &f.hit, &f.crit, &f.whiff] {
                if !name.is_empty() {
                    assert!(manifest.contains(&format!("\"{name}\": (")), "{}: sound {name} missing from assets/sfx/manifest.ron", k.key());
                }
            }
        }
        // Light and heavy must read differently at a glance: thin & bright vs thick & long.
        assert!(t.light.trail_width < t.heavy.trail_width && t.light.trail_glow > t.heavy.trail_glow);
        assert!(t.heavy.trail_lines > t.light.trail_lines);
        assert!(t.heavy.two_hand && !t.light.two_hand);
        assert_ne!(t.light.swish, t.heavy.swish);
    }

    #[test]
    fn body_pose_follows_a_heavy_overhead() {
        let cv = bd_sim::constructs::weapon_class::curve_for(bd_shapes::MoveSet::Heavy, held::Swing::Combo0, 1.6).expect("curve");
        let f = ClassFeel { lean: 0.45, two_hand: true, ..ClassFeel::default() };
        let (a, b) = cv.active_window();
        let coil = body_swing(cv, a, &f).expect("pose");
        let strike = body_swing(cv, (a + b) * 0.5 + (b - a) * 0.45, &f).expect("pose");
        assert!(coil.arm_pitch > strike.arm_pitch + 0.8, "arms up in the wind-up, down through the strike: {coil:?} {strike:?}");
        assert!(coil.lean < 0.0 && strike.lean > 0.0, "leans back coiling, into the strike");
        assert!(strike.two_hand > 0.5);
        assert!(body_swing(cv, a, &ClassFeel::default()).is_none(), "no body language, no overlay");
    }
}
