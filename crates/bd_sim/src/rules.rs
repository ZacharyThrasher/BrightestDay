//! Match setup ([`MatchRules`], chosen in the lobby) and the FFA match state machine
//! ([`MatchState`]: Warmup → Live → Ended → rematch).
//!
//! * **Warmup** — free play; nothing is scored. Once at least `min_players` are present a countdown
//!   of `warmup_secs` runs, then the match goes Live (everyone respawns fresh, scores zeroed).
//! * **Live** — first to `ko_target` KOs wins, or most KOs when `time_limit` runs out (ties: fewer
//!   deaths, then lower player id).
//! * **Ended** — scoreboard; no damage. After `ended_secs` an automatic rematch returns to Warmup
//!   (terrain regenerated if `regen_islands_between_rounds`).

use bd_core::PlayerId;
use serde::{Deserialize, Serialize};

/// Construct size clamps (COMBAT.md §5e).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScaleLimit {
    /// Per-form min/max (constructs/forms.rs).
    #[default]
    Normal,
    /// Per-form max ×2.5.
    Huge,
    /// Up to [`NO_LIMITS_MAX_SCALE`] m for any form; at most one colossal construct per player.
    NoLimits,
}

/// Willpower economy mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WillpowerMode {
    #[default]
    Normal,
    /// Pool 200, regen ×2.
    Double,
    /// Costs and upkeep are free; the pool stays full.
    Infinite,
}

/// "No cap" for a [`ConstructLimits`] field (the world's `MAX_CONSTRUCTS` still bounds everything).
pub const UNLIMITED: u8 = u8::MAX;

/// Per-player construct caps by kind (COMBAT.md §3), chosen on the host's Match Rules page.
/// Casting over a cap replaces the caster's oldest of that kind. `0` = that kind can't be summoned:
/// the cast becomes the nearest allowed kind (a kaiju → a big creature; a colossal → just big) or,
/// when there is none, fizzles (no cost). [`UNLIMITED`] = no cap. Defaults = the shipped caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ConstructLimits {
    pub held: u8,
    pub worn: u8,
    pub structures: u8,
    pub objects: u8,
    /// Creatures, kaiju included.
    pub creatures: u8,
    /// Kaiju-form creatures (also count as creatures).
    pub kaiju: u8,
    /// Colossal constructs (scale ≥ [`COLOSSAL_SCALE`], no-limits lobbies). At the cap a new
    /// colossal cast is refused (`CastError::ColossalLimit`), as shipped.
    pub colossal: u8,
    /// All of a player's constructs together.
    pub total: u8,
    /// Match-wide: live giants (kaiju + colossal) on the map. A caster who already has a giant
    /// replaces their oldest; anyone else gets the downsized version.
    pub giants_on_map: u8,
}

impl Default for ConstructLimits {
    fn default() -> Self {
        Self { held: 1, worn: 1, structures: 3, objects: 2, creatures: 2, kaiju: 1, colossal: 1, total: UNLIMITED, giants_on_map: UNLIMITED }
    }
}

impl ConstructLimits {
    /// The old `CapMode::Chaos`: structure / object / creature / kaiju caps ×3.
    pub fn chaos() -> Self {
        Self { structures: 9, objects: 6, creatures: 6, kaiju: 3, ..Self::default() }
    }

    /// Cap as a count (`UNLIMITED` → `usize::MAX`).
    pub fn n(v: u8) -> usize {
        if v == UNLIMITED { usize::MAX } else { v as usize }
    }
}

/// How a match is won (COMBAT.md §1a).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scoring {
    /// Most KOs when the clock runs out (or first to `ko_target`). Default FFA; party rounds.
    #[default]
    TimedKos,
    /// `lives` stocks each; last player standing wins (the clock still caps the match).
    Stock { lives: u8 },
}

/// Largest construct scale in [`ScaleLimit::NoLimits`] (m).
pub const NO_LIMITS_MAX_SCALE: f32 = 60.0;
/// A construct at or above this scale is "colossal" (≤ 1 per player).
pub const COLOSSAL_SCALE: f32 = 24.0;

/// Lobby-chosen rules. Part of match setup; replicate once on join.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchRules {
    pub scoring: Scoring,
    pub scale: ScaleLimit,
    pub willpower: WillpowerMode,
    /// KOs needed to win.
    pub ko_target: u16,
    /// Match length in seconds.
    pub time_limit: f32,
    /// Regenerate the arena voxels on rematch (always in no-limits lobbies).
    pub regen_islands_between_rounds: bool,
    // ---- lobby knobs added with GameSettings (settings.rs); defaults = the shipped rules ----
    /// Seconds dead before respawning (COMBAT.md §1: 3 s).
    pub respawn_secs: f32,
    /// Spawn shield seconds (can't deal or take damage; §1: 1.5 s).
    pub spawn_shield_secs: f32,
    /// Multiplies every knockback players take (1 = shipped).
    pub knockback_scale: f32,
    /// Falling into the void KOs (true). Off: you are put back on a spawn point, no KO.
    pub void_ko: bool,
    /// Player powers (COMBAT.md §5h) — master toggle, read by the powers module.
    pub powers_enabled: bool,
    /// §5h devil fruits allowed.
    pub devil_fruits: bool,
    /// §5h stands allowed.
    pub stands: bool,
    /// ASSIST HOOK: tethers hook players (hook-and-strike, `crate::hook`; COMBAT.md "Hook-and-strike").
    pub tether_hooks_players: bool,
    /// CAMERA: players may use the over-the-shoulder camera (off = first-person-only lobby; seats
    /// in vehicles keep their chase camera). Client-side view rule; the sim never reads it.
    pub allow_third_person: bool,
    /// CONSTRUCTS: per-kind caps (replace-oldest; 0 = that kind is off). Replaced the old
    /// Normal / Chaos `caps` switch (Chaos = [`ConstructLimits::chaos`]).
    pub limits: ConstructLimits,
}

