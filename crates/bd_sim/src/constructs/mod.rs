//! Constructs: the store, the per-construct state, and the interface the world talks to.
//!
//! > **PLACEHOLDER BEHAVIOURS.** Everything in [`placeholder`] is a deliberately simple stand-in so
//! > the world pipeline (cast → forming → behaviour → hits/carves/clash → shatter → snapshot) is
//! > testable end-to-end and bots can fight. The construct engineer replaces it with the component
//! > system of COMBAT.md §5b. The *interface* in this file is the contract with the world:
//!
//! * The world calls, each tick and in this order: [`ConstructStore::movement_mods`] (per player,
//!   before movement), [`ConstructStore::post_move`] (per player, after movement),
//!   [`ConstructStore::update`] (all constructs), then applies everything written into
//!   [`ConstructOut`] (hits, construct hits, explosions, carves, projectiles, impulses, statuses,
//!   willpower drain, ejects, events) and finally [`ConstructStore::reap`].
//! * Constructs never mutate players or terrain directly: they read [`ConstructCtx`] (a read-only
//!   view of players, terrain, colliders, lag-comp history, rules) and write intents to
//!   [`ConstructOut`]. The world owns all combat rules (`World::apply_hit`), so shields, KO credit,
//!   statuses and juice stay consistent no matter what a construct does.
//! * Casting, grafting, limits, costs and dismissal live in the world (`world/actions.rs`); the
//!   store only provides [`ConstructStore::spawn`], [`ConstructStore::graft`] and friends.
//! * Collision: [`ConstructStore::write_colliders`] feeds movement (solid / anchorable OBBs), and
//!   [`ConstructStore::segment_query`] is the generic swept test used by projectiles and aim.

pub mod forms;
mod placeholder;

use bd_core::input::PlayerInput;
use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::voxel::Aabb;
use bd_core::{ConstructId, PlayerId, Quat, Tick, Vec3};
use bd_voxel::VoxelWorld;
use bd_core::voxel::Material;
use bd_core::IVec3;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo};
use crate::events::{GameEvent, GoneReason};
use crate::geom;
use crate::history::HullHistory;
use crate::movement::{Collider, ColliderId, MoveEvents, MoveModifiers, MoveParams, PlayerState};
use crate::projectile::ProjectileSpawn;
use crate::rules::MatchRules;
use crate::status::StatusKind;

pub use forms::{FormSpec, KAIJU_LIMIT, Slot, form_spec, forming_secs, lifetime_secs};

/// Replicated animation state (clients pose models with bd_shapes from `state + time since`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AnimState {
    /// Voxels streaming in from the ring.
    #[default]
    Forming = 0,
    Idle = 1,
    /// Anticipation before an attack.
    Windup = 2,
    /// Attack hitbox live.
    Active = 3,
    Recover = 4,
    /// Holding a charge (heavy attack / bow draw).
    Charging = 5,
    /// Moving under its own power (walk / fly / roll / thrown).
    Move = 6,
    /// Telegraph before a dash / drop.
    Telegraph = 7,
    /// Flying back to its owner.
    Returning = 8,
    /// Stuck to something.
    Stuck = 9,
}

impl AnimState {
    pub fn from_u8(v: u8) -> AnimState {
        use AnimState::*;
        match v {
            1 => Idle,
            2 => Windup,
            3 => Active,
            4 => Recover,
            5 => Charging,
            6 => Move,
            7 => Telegraph,
            8 => Returning,
            9 => Stuck,
            _ => Forming,
        }
    }
}

/// Up to 8 ids already hit by the current swing / dash (no allocation).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HitList {
    ids: [u32; 8],
    n: u8,
}

impl HitList {
    pub fn contains(&self, id: u32) -> bool {
        self.ids[..self.n as usize].contains(&id)
    }
    /// Returns false if already present (or full).
    pub fn insert(&mut self, id: u32) -> bool {
        if self.contains(id) || self.n as usize >= self.ids.len() {
            return false;
        }
        self.ids[self.n as usize] = id;
        self.n += 1;
        true
    }
    pub fn clear(&mut self) {
        self.n = 0;
    }
}

