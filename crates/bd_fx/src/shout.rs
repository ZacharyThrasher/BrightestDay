//! Shout callouts: the words a player typed or yelled burst out of their ring as big hard-light voxel
//! letters, slam into place one by one, hold, then shatter — and their voxels stream into the
//! construct as it forms (the words literally become the thing). North star: "I want them excitedly
//! yelling out anything they can think of"; this is the payoff for yelling.
//!
//! # Integration
//! Spawn an entity with a [`Shout`] (the game fills it from the phrase + mic level) and keep writing
//! `anchor` / `head` / `toward` / `target` / `boost` while it lives; bd_fx lays the letters out, animates
//! them and despawns the entity when the letters have shattered. [`ShoutSfx`] fires at the audible
//! moments (erupt, headline slam, release) for the audio layer.
//!
//! * **Local** shouts (the viewer is the caster) live in the lower-right of the view (the ring hand),
//!   drawn on top of the world, never over the crosshair; at release they are flung toward the
//!   construct and stream into it.
//! * **Compact** local shouts ([`Shout::compact`]: power-move / form titles like "SHINRA TENSEI" for
//!   their caster): smaller (≤ 6 % of the screen height per letter), further right and lower, a short
//!   hold — still punchy, never over the aim. Everyone else still sees them big over the caster.
//! * **World** shouts (someone else) float camera-facing above the caster, distance-scaled so they
//!   stay readable at range but never fill the screen up close; far away (LOD) only the headline line
//!   is shown.
//! * Dynamics: [`Shout::energy`] (from [`typed_energy`] / [`voice_energy`]) × [`Shout::boost`] (from
//!   [`size_boost`]) = intensity → size, letter punch, word shake, hold time, shockwave density.
//! * [`ShoutSettings::gentle`] (comfort): gentler pop, no birth flashes, no word shake.
//! * Budget: at most [`ShoutSettings::max_live`] shouts (the oldest collapses early); letters reuse
//!   the onomatopoeia glyph meshes and the construct slot allocator; no per-frame allocation.

use std::collections::HashMap;

use bevy::camera::visibility::VisibilitySystems;
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use serde::Deserialize;

use crate::construct::{ConstructRuntime, ConstructVisual, FxAnchor, FxSlot, SlotAlloc, construct_gpu, random_voxel, write_slot};
use crate::effects::{ShatterScratch, shatter_model};
use crate::font::SPACE_ADVANCE;
use crate::gpu::FxUpload;
use crate::material::FxMaterials;
use crate::onomatopoeia::GlyphCache;
use crate::palette::Palette;
use crate::particles::{Emit, EmitSystems, FxParticles, PFlags, PKind};
use crate::util::{FxRng, fin, fin3, v3};
use crate::{FxSet, FxSettings};

// ------------------------------------------------------------------------------------ API

/// Longest shout (chars) shown or replicated; longer phrases are cut at a word boundary.
pub const SHOUT_MAX_CHARS: usize = 48;

/// A live shout. Spawn it; bd_fx adds the letters and despawns the entity when done.
#[derive(Component, Clone, Debug)]
#[require(Transform, Visibility)]
pub struct Shout {
    /// Sanitised phrase ([`sanitize_shout`]); shown upper-case.
    pub text: String,
    /// Caster corps colour (linear RGB).
    pub color: [f32; 3],
    /// How hard they shouted ([`typed_energy`] / [`voice_energy`]): ~0.35 calm … 1 loud … 2.2 yell.
    pub energy: f32,
    /// Construct size multiplier ([`size_boost`], ≥ 1). May rise while the shout lives.
    pub boost: f32,
    /// The viewer is the caster: screen-anchored lower-right, flung toward the construct at release.
    pub local: bool,
    /// World position of the caster's ring hand (letters erupt from here).
    pub anchor: Vec3,
    /// World position of the caster's head (world shouts float above it).
    pub head: Vec3,
    /// Where the construct forms (letters are flung / shatter toward it when there is no target).
    pub toward: Vec3,
    /// The construct visual (a [`ConstructVisual`] entity): at release the letters' voxels stream
    /// into its voxels.
    pub target: Option<Entity>,
    /// Hook (summon assembly): stream the letter voxels to this world point instead of into
    /// `target` (e.g. the start of a hard-light beam that snaps them on as bricks). Wins over `target`.
    pub stream_to: Option<Vec3>,
    /// The cast failed: the letters crumble instead of streaming.
    pub fizzle: bool,
    /// Cosmetic seed (letter jitter).
    pub seed: u64,
    /// Local only: a title for its caster (power moves, forms) — smaller, off the aim, short hold.
    pub compact: bool,
}

