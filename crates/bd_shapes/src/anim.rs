//! Procedural animation shared by the sim (hitboxes) and the client (visuals). Pure, deterministic,
//! allocation-free per call.
//!
//! `pose` writes one **model-space** `Affine3A` per part (index-aligned with `VoxelModel::parts`, in
//! metres) that maps the part's rest-pose voxels/mesh to the animated pose. Parts are rigid; the rig is
//! derived from `PartRole`s when the model is voxelised (see [`Rig`]), so curated icons animate too.
//!
//! Layering: `pose_layered(model, &[(base, t_base), (overlay, t_overlay)])` composes per-part deltas
//! (rotations multiply, offsets add, scales multiply) — e.g. `Locomotion` + `Attack(Bite)` bites while
//! walking; `Idle` + `HitReact` flinches while breathing.

use bd_core::shape::{PartRole, Tint};
use bd_core::voxel::Aabb;
use glam::{Affine3A, Quat, Vec3};

use crate::ease::{Ease, K, smoothstep, spring_impulse, track};
use crate::voxelize::{PartModel, VoxelModel};

use std::f32::consts::{PI, TAU};

// ------------------------------------------------------------------------------------ public API

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimState {
    /// Standing/hovering; `t` = any monotonic time (s).
    Idle,
    /// Moving. `distance` = metres travelled (caller accumulates) — drives gait phase so feet plant;
    /// at `speed` < 0.05 (and grounded) this is exactly `Idle` except that wheels/treads/cranks keep
    /// the phase of `distance` — use it instead of `Idle` for anything with wheels so they never snap.
    /// `speed` m/s scales amplitude/gait choice; `airborne` switches to flight/leap poses.
    /// `t` = monotonic time (wing beats, flicker).
    Locomotion { speed: f32, distance: f32, airborne: bool },
    /// `t` = seconds since the attack started. See [`attack_timing`].
    Attack(Attack),
    /// `dir` = direction the hit travels, model space; `strength` 0..1. `t` = seconds since the hit.
    HitReact { dir: Vec3, strength: f32 },
    /// Parts pop in with overshoot. `t` = seconds since the cast; finished at `duration`.
    Forming { duration: f32 },
    /// Swell, then collapse (tips first). `t` = seconds since dismiss; gone at `duration`.
    Dismiss { duration: f32 },
    /// Layer: machine turrets yaw / barrels pitch and heads look toward an aim direction.
    /// `yaw` rad (+ = left, i.e. toward -X from -Z), `pitch` rad (+ = up), model space.
    Aim { yaw: f32, pitch: f32 },
    /// Humanoid: power-slide at `speed` m/s (reclined, legs out front, trailing hand).
    Slide { speed: f32 },
    /// Humanoid: tether just fired from arm `side` (-1 left, +1 right; 0 = right). `t` = seconds
    /// since firing (snaps out in 50 ms, then holds the arm extended).
    TetherFire { side: i8 },
    /// Humanoid: hanging from a tether gripped overhead by `side` (0 = both hands). `lean` -1..1 =
    /// swing direction (+ = swinging forward). `t` = any monotonic time (legs kick, body sways).
    Swing { side: i8, lean: f32 },
    /// Humanoid: ring-arm (right) cast thrust. `t` = seconds since the cast; see [`CAST_TIMING`].
    Cast,
    /// Humanoid: knocked-back tumble spinning at `spin` rad/s about the body centre; limbs flail.
    Tumble { spin: f32 },
    /// Launched by a big hit travelling along `dir` (model space): the body spins end over end about
    /// the axis ⟂ `dir` at `spin` rad/s with arms and legs trailing, then (the caller) lands/recovers.
    Launched { dir: Vec3, spin: f32 },
}

/// Ring-arm cast: pull back (anticipation), thrust (active: spawn the construct at its end), hold +
/// recover. Fits inside the >= 150 ms forming budget in DESIGN.md.
pub const CAST_TIMING: AttackTiming = AttackTiming { anticipation: 0.09, active: 0.08, recovery: 0.33 };

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Attack {
    Bite,
    Swipe,
    /// Telegraph (0.4 s), dash (loop `t` inside the active window for as long as the dash lasts), skid.
    Charge,
    /// Heavy anticipation, impact frame at the end of the active window.
    Stomp,
    /// 360° spin with the tail trailing.
    TailSweep,
    /// `aim` = model-space direction to breathe along (head tracks it during the active window).
    Breath { aim: Vec3 },
    /// Whip/tentacle chain crack.
    Lash,
}

/// Timed data for creature attacks (the sim uses it for telegraphs and damage frames).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackTiming {
    pub anticipation: f32,
    pub active: f32,
    pub recovery: f32,
}

impl AttackTiming {
    pub fn total(&self) -> f32 {
        self.anticipation + self.active + self.recovery
    }
    pub fn active_window(&self) -> (f32, f32) {
        (self.anticipation, self.anticipation + self.active)
    }
    /// The frame where the hit lands (stomp/bite contact).
    pub fn impact(&self) -> f32 {
        self.anticipation + self.active
    }
    pub fn is_active(&self, t: f32) -> bool {
        t >= self.anticipation && t < self.anticipation + self.active
    }
}

/// Size- and weight-scaled attack timing for `model` (its rig height and `model.weight`; see
/// [`attack_timing_hw`]).
pub fn attack_timing(model: &VoxelModel, attack: Attack) -> AttackTiming {
    attack_timing_hw(model.rig.height, attack, model.weight)
}

/// Attack timing from rig height (m) and weight factor (`forms::weight_of`, 1 = authored). Big
/// creatures telegraph longer; heavier ones wind up longer, strike a little *snappier* (short active
/// window) and settle slower. Charge/Breath telegraphs stay fixed by COMBAT.md (0.4 s / 0.6 s) so
/// players learn them. The sim must call this with the same inputs so hitboxes match the pose.
pub fn attack_timing_hw(height: f32, attack: Attack, weight: f32) -> AttackTiming {
    let h = if height.is_finite() { height } else { 2.0 };
    let sf = (h / 2.0).max(0.1).powf(0.3).clamp(0.85, 2.0);
    let w = if weight.is_finite() { weight.clamp(0.5, 2.5) } else { 1.0 };
    let (wa, wc, wr) = (w.powf(0.6), w.powf(-0.2).clamp(0.85, 1.1), w.powf(0.5));
    let fixed = matches!(attack, Attack::Charge | Attack::Breath { .. });
    let (a, c, r) = match attack {
        Attack::Bite => (0.2 * sf, 0.08 * sf, 0.25 * sf),
        Attack::Swipe => (0.18 * sf, 0.1 * sf, 0.25 * sf),
        Attack::Charge => (0.4, 0.6, 0.3 * sf),
        Attack::Stomp => (0.42 * sf, 0.1 * sf, 0.45 * sf),
        Attack::TailSweep => (0.25 * sf, 0.45 * sf, 0.3 * sf),
        Attack::Breath { .. } => (0.6, 1.2, 0.35 * sf),
        Attack::Lash => (0.12, 0.12, 0.2),
    };
    if fixed {
        return AttackTiming { anticipation: a, active: c, recovery: r * wr };
    }
    AttackTiming { anticipation: a * wa, active: c * wc, recovery: r * wr }
}

/// Additive local transform of a part about its pivot (offset in metres, parent space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Local {
    pub rot: Quat,
    pub pos: Vec3,
    pub scale: Vec3,
}

impl Local {
    pub const IDENTITY: Local = Local { rot: Quat::IDENTITY, pos: Vec3::ZERO, scale: Vec3::ONE };

    pub fn rot(rot: Quat) -> Self {
        Local { rot, ..Self::IDENTITY }
    }
    /// Combine two deltas.
    pub fn then(self, o: Local) -> Local {
        Local { rot: (self.rot * o.rot).normalize(), pos: self.pos + o.pos, scale: self.scale * o.scale }
    }
    fn affine(&self, pivot: Vec3) -> Affine3A {
        Affine3A::from_translation(pivot + self.pos)
            * Affine3A::from_quat(self.rot)
            * Affine3A::from_scale(self.scale)
            * Affine3A::from_translation(-pivot)
    }
}

/// Evaluate a pose. `out` must hold at least `model.parts.len()` entries.
pub fn pose(model: &VoxelModel, anim: AnimState, t: f32, out: &mut [Affine3A]) {
    pose_layered(model, &[(anim, t)], out);
}

/// Evaluate up to 4 layered animations (extra layers are ignored).
pub fn pose_layered(model: &VoxelModel, layers: &[(AnimState, f32)], out: &mut [Affine3A]) {
    pose_layered_with(model, layers, None, out);
}

/// [`pose_layered`] plus a per-part `extra` delta (index-aligned with `model.parts`), applied
/// *outside* the clip layers (parent side: a wheel's steer yaw wraps its roll, a hatch hinge
/// wraps any clip motion). Client-only role behaviours come in here (see
/// `behaviours::ConstructAnimator::locals`); the sim never passes one.
pub fn pose_layered_with(model: &VoxelModel, layers: &[(AnimState, f32)], extra: Option<&[Local]>, out: &mut [Affine3A]) {
    let mut frames: [Option<Frame>; 4] = [None, None, None, None];
    for (slot, (a, t)) in frames.iter_mut().zip(layers.iter()) {
        *slot = Some(Frame::new(model, sanitize(*a), fin(*t, 0.0).clamp(-1e6, 1e6)));
    }
    let n = model.parts.len().min(out.len());
    for i in 0..n {
        let mut l = Local::IDENTITY;
        for f in frames.iter().flatten() {
            l = l.then(f.local(model, i));
        }
        if let Some(e) = extra.and_then(|e| e.get(i)) {
            l = e.then(l);
        }
        let p = &model.parts[i];
        let a = l.affine(p.pivot);
        let m = if i == 0 || p.parent_index >= i { a } else { out[p.parent_index] * a };
        out[i] = if m.is_finite() { m } else { Affine3A::IDENTITY };
    }
}

