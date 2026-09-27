//! Player actions and commands: ring blast / shove (inputs), and the between-tick commands the game
//! layer calls — `cast` (validate → graft / extract / spawn), `dismiss`, oath recital, seats.

use std::sync::Arc;

use bd_core::input::buttons;
use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::voxel::raycast;
use bd_core::{ConstructId, DT, IVec3, PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::{Seat, World};
use crate::combat::*;
use crate::constructs::{Slot, SpawnParams, forming_secs};
use crate::events::{CastError, GameEvent, GoneReason};
use crate::extract::{ExtractOp, apply_extract};
use crate::geom;
use crate::movement::{CollisionWorld, raycast_world};
use crate::projectile::ProjectileSpawn;
use crate::rules::ScaleLimit;
use crate::space::{FIELD_RESIST, ForceField, PORTAL_LIFETIME, PortalEnd};
use crate::willpower::*;

/// TUNING HOOK: pricing facts taken from the recipe the player typed (before power / party
/// grafts), carried through the cast so standing grafts never raise its price.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TypedPricing {
    /// Priced as an autonomous non-creature (creature base + upkeep).
    pub autonomous: bool,
    /// The typed phrase makes things hunt on their own (`Flies` / `Walks`) — for grafts.
    pub drives: bool,
    /// SPARKS / PARTY: (weight, sparks before staleness) of the recipe as typed, when party grafts
    /// changed it afterwards — party grafts are free: the cast costs what was typed.
    pub party_price: Option<(f32, u32)>,
}

impl TypedPricing {
    fn of(t: &crate::tuning::CombatTuning, r: &Recipe) -> Self {
        Self { autonomous: t.willpower.is_autonomous(r), drives: r.has(Mechanic::Flies) || r.has(Mechanic::Walks), party_price: None }
    }
}

/// Forms that autonomy pricing applies to (held / structure / object).
fn willpower_autonomous_form(f: Form) -> bool {
    matches!(f.category(), FormCategory::Held | FormCategory::Structure | FormCategory::Object)
}

/// Suits: a piloted construct at or below this scale wraps its pilot (COMBAT.md §3d).
pub const SUIT_MAX_SCALE: f32 = 2.6;
/// PILOTING: walk up to your own boardable construct and press the Transform key within this reach
/// (m, from your eye to its body) to take the seat (COMBAT.md §5d-ter).
pub const BOARD_REACH: f32 = 4.0;
/// PILOTING: "get in" / "ride it" / "take the wheel" boards an own construct within this range (m).
pub const BOARD_COMMAND_RANGE: f32 = 40.0;

/// Is a piloted `recipe` a suit (wraps its pilot) rather than a cockpit (the pilot sits in / on it)?
/// Small humanoids and small transformers are suits; a horse, a car or a dragon is ridden.
pub fn suit_sized(recipe: &Recipe) -> bool {
    recipe.scale <= SUIT_MAX_SCALE && (recipe.form == Form::Biped || recipe.has(Mechanic::Transforms))
}
/// Casting reach for placement (m).
pub const CAST_RANGE: f32 = 60.0;

/// What the caster was pointing at (the client may resolve it; the host re-checks).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TargetHint {
    Player(PlayerId),
    Construct(ConstructId),
    Terrain(IVec3),
    Point(Vec3),
    /// FREEDOM HOOK: a player the words named ("drop a meteor on Ethan") and how it reaches them
    /// (`crate::freedom::deliver`). Host-validated; an invalid name casts at the crosshair.
    Named { player: PlayerId, via: crate::freedom::Delivery },
}

/// Where a cast is aimed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aim {
    /// Usually the caster's eye.
    pub origin: Vec3,
    pub dir: Vec3,
    pub target_hint: Option<TargetHint>,
    /// The phrase is a modifier ("legs", "wings", "fire"): graft it onto an own construct or,
    /// aimed at terrain, animate the terrain (COMBAT.md §3c). A full phrase aimed at an own
    /// construct grafts too.
    pub modifier: bool,
}

impl Aim {
    pub fn new(origin: Vec3, dir: Vec3) -> Self {
        Self { origin, dir, target_hint: None, modifier: false }
    }
}

/// Host rule `allow_third_person` off: the most a client-supplied aim may differ from the caster's
/// replicated look (rad, ≈ 20°). First-person clients aim along their look; the slack absorbs
/// sub-tick mouse motion and the input jitter buffer (commands apply on arrival, inputs a few ticks
/// later). Clamped, never refused.
pub const FIRST_PERSON_AIM_SLACK: f32 = 0.35;

/// `dir` rotated toward `axis` until it is within `max` rad of it (both unit vectors).
pub fn clamp_to_cone(dir: Vec3, axis: Vec3, max: f32) -> Vec3 {
    let c = dir.dot(axis).clamp(-1.0, 1.0);
    if c >= max.cos() {
        return dir;
    }
    let perp = (dir - axis * c).normalize_or(axis.any_orthonormal_vector());
    (axis * max.cos() + perp * max.sin()).normalize_or(axis)
}

impl World {
    /// Host truth for a client-supplied aim of player index `i`: `(origin, dir)`. The origin must be
    /// near the caster's eye (else the eye). CAMERA: when the host rule `allow_third_person` is off,
    /// nothing camera-derived is honoured — the origin is the eye and the direction is clamped to
    /// [`FIRST_PERSON_AIM_SLACK`] of the replicated look, so a client can't cast / power-move along
    /// a converged over-the-shoulder ray the rule forbids. (Vision itself is the client's; the host
    /// enforces what reaches the sim. Rule on: unchanged.)
    pub fn resolve_aim(&self, i: usize, aim: &Aim) -> (Vec3, Vec3) {
        let p = &self.players[i];
        let eye = p.eye(&self.cfg.move_params);
        let look = p.look();
        let dir = if aim.dir.is_finite() && aim.dir.length_squared() > 1e-6 { aim.dir.normalize() } else { look };
        if self.cfg.rules.allow_third_person {
            let origin = if aim.origin.is_finite() && (aim.origin - eye).length() < 4.0 { aim.origin } else { eye };
            (origin, dir)
        } else {
            (eye, clamp_to_cone(dir, look, FIRST_PERSON_AIM_SLACK))
        }
    }
}

impl World {
    // ---------------------------------------------------------------------------- ring (inputs)

