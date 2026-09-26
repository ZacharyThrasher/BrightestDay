//! In-process TCP proxy that adds one-way latency and jitter (order-preserving, like a real TCP
//! path). Used by the tests; the game can also expose it as a "simulate lag" developer option:
//! host normally, start `LagProxy` in front of the host port, and join through `proxy.url()`.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::transport::runtime;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LagSettings {
    /// Added one-way delay in each direction (ms). RTT grows by twice this.
    pub latency_ms: f32,
    /// Extra uniformly random delay 0..jitter_ms per chunk (ms). Order is preserved.
    pub jitter_ms: f32,
}

pub struct LagProxy {
    addr: SocketAddr,
    settings: Arc<Mutex<LagSettings>>,
    shutdown: watch::Sender<bool>,
}

impl LagProxy {
    /// Listen on `127.0.0.1:0` and forward every connection to `target`.
    pub fn start(target: SocketAddr, settings: LagSettings) -> std::io::Result<Self> {
        let std_l = std::net::TcpListener::bind("127.0.0.1:0")?;
        std_l.set_nonblocking(true)?;
        let addr = std_l.local_addr()?;
        let rt = runtime();
        let listener = {
            let _g = rt.enter();
            TcpListener::from_std(std_l)?
        };
        let settings = Arc::new(Mutex::new(settings));
        let (sd_tx, sd_rx) = watch::channel(false);
        rt.spawn(accept(listener, target, settings.clone(), sd_rx));
        Ok(Self { addr, settings, shutdown: sd_tx })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `ws://127.0.0.1:PORT` to hand to `ClientSession::connect`.
    pub fn url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    /// Change latency/jitter on the fly.
    pub fn set(&self, s: LagSettings) {
        *self.settings.lock().unwrap() = s;
    }
}

impl Drop for LagProxy {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

async fn accept(listener: TcpListener, target: SocketAddr, settings: Arc<Mutex<LagSettings>>, mut sd: watch::Receiver<bool>) {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    loop {
        tokio::select! {
            _ = sd.changed() => return,
            r = listener.accept() => {
                let Ok((inbound, _)) = r else { continue };
                let Ok(outbound) = TcpStream::connect(target).await else { continue };
                let _ = inbound.set_nodelay(true);
                let _ = outbound.set_nodelay(true);
                let (ir, iw) = inbound.into_split();
                let (or, ow) = outbound.into_split();
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                tokio::spawn(pump(ir, ow, settings.clone(), seed ^ 1, sd.clone()));
                tokio::spawn(pump(or, iw, settings.clone(), seed ^ 2, sd.clone()));
            }
        }
    }
}

async fn pump(
    mut from: tokio::net::tcp::OwnedReadHalf,
    mut to: tokio::net::tcp::OwnedWriteHalf,
    settings: Arc<Mutex<LagSettings>>,
    mut rng: u64,
    mut sd: watch::Receiver<bool>,
) {
    let (tx, mut rx) = mpsc::unbounded_channel::<(Instant, Bytes)>();
    let writer = tokio::spawn(async move {
        while let Some((at, data)) = rx.recv().await {
            tokio::time::sleep_until(at).await;
            if to.write_all(&data).await.is_err() {
                break;
            }
        }
        let _ = to.shutdown().await;
    });
    let mut buf = vec![0u8; 64 * 1024];
    let mut last = Instant::now();
    loop {
        let n = tokio::select! {
            _ = sd.changed() => break,
            r = from.read(&mut buf) => match r { Ok(0) | Err(_) => break, Ok(n) => n },
        };
        let s = *settings.lock().unwrap();
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let r = (rng >> 11) as f64 / (1u64 << 53) as f64;
        let delay = (s.latency_ms as f64 + r * s.jitter_ms as f64).max(0.0);
        let at = (Instant::now() + Duration::from_secs_f64(delay / 1000.0)).max(last);
        last = at;
        if tx.send((at, Bytes::copy_from_slice(&buf[..n]))).is_err() {
            break;
        }
    }
    drop(tx);
    let _ = writer.await;
}
