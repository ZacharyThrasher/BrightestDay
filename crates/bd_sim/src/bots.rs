//! Bots: AI players that produce `PlayerInput` (and occasional casts / oath recitals) from a
//! read-only look at the [`World`]. Used by tests, practice mode and the stress / soak benches.
//!
//! Behaviour (deterministic: seeded RNG, reads only world state):
//! * picks the nearest threat (preferring whoever hit it last) and aims like a person
//!   ([`AimProfile`]): it sees the target **late** (per-bot reaction delay, 150–350 ms by
//!   difficulty) and extrapolates only part of that, its aim **trails** a target that sweeps across
//!   its view (error grows with angular speed), wobbles with gaussian-ish jitter, sometimes
//!   **overshoots** a big flick, and leads projectiles by their real muzzle speed (hitscan: none);
//! * strafes, **bunny-hops** at range (held jump + air strafing), backs off at low health;
//! * **tethers** across gaps toward its target and fires an emergency tether when falling toward
//!   the void (using the real `query_anchor` crosshair query);
//! * ring-blasts when it has a line of sight, shoves when crowded, swings / fires held constructs;
//! * casts from a small built-in word list ([`word_list`]) chosen by situation (close → melee
//!   weapon, mid → creature / thrown object, hurt → wall / dome / parachute);
//! * recites the oath when willpower is low and nobody is near.
//!
//! `difficulty` 0..1 scales reaction delay, turn speed, aim error, bhop use and typing speed.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic, ModelSource, Recipe, Temperament};
use bd_core::voxel::raycast;
use bd_core::{DT, PlayerId, Tick, Vec3};

use crate::movement::{CollisionWorld, query_anchor};
use crate::rng::SimRng;
use crate::willpower::{OathState, base_cost};
use crate::world::{Aim, TargetHint, World};

/// Build a recipe by hand (normalised).
pub fn recipe(name: &str, form: Form, mechanics: &[Mechanic], element: Element, scale: f32) -> Recipe {
    Recipe {
        name: name.to_string(),
        form,
        mechanics: mechanics.to_vec(),
        element,
        scale,
        mass: 20.0,
        count: 1,
        accent: None,
        model: ModelSource::Procedural { seed: crate::rng::word_hash(name) as u32 },
        payload: None,
        features: vec![],
        grants: None,
        temperament: Temperament::Calm,
        intensity: 0.0,
        mount: None,
    }
    .normalized()
}

/// Situational word groups for bots (no lexicon dependency).
pub struct WordList {
    pub melee: Vec<Recipe>,
    pub ranged: Vec<Recipe>,
    pub summon: Vec<Recipe>,
    pub thrown: Vec<Recipe>,
    pub defence: Vec<Recipe>,
    pub mobility: Vec<Recipe>,
}