impl Shout {
    pub fn new(text: String, color: [f32; 3], energy: f32, local: bool, anchor: Vec3) -> Self {
        Shout {
            text,
            color,
            energy,
            boost: 1.0,
            local,
            anchor,
            head: anchor,
            toward: anchor,
            target: None,
            stream_to: None,
            fizzle: false,
            seed: 0,
            compact: false,
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct ShoutSettings {
    /// Comfort: gentler pop, no birth flashes, no word shake.
    pub gentle: bool,
    /// Live shouts cap (the oldest non-local shout collapses early).
    pub max_live: usize,
    /// Stream cubes per shout at release (scaled by `FxSettings::particle_budget`).
    pub stream_cap: usize,
    /// Other players' shouts draw on top of the world (else the construct they announce, which
    /// forms between you and the caster, hides them). Trade-off: a shout reads through walls for
    /// its ~1 s life (the yell is audible anyway). `false` = depth-tested.
    pub world_on_top: bool,
}

impl Default for ShoutSettings {
    fn default() -> Self {
        ShoutSettings { gentle: false, max_live: 8, stream_cap: 700, world_on_top: true }
    }
}

/// Audible shout moments (hook for the audio layer: whoosh on erupt, thump on slam, rush on
/// release). `intensity` ≈ energy × boost.
#[derive(Message, Clone, Copy, Debug)]
pub struct ShoutSfx {
    pub phase: ShoutPhase,
    pub pos: Vec3,
    pub intensity: f32,
    pub local: bool,
    pub entity: Entity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShoutPhase {
    /// Letters leave the ring.
    Erupt,
    /// The headline's last letter landed (shockwave).
    Slam,
    /// Letters shattered; `streamed` = their voxels flow into the construct.
    Release { streamed: bool },
    /// The cast failed: letters crumble.
    Fizzle,
}

/// Shout keywords (`assets/fx/onomatopoeia.ron`, `shout: (..)`), data only.
#[derive(Resource, Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ShoutTable {
    /// Whole words (lower-case) that pump a shout up, with their energy bonus.
    pub amplifiers: HashMap<String, f32>,
    /// Attack-name joiners: "<kicker> no <headline>" shows the kicker small above a huge headline.
    pub joiners: Vec<String>,
    /// Mic level (0..1 ≙ −60..−6 dBFS) where talking turns into yelling / a full yell.
    pub voice_talk: f32,
    pub voice_yell: f32,
}

impl Default for ShoutTable {
    fn default() -> Self {
        ShoutTable { amplifiers: HashMap::new(), joiners: vec!["no".into()], voice_talk: 0.55, voice_yell: 0.92 }
    }
}

#[derive(Deserialize)]
struct ShoutFile {
    #[serde(default)]
    shout: ShoutTable,
}

const DEFAULT_TABLE: &str = include_str!("../../../assets/fx/onomatopoeia.ron");

impl ShoutTable {
    pub fn parse(src: &str) -> Result<Self, String> {
        let f: ShoutFile = ron::from_str(src).map_err(|e| e.to_string())?;
        Ok(f.shout)
    }
    fn amplifier(&self, word: &str) -> f32 {
        let mut buf = [0u8; 24];
        let mut n = 0;
        for b in word.bytes().filter(u8::is_ascii_alphabetic) {
            if n == buf.len() {
                return 0.0;
            }
            buf[n] = b.to_ascii_lowercase();
            n += 1;
        }
        let key = std::str::from_utf8(&buf[..n]).unwrap_or("");
        self.amplifiers.get(key).copied().filter(|v| v.is_finite()).unwrap_or(0.0).clamp(0.0, 1.0)
    }
    fn is_joiner(&self, word: &str) -> bool {
        self.joiners.iter().any(|j| j.eq_ignore_ascii_case(word))
    }
}

// ------------------------------------------------------------------------------------ pure logic

fn font_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '!' | '?' | '-' | '\'' | '.')
}

/// Untrusted phrase → what a shout may show: font characters only (A–Z a–z 0–9 `! ? - ' .`), case
/// kept (it carries energy), whitespace collapsed, runs of one character capped at 6 ("HAMMERRRRRRR"
/// → "HAMMERRRRRR"), cut to `max_chars` at a word boundary. Never panics, output is ASCII.
pub fn sanitize_shout(raw: &str, max_chars: usize) -> String {
    let max_chars = max_chars.clamp(1, 256);
    let mut out = String::with_capacity(max_chars + 8);
    let mut gap = false;
    let mut run = (' ', 0usize);
    for c in raw.chars().take(1024) {
        if font_char(c) {
            if gap && !out.is_empty() {
                out.push(' ');
                run = (' ', 0);
            }
            gap = false;
            let k = c.to_ascii_lowercase();
            run = if run.0 == k { (k, run.1 + 1) } else { (k, 1) };
            if run.1 > 6 {
                continue;
            }
            out.push(c);
            if out.len() > max_chars + 1 {
                break;
            }
        } else if c.is_whitespace() || matches!(c, ',' | '_' | ';' | ':' | '/' | '~' | '*') {
            gap = true;
        }
    }
    if out.len() > max_chars {
        let cut = out[..=max_chars].rfind(' ').filter(|&i| i >= max_chars / 2).unwrap_or(max_chars);
        out.truncate(cut);
    }
    out.truncate(out.trim_end().len());
    out
}

/// Energy of a typed phrase: lower-case is calm (~0.35); ALL CAPS, trailing "!!!", stretched letters
/// ("HAMMERRRR") and amplifier words ("SUPER", "MEGA", table) pump it up to ~2.2.
pub fn typed_energy(text: &str, table: &ShoutTable) -> f32 {
    let (mut letters, mut upper, mut bangs, mut stretch) = (0usize, 0usize, 0usize, 0usize);
    let mut run = (' ', 0usize);
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            letters += 1;
            upper += c.is_ascii_uppercase() as usize;
            let k = c.to_ascii_lowercase();
            run = if run.0 == k { (k, run.1 + 1) } else { (k, 1) };
            if run.1 >= 3 {
                stretch += 1;
            }
        } else {
            run = (' ', 0);
            bangs += (c == '!') as usize;
        }
    }
    if letters == 0 {
        return 0.4 + 0.16 * bangs.min(5) as f32;
    }
    let caps = upper as f32 / letters as f32;
    // "Hammer" (one capital) stays calm; mostly-caps counts fully.
    let caps_k = ((caps - 0.3) / 0.5).clamp(0.0, 1.0);
    let amp: f32 = text.split(' ').map(|w| table.amplifier(w)).sum::<f32>().min(0.8);
    (0.35 + 0.6 * caps_k + 0.16 * bangs.min(5) as f32 + 0.06 * stretch.min(8) as f32 + amp).clamp(0.25, 2.3)
}

/// Energy of a spoken phrase from the utterance's peak mic level (0..1; talking ≈ 0.55–0.7, a yell
/// ≈ 0.9–1): a real yell is ENORMOUS. Amplifier words and "!" in the transcript add a little.
pub fn voice_energy(level: f32, text: &str, table: &ShoutTable) -> f32 {
    let (talk, yell) = (fin(table.voice_talk, 0.55).clamp(0.0, 0.95), fin(table.voice_yell, 0.92).clamp(0.05, 1.0));
    let loud = ((fin(level, 0.0) - talk) / (yell - talk).max(0.05)).clamp(0.0, 1.0);
    let amp: f32 = text.split(' ').map(|w| table.amplifier(w)).sum::<f32>().min(0.6);
    let bangs = text.chars().filter(|c| *c == '!').count().min(2) as f32;
    (0.45 + 1.75 * loud.powf(1.3) + amp + 0.1 * bangs).clamp(0.3, 2.6)
}

/// Construct size (m, largest extent) → shout multiplier: hand-sized 1, a house ~1.5, kaiju/no-limits
/// (a MOON) 2.2.
pub fn size_boost(size_m: f32) -> f32 {
    let s = fin(size_m, 1.0).clamp(0.01, 1.0e6);
    (1.0 + 0.3 * (s / 2.0).log2()).clamp(1.0, 2.2)
}

/// Combined intensity (drives every visual knob).
pub fn shout_intensity(energy: f32, boost: f32) -> f32 {
    (fin(energy, 0.5).clamp(0.2, 2.6) * fin(boost, 1.0).clamp(1.0, 2.2)).clamp(0.2, 4.0)
}

/// One laid-out line: byte range into the text + relative size (1 = headline).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShoutLine {
    pub start: usize,
    pub end: usize,
    pub scale: f32,
}

/// At most two lines, top first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShoutLayout {
    pub lines: [ShoutLine; 2],
    pub n: usize,
    /// Split at an attack-name joiner ("GOMU GOMU NO" / "PISTOL").
    pub joiner: bool,
    /// Amplifier words or a joiner: an extra flourish (second shockwave, shine sweep).
    pub flourish: bool,
}

impl ShoutLayout {
    pub fn lines(&self) -> &[ShoutLine] {
        &self.lines[..self.n]
    }
}

/// Characters that fit on one line before wrapping.
const ONE_LINE: usize = 10;
/// Kicker (joiner line / quieter line) size.
const KICKER: f32 = 0.62;

