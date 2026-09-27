//! **Legendary move rules** — generic behaviours keyed off mechanic / element / form COMBINATIONS
//! (owner: "legendary weapons should feel legendary — mjolnir, cap's shield, buster sword, blades of
//! chaos…"). Nothing here knows a weapon's name: Mjolnir is a `Hammer [Blunt, Returning, Heavy] /
//! Electric`, so it recalls, flies and calls lightning; "a flaming returning hammer" calls meteors;
//! "an electric returning axe" is Stormbreaker without the icon. The rules (docs/COMBAT.md §5l):
//!
//! | rule | combination | control |
//! |---|---|---|
//! | **Recall** | any held melee item with `Returning` | RMB throws (shields: charged LMB); a thrown item that hits terrain **lodges**; tap RMB while it is out = **recall** (it flies back fast, hitting everything on the path; the catch snaps); hold RMB = **recall flight** (Thor: it leads along your aim, you follow — predicted) |
//! | **Call the storm** | held melee, element ≠ Light, and `Returning` or `Heavy` | hold LMB past `storm_hold`: the item is raised (crackle); release = the element called down at the aim after a telegraph: Electric lightning (+ arcs), Fire meteor rain, Ice icicle spears, Wind tornado, Water geyser, Psychic crushing field, Earth spikes, Shadow maw, Atomic nova, Poison miasma |
//! | **Ricochet chain** | `Returning` + `Ricochet` (thrown) | a hit re-aims at the next enemy in sight (≤ `chain_max`), then returns |
//! | **Deflect** | `Guard`, the Sword's parry, or `Blade` + `Fast` | Blade + Fast swings open a `deflect_window` at swing start (cooldown): projectiles and bolts go back **toward your aim** (bent onto the enemy nearest the crosshair); hitscan rounds go back at the shooter, damage capped; melee attackers are staggered |
//! | **Blade wave** | `Blade` (or Sword / Axe) melee | a heavy released at ≥ `wave_min_k` fires a crescent that pierces players and carves terrain lightly; `Blade` + `Shoots` fires small waves on every swing; `Heavy` blades fire bigger, slower waves |
//! | **Heavy blade** | `Blade` + `Heavy` | combos with a heavy finisher; **super armor** (knockback × `armor_kb`, never immunity) while charging / heavy swings / leaping; hold LMB ≥ `leap_hold` and release = **leap and cleave** (Braver: up and forward, dive, crater line + shockwave + a ground wave — predicted) |
//! | **Chain blades** | `Whip` + `Blade` (+ `Pull`) | the combo finisher is a 360° sweep at whip range; with `Pull`, RMB at an enemy yanks you in (and them a little toward you) |
//!
//! Everything is deterministic (pure functions of the construct, its owner's view, voxels and
//! [`LegendTuning`]); per-construct state is [`LegendState`] inside `HeldState` (fixed size, no heap).
//! The wielder's own movement (recall flight, the leap) runs in [`pre_move`] on the host and in client
//! prediction alike, from input-driven counters on [`LegendMoves`] (`PlayerState::extra.legend`); the
//! item side mirrors the same counters so both agree. Visuals ride `HeldFx` events
//! (`bd_game::present::legend`); waves are simulated by clients from their launch event.

use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, ModelSource, Recipe};
use bd_core::voxel::{VoxelQuery, raycast, world_to_voxel};
use bd_core::{DT, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use super::held::{self, HeldState, Mode, Swing};
use super::{AnimState, Construct, ConstructOut, ConstructStore, Env, player_key};
use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo, HitKind};
use crate::events::{GameEvent, HeldFx};
use crate::geom;
use crate::movement::PlayerState;
use crate::rng::SimRng;
use crate::status::StatusKind;

// ------------------------------------------------------------------------------------ tuning

/// `assets/tuning/combat.ron` → `legendary` (hot reload; host-synced — prediction uses it).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegendTuning {
    // ---- recall ----
    /// Recall flight speed back to the hand (m/s).
    pub recall_speed: f32,
    /// A thrown Returning item lodged in terrain comes back by itself after this (s).
    pub lodge_secs: f32,
    /// RMB held this long (from the throw / the recall press) = recall flight (s).
    pub thor_delay: f32,
    /// Recall flight: willpower drain (/s) (the wielder follows at `held::THOR_SPEED`).
    pub thor_drain: f32,
    // ---- call the storm ----
    /// LMB held this long (s) raises a storm-capable item; release calls the storm.
    pub storm_hold: f32,
    /// Farthest aim point (m); telegraph before the strike (s); cooldown (s).
    pub storm_range: f32,
    pub storm_delay: f32,
    pub storm_cd: f32,
    /// Strike radius (m), damage × the form's damage, knockback (m/s), connect impact.
    pub storm_radius: f32,
    pub storm_dmg: f32,
    pub storm_kb: f32,
    pub storm_impact: f32,
    /// Damage to each enemy construct in a strike.
    pub storm_construct_dmg: f32,
    /// Lightning: arcs to at most this many more enemies within this range (half damage).
    pub bolt_arcs: u8,
    pub bolt_arc_range: f32,
    /// Meteor / icicle rain: drops, scatter radius (m), time between drops (s), damage × per drop.
    pub rain_count: u8,
    pub rain_spread: f32,
    pub rain_interval: f32,
    pub rain_dmg: f32,
    /// Tornado / maw / miasma: seconds it lingers, pull (m/s²), damage tick (s), damage × per tick.
    pub linger_secs: f32,
    pub linger_pull: f32,
    pub linger_tick: f32,
    pub linger_dmg: f32,
    // ---- ricochet chain ----
    pub chain_max: u8,
    pub chain_range: f32,
    pub chain_speed: f32,
    // ---- deflect ----
    /// Blade + Fast swings: deflect window at swing start (s) and its cooldown (s).
    pub deflect_window: f32,
    pub deflect_cd: f32,
    /// Parried projectiles fly back along the parrier's aim (else straight back at the shooter).
    pub reflect_to_aim: bool,
    /// A parried hitscan round's damage / knockback cap.
    pub reflect_hitscan_cap: f32,
    // ---- blade wave ----
    /// Heavy fraction (0.5 tap … 1 full) a blade heavy needs to fire a wave.
    pub wave_min_k: f32,
    pub wave_speed: f32,
    pub wave_range: f32,
    /// Crescent half-width and thickness (m) at size 1.
    pub wave_half_width: f32,
    pub wave_thick: f32,
    /// Damage × the form's damage, knockback (m/s), connect impact, cooldown (s).
    pub wave_dmg: f32,
    pub wave_kb: f32,
    pub wave_impact: f32,
    pub wave_cd: f32,
    /// Terrain: carve radius (m, × size), metres between carves, most carves before it breaks.
    pub wave_carve: f32,
    pub wave_carve_step: f32,
    pub wave_carves: u8,
    pub wave_construct_dmg: f32,
    /// Heavy blades: wave size ×, speed ×.
    pub heavy_wave_size: f32,
    pub heavy_wave_speed: f32,
    /// Blade + Shoots: every swing fires a small wave (size ×, damage ×, cooldown s).
    pub shoot_wave_size: f32,
    pub shoot_wave_dmg: f32,
    pub shoot_wave_cd: f32,
    // ---- heavy blade ----
    /// Knockback taken while armored (charging, heavy swings, finisher, leap).
    pub armor_kb: f32,
    /// Leap: LMB hold (s), up / forward speed (m/s), rise before the dive (s), dive speed (m/s),
    /// longest leap (s), cooldown (s).
    pub leap_hold: f32,
    pub leap_up: f32,
    pub leap_fwd: f32,
    pub leap_rise: f32,
    pub leap_dive: f32,
    pub leap_max: f32,
    pub leap_cd: f32,
    /// Landing cleave: radius (m), damage ×, knockback (m/s), impact, crater radius (m).
    pub leap_radius: f32,
    pub leap_dmg: f32,
    pub leap_kb: f32,
    pub leap_impact: f32,
    pub leap_crater: f32,
    /// Heavy-blade / chain-blade combos: knockback + per combo step.
    pub combo_kb_step: f32,
    // ---- chain blades ----
    pub sweep_radius: f32,
    pub sweep_height: f32,
    pub sweep_dmg: f32,
    pub sweep_kb: f32,
    pub sweep_impact: f32,
    pub pull_range: f32,
    pub pull_cone_deg: f32,
    pub pull_speed: f32,
    pub pull_victim: f32,
    pub pull_cd: f32,
}

