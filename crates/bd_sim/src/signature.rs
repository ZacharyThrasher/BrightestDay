//! Signature casts (COMBAT.md §5j) — "domain expansion", "kamehameha", "bankai": an **amplifier over
//! the existing composition**, not a new mode. The lexicon labels a phrase as a signature
//! (`bd_lexicon::SignatureCall`); the game sends it as [`World::cast_signature`]; this module
//!
//! 1. rewrites the ordinary recipe with mechanics that already exist ([`amplify`]):
//!    * **Domain** → a sealed hard-light `Dome` around the caster (radius ~16–20 m by how hard it was
//!      shouted) with `Barrier` (stops projectiles, takes the hits), `Trap` (snaps shut on an enemy
//!      inside), keeping the phrase's element / emitters / fields (a Fire domain, a
//!      bouncy-castle domain, a shrine that `Shoots`);
//!    * **Beam** → an ultimate-caliber `Hands` + `Beam` (energy from the palms, the HandBeam archetype:
//!      hold to charge is the telegraph, release fires; never a gun model); a creature keeps its body
//!      and gains `Beam`;
//!    * **Summon** / **Transformation** → the construct itself at ultimate scale (Transformation also
//!      `Heavy`);
//! 2. lets that recipe past the per-form size clamp up to [`SignatureTuning`]'s caps, prices it with the
//!    **existing** cost formula × one multiplier (never more than a full bar), gives it a readable
//!    forming windup and a lifetime — four one-line `SIGNATURE HOOK`s in `world/actions.rs`;
//! 3. never says no: a signature the caster can't afford is cast as the plain phrase instead.
//!
//! The amplifier runs only for the duration of one cast (no per-tick state), so determinism,
//! snapshots, prediction and tick cost are untouched. Presentation (the cut-in DANGER alert, title
//! letters, audio stingers) keys off [`spectacle`] — one generic threshold on the spawn event, so
//! kaiju and no-limits moons get it too.
//!
//! [`orbital`]: an emitter that Flies + DropFromSky ("satellite death beam") hovers high above the
//! aim point and strikes down with a telegraphed, tracking beam (the one per-tick piece; zero cost
//! without orbitals).

pub mod domain;
pub mod orb;
pub mod orbital;
pub mod palm;

use bd_core::mechanics::{Form, FormCategory, Mechanic, ModelSource, Recipe};
use bd_core::shape::ShapeSpec;
use bd_core::{ConstructId, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

use crate::constructs::forms::form_spec;
use crate::events::CastError;
use crate::rules::MatchRules;
use crate::world::{Aim, World};

pub use orb::{OrbTuning, SigFx, is_piercing_orb};
pub use orbital::{OrbitalTuning, StrikePhase, is_orbital};
pub use palm::{BeamPhase, BeamView, PalmTuning};

/// Per-world signature state (`World::signature`): the cast in progress + live orbital emitters.
#[derive(Debug, Default)]
pub(crate) struct SignatureState {
    pub(crate) cast: Option<SignatureCtx>,
    pub(crate) orbitals: Vec<orbital::Orbital>,
    pub(crate) orbs: Vec<orb::Orb>,
    pub(crate) palm: palm::PalmState,
    pub(crate) domain: domain::DomainState,
}

/// What fantasy a signature expresses (mirrors `bd_lexicon::SignatureKind`; the game maps them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignatureKind {
    Domain,
    Beam,
    Transformation,
    Summon,
}

/// A signature cast request (travels with the recipe in the cast command).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignatureCast {
    pub kind: SignatureKind,
    /// 0..1: how hard it was shouted (voice loudness / CAPS / "!!!"). Scales the result a little.
    pub power: f32,
    /// A fusion-style windup (`windup: Merge(a, b)` in `assets/signatures.ron`): an orb of each
    /// element forms at either hand and they merge (presentation; announced as [`SigFx::Merge`]).
    #[serde(default)]
    pub merge: Option<[bd_core::mechanics::Element; 2]>,
}

impl SignatureCast {
    pub fn new(kind: SignatureKind, power: f32) -> Self {
        Self { kind, power, merge: None }.sanitized()
    }
    pub fn with_merge(mut self, a: bd_core::mechanics::Element, b: bd_core::mechanics::Element) -> Self {
        self.merge = Some([a, b]);
        self
    }
    /// Untrusted → safe.
    pub fn sanitized(self) -> Self {
        Self { kind: self.kind, power: if self.power.is_finite() { self.power.clamp(0.0, 1.0) } else { 0.0 }, merge: self.merge }
    }
}

