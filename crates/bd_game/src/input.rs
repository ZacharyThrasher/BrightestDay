//! Input: raw per-frame mouse look and per-tick `PlayerInput` building.
//!
//! * **Look** ([`LookAngles`]) is updated in `PreUpdate` right after Bevy's input systems from
//!   [`AccumulatedMouseMotion`] (winit raw device deltas → no OS acceleration), every rendered
//!   frame, with no smoothing: `degrees = counts × 0.022 × sensitivity` (Source units).
//! * **Buttons / movement** are sampled each frame into [`LocalInputState`]; presses shorter than a
//!   tick are latched so they are never lost. At each 120 Hz tick, `SimSet::Input` produces
//!   [`LocalInputState::current`] (a `bd_core::input::PlayerInput`, sequence-numbered) and pushes it
//!   to [`LocalInputState::history`] for prediction/reconciliation.
//! * [`UiFocus::typing`] (ring console / oath open) sets the `TYPING` bit and zeroes movement keys
//!   while aim and tether buttons stay live.

use crate::settings::Settings;
use crate::state::{AppState, InGameMenu};
use crate::tick::{FrameSet, SimSet, SimTick};
use bd_core::input::{buttons, PlayerInput};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use std::collections::VecDeque;

/// Source's `m_yaw` / `m_pitch`: degrees per mouse count at sensitivity 1.
pub const M_YAW_DEG: f32 = 0.022;
pub const PITCH_LIMIT_DEG: f32 = 89.0;
/// Mouse deltas are ignored for this many frames after the cursor is (re)grabbed, and at least
/// this long, to swallow the warp/jump some platforms report.
const GRAB_IGNORE_FRAMES: u32 = 2;
const GRAB_IGNORE_SECS: f32 = 0.06;
const INPUT_HISTORY: usize = 256;

/// Local view angles in radians. Authoritative for the local camera; copied into every tick's
/// `PlayerInput`. Yaw 0 looks down -Z, positive yaw turns left; positive pitch looks up.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct LookAngles {
    pub yaw: f32,
    pub pitch: f32,
}

impl LookAngles {
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }
    pub fn forward(&self) -> Vec3 {
        crate::util::v3(bd_core::input::look_dir(self.yaw, self.pitch))
    }
    pub fn set_degrees(&mut self, yaw: f32, pitch: f32) {
        self.yaw = yaw.to_radians();
        self.pitch = pitch.clamp(-PITCH_LIMIT_DEG, PITCH_LIMIT_DEG).to_radians();
    }
}

/// Whether the game currently owns the mouse.
#[derive(Resource, Debug, Default)]
pub struct CursorCapture {
    pub grabbed: bool,
    frames_since_grab: u32,
    secs_since_grab: f32,
}

/// UI focus flags other modules set.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct UiFocus {
    /// Ring console / oath typing: movement keys are text, not movement.
    pub typing: bool,
}

/// Recorded per-tick inputs to replay (ghost playback / automated tests). While non-empty, each
/// sim tick pops one input instead of reading the devices, and the view follows its yaw/pitch.
#[derive(Resource, Debug, Default)]
pub struct TickInputQueue(pub VecDeque<PlayerInput>);

/// Scripted input (automation / demos): when active, replaces the devices for buttons and movement.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct InputOverride {
    pub active: bool,
    pub held: u16,
    pub move_x: i8,
    pub move_z: i8,
}

