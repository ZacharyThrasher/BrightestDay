//! Energy balls — one generic rule for every thrown / summoned ball of energy: "rasengan", "spirit
//! bomb", "fireball", "giant ball of lightning", "tailed beast bomb", HOLLOW PURPLE, anything a
//! player invents that resolves to a `Ball` of energy ([`is_energy_ball`]). No per-name code (owner:
//! "too specific to the attack — generalize the ideas").
//!
//! 1. **Form** (the cast's windup): it gathers where its SIZE says — between the palms when small,
//!    overhead with both arms up past `overhead_diameter` (an erasing orb gathers ahead of the
//!    caster, as before). The caster keeps moving (slower the bigger it is: [`hold_slow`], read by
//!    the host and by prediction alike from the replicated anim + size).
//! 2. **Hold to grow**: after forming, holding PRIMARY grows it (`grow_rate`/s up to `grow_max` ×,
//!    draining willpower in proportion to its size — never free); releasing throws it at the aim. A
//!    tap — or no press within `grace` — throws the base size (no forced wait). A hit while holding
//!    (≥ `knock_pct` % in one go) knocks it loose: it bursts in the caster's hands.
//! 3. **Flight** (slower the bigger: `speed / (1 + speed_size × (d − 1))`):
//!    * **erasing** (Pierce + Attract: Hollow Purple) — bores a tunnel, shatters every construct it
//!      touches, launches every player once, never stops until `range`;
//!    * **explosive** — bursts on the first thing it touches (terrain, a player, a construct): the
//!      ordinary explosion, radius / damage by size (a spirit bomb leaves a crater);
//!    * **big** (≥ `plough_diameter`) and not explosive — ploughs terrain along its path and shoves
//!      players aside; a Barrier / Guard stops it.
//! 4. **Collisions** (generic energy bodies): two balls of different casters meet → they burst at
//!    the meeting point (size-weighted); a palm beam catches one → a push-back struggle
//!    (`signature::palm`, the ball pushes with its size).
//!
//! Replicated as [`GameEvent::SignatureFx`] (windup / launch / erase / end) + the construct in
//! snapshots (size_scale = growth, anim `Charging` while held) + ordinary Carve / Explosion events.
//! Deterministic, no allocation per tick, zero cost without balls.

use bd_core::input::buttons;
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo, HitKind};
use crate::constructs::{AnimState, Construct, ConstructStore};
use crate::events::{GameEvent, GoneReason};
use crate::movement::MoveModifiers;
use crate::world::World;

/// Windup gather distance past two radii ahead of the eye (m; erasing orbs).
pub const GATHER_BASE: f32 = 5.0;

/// Most live balls (older ones end early past this).
pub const MAX_ORBS: usize = 12;

/// Holding a ball this wide (m) or more: overhead, both arms up (presentation reads [`hold_view`]).
pub const OVERHEAD_DIAMETER: f32 = 3.0;

/// Move speed while holding a ball: × 1 / (1 + this × diameter).
pub const HOLD_SLOW: f32 = 0.1;

/// What a [`GameEvent::SignatureFx`] reports (presentation + audio).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SigFx {
    /// A fusion-style windup (`windup: Merge(a, b)` in `assets/signatures.ron`): an orb of each
    /// element forms at either hand and they merge over `secs` (any signature kind may carry it).
    Merge { a: Element, b: Element, secs: f32 },
    /// An energy ball leaves the hands: `dir` from `pos` at `speed` m/s for `range` m, `radius` m.
    Launch { speed: f32, range: f32, radius: f32 },
    /// It erased something at `pos` (a construct or a player hit).
    Erase,
    /// It reached its range, burst, or was stopped.
    End,
}

