//! Role × semantic-event behaviours: the procedural "premade" feel of every construct.
//!
//! A construct's motion is the **union of its parts' behaviours reacting to the same events** — a
//! tank (wheels + turret + barrel + hatch) rolls, steers, aims, recoils and opens for its pilot
//! without anyone authoring "tank"; a dragon with a gun on its back aims the gun *and* looks at you.
//! Roles come from [`crate::roles`]; events are derived client-side from replicated state (no new
//! network traffic): see [`AnimEvent`] / [`AnimDrive`].
//!
//! | role | behaviour |
//! |---|---|
//! | Turret | yaws toward the aim with weight-scaled lag + slew limit; idle scan; shudders on fire |
//! | Barrel | pitches toward the aim; recoils back along its length on fire and springs home |
//! | Wheel | (rolls via the machine kit / rig) front wheels steer into turns |
//! | Rotor | spins up on summon, spins with throttle / charge (reshapes the machine spin) |
//! | Hatch | opens on pilot approach, stays open while they climb in, clunks shut (bounce); reopens on exit |
//! | Piston | pumps on fire |
//! | Fin | flexes into turns, flutters with speed |
//! | Thruster / Exhaust | flare with throttle, acceleration and charge; exhaust puffs on acceleration |
//! | Head | looks at the nearest enemy / aim point (lags by weight); flinches on hits |
//! | Jaw | opens on attack roars and hits; roars at the end of the summon |
//! | Eye | blinks (pseudo-random, deterministic per construct); lights up at the end of the summon |
//! | Blade | slides out of its hilt on summon; flicks longer on attack |
//! | root | engine rumble (machines), hard-light hum (structures), hit flinch, fire kick, pilot dip |
//!
//! **Summon assembly** (visual only — the construct is live in the sim from its first tick): parts
//! stream out of the ring along the beam in role order (core → limbs/wheels → turret/head/wings →
//! barrels/blades/jaw → lock: hatches clunk shut, eyes light), arrive a voxel above/outside their
//! slot and snap on like a brick with a squash-and-settle; stages overlap; the whole assembly takes
//! 0.3–0.6 s by size. A click cue per stage ([`CueKind::AssembleClick`], pitch rising) and
//! [`CueKind::AssembleLock`] on the last. **Dismiss** plays the collapse in reverse.
//!
//! Mass scaling (`VoxelModel::weight`, `forms::weight_of`): heavy = slower springs, deeper recoil /
//! squash / slam, more overshoot, less flinch; light = snappy.
//!
//! Allocation happens only in [`ConstructAnimator::new`]; [`ConstructAnimator::update`] integrates
//! fixed 1/120 s substeps (deterministic, frame-rate independent) and never allocates. All outputs
//! are finite and bounded (tested).

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec3};

use crate::anim::{Gait, Local, about};
use crate::ease::{Ease, smoothstep, spring_impulse};
use crate::roles::{self, Role, Roles};
use crate::voxelize::VoxelModel;

const STEP: f32 = 1.0 / 120.0;
/// Longest a shout may hold back the assembly's later stages (s): the core is up at once, so the
/// silhouette stays close to final.
pub const MAX_STAGE_DELAY: f32 = 0.45;
const MAX_CUES: usize = 8;

// ------------------------------------------------------------------------------------ inputs

/// Per-frame inputs, derived client-side from replicated state (model space = the construct's
/// own frame: +Y up, −Z forward, metres).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimDrive {
    /// Construct clock (s) — the same `t` the pose layers use (machine spins are reshaped against it).
    pub time: f32,
    /// Ground speed (m/s).
    pub speed: f32,
    /// Yaw rate (rad/s, + = turning left).
    pub turn: f32,
    pub airborne: bool,
    /// Unit aim direction for turrets / barrels (pilot's look, else the nearest enemy).
    pub aim: Option<Vec3>,
    /// Point heads look at (nearest enemy / aim point).
    pub look: Option<Vec3>,
    /// The local player is next to and looking at this pilotable construct.
    pub pilot_near: bool,
    /// Someone is seated.
    pub piloted: bool,
    /// Owner is charging (power-up), 0..1.
    pub charge: f32,
    /// The caster's ring hand (beam source for assembly / dismiss), if known.
    pub ring: Option<Vec3>,
}

/// Semantic moments (client-side, from sim events / replicated flags).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimEvent {
    /// The construct was just cast: play the assembly.
    Summon,
    /// A shot / blast left this construct (`strength` ≈ 1 normal, 2+ huge).
    Fire {
        strength: f32,
    },
    /// Melee / creature attack telegraph (`roar`: open the jaw).
    Attack {
        roar: bool,
    },
    /// Took a hit travelling along `dir` (model space), `strength` 0..~2.
    Hit {
        dir: Vec3,
        strength: f32,
    },
    PilotEnter,
    PilotExit,
    /// Dismissed: collapse back into the ring over `duration` s.
    Dismiss {
        duration: f32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CueKind {
    /// A brick snapped on (`stage` of `stages`; pitch rises with the stage).
    #[default]
    AssembleClick,
    /// Final stage: hatch clunk / eyes light — the construct is "locked".
    AssembleLock,
    HatchOpen,
    HatchClose,
    /// Exhaust / thruster puff on acceleration (`pos`/`dir` model space, `strength` 0..1).
    ExhaustPuff,
    /// Rotor spin-up began (summon / take-off).
    RotorSpinUp,
}

/// A sound / particle hook for the client (model space).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cue {
    pub kind: CueKind,
    pub stage: u8,
    pub stages: u8,
    pub pos: Vec3,
    pub dir: Vec3,
    pub strength: f32,
}

