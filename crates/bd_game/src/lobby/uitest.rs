//! UI automation for screenshots / smoke tests: `BD_UI_SCRIPT` injects synthetic keyboard input.
//!
//! ```text
//! BD_UI_SCRIPT="1.0=key:Enter;1.2=type:giant flaming hammer;1.6=key:Enter;3=key:KeyF;3.3=type:in brightest"
//! brightest_day --practice --screenshot shot.png --screenshot-at 1.5,3.5 --quit-after 5
//! ```
//! Times are seconds since the first frame in `Menu` or `InGame`. Actions: `key:<KeyCode>` (press +
//! release), `down:<KeyCode>` / `up:<KeyCode>`, `type:<text>` (one press per char, in order),
//! `mouse:<Left|Right|Middle|Back|Forward>` (click), `mdown:<button>` / `mup:<button>` (hold /
//! release), `cursor:<x>,<y>` (a `CursorMoved` in logical px — the OS cursor is not touched),
//! `wheel:<lines>` (vertical wheel, + = up).

use crate::state::AppState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};

#[derive(Clone, Debug)]
enum Step {
    Key(KeyCode),
    Down(KeyCode),
    Up(KeyCode),
    Type(String),
    Mouse(MouseButton),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Cursor(Vec2),
    Wheel(f32),
}

#[derive(Resource, Default)]
struct UiScript {
    steps: Vec<(f32, Step)>,
    next: usize,
    t: Option<f32>,
    release: Vec<KeyCode>,
    release_mouse: Vec<MouseButton>,
}

fn keycode(s: &str) -> Option<KeyCode> {
    crate::lobby::prefs::Bind::from_id(s).and_then(|b| b.key()).or(match s {
        "Escape" => Some(KeyCode::Escape),
        "Backspace" => Some(KeyCode::Backspace),
        "F3" => Some(KeyCode::F3),
        _ => None,
    })
}

fn parse(src: &str) -> Vec<(f32, Step)> {
    let mut v = Vec::new();
    for part in src.split(';') {
        let Some((t, a)) = part.split_once('=') else { continue };
        let Ok(t) = t.trim().parse::<f32>() else { continue };
        let (kind, arg) = a.split_once(':').unwrap_or((a, ""));
        let button = |s: &str| match s.trim() {
            "Right" => MouseButton::Right,
            "Middle" => MouseButton::Middle,
            "Back" => MouseButton::Back,
            "Forward" => MouseButton::Forward,
            _ => MouseButton::Left,
        };
        let step = match kind.trim() {
            "mdown" => Some(Step::MouseDown(button(arg))),
            "mup" => Some(Step::MouseUp(button(arg))),
            "cursor" => arg.split_once(',').and_then(|(x, y)| Some(Step::Cursor(Vec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?)))),
            "wheel" => arg.trim().parse().ok().map(Step::Wheel),
            "key" => keycode(arg.trim()).map(Step::Key),
            "down" => keycode(arg.trim()).map(Step::Down),
            "up" => keycode(arg.trim()).map(Step::Up),
            "type" => Some(Step::Type(arg.to_string())),
            "mouse" => Some(Step::Mouse(match arg.trim() {
                "Right" => MouseButton::Right,
                "Middle" => MouseButton::Middle,
                "Back" => MouseButton::Back,
                "Forward" => MouseButton::Forward,
                _ => MouseButton::Left,
            })),
            _ => None,
        };
        if let Some(s) = step {
            v.push((t, s));
        }
    }
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

pub struct UiTestPlugin;

impl Plugin for UiTestPlugin {
    fn build(&self, app: &mut App) {
        let Ok(src) = std::env::var("BD_UI_SCRIPT") else { return };
        app.insert_resource(UiScript { steps: parse(&src), ..default() }).add_systems(First, drive);
    }
}

