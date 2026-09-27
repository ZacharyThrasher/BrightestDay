//! **WEAPON FEEL** — weapon classes (owner: "heavy weapons should feel noticeably different than
//! light weapons").
//!
//! Every held weapon belongs to a class derived from its data alone — form, size relative to the
//! form's default, `bd_shapes::weight_of`, mechanics — never its name ([`class_of`]):
//!
//! | class | what lands here (generic rule) | examples |
//! |---|---|---|
//! | Light | small blades (≤ 0.72 × the form's size), fast piercing blades, gauntlets | knife, dagger, rapier, fists |
//! | Medium | everything in between (the form's own moveset, unchanged) | sword, katana, mace, hammer, axe |
//! | Heavy | `Heavy`, ≥ 1.25 × size, or weight ≥ 1.9 | greatsword, warhammer, great axe, anchor, sledgehammer |
//! | Reach | spears, blunt staffs (quarterstaffs), telescoping poles | spear, bo staff, glaive, trident |
//! | Flexible | whips and chains | whip, chain, nunchaku |
//! | Shield | shields | shield |
//! | LightGun / HeavyGun | ranged held; heavy = cannons, Spin, Explodes, Pierce/Beam guns, `Heavy`, ≥ 1.5 × size | pistol, SMG, shotgun, bow / minigun, LMG, rocket, railgun |
//!
//! Each class carries its own moveset and feel in the sim (numbers: `assets/tuning/weapons.ron` →
//! [`WeaponTuning`], host-synced inside `CombatTuning::weapons`; curves: `bd_shapes::swing_class`)
//! and in the presentation (`bd_game::present::weapon_feel`):
//!
//! * **Light** — flick curves, faster swings, a 3-hit string whose last hit crits, jump / dodge /
//!   boost cancels the recovery (keep your string going off a hop), low knockback, small hit-stop.
//! * **Heavy** — an overhead chop / wide sweep pair with a long, readable wind-up; commitment (your
//!   ground speed is braked when the swing starts, then the strike carries you forward); super armor
//!   while winding up and swinging (knockback × `armor`, never immunity); big knockback with lift;
//!   whiffs into terrain crack it; a fully charged heavy breaks the ground (a shockwave); heavy to
//!   carry (sprint and air-strafe multipliers).
//! * **Reach** — thrust, twisting thrust, wide sweep; RMB lunge thrust; the tip is the sweet spot.
//! * **Flexible** — the whip's tip cracks (sweet spot: bonus damage and a crit).
//! * **Shield** — the bash steps you in.
//! * **Guns** — heavy guns are heavy to carry and their recoil shoves you on the ground too.
//!
//! Telescoping poles (`Mechanic::Extends`, the Ruyi staff) read as Reach but keep their own sim
//! moveset untouched ([`sim_class`] skips them). Legendary rules (`legend.rs`) run on top of the
//! class (a heavy blade keeps its 3-hit momentum combo and its own armor; the stronger armor wins).
//!
//! Deterministic, allocation-free per tick (curve tables are built once per weight bin); a world
//! without held weapons is untouched (the FFA golden hash doesn't move).

use std::sync::OnceLock;

use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Recipe};
use bd_core::{PlayerId, Vec3};
use bd_shapes::{MoveSet, SwingCurve};
use serde::{Deserialize, Serialize};

use super::held::{self, HeldState, Mode, Swing, WEIGHT_BINS};
use super::{Construct, ConstructOut, ConstructStore, Env, form_spec, player_key};
use crate::combat::{CarveRequest, HitInfo, HitKind};
use crate::events::HeldFx;
use crate::geom;

// ------------------------------------------------------------------------------------ classes

/// A held weapon's class (see the module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum WeaponClass {
    Light = 0,
    #[default]
    Medium = 1,
    Heavy = 2,
    Reach = 3,
    Flexible = 4,
    Shield = 5,
    LightGun = 6,
    HeavyGun = 7,
}

impl WeaponClass {
    pub const ALL: [WeaponClass; 8] = {
        use WeaponClass::*;
        [Light, Medium, Heavy, Reach, Flexible, Shield, LightGun, HeavyGun]
    };

    pub fn is_gun(self) -> bool {
        matches!(self, WeaponClass::LightGun | WeaponClass::HeavyGun)
    }

    /// Key used in `assets/tuning/weapons.ron` (and the presentation's `feel` table).
    pub fn key(self) -> &'static str {
        match self {
            WeaponClass::Light => "light",
            WeaponClass::Medium => "medium",
            WeaponClass::Heavy => "heavy",
            WeaponClass::Reach => "reach",
            WeaponClass::Flexible => "flexible",
            WeaponClass::Shield => "shield",
            WeaponClass::LightGun => "light_gun",
            WeaponClass::HeavyGun => "heavy_gun",
        }
    }
}

