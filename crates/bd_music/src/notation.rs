//! The song notation: a tiny MML ("Music Macro Language") dialect, one string per channel per
//! section. See the crate README for the full reference. Parsing happens once at load.
//!
//! ```text
//! o4 l8 v12 q7      octave 4 (c = middle C), default length 1/8, volume 12/15, gate 7/8
//! c d e f g a b     notes; + or # sharp, - flat, = natural (explicit accidentals are absolute;
//!                   otherwise the key signature applies)
//! c4 c8. c16 c12    lengths: 1 2 4 8 16 32 64, triplets 3 6 12 24 48, dots
//! c4&c16  c4^16     tie / extend
//! r4                rest
//! > <               octave up / down
//! {ceg}4            chord → chip arpeggio (notes ascend automatically)
//! ~e4               glide (portamento) from the previous note
//! @lead             instrument change;  k-12  transpose (semitones, absolute)
//! [ ... ]3          repeat;  $name  insert a named pattern;  |  bar check;  ; comment
//! ```
//! Drum channels use `k s h o c t m x n` (chip kit: kick, snare, closed hat, open hat, crash, low
//! tom, mid tom, clap, metallic blip) and `K S X G H O C b W R I` (modern kit: synth kick, snare,
//! clap, gated snare, hats, crash, cowbell, noise riser, reverse cymbal, impact) with the same
//! lengths, rests, `v`, `[ ]`, `$`, `|`, plus `p3` (pitch offset in semitones for following hits).
//!
//! Groove: `!` before a note/hit accents it (+3 volume), `?` makes it a ghost (40 % volume).
//!
//! Chord-driven parts (a section with `chords`): `R T F S O N` are the root, third, fifth,
//! seventh (octave if none), octave and ninth of the chord sounding at that moment; `X` / `Y` play
//! the triad / seventh chord as an arpeggio. `R` uses a slash chord's bass. `u4` places roots in the
//! octave starting 4 semitones above the current `o` (keeps bass lines in one register).
//!
//! Chord progressions: `"G | G | D7/F# | D | Em C | %"` — bars split by `|`, up to 4 chords per
//! bar (evenly split), `%` repeats the previous bar; `$name` and `[ ]n` work as in parts.

use std::collections::BTreeMap;

use crate::synth::Drum;

/// Ticks per quarter note.
pub const PPQ: u32 = 96;
/// Ticks per whole note.
pub const WHOLE: u32 = PPQ * 4;

/// A note event in a compiled part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    /// Start tick, relative to the section start.
    pub tick: u32,
    /// Written length in ticks.
    pub len: u32,
    /// Gate in eighths of the written length (1..=8).
    pub q: u8,
    /// MIDI key (tonal) or drum id (drum channel).
    pub key: u8,
    /// Extra chord keys (arpeggiated), 0 = unused.
    pub chord: [u8; 3],
    /// Volume 0..=15.
    pub vol: u8,
    /// Instrument index in the song's table, `u8::MAX` = channel default.
    pub inst: u8,
    /// Slide from the previous pitch.
    pub glide: bool,
    /// Drum pitch offset in semitones (drum channels).
    pub pitch: i8,
}

/// A chord symbol: root / bass pitch classes and the intervals of its tones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub root: i32,
    pub bass: i32,
    pub third: i32,
    pub fifth: i32,
    pub seventh: Option<i32>,
}

/// A chord starting at `tick` (section-relative).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordAt {
    pub tick: u32,
    pub chord: Chord,
}