/// Key used in a [`HitList`] for players (constructs use their raw id + this offset's complement).
#[inline]
pub(crate) fn player_key(p: PlayerId) -> u32 {
    0xFFFF_FF00 | p.0 as u32
}

/// An explicit voxel model: terrain voxels (0.5 m) lifted out of the island by a graft, keeping
/// their original materials. Coordinates are relative to `origin` (the world voxel the model was
/// cut around); the construct's pivot is the model's bottom-centre.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoxelModel {
    /// World voxel coordinate the local coordinates are relative to.
    pub origin: IVec3,
    pub voxels: Vec<(IVec3, Material)>,
    /// Inclusive local bounds.
    pub min: IVec3,
    pub max: IVec3,
}

impl VoxelModel {
    /// (half extents, local centre relative to the bottom-centre pivot), metres.
    pub fn body_box(&self) -> (Vec3, Vec3) {
        let vs = bd_core::VOXEL_SIZE;
        let size = (self.max - self.min + IVec3::ONE).as_vec3() * vs;
        let half = (size * 0.5).max(Vec3::splat(0.05));
        (half, Vec3::new(0.0, half.y, 0.0))
    }

    /// World position of the pivot (bottom-centre) where the model sat in the terrain.
    pub fn pivot_world(&self) -> Vec3 {
        let vs = bd_core::VOXEL_SIZE;
        let lo = (self.origin + self.min).as_vec3() * vs;
        let hi = (self.origin + self.max + IVec3::ONE).as_vec3() * vs;
        Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5)
    }
}

/// One live construct.
#[derive(Clone, Debug, PartialEq)]
pub struct Construct {
    pub id: ConstructId,
    pub owner: PlayerId,
    /// Validated recipe (grafts merge into it).
    pub recipe: Recipe,
    pub category: FormCategory,
    /// Optional voxel shape (from bd_shapes / icons). The collider is derived from it when present.
    pub shape: Option<Box<ShapeSpec>>,
    /// Explicit voxel model (terrain animated by a graft, COMBAT.md §3c). Overrides `shape`.
    pub model: Option<Arc<VoxelModel>>,
    /// Pivot: grip for held items, feet-centre for creatures and structures, centre for objects.
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    /// Collider half extents (m).
    pub half: Vec3,
    /// Collider centre relative to the pivot, in the construct's local frame.
    pub local_center: Vec3,
    pub integrity: f32,
    pub max_integrity: f32,
    /// Willpower paid (dismiss refund base).
    pub cost: f32,
    /// Remaining forming seconds (0 = active).
    pub forming_t: f32,
    pub anim_state: AnimState,
    /// Tick the current anim state began (clients derive anim time from it).
    pub anim_since: Tick,
    pub age: f32,
    /// Lifetime in seconds (∞ = until dismissed / shattered).
    pub life: f32,
    /// Current autonomous target.
    pub target: Option<PlayerId>,
    /// Seated pilot ([`Mechanic::Piloted`]).
    pub pilot: Option<PlayerId>,
    /// Human-scale piloted construct worn as a power suit (COMBAT.md §3d): follows its pilot, who
    /// keeps normal movement; its integrity absorbs the pilot's damage first.
    pub suit: bool,
    /// Scale ≥ `rules::COLOSSAL_SCALE`.
    pub colossal: bool,
    /// Size multiplier from Shrink/Grow (1 = normal).
    pub size_scale: f32,
    /// Set by behaviours / damage; the world removes it in [`ConstructStore::reap`].
    pub gone: Option<GoneReason>,
    /// Shattered this tick (impulse) — reported by `reap`.
    pub shatter: Option<Vec3>,
    // ---- behaviour scratch (placeholder; the component system will replace these) ----
    pub timer: f32,
    pub charge: f32,
    pub cooldown: f32,
    pub clash_cd: f32,
    pub heavy: bool,
    pub swing_u: f32,
    pub hits: HitList,
    pub bounces: u8,
    pub returning: bool,
    pub stuck_to: Option<PlayerId>,
    pub stuck_offset: Vec3,
    pub grounded: bool,
    pub portal_cd: f32,
    /// Gun state for held guns (the Gun component drives it; replicated as one byte).
    pub weapon: Option<crate::weapons::WeaponState>,
}