/// At or under this fraction of the form's default size a blade / hammer is Light.
pub const LIGHT_SIZE: f32 = 0.72;
/// At or over this fraction of the form's default size a melee weapon is Heavy.
pub const HEAVY_SIZE: f32 = 1.25;
/// `bd_shapes::weight_of` at or over this makes a melee weapon Heavy.
pub const HEAVY_WEIGHT: f32 = 1.9;
/// A ranged weapon this much bigger than its form's default is Heavy.
pub const HEAVY_GUN_SIZE: f32 = 1.5;
/// A gauntlet this much bigger than its default is Heavy (a titan's fist).
pub const HEAVY_FIST_SIZE: f32 = 1.8;

/// Size relative to the form's default (1 = a nominal sword / hammer / spear…).
pub fn rel_size(r: &Recipe) -> f32 {
    let d = form_spec(r.form).default_scale.max(0.05);
    let s = if r.scale.is_finite() && r.scale > 0.0 { r.scale } else { d };
    s / d
}

/// A blunt, non-shooting, non-elemental staff is a quarterstaff (melee pole, Reach class), not
/// a bolt caster: "staff", "bo staff", "naginata". "fire staff", "wand", "beam staff" still cast.
pub fn staff_is_pole(r: &Recipe) -> bool {
    r.form == Form::Staff
        && (r.has(Mechanic::Extends)
            || (r.has(Mechanic::Blunt)
                && r.element == Element::Light
                && !r.has(Mechanic::Shoots)
                && !r.has(Mechanic::Beam)
                && !r.has(Mechanic::Explodes)
                && !r.has(Mechanic::Homing)
                && r.payload.is_none()))
}

/// The class of a held weapon recipe (`None` for non-held forms and palm energy).
pub fn class_of(r: &Recipe) -> Option<WeaponClass> {
    use WeaponClass::*;
    if r.form.category() != FormCategory::Held || r.form == Form::Hands {
        return None;
    }
    let rel = rel_size(r);
    let heavy = r.has(Mechanic::Heavy);
    if crate::weapons::archetype_for(r).is_some() {
        let big = r.form == Form::Cannon
            || r.has(Mechanic::Spin)
            || r.has(Mechanic::Explodes)
            || (r.form == Form::Gun && (r.has(Mechanic::Pierce) || r.has(Mechanic::Beam)))
            || heavy
            || rel >= HEAVY_GUN_SIZE;
        return Some(if big { HeavyGun } else { LightGun });
    }
    Some(match r.form {
        Form::Shield => Shield,
        Form::Whip => Flexible,
        Form::Spear | Form::Staff => Reach,
        _ if r.has(Mechanic::Extends) => Reach,
        Form::Gauntlet => {
            if heavy || rel >= HEAVY_FIST_SIZE {
                Heavy
            } else {
                Light
            }
        }
        _ => {
            if heavy || rel >= HEAVY_SIZE || bd_shapes::weight_of(r) >= HEAVY_WEIGHT {
                Heavy
            } else if rel <= LIGHT_SIZE || (r.has(Mechanic::Fast) && r.has(Mechanic::Pierce)) {
                Light
            } else {
                Medium
            }
        }
    })
}

/// The class the SIM applies to a construct: held category only; telescoping poles keep their own
/// moveset (`None`).
#[inline]
pub fn sim_class(c: &Construct) -> Option<WeaponClass> {
    if c.category != FormCategory::Held || c.suit || c.has(Mechanic::Extends) {
        return None;
    }
    class_of(&c.recipe)
}

/// Which curve family a weapon swings with.
pub fn move_set(r: &Recipe) -> MoveSet {
    if r.has(Mechanic::Extends) {
        return MoveSet::Form; // the extend curves (held::curve_of) win anyway
    }
    match class_of(r) {
        Some(WeaponClass::Light) if r.form != Form::Gauntlet => MoveSet::Light,
        Some(WeaponClass::Heavy) => MoveSet::Heavy,
        Some(WeaponClass::Reach) => MoveSet::Reach,
        _ => MoveSet::Form,
    }
}

// ------------------------------------------------------------------------------------ tuning

