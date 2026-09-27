//! Domain expansions — one generic rule (owner: "right now it just makes a brick dome … kind of
//! lame"). Any named domain ("domain expansion: X", "unlimited void", "coffin of the iron mountain",
//! "<anything> domain") is the amplified `Dome` of the phrase's element / technique (`signature`
//! amplifier); this module makes it a DOMAIN:
//!
//! 1. **Expansion** (`expand_secs`, the cast's forming windup): the barrier closes in from the rim;
//!    nothing blocks yet — anyone near the edge can dash out before it seals. Control is never taken.
//! 2. **The barrier** (sealed): a sphere shell around the caster's feet, radius = the dome's, that
//!    blocks movement both ways ([`shell`], run by the host AND prediction from the replicated
//!    construct), projectiles and palm beams both ways, and has HP (`hp`, the construct's integrity:
//!    break it from inside or out). No voxel edits.
//! 3. **Sure hit**: inside, the caster's shots lock on and can't miss ([`aim`]: the CONNECT aim
//!    assist with a domain override — homing cone / turn / magnetism maxed), their palm beams and
//!    thrown energy steer onto the nearest enemy inside, and their casts cost fewer sparks
//!    ([`cast_cost`], before the spark quantisation).
//! 4. **Pulses**: every `pulse_every` s the domain strikes everyone inside but the caster, by its
//!    LOOK ([`DomainLook`]: the technique, else the element) — telegraphed (`pulse_telegraph` s, a
//!    marker at their feet; step off it), damage-tunable: Blade slashes (shallow cuts in the ground),
//!    Fire pillars, Electric chains, Ice freezing slow, Shadow grabbing hands (a brief root), Water a
//!    drag current, Earth spikes, Psychic / void a sensory overload (heavy slow + the client's white
//!    noise — full control kept), Light, Wind, Poison, Atomic their own. A "simple domain" (any Guard
//!    dome: "simple domain", "counter domain", "domain amplification") makes its caster immune for
//!    `guard_secs`.
//! 5. **Cost**: the signature price (a full bar), `life` s (`signature.domain_life`), then BURNOUT: no
//!    casting for `burnout` s. Two domains overlapping CLASH: the boundary moves toward the weaker
//!    (push = sparks spent + holding PRIMARY to pour willpower); the loser shatters.
//!
//! Replicated: the Dome construct (spawn recipe → look; forming; size = clash shrink; integrity = HP)
//! + [`GameEvent::Domain`] (sealed, pulse telegraph / strike, clash, shatter, burnout, guard).
//! Deterministic; zero cost without domains.

use bd_core::input::buttons;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use crate::aim_assist::{AimTuning, ArchAngle};
use crate::combat::{CarveRequest, HitInfo, HitKind};
use crate::constructs::{Construct, ConstructStore};
use crate::events::GameEvent;
use crate::movement::{MoveParams, PlayerState};
use crate::status::StatusKind;
use crate::world::World;

/// A Dome this big (radius, m) or bigger is a domain (plain domes stop at 5 m).
pub const DOMAIN_MIN_RADIUS: f32 = 8.0;

/// Half the thickness of the barrier's blocking band (m).
const SHELL: f32 = 0.7;

/// What a domain looks and strikes like: its technique, else its element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DomainLook {
    Blade,
    Void,
    Fire,
    Water,
    Electric,
    Shadow,
    Ice,
    Light,
    Earth,
    Wind,
    Poison,
    Atomic,
}

impl DomainLook {
    pub const ALL: [DomainLook; 12] = [
        DomainLook::Blade,
        DomainLook::Void,
        DomainLook::Fire,
        DomainLook::Water,
        DomainLook::Electric,
        DomainLook::Shadow,
        DomainLook::Ice,
        DomainLook::Light,
        DomainLook::Earth,
        DomainLook::Wind,
        DomainLook::Poison,
        DomainLook::Atomic,
    ];
}

/// The look of a domain recipe: a slashing technique (Blade) wins, else the element.
pub fn look_of(r: &Recipe) -> DomainLook {
    if r.has(Mechanic::Blade) {
        return DomainLook::Blade;
    }
    match r.element {
        Element::Psychic => DomainLook::Void,
        Element::Fire => DomainLook::Fire,
        Element::Water => DomainLook::Water,
        Element::Electric => DomainLook::Electric,
        Element::Shadow => DomainLook::Shadow,
        Element::Ice => DomainLook::Ice,
        Element::Light => DomainLook::Light,
        Element::Earth => DomainLook::Earth,
        Element::Wind => DomainLook::Wind,
        Element::Poison => DomainLook::Poison,
        Element::Atomic => DomainLook::Atomic,
    }
}

