//! Constructs: the store, the per-construct state, the interface the world talks to, and the
//! **component pipeline** (COMBAT.md §5b) that every behaviour plugs into.
//!
//! # World contract (unchanged)
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
//!
//! # Module ownership (three engineers work in this directory in parallel)
//!
//! | file | owner | what |
//! |---|---|---|
//! | `mod.rs` | construct (framework) | store, [`Construct`], pipeline, dispatch, [`Env`], [`Body`] |
//! | `body.rs` | construct | generic rigid-body physics: gravity, voxels, construct-vs-construct mass pushes |
//! | `components.rs` | construct | one generic component per [`Mechanic`] (works on ANY form) |
//! | `structure.rs` / `object.rs` | construct | Structure / Object form controllers |
//! | `graft.rs` | construct | recipe merging + §3c animated terrain controller |
//! | `chemistry.rs` / `counters.rs` | construct | §9 element chemistry, §8 counters, clash |
//! | `held.rs` / `worn.rs` / `pilot.rs` / `gun.rs` | held | wielded, worn, piloted, suits, guns |
//! | `creature.rs` / `ai.rs` / `temperament.rs` / `swarm.rs` | creature | autonomous creatures |
//!
//! Each owner keeps *their* per-construct state in a struct defined in their own file and embedded
//! in [`Construct`] (`c.held: held::HeldState`, `c.ai: ai::AiState`, `c.comp:
//! components::CompState`), so nobody has to edit someone else's struct. They must stay
//! `Clone + Debug + Default + PartialEq`, fixed-size (no heap: no allocation per tick).
//! The legacy scratch fields (`timer`, `charge`, `cooldown`, `heavy`, `swing_u`, `hits`) belong to
//! whichever **controller** runs the construct (at most one does).
//!
//! # The component pipeline (per tick, per construct, in id order)
//!
//! 1. **Housekeeping** ([`housekeeping`]): `prev_pos`, age, cooldowns, owner lookup (missing owner ⇒
//!    gone), forming countdown, lifetime expiry (death triggers such as Explodes fire here via
//!    [`components::on_expire`]).
//! 2. **Controller** — exactly one per construct, chosen by [`controller_of`] from form category and
//!    state (see [`Controller`]); looked up in the function table [`behaviour`]. Before it runs the
//!    framework resets `c.body` for the tick ([`Body::begin_tick`]).
//! 3. **Drivers by precedence** (§5b): `Attached > Hold (Grab/Constrict while holding) > Charge
//!    (while dashing) > Steer (Homing/Flies/Thrust/Walks/creature locomotion/thrown flight) >
//!    Physics`. [`components::drive`] runs *before* the controller and may claim `Hold`/`Charge`
//!    via [`Body::claim`]; a controller or component only writes `c.vel` for motion if
//!    [`Body::claim`] with its level succeeds (returns false when something higher already
//!    drives). Attached controllers claim `Attached`. After the controller,
//!    [`components::steer`] adds the steering components (Homing, Flies, Thrust, Returning, Walks,
//!    SlowFall/Glide on free bodies) — they refine or claim `Steer`.
//! 4. **Physics** ([`body::integrate`]): according to `c.body.loco` ([`Loco`]): gravity, voxel
//!    collision (swept, sliding), ground following for walkers, construct-vs-construct contacts
//!    using the OBB proxies of every other construct (mass-weighted pushes, stacking, toppling),
//!    player contacts. `Loco::Kinematic` = the controller placed `pos` itself (held items, stuck
//!    things) — physics only records contacts. All contacts land in `c.body.contacts`.
//! 5. **Component reactions** ([`components::react`]): contact events → Explodes / Sticky /
//!    Ricochet / Bounce / Returning / Grab / Constrict / Pull / Blade / Blunt / Heavy / Spin / Trap…;
//!    periodic emitters (Shoots / Beam / Breath with depth-1 `payload` recipes), Stomp, ArticulatedTail.
//! 6. After all constructs: the cross-construct **queue** (`out.queue`, [`Interaction`]) is applied
//!    (pushes, attachments, slices, splits, spawns), then [`chemistry::step`] (§9) and
//!    [`counters::clash`] (§3 clash + §8 qualitative counters).
//!
//! Everything a behaviour needs is in [`Env`]: the read-only [`ConstructCtx`], the owner's
//! [`PlayerView`], and `others`: this tick's [`Proxy`] snapshot of every construct (taken before
//! stepping, so iteration order never matters).
//!
//! # Rules for behaviour code
//!
//! * Deterministic: iterate slices in order, no hash maps, no wall clock, randomness only from
//!   [`crate::rng`] seeded from ids/ticks.
//! * No allocation per tick in steady state (push into the reused `out` vectors only).
//! * No panics (the crate denies `unwrap`/`expect`); guard every division and `normalize`.
//! * Cross-construct effects go through `out.queue` — never index `others` mutably.
//!
//! # Transforms (COMBAT.md §5d-bis) — shared hooks
//!
//! The generic two-mode machine lives in `components.rs` (`c.comp.xform`: `mode` Alt/Robot,
//! `progress` 0 = alt … 1 = robot over `XFORM_SECS` ≈ 0.9 s, `busy` while transforming,
//! `combined` for a combiner giant). The framework runs it for every non-attached construct:
//! * **Toggle**: [`components::request_transform`] — held engineer: call it from the pilot/suit
//!   transform key (Mouse3 / V-tap); "transform" grafts and "roll out" (`graft.rs` commands) call
//!   it too. While `busy`, `components::drive` claims `Driver::Hold` (no steering; momentum kept):
//!   a vehicle → robot transform in mid-air arms a **landing slam** (Stomp scaled by fall speed),
//!   fired in `components::react` when the body is grounded (`c.grounded || c.body.supported`, so
//!   pilot controllers must keep `c.grounded` up to date).
//! * **Autonomous** (no pilot): drive fast toward the target in alt mode, transform ~12 m out,
//!   fight in robot mode (punches; emitters only fire in robot mode), transform back beyond 25 m.
//!   Generic bodies (objects / structures / terrain) get this locomotion from `components`;
//!   **creature engineer**: creature-controlled transformers get the *mode machine* only — read
//!   `c.comp.xform.mode` for the gait (Alt = the form's own gait / vehicle drive, Robot = biped)
//!   and `progress`/`busy` to pause locomotion while panels fold.
//! * The collision box follows the silhouette (alt lies long, robot stands up).
//! * **Combiners**: "combine" / "devastator" with ≥ 3 owned transformers → `components::start_combine`
//!   (one per player, ~2 s: limbs fly to the torso, then one Kaiju-scale robot, `xform.combined`).
//! * Replication gap (needs a world/replicate owner): mode/progress aren't in snapshots yet; the
//!   client can only infer them from `AnimState::Telegraph` + `anim_since`.
//!
//! # Performance budget (hard gate — owner's standing order: SNAPPY above all)
//!
//! The WHOLE world tick must stay at soak p99 < 1 ms and max < 4 ms (quiet machine) with every
//! component active; construct code shares that budget. Measure with
//! `cargo run --release -p bd_sim --example soak -- --seed 1 --ticks 200000 --stats target/soak.json`
//! (`SOAK_TRACE=1 … --budget-ms 1.5` prints slow ticks with their events).
//! * Per construct per tick: O(constructs) proxy box tests at most, a handful of *bounded* voxel
//!   probes (column scans ≤ 96 cells, ≤ 2–3 raycasts, `ai::steer_ground` fans only every few
//!   ticks), no pathfinding, no per-voxel collision — collide via proxies (`Proxy` OBBs) only.
//! * Anything periodic (retargeting, steering probes, emitters, AI decisions) runs on cooldowns,
//!   not every tick. Chemistry runs at 10/20 Hz with fixed caps; terrain changes go through
//!   `out.carves` (the world paces them).
//! * No allocation per tick: fixed-size arrays in your state structs, push only into the reused
//!   `out` vectors. Spawning children (`Interaction::Spawn/Split`) is rare and capped.

