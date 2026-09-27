//! Primary window setup: present mode, fullscreen toggle (F11 / Alt+Enter), low-latency swapchain.

use crate::cli::CliArgs;
use crate::settings::Settings;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode, WindowResolution};
use std::num::NonZero;

pub fn present_mode(vsync: bool) -> PresentMode {
    if vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync }
}

pub fn window_mode(fullscreen: bool) -> WindowMode {
    if fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    }
}

pub fn initial_window(settings: &Settings, cli: &CliArgs) -> Window {
    let size = cli.size.unwrap_or(UVec2::new(settings.window_size.0, settings.window_size.1));
    let fullscreen = settings.fullscreen && !cli.windowed && cli.size.is_none();
    Window {
        title: "Brightest Day".into(),
        resolution: WindowResolution::new(size.x, size.y),
        present_mode: present_mode(settings.vsync),
        mode: window_mode(fullscreen),
        // Only one frame in flight in the swapchain queue: lowest input-to-photon latency.
        desired_maximum_frame_latency: match cli.latency {
            Some(n) => NonZero::new(n.max(1)),
            None if settings.low_latency => NonZero::new(1),
            None => None,
        },
        // `BD_OFFSCREEN_WINDOW=1`: benchmarks — a real (uncapped) window parked off the desktop,
        // never focused (a hidden window is presented at the compositor's pace).
        position: if std::env::var_os("BD_OFFSCREEN_WINDOW").is_some() {
            WindowPosition::At(IVec2::new(-6000, -6000))
        } else {
            WindowPosition::Centered(MonitorSelection::Primary)
        },
        // `BD_HIDDEN_WINDOW=1`: automated recordings / screenshots without a window popping up
        // (or stealing focus) on the desktop. Frames still render and capture.
        visible: std::env::var_os("BD_HIDDEN_WINDOW").is_none(),
        focused: std::env::var_os("BD_HIDDEN_WINDOW").is_none() && std::env::var_os("BD_OFFSCREEN_WINDOW").is_none(),
        ..default()
    }
}

pub struct WindowSetupPlugin;

impl Plugin for WindowSetupPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("BD_OFFSCREEN_WINDOW").is_some() || std::env::var_os("BD_UNCAPPED_UNFOCUSED").is_some() {
            // Benchmarks: an unfocused window would otherwise update at 60 Hz (Bevy's game default).
            app.insert_resource(bevy::winit::WinitSettings { focused_mode: bevy::winit::UpdateMode::Continuous, unfocused_mode: bevy::winit::UpdateMode::Continuous });
        }
        app.add_systems(Update, (toggle_fullscreen, apply_window_settings).chain());
    }
}

fn toggle_fullscreen(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>) {
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let alt_enter = alt && keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    if keys.just_pressed(KeyCode::F11) || alt_enter {
        settings.fullscreen = !settings.fullscreen;
    }
}

fn apply_window_settings(
    settings: Res<Settings>,
    cli: Res<CliArgs>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    if !settings.is_changed() {
        return;
    }
    let pm = present_mode(settings.vsync);
    if window.present_mode != pm {
        window.present_mode = pm;
    }
    let fullscreen = settings.fullscreen && !((cli.windowed || cli.size.is_some()) && settings.is_added());
    let mode = window_mode(fullscreen);
    if window.mode != mode {
        window.mode = mode;
    }
}
