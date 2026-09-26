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

/// Per-player construct caps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapMode {
    #[default]
    Normal,
    /// Structure/object/creature limits ×3 (held/worn stay 1).
    Chaos,
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
    pub caps: CapMode,
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
}

impl Default for MatchRules {
    fn default() -> Self {
        Self {
            scoring: Scoring::TimedKos,
            scale: ScaleLimit::Normal,
            willpower: WillpowerMode::Normal,
            caps: CapMode::Normal,
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

    /// Multiplier on per-slot limits (held/worn excluded).
    pub fn cap_mult(&self) -> usize {
        match self.caps {
            CapMode::Normal => 1,
            CapMode::Chaos => 3,
        }
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
