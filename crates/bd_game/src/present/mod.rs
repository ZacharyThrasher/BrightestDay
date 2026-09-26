//! Presentation of the sim (SIM engineer): everything the player sees and hears of
//! `bd_sim::World`, driven only by [`crate::sim_bridge::GameEvents`] + [`crate::sim_bridge::WorldViews`]
//! (+ the replicated snapshot in `ViewKeys`), so it works identically in practice, host and client.
//!
//! | module | what |
//! |---|---|
//! | [`sound`] | positional one-shots from the SFX manifest (pitch randomised), per-entity loops, mix duck, KO streak steps |
//! | [`terrain`] | render-mirror terrain edits (carve / extract / restore / arena reset) + debris with velocity inheritance + falling chunks |
//! | [`impact`] | hits → `Impact` juice, render-side hit-stop, crit snap, crosshair flare, KO shatter + streak + accents, wall splat, onomatopoeia, explosions |
//! | [`constructs`] | every construct as a bd_fx `ConstructVisual` (models built off-thread, cached by `ShapeKey`), anim state from the sim, forming / shatter / clash / creature telegraphs / held swings / transforms / loops |
//! | [`players`] | other players as costumes (bd_shapes) in corps colour, run/slide/tether/swing/cast/tumble, tether ropes, diegetic health |
//! | [`powers`] | fruits, rubber arm, afterimages, time-stop grey, ROOM dome, portals, links (chains), fields |
//! | [`guns`] | muzzle light, tracers/beams, casings, projectiles, bullet impacts |
//! | [`camera3p`] | over-the-shoulder rig (piloting / comfort), crosshair ray + projectile aim correction |
//! | [`local`] | local player: respawn facing, death cam, hop chain feedback |
//! | [`water`] | city-map sea: animated water plane + splashes |
//! | [`soak`] / [`record`] | `--soak-view` chaos viewer with auto-director, `--record` PNG frame dump |

pub mod camera3p;
pub mod constructs;
pub mod guns;
pub mod impact;
pub mod local;
pub mod players;
pub mod powers;
pub mod record;
pub mod soak;
pub mod sound;
pub mod terrain;
pub mod water;

use crate::sim_bridge::{ActiveWorld, BridgeSet, SimRole};
use crate::tick::FrameSet;
use bd_core::corps::Corps;
use bd_core::PlayerId;
use bevy::prelude::*;

/// Ordering inside `FrameSet::Present` for presentation systems.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PresentSet {
    /// Read `GameEvents` → messages / state (runs first).
    Events,
    /// Sync entities to `WorldViews` (transforms, anim layers).
    Sync,
}

pub struct PresentPlugin;

impl Plugin for PresentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bd_fx::BdFxPlugin)
            .configure_sets(Update, (PresentSet::Events, PresentSet::Sync).chain().in_set(FrameSet::Present).after(BridgeSet::Views))
            .configure_sets(Update, PresentSet::Events.before(crate::debris::DebrisSimSet))
            .add_plugins((
                sound::SoundPlugin,
                terrain::TerrainPlugin,
                impact::ImpactPlugin,
                constructs::ConstructsPlugin,
                players::PlayersPlugin,
                powers::PowersPlugin,
                guns::GunsPlugin,
                camera3p::Camera3pPlugin,
                local::LocalPlugin,
                soak::SoakPlugin,
                record::RecordPlugin,
            ))
            .add_plugins(water::WaterPlugin);
        use crate::warmup::WarmupAppExt;
        app.add_warmup(|w: &mut World| {
            for e in bd_fx::spawn_warmup(w) {
                w.entity_mut(e).insert(crate::warmup::WarmupItem);
            }
        });
    }
}

/// Linear corps colour.
pub fn corps_rgb(c: Corps) -> [f32; 3] {
    c.color()
}

pub fn rgb_color(c: [f32; 3]) -> Color {
    Color::linear_rgb(c[0], c[1], c[2])
}

/// Client role: the local player's own cosmetic moments were already played from local input.
pub fn replayed_locally(active: &ActiveWorld, owner: PlayerId) -> bool {
    active.role == SimRole::Client && active.local == Some(owner)
}

#[inline]
pub fn bv(v: bd_core::Vec3) -> Vec3 {
    Vec3::from_array(v.to_array())
}

#[inline]
pub fn bq(q: bd_core::Quat) -> Quat {
    Quat::from_array(q.to_array())
}

#[inline]
pub fn gv(v: Vec3) -> bd_core::Vec3 {
    bd_core::Vec3::from_array(v.to_array())
}
