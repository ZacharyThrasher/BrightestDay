//! Blast zones, the tether save, construct recall and visible summon placement
//! (COMBAT.md §1a "Blast zones", §3 "Recall", §3 "Placement").
//!
//! * [`Zones`] — the player blast zones, the same on every map: **Bottom** below
//!   `ArenaInfo::void_y` (the void / water; Orbit's re-entry burn-up), **Side** past the voxel world's
//!   sides + `smash.blast_margin_side`, **Top** above the voxel world's top + `smash.blast_margin_top`
//!   (the *star KO*; on Orbit that is "launched into space").
//! * **Tether save** ([`World::tether_holds`]) — a player with at least one hook *attached* to an
//!   anchor inside the zones is not KO'd by the Side / Bottom zones: they hang, swing, reel back.
//!   The star KO (Top) always applies. A rope holds at most `bounds.tether_hold_depth` past a zone
//!   (keeps bodies inside the world); a broken / released rope hands you back to the normal rules
//!   on the same tick.
//! * **Recall** ([`World::recall_lost_constructs`]) — a free construct that leaves the map (the
//!   zones, with the bottom `bounds.recall_below_void` under the void) blinks back next to its
//!   summoner (a free spot, slightly in front and above, velocity zeroed) with
//!   [`GameEvent::ConstructRecalled`]. Despawned instead (the old rule) when the summoner is
//!   KO'd / respawning / gone, for children (swarm copies, slice halves), power fruits, non-finite
//!   state, or after `bounds.recall_max` recalls within `bounds.recall_window` s (anti-loop valve).
//! * **Visible placement** ([`World::place_visible`]) — fresh creatures, structures and vehicles
//!   land where the caster can see them: along the aim at a size-aware distance
//!   (`summon.fit_k · radius`, clamped), snapped to the surface under the crosshair when it is in
//!   range, clear of terrain and players, with a line of sight from the eye (walked back / sideways
//!   when blocked); huge ones further out and to the side. Held / worn / thrown things are
//!   unaffected.
//!
//! Everything here is a pure function of the world state (fixed candidate orders, no hashing), so
//! host and replicas agree.

use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::voxel::{Aabb, VoxelQuery, raycast, world_to_voxel};
use bd_core::{ConstructId, DT, IVec3, Quat, Tick, Vec3};
use serde::{Deserialize, Serialize};

use super::World;
use crate::constructs::{Loco, body_box};
use crate::events::{GameEvent, GoneReason};
use crate::geom;
use crate::movement::{AnchorTarget, ColliderId, CollisionWorld, Hook, raycast_world};
use crate::smash::BlastZone;
use crate::space::LinkEnd;

// ------------------------------------------------------------------------------------ tuning

/// Blast-zone knobs (`tuning.bounds`, `assets/tuning/combat.ron`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoundsTuning {
    /// An attached tether anchored inside the arena cancels Side / Bottom KOs (never the star KO).
    pub tether_saves: bool,
    /// …but only this far past a Side / Bottom zone (m). Deeper = KO'd anyway.
    pub tether_hold_depth: f32,
    /// Constructs that leave the map blink back to their summoner (off = despawn, the old rule).
    pub recall: bool,
    /// Constructs are lost this far below `void_y` (m). Sides / top: the player zones.
    pub recall_below_void: f32,
    /// Anti-loop valve: after this many recalls within `recall_window` s the next exit despawns.
    pub recall_max: u8,
    pub recall_window: f32,
    /// Recall spot: this far in front of the summoner (+ the construct's footprint) …
    pub recall_ahead: f32,
    /// … and this far above their feet (m).
    pub recall_up: f32,
}