#[inline]
fn fin(x: f32, d: f32) -> f32 {
    if x.is_finite() { x } else { d }
}

fn fin3(v: Vec3) -> Vec3 {
    if v.is_finite() { v } else { Vec3::ZERO }
}

/// Clamp every input to something finite and sane.
fn sanitize(a: AnimState) -> AnimState {
    match a {
        AnimState::Locomotion { speed, distance, airborne } => AnimState::Locomotion {
            speed: fin(speed, 0.0).clamp(0.0, 500.0),
            distance: fin(distance, 0.0).clamp(-1e6, 1e6),
            airborne,
        },
        AnimState::Attack(Attack::Breath { aim }) => AnimState::Attack(Attack::Breath { aim: fin3(aim) }),
        AnimState::HitReact { dir, strength } => AnimState::HitReact { dir: fin3(dir), strength: fin(strength, 0.0).clamp(0.0, 2.0) },
        AnimState::Forming { duration } => AnimState::Forming { duration: fin(duration, 0.5).clamp(0.02, 30.0) },
        AnimState::Dismiss { duration } => AnimState::Dismiss { duration: fin(duration, 0.5).clamp(0.02, 30.0) },
        AnimState::Aim { yaw, pitch } => AnimState::Aim { yaw: fin(yaw, 0.0).clamp(-PI, PI), pitch: fin(pitch, 0.0).clamp(-1.5, 1.5) },
        AnimState::Slide { speed } => AnimState::Slide { speed: fin(speed, 0.0).clamp(0.0, 200.0) },
        AnimState::Swing { side, lean } => AnimState::Swing { side, lean: fin(lean, 0.0).clamp(-1.0, 1.0) },
        AnimState::Tumble { spin } => AnimState::Tumble { spin: fin(spin, 6.0).clamp(-60.0, 60.0) },
        AnimState::Launched { dir, spin } => AnimState::Launched { dir: fin3(dir), spin: fin(spin, 8.0).clamp(-60.0, 60.0) },
        other => other,
    }
}

/// Rest pose (identities).
pub fn rest_pose(model: &VoxelModel, out: &mut [Affine3A]) {
    let n = model.parts.len().min(out.len());
    out[..n].fill(Affine3A::IDENTITY);
}

/// Cross-fade two evaluated poses (`w` = 0 → a, 1 → b).
pub fn blend_poses(a: &[Affine3A], b: &[Affine3A], w: f32, out: &mut [Affine3A]) {
    let n = a.len().min(b.len()).min(out.len());
    for i in 0..n {
        let (sa, ra, ta) = a[i].to_scale_rotation_translation();
        let (sb, rb, tb) = b[i].to_scale_rotation_translation();
        out[i] = Affine3A::from_scale_rotation_translation(sa.lerp(sb, w), ra.slerp(rb, w), ta.lerp(tb, w));
    }
}

