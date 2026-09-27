//! Menu shell (bevy_ui) — title, host / join / lobby, match rules (every `GameSettings` knob +
//! presets), customize (name, corps, costume creator with turntable, loadout), settings in tabs
//! ([`SettingsTab`]: Gameplay / Controls (+ rebindable keys) / Video / Audio / Comfort / HUD) and the
//! in-game Esc overlay.
//!
//! Keyboard first: ↑/↓ select, Enter/Space activate, ←/→ adjust, Esc back, Q/E switch settings tabs
//! (←/→ too on the tab strip); text rows take typing directly (Ctrl+V pastes). Mouse: hover selects,
//! click activates / steps up, right-click steps down, wheel scrolls long pages, click a tab.
//!
//! Rebinding (Controls tab): an input another action holds is **swapped** (a toast names the moved
//! action); reserved inputs (Esc, F3, F4, F11, LMB; RMB except for aim) are refused with a toast.
//!
//! Networking goes through NET's API only: `NetRequest` (Host / Join / UpdateProfile / SetSettings /
//! SetReady / StartMatch / Leave) and `NetStatus` (lobby code, players, ready, banner). NET moves the
//! app in game when the host starts.

use crate::arena::{ArenaWorld, CurrentMap, MapKind};
use crate::audio::PlaySfx;
use crate::input::UiFocus;
use crate::lobby::prefs::{Action, Bind, KeyBinds, UiPrefs};
use crate::net::{NetIdentity, NetPhase, NetRequest, NetRole, NetStatus};
use crate::settings::{ComfortSettings, Settings, FOV_MAX, FOV_MIN};
use crate::state::{AfterLoading, AppState, GameMode, InGameMenu};
use bd_core::corps::Corps;
use bd_sim::settings::{GameMode as SimMode, GameSettings, Preset};
use bd_sim::rules::Scoring;
use bd_sim::rules::UNLIMITED;
use bd_sim::{ConstructLimits, ScaleLimit, WillpowerMode};
use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseButtonInput};
use bevy::input::ButtonState;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui::{ColorStop, LinearGradient};

// ------------------------------------------------------------------------------------------ model

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPage {
    Title,
    Join,
    HostSetup,
    Lobby,
    Rules,
    Customize,
    Costume,
    Settings,
    Pause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Practice,
    Tutorial,
    HostSetup,
    Join,
    Customize,
    Settings,
    Quit,
    Back,
    Resume,
    Leave,
    Connect,
    Paste,
    OpenLobby,
    Ready,
    Start,
    Rules,
    Costume,
    Randomize,
    ResetKeys,
    Copy,
    Lobby,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    JoinCode,
    Name,
    Loadout(u8),
    Costume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Knob {
    // settings
    Sensitivity,
    Fov,
    InvertY,
    ThirdPerson,
    /// CAMERA: over the left / right shoulder.
    Shoulder,
    /// CAMERA: ADS hold / toggle, ADS sensitivity, ADS zoom speed.
    AdsMode,
    AdsSens,
    AdsSpeed,
    ShowPercent,
    /// POWER HUD: the minimal strip (held powers' icons, cooldown sweeps, willpower arc, statuses).
    PowerHud,
    SummonPreview,
    /// TRACKING: enemy compass strip on / off.
    EnemyCompass,
    Outlines,
    OutlineWidth,
    OutlinePatterns,
    Comfort,
    SpeedFov,
    /// Strafe / tether-swing camera lean.
    Roll,
    Shake,
    FovKick,
    LandingDip,
    Flashes,
    Vignette,
    Vsync,
    Fullscreen,
    Aa,
    /// Startup-only render options (applied on the next launch).
    LowLatency,
    Pipelined,
    Volume,
    MusicVol,
    WorldVol,
    AmbienceVol,
    FeedbackVol,
    MusicStyle,
    NightMode,
    Mono,
    Voice,
    Ono,
    OnoLang,
    Shout,
    // match rules
    Map,
    Preset,
    Mode,
    Scale,
    Willpower,
    /// CONSTRUCTS: Normal / Chaos / Custom quick switch over every [`Lim`].
    Caps,
    /// CONSTRUCTS: one per-kind cap (`MatchRules::limits`).
    Limit(Lim),
    /// CONSTRUCTS: big things need a pickup from the map (`MatchRules::powerups`).
    Powerups,
    Scoring,
    Lives,
    KoTarget,
    TimeLimit,
    Respawn,
    Shield,
    Knockback,
    Gravity,
    GasRegen,
    VoidKo,
    /// Hook-and-strike: tethers hook players (`MatchRules::tether_hooks_players`).
    TetherHooks,
    /// CAMERA: `MatchRules::allow_third_person`.
    AllowThird,
    Islands,
    Powers,
    Fruits,
    Stands,
    Rounds,
    RoundKo,
    RoundTime,
    PtsKo,
    PtsWin,
    PickSecs,
    BoomBust,
    FirstTwo,
    PoolCommon,
    PoolLegend,
    PoolCursed,
    // customize
    Corps,
    Variant,
    Accent,
    HeadScale,
    Acc(u32),
}

/// CONSTRUCTS section rows: one per [`ConstructLimits`] field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lim {
    Held,
    Worn,
    Structures,
    Objects,
    Creatures,
    Kaiju,
    Colossal,
    Total,
    GiantsOnMap,
}

impl Lim {
    const ALL: [Lim; 9] = [Lim::Held, Lim::Worn, Lim::Structures, Lim::Objects, Lim::Creatures, Lim::Kaiju, Lim::Colossal, Lim::Total, Lim::GiantsOnMap];

    fn label(self) -> &'static str {
        match self {
            Lim::Held => "HELD",
            Lim::Worn => "WORN",
            Lim::Structures => "STRUCTURES",
            Lim::Objects => "OBJECTS",
            Lim::Creatures => "CREATURES",
            Lim::Kaiju => "  KAIJU",
            Lim::Colossal => "COLOSSAL",
            Lim::Total => "TOTAL PER PLAYER",
            Lim::GiantsOnMap => "GIANTS ON MAP",
        }
    }

    fn get(self, l: &mut ConstructLimits) -> &mut u8 {
        match self {
            Lim::Held => &mut l.held,
            Lim::Worn => &mut l.worn,
            Lim::Structures => &mut l.structures,
            Lim::Objects => &mut l.objects,
            Lim::Creatures => &mut l.creatures,
            Lim::Kaiju => &mut l.kaiju,
            Lim::Colossal => &mut l.colossal,
            Lim::Total => &mut l.total,
            Lim::GiantsOnMap => &mut l.giants_on_map,
        }
    }

    /// Stepper range; `true` = one more step past `max` is UNLIMITED.
    fn range(self) -> (u8, u8, bool) {
        match self {
            Lim::Held | Lim::Worn => (0, 1, false),
            Lim::Total => (1, 20, true),
            _ => (0, 10, true),
        }
    }

    fn step(self, v: u8, dir: i32) -> u8 {
        let (lo, hi, unl) = self.range();
        if v == UNLIMITED && unl {
            return if dir < 0 { hi } else { UNLIMITED };
        }
        let n = v.min(hi) as i32 + dir;
        if n > hi as i32 && unl { UNLIMITED } else { n.clamp(lo as i32, hi as i32) as u8 }
    }
}

fn lim_text(v: u8) -> String {
    match v {
        0 => "OFF".into(),
        UNLIMITED => "UNLIMITED".into(),
        n => n.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Button(Act, &'static str),
    Value(Knob),
    Text(Field),
    Key(Action),
    Header(&'static str),
    /// The settings tab strip (always drawn at the top of the page).
    Tabs,
}

/// Settings page tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Gameplay,
    Controls,
    Video,
    Audio,
    Comfort,
    Hud,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 6] = [SettingsTab::Gameplay, SettingsTab::Controls, SettingsTab::Video, SettingsTab::Audio, SettingsTab::Comfort, SettingsTab::Hud];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::Gameplay => "GAMEPLAY",
            SettingsTab::Controls => "CONTROLS",
            SettingsTab::Video => "VIDEO",
            SettingsTab::Audio => "AUDIO",
            SettingsTab::Comfort => "COMFORT",
            SettingsTab::Hud => "HUD",
        }
    }

    /// `--menu <id>` (`keys` = controls).
    pub fn from_id(s: &str) -> Option<Self> {
        match s {
            "gameplay" => Some(SettingsTab::Gameplay),
            "controls" | "keys" => Some(SettingsTab::Controls),
            "video" | "graphics" => Some(SettingsTab::Video),
            "audio" | "sound" => Some(SettingsTab::Audio),
            "comfort" => Some(SettingsTab::Comfort),
            "hud" => Some(SettingsTab::Hud),
            _ => None,
        }
    }

    fn step(self, d: i32) -> Self {
        cycle(&Self::ALL, self, d)
    }
}

/// Key rows on the Controls tab, by section (every [`Action`] exactly once — tested).
const KEY_SECTIONS: [(&str, &[Action]); 4] = [
    ("MOVE", &[Action::Forward, Action::Back, Action::Left, Action::Right, Action::Jump, Action::Crouch, Action::Boost, Action::Dodge, Action::TetherL, Action::TetherR, Action::Reel]),
    (
        "RING",
        &[Action::Console, Action::ConsoleAlt, Action::Voice, Action::Slot1, Action::Slot2, Action::Slot3, Action::Slot4, Action::Oath, Action::Charge, Action::Power, Action::Transform, Action::Dismiss],
    ),
    ("VIEW / AIM", &[Action::ToggleView, Action::Shoulder, Action::Aim, Action::AltFire]),
    ("OTHER", &[Action::Scoreboard]),
];

impl Row {
    fn selectable(self) -> bool {
        !matches!(self, Row::Header(_))
    }
}

const ACCENTS: [(&str, Option<[f32; 3]>); 9] = [
    ("AUTO", None),
    ("GOLD", Some([1.0, 0.78, 0.2])),
    ("SILVER", Some([0.8, 0.84, 0.9])),
    ("CRIMSON", Some([0.75, 0.08, 0.12])),
    ("AZURE", Some([0.2, 0.55, 1.0])),
    ("JADE", Some([0.1, 0.75, 0.45])),
    ("ORCHID", Some([0.8, 0.35, 0.9])),
    ("CHARCOAL", Some([0.12, 0.12, 0.14])),
    ("BONE", Some([0.93, 0.9, 0.8])),
];

const ACC_SLOTS: [(u32, &str); 10] = [
    (bd_shapes::acc::HEAD, "HEADGEAR"),
    (bd_shapes::acc::FACE, "FACE"),
    (bd_shapes::acc::BACK, "BACK"),
    (bd_shapes::acc::SHOULDERS, "SHOULDERS"),
    (bd_shapes::acc::LEFT_ARM, "LEFT ARM"),
    (bd_shapes::acc::HIP, "HIP"),
    (bd_shapes::acc::TAIL, "TAIL"),
    (bd_shapes::acc::FEET, "FEET"),
    (bd_shapes::acc::ARMS, "ARMS"),
    (bd_shapes::acc::AURA, "AURA"),
];

/// Phrases for RANDOMIZE on the costume page (the player sees the result, never a list).
const RANDOM_COSTUMES: &[&str] = &[
    "knight with a fox head",
    "robot pirate",
    "shark",
    "toaster",
    "samurai",
    "wizard with an owl head",
    "astronaut",
    "cowboy with a horse head",
    "dragon",
    "viking",
    "ninja with a cat head",
    "deep sea diver",
    "mushroom",
    "penguin",
    "space marine",
    "lion",
];

struct Ctx<'a> {
    settings: &'a mut Settings,
    prefs: &'a mut UiPrefs,
    net: Option<&'a NetStatus>,
    voice: bool,
    in_game: bool,
}

impl Ctx<'_> {
    fn net_role(&self) -> NetRole {
        self.net.map_or(NetRole::Offline, |n| n.role.clone())
    }
    fn is_host(&self) -> bool {
        self.net_role() == NetRole::Host
    }
}