/// Default bindings (rebinding later). Mouse4/5 mirror Q/E.
#[derive(Resource, Debug, Clone)]
pub struct Bindings {
    pub forward: KeyCode,
    pub back: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
    pub jump: KeyCode,
    pub crouch: KeyCode,
    pub boost: KeyCode,
    pub tether_l: KeyCode,
    pub tether_r: KeyCode,
    pub reel: KeyCode,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            forward: KeyCode::KeyW,
            back: KeyCode::KeyS,
            left: KeyCode::KeyA,
            right: KeyCode::KeyD,
            jump: KeyCode::Space,
            crouch: KeyCode::ControlLeft,
            boost: KeyCode::ShiftLeft,
            tether_l: KeyCode::KeyQ,
            tether_r: KeyCode::KeyE,
            reel: KeyCode::KeyR,
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct LocalInputState {
    /// Next sequence number to assign.
    next_seq: u32,
    /// Buttons held at the last frame sample.
    held: u16,
    /// Buttons pressed since the last tick (so sub-tick taps register for one tick).
    latched: u16,
    move_x: i8,
    move_z: i8,
    /// Unconsumed wheel notches (+ = reel in).
    reel_accum: f32,
    /// Input for the tick currently being simulated (valid inside `SimTick` after `SimSet::Input`).
    pub current: PlayerInput,
    /// Recent inputs, oldest first (for client-side prediction replay / redundancy on the wire).
    pub history: VecDeque<PlayerInput>,
}

impl LocalInputState {
    /// Builds and records the input for the next tick. Called once per tick.
    pub fn next_tick(&mut self, look: LookAngles, typing: bool) -> PlayerInput {
        let mut b = self.held | self.latched;
        self.latched = 0;
        let (mut mx, mut mz) = (self.move_x, self.move_z);
        if typing {
            b |= buttons::TYPING;
            b &= !(buttons::JUMP | buttons::CROUCH | buttons::BOOST);
            mx = 0;
            mz = 0;
        }
        let notches = self.reel_accum.trunc().clamp(-127.0, 127.0);
        self.reel_accum -= notches;
        let input = PlayerInput {
            seq: self.next_seq,
            yaw: look.yaw,
            pitch: look.pitch,
            move_x: mx,
            move_z: mz,
            buttons: b,
            reel_notches: notches as i8,
        };
        self.next_seq = self.next_seq.wrapping_add(1);
        self.current = input;
        if self.history.len() >= INPUT_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(input);
        input
    }

    fn clear_live(&mut self) {
        self.held = 0;
        self.latched = 0;
        self.move_x = 0;
        self.move_z = 0;
        self.reel_accum = 0.0;
    }
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookAngles>()
            .init_resource::<CursorCapture>()
            .init_resource::<UiFocus>()
            .init_resource::<Bindings>()
            .init_resource::<LocalInputState>()
            .init_resource::<InputOverride>()
            .init_resource::<TickInputQueue>()
            .add_systems(PreUpdate, (update_cursor_capture, apply_mouse_look).chain().after(InputSystems))
            .add_systems(Update, sample_frame_input.in_set(FrameSet::Input))
            .add_systems(SimTick, build_tick_input.in_set(SimSet::Input));
    }
}

fn update_cursor_capture(
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time<Real>>,
    mut capture: ResMut<CursorCapture>,
    window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let (window, mut cursor) = window.into_inner();
    let in_game = *state.get() == AppState::InGame && !menu.0;
    // Grab when entering gameplay; re-grab on click after alt-tab.
    let want = in_game && window.focused && (capture.grabbed || mouse.just_pressed(MouseButton::Left) || menu.is_changed() || state.is_changed());
    if want && !capture.grabbed {
        capture.grabbed = true;
        capture.frames_since_grab = 0;
        capture.secs_since_grab = 0.0;
    } else if !want && capture.grabbed {
        capture.grabbed = false;
    }
    if capture.grabbed {
        capture.frames_since_grab = capture.frames_since_grab.saturating_add(1);
        capture.secs_since_grab += time.delta_secs();
    }
    let (mode, visible) = if capture.grabbed {
        (CursorGrabMode::Locked, false)
    } else {
        (CursorGrabMode::None, true)
    };
    if cursor.grab_mode != mode {
        cursor.grab_mode = mode;
    }
    if cursor.visible != visible {
        cursor.visible = visible;
    }
}

fn apply_mouse_look(
    motion: Res<AccumulatedMouseMotion>,
    capture: Res<CursorCapture>,
    settings: Res<Settings>,
    mut look: ResMut<LookAngles>,
) {
    if !capture.grabbed
        || capture.frames_since_grab <= GRAB_IGNORE_FRAMES
        || capture.secs_since_grab < GRAB_IGNORE_SECS
    {
        return;
    }
    let d = motion.delta;
    if d == Vec2::ZERO {
        return;
    }
    let k = (M_YAW_DEG * settings.sensitivity).to_radians();
    let inv = if settings.invert_y { -1.0 } else { 1.0 };
    look.yaw = (look.yaw - d.x * k).rem_euclid(std::f32::consts::TAU);
    let lim = PITCH_LIMIT_DEG.to_radians();
    look.pitch = (look.pitch - d.y * k * inv).clamp(-lim, lim);
}

