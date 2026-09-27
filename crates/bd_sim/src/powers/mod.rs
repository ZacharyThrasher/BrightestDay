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
//!   [`PowerTable::id_for_name`]; `"random:devil_fruit"` / `"random:stand"` / `"random:mutagen"` /
//!   `"random:origin"` = the sim rolls one from that pool only). Casting such a recipe with
//!   [`crate::World::cast`] makes a pickup construct - a fruit or an origin (anyone who touches it
//!   takes it) - or calls the stand (held until KO).
//! * Before resolving a typed/spoken phrase, the client calls [`parse_phrase`] with the local
//!   player's power (`PlayerSnap::power.power()`): `Move(slot)` → `power_move`, `Incantation {rest}`
//!   → resolve `rest` with the lexicon and call [`crate::World::cast_incantation`], `Plain` → cast.
//! * While the ring console is open, every rush token the client counts
//!   ([`PowerTable::is_rush_token`]: ORA / MUDA / DORA / ARI / WRYY…) → `stand_rush_token`.
//! * State: `PlayerSnap::power` ([`PowerSnap`]); moments: `GameEvent::Power(`[`PowerEvent`]`)`.
//!   Clients mirroring a world for prediction copy `PowerSnap` into `Player::power` with
//!   [`PlayerPower::apply_snap`] so [`PlayerPower::modify_movement`] predicts identically.

mod data;
pub mod effects;
mod exec;
mod hooks;
/// Generic move ops (docs/toolbox/OPS.md), one file each.
pub mod ops;
#[cfg(test)]
mod tests;
pub mod chain;
pub mod toolbox;
/// VERB GRAMMAR: describe-a-power composed moves (docs/toolbox/COMPOSED_MOVE.md).
pub mod compose;
/// MODIFIERS & FUSION: twisted / fused moves ("giant rasengan", "rasengan + amaterasu").
pub mod twist;

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
const MAX_PLAYERS_HINT: usize = 16;
/// Grants key of the plain "power up" / "full power" phrase: the Ascend moment in your corps
/// colour, nothing else (cosmetic; the lexicon maps the phrases to it).
pub const ASCEND_KEY: &str = "ascend:power_up";
/// What a cast that made no construct returns (a technique learned, a power selected, a power-up).
pub const NO_CONSTRUCT: ConstructId = ConstructId(u32::MAX);

/// `"super_saiyan@4"` → (`"super_saiyan"`, `Some(4)`): a grants key may name a tier (the lexicon
/// writes it for a technique's tier names).
pub fn split_tier(key: &str) -> (&str, Option<u8>) {
    match key.rsplit_once('@') {
        Some((k, t)) => (k, t.parse::<u8>().ok().map(|t| t.min(15))),
        None => (key, None),
    }
}
/// Powers held besides the active one (`PowerRules::max_powers` - 1, at most this).
pub const MAX_EXTRA_POWERS: usize = 2;

/// `World::power_move` slot selectors (one byte on the wire; the ring's power phrases and the G key
/// produce them). Plain `0..=7` = the active power's moves (G tap = 0, G hold = 1; 2.. are
/// reached by their shouts only).
pub mod move_sel {
    pub const TAP: u8 = 0;
    pub const HOLD: u8 = 1;
    /// Most moves one power (or tier, or kit) has (TOOLBOX: 8).
    pub const MAX_MOVES: usize = 8;
    /// Make the next held power the active one (G double-tap).
    pub const CYCLE: u8 = 0x20;
    /// `SELECT | held << 4 | tier`: make held power `held` (0 = active, 1.. = the others in order)
    /// active and switch it to `tier` ([`KEEP_TIER`] = leave the tier alone).
    pub const SELECT: u8 = 0x80;
    pub const KEEP_TIER: u8 = 0x0F;
    /// `MOVE_OF | held << 3 | slot`: run move `slot` (0..=7) of held power `held` (1..=2) without
    /// swapping (shouts of a power that isn't active: moves from different powers chain).
    pub const MOVE_OF: u8 = 0x40;