/// Is this construct a domain barrier (a signature dome)?
pub fn is_domain(c: &Construct) -> bool {
    c.recipe.form == Form::Dome && c.recipe.scale >= DOMAIN_MIN_RADIUS && c.gone.is_none()
}

/// (centre, radius) of a domain barrier (radius follows a clash's shrink).
pub fn sphere(c: &Construct) -> (Vec3, f32) {
    // Snapped to a 1/16 m grid: the host's exact position and a client's network-quantised copy give
    // the same barrier (prediction parity at the wall).
    let q = |x: f32| (x * 16.0).round() / 16.0;
    (Vec3::new(q(c.pos.x), q(c.pos.y), q(c.pos.z)), q(c.recipe.scale * c.size_scale.clamp(0.1, 5.0)))
}

/// A Guard dome that isn't a domain: "simple domain", "counter domain", "domain amplification".
pub fn is_simple_domain(r: &Recipe) -> bool {
    r.form == Form::Dome && r.has(Mechanic::Guard) && r.scale < DOMAIN_MIN_RADIUS
}

/// What a domain event reports.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DomainEvent {
    /// The barrier sealed (the domain world is on for everyone inside).
    Sealed { id: ConstructId, owner: PlayerId, look: DomainLook, center: Vec3, radius: f32 },
    /// A pulse is coming at `pos` (a marker at the victim's feet) in `secs`.
    Telegraph { id: ConstructId, look: DomainLook, pos: Vec3, radius: f32, secs: f32 },
    /// It strikes at `pos` (`victim` = who it caught, if anyone).
    Strike { id: ConstructId, look: DomainLook, pos: Vec3, radius: f32, victim: Option<PlayerId> },
    /// Two domains push: the boundary point between them and the side giving way.
    Clash { a: ConstructId, b: ConstructId, point: Vec3, losing: ConstructId },
    /// The barrier broke (HP, clash, or time).
    Shatter { id: ConstructId, owner: PlayerId, center: Vec3, radius: f32 },
    /// `player` can't cast for `secs`.
    Burnout { player: PlayerId, secs: f32 },
    /// `player` is immune to sure-hit pulses for `secs` (a simple domain).
    Guard { player: PlayerId, secs: f32 },
}

/// Domain numbers (`tuning.signature.domain`; the life is `signature.domain_life`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DomainTuning {
    /// The expansion (forming windup): the barrier closes; nothing blocks yet.
    pub expand_secs: f32,
    /// Barrier HP (the dome's integrity).
    pub hp: f32,
    /// After the domain ends (any way): no casting for this long.
    pub burnout: f32,
    /// The caster's casts inside cost × this (before the spark rounding).
    pub cast_cost_mult: f32,
    /// Pulses: every `pulse_every` s, telegraphed `pulse_telegraph` s ahead, `pulse_radius` m wide.
    pub pulse_every: f32,
    pub pulse_telegraph: f32,
    pub pulse_radius: f32,
    /// Base damage / knockback of a pulse (each look scales it).
    pub pulse_damage: f32,
    pub pulse_knockback: f32,
    /// Status seconds (slow, root, burn, poison) a pulse applies.
    pub status_secs: f32,
    /// Blade: the shallow cuts (carve radius m, how many along the slash).
    pub cut_radius: f32,
    pub cut_count: u32,
    /// Electric: chain reach (m).
    pub chain_range: f32,
    /// Water: drag current (m/s toward the centre, sideways); Wind: gust (m/s); Earth: spike lift (m/s).
    pub current_speed: f32,
    pub gust_speed: f32,
    pub spike_speed: f32,
    /// Sure hit: the caster's homing / magnetism inside (deg / deg-per-s / m).
    pub sure_cone_deg: f32,
    pub sure_turn_deg: f32,
    pub sure_magnet_deg: f32,
    pub sure_range: f32,
    /// Clash: push = sparks spent / `push_per_spark` + `hold_push` while pouring (`hold_cost`/s);
    /// the weaker shrinks at `clash_rate` m/s per unit of push difference, shattering under
    /// `clash_min_radius` m.
    pub push_per_spark: f32,
    pub hold_push: f32,
    pub hold_cost: f32,
    pub clash_rate: f32,
    pub clash_min_radius: f32,
    /// A simple domain's immunity (s).
    pub guard_secs: f32,
}

