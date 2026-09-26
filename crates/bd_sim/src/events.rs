//! Everything noteworthy that happened in a tick, for the client (juice, audio, VFX, HUD, killfeed)
//! and for replication (send on the reliable lane; snapshots carry only continuous state).
//!
//! Events are deterministic: same inputs ⇒ same event list, so tests hash them to check
//! determinism. Only rare events (casts, joins) carry heap data.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Recipe};
use bd_core::{ConstructId, PlayerId, Quat, Vec3};
use bd_voxel::CarveOp;
use serde::{Deserialize, Serialize};

use crate::combat::{HitKind, ImpactFx};
use crate::extract::ExtractOp;
use crate::space::{ForceField, Link, PortalEnd};
use crate::projectile::ProjectileKind;
use crate::rules::MatchPhase;
use crate::status::StatusKind;
use crate::weapons::GunArchetype;
use crate::willpower::OathError;

/// Why a KO happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KoCause {
    /// Legacy (health model): never produced by the Smash model.
    Damage,
    /// Fell into the void / left the arena bounds.
    Void,
    WallSplat,
    /// Damage over time (burn / poison). Legacy: never produced by the Smash model.
    Status,
    /// Launched out through a side / top blast zone (`Ko.zone`).
    BlastZone,
}

/// Why a construct left the world (other than shattering).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GoneReason {
    /// Owner pressed X (refund 25 %).
    Dismissed,
    /// Lifetime ran out.
    Expired,
    /// Replaced by a newer cast in the same slot.
    Replaced,
    /// Owner was KO'd or left.
    OwnerGone,
    /// A thrown object finished its job (landed, exploded, was caught).
    Spent,
    /// Match reset.
    Reset,
}

/// Why a cast was refused.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CastError {
    UnknownPlayer,
    Dead,
    /// Cost exceeds current willpower (the ring flickers).
    NotEnoughWillpower { cost: f32, have: f32 },
    /// Busy reciting the oath.
    Reciting,
    /// Stunned or seated in a construct.
    Busy,
    /// No-limits: already has a colossal construct.
    ColossalLimit,
    /// Match ended.
    MatchOver,
    /// The lobby turned this kind of construct off (`MatchRules::limits`) and nothing near it is
    /// allowed: the ring fizzles, nothing paid.
    Fizzled,
}

/// Something that went through a portal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Traveller {
    Player(PlayerId),
    Construct(ConstructId),
    Projectile(u16),
}