/// Parse a chord symbol: `G Gm G7 Gmaj7 Gm7 Gdim Gdim7 Gm7b5 Gsus4 Gsus2 Gaug G5 G6 Gm6 G9`,
/// optional slash bass (`D7/F#`); `#`/`b` accidentals. `transpose` shifts it.
pub fn parse_chord(sym: &str, transpose: i32) -> Result<Chord, String> {
    let (main, slash) = match sym.split_once('/') {
        Some((a, b)) => (a, Some(b)),
        None => (sym, None),
    };
    let pc_of = |s: &str| -> Result<(i32, usize), String> {
        let mut it = s.chars();
        let l = it.next().ok_or_else(|| format!("empty chord '{sym}'"))?.to_ascii_lowercase();
        let pc = letter_pc(l).ok_or_else(|| format!("bad chord root in '{sym}'"))?;
        match it.next() {
            Some('#') => Ok((pc + 1, 2)),
            Some('b') => Ok((pc - 1, 2)),
            _ => Ok((pc, 1)),
        }
    };
    let (root, used) = pc_of(main)?;
    let q = &main[used..];
    let (third, fifth, seventh) = match q {
        "" | "maj" => (4, 7, None),
        "m" | "min" => (3, 7, None),
        "7" | "9" => (4, 7, Some(10)),
        "maj7" | "M7" => (4, 7, Some(11)),
        "m7" | "m9" => (3, 7, Some(10)),
        "mmaj7" => (3, 7, Some(11)),
        "dim" => (3, 6, None),
        "dim7" => (3, 6, Some(9)),
        "m7b5" => (3, 6, Some(10)),
        "sus4" => (5, 7, None),
        "sus2" => (2, 7, None),
        "7sus4" => (5, 7, Some(10)),
        "aug" | "+" => (4, 8, None),
        "5" => (7, 12, None),
        "6" => (4, 7, Some(9)),
        "m6" => (3, 7, Some(9)),
        _ => return Err(format!("unknown chord quality '{q}' in '{sym}'")),
    };
    let root = (root + transpose).rem_euclid(12);
    let bass = match slash {
        Some(b) => (pc_of(b)?.0 + transpose).rem_euclid(12),
        None => root,
    };
    Ok(Chord { root, bass, third, fifth, seventh })
}

/// Map notes written in bars of `src_bar` ticks onto bars of `dst_bar` ticks by doubling the first
/// beat (`dst_bar - src_bar` must be one beat): a 3/4 waltz becomes 4/4 (`d4 f#4 a4` → `d2 f#4 a4`),
/// 9/8 becomes 12/8.
pub fn remeter(notes: &mut [Note], src_bar: u32, dst_bar: u32) {
    let extra = dst_bar.saturating_sub(src_bar).max(1);
    let map = |t: u32| {
        let bar = t / src_bar;
        let x = t % src_bar;
        bar * dst_bar + if x < extra { 2 * x } else { x + extra }
    };
    for n in notes {
        let s = map(n.tick);
        let e = map(n.tick + n.len);
        n.tick = s;
        n.len = e.saturating_sub(s).max(1);
    }
}

/// Swing: delay every off-beat `unit` (e.g. every second 16th) by `amount` × `unit`.
pub fn swing(notes: &mut [Note], amount: f32, unit: u32) {
    let d = (amount.clamp(0.0, 0.75) * unit as f32).round() as u32;
    if d == 0 || unit == 0 {
        return;
    }
    for n in notes {
        if n.tick % (2 * unit) == unit {
            n.tick += d;
            n.len = n.len.saturating_sub(d).max(1);
        }
    }
}

