//! The bridge between Bevy and `bd_sim::World` — **the contract SIM, UI and NET code against**.
//!
//! One abstraction, used identically in practice, host and client modes:
//!
//! | piece | what | who writes | who reads |
//! |---|---|---|---|
//! | [`ActiveWorld`] (resource) | the authoritative (practice / host) or replica (client) `bd_sim::World`, the local [`PlayerId`], the [`SimRole`], bots, remote inputs | practice (SIM) or NET inserts it on `OnEnter(AppState::InGame)` | everyone (read-only outside the owner) |
//! | [`SimCommand`] (message) | player commands: cast, power move, stand rush, oath, dismiss, transform, eject, party pick… | UI (`MessageWriter<SimCommand>`), NET host (remote players' commands) | the bridge applies them in `Offline`/`Host`; in `Client` NET reads them and forwards them to the host |
//! | [`GameEvents`] (message) | every `bd_sim::GameEvent`, re-broadcast once per event after each tick | the bridge (from `World::tick`) or NET (client: events received from the host, via [`ActiveWorld::push_event`]) | presentation (SIM `present/**`), UI (HUD, scoreboard, console), NET host (forwarding) |
//! | [`ViewKeys`] (resource) | the two snapshots render time lies between + `alpha` | the bridge each tick (`Offline`/`Host`); NET each frame (`Client`, from its SnapshotBuffer) | the bridge → [`WorldViews`] |
//! | [`WorldViews`] (resource) | per-frame **interpolated** players / constructs in Bevy types | the bridge ([`BridgeSet::Views`]) | presentation (SIM renders them), UI (name tags, aim assist, scoreboard) |
//!
//! ## UI: pushing commands
//! ```ignore
//! fn cast_on_enter(mut cmds: MessageWriter<SimCommand>, /* … */) {
//!     let aim = bd_sim::Aim::new(eye, dir);                       // bd_sim (glam 0.30) types
//!     cmds.write(SimCommand::local(Cmd::Cast { recipe: Box::new(recipe), shape: None, alt_shape: None, aim, incantation: false }));
//! }
//! ```
//! `SimCommand::local(..)` targets the local player (whatever id it has). Results come back as
//! `GameEvents` (`ConstructSpawned`, `CastFailed`, `Power(..)`…) — never as return values, so the
//! same UI code works when the command has to travel to a host first. Start forming VFX/sound on
//! the key press (latency hiding); the bridge applies casts before the next tick of the same frame.
//! Graft = `Cmd::Cast` with `aim.modifier = true` or `aim.target_hint = Some(TargetHint::Construct(id))`.
//! Powers: call `active.world.parse_power_phrase(local, text)` (read-only) to decide between
//! `Cmd::PowerMove`, `Cmd::Cast { incantation: true, .. }` and a plain cast.
//!
//! ## NET: host / client modes
//! * **Host**: insert `ActiveWorld::new(world, Some(host_id), SimRole::Host)` (instead of practice).
//!   Each tick in [`SimSet::Input`] write the input to apply for every remote player into
//!   [`ActiveWorld::remote_inputs`] (the bridge consumes the list each tick; players with no entry
//!   repeat their last input). Push remote players' commands as `SimCommand::remote(id, cmd)`.
//!   In [`SimSet::Post`] read [`ActiveWorld::tick_events`] (this tick's events) and
//!   `world.write_snapshot(Some(id), ..)` to send. Call `world.set_view_tick` yourself.
//! * **Client**: insert `ActiveWorld::new(replica, Some(my_id), SimRole::Client)` (a replica
//!   world built with `World::with_arena`). The bridge then does **not** tick the world nor apply
//!   commands: NET reads `MessageReader<SimCommand>` and sends them; applies `apply_snapshot` /
//!   `apply_event` to `active.world`; calls [`ActiveWorld::push_event`] for every received event
//!   (so presentation/UI see them as `GameEvents`); predicts the local player and writes its
//!   predicted `prev`/`state` into the `PlayerSim` component of the `LocalPlayer` entity (camera,
//!   audio, movement juice read it); and each frame sets `ViewKeys { prev, curr, alpha }` from its
//!   interpolation buffer (two bracketing snapshots) with `ViewKeys::external = true`.
//!
//! ## Render-side mirror of the terrain
//! `ArenaWorld` (the rendered voxels) is a separate copy. Presentation applies every
//! `GameEvent::Carve` / `Extracted` / `Power(Restored)` / `ArenaReset` to it, in every mode — the
//! sim world applies its own copy (authoritative) or NET's `apply_event` does (replica).
//!
//! ## Ordering
//! `FrameSet::Input` → [`BridgeSet::Commands`] → `FrameSet::Sim` (ticks: `SimSet::Input` →
//! `SimSet::Simulate` (bridge step) → `SimSet::Post`) → [`BridgeSet::Flush`] (GameEvents) →
//! [`BridgeSet::Views`] → `FrameSet::Present`.