/// Energy-ball numbers (`tuning.signature.orb`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrbTuning {
    /// Erasing orb (a piercing Beam signature) sphere diameter (m) for a calm cast; +
    /// `diameter_shout` × power, × (1 + `full_bar_bonus`) when cast from a full bar.
    pub diameter: f32,
    pub diameter_shout: f32,
    pub full_bar_bonus: f32,
    /// Erasing-orb flight speed (m/s, at its base size) and every ball's range.
    pub speed: f32,
    pub range: f32,
    /// Tunnel: carve radius = the ball's radius × this (min 1 m), one carve every `carve_spacing` m.
    pub carve_radius_mult: f32,
    pub carve_spacing: f32,
    pub carve_power: f32,
    /// Erasing-orb player hit (once per player per orb).
    pub damage: f32,
    pub knockback: f32,
    /// Integrity damage to a construct an erasing orb touches (enough to shatter anything).
    pub construct_damage: f32,
    /// The suction wake of an erasing orb: an Attract field (radius m, strength).
    pub wake_radius: f32,
    pub wake_strength: f32,
    /// After forming: this long for a PRIMARY press before it throws itself at its base size.
    pub grace: f32,
    /// The longest hold (s) before it throws itself.
    pub hold_max: f32,
    /// Growth while held: size × per second (linear), up to `grow_max` × the base (≤ 5).
    pub grow_rate: f32,
    pub grow_max: f32,
    /// Willpower per second while growing, per metre of diameter.
    pub grow_cost: f32,
    /// A hit adding at least this much damage % while holding knocks it loose.
    pub knock_pct: f32,
    /// Thrown speed (m/s) of a 1 m ball; bigger is slower: / (1 + `speed_size` × (d − 1)); ≥ `min_speed`.
    pub throw_speed: f32,
    pub speed_size: f32,
    pub min_speed: f32,
    /// At least this wide (m) and not explosive: ploughs terrain along its path.
    pub plough_diameter: f32,
    /// Burst: radius = diameter × `blast_mult` + 1.5; damage / knockback × sqrt(diameter).
    pub blast_mult: f32,
    pub blast_damage: f32,
    pub blast_knockback: f32,
    /// A thrown ball's push in a beam struggle, per metre of diameter.
    pub push_per_m: f32,
    /// A beam pushing a ball back this far (m) bursts it.
    pub push_back: f32,
}

impl Default for OrbTuning {
    fn default() -> Self {
        Self {
            diameter: 3.5,
            diameter_shout: 0.6,
            full_bar_bonus: 0.15,
            speed: 50.0,
            range: 120.0,
            carve_radius_mult: 1.0,
            carve_spacing: 1.2,
            carve_power: 3.0,
            damage: 28.0,
            knockback: 36.0,
            construct_damage: 100_000.0,
            wake_radius: 7.0,
            wake_strength: 22.0,
            grace: 0.3,
            hold_max: 8.0,
            grow_rate: 0.7,
            grow_max: 3.0,
            grow_cost: 4.0,
            knock_pct: 4.0,
            throw_speed: 55.0,
            speed_size: 0.12,
            min_speed: 12.0,
            plough_diameter: 3.0,
            blast_mult: 1.3,
            blast_damage: 16.0,
            blast_knockback: 24.0,
            push_per_m: 0.55,
            push_back: 18.0,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl OrbTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.diameter = fin(self.diameter, d.diameter, 0.5, 12.0);
        self.diameter_shout = fin(self.diameter_shout, d.diameter_shout, 0.0, 6.0);
        self.full_bar_bonus = fin(self.full_bar_bonus, d.full_bar_bonus, 0.0, 2.0);
        self.speed = fin(self.speed, d.speed, 5.0, 300.0);
        self.range = fin(self.range, d.range, 5.0, 500.0);
        self.carve_radius_mult = fin(self.carve_radius_mult, d.carve_radius_mult, 0.0, 3.0);
        self.carve_spacing = fin(self.carve_spacing, d.carve_spacing, 0.25, 10.0);
        self.carve_power = fin(self.carve_power, d.carve_power, 0.0, 4.0);
        self.damage = fin(self.damage, d.damage, 0.0, 300.0);
        self.knockback = fin(self.knockback, d.knockback, 0.0, 120.0);
        self.construct_damage = fin(self.construct_damage, d.construct_damage, 0.0, 1.0e7);
        self.wake_radius = fin(self.wake_radius, d.wake_radius, 0.0, 30.0);
        self.wake_strength = fin(self.wake_strength, d.wake_strength, 0.0, 100.0);
        self.grace = fin(self.grace, d.grace, 0.0, 5.0);
        self.hold_max = fin(self.hold_max, d.hold_max, 0.2, 60.0);
        self.grow_rate = fin(self.grow_rate, d.grow_rate, 0.0, 10.0);
        self.grow_max = fin(self.grow_max, d.grow_max, 1.0, 5.0);
        self.grow_cost = fin(self.grow_cost, d.grow_cost, 0.0, 100.0);
        self.knock_pct = fin(self.knock_pct, d.knock_pct, 0.1, 200.0);
        self.throw_speed = fin(self.throw_speed, d.throw_speed, 5.0, 300.0);
        self.speed_size = fin(self.speed_size, d.speed_size, 0.0, 5.0);
        self.min_speed = fin(self.min_speed, d.min_speed, 1.0, 300.0);
        self.plough_diameter = fin(self.plough_diameter, d.plough_diameter, 0.5, 50.0);
        self.blast_mult = fin(self.blast_mult, d.blast_mult, 0.0, 10.0);
        self.blast_damage = fin(self.blast_damage, d.blast_damage, 0.0, 300.0);
        self.blast_knockback = fin(self.blast_knockback, d.blast_knockback, 0.0, 150.0);
        self.push_per_m = fin(self.push_per_m, d.push_per_m, 0.0, 20.0);
        self.push_back = fin(self.push_back, d.push_back, 1.0, 200.0);
        self
    }
}

/// A piercing, fired / thrown thing (not a creature, not a sky emitter): an erasing orb.
pub fn is_piercing_orb(r: &Recipe) -> bool {
    r.has(Mechanic::Pierce) && r.form.category() != FormCategory::Creature && !super::orbital::is_orbital(r)
}

/// A live erasing orb's recipe (what [`orb_recipe`] builds; the kill feed says "erased", the client
/// draws the imaginary-mass sphere). Also true after the flight strips its other verbs.
pub fn is_erasing_orb(r: &Recipe) -> bool {
    r.form == Form::Ball && r.has(Mechanic::Attract) && (r.has(Mechanic::Pierce) || r.mechanics.len() == 1) && r.scale >= 2.0
}

/// A ball of ENERGY (hold-to-grow, thrown, physical): an erasing orb, or a `Ball` that bursts
/// (`Explodes`) or carries an element, and isn't dropped from the sky ("meteor", "drop X on Ethan"
/// stay as they are) or a creature.
pub fn is_energy_ball(r: &Recipe) -> bool {
    r.form == Form::Ball && !r.has(Mechanic::DropFromSky) && (is_erasing_orb(r) || r.has(Mechanic::Explodes) || r.element != Element::Light)
}

/// The amplified orb recipe: a big hard-light Ball of the phrase's element with the suction wake.
pub fn orb_recipe(mut r: Recipe, t: &OrbTuning, power: f32, full_bar: bool) -> Recipe {
    r.form = Form::Ball;
    r.mechanics = vec![Mechanic::Pierce, Mechanic::Heavy, Mechanic::Attract];
    let d = (t.diameter + t.diameter_shout * power.clamp(0.0, 1.0)) * if full_bar { 1.0 + t.full_bar_bonus } else { 1.0 };
    r.scale = d;
    r.count = 1;
    r.payload = None;
    r.mass = r.mass.max(500.0);
    if let bd_core::mechanics::ModelSource::Icon(id) = &r.model {
        r.model = bd_core::mechanics::ModelSource::Procedural { seed: crate::rng::word_hash(id) as u32 };
    }
    r.normalized()
}

/// A held ball as everyone sees it (replicated: the construct's anim `Charging` + its size), for the
/// presentation's poses and the gather (presence): (diameter m, growth 0..1, overhead).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoldView {
    pub diameter: f32,
    pub growth: f32,
    pub overhead: bool,
}