impl Default for BoundsTuning {
    fn default() -> Self {
        Self {
            tether_saves: true,
            tether_hold_depth: 40.0,
            recall: true,
            recall_below_void: 6.0,
            recall_max: 3,
            recall_window: 10.0,
            recall_ahead: 2.5,
            recall_up: 1.0,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl BoundsTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.tether_hold_depth = fin(self.tether_hold_depth, d.tether_hold_depth, 0.0, 60.0);
        self.recall_below_void = fin(self.recall_below_void, d.recall_below_void, 0.0, 30.0);
        self.recall_max = self.recall_max.min(50);
        self.recall_window = fin(self.recall_window, d.recall_window, 0.0, 120.0);
        self.recall_ahead = fin(self.recall_ahead, d.recall_ahead, 0.5, 20.0);
        self.recall_up = fin(self.recall_up, d.recall_up, 0.0, 10.0);
        self
    }
}

/// Summon placement + construct lifetimes (`tuning.summon`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SummonTuning {
    /// Visible placement for fresh creatures / structures / vehicles (off = at the crosshair, the
    /// old rule).
    pub visible: bool,
    /// Fit distance from the eye: `clamp(fit_k · radius, near_min, fit_max)` (≈ fits a 90° view).
    pub fit_k: f32,
    /// Never closer than this (+ the construct's footprint) to the eye (m).
    pub near_min: f32,
    pub fit_max: f32,
    /// The surface under the crosshair is used when it is at most this far (m).
    pub snap_max: f32,
    /// Radius (m) from which a construct counts as huge (kaiju, colossal): placed at
    /// `clamp(huge_k · radius, near_min, huge_max)`, `huge_side_deg` to the right of the aim.
    pub huge_radius: f32,
    pub huge_k: f32,
    pub huge_max: f32,
    pub huge_side_deg: f32,
    /// Accept a spot when its centre is within this angle of the aim (first pass).
    pub view_half_deg: f32,
    /// Walkers / structures stand on ground at most this far below the spot (m).
    pub ground_drop: f32,
    // ---- lifetimes (COMBAT.md §3) ----
    pub life_creature: f32,
    pub life_kaiju: f32,
    pub life_structure: f32,
    pub life_object: f32,
    pub life_dome: f32,
    /// Piloted constructs live at least this long.
    pub life_piloted_min: f32,
    /// A thrown object that came to rest stays this long (s) after 1 s at rest.
    pub object_rest_linger: f32,
    /// An object that landed with a crater stays this long (s).
    pub object_landed_linger: f32,
}

impl Default for SummonTuning {
    fn default() -> Self {
        Self {
            visible: true,
            fit_k: 2.4,
            near_min: 3.0,
            fit_max: 26.0,
            snap_max: 40.0,
            huge_radius: 3.5,
            huge_k: 3.0,
            huge_max: 80.0,
            huge_side_deg: 16.0,
            view_half_deg: 40.0,
            ground_drop: 12.0,
            life_creature: 20.0,
            life_kaiju: 24.0,
            life_structure: 30.0,
            life_object: 10.0,
            life_dome: 3.0,
            life_piloted_min: 30.0,
            object_rest_linger: 4.0,
            object_landed_linger: 4.0,
        }
    }
}

