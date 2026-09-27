//! Over-head damage % — optional, never clutter. A small number above an enemy's head in its heat
//! colour, sized by distance, hidden beyond [`super::NumberTuning::max_dist`] and behind terrain.
//! `UiPrefs::enemy_percent`: **NEAR CROSSHAIR** (default: only the enemy you're aiming at / right
//! next to the crosshair, so an 8-player brawl shows at most one or two), **ALL** (every enemy in
//! range), **OFF**. One pooled text node per player slot; the text is rewritten only when the whole
//! number changes (no per-frame allocation).

use std::fmt::Write as _;

use bevy::picking::Pickable;
use bevy::prelude::*;

use super::{Readability, heat_color};
use crate::camera::PlayerCamera;
use crate::lobby::prefs::{EnemyPercent, UiPrefs};
use crate::sim_bridge::{ActiveWorld, WorldViews};
use crate::state::{AppState, InGameMenu};

/// Pooled labels (one per possible player).
const POOL: usize = bd_core::MAX_PLAYERS;

#[derive(Component)]
struct EnemyPct {
    slot: usize,
    /// Shown whole %, player id, fade 0..1.
    shown: Option<(i32, u8)>,
    fade: f32,
}

pub fn build(app: &mut App) {
    app.add_systems(Startup, spawn).add_systems(PostUpdate, draw.after(bevy::transform::TransformSystems::Propagate));
}

fn spawn(mut commands: Commands) {
    for slot in 0..POOL {
        commands.spawn((
            Name::new("EnemyDamagePct"),
            EnemyPct { slot, shown: None, fade: 0.0 },
            Text::new(""),
            TextFont::from_font_size(18.0),
            TextColor(Color::WHITE),
            TextShadow { offset: Vec2::new(0.0, 1.5), color: Color::srgba(0.0, 0.0, 0.0, 0.75) },
            TextLayout::justify(Justify::Center),
            Node { position_type: PositionType::Absolute, width: px(90), ..default() },
            Visibility::Hidden,
            Pickable::IGNORE,
            GlobalZIndex(12),
        ));
    }
}

/// Is `to` (from the camera) within `deg` of the view direction?
pub fn near_crosshair(fwd: Vec3, to: Vec3, deg: f32) -> bool {
    let l = to.length();
    l > 1e-3 && fwd.dot(to / l) >= deg.to_radians().cos()
}

/// Font size at `dist` metres (px at a 1080-line window).
pub fn font_at(t: &super::NumberTuning, dist: f32) -> f32 {
    let k = ((dist - t.near_m) / (t.max_dist - t.near_m).max(1.0)).clamp(0.0, 1.0);
    t.font_near + (t.font_far - t.font_near) * k
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    time: Res<Time<Real>>,
    tuning: Res<Readability>,
    prefs: Res<UiPrefs>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    active: Option<Res<ActiveWorld>>,
    views: Option<Res<WorldViews>>,
    cams: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut labels: Query<(&mut EnemyPct, &mut Text, &mut TextColor, &mut TextFont, &mut Node, &mut Visibility)>,
) {
    let dt = time.delta_secs().min(0.1);
    let t = &tuning.0.numbers;
    let heat = &tuning.0.heat;
    let on = prefs.enemy_percent != EnemyPercent::Off && *state.get() == AppState::InGame && !menu.0;
    let cam = cams.iter().find(|(c, _)| c.is_active);
    let scale = windows.iter().next().map_or(1.0, |w| (w.height() / 1080.0).max(0.75));
    let players = views.as_ref().map_or(&[][..], |v| &v.players[..]);
    for (mut l, mut text, mut color, mut font, mut node, mut vis) in &mut labels {
        // Which player this slot shows (slot k = the k-th other player), and where.
        let mut target = None;
        if on && let (Some((camera, gt)), Some(a)) = (cam, active.as_ref()) {
            let eye = gt.translation();
            let fwd = gt.forward().as_vec3();
            if let Some(v) = players.iter().filter(|v| !v.is_local).nth(l.slot).filter(|v| v.alive()) {
                let head = v.pos + Vec3::Y * (1.9 * v.size.max(0.3) + t.lift);
                let chest = v.pos + Vec3::Y * 1.1 * v.size.max(0.3);
                let to = chest - eye;
                let dist = to.length();
                let wanted = dist <= t.max_dist && (prefs.enemy_percent == EnemyPercent::All || near_crosshair(fwd, to, t.near_deg));
                // Line of sight to the head (no numbers through walls).
                let seen = wanted && {
                    let d = head - eye;
                    let l = d.length().max(1e-3);
                    bd_core::voxel::raycast(&a.world.voxels, crate::util::core_v3(eye), crate::util::core_v3(d / l), l - 0.4).is_none()
                };
                if seen && let Ok(p) = camera.world_to_viewport(gt, head) {
                    target = Some((v.id.0, v.damage_pct, p, dist));
                }
            }
        }
        let want = if target.is_some() { 1.0 } else { 0.0 };
        l.fade = (l.fade + (want - l.fade).signum() * t.fade_per_sec * dt).clamp(0.0, 1.0);
        let show = l.fade > 0.01 && target.is_some();
        let v = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != v {
            *vis = v;
        }
        let Some((id, pct, p, dist)) = target.filter(|_| show) else {
            if !show {
                l.shown = None;
            }
            continue;
        };
        let whole = pct.max(0.0).round().min(999.0) as i32;
        if l.shown != Some((whole, id)) {
            l.shown = Some((whole, id));
            text.0.clear();
            let _ = write!(text.0, "{whole}%");
        }
        let c = heat_color(heat, pct).with_alpha(0.95 * l.fade);
        if color.0 != c {
            color.0 = c;
        }
        let size = (font_at(t, dist) * scale).round();
        if font.font_size != FontSize::Px(size) {
            font.font_size = FontSize::Px(size);
        }
        let (left, top) = (px(p.x - 45.0), px(p.y - size));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_crosshair_cone_and_font_by_distance() {
        let f = Vec3::NEG_Z;
        assert!(near_crosshair(f, Vec3::new(0.0, 0.0, -10.0), 7.0));
        assert!(near_crosshair(f, Vec3::new(1.0, 0.0, -10.0), 7.0)); // ~5.7°
        assert!(!near_crosshair(f, Vec3::new(2.0, 0.0, -10.0), 7.0)); // ~11°
        assert!(!near_crosshair(f, Vec3::new(0.0, 0.0, 10.0), 7.0));
        assert!(!near_crosshair(f, Vec3::ZERO, 7.0));
        let t = super::super::NumberTuning::default();
        assert_eq!(font_at(&t, 1.0), t.font_near);
        assert_eq!(font_at(&t, 100.0), t.font_far);
        assert!(font_at(&t, 20.0) < t.font_near && font_at(&t, 20.0) > t.font_far);
    }
}
