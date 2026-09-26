//! **Temperament** (COMBAT.md §5c) — OWNED BY THE CREATURE ENGINEER.
//!
//! Emotion/attitude words change *behaviour and animation*, never stats. Two pure tables:
//! * [`profile`] — behaviour knobs the brains read (leash, kite distance, cover, orbit/intercept,
//!   weave/hops/pranks, sitting, whom it may attack, flanking, attack rhythm);
//! * [`anim_params`] — what the client reads to animate the same mood (twitch amplitude/frequency,
//!   bounce, droop, lean, glow — Sneaky is *dim*, crouch).
//!
//! Both are deterministic functions of `(Temperament, intensity)` from the replicated recipe, so
//! nothing extra goes on the wire. `intensity` (0..1) scales how *strongly* the behaviour shows:
//! a slightly lazy dog dawdles, a very lazy dog lies down in the road.
//!
//! The hooks at the bottom ([`steer_goal`], [`speed_mult`], [`cooldown_mult`]) are what the generic
//! motion components call for autonomous NON-creature constructs (a flying hammer, a homing
//! trampoline, a wall with legs), so "an angry flying hammer" and "a lazy wall with legs" behave
//! differently on the same code path. Creature controllers use the full profile.

use bd_core::Vec3;
use bd_core::mechanics::Temperament;

use super::ai::{hash01, noise1};
use super::{Construct, Env, PlayerView};

/// Behaviour knobs for the brains. "Numbers" here are behaviour *shapes* (distances, rhythms,
/// probabilities), not damage/health multipliers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub temperament: Temperament,
    pub intensity: f32,
    /// Locomotion pace multiplier (Lazy dawdles, Sneaky stalks).
    pub pace: f32,
    /// Attack rhythm: multiplier on cooldowns / recovery between attacks (Aggressive < 1).
    pub rhythm: f32,
    /// How far from its owner it will chase a target (m): unlimited, except Protective, which
    /// stays with its owner.
    pub leash: f32,
    /// Fearful: preferred distance to its target (m); 0 = close in.
    pub keep_away: f32,
    /// Fearful: seeks cover (breaks line of sight) when the target gets close.
    pub uses_cover: bool,
    /// Attacks from range with its payload / element instead of closing in.
    pub ranged: bool,
    /// Protective: guard radius around the owner (m); 0 = no guarding.
    pub guard_radius: f32,
    /// Protective: engage enemies that come this close to the owner (m).
    pub intercept_radius: f32,
    /// Playful: lateral weave (fraction of heading) and hop cadence (hops/s).
    pub weave: f32,
    pub hop_rate: f32,
    /// Playful: seconds between random target switches (0 = never at random).
    pub fickle_secs: f32,
    /// Playful: knockback pranks — hits shove more and hurt less (damage × (1 − p), knockback × (1 + p)).
    pub prank: f32,
    /// Lazy: fraction of time spent sitting, and how close a target must be to rouse it (m).
    pub sit_fraction: f32,
    pub rouse_range: f32,
    /// Chaotic: attacks any player but its owner, and any construct not its owner's.
    pub hits_anyone: bool,
    /// Chaotic: speed lurch amplitude (0..1) and retarget interval (s).
    pub lurch: f32,
    pub retarget_secs: f32,
    /// Sneaky: approaches from behind its target (flank distance, m); 0 = straight line.
    pub flank: f32,
    /// Never backs off, never pauses between attacks (Aggressive).
    pub relentless: bool,
}

/// Animation parameters the client reads (0..1 unless noted). Deterministic from the recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemperamentAnim {
    /// Twitchy jitter amplitude on head/limbs (Aggressive, Chaotic; Fearful shivers).
    pub twitch: f32,
    /// Twitch frequency (Hz).
    pub twitch_hz: f32,
    /// Bouncy squash & stretch on every step (Playful).
    pub bounce: f32,
    /// Head/body droop (Lazy, Fearful cower).
    pub droop: f32,
    /// Lean into the pursuit (+, Aggressive) or away (−, Fearful), radians.
    pub lean: f32,
    /// Glow brightness multiplier (Sneaky is dim: see [`TemperamentAnim::dim`]).
    pub glow: f32,
    /// Crouched stalking pose (Sneaky).
    pub crouch: f32,
}