impl Default for LegendTuning {
    fn default() -> Self {
        Self {
            recall_speed: 52.0,
            lodge_secs: 3.0,
            thor_delay: 0.15,
            thor_drain: held::THOR_DRAIN,
            storm_hold: 1.0,
            storm_range: 70.0,
            storm_delay: 0.5,
            storm_cd: 3.5,
            storm_radius: 3.5,
            storm_dmg: 1.6,
            storm_kb: 26.0,
            storm_impact: 2.4,
            storm_construct_dmg: 40.0,
            bolt_arcs: 2,
            bolt_arc_range: 7.0,
            rain_count: 5,
            rain_spread: 4.0,
            rain_interval: 0.11,
            rain_dmg: 0.7,
            linger_secs: 1.6,
            linger_pull: 16.0,
            linger_tick: 0.35,
            linger_dmg: 0.25,
            chain_max: 3,
            chain_range: 24.0,
            chain_speed: 42.0,
            deflect_window: 0.15,
            deflect_cd: 0.6,
            reflect_to_aim: true,
            reflect_hitscan_cap: 25.0,
            wave_min_k: 0.85,
            wave_speed: 48.0,
            wave_range: 36.0,
            wave_half_width: 1.3,
            wave_thick: 0.45,
            wave_dmg: 1.0,
            wave_kb: 16.0,
            wave_impact: 1.6,
            wave_cd: 0.6,
            wave_carve: 0.6,
            wave_carve_step: 2.0,
            wave_carves: 8,
            wave_construct_dmg: 25.0,
            heavy_wave_size: 1.7,
            heavy_wave_speed: 0.7,
            shoot_wave_size: 0.6,
            shoot_wave_dmg: 0.45,
            shoot_wave_cd: 0.45,
            armor_kb: 0.45,
            leap_hold: 0.45,
            leap_up: 13.0,
            leap_fwd: 10.0,
            leap_rise: 0.38,
            leap_dive: 32.0,
            leap_max: 2.5,
            leap_cd: 1.4,
            leap_radius: 5.5,
            leap_dmg: 1.8,
            leap_kb: 26.0,
            leap_impact: 2.6,
            leap_crater: 1.9,
            combo_kb_step: 0.2,
            sweep_radius: 9.0,
            sweep_height: 2.5,
            sweep_dmg: 1.2,
            sweep_kb: 15.0,
            sweep_impact: 1.7,
            pull_range: 15.0,
            pull_cone_deg: 14.0,
            pull_speed: 26.0,
            pull_victim: 9.0,
            pull_cd: 0.9,
        }
    }
}

impl LegendTuning {
    pub(crate) fn validated(mut self) -> Self {
        let d = Self::default();
        let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.recall_speed = fin(self.recall_speed, d.recall_speed, 5.0, 200.0);
        self.lodge_secs = fin(self.lodge_secs, d.lodge_secs, 0.0, 30.0);
        self.thor_delay = fin(self.thor_delay, d.thor_delay, 0.0, 2.0);
        self.thor_drain = fin(self.thor_drain, d.thor_drain, 0.0, 100.0);
        self.storm_hold = fin(self.storm_hold, d.storm_hold, 0.2, 5.0);
        self.storm_range = fin(self.storm_range, d.storm_range, 5.0, 300.0);
        self.storm_delay = fin(self.storm_delay, d.storm_delay, 0.0, 3.0);
        self.storm_cd = fin(self.storm_cd, d.storm_cd, 0.0, 60.0);
        self.storm_radius = fin(self.storm_radius, d.storm_radius, 0.5, 20.0);
        self.storm_dmg = fin(self.storm_dmg, d.storm_dmg, 0.0, 10.0);
        self.storm_kb = fin(self.storm_kb, d.storm_kb, 0.0, 120.0);
        self.storm_impact = fin(self.storm_impact, d.storm_impact, 0.0, 5.0);
        self.storm_construct_dmg = fin(self.storm_construct_dmg, d.storm_construct_dmg, 0.0, 1000.0);
        self.bolt_arcs = self.bolt_arcs.min(8);
        self.bolt_arc_range = fin(self.bolt_arc_range, d.bolt_arc_range, 0.0, 40.0);
        self.rain_count = self.rain_count.clamp(1, MAX_DROPS as u8);
        self.rain_spread = fin(self.rain_spread, d.rain_spread, 0.0, 20.0);
        self.rain_interval = fin(self.rain_interval, d.rain_interval, 0.0, 1.0);
        self.rain_dmg = fin(self.rain_dmg, d.rain_dmg, 0.0, 10.0);
        self.linger_secs = fin(self.linger_secs, d.linger_secs, 0.1, 6.0);
        self.linger_pull = fin(self.linger_pull, d.linger_pull, 0.0, 80.0);
        self.linger_tick = fin(self.linger_tick, d.linger_tick, 0.05, 2.0);
        self.linger_dmg = fin(self.linger_dmg, d.linger_dmg, 0.0, 5.0);
        self.chain_max = self.chain_max.min(8);
        self.chain_range = fin(self.chain_range, d.chain_range, 0.0, 80.0);
        self.chain_speed = fin(self.chain_speed, d.chain_speed, 5.0, 200.0);
        self.deflect_window = fin(self.deflect_window, d.deflect_window, 0.0, 1.0);
        self.deflect_cd = fin(self.deflect_cd, d.deflect_cd, 0.0, 10.0);
        self.reflect_hitscan_cap = fin(self.reflect_hitscan_cap, d.reflect_hitscan_cap, 0.0, 500.0);
        self.wave_min_k = fin(self.wave_min_k, d.wave_min_k, 0.5, 1.01);
        self.wave_speed = fin(self.wave_speed, d.wave_speed, 5.0, 300.0);
        self.wave_range = fin(self.wave_range, d.wave_range, 1.0, 150.0);
        self.wave_half_width = fin(self.wave_half_width, d.wave_half_width, 0.2, 8.0);
        self.wave_thick = fin(self.wave_thick, d.wave_thick, 0.05, 3.0);
        self.wave_dmg = fin(self.wave_dmg, d.wave_dmg, 0.0, 10.0);
        self.wave_kb = fin(self.wave_kb, d.wave_kb, 0.0, 120.0);
        self.wave_impact = fin(self.wave_impact, d.wave_impact, 0.0, 5.0);
        self.wave_cd = fin(self.wave_cd, d.wave_cd, 0.0, 10.0);
        self.wave_carve = fin(self.wave_carve, d.wave_carve, 0.0, 3.0);
        self.wave_carve_step = fin(self.wave_carve_step, d.wave_carve_step, 0.3, 20.0);
        self.wave_carves = self.wave_carves.min(32);
        self.wave_construct_dmg = fin(self.wave_construct_dmg, d.wave_construct_dmg, 0.0, 1000.0);
        self.heavy_wave_size = fin(self.heavy_wave_size, d.heavy_wave_size, 0.3, 4.0);
        self.heavy_wave_speed = fin(self.heavy_wave_speed, d.heavy_wave_speed, 0.1, 3.0);
        self.shoot_wave_size = fin(self.shoot_wave_size, d.shoot_wave_size, 0.1, 3.0);
        self.shoot_wave_dmg = fin(self.shoot_wave_dmg, d.shoot_wave_dmg, 0.0, 5.0);
        self.shoot_wave_cd = fin(self.shoot_wave_cd, d.shoot_wave_cd, 0.0, 10.0);
        self.armor_kb = fin(self.armor_kb, d.armor_kb, 0.0, 1.0);
        self.leap_hold = fin(self.leap_hold, d.leap_hold, 0.15, 5.0);
        self.leap_up = fin(self.leap_up, d.leap_up, 0.0, 60.0);
        self.leap_fwd = fin(self.leap_fwd, d.leap_fwd, 0.0, 60.0);
        self.leap_rise = fin(self.leap_rise, d.leap_rise, 0.0, 3.0);
        self.leap_dive = fin(self.leap_dive, d.leap_dive, 0.0, 120.0);
        self.leap_max = fin(self.leap_max, d.leap_max, 0.2, 10.0);
        self.leap_cd = fin(self.leap_cd, d.leap_cd, 0.0, 30.0);
        self.leap_radius = fin(self.leap_radius, d.leap_radius, 0.5, 20.0);
        self.leap_dmg = fin(self.leap_dmg, d.leap_dmg, 0.0, 10.0);
        self.leap_kb = fin(self.leap_kb, d.leap_kb, 0.0, 120.0);
        self.leap_impact = fin(self.leap_impact, d.leap_impact, 0.0, 5.0);
        self.leap_crater = fin(self.leap_crater, d.leap_crater, 0.0, 5.0);
        self.combo_kb_step = fin(self.combo_kb_step, d.combo_kb_step, 0.0, 2.0);
        self.sweep_radius = fin(self.sweep_radius, d.sweep_radius, 1.0, 30.0);
        self.sweep_height = fin(self.sweep_height, d.sweep_height, 0.5, 10.0);
        self.sweep_dmg = fin(self.sweep_dmg, d.sweep_dmg, 0.0, 10.0);
        self.sweep_kb = fin(self.sweep_kb, d.sweep_kb, 0.0, 120.0);
        self.sweep_impact = fin(self.sweep_impact, d.sweep_impact, 0.0, 5.0);
        self.pull_range = fin(self.pull_range, d.pull_range, 1.0, 60.0);
        self.pull_cone_deg = fin(self.pull_cone_deg, d.pull_cone_deg, 1.0, 60.0);
        self.pull_speed = fin(self.pull_speed, d.pull_speed, 0.0, 80.0);
        self.pull_victim = fin(self.pull_victim, d.pull_victim, 0.0, 60.0);
        self.pull_cd = fin(self.pull_cd, d.pull_cd, 0.05, 10.0);
        self
    }
}

// ------------------------------------------------------------------------------------ kits

/// Most drops in one rain.
pub const MAX_DROPS: usize = 8;
/// Live crescents per item.
pub const MAX_WAVES: usize = 2;