#[allow(clippy::too_many_arguments)]
fn sample_frame_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    binds: Res<Bindings>,
    capture: Res<CursorCapture>,
    ov: Res<InputOverride>,
    mut state: ResMut<LocalInputState>,
) {
    if ov.active {
        let pressed = ov.held & !state.held;
        state.held = ov.held;
        state.latched |= pressed;
        state.move_x = ov.move_x;
        state.move_z = ov.move_z;
        return;
    }
    if !capture.grabbed {
        state.clear_live();
        return;
    }
    let k = |c: KeyCode| keys.pressed(c);
    let kj = |c: KeyCode| keys.just_pressed(c);
    let m = |b: MouseButton| mouse.pressed(b);
    let mj = |b: MouseButton| mouse.just_pressed(b);

    let mut held = 0u16;
    let mut pressed = 0u16;
    let mut bit = |on: bool, just: bool, b: u16| {
        if on {
            held |= b;
        }
        if just {
            pressed |= b;
        }
    };
    bit(k(binds.jump), kj(binds.jump), buttons::JUMP);
    bit(
        k(binds.crouch) || k(KeyCode::KeyC),
        kj(binds.crouch) || kj(KeyCode::KeyC),
        buttons::CROUCH,
    );
    bit(k(binds.boost), kj(binds.boost), buttons::BOOST);
    bit(
        k(binds.tether_l) || m(MouseButton::Back),
        kj(binds.tether_l) || mj(MouseButton::Back),
        buttons::TETHER_L,
    );
    bit(
        k(binds.tether_r) || m(MouseButton::Forward),
        kj(binds.tether_r) || mj(MouseButton::Forward),
        buttons::TETHER_R,
    );
    bit(k(binds.reel), kj(binds.reel), buttons::REEL);
    bit(m(MouseButton::Left), mj(MouseButton::Left), buttons::PRIMARY);
    bit(m(MouseButton::Right), mj(MouseButton::Right), buttons::SECONDARY);

    state.held = held;
    state.latched |= pressed;
    let axis = |pos: bool, neg: bool| (pos as i8) - (neg as i8);
    state.move_x = axis(k(binds.right), k(binds.left));
    state.move_z = axis(k(binds.forward), k(binds.back));

    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 120.0,
    };
    state.reel_accum = (state.reel_accum + notches).clamp(-127.0, 127.0);
}

fn build_tick_input(
    mut look: ResMut<LookAngles>,
    focus: Res<UiFocus>,
    mut queue: ResMut<TickInputQueue>,
    mut state: ResMut<LocalInputState>,
) {
    let mut input = state.next_tick(*look, focus.typing);
    if let Some(r) = queue.0.pop_front() {
        input = PlayerInput { seq: input.seq, ..r };
        look.yaw = r.yaw;
        look.pitch = r.pitch;
        state.current = input;
        if let Some(last) = state.history.back_mut() {
            *last = input;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latched_tap_survives_one_tick() {
        let mut s = LocalInputState::default();
        s.latched = buttons::JUMP;
        let a = s.next_tick(LookAngles::default(), false);
        assert!(a.held(buttons::JUMP));
        let b = s.next_tick(LookAngles::default(), false);
        assert!(!b.held(buttons::JUMP));
        assert_eq!(b.seq, a.seq + 1);
    }

    #[test]
    fn reel_notches_consumed_once() {
        let mut s = LocalInputState::default();
        s.reel_accum = 2.5;
        assert_eq!(s.next_tick(LookAngles::default(), false).reel_notches, 2);
        assert_eq!(s.next_tick(LookAngles::default(), false).reel_notches, 0);
    }

    #[test]
    fn typing_blocks_movement() {
        let mut s = LocalInputState { move_z: 1, held: buttons::JUMP | buttons::TETHER_L, ..Default::default() };
        let i = s.next_tick(LookAngles::default(), true);
        assert_eq!(i.move_z, 0);
        assert!(i.held(buttons::TYPING));
        assert!(i.held(buttons::TETHER_L));
        assert!(!i.held(buttons::JUMP));
    }
}
