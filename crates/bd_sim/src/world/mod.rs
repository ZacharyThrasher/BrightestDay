//! The authoritative game world: one deterministic 120 Hz step for everything.
//!
//! ```text
//! let mut w = World::new(SimConfig::default());
//! w.add_player(PlayerId(0), "host".into(), Corps::Willpower)?;
//! loop {
//!     w.set_view_tick(id, packet.view_tick);           // lag compensation (optional)
//!     let events = w.tick(&[(id, input)]);             // 1 tick = DT
//!     // send events (reliable lane) + w.snapshot_for(Some(id)) (latest lane) …
//!     w.cast(id, recipe, None, aim);                   // commands between ticks
//! }
//! ```
//!
//! Order of a tick (see [`World::tick`]):
//! 1. inputs → players (edges), match clock, per-player timers (status DoT, size, cooldowns,
//!    spawn shield, respawn)
//! 2. construct colliders → movement for every living player (modifiers from statuses, worn
//!    constructs and size; seated pilots ride their seat), wall splats, portals, force fields
//! 3. light player-vs-player push, link constraints, void / out-of-bounds KOs, lag-comp history
//! 4. ring blast / shove, projectiles (swept, lag-compensated)
//! 5. construct behaviours → their intents (hits, explosions, carves, shots, impulses…) applied
//! 6. portals / fields / links housekeeping, willpower regen + upkeep + starvation, reap
//! 7. rate-limited / progressive terrain carves, match end check, invariants (debug)
//!
//! Events produced by commands between ticks (casts, oath, joins) are delivered with the next
//! tick's events. The event buffer and all per-tick scratch buffers are reused: a steady-state tick
//! does not allocate.

mod actions;
mod assist; // ASSIST HOOK: melee lunge + hook-and-strike plumbing (crate::melee_assist, crate::hook)
pub mod bounds; // BOUNDS HOOK: blast zones, tether save, construct recall, visible placement
mod combat;
mod invariants;
mod net;
mod player;
mod projectiles;
mod punch;
mod spatial;

use std::collections::VecDeque;

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::voxel::Aabb;
use bd_core::{DT, MAX_PLAYERS, PlayerId, Tick, Vec3};
use bd_voxel::{ArenaInfo, CarveOp, VoxelWorld, generate_arena, generate_flat_test};
use serde::{Deserialize, Serialize};

use crate::constructs::{ConstructOut, ConstructStore, PlayerView};
use crate::events::{GameEvent, GoneReason, move_flags};
use crate::history::HullHistory;
use crate::movement::{Collider, CollisionWorld, MoveModifiers, MoveParams, hand_pos, step_player, Hand};
use crate::projectile::Projectile;
use crate::rng::SimRng;
use crate::rules::{MatchPhase, MatchRules, MatchState, MatchTimings, ScoreRow, sort_scoreboard};
use crate::space::{ForceField, Link, PortalPair};
use crate::willpower::OathTable;

pub use actions::{Aim, TargetHint};
pub use combat::{EMBER_COLLAPSE_LIMITS, SIM_COLLAPSE_LIMITS, apply_carve_op, collapse_limits_for};
pub use invariants::{MAX_CONSTRUCTS, MAX_PROJECTILES};
pub use net::PredictCtx;
pub use player::{Player, Seat};

/// Which terrain to build.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ArenaSource {
    /// The seeded floating-islands arena.
    Archipelago { seed: u64 },
    /// Flat movement lab (fast to generate; tests).
    Flat { size_m: f32 },
}

/// Everything that configures a world. Serde so a lobby can send it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SimConfig {
    pub arena: ArenaSource,
    /// Seeds all sim randomness (independent of the arena seed).
    pub seed: u64,
    pub rules: MatchRules,
    pub timings: MatchTimings,
    pub move_params: MoveParams,
    /// Max carve ops applied per tick (the rest wait).
    pub carves_per_tick: usize,
    /// Estimated voxels carved per tick before the rest wait.
    pub carve_voxel_budget: f32,
    /// Ring blasts chip terrain (tiny carve) on impact.
    pub blast_chips_terrain: bool,
    pub oaths: OathTable,
    /// Run [`World::check_invariants`] every tick and panic on failure (defaults to debug builds).
    pub check_invariants: bool,
    /// Party mode (COMBAT.md §5g); `None` = FFA. Built by `settings::GameSettings::sim_config`.
    pub party: Option<crate::settings::PartySettings>,
    /// TUNING HOOK: every combat / economy number (`assets/tuning/combat.ron`, COMBAT.md §5i).
    pub tuning: crate::tuning::CombatTuning,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            arena: ArenaSource::Archipelago { seed: 1 },
            seed: 1,
            rules: MatchRules::default(),
            timings: MatchTimings::default(),
            move_params: MoveParams::default(),
            carves_per_tick: 12,
            carve_voxel_budget: 4000.0,
            blast_chips_terrain: false,
            oaths: OathTable::default(),
            check_invariants: cfg!(debug_assertions),
            party: None,
            tuning: crate::tuning::CombatTuning::default(),
        }
    }
}

/// Why `add_player` refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinError {
    Full,
    IdTaken,
}

pub struct World {
    pub cfg: SimConfig,
    pub voxels: VoxelWorld,
    pub arena: ArenaInfo,
    pub tick: Tick,
    /// Sorted by id.
    pub players: Vec<Player>,
    pub projectiles: Vec<Projectile>,
    pub constructs: ConstructStore,
    pub portals: Vec<PortalPair>,
    pub fields: Vec<ForceField>,
    pub links: Vec<Link>,
    pub match_state: MatchState,
    pub history: HullHistory,
    /// Sim randomness (seeded from `SimConfig::seed`) for the construct engineer's behaviours.
    pub rng: SimRng,
    pub(crate) events: Vec<GameEvent>,
    /// Events already returned by the previous tick (dropped at the start of the next).
    delivered: usize,
    colliders: Vec<Collider>,
    views: Vec<PlayerView>,
    cout: ConstructOut,
    /// (due tick, op), sorted by due tick.
    carve_queue: VecDeque<(Tick, CarveOp)>,
    pub(crate) carve_seed: u32,
    next_projectile: u16,
    next_space_id: u16,
    /// Seed of the terrain currently loaded (rematch regenerates from it).
    arena_seed: u64,
    /// Last invariant violation seen by the per-tick check (release builds keep running).
    pub last_invariant_error: Option<(Tick, String)>,
    /// Carve ops already applied this tick (punch-through) — counted against `carves_per_tick`.
    pub(crate) carve_ops_used: usize,
    /// `cast_shaped`'s authored robot shape while the cast runs (consumed by `spawn_cast`).
    pending_alt_shape: Option<bd_core::shape::ShapeSpec>,
    /// Burning voxels last announced (`FireVoxels`).
    fire_seen: Vec<bd_core::IVec3>,
    /// Next arena being built on a worker thread (never on the tick): (source, job).
    arena_job: Option<(ArenaSource, std::thread::JoinHandle<(VoxelWorld, ArenaInfo, u64)>)>,
    // ---- PARTY HOOK (crate::party) ----
    /// Party mode state (`None` = FFA).
    pub party: Option<crate::party::PartyState>,
    /// Cost discount for the cast in progress (party grafts are free); `None` outside party casts.
    pub(crate) party_cost_ratio: Option<f32>,
    /// The cast in progress is a party effect's free copy (no cost / staleness / replacement).
    pub(crate) free_cast: bool,
    // ---- POWERS HOOK (crate::powers, COMBAT.md §5h) ----
    /// Fruits in the world, the restore log, the incantation in progress.
    pub powers: crate::powers::PowerWorld,
    /// BOUNDS HOOK: recent construct recalls (id, tick) for the anti-loop valve (`world/bounds.rs`).
    pub(crate) recall_log: Vec<(bd_core::ConstructId, Tick)>,
    /// SIGNATURE HOOK: the signature cast in progress + live orbital emitters (`crate::signature`).
    pub(crate) signature: crate::signature::SignatureState,
}

