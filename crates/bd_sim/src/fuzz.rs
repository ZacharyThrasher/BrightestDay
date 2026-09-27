//! Chaos testing: random recipes from the full combination space and [`ChaosWorld`] — a headless
//! world full of bots that cast random things, open portals, chain, resize, pilot, recite, join
//! and leave. Shared by tests, `examples/soak.rs` and (later) the client's `--soak-view`.
//!
//! Every random cast is described in a human-readable line (e.g.
//! `"Serpent + Flies+Sticky+Attract, Fire, payload: Anvil, Aggressive 0.9, 3.2 m"`) and emitted as
//! `GameEvent::ChaosCast` so a viewer can caption it. The construct engineer extends
//! [`random_recipe`] as the component system grows.

use std::fmt::Write as _;
use std::time::Instant;

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::mechanics::{Element, Feature, Form, Mechanic, ModelSource, Recipe, Temperament};
use bd_core::{PlayerId, Vec3};

use crate::bots::{BotBrain, drive};
use crate::events::GameEvent;
use crate::rng::SimRng;
use crate::rules::{ConstructLimits, MatchRules, ScaleLimit, WillpowerMode};
use crate::space::{LinkEnd, PortalEnd};
use crate::world::{Aim, ArenaSource, SimConfig, TargetHint, World};

pub const FORMS: [Form; 39] = {
    use Form::*;
    [
        Hammer, Sword, Axe, Spear, Shield, Gauntlet, Whip, Gun, Bow, Cannon, Staff, Disc, Wings, Parachute, Jetpack, Cape, Biped, Kaiju,
        Quadruped, Serpent, Bird, Fish, Arachnid, Insect, Blob, Wall, Ramp, Pillar, Dome, Cage, Platform, Trampoline, Ball, Anvil, Bomb,
        Boulder, Rocket, Vehicle, Blob0,
    ]
};

pub const MECHANICS: [Mechanic; 40] = {
    use Mechanic::*;
    [
        Blunt, Blade, Pierce, Guard, Returning, Ricochet, Shoots, Beam, Explodes, Grab, Pull, SlowFall, Glide, Thrust, Bounce, Launch,
        Barrier, Trap, Homing, ArticulatedTail, Flies, Charge, Stomp, Breath, Swarm, Spin, DropFromSky, Sticky, Fast, Heavy, Constrict,
        Walks, Piloted, Portal, Attract, Repel, Shrink, Grow, Link, Transforms,
    ]
};

pub const ELEMENTS: [Element; 11] = {
    use Element::*;
    [Light, Fire, Electric, Ice, Water, Wind, Earth, Poison, Psychic, Shadow, Atomic]
};

pub const FEATURES: [Feature; 32] = {
    use Feature::*;
    [
        Wheels, Treads, Pistons, Gears, Boiler, Smokestack, Pipes, Exhaust, Propeller, Rotor, Jet, Turbine, Sails, Mast, Cockpit, Cab,
        Windows, Turret, Barrels, Drill, Saw, CraneArm, Chains, Antenna, Dish, Headlight, Plow, Fins, Spikes, Armor, Bell, Cage,
    ]
};

pub const TEMPERAMENTS: [Temperament; 8] = {
    use Temperament::*;
    [Calm, Aggressive, Fearful, Protective, Playful, Lazy, Chaotic, Sneaky]
};

fn form_index(f: Form) -> usize {
    FORMS.iter().position(|x| *x == f).unwrap_or(0)
}

fn mech_index(m: Mechanic) -> usize {
    MECHANICS.iter().position(|x| *x == m).unwrap_or(0)
}

const WORDS: [&str; 19] = [
    "hammer", "shark", "tornado", "mjolnir", "grandma's teapot", "GIANTTT", "eeyore", "sharknado", "🐝🐝", "a", "", "wall with legs",
    "very angry bees", "extremely huge moon", "ñandú", "x",
    // Command phrases (COMBAT.md §5d-bis): exercised as casts and as grafts.
    "roll out", "combine", "transform",
];

