//! Guns (COMBAT.md §4b — DOOM feel, never pea shooters): archetypes, their numbers, and a generic
//! [`WeaponState`] machine (spin-up, reload-snap, charge, dual-wield alternation) that the
//! construct engineer's Gun component drives. The world owns what a shot *does* (projectiles with
//! 100 % velocity inheritance, recoil impulses, lag-compensated hitscan, pellets with falloff,
//! momentum bonus from closing speed); this module only decides *when* and *what* is fired.
//!
//! Every archetype is data ([`gun_spec`]); caliber scales with the recipe's `scale` (a giant gun is
//! slower, heavier, kicks harder); `count = 2` dual-wields (barrels alternate, cadence halves).

use bd_core::mechanics::{Form, Mechanic, Recipe};
use bd_core::Vec3;
use serde::{Deserialize, Serialize};

use crate::constructs::form_spec;
use crate::projectile::ProjectileKind;

/// What kind of gun a held construct is (drives numbers, feel and client juice).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum GunArchetype {
    /// Empty-hand ring blast.
    #[default]
    Ring = 0,
    /// gun / pistol / revolver / rifle.
    HandCannon = 1,
    /// shotgun / blunderbuss (Gun + Shoots + count ≥ 3).
    SuperShotgun = 2,
    /// minigun / gatling (Gun + Spin).
    Minigun = 3,
    /// rocket launcher / bazooka (Gun or Cannon + Explodes).
    Rocket = 4,
    /// railgun / sniper / laser rifle (Gun + Pierce or Beam): hitscan.
    Railgun = 5,
    Bow = 6,
    Cannon = 7,
    Staff = 8,
    Disc = 9,
    /// Energy from the palms (`Form::Hands`: kamehameha, final flash): hold to charge, fires on
    /// RELEASE (charge scales it) or by itself at full charge; a hitscan beam like the railgun.
    HandBeam = 10,
}

impl GunArchetype {
    pub fn from_u8(v: u8) -> Self {
        use GunArchetype::*;
        match v {
            1 => HandCannon,
            2 => SuperShotgun,
            3 => Minigun,
            4 => Rocket,
            5 => Railgun,
            6 => Bow,
            7 => Cannon,
            8 => Staff,
            9 => Disc,
            10 => HandBeam,
            _ => Ring,
        }
    }
}

/// Per-archetype numbers (before caliber scaling).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GunSpec {
    /// Seconds between shots (per barrel set).
    pub cadence: f32,
    /// Damage per projectile / pellet / beam.
    pub damage: f32,
    /// Knockback per projectile / pellet.
    pub knockback: f32,
    /// Muzzle speed (m/s); 0 = hitscan-only (railgun). Hitscan guns keep a speed for the rounds
    /// that still fly (payload / explosive / ricochet / portal variants).
    pub speed: f32,
    /// Lag-compensated hitscan (hand cannon, super shotgun per pellet, minigun, railgun): players
    /// move 20–50 m/s, so slow slugs read as unfair. Behaviour, not a number: never tuned.
    pub hitscan: bool,
    /// Recoil impulse on the shooter per shot (m/s, opposite the aim; ×0.35 on the ground).
    pub recoil: f32,
    pub pellets: u8,
    /// Pellet cone half-angle (degrees).
    pub spread_deg: f32,
    /// Damage falls off beyond this distance (m); 0 = never.
    pub falloff_start: f32,
    pub spin_up: f32,
    pub spin_down: f32,
    /// Shots per reload (0 = no reload).
    pub clip: u8,
    pub reload: f32,
    /// Hold to charge this long before the shot (railgun, bow draw).
    pub charge: f32,
    /// Charge guns that fire by themselves when full (railgun): letting go early still fires
    /// (owner: "never require another click"), damage / knockback / recoil scaled from this fraction
    /// at a tap up to 1 at full charge. 0 = an early release fires nothing.
    pub release_min: f32,
    /// Explodes on impact: (blast radius, crater radius).
    pub explode: Option<(f32, f32)>,
    /// Hitscan: players it passes through; also pierces walls up to 1 m thick.
    pub pierce: u8,
    /// Terrain chip radius on impact (0 = none).
    pub chip: f32,
    pub kind: ProjectileKind,
    pub radius: f32,
}

