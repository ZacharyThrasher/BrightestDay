//! Combat tuning (COMBAT.md §5i.6): every combat / economy number in one serde struct, stored in
//! [`crate::SimConfig::tuning`] and hot-reloadable from `assets/tuning/combat.ron`
//! ([`crate::World::set_tuning`]).
//!
//! * `Default` reproduces the shipped numbers exactly — each default *is* the constant it replaces
//!   (`combat::BLAST_DAMAGE`, `willpower::STALE_PER_USE`, `forms::form_spec`, `weapons::gun_spec`…),
//!   and every formula here keeps the original floating-point operation order, so a default world
//!   is bit-identical to one built before this struct existed (tests/tuning.rs checks the
//!   formulas over the fuzz recipe space).
//! * Balance philosophy (§5i.1): balance with **willpower cost**, not damage. The cost knobs that
//!   the balance tooling adjusts are [`WillpowerTuning::mechanic_cost`] (extra cost weight per
//!   mechanic, added to the flat `per_mechanic`) and [`WillpowerTuning::form_cost`] (a multiplier
//!   per form). Both are empty by default.
//! * Hidden (§5i.4): nothing here is ever shown to players.
//! * Lobby knobs that already exist stay in [`crate::MatchRules`] (respawn time, spawn shield,
//!   knockback scale, willpower mode); they are not duplicated.
//!
//! Host-authoritative: a client predicting the sim must use the host's tuning (send it with the
//! `SimConfig`). `validated()` makes hostile / hand-edited files safe (NaN → default, clamps).

use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use serde::{Deserialize, Serialize};

use crate::combat;
use crate::constructs::forms::form_spec;
use crate::weapons::{GunArchetype, GunSpec, gun_spec};
use crate::willpower;

/// Every combat / economy number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CombatTuning {
    pub ring: RingTuning,
    pub hit: HitTuning,
    /// Smash model (COMBAT.md §1a): damage %, knockback growth, DI, tech, blast zones.
    pub smash: SmashTuning,
    /// Base damage / knockback of every form's default attack or impact (held table, creature
    /// bites, object impacts).
    pub forms: Vec<FormNumbers>,
    /// Gun archetypes (COMBAT.md §4b) before caliber scaling. The ring blast lives in `ring`.
    pub guns: Vec<GunNumbers>,
    pub creature: CreatureTuning,
    pub willpower: WillpowerTuning,
    pub construct: ConstructTuning,
    pub snowball: AntiSnowball,
    /// CHARGE HOOK: hold-to-power-up refill + movement (`crate::charge`).
    pub charge: crate::charge::ChargeTuning,
    /// BOUNDS HOOK: tether save + construct recall (`world/bounds.rs`, COMBAT.md §1a / §3).
    pub bounds: crate::world::bounds::BoundsTuning,
    /// BOUNDS HOOK: visible summon placement + construct lifetimes (`world/bounds.rs`, COMBAT.md §3).
    pub summon: crate::world::bounds::SummonTuning,
    /// ASSIST HOOK: melee lunge + hook-and-strike (`crate::melee_assist`, `crate::hook`).
    pub assist: crate::melee_assist::AssistTuning,
    /// SIGNATURE HOOK: signature casts (`crate::signature`, COMBAT.md §5j).
    pub signature: crate::signature::SignatureTuning,
    /// FREEDOM HOOK: shout bonus, sputter floor, "it" resize range, hidden legendary rolls (`crate::freedom`).
    pub freedom: crate::freedom::FreedomTuning,
    /// JUGGLE: per-victim juggle protection — knockback decay inside a chain, then a short guard
    /// (`crate::juggle`, COMBAT.md §1a).
    pub juggle: crate::juggle::JuggleTuning,
    /// WILLPOWER = LAUNCH (`crate::launch`, host rule `MatchRules::willpower_launch`): the launch
    /// curve, refill multipliers and the anti-camp leak.
    pub launch: crate::launch::LaunchTuning,
    /// INTERACT HOOK: High Roller's slot machines + roulette house spin (`world/interactables.rs`).
    pub casino: crate::world::interactables::CasinoTuning,
    /// GIANTS: weak points, limbs, topple, shockwaves, command, hijack (`constructs/giant.rs`).
    pub giants: crate::constructs::giant::GiantTuning,
    /// POWER-UPS: the gated list + pickup timers (`crate::pickups`, `MatchRules::powerups`).
    pub pickups: crate::pickups::PickupTuning,
    /// EXTENDS: telescoping weapons — twirl combos, thrust, falling pillar, pole vault
    /// (`constructs/extend.rs`; the vault is predicted by clients, so it rides the host sync).
    pub extend: crate::constructs::extend::ExtendTuning,
    /// LEGENDARY: generic legendary move rules — recall, storm call, ricochet chain, deflect, blade
    /// wave, heavy blade, chain blades (`constructs/legend.rs`; recall flight / the leap are predicted,
    /// so it rides the host sync).
    pub legendary: crate::constructs::legend::LegendTuning,
    /// TRANSFORMER COMBAT: archetypes from the alt mode, strings, transform attacks, counters
    /// (`constructs/brawl.rs`, COMBAT.md §5d-quater). Host-synced.
    pub transformer: crate::constructs::brawl::TransformerTuning,
    /// WEAPON FEEL: per-class weapon numbers (`assets/tuning/weapons.ron` → `classes`, merged in by
    /// the loader; `constructs/weapon_class.rs`). Host-synced with the rest.
    pub weapons: crate::constructs::weapon_class::WeaponTuning,
}

impl Default for CombatTuning {
    fn default() -> Self {
        Self {
            ring: RingTuning::default(),
            hit: HitTuning::default(),
            smash: SmashTuning::default(),
            forms: crate::fuzz::FORMS.iter().map(|&f| FormNumbers::of(f)).collect(),
            guns: GUN_ARCHETYPES.iter().map(|&a| GunNumbers::of(a)).collect(),
            creature: CreatureTuning::default(),
            willpower: WillpowerTuning::default(),
            construct: ConstructTuning::default(),
            snowball: AntiSnowball::default(),
            charge: crate::charge::ChargeTuning::default(),
            bounds: crate::world::bounds::BoundsTuning::default(),
            summon: crate::world::bounds::SummonTuning::default(),
            assist: crate::melee_assist::AssistTuning::default(),
            signature: crate::signature::SignatureTuning::default(),
            freedom: crate::freedom::FreedomTuning::default(),
            juggle: crate::juggle::JuggleTuning::default(),
            launch: crate::launch::LaunchTuning::default(),
            casino: crate::world::interactables::CasinoTuning::default(),
            giants: crate::constructs::giant::GiantTuning::default(),
            pickups: crate::pickups::PickupTuning::default(),
            extend: crate::constructs::extend::ExtendTuning::default(),
            legendary: crate::constructs::legend::LegendTuning::default(),
            transformer: crate::constructs::brawl::TransformerTuning::default(),
            weapons: crate::constructs::weapon_class::WeaponTuning::default(),
        }
    }
}