fn page_rows(page: MenuPage, tab: SettingsTab, c: &Ctx) -> Vec<Row> {
    use Knob as K;
    use Row::*;
    match page {
        MenuPage::Title => vec![
            Button(Act::Practice, "PRACTICE"),
            Button(Act::Tutorial, "TUTORIAL"),
            Button(Act::HostSetup, "HOST"),
            Button(Act::Join, "JOIN"),
            Button(Act::Customize, "CUSTOMIZE"),
            Button(Act::Settings, "SETTINGS"),
            Button(Act::Quit, "QUIT"),
        ],
        MenuPage::Join => vec![Text(Field::JoinCode), Button(Act::Paste, "PASTE"), Button(Act::Connect, "CONNECT"), Button(Act::Back, "BACK")],
        MenuPage::HostSetup => vec![Value(K::Map), Value(K::Preset), Value(K::Mode), Button(Act::Rules, "MATCH RULES"), Button(Act::OpenLobby, "OPEN LOBBY"), Button(Act::Back, "BACK")],
        MenuPage::Lobby => {
            let mut v = Vec::new();
            if c.net.is_some_and(|n| n.lobby_code.is_some() || (n.role == NetRole::Host && !n.lan.is_empty())) {
                v.push(Button(Act::Copy, "COPY CODE"));
            }
            let me_ready = c.net.and_then(|n| n.me.and_then(|me| n.players.iter().find(|p| p.id == me))).is_some_and(|p| p.ready);
            v.push(Button(Act::Ready, if me_ready { "NOT READY" } else { "READY" }));
            if c.is_host() {
                v.push(Button(Act::Start, "START MATCH"));
                v.push(Button(Act::Rules, "MATCH RULES"));
            }
            v.push(Button(Act::Customize, "CUSTOMIZE"));
            if c.in_game {
                v.push(Button(Act::Back, "BACK"));
            }
            v.push(Button(Act::Leave, "LEAVE"));
            v
        }
        MenuPage::Rules => {
            let mut v = vec![Header("MATCH"), Value(K::Preset), Value(K::Mode)];
            if c.prefs.game.mode != SimMode::Party {
                // Party rounds are always timed KOs (their own knobs below).
                v.push(Value(K::Scoring));
                v.push(if matches!(c.prefs.game.rules.scoring, Scoring::Stock { .. }) { Value(K::Lives) } else { Value(K::KoTarget) });
            } else {
                v.push(Value(K::KoTarget));
            }
            v.push(Value(K::TimeLimit));
            v.extend([Header("ARENA"), Value(K::Respawn), Value(K::Shield), Value(K::VoidKo), Value(K::Islands)]);
            v.extend([Header("RING"), Value(K::Scale), Value(K::Willpower), Value(K::Powers)]);
            if c.prefs.game.rules.powers_enabled {
                v.extend([Value(K::Fruits), Value(K::Stands)]);
            }
            v.extend([Header("CONSTRUCTS"), Value(K::Powerups), Value(K::Caps)]);
            for l in Lim::ALL {
                // Colossal things only exist in no-limits lobbies.
                if l != Lim::Colossal || c.prefs.game.rules.scale == ScaleLimit::NoLimits {
                    v.push(Value(K::Limit(l)));
                }
            }
            v.extend([Header("PHYSICS"), Value(K::Knockback), Value(K::Gravity), Value(K::GasRegen), Value(K::TetherHooks)]);
            v.extend([Header("VIEW"), Value(K::AllowThird)]);
            if c.prefs.game.mode == SimMode::Party {
                v.extend([
                    Header("PARTY"),
                    Value(K::Rounds),
                    Value(K::RoundKo),
                    Value(K::RoundTime),
                    Value(K::PtsKo),
                    Value(K::PtsWin),
                    Value(K::PickSecs),
                    Value(K::BoomBust),
                    Value(K::FirstTwo),
                    Value(K::PoolCommon),
                    Value(K::PoolLegend),
                    Value(K::PoolCursed),
                ]);
            }
            v.push(Button(Act::Back, "BACK"));
            v
        }
        MenuPage::Customize => vec![
            Text(Field::Name),
            Value(K::Corps),
            Button(Act::Costume, "COSTUME"),
            Header("QUICK CAST"),
            Text(Field::Loadout(0)),
            Text(Field::Loadout(1)),
            Text(Field::Loadout(2)),
            Text(Field::Loadout(3)),
            Button(Act::Back, "BACK"),
        ],
        MenuPage::Costume => {
            let mut v = vec![Text(Field::Costume), Value(K::Variant), Button(Act::Randomize, "RANDOMIZE"), Value(K::Accent), Value(K::HeadScale), Header("ACCESSORIES")];
            v.extend(ACC_SLOTS.iter().map(|(b, _)| Value(K::Acc(*b))));
            v.push(Button(Act::Back, "BACK"));
            v
        }
        MenuPage::Settings => {
            let mut v = vec![Tabs];
            match tab {
                SettingsTab::Gameplay => {
                    v.extend([Header("VIEW"), Value(K::ThirdPerson), Value(K::Shoulder)]);
                    v.extend([Header("AIM DOWN SIGHTS"), Value(K::AdsMode), Value(K::AdsSpeed)]);
                    if c.voice {
                        v.extend([Header("RING"), Value(K::Voice)]);
                    }
                }
                SettingsTab::Controls => {
                    v.extend([Header("MOUSE"), Value(K::Sensitivity), Value(K::AdsSens), Value(K::InvertY)]);
                    for (title, actions) in KEY_SECTIONS {
                        v.push(Header(title));
                        for &a in actions {
                            if a == Action::Voice && !c.voice {
                                continue;
                            }
                            v.push(Key(a));
                        }
                    }
                    v.push(Button(Act::ResetKeys, "RESET KEYS TO DEFAULTS"));
                }
                SettingsTab::Video => {
                    v.extend([Header("DISPLAY"), Value(K::Fov), Value(K::Fullscreen), Value(K::Vsync), Value(K::Aa)]);
                    v.extend([Header("LATENCY (NEXT LAUNCH)"), Value(K::LowLatency), Value(K::Pipelined)]);
                }
                SettingsTab::Audio => {
                    v.extend([Header("VOLUME"), Value(K::Volume), Value(K::MusicVol), Value(K::WorldVol), Value(K::AmbienceVol), Value(K::FeedbackVol)]);
                    v.extend([Header("MIX"), Value(K::MusicStyle), Value(K::NightMode), Value(K::Mono)]);
                }
                SettingsTab::Comfort => {
                    v.extend([Value(K::Comfort), Value(K::SpeedFov), Value(K::Roll), Value(K::Shake), Value(K::FovKick), Value(K::LandingDip), Value(K::Flashes), Value(K::Vignette)]);
                }
                SettingsTab::Hud => {
                    v.extend([Header("PLAYERS"), Value(K::Outlines), Value(K::OutlineWidth), Value(K::OutlinePatterns), Value(K::EnemyCompass), Value(K::ShowPercent), Value(K::PowerHud)]);
                    v.extend([Header("WORDS"), Value(K::Shout), Value(K::Ono), Value(K::OnoLang)]);
                    v.extend([Header("SUMMONS"), Value(K::SummonPreview)]);
                }
            }
            v.push(Button(Act::Back, "BACK"));
            v
        }
        MenuPage::Pause => {
            let mut v = vec![Button(Act::Resume, "RESUME")];
            if c.net_role() != NetRole::Offline {
                v.push(Button(Act::Lobby, "LOBBY"));
            }
            v.extend([Button(Act::Customize, "CUSTOMIZE"), Button(Act::Settings, "SETTINGS"), Button(Act::Leave, "LEAVE MATCH")]);
            v
        }
    }
}

/// Stock scoring: lives when switched on / the most the knob allows.
const STOCK_DEFAULT_LIVES: u8 = 3;
const STOCK_MAX_LIVES: u8 = 9;

fn on_off(b: bool) -> String {
    if b { "ON".into() } else { "OFF".into() }
}

fn pct(x: f32) -> String {
    if x <= 0.0 { "OFF".into() } else { format!("{:.0}%", x * 100.0) }
}

fn step4(x: f32, d: f32, max: f32) -> f32 {
    ((x + d * 0.25) * 4.0).round().clamp(0.0, max * 4.0) / 4.0
}

