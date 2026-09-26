//! Persisted player-facing preferences owned by the UI: key bindings for every action, loadout
//! phrases, costume, name/corps, voice / third-person / onomatopoeia toggles and the last lobby
//! `GameSettings` (+ map). Stored next to `settings.ron` as `ui_prefs.ron` (debounced autosave).
//!
//! * [`KeyBinds`] is the runtime table (one [`Bind`] per [`Action`]). Movement actions are
//!   mirrored into the shell's `input::Bindings` every time they change.
//! * [`UiPrefs::third_person`] / `voice` / `ono_*` are exposed as plain fields so other plugins
//!   (camera rig, onomatopoeia) can read the resource.

use crate::input::Bindings;
use crate::settings::Settings;
use bd_core::corps::Corps;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ------------------------------------------------------------------------------------------ binds

/// One physical input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bind {
    Key(KeyCode),
    Mouse(MouseButton),
    Unbound,
}

/// Keys offered for rebinding (and the only names we parse back).
const KEYS: &[KeyCode] = &[
    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE, KeyCode::KeyF, KeyCode::KeyG,
    KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN,
    KeyCode::KeyO, KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT, KeyCode::KeyU,
    KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY, KeyCode::KeyZ, KeyCode::Digit0, KeyCode::Digit1,
    KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7,
    KeyCode::Digit8, KeyCode::Digit9, KeyCode::F1, KeyCode::F2, KeyCode::F4, KeyCode::F5, KeyCode::F6,
    KeyCode::F7, KeyCode::F8, KeyCode::F9, KeyCode::F10, KeyCode::F12, KeyCode::Space, KeyCode::Tab,
    KeyCode::Enter, KeyCode::Backquote, KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::ControlLeft,
    KeyCode::ControlRight, KeyCode::AltLeft, KeyCode::AltRight, KeyCode::CapsLock, KeyCode::ArrowUp,
    KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::Minus, KeyCode::Equal,
    KeyCode::BracketLeft, KeyCode::BracketRight, KeyCode::Semicolon, KeyCode::Quote, KeyCode::Comma,
    KeyCode::Period, KeyCode::Slash, KeyCode::Backslash, KeyCode::Numpad0, KeyCode::Numpad1, KeyCode::Numpad2,
    KeyCode::Numpad3, KeyCode::Numpad4, KeyCode::Numpad5, KeyCode::Numpad6, KeyCode::Numpad7, KeyCode::Numpad8,
    KeyCode::Numpad9, KeyCode::Insert, KeyCode::Delete, KeyCode::Home, KeyCode::End, KeyCode::PageUp,
    KeyCode::PageDown,
];

const MOUSE: &[(MouseButton, &str)] = &[
    (MouseButton::Left, "MOUSE1"),
    (MouseButton::Right, "MOUSE2"),
    (MouseButton::Middle, "MOUSE3"),
    (MouseButton::Back, "MOUSE4"),
    (MouseButton::Forward, "MOUSE5"),
];

impl Bind {
    pub fn to_id(self) -> String {
        match self {
            Bind::Key(k) => format!("{k:?}"),
            Bind::Mouse(m) => MOUSE.iter().find(|(b, _)| *b == m).map(|(_, n)| n.to_string()).unwrap_or_default(),
            Bind::Unbound => "None".into(),
        }
    }

    pub fn from_id(s: &str) -> Option<Bind> {
        if s == "None" {
            return Some(Bind::Unbound);
        }
        if let Some((b, _)) = MOUSE.iter().find(|(_, n)| *n == s) {
            return Some(Bind::Mouse(*b));
        }
        KEYS.iter().find(|k| format!("{k:?}") == s).map(|k| Bind::Key(*k))
    }

    /// Short label for the settings panel ("W", "SPACE", "MOUSE3").
    pub fn label(self) -> String {
        match self {
            Bind::Unbound => "—".into(),
            Bind::Mouse(_) => self.to_id(),
            Bind::Key(k) => {
                let s = format!("{k:?}");
                let s = s.strip_prefix("Key").or_else(|| s.strip_prefix("Digit")).unwrap_or(&s).to_string();
                match s.as_str() {
                    "ShiftLeft" => "SHIFT".into(),
                    "ControlLeft" => "CTRL".into(),
                    "AltLeft" => "ALT".into(),
                    "Backquote" => "`".into(),
                    other => other.to_uppercase(),
                }
            }
        }
    }