/// Parse a chord progression for a section of `bars` bars.
pub fn parse_chords(
    src: &str,
    patterns: &BTreeMap<String, String>,
    bar_ticks: u32,
    bars: u32,
    transpose: i32,
) -> Result<Vec<ChordAt>, String> {
    let text = strip_comments(src);
    let text = expand_macros(&text, patterns, 0)?;
    let text = expand_repeats(&text)?;
    let mut out: Vec<ChordAt> = Vec::new();
    let mut prev: Vec<Chord> = Vec::new();
    let mut bar = 0u32;
    for cell in text.split('|') {
        let toks: Vec<&str> = cell.split_whitespace().collect();
        if toks.is_empty() {
            continue;
        }
        let chords: Vec<Chord> = if toks == ["%"] {
            if prev.is_empty() {
                return Err("'%' with no previous bar".into());
            }
            prev.clone()
        } else {
            toks.iter().map(|t| parse_chord(t, transpose)).collect::<Result<_, _>>()?
        };
        if chords.len() > 4 || !bar_ticks.is_multiple_of(chords.len() as u32) {
            return Err(format!("chord bar {} has {} chords (1, 2 or 4 fit a bar)", bar + 1, chords.len()));
        }
        let step = bar_ticks / chords.len() as u32;
        for (i, c) in chords.iter().enumerate() {
            out.push(ChordAt { tick: bar * bar_ticks + i as u32 * step, chord: *c });
        }
        prev = chords;
        bar += 1;
    }
    if bar != bars {
        return Err(format!("chord progression has {bar} bars, section has {bars}"));
    }
    Ok(out)
}

impl Note {
    pub fn keys(&self) -> ([u8; 4], usize) {
        let mut k = [self.key, 0, 0, 0];
        let mut n = 1;
        for &c in &self.chord {
            if c != 0 {
                if let Some(slot) = k.get_mut(n) {
                    *slot = c;
                }
                n += 1;
            }
        }
        (k, n.min(4))
    }
}

/// Everything the parser needs to know about the context of one part.
pub struct PartCtx<'a> {
    pub patterns: &'a BTreeMap<String, String>,
    /// Accidental applied to c d e f g a b by the key signature.
    pub key_sig: [i8; 7],
    pub drums: bool,
    pub bar_ticks: u32,
    pub transpose: i32,
    pub inst_index: &'a dyn Fn(&str) -> Option<u8>,
    /// The section's chord track (enables `R T F S O N X Y`).
    pub chords: Option<&'a [ChordAt]>,
}

/// Key signature for a key name like `G`, `Bb`, `F#m`, `Dm`. Returns (accidentals, tonic pc, minor).
pub fn key_signature(name: &str) -> Result<([i8; 7], i32, bool), String> {
    let name = name.trim();
    let minor = name.ends_with('m') && name.len() > 1;
    let root = if minor { &name[..name.len() - 1] } else { name };
    let mut chars = root.chars();
    let letter = chars.next().ok_or_else(|| "empty key".to_string())?.to_ascii_lowercase();
    let acc: i32 = match chars.as_str() {
        "" => 0,
        "#" | "+" => 1,
        "b" | "-" => -1,
        other => return Err(format!("bad key accidental '{other}' in key '{name}'")),
    };
    let pc = letter_pc(letter).ok_or_else(|| format!("bad key '{name}'"))? + acc;
    let tonic = pc.rem_euclid(12);
    let spelled = format!(
        "{letter}{}",
        if acc > 0 {
            "#"
        } else if acc < 0 {
            "b"
        } else {
            ""
        }
    );
    const MAJOR: [(&str, i32); 15] = [
        ("c", 0),
        ("g", 1),
        ("d", 2),
        ("a", 3),
        ("e", 4),
        ("b", 5),
        ("f#", 6),
        ("c#", 7),
        ("f", -1),
        ("bb", -2),
        ("eb", -3),
        ("ab", -4),
        ("db", -5),
        ("gb", -6),
        ("cb", -7),
    ];
    const MINOR: [(&str, i32); 15] = [
        ("a", 0),
        ("e", 1),
        ("b", 2),
        ("f#", 3),
        ("c#", 4),
        ("g#", 5),
        ("d#", 6),
        ("a#", 7),
        ("d", -1),
        ("g", -2),
        ("c", -3),
        ("f", -4),
        ("bb", -5),
        ("eb", -6),
        ("ab", -7),
    ];
    let table = if minor { &MINOR } else { &MAJOR };
    let fifths = table.iter().find(|(n, _)| *n == spelled).map(|&(_, f)| f).ok_or_else(|| format!("unsupported key '{name}'"))?;
    let mut sig = [0i8; 7];
    const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6]; // f c g d a e b (index into c d e f g a b)
    const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3]; // b e a d g c f
    if fifths > 0 {
        for &i in SHARP_ORDER.iter().take(fifths as usize) {
            sig[i] = 1;
        }
    } else {
        for &i in FLAT_ORDER.iter().take((-fifths) as usize) {
            sig[i] = -1;
        }
    }
    Ok((sig, tonic, minor))
}