const BASE: GunSpec = GunSpec {
    cadence: 0.35,
    damage: 16.0,
    knockback: 11.0,
    speed: 140.0,
    hitscan: false,
    recoil: 2.5,
    pellets: 1,
    spread_deg: 0.0,
    falloff_start: 0.0,
    spin_up: 0.0,
    spin_down: 0.0,
    clip: 0,
    reload: 0.0,
    charge: 0.0,
    release_min: 0.0,
    explode: None,
    pierce: 0,
    chip: 0.0,
    kind: ProjectileKind::Bolt,
    radius: 0.2,
};

/// The table (COMBAT.md §4b).
pub fn gun_spec(a: GunArchetype) -> GunSpec {
    use GunArchetype::*;
    match a {
        Ring => GunSpec { cadence: crate::combat::BLAST_COOLDOWN, damage: crate::combat::BLAST_DAMAGE, knockback: crate::combat::BLAST_KNOCKBACK, speed: crate::combat::BLAST_SPEED, recoil: 0.8, radius: crate::combat::BLAST_RADIUS, kind: ProjectileKind::RingBlast, ..BASE },
        // Hitscan balance pass: 16 → 13 (docs/BALANCE_REPORT.md).
        HandCannon => GunSpec { damage: 13.0, chip: 0.35, hitscan: true, ..BASE },
        SuperShotgun => GunSpec {
            cadence: 0.25,
            damage: 8.0,
            knockback: 2.0,
            speed: 110.0,
            hitscan: true,
            recoil: 9.0,
            pellets: 9,
            spread_deg: 4.0,
            falloff_start: 12.0,
            clip: 2,
            reload: 0.9,
            radius: 0.12,
            ..BASE
        },
        Minigun => GunSpec { cadence: 1.0 / 14.0, damage: 4.5, knockback: 3.0, speed: 120.0, hitscan: true, recoil: 1.2 / 14.0, spin_up: 0.4, spin_down: 0.3, radius: 0.12, ..BASE },
        Rocket => GunSpec {
            cadence: 0.8,
            damage: 30.0,
            knockback: 22.0,
            speed: 45.0,
            recoil: 4.0,
            explode: Some((3.5, 3.5 * 0.55)),
            kind: ProjectileKind::CannonBall,
            radius: 0.3,
            ..BASE
        },
        Railgun => GunSpec { cadence: 1.1, damage: 40.0, knockback: 20.0, speed: 0.0, hitscan: true, recoil: 6.0, charge: 0.5, release_min: 0.35, pierce: 2, ..BASE },
        Bow => GunSpec { cadence: 0.3, damage: 30.0, knockback: 14.0, speed: 90.0, recoil: 0.5, charge: 1.0, kind: ProjectileKind::Arrow, ..BASE },
        Cannon => GunSpec {
            cadence: 1.0,
            damage: 28.0,
            knockback: 24.0,
            speed: 45.0,
            recoil: 9.0,
            explode: Some((2.5, 1.2)),
            kind: ProjectileKind::CannonBall,
            radius: 0.35,
            ..BASE
        },
        Staff => GunSpec { cadence: 0.45, damage: 8.0, knockback: 8.0, speed: 70.0, recoil: 0.5, ..BASE },
        Disc => GunSpec { cadence: 0.8, damage: 8.0, knockback: 8.0, speed: 40.0, recoil: 0.5, kind: ProjectileKind::Disc, ..BASE },
        HandBeam => GunSpec { cadence: 1.1, damage: 40.0, knockback: 20.0, speed: 0.0, hitscan: true, recoil: 2.0, charge: 0.6, pierce: 2, chip: 0.8, ..BASE },
    }
}

/// Which archetype a held recipe is, if it shoots at all.
pub fn archetype_for(r: &Recipe) -> Option<GunArchetype> {
    use GunArchetype::*;
    match r.form {
        Form::Gun => Some(if r.has(Mechanic::Spin) {
            Minigun
        } else if r.has(Mechanic::Explodes) {
            Rocket
        } else if r.has(Mechanic::Pierce) || r.has(Mechanic::Beam) {
            Railgun
        } else if r.count >= 3 {
            SuperShotgun
        } else {
            HandCannon
        }),
        Form::Cannon => Some(if r.has(Mechanic::Explodes) { Rocket } else { Cannon }),
        Form::Bow => Some(Bow),
        Form::Staff => Some(Staff),
        Form::Disc => Some(Disc),
        Form::Hands => Some(HandBeam),
        _ => None,
    }
}

/// Spec for a recipe: archetype numbers scaled by caliber (`scale` vs the form default) and
/// dual-wield. Giant guns: slower cadence, more damage and knockback, more recoil, slower rounds.
pub fn spec_for(r: &Recipe, a: GunArchetype) -> GunSpec {
    scale_spec(gun_spec(a), r, a)
}

