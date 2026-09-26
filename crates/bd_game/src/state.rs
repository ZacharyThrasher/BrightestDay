//! Top-level app flow.
//!
//! * [`AppState::Loading`] pre-warms every render pipeline offscreen (see `warmup`) behind an
//!   opaque loading screen, then goes to Menu (or straight to InGame with `--practice`/`--bench`).
//! * [`AppState::Menu`]   title screen; the arena renders behind it with a slow orbit camera.
//! * [`AppState::InGame`] the player has control. [`GameMode`] says how we got here; bd_net hooks
//!   `OnEnter(AppState::InGame)` and reads `GameMode` to host / connect.
//!
//! [`InGameMenu`] is the pause overlay (Esc). The world keeps running (it is a multiplayer game).

use crate::cli::CliArgs;
use bevy::prelude::*;

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Loading,
    Menu,
    InGame,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub enum GameMode {
    #[default]
    Practice,
    /// Listen server (bd_net launches the tunnel).
    Host,
    /// Lobby code / `ip:port` typed on the Join page.
    Join(String),
}

/// True while the Esc overlay is open in-game (mouse released, input suppressed).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InGameMenu(pub bool);

/// Where to go once loading finishes.
#[derive(Resource, Debug, Clone)]
pub struct AfterLoading(pub AppState);

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<CliArgs>();
        let next = if cli.practice || cli.bench.is_some() || cli.script.is_some() || cli.map.is_some() { AppState::InGame } else { AppState::Menu };
        app.init_state::<AppState>()
            .insert_resource(AfterLoading(next))
            .init_resource::<GameMode>()
            .init_resource::<InGameMenu>()
            .add_systems(OnExit(AppState::InGame), |mut m: ResMut<InGameMenu>| m.0 = false);
    }
}

/// Run condition: the local player has control (in game, overlay closed).
pub fn gameplay_active(state: Res<State<AppState>>, menu: Res<InGameMenu>) -> bool {
    *state.get() == AppState::InGame && !menu.0
}