use crate::input::LocalInputState;
use crate::player::{FrameMoveEvents, FramePositions, LocalPlayer, PlayerSim};
use crate::state::AppState;
use crate::tick::{FrameSet, SimSet, SimTick, TickClock};
use crate::util::v3;
use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::mechanics::Recipe;
use bd_core::shape::ShapeSpec;
use bd_core::{ConstructId, PlayerId, Tick, DT};
use bd_sim::bots::BotBrain;
use bd_sim::powers::PowerSnap;
use bd_sim::replicate::{cflags, pflags, ConstructSnap, PlayerSnap, WorldSnapshot};
use bd_sim::{Aim, GameEvent, World};
use bevy::prelude::*;

// ------------------------------------------------------------------------------------ roles

/// How the local process relates to the world in [`ActiveWorld`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimRole {
    /// Practice: local authoritative world, no network.
    Offline,
    /// Listen server: local authoritative world; NET feeds remote inputs / commands.
    Host,
    /// Replica of a remote host's world (NET drives it; the bridge never ticks it).
    Client,
}

impl SimRole {
    /// The bridge ticks the world and applies commands.
    pub fn authoritative(self) -> bool {
        matches!(self, SimRole::Offline | SimRole::Host)
    }
}

/// The world everything renders and plays. Absent outside `AppState::InGame` (and on the
/// tutorial map, which keeps the standalone movement path).
#[derive(Resource)]
pub struct ActiveWorld {
    pub world: World,
    /// The locally controlled player (None = spectator, e.g. the soak viewer).
    pub local: Option<PlayerId>,
    pub role: SimRole,
    /// Bots driven by the authoritative side every tick (practice, host fill-ins).
    pub bots: Vec<BotBrain>,
    /// Host: inputs for remote players to apply on the next tick (consumed each tick).
    pub remote_inputs: Vec<(PlayerId, PlayerInput)>,
    /// Events of the most recent tick (host: forward these in `SimSet::Post`).
    pub tick_events: Vec<GameEvent>,
    /// Freeze the authoritative world (debug / soak failure). Commands still queue.
    pub paused: bool,
    /// Events waiting to be re-broadcast as [`GameEvents`] this frame.
    outbox: Vec<GameEvent>,
    /// Scratch for the tick's input list (no per-tick allocation).
    inputs: Vec<(PlayerId, PlayerInput)>,
}

impl ActiveWorld {
    pub fn new(world: World, local: Option<PlayerId>, role: SimRole) -> Self {
        Self {
            world,
            local,
            role,
            bots: Vec::new(),
            remote_inputs: Vec::with_capacity(bd_core::MAX_PLAYERS),
            tick_events: Vec::with_capacity(256),
            paused: false,
            outbox: Vec::with_capacity(512),
            inputs: Vec::with_capacity(bd_core::MAX_PLAYERS),
        }
    }

    /// Queue an event for re-broadcast as [`GameEvents`] (client mode: every event from the host).
    pub fn push_event(&mut self, e: GameEvent) {
        self.outbox.push(e);
    }

    pub fn local_player(&self) -> Option<&bd_sim::Player> {
        self.local.and_then(|id| self.world.player(id))
    }