/// Archetypes with a table row (the ring blast is [`RingTuning`]).
pub const GUN_ARCHETYPES: [GunArchetype; 9] = {
    use GunArchetype::*;
    [HandCannon, SuperShotgun, Minigun, Rocket, Railgun, Bow, Cannon, Staff, Disc]
};

/// NaN / ±inf → `d`, then clamp.
fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

// ------------------------------------------------------------------------------------------ ring

/// Empty-hand ring blast + shove (COMBAT.md §4, §4b).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RingTuning {
    pub blast_speed: f32,
    pub blast_damage: f32,
    pub blast_knockback: f32,
    pub blast_cooldown: f32,
    pub blast_radius: f32,
    pub blast_lifetime: f32,
    /// Recoil impulse (m/s) of one blast on the caster.
    pub blast_recoil: f32,
    /// Recoil impulses on the ground are scaled by this (air ×1.0).
    pub ground_recoil: f32,
    pub shove_range: f32,
    /// Half-angle of the shove cone (radians).
    pub shove_half_angle: f32,
    pub shove_knockback: f32,
    pub shove_cooldown: f32,
}

impl Default for RingTuning {
    fn default() -> Self {
        Self {
            blast_speed: combat::BLAST_SPEED,
            blast_damage: combat::BLAST_DAMAGE,
            blast_knockback: combat::BLAST_KNOCKBACK,
            blast_cooldown: combat::BLAST_COOLDOWN,
            blast_radius: combat::BLAST_RADIUS,
            blast_lifetime: combat::BLAST_LIFETIME,
            blast_recoil: gun_spec(GunArchetype::Ring).recoil,
            ground_recoil: combat::GROUND_RECOIL,
            shove_range: combat::SHOVE_RANGE,
            shove_half_angle: combat::SHOVE_HALF_ANGLE,
            shove_knockback: combat::SHOVE_KNOCKBACK,
            shove_cooldown: combat::SHOVE_COOLDOWN,
        }
    }
}

impl RingTuning {
    fn validated(mut self) -> Self {
        let d = Self::default();
        self.blast_speed = fin(self.blast_speed, d.blast_speed, 5.0, 400.0);
        self.blast_damage = fin(self.blast_damage, d.blast_damage, 0.0, 200.0);
        self.blast_knockback = fin(self.blast_knockback, d.blast_knockback, 0.0, 100.0);
        self.blast_cooldown = fin(self.blast_cooldown, d.blast_cooldown, 0.05, 10.0);
        self.blast_radius = fin(self.blast_radius, d.blast_radius, 0.05, 2.0);
        self.blast_lifetime = fin(self.blast_lifetime, d.blast_lifetime, 0.1, 10.0);
        self.blast_recoil = fin(self.blast_recoil, d.blast_recoil, 0.0, 40.0);
        self.ground_recoil = fin(self.ground_recoil, d.ground_recoil, 0.0, 2.0);
        self.shove_range = fin(self.shove_range, d.shove_range, 0.5, 20.0);
        self.shove_half_angle = fin(self.shove_half_angle, d.shove_half_angle, 0.05, 1.55);
        self.shove_knockback = fin(self.shove_knockback, d.shove_knockback, 0.0, 100.0);
        self.shove_cooldown = fin(self.shove_cooldown, d.shove_cooldown, 0.05, 30.0);
        self
    }

    /// The ring blast as a gun spec (what `weapons::gun_spec(Ring)` returns with defaults).
    pub fn blast_spec(&self) -> GunSpec {
        GunSpec {
            cadence: self.blast_cooldown,
            damage: self.blast_damage,
            knockback: self.blast_knockback,
            speed: self.blast_speed,
            recoil: self.blast_recoil,
            radius: self.blast_radius,
            ..gun_spec(GunArchetype::Ring)
        }
    }
}

// ------------------------------------------------------------------------------------------- hit

/// How a hit lands on a player (COMBAT.md §0–1; the Smash model's knockback numbers join here).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HitTuning {
    /// Knockback (m/s) above which the victim tumbles.
    pub tumble_impulse: f32,
    pub tumble_secs: f32,
    /// Minimum upward component of every knockback direction (Wind: `wind_min_lift`).
    pub min_lift: f32,
    pub wind_min_lift: f32,
    /// Knockback multipliers by element.
    pub water_knockback: f32,
    pub wind_knockback: f32,
    /// Melee momentum bonus `clamp((v_close − start) / span, 0, max_bonus)`.
    pub momentum_start: f32,
    pub momentum_span: f32,
    pub momentum_max_bonus: f32,
    /// Closing speed that earns the streak VFX.
    pub streak_speed: f32,
    /// Head zone (top of the hull, m) and its critical multiplier.
    pub head_zone: f32,
    pub head_mult: f32,
    /// Seconds a hit keeps KO credit.
    pub ko_credit_secs: f32,
    /// Wall splat: tumbling into a wall faster than `splat_min_speed`.
    pub splat_min_speed: f32,
    pub splat_dmg_per_ms: f32,
    pub splat_max_dmg: f32,
    pub splat_bounce: f32,
    pub splat_crater_radius: f32,
    /// Held heavy (charged) attack multipliers and the sword's third-hit finisher.
    pub heavy_damage: f32,
    pub heavy_knockback: f32,
    pub finisher_mult: f32,
}

impl Default for HitTuning {
    fn default() -> Self {
        use crate::constructs::held;
        Self {
            tumble_impulse: combat::TUMBLE_IMPULSE,
            tumble_secs: combat::TUMBLE_SECS,
            min_lift: combat::MIN_LIFT,
            wind_min_lift: combat::WIND_MIN_LIFT,
            water_knockback: combat::element_knockback(bd_core::mechanics::Element::Water),
            wind_knockback: combat::element_knockback(bd_core::mechanics::Element::Wind),
            momentum_start: combat::MOMENTUM_START,
            momentum_span: combat::MOMENTUM_SPAN,
            momentum_max_bonus: combat::MOMENTUM_MAX_BONUS,
            streak_speed: combat::STREAK_SPEED,
            head_zone: combat::HEAD_ZONE,
            head_mult: combat::HEAD_MULT,
            ko_credit_secs: combat::KO_CREDIT_SECS,
            splat_min_speed: combat::SPLAT_MIN_SPEED,
            splat_dmg_per_ms: combat::SPLAT_DMG_PER_MS,
            splat_max_dmg: combat::SPLAT_MAX_DMG,
            splat_bounce: combat::SPLAT_BOUNCE,
            splat_crater_radius: combat::SPLAT_CRATER_RADIUS,
            heavy_damage: held::HEAVY_DMG,
            heavy_knockback: held::HEAVY_KB,
            finisher_mult: held::FINISHER_MULT,
        }
    }
}