// ------------------------------------------------------------------------------------------- rig

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gait {
    /// No locomotion rig (items, structures, objects) — only overlays (wings, flames, wheels, cloth).
    #[default]
    Static,
    Biped,
    Quadruped,
    Hexapod,
    Octopod,
    Serpent,
    Fish,
    /// Wings, no legs.
    Flyer,
    /// Legless blob with a head lobe: hops with squash & stretch.
    Hopper,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartKind {
    Root,
    /// Index into `Rig::legs`.
    Leg(u8),
    /// −1 left, +1 right.
    Arm(i8),
    Head,
    Jaw,
    /// Chain index into `Rig::tails`, position k in chain.
    Tail { chain: u8, k: u8 },
    Wing(i8),
    /// Cloth/whip chain (role Other): chain index into `Rig::chains`, position k.
    Chain { chain: u8, k: u8 },
    /// All-Core Other part: exhaust flame (flickers).
    Flame,
    /// Disc-shaped Other part: spins with distance (radius m).
    Wheel(f32),
    /// Any other Other part: gentle sway (parachute canopy…).
    Sway,
    /// Pilot-seat marker (`voxelize::SEAT_PART`): rides its parent, never animated itself.
    Seat,
    /// Driven by the machine kit (`VoxelModel::machine`): wheels, cranks, rotors, turrets...
    Machine,
    /// A v1.5 construct-animation role part (hatch, turret, barrel, rotor, piston, fin, eye, blade)
    /// with no machine link: rides its parent in every clip; the client's role behaviours
    /// (`behaviours::ConstructAnimator`) move it. Never moves on the sim side.
    Rigid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegRig {
    pub part: usize,
    /// −1 left, +1 right.
    pub side: f32,
    /// Front→back rank within its side.
    pub rank: u8,
    /// Ranks on its side.
    pub ranks: u8,
    /// Pivot → ground (m).
    pub len: f32,
    /// Horizontal pivot → foot distance (m) — splayed legs step by yawing.
    pub reach: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rig {
    pub gait: Gait,
    pub kinds: Vec<PartKind>,
    pub legs: Vec<LegRig>,
    pub tails: Vec<Vec<usize>>,
    pub chains: Vec<Vec<usize>>,
    pub has_wings: bool,
    pub has_head: bool,
    pub has_jaw: bool,
    pub has_arms: bool,
    /// Model height (m).
    pub height: f32,
    /// Model length along Z (m).
    pub length: f32,
    /// Average leg length (m) or 0.4·height.
    pub leg_len: f32,
    /// Leg splayed sideways (arachnid/insect style stepping).
    pub splayed: bool,
    pub max_depth: u8,
}

impl Rig {
    pub(crate) fn analyze(parts: &[PartModel], bounds: &Aabb) -> Rig {
        let n = parts.len();
        let mut kinds = vec![PartKind::Sway; n];
        if n > 0 {
            kinds[0] = PartKind::Root;
        }
        let is_core = |p: &PartModel| !p.voxels.is_empty() && p.voxels.iter().all(|(_, t)| *t == Tint::Core);
        let child_with = |i: usize, f: &dyn Fn(&PartModel) -> bool| (i + 1..n).find(|&j| parts[j].parent_index == i && f(&parts[j]));

        let mut rig = Rig { height: (bounds.max.y - bounds.min.y).max(0.25), length: (bounds.max.z - bounds.min.z).max(0.25), ..Default::default() };
        // Tail chains.
        for i in 1..n {
            if parts[i].role != Some(PartRole::Tail) {
                continue;
            }
            let pi = parts[i].parent_index;
            if pi != 0 && parts[pi].role == Some(PartRole::Tail) {
                continue;
            }
            let mut chain = vec![i];
            let mut cur = i;
            while let Some(c) = child_with(cur, &|p| p.role == Some(PartRole::Tail)) {
                chain.push(c);
                cur = c;
            }
            let ci = rig.tails.len() as u8;
            for (k, &p) in chain.iter().enumerate() {
                kinds[p] = PartKind::Tail { chain: ci, k: k as u8 };
            }
            rig.tails.push(chain);
        }
        // Other: flames, chains, wheels, sway.
        let other = |p: &PartModel| matches!(p.role, Some(PartRole::Other | PartRole::Cloth)) && !is_core(p);
        for i in 1..n {
            let p = &parts[i];
            if p.role == Some(PartRole::Exhaust) {
                kinds[i] = PartKind::Flame;
                continue;
            }
            if !matches!(p.role, Some(PartRole::Other | PartRole::Cloth)) {
                continue;
            }
            if is_core(p) {
                // A small glowing part on a head is an eye (steady), not a flame.
                let ext = (p.vmax - p.vmin + glam::IVec3::ONE).max_element();
                let on_head = p.parent_index != 0 && parts[p.parent_index].role == Some(PartRole::Head);
                kinds[i] = if on_head && ext <= 3 && p.role == Some(PartRole::Other) { PartKind::Rigid } else { PartKind::Flame };
                continue;
            }
            let pi = p.parent_index;
            let parent_other = pi != 0 && other(&parts[pi]);
            if parent_other {
                continue; // handled from the chain start
            }
            if let Some(_c) = child_with(i, &other) {
                let mut chain = vec![i];
                let mut cur = i;
                while let Some(c) = child_with(cur, &other) {
                    chain.push(c);
                    cur = c;
                }
                let ci = rig.chains.len() as u8;
                for (k, &q) in chain.iter().enumerate() {
                    kinds[q] = PartKind::Chain { chain: ci, k: k as u8 };
                }
                rig.chains.push(chain);
                continue;
            }
            let ext = (p.vmax - p.vmin + glam::IVec3::ONE).as_vec3();
            if p.role == Some(PartRole::Other) && !p.voxels.is_empty() && ext.x <= ext.y * 0.75 && ext.x <= ext.z * 0.75 && (ext.y - ext.z).abs() <= 2.0 {
                kinds[i] = PartKind::Wheel(ext.y.max(ext.z) * 0.5 * p.voxel_size);
            } else {
                kinds[i] = PartKind::Sway;
            }
        }
        // Roles.
        let mut legs = Vec::new();
        for i in 1..n {
            let p = &parts[i];
            match p.role {
                Some(PartRole::Head) => {
                    kinds[i] = PartKind::Head;
                    rig.has_head = true;
                }
                Some(PartRole::Jaw) => {
                    kinds[i] = PartKind::Jaw;
                    rig.has_jaw = true;
                }
                Some(PartRole::ArmLeft) => {
                    kinds[i] = PartKind::Arm(-1);
                    rig.has_arms = true;
                }
                Some(PartRole::ArmRight) => {
                    kinds[i] = PartKind::Arm(1);
                    rig.has_arms = true;
                }
                Some(PartRole::WingLeft) => {
                    kinds[i] = PartKind::Wing(-1);
                    rig.has_wings = true;
                }
                Some(PartRole::WingRight) => {
                    kinds[i] = PartKind::Wing(1);
                    rig.has_wings = true;
                }
                Some(PartRole::Weapon) => kinds[i] = PartKind::Sway,
                Some(PartRole::Thruster) => kinds[i] = PartKind::Flame,
                Some(PartRole::Hatch | PartRole::Turret | PartRole::Barrel | PartRole::Rotor | PartRole::Piston | PartRole::Fin | PartRole::Eye | PartRole::Blade | PartRole::Grip | PartRole::Support) => kinds[i] = PartKind::Rigid,
                Some(r @ (PartRole::LegFrontLeft | PartRole::LegFrontRight | PartRole::LegBackLeft | PartRole::LegBackRight)) => {
                    let side = if matches!(r, PartRole::LegFrontLeft | PartRole::LegBackLeft) { -1.0 } else { 1.0 };
                    let (len, reach) = if p.voxels.is_empty() {
                        (0.5, 0.0)
                    } else {
                        let low = p.vmin.y;
                        let mut foot = Vec3::ZERO;
                        let mut c: f32 = 0.0;
                        for (v, _) in &p.voxels {
                            if v.y <= low + 1 {
                                foot += v.as_vec3();
                                c += 1.0;
                            }
                        }
                        let foot = foot / c.max(1.0) * p.voxel_size;
                        let len = (p.pivot.y - (low as f32 - 0.5) * p.voxel_size).max(0.1);
                        let reach = ((foot.x - p.pivot.x).powi(2) + (foot.z - p.pivot.z).powi(2)).sqrt();
                        (len, reach)
                    };
                    legs.push(LegRig { part: i, side, rank: 0, ranks: 1, len, reach });
                }
                _ => {}
            }
        }
        // Ranks front→back per side.
        for s in [-1.0f32, 1.0] {
            let mut idx: Vec<usize> = (0..legs.len()).filter(|&k| legs[k].side == s).collect();
            idx.sort_by(|&a, &b| parts[legs[a].part].pivot.z.total_cmp(&parts[legs[b].part].pivot.z));
            let cnt = idx.len() as u8;
            for (r, &k) in idx.iter().enumerate() {
                legs[k].rank = r as u8;
                legs[k].ranks = cnt;
            }
        }
        for (k, l) in legs.iter().enumerate() {
            kinds[l.part] = PartKind::Leg(k as u8);
        }
        let nl = legs.len();
        for i in 1..n {
            if parts[i].id == crate::voxelize::SEAT_PART {
                kinds[i] = PartKind::Seat;
            }
        }
        rig.leg_len = if nl > 0 { legs.iter().map(|l| l.len).sum::<f32>() / nl as f32 } else { rig.height * 0.4 };
        rig.splayed = nl >= 6 || (nl > 0 && legs.iter().all(|l| l.reach > l.len * 0.9));
        let longest_tail = rig.tails.iter().map(|c| c.len()).max().unwrap_or(0);
        rig.gait = match nl {
            0 => {
                if longest_tail >= 4 {
                    Gait::Serpent
                } else if longest_tail >= 1 && !rig.has_wings {
                    Gait::Fish
                } else if rig.has_wings && rig.has_head {
                    Gait::Flyer
                } else if rig.has_head {
                    Gait::Hopper
                } else {
                    Gait::Static
                }
            }
            1 | 2 => Gait::Biped,
            3..=5 => Gait::Quadruped,
            6 | 7 => Gait::Hexapod,
            _ => Gait::Octopod,
        };
        rig.legs = legs;
        rig.max_depth = parts.iter().map(|p| p.depth).max().unwrap_or(0);
        rig.kinds = kinds;
        rig
    }
}

// ----------------------------------------------------------------------------------------- frame

#[derive(Clone, Copy, Debug)]
struct Frame {
    anim: AnimState,
    t: f32,
    loco: Loco,
    timing: AttackTiming,
}

/// Per-call locomotion state (computed once, used by every part).
#[derive(Clone, Copy, Debug, Default)]
struct Loco {
    speed: f32,
    airborne: bool,
    /// Gait cycle phase (cycles, fractional part is the phase).
    phase: f32,
    stance: f32,
    amp: f32,
    /// 0..1 fade-in of gait motion with speed.
    w: f32,
    /// Quadruped gait mode: 0 walk, 1 trot, 2 gallop.
    mode: u8,
    root_drop: f32,
    /// Wing beat phase (cycles) and amplitude.
    flap: f32,
    flap_amp: f32,
    /// Raw metres travelled (wheels).
    dist: f32,
}

fn leg_cycle(p: f32, stance: f32) -> (f32, f32) {
    let p = p.rem_euclid(1.0);
    if p < stance {
        let s = p / stance;
        (1.0 - 2.0 * s, 0.0)
    } else {
        let s = (p - stance) / (1.0 - stance);
        (-1.0 + 2.0 * Ease::InOutSine.apply(s), (PI * s).sin())
    }
}

fn leg_offset(rig: &Rig, l: &LegRig, mode: u8) -> f32 {
    let right = l.side > 0.0;
    match rig.gait {
        Gait::Biped => {
            if right { 0.5 } else { 0.0 }
        }
        Gait::Quadruped => {
            let front = l.rank == 0 && l.ranks > 1;
            match mode {
                0 => match (right, front) {
                    (false, false) => 0.0,
                    (false, true) => 0.25,
                    (true, false) => 0.5,
                    (true, true) => 0.75,
                },
                1 => {
                    if front == right { 0.5 } else { 0.0 }
                }
                _ => match (right, front) {
                    (false, false) => 0.0,
                    (true, false) => 0.12,
                    (false, true) => 0.5,
                    (true, true) => 0.62,
                },
            }
        }
        _ => {
            let alt = ((l.rank as u32 + right as u32) % 2) as f32 * 0.5;
            alt + l.rank as f32 * 0.04
        }
    }
}

impl Frame {
    fn new(m: &VoxelModel, anim: AnimState, t: f32) -> Frame {
        let rig = &m.rig;
        let mut loco = Loco::default();
        let mut timing = AttackTiming { anticipation: 0.0, active: 0.0, recovery: 0.0 };
        match anim {
            AnimState::Locomotion { speed, distance, airborne } => {
                loco = Loco::new(rig, speed.max(0.0), distance, airborne, t);
            }
            AnimState::Attack(a) => {
                timing = attack_timing(m, a);
                if matches!(a, Attack::Charge) {
                    // Dash legs: a fast synthetic gallop.
                    let v = rig.leg_len.max(0.2) * 7.0;
                    let ta = (t - timing.anticipation).max(0.0);
                    loco = Loco::new(rig, v, ta * v, false, t);
                }
            }
            AnimState::Idle => {
                loco = Loco::new(rig, 0.0, 0.0, false, t);
            }
            _ => {}
        }
        Frame { anim, t, loco, timing }
    }

    fn local(&self, m: &VoxelModel, i: usize) -> Local {
        let t = self.t;
        match self.anim {
            AnimState::Idle => idle_local(m, i, t, &self.loco),
            AnimState::Locomotion { .. } => loco_local(m, i, t, &self.loco),
            AnimState::Attack(a) => attack_local(m, i, a, t, &self.timing, &self.loco),
            AnimState::HitReact { dir, strength } => hit_local(m, i, dir, strength, t),
            AnimState::Forming { duration } => forming_local(m, i, t, duration, false),
            AnimState::Dismiss { duration } => forming_local(m, i, t, duration, true),
            AnimState::Aim { yaw, pitch } => aim_layer(m, i, yaw, pitch),
            a @ (AnimState::Slide { .. } | AnimState::TetherFire { .. } | AnimState::Swing { .. } | AnimState::Cast | AnimState::Tumble { .. } | AnimState::Launched { .. }) => humanoid_local(m, i, a, t),
        }
    }
}

impl Loco {
    fn new(rig: &Rig, speed: f32, distance: f32, airborne: bool, t: f32) -> Loco {
        let ll = rig.leg_len.max(0.1);
        let v_rel = speed / ll;
        let w = smoothstep(0.05, 0.6, speed);
        let mut l = Loco { speed, airborne, w, dist: distance, ..Default::default() };
        // Wing beat.
        let hz = if rig.gait == Gait::Hexapod { 18.0 } else { (3.2 / rig.height.max(0.2).sqrt()).clamp(1.2, 5.0) };
        l.flap = t * hz;
        l.flap_amp = 1.0 - 0.8 * smoothstep(6.0, 14.0, speed);
        match rig.gait {
            Gait::Biped => {
                l.stance = 0.62 - 0.24 * smoothstep(2.0, 5.0, v_rel);
                l.amp = (0.22 + 0.07 * v_rel).clamp(0.22, 0.75);
            }
            Gait::Quadruped => {
                l.mode = if v_rel < 2.2 { 0 } else if v_rel < 5.0 { 1 } else { 2 };
                l.stance = [0.7, 0.5, 0.36][l.mode as usize];
                l.amp = [0.28, 0.4, 0.6][l.mode as usize];
            }
            Gait::Hexapod | Gait::Octopod => {
                l.stance = 0.55;
                l.amp = 0.32;
            }
            _ => {
                l.stance = 0.5;
                l.amp = 0.3;
            }
        }
        match rig.gait {
            Gait::Biped | Gait::Quadruped | Gait::Hexapod | Gait::Octopod => {
                let reach = if rig.splayed {
                    rig.legs.iter().map(|x| x.reach).fold(0.0, f32::max).max(ll * 0.5)
                } else {
                    ll
                };
                let stride = 2.0 * reach * l.amp.sin();
                let cycle = (stride / l.stance).max(0.05);
                l.phase = distance / cycle;
                // Keep stance feet planted: lower the body by the rigid-leg height loss.
                if !rig.splayed && !airborne {
                    let mut drop: f32 = 0.0;
                    for leg in &rig.legs {
                        let p = l.phase + leg_offset(rig, leg, l.mode);
                        let (a, lift) = leg_cycle(p, l.stance);
                        if lift == 0.0 {
                            let ang = a * l.amp * w;
                            drop = drop.max(leg.len * (1.0 - ang.cos()));
                        }
                    }
                    l.root_drop = drop;
                }
            }
            Gait::Serpent => {
                l.phase = distance / (rig.length * 0.8).max(0.3) + t * 0.12;
            }
            Gait::Fish => {
                l.phase = distance / (rig.length * 0.9).max(0.3) + t * 0.6;
            }
            Gait::Hopper => {
                l.phase = distance / (rig.height * 1.4).max(0.2);
            }
            _ => {}
        }
        l
    }
}

// ------------------------------------------------------------------------------------- overlays

fn rx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}
fn ry(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}
fn rz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

/// Things that move in every state: flames flicker, wheels spin, cloth sways, wings beat or fold.
fn overlay(m: &VoxelModel, i: usize, t: f32, lo: &Loco, dist: f32, _moving: bool) -> Option<Local> {
    let rig = &m.rig;
    if let Some(l) = crate::features::animate_features(m, i, &crate::features::FeatureDrive { time: t, speed: lo.speed, distance: dist }) {
        return Some(l);
    }
    match rig.kinds[i] {
        PartKind::Flame => {
            let boost = 1.0 + 0.35 * smoothstep(0.0, 10.0, lo.speed);
            let f = 1.0 + 0.16 * (t * 37.0 + i as f32 * 2.1).sin() + 0.09 * (t * 23.0 + i as f32).sin();
            Some(Local { scale: Vec3::new(1.0 + 0.4 * (f - 1.0), f, 1.0 + 0.4 * (f - 1.0)) * boost, ..Local::IDENTITY })
        }
        PartKind::Wheel(r) => Some(Local::rot(rx(-dist / r.max(0.05)))),
        PartKind::Chain { k, .. } => {
            let k = k as f32;
            let stream = smoothstep(0.0, 12.0, lo.speed);
            let first = if k == 0.0 { -1.1 * stream } else { 0.0 };
            let flutter = (0.07 + 0.12 * stream) * (t * (2.0 + 7.0 * stream) - k * 1.1).sin();
            Some(Local::rot(rx(first + flutter) * rz(0.05 * (t * 1.3 - k * 0.6).sin())))
        }
        PartKind::Sway => {
            let a = 0.05 * (t * 1.1 + i as f32).sin();
            let b = 0.04 * (t * 0.8 + 1.0).sin();
            let billow = 1.0 + 0.03 * (t * 2.2).sin();
            Some(Local { rot: rx(a) * rz(b), scale: Vec3::new(billow, 1.0 / billow, billow), ..Local::IDENTITY })
        }
        PartKind::Wing(side) => {
            let s = side as f32;
            let flying = lo.airborne || rig.gait == Gait::Flyer || rig.gait == Gait::Static;
            if flying {
                if rig.gait == Gait::Hexapod {
                    let a = 0.45 * (lo.flap * TAU).sin();
                    return Some(Local::rot(rz(s * a)));
                }
                let p = lo.flap.rem_euclid(1.0);
                let (up, down) = (0.95, -0.55);
                let v = if p < 0.45 {
                    up + (down - up) * Ease::InOutSine.apply(p / 0.45)
                } else {
                    down + (up - down) * Ease::InOutSine.apply((p - 0.45) / 0.55)
                };
                let glide = 0.12;
                let a = glide + (v - glide) * lo.flap_amp;
                // Sweep forward on the downstroke.
                let sweep = 0.18 * (p * TAU).sin() * lo.flap_amp;
                Some(Local::rot(ry(s * sweep) * rz(s * a)))
            } else {
                // Folded along the back, with a little breathing twitch.
                let tw = 0.05 * (t * 1.7 + s).sin();
                Some(Local::rot(ry(-s * 1.3) * rz(s * (-0.35 + tw))))
            }
        }
        _ => None,
    }
}

// ----------------------------------------------------------------------------------------- idle

fn breath_period(rig: &Rig) -> f32 {
    (2.0 * (rig.height / 2.0).max(0.1).sqrt()).clamp(1.4, 5.0)
}

fn idle_local(m: &VoxelModel, i: usize, t: f32, lo: &Loco) -> Local {
    let rig = &m.rig;
    // `lo.dist` is 0 for `Idle`; a stopped `Locomotion` keeps its distance so wheels hold their phase.
    if let Some(o) = overlay(m, i, t, lo, lo.dist, false) {
        return o;
    }
    let h = rig.height;
    let bp = breath_period(rig);
    let br = (t * TAU / bp).sin();
    match rig.kinds[i] {
        PartKind::Root => match rig.gait {
            Gait::Static => Local::IDENTITY,
            Gait::Hopper => {
                let s = 1.0 + 0.05 * (t * 3.0).sin();
                Local { scale: Vec3::new(1.0 / s.sqrt(), s, 1.0 / s.sqrt()), ..Local::IDENTITY }
            }
            Gait::Flyer => Local { pos: Vec3::Y * (0.04 * h * (t * 2.0).sin()), ..Local::IDENTITY },
            Gait::Serpent | Gait::Fish => {
                let ph = lo.phase;
                let a = if rig.gait == Gait::Serpent { 0.1 } else { 0.06 };
                Local::rot(ry(-a * (ph * TAU).sin()))
            }
            _ => {
                let sy = 1.0 + 0.018 * br;
                Local { pos: Vec3::Y * (0.008 * h * br), scale: Vec3::new(1.0 - 0.008 * br, sy, 1.0 - 0.008 * br), ..Local::IDENTITY }
            }
        },
        PartKind::Head => {
            let yaw = 0.28 * (t * 0.55 + 1.0).sin() + 0.08 * (t * 1.7).sin();
            let pitch = 0.06 * (t * 0.9).sin() - 0.03 * br;
            if rig.gait == Gait::Hopper {
                let s = 1.0 + 0.05 * (t * 3.0 - 0.6).sin();
                return Local { rot: ry(0.1 * (t * 0.8).sin()), scale: Vec3::new(1.0, s, 1.0), ..Local::IDENTITY };
            }
            if rig.gait == Gait::Serpent {
                return Local::rot(ry(0.1 * (lo.phase * TAU).sin() + yaw * 0.5) * rx(0.12 + pitch));
            }
            Local::rot(ry(yaw) * rx(pitch))
        }
        PartKind::Jaw => Local::rot(rx(-0.05 * (0.5 + 0.5 * (t * 0.8).sin()))),
        PartKind::Arm(s) => Local::rot(rx(0.05 * (t * 1.1 + s as f32).sin()) * rz(s as f32 * 0.03 * br)),
        PartKind::Tail { k, .. } => {
            let k = k as f32;
            match rig.gait {
                Gait::Serpent | Gait::Fish => serpent_seg(rig, i, lo.phase, if rig.gait == Gait::Serpent { 0.12 } else { 0.18 }),
                _ => Local::rot(ry(0.12 * (t * 1.1 - k * 0.6).sin()) * rx(0.03 * (t * 0.9 - k * 0.5).sin())),
            }
        }
        _ => Local::IDENTITY,
    }
}

// ---------------------------------------------------------------------------------- locomotion

/// Travelling wave for serpent/fish chains. Local yaw = difference of tangent angles so the body
/// follows an S-curve that travels backward at the forward speed (no sliding).
fn serpent_seg(rig: &Rig, i: usize, phase: f32, amp: f32) -> Local {
    if let PartKind::Tail { chain, k } = rig.kinds[i] {
        let n = rig.tails[chain as usize].len().max(1) as f32;
        let fish = rig.gait == Gait::Fish;
        let kw = if fish { 0.18 } else { 1.0 / (n * 0.75) };
        let tangent = |kk: f32| {
            let grow = if fish { 0.4 + 0.6 * (kk + 1.0) / n } else { 1.0 };
            amp * grow * (TAU * (phase - kk * kw)).sin()
        };
        let k = k as f32;
        let prev = if k == 0.0 {
            if fish { -amp * 0.3 * (TAU * phase).sin() } else { tangent(-1.0) }
        } else {
            tangent(k - 1.0)
        };
        Local::rot(ry(tangent(k) - prev))
    } else {
        Local::IDENTITY
    }
}

fn loco_local(m: &VoxelModel, i: usize, t: f32, lo: &Loco) -> Local {
    let rig = &m.rig;
    if lo.speed < 0.05 && !lo.airborne {
        // Parked: identical to Idle (breathing, sway) but machines keep their wheel/tread phase.
        return idle_local(m, i, t, lo);
    }
    if let Some(o) = overlay(m, i, t, lo, lo.dist, true) {
        return o;
    }
    let w = lo.w;
    let h = rig.height;
    let p = lo.phase;
    match rig.gait {
        Gait::Serpent | Gait::Fish => {
            let amp = if rig.gait == Gait::Serpent { 0.14 + 0.32 * w } else { 0.2 + 0.25 * w };
            return match rig.kinds[i] {
                PartKind::Root => {
                    let a = if rig.gait == Gait::Serpent { amp * 0.35 } else { -amp * 0.3 };
                    Local::rot(ry(a * (TAU * p).sin()))
                }
                PartKind::Head => Local::rot(ry(-amp * 0.3 * (TAU * p).sin()) * rx(if rig.gait == Gait::Serpent { 0.1 } else { 0.0 })),
                PartKind::Tail { .. } => serpent_seg(rig, i, p, amp),
                PartKind::Jaw => Local::rot(rx(-0.05)),
                _ => Local::IDENTITY,
            };
        }
        Gait::Hopper => return hopper_local(rig, i, p, lo.speed),
        Gait::Flyer => {
            return match rig.kinds[i] {
                PartKind::Root => {
                    let bob = 0.035 * h * (TAU * (lo.flap - 0.1)).sin() * lo.flap_amp;
                    Local { rot: rx(-0.2 * smoothstep(0.0, 8.0, lo.speed)), pos: Vec3::Y * bob, ..Local::IDENTITY }
                }
                PartKind::Tail { .. } => Local::rot(rx(0.1 * (TAU * lo.flap).sin())),
                PartKind::Head => Local::rot(rx(0.15 * smoothstep(0.0, 8.0, lo.speed))),
                _ => Local::IDENTITY,
            };
        }
        Gait::Static => return Local::IDENTITY,
        _ => {}
    }
    // Legged gaits.
    if lo.airborne {
        return airborne_local(rig, i, lo);
    }
    let lean = (0.018 * lo.speed).min(0.2) * w;
    match rig.kinds[i] {
        PartKind::Root => {
            let mut rot = rx(-lean);
            let mut pos = Vec3::Y * -lo.root_drop;
            if rig.gait == Gait::Biped {
                rot = rot * rz(0.05 * w * (TAU * p).sin()) * ry(0.06 * w * (TAU * p).sin());
                // Running bounce: extra lift in the flight phase.
                let fl = (1.0 - lo.stance * 2.0).max(0.0);
                pos.y += fl * rig.leg_len * 0.25 * (TAU * 2.0 * p).cos().max(0.0) * w;
            }
            if rig.gait == Gait::Quadruped && lo.mode == 2 {
                rot = rot * rx(0.09 * w * (TAU * p).sin());
                pos.y += rig.leg_len * 0.1 * (TAU * p).sin().max(0.0) * w;
            }
            if rig.splayed {
                pos.y += 0.015 * h * (TAU * 2.0 * p).sin() * w;
                rot = rot * rz(0.02 * w * (TAU * 2.0 * p).sin());
            }
            Local { rot, pos, ..Local::IDENTITY }
        }
        PartKind::Leg(k) => {
            let leg = &rig.legs[k as usize];
            let (a, lift) = leg_cycle(p + leg_offset(rig, leg, lo.mode), lo.stance);
            let ang = a * lo.amp * w;
            if rig.splayed {
                Local::rot(ry(leg.side * ang) * rz(leg.side * lift * 0.45 * w))
            } else {
                Local { rot: rx(ang + lift * 0.2 * w), pos: Vec3::Y * (lift * 0.14 * leg.len * w), ..Local::IDENTITY }
            }
        }
        PartKind::Arm(s) => {
            // Counter-swing: opposite the leg on the same side.
            let same = rig.legs.iter().find(|l| l.side == s as f32);
            let ang = same.map_or(0.0, |l| {
                let (a, _) = leg_cycle(p + leg_offset(rig, l, lo.mode), lo.stance);
                a * lo.amp * w
            });
            let k = if h > 6.0 { 0.35 } else { 0.9 };
            Local::rot(rx(-ang * k - 0.25 * smoothstep(3.0, 8.0, lo.speed)))
        }
        PartKind::Head => {
            let nod = if rig.gait == Gait::Quadruped { -0.08 * w * (TAU * 2.0 * p).sin() } else { 0.0 };
            Local::rot(rx(lean * 0.7 + nod))
        }
        PartKind::Tail { k, .. } => {
            let k = k as f32;
            Local::rot(ry(0.2 * w * (TAU * p - k * 0.7).sin()) * rx(-0.12 * smoothstep(2.0, 8.0, lo.speed) / (1.0 + k)))
        }
        PartKind::Jaw => Local::rot(rx(-0.1 * smoothstep(4.0, 10.0, lo.speed))),
        _ => Local::IDENTITY,
    }
}

fn airborne_local(rig: &Rig, i: usize, lo: &Loco) -> Local {
    let flying = rig.has_wings;
    match rig.kinds[i] {
        PartKind::Leg(k) => {
            let leg = &rig.legs[k as usize];
            if flying {
                Local::rot(rx(-0.8))
            } else if rig.gait == Gait::Biped {
                Local::rot(rx(if leg.side < 0.0 { 0.6 } else { -0.35 }))
            } else if rig.splayed {
                Local::rot(rz(leg.side * 0.35))
            } else {
                let front = leg.rank == 0 && leg.ranks > 1;
                Local::rot(rx(if front { 0.7 } else { -0.6 }))
            }
        }
        PartKind::Arm(_) => Local::rot(rx(-0.9)),
        PartKind::Root => {
            if flying {
                let bob = 0.03 * rig.height * (TAU * (lo.flap - 0.1)).sin() * lo.flap_amp;
                Local { rot: rx(-0.15 * smoothstep(0.0, 8.0, lo.speed)), pos: Vec3::Y * bob, ..Local::IDENTITY }
            } else {
                Local::rot(rx(-0.08))
            }
        }
        PartKind::Tail { k, .. } => Local::rot(rx(0.1 + 0.05 * k as f32)),
        _ => Local::IDENTITY,
    }
}

fn hopper_local(rig: &Rig, i: usize, p: f32, speed: f32) -> Local {
    let h = rig.height;
    let u = p.rem_euclid(1.0);
    let w = smoothstep(0.05, 0.4, speed);
    // squash → launch/stretch → air → land squash
    let (sy, y) = if u < 0.18 {
        (1.0 - 0.28 * Ease::OutQuad.apply(u / 0.18), 0.0)
    } else if u < 0.82 {
        let a = (u - 0.18) / 0.64;
        let y = 4.0 * a * (1.0 - a) * 0.45 * h;
        let st = 1.0 + 0.18 * (1.0 - (2.0 * a - 1.0).abs().powf(0.5)) * 0.0 + 0.15 * (1.0 - 2.0 * a).abs();
        (st, y)
    } else {
        let a = (u - 0.82) / 0.18;
        (1.0 - 0.22 * (PI * a).sin(), 0.0)
    };
    let sy = 1.0 + (sy - 1.0) * w;
    match rig.kinds[i] {
        PartKind::Root => Local { pos: Vec3::Y * y * w, scale: Vec3::new(1.0 / sy.sqrt(), sy, 1.0 / sy.sqrt()), rot: rx(-0.1 * w), ..Local::IDENTITY },
        PartKind::Head => {
            // Lagging jiggle of the top lobe.
            let lag = (TAU * (u - 0.12)).sin();
            Local { pos: Vec3::Y * (-0.05 * h * lag * w), scale: Vec3::new(1.0, 1.0 + 0.1 * lag * w, 1.0), ..Local::IDENTITY }
        }
        _ => Local::IDENTITY,
    }
}

// --------------------------------------------------------------------------------------- attacks

fn first_arm(rig: &Rig) -> Option<i8> {
    if rig.kinds.iter().any(|k| *k == PartKind::Arm(1)) {
        Some(1)
    } else if rig.kinds.iter().any(|k| *k == PartKind::Arm(-1)) {
        Some(-1)
    } else {
        None
    }
}

fn attack_local(m: &VoxelModel, i: usize, atk: Attack, t: f32, tm: &AttackTiming, lo: &Loco) -> Local {
    let rig = &m.rig;
    let (t1, t2, t3) = (tm.anticipation, tm.anticipation + tm.active, tm.total());
    let h = rig.height;
    let kind = rig.kinds[i];
    use Ease::*;
    match atk {
        Attack::Bite => {
            let head_pitch = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.45, OutCubic), K(t2, -0.3, InQuad), K(t2 + 0.3 * (t3 - t2), -0.36, OutQuad), K(t3, 0.0, InOutSine)]);
            let head_z = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.05 * h, OutCubic), K(t2, -0.1 * h, InQuad), K(t3, 0.0, InOutSine)]);
            let jaw = track(t, &[K(0.0, 0.0, Linear), K(t1, -0.8, OutCubic), K(t1 + 0.6 * (t2 - t1), -0.85, Linear), K(t2, 0.06, InCubic), K(t2 + 0.25 * (t3 - t2), 0.0, OutQuad), K(t3, 0.0, Linear)]);
            let lean = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.07, OutCubic), K(t2, -0.12, InQuad), K(t3, 0.0, InOutSine)]);
            let root_z = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.03 * h, OutCubic), K(t2, -0.07 * h, InQuad), K(t3, 0.0, InOutSine)]);
            let sq = track(t, &[K(0.0, 1.0, Linear), K(t1, 0.94, OutCubic), K(t2, 1.05, InQuad), K(t3, 1.0, OutBack)]);
            match kind {
                PartKind::Root => {
                    let (hp, hz) = if rig.has_head { (0.0, 0.0) } else { (head_pitch, head_z) };
                    Local { rot: rx(lean - hp * 0.3), pos: Vec3::Z * (root_z + hz * 0.5), scale: Vec3::new(1.0 / sq.sqrt(), sq, 1.0 / sq.sqrt()) }
                }
                PartKind::Head => Local { rot: rx(head_pitch), pos: Vec3::Z * head_z, ..Local::IDENTITY },
                PartKind::Jaw => Local::rot(rx(jaw)),
                PartKind::Tail { k, .. } => Local::rot(rx(-lean * 1.5 / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
        Attack::Swipe => {
            let side = first_arm(rig);
            let s = side.unwrap_or(1) as f32;
            let arm_p = track(t, &[K(0.0, 0.0, Linear), K(t1, -2.4, OutCubic), K(t2, 0.9, InQuad), K(t2 + 0.3 * (t3 - t2), 1.05, OutQuad), K(t3, 0.0, InOutSine)]);
            let arm_r = track(t, &[K(0.0, 0.0, Linear), K(t1, s * 0.6, OutCubic), K(t2, -s * 0.5, InQuad), K(t3, 0.0, InOutSine)]);
            let twist = track(t, &[K(0.0, 0.0, Linear), K(t1, -s * 0.35, OutCubic), K(t2, s * 0.4, InQuad), K(t3, 0.0, InOutSine)]);
            let lean = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.06, OutCubic), K(t2, -0.1, InQuad), K(t3, 0.0, InOutSine)]);
            match kind {
                PartKind::Root => Local::rot(ry(twist) * rx(lean)),
                PartKind::Arm(a) if Some(a) == side => Local::rot(rx(arm_p) * rz(arm_r)),
                PartKind::Arm(_) => Local::rot(rx(-arm_p * 0.15)),
                PartKind::Leg(k) if side.is_none() => {
                    // Paw swipe with the front-right leg.
                    let l = &rig.legs[k as usize];
                    if l.rank == 0 && l.side > 0.0 {
                        let a = track(t, &[K(0.0, 0.0, Linear), K(t1, 1.5, OutCubic), K(t2, 0.1, InQuad), K(t3, 0.0, InOutSine)]);
                        Local { rot: rx(a), pos: Vec3::Y * (0.25 * l.len * (a / 1.5).max(0.0)), ..Local::IDENTITY }
                    } else {
                        Local::IDENTITY
                    }
                }
                PartKind::Head => Local::rot(ry(-twist * 0.5)),
                PartKind::Tail { k, .. } => Local::rot(ry(-twist * 0.8 / (1.0 + 0.5 * k as f32))),
                _ => Local::IDENTITY,
            }
        }
        Attack::Charge => {
            let crouch = track(t, &[K(0.0, 1.0, Linear), K(t1, 0.9, OutCubic), K(t1 + 0.05, 1.04, OutQuad), K(t2, 1.0, Linear), K(t3, 1.0, Linear)]);
            let lean = track(t, &[K(0.0, 0.0, Linear), K(t1 * 0.7, 0.07, OutCubic), K(t1, 0.05, Linear), K(t1 + 0.08, -0.16, OutQuad), K(t2, -0.16, Linear), K(t2 + 0.3 * (t3 - t2), 0.14, OutQuad), K(t3, 0.0, InOutSine)]);
            let head = track(t, &[K(0.0, 0.0, Linear), K(t1, -0.45, OutCubic), K(t2, -0.45, Linear), K(t3, 0.0, OutBack)]);
            let dashing = t >= t1 && t < t2;
            match kind {
                PartKind::Root => Local { rot: rx(lean), scale: Vec3::new(1.0, crouch, 1.0 / crouch.sqrt() * if dashing { 1.06 } else { 1.0 }), ..Local::IDENTITY },
                PartKind::Head => Local::rot(rx(head)),
                PartKind::Leg(k) => {
                    let l = &rig.legs[k as usize];
                    if dashing {
                        loco_leg(rig, l, lo)
                    } else if t < t1 && l.rank == 0 && l.side > 0.0 {
                        // Scrape the ground twice.
                        let a = 0.5 * (t / t1 * 2.0 * TAU).sin();
                        Local { rot: rx(a), pos: Vec3::Y * (0.08 * l.len * a.max(0.0)), ..Local::IDENTITY }
                    } else {
                        Local::IDENTITY
                    }
                }
                PartKind::Arm(_) => Local::rot(rx(if dashing { 0.9 } else { 0.0 })),
                PartKind::Tail { k, .. } => Local::rot(rx(-0.25 / (1.0 + k as f32)) * ry(if dashing { 0.15 * (t * 20.0 - k as f32).sin() } else { 0.0 })),
                _ => Local::IDENTITY,
            }
        }
        Attack::Stomp => stomp_local(rig, kind, t, t1, t2, t3, m.weight),
        Attack::TailSweep => {
            let yaw = track(t, &[K(0.0, 0.0, Linear), K(t1, -0.5, OutCubic), K(t2, TAU + 0.25, InOutQuad), K(t3, TAU, OutBack)]);
            // Angular speed proxy for trailing lag.
            let dt = 0.02;
            let yaw2 = track(t + dt, &[K(0.0, 0.0, Linear), K(t1, -0.5, OutCubic), K(t2, TAU + 0.25, InOutQuad), K(t3, TAU, OutBack)]);
            let omega = ((yaw2 - yaw) / dt).clamp(-25.0, 25.0);
            let crouch = track(t, &[K(0.0, 1.0, Linear), K(t1, 0.9, OutCubic), K(t2, 0.97, Linear), K(t3, 1.0, OutBack)]);
            match kind {
                PartKind::Root => Local { rot: ry(yaw) * rz(-0.012 * omega), scale: Vec3::new(1.0, crouch, 1.0), ..Local::IDENTITY },
                PartKind::Tail { k, .. } => {
                    let k = k as f32;
                    // Lag behind the spin, lift off the ground; curl during the wind-up.
                    Local::rot(ry(-0.022 * omega / (1.0 + 0.3 * k)) * rx(-0.1 * (omega.abs() / 20.0).min(1.0)))
                }
                PartKind::Head => Local::rot(ry(0.01 * omega)),
                PartKind::Arm(s) => Local::rot(rz(s as f32 * 0.02 * omega.abs())),
                _ => Local::IDENTITY,
            }
        }
        Attack::Breath { aim } => {
            let aim = aim.normalize_or(Vec3::NEG_Z);
            let yaw_t = (-aim.x).atan2(-aim.z).clamp(-1.3, 1.3);
            let pitch_t = aim.y.atan2((aim.x * aim.x + aim.z * aim.z).sqrt()).clamp(-0.7, 0.6);
            let track_w = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.0, Linear), K(t1 + 0.12, 1.0, OutBack), K(t2, 1.0, Linear), K(t3, 0.0, InOutSine)]);
            let rear = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.7, OutCubic), K(t1 + 0.1, -0.2, OutQuad), K(t2, -0.1, Linear), K(t3, 0.0, InOutSine)]);
            let jaw = track(t, &[K(0.0, 0.0, Linear), K(t1, -0.25, InQuad), K(t1 + 0.08, -0.95, OutBack), K(t2, -0.95, Linear), K(t2 + 0.3 * (t3 - t2), 0.05, InCubic), K(t3, 0.0, OutQuad)]);
            let puff = track(t, &[K(0.0, 1.0, Linear), K(t1, 1.05, OutCubic), K(t1 + 0.1, 0.98, OutQuad), K(t2, 1.0, Linear), K(t3, 1.0, Linear)]);
            let ramp = (t / t1.max(0.01)).clamp(0.0, 1.0);
            let charge_shake = if t < t1 { 0.03 * ramp * ramp * (t * 70.0).sin() } else { 0.0 };
            let sustain_shake = if t >= t1 && t < t2 { 0.025 * (t * 47.0).sin() } else { 0.0 };
            let has_head = rig.has_head;
            match kind {
                PartKind::Root => {
                    let yw = if has_head { yaw_t * 0.4 } else { yaw_t };
                    let pw = if has_head { 0.0 } else { pitch_t };
                    Local {
                        rot: ry(yw * track_w) * rx(rear * 0.3 + pw * track_w + charge_shake * 0.3),
                        pos: Vec3::Z * (0.02 * h * track_w),
                        scale: Vec3::new(puff, 1.0 + (puff - 1.0) * 0.6, puff),
                    }
                }
                PartKind::Head => Local::rot(ry(yaw_t * 0.6 * track_w) * rx(rear + (pitch_t * 0.8 - 0.05) * track_w + charge_shake + sustain_shake)),
                PartKind::Jaw => Local::rot(rx(jaw + sustain_shake)),
                PartKind::Arm(s) => Local::rot(rx(-0.4 * ramp.min(1.0) * (1.0 - track_w * 0.5)) * rz(s as f32 * 0.1)),
                PartKind::Tail { k, .. } => Local::rot(rx(0.08 * rear / (1.0 + k as f32)) * ry(0.05 * (t * 3.0 - k as f32).sin())),
                _ => Local::IDENTITY,
            }
        }
        Attack::Lash => match kind {
            PartKind::Chain { chain, k } | PartKind::Tail { chain, k } => {
                let n = if matches!(kind, PartKind::Chain { .. }) { rig.chains[chain as usize].len() } else { rig.tails[chain as usize].len() } as f32;
                let f = k as f32 / n.max(1.0);
                let wind = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.35, OutCubic), K(t2, 0.0, Linear), K(t3, 0.0, Linear)]);
                let u = ((t - t1) / (t2 - t1).max(0.01)).clamp(0.0, 1.6);
                let bump = (-((u * 1.3 - f) * 5.0).powi(2)).exp();
                let settle = if t > t2 { spring_impulse(t - t2, 3.0, 6.0) * 0.2 * (1.0 - f) } else { 0.0 };
                Local::rot(rx(wind * (0.5 + f) - 0.9 * bump + settle))
            }
            _ => Local::IDENTITY,
        },
    }
}