pub mod ai;
pub mod body;
pub mod chemistry;
pub mod components;
pub mod counters;
pub mod creature;
pub mod extend; // WUKONG: telescoping weapons (Mechanic::Extends, bd_core v1.8)
pub mod forms;
pub mod giant; // GIANTS: weak points, limbs, topple, rideable colliders, shockwaves, command
pub mod graft;
pub mod gun;
pub mod held;
pub mod legend; // LEGENDARY: generic legendary move rules (recall, storm call, ricochet chain, deflect, blade wave, heavy blade, chain blades)
pub mod object;
pub mod pilot;
pub mod structure;
pub mod swarm;
pub mod temperament;
pub mod titan; // GIANTS: piloted walkers move like Titanfall titans (pilot state = body, predicted)
pub mod worn;

use bd_core::input::PlayerInput;
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::voxel::Aabb;
use bd_core::voxel::Material;
use bd_core::IVec3;
use bd_core::{ConstructId, PlayerId, Quat, Tick, Vec3};
use bd_voxel::VoxelWorld;
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

/// Key used in a [`HitList`] for players (constructs use their raw id, which never reaches this
/// range in practice).
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

// =====================================================================================
// Framework types: body, contacts, drivers, proxies, environment, interactions
// =====================================================================================

/// Motion precedence (COMBAT.md §5b). Higher wins; see [`Body::claim`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Driver {
    /// Nobody steers: gravity, collisions, momentum.
    #[default]
    Physics = 0,
    /// Homing / Flies / Thrust / Walks / creature locomotion / thrown flight.
    Steer = 1,
    /// A Charge dash in progress.
    Charge = 2,
    /// Grab / Constrict carrying something (or being carried: see [`CompState`](components::CompState)).
    Hold = 3,
    /// Held / worn / suit / piloted: the body follows a player.
    Attached = 4,
}

/// How [`body::integrate`] moves the body this tick. Controllers set it (default `Ballistic`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Loco {
    /// Free rigid body: gravity, swept voxel collision with restitution/friction, rests on ground.
    #[default]
    Ballistic,
    /// Placed structure: does not move unless pushed/toppled (then it becomes Ballistic).
    Static,
    /// Ground walker: gravity + ground following with step-up (`c.grounded` maintained).
    Walk,
    /// Self-propelled flier: no gravity, stops against terrain.
    Fly,
    /// The controller already set `pos` (held/worn/stuck/carried). Physics only records contacts.
    Kinematic,
}

/// What a contact touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContactWith {
    Terrain,
    Player(PlayerId),
    Construct(ConstructId),
}

/// One contact recorded by physics this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub with: ContactWith,
    /// World contact point.
    pub point: Vec3,
    /// Unit normal pointing from the other thing toward this construct.
    pub normal: Vec3,
    /// Closing speed along the normal just before the contact (≥ 0).
    pub speed: f32,
    /// This construct's velocity just before the contact.
    pub vel: Vec3,
}

