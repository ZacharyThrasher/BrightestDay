//! Creature AI toolkit (pure, allocation-free, deterministic): hash noise, cheap *bounded* terrain
//! probes over the voxel heightfield, ground / air steering, line of sight, and the attack timing
//! table shared with the client's animation.
//!
//! No pathfinding: a walker looks ≤ 2 probes ahead along a fan of ≤ 8 headings and takes the first
//! safe one (flat, a hop ≤ its step height, or a safe drop). A probe is one voxel column scan of
//! `(step_up + safe_drop) / 0.5 m` cells, so a creature costs < ~150 voxel reads per tick worst case.
//! Void edges are "cliffs": walkers never step off an island unless told to (charging / flying).

use bd_core::mechanics::Temperament;
use bd_core::voxel::{VoxelQuery, raycast, world_to_voxel};
use bd_core::{ConstructId, IVec3, PlayerId, VOXEL_SIZE, Vec3};
use serde::{Deserialize, Serialize};

use super::{Construct, Env, PlayerView};

// ------------------------------------------------------------------------------------ state

/// What a creature is doing (behaviour mode; drives anim params on the client too).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Mode {
    /// Closing on its target (default).
    #[default]
    Pursue = 0,
    /// No target: idling near the owner.
    Idle = 1,
    /// Fearful: backing off / breaking line of sight.
    Kite = 2,
    /// Protective: circling / body-blocking for the owner.
    Guard = 3,
    /// Lazy: sat down (a roadblock).
    Sit = 4,
    /// Sneaky: circling round to the target's back.
    Flank = 5,
    /// Caged / trapped: rooted in place, thrashing.
    Rooted = 6,
    /// Slammed out of the sky / toppled: pinned, helpless.
    Pinned = 7,
    /// Holding a grabbed / constricted player.
    Holding = 8,
    /// Can't reach its target (island gap): pacing at the edge.
    Stranded = 9,
    /// Transforms: folding between alt mode and robot mode (~0.9 s, stationary).
    Transforming = 10,
    /// Transforms: driving / running in alt mode (vehicle or beast).
    AltMode = 11,
}

impl Mode {
    pub fn from_u8(v: u8) -> Mode {
        use Mode::*;
        match v {
            1 => Idle,
            2 => Kite,
            3 => Guard,
            4 => Sit,
            5 => Flank,
            6 => Rooted,
            7 => Pinned,
            8 => Holding,
            9 => Stranded,
            10 => Transforming,
            11 => AltMode,
            _ => Pursue,
        }
    }
}

/// A creature's current target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Target {
    #[default]
    None,
    Player(PlayerId),
    /// Chaotic creatures attack other people's constructs too.
    Construct(ConstructId),
}

/// The attack in progress (at most one).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttackRun {
    pub kind: AttackKind,
    /// Seconds since it started (the client's anim time: `anim_since` is set at the start).
    pub t: f32,
    pub running: bool,
    /// World aim direction (breath cone, charge / swoop line).
    pub aim: Vec3,
    /// Phase events already emitted (bit 0 telegraph, 1 active, 2 impact).
    pub emitted: u8,
    /// Breath / squeeze damage-tick accumulator.
    pub pulse: f32,
}

/// Creature-engineer state embedded in every [`Construct`] (`c.ai`). Fixed size, no heap.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiState {
    pub init: bool,
    pub mode: Mode,
    pub target: Target,
    /// Seconds until the next retarget decision.
    pub retarget_t: f32,
    pub attack: AttackRun,
    /// Per-[`AttackKind`] cooldowns (s).
    pub cd: [f32; ATTACK_KINDS],
    /// Metres travelled (gait phase for the client, kaiju footsteps).
    pub distance: f32,
    /// Distance at which the next kaiju / Stomp footstep lands.
    pub next_step: f32,
    /// Which foot steps next (false = left).
    pub foot: bool,
    /// Mood timer (Lazy sit / stand cycle, Playful hops).
    pub mood_t: f32,
    pub hop_t: f32,
    /// Protective orbit angle (rad), Sneaky flank side (±1).
    pub orbit: f32,
    pub side: f32,
    /// Held player (Grab / Constrict) and seconds held.
    pub hold: Option<PlayerId>,
    pub hold_t: f32,
    /// Seconds pinned (slammed out of the sky / toppled).
    pub pinned: f32,
    /// Progress watchdog: seconds without getting closer to the goal.
    pub stuck_t: f32,
    pub best_dist: f32,
    /// Smoothed steering heading (horizontal unit) — steering is re-probed every few ticks.
    pub heading: Vec3,
    pub probe_cd: u8,
    /// Anim speed (m/s) for the client's gait.
    pub speed: f32,
    pub airborne: bool,
    /// Height (m) the attack timings are computed for (collider height at first tick ≈ rig height).
    pub timing_h: f32,
    /// Fearful: current cover spot and seconds until it is re-evaluated.
    pub cover: Vec3,
    pub cover_t: f32,
    /// Seconds rooted by overlapping an enemy cage / trap (self-detected; the framework's
    /// `comp.rooted` works the same way). Accumulated thrash time (breaks the cage).
    pub caged: f32,
    pub thrash: f32,
    /// A kaiju was already toppled by this cage.
    pub toppled: bool,
    /// Last mode reported to clients.
    pub sent_mode: u8,
}

