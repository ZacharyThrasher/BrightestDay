//! WORDLESS TAUNT + VOICE TELL — presentation state the host owns (no gameplay effect at all).
//!
//! * **Taunt** (`buttons::TAUNT`, default key Y): a *tap* plays the next body taunt of your rotation
//!   (flex → come-here beckon → shrug / brush-off → bow → …); *holding* the key past [`HOLD_SECS`]
//!   dances until you let go (at most [`DANCE_MAX`]). Taps start on release (a tap is ~0.1 s), so the
//!   dance never flashes a wrong taunt first.
//!   - Never locks you: the sim reads nothing here, and **any** other input (a move key, any button,
//!     the wheel, push-to-talk) ends the taunt on that tick ([`cancels`]).
//!   - Rate-limited on the host ([`TauntState::step`]): a bucket of [`BUCKET`] taunts refilling one per
//!     [`REFILL_SECS`], and at least [`MIN_GAP`] between starts — mashing the key can't fill a lobby
//!     with noise.
//!   - Only from a free body: alive, grounded, not tumbling / launched / seated / stunned, not reciting
//!     or charging.
//!   - Replicated as `GameEvent::Move` with the kind in `move_flags::TAUNT` (the start: sound + pose
//!     timer) and in `PlayerSnap::tell` ([`tell`]: what is playing now, so cancels and late joiners
//!     read right).
//! * **Voice tell** (`buttons::VOICE`): the client holds it while push-to-talk is listening for a cast
//!   in voice cast mode; the host copies it into `PlayerSnap::tell` so everyone sees the same casting
//!   body language as the typing tell (`pflags::TYPING`) — voice == typing.
//!
//! Nothing here writes movement, combat, willpower or the state hash: the FFA golden hash can't move.
//! The client runs the same [`TauntState`] on its own inputs for an instant local pose.

use bd_core::input::{PlayerInput, buttons};

/// Hold the taunt key this long (s) and the tap becomes a dance.
pub const HOLD_SECS: f32 = 0.3;
/// Taunts in the bucket (a full bucket = this many back to back, then one per refill).
pub const BUCKET: f32 = 2.0;
/// Seconds to earn back one taunt.
pub const REFILL_SECS: f32 = 3.0;
/// Least time between two taunt starts (s).
pub const MIN_GAP: f32 = 1.0;
/// Longest dance (s) per hold.
pub const DANCE_MAX: f32 = 4.0;

/// A wordless body taunt.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TauntKind {
    #[default]
    None = 0,
    /// Double-biceps flex, two pumps, a glance at the gun.
    Flex = 1,
    /// "Come here": the ring hand out, palm up, two beckons, chin up.
    Beckon = 2,
    /// Shrug, head tilt, then a dismissive brush-off flick.
    Shrug = 3,
    /// A theatrical sweeping bow.
    Bow = 4,
    /// A little bounce dance (held).
    Dance = 5,
}

impl TauntKind {
    /// The tap rotation.
    pub const TAPS: [TauntKind; 4] = [TauntKind::Flex, TauntKind::Beckon, TauntKind::Shrug, TauntKind::Bow];
    pub const ALL: [TauntKind; 5] = [TauntKind::Flex, TauntKind::Beckon, TauntKind::Shrug, TauntKind::Bow, TauntKind::Dance];

    pub fn from_bits(b: u8) -> Self {
        match b & 0b111 {
            1 => TauntKind::Flex,
            2 => TauntKind::Beckon,
            3 => TauntKind::Shrug,
            4 => TauntKind::Bow,
            5 => TauntKind::Dance,
            _ => TauntKind::None,
        }
    }

    pub fn bits(self) -> u8 {
        self as u8
    }

    pub fn is_some(self) -> bool {
        self != TauntKind::None
    }

    /// How long it plays (s). The dance plays while held, up to [`DANCE_MAX`].
    pub fn duration(self) -> f32 {
        match self {
            TauntKind::None => 0.0,
            TauntKind::Flex => 1.1,
            TauntKind::Beckon => 1.0,
            TauntKind::Shrug => 0.95,
            TauntKind::Bow => 1.25,
            TauntKind::Dance => DANCE_MAX,
        }
    }

    /// Sound id (`assets/sfx/manifest.ron`).
    pub fn sound(self) -> Option<&'static str> {
        Some(match self {
            TauntKind::None => return None,
            TauntKind::Flex => "taunt_flex",
            TauntKind::Beckon => "taunt_beckon",
            TauntKind::Shrug => "taunt_shrug",
            TauntKind::Bow => "taunt_bow",
            TauntKind::Dance => "taunt_dance",
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            TauntKind::None => "none",
            TauntKind::Flex => "flex",
            TauntKind::Beckon => "beckon",
            TauntKind::Shrug => "shrug",
            TauntKind::Bow => "bow",
            TauntKind::Dance => "dance",
        }
    }
}

/// Bits of `replicate::PlayerSnap::tell`.
pub mod tell {
    /// The taunt playing now ([`super::TauntKind`] bits; 0 = none).
    pub const KIND: u8 = 0b111;
    /// Speaking a cast (push-to-talk listening in voice cast mode).
    pub const VOICE: u8 = 1 << 3;
}

/// `PlayerSnap::tell` for a player.
pub fn tell_byte(alive: bool, taunt: TauntKind, voice: bool) -> u8 {
    if !alive {
        return 0;
    }
    (taunt.bits() & tell::KIND) | if voice { tell::VOICE } else { 0 }
}

/// Does this input end a taunt? Everything but the taunt key itself and mouse look.
pub fn cancels(i: &PlayerInput) -> bool {
    i.move_x != 0 || i.move_z != 0 || i.reel_notches != 0 || i.buttons & !buttons::TAUNT != 0
}

