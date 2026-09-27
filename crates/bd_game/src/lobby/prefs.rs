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

    /// Inputs the game hard-wires, which no action may take: Esc (menu), F3 (perf), F4 (render),
    /// F11 (fullscreen), LMB (primary). RMB is the hard-wired secondary, so only AIM (which RMB does
    /// with a gun in hand) may hold it.
    pub fn reserved_for(self, a: Action) -> bool {
        match self {
            Bind::Key(k) => matches!(k, KeyCode::Escape | KeyCode::F3 | KeyCode::F4 | KeyCode::F11),
            Bind::Mouse(MouseButton::Left) => true,
            Bind::Mouse(MouseButton::Right) => a != Action::Aim,
            _ => false,
        }
    }
}

/// Hard-wired extra inputs (not listed as binds): C also crouches, the mouse side buttons also fire
/// the tethers. Each is live only while no action is bound to that input (binding C to something
/// else turns the C crouch off).
pub const ALIASES: [(Action, Bind); 3] = [
    (Action::Crouch, Bind::Key(KeyCode::KeyC)),
    (Action::TetherL, Bind::Mouse(MouseButton::Back)),
    (Action::TetherR, Bind::Mouse(MouseButton::Forward)),
];

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
    /// UNSTICK: to the safest spawn (also in the Esc menu). Unbound by default.
    Unstick,
}

impl Action {
    pub const ALL: [Action; 29] = [
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
        Action::Unstick,
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
            Action::Unstick => "unstick",
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
            Action::Unstick => "UNSTICK",
        }
    }

    pub fn keys_only(self) -> bool {
        matches!(
            self,
            Action::Forward | Action::Back | Action::Left | Action::Right | Action::Jump | Action::Crouch | Action::Boost | Action::TetherL | Action::TetherR | Action::Reel | Action::Charge | Action::Dodge
        )
    }

    /// Can this action hold `b`? (Keys-only actions refuse mouse buttons; reserved inputs never.)
    pub fn accepts(self, b: Bind) -> bool {
        match b {
            Bind::Unbound => true,
            Bind::Mouse(_) if self.keys_only() => false,
            _ => !b.reserved_for(self),
        }
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
            // UNSTICK: an escape hatch, not a combat key — nothing by default.
            Action::Unstick => return Bind::Unbound,
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
    /// One input, one action. Binding an input another action holds **swaps**: that action takes
    /// `a`'s previous input (or none, when it can't take it). Returns the displaced action. Callers
    /// refuse [`Bind::reserved_for`] inputs first ([`Self::try_set`]).
    pub fn set(&mut self, a: Action, b: Bind) -> Option<Action> {
        let old = self.get(a);
        let mut displaced = None;
        if b != Bind::Unbound && b != old {
            for other in Action::ALL {
                if other != a && self.get(other) == b {
                    self.0[other.index()] = if other.accepts(old) { old } else { Bind::Unbound };
                    displaced = Some(other);
                }
            }
        }
        self.0[a.index()] = b;
        displaced
    }

    /// Rebinding from the settings UI: refuses reserved / wrong-kind inputs.
    pub fn try_set(&mut self, a: Action, b: Bind) -> Result<Option<Action>, ()> {
        if !a.accepts(b) {
            return Err(());
        }
        Ok(self.set(a, b))
    }

    /// The hard-wired alias of `a` ([`ALIASES`]) while it is live (no action holds that input).
    pub fn alias(&self, a: Action) -> Option<Bind> {
        ALIASES.iter().find(|(x, _)| *x == a).map(|(_, b)| *b).filter(|b| !self.0.contains(b))
    }

    /// Pairs of actions sharing an input (always empty for a table built through [`Self::set`]).
    pub fn conflicts(&self) -> Vec<(Action, Action)> {
        let mut out = Vec::new();
        for (i, a) in Action::ALL.iter().enumerate() {
            for b in &Action::ALL[i + 1..] {
                if self.get(*a) != Bind::Unbound && self.get(*a) == self.get(*b) {
                    out.push((*a, *b));
                }
            }
        }
        out
    }

    /// Hand-edited / old prefs files: reserved or wrong-kind inputs fall back to the default, and
    /// an input held twice stays with the action whose default it is (the other gets its own
    /// default when free, else nothing).
    fn sanitized(mut self) -> Self {
        for a in Action::ALL {
            if !a.accepts(self.get(a)) {
                self.0[a.index()] = a.default_bind();
            }
        }
        while let Some(&(a, b)) = self.conflicts().first() {
            let bind = self.get(a);
            let loser = if b.default_bind() == bind && a.default_bind() != bind { a } else { b };
            let d = loser.default_bind();
            self.0[loser.index()] = if d != bind && !self.0.contains(&d) { d } else { Bind::Unbound };
        }
        self
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
        out.sanitized()
    }
}