fn loco_leg(rig: &Rig, l: &LegRig, lo: &Loco) -> Local {
    let (a, lift) = leg_cycle(lo.phase + leg_offset(rig, l, lo.mode), lo.stance);
    let ang = a * lo.amp;
    if rig.splayed {
        Local::rot(ry(l.side * ang) * rz(l.side * lift * 0.45))
    } else {
        Local { rot: rx(ang + lift * 0.2), pos: Vec3::Y * (lift * 0.14 * l.len), ..Local::IDENTITY }
    }
}

fn stomp_local(rig: &Rig, kind: PartKind, t: f32, t1: f32, t2: f32, t3: f32, weight: f32) -> Local {
    use Ease::*;
    let h = rig.height;
    let rec = t3 - t2;
    let wk = weight.clamp(0.5, 2.5);
    // Body: rise & lean back on the anticipation, slam + squash on impact, springy recovery.
    let sq = track(t, &[K(0.0, 1.0, Linear), K(t1 * 0.35, 0.94, OutQuad), K(t1, 1.05, OutCubic), K(t2, 0.86, InCubic), K(t2 + 0.25 * rec, 1.04, OutQuad), K(t2 + 0.6 * rec, 0.99, InOutSine), K(t3, 1.0, InOutSine)]);
    let lift = track(t, &[K(0.0, 0.0, Linear), K(t1 * 0.35, -0.01, OutQuad), K(t1, 0.05, OutCubic), K(t2, -0.04, InCubic), K(t2 + 0.3 * rec, 0.01, OutQuad), K(t3, 0.0, InOutSine)]);
    let lean = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.12, OutCubic), K(t2, -0.1, InCubic), K(t2 + 0.3 * rec, 0.03, OutQuad), K(t3, 0.0, InOutSine)]);
    let shift = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.07, OutCubic), K(t2, 0.0, InCubic), K(t3, 0.0, Linear)]);
    // Heavier: deeper squash on impact, lower & slower ring-out.
    let impact = if t >= t2 { spring_impulse(t - t2, 6.0 / wk.sqrt(), 9.0 / wk) * wk.sqrt() } else { 0.0 };
    let sq = 1.0 + (sq - 1.0) * wk.sqrt();
    match rig.gait {
        Gait::Quadruped => {
            // Rear up on the hind legs, slam the front down.
            let rear = track(t, &[K(0.0, 0.0, Linear), K(t1 * 0.3, -0.05, OutQuad), K(t1, 0.5, OutCubic), K(t2, -0.04, InCubic), K(t2 + 0.3 * rec, 0.02, OutQuad), K(t3, 0.0, InOutSine)]);
            let back_z = rig.legs.iter().filter(|l| l.rank + 1 == l.ranks).map(|_| rig.length * 0.33).next().unwrap_or(0.0);
            match kind {
                PartKind::Root => {
                    let pivot = Vec3::new(0.0, 0.0, back_z);
                    let q = rx(rear);
                    let pos = pivot - q * pivot;
                    Local { rot: q, pos, scale: Vec3::new(1.0 / sq.sqrt(), sq, 1.0 / sq.sqrt()) }
                }
                PartKind::Leg(k) => {
                    let l = &rig.legs[k as usize];
                    if l.rank == 0 && l.ranks > 1 {
                        Local::rot(rx(rear * 1.2))
                    } else {
                        Local::rot(rx(-rear))
                    }
                }
                PartKind::Head => Local::rot(rx(-rear * 0.6 - 0.3 * impact)),
                PartKind::Tail { k, .. } => Local::rot(rx(rear * 0.5 / (1.0 + k as f32) + 0.2 * impact)),
                _ => Local::IDENTITY,
            }
        }
        Gait::Biped | Gait::Hexapod | Gait::Octopod => {
            let right_leg = rig.legs.iter().position(|l| l.side > 0.0);
            let leg_up = track(t, &[K(0.0, 0.0, Linear), K(t1 * 0.55, 0.95, OutQuad), K(t1, 1.15, OutSine2), K(t2, -0.06, InCubic), K(t2 + 0.25 * rec, 0.02, OutQuad), K(t3, 0.0, InOutSine)]);
            let leg_lift = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.35, OutCubic), K(t2, 0.0, InCubic), K(t3, 0.0, Linear)]);
            match kind {
                PartKind::Root => Local {
                    rot: rx(lean) * rz(shift),
                    pos: Vec3::Y * (lift * h),
                    scale: Vec3::new(1.0 / sq.sqrt(), sq, 1.0 / sq.sqrt()),
                },
                PartKind::Leg(k) => {
                    let l = &rig.legs[k as usize];
                    if Some(k as usize) == right_leg {
                        Local { rot: rx(leg_up), pos: Vec3::Y * (leg_lift * l.len), ..Local::IDENTITY }
                    } else {
                        Local::rot(rx(-lean * 0.5))
                    }
                }
                PartKind::Arm(s) => {
                    let up = track(t, &[K(0.0, 0.0, Linear), K(t1, -1.1, OutCubic), K(t2, 0.35, InCubic), K(t3, 0.0, InOutSine)]);
                    Local::rot(rx(up) * rz(s as f32 * 0.3 * (up.abs())))
                }
                PartKind::Head => Local::rot(rx(-lean * 0.8 - 0.35 * impact)),
                PartKind::Jaw => Local::rot(rx(-0.3 * impact.abs() - 0.2 * (lean.max(0.0) / 0.12))),
                PartKind::Tail { k, .. } => {
                    let k = k as f32;
                    Local::rot(rx(-lean * 2.0 / (1.0 + 0.5 * k) + 0.25 * impact / (1.0 + 0.3 * k)))
                }
                _ => Local::IDENTITY,
            }
        }
        _ => {
            // Hop-slam for anything without a leg rig.
            let y = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.35 * h, OutCubic), K(t2, 0.0, InCubic), K(t3, 0.0, Linear)]);
            match kind {
                PartKind::Root => Local { pos: Vec3::Y * y, scale: Vec3::new(1.0 / sq.sqrt(), sq, 1.0 / sq.sqrt()), ..Local::IDENTITY },
                _ => Local::IDENTITY,
            }
        }
    }
}

