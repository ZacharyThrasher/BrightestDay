//! Offline packer: `tools/lexicon_build/cache/lexicon.json` -> `assets/lexicon.bin` (feature "builder").

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use crate::data::{self, AdjRec, Meta, NONE8, REC_SIZE, Rec, TOKO_SIZE, VerbRec, fnv32};
use crate::parse::GRAMMAR_WORDS;
use crate::phonetic::phonetic_key;
use crate::tables::*;

#[derive(Deserialize)]
struct JEntry {
    k: String,
    #[serde(default)]
    d: Option<String>,
    f: String,
    #[serde(default)]
    m: Vec<String>,
    #[serde(default)]
    e: String,
    s: f32,
    w: f32,
    #[serde(default)]
    n: u32,
    #[serde(default)]
    a: Option<[f32; 3]>,
    c: f32,
    r: f32,
    #[serde(default)]
    g: Vec<String>,
    #[serde(default)]
    ge: Option<String>,
    #[serde(default)]
    ft: Vec<String>,
    #[serde(default)]
    fl: u32,
    #[serde(default)]
    x: Vec<String>,
    #[serde(default)]
    t: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct JAdj {
    k: String,
    e: Option<String>,
    m: Vec<String>,
    s: f32,
    w: f32,
    f: Vec<String>,
    c: f32,
    x: Vec<String>,
    t: Option<String>,
    we: Option<String>,
    abs: Option<f32>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct JVerb {
    k: String,
    lemma: String,
    m: Vec<String>,
    e: Option<String>,
    p: Option<Vec<Option<String>>>,
    c: f32,
    x: String,
}

#[derive(Deserialize)]
struct JColl {
    k: String,
    n: u32,
    swarm: bool,
}

#[derive(Deserialize)]
struct JLex {
    #[serde(default)]
    generated: String,
    entries: Vec<JEntry>,
    #[serde(default)]
    adjectives: Vec<JAdj>,
    #[serde(default)]
    verbs: Vec<JVerb>,
    #[serde(default)]
    collectives: Vec<JColl>,
    #[serde(default)]
    inflections: Vec<(String, String)>,
    #[serde(default)]
    bias: Vec<String>,
    #[serde(default)]
    payload_hosts: Vec<String>,
    #[serde(default)]
    pure: Vec<(String, String, Option<String>)>,
    #[serde(default)]
    intensifiers: Vec<(String, f32)>,
    #[serde(default)]
    blocked: Vec<String>,
}

fn mech_bits(names: &[String], warn: &mut Vec<String>) -> u64 {
    let mut b = 0u64;
    for n in names {
        let n = n.trim_start_matches('-');
        match mechanic_from_name(n) {
            Some(m) => b |= 1u64 << mechanic_index(m),
            None => warn.push(format!("unknown mechanic {n}")),
        }
    }
    b
}

fn feat_bits(names: &[String], warn: &mut Vec<String>) -> u64 {
    let mut b = 0u64;
    for n in names {
        match feature_from_name(n) {
            Some(f) => b |= 1u64 << feature_index(f),
            None => warn.push(format!("unknown feature {n}")),
        }
    }
    b
}

fn elem_idx(n: Option<&str>) -> u8 {
    n.and_then(element_from_name)
        .map(element_index)
        .unwrap_or(NONE8)
}

struct Pool {
    bytes: Vec<u8>,
    interned: HashMap<String, (u32, u16)>,
}

impl Pool {
    fn add(&mut self, s: &str) -> (u32, u16) {
        if s.is_empty() {
            return (0, 0);
        }
        let mut s = s;
        if s.len() > 400 {
            let mut end = 400;
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            s = &s[..end];
        }
        if let Some(&r) = self.interned.get(s) {
            return r;
        }
        let r = (self.bytes.len() as u32, s.len() as u16);
        self.bytes.extend_from_slice(s.as_bytes());
        self.interned.insert(s.to_string(), r);
        r
    }
}

fn fst_err(e: fst::Error) -> String {
    format!("fst: {e}")
}

/// Pack the pipeline JSON into the runtime binary. Returns (bytes, human summary).
pub fn pack(json: &str) -> Result<(Vec<u8>, String), String> {
    let lex: JLex = serde_json::from_str(json).map_err(|e| format!("json: {e}"))?;
    let mut warn = Vec::new();
    let mut entries = lex.entries;
    entries.sort_by(|a, b| a.k.as_bytes().cmp(b.k.as_bytes()));
    entries.dedup_by(|a, b| a.k == b.k);
    entries.retain(|e| !e.k.is_empty() && e.k.len() < 200);

    let mut pool = Pool {
        bytes: Vec::new(),
        interned: HashMap::new(),
    };
    let mut recs = Vec::with_capacity(entries.len() * REC_SIZE);
    let mut keys = fst::MapBuilder::memory();
    let mut tok_rank: BTreeMap<String, f32> = BTreeMap::new();
    let mut phon: Vec<(u32, u32)> = Vec::new();
    for (id, e) in entries.iter().enumerate() {
        let form = form_from_name(&e.f).map(form_index).unwrap_or_else(|| {
            warn.push(format!("unknown form {}", e.f));
            form_index(bd_core::mechanics::Form::Blob0)
        });
        let prefix = format!("{} ", e.k);
        let mut strip = |s: &String| -> (u32, u16) {
            match s.strip_prefix(prefix.as_str()) {
                Some(rest) => pool.add(&format!("\u{1}{rest}")),
                None => pool.add(s),
            }
        };
        let why1 = e.x.first().map(&mut strip).unwrap_or((0, 0));
        let why2 = e.x.get(1).map(&mut strip).unwrap_or((0, 0));
        let key_ref = pool.add(&e.k);
        let rec = Rec {
            key: key_ref,
            display: e.d.as_deref().map(|s| pool.add(s)).unwrap_or((0, 0)),
            form,
            element: elem_idx(Some(e.e.as_str()))
                .min(element_index(bd_core::mechanics::Element::Atomic)),
            mech: mech_bits(&e.m, &mut warn),
            graft: mech_bits(&e.g, &mut warn),
            feat: feat_bits(&e.ft, &mut warn),
            scale: e.s,
            mass: e.w,
            why1,
            why2,
            count: e.n.clamp(1, 255) as u8,
            conf: (e.c.clamp(0.0, 1.0) * 255.0).round() as u8,
            rank: (e.r.max(0.0) * 25.0).min(255.0) as u8,
            graft_element: elem_idx(e.ge.as_deref()),
            flags: e.fl.min(u16::MAX as u32) as u16,
            temper: e
                .t
                .as_deref()
                .and_then(temperament_from_name)
                .map(temperament_index)
                .unwrap_or(NONE8),
            accent: e
                .a
                .map(|a| [0, 1, 2].map(|i| (a[i].clamp(0.0, 1.0) * 255.0).round() as u8)),
        };
        recs.extend_from_slice(&rec.encode());
        keys.insert(e.k.as_bytes(), id as u64).map_err(fst_err)?;
        for t in e.k.split(' ') {
            let r = tok_rank.entry(t.to_string()).or_insert(0.0);
            *r = r.max(e.r);
        }
        let popular = e.fl & (data::F_POP | data::F_POKEMON | data::F_ANCHOR) as u32 != 0;
        if (e.r >= 3.0 || (popular && e.r >= 2.0)) && e.k.len() >= 3 {
            let pk = phonetic_key(&e.k);
            if pk.len() >= 2 {
                phon.push((fnv32(pk.as_bytes()), id as u32));
            }
        }
    }
    let keys = keys.into_inner().map_err(fst_err)?;

    // modifier tables
    let adjectives: Vec<AdjRec> = lex
        .adjectives
        .iter()
        .map(|a| AdjRec {
            k: a.k.clone(),
            element: elem_idx(a.e.as_deref()),
            weak_element: elem_idx(a.we.as_deref()),
            mech: mech_bits(&a.m, &mut warn),
            feat: feat_bits(&a.f, &mut warn),
            scale: if a.s > 0.0 { a.s } else { 1.0 },
            mass: if a.w > 0.0 { a.w } else { 1.0 },
            abs: a.abs.unwrap_or(0.0),
            temper: a
                .t
                .as_deref()
                .and_then(temperament_from_name)
                .map(temperament_index)
                .unwrap_or(NONE8),
            conf: if a.c > 0.0 { a.c } else { 1.0 },
            why: a.x.first().cloned().unwrap_or_default(),
        })
        .collect();
    let verbs: Vec<VerbRec> = lex
        .verbs
        .iter()
        .map(|v| {
            let payload = v.p.as_ref().and_then(|p| {
                let host = p
                    .first()?
                    .as_deref()
                    .and_then(mechanic_from_name)
                    .map(mechanic_index)?;
                let pm = p
                    .get(1)
                    .and_then(|x| x.as_deref())
                    .and_then(mechanic_from_name)
                    .map(mechanic_index);
                Some((host, pm.unwrap_or(NONE8)))
            });
            VerbRec {
                k: v.k.clone(),
                lemma: v.lemma.clone(),
                mech: mech_bits(&v.m, &mut warn),
                element: elem_idx(v.e.as_deref()),
                payload,
                conf: if v.c > 0.0 { v.c } else { 1.0 },
                why: v.x.clone(),
            }
        })
        .collect();
    for w in adjectives
        .iter()
        .map(|a| &a.k)
        .chain(verbs.iter().map(|v| &v.k))
    {
        for t in w.split(' ') {
            let r = tok_rank.entry(t.to_string()).or_insert(0.0);
            *r = r.max(5.0);
        }
    }
    let extra = lex
        .intensifiers
        .iter()
        .map(|x| x.0.clone())
        .chain(lex.collectives.iter().map(|c| c.k.clone()))
        .chain(GRAMMAR_WORDS.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    for w in extra {
        for t in w.split(' ') {
            let r = tok_rank.entry(t.to_string()).or_insert(0.0);
            *r = r.max(6.0);
        }
    }
    let blocked_tokens: Vec<String> = lex
        .blocked
        .iter()
        .map(|b| b.trim_start_matches('*').to_string())
        .collect();
    for b in &blocked_tokens {
        tok_rank.remove(b);
    }
    tok_rank.retain(|t, _| !t.is_empty() && t.len() <= 40);

    // tokens
    let mut toks = fst::MapBuilder::memory();
    let mut toko = Vec::with_capacity(tok_rank.len() * TOKO_SIZE);
    let mut rev: Vec<(String, u32)> = Vec::new();
    let mut dels: Vec<(u32, u32)> = Vec::new();
    for (tid, (t, r)) in tok_rank.iter().enumerate() {
        toks.insert(t.as_bytes(), tid as u64).map_err(fst_err)?;
        let (off, len) = pool.add(t);
        toko.extend_from_slice(&off.to_le_bytes());
        toko.extend_from_slice(&len.to_le_bytes());
        toko.push((r * 25.0).clamp(0.0, 255.0) as u8);
        toko.push(0);
        if t.len() >= 4 && *r >= 2.5 && t.bytes().all(|b| b.is_ascii_lowercase()) {
            rev.push((t.chars().rev().collect(), tid as u32));
        }
        if t.len() >= 6 && *r >= 2.0 {
            for d in crate::fuzzy::deletes1(t) {
                dels.push((fnv32(d.as_bytes()), tid as u32));
            }
        }
    }
    let toks = toks.into_inner().map_err(fst_err)?;
    rev.sort();
    rev.dedup_by(|a, b| a.0 == b.0);
    let mut rtok = fst::MapBuilder::memory();
    for (r, id) in &rev {
        rtok.insert(r.as_bytes(), *id as u64).map_err(fst_err)?;
    }
    let rtok = rtok.into_inner().map_err(fst_err)?;
    dels.sort_unstable();
    dels.dedup();
    phon.sort_unstable();
    phon.dedup();

    // top completions for 1-2 char prefixes
    let mut top: BTreeMap<String, Vec<(u8, u32)>> = BTreeMap::new();
    for (id, e) in entries.iter().enumerate() {
        let rank = (e.r.max(0.0) * 25.0).min(255.0) as u8;
        for plen in 1..=2 {
            if e.k.len() < plen || !e.k.is_char_boundary(plen) {
                continue;
            }
            let v = top.entry(e.k[..plen].to_string()).or_default();
            v.push((rank, id as u32));
            if v.len() > 64 {
                v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                v.truncate(16);
            }
        }
    }
    let top_prefix = top
        .into_iter()
        .map(|(p, mut v)| {
            v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            v.truncate(16);
            (p, v.into_iter().map(|x| x.1).collect())
        })
        .collect();

    let pure = lex
        .pure
        .iter()
        .map(|(k, e, m)| {
            (
                k.clone(),
                elem_idx(Some(e.as_str())),
                m.as_deref()
                    .and_then(mechanic_from_name)
                    .map(mechanic_index)
                    .unwrap_or(NONE8),
            )
        })
        .collect();
    let meta = Meta {
        generated: lex.generated.clone(),
        n_entries: entries.len() as u32,
        adjectives,
        verbs,
        collectives: lex
            .collectives
            .iter()
            .map(|c| (c.k.clone(), c.n.clamp(1, 255) as u8, c.swarm))
            .collect(),
        intensifiers: lex.intensifiers.clone(),
        inflections: lex.inflections.clone(),
        blocked: lex.blocked.clone(),
        bias: lex.bias.clone(),
        payload_hosts: lex.payload_hosts.clone(),
        pure,
        top_prefix,
    };
    let meta_bytes = bincode::serde::encode_to_vec(&meta, bincode::config::standard())
        .map_err(|e| format!("meta: {e}"))?;

    let pairs = |v: &[(u32, u32)]| -> Vec<u8> {
        let mut b = Vec::with_capacity(v.len() * 8);
        for (h, x) in v {
            b.extend_from_slice(&h.to_le_bytes());
            b.extend_from_slice(&x.to_le_bytes());
        }
        b
    };
    let sections: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"META", meta_bytes),
        (b"KEYS", keys),
        (b"RECS", recs),
        (b"POOL", pool.bytes),
        (b"TOKS", toks),
        (b"TOKO", toko),
        (b"RTOK", rtok),
        (b"DELS", pairs(&dels)),
        (b"PHON", pairs(&phon)),
    ];
    let mut out = Vec::new();
    out.extend_from_slice(data::MAGIC);
    out.extend_from_slice(&data::VERSION.to_le_bytes());
    out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
    let mut off = 12 + sections.len() * 20;
    for (tag, b) in &sections {
        out.extend_from_slice(*tag);
        out.extend_from_slice(&(off as u64).to_le_bytes());
        out.extend_from_slice(&(b.len() as u64).to_le_bytes());
        off += b.len();
    }
    let mut summary = format!(
        "{} entries, {} tokens, {} adjectives, {} verb forms, {} phonetic keys, {} typo deletes",
        entries.len(),
        tok_rank.len(),
        meta.adjectives.len(),
        meta.verbs.len(),
        phon.len(),
        dels.len()
    );
    for (tag, b) in sections {
        summary.push_str(&format!(
            "\n  {} {:.1} MB",
            String::from_utf8_lossy(tag),
            b.len() as f64 / 1e6
        ));
        out.extend_from_slice(&b);
    }
    warn.sort();
    warn.dedup();
    for w in warn.iter().take(20) {
        summary.push_str(&format!("\n  WARN {w}"));
    }
    Ok((out, summary))
}