impl TemperamentAnim {
    /// The "dim flag": Sneaky constructs render with a dimmed glow.
    pub fn dim(&self) -> bool {
        self.glow < 0.85
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn clamp_intensity(i: f32) -> f32 {
    if i.is_finite() { i.clamp(0.0, 1.0) } else { 0.0 }
}

/// Behaviour profile. A temperament word with intensity 0 still shows faintly (words always *do*
/// something): the effective strength is `0.35 + 0.65·intensity`.
pub fn profile(t: Temperament, intensity: f32) -> Profile {
    let i = clamp_intensity(intensity);
    let k = 0.35 + 0.65 * i;
    let mut p = Profile {
        temperament: t,
        intensity: i,
        pace: 1.0,
        rhythm: 1.0,
        leash: f32::INFINITY,
        keep_away: 0.0,
        uses_cover: false,
        ranged: false,
        guard_radius: 0.0,
        intercept_radius: 0.0,
        weave: 0.0,
        hop_rate: 0.0,
        fickle_secs: 0.0,
        prank: 0.0,
        sit_fraction: 0.0,
        rouse_range: 0.0,
        hits_anyone: false,
        lurch: 0.0,
        retarget_secs: 0.0,
        flank: 0.0,
        relentless: false,
    };
    match t {
        Temperament::Calm => {}
        Temperament::Aggressive => {
            p.rhythm = lerp(0.8, 0.45, i);
            p.leash = f32::INFINITY;
            p.relentless = true;
        }
        Temperament::Fearful => {
            p.keep_away = lerp(7.0, 13.0, i);
            p.uses_cover = true;
            p.ranged = true;
        }
        Temperament::Protective => {
            p.guard_radius = lerp(3.0, 2.2, i);
            p.intercept_radius = lerp(8.0, 12.0, i);
            p.leash = 14.0;
        }
        Temperament::Playful => {
            p.weave = 0.35 + 0.55 * k;
            p.hop_rate = lerp(0.6, 1.6, i);
            p.fickle_secs = lerp(6.0, 3.0, i);
            p.prank = 0.25 + 0.35 * k;
        }
        Temperament::Lazy => {
            p.pace = lerp(0.7, 0.4, i);
            p.sit_fraction = lerp(0.35, 0.75, i);
            p.rouse_range = lerp(9.0, 4.5, i);
            p.rhythm = lerp(1.2, 1.6, i);
        }
        Temperament::Chaotic => {
            p.hits_anyone = true;
            p.lurch = 0.25 + 0.45 * k;
            p.retarget_secs = lerp(2.5, 1.2, i);
            p.weave = 0.2 * k;
        }
        Temperament::Sneaky => {
            p.flank = lerp(4.0, 6.5, i);
            p.pace = lerp(0.85, 0.7, i);
        }
    }
    p
}

/// Client-side animation parameters for a temperament.
pub fn anim_params(t: Temperament, intensity: f32) -> TemperamentAnim {
    let i = clamp_intensity(intensity);
    let k = 0.35 + 0.65 * i;
    let mut a = TemperamentAnim { twitch: 0.0, twitch_hz: 0.0, bounce: 0.0, droop: 0.0, lean: 0.0, glow: 1.0, crouch: 0.0 };
    match t {
        Temperament::Calm => {}
        Temperament::Aggressive => {
            a.twitch = 0.3 + 0.7 * k;
            a.twitch_hz = lerp(7.0, 13.0, i);
            a.lean = 0.12 * k;
            a.glow = 1.0 + 0.25 * k;
        }
        Temperament::Fearful => {
            a.twitch = 0.25 * k;
            a.twitch_hz = 16.0;
            a.droop = 0.4 * k;
            a.lean = -0.1 * k;
        }
        Temperament::Protective => a.lean = 0.04,
        Temperament::Playful => a.bounce = 0.4 + 0.6 * k,
        Temperament::Lazy => {
            a.droop = 0.5 + 0.5 * k;
            a.glow = 1.0 - 0.1 * k;
        }
        Temperament::Chaotic => {
            a.twitch = 0.5 + 0.5 * k;
            a.twitch_hz = lerp(4.0, 9.0, i);
            a.bounce = 0.2 * k;
        }
        Temperament::Sneaky => {
            a.glow = 1.0 - 0.6 * k;
            a.crouch = 0.3 + 0.5 * k;
        }
    }
    a
}

/// The profile of a construct's recipe.
#[inline]
pub fn of(c: &Construct) -> Profile {
    profile(c.recipe.temperament, c.recipe.intensity)
}

/// Lazy: is it sitting right now? A deterministic ~6 s sit/stand cycle per construct (phase from
/// its id), unless a target is within `rouse_range`.
pub fn sitting(c: &Construct, p: &Profile, time: f32, target_dist: Option<f32>) -> bool {
    if p.sit_fraction <= 0.0 || target_dist.is_some_and(|d| d < p.rouse_range) {
        return false;
    }
    let period = 6.0;
    let phase = (time / period + hash01(c.id.0, 0, 3)).fract();
    phase < p.sit_fraction
}

// ------------------------------------------------------------------------------------ hooks

/// Where an autonomous construct wants to go this tick, given its target and the default goal
/// (the target's position, or near the owner without a target): Fearful kites to `keep_away`,
/// Protective stands between its owner and the threat (body-blocking) or intercepts close
/// attackers, Playful weaves, Chaotic lurches, Sneaky circles to the target's back, Lazy sits.
pub fn steer_goal(c: &Construct, env: &Env<'_>, target: Option<&PlayerView>, goal: Vec3) -> Vec3 {
    let p = of(c);
    let time = env.tick() as f32 * env.dt();
    let here = c.pos;
    let owner = env.owner.pos;
    match p.temperament {
        Temperament::Protective => match target {
            Some(t) if (t.pos - owner).length() <= p.intercept_radius => t.pos,
            Some(t) => owner + flat(t.pos - owner).normalize_or(Vec3::NEG_Z) * p.guard_radius,
            None => {
                let a = time * 0.8 + hash01(c.id.0, 1, 5) * std::f32::consts::TAU;
                owner + Vec3::new(a.cos(), 0.0, a.sin()) * p.guard_radius
            }
        },
        Temperament::Fearful => match target {
            Some(t) => {
                let away = flat(here - t.pos).normalize_or(Vec3::Z);
                t.pos + away * p.keep_away
            }
            None => goal,
        },
        Temperament::Lazy => {
            let d = target.map(|t| (t.pos - here).length());
            if sitting(c, &p, time, d) { here } else { goal }
        }
        Temperament::Playful | Temperament::Chaotic => {
            let to = flat(goal - here);
            let side = Vec3::new(-to.z, 0.0, to.x).normalize_or_zero();
            let w = noise1(c.id.0, time * if p.temperament == Temperament::Playful { 0.9 } else { 1.7 });
            goal + side * (w * p.weave.max(p.lurch) * to.length().min(8.0))
        }
        Temperament::Sneaky => match target {
            Some(t) => flank_point(t, here, p.flank, if hash01(c.id.0, 2, 9) < 0.5 { -1.0 } else { 1.0 }),
            None => goal,
        },
        _ => goal,
    }
}

/// Sneaky: a point behind `t` (relative to its look), off to `side`; once already behind it
/// (outside its ~120° view cone) head straight in.
pub fn flank_point(t: &PlayerView, here: Vec3, flank: f32, side: f32) -> Vec3 {
    let look = flat(t.look).normalize_or(Vec3::NEG_Z);
    let to_me = flat(here - t.pos);
    let d = to_me.length();
    let seen = d > 1e-3 && look.dot(to_me / d) > -0.2;
    if !seen || d < 2.5 {
        return t.pos;
    }
    let right = Vec3::new(-look.z, 0.0, look.x);
    t.pos - look * flank + right * (side * flank * 0.6)
}

/// Speed multiplier for autonomous motion (Lazy dawdles and stops, Sneaky stalks, Chaotic lurches).
pub fn speed_mult(c: &Construct) -> f32 {
    let p = of(c);
    let mut s = p.pace;
    if p.lurch > 0.0 {
        s *= 1.0 + p.lurch * noise1(c.id.0 ^ 0x55, c.age * 2.3);
    }
    s.clamp(0.2, 1.6)
}

/// Multiplier on attack / emitter cooldowns (Aggressive attacks faster, Lazy slower).
pub fn cooldown_mult(c: &Construct) -> f32 {
    of(c).rhythm
}

#[inline]
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Temperament; 8] = [
        Temperament::Calm,
        Temperament::Aggressive,
        Temperament::Fearful,
        Temperament::Protective,
        Temperament::Playful,
        Temperament::Lazy,
        Temperament::Chaotic,
        Temperament::Sneaky,
    ];