/// The bots' vocabulary.
pub fn word_list() -> WordList {
    use Element::*;
    use Form::*;
    use Mechanic::*;
    let mut bees = recipe("angry bees", Insect, &[Swarm, Flies], Light, 0.5);
    bees.count = 6;
    bees.temperament = Temperament::Aggressive;
    bees.intensity = 0.8;
    let mut godzilla = recipe("godzilla", Kaiju, &[Stomp, ArticulatedTail, Breath], Atomic, 14.0);
    godzilla.mass = 20000.0;
    let mut cannon = recipe("cannon that shoots anvils", Cannon, &[], Light, 1.2);
    cannon.payload = Some(Box::new(recipe("anvil", Anvil, &[Heavy], Light, 1.0)));
    WordList {
        melee: vec![
            recipe("hammer", Hammer, &[Blunt], Light, 1.2),
            recipe("sword", Sword, &[Blade], Light, 1.1),
            recipe("axe", Axe, &[Blade], Light, 1.0),
            recipe("spear", Spear, &[Pierce], Light, 2.2),
            recipe("mjolnir", Hammer, &[Returning, Blunt], Electric, 1.0),
            recipe("flaming sword", Sword, &[Blade], Fire, 1.2),
            recipe("ice gauntlet", Gauntlet, &[], Ice, 0.5),
        ],
        ranged: vec![
            recipe("gun", Gun, &[Shoots], Light, 0.6),
            { let mut r = recipe("shotgun", Gun, &[Shoots], Light, 0.8); r.count = 3; r },
            recipe("minigun", Gun, &[Shoots, Spin], Light, 1.0),
            recipe("rocket launcher", Gun, &[Shoots, Explodes], Fire, 1.0),
            recipe("railgun", Gun, &[Shoots, Pierce], Electric, 1.0),
            recipe("bow", Bow, &[Shoots], Light, 1.0),
            cannon,
            recipe("fire staff", Staff, &[Shoots], Fire, 1.6),
            recipe("shrink ray", Staff, &[Shoots, Shrink], Psychic, 1.2),
            recipe("water gun", Gun, &[Shoots], Water, 0.6),
        ],
        summon: vec![
            recipe("wolf", Quadruped, &[Walks], Light, 1.2),
            recipe("charging bull", Quadruped, &[Charge], Light, 1.6),
            recipe("dragon", Biped, &[Flies, Breath], Fire, 2.5),
            bees,
            godzilla,
        ],
        thrown: vec![
            recipe("ball", Ball, &[Bounce], Light, 0.8),
            recipe("bomb", Bomb, &[Explodes], Fire, 0.7),
            recipe("anvil", Anvil, &[DropFromSky, Heavy], Light, 1.0),
            recipe("boulder", Boulder, &[Heavy], Earth, 1.6),
            recipe("rocket", Rocket, &[Homing, Explodes], Fire, 1.2),
            recipe("black hole", Ball, &[Attract, Explodes], Shadow, 1.0),
        ],
        defence: vec![
            recipe("wall", Wall, &[Barrier], Light, 6.0),
            recipe("dome", Dome, &[Barrier], Light, 3.0),
            recipe("pillar", Pillar, &[], Light, 8.0),
            recipe("trampoline", Trampoline, &[Bounce], Light, 3.0),
        ],
        mobility: vec![
            recipe("wings", Wings, &[Glide], Light, 2.0),
            recipe("parachute", Parachute, &[SlowFall], Light, 1.5),
            recipe("jetpack", Jetpack, &[Thrust], Fire, 0.8),
        ],
    }
}

/// How a bot aims (COMBAT.md §4b: hitscan bots must not be aimbots). All angles in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimProfile {
    /// Perception delay (s): the bot aims at where the target *was* this long ago.
    pub reaction: f32,
    /// Fraction of the (delayed) target velocity extrapolated over the reaction delay (0 = none,
    /// 1 = perfect compensation for straight-line movers).
    pub predict: f32,
    /// Gaussian-ish aim wobble σ (rad) on a stationary target.
    pub jitter: f32,
    /// Tracking lag (s): the aim trails a target crossing the view by `angular speed × tracking`.
    pub tracking: f32,
    /// Chance that a big flick (> [`FLICK_ANGLE`]) overshoots…
    pub overshoot_chance: f32,
    /// …by about this fraction of the flick.
    pub overshoot: f32,
    /// Max turn rate (rad/s).
    pub turn_rate: f32,
}

/// A re-aim larger than this (rad) is a flick (may overshoot).
pub const FLICK_ANGLE: f32 = 0.35;
/// Perception history length (ticks): ≥ the slowest reaction delay.
const SEEN_TICKS: usize = 64;

impl AimProfile {
    /// Difficulty 0..1 → aim numbers (reaction 350 → 150 ms, jitter ≈ 2° → 0.2°, …).
    pub fn for_difficulty(difficulty: f32) -> Self {
        let d = if difficulty.is_finite() { difficulty.clamp(0.0, 1.0) } else { 0.5 };
        let e = 1.0 - d;
        Self {
            reaction: 0.35 - 0.20 * d,
            predict: 0.5 + 0.45 * d,
            jitter: 0.004 + 0.030 * e,
            tracking: 0.02 + 0.10 * e,
            overshoot_chance: 0.2 + 0.5 * e,
            overshoot: 0.08 + 0.2 * e,
            turn_rate: 4.0 + 14.0 * d,
        }
    }
}

/// What a bot wants to do this tick.
#[derive(Clone, Debug, Default)]
pub struct BotAction {
    pub input: PlayerInput,
    pub cast: Option<(Recipe, Aim)>,
    /// Start reciting this oath section (bots rotate through the sections like players do).
    pub begin_oath: Option<u8>,
    /// (accuracy, seconds)
    pub finish_oath: Option<(f32, f32)>,
    pub dismiss: bool,
}