#[allow(non_upper_case_globals)]
const OutSine2: Ease = Ease::OutQuad;

// ------------------------------------------------------------------------------------ hit react

/// Exaggerated hit reaction: the body is shoved along the hit (hips first) so the spine reads as a
/// C-curve — shoulders and legs lag behind, the head whips back (chin snap), arms fling and trail —
/// and at high strength the feet leave the ground; then a snappy spring recovery.
fn hit_local(m: &VoxelModel, i: usize, dir: Vec3, strength: f32, t: f32) -> Local {
    let rig = &m.rig;
    let s = strength.clamp(0.0, 1.5);
    let d = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::Z);
    let axis = Vec3::Y.cross(d).normalize_or(Vec3::X);
    // Fast attack (45 ms), then a quick damped ring-out.
    let env = |t: f32| -> f32 {
        if t < 0.0 {
            0.0
        } else if t < 0.045 {
            (t / 0.045 * PI * 0.5).sin()
        } else {
            let u = t - 0.045;
            (-u * 9.5).exp() * (u * 15.0).cos()
        }
    };
    let e = env(t) * s;
    let lag = env(t - 0.035) * s;
    let lag2 = env(t - 0.06) * s;
    let pulse = (-t * 14.0).exp() * s;
    let h = rig.height;
    let pivot = m.parts[i].pivot;
    // Feet leave the ground on big hits.
    let air = (s - 0.55).max(0.0) * (PI * (t / 0.32).clamp(0.0, 1.0)).sin();
    match rig.kinds[i] {
        PartKind::Root => {
            let shove = d * (0.1 * h * s * (1.0 - (-t * 30.0).exp()) * (-t * 4.5).exp());
            // Upper body lags (top toward −d) about the hips.
            let hip = Vec3::Y * rig.leg_len.max(h * 0.3);
            let q = Quat::from_axis_angle(axis, -0.42 * e);
            let mut l = about(q, hip, pivot);
            l.pos += shove + Vec3::Y * (0.22 * h * air);
            let sq = 1.0 - 0.14 * pulse;
            l.scale = Vec3::new(1.0 + 0.07 * pulse, sq, 1.0 + 0.07 * pulse);
            l
        }
        // Chin snaps back: the head whips along the hit after the torso.
        PartKind::Head => Local::rot(Quat::from_axis_angle(axis, 0.75 * lag2) * rx(-0.25 * pulse)),
        PartKind::Jaw => Local::rot(rx(-0.45 * pulse)),
        // Arms fling back and out, trailing.
        PartKind::Arm(sd) => Local::rot(Quat::from_axis_angle(axis, 1.1 * lag) * rz(sd as f32 * 0.7 * lag.abs())),
        // Legs trail the shoved hips (feet swing back toward the attacker), more when airborne.
        PartKind::Leg(k) => {
            let side = rig.legs.get(k as usize).map_or(1.0, |l| l.side);
            Local::rot(Quat::from_axis_angle(axis, 0.5 * lag + 0.35 * air) * rz(side * 0.12 * air))
        }
        PartKind::Wing(_) => Local::rot(Quat::from_axis_angle(axis, 0.8 * lag)),
        PartKind::Tail { k, .. } | PartKind::Chain { k, .. } => Local::rot(Quat::from_axis_angle(axis, -0.45 * env(t - 0.03 * (k as f32 + 1.0)) * s)),
        _ => Local::IDENTITY,
    }
}

