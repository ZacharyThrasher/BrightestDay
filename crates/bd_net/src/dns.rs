//! Minimal direct DNS (A record over UDP) used for brand-new `*.trycloudflare.com` hostnames.
//!
//! A quick tunnel's hostname only exists from the moment the host starts it. If a friend's OS
//! resolver asks too early it caches the NXDOMAIN (Windows keeps negative answers for minutes),
//! and every retry fails with "No such host is known". Asking public resolvers directly bypasses
//! that cache; the OS resolver remains the fallback (networks that block outbound DNS).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;

const SERVERS: [&str; 3] = ["1.1.1.1:53", "1.0.0.1:53", "8.8.8.8:53"];

/// Result of a direct lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DnsAnswer {
    Addrs(Vec<IpAddr>),
    /// A resolver answered, but the name does not exist (yet).
    NoSuchName,
    /// No public resolver answered (outbound DNS blocked?) — use the OS resolver.
    NoResponse,
}

/// Resolve `host` to IPv4 addresses by querying public resolvers directly.
pub async fn resolve_direct(host: &str) -> DnsAnswer {
    let id = (crate::now_secs().to_bits() as u16) ^ 0x5A5A;
    let Some(query) = build_query(id, host) else { return DnsAnswer::NoResponse };
    let mut answered = false;
    for server in SERVERS {
        let server: SocketAddr = server.parse().unwrap();
        let Ok(sock) = UdpSocket::bind("0.0.0.0:0").await else { continue };
        if sock.send_to(&query, server).await.is_err() {
            continue;
        }
        let mut buf = [0u8; 1500];
        if let Ok(Ok((n, from))) = tokio::time::timeout(Duration::from_millis(800), sock.recv_from(&mut buf)).await {
            if from == server {
                if let Some(ips) = parse_a_records(id, &buf[..n]) {
                    if !ips.is_empty() {
                        return DnsAnswer::Addrs(ips);
                    }
                    answered = true;
                }
            }
        }
    }
    if answered { DnsAnswer::NoSuchName } else { DnsAnswer::NoResponse }
}

fn build_query(id: u16, host: &str) -> Option<Vec<u8>> {
    let mut q = Vec::with_capacity(32 + host.len());
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00]); // recursion desired
    q.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // 1 question
    for label in host.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    q.extend_from_slice(&[0, 1, 0, 1]); // type A, class IN
    Some(q)
}

fn skip_name(b: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *b.get(i)? as usize;
        if len & 0xC0 == 0xC0 {
            return Some(i + 2);
        }
        if len == 0 {
            return Some(i + 1);
        }
        i += 1 + len;
    }
}

/// `None` for a malformed / foreign packet, otherwise the A records (empty for NXDOMAIN).
fn parse_a_records(id: u16, b: &[u8]) -> Option<Vec<IpAddr>> {
    let mut out = Vec::new();
    if b.len() < 12 || u16::from_be_bytes([b[0], b[1]]) != id || b[2] & 0x80 == 0 {
        return None;
    }
    if b[3] & 0x0F != 0 {
        return Some(out);
    }
    let qd = u16::from_be_bytes([b[4], b[5]]) as usize;
    let an = u16::from_be_bytes([b[6], b[7]]) as usize;
    let mut i = 12;
    for _ in 0..qd {
        let Some(j) = skip_name(b, i) else { return Some(out) };
        i = j + 4;
    }
    for _ in 0..an {
        let Some(j) = skip_name(b, i) else { return Some(out) };
        if j + 10 > b.len() {
            return Some(out);
        }
        let ty = u16::from_be_bytes([b[j], b[j + 1]]);
        let len = u16::from_be_bytes([b[j + 8], b[j + 9]]) as usize;
        let data = j + 10;
        if data + len > b.len() {
            return Some(out);
        }
        if ty == 1 && len == 4 {
            out.push(IpAddr::V4(Ipv4Addr::new(b[data], b[data + 1], b[data + 2], b[data + 3])));
        }
        i = data + len;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_parse() {
        let q = build_query(0x1234, "a-b.trycloudflare.com").unwrap();
        assert_eq!(&q[12..16], &[3, b'a', b'-', b'b']);
        // Response: header + question + CNAME answer (pointer name) + A answer.
        let mut r = q.clone();
        r[2] = 0x81;
        r[3] = 0x80;
        r[7] = 2; // ancount
        r.extend_from_slice(&[0xC0, 12, 0, 5, 0, 1, 0, 0, 0, 60, 0, 2, 0xC0, 12]); // CNAME
        r.extend_from_slice(&[0xC0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 104, 16, 230, 132]); // A
        assert_eq!(parse_a_records(0x1234, &r), Some(vec![IpAddr::V4(Ipv4Addr::new(104, 16, 230, 132))]));
        assert_eq!(parse_a_records(0x9999, &r), None);
        r[3] = 0x83; // NXDOMAIN
        assert_eq!(parse_a_records(0x1234, &r), Some(vec![]));
    }
}
