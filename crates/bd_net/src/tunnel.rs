//! Cloudflare quick tunnel launcher.
//!
//! `Tunnel::start(port)` runs `cloudflared tunnel --no-autoupdate --url http://127.0.0.1:PORT`,
//! parses the `https://<words>.trycloudflare.com` address from its log (stderr), and reports
//! `Ready` once cloudflared has registered a connection with the Cloudflare edge. Friends join with
//! just `<words>` (the lobby code). The child process is killed when the `Tunnel` is dropped; on
//! Windows it is also placed in a kill-on-close job object so it dies even if the game crashes.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::address::lobby_code_from_url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TunnelStatus {
    /// cloudflared is starting / negotiating.
    Starting,
    /// Public URL (`https://<code>.trycloudflare.com`) and the lobby code.
    Ready { url: String, code: String },
    /// Could not start, or cloudflared exited. The reason is human readable.
    Failed(String),
}

struct Shared {
    status: TunnelStatus,
    url: Option<String>,
    log: VecDeque<String>,
}

pub struct Tunnel {
    child: Option<Child>,
    shared: Arc<Mutex<Shared>>,
    #[cfg(windows)]
    _job: Option<job::Job>,
}

const LOG_LINES: usize = 40;

impl Tunnel {
    /// Locate `cloudflared`: `$BD_CLOUDFLARED`, next to the game executable, `PATH`, then the
    /// standard install locations.
    pub fn find_cloudflared() -> Option<PathBuf> {
        let exe_name = if cfg!(windows) { "cloudflared.exe" } else { "cloudflared" };
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(p) = std::env::var_os("BD_CLOUDFLARED") {
            candidates.push(PathBuf::from(p));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join(exe_name));
                candidates.push(dir.join("cloudflared").join(exe_name));
            }
        }
        if let Some(path) = std::env::var_os("PATH") {
            candidates.extend(std::env::split_paths(&path).map(|d| d.join(exe_name)));
        }
        if cfg!(windows) {
            candidates.push(PathBuf::from(r"C:\Program Files (x86)\cloudflared\cloudflared.exe"));
            candidates.push(PathBuf::from(r"C:\Program Files\cloudflared\cloudflared.exe"));
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                candidates.push(PathBuf::from(local).join("cloudflared").join(exe_name));
            }
        } else {
            for d in ["/usr/local/bin", "/usr/bin", "/opt/homebrew/bin"] {
                candidates.push(PathBuf::from(d).join(exe_name));
            }
        }
        candidates.into_iter().find(|p| p.is_file())
    }

    /// Start a quick tunnel to the local host port. Never fails; check [`status`](Self::status).
    pub fn start(port: u16) -> Self {
        let shared = Arc::new(Mutex::new(Shared { status: TunnelStatus::Starting, url: None, log: VecDeque::new() }));
        let fail = |msg: String| {
            shared.lock().unwrap().status = TunnelStatus::Failed(msg);
            Tunnel {
                child: None,
                shared: shared.clone(),
                #[cfg(windows)]
                _job: None,
            }
        };
        let Some(exe) = Self::find_cloudflared() else {
            return fail(
                "cloudflared not found — install it (winget install Cloudflare.cloudflared) or put cloudflared.exe next to the game".into(),
            );
        };
        let mut cmd = Command::new(&exe);
        cmd.args(["tunnel", "--no-autoupdate", "--url", &format!("http://127.0.0.1:{port}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return fail(format!("could not start {}: {e}", exe.display())),
        };
        #[cfg(windows)]
        let job = job::Job::kill_on_close_for(&child);
        let stderr = child.stderr.take().unwrap();
        let sh = shared.clone();
        std::thread::Builder::new()
            .name("bd-cloudflared".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    let mut s = sh.lock().unwrap();
                    if s.url.is_none() {
                        s.url = parse_tunnel_url(&line);
                    }
                    if line.contains("Registered tunnel connection") {
                        if let (TunnelStatus::Starting, Some(url)) = (&s.status, &s.url) {
                            let code = lobby_code_from_url(url).unwrap_or_default();
                            s.status = TunnelStatus::Ready { url: url.clone(), code };
                        }
                    }
                    if s.log.len() == LOG_LINES {
                        s.log.pop_front();
                    }
                    s.log.push_back(line);
                }
                let mut s = sh.lock().unwrap();
                let errors: Vec<&str> = s.log.iter().filter(|l| l.contains(" ERR ") || l.contains("error")).map(|l| l.as_str()).collect();
                let detail = errors.last().copied().or(s.log.back().map(|l| l.as_str())).unwrap_or("no output").to_string();
                s.status = TunnelStatus::Failed(format!("cloudflared exited: {detail}"));
            })
            .expect("spawn cloudflared log thread");
        Tunnel {
            child: Some(child),
            shared,
            #[cfg(windows)]
            _job: job,
        }
    }

    pub fn status(&self) -> TunnelStatus {
        self.shared.lock().unwrap().status.clone()
    }

    /// The public URL as soon as cloudflared printed it (may be before `Ready`).
    pub fn url(&self) -> Option<String> {
        self.shared.lock().unwrap().url.clone()
    }

    /// Last lines cloudflared logged (for an error dialog).
    pub fn log_tail(&self) -> Vec<String> {
        self.shared.lock().unwrap().log.iter().cloned().collect()
    }

    /// Block until ready or failed (for tools/tests; the game should poll `status()` instead).
    pub fn wait_ready(&self, timeout: Duration) -> Result<(String, String), String> {
        let end = Instant::now() + timeout;
        loop {
            match self.status() {
                TunnelStatus::Ready { url, code } => return Ok((url, code)),
                TunnelStatus::Failed(e) => return Err(e),
                TunnelStatus::Starting if Instant::now() >= end => return Err("timed out waiting for cloudflared".into()),
                TunnelStatus::Starting => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    /// Kill cloudflared.
    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Find `https://<words>.trycloudflare.com` in a cloudflared log line.
pub fn parse_tunnel_url(line: &str) -> Option<String> {
    let mut rest = line;
    while let Some(i) = rest.find("https://") {
        let cand = &rest[i..];
        let end = cand.find(|c: char| c.is_whitespace() || c == '|' || c == '"').unwrap_or(cand.len());
        let url = cand[..end].trim_end_matches('/');
        if url.ends_with(".trycloudflare.com") && !url.starts_with("https://api.") && lobby_code_from_url(url).is_some() {
            return Some(url.to_string());
        }
        rest = &cand[8..];
    }
    None
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    /// Job object that kills its processes when the last handle closes (i.e. when we exit).
    pub struct Job(HANDLE);
    // SAFETY: a job handle is a kernel handle usable from any thread.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub fn kill_on_close_for(child: &std::process::Child) -> Option<Job> {
            // SAFETY: plain Win32 calls with valid arguments; the handle is closed on failure.
            unsafe {
                let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if h.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    h,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 || AssignProcessToJobObject(h, child.as_raw_handle() as HANDLE) == 0 {
                    CloseHandle(h);
                    return None;
                }
                Some(Job(h))
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: we own the handle.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_url() {
        let l = "2026-09-26T10:00:00Z INF |  https://quiet-lantern-green-oath.trycloudflare.com                                 |";
        assert_eq!(parse_tunnel_url(l).as_deref(), Some("https://quiet-lantern-green-oath.trycloudflare.com"));
        let e = r#"2026 ERR failed to request quick Tunnel: Post "https://api.trycloudflare.com/tunnel": dial tcp"#;
        assert_eq!(parse_tunnel_url(e), None);
        assert_eq!(parse_tunnel_url("INF Requesting new quick Tunnel on trycloudflare.com..."), None);
    }
}