pub struct BotBrain {
    pub id: PlayerId,
    /// 0..1.
    pub difficulty: f32,
    pub casts_enabled: bool,
    rng: SimRng,
    words: WordList,
    target: Option<PlayerId>,
    retarget: f32,
    strafe: f32,
    strafe_t: f32,
    yaw: f32,
    pitch: f32,
    noise: (f32, f32),
    noise_goal: (f32, f32),
    noise_t: f32,
    aim: AimProfile,
    /// Perception ring: the current target's (chest, velocity) per tick, newest at `seen_head`.
    seen: [(Vec3, Vec3); SEEN_TICKS],
    seen_head: usize,
    seen_n: usize,
    seen_of: Option<PlayerId>,
    /// Slowly varying multiplier on the tracking-lag trail.
    trail_k: f32,
    /// Active flick overshoot (yaw, pitch), decaying.
    overshoot: (f32, f32),
    overshoot_cd: f32,
    tether_t: f32,
    tether_dir: Option<Vec3>,
    cast_cd: f32,
    oath_plan: Option<(Tick, f32, f32)>,
    /// Next oath section to recite (rotates, never the same one twice in a row).
    oath_section: u8,
    seq: u32,
    last_ground: Vec3,
    /// ASSIST HOOK: hook-and-strike attempt (`crate::hook::bot_hook_plan`).
    hook: crate::hook::BotHook,
    /// DODGE HOOK: air jump + dodge decisions (`crate::dodge::bot_moves`).
    moves: crate::dodge::BotMoves,
}

impl BotBrain {
    pub fn new(id: PlayerId, difficulty: f32, seed: u64) -> Self {
        let mut rng = SimRng::new(seed ^ (id.0 as u64).wrapping_mul(0x9E37_79B9));
        let cast_cd = rng.range(1.0, 4.0);
        let mut aim = AimProfile::for_difficulty(difficulty);
        // Per-bot personality: reaction varies ±15 % between bots of the same difficulty.
        aim.reaction *= rng.range(0.85, 1.15);
        Self {
            id,
            difficulty: difficulty.clamp(0.0, 1.0),
            casts_enabled: true,
            rng,
            words: word_list(),
            target: None,
            retarget: 0.0,
            strafe: 1.0,
            strafe_t: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            noise: (0.0, 0.0),
            noise_goal: (0.0, 0.0),
            noise_t: 0.0,
            aim,
            seen: [(Vec3::ZERO, Vec3::ZERO); SEEN_TICKS],
            seen_head: 0,
            seen_n: 0,
            seen_of: None,
            trail_k: 1.0,
            overshoot: (0.0, 0.0),
            overshoot_cd: 0.0,
            tether_t: 0.0,
            tether_dir: None,
            cast_cd,
            oath_plan: None,
            oath_section: 0,
            seq: 0,
            last_ground: Vec3::ZERO,
            hook: Default::default(),
            moves: Default::default(),
        }
    }