    pub(crate) fn player_actions(&mut self) {
        let base = self.cfg.move_params.clone();
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if p.busy() || p.seat.is_some() || self.match_state.phase == crate::rules::MatchPhase::Ended {
                continue;
            }
            // A held construct (or one forming) owns the mouse buttons — not one lying where it
            // was thrown (`held::LINGER_SECS`: the hands are empty until it's picked up).
            if self.constructs.held_by(p.id).is_some_and(|c| !(c.held.thrown && c.held.landed)) {
                continue;
            }
            // SIGNATURE HOOK: an energy ball in the hands owns PRIMARY (hold to grow, release to throw).
            if crate::signature::orb::holding(self, p.id) {
                continue;
            }
            let id = p.id;
            let eye = p.eye(&base);
            let look = p.look();
            let wants_blast = p.input.held(buttons::PRIMARY) && p.blast_cd <= 0.0;
            let wants_shove = p.pressed & buttons::SECONDARY != 0 && p.shove_cd <= 0.0;
            let view = p.view_tick;
            if wants_blast {
                self.players[i].blast_cd = self.cfg.tuning.ring.blast_cooldown; // TUNING HOOK
                let origin = eye + look * 0.4;
                let spec = self.cfg.tuning.ring.blast_spec(); // TUNING HOOK
                self.events.push(GameEvent::ShotFired {
                    owner: id,
                    construct: None,
                    archetype: crate::weapons::GunArchetype::Ring,
                    muzzle: origin,
                    dir: look,
                    recoil: spec.recoil,
                    pellets: 1,
                });
                // TUNING HOOK: ring blast numbers from `tuning.ring`.
                let mut shot = ProjectileSpawn::ring_blast(id, origin, look);
                shot.vel = look.normalize_or_zero() * spec.speed;
                shot.damage = spec.damage;
                shot.knockback = spec.knockback;
                shot.radius = spec.radius;
                shot.life = self.cfg.tuning.ring.blast_lifetime;
                self.fire_projectile(shot, view, Some(id));
                self.recoil(id, look, spec.recoil);
            }
            if wants_shove {
                self.players[i].shove_cd = self.cfg.tuning.ring.shove_cooldown; // TUNING HOOK
                self.ring_shove(i, eye, look);
            }
        }
    }

    /// 4 m cone push, 14 kb, no damage: players, enemy constructs and enemy projectiles (which are
    /// batted back, now owned by the shover).
    fn ring_shove(&mut self, i: usize, eye: Vec3, look: Vec3) {
        let id = self.players[i].id;
        self.events.push(GameEvent::Shove { player: id, origin: eye, dir: look });
        let base = self.cfg.move_params.clone();
        // TUNING HOOK: shove numbers from `tuning.ring`.
        let (shove_range, shove_kb) = (self.cfg.tuning.ring.shove_range, self.cfg.tuning.ring.shove_knockback);
        let cos = self.cfg.tuning.ring.shove_half_angle.cos();
        let in_cone = |p: Vec3| {
            let d = p - eye;
            let len = d.length();
            len <= shove_range && (len < 0.5 || d.dot(look) / len >= cos)
        };
        let mut targets = [None; bd_core::MAX_PLAYERS];
        let mut n = 0;
        for p in &self.players {
            if p.id == id || !p.alive {
                continue;
            }
            let h = p.hull(&base);
            if in_cone(geom::closest_point(&h, eye + look * 1.5)) && n < targets.len() {
                targets[n] = Some((p.id, h.center()));
                n += 1;
            }
        }
        for (tid, c) in targets.iter().flatten().copied() {
            let dir = (look + (c - eye).normalize_or_zero()) * 0.5;
            self.apply_hit(HitInfo::new(tid, Some(id), HitKind::RingShove, 0.0, shove_kb, dir, c).with_impact(0.8));
        }
        for c in self.constructs.iter_mut() {
            if c.owner == id || c.attached() || !c.active() || matches!(c.category, FormCategory::Structure) {
                continue;
            }
            if in_cone(c.center()) {
                let m = c.mass().max(1.0);
                let resist = if c.has(Mechanic::Heavy) { FIELD_RESIST } else { 1.0 };
                c.vel += look * (shove_kb * resist * (80.0 / m).clamp(0.1, 1.5));
            }
        }
        for pr in &mut self.projectiles {
            if pr.spec.owner != id && !pr.dead && in_cone(pr.pos) {
                let speed = pr.vel.length();
                pr.vel = look * speed;
                pr.spec.owner = id;
                pr.origin = pr.pos;
                pr.vel0 = pr.vel;
                pr.segment_tick = self.tick;
                self.events.push(GameEvent::ProjectileFired { id: pr.id, owner: id, kind: pr.spec.kind, origin: pr.pos, vel: pr.vel });
            }
        }
    }

    // ---------------------------------------------------------------------------- casting

    /// SPARKS: a cast's weight (its plain willpower cost × the party ratio × signature / orbital
    /// multipliers — the presence tier's input and the spawn event's `cost`) and its price in whole
    /// sparks (`willpower::cast_sparks` at `presence_tier::cast_tier`; cheaper inside your own domain).
    pub(crate) fn cast_price(&self, player: PlayerId, recipe: &Recipe, autonomous: bool, uses: u32) -> (f32, f32) {
        let (weight, n) = self.cast_sparks_of(player, recipe, autonomous);
        (weight, self.stale_price(n, uses))
    }

    /// SPARKS: (presence weight, sparks before staleness) of a cast of `recipe`.
    fn cast_sparks_of(&self, player: PlayerId, recipe: &Recipe, autonomous: bool) -> (f32, u32) {
        let wt = &self.cfg.tuning.willpower;
        // The weight is the price-free presence input: no balance knobs (the gun multiplier is a
        // balance price, not gravitas), no autonomy pricing, no staleness.
        let weight = crate::signature::cost(self, recipe, wt.base_cost_unadjusted(recipe));
        let signature = self.signature.cast.is_some();
        let tier = crate::presence_tier::cast_tier(recipe, weight, false, signature, &crate::presence_tier::TierKnobs::default());
        let mult = crate::signature::domain::cast_cost(self, player, 1.0); // SIGNATURE HOOK: your own domain
        let famous = crate::constructs::legend::famous(recipe);
        let n = cast_sparks(&wt.sparks, tier.index(), recipe, famous, autonomous, 0, self.cfg.rules.pool_segments(), mult);
        (weight, self.call_cast_sparks(n)) // FIRST CALL HOOK: an echo is a spark cheaper
    }

    /// SPARKS: a signature cast's price (the Signature tier, + adjustments, + repeats).
    pub(crate) fn signature_price(&self, player: PlayerId, recipe: &Recipe, uses: u32) -> f32 {
        let wt = &self.cfg.tuning.willpower;
        let mult = crate::signature::domain::cast_cost(self, player, 1.0);
        let famous = crate::constructs::legend::famous(recipe);
        let tier = crate::presence_tier::Tier::Signature.index();
        let n = cast_sparks(&wt.sparks, tier, recipe, famous, wt.is_autonomous(recipe), 0, self.cfg.rules.pool_segments(), mult);
        self.stale_price(self.call_cast_sparks(n), uses) // FIRST CALL HOOK: an echo is a spark cheaper
    }

    /// Willpower for `n` sparks plus the repeat surcharge (+`sparks.stale` per use), capped by the bar.
    fn stale_price(&self, n: u32, uses: u32) -> f32 {
        let rules = &self.cfg.rules;
        let n = (n + uses.min(64) * rules.sparks(&self.cfg.tuning.willpower.sparks).stale as u32).clamp(1, rules.pool_segments());
        n as f32 * rules.segment_size()
    }

    /// Would this recipe cost `player` right now? (validated recipe, cost). For UI previews.
    pub fn preview_cost(&self, player: PlayerId, recipe: Recipe) -> (Recipe, f32) {
        let r = validate_recipe(recipe, &self.cfg.rules);
        // TUNING HOOK: costs from `tuning.willpower`.
        let wt = &self.cfg.tuning.willpower;
        let uses = self.player(player).map_or(0, |p| p.casts.uses_in(&r.name, self.tick, wt.stale_window));
        // SPARKS: the preview is the exact whole-spark price (`cast_price`, like the cast).
        let autonomous = wt.is_autonomous(&r);
        let cost = if self.cfg.rules.infinite_willpower() { 0.0 } else { self.cast_price(player, &r, autonomous, uses).1 };
        (r, cost)
    }

    /// Cast a construct. Validates/clamps the recipe, then (in order): graft onto an own construct
    /// under the crosshair; animate terrain if a modifier phrase is aimed at terrain; otherwise pay
    /// and spawn (replacing the oldest in a full slot). Piloted casts seat the caster.
    pub fn cast(&mut self, player: PlayerId, recipe: Recipe, shape: Option<ShapeSpec>, aim: Aim) -> Result<ConstructId, CastError> {
        self.cast_shaped(player, recipe, shape, None, aim)
    }

    /// [`Self::cast`] with a two-mode icon's hand-authored robot shape (`alt_shape`; `shape` is the
    /// alt / vehicle). A fresh construct stores it, the Transforms component uses its boxes, and
    /// clients get it once in `ConstructShapes`. Ignored by grafts.
    pub fn cast_shaped(&mut self, player: PlayerId, recipe: Recipe, shape: Option<ShapeSpec>, alt_shape: Option<ShapeSpec>, aim: Aim) -> Result<ConstructId, CastError> {
        self.pending_alt_shape = alt_shape.map(sanitize_shape);
        let res = self.cast_plain(player, recipe, shape, aim);
        self.pending_alt_shape = None;
        res
    }

    fn cast_plain(&mut self, player: PlayerId, recipe: Recipe, shape: Option<ShapeSpec>, aim: Aim) -> Result<ConstructId, CastError> {
        // PARTY HOOK: after-cast hooks (Echo, Mini-Me) look for this cast's spawn from here.
        let ev0 = self.events.len();
        let (recipe, gate) = self.call_cast(player, recipe); // FIRST CALL HOOK: rival / echo version (no-op while off)
        let outer = crate::freedom::begin_cast(self, player); // FREEDOM HOOK: the phrase / shout energy / "it" resize of this cast
        let res = self.cast_inner(player, recipe, shape, aim);
        self.call_done(gate, res.is_ok()); // FIRST CALL HOOK: claim / life log / marks
        // PARTY HOOK: the party cost discount (set by `party::modify_cast`) is per cast.
        self.party_cost_ratio = None;
        match res {
            Err(reason) => self.events.push(GameEvent::CastFailed { player, reason }),
            Ok(id) => crate::party::hooks::after_cast(self, player, id, aim, ev0), // PARTY HOOK (no-op in FFA)
        }
        crate::freedom::end_cast(self, outer); // FREEDOM HOOK
        res
    }

    fn cast_inner(&mut self, player: PlayerId, recipe: Recipe, shape: Option<ShapeSpec>, aim: Aim) -> Result<ConstructId, CastError> {
        let Some(i) = self.pidx(player) else { return Err(CastError::UnknownPlayer) };
        if self.match_state.phase == crate::rules::MatchPhase::Ended {
            return Err(CastError::MatchOver);
        }
        let p = &self.players[i];
        if !p.alive {
            return Err(CastError::Dead);
        }
        if p.oath.reciting() {
            return Err(CastError::Reciting);
        }
        // TRANSFORMERS FEEL: a cockpit pilot's words still work mid-fight ("transform", "roll out",
        // "energon axe" = the robot's own heavy: `constructs::brawl::seated_words`).
        if let Some(seat) = p.seat.filter(|s| !s.suit && !p.status.stunned()) {
            if let Some(r) = self.seated_words(player, seat.construct, &recipe) {
                return r;
            }
        }
        let p = &self.players[i];
        // SIGNATURE HOOK: domain burnout — no casting for a few seconds after a domain ends.
        if p.status.stunned() || p.charge.active || p.seat.is_some_and(|s| !s.suit) || crate::signature::domain::burnt_out(self, player) {
            return Err(CastError::Busy);
        }
        let (origin, dir) = self.resolve_aim(i, &aim); // CAMERA: host-side aim validation
        // FREEDOM HOOK: a named target is delivered to (freedom/deliver.rs): the cast faces them.
        let named = self.named_target(i, aim.target_hint, origin);
        let dir = named.map_or(dir, |n| (n.center - origin).normalize_or(dir));
        let recipe = validate_recipe(recipe, &self.cfg.rules);
        // TUNING HOOK: autonomy pricing is decided on the recipe as typed, before power / party
        // grafts (standing grafts never raise a cast's price).
        let mut typed = TypedPricing::of(&self.cfg.tuning, &recipe);
        // POWERS HOOK — cast modifier pipeline step 1 (powers, then party): fruit / stand casts
        // are handled by the powers module; incantations + standing power grafts rewrite the recipe.
        let recipe = match self.powers_on_cast(i, recipe, origin, dir) {
            Ok(r) => r,
            Err(done) => return done,
        };
        // PARTY HOOK: Wild Magic + standing grafts + Kaiju Heart (re-validates; no-op in FFA).
        let mut recipe = recipe;
        let before_party = recipe.clone();
        crate::party::modify_cast(self, i, &mut recipe, typed.autonomous);
        if self.party_cost_ratio.is_some() {
            // SPARKS / PARTY: party grafts are free: price the recipe as it was before them.
            typed.party_price = Some(self.cast_sparks_of(self.players[i].id, &before_party, typed.autonomous));
        }
        super::interactables::modify_cast(self, i, &mut recipe, typed.autonomous); // INTERACT HOOK: slot-won party effect (casino)
        crate::signature::lift_scale(self, &mut recipe); // SIGNATURE HOOK: ultimate scale past the form clamp
        let shape = shape.map(sanitize_shape);
        // Curated icons: normalise the stored spec to real size ONCE here (the client model is built
        // from the same normalised spec, so `shape_scale` / `body_box` / `timing_height` agree and the
        // rescale is ~1). A two-mode icon's robot shape uses the alt (vehicle) shape's factor, as
        // `bd_shapes::transform_rig_icon` does.
        let shape = match shape {
            Some(s) if matches!(recipe.model, bd_core::mechanics::ModelSource::Icon(_)) => {
                let f = bd_shapes::icon_scale_factor(&recipe, &s);
                let mut s = s;
                bd_shapes::scale_spec(&mut s, f);
                if let Some(robot) = self.pending_alt_shape.as_mut() {
                    bd_shapes::scale_spec(robot, f);
                }
                Some(s)
            }
            other => other,
        };

        // Where does the aim land?
        let cw = CollisionWorld::new(&self.voxels, &self.colliders);
        let world_hit = raycast_world(&cw, origin, dir, CAST_RANGE, true);
        let own_hit = self.constructs.segment_query(origin, origin + dir * CAST_RANGE, 0.3, None).filter(|(id, t, _)| {
            self.constructs.get(*id).is_some_and(|c| c.owner == player) && world_hit.is_none_or(|w| *t * CAST_RANGE <= w.t + 0.5)
        });
        let hinted_own = match aim.target_hint {
            Some(TargetHint::Construct(c)) => self.constructs.get(c).filter(|c| c.owner == player && c.gone.is_none()).map(|c| c.id),
            _ => None,
        };
        // Target player for autonomous constructs.
        if let Some(TargetHint::Player(t)) = aim.target_hint {
            if t != player && self.pidx(t).is_some() {
                self.players[i].last_target = Some(t);
            }
        }
        if let Some(n) = named {
            self.players[i].last_target = Some(n.player);
        }

        // 0. PILOTING: "get in" / "ride it" / "take the wheel" boards an own construct (never builds).
        if crate::constructs::graft::command_of(&recipe) == Some(crate::constructs::graft::Command::Board) {
            return self.board_command(player, hinted_own.or(own_hit.map(|h| h.0)));
        }
        // 1. Graft onto an own construct (never when the words named a player).
        if let Some(cid) = hinted_own.or(own_hit.map(|h| h.0)).filter(|_| !crate::signature::builds_fresh(self) && named.is_none()) {
            return self.graft(i, cid, recipe, typed);
        }
        // 2. Animate terrain (modifier phrase aimed at terrain).
        if aim.modifier && named.is_none() {
            let terrain = match aim.target_hint {
                Some(TargetHint::Terrain(v)) if self.voxels.get(v) != 0 => Some(v),
                _ => raycast(&self.voxels, origin, dir, CAST_RANGE).map(|h| h.voxel),
            };
            if let Some(v) = terrain {
                return self.animate_terrain(i, v, recipe, dir, typed);
            }
        }
        // 3. A fresh construct (named: aimed at their feet as the caster saw them).
        let aim_point = named.map(|n| n.feet).or(world_hit.map(|h| h.point)).unwrap_or(origin + dir * 12.0);
        let aim_normal = if named.is_some() { Vec3::Y } else { world_hit.map(|h| h.normal).unwrap_or(-dir) };
        self.spawn_cast(i, recipe, shape, None, origin, dir, aim_point, aim_normal, typed, named)
    }

    fn pay(&mut self, i: usize, cost: f32) -> Result<(), CastError> {
        // PARTY HOOK: `free_cast` = a party effect's free copy (Echo, Mini-Me, Copycat…).
        if self.cfg.rules.infinite_willpower() || self.free_cast {
            return Ok(());
        }
        let regen_pause = self.cfg.tuning.willpower.regen_pause; // TUNING HOOK
        let never = crate::launch::never_refuse(self); // WILLPOWER = LAUNCH: fires anyway, drains to 0
        let p = &mut self.players[i];
        if cost > p.willpower + 1e-4 && !never {
            return Err(CastError::NotEnoughWillpower { cost, have: p.willpower });
        }
        p.willpower = (p.willpower - cost).max(0.0);
        p.regen_pause = regen_pause;
        Ok(())
    }

    fn graft(&mut self, i: usize, cid: ConstructId, add: Recipe, typed: TypedPricing) -> Result<ConstructId, CastError> {
        let id = self.players[i].id;
        // TUNING HOOK: costs / integrity from `tuning` (integrity ignores the balance cost knobs).
        let wt = &self.cfg.tuning.willpower;
        let uses = self.players[i].casts.uses_in(&add.name, self.tick, wt.stale_window);
        // PARTY HOOK: `party_cost_ratio` makes party-grafted mechanics free (None in FFA).
        let cost = wt.base_cost_as(&add, typed.autonomous) * wt.graft_cost_fraction * self.party_cost_ratio.unwrap_or(1.0);
        let integrity_cost = cost * wt.staleness(uses) * wt.adjustment_ratio_as(&add, typed.autonomous);
        // SPARKS: a graft costs a fraction (`graft_cost_fraction`) of what the grafted phrase would
        // cost as a fresh cast, in whole sparks (never more than casting it fresh).
        let graft_sparks = {
            let (_, n) = self.cast_sparks_of(id, &add, typed.autonomous);
            let k = wt.graft_cost_fraction * self.party_cost_ratio.unwrap_or(1.0);
            if k > 0.0 { ((n as f32 * k).round() as u32).max(1) } else { 0 }
        };
        let integrity_gain = self.cfg.tuning.construct.integrity_for_cost(integrity_cost) * 0.25;
        // A typed graft that makes a non-creature hunt on its own ("wings" on your hammer) pays the
        // difference to a creature's base, so casting it plain and grafting isn't a discount.
        let surcharge = match self.constructs.get(cid) {
            Some(c) if typed.drives && wt.autonomous_pricing && !c.priced_autonomous && willpower_autonomous_form(c.recipe.form) => {
                (wt.autonomous_base - wt.category_base(c.recipe.form)).max(0.0) * wt.size_factor(&c.recipe)
            }
            _ => 0.0,
        };
        let cost = cost + surcharge;
        // SPARKS: the hunting surcharge is the autonomous +spark.
        let graft_sparks = graft_sparks + if surcharge > 0.0 { wt.sparks.autonomous as u32 } else { 0 };
        // FREEDOM HOOK: an "it" resize is paid like buying the bigger construct; an unaffordable graft
        // still applies — it drains what's left and gains integrity only for what was paid.
        let (resize, resize_cost) = crate::freedom::graft_resize(self, i, cid);
        // SPARKS: whole sparks (the "it" resize rounded like any cost); repeats add a spark each.
        let cost = if self.cfg.rules.infinite_willpower() {
            cost + resize_cost
        } else {
            let spark = self.cfg.rules.segment_size();
            let n = graft_sparks + crate::willpower::cost_segments(resize_cost, spark) + uses.min(64) * self.cfg.rules.sparks(&self.cfg.tuning.willpower.sparks).stale as u32;
            (n.min(self.cfg.rules.pool_segments()) as f32 * spark).max(0.0)
        };
        let (cost, paid) = crate::freedom::drain_afford(self, i, cost);
        let integrity_gain = integrity_gain * paid;
        self.pay(i, cost)?;
        self.players[i].casts.record(&add.name, self.tick);
        let grow = add.has(Mechanic::Grow);
        let shrink = add.has(Mechanic::Shrink);
        let merged = self.constructs.graft(cid, &add).cloned();
        if let Some(c) = self.constructs.get_mut(cid) {
            if surcharge > 0.0 {
                c.priced_autonomous = true; // TUNING HOOK: creature upkeep from now on
            }
            // "Grow your own hammer."
            if grow {
                c.resize(crate::space::GROW_SCALE);
            }
            if shrink {
                c.resize(crate::space::SHRINK_SCALE);
            }
            c.integrity = (c.integrity + integrity_gain).min(c.max_integrity * 1.5);
            c.max_integrity = c.max_integrity.max(c.integrity);
        }
        crate::freedom::graft_after(self, cid, resize, paid); // FREEDOM HOOK
        if let Some(r) = merged {
            self.events.push(GameEvent::Grafted { player: id, id: cid, recipe: Box::new(r.clone()), cost });
            self.attach_space_effects(cid, &r);
        }
        Ok(cid)
    }

    /// COMBAT.md §3c: lift the terrain piece at `v` and make it a construct with the phrase's
    /// mechanics. Replicated as `Extracted { op }` + `ConstructSpawned`.
    fn animate_terrain(&mut self, i: usize, v: IVec3, mut recipe: Recipe, dir: Vec3, typed: TypedPricing) -> Result<ConstructId, CastError> {
        let op = ExtractOp::at(v);
        // Cost first (the piece's size decides it): measure without mutating by extracting into a
        // clone-free dry run is expensive, so estimate from the op's bound and refund nothing.
        let autonomous = recipe.has(Mechanic::Walks) || recipe.has(Mechanic::Flies) || recipe.form.category() == FormCategory::Creature;
        recipe.form = if autonomous { Form::Blob } else { Form::Boulder };
        let est_scale = 3.0;
        recipe.scale = est_scale;
        let mut recipe = validate_recipe(recipe, &self.cfg.rules);
        self.steer_by_limits(self.players[i].id, &mut recipe)?; // CONSTRUCTS rules: before lifting anything
        let wt = &self.cfg.tuning.willpower; // TUNING HOOK
        let uses = self.players[i].casts.uses_in(&recipe.name, self.tick, wt.stale_window);
        let cost = self.cast_price(self.players[i].id, &recipe, typed.autonomous, uses).1; // SPARKS: by presence tier (party ratio inside)
        let (cost, _) = crate::freedom::drain_afford(self, i, cost); // FREEDOM HOOK: never refuse for willpower
        self.pay(i, cost)?;
        let outcome = apply_extract(&mut self.voxels, &op);
        if outcome.model.voxels.is_empty() {
            // Nothing to lift (indestructible / air): refund and fall back to a normal cast.
            let p = &mut self.players[i];
            p.willpower += cost;
            return Err(CastError::Busy);
        }
        self.players[i].casts.record(&recipe.name, self.tick);
        let model = Arc::new(outcome.model);
        let pivot = model.pivot_world();
        let (half, _) = model.body_box();
        let mut recipe = recipe;
        recipe.scale = (half.max_element() * 2.0).max(0.5);
        recipe.mass = (model.voxels.len() as f32 * 0.125 * 1500.0).clamp(crate::constructs::forms::MIN_MASS, crate::constructs::forms::MAX_MASS);
        let voxels = model.voxels.len() as u32;
        let id = self.spawn_cast(i, recipe, None, Some(model), pivot, dir, pivot, Vec3::Y, typed, None)?;
        self.events.push(GameEvent::Extracted { op, construct: id, voxels });
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_cast(
        &mut self,
        i: usize,
        recipe: Recipe,
        shape: Option<ShapeSpec>,
        model: Option<Arc<crate::constructs::VoxelModel>>,
        origin: Vec3,
        dir: Vec3,
        aim_point: Vec3,
        aim_normal: Vec3,
        typed: TypedPricing,
        named: Option<crate::freedom::deliver::Named>,
    ) -> Result<ConstructId, CastError> {
        let id = self.players[i].id;
        // FREEDOM HOOK: shout bonus (≤ +15 %), hidden legendary roll, novelty / highlights bookkeeping.
        let mut recipe = recipe;
        if let Some(n) = named {
            crate::freedom::deliver::prepare(&mut recipe, n.via); // FREEDOM HOOK: "a car on Ethan" falls on him
        }
        let flavor = crate::freedom::spawn_prelude(self, i, &mut recipe, model.is_some());
        // CONSTRUCTS rules: a kind the lobby turned off steers to the nearest allowed kind (or fizzles).
        self.steer_by_limits(id, &mut recipe)?;
        // POWER-UPS HOOK: giants / meteors need a pickup (Locked → the small version; Loaded → spent below).
        let spend = crate::pickups::gate_cast(self, id, &mut recipe);
        let colossal = is_colossal(&recipe);
        if colossal && self.constructs.owned_by(id).filter(|c| c.colossal).count() >= self.cfg.rules.colossal_limit() {
            return Err(CastError::ColossalLimit);
        }
        // TOTAL LIMIT (crate::limit): at the cap a construct that would ADD one more is refused before
        // anything is paid (a full per-kind cap replaces instead: the total doesn't grow).
        if self.limit_refuses_construct(id, &recipe) {
            return Err(CastError::AtLimit);
        }
        // TUNING HOOK: costs / integrity from `tuning` (integrity ignores the balance cost knobs).
        let wt = &self.cfg.tuning.willpower;
        let uses = self.players[i].casts.uses_in(&recipe.name, self.tick, wt.stale_window);
        // PARTY HOOK: cost ratio (party grafts are free); free casts skip paying, staleness and
        // slot replacement (they are children of the cast they copy).
        // SPARKS: the price is whole sparks by the presence tier players see and hear
        // (`cast_price`); the plain willpower cost is the cast's weight (the tier's input, the spawn
        // event, integrity). Repeats add sparks (anti-spam) instead of multiplying the weight.
        let stale = wt.staleness(uses);
        let (weight, full) = match (model.is_some(), typed.party_price) {
            (true, _) => (0.0, 0.0),
            (false, Some((w, n))) => (w, self.stale_price(n, uses)), // PARTY HOOK: what was typed
            (false, None) => self.cast_price(id, &recipe, typed.autonomous, uses),
        };
        // FREEDOM HOOK: never refuse for willpower — an unaffordable cast sputters (scaled down to fit,
        // drains everything left; a tiny fizzle at ~0).
        let price = |w: &World, r: &Recipe| w.cast_price(id, r, typed.autonomous, uses).1;
        let (cost, sputter) = if model.is_some() { (0.0, false) } else { crate::freedom::afford(self, i, &mut recipe, full, &price) };
        let weight = if sputter { self.cast_price(id, &recipe, typed.autonomous, uses).0 } else { weight };
        // Integrity is bought at the weight (unadjusted × staleness: exactly as before sparks), scaled
        // by what a sputter paid.
        let paid = if full > 1e-6 { (cost / full).clamp(0.0, 1.0) } else { 1.0 };
        let integrity_cost = weight * stale * paid;
        if model.is_none() && !self.free_cast {
            self.pay(i, cost)?;
            self.players[i].casts.record(&recipe.name, self.tick);
        }
        crate::freedom::legendary_boost(self, &mut recipe, flavor); // FREEDOM HOOK: hidden, so never priced
        if self.free_cast && self.constructs.len() >= super::MAX_CONSTRUCTS - 8 {
            return Err(CastError::Busy);
        }
        // Limits (`MatchRules::limits`): replace the oldest in a full slot, then kaiju, all-kinds
        // total and the map-wide giants cap. Free copies are children and never replace.
        let free = self.free_cast;
        if !free {
            let rules = &self.cfg.rules;
            let (slot_cap, kaiju_cap, total_cap, giants_cap) = (rules.slot_limit(Slot::of(recipe.form)), rules.kaiju_limit(), rules.total_limit(), rules.giants_on_map_limit());
            let in_slot: Vec<ConstructId> = self.constructs.in_slot(id, Slot::of(recipe.form)).map(|c| c.id).collect();
            self.replace_oldest(&in_slot, slot_cap);
            if recipe.form == Form::Kaiju {
                let kaiju: Vec<ConstructId> = self.constructs.owned_by(id).filter(|c| c.recipe.form == Form::Kaiju && c.gone.is_none()).map(|c| c.id).collect();
                self.replace_oldest(&kaiju, kaiju_cap);
            }
            let mine: Vec<ConstructId> = self.constructs.owned_by(id).filter(|c| c.parent.is_none()).map(|c| c.id).collect();
            self.replace_oldest(&mine, total_cap);
            if recipe.form == Form::Kaiju || colossal {
                // `steer_by_limits` only lets a giant through a full map when the caster has one to swap.
                let on_map = self.constructs.iter().filter(|c| c.gone.is_none() && c.parent.is_none() && is_giant(c)).count();
                if on_map >= giants_cap {
                    let own: Vec<ConstructId> = self.constructs.owned_by(id).filter(|c| c.parent.is_none() && is_giant(c)).map(|c| c.id).collect();
                    self.retire_oldest(&own, on_map + 1 - giants_cap);
                }
            }
        }
        // Global safety cap: never exceed MAX_CONSTRUCTS (replace this player's oldest).
        if self.constructs.len() >= super::MAX_CONSTRUCTS {
            let oldest = self.constructs.owned_by(id).map(|c| c.id).next();
            match oldest {
                Some(o) => {
                    if let Some(c) = self.constructs.get_mut(o) {
                        c.gone = Some(GoneReason::Replaced);
                    }
                    self.reap_constructs();
                }
                None => return Err(CastError::Busy),
            }
        }
        let base = self.cfg.move_params.clone();
        let p = &self.players[i];
        let look = dir;
        let yaw_face = geom::yaw_rotation(look);
        let category = recipe.form.category();
        let (pos, rot, vel) = if model.is_some() {
            (aim_point, Quat::IDENTITY, Vec3::ZERO)
        } else {
            match category {
                // FREEDOM HOOK: a held volley / dropped weapon falls onto the aim point (lifted by the drop)
                FormCategory::Held if recipe.has(Mechanic::DropFromSky) && !recipe.has(Mechanic::Returning) && !recipe.has(Mechanic::Flies) => {
                    (aim_point, geom::look_rotation(Vec3::new(look.x * 0.1, -1.0, look.z * 0.1).normalize_or(Vec3::NEG_Y)), Vec3::ZERO)
                }
                FormCategory::Held => (p.eye(&base), geom::look_rotation(look), Vec3::ZERO),
                FormCategory::Worn => (p.hull(&base).center(), yaw_face, Vec3::ZERO),
                FormCategory::Object => {
                    if recipe.has(Mechanic::DropFromSky) || recipe.form == Form::Anvil {
                        (aim_point + Vec3::Y * 12.0, yaw_face, Vec3::ZERO)
                    } else {
                        (origin + look * 1.0, geom::look_rotation(look), Vec3::ZERO)
                    }
                }
                FormCategory::Creature => {
                    let flier = recipe.has(Mechanic::Flies) || matches!(recipe.form, Form::Bird | Form::Insect);
                    let ground = self.ground_point_below(aim_point + aim_normal * 0.3 + Vec3::Y * 0.5);
                    let at = if flier { aim_point + Vec3::Y * 2.0 } else { ground.unwrap_or(aim_point) };
                    (at, yaw_face.inverse(), Vec3::ZERO)
                }
                FormCategory::Structure => self.place_structure(&recipe, origin, look, aim_point, aim_normal, p.mv.pos),
            }
        };
        // BOUNDS HOOK (world/bounds.rs): creatures / structures / vehicles land where the caster sees them.
        let pos = if model.is_none() && named.is_none() { self.place_visible(i, &recipe, shape.as_ref(), origin, look, rot, pos) } else { pos };
        // FREEDOM HOOK: named delivery — next to / around / in front of them (freedom/deliver.rs)
        let (pos, rot) = match named {
            Some(n) if model.is_none() => self.deliver_place(&recipe, shape.as_ref(), &n, (pos, rot)),
            _ => (pos, rot),
        };
        let pos = if model.is_none() { crate::signature::orbital_spawn(self, &recipe, origin, aim_point, pos) } else { pos }; // SIGNATURE HOOK: orbital emitters hover high
        let piloted = recipe.has(Mechanic::Piloted) && !matches!(category, FormCategory::Held | FormCategory::Worn);
        let suit = piloted && suit_sized(&recipe);
        // BOUNDS HOOK: lifetimes from `tuning.summon` (was `lifetime_secs`, dome 3 s, piloted ≥ 30 s).
        let mut life = self.cfg.tuning.summon.lifetime(recipe.form);
        if piloted {
            life = life.max(self.cfg.tuning.summon.life_piloted_min);
        }
        let life = crate::signature::life(self, recipe.form, life); // SIGNATURE HOOK: domain / ultimate lifetimes
        let target = self.players[i].last_target;
        let forming = crate::signature::forming(self, forming_secs(recipe.form)); // SIGNATURE HOOK: readable windup
        let cid = self.constructs.spawn(SpawnParams {
            owner: id,
            recipe: recipe.clone(),
            shape,
            pos,
            rot,
            vel,
            target,
            cost,
            integrity: self.cfg.tuning.construct.integrity_for_cost(integrity_cost.max(6.0)), // TUNING HOOK
            forming,
            life,
            tick: self.tick,
            colossal,
            model,
        });
        if let Some(c) = self.constructs.get_mut(cid) {
            if let Some(n) = named {
                c.comp.named = Some(n.player); // FREEDOM HOOK: homing / AI lock, thrown at them
                c.target = Some(n.player);
            }
            c.suit = suit;
            c.priced_autonomous = typed.autonomous && willpower_autonomous_form(c.recipe.form); // TUNING HOOK
        }
        // SPARKS: the event carries the cast's WEIGHT (not the spark price) — clients derive the same
        // presence tier from it (`presence_tier::cast_tier`), so what they see is what it cost.
        self.events.push(GameEvent::ConstructSpawned { id: cid, owner: id, recipe: Box::new(recipe.clone()), pos, rot, forming, cost: weight });
        crate::freedom::after_spawn(self, i, cid, &recipe, flavor, sputter); // FREEDOM HOOK: golden / sputter flags, feed tag
        if self.signature.cast.is_some()
            && let Some(c) = self.constructs.get_mut(cid)
        {
            c.flair |= crate::replicate::cflags::SIGNATURE; // SPARKS / PRESENCE: the Signature tier everywhere
        }
        if let Some(k) = spend {
            crate::pickups::spend(self, id, k); // POWER-UPS HOOK: the FULL-size payoff used the pickup
        }
        if let Some(alt) = self.pending_alt_shape.take() {
            self.attach_alt_shape(cid, alt);
        }
        if piloted {
            self.seat(id, cid, 0, suit);
        }
        self.attach_space_effects(cid, &recipe);
        crate::signature::orbital_spawned(self, cid, &recipe, origin, aim_point); // SIGNATURE HOOK: orbital strike cycle
        // Portals: first ring at the aim point, second in front of the caster.
        // Bigger casts open bigger gates (floor PORTAL_RADIUS); a Wall-form portal is a doorway
        // (upright rectangle, e.g. the doorman's door). Both ends are fitted to where they land.
        if recipe.has(Mechanic::Portal) && !(category == FormCategory::Held && crate::weapons::archetype_for(&recipe).is_some()) {
            let fwd = Vec3::new(look.x, 0.0, look.z).normalize_or_zero();
            let fwd = if fwd == Vec3::ZERO { Vec3::NEG_Z } else { fwd };
            let door = recipe.form == bd_core::mechanics::Form::Wall;
            let r = if door { (recipe.scale * 0.5).clamp(1.5, 3.0) } else { crate::space::portal_radius_for(recipe.scale) };
            let style = |e: PortalEnd| if door { e.door() } else { e };
            // A doorway aimed at the floor stands up on it, facing the caster.
            let a = if door && aim_normal.y.abs() > 0.7 {
                PortalEnd::new(aim_point + Vec3::Y * r, -fwd, Vec3::Y, r)
            } else {
                PortalEnd::new(aim_point + aim_normal * 0.05, aim_normal, Vec3::Y, r)
            };
            let a = crate::space::fit_portal(&self.voxels, style(a));
            let b_pos = self.players[i].mv.pos + fwd * 3.0 + Vec3::Y * if door { r } else { 1.2 };
            let b = crate::space::fit_portal(&self.voxels, style(PortalEnd::new(b_pos, -fwd, Vec3::Y, r)));
            self.create_portal(id, a, b, PORTAL_LIFETIME, Some(cid));
        }
        Ok(cid)
    }

    /// Retire the oldest of `ids` (oldest first) so at most `cap - 1` are left (room for one more).
    fn replace_oldest(&mut self, ids: &[ConstructId], cap: usize) {
        self.retire_oldest(ids, (ids.len() + 1).saturating_sub(cap));
    }

    /// Mark the `k` oldest of `ids` (oldest first) replaced.
    fn retire_oldest(&mut self, ids: &[ConstructId], k: usize) {
        for old in &ids[..k.min(ids.len())] {
            if let Some(c) = self.constructs.get_mut(*old) {
                c.gone = Some(GoneReason::Replaced);
            }
        }
    }

    /// CONSTRUCTS rules (`MatchRules::limits`), before anything is paid: steer a cast to a kind the
    /// lobby allows. A kaiju with kaiju off (or with the map full of giants and none of the caster's
    /// own to swap out) becomes a big creature; a colossal one just big. A kind that is off with
    /// nothing near it fizzles ([`CastError::Fizzled`]).
    pub(crate) fn steer_by_limits(&self, id: PlayerId, recipe: &mut Recipe) -> Result<(), CastError> {
        let rules = &self.cfg.rules;
        if recipe.form == Form::Kaiju || is_colossal(recipe) {
            let on_map = self.constructs.iter().filter(|c| c.gone.is_none() && c.parent.is_none() && is_giant(c)).count();
            let swappable = !self.free_cast && self.constructs.owned_by(id).any(|c| c.parent.is_none() && is_giant(c));
            let cap = rules.giants_on_map_limit();
            let downsize = on_map >= cap && (!swappable || cap == 0);
            if recipe.form == Form::Kaiju && (rules.kaiju_limit() == 0 || downsize) {
                recipe.form = Form::Biped;
                let spec = crate::constructs::form_spec(Form::Biped);
                let (lo, hi) = rules.scale_range(spec.min_scale, spec.max_scale);
                recipe.scale = recipe.scale.clamp(lo, hi);
            }
            if is_colossal(recipe) && (rules.colossal_limit() == 0 || downsize) {
                recipe.scale = crate::rules::COLOSSAL_SCALE - 0.01;
            }
        }
        if rules.slot_limit(Slot::of(recipe.form)) == 0 || rules.total_limit() == 0 {
            return Err(CastError::Fizzled);
        }
        Ok(())
    }

    /// Store an authored robot shape on `cid` and tell clients (once).
    pub(crate) fn attach_alt_shape(&mut self, cid: ConstructId, alt: ShapeSpec) {
        if self.constructs.set_alt_shape(cid, alt) {
            if let Some(c) = self.constructs.get(cid) {
                if let Some(a) = c.alt_shape.clone() {
                    self.events.push(GameEvent::ConstructShapes { id: cid, shape: c.shape.clone(), alt_shape: a });
                }
            }
        }
    }

    /// Fields for Attract / Repel constructs (follow the construct, die with it).
    fn attach_space_effects(&mut self, cid: ConstructId, recipe: &Recipe) {
        let attract = recipe.has(Mechanic::Attract);
        let repel = recipe.has(Mechanic::Repel);
        if !(attract || repel) || self.fields.iter().any(|f| f.construct == Some(cid)) {
            return;
        }
        let Some(c) = self.constructs.get(cid) else { return };
        let radius = (6.0 + recipe.scale * 2.0).clamp(6.0, 14.0);
        let strength = if attract && !repel { 22.0 } else if repel && !attract { -22.0 } else { 0.0 };
        if strength == 0.0 {
            return;
        }
        let secs = if c.life.is_finite() { c.life - c.age } else { 20.0 };
        let (owner, pos) = (c.owner, c.center());
        self.create_field(owner, pos, radius, strength, secs, Some(cid));
    }

    /// Structures stand on the ground at the aim point, facing the caster; platforms float,
    /// domes wrap the caster.
    fn place_structure(&self, r: &Recipe, _origin: Vec3, look: Vec3, aim: Vec3, _n: Vec3, caster_feet: Vec3) -> (Vec3, Quat, Vec3) {
        let face = geom::yaw_rotation(look);
        match r.form {
            Form::Dome => (caster_feet, face, Vec3::ZERO),
            Form::Platform => (aim, face, Vec3::ZERO),
            Form::Ramp => {
                // Rises away from the caster at 30°; a gliding ramp ("surf ramp", "half pipe": Glide)
                // stands at 60° — a surf face (movement/surf.rs).
                let foot = self.ground_point_below(aim + Vec3::Y * 1.0).unwrap_or(aim);
                let deg = if r.mechanics.contains(&Mechanic::Glide) { 60f32 } else { 30.0 };
                let tilt = Quat::from_rotation_x(deg.to_radians());
                (foot, face * tilt, Vec3::ZERO)
            }
            _ => {
                let ground = self.ground_point_below(aim + Vec3::Y * 1.0);
                (ground.unwrap_or(aim), face, Vec3::ZERO)
            }
        }
    }

    /// Dismiss (X): every construct of `player` dissolves with a 25 % refund.
    pub fn dismiss(&mut self, player: PlayerId) -> usize {
        let mut n = 0;
        for c in self.constructs.iter_mut() {
            if c.owner == player && c.gone.is_none() {
                c.gone = Some(GoneReason::Dismissed);
                n += 1;
            }
        }
        self.reap_constructs();
        n
    }

    /// Dismiss one construct (25 % refund).
    pub fn dismiss_one(&mut self, player: PlayerId, id: ConstructId) -> bool {
        let ok = match self.constructs.get_mut(id) {
            Some(c) if c.owner == player && c.gone.is_none() => {
                c.gone = Some(GoneReason::Dismissed);
                true
            }
            _ => false,
        };
        if ok {
            self.reap_constructs();
        }
        ok
    }

    // ---------------------------------------------------------------------------- oath

    /// F: start reciting one `section` of the corps oath (vulnerable; movement keys ignored
    /// client-side via TYPING). The length comes from the host's own [`crate::willpower::OathTable`]
    /// (the client never sends one); an out-of-range section reads as section 0.
    pub fn begin_oath(&mut self, player: PlayerId, section: u8) -> bool {
        let Some(i) = self.pidx(player) else { return false };
        let p = &mut self.players[i];
        if !p.alive {
            return false;
        }
        if let OathState::Reciting { started, len, progress: 0, .. } = &mut p.oath {
            // Sent again before any progress (voice mode, or the first typed letter): restart the
            // clock where the recital really starts. Only ever makes the host's floor stricter.
            *started = self.tick;
            *len = self.cfg.oaths.section_len(p.corps, section);
            return true;
        }
        if p.oath.reciting() {
            return false;
        }
        let len = self.cfg.oaths.section_len(p.corps, section);
        p.oath = OathState::Reciting { started: self.tick, len, progress: 0, accuracy: 0.9 };
        self.events.push(GameEvent::OathBegan { player });
        true
    }

    /// Optional progress report while typing (improves the partial refill on interruption).
    pub fn oath_progress(&mut self, player: PlayerId, correct_chars: u16, accuracy: f32) {
        if let Some(p) = self.player_mut(player) {
            if let OathState::Reciting { len, progress, accuracy: acc, .. } = &mut p.oath {
                *progress = correct_chars.min(*len);
                if accuracy.is_finite() {
                    *acc = accuracy.clamp(0.0, 1.0);
                }
            }
        }
    }

    /// The client finished the typing test: validate plausibility, refill, end the recital.
    pub fn oath_result(&mut self, player: PlayerId, accuracy: f32, seconds: f32) -> Result<f32, OathError> {
        self.oath_result_as(player, accuracy, seconds, false)
    }

    /// [`Self::oath_result`] for a typed (`voice = false`) or spoken recital. Same formula and
    /// plausibility check; a spoken one gets `tuning.willpower.oath_voice_slack` before the
    /// host-measured time floors its claimed speaking time (Whisper's latency is not the player's).
    pub fn oath_result_as(&mut self, player: PlayerId, accuracy: f32, seconds: f32, voice: bool) -> Result<f32, OathError> {
        let res = self.oath_result_inner(player, accuracy, seconds, voice);
        match res {
            Ok(refill) => self.events.push(GameEvent::OathFinished { player, refill }),
            Err(reason) => self.events.push(GameEvent::OathRejected { player, reason }),
        }
        res
    }

    fn oath_result_inner(&mut self, player: PlayerId, accuracy: f32, seconds: f32, voice: bool) -> Result<f32, OathError> {
        let Some(i) = self.pidx(player) else { return Err(OathError::UnknownPlayer) };
        let pool = self.cfg.rules.willpower_max();
        let slack = if voice { self.cfg.tuning.willpower.oath_voice_slack.max(OATH_LATENCY_SLACK) } else { OATH_LATENCY_SLACK };
        let tick = self.tick;
        let oath_scale = crate::launch::oath_scale(self); // WILLPOWER = LAUNCH: faster refills (×1 off)
        let p = &mut self.players[i];
        if !p.alive {
            p.oath = OathState::Idle;
            return Err(OathError::Dead);
        }
        let OathState::Reciting { started, len, .. } = p.oath else { return Err(OathError::NotReciting) };
        p.oath = OathState::Idle;
        let host_elapsed = tick.saturating_sub(started) as f32 * DT;
        let min = oath_min_secs(len);
        if !seconds.is_finite() || !accuracy.is_finite() || seconds < min || host_elapsed + OATH_LATENCY_SLACK < min {
            return Err(OathError::Implausible);
        }
        // Never credit a time faster than the host saw (minus network / transcription slack).
        let t = seconds.max(host_elapsed - slack).max(min);
        let refill = oath_refill(accuracy, t, len, pool) * oath_scale;
        p.willpower = (p.willpower + refill).min(pool);
        Ok(refill)
    }

    /// Cancel the recital (no refill).
    pub fn cancel_oath(&mut self, player: PlayerId) {
        if let Some(p) = self.player_mut(player) {
            if p.oath.reciting() {
                p.oath = OathState::Idle;
                self.events.push(GameEvent::OathCancelled { player });
            }
        }
    }

    // ---------------------------------------------------------------------------- seats

    /// Seat `player` in construct `cid` (cockpit, or suit when `suit`).
    pub fn seat(&mut self, player: PlayerId, cid: ConstructId, part: u8, suit: bool) -> bool {
        let Some(i) = self.pidx(player) else { return false };
        if !self.players[i].alive || self.players[i].seat.is_some() {
            return false;
        }
        let Some(c) = self.constructs.get_mut(cid) else { return false };
        if c.pilot.is_some() || c.gone.is_some() {
            return false;
        }
        c.pilot = Some(player);
        c.suit = suit;
        crate::constructs::pilot::on_board(c); // PILOTING: no launch pending; a transformer keeps its mode
        let titan = !suit && crate::constructs::titan::walks(c);
        let p = &mut self.players[i];
        p.seat = Some(Seat { construct: cid, part, suit });
        if !suit {
            p.mv.release_tethers();
        }
        // TITANS: the pilot's movement state becomes the walker's feet (dash charges full).
        if titan {
            crate::constructs::titan::seat(&mut p.mv, c, &self.cfg.tuning.giants);
        }
        self.events.push(GameEvent::Seated { player, construct: cid, part });
        true
    }

    /// Transform key (client binding: Mouse3; also the "transform" command) — HELD ENGINEER
    /// (COMBAT.md §5d-bis): toggles the seated suit / cockpit construct with `Mechanic::Transforms`
    /// between robot and alt (vehicle) mode. The switch happens on the next tick
    /// (`constructs::pilot::toggle`). Returns false if not seated in a transformer.
    ///
    /// PILOTING (COMBAT.md §5d-ter): on foot, the same key BOARDS your own boardable construct
    /// ([`Self::board_nearby`]: within [`BOARD_REACH`] and looked at) — so the one key that
    /// transforms a transformer also gets you into it.
    pub fn toggle_transform(&mut self, player: PlayerId) -> bool {
        let Some(seat) = self.player(player).and_then(|p| p.seat) else { return self.board_nearby(player).is_some() };
        match self.constructs.get_mut(seat.construct) {
            Some(c) if c.gone.is_none() && c.has(Mechanic::Transforms) => {
                c.held.transform_req = true;
                true
            }
            _ => false,
        }
    }

    // ---------------------------------------------------------------------------- boarding

    /// Why `player` can't board `cid` right now (None = they can). Host rule: only the OWNER, alive,
    /// on foot (not seated, not stunned / reciting / charging), a live boardable construct nobody
    /// pilots, within `reach` (m, eye to its body). Enemies never board your constructs (a giant's
    /// hijack is its own rule: `constructs::giant::step_hijack`).
    pub fn board_blocker(&self, player: PlayerId, cid: ConstructId, reach: f32) -> Option<CastError> {
        let Some(p) = self.player(player) else { return Some(CastError::UnknownPlayer) };
        if !p.alive {
            return Some(CastError::Dead);
        }
        if p.seat.is_some() || p.status.stunned() || p.oath.reciting() || p.charge.active {
            return Some(CastError::Busy);
        }
        let Some(c) = self.constructs.get(cid) else { return Some(CastError::Fizzled) };
        if c.owner != player || c.pilot.is_some() || !crate::constructs::pilot::boardable(c) {
            return Some(CastError::Fizzled);
        }
        let eye = p.eye(&self.cfg.move_params);
        let b = c.bounds();
        if geom::dist_sq_point_aabb(&b, eye) > reach * reach {
            return Some(CastError::Fizzled);
        }
        None
    }

    /// Board `cid` (see [`Self::board_blocker`]): graft `Piloted` onto it (replicated as a free
    /// `Grafted`) and seat the owner — a cockpit, or a suit for small humanoids / transformers. A
    /// transformer keeps the mode it is in (robot or vehicle); the Transform key still switches it.
    pub fn board(&mut self, player: PlayerId, cid: ConstructId, reach: f32) -> Result<ConstructId, CastError> {
        if let Some(e) = self.board_blocker(player, cid, reach) {
            return Err(e);
        }
        let Some(c) = self.constructs.get(cid) else { return Err(CastError::Fizzled) };
        if !c.has(Mechanic::Piloted) {
            let mut add = Recipe::unknown("piloted");
            add.mechanics = vec![Mechanic::Piloted];
            let add = add.normalized();
            if let Some(r) = self.constructs.graft(cid, &add).cloned() {
                self.events.push(GameEvent::Grafted { player, id: cid, recipe: Box::new(r), cost: 0.0 });
            }
        }
        let suit = self.constructs.get(cid).is_some_and(|c| suit_sized(&c.recipe));
        if self.seat(player, cid, 0, suit) { Ok(cid) } else { Err(CastError::Busy) }
    }

    /// The Transform key on foot: board [`Self::board_candidate`]. Returns the boarded construct.
    pub fn board_nearby(&mut self, player: PlayerId) -> Option<ConstructId> {
        let cid = self.board_candidate(player)?;
        self.board(player, cid, BOARD_REACH + 0.5).ok()
    }

    /// Which own construct the Transform key would board right now: a boardable one nobody pilots
    /// within [`BOARD_REACH`] of the eye — the one the look ray passes through, else the nearest in
    /// a 45° cone. Read-only and deterministic (ties → lower id), so the client's "you can get in"
    /// glow (bd_game `construct_anim`) asks exactly this.
    pub fn board_candidate(&self, player: PlayerId) -> Option<ConstructId> {
        let p = self.player(player)?;
        if !p.alive || p.seat.is_some() {
            return None;
        }
        let eye = p.eye(&self.cfg.move_params);
        let look = p.look();
        let mut best: Option<(f32, ConstructId)> = None;
        for c in self.constructs.owned_by(player) {
            if c.pilot.is_some() || !crate::constructs::pilot::boardable(c) {
                continue;
            }
            let b = c.bounds();
            let d2 = geom::dist_sq_point_aabb(&b, eye);
            if d2 > BOARD_REACH * BOARD_REACH {
                continue;
            }
            let to = (b.center() - eye).normalize_or(look);
            let looked = geom::segment_aabb(eye, eye + look * (BOARD_REACH + 0.5), &b).is_some();
            if !looked && to.dot(look) < std::f32::consts::FRAC_1_SQRT_2 && d2 > 0.25 {
                continue;
            }
            let score = if looked { d2 } else { d2 + 100.0 };
            if best.is_none_or(|(s, id)| score < s || (score == s && c.id < id)) {
                best = Some((score, c.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// "get in" / "hop in" / "ride it" / "let me drive" / "take the wheel": board the aimed own
    /// construct if boardable, else the nearest boardable one within [`BOARD_COMMAND_RANGE`].
    /// Nothing to board fizzles (no cost: a command never builds anything).
    pub fn board_command(&mut self, player: PlayerId, aimed: Option<ConstructId>) -> Result<ConstructId, CastError> {
        if let Some(cid) = aimed.filter(|&cid| self.board_blocker(player, cid, BOARD_COMMAND_RANGE).is_none()) {
            return self.board(player, cid, BOARD_COMMAND_RANGE);
        }
        let eye = self.player(player).map(|p| p.eye(&self.cfg.move_params)).ok_or(CastError::UnknownPlayer)?;
        let mut best: Option<(f32, ConstructId)> = None;
        for c in self.constructs.owned_by(player) {
            if c.pilot.is_some() || !crate::constructs::pilot::boardable(c) {
                continue;
            }
            let d2 = geom::dist_sq_point_aabb(&c.bounds(), eye);
            if d2 <= BOARD_COMMAND_RANGE * BOARD_COMMAND_RANGE && best.is_none_or(|(s, id)| d2 < s || (d2 == s && c.id < id)) {
                best = Some((d2, c.id));
            }
        }
        match best {
            Some((_, cid)) => self.board(player, cid, BOARD_COMMAND_RANGE),
            None => Err(CastError::Fizzled),
        }
    }

    /// E: leave the seat with a big upward launch (a movement tech).
    pub fn eject(&mut self, player: PlayerId) -> bool {
        let Some(i) = self.pidx(player) else { return false };
        let Some(seat) = self.players[i].seat else { return false };
        let look = self.players[i].look();
        let fwd = Vec3::new(look.x, 0.0, look.z).normalize_or_zero();
        let cvel = self.constructs.get(seat.construct).map_or(Vec3::ZERO, |c| c.vel);
        let vel = cvel + Vec3::Y * 20.0 + fwd * 4.0;
        self.eject_with(player, seat.construct, vel, false)
    }

    pub(crate) fn eject_with(&mut self, player: PlayerId, cid: ConstructId, vel: Vec3, tumble: bool) -> bool {
        let Some(i) = self.pidx(player) else { return false };
        if self.players[i].seat.is_none_or(|s| s.construct != cid) {
            return false;
        }
        let top = self.constructs.get(cid).map(|c| c.bounds().max.y);
        if let Some(c) = self.constructs.get_mut(cid) {
            c.pilot = None;
            crate::constructs::pilot::on_leave(c); // PILOTING: hop out = your follower from here
        }
        let gas_max = self.cfg.move_params.gas_max;
        let p = &mut self.players[i];
        let was_suit = p.seat.is_some_and(|s| s.suit);
        p.seat = None;
        if !was_suit {
            p.mv.gas = gas_max; // TITANS: the dash charges lived in `gas` while seated
            if let Some(top) = top {
                p.mv.pos.y = p.mv.pos.y.max(top + 0.1);
            }
        }
        p.mv.vel = if vel.is_finite() { vel } else { Vec3::Y * 10.0 };
        p.mv.grounded = false;
        p.mv.prev_buttons = p.input.buttons;
        p.mv.hop_chain = 0;
        p.mv.tumble = if tumble { TUMBLE_SECS } else { 0.0 };
        p.eject_cd = 0.5;
        let v = p.mv.vel;
        self.events.push(GameEvent::Ejected { player, construct: cid, vel: v });
        true
    }

    // ---------------------------------------------------------------------------- resize

    /// Shrink / Grow a player (`scale` 0.5 … 1.6) for `secs`.
    pub fn resize_player(&mut self, player: PlayerId, scale: f32, secs: f32) {
        let scale = if scale.is_finite() { scale } else { 1.0 };
        let secs = if secs.is_finite() { secs } else { 0.0 };
        if let Some(p) = self.player_mut(player) {
            if !p.alive {
                return;
            }
            p.size.apply(scale, secs);
            let (s, t) = (p.size.target, p.size.timer);
            self.events.push(GameEvent::Resized { player, scale: s, secs: t });
        }
    }

    /// Placeholder hook kept for the scale rules (NoLimits colossal casts).
    pub fn scale_limit(&self) -> ScaleLimit {
        self.cfg.rules.scale
    }

    /// Mark: field on a construct (used by grafts) — public so the construct engineer can reuse.
    pub fn field_for_construct(&self, cid: ConstructId) -> Option<&ForceField> {
        self.fields.iter().find(|f| f.construct == Some(cid))
    }
}

impl World {
    /// PARTY HOOK: a free cast for party effects (Echo, Mini-Me, Copycat, Sidekick): validated and
    /// placed exactly like a fresh cast (never grafts / animates terrain), but costs nothing, adds
    /// no staleness, replaces nothing (it is a child of `parent`) and refunds nothing on dismiss.
    /// `shape` / `alt_shape`: the copied construct's voxel shape and authored robot shape, if any.
    pub(crate) fn spawn_free(
        &mut self,
        player: PlayerId,
        recipe: Recipe,
        aim: Aim,
        parent: Option<ConstructId>,
        shape: Option<ShapeSpec>,
        alt_shape: Option<ShapeSpec>,
    ) -> Result<ConstructId, CastError> {
        let Some(i) = self.pidx(player) else { return Err(CastError::UnknownPlayer) };
        if self.match_state.phase == crate::rules::MatchPhase::Ended {
            return Err(CastError::MatchOver);
        }
        let p = &self.players[i];
        if !p.alive {
            return Err(CastError::Dead);
        }
        if p.seat.is_some_and(|s| !s.suit) {
            return Err(CastError::Busy);
        }
        let base = self.cfg.move_params.clone();
        let eye = p.eye(&base);
        let origin = if aim.origin.is_finite() && (aim.origin - eye).length() < 4.0 { aim.origin } else { eye };
        let dir = if aim.dir.is_finite() && aim.dir.length_squared() > 1e-6 { aim.dir.normalize() } else { p.look() };
        let recipe = validate_recipe(recipe, &self.cfg.rules);
        let cw = CollisionWorld::new(&self.voxels, &self.colliders);
        let world_hit = raycast_world(&cw, origin, dir, CAST_RANGE, true);
        let aim_point = world_hit.map(|h| h.point).unwrap_or(origin + dir * 12.0);
        let aim_normal = world_hit.map(|h| h.normal).unwrap_or(-dir);
        self.free_cast = true;
        // Party / power free copies are never priced up (TypedPricing::default()).
        let res = self.spawn_cast(i, recipe, shape.map(sanitize_shape), None, origin, dir, aim_point, aim_normal, TypedPricing::default(), None);
        self.free_cast = false;
        if let (Ok(id), Some(alt)) = (res, alt_shape) {
            self.attach_alt_shape(id, sanitize_shape(alt));
        }
        if let Ok(id) = res {
            if let Some(c) = self.constructs.get_mut(id) {
                // No parent ⇒ its own parent: party copies never count toward slot limits.
                c.parent = Some(parent.unwrap_or(id));
                c.cost = 0.0;
            }
        }
        res
    }
}

/// Clamp an untrusted shape (prim count / sizes / NaNs).
/// A giant for the map-wide cap (`ConstructLimits::giants_on_map`): a kaiju or anything colossal.
fn is_giant(c: &crate::constructs::Construct) -> bool {
    c.recipe.form == Form::Kaiju || c.colossal
}

pub(crate) fn sanitize_shape(mut s: ShapeSpec) -> ShapeSpec {
    s.prims.truncate(256);
    s.parts.truncate(64);
    s.prims.retain(|p| p.center.is_finite() && p.size.is_finite() && p.rot_deg.is_finite());
    for p in &mut s.prims {
        p.size = p.size.abs().min(Vec3::splat(400.0));
        p.center = p.center.clamp(Vec3::splat(-400.0), Vec3::splat(400.0));
    }
    s
}