/// A random recipe anywhere in the combination space — including hostile values (NaN / negative
/// scale and mass, huge counts, unsorted duplicate mechanics, nested payloads, odd names) that the
/// sim must clamp.
pub fn random_recipe(rng: &mut SimRng, payload_depth: u32) -> Recipe {
    let form = FORMS[rng.below(FORMS.len() as u32) as usize];
    let n = if rng.chance(0.05) { rng.below(16) } else { rng.below(5) };
    let mechanics: Vec<Mechanic> = (0..n).map(|_| MECHANICS[rng.below(MECHANICS.len() as u32) as usize]).collect();
    let features: Vec<Feature> = (0..rng.below(4)).map(|_| FEATURES[rng.below(FEATURES.len() as u32) as usize]).collect();
    let scale = match rng.below(20) {
        0 => f32::NAN,
        1 => -3.0,
        2 => 1e9,
        3 => rng.range(20.0, 80.0),
        _ => (rng.range(-2.5, 3.5)).exp(),
    };
    let mass = match rng.below(20) {
        0 => f32::INFINITY,
        1 => -1.0,
        _ => (rng.range(-1.0, 9.0)).exp(),
    };
    let mut name = WORDS[rng.below(WORDS.len() as u32) as usize].to_string();
    if rng.chance(0.02) {
        name = "long".repeat(40);
    }
    let payload = if payload_depth > 0 && rng.chance(0.3) { Some(Box::new(random_recipe(rng, payload_depth - 1))) } else { None };
    Recipe {
        name,
        form,
        mechanics,
        element: ELEMENTS[rng.below(ELEMENTS.len() as u32) as usize],
        scale,
        mass,
        count: if rng.chance(0.1) { rng.below(40) as u8 } else { 1 + rng.below(3) as u8 },
        accent: if rng.chance(0.2) { Some([rng.f32(), f32::NAN, 2.0]) } else { None },
        model: ModelSource::Procedural { seed: rng.next_u32() },
        payload,
        features,
        grants: None,
        temperament: TEMPERAMENTS[rng.below(TEMPERAMENTS.len() as u32) as usize],
        intensity: if rng.chance(0.05) { f32::NAN } else { rng.range(-0.2, 1.3) },
        mount: None,
        opening: None,
    }
}

const WILD_WORDS: [&str; 12] = [
    "banana", "teapot", "grandma", "tuba", "lawnmower", "jellyfish", "disco ball", "cactus", "spatula", "moon", "rubber duck", "shark",
];

/// A random but *sane* recipe (form default scale, 0–2 mechanics, any element): Wild Magic's
/// fallback when the game layer injected no phrase source (the sim has no lexicon).
pub fn random_sane_recipe(rng: &mut SimRng) -> Recipe {
    let form = FORMS[rng.below(FORMS.len() as u32) as usize];
    let mut mechanics = Vec::with_capacity(2);
    for _ in 0..rng.below(3) {
        let m = MECHANICS[rng.below(MECHANICS.len() as u32) as usize];
        if !matches!(m, Mechanic::Piloted | Mechanic::Portal | Mechanic::Swarm) {
            mechanics.push(m);
        }
    }
    let name = WILD_WORDS[rng.below(WILD_WORDS.len() as u32) as usize];
    let mut r = crate::bots::recipe(name, form, &mechanics, ELEMENTS[rng.below(ELEMENTS.len() as u32) as usize], crate::constructs::form_spec(form).default_scale);
    r.temperament = TEMPERAMENTS[rng.below(TEMPERAMENTS.len() as u32) as usize];
    r.intensity = if r.temperament == Temperament::Calm { 0.0 } else { 0.7 };
    r
}

/// Human-readable one-liner for a recipe.
pub fn describe(r: &Recipe) -> String {
    let mut s = format!("{:?}", r.form);
    if !r.mechanics.is_empty() {
        s.push_str(" + ");
        for (i, m) in r.mechanics.iter().enumerate() {
            if i > 0 {
                s.push('+');
            }
            let _ = write!(s, "{m:?}");
        }
    }
    let _ = write!(s, ", {:?}", r.element);
    if let Some(p) = &r.payload {
        let _ = write!(s, ", payload: {:?}", p.form);
    }
    if r.temperament != Temperament::Calm {
        let _ = write!(s, ", {:?} {:.1}", r.temperament, r.intensity);
    }
    if r.count > 1 {
        let _ = write!(s, ", ×{}", r.count);
    }
    let _ = write!(s, ", {:.1} m", r.scale);
    s
}