fn mmss(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

fn map_of(p: &UiPrefs) -> MapKind {
    MapKind::from_cli(&p.map).filter(|m| *m != MapKind::Tutorial).unwrap_or(MapKind::Arena)
}

fn preset_of(g: &GameSettings) -> Option<Preset> {
    Preset::ALL.into_iter().find(|p| GameSettings::preset(*p) == *g)
}

fn knob_text(k: Knob, c: &Ctx) -> (String, String) {
    let s = &*c.settings;
    let g = &c.prefs.game;
    let r = &g.rules;
    let pa = &g.party;
    let lbl = |x: &str| x.to_string();
    match k {
        Knob::Sensitivity => (lbl("SENSITIVITY"), format!("{:.2}", s.sensitivity)),
        Knob::Fov => (lbl("FIELD OF VIEW"), format!("{:.0}", s.fov)),
        Knob::InvertY => (lbl("INVERT Y"), on_off(s.invert_y)),
        Knob::ThirdPerson => (lbl("DEFAULT VIEW"), if c.prefs.camera.third.unwrap_or(c.prefs.third_person) { "THIRD PERSON".into() } else { "FIRST PERSON".into() }),
        Knob::Shoulder => (lbl("SHOULDER"), if c.prefs.camera.shoulder_left { "LEFT".into() } else { "RIGHT".into() }),
        Knob::AdsMode => (lbl("ADS MODE"), if c.prefs.camera.ads_toggle { "TOGGLE".into() } else { "HOLD".into() }),
        Knob::AdsSens => (lbl("ADS SENSITIVITY"), format!("{:.2}X", c.prefs.camera.ads_sens())),
        Knob::AdsSpeed => (lbl("ADS ZOOM SPEED"), format!("{:.2}X", c.prefs.camera.ads_speed())),
        Knob::ShowPercent => (lbl("DAMAGE % NUMBER"), on_off(c.prefs.show_percent)),
        Knob::PowerHud => (lbl("POWER HUD"), on_off(c.prefs.power_hud)),
        Knob::SummonPreview => (lbl("SUMMON PREVIEW"), on_off(c.prefs.summon_preview)),
        Knob::EnemyCompass => (lbl("ENEMY COMPASS"), on_off(c.prefs.enemy_compass)),
        Knob::Outlines => (lbl("PLAYER OUTLINES"), on_off(c.prefs.outlines)),
        Knob::OutlineWidth => (lbl("  OUTLINE THICKNESS"), format!("{:.0}%", c.prefs.outline_width * 100.0)),
        Knob::OutlinePatterns => (lbl("  COLOUR-BLIND PATTERNS"), on_off(c.prefs.outline_patterns)),
        Knob::Comfort => (lbl("COMFORT PRESET"), on_off(s.comfort.is_comfort_preset())),
        Knob::SpeedFov => (lbl("  SPEED FOV"), on_off(s.comfort.speed_fov)),
        Knob::Roll => (lbl("  STRAFE LEAN"), on_off(s.comfort.roll_on())),
        Knob::Shake => (lbl("  CAMERA SHAKE"), pct(s.comfort.shake)),
        Knob::FovKick => (lbl("  FOV KICKS"), pct(s.comfort.fov_kick)),
        Knob::LandingDip => (lbl("  LANDING DIP"), pct(s.comfort.landing_dip)),
        Knob::Flashes => (lbl("  SCREEN FLASHES"), pct(s.comfort.flashes)),
        Knob::Vignette => (lbl("  COMFORT VIGNETTE"), pct(s.comfort.vignette)),
        Knob::Vsync => (lbl("VSYNC"), on_off(s.vsync)),
        Knob::Fullscreen => (lbl("FULLSCREEN"), on_off(s.fullscreen)),
        Knob::Aa => (lbl("ANTI-ALIASING"), s.antialiasing.label().into()),
        Knob::LowLatency => (lbl("LOW-LATENCY QUEUE"), on_off(s.low_latency)),
        Knob::Pipelined => (lbl("PIPELINED RENDERING"), on_off(s.pipelined_rendering)),
        Knob::Volume => (lbl("MASTER"), format!("{:.0}%", s.master_volume * 100.0)),
        Knob::MusicVol => (lbl("  MUSIC"), format!("{:.0}%", s.audio.music * 100.0)),
        Knob::WorldVol => (lbl("  WORLD"), format!("{:.0}%", s.audio.world * 100.0)),
        Knob::AmbienceVol => (lbl("  AMBIENCE"), format!("{:.0}%", s.audio.ambience * 100.0)),
        Knob::FeedbackVol => (lbl("  HIT FEEDBACK"), format!("{:.0}%", s.audio.feedback * 100.0)),
        Knob::MusicStyle => (lbl("MUSIC STYLE"), s.audio.music_style.label().into()),
        Knob::NightMode => (lbl("REDUCED DYNAMIC RANGE"), on_off(s.audio.night_mode)),
        Knob::Mono => (lbl("MONO AUDIO"), on_off(s.audio.mono)),
        Knob::Voice => (lbl("VOICE CASTING"), on_off(c.prefs.voice)),
        Knob::Ono => (lbl("COMIC WORDS"), c.prefs.ono.label().into()),
        Knob::OnoLang => (lbl("COMIC STYLE"), if c.prefs.ono_manga { "MANGA".into() } else { "CLASSIC".into() }),
        Knob::Shout => (lbl("SHOUT WORDS"), c.prefs.shout.label().into()),
        Knob::Map => (lbl("MAP"), match map_of(c.prefs) {
            MapKind::Miami => "MIAMI".into(),
            MapKind::Orbit => "ORBIT".into(),
            MapKind::Backrooms => "BACKROOMS".into(),
            MapKind::Sakura => "SAKURA KEEP".into(),
            MapKind::Ember => "EMBER".into(),
            MapKind::Mirrorfold => "MIRRORFOLD".into(),
            MapKind::Casino => "HIGH ROLLER".into(),
            MapKind::Valley => "VALLEY OF THE END".into(),
            MapKind::Cybertron => "CYBERTRON".into(),
            MapKind::Lair => "TURTLE LAIR".into(),
            MapKind::GiantsRoom => "GIANT'S ROOM".into(),
            MapKind::Hyperline => "HYPERLINE".into(),
            _ => "ARCHIPELAGO".into(),
        }),
        Knob::Preset => (lbl("PRESET"), preset_of(g).map_or("CUSTOM".into(), |p| p.name().to_uppercase())),
        Knob::Mode => (lbl("MODE"), if g.mode == SimMode::Party { "PARTY".into() } else { "FREE FOR ALL".into() }),
        Knob::Scale => (lbl("CONSTRUCT SIZE"), match r.scale {
            ScaleLimit::Normal => "NORMAL".into(),
            ScaleLimit::Huge => "HUGE".into(),
            ScaleLimit::NoLimits => "NO LIMITS".into(),
        }),
        Knob::Willpower => (lbl("WILLPOWER"), match r.willpower {
            WillpowerMode::Normal => "NORMAL".into(),
            WillpowerMode::Double => "DOUBLE".into(),
            WillpowerMode::Infinite => "INFINITE".into(),
        }),
        Knob::Caps => (lbl("CAPS"), if r.limits == ConstructLimits::default() {
            "NORMAL".into()
        } else if r.limits == ConstructLimits::chaos() {
            "CHAOS".into()
        } else {
            "CUSTOM".into()
        }),
        Knob::Powerups => (lbl("POWER-UPS"), on_off(r.powerups)),
        Knob::Limit(l) => {
            let mut lim = r.limits;
            (lbl(l.label()), lim_text(*l.get(&mut lim)))
        }
        Knob::Scoring => (lbl("SCORING"), match r.scoring {
            Scoring::TimedKos => "TIMED KOS".into(),
            Scoring::Stock { .. } => "STOCK".into(),
        }),
        Knob::Lives => (lbl("LIVES"), match r.scoring {
            Scoring::Stock { lives } => lives.max(1).to_string(),
            Scoring::TimedKos => "-".into(),
        }),
        Knob::KoTarget => (lbl("KOS TO WIN"), r.ko_target.to_string()),
        Knob::TimeLimit => (lbl("TIME LIMIT"), mmss(r.time_limit)),
        Knob::Respawn => (lbl("RESPAWN"), format!("{:.1} S", r.respawn_secs)),
        Knob::Shield => (lbl("SPAWN SHIELD"), format!("{:.1} S", r.spawn_shield_secs)),
        Knob::Knockback => (lbl("KNOCKBACK"), format!("{:.2}X", r.knockback_scale)),
        Knob::Gravity => (lbl("GRAVITY"), format!("{:.2}X", g.gravity_scale)),
        Knob::GasRegen => (lbl("GAS REGEN"), format!("{:.2}X", g.gas_regen_scale)),
        Knob::VoidKo => (lbl("VOID KO"), on_off(r.void_ko)),
        Knob::TetherHooks => (lbl("TETHERS HOOK PLAYERS"), on_off(r.tether_hooks_players)),
        Knob::AllowThird => (lbl("ALLOW THIRD PERSON"), on_off(r.allow_third_person)),
        Knob::Islands => (lbl("REBUILD ISLANDS"), on_off(r.regen_islands_between_rounds)),
        Knob::Powers => (lbl("POWERS"), on_off(r.powers_enabled)),
        Knob::Fruits => (lbl("  FRUITS"), on_off(r.devil_fruits)),
        Knob::Stands => (lbl("  STANDS"), on_off(r.stands)),
        Knob::Rounds => (lbl("ROUNDS"), pa.rounds.to_string()),
        Knob::RoundKo => (lbl("ROUND KOS"), pa.round_ko_target.to_string()),
        Knob::RoundTime => (lbl("ROUND TIME"), mmss(pa.round_time)),
        Knob::PtsKo => (lbl("POINTS PER KO"), pa.points_per_ko.to_string()),
        Knob::PtsWin => (lbl("ROUND WIN BONUS"), pa.points_round_win.to_string()),
        Knob::PickSecs => (lbl("PICK TIMER"), format!("{:.0} S", pa.pick_secs)),
        Knob::BoomBust => (lbl("BOOM OR BUST"), on_off(pa.boom_or_bust)),
        Knob::FirstTwo => (lbl("LEADER GETS 2 CARDS"), on_off(pa.first_place_two_choices)),
        Knob::PoolCommon => (lbl("COMMON CARDS"), on_off(pa.pool_common)),
        Knob::PoolLegend => (lbl("LEGENDARY CARDS"), on_off(pa.pool_legendary)),
        Knob::PoolCursed => (lbl("CURSED CARDS"), on_off(pa.pool_cursed)),
        Knob::Corps => (lbl("CORPS"), c.prefs.corps.name().to_uppercase()),
        Knob::Variant => {
            // Character looks have their own variant count (blindfold / sunglasses / eyes…).
            let n = crate::lobby::costume::variant_count(&c.prefs.costume.phrase).unwrap_or(4).max(1) as u32;
            (lbl("VARIANT"), format!("{} / {n}", c.prefs.costume.seed % n + 1))
        }
        Knob::Accent => (lbl("ACCENT"), ACCENTS.iter().find(|(_, a)| *a == c.prefs.costume.accent).map_or("CUSTOM", |(n, _)| n).into()),
        Knob::HeadScale => (lbl("HEAD SIZE"), format!("{:.0}%", c.prefs.costume.head_scale * 100.0)),
        Knob::Acc(bit) => (ACC_SLOTS.iter().find(|(b, _)| *b == bit).map_or("?", |(_, n)| n).to_string(), on_off(c.prefs.costume.hide & bit == 0)),
    }
}

fn cycle<T: Copy + PartialEq>(all: &[T], cur: T, d: i32) -> T {
    let i = all.iter().position(|x| *x == cur).unwrap_or(0) as i32;
    all[(i + d).rem_euclid(all.len() as i32) as usize]
}

/// Returns true if the change touches the lobby GameSettings.
fn step_knob(k: Knob, c: &mut Ctx, dir: i32, fine: bool) -> bool {
    let d = dir as f32;
    let s = &mut *c.settings;
    let g = &mut c.prefs.game;
    let mut game = true;
    match k {
        Knob::Sensitivity => {
            let step = if fine { 0.01 } else { 0.05 };
            s.sensitivity = (((s.sensitivity + d * step) * 100.0).round() / 100.0).clamp(0.05, 20.0);
            game = false;
        }
        Knob::Fov => {
            s.fov = (s.fov + d * if fine { 1.0 } else { 5.0 }).clamp(FOV_MIN, FOV_MAX).round();
            game = false;
        }
        Knob::InvertY => {
            s.invert_y = !s.invert_y;
            game = false;
        }
        Knob::ThirdPerson => {
            let third = c.prefs.camera.third.unwrap_or(c.prefs.third_person);
            c.prefs.camera.third = Some(!third);
            game = false;
        }
        Knob::Shoulder => {
            c.prefs.camera.shoulder_left = !c.prefs.camera.shoulder_left;
            game = false;
        }
        Knob::AdsMode => {
            c.prefs.camera.ads_toggle = !c.prefs.camera.ads_toggle;
            game = false;
        }
        Knob::AdsSens => {
            let step = if fine { 0.01 } else { 0.05 };
            c.prefs.camera.ads_sens = (((c.prefs.camera.ads_sens() + d * step) * 100.0).round() / 100.0).clamp(0.2, 2.5);
            game = false;
        }
        Knob::AdsSpeed => {
            c.prefs.camera.ads_speed = (((c.prefs.camera.ads_speed() + d * 0.25) * 4.0).round() / 4.0).clamp(0.25, 4.0);
            game = false;
        }
        Knob::ShowPercent => {
            c.prefs.show_percent = !c.prefs.show_percent;
            game = false;
        }
        Knob::PowerHud => {
            c.prefs.power_hud = !c.prefs.power_hud;
            game = false;
        }
        Knob::SummonPreview => {
            c.prefs.summon_preview = !c.prefs.summon_preview;
            game = false;
        }
        Knob::EnemyCompass => {
            c.prefs.enemy_compass = !c.prefs.enemy_compass;
            game = false;
        }
        Knob::Outlines => {
            c.prefs.outlines = !c.prefs.outlines;
            game = false;
        }
        Knob::OutlineWidth => {
            c.prefs.outline_width = (((c.prefs.outline_width + d * 0.25) * 4.0).round() / 4.0).clamp(0.5, 2.0);
            game = false;
        }
        Knob::OutlinePatterns => {
            c.prefs.outline_patterns = !c.prefs.outline_patterns;
            game = false;
        }
        Knob::Comfort => {
            s.comfort = if s.comfort.is_comfort_preset() { ComfortSettings::default() } else { ComfortSettings::COMFORT };
            game = false;
        }
        Knob::SpeedFov => {
            s.comfort.speed_fov = !s.comfort.speed_fov;
            game = false;
        }
        Knob::Roll => {
            s.comfort.strafe_roll = Some(!s.comfort.roll_on());
            game = false;
        }
        Knob::Shake | Knob::FovKick | Knob::LandingDip | Knob::Flashes | Knob::Vignette => {
            let cm = &mut s.comfort;
            match k {
                Knob::Shake => cm.shake = step4(cm.shake, d, 2.0),
                Knob::FovKick => cm.fov_kick = step4(cm.fov_kick, d, 2.0),
                Knob::LandingDip => cm.landing_dip = step4(cm.landing_dip, d, 2.0),
                Knob::Flashes => cm.flashes = step4(cm.flashes, d, 1.0),
                _ => cm.vignette = step4(cm.vignette, d, 1.0),
            }
            game = false;
        }
        Knob::Vsync => {
            s.vsync = !s.vsync;
            game = false;
        }
        Knob::Fullscreen => {
            s.fullscreen = !s.fullscreen;
            game = false;
        }
        Knob::Aa => {
            s.antialiasing = if dir > 0 { s.antialiasing.next() } else { s.antialiasing.prev() };
            game = false;
        }
        Knob::LowLatency => {
            s.low_latency = !s.low_latency;
            game = false;
        }
        Knob::Pipelined => {
            s.pipelined_rendering = !s.pipelined_rendering;
            game = false;
        }
        Knob::Volume => {
            s.master_volume = ((s.master_volume + d * 0.05) * 20.0).round().clamp(0.0, 20.0) / 20.0;
            game = false;
        }
        Knob::MusicVol | Knob::WorldVol | Knob::AmbienceVol | Knob::FeedbackVol => {
            let a = &mut s.audio;
            let v = match k {
                Knob::MusicVol => &mut a.music,
                Knob::WorldVol => &mut a.world,
                Knob::AmbienceVol => &mut a.ambience,
                _ => &mut a.feedback,
            };
            *v = ((*v + d * 0.05) * 20.0).round().clamp(0.0, 20.0) / 20.0;
            game = false;
        }
        Knob::MusicStyle => {
            s.audio.music_style = s.audio.music_style.next(dir);
            game = false;
        }
        Knob::NightMode => {
            s.audio.night_mode = !s.audio.night_mode;
            game = false;
        }
        Knob::Mono => {
            s.audio.mono = !s.audio.mono;
            game = false;
        }
        Knob::Voice => {
            c.prefs.voice = !c.prefs.voice;
            game = false;
        }
        Knob::Ono => {
            c.prefs.ono = c.prefs.ono.next(dir);
            game = false;
        }
        Knob::OnoLang => {
            c.prefs.ono_manga = !c.prefs.ono_manga;
            game = false;
        }
        Knob::Shout => {
            c.prefs.shout = c.prefs.shout.next(dir);
            game = false;
        }
        Knob::Map => {
            let m = cycle(&[MapKind::Arena, MapKind::Miami, MapKind::Orbit, MapKind::Backrooms, MapKind::Sakura, MapKind::Ember, MapKind::Mirrorfold, MapKind::Casino, MapKind::Valley, MapKind::Cybertron, MapKind::Lair, MapKind::GiantsRoom, MapKind::Hyperline], map_of(c.prefs), dir);
            c.prefs.map = match m {
                MapKind::Miami => "miami",
                MapKind::Orbit => "orbit",
                MapKind::Backrooms => "backrooms",
                MapKind::Sakura => "sakura",
                MapKind::Ember => "ember",
                MapKind::Mirrorfold => "mirror",
                MapKind::Casino => "casino",
                MapKind::Valley => "valley",
                MapKind::Cybertron => "cybertron",
                MapKind::Lair => "lair",
                MapKind::GiantsRoom => "giant",
                MapKind::Hyperline => "train",
                _ => "arena",
            }
            .into();
        }
        Knob::Preset => {
            let cur = preset_of(g);
            let next = match cur {
                Some(p) => cycle(&Preset::ALL, p, dir),
                None => Preset::Classic,
            };
            *g = GameSettings::preset(next);
        }
        Knob::Mode => g.mode = if g.mode == SimMode::Party { SimMode::Ffa } else { SimMode::Party },
        Knob::Scale => g.rules.scale = cycle(&[ScaleLimit::Normal, ScaleLimit::Huge, ScaleLimit::NoLimits], g.rules.scale, dir),
        Knob::Willpower => g.rules.willpower = cycle(&[WillpowerMode::Normal, WillpowerMode::Double, WillpowerMode::Infinite], g.rules.willpower, dir),
        Knob::Caps => g.rules.limits = if g.rules.limits == ConstructLimits::default() { ConstructLimits::chaos() } else { ConstructLimits::default() },
        Knob::Powerups => g.rules.powerups = !g.rules.powerups,
        Knob::Limit(l) => {
            let v = l.get(&mut g.rules.limits);
            *v = l.step(*v, dir);
        }
        Knob::Scoring => {
            g.rules.scoring = match g.rules.scoring {
                Scoring::TimedKos => Scoring::Stock { lives: STOCK_DEFAULT_LIVES },
                Scoring::Stock { .. } => Scoring::TimedKos,
            }
        }
        Knob::Lives => {
            if let Scoring::Stock { lives } = &mut g.rules.scoring {
                *lives = (*lives as i32 + dir).clamp(1, STOCK_MAX_LIVES as i32) as u8;
            }
        }
        Knob::KoTarget => g.rules.ko_target = (g.rules.ko_target as i32 + dir * if fine { 1 } else { 5 }).clamp(1, 200) as u16,
        Knob::TimeLimit => g.rules.time_limit = (g.rules.time_limit + d * 60.0).clamp(60.0, 3600.0),
        Knob::Respawn => g.rules.respawn_secs = (g.rules.respawn_secs + d * 0.5).clamp(0.0, 30.0),
        Knob::Shield => g.rules.spawn_shield_secs = (g.rules.spawn_shield_secs + d * 0.5).clamp(0.0, 10.0),
        Knob::Knockback => g.rules.knockback_scale = ((g.rules.knockback_scale + d * 0.25) * 4.0).round().clamp(0.0, 16.0) / 4.0,
        Knob::Gravity => g.gravity_scale = ((g.gravity_scale + d * 0.25) * 4.0).round().clamp(1.0, 12.0) / 4.0,
        Knob::GasRegen => g.gas_regen_scale = ((g.gas_regen_scale + d * 0.25) * 4.0).round().clamp(0.0, 16.0) / 4.0,
        Knob::VoidKo => g.rules.void_ko = !g.rules.void_ko,
        Knob::TetherHooks => g.rules.tether_hooks_players = !g.rules.tether_hooks_players,
        Knob::AllowThird => g.rules.allow_third_person = !g.rules.allow_third_person,
        Knob::Islands => g.rules.regen_islands_between_rounds = !g.rules.regen_islands_between_rounds,
        Knob::Powers => g.rules.powers_enabled = !g.rules.powers_enabled,
        Knob::Fruits => g.rules.devil_fruits = !g.rules.devil_fruits,
        Knob::Stands => g.rules.stands = !g.rules.stands,
        Knob::Rounds => g.party.rounds = (g.party.rounds as i32 + dir).clamp(1, 15) as u8,
        Knob::RoundKo => g.party.round_ko_target = (g.party.round_ko_target as i32 + dir).clamp(1, 100) as u16,
        Knob::RoundTime => g.party.round_time = (g.party.round_time + d * 30.0).clamp(30.0, 1800.0),
        Knob::PtsKo => g.party.points_per_ko = (g.party.points_per_ko as i32 + dir).clamp(0, 100) as u16,
        Knob::PtsWin => g.party.points_round_win = (g.party.points_round_win as i32 + dir).clamp(0, 100) as u16,
        Knob::PickSecs => g.party.pick_secs = (g.party.pick_secs + d * 2.0).clamp(4.0, 60.0),
        Knob::BoomBust => g.party.boom_or_bust = !g.party.boom_or_bust,
        Knob::FirstTwo => g.party.first_place_two_choices = !g.party.first_place_two_choices,
        Knob::PoolCommon => g.party.pool_common = !g.party.pool_common,
        Knob::PoolLegend => g.party.pool_legendary = !g.party.pool_legendary,
        Knob::PoolCursed => g.party.pool_cursed = !g.party.pool_cursed,
        Knob::Corps => {
            c.prefs.corps = cycle(&Corps::ALL, c.prefs.corps, dir);
            game = false;
        }
        Knob::Variant => {
            let n = crate::lobby::costume::variant_count(&c.prefs.costume.phrase).unwrap_or(4).max(1) as i32;
            c.prefs.costume.seed = ((c.prefs.costume.seed as i32).rem_euclid(n) + dir).rem_euclid(n) as u32;
            game = false;
        }
        Knob::Accent => {
            let names: Vec<Option<[f32; 3]>> = ACCENTS.iter().map(|(_, a)| *a).collect();
            let cur = if names.contains(&c.prefs.costume.accent) { c.prefs.costume.accent } else { None };
            c.prefs.costume.accent = cycle(&names, cur, dir);
            game = false;
        }
        Knob::HeadScale => {
            c.prefs.costume.head_scale = ((c.prefs.costume.head_scale + d * 0.05) * 20.0).round().clamp(14.0, 27.0) / 20.0;
            game = false;
        }
        Knob::Acc(bit) => {
            c.prefs.costume.hide ^= bit;
            game = false;
        }
    }
    if game {
        c.prefs.game = c.prefs.game.clone().validated();
    }
    game
}

fn field_text(f: Field, m: &MenuState, p: &UiPrefs) -> (String, String) {
    match f {
        Field::JoinCode => ("CODE".into(), m.join_code.to_uppercase()),
        Field::Name => ("NAME".into(), p.name.clone()),
        Field::Loadout(i) => (format!("{}", i + 1), p.loadout[i as usize].clone()),
        Field::Costume => ("PHRASE".into(), p.costume.phrase.clone()),
    }
}

fn field_buf<'a>(f: Field, m: &'a mut MenuState, p: &'a mut UiPrefs) -> (&'a mut String, usize) {
    match f {
        Field::JoinCode => (&mut m.join_code, 96),
        Field::Name => (&mut p.name, 16),
        Field::Loadout(i) => (&mut p.loadout[i as usize], 60),
        Field::Costume => (&mut p.costume.phrase, 60),
    }
}

