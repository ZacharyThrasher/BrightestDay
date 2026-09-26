//! Costume creator view controls: free rotation / zoom of the turntable preview
//! (`lobby::preview`). The model spins in camera space around its own centre, so the menu backdrop
//! is untouched.
//!
//! - Drag (LMB or RMB) on the preview side of the screen (right of the menu panel): horizontal =
//!   yaw (unlimited), vertical = pitch (clamped). Released drags carry a little inertia.
//! - Wheel over the preview: zoom (clamped). Over the panel the wheel still scrolls the rows.
//! - Keys (only while the cursor is not on the PHRASE text row, so typing is never stolen):
//!   Q / E or [ / ] turn, PageUp / PageDown tilt, - / = zoom, R reset. Double-click resets too.
//! - Gamepad right stick turns / tilts.
//! - Idle: a slow turntable spin that pauses on any input and resumes ~3 s later, never when the
//!   comfort settings have camera motion turned off.

use crate::menu::{costume_page_open, MenuPage, MenuState};
use crate::settings::ComfortSettings;
use crate::state::{AppState, InGameMenu};
use bevy::input::gamepad::Gamepad;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::{CursorLeft, CursorMoved, PrimaryWindow};
use std::f32::consts::TAU;

pub const PITCH_MIN: f32 = -30.0 * std::f32::consts::PI / 180.0;
pub const PITCH_MAX: f32 = 60.0 * std::f32::consts::PI / 180.0;
/// Zoom is a distance factor (1 = the default framing, smaller = closer).
pub const ZOOM_MIN: f32 = 0.45;
pub const ZOOM_MAX: f32 = 1.35;
/// Radians per logical pixel of drag.
const DRAG_RAD_PER_PX: f32 = 0.008;
const KEY_YAW_RATE: f32 = 2.4;
const KEY_PITCH_RATE: f32 = 1.4;
const IDLE_SPIN_RATE: f32 = 0.35;
const IDLE_RESUME_SECS: f32 = 3.0;
const DOUBLE_CLICK_SECS: f32 = 0.35;

/// Current (eased) and target view of the costume preview. Read by `preview::costume_preview`.
#[derive(Resource, Debug, Clone)]
pub struct CostumeView {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    target_yaw: f32,
    target_pitch: f32,
    target_zoom: f32,
    vel: Vec2,
    spin_rate: f32,
    dragging: bool,
    cursor: Option<Vec2>,
    drag_prev: Option<Vec2>,
    last_input: Option<f32>,
    last_click: Option<(f32, Vec2)>,
    active: bool,
    /// The pointer is over the preview (not the menu panel): the wheel zooms instead of scrolling.
    pub pointer_on_preview: bool,
}

impl Default for CostumeView {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            zoom: 1.0,
            target_yaw: 0.0,
            target_pitch: 0.0,
            target_zoom: 1.0,
            vel: Vec2::ZERO,
            spin_rate: 0.0,
            dragging: false,
            cursor: None,
            drag_prev: None,
            last_input: None,
            last_click: None,
            active: false,
            pointer_on_preview: false,
        }
    }
}

impl CostumeView {
    /// Turn / tilt the target view by `d` radians (x = yaw, y = pitch).
    pub fn rotate(&mut self, d: Vec2) {
        self.target_yaw += d.x;
        self.target_pitch = (self.target_pitch + d.y).clamp(PITCH_MIN, PITCH_MAX);
    }
    /// Zoom by `steps` wheel lines (positive = closer).
    pub fn zoom_by(&mut self, steps: f32) {
        self.target_zoom = (self.target_zoom * 0.88f32.powf(steps)).clamp(ZOOM_MIN, ZOOM_MAX);
    }
    /// Back to the front view at the default distance, via the nearest whole turn (no unwinding).
    pub fn reset(&mut self) {
        self.target_yaw = (self.target_yaw / TAU).round() * TAU;
        self.target_pitch = 0.0;
        self.target_zoom = 1.0;
        self.vel = Vec2::ZERO;
        self.spin_rate = 0.0;
    }
    /// 0 at the default distance → 1 fully zoomed in (the preview frames the upper body).
    pub fn zoom_t(&self) -> f32 {
        ((1.0 - self.zoom) / (1.0 - ZOOM_MIN)).clamp(0.0, 1.0)
    }
    /// Model rotation in camera space (`base` = the model's resting facing).
    pub fn rotation(&self, base: f32) -> Quat {
        Quat::from_rotation_x(self.pitch) * Quat::from_rotation_y(base + self.yaw)
    }
    /// Frame-rate independent ease of the shown view toward the target (+ inertia, idle spin).
    fn step(&mut self, dt: f32, now: f32, auto_spin: bool) {
        if !self.dragging && self.vel != Vec2::ZERO {
            let v = self.vel;
            self.rotate(v * dt);
            if self.target_pitch <= PITCH_MIN || self.target_pitch >= PITCH_MAX {
                self.vel.y = 0.0;
            }
            self.vel *= (-5.0 * dt).exp();
            if self.vel.length() < 0.02 {
                self.vel = Vec2::ZERO;
            }
        }
        let idle = !self.dragging && self.vel == Vec2::ZERO && self.last_input.is_none_or(|t| now - t > IDLE_RESUME_SECS);
        let want = if idle && auto_spin { IDLE_SPIN_RATE } else { 0.0 };
        // Spin eases in slowly (a gentle start) and stops at once when the player grabs it.
        self.spin_rate = if want > self.spin_rate { (self.spin_rate + dt * 0.35).min(want) } else { want };
        self.target_yaw += self.spin_rate * dt;
        let k = 1.0 - (-16.0 * dt).exp();
        self.yaw += (self.target_yaw - self.yaw) * k;
        self.pitch += (self.target_pitch - self.pitch) * k;
        self.zoom += (self.target_zoom - self.zoom) * (1.0 - (-12.0 * dt).exp());
        // Keep the numbers small over long sessions without a visible jump.
        if self.yaw.abs() > 64.0 * TAU {
            let w = (self.yaw / TAU).trunc() * TAU;
            self.yaw -= w;
            self.target_yaw -= w;
        }
    }
}

