//! Orbital strikes — "SATELLITE DEATH BEAM", "UFO abduction beam", "dragon breath from the sky".
//!
//! Not a new verb: the combination the lexicon already produces — an emitter (`Shoots` / `Beam` /
//! `Breath`) that `Flies` and is placed with `DropFromSky` — is read as "hovering high above the aim
//! point, striking down" ([`is_orbital`]). Everything else is existing mechanics:
//!
//! 1. **Placement** ([`spawn_point`]): `altitude` metres above the ground at the aim point (clamped to
//!    the map's ceiling), nudged along the aim azimuth so it sits within ~`view_elev_deg` of the
//!    caster's view when that keeps the beam within 45° of vertical. It hovers there (its own
//!    Flies / emitters are switched off on the live construct: the strike below replaces them).
//! 2. **Telegraph**: a ground TARGET MARKER at the impact point for `telegraph` s before the first
//!    beam — [`GameEvent::OrbitalStrike`] `phase: Telegraph` (everyone sees it; counterplay).
//! 3. **The strike**: for `strike_secs` the beam comes down from the emitter to the marker point and
//!    **tracks** at ≤ `slew` m/s — the nearest enemy near the point with `Homing`, else the caster's
//!    crosshair ground point — so good movement outruns it. Pulses every `pulse` s: the existing Beam
//!    hit (damage × Heavy/Fast traits, element status), a `Hitscan` event for the visuals, and every
//!    `carve_every` s a terrain carve. `Attract` → it lifts (abducts) instead of blasting; `Breath` →
//!    a wider element column (fire sets the ground alight); `Shoots` → a hail of bolts at the point.
//! 4. Then `cooldown` s and the next strike (new telegraph) while the construct lives.
//!
//! Deterministic (fixed order, no RNG), no allocation per tick (fixed-capacity list), zero cost with
//! no orbitals. Host-only state; clients see the construct (snapshots) + the events.

use bd_core::mechanics::{Element, FormCategory, Mechanic, Recipe};
use bd_core::{ConstructId, DT, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use crate::combat::{CarveRequest, HitInfo, HitKind};
use crate::events::GameEvent;
use crate::world::World;

/// Most live orbitals (the oldest is forgotten — its construct just hovers — past this).
pub const MAX_ORBITALS: usize = 16;

/// Phase of a strike (replicated in [`GameEvent::OrbitalStrike`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum StrikePhase {
    /// The ground marker shows at `point` for `secs` (no damage yet).
    Telegraph = 0,
    /// The beam is live for `secs` (it tracks; `Hitscan` events carry the moving end point).
    Strike = 1,
    /// The strike ended (cooldown before the next telegraph).
    End = 2,
}

/// Orbital-strike numbers (`tuning.signature.orbital`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrbitalTuning {
    /// Hover height above the ground under the aim point (m), and its clamp.
    pub altitude: f32,
    pub altitude_min: f32,
    pub altitude_max: f32,
    /// Stay at least this far below the map ceiling (m).
    pub ceiling_margin: f32,
    /// Try to sit within this elevation (deg) of the caster's eye.
    pub view_elev_deg: f32,
    /// Ground-marker seconds before each strike.
    pub telegraph: f32,
    pub strike_secs: f32,
    pub cooldown: f32,
    /// Max speed (m/s) the beam's ground point moves while tracking.
    pub slew: f32,
    /// With Homing: follow the nearest enemy within this distance of the point (m).
    pub track_radius: f32,
    /// Beam radius at the ground (m) (Breath ×2), scaled a little by construct size.
    pub beam_radius: f32,
    /// Seconds between beam pulses (hits + visuals).
    pub pulse: f32,
    pub damage: f32,
    pub knockback: f32,
    pub carve_every: f32,
    pub carve_radius: f32,
    /// Attract (abduction): upward acceleration in the beam (m/s², gravity is 25), up to `abduct_max` m/s.
    pub abduct_accel: f32,
    pub abduct_max: f32,
    /// THE orbital cost knob: existing cast cost (autonomous + scale) × this.
    pub cost_mult: f32,
}