    pub fn pressed(self, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        match self {
            Bind::Key(k) => keys.pressed(k),
            Bind::Mouse(m) => mouse.pressed(m),
            Bind::Unbound => false,
        }
    }
    pub fn just_pressed(self, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        match self {
            Bind::Key(k) => keys.just_pressed(k),
            Bind::Mouse(m) => mouse.just_pressed(m),
            Bind::Unbound => false,
        }
    }
    pub fn just_released(self, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        match self {
            Bind::Key(k) => keys.just_released(k),
            Bind::Mouse(m) => mouse.just_released(m),
            Bind::Unbound => false,
        }
    }
    pub fn key(self) -> Option<KeyCode> {
        match self {
            Bind::Key(k) => Some(k),
            _ => None,
        }
    }
}

/// Every rebindable action. Movement actions accept keys only (the shell samples them as keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Crouch,
    Boost,
    TetherL,
    TetherR,
    Reel,
    Console,
    ConsoleAlt,
    Voice,
    Oath,
    Charge,
    Dismiss,
    Power,
    Transform,
    Scoreboard,
    Slot1,
    Slot2,
    Slot3,
    Slot4,
    /// CAMERA: first ⇄ third person.
    ToggleView,
    /// CAMERA: over-the-shoulder side swap.
    Shoulder,
    /// CAMERA: aim down sights (with a gun in hand).
    Aim,
    /// CAMERA: a held gun's alt-fire (RMB aims guns).
    AltFire,
    /// MOVEMENT: dodge (a short intangible burst).
    Dodge,
}

impl Action {
    pub const ALL: [Action; 28] = [
        Action::Forward,
        Action::Back,
        Action::Left,
        Action::Right,
        Action::Jump,
        Action::Crouch,
        Action::Boost,
        Action::TetherL,
        Action::TetherR,
        Action::Reel,
        Action::Console,
        Action::ConsoleAlt,
        Action::Voice,
        Action::Oath,
        Action::Charge,
        Action::Dismiss,
        Action::Power,
        Action::Transform,
        Action::Scoreboard,
        Action::Slot1,
        Action::Slot2,
        Action::Slot3,
        Action::Slot4,
        Action::ToggleView,
        Action::Shoulder,
        Action::Aim,
        Action::AltFire,
        Action::Dodge,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|a| *a == self).unwrap_or(0)
    }

    pub fn id(self) -> &'static str {
        match self {
            Action::Forward => "forward",
            Action::Back => "back",
            Action::Left => "left",
            Action::Right => "right",
            Action::Jump => "jump",
            Action::Crouch => "crouch",
            Action::Boost => "boost",
            Action::TetherL => "tether_left",
            Action::TetherR => "tether_right",
            Action::Reel => "reel",
            Action::Console => "ring_console",
            Action::ConsoleAlt => "ring_console_alt",
            Action::Voice => "voice",
            Action::Oath => "oath",
            Action::Charge => "charge",
            Action::Dismiss => "dismiss",
            Action::Power => "power",
            Action::Transform => "transform",
            Action::Scoreboard => "scoreboard",
            Action::Slot1 => "slot1",
            Action::Slot2 => "slot2",
            Action::Slot3 => "slot3",
            Action::Slot4 => "slot4",
            Action::ToggleView => "toggle_view",
            Action::Shoulder => "swap_shoulder",
            Action::Aim => "aim",
            Action::AltFire => "alt_fire",
            Action::Dodge => "dodge",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::Forward => "FORWARD",
            Action::Back => "BACK",
            Action::Left => "STRAFE LEFT",
            Action::Right => "STRAFE RIGHT",
            Action::Jump => "JUMP",
            Action::Crouch => "CROUCH / SLIDE",
            Action::Boost => "GAS BOOST",
            Action::TetherL => "LEFT TETHER",
            Action::TetherR => "RIGHT TETHER",
            Action::Reel => "REEL IN",
            Action::Console => "RING CONSOLE",
            Action::ConsoleAlt => "RING CONSOLE (ALT)",
            Action::Voice => "VOICE (HOLD)",
            Action::Oath => "RECITE OATH",
            Action::Charge => "CHARGE (HOLD)",
            Action::Dismiss => "DISMISS",
            Action::Power => "POWER (TAP / HOLD)",
            Action::Transform => "TRANSFORM",
            Action::Scoreboard => "SCOREBOARD",
            Action::Slot1 => "QUICK CAST 1",
            Action::Slot2 => "QUICK CAST 2",
            Action::Slot3 => "QUICK CAST 3",
            Action::Slot4 => "QUICK CAST 4",
            Action::ToggleView => "FIRST / THIRD PERSON",
            Action::Shoulder => "SWAP SHOULDER",
            Action::Aim => "AIM DOWN SIGHTS",
            Action::AltFire => "ALT FIRE (GUNS)",
            Action::Dodge => "DODGE",
        }
    }

    pub fn keys_only(self) -> bool {
        matches!(
            self,
            Action::Forward | Action::Back | Action::Left | Action::Right | Action::Jump | Action::Crouch | Action::Boost | Action::TetherL | Action::TetherR | Action::Reel | Action::Charge | Action::Dodge
        )
    }

    pub fn default_bind(self) -> Bind {
        use KeyCode as K;
        Bind::Key(match self {
            Action::Forward => K::KeyW,
            Action::Back => K::KeyS,
            Action::Left => K::KeyA,
            Action::Right => K::KeyD,
            Action::Jump => K::Space,
            Action::Crouch => K::ControlLeft,
            Action::Boost => K::ShiftLeft,
            Action::TetherL => K::KeyQ,
            Action::TetherR => K::KeyE,
            Action::Reel => K::KeyR,
            Action::Console => K::Enter,
            Action::ConsoleAlt => K::KeyT,
            Action::Voice => K::KeyV,
            Action::Oath => K::KeyF,
            Action::Charge => K::KeyZ,
            Action::Dismiss => K::KeyX,
            Action::Power => K::KeyG,
            Action::Transform => return Bind::Mouse(MouseButton::Middle),
            Action::Scoreboard => K::Tab,
            Action::Slot1 => K::Digit1,
            Action::Slot2 => K::Digit2,
            Action::Slot3 => K::Digit3,
            Action::Slot4 => K::Digit4,
            // CAMERA: F5 is the long-standing "change view" key and free here (F3 perf, F4 render,
            // F11 fullscreen); B sits by the movement hand; H = gun alt-fire (rarely used; C is a hard-wired crouch alias).
            Action::ToggleView => K::F5,
            Action::Shoulder => K::KeyB,
            Action::Aim => return Bind::Mouse(MouseButton::Right),
            Action::AltFire => K::KeyH,
            // Dodge on the thumb while holding W. Not Caps Lock: it toggles caps, and CAPS in the ring
            // console means "yelling" (shout energy).
            Action::Dodge => K::AltLeft,
        })
    }
}