    pub fn select(held: usize, tier: u8) -> u8 {
        SELECT | ((held.min(3) as u8) << 4) | tier.min(KEEP_TIER)
    }
    pub fn move_of(held: usize, slot: u8) -> u8 {
        MOVE_OF | ((held.min(3) as u8) << 3) | (slot & 7)
    }
    /// `(held, slot)` of a [`MOVE_OF`] selector (`(0, sel)` for a plain slot).
    pub fn split_move_of(sel: u8) -> (usize, u8) {
        if sel & MOVE_OF != 0 { (((sel >> 3) & 3) as usize, sel & 7) } else { (0, sel) }
    }
}

/// Bits of [`PowerSnap::flags`].
pub mod power_flags {
    /// Water / weakness drain active.
    pub const DRAINED: u8 = 1 << 0;
    /// Held by a seastone link.
    pub const SEASTONE: u8 = 1 << 1;
    pub const TRANSFORMED: u8 = 1 << 2;
    /// Time stop: grey the world (for everyone) — only this player is fast.
    pub const TIME_STOP: u8 = 1 << 3;
    /// Falling over the void (devil-fruit users sink faster; origins and stands don't).
    pub const OVER_VOID: u8 = 1 << 4;
    /// The stand is out front rushing.
    pub const RUSH: u8 = 1 << 5;
    /// Logia scatter is ready.
    pub const SCATTER_READY: u8 = 1 << 6;
    /// An auto-dodge (Ultra Instinct) is ready.
    pub const AUTO_DODGE_READY: u8 = 1 << 7;
}

/// Replicated per-player power state (in `PlayerSnap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PowerSnap {
    /// Active power: table index + 1 (0 = none). `u16`: the table holds thousands (TOOLBOX).
    pub id: u16,
    pub flags: u8,
    /// The other held powers (index + 1; 0 = empty), in held order.
    #[serde(default)]
    pub others: [u16; MAX_EXTRA_POWERS],
    /// Tier of the active power, then of each other (low 4 bits) | 0x80 = that other is transformed.
    #[serde(default)]
    pub tiers: [u8; 1 + MAX_EXTRA_POWERS],
}

impl PowerSnap {
    /// The active power.
    pub fn power(&self) -> Option<u16> {
        self.id.checked_sub(1)
    }
    /// Every held power: (id, tier, transformed), active first.
    pub fn held(&self) -> impl Iterator<Item = (u16, u8, bool)> + '_ {
        let active = self.power().map(|id| (id, self.tiers[0] & 0x0F, self.has(power_flags::TRANSFORMED)));
        active.into_iter().chain(self.others.iter().enumerate().filter_map(|(k, o)| o.checked_sub(1).map(|id| (id, self.tiers[k + 1] & 0x0F, self.tiers[k + 1] & 0x80 != 0))))
    }
    pub fn key(&self) -> Option<&'static str> {
        self.power().and_then(|i| PowerTable::get().def(i)).map(|d| d.id.as_str())
    }
    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }
}

/// A held power that isn't the active one: its own cooldowns, tier and form keep going (its body,
/// graft and effects stay on — powers MIX).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeldPower {
    pub id: u16,
    pub move_cd: [f32; move_sel::MAX_MOVES],
    pub transformed: bool,
    pub tier: u8,
    /// Gain order (the oldest is replaced first when the slots are full).
    pub since: u32,
}

/// One held power as seen by the mixers: (definition, tier, transformed).
#[derive(Clone, Copy, Debug)]
pub struct HeldView {
    pub id: u16,
    pub def: &'static PowerDef,
    pub tier: u8,
    pub transformed: bool,
    /// 0 = the active power, 1.. = the others in held order.
    pub slot: usize,
}