impl Default for OrbitalTuning {
    fn default() -> Self {
        Self {
            altitude: 30.0,
            altitude_min: 22.0,
            altitude_max: 40.0,
            ceiling_margin: 6.0,
            view_elev_deg: 32.0,
            telegraph: 0.7,
            strike_secs: 2.5,
            cooldown: 3.0,
            slew: 6.5,
            track_radius: 12.0,
            beam_radius: 1.4,
            pulse: 0.1,
            damage: 3.0,
            knockback: 5.0,
            carve_every: 0.3,
            carve_radius: 1.2,
            abduct_accel: 40.0,
            abduct_max: 8.0,
            cost_mult: 1.25,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl OrbitalTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.altitude_min = fin(self.altitude_min, d.altitude_min, 4.0, 120.0);
        self.altitude_max = fin(self.altitude_max, d.altitude_max, self.altitude_min, 150.0);
        self.altitude = fin(self.altitude, d.altitude, self.altitude_min, self.altitude_max);
        self.ceiling_margin = fin(self.ceiling_margin, d.ceiling_margin, 0.0, 50.0);
        self.view_elev_deg = fin(self.view_elev_deg, d.view_elev_deg, 5.0, 85.0);
        self.telegraph = fin(self.telegraph, d.telegraph, 0.0, 5.0);
        self.strike_secs = fin(self.strike_secs, d.strike_secs, 0.1, 30.0);
        self.cooldown = fin(self.cooldown, d.cooldown, 0.0, 60.0);
        self.slew = fin(self.slew, d.slew, 0.0, 100.0);
        self.track_radius = fin(self.track_radius, d.track_radius, 0.0, 100.0);
        self.beam_radius = fin(self.beam_radius, d.beam_radius, 0.2, 10.0);
        self.pulse = fin(self.pulse, d.pulse, DT, 2.0);
        self.damage = fin(self.damage, d.damage, 0.0, 100.0);
        self.knockback = fin(self.knockback, d.knockback, 0.0, 100.0);
        self.carve_every = fin(self.carve_every, d.carve_every, DT, 10.0);
        self.carve_radius = fin(self.carve_radius, d.carve_radius, 0.0, 6.0);
        self.abduct_accel = fin(self.abduct_accel, d.abduct_accel, 0.0, 200.0);
        self.abduct_max = fin(self.abduct_max, d.abduct_max, 0.0, 60.0);
        self.cost_mult = fin(self.cost_mult, d.cost_mult, 0.0, 10.0);
        self
    }
}

/// An emitter that flies and is dropped from the sky strikes from orbit.
pub fn is_orbital(r: &Recipe) -> bool {
    !matches!(r.form.category(), FormCategory::Held | FormCategory::Worn)
        && r.has(Mechanic::DropFromSky)
        && r.has(Mechanic::Flies)
        && (r.has(Mechanic::Beam) || r.has(Mechanic::Shoots) || r.has(Mechanic::Breath))
}

/// What comes down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Emit {
    Beam,
    Breath,
    Shots,
}

/// One live orbital emitter (host state).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbital {
    pub construct: ConstructId,
    pub owner: PlayerId,
    /// Where it hovers.
    pub anchor: Vec3,
    /// The beam's ground point (the marker while telegraphing).
    pub point: Vec3,
    pub phase: StrikePhase,
    /// Seconds into the phase (Cooldown uses `End`).
    pub t: f32,
    emit: Emit,
    homing: bool,
    abduct: bool,
    element: Element,
    dmg_mult: f32,
    radius: f32,
    pulse_t: f32,
    carve_t: f32,
}

/// Placement: (hover point, ground aim point).
pub fn spawn_point(w: &World, caster_eye: Vec3, aim_point: Vec3) -> (Vec3, Vec3) {
    let t = &w.cfg.tuning.signature.orbital;
    let ground = w.ground_point_below(aim_point + Vec3::Y * 2.0).unwrap_or(aim_point);
    let ceiling = w.arena.bounds.max.y - t.ceiling_margin;
    let alt = t.altitude.min((ceiling - ground.y).max(t.altitude_min.min(ceiling - ground.y).max(4.0)));
    let flat = Vec3::new(ground.x - caster_eye.x, 0.0, ground.z - caster_eye.z);
    let d = flat.length();
    let dir = if d > 1e-3 { flat / d } else { Vec3::NEG_Z };
    // Push it out along the aim azimuth so it sits within view_elev of the caster's eye — but never
    // more than `alt` (the beam stays within 45° of vertical).
    let h = ground.y + alt - caster_eye.y;
    let want_d = if h > 0.0 { h / t.view_elev_deg.to_radians().tan() } else { d };
    let shift = (want_d - d).clamp(0.0, alt);
    (ground + dir * shift + Vec3::Y * alt, ground)
}