// ------------------------------------------------------------------------------------------ prefs

/// Onomatopoeia mode (mirrors `bd_fx::OnoMode`, kept serialisable here).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnoPref {
    Off,
    BigHits,
    #[default]
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
            // COMIC TEXT (comic words + move titles; bd_fx text_budget): big hits / crits / finishers
            // only and fewer words near you; everything (light hits thinned); none.
            OnoPref::BigHits => "REDUCED",
            OnoPref::Full => "FULL",
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

/// READABILITY: the over-head damage % on other players (`readability::numbers`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnemyPercent {
    /// Only the enemy at / right next to the crosshair (never clutters an 8-player brawl).
    #[default]
    NearCrosshair,
    /// Every enemy in range.
    All,
    Off,
}

impl EnemyPercent {
    pub fn next(self, d: i32) -> Self {
        let all = [EnemyPercent::NearCrosshair, EnemyPercent::All, EnemyPercent::Off];
        let i = all.iter().position(|x| *x == self).unwrap_or(0) as i32;
        all[(i + d).rem_euclid(3) as usize]
    }
    pub fn label(self) -> &'static str {
        match self {
            EnemyPercent::NearCrosshair => "NEAR CROSSHAIR",
            EnemyPercent::All => "ALL",
            EnemyPercent::Off => "OFF",
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
    /// Random select: a fresh random look every time you enter a match (practice / host / join).
    pub random: bool,
}

impl Default for CostumePref {
    fn default() -> Self {
        Self { phrase: "knight".into(), seed: 0, accent: None, head_scale: 1.0, hide: 0, random: false }
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
    /// POWER HUD: the minimal strip (held powers, cooldowns, forms, willpower, pickup, statuses).
    /// On by default; the ring gems / motes are diegetic and always on.
    pub power_hud: bool,
    /// READABILITY: the willpower gauge around the crosshair (`power_hud::gauge`). On by default;
    /// off = the power strip's slim arc carries willpower instead (comfort / minimal HUD).
    pub crosshair_gauge: bool,
    /// READABILITY: other players' damage % above their heads (near the crosshair by default).
    pub enemy_percent: EnemyPercent,
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
    /// One-time migration (owner 2026-09-27: COMIC TEXT defaults to FULL): saves from before it
    /// still carry the old REDUCED default, so they are lifted to FULL once.
    pub comic_full_default: bool,
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
            ono: OnoPref::Full,
            ono_manga: false,
            shout: ShoutPref::On,
            show_percent: false,
            power_hud: true,
            crosshair_gauge: true,
            enemy_percent: EnemyPercent::NearCrosshair,
            summon_preview: true,
            enemy_compass: true,
            outlines: true,
            outline_width: 1.0,
            outline_patterns: false,
            camera: Default::default(),
            game: Default::default(),
            map: "arena".into(),
            binds: KeyBinds::default().to_map(),
            comic_full_default: true,
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
        let saved = std::fs::read_to_string(&p).ok().and_then(|t| ron::from_str::<UiPrefs>(&t).ok());
        let first_launch = saved.is_none() && !p.exists();
        let mut s = saved.unwrap_or_default();
        if first_launch {
            // First impression: a random iconic character instead of the plain default, kept from then on.
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
            s.costume = first_costume(t);
            let _ = s.save();
        }
        if !s.comic_full_default {
            if s.ono == OnoPref::BigHits {
                s.ono = OnoPref::Full;
            }
            s.comic_full_default = true;
            let _ = s.save();
        }
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

/// A random iconic character look (one of its variants) for a first launch.
fn first_costume(seed: u64) -> CostumePref {
    let mut r = crate::util::Rng::new(seed);
    let mut c = CostumePref::default();
    if let Some(name) = bd_shapes::costume::look::random_name(r.next_u32()) {
        let v = crate::lobby::costume::variant_count(name).unwrap_or(1).max(1) as u32;
        c.phrase = name.into();
        c.seed = r.next_u32() % v;
    }
    c
}

pub struct PrefsPlugin;

impl Plugin for PrefsPlugin {
    fn build(&self, app: &mut App) {
        let prefs = UiPrefs::load();
        let binds = KeyBinds::from_map(&prefs.binds);
        app.insert_resource(prefs).insert_resource(binds).add_systems(PreUpdate, mirror_ono)
            // `Last`: sees this frame's changes and an `AppExit` written this frame.
            .add_systems(Last, autosave)
            .add_systems(Update, sync_binds.after(super::UiSet::Input).before(crate::tick::FrameSet::Input));
    }
}

/// KeyBinds → prefs map + shell `Bindings` (movement keys).
fn sync_binds(kb: Res<KeyBinds>, focus: Res<crate::input::UiFocus>, mut prefs: ResMut<UiPrefs>, mut shell: ResMut<Bindings>) {
    if kb.is_changed() {
        let map = kb.to_map();
        if prefs.binds != map {
            prefs.binds = map;
        }
    }
    let want = shell_bindings(&kb, focus.typing);
    if *shell != want {
        *shell = want;
    }
}

/// The shell's device table for `kb`. While typing (console / oath) every keyboard key the shell
/// reads except movement (masked per tick by `LocalInputState::next_tick`) is parked on an
/// unreachable key so letters never fire tethers / reel / charge / dodge / the C crouch; the mouse
/// side buttons and the wheel keep working.
pub fn shell_bindings(kb: &KeyBinds, typing: bool) -> Bindings {
    // Unbound keys map to an unreachable key so nothing fires.
    let none = KeyCode::F24;
    let k = |a: Action| kb.get(a).key().unwrap_or(none);
    let live = |a: Action| if typing { none } else { k(a) };
    let mouse = |a: Action| match kb.alias(a) {
        Some(Bind::Mouse(m)) => Some(m),
        _ => None,
    };
    Bindings {
        forward: k(Action::Forward),
        back: k(Action::Back),
        left: k(Action::Left),
        right: k(Action::Right),
        jump: k(Action::Jump),
        crouch: k(Action::Crouch),
        crouch_alt: if typing { none } else { kb.alias(Action::Crouch).and_then(Bind::key).unwrap_or(none) },
        boost: k(Action::Boost),
        tether_l: live(Action::TetherL),
        tether_r: live(Action::TetherR),
        tether_l_mouse: mouse(Action::TetherL),
        tether_r_mouse: mouse(Action::TetherR),
        reel: live(Action::Reel),
        charge: live(Action::Charge),
        dodge: live(Action::Dodge),
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

/// Debounced save 0.6 s after the last change, or right away when the app is quitting.
fn autosave(prefs: Res<UiPrefs>, time: Res<Time<Real>>, mut exit: MessageReader<AppExit>, mut dirty: Local<Option<f32>>) {
    let now = time.elapsed_secs();
    if prefs.is_changed() && !prefs.is_added() {
        *dirty = Some(now);
    }
    let quitting = exit.read().count() > 0;
    if let Some(t) = *dirty
        && (now - t > 0.6 || quitting)
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
    fn first_launch_costume_is_a_random_iconic_character() {
        let picks: std::collections::HashSet<String> = (0..40u64).map(|s| first_costume(s * 7919 + 13).phrase).collect();
        assert!(picks.len() > 10, "varied: {picks:?}");
        for s in 0..40u64 {
            let c = first_costume(s * 7919 + 13);
            assert_ne!(c.phrase, "knight");
            let v = crate::lobby::costume::variant_count(&c.phrase).expect("a character look");
            assert!((c.seed as usize) < v.max(1));
        }
    }

    #[test]
    fn bind_ids_roundtrip() {
        for a in Action::ALL {
            let b = a.default_bind();
            assert_eq!(Bind::from_id(&b.to_id()), Some(b), "{a:?}");
        }
        let mut kb = KeyBinds::default();
        // Taking W swaps: FORWARD gets the oath's F.
        assert_eq!(kb.set(Action::Oath, Bind::Key(KeyCode::KeyW)), Some(Action::Forward));
        assert_eq!(kb.get(Action::Forward), Bind::Key(KeyCode::KeyF));
        assert_eq!(KeyBinds::from_map(&kb.to_map()), kb);
        // A keys-only action can't take the displaced mouse button: it ends up unbound.
        let mut kb = KeyBinds::default();
        assert_eq!(kb.set(Action::Transform, Bind::Key(KeyCode::KeyQ)), Some(Action::TetherL));
        assert_eq!(kb.get(Action::TetherL), Bind::Unbound);
        assert!(kb.conflicts().is_empty());
    }

    #[test]
    fn defaults_have_no_conflicts_and_no_caps_lock() {
        let kb = KeyBinds::default();
        assert!(kb.conflicts().is_empty(), "{:?}", kb.conflicts());
        for a in Action::ALL {
            let b = a.default_bind();
            assert_ne!(b, Bind::Key(KeyCode::CapsLock), "{a:?}: caps lock is yelling, never a default");
            assert!(a.accepts(b), "{a:?} default {b:?} is reserved / wrong kind");
            // Hard-wired aliases stay live with the defaults.
            assert!(!ALIASES.iter().any(|(_, x)| *x == b), "{a:?} default takes an alias");
        }
        for (a, b) in ALIASES {
            assert_eq!(kb.alias(a), Some(b));
        }
    }

    #[test]
    fn intended_defaults() {
        use KeyCode as K;
        let k = |a: Action| KeyBinds::default().get(a);
        let want = [
            (Action::Forward, Bind::Key(K::KeyW)),
            (Action::Jump, Bind::Key(K::Space)),
            (Action::Crouch, Bind::Key(K::ControlLeft)),
            (Action::Boost, Bind::Key(K::ShiftLeft)),
            (Action::TetherL, Bind::Key(K::KeyQ)),
            (Action::TetherR, Bind::Key(K::KeyE)),
            (Action::Reel, Bind::Key(K::KeyR)),
            (Action::Console, Bind::Key(K::Enter)),
            (Action::ConsoleAlt, Bind::Key(K::KeyT)),
            (Action::Voice, Bind::Key(K::KeyV)),
            (Action::Oath, Bind::Key(K::KeyF)),
            (Action::Charge, Bind::Key(K::KeyZ)),
            (Action::Dodge, Bind::Key(K::AltLeft)),
            (Action::Dismiss, Bind::Key(K::KeyX)),
            (Action::Power, Bind::Key(K::KeyG)),
            (Action::Transform, Bind::Mouse(MouseButton::Middle)),
            (Action::Scoreboard, Bind::Key(K::Tab)),
            (Action::Slot1, Bind::Key(K::Digit1)),
            (Action::Slot4, Bind::Key(K::Digit4)),
            (Action::ToggleView, Bind::Key(K::F5)),
            (Action::Shoulder, Bind::Key(K::KeyB)),
            (Action::Aim, Bind::Mouse(MouseButton::Right)),
            (Action::AltFire, Bind::Key(K::KeyH)),
        ];
        for (a, b) in want {
            assert_eq!(k(a), b, "{a:?}");
        }
    }

    #[test]
    fn reserved_inputs_refused() {
        let mut kb = KeyBinds::default();
        for b in [Bind::Key(KeyCode::F3), Bind::Key(KeyCode::F4), Bind::Key(KeyCode::F11), Bind::Key(KeyCode::Escape), Bind::Mouse(MouseButton::Left), Bind::Mouse(MouseButton::Right)] {
            assert!(kb.try_set(Action::Power, b).is_err(), "{b:?}");
        }
        assert!(kb.try_set(Action::Jump, Bind::Mouse(MouseButton::Middle)).is_err());
        assert_eq!(kb, KeyBinds::default());
        assert!(kb.try_set(Action::Aim, Bind::Mouse(MouseButton::Right)).is_ok());
    }

    #[test]
    fn binding_an_alias_input_turns_the_alias_off() {
        let mut kb = KeyBinds::default();
        kb.set(Action::AltFire, Bind::Key(KeyCode::KeyC));
        assert_eq!(kb.alias(Action::Crouch), None);
        let sh = shell_bindings(&kb, false);
        assert_eq!(sh.crouch_alt, KeyCode::F24);
        kb.set(Action::Transform, Bind::Mouse(MouseButton::Back));
        assert_eq!(shell_bindings(&kb, false).tether_l_mouse, None);
        assert_eq!(shell_bindings(&kb, false).tether_r_mouse, Some(MouseButton::Forward));
    }

    #[test]
    fn old_or_hand_edited_binds_are_sanitized() {
        // Old files: alt-fire on Alt (before dodge took it); a duplicate; a reserved key.
        let mut m = KeyBinds::default().to_map();
        m.insert("alt_fire".into(), "AltLeft".into());
        m.insert("power".into(), "F4".into());
        m.insert("dismiss".into(), "KeyW".into());
        m.insert("oath".into(), "CapsLock".into());
        m.remove("toggle_view");
        let kb = KeyBinds::from_map(&m);
        assert!(kb.conflicts().is_empty(), "{:?}", kb.conflicts());
        assert_eq!(kb.get(Action::Dodge), Bind::Key(KeyCode::AltLeft));
        assert_eq!(kb.get(Action::AltFire), Bind::Key(KeyCode::KeyH));
        assert_eq!(kb.get(Action::Forward), Bind::Key(KeyCode::KeyW));
        assert_eq!(kb.get(Action::Dismiss), Bind::Key(KeyCode::KeyX));
        assert_eq!(kb.get(Action::Power), Bind::Key(KeyCode::KeyG));
        assert_eq!(kb.get(Action::ToggleView), Bind::Key(KeyCode::F5));
        // The player's own choice survives (caps lock is allowed when they pick it).
        assert_eq!(kb.get(Action::Oath), Bind::Key(KeyCode::CapsLock));
    }

    #[test]
    fn typing_parks_every_letter_the_shell_reads() {
        let kb = KeyBinds::default();
        let t = shell_bindings(&kb, true);
        let f24 = KeyCode::F24;
        for k in [t.tether_l, t.tether_r, t.reel, t.charge, t.dodge, t.crouch_alt] {
            assert_eq!(k, f24);
        }
        // Mouse side buttons stay live while typing.
        assert_eq!(t.tether_l_mouse, Some(MouseButton::Back));
        assert_eq!(shell_bindings(&kb, false), crate::input::Bindings::default());
    }

    #[test]
    fn old_prefs_file_loads() {
        // A file from before the camera / outline / shout / compass settings existed.
        let old = r#"(name: "Old", corps: Rage, voice: false, third_person: true, ono: Full, binds: {"jump": "Space", "alt_fire": "KeyC"})"#;
        let p: UiPrefs = ron::from_str(old).unwrap();
        assert_eq!(p.name, "Old");
        assert!(p.third_person && !p.voice);
        assert_eq!(p.shout, ShoutPref::On);
        assert!(p.outlines && p.summon_preview && p.enemy_compass);
        assert!(p.crosshair_gauge && p.enemy_percent == EnemyPercent::NearCrosshair);
        assert_eq!(p.camera, crate::present::camera3p::CameraPrefs::default());
        let kb = KeyBinds::from_map(&p.binds);
        assert_eq!(kb.get(Action::AltFire), Bind::Key(KeyCode::KeyC));
        assert_eq!(kb.alias(Action::Crouch), None);
    }

    #[test]
    fn every_pref_roundtrips() {
        let mut p = UiPrefs::default();
        p.shout = ShoutPref::Mine;
        p.show_percent = true;
        p.power_hud = false;
        p.crosshair_gauge = false;
        p.enemy_percent = EnemyPercent::All;
        p.summon_preview = false;
        p.enemy_compass = false;
        p.outlines = false;
        p.outline_width = 1.75;
        p.outline_patterns = true;
        p.ono = OnoPref::Full;
        p.ono_manga = true;
        p.voice = false;
        p.camera.third = Some(true);
        p.camera.shoulder_left = true;
        p.camera.ads_toggle = true;
        p.camera.ads_sens = 0.8;
        p.camera.ads_speed = 2.0;
        let mut kb = KeyBinds::default();
        kb.set(Action::Dodge, Bind::Key(KeyCode::KeyC));
        p.binds = kb.to_map();
        let t = ron::ser::to_string_pretty(&p, Default::default()).unwrap();
        let back: UiPrefs = ron::from_str(&t).unwrap();
        assert_eq!(p, back);
        assert_eq!(KeyBinds::from_map(&back.binds), kb);
    }

    #[test]
    fn prefs_ron_roundtrip() {
        let p = UiPrefs::default();
        let t = ron::ser::to_string_pretty(&p, Default::default()).unwrap();
        let back: UiPrefs = ron::from_str(&t).unwrap();
        assert_eq!(p, back);
    }
}