impl Default for DomainTuning {
    fn default() -> Self {
        Self {
            expand_secs: 1.5,
            hp: 900.0,
            burnout: 3.0,
            cast_cost_mult: 0.5,
            pulse_every: 1.4,
            pulse_telegraph: 0.45,
            pulse_radius: 2.4,
            pulse_damage: 7.0,
            pulse_knockback: 6.0,
            status_secs: 1.6,
            cut_radius: 0.55,
            cut_count: 4,
            chain_range: 9.0,
            current_speed: 9.0,
            gust_speed: 14.0,
            spike_speed: 13.0,
            sure_cone_deg: 75.0,
            sure_turn_deg: 1200.0,
            sure_magnet_deg: 20.0,
            sure_range: 60.0,
            push_per_spark: 25.0,
            hold_push: 1.0,
            hold_cost: 15.0,
            clash_rate: 3.0,
            clash_min_radius: 5.0,
            guard_secs: 3.0,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl DomainTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.expand_secs = fin(self.expand_secs, d.expand_secs, 0.2, 6.0);
        self.hp = fin(self.hp, d.hp, 1.0, 1.0e6);
        self.burnout = fin(self.burnout, d.burnout, 0.0, 30.0);
        self.cast_cost_mult = fin(self.cast_cost_mult, d.cast_cost_mult, 0.0, 2.0);
        self.pulse_every = fin(self.pulse_every, d.pulse_every, 0.2, 20.0);
        self.pulse_telegraph = fin(self.pulse_telegraph, d.pulse_telegraph, 0.0, 5.0);
        self.pulse_radius = fin(self.pulse_radius, d.pulse_radius, 0.3, 20.0);
        self.pulse_damage = fin(self.pulse_damage, d.pulse_damage, 0.0, 200.0);
        self.pulse_knockback = fin(self.pulse_knockback, d.pulse_knockback, 0.0, 100.0);
        self.status_secs = fin(self.status_secs, d.status_secs, 0.0, 20.0);
        self.cut_radius = fin(self.cut_radius, d.cut_radius, 0.0, 3.0);
        self.cut_count = self.cut_count.min(12);
        self.chain_range = fin(self.chain_range, d.chain_range, 0.0, 60.0);
        self.current_speed = fin(self.current_speed, d.current_speed, 0.0, 60.0);
        self.gust_speed = fin(self.gust_speed, d.gust_speed, 0.0, 60.0);
        self.spike_speed = fin(self.spike_speed, d.spike_speed, 0.0, 60.0);
        self.sure_cone_deg = fin(self.sure_cone_deg, d.sure_cone_deg, 0.0, 180.0);
        self.sure_turn_deg = fin(self.sure_turn_deg, d.sure_turn_deg, 0.0, 5000.0);
        self.sure_magnet_deg = fin(self.sure_magnet_deg, d.sure_magnet_deg, 0.0, 90.0);
        self.sure_range = fin(self.sure_range, d.sure_range, 1.0, 400.0);
        self.push_per_spark = fin(self.push_per_spark, d.push_per_spark, 0.1, 1000.0);
        self.hold_push = fin(self.hold_push, d.hold_push, 0.0, 20.0);
        self.hold_cost = fin(self.hold_cost, d.hold_cost, 0.0, 200.0);
        self.clash_rate = fin(self.clash_rate, d.clash_rate, 0.0, 50.0);
        self.clash_min_radius = fin(self.clash_min_radius, d.clash_min_radius, 0.5, 40.0);
        self.guard_secs = fin(self.guard_secs, d.guard_secs, 0.0, 30.0);
        self
    }

    /// The caster's aim assist inside their domain: `base` with homing / magnetism maxed.
    fn sure(&self, base: &AimTuning) -> AimTuning {
        let mut a = base.clone();
        a.proj_cone_deg = self.sure_cone_deg;
        a.proj_range = self.sure_range;
        a.proj_turn_deg = self.sure_turn_deg;
        a.proj_decay_secs = 3.0;
        a.proj_release_deg = 180.0;
        a.proj_explosive = 1.0;
        for k in &mut a.proj_kinds {
            k.mult = k.mult.max(1.0);
        }
        a.thrown_cone_deg = self.sure_cone_deg;
        a.thrown_range = self.sure_range;
        a.thrown_turn_deg = self.sure_turn_deg;
        a.thrown_decay_secs = 3.0;
        a.thrown_release_deg = 180.0;
        a.hitscan = crate::tuning::GUN_ARCHETYPES.iter().map(|&arch| ArchAngle { arch, deg: self.sure_magnet_deg }).collect();
        a.hitscan_beam_deg = self.sure_magnet_deg;
        a.hitscan_range = self.sure_range;
        a
    }
}

/// One live domain (host state).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Domain {
    pub construct: ConstructId,
    pub owner: PlayerId,
    pub look: DomainLook,
    pub element: Element,
    pub center: Vec3,
    pub radius: f32,
    sealed: bool,
    pulse_t: f32,
    /// Pending strike: (seconds left, where, victim).
    pending: [(f32, Vec3, Option<PlayerId>); 8],
    n_pending: usize,
    /// Sparks spent on it (the clash push).
    spent: f32,
    pulses: u32,
}