// ------------------------------------------------------------------------------------------ state

#[derive(Resource, Debug)]
pub struct MenuState {
    pub page: MenuPage,
    /// Settings tab (kept while the menu is closed / reopened).
    pub tab: SettingsTab,
    stack: Vec<MenuPage>,
    cursor: usize,
    scroll: usize,
    pub join_code: String,
    rebuild: bool,
    caret_t: f32,
    /// Waiting for a key to bind.
    capture: Option<Action>,
    /// Transient status line ("copied", "connecting…").
    toast: Option<(String, f32)>,
    title_t: f32,
    /// The cursor is on a text row (keys are text): the costume view skips its key controls.
    pub typing: bool,
    /// Put the cursor on this row at the next rebuild (`--menu rules:constructs`).
    focus: Option<Knob>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self { page: MenuPage::Title, tab: SettingsTab::Gameplay, stack: Vec::new(), cursor: 0, scroll: 0, join_code: String::new(), rebuild: true, caret_t: 0.0, capture: None, toast: None, title_t: 0.0, typing: false, focus: None }
    }
}

impl MenuState {
    fn goto(&mut self, page: MenuPage) {
        if page != self.page {
            self.stack.push(self.page);
        }
        self.page = page;
        self.cursor = 0;
        self.scroll = 0;
        self.rebuild = true;
        self.capture = None;
    }
    fn reset(&mut self, page: MenuPage) {
        self.stack.clear();
        self.page = page;
        self.cursor = 0;
        self.scroll = 0;
        self.rebuild = true;
        self.capture = None;
    }
    /// Open the in-game menu's Match Rules page (the end screen's host button; BACK → pause).
    /// The caller sets `InGameMenu`.
    pub fn open_rules(&mut self) {
        self.reset(MenuPage::Pause);
        self.goto(MenuPage::Rules);
    }
    fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), 2.5));
        self.rebuild = true;
    }
    fn set_tab(&mut self, tab: SettingsTab) {
        self.tab = tab;
        self.cursor = 0;
        self.scroll = 0;
        self.capture = None;
        self.rebuild = true;
    }
}

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct MenuRow(usize);
#[derive(Component)]
struct RowBar(usize);
#[derive(Component)]
struct RowLabel(usize);
#[derive(Component)]
struct RowValue(usize);
#[derive(Component)]
struct TabButton(SettingsTab);
#[derive(Component)]
struct TitleVoxel {
    col: f32,
    row: f32,
}

const MAX_VISIBLE: usize = 15;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(OnEnter(AppState::Menu), enter_menu)
            .add_systems(OnExit(AppState::Menu), |mut m: ResMut<MenuState>| m.rebuild = true)
            .add_systems(
                Update,
                (toggle_pause, net_pages, menu_input, menu_mouse, push_profile, rebuild_menu, style_rows, animate_title)
                    .chain()
                    .after(crate::lobby::UiSet::Input)
                    .run_if(not(in_state(AppState::Loading))),
            )
            .add_systems(Update, crate::lobby::preview::costume_preview.after(rebuild_menu).run_if(not(in_state(AppState::Loading))))
            .add_plugins(crate::lobby::costume_view::CostumeViewPlugin);
    }
}

/// Title page on every entry; the first entry honours `--menu <page>`.
fn enter_menu(cli: Res<crate::cli::CliArgs>, net: Option<Res<NetStatus>>, mut m: ResMut<MenuState>, mut first_done: Local<bool>) {
    let in_lobby = net.as_ref().is_some_and(|n| n.role != NetRole::Offline);
    m.reset(if in_lobby { MenuPage::Lobby } else { MenuPage::Title });
    if std::mem::replace(&mut *first_done, true) {
        return;
    }
    let tab = cli.menu_page.as_deref().and_then(|p| SettingsTab::from_id(p.strip_prefix("settings:").unwrap_or(p)));
    if let Some(tab) = tab {
        m.goto(MenuPage::Settings);
        m.tab = tab;
        return;
    }
    match cli.menu_page.as_deref() {
        Some("settings") => m.goto(MenuPage::Settings),
        Some("join") => m.goto(MenuPage::Join),
        Some("host") => m.goto(MenuPage::HostSetup),
        Some("rules") => m.goto(MenuPage::Rules),
        Some("rules:constructs") => {
            m.goto(MenuPage::Rules);
            m.focus = Some(Knob::Limit(Lim::GiantsOnMap)); // the whole CONSTRUCTS section in view
        }
        Some("customize") => m.goto(MenuPage::Customize),
        Some("costume") => m.goto(MenuPage::Costume),
        _ => {}
    }
}