/// Caliber / dual-wield / trait scaling of an archetype spec (TUNING HOOK: shared by
/// [`spec_for`] and `tuning::CombatTuning::spec_for`, which starts from the tuned row).
pub fn scale_spec(spec: GunSpec, r: &Recipe, a: GunArchetype) -> GunSpec {
    let mut s = spec;
    let d = form_spec(r.form).default_scale.max(0.1);
    let k = (r.scale / d).sqrt().clamp(0.6, 2.5);
    s.damage *= k;
    s.knockback *= k;
    s.recoil *= k;
    s.cadence *= k;
    s.radius *= k;
    if s.speed > 0.0 {
        s.speed /= k.sqrt();
    }
    if let Some((br, cr)) = s.explode {
        s.explode = Some((br * k, cr * k));
    }
    if r.count == 2 && a != GunArchetype::SuperShotgun {
        s.cadence *= 0.5;
    }
    if r.has(Mechanic::Fast) {
        s.speed *= 1.4;
        s.damage *= 0.8;
    }
    if r.has(Mechanic::Heavy) {
        s.damage *= 1.5;
        s.knockback *= 1.5;
        s.recoil *= 1.3;
    }
    s
}

/// Weapon state machine phase (replicated so viewmodels spin / snap / glow in sync).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum WeaponPhase {
    #[default]
    Idle = 0,
    SpinUp = 1,
    Firing = 2,
    SpinDown = 3,
    Reloading = 4,
    Charging = 5,
}

impl WeaponPhase {
    pub fn from_u8(v: u8) -> Self {
        use WeaponPhase::*;
        match v {
            1 => SpinUp,
            2 => Firing,
            3 => SpinDown,
            4 => Reloading,
            5 => Charging,
            _ => Idle,
        }
    }
}

/// A shot the machine wants fired this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireCmd {
    /// Charge fraction for charged weapons (bow draw), 1 otherwise.
    pub charge: f32,
    /// Which barrel (dual wield alternates).
    pub alt: bool,
}

/// Generic gun state. The Gun component owns one per gun construct and steps it every tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WeaponState {
    pub arch: GunArchetype,
    pub phase: WeaponPhase,
    /// Seconds in the current phase.
    pub t: f32,
    pub cooldown: f32,
    /// Minigun barrel spin 0..1.
    pub spin: f32,
    /// Charge 0..1 (railgun / bow).
    pub charge: f32,
    /// Rounds left before a reload (clip weapons).
    pub clip: u8,
    /// Next barrel (dual wield).
    pub alt: bool,
}

impl WeaponState {
    pub fn new(arch: GunArchetype, spec: &GunSpec) -> Self {
        Self { arch, clip: spec.clip, ..Default::default() }
    }

    fn set(&mut self, p: WeaponPhase) {
        if self.phase != p {
            self.phase = p;
            self.t = 0.0;
        }
    }

    /// Replicated byte: phase (3 bits) + level (spin / charge / reload progress, 5 bits).
    pub fn wire(&self, spec: &GunSpec) -> u8 {
        let level = match self.phase {
            WeaponPhase::SpinUp | WeaponPhase::Firing | WeaponPhase::SpinDown => self.spin,
            WeaponPhase::Charging => self.charge,
            WeaponPhase::Reloading => (self.t / spec.reload.max(1e-3)).min(1.0),
            WeaponPhase::Idle => 0.0,
        };
        (self.phase as u8 & 7) | (((level.clamp(0.0, 1.0) * 31.0).round() as u8) << 3)
    }

