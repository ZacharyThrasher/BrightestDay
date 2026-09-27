//! Per-player live state + shared targeting helpers for the timed toolbox ops (docs/toolbox/OPS.md):
//! `Drill`, `MarkRecall`, `WarpTo`, `CopyLast`, `GazeStun`, `Phase`, `Banish`, `Rush`, `Contagion`,
//! `Wave`. Everything here is plain data on [`crate::powers::PlayerPower::live`] (fixed size, no
//! allocation) stepped once per tick by [`World::step_live_ops`]; a player who never used one of
//! these ops pays one `idle()` check per tick and nothing changes (FFA golden hash untouched).
//!
//! Presentation: every op moment is one `PowerEvent::Op(OpEvent::Toolbox { .. })` (`fx` names the op
//! beat; `from` / `to` / `secs` / `size` are enough for a client to draw it — a wave's flight is
//! fully determined by its start event).

use bd_core::mechanics::Element;
use bd_core::voxel::{VoxelQuery, world_to_voxel};
use bd_core::{PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use super::OpEvent;
use crate::combat::{HitInfo, HitKind};
use crate::movement::{CollisionWorld, raycast_world};
use crate::status::StatusKind;
use crate::world::World;

/// A beat of a toolbox op, for presentation (sound, VFX, cut-ins). Never read by the sim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolboxFx {
    /// A hand-held orb / lightning lunge starts (`from` = hand, `to` = lunge direction point).
    DrillStart,
    /// The orb met someone: they are ground against the palm (`to` = contact point, `secs` = grind).
    DrillLatch,
    /// The drill's release: the victim is blasted away spinning (`to` = contact point).
    DrillRelease,
    /// A whiffed / terrain-ending drill (`to` = where it ended).
    DrillEnd,
    /// A marker is thrown (`from` = hand, `to` = where it stuck; `size` 1 = on a player).
    Mark,
    /// Teleport to the marker (`from` → `to`).
    Recall,
    /// Teleport to a looked-at enemy (`from` → `to`).
    Warp,
    /// A seen move is copied (`from` = your eyes).
    Copy,
    /// A gaze landed (`from` = your eyes, `to` = the victim, `secs` = the status time).
    Gaze,
    /// A gaze / flash went off (radial: `size` = range).
    Flash,
    /// Intangible for `secs`.
    PhaseOn,
    PhaseOff,
    /// The aimed thing is warped away (`from` = where it was, `to` = where it went; `size` 1 = a
    /// construct swallowed).
    Banish,
    /// A rush flurry starts (`secs` = its length).
    Rush,
    /// A brand took hold (`to` = the victim); `size` 1 = it spread from someone else.
    Brand,
    /// A brand burned out (`to` = the victim).
    BrandEnd,
    /// A travelling slash leaves (`from` = start, `to` = the far end it will reach, `secs` = flight
    /// time, `size` = width).
    Wave,
}

/// Drill state (see `ops/drill.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrillState {
    /// Lunge time left (before contact).
    pub t: f32,
    pub dir: Vec3,
    pub speed: f32,
    /// Configured grind (s); 0 = one strike on contact.
    pub grind: f32,
    /// Grind time left once latched.
    pub grind_t: f32,
    pub target: Option<PlayerId>,
    /// Next grind tick in.
    pub tick_t: f32,
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub element: Element,
}

/// Where a mark is (see `ops/mark_recall.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkAt {
    /// A spot on terrain (already backed off the surface).
    Point(Vec3),
    /// Stuck on a player: follows them.
    Player(PlayerId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkState {
    pub at: MarkAt,
    /// Seconds left.
    pub t: f32,
}

/// The last power move an enemy fired near this player (see `ops/copy_last.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub power: u16,
    pub tier: u8,
    pub slot: u8,
    /// Seconds since.
    pub age: f32,
    /// How far away it was fired (m).
    pub dist: f32,
}

/// A rush flurry in progress (see `ops/rush.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RushState {
    pub left: u8,
    pub every: f32,
    pub next: f32,
    pub damage: f32,
    pub knockback: f32,
    pub range: f32,
    /// A temporary stand summoned for this rush (none when the player has a companion).
    pub stand: Option<bd_core::ConstructId>,
    pub n: u8,
}

/// A contagious brand ON this player (see `ops/contagion.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brand {
    pub status: StatusKind,
    pub t: f32,
    pub spread: f32,
    pub source: PlayerId,
    pub tick_t: f32,
}