/// `Some` while `c` is an energy ball in its caster's hands (forming or held).
pub fn hold_view(c: &Construct) -> Option<HoldView> {
    if c.recipe.form != Form::Ball || c.anim_state != AnimState::Charging || c.gone.is_some() {
        return None;
    }
    let diameter = (c.recipe.scale * c.size_scale).max(0.1);
    Some(HoldView { diameter, growth: ((c.size_scale - 1.0) / 2.0).clamp(0.0, 1.0), overhead: diameter >= OVERHEAD_DIAMETER })
}

/// MOVEMENT HOOK (`ConstructStore::movement_mods`, host and prediction alike): a caster holding a
/// ball moves slower the bigger it is.
pub fn hold_slow(store: &ConstructStore, owner: PlayerId, mods: &mut MoveModifiers) {
    for c in store.iter() {
        if c.owner == owner
            && let Some(h) = hold_view(c)
        {
            mods.speed_mult /= 1.0 + HOLD_SLOW * h.diameter;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The cast's forming time (gathering where it will be held).
    Windup,
    /// In the hands: grows while PRIMARY is held; release / grace / `hold_max` throws.
    Hold,
    Travel,
}

/// One live ball (host state).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orb {
    pub construct: ConstructId,
    pub owner: PlayerId,
    phase: Phase,
    pos: Vec3,
    dir: Vec3,
    /// Current radius (m) and at the cast (growth = radius / base).
    radius: f32,
    base_radius: f32,
    travelled: f32,
    next_carve: f32,
    /// Players already hit (bit = player id % 64).
    hit: u64,
    element: Element,
    erase: bool,
    explodes: bool,
    speed: f32,
    /// Hold: a press happened, time held, grace left, the caster's damage % last tick.
    pressed: bool,
    held_t: f32,
    grace: f32,
    pct: f32,
    /// Caught by a palm beam: `signature::palm` places it (a push-back struggle).
    clash: bool,
    /// Where the flight started (a beam struggle pushes it back toward here).
    launched_at: Vec3,
}

impl Orb {
    fn diameter(&self) -> f32 {
        self.radius * 2.0
    }
}

/// SIGNATURE HOOK (spawn): a fresh energy ball starts its windup in the caster's hands.
pub(crate) fn register(w: &mut World, cid: ConstructId) {
    let tick = w.tick;
    let Some(c) = w.constructs.get_mut(cid) else { return };
    let radius = c.half.max_element().max(0.25);
    let element = c.recipe.element;
    let erase = is_erasing_orb(&c.recipe);
    let explodes = c.recipe.has(Mechanic::Explodes);
    let owner = c.owner;
    // The flight is ours: its own verbs go quiet (an erasing orb's field waits for the release —
    // riding in the hand it would swallow its own caster).
    c.recipe.mechanics.retain(|m| *m == Mechanic::Attract);
    crate::constructs::components::on_graft(c);
    c.set_anim(AnimState::Charging, tick);
    c.vel = Vec3::ZERO;
    w.fields.retain(|f| f.construct != Some(cid));
    let pct = w.player(owner).map_or(0.0, |p| p.damage_pct);
    let o = Orb {
        construct: cid,
        owner,
        phase: Phase::Windup,
        pos: c_pos(w, cid),
        dir: Vec3::NEG_Z,
        radius,
        base_radius: radius,
        travelled: 0.0,
        next_carve: 0.0,
        hit: 0,
        element,
        erase,
        explodes,
        speed: 0.0,
        pressed: false,
        held_t: 0.0,
        grace: 0.0,
        pct,
        clash: false,
        launched_at: Vec3::ZERO,
    };
    let list = &mut w.signature.orbs;
    if list.len() >= MAX_ORBS {
        let old = list.remove(0);
        if let Some(c) = w.constructs.get_mut(old.construct) {
            c.gone = Some(GoneReason::Spent);
        }
    }
    w.signature.orbs.push(o);
}

fn c_pos(w: &World, cid: ConstructId) -> Vec3 {
    w.constructs.get(cid).map_or(Vec3::ZERO, |c| c.pos)
}

/// Where a ball waits for its caster: erasing orbs ahead (a 4 m sphere in the hands would hide the
/// view), big ones overhead, small ones between the palms.
fn hold_point(eye: Vec3, look: Vec3, o: &Orb) -> Vec3 {
    if o.erase {
        // CAMERA: far enough out that a 3-4 m sphere doesn't fill the view.
        eye + look * (o.radius * 2.0 + GATHER_BASE) + Vec3::Y * (o.radius * 0.5)
    } else if o.diameter() >= OVERHEAD_DIAMETER {
        eye + Vec3::Y * (0.9 + o.radius) + Vec3::new(look.x, 0.0, look.z).normalize_or_zero() * 0.4
    } else {
        eye + look * (0.55 + o.radius) - Vec3::Y * 0.3
    }
}

/// World::tick, after constructs: hold, grow, throw, fly.
pub(crate) fn step(w: &mut World, dt: f32) {
    if w.signature.orbs.is_empty() {
        return;
    }
    let t = w.cfg.tuning.signature.orb.clone();
    let base = w.cfg.move_params.clone();
    let tick = w.tick;
    let mut i = 0;
    while i < w.signature.orbs.len() {
        let mut o = w.signature.orbs[i];
        let Some(c) = w.constructs.get(o.construct).filter(|c| c.gone.is_none()) else {
            w.events.push(GameEvent::SignatureFx { id: o.construct, owner: o.owner, fx: SigFx::End, pos: o.pos, dir: o.dir });
            w.signature.orbs.remove(i);
            continue;
        };
        let forming = !c.active();
        let mut burst: Option<(Vec3, bool)> = None;
        match o.phase {
            Phase::Windup | Phase::Hold => {
                let Some(p) = w.player(o.owner).filter(|p| p.alive) else {
                    // The caster is gone: it bursts where it was.
                    burst = Some((o.pos, true));
                    finish(w, &mut o, &t, burst);
                    w.signature.orbs.remove(i);
                    continue;
                };
                let (eye, look, held, pct, wp) = (p.eye(&base), p.look(), p.input.held(buttons::PRIMARY), p.damage_pct, p.willpower);
                o.dir = look;
                // A hard hit while holding knocks it loose (the counter to the huge ones).
                let knocked = pct >= o.pct + t.knock_pct;
                o.pct = pct;
                if knocked {
                    burst = Some((o.pos, true));
                } else if o.phase == Phase::Windup {
                    o.pressed |= held;
                    if !forming {
                        o.phase = Phase::Hold;
                        o.grace = t.grace;
                    }
                } else {
                    o.held_t += dt;
                    o.pressed |= held;
                    let grow_cap = o.base_radius * t.grow_max.min(5.0);
                    if o.pressed && held && wp > 0.0 && o.radius < grow_cap {
                        let r = (o.radius + o.base_radius * t.grow_rate * dt).min(grow_cap);
                        let f = r / o.radius.max(1e-3);
                        o.radius = r;
                        if let Some(c) = w.constructs.get_mut(o.construct) {
                            c.resize(f);
                        }
                        if let Some(p) = w.player_mut(o.owner) {
                            p.willpower = (p.willpower - t.grow_cost * r * 2.0 * dt).max(0.0);
                            p.regen_pause = p.regen_pause.max(0.5);
                        }
                    }
                    o.grace -= dt;
                    let release = (o.pressed && !held) || (!o.pressed && o.grace <= 0.0) || o.held_t >= t.hold_max;
                    if release {
                        launch(w, &mut o, &t, look);
                    }
                }
                if o.phase != Phase::Travel {
                    o.pos = hold_point(eye, look, &o);
                    if let Some(c) = w.constructs.get_mut(o.construct) {
                        c.pos = o.pos;
                        c.vel = Vec3::ZERO;
                        c.set_anim(AnimState::Charging, tick);
                    }
                }
            }
            Phase::Travel => {
                if !o.clash {
                    let step = o.speed * dt;
                    let from = o.pos;
                    o.pos += o.dir * step;
                    o.travelled += step;
                    if o.erase {
                        erase(w, &mut o, &t);
                    } else {
                        burst = fly(w, &mut o, &t, from, step);
                    }
                }
            }
        }
        if o.phase == Phase::Travel && burst.is_none() {
            if let Some(c) = w.constructs.get_mut(o.construct) {
                c.pos = o.pos;
                // Zero: it is placed, not thrown (a "thrown" body would add its own contact hits).
                c.vel = Vec3::ZERO;
                c.set_anim(AnimState::Move, tick);
            }
            // Range reached, or about to leave the map (before the blast zones would recall it).
            let b = &w.arena.bounds;
            let out = o.pos.x < b.min.x - 10.0
                || o.pos.x > b.max.x + 10.0
                || o.pos.z < b.min.z - 10.0
                || o.pos.z > b.max.z + 10.0
                || o.pos.y < b.min.y - 10.0
                || o.pos.y > b.max.y + 10.0;
            if o.travelled >= t.range || out {
                burst = if o.explodes && !out { Some((o.pos, false)) } else { None };
                finish(w, &mut o, &t, burst);
                w.signature.orbs.remove(i);
                continue;
            }
        }
        if burst.is_some() {
            finish(w, &mut o, &t, burst);
            w.signature.orbs.remove(i);
            continue;
        }
        w.signature.orbs[i] = o;
        i += 1;
    }
    collide(w, &t);
}

fn launch(w: &mut World, o: &mut Orb, t: &OrbTuning, look: Vec3) {
    o.phase = Phase::Travel;
    let dir = look.normalize_or(Vec3::NEG_Z);
    o.dir = dir;
    let d = o.diameter();
    let base = if o.erase { t.speed * (o.base_radius / o.radius).sqrt() } else { t.throw_speed / (1.0 + t.speed_size * (d - 1.0).max(0.0)) };
    o.speed = base.max(t.min_speed);
    o.launched_at = o.pos;
    if o.erase && t.wake_radius > 0.0 && t.wake_strength > 0.0 {
        let secs = t.range / o.speed + 0.2;
        let _ = w.create_field(o.owner, o.pos, t.wake_radius, t.wake_strength, secs, Some(o.construct));
    }
    w.events.push(GameEvent::SignatureFx {
        id: o.construct,
        owner: o.owner,
        fx: SigFx::Launch { speed: o.speed, range: t.range, radius: o.radius },
        pos: o.pos,
        dir,
    });
}

/// The ball is done: burst (an explosion by size) or just end.
fn finish(w: &mut World, o: &mut Orb, t: &OrbTuning, burst: Option<(Vec3, bool)>) {
    if let Some((at, loose)) = burst {
        blast(w, o, t, at, 1.0, loose);
    }
    if let Some(c) = w.constructs.get_mut(o.construct) {
        c.gone = Some(GoneReason::Spent);
    }
    w.events.push(GameEvent::SignatureFx { id: o.construct, owner: o.owner, fx: SigFx::End, pos: o.pos, dir: o.dir });
}

/// The ordinary explosion, sized by the ball (`loose`: knocked out of the caster's hands — it hurts
/// them too).
fn blast(w: &mut World, o: &Orb, t: &OrbTuning, at: Vec3, mult: f32, loose: bool) {
    let d = o.diameter();
    let k = d.max(0.5).sqrt() * mult;
    w.explode(Explosion {
        pos: at,
        radius: (d * t.blast_mult + 1.5) * mult.sqrt(),
        damage: t.blast_damage * k,
        knockback: t.blast_knockback * k,
        source: Some(o.owner),
        construct: Some(o.construct),
        element: o.element,
        carve_radius: (d * 0.5 + 1.0) * mult.sqrt(),
        self_damage: loose,
    });
}

/// A non-erasing ball's flight this tick: burst on contact (explosive), or plough (big).
fn fly(w: &mut World, o: &mut Orb, t: &OrbTuning, from: Vec3, step: f32) -> Option<(Vec3, bool)> {
    let plough = !o.explodes && o.diameter() >= t.plough_diameter;
    // Terrain.
    if let Some(h) = bd_core::voxel::raycast(&w.voxels, from, o.dir, step + o.radius * 0.8) {
        if !plough {
            return Some((h.point - o.dir * (o.radius * 0.3), false));
        }
    }
    if plough {
        let carve_r = (o.radius * t.carve_radius_mult).max(1.0);
        while o.travelled >= o.next_carve {
            let at = o.pos - o.dir * (o.travelled - o.next_carve);
            o.next_carve += t.carve_spacing;
            w.request_carve(CarveRequest { center: at, radius: carve_r, power: t.carve_power * 0.6 });
        }
    }
    // Constructs: a Barrier / Guard / indestructible stops anything; others burst an explosive ball
    // or get shoved by a plough.
    for c in w.constructs.iter() {
        if c.id == o.construct || c.owner == o.owner || c.gone.is_some() || is_orb(w, c.id) {
            continue;
        }
        let reach = o.radius + c.half.max_element();
        if (c.center() - o.pos).length_squared() > reach * reach || !crate::geom::sphere_obb(o.pos, o.radius, c.center(), c.half, c.rot) {
            continue;
        }
        let blocks = c.recipe.has(Mechanic::Barrier) || c.recipe.has(Mechanic::Guard) || !c.integrity.is_finite();
        if blocks || o.explodes {
            return Some((o.pos, false));
        }
    }
    // Players: an explosive ball bursts on them; a plough shoves them aside (once each).
    let base = w.cfg.move_params.clone();
    let mut shove = [(PlayerId(0), Vec3::ZERO); bd_core::MAX_PLAYERS];
    let mut n = 0;
    for p in &w.players {
        if p.id == o.owner || !p.alive || p.spawn_shield > 0.0 || n >= shove.len() {
            continue;
        }
        let h = p.hull(&base);
        let reach = o.radius + 0.35;
        if crate::geom::dist_sq_point_aabb(&h, o.pos) > reach * reach {
            continue;
        }
        if o.explodes {
            return Some((o.pos, false));
        }
        let bit = 1u64 << (p.id.0 as u32 % 64);
        if o.hit & bit == 0 {
            o.hit |= bit;
            shove[n] = (p.id, h.center());
            n += 1;
        }
    }
    for &(pid, c) in &shove[..n] {
        let side = (c - o.pos).normalize_or(o.dir);
        let dir = (side + o.dir + Vec3::Y * 0.4).normalize_or(o.dir);
        let k = o.diameter().sqrt();
        let hit = HitInfo::new(pid, Some(o.owner), HitKind::Shot, t.blast_damage * 0.5 * k, t.blast_knockback * k, dir, c)
            .with_element(o.element)
            .with_construct(o.construct)
            .with_impact(1.0);
        w.apply_hit(hit);
    }
    None
}

/// Two balls of different casters meet: both burst at the meeting point (size-weighted).
fn collide(w: &mut World, t: &OrbTuning) {
    let n = w.signature.orbs.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let (a, b) = (w.signature.orbs[i], w.signature.orbs[j]);
            if a.phase != Phase::Travel || b.phase != Phase::Travel || a.owner == b.owner {
                continue;
            }
            if a.pos.distance(b.pos) > a.radius + b.radius {
                continue;
            }
            let wa = a.radius * a.radius;
            let wb = b.radius * b.radius;
            let at = (a.pos * wb + b.pos * wa) / (wa + wb).max(1e-4);
            let big = if a.radius >= b.radius { a } else { b };
            let mult = 1.0 + (a.radius.min(b.radius) / big.radius.max(1e-3));
            blast(w, &big, t, at, mult, false);
            for o in [a, b] {
                if let Some(c) = w.constructs.get_mut(o.construct) {
                    c.gone = Some(GoneReason::Spent);
                }
                w.events.push(GameEvent::SignatureFx { id: o.construct, owner: o.owner, fx: SigFx::End, pos: at, dir: o.dir });
            }
            // The spent pair leaves the list next tick (their constructs are gone).
            return;
        }
    }
}