/// One class's numbers (`assets/tuning/weapons.ron` → `classes`). Neutral = [`ClassNumbers::NEUTRAL`]
/// (the medium class: what every weapon did before classes existed).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClassNumbers {
    /// Swing playback speed (> 1 faster). Applies on top of the curve's authored timing and the
    /// Fast / Heavy pace; the client animates at the same rate.
    pub speed: f32,
    /// Damage / knockback multipliers of every swing hit.
    pub damage: f32,
    pub knockback: f32,
    /// Extra upward share added to the knockback direction (launchers).
    pub lift: f32,
    /// Juice weight of its hits (hit-stop, shake, impact frames): `HitInfo::impact` ×.
    pub impact: f32,
    /// Combo length 1..=3 (0 = the form's own rule: swords and gauntlets 3, others 1).
    pub combo: u8,
    /// The last hit of a full combo: damage × this, and it crits (1 = plain).
    pub finisher_crit: f32,
    /// Jump / dodge / boost during the recovery ends the swing once this fraction of the recovery
    /// has played (0 = at once, ≥ 1 = never).
    pub cancel: f32,
    /// Knockback kept while winding up, charging and swinging (super armor; 1 = none).
    pub armor: f32,
    /// Grounded horizontal speed kept when a swing starts (commitment; 1 = none).
    pub brake: f32,
    /// Forward push at the strike (m/s along the flat aim), only up to `step_cap` forward speed.
    pub step: f32,
    pub step_cap: f32,
    /// While held: run speed and air-strafe acceleration multipliers.
    pub carry_speed: f32,
    pub carry_air: f32,
    /// Sweet spot: hits with the outer `tip` fraction of the weapon deal × `tip_damage` /
    /// × `tip_knockback` and crit (0 = none).
    pub tip: f32,
    pub tip_damage: f32,
    pub tip_knockback: f32,
    /// A strike whose head meets terrain carves a crater this big (m; 0 = the form's own rule).
    pub crater: f32,
    /// Charged-heavy ground break: at `slam_charge`+ charge the strike's shockwave (radius m; 0 =
    /// none), damage and knockback × the form's numbers.
    pub slam_charge: f32,
    pub slam_radius: f32,
    pub slam_damage: f32,
    pub slam_knockback: f32,
    /// Guns: recoil × this while grounded (the world already scales ground recoil down).
    pub ground_recoil: f32,
}

impl ClassNumbers {
    pub const NEUTRAL: ClassNumbers = ClassNumbers {
        speed: 1.0,
        damage: 1.0,
        knockback: 1.0,
        lift: 0.0,
        impact: 1.0,
        combo: 0,
        finisher_crit: 1.0,
        cancel: 1.0,
        armor: 1.0,
        brake: 1.0,
        step: 0.0,
        step_cap: 0.0,
        carry_speed: 1.0,
        carry_air: 1.0,
        tip: 0.0,
        tip_damage: 1.0,
        tip_knockback: 1.0,
        crater: 0.0,
        slam_charge: 1.0,
        slam_radius: 0.0,
        slam_damage: 0.0,
        slam_knockback: 0.0,
        ground_recoil: 1.0,
    };

    fn validated(mut self) -> Self {
        let d = Self::NEUTRAL;
        let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.speed = fin(self.speed, d.speed, 0.4, 2.5);
        self.damage = fin(self.damage, d.damage, 0.0, 4.0);
        self.knockback = fin(self.knockback, d.knockback, 0.0, 4.0);
        self.lift = fin(self.lift, d.lift, 0.0, 2.0);
        self.impact = fin(self.impact, d.impact, 0.1, 4.0);
        self.combo = self.combo.min(3);
        self.finisher_crit = fin(self.finisher_crit, d.finisher_crit, 0.5, 4.0);
        self.cancel = fin(self.cancel, d.cancel, 0.0, 1.0);
        self.armor = fin(self.armor, d.armor, 0.1, 1.0);
        self.brake = fin(self.brake, d.brake, 0.0, 1.0);
        self.step = fin(self.step, d.step, 0.0, 30.0);
        self.step_cap = fin(self.step_cap, d.step_cap, 0.0, 60.0);
        self.carry_speed = fin(self.carry_speed, d.carry_speed, 0.5, 1.3);
        self.carry_air = fin(self.carry_air, d.carry_air, 0.3, 1.5);
        self.tip = fin(self.tip, d.tip, 0.0, 1.0);
        self.tip_damage = fin(self.tip_damage, d.tip_damage, 0.5, 4.0);
        self.tip_knockback = fin(self.tip_knockback, d.tip_knockback, 0.5, 4.0);
        self.crater = fin(self.crater, d.crater, 0.0, 4.0);
        self.slam_charge = fin(self.slam_charge, d.slam_charge, 0.0, 1.0);
        self.slam_radius = fin(self.slam_radius, d.slam_radius, 0.0, 12.0);
        self.slam_damage = fin(self.slam_damage, d.slam_damage, 0.0, 6.0);
        self.slam_knockback = fin(self.slam_knockback, d.slam_knockback, 0.0, 6.0);
        self.ground_recoil = fin(self.ground_recoil, d.ground_recoil, 0.0, 3.0);
        self
    }
}