impl Default for Contact {
    fn default() -> Self {
        Contact { with: ContactWith::Terrain, point: Vec3::ZERO, normal: Vec3::Y, speed: 0.0, vel: Vec3::ZERO }
    }
}

/// Max contacts remembered per tick (extra ones are dropped; the strongest are kept).
pub const MAX_CONTACTS: usize = 6;

/// Fixed-capacity contact list (no allocation).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Contacts {
    items: [Contact; MAX_CONTACTS],
    n: u8,
}

impl Contacts {
    pub fn clear(&mut self) {
        self.n = 0;
    }
    pub fn len(&self) -> usize {
        self.n as usize
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    pub fn as_slice(&self) -> &[Contact] {
        &self.items[..self.n as usize]
    }
    /// Add a contact; when full, replaces the weakest if this one is stronger. One entry per
    /// touched thing (the strongest).
    pub fn push(&mut self, c: Contact) {
        let n = self.n as usize;
        if let Some(e) = self.items[..n].iter_mut().find(|e| e.with == c.with) {
            if c.speed > e.speed {
                *e = c;
            }
            return;
        }
        if n < MAX_CONTACTS {
            self.items[n] = c;
            self.n += 1;
            return;
        }
        let mut weakest = 0;
        for i in 1..n {
            if self.items[i].speed < self.items[weakest].speed {
                weakest = i;
            }
        }
        if c.speed > self.items[weakest].speed {
            self.items[weakest] = c;
        }
    }
    /// First contact with a player, if any.
    pub fn player(&self) -> Option<(PlayerId, &Contact)> {
        self.as_slice().iter().find_map(|c| match c.with {
            ContactWith::Player(p) => Some((p, c)),
            _ => None,
        })
    }
    /// Strongest contact of any kind.
    pub fn strongest(&self) -> Option<&Contact> {
        self.as_slice().iter().max_by(|a, b| a.speed.total_cmp(&b.speed))
    }
}

/// Framework-owned physics state of a construct (reset per tick by [`Body::begin_tick`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Who drives motion this tick (highest claim wins).
    pub driver: Driver,
    pub loco: Loco,
    /// Contacts found by physics this tick.
    pub contacts: Contacts,
    /// Gravity multiplier for Ballistic / Walk (SlowFall/Glide lower it).
    pub gravity: f32,
    /// Bounciness vs terrain/constructs (0 = thud, 1 = elastic; Bounce > 1).
    pub restitution: f32,
    /// Standing on terrain or another construct this tick.
    pub supported: bool,
    /// A structure knocked loose (pushed hard / sliced / undermined): it now falls and tips over.
    pub loose: bool,
    /// Angular velocity (rad/s, world) for tumbling / toppling bodies.
    pub spin: Vec3,
}

impl Default for Body {
    fn default() -> Self {
        Body {
            driver: Driver::Physics,
            loco: Loco::Ballistic,
            contacts: Contacts::default(),
            gravity: 1.0,
            restitution: 0.3,
            supported: false,
            loose: false,
            spin: Vec3::ZERO,
        }
    }
}

impl Body {
    /// Start-of-tick reset (keeps `loose`, `spin`, `restitution`, `gravity`, `loco`).
    pub fn begin_tick(&mut self) {
        self.driver = Driver::Physics;
        self.contacts.clear();
    }
    /// Claim motion at `level`. Returns true if nothing of higher precedence drives this tick
    /// (then the caller may write `c.vel`/`c.pos`).
    #[inline]
    pub fn claim(&mut self, level: Driver) -> bool {
        if level >= self.driver {
            self.driver = level;
            true
        } else {
            false
        }
    }
    /// Would a claim at `level` succeed (without claiming)?
    #[inline]
    pub fn free(&self, level: Driver) -> bool {
        level >= self.driver
    }
}

/// Bitset of a recipe's mechanics (fast `has` for proxies).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MechSet(pub u64);

impl MechSet {
    pub fn of(r: &Recipe) -> MechSet {
        let mut b = 0u64;
        for m in &r.mechanics {
            b |= 1u64 << (*m as u32 & 63);
        }
        MechSet(b)
    }
    #[inline]
    pub fn has(self, m: Mechanic) -> bool {
        self.0 & (1u64 << (m as u32 & 63)) != 0
    }
}

/// Read-only per-tick snapshot of a construct, for cross-construct queries (collision, targeting,
/// counters). Taken before any construct steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Proxy {
    pub id: ConstructId,
    pub owner: PlayerId,
    pub category: FormCategory,
    pub form: Form,
    pub mech: MechSet,
    pub element: Element,
    pub center: Vec3,
    pub half: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub mass: f32,
    /// Formed and not gone.
    pub active: bool,
    /// Rides a player (held / worn / suit): not a free body.
    pub attached: bool,
    /// Solid for other constructs to stand on / bump into.
    pub solid: bool,
    /// Moves under contact impulses (free body); false = immovable (placed structures, attached).
    pub free: bool,
    /// Riding / held by this construct.
    pub attached_to: Option<ConstructId>,
    /// Child of (swarm copy, payload round, slice half).
    pub parent: Option<ConstructId>,
    pub anim: AnimState,
    /// Frozen solid (brittle) — chemistry.
    pub frozen: bool,
    /// On fire — chemistry.
    pub burning: bool,
    /// GIANTS: a giant (its own weak points; `center` / `half` are its whole-body hit box).
    pub giant: bool,
}