fn char_key(c: char) -> KeyCode {
    let c = c.to_ascii_uppercase();
    if c == ' ' {
        return KeyCode::Space;
    }
    let id = if c.is_ascii_digit() { format!("Digit{c}") } else { format!("Key{c}") };
    keycode(&id).unwrap_or(KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified))
}

fn drive(
    time: Res<Time<Real>>,
    state: Res<State<AppState>>,
    window: Query<Entity, With<PrimaryWindow>>,
    mut s: ResMut<UiScript>,
    mut keys: MessageWriter<KeyboardInput>,
    mut mouse: MessageWriter<MouseButtonInput>,
    mut cursor: MessageWriter<CursorMoved>,
    mut wheel: MessageWriter<MouseWheel>,
) {
    let Ok(window) = window.single() else { return };
    if *state.get() == AppState::Loading && s.t.is_none() {
        return;
    }
    let t = s.t.get_or_insert(0.0);
    *t += time.delta_secs();
    let t = *t;
    let ev = |k: KeyCode, st: ButtonState, text: Option<&str>| KeyboardInput {
        key_code: k,
        logical_key: match (k, text) {
            (KeyCode::Enter, _) => Key::Enter,
            (KeyCode::Escape, _) => Key::Escape,
            (KeyCode::Backspace, _) => Key::Backspace,
            (KeyCode::Tab, _) => Key::Tab,
            (KeyCode::Space, _) => Key::Space,
            (KeyCode::ArrowUp, _) => Key::ArrowUp,
            (KeyCode::ArrowDown, _) => Key::ArrowDown,
            (KeyCode::ArrowLeft, _) => Key::ArrowLeft,
            (KeyCode::ArrowRight, _) => Key::ArrowRight,
            (_, Some(tx)) => Key::Character(tx.into()),
            (k, None) => Key::Character(format!("{k:?}").trim_start_matches("Key").trim_start_matches("Digit").to_lowercase().into()),
        },
        state: st,
        text: if st == ButtonState::Pressed { text.map(|x| x.into()) } else { None },
        repeat: false,
        window,
    };
    // Release last frame's taps.
    for k in std::mem::take(&mut s.release) {
        keys.write(ev(k, ButtonState::Released, None));
    }
    for b in std::mem::take(&mut s.release_mouse) {
        mouse.write(MouseButtonInput { button: b, state: ButtonState::Released, window });
    }
    while s.next < s.steps.len() && s.steps[s.next].0 <= t {
        let step = s.steps[s.next].1.clone();
        s.next += 1;
        info!("ui script: {step:?}");
        match step {
            Step::Key(k) => {
                let text = match k {
                    KeyCode::Space => Some(" "),
                    _ => None,
                };
                keys.write(ev(k, ButtonState::Pressed, text));
                s.release.push(k);
            }
            Step::Down(k) => {
                keys.write(ev(k, ButtonState::Pressed, None));
            }
            Step::Up(k) => {
                keys.write(ev(k, ButtonState::Released, None));
            }
            Step::Type(text) => {
                for c in text.chars() {
                    let k = char_key(c);
                    let mut buf = [0u8; 4];
                    let st: &str = c.encode_utf8(&mut buf);
                    keys.write(ev(k, ButtonState::Pressed, Some(st)));
                    keys.write(ev(k, ButtonState::Released, None));
                }
            }
            Step::Mouse(b) => {
                mouse.write(MouseButtonInput { button: b, state: ButtonState::Pressed, window });
                s.release_mouse.push(b);
            }
            Step::MouseDown(b) => {
                mouse.write(MouseButtonInput { button: b, state: ButtonState::Pressed, window });
            }
            Step::MouseUp(b) => {
                mouse.write(MouseButtonInput { button: b, state: ButtonState::Released, window });
            }
            Step::Cursor(position) => {
                cursor.write(CursorMoved { window, position, delta: None });
            }
            Step::Wheel(y) => {
                wheel.write(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y, window, phase: TouchPhase::Moved });
            }
        }
    }
}