impl Construct {
    #[inline]
    pub fn active(&self) -> bool {
        self.forming_t <= 0.0 && self.gone.is_none()
    }

    #[inline]
    pub fn has(&self, m: Mechanic) -> bool {
        self.recipe.has(m)
    }

    /// Held / worn constructs ride their owner (clients derive their transform from the owner).
    #[inline]
    pub fn attached(&self) -> bool {
        self.suit || matches!(self.category, FormCategory::Held | FormCategory::Worn)
    }

    #[inline]
    pub fn center(&self) -> Vec3 {
        self.pos + self.rot * self.local_center
    }

    #[inline]
    pub fn bounds(&self) -> Aabb {
        geom::obb_bounds(self.center(), self.half, self.rot)
    }

    /// Where a pilot's feet sit (top of the collider). The component system may use `part`.
    pub fn seat_pos(&self, _part: u8) -> Vec3 {
        self.center() + self.rot * Vec3::new(0.0, self.half.y, 0.0)
    }

    /// Mass for links / fields / clashes (kg).
    pub fn mass(&self) -> f32 {
        self.recipe.mass.max(1.0) * self.size_scale.powi(3)
    }

    pub fn set_anim(&mut self, s: AnimState, tick: Tick) {
        if self.anim_state != s {
            self.anim_state = s;
            self.anim_since = tick;
        }
    }

    /// Scale the body by `f` (Shrink / Grow on a construct).
    pub fn resize(&mut self, f: f32) {
        if !(f.is_finite() && f > 0.0) {
            return;
        }
        let f = f.clamp(0.25, 4.0);
        self.half *= f;
        self.local_center *= f;
        self.size_scale = (self.size_scale * f).clamp(0.1, 8.0);
    }

    fn collider(&self) -> Option<Collider> {
        if self.gone.is_some() {
            return None;
        }
        if self.suit {
            return None;
        }
        let (solid, anchorable) = match self.category {
            FormCategory::Held | FormCategory::Worn => return None,
            FormCategory::Structure => {
                let solid = self.forming_t <= 0.0 && !matches!(self.recipe.form, Form::Dome | Form::Cage);
                (solid, true)
            }
            FormCategory::Creature => (false, true),
            FormCategory::Object => (false, self.half.max_element() >= 0.4),
        };
        let mut c = Collider::new(self.center(), self.half, self.rot, ColliderId::Construct { id: self.id, part: 0 });
        c.vel = self.vel;
        c.solid = solid;
        c.anchorable = anchorable;
        Some(c)
    }
}

/// What constructs may know about a player this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerView {
    pub id: PlayerId,
    pub alive: bool,
    /// Spawn shield (can't be hurt; constructs may ignore them as targets).
    pub shielded: bool,
    pub pos: Vec3,
    pub vel: Vec3,
    pub hull: Aabb,
    pub eye: Vec3,
    /// Right-hand position (held constructs attach here).
    pub hand: Vec3,
    pub look: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub input: PlayerInput,
    /// Buttons that went down / up this tick.
    pub pressed: u16,
    pub released: u16,
    pub grounded: bool,
    /// Can't act (stunned / reciting / seated elsewhere).
    pub busy: bool,
    pub willpower: f32,
    /// Enemy this player last hit or aimed a cast at (creature targeting).
    pub last_target: Option<PlayerId>,
    pub seat: Option<ConstructId>,
    /// Shrink / Grow body scale (1 = normal).
    pub size: f32,
    /// Rewind (ticks, fractional) for this player's hit tests against others (lag compensation).
    pub view_tick: Option<f64>,
}

/// Read-only world view handed to construct behaviours.
pub struct ConstructCtx<'a> {
    pub tick: Tick,
    pub dt: f32,
    pub players: &'a [PlayerView],
    pub voxels: &'a VoxelWorld,
    /// Last tick's construct colliders (for placement / line of sight).
    pub colliders: &'a [Collider],
    pub params: &'a MoveParams,
    pub history: &'a HullHistory,
    pub rules: &'a MatchRules,
    /// Below this height things fall out of the world.
    pub void_y: f32,
}