// ------------------------------------------------------------------------------- forming/dismiss

fn forming_local(m: &VoxelModel, i: usize, t: f32, duration: f32, dismiss: bool) -> Local {
    let rig = &m.rig;
    let dur = duration.max(0.05);
    let depth = m.parts[i].depth as f32;
    let md = rig.max_depth.max(1) as f32;
    let jitter = ((i as f32 * 0.618_034).fract()) * 0.06;
    if !dismiss {
        let start = dur * (0.4 * depth / md + jitter);
        let len = dur * 0.55;
        let u = ((t - start) / len).clamp(0.0, 1.0);
        let s = Ease::OutBack.apply(u).max(0.001);
        let twist = if i == 0 { (1.0 - Ease::OutCubic.apply(u)) * 0.9 } else { 0.0 };
        Local { rot: ry(twist), scale: Vec3::splat(s), ..Local::IDENTITY }
    } else {
        let start = dur * (0.35 * (1.0 - depth / md) + jitter);
        let len = dur * 0.6;
        let u = ((t - start) / len).clamp(0.0, 1.0);
        let s = if u < 0.35 { 1.0 + 0.1 * (PI * u / 0.35).sin() } else { (1.0 - (u - 0.35) / 0.65).powi(2) };
        let rise = if i == 0 { Vec3::Y * (0.05 * rig.height * u) } else { Vec3::ZERO };
        Local { pos: rise, scale: Vec3::splat(s.max(0.001)), ..Local::IDENTITY }
    }
}

