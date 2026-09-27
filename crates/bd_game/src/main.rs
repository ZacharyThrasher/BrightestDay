//! Brightest Day client shell.
//!
//! Plugin layout (each module owns one plugin; see the module docs for the integration API):
//! - [`cli`]          command-line flags (automation: screenshots, benchmarks)
//! - [`settings`]     persisted user settings (RON)
//! - [`window`]       present mode / fullscreen toggles
//! - [`state`]        app states (menu / in-game) and game mode
//! - [`tick`]         fixed 120 Hz `SimTick` schedule + render interpolation alpha
//! - [`input`]        raw per-frame mouse look + per-tick `PlayerInput` accumulation
//! - [`camera`]       first-person camera, dynamic FOV, final camera transform
//! - [`player`]       local player: bd_sim movement on the tick, interpolated eye, anchor preview
//! - [`audio`]        SFX manifest playback, movement sounds, speed-driven loops
//! - [`render`]       sky, sun, fog, bloom, tonemapping, voxel chunk rendering
//! - [`debris`]       pooled GPU-instanced debris cubes + shatter helper
//! - [`juice`]        hit-stop, camera shake, FOV punch, screen flash/vignette
//! - [`menu`]         title / join / settings / pause menus
//! - [`hud`]          crosshair + F3 perf overlay
//! - [`arena`]        bd_voxel Archipelago: generation, chunk meshing, carving (G)
//! - [`tutorial`]     movement course: station teleports, peak-run timer, ghosts, speedometer
//! - [`warmup`]       loading screen + render-pipeline pre-warm (no mid-match shader hitches)
//! - [`automation`]   `--screenshot`, `--quit-after`, `--bench-debris`

mod alloc_count;
mod arena;
mod audio;
mod automation;
mod camera;
mod cli;
mod combat_tuning;
mod console;
mod debris;
mod hotbar; // POWER HOTBAR: keys 1–4 as a dynamic power bar (auto-bind, Tab drag-drop, dismiss by slot)
mod hud;
mod input;
mod juice;
mod lobby;
mod menu;
mod net;
mod oath;
mod player;
mod power_hud;
mod practice;
mod readability;
mod present;
mod render;
mod settings;
mod showcase;
mod sim_bridge;
mod state;
mod tick;
mod toolbox; // TOOLBOX: kit / borrowed moves + chains (console + voice)
mod tutorial;
mod unstick;
mod util;
mod viewmodel;
mod voice_fluency; // VOICE FLUENCY: transcript → the line the console reads (voice == typing)
mod voice_ui;
mod warmup;
mod window;

use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The workspace `assets/` directory. Resolution order: `BD_ASSETS` env var, `assets/` next to the
/// executable, the nearest ancestor of the executable containing `assets/sfx`, `./assets`.
pub fn assets_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(p) = std::env::var_os("BD_ASSETS") {
            return PathBuf::from(p);
        }
        if let Ok(exe) = std::env::current_exe() {
            for dir in exe.ancestors().skip(1) {
                let cand = dir.join("assets");
                if cand.join("sfx").is_dir() || cand.join("oaths.ron").is_file() {
                    return cand;
                }
            }
        }
        std::env::current_dir().map(|d| d.join("assets")).unwrap_or_else(|_| PathBuf::from("assets"))
    })
    .clone()
}

fn main() {
    let cli = cli::CliArgs::parse();
    let mut settings = settings::Settings::load();
    cli.apply_overrides(&mut settings);

    let mut app = App::new();
    let primary_window = window::initial_window(&settings, &cli);

    let default_plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(primary_window),
            ..default()
        })
        .set(AssetPlugin { file_path: assets_dir().to_string_lossy().into_owned(), ..default() })
        .set(bevy::log::LogPlugin {
            filter: "wgpu=error,naga=warn,bevy_render=info,bevy_app=info".into(),
            ..default()
        });

    // Pipelined rendering overlaps frame N's render with frame N+1's simulation, which adds a frame
    // of input latency. We would rather pay the (small) CPU cost and keep the mouse snappy.
    // Bevy's audio output is replaced by bd_audio's mixer on its own stream (`audio`).
    if settings.pipelined_rendering {
        app.add_plugins(default_plugins.build().disable::<bevy::audio::AudioPlugin>());
    } else {
        app.add_plugins(default_plugins.build().disable::<PipelinedRenderingPlugin>().disable::<bevy::audio::AudioPlugin>());
    }

    app.insert_resource(cli)
        .insert_resource(settings)
        .add_plugins((
            settings::SettingsPlugin,
            window::WindowSetupPlugin,
            state::StatePlugin,
            tick::TickPlugin,
            input::InputPlugin,
            camera::CameraPlugin,
            player::PlayerPlugin,
            render::RenderSetupPlugin,
            debris::DebrisPlugin,
            juice::JuicePlugin,
        ))
        .add_plugins((
            menu::MenuPlugin,
            hud::HudPlugin,
            arena::ArenaPlugin,
            warmup::WarmupPlugin,
            automation::AutomationPlugin,
            audio::AudioFxPlugin,
            tutorial::TutorialPlugin,
        ))
        .add_plugins(sim_bridge::SimBridgePlugin)
        .add_plugins(combat_tuning::CombatTuningPlugin)
        .add_plugins((practice::PracticePlugin, present::PresentPlugin))
        .add_plugins(net::NetPlugin)
        .add_plugins(lobby::UiPlugin)
        .add_plugins(showcase::ShowcasePlugin)
        .add_plugins(power_hud::PowerHudPlugin) // POWER HUD: ring gems, motes, minimal strip
        .add_plugins(hotbar::HotbarPlugin) // POWER HOTBAR (after the strip + prefs)
        .add_plugins(readability::ReadabilityPlugin) // READABILITY: damage heat ramp, KO smoke, over-head %
        .add_plugins(alloc_count::AllocCountPlugin)
        .add_plugins(unstick::UnstickPlugin) // UNSTICK: Esc-menu / key request, timer, arrival shimmer
        .run();
}