/// A held melee item (not a gun, not a telescoping pole, not a suit).
#[inline]
pub fn melee_item(c: &Construct) -> bool {
    c.category == FormCategory::Held && !c.suit && !super::gun::is_gun(c) && !super::extend::is_extending(c)
}

fn guards(c: &Construct) -> bool {
    c.recipe.form == Form::Shield || c.has(Mechanic::Guard)
}

/// RECALL: a Returning melee item.
#[inline]
pub fn recalls(c: &Construct) -> bool {
    melee_item(c) && c.has(Mechanic::Returning)
}

/// RECALL FLIGHT: a Returning item thrown with RMB (not a guard: shields block on RMB).
#[inline]
pub fn thor_item(c: &Construct) -> bool {
    recalls(c) && !guards(c)
}

/// HEAVY BLADE: Blade + Heavy.
#[inline]
pub fn heavy_blade(c: &Construct) -> bool {
    melee_item(c) && held::is_blade(c) && c.has(Mechanic::Heavy)
}

/// CHAIN BLADES: Whip + Blade.
#[inline]
pub fn chain_blade(c: &Construct) -> bool {
    melee_item(c) && c.recipe.form == Form::Whip && c.has(Mechanic::Blade)
}

/// CHAIN PULL: chain blades that also Pull.
#[inline]
pub fn chain_pulls(c: &Construct) -> bool {
    chain_blade(c) && c.has(Mechanic::Pull)
}

/// DEFLECT on swings: Blade + Fast.
#[inline]
pub fn swing_deflects(c: &Construct) -> bool {
    melee_item(c) && held::is_blade(c) && c.has(Mechanic::Fast)
}

/// BLADE WAVE: bladed melee (whips sweep instead).
#[inline]
pub fn waves(c: &Construct) -> bool {
    melee_item(c) && held::is_blade(c) && c.recipe.form != Form::Whip
}

/// Heavy blades and chain blades chain a combo whatever their form.
#[inline]
pub fn combos(c: &Construct) -> bool {
    heavy_blade(c) || chain_blade(c)
}

/// What an element calls down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StormKind {
    /// Electric: a lightning bolt that arcs on.
    Bolt = 1,
    /// Fire: meteor rain (explosions).
    Meteors = 2,
    /// Ice: icicle spears.
    Icicles = 3,
    /// Wind: a tornado that pulls in and flings up.
    Tornado = 4,
    /// Water: a geyser that launches straight up.
    Geyser = 5,
    /// Psychic: a crushing field (slammed down, staggered).
    Crush = 6,
    /// Earth: rock spikes erupt (crater + knock-up).
    Spikes = 7,
    /// Shadow: a maw that drags in and chews.
    Maw = 8,
    /// Atomic: a nova (big blast and crater).
    Nova = 9,
    /// Poison: a lingering miasma.
    Miasma = 10,
}

impl StormKind {
    pub fn of(e: Element) -> Option<StormKind> {
        Some(match e {
            Element::Light => return None,
            Element::Electric => StormKind::Bolt,
            Element::Fire => StormKind::Meteors,
            Element::Ice => StormKind::Icicles,
            Element::Wind => StormKind::Tornado,
            Element::Water => StormKind::Geyser,
            Element::Psychic => StormKind::Crush,
            Element::Earth => StormKind::Spikes,
            Element::Shadow => StormKind::Maw,
            Element::Atomic => StormKind::Nova,
            Element::Poison => StormKind::Miasma,
        })
    }

    pub fn from_u8(v: u8) -> Option<StormKind> {
        use StormKind::*;
        Some(match v {
            1 => Bolt,
            2 => Meteors,
            3 => Icicles,
            4 => Tornado,
            5 => Geyser,
            6 => Crush,
            7 => Spikes,
            8 => Maw,
            9 => Nova,
            10 => Miasma,
            _ => return None,
        })
    }

    /// Rains a volley of drops.
    pub fn rains(self) -> bool {
        matches!(self, StormKind::Meteors | StormKind::Icicles)
    }

    /// Lingers for `linger_secs` (tornado, maw, miasma).
    pub fn lingers(self) -> bool {
        matches!(self, StormKind::Tornado | StormKind::Maw | StormKind::Miasma)
    }
}

/// CALL THE STORM: a melee item with a callable element that Returns or is Heavy.
#[inline]
pub fn storms(c: &Construct) -> bool {
    melee_item(c) && StormKind::of(c.recipe.element).is_some() && (c.has(Mechanic::Returning) || c.has(Mechanic::Heavy))
}

/// The charge never auto-releases (the raise / the leap are held moves).
#[inline]
pub fn long_charge(c: &Construct) -> bool {
    heavy_blade(c) || storms(c)
}

/// A curated famous held weapon (an icon model — Mjolnir, the Buster Sword, the golden gun): its
/// presence reads legendary on clients. Data, not a name list.
pub fn famous(r: &Recipe) -> bool {
    r.form.category() == FormCategory::Held && matches!(&r.model, ModelSource::Icon(id) if !id.starts_with("costume:"))
}

/// Does a release after holding LMB `hold` s leap (vs raising the storm / a plain heavy)?
#[inline]
pub fn leap_release(c: &Construct, hold: f32, t: &LegendTuning) -> bool {
    heavy_blade(c) && hold >= t.leap_hold && !(storms(c) && hold >= t.storm_hold)
}

// ------------------------------------------------------------------------------------ state

/// A called storm in progress.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Storm {
    /// `StormKind as u8` (0 = none).
    pub kind: u8,
    pub at: Vec3,
    pub t: f32,
    /// Drops landed / linger ticks done.
    pub done: u8,
    pub seed: u32,
    pub tick_t: f32,
}

/// A crescent in flight (clients simulate it from its `HeldFx::Wave`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wave {
    pub live: bool,
    pub origin: Vec3,
    pub dir: Vec3,
    /// The crescent's chord axis (unit, ⟂ `dir`).
    pub axis: Vec3,
    pub size: f32,
    pub speed: f32,
    pub t: f32,
    pub dmg: f32,
    pub kb: f32,
    pub heavy: bool,
    /// Players hit (bit per id & 31), constructs hit.
    pub hit: u32,
    pub chits: [u32; 4],
    pub carves: u8,
    pub carved_at: f32,
}

/// Per-item legendary state (inside `HeldState`; fixed size).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LegendState {
    /// LMB / RMB held continuously (s; same counting as [`LegendMoves`]); LMB's value before this
    /// tick's release.
    pub lmb: f32,
    pub lmb_prev: f32,
    pub rmb: f32,
    /// Coming back because it was recalled (fast, catch snaps).
    pub recall: bool,
    /// Stuck in terrain waiting for a recall.
    pub lodged: bool,
    pub lodge_t: f32,
    /// Enemies chained through this throw.
    pub chain: u8,
    /// Raised for a storm call.
    pub raised: bool,
    pub storm_cd: f32,
    pub storm: Storm,
    /// The heavy that follows a storm call fires no wave.
    pub called: bool,
    pub deflect_t: f32,
    pub deflect_cd: f32,
    pub waves: [Wave; MAX_WAVES],
    pub wave_cd: f32,
    /// Leap and cleave.
    pub leap: bool,
    pub leap_t: f32,
    pub leap_air: bool,
    pub leap_fall: f32,
    pub leap_cd: f32,
    pub pull_cd: f32,
}

/// Wielder-side counters + moves (`PlayerState::extra.legend`, replicated to the owner): the
/// movement half of recall flight and the leap, predicted exactly by clients.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegendMoves {
    pub lmb: f32,
    pub rmb: f32,
    /// Recall flight this tick.
    pub thor: bool,
    /// Leap phase time (0 = not leaping), past the apex (diving), cooldown.
    pub leap_t: f32,
    pub dove: bool,
    pub leap_cd: f32,
}

// ------------------------------------------------------------------------------------ helpers

fn fx(out: &mut ConstructOut, c: &Construct, kind: HeldFx, pos: Vec3, dir: Vec3, strength: f32) {
    out.events.push(GameEvent::HeldFx { id: c.id, owner: c.owner, fx: kind, pos, dir, strength });
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z).normalize_or_zero()
}

fn heading(o: &super::PlayerView) -> Vec3 {
    let f = flat(o.look);
    if f == Vec3::ZERO { flat(Vec3::new(-o.yaw.sin(), 0.0, -o.yaw.cos())).normalize_or(Vec3::NEG_Z) } else { f }
}

fn clear(voxels: &bd_voxel::VoxelWorld, a: Vec3, b: Vec3) -> bool {
    let d = b - a;
    let len = d.length();
    len < 1e-3 || raycast(voxels, a, d / len, len).is_none_or(|h| h.t >= len - 0.05)
}

fn pid_bit(p: PlayerId) -> u32 {
    1u32 << (p.0 as u32 & 31)
}

fn base_dmg(c: &Construct, env: &Env<'_>) -> f32 {
    env.ctx.tuning.form_spec(c.recipe.form).damage * held::mech_mult(c).0 // TUNING HOOK
}

/// Input counters (same rule as [`pre_move`]'s): held continuously while able to act.
fn count(held: bool, v: f32) -> f32 {
    if held { v + DT } else { 0.0 }
}