    pub fn corps_of(&self, id: PlayerId) -> Corps {
        self.world.player(id).map_or(Corps::from_index(id.0), |p| p.corps)
    }

    /// One authoritative tick with the given local input: bots think, inputs gathered, world
    /// stepped, events queued. Returns the tick's events.
    pub fn step(&mut self, local_input: Option<PlayerInput>) -> &[GameEvent] {
        self.inputs.clear();
        if let (Some(id), Some(i)) = (self.local, local_input) {
            self.inputs.push((id, i));
        }
        self.inputs.append(&mut self.remote_inputs);
        bd_sim::bots::drive(&mut self.world, &mut self.bots, &mut self.inputs);
        let evs = self.world.tick(&self.inputs);
        self.tick_events.clear();
        self.tick_events.extend_from_slice(evs);
        self.outbox.extend_from_slice(evs);
        &self.tick_events
    }

    /// Apply one command for `player` to the authoritative world.
    pub fn apply_command(&mut self, player: PlayerId, cmd: Cmd) {
        let w = &mut self.world;
        match cmd {
            Cmd::Cast { recipe, shape, alt_shape, aim, incantation } => {
                let _ = if incantation { w.cast_incantation(player, *recipe, shape, aim) } else { w.cast_shaped(player, *recipe, shape, alt_shape, aim) };
            }
            Cmd::PowerMove(slot) => {
                let _ = w.power_move(player, slot);
            }
            Cmd::StandRush(n) => {
                w.stand_rush(player, n);
            }
            Cmd::OathBegin => {
                w.begin_oath(player);
            }
            Cmd::OathProgress { correct_chars, accuracy } => w.oath_progress(player, correct_chars, accuracy),
            Cmd::OathResult { accuracy, seconds } => {
                let _ = w.oath_result(player, accuracy, seconds);
            }
            Cmd::OathCancel => w.cancel_oath(player),
            Cmd::Dismiss => {
                w.dismiss(player);
            }
            Cmd::DismissOne(id) => {
                w.dismiss_one(player, id);
            }
            Cmd::ToggleTransform => {
                w.toggle_transform(player);
            }
            Cmd::Eject => {
                w.eject(player);
            }
            Cmd::PartyPick(i) => {
                let _ = w.party_choose(player, i);
            }
            Cmd::GrantPower(key) => {
                w.grant_power(player, &key);
            }
        }
    }
}

// ------------------------------------------------------------------------------------ commands

/// A player command (between ticks). Mirrors the `World` command methods.
#[derive(Clone, Debug)]
pub enum Cmd {
    /// Cast a resolved phrase. `incantation` = the phrase followed a power prefix
    /// (`parse_power_phrase` → `Incantation { rest }`): uses `World::cast_incantation`.
    /// `alt_shape` = a curated two-mode Transformer's hand-authored robot mode (lexicon).
    Cast { recipe: Box<Recipe>, shape: Option<ShapeSpec>, alt_shape: Option<ShapeSpec>, aim: Aim, incantation: bool },
    /// G tap (0) / G hold (1).
    PowerMove(u8),
    /// Rush tokens typed while the console is open (ORA ×n).
    StandRush(u16),
    OathBegin,
    OathProgress { correct_chars: u16, accuracy: f32 },
    OathResult { accuracy: f32, seconds: f32 },
    OathCancel,
    /// X: dismiss every own construct.
    Dismiss,
    DismissOne(ConstructId),
    /// Transformer robot ⇄ vehicle.
    ToggleTransform,
    /// Leave a cockpit (E while seated).
    Eject,
    /// Party pick screen card index.
    PartyPick(u8),
    /// Tools / lobby "random powers": give a power by key.
    GrantPower(String),
}

/// Who issues a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmdSource {
    /// The local player (whatever id it has).
    Local,
    /// A specific player (NET host: a remote client's command).
    Player(PlayerId),
}

/// The command queue (a Bevy message). UI writes `SimCommand::local(..)`.
#[derive(Message, Clone, Debug)]
pub struct SimCommand {
    pub source: CmdSource,
    pub cmd: Cmd,
}