/// Signature numbers (`assets/tuning/combat.ron` → `signature`, hot reload). Never shown to players.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SignatureTuning {
    /// Domain dome radius (m) for a calm cast; a full-power shout adds `domain_radius_shout`.
    pub domain_radius: f32,
    pub domain_radius_shout: f32,
    /// Domain lifetime (s, forming windup included). Plain domes live `summon.life_dome`.
    pub domain_life: f32,
    /// Summon / Transformation: scale × (`scale_mult` + `scale_mult_shout` · power).
    pub scale_mult: f32,
    pub scale_mult_shout: f32,
    /// A signature may grow up to the form's max scale × this (and never past `max_scale`).
    pub scale_cap_mult: f32,
    pub max_scale: f32,
    /// Beam: gun caliber `k = sqrt(scale / default)` (the weapons table caps it at 2.5).
    pub beam_caliber: f32,
    /// Minimum forming time (s): the readable windup everyone sees (the caster keeps moving).
    pub windup: f32,
    /// Held / worn signature constructs (ultimate beams, bankai blades) expire after this (s).
    pub held_life: f32,
    /// THE signature cost knob: existing cast cost × this, capped at a full willpower bar.
    pub cost_mult: f32,
    /// Presentation threshold (cut-in alert + audio): a spawn costing at least this, or at least
    /// `alert_scale` metres, or colossal, is announced to everyone.
    pub alert_cost: f32,
    pub alert_scale: f32,
    /// Bots shout a signature sometimes when their willpower is at least this.
    pub bot_willpower: f32,
    /// Chance per bot cast (above `bot_willpower`) that it becomes a signature.
    pub bot_chance: f32,
    /// Orbital strikes (an emitter that Flies + DropFromSky), see [`orbital`].
    pub orbital: OrbitalTuning,
    /// Erasing orbs (a piercing Beam signature: Hollow Purple), see [`orb`].
    pub orb: OrbTuning,
    /// Physical palm beams (Form::Hands shots), see [`palm`].
    pub palm: PalmTuning,
    /// Domain expansions (expansion, barrier HP, sure-hit pulses, burnout, clash), see [`domain`].
    pub domain: domain::DomainTuning,
}