/// A travelling slash (see `ops/wave.rs`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveLive {
    pub pos: Vec3,
    pub dir: Vec3,
    /// Metres of flight left.
    pub left: f32,
    pub speed: f32,
    pub width: f32,
    pub carve: f32,
    /// Metres since the last carve.
    pub since_carve: f32,
    pub damage: f32,
    pub knockback: f32,
    pub up: f32,
    pub element: Element,
    pub thin: bool,
    /// Player indices already struck (bit k = players[k]).
    pub hit: u32,
    /// The construct it cut last (each construct is cut once).
    pub last_c: Option<bd_core::ConstructId>,
}

/// Most live waves per player (a new one replaces the oldest).
pub const MAX_WAVES: usize = 3;

/// Every live toolbox op of one player. `Default` = nothing live.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveOps {
    pub drill: Option<DrillState>,
    pub mark: Option<MarkState>,
    pub seen: Option<Seen>,
    /// Intangible for this long.
    pub phase: f32,
    pub rush: Option<RushState>,
    pub brand: Option<Brand>,
    pub waves: [Option<WaveLive>; MAX_WAVES],
}

impl LiveOps {
    /// Nothing to step.
    pub fn idle(&self) -> bool {
        self.drill.is_none() && self.mark.is_none() && self.seen.is_none() && self.phase <= 0.0 && self.rush.is_none() && self.brand.is_none() && self.waves.iter().all(|w| w.is_none())
    }
}

impl World {
    /// One toolbox op beat for the clients.
    pub(crate) fn toolbox_fx(&mut self, player: PlayerId, fx: ToolboxFx, from: Vec3, to: Vec3, secs: f32, size: f32) {
        self.op_event(OpEvent::Toolbox { player, fx, from, to, secs, size });
    }

    /// Eye + look of player `i` for a power move (the host-validated move aim when one is set).
    pub(crate) fn toolbox_eye_look(&self, i: usize) -> (Vec3, Vec3) {
        if let Some((k, eye, dir)) = self.powers.move_aim
            && k == i
        {
            return (eye, dir);
        }
        let p = &self.players[i];
        (p.eye(&self.cfg.move_params), p.look())
    }

    /// The living enemy under the crosshair within `range`, else the one closest to the crosshair
    /// inside `cone`° (aim forgiveness). `los`: terrain must not block the eye → them.
    pub(crate) fn toolbox_target(&self, i: usize, range: f32, cone: f32, los: bool) -> Option<usize> {
        let (eye, look) = self.toolbox_eye_look(i);
        let base = &self.cfg.move_params;
        let range = range.clamp(0.5, 80.0);
        let wall = if los { bd_core::voxel::raycast(&self.voxels, eye, look, range).map_or(f32::INFINITY, |h| h.t) } else { f32::INFINITY };
        let end = eye + look * range;
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
        if let Some((k, _)) = best {
            return Some(k);
        }
        let cone = cone.clamp(0.0, 45.0);
        if cone <= 0.0 {
            return None;
        }
        let cos = cone.to_radians().cos();
        let mut best: Option<(usize, f32)> = None;
        for (k, q) in self.players.iter().enumerate() {
            if k == i || !q.alive {
                continue;
            }
            let c = q.hull(base).center();
            let d = c - eye;
            let len = d.length();
            if !(0.5..=range).contains(&len) {
                continue;
            }
            let dot = d.dot(look) / len;
            if dot < cos || (los && bd_core::voxel::raycast(&self.voxels, eye, d / len, len - 0.5).is_some()) {
                continue;
            }
            if best.is_none_or(|b| dot > b.1) {
                best = Some((k, dot));
            }
        }
        best.map(|b| b.0)
    }