impl ConstructCtx<'_> {
    pub fn player(&self, id: PlayerId) -> Option<&PlayerView> {
        self.players.iter().find(|p| p.id == id)
    }

    /// Hull of `target` as `attacker` saw it (lag-compensated when the attacker has a view tick).
    pub fn hull_for(&self, attacker: PlayerId, target: &PlayerView) -> Aabb {
        match self.player(attacker).and_then(|a| a.view_tick) {
            Some(vt) => self.history.hull_at(target.id, vt).unwrap_or(target.hull),
            None => target.hull,
        }
    }

    /// Living, hittable enemies of `owner`.
    pub fn enemies_of(&self, owner: PlayerId) -> impl Iterator<Item = &PlayerView> {
        self.players.iter().filter(move |p| p.id != owner && p.alive && !p.shielded)
    }

    /// Nearest living enemy of `owner` to `pos`.
    pub fn nearest_enemy(&self, owner: PlayerId, pos: Vec3) -> Option<&PlayerView> {
        let mut best: Option<(&PlayerView, f32)> = None;
        for p in self.enemies_of(owner) {
            let d = (p.hull.center() - pos).length_squared();
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((p, d));
            }
        }
        best.map(|b| b.0)
    }
}

/// Intents produced by constructs this tick. The world applies them in field order.
#[derive(Default)]
pub struct ConstructOut {
    pub player_hits: Vec<HitInfo>,
    pub construct_hits: Vec<ConstructHit>,
    pub explosions: Vec<Explosion>,
    pub carves: Vec<CarveRequest>,
    pub projectiles: Vec<ProjectileSpawn>,
    /// Velocity changes for players (trampolines, sword-lock): (player, dv, tumble secs).
    pub impulses: Vec<(PlayerId, Vec3, f32)>,
    /// Gun recoil: (shooter, aim direction, strength m/s) — the world applies it opposite the aim
    /// (×0.35 on the ground), never tumbling.
    pub recoils: Vec<(PlayerId, Vec3, f32)>,
    /// Hitscan beams (railgun), resolved lag-compensated by the world.
    pub hitscans: Vec<crate::weapons::HitscanShot>,
    pub statuses: Vec<(PlayerId, StatusKind, f32, Option<PlayerId>)>,
    /// Extra willpower drain this tick (jetpack…).
    pub drain: Vec<(PlayerId, f32)>,
    /// Pilots to eject: (player, construct, launch velocity).
    pub ejects: Vec<(PlayerId, ConstructId, Vec3)>,
    pub events: Vec<GameEvent>,
}

impl ConstructOut {
    pub fn clear(&mut self) {
        self.player_hits.clear();
        self.construct_hits.clear();
        self.explosions.clear();
        self.carves.clear();
        self.projectiles.clear();
        self.impulses.clear();
        self.recoils.clear();
        self.hitscans.clear();
        self.statuses.clear();
        self.drain.clear();
        self.ejects.clear();
        self.events.clear();
    }
}

/// Everything needed to create a construct (the world fills it in `cast`).
#[derive(Clone, Debug)]
pub struct SpawnParams {
    pub owner: PlayerId,
    pub recipe: Recipe,
    pub shape: Option<ShapeSpec>,
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub target: Option<PlayerId>,
    pub cost: f32,
    pub integrity: f32,
    pub forming: f32,
    pub life: f32,
    pub tick: Tick,
    pub colossal: bool,
    pub model: Option<Arc<VoxelModel>>,
}

/// All live constructs, ordered by id (ids are monotonic, so push keeps the order and iteration is
/// deterministic).
#[derive(Default)]
pub struct ConstructStore {
    items: Vec<Construct>,
    next_id: u32,
}