impl Default for SignatureTuning {
    fn default() -> Self {
        Self {
            domain_radius: 16.0,
            domain_radius_shout: 4.0,
            domain_life: 9.0,
            scale_mult: 2.0,
            scale_mult_shout: 0.6,
            scale_cap_mult: 2.5,
            max_scale: 30.0,
            beam_caliber: 2.5,
            windup: 0.6,
            held_life: 10.0,
            cost_mult: 1.35,
            alert_cost: 45.0,
            alert_scale: 12.0,
            bot_willpower: 85.0,
            bot_chance: 0.2,
            orbital: OrbitalTuning::default(),
            orb: OrbTuning::default(),
            palm: PalmTuning::default(),
            domain: domain::DomainTuning::default(),
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl SignatureTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.domain_radius = fin(self.domain_radius, d.domain_radius, 2.0, 40.0);
        self.domain_radius_shout = fin(self.domain_radius_shout, d.domain_radius_shout, 0.0, 20.0);
        self.domain_life = fin(self.domain_life, d.domain_life, 0.5, 60.0);
        self.scale_mult = fin(self.scale_mult, d.scale_mult, 0.5, 10.0);
        self.scale_mult_shout = fin(self.scale_mult_shout, d.scale_mult_shout, 0.0, 10.0);
        self.scale_cap_mult = fin(self.scale_cap_mult, d.scale_cap_mult, 1.0, 10.0);
        self.max_scale = fin(self.max_scale, d.max_scale, 1.0, crate::rules::NO_LIMITS_MAX_SCALE);
        self.beam_caliber = fin(self.beam_caliber, d.beam_caliber, 1.0, 4.0);
        self.windup = fin(self.windup, d.windup, 0.0, 3.0);
        self.held_life = fin(self.held_life, d.held_life, 1.0, 600.0);
        self.cost_mult = fin(self.cost_mult, d.cost_mult, 0.0, 10.0);
        self.alert_cost = fin(self.alert_cost, d.alert_cost, 0.0, 1000.0);
        self.alert_scale = fin(self.alert_scale, d.alert_scale, 0.5, 1000.0);
        self.bot_willpower = fin(self.bot_willpower, d.bot_willpower, 0.0, 200.0);
        self.bot_chance = fin(self.bot_chance, d.bot_chance, 0.0, 1.0);
        self.orbital = self.orbital.validated();
        self.orb = self.orb.validated();
        self.palm = self.palm.validated();
        self.domain = self.domain.validated();
        self
    }

    /// Largest scale a signature of `form` may reach under `rules`.
    pub fn scale_cap(&self, form: Form, rules: &MatchRules) -> f32 {
        let spec = form_spec(form);
        let (_, rules_hi) = rules.scale_range(spec.min_scale, spec.max_scale);
        let domain = if form == Form::Dome { self.domain_radius + self.domain_radius_shout } else { 0.0 };
        (spec.max_scale * self.scale_cap_mult).min(self.max_scale).max(domain).max(rules_hi)
    }
}

/// The fantasy a recipe expresses when nobody said (bots, the presentation threshold).
pub fn infer_kind(r: &Recipe) -> SignatureKind {
    if matches!(r.form, Form::Dome | Form::Cage) {
        return SignatureKind::Domain;
    }
    if r.has(Mechanic::Beam) || (r.has(Mechanic::Breath) && r.form.category() != FormCategory::Creature) {
        return SignatureKind::Beam;
    }
    match r.form.category() {
        FormCategory::Held | FormCategory::Worn => SignatureKind::Transformation,
        _ => SignatureKind::Summon,
    }
}

/// Mechanics that would make a 30 m dome wander, fly off, split or seat someone: a domain keeps
/// the rest (element, emitters, fields, traits, Explodes = a finale when it ends).
const DOMAIN_DROPS: [Mechanic; 13] = [
    Mechanic::Flies,
    Mechanic::Walks,
    Mechanic::Charge,
    Mechanic::Thrust,
    Mechanic::Piloted,
    Mechanic::Swarm,
    Mechanic::Transforms,
    Mechanic::Returning,
    Mechanic::Ricochet,
    Mechanic::DropFromSky,
    Mechanic::Portal,
    Mechanic::SlowFall,
    Mechanic::Glide,
];

/// Rewrite an ordinary resolution into its signature form, using existing mechanics only. The
/// result is pre-validation: the cast pipeline clamps everything except the lifted scale.
/// `full_bar`: cast from a full willpower bar (charged up) — a slightly bigger orb.
pub fn amplify(mut r: Recipe, sig: SignatureCast, t: &SignatureTuning, rules: &MatchRules, full_bar: bool) -> Recipe {
    let sig = sig.sanitized();
    let p = sig.power;
    let grow = t.scale_mult + t.scale_mult_shout * p;
    match sig.kind {
        SignatureKind::Domain => {
            r.form = Form::Dome;
            r.mechanics.retain(|m| !DOMAIN_DROPS.contains(m));
            // (No Homing: on a dome wrapped around its caster it only switched the shell to walking physics.)
            r.mechanics.retain(|m| *m != Mechanic::Homing);
            // The barrier; the domain's pulses (signature::domain) are its hands (no Trap yank).
            r.mechanics.retain(|m| *m != Mechanic::Trap);
            r.mechanics.push(Mechanic::Barrier);
            r.scale = t.domain_radius + t.domain_radius_shout * p;
            r.count = 1;
            r.mass = r.mass.max(5_000.0);
            // The dome is built procedurally from the recipe (an icon's own shape is not a dome).
            if let ModelSource::Icon(id) = &r.model {
                r.model = ModelSource::Procedural { seed: crate::rng::word_hash(id) as u32 };
            }
        }
        SignatureKind::Beam => {
            let palms = r.form == Form::Hands;
            let held_weapon = r.form.category() == FormCategory::Held && !matches!(r.form, Form::Gun | Form::Cannon | Form::Hands);
            if !palms && (r.form.category() == FormCategory::Creature || is_orbital(&r)) {
                // A creature / an orbital emitter keeps its body: bigger, with a Beam.
                r.mechanics.push(Mechanic::Beam);
                r.scale *= grow;
            } else if !palms && is_piercing_orb(&r) {
                // A piercing thing that is fired / thrown: an erasing orb (Hollow Purple).
                r = orb::orb_recipe(r, &t.orb, p, full_bar);
            } else if held_weapon {
                // A wand / sword the phrase named keeps its body (the lexicon's `delivery: Held`).
                r.mechanics.push(Mechanic::Beam);
                r.scale *= grow;
            } else {
                // Energy from the palms (Form::Hands, the HandBeam archetype): charge, release, a huge
                // piercing beam. Never a gun model (owner: "final flash gave me a gun").
                r.form = Form::Hands;
                r.mechanics.retain(|m| {
                    !matches!(m, Mechanic::Spin | Mechanic::Explodes | Mechanic::Shoots | Mechanic::Swarm | Mechanic::Returning)
                });
                r.mechanics.push(Mechanic::Beam);
                r.scale = form_spec(Form::Hands).default_scale * t.beam_caliber * t.beam_caliber;
                r.count = 1;
            }
        }
        SignatureKind::Summon => {
            r.scale *= grow;
            // A summoned ball of energy is thrown from the hands (hold to grow), not dropped.
            if r.form == Form::Ball && (r.has(Mechanic::Explodes) || r.element != bd_core::mechanics::Element::Light) {
                r.mechanics.retain(|m| *m != Mechanic::DropFromSky);
            }
        }
        SignatureKind::Transformation => {
            r.scale *= grow;
            r.mechanics.push(Mechanic::Heavy);
        }
    }
    r.scale = r.scale.min(t.scale_cap(r.form, rules));
    r.normalized()
}

/// The signature cast in progress (set by [`World::cast_signature`] for the duration of one cast).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SignatureCtx {
    /// Lifted scale (restored after the pipeline's per-form clamp).
    scale: f32,
    form: Form,
    kind: SignatureKind,
}