/// One player's taunt: what plays, the tap / hold detector, the rotation and the rate limiter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TauntState {
    /// Playing now (`None` = nothing).
    pub kind: TauntKind,
    /// Seconds since it started.
    pub t: f32,
    /// The key went down this long ago and no taunt has started from it yet.
    armed: Option<f32>,
    was_held: bool,
    /// Next tap taunt (index into [`TauntKind::TAPS`]).
    next: u8,
    /// Rate limiter: taunts available (≤ [`BUCKET`]) and time until the next may start.
    credit: f32,
    gap: f32,
}

impl Default for TauntState {
    fn default() -> Self {
        Self { kind: TauntKind::None, t: 0.0, armed: None, was_held: false, next: 0, credit: BUCKET, gap: 0.0 }
    }
}

impl TauntState {
    /// One tick. `free` = the body may taunt (alive, grounded, not tumbling / seated / stunned /
    /// reciting / charging). Returns the taunt that started this tick (for the event / sound).
    pub fn step(&mut self, input: &PlayerInput, free: bool, dt: f32) -> Option<TauntKind> {
        self.credit = (self.credit + dt / REFILL_SECS).min(BUCKET);
        self.gap = (self.gap - dt).max(0.0);
        let held = input.held(buttons::TAUNT);
        let pressed = held && !self.was_held;
        self.was_held = held;
        let cancel = cancels(input) || !free;
        if self.kind.is_some() {
            self.t += dt;
            let over = match self.kind {
                TauntKind::Dance => !held || self.t >= DANCE_MAX,
                k => self.t >= k.duration(),
            };
            if cancel || over {
                self.stop();
            }
        }
        if cancel {
            self.armed = None;
            return None;
        }
        if pressed {
            self.armed = Some(0.0);
        }
        let a = self.armed?;
        if held {
            let a = a + dt;
            if a < HOLD_SECS {
                self.armed = Some(a);
                return None;
            }
            self.armed = None;
            return self.try_start(TauntKind::Dance);
        }
        // Released: a tap.
        self.armed = None;
        let k = TauntKind::TAPS[self.next as usize % TauntKind::TAPS.len()];
        let started = self.try_start(k);
        if started.is_some() {
            self.next = (self.next + 1) % TauntKind::TAPS.len() as u8;
        }
        started
    }

    fn try_start(&mut self, k: TauntKind) -> Option<TauntKind> {
        if self.credit < 1.0 || self.gap > 0.0 {
            return None;
        }
        self.credit -= 1.0;
        self.gap = MIN_GAP;
        self.kind = k;
        self.t = 0.0;
        Some(k)
    }

    /// End whatever plays (death, respawn, a hit, a seat).
    pub fn stop(&mut self) {
        self.kind = TauntKind::None;
        self.t = 0.0;
    }

    /// Taunts available right now (for UIs / tests).
    pub fn credit(&self) -> f32 {
        self.credit
    }
}

/// Per tick on the host (after movement, so `grounded` is this tick's): every player's taunt, and
/// a `GameEvent::Move` with the kind in `move_flags::TAUNT` when one starts.
pub(crate) fn step(w: &mut crate::World, dt: f32) {
    let crate::World { players, events, .. } = w;
    for p in players.iter_mut() {
        let free = p.alive
            && p.mv.grounded
            && p.mv.tumble <= 0.0
            && p.launch.is_none()
            && p.seat.is_none()
            && !p.status.stunned()
            && !p.oath.reciting()
            && !p.charge.active;
        if !p.alive {
            // Nothing carries over a KO (the rate limiter does).
            p.taunt.stop();
            continue;
        }
        if let Some(k) = p.taunt.step(&p.input, free, dt) {
            events.push(crate::GameEvent::Move { player: p.id, flags: crate::events::move_flags::taunt(k), speed: 0.0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::DT;

    fn inp(b: u16) -> PlayerInput {
        PlayerInput { buttons: b, ..Default::default() }
    }

    fn run(s: &mut TauntState, b: u16, ticks: usize) -> Vec<TauntKind> {
        (0..ticks).filter_map(|_| s.step(&inp(b), true, DT)).collect()
    }

    #[test]
    fn tap_cycles_the_rotation_on_release() {
        let mut s = TauntState::default();
        assert!(run(&mut s, buttons::TAUNT, 6).is_empty(), "nothing while the key is down");
        assert_eq!(run(&mut s, 0, 1), vec![TauntKind::Flex]);
        run(&mut s, 0, (4.0 / DT) as usize);
        run(&mut s, buttons::TAUNT, 3);
        assert_eq!(run(&mut s, 0, 1), vec![TauntKind::Beckon]);
    }

    #[test]
    fn hold_dances_while_held() {
        let mut s = TauntState::default();
        let started = run(&mut s, buttons::TAUNT, (1.0 / DT) as usize);
        assert_eq!(started, vec![TauntKind::Dance]);
        assert_eq!(s.kind, TauntKind::Dance);
        run(&mut s, 0, 1);
        assert_eq!(s.kind, TauntKind::None, "letting go ends the dance");
    }

    #[test]
    fn tell_bits() {
        assert_eq!(TauntKind::from_bits(tell_byte(true, TauntKind::Bow, true) & tell::KIND), TauntKind::Bow);
        assert_ne!(tell_byte(true, TauntKind::None, true) & tell::VOICE, 0);
        assert_eq!(tell_byte(false, TauntKind::Bow, true), 0);
        for k in TauntKind::ALL {
            assert_eq!(TauntKind::from_bits(k.bits()), k);
            assert!(k.sound().is_some());
        }
    }
}