/// Movement moments worth a sound on remote clients (bits of [`GameEvent::Move`]).
pub mod move_flags {
    pub const JUMP: u16 = 1 << 0;
    pub const PERFECT_HOP: u16 = 1 << 1;
    pub const LAND_HARD: u16 = 1 << 2;
    pub const SLIDE: u16 = 1 << 3;
    pub const TETHER_FIRE: u16 = 1 << 4;
    pub const TETHER_ATTACH: u16 = 1 << 5;
    pub const TETHER_RELEASE: u16 = 1 << 6;
    pub const BOOST: u16 = 1 << 7;
    pub const WALL_HIT: u16 = 1 << 8;
    /// The one air jump (hard-light step under the feet).
    pub const AIR_JUMP: u16 = 1 << 9;
    /// A dodge started (hard-light streak).
    pub const DODGE: u16 = 1 << 10;
    /// A wall / surf-ramp touch gave back the air jump (a small scuff at the contact).
    pub const WALL_REFRESH: u16 = 1 << 11;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GameEvent {
    PlayerJoined { player: PlayerId, name: String, corps: Corps },
    PlayerLeft { player: PlayerId },
    /// A player took a hit. `impact` is everything the client needs for juice.
    Hit {
        target: PlayerId,
        source: Option<PlayerId>,
        construct: Option<ConstructId>,
        kind: HitKind,
        element: Element,
        damage: f32,
        /// Final knockback speed (m/s) after health scaling and bonuses.
        knockback: f32,
        point: Vec3,
        /// Momentum bonus fraction applied (0..0.6).
        bonus: f32,
        tumble: bool,
        lethal: bool,
        /// Head-zone hit (×1.5) or a construct weak point.
        crit: bool,
        impact: ImpactFx,
    },
    /// A hit was absorbed (spawn shield).
    Blocked { target: PlayerId, source: Option<PlayerId>, point: Vec3 },
    /// Tumbling into a wall: bonus damage + crater + bounce.
    WallSplat { player: PlayerId, point: Vec3, normal: Vec3, speed: f32, damage: f32 },
    StatusApplied { player: PlayerId, status: StatusKind, secs: f32, source: Option<PlayerId> },
    /// KO confirmation (DESIGN.md "Hit & KO confirmation").
    Ko {
        victim: PlayerId,
        killer: Option<PlayerId>,
        cause: KoCause,
        victim_corps: Corps,
        /// Killer's streak (KOs since their last death) including this one.
        streak: u16,
        /// ≤ 1 s since the killer's previous KO.
        double: bool,
        /// Fell into the void / out of bounds.
        void: bool,
        /// The victim was on a streak of ≥ 3.
        shutdown: bool,
        /// Victim's last pose + velocity (the client shatters them from here).
        pos: Vec3,
        vel: Vec3,
        yaw: f32,
        /// Blast zone crossed (None for other KOs).
        zone: Option<crate::smash::BlastZone>,
        /// Star KO (top zone): play the twinkle, poof after this many seconds.
        poof_delay: f32,
        /// Killer's KO total after this KO (incl. any shutdown bonus).
        killer_kos: u16,
        /// Victim's damage % when KO'd.
        pct: f32,
        /// Stock mode: victim's lives left (None in timed matches).
        stocks_left: Option<u8>,
    },
    /// Stock mode: out of lives.
    Eliminated { player: PlayerId },
    /// A hit launched a player (knockback ≥ `kb_min_launch`): trail + telemetry.
    Launched { player: PlayerId, source: Option<PlayerId>, speed: f32, dir: Vec3, pct: f32 },
    /// A launch ended in play (landed / tethered / tumble over) — the player recovered.
    Recovered { player: PlayerId, source: Option<PlayerId>, pct: f32 },
    /// Jump just before a wall / ground contact while tumbling: splat cancelled.
    Tech { player: PlayerId, pos: Vec3, normal: Vec3 },
    /// A launched player plowed through terrain: `entry` → `exit` (debris burst, CRASH).
    PunchThrough { player: PlayerId, entry: Vec3, exit: Vec3, material: u8 },
    Respawn { player: PlayerId, pos: Vec3, shield: f32 },
    /// A projectile left a hand / weapon. Clients simulate it from here (ballistic).
    ProjectileFired { id: u16, owner: PlayerId, kind: ProjectileKind, origin: Vec3, vel: Vec3 },
    /// A projectile ended (hit something or timed out).
    ProjectileEnded { id: u16, pos: Vec3, normal: Vec3, hit_player: Option<PlayerId> },
    Shove { player: PlayerId, origin: Vec3, dir: Vec3 },
    /// A gun (or the ring) fired: fire juice (viewmodel kick, muzzle light, casings, camera punch).
    ShotFired {
        owner: PlayerId,
        construct: Option<ConstructId>,
        archetype: GunArchetype,
        muzzle: Vec3,
        dir: Vec3,
        /// Recoil impulse strength (m/s) before the ground factor.
        recoil: f32,
        pellets: u8,
    },
    /// A hitscan beam (railgun): draw the lingering beam from `origin` to `end`.
    Hitscan { owner: PlayerId, origin: Vec3, end: Vec3, hits: u8 },
    /// A hitscan gun round (hand cannon / minigun round / one super-shotgun pellet): the client
    /// draws a fast, short-lived chunky voxel tracer `from` the muzzle `to` the impact, then the
    /// impact FX. `normal` is zero when the round flew to max range; `hit_player` = who it struck.
    ShotTrace { owner: PlayerId, archetype: GunArchetype, from: Vec3, to: Vec3, normal: Vec3, hit_player: Option<PlayerId> },
    /// A construct was cast; clients build its model from the recipe (same phrase = same model).
    ConstructSpawned {
        id: ConstructId,
        owner: PlayerId,
        recipe: Box<Recipe>,
        pos: Vec3,
        rot: Quat,
        forming: f32,
        cost: f32,
    },
    CastFailed { player: PlayerId, reason: CastError },
    /// A cast was grafted onto an existing construct; `recipe` is the merged result.
    Grafted { player: PlayerId, id: ConstructId, recipe: Box<Recipe>, cost: f32 },
    /// Integrity damage to a construct (flash).
    ConstructHit { id: ConstructId, source: Option<PlayerId>, damage: f32, point: Vec3 },
    /// Integrity hit 0: burst into its own voxels along `impulse`.
    ConstructShattered { id: ConstructId, impulse: Vec3, pos: Vec3 },
    ConstructGone { id: ConstructId, reason: GoneReason, refund: f32 },
    /// Creature attack cue (constructs/creature.rs). `kind` = `constructs::ai::AttackKind as u8`,
    /// `phase` = `constructs::ai::phase::*` (telegraph → active → impact → end). The client plays the
    /// kind's `bd_shapes::anim::Attack` clip from `Telegraph`; the sim's hit window comes from the same
    /// `attack_timing`, so what is animated is what hits. `aim` = world direction (breath cone, dash
    /// line); `impact` = juice scalar (0 before impact; ≈1 bite … 2 kaiju stomp).
    CreatureAttack { id: ConstructId, kind: u8, phase: u8, aim: Vec3, impact: f32 },
    /// Creature behaviour mode changed (`constructs::ai::Mode as u8`: sitting, kiting, guarding,
    /// rooted in a cage, pinned, holding…) — the client picks idle/pose variations from it.
    CreatureMode { id: ConstructId, mode: u8 },
    /// Two opposing constructs collided (both flash, shard ring).
    Clash { a: ConstructId, b: ConstructId, point: Vec3, strength: f32 },
    Explosion { pos: Vec3, radius: f32, element: Element, source: Option<PlayerId> },
    /// Replicated terrain destruction: every client applies exactly this op.
    Carve { op: CarveOp },
    Move { player: PlayerId, flags: u16, speed: f32 },
    OathBegan { player: PlayerId },
    OathFinished { player: PlayerId, refill: f32 },
    OathInterrupted { player: PlayerId, refill: f32 },
    OathCancelled { player: PlayerId },
    OathRejected { player: PlayerId, reason: OathError },
    Seated { player: PlayerId, construct: ConstructId, part: u8 },
    Ejected { player: PlayerId, construct: ConstructId, vel: Vec3 },
    /// Terrain lifted out by a graft (COMBAT.md §3c). Clients apply the same op to get the model.
    Extracted { op: ExtractOp, construct: ConstructId, voxels: u32 },
    PortalOpened { id: u16, owner: PlayerId, ends: [PortalEnd; 2] },
    PortalClosed { id: u16 },
    /// Went through a portal. For the local player the client rotates its camera by `rot`.
    Teleported { who: Traveller, portal: u16, rot: Quat, pos: Vec3 },
    FieldCreated { field: ForceField },
    FieldEnded { id: u16 },
    LinkCreated { link: Link },
    LinkBroken { id: u16, cut: bool },
    /// Shrink / Grow applied (target scale, seconds).
    Resized { player: PlayerId, scale: f32, secs: f32 },
    /// Chaos soak: a human-readable description of a random cast (for the soak viewer).
    ChaosCast { player: PlayerId, description: String },
    MatchPhase { phase: MatchPhase, round: u32, winner: Option<PlayerId> },
    /// Terrain regenerated from this seed (rematch). Clients regenerate too.
    ArenaReset { seed: u64 },
    /// A held / worn / piloted construct moment (swing phases, parry, dive slam, whip latch, flap,
    /// eject…) for client juice. `strength` is the impact scalar (≈ 1 normal, 2+ huge); `dir` the
    /// swing / launch direction. (Held engineer: `constructs/held.rs`.)
    HeldFx { id: ConstructId, owner: PlayerId, fx: HeldFx, pos: Vec3, dir: Vec3, strength: f32 },
    /// POWERS HOOK: a player-power moment (fruit eaten, stand rush, time stop, scatter,
    /// restore…), see `crate::powers::PowerEvent`.
    Power(crate::powers::PowerEvent),
    // ---- world / construct presentation (world owner) ----
    /// A portal gun placed one ring (`slot` 0 = A, 1 = B). When both exist the pair opens
    /// (`PortalOpened`, re-sent whenever a gun shot moves one end).
    PortalPlaced { owner: PlayerId, slot: u8, end: PortalEnd },
    /// A Guard (front) or Bounce (any face) construct turned a projectile around; it now belongs to
    /// the construct's owner.
    Deflected { construct: ConstructId, projectile: u16, pos: Vec3, dir: Vec3, guard: bool },
    /// A transformer switched target mode (`robot` false = alt / vehicle) or became a combined giant.
    /// Pose it from the snapshot's `xform` byte (mode + progress).
    Transformed { id: ConstructId, robot: bool, combined: bool },
    /// Burning terrain voxels changed (chemistry VFX): newly lit / went out (burnt or doused).
    /// Voxel coordinates as i16 (the arena fits).
    FireVoxels { lit: Vec<[i16; 3]>, out: Vec<[i16; 3]> },
    /// Authored shapes of a construct that clients can't rebuild from the recipe alone (a two-mode
    /// icon's robot `alt_shape`, with its alt `shape`). Sent once, right after `ConstructSpawned`,
    /// and in late-join manifests.
    ConstructShapes { id: ConstructId, shape: Option<Box<bd_core::shape::ShapeSpec>>, alt_shape: Box<bd_core::shape::ShapeSpec> },
    /// A slice half: draw its parent's model clipped by `cut` (sent right after its
    /// `ConstructSpawned`, and in late-join manifests).
    ConstructSliced { id: ConstructId, parent: ConstructId, cut: crate::constructs::SliceCut },
    // ---- lobby knobs + Party mode (COMBAT.md §5g; crate::party) ----
    /// "Void off" lobby knob: a player fell out and was put back on a spawn point (no KO).
    VoidRescue { player: PlayerId, pos: Vec3 },
    /// MAP MECH HOOK: transforming panel `mover` entered `phase` at tick `since` (replicated: the
    /// client mirrors mover colliders from it; presentation plays the telegraph + panel sounds).
    MapMover { mover: u8, phase: crate::world::map_mech::MoverPhase, since: i64 },
    /// Party phase changed. `secs` = phase length (0 for Lobby / Round: see the match clock).
    PartyPhase { phase: crate::party::PartyPhase, round: u8, rounds: u8, secs: f32 },
    /// A round was scored (points include the round-win bonus for `winners`).
    PartyRoundEnd { round: u8, results: Vec<crate::party::PartyRoundResult>, winners: Vec<PlayerId> },
    /// `player`'s cards for this pick (send to that player; revealing others' offers is harmless).
    PartyOffer { player: PlayerId, offer: crate::party::PartyOffer },
    /// `player` locked a card (which one stays hidden until the reveal).
    PartyLocked { player: PlayerId },
    /// Boom or Bust coin flip: `boom` = Legendary, else Cursed. `effect` None = nothing left.
    PartyBoomOrBust { player: PlayerId, boom: bool, effect: Option<crate::party::EffectId> },
    /// Everyone's pick, revealed together.
    PartyReveal { picks: Vec<crate::party::PartyPick> },
    /// Podium (best first; ties share a place).
    PartyEnd { podium: Vec<crate::party::PartyStanding> },
    /// A party effect did something visible (confetti, honk, echo, slip…) for client juice.
    PartyFx { player: PlayerId, fx: crate::party::PartyFx, pos: Vec3, construct: Option<ConstructId> },
    /// CHARGE HOOK: a player's power-up started / filled the pool / was broken by a hit / was
    /// released (`held` = seconds into the charge). The aura itself follows `pflags::CHARGING`.
    Charge { player: PlayerId, kind: crate::charge::ChargeEvent, held: f32 },
    /// BOUNDS HOOK (`world/bounds.rs`): a construct left the map (blast zone / void) and blinked
    /// back next to its summoner at `to` (velocity zeroed). `from` = where it was lost. The client
    /// plays a hard-light blink-in at `to`; the snapshot already carries the new transform.
    ConstructRecalled { id: ConstructId, owner: PlayerId, from: Vec3, to: Vec3 },
    /// ASSIST HOOK (`crate::melee_assist`, `crate::hook`): `player` lunged at `other`, hooked
    /// `other` with a tether, lost that rope (why), reeled in for a slam, or landed the slam.
    Assist { player: PlayerId, other: Option<PlayerId>, kind: crate::hook::AssistEvent, pos: Vec3 },
    /// SIGNATURE HOOK (`crate::signature::orbital`): an orbital emitter's strike cycle — `Telegraph`
    /// (show the ground target marker at `point` for `secs`), `Strike` (the beam is live for `secs`;
    /// `Hitscan` events carry its moving end point), `End` (cooldown `secs`).
    OrbitalStrike { id: ConstructId, owner: PlayerId, phase: crate::signature::StrikePhase, point: Vec3, radius: f32, secs: f32 },
    /// SIGNATURE HOOK (`crate::signature`): a signature's moments for presentation / audio — a fusion
    /// windup (`Merge`), an erasing orb's launch / erase / end (`crate::signature::orb`).
    SignatureFx { id: ConstructId, owner: PlayerId, fx: crate::signature::SigFx, pos: Vec3, dir: Vec3 },
    /// FREEDOM HOOK (`crate::freedom`): kill-feed line (the killer construct's phrase as cast), a
    /// legendary construct appeared (golden sting), end-of-match highlights.
    Freedom(crate::freedom::FreedomEvent),
    /// INTERACT HOOK (`world/interactables.rs`): slot machine pulls / payouts, roulette house spin.
    Casino(crate::world::interactables::CasinoEvent),
    /// GIANTS (`constructs/giant.rs`): anatomy (replica colliders), weak-point crits, limbs cut off,
    /// topples, attack warnings, stomp shockwaves, command focus, hijack.
    Giant(crate::constructs::giant::GiantEvent),
}

/// What [`GameEvent::HeldFx`] reports (COMBAT.md §4 Held / Worn, §4b, §5d).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum HeldFx {
    /// Anticipation begins (wind-up whoosh). `strength`: 1 quick, 1 + combo step, 2 heavy.
    SwingStart = 0,
    /// The hitbox went live (swing whoosh / crack).
    SwingActive = 1,
    /// A swing struck terrain (sparks, chips, thud).
    SwingImpact = 2,
    /// Holding a charge (heavy / draw): glow builds.
    ChargeStart = 3,
    /// Perfectly-timed parry (flash, ting, micro slow-mo).
    Parry = 4,
    /// Guard absorbed a hit.
    Block = 5,
    /// Guard raised (shield up / parry stance).
    GuardUp = 6,
    /// Airborne hammer dive begins.
    DiveStart = 7,
    /// Dive slam landed (crater, radial shockwave). `strength` scales with fall speed.
    DiveSlam = 8,
    GroundPound = 9,
    /// Spear lunge dash.
    Lunge = 10,
    /// Held item thrown (Returning / axe throw).
    Throw = 11,
    /// Returning item caught back in the hand.
    Catch = 12,
    /// Grab seized a player.
    Grab = 13,
    /// Grabbed player thrown.
    GrabThrow = 14,
    Uppercut = 15,
    /// Whip latched onto terrain (a third tether).
    WhipLatch = 16,
    WhipRelease = 17,
    /// Whip yanked a player toward its owner.
    WhipYank = 18,
    /// Thor flight: the thrown item drags its owner.
    ThorFlight = 19,
    /// Two swings met: sword-lock knockback.
    SwordLock = 20,
    /// Wings flap.
    Flap = 21,
    /// Cape mid-air dash.
    CapeDash = 22,
    /// Parachute / umbrella deployed (cancels tumble).
    ChuteOpen = 23,
    /// Jetpack / suit thrusters lit.
    Thrust = 24,
    /// Piloted construct attack (punch / stomp / breath / volley).
    PilotAttack = 25,
    /// A piloted / suit weak point took a critical hit.
    WeakPoint = 26,
    /// Wielder launched by a Thrust hammer / Repel swing.
    Launch = 27,
    /// Transformer mode switch began (`strength` +1 → robot, −1 → alt/vehicle); ~0.9 s sequence.
    Transform = 28,
    /// Vehicle → robot transform in mid-air landed: stomp. `strength` scales with fall speed.
    TransformSlam = 29,
}