impl World {
    /// Cast a signature (see the module docs). Resolves like [`World::cast_shaped`] with the
    /// amplified recipe; if the caster can't afford it, the plain phrase is cast instead.
    pub fn cast_signature(
        &mut self,
        player: PlayerId,
        recipe: Recipe,
        shape: Option<ShapeSpec>,
        aim: Aim,
        sig: SignatureCast,
    ) -> Result<ConstructId, CastError> {
        let sig = sig.sanitized();
        // POWER-UPS HOOK: an ultimate needs its pickup; without it, the plain phrase (never a no).
        let gate = crate::pickups::gate_signature(self, player, sig.kind);
        if gate == crate::pickups::Gate::Locked {
            return self.cast_shaped(player, recipe, shape, None, aim);
        }
        let t = self.cfg.tuning.signature.clone();
        let full_bar = self.player(player).is_some_and(|p| p.willpower >= self.cfg.rules.willpower_max() - 0.5);
        let amped = amplify(recipe.clone(), sig, &t, &self.cfg.rules, full_bar);
        let (_, cost) = self.preview_signature_cost(player, amped.clone());
        let have = self.player(player).map_or(0.0, |p| p.willpower);
        if cost > have + 1e-4 && !self.cfg.rules.infinite_willpower() {
            // The ring never says no: the plain phrase instead of a failed ultimate.
            return self.cast_shaped(player, recipe, shape, None, aim);
        }
        let shape = if sig.kind == SignatureKind::Domain { None } else { shape };
        let aim = Aim { modifier: false, ..aim };
        self.signature.cast = Some(SignatureCtx { scale: amped.scale, form: amped.form, kind: sig.kind });
        let orb = sig.kind == SignatureKind::Beam && is_piercing_orb(&amped);
        let shape = if orb { None } else { shape };
        let res = self.cast_shaped(player, amped, shape, None, aim);
        self.signature.cast = None;
        if res.is_ok() && gate == crate::pickups::Gate::Loaded {
            crate::pickups::spend(self, player, crate::pickups::GateKind::Ultimate); // POWER-UPS HOOK
        }
        if let (Ok(cid), Some([a, b])) = (res, sig.merge) {
            let secs = self.constructs.get(cid).map_or(0.0, |c| c.forming_t);
            let pos = self.constructs.get(cid).map_or(aim.origin, |c| c.pos);
            self.events.push(crate::events::GameEvent::SignatureFx {
                id: cid,
                owner: player,
                fx: SigFx::Merge { a, b, secs },
                pos,
                dir: aim.dir,
            });
        }
        res
    }