impl HitTuning {
    fn validated(mut self) -> Self {
        let d = Self::default();
        self.tumble_impulse = fin(self.tumble_impulse, d.tumble_impulse, 0.0, 200.0);
        self.tumble_secs = fin(self.tumble_secs, d.tumble_secs, 0.0, 5.0);
        self.min_lift = fin(self.min_lift, d.min_lift, 0.0, 0.95);
        self.wind_min_lift = fin(self.wind_min_lift, d.wind_min_lift, 0.0, 0.95);
        self.water_knockback = fin(self.water_knockback, d.water_knockback, 0.0, 5.0);
        self.wind_knockback = fin(self.wind_knockback, d.wind_knockback, 0.0, 5.0);
        self.momentum_start = fin(self.momentum_start, d.momentum_start, 0.0, 100.0);
        self.momentum_span = fin(self.momentum_span, d.momentum_span, 1.0, 500.0);
        self.momentum_max_bonus = fin(self.momentum_max_bonus, d.momentum_max_bonus, 0.0, 3.0);
        self.streak_speed = fin(self.streak_speed, d.streak_speed, 0.0, 500.0);
        self.head_zone = fin(self.head_zone, d.head_zone, 0.0, 1.0);
        self.head_mult = fin(self.head_mult, d.head_mult, 1.0, 5.0);
        self.ko_credit_secs = fin(self.ko_credit_secs, d.ko_credit_secs, 0.0, 60.0);
        self.splat_min_speed = fin(self.splat_min_speed, d.splat_min_speed, 1.0, 500.0);
        self.splat_dmg_per_ms = fin(self.splat_dmg_per_ms, d.splat_dmg_per_ms, 0.0, 20.0);
        self.splat_max_dmg = fin(self.splat_max_dmg, d.splat_max_dmg, 0.0, 200.0);
        self.splat_bounce = fin(self.splat_bounce, d.splat_bounce, 0.0, 1.5);
        self.splat_crater_radius = fin(self.splat_crater_radius, d.splat_crater_radius, 0.0, 5.0);
        self.heavy_damage = fin(self.heavy_damage, d.heavy_damage, 1.0, 10.0);
        self.heavy_knockback = fin(self.heavy_knockback, d.heavy_knockback, 1.0, 10.0);
        self.finisher_mult = fin(self.finisher_mult, d.finisher_mult, 1.0, 10.0);
        self
    }

    /// Melee bonus fraction from closing speed (`combat::momentum_bonus` with these numbers).
    #[inline]
    pub fn momentum_bonus(&self, v_close: f32) -> f32 {
        ((v_close - self.momentum_start) / self.momentum_span).clamp(0.0, self.momentum_max_bonus)
    }

    /// Wall-splat damage (`combat::splat_damage` with these numbers).
    #[inline]
    pub fn splat_damage(&self, speed: f32) -> f32 {
        if speed <= self.splat_min_speed { 0.0 } else { ((speed - self.splat_min_speed) * self.splat_dmg_per_ms).min(self.splat_max_dmg) }
    }
}

// ----------------------------------------------------------------------------------------- smash

/// The Smash model (COMBAT.md §1a). Knockback:
/// `kb = (kb_base + kb_growth · (pct_after / 100) · (dmg / kb_dmg_divisor + 1)) · (1 / weight) · kb_scale`.
/// Owned by the world engineer's Smash implementation (these are the agreed field names; the
/// numbers are starting points they tune).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmashTuning {
    pub kb_base: f32,
    pub kb_growth: f32,
    pub kb_dmg_divisor: f32,
    pub kb_scale: f32,
    /// Knockback above which a hit launches (tumble + launch trail).
    pub kb_min_launch: f32,
    /// Launch speed at which the trail reaches full intensity.
    pub launch_trail_kb: f32,
    pub weight_default: f32,
    pub weight_grow: f32,
    pub weight_shrink: f32,
    pub weight_suit: f32,
    /// DI: max rotation of the launch vector (degrees) and how long it takes to ramp in.
    pub di_max_deg: f32,
    pub di_ramp_secs: f32,
    /// Tech: Jump within this many seconds of a contact while tumbling cancels the splat/bounce.
    pub tech_window: f32,
    /// Blast zones beyond the arena bounds (m).
    pub blast_margin_side: f32,
    pub blast_margin_top: f32,
    /// Star KO: the KO lands this long after crossing the top blast zone.
    pub star_ko_delay: f32,
    /// Displayed / stored damage % cap.
    pub max_percent: f32,
    /// Damage % after a respawn.
    pub respawn_pct: f32,
    /// Launched players punching through terrain / constructs.
    pub punch_speed: f32,
    pub punch_radius: f32,
    pub punch_loss_per_m_per_hardness: f32,
    pub punch_hardness_speed_scale: f32,
    pub punch_construct_loss_per_kg: f32,
    pub punch_max_carves_per_tick: u16,
    /// Knockback cap (m/s).
    pub kb_max: f32,
    /// Tumble length: `min + kb · per_kb`, clamped to `max` (seconds).
    pub tumble_secs_min: f32,
    pub tumble_secs_per_kb: f32,
    pub tumble_secs_max: f32,
    /// Softest material hardness that still counts (leaves / glass).
    pub punch_min_hardness: f32,
    /// Carve power of the punch-through capsule (≥ every destructible hardness).
    pub punch_carve_power: f32,
    /// LAUNCH FEEL (modern launch, `smash::launch_knockback`; host rule `MatchRules::classic_launch`
    /// restores the formula above untouched). A move's base knockback is scaled by
    /// `curve_base_low` at 0 %, rising linearly to ×1 at `curve_base_full_pct`; strong moves
    /// (base knockback ≥ `curve_strong_kb`) keep up to `curve_strong_keep` more of it early. The
    /// %-growth term is multiplied by `curve_growth_mult`.
    pub curve_base_low: f32,
    pub curve_base_full_pct: f32,
    pub curve_strong_kb: f32,
    pub curve_strong_keep: f32,
    pub curve_growth_mult: f32,
    /// LAUNCH FEEL: hitstun (no air jump / air dodge) = `min + kb · per_kb`, capped at `max`; the
    /// rest of the tumble is actionable (still reduced air control while carrying the launch).
    pub hitstun_secs_min: f32,
    pub hitstun_secs_per_kb: f32,
    pub hitstun_secs_max: f32,
}

impl Default for SmashTuning {
    fn default() -> Self {
        Self {
            // kb_base multiplies each move's own base knockback (so moves keep their feel);
            // growth tuned so ~120–160 % + a heavy hit launches off a mid-size island.
            kb_base: 1.0,
            kb_growth: 4.0,
            kb_dmg_divisor: 10.0,
            kb_scale: 1.0,
            kb_min_launch: 22.0,
            launch_trail_kb: 30.0,
            weight_default: 1.0,
            weight_grow: 1.6,
            weight_shrink: 0.5,
            weight_suit: 1.3,
            di_max_deg: 15.0,
            di_ramp_secs: 0.1,
            tech_window: 0.15,
            blast_margin_side: 25.0,
            blast_margin_top: 40.0,
            star_ko_delay: 0.8,
            max_percent: 999.0,
            respawn_pct: 0.0,
            punch_speed: 22.0,
            punch_radius: 0.9,
            punch_loss_per_m_per_hardness: 4.0,
            punch_hardness_speed_scale: 1.0,
            punch_construct_loss_per_kg: 0.02,
            punch_max_carves_per_tick: 4,
            kb_max: 90.0,
            tumble_secs_min: 0.6,
            tumble_secs_per_kb: 0.02,
            tumble_secs_max: 2.5,
            punch_min_hardness: 0.25,
            punch_carve_power: 4.0,
            curve_base_low: 0.35,
            curve_base_full_pct: 90.0,
            curve_strong_kb: 20.0,
            curve_strong_keep: 0.65,
            curve_growth_mult: 1.15,
            hitstun_secs_min: 0.15,
            hitstun_secs_per_kb: 0.01,
            hitstun_secs_max: 0.75,
        }
    }
}