// ------------------------------------------------------------------------------------ item: per tick

/// Every tick for a held construct, before its mode runs (also while thrown): counters, cooldowns,
/// storms and waves in flight, the release decisions (storm call / leap) and recall flight from the
/// hand.
pub fn tick(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    if !melee_item(c) {
        return;
    }
    let t = env.ctx.tuning.legendary;
    let o = env.owner;
    let dt = env.dt();
    let l = &mut s.leg;
    let ok = !o.busy && !o.input.held(buttons::TYPING);
    let lmb_down = ok && o.input.held(buttons::PRIMARY);
    l.lmb_prev = l.lmb;
    l.lmb = count(lmb_down, l.lmb);
    l.rmb = count(ok && o.input.held(buttons::SECONDARY), l.rmb);
    l.storm_cd = (l.storm_cd - dt).max(0.0);
    l.deflect_cd = (l.deflect_cd - dt).max(0.0);
    l.deflect_t = (l.deflect_t - dt).max(0.0);
    l.wave_cd = (l.wave_cd - dt).max(0.0);
    l.leap_cd = (l.leap_cd - dt).max(0.0);
    l.pull_cd = (l.pull_cd - dt).max(0.0);
    storm_step(c, s, env, out, &t);
    for k in 0..MAX_WAVES {
        if s.leg.waves[k].live {
            wave_step(c, s, k, env, out, &t);
        }
    }
    if s.mode == Mode::Thrown || !c.active() {
        return;
    }
    // Raise the item for a storm (the charge keeps holding).
    if s.mode == Mode::Charge && storms(c) && s.leg.lmb >= t.storm_hold && !s.leg.raised {
        s.leg.raised = true;
        c.set_anim(AnimState::Telegraph, env.tick());
        fx(out, c, HeldFx::StormRaise, o.hand, Vec3::Y, if s.leg.storm_cd > 0.0 { 0.0 } else { 1.0 });
    }
    if s.mode != Mode::Charge {
        s.leg.raised = false;
    }
    // A release (same rule as the wielder's `pre_move`): the leap, or the storm call.
    let released = ok && !lmb_down && s.leg.lmb_prev > 0.0 && s.mode != Mode::Legend;
    if released && leap_release(c, s.leg.lmb_prev, &t) && s.leg.leap_cd <= 0.0 {
        start_leap(c, s, env, &t);
        return;
    }
    if released && storms(c) && s.leg.lmb_prev >= t.storm_hold {
        call_storm(c, s, env, out, &t);
        return;
    }
    // Recall flight straight from the hand (the item was caught while RMB was still held).
    if thor_item(c) && s.leg.rmb >= t.thor_delay && o.willpower > 0.5 && s.mode != Mode::Legend {
        held::start_throw(c, s, env, out);
    }
}

/// After the melee state machine (in hand): the raised pose.
pub fn after_melee(c: &mut Construct, s: &HeldState, env: &Env<'_>) {
    if s.leg.raised && s.mode == Mode::Charge {
        c.rot = geom::look_rotation((Vec3::Y + env.owner.look * 0.15).normalize_or(Vec3::Y));
    }
}

// ------------------------------------------------------------------------------------ recall

/// Recall flight right now (the item side of [`LegendMoves::thor`]).
pub fn thor_now(c: &Construct, s: &HeldState, env: &Env<'_>) -> bool {
    let t = &env.ctx.tuning.legendary;
    thor_item(c) && s.leg.rmb >= t.thor_delay && env.owner.willpower > 0.5
}

/// Thrown-flight bookkeeping (from `held::thrown_inner`, after `fly_t` advanced). Returns true when
/// the item is lodged this tick (nothing else moves it).
pub fn thrown(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) -> bool {
    if !recalls(c) {
        return false;
    }
    let o = env.owner;
    let t = env.ctx.tuning.legendary;
    let press = !o.busy && o.pressed & buttons::SECONDARY != 0;
    if s.leg.lodged {
        if press || thor_now(c, s, env) {
            s.leg.lodged = false;
            start_recall(c, s, out);
            return false;
        }
        s.leg.lodge_t += env.dt();
        c.vel = Vec3::ZERO;
        if s.leg.lodge_t >= t.lodge_secs {
            s.leg.lodged = false;
            s.returning = true;
            return false;
        }
        c.set_anim(AnimState::Stuck, env.tick());
        return true;
    }
    if press && !s.thor && !s.returning {
        start_recall(c, s, out);
    }
    false
}

fn start_recall(c: &mut Construct, s: &mut HeldState, out: &mut ConstructOut) {
    s.returning = true;
    s.leg.recall = true;
    // The way back hits everything on the path, again.
    c.hits.clear();
    fx(out, c, HeldFx::RecallStart, c.center(), Vec3::ZERO, 1.0);
}

/// Speed of the way back.
pub fn return_speed(s: &HeldState, env: &Env<'_>) -> f32 {
    if s.leg.recall { env.ctx.tuning.legendary.recall_speed } else { held::RETURN_SPEED }
}

/// The catch strength (`HeldFx::Catch`): 2 = a recall snapping into the hand.
pub fn catch_strength(c: &Construct, s: &HeldState) -> f32 {
    if s.leg.recall || (recalls(c) && s.leg.chain > 0) { 2.0 } else { 1.0 }
}

/// Reset the flight bits (throw / catch).
pub fn reset_flight(s: &mut HeldState) {
    s.leg.recall = false;
    s.leg.lodged = false;
    s.leg.lodge_t = 0.0;
    s.leg.chain = 0;
}

/// A thrown Returning item met terrain (not bouncing): it lodges (until recalled / `lodge_secs`).
pub fn lodge(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, point: Vec3, nrm: Vec3) {
    let t = &env.ctx.tuning.legendary;
    let r = c.half.min_element().max(0.15);
    if !recalls(c) || t.lodge_secs <= 0.0 || s.leg.recall || s.thor {
        s.returning = true;
        c.pos = point + nrm * (r + 0.05);
        return;
    }
    s.leg.lodged = true;
    s.leg.lodge_t = 0.0;
    c.pos = point + nrm * (r * 0.4);
    c.vel = Vec3::ZERO;
    c.set_anim(AnimState::Stuck, env.tick());
    fx(out, c, HeldFx::Lodge, point, nrm, 1.0);
}

// ------------------------------------------------------------------------------------ ricochet chain

/// A thrown Returning + Ricochet item just hit someone: re-aim at the next enemy in sight (nearest,
/// clear line, not hit yet) — true if it chained, false = come back.
pub fn chain_next(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, at: Vec3) -> bool {
    let t = &env.ctx.tuning.legendary;
    if !(recalls(c) && c.has(Mechanic::Ricochet)) || s.leg.recall || s.thor || s.leg.chain >= t.chain_max {
        return false;
    }
    let ctx = env.ctx;
    let mut best: Option<(Vec3, f32)> = None;
    for e in ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(e.id)) {
            continue;
        }
        let p = ctx.hull_for(c.owner, e).center();
        let d = (p - at).length();
        if d > t.chain_range || best.is_some_and(|(_, b)| d >= b) || !clear(ctx.voxels, at, p) {
            continue;
        }
        best = Some((p, d));
    }
    let Some((p, _)) = best else { return false };
    let dir = (p - at).normalize_or_zero();
    if dir == Vec3::ZERO {
        return false;
    }
    s.leg.chain += 1;
    c.vel = dir * c.vel.length().max(t.chain_speed);
    s.home = None;
    fx(out, c, HeldFx::Ricochet, at, dir, s.leg.chain as f32);
    true
}

// ------------------------------------------------------------------------------------ deflect

/// A swing starts: Blade + Fast items open their deflect window (if it has come back).
pub fn on_swing_start(c: &Construct, s: &mut HeldState, env: &Env<'_>) {
    let t = &env.ctx.tuning.legendary;
    if swing_deflects(c) && s.swing != Swing::Secondary && s.leg.deflect_cd <= 0.0 && t.deflect_window > 0.0 {
        s.leg.deflect_t = t.deflect_window;
        s.leg.deflect_cd = t.deflect_cd;
    }
}

/// The swing's guard this tick (Blade + Fast deflect window): parry-only, timed from the swing start.
pub fn swing_guard(s: &mut HeldState, t: &LegendTuning) -> bool {
    if s.leg.deflect_t <= 0.0 {
        return false;
    }
    s.parry_only = true;
    s.guard_t = (t.deflect_window - s.leg.deflect_t).max(0.0);
    true
}

// ------------------------------------------------------------------------------------ combos / hits

/// Heavy-blade / chain-blade combos: momentum grows along the chain, the finisher is a heavy hit.
pub fn hit_spec(c: &Construct, s: &HeldState, t: &LegendTuning, kb: &mut f32, kind: &mut HitKind, impact: &mut f32) {
    if !combos(c) {
        return;
    }
    match s.swing {
        Swing::Combo1 => *kb *= 1.0 + t.combo_kb_step,
        Swing::Combo2 => {
            *kb *= 1.0 + 2.0 * t.combo_kb_step;
            *kind = HitKind::HeavyMelee;
            *impact = impact.max(1.6);
        }
        _ => {}
    }
}

