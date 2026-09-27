//! `Rewind { secs }` — snap back along your own recent path (Tracer's Recall, Ekko's Chronobreak,
//! a VHS rewind): you reappear where you were `secs` ago (≤ [`MAX_SECS`]) with the damage % you had
//! then **if it was lower** (never higher — a rewind is an escape, not a trap), moving slowly, out
//! of tumble, ropes dropped. The route you took is kept for the streak.
//!
//! The trail is always recorded (every [`SAMPLE_TICKS`] ticks, fixed arrays, no allocation) so a
//! borrowed rewind works the first time anyone says it. It is cleared on death, and the part you
//! rewound through is consumed (two rewinds in a row don't loop you).

use bd_core::{MAX_PLAYERS, PlayerId, Vec3};

use super::{OpEvent, flat};
use crate::powers::{PowerError, PowerEvent};
use crate::events::GameEvent;
use crate::world::World;

/// Ticks between trail samples (120 Hz → 0.1 s).
pub const SAMPLE_TICKS: u64 = 12;
/// Samples kept per player.
pub const SAMPLES: usize = 32;
/// Longest rewind honoured (s).
pub const MAX_SECS: f32 = 3.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Sample {
    pos: Vec3,
    pct: f32,
}

/// Every player's recent route (ring buffers).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Trails {
    ids: [Option<PlayerId>; MAX_PLAYERS],
    /// Next write slot and number of valid samples.
    head: [u8; MAX_PLAYERS],
    count: [u8; MAX_PLAYERS],
    samples: [[Sample; SAMPLES]; MAX_PLAYERS],
}

impl Default for Trails {
    fn default() -> Self {
        Self { ids: [None; MAX_PLAYERS], head: [0; MAX_PLAYERS], count: [0; MAX_PLAYERS], samples: [[Sample::default(); SAMPLES]; MAX_PLAYERS] }
    }
}

impl Trails {
    fn reset(&mut self, k: usize, id: Option<PlayerId>) {
        self.ids[k] = id;
        self.head[k] = 0;
        self.count[k] = 0;
    }

    fn push(&mut self, k: usize, s: Sample) {
        let h = self.head[k] as usize % SAMPLES;
        self.samples[k][h] = s;
        self.head[k] = ((h + 1) % SAMPLES) as u8;
        self.count[k] = (self.count[k] as usize + 1).min(SAMPLES) as u8;
    }

    /// The sample `back` steps before the newest (0 = newest).
    fn back(&self, k: usize, back: usize) -> Sample {
        let h = self.head[k] as usize + SAMPLES * 2 - 1 - back.min(SAMPLES - 1);
        self.samples[k][h % SAMPLES]
    }
}

impl World {
    /// Sample every living player's position + damage % (every [`SAMPLE_TICKS`] ticks).
    pub(crate) fn record_trails(&mut self) {
        if self.tick % SAMPLE_TICKS != 0 {
            return;
        }
        let n = self.players.len().min(MAX_PLAYERS);
        for k in 0..n {
            let p = &self.players[k];
            let (id, alive, pos, pct) = (p.id, p.alive, p.mv.pos, p.damage_pct);
            let tr = &mut self.powers.ops.trails;
            if tr.ids[k] != Some(id) || !alive {
                tr.reset(k, Some(id));
            }
            if alive && pos.is_finite() {
                tr.push(k, Sample { pos, pct });
            }
        }
        for k in n..MAX_PLAYERS {
            self.powers.ops.trails.reset(k, None);
        }
    }

    pub(crate) fn op_rewind(&mut self, i: usize, secs: f32) -> Result<(), PowerError> {
        let id = self.players[i].id;
        let tr = &self.powers.ops.trails;
        if tr.ids[i] != Some(id) || tr.count[i] < 2 {
            return Err(PowerError::NoTarget);
        }
        let want = ((secs.clamp(0.2, MAX_SECS) * 120.0 / SAMPLE_TICKS as f32).round() as usize).max(1);
        let back = want.min(tr.count[i] as usize - 1);
        let to_s = tr.back(i, back);
        // The streak: 8 points from the rewind target (oldest) to now.
        let mut path = [Vec3::ZERO; 8];
        for (n, slot) in path.iter_mut().enumerate() {
            let b = back - (back * n) / 7;
            *slot = tr.back(i, b).pos;
        }
        let from = self.players[i].mv.pos;
        let to = to_s.pos;
        if !self.in_play(to, 0.0) {
            return Err(PowerError::NoTarget);
        }
        // Consume the rewound stretch of trail.
        {
            let tr = &mut self.powers.ops.trails;
            tr.count[i] = tr.count[i].saturating_sub(back as u8).max(1);
            tr.head[i] = ((tr.head[i] as usize + SAMPLES * 2 - back) % SAMPLES) as u8;
        }
        let p = &mut self.players[i];
        let look = p.look();
        p.mv.pos = to;
        p.mv.vel = flat(look, Vec3::NEG_Z) * 1.5;
        p.mv.grounded = false;
        p.mv.tumble = 0.0;
        p.mv.release_tethers();
        p.launch = None;
        p.pending_splat = None;
        p.damage_pct = p.damage_pct.min(to_s.pct.max(0.0));
        self.inject_event(GameEvent::Power(PowerEvent::Blinked { player: id, from, to, afterimage: true }));
        self.op_event(OpEvent::Rewound { player: id, from, to, path });
        Ok(())
    }
}