impl SmashTuning {
    fn validated(mut self) -> Self {
        let d = Self::default();
        self.kb_base = fin(self.kb_base, d.kb_base, 0.0, 200.0);
        self.kb_growth = fin(self.kb_growth, d.kb_growth, 0.0, 200.0);
        self.kb_dmg_divisor = fin(self.kb_dmg_divisor, d.kb_dmg_divisor, 0.1, 1000.0);
        self.kb_scale = fin(self.kb_scale, d.kb_scale, 0.0, 10.0);
        self.kb_min_launch = fin(self.kb_min_launch, d.kb_min_launch, 0.0, 200.0);
        self.launch_trail_kb = fin(self.launch_trail_kb, d.launch_trail_kb, 1.0, 500.0);
        self.weight_default = fin(self.weight_default, d.weight_default, 0.05, 20.0);
        self.weight_grow = fin(self.weight_grow, d.weight_grow, 0.05, 20.0);
        self.weight_shrink = fin(self.weight_shrink, d.weight_shrink, 0.05, 20.0);
        self.weight_suit = fin(self.weight_suit, d.weight_suit, 0.05, 20.0);
        self.di_max_deg = fin(self.di_max_deg, d.di_max_deg, 0.0, 90.0);
        self.kb_max = fin(self.kb_max, d.kb_max, 1.0, 500.0);
        self.tumble_secs_min = fin(self.tumble_secs_min, d.tumble_secs_min, 0.0, 5.0);
        self.tumble_secs_per_kb = fin(self.tumble_secs_per_kb, d.tumble_secs_per_kb, 0.0, 1.0);
        self.tumble_secs_max = fin(self.tumble_secs_max, d.tumble_secs_max, self.tumble_secs_min, 10.0);
        self.punch_min_hardness = fin(self.punch_min_hardness, d.punch_min_hardness, 0.01, 10.0);
        self.punch_carve_power = fin(self.punch_carve_power, d.punch_carve_power, 0.0, 10.0);
        self.di_ramp_secs = fin(self.di_ramp_secs, d.di_ramp_secs, 0.0, 5.0);
        self.tech_window = fin(self.tech_window, d.tech_window, 0.0, 2.0);
        self.blast_margin_side = fin(self.blast_margin_side, d.blast_margin_side, 0.0, 500.0);
        self.blast_margin_top = fin(self.blast_margin_top, d.blast_margin_top, 0.0, 500.0);
        self.star_ko_delay = fin(self.star_ko_delay, d.star_ko_delay, 0.0, 10.0);
        self.max_percent = fin(self.max_percent, d.max_percent, 1.0, 9999.0);
        self.respawn_pct = fin(self.respawn_pct, d.respawn_pct, 0.0, self.max_percent);
        self.punch_speed = fin(self.punch_speed, d.punch_speed, 0.0, 500.0);
        self.punch_radius = fin(self.punch_radius, d.punch_radius, 0.0, 5.0);
        self.punch_loss_per_m_per_hardness = fin(self.punch_loss_per_m_per_hardness, d.punch_loss_per_m_per_hardness, 0.0, 1000.0);
        self.punch_hardness_speed_scale = fin(self.punch_hardness_speed_scale, d.punch_hardness_speed_scale, 0.0, 100.0);
        self.punch_construct_loss_per_kg = fin(self.punch_construct_loss_per_kg, d.punch_construct_loss_per_kg, 0.0, 100.0);
        self.punch_max_carves_per_tick = self.punch_max_carves_per_tick.min(64);
        self.curve_base_low = fin(self.curve_base_low, d.curve_base_low, 0.0, 1.0);
        self.curve_base_full_pct = fin(self.curve_base_full_pct, d.curve_base_full_pct, 1.0, 999.0);
        self.curve_strong_kb = fin(self.curve_strong_kb, d.curve_strong_kb, 0.1, 500.0);
        self.curve_strong_keep = fin(self.curve_strong_keep, d.curve_strong_keep, 0.0, 1.0);
        self.curve_growth_mult = fin(self.curve_growth_mult, d.curve_growth_mult, 0.0, 10.0);
        self.hitstun_secs_min = fin(self.hitstun_secs_min, d.hitstun_secs_min, 0.0, 5.0);
        self.hitstun_secs_per_kb = fin(self.hitstun_secs_per_kb, d.hitstun_secs_per_kb, 0.0, 1.0);
        self.hitstun_secs_max = fin(self.hitstun_secs_max, d.hitstun_secs_max, self.hitstun_secs_min, 10.0);
        self
    }
}

// ----------------------------------------------------------------------------------------- forms

/// One row of the form table.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormNumbers {
    pub form: Form,
    pub damage: f32,
    pub knockback: f32,
}

impl FormNumbers {
    pub fn of(form: Form) -> Self {
        let s = form_spec(form);
        Self { form, damage: s.damage, knockback: s.knockback }
    }
}

/// One gun archetype row (see [`GunSpec`] for the meaning of each field).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GunNumbers {
    pub arch: GunArchetype,
    pub cadence: f32,
    pub damage: f32,
    pub knockback: f32,
    pub speed: f32,
    pub recoil: f32,
    pub pellets: u8,
    pub spread_deg: f32,
    pub falloff_start: f32,
    pub spin_up: f32,
    pub spin_down: f32,
    pub clip: u8,
    pub reload: f32,
    pub charge: f32,
    /// Railgun early release: fraction of the full shot at a tap (0 = an early release fires nothing).
    #[serde(default)]
    pub release_min: f32,
    pub explode: Option<(f32, f32)>,
    pub pierce: u8,
    pub chip: f32,
    pub radius: f32,
}

impl GunNumbers {
    pub fn of(arch: GunArchetype) -> Self {
        let s = gun_spec(arch);
        Self {
            arch,
            cadence: s.cadence,
            damage: s.damage,
            knockback: s.knockback,
            speed: s.speed,
            recoil: s.recoil,
            pellets: s.pellets,
            spread_deg: s.spread_deg,
            falloff_start: s.falloff_start,
            spin_up: s.spin_up,
            spin_down: s.spin_down,
            clip: s.clip,
            reload: s.reload,
            charge: s.charge,
            release_min: s.release_min,
            explode: s.explode,
            pierce: s.pierce,
            chip: s.chip,
            radius: s.radius,
        }
    }