/// One-shot effects when a swing's hitbox opens: blade waves, the chain blades' 360° sweep.
pub fn on_active(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    if !melee_item(c) {
        return;
    }
    let t = env.ctx.tuning.legendary;
    if chain_blade(c) && s.swing == Swing::Combo2 {
        chain_sweep(c, env, out, &t);
    }
    if !waves(c) {
        return;
    }
    let called = std::mem::take(&mut s.leg.called);
    if s.swing == Swing::Heavy && s.heavy_k >= t.wave_min_k && !called && s.leg.wave_cd <= 0.0 {
        let heavy = c.has(Mechanic::Heavy);
        let size = item_size(c) * if heavy { t.heavy_wave_size } else { 1.0 };
        let speed = t.wave_speed * if heavy { t.heavy_wave_speed } else { 1.0 };
        let dmg = base_dmg(c, env) * t.wave_dmg * s.heavy_k;
        launch_wave(c, s, env, out, &t, size, speed, dmg, t.wave_kb * held::mech_mult(c).1, true, heavy);
        s.leg.wave_cd = t.wave_cd;
    } else if c.has(Mechanic::Shoots) && s.swing != Swing::Secondary && s.swing != Swing::Heavy && s.leg.wave_cd <= 0.0 {
        let size = item_size(c) * t.shoot_wave_size;
        let dmg = base_dmg(c, env) * t.shoot_wave_dmg;
        launch_wave(c, s, env, out, &t, size, t.wave_speed, dmg, t.wave_kb * 0.6, false, false);
        s.leg.wave_cd = t.shoot_wave_cd;
    }
}

/// Wave size factor from the item's length (a 1.2 m sword = 1).
fn item_size(c: &Construct) -> f32 {
    ((c.half.z * 2.0) / 1.2).clamp(0.8, 2.0)
}

/// Knockback multiplier for `target` right now: heavy blades are armored while charging, swinging
/// heavy / finishing, and leaping (never immune). 1 = none. Used by `World::apply_hit`.
pub fn armor(store: &ConstructStore, target: PlayerId, t: &LegendTuning) -> f32 {
    for c in store.owned_by(target) {
        if !c.active() || !heavy_blade(c) || c.held.thrown {
            continue;
        }
        let s = &c.held;
        let big = match s.mode {
            Mode::Charge | Mode::Legend => true,
            Mode::Swing => matches!(s.swing, Swing::Heavy | Swing::Combo2),
            _ => false,
        };
        if big {
            return t.armor_kb;
        }
    }
    1.0
}

// ------------------------------------------------------------------------------------ waves

#[allow(clippy::too_many_arguments)]
fn launch_wave(c: &Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning, size: f32, speed: f32, dmg: f32, kb: f32, vertical: bool, heavy: bool) {
    let o = env.owner;
    let dir = o.look.normalize_or(Vec3::NEG_Z);
    // Anyone inside the blade's own reach is the swing's business: the crescent flies on past them.
    let reach = c.half.z * 2.0 + 1.2;
    let mut skip = 0u32;
    for e in env.ctx.enemies_of(c.owner) {
        if geom::dist_sq_point_aabb(&env.ctx.hull_for(c.owner, e), o.hand) <= reach * reach {
            skip |= pid_bit(e.id);
        }
    }
    launch_wave_at(c, s, out, t, o.hand + dir * 0.8, dir, size, speed, dmg, kb, vertical, heavy, skip);
}

/// Launch a crescent from `origin` along `dir`. `vertical`: the chord stands up (a cleave); else it
/// lies flat (a horizontal slash).
#[allow(clippy::too_many_arguments)]
fn launch_wave_at(c: &Construct, s: &mut HeldState, out: &mut ConstructOut, t: &LegendTuning, origin: Vec3, dir: Vec3, size: f32, speed: f32, dmg: f32, kb: f32, vertical: bool, heavy: bool, skip: u32) {
    let Some(k) = (0..MAX_WAVES).find(|k| !s.leg.waves[*k].live).or_else(|| {
        // Replace the oldest.
        (0..MAX_WAVES).max_by(|a, b| s.leg.waves[*a].t.total_cmp(&s.leg.waves[*b].t))
    }) else {
        return;
    };
    let axis = wave_axis(dir, vertical);
    s.leg.waves[k] = Wave { live: true, origin, dir, axis, size, speed, t: 0.0, dmg, kb, heavy, hit: skip, chits: [0; 4], carves: 0, carved_at: 0.0 };
    let _ = t;
    // Clients simulate the crescent: strength = size (negative = vertical chord), speed from tuning.
    fx(out, c, HeldFx::Wave, origin, dir, if vertical { -size } else { size });
}

/// The crescent's chord axis for travel direction `dir` (shared with clients).
pub fn wave_axis(dir: Vec3, vertical: bool) -> Vec3 {
    let dir = dir.normalize_or(Vec3::NEG_Z);
    let right = dir.cross(Vec3::Y).normalize_or(Vec3::X);
    if vertical { right.cross(dir).normalize_or(Vec3::Y) } else { right }
}

/// Speed of a wave of `size` launched by a heavy blade or not (clients use it too).
pub fn wave_speed(t: &LegendTuning, heavy: bool) -> f32 {
    t.wave_speed * if heavy { t.heavy_wave_speed } else { 1.0 }
}

/// Crescent sample `k` of 5 (−2..=2) at `center` (the tips trail behind the middle).
pub fn wave_point(center: Vec3, dir: Vec3, axis: Vec3, half_w: f32, k: i32) -> Vec3 {
    let f = k as f32 / 2.0;
    center + axis * (f * half_w) - dir * (f * f * half_w * 0.45)
}

fn wave_step(c: &mut Construct, s: &mut HeldState, k: usize, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning) {
    let ctx = env.ctx;
    let dt = env.dt();
    let mut w = s.leg.waves[k];
    let d0 = w.speed * w.t;
    w.t += dt;
    let d1 = (w.speed * w.t).min(t.wave_range);
    if d0 >= t.wave_range {
        w.live = false;
        s.leg.waves[k] = w;
        return;
    }
    let half_w = t.wave_half_width * w.size;
    let thick = t.wave_thick * w.size.sqrt();
    let (c0, c1) = (w.origin + w.dir * d0, w.origin + w.dir * d1);
    let kind = if w.heavy || w.size > 1.2 { HitKind::HeavyMelee } else { HitKind::Melee };
    for e in ctx.enemies_of(c.owner) {
        if w.hit & pid_bit(e.id) != 0 {
            continue;
        }
        let hull = geom::inflate(&ctx.hull_for(c.owner, e), thick);
        let mut at = None;
        for j in -2..=2 {
            let (p0, p1) = (wave_point(c0, w.dir, w.axis, half_w, j), wave_point(c1, w.dir, w.axis, half_w, j));
            if let Some((f, _)) = geom::segment_aabb(p0, p1, &hull) {
                at = Some(p0 + (p1 - p0) * f);
                break;
            }
        }
        let Some(point) = at else { continue };
        w.hit |= pid_bit(e.id);
        out.player_hits.push(
            HitInfo::new(e.id, Some(c.owner), kind, w.dmg, w.kb, w.dir + Vec3::Y * 0.2, point)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_closing(0.0)
                .with_impact(t.wave_impact * w.size.sqrt()),
        );
    }
    for px in env.others {
        if px.owner == c.owner || !px.active || px.attached || w.chits.contains(&px.id.0) {
            continue;
        }
        let mut hit = None;
        for j in [-2, 0, 2] {
            let (p0, p1) = (wave_point(c0, w.dir, w.axis, half_w, j), wave_point(c1, w.dir, w.axis, half_w, j));
            if let Some((f, _)) = geom::segment_obb(p0, p1, px.center, px.half, px.rot, thick) {
                hit = Some(p0 + (p1 - p0) * f);
                break;
            }
        }
        let Some(point) = hit else { continue };
        if let Some(slot) = w.chits.iter_mut().find(|v| **v == 0) {
            *slot = px.id.0;
        }
        out.construct_hits.push(ConstructHit {
            pierce: true,
            blunt: false,
            target: px.id,
            source: Some(c.owner),
            damage: t.wave_construct_dmg * w.size,
            impulse: w.dir * w.kb,
            blade: true,
            point,
        });
    }
    // Terrain: a light groove wherever it cuts. Skimming the ground with a tip scars it for free;
    // ploughing through with its middle spends the budget, and it breaks after `wave_carves` cuts.
    let solid = |p: Vec3| ctx.voxels.is_solid(world_to_voxel(p));
    let core = solid(c1);
    let tips = [-2, 2].map(|j| wave_point(c1, w.dir, w.axis, half_w * 0.8, j));
    if core || tips.iter().any(|p| solid(*p)) {
        if core && w.carves >= t.wave_carves {
            w.live = false;
            fx(out, c, HeldFx::SwingImpact, c1, w.dir, w.size);
        } else if d1 - w.carved_at >= t.wave_carve_step || w.carved_at == 0.0 {
            if core {
                w.carves += 1;
            }
            w.carved_at = d1.max(1e-3);
            if t.wave_carve > 0.0 {
                let r = t.wave_carve * w.size.sqrt();
                for p in [c1, tips[0], tips[1]] {
                    if solid(p) {
                        out.carves.push(CarveRequest { center: p, radius: if p == c1 { r } else { r * 0.7 }, power: 0.5 });
                    }
                }
            }
        }
    }
    if d1 >= t.wave_range {
        w.live = false;
    }
    s.leg.waves[k] = w;
}

