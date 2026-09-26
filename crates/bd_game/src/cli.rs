//! Command-line flags. Mostly for automated checks:
//!
//! ```text
//! brightest_day --practice --bench-debris 20000 --quit-after 12 --screenshot shot.png
//! ```
//!
//! | flag | meaning |
//! |---|---|
//! | `--practice` | skip the title menu and enter the practice scene |
//! | `--map arena`, `--map miami`, `--map orbit`, `--map tutorial` | which world to load (tutorial = movement course); goes straight in game |
//! | `--menu <page>` | open the menu on `title` / `settings` / `join` |
//! | `--screenshot <path>` | save a PNG of the primary window |
//! | `--screenshot-at <t1,t2,..>` | when (default: 0.5 s before `--quit-after`, else 3 s); several times ⇒ `<stem>_<i>.png` |
//! | `--script swing` | scripted input demo (aim at the centre tower, tether, reel, boost, release) |
//! | `--quit-after <secs>` | exit after N seconds and print a `PERF` summary line to stdout |
//! | `--bench <secs>` | regression gate: practice scene + continuous G-explosion stress for N s,
//! |   | writes frame stats JSON (avg/p50/p99/max/hitches) to `--bench-out` (default `bench.json`), quits |
//! | `--bench-out <path>` | JSON output path for `--bench` |
//! | `--bench-debris <n>` | keep `n` debris cubes alive in front of the camera |
//! | `--explode-at <secs>` | trigger a `G` explosion at the crosshair at time t |
//! | `--juice-test` | fire a medium impact (shake/FOV punch/flash/hit-stop) every second |
//! | `--pos x,y,z` / `--look yaw,pitch` | initial camera position / view angles (degrees) |
//! | `--size WxH` | window size override |
//! | `--windowed` | force windowed mode |
//! | `--aa off\|fxaa\|msaa`, `--latency N`, `--pipelined`, `--no-shadows`, `--vsync` | perf experiments (not saved) |

use bevy::prelude::*;
use std::path::PathBuf;

#[derive(Resource, Clone, Debug, Default)]
pub struct CliArgs {
    pub practice: bool,
    pub menu_page: Option<String>,
    pub screenshot: Option<PathBuf>,
    pub screenshot_at: Vec<f32>,
    pub script: Option<String>,
    /// `--map arena|tutorial` (implies going straight into the game).
    pub map: Option<String>,
    pub quit_after: Option<f32>,
    pub bench_debris: Option<usize>,
    /// `--bench <secs>`: stress benchmark duration.
    pub bench: Option<f32>,
    pub bench_out: Option<PathBuf>,
    pub explode_at: Option<f32>,
    pub juice_test: bool,
    pub pos: Option<Vec3>,
    /// Yaw, pitch in degrees.
    pub look: Option<Vec2>,
    pub size: Option<UVec2>,
    pub windowed: bool,
    pub aa: Option<String>,
    pub latency: Option<u32>,
    pub pipelined: bool,
    pub no_shadows: bool,
    pub vsync: bool,
}

impl CliArgs {
    pub fn parse() -> Self {
        Self::parse_from(std::env::args().skip(1))
    }