#[derive(Debug, Default)]
pub(crate) struct DomainState {
    pub domains: Vec<Domain>,
    /// (player, seconds left) — no casting.
    pub burnout: Vec<(PlayerId, f32)>,
    /// (player, seconds left) — immune to pulses.
    pub guard: Vec<(PlayerId, f32)>,
    /// The sure-hit aim tuning (built from the base when the first domain opens).
    sure: Option<AimTuning>,
}

// ------------------------------------------------------------------------------ SIGNATURE HOOKs

/// After a Domain signature's dome spawned: it is a domain (HP, expansion time).
pub(crate) fn register(w: &mut World, cid: ConstructId, cost: f32) {
    let t = w.cfg.tuning.signature.domain.clone();
    let Some(c) = w.constructs.get_mut(cid) else { return };
    c.max_integrity = t.hp;
    c.integrity = t.hp;
    // The rest's grabbing verbs don't belong to a domain (the pulses are its hands).
    c.recipe.mechanics.retain(|m| *m != Mechanic::Trap);
    crate::constructs::components::on_graft(c);
    let (center, radius) = sphere(c);
    let d = Domain {
        construct: cid,
        owner: c.owner,
        look: look_of(&c.recipe),
        element: c.recipe.element,
        center,
        radius,
        sealed: false,
        pulse_t: 0.0,
        pending: [(0.0, Vec3::ZERO, None); 8],
        n_pending: 0,
        spent: cost.max(0.0),
        pulses: 0,
    };
    if w.signature.domain.sure.is_none() {
        w.signature.domain.sure = Some(t.sure(&w.cfg.tuning.assist.aim));
    }
    w.signature.domain.domains.push(d);
}

/// A simple domain was cast: its caster is immune to pulses for a while.
pub(crate) fn guard(w: &mut World, owner: PlayerId) {
    let secs = w.cfg.tuning.signature.domain.guard_secs;
    let g = &mut w.signature.domain.guard;
    match g.iter_mut().find(|g| g.0 == owner) {
        Some(e) => e.1 = e.1.max(secs),
        None => g.push((owner, secs)),
    }
    w.events.push(GameEvent::Domain(DomainEvent::Guard { player: owner, secs }));
}

/// CAST HOOK: burnt out after a domain — no casting.
pub(crate) fn burnt_out(w: &World, pid: PlayerId) -> bool {
    w.signature.domain.burnout.iter().any(|b| b.0 == pid && b.1 > 0.0)
}

/// COST HOOK (before the spark rounding): the caster's own casts inside their sealed domain are cheaper.
pub(crate) fn cast_cost(w: &World, pid: PlayerId, cost: f32) -> f32 {
    if owned_sealed(w, pid).is_some() { cost * w.cfg.tuning.signature.domain.cast_cost_mult } else { cost }
}

/// AIM HOOK: the aim assist for `pid` — maxed inside their own sealed domain.
pub fn aim(w: &World, pid: PlayerId) -> &AimTuning {
    match (&w.signature.domain.sure, owned_sealed(w, pid)) {
        (Some(s), Some(_)) => s,
        _ => &w.cfg.tuning.assist.aim,
    }
}

/// The sealed domain `pid` stands in and owns.
fn owned_sealed(w: &World, pid: PlayerId) -> Option<&Domain> {
    if w.signature.domain.domains.is_empty() {
        return None;
    }
    let p = w.player(pid)?;
    w.signature.domain.domains.iter().find(|d| d.owner == pid && d.sealed && d.center.distance(p.mv.pos) < d.radius)
}