    /// Decide this tick's input and commands. Reads the world only.
    pub fn think(&mut self, w: &World) -> BotAction {
        let dt = DT;
        let mut act = BotAction::default();
        self.seq = self.seq.wrapping_add(1);
        act.input.seq = self.seq;
        let Some(me) = w.player(self.id) else { return act };
        if !me.alive {
            self.target = None;
            self.tether_dir = None;
            self.oath_plan = None;
            self.yaw = me.input.yaw;
            act.input.yaw = self.yaw;
            return act;
        }
        let params = w.params();
        let eye = me.eye(params);
        let pos = me.mv.pos;
        // Smash model: high damage % = fragile (big knockback) → play safe.
        let pct = me.damage_pct;
        // ---------------------------------------------------------------- oath
        if let OathState::Reciting { started, len, .. } = me.oath {
            act.input.buttons |= buttons::TYPING;
            act.input.yaw = self.yaw;
            act.input.pitch = self.pitch;
            // Keep drifting on the keys held when the recital began (like a player's latched move),
            // but let go rather than walk off a ledge.
            let (mx, mz) = (me.input.move_x, me.input.move_z);
            let (sy, cy) = self.yaw.sin_cos();
            let d = (Vec3::new(-sy, 0.0, -cy) * mz as f32 + Vec3::new(cy, 0.0, -sy) * mx as f32).normalize_or_zero();
            if d != Vec3::ZERO
                && [1.5f32, 3.0].iter().all(|k| raycast(&w.voxels, me.mv.pos + d * *k + Vec3::Y * 0.5, Vec3::NEG_Y, 3.5).is_some())
            {
                act.input.move_x = mx;
                act.input.move_z = mz;
            }
            if let Some((t0, secs, acc)) = self.oath_plan {
                if t0 == started && w.tick.saturating_sub(started) as f32 * DT >= secs {
                    act.finish_oath = Some((acc, secs));
                    self.oath_plan = None;
                }
            } else {
                // Lost track (e.g. interrupted and restarted): plan now.
                let cps = 4.0 + 6.0 * self.difficulty;
                self.oath_plan = Some((started, len as f32 / cps, 0.8 + 0.18 * self.difficulty));
            }
            return act;
        }
        // ---------------------------------------------------------------- target
        self.retarget -= dt;
        let valid = |id: PlayerId| w.player(id).is_some_and(|p| p.alive && p.id != self.id);
        if self.retarget <= 0.0 || !self.target.is_some_and(valid) {
            self.retarget = 1.0;
            let attacker = me.last_attacker.map(|a| a.0).filter(|a| valid(*a));
            self.target = attacker.or_else(|| {
                w.players
                    .iter()
                    .filter(|p| p.alive && p.id != self.id && !p.shielded())
                    // Nearest, but a high-% enemy (easy launch) counts as closer.
                    .min_by(|a, b| {
                        let score = |q: &crate::world::Player| (q.mv.pos - pos).length_squared() / (1.0 + q.damage_pct / 60.0);
                        score(a).total_cmp(&score(b))
                    })
                    .map(|p| p.id)
            });
        }
        let target = self.target.and_then(|t| w.player(t));
        // ---------------------------------------------------------------- aim (human-like)
        // Perceive late: remember the target every tick, aim from the sample `reaction` ago.
        if self.seen_of != self.target {
            self.seen_of = self.target;
            self.seen_n = 0;
            // A new target is a flick: it may overshoot.
            self.overshoot_cd = 0.0;
        }
        let lead_speed = self.lead_speed(w);
        let (aim_point, dist, omega) = match target {
            Some(t) => {
                let chest = t.hull(params).center();
                self.seen_head = (self.seen_head + 1) % SEEN_TICKS;
                self.seen[self.seen_head] = (chest, t.mv.vel);
                self.seen_n = (self.seen_n + 1).min(SEEN_TICKS);
                let delay = ((self.aim.reaction / dt).round() as usize).clamp(1, SEEN_TICKS - 1);
                let back = delay.min(self.seen_n - 1);
                let (c0, v0) = self.seen[(self.seen_head + SEEN_TICKS - back) % SEEN_TICKS];
                let d = (c0 - eye).length();
                // Partial extrapolation over the reaction delay, plus the round's flight time
                // (our rounds inherit our velocity: lead with the relative velocity).
                let rel = v0 - me.mv.vel;
                let flight = if lead_speed > 0.0 { d / lead_speed * (0.5 + 0.5 * self.difficulty) } else { 0.0 };
                let mut p = c0 + v0 * (back as f32 * dt * self.aim.predict) + rel * flight;
                // Tracking lag: the crosshair trails a target sweeping across the view.
                let los = (c0 - eye).normalize_or_zero();
                let lat = rel - los * rel.dot(los);
                p -= lat * (self.aim.tracking * self.trail_k);
                (p, (chest - eye).length(), lat.length() / d.max(1.0))
            }
            None => (w.arena.center + Vec3::Y * 2.0, (w.arena.center - pos).length(), 0.0),
        };
        // Gaussian-ish wobble (sum of three uniforms), growing with the target's angular speed;
        // the crosshair drifts toward each new sample instead of jumping.
        self.noise_t -= dt;
        if self.noise_t <= 0.0 {
            self.noise_t = self.rng.range(0.12, 0.25);
            let sigma = self.aim.jitter + 0.2 * omega * self.aim.tracking;
            let g = |r: &mut SimRng| (r.range(-1.0, 1.0) + r.range(-1.0, 1.0) + r.range(-1.0, 1.0)) * sigma;
            self.noise_goal = (g(&mut self.rng), g(&mut self.rng) * 0.7);
            self.trail_k = self.rng.range(0.2, 1.2);
        }
        let k = (12.0 * dt).min(1.0);
        self.noise.0 += (self.noise_goal.0 - self.noise.0) * k;
        self.noise.1 += (self.noise_goal.1 - self.noise.1) * k;
        let decay = (-dt / 0.12).exp();
        self.overshoot = (self.overshoot.0 * decay, self.overshoot.1 * decay);
        self.overshoot_cd = (self.overshoot_cd - dt).max(0.0);
        let to = (aim_point - eye).normalize_or_zero();
        let mut want_yaw = (-to.x).atan2(-to.z) + self.noise.0;
        let mut want_pitch = to.y.clamp(-1.0, 1.0).asin() + self.noise.1;
        if target.is_some() && self.overshoot_cd <= 0.0 {
            let (dy, dp) = (wrap_angle(want_yaw - self.yaw), want_pitch - self.pitch);
            if dy.abs() + dp.abs() > FLICK_ANGLE {
                self.overshoot_cd = 0.6;
                if self.rng.chance(self.aim.overshoot_chance) {
                    let f = self.aim.overshoot * self.rng.range(0.5, 1.2);
                    self.overshoot = (dy * f, dp * f);
                }
            }
        }
        want_yaw += self.overshoot.0;
        want_pitch += self.overshoot.1;
        // ---------------------------------------------------------------- movement plan
        let h_to = Vec3::new(aim_point.x - pos.x, 0.0, aim_point.z - pos.z);
        let h_dir = h_to.normalize_or_zero();
        self.strafe_t -= dt;
        if self.strafe_t <= 0.0 {
            self.strafe_t = self.rng.range(0.4, 1.4);
            self.strafe = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        }
        let low = pct > 110.0;
        let melee = w.constructs.held_by(self.id).is_some_and(|c| crate::constructs::form_spec(c.recipe.form).speed <= 0.0);
        let close_to = if melee { 2.0 } else { 9.0 };
        let mut move_z: i8 = if low { -1 } else if dist > close_to { 1 } else if dist < 4.0 && !melee { -1 } else { 0 };
        let mut move_x: i8 = if self.strafe > 0.0 { 1 } else { -1 };
        let mut b: u16 = 0;
        if me.mv.grounded {
            self.last_ground = pos;
        }
        // Bunny-hop at range, only toward solid ground (a hop carries ~9 m).
        let path_ok = [3.0f32, 6.0, 9.0].iter().all(|d| raycast(&w.voxels, pos + h_dir * *d + Vec3::Y * 0.5, Vec3::NEG_Y, 8.0).is_some());
        if dist > 12.0 && self.difficulty > 0.25 && !low && path_ok {
            b |= buttons::JUMP; // held = auto-hop; strafing while turning gains speed
        }
        let cw = CollisionWorld::new(&w.voxels, w.colliders());
        let ahead = pos + h_dir * 3.0 + Vec3::Y * 1.0;
        let gap = raycast(&w.voxels, ahead, Vec3::NEG_Y, 30.0).is_none();
        let ground_near = raycast(&w.voxels, pos + Vec3::Y * 0.5, Vec3::NEG_Y, 15.0).is_some();
        let falling = !me.mv.grounded && !ground_near && me.mv.vel.y < -6.0;
        self.tether_t = (self.tether_t - dt).max(0.0);
        // Only as a rescue: deliberate gap-crossing swings were a third of all bot KOs (self-falls)
        // under the Smash model. Bots fight across gaps with ranged blasts instead.
        let _ = gap;
        let want_tether = falling;
        if want_tether && self.tether_dir.is_none() && self.tether_t <= 0.0 {
            // Anchors above us: toward the goal first, then steeper.
            let goal = if falling { self.last_ground } else { aim_point };
            let hd = Vec3::new(goal.x - pos.x, 0.0, goal.z - pos.z).normalize_or_zero();
            let lifts: &[f32] = if falling { &[1.4, 2.5, 0.9, 5.0] } else { &[0.9, 1.4, 0.6] };
            for &lift in lifts {
                let dir = (hd + Vec3::Y * lift).normalize_or_zero();
                if let Some(q) = query_anchor(&cw, eye, dir, params).filter(|q| q.point.y > pos.y + 1.0) {
                    let _ = q;
                    self.tether_dir = Some(dir);
                    self.tether_t = 3.0;
                    break;
                }
            }
        }
        let mut tether_hold = false;
        if let Some(dir) = self.tether_dir {
            want_yaw = (-dir.x).atan2(-dir.z);
            want_pitch = dir.y.clamp(-1.0, 1.0).asin();
            b &= !buttons::JUMP;
            let hook = me.mv.hooks[1].phase;
            // Keep holding once fired; the press itself waits until the view is on the anchor.
            tether_hold = hook != crate::movement::HookPhase::Idle;
            move_z = if hook == crate::movement::HookPhase::Attached { 1 } else { 0 };
            if me.mv.grounded && !me.mv.hooks[1].attached() {
                move_x = 0;
            }
            if me.mv.hooks[1].attached() {
                let a = me.mv.hooks[1].pos;
                // Reached = beside it AND up at it (a cliff-edge anchor right above a falling bot is
                // not "reached": letting go there drops it into the void).
                let close = Vec3::new(a.x - pos.x, 0.0, a.z - pos.z).length() < 2.0 && pos.y > a.y - 1.5;
                // BOUNDS HOOK (world/bounds.rs): hanging past a blast zone (or just over the void) the
                // rope is all that keeps us in: hold on and reel until we're back in.
                let hanging = !w.blast_zones().contains(pos) || pos.y < w.arena.void_y + 4.0;
                if hanging {
                    self.tether_t = self.tether_t.max(0.5);
                }
                // Let go once solid ground is under us (or we reached the anchor).
                if !hanging && ((ground_near && !falling && self.tether_t < 2.6) || close || me.mv.grounded && self.tether_t < 2.5) {
                    self.tether_dir = None;
                    tether_hold = false;
                }
            }
            if self.tether_t <= 0.0 {
                self.tether_dir = None;
            }
        } else if !me.mv.grounded && !ground_near {
            // Airborne over a drop without a rope: steer back toward the last solid ground.
            b &= !buttons::JUMP;
            let back = Vec3::new(self.last_ground.x - pos.x, 0.0, self.last_ground.z - pos.z).normalize_or_zero();
            let (sy, cy) = self.yaw.sin_cos();
            let fwd = Vec3::new(-sy, 0.0, -cy);
            let right = Vec3::new(cy, 0.0, -sy);
            let (f, r) = (back.dot(fwd), back.dot(right));
            move_z = if f > 0.3 { 1 } else if f < -0.3 { -1 } else { 0 };
            move_x = if r > 0.3 { 1 } else if r < -0.3 { -1 } else { 0 };
        }
        // Edge safety: never walk / hop off a ledge (unless on a rope).
        if self.tether_dir.is_none() && me.mv.grounded {
            let (sy, cy) = self.yaw.sin_cos();
            let fwd = Vec3::new(-sy, 0.0, -cy);
            let right = Vec3::new(cy, 0.0, -sy);
            let safe = |mx: i8, mz: i8| {
                let d = (fwd * mz as f32 + right * mx as f32).normalize_or_zero();
                d == Vec3::ZERO
                    || [1.5f32, 3.0].iter().all(|k| raycast(&w.voxels, pos + d * *k + Vec3::Y * 0.5, Vec3::NEG_Y, 3.5).is_some())
            };
            if !safe(move_x, move_z) {
                if safe(0, move_z) {
                    move_x = 0;
                } else if safe(move_x, 0) {
                    move_z = 0;
                } else if safe(-move_x, -move_z) {
                    move_x = -move_x;
                    move_z = -move_z;
                } else {
                    move_x = 0;
                    move_z = 0;
                }
                b &= !buttons::JUMP;
            }
        }
        // ASSIST HOOK: now and then, hook-and-strike a visible target with a melee weapon in hand.
        if let Some(at) = crate::hook::bot_hook_plan(&mut self.hook, w, me, target, eye, self.tether_dir.is_none(), &mut self.rng) {
            let d = (at - eye).normalize_or_zero();
            want_yaw = (-d.x).atan2(-d.z);
            want_pitch = d.y.clamp(-1.0, 1.0).asin();
            b &= !buttons::JUMP;
            move_z = 1;
        }
        // ---------------------------------------------------------------- turn (reaction-limited)
        let turn = self.aim.turn_rate * dt;
        let dy = wrap_angle(want_yaw - self.yaw);
        self.yaw = wrap_angle(self.yaw + dy.clamp(-turn, turn));
        self.pitch = (self.pitch + (want_pitch - self.pitch).clamp(-turn, turn)).clamp(-1.5, 1.5);
        let aim_err = wrap_angle(want_yaw - self.yaw).abs() + (want_pitch - self.pitch).abs();
        if self.tether_dir.is_some() && (tether_hold || aim_err < 0.05) {
            b |= buttons::TETHER_R;
            if me.mv.hooks[1].attached() {
                b |= buttons::REEL;
            }
        }
        b |= crate::hook::bot_hook_buttons(&mut self.hook, me, aim_err); // ASSIST HOOK
        // DODGE HOOK: air jump (recovery / chase) and dodging telegraphed attacks.
        let roping = self.tether_dir.is_some();
        let (moves, rng) = (&mut self.moves, &mut self.rng);
        crate::dodge::bot_moves(moves, w, me, target, self.last_ground, roping, self.difficulty, self.yaw, rng, &mut b, &mut move_x, &mut move_z);
        // ---------------------------------------------------------------- attack
        let held = w.constructs.held_by(self.id);
        let tethering = self.tether_dir.is_some();
        if let Some(t) = target {
            let visible = raycast(&w.voxels, eye, aim_point - eye, dist).is_none();
            // No trigger before the target has been seen for a reaction time.
            let reacted = self.seen_n as f32 * dt >= self.aim.reaction;
            if !tethering && visible && reacted && aim_err < 0.12 && !t.shielded() {
                match held {
                    Some(c) if crate::constructs::form_spec(c.recipe.form).speed <= 0.0 => {
                        if dist < 3.2 + c.half.z * 2.0 {
                            // Tap to swing (release every other tick so presses register).
                            if self.seq % 20 < 10 {
                                b |= buttons::PRIMARY;
                            }
                        }
                    }
                    _ => {
                        if dist < 70.0 {
                            b |= buttons::PRIMARY;
                        }
                    }
                }
            }
            // Shove to defend when fragile, or to push a hurt enemy off a ledge (edge = no ground
            // a few metres behind them, seen from us).
            let away = Vec3::new(t.mv.pos.x - pos.x, 0.0, t.mv.pos.z - pos.z).normalize_or_zero();
            let edge = raycast(&w.voxels, t.mv.pos + away * 3.0 + Vec3::Y * 0.5, Vec3::NEG_Y, 12.0).is_none();
            if dist < 3.5 && (pct > 60.0 || edge || self.rng.chance(0.02)) {
                b |= buttons::SECONDARY;
            }
        }
        act.input.yaw = self.yaw;
        act.input.pitch = self.pitch;
        act.input.move_x = move_x;
        act.input.move_z = move_z;
        act.input.buttons = b;
        // ---------------------------------------------------------------- casts / oath / dismiss
        self.cast_cd -= dt;
        let wp = me.willpower;
        let near = w.players.iter().filter(|p| p.alive && p.id != self.id).map(|p| (p.mv.pos - pos).length()).fold(f32::INFINITY, f32::min);
        // CHARGE HOOK: power up when low and safe (crate::charge).
        if crate::charge::bot_wants_charge(w, me, near, &mut self.rng) {
            act.input = crate::charge::bot_charge_input(act.input);
            return act;
        }
        if wp < 20.0 && near > 18.0 && me.mv.grounded && !w.rules().infinite_willpower() {
            let section = w.cfg.oaths.valid_section(me.corps, self.oath_section);
            self.oath_section = (section + 1) % w.cfg.oaths.section_count(me.corps);
            let len = w.cfg.oaths.section_len(me.corps, section);
            let cps = 4.0 + 6.0 * self.difficulty;
            self.oath_plan = Some((w.tick, len as f32 / cps, 0.8 + 0.18 * self.difficulty));
            act.begin_oath = Some(section);
            return act;
        }
        if wp < 6.0 && w.constructs.owned_by(self.id).count() > 1 && self.rng.chance(0.01) {
            act.dismiss = true;
        }
        if self.casts_enabled && self.cast_cd <= 0.0 && !tethering {
            self.cast_cd = self.rng.range(3.0, 7.0) * (1.5 - self.difficulty * 0.7);
            let group = if low {
                if self.rng.chance(0.5) { &self.words.defence } else { &self.words.mobility }
            } else if dist < 8.0 {
                &self.words.melee
            } else if dist < 30.0 {
                match self.rng.below(3) {
                    0 => &self.words.summon,
                    1 => &self.words.thrown,
                    _ => &self.words.ranged,
                }
            } else if self.rng.chance(0.5) {
                &self.words.ranged
            } else {
                &self.words.mobility
            };
            if !group.is_empty() {
                let r = group[self.rng.below(group.len() as u32) as usize].clone();
                if base_cost(&r) + 10.0 < wp {
                    let look = bd_core::input::look_dir(self.yaw, self.pitch);
                    let mut aim = Aim::new(eye, look);
                    aim.target_hint = self.target.map(TargetHint::Player);
                    act.cast = Some((r, aim));
                }
            }
        }
        act
    }
}