impl Proxy {
    pub fn of(c: &Construct) -> Proxy {
        let (center, half) = c.hit_box();
        Proxy {
            id: c.id,
            owner: c.owner,
            category: c.category,
            form: c.recipe.form,
            mech: c.comp.eff,
            element: c.recipe.element,
            center,
            half,
            rot: c.rot,
            vel: c.vel,
            mass: c.mass(),
            active: c.active(),
            attached: c.attached(),
            solid: c.active() && !c.attached() && !matches!(c.recipe.form, Form::Dome | Form::Cage),
            free: body::is_free(c),
            attached_to: c.comp.attached_to,
            parent: c.parent,
            anim: c.anim_state,
            frozen: c.comp.frozen > 0.0,
            burning: c.comp.burning > 0.0,
            giant: c.ai.giant.on,
        }
    }
    #[inline]
    pub fn has(&self, m: Mechanic) -> bool {
        self.mech.has(m)
    }
    pub fn bounds(&self) -> Aabb {
        geom::obb_bounds(self.center, self.half, self.rot)
    }
}

/// What a behaviour sees while stepping one construct.
pub struct Env<'e> {
    pub ctx: &'e ConstructCtx<'e>,
    /// The construct's owner (always present: constructs of departed owners never step).
    pub owner: PlayerView,
    /// Every construct as it was at the start of this update (including the one stepping).
    pub others: &'e [Proxy],
}

impl Env<'_> {
    #[inline]
    pub fn dt(&self) -> f32 {
        self.ctx.dt
    }
    #[inline]
    pub fn tick(&self) -> Tick {
        self.ctx.tick
    }
    pub fn proxy(&self, id: ConstructId) -> Option<&Proxy> {
        self.others.binary_search_by_key(&id.0, |p| p.id.0).ok().map(|i| &self.others[i])
    }
    /// The autonomous target for `c` (creature engineer's targeting rules: owner's last aim/hit,
    /// else nearest; temperaments may change it). Every autonomous component uses this.
    pub fn target_for(&self, c: &Construct) -> Option<PlayerView> {
        ai::pick_target(c, self)
    }
}

/// How one construct attaches to another (queued, applied after the pass).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttachKind {
    /// Sticky: rides the other thing.
    Stick,
    /// Grab / Constrict / Trap: the *holder* carries the held construct.
    Hold,
}

/// Cross-construct effects queued during the pass (`out.queue`) and applied after every construct
/// stepped, in queue order (deterministic). The world never sees these.
#[derive(Clone, Debug, PartialEq)]
pub enum Interaction {
    /// Change construct `id`'s velocity (mass-weighted contact pushes, rams, launches).
    Push { id: ConstructId, dv: Vec3 },
    /// Knock a structure loose (it starts falling / toppling) with an angular kick.
    Loosen { id: ConstructId, spin: Vec3 },
    /// `id` attaches to `to` at `offset` (in `to`'s local frame).
    Attach { id: ConstructId, to: ConstructId, offset: Vec3, kind: AttachKind },
    /// `holder` releases whatever it holds with velocity `vel` (throws).
    Release { holder: ConstructId, vel: Vec3 },
    /// Slice `id` in two along the plane through `point` with normal `normal` (Blade counter).
    Slice { id: ConstructId, point: Vec3, normal: Vec3, by: Option<PlayerId> },
    /// Chemistry on a construct: freeze (secs), ignite (secs), douse.
    Freeze { id: ConstructId, secs: f32 },
    Ignite { id: ConstructId, secs: f32 },
    Douse { id: ConstructId },
    /// Root a (creature) construct in place for `secs` (cage / trap counter); `pinned` = slammed
    /// down / toppled (fliers fall, kaiju tip over).
    Root { id: ConstructId, secs: f32, pinned: bool, by: ConstructId },
    /// Shrink / Grow a construct.
    Resize { id: ConstructId, factor: f32 },
    /// Set Leaves / Wood voxels alight around `pos` (chemistry, §9).
    IgniteTerrain { pos: Vec3, radius: f32, by: Option<PlayerId> },
    /// Split `id` into `count` independent copies at `scale` × size (Swarm).
    Split { id: ConstructId, count: u8, scale: f32 },
    /// Spawn a brand-new child construct (payload emitters, debris halves). `parent` children do
    /// not count toward slot limits.
    Spawn { parent: ConstructId, params: Box<SpawnParams> },
}

/// Chooses the one controller that runs a construct (the form's default control).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Controller {
    /// held.rs (+ gun.rs): rides the owner's hand, LMB/RMB.
    Held = 0,
    /// worn.rs: rides the owner's back.
    Worn = 1,
    /// pilot.rs: a human-scale power suit around its pilot.
    Suit = 2,
    /// pilot.rs: a seated pilot drives it.
    Piloted = 3,
    /// creature.rs (+ ai / temperament / swarm): autonomous creature.
    Creature = 4,
    /// structure.rs: placed structure.
    Structure = 5,
    /// object.rs: thrown / dropped / launched object.
    Object = 6,
    /// graft.rs: animated terrain piece (§3c).
    Terrain = 7,
    /// components.rs: the generic walker for non-creatures with `Walks` ("wall with legs").
    Walker = 8,
}