    /// Advance one tick with the trigger state; returns a shot to fire.
    pub fn step(&mut self, spec: &GunSpec, held: bool, released: bool, dt: f32) -> Option<FireCmd> {
        self.t += dt;
        self.cooldown = (self.cooldown - dt).max(0.0);
        // Reload snap.
        if self.phase == WeaponPhase::Reloading {
            if self.t >= spec.reload {
                self.clip = spec.clip;
                self.set(WeaponPhase::Idle);
            } else {
                return None;
            }
        }
        let mut fire = None;
        if spec.spin_up > 0.0 {
            // Minigun: spin up while held, fire at cadence once spun, spin down when released.
            if held {
                self.spin = (self.spin + dt / spec.spin_up).min(1.0);
                if self.spin >= 1.0 {
                    self.set(WeaponPhase::Firing);
                    if self.cooldown <= 0.0 {
                        fire = Some(FireCmd { charge: 1.0, alt: self.alt });
                    }
                } else {
                    self.set(WeaponPhase::SpinUp);
                }
            } else if self.spin > 0.0 {
                self.spin = (self.spin - dt / spec.spin_down.max(1e-3)).max(0.0);
                self.set(if self.spin > 0.0 { WeaponPhase::SpinDown } else { WeaponPhase::Idle });
            }
        } else if spec.charge > 0.0 {
            // Railgun: hold charges, fires by itself when full. Bow: fires on release (draw scales).
            // Hand beam: both — release fires (charge scales it), a full charge fires by itself.
            // Railgun (`release_min` > 0): an early release fires a weaker shot too (any charge).
            let bow = self.arch == GunArchetype::Bow;
            let early = self.arch == GunArchetype::Railgun && spec.release_min > 0.0;
            let on_release = bow || self.arch == GunArchetype::HandBeam || early;
            let min_release = if bow { 0.05 } else if early { 0.0 } else { 0.2 };
            if held && self.cooldown <= 0.0 {
                self.charge = (self.charge + dt / spec.charge).min(1.0);
                self.set(WeaponPhase::Charging);
                if !bow && self.charge >= 1.0 {
                    fire = Some(FireCmd { charge: 1.0, alt: self.alt });
                }
            } else if on_release && released && self.charge > min_release {
                fire = Some(FireCmd { charge: self.charge, alt: self.alt });
            } else if !held {
                self.charge = 0.0;
                if self.phase == WeaponPhase::Charging {
                    self.set(WeaponPhase::Idle);
                }
            }
        } else if held && self.cooldown <= 0.0 {
            fire = Some(FireCmd { charge: 1.0, alt: self.alt });
        } else if self.phase == WeaponPhase::Firing && self.cooldown <= 0.0 {
            self.set(WeaponPhase::Idle);
        }
        if let Some(f) = fire {
            self.cooldown = spec.cadence;
            self.alt = !self.alt;
            self.charge = 0.0;
            if spec.spin_up <= 0.0 {
                self.set(WeaponPhase::Firing);
            }
            if spec.clip > 0 {
                self.clip = self.clip.saturating_sub(1);
                if self.clip == 0 {
                    self.set(WeaponPhase::Reloading);
                }
            }
            return Some(f);
        }
        None
    }
}

/// Deterministic pellet directions: a fixed, learnable pattern (centre + rings), never random.
pub fn pellet_dirs(aim: Vec3, pellets: u8, spread_deg: f32, out: &mut [Vec3; 16]) -> usize {
    let n = (pellets as usize).clamp(1, out.len());
    let aim = aim.normalize_or_zero();
    if n == 1 || spread_deg <= 0.0 {
        out[0] = aim;
        return 1;
    }
    let (u, v) = aim.any_orthonormal_pair();
    out[0] = aim;
    let ring = n - 1;
    for k in 0..ring {
        // Inner ring for the first half, outer ring for the rest.
        let outer = k >= ring / 2;
        let ang = spread_deg.to_radians() * if outer { 1.0 } else { 0.5 };
        let phi = k as f32 / ring as f32 * std::f32::consts::TAU + if outer { 0.4 } else { 0.0 };
        let (sa, ca) = ang.sin_cos();
        let (sp, cp) = phi.sin_cos();
        out[k + 1] = (aim * ca + (u * cp + v * sp) * sa).normalize_or_zero();
    }
    n
}

/// A lag-compensated hitscan ray: the railgun beam, and every hand cannon / minigun round and
/// super-shotgun pellet (COMBAT.md §4b). Resolved by `World::hitscan` against other players
/// rewound to the shooter's view tick.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitscanShot {
    pub owner: bd_core::PlayerId,
    pub construct: Option<bd_core::ConstructId>,
    /// Ray origin (the shooter's eye: what the crosshair saw).
    pub origin: Vec3,
    pub dir: Vec3,
    pub range: f32,
    pub damage: f32,
    pub knockback: f32,
    pub element: bd_core::mechanics::Element,
    /// Players it passes through (hits up to `pierce + 1`).
    pub pierce: u8,
    /// Solid terrain (m) it punches through.
    pub wall: f32,
    /// Where the visible tracer starts (the gun's muzzle); `origin` if zero.
    #[serde(default)]
    pub muzzle: Vec3,
    /// Damage / knockback fall off beyond this distance (to 30 % at +18 m, like pellets); 0 = never.
    #[serde(default)]
    pub falloff_start: f32,
    /// Terrain chip radius where the ray ends on terrain (0 = none).
    #[serde(default)]
    pub chip: f32,
    /// `projectile::effects` bits the round carries (Pull yanks; Shrink/Grow/Link ride the construct).
    #[serde(default)]
    pub effects: u8,
    /// `Some(archetype)`: a gun round — the client draws a chunky voxel tracer (`GameEvent::ShotTrace`).
    /// `None`: a beam (railgun / staff) — `GameEvent::Hitscan`.
    #[serde(default)]
    pub trace: Option<GunArchetype>,
}

