//! bd_sim — the deterministic 120 Hz simulation of Brightest Day (see docs/DESIGN.md, COMBAT.md).
//!
//! Headless, allocation-free in steady state, host-authoritative; clients run the same code for
//! prediction. Modules:
//! - [`movement`]: the player character controller (Quake/Source ground+air movement, slide,
//!   AoTTG-style dual tethers, gas boost) as a pure `step_player` function shared by host and client.
//! - [`world`]: the authoritative [`World`] — players, combat, projectiles, constructs, match rules,
//!   one [`World::tick`] per `DT`, commands (`cast`, `dismiss`, oath, seats) between ticks.
//! - [`combat`]: hit data + formulas (knockback, momentum bonus, wall splat, juice).
//! - [`status`]: burn / poison / chill / stun / float / shadow / wet.
//! - [`willpower`]: recipe validation, costs, staleness, upkeep, oath recital.
//! - [`constructs`]: the construct store + interface (**placeholder behaviours** for now).
//! - [`space`]: portals, force fields, links, Shrink/Grow; [`extract`]: animating terrain.
//! - [`projectile`], [`history`] (lag compensation), [`rules`] (lobby rules + match state machine).
//! - [`events`]: everything clients need to hear about; [`replicate`]: compact snapshots + deltas.
//! - [`weapons`]: gun archetypes (DOOM feel) + the generic weapon state machine.
//! - [`bots`]: AI players for tests, practice and stress; [`fuzz`]: random recipes + chaos worlds.
//! - [`settings`]: every lobby knob + presets; [`party`]: Party mode (rounds, picks, effects).
//! - [`tuning`]: every combat / economy number (`assets/tuning/combat.ron`, [`World::set_tuning`]);
//!   [`telemetry`]: hidden per-match balance recorder (`logs/matches/*.json`, tournaments).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod aim_assist; // CONNECT: melee hit volume, projectile / thrown homing, hitscan magnetism
pub mod bots;
pub mod charge;
pub mod combat;
pub mod constructs;
pub mod dodge; // DODGE: intangibility window + hits restore the air jump (movement/air_jump.rs, dodge.rs)
pub mod events;
pub mod extract;
pub mod first_call; // FIRST CALL + BURN ON DEATH: named-thing claims / rival versions, burns / echoes (host rules; off = no-op)
pub mod freedom; // FREEDOM HOOK: "it", name targeting, sputter, shout bonus, kill feed, highlights
pub mod fuzz;
pub mod geom;
pub mod history;
pub mod limit; // TOTAL LIMIT: a host rule capping each player's active things (constructs + powers; off = no-op)
pub mod legendary; // FREEDOM HOOK: hidden novelty-driven legendary rolls (never shown)
pub mod match_stats; // SCOREBOARD: competitive per-player stats (observer over events; never writes the world)
pub mod juggle; // JUGGLE: per-victim juggle protection (knockback decay + short guard; COMBAT.md §1a)
pub mod hook; // ASSIST HOOK: hook-and-strike (tethers hook players)
pub mod melee_assist; // ASSIST HOOK: melee lunge (Destiny-style dash, aim nudge)
pub mod movement;
pub mod party;
pub mod pickups; // POWER-UPS: gated big things + map pickups (MatchRules::powerups; off = no-op)
pub mod powers;
pub mod presence_tier; // PRESENCE: a construct's gravitas tier (pure; bd_game presence + the phrase tool)
pub mod projectile;
pub mod readiness; // POWER HUD: held powers' readiness / forms + own status timers (replicated; bd_game power_hud)
pub mod replicate;
pub mod rng;
pub mod rules;
pub mod settings;
pub mod slipstream; // SLIPSTREAM HOOK: the Hyperline train's wind on players (host + prediction)
pub mod signature; // SIGNATURE HOOK: signature casts = an amplifier over existing mechanics (COMBAT.md §5j)
pub mod smash;
pub mod space;
pub mod status;
pub mod taunt; // TAUNT & VOICE TELL: wordless body taunts + the voice-cast tell (presentation state, no gameplay effect)
pub mod telemetry;
pub mod tuning;
pub mod weapons;
pub mod willpower;
pub mod world;

pub use combat::{HitInfo, HitKind, HitResult, ImpactFx};
pub use events::GameEvent;
pub use replicate::{DeltaDecoder, DeltaEncoder, SnapshotDelta, WorldSnapshot};
pub use rules::{ConstructLimits, MatchPhase, MatchRules, ScaleLimit, WillpowerMode};
pub use world::{Aim, ArenaSource, Player, SimConfig, TargetHint, World};
