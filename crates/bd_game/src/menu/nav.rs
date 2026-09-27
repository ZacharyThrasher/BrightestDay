//! MENU FEEL: input arbitration + navigation helpers for the menu shell (pure logic, unit-tested).
//!
//! **The last-used device owns focus.** Keyboard / controller navigation hands focus to the keys;
//! a stationary mouse never takes it back. Hover moves focus only on frames where the pointer
//! actually moved — and, after key navigation, only once it has travelled [`Arbiter::WAKE_PX`] from
//! where it rested — or when the player clicks. (The old shell rebuilt every row on each ↑/↓, the
//! freshly spawned row under a resting pointer reported "hovered", and focus snapped back to it.)
//!
//! Also here: held-key repeat with acceleration, the scroll window (keep the focused row in view;
//! the wheel scrolls the view and drags focus along only as far as needed) and slider curves.

use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::prelude::*;

/// Which device owns menu focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Owner {
    /// Keyboard or controller (the menu opens this way: no hover until the mouse moves).
    #[default]
    Keys,
    Mouse,
}

/// The focus arbitration state machine.
#[derive(Clone, Debug, Default)]
pub struct Arbiter {
    owner: Owner,
    /// Where the pointer rested when the keys took over.
    anchor: Option<Vec2>,
    /// Last known pointer position (logical px).
    pub pos: Option<Vec2>,
}

impl Arbiter {
    /// Pointer travel (logical px) that wakes the mouse after key navigation.
    pub const WAKE_PX: f32 = 6.0;

    pub fn owner(&self) -> Owner {
        self.owner
    }

    pub fn mouse_owns(&self) -> bool {
        self.owner == Owner::Mouse
    }

    /// Keyboard / controller navigation: the keys own focus until the pointer really moves.
    pub fn key_nav(&mut self) {
        self.owner = Owner::Keys;
        self.anchor = self.pos;
    }

    /// The pointer moved to `pos` this frame. `true` = hover may move focus now.
    pub fn pointer_moved(&mut self, pos: Vec2) -> bool {
        self.pos = Some(pos);
        if self.owner == Owner::Mouse {
            return true;
        }
        match self.anchor {
            // First sighting while the keys own focus: rest here, wake on real travel.
            None => {
                self.anchor = Some(pos);
                false
            }
            Some(a) if a.distance(pos) < Self::WAKE_PX => false,
            Some(_) => {
                self.owner = Owner::Mouse;
                self.anchor = None;
                true
            }
        }
    }

    /// A click (or wheel over a control) is always mouse use.
    pub fn click(&mut self) {
        self.owner = Owner::Mouse;
        self.anchor = None;
    }

    pub fn pointer_left(&mut self) {
        self.pos = None;
    }
}

/// Held-key repeat with acceleration: one fire on press, then after [`Repeat::DELAY`] repeats that
/// speed up from ~8/s to ~28/s over [`Repeat::RAMP`] seconds.
#[derive(Clone, Debug, Default)]
pub struct Repeat {
    held: Option<f32>,
    next: f32,
}

impl Repeat {
    pub const DELAY: f32 = 0.34;
    pub const RAMP: f32 = 1.2;
    const SLOW: f32 = 0.12;
    const FAST: f32 = 0.035;

    fn interval(at: f32) -> f32 {
        let k = ((at - Self::DELAY) / Self::RAMP).clamp(0.0, 1.0);
        Self::SLOW + (Self::FAST - Self::SLOW) * k
    }

    /// Fires this frame for a button that is `down` (at most 8 per frame).
    pub fn tick(&mut self, down: bool, dt: f32) -> u32 {
        if !down {
            self.held = None;
            return 0;
        }
        let Some(h) = self.held.as_mut() else {
            self.held = Some(0.0);
            self.next = Self::DELAY;
            return 1;
        };
        *h += dt.max(0.0);
        let h = *h;
        let mut n = 0;
        while h >= self.next && n < 8 {
            n += 1;
            self.next += Self::interval(self.next);
        }
        n
    }

    /// Seconds held (0 when up).
    pub fn held(&self) -> f32 {
        self.held.unwrap_or(0.0)
    }
}