/// Collider box (half extents, local centre) for a recipe at its clamped scale, or from its shape.
pub fn body_box(recipe: &Recipe, shape: Option<&ShapeSpec>) -> (Vec3, Vec3) {
    if let Some(s) = shape.filter(|s| !s.prims.is_empty()) {
        let k = bd_core::CONSTRUCT_VOXEL_SIZE;
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for p in s.prims.iter().filter(|p| !p.subtract) {
            // Rotation-agnostic: use the prim's bounding sphere-ish box (max extent on all axes when rotated).
            let h = if p.rot_deg == Vec3::ZERO { p.size * 0.5 } else { Vec3::splat(p.size.max_element() * 0.5) };
            lo = lo.min(p.center - h);
            hi = hi.max(p.center + h);
        }
        if lo.is_finite() && hi.is_finite() {
            let half = ((hi - lo) * 0.5 * k).max(Vec3::splat(0.05)).min(Vec3::splat(40.0));
            let center = ((hi + lo) * 0.5 * k).clamp(Vec3::splat(-40.0), Vec3::splat(40.0));
            return (half, center);
        }
    }
    let spec = form_spec(recipe.form);
    let size = spec.dims * recipe.scale;
    let half = (size * 0.5).max(Vec3::splat(0.05));
    let center = match recipe.form.category() {
        FormCategory::Held => Vec3::new(0.0, 0.0, -half.z),
        FormCategory::Creature | FormCategory::Structure => Vec3::new(0.0, half.y, 0.0),
        FormCategory::Worn | FormCategory::Object => Vec3::ZERO,
    };
    (half, center)
}

/// Build a construct from spawn parameters (collider from model / shape / form table).
pub fn build_construct(id: ConstructId, p: SpawnParams) -> Construct {
    let (half, local_center) = match &p.model {
        Some(m) => m.body_box(),
        None => body_box(&p.recipe, p.shape.as_ref()),
    };
    let category = p.recipe.form.category();
    Construct {
        id,
        owner: p.owner,
        category,
        shape: p.shape.map(Box::new),
        model: p.model,
        pos: p.pos,
        prev_pos: p.pos,
        rot: p.rot,
        vel: p.vel,
        half,
        local_center,
        integrity: p.integrity,
        max_integrity: p.integrity,
        cost: p.cost,
        forming_t: p.forming,
        anim_state: AnimState::Forming,
        anim_since: p.tick,
        age: 0.0,
        life: p.life,
        target: p.target,
        pilot: None,
        suit: false,
        colossal: p.colossal,
        size_scale: 1.0,
        gone: None,
        shatter: None,
        timer: 0.0,
        charge: 0.0,
        cooldown: 0.0,
        clash_cd: 0.0,
        heavy: false,
        swing_u: 0.0,
        hits: HitList::default(),
        bounces: 0,
        returning: false,
        stuck_to: None,
        stuck_offset: Vec3::ZERO,
        grounded: false,
        portal_cd: 0.0,
        weapon: if category == FormCategory::Held {
            crate::weapons::archetype_for(&p.recipe).map(|a| crate::weapons::WeaponState::new(a, &crate::weapons::spec_for(&p.recipe, a)))
        } else {
            None
        },
        recipe: p.recipe,
    }
}