/// Per-tick controller entry point: form default control. Must respect [`Body::claim`] for motion.
pub type StepFn = fn(&mut Construct, &Env<'_>, &mut ConstructOut);

/// The function table other modules implement (one row per [`Controller`]).
pub struct Behaviour {
    pub name: &'static str,
    pub step: StepFn,
}

static BEHAVIOURS: [Behaviour; 9] = [
    Behaviour { name: "held", step: held::step },
    Behaviour { name: "worn", step: worn::step },
    Behaviour { name: "suit", step: pilot::step_suit },
    Behaviour { name: "piloted", step: pilot::step_piloted },
    Behaviour { name: "creature", step: creature::step },
    Behaviour { name: "structure", step: structure::step },
    Behaviour { name: "object", step: object::step },
    Behaviour { name: "terrain", step: graft::step_terrain },
    Behaviour { name: "walker", step: components::step_walker },
];

/// The behaviour row for a controller.
#[inline]
pub fn behaviour(c: Controller) -> &'static Behaviour {
    &BEHAVIOURS[c as usize]
}

/// Which controller runs `c` this tick.
pub fn controller_of(c: &Construct) -> Controller {
    if c.suit {
        return Controller::Suit;
    }
    match c.category {
        FormCategory::Held if c.free_flier() => return Controller::Object,
        FormCategory::Held => return Controller::Held,
        FormCategory::Worn => return Controller::Worn,
        _ => {}
    }
    if c.pilot.is_some() {
        return Controller::Piloted;
    }
    if c.model.is_some() {
        return Controller::Terrain;
    }
    match c.category {
        FormCategory::Creature => Controller::Creature,
        FormCategory::Structure | FormCategory::Object if c.has(Mechanic::Walks) => Controller::Walker,
        FormCategory::Structure => Controller::Structure,
        _ => Controller::Object,
    }
}

// =====================================================================================
// Construct
// =====================================================================================

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
    /// TUNING HOOK: priced as an autonomous non-creature (creature upkeep; decided on the typed
    /// recipe at cast / by a typed graft — see `tuning::WillpowerTuning::autonomous_pricing`).
    pub priced_autonomous: bool,
    /// Scale ≥ `rules::COLOSSAL_SCALE`.
    pub colossal: bool,
    /// Size multiplier from Shrink/Grow (1 = normal).
    pub size_scale: f32,
    /// FREEDOM HOOK: presentation bits `replicate::cflags::LEGENDARY | SPUTTER` (`crate::freedom`).
    pub flair: u8,
    /// Set by behaviours / damage; the world removes it in [`ConstructStore::reap`].
    pub gone: Option<GoneReason>,
    /// Shattered this tick (impulse) — reported by `reap`.
    pub shatter: Option<Vec3>,
    /// A child made by the construct system (swarm copy, slice half, emitted payload). Children do
    /// not count toward the owner's slot limits.
    pub parent: Option<ConstructId>,
    // ---- framework / component state ----
    /// Physics state (framework).
    pub body: Body,
    /// Generic mechanic components' state (components.rs).
    pub comp: components::CompState,
    /// Held / worn / pilot controllers' state (held.rs).
    pub held: held::HeldState,
    /// Creature controllers' state (ai.rs).
    pub ai: ai::AiState,
    // ---- controller scratch (belongs to whichever controller runs the construct) ----
    pub timer: f32,
    pub charge: f32,
    pub cooldown: f32,
    pub clash_cd: f32,
    pub heavy: bool,
    pub swing_u: f32,
    pub hits: HitList,
    pub bounces: u8,
    pub returning: bool,
    /// Stuck to a player (Sticky). Constructs stuck to constructs use `comp.attached_to`.
    pub stuck_to: Option<PlayerId>,
    pub stuck_offset: Vec3,
    pub grounded: bool,
    pub portal_cd: f32,
    /// Gun state for held guns (the Gun component drives it; replicated as one byte).
    pub weapon: Option<crate::weapons::WeaponState>,
    /// A slice half: which part of the parent's model this is (clients clip the parent's model).
    pub slice: Option<SliceCut>,
    /// World bookkeeping: last transformer state announced (`Transformed` events).
    pub xform_seen: u8,
    /// Two-mode icons: the hand-authored robot shape (`shape` is the alt / vehicle). Clients rig
    /// the transformation with `bd_shapes::transform_rig_authored(shape, alt_shape, cap)`.
    pub alt_shape: Option<Box<ShapeSpec>>,
    /// With `alt_shape`: the collision boxes (half, local centre) of [alt, robot] — the Transforms
    /// component swaps between them instead of guessing the robot silhouette.
    pub authored_boxes: Option<[(Vec3, Vec3); 2]>,
}

