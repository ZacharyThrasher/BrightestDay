//! Player powers (COMBAT.md §5h) — ONE framework for every iconic power system.
//!
//! A power is data (`assets/powers.ron`, [`PowerTable`]) in a fixed template: `body` (passive
//! traits), `graft` (merged into every cast), `incantation` (`<prefix> <phrase>` delivery), `moves`
//! (G tap / G hold → [`crate::World::power_move`]), `transform` (Zoan beast form / piloted dragon)
//! and `companion` (JoJo stands). Every piece reduces to closed-set things the sim already does.
//!
//! ## Client / lexicon contract
//! * The lexicon maps fruit / stand names to recipes with `grants: Some(id)` (it scans `id`,
//!   `name`, `names`, `incantations`, `kind`, `companion` of each powers.ron entry;
//!   [`PowerTable::id_for_name`]; `"random:devil_fruit"` / `"random:stand"` = the sim rolls one). Casting such a recipe with [`crate::World::cast`] makes an edible fruit construct
//!   (anyone who touches it eats it) or calls the stand (held until KO).
//! * Before resolving a typed/spoken phrase, the client calls [`parse_phrase`] with the local
//!   player's power (`PlayerSnap::power.power()`): `Move(slot)` → `power_move`, `Incantation {rest}`
//!   → resolve `rest` with the lexicon and call [`crate::World::cast_incantation`], `Plain` → cast.
//! * While the ring console is open, every rush token the client counts
//!   ([`PowerTable::is_rush_token`]: ORA / MUDA / DORA / ARI / WRYY…) → `stand_rush_token`.
//! * State: `PlayerSnap::power` ([`PowerSnap`]); moments: `GameEvent::Power(`[`PowerEvent`]`)`.
//!   Clients mirroring a world for prediction copy `PowerSnap` into `Player::power` with
//!   [`PlayerPower::apply_snap`] so [`PlayerPower::modify_movement`] predicts identically.

mod data;
mod exec;
mod hooks;

use std::collections::VecDeque;

use bd_core::mechanics::{Element, Recipe};
use bd_core::{ConstructId, IVec3, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

pub use data::*;

use crate::movement::MoveModifiers;

/// Most uneaten fruits in the world at once (the oldest is lost beyond it).
pub const MAX_FRUITS: usize = 16;
/// Removed terrain voxels remembered for Crazy Diamond.
pub const RESTORE_LOG: usize = 4096;
/// Most voxels one restore rebuilds.
pub const MAX_RESTORE_VOXELS: usize = 1500;
/// Own shattered constructs remembered for restoring (and for how long).
pub const SHATTER_LOG: usize = 8;
pub const SHATTER_RESTORE_SECS: f32 = 10.0;
/// A fresh fruit can't be eaten for this long (lets it land).
pub const FRUIT_EAT_DELAY: f32 = 0.35;
/// Most rush tokens waiting to be punched.
pub const MAX_RUSH_QUEUE: u8 = 12;
/// Stand integrity (damage is mirrored to the user; the stand itself is kept whole).
pub const STAND_INTEGRITY: f32 = 400.0;

/// Bits of [`PowerSnap::flags`].
pub mod power_flags {
    /// Water / weakness drain active.
    pub const DRAINED: u8 = 1 << 0;
    /// Held by a seastone link.
    pub const SEASTONE: u8 = 1 << 1;
    pub const TRANSFORMED: u8 = 1 << 2;
    /// Time stop: grey the world (for everyone) — only this player is fast.
    pub const TIME_STOP: u8 = 1 << 3;
    /// Falling over the void (fruit users sink faster).
    pub const OVER_VOID: u8 = 1 << 4;
    /// The stand is out front rushing.
    pub const RUSH: u8 = 1 << 5;
    /// Logia scatter is ready.
    pub const SCATTER_READY: u8 = 1 << 6;
}

/// Replicated per-player power state (in `PlayerSnap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PowerSnap {
    /// Power table index + 1 (0 = none).
    pub id: u8,
    pub flags: u8,
}

impl PowerSnap {
    pub fn power(&self) -> Option<u16> {
        self.id.checked_sub(1).map(u16::from)
    }
    pub fn key(&self) -> Option<&'static str> {
        self.power().and_then(|i| PowerTable::get().def(i)).map(|d| d.id.as_str())
    }
    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }
}

