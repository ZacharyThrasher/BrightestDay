//! Input: raw per-frame mouse look and per-tick `PlayerInput` building.
//!
//! * **Look** ([`LookAngles`]) is updated in `PreUpdate` right after Bevy's input systems from
//!   [`AccumulatedMouseMotion`] (winit raw device deltas → no OS acceleration), every rendered
//!   frame, with no smoothing: `degrees = counts × 0.022 × sensitivity` (Source units).
//! * **Buttons / movement** are sampled each frame into [`LocalInputState`]; presses shorter than a
//!   tick are latched so they are never lost. At each 120 Hz tick, `SimSet::Input` produces
//!   [`LocalInputState::current`] (a `bd_core::input::PlayerInput`, sequence-numbered) and pushes it
//!   to [`LocalInputState::history`] for prediction/reconciliation.
//! * [`UiFocus::typing`] (ring console / oath open) sets the `TYPING` bit and keeps sending the move
//!   axes held on the tick typing began (letters are text, but you keep running — voice == typing);
//!   on close, anything newly held is swallowed until released (no phantom presses). The keyboard tether / reel / charge / dodge / C keys are parked (`lobby::prefs::shell_bindings`)
//!   while aim, the mouse side-button tethers and the wheel stay live.

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

/// Mouse-look sensitivity multiplier on top of `Settings::sensitivity` (ADS scaling, written by
/// `present::ads`; 1 = unchanged).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct LookScale(pub f32);

impl Default for LookScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Whether the game currently owns the mouse.
#[derive(Resource, Debug, Default)]
pub struct CursorCapture {
    pub grabbed: bool,
    frames_since_grab: u32,
    secs_since_grab: f32,
}

/// A UI screen wants the mouse (the end-of-match vote): the cursor is released while set, and
/// grabbed again when it clears.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct FreeCursor(pub bool);

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

/// The keys / buttons the shell samples (mirrored from `lobby::prefs::KeyBinds` by its
/// `shell_bindings`; `F24` = unbound / parked while typing).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Bindings {
    pub forward: KeyCode,
    pub back: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
    pub jump: KeyCode,
    pub crouch: KeyCode,
    /// Hard-wired crouch alias (C) while no action holds it.
    pub crouch_alt: KeyCode,
    pub boost: KeyCode,
    pub tether_l: KeyCode,
    pub tether_r: KeyCode,
    /// Hard-wired tether aliases (mouse side buttons) while no action holds them.
    pub tether_l_mouse: Option<MouseButton>,
    pub tether_r_mouse: Option<MouseButton>,
    pub reel: KeyCode,
    /// Hold to charge (power up).
    pub charge: KeyCode,
    /// Press to dodge.
    pub dodge: KeyCode,
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
            crouch_alt: KeyCode::KeyC,
            boost: KeyCode::ShiftLeft,
            tether_l: KeyCode::KeyQ,
            tether_r: KeyCode::KeyE,
            tether_l_mouse: Some(MouseButton::Back),
            tether_r_mouse: Some(MouseButton::Forward),
            reel: KeyCode::KeyR,
            charge: KeyCode::KeyZ,
            dodge: KeyCode::AltLeft,
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
    /// CAMERA (`present::ads`): contextual buttons held this frame (ADS, alt-fire), a tap latch
    /// for them, and device bits to drop (RMB's SECONDARY while RMB aims a gun).
    pub ext_held: u16,
    pub ext_latched: u16,
    pub ext_mask: u16,
    /// Move axes latched on the tick typing began; sent unchanged until typing ends.
    typing_move: Option<(i8, i8)>,
    /// Buttons held across the end of typing that weren't being sent: dropped until released.
    swallow: u16,
}