// ------------------------------------------------------------------------------------ storms

fn call_storm(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning) {
    let o = env.owner;
    s.leg.raised = false;
    let kind = StormKind::of(c.recipe.element);
    // The item comes down in a full heavy either way (the called swing fires no wave).
    held::begin_heavy(c, s, env, out, 1.0);
    s.leg.called = true;
    let Some(kind) = kind.filter(|_| s.leg.storm_cd <= 0.0) else {
        return;
    };
    let ctx = env.ctx;
    // Where: the first thing the aim meets (terrain, or an enemy as the caster saw them), else the
    // ground under the aim at range.
    let mut reach = raycast(ctx.voxels, o.eye, o.look, t.storm_range).map_or(t.storm_range, |h| h.t);
    for e in ctx.enemies_of(c.owner) {
        let hull = ctx.hull_for(c.owner, e);
        if let Some((f, _)) = geom::segment_aabb(o.eye, o.eye + o.look * reach, &geom::inflate(&hull, 0.6)) {
            reach *= f;
        }
    }
    let aim = o.eye + o.look * reach;
    let at = raycast(ctx.voxels, aim + Vec3::Y * 0.5, Vec3::NEG_Y, 80.0).map_or(aim, |h| h.point);
    s.leg.storm = Storm { kind: kind as u8, at, t: 0.0, done: 0, seed: (c.id.0 ^ (env.tick() as u32).wrapping_mul(0x9E37_79B9)) | 1, tick_t: 0.0 };
    s.leg.storm_cd = t.storm_cd;
    if kind.rains() {
        for i in 0..t.rain_count as usize {
            let p = drop_point(&s.leg.storm, i, t);
            let land = t.storm_delay + i as f32 * t.rain_interval;
            fx(out, c, HeldFx::StormMark, p, Vec3::Y, land);
        }
    } else {
        fx(out, c, HeldFx::StormMark, at, Vec3::Y, t.storm_delay);
    }
}

/// Where drop `i` of a rain lands (deterministic from the storm's seed; the first lands dead on).
pub fn drop_point(st: &Storm, i: usize, t: &LegendTuning) -> Vec3 {
    if i == 0 {
        return st.at;
    }
    let mut r = SimRng::new(st.seed as u64 ^ (i as u64).wrapping_mul(0x5851_F42D_4C95_7F2D));
    let a = r.range(0.0, std::f32::consts::TAU);
    let d = t.rain_spread * r.range(0.35, 1.0);
    st.at + Vec3::new(a.cos() * d, 0.0, a.sin() * d)
}

fn storm_step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning) {
    let Some(kind) = StormKind::from_u8(s.leg.storm.kind) else { return };
    let dt = env.dt();
    let mut st = s.leg.storm;
    st.t += dt;
    let bd = base_dmg(c, env);
    if kind.rains() {
        let n = t.rain_count;
        while st.done < n && st.t >= t.storm_delay + st.done as f32 * t.rain_interval {
            let p = drop_point(&st, st.done as usize, t);
            let ground = raycast(env.ctx.voxels, p + Vec3::Y * 6.0, Vec3::NEG_Y, 20.0).map_or(p, |h| h.point);
            drop_impact(c, env, out, t, kind, ground, bd);
            st.done += 1;
        }
        if st.done >= n {
            st.kind = 0;
        }
    } else if kind.lingers() {
        if st.t >= t.storm_delay {
            if st.done == 0 {
                st.done = 1;
                fx(out, c, HeldFx::StormStrike, st.at, Vec3::Y, t.storm_radius);
            }
            linger_tick(c, env, out, t, kind, &mut st, bd);
            if st.t >= t.storm_delay + t.linger_secs {
                if kind == StormKind::Tornado {
                    area_hits(c, env, out, st.at, t.storm_radius * 1.3, 8.0, bd * t.storm_dmg * 0.6, t.storm_kb, |_| Vec3::Y, HitKind::Explosion, t.storm_impact, c.recipe.element);
                    fx(out, c, HeldFx::StormStrike, st.at, Vec3::Y, t.storm_radius * 1.3);
                }
                st.kind = 0;
            }
        }
    } else if st.t >= t.storm_delay {
        strike(c, env, out, t, kind, st.at, bd);
        st.kind = 0;
    }
    s.leg.storm = st;
}

/// Hit every enemy in a vertical column of `radius` over [at.y − 1, at.y + height] once.
#[allow(clippy::too_many_arguments)]
fn area_hits(
    c: &Construct,
    env: &Env<'_>,
    out: &mut ConstructOut,
    at: Vec3,
    radius: f32,
    height: f32,
    dmg: f32,
    kb: f32,
    dir: impl Fn(Vec3) -> Vec3,
    kind: HitKind,
    impact: f32,
    element: Element,
) -> u32 {
    let ctx = env.ctx;
    let mut mask = 0u32;
    for e in ctx.enemies_of(c.owner) {
        let hull = ctx.hull_for(c.owner, e);
        let probe = Vec3::new(at.x, hull.center().y.clamp(at.y - 1.0, at.y + height), at.z);
        let d = geom::dist_sq_point_aabb(&hull, probe).sqrt();
        if d > radius {
            continue;
        }
        let fall = 1.0 - 0.5 * (d / radius.max(1e-3));
        mask |= pid_bit(e.id);
        out.player_hits.push(
            HitInfo::new(e.id, Some(c.owner), kind, dmg * fall, kb * fall, dir(hull.center()), geom::closest_point(&hull, probe))
                .with_element(element)
                .with_construct(c.id)
                .with_closing(0.0)
                .with_impact(impact * fall.max(0.6)),
        );
    }
    for px in env.others {
        if px.owner == c.owner || !px.active || px.attached {
            continue;
        }
        let d = Vec3::new(px.center.x - at.x, 0.0, px.center.z - at.z).length() - px.half.x.max(px.half.z);
        if d > radius || px.center.y + px.half.y < at.y - 1.0 || px.center.y - px.half.y > at.y + height {
            continue;
        }
        out.construct_hits.push(ConstructHit {
            pierce: false,
            blunt: true,
            target: px.id,
            source: Some(c.owner),
            damage: ctx.tuning.legendary.storm_construct_dmg * (dmg / ctx.tuning.form_spec(c.recipe.form).damage.max(1.0)).clamp(0.2, 2.0),
            impulse: dir(px.center) * kb,
            blade: false,
            point: px.center,
        });
    }
    mask
}

fn outward(at: Vec3, up: f32) -> impl Fn(Vec3) -> Vec3 {
    move |p: Vec3| flat(p - at) * 0.5 + Vec3::Y * up
}

fn strike(c: &Construct, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning, kind: StormKind, at: Vec3, bd: f32) {
    let r = t.storm_radius;
    let el = c.recipe.element;
    match kind {
        StormKind::Bolt => {
            let hit = area_hits(c, env, out, at, r, 40.0, bd * t.storm_dmg, t.storm_kb, outward(at, 1.0), HitKind::Explosion, t.storm_impact, el);
            out.carves.push(CarveRequest { center: at, radius: 1.0, power: 0.6 });
            fx(out, c, HeldFx::StormStrike, at, Vec3::Y, r);
            // Arcs: to the nearest unstruck enemies near anyone struck (half damage).
            let ctx = env.ctx;
            let mut arcs = 0u8;
            let mut from = at;
            let mut struck = hit;
            while arcs < t.bolt_arcs {
                let mut best: Option<(PlayerId, Vec3, f32)> = None;
                for e in ctx.enemies_of(c.owner) {
                    if struck & pid_bit(e.id) != 0 {
                        continue;
                    }
                    let p = ctx.hull_for(c.owner, e).center();
                    let d = (p - from).length();
                    if d <= t.bolt_arc_range && best.is_none_or(|b| d < b.2) && clear(ctx.voxels, from, p) {
                        best = Some((e.id, p, d));
                    }
                }
                let Some((id, p, d)) = best else { break };
                struck |= pid_bit(id);
                out.player_hits.push(
                    HitInfo::new(id, Some(c.owner), HitKind::Chain, bd * t.storm_dmg * 0.5, t.storm_kb * 0.4, (p - from).normalize_or(Vec3::Y), p)
                        .with_element(el)
                        .with_construct(c.id)
                        .with_closing(0.0)
                        .with_impact(t.storm_impact * 0.6),
                );
                fx(out, c, HeldFx::StormArc, from, (p - from).normalize_or(Vec3::Y), d);
                from = p;
                arcs += 1;
            }
        }
        StormKind::Geyser => {
            area_hits(c, env, out, at, r, 6.0, bd * t.storm_dmg * 0.5, t.storm_kb * 1.2, |_| Vec3::Y, HitKind::Explosion, t.storm_impact, el);
            out.carves.push(CarveRequest { center: at, radius: 1.2, power: 0.5 });
            fx(out, c, HeldFx::StormStrike, at, Vec3::Y, r);
        }
        StormKind::Spikes => {
            area_hits(c, env, out, at, r, 4.0, bd * t.storm_dmg * 0.9, t.storm_kb * 0.95, outward(at, 1.0), HitKind::Explosion, t.storm_impact, el);
            out.carves.push(CarveRequest { center: at, radius: 1.8, power: 1.0 });
            fx(out, c, HeldFx::StormStrike, at, Vec3::Y, r);
        }
        StormKind::Crush => {
            // Slammed flat: damage, a short stagger, velocity driven into the ground (no lift).
            let mask = area_hits(c, env, out, at, r * 1.3, 12.0, bd * t.storm_dmg * 0.8, 0.0, |_| Vec3::NEG_Y, HitKind::Crush, t.storm_impact, Element::Light);
            for e in env.ctx.enemies_of(c.owner) {
                if mask & pid_bit(e.id) != 0 {
                    out.impulses.push((e.id, Vec3::new(-e.vel.x * 0.6, -(e.vel.y.max(0.0)) - 18.0, -e.vel.z * 0.6), 0.0));
                    out.statuses.push((e.id, StatusKind::Stun, 0.25, Some(c.owner)));
                }
            }
            out.carves.push(CarveRequest { center: at, radius: 1.4, power: 0.5 });
            fx(out, c, HeldFx::StormStrike, at, Vec3::NEG_Y, r * 1.3);
        }
        StormKind::Nova => {
            out.explosions.push(Explosion {
                pos: at + Vec3::Y * 0.5,
                radius: r * 1.5,
                damage: bd * t.storm_dmg,
                knockback: t.storm_kb * 1.1,
                source: Some(c.owner),
                construct: Some(c.id),
                element: el,
                carve_radius: 2.4,
                self_damage: false,
            });
            fx(out, c, HeldFx::StormStrike, at, Vec3::Y, r * 1.5);
        }
        _ => {}
    }
}

