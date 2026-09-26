//! Player outlines — "where is everyone, who are they" in the busiest brawl.
//!
//! Every player body (and the hard light they hold / wear) gets two inverted hulls from
//! `bd_fx::outline` as children of its part entities: a thin dark line hugging the silhouette and a
//! corps-coloured line outside it. Hard-light constructs never get one, so a body reads apart from
//! any glow of the same colour (own constructs, an enemy of the same corps, kaiju). Constant pixel
//! width at any distance, depth-tested (line of sight only — through-wall reveals belong to
//! `tracking`), below the bloom threshold (never smears).
//!
//! * Damage %: from [`DANGER_FROM`] % the line widens and brightens and breathes slowly
//!   (≤ [`PULSE_HZ_MAX`] Hz, far from the 3–30 Hz photosensitive band; static with flashes off) —
//!   "who's about to go" at a glance.
//! * The local body in third person gets a subtle one; the first-person held item none.
//! * Settings (`UiPrefs`): `outlines` on/off, `outline_width` (×), `outline_patterns` (colour-blind
//!   aid: per-corps dash rhythm + brightness).
//! * Only the gameplay camera draws hulls ([`OUTLINE_LAYER`]); the summon cut-in window does not.
//! * `BD_OUTLINE=0` forces them off (A/B screenshots and perf).

use super::camera3p::ViewMode;
use super::constructs::SimConstruct;
use super::players::PlayerBody;
use crate::camera::PlayerCamera;
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, WorldViews};
use bd_core::PlayerId;
use bd_core::corps::Corps;
use bd_core::mechanics::FormCategory;
use bd_fx::outline::{MAX_OUTLINE_SLOTS, OutlineAssets, OutlineGpu, OutlineLayer, OutlineUpload};
use bd_fx::{ConstructEnd, ConstructParts, ConstructVisual, FxSet};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// Render layer of the hulls (the gameplay camera sees it; the summon cut-in camera does not).
pub const OUTLINE_LAYER: usize = 24;

/// Damage % where the danger read starts / is full.
pub const DANGER_FROM: f32 = 60.0;
pub const DANGER_FULL: f32 = 150.0;
/// Slowest / fastest breathing of the danger pulse.
pub const PULSE_HZ_MIN: f32 = 0.7;
pub const PULSE_HZ_MAX: f32 = 1.3;

/// Line widths (px at 1× width): outer edge from the silhouette, inner dark line.
const OUTER_PX: f32 = 3.0;
const INNER_PX: f32 = 1.25;
/// Brightest channel of the corps line (linear): under the bloom knee (threshold 1, softness 0.35).
const LINE_LUMA: f32 = 0.62;

/// What the viewer chose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutlineStyle {
    /// Width multiplier (0.5 … 2).
    pub width: f32,
    /// Colour-blind aid: per-corps dash pattern + brightness step.
    pub patterns: bool,
    /// Danger breathing (off ⇒ static brighten; comfort / photosensitivity).
    pub pulse: bool,
}

impl Default for OutlineStyle {
    fn default() -> Self {
        Self { width: 1.0, patterns: false, pulse: true }
    }
}

/// 0 below [`DANGER_FROM`] % … 1 at [`DANGER_FULL`] % (smooth).
pub fn danger(damage_pct: f32) -> f32 {
    let t = ((damage_pct - DANGER_FROM) / (DANGER_FULL - DANGER_FROM)).clamp(0.0, 1.0);
    if t.is_finite() { t * t * (3.0 - 2.0 * t) } else { 0.0 }
}

pub fn pulse_hz(danger: f32) -> f32 {
    PULSE_HZ_MIN + (PULSE_HZ_MAX - PULSE_HZ_MIN) * danger.clamp(0.0, 1.0)
}

/// Corps line colour: hue kept, lifted a touch toward white, brightest channel = [`LINE_LUMA`].
pub fn line_color(corps: [f32; 3]) -> [f32; 3] {
    let m = corps[0].max(corps[1]).max(corps[2]).max(1e-3);
    corps.map(|c| ((c / m) * 0.88 + 0.12) * LINE_LUMA)
}

