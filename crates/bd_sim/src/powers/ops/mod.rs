//! Generic move ops (docs/toolbox/OPS.md) — one file per op, named for its BEHAVIOUR, never for a
//! franchise: `Rewind` is Tracer's Recall and Ekko's Chronobreak, `Ram` is Reinhardt's Charge and
//! Doomfist's Rocket Punch, `Hook` is Scorpion's spear and Blitzcrank's grab…
//!
//! Registration lives in three commented lines elsewhere: the `MoveOp` variants (data.rs), the
//! `run_op` arm that forwards here ([`World::run_generic_op`]) and the per-tick step
//! ([`World::step_generic_ops`], first thing in `step_powers`). Hits reach the stances through
//! [`World::ops_on_hit`] (top of `powers_on_hit`).
//!
//! Every op is deterministic, allocation-free per tick (fixed arrays indexed by player index and
//! validated by id) and does nothing at all while nobody uses one (the FFA golden hash never
//! moves: the only always-on work is the Rewind trail, which the state hash doesn't read).
//!
//! Chains (BRIEF §3): [`active_secs`] is how long an op's own active phase lasts when it starts,
//! and [`World::op_busy`] says whether a player still has one running (a ram in flight, a hook out,
//! a grab held, a lock-on channel, a counter window) — the next link of a chain waits on it.