/// Random lobby rules (covers no-limits / double / infinite / chaos caps).
pub fn random_rules(rng: &mut SimRng) -> MatchRules {
    MatchRules {
        scoring: if rng.chance(0.25) { crate::rules::Scoring::Stock { lives: 1 + rng.below(3) as u8 } } else { crate::rules::Scoring::TimedKos },
        scale: [ScaleLimit::Normal, ScaleLimit::Huge, ScaleLimit::NoLimits][rng.below(3) as usize],
        willpower: [WillpowerMode::Normal, WillpowerMode::Double, WillpowerMode::Infinite][rng.below(3) as usize],
        limits: if rng.chance(0.5) { ConstructLimits::default() } else { ConstructLimits::chaos() },
        ko_target: 10,
        time_limit: 480.0,
        regen_islands_between_rounds: true,
        ..MatchRules::default()
    }
}

/// Coverage + performance statistics of chaos runs.
#[derive(Clone, Debug)]
pub struct ChaosStats {
    pub ticks: u64,
    pub worlds: u32,
    pub casts: u64,
    pub casts_ok: u64,
    pub per_form: [u64; FORMS.len()],
    pub per_mechanic: [u64; MECHANICS.len()],
    /// Row-major MECHANICS × MECHANICS co-occurrence in successful casts.
    pub pairs: Vec<u64>,
    /// Tick time histogram: bucket k counts ticks in [2^k, 2^(k+1)) µs (k = 0..20).
    pub hist_us: [u64; 21],
    pub tick_us: Vec<u32>,
    pub max_us: u32,
    pub failures: Vec<(u64, u64, String)>,
    pub determinism: Option<bool>,
    pub events: u64,
}

impl Default for ChaosStats {
    fn default() -> Self {
        Self {
            ticks: 0,
            worlds: 0,
            casts: 0,
            casts_ok: 0,
            per_form: [0; FORMS.len()],
            per_mechanic: [0; MECHANICS.len()],
            pairs: vec![0; MECHANICS.len() * MECHANICS.len()],
            hist_us: [0; 21],
            tick_us: Vec::new(),
            max_us: 0,
            failures: Vec::new(),
            determinism: None,
            events: 0,
        }
    }
}

impl ChaosStats {
    pub fn record_tick(&mut self, us: u32) {
        self.ticks += 1;
        let k = (32 - us.max(1).leading_zeros() - 1).min(20) as usize;
        self.hist_us[k] += 1;
        self.max_us = self.max_us.max(us);
        // Keep a bounded reservoir for percentiles.
        if self.tick_us.len() < 2_000_000 {
            self.tick_us.push(us);
        }
    }

    pub fn record_cast(&mut self, r: &Recipe, ok: bool) {
        self.casts += 1;
        if !ok {
            return;
        }
        self.casts_ok += 1;
        self.per_form[form_index(r.form)] += 1;
        for (i, a) in r.mechanics.iter().enumerate() {
            let ia = mech_index(*a);
            self.per_mechanic[ia] += 1;
            for b in &r.mechanics[i + 1..] {
                let ib = mech_index(*b);
                self.pairs[ia * MECHANICS.len() + ib] += 1;
                self.pairs[ib * MECHANICS.len() + ia] += 1;
            }
        }
    }

    pub fn merge(&mut self, o: &ChaosStats) {
        self.ticks += o.ticks;
        self.worlds += o.worlds;
        self.casts += o.casts;
        self.casts_ok += o.casts_ok;
        for (a, b) in self.per_form.iter_mut().zip(o.per_form.iter()) {
            *a += b;
        }
        for (a, b) in self.per_mechanic.iter_mut().zip(o.per_mechanic.iter()) {
            *a += b;
        }
        for (a, b) in self.pairs.iter_mut().zip(o.pairs.iter()) {
            *a += b;
        }
        for (a, b) in self.hist_us.iter_mut().zip(o.hist_us.iter()) {
            *a += b;
        }
        self.tick_us.extend_from_slice(&o.tick_us);
        self.max_us = self.max_us.max(o.max_us);
        self.failures.extend(o.failures.iter().cloned());
        self.events += o.events;
    }