/// Runtime binding table.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct KeyBinds(pub [Bind; Action::COUNT]);

impl Default for KeyBinds {
    fn default() -> Self {
        Self(Action::ALL.map(|a| a.default_bind()))
    }
}

impl KeyBinds {
    pub fn get(&self, a: Action) -> Bind {
        self.0[a.index()]
    }
    pub fn set(&mut self, a: Action, b: Bind) {
        // One input, one action: steal it from whoever had it.
        if b != Bind::Unbound {
            for x in self.0.iter_mut() {
                if *x == b {
                    *x = Bind::Unbound;
                }
            }
        }
        self.0[a.index()] = b;
    }
    pub fn pressed(&self, a: Action, k: &ButtonInput<KeyCode>, m: &ButtonInput<MouseButton>) -> bool {
        self.get(a).pressed(k, m)
    }
    pub fn just_pressed(&self, a: Action, k: &ButtonInput<KeyCode>, m: &ButtonInput<MouseButton>) -> bool {
        self.get(a).just_pressed(k, m)
    }
    pub fn just_released(&self, a: Action, k: &ButtonInput<KeyCode>, m: &ButtonInput<MouseButton>) -> bool {
        self.get(a).just_released(k, m)
    }
    fn to_map(&self) -> BTreeMap<String, String> {
        Action::ALL.iter().map(|a| (a.id().to_string(), self.get(*a).to_id())).collect()
    }
    fn from_map(m: &BTreeMap<String, String>) -> Self {
        let mut out = Self::default();
        for a in Action::ALL {
            if let Some(b) = m.get(a.id()).and_then(|s| Bind::from_id(s)) {
                if a.keys_only() && matches!(b, Bind::Mouse(_)) {
                    continue;
                }
                out.0[a.index()] = b;
            }
        }
        out
    }
}

// ------------------------------------------------------------------------------------------ prefs

/// Onomatopoeia mode (mirrors `bd_fx::OnoMode`, kept serialisable here).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnoPref {
    Off,
    #[default]
    BigHits,
    Full,
}