impl HitscanShot {
    /// A plain single-target beam from `origin` (no pierce, no wall punch, no tracer).
    pub fn new(owner: bd_core::PlayerId, origin: Vec3, dir: Vec3, damage: f32, knockback: f32) -> Self {
        Self {
            owner,
            construct: None,
            origin,
            dir,
            range: 250.0,
            damage,
            knockback,
            element: bd_core::mechanics::Element::Light,
            pierce: 0,
            wall: 0.0,
            muzzle: origin,
            falloff_start: 0.0,
            chip: 0.0,
            effects: 0,
            trace: None,
        }
    }
}

/// Distance falloff shared by pellets and hitscan rounds: 1 up to `start`, then linear to 30 % at
/// `start + 18 m`. `start <= 0` = no falloff.
#[inline]
pub fn falloff(start: f32, dist: f32) -> f32 {
    if start > 0.0 { (1.0 - (dist - start).max(0.0) / 18.0).max(0.3) } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::recipe;
    use bd_core::mechanics::Element;

    #[test]
    fn archetypes_from_phrases() {
        let gun = |m: &[Mechanic], count: u8| {
            let mut r = recipe("g", Form::Gun, m, Element::Light, 0.6);
            r.count = count;
            archetype_for(&r)
        };
        assert_eq!(gun(&[], 1), Some(GunArchetype::HandCannon));
        assert_eq!(gun(&[Mechanic::Shoots], 3), Some(GunArchetype::SuperShotgun));
        assert_eq!(gun(&[Mechanic::Spin], 1), Some(GunArchetype::Minigun));
        assert_eq!(gun(&[Mechanic::Explodes], 1), Some(GunArchetype::Rocket));
        assert_eq!(gun(&[Mechanic::Pierce], 1), Some(GunArchetype::Railgun));
        assert_eq!(archetype_for(&recipe("h", Form::Hammer, &[], Element::Light, 1.0)), None);
    }

    #[test]
    fn minigun_spins_up_then_fires_at_cadence() {
        let s = gun_spec(GunArchetype::Minigun);
        let mut w = WeaponState::new(GunArchetype::Minigun, &s);
        let dt = 1.0 / 120.0;
        let mut first = None;
        let mut shots = 0;
        for k in 0..240 {
            if w.step(&s, true, false, dt).is_some() {
                shots += 1;
                first.get_or_insert(k);
            }
        }
        assert!(first.unwrap() >= 47, "0.4 s spin-up");
        assert!((20..=24).contains(&shots), "≈14/s after spin-up: {shots}");
        for _ in 0..40 {
            w.step(&s, false, true, dt);
        }
        assert_eq!(w.phase, WeaponPhase::Idle);
    }

    #[test]
    fn shotgun_two_shots_then_reload_snap() {
        let s = gun_spec(GunArchetype::SuperShotgun);
        let mut w = WeaponState::new(GunArchetype::SuperShotgun, &s);
        let dt = 1.0 / 120.0;
        let mut ticks = Vec::new();
        for k in 0..300 {
            if w.step(&s, true, false, dt).is_some() {
                ticks.push(k);
            }
        }
        assert!(ticks.len() >= 3);
        // Barrel 1 at 0, barrel 2 after 0.25 s, then a 0.9 s reload.
        assert!(ticks[1] - ticks[0] >= 29 && ticks[2] - ticks[1] >= 108, "{ticks:?}");
        let mut dirs = [Vec3::ZERO; 16];
        assert_eq!(pellet_dirs(Vec3::NEG_Z, 9, 4.0, &mut dirs), 9);
        assert!(dirs[..9].iter().all(|d| d.dot(Vec3::NEG_Z) > 4f32.to_radians().cos() - 1e-4));
    }

    #[test]
    fn railgun_charges_then_fires() {
        let s = gun_spec(GunArchetype::Railgun);
        let mut w = WeaponState::new(GunArchetype::Railgun, &s);
        let dt = 1.0 / 120.0;
        let fired = (0..120).position(|_| w.step(&s, true, false, dt).is_some());
        assert!(fired.is_some_and(|k| (59..=61).contains(&k)), "{fired:?}");
    }
}
