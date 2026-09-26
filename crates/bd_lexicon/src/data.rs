//! `assets/lexicon.bin` container: a small section table over one byte buffer. Everything big is read
//! in place (zero-copy): fst maps for keys/tokens, fixed-size little-endian entry records, a string pool.
//!
//! ```text
//! "BDLX" u32 version  u32 n_sections  n × { [u8;4] tag, u64 offset, u64 len }  ...section bytes...
//! META  bincode(serde) Meta — modifier tables, grammar data (small)
//! KEYS  fst::Map  normalised key -> entry id
//! RECS  n × 64-byte records (see `Rec`)
//! POOL  UTF-8 string pool (display names, explanations, token strings)
//! TOKS  fst::Map  token -> token id          TOKO  n × (u32 off, u16 len, u8 rank, u8 pad) into POOL
//! RTOK  fst::Map  reversed token -> token id (portmanteau suffix lookup: "sharknado" -> tornado)
//! DELS  sorted (u32 hash(delete-1 of token), u32 token id) — distance-2 typo candidates
//! PHON  sorted (u32 hash(phonetic key of entry key), u32 entry id) — sound-alike lookup
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub const MAGIC: &[u8; 4] = b"BDLX";
pub const VERSION: u32 = 4;
pub const REC_SIZE: usize = 72;
pub const TOKO_SIZE: usize = 8;
pub const NONE8: u8 = 0xFF;

// entry flags (mirror tools/lexicon_build/build.py)
pub const F_PART: u16 = 1;
pub const F_POKEMON: u16 = 2;
pub const F_ENTITY: u16 = 4;
pub const F_POP: u16 = 8;
pub const F_ANCHOR: u16 = 16;
pub const F_PURE: u16 = 32;
#[allow(dead_code)]
pub const F_CN: u16 = 64;
#[allow(dead_code)]
pub const F_PERSON: u16 = 128;
pub const F_SPIRIT: u16 = 256;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AdjRec {
    pub k: String,
    pub element: u8,
    pub weak_element: u8,
    pub mech: u64,
    pub feat: u64,
    pub scale: f32,
    pub mass: f32,
    /// Absolute size in metres ("planet sized"); 0 = none.
    pub abs: f32,
    pub temper: u8,
    pub conf: f32,
    pub why: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VerbRec {
    pub k: String,
    pub lemma: String,
    pub mech: u64,
    pub element: u8,
    /// Payload verb: (mechanic index the host gains, mechanic index added to the payload or NONE8).
    pub payload: Option<(u8, u8)>,
    pub conf: f32,
    pub why: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Meta {
    pub generated: String,
    pub n_entries: u32,
    pub adjectives: Vec<AdjRec>,
    pub verbs: Vec<VerbRec>,
    /// (word, count, implies Swarm)
    pub collectives: Vec<(String, u8, bool)>,
    pub intensifiers: Vec<(String, f32)>,
    /// (inflected, base)
    pub inflections: Vec<(String, String)>,
    /// Slur / hate-speech tokens; a leading '*' means "also match inside longer names".
    pub blocked: Vec<String>,
    pub bias: Vec<String>,
    pub payload_hosts: Vec<String>,
    /// (word, element index, mechanic index or NONE8)
    pub pure: Vec<(String, u8, u8)>,
    /// Top completions for 1- and 2-character prefixes: (prefix, entry ids by rank).
    pub top_prefix: Vec<(String, Vec<u32>)>,
}

/// The lexicon bytes: an owned buffer, or a read-only memory map of lexicon.bin (the default for
/// `Lexicon::load`: the OS pages sections in on demand, so loading doesn't read 85 MB up front).
pub enum Buf {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl std::ops::Deref for Buf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Buf::Owned(v) => v,
            Buf::Mapped(m) => m,
        }
    }
}