fn build_arena(src: ArenaSource) -> (VoxelWorld, ArenaInfo, u64) {
    match src {
        ArenaSource::Archipelago { seed } => {
            let (w, i) = generate_arena(seed);
            (w, i, seed)
        }
        ArenaSource::Flat { size_m } => {
            let (w, i) = generate_flat_test(if size_m.is_finite() { size_m.clamp(32.0, 256.0) } else { 96.0 });
            (w, i, 0)
        }
    }
}

impl World {
    pub fn new(cfg: SimConfig) -> Self {
        let (voxels, arena, arena_seed) = build_arena(cfg.arena);
        Self::with_arena(cfg, voxels, arena, arena_seed)
    }

    /// Build around an existing voxel world (e.g. a client that generated / received it).
    pub fn with_arena(mut cfg: SimConfig, voxels: VoxelWorld, arena: ArenaInfo, arena_seed: u64) -> Self {
        let rng = SimRng::new(cfg.seed);
        // PARTY HOOK: a party round is a match with the round KO target / round time.
        let party = cfg.party.clone().map(|s| {
            let s = s.validated();
            cfg.rules.ko_target = s.round_ko_target;
            cfg.rules.time_limit = s.round_time;
            crate::party::PartyState::new(s, cfg.seed)
        });
        Self {
            voxels,
            arena,
            tick: 0,
            players: Vec::with_capacity(MAX_PLAYERS),
            projectiles: Vec::with_capacity(64),
            constructs: ConstructStore::new(),
            portals: Vec::new(),
            fields: Vec::new(),
            links: Vec::new(),
            match_state: MatchState::default(),
            history: HullHistory::default(),
            rng,
            events: Vec::with_capacity(256),
            delivered: 0,
            colliders: Vec::with_capacity(128),
            views: Vec::with_capacity(MAX_PLAYERS),
            cout: ConstructOut::default(),
            carve_queue: VecDeque::with_capacity(64),
            carve_seed: 1,
            next_projectile: 1,
            next_space_id: 1,
            arena_seed,
            last_invariant_error: None,
            pending_alt_shape: None,
            carve_ops_used: 0,
            fire_seen: Vec::with_capacity(crate::constructs::chemistry::MAX_BURNING),
            arena_job: None,
            party,
            party_cost_ratio: None,
            free_cast: false,
            powers: crate::powers::PowerWorld::default(), // POWERS HOOK
            recall_log: Vec::new(), // BOUNDS HOOK
            signature: Default::default(), // SIGNATURE HOOK
            cfg,
        }
    }

    // ------------------------------------------------------------------------------ accessors

    #[inline]
    pub fn params(&self) -> &MoveParams {
        &self.cfg.move_params
    }

    #[inline]
    pub fn rules(&self) -> &MatchRules {
        &self.cfg.rules
    }