/// Sure hit for beams / thrown energy: the nearest enemy inside `pid`'s sealed domain (their eye).
pub(crate) fn sure_target(w: &World, pid: PlayerId, from: Vec3) -> Option<Vec3> {
    let d = owned_sealed(w, pid)?;
    let base = &w.cfg.move_params;
    w.players
        .iter()
        .filter(|q| q.id != pid && q.alive && !q.shielded() && d.center.distance(q.mv.pos) < d.radius)
        .map(|q| q.hull(base).center())
        .min_by(|a, b| a.distance_squared(from).total_cmp(&b.distance_squared(from)))
}

/// Where a segment `a`–`b` first meets a sealed barrier (fraction 0..1), if it crosses one.
pub(crate) fn segment_block(w: &World, a: Vec3, b: Vec3) -> Option<(f32, ConstructId)> {
    let mut best: Option<(f32, ConstructId)> = None;
    for d in w.signature.domain.domains.iter().filter(|d| d.sealed) {
        let (da, db) = (a.distance(d.center) - d.radius, b.distance(d.center) - d.radius);
        if da.signum() == db.signum() {
            continue;
        }
        // Bisect the crossing (a sphere crossing is monotone enough over a beam's length for this).
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..16 {
            let m = (lo + hi) * 0.5;
            let dm = a.lerp(b, m).distance(d.center) - d.radius;
            if dm.signum() == da.signum() {
                lo = m;
            } else {
                hi = m;
            }
        }
        if best.is_none_or(|(f, _)| lo < f) {
            best = Some((lo, d.construct));
        }
    }
    best
}

/// MOVEMENT HOOK (`ConstructStore::post_move`, host and prediction alike): a sealed domain's
/// barrier holds everyone on the side they are on (a band at the shell pushes to the nearer side).
pub fn shell(store: &ConstructStore, st: &mut PlayerState, p: &MoveParams) {
    for c in store.iter() {
        if !is_domain(c) || !c.active() {
            continue;
        }
        let (center, r) = sphere(c);
        let body = st.pos + Vec3::Y * (p.stand_height * 0.5);
        let off = body - center;
        let d = off.length();
        if d < 1e-3 || (d - r).abs() > SHELL {
            continue;
        }
        let n = off / d;
        let (target, dir) = if d < r { (r - SHELL, -1.0) } else { (r + SHELL, 1.0) };
        st.pos += n * (target - d);
        // No speed through the wall.
        let radial = st.vel.dot(n);
        if radial * dir < 0.0 {
            st.vel -= n * radial;
        }
    }
}

// ------------------------------------------------------------------------------------ step

/// World::tick (`signature::step`): seal, pulses, projectiles at the barrier, clash, burnout.
pub(crate) fn step(w: &mut World, dt: f32) {
    let st = &mut w.signature.domain;
    if st.domains.is_empty() && st.burnout.is_empty() && st.guard.is_empty() {
        return;
    }
    for b in st.burnout.iter_mut().chain(st.guard.iter_mut()) {
        b.1 -= dt;
    }
    st.burnout.retain(|b| b.1 > 0.0);
    st.guard.retain(|b| b.1 > 0.0);
    if st.domains.is_empty() {
        return;
    }
    let t = w.cfg.tuning.signature.domain.clone();
    let mut i = 0;
    while i < w.signature.domain.domains.len() {
        let mut d = w.signature.domain.domains[i];
        let Some(c) = w.constructs.get(d.construct).filter(|c| c.gone.is_none()) else {
            end(w, &d, &t);
            w.signature.domain.domains.remove(i);
            continue;
        };
        let (center, radius) = sphere(c);
        let active = c.active();
        d.center = center;
        d.radius = radius;
        if active && !d.sealed {
            d.sealed = true;
            d.pulse_t = t.pulse_every * 0.5;
            w.events.push(GameEvent::Domain(DomainEvent::Sealed { id: d.construct, owner: d.owner, look: d.look, center, radius }));
        }
        if d.sealed {
            pulses(w, &mut d, &t, dt);
            block_projectiles(w, &d);
        }
        w.signature.domain.domains[i] = d;
        i += 1;
    }
    clash(w, &t, dt);
}