    /// Is a player hull of player `i`'s size clear of terrain with its feet at `feet`, and in play?
    pub(crate) fn toolbox_clear(&self, i: usize, feet: Vec3) -> bool {
        if !feet.is_finite() || !self.in_play(feet, 0.0) {
            return false;
        }
        let hull = self.players[i].hull(&self.cfg.move_params);
        let half = hull.half();
        let lo = feet + Vec3::new(-half.x + 0.08, 0.08, -half.z + 0.08);
        let hi = feet + Vec3::new(half.x - 0.08, half.y * 2.0 - 0.08, half.z - 0.08);
        let (a, b) = (world_to_voxel(lo), world_to_voxel(hi));
        for x in a.x..=b.x {
            for y in a.y..=b.y {
                for z in a.z..=b.z {
                    if self.voxels.is_solid(bd_core::IVec3::new(x, y, z)) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// A clear spot for player `i`'s feet near `want` (then up, then a small ring). Deterministic.
    pub(crate) fn toolbox_spot(&self, i: usize, want: Vec3) -> Option<Vec3> {
        if self.toolbox_clear(i, want) {
            return Some(want);
        }
        for dy in [0.5, 1.0, 1.5, 2.5] {
            let p = want + Vec3::Y * dy;
            if self.toolbox_clear(i, p) {
                return Some(p);
            }
        }
        for r in [0.8, 1.6, 2.4] {
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                for dy in [0.0, 1.0] {
                    let p = want + Vec3::new(a.cos() * r, dy, a.sin() * r);
                    if self.toolbox_clear(i, p) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    /// Move player `i`'s feet to `to` (a teleport): the old velocity is kept only vertically damped.
    pub(crate) fn toolbox_teleport(&mut self, i: usize, to: Vec3) {
        let p = &mut self.players[i];
        p.mv.pos = to;
        p.mv.vel *= 0.25;
        p.mv.grounded = false;
    }

    /// A zero-damage credit hit (interrupts, KO credit, dodge-able). Returns whether it landed.
    pub(crate) fn toolbox_touch(&mut self, i: usize, k: usize, kind: HitKind) -> bool {
        let id = self.players[i].id;
        let tid = self.players[k].id;
        let c = self.players[k].hull(&self.cfg.move_params).center();
        self.apply_hit(HitInfo::new(tid, Some(id), kind, 0.0, 0.0, Vec3::Y, c)).applied
    }

    /// Hit every enemy player within `radius` of `at` (a round strike: arrival slashes, releases).
    /// Returns how many were struck.
    pub(crate) fn toolbox_burst(&mut self, i: usize, at: Vec3, radius: f32, damage: f32, knockback: f32, element: Element) -> u8 {
        let id = self.players[i].id;
        let base = self.cfg.move_params.clone();
        let mut hits = 0u8;
        for k in 0..self.players.len() {
            let q = &self.players[k];
            if k == i || !q.alive {
                continue;
            }
            let c = q.hull(&base).center();
            if c.distance(at) > radius + 0.4 {
                continue;
            }
            let dir = (c - at).normalize_or(Vec3::Y);
            let tid = q.id;
            let hit = HitInfo::new(tid, Some(id), HitKind::Melee, damage, knockback, dir, c).with_element(element);
            if self.apply_hit(hit).applied {
                hits = hits.saturating_add(1);
            }
        }
        hits
    }

    /// Does terrain block the straight line `a` → `b`?
    pub(crate) fn toolbox_blocked(&self, i: usize, a: Vec3, b: Vec3) -> bool {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return false;
        }
        let id = self.players[i].id;
        let cw = CollisionWorld::new(&self.voxels, self.colliders()).ignoring(crate::movement::ColliderId::Player(id));
        raycast_world(&cw, a, d / len, len, false).is_some()
    }

    /// Step every player's live toolbox ops (called at the top of the power step; players with
    /// nothing live cost one check).
    pub(crate) fn step_live_ops(&mut self, dt: f32) {
        for i in 0..self.players.len() {
            if self.players[i].power.live.idle() {
                continue;
            }
            if !self.players[i].alive {
                // A KO ends everything but a brand's spread memory (it burns out with them).
                let id = self.players[i].id;
                let live = std::mem::take(&mut self.players[i].power.live);
                if let Some(s) = live.rush.and_then(|r| r.stand) {
                    self.toolbox_dismiss(s);
                }
                let _ = id;
                continue;
            }
            self.step_drill(i, dt);
            self.step_mark(i, dt);
            self.step_seen(i, dt);
            self.step_phase(i, dt);
            self.step_rush(i, dt);
            self.step_brand(i, dt);
            self.step_waves(i, dt);
        }
    }

    /// A temporary construct (a rush stand) is done.
    pub(crate) fn toolbox_dismiss(&mut self, c: bd_core::ConstructId) {
        if let Some(c) = self.constructs.get_mut(c)
            && c.gone.is_none()
        {
            c.gone = Some(crate::events::GoneReason::Expired);
        }
    }
}
