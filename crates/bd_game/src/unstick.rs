//! UNSTICK, the player-facing escape (the sim side is `bd_sim::world::unstick`).
//!
//! * **Asking:** the Esc menu's UNSTICK entry (`menu.rs` writes [`UnstickRequest`]) or the
//!   Controls-tab `UNSTICK` key (unbound by default) → `Cmd::Unstick` (host / practice: straight
//!   into the world; clients: `NetCmd::Unstick` to the host, which validates it).
//! * **Never says no:** the host queues a request it can't fire yet (10 s cooldown; 3 s after taking
//!   damage unless stuck). While one waits, the menu button reads `UNSTICK · 4s` and a small timer
//!   sits under the crosshair. Host / practice read the exact wait from the world; clients estimate
//!   it from their own events (the last unstick, their % going up) until the host's
//!   `GameEvent::Unstuck` arrives.
//! * **Arrival:** every `Unstuck` (the request, or the sim's embedded pop-out) gets a brief shimmer —
//!   a corps-coloured flash + glints and a soft chime at the new spot.

use bd_core::PlayerId;
use bd_fx::{BurstFx, BurstKind};
use bd_net::now_secs;
use bd_sim::GameEvent;
use bd_sim::world::unstick::{UNSTICK_COOLDOWN, UNSTICK_DAMAGE_LOCKOUT};
use bevy::prelude::*;

use crate::input::UiFocus;
use crate::lobby::prefs::{Action, KeyBinds};
use crate::present::sound::Sfx3d;
use crate::sim_bridge::{ActiveWorld, Cmd, GameEvents, SimCommand, SimRole};
use crate::state::{AppState, InGameMenu};

/// The Esc menu's UNSTICK button was pressed.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct UnstickRequest;

/// Marks the menu button's label text (its timer is written here every frame).
#[derive(Component)]
pub struct UnstickLabel;

/// The small "UNSTICK · 4s" line under the crosshair while a request waits.
#[derive(Component)]
struct UnstickHud;

/// Local view of the request (clients estimate; host / practice read the world).
#[derive(Resource, Default, Debug)]
pub struct UnstickUi {
    queued: bool,
    queued_at: f64,
    cd_until: f64,
    hurt_at: f64,
    last_pct: f32,
    me: Option<PlayerId>,
}

impl UnstickUi {
    /// Seconds until the waiting request fires (`None` = nothing waiting), and the cooldown left.
    fn wait(&self, a: &ActiveWorld, now: f64) -> (Option<f32>, f32) {
        let Some(me) = a.local else { return (None, 0.0) };
        if a.role != SimRole::Client {
            // Authoritative world: exact.
            let queued = a.world.player(me).is_some_and(|p| p.unstick.queued);
            let w = a.world.unstick_wait(me);
            let cd = a.world.player(me).map_or(0.0, |p| p.unstick.cd);
            return (queued.then_some(w), cd);
        }
        let cd = (self.cd_until - now).max(0.0) as f32;
        if !self.queued {
            return (None, cd);
        }
        let stuck = a.world.player_embedded(me);
        let lock = if stuck { 0.0 } else { (self.hurt_at + f64::from(UNSTICK_DAMAGE_LOCKOUT) - now).max(0.0) as f32 };
        (Some(cd.max(lock)), cd)
    }
}

pub struct UnstickPlugin;

impl Plugin for UnstickPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<UnstickRequest>()
            .add_message::<SimCommand>()
            .add_message::<GameEvents>()
            .add_message::<BurstFx>()
            .add_message::<Sfx3d>()
            .init_resource::<UnstickUi>()
            .add_systems(Update, (unstick_input, unstick_track, unstick_labels).chain());
    }
}

#[allow(clippy::too_many_arguments)]
fn unstick_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Option<Res<KeyBinds>>,
    state: Res<State<AppState>>,
    menu: Res<InGameMenu>,
    focus: Option<Res<UiFocus>>,
    active: Option<Res<ActiveWorld>>,
    mut asked: MessageReader<UnstickRequest>,
    mut ui: ResMut<UnstickUi>,
    mut cmds: MessageWriter<SimCommand>,
) {
    let from_menu = asked.read().count() > 0;
    let key = *state.get() == AppState::InGame
        && !menu.0
        && !focus.is_some_and(|f| f.typing)
        && binds.is_some_and(|b| b.just_pressed(Action::Unstick, &keys, &mouse));
    let alive = active.as_ref().is_some_and(|a| a.local_player().is_some_and(|p| p.alive));
    if (from_menu || key) && alive {
        cmds.write(SimCommand::local(Cmd::Unstick));
        if !ui.queued {
            ui.queued = true;
            ui.queued_at = now_secs();
        }
    }
}

