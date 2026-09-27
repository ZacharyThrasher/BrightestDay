//! Onomatopoeia: comic hard-light voxel words at impacts ("Juicy Text", ICMI'24: text + particles
//! reads best; words carry material/semantics).
//!
//! `SpawnOnomatopoeia` → a word of voxel glyphs (see [`crate::font`]) facing the camera at the
//! contact point: letters pop in with overshoot (~80 ms, staggered), wobble, hold ~0.35–0.5 s, crack,
//! then **shatter into their voxels** through the normal shatter path (blending with the impact
//! debris). Size/boldness/jitter scale with `impact`. Performance words (PERFECT, GOOD, SLOW,
//! PARRY!, medals) are screen-anchored under the crosshair, small and brief.
//!
//! Every impact word goes through the world-text policy ([`crate::text_budget`],
//! `assets/fx/text_budget.ron`): never in the camera's lap or over the crosshair, size clamped to a
//! screen fraction, your own hits small / peripheral / brief, a priority-based cap on words near
//! you, and rapid light hits thinned to every Nth word.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use bd_shapes::VoxelModel;
use bevy::camera::visibility::VisibilitySystems;
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use serde::Deserialize;

use crate::construct::{ConstructMeshCache, FxSlot, ModelKey, SlotAlloc, construct_gpu, write_slot};
use crate::effects::{ShatterScratch, shatter_model};
use crate::font::{ITALIC, SPACE_ADVANCE, all_chars, glyph_bits, glyph_spec};
use crate::gpu::FxUpload;
use crate::material::FxMaterials;
use crate::palette::Palette;
use crate::particles::{Emit, EmitSystems, FxParticles, PFlags, PKind};
use crate::text_budget::{LightGate, TextBudget, View, pick_eviction, place_word, priority};
use crate::util::{FxRng, fin, fin3};
use crate::{FxSet, FxSettings};

// ------------------------------------------------------------------------------------ API