impl Default for ClassNumbers {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// Every class's numbers (`CombatTuning::weapons`; loaded from `assets/tuning/weapons.ron` →
/// `classes`, hot-reloaded and host-synced with the rest of the combat tuning).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WeaponTuning {
    pub light: ClassNumbers,
    pub medium: ClassNumbers,
    pub heavy: ClassNumbers,
    pub reach: ClassNumbers,
    pub flexible: ClassNumbers,
    pub shield: ClassNumbers,
    pub light_gun: ClassNumbers,
    pub heavy_gun: ClassNumbers,
}

impl Default for WeaponTuning {
    fn default() -> Self {
        let n = ClassNumbers::NEUTRAL;
        Self {
            light: ClassNumbers {
                speed: 1.2,
                damage: 0.75,
                knockback: 0.6,
                impact: 0.6,
                combo: 3,
                finisher_crit: 1.35,
                cancel: 0.0,
                ..n.clone()
            },
            medium: n.clone(),
            heavy: ClassNumbers {
                damage: 1.1,
                knockback: 1.4,
                lift: 0.35,
                impact: 1.6,
                combo: 2,
                armor: 0.55,
                brake: 0.45,
                step: 5.0,
                step_cap: 11.0,
                carry_speed: 0.88,
                carry_air: 0.7,
                crater: 1.3,
                slam_charge: 0.95,
                slam_radius: 4.5,
                slam_damage: 0.8,
                slam_knockback: 1.3,
                ..n.clone()
            },
            reach: ClassNumbers { combo: 3, tip: 0.34, tip_damage: 1.3, tip_knockback: 1.3, ..n.clone() },
            flexible: ClassNumbers { tip: 0.25, tip_damage: 1.5, tip_knockback: 1.2, ..n.clone() },
            shield: ClassNumbers { knockback: 1.15, step: 4.0, step_cap: 10.0, ..n.clone() },
            light_gun: n.clone(),
            heavy_gun: ClassNumbers { carry_speed: 0.9, carry_air: 0.8, ground_recoil: 1.8, ..n },
        }
    }
}

impl WeaponTuning {
    /// Every class neutral (classes change nothing but the curves): A/B baseline for tests.
    pub fn neutral() -> Self {
        let n = ClassNumbers::NEUTRAL;
        Self {
            light: n.clone(),
            medium: n.clone(),
            heavy: n.clone(),
            reach: n.clone(),
            flexible: n.clone(),
            shield: n.clone(),
            light_gun: n.clone(),
            heavy_gun: n,
        }
    }

    pub fn of(&self, class: WeaponClass) -> &ClassNumbers {
        match class {
            WeaponClass::Light => &self.light,
            WeaponClass::Medium => &self.medium,
            WeaponClass::Heavy => &self.heavy,
            WeaponClass::Reach => &self.reach,
            WeaponClass::Flexible => &self.flexible,
            WeaponClass::Shield => &self.shield,
            WeaponClass::LightGun => &self.light_gun,
            WeaponClass::HeavyGun => &self.heavy_gun,
        }
    }

    pub fn validated(self) -> Self {
        Self {
            light: self.light.validated(),
            medium: self.medium.validated(),
            heavy: self.heavy.validated(),
            reach: self.reach.validated(),
            flexible: self.flexible.validated(),
            shield: self.shield.validated(),
            light_gun: self.light_gun.validated(),
            heavy_gun: self.heavy_gun.validated(),
        }
    }

    /// Parse the `classes` section of `assets/tuning/weapons.ron` (the file's other sections —
    /// the presentation's `feel` — are ignored here).
    pub fn from_weapons_ron(src: &str) -> Result<Self, String> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct File {
            classes: WeaponTuning,
        }
        ron::from_str::<File>(src).map(|f| f.classes.validated()).map_err(|e| e.to_string())
    }
}

/// The numbers the sim applies to `c` (neutral for poles, non-held and suits).
#[inline]
pub fn numbers<'a>(c: &Construct, t: &'a WeaponTuning) -> &'a ClassNumbers {
    static NEUTRAL: ClassNumbers = ClassNumbers::NEUTRAL;
    match sim_class(c) {
        Some(k) => t.of(k),
        None => &NEUTRAL,
    }
}

// ------------------------------------------------------------------------------------ curves

