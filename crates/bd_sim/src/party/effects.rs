//! Party effect data (`assets/party_effects.ron`, COMBAT.md §5g): id, name, rarity, a ≤ 6-word
//! description, a [`StandingGraft`] merged into every cast, and a small closed set of [`Hook`]s
//! implemented in `party/hooks.rs`. Effects are behaviour, never numbers.
//!
//! Host and clients load the same file, so an effect is referenced on the wire by its index in the
//! table ([`EffectId`]).

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use super::graft::StandingGraft;

/// Index into [`EffectTable::effects`].
pub type EffectId = u16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    #[default]
    Common,
    /// Boom (Boom-or-Bust win).
    Legendary,
    /// Bust (Boom-or-Bust loss).
    Cursed,
}

/// Event hooks (closed set; data picks which an effect uses and their few parameters).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Hook {
    /// Party Popper: shattered constructs burst in a confetti shove (knockback, no damage).
    Confetti,
    /// Portal Pals: a shattered construct leaves a portal pair (where it broke ↔ in front of you).
    PortalPair,
    /// Black Hole Sun: a shattered construct collapses into a brief Attract field, then pops.
    BlackHole,
    /// Main Character: the first lethal hit splits the construct in two instead of shattering it.
    MainCharacter,
    /// Echo: every cast repeats `delay` s later as a free copy (never held / worn).
    Echo { delay: f32 },
    /// Mini-Me: every cast also makes a tiny flying copy that follows it.
    MiniMe,
    /// Growth Spurt: constructs grow a step every `every` s of their life (up to ×2).
    GrowthSpurt { every: f32 },
    /// Kaiju Heart: creatures cast a size class bigger per KO this round; at 3 they are Kaiju.
    KaijuHeart,
    /// Wild Magic: `chance` of each cast resolves as a random phrase.
    WildMagic { chance: f32 },
    /// Butterfingers: the held item slips out of your hand and flies off every ~`mean_secs`.
    Butterfingers { mean_secs: f32 },
    /// Piñata: getting hit spills a small random construct for the hitter.
    Pinata,
    /// Reverse Card: your projectiles curve back toward you after `after` s.
    ReverseCard { after: f32 },
    /// Slippery: ice under your feet (ground friction almost gone).
    Slippery,
    /// Loud: your constructs honk (a periodic fx event) and can't sneak.
    Loud,
    /// Copycat: an enemy construct that hits you is copied for you (6 s cooldown).
    Copycat,
    /// Sidekick: a small Protective spirit keeps re-forming at your side.
    Sidekick,
    /// Last Words: where you are KO'd, a bomb goes off a second later.
    LastWords,
    /// Popcorn: your projectiles pop into three bouncing kernels when they hit terrain.
    Popcorn,
    /// Magnetic Personality: everyone else's loose constructs drift toward you.
    MagneticPersonality,
    /// Hiccups: every ~`mean_secs` you hop involuntarily.
    Hiccups { mean_secs: f32 },
}

impl Hook {
    /// Bit in a player's hook set.
    pub fn bit(&self) -> u32 {
        1 << match self {
            Hook::Confetti => 0,
            Hook::PortalPair => 1,
            Hook::BlackHole => 2,
            Hook::MainCharacter => 3,
            Hook::Echo { .. } => 4,
            Hook::MiniMe => 5,
            Hook::GrowthSpurt { .. } => 6,
            Hook::KaijuHeart => 7,
            Hook::WildMagic { .. } => 8,
            Hook::Butterfingers { .. } => 9,
            Hook::Pinata => 10,
            Hook::ReverseCard { .. } => 11,
            Hook::Slippery => 12,
            Hook::Loud => 13,
            Hook::Copycat => 14,
            Hook::Sidekick => 15,
            Hook::LastWords => 16,
            Hook::Popcorn => 17,
            Hook::MagneticPersonality => 18,
            Hook::Hiccups { .. } => 19,
        }
    }
}