fn drop_impact(c: &Construct, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning, kind: StormKind, at: Vec3, bd: f32) {
    let el = c.recipe.element;
    match kind {
        StormKind::Meteors => out.explosions.push(Explosion {
            pos: at + Vec3::Y * 0.3,
            radius: 2.6,
            damage: bd * t.storm_dmg * t.rain_dmg,
            knockback: t.storm_kb * 0.7,
            source: Some(c.owner),
            construct: Some(c.id),
            element: el,
            carve_radius: 1.3,
            self_damage: false,
        }),
        _ => {
            area_hits(c, env, out, at, 1.6, 3.0, bd * t.storm_dmg * t.rain_dmg, t.storm_kb * 0.5, outward(at, 0.6), HitKind::Explosion, t.storm_impact * 0.7, el);
            out.carves.push(CarveRequest { center: at, radius: 0.6, power: 0.4 });
        }
    }
    fx(out, c, HeldFx::StormStrike, at, Vec3::NEG_Y, if kind == StormKind::Meteors { 2.6 } else { 1.6 });
}

fn linger_tick(c: &Construct, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning, kind: StormKind, st: &mut Storm, bd: f32) {
    let dt = env.dt();
    let r = t.storm_radius * 1.6;
    let at = st.at;
    if kind != StormKind::Miasma {
        let lift = if kind == StormKind::Tornado { 0.55 } else { 0.1 };
        for e in env.ctx.enemies_of(c.owner) {
            let to = at - e.pos;
            let d = Vec3::new(to.x, 0.0, to.z).length();
            if d > r || e.pos.y > at.y + 12.0 || e.pos.y < at.y - 2.0 {
                continue;
            }
            let k = 1.0 - 0.5 * d / r;
            let dv = (flat(to) + Vec3::Y * lift) * t.linger_pull * k * dt;
            out.impulses.push((e.id, dv, 0.0));
        }
    }
    st.tick_t += dt;
    if st.tick_t >= t.linger_tick {
        st.tick_t -= t.linger_tick;
        let dmg = bd * t.storm_dmg * t.linger_dmg;
        area_hits(c, env, out, at, t.storm_radius, 8.0, dmg, 0.0, |_| Vec3::Y, HitKind::Explosion, 0.6, c.recipe.element);
    }
}

// ------------------------------------------------------------------------------------ heavy blade: leap

fn start_leap(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, t: &LegendTuning) {
    s.mode = Mode::Legend;
    s.leg.leap = true;
    s.leg.leap_t = 0.0;
    s.leg.leap_air = false;
    s.leg.leap_fall = 0.0;
    s.leg.leap_cd = t.leap_cd;
    s.leg.raised = false;
    s.guarding = false;
    s.heavy_k = 1.0;
    c.hits.clear();
    c.set_anim(AnimState::Windup, env.tick());
}

/// `Mode::Legend` (the leap): wait for the landing, then cleave.
pub fn step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let o = env.owner;
    let t = env.ctx.tuning.legendary;
    if !s.leg.leap {
        s.mode = Mode::Idle;
        return;
    }
    s.leg.leap_t += env.dt();
    if !o.grounded {
        s.leg.leap_air = true;
    }
    s.leg.leap_fall = s.leg.leap_fall.max(-o.vel.y);
    let f = heading(&o);
    // Raised behind the head on the way up, then brought down in front.
    let diving = o.vel.y < 0.0 || s.leg.leap_t >= t.leap_rise;
    c.rot = geom::look_rotation(if diving { (f * 0.4 + Vec3::NEG_Y).normalize_or(Vec3::NEG_Y) } else { (Vec3::Y - f * 0.3).normalize_or(Vec3::Y) });
    if diving && c.anim_state != AnimState::Active {
        c.set_anim(AnimState::Active, env.tick());
    }
    let near = raycast(env.ctx.voxels, o.pos + Vec3::Y * 0.1, Vec3::NEG_Y, 0.6).is_some();
    if (s.leg.leap_air && (o.grounded || near) && s.leg.leap_t > 0.08) || s.leg.leap_t > t.leap_max {
        leap_slam(c, s, env, out, &t);
        s.leg.leap = false;
        s.mode = Mode::Idle;
        s.cd2 = s.cd2.max(0.3);
        c.set_anim(AnimState::Recover, env.tick());
    }
}

fn leap_slam(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning) {
    let o = env.owner;
    let ctx = env.ctx;
    let f = heading(&o);
    let strength = (1.0 + (s.leg.leap_fall - 15.0).max(0.0) / 30.0).clamp(1.0, 1.8);
    // A line of craters where the blade bites, ahead of the feet.
    for k in [1.3f32, 2.8, 4.3] {
        let p = o.pos + f * k;
        if let Some(h) = raycast(ctx.voxels, p + Vec3::Y * 1.5, Vec3::NEG_Y, 4.0) {
            out.carves.push(CarveRequest { center: h.point, radius: t.leap_crater * (1.0 - k / 12.0), power: 1.0 });
        }
    }
    let at = o.pos + f * 2.0;
    let radius = t.leap_radius * strength.sqrt();
    let bd = base_dmg(c, env);
    let (_, km) = held::mech_mult(c);
    for e in ctx.enemies_of(c.owner) {
        let hull = ctx.hull_for(c.owner, e);
        let d = geom::dist_sq_point_aabb(&hull, at).sqrt();
        if d > radius || (hull.center().y - at.y).abs() > 3.5 {
            continue;
        }
        let fall = 1.0 - 0.55 * (d / radius);
        c.hits.insert(player_key(e.id));
        let dir = flat(hull.center() - o.pos) * 0.6 + Vec3::Y;
        out.player_hits.push(
            HitInfo::new(e.id, Some(c.owner), HitKind::HeavyMelee, bd * t.leap_dmg * fall, t.leap_kb * km * fall * strength.min(1.4), dir, hull.center())
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_closing(s.leg.leap_fall)
                .with_impact(t.leap_impact * strength),
        );
    }
    for px in env.others {
        if px.owner == c.owner || !px.active || px.attached {
            continue;
        }
        if (px.center - at).length() - px.half.max_element() > radius {
            continue;
        }
        out.construct_hits.push(ConstructHit { pierce: false, blunt: true, target: px.id, source: Some(c.owner), damage: t.wave_construct_dmg * 2.0, impulse: (flat(px.center - at) + Vec3::Y) * t.leap_kb, blade: true, point: px.center });
    }
    fx(out, c, HeldFx::LeapSlam, at, f, radius);
    // The cleave sends a standing wave along the ground.
    if t.wave_dmg > 0.0 {
        let size = item_size(c) * t.heavy_wave_size;
        let origin = o.pos + f * 1.2 + Vec3::Y * (0.4 + t.wave_half_width * size * 0.5);
        let mut skip = 0u32;
        for e in ctx.enemies_of(c.owner) {
            if c.hits.contains(player_key(e.id)) {
                skip |= pid_bit(e.id);
            }
        }
        launch_wave_at(c, s, out, t, origin, f, size, wave_speed(t, true), bd * t.wave_dmg, t.wave_kb * km, true, true, skip);
    }
}

// ------------------------------------------------------------------------------------ chain blades

