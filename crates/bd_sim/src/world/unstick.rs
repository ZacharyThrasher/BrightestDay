//! UNSTICK: nobody stays inside the map.
//!
//! Playtest bug: after a map swap friends "spawned inside objects and needed to rejoin to get out".
//! Two layers keep that from ever needing a rejoin again:
//!
//! * **Pop-out** ([`embed_step`], the safety net) — part of the shared player step: the host's
//!   `step_movement` and client prediction (`World::predict_player`) run it right after the move,
//!   with the same inputs, so they agree. A living player whose hull has overlapped solid voxels or
//!   a static map collider (surf ramps, map movers; never water-walk sheets) for more than
//!   [`POP_AFTER`] s is moved to the nearest free space: straight up first (onto whatever they are
//!   buried in), then outward rings, a bounded search. Nothing free in range → the host puts them
//!   on the safest spawn ([`World::safest_spawn`]). The timer lives in the replicated movement
//!   state (`ExtraMoves::stuck`) so prediction replays it. Never fights intentional overlaps:
//!   seated / suited players, launched bodies (tumbling: punch-through plows *through* rock),
//!   bodies at a portal (tunnelling through a wall portal), bodies touching a construct (riding a
//!   giant, inside something being worn / piloted) and water-walk sheets are exempt. It never
//!   fires in normal play (the controller depenetrates shallow contacts itself), so the FFA golden
//!   hash doesn't move.
//! * **Unstick request** ([`World::request_unstick`], the player's button) — the host moves the
//!   player to the safest clear spawn of the current map (farthest from enemies). No damage, %, or
//!   score change; [`GameEvent::Unstuck`] drives a shimmer on arrival. Anti-abuse: once per
//!   [`UNSTICK_COOLDOWN`] s, and not within [`UNSTICK_DAMAGE_LOCKOUT`] s of taking damage (no free
//!   escape from a combo) — unless the player is detected as stuck (inside geometry, or
//!   [`NO_PROGRESS_SECS`] s pushing movement keys without getting anywhere). It never says no: a
//!   request that can't fire yet stays queued and fires when it can.
//!
//! Spawns: [`World::spawn_clear`] is the clearance test fresh starts, respawns, map swaps and
//! unsticks share (hull clear of terrain and static colliders, ground under the feet);
//! `pick_spawn` skips spawns that fail it.

use bd_core::voxel::{Aabb, VoxelQuery, world_to_voxel};
use bd_core::{DT, IVec3, PlayerId, Tick, VOXEL_SIZE, Vec3};

use super::World;
use crate::events::GameEvent;
use crate::movement::{Collider, ColliderId, CollisionWorld, MoveParams, PlayerState, is_water, raycast_world};
use crate::space::PortalPair;

/// Seconds a hull may stay embedded before it is popped out.
pub const POP_AFTER: f32 = 0.25;
/// Seconds between two unsticks of the same player.
pub const UNSTICK_COOLDOWN: f32 = 10.0;
/// No unstick within this many seconds of taking damage (unless stuck).
pub const UNSTICK_DAMAGE_LOCKOUT: f32 = 3.0;
/// Pushing movement keys this long without moving [`NO_PROGRESS_M`] counts as stuck.
pub const NO_PROGRESS_SECS: f32 = 3.0;
pub const NO_PROGRESS_M: f32 = 1.0;
/// Overlaps shallower than this are contact, not embedding (the controller keeps a 2 mm skin).
const EMBED_EPS: f32 = 0.02;
/// Spawns need ground within this far under the feet.
const SPAWN_GROUND_M: f32 = 3.0;
/// Pop-out search: up to this far straight up …
const POP_UP_M: f32 = 6.0;
/// … then these rings (m) …
const POP_RINGS: [f32; 8] = [0.75, 1.5, 2.25, 3.0, 4.0, 5.0, 6.5, 8.0];
/// … at these heights above the stuck feet (m).
const POP_RING_DY: [f32; 5] = [0.0, 0.5, 1.0, 2.0, 3.5];
/// Directions per ring (fixed order: deterministic).
const POP_DIRS: usize = 12;
/// A portal end this close (m, plus its radius) exempts the body (tunnelling through a wall portal).
const PORTAL_CLEARANCE: f32 = 2.0;