/// A fresh orbital construct: switch off its own flying / emitting (the strike replaces them) and
/// start the first telegraph.
pub(crate) fn register(w: &mut World, cid: ConstructId, anchor: Vec3, ground: Vec3) {
    let Some(c) = w.constructs.get_mut(cid) else { return };
    let r = c.recipe.clone();
    let emit = if r.has(Mechanic::Beam) {
        Emit::Beam
    } else if r.has(Mechanic::Breath) {
        Emit::Breath
    } else {
        Emit::Shots
    };
    let mut dmg_mult = 1.0;
    if r.has(Mechanic::Heavy) {
        dmg_mult *= 1.5;
    }
    if r.has(Mechanic::Fast) {
        dmg_mult *= 0.8;
    }
    let t = w.cfg.tuning.signature.orbital.clone();
    let size = (r.scale / crate::constructs::forms::form_spec(r.form).default_scale).clamp(0.5, 4.0);
    let radius = t.beam_radius * size.sqrt() * if emit == Emit::Breath { 2.0 } else { 1.0 };
    c.recipe.mechanics.retain(|m| {
        !matches!(
            m,
            Mechanic::Flies
                | Mechanic::Homing
                | Mechanic::DropFromSky
                | Mechanic::Beam
                | Mechanic::Shoots
                | Mechanic::Breath
                | Mechanic::Charge
                | Mechanic::Thrust
                | Mechanic::Walks
        )
    });
    crate::constructs::components::on_graft(c);
    c.pos = anchor;
    c.vel = Vec3::ZERO;
    let o = Orbital {
        construct: cid,
        owner: c.owner,
        anchor,
        point: ground,
        phase: StrikePhase::Telegraph,
        t: 0.0,
        emit,
        homing: r.has(Mechanic::Homing),
        abduct: r.has(Mechanic::Attract),
        element: r.element,
        dmg_mult,
        radius,
        pulse_t: 0.0,
        carve_t: 0.0,
    };
    let list = &mut w.signature.orbitals;
    if list.len() >= MAX_ORBITALS {
        list.remove(0);
    }
    list.push(o);
    w.events.push(GameEvent::OrbitalStrike {
        id: cid,
        owner: o.owner,
        phase: StrikePhase::Telegraph,
        point: ground,
        radius,
        secs: t.telegraph,
    });
}

/// SIGNATURE HOOK (World::tick, after constructs): hover, telegraph, strike, cool down.
pub(crate) fn step(w: &mut World, dt: f32) {
    if w.signature.orbitals.is_empty() {
        return;
    }
    let t = w.cfg.tuning.signature.orbital.clone();
    let mut i = 0;
    while i < w.signature.orbitals.len() {
        let mut o = w.signature.orbitals[i];
        let alive = w.constructs.get(o.construct).is_some_and(|c| c.gone.is_none());
        if !alive {
            if o.phase == StrikePhase::Strike {
                w.events.push(GameEvent::OrbitalStrike {
                    id: o.construct,
                    owner: o.owner,
                    phase: StrikePhase::End,
                    point: o.point,
                    radius: o.radius,
                    secs: 0.0,
                });
            }
            w.signature.orbitals.remove(i);
            continue;
        }
        // Hover (its own Flies / DropFromSky are off; gravity and pushes are undone every tick).
        if let Some(c) = w.constructs.get_mut(o.construct) {
            c.pos = o.anchor;
            c.vel = Vec3::ZERO;
            c.rot = crate::geom::yaw_rotation(Vec3::new(o.point.x - o.anchor.x, 0.0, o.point.z - o.anchor.z).normalize_or(Vec3::NEG_Z));
        }
        o.t += dt;
        match o.phase {
            StrikePhase::Telegraph => {
                if o.t >= t.telegraph {
                    o.phase = StrikePhase::Strike;
                    o.t = 0.0;
                    o.pulse_t = 0.0;
                    o.carve_t = 0.0;
                    w.events.push(GameEvent::OrbitalStrike {
                        id: o.construct,
                        owner: o.owner,
                        phase: StrikePhase::Strike,
                        point: o.point,
                        radius: o.radius,
                        secs: t.strike_secs,
                    });
                }
            }
            StrikePhase::Strike => {
                track(w, &mut o, &t, dt);
                if o.abduct {
                    lift(w, &o, &t, dt);
                }
                o.pulse_t -= dt;
                if o.pulse_t <= 0.0 {
                    o.pulse_t += t.pulse;
                    pulse(w, &mut o, &t);
                }
                if o.t >= t.strike_secs {
                    o.phase = StrikePhase::End;
                    o.t = 0.0;
                    w.events.push(GameEvent::OrbitalStrike {
                        id: o.construct,
                        owner: o.owner,
                        phase: StrikePhase::End,
                        point: o.point,
                        radius: o.radius,
                        secs: t.cooldown,
                    });
                }
            }
            StrikePhase::End => {
                if o.t >= t.cooldown {
                    o.phase = StrikePhase::Telegraph;
                    o.t = 0.0;
                    o.point = goal(w, &o, &t).unwrap_or(o.point);
                    w.events.push(GameEvent::OrbitalStrike {
                        id: o.construct,
                        owner: o.owner,
                        phase: StrikePhase::Telegraph,
                        point: o.point,
                        radius: o.radius,
                        secs: t.telegraph,
                    });
                }
            }
        }
        w.signature.orbitals[i] = o;
        i += 1;
    }
}