    /// (amplified + validated recipe, cost) of a signature cast right now — the client's "can I?".
    pub fn preview_signature_cost(&self, player: PlayerId, amplified: Recipe) -> (Recipe, f32) {
        let scale = amplified.scale;
        let (mut r, _) = self.preview_cost(player, amplified);
        r.scale = scale.min(self.cfg.tuning.signature.scale_cap(r.form, &self.cfg.rules));
        if self.cfg.rules.infinite_willpower() {
            return (r, 0.0);
        }
        let wt = &self.cfg.tuning.willpower;
        let uses = self.player(player).map_or(0, |p| p.casts.uses_in(&r.name, self.tick, wt.stale_window));
        let plain = wt.base_cost_as(&r, wt.is_autonomous(&r)) * wt.staleness(uses);
        let pool = self.cfg.rules.willpower_max();
        let plain = if is_orbital(&r) { (plain * self.cfg.tuning.signature.orbital.cost_mult).min(pool) } else { plain };
        let cost = signature_cost(plain, &self.cfg.tuning.signature, self.cfg.rules.willpower_max());
        (r, cost)
    }
}

/// Existing cost × the one multiplier, never more than a full bar.
pub fn signature_cost(plain: f32, t: &SignatureTuning, pool_max: f32) -> f32 {
    (plain * t.cost_mult).min(pool_max)
}

// ------------------------------------------------------------------------ SIGNATURE HOOKs

/// After the per-form clamp (and party re-validation): restore the lifted scale.
pub(crate) fn lift_scale(w: &World, r: &mut Recipe) {
    if let Some(c) = w.signature.cast
        && r.form == c.form
    {
        r.scale = c.scale.min(w.cfg.tuning.signature.scale_cap(r.form, &w.cfg.rules));
    }
}

/// A signature never grafts onto the construct under the crosshair: it always builds.
pub(crate) fn builds_fresh(w: &World) -> bool {
    w.signature.cast.is_some()
}

/// Cast cost: a signature × `cost_mult` (≤ a full bar); an orbital emitter × `orbital.cost_mult`;
/// every other cast unchanged.
pub(crate) fn cost(w: &World, recipe: &Recipe, plain: f32) -> f32 {
    if plain <= 0.0 {
        return plain;
    }
    let t = &w.cfg.tuning.signature;
    let pool = w.cfg.rules.willpower_max();
    // An orbital emitter (autonomous + big already) × its knob, never more than a full bar.
    let plain = if is_orbital(recipe) { (plain * t.orbital.cost_mult).min(pool) } else { plain };
    match w.signature.cast {
        Some(_) => signature_cost(plain, t, pool),
        None => plain,
    }
}

/// Orbital emitters spawn high above the aim point (see [`orbital::spawn_point`]).
pub(crate) fn orbital_spawn(w: &World, recipe: &Recipe, caster_eye: Vec3, aim_point: Vec3, pos: Vec3) -> Vec3 {
    if is_orbital(recipe) { orbital::spawn_point(w, caster_eye, aim_point).0 } else { pos }
}

/// After the spawn: an orbital emitter starts its strike cycle; a Beam signature's erasing orb its
/// windup.
pub(crate) fn orbital_spawned(w: &mut World, cid: ConstructId, recipe: &Recipe, caster_eye: Vec3, aim_point: Vec3) {
    if w.signature.cast.is_some_and(|c| c.kind == SignatureKind::Beam) && is_piercing_orb(recipe) {
        orb::register(w, cid);
    } else if is_orbital(recipe) {
        let (anchor, ground) = orbital::spawn_point(w, caster_eye, aim_point);
        orbital::register(w, cid, anchor, ground);
    } else if orb::is_energy_ball(recipe) {
        // Any ball of energy (rasengan, spirit bomb, fireball…): hold to grow, release to throw.
        orb::register(w, cid);
    } else if w.signature.cast.is_some_and(|c| c.kind == SignatureKind::Domain) && recipe.form == Form::Dome {
        // A domain: expansion, barrier HP, sure-hit pulses (the spark price = its clash push).
        let cost = w.events.iter().rev().find_map(|e| match e {
            crate::events::GameEvent::ConstructSpawned { id, cost, .. } if *id == cid => Some(*cost),
            _ => None,
        });
        domain::register(w, cid, cost.unwrap_or(0.0));
    } else if domain::is_simple_domain(recipe) {
        // "simple domain" / "counter domain": immune to sure-hit pulses for a while.
        if let Some(owner) = w.constructs.get(cid).map(|c| c.owner) {
            domain::guard(w, owner);
        }
    }
}