/// Number of [`AttackKind`] variants.
pub const ATTACK_KINDS: usize = 11;

/// `GameEvent::CreatureAttack::phase` values.
pub mod phase {
    /// Anticipation starts (clip time 0).
    pub const TELEGRAPH: u8 = 0;
    /// Hit window opens (clip time = anticipation).
    pub const ACTIVE: u8 = 1;
    /// The blow landed (carries the impact scalar).
    pub const IMPACT: u8 = 2;
    /// Recovery finished / attack cancelled.
    pub const END: u8 = 3;
}

// ------------------------------------------------------------------------------------ targeting

/// Default autonomous targeting (every autonomous construct, creature or not): the owner's last
/// aimed / hit enemy if still valid and within the temperament's leash, else the nearest enemy
/// (to the owner for Protective — it defends; to itself otherwise). Playful picks a random enemy
/// that changes every few seconds. Never the owner, never a spawn-shielded or dead player.
pub fn pick_target(c: &Construct, env: &Env<'_>) -> Option<PlayerView> {
    let ctx = env.ctx;
    let prof = super::temperament::profile(c.recipe.temperament, c.recipe.intensity);
    let here = c.center();
    let valid = |p: &PlayerView| p.id != c.owner && p.alive && !p.shielded;
    let leash_ok = |p: &PlayerView| {
        prof.leash.is_infinite() || (p.pos - env.owner.pos).length() <= prof.leash.max(20.0) || (p.pos - here).length() <= 12.0
    };
    // Playful: a random enemy, re-rolled every `fickle_secs`.
    if prof.fickle_secs > 0.0 {
        let n = ctx.enemies_of(c.owner).count() as u32;
        if n > 0 {
            let epoch = (ctx.tick as f32 * ctx.dt / prof.fickle_secs) as u64;
            let k = ((hash01(c.id.0, epoch, 11) * n as f32) as u32).min(n - 1);
            return ctx.enemies_of(c.owner).nth(k as usize).copied();
        }
        return None;
    }
    // Owner's last aimed/hit enemy.
    if let Some(t) = env.owner.last_target.and_then(|t| ctx.player(t)).filter(|t| valid(t) && leash_ok(t)) {
        return Some(*t);
    }
    // Sticky current target (hysteresis: don't flip between two equidistant enemies).
    if let Some(t) = c.target.and_then(|t| ctx.player(t)).filter(|t| valid(t) && leash_ok(t))
        && (prof.temperament == Temperament::Aggressive || (t.pos - here).length() < 30.0)
    {
        return Some(*t);
    }
    let from = if prof.guard_radius > 0.0 { env.owner.pos } else { here };
    ctx.nearest_enemy(c.owner, from).filter(|t| leash_ok(t)).copied()
}

// ------------------------------------------------------------------------------------ noise