use bd_core::mechanics::Element;
use bd_core::{MAX_PLAYERS, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use super::{At, MoveOp, PowerError, PowerEvent};
use crate::combat::HitInfo;
use crate::events::GameEvent;
use crate::status::StatusKind;
use crate::world::World;

mod counter;
mod deflect;
mod grab;
mod hook;
mod lock_on;
mod lockout;
mod ram;
mod rewind;
mod zone;
// MOVIES & COMICS
mod afflict;
mod barrage;
mod blink_carry;
mod bounce_beam;
mod delayed_hit;
mod disarm;
// MYTHOLOGY & FOLKLORE
mod haul;
pub mod leap;
pub mod roulette;
mod seeker;
mod self_resize;
mod surge;
pub use haul::HaulPhase;
pub use leap::LeapArc;
pub use seeker::SeekPhase;
pub use surge::SurgePhase;
// ADVENTURE GAMES
mod capture;
mod empower;
mod evade;
mod mend;
mod place_portal;
pub mod pounce;
pub mod salvo;
// MODERN ANIME
mod erupt;
mod finisher;
mod raise_fallen;
pub mod tag;
mod transpose;
// TV & CARTOONS
mod strain;
// SHONEN CLASSICS
mod banish;
mod contagion;
mod copy_last;
mod drill;
mod gaze_stun;
pub mod live;
mod mark_recall;
mod phase;
mod rush;
mod warp_to;
mod wave;
// VERB GRAMMAR (docs/toolbox/COMPOSED_MOVE.md)
mod pass_through;
pub mod relocate;
pub use strain::STRAIN_MAX_PCT;
pub use pounce::PouncePhase;

/// Most lingering zones at once (the oldest ends beyond it).
pub const MAX_ZONES: usize = 12;

/// Presentation moments of the generic ops (`GameEvent::Power(PowerEvent::Op(..))`). The sim
/// never reads them back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OpEvent {
    /// Rewind: `player` zipped back from `from` to `to` along `path` (oldest → newest samples of
    /// the route they took, for the rewind streak).
    Rewound { player: PlayerId, from: Vec3, to: Vec3, path: [Vec3; 8] },
    /// Ram phases (`target` = who was pinned / struck).
    Ram { player: PlayerId, phase: RamPhase, target: Option<PlayerId>, pos: Vec3, dir: Vec3 },
    /// Hook phases. `Fired` carries the flight (`pos` + `dir` × `speed`, up to `range` m) so the
    /// client draws the line travelling exactly as the host steps it.
    Hook { player: PlayerId, phase: HookPhase, target: Option<PlayerId>, pos: Vec3, dir: Vec3, speed: f32, range: f32 },
    /// Command grab: seized (`Seize`), thrown (`Throw`, `dir` = the throw), or grabbed air (`Whiff`).
    Grab { player: PlayerId, phase: GrabPhase, target: Option<PlayerId>, pos: Vec3, dir: Vec3 },
    /// A stance opened (deflect / counter / guard) for `secs`; `angle` = the deflect cone half-angle.
    Stance { player: PlayerId, kind: StanceKind, secs: f32, angle: f32 },
    /// A stance did its thing: a round sent back (`Deflect`), a hit answered (`Counter`), a hit
    /// shrugged (`Guard`). `dir` = where it went.
    Parried { player: PlayerId, kind: StanceKind, attacker: Option<PlayerId>, pos: Vec3, dir: Vec3 },
    /// Lock-on phases: `Start` (everyone gets the telegraph), `Mark` (a target crossed `frac` of a
    /// full lock), `Fire` (`hits` targets struck), `Broken` (interrupted, nothing fired).
    LockOn { player: PlayerId, phase: LockPhase, target: Option<PlayerId>, frac: f32, secs: f32, hits: u8 },
    /// `target`'s power moves are locked for `secs` (a hack, a silence).
    LockedOut { player: PlayerId, target: PlayerId, secs: f32 },
    /// A lingering zone opened (drawn as `field`, a strength-0 force field that also carries it).
    Zone { player: PlayerId, field: Option<u16>, pos: Vec3, radius: f32, secs: f32, status: Option<StatusKind>, slow: f32, lockout: bool },
    /// Seeker beats (the body is a construct everyone already sees): launch, strike, dodge, chain,
    /// blocked, return, catch.
    Seek { player: PlayerId, phase: SeekPhase, target: Option<PlayerId>, pos: Vec3, dir: Vec3 },
    /// Haul beats: a player dragged in, a chunk torn loose (`from` → `to` over `secs`), a strike on
    /// the way, the landing.
    Haul { player: PlayerId, phase: HaulPhase, target: Option<PlayerId>, from: Vec3, to: Vec3, secs: f32 },
    /// A leap's arc (the landing marker).
    Leap(LeapArc),
    /// Surge beats: the wall rose (`width`, `speed`, `dist` = the whole run), swept someone, broke.
    Surge { player: PlayerId, phase: SurgePhase, target: Option<PlayerId>, pos: Vec3, dir: Vec3, width: f32, speed: f32, dist: f32 },
    /// A roulette picked list `pick` of `of`.
    Roulette { player: PlayerId, pick: u8, of: u8 },
    // ADVENTURE GAMES
    /// Pounce beats: the lunge left (`dir`), struck `target` (the bounce follows), or whiffed.
    Pounce { player: PlayerId, phase: PouncePhase, target: Option<PlayerId>, pos: Vec3, dir: Vec3 },
    /// A strike is marked at `pos` and lands in `secs` (Salvo): draw the warning ring / the cut line.
    Telegraph { player: PlayerId, pos: Vec3, radius: f32, secs: f32, element: Element },
    /// A marked strike landed at `pos` (`cut` = a clean cut / bolt, else a blast that also sends
    /// the world's `Explosion`): draw it in the move's style.
    Strike { player: PlayerId, pos: Vec3, radius: f32, element: Element, cut: bool },
    /// A heal: `pct` at once, healing on for `secs` more (0 = none).
    Mended { player: PlayerId, pct: f32, secs: f32 },
    /// Evade: the window opened (`dodged: false`, `secs` = window) or a hit was slipped (`dodged:
    /// true`, `attacker`, `secs` = the time slow granted).
    Evade { player: PlayerId, dodged: bool, attacker: Option<PlayerId>, pos: Vec3, secs: f32 },
    /// A portal-gun shot landed (`orange` = the second colour); a pair opens as `PortalOpened`.
    PortalShot { player: PlayerId, pos: Vec3, normal: Vec3, orange: bool },
    /// A timed super state started (`secs`) or ended.
    Empowered { player: PlayerId, secs: f32, on: bool },
    /// `player` captured `construct` for `secs` (`on`) — or let it go (`on: false`, back to its maker).
    Captured { player: PlayerId, construct: bd_core::ConstructId, pos: Vec3, secs: f32, on: bool },
    /// MOVIES & COMICS: a ray / bolt from an op — a spell hand → target (Afflict, Disarm) or one leg
    /// of a bouncing beam (`target` = who it was aimed at, if anyone).
    Zap { player: PlayerId, from: Vec3, to: Vec3, element: Element, target: Option<PlayerId> },
    /// MOVIES & COMICS: a DelayedHit mark planted on `target` (bursts in `secs`), or its burst
    /// (`burst`, at `point`).
    Marked { player: PlayerId, target: PlayerId, secs: f32, burst: bool, point: Vec3 },
    /// MODERN ANIME: a Tag stuck on `target` for `secs` (`strike: false`), or struck (`strike: true`:
    /// the TagStrike reached them — Resonance's doll, Bungee Gum's snap).
    Tagged { player: PlayerId, target: PlayerId, secs: f32, strike: bool },
    /// SHONEN CLASSICS ops (ops/live.rs): one beat of a Drill / MarkRecall / WarpTo / CopyLast / GazeStun /
    /// Phase / Banish / Rush / Contagion / Wave: `fx` names the beat, `from` / `to` / `secs` / `size` draw it.
    Toolbox { player: PlayerId, fx: live::ToolboxFx, from: Vec3, to: Vec3, secs: f32, size: f32 },
    /// MODIFIERS & FUSION (powers/twist): a TWISTED / FUSED move fired (sent with its `Fired`, before
    /// its ops' own events). `a` / `b` = the base moves (`b` = fused with; `None` for a famous cast),
    /// `mods` = modifier wire bytes in three groups of `twist::MAX_MODS` (A's, B's, the whole's; 0 =
    /// empty) — `twist::exec::interned(a, b, &mods)` rebuilds the twisted `MoveDef` for presentation.
    /// `title` ("GIANT HOMING RASENGAN", "RASENGAN + AMATERASU"), `element` after swaps, `size` ×,
    /// `copies` fanned at once × `shots` in a row, `merge` = a two-element windup (fusion).
    Twisted {
        player: PlayerId,
        a: Option<super::toolbox::MoveRef>,
        b: Option<super::toolbox::MoveRef>,
        mods: [u8; 3 * super::twist::MAX_MODS],
        title: String,
        element: Element,
        size: f32,
        copies: u8,
        shots: u8,
        merge: Option<[Element; 2]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RamPhase {
    Start,
    /// Struck someone (knocked away, or kept going for `through` rams).
    Hit,
    /// Picked someone up on the front of the charge.
    Pinned,
    /// Drove them into a wall (the big one).
    Slam,
    /// The pinned target broke free (dodge / tether).
    Escaped,
    /// Ran into a wall with nobody on the front.
    Bonk,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookPhase {
    Fired,
    /// Caught a player (`target`); they are being reeled in when the hook pulls.
    Caught,
    /// Bit into terrain: an anchor hook reels its thrower there.
    Anchored,
    /// Snapped (dodged out, blocked by a construct, out of range, target in front of you).
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GrabPhase {
    Seize,
    Throw,
    Whiff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StanceKind {
    Deflect,
    Counter,
    Guard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LockPhase {
    Start,
    Mark,
    Fire,
    Broken,
}

/// Per-player stance timers (deflect / counter / guard) — indexed by player index, validated by id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stance {
    pub owner: PlayerId,
    pub deflect: f32,
    /// cos of the deflect cone half-angle.
    pub deflect_cos: f32,
    pub counter: f32,
    pub guard: f32,
    pub c_damage: f32,
    pub c_knockback: f32,
    pub c_up: f32,
    pub c_element: Element,
}

impl Stance {
    pub(crate) fn new(owner: PlayerId) -> Self {
        Self { owner, deflect: 0.0, deflect_cos: 0.0, counter: 0.0, guard: 0.0, c_damage: 0.0, c_knockback: 0.0, c_up: 0.0, c_element: Element::Light }
    }
}

/// World-level state of the generic ops (lives in `PowerWorld::ops`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpState {
    pub(crate) trails: rewind::Trails,
    pub(crate) rams: [Option<ram::Ram>; MAX_PLAYERS],
    pub(crate) hooks: [Option<hook::HookLine>; MAX_PLAYERS],
    pub(crate) grabs: [Option<grab::Hold>; MAX_PLAYERS],
    pub(crate) stances: [Option<Stance>; MAX_PLAYERS],
    pub(crate) lock_ons: [Option<lock_on::Channel>; MAX_PLAYERS],
    /// (victim, seconds left) — indexed by the victim's player index.
    pub(crate) locked: [Option<(PlayerId, f32)>; MAX_PLAYERS],
    pub(crate) zones: [Option<zone::Zone>; MAX_ZONES],
    // MYTHOLOGY & FOLKLORE: thrown seekers, hauled chunks, travelling wall fronts (one per player).
    pub(crate) seekers: [Option<seeker::Seeker>; MAX_PLAYERS],
    pub(crate) hauls: [Option<haul::Haul>; MAX_PLAYERS],
    pub(crate) surges: [Option<surge::Surge>; MAX_PLAYERS],
    // ADVENTURE GAMES: lunges, placed strikes, heals, dodge windows, portal guns, super states.
    pub(crate) pounces: [Option<pounce::Pounce>; MAX_PLAYERS],
    pub(crate) strikes: [Option<salvo::Strike>; salvo::MAX_STRIKES],
    pub(crate) mends: [Option<mend::Mending>; MAX_PLAYERS],
    pub(crate) evades: [Option<evade::EvadeWindow>; MAX_PLAYERS],
    pub(crate) portal_guns: [Option<place_portal::PortalGun>; MAX_PLAYERS],
    pub(crate) empowers: [Option<empower::Empowered>; MAX_PLAYERS],
    pub(crate) captures: [Option<capture::Captured>; MAX_PLAYERS],
    // MOVIES & COMICS: delayed marks (five-point palm, a dusting wave) and live flurries.
    pub(crate) marks: [Option<delayed_hit::DelayedMark>; delayed_hit::MAX_MARKS],
    pub(crate) flurries: [Option<barrage::Flurry>; MAX_PLAYERS],
    // MODERN ANIME: tags (Resonance / Bungee Gum); expiry is a tick stamp, nothing steps them.
    pub(crate) tags: [Option<tag::Tag>; tag::MAX_TAGS],
    /// VERB GRAMMAR: per-player composed-move memory ("him", chain `Same`, the target being fired
    /// at right now). Nothing steps it; ticks are stamps.
    pub(crate) compose: [crate::powers::compose::ComposeMemo; MAX_PLAYERS],
    /// Re-entrancy guard: a counter's answer can't be countered back in the same call.
    pub(crate) answering: bool,
    /// A grab's throw is being applied: stances don't stop it (a grab beats a block).
    pub(crate) unblockable: bool,
}

impl OpState {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Anything besides the always-on trail is running.
    fn any_active(&self) -> bool {
        self.rams.iter().any(Option::is_some)
            || self.marks.iter().any(Option::is_some)
            || self.flurries.iter().any(Option::is_some)
            || self.hooks.iter().any(Option::is_some)
            || self.grabs.iter().any(Option::is_some)
            || self.stances.iter().any(Option::is_some)
            || self.lock_ons.iter().any(Option::is_some)
            || self.locked.iter().any(Option::is_some)
            || self.zones.iter().any(Option::is_some)
            || self.seekers.iter().any(Option::is_some)
            || self.hauls.iter().any(Option::is_some)
            || self.surges.iter().any(Option::is_some)
            || self.adventure_active()
    }

    /// ADVENTURE GAMES ops running.
    fn adventure_active(&self) -> bool {
        self.pounces.iter().any(Option::is_some)
            || self.strikes.iter().any(Option::is_some)
            || self.mends.iter().any(Option::is_some)
            || self.evades.iter().any(Option::is_some)
            || self.portal_guns.iter().any(Option::is_some)
            || self.empowers.iter().any(Option::is_some)
            || self.captures.iter().any(Option::is_some)
    }
}

/// How long `op`'s own active phase lasts once it starts (0 = instant). A chain's next link fires
/// after the longest op of the move; ops that end on contact (ram, hook, lock-on) may finish sooner,
/// which [`World::op_busy`] reports.
pub fn active_secs(op: &MoveOp) -> f32 {
    match *op {
        MoveOp::Rewind { .. } => 0.12,
        // MOVIES & COMICS
        MoveOp::BlinkCarry { .. } => blink_carry::CARRY_HANG_SECS * 0.75,
        MoveOp::Barrage { hits, interval, .. } => (hits.clamp(1, barrage::MAX_HITS) as f32 - 1.0) * interval.clamp(0.05, 1.0) + 0.05,
        MoveOp::Afflict { .. } | MoveOp::BounceBeam { .. } | MoveOp::Disarm { .. } => 0.1,
        // MODERN ANIME: the lunge-strike and the rising terrain read as a beat.
        MoveOp::Finisher { .. } => 0.15,
        MoveOp::Erupt { .. } => 0.25,
        MoveOp::Ram { secs, .. } => fclamp(secs, 0.05, ram::MAX_SECS),
        MoveOp::Hook { range, speed, pull, .. } => fclamp(range, 1.0, hook::MAX_RANGE) / fclamp(speed, hook::MIN_SPEED, hook::MAX_SPEED) + if pull > 0.0 { hook::TOW_SECS } else { 0.0 },
        MoveOp::Grab { hold, .. } => grab::hold_secs(hold),
        MoveOp::Deflect { secs, .. } => fclamp(secs, 0.1, deflect::MAX_SECS),
        MoveOp::Counter { secs, .. } => fclamp(secs, 0.05, counter::MAX_SECS),
        MoveOp::LockOn { secs, .. } => fclamp(secs, 0.3, lock_on::MAX_SECS),
        // MYTHOLOGY & FOLKLORE: the throw / the cast line / the wall rising; a leap is its flight.
        MoveOp::Seeker { .. } => 0.3,
        MoveOp::Haul { .. } => 0.35,
        MoveOp::Surge { .. } => 0.4,
        MoveOp::Leap { range, apex } => (0.6 + range.clamp(3.0, leap::MAX_RANGE) / 40.0 + apex.clamp(0.0, leap::MAX_APEX) / 20.0).min(2.5),
        // ADVENTURE GAMES: a lunge's flight; a salvo until its last strike lands; a dodge window.
        MoveOp::Pounce { range, speed, secs, .. } => pounce::active_secs(range, speed, secs),
        MoveOp::Salvo { count, delay, interval, .. } => salvo::active_secs(count, delay, interval),
        MoveOp::Evade { secs, .. } => secs.clamp(0.05, evade::MAX_SECS),
        // SHONEN CLASSICS: the lunge (+ grind), the flurry; a wave / warp / gaze is instant.
        MoveOp::Drill { secs, grind, .. } => secs.clamp(0.05, 1.2) + grind.clamp(0.0, 0.9),
        MoveOp::Rush { hits, rate, .. } => hits.clamp(1, 40) as f32 / if rate > 0.0 { rate.clamp(2.0, 30.0) } else { 16.0 },
        // VERB GRAMMAR: a relocation / a step through the wall is an arrival beat.
        MoveOp::Relocate { .. } | MoveOp::PassThrough { .. } => 0.12,
        _ => 0.0,
    }
}

impl World {
    /// Run one of the generic ops (`None` = not one of these). Called from `run_op`.
    pub(crate) fn run_generic_op(&mut self, i: usize, op: &MoveOp) -> Option<Result<(), PowerError>> {
        if i >= MAX_PLAYERS || i >= self.players.len() {
            return Some(Err(PowerError::UnknownPlayer));
        }
        Some(match *op {
            MoveOp::Rewind { secs } => self.op_rewind(i, secs),
            MoveOp::Ram { speed, secs, damage, knockback, up, pin, through, wall, pitch, steer, keep, element } => {
                self.op_ram(i, ram::RamSpec { speed, secs, damage, knockback, up, pin, through, wall, pitch, steer, keep, element })
            }
            MoveOp::Hook { range, speed, pull, damage, status, secs, anchor } => self.op_hook(i, hook::HookSpec { range, speed, pull, damage, status, secs, anchor }),
            MoveOp::Grab { range, hold, damage, knockback, up, lift, back } => self.op_grab(i, grab::GrabSpec { range, hold, damage, knockback, up, lift, back }),
            MoveOp::Deflect { secs, angle } => self.op_deflect(i, secs, angle),
            MoveOp::Counter { secs, damage, knockback, up, element } => self.op_counter(i, secs, damage, knockback, up, element),
            MoveOp::LockOn { secs, range, damage, knockback, angle } => self.op_lock_on(i, secs, range, damage, knockback, angle),
            MoveOp::Lockout { range, secs } => self.op_lockout(i, range, secs),
            MoveOp::Zone { radius, secs, status, slow, dps, lockout, at, dist, follow } => {
                self.op_zone(i, zone::ZoneSpec { radius, secs, status, slow, dps, lockout, at, dist, follow })
            }
            // MYTHOLOGY & FOLKLORE
            MoveOp::Seeker { ref recipe, range, cone, speed, damage, knockback, up, element, hits, chain } => {
                self.op_seeker(i, seeker::SeekerSpec { recipe: recipe.clone(), range, cone, speed, damage, knockback, up, element, hits, chain })
            }
            MoveOp::Haul { range, radius, speed, damage } => self.op_haul(i, range, radius, speed, damage),
            MoveOp::Leap { range, apex } => self.op_leap(i, range, apex),
            MoveOp::SelfResize { scale, secs } => self.op_self_resize(i, scale, secs),
            MoveOp::Surge { width, dist, speed, push, damage, element } => self.op_surge(i, surge::SurgeSpec { width, dist, speed, push, damage, element }),
            // ADVENTURE GAMES
            MoveOp::Pounce { range, speed, cone, damage, knockback, element, up, bounce, secs } => {
                self.op_pounce(i, pounce::PounceSpec { range, speed, cone, damage, knockback, element, up, bounce, secs })
            }
            MoveOp::Salvo { count, delay, interval, radius, damage, knockback, element, carve, up, at, dist, spread, seek, track, cut } => {
                self.op_salvo(i, salvo::SalvoSpec { count, delay, interval, radius, damage, knockback, element, carve, up, at, dist, spread, seek, track, cut })
            }
            MoveOp::Mend { pct, per_s, secs } => self.op_mend(i, pct, per_s, secs),
            MoveOp::Evade { secs, slow, speed, blink } => self.op_evade(i, secs, slow, speed, blink),
            MoveOp::PlacePortal { range, secs } => self.op_place_portal(i, range, secs),
            MoveOp::Capture { range, secs, ride } => self.op_capture(i, range, secs, ride),
            MoveOp::Empower { secs, damage, armor, contact, contact_kb, contact_cd, element } => {
                self.op_empower(i, empower::EmpowerSpec { secs, damage, armor, contact, contact_kb, contact_cd, element })
            }
            // MOVIES & COMICS
            MoveOp::BlinkCarry { dist, up, grab } => self.op_blink_carry(i, dist, up, grab),
            MoveOp::Afflict { range, cone, status, secs, damage, knockback, up, element } => {
                self.op_afflict(i, afflict::AfflictArgs { range, cone, status, secs, damage, knockback, up, element })
            }
            MoveOp::DelayedHit { range, cone, radius, secs, damage, knockback, up, element } => {
                self.op_delayed_hit(i, delayed_hit::DelayedHitArgs { range, cone, radius, secs, damage, knockback, up, element })
            }
            MoveOp::BounceBeam { length, bounces, radius, damage, knockback, element } => {
                self.op_bounce_beam(i, bounce_beam::BounceBeamArgs { length, bounces, radius, damage, knockback, element })
            }
            MoveOp::Barrage { hits, interval, range, angle, damage, knockback, finish, advance, up, blink, element } => {
                self.op_barrage(i, barrage::BarrageArgs { hits, interval, range, angle, damage, knockback, finish, advance, up, blink, element })
            }
            MoveOp::Disarm { range, cone, pull, knockback } => self.op_disarm(i, range, cone, pull, knockback),
            // MODERN ANIME
            MoveOp::Transpose { range, cone, others } => self.op_transpose(i, range, cone, others),
            MoveOp::Finisher { lunge, range, angle, damage, knockback, element, up, window, mult, fx } => {
                self.op_finisher(i, finisher::FinisherOp { lunge, range, angle, damage, knockback, element, up, window, mult, fx })
            }
            MoveOp::Erupt { length, width, height, count, element, damage, launch, across, dist, shape, at } => {
                let shape = if across { super::EruptShape::Wall } else { shape };
                self.op_erupt(i, erupt::EruptOp { length, width, height, count, element, damage, launch, shape, dist, at })
            }
            MoveOp::Tag { range, cone, secs } => self.op_tag(i, range, cone, secs),
            MoveOp::RaiseFallen { ref recipe, count, radius } => self.op_raise_fallen(i, recipe, count, radius),
            MoveOp::TagStrike { damage, knockback, up, element, pull } => self.op_tag_strike(i, tag::TagStrikeOp { damage, knockback, up, element, pull }),
            MoveOp::Strain { pct } => self.op_strain(i, pct),
            // SHONEN CLASSICS
            MoveOp::Drill { speed, secs, grind, damage, knockback, element, up } => self.op_drill(i, speed, secs, grind, damage, knockback, element, up),
            MoveOp::MarkRecall { range, secs, strike } => self.op_mark_recall(i, range, secs, strike),
            MoveOp::WarpTo { range, cone, behind, line } => self.op_warp_to(i, range, cone, behind, line),
            MoveOp::CopyLast { range, secs } => self.op_copy_last(i, range, secs),
            MoveOp::GazeStun { range, cone, secs, status, aimed } => self.op_gaze_stun(i, range, cone, secs, status, aimed),
            MoveOp::Phase { secs } => self.op_phase(i, secs),
            MoveOp::Banish { range, dist } => self.op_banish(i, range, dist),
            MoveOp::Rush { hits, rate, damage, knockback, range, ref stand } => self.op_rush(i, hits, rate, damage, knockback, range, stand.as_ref()),
            MoveOp::Contagion { range, cone, status, secs, spread } => self.op_contagion(i, range, cone, status, secs, spread),
            MoveOp::Wave { length, speed, width, carve, damage, knockback, up, element, thin } => {
                self.op_wave(i, length, speed, width, carve, damage, knockback, up, element, thin)
            }
            // VERB GRAMMAR
            MoveOp::Relocate { range, cone, back, side, up, dist, me } => self.op_relocate(i, relocate::RelocateSpec { range, cone, back, side, up, dist, me }),
            MoveOp::PassThrough { depth } => self.op_pass_through(i, depth),
            _ => return None,
        })
    }

    /// Fire a single op for `player` outside any power (tools, tests, a borrowed one-shot). The
    /// normal `run_op` path is used for built-in ops; the generic ones run here directly.
    pub fn fire_op(&mut self, player: PlayerId, op: &MoveOp) -> Result<(), PowerError> {
        let Some(i) = self.pidx(player) else { return Err(PowerError::UnknownPlayer) };
        if !self.players[i].alive {
            return Err(PowerError::Dead);
        }
        self.run_generic_op(i, op).unwrap_or(Err(PowerError::NoSuchMove))
    }

    /// Does `player` still have a generic op in its active phase (a ram in flight, a hook out, a
    /// grab held, a lock-on channel, a counter / deflect window)? Chains wait on it.
    pub fn op_busy(&self, player: PlayerId) -> bool {
        let Some(i) = self.pidx(player) else { return false };
        if i >= MAX_PLAYERS {
            return false;
        }
        let o = &self.powers.ops;
        let mine = |id: Option<PlayerId>| id == Some(player);
        mine(o.rams[i].as_ref().map(|r| r.owner))
            || mine(o.hooks[i].as_ref().map(|h| h.owner))
            || mine(o.grabs[i].as_ref().map(|g| g.owner))
            || mine(o.lock_ons[i].as_ref().map(|c| c.owner))
            || mine(o.flurries[i].as_ref().map(|f| f.owner))
            || mine(o.pounces[i].as_ref().map(|p| p.owner))
            || o.evades[i].as_ref().is_some_and(|e| e.owner == player && e.left > 0.0)
            || o.stances[i].as_ref().is_some_and(|s| s.owner == player && (s.counter > 0.0 || s.deflect > 0.0 || s.guard > 0.0))
            || self.players[i].power.live.drill.is_some() // SHONEN CLASSICS: a lunge / grind, a rush flurry
            || self.players[i].power.live.rush.is_some()
    }

    /// `player`'s power moves are locked (a hack / silence / EMP zone).
    pub fn power_locked(&self, player: PlayerId) -> bool {
        self.pidx(player).is_some_and(|i| self.locked_at(i))
    }

    pub(crate) fn locked_at(&self, i: usize) -> bool {
        let Some(p) = self.players.get(i) else { return false };
        i < MAX_PLAYERS && self.powers.ops.locked[i].is_some_and(|(id, t)| id == p.id && t > 0.0)
    }

    /// Per-tick step of every generic op (first thing in `step_powers`, which runs after
    /// movement / projectiles / constructs). No allocation.
    pub(crate) fn step_generic_ops(&mut self, dt: f32) {
        self.record_trails();
        self.step_live_ops(dt); // SHONEN CLASSICS (per-player state; one idle check each when unused)
        if !self.powers.ops.any_active() {
            return;
        }
        self.step_stances(dt);
        self.step_locked(dt);
        self.step_rams(dt);
        self.step_hooks(dt);
        self.step_grabs(dt);
        self.step_lock_ons(dt);
        self.step_zones(dt);
        self.step_delayed_hits(dt); // MOVIES & COMICS
        self.step_flurries(dt);
        self.step_seekers(dt); // MYTHOLOGY & FOLKLORE
        self.step_hauls(dt);
        self.step_surges(dt);
        if self.powers.ops.adventure_active() {
            // ADVENTURE GAMES
            self.step_pounces(dt);
            self.step_strikes(dt);
            self.step_mends(dt);
            self.step_evades(dt);
            self.step_portal_guns();
            self.step_empowers(dt);
            self.step_captures(dt);
        }
    }

    /// Pre-damage hook (top of `powers_on_hit`, before any held-power rule — a borrowed stance works
    /// without holding a power): guard / counter / deflect stances answer the hit, and a hard hit
    /// interrupts the victim's ram / lock-on / grab. `None` = the hit is cancelled.
    pub(crate) fn ops_on_hit(&mut self, ti: usize, hit: HitInfo) -> Option<HitInfo> {
        // SHONEN CLASSICS: intangible (Phase) - the hit passes straight through.
        if self.phase_ignores(ti) {
            return None;
        }
        if ti >= MAX_PLAYERS || !self.powers.ops.any_active() {
            return Some(hit);
        }
        // ADVENTURE GAMES: a perfect dodge slips it before anything else; super-state damage /
        // armor; a damaging hit stops a heal-over-time and a lunge.
        if self.powers.ops.adventure_active() {
            if self.evade_on_hit(ti, &hit) {
                return None;
            }
        }
        let hit = self.stance_on_hit(ti, hit)?;
        self.interrupt_on_hit(ti, &hit);
        let hit = if self.powers.ops.adventure_active() {
            self.mend_on_hit(ti, &hit);
            if hit.damage > 0.0 && hit.source != Some(hit.target) && self.powers.ops.pounces[ti].is_some_and(|p| p.owner == hit.target) && hit.knockback >= 12.0 {
                self.powers.ops.pounces[ti] = None;
            }
            self.empower_on_hit(ti, hit)
        } else {
            hit
        };
        Some(hit)
    }

    fn step_locked(&mut self, dt: f32) {
        for k in 0..MAX_PLAYERS {
            let Some((id, t)) = self.powers.ops.locked[k] else { continue };
            let valid = self.players.get(k).is_some_and(|p| p.id == id && p.alive);
            self.powers.ops.locked[k] = if valid && t - dt > 0.0 { Some((id, t - dt)) } else { None };
        }
    }

    // ------------------------------------------------------------------ shared helpers

    pub(crate) fn op_event(&mut self, e: OpEvent) {
        self.inject_event(GameEvent::Power(PowerEvent::Op(e)));
    }

    /// The move's aim (third person: the validated crosshair ray) or the eye + look.
    pub(crate) fn op_eye_look(&self, i: usize) -> (Vec3, Vec3) {
        if let Some((k, eye, dir)) = self.powers.move_aim
            && k == i
        {
            return (eye, dir);
        }
        let p = &self.players[i];
        (p.eye(&self.cfg.move_params), p.look())
    }

    /// Where an `At` lands for player `i` (same rule as the built-in area ops).
    pub(crate) fn op_at_point(&self, i: usize, at: At, dist: f32) -> Vec3 {
        let (eye, look) = self.op_eye_look(i);
        match at {
            At::Me => self.players[i].hull(&self.cfg.move_params).center(),
            At::Front => eye + look * dist.clamp(0.0, 30.0),
            At::Aim => {
                let cw = crate::movement::CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(self.players[i].id));
                crate::movement::raycast_world(&cw, eye, look, 60.0, false).map_or(eye + look * 20.0, |h| h.point + h.normal * 0.3)
            }
        }
    }

    /// The living enemy the crosshair is on within `range` (terrain blocks), else the one closest
    /// to the crosshair inside a `cone`° assist cone with line of sight.
    pub(crate) fn op_aimed_player(&self, i: usize, range: f32, cone: f32) -> Option<usize> {
        let (eye, look) = self.op_eye_look(i);
        let base = &self.cfg.move_params;
        let end = eye + look * range;
        let wall = bd_core::voxel::raycast(&self.voxels, eye, look, range).map_or(f32::INFINITY, |h| h.t);
        let mut best: Option<(usize, f32)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive {
                continue;
            }
            let hull = crate::geom::inflate(&q.hull(base), 0.45);
            if let Some((t, _)) = crate::geom::segment_aabb(eye, end, &hull) {
                let d = t * range;
                if d <= wall && best.is_none_or(|b| d < b.1) {
                    best = Some((k, d));
                }
            }
        }
        if best.is_some() || cone <= 0.0 {
            return best.map(|b| b.0);
        }
        let cos = cone.clamp(0.0, 45.0).to_radians().cos();
        let mut best: Option<(usize, f32)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive {
                continue;
            }
            let d = q.hull(base).center() - eye;
            let len = d.length();
            if len > range || len < 0.3 {
                continue;
            }
            let dot = d.dot(look) / len;
            if dot < cos || bd_core::voxel::raycast(&self.voxels, eye, d / len, len - 0.5).is_some() {
                continue;
            }
            if best.is_none_or(|b| dot > b.1) {
                best = Some((k, dot));
            }
        }
        best.map(|b| b.0)
    }

    /// A zero-damage, zero-knockback hit: kill credit + interrupts (oath, charge) without a push.
    pub(crate) fn op_touch(&mut self, i: usize, k: usize, dir: Vec3) -> bool {
        let (src, tid) = (self.players[i].id, self.players[k].id);
        let c = self.players[k].hull(&self.cfg.move_params).center();
        self.apply_hit(HitInfo::new(tid, Some(src), crate::combat::HitKind::RingShove, 0.0, 0.0, dir, c)).applied
    }
}

/// `v.clamp(lo, hi)` for DATA numbers (not sanitised upstream): a non-finite `v` becomes `lo`.
pub(crate) fn fclamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { lo }
}

/// Horizontal part of `v`, normalised (`fallback` if flat).
pub(crate) fn flat(v: Vec3, fallback: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z).normalize_or(fallback)
}
