//! The minimal HUD strip (bottom centre): power icons, status pips, the carried pickup and the slim
//! willpower arc — one `UiMaterial` ([`IconMaterial`], `power_icon.wgsl`). Every node is spawned
//! once; a frame only flips `Display` / writes a material when its quantised value changed (no
//! per-frame allocation, like the scoreboard).

use super::derive::{self, PowerHud, SLOTS};
use super::PowerHudState;
use crate::lobby::prefs::UiPrefs;
use crate::sim_bridge::ActiveWorld;
use crate::state::{AppState, InGameMenu};
use bd_sim::readiness::HUD_TIMERS;
use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub const ICON_SHADER: Handle<Shader> = uuid_handle!("7e1c2a55-9b0d-4d3e-a1f6-3c0b9d6e2a41");

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct IconParams {
    /// rgb (linear) + opacity.
    pub color: Vec4,
    /// glyph, fill, form (-1 = none), flash.
    pub a: Vec4,
    /// mode (0 power, 1 status, 2 arc, 3 pickup), active, dim, extra (arc: cost).
    pub b: Vec4,
    /// form colour + time seed.
    pub c: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct IconMaterial {
    #[uniform(0)]
    pub params: IconParams,
}

impl UiMaterial for IconMaterial {
    fn fragment_shader() -> ShaderRef {
        ICON_SHADER.into()
    }
}

/// The row holding the power icons (borrowed-move chips / chain pips append to it: `borrowed.rs`).
#[derive(Component)]
pub struct StripRow;

const ICON: f32 = 34.0;
const ICON_ACTIVE: f32 = 42.0;
const PIP: f32 = 20.0;
const PICKUP: f32 = 30.0;

#[derive(Resource)]
pub(crate) struct Strip {
    root: Entity,
    /// Icon entities (`labels::tooltip` hit-tests these to find a hovered held power under Tab).
    pub(crate) icons: [(Entity, Handle<IconMaterial>); SLOTS],
    pips: [(Entity, Handle<IconMaterial>); HUD_TIMERS],
    /// The carried-pickup icon (also hit-tested for its hover tooltip).
    pub(crate) pickup: (Entity, Handle<IconMaterial>),
    arc: (Entity, Handle<IconMaterial>),
}

pub fn build(app: &mut App) {
    load_internal_asset!(app, ICON_SHADER, "power_icon.wgsl", Shader::from_wgsl);
    app.add_plugins(UiMaterialPlugin::<IconMaterial>::default())
        .add_systems(Startup, spawn_strip)
        .add_systems(Update, draw_strip.in_set(crate::present::PresentSet::Sync));
}

fn node(size: f32) -> Node {
    Node { width: px(size), height: px(size), flex_shrink: 0.0, ..default() }
}

fn spawn_strip(mut commands: Commands, mut mats: ResMut<Assets<IconMaterial>>) {
    let mut mk = |mode: f32, seed: f32| mats.add(IconMaterial { params: IconParams { b: Vec4::new(mode, 0.0, 0.0, 0.0), c: Vec4::new(1.0, 1.0, 1.0, seed), ..default() } });
    let icon_m: [Handle<IconMaterial>; SLOTS] = std::array::from_fn(|k| mk(0.0, k as f32 * 1.7));
    let pip_m: [Handle<IconMaterial>; HUD_TIMERS] = std::array::from_fn(|k| mk(1.0, k as f32 * 0.9));
    let pickup_m = mk(3.0, 0.0);
    // The arc stays displayed (alpha 0 when hidden) so the pipeline is compiled at startup, never
    // on the first power of a match.
    let arc_m = mk(2.0, 0.0);
    let root = commands
        .spawn((
            Name::new("PowerHudStrip"),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(14),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(2),
                ..default()
            },
            UiTransform::default(),
            Pickable::IGNORE,
            GlobalZIndex(35),
        ))
        .id();
    let row = commands.spawn((StripRow, Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), ..default() }, Pickable::IGNORE)).id();
    let pips_row = commands.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(2), ..default() }, Pickable::IGNORE)).id();
    let pips = std::array::from_fn(|k| {
        let e = commands.spawn((Node { display: Display::None, ..node(PIP) }, MaterialNode(pip_m[k].clone()), Pickable::IGNORE)).id();
        (e, pip_m[k].clone())
    });
    let icons = std::array::from_fn(|k| {
        let e = commands.spawn((Node { display: Display::None, ..node(ICON) }, MaterialNode(icon_m[k].clone()), Pickable::IGNORE)).id();
        (e, icon_m[k].clone())
    });
    let pickup = commands.spawn((Node { display: Display::None, ..node(PICKUP) }, MaterialNode(pickup_m.clone()), Pickable::IGNORE)).id();
    let arc = commands.spawn((Node { width: px(170), height: px(16), ..default() }, MaterialNode(arc_m.clone()), Pickable::IGNORE)).id();
    commands.entity(pips_row).add_children(&pips.iter().map(|p: &(Entity, Handle<IconMaterial>)| p.0).collect::<Vec<_>>());
    commands.entity(row).add_child(pips_row);
    for (e, _) in &icons {
        commands.entity(row).add_child(*e);
    }
    commands.entity(row).add_child(pickup);
    commands.entity(root).add_children(&[row, arc]);
    commands.insert_resource(Strip { root, icons, pips, pickup: (pickup, pickup_m), arc: (arc, arc_m) });
}

fn lin(c: [f32; 3], a: f32) -> Vec4 {
    Vec4::new(c[0], c[1], c[2], a)
}