impl SimCommand {
    pub fn local(cmd: Cmd) -> Self {
        Self { source: CmdSource::Local, cmd }
    }
    pub fn remote(player: PlayerId, cmd: Cmd) -> Self {
        Self { source: CmdSource::Player(player), cmd }
    }
}

// ------------------------------------------------------------------------------------ events

/// One re-broadcast `bd_sim::GameEvent` (one message per event, in sim order).
#[derive(Message, Clone, Debug)]
pub struct GameEvents {
    /// World tick when the event was flushed (client: the replica's tick).
    pub tick: Tick,
    pub event: GameEvent,
}

// ------------------------------------------------------------------------------------ views

/// The two snapshots render time lies between.
#[derive(Resource, Default)]
pub struct ViewKeys {
    pub prev: WorldSnapshot,
    pub curr: WorldSnapshot,
    /// Render time between `prev` (0) and `curr` (1).
    pub alpha: f32,
    /// NET (client) fills the keys itself; the bridge only interpolates them.
    pub external: bool,
    /// Keys are valid (at least one written).
    pub valid: bool,
}

/// An interpolated player for this frame (Bevy types).
#[derive(Clone, Debug)]
pub struct PlayerView {
    pub id: PlayerId,
    pub corps: Corps,
    pub is_local: bool,
    /// Feet position.
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// `replicate::pflags` bits.
    pub flags: u16,
    /// Smash model: damage % (0 … 999+).
    pub damage_pct: f32,
    /// Launch trail intensity 0..1 (0 = not flying from a launch).
    pub launch: f32,
    /// Stock mode lives (255 = timed match).
    pub stocks: u8,
    pub willpower: f32,
    pub status: u8,
    pub size: f32,
    /// Hook heads / anchors (flying or attached).
    pub hooks: [Option<Vec3>; 2],
    pub seat: Option<(ConstructId, u8)>,
    pub power: PowerSnap,
}

impl PlayerView {
    pub fn has(&self, f: u16) -> bool {
        self.flags & f != 0
    }
    pub fn alive(&self) -> bool {
        self.has(pflags::ALIVE)
    }
}

/// An interpolated construct for this frame.
#[derive(Clone, Debug)]
pub struct ConstructView {
    pub id: ConstructId,
    pub owner: PlayerId,
    /// Meaningless when `attached()` (held / worn: attach to the owner).
    pub pos: Vec3,
    pub rot: Quat,
    /// `bd_sim::constructs::AnimState as u8`.
    pub anim: u8,
    /// Seconds since the anim state began (interpolated).
    pub anim_time: f32,
    pub integrity: f32,
    /// `replicate::cflags` bits.
    pub flags: u8,
    pub size: f32,
    pub weapon: u8,
    /// Linear speed (m/s), for locomotion clips.
    pub speed: f32,
}

impl ConstructView {
    pub fn attached(&self) -> bool {
        self.flags & cflags::ATTACHED != 0
    }
}

/// Interpolated render state, rebuilt every frame in [`BridgeSet::Views`]. Sorted by id.
#[derive(Resource, Default)]
pub struct WorldViews {
    pub players: Vec<PlayerView>,
    pub constructs: Vec<ConstructView>,
    pub tick: Tick,
}

impl WorldViews {
    pub fn player(&self, id: PlayerId) -> Option<&PlayerView> {
        self.players.binary_search_by_key(&id, |p| p.id).ok().map(|i| &self.players[i])
    }
    pub fn construct(&self, id: ConstructId) -> Option<&ConstructView> {
        self.constructs.binary_search_by_key(&id, |c| c.id).ok().map(|i| &self.constructs[i])
    }
}

/// Authoritative tick cost (ms), for overlays / benchmarks.
#[derive(Resource, Default, Debug)]
pub struct SimTiming {
    pub last_ms: f32,
    /// Every tick's cost since startup (for the bench report; capped).
    pub samples: Vec<f32>,
}