    fn validated(mut self) -> Self {
        let d = Self::of(self.arch);
        self.cadence = fin(self.cadence, d.cadence, 0.02, 10.0);
        self.damage = fin(self.damage, d.damage, 0.0, 300.0);
        self.knockback = fin(self.knockback, d.knockback, 0.0, 120.0);
        self.speed = fin(self.speed, d.speed, 0.0, 600.0);
        // Hitscan stays hitscan and projectiles stay projectiles (behaviour, not numbers).
        if (d.speed == 0.0) != (self.speed == 0.0) {
            self.speed = d.speed;
        }
        self.recoil = fin(self.recoil, d.recoil, 0.0, 40.0);
        self.pellets = self.pellets.clamp(1, 16);
        self.spread_deg = fin(self.spread_deg, d.spread_deg, 0.0, 45.0);
        self.falloff_start = fin(self.falloff_start, d.falloff_start, 0.0, 500.0);
        self.spin_up = fin(self.spin_up, d.spin_up, 0.0, 5.0);
        self.spin_down = fin(self.spin_down, d.spin_down, 0.0, 5.0);
        self.clip = self.clip.min(64);
        self.reload = fin(self.reload, d.reload, 0.0, 10.0);
        self.charge = fin(self.charge, d.charge, 0.0, 5.0);
        self.release_min = fin(self.release_min, d.release_min, 0.0, 1.0);
        self.explode = match (self.explode, d.explode) {
            (Some((b, c)), Some(dd)) => Some((fin(b, dd.0, 0.1, 20.0), fin(c, dd.1, 0.0, 20.0))),
            (Some((b, c)), None) => Some((fin(b, 2.0, 0.1, 20.0), fin(c, 1.0, 0.0, 20.0))),
            (None, _) => None,
        };
        self.pierce = self.pierce.min(8);
        self.chip = fin(self.chip, d.chip, 0.0, 3.0);
        self.radius = fin(self.radius, d.radius, 0.02, 2.0);
        self
    }

    /// Apply this row over the archetype's built-in spec (keeps its projectile kind).
    pub fn apply(&self, base: GunSpec) -> GunSpec {
        GunSpec {
            cadence: self.cadence,
            damage: self.damage,
            knockback: self.knockback,
            speed: self.speed,
            recoil: self.recoil,
            pellets: self.pellets,
            spread_deg: self.spread_deg,
            falloff_start: self.falloff_start,
            spin_up: self.spin_up,
            spin_down: self.spin_down,
            clip: self.clip,
            reload: self.reload,
            charge: self.charge,
            release_min: self.release_min,
            explode: self.explode,
            pierce: self.pierce,
            chip: self.chip,
            radius: self.radius,
            ..base
        }
    }
}

// -------------------------------------------------------------------------------------- creature

/// Creature attack numbers that are not the form's base damage (COMBAT.md §4 Creatures).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatureTuning {
    pub charge_knockback: f32,
    /// Breath damage per pulse.
    pub breath_damage: f32,
    pub stomp_damage: f32,
    pub stomp_damage_kaiju: f32,
    pub stomp_knockback: f32,
    pub stomp_knockback_kaiju: f32,
    pub tail_knockback: f32,
    /// Damage of one swarm unit's nip.
    pub swarm_unit_damage: f32,
}

impl Default for CreatureTuning {
    fn default() -> Self {
        use crate::constructs::{creature, swarm};
        Self {
            charge_knockback: creature::CHARGE_KB,
            breath_damage: creature::BREATH_DMG,
            stomp_damage: 10.0,
            stomp_damage_kaiju: 18.0,
            stomp_knockback: 14.0,
            stomp_knockback_kaiju: 20.0,
            tail_knockback: 20.0,
            swarm_unit_damage: swarm::UNIT_DAMAGE,
        }
    }
}

impl CreatureTuning {
    fn validated(mut self) -> Self {
        let d = Self::default();
        self.charge_knockback = fin(self.charge_knockback, d.charge_knockback, 0.0, 100.0);
        self.breath_damage = fin(self.breath_damage, d.breath_damage, 0.0, 100.0);
        self.stomp_damage = fin(self.stomp_damage, d.stomp_damage, 0.0, 200.0);
        self.stomp_damage_kaiju = fin(self.stomp_damage_kaiju, d.stomp_damage_kaiju, 0.0, 200.0);
        self.stomp_knockback = fin(self.stomp_knockback, d.stomp_knockback, 0.0, 100.0);
        self.stomp_knockback_kaiju = fin(self.stomp_knockback_kaiju, d.stomp_knockback_kaiju, 0.0, 100.0);
        self.tail_knockback = fin(self.tail_knockback, d.tail_knockback, 0.0, 100.0);
        self.swarm_unit_damage = fin(self.swarm_unit_damage, d.swarm_unit_damage, 0.0, 100.0);
        self
    }
}

// ------------------------------------------------------------------------------------- willpower

/// Extra cast-cost weight for one mechanic (added to the flat `per_mechanic`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MechanicCost {
    pub mechanic: Mechanic,
    pub extra: f32,
}

/// Willpower multiplier for one form: scales its cast cost **and** its upkeep (a held gun's
/// damage is sustained, so pricing only the cast would barely move it).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormCost {
    pub form: Form,
    pub mult: f32,
}

/// The willpower economy (COMBAT.md §2–3).
///
/// Cost = `base(form) · form_mult · size · (1 + per_mechanic·n + Σ extra(m)) · count · staleness`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WillpowerTuning {
    pub base_held: f32,
    pub base_worn: f32,
    pub base_structure: f32,
    pub base_object: f32,
    pub base_creature: f32,
    pub base_kaiju: f32,
    /// Autonomous non-creatures (held / structure / object that `Flies` or `Walks`: "flying
    /// hammer", "wall with legs") are priced like creatures — this cast base and upkeep. Decided on
    /// the recipe the player typed (party / power grafts never raise a price). Behaviour unchanged.
    pub autonomous_pricing: bool,
    pub autonomous_base: f32,
    pub autonomous_upkeep: f32,
    /// Flat weight of each mechanic (a payload counts as one).
    pub per_mechanic: f32,
    /// Balance knob: extra weight per mechanic (empty = none).
    pub mechanic_cost: Vec<MechanicCost>,
    /// Balance knob: multiplier per form on cast cost and upkeep (empty = ×1).
    pub form_cost: Vec<FormCost>,
    /// `size_factor = clamp((scale/default)^size_exp, size_min, size_max)`.
    pub size_exp: f32,
    pub size_min: f32,
    pub size_max: f32,
    /// `count^count_exp` (swarms: `count^swarm_count_exp`).
    pub count_exp: f32,
    pub swarm_count_exp: f32,
    /// Staleness `1 + stale_per_use · uses` within `stale_window` seconds.
    pub stale_window: f32,
    pub stale_per_use: f32,
    pub regen_per_sec: f32,
    pub regen_pause: f32,
    /// Upkeep per second while the construct lives.
    pub upkeep_held_worn: f32,
    pub upkeep_structure: f32,
    pub upkeep_object: f32,
    pub upkeep_creature: f32,
    pub upkeep_kaiju: f32,
    /// Piloting drains at least this (colossal things: `upkeep_pilot_colossal`).
    pub upkeep_pilot: f32,
    pub upkeep_pilot_colossal: f32,
    pub graft_cost_fraction: f32,
    pub dismiss_refund: f32,
    /// At 0 willpower a player's constructs lose this fraction of max integrity per second.
    pub starved_decay: f32,
    /// Spoken oath recital: seconds of transcription latency the host forgives before its own
    /// measured time floors the claimed speaking time (`willpower::OATH_VOICE_SLACK`).
    pub oath_voice_slack: f32,
    /// SPARKS: cast prices in whole sparks by presence tier (`willpower::cast_sparks`).
    pub sparks: willpower::SparkTuning,
}