/// Colour-blind dash rhythm per corps: (period px, lit fraction, angle rad, brightness).
fn pattern(corps_index: u8) -> ([f32; 3], f32) {
    const D: f32 = std::f32::consts::FRAC_PI_4;
    match corps_index % 8 {
        0 => ([0.0, 1.0, 0.0], 1.0),
        1 => ([7.0, 0.5, D], 0.8),
        2 => ([12.0, 0.72, -D], 1.0),
        3 => ([4.0, 0.5, D], 0.8),
        4 => ([14.0, 0.4, -D], 1.0),
        5 => ([9.0, 0.78, D], 0.8),
        6 => ([5.0, 0.3, -D], 1.0),
        _ => ([16.0, 0.85, D], 0.8),
    }
}

/// One player's hull params. `pulse01` is the breathing phase value (0..1); `own` = the viewer's
/// own body in third person (subtle).
pub fn outline_params(corps: Corps, damage_pct: f32, pulse01: f32, own: bool, style: OutlineStyle) -> OutlineGpu {
    let d = danger(damage_pct);
    let breath = if style.pulse { pulse01.clamp(0.0, 1.0) } else { 0.6 };
    let w = if style.width.is_finite() { style.width.clamp(0.5, 2.0) } else { 1.0 };
    let mut col = line_color(corps.color());
    let mut gain = 1.0 + 0.45 * d * breath;
    let mut outer = (OUTER_PX + 1.0 * d + 0.6 * d * breath) * w;
    let mut inner = (INNER_PX * w).max(0.9);
    let mut pat = [0.0, 1.0, 0.0];
    if style.patterns {
        let (p, b) = pattern(corps.index());
        pat = p;
        gain *= b;
    }
    if own {
        outer *= 0.55;
        inner *= 0.8;
        gain *= 0.6;
        pat = [0.0, 1.0, 0.0];
    }
    col = col.map(|c| c * gain);
    let dark = corps.color().map(|c| c * 0.035);
    OutlineGpu {
        outer: [col[0], col[1], col[2], outer.max(inner + 0.5)],
        inner: [dark[0], dark[1], dark[2], inner],
        pattern: [pat[0], pat[1], pat[2], 0.45],
    }
}

// ------------------------------------------------------------------------------------ plugin

/// Hulls spawned under a root's part entities.
#[derive(Component)]
struct OutlineHulls {
    owner: PlayerId,
    hulls: Vec<Entity>,
    shown: bool,
}

/// The resolved viewer setting (mirrored from `UiPrefs` + comfort each frame).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct OutlineSettings {
    pub enabled: bool,
    pub style: OutlineStyle,
}

impl Default for OutlineSettings {
    fn default() -> Self {
        Self { enabled: true, style: OutlineStyle::default() }
    }
}

fn forced_off() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("BD_OUTLINE").is_ok_and(|v| v == "0" || v.eq_ignore_ascii_case("off")))
}

pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bd_fx::outline::OutlinePlugin).init_resource::<OutlineSettings>().add_systems(
            PostUpdate,
            (camera_layer, mirror_settings, attach_hulls, update_outlines)
                .chain()
                .after(FxSet::Lifecycle)
                .before(TransformSystems::Propagate),
        );
        use crate::warmup::WarmupAppExt;
        app.add_warmup(warmup_hull);
        if dev_test().is_some() {
            app.add_systems(Update, dev_views.after(crate::sim_bridge::BridgeSet::Views).before(super::PresentSet::Events)).add_systems(
                PostUpdate,
                dev_camera.after(crate::camera::CameraUpdateSet).after(super::shot_camera).before(TransformSystems::Propagate),
            );
        }
    }
}