/// The domain ended (time / HP / clash): shatter + burnout.
fn end(w: &mut World, d: &Domain, t: &DomainTuning) {
    w.events.push(GameEvent::Domain(DomainEvent::Shatter { id: d.construct, owner: d.owner, center: d.center, radius: d.radius }));
    let b = &mut w.signature.domain.burnout;
    b.retain(|x| x.0 != d.owner);
    if t.burnout > 0.0 {
        b.push((d.owner, t.burnout));
        w.events.push(GameEvent::Domain(DomainEvent::Burnout { player: d.owner, secs: t.burnout }));
    }
}

fn inside(d: &Domain, p: Vec3) -> bool {
    d.center.distance(p) < d.radius
}

/// Telegraph a pulse at everyone inside (not the caster, not guarded); strike when it's due.
fn pulses(w: &mut World, d: &mut Domain, t: &DomainTuning, dt: f32) {
    // Due strikes.
    let mut k = 0;
    while k < d.n_pending {
        d.pending[k].0 -= dt;
        if d.pending[k].0 <= 0.0 {
            let (_, at, victim) = d.pending[k];
            strike(w, d, t, at, victim);
            d.pending[k] = d.pending[d.n_pending - 1];
            d.n_pending -= 1;
            continue;
        }
        k += 1;
    }
    d.pulse_t -= dt;
    if d.pulse_t > 0.0 {
        return;
    }
    d.pulse_t += t.pulse_every;
    d.pulses = d.pulses.wrapping_add(1);
    let base = &w.cfg.move_params;
    let guard = &w.signature.domain.guard;
    let mut marks = [(Vec3::ZERO, PlayerId(0)); 8];
    let mut n = 0;
    for p in &w.players {
        if p.id == d.owner || !p.alive || n >= marks.len() || !inside(d, p.mv.pos) || guard.iter().any(|g| g.0 == p.id) {
            continue;
        }
        let _ = base;
        marks[n] = (p.mv.pos, p.id);
        n += 1;
    }
    for &(pos, pid) in &marks[..n] {
        if d.n_pending < d.pending.len() {
            d.pending[d.n_pending] = (t.pulse_telegraph, pos, Some(pid));
            d.n_pending += 1;
        }
        w.events.push(GameEvent::Domain(DomainEvent::Telegraph { id: d.construct, look: d.look, pos, radius: t.pulse_radius, secs: t.pulse_telegraph }));
    }
}