/// First visible row so `cursor` stays in view (`lo` = rows pinned above the window, e.g. the
/// settings tab strip; scrolling up keeps the row above the cursor — its header — visible).
pub fn follow(cursor: usize, scroll: usize, n: usize, vis: usize, lo: usize) -> usize {
    let cur = cursor.max(lo);
    let mut scroll = scroll.max(lo);
    if cur < scroll {
        scroll = cur.saturating_sub(1).max(lo);
    }
    if cur >= scroll + vis {
        scroll = cur + 1 - vis;
    }
    scroll.min(n.saturating_sub(vis).max(lo))
}

/// Wheel: scroll the view by `d` rows; focus is dragged along only as far as needed to stay in view.
/// Returns (cursor, scroll).
pub fn wheel(selectable: &[bool], cursor: usize, scroll: usize, vis: usize, lo: usize, d: i32) -> (usize, usize) {
    let n = selectable.len();
    let max = n.saturating_sub(vis).max(lo);
    let scroll = (scroll.max(lo) as i32 + d).clamp(lo as i32, max as i32) as usize;
    let end = (scroll + vis).min(n);
    let mut cursor = cursor;
    if cursor < scroll {
        cursor = (scroll..end).find(|&i| selectable[i]).unwrap_or(cursor);
    } else if cursor >= end {
        cursor = (scroll..end).rev().find(|&i| selectable[i]).unwrap_or(cursor);
    }
    (cursor, scroll)
}

/// Slider value ↔ track position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Lin,
    /// Logarithmic (ranges spanning decades, e.g. sensitivity 0.05–20: 1.0 sits mid-track).
    Log,
}

pub fn to_frac(v: f32, lo: f32, hi: f32, curve: Curve) -> f32 {
    if hi <= lo {
        return 0.0;
    }
    let f = match curve {
        Curve::Log if lo > 0.0 => (v.max(lo) / lo).ln() / (hi / lo).ln(),
        _ => (v - lo) / (hi - lo),
    };
    f.clamp(0.0, 1.0)
}

pub fn from_frac(f: f32, lo: f32, hi: f32, curve: Curve) -> f32 {
    let f = f.clamp(0.0, 1.0);
    match curve {
        Curve::Log if lo > 0.0 && hi > lo => lo * (hi / lo).powf(f),
        _ => lo + (hi - lo) * f,
    }
}

/// One frame of menu navigation intent from keyboard + controller (repeats already applied).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Intent {
    pub up: u32,
    pub down: u32,
    pub left: u32,
    pub right: u32,
    /// Controller confirm (A / south).
    pub pad_confirm: bool,
    /// Controller back (B / east).
    pub pad_back: bool,
    /// Controller bumpers: settings tab −1 / +1.
    pub pad_tab: i32,
    /// ↑/↓ fires come from a held key (repeats stop at the list ends; a fresh press wraps).
    pub held_v: bool,
}

impl Intent {
    pub fn any(&self) -> bool {
        self.up + self.down + self.left + self.right > 0 || self.pad_confirm || self.pad_back || self.pad_tab != 0
    }
}

/// Navigation repeat timers + the arbiter (lives in [`MenuNav`]).
#[derive(Clone, Debug, Default)]
pub struct Keys {
    up: Repeat,
    down: Repeat,
    left: Repeat,
    right: Repeat,
}