/// Where a slice half was cut from its parent (all in the parent's local frame, metres).
/// Clients build the parent's model (`parent_scale`, same recipe otherwise) and keep the part on
/// `side` of the plane `local[axis] = cut`, then place it with this half's transform
/// (`offset` = parent-local centre of the half, subtract it from the kept part).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SliceCut {
    pub parent: ConstructId,
    /// 0 = local X, 2 = local Z.
    pub axis: u8,
    /// −1 keeps `local < cut`, +1 keeps `local > cut`.
    pub side: i8,
    pub cut: f32,
    /// The parent's collider half extents / scale before the cut.
    pub parent_half: Vec3,
    pub parent_scale: f32,
    /// The half's centre in the parent's local frame (relative to the parent's collider centre).
    pub offset: Vec3,
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
    /// HELD ENGINEER EDIT: a thrown held item (Returning / axe throw / Thor flight, `held.thrown`)
    /// is a free body while out of the hand (its transform is replicated).
    #[inline]
    pub fn attached(&self) -> bool {
        self.suit || (matches!(self.category, FormCategory::Held | FormCategory::Worn) && !self.held.thrown && !self.free_flier())
    }

    /// CONSTRUCT ENGINEER: a held form that `Flies` without `Returning` ("a flying hammer", §0)
    /// leaves the hand and hunts on its own. It runs the Object controller + generic components
    /// (a free body, not attached) and still occupies the owner's Held slot. Flies + Returning
    /// stays with the held controller (Mjolnir / Thor flight). FREEDOM HOOK: so does a held form that
    /// `DropFromSky` without `Returning` — a volley ("rain of arrows", "shower of swords") or a
    /// dropped weapon ("drop a sword on Jake") falls from the sky like an object.
    #[inline]
    pub fn free_flier(&self) -> bool {
        self.category == FormCategory::Held
            && self.pilot.is_none()
            && (self.recipe.has(Mechanic::Flies) || self.recipe.has(Mechanic::DropFromSky))
            && !self.recipe.has(Mechanic::Returning)
    }

    #[inline]
    pub fn center(&self) -> Vec3 {
        self.pos + self.rot * self.local_center
    }

    #[inline]
    pub fn bounds(&self) -> Aabb {
        geom::obb_bounds(self.center(), self.half, self.rot)
    }

    /// Box that hits (melee, shots, other constructs) test against: the collider box, or for a
    /// giant every part of it (snout and tail included). World centre + half extents, `rot` frame.
    #[inline]
    pub fn hit_box(&self) -> (Vec3, Vec3) {
        if self.ai.giant.on { giant::hit_box(self) } else { (self.center(), self.half) }
    }

    /// Where a pilot's feet sit (top of the collider). The component system may use `part`.
    pub fn seat_pos(&self, _part: u8) -> Vec3 {
        self.center() + self.rot * Vec3::new(0.0, self.half.y, 0.0)
    }

    /// Mass for links / fields / clashes / pushes (kg).
    pub fn mass(&self) -> f32 {
        self.recipe.mass.max(1.0) * self.size_scale.powi(3)
    }

    /// Local −Z (the facing) in world space.
    #[inline]
    pub fn forward(&self) -> Vec3 {
        self.rot * Vec3::NEG_Z
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
            // A Lazy creature sits down as a real roadblock (players collide with it).
            FormCategory::Creature => (self.forming_t <= 0.0 && self.recipe.temperament == bd_core::mechanics::Temperament::Lazy, true),
            FormCategory::Object => (structure::object_is_solid(self), self.half.max_element() >= 0.4),
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
    /// Map gravity by altitude (Orbit); `None` = full gravity everywhere.
    pub gravity: Option<bd_voxel::GravityProfile>,
    /// TUNING HOOK: combat numbers (form damage table, guns, creature attacks).
    pub tuning: &'a crate::tuning::CombatTuning,
}

impl ConstructCtx<'_> {
    /// Gravity multiplier at height `y` (map gravity profile; 1 without one).
    #[inline]
    pub fn gravity_at(&self, y: f32) -> f32 {
        self.gravity.map_or(1.0, |g| g.scale_at(y))
    }

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
    /// Construct-system internal: cross-construct effects, applied at the end of
    /// [`ConstructStore::update`] (empty by the time the world looks). See [`Interaction`].
    pub queue: Vec<Interaction>,
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
        self.queue.clear();
    }
}

/// Everything needed to create a construct (the world fills it in `cast`).
#[derive(Clone, Debug, PartialEq)]
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
    /// Per-update proxy snapshot (reused buffer).
    proxies: Vec<Proxy>,
    /// Element chemistry on terrain (burning voxels…), §9.
    chem: chemistry::Chemistry,
    /// GIANTS: this update's giant tuning (damage is applied outside `update`).
    pub(crate) giants: giant::GiantTuning,
}

/// Uniform scale applied to a construct's shape so its characteristic size matches
/// `recipe.scale` (icons are authored at their own size). 1.0 for procedural shapes (bd_shapes
/// already builds them at the recipe's scale). Clients must use the same factor.
pub fn shape_scale(recipe: &Recipe, shape: &ShapeSpec) -> f32 {
    if !matches!(recipe.model, bd_core::mechanics::ModelSource::Icon(_)) {
        return 1.0;
    }
    let Some((lo, hi)) = shape_extent(shape) else { return 1.0 };
    let size = (hi - lo) * bd_core::CONSTRUCT_VOXEL_SIZE;
    // Characteristic axis of the form (the one normalised to 1.0 in the form table).
    let dims = form_spec(recipe.form).dims;
    let axis = if (dims.x - 1.0).abs() < 1e-3 {
        0
    } else if (dims.y - 1.0).abs() < 1e-3 {
        1
    } else if (dims.z - 1.0).abs() < 1e-3 {
        2
    } else {
        3
    };
    let ext = match axis {
        0 => size.x,
        1 => size.y,
        2 => size.z,
        _ => size.max_element(),
    };
    // Dome scale is a radius.
    let want = if recipe.form == Form::Dome { recipe.scale * 2.0 } else { recipe.scale };
    if ext > 1e-3 && want.is_finite() && want > 0.0 { (want / ext).clamp(0.05, 40.0) } else { 1.0 }
}