/// Deterministic hash → [0, 1). Same inputs, same value, on every machine.
#[inline]
pub fn hash01(a: u32, b: u64, salt: u32) -> f32 {
    let mut z = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ b.wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ (salt as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 * (1.0 / (1u64 << 24) as f32)
}

/// Smooth value noise in [-1, 1] over `t` (one knot per unit), deterministic per `seed`.
pub fn noise1(seed: u32, t: f32) -> f32 {
    let t = if t.is_finite() { t.clamp(-1e6, 1e6) } else { 0.0 };
    let i = t.floor();
    let f = t - i;
    let k = i as i64 as u64;
    let a = hash01(seed, k, 7) * 2.0 - 1.0;
    let b = hash01(seed, k.wrapping_add(1), 7) * 2.0 - 1.0;
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

// ------------------------------------------------------------------------------------ probes

/// Highest walkable surface under (x, z) within [y − down, y + up] (top of a solid voxel with air
/// above). Bounded column scan.
pub fn ground_at<V: VoxelQuery + ?Sized>(v: &V, p: Vec3, up: f32, down: f32) -> Option<f32> {
    if !p.is_finite() {
        return None;
    }
    let top = world_to_voxel(p + Vec3::Y * up.clamp(0.0, 64.0));
    let bottom = world_to_voxel(p - Vec3::Y * down.clamp(0.0, 64.0));
    let mut y = top.y;
    while y >= bottom.y {
        let cell = IVec3::new(top.x, y, top.z);
        if v.is_solid(cell) && !v.is_solid(cell + IVec3::Y) {
            return Some((y + 1) as f32 * VOXEL_SIZE);
        }
        y -= 1;
    }
    None
}

/// What lies `dist` metres ahead of a pair of feet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Footing {
    /// Walkable at about the same height (ground y).
    Flat(f32),
    /// A ledge up to hop onto (ground y), within the step height.
    Step(f32),
    /// Lower ground within the safe drop (ground y).
    Drop(f32),
    /// Too tall to hop.
    Wall,
    /// No ground within the safe drop: an island edge / the void.
    Cliff,
}

impl Footing {
    #[inline]
    pub fn walkable(self) -> bool {
        matches!(self, Footing::Flat(_) | Footing::Step(_) | Footing::Drop(_))
    }
    pub fn ground(self) -> Option<f32> {
        match self {
            Footing::Flat(y) | Footing::Step(y) | Footing::Drop(y) => Some(y),
            _ => None,
        }
    }
}

/// Probe one spot `dist` ahead along horizontal `dir` (unit). `clear` = body height that must be
/// free above a step (a hop needs head room, capped so kaiju don't scan whole towers).
pub fn probe<V: VoxelQuery + ?Sized>(v: &V, feet: Vec3, dir: Vec3, dist: f32, step_up: f32, safe_drop: f32) -> Footing {
    let q = feet + dir * dist;
    // Wall: solid just above the hop height.
    let wall_probe = q + Vec3::Y * (step_up + 0.25);
    if v.is_solid(world_to_voxel(wall_probe)) {
        return Footing::Wall;
    }
    match ground_at(v, q, step_up + 0.05, safe_drop) {
        Some(gy) if gy > feet.y + 0.3 => Footing::Step(gy),
        Some(gy) if gy < feet.y - 0.3 => Footing::Drop(gy),
        Some(gy) => Footing::Flat(gy),
        None => Footing::Cliff,
    }
}

/// Headings tried by [`steer_ground`], radians from the desired heading (sign flipped by `bias`).
const FAN: [f32; 8] = [0.0, 0.6, -0.6, 1.2, -1.2, 1.9, -1.9, std::f32::consts::PI];

#[inline]
fn rot_y(d: Vec3, a: f32) -> Vec3 {
    let (s, c) = a.sin_cos();
    Vec3::new(d.x * c + d.z * s, 0.0, -d.x * s + d.z * c)
}

/// Result of a ground steering decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steer {
    /// Horizontal unit heading to move along (ZERO = nowhere safe: stand still).
    pub dir: Vec3,
    /// Footing along it (at the near probe).
    pub footing: Footing,
    /// True when the desired heading itself was blocked (the creature detoured / stopped).
    pub blocked: bool,
}

