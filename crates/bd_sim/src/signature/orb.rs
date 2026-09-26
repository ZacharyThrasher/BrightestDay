//! Erasing orbs — "HOLLOW PURPLE" and every other piercing ultimate: a Beam signature whose phrase
//! resolved to something that `Pierce`s (goes through) and is fired / thrown, not a creature or an
//! orbital emitter ([`is_piercing_orb`]). No per-name code: the amplifier makes it a big hard-light
//! `Ball` (+ `Attract`: the existing force field that follows a construct is the suction wake), and
//! this module flies it:
//!
//! 1. **Windup**: it forms in the caster's hand for the signature windup (the caster keeps moving and
//!    aiming; the Attract field already pulls at nearby enemies — the "blue" half).
//! 2. **Release**: it leaves along the caster's aim at `speed` m/s — fast but readable — and never
//!    stops for anything until `range` m: it bores a cylindrical tunnel (a `carve_radius` carve every
//!    `carve_spacing` m, `carve_power` ≥ every destructible hardness), shatters every construct it
//!    touches (not its caster's), and launches every player it touches once (`damage` %, big
//!    `knockback` through the ordinary Smash formula: survivable at low %, a KO from mid %). Dodge
//!    i-frames and movement are the counterplay (the ordinary hit path honours them).
//! 3. It ends at `range` (the construct is spent).
//!
//! Replicated as [`GameEvent::SignatureFx`] (windup / launch / erase / end) + the construct in
//! snapshots + ordinary `Carve` events. Deterministic, no allocation per tick, zero cost without orbs.

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use crate::combat::{CarveRequest, ConstructHit, HitInfo, HitKind};
use crate::events::{GameEvent, GoneReason};
use crate::world::World;

/// Most live orbs (older ones end early past this).
pub const MAX_ORBS: usize = 8;

/// What a [`GameEvent::SignatureFx`] reports (presentation + audio).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SigFx {
    /// A fusion-style windup (`windup: Merge(a, b)` in `assets/signatures.ron`): an orb of each
    /// element forms at either hand and they merge over `secs` (any signature kind may carry it).
    Merge { a: Element, b: Element, secs: f32 },
    /// An erasing orb leaves the hand: `dir` from `pos` at `speed` m/s for `range` m, `radius` m.
    Launch { speed: f32, range: f32, radius: f32 },
    /// It erased something at `pos` (a construct or a player hit).
    Erase,
    /// It reached its range (or was stopped).
    End,
}

/// Erasing-orb numbers (`tuning.signature.orb`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrbTuning {
    /// Sphere diameter (m) for a calm cast; + `diameter_shout` × power, × (1 + `full_bar_bonus`) when
    /// cast from a full willpower bar (charged up).
    pub diameter: f32,
    pub diameter_shout: f32,
    pub full_bar_bonus: f32,
    pub speed: f32,
    pub range: f32,
    /// Tunnel: carve radius = the orb's radius × this (min 1 m), one carve every `carve_spacing` m.
    pub carve_radius_mult: f32,
    pub carve_spacing: f32,
    pub carve_power: f32,
    /// Player hit (once per player per orb).
    pub damage: f32,
    pub knockback: f32,
    /// Integrity damage to a touched construct (enough to shatter anything).
    pub construct_damage: f32,
    /// The suction wake: an Attract field (radius m, strength) that rides the orb once it is released.
    pub wake_radius: f32,
    pub wake_strength: f32,
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
        self
    }
}

/// A piercing, fired / thrown thing (not a creature, not a sky emitter): an erasing orb.
pub fn is_piercing_orb(r: &Recipe) -> bool {
    r.has(Mechanic::Pierce) && r.form.category() != FormCategory::Creature && !super::orbital::is_orbital(r)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Windup,
    Travel,
}

/// One live orb (host state).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orb {
    pub construct: ConstructId,
    pub owner: PlayerId,
    phase: Phase,
    pos: Vec3,
    dir: Vec3,
    radius: f32,
    travelled: f32,
    next_carve: f32,
    /// Players already hit (bit = player id % 64).
    hit: u64,
    element: Element,
}