fn unstick_track(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    mut ui: ResMut<UnstickUi>,
    mut burst: MessageWriter<BurstFx>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    let Some(a) = active else {
        events.clear();
        *ui = UnstickUi::default();
        return;
    };
    let now = now_secs();
    if ui.me != a.local {
        *ui = UnstickUi { me: a.local, ..default() };
    }
    // Own damage (the client-side lockout estimate).
    if let Some(p) = a.local_player() {
        if p.damage_pct > ui.last_pct + 1e-3 {
            ui.hurt_at = now;
        }
        ui.last_pct = p.damage_pct;
        if !p.alive {
            ui.queued = false;
        }
    }
    // A request the host never answered (dropped on a resync): give the button back.
    if ui.queued && now - ui.queued_at > f64::from(UNSTICK_COOLDOWN + UNSTICK_DAMAGE_LOCKOUT) + 3.0 {
        ui.queued = false;
    }
    for ev in events.read() {
        let GameEvent::Unstuck { player, to, auto, .. } = &ev.event else { continue };
        if Some(*player) == a.local && !*auto {
            ui.queued = false;
            ui.cd_until = now + f64::from(UNSTICK_COOLDOWN);
        }
        // The shimmer: a soft corps-coloured flash, glints rising through the body, a chime.
        let p = crate::present::bv(*to);
        let c = crate::present::corps_rgb(a.corps_of(*player));
        burst.write(BurstFx { kind: BurstKind::Flash, pos: p + Vec3::Y * 0.9, dir: Vec3::Y, scale: 0.7, color: Some(c) });
        burst.write(BurstFx { kind: BurstKind::Shards, pos: p + Vec3::Y * 0.4, dir: Vec3::Y, scale: 0.8, color: Some([c[0] * 0.5 + 0.5, c[1] * 0.5 + 0.5, c[2] * 0.5 + 0.5]) });
        let mine = Some(*player) == a.local;
        sfx.write(Sfx3d::at("respawn", p).gain(if mine { 0.7 } else { 0.4 }).pitch(1.25));
    }
}

fn unstick_labels(
    mut commands: Commands,
    active: Option<Res<ActiveWorld>>,
    state: Res<State<AppState>>,
    ui: Res<UnstickUi>,
    mut labels: Query<&mut Text, (With<UnstickLabel>, Without<UnstickHud>)>,
    mut hud: Query<(Entity, &mut Text), (With<UnstickHud>, Without<UnstickLabel>)>,
) {
    let now = now_secs();
    let (wait, cd) = active.as_deref().map_or((None, 0.0), |a| ui.wait(a, now));
    let label = match wait {
        Some(w) if w > 0.05 => format!("UNSTICK · {:.0}s", w.ceil()),
        Some(_) => "UNSTICK · NOW".to_string(),
        None if cd > 0.05 => format!("UNSTICK · {:.0}s", cd.ceil()),
        None => "UNSTICK".to_string(),
    };
    for mut t in &mut labels {
        if t.0 != label {
            t.0.clone_from(&label);
        }
    }
    // The waiting timer under the crosshair (only while a request waits, in game).
    let show = *state.get() == AppState::InGame && wait.is_some_and(|w| w > 0.05);
    match (show, hud.single_mut()) {
        (true, Ok((_, mut t))) => {
            if t.0 != label {
                t.0.clone_from(&label);
            }
        }
        (true, Err(_)) => {
            commands.spawn((
                UnstickHud,
                Text::new(label),
                TextFont::from_font_size(15.0),
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
                Node { position_type: PositionType::Absolute, top: percent(58.0), width: percent(100.0), justify_content: JustifyContent::Center, ..default() },
                TextLayout::justify(Justify::Center),
                Pickable::IGNORE,
            ));
        }
        (false, Ok((e, _))) => commands.entity(e).despawn(),
        (false, Err(_)) => {}
    }
}