/// Owned byte range usable as fst backing storage without copying.
#[derive(Clone)]
pub struct Bytes {
    buf: Arc<Buf>,
    start: usize,
    end: usize,
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.buf.get(self.start..self.end).unwrap_or(&[])
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rec {
    pub key: (u32, u16),
    pub display: (u32, u16),
    pub form: u8,
    pub element: u8,
    pub mech: u64,
    pub graft: u64,
    pub feat: u64,
    pub scale: f32,
    pub mass: f32,
    pub why1: (u32, u16),
    pub why2: (u32, u16),
    pub count: u8,
    pub conf: u8,
    pub rank: u8,
    pub graft_element: u8,
    pub flags: u16,
    pub temper: u8,
    pub accent: Option<[u8; 3]>,
}

#[inline]
fn rd<const N: usize>(b: &[u8], off: usize) -> [u8; N] {
    let mut out = [0u8; N];
    if let Some(s) = b.get(off..off + N) {
        out.copy_from_slice(s);
    }
    out
}
#[inline]
pub fn rd_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(rd::<2>(b, off))
}
#[inline]
pub fn rd_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(rd::<4>(b, off))
}
#[inline]
pub fn rd_u64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(rd::<8>(b, off))
}
#[inline]
pub fn rd_f32(b: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(rd::<4>(b, off))
}

impl Rec {
    #[cfg_attr(not(feature = "builder"), allow(dead_code))]
    pub fn encode(&self) -> [u8; REC_SIZE] {
        let mut b = [0u8; REC_SIZE];
        b[0..4].copy_from_slice(&self.display.0.to_le_bytes());
        b[4..6].copy_from_slice(&self.display.1.to_le_bytes());
        b[6] = self.form;
        b[7] = self.element;
        b[8..16].copy_from_slice(&self.mech.to_le_bytes());
        b[16..24].copy_from_slice(&self.graft.to_le_bytes());
        b[24..32].copy_from_slice(&self.feat.to_le_bytes());
        b[32..36].copy_from_slice(&self.scale.to_le_bytes());
        b[36..40].copy_from_slice(&self.mass.to_le_bytes());
        b[40..44].copy_from_slice(&self.why1.0.to_le_bytes());
        b[44..46].copy_from_slice(&self.why1.1.to_le_bytes());
        b[46..48].copy_from_slice(&self.why2.1.to_le_bytes());
        b[48..52].copy_from_slice(&self.why2.0.to_le_bytes());
        b[52] = self.count;
        b[53] = self.conf;
        b[54] = self.rank;
        b[55] = self.graft_element;
        b[56..58].copy_from_slice(&self.flags.to_le_bytes());
        b[58] = self.temper;
        if let Some(a) = self.accent {
            b[59] = 1;
            b[60..63].copy_from_slice(&a);
        }
        b[64..68].copy_from_slice(&self.key.0.to_le_bytes());
        b[68..70].copy_from_slice(&self.key.1.to_le_bytes());
        b
    }

    pub fn decode(b: &[u8]) -> Rec {
        Rec {
            key: (rd_u32(b, 64), rd_u16(b, 68)),
            display: (rd_u32(b, 0), rd_u16(b, 4)),
            form: b.get(6).copied().unwrap_or(NONE8),
            element: b.get(7).copied().unwrap_or(0),
            mech: rd_u64(b, 8),
            graft: rd_u64(b, 16),
            feat: rd_u64(b, 24),
            scale: rd_f32(b, 32),
            mass: rd_f32(b, 36),
            why1: (rd_u32(b, 40), rd_u16(b, 44)),
            why2: (rd_u32(b, 48), rd_u16(b, 46)),
            count: b.get(52).copied().unwrap_or(1),
            conf: b.get(53).copied().unwrap_or(0),
            rank: b.get(54).copied().unwrap_or(0),
            graft_element: b.get(55).copied().unwrap_or(NONE8),
            flags: rd_u16(b, 56),
            temper: b.get(58).copied().unwrap_or(NONE8),
            accent: if b.get(59).copied().unwrap_or(0) == 1 { Some(rd::<3>(b, 60)) } else { None },
        }
    }