impl Keys {
    /// Arrow keys (+ WASD / Tab when not typing), the d-pad and the left stick.
    pub fn read(&mut self, keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, typing: bool, dt: f32) -> Intent {
        let (mut pu, mut pd, mut pl, mut pr) = (false, false, false, false);
        let mut it = Intent::default();
        for pad in pads {
            let stick = Vec2::new(pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0), pad.get(GamepadAxis::LeftStickY).unwrap_or(0.0));
            pu |= pad.pressed(GamepadButton::DPadUp) || stick.y > 0.6;
            pd |= pad.pressed(GamepadButton::DPadDown) || stick.y < -0.6;
            pl |= pad.pressed(GamepadButton::DPadLeft) || stick.x < -0.6;
            pr |= pad.pressed(GamepadButton::DPadRight) || stick.x > 0.6;
            it.pad_confirm |= pad.just_pressed(GamepadButton::South);
            it.pad_back |= pad.just_pressed(GamepadButton::East);
            it.pad_tab += pad.just_pressed(GamepadButton::RightTrigger) as i32 - pad.just_pressed(GamepadButton::LeftTrigger) as i32;
        }
        let k = |c: KeyCode| keys.pressed(c);
        let up = k(KeyCode::ArrowUp) || (!typing && k(KeyCode::KeyW)) || pu;
        let down = k(KeyCode::ArrowDown) || (!typing && k(KeyCode::KeyS)) || k(KeyCode::Tab) || pd;
        let left = k(KeyCode::ArrowLeft) || (!typing && k(KeyCode::KeyA)) || pl;
        let right = k(KeyCode::ArrowRight) || (!typing && k(KeyCode::KeyD)) || pr;
        it.up = self.up.tick(up, dt);
        it.down = self.down.tick(down, dt);
        it.left = self.left.tick(left, dt);
        it.right = self.right.tick(right, dt);
        it.held_v = self.up.held() > 0.0 || self.down.held() > 0.0;
        it
    }

    /// Step multiplier for held ←/→ on a slider (long holds cover wide ranges).
    pub fn slide_boost(&self) -> u32 {
        let h = self.left.held().max(self.right.held());
        if h > 2.6 {
            4
        } else if h > 1.4 {
            2
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stationary_mouse_never_steals_focus_from_keys() {
        let mut a = Arbiter::default();
        assert_eq!(a.owner(), Owner::Keys, "menus open keyboard-owned: a resting pointer doesn't pick a row");
        // Mouse used: it owns.
        assert!(!a.pointer_moved(Vec2::new(100.0, 100.0)), "first sighting only rests");
        assert!(a.pointer_moved(Vec2::new(120.0, 100.0)));
        assert!(a.mouse_owns());
        // ↓ pressed: keys own; the row respawned under the resting pointer can't take focus back.
        a.key_nav();
        assert!(!a.mouse_owns());
        // Jitter below the threshold doesn't wake it either.
        assert!(!a.pointer_moved(Vec2::new(123.0, 102.0)));
        assert!(!a.mouse_owns());
        // Real travel does.
        assert!(a.pointer_moved(Vec2::new(100.0, 140.0)));
        assert!(a.mouse_owns());
        // Once awake, every move is hover.
        assert!(a.pointer_moved(Vec2::new(101.0, 140.0)));
    }

    #[test]
    fn keys_then_click_hands_focus_to_mouse_and_back() {
        let mut a = Arbiter::default();
        a.pointer_moved(Vec2::new(10.0, 10.0));
        a.key_nav();
        assert!(!a.mouse_owns());
        a.click();
        assert!(a.mouse_owns(), "a click is mouse use even without travel");
        a.key_nav();
        assert!(!a.mouse_owns());
        // The anchor is where the pointer rested at the key press.
        assert!(!a.pointer_moved(Vec2::new(14.0, 10.0)));
        assert!(a.pointer_moved(Vec2::new(17.0, 10.0)));
    }

    #[test]
    fn pointer_leaving_then_returning_needs_travel() {
        let mut a = Arbiter::default();
        a.pointer_moved(Vec2::ZERO);
        a.pointer_moved(Vec2::new(50.0, 0.0));
        a.pointer_left();
        a.key_nav();
        assert!(!a.pointer_moved(Vec2::new(300.0, 300.0)), "re-entry only rests");
        assert!(a.pointer_moved(Vec2::new(300.0, 320.0)));
    }

    #[test]
    fn held_keys_repeat_and_accelerate() {
        let mut r = Repeat::default();
        assert_eq!(r.tick(true, 0.016), 1, "press fires at once");
        let mut fires = 0;
        let mut t = 0.0;
        while t < Repeat::DELAY - 0.02 {
            fires += r.tick(true, 0.01);
            t += 0.01;
        }
        assert_eq!(fires, 0, "no repeat before the delay");
        // First second of repeating vs a second after the ramp.
        let count = |r: &mut Repeat, secs: f32| (0..(secs / 0.01) as usize).map(|_| r.tick(true, 0.01)).sum::<u32>();
        let early = count(&mut r, 0.5);
        let _ = count(&mut r, 1.0);
        let late = count(&mut r, 0.5);
        assert!(early >= 3, "{early}");
        assert!(late > early * 2, "accelerates: {early} → {late}");
        assert_eq!(r.tick(false, 0.01), 0);
        assert_eq!(r.held(), 0.0);
        assert_eq!(r.tick(true, 0.01), 1, "fresh press fires again");
        // A long frame can't flood.
        assert!(r.tick(true, 10.0) <= 8);
    }

    #[test]
    fn follow_keeps_the_focused_row_in_view() {
        // 30 rows, 14 visible under a pinned strip (row 0).
        assert_eq!(follow(0, 0, 30, 14, 1), 1);
        assert_eq!(follow(14, 1, 30, 14, 1), 1, "last visible row: no scroll");
        assert_eq!(follow(15, 1, 30, 14, 1), 2);
        assert_eq!(follow(29, 1, 30, 14, 1), 16);
        assert_eq!(follow(5, 16, 30, 14, 1), 4, "scrolling up shows the row above (its header)");
        assert_eq!(follow(3, 0, 5, 14, 0), 0, "short lists never scroll");
    }

    #[test]
    fn wheel_scrolls_the_view_and_drags_focus_only_as_needed() {
        let mut sel = vec![true; 30];
        sel[0] = false; // strip
        sel[1] = false; // header
        sel[16] = false; // header
        // Focus mid-view: the view scrolls, focus stays.
        assert_eq!(wheel(&sel, 8, 1, 14, 1, 1), (8, 2));
        // Focus scrolled off the top: dragged to the first selectable row in view.
        assert_eq!(wheel(&sel, 2, 1, 14, 1, 1), (2, 2), "top edge is still in view");
        assert_eq!(wheel(&sel, 2, 1, 14, 1, 2), (3, 3));
        assert_eq!(wheel(&sel, 15, 1, 14, 1, 15), (17, 16), "skips a header at the new top");
        let (c, s) = wheel(&sel, 17, 15, 14, 1, -1);
        assert_eq!((c, s), (17, 14), "still in view");
        let (c, s) = wheel(&sel, 28, 16, 14, 1, -3);
        assert_eq!(s, 13);
        assert_eq!(c, 26, "last selectable row in view");
        // Clamped at both ends; a follow() afterwards doesn't undo the wheel.
        assert_eq!(wheel(&sel, 5, 1, 14, 1, -5).1, 1);
        assert_eq!(wheel(&sel, 20, 16, 14, 1, 9).1, 16);
        let (c, s) = wheel(&sel, 2, 1, 14, 1, 4);
        assert_eq!(follow(c, s, 30, 14, 1), s);
    }

    #[test]
    fn slider_curves_round_trip() {
        for curve in [Curve::Lin, Curve::Log] {
            for v in [0.05, 0.3, 1.0, 4.0, 20.0] {
                let f = to_frac(v, 0.05, 20.0, curve);
                assert!((0.0..=1.0).contains(&f));
                assert!((from_frac(f, 0.05, 20.0, curve) - v).abs() < 1e-3, "{curve:?} {v}");
            }
        }
        let mid = to_frac(1.0, 0.05, 20.0, Curve::Log);
        assert!((mid - 0.5).abs() < 0.01, "sensitivity 1.0 sits mid-track: {mid}");
        assert_eq!(to_frac(-1.0, 0.0, 1.0, Curve::Lin), 0.0);
        assert_eq!(to_frac(2.0, 0.0, 1.0, Curve::Lin), 1.0);
        assert_eq!(to_frac(0.0, 0.0, 1.0, Curve::Log), 0.0, "log needs lo > 0: falls back to linear");
        assert_eq!(to_frac(5.0, 5.0, 5.0, Curve::Lin), 0.0);
    }
}
