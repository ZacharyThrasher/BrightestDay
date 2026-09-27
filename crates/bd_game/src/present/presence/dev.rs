//! Presence dev hooks (never set in play):
//!
//! * `BD_PRESENCE_STRIP=<dir>` + `BD_PRESENCE_STRIP_NAMES=sword,gun,…`: when a construct whose
//!   recipe name contains one of the names (case-insensitive) spawns, screenshot the primary window
//!   at fixed offsets after it ([`OFFSETS`]) → `<dir>/<name>_<k>.png` (the frame strips of
//!   docs/presence; once per name). Pair with `BD_SIGNATURE_TEST` casts and `BD_HIDDEN_WINDOW=1`.
//! * `BD_PRESENCE_LEGENDARY=<name>`: practice / host only — constructs with that recipe name get the
//!   legendary flair bit (the golden roll is hidden and deterministic; this lets a strip show one).

use crate::sim_bridge::{ActiveWorld, GameEvents};
use bd_sim::GameEvent;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

/// Seconds after the spawn event at which a strip frame is taken.
pub const OFFSETS: [f32; 8] = [0.0, 0.06, 0.12, 0.2, 0.35, 0.6, 1.2, 2.2];

#[derive(Default)]
pub struct StripDev {
    t: f32,
    done: Vec<String>,
    /// (at, path) pending shots.
    shots: Vec<(f32, std::path::PathBuf)>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

pub fn strips(time: Res<Time<Real>>, mut events: MessageReader<GameEvents>, mut commands: Commands, mut st: Local<StripDev>) {
    static CFG: std::sync::OnceLock<Option<(std::path::PathBuf, Vec<String>)>> = std::sync::OnceLock::new();
    let Some((dir, names)) = CFG.get_or_init(|| {
        let dir = env("BD_PRESENCE_STRIP")?;
        let names = env("BD_PRESENCE_STRIP_NAMES").unwrap_or_default().split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
        let _ = std::fs::create_dir_all(&dir);
        Some((std::path::PathBuf::from(dir), names))
    }) else {
        events.clear();
        return;
    };
    st.t += time.delta_secs();
    for ev in events.read() {
        let GameEvent::ConstructSpawned { recipe, .. } = &ev.event else { continue };
        let lname = recipe.name.to_lowercase();
        let Some(key) = names.iter().find(|n| lname.contains(n.as_str())) else { continue };
        if st.done.contains(key) {
            continue;
        }
        st.done.push(key.clone());
        info!("presence strip: {:?} ({})", recipe.name, key);
        let file = key.replace(' ', "_");
        for (k, off) in OFFSETS.iter().enumerate() {
            let at = st.t + off;
            st.shots.push((at, dir.join(format!("{file}_{k}.png"))));
        }
    }
    let now = st.t;
    st.shots.retain(|(at, path)| {
        if *at <= now {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
            false
        } else {
            true
        }
    });
}

pub fn legendary(mut events: MessageReader<GameEvents>, active: Option<ResMut<ActiveWorld>>) {
    static NAME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let Some(want) = NAME.get_or_init(|| env("BD_PRESENCE_LEGENDARY").map(|s| s.to_lowercase())) else {
        events.clear();
        return;
    };
    let Some(mut active) = active else {
        events.clear();
        return;
    };
    let ids: Vec<bd_core::ConstructId> = events
        .read()
        .filter_map(|ev| match &ev.event {
            GameEvent::ConstructSpawned { id, recipe, .. } if recipe.name.to_lowercase().contains(want.as_str()) => Some(*id),
            _ => None,
        })
        .collect();
    for id in ids {
        if let Some(c) = active.world.constructs.get_mut(id) {
            c.flair |= bd_sim::replicate::cflags::LEGENDARY;
        }
    }
}