/// World::tick, after constructs: orbital strikes, energy balls, palm beams, domains.
pub(crate) fn step(w: &mut World, dt: f32) {
    orbital::step(w, dt);
    orb::step(w, dt);
    palm::step(w, dt);
    domain::step(w, dt);
}

/// Forming time: at least the readable windup; a domain's is its expansion.
pub(crate) fn forming(w: &World, secs: f32) -> f32 {
    match w.signature.cast {
        Some(c) if c.kind == SignatureKind::Domain && c.form == Form::Dome => secs.max(w.cfg.tuning.signature.domain.expand_secs),
        Some(_) => secs.max(w.cfg.tuning.signature.windup),
        None => secs,
    }
}

/// Lifetime: domains last `domain_life`; held / worn ultimates expire after `held_life`.
pub(crate) fn life(w: &World, form: Form, secs: f32) -> f32 {
    let Some(c) = w.signature.cast else { return secs };
    let t = &w.cfg.tuning.signature;
    if c.kind == SignatureKind::Domain && form == Form::Dome {
        t.domain_life
    } else if matches!(form.category(), FormCategory::Held | FormCategory::Worn) {
        secs.min(t.held_life)
    } else {
        secs
    }
}

// ------------------------------------------------------------------------ bots

/// Bots: an ordinary cast, or — sometimes, with a full-ish bar — the same recipe as a signature.
/// Deterministic (hash of tick and bot id; no RNG draw, so bot streams are unchanged).
pub fn bot_cast(w: &mut World, bot: PlayerId, recipe: Recipe, aim: Aim) -> Result<ConstructId, CastError> {
    let t = &w.cfg.tuning.signature;
    let wp = w.player(bot).map_or(0.0, |p| p.willpower);
    let mut h = (w.tick ^ ((bot.0 as u64) << 40) ^ 0x5167_0000).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 29;
    let roll = (h % 10_000) as f32 / 10_000.0;
    if wp < t.bot_willpower || roll >= t.bot_chance {
        return w.cast(bot, recipe, None, aim);
    }
    // Half of them are domains of the recipe's element; the rest amplify what the bot picked.
    let kind = if roll < t.bot_chance * 0.5 { SignatureKind::Domain } else { infer_kind(&recipe) };
    let mut r = recipe;
    r.name = match kind {
        SignatureKind::Domain => "Domain Expansion".to_string(),
        _ => format!("Ultimate {}", r.name),
    };
    w.cast_signature(bot, r, None, aim, SignatureCast::new(kind, roll / t.bot_chance.max(1e-3)))
}

// ------------------------------------------------------------------------ presentation

/// How the client should announce a spawn (cut-in DANGER alert, title, audio). Generic: any
/// signature and anything big enough (kaiju, no-limits moons) — no per-move code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spectacle {
    pub kind: SignatureKind,
    /// Danger radius around the construct (m): the dome itself, else a size-scaled zone.
    pub radius: f32,
    /// Seconds until it is live (the forming windup).
    pub windup: f32,
}

/// Mechanics that hurt / seize whoever is near: a big construct with one of these is a threat.
const HOSTILE: [Mechanic; 15] = [
    Mechanic::Explodes,
    Mechanic::Beam,
    Mechanic::Shoots,
    Mechanic::Breath,
    Mechanic::Blade,
    Mechanic::Pierce,
    Mechanic::Blunt,
    Mechanic::Stomp,
    Mechanic::Charge,
    Mechanic::Spin,
    Mechanic::Trap,
    Mechanic::Grab,
    Mechanic::Constrict,
    Mechanic::DropFromSky,
    Mechanic::ArticulatedTail,
];

/// Is this a THREAT worth a DANGER alert? Domains (Trap), palm beams (Beam), erasing orbs, orbital
/// strikes and anything with a hostile mechanic, a kaiju, an aggressive / chaotic creature, a swarm.
/// A parachute, wall, ramp or weaponless vehicle is not, however big or pricey ("ULTIMATE PARACHUTE").
pub fn is_threat(r: &Recipe) -> bool {
    if HOSTILE.iter().any(|m| r.has(*m)) || orb::is_erasing_orb(r) || is_orbital(r) || r.form == Form::Hands {
        return true;
    }
    r.form.category() == FormCategory::Creature
        && (r.form == Form::Kaiju
            || r.has(Mechanic::Swarm)
            || matches!(r.temperament, bd_core::mechanics::Temperament::Aggressive | bd_core::mechanics::Temperament::Chaotic))
}

