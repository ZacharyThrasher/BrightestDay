//! Sim-side player: movement state plus everything combat, economy and rules track per player.

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::{ConstructId, PlayerId, Tick, Vec3};
use serde::{Deserialize, Serialize};

use crate::movement::{MoveEvents, MoveParams, PlayerState};
use crate::space::SizeState;
use crate::status::StatusSet;
use crate::willpower::{CastHistory, OathState};

/// A player seated in a piloted construct (COMBAT.md §5d / §3d).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seat {
    pub construct: ConstructId,
    /// Cockpit / saddle part (the component system decides where it is).
    pub part: u8,
    /// Power suit: the pilot keeps normal movement (×0.85) and the suit absorbs damage first.
    /// Otherwise (cockpit): movement is suspended, the hull rides the seat, inputs drive the
    /// construct and E (TETHER_R) ejects.
    pub suit: bool,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub corps: Corps,
    /// Movement state (the part clients predict).
    pub mv: PlayerState,
    /// Input applied this tick (repeated when none arrives).
    pub input: PlayerInput,
    /// Buttons of the previous tick (action edges; movement keeps its own copy in `mv`).
    pub prev_buttons: u16,
    pub pressed: u16,
    pub released: u16,
    /// Sequence number of the last input applied (the per-recipient snapshot `ack`).
    pub last_input_seq: u32,
    /// Interpolation tick the client was looking at (lag compensation), if networked.
    pub view_tick: Option<f64>,
    /// Smash damage % (COMBAT.md §1a): 0 at spawn, uncapped; knockback grows with it.
    pub damage_pct: f32,
    /// Last position standing on ground (Smash KOs happen off-map: "where they fell" effects —
    /// fruits, Last Words — use this).
    pub last_safe_pos: Vec3,
    /// Flying from a launch (DI window, trail, punch-through).
    pub launch: Option<crate::smash::LaunchState>,
    /// Plowing through terrain (punch-through): entry point, material, carved tunnel end.
    pub punch: Option<crate::smash::PunchState>,
    /// Seconds since Jump was last pressed while tumbling (tech buffer).
    pub tech_buffer: f32,
    /// Stock mode: lives left (`None` in timed matches)…
    pub stocks: Option<u8>,
    /// …and out of lives (spectating until the next match).
    pub eliminated: bool,
    pub willpower: f32,
    pub alive: bool,
    pub respawn_timer: f32,
    pub spawn_shield: f32,
    pub status: StatusSet,
    pub size: SizeState,
    pub kos: u16,
    pub deaths: u16,
    /// Last enemy that hit us (for void / splat / DoT credit) and when.
    pub last_attacker: Option<(PlayerId, Tick)>,
    /// Enemy we last hit or aimed a cast at (autonomous constructs hunt it).
    pub last_target: Option<PlayerId>,
    pub blast_cd: f32,
    pub shove_cd: f32,
    /// Passive regen is paused while > 0.
    pub regen_pause: f32,
    pub casts: CastHistory,
    pub oath: OathState,
    pub seat: Option<Seat>,
    /// Movement events of the last tick (sounds / camera; also used for wall splats).
    pub move_events: MoveEvents,
    pub portal_cd: f32,
    pub eject_cd: f32,
    /// A tumbling wall impact from this tick's movement, waiting to be resolved as a splat.
    pub pending_splat: Option<crate::movement::WallHit>,
    /// KOs since this player's last death.
    pub streak: u16,
    /// Tick of this player's last KO (double-KO detection).
    pub last_ko_tick: Option<Tick>,
    /// Portal gun: which end the next shot places (0 = A, 1 = B)…
    pub portal_slot: u8,
    /// …and the first ring while its partner hasn't been shot yet.
    pub portal_pending: Option<crate::space::PortalEnd>,
    /// POWERS HOOK: held player power (devil fruit / stand), COMBAT.md §5h.
    pub power: crate::powers::PlayerPower,
}

impl Player {
    pub fn new(id: PlayerId, name: String, corps: Corps, pos: Vec3, params: &MoveParams) -> Self {
        Self {
            id,
            name,
            corps,
            mv: PlayerState::new(pos, params),
            input: PlayerInput::default(),
            prev_buttons: 0,
            pressed: 0,
            released: 0,
            last_input_seq: 0,
            view_tick: None,
            damage_pct: 0.0,
            last_safe_pos: pos,
            launch: None,
            punch: None,
            tech_buffer: 99.0,
            stocks: None,
            eliminated: false,
            willpower: 100.0,
            alive: false,
            respawn_timer: 0.0,
            spawn_shield: 0.0,
            status: StatusSet::default(),
            size: SizeState::default(),
            kos: 0,
            deaths: 0,
            last_attacker: None,
            last_target: None,
            blast_cd: 0.0,
            shove_cd: 0.0,
            regen_pause: 0.0,
            casts: CastHistory::default(),
            oath: OathState::Idle,
            seat: None,
            move_events: MoveEvents::default(),
            portal_cd: 0.0,
            eject_cd: 0.0,
            pending_splat: None,
            streak: 0,
            last_ko_tick: None,
            portal_slot: 0,
            portal_pending: None,
            power: Default::default(),
        }
    }

    /// Movement params for this player's current body size.
    pub fn params(&self, base: &MoveParams) -> MoveParams {
        base.scaled(self.size.scale)
    }

    /// Current hull (seated players' hulls ride the seat).
    pub fn hull(&self, base: &MoveParams) -> bd_core::voxel::Aabb {
        if self.size.scale == 1.0 { self.mv.hull(base) } else { self.mv.hull(&self.params(base)) }
    }

    pub fn eye(&self, base: &MoveParams) -> Vec3 {
        if self.size.scale == 1.0 { self.mv.eye(base) } else { self.mv.eye(&self.params(base)) }
    }

    pub fn look(&self) -> Vec3 {
        self.input.look_dir()
    }

    /// Stunned, reciting or seated in a cockpit: can't attack.
    pub fn busy(&self) -> bool {
        !self.alive || self.status.stunned() || self.oath.reciting() || self.seat.is_some_and(|s| !s.suit)
    }

    pub fn shielded(&self) -> bool {
        self.spawn_shield > 0.0
    }
}