impl SimTiming {
    pub fn record(&mut self, ms: f32) {
        self.last_ms = ms;
        if self.samples.len() < 200_000 {
            self.samples.push(ms);
        }
    }
    /// (avg, p99, max) over the recorded ticks.
    pub fn stats(&self) -> (f32, f32, f32) {
        if self.samples.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let mut v = self.samples.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        let avg = v.iter().sum::<f32>() / v.len() as f32;
        (avg, v[((v.len() as f32 * 0.99) as usize).min(v.len() - 1)], v[v.len() - 1])
    }
}

// ------------------------------------------------------------------------------------ plugin

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum BridgeSet {
    /// Apply queued `SimCommand`s (authoritative roles), before the frame's ticks.
    Commands,
    /// Re-broadcast queued events as `GameEvents`.
    Flush,
    /// Rebuild `WorldViews`.
    Views,
}

pub struct SimBridgePlugin;

impl Plugin for SimBridgePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SimCommand>()
            .add_message::<GameEvents>()
            .init_resource::<ViewKeys>()
            .init_resource::<WorldViews>()
            .init_resource::<SimTiming>()
            .configure_sets(Update, BridgeSet::Commands.after(FrameSet::Input).before(FrameSet::Sim))
            .configure_sets(Update, (BridgeSet::Flush, BridgeSet::Views).chain().after(FrameSet::Sim).before(FrameSet::Present))
            .add_systems(Update, apply_commands.in_set(BridgeSet::Commands))
            .add_systems(SimTick, step_world.in_set(SimSet::Simulate))
            .add_systems(Update, flush_events.in_set(BridgeSet::Flush))
            .add_systems(Update, build_views.in_set(BridgeSet::Views))
            .add_systems(OnExit(AppState::InGame), teardown);
    }
}

fn teardown(mut commands: Commands, mut keys: ResMut<ViewKeys>, mut views: ResMut<WorldViews>) {
    commands.remove_resource::<ActiveWorld>();
    keys.valid = false;
    keys.external = false;
    views.players.clear();
    views.constructs.clear();
}

fn apply_commands(mut reader: MessageReader<SimCommand>, active: Option<ResMut<ActiveWorld>>) {
    let Some(mut a) = active else {
        reader.clear();
        return;
    };
    if !a.role.authoritative() {
        // Client: NET reads these with its own reader and forwards them.
        reader.clear();
        return;
    }
    for c in reader.read() {
        let who = match c.source {
            CmdSource::Local => match a.local {
                Some(id) => id,
                None => continue,
            },
            CmdSource::Player(id) => id,
        };
        a.apply_command(who, c.cmd.clone());
    }
}

/// `SimSet::Simulate`: one authoritative tick; mirrors the local player's movement into
/// `PlayerSim` (camera / audio / juice keep working unchanged) and writes the view keys.
fn step_world(
    active: Option<ResMut<ActiveWorld>>,
    input: Res<LocalInputState>,
    mut keys: ResMut<ViewKeys>,
    mut move_events: ResMut<FrameMoveEvents>,
    mut positions: ResMut<FramePositions>,
    mut local_q: Query<&mut PlayerSim, With<LocalPlayer>>,
    mut timing: ResMut<SimTiming>,
) {
    let Some(mut a) = active else { return };
    if !a.role.authoritative() || a.paused {
        return;
    }
    let before = a.local_player().map(|p| p.mv);
    let local_input = Some(input.current);
    let t0 = std::time::Instant::now();
    a.step(local_input);
    timing.record(t0.elapsed().as_secs_f32() * 1000.0);
    if let (Some(prev), Some(p)) = (before, a.local_player()) {
        if let Ok(mut sim) = local_q.single_mut() {
            sim.prev = prev;
            sim.state = p.mv;
            // A respawn / teleport: don't interpolate across it.
            if prev.pos.distance_squared(p.mv.pos) > 25.0 * 25.0 {
                sim.prev = p.mv;
            }
        }
        move_events.0.push(p.move_events);
        positions.0.push((v3(prev.pos), v3(p.mv.pos)));
    }
    if !keys.external {
        let keys = &mut *keys;
        std::mem::swap(&mut keys.prev, &mut keys.curr);
        a.world.write_snapshot(a.local, &mut keys.curr);
        if !keys.valid {
            a.world.write_snapshot(a.local, &mut keys.prev);
            keys.valid = true;
        }
    }
}