/// Where the beam wants to be: the nearest enemy near the point (Homing), else the caster's
/// crosshair on the ground.
fn goal(w: &World, o: &Orbital, t: &OrbitalTuning) -> Option<Vec3> {
    if o.homing {
        let mut best: Option<(f32, Vec3)> = None;
        for p in &w.players {
            if p.id == o.owner || !p.alive || p.spawn_shield > 0.0 {
                continue;
            }
            let d = Vec3::new(p.mv.pos.x - o.point.x, 0.0, p.mv.pos.z - o.point.z).length();
            if d <= t.track_radius && best.is_none_or(|b| d < b.0) {
                best = Some((d, p.mv.pos));
            }
        }
        if let Some((_, pos)) = best {
            return Some(pos);
        }
    }
    let p = w.player(o.owner).filter(|p| p.alive)?;
    let eye = p.eye(&w.cfg.move_params);
    let hit = bd_core::voxel::raycast(&w.voxels, eye, p.look(), 120.0)?;
    Some(hit.point)
}

/// Move the ground point toward the goal at ≤ `slew` m/s, then drop it onto the ground under it.
fn track(w: &World, o: &mut Orbital, t: &OrbitalTuning, dt: f32) {
    let Some(g) = goal(w, o, t) else { return };
    let to = Vec3::new(g.x - o.point.x, 0.0, g.z - o.point.z);
    let step = t.slew * dt;
    let mv = if to.length() <= step { to } else { to.normalize_or_zero() * step };
    let p = o.point + mv;
    let top = Vec3::new(p.x, o.anchor.y - 1.0, p.z);
    o.point = bd_core::voxel::raycast(&w.voxels, top, Vec3::NEG_Y, o.anchor.y + 80.0).map_or(Vec3::new(p.x, o.point.y, p.z), |h| h.point);
}

/// Attract (UFO abduction): everyone in the column rises toward the emitter (every tick, smooth).
fn lift(w: &mut World, o: &Orbital, t: &OrbitalTuning, dt: f32) {
    let origin = o.anchor - Vec3::Y * 1.0;
    let seg = o.point - origin;
    let seg_len2 = seg.length_squared().max(1e-6);
    let base = w.cfg.move_params.clone();
    for p in w.players.iter_mut() {
        if p.id == o.owner || !p.alive || p.spawn_shield > 0.0 {
            continue;
        }
        let c = p.hull(&base).center();
        let k = ((c - origin).dot(seg) / seg_len2).clamp(0.0, 1.0);
        if (c - (origin + seg * k)).length() > o.radius + 0.5 || c.y > origin.y - 2.0 {
            continue;
        }
        let up = (t.abduct_max - p.mv.vel.y).clamp(0.0, t.abduct_accel * dt);
        if up > 0.0 {
            p.mv.apply_impulse(Vec3::Y * up, 0.0);
        }
    }
}