    #[test]
    fn profiles_are_finite_and_sane_for_any_intensity() {
        for t in ALL {
            for i in [f32::NAN, -1.0, 0.0, 0.3, 0.6, 1.0, 7.0] {
                let p = profile(t, i);
                assert!(p.pace > 0.0 && p.pace <= 1.0 && p.rhythm > 0.0 && p.leash > 0.0, "{t:?}");
                assert!((0.0..=1.0).contains(&p.intensity));
                let a = anim_params(t, i);
                assert!(a.twitch.is_finite() && a.glow > 0.0 && a.bounce >= 0.0);
            }
        }
    }

    #[test]
    fn intensity_scales_behaviour_strength() {
        assert!(profile(Temperament::Aggressive, 1.0).rhythm < profile(Temperament::Aggressive, 0.1).rhythm);
        assert!(anim_params(Temperament::Aggressive, 1.0).twitch > anim_params(Temperament::Aggressive, 0.1).twitch);
        assert!(profile(Temperament::Lazy, 1.0).sit_fraction > profile(Temperament::Lazy, 0.1).sit_fraction);
        assert!(profile(Temperament::Fearful, 1.0).keep_away > profile(Temperament::Fearful, 0.1).keep_away);
        assert!(anim_params(Temperament::Sneaky, 1.0).dim() && !anim_params(Temperament::Calm, 1.0).dim());
    }

    #[test]
    fn each_temperament_has_a_distinct_signature() {
        // Behaviour, not numbers: every non-calm temperament flips a qualitative switch.
        let c = profile(Temperament::Calm, 0.6);
        for t in ALL.into_iter().skip(1) {
            let p = profile(t, 0.6);
            let differs = p.relentless != c.relentless
                || p.keep_away != c.keep_away
                || p.guard_radius != c.guard_radius
                || p.weave != c.weave
                || p.sit_fraction != c.sit_fraction
                || p.hits_anyone != c.hits_anyone
                || p.flank != c.flank;
            assert!(differs, "{t:?} has no behaviour");
        }
    }
}
