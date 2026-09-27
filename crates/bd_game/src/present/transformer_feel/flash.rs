//! TRANSFORMERS V2 — the FLASH / BRANCH cue (COMBAT.md §5d-quinquies), wordless:
//!
//! * `HeldFx::Flash` with `strength` > 0 = the FLASH window opened (a string's ender / a launcher /
//!   a counter connected: Transform now = the vehicle attack); < 0 = the BRANCH window after a vehicle
//!   attack connected (LMB rush / RMB launch / Transform drift).
//! * Everyone sees a burst on the robot: gold for the flash, blue for the branch.
//! * The pilot also hears a bright chime (UI, unpositioned) and sees a small pip ring close in around
//!   the crosshair for exactly the window (it shrinks to the dot as the window runs out).

use super::super::bv;
use super::super::sound::Sfx3d;
use crate::sim_bridge::{ActiveWorld, GameEvents};
use bd_fx::{BurstFx, BurstKind};
use bd_sim::GameEvent;
use bd_sim::events::HeldFx;
use bevy::picking::Pickable;
use bevy::prelude::*;

const GOLD: [f32; 3] = [1.0, 0.78, 0.22];
const BLUE: [f32; 3] = [0.3, 0.7, 1.0];
/// The pip ring's size at the window's start (px); it closes in to the crosshair dot.
const PIP_PX: f32 = 26.0;

#[derive(Component)]
pub struct FlashPip;

/// The local pilot's live window (s left, its length, flash or branch).
#[derive(Resource, Default)]
pub struct PipState {
    left: f32,
    secs: f32,
    branch: bool,
}

pub fn spawn_pip(mut commands: Commands) {
    commands.spawn((
        Name::new("TfFlashPip"),
        FlashPip,
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(PIP_PX),
            height: px(PIP_PX),
            margin: UiRect { left: px(-PIP_PX * 0.5), top: px(-PIP_PX * 0.5), ..default() },
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(PIP_PX * 0.5)),
            ..default()
        },
        BorderColor::all(Color::NONE),
        BackgroundColor(Color::NONE),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(41),
    ));
}

pub fn events(mut events: MessageReader<GameEvents>, active: Option<Res<ActiveWorld>>, mut sfx: MessageWriter<Sfx3d>, mut bursts: MessageWriter<BurstFx>, mut pip: ResMut<PipState>) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    for ev in events.read() {
        let GameEvent::HeldFx { fx: HeldFx::Flash, owner, pos, strength, .. } = &ev.event else { continue };
        let branch = *strength < 0.0;
        let secs = strength.abs().clamp(0.05, 2.0);
        let color = if branch { BLUE } else { GOLD };
        bursts.write(BurstFx { kind: BurstKind::Flash, pos: bv(*pos), dir: Vec3::ZERO, scale: if branch { 1.0 } else { 1.4 }, color: Some(color) });
        if active.local == Some(*owner) {
            sfx.write(Sfx3d::ui("ko_streak_willpower").pitch(if branch { 1.0 } else { 1.3 }).gain(0.75));
            *pip = PipState { left: secs, secs, branch };
        }
    }
}

pub fn draw_pip(time: Res<Time<Real>>, mut pip: ResMut<PipState>, mut q: Query<(&mut Node, &mut Visibility, &mut BorderColor), With<FlashPip>>) {
    pip.left = (pip.left - time.delta_secs()).max(0.0);
    let Ok((mut node, mut vis, mut border)) = q.single_mut() else { return };
    if pip.left <= 0.0 {
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        return;
    }
    let u = pip.left / pip.secs.max(1e-3); // 1 → 0 over the window
    let size = 8.0 + (PIP_PX - 8.0) * u;
    node.width = px(size);
    node.height = px(size);
    node.margin = UiRect { left: px(-size * 0.5), top: px(-size * 0.5), ..default() };
    node.border_radius = BorderRadius::all(px(size * 0.5));
    let c = if pip.branch { BLUE } else { GOLD };
    *border = BorderColor::all(Color::srgba(c[0], c[1], c[2], 0.55 + 0.45 * u));
    *vis = Visibility::Visible;
}