/// Host-side unstick bookkeeping of one player (`Player::unstick`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UnstickState {
    /// Seconds until the next unstick may fire.
    pub cd: f32,
    /// A request is waiting (cooldown / damage lockout).
    pub queued: bool,
    /// Seconds pushing movement keys without getting [`NO_PROGRESS_M`] away from `anchor`.
    pub push_t: f32,
    pub anchor: Vec3,
    /// Tick the player last took damage (their % went up).
    pub hurt_tick: Option<Tick>,
    /// % seen last tick (damage detection).
    pub last_pct: f32,
}

/// What [`embed_step`] did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Pop {
    /// Not embedded (or exempt, or not for long enough yet).
    None,
    /// Popped out; the feet were at this position.
    Moved(Vec3),
    /// Embedded too long and no free space in range: the host puts the player on a spawn.
    NoRoom,
}

/// Solid static map collider (surf ramps, map movers) — not constructs, players or water sheets.
#[inline]
fn static_solid(c: &Collider) -> bool {
    c.solid && matches!(c.id, ColliderId::Other(_)) && !is_water(c.id)
}

#[inline]
fn hull_at(pos: Vec3, half_w: f32, height: f32) -> Aabb {
    Aabb { min: pos - Vec3::new(half_w, 0.0, half_w), max: pos + Vec3::new(half_w, height, half_w) }
}

/// Does the AABB overlap the oriented box (separating axis test, strict)?
fn aabb_obb_overlap(a: &Aabb, c: &Collider) -> bool {
    let ca = a.center();
    let ha = a.half();
    let u = [c.rot * Vec3::X, c.rot * Vec3::Y, c.rot * Vec3::Z];
    let w = [Vec3::X, Vec3::Y, Vec3::Z];
    let d = c.center - ca;
    let mut axes = [Vec3::ZERO; 15];
    axes[..3].copy_from_slice(&w);
    axes[3..6].copy_from_slice(&u);
    let mut k = 6;
    for wi in w {
        for ui in u {
            axes[k] = wi.cross(ui);
            k += 1;
        }
    }
    for l in axes {
        let len2 = l.length_squared();
        if len2 < 1e-6 {
            continue;
        }
        let l = l / len2.sqrt();
        let ra = ha.x * l.x.abs() + ha.y * l.y.abs() + ha.z * l.z.abs();
        let rb = c.half.x * u[0].dot(l).abs() + c.half.y * u[1].dot(l).abs() + c.half.z * u[2].dot(l).abs();
        if d.dot(l).abs() >= ra + rb {
            return false;
        }
    }
    true
}

/// Is `hull` inside solid voxels or a static map collider (deeper than contact)?
pub fn hull_embedded<V: VoxelQuery + ?Sized>(voxels: &V, colliders: &[Collider], hull: &Aabb) -> bool {
    let h = Aabb { min: hull.min + Vec3::splat(EMBED_EPS), max: hull.max - Vec3::splat(EMBED_EPS) };
    if !(h.min.is_finite() && h.max.is_finite()) || h.min.cmpgt(h.max).any() {
        return false;
    }
    let (lo, hi) = (world_to_voxel(h.min), world_to_voxel(h.max));
    // A player hull spans a handful of voxels; anything huge (a giant) is not ours to judge.
    let n = (hi - lo + IVec3::ONE).as_i64vec3();
    if n.x * n.y * n.z > 4096 {
        return false;
    }
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                if voxels.is_solid(IVec3::new(x, y, z)) {
                    return true;
                }
            }
        }
    }
    colliders.iter().any(|c| static_solid(c) && aabb_obb_overlap(&h, c))
}