impl OnoPref {
    pub fn next(self, d: i32) -> Self {
        let all = [OnoPref::Off, OnoPref::BigHits, OnoPref::Full];
        let i = all.iter().position(|x| *x == self).unwrap_or(1) as i32;
        all[(i + d).rem_euclid(3) as usize]
    }
    pub fn label(self) -> &'static str {
        match self {
            OnoPref::Off => "OFF",
            OnoPref::BigHits => "BIG HITS",
            OnoPref::Full => "FULL COMIC",
        }
    }
}

/// Shout callouts (present::shout): everyone's words / only mine / none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShoutPref {
    #[default]
    On,
    Mine,
    Off,
}

impl ShoutPref {
    pub fn next(self, d: i32) -> Self {
        let all = [ShoutPref::On, ShoutPref::Mine, ShoutPref::Off];
        let i = all.iter().position(|x| *x == self).unwrap_or(0) as i32;
        all[(i + d).rem_euclid(3) as usize]
    }
    pub fn label(self) -> &'static str {
        match self {
            ShoutPref::On => "ON",
            ShoutPref::Mine => "MINE ONLY",
            ShoutPref::Off => "OFF",
        }
    }
}

/// Costume (sent once at join).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CostumePref {
    pub phrase: String,
    pub seed: u32,
    pub accent: Option<[f32; 3]>,
    pub head_scale: f32,
    /// `bd_shapes::costume::acc` bits to hide.
    pub hide: u32,
}

impl Default for CostumePref {
    fn default() -> Self {
        Self { phrase: "knight".into(), seed: 0, accent: None, head_scale: 1.0, hide: 0 }
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub name: String,
    pub corps: Corps,
    pub loadout: [String; 4],
    pub costume: CostumePref,
    pub voice: bool,
    pub third_person: bool,
    pub ono: OnoPref,
    pub ono_manga: bool,
    /// Shout callouts: On / Mine only / Off.
    pub shout: ShoutPref,
    /// Small numeric damage % at the bottom (off by default: the ring and the screen edges say it).
    pub show_percent: bool,
    /// Off-screen summon ping shows a tiny preview window of the construct (off = arrow only).
    pub summon_preview: bool,
    /// TRACKING: the enemy compass strip at the top of the screen (`present::tracking`).
    pub enemy_compass: bool,
    /// Player outlines (present::outline): on/off, width multiplier (0.5–2), colour-blind patterns.
    pub outlines: bool,
    pub outline_width: f32,
    pub outline_patterns: bool,
    /// CAMERA: default view (first / third), ADS hold-vs-toggle, ADS sensitivity + zoom speed.
    /// (`third_person` above mirrors the *live* view for readers such as the viewmodel.)
    pub camera: crate::present::camera3p::CameraPrefs,
    /// Last lobby settings (host).
    pub game: bd_sim::settings::GameSettings,
    /// "arena" | "tutorial" | other map ids.
    pub map: String,
    pub binds: BTreeMap<String, String>,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            name: "Lantern".into(),
            corps: Corps::Willpower,
            loadout: ["hammer".into(), "shield".into(), "wings".into(), "rocket launcher".into()],
            costume: CostumePref::default(),
            voice: true,
            third_person: false,
            ono: OnoPref::BigHits,
            ono_manga: false,
            shout: ShoutPref::On,
            show_percent: false,
            summon_preview: true,
            enemy_compass: true,
            outlines: true,
            outline_width: 1.0,
            outline_patterns: false,
            camera: Default::default(),
            game: Default::default(),
            map: "arena".into(),
            binds: KeyBinds::default().to_map(),
        }
    }
}

impl UiPrefs {
    /// `BD_UI_PREFS=<file>` overrides the location (tests / automation).
    pub fn path() -> PathBuf {
        if let Some(p) = std::env::var_os("BD_UI_PREFS") {
            return PathBuf::from(p);
        }
        Settings::path().with_file_name("ui_prefs.ron")
    }
    pub fn load() -> Self {
        let p = Self::path();
        let mut s: UiPrefs = std::fs::read_to_string(&p).ok().and_then(|t| ron::from_str(&t).ok()).unwrap_or_default();
        s.game = s.game.validated();
        s.costume.head_scale = if s.costume.head_scale.is_finite() { s.costume.head_scale.clamp(0.7, 1.35) } else { 1.0 };
        s.outline_width = if s.outline_width.is_finite() { s.outline_width.clamp(0.5, 2.0) } else { 1.0 };
        for l in s.loadout.iter_mut() {
            l.truncate(80);
        }
        s
    }
    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let t = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).map_err(std::io::Error::other)?;
        std::fs::write(p, t)
    }
}