/// One pulse lands at `at`: whoever stands on the marker takes the domain's element.
fn strike(w: &mut World, d: &Domain, t: &DomainTuning, at: Vec3, marked: Option<PlayerId>) {
    let base = w.cfg.move_params.clone();
    let guarded = |w: &World, id: PlayerId| w.signature.domain.guard.iter().any(|g| g.0 == id);
    // The void overloads everyone inside (no marker to step off): full control kept.
    let everyone = d.look == DomainLook::Void;
    let mut hit = [PlayerId(0); 8];
    let mut n = 0;
    for p in &w.players {
        if p.id == d.owner || !p.alive || p.spawn_shield > 0.0 || n >= hit.len() || guarded(w, p.id) || !inside(d, p.mv.pos) {
            continue;
        }
        let on_mark = Vec3::new(p.mv.pos.x - at.x, 0.0, p.mv.pos.z - at.z).length() <= t.pulse_radius && (p.mv.pos.y - at.y).abs() < 3.0;
        if on_mark || (everyone && Some(p.id) == marked) {
            hit[n] = p.id;
            n += 1;
        }
    }
    let victim = hit[..n].first().copied();
    w.events.push(GameEvent::Domain(DomainEvent::Strike { id: d.construct, look: d.look, pos: at, radius: t.pulse_radius, victim }));
    let dmg = t.pulse_damage;
    let kb = t.pulse_knockback;
    let s = t.status_secs;
    match d.look {
        DomainLook::Blade => {
            // Invisible slashes: shallow cuts across the ground through the mark.
            let a = (d.pulses as f32 * 2.399_963 + at.x * 0.37).sin_cos();
            let dir = Vec3::new(a.0, 0.0, a.1);
            for k in 0..t.cut_count {
                let f = k as f32 / t.cut_count.max(1) as f32 - 0.5;
                let p = at + dir * (f * t.pulse_radius * 2.2) - Vec3::Y * 0.2;
                if t.cut_radius > 0.0 {
                    w.request_carve(CarveRequest { center: p, radius: t.cut_radius, power: 0.8 });
                }
            }
        }
        DomainLook::Fire | DomainLook::Atomic => w.ignite_terrain(at, t.pulse_radius, Some(d.owner)),
        _ => {}
    }
    for &pid in &hit[..n] {
        let Some(i) = w.pidx(pid) else { continue };
        let c = w.players[i].hull(&base).center();
        let away = Vec3::new(c.x - d.center.x, 0.0, c.z - d.center.z).normalize_or(Vec3::X);
        let (mult, dir, status): (f32, Vec3, Option<(StatusKind, f32)>) = match d.look {
            DomainLook::Blade => (1.3, (away + Vec3::Y * 0.3).normalize(), None),
            DomainLook::Fire => (1.1, Vec3::Y, Some((StatusKind::Burn, s))),
            DomainLook::Atomic => (1.2, Vec3::Y, Some((StatusKind::Burn, s))),
            DomainLook::Electric => (1.0, Vec3::Y, Some((StatusKind::Wet, 0.0))),
            DomainLook::Ice => (0.5, Vec3::Y, Some((StatusKind::Chill, s * 1.5))),
            DomainLook::Shadow => (0.7, Vec3::Y, Some((StatusKind::Shadow, s))),
            DomainLook::Water => (0.5, -away, Some((StatusKind::Wet, s * 2.0))),
            DomainLook::Earth => (1.1, Vec3::Y, None),
            DomainLook::Void => (0.4, Vec3::Y, Some((StatusKind::Chill, s * 1.8))),
            DomainLook::Light => (1.2, (away + Vec3::Y).normalize(), None),
            DomainLook::Wind => (0.5, (away + Vec3::Y * 0.4).normalize(), None),
            DomainLook::Poison => (0.5, Vec3::Y, Some((StatusKind::Poison, s * 2.0))),
        };
        let h = HitInfo::new(pid, Some(d.owner), HitKind::Shot, dmg * mult, kb * mult.min(1.0), dir, c).with_element(d.element).with_construct(d.construct).with_impact(0.4);
        w.apply_hit(h);
        if let Some((k, secs)) = status
            && secs > 0.0
        {
            w.apply_status(i, k, secs, Some(d.owner));
        }
        let p = &mut w.players[i];
        match d.look {
            // Grabbing hands: a brief root (the horizontal speed dies; they keep full control).
            DomainLook::Shadow => {
                let v = p.mv.vel;
                p.mv.vel = Vec3::new(v.x * 0.05, v.y.min(0.0), v.z * 0.05);
                w.apply_status(i, StatusKind::Chill, s * 0.6, Some(d.owner));
            }
            // A drag current toward the centre.
            DomainLook::Water => p.mv.apply_impulse(-away * t.current_speed, 0.0),
            DomainLook::Wind => p.mv.apply_impulse((away + Vec3::Y * 0.5).normalize() * t.gust_speed, 0.0),
            DomainLook::Earth => p.mv.apply_impulse(Vec3::Y * t.spike_speed, 0.0),
            _ => {}
        }
    }
    // Electric: the strike chains on to everyone inside within reach of the one it caught.
    if d.look == DomainLook::Electric
        && let Some(first) = victim
        && let Some(from) = w.player(first).map(|p| p.mv.pos)
    {
        let mut chain = [PlayerId(0); 8];
        let mut m = 0;
        for p in &w.players {
            if p.id == d.owner || p.id == first || !p.alive || m >= chain.len() || guarded(w, p.id) || !inside(d, p.mv.pos) {
                continue;
            }
            if p.mv.pos.distance(from) <= t.chain_range {
                chain[m] = p.id;
                m += 1;
            }
        }
        for &pid in &chain[..m] {
            let Some(c) = w.player(pid).map(|p| p.hull(&base).center()) else { continue };
            w.events.push(GameEvent::Domain(DomainEvent::Strike { id: d.construct, look: d.look, pos: c, radius: 0.5, victim: Some(pid) }));
            let h = HitInfo::new(pid, Some(d.owner), HitKind::Shot, dmg * 0.6, kb * 0.3, Vec3::Y, c).with_element(d.element).with_construct(d.construct);
            w.apply_hit(h);
        }
    }
}

