//! Juice: hit-stop, directional camera shake, FOV punch, screen flash / vignette.
//!
//! Drive it with messages:
//! * [`Impact`] — the one-stop call (DESIGN "Impact": everything scales from one `impact` scalar).
//! * or the individual channels: [`HitStop`], [`CameraShakeEvent`], [`FovPunch`], [`ScreenFlash`].
//!
//! Hit-stop is budgeted (≤ [`HitStopState::BUDGET_PER_SEC`] of freeze per second) so a flurry of
//! hits never turns into lag. While active, [`HitStopState::time_scale`] is 0. It never freezes the
//! sim tick ([`HitStopState::freeze_sim`] = false, offline too): the attacker / victim visuals hold
//! instead (`present::impact::HitFreeze`, tiered in `present::impact::connect`).
//!
//! Shake never touches `LookAngles`: it is a camera-local offset + small rotation applied on top
//! of the aim in `camera`, decaying within a few hundred ms, so the aim point always returns.

use crate::camera::{DynamicFov, PlayerCamera};
use crate::input::LookAngles;
use crate::settings::ComfortSettings;
use crate::state::AppState;
use crate::tick::{FrameSet, TickClock};
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui::{ColorStop, RadialGradient, RadialGradientShape, UiPosition};

/// Unified impact: `impact` ≈ 0.1 (tap) … 1.0 (big hit) … 2.0 (kaiju stomp).
#[derive(Message, Clone, Copy, Debug)]
pub struct Impact {
    pub impact: f32,
    /// World-space direction the hit travels (shake kicks along it). Zero = omnidirectional.
    pub dir: Vec3,
    /// The local player was the victim (flash + vignette) vs. the attacker (lighter channels).
    pub local_victim: bool,
    /// Flash tint for the victim (usually the attacker's corps colour).
    pub color: Color,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct HitStop {
    pub secs: f32,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct CameraShakeEvent {
    /// Added trauma (0..1; stacks, clamped to 1).
    pub trauma: f32,
    /// World-space hit direction (may be zero).
    pub dir: Vec3,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct FovPunch {
    pub degrees: f32,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct ScreenFlash {
    pub color: Color,
    /// Peak alpha of the full-screen flash (0..1).
    pub intensity: f32,
    pub duration: f32,
    /// Also pulse the edge vignette in this colour.
    pub vignette: bool,
}

#[derive(Resource, Debug)]
pub struct HitStopState {
    pub remaining: f32,
    /// Seconds of freeze still allowed (refills at BUDGET_PER_SEC, capped at BUDGET_PER_SEC).
    pub budget: f32,
    /// Freeze the local sim tick while active (offline/practice). Off for networked play.
    pub freeze_sim: bool,
}

impl Default for HitStopState {
    fn default() -> Self {
        // CONNECT: hit-stop is presentation time only (`present::impact::HitFreeze` holds the
        // involved players' visuals); the sim tick never freezes, offline included.
        Self { remaining: 0.0, budget: Self::BUDGET_PER_SEC, freeze_sim: false }
    }
}

impl HitStopState {
    pub const BUDGET_PER_SEC: f32 = 0.3;
    pub const MAX_SINGLE: f32 = 0.14;

    pub fn active(&self) -> bool {
        self.remaining > 0.0
    }
    /// 0 while frozen, else 1.
    pub fn time_scale(&self) -> f32 {
        if self.active() { 0.0 } else { 1.0 }
    }
    pub fn request(&mut self, secs: f32) {
        let want = secs.clamp(0.0, Self::MAX_SINGLE);
        // Overlapping requests extend to the longer one, paid from the budget.
        let extra = (want - self.remaining).max(0.0).min(self.budget);
        self.remaining += extra;
        self.budget -= extra;
    }
    fn update(&mut self, dt: f32) {
        self.remaining = (self.remaining - dt).max(0.0);
        self.budget = (self.budget + Self::BUDGET_PER_SEC * dt).min(Self::BUDGET_PER_SEC);
    }
}

/// Camera-local shake output, consumed by `camera`.
#[derive(Resource, Debug, Clone)]
pub struct CameraShake {
    pub trauma: f32,
    /// Camera-local unit direction of the last hit (x right, y up, z back).
    local_dir: Vec3,
    t: f32,
    pub offset: Vec3,
    pub rotation: Quat,
}

impl Default for CameraShake {
    fn default() -> Self {
        Self { trauma: 0.0, local_dir: Vec3::ZERO, t: 0.0, offset: Vec3::ZERO, rotation: Quat::IDENTITY }
    }
}

impl CameraShake {
    /// Trauma decay per second: a full-trauma shake is gone in ~0.4 s.
    pub const DECAY: f32 = 2.6;

    fn update(&mut self, dt: f32, strength: f32) {
        self.trauma = (self.trauma - Self::DECAY * dt).max(0.0);
        self.t += dt;
        if self.trauma <= 0.0 || strength <= 0.0 {
            self.offset = Vec3::ZERO;
            self.rotation = Quat::IDENTITY;
            return;
        }
        let s = self.trauma * self.trauma * strength;
        let t = self.t;
        // Cheap smooth noise: incommensurate sines.
        let n = |f: f32, p: f32| (t * f + p).sin() * 0.6 + (t * f * 2.13 + p * 1.7).sin() * 0.4;
        // Directional kick: oscillates along the hit direction, fast.
        let kick = self.local_dir * (t * 55.0).cos() * 0.10 * s;
        let jitter = Vec3::new(n(41.0, 0.0), n(47.0, 1.3), 0.0) * 0.045 * s;
        self.offset = kick + jitter;
        // Never roll (motion comfort): only a tiny pitch/yaw wobble on top of the translation.
        let pitch = n(37.0, 4.2) * 0.45f32.to_radians() * s;
        let yaw = n(33.0, 5.9) * 0.45f32.to_radians() * s;
        self.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    }
}

#[derive(Component)]
struct FlashOverlay;
#[derive(Component)]
struct ComfortVignette {
    current: f32,
}
#[derive(Component)]
struct VignetteOverlay;

#[derive(Resource, Default, Debug)]
struct FlashState {
    color: LinearRgba,
    intensity: f32,
    duration: f32,
    t: f32,
    vignette: bool,
}

pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Impact>()
            .add_message::<HitStop>()
            .add_message::<CameraShakeEvent>()
            .add_message::<FovPunch>()
            .add_message::<ScreenFlash>()
            .init_resource::<HitStopState>()
            .init_resource::<CameraShake>()
            .init_resource::<FlashState>()
            .add_systems(Startup, spawn_overlays)
            // Hit-stop must be decided before the tick runs this frame.
            .add_systems(Update, (fan_out_impacts, apply_hitstop).chain().before(FrameSet::Sim).after(FrameSet::Input))
            .add_systems(Update, (update_shake, apply_fov_punch, update_flash, comfort_vignette).in_set(FrameSet::Present));
    }
}

fn fan_out_impacts(
    mut impacts: MessageReader<Impact>,
    mut hitstop: MessageWriter<HitStop>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut fov: MessageWriter<FovPunch>,
    mut flash: MessageWriter<ScreenFlash>,
) {
    for i in impacts.read() {
        let k = i.impact.max(0.0);
        hitstop.write(HitStop { secs: 0.03 + 0.11 * k.min(1.0) });
        let trauma = if i.local_victim { 0.35 + 0.5 * k } else { 0.15 + 0.3 * k };
        shake.write(CameraShakeEvent { trauma: trauma.min(1.0), dir: i.dir });
        fov.write(FovPunch { degrees: if i.local_victim { 3.0 * k } else { -1.5 * k } });
        if i.local_victim {
            flash.write(ScreenFlash { color: i.color, intensity: (0.12 + 0.25 * k).min(0.45), duration: 0.18 + 0.12 * k, vignette: true });
        }
    }
}

fn apply_hitstop(
    time: Res<Time<Real>>,
    mut requests: MessageReader<HitStop>,
    mut state: ResMut<HitStopState>,
    mut clock: ResMut<TickClock>,
) {
    for r in requests.read() {
        state.request(r.secs);
    }
    state.update(time.delta_secs());
    clock.time_scale = if state.freeze_sim { state.time_scale() } else { 1.0 };
}

fn update_shake(
    time: Res<Time<Real>>,
    comfort: Res<ComfortSettings>,
    look: Res<LookAngles>,
    mut events: MessageReader<CameraShakeEvent>,
    mut shake: ResMut<CameraShake>,
) {
    for e in events.read() {
        shake.trauma = (shake.trauma + e.trauma).min(1.0);
        let local = look.rotation().inverse() * e.dir;
        // Mostly lateral/vertical: forward/back kicks read as zoom, which FOV punch already does.
        let l = Vec3::new(local.x, local.y, local.z * 0.3);
        shake.local_dir = l.normalize_or_zero();
    }
    shake.update(time.delta_secs().min(0.05), comfort.shake);
}

fn apply_fov_punch(
    comfort: Res<ComfortSettings>,
    mut events: MessageReader<FovPunch>,
    mut q: Query<&mut DynamicFov, With<PlayerCamera>>,
) {
    for e in events.read() {
        if comfort.fov_kick <= 0.0 {
            continue;
        }
        for mut f in &mut q {
            f.kick(e.degrees * comfort.fov_kick);
        }
    }
}

fn spawn_overlays(mut commands: Commands) {
    let full = Node {
        position_type: PositionType::Absolute,
        left: px(0),
        right: px(0),
        top: px(0),
        bottom: px(0),
        ..default()
    };
    commands.spawn((
        Name::new("FlashOverlay"),
        FlashOverlay,
        full.clone(),
        BackgroundColor(Color::NONE),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(50),
    ));
    commands.spawn((
        Name::new("ComfortVignette"),
        ComfortVignette { current: 0.0 },
        full.clone(),
        BackgroundGradient::from(comfort_gradient(0.0)),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(48),
    ));
    commands.spawn((
        Name::new("VignetteOverlay"),
        VignetteOverlay,
        full,
        BackgroundGradient::from(vignette_gradient(Color::NONE)),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(49),
    ));
}

fn vignette_gradient(edge: Color) -> RadialGradient {
    RadialGradient::new(
        UiPosition::CENTER,
        RadialGradientShape::FarthestCorner,
        vec![ColorStop::new(Color::NONE, percent(62)), ColorStop::new(edge, percent(100))],
    )
}

fn update_flash(
    time: Res<Time<Real>>,
    comfort: Res<ComfortSettings>,
    app_state: Res<State<AppState>>,
    mut events: MessageReader<ScreenFlash>,
    mut state: ResMut<FlashState>,
    mut flash: Query<(&mut BackgroundColor, &mut Visibility), (With<FlashOverlay>, Without<VignetteOverlay>)>,
    mut vignette: Query<(&mut BackgroundGradient, &mut Visibility), (With<VignetteOverlay>, Without<FlashOverlay>)>,
) {
    for e in events.read() {
        if comfort.flashes <= 0.0 {
            continue;
        }
        let e = ScreenFlash { intensity: e.intensity * comfort.flashes, ..*e };
        // A stronger flash replaces a weaker one in progress.
        let current = state.intensity * (1.0 - state.t / state.duration.max(1e-3)).max(0.0);
        if e.intensity >= current {
            *state = FlashState { color: e.color.to_linear(), intensity: e.intensity, duration: e.duration.max(0.02), t: 0.0, vignette: e.vignette };
        }
    }
    // Pre-warm the overlay pipelines behind the loading screen (first real flash must not hitch).
    if *app_state.get() == AppState::Loading {
        state.intensity = 0.004;
        state.duration = 1.0;
        state.t = 0.0;
        state.vignette = true;
    }
    let active = state.intensity > 0.0 && state.t < state.duration;
    let a = if active {
        let k = 1.0 - state.t / state.duration;
        state.t += time.delta_secs();
        state.intensity * k * k
    } else {
        state.intensity = 0.0;
        0.0
    };
    if let Ok((mut bg, mut vis)) = flash.single_mut() {
        let want = if a > 0.001 { Visibility::Visible } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if a > 0.001 {
            bg.0 = Color::LinearRgba(state.color.with_alpha(a * 0.28));
        }
    }
    if let Ok((mut grad, mut vis)) = vignette.single_mut() {
        let on = a > 0.001 && state.vignette;
        let want = if on { Visibility::Visible } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if on {
            *grad = BackgroundGradient::from(vignette_gradient(Color::LinearRgba(state.color.with_alpha((a * 1.3).min(0.5)))));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hitstop_budget_limits_freeze() {
        let mut h = HitStopState::default();
        for _ in 0..10 {
            h.request(0.14);
            h.update(0.14);
        }
        // 10 × 140 ms requested in 1.4 s, but the budget allows far less.
        assert!(h.budget >= 0.0);
        let mut total = 0.0;
        let mut h = HitStopState::default();
        for _ in 0..100 {
            h.request(0.14);
            let before = h.remaining;
            h.update(0.01);
            total += before - h.remaining;
        }
        // 1 s of wall time: at most budget + refill.
        assert!(total <= 0.61, "froze {total}");
    }

    #[test]
    fn shake_decays_to_zero() {
        let mut s = CameraShake { trauma: 1.0, local_dir: Vec3::X, ..Default::default() };
        for _ in 0..144 {
            s.update(1.0 / 144.0, 1.0);
        }
        assert_eq!(s.offset, Vec3::ZERO);
        assert_eq!(s.rotation, Quat::IDENTITY);
    }
}

fn comfort_gradient(alpha: f32) -> RadialGradient {
    RadialGradient::new(
        UiPosition::CENTER,
        RadialGradientShape::FarthestCorner,
        vec![
            ColorStop::new(Color::NONE, percent(38)),
            ColorStop::new(Color::srgba(0.0, 0.0, 0.0, alpha * 0.55), percent(72)),
            ColorStop::new(Color::srgba(0.0, 0.0, 0.0, alpha), percent(100)),
        ],
    )
}

/// Comfort vignette: darkens the view edges with speed and turn/swing rate (reduces vection).
fn comfort_vignette(
    time: Res<Time<Real>>,
    comfort: Res<ComfortSettings>,
    anchor: Res<crate::camera::ViewAnchor>,
    look: Res<LookAngles>,
    state: Res<State<AppState>>,
    mut last_yaw: Local<f32>,
    mut q: Query<(&mut ComfortVignette, &mut BackgroundGradient, &mut Visibility)>,
) {
    let Ok((mut v, mut grad, mut vis)) = q.single_mut() else { return };
    let dt = time.delta_secs().max(1e-4);
    let dyaw = (look.yaw - *last_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    *last_yaw = look.yaw;
    let in_game = *state.get() == AppState::InGame;
    let target = if in_game && comfort.vignette > 0.0 {
        let speed = ((anchor.velocity.length() - 10.0) / 25.0).clamp(0.0, 1.0);
        let turn = ((dyaw.abs() / dt - 1.5) / 5.0).clamp(0.0, 1.0);
        comfort.vignette * speed.max(turn) * 0.85
    } else {
        0.0
    };
    // Quick to darken, slow to clear.
    let rate = if target > v.current { 8.0 } else { 2.5 };
    v.current += (target - v.current) * (1.0 - (-rate * dt).exp());
    let want = if v.current > 0.01 { Visibility::Visible } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if v.current > 0.01 {
        *grad = BackgroundGradient::from(comfort_gradient(v.current));
    }
}
