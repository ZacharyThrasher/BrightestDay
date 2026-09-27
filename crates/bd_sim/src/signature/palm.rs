//! Palm beams — kamehameha, final flash, galick gun, repulsor… (`Form::Hands`, the HandBeam
//! archetype) as PHYSICAL, sustained beams (owner: "make them physical objects so they destroy the
//! environment and could clash"). No per-name code: every Hands shot is one of these.
//!
//! * **Fire**: the HandBeam release (`constructs::gun`, a charge-scaled `HitscanShot`) is routed here
//!   ([`fire`], one hook in `world/combat.rs`) instead of an instant ray. The beam's HEAD leaves the
//!   palms at `speed` m/s and grows to `range`; it lives `secs` (plain ≈ 0.6 s … ultimate ≈ 1.5 s by
//!   caliber, a little longer at full charge); width by caliber.
//! * **Steer**: the root follows the caster's palms; the direction turns toward their aim at a capped
//!   rate (an ultimate is heavy). Firing never locks the caster: they move, jump, dodge freely.
//! * **Physical**: the head stops at terrain and BORES through it (a carve at the tip every
//!   `carve_every`, radius by width — an ultimate tunnels a hill in a blink, a plain beam chips);
//!   a construct in the way takes damage every pulse and breaks, or BLOCKS it (Barrier / Guard /
//!   indestructible: the beam ends at its face). Every player along it is hit every `pulse` s
//!   (damage + a push along the beam; pierces).
//! * **Clash** (the beam struggle; control is never taken): two beams of different casters whose
//!   heads / segments meet within their widths lock at a clash point (both end there). The point
//!   drifts toward the side with less PUSH (charge at fire × size, + holding PRIMARY pours willpower
//!   in at `hold_cost`/s). While clashing the beams hold (the clash has its own timer): stop pouring and
//!   you are overwhelmed; dodge / die / get knocked out and the clash dissolves and the other beam
//!   blasts on. When the point reaches a caster, or after `clash_max` s, it DETONATES: a crater and
//!   a radial blast weighted toward the loser. An erasing orb (Hollow Purple) caught by a beam clashes
//!   the same way (the orb holds still and pushes with `orb_push`).
//! * **Bots** sometimes counter-beam a beam coming at them ([`bot_counter`], `bots::drive`).
//!
//! Replicated as [`GameEvent::PalmBeam`] ([`BeamView`]: start, ~40 Hz updates, clash, end,
//! detonation) + ordinary Carve / Hit events. Deterministic (slice order, no floats from
//! hash maps), no allocation per tick, zero cost without beams.

use bd_core::input::buttons;
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{ConstructId, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo, HitKind};
use crate::events::GameEvent;
use crate::weapons::{GunArchetype, HitscanShot};
use crate::world::World;

/// Palm-beam numbers (`tuning.signature.palm`). `*_plain` at caliber 1, `*_ult` at the ultimate
/// caliber (2.5); in between by caliber.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PalmTuning {
    /// Head speed (m/s): fast but visible.
    pub speed: f32,
    pub range: f32,
    pub radius_plain: f32,
    pub radius_ult: f32,
    pub secs_plain: f32,
    pub secs_ult: f32,
    /// Extra life at full charge (× secs).
    pub secs_charge_bonus: f32,
    /// Steering (deg/s).
    pub turn_plain: f32,
    pub turn_ult: f32,
    /// Hits along the beam every `pulse` s: the shot's damage × `pulse_damage`, knockback × `pulse_knockback`.
    pub pulse: f32,
    pub pulse_damage: f32,
    pub pulse_knockback: f32,
    /// Construct integrity damage per pulse = the pulse damage × this.
    pub construct_mult: f32,
    /// Boring: a carve at the tip every `carve_every_*` s, radius = width × `carve_radius_mult`.
    pub carve_every_plain: f32,
    pub carve_every_ult: f32,
    pub carve_radius_mult: f32,
    pub carve_power_plain: f32,
    pub carve_power_ult: f32,
    /// Clash: push = (`push_base` + charge) × (1 + caliber t) (+ `hold_push` while pouring).
    pub push_base: f32,
    pub hold_push: f32,
    /// Willpower per second while pouring into a clash.
    pub hold_cost: f32,
    /// Clash point drift (m/s per unit of push difference) and its cap.
    pub clash_drift: f32,
    pub clash_drift_max: f32,
    pub clash_max: f32,
    /// The point this close to a caster's palms detonates on them.
    pub clash_reach: f32,
    /// Detonation: radius = `blast_radius` + both widths × 2; damage / knockback at the centre.
    pub blast_radius: f32,
    pub blast_damage: f32,
    pub blast_knockback: f32,
    pub blast_loser: f32,
    pub blast_winner: f32,
    /// An erasing orb's push in a clash.
    pub orb_push: f32,
    /// Bots: chance to counter-beam a beam coming at them (≥ `bot_willpower`; costs `bot_cost`).
    pub bot_chance: f32,
    pub bot_willpower: f32,
    pub bot_cost: f32,
    /// Most live beams (older ones end early past this).
    pub max_beams: usize,
    /// A blocker (Barrier / Guard) takes this fraction of the beam's construct damage.
    pub block_damage: f32,
}