/// Starts a local game on a map: switches maps through the loading screen when needed.
#[derive(SystemParam)]
struct GameStarter<'w> {
    mode: ResMut<'w, GameMode>,
    current: ResMut<'w, CurrentMap>,
    after: ResMut<'w, AfterLoading>,
    arena: Option<Res<'w, ArenaWorld>>,
}

impl GameStarter<'_> {
    fn start(&mut self, start: Option<(MapKind, GameMode)>, next: &mut NextState<AppState>) {
        let Some((map, mode)) = start else { return };
        *self.mode = mode;
        let loaded = self.arena.as_ref().map(|a| a.info.kind);
        if loaded == Some(map) {
            next.set(AppState::InGame);
        } else {
            self.current.0 = map;
            self.after.0 = AppState::InGame;
            next.set(AppState::Loading);
        }
    }
    fn set_map(&mut self, map: MapKind) {
        self.current.0 = map;
    }
}

fn menu_open(state: &AppState, pause: &InGameMenu) -> bool {
    *state == AppState::Menu || pause.0
}

fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, state: Res<State<AppState>>, focus: Res<UiFocus>, mut pause: ResMut<InGameMenu>, mut menu: ResMut<MenuState>) {
    if *state.get() != AppState::InGame || !keys.just_pressed(KeyCode::Escape) || focus.typing {
        return;
    }
    if !pause.0 {
        pause.0 = true;
        menu.reset(MenuPage::Pause);
    } else if menu.page == MenuPage::Pause {
        pause.0 = false;
        menu.rebuild = true;
    }
}

/// Signature of what the lobby page shows (rebuild only when it changes).
fn lobby_sig(n: &NetStatus) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}{:?}{:?}", n.phase, n.lobby_code, n.tunnel).hash(&mut h);
    n.lan.hash(&mut h);
    for p in &n.players {
        (p.id.0, &p.name, p.corps.index(), p.ready, p.is_host, p.rtt_ms / 10).hash(&mut h);
    }
    h.finish()
}

/// Follow NET: show the lobby while hosting/joining in the menu; drop back when it ends.
fn net_pages(state: Res<State<AppState>>, net: Option<Res<NetStatus>>, time: Res<Time<Real>>, mut m: ResMut<MenuState>, mut last_phase: Local<Option<NetPhase>>, mut last_sig: Local<u64>) {
    if let Some((_, t)) = m.toast.as_mut() {
        *t -= time.delta_secs();
        if *t <= 0.0 {
            m.toast = None;
            m.rebuild = true;
        }
    }
    let Some(net) = net else { return };
    if !net.is_changed() {
        return;
    }
    let sig = lobby_sig(&net);
    let phase_changed = last_phase.as_ref() != Some(&net.phase);
    if sig == *last_sig && !phase_changed {
        return;
    }
    *last_sig = sig;
    *last_phase = Some(net.phase.clone());
    if *state.get() == AppState::Menu && phase_changed {
        match &net.phase {
            NetPhase::Connecting | NetPhase::Lobby | NetPhase::Reconnecting { .. } => {
                if !matches!(m.page, MenuPage::Lobby | MenuPage::Rules | MenuPage::Customize | MenuPage::Costume | MenuPage::Settings) {
                    m.reset(MenuPage::Lobby);
                }
            }
            NetPhase::Failed(msg) => {
                let msg = msg.clone();
                if m.page == MenuPage::Lobby {
                    m.reset(MenuPage::Title);
                }
                m.toast(msg);
            }
            NetPhase::Idle if m.page == MenuPage::Lobby => m.reset(MenuPage::Title),
            _ => {}
        }
    }
    if m.page == MenuPage::Lobby {
        m.rebuild = true;
    }
}

#[derive(SystemParam)]
struct MenuIo<'w> {
    settings: ResMut<'w, Settings>,
    prefs: ResMut<'w, UiPrefs>,
    binds: ResMut<'w, KeyBinds>,
    net: Option<Res<'w, NetStatus>>,
    req: MessageWriter<'w, NetRequest>,
    next: ResMut<'w, NextState<AppState>>,
    pause: ResMut<'w, InGameMenu>,
    starter: GameStarter<'w>,
    exit: MessageWriter<'w, AppExit>,
    sfx: MessageWriter<'w, PlaySfx>,
    voice: Res<'w, crate::voice_ui::VoiceUi>,
    clipboard: Option<ResMut<'w, bevy::clipboard::Clipboard>>,
}

impl MenuIo<'_> {
    fn ctx(&mut self, in_game: bool) -> Ctx<'_> {
        let voice = bd_voice::ENABLED && self.voice.status != crate::voice_ui::VoiceStatus::Unavailable;
        Ctx { settings: &mut self.settings, prefs: &mut self.prefs, net: self.net.as_deref(), voice, in_game }
    }
    /// Read-only: must not flag `Settings` / `UiPrefs` as changed (that would hold off their
    /// debounced autosave for as long as the menu is open).
    fn rows(&mut self, page: MenuPage, tab: SettingsTab, in_game: bool) -> Vec<Row> {
        let voice = bd_voice::ENABLED && self.voice.status != crate::voice_ui::VoiceStatus::Unavailable;
        let c = Ctx { settings: self.settings.bypass_change_detection(), prefs: self.prefs.bypass_change_detection(), net: self.net.as_deref(), voice, in_game };
        page_rows(page, tab, &c)
    }
}

fn activate(row: Row, menu: &mut MenuState, io: &mut MenuIo, state: &AppState) {
    let in_game = *state == AppState::InGame;
    match row {
        Row::Button(a, _) => act(a, menu, io, state),
        Row::Value(k) => {
            let game = step_knob(k, &mut io.ctx(in_game), 1, false);
            if game && io.net.as_ref().is_some_and(|n| n.role == NetRole::Host) {
                io.req.write(NetRequest::SetSettings(io.prefs.game.clone()));
            }
            menu.rebuild = true;
        }
        Row::Text(Field::JoinCode) => act(Act::Connect, menu, io, state),
        Row::Text(_) => {
            // Enter on a text row moves on.
            menu.cursor += 1;
            menu.rebuild = true;
        }
        Row::Key(a) => {
            menu.capture = Some(a);
            menu.rebuild = true;
        }
        Row::Tabs => menu.set_tab(menu.tab.step(1)),
        Row::Header(_) => {}
    }
}

fn act(a: Act, menu: &mut MenuState, io: &mut MenuIo, state: &AppState) {
    match a {
        Act::Practice => {
            let map = map_of(&io.prefs);
            io.starter.start(Some((map, GameMode::Practice)), &mut io.next);
        }
        Act::Tutorial => io.starter.start(Some((MapKind::Tutorial, GameMode::Practice)), &mut io.next),
        Act::HostSetup => menu.goto(MenuPage::HostSetup),
        Act::Join => menu.goto(MenuPage::Join),
        Act::Customize => menu.goto(MenuPage::Customize),
        Act::Settings => menu.goto(MenuPage::Settings),
        Act::Rules => menu.goto(MenuPage::Rules),
        Act::Costume => menu.goto(MenuPage::Costume),
        Act::Lobby => menu.goto(MenuPage::Lobby),
        Act::Quit => {
            io.exit.write(AppExit::Success);
        }
        Act::Back => back(menu, state, &mut io.pause),
        Act::Resume => {
            io.pause.0 = false;
            menu.rebuild = true;
        }
        Act::Leave => {
            io.pause.0 = false;
            if io.net.as_ref().is_some_and(|n| n.role != NetRole::Offline) {
                io.req.write(NetRequest::Leave);
            }
            if *state == AppState::InGame {
                io.next.set(AppState::Menu);
            }
            menu.reset(MenuPage::Title);
        }
        Act::Connect => {
            let code = menu.join_code.trim().to_string();
            if !code.is_empty() {
                send_profile(io);
                io.req.write(NetRequest::Join { address: code });
                menu.reset(MenuPage::Lobby);
                menu.toast("connecting");
            }
        }
        Act::Paste => {
            if let Some(cb) = io.clipboard.as_mut()
                && let Some(Ok(t)) = cb.fetch_text().poll_result()
            {
                menu.join_code = t.trim().chars().filter(|c| !c.is_control()).take(96).collect();
                menu.rebuild = true;
            }
        }
        Act::OpenLobby => {
            let map = map_of(&io.prefs);
            io.starter.set_map(map);
            send_profile(io);
            io.req.write(NetRequest::SetSettings(io.prefs.game.clone()));
            io.req.write(NetRequest::Host { port: None, tunnel: true });
            io.req.write(NetRequest::SetSettings(io.prefs.game.clone()));
            menu.reset(MenuPage::Lobby);
            menu.toast("opening lobby");
        }
        Act::Ready => {
            let ready = io.net.as_ref().and_then(|n| n.me.and_then(|me| n.players.iter().find(|p| p.id == me))).is_some_and(|p| p.ready);
            io.req.write(NetRequest::SetReady(!ready));
            menu.rebuild = true;
        }
        Act::Start => {
            io.req.write(NetRequest::SetSettings(io.prefs.game.clone()));
            io.req.write(NetRequest::StartMatch);
        }
        Act::Randomize => {
            let n = RANDOM_COSTUMES.len();
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7);
            let mut r = crate::util::Rng::new(t as u64);
            // Mostly an iconic character (their own colours), sometimes a wild vocabulary phrase.
            match bd_shapes::costume::look::random_name(r.next_u32()).filter(|_| r.next_u32() % 5 != 0) {
                Some(name) => {
                    io.prefs.costume.phrase = name.into();
                    let v = crate::lobby::costume::variant_count(name).unwrap_or(1).max(1) as u32;
                    io.prefs.costume.seed = r.next_u32() % v;
                    io.prefs.costume.accent = None;
                }
                None => {
                    io.prefs.costume.phrase = RANDOM_COSTUMES[(r.next_u32() as usize) % n].into();
                    io.prefs.costume.seed = r.next_u32() % 4;
                    io.prefs.costume.accent = ACCENTS[(r.next_u32() as usize) % ACCENTS.len()].1;
                }
            }
            menu.rebuild = true;
        }
        Act::ResetKeys => {
            *io.binds = KeyBinds::default();
            menu.rebuild = true;
        }
        Act::Copy => {
            if let (Some(code), Some(cb)) = (io.net.as_ref().and_then(|n| n.lobby_code.clone().or_else(|| n.lan.first().cloned())), io.clipboard.as_mut()) {
                menu.toast(if cb.set_text(code).is_ok() { "copied" } else { "couldn't copy" });
            }
        }
    }
}

fn send_profile(io: &mut MenuIo) {
    let p = &io.prefs;
    let mut id = NetIdentity { name: p.name.clone(), corps: p.corps, ..Default::default() };
    id.profile.loadout = p.loadout.clone();
    let c = &p.costume;
    id.profile.costume = crate::net::NetCostume { phrase: c.phrase.clone(), seed: c.seed, accent: c.accent, head_scale: c.head_scale, hide: c.hide };
    io.req.write(NetRequest::UpdateProfile(id));
}

fn back(menu: &mut MenuState, state: &AppState, pause: &mut InGameMenu) {
    if let Some(prev) = menu.stack.pop() {
        menu.page = prev;
        menu.cursor = 0;
        menu.scroll = 0;
        menu.rebuild = true;
        menu.capture = None;
        return;
    }
    match menu.page {
        MenuPage::Pause => {
            pause.0 = false;
            menu.rebuild = true;
        }
        _ if *state == AppState::InGame => menu.reset(MenuPage::Pause),
        MenuPage::Lobby | MenuPage::Title => {}
        _ => menu.reset(MenuPage::Title),
    }
}

/// First selectable row at or after `i` (wrapping in `dir`).
fn next_selectable(rows: &[Row], from: usize, dir: i32) -> usize {
    let n = rows.len() as i32;
    if n == 0 {
        return 0;
    }
    let mut i = from as i32;
    for _ in 0..n {
        i = (i + dir).rem_euclid(n);
        if rows[i as usize].selectable() {
            return i as usize;
        }
    }
    from
}