/// SIGNATURE HOOK (spawn): a fresh erasing orb starts its windup in the hand.
pub(crate) fn register(w: &mut World, cid: ConstructId) {
    let Some(c) = w.constructs.get_mut(cid) else { return };
    let radius = c.half.max_element().max(0.5);
    let element = c.recipe.element;
    // The flight is ours: its own verbs go quiet (the field that followed it from the hand waits for
    // the release — riding in the hand it would swallow its own caster).
    c.recipe.mechanics.retain(|m| *m == Mechanic::Attract);
    crate::constructs::components::on_graft(c);
    w.fields.retain(|f| f.construct != Some(cid));
    let o = Orb {
        construct: cid,
        owner: c.owner,
        phase: Phase::Windup,
        pos: c.pos,
        dir: Vec3::NEG_Z,
        radius,
        travelled: 0.0,
        next_carve: 0.0,
        hit: 0,
        element,
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

/// World::tick, after constructs: windup in the hand, then the straight erasing flight.
pub(crate) fn step(w: &mut World, dt: f32) {
    if w.signature.orbs.is_empty() {
        return;
    }
    let t = w.cfg.tuning.signature.orb.clone();
    let mut i = 0;
    while i < w.signature.orbs.len() {
        let mut o = w.signature.orbs[i];
        let Some(c) = w.constructs.get(o.construct).filter(|c| c.gone.is_none()) else {
            w.events.push(GameEvent::SignatureFx { id: o.construct, owner: o.owner, fx: SigFx::End, pos: o.pos, dir: o.dir });
            w.signature.orbs.remove(i);
            continue;
        };
        let forming = !c.active();
        let cpos = c.pos;
        match o.phase {
            Phase::Windup => {
                // It gathers ahead of and above the caster (a sphere this big in the hand would hide
                // everything); aim follows the caster until the release.
                o.pos = cpos;
                if let Some(p) = w.player(o.owner) {
                    o.dir = p.look();
                    let eye = p.eye(&w.cfg.move_params);
                    o.pos = eye + o.dir * (o.radius + 3.0) + Vec3::Y * (o.radius * 0.5);
                }
                if let Some(c) = w.constructs.get_mut(o.construct) {
                    c.pos = o.pos;
                    c.vel = Vec3::ZERO;
                }
                if !forming {
                    o.phase = Phase::Travel;
                    let dir = o.dir.normalize_or(Vec3::NEG_Z);
                    o.dir = dir;
                    if t.wake_radius > 0.0 && t.wake_strength > 0.0 {
                        let secs = t.range / t.speed + 0.2;
                        let _ = w.create_field(o.owner, o.pos, t.wake_radius, t.wake_strength, secs, Some(o.construct));
                    }
                    w.events.push(GameEvent::SignatureFx {
                        id: o.construct,
                        owner: o.owner,
                        fx: SigFx::Launch { speed: t.speed, range: t.range, radius: o.radius },
                        pos: o.pos,
                        dir,
                    });
                }
            }
            Phase::Travel => {
                let step = t.speed * dt;
                o.pos += o.dir * step;
                o.travelled += step;
                erase(w, &mut o, &t);
            }
        }
        if o.phase == Phase::Travel {
            if let Some(c) = w.constructs.get_mut(o.construct) {
                c.pos = o.pos;
                // Zero: it is placed, not thrown (a "thrown" body would add its own contact hits).
                c.vel = Vec3::ZERO;
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
                if let Some(c) = w.constructs.get_mut(o.construct) {
                    c.gone = Some(GoneReason::Spent);
                }
                w.events.push(GameEvent::SignatureFx { id: o.construct, owner: o.owner, fx: SigFx::End, pos: o.pos, dir: o.dir });
                w.signature.orbs.remove(i);
                continue;
            }
        }
        w.signature.orbs[i] = o;
        i += 1;
    }
}

/// Everything the sphere touches this tick: terrain (tunnel), constructs (shattered), players
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
        if c.id == o.construct || c.owner == o.owner || c.gone.is_some() || n >= victims.len() {
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
        let big = orb_recipe(r.clone(), &OrbTuning::default(), 1.0, true);
        assert!(big.scale > o.scale * 1.2);
        let mut beast = r;
        beast.form = Form::Quadruped;
        assert!(!is_piercing_orb(&beast), "creatures keep their body");
    }

    #[test]
    fn hostile_tuning_is_clamped() {
        let t = OrbTuning { speed: f32::NAN, range: -4.0, carve_power: 99.0, ..Default::default() }.validated();
        assert_eq!(t.speed, OrbTuning::default().speed);
        assert!(t.range >= 5.0 && t.carve_power <= 4.0);
        assert_eq!(OrbTuning::default().validated(), OrbTuning::default());
    }
}