impl Default for PalmTuning {
    fn default() -> Self {
        Self {
            speed: 150.0,
            range: 160.0,
            radius_plain: 0.35,
            radius_ult: 1.5,
            secs_plain: 0.6,
            secs_ult: 1.5,
            secs_charge_bonus: 0.2,
            turn_plain: 70.0,
            turn_ult: 32.0,
            pulse: 0.1,
            pulse_damage: 0.22,
            pulse_knockback: 0.3,
            construct_mult: 2.5,
            carve_every_plain: 0.2,
            carve_every_ult: 0.03,
            carve_radius_mult: 1.15,
            carve_power_plain: 1.0,
            carve_power_ult: 3.0,
            push_base: 0.6,
            hold_push: 1.0,
            hold_cost: 20.0,
            clash_drift: 5.0,
            clash_drift_max: 14.0,
            clash_max: 4.0,
            clash_reach: 2.5,
            blast_radius: 6.0,
            blast_damage: 26.0,
            blast_knockback: 34.0,
            blast_loser: 1.5,
            blast_winner: 0.4,
            orb_push: 1.8,
            bot_chance: 0.4,
            bot_willpower: 35.0,
            bot_cost: 25.0,
            max_beams: 12,
            block_damage: 0.05,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl PalmTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.speed = fin(self.speed, d.speed, 10.0, 600.0);
        self.range = fin(self.range, d.range, 5.0, 500.0);
        self.radius_plain = fin(self.radius_plain, d.radius_plain, 0.05, 6.0);
        self.radius_ult = fin(self.radius_ult, d.radius_ult, 0.05, 8.0);
        self.secs_plain = fin(self.secs_plain, d.secs_plain, 0.1, 10.0);
        self.secs_ult = fin(self.secs_ult, d.secs_ult, 0.1, 10.0);
        self.secs_charge_bonus = fin(self.secs_charge_bonus, d.secs_charge_bonus, 0.0, 2.0);
        self.turn_plain = fin(self.turn_plain, d.turn_plain, 0.0, 720.0);
        self.turn_ult = fin(self.turn_ult, d.turn_ult, 0.0, 720.0);
        self.pulse = fin(self.pulse, d.pulse, 0.02, 1.0);
        self.pulse_damage = fin(self.pulse_damage, d.pulse_damage, 0.0, 5.0);
        self.pulse_knockback = fin(self.pulse_knockback, d.pulse_knockback, 0.0, 5.0);
        self.construct_mult = fin(self.construct_mult, d.construct_mult, 0.0, 100.0);
        self.carve_every_plain = fin(self.carve_every_plain, d.carve_every_plain, 0.01, 5.0);
        self.carve_every_ult = fin(self.carve_every_ult, d.carve_every_ult, 0.01, 5.0);
        self.carve_radius_mult = fin(self.carve_radius_mult, d.carve_radius_mult, 0.0, 4.0);
        self.carve_power_plain = fin(self.carve_power_plain, d.carve_power_plain, 0.0, 4.0);
        self.carve_power_ult = fin(self.carve_power_ult, d.carve_power_ult, 0.0, 4.0);
        self.push_base = fin(self.push_base, d.push_base, 0.0, 10.0);
        self.hold_push = fin(self.hold_push, d.hold_push, 0.0, 10.0);
        self.hold_cost = fin(self.hold_cost, d.hold_cost, 0.0, 200.0);
        self.clash_drift = fin(self.clash_drift, d.clash_drift, 0.0, 100.0);
        self.clash_drift_max = fin(self.clash_drift_max, d.clash_drift_max, 0.0, 200.0);
        self.clash_max = fin(self.clash_max, d.clash_max, 0.2, 20.0);
        self.clash_reach = fin(self.clash_reach, d.clash_reach, 0.5, 20.0);
        self.blast_radius = fin(self.blast_radius, d.blast_radius, 0.0, 40.0);
        self.blast_damage = fin(self.blast_damage, d.blast_damage, 0.0, 300.0);
        self.blast_knockback = fin(self.blast_knockback, d.blast_knockback, 0.0, 150.0);
        self.blast_loser = fin(self.blast_loser, d.blast_loser, 0.0, 5.0);
        self.blast_winner = fin(self.blast_winner, d.blast_winner, 0.0, 5.0);
        self.orb_push = fin(self.orb_push, d.orb_push, 0.0, 20.0);
        self.bot_chance = fin(self.bot_chance, d.bot_chance, 0.0, 1.0);
        self.bot_willpower = fin(self.bot_willpower, d.bot_willpower, 0.0, 200.0);
        self.bot_cost = fin(self.bot_cost, d.bot_cost, 0.0, 200.0);
        self.max_beams = self.max_beams.clamp(1, 64);
        self.block_damage = fin(self.block_damage, d.block_damage, 0.0, 10.0);
        self
    }

    /// (radius, secs at no charge, turn rad/s, carve every, carve power) at caliber `k` (0 plain … 1 ultimate).
    fn at(&self, k: f32) -> (f32, f32, f32, f32, f32) {
        let l = |a: f32, b: f32| a + (b - a) * k;
        (
            l(self.radius_plain, self.radius_ult),
            l(self.secs_plain, self.secs_ult),
            l(self.turn_plain, self.turn_ult).to_radians(),
            l(self.carve_every_plain, self.carve_every_ult),
            l(self.carve_power_plain, self.carve_power_ult),
        )
    }
}

/// Where a beam is in its life (replicated).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BeamPhase {
    /// Pouring out (the first report of a beam is its start).
    Live,
    /// Locked in a clash at `point`; `heat` 0..1 grows toward the detonation.
    Clash { point: Vec3, heat: f32 },
    /// Gone (let go, spent, blocked caster, KO).
    End,
    /// The clash blew up at `point` (both beams end).
    Detonate { point: Vec3, radius: f32 },
}