fn shape_extent(s: &ShapeSpec) -> Option<(Vec3, Vec3)> {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in s.prims.iter().filter(|p| !p.subtract) {
        // Rotation-agnostic: use the prim's bounding sphere-ish box (max extent on all axes when rotated).
        let h = if p.rot_deg == Vec3::ZERO { p.size * 0.5 } else { Vec3::splat(p.size.max_element() * 0.5) };
        lo = lo.min(p.center - h);
        hi = hi.max(p.center + h);
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// Collider box (half extents, local centre) for a recipe at its clamped scale, or from its shape
/// (icons rescaled uniformly to `recipe.scale`, see [`shape_scale`]).
pub fn body_box(recipe: &Recipe, shape: Option<&ShapeSpec>) -> (Vec3, Vec3) {
    if let Some(s) = shape.filter(|s| !s.prims.is_empty()) {
        if let Some((lo, hi)) = shape_extent(s) {
            let k = bd_core::CONSTRUCT_VOXEL_SIZE * shape_scale(recipe, s);
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
    let mut c = Construct {
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
        priced_autonomous: false,
        colossal: p.colossal,
        size_scale: 1.0,
        flair: 0,
        gone: None,
        shatter: None,
        parent: None,
        body: Body::default(),
        comp: components::CompState::default(),
        held: held::HeldState::default(),
        ai: ai::AiState::default(),
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
        slice: None,
        xform_seen: 0,
        alt_shape: None,
        authored_boxes: None,
        weapon: if category == FormCategory::Held {
            crate::weapons::archetype_for(&p.recipe).map(|a| crate::weapons::WeaponState::new(a, &crate::weapons::spec_for(&p.recipe, a)))
        } else {
            None
        },
        recipe: p.recipe,
    };
    components::init(&mut c);
    c
}

/// Pipeline step 1. Returns the owner's view when the construct should run this tick.
fn housekeeping(c: &mut Construct, ctx: &ConstructCtx, out: &mut ConstructOut) -> Option<PlayerView> {
    if c.gone.is_some() {
        return None;
    }
    let dt = ctx.dt;
    c.prev_pos = c.pos;
    c.age += dt;
    c.clash_cd = (c.clash_cd - dt).max(0.0);
    c.cooldown = (c.cooldown - dt).max(0.0);
    c.portal_cd = (c.portal_cd - dt).max(0.0);
    // Constructs outlive their owner's KO (normal upkeep/integrity/lifetime rules apply); they
    // only go when the owner leaves the match.
    let Some(owner) = ctx.player(c.owner).copied() else {
        c.gone = Some(GoneReason::OwnerGone);
        return None;
    };
    if c.forming_t > 0.0 {
        c.forming_t -= dt;
        if c.forming_t <= 0.0 {
            c.forming_t = 0.0;
            c.set_anim(AnimState::Idle, ctx.tick);
        }
    }
    if c.age >= c.life {
        components::on_expire(c, out);
        c.gone = Some(GoneReason::Expired);
        return None;
    }
    Some(owner)
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

    /// Terrain chemistry state, mutable (world entry points such as `World::ignite_terrain`).
    pub fn chemistry_mut(&mut self) -> &mut chemistry::Chemistry {
        &mut self.chem
    }

    /// Terrain chemistry state (burning voxels), read-only.
    pub fn chemistry(&self) -> &chemistry::Chemistry {
        &self.chem
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

    /// Live constructs of `owner` in `slot`, oldest first. Children (swarm copies, slice halves,
    /// emitted payloads) never count.
    pub fn in_slot(&self, owner: PlayerId, slot: Slot) -> impl Iterator<Item = &Construct> {
        self.owned_by(owner).filter(move |c| c.parent.is_none() && Slot::of(c.recipe.form) == slot)
    }

    /// The owner's held construct, if any.
    pub fn held_by(&self, owner: PlayerId) -> Option<&Construct> {
        self.in_slot(owner, Slot::Held).next()
    }

    /// Merge a grafted phrase into construct `id` (see [`graft::merge`]).
    pub fn graft(&mut self, id: ConstructId, add: &Recipe) -> Option<&Recipe> {
        let owner = {
            let c = self.get_mut(id)?;
            let had = components::eff_has(c, Mechanic::Transforms);
            graft::merge(c, add);
            components::on_graft(c);
            // "transform" grafted onto a transformer toggles it.
            if had && add.has(Mechanic::Transforms) {
                components::request_transform(c);
            }
            c.owner
        };
        graft::graft_command(&mut self.items, owner, add);
        self.get(id).map(|c| &c.recipe)
    }

    /// Movement modifiers from the owner's worn / held constructs (wings, parachute, jetpack,
    /// umbrella…). Called before `step_player`; `mods` already holds status effects.
    pub fn movement_mods(&self, owner: PlayerId, input: &PlayerInput, st: &PlayerState, mods: &mut MoveModifiers) {
        worn::movement_mods(self, owner, input, st, mods);
        crate::signature::orb::hold_slow(self, owner, mods); // SIGNATURE HOOK: holding a big energy ball
    }

    /// After a player moved: trampolines, launch pads, bouncy surfaces (structure.rs), parachute
    /// tumble-cancel (worn.rs).
    pub fn post_move(&self, pid: PlayerId, input: &PlayerInput, st: &mut PlayerState, pre_vel: Vec3, ev: &MoveEvents, p: &MoveParams) {
        worn::post_move(self, pid, input, st, pre_vel, ev, p);
        structure::post_move(self, pid, input, st, pre_vel, ev, p);
        crate::signature::domain::shell(self, st, p); // SIGNATURE HOOK: a sealed domain's barrier (both ways)
    }

    /// Run every construct's behaviour for one tick (the pipeline in the module docs).
    pub fn update(&mut self, ctx: &ConstructCtx, out: &mut ConstructOut) {
        graft::run_commands(&mut self.items);
        self.giants = ctx.tuning.giants;
        self.proxies.clear();
        self.proxies.extend(self.items.iter().map(Proxy::of));
        {
            let Self { items, proxies, .. } = self;
            for c in items.iter_mut() {
                let Some(owner) = housekeeping(c, ctx, out) else { continue };
                let env = Env { ctx, owner, others: proxies };
                c.body.begin_tick();
                components::drive(c, &env, out);
                let ctrl = controller_of(c);
                (behaviour(ctrl).step)(c, &env, out);
                if c.gone.is_some() {
                    continue;
                }
                components::steer(c, &env, out);
                body::integrate(c, &env, out);
                components::react(c, &env, out);
                components::post_react(c, &env, out);
                giant::end_tick(c, ctx.dt); // GIANTS: this tick's turn → the colliders' spin (riders)
                // BOUNDS HOOK: falling off the map is the world's recall (world/bounds.rs).
                if !c.pos.is_finite() || !c.vel.is_finite() {
                    c.gone = Some(GoneReason::Expired);
                }
            }
        }
        self.apply_queue(ctx, out);
        chemistry::step(&mut self.chem, &mut self.items, ctx, out);
        // HELD ENGINEER EDIT: swing-vs-swing sword-locks + Guard (block / perfect parry) on this
        // tick's melee hits. Runs before the generic clash so a sword-lock's `clash_cd` keeps the
        // generic pass from clashing the same pair twice.
        held::after_update(&mut self.items, ctx, out);
        counters::clash(&mut self.items, ctx, out);
    }

    /// Apply the cross-construct queue (pipeline step 6).
    fn apply_queue(&mut self, ctx: &ConstructCtx, out: &mut ConstructOut) {
        if out.queue.is_empty() {
            return;
        }
        // Interactions may queue more (a split copy that explodes…): bounded rounds.
        for _round in 0..3 {
            if out.queue.is_empty() {
                break;
            }
            let mut q = std::mem::take(&mut out.queue);
            for it in q.drain(..) {
                components::apply_interaction(self, it, ctx, out);
            }
            // Keep the buffer's capacity (steady state: no allocation).
            if out.queue.is_empty() {
                out.queue = q;
            } else {
                let more = std::mem::take(&mut out.queue);
                out.queue = q;
                out.queue.extend(more);
            }
        }
        out.queue.clear();
    }

    /// Framework: add a child construct during an update (swarm copies, slice halves, payloads).
    /// Emits `ConstructSpawned` so clients mirror it. Returns None at the global cap.
    pub(crate) fn spawn_child(&mut self, parent: ConstructId, p: SpawnParams, events: &mut Vec<GameEvent>) -> Option<ConstructId> {
        if self.items.len() >= crate::world::MAX_CONSTRUCTS - 8 {
            return None;
        }
        let (owner, recipe, pos, rot, forming) = (p.owner, Box::new(p.recipe.clone()), p.pos, p.rot, p.forming);
        let id = self.spawn(p);
        if let Some(c) = self.get_mut(id) {
            c.parent = Some(parent);
            c.cost = 0.0;
        }
        events.push(GameEvent::ConstructSpawned { id, owner, recipe, pos, rot, forming, cost: 0.0 });
        Some(id)
    }

    /// Solid / anchorable boxes for movement and tethers.
    pub fn write_colliders(&self, out: &mut Vec<Collider>) {
        for c in &self.items {
            // GIANTS: one solid, anchorable, moving box per body part (stand on it, ride it).
            if c.ai.giant.on && c.gone.is_none() && c.forming_t <= 0.0 {
                giant::push_colliders(c, out);
                continue;
            }
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
            let (hc, hh) = c.hit_box();
            if !geom::obb_bounds(hc, hh, c.rot).intersects(&seg) {
                continue;
            }
            if let Some((t, n)) = geom::segment_obb(p0, p1, hc, hh, c.rot, r) {
                if best.is_none_or(|b| t < b.1) {
                    best = Some((c.id, t, n));
                }
            }
        }
        best
    }

    /// Apply integrity damage (with the §8 counters of [`counters::on_damage`]: brittle frozen
    /// things shatter, Blade slices structures in two…). Marks the construct shattered at 0
    /// (reported by [`Self::reap`]). Returns true if it shattered.
    pub fn damage(&mut self, hit: &ConstructHit, events: &mut Vec<GameEvent>) -> bool {
        counters::on_damage(self, hit, events)
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
        let any_gone = self.items.iter().any(|c| c.gone.is_some());
        if !any_gone {
            return;
        }
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
        // Anything riding / held by a removed construct is released.
        components::release_orphans(&mut self.items);
    }

    /// Attach a hand-authored robot shape to a (two-mode) construct: stores it and the authored
    /// alt / robot collision boxes (computed once here, never per tick).
    pub fn set_alt_shape(&mut self, id: ConstructId, robot: ShapeSpec) -> bool {
        let Some(c) = self.get_mut(id) else { return false };
        let alt_box = (c.half, c.local_center);
        let robot_box = body_box(&c.recipe, Some(&robot));
        c.authored_boxes = Some([alt_box, robot_box]);
        c.alt_shape = Some(Box::new(robot));
        true
    }

    /// Drop everything (match reset). No events.
    pub fn clear(&mut self) {
        self.items.clear();
        self.chem.clear();
    }
}