/// Lay a sanitised phrase out: "<kicker> no <headline>" → small kicker over a big headline; short →
/// one line; longer → two balanced lines, the shoutier one big and the other smaller (equal when
/// they're equally loud). Never more than two lines.
pub fn layout_shout(text: &str, table: &ShoutTable) -> ShoutLayout {
    let z = ShoutLine { start: 0, end: 0, scale: 1.0 };
    let one = |flourish| ShoutLayout { lines: [ShoutLine { start: 0, end: text.len(), scale: 1.0 }, z], n: 1, joiner: false, flourish };
    let amplified = text.split(' ').any(|w| table.amplifier(w) > 0.0);
    // Word byte ranges (bounded: sanitised text is short; anything longer only uses the first 32).
    let mut words = [(0usize, 0usize); 32];
    let mut nw = 0;
    let mut i = 0;
    for w in text.split(' ') {
        if !w.is_empty() && nw < words.len() {
            words[nw] = (i, i + w.len());
            nw += 1;
        }
        i += w.len() + 1;
    }
    if nw == 0 {
        return one(false);
    }
    // Attack-name joiner with words on both sides.
    for j in 1..nw.saturating_sub(1) {
        let (a, b) = words[j];
        if table.is_joiner(&text[a..b]) {
            let kick = ShoutLine { start: 0, end: b, scale: KICKER };
            let head = ShoutLine { start: words[j + 1].0, end: words[nw - 1].1, scale: 1.0 };
            return ShoutLayout { lines: [kick, head], n: 2, joiner: true, flourish: true };
        }
    }
    if text.len() <= ONE_LINE || nw == 1 {
        return one(amplified);
    }
    // Balanced split at a word boundary.
    let mut best = (usize::MAX, 1);
    for k in 1..nw {
        let a = words[k - 1].1;
        let b = text.len() - words[k].0;
        let d = a.abs_diff(b);
        if d < best.0 {
            best = (d, k);
        }
    }
    let k = best.1;
    let (l0, l1) = ((0, words[k - 1].1), (words[k].0, words[nw - 1].1));
    let (e0, e1) = (typed_energy(&text[l0.0..l0.1], table), typed_energy(&text[l1.0..l1.1], table));
    let (s0, s1) = if (e0 - e1).abs() < 0.15 {
        (1.0, 1.0)
    } else if e0 > e1 {
        (1.0, KICKER + 0.08)
    } else {
        (KICKER + 0.08, 1.0)
    };
    ShoutLayout {
        lines: [ShoutLine { start: l0.0, end: l0.1, scale: s0 }, ShoutLine { start: l1.0, end: l1.1, scale: s1 }],
        n: 2,
        joiner: false,
        flourish: amplified,
    }
}

// ------------------------------------------------------------------------------------ internals

/// Glyph cell height in voxels (7 rows) and the line gap.
const GLYPH_H: f32 = 7.0 * bd_core::CONSTRUCT_VOXEL_SIZE;
const LINE_GAP: f32 = 0.45;
/// Local shouts are drawn this far in front of the camera (overlay material: always on top).
const LOCAL_DEPTH: f32 = 3.0;
/// Letter flight from the ring hand to its slot.
const FLY: f32 = 0.11;
/// Slam punch settle time.
const SLAM: f32 = 0.24;
/// Local shouts: flung toward the construct for this long before they shatter (crosses the centre
/// of the screen for well under 0.25 s).
const THROW: f32 = 0.14;
/// Summon hook: letter voxels need this long to reach the ring beam and ride it to a brick (s);
/// stages landing sooner than this after release are skipped.
const BRICK_LEAD: f32 = 0.16;
/// Summon hook: while the target is assembling, the words hold at most this long after the last
/// letter lands, then become its bricks (s).
const BRICK_HOLD: f32 = 0.3;
/// Summon hook: first leg (letter voxel → ring beam), seconds.
const BRICK_TO_RING: f32 = 0.1;

struct SLetter {
    entity: Entity,
    ch: char,
    slot: u32,
    line: u8,
    /// Letter centre in shout units (incl. the mesh centring offset × line scale).
    home: Vec3,
    scale: f32,
    delay: f32,
    rot: f32,
    spin: f32,
}

#[derive(Component)]
struct ShoutState {
    letters: Vec<SLetter>,
    age: f32,
    /// Time the last letter lands.
    land: f32,
    hold: f32,
    /// Local: throw started at this age.
    throw_at: Option<f32>,
    released: bool,
    slammed: bool,
    boost: f32,
    /// Block size in shout units (width, height) and the lines' vertical centres.
    block: Vec2,
    line_y: [f32; 2],
    line_scale: [f32; 2],
    lines: usize,
    flourish: bool,
    /// Hide the smaller line (far LOD).
    far: bool,
    serial: u64,
}

#[derive(Resource, Default)]
struct ShoutRuntime {
    serial: u64,
    scratch: ShatterScratch,
}

pub fn build(app: &mut App) {
    let table = ShoutTable::parse(DEFAULT_TABLE).unwrap_or_else(|e| {
        warn!("bd_fx: onomatopoeia.ron shout table: {e}");
        ShoutTable::default()
    });
    app.insert_resource(table)
        .init_resource::<ShoutSettings>()
        .init_resource::<ShoutRuntime>()
        .add_message::<ShoutSfx>()
        .add_systems(PostUpdate, init_shouts.in_set(FxSet::Lifecycle))
        .add_systems(PostUpdate, animate_shouts.in_set(FxSet::Pose))
        .add_systems(PostUpdate, finish_shouts.in_set(EmitSystems).before(VisibilitySystems::CheckVisibility));
}

fn camera(q: &Query<(&Camera, &GlobalTransform, &Projection), With<Camera3d>>) -> Option<(GlobalTransform, f32, f32)> {
    // Full-window cameras only (inset viewports, e.g. the summon preview, never own the words).
    q.iter().filter(|(c, _, _)| c.is_active && c.viewport.is_none()).max_by_key(|(c, _, _)| c.order).map(|(_, g, p)| match p {
        Projection::Perspective(p) => (*g, fin(p.fov, 1.2).clamp(0.2, 3.0), fin(p.aspect_ratio, 1.78).clamp(0.3, 5.0)),
        _ => (*g, 1.2, 1.78),
    })
}