impl LocalInputState {
    /// Builds and records the input for the next tick. Called once per tick.
    pub fn next_tick(&mut self, look: LookAngles, typing: bool) -> PlayerInput {
        let mut b = ((self.held | self.latched) & !self.ext_mask) | self.ext_held | self.ext_latched;
        self.latched = 0;
        self.ext_latched = 0;
        let (mut mx, mut mz) = (self.move_x, self.move_z);
        if typing {
            // Keep running on what was being sent when the ring opened (the last live tick, so a
            // letter typed before the first typing tick can't latch); aim still steers it.
            let live = (self.current.move_x, self.current.move_z);
            (mx, mz) = *self.typing_move.get_or_insert(if self.current.held(buttons::TYPING) { (0, 0) } else { live });
            b |= buttons::TYPING;
            b &= !(buttons::JUMP | buttons::CROUCH | buttons::BOOST | buttons::CHARGE | buttons::DODGE);
        } else {
            if self.typing_move.take().is_some() {
                // Hand-off to the live keys: letters still down (Space, Q, Shift…) must not fire.
                self.swallow = b & !self.current.buttons;
            } else {
                self.swallow &= self.held;
            }
            b &= !self.swallow;
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
        self.ext_latched = 0;
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
            .init_resource::<FreeCursor>()
            .init_resource::<Bindings>()
            .init_resource::<LocalInputState>()
            .init_resource::<InputOverride>()
            .init_resource::<TickInputQueue>()
            .init_resource::<LookScale>()
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
    free: Res<FreeCursor>,
    window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let (window, mut cursor) = window.into_inner();
    let in_game = *state.get() == AppState::InGame && !menu.0 && !free.0;
    // Grab when entering gameplay; re-grab on click after alt-tab.
    let want = in_game && window.focused && (capture.grabbed || mouse.just_pressed(MouseButton::Left) || menu.is_changed() || state.is_changed() || free.is_changed());
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
    scale: Res<LookScale>,
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
    let k = (M_YAW_DEG * settings.sensitivity * scale.0).to_radians();
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
        k(binds.crouch) || k(binds.crouch_alt),
        kj(binds.crouch) || kj(binds.crouch_alt),
        buttons::CROUCH,
    );
    bit(k(binds.boost), kj(binds.boost), buttons::BOOST);
    let (ml, mr) = (binds.tether_l_mouse, binds.tether_r_mouse);
    bit(
        k(binds.tether_l) || ml.is_some_and(m),
        kj(binds.tether_l) || ml.is_some_and(mj),
        buttons::TETHER_L,
    );
    bit(
        k(binds.tether_r) || mr.is_some_and(m),
        kj(binds.tether_r) || mr.is_some_and(mj),
        buttons::TETHER_R,
    );
    bit(k(binds.reel), kj(binds.reel), buttons::REEL);
    bit(k(binds.charge), kj(binds.charge), buttons::CHARGE);
    bit(k(binds.dodge), kj(binds.dodge), buttons::DODGE);
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
    fn typing_latches_movement() {
        let mut s = LocalInputState { move_z: 1, held: buttons::JUMP | buttons::TETHER_L, ..Default::default() };
        s.next_tick(LookAngles::default(), false);
        let i = s.next_tick(LookAngles::default(), true);
        assert_eq!((i.move_x, i.move_z), (0, 1));
        assert!(i.held(buttons::TYPING));
        assert!(i.held(buttons::TETHER_L));
        assert!(!i.held(buttons::JUMP));
        // Typed letters (W/A/S/D as text) don't move you: the latch holds.
        s.move_z = -1;
        s.move_x = 1;
        let i = s.next_tick(LookAngles::default(), true);
        assert_eq!((i.move_x, i.move_z), (0, 1));
    }

    #[test]
    fn typing_with_nothing_held_coasts() {
        let mut s = LocalInputState::default();
        s.next_tick(LookAngles::default(), false);
        s.move_z = 1; // typed "w" after the ring opened
        let i = s.next_tick(LookAngles::default(), true);
        assert_eq!((i.move_x, i.move_z), (0, 0));
    }

    #[test]
    fn closing_typing_hands_off_cleanly() {
        let mut s = LocalInputState { move_z: 1, held: buttons::TETHER_R, ..Default::default() };
        s.next_tick(LookAngles::default(), false);
        s.next_tick(LookAngles::default(), true);
        // Released W while typing, still holding Space + a keyboard tether key at close.
        s.move_z = 0;
        s.held = buttons::TETHER_R | buttons::JUMP | buttons::TETHER_L;
        s.latched = buttons::JUMP;
        let i = s.next_tick(LookAngles::default(), false);
        assert_eq!((i.move_x, i.move_z), (0, 0), "no stuck direction");
        assert!(!i.held(buttons::TYPING));
        assert!(i.held(buttons::TETHER_R), "held through typing stays held");
        assert!(!i.held(buttons::JUMP) && !i.held(buttons::TETHER_L), "no phantom presses");
        // Still held: still swallowed.
        let i = s.next_tick(LookAngles::default(), false);
        assert!(!i.held(buttons::JUMP) && !i.held(buttons::TETHER_L));
        // Released, then pressed again: live.
        s.held = buttons::TETHER_R;
        s.next_tick(LookAngles::default(), false);
        s.held = buttons::TETHER_R | buttons::JUMP;
        s.move_z = 1;
        let i = s.next_tick(LookAngles::default(), false);
        assert!(i.held(buttons::JUMP));
        assert_eq!(i.move_z, 1);
        // A fresh ring latches afresh.
        s.move_z = -1;
        s.next_tick(LookAngles::default(), false);
        assert_eq!(s.next_tick(LookAngles::default(), true).move_z, -1);
    }
}