// ------------------------------------------------------------------------------------ aim layer

fn aim_layer(m: &VoxelModel, i: usize, yaw: f32, pitch: f32) -> Local {
    if let Some(l) = crate::features::aim_local(m, i, yaw, pitch) {
        return l;
    }
    match m.rig.kinds[i] {
        PartKind::Head => Local::rot(ry(yaw.clamp(-1.3, 1.3) * 0.75) * rx(pitch.clamp(-0.9, 0.9) * 0.6)),
        _ => Local::IDENTITY,
    }
}

// ---------------------------------------------------------------------------- humanoid (players)

/// Rotation `q` about point `c` expressed as a local about the part pivot `p`.
pub(crate) fn about(q: Quat, c: Vec3, p: Vec3) -> Local {
    // Want x' = c + q (x - c); a local maps x' = p + pos + q (x - p)  =>  pos = c - p + q (p - c).
    Local { rot: q, pos: c - p + q * (p - c), scale: Vec3::ONE }
}

fn humanoid_local(m: &VoxelModel, i: usize, a: AnimState, t: f32) -> Local {
    let rig = &m.rig;
    let speed = match a {
        AnimState::Slide { speed } => speed,
        AnimState::Swing { .. } | AnimState::Tumble { .. } | AnimState::Launched { .. } => 9.0,
        _ => 0.0,
    };
    let lo = Loco { speed, ..Default::default() };
    if let Some(o) = overlay(m, i, t, &lo, 0.0, true) {
        return o;
    }
    let h = rig.height;
    let ll = rig.leg_len.max(0.1);
    let pivot = m.parts[i].pivot;
    let kind = rig.kinds[i];
    let leg_side = |k: u8| rig.legs.get(k as usize).map_or(1.0, |l| l.side);
    use Ease::*;
    match a {
        AnimState::Slide { speed } => {
            let s = smoothstep(1.0, 8.0, speed);
            let shake = 0.015 * s * (t * 43.0).sin();
            let recline = 0.55 + shake;
            let hip = Vec3::Y * ll;
            match kind {
                PartKind::Root => {
                    let mut l = about(rx(recline), hip, pivot);
                    l.pos.y -= 0.66 * ll;
                    l
                }
                PartKind::Leg(k) => {
                    let side = leg_side(k);
                    let lead = side > 0.0;
                    let world = if lead { -1.38 } else { -1.02 };
                    Local::rot(rx(world - recline + 0.04 * (t * 31.0 + side).sin() * s) * rz(side * if lead { 0.05 } else { 0.22 }))
                }
                PartKind::Arm(sd) => {
                    if sd < 0 {
                        Local::rot(rx(1.0 - recline) * rz(-0.45))
                    } else {
                        Local::rot(rx(-0.55 - recline) * rz(0.55))
                    }
                }
                PartKind::Head => Local::rot(rx(-recline * 0.8)),
                PartKind::Tail { k, .. } => Local::rot(rx(-0.4 / (1.0 + k as f32)) * ry(0.1 * (t * 9.0).sin())),
                _ => Local::IDENTITY,
            }
        }
        AnimState::TetherFire { side } => {
            let sd = if side < 0 { -1.0 } else { 1.0 };
            let e = track(t, &[K(0.0, 0.0, Linear), K(0.05, 1.0, OutBack), K(0.25, 0.94, InOutSine), K(0.6, 0.9, InOutSine)]);
            let kick = spring_impulse(t - 0.05, 7.0, 11.0) * 0.18;
            match kind {
                PartKind::Arm(a) if a as f32 == sd => Local::rot(rx(-1.8 * e + kick) * rz(-sd * 0.12 * e)),
                PartKind::Arm(_) => Local::rot(rx(0.35 * e) * rz(-sd * 0.2 * e)),
                PartKind::Root => Local::rot(ry(sd * 0.24 * e) * rx(-0.06 * e + kick * 0.2)),
                PartKind::Head => Local::rot(ry(-sd * 0.18 * e)),
                PartKind::Leg(k) => {
                    let side = leg_side(k);
                    Local::rot(rx(if side == sd { -0.15 } else { 0.12 } * e))
                }
                _ => Local::IDENTITY,
            }
        }
        AnimState::Swing { side, lean } => {
            let sway = 0.05 * (t * 2.1).sin();
            let grip = Vec3::Y * (h * 0.8);
            let body = lean * 0.5 + sway;
            match kind {
                PartKind::Root => about(rx(body), grip, pivot),
                PartKind::Arm(a) => {
                    let af = a as f32;
                    if side == 0 || a == side.signum() {
                        Local::rot(rx(-2.85 - body) * rz(-af * 0.18))
                    } else {
                        Local::rot(rx(-0.6 - body * 0.5) * rz(af * (0.7 + 0.1 * (t * 3.0).sin())))
                    }
                }
                PartKind::Leg(k) => {
                    let side = leg_side(k);
                    let kickv = 0.16 * (t * 3.2 + side * 1.4).sin();
                    Local::rot(rx(0.28 - lean * 0.45 - body * 0.3 + kickv) * rz(side * 0.08))
                }
                PartKind::Head => Local::rot(rx(-body * 0.6 - 0.12)),
                PartKind::Tail { k, .. } => Local::rot(rx(0.2 + lean * 0.3 / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
        AnimState::Cast => {
            let tm = CAST_TIMING;
            let (t1, t2, t3) = (tm.anticipation, tm.anticipation + tm.active, tm.total());
            let hold = t2 + 0.1;
            let arm = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.75, OutCubic), K(t2, -1.62, InQuad), K(hold, -1.52, OutQuad), K(t3, 0.0, InOutSine)]);
            let twist = track(t, &[K(0.0, 0.0, Linear), K(t1, -0.28, OutCubic), K(t2, 0.3, InQuad), K(hold, 0.26, Linear), K(t3, 0.0, InOutSine)]);
            let lean = track(t, &[K(0.0, 0.0, Linear), K(t1, 0.06, OutCubic), K(t2, -0.1, InQuad), K(t3, 0.0, InOutSine)]);
            let punch = if t > t2 { spring_impulse(t - t2, 6.0, 10.0) * 0.08 } else { 0.0 };
            let ext = (-arm).max(0.0) / 1.6;
            match kind {
                PartKind::Arm(1) => Local::rot(rx(arm + punch) * rz(0.1 * ext)),
                PartKind::Arm(_) => Local::rot(rx(-arm * 0.3) * rz(-0.15 * ext)),
                PartKind::Root => Local::rot(ry(twist) * rx(lean)),
                PartKind::Head => Local::rot(ry(-twist * 0.75) * rx(-lean * 0.5)),
                PartKind::Leg(k) => {
                    let side = leg_side(k);
                    Local::rot(rx(side * twist * 0.3))
                }
                _ => Local::IDENTITY,
            }
        }
        AnimState::Tumble { spin } => {
            let ang = (t * spin).rem_euclid(TAU);
            let axis = Vec3::new(1.0, 0.0, 0.3).normalize();
            let com = Vec3::Y * (h * 0.5);
            match kind {
                PartKind::Root => about(Quat::from_axis_angle(axis, ang), com, pivot),
                PartKind::Arm(sd) => {
                    let sd = sd as f32;
                    Local::rot(rx(-1.3 + 0.7 * (t * 9.0 + sd).sin()) * rz(sd * (0.9 + 0.35 * (t * 7.0).sin())))
                }
                PartKind::Leg(k) => {
                    let side = leg_side(k);
                    Local::rot(rx(-0.7 + 0.5 * (t * 8.0 + side * 1.7).sin()) * rz(side * 0.3))
                }
                PartKind::Head => Local::rot(rx(0.2 * (t * 6.0).sin()) * ry(0.15 * (t * 4.0).sin())),
                PartKind::Tail { k, .. } => Local::rot(ry(0.5 * (t * 10.0 - k as f32).sin())),
                _ => Local::IDENTITY,
            }
        }
        AnimState::Launched { dir, spin } => {
            // End-over-end about the axis ⟂ travel; limbs stream behind (toward −dir in body space).
            let d = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::Z);
            let axis = Vec3::Y.cross(d).normalize_or(Vec3::X);
            let ang = (t * spin).rem_euclid(TAU);
            let com = Vec3::Y * (h * 0.5);
            let trail = 0.9 + 0.12 * (t * 11.0).sin();
            match kind {
                PartKind::Root => about(Quat::from_axis_angle(axis, ang), com, pivot),
                PartKind::Arm(sd) => {
                    let sd = sd as f32;
                    Local::rot(Quat::from_axis_angle(axis, -trail * 1.6) * rz(sd * (0.5 + 0.2 * (t * 9.0 + sd).sin())))
                }
                PartKind::Leg(k) => {
                    let side = leg_side(k);
                    Local::rot(Quat::from_axis_angle(axis, -trail * 0.7 + 0.15 * (t * 8.0 + side).sin()) * rz(side * 0.25))
                }
                PartKind::Head => Local::rot(Quat::from_axis_angle(axis, -0.35)),
                PartKind::Tail { k, .. } => Local::rot(Quat::from_axis_angle(axis, -0.4 / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
        _ => Local::IDENTITY,
    }
}

// ----------------------------------------------------------------------------------- footfalls

/// A foot planting (legged creatures, costumes, walking constructs): drive camera shake / ground
/// cracks / dust from `impact` (≈ 0.2 for a person, ≈ 1 for a horse, 5+ for a kaiju).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footfall {
    /// Leg part index (pose with `pose[part]` for the posed foot; `pos` is the rest-pose foot).
    pub part: usize,
    /// Rest-pose foot position (model space, m; y = ground).
    pub pos: Vec3,
    /// Impact scalar: size^0.8 × weight × speed factor.
    pub impact: f32,
}

/// Feet that planted between locomotion states `(prev_distance → now_distance)` at `speed` (at most
/// one per leg per call). Same gait phase as `pose`, so the event lines up with the visible step.
/// Allocation-free apart from pushing into `out`.
pub fn footfalls(model: &VoxelModel, speed: f32, prev_distance: f32, now_distance: f32, out: &mut Vec<Footfall>) {
    let rig = &model.rig;
    if rig.legs.is_empty() || !matches!(rig.gait, Gait::Biped | Gait::Quadruped | Gait::Hexapod | Gait::Octopod) {
        return;
    }
    let sp = fin(speed, 0.0).clamp(0.0, 500.0);
    let (d0, d1) = (fin(prev_distance, 0.0).clamp(-1e6, 1e6), fin(now_distance, 0.0).clamp(-1e6, 1e6));
    if sp < 0.05 || d1 <= d0 {
        return;
    }
    let a = Loco::new(rig, sp, d0, false, 0.0);
    let b = Loco::new(rig, sp, d1, false, 0.0);
    let size = (rig.height / 2.0).clamp(0.1, 40.0).powf(0.8);
    let impact = size * model.weight.clamp(0.5, 2.5) * (0.35 + 0.65 * smoothstep(0.5, 8.0, sp));
    for leg in &rig.legs {
        let off = leg_offset(rig, leg, b.mode);
        // Stance begins at cycle phase 0 (leg_cycle): count crossings of whole cycles.
        let n0 = (a.phase + off).floor();
        let n1 = (b.phase + off).floor();
        if n1 > n0 {
            let p = model.parts[leg.part].pivot;
            out.push(Footfall { part: leg.part, pos: Vec3::new(p.x, model.bounds.min.y, p.z), impact });
        }
    }
}