const SETS: usize = 3;
static CURVES: [OnceLock<Vec<Option<SwingCurve>>>; WEIGHT_BINS] = [const { OnceLock::new() }; WEIGHT_BINS];

/// The class curve `c` swings along for `swing` (its recipe's weight bin), or `None` when its
/// class keeps the form's own curve for that swing. Built once per bin, then shared.
pub fn curve(c: &Construct, swing: Swing) -> Option<&'static SwingCurve> {
    curve_for(move_set(&c.recipe), swing, bd_shapes::weight_of(&c.recipe))
}

/// [`curve`] by set / swing / weight (clients use the construct form).
pub fn curve_for(set: MoveSet, swing: Swing, weight: f32) -> Option<&'static SwingCurve> {
    let si = MoveSet::AUTHORED.iter().position(|s| *s == set)?;
    let bin = held::weight_bin(weight);
    let table = CURVES[bin].get_or_init(|| {
        let w = held::bin_weight(bin);
        let mut v = Vec::with_capacity(SETS * Swing::ALL.len());
        for s in MoveSet::AUTHORED {
            for k in Swing::ALL {
                v.push(bd_shapes::swing_curve_set_weighted(s, Form::Sword, k.kind(), w));
            }
        }
        v
    });
    table.get(si * Swing::ALL.len() + swing as usize).and_then(|c| c.as_ref())
}

// ------------------------------------------------------------------------------------ combos / timing

/// How many swings `c`'s LMB string has (1 = no combo).
pub fn combo_len(c: &Construct, t: &WeaponTuning) -> u8 {
    if c.has(Mechanic::Extends) || super::legend::combos(c) {
        return 3;
    }
    match numbers(c, t).combo {
        0 => {
            if matches!(c.recipe.form, Form::Sword | Form::Gauntlet) {
                3
            } else {
                1
            }
        }
        n => n.clamp(1, 3),
    }
}

/// The next combo swing after `prev` (the last combo index played) in a string of `len`.
pub fn next_combo(prev: u8, len: u8) -> Swing {
    match (prev + 1) % len.max(1) {
        1 => Swing::Combo1,
        2 => Swing::Combo2,
        _ => Swing::Combo0,
    }
}

/// Class playback speed of `c`'s swings (1 = authored).
#[inline]
pub fn speed(c: &Construct, t: &WeaponTuning) -> f32 {
    numbers(c, t).speed
}

