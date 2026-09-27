//! Themed city maps built from the [`crate::city`] kit.
//!
//! A map is a layout function returning the voxels plus a [`MapInfo`]: everything in [`ArenaInfo`]
//! (spawns, bounds, void, sky/sun/fog palette — so the sim and client can use it unchanged via
//! `info.arena` or `Deref`) plus what a city needs on top: a water plane (falling in is a KO; the
//! client renders the surface and splashes), extra sky/lighting colours, named districts and the
//! authored movement routes between them (used by bots, the camera director and the route tests).

#[cfg(feature = "backrooms")]
pub mod backrooms;
#[cfg(feature = "casino")]
pub mod casino;
#[cfg(feature = "cybertron")]
pub mod cybertron;
#[cfg(feature = "ember")]
pub mod ember;
#[cfg(feature = "giant")]
pub mod giant;
#[cfg(feature = "hyperline")]
pub mod hyperline;
#[cfg(feature = "lair")]
pub mod lair;
#[cfg(feature = "lookout")]
pub mod lookout;
pub mod miami;
#[cfg(feature = "mirror")]
pub mod mirror;
pub mod orbit;
#[cfg(feature = "sakura")]
pub mod sakura;
#[cfg(feature = "valley")]
pub mod valley;

use std::ops::Deref;

use glam::Vec3;

use crate::arena::ArenaInfo;

#[cfg(feature = "backrooms")]
pub use backrooms::{BACKROOMS_SIZE_VOXELS, generate_backrooms};
#[cfg(feature = "casino")]
pub use casino::{CASINO_SIZE_VOXELS, generate_casino};
#[cfg(feature = "cybertron")]
pub use cybertron::{CYBERTRON_SIZE_VOXELS, generate_cybertron};
#[cfg(feature = "ember")]
pub use ember::{EMBER_SIZE_VOXELS, generate_ember};
#[cfg(feature = "giant")]
pub use giant::{GIANT_SIZE_VOXELS, generate_giant};
#[cfg(feature = "hyperline")]
pub use hyperline::{HYPERLINE_SIZE_VOXELS, generate_hyperline};
#[cfg(feature = "lookout")]
pub use lookout::{LOOKOUT_SIZE_VOXELS, generate_lookout};
pub use miami::{MIAMI_SIZE_VOXELS, generate_miami};
#[cfg(feature = "mirror")]
pub use mirror::{MIRROR_SIZE_VOXELS, generate_mirror};
pub use orbit::{ORBIT_SIZE_VOXELS, generate_orbit};
#[cfg(feature = "sakura")]
pub use sakura::{SAKURA_SIZE_VOXELS, generate_sakura};
#[cfg(feature = "valley")]
pub use valley::{VALLEY_SIZE_VOXELS, generate_valley};

/// A named area of the map (for spawn logic, bots, the camera and UI callouts).
#[derive(Clone, Debug, PartialEq)]
pub struct District {
    pub name: &'static str,
    /// A representative ground point.
    pub center: Vec3,
    /// Horizontal extent (min x, min z, max x, max z) in metres.
    pub area: [f32; 4],
}

/// How a route is meant to be travelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    /// Ground: run / bunny-hop / slide.
    Ground,
    /// Tether swings (anchors listed).
    Swing,
    /// Mixed.
    Mixed,
}

/// An authored traversal line between districts (feet-height waypoints + suggested anchors).
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub name: &'static str,
    pub from: &'static str,
    pub to: &'static str,
    pub kind: RouteKind,
    pub waypoints: Vec<Vec3>,
    pub anchors: Vec<Vec3>,
}

/// Everything about a city map besides its voxels.
#[derive(Clone, Debug, PartialEq)]
pub struct MapInfo {
    pub name: &'static str,
    /// Spawns, bounds, void height (= just under the water surface), landmark centre, sun and sky.
    pub arena: ArenaInfo,
    /// Sea level. Anything below is water: the client draws the surface (and splashes); feet below
    /// `arena.void_y` (slightly under this) is an out-of-bounds KO.
    pub water_y: f32,
    /// Water colours (sRGB 0–1): shallow/surface tint and deep.
    pub water_color: [f32; 3],
    pub water_deep: [f32; 3],
    /// Sun disc / key-light colour (sRGB 0–1) and a mid-sky band colour for a three-stop gradient
    /// (zenith → `sky_mid` → horizon).
    pub sun_color: [f32; 3],
    pub sky_mid: [f32; 3],
    /// Fog start/end distances (m).
    pub fog_start: f32,
    pub fog_end: f32,
    /// Emissive (neon) boost the client may apply at this time of day.
    pub neon_boost: f32,
    pub districts: Vec<District>,
    pub routes: Vec<Route>,
    /// Generation diagnostic: stray voxels removed because they weren't connected to anything
    /// indestructible (should stay tiny; a big number means a kit piece came out detached).
    pub pruned_voxels: usize,
}

impl Deref for MapInfo {
    type Target = ArenaInfo;
    fn deref(&self) -> &ArenaInfo {
        &self.arena
    }
}

impl MapInfo {
    pub fn district(&self, name: &str) -> Option<&District> {
        self.districts.iter().find(|d| d.name == name)
    }
    pub fn route(&self, name: &str) -> Option<&Route> {
        self.routes.iter().find(|r| r.name == name)
    }
}