/// One beam pulse: hits along the column, visuals, carves.
fn pulse(w: &mut World, o: &mut Orbital, t: &OrbitalTuning) {
    let origin = o.anchor - Vec3::Y * 1.0;
    let to = o.point - origin;
    let len = to.length().max(1e-3);
    let dir = to / len;
    // Terrain between the emitter and the point (the beam stops at the first solid voxel).
    let end = bd_core::voxel::raycast(&w.voxels, origin, dir, len + 0.5).map_or(o.point, |h| h.point);
    let seg = end - origin;
    let seg_len = seg.length().max(1e-3);
    let base = w.cfg.move_params.clone();
    let mut hits: u8 = 0;
    for i in 0..w.players.len() {
        let p = &w.players[i];
        if p.id == o.owner || !p.alive || p.spawn_shield > 0.0 {
            continue;
        }
        let c = p.hull(&base).center();
        let k = ((c - origin).dot(seg) / (seg_len * seg_len)).clamp(0.0, 1.0);
        let closest = origin + seg * k;
        if (c - closest).length() > o.radius + 0.5 {
            continue;
        }
        hits = hits.saturating_add(1);
        let pid = p.id;
        if o.abduct {
            continue; // lifted every tick in `lift`, never blasted
        }
        let out = Vec3::new(c.x - end.x, 0.0, c.z - end.z).normalize_or(Vec3::X) * 0.6 + Vec3::Y * 0.4;
        let hit = HitInfo::new(pid, Some(o.owner), HitKind::Shot, t.damage * o.dmg_mult, t.knockback, out, c)
            .with_element(o.element)
            .with_construct(o.construct)
            .with_impact(0.35);
        w.apply_hit(hit);
    }
    match o.emit {
        Emit::Shots => {
            // A hail of bolts at the point instead of a continuous column.
            let spread = Vec3::new(((o.t * 37.0).sin()) * o.radius, 0.0, ((o.t * 23.0).cos()) * o.radius);
            let aim = (o.point + spread - origin).normalize_or(Vec3::NEG_Y);
            let shot = crate::projectile::ProjectileSpawn {
                owner: o.owner,
                construct: Some(o.construct),
                kind: crate::projectile::ProjectileKind::Bolt,
                pos: origin,
                vel: aim * 90.0,
                damage: 8.0 * o.dmg_mult,
                knockback: 8.0,
                element: o.element,
                radius: 0.3,
                life: 1.5,
                explode: None,
                hit_kind: HitKind::Shot,
                ricochet: 0,
                effects: 0,
                falloff_start: 0.0,
                chip: 0.3,
                muzzle_speed: 90.0,
            };
            w.fire_projectile(shot, None, None);
        }
        Emit::Beam | Emit::Breath => {
            w.events.push(GameEvent::Hitscan { owner: o.owner, origin, end, hits });
        }
    }
    o.carve_t -= t.pulse;
    if o.carve_t <= 0.0 && !o.abduct {
        o.carve_t += t.carve_every;
        if t.carve_radius > 0.0 {
            let earth = if o.element == Element::Earth { 2.0 } else { 1.0 };
            w.request_carve(CarveRequest { center: end, radius: t.carve_radius * earth, power: 1.0 });
        }
        if matches!(o.element, Element::Fire | Element::Atomic) {
            w.ignite_terrain(end, o.radius, Some(o.owner));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::mechanics::Form;

    #[test]
    fn detection_is_the_combination_only() {
        let mut r = Recipe::unknown("satellite");
        r.form = Form::Vehicle;
        r.mechanics = vec![Mechanic::Flies, Mechanic::DropFromSky, Mechanic::Beam];
        let r = r.normalized();
        assert!(is_orbital(&r));
        let mut plain = r.clone();
        plain.mechanics.retain(|m| *m != Mechanic::DropFromSky);
        assert!(!is_orbital(&plain), "a flying beam emitter is a plain flier");
        let mut gun = r.clone();
        gun.form = Form::Gun;
        assert!(!is_orbital(&gun), "held things never orbit");
    }

    #[test]
    fn hostile_tuning_is_clamped() {
        let t = OrbitalTuning { altitude: f32::NAN, altitude_min: 500.0, pulse: 0.0, ..Default::default() }.validated();
        assert!(t.altitude.is_finite() && t.altitude_min <= 120.0 && t.pulse >= DT);
        assert_eq!(OrbitalTuning::default().validated(), OrbitalTuning::default());
    }
}