/// Which swing a `HeldFx` event started (clients: the exact curve the sim sweeps). `None` = the
/// event starts no swing.
pub fn event_swing(c: &Construct, fx: HeldFx, strength: f32, t: &WeaponTuning) -> Option<(Swing, bool)> {
    Some(match fx {
        HeldFx::SwingStart if strength >= 1.9 => (Swing::Heavy, false),
        HeldFx::SwingStart => {
            if combo_len(c, t) >= 2 {
                let k = ((strength - 1.0) / 0.25).round().clamp(0.0, 2.0) as u8;
                (
                    match k {
                        1 => Swing::Combo1,
                        2 => Swing::Combo2,
                        _ => Swing::Combo0,
                    },
                    false,
                )
            } else {
                (Swing::Primary, false)
            }
        }
        HeldFx::ChargeStart => (Swing::Heavy, true),
        HeldFx::GuardUp | HeldFx::Parry => (Swing::Secondary, true),
        HeldFx::Uppercut | HeldFx::Lunge | HeldFx::GroundPound | HeldFx::DiveStart => (Swing::Secondary, false),
        HeldFx::Throw => (Swing::Throw, false),
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------ sim hooks (held.rs)

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z).normalize_or_zero()
}

/// A swing (or a charge's coil) starts: heavy weapons brake the wielder's ground speed.
pub(super) fn on_swing_start(c: &Construct, swing: Swing, env: &Env<'_>, out: &mut ConstructOut) {
    if matches!(swing, Swing::Secondary | Swing::Throw) {
        return;
    }
    let n = numbers(c, &env.ctx.tuning.weapons);
    let o = &env.owner;
    if n.brake < 1.0 && o.grounded {
        let h = Vec3::new(o.vel.x, 0.0, o.vel.z);
        if h.length_squared() > 1e-4 {
            out.impulses.push((c.owner, -h * (1.0 - n.brake), 0.0));
        }
    }
}

/// The hitbox opens: the strike carries the wielder forward (heavy momentum, shield bash).
pub(super) fn on_active(c: &Construct, s: &HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    if matches!(s.swing, Swing::Throw) || (s.swing == Swing::Secondary && c.recipe.form != Form::Shield) {
        return;
    }
    let n = numbers(c, &env.ctx.tuning.weapons);
    if n.step <= 0.0 {
        return;
    }
    let o = &env.owner;
    let fwd = flat(o.look);
    let along = o.vel.dot(fwd);
    let dv = n.step.min((n.step_cap - along).max(0.0));
    if dv > 0.05 {
        out.impulses.push((c.owner, fwd * dv, 0.0));
    }
}

/// Jump / dodge / boost in the recovery cancels the rest of a class-cancelable swing.
pub(super) fn move_cancel(c: &Construct, s: &HeldState, env: &Env<'_>, t1: f32, strike_end: f32, recovery: f32) -> bool {
    use bd_core::input::buttons;
    if !matches!(s.swing, Swing::Primary | Swing::Combo0 | Swing::Combo1 | Swing::Combo2 | Swing::Heavy) {
        return false;
    }
    let n = numbers(c, &env.ctx.tuning.weapons);
    n.cancel < 1.0
        && t1 >= strike_end + recovery * n.cancel
        && env.owner.pressed & (buttons::JUMP | buttons::DODGE | buttons::BOOST) != 0
}

/// Class multipliers on the swing's base numbers (`held::hit_spec`).
pub(super) fn hit_spec(c: &Construct, s: &HeldState, t: &WeaponTuning, dmg: &mut f32, kb: &mut f32, impact: &mut f32) {
    let n = numbers(c, t);
    *dmg *= n.damage;
    *kb *= n.knockback;
    *impact *= n.impact;
    // The last hit of a full string.
    if n.finisher_crit != 1.0
        && let Some(k) = s.swing.combo_index()
        && k + 1 == combo_len(c, t)
        && k >= 2
    {
        *dmg *= n.finisher_crit;
        *impact *= 1.2;
    }
}

/// Per-hit class tweaks: launch lift, the sweet spot (`frac` = where along the weapon it
/// connected, 0 grip … 1 tip), the finisher crit. Returns whether the hit crits.
pub(super) fn hit_player(c: &Construct, s: &HeldState, t: &WeaponTuning, frac: f32, dmg: &mut f32, kb: &mut f32, dir: &mut Vec3, impact: &mut f32) -> bool {
    let n = numbers(c, t);
    let mut crit = false;
    if n.lift > 0.0 {
        *dir = dir.normalize_or_zero() + Vec3::Y * n.lift;
    }
    if n.tip > 0.0 && frac >= 1.0 - n.tip - 1e-4 && !matches!(s.swing, Swing::Secondary | Swing::Throw) {
        *dmg *= n.tip_damage;
        *kb *= n.tip_knockback;
        *impact *= 1.25;
        crit = true;
    }
    if n.finisher_crit != 1.0
        && let Some(k) = s.swing.combo_index()
        && k + 1 == combo_len(c, t)
        && k >= 2
    {
        crit = true;
    }
    crit
}

/// Terrain bite of a strike swing (m; 0 = the form's own rule).
pub(super) fn crater(c: &Construct, s: &HeldState, t: &WeaponTuning) -> f32 {
    if matches!(s.swing, Swing::Secondary | Swing::Throw) {
        return 0.0;
    }
    numbers(c, t).crater
}

/// The swing's hitbox window closed: a fully charged heavy breaks the ground under its head.
pub(super) fn on_window_end(c: &mut Construct, s: &HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    if s.swing != Swing::Heavy {
        return;
    }
    let ctx = env.ctx;
    let n = numbers(c, &ctx.tuning.weapons);
    if n.slam_radius <= 0.0 || s.heavy_k + 1e-4 < n.slam_charge {
        return;
    }
    let o = &env.owner;
    // Where the head came down: under the tip (or in front of the wielder if the tip is unknown).
    let tip = if s.blade1 != Vec3::ZERO { s.blade1 } else { o.hand + flat(o.look) * 1.5 };
    let from = tip + Vec3::Y * 0.6;
    let Some(h) = bd_core::voxel::raycast(ctx.voxels, from, Vec3::NEG_Y, 4.0) else { return };
    let at = h.point;
    let spec = ctx.tuning.form_spec(c.recipe.form); // TUNING HOOK
    let (dm, km) = held::mech_mult(c);
    let r = n.slam_radius;
    for t in ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(t.id)) {
            continue; // struck by the blade itself
        }
        let d = geom::dist_sq_point_aabb(&t.hull, at).sqrt();
        if d > r || (t.pos.y - at.y).abs() > 3.0 {
            continue;
        }
        let fall = 1.0 - 0.5 * (d / r);
        let dir = flat(t.pos - at) + Vec3::Y * 1.1;
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::HeavyMelee, spec.damage * n.slam_damage * dm * fall, spec.knockback * n.slam_knockback * km * fall, dir, t.hull.center())
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(2.0 * n.impact.max(1.0) * fall),
        );
        c.hits.insert(player_key(t.id));
    }
    let crater = 1.6 * if c.recipe.element == Element::Earth { 1.26 } else { 1.0 };
    out.carves.push(CarveRequest { center: at, radius: crater, power: 1.0 });
    held::fx(out, c, HeldFx::DiveSlam, at, Vec3::NEG_Y, 2.0 + r / 4.5);
}