/// Pick a safe horizontal heading as close as possible to `desired` (need not be unit). Two probes
/// per candidate (near = one body radius + a step, far = `look`); both must be walkable unless
/// `allow_void` (a charging bull runs off cliffs). `bias` > 0 prefers turning right first.
#[allow(clippy::too_many_arguments)]
pub fn steer_ground<V: VoxelQuery + ?Sized>(
    v: &V,
    feet: Vec3,
    desired: Vec3,
    look: f32,
    step_up: f32,
    safe_drop: f32,
    allow_void: bool,
    bias: f32,
) -> Steer {
    let d = Vec3::new(desired.x, 0.0, desired.z).normalize_or_zero();
    if d == Vec3::ZERO {
        return Steer { dir: Vec3::ZERO, footing: Footing::Flat(feet.y), blocked: false };
    }
    let near = (look * 0.45).clamp(0.4, 3.0);
    let far = look.clamp(near + 0.1, 12.0);
    let sign = if bias < 0.0 { -1.0 } else { 1.0 };
    for (k, a) in FAN.iter().enumerate() {
        let dir = rot_y(d, a * sign);
        let f1 = probe(v, feet, dir, near, step_up, safe_drop);
        let ok1 = f1.walkable() || (allow_void && f1 == Footing::Cliff);
        if !ok1 {
            continue;
        }
        // From the near footing, look further (a hop then a cliff is still a cliff).
        let near_pt = feet + dir * near;
        let base = Vec3::new(near_pt.x, f1.ground().unwrap_or(feet.y), near_pt.z);
        let f2 = probe(v, base, dir, far - near, step_up, safe_drop);
        let ok2 = f2.walkable() || (allow_void && f2 == Footing::Cliff) || f2 == Footing::Wall;
        // A wall further out is fine for now (we'll turn when we get there); a cliff is not.
        if ok2 {
            return Steer { dir, footing: f1, blocked: k != 0 };
        }
    }
    Steer { dir: Vec3::ZERO, footing: Footing::Cliff, blocked: true }
}

/// Air steering for fliers: keep `desired_vel` but bend away from terrain within `lookahead`
/// (one ray along the velocity, one down for ground clearance). Returns the adjusted velocity.
pub fn steer_air<V: VoxelQuery + ?Sized>(v: &V, pos: Vec3, desired_vel: Vec3, radius: f32, lookahead: f32, min_clearance: f32) -> Vec3 {
    let speed = desired_vel.length();
    if speed < 1e-3 || !pos.is_finite() {
        return desired_vel;
    }
    let dir = desired_vel / speed;
    let mut out = dir;
    let reach = lookahead.max(1.0) + radius;
    if let Some(h) = raycast(v, pos, dir, reach) {
        let n = h.normal.as_vec3();
        let n = if n == Vec3::ZERO { -dir } else { n };
        let urgency = 1.0 - (h.t / reach).clamp(0.0, 1.0);
        // Slide along the obstacle and climb over it.
        out = (dir - n * dir.dot(n) * (1.0 + urgency) + Vec3::Y * (0.6 + urgency)).normalize_or(Vec3::Y);
    }
    // Ground clearance.
    if min_clearance > 0.0 && raycast(v, pos, Vec3::NEG_Y, min_clearance + radius).is_some() && out.y < 0.3 {
        out = (out + Vec3::Y * 0.6).normalize_or(Vec3::Y);
    }
    out * speed
}

/// Clear line between two points through terrain?
#[inline]
pub fn line_of_sight<V: VoxelQuery + ?Sized>(v: &V, a: Vec3, b: Vec3) -> bool {
    let d = b - a;
    let len = d.length();
    len < 1e-3 || raycast(v, a, d, len).is_none()
}

/// Is standing at `p` hidden from an eye at `eye` (terrain blocks the line to p's chest)?
#[inline]
pub fn hidden_from<V: VoxelQuery + ?Sized>(v: &V, eye: Vec3, p: Vec3, chest: f32) -> bool {
    !line_of_sight(v, eye, p + Vec3::Y * chest)
}

// ------------------------------------------------------------------------------------ attacks

/// Creature attacks (the verbs of COMBAT.md §4 "Creatures"). Replicated as a byte; the client maps
/// it to its animation clip ([`AttackKind::clip`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AttackKind {
    #[default]
    Bite = 0,
    Swipe = 1,
    /// Telegraph 0.4 s, dash 25 m/s.
    Charge = 2,
    /// Shockwave ring + carve.
    Stomp = 3,
    /// 360° tail sweep.
    TailSweep = 4,
    /// Telegraph 0.6 s, cone of the element (or the payload) for the active window.
    Breath = 5,
    /// Whip / tentacle crack.
    Lash = 6,
    /// Flier dive through the target.
    Swoop = 7,
    /// Jaws / pincers seize: hold 1 s, then fling.
    Grab = 8,
    /// Wrap & squeeze (DoT); a tether shot breaks it.
    Constrict = 9,
    /// Ranged spit of the payload / element (Fearful kiting, Shoots).
    Spit = 10,
}