/// Projectiles crossing a sealed barrier (either way) end at it; the barrier takes the hit.
fn block_projectiles(w: &mut World, d: &Domain) {
    let dt = bd_core::DT;
    let mut hits = 0.0f32;
    for p in w.projectiles.iter_mut() {
        if p.dead {
            continue;
        }
        let prev = p.pos - p.vel * dt;
        let (a, b) = (prev.distance(d.center) < d.radius, p.pos.distance(d.center) < d.radius);
        if a != b {
            // (Marked here, removed below: the world's reap for this tick already ran.)
            p.dead = true;
            hits += p.spec.damage.max(0.0);
            let n = (p.pos - d.center).normalize_or(Vec3::Y);
            w.events.push(GameEvent::ProjectileEnded { id: p.id, pos: d.center + n * d.radius, normal: if a { -n } else { n }, hit_player: None });
        }
    }
    w.projectiles.retain(|p| !p.dead);
    if hits > 0.0 {
        hit_barrier(w, d.construct, hits, d.center);
    }
}

/// Damage the barrier (its HP).
pub(crate) fn hit_barrier(w: &mut World, id: ConstructId, damage: f32, point: Vec3) {
    w.apply_construct_hit(crate::combat::ConstructHit { target: id, source: None, damage, impulse: Vec3::ZERO, blade: false, pierce: false, blunt: false, point });
}

/// Overlapping domains push: the weaker shrinks; under the minimum it shatters.
fn clash(w: &mut World, t: &DomainTuning, dt: f32) {
    let n = w.signature.domain.domains.len();
    if n < 2 {
        return;
    }
    for i in 0..n {
        for j in (i + 1)..n {
            let (a, b) = (w.signature.domain.domains[i], w.signature.domain.domains[j]);
            if !a.sealed || !b.sealed || a.owner == b.owner || a.center.distance(b.center) >= a.radius + b.radius {
                continue;
            }
            let push = |w: &mut World, d: &Domain| {
                let pouring = w.player(d.owner).is_some_and(|p| p.alive && p.willpower > 0.0 && p.input.held(buttons::PRIMARY));
                if pouring && let Some(p) = w.player_mut(d.owner) {
                    p.willpower = (p.willpower - t.hold_cost * dt).max(0.0);
                    p.regen_pause = p.regen_pause.max(0.5);
                }
                d.spent / t.push_per_spark + if pouring { t.hold_push } else { 0.0 }
            };
            let (pa, pb) = (push(w, &a), push(w, &b));
            let (loser, diff) = if pa >= pb { (b, pa - pb) } else { (a, pb - pa) };
            let winner = if loser.construct == a.construct { b } else { a };
            let shrink = (t.clash_rate * diff.max(0.15) * dt).min(loser.radius);
            let axis = (loser.center - winner.center).normalize_or(Vec3::X);
            let point = winner.center + axis * (winner.radius.min(winner.center.distance(loser.center)));
            w.events.push(GameEvent::Domain(DomainEvent::Clash { a: a.construct, b: b.construct, point, losing: loser.construct }));
            let new_r = loser.radius - shrink;
            if new_r < t.clash_min_radius {
                // The loser's barrier breaks (the ordinary shatter).
                hit_barrier(w, loser.construct, 1.0e9, point);
            } else if let Some(c) = w.constructs.get_mut(loser.construct) {
                c.resize(new_r / loser.radius.max(1e-3));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_follow_technique_then_element() {
        let mut r = Recipe::unknown("shrine");
        r.form = Form::Dome;
        r.element = Element::Shadow;
        r.mechanics = vec![Mechanic::Blade, Mechanic::Barrier];
        assert_eq!(look_of(&r), DomainLook::Blade);
        r.mechanics.clear();
        assert_eq!(look_of(&r), DomainLook::Shadow);
        r.element = Element::Psychic;
        assert_eq!(look_of(&r), DomainLook::Void);
    }

    #[test]
    fn hostile_tuning_is_clamped() {
        let t = DomainTuning { hp: f32::NAN, pulse_every: -1.0, cut_count: 99, ..Default::default() }.validated();
        assert_eq!(t.hp, DomainTuning::default().hp);
        assert!(t.pulse_every >= 0.2 && t.cut_count <= 12);
        assert_eq!(DomainTuning::default().validated(), DomainTuning::default());
    }

    #[test]
    fn sure_hit_maxes_the_assist() {
        let base = AimTuning::default();
        let s = DomainTuning::default().sure(&base);
        assert!(s.proj_cone_deg > base.proj_cone_deg * 4.0 && s.proj_turn_deg > base.proj_turn_deg * 3.0);
        assert!(s.hitscan_rad(Some(crate::weapons::GunArchetype::HandCannon)) > base.hitscan_rad(Some(crate::weapons::GunArchetype::HandCannon)) * 5.0);
    }
}