    /// (p50, p99, max) tick time in µs.
    pub fn percentiles(&self) -> (u32, u32, u32) {
        if self.tick_us.is_empty() {
            return (0, 0, 0);
        }
        let mut v = self.tick_us.clone();
        v.sort_unstable();
        let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        (at(0.5), at(0.99), self.max_us)
    }

    /// Mechanic pairs never seen together in a successful cast.
    pub fn uncovered_pairs(&self) -> usize {
        let n = MECHANICS.len();
        (0..n).flat_map(|i| ((i + 1)..n).map(move |j| (i, j))).filter(|&(i, j)| self.pairs[i * n + j] == 0).count()
    }

    /// Dashboard JSON (hand-written: no serde_json dependency).
    pub fn to_json(&self) -> String {
        let (p50, p99, max) = self.percentiles();
        let mut s = String::new();
        let _ = write!(s, "{{\"ticks\":{},\"worlds\":{},\"casts\":{},\"casts_ok\":{},\"events\":{},", self.ticks, self.worlds, self.casts, self.casts_ok, self.events);
        let _ = write!(s, "\"tick_us\":{{\"p50\":{p50},\"p99\":{p99},\"max\":{max},\"histogram_pow2\":[");
        for (i, h) in self.hist_us.iter().enumerate() {
            let _ = write!(s, "{}{}", if i > 0 { "," } else { "" }, h);
        }
        s.push_str("]},\"per_form\":{");
        for (i, f) in FORMS.iter().enumerate() {
            let _ = write!(s, "{}\"{:?}\":{}", if i > 0 { "," } else { "" }, f, self.per_form[i]);
        }
        s.push_str("},\"per_mechanic\":{");
        for (i, m) in MECHANICS.iter().enumerate() {
            let _ = write!(s, "{}\"{:?}\":{}", if i > 0 { "," } else { "" }, m, self.per_mechanic[i]);
        }
        s.push_str("},\"mechanics\":[");
        for (i, m) in MECHANICS.iter().enumerate() {
            let _ = write!(s, "{}\"{:?}\"", if i > 0 { "," } else { "" }, m);
        }
        s.push_str("],\"pair_matrix\":[");
        let n = MECHANICS.len();
        for i in 0..n {
            s.push_str(if i > 0 { ",[" } else { "[" });
            for j in 0..n {
                let _ = write!(s, "{}{}", if j > 0 { "," } else { "" }, self.pairs[i * n + j]);
            }
            s.push(']');
        }
        let _ = write!(s, "],\"uncovered_pairs\":{},\"failures\":[", self.uncovered_pairs());
        for (i, (seed, tick, msg)) in self.failures.iter().enumerate() {
            let esc: String = msg.chars().map(|c| if c == '"' || c == '\\' { '\'' } else { c }).filter(|c| !c.is_control()).collect();
            let _ = write!(s, "{}{{\"seed\":{seed},\"tick\":{tick},\"error\":\"{esc}\"}}", if i > 0 { "," } else { "" });
        }
        let det = match self.determinism {
            Some(true) => "true",
            Some(false) => "false",
            None => "null",
        };
        let _ = write!(s, "],\"determinism\":{det}}}");
        s
    }
}

/// What one chaos tick did.
#[derive(Clone, Debug, Default)]
pub struct ChaosTickReport {
    pub tick: u64,
    pub tick_us: u32,
    pub events: usize,
    /// (player, description) of random casts this tick.
    pub casts: Vec<(PlayerId, String)>,
    pub invariant_error: Option<String>,
}

/// Options for a chaos world.
#[derive(Clone, Debug)]
pub struct ChaosConfig {
    pub seed: u64,
    pub rules: MatchRules,
    pub arena: ArenaSource,
    pub bots: u8,
    /// Mean random casts per bot per second.
    pub cast_rate: f32,
    /// Run Party mode (short rounds) with this many random effects stacked on every bot at the
    /// start (bots keep picking more between rounds). `None` = FFA.
    pub party: Option<u8>,
}

