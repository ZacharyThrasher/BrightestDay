//! `--record <dir|file.mp4> --record-secs N [--record-size WxH] [--record-fps F]`: capture exactly
//! `F` fps (default 30) of game time — the app runs on a fixed 1/F s frame step while recording
//! (the sim advances a fixed number of 120 Hz ticks per captured frame), so the clip plays at real
//! speed however long capture/encoding takes — then quit.
//!
//! * `<dir>`: a PNG frame sequence (`frame_%05d.png`, encoded on the IO task pool). Encode with
//!   `ffmpeg -framerate F -i <dir>/frame_%05d.png -c:v libx264 -pix_fmt yuv420p out.mp4`.
//! * `<file.mp4>`: frames are piped raw to `ffmpeg` (x264 CRF 18, yuv420p) on a writer thread, in
//!   order; no PNGs on disk. The pipe is bounded, so a slow encoder paces the (offline) capture.

use crate::practice::SimArgs;
use crate::state::AppState;
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::tasks::IoTaskPool;
use bevy::time::TimeUpdateStrategy;
use bevy::window::PrimaryWindow;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const RECORD_FPS: f32 = 30.0;
/// Game seconds in-game before the first frame is kept (let the scene settle).
const SETTLE_SECS: f32 = 1.5;

/// `--record-fps F` (parsed here; the shared parsers ignore it).
fn record_fps() -> f32 {
    let a: Vec<String> = std::env::args().collect();
    a.windows(2).find(|w| w[0] == "--record-fps").and_then(|w| w[1].parse::<f32>().ok()).filter(|f| (1.0..=240.0).contains(f)).unwrap_or(RECORD_FPS)
}

type Frame = (usize, u32, u32, Vec<u8>);

/// Ordered raw-frame pipe into ffmpeg.
struct Pipe {
    tx: Mutex<Option<SyncSender<Frame>>>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[derive(Resource)]
struct Recorder {
    dir: PathBuf,
    fps: f32,
    frames_total: usize,
    frame: usize,
    t_game: f32,
    pending: Arc<AtomicUsize>,
    done_at: Option<f32>,
    pipe: Option<Arc<Pipe>>,
}

pub struct RecordPlugin;

impl Plugin for RecordPlugin {
    fn build(&self, app: &mut App) {
        let args = app.world().resource::<SimArgs>().clone();
        let Some(dir) = args.record.clone() else { return };
        let fps = record_fps();
        let to_mp4 = dir.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
        let pipe = if to_mp4 {
            if let Some(parent) = dir.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            Some(start_pipe(dir.clone(), fps))
        } else {
            let _ = std::fs::create_dir_all(&dir);
            // Clear old frames so a shorter take doesn't leave a stale tail.
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().starts_with("frame_") {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            None
        };
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / fps as f64)))
            .insert_resource(Recorder {
                dir,
                fps,
                frames_total: (args.record_secs.max(0.5) * fps).round() as usize,
                frame: 0,
                t_game: 0.0,
                pending: Arc::new(AtomicUsize::new(0)),
                done_at: None,
                pipe,
            })
            .add_systems(Startup, size_window)
            .add_systems(Last, capture.run_if(in_state(AppState::InGame)));
    }
}

/// Writer thread: reorders frames by index and streams RGB24 into `ffmpeg` (spawned on the first
/// frame, when the size is known).
fn start_pipe(out: PathBuf, fps: f32) -> Arc<Pipe> {
    let (tx, rx) = sync_channel::<Frame>(6);
    let join = std::thread::Builder::new()
        .name("record-ffmpeg".into())
        .spawn(move || {
            let mut next = 0usize;
            let mut buf: BTreeMap<usize, (u32, u32, Vec<u8>)> = BTreeMap::new();
            let mut child: Option<(std::process::Child, u32, u32)> = None;
            let write = |w: u32, h: u32, data: &[u8], child: &mut Option<(std::process::Child, u32, u32)>| {
                if child.is_none() {
                    let c = std::process::Command::new("ffmpeg")
                        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s"])
                        .arg(format!("{w}x{h}"))
                        .arg("-r")
                        .arg(format!("{fps}"))
                        .args(["-i", "-", "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
                        .arg(&out)
                        .stdin(std::process::Stdio::piped())
                        .spawn();
                    match c {
                        Ok(c) => *child = Some((c, w, h)),
                        Err(e) => {
                            eprintln!("record: could not start ffmpeg: {e}");
                            return;
                        }
                    }
                }
                if let Some((c, cw, ch)) = child.as_mut()
                    && *cw == w
                    && *ch == h
                    && let Some(stdin) = c.stdin.as_mut()
                {
                    let _ = stdin.write_all(data);
                }
            };
            while let Ok((i, w, h, data)) = rx.recv() {
                buf.insert(i, (w, h, data));
                while let Some((w, h, d)) = buf.remove(&next) {
                    write(w, h, &d, &mut child);
                    next += 1;
                }
            }
            // Sender dropped: flush whatever is left in order, then finish the file.
            for (_, (w, h, d)) in std::mem::take(&mut buf) {
                write(w, h, &d, &mut child);
            }
            if let Some((mut c, _, _)) = child {
                drop(c.stdin.take());
                let _ = c.wait();
            }
        })
        .ok();
    Arc::new(Pipe { tx: Mutex::new(Some(tx)), join: Mutex::new(join) })
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
    rec.t_game += 1.0 / rec.fps;
    if rec.t_game < SETTLE_SECS {
        return;
    }
    if rec.frame >= rec.frames_total {
        // Wait for the encoders, then quit.
        let t = rec.t_game;
        let done = *rec.done_at.get_or_insert(t);
        if rec.pending.load(Ordering::Acquire) == 0 || t - done > 20.0 {
            if let Some(p) = rec.pipe.take() {
                drop(p.tx.lock().ok().and_then(|mut g| g.take()));
                if let Some(j) = p.join.lock().ok().and_then(|mut g| g.take()) {
                    let _ = j.join();
                }
            }
            info!("record: {} frames written to {}", rec.frames_total, rec.dir.display());
            println!("RECORD {} frames {}", rec.frames_total, rec.dir.display());
            exit.write(AppExit::Success);
        }
        return;
    }
    let index = rec.frame;
    rec.frame += 1;
    let pending = rec.pending.clone();
    pending.fetch_add(1, Ordering::AcqRel);
    if let Some(pipe) = rec.pipe.clone() {
        commands.spawn(Screenshot::primary_window()).observe(move |shot: On<ScreenshotCaptured>| {
            let img = shot.image.clone();
            let pipe = pipe.clone();
            let pending = pending.clone();
            IoTaskPool::get()
                .spawn(async move {
                    if let Ok(d) = img.try_into_dynamic() {
                        let rgb = d.to_rgb8();
                        let (w, h) = rgb.dimensions();
                        let tx = pipe.tx.lock().ok().and_then(|g| g.clone());
                        if let Some(tx) = tx {
                            let _ = tx.send((index, w, h, rgb.into_raw()));
                        }
                    }
                    pending.fetch_sub(1, Ordering::AcqRel);
                })
                .detach();
        });
        return;
    }
    let path = rec.dir.join(format!("frame_{index:05}.png"));
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