impl ConstructStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Construct> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Construct> {
        self.items.iter_mut()
    }

    pub fn as_slice(&self) -> &[Construct] {
        &self.items
    }

    fn index(&self, id: ConstructId) -> Option<usize> {
        self.items.binary_search_by_key(&id.0, |c| c.id.0).ok()
    }

    pub fn get(&self, id: ConstructId) -> Option<&Construct> {
        self.index(id).map(|i| &self.items[i])
    }

    pub fn get_mut(&mut self, id: ConstructId) -> Option<&mut Construct> {
        self.index(id).map(move |i| &mut self.items[i])
    }

    pub fn next_id(&self) -> u32 {
        self.next_id
    }

    /// Create a construct. Returns its id.
    pub fn spawn(&mut self, p: SpawnParams) -> ConstructId {
        let id = ConstructId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let c = build_construct(id, p);
        // Ids are monotonic, but guard against wrap-around keeping the vector sorted.
        match self.items.last() {
            Some(last) if last.id.0 >= id.0 => {
                let at = self.items.partition_point(|c| c.id.0 < id.0);
                self.items.insert(at, c);
            }
            _ => self.items.push(c),
        }
        id
    }

    /// Client mirror: create (or replace) a construct with the host's id.
    pub fn spawn_with_id(&mut self, id: ConstructId, p: SpawnParams) {
        self.insert_mirror(build_construct(id, p));
    }

    /// Insert a construct with a known id (client mirror from snapshots/events).
    pub fn insert_mirror(&mut self, c: Construct) {
        match self.items.binary_search_by_key(&c.id.0, |x| x.id.0) {
            Ok(i) => self.items[i] = c,
            Err(i) => self.items.insert(i, c),
        }
        self.next_id = self.next_id.max(self.items.last().map_or(0, |l| l.id.0.wrapping_add(1)));
    }

    /// Remove immediately (no event). Prefer marking `gone` + [`Self::reap`] during a tick.
    pub fn remove(&mut self, id: ConstructId) -> Option<Construct> {
        self.index(id).map(|i| self.items.remove(i))
    }

    pub fn owned_by(&self, owner: PlayerId) -> impl Iterator<Item = &Construct> {
        self.items.iter().filter(move |c| c.owner == owner && c.gone.is_none())
    }

    /// Live constructs of `owner` in `slot`, oldest first.
    pub fn in_slot(&self, owner: PlayerId, slot: Slot) -> impl Iterator<Item = &Construct> {
        self.owned_by(owner).filter(move |c| Slot::of(c.recipe.form) == slot)
    }

    /// The owner's held construct, if any.
    pub fn held_by(&self, owner: PlayerId) -> Option<&Construct> {
        self.in_slot(owner, Slot::Held).next()
    }

    /// Merge a grafted phrase into construct `id`: mechanics and features are unioned, a non-Light
    /// element replaces the old one, a payload is added/replaced, temperament taken if set.
    /// PLACEHOLDER semantics — the component system decides what grafting a form means.
    pub fn graft(&mut self, id: ConstructId, add: &Recipe) -> Option<&Recipe> {
        let c = self.get_mut(id)?;
        let r = &mut c.recipe;
        r.mechanics.extend_from_slice(&add.mechanics);
        r.mechanics.sort();
        r.mechanics.dedup();
        r.mechanics.truncate(crate::willpower::MAX_MECHANICS);
        r.features.extend_from_slice(&add.features);
        r.features.sort();
        r.features.dedup();
        r.features.truncate(crate::willpower::MAX_FEATURES);
        if add.element != bd_core::mechanics::Element::Light {
            r.element = add.element;
        }
        if add.payload.is_some() {
            r.payload = add.payload.clone();
        }
        if add.temperament != bd_core::mechanics::Temperament::Calm {
            r.temperament = add.temperament;
            r.intensity = add.intensity;
        }
        Some(&c.recipe)
    }

    /// Movement modifiers from the owner's worn / held constructs (wings, parachute, jetpack,
    /// umbrella…). Called before `step_player`; `mods` already holds status effects.
    pub fn movement_mods(&self, owner: PlayerId, input: &PlayerInput, st: &PlayerState, mods: &mut MoveModifiers) {
        placeholder::movement_mods(self, owner, input, st, mods);
    }

    /// After a player moved: trampolines, launch pads, parachute tumble-cancel.
    pub fn post_move(&self, pid: PlayerId, input: &PlayerInput, st: &mut PlayerState, pre_vel: Vec3, ev: &MoveEvents, p: &MoveParams) {
        placeholder::post_move(self, pid, input, st, pre_vel, ev, p);
    }

    /// Run every construct's behaviour for one tick.
    pub fn update(&mut self, ctx: &ConstructCtx, out: &mut ConstructOut) {
        placeholder::update(&mut self.items, ctx, out);
    }

    /// Solid / anchorable boxes for movement and tethers.
    pub fn write_colliders(&self, out: &mut Vec<Collider>) {
        for c in &self.items {
            if let Some(col) = c.collider() {
                out.push(col);
            }
        }
    }

    /// Earliest active construct hit by the swept sphere `p0 → p1` (radius `r`), skipping those
    /// owned by `skip_owner`. Returns (id, fraction, normal).
    pub fn segment_query(&self, p0: Vec3, p1: Vec3, r: f32, skip_owner: Option<PlayerId>) -> Option<(ConstructId, f32, Vec3)> {
        let mut best: Option<(ConstructId, f32, Vec3)> = None;
        let seg = Aabb { min: p0.min(p1) - Vec3::splat(r), max: p0.max(p1) + Vec3::splat(r) };
        for c in &self.items {
            if !c.active() || Some(c.owner) == skip_owner || c.attached() {
                continue;
            }
            if !c.bounds().intersects(&seg) {
                continue;
            }
            if let Some((t, n)) = geom::segment_obb(p0, p1, c.center(), c.half, c.rot, r) {
                if best.is_none_or(|b| t < b.1) {
                    best = Some((c.id, t, n));
                }
            }
        }
        best
    }

    /// Apply integrity damage. Marks the construct shattered at 0 (reported by [`Self::reap`]).
    /// Returns true if it shattered.
    pub fn damage(&mut self, hit: &ConstructHit, events: &mut Vec<GameEvent>) -> bool {
        let Some(c) = self.get_mut(hit.target) else { return false };
        if c.gone.is_some() || !(hit.damage.is_finite()) {
            return false;
        }
        let dmg = hit.damage.max(0.0) * if hit.blade { 2.0 } else { 1.0 };
        c.integrity -= dmg;
        if hit.impulse.is_finite() && !c.attached() && c.category != FormCategory::Structure {
            c.vel += hit.impulse / c.mass().max(1.0).sqrt().max(1.0);
        }
        events.push(GameEvent::ConstructHit { id: c.id, source: hit.source, damage: dmg, point: hit.point });
        if c.integrity <= 0.0 {
            c.integrity = 0.0;
            c.gone = Some(GoneReason::Spent);
            c.shatter = Some(if hit.impulse.is_finite() { hit.impulse } else { Vec3::ZERO });
            return true;
        }
        false
    }

    /// Lose integrity (starvation at 0 willpower). Shatters at 0.
    pub fn decay_owner(&mut self, owner: PlayerId, frac_per_sec: f32, dt: f32) {
        for c in self.items.iter_mut().filter(|c| c.owner == owner && c.gone.is_none()) {
            c.integrity -= c.max_integrity * frac_per_sec * dt;
            if c.integrity <= 0.0 {
                c.integrity = 0.0;
                c.gone = Some(GoneReason::Spent);
                c.shatter = Some(Vec3::ZERO);
            }
        }
    }

    /// Mark every construct of `owner` as gone.
    pub fn mark_owner_gone(&mut self, owner: PlayerId, reason: GoneReason) {
        for c in self.items.iter_mut().filter(|c| c.owner == owner && c.gone.is_none()) {
            c.gone = Some(reason);
        }
    }

    /// Remove every construct marked gone, emitting `ConstructShattered` / `ConstructGone` and
    /// reporting seated pilots that must be ejected (into `ejects`: player, construct, velocity).
    /// `refund(owner, amount)` receives dismiss refunds.
    pub fn reap(&mut self, events: &mut Vec<GameEvent>, ejects: &mut Vec<(PlayerId, ConstructId, Vec3)>, mut refund: impl FnMut(PlayerId, f32)) {
        self.items.retain(|c| {
            let Some(reason) = c.gone else { return true };
            if let Some(p) = c.pilot {
                // Shattered → pilot ejected tumbling; otherwise a gentle hop out.
                let v = if c.shatter.is_some() { c.vel + Vec3::new(0.0, 12.0, 0.0) } else { c.vel + Vec3::new(0.0, 6.0, 0.0) };
                ejects.push((p, c.id, v));
            }
            if let Some(imp) = c.shatter {
                events.push(GameEvent::ConstructShattered { id: c.id, impulse: imp, pos: c.center() });
            } else {
                let amount = if reason == GoneReason::Dismissed { c.cost * crate::willpower::DISMISS_REFUND } else { 0.0 };
                if amount > 0.0 {
                    refund(c.owner, amount);
                }
                events.push(GameEvent::ConstructGone { id: c.id, reason, refund: amount });
            }
            false
        });
    }

    /// Drop everything (match reset). No events.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}