/// One beam's replicated state ([`GameEvent::PalmBeam`]): the client draws from `origin` along
/// `dir` for `len` m (a clash: to the clash point), `radius` wide, in the `element` colour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeamView {
    pub id: u32,
    pub owner: PlayerId,
    pub construct: Option<ConstructId>,
    pub origin: Vec3,
    pub dir: Vec3,
    pub len: f32,
    pub radius: f32,
    pub element: Element,
    /// 0 plain … 1 ultimate (look / sound size).
    pub size: f32,
    /// Seconds of life left (0 = unknown / ending).
    pub left: f32,
    /// Head speed (m/s) for client extrapolation.
    pub speed: f32,
    pub phase: BeamPhase,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Beam {
    pub id: u32,
    pub owner: PlayerId,
    pub construct: Option<ConstructId>,
    pub origin: Vec3,
    pub dir: Vec3,
    pub len: f32,
    pub radius: f32,
    pub element: Element,
    /// Caliber 0..1, charge at fire 0..1.
    pub k: f32,
    pub charge: f32,
    pub left: f32,
    turn: f32,
    carve_every: f32,
    carve_power: f32,
    pulse_t: f32,
    carve_t: f32,
    dmg: f32,
    kb: f32,
    pub clash: Option<u32>,
    /// A bot's counter-beam: aimed at `target`, pours while it has willpower.
    bot: Option<PlayerId>,
    /// Ends this tick.
    done: bool,
    /// The head is on terrain (boring).
    boring: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Partner {
    Beam(u32),
    Orb(ConstructId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Clash {
    pub id: u32,
    pub a: u32,
    pub b: Partner,
    pub point: Vec3,
    /// Where it formed.
    pub start: Vec3,
    pub t: f32,
}

#[derive(Debug, Default)]
pub(crate) struct PalmState {
    pub beams: Vec<Beam>,
    pub clashes: Vec<Clash>,
    next_id: u32,
    ticks: u32,
    /// (beam id, bot) counter-beam decisions already rolled (bounded).
    rolled: Vec<(u32, PlayerId)>,
}

/// A Barrier / Guard / indestructible construct stops a beam (and a thrown ball).
pub(crate) fn blocks(c: &crate::constructs::Construct) -> bool {
    c.recipe.has(Mechanic::Barrier) || c.recipe.has(Mechanic::Guard) || !c.integrity.is_finite()
}

/// Caliber 0 (plain) … 1 (ultimate) from a Hands construct's scale.
pub fn caliber_t(scale: f32) -> f32 {
    let d = crate::constructs::forms::form_spec(Form::Hands).default_scale.max(0.1);
    let k = if scale.is_finite() { (scale / d).sqrt().clamp(1.0, 2.5) } else { 1.0 };
    (k - 1.0) / 1.5
}

/// SIGNATURE HOOK (world/combat.rs): a HandBeam shot becomes a sustained palm beam. `false` = not
/// ours (an ordinary hitscan).
pub(crate) fn fire(w: &mut World, h: &HitscanShot) -> bool {
    let Some(cid) = h.construct else { return false };
    let Some(c) = w.constructs.get(cid) else { return false };
    if c.recipe.form != Form::Hands {
        return false;
    }
    // One beam per pair of hands at a time (a re-fire while pouring is swallowed).
    if w.signature.palm.beams.iter().any(|b| b.construct == Some(cid) && !b.done) {
        return true;
    }
    let spec = w.cfg.tuning.spec_for(&c.recipe, GunArchetype::HandBeam);
    let charge = if spec.damage > 1e-3 { ((h.damage / spec.damage - 0.35) / 0.65).clamp(0.0, 1.0) } else { 1.0 };
    let (k, element, owner) = (caliber_t(c.recipe.scale), c.recipe.element, c.owner);
    // MODIFIERS & FUSION: a pair of hands with `count` > 1 ("triple kamehameha") fires a FAN of beams
    // that then steer back onto the aim like any beam (each a little weaker).
    let n = (c.recipe.count as usize).clamp(1, MAX_HANDS as usize);
    let per = if n > 1 { HANDS_DAMAGE } else { 1.0 };
    for j in 0..n {
        let yaw = match j {
            0 => 0.0,
            1 => HANDS_FAN_DEG,
            _ => -HANDS_FAN_DEG,
        };
        let dir = if yaw == 0.0 { h.dir } else { (bd_core::Quat::from_rotation_y(yaw.to_radians()) * h.dir).normalize_or(h.dir) };
        start(w, owner, Some(cid), element, k, charge, h.damage * per, h.knockback * per, h.origin, dir, None);
    }
    true
}

/// Most beams one pair of hands fires at once (a "triple kamehameha").
pub const MAX_HANDS: u8 = 3;
/// Fan between those beams (deg) and each one's share of the shot's damage / knockback.
pub const HANDS_FAN_DEG: f32 = 9.0;
pub const HANDS_DAMAGE: f32 = 0.7;

#[allow(clippy::too_many_arguments)]
fn start(
    w: &mut World,
    owner: PlayerId,
    construct: Option<ConstructId>,
    element: Element,
    k: f32,
    charge: f32,
    damage: f32,
    knockback: f32,
    eye: Vec3,
    dir: Vec3,
    bot: Option<PlayerId>,
) {
    let t = &w.cfg.tuning.signature.palm;
    let (radius, secs, turn, carve_every, carve_power) = t.at(k.clamp(0.0, 1.0));
    let dir = dir.normalize_or(Vec3::NEG_Z);
    let st = &mut w.signature.palm;
    st.next_id = st.next_id.wrapping_add(1).max(1);
    let b = Beam {
        id: st.next_id,
        owner,
        construct,
        origin: palms(eye, dir),
        dir,
        len: 0.0,
        radius,
        element,
        k,
        charge,
        left: secs * (1.0 + t.secs_charge_bonus * charge),
        turn,
        carve_every,
        carve_power,
        pulse_t: 0.0,
        carve_t: 0.0,
        dmg: damage.max(0.0) * t.pulse_damage,
        kb: knockback.max(0.0) * t.pulse_knockback,
        clash: None,
        bot,
        done: false,
        boring: false,
    };
    if st.beams.len() >= t.max_beams {
        st.beams[0].done = true;
    }
    st.beams.push(b);
    let v = view(&b, t.speed, BeamPhase::Live);
    w.events.push(GameEvent::PalmBeam(v));
}

/// The beam's root: in front of and a little below the eye (the palms).
fn palms(eye: Vec3, look: Vec3) -> Vec3 {
    eye + look * 0.6 - Vec3::Y * 0.25
}

fn view(b: &Beam, speed: f32, phase: BeamPhase) -> BeamView {
    BeamView {
        id: b.id,
        owner: b.owner,
        construct: b.construct,
        origin: b.origin,
        dir: b.dir,
        len: b.len,
        radius: b.radius,
        element: b.element,
        size: b.k,
        left: b.left.max(0.0),
        speed,
        phase,
    }
}

/// Rotate `from` toward `to` by at most `max` radians.
fn turn_toward(from: Vec3, to: Vec3, max: f32) -> Vec3 {
    let to = to.normalize_or(from);
    let ang = from.dot(to).clamp(-1.0, 1.0).acos();
    if ang <= max || ang < 1e-5 {
        return to;
    }
    let axis = from.cross(to);
    let axis = if axis.length_squared() > 1e-10 { axis.normalize() } else { from.any_orthonormal_vector() };
    (bd_core::Quat::from_axis_angle(axis, max) * from).normalize_or(from)
}

/// Closest points of segments p0-p1 and q0-q1 → (distance, midpoint).
fn seg_seg(p0: Vec3, p1: Vec3, q0: Vec3, q1: Vec3) -> (f32, Vec3) {
    let (d1, d2, r) = (p1 - p0, q1 - q0, p0 - q0);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (s, t) = if a <= 1e-8 && e <= 1e-8 {
        (0.0, 0.0)
    } else if a <= 1e-8 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 1e-8 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let den = a * e - b * b;
            let mut s = if den > 1e-8 { ((b * f - c * e) / den).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    let (cp, cq) = (p0 + d1 * s, q0 + d2 * t);
    (cp.distance(cq), (cp + cq) * 0.5)
}

fn dist_point_seg(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let k = if ab.length_squared() > 1e-8 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * k)
}

/// Live beams' replicated views (practice / host presentation, tests).
pub fn views(w: &World) -> Vec<BeamView> {
    let st = &w.signature.palm;
    let speed = w.cfg.tuning.signature.palm.speed;
    st.beams
        .iter()
        .map(|b| {
            let phase = match b.clash.and_then(|id| st.clashes.iter().find(|c| c.id == id)) {
                Some(c) => BeamPhase::Clash { point: c.point, heat: c.t / w.cfg.tuning.signature.palm.clash_max },
                None => BeamPhase::Live,
            };
            view(b, speed, phase)
        })
        .collect()
}

/// World::tick, after constructs (`signature::step`).
pub(crate) fn step(w: &mut World, dt: f32) {
    if w.signature.palm.beams.is_empty() && w.signature.palm.clashes.is_empty() {
        return;
    }
    let t = w.cfg.tuning.signature.palm.clone();
    let mut beams = std::mem::take(&mut w.signature.palm.beams);
    let mut clashes = std::mem::take(&mut w.signature.palm.clashes);
    w.signature.palm.ticks = w.signature.palm.ticks.wrapping_add(1);
    let sync = w.signature.palm.ticks.is_multiple_of(3);
    let base = w.cfg.move_params.clone();

    // 1. Follow the caster, steer, pour / run down.
    for b in beams.iter_mut() {
        let Some(p) = w.player(b.owner).filter(|p| p.alive && !p.mv.extra.dodging() && p.seat.is_none()) else {
            b.done = true;
            continue;
        };
        let eye = p.eye(&base);
        let aim = match b.bot.and_then(|tp| w.player(tp)) {
            Some(tp) => (tp.eye(&base) - eye).normalize_or(p.look()),
            // Inside their own domain the caster's beam can't miss: it swings onto the nearest enemy.
            None => super::domain::sure_target(w, b.owner, eye).map_or(p.look(), |at| (at - eye).normalize_or(p.look())),
        };
        let pouring = b.clash.is_some() && p.willpower > 0.0 && (if b.bot.is_some() { true } else { p.input.held(buttons::PRIMARY) });
        if b.clash.is_none() {
            b.dir = turn_toward(b.dir, aim, b.turn * dt);
        }
        b.origin = palms(eye, b.dir);
        // In a clash the beams hold (the clash has its own timer); pouring willpower adds push.
        if pouring && let Some(p) = w.player_mut(b.owner) {
            p.willpower = (p.willpower - t.hold_cost * dt).max(0.0);
            p.regen_pause = p.regen_pause.max(0.5);
        }
        if b.clash.is_none() {
            b.left -= dt;
        }
        if b.left <= 0.0 {
            b.done = true;
        }
    }

    // 2. Clashes: drift, detonate, or dissolve.
    let mut ci = 0;
    while ci < clashes.len() {
        let mut c = clashes[ci];
        let ai = beams.iter().position(|b| b.id == c.a && !b.done);
        let bi = match c.b {
            Partner::Beam(id) => beams.iter().position(|b| b.id == id && !b.done).map(Some),
            Partner::Orb(cid) => super::orb::live(w, cid).then_some(None),
        };
        let (Some(ai), Some(bi)) = (ai, bi) else {
            // One side let go: the other blasts on.
            for b in beams.iter_mut().filter(|b| b.clash == Some(c.id)) {
                b.clash = None;
            }
            if let Partner::Orb(cid) = c.b {
                super::orb::hold(w, cid, None);
            }
            clashes.remove(ci);
            continue;
        };
        c.t += dt;
        let push = |b: &Beam, w: &World| {
            let pouring = w.player(b.owner).is_some_and(|p| p.willpower > 0.0 && (b.bot.is_some() || p.input.held(buttons::PRIMARY)));
            (t.push_base + b.charge) * (1.0 + b.k) + if pouring { t.hold_push } else { 0.0 }
        };
        let pa = push(&beams[ai], w);
        let (pb, b_origin, rb, b_owner) = match bi {
            Some(bi) => (push(&beams[bi], w), beams[bi].origin, beams[bi].radius, beams[bi].owner),
            // A thrown ball pushes with its size; it is driven back toward where it was thrown from.
            None => match super::orb::struggle(w, c.b_orb()) {
                Some((push, from, owner, r)) => (push, from, r, owner),
                None => (t.orb_push, c.point, 1.0, beams[ai].owner),
            },
        };
        let a_origin = beams[ai].origin;
        // Along the line between the two roots: toward the weaker side.
        let axis = (b_origin - a_origin).normalize_or(beams[ai].dir);
        let v = ((pa - pb) * t.clash_drift).clamp(-t.clash_drift_max, t.clash_drift_max);
        c.point += axis * v * dt;
        // Keep it between the two (a clash never pushes behind a caster).
        let span = a_origin.distance(b_origin).max(1e-3);
        let s = ((c.point - a_origin).dot(axis) / span).clamp(0.0, 1.0);
        let off = c.point - (a_origin + axis * (s * span));
        c.point = a_origin + axis * (s * span) + off * 0.9;
        let da = c.point.distance(a_origin);
        let db = c.point.distance(b_origin);
        {
            let a = &mut beams[ai];
            a.len = da;
            a.dir = (c.point - a.origin).normalize_or(a.dir);
        }
        match bi {
            Some(bi) => {
                let b = &mut beams[bi];
                b.len = db;
                b.dir = (c.point - b.origin).normalize_or(b.dir);
            }
            None => super::orb::hold(w, c.b_orb(), Some(c.point)),
        }
        // A ball pushed back far enough (or all the way home) bursts.
        let ball_beaten = bi.is_none() && (db <= t.clash_reach || c.point.distance(c.start) >= w.cfg.tuning.signature.orb.push_back);
        let loser = if da <= t.clash_reach {
            Some(0)
        } else if (db <= t.clash_reach && bi.is_some()) || ball_beaten {
            Some(1)
        } else if c.t >= t.clash_max {
            Some(if da / (da + db).max(1e-3) < 0.5 { 0 } else { 1 })
        } else {
            None
        };
        if let Some(l) = loser {
            let owners = [beams[ai].owner, b_owner];
            let radius = t.blast_radius + (beams[ai].radius + rb) * 2.0;
            detonate(w, &t, c.point, radius, owners[l], owners[1 - l], beams[ai].element);
            let va = view(&beams[ai], t.speed, BeamPhase::Detonate { point: c.point, radius });
            w.events.push(GameEvent::PalmBeam(va));
            beams[ai].done = true;
            match bi {
                Some(bi) => beams[bi].done = true,
                None => super::orb::spend(w, c.b_orb(), c.point),
            }
            clashes.remove(ci);
            continue;
        }
        clashes[ci] = c;
        ci += 1;
    }

    // 3. Heads: grow, stop at terrain (and bore), stop at blockers; pulses hit what's along them.
    for b in beams.iter_mut() {
        if b.done {
            continue;
        }
        if b.clash.is_none() {
            let want = (b.len + t.speed * dt).min(t.range);
            let hit = bd_core::voxel::raycast(&w.voxels, b.origin, b.dir, want);
            b.boring = hit.is_some();
            b.len = hit.map_or(want, |h| h.t.min(want));
            // Blockers: the first Barrier / Guard / indestructible construct in the way.
            let end = b.origin + b.dir * b.len;
            for c in w.constructs.iter() {
                if c.owner == b.owner || c.gone.is_some() || Some(c.id) == b.construct || super::orb::is_orb(w, c.id) {
                    continue;
                }
                // (A domain's barrier is a sphere, handled below — not its box.)
                if !blocks(c) || super::domain::is_domain(c) {
                    continue;
                }
                if let Some((f, _)) = crate::geom::segment_obb(b.origin, end, c.center(), c.half, c.rot, b.radius * 0.5) {
                    b.len = b.len.min(f * b.len);
                    b.boring = false;
                }
            }
            // A sealed domain's barrier stops it (either way) and takes the pulses.
            if let Some((f, id)) = super::domain::segment_block(w, b.origin, b.origin + b.dir * b.len) {
                b.len = b.len.min(f * b.len);
                b.boring = false;
                b.pulse_t -= dt;
                if b.pulse_t <= 0.0 {
                    let at = b.origin + b.dir * b.len;
                    super::domain::hit_barrier(w, id, b.dmg * t.construct_mult, at);
                }
            }
        }
        // Boring: chew the terrain at the tip.
        b.carve_t -= dt;
        if b.boring && b.carve_t <= 0.0 && t.carve_radius_mult > 0.0 {
            b.carve_t = b.carve_every;
            let at = b.origin + b.dir * (b.len + b.radius * 0.3);
            w.request_carve(CarveRequest { center: at, radius: (b.radius * t.carve_radius_mult).max(0.5), power: b.carve_power });
            if matches!(b.element, Element::Fire | Element::Atomic) {
                w.ignite_terrain(at, b.radius, Some(b.owner));
            }
        }
        // Pulses.
        b.pulse_t -= dt;
        if b.pulse_t <= 0.0 {
            b.pulse_t += t.pulse;
            pulse(w, b, &t);
        }
    }

    // 4. New clashes (beam vs beam of another caster; beam vs an erasing orb).
    for i in 0..beams.len() {
        if beams[i].done || beams[i].clash.is_some() || beams[i].len < 0.5 {
            continue;
        }
        for j in (i + 1)..beams.len() {
            let (a, b) = (&beams[i], &beams[j]);
            if b.done || b.clash.is_some() || b.owner == a.owner || b.len < 0.5 {
                continue;
            }
            let (d, point) = seg_seg(a.origin, a.origin + a.dir * a.len, b.origin, b.origin + b.dir * b.len);
            if d <= a.radius + b.radius {
                let st = &mut w.signature.palm;
                st.next_id = st.next_id.wrapping_add(1).max(1);
                let id = st.next_id;
                clashes.push(Clash { id, a: a.id, b: Partner::Beam(b.id), point, start: point, t: 0.0 });
                beams[i].clash = Some(id);
                beams[j].clash = Some(id);
                break;
            }
        }
        if beams[i].clash.is_some() {
            continue;
        }
        let (o, e, r, owner) = (beams[i].origin, beams[i].origin + beams[i].dir * beams[i].len, beams[i].radius, beams[i].owner);
        if let Some((cid, pos)) = super::orb::touching(w, o, e, r, owner) {
            let st = &mut w.signature.palm;
            st.next_id = st.next_id.wrapping_add(1).max(1);
            let id = st.next_id;
            clashes.push(Clash { id, a: beams[i].id, b: Partner::Orb(cid), point: pos, start: pos, t: 0.0 });
            beams[i].clash = Some(id);
            super::orb::hold(w, cid, Some(pos));
        }
    }

    // 5. Report and retire.
    let mut i = 0;
    while i < beams.len() {
        let b = beams[i];
        if b.done {
            w.events.push(GameEvent::PalmBeam(view(&b, t.speed, BeamPhase::End)));
            beams.remove(i);
            continue;
        }
        if sync {
            let phase = match b.clash.and_then(|id| clashes.iter().find(|c| c.id == id)) {
                Some(c) => BeamPhase::Clash { point: c.point, heat: (c.t / t.clash_max).min(1.0) },
                None => BeamPhase::Live,
            };
            w.events.push(GameEvent::PalmBeam(view(&b, t.speed, phase)));
        }
        i += 1;
    }
    // A clash whose beams all retired dissolves.
    clashes.retain(|c| beams.iter().any(|b| b.clash == Some(c.id)));
    let st = &mut w.signature.palm;
    st.beams = beams;
    st.clashes = clashes;
    let live: Vec<u32> = st.beams.iter().map(|b| b.id).collect();
    st.rolled.retain(|(id, _)| live.contains(id));
}

impl Clash {
    fn b_orb(&self) -> ConstructId {
        match self.b {
            Partner::Orb(c) => c,
            Partner::Beam(_) => ConstructId(0),
        }
    }
}

/// Everything along the beam this pulse: players (damage + a push along it) and constructs (damage;
/// the non-blocking ones break and let it through).
fn pulse(w: &mut World, b: &Beam, t: &PalmTuning) {
    let (a, e) = (b.origin, b.origin + b.dir * b.len);
    let base = w.cfg.move_params.clone();
    let mut hits = [(PlayerId(0), Vec3::ZERO); bd_core::MAX_PLAYERS];
    let mut n = 0;
    for p in &w.players {
        if p.id == b.owner || !p.alive || p.spawn_shield > 0.0 || n >= hits.len() {
            continue;
        }
        let c = p.hull(&base).center();
        if dist_point_seg(c, a, e) <= b.radius + 0.45 {
            hits[n] = (p.id, c);
            n += 1;
        }
    }
    for &(pid, c) in &hits[..n] {
        let dir = (b.dir + Vec3::Y * 0.2).normalize_or(b.dir);
        let mut hit = HitInfo::new(pid, Some(b.owner), HitKind::Shot, b.dmg, b.kb, dir, c).with_element(b.element).with_impact(0.5);
        if let Some(cid) = b.construct {
            hit = hit.with_construct(cid);
        }
        w.apply_hit(hit);
    }
    let mut victims = [(ConstructId(0), Vec3::ZERO, 1.0f32); 8];
    let mut m = 0;
    for c in w.constructs.iter() {
        if c.owner == b.owner || c.gone.is_some() || Some(c.id) == b.construct || m >= victims.len() || super::orb::is_orb(w, c.id) {
            continue;
        }
        if let Some((f, _)) = crate::geom::segment_obb(a, e, c.center(), c.half, c.rot, b.radius) {
            // A blocker holds (it takes a fraction); anything else is chewed through.
            let k = if blocks(c) { t.block_damage } else { 1.0 };
            victims[m] = (c.id, a + (e - a) * f, k);
            m += 1;
        }
    }
    for &(id, point, k) in &victims[..m] {
        w.apply_construct_hit(ConstructHit {
            target: id,
            source: Some(b.owner),
            damage: b.dmg * t.construct_mult * k,
            impulse: b.dir * b.kb,
            blade: false,
            pierce: true,
            blunt: false,
            point,
        });
    }
}

/// The clash blows up: a crater and a radial blast, heavier on the loser.
fn detonate(w: &mut World, t: &PalmTuning, at: Vec3, radius: f32, loser: PlayerId, winner: PlayerId, element: Element) {
    // The ordinary explosion (crater, blast, visuals), credited to the winner (who only feels the
    // push), then the loser's extra share.
    w.explode(Explosion {
        pos: at,
        radius,
        damage: t.blast_damage,
        knockback: t.blast_knockback * t.blast_winner.max(0.1),
        source: Some(winner),
        construct: None,
        element,
        carve_radius: radius * 0.6,
        self_damage: false,
    });
    if loser == winner {
        return;
    }
    let base = w.cfg.move_params.clone();
    let Some(p) = w.player(loser).filter(|p| p.alive) else { return };
    let c = p.hull(&base).center();
    let d = c.distance(at);
    if d > radius {
        return;
    }
    let k = (1.0 - d / radius).max(0.3) * (t.blast_loser - 1.0).max(0.0);
    let dir = ((c - at).normalize_or(Vec3::Y) + Vec3::Y * 0.5).normalize_or(Vec3::Y);
    let hit = HitInfo::new(loser, Some(winner), HitKind::Explosion, t.blast_damage * k, t.blast_knockback * k, dir, c).with_impact(1.4);
    w.apply_hit(hit);
}

/// Dev / screenshots: `a` and `b` fire ultimate palm beams at each other (aimed at each other, pouring
/// while they have willpower) — a beam struggle for a bystander to watch.
pub fn dev_duel(w: &mut World, a: PlayerId, b: PlayerId) {
    let base = w.cfg.move_params.clone();
    let (Some(ea), Some(eb)) = (w.player(a).map(|p| p.eye(&base)), w.player(b).map(|p| p.eye(&base))) else { return };
    let dmg = w.cfg.tuning.gun_spec(GunArchetype::HandBeam).damage;
    start(w, a, None, Element::Light, 1.0, 1.0, dmg * 2.5, 40.0, ea, eb - ea, Some(b));
    start(w, b, None, Element::Psychic, 1.0, 1.0, dmg * 2.5, 40.0, eb, ea - eb, Some(a));
}

/// Dev / screenshots: `a` fires an ultimate palm beam at `b` (pouring) while `b` hurls a ball of
/// lightning straight down it — a beam pushing back a thrown ball, for a bystander to watch.
pub fn dev_ball_clash(w: &mut World, a: PlayerId, b: PlayerId) {
    let base = w.cfg.move_params.clone();
    let (Some(ea), Some(eb)) = (w.player(a).map(|p| p.eye(&base)), w.player(b).map(|p| p.eye(&base))) else { return };
    let dmg = w.cfg.tuning.gun_spec(GunArchetype::HandBeam).damage;
    start(w, a, None, Element::Light, 1.0, 1.0, dmg * 2.5, 40.0, ea, eb - ea, Some(b));
    let mut r = bd_core::mechanics::Recipe::unknown("ball of lightning");
    r.form = Form::Ball;
    r.element = Element::Electric;
    r.scale = 2.5;
    let r = r.normalized();
    if let Some(p) = w.player_mut(b) {
        p.willpower = 100.0;
    }
    let dir = (ea - eb).normalize_or(Vec3::Z);
    if let Ok(cid) = w.cast(b, r, None, crate::world::Aim::new(eb, dir)) {
        super::orb::dev_launch(w, cid, dir);
    }
}

/// BOT HOOK (`bots::drive`): a bot with enough willpower sometimes answers a beam coming at it with
/// its own (one roll per beam; deterministic).
pub fn bot_counter(w: &mut World, bot: PlayerId) {
    if w.signature.palm.beams.is_empty() {
        return;
    }
    let t = w.cfg.tuning.signature.palm.clone();
    let base = w.cfg.move_params.clone();
    let Some(p) = w.player(bot).filter(|p| p.alive && p.willpower >= t.bot_willpower.max(t.bot_cost)) else { return };
    if w.signature.palm.beams.iter().any(|b| b.owner == bot && !b.done) {
        return;
    }
    let eye = p.eye(&base);
    let element = Element::Light;
    let incoming = w.signature.palm.beams.iter().find(|b| {
        !b.done && b.owner != bot && b.clash.is_none() && {
            let to = eye - b.origin;
            let ahead = to.dot(b.dir);
            ahead > 4.0 && ahead < t.range && (to - b.dir * ahead).length() < 6.0 + b.radius
        }
    });
    let Some(&b) = incoming else { return };
    if w.signature.palm.rolled.iter().any(|&(id, who)| id == b.id && who == bot) {
        return;
    }
    if w.signature.palm.rolled.len() < 256 {
        w.signature.palm.rolled.push((b.id, bot));
    }
    let mut h = (b.id as u64) ^ ((bot.0 as u64) << 32) ^ 0xB3A7_0000;
    h = h.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 31;
    if ((h % 10_000) as f32 / 10_000.0) >= t.bot_chance {
        return;
    }
    if let Some(p) = w.player_mut(bot) {
        p.willpower -= t.bot_cost;
    }
    let dir = (b.origin - eye).normalize_or(-b.dir);
    start(w, bot, None, element, 0.4, 0.7, 36.0, 18.0, eye, dir, Some(b.owner));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_meet() {
        let (d, p) = seg_seg(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(5.0, 1.0, -5.0), Vec3::new(5.0, 1.0, 5.0));
        assert!((d - 1.0).abs() < 1e-4 && (p - Vec3::new(5.0, 0.5, 0.0)).length() < 1e-4);
        let (d, _) = seg_seg(Vec3::ZERO, Vec3::X, Vec3::new(3.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0));
        assert!((d - 2.0).abs() < 1e-4);
    }

    #[test]
    fn steering_is_capped() {
        let d = turn_toward(Vec3::NEG_Z, Vec3::X, 0.1);
        assert!((d.dot(Vec3::NEG_Z).acos() - 0.1).abs() < 1e-3);
        assert_eq!(turn_toward(Vec3::NEG_Z, Vec3::NEG_Z, 0.1), Vec3::NEG_Z);
    }

    #[test]
    fn ultimates_are_bigger_and_longer() {
        let t = PalmTuning::default();
        let (rp, sp, tp, cp, _) = t.at(0.0);
        let (ru, su, tu, cu, _) = t.at(1.0);
        assert!(ru > rp * 3.0 && su > sp * 2.0 && tu < tp && cu < cp);
        assert!((0.5..=0.7).contains(&sp) && (1.2..=1.81).contains(&(su * (1.0 + t.secs_charge_bonus))));
        assert_eq!(caliber_t(f32::NAN), 0.0);
    }

    #[test]
    fn hostile_tuning_is_clamped() {
        let t = PalmTuning { speed: f32::NAN, range: -1.0, max_beams: 0, ..Default::default() }.validated();
        assert_eq!(t.speed, PalmTuning::default().speed);
        assert!(t.range >= 5.0 && t.max_beams >= 1);
        assert_eq!(PalmTuning::default().validated(), PalmTuning::default());
    }
}