// ------------------------------------------------------------------------------------ world hooks

/// Super armor: knockback multiplier for `target` right now (1 = none). `World::apply_hit` takes the
/// stronger of this and the legendary heavy-blade armor.
pub fn armor(store: &ConstructStore, target: PlayerId, t: &WeaponTuning) -> f32 {
    for c in store.owned_by(target) {
        if !c.active() || c.held.thrown {
            continue;
        }
        let n = numbers(c, t);
        if n.armor >= 1.0 {
            continue;
        }
        let s = &c.held;
        let live = match s.mode {
            Mode::Charge => true,
            Mode::Swing if !matches!(s.swing, Swing::Secondary | Swing::Throw) => {
                held::curve_of(c, s.swing).is_some_and(|cv| s.t < cv.active_window().1)
            }
            _ => false,
        };
        if live {
            return n.armor;
        }
    }
    1.0
}

/// Carrying: heavy weapons slow your run and your air-strafing. Called by the host's movement step
/// and client prediction (`World::move_mods_for`) right after the construct modifiers, so both agree.
pub fn carry_mods(store: &ConstructStore, t: &WeaponTuning, owner: PlayerId, mods: &mut crate::movement::MoveModifiers) {
    let Some(c) = store.held_by(owner) else { return };
    if !c.active() || !c.attached() || c.suit {
        return;
    }
    let Some(k) = class_of(&c.recipe) else { return };
    let n = t.of(k);
    mods.speed_mult *= n.carry_speed;
    mods.air_control *= n.carry_air;
}