    #[inline]
    pub(crate) fn pidx(&self, id: PlayerId) -> Option<usize> {
        self.players.binary_search_by_key(&id, |p| p.id).ok()
    }

    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.pidx(id).map(|i| &self.players[i])
    }

    pub fn player_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.pidx(id).map(move |i| &mut self.players[i])
    }

    /// Events generated since the last tick returned (commands between ticks + this tick).
    pub fn events(&self) -> &[GameEvent] {
        &self.events
    }

    /// Construct colliders as fed to movement this tick.
    pub fn colliders(&self) -> &[Collider] {
        &self.colliders
    }

    /// Scoreboard, best first.
    pub fn scoreboard(&self) -> Vec<ScoreRow> {
        let mut rows: Vec<ScoreRow> = self.players.iter().map(|p| ScoreRow { player: p.id, kos: p.kos, deaths: p.deaths }).collect();
        sort_scoreboard(&mut rows);
        rows
    }

    pub fn time_left(&self) -> f32 {
        self.match_state.time_left(&self.cfg.rules, &self.cfg.timings)
    }

    /// Add an event from outside the sim (tools, the chaos soak); delivered with the next tick.
    pub fn inject_event(&mut self, e: GameEvent) {
        self.events.push(e);
    }

    pub(crate) fn next_space_id(&mut self) -> u16 {
        let id = self.next_space_id;
        self.next_space_id = self.next_space_id.wrapping_add(1).max(1);
        id
    }

    // ------------------------------------------------------------------------------ players

    /// Add a player (mid-match joins are fine). They spawn on the next tick.
    pub fn add_player(&mut self, id: PlayerId, name: String, corps: Corps) -> Result<(), JoinError> {
        if self.pidx(id).is_some() {
            return Err(JoinError::IdTaken);
        }
        if self.players.len() >= MAX_PLAYERS {
            return Err(JoinError::Full);
        }
        let mut name = name;
        if name.len() > 32 {
            let mut cut = 32;
            while !name.is_char_boundary(cut) {
                cut -= 1;
            }
            name.truncate(cut);
        }
        let pos = self.arena.center;
        let mut p = Player::new(id, name.clone(), corps, pos, &self.cfg.move_params);
        p.willpower = self.cfg.rules.willpower_max();
        if let crate::rules::Scoring::Stock { lives } = self.cfg.rules.scoring {
            p.stocks = Some(lives.max(1));
        }
        let at = self.players.partition_point(|q| q.id < id);
        self.players.insert(at, p);
        self.events.push(GameEvent::PlayerJoined { player: id, name, corps });
        Ok(())
    }

    /// Remove a player: their constructs dissolve, their projectiles vanish, references to them are
    /// cleared.
    pub fn remove_player(&mut self, id: PlayerId) -> bool {
        let Some(i) = self.pidx(id) else { return false };
        if let Some(seat) = self.players[i].seat {
            if let Some(c) = self.constructs.get_mut(seat.construct) {
                c.pilot = None;
            }
        }
        self.players.remove(i);
        self.constructs.mark_owner_gone(id, GoneReason::OwnerGone);
        for c in self.constructs.iter_mut() {
            if c.pilot == Some(id) {
                c.pilot = None;
            }
            if c.target == Some(id) {
                c.target = None;
            }
        }
        let mut scratch = std::mem::take(&mut self.cout.ejects);
        let events = &mut self.events;
        let players = &mut self.players;
        self.constructs.reap(events, &mut scratch, |owner, amt| {
            if let Ok(k) = players.binary_search_by_key(&owner, |p| p.id) {
                players[k].willpower += amt;
            }
        });
        scratch.clear();
        self.cout.ejects = scratch;
        for p in &mut self.players {
            if p.last_attacker.is_some_and(|(a, _)| a == id) {
                p.last_attacker = None;
            }
            if p.last_target == Some(id) {
                p.last_target = None;
            }
            for s in p.status.sources.iter_mut() {
                if *s == Some(id) {
                    *s = None;
                }
            }
        }
        for pr in &mut self.projectiles {
            if pr.spec.owner == id {
                pr.dead = true;
            }
        }
        self.projectiles.retain(|p| !p.dead);
        self.remove_space_refs_player(id);
        self.events.push(GameEvent::PlayerLeft { player: id });
        true
    }

    /// A player dropped (connection lost): remove them like [`Self::remove_player`] but return what
    /// a reconnect restores — same `PlayerId`, score and stocks (they respawn after the usual delay).
    /// The host keeps it under its own reconnect token and hands it to [`Self::rejoin_player`].
    pub fn remove_player_for_reconnect(&mut self, id: PlayerId) -> Option<DepartedPlayer> {
        let p = self.player(id)?;
        let d = DepartedPlayer {
            id,
            name: p.name.clone(),
            corps: p.corps,
            kos: p.kos,
            deaths: p.deaths,
            stocks: p.stocks,
            eliminated: p.eliminated,
            round: self.match_state.round,
        };
        self.remove_player(id);
        Some(d)
    }

    /// Re-admit a dropped player under their old id. Score / stocks come back if it is still the
    /// same round (a rematch in between starts them fresh, like everyone else).
    pub fn rejoin_player(&mut self, d: &DepartedPlayer) -> Result<(), JoinError> {
        self.add_player(d.id, d.name.clone(), d.corps)?;
        if d.round == self.match_state.round {
            if let Some(p) = self.player_mut(d.id) {
                p.kos = d.kos;
                p.deaths = d.deaths;
                if p.stocks.is_some() {
                    p.stocks = d.stocks;
                }
                p.eliminated = d.eliminated;
            }
        }
        Ok(())
    }

    /// Lag compensation: the (fractional) tick this player's client was rendering when it sampled
    /// its latest input (`bd_net::InputPacket::view_tick`). `None` disables rewinding.
    pub fn set_view_tick(&mut self, id: PlayerId, view_tick: Option<f64>) {
        if let Some(p) = self.player_mut(id) {
            p.view_tick = view_tick.filter(|v| v.is_finite());
        }
    }

    // ------------------------------------------------------------------------------ tick

    /// Advance the world by one tick (`DT`). Players without an input this tick repeat their last
    /// one (minus one-shot reel notches). Returns every event since the previous `tick` call.
    pub fn tick(&mut self, inputs: &[(PlayerId, PlayerInput)]) -> &[GameEvent] {
        self.events.drain(..self.delivered.min(self.events.len()));
        self.tick += 1;
        let dt = DT;

        self.apply_inputs(inputs);
        self.step_match(dt);
        self.step_player_timers(dt);
        self.step_charge(dt); // CHARGE HOOK: hold-to-power-up (crate::charge)

        // Constructs feed movement.
        self.colliders.clear();
        self.constructs.write_colliders(&mut self.colliders);

        self.carve_ops_used = 0;
        self.punch_through(dt); // Smash: launched players plow through terrain (world/punch.rs)
        let n_cols = self.assist_push_player_colliders(); // ASSIST HOOK: players are hook targets during movement
        self.step_movement(dt);
        self.assist_post_move(n_cols, dt); // ASSIST HOOK: hook-and-strike rope rules
        self.push_players_apart(dt);
        self.solve_links(dt);
        self.check_out_of_bounds();
        self.record_history();

        self.player_actions();
        self.step_projectiles(dt);
        self.step_constructs(dt);
        crate::signature::step(self, dt); // SIGNATURE HOOK: orbital strikes (no-op without orbitals)
        self.step_space(dt);
        self.step_powers(dt); // POWERS HOOK: stands, fruits, drain/seastone, time stop (no-op without powers)
        self.step_willpower(dt);
        // PARTY HOOK: effect hooks (this tick's events so far, scheduled casts, per-tick effects);
        // before the reap so whatever they break is removed this tick.
        crate::party::hooks::end_of_tick(self, dt);
        self.reap_constructs();
        self.apply_carves();
        self.check_match_end();

        if self.cfg.check_invariants {
            if let Err(e) = self.check_invariants() {
                // Debug builds stop right here; release builds record it (tools read it) and go on.
                debug_assert!(false, "bd_sim invariant violated at tick {}: {e}", self.tick);
                self.last_invariant_error = Some((self.tick, e));
            }
        }
        self.delivered = self.events.len();
        &self.events
    }

    fn apply_inputs(&mut self, inputs: &[(PlayerId, PlayerInput)]) {
        for p in &mut self.players {
            // Repeat last input when none arrives (clear one-shot fields).
            p.input.reel_notches = 0;
        }
        for (id, inp) in inputs {
            if let Some(i) = self.pidx(*id) {
                let p = &mut self.players[i];
                p.input = sanitize_input(inp);
                p.last_input_seq = inp.seq;
            }
        }
        for p in &mut self.players {
            let b = p.input.buttons;
            p.pressed = b & !p.prev_buttons;
            p.released = p.prev_buttons & !b;
            p.prev_buttons = b;
        }
    }

    fn step_player_timers(&mut self, dt: f32) {
        for i in 0..self.players.len() {
            let p = &mut self.players[i];
            p.blast_cd = (p.blast_cd - dt).max(0.0);
            p.shove_cd = (p.shove_cd - dt).max(0.0);
            p.regen_pause = (p.regen_pause - dt).max(0.0);
            p.spawn_shield = (p.spawn_shield - dt).max(0.0);
            p.portal_cd = (p.portal_cd - dt).max(0.0);
            p.eject_cd = (p.eject_cd - dt).max(0.0);
            p.size.step(dt);
            if !p.alive {
                if p.eliminated {
                    continue;
                }
                p.respawn_timer -= dt;
                if p.respawn_timer <= 0.0 {
                    self.respawn(i);
                }
                continue;
            }
            let (dmg, src) = p.status.tick(dt);
            if dmg > 0.0 {
                self.damage_over_time(i, dmg, src);
            }
        }
    }

    fn step_movement(&mut self, dt: f32) {
        let Self { players, voxels, colliders, constructs, cfg, portals, fields, events, party, arena, history, .. } = self;
        let base = &cfg.move_params;
        for p in players.iter_mut() {
            p.move_events = Default::default();
            if !p.alive {
                continue;
            }
            // Cockpit seats: the hull rides the seat; the construct gets the input.
            if let Some(seat) = p.seat.filter(|s| !s.suit) {
                match constructs.get(seat.construct) {
                    Some(c) => {
                        p.mv.pos = c.seat_pos(seat.part);
                        p.mv.vel = c.vel;
                        p.mv.grounded = false;
                        p.mv.release_tethers();
                        continue;
                    }
                    None => p.seat = None,
                }
            }
            // Worn air moves (wing flap / cape dash) — shared with client prediction.
            let busy = p.busy();
            crate::constructs::worn::pre_move(constructs, p.id, &p.input, &mut p.mv, busy, |e| events.push(e));
            let mut mods = MoveModifiers::default();
            p.status.modify_movement(&mut mods);
            mods.speed_mult *= p.size.speed_mult();
            if p.seat.is_some_and(|s| s.suit) {
                mods.speed_mult *= 0.85;
            }
            constructs.movement_mods(p.id, &p.input, &p.mv, &mut mods);
            p.power.modify_movement(p.mv.grounded, &mut mods); // POWERS HOOK (body, time stop, void sink)
            crate::charge::modify_movement(&cfg.tuning.charge, &p.input, p.mv.grounded, &mut mods); // CHARGE HOOK (feet planted)
            if let Some(g) = arena.gravity {
                mods.gravity_scale *= g.scale_at(p.mv.pos.y); // MAP HOOK: gravity by altitude (Orbit)
            }
            p.mv.mods = mods;
            let input = if p.status.stunned() { stunned_input(&p.input) } else { p.input };
            let mut params = p.params(base);
            crate::party::hooks::tweak_params(party.as_ref(), p.id, &mut params); // PARTY HOOK (Slippery)
            let was_tumbling = p.mv.tumble > 0.0;
            // Smash: tech buffer (Jump pressed while tumbling) and DI (steer the launch).
            let sm = &cfg.tuning.smash;
            if was_tumbling && p.pressed & buttons::JUMP != 0 {
                p.tech_buffer = 0.0;
            } else {
                p.tech_buffer = (p.tech_buffer + dt).min(99.0);
            }
            if let Some(l) = p.launch.as_mut() {
                l.di += crate::smash::apply_di(sm, &mut p.mv, &p.input, dt);
                l.t += dt;
            } else {
                p.mv.di_window = 0.0;
            }
            crate::melee_assist::dash_pre_move(&mut p.mv, &params, dt); // ASSIST HOOK: melee lunge dash (predicted too)
            let pre_vel = p.mv.vel;
            let pre_tumble = p.mv.tumble;
            // Plowing through rock is flight, not running: no ground friction in the tunnel.
            if p.punch.is_some() {
                p.mv.grounded = false;
            }
            let pre_center = p.hull(base).center();
            assist::rewind_player_colliders(colliders, history, p.view_tick, cfg.tuning.assist.hook.radius); // ASSIST HOOK: hook targets as this player saw them
            let cw = CollisionWorld::new(voxels, colliders).ignoring(crate::movement::ColliderId::Player(p.id));
            let ev = step_player(&mut p.mv, &input, &cw, &params, dt);
            constructs.post_move(p.id, &p.input, &mut p.mv, pre_vel, &ev, &params);
            p.move_events = ev;
            // Plowing through terrain: landing on the tunnel floor doesn't end the tumble.
            if p.punch.is_some() && pre_tumble > 0.0 && p.mv.tumble <= 0.0 {
                p.mv.tumble = (pre_tumble - dt).max(dt);
            }
            // Wall splat: tumbling into a wall fast (resolved after the loop; needs &mut self).
            p.pending_splat = ev.wall_hit.filter(|w| {
                was_tumbling && w.normal.y.abs() < 0.7 && w.speed > cfg.tuning.hit.splat_min_speed
            });
            if p.mv.grounded {
                p.last_safe_pos = p.mv.pos;
            }
            // Tech: Jump pressed just before a wall / ground contact while tumbling.
            let contact = ev.wall_hit.map(|w| w.normal).or(ev.landed.map(|_| Vec3::Y));
            if let Some(n) = contact.filter(|_| was_tumbling && p.tech_buffer <= cfg.tuning.smash.tech_window) {
                p.pending_splat = None;
                p.mv.vel *= 0.3;
                p.mv.tumble = 0.0;
                p.launch = None;
                p.tech_buffer = 99.0;
                events.push(GameEvent::Tech { player: p.id, pos: p.mv.pos, normal: n });
            }
            // The launch is over once the tumble ends in play (landed / tethered / timed out).
            if let Some(l) = p.launch.filter(|_| p.mv.tumble <= 0.0) {
                p.launch = None;
                events.push(GameEvent::Recovered { player: p.id, source: l.source, pct: p.damage_pct });
            }
            // Portals + fields (shared with client prediction).
            let tethered = p.mv.tethered();
            if let Some((portal, rot)) = spatial::player_portals(&mut p.mv, &mut p.input, &mut p.portal_cd, &params, pre_center, pre_vel, portals) {
                events.push(GameEvent::Teleported { who: crate::events::Traveller::Player(p.id), portal, rot, pos: p.mv.pos });
            }
            spatial::player_fields(&mut p.mv, p.id, tethered, p.size.scale, fields, dt);
            let mut flags = 0u16;
            if ev.jumped {
                flags |= move_flags::JUMP;
            }
            if ev.perfect_hop {
                flags |= move_flags::PERFECT_HOP;
            }
            if ev.landed.is_some_and(|l| l.impact_speed > 12.0) {
                flags |= move_flags::LAND_HARD;
            }
            if ev.slide_start {
                flags |= move_flags::SLIDE;
            }
            if ev.tether_fired.iter().any(|b| *b) {
                flags |= move_flags::TETHER_FIRE;
            }
            if ev.tether_attached.iter().any(|b| *b) {
                flags |= move_flags::TETHER_ATTACH;
            }
            if ev.tether_released.iter().any(|b| *b) {
                flags |= move_flags::TETHER_RELEASE;
            }
            if ev.boost_start {
                flags |= move_flags::BOOST;
            }
            if ev.wall_hit.is_some() {
                flags |= move_flags::WALL_HIT;
            }
            if ev.air_jumped {
                flags |= move_flags::AIR_JUMP;
            }
            if ev.dodged {
                flags |= move_flags::DODGE;
            }
            if flags != 0 {
                events.push(GameEvent::Move { player: p.id, flags, speed: p.mv.speed() });
            }
        }
        self.resolve_wall_splats();
        // Eject from cockpits: E (TETHER_R) pressed while seated.
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if p.alive && p.seat.is_some_and(|s| !s.suit) && p.pressed & buttons::TETHER_R != 0 && p.eject_cd <= 0.0 {
                let id = p.id;
                self.eject(id);
            }
        }
    }

    fn push_players_apart(&mut self, dt: f32) {
        let n = self.players.len();
        let base = self.cfg.move_params.clone();
        for i in 0..n {
            for j in (i + 1)..n {
                let (a, b) = (&self.players[i], &self.players[j]);
                if !a.alive || !b.alive || a.seat.is_some_and(|s| !s.suit) || b.seat.is_some_and(|s| !s.suit) {
                    continue;
                }
                let (ha, hb) = (a.hull(&base), b.hull(&base));
                if !ha.intersects(&hb) {
                    continue;
                }
                let d = Vec3::new(hb.center().x - ha.center().x, 0.0, hb.center().z - ha.center().z);
                let dist = d.length();
                let dir = if dist > 1e-4 { d / dist } else { Vec3::X };
                let overlap = (ha.half().x + hb.half().x) - dist;
                // Light push: a few m/s apart, scaled by overlap.
                let push = (overlap * 20.0).min(4.0) * dt * 60.0 * 0.1;
                self.players[i].mv.vel -= dir * push;
                self.players[j].mv.vel += dir * push;
            }
        }
    }

    /// Smash blast zones: below the void, past the side bounds (+ margin), over the top (+ margin,
    /// a star KO). The only way to be KO'd.
    fn check_out_of_bounds(&mut self) {
        // Blast zones: the terrain's own extent + margins (ArenaInfo.bounds already pads it).
        let b = self.voxels.bounds();
        let void_y = self.arena.void_y;
        let (side, top) = (self.cfg.tuning.smash.blast_margin_side, self.cfg.tuning.smash.blast_margin_top);
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive {
                continue;
            }
            let pos = p.mv.pos;
            let zone = if !pos.is_finite() || pos.y < void_y {
                Some(crate::smash::BlastZone::Bottom)
            } else if pos.y > b.max.y + top {
                Some(crate::smash::BlastZone::Top)
            } else if pos.x < b.min.x - side || pos.x > b.max.x + side || pos.z < b.min.z - side || pos.z > b.max.z + side {
                Some(crate::smash::BlastZone::Side)
            } else {
                None
            };
            // BOUNDS HOOK (world/bounds.rs): an attached tether anchored in the arena holds you past
            // the side / bottom zones; only the star KO (top) still applies.
            let zone = zone.filter(|z| *z == crate::smash::BlastZone::Top || !self.tether_holds(i));
            let out = zone.is_some();
            if out && !self.cfg.rules.void_ko && pos.is_finite() {
                // Lobby knob "void off": back onto a spawn point, no KO.
                let id = p.id;
                let at = self.pick_spawn(id);
                let p = &mut self.players[i];
                p.mv.pos = at;
                p.mv.vel = Vec3::ZERO;
                p.mv.release_tethers();
                self.events.push(GameEvent::VoidRescue { player: id, pos: at });
            } else if let Some(z) = zone {
                let killer = self.credit(i);
                let cause = if z == crate::smash::BlastZone::Bottom { crate::events::KoCause::Void } else { crate::events::KoCause::BlastZone };
                self.ko_zone(i, killer, cause, Some(z));
            }
        }
    }

    fn record_history(&mut self) {
        let base = &self.cfg.move_params;
        let tick = self.tick;
        let players = &self.players;
        self.history.record(tick, players.iter().filter(|p| p.alive).map(|p| (p.id, p.hull(base))));
    }

    /// Rebuild the read-only player views constructs see.
    fn build_views(&mut self) {
        self.views.clear();
        let base = &self.cfg.move_params;
        for p in &self.players {
            let params = p.params(base);
            self.views.push(PlayerView {
                id: p.id,
                alive: p.alive,
                shielded: p.shielded(),
                pos: p.mv.pos,
                vel: p.mv.vel,
                hull: p.mv.hull(&params),
                eye: p.mv.eye(&params),
                hand: hand_pos(&p.mv, &params, p.input.yaw, p.input.pitch, Hand::Right),
                look: p.look(),
                yaw: p.input.yaw,
                pitch: p.input.pitch,
                input: p.input,
                pressed: p.pressed,
                released: p.released,
                grounded: p.mv.grounded,
                busy: p.busy(),
                willpower: p.willpower,
                last_target: p.last_target,
                seat: p.seat.map(|s| s.construct),
                size: p.size.scale,
                view_tick: p.view_tick,
            });
        }
    }

    fn step_constructs(&mut self, dt: f32) {
        self.build_views();
        self.assist_bias_views(); // ASSIST HOOK: melee swing hitbox aim nudge
        self.cout.clear();
        {
            let ctx = crate::constructs::ConstructCtx {
                tick: self.tick,
                dt,
                players: &self.views,
                voxels: &self.voxels,
                colliders: &self.colliders,
                params: &self.cfg.move_params,
                history: &self.history,
                rules: &self.cfg.rules,
                void_y: self.arena.void_y,
                gravity: self.arena.gravity, // MAP HOOK (Orbit gravity by altitude)
                tuning: &self.cfg.tuning, // TUNING HOOK
            };
            self.constructs.update(&ctx, &mut self.cout);
        }
        // BOUNDS HOOK (world/bounds.rs): free bodies that left the map (thrown into the sky / the
        // void) blink back to their summoner, or are gone (summoner KO'd, anti-loop valve).
        self.recall_lost_constructs();
        self.assist_after_constructs(); // ASSIST HOOK: swing starts (lunge dash, slam), lunge / slam hits
        self.apply_construct_out();
        self.announce_transforms();
        self.announce_fire();
    }

    /// `Transformed` whenever a transformer's target mode / combined state changes.
    fn announce_transforms(&mut self) {
        for c in self.constructs.iter_mut() {
            if c.gone.is_some() {
                continue;
            }
            let key = net::xform_key(c);
            if key != c.xform_seen {
                // First sighting of a fresh alt-mode transformer is not a toggle.
                if c.xform_seen != 0 || key & (64 | 128) != 0 {
                    self.events.push(GameEvent::Transformed { id: c.id, robot: key & 128 != 0, combined: key & 64 != 0 });
                }
                c.xform_seen = key;
            }
        }
    }

    /// `FireVoxels` when the set of burning terrain voxels changed (merge-walk of two sorted lists;
    /// allocates only when something changed).
    fn announce_fire(&mut self) {
        let burning = self.constructs.chemistry().burning();
        if burning.is_empty() && self.fire_seen.is_empty() {
            return;
        }
        let key = |v: bd_core::IVec3| (v.x, v.y, v.z);
        let q = |v: bd_core::IVec3| [v.x.clamp(-32768, 32767) as i16, v.y.clamp(-32768, 32767) as i16, v.z.clamp(-32768, 32767) as i16];
        let (mut lit, mut out) = (Vec::new(), Vec::new());
        let (mut i, mut j) = (0, 0);
        while i < self.fire_seen.len() || j < burning.len() {
            match (self.fire_seen.get(i), burning.get(j)) {
                (Some(a), Some(b)) if key(*a) == key(b.v) => {
                    i += 1;
                    j += 1;
                }
                (Some(a), Some(b)) if key(*a) < key(b.v) => {
                    out.push(q(*a));
                    i += 1;
                }
                (Some(a), None) => {
                    out.push(q(*a));
                    i += 1;
                }
                (_, Some(b)) => {
                    lit.push(q(b.v));
                    j += 1;
                }
                (None, None) => break,
            }
        }
        if lit.is_empty() && out.is_empty() {
            return;
        }
        self.fire_seen.clear();
        self.fire_seen.extend(burning.iter().map(|b| b.v));
        self.events.push(GameEvent::FireVoxels { lit, out });
    }

    fn step_willpower(&mut self, dt: f32) {
        let infinite = self.cfg.rules.infinite_willpower();
        let max = self.cfg.rules.willpower_max();
        let regen = self.cfg.tuning.willpower.regen_per_sec * self.cfg.rules.regen_mult();
        let starved = self.cfg.tuning.willpower.starved_decay;
        for i in 0..self.players.len() {
            let id = self.players[i].id;
            if infinite {
                self.players[i].willpower = max;
                continue;
            }
            let upkeep: f32 = self
                .constructs
                .owned_by(id)
                .map(|c| self.cfg.tuning.willpower.upkeep_per_sec_as(&c.recipe, c.pilot.is_some(), c.priced_autonomous)) // TUNING HOOK
                .sum();
            let p = &mut self.players[i];
            let gain = if p.regen_pause > 0.0 { 0.0 } else { regen };
            p.willpower = (p.willpower + (gain - upkeep) * dt).clamp(0.0, max);
            if p.willpower <= 0.0 && upkeep > 0.0 {
                self.constructs.decay_owner(id, starved, dt);
            }
        }
    }

    fn reap_constructs(&mut self) {
        // PARTY HOOK: shatter hooks (Main Character split, confetti, portals, black holes).
        crate::party::hooks::pre_reap(self);
        let mut ejects = std::mem::take(&mut self.cout.ejects);
        ejects.clear();
        {
            let players = &mut self.players;
            let max = self.cfg.rules.willpower_max();
            self.constructs.reap(&mut self.events, &mut ejects, |owner, amt| {
                if let Ok(k) = players.binary_search_by_key(&owner, |p| p.id) {
                    players[k].willpower = (players[k].willpower + amt).min(max);
                }
            });
        }
        for &(pid, cid, vel) in &ejects {
            self.eject_with(pid, cid, vel, true);
        }
        // Seats whose construct vanished without an eject record.
        for p in &mut self.players {
            if let Some(s) = p.seat {
                if self.constructs.get(s.construct).is_none() {
                    p.seat = None;
                }
            }
        }
        ejects.clear();
        self.cout.ejects = ejects;
        self.cleanup_space_refs();
    }

    // ------------------------------------------------------------------------------ match

    fn step_match(&mut self, dt: f32) {
        // PARTY HOOK: Party mode runs its own round / pick / podium flow.
        if self.party.is_some() {
            self.step_party_match(dt);
            return;
        }
        let t = self.cfg.timings.clone();
        match self.match_state.phase {
            MatchPhase::Warmup => {
                if self.players.len() >= t.min_players.max(1) {
                    self.match_state.phase_time += dt;
                    if self.match_state.phase_time >= t.warmup_secs {
                        self.start_live();
                    }
                } else {
                    self.match_state.phase_time = 0.0;
                }
            }
            MatchPhase::Live => {
                self.match_state.phase_time += dt;
            }
            MatchPhase::Ended => {
                self.match_state.phase_time += dt;
                if t.auto_rematch && self.match_state.phase_time >= t.ended_secs {
                    self.rematch();
                }
            }
        }
    }

    /// Force the match live now (skip the warmup countdown).
    pub fn start_live(&mut self) {
        // PARTY HOOK: in Party mode this starts the next round.
        if self.party.is_some() {
            self.party_start_round();
            return;
        }
        self.start_live_ffa();
    }

    /// The FFA "go live" (also used by party rounds).
    pub(crate) fn start_live_ffa(&mut self) {
        self.reset_round();
        self.match_state.phase = MatchPhase::Live;
        self.match_state.phase_time = 0.0;
        self.match_state.winner = None;
        self.events.push(GameEvent::MatchPhase { phase: MatchPhase::Live, round: self.match_state.round, winner: None });
    }

    /// Rematch with terrain the host generated ahead of time (e.g. on a worker thread, so the
    /// ~150 ms arena build never lands on a tick). Set `timings.auto_rematch = false` and call this
    /// when the scoreboard time is over.
    pub fn rematch_with_arena(&mut self, voxels: VoxelWorld, arena: ArenaInfo, seed: u64) {
        self.swap_arena(voxels, arena, seed);
        self.party_reset(); // PARTY HOOK: a rematch starts a fresh party
        self.match_state.round += 1;
        self.reset_round();
        self.match_state.phase = MatchPhase::Warmup;
        self.match_state.phase_time = 0.0;
        self.match_state.winner = None;
        self.events.push(GameEvent::MatchPhase { phase: MatchPhase::Warmup, round: self.match_state.round, winner: None });
    }

    /// Load pre-built terrain (rematch / party rounds) and tell clients to regenerate it.
    pub(crate) fn swap_arena(&mut self, voxels: VoxelWorld, arena: ArenaInfo, seed: u64) {
        // Freeing tens of MB of chunks isn't free either: drop the old terrain on a worker.
        let old = std::mem::replace(&mut self.voxels, voxels);
        if std::thread::Builder::new().name("bd_arena_drop".into()).spawn(move || drop(old)).is_err() {
            // (thread spawn failed: `old` was moved into the failed closure and dropped here)
        }
        self.arena = arena;
        self.arena_seed = seed;
        self.carve_queue.clear();
        self.carve_seed = 1;
        self.events.push(GameEvent::ArenaReset { seed });
    }

    /// Build terrain for `src` (about 150 ms for an archipelago: call it on a worker thread).
    pub fn build_arena(src: ArenaSource) -> (VoxelWorld, ArenaInfo, u64) {
        build_arena(src)
    }

    /// Start building the next round's terrain (`cfg.arena`) on a worker thread. Idempotent: a
    /// build already running for the same source is kept. Deterministic: the result depends only
    /// on the source; *when* it is swapped in is decided by the sim (`take_prefetched_arena`).
    pub fn prefetch_arena(&mut self) {
        let src = self.cfg.arena;
        if self.arena_job.as_ref().is_some_and(|(s, _)| *s == src) {
            return;
        }
        let spawned = std::thread::Builder::new().name("bd_arena_build".into()).spawn(move || build_arena(src));
        self.arena_job = spawned.ok().map(|h| (src, h));
    }

    /// Is a prefetched arena built and ready (no wait)?
    pub fn prefetched_arena_ready(&self) -> bool {
        self.arena_job.as_ref().is_some_and(|(_, h)| h.is_finished())
    }

    /// Take the prefetched terrain (waits for the worker if it hasn't finished — it was started
    /// seconds earlier, so this is normally instant). `None` if no prefetch was started.
    pub fn take_prefetched_arena(&mut self) -> Option<(VoxelWorld, ArenaInfo, u64)> {
        let (_, job) = self.arena_job.take()?;
        job.join().ok()
    }

    /// Like [`Self::take_prefetched_arena`] but only if it was built for `src`.
    pub fn take_prefetched_arena_for(&mut self, src: ArenaSource) -> Option<(VoxelWorld, ArenaInfo, u64)> {
        if self.arena_job.as_ref().is_some_and(|(s, _)| *s == src) { self.take_prefetched_arena() } else { None }
    }

    /// The seed of the terrain currently loaded (clients regenerate from it).
    pub fn arena_seed(&self) -> u64 {
        self.arena_seed
    }

    /// Back to warmup with fresh scores (and fresh terrain if the rules say so).
    pub fn rematch(&mut self) {
        self.party_reset(); // PARTY HOOK: a rematch starts a fresh party
        self.match_state.round += 1;
        if self.cfg.rules.regen_islands_between_rounds && self.carve_seed > 1 {
            // Never build on the tick: use the terrain prefetched since the match ended. Without
            // one (rematch forced early) keep the current terrain and prefetch for next time.
            match self.take_prefetched_arena() {
                Some((w, info, seed)) => self.swap_arena(w, info, seed),
                None => self.prefetch_arena(),
            }
        }
        self.reset_round();
        self.match_state.phase = MatchPhase::Warmup;
        self.match_state.phase_time = 0.0;
        self.match_state.winner = None;
        self.events.push(GameEvent::MatchPhase { phase: MatchPhase::Warmup, round: self.match_state.round, winner: None });
    }

    fn reset_round(&mut self) {
        self.powers_reset_round(); // POWERS HOOK: powers + fruits don't survive a new round
        self.constructs.clear();
        self.projectiles.clear();
        self.portals.clear();
        self.fields.clear();
        self.links.clear();
        let max = self.cfg.rules.willpower_max();
        for p in &mut self.players {
            p.kos = 0;
            p.deaths = 0;
            p.alive = false;
            p.respawn_timer = 0.0;
            p.willpower = max;
            p.seat = None;
            p.last_attacker = None;
            p.last_target = None;
            p.casts.clear();
            p.oath = crate::willpower::OathState::Idle;
            p.status.clear();
            p.size = Default::default();
            p.streak = 0;
            p.last_ko_tick = None;
            p.damage_pct = 0.0;
            p.launch = None;
            p.eliminated = false;
            p.stocks = match self.cfg.rules.scoring {
                crate::rules::Scoring::Stock { lives } => Some(lives.max(1)),
                crate::rules::Scoring::TimedKos => None,
            };
        }
        for i in 0..self.players.len() {
            self.respawn(i);
        }
    }

    fn check_match_end(&mut self) {
        if self.match_state.phase != MatchPhase::Live {
            return;
        }
        let timeout = self.match_state.phase_time >= self.cfg.rules.time_limit;
        let (over, stock_winner) = match self.cfg.rules.scoring {
            crate::rules::Scoring::TimedKos => {
                let target = self.cfg.rules.ko_target.max(1);
                (self.players.iter().any(|p| p.kos >= target), None)
            }
            crate::rules::Scoring::Stock { .. } => {
                // Last standing wins (the clock still caps it: most stocks, then lowest %).
                let standing: Vec<&Player> = self.players.iter().filter(|p| !p.eliminated).collect();
                let over = self.players.len() >= 2 && standing.len() <= 1;
                let best = standing
                    .iter()
                    .max_by(|a, b| a.stocks.cmp(&b.stocks).then(b.damage_pct.total_cmp(&a.damage_pct)).then(b.id.cmp(&a.id)))
                    .map(|p| p.id);
                (over, best)
            }
        };
        if over || timeout {
            let rows = self.scoreboard();
            let winner = stock_winner.or(rows.first().map(|r| r.player));
            self.match_state.phase = MatchPhase::Ended;
            self.match_state.phase_time = 0.0;
            self.match_state.winner = winner;
            self.events.push(GameEvent::MatchPhase { phase: MatchPhase::Ended, round: self.match_state.round, winner });
            if self.cfg.rules.regen_islands_between_rounds && self.carve_seed > 1 {
                self.prefetch_arena();
            }
        }
    }

    // ------------------------------------------------------------------------------ KO / spawn

    /// Who gets credit for a KO of player `i` right now (last attacker within 6 s).
    pub(crate) fn credit(&self, i: usize) -> Option<PlayerId> {
        let p = &self.players[i];
        let window = (self.cfg.tuning.hit.ko_credit_secs.max(0.0) / DT) as Tick;
        p.last_attacker.filter(|(a, t)| *a != p.id && self.tick.saturating_sub(*t) <= window && self.pidx(*a).is_some()).map(|(a, _)| a)
    }

    pub(crate) fn ko(&mut self, i: usize, killer: Option<PlayerId>, cause: crate::events::KoCause) {
        self.ko_zone(i, killer, cause, None);
    }

    pub(crate) fn ko_zone(&mut self, i: usize, killer: Option<PlayerId>, cause: crate::events::KoCause, zone: Option<crate::smash::BlastZone>) {
        self.powers_on_ko(i); // POWERS HOOK: power lost; a fruit respawns where you died
        let live = self.match_state.live();
        let victim = self.players[i].id;
        let (victim_corps, pos, vel, yaw, victim_streak, pct) = {
            let p = &self.players[i];
            (p.corps, p.mv.pos, p.mv.vel, p.input.yaw, p.streak, p.damage_pct)
        };
        if let Some(seat) = self.players[i].seat.take() {
            if let Some(c) = self.constructs.get_mut(seat.construct) {
                c.pilot = None;
                if seat.suit {
                    // An empty suit has nothing to do.
                    c.gone = Some(GoneReason::OwnerGone);
                }
            }
        }
        {
            let p = &mut self.players[i];
            p.alive = false;
            p.launch = None;
            p.punch = None;
            p.respawn_timer = self.cfg.rules.respawn_secs; // lobby knob (default RESPAWN_SECS)
            p.mv.release_tethers();
            p.mv.vel = Vec3::ZERO;
            p.status.clear();
            p.size = Default::default();
            if p.oath.reciting() {
                p.oath = crate::willpower::OathState::Idle;
            }
            if live {
                p.deaths = p.deaths.saturating_add(1);
            }
            p.last_attacker = None;
            p.streak = 0;
            // Stock mode: lose a life; out of lives = eliminated (no respawn this match).
            if live {
                if let Some(s) = p.stocks.as_mut() {
                    *s = s.saturating_sub(1);
                    if *s == 0 {
                        p.eliminated = true;
                    }
                }
            }
        }
        let stocks_left = self.players[i].stocks;
        if self.players[i].eliminated && live {
            self.events.push(GameEvent::Eliminated { player: victim });
        }
        let mut streak = 0;
        let mut double = false;
        let mut killer_kos = 0;
        let (shutdown_streak, shutdown_bonus) = (self.cfg.tuning.snowball.shutdown_streak, self.cfg.tuning.snowball.shutdown_bonus);
        if let Some(ki) = killer.filter(|k| *k != victim).and_then(|k| self.pidx(k)) {
            let tick = self.tick;
            let k = &mut self.players[ki];
            if live {
                k.kos = k.kos.saturating_add(1);
                // Anti-snowball: ending a streak is worth extra (tuning `snowball`).
                if victim_streak >= shutdown_streak {
                    k.kos = k.kos.saturating_add(shutdown_bonus);
                }
            }
            killer_kos = k.kos;
            k.streak = k.streak.saturating_add(1);
            streak = k.streak;
            double = k.last_ko_tick.is_some_and(|t| tick.saturating_sub(t) as f32 * DT <= 1.0);
            k.last_ko_tick = Some(tick);
        }
        self.remove_links_of_player(victim);
        self.events.push(GameEvent::Ko {
            victim,
            killer,
            cause,
            victim_corps,
            streak,
            double,
            void: cause == crate::events::KoCause::Void,
            shutdown: victim_streak >= 3,
            pos,
            vel,
            yaw,
            zone,
            poof_delay: if zone == Some(crate::smash::BlastZone::Top) { self.cfg.tuning.smash.star_ko_delay } else { 0.0 },
            killer_kos,
            pct,
            stocks_left,
        });
    }

    /// Spawn point farthest from living enemies (ties → lowest index; nobody around → spread by id).
    pub fn pick_spawn(&self, for_player: PlayerId) -> Vec3 {
        let spawns = &self.arena.spawns;
        if spawns.is_empty() {
            return self.arena.center + Vec3::Y * 2.0;
        }
        let mut best = (usize::MAX, f32::NEG_INFINITY);
        let mut any_enemy = false;
        for (k, s) in spawns.iter().enumerate() {
            let mut nearest = f32::INFINITY;
            for p in &self.players {
                if p.alive && p.id != for_player {
                    any_enemy = true;
                    nearest = nearest.min((p.mv.pos - *s).length_squared());
                }
            }
            if nearest > best.1 {
                best = (k, nearest);
            }
        }
        if !any_enemy {
            return spawns[(for_player.0 as usize) % spawns.len()];
        }
        spawns.get(best.0).copied().unwrap_or(self.arena.center)
    }

    pub(crate) fn respawn(&mut self, i: usize) {
        let id = self.players[i].id;
        let pos = self.pick_spawn(id);
        let base = self.cfg.move_params.clone();
        let wp_min = self.cfg.rules.willpower_max() * self.cfg.tuning.snowball.respawn_willpower_frac;
        let respawn_pct = self.cfg.tuning.smash.respawn_pct;
        let shield = self.cfg.rules.spawn_shield_secs; // lobby knob (default SPAWN_SHIELD_SECS)
        let p = &mut self.players[i];
        p.mv = crate::movement::PlayerState::new(pos, &base);
        p.mv.prev_buttons = p.input.buttons;
        p.alive = true;
        p.last_safe_pos = pos;
        p.damage_pct = respawn_pct;
        p.launch = None;
        p.tech_buffer = 99.0;
        p.willpower = p.willpower.max(wp_min);
        p.spawn_shield = shield;
        p.respawn_timer = 0.0;
        p.status.clear();
        p.size = Default::default();
        p.seat = None;
        // Face the arena centre.
        let to = self.arena.center - pos;
        p.input.yaw = (-to.x).atan2(-to.z);
        p.input.pitch = 0.0;
        self.events.push(GameEvent::Respawn { player: id, pos, shield });
    }

    /// Is `p` inside the arena bounds grown by `margin` (and above the void)?
    pub fn in_play(&self, p: Vec3, margin: f32) -> bool {
        let b = self.arena.bounds;
        p.is_finite()
            && p.y >= self.arena.void_y - margin
            && p.x >= b.min.x - margin
            && p.x <= b.max.x + margin
            && p.z >= b.min.z - margin
            && p.z <= b.max.z + margin
            && p.y <= b.max.y + margin
    }

    /// Hulls of every living player (for tools / bots).
    pub fn hull_of(&self, id: PlayerId) -> Option<Aabb> {
        self.player(id).filter(|p| p.alive).map(|p| p.hull(&self.cfg.move_params))
    }
}