/// Nearest free spot for a hull (`half_w` × `height`) whose feet are stuck at `feet`: straight up
/// first (voxel-aligned, landing on what they are buried in), then outward rings. Deterministic;
/// `ok` filters candidates (in play).
fn free_spot<V: VoxelQuery + ?Sized>(
    voxels: &V,
    colliders: &[Collider],
    feet: Vec3,
    half_w: f32,
    height: f32,
    ok: impl Fn(Vec3) -> bool,
) -> Option<Vec3> {
    let free = |p: Vec3| ok(p) && !hull_embedded(voxels, colliders, &hull_at(p, half_w, height));
    // Up: feet on each voxel top above the current feet.
    let base = (feet.y / VOXEL_SIZE).floor() * VOXEL_SIZE + 0.01;
    let steps = (POP_UP_M / VOXEL_SIZE) as i32;
    for k in 1..=steps {
        let p = Vec3::new(feet.x, base + k as f32 * VOXEL_SIZE, feet.z);
        if free(p) {
            return Some(p);
        }
    }
    // Outward.
    for r in POP_RINGS {
        for dy in POP_RING_DY {
            for k in 0..POP_DIRS {
                let a = k as f32 * std::f32::consts::TAU / POP_DIRS as f32;
                let p = Vec3::new(feet.x + a.cos() * r, feet.y + dy + 0.01, feet.z + a.sin() * r);
                if free(p) {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// The pop-out safety net, one tick (host `step_movement` and client prediction, after the move).
/// `exempt`: seated / suited (the caller knows seats).
#[allow(clippy::too_many_arguments)]
pub(crate) fn embed_step<V: VoxelQuery + ?Sized>(
    st: &mut PlayerState,
    params: &MoveParams,
    voxels: &V,
    colliders: &[Collider],
    portals: &[PortalPair],
    exempt: bool,
    in_play: impl Fn(Vec3) -> bool,
    dt: f32,
) -> Pop {
    let hull = st.hull(params);
    let intentional = exempt
        // Launched: punch-through plows through rock on purpose (world/punch.rs).
        || st.tumble > 0.0
        // Tunnelling through a portal in a wall.
        || portals.iter().any(|pp| pp.ends.iter().any(|e| e.pos.distance(hull.center()) < e.radius + PORTAL_CLEARANCE))
        // Riding / wearing / inside a construct (giants, mechs, suits): its colliders carry you.
        || {
            let near = Aabb { min: hull.min - Vec3::splat(0.25), max: hull.max + Vec3::splat(0.25) };
            colliders.iter().any(|c| matches!(c.id, ColliderId::Construct { .. }) && c.solid && aabb_obb_overlap(&near, c))
        };
    if intentional || !hull_embedded(voxels, colliders, &hull) {
        st.extra.stuck = 0.0;
        return Pop::None;
    }
    st.extra.stuck += dt;
    if st.extra.stuck <= POP_AFTER {
        return Pop::None;
    }
    let from = st.pos;
    match free_spot(voxels, colliders, st.pos, params.hull_half_width, st.height(params), in_play) {
        Some(p) => {
            st.pos = p;
            st.vel = Vec3::ZERO;
            st.grounded = false;
            st.release_tethers();
            st.extra.stuck = 0.0;
            Pop::Moved(from)
        }
        None => Pop::NoRoom,
    }
}

/// Pop-out candidates stay in the arena: above the void, inside the terrain's sides.
pub(crate) fn in_play(arena: &bd_voxel::ArenaInfo, b: Aabb) -> impl Fn(Vec3) -> bool + use<> {
    let void_y = arena.void_y;
    move |p: Vec3| p.is_finite() && p.y > void_y + 1.0 && p.x > b.min.x && p.x < b.max.x && p.z > b.min.z && p.z < b.max.z && p.y < b.max.y
}

impl World {
    /// Static map colliders (surf ramps, water sheets, movers) as movement sees them. Rebuilt here
    /// because a fresh world has none until its first tick.
    fn static_colliders(&self) -> Vec<Collider> {
        let mut v = Vec::new();
        crate::movement::push_surf_colliders(&self.arena.surf_lines, &mut v);
        crate::movement::push_water_colliders(&self.arena, &mut v);
        super::map_mech::push_mover_colliders(&self.arena, &self.map_mech, &mut v);
        v
    }

    /// Is the living player `id` inside solid terrain / a static map collider (host or replica)?
    pub fn player_embedded(&self, id: PlayerId) -> bool {
        let Some(p) = self.player(id).filter(|p| p.alive) else { return false };
        let cols = if self.colliders.is_empty() { self.static_colliders() } else { Vec::new() };
        let cols = if self.colliders.is_empty() { &cols[..] } else { &self.colliders[..] };
        hull_embedded(&self.voxels, cols, &p.hull(&self.cfg.move_params))
    }

    /// Can a standing player spawn with their feet at `pos`: hull clear of terrain and static map
    /// colliders, ground (terrain, a ramp, walkable water) within a few metres under the feet?
    pub fn spawn_clear(&self, pos: Vec3) -> bool {
        self.spawn_clear_with(pos, &self.static_colliders())
    }

    fn spawn_clear_with(&self, pos: Vec3, cols: &[Collider]) -> bool {
        let mp = &self.cfg.move_params;
        if !pos.is_finite() || hull_embedded(&self.voxels, cols, &hull_at(pos, mp.hull_half_width, mp.stand_height)) {
            return false;
        }
        let cw = CollisionWorld::new(&self.voxels, cols);
        raycast_world(&cw, pos + Vec3::Y * 0.3, Vec3::NEG_Y, SPAWN_GROUND_M + 0.3, false).is_some()
    }

    /// Indices of the arena spawns that pass [`Self::spawn_clear`] (all of them if none does).
    pub(crate) fn clear_spawns(&self) -> Vec<usize> {
        let cols = self.static_colliders();
        let ok: Vec<usize> = (0..self.arena.spawns.len()).filter(|&k| self.spawn_clear_with(self.arena.spawns[k], &cols)).collect();
        if ok.is_empty() { (0..self.arena.spawns.len()).collect() } else { ok }
    }

    /// The safest clear spawn for `id`: farthest from every living enemy (ties → lowest index).
    pub fn safest_spawn(&self, id: PlayerId) -> Vec3 {
        let spawns = &self.arena.spawns;
        if spawns.is_empty() {
            return self.arena.center + Vec3::Y * 2.0;
        }
        let mut best = (usize::MAX, f32::NEG_INFINITY);
        for k in self.clear_spawns() {
            let s = spawns[k];
            let nearest = self
                .players
                .iter()
                .filter(|p| p.alive && p.id != id)
                .map(|p| (p.mv.pos - s).length_squared())
                .fold(f32::INFINITY, f32::min);
            if nearest > best.1 {
                best = (k, nearest);
            }
        }
        spawns.get(best.0).copied().unwrap_or(self.arena.center)
    }

    /// The player asked to be unstuck (the Esc menu / a bound key; host command). Queues; fires in
    /// [`Self::step_unstick`] as soon as the cooldown and damage lockout allow. Returns false for
    /// an unknown / dead player.
    pub fn request_unstick(&mut self, id: PlayerId) -> bool {
        let Some(p) = self.player_mut(id).filter(|p| p.alive) else { return false };
        p.unstick.queued = true;
        true
    }

    /// Seconds until a queued unstick of `id` can fire (0 = next tick), for UIs.
    pub fn unstick_wait(&self, id: PlayerId) -> f32 {
        let Some(p) = self.player(id) else { return 0.0 };
        let u = &p.unstick;
        let lock = if self.unstick_stuck(p) {
            0.0
        } else {
            u.hurt_tick.map_or(0.0, |t| (UNSTICK_DAMAGE_LOCKOUT - self.tick.saturating_sub(t) as f32 * DT).max(0.0))
        };
        u.cd.max(lock)
    }

    /// Stuck, for the damage-lockout exception: embedded now / recently, or pushing without progress.
    fn unstick_stuck(&self, p: &super::Player) -> bool {
        p.mv.extra.stuck > 0.0 || p.unstick.push_t >= NO_PROGRESS_SECS || hull_embedded(&self.voxels, &self.colliders, &p.hull(&self.cfg.move_params))
    }

    /// Per tick (host, after movement): progress tracking, damage detection, queued requests.
    pub(crate) fn step_unstick(&mut self, dt: f32) {
        for i in 0..self.players.len() {
            let tick = self.tick;
            let p = &mut self.players[i];
            let u = &mut p.unstick;
            u.cd = (u.cd - dt).max(0.0);
            if p.damage_pct > u.last_pct {
                u.hurt_tick = Some(tick);
            }
            u.last_pct = p.damage_pct;
            if !p.alive {
                u.queued = false;
                u.push_t = 0.0;
                u.anchor = p.mv.pos;
                continue;
            }
            let pushing = p.input.move_x != 0 || p.input.move_z != 0;
            let d = p.mv.pos - u.anchor;
            if !pushing || Vec3::new(d.x, 0.0, d.z).length() >= NO_PROGRESS_M || d.y.abs() >= NO_PROGRESS_M * 2.0 {
                u.anchor = p.mv.pos;
                u.push_t = 0.0;
            } else {
                u.push_t += dt;
            }
            if !u.queued || u.cd > 0.0 {
                continue;
            }
            let p = &self.players[i];
            let hurt = p.unstick.hurt_tick.is_some_and(|t| tick.saturating_sub(t) as f32 * DT < UNSTICK_DAMAGE_LOCKOUT);
            if hurt && !self.unstick_stuck(p) {
                continue;
            }
            let id = p.id;
            self.unstick_now(id);
        }
    }

    /// Move `id` to the safest clear spawn now (no damage / % / score change).
    pub(crate) fn unstick_now(&mut self, id: PlayerId) {
        if self.player(id).is_some_and(|p| p.seat.is_some()) {
            self.eject(id);
        }
        let to = self.safest_spawn(id);
        let Some(i) = self.pidx(id) else { return };
        let p = &mut self.players[i];
        let from = p.mv.pos;
        p.mv.pos = to;
        p.mv.vel = Vec3::ZERO;
        p.mv.grounded = false;
        p.mv.tumble = 0.0;
        p.mv.release_tethers();
        p.mv.extra.stuck = 0.0;
        p.launch = None;
        p.punch = None;
        p.last_safe_pos = to;
        p.unstick.cd = UNSTICK_COOLDOWN;
        p.unstick.queued = false;
        p.unstick.push_t = 0.0;
        p.unstick.anchor = to;
        self.events.push(GameEvent::Unstuck { player: id, from, to, auto: false });
    }

    /// Host side of [`embed_step`]'s outcome for player index `i`.
    pub(crate) fn after_pop(&mut self, i: usize, pop: Pop) {
        let id = self.players[i].id;
        match pop {
            Pop::None => {}
            Pop::Moved(from) => {
                let to = self.players[i].mv.pos;
                self.events.push(GameEvent::Unstuck { player: id, from, to, auto: true });
            }
            Pop::NoRoom => {
                let from = self.players[i].mv.pos;
                let to = self.safest_spawn(id);
                let p = &mut self.players[i];
                p.mv.pos = to;
                p.mv.vel = Vec3::ZERO;
                p.mv.grounded = false;
                p.mv.release_tethers();
                p.mv.extra.stuck = 0.0;
                p.last_safe_pos = to;
                self.events.push(GameEvent::Unstuck { player: id, from, to, auto: true });
            }
        }
    }
}