/// Which word to show.
#[derive(Clone, Debug)]
pub enum OnoWord {
    /// Look up `assets/fx/onomatopoeia.ron` (e.g. "stone", "explosion", "hit_heavy", "perfect").
    Key(Cow<'static, str>),
    /// Literal text (unsupported characters are skipped).
    Text(Cow<'static, str>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnoStyle {
    /// World-space word at the contact point.
    #[default]
    Impact,
    /// Screen-anchored near the crosshair, small, brief (PERFECT / GOOD / SLOW / PARRY! / medals).
    Performance,
}

/// Spawn a comic word.
#[derive(Message, Clone, Debug)]
pub struct SpawnOnomatopoeia {
    pub word: OnoWord,
    pub pos: Vec3,
    /// Hit direction (the word leans/drifts along it; zero = none).
    pub dir: Vec3,
    /// ≈ normalised impact (0.2 tap … 1 heavy … 2+ kaiju). Scales size, boldness, jitter, hold.
    pub impact: f32,
    /// Attacker corps colour (linear RGB); material words lean toward the table's tint.
    pub color: [f32; 3],
    pub style: OnoStyle,
    /// Deterministic word/jitter choice (e.g. the sim event id).
    pub event_id: u64,
    /// Events involving the local player get priority under the rate limit — and are drawn small,
    /// peripheral and brief (text_budget `own_*`).
    pub local_player: bool,
    /// Who caused it (e.g. attacker id + 1; 0 = anonymous): rapid light hits from one source are
    /// thinned to every Nth word (text_budget `light_every`).
    pub source: u64,
    /// A crit / finisher / KO: top priority, never thinned, shown even in the reduced mode.
    pub accent: bool,
}

impl SpawnOnomatopoeia {
    pub fn key(key: &'static str, pos: Vec3, impact: f32, color: [f32; 3]) -> Self {
        SpawnOnomatopoeia { word: OnoWord::Key(Cow::Borrowed(key)), pos, dir: Vec3::ZERO, impact, color, style: OnoStyle::Impact, event_id: 0, local_player: false, source: 0, accent: false }
    }
    pub fn performance(key: &'static str, color: [f32; 3]) -> Self {
        SpawnOnomatopoeia { word: OnoWord::Key(Cow::Borrowed(key)), pos: Vec3::ZERO, dir: Vec3::ZERO, impact: 0.5, color, style: OnoStyle::Performance, event_id: 0, local_player: true, source: 0, accent: true }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnoMode {
    /// No comic words and no move titles (`Shout::title`).
    Off,
    /// "Reduced": impact words only above `big_hit_threshold` or accented (performance words always),
    /// fewer words near you (text_budget `max_near_reduced`).
    #[default]
    BigHitsOnly,
    FullComic,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnoLanguage {
    #[default]
    En,
    Manga,
}

#[derive(Resource, Clone, Debug)]
pub struct OnomatopoeiaSettings {
    pub mode: OnoMode,
    pub language: OnoLanguage,
    /// Token-bucket rate limit (local-player events may borrow 2 extra).
    pub max_per_second: f32,
    pub big_hit_threshold: f32,
    /// Live words cap (oldest non-local word shatters early).
    pub max_live: usize,
    /// Multiplier on the hold time (1 = default; debug/tuning).
    pub hold_scale: f32,
}

impl Default for OnomatopoeiaSettings {
    fn default() -> Self {
        OnomatopoeiaSettings { mode: OnoMode::BigHitsOnly, language: OnoLanguage::En, max_per_second: 4.0, big_hit_threshold: 0.6, max_live: 10, hold_scale: 1.0 }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct OnoEntry {
    pub en: Vec<String>,
    pub manga: Vec<String>,
    #[serde(default)]
    pub tint: Option<(f32, f32, f32)>,
    #[serde(default = "one")]
    pub size: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize)]
struct TableFile {
    entries: HashMap<String, OnoEntry>,
}

/// Event key → words (`assets/fx/onomatopoeia.ron`, compiled in; `load` to hot-swap).
#[derive(Resource, Clone, Debug, Default)]
pub struct OnomatopoeiaTable {
    pub entries: HashMap<String, OnoEntry>,
}

const DEFAULT_TABLE: &str = include_str!("../../../assets/fx/onomatopoeia.ron");

impl OnomatopoeiaTable {
    pub fn parse(src: &str) -> Result<Self, String> {
        let f: TableFile = ron::from_str(src).map_err(|e| e.to_string())?;
        Ok(OnomatopoeiaTable { entries: f.entries })
    }
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::parse(&s)
    }
}

// ------------------------------------------------------------------------------------ internals

pub(crate) struct Glyph {
    pub(crate) model: Arc<VoxelModel>,
    pub(crate) mesh: Handle<Mesh>,
    /// Mesh centre in glyph model space (m).
    pub(crate) offset: Vec3,
    /// Advance (m, before word scale).
    pub(crate) advance: f32,
}

/// Shared with [`crate::shout`].
#[derive(Resource, Default)]
pub(crate) struct GlyphCache {
    pub(crate) glyphs: HashMap<char, Glyph>,
}

struct Letter {
    entity: Entity,
    ch: char,
    slot: u32,
    /// Letter centre in word space (m, before word scale).
    centre: Vec3,
    delay: f32,
    jitter_rot: f32,
    jitter_scale: f32,
}

#[derive(Component)]
struct OnoWordState {
    letters: Vec<Letter>,
    age: f32,
    pop: f32,
    hold: f32,
    impact: f32,
    style: OnoStyle,
    dir: Vec3,
    base: Vec3,
    color: [f32; 3],
    rainbow: bool,
    scale: f32,
    roll: f32,
    local: bool,
    serial: u64,
    shattered: bool,
    /// World half extents (m) of the laid-out word (for de-overlapping).
    ext: Vec2,
    /// Smoothed de-overlap offset (world).
    stack: Vec3,
    /// Budget priority (text_budget::priority).
    pri: f32,
    /// Sticky crosshair push side for own words (0 = not chosen yet).
    side: i8,
    /// This frame's placement scale (text_budget size clamp) — `ext` × this is on screen.
    place_scale: f32,
}

#[derive(Resource, Default)]
struct OnoRuntime {
    tokens: f32,
    serial: u64,
    scratch: ShatterScratch,
    light: LightGate,
}

pub fn build(app: &mut App) {
    let table = OnomatopoeiaTable::parse(DEFAULT_TABLE).unwrap_or_else(|e| {
        warn!("bd_fx: onomatopoeia.ron parse error: {e}");
        OnomatopoeiaTable::default()
    });
    app.insert_resource(table)
        .init_resource::<OnomatopoeiaSettings>()
        .init_resource::<GlyphCache>()
        .init_resource::<OnoRuntime>()
        .add_message::<SpawnOnomatopoeia>()
        .add_systems(Startup, build_glyphs)
        .add_systems(PostUpdate, spawn_words.in_set(FxSet::Lifecycle))
        .add_systems(PostUpdate, animate_words.in_set(FxSet::Pose))
        .add_systems(PostUpdate, finish_words.in_set(EmitSystems).before(VisibilitySystems::CheckVisibility));
}

fn build_glyphs(mut glyphs: ResMut<GlyphCache>, mut cache: ResMut<ConstructMeshCache>, mut meshes: ResMut<Assets<Mesh>>) {
    for c in all_chars() {
        let Some(bits) = glyph_bits(c) else { continue };
        let model = bd_shapes::voxelize(&glyph_spec(&bits));
        let parts = cache.insert_now(ModelKey::Named("glyph", c as u32), &model, &mut meshes);
        let Some(Some(p)) = parts.first() else { continue };
        // Smooth italic: shear the glyph mesh (about its centre) instead of stepping voxels.
        if let Some(mut mesh) = meshes.get_mut(&p.mesh)
            && let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {
            for v in pos.iter_mut() {
                v[0] += v[1] * ITALIC;
            }
        }
        // glyph + italic step (1) + drop extrusion (1) + gap (1)
        let advance = (bits.w as f32 + 2.5) * model.voxel_size;
        glyphs.glyphs.insert(c, Glyph { model: Arc::new(model), mesh: p.mesh.clone(), offset: p.offset, advance });
    }
}

type Cams<'w, 's> = Query<'w, 's, (&'static Camera, &'static GlobalTransform, &'static Projection), With<Camera3d>>;

fn camera(q: &Cams) -> Option<(GlobalTransform, View)> {
    // Full-window cameras only (inset viewports, e.g. the summon preview, never own the words).
    q.iter().filter(|(c, _, _)| c.is_active && c.viewport.is_none()).max_by_key(|(c, _, _)| c.order).map(|(_, g, p)| {
        let (fov, aspect) = match p {
            Projection::Perspective(p) => (fin(p.fov, 1.2), fin(p.aspect_ratio, 1.78)),
            _ => (1.2, 1.78),
        };
        (*g, View::new(g, fov, aspect))
    })
}

fn hue(h: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let f = |n: f32| {
        let k = (n + h).rem_euclid(6.0);
        1.0 - (k.min(4.0 - k).clamp(0.0, 1.0))
    };
    [f(5.0), f(3.0), f(1.0)]
}

fn spawn_words(
    mut commands: Commands,
    mut msgs: MessageReader<SpawnOnomatopoeia>,
    settings: Res<OnomatopoeiaSettings>,
    table: Res<OnomatopoeiaTable>,
    glyphs: Res<GlyphCache>,
    mats: Res<FxMaterials>,
    mut alloc: ResMut<SlotAlloc>,
    mut rt: ResMut<OnoRuntime>,
    time: Res<Time>,
    budget: Res<TextBudget>,
    cams: Cams,
    mut live: Query<&mut OnoWordState>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    let cam_pos = camera(&cams).map(|(g, _)| g.translation());
    let b = &budget.budget;
    rt.tokens = (rt.tokens + settings.max_per_second.max(0.0) * dt).min(settings.max_per_second.max(1.0));
    for m in msgs.read() {
        if settings.mode == OnoMode::Off {
            continue;
        }
        let impact = fin(m.impact, 0.5).clamp(0.0, 3.0);
        let perf = m.style == OnoStyle::Performance;
        if !perf && !m.accent && settings.mode == OnoMode::BigHitsOnly && impact < settings.big_hit_threshold {
            continue;
        }
        // Rapid light hits: every Nth word per source (crits / finishers always).
        if !perf && !rt.light.admit(b, m.source, now, impact, m.accent) {
            continue;
        }
        // The camera is inside the event (you are the blast / the victim up close): no word.
        if !perf && cam_pos.is_some_and(|c| c.distance(fin3(m.pos)) < budget.placement.skip_cam_dist) {
            continue;
        }
        let pri = priority(b, impact, m.local_player, m.accent);
        // Global cap near the camera: replace the least important live word, or skip.
        if !perf && let Some(c) = cam_pos {
            let p = fin3(m.pos);
            if p.distance(c) < b.near_cam_m {
                let max = budget.max_near(settings.mode == OnoMode::BigHitsOnly);
                let mut near: Vec<Mut<OnoWordState>> = live
                    .iter_mut()
                    .filter(|w| !w.shattered && w.style == OnoStyle::Impact && w.hold > w.age - w.pop && w.base.distance(c) < b.near_cam_m)
                    .collect();
                let ranks: Vec<(f32, f32)> = near.iter().map(|w| (w.pri, w.age)).collect();
                match pick_eviction(&ranks, max, pri) {
                    Ok(None) => {}
                    Ok(Some(i)) => {
                        let w = &mut near[i];
                        w.hold = w.hold.min((w.age - w.pop).max(0.0));
                    }
                    Err(()) => continue,
                }
            }
        }
        // Rate limit (performance words and local-player events may borrow).
        let floor = if m.local_player || m.style == OnoStyle::Performance { -2.0 } else { 1.0 };
        if rt.tokens < floor {
            continue;
        }
        rt.tokens -= 1.0;
        // Resolve text.
        let (text, tint, size_k): (Cow<str>, Option<(f32, f32, f32)>, f32) = match &m.word {
            OnoWord::Text(t) => (t.clone(), None, 1.0),
            OnoWord::Key(k) => match table.entries.get(k.as_ref()) {
                Some(e) => {
                    let list = if settings.language == OnoLanguage::Manga && !e.manga.is_empty() { &e.manga } else { &e.en };
                    if list.is_empty() {
                        continue;
                    }
                    let w = &list[(m.event_id as usize) % list.len()];
                    (Cow::Owned(w.clone()), e.tint, e.size)
                }
                None => continue,
            },
        };
        // Evict the oldest non-local word when over the cap.
        let n_live = live.iter().filter(|w| !w.shattered).count();
        if n_live >= settings.max_live {
            let oldest = live.iter_mut().filter(|w| !w.shattered && (!w.local || m.local_player)).max_by(|a, b| a.age.total_cmp(&b.age));
            match oldest {
                Some(mut w) => {
                    w.hold = w.hold.min(w.age - w.pop);
                }
                None => continue,
            }
        }
        if perf {
            // One fixed near-crosshair slot: the new performance word replaces the old one.
            for mut w in live.iter_mut().filter(|w| !w.shattered && w.style == OnoStyle::Performance) {
                w.hold = w.hold.min((w.age - w.pop).max(0.0));
            }
        } else {
            let p = fin3(m.pos);
            let mut near: Vec<Mut<OnoWordState>> = live.iter_mut().filter(|w| !w.shattered && w.style == OnoStyle::Impact && w.base.distance(p) < b.cluster_m && w.hold > w.age - w.pop).collect();
            let max_cluster = b.max_cluster.max(1);
            if near.len() >= max_cluster {
                near.sort_by(|a, b| b.age.total_cmp(&a.age));
                let drop = near.len() + 1 - max_cluster;
                for w in near.iter_mut().take(drop) {
                    w.hold = w.hold.min((w.age - w.pop).max(0.0));
                }
            }
        }
        let mut rng = FxRng::new(m.event_id ^ (rt.serial << 20) ^ 0x0A0A);
        rt.serial += 1;
        // Colour: corps colour leaning toward the material tint.
        let mut color = m.color;
        if let Some((r, g, b)) = tint {
            let k = if perf { 0.85 } else { 0.55 };
            color = [color[0] + (r - color[0]) * k, color[1] + (g - color[1]) * k, color[2] + (b - color[2]) * k];
        }
        let upper = text.to_uppercase();
        let rainbow = perf && (upper.starts_with("PERFECT") || upper.starts_with("K.O"));
        // Layout.
        let mut x = 0.0;
        let mut placed: Vec<(char, f32)> = Vec::new();
        for c in upper.chars() {
            if c == ' ' {
                x += SPACE_ADVANCE * bd_core::CONSTRUCT_VOXEL_SIZE;
                continue;
            }
            if let Some(g) = glyphs.glyphs.get(&c) {
                placed.push((c, x));
                x += g.advance;
            }
        }
        if placed.is_empty() {
            continue;
        }
        let width = x;
        let root = commands
            .spawn((Name::new(format!("ono {upper}")), Transform::from_translation(fin3(m.pos)), Visibility::Visible))
            .id();
        let stagger = if perf { 0.02 } else { 0.03 };
        let jit = if perf { 0.0 } else { 0.025 + 0.035 * impact.min(1.5) };
        let mut letters = Vec::with_capacity(placed.len());
        for (i, (c, lx)) in placed.iter().enumerate() {
            let g = &glyphs.glyphs[c];
            let slot = alloc.slot();
            let centre = Vec3::new(lx - width * 0.5, 0.0, 0.0) + g.offset;
            let e = commands
                .spawn((
                    Mesh3d(g.mesh.clone()),
                    MeshMaterial3d(if perf { mats.construct_overlay.clone() } else { mats.construct(false) }),
                    MeshTag(slot),
                    FxSlot(slot),
                    Transform::from_translation(centre).with_scale(Vec3::ZERO),
                    NotShadowCaster,
                    ChildOf(root),
                ))
                .id();
            letters.push(Letter {
                entity: e,
                ch: *c,
                slot,
                centre,
                delay: i as f32 * stagger,
                jitter_rot: rng.range(-jit, jit),
                jitter_scale: 1.0 + rng.range(-0.5, 0.5) * jit * 0.6,
            });
        }
        let height = if perf { 0.05 } else { (0.3 + 0.3 * impact.min(2.0)) * size_k };
        commands.entity(root).insert(OnoWordState {
            letters,
            age: 0.0,
            pop: 0.08,
            hold: settings.hold_scale.max(0.1)
                * if perf { 0.32 } else { 0.35 + 0.1 * impact.min(1.5) }
                * if m.local_player && !perf && !m.accent { budget.placement.own_hold.clamp(0.1, 1.0) } else { 1.0 },
            impact,
            style: m.style,
            dir: fin3(m.dir),
            base: fin3(m.pos),
            color,
            rainbow,
            scale: height / (7.0 * bd_core::CONSTRUCT_VOXEL_SIZE),
            ext: Vec2::new(width * 0.5, 9.0 * bd_core::CONSTRUCT_VOXEL_SIZE * 0.5) * (height / (7.0 * bd_core::CONSTRUCT_VOXEL_SIZE)),
            stack: Vec3::ZERO,
            roll: if perf { 0.0 } else { rng.range(-0.18, 0.18) },
            local: m.local_player,
            serial: rt.serial,
            shattered: false,
            pri,
            side: 0,
            place_scale: 1.0,
        });
    }
}

fn ease_back(x: f32, s: f32) -> f32 {
    let x = x.clamp(0.0, 1.0) - 1.0;
    1.0 + (s + 1.0) * x * x * x + s * x * x
}

/// Screen-space de-overlap: performance word first (fixed slot), then the crosshair's protected box,
/// then impact words oldest → newest; a word that overlaps an already placed one (or the crosshair)
/// is pushed up (camera up) until it is clear.
fn resolve_overlaps(cam: &GlobalTransform, crosshair: f32, items: &mut [(Entity, Vec3, Vec2, u64, bool, Vec3)], placed: &mut Vec<Vec4>) {
    let (cp, right, upv, fwd) = (cam.translation(), cam.right().as_vec3(), cam.up().as_vec3(), cam.forward().as_vec3());
    items.sort_by_key(|it| (!it.4, it.3));
    placed.clear();
    let mut guarded = false;
    for it in items.iter_mut() {
        if !it.4 && !guarded {
            guarded = true;
            placed.push(Vec4::new(0.0, 0.0, crosshair, crosshair));
        }
        let rel = it.1 - cp;
        let d = rel.dot(fwd).max(0.3);
        let u = rel.dot(right) / d;
        let v0 = rel.dot(upv) / d;
        let (hw, hh) = (it.2.x / d, it.2.y / d);
        let mut v = v0;
        for _ in 0..8 {
            let mut moved = false;
            for p in placed.iter() {
                if (u - p.x).abs() < hw + p.z && (v - p.y).abs() < hh + p.w {
                    v = p.y + p.w + hh + 0.01;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        placed.push(Vec4::new(u, v, hw, hh));
        it.5 = upv * ((v - v0) * d);
    }
}

fn animate_words(
    time: Res<Time>,
    cams: Cams,
    budget: Res<TextBudget>,
    mut up: ResMut<FxUpload>,
    mut words: Query<(Entity, &mut OnoWordState, &mut Transform)>,
    mut letters: Query<&mut Transform, Without<OnoWordState>>,
    mut items: Local<Vec<(Entity, Vec3, Vec2, u64, bool, Vec3)>>,
    mut placed: Local<Vec<Vec4>>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    let cam = camera(&cams);
    items.clear();
    for (e, w, tf) in &words {
        if !w.shattered {
            items.push((e, tf.translation - w.stack, w.ext * w.place_scale, w.serial, w.style == OnoStyle::Performance, Vec3::ZERO));
        }
    }
    let pl = &budget.placement;
    let crosshair = (pl.crosshair_deg + pl.margin_deg).to_radians().tan();
    if let Some((cam, _)) = cam {
        resolve_overlaps(&cam, crosshair, &mut items, &mut placed);
    }
    let k = 1.0 - (-dt * 18.0).exp();
    for (e, mut w, mut tf) in &mut words {
        let target = items.iter().find(|it| it.0 == e).map(|it| it.5).unwrap_or(Vec3::ZERO);
        let st = w.stack;
        w.stack = st + (target - st) * k;
        w.age += dt;
        let age = w.age;
        let mut s = w.scale;
        // Root placement.
        if let Some((cam, view)) = cam {
            let (_, cam_rot, cam_pos) = cam.to_scale_rotation_translation();
            if w.style == OnoStyle::Performance {
                let fwd = cam.forward().as_vec3();
                let down = -cam.up().as_vec3();
                tf.translation = cam_pos + fwd * 0.9 + down * (0.11 + 0.02 * (age * 3.0).min(1.0));
                tf.rotation = cam_rot;
            } else {
                // Readability (outline pass): only a small lean along the hit — a launched victim flies
                // *along* `dir`, so a word drifting with it sat on top of them; it rises instead.
                let drift = w.dir * (0.15 * (1.0 - (-age * 5.0).exp())) + Vec3::Y * (0.5 + 0.25 * age);
                let pos = w.base + drift * s.max(0.3) * 2.0;
                let to_cam = (cam_pos - pos).normalize_or_zero();
                let pos = pos + to_cam * 0.4;
                // World-text policy: out of your lap, off the crosshair, screen-size clamped; own
                // words small and to the side (text_budget::place_word).
                let mut side = w.side;
                let placed = place_word(&view, pos, w.ext, w.local, &mut side, pl);
                w.side = side;
                let (pos, k) = match placed {
                    Some(p) => (p.pos, p.scale),
                    None => {
                        // The camera is inside it now: gone.
                        w.hold = w.hold.min((age - w.pop).max(0.0));
                        (pos, 0.0)
                    }
                };
                w.place_scale = k;
                s *= k;
                tf.translation = pos + w.stack;
                let to_cam = (cam_pos - pos).normalize_or(Vec3::Z);
                let facing = Transform::from_translation(pos).looking_to(-to_cam, Vec3::Y).rotation;
                tf.rotation = facing * Quat::from_rotation_z(w.roll);
            }
        }
        tf.scale = Vec3::splat(s.max(1e-4));
        let end = w.pop + w.hold;
        let crack = ((age - (end - 0.12)) / 0.12).clamp(0.0, 1.0);
        let n = w.letters.len().max(1) as f32;
        for (i, l) in w.letters.iter().enumerate() {
            let t = age - l.delay;
            let pop = if t <= 0.0 { 0.0 } else { ease_back(t / w.pop, 2.6) };
            let wob = (age * 17.0 + i as f32 * 1.3).sin() * (0.06 * (-age * 5.0).exp() + 0.008) * (0.5 + 0.5 * w.impact.min(1.5));
            let bounce = (age * 11.0 + i as f32 * 0.8).sin() * 0.06 * (-age * 3.0).exp();
            if let Ok(mut lt) = letters.get_mut(l.entity) {
                lt.translation = l.centre + Vec3::Y * bounce;
                lt.rotation = Quat::from_rotation_z(l.jitter_rot + wob);
                lt.scale = Vec3::splat(pop.max(0.0) * l.jitter_scale);
            }
            let color = if w.rainbow { hue(now * 1.6 + i as f32 / n) } else { w.color };
            let pal = Palette::new(color, None);
            let flash = (1.0 - t / 0.1).clamp(0.0, 1.0) * if t > 0.0 { 1.0 } else { 0.0 };
            let integrity = 1.0 - crack * 0.9;
            let mut gg = construct_gpu(&pal, integrity, 1.0, flash * 0.8, 0.0, bd_core::CONSTRUCT_VOXEL_SIZE, l.slot * 7 + 3, 1.0, false, Vec3::ZERO, 1.0);
            gg.accent[3] = 1.0;
            write_slot(&mut up, l.slot, gg);
        }
    }
}

fn finish_words(
    mut commands: Commands,
    mut fx: ResMut<FxParticles>,
    settings: Res<FxSettings>,
    glyphs: Res<GlyphCache>,
    mut rt: ResMut<OnoRuntime>,
    mut words: Query<(Entity, &mut OnoWordState, &GlobalTransform)>,
    letters: Query<&GlobalTransform>,
) {
    let rt = &mut *rt;
    for (e, mut w, _gt) in &mut words {
        if w.age < w.pop + w.hold || w.shattered {
            continue;
        }
        w.shattered = true;
        let _ = w.serial;
        let perf = w.style == OnoStyle::Performance;
        for (i, l) in w.letters.iter().enumerate() {
            let Some(g) = glyphs.glyphs.get(&l.ch) else { continue };
            let Ok(lgt) = letters.get(l.entity) else { continue };
            // Letter world transform: the entity's transform includes the mesh centring offset.
            let tf = lgt.affine() * bevy::math::Affine3A::from_translation(-g.offset);
            let color = if w.rainbow { hue(i as f32 / w.letters.len().max(1) as f32) } else { w.color };
            let pal = Palette::new(color, None);
            if perf {
                // Tiny sparkle instead of a full shatter right in front of the camera.
                let p = lgt.translation();
                for _ in 0..4 {
                    let r = &mut fx.rng;
                    let seed = r.next_u32();
                    let vel = r.dir() * 0.25;
                    fx.emit(Emit { kind: PKind::Ballistic, flags: PFlags::PLAIN, pos: p, vel, size: 0.006, color, emissive: 4.0, life: 0.25, gravity: 0.0, drag: 2.0, spin: 6.0, seed, ..default() });
                }
                continue;
            }
            let impulse = w.dir * (1.5 + 2.0 * w.impact.min(2.0)) + Vec3::Y * 0.5;
            shatter_model(&mut fx, &settings, &g.model, &tf, &[], &pal, impulse, Some(lgt.translation() - w.dir * 0.3), None, 0.5, 0.75, 0.5, &mut rt.scratch);
        }
        commands.entity(e).despawn();
    }
}