fn init_shouts(
    mut commands: Commands,
    new: Query<(Entity, &Shout), Without<ShoutState>>,
    mut live: Query<(&Shout, &mut ShoutState)>,
    settings: Res<ShoutSettings>,
    table: Res<ShoutTable>,
    glyphs: Res<GlyphCache>,
    mats: Res<FxMaterials>,
    mut alloc: ResMut<SlotAlloc>,
    mut rt: ResMut<ShoutRuntime>,
    mut sfx: MessageWriter<ShoutSfx>,
) {
    for (added, (e, s)) in new.iter().enumerate() {
        // Cap: the oldest live shout (non-local first) collapses now.
        let n_live = live.iter().filter(|(_, st)| !st.released && st.throw_at.is_none() && st.age <= st.land + st.hold).count() + added;
        if n_live >= settings.max_live.max(1) {
            let oldest = live
                .iter_mut()
                .filter(|(sh, st)| !st.released && st.throw_at.is_none() && (!sh.local || s.local))
                .max_by(|a, b| a.1.age.total_cmp(&b.1.age));
            if let Some((_, mut st)) = oldest {
                st.hold = st.hold.min((st.age - st.land).max(0.0));
            }
        }
        let text: String = s.text.chars().take(SHOUT_MAX_CHARS * 2).map(|c| c.to_ascii_uppercase()).collect();
        let lay = layout_shout(&text, &table);
        let mut rng = FxRng::new(s.seed ^ (rt.serial << 24) ^ 0x5EED);
        rt.serial += 1;
        let intensity = shout_intensity(s.energy, s.boost);
        let space = SPACE_ADVANCE * bd_core::CONSTRUCT_VOXEL_SIZE;
        // Line widths (units, before line scale).
        let mut widths = [0.0f32; 2];
        for (li, l) in lay.lines().iter().enumerate() {
            let mut x = 0.0;
            for c in text.get(l.start..l.end).unwrap_or("").chars() {
                if c == ' ' {
                    x += space;
                } else if let Some(g) = glyphs.glyphs.get(&c) {
                    x += g.advance;
                }
            }
            widths[li] = x;
        }
        let n = lay.n;
        let heights: [f32; 2] = [lay.lines[0].scale * GLYPH_H, if n > 1 { lay.lines[1].scale * GLYPH_H } else { 0.0 }];
        let gap = if n > 1 { LINE_GAP * GLYPH_H } else { 0.0 };
        let block_h = heights[0] + heights[1] + gap;
        let block_w = (widths[0] * lay.lines[0].scale).max(if n > 1 { widths[1] * lay.lines[1].scale } else { 0.0 });
        let line_y = [block_h * 0.5 - heights[0] * 0.5, -block_h * 0.5 + heights[1] * 0.5];
        let total = text.len().max(1) as f32;
        let stagger = (0.34 / total).min(0.045);
        let jit = if settings.gentle { 0.02 } else { 0.04 + 0.03 * intensity.min(2.0) };
        let mut letters = Vec::with_capacity(text.len());
        let mat = if s.local || settings.world_on_top { mats.construct_overlay.clone() } else { mats.construct(false) };
        let mut order = 0usize;
        for (li, l) in lay.lines().iter().enumerate() {
            let ls = l.scale;
            let mut x = -widths[li] * ls * 0.5;
            for c in text.get(l.start..l.end).unwrap_or("").chars() {
                if c == ' ' {
                    x += space * ls;
                    order += 1;
                    continue;
                }
                let Some(g) = glyphs.glyphs.get(&c) else { continue };
                let slot = alloc.slot();
                let home = Vec3::new(x, line_y[li] - GLYPH_H * ls * 0.5, 0.0) + g.offset * ls;
                x += g.advance * ls;
                let ent = commands
                    .spawn((
                        Mesh3d(g.mesh.clone()),
                        MeshMaterial3d(mat.clone()),
                        MeshTag(slot),
                        FxSlot(slot),
                        Transform::from_scale(Vec3::ZERO),
                        NotShadowCaster,
                        ChildOf(e),
                    ))
                    .id();
                letters.push(SLetter {
                    entity: ent,
                    ch: c,
                    slot,
                    line: li as u8,
                    home,
                    scale: ls,
                    delay: order as f32 * stagger,
                    rot: rng.range(-jit, jit),
                    spin: if rng.f() < 0.5 { -1.0 } else { 1.0 } * rng.range(1.5, 3.5),
                });
                order += 1;
            }
        }
        let land = letters.last().map_or(0.0, |l| l.delay) + FLY;
        let hold = (0.38 + 0.3 * intensity.min(2.5) + if lay.flourish { 0.15 } else { 0.0 }).min(1.25);
        let hold = if s.local && s.compact { hold.min(COMPACT_HOLD) } else { hold };
        commands.entity(e).insert(ShoutState {
            letters,
            age: 0.0,
            land,
            hold,
            throw_at: None,
            released: false,
            slammed: false,
            boost: fin(s.boost, 1.0).clamp(1.0, 2.2),
            block: Vec2::new(block_w, block_h),
            line_y,
            line_scale: [lay.lines[0].scale, lay.lines[1].scale],
            lines: n,
            flourish: lay.flourish,
            far: false,
            serial: rt.serial,
        });
        sfx.write(ShoutSfx { phase: ShoutPhase::Erupt, pos: fin3(s.anchor), intensity, local: s.local, entity: e });
    }
}

/// Comic-book contrast: bright letters, a near-black drop extrusion, white-hot core — readable on
/// any background (a green shout on green grass).
fn shout_palette(color: [f32; 3]) -> Palette {
    let c = Palette::new(color, None).0[0];
    let mix = |t: f32| [c[0] + (1.0 - c[0]) * t, c[1] + (1.0 - c[1]) * t, c[2] + (1.0 - c[2]) * t];
    Palette([mix(0.15), mix(0.6), [c[0] * 0.05, c[1] * 0.05, c[2] * 0.05], mix(0.85)])
}

fn ease_out3(u: f32) -> f32 {
    let u = 1.0 - u.clamp(0.0, 1.0);
    1.0 - u * u * u
}