    pub fn conf_f(&self) -> f32 {
        self.conf as f32 / 255.0
    }
    pub fn rank_f(&self) -> f32 {
        self.rank as f32 / 25.0
    }
}

/// Parsed container. Construction validates the header and every section bound; accessors never panic.
pub struct LexData {
    pub buf: Arc<Buf>,
    pub meta: Meta,
    pub keys: fst::Map<Bytes>,
    pub toks: fst::Map<Bytes>,
    pub rtok: fst::Map<Bytes>,
    recs: (usize, usize),
    pool: (usize, usize),
    toko: (usize, usize),
    dels: (usize, usize),
    phon: (usize, usize),
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Format(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "lexicon io error: {e}"),
            LoadError::Format(s) => write!(f, "lexicon format error: {s}"),
        }
    }
}
impl std::error::Error for LoadError {}
impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        LoadError::Io(e)
    }
}

fn fmt_err<T>(s: impl Into<String>) -> Result<T, LoadError> {
    Err(LoadError::Format(s.into()))
}

impl LexData {
    pub fn parse(bytes: Vec<u8>) -> Result<LexData, LoadError> {
        Self::parse_buf(Buf::Owned(bytes))
    }

    /// Memory-map a lexicon.bin file (falls back to reading it if mapping fails).
    pub fn map_file(path: &std::path::Path) -> Result<LexData, LoadError> {
        let file = std::fs::File::open(path)?;
        // SAFETY: the map is read-only and every access is bounds-checked; lexicon.bin is a build
        // artefact nobody writes while the game runs (a concurrent rewrite could at worst yield
        // garbage bytes, which the parser treats like any corrupt data).
        match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => Self::parse_buf(Buf::Mapped(m)),
            Err(_) => Self::parse(std::fs::read(path)?),
        }
    }

    fn parse_buf(bytes: Buf) -> Result<LexData, LoadError> {
        if bytes.len() < 12 || &bytes[0..4] != MAGIC {
            return fmt_err("bad magic (not a lexicon.bin)");
        }
        let ver = rd_u32(&bytes, 4);
        if ver != VERSION {
            return fmt_err(format!("version {ver}, expected {VERSION} (rebuild with tools/lexicon_build)"));
        }
        let n = rd_u32(&bytes, 8) as usize;
        if n > 64 || 12 + n * 20 > bytes.len() {
            return fmt_err("corrupt section table");
        }
        let mut secs = std::collections::HashMap::new();
        for i in 0..n {
            let o = 12 + i * 20;
            let tag: [u8; 4] = rd::<4>(&bytes, o);
            let off = rd_u64(&bytes, o + 4) as usize;
            let len = rd_u64(&bytes, o + 12) as usize;
            let end = off.saturating_add(len);
            if end > bytes.len() {
                return fmt_err(format!("section {} out of bounds", String::from_utf8_lossy(&tag)));
            }
            secs.insert(tag, (off, end));
        }
        let get = |t: &[u8; 4]| -> Result<(usize, usize), LoadError> {
            secs.get(t).copied().ok_or_else(|| LoadError::Format(format!("missing section {}", String::from_utf8_lossy(t))))
        };
        let buf = Arc::new(bytes);
        let (ms, me) = get(b"META")?;
        let meta: Meta = match bincode::serde::decode_from_slice(&buf[ms..me], bincode::config::standard()) {
            Ok((m, _)) => m,
            Err(e) => return fmt_err(format!("META: {e}")),
        };
        let mk = |t: &[u8; 4]| -> Result<fst::Map<Bytes>, LoadError> {
            let (s, e) = get(t)?;
            fst::Map::new(Bytes { buf: buf.clone(), start: s, end: e })
                .map_err(|e| LoadError::Format(format!("{}: {e}", String::from_utf8_lossy(t))))
        };
        let keys = mk(b"KEYS")?;
        let toks = mk(b"TOKS")?;
        let rtok = mk(b"RTOK")?;
        let recs = get(b"RECS")?;
        if (recs.1 - recs.0) % REC_SIZE != 0 {
            return fmt_err("RECS size");
        }
        let n_recs = (recs.1 - recs.0) / REC_SIZE;
        if keys.len() != n_recs || meta.n_entries as usize != n_recs {
            return fmt_err("KEYS/RECS count mismatch");
        }
        let toko = get(b"TOKO")?;
        if (toko.1 - toko.0) / TOKO_SIZE != toks.len() {
            return fmt_err("TOKO/TOKS count mismatch");
        }
        Ok(LexData { meta, keys, toks, rtok, recs, pool: get(b"POOL")?, toko, dels: get(b"DELS")?, phon: get(b"PHON")?, buf })
    }

    pub fn n_entries(&self) -> usize {
        (self.recs.1 - self.recs.0) / REC_SIZE
    }

    pub fn rec(&self, id: u32) -> Option<Rec> {
        let o = self.recs.0 + id as usize * REC_SIZE;
        if o + REC_SIZE > self.recs.1 {
            return None;
        }
        self.buf.get(o..o + REC_SIZE).map(Rec::decode)
    }

    pub fn pool_str(&self, (off, len): (u32, u16)) -> &str {
        let s = self.pool.0 + off as usize;
        let e = s + len as usize;
        if len == 0 || e > self.pool.1 {
            return "";
        }
        self.buf.get(s..e).and_then(|b| std::str::from_utf8(b).ok()).unwrap_or("")
    }

    /// Explanation string of a record: key-less templates ("\u{1}≈ mule (Wikidata)") get the key back.
    pub fn why_str(&self, r: (u32, u16), key: &str) -> String {
        let s = self.pool_str(r);
        match s.strip_prefix('\u{1}') {
            Some(rest) => format!("{key} {rest}"),
            None => s.to_string(),
        }
    }

    pub fn entry_id(&self, key: &str) -> Option<u32> {
        let id = self.keys.get(key)? as usize;
        (id < self.n_entries()).then_some(id as u32)
    }

    pub fn token_id(&self, tok: &str) -> Option<u32> {
        let id = self.toks.get(tok)? as usize;
        (id < self.toks.len()).then_some(id as u32)
    }

    /// (token string, rank 0..255)
    pub fn token(&self, id: u32) -> (&str, u8) {
        let o = self.toko.0 + id as usize * TOKO_SIZE;
        if o + TOKO_SIZE > self.toko.1 {
            return ("", 0);
        }
        let b = &self.buf[..];
        let off = rd_u32(b, o);
        let len = rd_u16(b, o + 4);
        let rank = b.get(o + 6).copied().unwrap_or(0);
        (self.pool_str((off, len)), rank)
    }

    fn pairs(&self, sec: (usize, usize)) -> &[u8] {
        self.buf.get(sec.0..sec.1).unwrap_or(&[])
    }

    /// All values stored under `hash` in a sorted (u32 hash, u32 value) section.
    fn lookup_pairs(&self, sec: (usize, usize), hash: u32, out: &mut Vec<u32>, limit: usize) {
        let start_len = out.len();
        let b = self.pairs(sec);
        let n = b.len() / 8;
        let (mut lo, mut hi) = (0usize, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if rd_u32(b, mid * 8) < hash {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let mut i = lo;
        while i < n && rd_u32(b, i * 8) == hash {
            out.push(rd_u32(b, i * 8 + 4));
            i += 1;
            if out.len() - start_len >= limit {
                break;
            }
        }
    }

    pub fn dels_lookup(&self, hash: u32, out: &mut Vec<u32>) {
        self.lookup_pairs(self.dels, hash, out, 12)
    }
    pub fn phon_lookup(&self, hash: u32, out: &mut Vec<u32>) {
        self.lookup_pairs(self.phon, hash, out, 24)
    }
}

/// FNV-1a 32-bit (stable across platforms/runs; used for the sorted hash indices).
pub fn fnv32(s: &[u8]) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for &b in s {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

/// FNV-1a 64-bit (stable; used for signatures and procedural seeds).
pub fn fnv64(s: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