impl ChaosConfig {
    pub fn new(seed: u64, rules: MatchRules) -> Self {
        Self { seed, rules, arena: ArenaSource::Archipelago { seed }, bots: 8, cast_rate: 0.6, party: None }
    }
}

/// A self-driving chaos world.
pub struct ChaosWorld {
    pub world: World,
    pub bots: Vec<BotBrain>,
    pub cfg: ChaosConfig,
    pub stats: ChaosStats,
    rng: SimRng,
    inputs: Vec<(PlayerId, PlayerInput)>,
    next_id: u8,
}

impl ChaosWorld {
    pub fn new(seed: u64, rules: MatchRules) -> Self {
        Self::with_config(ChaosConfig::new(seed, rules))
    }

    pub fn with_config(cfg: ChaosConfig) -> Self {
        let mut sim = SimConfig { arena: cfg.arena, seed: cfg.seed, rules: cfg.rules.clone(), check_invariants: false, ..SimConfig::default() };
        if cfg.party.is_some() {
            sim.party = Some(crate::settings::PartySettings { rounds: 15, round_time: 40.0, round_ko_target: 6, pick_secs: 3.0, round_end_secs: 1.0, reveal_secs: 1.0, party_end_secs: 3.0, ..Default::default() });
        }
        let mut world = World::new(sim);
        world.cfg.timings.warmup_secs = 2.0;
        let mut bots = Vec::new();
        for k in 0..cfg.bots.min(8) {
            let id = PlayerId(k);
            if world.add_player(id, format!("chaos{k}"), Corps::from_index(k)).is_ok() {
                let mut b = BotBrain::new(id, 0.3 + 0.1 * (k % 7) as f32, cfg.seed);
                b.casts_enabled = false; // chaos casts instead
                bots.push(b);
            }
        }
        if let Some(n) = cfg.party {
            // Stack random effects of every rarity on every bot.
            let mut r = SimRng::new(cfg.seed ^ 0xFA27_EFFE);
            let count = world.party().map_or(0, |p| p.table.len()) as u32;
            for b in &bots {
                for _ in 0..n {
                    let e = r.below(count.max(1)) as u16;
                    world.grant_party_effect(b.id, e);
                }
            }
        }
        let stats = ChaosStats { worlds: 1, ..Default::default() };
        Self { world, bots, rng: SimRng::new(cfg.seed ^ 0xC4A0_5EED), inputs: Vec::with_capacity(8), next_id: cfg.bots.min(8), cfg, stats }
    }

    /// One tick: bots think, chaos acts, the world steps, invariants are checked.
    pub fn step(&mut self) -> ChaosTickReport {
        let mut report = ChaosTickReport::default();
        self.inputs.clear();
        drive(&mut self.world, &mut self.bots, &mut self.inputs);
        self.chaos_actions(&mut report);
        let t0 = Instant::now();
        let n = self.world.tick(&self.inputs).len();
        let us = t0.elapsed().as_micros().min(u32::MAX as u128) as u32;
        self.stats.record_tick(us);
        self.stats.events += n as u64;
        report.tick = self.world.tick;
        report.tick_us = us;
        report.events = n;
        if let Err(e) = self.world.check_invariants() {
            self.stats.failures.push((self.cfg.seed, self.world.tick, e.clone()));
            report.invariant_error = Some(e);
        }
        report
    }

    fn random_player(&mut self) -> Option<PlayerId> {
        let alive: Vec<PlayerId> = self.world.players.iter().filter(|p| p.alive).map(|p| p.id).collect();
        if alive.is_empty() { None } else { Some(alive[self.rng.below(alive.len() as u32) as usize]) }
    }