/// Hook-set bits (see [`Hook::bit`]).
pub mod hook_bits {
    pub const CONFETTI: u32 = 1 << 0;
    pub const PORTAL_PAIR: u32 = 1 << 1;
    pub const BLACK_HOLE: u32 = 1 << 2;
    pub const MAIN_CHARACTER: u32 = 1 << 3;
    pub const ECHO: u32 = 1 << 4;
    pub const MINI_ME: u32 = 1 << 5;
    pub const GROWTH_SPURT: u32 = 1 << 6;
    pub const KAIJU_HEART: u32 = 1 << 7;
    pub const WILD_MAGIC: u32 = 1 << 8;
    pub const BUTTERFINGERS: u32 = 1 << 9;
    pub const PINATA: u32 = 1 << 10;
    pub const REVERSE_CARD: u32 = 1 << 11;
    pub const SLIPPERY: u32 = 1 << 12;
    pub const LOUD: u32 = 1 << 13;
    pub const COPYCAT: u32 = 1 << 14;
    pub const SIDEKICK: u32 = 1 << 15;
    pub const LAST_WORDS: u32 = 1 << 16;
    pub const POPCORN: u32 = 1 << 17;
    pub const MAGNETIC: u32 = 1 << 18;
    pub const HICCUPS: u32 = 1 << 19;
    /// Hooks that act when one of the owner's constructs shatters.
    pub const ON_SHATTER: u32 = CONFETTI | PORTAL_PAIR | BLACK_HOLE | MAIN_CHARACTER;
}

/// One party effect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectSpec {
    /// Stable key (`"bouncy_castle"`).
    pub id: String,
    pub name: String,
    pub rarity: Rarity,
    /// ≤ 6 words, shown on the pick card.
    pub desc: String,
    #[serde(default)]
    pub graft: StandingGraft,
    #[serde(default)]
    pub hooks: Vec<Hook>,
}

impl EffectSpec {
    pub fn hook_bits(&self) -> u32 {
        self.hooks.iter().fold(0, |b, h| b | h.bit())
    }

    pub fn hook<T>(&self, f: impl Fn(&Hook) -> Option<T>) -> Option<T> {
        self.hooks.iter().find_map(f)
    }
}

/// The effect table.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectTable {
    pub effects: Vec<EffectSpec>,
}

/// The shipped table source.
pub const SHIPPED_RON: &str = include_str!("../../../../assets/party_effects.ron");

impl EffectTable {
    /// Parse a table (a RON list of [`EffectSpec`]). Duplicate ids and over-long descriptions are
    /// rejected so the data stays honest.
    pub fn from_ron(src: &str) -> Result<Self, String> {
        let effects: Vec<EffectSpec> = ron::from_str(src).map_err(|e| e.to_string())?;
        if effects.len() >= u16::MAX as usize {
            return Err("too many effects".into());
        }
        for (i, e) in effects.iter().enumerate() {
            if effects[..i].iter().any(|o| o.id == e.id) {
                return Err(format!("duplicate effect id {}", e.id));
            }
            if e.desc.split_whitespace().count() > 6 {
                return Err(format!("{}: description longer than 6 words", e.id));
            }
        }
        Ok(Self { effects })
    }

    /// The shipped table, parsed once (empty if the shipped file were ever broken — tests catch it).
    pub fn shipped() -> Arc<EffectTable> {
        static T: OnceLock<Arc<EffectTable>> = OnceLock::new();
        T.get_or_init(|| Arc::new(Self::from_ron(SHIPPED_RON).unwrap_or_default())).clone()
    }

    pub fn get(&self, id: EffectId) -> Option<&EffectSpec> {
        self.effects.get(id as usize)
    }

    pub fn find(&self, key: &str) -> Option<EffectId> {
        self.effects.iter().position(|e| e.id == key).map(|i| i as EffectId)
    }

    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }
}