/// Everything an erasing orb touches this tick: terrain (tunnel), constructs (shattered), players
/// (launched once).
fn erase(w: &mut World, o: &mut Orb, t: &OrbTuning) {
    // Tunnel.
    let carve_r = (o.radius * t.carve_radius_mult).max(1.0);
    while t.carve_radius_mult > 0.0 && o.travelled >= o.next_carve {
        let back = o.travelled - o.next_carve;
        let at = o.pos - o.dir * back;
        o.next_carve += t.carve_spacing;
        w.request_carve(CarveRequest { center: at, radius: carve_r, power: t.carve_power });
    }
    // Constructs (anything but the caster's own).
    let mut victims = [ConstructId(0); 8];
    let mut n = 0;
    for c in w.constructs.iter() {
        if c.id == o.construct || c.owner == o.owner || c.gone.is_some() || n >= victims.len() || is_orb(w, c.id) {
            continue;
        }
        let reach = o.radius + c.half.max_element();
        if (c.center() - o.pos).length_squared() <= reach * reach && crate::geom::sphere_obb(o.pos, o.radius, c.center(), c.half, c.rot) {
            victims[n] = c.id;
            n += 1;
        }
    }
    for &id in &victims[..n] {
        let point = w.constructs.get(id).map_or(o.pos, |c| c.center());
        w.apply_construct_hit(ConstructHit {
            target: id,
            source: Some(o.owner),
            damage: t.construct_damage,
            impulse: o.dir * 20.0,
            blade: false,
            pierce: true,
            blunt: false,
            point,
        });
        w.events.push(GameEvent::SignatureFx { id: o.construct, owner: o.owner, fx: SigFx::Erase, pos: point, dir: o.dir });
    }
    // Players (once each).
    let base = w.cfg.move_params.clone();
    let mut hits = [(PlayerId(0), Vec3::ZERO); bd_core::MAX_PLAYERS];
    let mut m = 0;
    for p in &w.players {
        if p.id == o.owner || !p.alive || m >= hits.len() {
            continue;
        }
        let bit = 1u64 << (p.id.0 as u32 % 64);
        if o.hit & bit != 0 {
            continue;
        }
        let h = p.hull(&base);
        let reach = o.radius + 0.35;
        if crate::geom::dist_sq_point_aabb(&h, o.pos) <= reach * reach {
            o.hit |= bit;
            hits[m] = (p.id, h.center());
            m += 1;
        }
    }
    for &(pid, c) in &hits[..m] {
        let dir = (o.dir + Vec3::Y * 0.35).normalize_or(o.dir);
        let hit = HitInfo::new(pid, Some(o.owner), HitKind::Shot, t.damage, t.knockback, dir, c)
            .with_element(o.element)
            .with_construct(o.construct)
            .with_impact(1.6);
        w.apply_hit(hit);
        w.events.push(GameEvent::SignatureFx { id: o.construct, owner: o.owner, fx: SigFx::Erase, pos: c, dir: o.dir });
    }
}