impl SummonTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.fit_k = fin(self.fit_k, d.fit_k, 0.5, 10.0);
        self.near_min = fin(self.near_min, d.near_min, 0.5, 20.0);
        self.fit_max = fin(self.fit_max, d.fit_max, self.near_min, 100.0);
        self.snap_max = fin(self.snap_max, d.snap_max, 1.0, crate::world::actions::CAST_RANGE);
        self.huge_radius = fin(self.huge_radius, d.huge_radius, 0.5, 100.0);
        self.huge_k = fin(self.huge_k, d.huge_k, 0.5, 10.0);
        self.huge_max = fin(self.huge_max, d.huge_max, self.near_min, 200.0);
        self.huge_side_deg = fin(self.huge_side_deg, d.huge_side_deg, 0.0, 40.0);
        self.view_half_deg = fin(self.view_half_deg, d.view_half_deg, 5.0, 90.0);
        self.ground_drop = fin(self.ground_drop, d.ground_drop, 0.0, 60.0);
        self.life_creature = fin(self.life_creature, d.life_creature, 1.0, 600.0);
        self.life_kaiju = fin(self.life_kaiju, d.life_kaiju, 1.0, 600.0);
        self.life_structure = fin(self.life_structure, d.life_structure, 1.0, 600.0);
        self.life_object = fin(self.life_object, d.life_object, 1.0, 600.0);
        self.life_dome = fin(self.life_dome, d.life_dome, 0.5, 60.0);
        self.life_piloted_min = fin(self.life_piloted_min, d.life_piloted_min, 1.0, 600.0);
        self.object_rest_linger = fin(self.object_rest_linger, d.object_rest_linger, 0.0, 60.0);
        self.object_landed_linger = fin(self.object_landed_linger, d.object_landed_linger, 0.0, 60.0);
        self
    }

    /// Lifetime of a fresh cast (∞ for held / worn). Replaces `constructs::lifetime_secs` for casts.
    pub fn lifetime(&self, form: Form) -> f32 {
        if form == Form::Kaiju {
            return self.life_kaiju;
        }
        if form == Form::Dome {
            return self.life_dome;
        }
        match form.category() {
            FormCategory::Held | FormCategory::Worn => f32::INFINITY,
            FormCategory::Structure => self.life_structure,
            FormCategory::Object => self.life_object,
            FormCategory::Creature => self.life_creature,
        }
    }
}

// ------------------------------------------------------------------------------------- zones

/// Blast zones (see the module docs). `contains` = in play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zones {
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
    /// Below this: Bottom.
    pub bottom: f32,
    /// Above this: Top (star KO).
    pub top: f32,
}

impl Zones {
    /// Which zone `p` is in (non-finite = Bottom). Same order as the KO check: Bottom, Top, Side.
    pub fn zone_of(&self, p: Vec3) -> Option<BlastZone> {
        if !p.is_finite() || p.y < self.bottom {
            Some(BlastZone::Bottom)
        } else if p.y > self.top {
            Some(BlastZone::Top)
        } else if p.x < self.min_x || p.x > self.max_x || p.z < self.min_z || p.z > self.max_z {
            Some(BlastZone::Side)
        } else {
            None
        }
    }

    #[inline]
    pub fn contains(&self, p: Vec3) -> bool {
        self.zone_of(p).is_none()
    }

    /// How far past the Side / Bottom zones `p` is (0 inside; ∞ for non-finite).
    pub fn depth(&self, p: Vec3) -> f32 {
        if !p.is_finite() {
            return f32::INFINITY;
        }
        (self.bottom - p.y).max(self.min_x - p.x).max(p.x - self.max_x).max(self.min_z - p.z).max(p.z - self.max_z).max(0.0)
    }

    /// Shrunk by `m` on every side (a spot comfortably in play).
    pub fn inset(&self, m: f32) -> Self {
        Self { min_x: self.min_x + m, max_x: self.max_x - m, min_z: self.min_z + m, max_z: self.max_z - m, bottom: self.bottom + m, top: self.top - m }
    }
}

/// Voxel-world overlap budget per box test (big boxes are sampled on a coarser grid).
const OVERLAP_SAMPLES: i64 = 8192;

impl World {
    /// The player blast zones (COMBAT.md §1a).
    pub fn blast_zones(&self) -> Zones {
        let b = self.voxels.bounds();
        let (side, top) = (self.cfg.tuning.smash.blast_margin_side, self.cfg.tuning.smash.blast_margin_top);
        Zones { min_x: b.min.x - side, max_x: b.max.x + side, min_z: b.min.z - side, max_z: b.max.z + side, bottom: self.arena.void_y, top: b.max.y + top }
    }

    /// Where constructs count as lost: the player zones, bottom `recall_below_void` under the void.
    pub fn construct_zones(&self) -> Zones {
        Zones { bottom: self.arena.void_y - self.cfg.tuning.bounds.recall_below_void, ..self.blast_zones() }
    }