impl HeldView {
    /// The body in effect for this power: its tier's, else its (non-pilot) transform's, else its own.
    pub fn body(&self) -> &'static BodyDef {
        if let Some(b) = self.def.tier(self.tier).and_then(|t| t.body.as_ref()) {
            return b;
        }
        if self.transformed {
            if let Some(t) = self.def.transform.as_ref().filter(|t| t.pilot.is_none()) {
                return &t.body;
            }
        }
        &self.def.body
    }

    /// Standing grafts in effect (base, tier, transform).
    pub fn grafts(&self) -> impl Iterator<Item = &'static GraftDef> + use<> {
        let def = self.def;
        let t = def.tier(self.tier).and_then(|t| t.graft.as_ref());
        let x = if self.transformed { def.transform.as_ref().and_then(|t| t.graft.as_ref()) } else { None };
        def.graft.as_ref().into_iter().chain(t).chain(x)
    }

    /// Effects in effect (base, tier, transform).
    pub fn effects(&self) -> impl Iterator<Item = &'static PowerEffect> + use<> {
        let def = self.def;
        // A tier that lists effects REPLACES the base ones (tier numbers are totals: SSJ3 is ×1.45,
        // not ×1.15 × 1.45); a tier without effects keeps the base.
        let t: &'static [PowerEffect] = def.tier(self.tier).map(|t| t.effects.as_slice()).unwrap_or(&[]);
        let base: &'static [PowerEffect] = if t.is_empty() { &def.effects } else { &[] };
        let x: &'static [PowerEffect] = if self.transformed { def.transform.as_ref().map(|t| t.effects.as_slice()).unwrap_or(&[]) } else { &[] };
        base.iter().chain(t.iter()).chain(x.iter())
    }
}

/// Per-player power state (lives on [`crate::world::Player`]). A player holds up to
/// `PowerRules::max_powers` powers: the ACTIVE one (`id`, `move_cd`, `transformed`, `tier` — what G
/// fires) plus [`Self::others`]. Every held power's body, graft and effects apply at once.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerPower {
    /// Index into [`PowerTable::get`] of the ACTIVE power.
    pub id: Option<u16>,
    /// Seconds of Water drain left (all powers off).
    pub drained: f32,
    /// Currently held by a seastone link.
    pub seastone: bool,
    /// Hit by a seastone Link construct: becomes `seastone` once the link exists.
    pub seastone_mark: f32,
    pub scatter_cd: f32,
    pub move_cd: [f32; move_sel::MAX_MOVES],
    /// Seconds of time stop left and the user's speed factor.
    pub time_stop: f32,
    pub time_speed: f32,
    pub transformed: bool,
    /// Tier of the active power (0 = base).
    pub tier: u8,
    /// Gain order of the active power.
    pub since: u32,
    /// The other held powers, in held order.
    pub others: [Option<HeldPower>; MAX_EXTRA_POWERS],
    /// Powers gained so far (orders [`HeldPower::since`]).
    pub gains: u32,
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
    /// Auto-dodge (Ultra Instinct) cooldown left.
    pub auto_dodge_cd: f32,
    /// A cosmetic "power up" can't be spammed faster than this.
    pub ascend_cd: f32,
    /// Live timed toolbox ops (Drill, MarkRecall, CopyLast memory, Phase, Rush, Contagion, Wave:
    /// `ops/live.rs`). Reset with the rest of this struct at KO / round start.
    pub live: ops::live::LiveOps,
    /// TOOLBOX: borrowed / kit move cooldowns, the pending chain, the active phase ([`chain`]).
    pub toolbox: chain::Toolbox,
}