impl BotBrain {
    /// This bot's aim numbers (after its personal reaction jitter).
    pub fn aim_profile(&self) -> AimProfile {
        self.aim
    }

    /// Speed of the round the bot's weapon fires (for lead): the ring blast, a held projectile
    /// gun's muzzle speed, or 0 for hitscan / melee (no lead).
    fn lead_speed(&self, w: &World) -> f32 {
        match w.constructs.held_by(self.id) {
            None => w.cfg.tuning.ring.blast_speed,
            Some(c) => match crate::weapons::archetype_for(&c.recipe) {
                Some(a) => {
                    let spec = w.cfg.tuning.spec_for(&c.recipe, a);
                    if crate::constructs::gun::fires_hitscan(c, a, &spec) { 0.0 } else { spec.speed }
                }
                None => 0.0,
            },
        }
    }
}

fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a.rem_euclid(tau);
    if a > std::f32::consts::PI {
        a -= tau;
    }
    a
}

/// Run every bot for one tick: think, apply their commands to the world (casts, oath, dismiss),
/// and append their inputs to `inputs`. Deterministic (bots in slice order).
pub fn drive(world: &mut World, bots: &mut [BotBrain], inputs: &mut Vec<(PlayerId, PlayerInput)>) {
    for bot in bots.iter_mut() {
        let act = bot.think(world);
        if let Some(section) = act.begin_oath {
            world.begin_oath(bot.id, section);
        }
        if let Some((acc, secs)) = act.finish_oath {
            let _ = world.oath_result(bot.id, acc, secs);
        }
        if act.dismiss {
            world.dismiss(bot.id);
        }
        if let Some((r, aim)) = act.cast {
            // SIGNATURE HOOK: with a full-ish bar, sometimes shout it as a signature (crate::signature).
            let _ = crate::signature::bot_cast(world, bot.id, r, aim);
        }
        // SIGNATURE HOOK: sometimes answer a palm beam coming at it with one (a beam struggle).
        crate::signature::palm::bot_counter(world, bot.id);
        // Party mode: pick a card on the pick screen.
        if let Some(ix) = party_pick(world, bot) {
            let _ = world.party_choose(bot.id, ix);
        }
        inputs.push((bot.id, act.input));
    }
}