    // --------------------------------------------------------------------------- tether save

    /// Is player `i` held in play by a tether? (At least one hook attached to an anchor inside the
    /// zones, and not deeper than `tether_hold_depth` past them.) Only Side / Bottom KOs consult it.
    pub fn tether_holds(&self, i: usize) -> bool {
        let t = &self.cfg.tuning.bounds;
        let Some(p) = self.players.get(i) else { return false };
        if !t.tether_saves || !p.alive || !p.mv.pos.is_finite() || p.seat.is_some_and(|s| !s.suit) {
            return false;
        }
        let z = self.blast_zones();
        if z.depth(p.mv.pos) > t.tether_hold_depth {
            return false;
        }
        p.mv.hooks.iter().any(|h| h.attached() && z.contains(h.pos))
    }

    // -------------------------------------------------------------------------------- recall

    /// A free construct outside [`Self::construct_zones`] (or with non-finite state).
    fn is_lost(c: &crate::constructs::Construct, z: &Zones) -> bool {
        c.gone.is_none() && !c.attached() && (!c.vel.is_finite() || !z.contains(c.pos))
    }

    /// Recall (or, failing that, despawn) every construct that left the map this tick.
    pub(crate) fn recall_lost_constructs(&mut self) {
        let z = self.construct_zones();
        let t = self.cfg.tuning.bounds;
        let now = self.tick;
        let window = (t.recall_window / DT) as Tick;
        if !self.recall_log.is_empty() {
            self.recall_log.retain(|&(_, at)| now.saturating_sub(at) <= window);
        }
        // Steady state: nothing lost, nothing allocated.
        if !self.constructs.iter().any(|c| Self::is_lost(c, &z)) {
            return;
        }
        let lost: Vec<ConstructId> = self.constructs.iter().filter(|c| Self::is_lost(c, &z)).map(|c| c.id).collect();
        for id in lost {
            if !self.recall_one(id, &t)
                && let Some(c) = self.constructs.get_mut(id)
            {
                c.gone = Some(GoneReason::Expired);
            }
        }
    }

    /// Blink one lost construct back to its summoner. False = despawn it instead.
    fn recall_one(&mut self, id: ConstructId, t: &BoundsTuning) -> bool {
        let Some(c) = self.constructs.get(id) else { return false };
        let (owner, from, pilot) = (c.owner, c.pos, c.pilot);
        let recalls = self.recall_log.iter().filter(|(cid, _)| *cid == id).count();
        let fruit = self.powers.fruits.iter().any(|f| f.construct == id);
        let ok = t.recall && from.is_finite() && c.rot.is_finite() && c.parent.is_none() && !fruit && recalls < usize::from(t.recall_max);
        let Some(k) = self.pidx(owner).filter(|&k| ok && self.players[k].alive && !self.players[k].eliminated) else { return false };
        // A seated pilot is ejected where the vehicle was lost: the tether / KO rules decide their fate.
        if let Some(pid) = pilot {
            let vel = if c.vel.is_finite() { c.vel } else { Vec3::ZERO };
            self.eject_with(pid, id, vel, true);
        }
        let Some((to, rot)) = self.recall_spot(k, id) else { return false };
        let Some(c) = self.constructs.get_mut(id) else { return false };
        c.pos = to;
        c.prev_pos = to;
        c.rot = rot;
        c.vel = Vec3::ZERO;
        c.body.spin = Vec3::ZERO;
        c.stuck_to = None;
        c.comp.attached_to = None;
        if c.body.loose && c.category == FormCategory::Structure {
            c.body.loose = false;
            if c.body.loco == Loco::Ballistic {
                c.body.loco = Loco::Static;
            }
        }
        if c.held.thrown {
            // A thrown held item comes back to the hand (the held controller poses it next tick).
            c.held.thrown = false;
            c.held.returning = false;
            c.held.thor = false;
            c.held.fly_t = 0.0;
            c.held.fly_vel = Vec3::ZERO;
            c.held.mode = crate::constructs::held::Mode::Idle;
        }
        // Ropes / chains tied to it snap (it is somewhere else now).
        for p in &mut self.players {
            for h in &mut p.mv.hooks {
                if h.attached() && matches!(h.target, Some(AnchorTarget::Collider(ColliderId::Construct { id: cid, .. })) if cid == id) {
                    *h = Hook::default();
                }
            }
        }
        for l in &mut self.links {
            if l.a == LinkEnd::Construct(id) || l.b == LinkEnd::Construct(id) {
                l.hp = 0.0;
            }
        }
        self.recall_log.push((id, self.tick));
        self.events.push(GameEvent::ConstructRecalled { id, owner, from, to });
        true
    }

