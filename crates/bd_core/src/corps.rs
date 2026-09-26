//! Lantern corps. Each player picks one; it is their colour for everything they construct.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Corps {
    Willpower,
    Rage,
    Greed,
    Fear,
    Hope,
    Compassion,
    Love,
    Life,
}

impl Corps {
    pub const ALL: [Corps; 8] = [
        Corps::Willpower,
        Corps::Rage,
        Corps::Greed,
        Corps::Fear,
        Corps::Hope,
        Corps::Compassion,
        Corps::Love,
        Corps::Life,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Corps::Willpower => "Willpower",
            Corps::Rage => "Rage",
            Corps::Greed => "Greed",
            Corps::Fear => "Fear",
            Corps::Hope => "Hope",
            Corps::Compassion => "Compassion",
            Corps::Love => "Love",
            Corps::Life => "Life",
        }
    }

    /// Linear-ish sRGB construct colour (0..1). Saturated so it blooms well.
    pub fn color(self) -> [f32; 3] {
        match self {
            Corps::Willpower => [0.20, 1.00, 0.35],
            Corps::Rage => [1.00, 0.12, 0.10],
            Corps::Greed => [1.00, 0.50, 0.05],
            Corps::Fear => [1.00, 0.90, 0.10],
            Corps::Hope => [0.15, 0.55, 1.00],
            Corps::Compassion => [0.35, 0.20, 1.00],
            Corps::Love => [0.95, 0.25, 0.85],
            Corps::Life => [1.00, 1.00, 1.00],
        }
    }

    /// Index 0..8, stable (used on the wire and for spawn order).
    pub fn index(self) -> u8 {
        Corps::ALL.iter().position(|c| *c == self).unwrap() as u8
    }

    pub fn from_index(i: u8) -> Corps {
        Corps::ALL[(i as usize) % 8]
    }
}
