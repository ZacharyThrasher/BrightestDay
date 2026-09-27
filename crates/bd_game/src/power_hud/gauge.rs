//! The crosshair willpower gauge: whole SPARKS on a thin lantern-band arc hugging the crosshair from
//! below (filling left → right, the corps colour), so willpower is read without leaving the aim.
//! Reads [`super::PowerHudState`]'s [`super::will::WillGauge`] only (one source with the ring hand
//! and the strip). One `UiMaterial` node ([`GaugeMaterial`], `gauge.wgsl`); its params are written
//! only when a quantised value changes (flicker, shake, shimmer run on the shader clock).
//!
//! * Typing a cast: the exact sparks it will spend flicker light; can't afford it → the filled
//!   sparks go red and the gauge shakes, the missing sparks show as red ghost outlines (past the
//!   gauge's end when it's more than a full bar).
//! * On a spend the sparks break off and streak down-right (toward the ring hand) in < 0.3 s; a
//!   spark filling clicks in with a glint + a soft tick; a full gauge flashes once, then settles to a
//!   faint baseline while idle. Low: dims and sputters. Charging (Z): a shimmer rises along it.
//! * WILLPOWER = LAUNCH (`bd_sim::launch`, host rule): the gauge reads as LAUNCH POWER — never red /
//!   shaking (nothing is refused; the bite still shows what a cast will drain), the filled sparks run
//!   from a dim, cool corps tint (weak launches) to hot white-gold (loaded), a notch marks the ×1
//!   point, and a loaded bar breathes.
//! * `UiPrefs::crosshair_gauge` (HUD settings "CROSSHAIR GAUGE", on by default). Screen flashes off
//!   (comfort) → no flicker / shake.

use super::PowerHudState;
use crate::lobby::prefs::UiPrefs;
use crate::readability::Readability;
use crate::sim_bridge::ActiveWorld;
use crate::state::{AppState, InGameMenu};
use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub const GAUGE_SHADER: Handle<Shader> = uuid_handle!("3c8f6b1e-5a27-4d90-b0e4-7d21c9a4f5e8");

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct GaugeParams {
    pub color: Vec4,
    pub a: Vec4,
    pub b: Vec4,
    pub c: Vec4,
    pub d: Vec4,
    pub e: Vec4,
    pub f: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GaugeMaterial {
    #[uniform(0)]
    pub params: GaugeParams,
}

impl UiMaterial for GaugeMaterial {
    fn fragment_shader() -> ShaderRef {
        GAUGE_SHADER.into()
    }
}

/// Room around the arc for the break-off streak and the ghosts (px at 1×).
const MARGIN: f32 = 40.0;

#[derive(Resource)]
struct Gauge {
    node: Entity,
    mat: Handle<GaugeMaterial>,
}

pub fn build(app: &mut App) {
    load_internal_asset!(app, GAUGE_SHADER, "gauge.wgsl", Shader::from_wgsl);
    app.add_plugins(UiMaterialPlugin::<GaugeMaterial>::default())
        .add_systems(Startup, spawn)
        .add_systems(Update, draw.in_set(crate::present::PresentSet::Sync));
}

fn spawn(mut commands: Commands, mut mats: ResMut<Assets<GaugeMaterial>>) {
    let mat = mats.add(GaugeMaterial { params: GaugeParams::default() });
    // Always displayed (opacity 0 when hidden) so the pipeline compiles at startup.
    let node = commands
        .spawn((
            Name::new("CrosshairGauge"),
            Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), width: px(160), height: px(160), ..default() },
            MaterialNode(mat.clone()),
            Pickable::IGNORE,
            GlobalZIndex(39),
        ))
        .id();
    commands.insert_resource(Gauge { node, mat });
}

fn q(x: f32, steps: f32) -> f32 {
    (x * steps).round() / steps
}

#[allow(clippy::too_many_arguments)]
fn draw(
    st: Res<PowerHudState>,
    prefs: Res<UiPrefs>,
    tuning: Res<Readability>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    rig: Res<crate::camera::CameraRig>,
    comfort: Res<crate::settings::ComfortSettings>,
    active: Option<Res<ActiveWorld>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    gauge: Option<Res<Gauge>>,
    mut mats: ResMut<Assets<GaugeMaterial>>,
    mut nodes: Query<&mut Node>,
) {
    let Some(g) = gauge else { return };
    let t = &tuning.0.gauge;
    let will = &st.0.will;
    let alive = active.as_ref().and_then(|a| a.local_player()).is_some_and(|p| p.alive);
    let show = prefs.crosshair_gauge && *state.get() == AppState::InGame && !menu.0 && *rig != crate::camera::CameraRig::Director && alive && will.pool > 0;
    // Size: px at a 1080-line window, never below min_scale.
    let scale = windows.iter().next().map_or(1.0, |w| (w.height() / 1080.0).max(t.min_scale));
    let (r, th) = ((t.radius_px * scale).round(), (t.thickness_px * scale).max(2.0));
    let side = 2.0 * (r + th + MARGIN * scale).round();
    let margin = UiRect { left: px(-side * 0.5), top: px(-side * 0.5), ..default() };
    if let Ok(mut n) = nodes.get_mut(g.node)
        && (n.width != px(side) || n.margin != margin)
    {
        n.width = px(side);
        n.height = px(side);
        n.margin = margin;
    }
    let corps = active.as_ref().and_then(|a| a.local_player()).map_or([0.3, 1.0, 0.45], |p| p.corps.color());
    let col = super::derive::vivid(corps);
    let alpha = if show { q(will.alpha, 64.0) } else { 0.0 };
    let b = will.bite;
    // WILLPOWER = LAUNCH: launch power 0..1 + the ×1 notch (in sparks); never "can't afford".
    let launch = active.as_ref().filter(|a| bd_sim::launch::active(a.world.rules())).map(|a| {
        let t = &a.world.cfg.tuning.launch;
        let frac = if will.pool > 0 { will.have / will.pool as f32 } else { 0.0 };
        (q(bd_sim::launch::power01(t, frac), 64.0), t.mid_frac * will.pool as f32)
    });
    let afford = b.afford || launch.is_some();
    let p = if alpha <= 0.0 {
        GaugeParams::default()
    } else {
        GaugeParams {
            color: Vec4::new(col[0], col[1], col[2], alpha),
            a: Vec4::new(will.pool as f32, q(will.have, 256.0), q(b.lo, 256.0), q(b.hi, 256.0)),
            b: Vec4::new(b.cost, afford as u8 as f32, q(b.missing, 256.0), will.charging as u8 as f32),
            c: Vec4::new(will.low as u8 as f32, will.glint.map_or(-1.0, |(i, _)| i as f32), will.glint.map_or(0.0, |(_, a)| q(a, 64.0)), q(will.full_flash, 64.0)),
            d: will.brk.map_or(Vec4::new(0.0, 0.0, -1.0, t.ghost_max as f32), |k| Vec4::new(k.lo, k.hi, q(k.age, 64.0), t.ghost_max as f32)),
            e: Vec4::new(r, th, t.span_deg.to_radians(), t.gap_deg.to_radians()),
            f: Vec4::new((comfort.flashes <= 0.01) as u8 as f32, launch.is_some() as u8 as f32, launch.map_or(0.0, |l| l.0), launch.map_or(0.0, |l| l.1)),
        }
    };
    if mats.get(&g.mat).is_some_and(|m| m.params != p)
        && let Some(mut m) = mats.get_mut(&g.mat)
    {
        m.params = p;
    }
}
