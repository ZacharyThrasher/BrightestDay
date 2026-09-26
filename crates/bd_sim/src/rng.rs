//! Tiny deterministic RNG (SplitMix64) and a stable string hash. The sim never touches a global or
//! OS-seeded RNG: every random decision derives from the world seed and the tick, so host replays,
//! tests and bots are bit-reproducible.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimRng {
    state: u64,
}

impl SimRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed ^ 0x9E37_79B9_7F4A_7C15 }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in [0, 1).
    #[inline]
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / (1u64 << 24) as f32)
    }

    /// Uniform in [lo, hi).
    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform integer in [0, n). `n = 0` returns 0.
    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { (self.next_u64() % n as u64) as u32 }
    }

    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
}

/// FNV-1a over the lowercase bytes of `s` (stable across platforms and runs). Used for the
/// "same phrase" staleness check and deterministic per-word seeds.
pub fn word_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b.to_ascii_lowercase() as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// Incremental FNV-1a hasher over raw bytes (event hashes in tests / desync checks).
#[derive(Clone, Copy, Debug)]
pub struct Fnv64(pub u64);

impl Default for Fnv64 {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv64 {
    pub fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    pub fn write_u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }
    pub fn write_f32(&mut self, v: f32) {
        self.write(&v.to_bits().to_le_bytes());
    }
}