fn letter_pc(c: char) -> Option<i32> {
    Some(match c {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    })
}

fn letter_idx(c: char) -> Option<usize> {
    Some(match c {
        'c' => 0,
        'd' => 1,
        'e' => 2,
        'f' => 3,
        'g' => 4,
        'a' => 5,
        'b' => 6,
        _ => return None,
    })
}

fn strip_comments(src: &str) -> String {
    src.lines().map(|l| l.split(';').next().unwrap_or("")).collect::<Vec<_>>().join(" ")
}

fn expand_macros(src: &str, patterns: &BTreeMap<String, String>, depth: u32) -> Result<String, String> {
    if depth > 8 {
        return Err("pattern macros nested more than 8 deep (recursive $pattern?)".into());
    }
    let mut out = String::with_capacity(src.len());
    let mut it = src.char_indices().peekable();
    while let Some((_, c)) = it.next() {
        if c == '$' {
            let mut name = String::new();
            while let Some(&(_, n)) = it.peek() {
                if n.is_ascii_alphanumeric() || n == '_' {
                    name.push(n);
                    it.next();
                } else {
                    break;
                }
            }
            let body = patterns.get(&name).ok_or_else(|| format!("unknown pattern ${name}"))?;
            let body = strip_comments(body);
            out.push(' ');
            out.push_str(&expand_macros(&body, patterns, depth + 1)?);
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

fn expand_repeats(src: &str) -> Result<String, String> {
    let mut s: Vec<char> = src.chars().collect();
    loop {
        let Some(close) = s.iter().position(|&c| c == ']') else {
            if s.contains(&'[') {
                return Err("unmatched '['".into());
            }
            return Ok(s.into_iter().collect());
        };
        let open = s[..close].iter().rposition(|&c| c == '[').ok_or("unmatched ']'")?;
        let mut end = close + 1;
        let mut digits = String::new();
        while let Some(&d) = s.get(end) {
            if d.is_ascii_digit() {
                digits.push(d);
                end += 1;
            } else {
                break;
            }
        }
        let n: usize = if digits.is_empty() { 2 } else { digits.parse().map_err(|_| "bad repeat count")? };
        if n > 256 {
            return Err(format!("repeat count {n} too large"));
        }
        let body: Vec<char> = s[open + 1..close].to_vec();
        let mut rep = Vec::with_capacity(body.len() * n + 2);
        for _ in 0..n {
            rep.push(' ');
            rep.extend_from_slice(&body);
        }
        rep.push(' ');
        s.splice(open..end, rep);
    }
}

struct Parser<'a> {
    s: Vec<char>,
    i: usize,
    ctx: &'a PartCtx<'a>,
    octave: i32,
    deflen: u32,
    vol: u8,
    q: u8,
    inst: u8,
    transpose: i32,
    tick: u32,
    notes: Vec<Note>,
    /// Index of the last note in `notes` (for ties) — None after a rest.
    last: Option<usize>,
    last_key: Option<u8>,
    glide_next: bool,
    /// Pending `!` (+1) / `?` (-1) for the next note.
    accent: i8,
    /// Drum pitch offset (`p`).
    dpitch: i8,
    /// Root placement offset for chord degrees (`u`).
    uoff: i32,
}

impl Parser<'_> {
    /// The chord sounding at the current tick.
    fn chord_now(&self) -> Result<Chord, String> {
        let chords = self.ctx.chords.ok_or_else(|| self.err("chord degrees need a section with `chords`"))?;
        chords.iter().rev().find(|c| c.tick <= self.tick).or(chords.first()).map(|c| c.chord).ok_or_else(|| self.err("empty chord track"))
    }

    /// Root key of the current chord in the current octave (honouring `u`), using the slash bass
    /// for `bass = true`.
    fn root_key(&self, c: &Chord, bass: bool) -> i32 {
        let pc = if bass { c.bass } else { c.root };
        let base = (self.octave + 1) * 12 + self.uoff + self.transpose;
        base + (pc - base).rem_euclid(12)
    }

    fn degree_key(&self, d: char) -> Result<i32, String> {
        let c = self.chord_now()?;
        let root = self.root_key(&c, false);
        let key = match d {
            'R' => self.root_key(&c, true),
            'T' => root + c.third,
            'F' => root + c.fifth,
            'S' => root + c.seventh.unwrap_or(12),
            'O' => root + 12,
            _ => root + 14,
        };
        if !(12..=120).contains(&key) {
            return Err(self.err(format!("chord degree out of range (midi {key})")));
        }
        Ok(key)
    }

    fn take_vol(&mut self) -> u8 {
        match std::mem::take(&mut self.accent) {
            1 => (self.vol + 3).min(15),
            -1 => ((f32::from(self.vol) * 0.4).round() as u8).max(1),
            _ => self.vol,
        }
    }

    fn err(&self, msg: impl std::fmt::Display) -> String {
        let a = self.i.saturating_sub(24);
        let b = (self.i + 16).min(self.s.len());
        let ctx: String = self.s[a..b].iter().collect();
        let bar = self.tick / self.ctx.bar_ticks.max(1) + 1;
        format!("{msg} (bar {bar}, near '…{}…')", ctx.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn number(&mut self) -> Option<i64> {
        let neg = if self.peek() == Some('-') {
            self.i += 1;
            true
        } else {
            if self.peek() == Some('+') {
                self.i += 1;
            }
            false
        };
        let st = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        if st == self.i {
            return None;
        }
        let v: i64 = self.s[st..self.i].iter().collect::<String>().parse().ok()?;
        Some(if neg { -v } else { v })
    }

    fn unsigned(&mut self) -> Option<u32> {
        let st = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        if st == self.i {
            return None;
        }
        self.s[st..self.i].iter().collect::<String>().parse().ok()
    }

    /// Optional length (+ dots) → ticks.
    fn length(&mut self) -> Result<u32, String> {
        let base = match self.unsigned() {
            Some(0) => return Err(self.err("length 0")),
            Some(n) => {
                if !WHOLE.is_multiple_of(n) {
                    return Err(self.err(format!("length {n} does not divide a whole note")));
                }
                WHOLE / n
            }
            None => self.deflen,
        };
        let mut total = base;
        let mut add = base;
        while self.peek() == Some('.') {
            self.i += 1;
            add /= 2;
            total += add;
        }
        Ok(total)
    }

    fn accidental(&mut self) -> Option<i32> {
        let mut acc = None;
        loop {
            match self.peek() {
                Some('+') | Some('#') => {
                    acc = Some(acc.unwrap_or(0) + 1);
                    self.i += 1;
                }
                Some('-') => {
                    acc = Some(acc.unwrap_or(0) - 1);
                    self.i += 1;
                }
                Some('=') => {
                    acc = Some(acc.unwrap_or(0));
                    self.i += 1;
                }
                _ => return acc,
            }
        }
    }

    fn pitch(&mut self, letter: char, octave: i32) -> Result<i32, String> {
        let pc = letter_pc(letter).ok_or_else(|| self.err("not a note"))?;
        let li = letter_idx(letter).unwrap_or(0);
        let acc = match self.accidental() {
            Some(a) => a,
            None => i32::from(self.ctx.key_sig[li]),
        };
        let key = (octave + 1) * 12 + pc + acc + self.transpose + self.ctx.transpose;
        if !(12..=120).contains(&key) {
            return Err(self.err(format!("note out of range (midi {key})")));
        }
        Ok(key)
    }

    fn push_note(&mut self, key: u8, chord: [u8; 3], len: u32) {
        let glide = std::mem::take(&mut self.glide_next);
        let vol = self.take_vol();
        let pitch = if self.ctx.drums { self.dpitch } else { 0 };
        self.notes.push(Note { tick: self.tick, len, q: self.q, key, chord, vol, inst: self.inst, glide, pitch });
        self.last = Some(self.notes.len() - 1);
        self.last_key = Some(key);
        self.tick += len;
    }

    /// `&` tie / `^` extend after a note or rest.
    fn ties(&mut self, key: Option<u8>) -> Result<(), String> {
        loop {
            self.skip_ws();
            match self.peek() {
                Some('^') => {
                    self.i += 1;
                    let l = self.length()?;
                    self.extend(l);
                }
                Some('&') => {
                    self.i += 1;
                    self.skip_ws();
                    let Some(c) = self.peek() else { return Err(self.err("'&' at end")) };
                    self.i += 1;
                    if self.ctx.drums || c == 'r' {
                        return Err(self.err("'&' must be followed by the same note (use ^len)"));
                    }
                    let k = self.pitch(c, self.octave)?;
                    if Some(k as u8) != key {
                        return Err(self.err("tie to a different pitch (use ~ for slides)"));
                    }
                    let l = self.length()?;
                    self.extend(l);
                }
                _ => return Ok(()),
            }
        }
    }

    fn extend(&mut self, l: u32) {
        if let Some(n) = self.last.and_then(|i| self.notes.get_mut(i)) {
            n.len += l;
        }
        self.tick += l;
    }

    fn run(&mut self) -> Result<(), String> {
        loop {
            self.skip_ws();
            let Some(c) = self.peek() else { return Ok(()) };
            self.i += 1;
            match c {
                '|' => {
                    if !self.tick.is_multiple_of(self.ctx.bar_ticks) {
                        let off = self.tick % self.ctx.bar_ticks;
                        return Err(self.err(format!(
                            "bar check failed: {off} ticks into a bar ({} ticks = 1/{} short/over)",
                            off,
                            WHOLE / off.min(self.ctx.bar_ticks - off).max(1)
                        )));
                    }
                }
                'o' if !self.ctx.drums => {
                    let n = self.unsigned().ok_or_else(|| self.err("o needs a number"))?;
                    if n > 8 {
                        return Err(self.err("octave > 8"));
                    }
                    self.octave = n as i32;
                }
                '>' => self.octave += 1,
                '<' => self.octave -= 1,
                'l' => {
                    let n = self.length()?;
                    self.deflen = n;
                }
                'v' => {
                    let n = self.unsigned().ok_or_else(|| self.err("v needs 0..15"))?;
                    if n > 15 {
                        return Err(self.err("volume > 15"));
                    }
                    self.vol = n as u8;
                }
                'q' => {
                    let n = self.unsigned().ok_or_else(|| self.err("q needs 1..8"))?;
                    if !(1..=8).contains(&n) {
                        return Err(self.err("q must be 1..8"));
                    }
                    self.q = n as u8;
                }
                'k' if !self.ctx.drums => {
                    let n = self.number().ok_or_else(|| self.err("k needs a signed number"))?;
                    self.transpose = n as i32;
                }
                '@' => {
                    let st = self.i;
                    while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                        self.i += 1;
                    }
                    let name: String = self.s[st..self.i].iter().collect();
                    self.inst = (self.ctx.inst_index)(&name).ok_or_else(|| self.err(format!("unknown instrument @{name}")))?;
                }
                '~' => self.glide_next = true,
                '!' => self.accent = 1,
                '?' => self.accent = -1,
                'p' if self.ctx.drums => {
                    let n = self.number().ok_or_else(|| self.err("p needs a signed number"))?;
                    self.dpitch = n.clamp(-48, 48) as i8;
                }
                'u' if !self.ctx.drums => {
                    let n = self.unsigned().ok_or_else(|| self.err("u needs 0..11"))?;
                    self.uoff = (n as i32).clamp(0, 11);
                }
                d @ ('R' | 'T' | 'F' | 'S' | 'O' | 'N') if !self.ctx.drums => {
                    let k = self.degree_key(d)?;
                    let len = self.length()?;
                    self.push_note(k as u8, [0; 3], len);
                    self.ties(None)?;
                }
                d @ ('X' | 'Y') if !self.ctx.drums => {
                    let c = self.chord_now()?;
                    let r = self.root_key(&c, false);
                    let mut keys = [r, r + c.third, r + c.fifth, r + c.seventh.unwrap_or(12)];
                    let n = if d == 'X' { 3 } else { 4 };
                    if keys.iter().take(n).any(|&k| !(12..=120).contains(&k)) {
                        return Err(self.err("chord out of range"));
                    }
                    keys.sort_unstable();
                    let mut chord = [0u8; 3];
                    for (slot, &k) in chord.iter_mut().zip(keys.iter().take(n).skip(1)) {
                        *slot = k as u8;
                    }
                    let len = self.length()?;
                    self.push_note(keys[0] as u8, chord, len);
                    self.ties(None)?;
                }
                'r' => {
                    let l = self.length()?;
                    self.last = None;
                    self.tick += l;
                    // `r4^8` extends the rest
                    while {
                        self.skip_ws();
                        self.peek() == Some('^')
                    } {
                        self.i += 1;
                        self.tick += self.length()?;
                    }
                }
                '{' if !self.ctx.drums => {
                    let mut keys: Vec<i32> = Vec::new();
                    let mut oct = self.octave;
                    loop {
                        self.skip_ws();
                        let Some(n) = self.peek() else { return Err(self.err("unterminated chord")) };
                        self.i += 1;
                        match n {
                            '}' => break,
                            '>' => oct += 1,
                            '<' => oct -= 1,
                            l if letter_pc(l).is_some() => {
                                let mut k = self.pitch(l, oct)?;
                                if let Some(&prev) = keys.last() {
                                    while k <= prev {
                                        k += 12;
                                    }
                                }
                                keys.push(k);
                            }
                            _ => return Err(self.err(format!("'{n}' in chord"))),
                        }
                    }
                    if keys.is_empty() || keys.len() > 4 {
                        return Err(self.err("chords need 1..4 notes"));
                    }
                    if keys.iter().any(|&k| k > 120) {
                        return Err(self.err("chord note out of range"));
                    }
                    let l = self.length()?;
                    let mut chord = [0u8; 3];
                    for (slot, &k) in chord.iter_mut().zip(keys.iter().skip(1)) {
                        *slot = k as u8;
                    }
                    self.push_note(keys[0] as u8, chord, l);
                    self.ties(None)?;
                }
                l if !self.ctx.drums && letter_pc(l).is_some() => {
                    let k = self.pitch(l, self.octave)?;
                    let len = self.length()?;
                    self.push_note(k as u8, [0; 3], len);
                    self.ties(Some(k as u8))?;
                }
                d if self.ctx.drums && Drum::from_letter(d).is_some() => {
                    let id = Drum::from_letter(d).map(|d| d as u8).unwrap_or(0);
                    let len = self.length()?;
                    self.push_note(id, [0; 3], len);
                    self.ties(None)?;
                }
                other => {
                    self.i -= 1;
                    return Err(self.err(format!("unexpected '{other}'")));
                }
            }
        }
    }
}

/// Parse one part. Returns the notes and the total length in ticks.
pub fn parse_part(src: &str, ctx: &PartCtx) -> Result<(Vec<Note>, u32), String> {
    let text = strip_comments(src);
    let text = expand_macros(&text, ctx.patterns, 0)?;
    let text = expand_repeats(&text)?;
    let mut p = Parser {
        s: text.chars().collect(),
        i: 0,
        ctx,
        octave: 4,
        deflen: PPQ,
        vol: 12,
        q: 8,
        inst: u8::MAX,
        transpose: 0,
        tick: 0,
        notes: Vec::new(),
        last: None,
        last_key: None,
        glide_next: false,
        accent: 0,
        dpitch: 0,
        uoff: 0,
    };
    p.run()?;
    let _ = p.last_key;
    Ok((p.notes, p.tick))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(pats: &'a BTreeMap<String, String>, key: &str, drums: bool) -> PartCtx<'a> {
        let (key_sig, _, _) = key_signature(key).unwrap();
        PartCtx { patterns: pats, key_sig, drums, bar_ticks: WHOLE, transpose: 0, inst_index: &|_| Some(0), chords: None }
    }

    #[test]
    fn key_signatures() {
        assert_eq!(key_signature("G").unwrap().0, [0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(key_signature("Em").unwrap().0, [0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(key_signature("Bb").unwrap().0, [0, 0, -1, 0, 0, 0, -1]);
        assert_eq!(key_signature("Bm").unwrap().0, [1, 0, 0, 1, 0, 0, 0]);
        assert_eq!(key_signature("E").unwrap().0, [1, 1, 0, 1, 1, 0, 0]);
        assert_eq!(key_signature("Gm").unwrap().0, [0, 0, -1, 0, 0, 0, -1]);
        assert_eq!(key_signature("Am").unwrap().1, 9);
    }

    #[test]
    fn basic_notes_and_bars() {
        let p = BTreeMap::new();
        let (n, len) = parse_part("o4 l8 g4 r8 d g4 r8 d | g d g b > d4 r4 |", &ctx(&p, "G", false)).unwrap();
        assert_eq!(len, 2 * WHOLE);
        assert_eq!(n[0].key, 67);
        assert_eq!(n[1].key, 62);
        assert_eq!(n[1].tick, 144);
        assert_eq!(n.last().unwrap().key, 74);
    }

    #[test]
    fn key_sig_and_accidentals() {
        let p = BTreeMap::new();
        let (n, _) = parse_part("f f= f- f+", &ctx(&p, "G", false)).unwrap();
        assert_eq!(n.iter().map(|n| n.key).collect::<Vec<_>>(), vec![66, 65, 64, 66]);
    }

    #[test]
    fn repeats_macros_ties_chords() {
        let mut p = BTreeMap::new();
        p.insert("x".to_string(), "c8 d8".to_string());
        let (n, len) = parse_part("[$x]2 e4&e8 {ceg}8 | ", &ctx(&p, "C", false)).unwrap();
        assert_eq!(len, WHOLE);
        assert_eq!(n.len(), 6);
        assert_eq!(n[4].len, 144);
        assert_eq!(n[5].chord, [64, 67, 0]);
    }

    #[test]
    fn bar_check_catches_errors() {
        let p = BTreeMap::new();
        let e = parse_part("c4 d4 e4 |", &ctx(&p, "C", false)).unwrap_err();
        assert!(e.contains("bar check"), "{e}");
    }

    #[test]
    fn drums() {
        let p = BTreeMap::new();
        let (n, len) = parse_part("[k8 h8 s8 h8]2 |", &ctx(&p, "C", true)).unwrap();
        assert_eq!(len, WHOLE);
        assert_eq!(n[0].key, Drum::Kick as u8);
        assert_eq!(n[2].key, Drum::Snare as u8);
    }
}