fn chain_sweep(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, t: &LegendTuning) {
    let o = env.owner;
    let ctx = env.ctx;
    let reach = if o.size > 1.0 { o.size.min(1.4) } else { o.size.max(0.25) };
    let radius = t.sweep_radius * reach;
    let bd = base_dmg(c, env);
    let (_, km) = held::mech_mult(c);
    let center = o.pos + Vec3::Y * 1.0;
    for e in ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(e.id)) {
            continue;
        }
        let hull = ctx.hull_for(c.owner, e);
        let probe = Vec3::new(center.x, hull.center().y.clamp(center.y - t.sweep_height, center.y + t.sweep_height), center.z);
        let d = geom::dist_sq_point_aabb(&hull, probe).sqrt();
        if d > radius || !clear(ctx.voxels, o.hand, geom::closest_point(&hull, o.hand)) {
            continue;
        }
        c.hits.insert(player_key(e.id));
        let dir = flat(hull.center() - center) + Vec3::Y * 0.35;
        out.player_hits.push(
            HitInfo::new(e.id, Some(c.owner), HitKind::HeavyMelee, bd * t.sweep_dmg, t.sweep_kb * km, dir, geom::closest_point(&hull, center))
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(t.sweep_impact),
        );
    }
    for px in env.others {
        if px.owner == c.owner || !px.active || px.attached || c.hits.contains(px.id.0) {
            continue;
        }
        let d = Vec3::new(px.center.x - center.x, 0.0, px.center.z - center.z).length() - px.half.x.max(px.half.z);
        if d > radius || (px.center.y - center.y).abs() > t.sweep_height + px.half.y {
            continue;
        }
        c.hits.insert(px.id.0);
        out.construct_hits.push(ConstructHit { pierce: false, blunt: false, target: px.id, source: Some(c.owner), damage: bd * t.sweep_dmg * 2.0, impulse: flat(px.center - center) * t.sweep_kb, blade: true, point: px.center });
    }
    fx(out, c, HeldFx::ChainSweep, center, heading(&o), radius);
}

/// RMB with pulling chain blades at an enemy in reach: yank yourself in (and them a little toward
/// you). True if it latched (else the whip's terrain lash runs).
pub fn chain_pull(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) -> bool {
    let t = env.ctx.tuning.legendary;
    if !chain_pulls(c) || s.leg.pull_cd > 0.0 {
        return false;
    }
    let o = env.owner;
    let ctx = env.ctx;
    let cands = ctx.enemies_of(c.owner).map(|e| (e.id, ctx.hull_for(c.owner, e)));
    let Some((id, at)) = crate::aim_assist::pick_target(o.eye, o.look, t.pull_cone_deg.to_radians().cos(), t.pull_range, cands, |a, b| clear(ctx.voxels, a, b)) else {
        return false;
    };
    let to = at - o.eye;
    let dist = to.length();
    let dir = to.normalize_or_zero();
    if dir == Vec3::ZERO {
        return false;
    }
    let add = (t.pull_speed - o.vel.dot(dir)).max(0.0);
    out.impulses.push((c.owner, dir * add + Vec3::Y * 2.5, 0.0));
    out.impulses.push((id, -dir * t.pull_victim + Vec3::Y * 3.0, 0.0));
    out.player_hits.push(
        HitInfo::new(id, Some(c.owner), HitKind::Melee, base_dmg(c, env) * 0.4, 0.0, -dir, at)
            .with_element(c.recipe.element)
            .with_construct(c.id)
            .with_closing(0.0)
            .with_impact(1.2),
    );
    s.leg.pull_cd = t.pull_cd;
    held::begin_swing(c, s, env, out, Swing::Secondary);
    s.cd2 = s.cd2.max(0.3);
    fx(out, c, HeldFx::ChainPull, at, -dir, dist);
    true
}

// ------------------------------------------------------------------------------------ wielder

/// Before movement (host `step_movement` and client `predict_player`, same place, before the
/// movement modifiers): the input counters, recall flight (Thor) and the heavy-blade leap. Only
/// touches `st.extra.legend` / velocity when the player holds a legendary-kit item.
#[allow(clippy::too_many_arguments)]
pub fn pre_move(store: &ConstructStore, t: &LegendTuning, owner: PlayerId, input: &PlayerInput, st: &mut PlayerState, busy: bool, willpower: f32, mut emit: impl FnMut(GameEvent)) {
    let item = store.owned_by(owner).find(|c| c.active() && c.pilot.is_none() && (thor_item(c) || heavy_blade(c)));
    let Some(c) = item else {
        if st.extra.legend != LegendMoves::default() {
            st.extra.legend = LegendMoves::default();
        }
        return;
    };
    let ok = !busy && !input.held(buttons::TYPING);
    let lmb_down = ok && input.held(buttons::PRIMARY);
    let l = &mut st.extra.legend;
    let prev = l.lmb;
    l.lmb = count(lmb_down, l.lmb);
    l.rmb = count(ok && input.held(buttons::SECONDARY), l.rmb);
    l.leap_cd = (l.leap_cd - DT).max(0.0);
    l.thor = thor_item(c) && l.rmb >= t.thor_delay && willpower > 0.5;
    // The leap in progress: rise, then dive at the apex (or after `leap_rise`); over on landing.
    if l.leap_t > 0.0 {
        l.leap_t += DT;
        if (st.grounded && l.leap_t > 0.1) || st.tumble > 0.0 || l.thor || l.leap_t > t.leap_max {
            l.leap_t = 0.0;
            l.dove = false;
        } else if !l.dove && (st.vel.y <= 0.0 || l.leap_t >= t.leap_rise) {
            l.dove = true;
            st.vel.y = st.vel.y.min(-t.leap_dive);
        }
    }
    let l = st.extra.legend;
    let released = ok && !lmb_down && prev > 0.0;
    if released && c.attached() && l.leap_t <= 0.0 && l.leap_cd <= 0.0 && !l.thor && leap_release(c, prev, t) {
        let f = {
            let lk = input.look_dir();
            let b = Vec3::new(lk.x, 0.0, lk.z).normalize_or_zero();
            if b == Vec3::ZERO { Vec3::NEG_Z } else { b }
        };
        let up = (t.leap_up - st.vel.y).max(0.0);
        let fwd = (t.leap_fwd - st.vel.dot(f)).max(0.0);
        st.apply_impulse(Vec3::Y * up + f * fwd, 0.0);
        st.grounded = false;
        let l = &mut st.extra.legend;
        l.leap_t = DT;
        l.dove = false;
        l.leap_cd = t.leap_cd;
        emit(GameEvent::HeldFx { id: c.id, owner, fx: HeldFx::LeapStart, pos: st.pos, dir: f, strength: up + fwd });
    }
}

/// Recall flight's movement: no gravity while it drags you.
pub fn movement_mods(st: &PlayerState, mods: &mut crate::movement::MoveModifiers) {
    if st.extra.legend.thor {
        mods.gravity_scale = 0.0;
    }
}

/// Recall flight's movement: you follow the item along your aim.
pub fn post_move(input: &PlayerInput, st: &mut PlayerState, dt: f32) {
    if !st.extra.legend.thor {
        return;
    }
    let look = input.look_dir();
    let want = look * held::THOR_SPEED;
    st.vel += (want - st.vel) * (5.0 * dt).min(1.0);
    st.tumble = 0.0;
    if want.y > 0.0 {
        st.grounded = false;
    }
}

// ------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storm_kinds_cover_every_element_but_light() {
        for e in [
            Element::Fire,
            Element::Electric,
            Element::Ice,
            Element::Water,
            Element::Wind,
            Element::Earth,
            Element::Poison,
            Element::Psychic,
            Element::Shadow,
            Element::Atomic,
        ] {
            let k = StormKind::of(e).expect("callable");
            assert_eq!(StormKind::from_u8(k as u8), Some(k));
        }
        assert_eq!(StormKind::of(Element::Light), None);
    }

    #[test]
    fn drops_are_deterministic_and_scattered() {
        let t = LegendTuning::default();
        let st = Storm { kind: StormKind::Meteors as u8, at: Vec3::new(3.0, 8.0, -4.0), seed: 77, ..Default::default() };
        assert_eq!(drop_point(&st, 0, &t), st.at);
        for i in 1..t.rain_count as usize {
            let p = drop_point(&st, i, &t);
            assert_eq!(p, drop_point(&st, i, &t));
            let d = (p - st.at).length();
            assert!(d > 0.3 * t.rain_spread && d <= t.rain_spread + 1e-4, "{d}");
        }
    }

    #[test]
    fn wave_axis_is_perpendicular() {
        for dir in [Vec3::NEG_Z, Vec3::new(0.3, -0.4, -0.8).normalize(), Vec3::Y] {
            for v in [false, true] {
                let a = wave_axis(dir, v);
                assert!(a.dot(dir.normalize()).abs() < 1e-4, "{dir:?} {v}");
                assert!((a.length() - 1.0).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn validated_is_idempotent_and_safe() {
        let t = LegendTuning::default().validated();
        assert_eq!(t, LegendTuning::default());
        let bad = LegendTuning { storm_radius: f32::NAN, rain_count: 200, leap_hold: -3.0, ..Default::default() }.validated();
        assert_eq!(bad.storm_radius, LegendTuning::default().storm_radius);
        assert_eq!(bad.rain_count as usize, MAX_DROPS);
        assert!(bad.leap_hold >= 0.15);
    }
}