/// The gameplay camera also sees [`OUTLINE_LAYER`].
fn camera_layer(mut commands: Commands, cams: Query<Entity, Added<PlayerCamera>>) {
    for e in &cams {
        commands.entity(e).insert(RenderLayers::from_layers(&[0, OUTLINE_LAYER]));
    }
}

fn mirror_settings(
    prefs: Option<Res<crate::lobby::prefs::UiPrefs>>,
    comfort: Option<Res<ComfortSettings>>,
    mut s: ResMut<OutlineSettings>,
) {
    let mut want = OutlineSettings::default();
    if let Some(p) = prefs.as_ref() {
        want.enabled = p.outlines;
        want.style.width = p.outline_width;
        want.style.patterns = p.outline_patterns;
    }
    if let Some(c) = comfort.as_ref() {
        want.style.pulse = c.flashes > 0.0;
    }
    if forced_off() {
        want.enabled = false;
    }
    if *s != want {
        *s = want;
    }
}

/// Held / worn hard light rides the silhouette: it is outlined with its owner.
fn outlined_construct(sc: &SimConstruct) -> bool {
    matches!(sc.category, FormCategory::Held | FormCategory::Worn)
}

type RootItem<'a> = (Entity, &'a ConstructParts, Option<&'a PlayerBody>, Option<&'a SimConstruct>, Option<&'a OutlineHulls>);
type RootFilter = (Or<(With<PlayerBody>, With<SimConstruct>)>, Or<(Changed<ConstructParts>, Without<OutlineHulls>)>);
type HullRoot<'a> = (&'a mut OutlineHulls, Option<&'a SimConstruct>, Option<&'a ConstructVisual>, Has<ConstructEnd>);

fn attach_hulls(
    mut commands: Commands,
    settings: Res<OutlineSettings>,
    assets: Res<OutlineAssets>,
    meshes: Query<&Mesh3d>,
    roots: Query<RootItem, RootFilter>,
    all: Query<(Entity, &OutlineHulls)>,
) {
    if !settings.enabled {
        for (e, h) in &all {
            for hull in &h.hulls {
                commands.entity(*hull).try_despawn();
            }
            commands.entity(e).try_remove::<OutlineHulls>();
        }
        return;
    }
    for (e, parts, body, sc, old) in &roots {
        let owner = match (body, sc) {
            (Some(b), _) => b.id,
            (None, Some(sc)) if outlined_construct(sc) => sc.owner,
            _ => continue,
        };
        if let Some(old) = old {
            for hull in &old.hulls {
                commands.entity(*hull).try_despawn();
            }
        }
        // Part entities not materialised yet: retry next frame.
        if parts.parts.iter().any(|(_, pe)| !meshes.contains(*pe)) {
            if old.is_some() {
                commands.entity(e).try_remove::<OutlineHulls>();
            }
            continue;
        }
        let slot = owner.0 as u32;
        let mut hulls = Vec::with_capacity(parts.parts.len() * 2);
        for (_, pe) in &parts.parts {
            let Ok(mesh) = meshes.get(*pe) else { continue };
            for layer in [OutlineLayer::Inner, OutlineLayer::Outer] {
                hulls.push(
                    commands.spawn((assets.hull(mesh.0.clone(), slot, layer), RenderLayers::layer(OUTLINE_LAYER), ChildOf(*pe))).id(),
                );
            }
        }
        commands.entity(e).try_insert(OutlineHulls { owner, hulls, shown: true });
    }
}