fn animate_shouts(
    time: Res<Time>,
    settings: Res<ShoutSettings>,
    cams: Query<(&Camera, &GlobalTransform, &Projection), With<Camera3d>>,
    targets: Query<&GlobalTransform, With<ConstructVisual>>,
    mut assemblies: Query<&mut crate::ConstructAssembly>,
    mut up: ResMut<FxUpload>,
    mut fx: ResMut<FxParticles>,
    mut sfx: MessageWriter<ShoutSfx>,
    mut shouts: Query<(Entity, &Shout, &mut ShoutState, &mut Transform)>,
    mut letters: Query<&mut Transform, Without<ShoutState>>,
) {
    let dt = time.delta_secs().min(0.1);
    let Some((cam, fov, aspect)) = camera(&cams) else { return };
    let (_, cam_rot, cam_pos) = cam.to_scale_rotation_translation();
    let (right, upv, fwd) = (cam.right().as_vec3(), cam.up().as_vec3(), cam.forward().as_vec3());
    let gentle = settings.gentle;
    for (e, s, mut st, mut tf) in &mut shouts {
        if st.released {
            continue;
        }
        st.age += dt;
        let target_boost = fin(s.boost, 1.0).clamp(1.0, 2.2);
        st.boost += (target_boost - st.boost) * (1.0 - (-dt * 6.0).exp());
        let intensity = shout_intensity(s.energy, st.boost);
        let age = st.age;
        // Summon assembly hook: the target construct is snapping its bricks on right now — keep the
        // hold short and ask its later stages to wait, so the letters release while bricks are
        // still to land and stream in *as* those bricks (finish_shouts).
        if !s.fizzle
            && s.stream_to.is_none()
            && let Some(t) = s.target
            && let Ok(mut a) = assemblies.get_mut(t)
            && a.progress < 1.0
        {
            // Short, punchy hold while bricks wait for the letters…
            st.hold = st.hold.min(BRICK_HOLD);
            let throw = if s.local { THROW } else { 0.0 };
            let release_in = (st.land + st.hold - age).max(0.0) + throw;
            // …and ask the later stages to wait (capped by the animator) so the first one still to
            // land lands just as the letter voxels arrive along the beam.
            let n = (a.stages as usize).min(5);
            if let Some(first) = (1..n).find(|&k| a.land_in[k] > 0.0) {
                let need = release_in + BRICK_LEAD - a.land_in[first];
                if need > 0.0 {
                    a.hold_request = a.hold_request.max(need);
                }
            }
        }
        let toward = s.target.and_then(|t| targets.get(t).ok()).map_or(fin3(s.toward), |g| g.translation());
        // ---- root placement
        let (pos, rot, unit) = if s.local {
            let hs = 2.0 * LOCAL_DEPTH * (fov * 0.5).tan();
            let ws = hs * aspect;
            // Letter height as a fraction of the screen: calm 5 %, a real yell ~15 %, a yelled MOON ~18 %.
            let frac = 0.045 + 0.1 * (intensity / 2.2).min(1.0) + 0.03 * ((intensity - 2.2) / 1.8).clamp(0.0, 1.0);
            // Compact (a caster's power title): ≤ 6 %, the block ≤ 30 % wide / 18 % tall, right and low.
            let (frac, w_cap, h_cap, cx, cy) = if s.compact { (frac.min(0.06), 0.3, 0.18, 0.3, -0.3) } else { (frac, 0.46, 0.36, 0.26, -0.2) };
            let unit = (frac * hs / GLYPH_H).min(w_cap * ws / st.block.x.max(0.1)).min(h_cap * hs / st.block.y.max(0.1));
            let mut p = cam_pos + fwd * LOCAL_DEPTH + right * (cx * ws) + upv * (cy * hs);
            if let Some(t0) = st.throw_at {
                // Flung out toward the construct: accelerates away (shrinks with perspective).
                let u = ((age - t0) / THROW).clamp(0.0, 1.0);
                let dest = if (toward - cam_pos).dot(fwd) > 1.0 { toward } else { cam_pos + fwd * 12.0 };
                p = p.lerp(dest, u * u * 0.45);
            }
            (p, cam_rot, unit)
        } else {
            let head = fin3(s.head);
            let dist = head.distance(cam_pos).max(0.5);
            st.far = dist > 28.0 && st.lines > 1 && st.line_scale[0] != st.line_scale[1];
            let lines_h = if st.far { GLYPH_H } else { st.block.y };
            // Readable at range (≥ ~1.8° per letter), never screen-filling up close.
            let h = (0.35 + 0.4 * intensity).clamp(dist * 0.032, (dist * 0.075).max(0.3));
            let unit = (h / GLYPH_H).min((dist * 0.9).max(3.0) / st.block.x.max(0.1));
            let centre = head + Vec3::Y * (0.55 + lines_h * unit * 0.5);
            let to_cam = (cam_pos - centre).normalize_or(Vec3::Z);
            let p = centre + to_cam * 0.3;
            let r = Transform::from_translation(p).looking_to(-to_cam, Vec3::Y).rotation;
            (p, r, unit)
        };
        tf.translation = pos;
        tf.rotation = rot;
        tf.scale = Vec3::splat(unit.max(1e-4));
        // Far LOD: re-centre on the visible (headline) line.
        let hidden_line = if st.far { Some(if st.line_scale[0] < st.line_scale[1] { 0u8 } else { 1u8 }) } else { None };
        let recentre = match hidden_line {
            Some(h) => -st.line_y[1 - h as usize],
            None => 0.0,
        };
        // Ring hand in shout units (letters erupt from it). Local: the first-person ring hand, lower
        // right, at the shout's depth (so an erupting letter never fills the screen).
        let hand = if s.local {
            let hs = 2.0 * LOCAL_DEPTH * (fov * 0.5).tan();
            cam_pos + fwd * LOCAL_DEPTH + right * (0.36 * hs * aspect) + upv * (-0.5 * hs)
        } else {
            fin3(s.anchor)
        };
        let origin = (rot.inverse() * (hand - pos)) / unit.max(1e-4);
        let origin = if origin.is_finite() { origin.clamp(Vec3::splat(-400.0), Vec3::splat(400.0)) } else { Vec3::ZERO };
        let punch = if gentle { 0.14 } else { 0.32 + 0.22 * intensity.min(2.5) };
        let shake = if gentle { 0.0 } else { ((intensity - 0.9).max(0.0) * 0.14).min(0.5) };
        let end = st.land + st.hold;
        let crack = ((age - (end - 0.1)) / 0.1).clamp(0.0, 1.0);
        let pal = shout_palette(s.color);
        let dim = if s.fizzle { 0.55 } else { 0.0 };
        let n = st.letters.len().max(1) as f32;
        for (i, l) in st.letters.iter().enumerate() {
            let t = age - l.delay;
            let fi = i as f32;
            let (mut p, mut sc, mut rz) = (origin, Vec3::ZERO, 0.0);
            if Some(l.line) == hidden_line {
                // hidden (far LOD)
            } else if t > 0.0 && t < FLY {
                let u = t / FLY;
                let k = ease_out3(u);
                p = origin.lerp(l.home, k) + Vec3::Y * (std::f32::consts::PI * u).sin() * GLYPH_H * 0.6 * l.scale;
                sc = Vec3::splat(l.scale * (0.3 + 0.7 * k));
                rz = l.spin * (1.0 - k);
            } else if t >= FLY {
                let k = ((t - FLY) / SLAM).clamp(0.0, 1.0);
                let pk = (k * std::f32::consts::PI * 2.5).sin() * (-k * 4.0).exp() * punch;
                let wob = (age * 13.0 + fi * 1.3).sin() * (0.05 * (-t * 3.0).exp() + 0.012);
                let jitter =
                    Vec3::new((age * 57.0 + fi * 3.1).sin(), (age * 61.0 + fi * 1.7).cos(), 0.0) * shake * GLYPH_H * 0.25 * l.scale;
                p = l.home + jitter;
                sc = Vec3::new(l.scale * (1.0 + 0.9 * pk), l.scale * (1.0 - 0.6 * pk), l.scale);
                rz = l.rot + wob;
            }
            p.y += recentre;
            if let Ok(mut lt) = letters.get_mut(l.entity) {
                lt.translation = p;
                lt.rotation = Quat::from_rotation_z(rz);
                lt.scale = sc;
            }
            let birth = if t > FLY { (1.0 - (t - FLY) / 0.09).clamp(0.0, 1.0) } else { 0.0 };
            let shine =
                if st.flourish && age > st.land { ((age - st.land) * 7.0 - fi / n * 3.0).sin().max(0.0).powi(8) * 0.55 } else { 0.0 };
            let flash = if gentle { birth * 0.25 } else { birth * 0.9 }.max(shine);
            let mut g = construct_gpu(
                &pal,
                1.0 - crack * 0.9,
                1.0,
                flash,
                dim,
                bd_core::CONSTRUCT_VOXEL_SIZE,
                l.slot * 7 + 3,
                1.0,
                false,
                Vec3::ZERO,
                1.0,
            );
            g.accent[3] = 1.0;
            write_slot(&mut up, l.slot, g);
        }
        // ---- headline slam: shockwave ring
        if !st.slammed && age >= st.land {
            st.slammed = true;
            let (r_axis, u_axis) = (rot * Vec3::X, rot * Vec3::Y);
            let centre = pos + u_axis * recentre * unit;
            let half_w = st.block.x * 0.5 * unit;
            let half_h = if st.far { GLYPH_H } else { st.block.y } * 0.5 * unit;
            let rings = if st.flourish { 2 } else { 1 };
            let count = ((14.0 + 16.0 * intensity.min(2.5)) * if gentle { 0.5 } else { 1.0 }) as usize;
            for ring in 0..rings {
                for k in 0..count {
                    let a = k as f32 / count as f32 * std::f32::consts::TAU + ring as f32 * 0.3;
                    let d = r_axis * a.cos() + u_axis * a.sin() * 0.55;
                    let start = centre + r_axis * a.cos() * half_w * 0.9 + u_axis * a.sin() * half_h * 1.1;
                    let seed = fx.rng.next_u32();
                    let c = pal.slot(if k % 3 == 0 { 3 } else { 0 });
                    fx.emit(Emit {
                        kind: PKind::Ballistic,
                        flags: PFlags::STRETCH | PFlags::PLAIN | if gentle { 0 } else { PFlags::WHITE_BIRTH },
                        pos: start,
                        vel: d * unit * (22.0 + 10.0 * intensity.min(2.5)) * (1.0 + 0.35 * ring as f32),
                        size: bd_core::CONSTRUCT_VOXEL_SIZE * unit * 1.3,
                        color: c,
                        emissive: 3.0,
                        life: 0.28 + 0.08 * ring as f32,
                        gravity: 0.0,
                        drag: 6.0,
                        spin: 3.0,
                        seed,
                        delay: 0.07 * ring as f32,
                        ..default()
                    });
                }
            }
            sfx.write(ShoutSfx { phase: ShoutPhase::Slam, pos: centre, intensity, local: s.local, entity: e });
        }
        // ---- end of hold: local shouts are flung first, then everything shatters (finish_shouts)
        if age >= end && s.local && st.throw_at.is_none() && !s.fizzle {
            // Flung out only toward something out in the world; a construct in the hand (sword)
            // takes the letters straight from the lower-right block.
            let far_target = (toward - cam_pos).dot(fwd) > 2.0;
            st.throw_at = Some(if far_target { age } else { age - THROW });
        }
    }
}