/// Settings → Controls rebind: refuse reserved / wrong-kind inputs, swap with whoever held it, and
/// say what moved.
fn rebind(menu: &mut MenuState, binds: &mut KeyBinds, action: Action, b: Bind) {
    let before: Vec<Option<Bind>> = crate::lobby::prefs::ALIASES.iter().map(|(a, _)| binds.alias(*a)).collect();
    match binds.try_set(action, b) {
        Err(()) if matches!(b, Bind::Mouse(_)) && action.keys_only() => menu.toast(format!("{} TAKES A KEY", action.label())),
        Err(()) => menu.toast(format!("{} IS RESERVED", b.label())),
        Ok(Some(other)) => {
            let now = binds.get(other);
            menu.toast(if now == Bind::Unbound { format!("{} IS NOW UNBOUND", other.label()) } else { format!("SWAPPED: {} IS NOW {}", other.label(), now.label()) });
        }
        Ok(None) => {
            for ((a, alias), was) in crate::lobby::prefs::ALIASES.iter().zip(before) {
                if was.is_some() && binds.alias(*a).is_none() {
                    menu.toast(format!("{} NO LONGER DOES {}", alias.label(), a.label()));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_input(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    mut mouse_btn: MessageReader<MouseButtonInput>,
    state: Res<State<AppState>>,
    mut menu: ResMut<MenuState>,
    mut io: MenuIo,
) {
    if !menu_open(state.get(), &io.pause) {
        typed.clear();
        mouse_btn.clear();
        return;
    }
    let in_game = *state.get() == AppState::InGame;
    let menu = &mut *menu;
    menu.caret_t += time.delta_secs();
    menu.title_t += time.delta_secs();
    let rows = io.rows(menu.page, menu.tab, in_game);
    let n = rows.len();
    if n == 0 {
        return;
    }
    if !rows[menu.cursor.min(n - 1)].selectable() {
        menu.cursor = next_selectable(&rows, menu.cursor, 1);
    }
    menu.cursor = menu.cursor.min(n - 1);
    let current = rows[menu.cursor];

    // Key capture for rebinding.
    if let Some(action) = menu.capture {
        for ev in typed.read() {
            if ev.state != ButtonState::Pressed {
                continue;
            }
            match ev.key_code {
                KeyCode::Escape => {}
                KeyCode::Backspace | KeyCode::Delete => {
                    io.binds.set(action, Bind::Unbound);
                }
                k => match Bind::from_id(&format!("{k:?}")) {
                    Some(b) => rebind(menu, &mut io.binds, action, b),
                    None => menu.toast(format!("{} IS RESERVED", Bind::Key(k).label())),
                },
            }
            menu.capture = None;
            menu.rebuild = true;
            return;
        }
        for ev in mouse_btn.read() {
            // LMB is how the row was clicked (and reserved): keep waiting.
            if ev.state == ButtonState::Pressed && ev.button != MouseButton::Left {
                rebind(menu, &mut io.binds, action, Bind::Mouse(ev.button));
                menu.capture = None;
                menu.rebuild = true;
                return;
            }
        }
        return;
    }
    mouse_btn.clear();

    // Text entry on text rows.
    let typing = matches!(current, Row::Text(_));
    menu.typing = typing;
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if let Row::Text(f) = current {
        let mut changed = false;
        for ev in typed.read() {
            if ev.state != ButtonState::Pressed {
                continue;
            }
            if ctrl && ev.key_code == KeyCode::KeyV {
                if let Some(cb) = io.clipboard.as_mut()
                    && let Some(Ok(t)) = cb.fetch_text().poll_result()
                {
                    let (buf, max) = field_buf(f, menu, &mut io.prefs);
                    for c in t.chars().filter(|c| !c.is_control()) {
                        if buf.chars().count() < max {
                            buf.push(c);
                        }
                    }
                    changed = true;
                }
                continue;
            }
            let (buf, max) = field_buf(f, menu, &mut io.prefs);
            match &ev.logical_key {
                Key::Backspace => {
                    if ctrl {
                        let t = buf.trim_end().len();
                        let cut = buf[..t].rfind(' ').map(|i| i + 1).unwrap_or(0);
                        buf.truncate(cut);
                    } else {
                        buf.pop();
                    }
                    changed = true;
                }
                _ => {
                    let text = ev.text.as_ref().map(|t| t.as_str()).or(match &ev.logical_key {
                        Key::Character(c) => Some(c.as_str()),
                        Key::Space => Some(" "),
                        _ => None,
                    });
                    if let Some(t) = text {
                        for c in t.chars().filter(|c| !c.is_control()) {
                            let ok = match f {
                                Field::JoinCode => c.is_ascii_alphanumeric() || "-._:/".contains(c),
                                _ => true,
                            };
                            if ok && buf.chars().count() < max {
                                buf.push(if f == Field::JoinCode { c.to_ascii_lowercase() } else { c });
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if changed {
            menu.rebuild = true;
        }
    } else {
        typed.clear();
    }

    let up = keys.just_pressed(KeyCode::ArrowUp) || (!typing && keys.just_pressed(KeyCode::KeyW));
    let down = keys.just_pressed(KeyCode::ArrowDown) || (!typing && keys.just_pressed(KeyCode::KeyS)) || keys.just_pressed(KeyCode::Tab);
    let left = keys.just_pressed(KeyCode::ArrowLeft) || (!typing && keys.just_pressed(KeyCode::KeyA));
    let right = keys.just_pressed(KeyCode::ArrowRight) || (!typing && keys.just_pressed(KeyCode::KeyD));
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let enter = !alt && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) || (!typing && keys.just_pressed(KeyCode::Space)));
    let fine = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    // Settings tabs: Q / E anywhere on the page, ←/→ on the tab strip.
    if menu.page == MenuPage::Settings && !typing {
        let on_strip = current == Row::Tabs;
        let d = (keys.just_pressed(KeyCode::KeyE) || (on_strip && right)) as i32 - (keys.just_pressed(KeyCode::KeyQ) || (on_strip && left)) as i32;
        if d != 0 {
            menu.set_tab(menu.tab.step(d));
            io.sfx.write(PlaySfx::new("ui_move"));
            return;
        }
    }

    if up {
        menu.cursor = next_selectable(&rows, menu.cursor, -1);
        io.sfx.write(PlaySfx::new("ui_move"));
    }
    if down {
        menu.cursor = next_selectable(&rows, menu.cursor, 1);
        io.sfx.write(PlaySfx::new("ui_move"));
    }
    if (left || right)
        && let Row::Value(k) = current
    {
        let game = step_knob(k, &mut io.ctx(in_game), if right { 1 } else { -1 }, fine);
        if game && io.net.as_ref().is_some_and(|n| n.role == NetRole::Host) {
            io.req.write(NetRequest::SetSettings(io.prefs.game.clone()));
        }
        menu.rebuild = true;
        io.sfx.write(PlaySfx::new("ui_move"));
    }
    if enter {
        activate(current, menu, &mut io, state.get());
        io.sfx.write(PlaySfx::new("ui_select"));
    }
    if keys.just_pressed(KeyCode::Escape) && menu.page != MenuPage::Pause {
        back(menu, state.get(), &mut io.pause);
        io.sfx.write(PlaySfx::new("ui_back"));
    }
    if up || down {
        menu.rebuild = true;
    }
    // Blinking caret on text rows.
    if typing && ((menu.caret_t * 2.0) as i32) != (((menu.caret_t - time.delta_secs()) * 2.0) as i32) {
        menu.rebuild = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_mouse(
    tabs_q: Query<(&Interaction, &TabButton), Changed<Interaction>>,
    rows_q: Query<(&Interaction, &MenuRow), Changed<Interaction>>,
    all_rows: Query<(&Interaction, &MenuRow)>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    state: Res<State<AppState>>,
    costume_view: Option<Res<crate::lobby::costume_view::CostumeView>>,
    mut menu: ResMut<MenuState>,
    mut io: MenuIo,
) {
    if !menu_open(state.get(), &io.pause) || menu.capture.is_some() {
        return;
    }
    let in_game = *state.get() == AppState::InGame;
    let menu = &mut *menu;
    for (i, t) in &tabs_q {
        if *i == Interaction::Pressed && menu.page == MenuPage::Settings && menu.tab != t.0 {
            menu.set_tab(t.0);
            io.sfx.write(PlaySfx::new("ui_select"));
            return;
        }
    }
    let rows = io.rows(menu.page, menu.tab, in_game);
    // Over the costume preview the wheel zooms the model instead (`lobby::costume_view`).
    let wheel_on_preview = menu.page == MenuPage::Costume && costume_view.is_some_and(|v| v.pointer_on_preview);
    if scroll.delta.y.abs() > 0.0 && rows.len() > MAX_VISIBLE && !wheel_on_preview {
        let d = if scroll.delta.y > 0.0 { -1 } else { 1 };
        menu.cursor = next_selectable(&rows, menu.cursor, d);
        menu.rebuild = true;
    }
    for (i, row) in &rows_q {
        if matches!(i, Interaction::Hovered | Interaction::Pressed) && row.0 < rows.len() && rows[row.0].selectable() && menu.cursor != row.0 {
            menu.cursor = row.0;
        }
    }
    let hovered = all_rows.iter().find(|(i, _)| matches!(i, Interaction::Hovered | Interaction::Pressed)).map(|(_, r)| r.0);
    let Some(idx) = hovered else { return };
    let Some(&row) = rows.get(idx) else { return };
    if mouse.just_pressed(MouseButton::Left) {
        activate(row, menu, &mut io, state.get());
        io.sfx.write(PlaySfx::new("ui_select"));
    } else if mouse.just_pressed(MouseButton::Right)
        && let Row::Value(k) = row
    {
        let game = step_knob(k, &mut io.ctx(in_game), -1, false);
        if game && io.net.as_ref().is_some_and(|n| n.role == NetRole::Host) {
            io.req.write(NetRequest::SetSettings(io.prefs.game.clone()));
        }
        menu.rebuild = true;
    }
}

/// Keep NET's copy of our profile current (name / corps / costume / loadout).
fn push_profile(prefs: Res<UiPrefs>, mut io_req: MessageWriter<NetRequest>, mut last: Local<Option<(String, Corps, String, u32, u32, u32, [String; 4])>>) {
    if !prefs.is_changed() && last.is_some() {
        return;
    }
    let c = &prefs.costume;
    let key = (prefs.name.clone(), prefs.corps, c.phrase.clone(), c.seed, c.hide, (c.head_scale * 100.0) as u32, prefs.loadout.clone());
    if last.as_ref() == Some(&key) {
        return;
    }
    *last = Some(key);
    let mut id = NetIdentity { name: prefs.name.clone(), corps: prefs.corps, ..Default::default() };
    id.profile.loadout = prefs.loadout.clone();
    id.profile.costume = crate::net::NetCostume { phrase: c.phrase.clone(), seed: c.seed, accent: c.accent, head_scale: c.head_scale, hide: c.hide };
    io_req.write(NetRequest::UpdateProfile(id));
}

// ------------------------------------------------------------------------------------------ drawing

const ACCENT: Color = Color::srgb(0.25, 1.0, 0.42);

/// 5×7 voxel glyphs for the title.
fn glyph(c: char) -> [&'static str; 7] {
    match c {
        'B' => ["11110", "10001", "10001", "11110", "10001", "10001", "11110"],
        'R' => ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
        'I' => ["11111", "00100", "00100", "00100", "00100", "00100", "11111"],
        'G' => ["01111", "10000", "10000", "10011", "10001", "10001", "01111"],
        'H' => ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
        'T' => ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
        'E' => ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
        'S' => ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
        'D' => ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
        'A' => ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
        'Y' => ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
        _ => ["00000"; 7],
    }
}

fn spawn_voxel_title(root: &mut ChildSpawnerCommands, text: &str, vox: f32, glow: Color) {
    let gap = 1.0;
    let cell = vox + gap;
    let letters: Vec<char> = text.chars().collect();
    let width = letters.len() as f32 * cell * 6.0;
    root.spawn((Node { width: px(width), height: px(cell * 7.0), margin: UiRect::bottom(px(30)), ..default() }, Pickable::IGNORE)).with_children(|t| {
        for (li, ch) in letters.iter().enumerate() {
            for (ri, row) in glyph(*ch).iter().enumerate() {
                for (ci, b) in row.chars().enumerate() {
                    if b != '1' {
                        continue;
                    }
                    let col = li as f32 * 6.0 + ci as f32;
                    t.spawn((
                        TitleVoxel { col, row: ri as f32 },
                        Node { position_type: PositionType::Absolute, left: px(col * cell), top: px(ri as f32 * cell), width: px(vox), height: px(vox), ..default() },
                        BackgroundColor(Color::WHITE),
                        BoxShadow::new(glow.with_alpha(0.6), px(0), px(0), px(1), px(vox * 1.3)),
                        UiTransform::default(),
                        Pickable::IGNORE,
                    ));
                }
            }
        }
    });
}

/// Title voxels: form in from scattered on first show, then a slow light sweep + shimmer.
fn animate_title(menu: Res<MenuState>, prefs: Res<UiPrefs>, mut q: Query<(&TitleVoxel, &mut BackgroundColor, &mut UiTransform)>) {
    if q.is_empty() {
        return;
    }
    let t = menu.title_t;
    let corps = crate::console::corps_color(prefs.corps);
    for (v, mut bg, mut tf) in &mut q {
        // Forming: each voxel arrives with a small delay by column.
        let arrive = ((t - v.col * 0.012 - v.row * 0.02) / 0.35).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - arrive).powi(3);
        let scatter = (v.col * 12.9898 + v.row * 78.233).sin() * 43758.547;
        let (sx, sy) = ((scatter.fract() - 0.5) * 60.0, ((scatter * 1.7).fract() - 0.5) * 40.0);
        tf.translation = Val2::px(sx * (1.0 - e), sy * (1.0 - e) - 8.0 * (1.0 - e));
        tf.scale = Vec2::splat(0.3 + 0.7 * e);
        // Sweep of light across the letters every ~4 s.
        let sweep = ((t * 0.25).fract() * 110.0 - 20.0 - v.col).abs();
        let hot = (1.0 - sweep / 6.0).clamp(0.0, 1.0);
        let shimmer = 0.9 + 0.1 * ((t * 3.0 + v.col * 0.7 + v.row * 1.3).sin());
        let base = corps.mix(&Color::WHITE, 0.62 + 0.38 * hot);
        bg.0 = base.with_alpha(e * shimmer.min(1.0));
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_menu(mut commands: Commands, mut menu: ResMut<MenuState>, mut io: MenuIo, state: Res<State<AppState>>, roots: Query<Entity, With<MenuRoot>>) {
    let open = menu_open(state.get(), &io.pause);
    if !menu.rebuild && (open == !roots.is_empty()) {
        return;
    }
    menu.rebuild = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
    if !open {
        return;
    }
    let in_game = *state.get() == AppState::InGame;
    let page = menu.page;
    if page == MenuPage::Title && roots.is_empty() {
        menu.title_t = 0.0;
    }
    let rows = io.rows(page, menu.tab, in_game);
    if let Some(k) = menu.focus.take() {
        menu.cursor = rows.iter().position(|r| *r == Row::Value(k)).unwrap_or(menu.cursor);
    }
    // Scroll window (a settings tab strip stays pinned above it).
    let n = rows.len();
    let pinned = rows.first() == Some(&Row::Tabs);
    let lo = usize::from(pinned);
    let vis = MAX_VISIBLE - lo;
    let cur = menu.cursor.max(lo);
    menu.scroll = menu.scroll.max(lo);
    if cur < menu.scroll {
        menu.scroll = cur.saturating_sub(1).max(lo);
    }
    if cur >= menu.scroll + vis {
        menu.scroll = cur + 1 - vis;
    }
    menu.scroll = menu.scroll.min(n.saturating_sub(vis).max(lo));
    let (from, to) = (menu.scroll, (menu.scroll + vis).min(n));
    let tab = menu.tab;
    let corps = crate::console::corps_color(io.prefs.corps);
    let glow = corps;

    let settings_like_bd = matches!(page, MenuPage::Settings | MenuPage::Rules | MenuPage::Customize | MenuPage::HostSetup | MenuPage::Join | MenuPage::Lobby | MenuPage::Costume);
    let backdrop = if settings_like_bd {
        BackgroundGradient::from(LinearGradient::new(
            LinearGradient::TO_RIGHT,
            vec![
                ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.92), percent(0)),
                ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.86), percent(44)),
                ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.0), percent(64)),
            ],
        ))
    } else if in_game {
        BackgroundGradient::from(LinearGradient::new(
            LinearGradient::TO_RIGHT,
            vec![ColorStop::new(Color::srgba(0.0, 0.01, 0.02, 0.88), percent(0)), ColorStop::new(Color::srgba(0.0, 0.01, 0.02, if page == MenuPage::Costume { 0.0 } else { 0.55 }), percent(60))],
        ))
    } else {
        BackgroundGradient::from(LinearGradient::new(
            LinearGradient::TO_RIGHT,
            vec![
                ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.9), percent(0)),
                ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.78), percent(34)),
                ColorStop::new(Color::srgba(0.0, 0.01, 0.03, 0.3), percent(55)),
                ColorStop::new(Color::srgba(0.0, 0.0, 0.0, 0.0), percent(75)),
            ],
        ))
    };
    let settings_like = matches!(page, MenuPage::Settings | MenuPage::Rules | MenuPage::Costume | MenuPage::Customize | MenuPage::HostSetup | MenuPage::Join | MenuPage::Lobby);
    let prefs_snapshot = io.prefs.clone();
    let net = io.net.as_deref();
    let ctx_voice = bd_voice::ENABLED;
    let menu_ref = &*menu;
    commands
        .spawn((
            Name::new("MenuRoot"),
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::left(percent(7)),
                row_gap: px(4),
                ..default()
            },
            backdrop,
            GlobalZIndex(80),
        ))
        .with_children(|root| {
            let (title, sub): (&str, Option<String>) = match page {
                MenuPage::Title => ("", None),
                MenuPage::Join => ("JOIN", Some("lobby code, ip:port or link".into())),
                MenuPage::HostSetup => ("HOST", None),
                MenuPage::Lobby => ("LOBBY", None),
                MenuPage::Rules => ("MATCH RULES", None),
                MenuPage::Customize => ("CUSTOMIZE", None),
                MenuPage::Costume => ("COSTUME", Some("type anything".into())),
                MenuPage::Settings => ("SETTINGS", None),
                MenuPage::Pause => ("PAUSED", None),
            };
            if page == MenuPage::Title {
                spawn_voxel_title(root, "BRIGHTEST DAY", 10.0, glow);
            } else {
                root.spawn((
                    Text::new(title),
                    TextFont::from_font_size(44.0),
                    TextColor(Color::WHITE),
                    TextShadow { offset: Vec2::new(0.0, 3.0), color: glow.with_alpha(0.35) },
                    Node { margin: UiRect::bottom(px(if sub.is_some() { 2.0 } else { 18.0 })), ..default() },
                ));
                if let Some(sub) = sub {
                    root.spawn((Text::new(sub), TextFont::from_font_size(15.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)), Node { margin: UiRect::bottom(px(16)), ..default() }));
                }
                root.spawn((Node { width: px(64), height: px(3), margin: UiRect::bottom(px(16)), ..default() }, BackgroundColor(glow)));
            }

            // Lobby: code big + players.
            if page == MenuPage::Lobby {
                lobby_panel(root, net, glow);
            }

            // Settings tab strip: pinned above the scrolling rows.
            if pinned {
                let on_strip = menu_ref.cursor == 0;
                root.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(6), margin: UiRect { left: px(17), bottom: px(6), ..default() }, ..default() },)).with_children(|strip| {
                    for t in SettingsTab::ALL {
                        let active = t == tab;
                        let fg = if active { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.4) };
                        let bg = if active { glow.with_alpha(if on_strip { 0.35 } else { 0.18 }) } else { Color::NONE };
                        strip
                            .spawn((
                                TabButton(t),
                                Button,
                                Node { padding: UiRect::axes(px(10), px(4)), border: UiRect::bottom(px(2)), ..default() },
                                BorderColor::all(if active { glow } else { Color::NONE }),
                                BackgroundColor(bg),
                            ))
                            .with_children(|b| {
                                b.spawn((Text::new(t.label()), TextFont::from_font_size(16.0), TextColor(fg), Pickable::IGNORE));
                            });
                    }
                });
            }
            if from > lo {
                root.spawn((Text::new("^"), TextFont::from_font_size(12.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.35)), Node { margin: UiRect::left(px(20)), ..default() }));
            }
            for (i, row) in rows.iter().enumerate().take(to).skip(from) {
                if *row == Row::Tabs {
                    continue;
                }
                if let Row::Header(h) = row {
                    root.spawn((
                        Text::new(*h),
                        TextFont::from_font_size(13.0),
                        TextColor(glow.with_alpha(0.75)),
                        Node { margin: UiRect { top: px(10), left: px(17), bottom: px(2), ..default() }, ..default() },
                    ));
                    continue;
                }
                let (label, value): (String, Option<String>) = match *row {
                    Row::Button(_, l) => (l.to_string(), None),
                    Row::Value(k) => {
                        // Ctx needs &mut; build a read-only copy for text.
                        let mut s = io.settings.clone();
                        let mut p = prefs_snapshot.clone();
                        let c = Ctx { settings: &mut s, prefs: &mut p, net, voice: ctx_voice, in_game };
                        let (l, v) = knob_text(k, &c);
                        (l, Some(v))
                    }
                    Row::Text(f) => {
                        let (l, v) = field_text(f, menu_ref, &prefs_snapshot);
                        let caret = if i == menu_ref.cursor && (menu_ref.caret_t * 2.0) as i32 % 2 == 0 { "_" } else { " " };
                        (l, Some(format!("{v}{caret}")))
                    }
                    Row::Key(a) => {
                        let v = if menu_ref.capture == Some(a) {
                            "PRESS A KEY".to_string()
                        } else {
                            match io.binds.alias(a) {
                                Some(alias) if io.binds.get(a) != Bind::Unbound => format!("{} / {}", io.binds.get(a).label(), alias.label()),
                                Some(alias) => alias.label(),
                                None => io.binds.get(a).label(),
                            }
                        };
                        (a.label().to_string(), Some(v))
                    }
                    Row::Header(_) | Row::Tabs => unreachable!(),
                };
                let wide = settings_like;
                let text_row = matches!(row, Row::Text(_));
                root.spawn((
                    MenuRow(i),
                    Button,
                    Node {
                        width: px(if wide { 560.0 } else { 360.0 }),
                        height: px(if wide { 31.0 } else { 40.0 }),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: px(14),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|r| {
                    r.spawn((RowBar(i), Node { width: px(3), height: px(22), ..default() }, BackgroundColor(Color::NONE)));
                    r.spawn((
                        RowLabel(i),
                        Text::new(label),
                        TextFont::from_font_size(if wide { 21.0 } else { 24.0 }),
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.55)),
                        Node { flex_grow: if text_row { 0.0 } else { 1.0 }, min_width: px(if text_row { 90.0 } else { 0.0 }), ..default() },
                    ));
                    if let Some(v) = value {
                        let corps_value = matches!(row, Row::Value(Knob::Corps));
                        r.spawn((
                            RowValue(i),
                            Text::new(v),
                            TextFont::from_font_size(if wide { 20.0 } else { 22.0 }),
                            TextColor(if corps_value { crate::console::corps_color(prefs_snapshot.corps) } else { Color::srgba(1.0, 1.0, 1.0, 0.55) }),
                            Node { flex_grow: if text_row { 1.0 } else { 0.0 }, ..default() },
                        ));
                    }
                });
            }
            if to < n {
                root.spawn((Text::new("v"), TextFont::from_font_size(12.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.35)), Node { margin: UiRect::left(px(20)), ..default() }));
            }
            // Footer hints (controls only — never mechanics).
            let hint = match page {
                MenuPage::Settings if tab == SettingsTab::Controls => Some("q/e tab   enter = rebind   backspace = clear   esc back"),
                MenuPage::Settings => Some("q/e tab   left/right adjust   shift = fine   esc back"),
                MenuPage::Rules | MenuPage::HostSetup => Some("left/right adjust   shift = fine   esc back"),
                MenuPage::Costume => Some("left/right adjust   shift = fine   drag rotate   wheel zoom   r reset   esc back"),
                MenuPage::Customize => Some("quick cast: the phrase your ring builds on 1-4"),
                _ => None,
            };
            if let Some(h) = hint {
                root.spawn((Text::new(h), TextFont::from_font_size(13.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.3)), Node { margin: UiRect::top(px(16)), ..default() }));
            }
            if let Some((t, _)) = &menu_ref.toast {
                root.spawn((Text::new(t.to_uppercase()), TextFont::from_font_size(15.0), TextColor(glow.mix(&Color::WHITE, 0.4)), Node { margin: UiRect::top(px(12)), ..default() }));
            }
        });
}