impl Default for WillpowerTuning {
    fn default() -> Self {
        Self {
            base_held: willpower::category_base(Form::Hammer),
            base_worn: willpower::category_base(Form::Wings),
            base_structure: willpower::category_base(Form::Wall),
            base_object: willpower::category_base(Form::Ball),
            base_creature: willpower::category_base(Form::Biped),
            base_kaiju: willpower::category_base(Form::Kaiju),
            autonomous_pricing: true,
            autonomous_base: willpower::AUTONOMOUS_BASE,
            autonomous_upkeep: willpower::AUTONOMOUS_UPKEEP,
            per_mechanic: 0.15,
            mechanic_cost: Vec::new(),
            // Shipped balance (tournament passes, §5i.1): held guns / staves are sustained ranged
            // damage, so they cost (and upkeep) more. Kept equal to assets/tuning/combat.ron so
            // tests, bots, tournaments and the game agree.
            form_cost: vec![FormCost { form: Form::Gun, mult: 2.6 }, FormCost { form: Form::Staff, mult: 2.0 }],
            size_exp: 1.3,
            size_min: 0.5,
            size_max: 3.0,
            count_exp: 0.6,
            swarm_count_exp: 0.4,
            stale_window: willpower::STALE_WINDOW_SECS,
            stale_per_use: willpower::STALE_PER_USE,
            regen_per_sec: willpower::REGEN_PER_SEC,
            regen_pause: willpower::REGEN_PAUSE_AFTER_CAST,
            upkeep_held_worn: 0.6,
            upkeep_structure: 0.3,
            upkeep_object: 0.0,
            upkeep_creature: 1.5,
            upkeep_kaiju: 4.0,
            upkeep_pilot: 1.5,
            upkeep_pilot_colossal: 4.0,
            graft_cost_fraction: willpower::GRAFT_COST_FRACTION,
            dismiss_refund: willpower::DISMISS_REFUND,
            starved_decay: willpower::STARVED_DECAY_PER_SEC,
            oath_voice_slack: willpower::OATH_VOICE_SLACK,
            sparks: willpower::SparkTuning::default(),
        }
    }
}

impl WillpowerTuning {
    fn validated(mut self) -> Self {
        let d = Self::default();
        self.base_held = fin(self.base_held, d.base_held, 0.0, 500.0);
        self.base_worn = fin(self.base_worn, d.base_worn, 0.0, 500.0);
        self.base_structure = fin(self.base_structure, d.base_structure, 0.0, 500.0);
        self.base_object = fin(self.base_object, d.base_object, 0.0, 500.0);
        self.base_creature = fin(self.base_creature, d.base_creature, 0.0, 500.0);
        self.base_kaiju = fin(self.base_kaiju, d.base_kaiju, 0.0, 500.0);
        self.autonomous_base = fin(self.autonomous_base, d.autonomous_base, 0.0, 500.0);
        self.autonomous_upkeep = fin(self.autonomous_upkeep, d.autonomous_upkeep, 0.0, 100.0);
        self.per_mechanic = fin(self.per_mechanic, d.per_mechanic, 0.0, 5.0);
        self.mechanic_cost.truncate(64);
        for m in &mut self.mechanic_cost {
            m.extra = fin(m.extra, 0.0, -0.15, 5.0);
        }
        self.form_cost.truncate(64);
        for f in &mut self.form_cost {
            f.mult = fin(f.mult, 1.0, 0.1, 10.0);
        }
        self.size_exp = fin(self.size_exp, d.size_exp, 0.0, 4.0);
        self.size_min = fin(self.size_min, d.size_min, 0.01, 10.0);
        self.size_max = fin(self.size_max, d.size_max, self.size_min, 50.0);
        self.count_exp = fin(self.count_exp, d.count_exp, 0.0, 2.0);
        self.swarm_count_exp = fin(self.swarm_count_exp, d.swarm_count_exp, 0.0, 2.0);
        self.stale_window = fin(self.stale_window, d.stale_window, 0.0, 600.0);
        self.stale_per_use = fin(self.stale_per_use, d.stale_per_use, 0.0, 5.0);
        self.regen_per_sec = fin(self.regen_per_sec, d.regen_per_sec, 0.0, 100.0);
        self.regen_pause = fin(self.regen_pause, d.regen_pause, 0.0, 30.0);
        self.upkeep_held_worn = fin(self.upkeep_held_worn, d.upkeep_held_worn, 0.0, 100.0);
        self.upkeep_structure = fin(self.upkeep_structure, d.upkeep_structure, 0.0, 100.0);
        self.upkeep_object = fin(self.upkeep_object, d.upkeep_object, 0.0, 100.0);
        self.upkeep_creature = fin(self.upkeep_creature, d.upkeep_creature, 0.0, 100.0);
        self.upkeep_kaiju = fin(self.upkeep_kaiju, d.upkeep_kaiju, 0.0, 100.0);
        self.upkeep_pilot = fin(self.upkeep_pilot, d.upkeep_pilot, 0.0, 100.0);
        self.upkeep_pilot_colossal = fin(self.upkeep_pilot_colossal, d.upkeep_pilot_colossal, 0.0, 100.0);
        self.graft_cost_fraction = fin(self.graft_cost_fraction, d.graft_cost_fraction, 0.0, 5.0);
        self.dismiss_refund = fin(self.dismiss_refund, d.dismiss_refund, 0.0, 1.0);
        self.starved_decay = fin(self.starved_decay, d.starved_decay, 0.0, 10.0);
        self.oath_voice_slack = fin(self.oath_voice_slack, d.oath_voice_slack, willpower::OATH_LATENCY_SLACK, 10.0);
        self.sparks.tier = self.sparks.tier.map(|x| x.clamp(1, 24));
        self.sparks.legendary = self.sparks.legendary.clamp(1, 24);
        self.sparks.cap = self.sparks.cap.clamp(1, 24);
        self.sparks.crowd_count = self.sparks.crowd_count.max(2);
        self
    }

    /// Category base cost (`willpower::category_base`).
    pub fn category_base(&self, form: Form) -> f32 {
        if form == Form::Kaiju {
            return self.base_kaiju;
        }
        match form.category() {
            FormCategory::Held => self.base_held,
            FormCategory::Worn => self.base_worn,
            FormCategory::Structure => self.base_structure,
            FormCategory::Object => self.base_object,
            FormCategory::Creature => self.base_creature,
        }
    }

    /// `willpower::size_factor`.
    pub fn size_factor(&self, recipe: &Recipe) -> f32 {
        let d = form_spec(recipe.form).default_scale;
        (recipe.scale / d).powf(self.size_exp).clamp(self.size_min, self.size_max)
    }