#[allow(clippy::too_many_arguments)]
fn update_outlines(
    time: Res<Time>,
    settings: Res<OutlineSettings>,
    active: Option<Res<ActiveWorld>>,
    views: Option<Res<WorldViews>>,
    mode: Res<ViewMode>,
    mut up: ResMut<OutlineUpload>,
    mut phase: Local<Vec<f32>>,
    mut roots: Query<HullRoot>,
    mut vis: Query<&mut Visibility>,
) {
    let (Some(active), Some(views)) = (active, views) else { return };
    if !settings.enabled {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let third = mode.third_person();
    if phase.len() < MAX_OUTLINE_SLOTS {
        phase.resize(MAX_OUTLINE_SLOTS, 0.0);
    }
    for v in &views.players {
        let i = v.id.0 as usize;
        let d = danger(v.damage_pct);
        let ph = &mut phase[i];
        *ph = (*ph + dt * pulse_hz(d)).fract();
        let pulse01 = 0.5 - 0.5 * (*ph * std::f32::consts::TAU).cos();
        let own = v.is_local && !dev_test().is_some_and(|t| t.full_self);
        up.set(v.id.0 as u32, outline_params(v.corps, v.damage_pct, pulse01, own, settings.style));
    }
    for (mut h, sc, cv, ending) in &mut roots {
        let show = match sc {
            None => true,
            Some(sc) => {
                let local = active.local == Some(h.owner);
                let formed = cv.is_some_and(|c| c.forming >= 1.0);
                let attached = views.construct(sc.id).is_some_and(|c| c.attached());
                formed && !ending && attached && !(local && !third)
            }
        };
        if show == h.shown {
            continue;
        }
        h.shown = show;
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        for e in &h.hulls {
            if let Ok(mut v) = vis.get_mut(*e) {
                *v = want;
            }
        }
    }
}

// ------------------------------------------------------------------------------------ dev hook

/// `BD_OUTLINE_TEST=pct:140,self,cam:60,yaw:30,h:2` — look checks / screenshots (never set in play):
/// `pct` forces every player's shown damage % (render side only), `self` gives the local body the
/// full outline, `cam` pins the gameplay camera `cam` metres from the local player (`yaw` degrees
/// around it, `h` metres above its feet).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct DevTest {
    pct: Option<f32>,
    full_self: bool,
    cam: Option<f32>,
    yaw: f32,
    h: f32,
}

fn dev_test() -> Option<DevTest> {
    static V: std::sync::OnceLock<Option<DevTest>> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        let s = std::env::var("BD_OUTLINE_TEST").ok()?;
        let mut t = DevTest { h: 1.6, ..default() };
        for kv in s.split(',') {
            let (k, v) = kv.split_once(':').unwrap_or((kv, ""));
            let f = v.trim().parse::<f32>().ok();
            match k.trim() {
                "pct" => t.pct = f,
                "self" => t.full_self = true,
                "cam" => t.cam = f,
                "yaw" => t.yaw = f.unwrap_or(0.0),
                "h" => t.h = f.unwrap_or(1.6),
                _ => {}
            }
        }
        Some(t)
    })
}

fn dev_views(mut views: ResMut<WorldViews>) {
    let Some(t) = dev_test() else { return };
    for v in views.players.iter_mut() {
        if let Some(p) = t.pct {
            v.damage_pct = p;
        }
    }
}

fn dev_camera(active: Option<Res<ActiveWorld>>, views: Res<WorldViews>, mut cams: Query<&mut Transform, With<PlayerCamera>>) {
    let Some(DevTest { cam: Some(dist), yaw, h, .. }) = dev_test() else { return };
    let Some(p) = active.and_then(|a| a.local).and_then(|id| views.player(id)).map(|v| v.pos) else { return };
    let dir = Quat::from_rotation_y(yaw.to_radians()) * Vec3::Z;
    let target = p + Vec3::Y * 1.0;
    for mut tf in &mut cams {
        *tf = Transform::from_translation(p + dir * dist + Vec3::Y * h).looking_at(target, Vec3::Y);
    }
}