// ------------------------------------------------------------------ palm-beam struggle (palm.rs)

/// Is `cid` a live energy ball (any phase)?
pub(crate) fn is_orb(w: &World, cid: ConstructId) -> bool {
    w.signature.orbs.iter().any(|o| o.construct == cid)
}

/// A thrown ball (in flight) whose sphere touches the segment `a`–`b` (not `owner`'s): (id, where).
pub(crate) fn touching(w: &World, a: Vec3, b: Vec3, r: f32, owner: PlayerId) -> Option<(ConstructId, Vec3)> {
    w.signature.orbs.iter().find_map(|o| {
        if o.phase != Phase::Travel || o.owner == owner || o.clash {
            return None;
        }
        let ab = b - a;
        let k = if ab.length_squared() > 1e-8 { ((o.pos - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        let q = a + ab * k;
        (q.distance(o.pos) <= o.radius + r).then_some((o.construct, o.pos - (o.pos - q).normalize_or_zero() * 0.0))
    })
}

/// In flight and not spent.
pub(crate) fn live(w: &World, cid: ConstructId) -> bool {
    w.signature.orbs.iter().any(|o| o.construct == cid && o.phase == Phase::Travel) && w.constructs.get(cid).is_some_and(|c| c.gone.is_none())
}

/// A beam holds the ball at `at` (`None`: released — it flies on).
pub(crate) fn hold(w: &mut World, cid: ConstructId, at: Option<Vec3>) {
    if let Some(o) = w.signature.orbs.iter_mut().find(|o| o.construct == cid) {
        o.clash = at.is_some();
        if let Some(p) = at {
            o.pos = p;
        }
    }
    if let (Some(p), Some(c)) = (at, w.constructs.get_mut(cid)) {
        c.pos = p;
        c.vel = Vec3::ZERO;
    }
}

/// (push in a beam struggle, where it was thrown from, its owner).
pub(crate) fn struggle(w: &World, cid: ConstructId) -> Option<(f32, Vec3, PlayerId, f32)> {
    let t = &w.cfg.tuning.signature.orb;
    w.signature.orbs.iter().find(|o| o.construct == cid).map(|o| (t.push_per_m * o.diameter() * if o.erase { 1.6 } else { 1.0 }, o.launched_at, o.owner, o.radius))
}

/// The struggle is lost: the ball bursts at `at` (the palm module blasts; this ends it).
pub(crate) fn spend(w: &mut World, cid: ConstructId, at: Vec3) {
    let t = w.cfg.tuning.signature.orb.clone();
    if let Some(i) = w.signature.orbs.iter().position(|o| o.construct == cid) {
        let mut o = w.signature.orbs.remove(i);
        o.pos = at;
        finish(w, &mut o, &t, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piercing_fired_things_are_orbs() {
        let mut r = Recipe::unknown("hollow purple");
        r.form = Form::Cannon;
        r.mechanics = vec![Mechanic::Pierce, Mechanic::Explodes, Mechanic::Heavy];
        let r = r.normalized();
        assert!(is_piercing_orb(&r));
        let o = orb_recipe(r.clone(), &OrbTuning::default(), 0.0, false);
        assert_eq!(o.form, Form::Ball);
        assert!(o.has(Mechanic::Attract) && !o.has(Mechanic::Explodes));
        assert!((o.scale - 3.5).abs() < 1e-4);
        assert!(is_erasing_orb(&o));
        assert!(is_energy_ball(&o));
        assert_eq!(crate::freedom::verb_text(crate::freedom::verb_for(&o)), "erased");
        let mut plain = Recipe::unknown("ball");
        plain.form = Form::Ball;
        assert!(!is_erasing_orb(&plain.clone().normalized()));
        assert!(!is_energy_ball(&plain.normalized()), "a plain ball is a ball");
        let big = orb_recipe(r.clone(), &OrbTuning::default(), 1.0, true);
        assert!(big.scale > o.scale * 1.2);
        let mut beast = r;
        beast.form = Form::Quadruped;
        assert!(!is_piercing_orb(&beast), "creatures keep their body");
    }

    #[test]
    fn energy_balls_are_a_rule_not_names() {
        let ball = |mechs: &[Mechanic], e: Element| {
            let mut r = Recipe::unknown("x");
            r.form = Form::Ball;
            r.mechanics = mechs.to_vec();
            r.element = e;
            r.normalized()
        };
        assert!(is_energy_ball(&ball(&[Mechanic::Explodes], Element::Fire)), "fireball");
        assert!(is_energy_ball(&ball(&[Mechanic::Spin], Element::Wind)), "rasengan");
        assert!(is_energy_ball(&ball(&[], Element::Electric)), "ball of lightning");
        assert!(!is_energy_ball(&ball(&[Mechanic::Explodes, Mechanic::DropFromSky], Element::Fire)), "a meteor stays a meteor");
        assert!(!is_energy_ball(&ball(&[Mechanic::Heavy], Element::Light)), "a bowling ball");
    }

    #[test]
    fn hostile_tuning_is_clamped() {
        let t = OrbTuning { speed: f32::NAN, range: -4.0, carve_power: 99.0, grow_max: 99.0, ..Default::default() }.validated();
        assert_eq!(t.speed, OrbTuning::default().speed);
        assert!(t.range >= 5.0 && t.carve_power <= 4.0 && t.grow_max <= 5.0);
        assert_eq!(OrbTuning::default().validated(), OrbTuning::default());
    }
}
