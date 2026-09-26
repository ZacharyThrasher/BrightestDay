//! Turning whatever the player typed into a WebSocket URL.
//!
//! Accepted forms:
//! - `ws://host:port[/path]`, `wss://host[/path]`
//! - `http(s)://...` (rewritten to `ws(s)://`, handy when pasting the tunnel URL)
//! - `something.trycloudflare.com` → `wss://something.trycloudflare.com`
//! - `ip:port`, `host:port` → `ws://ip:port`
//! - `192.168.1.5`, `localhost`, `my-pc.lan` → `ws://…:27960`
//! - a bare lobby code (`<words>` with hyphens, no dots) → `wss://<words>.trycloudflare.com`

use crate::{DEFAULT_PORT, NetError};

const TUNNEL_SUFFIX: &str = ".trycloudflare.com";

/// Resolve user input (address or lobby code) into a `ws://` / `wss://` URL.
pub fn resolve_address(input: &str) -> Result<String, NetError> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        return Err(NetError::BadAddress("empty address".into()));
    }
    if s.chars().any(char::is_whitespace) {
        return Err(NetError::BadAddress(format!("'{s}' contains spaces")));
    }
    let lower = s.to_ascii_lowercase();
    for (scheme, ws) in [("ws://", "ws://"), ("wss://", "wss://"), ("http://", "ws://"), ("https://", "wss://")] {
        if let Some(rest) = lower.strip_prefix(scheme) {
            if rest.is_empty() {
                return Err(NetError::BadAddress(format!("'{s}' has no host")));
            }
            // Preserve the original casing of any path, but hosts are case-insensitive anyway.
            return Ok(format!("{ws}{}", &s[scheme.len()..]));
        }
    }
    if lower.contains("://") {
        return Err(NetError::BadAddress(format!("unsupported scheme in '{s}'")));
    }
    if lower.ends_with(TUNNEL_SUFFIX) {
        return Ok(format!("wss://{lower}"));
    }
    // IPv6 literal with port: [::1]:27960
    if lower.starts_with('[') {
        return if lower.contains("]:") { Ok(format!("ws://{lower}")) } else { Ok(format!("ws://{lower}:{DEFAULT_PORT}")) };
    }
    let colons = lower.matches(':').count();
    if colons == 1 {
        let (host, port) = lower.split_once(':').unwrap();
        if host.is_empty() || port.parse::<u16>().is_err() {
            return Err(NetError::BadAddress(format!("'{s}' is not host:port")));
        }
        return Ok(format!("ws://{lower}"));
    }
    if colons > 1 {
        // Bare IPv6.
        return Ok(format!("ws://[{lower}]:{DEFAULT_PORT}"));
    }
    if !lower.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        return Err(NetError::BadAddress(format!("'{s}' is not an address or lobby code")));
    }
    if !lower.contains('.') && lower.contains('-') {
        // Lobby code: the <words> of https://<words>.trycloudflare.com
        return Ok(format!("wss://{lower}{TUNNEL_SUFFIX}"));
    }
    Ok(format!("ws://{lower}:{DEFAULT_PORT}"))
}

/// Extract the lobby code (`<words>`) from `https://<words>.trycloudflare.com`.
pub fn lobby_code_from_url(url: &str) -> Option<String> {
    let host = url.split("://").nth(1).unwrap_or(url);
    let host = host.split('/').next()?;
    let code = host.strip_suffix(TUNNEL_SUFFIX)?;
    (!code.is_empty() && !code.contains('.')).then(|| code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms() {
        let r = |s| resolve_address(s).unwrap();
        assert_eq!(r("ws://1.2.3.4:5"), "ws://1.2.3.4:5");
        assert_eq!(r("wss://abc-def.trycloudflare.com/"), "wss://abc-def.trycloudflare.com");
        assert_eq!(r("https://abc-def.trycloudflare.com"), "wss://abc-def.trycloudflare.com");
        assert_eq!(r("http://localhost:9000"), "ws://localhost:9000");
        assert_eq!(r("Abc-Def.trycloudflare.com"), "wss://abc-def.trycloudflare.com");
        assert_eq!(r("  loving-mouse-harbor-quiet  "), "wss://loving-mouse-harbor-quiet.trycloudflare.com");
        assert_eq!(r("192.168.1.20:27961"), "ws://192.168.1.20:27961");
        assert_eq!(r("192.168.1.20"), "ws://192.168.1.20:27960");
        assert_eq!(r("localhost"), "ws://localhost:27960");
        assert_eq!(r("my-pc.lan"), "ws://my-pc.lan:27960");
        assert_eq!(r("[::1]:4000"), "ws://[::1]:4000");
        assert_eq!(r("::1"), "ws://[::1]:27960");
        assert!(resolve_address("").is_err());
        assert!(resolve_address("a b").is_err());
        assert!(resolve_address("host:notaport").is_err());
        assert!(resolve_address("ftp://x").is_err());
        assert!(resolve_address("bad_code!").is_err());
    }

    #[test]
    fn codes() {
        assert_eq!(lobby_code_from_url("https://a-b-c.trycloudflare.com").as_deref(), Some("a-b-c"));
        assert_eq!(lobby_code_from_url("https://api.x.trycloudflare.com"), None);
        assert_eq!(lobby_code_from_url("https://example.com"), None);
    }
}