impl PlayerPower {
    /// The ACTIVE power's definition.
    pub fn def(&self) -> Option<&'static PowerDef> {
        self.id.and_then(|i| PowerTable::get().def(i))
    }

    /// Every held power (active first). No allocation.
    pub fn held(&self) -> impl Iterator<Item = HeldView> + '_ {
        let t = PowerTable::get();
        let active = self.id.and_then(|id| t.def(id).map(|def| HeldView { id, def, tier: self.tier, transformed: self.transformed, slot: 0 }));
        active.into_iter().chain(self.others.iter().enumerate().filter_map(move |(k, o)| {
            let o = (*o)?;
            t.def(o.id).map(|def| HeldView { id: o.id, def, tier: o.tier, transformed: o.transformed, slot: k + 1 })
        }))
    }

    /// How many powers are held.
    pub fn count(&self) -> usize {
        self.id.map_or(0, |_| 1) + self.others.iter().flatten().count()
    }

    /// Held slot (0 = active) of power `id`.
    pub fn slot_of(&self, id: u16) -> Option<usize> {
        if self.id == Some(id) {
            return Some(0);
        }
        self.others.iter().position(|o| o.is_some_and(|o| o.id == id)).map(|k| k + 1)
    }

    /// The companion (stand) of whichever held power has one.
    pub fn companion_def(&self) -> Option<&'static CompanionDef> {
        self.held().find_map(|h| h.def.companion.as_ref())
    }

    /// Any held power is a devil fruit (void sinking, overdose, Water drain).
    pub fn is_fruit(&self) -> bool {
        self.held().any(|h| h.def.kind.is_fruit())
    }

    /// Powers work (not drained, not seastoned).
    pub fn active(&self) -> bool {
        self.id.is_some() && self.drained <= 0.0 && !self.seastone
    }

    /// Body of the ACTIVE power in effect (tier / transform body when on).
    pub fn body(&self) -> Option<&'static BodyDef> {
        self.held().next().map(|h| h.body())
    }

    /// Every effect of every held power (none while drained).
    pub fn effects(&self) -> impl Iterator<Item = &'static PowerEffect> + '_ {
        let on = self.active();
        self.held().filter(move |_| on).flat_map(|h| h.effects())
    }

    /// Make held slot `k` (1..) the active power (the old active takes its place).
    pub fn swap_active(&mut self, k: usize) -> bool {
        let Some(Some(o)) = self.others.get(k.wrapping_sub(1)).copied() else { return false };
        let old = self.id.map(|id| HeldPower { id, move_cd: self.move_cd, transformed: self.transformed, tier: self.tier, since: self.since });
        self.others[k - 1] = old;
        self.id = Some(o.id);
        self.move_cd = o.move_cd;
        self.transformed = o.transformed;
        self.tier = o.tier;
        self.since = o.since;
        self.time_speed = self.def().map_or(2.0, time_stop_speed);
        true
    }

    /// Movement modifiers of every held power (host and client prediction both call this).
    pub fn modify_movement(&self, grounded: bool, m: &mut MoveModifiers) {
        if self.id.is_none() {
            return;
        }
        if self.active() {
            for h in self.held() {
                h.body().moves.apply(m);
            }
            let k = effects::speed_mult(self);
            if k != 1.0 {
                m.speed_mult *= k;
                m.accel_mult *= k.max(1.0);
            }
            m.extra_air_jumps = m.extra_air_jumps.saturating_add(effects::extra_air_jumps(self));
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
        set(SCATTER_READY, self.scatter_cd <= 0.0 && self.held().any(|h| h.body().scatter));
        set(AUTO_DODGE_READY, self.auto_dodge_cd <= 0.0 && self.effects().any(|e| matches!(e, PowerEffect::AutoDodge { .. })));
        let enc = |id: u16| id.saturating_add(1);
        let mut others = [0u16; MAX_EXTRA_POWERS];
        let mut tiers = [0u8; 1 + MAX_EXTRA_POWERS];
        tiers[0] = self.tier.min(15);
        for (k, o) in self.others.iter().enumerate() {
            if let Some(o) = o {
                others[k] = enc(o.id);
                tiers[k + 1] = o.tier.min(15) | if o.transformed { 0x80 } else { 0 };
            }
        }
        PowerSnap { id: self.id.map_or(0, enc), flags: f, others, tiers }
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
        self.auto_dodge_cd = if s.has(AUTO_DODGE_READY) { 0.0 } else { 1.0 };
        self.tier = s.tiers[0] & 0x0F;
        for (k, o) in self.others.iter_mut().enumerate() {
            let keep = *o;
            *o = s.others[k].checked_sub(1).map(|id| HeldPower {
                id,
                move_cd: keep.filter(|o| o.id == id).map_or([0.0; move_sel::MAX_MOVES], |o| o.move_cd),
                transformed: s.tiers[k + 1] & 0x80 != 0,
                tier: s.tiers[k + 1] & 0x0F,
                since: 0,
            });
        }
    }

    /// What a typed / spoken phrase means for this player (client + host; see [`PowerPhrase`]):
    /// a shout of ANY held power (the active one first), naming a tier of a held power (→ that
    /// tier, made active), naming a held power again (→ make it active), or an incantation prefix.
    pub fn parse_phrase(&self, text: &str) -> PowerPhrase {
        let n = norm_words(text);
        if n.is_empty() || self.id.is_none() {
            return PowerPhrase::Plain;
        }
        let mut held: [Option<HeldView>; 1 + MAX_EXTRA_POWERS] = [None; 1 + MAX_EXTRA_POWERS];
        for (k, h) in self.held().enumerate().take(held.len()) {
            held[k] = Some(h);
        }
        // Shouts (tier shouts first: a tier may re-voice a move).
        for h in held.iter().flatten() {
            let tier_shouts = h.def.tier(h.tier).map(|t| t.shouts.as_slice()).unwrap_or(&[]);
            if let Some((_, slot)) = tier_shouts.iter().chain(h.def.shouts.iter()).find(|(s, _)| *s == n) {
                return PowerPhrase::Move(if h.slot == 0 { *slot & 7 } else { move_sel::move_of(h.slot, *slot) });
            }
        }
        // A tier of a held power, or the power's own name again (→ active; a tiered power → base).
        for h in held.iter().flatten() {
            if let Some(t) = h.def.tier_named(&n) {
                return PowerPhrase::Move(move_sel::select(h.slot, t));
            }
        }
        for h in held.iter().flatten() {
            if h.def.is_named(&n) {
                let tier = if h.def.tiers.is_empty() { move_sel::KEEP_TIER } else { 0 };
                return PowerPhrase::Move(move_sel::select(h.slot, tier));
            }
        }
        // Incantations: the longest prefix of any held power (tier prefixes included).
        let mut best: Option<&str> = None;
        for h in held.iter().flatten() {
            let tier_pre = h.def.tier(h.tier).map(|t| t.incantations.as_slice()).unwrap_or(&[]);
            for pre in tier_pre.iter().chain(h.def.incantations.iter()) {
                let fits = n.len() > pre.len() && n.starts_with(pre.as_str()) && n.as_bytes()[pre.len()] == b' ';
                if fits && best.is_none_or(|b| pre.len() > b.len()) {
                    best = Some(pre.as_str());
                }
            }
        }
        if let Some(pre) = best {
            let rest = n[pre.len() + 1..].trim();
            if !rest.is_empty() {
                return PowerPhrase::Incantation { rest: rest.to_string() };
            }
        }
        PowerPhrase::Plain
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

/// An uneaten fruit or ungrabbed origin pickup (a construct with `recipe.grants`).
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
    /// CAMERA: the host-validated aim `(player index, eye, dir)` of the power move being run
    /// (`World::power_move_aimed`); `None` outside it (ops then use the look).
    pub(crate) move_aim: Option<(usize, Vec3, Vec3)>,
    /// IMPACT FRAMES: the running move's guaranteed crit `(player index, mult, fx)`
    /// (`MoveDef::crit`); `None` outside it.
    pub(crate) move_crit: Option<(usize, f32, CritFx)>,
    pub fruits: Vec<Fruit>,
    restore_log: VecDeque<(IVec3, u8)>,
    shattered: Vec<Shattered>,
    /// Live gravity cores (`MoveOp::Singularity`).
    pub(crate) cores: Vec<effects::Core>,
    /// This tick's slow fields: (holder, centre, radius, factor) — `effects`.
    pub(crate) slow_fields: Vec<(PlayerId, Vec3, f32, f32)>,
    /// (player, power, tier) forms already awakened this round: the first Ascend of each is the
    /// big one.
    pub(crate) awakened: Vec<(PlayerId, u16, u8)>,
    /// State of the generic ops (`ops::OpState`: rams, hooks, stances, zones, rewind trails…).
    pub(crate) ops: ops::OpState,
}

impl Default for PowerWorld {
    fn default() -> Self {
        Self {
            incanting: false,
            delivered: None,
            move_aim: None,
            move_crit: None,
            fruits: Vec::with_capacity(MAX_FRUITS),
            restore_log: VecDeque::with_capacity(RESTORE_LOG),
            shattered: Vec::with_capacity(SHATTER_LOG),
            cores: Vec::new(),
            slow_fields: Vec::with_capacity(MAX_PLAYERS_HINT),
            awakened: Vec::new(),
            ops: ops::OpState::default(),
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
        self.cores.clear();
        self.slow_fields.clear();
        self.awakened.clear();
        self.ops.clear();
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
    /// Ate a fruit / grabbed an origin pickup.
    Ate,
    /// Called a stand (or the stand arrow picked one).
    Stand,
    /// Learned a technique by saying it.
    Learned,
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
    /// Ate a second devil fruit: self-KO blast (a second origin just replaces the first).
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
    /// The transformation moment (bd_game `present::ascend`): `power` + `tier` pick the data
    /// (`AscendDef`; `None` = a plain "power up" in corps colour). `awakening` = the first time this
    /// player took this form this round (the bigger one + a music stinger).
    Ascend { player: PlayerId, power: Option<u16>, tier: u8, transform: bool, awakening: bool },
    /// The active power / its tier changed (swap by name, G double-tap, a tier phrase).
    Selected { player: PlayerId, power: u16, tier: u8 },
    /// Ultra Instinct: the body moved on its own (`dir` = the sidestep).
    AutoDodged { player: PlayerId, from: Vec3, dir: Vec3 },
    /// A critical melee connect (Black Flash).
    Crit { attacker: PlayerId, target: PlayerId, point: Vec3, fx: CritFx, mult: f32 },
    /// Universal Pull: `target` (player or construct) yanked from `from` toward `player`.
    Yanked { player: PlayerId, target: Option<PlayerId>, construct: Option<ConstructId>, from: Vec3 },
    /// A gravity core (`construct`) started (`end == false`) or collapsed.
    Singularity { player: PlayerId, construct: ConstructId, pos: Vec3, radius: f32, secs: f32, end: bool },
    /// A generic op's moment (`ops::OpEvent`: ram slam, hook caught, parry, lock-on…).
    Op(ops::OpEvent),
    /// TOOLBOX: ANY move fired — held, borrowed or kit (docs/toolbox/PRESENTATION_API.md). `mv`
    /// resolves to the power + move data (`MoveRef::resolve(PowerTable::get())`: ops, juice, fx,
    /// tags); `op` = the op it leads with; `from` / `to` = the mover's feet before / after (a blink's
    /// arrival); `dir` = the aim it fired along; `step` = steps of the current chain fired before it;
    /// `sparks` = what a borrowed / kit move cost (0 for held moves).
    Fired {
        player: PlayerId,
        mv: toolbox::MoveRef,
        how: chain::FireHow,
        op: chain::OpKind,
        element: Element,
        from: Vec3,
        to: Vec3,
        dir: Vec3,
        step: u8,
        sparks: u8,
    },
    /// TOOLBOX: a pending chain was cut short (`why`: NotEnoughWillpower = out of sparks, Dead = KO,
    /// Busy = stunned or a new command).
    ChainStopped { player: PlayerId, why: PowerError },
    /// VERB GRAMMAR: a composed move fired ("shadow teleport behind ethan"; docs/toolbox/
    /// COMPOSED_MOVE.md). Sent right before its `Fired { how: Composed }`; `spec.expand()` (or
    /// `compose::interned(&spec)`) gives the `MoveDef` (ops, fx, name) to present.
    Composed { player: PlayerId, spec: compose::ComposedMove, from: Vec3, to: Vec3, dir: Vec3, step: u8, sparks: u8 },
}

/// Client: apply a [`PowerEvent::Restored`] voxel list exactly as the host did.
pub fn apply_restore(voxels: &mut bd_voxel::VoxelWorld, list: &[(IVec3, u8)]) {
    for &(v, m) in list {
        if voxels.get(v) == 0 {
            voxels.set(v, m);
        }
    }
}


