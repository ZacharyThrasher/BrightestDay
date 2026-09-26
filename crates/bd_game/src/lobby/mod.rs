//! Player-facing UI (UI engineer): one [`UiPlugin`] registers the ring console, voice, oath,
//! viewmodel, persisted prefs/keybinds and the lobby screens (host/join, match settings + presets,
//! map picker, costume creator, loadout, party pick/coin/podium, scoreboard).
//!
//! Ordering inside `Update`: [`UiSet::Modal`] (oath) → [`UiSet::Input`] (console, quick-cast,
//! power/transform/dismiss keys, voice) → the shell's `FrameSet::Input` (so the TYPING bit and
//! consumed keys apply to this frame's tick input) … → [`UiSet::Draw`] after the bridge's views.

pub mod costume;
pub mod feed; // FREEDOM: kill feed, highlights data, "it" / names / again planner
pub mod party;
pub mod prefs;
pub mod preview;
pub mod scoreboard;
pub mod uitest;

use crate::sim_bridge::BridgeSet;
use crate::tick::FrameSet;
use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum UiSet {
    /// Modal typing overlays (oath) — first, so they own the keyboard.
    Modal,
    /// Console / gameplay keys owned by the UI.
    Input,
    /// Presentation of UI state (after the frame's ticks and views).
    Draw,
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, (UiSet::Modal, UiSet::Input).chain().before(FrameSet::Input))
            .configure_sets(Update, UiSet::Draw.after(BridgeSet::Views))
            .add_plugins((
                prefs::PrefsPlugin,
                crate::console::ConsolePlugin,
                crate::oath::OathPlugin,
                crate::voice_ui::VoicePlugin,
                crate::viewmodel::ViewmodelPlugin,
                party::PartyUiPlugin,
                scoreboard::ScoreboardPlugin,
                uitest::UiTestPlugin,
            ));
    }
}
