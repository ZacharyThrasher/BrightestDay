//! Persisted user settings.
//!
//! Location: if a `settings.ron` exists next to the executable it is used (portable install);
//! otherwise `%APPDATA%\BrightestDay\settings.ron` (or `$XDG_CONFIG_HOME`/`~/.config` elsewhere).
//! Any change to the [`Settings`] resource is written back after a short debounce.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AntiAliasing {
    Off,
    Fxaa,
    #[default]
    Msaa4,
}

impl AntiAliasing {
    pub fn next(self) -> Self {
        match self {
            AntiAliasing::Off => AntiAliasing::Fxaa,
            AntiAliasing::Fxaa => AntiAliasing::Msaa4,
            AntiAliasing::Msaa4 => AntiAliasing::Off,
        }
    }
    pub fn prev(self) -> Self {
        self.next().next()
    }
    pub fn label(self) -> &'static str {
        match self {
            AntiAliasing::Off => "OFF",
            AntiAliasing::Fxaa => "FXAA",
            AntiAliasing::Msaa4 => "MSAA 4X",
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Source-engine sensitivity: degrees per count = 0.022 × sensitivity.
    pub sensitivity: f32,
    pub invert_y: bool,
    /// Horizontal field of view in degrees at the window's aspect ratio.
    pub fov: f32,
    /// Motion-comfort options: every camera effect is routed through these.
    pub comfort: ComfortSettings,
    pub vsync: bool,
    pub fullscreen: bool,
    /// Windowed-mode size in physical pixels.
    pub window_size: (u32, u32),
    /// 0..1
    pub master_volume: f32,
    /// World / ambience / feedback / music volumes, night mode, mono, music style.
    pub audio: crate::audio::AudioPrefs,
    pub antialiasing: AntiAliasing,
    /// Allow the GPU driver to queue only one frame (lower latency). Applied at startup.
    pub low_latency: bool,
    /// Bevy pipelined rendering (+throughput, +1 frame latency). Applied at startup.
    pub pipelined_rendering: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: 1.5,
            invert_y: false,
            fov: 100.0,
            comfort: ComfortSettings::default(),
            vsync: false,
            fullscreen: false,
            window_size: (1600, 900),
            master_volume: 0.8,
            audio: crate::audio::AudioPrefs::default(),
            antialiasing: AntiAliasing::Msaa4,
            low_latency: true,
            pipelined_rendering: false,
        }
    }
}

/// Motion comfort (docs/DESIGN.md "Motion comfort"). The camera never rolls regardless of these.
/// Mirrored into a `ComfortSettings` resource that every camera effect reads.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ComfortSettings {
    /// Camera shake strength (0 = off). Shake only translates + tiny pitch/yaw.
    pub shake: f32,
    /// Speed-based FOV widening.
    pub speed_fov: bool,
    /// FOV punches / kicks strength (0 = off).
    pub fov_kick: f32,
    /// Landing dip / step smoothing kick strength (0 = off).
    pub landing_dip: f32,
    /// Full-screen flashes + hit vignette strength (0 = off; photosensitivity).
    pub flashes: f32,
    /// Comfort vignette strength (edge darkening scaled by speed / turn rate; 0 = off).
    pub vignette: f32,
}

impl Default for ComfortSettings {
    fn default() -> Self {
        Self { shake: 1.0, speed_fov: true, fov_kick: 1.0, landing_dip: 1.0, flashes: 1.0, vignette: 0.0 }
    }
}

impl ComfortSettings {
    /// The one-click Comfort preset.
    pub const COMFORT: Self = Self { shake: 0.0, speed_fov: false, fov_kick: 0.0, landing_dip: 0.0, flashes: 0.0, vignette: 0.7 };

    pub fn is_comfort_preset(&self) -> bool {
        *self == Self::COMFORT
    }

    pub fn sanitized(mut self) -> Self {
        let c = |v: f32, hi: f32| if v.is_finite() { v.clamp(0.0, hi) } else { 1.0 };
        self.shake = c(self.shake, 2.0);
        self.fov_kick = c(self.fov_kick, 2.0);
        self.landing_dip = c(self.landing_dip, 2.0);
        self.flashes = c(self.flashes, 1.0);
        self.vignette = c(self.vignette, 1.0);
        self
    }
}

pub const FOV_MIN: f32 = 70.0;
pub const FOV_MAX: f32 = 130.0;

impl Settings {
    pub fn path() -> PathBuf {
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            let portable = dir.join("settings.ron");
            if portable.exists() {
                return portable;
            }
        }
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("BrightestDay").join("settings.ron")
    }

    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match ron::from_str::<Settings>(&text) {
                Ok(s) => s.sanitized(),
                Err(e) => {
                    eprintln!("settings: failed to parse {}: {e}; using defaults", path.display());
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn sanitized(mut self) -> Self {
        let d = Settings::default();
        if !self.sensitivity.is_finite() || self.sensitivity <= 0.0 {
            self.sensitivity = d.sensitivity;
        }
        self.sensitivity = self.sensitivity.clamp(0.05, 20.0);
        if !self.fov.is_finite() {
            self.fov = d.fov;
        }
        self.fov = self.fov.clamp(FOV_MIN, FOV_MAX);
        self.master_volume = if self.master_volume.is_finite() { self.master_volume.clamp(0.0, 1.0) } else { 0.8 };
        self.audio = self.audio.sanitized();
        self.comfort = self.comfort.sanitized();
        self.window_size.0 = self.window_size.0.clamp(640, 7680);
        self.window_size.1 = self.window_size.1.clamp(360, 4320);
        self
    }
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let comfort = app.world().resource::<Settings>().comfort;
        app.insert_resource(comfort)
            .add_systems(PreUpdate, mirror_comfort)
            // `Last`: sees this frame's changes and an `AppExit` written this frame.
            .add_systems(Last, autosave_settings);
    }
}

fn mirror_comfort(settings: Res<Settings>, mut comfort: ResMut<ComfortSettings>) {
    if settings.is_changed() && *comfort != settings.comfort {
        *comfort = settings.comfort;
    }
}

/// Debounced autosave: write 0.5 s after the last change, or right away when the app is quitting.
fn autosave_settings(settings: Res<Settings>, time: Res<Time<Real>>, mut exit: MessageReader<AppExit>, mut dirty_since: Local<Option<f32>>) {
    let now = time.elapsed_secs();
    if settings.is_changed() && !settings.is_added() {
        *dirty_since = Some(now);
    }
    let quitting = exit.read().count() > 0;
    if let Some(t) = *dirty_since
        && (now - t > 0.5 || quitting)
    {
        *dirty_since = None;
        if let Err(e) = settings.save() {
            warn!("failed to save settings: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ron() {
        let s = Settings { sensitivity: 2.25, fov: 110.0, ..Default::default() };
        let text = ron::ser::to_string_pretty(&s, Default::default()).unwrap();
        let back: Settings = ron::from_str(&text).unwrap();
        assert_eq!(s, back);
        // Missing fields fall back to defaults.
        let partial: Settings = ron::from_str("(sensitivity: 3.0)").unwrap();
        assert_eq!(partial.sensitivity, 3.0);
        assert_eq!(partial.fov, Settings::default().fov);
    }
}