/// Per-player power state (lives on [`crate::world::Player`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerPower {
    /// Index into [`PowerTable::get`].
    pub id: Option<u16>,
    /// Seconds of Water drain left.
    pub drained: f32,
    /// Currently held by a seastone link.
    pub seastone: bool,
    /// Hit by a seastone Link construct: becomes `seastone` once the link exists.
    pub seastone_mark: f32,
    pub scatter_cd: f32,
    pub move_cd: [f32; 2],
    /// Seconds of time stop left and the user's speed factor.
    pub time_stop: f32,
    pub time_speed: f32,
    pub transformed: bool,
    /// Transform = a piloted construct (Kaido's dragon).
    pub pilot: Option<ConstructId>,
    /// The stand.
    pub companion: Option<ConstructId>,
    pub resummon: f32,
    /// Stand out front (rushing) for this long.
    pub rush_t: f32,
    pub rush_cd: f32,
    pub rushes: u32,
    /// Rush tokens typed faster than the max rush rate, thrown on the next ticks.
    pub rush_queue: u8,
    pub over_void: bool,
    /// Last feet position on the ground (void KOs respawn the fruit here).
    pub last_ground: Option<Vec3>,
    /// Pending landing slam (radius, damage, knockback, carve).
    pub slam: Option<[f32; 4]>,
    /// Ope Ope ROOM (field id).
    pub room: Option<u16>,
}

impl PlayerPower {
    pub fn def(&self) -> Option<&'static PowerDef> {
        self.id.and_then(|i| PowerTable::get().def(i))
    }

    pub fn is_fruit(&self) -> bool {
        self.def().is_some_and(|d| d.kind.is_fruit())
    }

    /// Powers work (not drained, not seastoned).
    pub fn active(&self) -> bool {
        self.id.is_some() && self.drained <= 0.0 && !self.seastone
    }

    /// Body in effect (transform body while transformed).
    pub fn body(&self) -> Option<&'static BodyDef> {
        let d = self.def()?;
        if self.transformed {
            if let Some(t) = d.transform.as_ref().filter(|t| t.pilot.is_none()) {
                return Some(&t.body);
            }
        }
        Some(&d.body)
    }

    /// Movement modifiers of the power (host and client prediction both call this).
    pub fn modify_movement(&self, grounded: bool, m: &mut MoveModifiers) {
        if self.id.is_none() {
            return;
        }
        if self.active() {
            if let Some(b) = self.body() {
                b.moves.apply(m);
            }
        }
        if self.time_stop > 0.0 {
            let k = if self.time_speed.is_finite() { self.time_speed.clamp(1.0, 3.0) } else { 2.0 };
            m.speed_mult *= k;
            m.accel_mult *= k;
            m.air_control *= k;
        }
        if self.over_void && !grounded && self.is_fruit() {
            m.gravity_scale *= PowerTable::get().rules.void_sink.clamp(1.0, 3.0);
        }
    }

    pub fn snap(&self) -> PowerSnap {
        use power_flags::*;
        let mut f = 0u8;
        let mut set = |b: u8, on: bool| {
            if on {
                f |= b;
            }
        };
        set(DRAINED, self.drained > 0.0);
        set(SEASTONE, self.seastone);
        set(TRANSFORMED, self.transformed);
        set(TIME_STOP, self.time_stop > 0.0);
        set(OVER_VOID, self.over_void);
        set(RUSH, self.rush_t > 0.0);
        set(SCATTER_READY, self.scatter_cd <= 0.0 && self.body().is_some_and(|b| b.scatter));
        PowerSnap { id: self.id.map_or(0, |i| (i + 1).min(255) as u8), flags: f }
    }

    /// Client mirror: take the replicated state (what [`Self::modify_movement`] reads).
    pub fn apply_snap(&mut self, s: PowerSnap) {
        use power_flags::*;
        self.id = s.power();
        self.drained = if s.has(DRAINED) { 1.0 } else { 0.0 };
        self.seastone = s.has(SEASTONE);
        self.transformed = s.has(TRANSFORMED);
        self.over_void = s.has(OVER_VOID);
        self.time_stop = if s.has(TIME_STOP) { 1.0 } else { 0.0 };
        self.time_speed = self.def().map_or(2.0, time_stop_speed);
        self.rush_t = if s.has(RUSH) { 0.1 } else { 0.0 };
    }
}

/// The speed factor of a power's time stop move (2.0 if none).
pub fn time_stop_speed(d: &PowerDef) -> f32 {
    d.moves
        .iter()
        .flat_map(|m| m.ops.iter())
        .find_map(|o| if let MoveOp::TimeStop { speed, .. } = o { Some(*speed) } else { None })
        .unwrap_or(2.0)
}

/// An uneaten fruit (a construct with `recipe.grants`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fruit {
    pub construct: ConstructId,
    pub power: u16,
    pub owner: PlayerId,
    pub last_pos: Vec3,
    pub age: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct Shattered {
    owner: PlayerId,
    recipe: Recipe,
    pos: Vec3,
    rot: bd_core::Quat,
    age: f32,
}