    pub fn parse_from(args: impl IntoIterator<Item = String>) -> Self {
        let mut out = CliArgs::default();
        let mut it = args.into_iter().peekable();
        while let Some(arg) = it.next() {
            let mut val = || it.next().unwrap_or_default();
            match arg.as_str() {
                "--practice" => out.practice = true,
                "--menu" => out.menu_page = Some(val()),
                "--screenshot" => out.screenshot = Some(PathBuf::from(val())),
                "--screenshot-at" => out.screenshot_at = val().split(',').filter_map(|t| t.trim().parse().ok()).collect(),
                "--script" => out.script = Some(val()),
                "--map" => out.map = Some(val()),
                "--quit-after" => out.quit_after = val().parse().ok(),
                "--bench-debris" => out.bench_debris = val().parse().ok(),
                "--bench" => out.bench = val().parse().ok(),
                "--bench-out" => out.bench_out = Some(PathBuf::from(val())),
                "--explode-at" => out.explode_at = val().parse().ok(),
                "--juice-test" => out.juice_test = true,
                "--pos" => out.pos = parse_floats(&val()).map(|v| Vec3::new(v[0], v[1], v[2])),
                "--look" => out.look = parse_floats(&val()).map(|v| Vec2::new(v[0], v[1])),
                "--size" => {
                    let v = val();
                    let mut parts = v.split(['x', 'X', ',']);
                    if let (Some(w), Some(h)) = (parts.next(), parts.next())
                        && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
                    {
                        out.size = Some(UVec2::new(w, h));
                    }
                }
                "--windowed" => out.windowed = true,
                "--aa" => out.aa = Some(val()),
                "--latency" => out.latency = val().parse().ok(),
                "--pipelined" => out.pipelined = true,
                "--no-shadows" => out.no_shadows = true,
                "--vsync" => out.vsync = true,
                other => eprintln!("brightest_day: ignoring unknown argument {other:?}"),
            }
        }
        out
    }

    /// Apply perf-experiment overrides to the loaded settings (these are not persisted unless the
    /// user later changes a setting in the menu).
    pub fn apply_overrides(&self, s: &mut crate::settings::Settings) {
        use crate::settings::AntiAliasing;
        match self.aa.as_deref() {
            Some("off") => s.antialiasing = AntiAliasing::Off,
            Some("fxaa") => s.antialiasing = AntiAliasing::Fxaa,
            Some("msaa") => s.antialiasing = AntiAliasing::Msaa4,
            _ => {}
        }
        if let Some(l) = self.latency {
            s.low_latency = l <= 1;
        }
        if self.pipelined {
            s.pipelined_rendering = true;
        }
        if self.vsync {
            s.vsync = true;
        }
        if self.size.is_some() || self.windowed {
            s.fullscreen = false;
        }
    }

    /// Automated screenshots `(path, time)`, in time order.
    pub fn screenshots(&self) -> Vec<(PathBuf, f32)> {
        let Some(path) = self.screenshot.as_ref() else { return Vec::new() };
        let mut times = self.screenshot_at.clone();
        if times.is_empty() {
            times.push(self.quit_after.map(|q| (q - 0.5).max(0.5)).unwrap_or(3.0));
        }
        times.sort_by(|a, b| a.total_cmp(b));
        if times.len() == 1 {
            return vec![(path.clone(), times[0])];
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "shot".into());
        let ext = path.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "png".into());
        times
            .iter()
            .enumerate()
            .map(|(i, t)| (path.with_file_name(format!("{stem}_{}.{ext}", i + 1)), *t))
            .collect()
    }
}

fn parse_floats(s: &str) -> Option<Vec<f32>> {
    let v: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    (v.len() >= 2).then(|| {
        let mut v = v;
        v.resize(3, 0.0);
        v
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags() {
        let a = CliArgs::parse_from(
            "--practice --screenshot a.png --quit-after 5 --bench-debris 20000 --look 10,-5 --size 1280x720"
                .split(' ')
                .map(String::from),
        );
        assert!(a.practice);
        assert_eq!(a.quit_after, Some(5.0));
        assert_eq!(a.bench_debris, Some(20000));
        assert_eq!(a.look, Some(Vec2::new(10.0, -5.0)));
        assert_eq!(a.size, Some(UVec2::new(1280, 720)));
        assert_eq!(a.screenshots(), vec![(PathBuf::from("a.png"), 4.5)]);
        let b = CliArgs::parse_from("--screenshot s.png --screenshot-at 2,1".split(' ').map(String::from));
        assert_eq!(b.screenshots(), vec![(PathBuf::from("s_1.png"), 1.0), (PathBuf::from("s_2.png"), 2.0)]);
    }
}