    /// `willpower::count_factor`.
    pub fn count_factor(&self, recipe: &Recipe) -> f32 {
        let c = recipe.count.max(1) as f32;
        if recipe.has(Mechanic::Swarm) { c.powf(self.swarm_count_exp) } else { c.powf(self.count_exp) }
    }

    /// Mechanic factor `1 + per_mechanic·n + Σ extra(m)` (a payload counts as one plain mechanic).
    pub fn mechanic_factor(&self, recipe: &Recipe) -> f32 {
        let n = recipe.mechanics.len() + recipe.payload.is_some() as usize;
        let mut f = 1.0 + self.per_mechanic * n as f32;
        if !self.mechanic_cost.is_empty() {
            for m in &recipe.mechanics {
                if let Some(e) = self.mechanic_cost.iter().find(|e| e.mechanic == *m) {
                    f += e.extra;
                }
            }
            f = f.max(0.1);
        }
        f
    }

    /// Multiplier for this form (1 if none).
    pub fn form_mult(&self, form: Form) -> f32 {
        self.form_cost.iter().find(|f| f.form == form).map_or(1.0, |f| f.mult)
    }

    /// Is `recipe` priced as an autonomous non-creature? Call it on the **typed** recipe.
    pub fn is_autonomous(&self, recipe: &Recipe) -> bool {
        self.autonomous_pricing && willpower::is_autonomous(recipe)
    }

    /// Cast cost before staleness (`willpower::base_cost` with these numbers), autonomy decided
    /// from `recipe` itself.
    pub fn base_cost(&self, recipe: &Recipe) -> f32 {
        self.base_cost_as(recipe, self.is_autonomous(recipe))
    }

    /// Cast cost before staleness with the autonomy pricing decided by the caller (the typed
    /// recipe's). The original operation order is kept (base · size · mechanics · count), so
    /// defaults are bit-identical to `willpower::base_cost_as`.
    pub fn base_cost_as(&self, recipe: &Recipe, autonomous: bool) -> f32 {
        let mut base = if autonomous && recipe.form.category() != FormCategory::Creature { self.autonomous_base } else { self.category_base(recipe.form) };
        if !self.form_cost.is_empty() {
            base *= self.form_mult(recipe.form);
        }
        base * self.size_factor(recipe) * self.mechanic_factor(recipe) * self.count_factor(recipe)
    }

    /// [`Self::base_cost`] without the balance knobs (`mechanic_cost` / `form_cost`) and without
    /// autonomy pricing. Integrity is derived from this, so a price change changes nothing else.
    pub fn base_cost_unadjusted(&self, recipe: &Recipe) -> f32 {
        let n = recipe.mechanics.len() + recipe.payload.is_some() as usize;
        self.category_base(recipe.form) * self.size_factor(recipe) * (1.0 + self.per_mechanic * n as f32) * self.count_factor(recipe)
    }

    /// Ratio `unadjusted / adjusted` for a recipe (1 without balance knobs or autonomy pricing).
    pub fn adjustment_ratio(&self, recipe: &Recipe) -> f32 {
        self.adjustment_ratio_as(recipe, self.is_autonomous(recipe))
    }

    /// [`Self::adjustment_ratio`] with the caller's autonomy decision.
    pub fn adjustment_ratio_as(&self, recipe: &Recipe, autonomous: bool) -> f32 {
        if self.mechanic_cost.is_empty() && self.form_cost.is_empty() && !autonomous {
            return 1.0;
        }
        let a = self.base_cost_as(recipe, autonomous);
        if a > 1e-6 { self.base_cost_unadjusted(recipe) / a } else { 1.0 }
    }

    /// `1 + stale_per_use · uses`.
    pub fn staleness(&self, uses: u32) -> f32 {
        1.0 + self.stale_per_use * uses as f32
    }

    /// Willpower drained per second while the construct lives (`willpower::upkeep_per_sec`),
    /// autonomy decided from `recipe` itself.
    pub fn upkeep_per_sec(&self, recipe: &Recipe, piloted: bool) -> f32 {
        self.upkeep_per_sec_as(recipe, piloted, self.is_autonomous(recipe))
    }

    /// Upkeep with the caller's autonomy decision (a construct keeps the one it was cast with).
    pub fn upkeep_per_sec_as(&self, recipe: &Recipe, piloted: bool, autonomous: bool) -> f32 {
        let cat: f32 = if recipe.form == Form::Kaiju {
            self.upkeep_kaiju
        } else {
            match recipe.form.category() {
                FormCategory::Held | FormCategory::Worn => self.upkeep_held_worn,
                FormCategory::Creature => self.upkeep_creature,
                FormCategory::Structure => self.upkeep_structure,
                FormCategory::Object => self.upkeep_object,
            }
        };
        let base = if autonomous && recipe.form != Form::Kaiju && recipe.form.category() != FormCategory::Creature { self.autonomous_upkeep.max(cat) } else { cat };
        let v = if piloted {
            let p = if willpower::is_colossal(recipe) { self.upkeep_pilot_colossal } else { self.upkeep_pilot };
            base.max(p)
        } else {
            base
        };
        // A form's cost multiplier prices its upkeep too (held weapons are sustained damage).
        if self.form_cost.is_empty() { v } else { v * self.form_mult(recipe.form) }
    }
}

// ------------------------------------------------------------------------------------ construct

/// Construct integrity (COMBAT.md §3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConstructTuning {
    /// Integrity = `integrity_base + integrity_per_cost · cost`.
    pub integrity_base: f32,
    pub integrity_per_cost: f32,
}

impl Default for ConstructTuning {
    fn default() -> Self {
        Self { integrity_base: 20.0, integrity_per_cost: 4.0 }
    }
}

impl ConstructTuning {
    fn validated(mut self) -> Self {
        let d = Self::default();
        self.integrity_base = fin(self.integrity_base, d.integrity_base, 1.0, 5000.0);
        self.integrity_per_cost = fin(self.integrity_per_cost, d.integrity_per_cost, 0.0, 100.0);
        self
    }

    /// `willpower::integrity_for_cost`.
    pub fn integrity_for_cost(&self, cost: f32) -> f32 {
        self.integrity_base + self.integrity_per_cost * cost
    }
}

// ---------------------------------------------------------------------------------- anti-snowball

/// Soft anti-snowball for FFA (COMBAT.md §5i.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AntiSnowball {
    /// Respawn with at least this fraction of the willpower pool (§5i: full = 1.0; the pre-§5i
    /// sim used 0.5).
    pub respawn_willpower_frac: f32,
    /// Ending a streak of at least this many KOs is a shutdown…
    pub shutdown_streak: u16,
    /// …worth this many extra KO points to the killer (only while the match is live).
    pub shutdown_bonus: u16,
}

impl AntiSnowball {
    /// The pre-§5i behaviour (respawn with ≥ 50 % willpower, no shutdown bonus).
    pub const LEGACY: Self = Self { respawn_willpower_frac: 0.5, shutdown_streak: 3, shutdown_bonus: 0 };
}

impl Default for AntiSnowball {
    fn default() -> Self {
        Self { respawn_willpower_frac: 1.0, shutdown_streak: 3, shutdown_bonus: 1 }
    }
}