    /// A free spot next to summoner `k` for construct `id`: slightly in front and above, not in
    /// terrain / players / other constructs, in play. Returns (pivot, rotation).
    fn recall_spot(&self, k: usize, id: ConstructId) -> Option<(Vec3, Quat)> {
        let c = self.constructs.get(id)?;
        let t = self.cfg.tuning.bounds;
        let p = &self.players[k];
        let z = self.blast_zones().inset(2.0);
        // Summoner hanging off the map on a rope / falling: the last solid ground they stood on.
        let feet = if z.contains(p.mv.pos) && p.mv.pos.y > self.arena.void_y + 2.0 { p.mv.pos } else { p.last_safe_pos };
        let yaw = p.input.yaw;
        let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let upright = matches!(c.category, FormCategory::Creature | FormCategory::Structure);
        let rot = if upright { geom::yaw_rotation(c.rot * Vec3::NEG_Z) } else { c.rot };
        let grounded = upright && c.body.loco != Loco::Fly && c.recipe.form != Form::Platform;
        let (half, lc) = (c.half, c.local_center);
        let foot = Vec3::new(half.x, 0.0, half.z).length();
        let eye = p.eye(&self.cfg.move_params);
        for pass in 0..2 {
            for (yaw_deg, extra) in RECALL_RING {
                let dir = Quat::from_rotation_y(yaw_deg.to_radians()) * fwd;
                let spot = feet + dir * (t.recall_ahead + foot + extra) + Vec3::Y * t.recall_up;
                let lift = half.y.min(3.0) + 1.0;
                let pivot = match self.ground_below(spot + Vec3::Y * lift, lift + RECALL_GROUND_DROP).filter(|_| grounded) {
                    Some(g) => g + Vec3::Y * 0.02,
                    // No ground below (a beam, a ledge): the second pass drops it from the air.
                    None if grounded && pass == 0 => continue,
                    // Centre slightly above the summoner's head height.
                    None => spot + Vec3::Y * 1.0 - rot * lc,
                };
                let center = pivot + rot * lc;
                if !z.contains(center) || !self.box_free(center, half, rot, Some(id)) {
                    continue;
                }
                if pass == 0 && !self.line_of_sight(eye, center, half.length()) {
                    continue;
                }
                return Some((pivot, rot));
            }
        }
        None
    }

    // ---------------------------------------------------------------------- visible placement