impl Default for MatchRules {
    fn default() -> Self {
        Self {
            scoring: Scoring::TimedKos,
            scale: ScaleLimit::Normal,
            willpower: WillpowerMode::Normal,
            ko_target: 10,
            time_limit: 8.0 * 60.0,
            regen_islands_between_rounds: true,
            respawn_secs: crate::combat::RESPAWN_SECS,
            spawn_shield_secs: crate::combat::SPAWN_SHIELD_SECS,
            knockback_scale: 1.0,
            void_ko: true,
            powers_enabled: true,
            devil_fruits: true,
            stands: true,
            tether_hooks_players: true,
            allow_third_person: true,
            limits: ConstructLimits::default(),
        }
    }
}

impl MatchRules {
    /// Maximum willpower.
    pub fn willpower_max(&self) -> f32 {
        match self.willpower {
            WillpowerMode::Double => 200.0,
            _ => 100.0,
        }
    }

    /// Passive regen multiplier.
    pub fn regen_mult(&self) -> f32 {
        match self.willpower {
            WillpowerMode::Double => 2.0,
            _ => 1.0,
        }
    }

    pub fn infinite_willpower(&self) -> bool {
        self.willpower == WillpowerMode::Infinite
    }

    /// Max live constructs per player in `slot` (`usize::MAX` = unlimited; 0 = off).
    pub fn slot_limit(&self, slot: crate::constructs::Slot) -> usize {
        use crate::constructs::Slot;
        let l = &self.limits;
        ConstructLimits::n(match slot {
            Slot::Held => l.held,
            Slot::Worn => l.worn,
            Slot::Structure => l.structures,
            Slot::Object => l.objects,
            Slot::Creature => l.creatures,
        })
    }

    /// Max live kaiju per player.
    pub fn kaiju_limit(&self) -> usize {
        ConstructLimits::n(self.limits.kaiju)
    }

    /// Max live colossal constructs per player.
    pub fn colossal_limit(&self) -> usize {
        ConstructLimits::n(self.limits.colossal)
    }

    /// Max live constructs per player, all kinds.
    pub fn total_limit(&self) -> usize {
        ConstructLimits::n(self.limits.total)
    }

    /// Max live giants (kaiju + colossal) on the whole map.
    pub fn giants_on_map_limit(&self) -> usize {
        ConstructLimits::n(self.limits.giants_on_map)
    }

    /// Scale clamp for a form with table range `[min, max]`.
    pub fn scale_range(&self, min: f32, max: f32) -> (f32, f32) {
        match self.scale {
            ScaleLimit::Normal => (min, max),
            ScaleLimit::Huge => (min, max * 2.5),
            ScaleLimit::NoLimits => (min, NO_LIMITS_MAX_SCALE.max(max)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum MatchPhase {
    #[default]
    Warmup = 0,
    Live = 1,
    Ended = 2,
}

/// Timings of the state machine that are not lobby rules.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchTimings {
    /// Warmup countdown once enough players are present.
    pub warmup_secs: f32,
    pub min_players: usize,
    /// Scoreboard time after the match ends.
    pub ended_secs: f32,
    /// Return to Warmup automatically after `ended_secs`.
    pub auto_rematch: bool,
}

impl Default for MatchTimings {
    fn default() -> Self {
        Self { warmup_secs: 10.0, min_players: 2, ended_secs: 12.0, auto_rematch: true }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MatchState {
    pub phase: MatchPhase,
    /// Seconds spent in the current phase (Warmup: only counts while enough players are present).
    pub phase_time: f32,
    pub winner: Option<PlayerId>,
    /// Incremented on every rematch.
    pub round: u32,
}

impl MatchState {
    pub fn live(&self) -> bool {
        self.phase == MatchPhase::Live
    }

    /// Seconds left in the current phase (Warmup countdown, Live clock, Ended scoreboard).
    pub fn time_left(&self, rules: &MatchRules, t: &MatchTimings) -> f32 {
        let total = match self.phase {
            MatchPhase::Warmup => t.warmup_secs,
            MatchPhase::Live => rules.time_limit,
            MatchPhase::Ended => t.ended_secs,
        };
        (total - self.phase_time).max(0.0)
    }
}

/// One scoreboard row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreRow {
    pub player: PlayerId,
    pub kos: u16,
    pub deaths: u16,
}

/// Sort a scoreboard best-first: KOs desc, deaths asc, id asc.
pub fn sort_scoreboard(rows: &mut [ScoreRow]) {
    rows.sort_by(|a, b| b.kos.cmp(&a.kos).then(a.deaths.cmp(&b.deaths)).then(a.player.cmp(&b.player)));
}
