//! Lobby settings (COMBAT.md §5g "Settings"): every gameplay knob in one serde struct with
//! presets, validation/clamping, and one place where it is wired into a [`SimConfig`].
//!
//! ```text
//! let settings = GameSettings::preset(Preset::Party).validated();
//! let cfg = settings.sim_config(SimConfig { arena, seed, ..SimConfig::default() });
//! let world = World::new(cfg);            // Party mode if settings.mode == Party
//! ```
//!
//! Where each knob lands:
//! * `rules` → [`SimConfig::rules`] ([`MatchRules`]: scale, willpower, caps, KO target, time limit,
//!   islands regen, respawn time, spawn shield, knockback scale, void on/off, power toggles).
//! * `gravity_scale` / `gas_regen_scale` → [`SimConfig::move_params`] (gravity; ground/air/tether
//!   gas regen). Clients predict with the same params, so feel stays consistent.
//! * `mode` + `party` → [`SimConfig::party`] (`None` = FFA). In Party mode the round KO target /
//!   round time replace `rules.ko_target` / `rules.time_limit`.
//!
//! Defaults reproduce the shipped FFA bit-for-bit (every scale is exactly 1.0).

use serde::{Deserialize, Serialize};

use crate::rules::{ConstructLimits, MatchRules, ScaleLimit, WillpowerMode};
use crate::world::SimConfig;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameMode {
    #[default]
    Ffa,
    Party,
}

/// Party mode knobs (COMBAT.md §5g).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PartySettings {
    /// Rounds in a party (1..=15).
    pub rounds: u8,
    /// KOs that end a round.
    pub round_ko_target: u16,
    /// Round length (s).
    pub round_time: f32,
    pub points_per_ko: u16,
    /// Bonus for the round winner(s) (most KOs, then fewest deaths; ties share it).
    pub points_round_win: u16,
    /// Pick phase length (s); unpicked on timeout = random.
    pub pick_secs: f32,
    /// Scoreboard pause after a round (s).
    pub round_end_secs: f32,
    /// Everyone sees what everyone took (s).
    pub reveal_secs: f32,
    /// Podium time before an automatic new party (s).
    pub party_end_secs: f32,
    /// Last place gets a 4th card: Boom or Bust.
    pub boom_or_bust: bool,
    /// First place only gets 2 choices.
    pub first_place_two_choices: bool,
    pub pool_common: bool,
    pub pool_legendary: bool,
    pub pool_cursed: bool,
}

impl Default for PartySettings {
    fn default() -> Self {
        Self {
            rounds: 7,
            round_ko_target: 5,
            round_time: 150.0,
            points_per_ko: 1,
            points_round_win: 3,
            pick_secs: 12.0,
            round_end_secs: 4.0,
            reveal_secs: 3.0,
            party_end_secs: 15.0,
            boom_or_bust: true,
            first_place_two_choices: true,
            pool_common: true,
            pool_legendary: true,
            pool_cursed: true,
        }
    }
}

impl PartySettings {
    /// Clamp every knob into a sane range (NaN → default).
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.rounds = self.rounds.clamp(1, 15);
        self.round_ko_target = self.round_ko_target.clamp(1, 100);
        self.round_time = fin(self.round_time, d.round_time).clamp(10.0, 1800.0);
        self.points_per_ko = self.points_per_ko.min(100);
        self.points_round_win = self.points_round_win.min(100);
        self.pick_secs = fin(self.pick_secs, d.pick_secs).clamp(2.0, 120.0);
        self.round_end_secs = fin(self.round_end_secs, d.round_end_secs).clamp(0.5, 60.0);
        self.reveal_secs = fin(self.reveal_secs, d.reveal_secs).clamp(0.5, 30.0);
        self.party_end_secs = fin(self.party_end_secs, d.party_end_secs).clamp(1.0, 300.0);
        self
    }
}