/// Gun recoil multiplier (`gun.rs`): heavy guns shove you on the ground too.
pub fn ground_recoil(c: &Construct, grounded: bool, t: &WeaponTuning) -> f32 {
    if !grounded {
        return 1.0;
    }
    match class_of(&c.recipe) {
        Some(k) if k.is_gun() => t.of(k).ground_recoil,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::recipe;
    use Mechanic::*;

    /// Class of a recipe with a realistic mass (kg): weight_of reads mass against size.
    fn cls(name: &str, form: Form, m: &[Mechanic], scale: f32, mass: f32) -> Option<WeaponClass> {
        let mut r = recipe(name, form, m, Element::Light, scale);
        r.mass = mass;
        class_of(&r)
    }

    #[test]
    fn classes_come_from_data_not_names() {
        use WeaponClass::*;
        // Same data, different names: same class.
        assert_eq!(cls("anything", Form::Sword, &[Blade], 1.1, 3.0), cls("zzz", Form::Sword, &[Blade], 1.1, 3.0));
        assert_eq!(cls("sword", Form::Sword, &[Blade], 1.1, 3.0), Some(Medium));
        assert_eq!(cls("katana", Form::Sword, &[Blade, Fast], 1.1, 3.0), Some(Medium));
        assert_eq!(cls("dagger", Form::Sword, &[Blade, Pierce], 0.55, 0.8), Some(Light));
        assert_eq!(cls("rapier", Form::Sword, &[Blade, Pierce, Fast], 1.0, 1.5), Some(Light));
        assert_eq!(cls("fists", Form::Gauntlet, &[Blunt], 0.4, 3.0), Some(Light));
        assert_eq!(cls("greatsword", Form::Sword, &[Blade, Mechanic::Heavy], 1.8, 12.0), Some(Heavy));
        assert_eq!(cls("giant sword", Form::Sword, &[Blade], 3.0, 46.0), Some(Heavy));
        assert_eq!(cls("warhammer", Form::Hammer, &[Blunt, Mechanic::Heavy], 1.2, 6.0), Some(Heavy));
        assert_eq!(cls("great axe", Form::Axe, &[Blade], 1.4, 10.0), Some(Heavy));
        assert_eq!(cls("hammer", Form::Hammer, &[Blunt], 1.2, 6.0), Some(Medium));
        // Mass reads against size: a small thing that weighs a ton is heavy.
        assert_eq!(cls("lead dagger", Form::Sword, &[Blade, Pierce], 0.55, 40.0), Some(Heavy));
        assert_eq!(cls("spear", Form::Spear, &[Pierce], 2.2, 3.0), Some(Reach));
        assert_eq!(cls("bo staff", Form::Staff, &[Blunt], 1.8, 2.0), Some(Reach));
        assert_eq!(cls("whip", Form::Whip, &[Pull], 3.0, 1.0), Some(Flexible));
        assert_eq!(cls("shield", Form::Shield, &[Guard], 1.0, 6.0), Some(Shield));
        assert_eq!(cls("pistol", Form::Gun, &[Shoots], 0.8, 4.0), Some(LightGun));
        assert_eq!(cls("smg", Form::Gun, &[Shoots, Fast], 0.8, 4.0), Some(LightGun));
        assert_eq!(cls("minigun", Form::Gun, &[Shoots, Spin, Fast], 0.8, 4.0), Some(HeavyGun));
        assert_eq!(cls("lmg", Form::Gun, &[Shoots, Fast, Mechanic::Heavy], 1.0, 8.0), Some(HeavyGun));
        assert_eq!(cls("rocket launcher", Form::Cannon, &[Shoots, Explodes], 1.8, 80.0), Some(HeavyGun));
        assert_eq!(cls("railgun", Form::Gun, &[Shoots, Pierce, Beam], 0.8, 4.0), Some(HeavyGun));
        assert_eq!(cls("fire staff", Form::Staff, &[Shoots], 1.6, 2.0), Some(LightGun));
        assert_eq!(cls("wolf", Form::Quadruped, &[], 1.2, 30.0), None);
        let mut ruyi = recipe("ruyi", Form::Spear, &[Blunt, Extends], Element::Light, 2.4);
        assert_eq!(class_of(&ruyi), Some(Reach));
        ruyi.form = Form::Staff;
        assert_eq!(class_of(&ruyi), Some(Reach));
        // A blunt staff is a pole (melee), an elemental / shooting one still casts.
        assert!(staff_is_pole(&recipe("bo staff", Form::Staff, &[Blunt], Element::Light, 1.8)));
        assert!(!staff_is_pole(&recipe("fire staff", Form::Staff, &[Blunt], Element::Fire, 1.8)));
        assert!(!staff_is_pole(&recipe("wand", Form::Staff, &[Shoots], Element::Psychic, 0.4)));
        assert!(crate::weapons::archetype_for(&recipe("bo staff", Form::Staff, &[Blunt], Element::Light, 1.8)).is_none());
    }

    #[test]
    fn tuning_parses_and_neutral_medium() {
        let t = WeaponTuning::default();
        assert_eq!(t.medium, ClassNumbers::NEUTRAL, "medium weapons are exactly what they were");
        let src = include_str!("../../../../assets/tuning/weapons.ron");
        let parsed = WeaponTuning::from_weapons_ron(src).expect("weapons.ron parses");
        assert_eq!(parsed, t, "assets/tuning/weapons.ron `classes` == the built-in defaults");
        let bad = WeaponTuning { heavy: ClassNumbers { speed: f32::NAN, armor: -3.0, ..ClassNumbers::NEUTRAL }, ..WeaponTuning::default() }.validated();
        assert_eq!(bad.heavy.speed, 1.0);
        assert_eq!(bad.heavy.armor, 0.1);
    }

    #[test]
    fn combo_steps_wrap() {
        assert_eq!(next_combo(0, 3), Swing::Combo1);
        assert_eq!(next_combo(1, 3), Swing::Combo2);
        assert_eq!(next_combo(2, 3), Swing::Combo0);
        assert_eq!(next_combo(0, 2), Swing::Combo1);
        assert_eq!(next_combo(1, 2), Swing::Combo0);
    }

    #[test]
    fn class_curves_are_cached_and_shared() {
        let a = curve_for(MoveSet::Heavy, Swing::Combo0, 1.8).map(|c| c as *const SwingCurve);
        let b = curve_for(MoveSet::Heavy, Swing::Combo0, 1.8).map(|c| c as *const SwingCurve);
        assert!(a.is_some());
        assert_eq!(a, b);
        assert!(curve_for(MoveSet::Form, Swing::Combo0, 1.0).is_none());
        assert!(curve_for(MoveSet::Heavy, Swing::Throw, 1.0).is_none(), "throws keep the form curve");
        let heavy = curve_for(MoveSet::Heavy, Swing::Combo0, 1.8).expect("heavy");
        let light = curve_for(MoveSet::Light, Swing::Combo0, 0.7).expect("light");
        assert!(heavy.anticipation > 3.0 * light.anticipation, "{} vs {}", heavy.anticipation, light.anticipation);
    }
}