/// Clamp untrusted input fields (NaN look angles, out-of-range axes).
pub fn sanitize_input(i: &PlayerInput) -> PlayerInput {
    let mut o = *i;
    o.yaw = if i.yaw.is_finite() { i.yaw.rem_euclid(std::f32::consts::TAU) } else { 0.0 };
    o.pitch = if i.pitch.is_finite() { i.pitch.clamp(-1.56, 1.56) } else { 0.0 };
    o.move_x = i.move_x.clamp(-1, 1);
    o.move_z = i.move_z.clamp(-1, 1);
    o.reel_notches = i.reel_notches.clamp(-8, 8);
    o
}

/// Input as seen while micro-stunned: movement keys, jump, crouch, boost and attacks ignored;
/// aim and tether holds stay live (never fully remove control).
fn stunned_input(i: &PlayerInput) -> PlayerInput {
    let keep = buttons::TETHER_L | buttons::TETHER_R | buttons::REEL | buttons::TYPING;
    PlayerInput { move_x: 0, move_z: 0, buttons: i.buttons & keep, ..*i }
}

/// What a dropped player gets back on reconnect (see [`World::remove_player_for_reconnect`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DepartedPlayer {
    pub id: PlayerId,
    pub name: String,
    pub corps: Corps,
    pub kos: u16,
    pub deaths: u16,
    pub stocks: Option<u8>,
    pub eliminated: bool,
    /// `MatchState::round` when they dropped.
    pub round: u32,
}