/// The animation clip family (mirrors `bd_shapes::anim::Attack` without the dependency). The sim
/// takes its telegraph / hit windows from [`attack_timing`] with the same clip, so what the client
/// animates is what hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    Bite,
    Swipe,
    Charge,
    Stomp,
    TailSweep,
    Breath,
    Lash,
}

impl AttackKind {
    pub fn from_u8(v: u8) -> AttackKind {
        use AttackKind::*;
        match v {
            1 => Swipe,
            2 => Charge,
            3 => Stomp,
            4 => TailSweep,
            5 => Breath,
            6 => Lash,
            7 => Swoop,
            8 => Grab,
            9 => Constrict,
            10 => Spit,
            _ => Bite,
        }
    }

    /// Which `bd_shapes::anim::Attack` clip animates it.
    pub fn clip(self) -> Clip {
        match self {
            AttackKind::Bite | AttackKind::Grab | AttackKind::Constrict | AttackKind::Spit => Clip::Bite,
            AttackKind::Swipe => Clip::Swipe,
            AttackKind::Charge | AttackKind::Swoop => Clip::Charge,
            AttackKind::Stomp => Clip::Stomp,
            AttackKind::TailSweep => Clip::TailSweep,
            AttackKind::Breath => Clip::Breath,
            AttackKind::Lash => Clip::Lash,
        }
    }
}

/// Telegraph (anticipation) / hit window (active) / recovery, seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timing {
    pub anticipation: f32,
    pub active: f32,
    pub recovery: f32,
}

impl Timing {
    #[inline]
    pub fn total(&self) -> f32 {
        self.anticipation + self.active + self.recovery
    }
    /// The frame the blow lands (bite contact, stomp impact).
    #[inline]
    pub fn impact(&self) -> f32 {
        self.anticipation + self.active
    }
    #[inline]
    pub fn is_active(&self, t: f32) -> bool {
        t >= self.anticipation && t < self.anticipation + self.active
    }
}

/// **Must equal `bd_shapes::anim::attack_timing(model, clip)`** for a model whose rig height is
/// `height` (m). Size-scaled (big creatures telegraph longer); Charge / Breath telegraphs are fixed
/// by COMBAT.md (0.4 s / 0.6 s) so players learn them. Kept in lock-step by the
/// `timing_matches_bd_shapes` test (bd_shapes is a dev-dependency).
pub fn attack_timing(height: f32, clip: Clip) -> Timing {
    attack_timing_hw(height, clip, 1.0)
}