/// Quantise so a sweep writes its material ~96 times per revolution, not every frame.
fn q(x: f32) -> f32 {
    (x * 96.0).round() / 96.0
}

fn set(mats: &mut Assets<IconMaterial>, h: &Handle<IconMaterial>, p: IconParams) {
    if mats.get(h).is_some_and(|m| m.params != p)
        && let Some(mut m) = mats.get_mut(h)
    {
        m.params = p;
    }
}

fn set_display(nodes: &mut Query<&mut Node>, e: Entity, on: bool, size: Option<f32>) {
    let Ok(mut n) = nodes.get_mut(e) else { return };
    let d = if on { Display::Flex } else { Display::None };
    if n.display != d {
        n.display = d;
    }
    if let Some(s) = size
        && n.width != px(s)
    {
        n.width = px(s);
        n.height = px(s);
    }
}

fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

/// Should the strip show at all right now?
pub fn strip_visible(hud: &PowerHud, prefs: &UiPrefs, in_game: bool, menu: bool, director: bool, alive: bool) -> bool {
    prefs.power_hud && in_game && !menu && !director && alive && (hud.count > 0 || hud.pickup.is_some() || hud.any_timer())
}

#[allow(clippy::too_many_arguments)]
fn draw_strip(
    st: Res<PowerHudState>,
    prefs: Res<UiPrefs>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    rig: Res<crate::camera::CameraRig>,
    active: Option<Res<ActiveWorld>>,
    strip: Option<Res<Strip>>,
    mut mats: ResMut<Assets<IconMaterial>>,
    mut nodes: Query<&mut Node>,
    mut tf: Query<&mut UiTransform>,
) {
    let Some(s) = strip else { return };
    let hud = &st.0;
    let alive = active.as_ref().and_then(|a| a.local_player()).is_some_and(|p| p.alive);
    let show = strip_visible(hud, &prefs, *state.get() == AppState::InGame, menu.0, *rig == crate::camera::CameraRig::Director, alive);
    // Lift above the optional damage % number.
    if let Ok(mut n) = nodes.get_mut(s.root) {
        let b = px(if prefs.show_percent { 44 } else { 14 });
        if n.bottom != b {
            n.bottom = b;
        }
    }
    if let Ok(mut t) = tf.get_mut(s.root) {
        let k = if hud.pop > 0.0 { 0.55 + 0.45 * ease_out_back(1.0 - hud.pop) } else { 1.0 };
        let want = Vec2::splat(k);
        if t.scale != want {
            t.scale = want;
        }
    }
    let powers = show && hud.count > 0;
    // Icons.
    for (k, (e, h)) in s.icons.iter().enumerate() {
        let g = &hud.gems[k];
        let on = powers && g.power.is_some();
        set_display(&mut nodes, *e, on, Some(if g.active { ICON_ACTIVE } else { ICON }));
        if !on {
            continue;
        }
        let p = IconParams {
            color: lin(g.look.color, 1.0),
            a: Vec4::new(g.look.glyph as f32, q(g.shown), g.form.map_or(-1.0, q), q(g.flash)),
            b: Vec4::new(0.0, g.active as u8 as f32, hud.drained as u8 as f32, 0.0),
            c: lin(g.look.form_color, k as f32 * 1.7),
        };
        set(&mut mats, h, p);
    }
    // Status pips.
    for (k, (e, h)) in s.pips.iter().enumerate() {
        let on = show && hud.timers[k] > 0.0;
        set_display(&mut nodes, *e, on, None);
        if !on {
            continue;
        }
        let (col, glyph) = derive::status_look(k, hud.grown);
        let p = IconParams { color: lin(col, 1.0), a: Vec4::new(glyph as f32, q(hud.timer_frac[k]), -1.0, 0.0), b: Vec4::new(1.0, 0.0, 0.0, 0.0), c: Vec4::new(1.0, 1.0, 1.0, k as f32 * 0.9) };
        set(&mut mats, h, p);
    }
    // Carried pickup.
    let pk = hud.pickup.filter(|_| show);
    set_display(&mut nodes, s.pickup.0, pk.is_some(), None);
    if let Some((kind, life)) = pk {
        let (col, glyph) = derive::pickup_look(kind);
        let p = IconParams { color: lin(col, 1.0), a: Vec4::new(glyph as f32, q(life), -1.0, 0.0), b: Vec4::new(3.0, 0.0, 0.0, 0.0), c: Vec4::new(1.0, 1.0, 1.0, 0.0) };
        set(&mut mats, &s.pickup.1, p);
    }
    // Willpower arc (the ring's colour: corps), in whole sparks. Only when the crosshair gauge is
    // off: that gauge carries willpower where the eyes already are, so this would be a duplicate.
    let corps = active.as_ref().and_then(|a| a.local_player()).map_or([0.3, 1.0, 0.45], |p| p.corps.color());
    let arc_on = show && (hud.count > 0 || hud.pickup.is_some()) && !prefs.crosshair_gauge;
    let p = IconParams {
        color: lin(derive::vivid(corps), if arc_on { 0.9 } else { 0.0 }),
        a: Vec4::new(0.0, q(hud.willpower), -1.0, 0.0),
        b: Vec4::new(2.0, 0.0, 0.0, hud.cost.map_or(0.0, q)),
        c: Vec4::new(hud.will.pool as f32, 0.0, 0.0, 0.0),
    };
    set(&mut mats, &s.arc.1, p);
}