    fn chaos_actions(&mut self, report: &mut ChaosTickReport) {
        let p_cast = self.cfg.cast_rate * bd_core::DT;
        let ids: Vec<PlayerId> = self.bots.iter().map(|b| b.id).collect();
        for id in ids {
            if !self.rng.chance(p_cast) {
                continue;
            }
            let Some(p) = self.world.player(id).filter(|p| p.alive) else { continue };
            let eye = p.eye(self.world.params());
            let look = if self.rng.chance(0.6) {
                p.look()
            } else {
                Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-0.8, 0.5), self.rng.range(-1.0, 1.0)).normalize_or_zero()
            };
            let recipe = random_recipe(&mut self.rng, 2);
            let validated = crate::willpower::validate_recipe(recipe.clone(), &self.world.cfg.rules);
            let desc = describe(&validated);
            let mut aim = Aim::new(eye, look);
            aim.modifier = self.rng.chance(0.15);
            if self.rng.chance(0.2) {
                aim.target_hint = self.world.constructs.owned_by(id).map(|c| c.id).last().map(TargetHint::Construct);
            }
            let ok = self.world.cast(id, recipe, None, aim).is_ok();
            self.stats.record_cast(&validated, ok);
            self.world_event(GameEvent::ChaosCast { player: id, description: desc.clone() });
            report.casts.push((id, desc));
        }
        let r = self.rng.f32();
        // Rare world-level chaos.
        if r < 1.0 / 600.0 {
            if let (Some(a), Some(b)) = (self.random_player(), self.random_player()) {
                let pa = self.world.player(a).map(|p| p.mv.pos).unwrap_or_default();
                let pb = self.world.player(b).map(|p| p.mv.pos).unwrap_or_default();
                let n1 = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0));
                let n2 = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0));
                let ea = PortalEnd::new(pa + Vec3::new(3.0, 1.0, 0.0), n1, Vec3::Y, 1.4);
                let eb = PortalEnd::new(pb + Vec3::new(-3.0, 1.0, 0.0), n2, Vec3::Y, 1.4);
                self.world.create_portal(a, ea, eb, 12.0, None);
            }
        } else if r < 2.0 / 600.0 {
            if let Some(a) = self.random_player() {
                let pos = self.world.player(a).map(|p| p.mv.pos).unwrap_or_default() + Vec3::new(self.rng.range(-8.0, 8.0), 2.0, self.rng.range(-8.0, 8.0));
                let s = if self.rng.chance(0.5) { 22.0 } else { -22.0 };
                self.world.create_field(a, pos, self.rng.range(6.0, 14.0), s, 6.0, None);
            }
        } else if r < 3.0 / 600.0 {
            if let (Some(a), Some(b)) = (self.random_player(), self.random_player()) {
                let end_b = if a == b || self.rng.chance(0.3) {
                    LinkEnd::Point(self.world.player(a).map(|p| p.mv.pos).unwrap_or_default())
                } else {
                    LinkEnd::Player(b)
                };
                self.world.create_link(Some(a), LinkEnd::Player(a), end_b, 8.0, 6.0);
            }
        } else if r < 4.0 / 600.0 {
            if let Some(a) = self.random_player() {
                let s = if self.rng.chance(0.5) { crate::space::SHRINK_SCALE } else { crate::space::GROW_SCALE };
                self.world.resize_player(a, s, 6.0);
            }
        } else if r < 4.5 / 600.0 {
            if let Some(a) = self.random_player() {
                self.world.eject(a);
            }
        } else if r < 4.8 / 600.0 {
            // Leave + rejoin with a new id.
            if self.bots.len() > 2 {
                let k = self.rng.below(self.bots.len() as u32) as usize;
                let gone = self.bots.remove(k).id;
                self.world.remove_player(gone);
                let id = PlayerId(self.next_id);
                self.next_id = self.next_id.wrapping_add(1);
                if self.world.add_player(id, format!("chaos{}", id.0), Corps::from_index(id.0)).is_ok() {
                    let mut b = BotBrain::new(id, 0.5, self.cfg.seed ^ id.0 as u64);
                    b.casts_enabled = false;
                    self.bots.push(b);
                }
            }
        } else if r < 5.0 / 600.0 {
            if let Some(a) = self.random_player() {
                self.world.dismiss(a);
            }
        }
    }

    fn world_event(&mut self, e: GameEvent) {
        // Commands between ticks land in the next tick's event list.
        self.world.inject_event(e);
    }
}