/// Assembly progress (for the shout-words hook and HUD-free feedback).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Assembly {
    pub active: bool,
    /// 0..1 over the whole assembly.
    pub progress: f32,
    /// Stage landing next / number of stages.
    pub stage: u8,
    pub stages: u8,
    /// The next brick to land: (model-space slot centre, seconds until it lands).
    pub next: Option<(Vec3, f32)>,
}

// ----------------------------------------------------------------------------------- springs

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Spring {
    x: f32,
    v: f32,
}

impl Spring {
    #[inline]
    fn step(&mut self, target: f32, hz: f32, zeta: f32, dt: f32) {
        let w = TAU * hz.clamp(0.05, 30.0);
        let a = (target - self.x) * w * w - 2.0 * zeta * w * self.v;
        self.v = (self.v + a * dt).clamp(-400.0, 400.0);
        self.x = (self.x + self.v * dt).clamp(-50.0, 50.0);
        if !(self.x.is_finite() && self.v.is_finite()) {
            *self = Spring::default();
        }
    }
}

fn wrap_pi(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

fn hash01(seed: u32, k: u32) -> f32 {
    let mut x = seed ^ k.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / 16_777_216.0
}

fn fin(x: f32, d: f32) -> f32 {
    if x.is_finite() { x } else { d }
}

fn fin3(v: Vec3) -> Vec3 {
    if v.is_finite() { v } else { Vec3::ZERO }
}

// ---------------------------------------------------------------------------------- animator

/// Per-construct role-behaviour state (client-only, cosmetic). See module docs.
#[derive(Clone, Debug)]
pub struct ConstructAnimator {
    pub roles: Roles,
    locals: Vec<Local>,
    /// Primary / secondary spring per part (meaning depends on the role).
    s1: Vec<Spring>,
    s2: Vec<Spring>,
    /// Integrated rotor angle per part (rad, wrapped).
    ang: Vec<f32>,
    /// Pivot per part (m) and parent index.
    pivot: Vec<Vec3>,
    parent: Vec<usize>,
    // Root springs: flinch about X / Z, dip (Y), fire kick.
    flinch_x: Spring,
    flinch_z: Spring,
    dip: Spring,
    kick: Spring,
    rotor_k: Spring,
    flare: Spring,
    // Mass/size scaling.
    weight: f32,
    hz_k: f32,
    zeta_k: f32,
    height: f32,
    vox: f32,
    gait: Gait,
    // Clocks.
    carry: f32,
    clock: f32,
    summon: Option<f32>,
    dismiss: Option<(f32, f32)>,
    attack: Option<(f32, bool)>,
    hit_t: f32,
    enter_hold: f32,
    exit_hold: f32,
    hatch_open: bool,
    hatch_closing: bool,
    blink_t: f32,
    blink_n: u32,
    blink_next: f32,
    puff_cd: f32,
    prev_speed: f32,
    seed: u32,
    // Assembly timing.
    asm_total: f32,
    /// Extra wait before stages ≥ 1 (a shout's letters become those bricks; see [Self::hold_stages]).
    stage_delay: f32,
    asm_flight: f32,
    asm_snap: f32,
    asm_step: f32,
    stage_center: [Vec3; 5],
    stage_radius: [f32; 5],
    stage_landed: u8,
    has_jaw: bool,
    drive: AnimDrive,
    cues: [Cue; MAX_CUES],
    ncue: usize,
}

impl ConstructAnimator {
    /// Analyse `model` (allocates once). `seed` varies blink timing / idle scans per construct.
    pub fn new(model: &VoxelModel, seed: u32) -> Self {
        let roles = roles::analyze(model);
        let n = model.parts.len();
        let w = if model.weight.is_finite() { model.weight.clamp(0.5, 2.5) } else { 1.0 };
        let height = (model.bounds.max.y - model.bounds.min.y).max(0.25);
        let size = roles.size;
        let size_k = (size / 2.0).clamp(0.5, 20.0).powf(0.25);
        // Assembly timing: 0.3–0.6 s by size; heavier bricks settle a touch slower.
        let total = (0.28 + 0.1 * size.max(0.1).sqrt()).clamp(0.3, 0.6);
        let sf = smoothstep(1.0, 8.0, size);
        let snap = ((0.06 + 0.03 * sf) * w.powf(0.25)).clamp(0.06, 0.09);
        let flight = 0.08 + 0.05 * sf;
        let stages = roles.stages.max(1);
        let step = if stages > 1 { ((total - flight - snap) / (stages - 1) as f32).max(0.03) } else { 0.0 };
        let mut stage_center = [Vec3::ZERO; 5];
        let mut counts = [0u32; 5];
        for p in &roles.parts {
            let s = (p.stage as usize).min(4);
            stage_center[s] += p.center;
            counts[s] += 1;
        }
        for s in 0..5 {
            if counts[s] > 0 {
                stage_center[s] /= counts[s] as f32;
            }
        }
        // How far a stage's bricks spread around its centroid (targets for streamed voxels).
        let mut stage_radius = [0.0f32; 5];
        for (p, m) in roles.parts.iter().zip(&model.parts) {
            if m.voxels.is_empty() {
                continue;
            }
            let s = (p.stage as usize).min(4);
            let b = m.aabb();
            let r = stage_center[s].distance(p.center) + 0.35 * (b.max - b.min).max_element();
            stage_radius[s] = stage_radius[s].max(r);
        }
        // A stage with no bricks of its own (the lock stage of a hatch-only model): the whole body.
        for s in 0..5 {
            if counts[s] == 0 {
                stage_center[s] = roles.center;
                stage_radius[s] = 0.3 * roles.size;
            }
        }
        let has_jaw = roles.parts.iter().any(|p| p.role == Role::Jaw);
        ConstructAnimator {
            locals: vec![Local::IDENTITY; n],
            s1: vec![Spring::default(); n],
            s2: vec![Spring::default(); n],
            ang: vec![0.0; n],
            pivot: model.parts.iter().map(|p| p.pivot).collect(),
            parent: model.parts.iter().map(|p| p.parent_index).collect(),
            flinch_x: Spring::default(),
            flinch_z: Spring::default(),
            dip: Spring::default(),
            kick: Spring::default(),
            rotor_k: Spring { x: 1.0, v: 0.0 },
            flare: Spring::default(),
            weight: w,
            hz_k: 1.0 / (w.sqrt() * size_k),
            zeta_k: w.powf(-0.35),
            height,
            vox: model.voxel_size.max(0.05),
            gait: model.rig.gait,
            carry: 0.0,
            clock: 0.0,
            summon: None,
            dismiss: None,
            attack: None,
            hit_t: 10.0,
            enter_hold: 0.0,
            exit_hold: 0.0,
            hatch_open: false,
            hatch_closing: false,
            blink_t: 10.0,
            blink_n: 0,
            blink_next: 1.5 + 3.0 * hash01(seed, 1),
            puff_cd: 0.0,
            prev_speed: 0.0,
            seed,
            asm_total: total,
            stage_delay: 0.0,
            asm_flight: flight,
            asm_snap: snap,
            asm_step: step,
            stage_center,
            stage_radius,
            stage_landed: 0,
            has_jaw,
            drive: AnimDrive::default(),
            cues: [Cue::default(); MAX_CUES],
            ncue: 0,
            roles,
        }
    }

    /// Per-part deltas to pass to `anim::pose_layered_with` (index-aligned with the model's parts).
    pub fn locals(&self) -> &[Local] {
        &self.locals
    }

    /// Cues emitted by the last [`Self::update`] (and events since).
    pub fn cues(&self) -> &[Cue] {
        &self.cues[..self.ncue]
    }

    /// Seconds the summon assembly takes for this model.
    pub fn assembly_seconds(&self) -> f32 {
        self.asm_total + self.stage_delay
    }

    fn cue(&mut self, c: Cue) {
        if self.ncue < MAX_CUES {
            self.cues[self.ncue] = c;
            self.ncue += 1;
        }
    }

    /// Feed a semantic event (takes effect immediately; springs get their kicks here).
    pub fn event(&mut self, e: AnimEvent) {
        let w = self.weight;
        match e {
            AnimEvent::Summon => {
                self.summon = Some(0.0);
                self.stage_landed = 0;
                // Rotors start from rest and spin up; hatches start open (they shut at the lock).
                self.rotor_k = Spring::default();
                if self.roles.count(|r| matches!(r, Role::Rotor { .. })) > 0 {
                    self.cue(Cue { kind: CueKind::RotorSpinUp, pos: self.roles.center, strength: 1.0, ..Default::default() });
                }
                for i in 0..self.s1.len() {
                    if matches!(self.roles.role(i), Role::Hatch { .. }) {
                        self.s1[i] = Spring { x: 1.0, v: 0.0 };
                    }
                }
            }
            AnimEvent::Fire { strength } => {
                let s = fin(strength, 1.0).clamp(0.2, 3.0);
                for i in 0..self.s1.len() {
                    match self.roles.role(i) {
                        Role::Barrel { len } => {
                            // Recoil: kick back along +Z; deeper for heavy guns.
                            self.s2[i].v += s * (len.clamp(0.2, 6.0) * 2.2) * w.powf(0.3);
                            self.s1[i].v += 0.9 * s / w.sqrt();
                        }
                        Role::Turret => self.s1[i].v += 0.35 * s * (hash01(self.seed, self.blink_n) - 0.5),
                        Role::Piston { .. } => self.s1[i].v += 7.0 * s,
                        Role::Thruster { .. } | Role::Exhaust { .. } => self.flare.v += 3.0 * s,
                        _ => {}
                    }
                }
                self.kick.v += 1.2 * s / w.powf(0.7);
            }
            AnimEvent::Attack { roar } => {
                self.attack = Some((0.0, roar));
                for i in 0..self.s1.len() {
                    if matches!(self.roles.role(i), Role::Blade { .. }) {
                        self.s1[i].v += 3.0;
                    }
                }
            }
            AnimEvent::Hit { dir, strength } => {
                let s = fin(strength, 0.5).clamp(0.0, 2.0) / w.powf(0.7);
                let d = Vec3::new(fin3(dir).x, 0.0, fin3(dir).z).normalize_or(Vec3::Z);
                // Tilt away from the hit: about X for a hit along Z, about Z for a hit along X.
                self.flinch_x.v += d.z * 5.0 * s;
                self.flinch_z.v -= d.x * 5.0 * s;
                self.hit_t = 0.0;
                for i in 0..self.s1.len() {
                    match self.roles.role(i) {
                        Role::Head => self.s2[i].v -= 4.0 * s,
                        Role::Jaw => self.s1[i].v += 6.0 * s,
                        _ => {}
                    }
                }
                self.blink_t = 0.0;
            }
            AnimEvent::PilotEnter => {
                self.enter_hold = 0.35;
                self.exit_hold = 0.0;
                self.dip.v -= 1.6 / w.sqrt();
            }
            AnimEvent::PilotExit => {
                self.exit_hold = 1.1;
                self.enter_hold = 0.0;
                self.dip.v += 0.8 / w.sqrt();
            }
            AnimEvent::Dismiss { duration } => {
                self.dismiss = Some((0.0, fin(duration, 0.4).clamp(0.1, 2.0)));
                self.summon = None;
            }
        }
    }

    /// Current assembly progress (for hooks).
    pub fn assembly(&self) -> Assembly {
        let Some(t) = self.summon else { return Assembly::default() };
        let stages = self.roles.stages.max(1);
        let mut next = None;
        let mut stage = stages;
        for k in 0..stages {
            let land = self.land_time(k);
            if land > t {
                stage = k;
                next = Some((self.stage_center[(k as usize).min(4)], land - t));
                break;
            }
        }
        let total = self.asm_total + self.stage_delay;
        Assembly { active: t < total, progress: (t / total).clamp(0.0, 1.0), stage, stages, next }
    }

    /// Per-stage schedule (model space): `(slot centroid, seconds until it lands, spread radius)`
    /// for stages `..roles.stages` (unused entries are zero). Seconds are relative to now (≤ 0 =
    /// landed); all `f32::MIN` when no assembly is playing.
    pub fn stage_schedule(&self) -> [(Vec3, f32, f32); 5] {
        let mut out = [(Vec3::ZERO, f32::MIN, 0.0); 5];
        let Some(t) = self.summon else { return out };
        for (k, o) in out.iter_mut().enumerate().take(self.roles.stages.min(5) as usize) {
            *o = (self.stage_center[k], self.land_time(k as u8) - t, self.stage_radius[k]);
        }
        out
    }

    /// Push the not-yet-started stages (≥ 1) back by `secs` more (total wait capped at
    /// [`MAX_STAGE_DELAY`]) — a shout is about to shatter and its letters should arrive *as* those
    /// bricks. The core (stage 0, the bulk of the silhouette) is never delayed, and nothing already
    /// on screen is taken back: once stage 1 has started, further requests are ignored.
    pub fn hold_stages(&mut self, secs: f32) {
        let Some(t) = self.summon else { return };
        if !(secs.is_finite() && secs > 0.0) || self.roles.stages < 2 || t >= self.stage_start(1) {
            return;
        }
        self.stage_delay = (self.stage_delay + secs).min(MAX_STAGE_DELAY);
    }

    fn stage_start(&self, k: u8) -> f32 {
        k as f32 * self.asm_step + if k >= 1 { self.stage_delay } else { 0.0 }
    }

    /// When stage `k`'s bricks touch their slots.
    fn land_time(&self, k: u8) -> f32 {
        if k == 0 { self.asm_snap * 0.6 } else { self.stage_start(k) + self.asm_flight }
    }

    /// Advance by `dt` seconds with this frame's inputs; recompute [`Self::locals`] and cues.
    pub fn update(&mut self, drive: &AnimDrive, dt: f32) {
        self.ncue = 0;
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        let mut d = *drive;
        d.time = fin(d.time, 0.0).clamp(-1e6, 1e6);
        d.speed = fin(d.speed, 0.0).clamp(0.0, 200.0);
        d.turn = fin(d.turn, 0.0).clamp(-20.0, 20.0);
        d.charge = fin(d.charge, 0.0).clamp(0.0, 1.0);
        d.aim = d.aim.filter(|a| a.is_finite() && a.length_squared() > 1e-6).map(|a| a.normalize());
        d.look = d.look.filter(|a| a.is_finite());
        d.ring = d.ring.filter(|a| a.is_finite());
        let accel = if dt > 0.0 { (d.speed - self.prev_speed) / dt } else { 0.0 };
        self.prev_speed = d.speed;
        self.drive = d;

        // Hatch intent (pilot approach / climb-in hold / exit hold; assembly keeps it open until lock).
        self.enter_hold = (self.enter_hold - dt).max(0.0);
        self.exit_hold = (self.exit_hold - dt).max(0.0);
        let lock_t = self.land_time(self.roles.stages.saturating_sub(1));
        let assembling_open = self.summon.is_some_and(|t| t < lock_t);
        let dismissing = self.dismiss.is_some();
        let want_open = assembling_open || dismissing || (d.pilot_near && !d.piloted) || self.enter_hold > 0.0 || self.exit_hold > 0.0;
        if self.roles.has_hatch && want_open != self.hatch_open {
            self.hatch_open = want_open;
            self.hatch_closing = !want_open;
            if want_open && !assembling_open {
                self.cue(Cue { kind: CueKind::HatchOpen, pos: self.roles.center, strength: 1.0, ..Default::default() });
            }
        }

        // Timers.
        if let Some(t) = self.summon.as_mut() {
            *t += dt;
        }
        if let Some((t, _)) = self.dismiss.as_mut() {
            *t += dt;
        }
        if let Some((t, _)) = self.attack.as_mut() {
            *t += dt;
            if *t > 0.9 {
                self.attack = None;
            }
        }
        self.hit_t += dt;
        self.blink_t += dt;
        self.puff_cd = (self.puff_cd - dt).max(0.0);
        if self.blink_t > self.blink_next {
            self.blink_t = 0.0;
            self.blink_n = self.blink_n.wrapping_add(1);
            self.blink_next = 2.2 + 3.3 * hash01(self.seed, self.blink_n);
        }

        // Assembly cues: one click per stage as its bricks land; the last one locks.
        if let Some(t) = self.summon {
            let stages = self.roles.stages.max(1);
            for k in 0..stages.min(8) {
                let bit = 1u8 << k;
                if self.stage_landed & bit == 0 && t >= self.land_time(k) {
                    self.stage_landed |= bit;
                    let last = k + 1 == stages && stages > 1;
                    let kind = if last { CueKind::AssembleLock } else { CueKind::AssembleClick };
                    self.cue(Cue {
                        kind,
                        stage: k,
                        stages,
                        pos: self.stage_center[(k as usize).min(4)],
                        strength: (self.roles.size / 3.0).clamp(0.4, 2.0),
                        ..Default::default()
                    });
                    if last && self.has_jaw {
                        self.attack = Some((0.0, true)); // a roar as the eyes light
                    }
                }
            }
            if t > self.asm_total + self.stage_delay + 0.3 {
                self.summon = None;
            }
        }

        // Exhaust puffs on acceleration.
        if accel > 3.0 && self.puff_cd <= 0.0 && d.speed > 0.5 {
            self.puff_cd = 0.18;
            for i in 0..self.s1.len() {
                if let Role::Exhaust { dir } | Role::Thruster { dir } = self.roles.role(i) {
                    let pos = self.roles.parts[i].center;
                    self.cue(Cue { kind: CueKind::ExhaustPuff, pos, dir, strength: smoothstep(3.0, 15.0, accel), ..Default::default() });
                }
            }
        }

        // Fixed substeps.
        self.carry += dt;
        let mut n = 0;
        while self.carry >= STEP && n < 16 {
            self.carry -= STEP;
            n += 1;
            self.substep(&d, accel);
        }
        if n == 16 {
            self.carry = 0.0;
        }
        self.clock += dt;
        self.compose();
    }

    fn throttle(&self) -> f32 {
        let d = &self.drive;
        let mut th = smoothstep(0.0, 12.0, d.speed);
        if d.piloted {
            th = th.max(0.35);
        }
        if d.airborne {
            th = th.max(0.8);
        }
        th.max(d.charge * 0.8)
    }

    fn substep(&mut self, d: &AnimDrive, accel: f32) {
        let dt = STEP;
        let (hk, zk) = (self.hz_k, self.zeta_k);
        let th = self.throttle();
        let t = d.time;
        // Root springs.
        self.flinch_x.step(0.0, 3.2 * hk, 0.32 * zk, dt);
        self.flinch_z.step(0.0, 3.2 * hk, 0.32 * zk, dt);
        self.dip.step(0.0, 2.6 * hk, 0.35 * zk, dt);
        self.kick.step(0.0, 3.0 * hk, 0.3 * zk, dt);
        // Rotor speed factor (spin-up from rest on summon, throttle, charge).
        let rotor_target = 0.55 + 0.45 * th + 0.35 * d.charge;
        self.rotor_k.step(rotor_target, 0.45 * hk, 1.0, dt);
        let flare_target = 0.35 * th + 0.6 * smoothstep(1.0, 12.0, accel) + 0.5 * d.charge;
        self.flare.step(flare_target, 4.0, 0.6, dt);
        let attack_w = if self.attack.is_some() { 0.3 } else { 1.0 };
        for i in 1..self.s1.len() {
            match self.roles.role(i) {
                Role::Turret => {
                    let target = match d.aim {
                        Some(a) => (-a.x).atan2(-a.z),
                        None => 0.55 * (t * 0.35 + 6.0 * hash01(self.seed, 7)).sin() * if self.roles.machine { 1.0 } else { 0.0 },
                    };
                    // Shortest way round, slew-limited (heavier turrets traverse slower).
                    let cur = self.s1[i].x;
                    let delta = wrap_pi(target - cur).clamp(-1.6, 1.6);
                    self.s1[i].step(cur + delta, 1.4 * hk, 0.8 * zk.min(1.0), dt);
                }
                Role::Barrel { len } => {
                    let target = match d.aim {
                        Some(a) => a.y.atan2(Vec3::new(a.x, 0.0, a.z).length()).clamp(-0.25, 0.7),
                        None => 0.04 * (t * 0.5 + 1.3).sin(),
                    };
                    self.s1[i].step(target, 2.2 * hk, 0.7, dt);
                    self.s2[i].step(0.0, 4.5 * hk, 0.42 * zk, dt);
                    self.s2[i].x = self.s2[i].x.clamp(-0.03 * len, 0.2 * len.clamp(0.2, 6.0));
                }
                Role::Wheel { steer: true, .. } => {
                    let wb = (self.roles.size * 0.6).max(0.5);
                    let target = if d.speed > 0.3 { (d.turn * wb / d.speed.max(0.5)).atan().clamp(-0.55, 0.55) } else { 0.0 };
                    self.s1[i].step(target, 3.0 * hk, 0.8, dt);
                }
                Role::Rotor { rate, machine, .. } => {
                    let rate = if machine { rate } else { 14.0 };
                    self.ang[i] = (self.ang[i] + rate * self.rotor_k.x * dt).rem_euclid(TAU);
                }
                Role::Hatch { .. } => {
                    let open = self.hatch_open;
                    let target = if open { 1.0 } else { 0.0 };
                    let (hz, zeta) = if open { (1.6 * hk, 0.75) } else { (3.2 * hk, 0.3 * zk) };
                    let before = self.s1[i].x;
                    self.s1[i].step(target, hz, zeta, dt);
                    // Clunk: bounce off the frame when slamming shut.
                    if !open && self.s1[i].x < 0.0 && before >= 0.0 {
                        let slam = -self.s1[i].v;
                        self.s1[i].x = 0.0;
                        self.s1[i].v = slam * 0.3 * self.weight.powf(-0.3);
                        if self.hatch_closing && slam > 0.8 {
                            self.hatch_closing = false;
                            self.dip.v -= 0.25 * slam.min(8.0) / self.weight.sqrt();
                            let pos = self.roles.parts[i].center;
                            if self.ncue < MAX_CUES {
                                self.cues[self.ncue] =
                                    Cue { kind: CueKind::HatchClose, pos, strength: (slam / 6.0).clamp(0.3, 1.5), ..Default::default() };
                                self.ncue += 1;
                            }
                        }
                    }
                }
                Role::Piston { .. } => self.s1[i].step(0.0, 5.0 * hk, 0.3, dt),
                Role::Fin { .. } => {
                    let target = (-d.turn * 0.22).clamp(-0.35, 0.35) + 0.04 * th * (t * 9.0 + i as f32).sin();
                    self.s1[i].step(target, 2.5 * hk, 0.5, dt);
                }
                Role::Head => {
                    let (yaw, pitch) = match d.look {
                        Some(p) => {
                            let v = p - self.pivot[i];
                            let yaw = (-v.x).atan2(-v.z).clamp(-1.1, 1.1);
                            let pitch = v.y.atan2(Vec3::new(v.x, 0.0, v.z).length()).clamp(-0.5, 0.5);
                            (yaw * attack_w, pitch * attack_w)
                        }
                        None => (0.0, 0.0),
                    };
                    self.s1[i].step(yaw, 1.8 * hk, 0.62, dt);
                    self.s2[i].step(pitch, 1.8 * hk, 0.62, dt);
                }
                Role::Jaw => {
                    let roar = match self.attack {
                        Some((at, true)) => 0.6 * (1.0 - smoothstep(0.35, 0.8, at)) * smoothstep(0.0, 0.08, at),
                        _ => 0.0,
                    };
                    self.s1[i].step(roar, 4.0 * hk, 0.45, dt);
                }
                Role::Blade { .. } => self.s1[i].step(0.0, 6.0 * hk, 0.35, dt),
                _ => {}
            }
        }
    }

    /// Build every part's delta from the springs + assembly.
    fn compose(&mut self) {
        let d = self.drive;
        let t = d.time;
        let th = self.throttle();
        let n = self.locals.len();
        for i in 0..n {
            let pv = self.pivot[i];
            let mut l = Local::IDENTITY;
            if i == 0 {
                let h = self.height;
                let rumble = if self.roles.machine && self.gait == Gait::Static {
                    let amp = 0.0025 * h * (0.5 + 0.5 * th + if d.piloted { 0.3 } else { 0.0 }) / self.weight;
                    amp * ((t * TAU * 17.0).sin() + 0.5 * (t * TAU * 29.0 + 1.3).sin())
                } else {
                    0.0
                };
                let hum = if !self.roles.machine && self.gait == Gait::Static { 0.004 * (t * PI).sin() } else { 0.0 };
                // Hits tilt the top along the hit; firing rocks the nose up (recoil).
                let q = Quat::from_rotation_x(self.flinch_x.x * 0.08 + self.kick.x * 0.04) * Quat::from_rotation_z(self.flinch_z.x * 0.08);
                // Tilt about the base centre so the construct rocks on its footprint.
                let mut r = about(q, Vec3::ZERO, pv);
                r.pos.y += rumble + self.dip.x * 0.03 * h.min(6.0);
                r.scale = Vec3::splat(1.0 + hum);
                l = r;
            } else {
                match self.roles.role(i) {
                    Role::Turret => l = Local::rot(Quat::from_rotation_y(self.s1[i].x)),
                    Role::Barrel { .. } => {
                        let q = Quat::from_rotation_x(self.s1[i].x);
                        l = Local { rot: q, pos: q * Vec3::new(0.0, 0.0, self.s2[i].x.max(-0.05)), scale: Vec3::ONE };
                    }
                    Role::Wheel { steer: true, .. } => l = Local::rot(Quat::from_rotation_y(self.s1[i].x)),
                    Role::Rotor { axis, rate, machine } => {
                        let a = if machine { wrap_pi(self.ang[i] - (rate * t).rem_euclid(TAU)) } else { self.ang[i] };
                        l = Local::rot(Quat::from_axis_angle(axis, a));
                    }
                    Role::Hatch { hinge, axis, open } => {
                        let q = Quat::from_axis_angle(axis, open * self.s1[i].x.clamp(-0.08, 1.15));
                        l = about(q, hinge, pv);
                    }
                    Role::Piston { axis, amp } => {
                        l = Local { pos: axis * (amp * (self.s1[i].x * 0.08).clamp(-0.6, 0.6)), ..Local::IDENTITY }
                    }
                    Role::Fin { axis } => l = Local::rot(Quat::from_axis_angle(axis, self.s1[i].x)),
                    Role::Thruster { .. } | Role::Exhaust { .. } => {
                        let f = 1.0
                            + (self.flare.x * if matches!(self.roles.role(i), Role::Thruster { .. }) { 0.6 } else { 0.35 })
                                .clamp(-0.3, 1.2);
                        l = Local { scale: Vec3::splat(f), ..Local::IDENTITY };
                    }
                    Role::Head => l = Local::rot(Quat::from_rotation_y(self.s1[i].x) * Quat::from_rotation_x(self.s2[i].x)),
                    Role::Jaw => l = Local::rot(Quat::from_rotation_x(-self.s1[i].x.clamp(-0.2, 1.0))),
                    Role::Eye => {
                        let u = self.blink_t / 0.13;
                        let b = if u < 1.0 { 1.0 - 0.85 * (PI * u).sin() } else { 1.0 };
                        l = Local { scale: Vec3::new(1.0, b.max(0.1), 1.0), ..Local::IDENTITY };
                    }
                    Role::Blade { axis } => {
                        let k = 1.0 + (self.s1[i].x * 0.03).clamp(-0.1, 0.15);
                        let mut s = Vec3::ONE;
                        s[axis.min(2) as usize] = k;
                        l = Local { scale: s, ..Local::IDENTITY };
                    }
                    _ => {}
                }
            }
            if let Some(a) = self.assembly_local(i) {
                l = a.then(l);
            }
            self.locals[i] = sane(l);
        }
    }

    /// Assembly / collapse delta for part `i` (None = no effect this frame).
    fn assembly_local(&self, i: usize) -> Option<Local> {
        let info = self.roles.parts.get(i)?;
        let vox = self.vox;
        let w = self.weight;
        let hidden = Local { scale: Vec3::splat(0.001), ..Local::IDENTITY };
        if let Some((t, dur)) = self.dismiss {
            // Reverse order: the lock stage lets go first, the core last.
            let stages = self.roles.stages.max(1);
            let rev = (stages - 1 - info.stage.min(stages - 1)) as f32;
            let step = dur * 0.6 / stages.max(2) as f32;
            let u = ((t - rev * step) / (dur * 0.45).max(0.05)).clamp(0.0, 1.0);
            if u <= 0.0 {
                return None;
            }
            let lift = info.out * (vox * 1.2) * Ease::OutQuad.apply((u / 0.4).min(1.0));
            let to_ring =
                self.drive.ring.map_or(Vec3::Y * vox * 4.0, |r| (r - info.center) * Ease::InCubic.apply(((u - 0.4) / 0.6).clamp(0.0, 1.0)));
            let s = 1.0 - 0.7 * Ease::InQuad.apply(((u - 0.3) / 0.7).clamp(0.0, 1.0));
            let pos = if i == 0 { lift * 0.5 } else { lift + to_ring * 0.35 };
            return Some(Local { pos, scale: Vec3::splat(s.max(0.001)), ..Local::IDENTITY });
        }
        let t = self.summon?;
        if t > self.asm_total + self.stage_delay + 0.25 {
            return None;
        }
        let k = info.stage;
        let (f, sn) = (self.asm_flight, self.asm_snap);
        let over = vox * 0.35 * w.sqrt();
        let hover = vox * (1.0 + 0.02 * self.roles.size.min(10.0));
        if i == 0 {
            // The core drops the last voxel into place and squashes.
            let u = t / sn;
            if u < 0.6 {
                let y = hover * (1.0 - Ease::InQuad.apply(u / 0.6));
                return Some(Local { pos: Vec3::Y * y, scale: Vec3::splat(0.85 + 0.15 * u / 0.6), ..Local::IDENTITY });
            }
            let sq = 0.14 * w.sqrt() * spring_impulse(t - sn * 0.6, 7.0 / w.sqrt(), 14.0);
            return Some(Local { scale: Vec3::new(1.0 + 0.5 * sq, 1.0 - sq, 1.0 + 0.5 * sq), ..Local::IDENTITY });
        }
        let pi = self.parent.get(i).copied().unwrap_or(0);
        let parent_stage = self.roles.parts.get(pi).map_or(0, |p| p.stage);
        let role = info.role;
        let start = self.stage_start(k);
        if matches!(role, Role::Eye) {
            let land = self.land_time(k);
            if t < land {
                return Some(hidden);
            }
            let pop = 0.4 * spring_impulse(t - land, 5.0, 10.0);
            return Some(Local { scale: Vec3::splat(1.0 + pop), ..Local::IDENTITY });
        }
        if matches!(role, Role::Hatch { .. } | Role::Seat | Role::Support | Role::Grip) || (pi != 0 && k == parent_stage) {
            return None; // rides its parent (a composite brick)
        }
        if t < start {
            return Some(hidden);
        }
        if let Role::Blade { axis } = role {
            // Blades don't fly: they slide out of the hilt with an overshoot.
            let u = ((t - start) / (f + sn)).clamp(0.0, 1.0);
            let e = Ease::OutBack.apply(u).max(0.02);
            let mut s = Vec3::ONE;
            s[axis.min(2) as usize] = e;
            return Some(Local { scale: s, ..Local::IDENTITY });
        }
        let hover_v = info.out * hover;
        if t < start + f {
            // Streaming out of the ring along the beam (only the last stretch, so the silhouette
            // stays close to final).
            let u = (t - start) / f;
            let reach = (0.5 * self.roles.size).clamp(0.75, 3.0);
            let from = match self.drive.ring {
                Some(r) => {
                    let v = r - info.center;
                    v.normalize_or(info.out) * v.length().min(reach)
                }
                None => info.out * reach,
            };
            let e = Ease::OutCubic.apply(u);
            let pos = from.lerp(hover_v, e);
            return Some(Local { pos, scale: Vec3::splat(0.45 + 0.55 * e), ..Local::IDENTITY });
        }
        let u = (t - start - f) / sn;
        if u < 1.0 {
            // Snap: drop into the slot, overshoot ~a third of a voxel, squash, settle.
            let a = if u < 0.55 { 1.0 - Ease::InQuad.apply(u / 0.55) } else { -(over / hover.max(1e-3)) * (PI * (u - 0.55) / 0.45).sin() };
            let sq = 0.16 * w.sqrt() * (PI * u).sin() * smoothstep(0.4, 0.6, u);
            return Some(Local { pos: hover_v * a, scale: Vec3::new(1.0 + 0.5 * sq, 1.0 - sq, 1.0 + 0.5 * sq), ..Local::IDENTITY });
        }
        None
    }
}

fn sane(l: Local) -> Local {
    let ok = l.rot.is_finite() && l.pos.is_finite() && l.scale.is_finite();
    if !ok {
        return Local::IDENTITY;
    }
    Local {
        rot: l.rot.normalize(),
        pos: l.pos.clamp(Vec3::splat(-100.0), Vec3::splat(100.0)),
        scale: l.scale.clamp(Vec3::splat(0.001), Vec3::splat(4.0)),
    }
}