fn flush_events(active: Option<ResMut<ActiveWorld>>, mut out: MessageWriter<GameEvents>) {
    let Some(mut a) = active else { return };
    if a.outbox.is_empty() {
        return;
    }
    let tick = a.world.tick;
    let a = &mut *a;
    out.write_batch(a.outbox.drain(..).map(|event| GameEvents { tick, event }));
}

fn lerp_player(a: &PlayerSnap, b: &PlayerSnap, t: f32) -> (Vec3, Vec3, f32, f32) {
    let (pa, pb) = (v3(a.pos.get()), v3(b.pos.get()));
    // Teleports / respawns: snap.
    let pos = if pa.distance_squared(pb) > 16.0 { pb } else { pa.lerp(pb, t) };
    let vel = v3(a.vel.get()).lerp(v3(b.vel.get()), t);
    let (ya, pia) = a.look.get();
    let (yb, pib) = b.look.get();
    let dy = (yb - ya + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    (pos, vel, ya + dy * t, pia + (pib - pia) * t)
}

fn build_views(active: Option<Res<ActiveWorld>>, clock: Res<TickClock>, mut keys: ResMut<ViewKeys>, mut views: ResMut<WorldViews>) {
    let views = &mut *views;
    views.players.clear();
    views.constructs.clear();
    let Some(a) = active else { return };
    if !keys.valid {
        return;
    }
    if !keys.external {
        keys.alpha = clock.alpha;
    }
    let t = keys.alpha.clamp(0.0, 1.0);
    let (prev, curr) = (&keys.prev, &keys.curr);
    views.tick = curr.tick;
    for b in &curr.players {
        let id = PlayerId(b.id);
        let a_snap = prev.players.binary_search_by_key(&b.id, |p| p.id).ok().map_or(b, |i| &prev.players[i]);
        let (pos, vel, yaw, pitch) = lerp_player(a_snap, b, t);
        let hook = |i: usize| match (a_snap.hooks[i], b.hooks[i]) {
            (Some(x), Some(y)) => Some(v3(x.get()).lerp(v3(y.get()), t)),
            (_, y) => y.map(|y| v3(y.get())),
        };
        views.players.push(PlayerView {
            id,
            corps: a.corps_of(id),
            is_local: a.local == Some(id),
            pos,
            vel,
            yaw,
            pitch,
            flags: b.flags(),
            damage_pct: b.pct as f32 / 10.0,
            launch: b.launch as f32 / 255.0,
            stocks: b.stocks,
            willpower: b.willpower as f32,
            status: b.status,
            size: b.size_scale(),
            hooks: [hook(0), hook(1)],
            seat: b.seat.map(|(c, p)| (ConstructId(c), p)),
            power: b.power,
        });
    }
    let tick16 = curr.tick as u16;
    for b in &curr.constructs {
        let a_snap: &ConstructSnap = prev.constructs.binary_search_by_key(&b.id, |c| c.id).ok().map_or(b, |i| &prev.constructs[i]);
        let (pa, pb) = (v3(a_snap.pos.get()), v3(b.pos.get()));
        let pos = if pa.distance_squared(pb) > 100.0 { pb } else { pa.lerp(pb, t) };
        let rot = Quat::from_array(a_snap.rot.get().to_array()).slerp(Quat::from_array(b.rot.get().to_array()), t);
        let dt_ticks = curr.tick.saturating_sub(prev.tick).max(1) as f32;
        let speed = pa.distance(pb) / (dt_ticks * DT);
        let since = tick16.wrapping_sub(b.anim_since) as f32;
        views.constructs.push(ConstructView {
            id: ConstructId(b.id),
            owner: PlayerId(b.owner),
            pos,
            rot,
            anim: b.anim,
            anim_time: (since + t) * DT,
            integrity: b.integrity as f32 / 255.0,
            flags: b.flags,
            size: b.size as f32 / 50.0,
            weapon: b.weapon,
            speed: if speed.is_finite() { speed } else { 0.0 },
        });
    }
}