/// Named lobby presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Preset {
    /// The shipped FFA.
    Classic,
    /// Party mode with default knobs.
    Party,
    /// Party mode, no-limits scale, infinite willpower, chaos caps.
    Chaos,
    /// FFA with knockback ×1.5 and gas regen ×1.5 (more flying, more ring-outs).
    MovementGods,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Classic, Preset::Party, Preset::Chaos, Preset::MovementGods];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Classic => "Classic",
            Preset::Party => "Party",
            Preset::Chaos => "Chaos",
            Preset::MovementGods => "Movement Gods",
        }
    }
}

/// Every lobby knob.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    pub mode: GameMode,
    /// Match rules (see module docs).
    pub rules: MatchRules,
    /// Multiplies gravity (0.25..3).
    pub gravity_scale: f32,
    /// Multiplies gas regen on ground / in the air / on a tether (0..4).
    pub gas_regen_scale: f32,
    pub party: PartySettings,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self { mode: GameMode::Ffa, rules: MatchRules::default(), gravity_scale: 1.0, gas_regen_scale: 1.0, party: PartySettings::default() }
    }
}

fn fin(v: f32, d: f32) -> f32 {
    if v.is_finite() { v } else { d }
}

impl GameSettings {
    pub fn preset(p: Preset) -> Self {
        let mut s = Self::default();
        match p {
            Preset::Classic => {}
            Preset::Party => s.mode = GameMode::Party,
            Preset::Chaos => {
                s.mode = GameMode::Party;
                s.rules.scale = ScaleLimit::NoLimits;
                s.rules.willpower = WillpowerMode::Infinite;
                s.rules.limits = ConstructLimits::chaos();
                s.rules.regen_islands_between_rounds = true;
            }
            Preset::MovementGods => {
                s.rules.knockback_scale = 1.5;
                s.gas_regen_scale = 1.5;
            }
        }
        s
    }

    /// Clamp every knob (untrusted lobby data; NaN → default).
    pub fn validated(mut self) -> Self {
        let d = MatchRules::default();
        let r = &mut self.rules;
        r.ko_target = r.ko_target.clamp(1, 200);
        r.time_limit = fin(r.time_limit, d.time_limit).clamp(30.0, 3600.0);
        r.respawn_secs = fin(r.respawn_secs, d.respawn_secs).clamp(0.0, 30.0);
        r.spawn_shield_secs = fin(r.spawn_shield_secs, d.spawn_shield_secs).clamp(0.0, 10.0);
        r.knockback_scale = fin(r.knockback_scale, 1.0).clamp(0.0, 4.0);
        r.willpower_segments = r.willpower_segments.clamp(crate::rules::WILLPOWER_SEGMENTS_MIN, crate::rules::WILLPOWER_SEGMENTS_MAX);
        self.gravity_scale = fin(self.gravity_scale, 1.0).clamp(0.25, 3.0);
        self.gas_regen_scale = fin(self.gas_regen_scale, 1.0).clamp(0.0, 4.0);
        self.party = self.party.validated();
        self
    }

    /// `base` with these settings applied (validated first). `base.move_params` is taken as the
    /// unscaled tuning; defaults leave `base` exactly as it was (FFA, every scale 1.0).
    pub fn sim_config(&self, mut base: SimConfig) -> SimConfig {
        let s = self.clone().validated();
        base.rules = s.rules.clone();
        let mp = &mut base.move_params;
        mp.gravity *= s.gravity_scale;
        mp.gas_regen_ground *= s.gas_regen_scale;
        mp.gas_regen_air *= s.gas_regen_scale;
        mp.gas_regen_tethered *= s.gas_regen_scale;
        base.party = match s.mode {
            GameMode::Ffa => None,
            GameMode::Party => Some(s.party.clone()),
        };
        base
    }

    /// RON for saving presets / lobby files.
    pub fn to_ron(&self) -> Result<String, String> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).map_err(|e| e.to_string())
    }

    /// Parse (partial files fill in defaults) and validate.
    pub fn from_ron(src: &str) -> Result<Self, String> {
        ron::from_str::<Self>(src).map(Self::validated).map_err(|e| e.to_string())
    }
}