/// A bot's party pick (COMBAT.md §5g), once it has "read the cards" (1–2.5 s into the pick).
/// Simple taste: gambles on Boom or Bust when it is not winning and feeling brave (difficulty),
/// otherwise prefers effects that make its constructs act on their own (homing, flying, walking,
/// shooting, companions), then elements, then anything — ties broken by a per-bot seeded hash.
pub fn party_pick(world: &World, bot: &BotBrain) -> Option<u8> {
    use crate::party::{PartyCard, PartyPhase};
    let party = world.party()?;
    if party.phase != PartyPhase::Pick {
        return None;
    }
    let me = party.player(bot.id)?;
    if me.picked.is_some() || me.offer.is_empty() {
        return None;
    }
    let think = 1.0 + 0.5 * (bot.id.0 % 4) as f32;
    if party.phase_time < think.min(party.settings.pick_secs * 0.5) {
        return None;
    }
    let score = |card: PartyCard| -> u32 {
        match card {
            PartyCard::BoomOrBust => {
                if bot.difficulty < 0.6 { 90 } else { 10 }
            }
            PartyCard::Effect(e) => {
                let Some(spec) = party.table.get(e) else { return 0 };
                let g = &spec.graft;
                let active = [Mechanic::Homing, Mechanic::Flies, Mechanic::Walks, Mechanic::Shoots, Mechanic::Charge, Mechanic::Breath];
                let mut s = if g.mechanics.iter().any(|m| active.contains(m)) { 60 } else if !spec.hooks.is_empty() { 50 } else if g.element.is_some() { 40 } else { 20 };
                s += (crate::rng::word_hash(&spec.id) ^ bot.id.0 as u64).wrapping_mul(0x9E37_79B9) as u32 % 17;
                s
            }
        }
    };
    (0..me.offer.n).filter_map(|i| me.offer.card(i).map(|c| (i, score(c)))).max_by_key(|&(i, s)| (s, u8::MAX - i)).map(|(i, _)| i)
}

/// Add `n` bots (ids `first..first+n`, corps by id) and return their brains.
pub fn add_bots(world: &mut World, first: u8, n: u8, difficulty: f32, seed: u64) -> Vec<BotBrain> {
    let mut v = Vec::new();
    for k in 0..n {
        let id = PlayerId(first.saturating_add(k));
        if world.add_player(id, format!("bot{}", id.0), Corps::from_index(id.0)).is_ok() {
            v.push(BotBrain::new(id, difficulty, seed));
        }
    }
    v
}
