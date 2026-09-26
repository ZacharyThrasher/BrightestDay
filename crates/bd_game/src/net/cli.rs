//! Networking command-line flags (parsed from `std::env::args` independently of `cli.rs`).
//!
//! | flag | meaning |
//! |---|---|
//! | `--host` | host a lobby (cloudflared quick tunnel + LAN) and start the match right away |
//! | `--port N` | host port (default 27960, falls back to a free one) |
//! | `--no-tunnel` | LAN / `ip:port` only |
//! | `--join <code\|ip:port\|url>` | join a lobby |
//! | `--name <s>` / `--corps <name\|0-7>` | player name / corps |
//! | `--headless` | no window: `--host` runs a bot-filled authoritative server, `--join` a scripted bot client |
//! | `--quit-after <s>` | (headless) run this long, print `NETSTATS {json}` and exit |
//! | `--lag-ms N` / `--jitter-ms N` | (join) route through an in-process `LagProxy` (one-way ms each; ip:port only) |
//! | `--net-check-secs S` | (host) terrain hash interval |
//! | `--net-bots N` | (headless host) server-side bots |
//! | `--seed N` | (headless) script / bot seed |
//! | `--net-map archipelago\|miami` | (headless host) map |

use bd_core::corps::Corps;

#[derive(Clone, Debug, Default)]
pub struct NetCli {
    pub host: bool,
    pub port: Option<u16>,
    pub no_tunnel: bool,
    pub join: Option<String>,
    pub name: Option<String>,
    pub corps: Option<Corps>,
    pub headless: bool,
    pub quit_after: Option<f32>,
    pub lag_ms: f32,
    pub jitter_ms: f32,
    pub check_secs: Option<f32>,
    pub bots: u8,
    pub seed: u64,
    /// (headless host) `archipelago` | `miami`.
    pub map: Option<String>,
}

pub fn parse_corps(s: &str) -> Option<Corps> {
    if let Ok(i) = s.parse::<u8>() {
        return Some(Corps::from_index(i));
    }
    Corps::ALL.iter().copied().find(|c| c.name().eq_ignore_ascii_case(s.trim()))
}

impl NetCli {
    pub fn from_env() -> Self {
        Self::parse_from(std::env::args().skip(1))
    }

    pub fn parse_from(args: impl IntoIterator<Item = String>) -> Self {
        let mut o = NetCli { seed: 1, ..Default::default() };
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            let mut val = || it.next().unwrap_or_default();
            match a.as_str() {
                "--host" => o.host = true,
                "--port" => o.port = val().parse().ok(),
                "--no-tunnel" => o.no_tunnel = true,
                "--join" => o.join = Some(val()),
                "--name" => o.name = Some(val()),
                "--corps" => o.corps = parse_corps(&val()),
                "--headless" | "--headless-client" => o.headless = true,
                "--quit-after" => o.quit_after = val().parse().ok(),
                "--lag-ms" => o.lag_ms = val().parse().unwrap_or(0.0),
                "--jitter-ms" => o.jitter_ms = val().parse().unwrap_or(0.0),
                "--net-check-secs" => o.check_secs = val().parse().ok(),
                "--net-bots" => o.bots = val().parse().unwrap_or(0),
                "--seed" => o.seed = val().parse().unwrap_or(1),
                "--net-map" => o.map = Some(val().to_ascii_lowercase()),
                _ => {}
            }
        }
        o
    }

    pub fn active(&self) -> bool {
        self.host || self.join.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let c = NetCli::parse_from("--join quiet-lantern --name Zed --corps rage --headless --lag-ms 40".split(' ').map(String::from));
        assert_eq!(c.join.as_deref(), Some("quiet-lantern"));
        assert_eq!(c.corps, Some(Corps::Rage));
        assert!(c.headless && !c.host);
        assert_eq!(c.lag_ms, 40.0);
        let h = NetCli::parse_from("--host --port 4000 --no-tunnel".split(' ').map(String::from));
        assert!(h.host && h.no_tunnel);
        assert_eq!(h.port, Some(4000));
    }
}