/// Loading: one hull with the construct part vertex layout so the pipeline compiles up front.
fn warmup_hull(world: &mut World) {
    let Some(assets) = world.get_resource::<OutlineAssets>().cloned() else { return };
    let p = vec![[-0.5f32, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]];
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0f32, 1.0]; 4])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3, 0, 2, 1, 0, 3, 2]));
    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
    world.resource_mut::<OutlineUpload>().set(0, outline_params(Corps::Willpower, 0.0, 0.0, false, OutlineStyle::default()));
    for layer in [OutlineLayer::Inner, OutlineLayer::Outer] {
        world.spawn((
            Name::new("outline warmup"),
            assets.hull(mesh.clone(), 0, layer),
            RenderLayers::layer(OUTLINE_LAYER),
            crate::warmup::WarmupItem,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn danger_ramps_smoothly() {
        assert_eq!(danger(0.0), 0.0);
        assert_eq!(danger(DANGER_FROM), 0.0);
        assert_eq!(danger(DANGER_FULL), 1.0);
        assert_eq!(danger(999.0), 1.0);
        assert_eq!(danger(f32::NAN), 0.0);
        let mut last = 0.0;
        for p in 0..200 {
            let d = danger(p as f32);
            assert!(d >= last);
            last = d;
        }
    }

    #[test]
    fn pulse_is_comfort_safe() {
        // Far below the 3 Hz photosensitivity floor at any damage.
        for p in [0.0, 80.0, 150.0, 400.0] {
            let hz = pulse_hz(danger(p));
            assert!((PULSE_HZ_MIN..=PULSE_HZ_MAX).contains(&hz) && hz < 3.0);
        }
    }

    #[test]
    fn line_stays_under_bloom_knee_and_keeps_hue() {
        for c in Corps::ALL {
            let l = line_color(c.color());
            let m = l[0].max(l[1]).max(l[2]);
            assert!((m - LINE_LUMA).abs() < 1e-4, "{c:?} {l:?}");
            // Hottest (max danger, peak breath) stays near the knee (≤ 0.65 × 1.45).
            let g = outline_params(c, 999.0, 1.0, false, OutlineStyle::default());
            assert!(g.outer[0].max(g.outer[1]).max(g.outer[2]) < 0.95);
            // Inner line is dark.
            assert!(g.inner[0].max(g.inner[1]).max(g.inner[2]) < 0.05);
        }
        // Hue order kept (Rage stays red-dominant).
        let r = line_color(Corps::Rage.color());
        assert!(r[0] > r[1] && r[0] > r[2]);
    }

    #[test]
    fn danger_widens_and_brightens_own_is_subtle() {
        let s = OutlineStyle::default();
        let calm = outline_params(Corps::Hope, 0.0, 1.0, false, s);
        let hot = outline_params(Corps::Hope, 160.0, 1.0, false, s);
        assert!(hot.outer[3] > calm.outer[3]);
        assert!(hot.outer[2] > calm.outer[2]);
        // No pulse when flashes are off: breath phase does not matter.
        let still = OutlineStyle { pulse: false, ..s };
        assert_eq!(outline_params(Corps::Hope, 160.0, 0.0, false, still), outline_params(Corps::Hope, 160.0, 1.0, false, still));
        let own = outline_params(Corps::Hope, 0.0, 1.0, true, s);
        assert!(own.outer[3] < calm.outer[3] && own.outer[2] < calm.outer[2]);
        // The outer edge always clears the inner line.
        for w in [0.5, 1.0, 2.0] {
            let g = outline_params(Corps::Life, 0.0, 0.0, true, OutlineStyle { width: w, ..s });
            assert!(g.outer[3] > g.inner[3]);
        }
    }

    #[test]
    fn patterns_distinguish_every_corps() {
        let s = OutlineStyle { patterns: true, ..OutlineStyle::default() };
        let mut seen = Vec::new();
        for c in Corps::ALL {
            let g = outline_params(c, 0.0, 0.0, false, s);
            let key = (g.pattern[0].to_bits(), g.pattern[1].to_bits());
            assert!(!seen.contains(&key), "{c:?} repeats a pattern");
            seen.push(key);
        }
        // Off by default.
        assert_eq!(outline_params(Corps::Rage, 0.0, 0.0, false, OutlineStyle::default()).pattern[0], 0.0);
    }
}