fn lobby_panel(root: &mut ChildSpawnerCommands, net: Option<&NetStatus>, glow: Color) {
    let Some(net) = net else { return };
    let code = net.lobby_code.clone().or_else(|| match &net.tunnel {
        crate::net::TunnelInfo::Ready { code, .. } => Some(code.clone()),
        _ => None,
    });
    let status = match (&net.phase, &net.tunnel, net.role.clone()) {
        (NetPhase::Connecting, _, NetRole::Client) => Some("connecting".to_string()),
        (_, crate::net::TunnelInfo::Starting, NetRole::Host) => Some("opening a tunnel".to_string()),
        (_, crate::net::TunnelInfo::Failed(msg), NetRole::Host) => Some(format!("LAN only - {msg}")),
        (NetPhase::Reconnecting { attempt }, _, _) => Some(format!("reconnecting ({attempt})")),
        _ => None,
    };
    let lan_big = code.is_none() && net.role == NetRole::Host;
    let code = code.or_else(|| if lan_big { net.lan.first().cloned() } else { None });
    if let Some(code) = code {
        root.spawn((
            Text::new(code.to_uppercase()),
            TextFont::from_font_size(40.0),
            TextColor(Color::WHITE),
            TextShadow { offset: Vec2::ZERO, color: glow.with_alpha(0.8) },
            Node { margin: UiRect::bottom(px(2)), ..default() },
        ));
    }
    let mut small = Vec::new();
    if let Some(s) = status {
        small.push(s);
    }
    if !lan_big && !net.lan.is_empty() && net.role == NetRole::Host {
        small.push(format!("lan {}", net.lan.join("  ")));
    }
    if !small.is_empty() {
        root.spawn((Text::new(small.join("   ")), TextFont::from_font_size(14.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)), Node { margin: UiRect::bottom(px(10)), ..default() }));
    }
    root.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(3), margin: UiRect::bottom(px(14)), ..default() }, Pickable::IGNORE)).with_children(|list| {
        for p in &net.players {
            let c = crate::console::corps_color(p.corps);
            let me = net.me == Some(p.id);
            list.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(12), align_items: AlignItems::Center, width: px(520), ..default() }, Pickable::IGNORE)).with_children(|row| {
                row.spawn((Node { width: px(4), height: px(20), ..default() }, BackgroundColor(c)));
                row.spawn((Text::new(p.name.to_uppercase()), TextFont::from_font_size(19.0), TextColor(Color::WHITE.with_alpha(if me { 1.0 } else { 0.8 })), Node { flex_grow: 1.0, ..default() }));
                if p.is_host {
                    row.spawn((Text::new("HOST"), TextFont::from_font_size(12.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.4))));
                } else if p.rtt_ms > 0 {
                    row.spawn((Text::new(format!("{} MS", p.rtt_ms)), TextFont::from_font_size(12.0), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.4))));
                }
                row.spawn((
                    Text::new(if p.ready { "READY" } else { "..." }),
                    TextFont::from_font_size(15.0),
                    TextColor(if p.ready { c.mix(&Color::WHITE, 0.3) } else { Color::srgba(1.0, 1.0, 1.0, 0.35) }),
                    Node { width: px(70), ..default() },
                ));
            });
        }
    });
}