/// Comfort settings that turn camera motion off also stop the idle turntable.
pub fn reduces_motion(c: &ComfortSettings) -> bool {
    c.is_comfort_preset() || (c.shake <= 0.0 && c.fov_kick <= 0.0 && c.landing_dip <= 0.0)
}

/// Right edge (logical px) of the menu panel: 7% left padding + the 560 px rows + a small margin.
pub fn panel_right(window_width: f32) -> f32 {
    window_width * 0.07 + 580.0
}

pub struct CostumeViewPlugin;

impl Plugin for CostumeViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CostumeView>()
            .add_message::<CursorMoved>()
            .add_message::<CursorLeft>()
            .add_systems(Update, drive.before(crate::lobby::preview::costume_preview).run_if(not(in_state(AppState::Loading))));
    }
}

#[allow(clippy::too_many_arguments)]
fn drive(
    time: Res<Time<Real>>,
    menu: Res<MenuState>,
    state: Res<State<AppState>>,
    pause: Res<InGameMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut moved: MessageReader<CursorMoved>,
    mut left: MessageReader<CursorLeft>,
    windows: Query<&Window, With<PrimaryWindow>>,
    pads: Query<&Gamepad>,
    comfort: Option<Res<ComfortSettings>>,
    mut v: ResMut<CostumeView>,
) {
    // Track the pointer from the messages (also what the UI script harness drives).
    for ev in moved.read() {
        v.cursor = Some(ev.position);
    }
    if left.read().count() > 0 {
        v.cursor = None;
    }
    if !costume_page_open(&menu, state.get(), &pause) {
        if v.active {
            let cursor = v.cursor;
            *v = CostumeView { cursor, ..default() };
        }
        return;
    }
    v.active = true;
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    let interactive = menu.page == MenuPage::Costume;
    let width = windows.single().map(|w| w.width()).unwrap_or(1280.0);
    let on_preview = interactive && v.cursor.is_some_and(|c| c.x > panel_right(width));
    v.pointer_on_preview = on_preview;
    let mut touched = false;

    if interactive {
        // Drag to rotate.
        let grab = mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right);
        if grab && on_preview {
            if mouse.just_pressed(MouseButton::Left)
                && let Some(c) = v.cursor
            {
                if v.last_click.is_some_and(|(t, p)| now - t < DOUBLE_CLICK_SECS && p.distance(c) < 8.0) {
                    v.reset();
                    v.last_click = None;
                } else {
                    v.last_click = Some((now, c));
                }
            }
            v.dragging = true;
            v.drag_prev = v.cursor;
            v.vel = Vec2::ZERO;
            touched = true;
        }
        if v.dragging && !(mouse.pressed(MouseButton::Left) || mouse.pressed(MouseButton::Right)) {
            v.dragging = false;
            v.vel = v.vel.clamp_length_max(12.0);
        }
        if v.dragging {
            if let (Some(c), Some(p)) = (v.cursor, v.drag_prev) {
                let d = (c - p) * DRAG_RAD_PER_PX;
                v.rotate(d);
                if dt > 0.0 {
                    let inst = d / dt;
                    let a = 1.0 - (-25.0 * dt).exp();
                    v.vel = v.vel.lerp(inst, a);
                }
            }
            v.drag_prev = v.cursor;
            touched = true;
        }

        // Wheel zoom (only over the preview; over the panel it scrolls the rows).
        if on_preview && scroll.delta.y != 0.0 {
            let lines = match scroll.unit {
                MouseScrollUnit::Line => scroll.delta.y,
                MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
            };
            v.zoom_by(lines);
            touched = true;
        }

        // Keyboard fallback — never while the PHRASE row has the text caret.
        if !menu.typing {
            let axis = |pos: &[KeyCode], neg: &[KeyCode]| keys.any_pressed(pos.iter().copied()) as i32 as f32 - keys.any_pressed(neg.iter().copied()) as i32 as f32;
            let yaw = axis(&[KeyCode::KeyE, KeyCode::BracketRight], &[KeyCode::KeyQ, KeyCode::BracketLeft]);
            let pitch = axis(&[KeyCode::PageUp], &[KeyCode::PageDown]);
            let zoom = axis(&[KeyCode::Equal, KeyCode::NumpadAdd], &[KeyCode::Minus, KeyCode::NumpadSubtract]);
            if yaw != 0.0 || pitch != 0.0 {
                v.vel = Vec2::ZERO;
                v.rotate(Vec2::new(yaw * KEY_YAW_RATE, pitch * KEY_PITCH_RATE) * dt);
                touched = true;
            }
            if zoom != 0.0 {
                v.zoom_by(zoom * 6.0 * dt);
                touched = true;
            }
            if keys.just_pressed(KeyCode::KeyR) {
                v.reset();
                touched = true;
            }
        }

        // Gamepad right stick.
        for pad in &pads {
            let rs = pad.right_stick();
            let rs = Vec2::new(if rs.x.abs() > 0.2 { rs.x } else { 0.0 }, if rs.y.abs() > 0.2 { rs.y } else { 0.0 });
            if rs != Vec2::ZERO {
                v.vel = Vec2::ZERO;
                v.rotate(Vec2::new(rs.x * KEY_YAW_RATE, -rs.y * KEY_PITCH_RATE) * dt);
                touched = true;
            }
        }
    } else {
        v.dragging = false;
    }

    if touched {
        v.last_input = Some(now);
    }
    let auto_spin = !comfort.is_some_and(|c| reduces_motion(&c));
    v.step(dt, now, auto_spin);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_is_clamped_and_yaw_is_unlimited() {
        let mut v = CostumeView::default();
        v.rotate(Vec2::new(50.0, 10.0));
        assert_eq!(v.target_pitch, PITCH_MAX);
        assert_eq!(v.target_yaw, 50.0);
        v.rotate(Vec2::new(0.0, -10.0));
        assert_eq!(v.target_pitch, PITCH_MIN);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = CostumeView::default();
        v.zoom_by(100.0);
        assert_eq!(v.target_zoom, ZOOM_MIN);
        v.zoom_by(-100.0);
        assert_eq!(v.target_zoom, ZOOM_MAX);
    }

    #[test]
    fn reset_goes_to_nearest_front_without_unwinding() {
        let mut v = CostumeView::default();
        v.rotate(Vec2::new(3.0 * TAU + 0.4, 0.5));
        v.zoom_by(3.0);
        v.reset();
        assert!((v.target_yaw - 3.0 * TAU).abs() < 1e-4);
        assert_eq!(v.target_pitch, 0.0);
        assert_eq!(v.target_zoom, 1.0);
    }

    #[test]
    fn eases_to_target_and_idles_into_a_spin() {
        let mut v = CostumeView::default();
        v.rotate(Vec2::new(1.0, 0.3));
        v.last_input = Some(0.0);
        for i in 0..60 {
            v.step(1.0 / 60.0, i as f32 / 60.0, true);
        }
        // Within a second the view has (nearly) arrived and no idle spin has started.
        assert!((v.yaw - 1.0).abs() < 0.01 && (v.pitch - 0.3).abs() < 0.01, "{v:?}");
        assert_eq!(v.spin_rate, 0.0);
        for i in 60..600 {
            v.step(1.0 / 60.0, i as f32 / 60.0, true);
        }
        assert!(v.spin_rate > 0.0 && v.yaw > 1.2, "{v:?}");
    }

    #[test]
    fn no_idle_spin_when_comfort_reduces_motion() {
        assert!(reduces_motion(&ComfortSettings::COMFORT));
        assert!(!reduces_motion(&ComfortSettings::default()));
        let mut v = CostumeView::default();
        for i in 0..600 {
            v.step(1.0 / 60.0, i as f32 / 60.0, false);
        }
        assert_eq!(v.yaw, 0.0);
    }

    #[test]
    fn inertia_decays() {
        let mut v = CostumeView { vel: Vec2::new(6.0, 0.0), ..default() };
        v.last_input = Some(0.0);
        for i in 0..240 {
            v.step(1.0 / 60.0, i as f32 / 60.0, false);
        }
        assert_eq!(v.vel, Vec2::ZERO);
        assert!(v.target_yaw > 0.5 && v.target_yaw < 2.0, "{}", v.target_yaw);
    }

    #[test]
    fn panel_hit_test() {
        assert!(panel_right(1920.0) < 1920.0 * 0.5);
        assert!(panel_right(1280.0) > 600.0);
    }
}