/// Mass-scaled attack timing: **must equal `bd_shapes::anim::attack_timing_hw(height, clip,
/// weight)`** (and so `attack_timing(model, clip)` for a model whose `weight` is `weight`). The sim
/// passes `bd_shapes::weight_of(&recipe)` and the client sets `model.weight` from the same recipe.
/// Heavier: longer wind-up, a slightly snappier strike, slower settle; Charge / Breath telegraphs
/// stay fixed (COMBAT.md). Weight 1 is bit-identical to the authored timing.
pub fn attack_timing_hw(height: f32, clip: Clip, weight: f32) -> Timing {
    let h = if height.is_finite() { height } else { 2.0 };
    let sf = (h / 2.0).max(0.1).powf(0.3).clamp(0.85, 2.0);
    let w = if weight.is_finite() { weight.clamp(0.5, 2.5) } else { 1.0 };
    let (wa, wc, wr) = (w.powf(0.6), w.powf(-0.2).clamp(0.85, 1.1), w.powf(0.5));
    let fixed = matches!(clip, Clip::Charge | Clip::Breath);
    let (a, c, r) = match clip {
        Clip::Bite => (0.2 * sf, 0.08 * sf, 0.25 * sf),
        Clip::Swipe => (0.18 * sf, 0.1 * sf, 0.25 * sf),
        Clip::Charge => (0.4, 0.6, 0.3 * sf),
        Clip::Stomp => (0.42 * sf, 0.1 * sf, 0.45 * sf),
        Clip::TailSweep => (0.25 * sf, 0.45 * sf, 0.3 * sf),
        Clip::Breath => (0.6, 1.2, 0.35 * sf),
        Clip::Lash => (0.12, 0.12, 0.2),
    };
    if fixed {
        return Timing { anticipation: a, active: c, recovery: r * wr };
    }
    Timing { anticipation: a * wa, active: c * wc, recovery: r * wr }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny grid: ground at y = 0 (top at 0.5 m) over x,z ∈ [-20, 20) voxels, with a 1 m ledge
    /// block for x ≥ 6 and a 3 m wall for z ≥ 8; void beyond x ≥ 16.
    struct Grid;
    impl VoxelQuery for Grid {
        fn material(&self, v: IVec3) -> u8 {
            if v.x < -20 || v.x >= 16 || v.z < -20 || v.z >= 20 {
                return 0;
            }
            let top = if v.z >= 8 {
                6
            } else if v.x >= 6 {
                2
            } else {
                0
            };
            u8::from(v.y >= -2 && v.y <= top)
        }
    }

    #[test]
    fn noise_is_deterministic_and_bounded() {
        for k in 0..200 {
            let t = k as f32 * 0.37;
            let a = noise1(3, t);
            assert_eq!(a, noise1(3, t));
            assert!((-1.0..=1.0).contains(&a));
        }
        assert_ne!(hash01(1, 2, 3), hash01(1, 2, 4));
    }

    #[test]
    fn probes_classify_terrain() {
        let feet = Vec3::new(0.25, 0.5, 0.25);
        assert_eq!(probe(&Grid, feet, Vec3::Z, 1.0, 1.0, 3.0), Footing::Flat(0.5));
        // 1 m ledge at x ≥ 3 m: hop-able.
        assert_eq!(probe(&Grid, feet, Vec3::X, 3.2, 1.0, 3.0), Footing::Step(1.5));
        // 3 m wall at z ≥ 4 m: not.
        assert_eq!(probe(&Grid, feet, Vec3::Z, 4.2, 1.0, 3.0), Footing::Wall);
        // Island edge at x = -10 m.
        assert_eq!(probe(&Grid, feet, Vec3::NEG_X, 11.0, 1.0, 3.0), Footing::Cliff);
    }

    #[test]
    fn steering_detours_around_cliffs_and_walls() {
        let feet = Vec3::new(-9.0, 0.5, 0.25);
        // Straight on is the void: must turn.
        let s = steer_ground(&Grid, feet, Vec3::NEG_X, 2.0, 1.0, 3.0, false, 1.0);
        assert!(s.blocked && s.dir != Vec3::ZERO && s.dir.x > -0.9);
        assert!(probe(&Grid, feet, s.dir, 2.0, 1.0, 3.0).walkable());
        // Allowed off the edge when charging.
        assert!(!steer_ground(&Grid, feet, Vec3::NEG_X, 2.0, 1.0, 3.0, true, 1.0).blocked);
        // Straight at the wall from close: detours.
        let s = steer_ground(&Grid, Vec3::new(0.0, 0.5, 3.4), Vec3::Z, 1.5, 1.0, 3.0, false, 1.0);
        assert!(s.blocked && s.dir.z < 0.9);
    }

    #[test]
    fn air_steering_climbs_over_terrain() {
        let v = steer_air(&Grid, Vec3::new(0.0, 2.0, 0.0), Vec3::Z * 10.0, 0.5, 6.0, 0.0);
        assert!(v.y > 1.0, "climbs over the wall ahead: {v:?}");
        assert!((v.length() - 10.0).abs() < 1e-3);
        let free = steer_air(&Grid, Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Z * 10.0, 0.5, 6.0, 0.0);
        assert_eq!(free, Vec3::NEG_Z * 10.0);
    }

    #[test]
    fn line_of_sight_through_terrain() {
        assert!(line_of_sight(&Grid, Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 2.0, 3.0)));
        assert!(!line_of_sight(&Grid, Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 2.0, 12.0)));
    }

    #[test]
    fn fixed_telegraphs_per_design() {
        for h in [0.5, 2.0, 14.0, 60.0] {
            assert_eq!(attack_timing(h, Clip::Charge).anticipation, 0.4);
            assert_eq!(attack_timing(h, Clip::Breath).anticipation, 0.6);
        }
        assert!(attack_timing(20.0, Clip::Stomp).anticipation > attack_timing(1.0, Clip::Stomp).anticipation);
        assert_eq!(AttackKind::from_u8(AttackKind::Spit as u8), AttackKind::Spit);
        assert_eq!(AttackKind::from_u8(200), AttackKind::Bite);
    }
}