#[allow(clippy::type_complexity)]
fn style_rows(
    time: Res<Time<Real>>,
    menu: Res<MenuState>,
    prefs: Res<UiPrefs>,
    mut rows: Query<(&MenuRow, &mut Node, &mut BackgroundColor), (Without<RowBar>, Without<RowLabel>)>,
    mut bars: Query<(&RowBar, &mut BackgroundColor, &mut Node), Without<MenuRow>>,
    mut labels: Query<(&RowLabel, &mut TextColor), Without<RowValue>>,
    mut values: Query<(&RowValue, &mut TextColor), Without<RowLabel>>,
) {
    let dt = time.delta_secs();
    let k = 1.0 - (-18.0 * dt).exp();
    let accent = crate::console::corps_color(prefs.corps).mix(&ACCENT, 0.0);
    for (row, mut node, mut bg) in &mut rows {
        let sel = row.0 == menu.cursor;
        let target = if sel { 14.0 } else { 0.0 };
        let cur = if let Val::Px(x) = node.padding.left { x } else { 0.0 };
        let next = cur + (target - cur) * k;
        if (next - cur).abs() > 0.01 {
            node.padding.left = px(next);
        }
        let want = if sel { accent.with_alpha(0.08) } else { Color::NONE };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (bar, mut bg, mut node) in &mut bars {
        let sel = bar.0 == menu.cursor;
        let want = if sel { accent } else { Color::srgba(1.0, 1.0, 1.0, 0.12) };
        if bg.0 != want {
            bg.0 = want;
        }
        let h = if sel { px(26) } else { px(14) };
        if node.height != h {
            node.height = h;
        }
    }
    for (l, mut c) in &mut labels {
        let want = if l.0 == menu.cursor { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.55) };
        if c.0 != want {
            c.0 = want;
        }
    }
    for (v, mut c) in &mut values {
        if c.0.alpha() > 0.99 && v.0 != menu.cursor {
            continue; // coloured values (corps) keep their colour
        }
        let want = if v.0 == menu.cursor { accent.mix(&Color::WHITE, 0.25) } else { Color::srgba(1.0, 1.0, 1.0, 0.45) };
        if c.0 != want {
            c.0 = want;
        }
    }
}

/// Used by the costume preview to know when to show itself.
pub fn costume_page_open(menu: &MenuState, state: &AppState, pause: &InGameMenu) -> bool {
    menu_open(state, pause) && matches!(menu.page, MenuPage::Costume | MenuPage::Customize)
}


#[cfg(test)]
mod tests {
    use super::*;

    /// COPY CODE / paste must reach the OS clipboard (`system_clipboard`), not bevy's in-process
    /// fallback buffer. Ignored: it overwrites the machine's clipboard.
    #[test]
    #[ignore]
    fn copy_code_reaches_the_os_clipboard() {
        let code = "bd-clipboard-probe-7f3a";
        assert!(bevy::clipboard::Clipboard::default().set_text(code).is_ok());
        // A fresh instance can only see it through the OS.
        let mut other = bevy::clipboard::Clipboard::default();
        let got = other.fetch_text().poll_result();
        assert_eq!(got.and_then(|r| r.ok()).as_deref(), Some(code));
    }

    fn settings_rows(tab: SettingsTab, voice: bool) -> Vec<Row> {
        let mut s = Settings::default();
        let mut p = UiPrefs::default();
        let c = Ctx { settings: &mut s, prefs: &mut p, net: None, voice, in_game: false };
        page_rows(MenuPage::Settings, tab, &c)
    }

    #[test]
    fn every_action_is_rebindable_exactly_once() {
        let rows = settings_rows(SettingsTab::Controls, true);
        for a in Action::ALL {
            let n = rows.iter().filter(|r| **r == Row::Key(a)).count();
            assert_eq!(n, 1, "{a:?}");
        }
        // Key rows only live on the Controls tab.
        for t in SettingsTab::ALL.into_iter().filter(|t| *t != SettingsTab::Controls) {
            assert!(!settings_rows(t, true).iter().any(|r| matches!(r, Row::Key(_))), "{t:?}");
        }
    }

    #[test]
    fn every_setting_lives_in_exactly_one_tab() {
        let all: Vec<Row> = SettingsTab::ALL.into_iter().flat_map(|t| settings_rows(t, true)).collect();
        let knobs = [
            Knob::Sensitivity, Knob::Fov, Knob::InvertY, Knob::ThirdPerson, Knob::Shoulder, Knob::AdsMode, Knob::AdsSens,
            Knob::AdsSpeed, Knob::ShowPercent, Knob::PowerHud, Knob::SummonPreview, Knob::EnemyCompass, Knob::Outlines, Knob::OutlineWidth,
            Knob::OutlinePatterns, Knob::Comfort, Knob::SpeedFov, Knob::Roll, Knob::Shake, Knob::FovKick, Knob::LandingDip, Knob::Flashes,
            Knob::Vignette, Knob::Vsync, Knob::Fullscreen, Knob::Aa, Knob::LowLatency, Knob::Pipelined, Knob::Volume,
            Knob::MusicVol, Knob::WorldVol, Knob::AmbienceVol, Knob::FeedbackVol, Knob::MusicStyle, Knob::NightMode, Knob::Mono,
            Knob::Voice, Knob::Ono, Knob::OnoLang, Knob::Shout,
        ];
        for k in knobs {
            assert_eq!(all.iter().filter(|r| **r == Row::Value(k)).count(), 1, "{k:?}");
        }
        // Every tab starts with the strip and ends with BACK.
        for t in SettingsTab::ALL {
            let r = settings_rows(t, false);
            assert_eq!(r.first(), Some(&Row::Tabs));
            assert_eq!(r.last(), Some(&Row::Button(Act::Back, "BACK")));
        }
    }

    #[test]
    fn settings_knobs_are_local_and_step_both_ways() {
        let mut s = Settings::default();
        let mut p = UiPrefs::default();
        let game = p.game.clone();
        for t in SettingsTab::ALL {
            for row in settings_rows(t, true) {
                if let Row::Value(k) = row {
                    let mut c = Ctx { settings: &mut s, prefs: &mut p, net: None, voice: true, in_game: false };
                    assert!(!step_knob(k, &mut c, 1, false), "{k:?} touched the lobby rules");
                    step_knob(k, &mut c, -1, false);
                }
            }
        }
        assert_eq!(p.game, game);
        // Settings still persist through the file format.
        let text = ron::ser::to_string_pretty(&s, Default::default()).unwrap();
        assert_eq!(ron::from_str::<Settings>(&text).unwrap(), s);
    }

    #[test]
    fn constructs_section_has_every_cap_and_steps() {
        let mut s = Settings::default();
        let mut p = UiPrefs::default();
        p.game.rules.scale = ScaleLimit::NoLimits;
        let c = Ctx { settings: &mut s, prefs: &mut p, net: None, voice: true, in_game: false };
        let rows = page_rows(MenuPage::Rules, SettingsTab::Gameplay, &c);
        let at = rows.iter().position(|r| *r == Row::Header("CONSTRUCTS")).expect("CONSTRUCTS section");
        assert_eq!(rows[at + 1], Row::Value(Knob::Powerups));
        assert_eq!(rows[at + 2], Row::Value(Knob::Caps));
        for (i, l) in Lim::ALL.into_iter().enumerate() {
            assert_eq!(rows[at + 3 + i], Row::Value(Knob::Limit(l)), "{l:?}");
        }
        // Steppers: 0..=max, then UNLIMITED, and back down.
        assert_eq!(Lim::Structures.step(10, 1), UNLIMITED);
        assert_eq!(Lim::Structures.step(UNLIMITED, 1), UNLIMITED);
        assert_eq!(Lim::Structures.step(UNLIMITED, -1), 10);
        assert_eq!(Lim::Structures.step(0, -1), 0);
        assert_eq!(Lim::Held.step(1, 1), 1);
        assert_eq!(Lim::Total.step(1, -1), 1);
        assert_eq!((lim_text(0), lim_text(3), lim_text(UNLIMITED)), ("OFF".to_string(), "3".to_string(), "UNLIMITED".to_string()));
        // Knobs touch the lobby rules; CAPS toggles Normal ↔ Chaos, stepping makes it CUSTOM.
        let mut c = Ctx { settings: &mut s, prefs: &mut p, net: None, voice: true, in_game: false };
        assert!(step_knob(Knob::Caps, &mut c, 1, false));
        assert_eq!(c.prefs.game.rules.limits, ConstructLimits::chaos());
        assert!(step_knob(Knob::Limit(Lim::Kaiju), &mut c, -1, false));
        assert_eq!(c.prefs.game.rules.limits.kaiju, 2);
        assert_eq!(knob_text(Knob::Caps, &c).1, "CUSTOM");
        assert!(step_knob(Knob::Caps, &mut c, 1, false));
        assert_eq!(c.prefs.game.rules.limits, ConstructLimits::default());
        // POWER-UPS: off by default, one toggle, a lobby rule.
        assert_eq!(knob_text(Knob::Powerups, &c).1, "OFF");
        assert!(step_knob(Knob::Powerups, &mut c, 1, false));
        assert!(c.prefs.game.rules.powerups);
        // Colossal only shows in no-limits lobbies.
        c.prefs.game.rules.scale = ScaleLimit::Normal;
        assert!(!page_rows(MenuPage::Rules, SettingsTab::Gameplay, &c).contains(&Row::Value(Knob::Limit(Lim::Colossal))));
    }

    #[test]
    fn tab_ids() {
        for t in SettingsTab::ALL {
            assert_eq!(SettingsTab::from_id(&t.label().to_lowercase()), Some(t));
        }
        assert_eq!(SettingsTab::from_id("keys"), Some(SettingsTab::Controls));
        assert_eq!(SettingsTab::Hud.step(1), SettingsTab::Gameplay);
    }
}
