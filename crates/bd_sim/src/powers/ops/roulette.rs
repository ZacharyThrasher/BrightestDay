//! `Roulette { picks }` — one of several op lists, picked by a deterministic roll
//! (docs/toolbox/OPS.md): Pandora's box, a genie's wish, Loki's tricks, a mystery box.
//!
//! `picks` is a list of op lists (each one a whole little move). The roll is a hash of (tick,
//! player, number of picks): the same on every peer, no RNG stream touched. The chosen list runs
//! through the ordinary `run_op` (so a pick may be any op, generic ones included); a `Roulette`
//! nested inside a pick is skipped (no recursion). The move's own cost / cooldown / juice cover
//! the whole thing; `OpEvent::Roulette` names the pick for presentation.

use bd_core::PlayerId;

use crate::world::World;

/// Deterministic pick in `0..n` for `player` on `tick` (n ≥ 1).
pub fn roulette_roll(tick: u64, player: PlayerId, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in tick.to_le_bytes().iter().chain([player.0, n as u8, 0x9D].iter()) {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= h >> 29;
    (h % n as u64) as usize
}

impl World {
    /// The pick for player `i` among `n` lists (announces it).
    pub(crate) fn op_roulette_pick(&mut self, i: usize, n: usize) -> usize {
        let id = self.players[i].id;
        let k = roulette_roll(self.tick, id, n);
        self.op_event(super::OpEvent::Roulette { player: id, pick: k.min(255) as u8, of: n.min(255) as u8 });
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_is_spread_and_in_range() {
        let mut seen = [0u32; 5];
        for t in 0..500u64 {
            let k = roulette_roll(t, PlayerId(3), 5);
            assert!(k < 5);
            seen[k] += 1;
        }
        assert!(seen.iter().all(|&c| c > 50), "{seen:?}");
        assert_eq!(roulette_roll(7, PlayerId(1), 1), 0);
    }
}