pub struct PrefsPlugin;

impl Plugin for PrefsPlugin {
    fn build(&self, app: &mut App) {
        let prefs = UiPrefs::load();
        let binds = KeyBinds::from_map(&prefs.binds);
        app.insert_resource(prefs).insert_resource(binds).add_systems(PreUpdate, (autosave, mirror_ono))
            .add_systems(Update, sync_binds.after(super::UiSet::Input).before(crate::tick::FrameSet::Input));
    }
}

/// KeyBinds → prefs map + shell `Bindings` (movement keys). While typing (console / oath) the
/// keyboard tether + reel keys are parked on an unreachable key so letters never fire tethers; the
/// mouse side buttons and the wheel keep working.
fn sync_binds(kb: Res<KeyBinds>, focus: Res<crate::input::UiFocus>, mut prefs: ResMut<UiPrefs>, mut shell: ResMut<Bindings>) {
    if kb.is_changed() {
        let map = kb.to_map();
        if prefs.binds != map {
            prefs.binds = map;
        }
    }
    // Unbound keys map to an unreachable key so nothing fires.
    let none = KeyCode::F24;
    let k = |a: Action| kb.get(a).key().unwrap_or(none);
    let typing = focus.typing;
    let want = Bindings {
        forward: k(Action::Forward),
        back: k(Action::Back),
        left: k(Action::Left),
        right: k(Action::Right),
        jump: k(Action::Jump),
        crouch: k(Action::Crouch),
        boost: k(Action::Boost),
        tether_l: if typing { none } else { k(Action::TetherL) },
        tether_r: if typing { none } else { k(Action::TetherR) },
        reel: if typing { none } else { k(Action::Reel) },
        charge: if typing { none } else { k(Action::Charge) },
        dodge: if typing { none } else { k(Action::Dodge) },
    };
    let s = &*shell;
    let same = s.forward == want.forward
        && s.back == want.back
        && s.left == want.left
        && s.right == want.right
        && s.jump == want.jump
        && s.crouch == want.crouch
        && s.boost == want.boost
        && s.tether_l == want.tether_l
        && s.tether_r == want.tether_r
        && s.reel == want.reel
        && s.charge == want.charge
        && s.dodge == want.dodge;
    if !same {
        *shell = want;
    }
}

/// Comic-word mode / style → bd_fx (if the fx plugin is present).
fn mirror_ono(prefs: Res<UiPrefs>, ono: Option<ResMut<bd_fx::OnomatopoeiaSettings>>) {
    let Some(mut ono) = ono else { return };
    let mode = match prefs.ono {
        OnoPref::Off => bd_fx::OnoMode::Off,
        OnoPref::BigHits => bd_fx::OnoMode::BigHitsOnly,
        OnoPref::Full => bd_fx::OnoMode::FullComic,
    };
    let lang = if prefs.ono_manga { bd_fx::OnoLanguage::Manga } else { bd_fx::OnoLanguage::En };
    if ono.mode != mode || ono.language != lang {
        ono.mode = mode;
        ono.language = lang;
    }
}

fn autosave(prefs: Res<UiPrefs>, time: Res<Time<Real>>, mut dirty: Local<Option<f32>>) {
    let now = time.elapsed_secs();
    if prefs.is_changed() && !prefs.is_added() {
        *dirty = Some(now);
    }
    if let Some(t) = *dirty
        && now - t > 0.6
    {
        *dirty = None;
        if let Err(e) = prefs.save() {
            warn!("ui prefs: save failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_ids_roundtrip() {
        for a in Action::ALL {
            let b = a.default_bind();
            assert_eq!(Bind::from_id(&b.to_id()), Some(b), "{a:?}");
        }
        let mut kb = KeyBinds::default();
        kb.set(Action::Oath, Bind::Key(KeyCode::KeyW));
        assert_eq!(kb.get(Action::Forward), Bind::Unbound);
        assert_eq!(KeyBinds::from_map(&kb.to_map()), kb);
    }

    #[test]
    fn prefs_ron_roundtrip() {
        let p = UiPrefs::default();
        let t = ron::ser::to_string_pretty(&p, Default::default()).unwrap();
        let back: UiPrefs = ron::from_str(&t).unwrap();
        assert_eq!(p, back);
    }
}