/// `Some` when a `ConstructSpawned { recipe, cost, forming }` deserves the alert: a THREAT (see
/// [`is_threat`]) that is big or pricey.
pub fn spectacle(recipe: &Recipe, cost: f32, forming: f32, t: &SignatureTuning) -> Option<Spectacle> {
    let big = recipe.scale >= t.alert_scale || recipe.scale >= crate::rules::COLOSSAL_SCALE;
    let pricey = cost.is_finite() && cost >= t.alert_cost;
    if !(big || pricey) || !is_threat(recipe) {
        return None;
    }
    let kind = infer_kind(recipe);
    let radius = match recipe.form {
        Form::Dome => recipe.scale,
        _ if kind == SignatureKind::Beam => 12.0 + recipe.scale * 4.0,
        _ => (recipe.scale * 1.5).max(8.0),
    };
    Some(Spectacle { kind, radius: radius.min(80.0), windup: if forming.is_finite() { forming.max(0.0) } else { 0.0 } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(form: Form, mechs: &[Mechanic], scale: f32) -> Recipe {
        let mut r = Recipe::unknown("x");
        r.form = form;
        r.mechanics = mechs.to_vec();
        r.scale = scale;
        r.normalized()
    }

    #[test]
    fn domain_amplifier_is_existing_mechanics_only() {
        let t = SignatureTuning::default();
        let rules = MatchRules::default();
        let bouncy = rec(Form::Trampoline, &[Mechanic::Bounce, Mechanic::Flies], 4.0);
        let d = amplify(bouncy, SignatureCast::new(SignatureKind::Domain, 0.0), &t, &rules, false);
        assert_eq!(d.form, Form::Dome);
        for m in [Mechanic::Barrier, Mechanic::Trap, Mechanic::Bounce] {
            assert!(d.has(m), "{m:?}");
        }
        assert!(!d.has(Mechanic::Flies));
        assert!((d.scale - 16.0).abs() < 1e-4);
        let loud = amplify(rec(Form::Dome, &[], 4.0), SignatureCast::new(SignatureKind::Domain, 1.0), &t, &rules, false);
        assert!((loud.scale - 20.0).abs() < 1e-4);
    }

    #[test]
    fn beam_becomes_an_ultimate_palm_beam_never_a_gun() {
        let t = SignatureTuning::default();
        let rules = MatchRules::default();
        let beam = SignatureCast::new(SignatureKind::Beam, 0.5);
        for form in [Form::Gun, Form::Cannon, Form::Ball, Form::Blob0, Form::Hands] {
            let b = amplify(rec(form, &[Mechanic::Beam, Mechanic::Heavy], 1.2), beam, &t, &rules, false);
            assert_eq!(b.form, Form::Hands, "{form:?}");
            assert_eq!(crate::weapons::archetype_for(&b), Some(crate::weapons::GunArchetype::HandBeam));
        }
        // Palms first: a piercing palm beam is still a palm beam, not an erasing orb.
        let b = amplify(rec(Form::Hands, &[Mechanic::Beam, Mechanic::Pierce], 1.2), beam, &t, &rules, false);
        assert_eq!(b.form, Form::Hands);
        let k = (b.scale / form_spec(Form::Hands).default_scale).sqrt();
        assert!((k - 2.5).abs() < 1e-3, "caliber {k}");
        // A named wand keeps its body (the lexicon's `delivery: Held`).
        let w = amplify(rec(Form::Staff, &[Mechanic::Beam], 1.2), beam, &t, &rules, false);
        assert_eq!(w.form, Form::Staff);
        assert!(w.scale > 1.2 && w.has(Mechanic::Beam));
    }

    #[test]
    fn caps_and_hostile_power() {
        let t = SignatureTuning::default();
        let rules = MatchRules::default();
        let k = amplify(
            rec(Form::Kaiju, &[], 22.0),
            SignatureCast { kind: SignatureKind::Summon, power: f32::NAN, merge: None },
            &t,
            &rules,
            false,
        );
        assert!(k.scale <= t.max_scale + 1e-4);
        let h = amplify(
            rec(Form::Hammer, &[], 1.2),
            SignatureCast { kind: SignatureKind::Transformation, power: 99.0, merge: None },
            &t,
            &rules,
            false,
        );
        assert!(h.has(Mechanic::Heavy) && h.scale <= form_spec(Form::Hammer).max_scale * t.scale_cap_mult + 1e-4);
        assert_eq!(SignatureTuning { cost_mult: f32::NAN, ..Default::default() }.validated(), t);
    }

    #[test]
    fn cost_never_exceeds_a_full_bar() {
        let t = SignatureTuning::default();
        assert!((signature_cost(40.0, &t, 100.0) - 54.0).abs() < 1e-3);
        assert_eq!(signature_cost(500.0, &t, 100.0), 100.0);
    }

    #[test]
    fn spectacle_threshold() {
        let t = SignatureTuning::default();
        assert!(spectacle(&rec(Form::Hammer, &[], 1.2), 12.0, 0.15, &t).is_none());
        let s = spectacle(&rec(Form::Dome, &[Mechanic::Barrier, Mechanic::Trap], 18.0), 70.0, 0.7, &t).expect("domain");
        assert_eq!(s.kind, SignatureKind::Domain);
        assert!((s.radius - 18.0).abs() < 1e-4);
        assert!(spectacle(&rec(Form::Kaiju, &[], 14.0), 20.0, 1.2, &t).is_some(), "big things too");
    }

    /// Owner (Ember screenshot): a bot's "ULTIMATE PARACHUTE" raised the DANGER alert. Only threats do.
    #[test]
    fn only_threats_raise_the_alert() {
        let t = SignatureTuning::default();
        let rules = MatchRules::default();
        let ult = |r: Recipe, kind| amplify(r, SignatureCast::new(kind, 1.0), &t, &rules, true);
        // Harmless / utility ultimates: big and pricey, but no alert.
        for (name, r) in [
            ("parachute", ult(rec(Form::Parachute, &[Mechanic::SlowFall, Mechanic::Glide], 2.0), SignatureKind::Transformation)),
            ("wall", ult(rec(Form::Wall, &[Mechanic::Barrier], 6.0), SignatureKind::Summon)),
            ("ramp", ult(rec(Form::Ramp, &[Mechanic::Launch], 6.0), SignatureKind::Summon)),
            ("car", ult(rec(Form::Vehicle, &[Mechanic::Piloted], 6.0), SignatureKind::Summon)),
            ("trampoline", ult(rec(Form::Trampoline, &[Mechanic::Bounce], 6.0), SignatureKind::Summon)),
            ("calm dog", rec(Form::Quadruped, &[], 20.0)),
        ] {
            assert!(!is_threat(&r), "{name}: {r:?}");
            assert!(spectacle(&r, 100.0, 0.6, &t).is_none(), "{name} raised the alert");
        }
        // Threats: domains, palm beams, erasing orbs, orbital strikes, hostile big things.
        let dome = ult(rec(Form::Dome, &[Mechanic::Barrier], 4.0), SignatureKind::Domain);
        let palms = ult(rec(Form::Hands, &[Mechanic::Beam], 1.2), SignatureKind::Beam);
        let orb = ult(rec(Form::Cannon, &[Mechanic::Pierce, Mechanic::Explodes], 2.0), SignatureKind::Beam);
        let sky = ult(rec(Form::Disc, &[Mechanic::Flies, Mechanic::DropFromSky, Mechanic::Beam], 4.0), SignatureKind::Beam);
        let bomb = ult(rec(Form::Bomb, &[Mechanic::Explodes], 2.0), SignatureKind::Summon);
        let mut wolf = rec(Form::Quadruped, &[], 20.0);
        wolf.temperament = bd_core::mechanics::Temperament::Aggressive;
        let kaiju = rec(Form::Kaiju, &[], 20.0);
        for (name, r) in
            [("domain", dome), ("palm beam", palms), ("orb", orb), ("orbital", sky), ("bomb", bomb), ("wolf", wolf), ("kaiju", kaiju)]
        {
            assert!(spectacle(&r, 100.0, 0.6, &t).is_some(), "{name} is a threat: {r:?}");
        }
    }
}