impl AntiSnowball {
    fn validated(mut self) -> Self {
        self.respawn_willpower_frac = fin(self.respawn_willpower_frac, 1.0, 0.0, 1.0);
        self.shutdown_streak = self.shutdown_streak.clamp(1, 100);
        self.shutdown_bonus = self.shutdown_bonus.min(10);
        self
    }
}

// ------------------------------------------------------------------------------------ top level

impl CombatTuning {
    /// Tuning that reproduces the economy from before the §5i balance passes (no anti-snowball,
    /// no autonomy pricing, no `form_cost` / `mechanic_cost` knobs).
    pub fn legacy() -> Self {
        let mut t = Self { snowball: AntiSnowball::LEGACY, ..Self::default() };
        t.willpower.autonomous_pricing = false;
        t.willpower.form_cost.clear();
        t.willpower.mechanic_cost.clear();
        t
    }

    /// Clamp everything into sane ranges (untrusted / hand-edited data; NaN → default). Missing
    /// form / gun rows are filled from the built-in tables; duplicates keep the first row.
    pub fn validated(mut self) -> Self {
        self.ring = self.ring.validated();
        self.hit = self.hit.validated();
        self.smash = self.smash.validated();
        let mut forms = Vec::with_capacity(crate::fuzz::FORMS.len());
        for &f in &crate::fuzz::FORMS {
            let d = FormNumbers::of(f);
            let row = self.forms.iter().find(|r| r.form == f).copied().unwrap_or(d);
            forms.push(FormNumbers { form: f, damage: fin(row.damage, d.damage, 0.0, 300.0), knockback: fin(row.knockback, d.knockback, 0.0, 120.0) });
        }
        self.forms = forms;
        let mut guns = Vec::with_capacity(GUN_ARCHETYPES.len());
        for &a in &GUN_ARCHETYPES {
            let row = self.guns.iter().find(|r| r.arch == a).copied().unwrap_or(GunNumbers::of(a));
            guns.push(row.validated());
        }
        self.guns = guns;
        self.creature = self.creature.validated();
        self.willpower = self.willpower.validated();
        self.construct = self.construct.validated();
        self.snowball = self.snowball.validated();
        self.charge = self.charge.validated();
        self.bounds = self.bounds.validated();
        self.summon = self.summon.validated();
        self.assist = self.assist.validated(); // ASSIST HOOK
        self.signature = self.signature.validated(); // SIGNATURE HOOK
        self.freedom = self.freedom.validated(); // FREEDOM HOOK
        self.juggle = self.juggle.validated(); // JUGGLE
        self.launch = self.launch.validated(); // WILLPOWER = LAUNCH
        self.casino = self.casino.validated(); // INTERACT HOOK
        self.giants = self.giants.validated(); // GIANTS
        self.pickups = self.pickups.validated(); // POWER-UPS
        self.extend = self.extend.validated(); // EXTENDS
        self.legendary = self.legendary.validated(); // LEGENDARY
        self.transformer = self.transformer.validated(); // TRANSFORMER COMBAT
        self.weapons = self.weapons.validated(); // WEAPON FEEL
        self
    }

    /// Base (damage, knockback) of a form's default attack / impact.
    #[inline]
    pub fn form(&self, form: Form) -> (f32, f32) {
        match self.forms.iter().find(|r| r.form == form) {
            Some(r) => (r.damage, r.knockback),
            None => {
                let s = form_spec(form);
                (s.damage, s.knockback)
            }
        }
    }

    /// [`form_spec`] with this tuning's damage / knockback.
    pub fn form_spec(&self, form: Form) -> crate::constructs::forms::FormSpec {
        let (damage, knockback) = self.form(form);
        crate::constructs::forms::FormSpec { damage, knockback, ..form_spec(form) }
    }

    /// Archetype numbers before caliber scaling (`weapons::gun_spec` with this tuning).
    pub fn gun_spec(&self, a: GunArchetype) -> GunSpec {
        if a == GunArchetype::Ring {
            return self.ring.blast_spec();
        }
        let base = gun_spec(a);
        match self.guns.iter().find(|g| g.arch == a) {
            Some(g) => g.apply(base),
            None => base,
        }
    }

    /// Caliber-scaled spec for a held recipe (`weapons::spec_for` with this tuning).
    pub fn spec_for(&self, r: &Recipe, a: GunArchetype) -> GunSpec {
        crate::weapons::scale_spec(self.gun_spec(a), r, a)
    }

    // -------------------------------------------------------------------------------- RON

    /// Pretty RON (the `assets/tuning/combat.ron` format).
    pub fn to_ron(&self) -> Result<String, String> {
        let body = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default().struct_names(false)).map_err(|e| e.to_string())?;
        Ok(format!(
            "// Combat tuning (bd_sim::tuning::CombatTuning, COMBAT.md §5i). Hot-reloaded while the game runs.\n\
             // Missing fields use the built-in defaults. Balance with willpower cost (willpower.mechanic_cost /\n\
             // willpower.form_cost), not damage. Never shown to players.\n{body}\n"
        ))
    }

    /// Parse (partial files fill in defaults) and validate.
    pub fn from_ron(src: &str) -> Result<Self, String> {
        ron::from_str::<Self>(src).map(Self::validated).map_err(|e| e.to_string())
    }

    /// Load `path`, or `None` if it is missing / unreadable / invalid (callers keep their tuning).
    pub fn load(path: &std::path::Path) -> Option<Self> {
        let src = std::fs::read_to_string(path).ok()?;
        Self::from_ron(&src).ok()
    }
}

// ------------------------------------------------------------------------------------ world API

impl crate::World {
    /// The combat tuning this world runs with.
    #[inline]
    pub fn tuning(&self) -> &CombatTuning {
        &self.cfg.tuning
    }

    /// Replace the combat tuning (hot reload of `assets/tuning/combat.ron`). Validated first.
    /// Takes effect from the next tick / cast; live constructs keep the cost and integrity they
    /// were cast with. Host-authoritative: predicting clients must receive the same tuning.
    pub fn set_tuning(&mut self, t: CombatTuning) {
        self.cfg.tuning = t.validated();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_round_trip() {
        let t = CombatTuning::default();
        assert_eq!(t.clone().validated(), t);
        let ron = t.to_ron().unwrap();
        assert_eq!(CombatTuning::from_ron(&ron).unwrap(), t);
        assert_eq!(CombatTuning::from_ron("()").unwrap(), t);
    }

    #[test]
    fn hostile_values_are_clamped() {
        let mut t = CombatTuning::default();
        t.ring.blast_damage = f32::NAN;
        t.willpower.size_max = -5.0;
        t.guns.clear();
        t.forms.truncate(3);
        let v = t.validated();
        assert_eq!(v.ring.blast_damage, combat::BLAST_DAMAGE);
        assert!(v.willpower.size_max >= v.willpower.size_min);
        assert_eq!(v.guns.len(), GUN_ARCHETYPES.len());
        assert_eq!(v.forms.len(), crate::fuzz::FORMS.len());
    }
}