/// World-level power state (lives on [`crate::World::powers`]).
#[derive(Clone, Debug, PartialEq)]
pub struct PowerWorld {
    /// Set by `cast_incantation` for the duration of the cast.
    pub(crate) incanting: bool,
    /// Delivery applied by the last incanted cast: (from_fist, arm).
    pub(crate) delivered: Option<(bool, bool)>,
    pub fruits: Vec<Fruit>,
    restore_log: VecDeque<(IVec3, u8)>,
    shattered: Vec<Shattered>,
}

impl Default for PowerWorld {
    fn default() -> Self {
        Self {
            incanting: false,
            delivered: None,
            fruits: Vec::with_capacity(MAX_FRUITS),
            restore_log: VecDeque::with_capacity(RESTORE_LOG),
            shattered: Vec::with_capacity(SHATTER_LOG),
        }
    }
}

impl PowerWorld {
    /// Remember removed terrain (bounded ring; Crazy Diamond rebuilds from it).
    pub(crate) fn record_removed(&mut self, v: IVec3, m: u8) {
        if m == 0 {
            return;
        }
        if self.restore_log.len() >= RESTORE_LOG {
            self.restore_log.pop_front();
        }
        self.restore_log.push_back((v, m));
    }

    pub(crate) fn clear(&mut self) {
        self.incanting = false;
        self.delivered = None;
        self.fruits.clear();
        self.restore_log.clear();
        self.shattered.clear();
    }
}

/// Why a power command was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerError {
    UnknownPlayer,
    Dead,
    NoPower,
    /// Powers are toggled off in the lobby.
    Disabled,
    /// Water drain / seastone.
    Drained,
    NoSuchMove,
    Cooldown,
    NotEnoughWillpower,
    Busy,
    /// The move had nothing to act on (no anchor / target / room).
    NoTarget,
}

/// How a power was gained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GainHow {
    /// Ate a fruit construct.
    Ate,
    /// Called a stand (or the stand arrow picked one).
    Stand,
    /// Granted directly (tools / tests).
    Granted,
}

/// Why a hit did (almost) nothing to a power user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Shrug {
    /// Element immunity (rubber vs electric, fire vs fire…).
    Immune,
    /// Rubber: boing.
    Bounced,
    /// Bara Bara: the blade passed through.
    BladePassed,
    /// Bomu Bomu: explosions don't hurt.
    Blastproof,
}

/// Power moments for the client (juice, HUD, the grey world, the stretching arm…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PowerEvent {
    Gained { player: PlayerId, power: u16, how: GainHow },
    /// Lost at KO / replaced.
    Lost { player: PlayerId, power: u16 },
    /// A fruit construct appeared (cast, or respawned where its eater died).
    FruitSpawned { construct: ConstructId, power: u16, pos: Vec3, respawn: bool },
    /// Ate a second fruit: self-KO blast.
    Overdose { player: PlayerId, pos: Vec3 },
    Drained { player: PlayerId, secs: f32 },
    Seastone { player: PlayerId, on: bool },
    /// Logia: dissolved into the element and reformed.
    Scattered { player: PlayerId, from: Vec3, to: Vec3, element: Element },
    Shrugged { player: PlayerId, why: Shrug, point: Vec3 },
    /// A move fired (slot 0 = tap, 1 = hold).
    Move { player: PlayerId, power: u16, slot: u8 },
    /// An incanted cast: draw the delivery (arm = stretching arm from the hand to `construct`).
    Delivered { player: PlayerId, construct: ConstructId, arm: bool, from_fist: bool },
    Blinked { player: PlayerId, from: Vec3, to: Vec3, afterimage: bool },
    Swapped { a: PlayerId, b_player: Option<PlayerId>, b_construct: Option<ConstructId>, a_pos: Vec3, b_pos: Vec3 },
    /// The world greys; only `player` moves (×speed) for `secs`.
    TimeStop { player: PlayerId, secs: f32 },
    Transformed { player: PlayerId, on: bool },
    /// One stand rush punch (`hits` players/constructs struck).
    StandRush { player: PlayerId, stand: ConstructId, point: Vec3, hits: u8 },
    /// Terrain rebuilt: clients set exactly these voxels ([`apply_restore`]).
    Restored { player: PlayerId, center: Vec3, radius: f32, voxels: Vec<(IVec3, u8)>, constructs: u8 },
}

/// Client: apply a [`PowerEvent::Restored`] voxel list exactly as the host did.
pub fn apply_restore(voxels: &mut bd_voxel::VoxelWorld, list: &[(IVec3, u8)]) {
    for &(v, m) in list {
        if voxels.get(v) == 0 {
            voxels.set(v, m);
        }
    }
}