fn finish_shouts(
    mut commands: Commands,
    mut fx: ResMut<FxParticles>,
    fx_settings: Res<FxSettings>,
    settings: Res<ShoutSettings>,
    glyphs: Res<GlyphCache>,
    mut rt: ResMut<ShoutRuntime>,
    mut sfx: MessageWriter<ShoutSfx>,
    mut shouts: Query<(Entity, &Shout, &mut ShoutState)>,
    letters: Query<&GlobalTransform>,
    targets: Query<(&ConstructVisual, &GlobalTransform, &FxAnchor, &ConstructRuntime)>,
    assemblies: Query<&crate::ConstructAssembly>,
) {
    let rt = &mut *rt;
    for (e, s, mut st) in &mut shouts {
        if st.released {
            continue;
        }
        let done = match st.throw_at {
            Some(t0) => st.age >= t0 + THROW,
            None => st.age >= st.land + st.hold && (!s.local || s.fizzle),
        };
        if !done {
            continue;
        }
        st.released = true;
        let _ = st.serial;
        let intensity = shout_intensity(s.energy, st.boost);
        let pal = shout_palette(s.color);
        let hidden_line = if st.far { Some(if st.line_scale[0] < st.line_scale[1] { 0u8 } else { 1u8 }) } else { None };
        let target = if s.fizzle || s.stream_to.is_some() { None } else { s.target.and_then(|t| targets.get(t).ok()) };
        let mut centre = Vec3::ZERO;
        let mut shown = 0usize;
        if let Some(to) = s.stream_to.filter(|p| p.is_finite() && !s.fizzle) {
            // Hook: every letter voxel flies to one world point (anchor 0 = world frame).
            let total: usize =
                st.letters.iter().filter_map(|l| glyphs.glyphs.get(&l.ch)).map(|g| g.model.voxel_count).sum::<usize>().max(1);
            let cap = (settings.stream_cap as f32 * fx_settings.particle_budget.clamp(0.0, 1.0)).max(24.0);
            let keep = (cap / total as f32).min(1.0);
            for (i, l) in st.letters.iter().enumerate() {
                let (Some(g), Ok(lgt)) = (glyphs.glyphs.get(&l.ch), letters.get(l.entity)) else { continue };
                if Some(l.line) == hidden_line {
                    continue;
                }
                centre += lgt.translation();
                shown += 1;
                let tf = lgt.affine() * bevy::math::Affine3A::from_translation(-g.offset);
                let size = (g.model.voxel_size * tf.matrix3.x_axis.length() * 1.4).clamp(0.02, 0.2);
                for part in &g.model.parts {
                    for (vx, tint) in &part.voxels {
                        if matches!(tint, bd_core::shape::Tint::Dark) || fx.rng.f() > keep {
                            continue;
                        }
                        let wp = tf.transform_point3(v3(part.voxel_center(*vx)));
                        let jitter = fx.rng.vec() * 0.12;
                        let seed = fx.rng.next_u32();
                        let life = 0.28 + 0.25 * fx.rng.f() + 0.012 * i as f32;
                        let color = pal.slot(if fx.rng.f() < 0.75 { 0 } else { 3 });
                        fx.emit(Emit {
                            kind: PKind::Stream,
                            pos: wp,
                            vel: to + jitter,
                            size,
                            color,
                            emissive: 2.6,
                            life,
                            anchor: 0,
                            seed,
                            ..default()
                        });
                    }
                }
            }
            sfx.write(ShoutSfx {
                phase: ShoutPhase::Release { streamed: true },
                pos: centre / shown.max(1) as f32,
                intensity,
                local: s.local,
                entity: e,
            });
            commands.entity(e).despawn();
            continue;
        }
        // Summon assembly hook: the target is still snapping bricks on — the letter voxels fly into
        // the ring beam, then ride it to the stages still to land, arriving as each stage lands.
        let asm = s.target.filter(|_| target.is_some()).and_then(|t| assemblies.get(t).ok()).filter(|a| a.pending(BRICK_LEAD).next().is_some());
        if let Some(a) = asm {
            let n_stages = a.pending(BRICK_LEAD).count().max(1);
            let total: usize =
                st.letters.iter().filter(|l| Some(l.line) != hidden_line).filter_map(|l| glyphs.glyphs.get(&l.ch)).map(|g| g.model.voxel_count).sum::<usize>().max(1);
            // Two particles per voxel (to the ring, then along the beam): half the usual count.
            let cap = (settings.stream_cap as f32 * fx_settings.particle_budget.clamp(0.0, 1.0) * 0.5).max(24.0);
            let keep = (cap / total as f32).min(1.0);
            let ring = fin3(a.ring);
            let mut k = 0usize;
            for l in &st.letters {
                if Some(l.line) == hidden_line {
                    continue;
                }
                let (Some(g), Ok(lgt)) = (glyphs.glyphs.get(&l.ch), letters.get(l.entity)) else { continue };
                centre += lgt.translation();
                shown += 1;
                let tf = lgt.affine() * bevy::math::Affine3A::from_translation(-g.offset);
                let size = (g.model.voxel_size * tf.matrix3.x_axis.length() * 1.4).clamp(0.03, 0.22);
                for part in &g.model.parts {
                    for (vx, tint) in &part.voxels {
                        if matches!(tint, bd_core::shape::Tint::Dark) || fx.rng.f() > keep {
                            continue;
                        }
                        // Round-robin over the stages still to land (no allocation).
                        let pick = k % n_stages;
                        k += 1;
                        let Some((_, slot, secs, radius)) = a.pending(BRICK_LEAD).nth(pick) else { continue };
                        let wp = tf.transform_point3(v3(part.voxel_center(*vx)));
                        let to_ring = BRICK_TO_RING * (0.8 + 0.4 * fx.rng.f());
                        let brick = slot + fx.rng.vec() * radius.clamp(0.05, 20.0) * 0.8;
                        let color = pal.slot(if fx.rng.f() < 0.75 { 0 } else { 3 });
                        let (s1, s2) = (fx.rng.next_u32(), fx.rng.next_u32());
                        let ring_j = ring + fx.rng.vec() * 0.08;
                        fx.emit(Emit {
                            kind: PKind::Stream,
                            flags: if settings.gentle { 0 } else { PFlags::WHITE_BIRTH },
                            pos: wp,
                            vel: ring_j,
                            size,
                            color,
                            emissive: 2.6,
                            life: to_ring,
                            anchor: 0,
                            seed: s1,
                            ..default()
                        });
                        fx.emit(Emit {
                            kind: PKind::Stream,
                            pos: ring,
                            vel: brick,
                            size: size * 0.9,
                            color,
                            emissive: 3.0,
                            life: (secs - to_ring).max(0.05),
                            anchor: 0,
                            seed: s2,
                            delay: to_ring,
                            ..default()
                        });
                    }
                }
            }
            sfx.write(ShoutSfx { phase: ShoutPhase::Release { streamed: true }, pos: centre / shown.max(1) as f32, intensity, local: s.local, entity: e });
            commands.entity(e).despawn();
            continue;
        }
        match target {
            Some((v, gt, anchor, crt)) => {
                // The words become the thing: letter voxels stream into the construct's voxels.
                let root = gt.affine();
                let ring = v.ring.unwrap_or(gt.translation());
                let tscale = root.matrix3.x_axis.length().max(1e-3);
                let tsize = v.model.voxel_size * tscale;
                let total: usize = st
                    .letters
                    .iter()
                    .filter(|l| Some(l.line) != hidden_line)
                    .filter_map(|l| glyphs.glyphs.get(&l.ch))
                    .map(|g| g.model.voxel_count)
                    .sum::<usize>()
                    .max(1);
                let cap = (settings.stream_cap as f32 * fx_settings.particle_budget.clamp(0.0, 1.0)).max(24.0);
                let keep = (cap / total as f32).min(1.0);
                for (i, l) in st.letters.iter().enumerate() {
                    if Some(l.line) == hidden_line {
                        continue;
                    }
                    let (Some(g), Ok(lgt)) = (glyphs.glyphs.get(&l.ch), letters.get(l.entity)) else { continue };
                    centre += lgt.translation();
                    shown += 1;
                    let tf = lgt.affine() * bevy::math::Affine3A::from_translation(-g.offset);
                    let lsize = g.model.voxel_size * tf.matrix3.x_axis.length();
                    let size = (tsize * 0.8).min((lsize * 1.4).max(tsize * 0.3));
                    for part in &g.model.parts {
                        for (vx, tint) in &part.voxels {
                            if matches!(tint, bd_core::shape::Tint::Dark) || fx.rng.f() > keep {
                                continue;
                            }
                            let wp = tf.transform_point3(v3(part.voxel_center(*vx)));
                            let r = fx.rng.next_u32();
                            let Some((pi, c)) = random_voxel(&v.model, crt.cum_slice(), r) else { break };
                            let local = crt.pose.get(pi).map(|p| p.transform_point3(c)).unwrap_or(c);
                            let seed = fx.rng.next_u32();
                            let life = 0.32 + 0.3 * fx.rng.f() + 0.015 * i as f32;
                            let color = pal.slot(if fx.rng.f() < 0.75 { 0 } else { 3 });
                            fx.emit(Emit {
                                kind: PKind::Stream,
                                flags: if settings.gentle { 0 } else { PFlags::WHITE_BIRTH },
                                pos: wp - ring,
                                vel: v3(local),
                                size,
                                color,
                                emissive: 2.6,
                                life,
                                anchor: anchor.0,
                                seed,
                                ..default()
                            });
                        }
                    }
                }
                sfx.write(ShoutSfx {
                    phase: ShoutPhase::Release { streamed: true },
                    pos: centre / shown.max(1) as f32,
                    intensity,
                    local: s.local,
                    entity: e,
                });
            }
            None => {
                let toward = fin3(s.toward);
                let keep = if st.far { 0.35 } else { 0.7 };
                for l in &st.letters {
                    if Some(l.line) == hidden_line {
                        continue;
                    }
                    let (Some(g), Ok(lgt)) = (glyphs.glyphs.get(&l.ch), letters.get(l.entity)) else { continue };
                    let p = lgt.translation();
                    centre += p;
                    shown += 1;
                    let tf = lgt.affine() * bevy::math::Affine3A::from_translation(-g.offset);
                    let impulse = if s.fizzle {
                        Vec3::new(0.0, -0.6, 0.0)
                    } else {
                        (toward - p).normalize_or(Vec3::NEG_Y) * (5.0 + 3.0 * intensity.min(2.5)) * if s.local { 0.35 } else { 1.0 }
                    };
                    let life = if s.fizzle { 0.45 } else { 0.55 };
                    shatter_model(
                        &mut fx,
                        &fx_settings,
                        &g.model,
                        &tf,
                        &[],
                        &pal,
                        impulse,
                        Some(p - impulse.normalize_or_zero() * 0.2),
                        None,
                        life,
                        0.8,
                        keep,
                        &mut rt.scratch,
                    );
                }
                let phase = if s.fizzle { ShoutPhase::Fizzle } else { ShoutPhase::Release { streamed: false } };
                sfx.write(ShoutSfx { phase, pos: centre / shown.max(1) as f32, intensity, local: s.local, entity: e });
            }
        }
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> ShoutTable {
        ShoutTable::parse(DEFAULT_TABLE).expect("shout table parses")
    }

    #[test]
    fn table_has_data() {
        let t = table();
        assert!(t.amplifiers.contains_key("super") && t.amplifiers.contains_key("mega"));
        assert!(t.joiners.iter().any(|j| j == "no"));
        assert!(t.voice_yell > t.voice_talk);
    }

    #[test]
    fn sanitize_caps_length_and_charset() {
        assert_eq!(sanitize_shout("  giant\t\nhammer!!!  ", 48), "giant hammer!!!");
        assert_eq!(sanitize_shout("héllo <script>wörld", 48), "hllo scriptwrld");
        assert_eq!(sanitize_shout("\u{7}\u{0}", 48), "");
        assert_eq!(sanitize_shout("HAMMERRRRRRRRRRRRRRRR", 48), "HAMMERRRRRR");
        let long = "giant eeyore shooting laser beams at a very large castle made of cheese";
        let s = sanitize_shout(long, SHOUT_MAX_CHARS);
        assert!(s.len() <= SHOUT_MAX_CHARS, "{s}");
        assert!(long.starts_with(&s), "cut at a word boundary: {s}");
        assert!(!s.ends_with(' '));
        let s = sanitize_shout(&"x".repeat(10_000), SHOUT_MAX_CHARS);
        assert!(s.len() <= SHOUT_MAX_CHARS);
        let s = sanitize_shout(&"ab ".repeat(10_000), SHOUT_MAX_CHARS);
        assert!(s.len() <= SHOUT_MAX_CHARS && s.is_ascii());
        for c in sanitize_shout("¡Gomu-Gomu no… PISTOL?!", 48).chars() {
            assert!(font_char(c) || c == ' ');
        }
    }

    #[test]
    fn typed_energy_orders_by_shoutiness() {
        let t = table();
        let calm = typed_energy("sword", &t);
        let cap = typed_energy("Sword", &t);
        let caps = typed_energy("SWORD", &t);
        let bang = typed_energy("GIANT HAMMER!!!", &t);
        let stretch = typed_energy("HAMMERRRR", &t);
        assert!(calm < 0.5, "lowercase is calm: {calm}");
        assert!((cap - calm).abs() < 0.05, "one capital stays calm");
        assert!(caps > calm + 0.4);
        assert!(stretch > caps);
        assert!(bang > caps + 0.3, "{bang}");
        assert!(typed_energy("super mega hammer", &t) > typed_energy("big red hammer", &t));
        for s in ["", "!!!!!!!!!!!!", "ÄÖÜ", "a", &"A".repeat(200)] {
            let e = typed_energy(s, &t);
            assert!(e.is_finite() && (0.25..=2.3).contains(&e), "{s:?} → {e}");
        }
    }

    #[test]
    fn voice_energy_scales_with_level() {
        let t = table();
        let talk = voice_energy(0.6, "hammer", &t);
        let yell = voice_energy(1.0, "hammer", &t);
        assert!(talk < 0.7, "talking is modest: {talk}");
        assert!(yell > 2.0, "a real yell is enormous: {yell}");
        let mut last = 0.0;
        for i in 0..=20 {
            let e = voice_energy(i as f32 / 20.0, "x", &t);
            assert!(e >= last - 1e-6, "monotonic");
            last = e;
        }
        for l in [f32::NAN, f32::INFINITY, -5.0, 9.0] {
            assert!(voice_energy(l, "x", &t).is_finite());
        }
    }

    #[test]
    fn size_and_intensity() {
        assert_eq!(size_boost(0.5), 1.0);
        assert!(size_boost(10.0) > 1.4);
        assert_eq!(size_boost(3000.0), 2.2, "a moon maxes out");
        assert_eq!(size_boost(f32::NAN), 1.0);
        let moon = shout_intensity(voice_energy(1.0, "MOON", &table()), size_boost(3000.0));
        let sword = shout_intensity(typed_energy("sword", &table()), size_boost(1.0));
        assert!(moon > 4.0 * sword, "{moon} vs {sword}");
        assert!(shout_intensity(f32::NAN, f32::INFINITY).is_finite());
    }

    #[test]
    fn layout_rules() {
        let t = table();
        let one = layout_shout("SWORD", &t);
        assert_eq!(one.n, 1);
        assert_eq!(one.lines[0].scale, 1.0);
        let gomu = "GOMU GOMU NO PISTOL";
        let l = layout_shout(gomu, &t);
        assert!(l.joiner && l.flourish && l.n == 2);
        assert_eq!(&gomu[l.lines[0].start..l.lines[0].end], "GOMU GOMU NO");
        assert_eq!(&gomu[l.lines[1].start..l.lines[1].end], "PISTOL");
        assert!(l.lines[0].scale < l.lines[1].scale);
        // "no" at the edges is not a joiner.
        assert!(!layout_shout("no mercy", &t).joiner);
        let long = "GIANT EEYORE SHOOTING LASER BEAMS";
        let l = layout_shout(long, &t);
        assert_eq!(l.n, 2);
        let (a, b) = (&long[l.lines[0].start..l.lines[0].end], &long[l.lines[1].start..l.lines[1].end]);
        assert_eq!(format!("{a} {b}"), long);
        assert!(a.len().abs_diff(b.len()) <= 8, "{a} / {b}");
        assert_eq!(l.lines[0].scale, l.lines[1].scale, "equally loud lines are equal");
        let l = layout_shout("a very large ULTRA HAMMER!!!", &t);
        assert!(l.flourish);
        assert!(l.lines[1].scale > l.lines[0].scale, "the shoutier line is the headline");
        for s in ["", " ", "a b c d e f g h i j k l m n o p q r s t u v w x y z a b c d e f g h", &"W".repeat(48)] {
            let l = layout_shout(s, &t);
            assert!(l.n >= 1 && l.n <= 2);
            for line in l.lines() {
                assert!(line.start <= line.end && line.end <= s.len());
            }
        }
    }
}
