//! `--record <dir> --record-secs N [--record-size WxH]`: dump a PNG frame sequence at exactly 30 fps
//! of game time (the app runs on a fixed 1/30 s frame step while recording, so the clip plays at
//! real speed regardless of how long capture/encoding takes), then quit. Encode with e.g.
//! `ffmpeg -framerate 30 -i <dir>/frame_%05d.png -c:v libx264 -pix_fmt yuv420p out.mp4`.
//! PNG encoding runs on the IO task pool (never on the main thread).

use crate::practice::SimArgs;
use crate::state::AppState;
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::tasks::IoTaskPool;
use bevy::time::TimeUpdateStrategy;
use bevy::window::PrimaryWindow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const RECORD_FPS: f32 = 30.0;
/// Game seconds in-game before the first frame is kept (let the scene settle).
const SETTLE_SECS: f32 = 1.5;

#[derive(Resource)]
struct Recorder {
    dir: PathBuf,
    frames_total: usize,
    frame: usize,
    t_game: f32,
    pending: Arc<AtomicUsize>,
    done_at: Option<f32>,
}

pub struct RecordPlugin;

impl Plugin for RecordPlugin {
    fn build(&self, app: &mut App) {
        let args = app.world().resource::<SimArgs>().clone();
        let Some(dir) = args.record.clone() else { return };
        let _ = std::fs::create_dir_all(&dir);
        // Clear old frames so a shorter take doesn't leave a stale tail.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("frame_") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / RECORD_FPS)))
            .insert_resource(Recorder {
                dir,
                frames_total: (args.record_secs.max(0.5) * RECORD_FPS).round() as usize,
                frame: 0,
                t_game: 0.0,
                pending: Arc::new(AtomicUsize::new(0)),
                done_at: None,
            })
            .add_systems(Startup, size_window)
            .add_systems(Last, capture.run_if(in_state(AppState::InGame)));
    }
}

fn size_window(args: Res<SimArgs>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    let Some(size) = args.record_size else { return };
    for mut w in &mut windows {
        w.mode = bevy::window::WindowMode::Windowed;
        w.resolution.set(size.x as f32, size.y as f32);
        w.resolution.set_scale_factor_override(Some(1.0));
    }
}

fn capture(mut commands: Commands, mut rec: ResMut<Recorder>, mut exit: MessageWriter<AppExit>) {
    rec.t_game += 1.0 / RECORD_FPS;
    if rec.t_game < SETTLE_SECS {
        return;
    }
    if rec.frame >= rec.frames_total {
        // Wait for the encoders, then quit.
        let t = rec.t_game;
        let done = *rec.done_at.get_or_insert(t);
        if rec.pending.load(Ordering::Acquire) == 0 || t - done > 20.0 {
            info!("record: {} frames written to {}", rec.frames_total, rec.dir.display());
            println!("RECORD {} frames {}", rec.frames_total, rec.dir.display());
            exit.write(AppExit::Success);
        }
        return;
    }
    let path = rec.dir.join(format!("frame_{:05}.png", rec.frame));
    rec.frame += 1;
    let pending = rec.pending.clone();
    pending.fetch_add(1, Ordering::AcqRel);
    commands.spawn(Screenshot::primary_window()).observe(move |shot: On<ScreenshotCaptured>| {
        let img = shot.image.clone();
        let path = path.clone();
        let pending = pending.clone();
        IoTaskPool::get()
            .spawn(async move {
                if let Ok(d) = img.try_into_dynamic() {
                    let _ = d.to_rgb8().save(&path);
                }
                pending.fetch_sub(1, Ordering::AcqRel);
            })
            .detach();
    });
}