    /// Where a fresh creature / structure / vehicle should stand so its caster sees it (module
    /// docs). `fallback` = the old crosshair placement, returned when this doesn't apply or finds
    /// nothing. `origin` / `dir` = the validated cast ray; `rot` = the construct's spawn rotation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn place_visible(&self, i: usize, recipe: &Recipe, shape: Option<&ShapeSpec>, origin: Vec3, dir: Vec3, rot: Quat, fallback: Vec3) -> Vec3 {
        let t = self.cfg.tuning.summon;
        let category = recipe.form.category();
        let suit = recipe.has(Mechanic::Piloted) && recipe.scale <= super::actions::SUIT_MAX_SCALE;
        let applies = match category {
            FormCategory::Creature => true,
            // Domes wrap the caster, cages close around what you aim at, platforms float where aimed.
            FormCategory::Structure => !matches!(recipe.form, Form::Dome | Form::Cage | Form::Platform),
            _ => false,
        };
        if !t.visible || !applies || suit || !origin.is_finite() || !dir.is_finite() {
            return fallback;
        }
        let Some(p) = self.players.get(i) else { return fallback };
        let (half, lc) = body_box(recipe, shape);
        let r = half.length().max(0.3);
        let foot = Vec3::new(half.x, 0.0, half.z).length();
        // Huge = big creatures (kaiju) and colossal things; a big wall still goes where it is aimed.
        let huge = (category == FormCategory::Creature && r >= t.huge_radius) || crate::willpower::is_colossal(recipe);
        let fit = if huge { (t.huge_k * r).clamp(t.near_min, t.huge_max) } else { (t.fit_k * r).clamp(t.near_min, t.fit_max) };
        let near = t.near_min + foot;
        let flier = category == FormCategory::Creature && (recipe.has(Mechanic::Flies) || matches!(recipe.form, Form::Bird | Form::Insect));
        let grounded = !flier;
        let aim = dir.normalize_or(p.look());
        let flat = {
            let f = Vec3::new(aim.x, 0.0, aim.z).normalize_or_zero();
            if f == Vec3::ZERO { Vec3::new(-p.input.yaw.sin(), 0.0, -p.input.yaw.cos()) } else { f }
        };
        let side = if huge { -t.huge_side_deg.to_radians() } else { 0.0 };
        let cos_view = t.view_half_deg.to_radians().cos();
        let z = self.blast_zones().inset(2.0);
        // Candidate points, best first: along the aim (snapped to the crosshair surface when in
        // range, else at the fit distance), then along the level view direction; each walked back
        // toward the caster, then swung to either side.
        let cw = CollisionWorld::new(&self.voxels, &self.colliders);
        let mut cands: Vec<Vec3> = Vec::with_capacity(72);
        // ±60° / ±90° only pass the second (no view cone) pass: at a ledge, along the edge.
        for yaw_deg in [0.0f32, 18.0, -18.0, 34.0, -34.0, 60.0, -60.0, 90.0, -90.0] {
            let q = Quat::from_rotation_y(side + yaw_deg.to_radians());
            let (a, f) = (q * aim, q * flat);
            // Terrain and constructs (an enemy wall in the way is a wall).
            match raycast_world(&cw, origin, a, t.snap_max, false).filter(|h| h.t >= near) {
                Some(h) => {
                    let floor = h.normal.y > 0.5;
                    // Stand on a floor under the crosshair; back off a wall by the footprint.
                    let d0 = if floor { h.t } else { (h.t - foot - 0.3).max(near) };
                    if floor {
                        cands.push(h.point);
                    }
                    for k in [1.0f32, 0.75, 0.5] {
                        cands.push(origin + a * (d0 * k).max(near));
                    }
                }
                None if !grounded => {
                    // Fliers hover on the aim at the fit distance.
                    cands.push(origin + a * fit);
                }
                None => {}
            }
            for k in [1.0f32, 0.72, 0.5] {
                cands.push(origin + f * (fit * k).max(near));
            }
        }
        for pass in 0..2 {
            for &spot in &cands {
                let pivot = if grounded {
                    let Some(g) = self.ground_below(spot + Vec3::Y * 2.5, 2.5 + t.ground_drop) else { continue };
                    g + Vec3::Y * 0.02
                } else {
                    spot - rot * lc
                };
                let center = pivot + rot * lc;
                let to = center - origin;
                let dist = to.length();
                if !z.contains(center) || Vec3::new(to.x, 0.0, to.z).length() < near - 0.05 || dist < 1e-3 {
                    continue;
                }
                if pass == 0 && to.dot(aim) / dist < cos_view {
                    continue;
                }
                if !self.line_of_sight(origin, center, r) || !self.box_free(center, half, rot, None) {
                    continue;
                }
                return pivot;
            }
        }
        fallback
    }

    // ----------------------------------------------------------------------------- free space

    /// Top of the ground below `from` within `max` metres.
    pub(crate) fn ground_below(&self, from: Vec3, max: f32) -> Option<Vec3> {
        raycast(&self.voxels, from, Vec3::NEG_Y, max).filter(|h| h.normal.y > 0).map(|h| h.point)
    }

    /// Can the eye at `eye` see the centre of something of radius `r` at `center` (terrain and
    /// construct colliders block)?
    pub(crate) fn line_of_sight(&self, eye: Vec3, center: Vec3, r: f32) -> bool {
        let d = center - eye;
        let dist = d.length();
        if !dist.is_finite() || dist < 1e-3 {
            return true;
        }
        let cw = CollisionWorld::new(&self.voxels, &self.colliders);
        raycast_world(&cw, eye, d / dist, dist, false).is_none_or(|h| h.t >= dist - r * 0.5)
    }

    /// Is an oriented box (centre, half extents, rotation) clear of terrain, living players and
    /// other constructs' colliders (`skip` = its own)? Touching counts as clear.
    pub(crate) fn box_free(&self, center: Vec3, half: Vec3, rot: Quat, skip: Option<ConstructId>) -> bool {
        if !(center.is_finite() && half.is_finite() && rot.is_finite()) {
            return false;
        }
        let b = geom::obb_bounds(center, half, rot);
        let b = Aabb { min: b.min + Vec3::splat(0.06), max: b.max - Vec3::splat(0.06) };
        if b.min.cmpgt(b.max).any() {
            return true;
        }
        if self.voxels_overlap(&b) {
            return false;
        }
        let base = &self.cfg.move_params;
        if self.players.iter().any(|p| p.alive && p.hull(base).intersects(&b)) {
            return false;
        }
        !self.colliders.iter().any(|c| {
            let own = matches!(c.id, ColliderId::Construct { id, .. } if Some(id) == skip);
            let mine = matches!(c.id, ColliderId::Player(_));
            !own && !mine && c.solid && geom::obb_bounds(c.center, c.half, c.rot).intersects(&b)
        })
    }

    /// Any solid voxel inside `b`? Boxes over [`OVERLAP_SAMPLES`] voxels are sampled on a coarser
    /// grid (always including the far faces).
    fn voxels_overlap(&self, b: &Aabb) -> bool {
        let wb = self.voxels.bounds();
        let lo = world_to_voxel(b.min.max(wb.min));
        let hi = world_to_voxel(b.max.min(wb.max - Vec3::splat(1e-3)));
        if lo.cmpgt(hi).any() {
            return false;
        }
        let n = (hi - lo + IVec3::ONE).as_i64vec3();
        let count = n.x * n.y * n.z;
        let stride = if count <= OVERLAP_SAMPLES { 1 } else { ((count as f64 / OVERLAP_SAMPLES as f64).cbrt().ceil() as i32).max(1) };
        let axis = |a: i32, b: i32| (a..=b).step_by(stride as usize).chain(std::iter::once(b));
        for x in axis(lo.x, hi.x) {
            for y in axis(lo.y, hi.y) {
                for z in axis(lo.z, hi.z) {
                    if self.voxels.is_solid(IVec3::new(x, y, z)) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Walkers / structures recalled next to a summoner stand on ground at most this far below them.
const RECALL_GROUND_DROP: f32 = 16.0;

/// Recall candidates around the summoner: (yaw offset from their facing in degrees, extra metres).
const RECALL_RING: [(f32, f32); 12] = [
    (0.0, 0.0),
    (30.0, 0.0),
    (-30.0, 0.0),
    (0.0, 2.5),
    (70.0, 0.0),
    (-70.0, 0.0),
    (110.0, 0.0),
    (-110.0, 0.0),
    (180.0, 0.0),
    (0.0, 6.0),
    (90.0, 4.0),
    (-90.0, 4.0),
];
